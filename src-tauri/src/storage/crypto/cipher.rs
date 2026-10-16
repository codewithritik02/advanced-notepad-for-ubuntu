use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;

use crate::storage::crypto::metadata::{AEAD_TAG_LEN_BYTES, DEK_LEN_BYTES, NONCE_LEN_BYTES};
use crate::storage::crypto::secret::SecretBytes;
use crate::storage::errors::CryptoError;

pub const ENVELOPE_VERSION_V1: u8 = 0x01;
pub const MIN_ENVELOPE_LEN: usize = 1 + NONCE_LEN_BYTES + AEAD_TAG_LEN_BYTES; // 29 bytes

/// Encrypts raw byte payload with AES-256-GCM using a unique 12-byte random nonce and Associated Data (Task 4, 9).
/// Output envelope structure: `[Version (1B)] + [Nonce (12B)] + [Ciphertext (NB)] + [Auth Tag (16B)]`.
pub fn encrypt_payload(
    plaintext: &[u8],
    dek: &SecretBytes,
    associated_data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if dek.len() != DEK_LEN_BYTES {
        return Err(CryptoError::KeyManagement(format!(
            "Invalid DEK length for encryption: {} bytes",
            dek.len()
        )));
    }

    let mut nonce_bytes = [0u8; NONCE_LEN_BYTES];
    OsRng.fill_bytes(&mut nonce_bytes);

    let cipher_key = Key::<Aes256Gcm>::from_slice(dek.as_slice());
    let cipher = Aes256Gcm::new(cipher_key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let payload = Payload {
        msg: plaintext,
        aad: associated_data,
    };

    let ciphertext_with_tag = cipher
        .encrypt(nonce, payload)
        .map_err(|e| CryptoError::EncryptionFailed(format!("AES-256-GCM encryption failed: {e}")))?;

    let mut envelope = Vec::with_capacity(1 + NONCE_LEN_BYTES + ciphertext_with_tag.len());
    envelope.push(ENVELOPE_VERSION_V1);
    envelope.extend_from_slice(&nonce_bytes);
    envelope.extend_from_slice(&ciphertext_with_tag);

    Ok(envelope)
}

/// Decrypts an authenticated envelope with AES-256-GCM and verifies the 16-byte authentication tag (Task 4, 9).
/// Strictly detects bit flips, payload truncation, or tampering before returning plaintext.
pub fn decrypt_payload(
    envelope: &[u8],
    dek: &SecretBytes,
    associated_data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if envelope.len() < MIN_ENVELOPE_LEN {
        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::DataIntegrityFailure,
            crate::storage::crypto::SecurityLogLevel::Error,
            "Ciphertext envelope is truncated",
            None,
        );
        return Err(CryptoError::TamperedOrCorrupted(format!(
            "Ciphertext envelope is truncated (length {} bytes, minimum {})",
            envelope.len(),
            MIN_ENVELOPE_LEN
        )));
    }

    let version = envelope[0];
    if version != ENVELOPE_VERSION_V1 {
        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::UnsupportedVersionEncountered,
            crate::storage::crypto::SecurityLogLevel::Error,
            "Unsupported ciphertext version encountered",
            Some(&format!("version={version}")),
        );
        return Err(CryptoError::UnsupportedFormatVersion {
            version: version as u32,
            max_supported: ENVELOPE_VERSION_V1 as u32,
        });
    }

    if dek.len() != DEK_LEN_BYTES {
        return Err(CryptoError::KeyManagement(format!(
            "Invalid DEK length for decryption: {} bytes",
            dek.len()
        )));
    }

    let nonce_bytes = &envelope[1..1 + NONCE_LEN_BYTES];
    let ciphertext_with_tag = &envelope[1 + NONCE_LEN_BYTES..];

    let cipher_key = Key::<Aes256Gcm>::from_slice(dek.as_slice());
    let cipher = Aes256Gcm::new(cipher_key);
    let nonce = Nonce::from_slice(nonce_bytes);

    let payload = Payload {
        msg: ciphertext_with_tag,
        aad: associated_data,
    };

    cipher.decrypt(nonce, payload).map_err(|_| {
        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::DataIntegrityFailure,
            crate::storage::crypto::SecurityLogLevel::Critical,
            "Authentication tag verification failed: payload has been modified or incorrect key was used",
            None,
        );
        CryptoError::TamperedOrCorrupted(
            "Authentication tag verification failed: payload has been modified or incorrect key was used".to_string(),
        )
    })
}

pub const ENCRYPTED_PREFIX_V1: &str = "ENC$v1$";

/// Returns true if the string begins with the canonical encrypted prefix (Task 9).
pub fn is_encrypted_string(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX_V1)
}

/// Encrypts a string into a versioned, prefixed, base64-encoded authenticated ciphertext envelope.
pub fn encrypt_string(
    plaintext: &str,
    dek: &SecretBytes,
    associated_data: &[u8],
) -> Result<String, CryptoError> {
    let envelope = encrypt_payload(plaintext.as_bytes(), dek, associated_data)?;
    Ok(format!("{}{}", ENCRYPTED_PREFIX_V1, BASE64_STANDARD.encode(envelope)))
}

/// Decrypts a base64-encoded authenticated ciphertext envelope (with or without prefix) into a validated UTF-8 string.
pub fn decrypt_string(
    ciphertext: &str,
    dek: &SecretBytes,
    associated_data: &[u8],
) -> Result<String, CryptoError> {
    let trimmed = ciphertext.trim();
    let raw_b64 = if let Some(stripped) = trimmed.strip_prefix(ENCRYPTED_PREFIX_V1) {
        stripped
    } else {
        trimmed
    };
    let envelope = BASE64_STANDARD.decode(raw_b64).map_err(|e| {
        CryptoError::TamperedOrCorrupted(format!("Invalid base64 ciphertext: {e}"))
    })?;
    let decrypted_bytes = decrypt_payload(&envelope, dek, associated_data)?;
    String::from_utf8(decrypted_bytes).map_err(|e| {
        CryptoError::TamperedOrCorrupted(format!("Decrypted plaintext is not valid UTF-8: {e}"))
    })
}

/// Encrypts an attachment file on disk directly into a staging file using atomic file operations (Task 10).
pub fn encrypt_file_to_file(
    src_path: &Path,
    dst_path: &Path,
    dek: &SecretBytes,
    associated_data: &[u8],
) -> Result<(), CryptoError> {
    let mut file = File::open(src_path).map_err(|e| {
        CryptoError::EncryptionFailed(format!("Failed to open source file for encryption: {e}"))
    })?;

    let meta = file.metadata().map_err(|e| {
        CryptoError::EncryptionFailed(format!("Failed to read source file metadata: {e}"))
    })?;

    let file_len = meta.len();
    if file_len > crate::storage::limits::MAX_ATTACHMENT_SIZE_BYTES {
        return Err(CryptoError::EncryptionFailed(format!(
            "File size ({} bytes) exceeds maximum permitted attachment size ({} bytes)",
            file_len,
            crate::storage::limits::MAX_ATTACHMENT_SIZE_BYTES
        )));
    }

    let mut plaintext = Vec::with_capacity(file_len as usize);
    file.read_to_end(&mut plaintext).map_err(|e| {
        CryptoError::EncryptionFailed(format!("Failed to read source file: {e}"))
    })?;

    let envelope = encrypt_payload(&plaintext, dek, associated_data)?;

    let write_res = (|| -> Result<(), CryptoError> {
        let mut out_file = BufWriter::new(File::create(dst_path).map_err(|e| {
            if e.raw_os_error() == Some(28) {
                CryptoError::EncryptionFailed("Insufficient disk space while creating encrypted file".to_string())
            } else {
                CryptoError::EncryptionFailed(format!("Failed to create destination file for encryption: {e}"))
            }
        })?);

        for chunk in envelope.chunks(64 * 1024) {
            out_file.write_all(chunk).map_err(|e| {
                if e.raw_os_error() == Some(28) {
                    CryptoError::EncryptionFailed("Insufficient disk space while writing encrypted payload to disk".to_string())
                } else {
                    CryptoError::EncryptionFailed(format!("Failed to write encrypted payload to disk: {e}"))
                }
            })?;
        }

        out_file.flush().map_err(|e| {
            if e.raw_os_error() == Some(28) {
                CryptoError::EncryptionFailed("Insufficient disk space while flushing encrypted payload to disk".to_string())
            } else {
                CryptoError::EncryptionFailed(format!("Failed to flush encrypted payload to disk: {e}"))
            }
        })?;

        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = fs::remove_file(dst_path);
        return Err(err);
    }

    Ok(())
}

/// Decrypts an attachment file on disk directly into a staging destination (Task 10).
pub fn decrypt_file_to_file(
    src_path: &Path,
    dst_path: &Path,
    dek: &SecretBytes,
    associated_data: &[u8],
) -> Result<(), CryptoError> {
    let mut file = BufReader::new(File::open(src_path).map_err(|e| {
        CryptoError::DecryptionFailed(format!("Failed to open encrypted file for decryption: {e}"))
    })?);

    let meta = file.get_ref().metadata().map_err(|e| {
        CryptoError::DecryptionFailed(format!("Failed to read encrypted file metadata: {e}"))
    })?;

    let file_len = meta.len();
    if file_len > crate::storage::limits::MAX_ATTACHMENT_SIZE_BYTES + MIN_ENVELOPE_LEN as u64 {
        return Err(CryptoError::DecryptionFailed(format!(
            "Encrypted file size ({file_len} bytes) exceeds maximum permitted attachment size"
        )));
    }

    let mut envelope = Vec::with_capacity(file_len as usize);
    file.read_to_end(&mut envelope).map_err(|e| {
        CryptoError::DecryptionFailed(format!("Failed to read encrypted file: {e}"))
    })?;

    let plaintext = decrypt_payload(&envelope, dek, associated_data)?;

    let write_res = (|| -> Result<(), CryptoError> {
        let mut out_file = BufWriter::new(File::create(dst_path).map_err(|e| {
            if e.raw_os_error() == Some(28) {
                CryptoError::DecryptionFailed("Insufficient disk space while creating decrypted file".to_string())
            } else {
                CryptoError::DecryptionFailed(format!("Failed to create destination file for decryption: {e}"))
            }
        })?);

        for chunk in plaintext.chunks(64 * 1024) {
            out_file.write_all(chunk).map_err(|e| {
                if e.raw_os_error() == Some(28) {
                    CryptoError::DecryptionFailed("Insufficient disk space while writing decrypted payload".to_string())
                } else {
                    CryptoError::DecryptionFailed(format!("Failed to write decrypted payload: {e}"))
                }
            })?;
        }

        out_file.flush().map_err(|e| {
            if e.raw_os_error() == Some(28) {
                CryptoError::DecryptionFailed("Insufficient disk space while flushing decrypted payload".to_string())
            } else {
                CryptoError::DecryptionFailed(format!("Failed to flush decrypted payload: {e}"))
            }
        })?;

        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = fs::remove_file(dst_path);
        return Err(err);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::crypto::kdf::generate_dek;

    #[test]
    fn test_payload_encryption_and_decryption_roundtrip() {
        let dek = generate_dek();
        let message = b"Confidential Personal Notepad Data 12345! \xf0\x9f\x94\x92";
        let ad = b"note:12345:body";

        let envelope = encrypt_payload(message, &dek, ad).expect("Encryption should succeed");
        assert!(envelope.len() > message.len());
        assert_eq!(envelope[0], ENVELOPE_VERSION_V1);

        // Decrypt with correct key & AD
        let decrypted = decrypt_payload(&envelope, &dek, ad).expect("Decryption should succeed");
        assert_eq!(message, decrypted.as_slice());

        // Decrypt with mismatching Associated Data -> MUST fail
        let wrong_ad_res = decrypt_payload(&envelope, &dek, b"note:99999:body");
        assert!(wrong_ad_res.is_err());

        // Decrypt with wrong key -> MUST fail
        let wrong_dek = generate_dek();
        let wrong_key_res = decrypt_payload(&envelope, &wrong_dek, ad);
        assert!(wrong_key_res.is_err());

        // Tampered ciphertext -> MUST fail
        let mut tampered = envelope.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        let tampered_res = decrypt_payload(&tampered, &dek, ad);
        assert!(tampered_res.is_err());
    }

    #[test]
    fn test_string_encryption_roundtrip() {
        let dek = generate_dek();
        let text = "आज की मीटिंग के नोट्स 📝 Hello World!";
        let ad = b"note:abc:title";

        let encrypted_b64 = encrypt_string(text, &dek, ad).expect("String encryption should succeed");
        assert_ne!(text, encrypted_b64);

        let decrypted = decrypt_string(&encrypted_b64, &dek, ad).expect("String decryption should succeed");
        assert_eq!(text, decrypted);
    }
}
