use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::storage::crypto::metadata::{
    KdfConfig, WrappedKey, AEAD_TAG_LEN_BYTES, DEK_LEN_BYTES, NONCE_LEN_BYTES,
};
use crate::storage::crypto::secret::SecretBytes;
use crate::storage::errors::CryptoError;

pub const DEK_WRAP_ASSOCIATED_DATA: &[u8] = b"pn_dek_wrap_v1";

/// Derives a 256-bit Key Encryption Key (KEK) from a user password and salt using Argon2id (Task 5, 7).
pub fn derive_kek(password: &str, config: &KdfConfig) -> Result<SecretBytes, CryptoError> {
    config.validate()?;

    let salt_bytes = config.salt_bytes()?;

    let params = Params::new(
        config.memory_kib,
        config.iterations,
        config.parallelism,
        Some(DEK_LEN_BYTES),
    )
    .map_err(|e| CryptoError::InvalidKdfConfig(format!("Failed to build Argon2 params: {e}")))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut kek = [0u8; DEK_LEN_BYTES];
    argon2
        .hash_password_into(password.as_bytes(), &salt_bytes, &mut kek)
        .map_err(|e| CryptoError::InvalidKdfConfig(format!("Argon2id key derivation failed: {e}")))?;

    Ok(SecretBytes::new(kek.to_vec()))
}

/// Generates a fresh, cryptographically secure 256-bit Data Encryption Key (DEK) via OS CSPRNG (Task 5).
pub fn generate_dek() -> SecretBytes {
    let mut dek = [0u8; DEK_LEN_BYTES];
    OsRng.fill_bytes(&mut dek);
    SecretBytes::new(dek.to_vec())
}

/// Generates a fresh 16-byte random salt for Argon2id KDF derivation via OS CSPRNG (Task 5).
pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Protects (wraps) the master Data Encryption Key using the derived Key Encryption Key (AES-256-GCM).
pub fn wrap_dek(dek: &SecretBytes, kek: &SecretBytes) -> Result<WrappedKey, CryptoError> {
    if dek.len() != DEK_LEN_BYTES {
        return Err(CryptoError::KeyManagement(format!(
            "Invalid DEK length: {} bytes, expected {}",
            dek.len(),
            DEK_LEN_BYTES
        )));
    }
    if kek.len() != DEK_LEN_BYTES {
        return Err(CryptoError::KeyManagement(format!(
            "Invalid KEK length: {} bytes, expected {}",
            kek.len(),
            DEK_LEN_BYTES
        )));
    }

    let mut nonce_bytes = [0u8; NONCE_LEN_BYTES];
    OsRng.fill_bytes(&mut nonce_bytes);

    let cipher_key = Key::<Aes256Gcm>::from_slice(kek.as_slice());
    let cipher = Aes256Gcm::new(cipher_key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let payload = Payload {
        msg: dek.as_slice(),
        aad: DEK_WRAP_ASSOCIATED_DATA,
    };

    let encrypted_with_tag = cipher
        .encrypt(nonce, payload)
        .map_err(|e| CryptoError::EncryptionFailed(format!("Failed to wrap DEK: {e}")))?;

    // AES-GCM appends the 16-byte tag to the ciphertext
    if encrypted_with_tag.len() < AEAD_TAG_LEN_BYTES {
        return Err(CryptoError::EncryptionFailed(
            "Wrapped ciphertext payload too short".to_string(),
        ));
    }

    let split_pos = encrypted_with_tag.len() - AEAD_TAG_LEN_BYTES;
    let ciphertext = &encrypted_with_tag[..split_pos];
    let tag = &encrypted_with_tag[split_pos..];

    Ok(WrappedKey::new(&nonce_bytes, ciphertext, tag))
}

/// Recovers (unwraps) the master Data Encryption Key using the derived Key Encryption Key (AES-256-GCM).
/// Returns `CryptoError::AuthenticationFailed` if the password is incorrect or data was tampered with.
pub fn unwrap_dek(wrapped: &WrappedKey, kek: &SecretBytes) -> Result<SecretBytes, CryptoError> {
    wrapped.validate()?;

    if kek.len() != DEK_LEN_BYTES {
        return Err(CryptoError::KeyManagement(format!(
            "Invalid KEK length: {} bytes, expected {}",
            kek.len(),
            DEK_LEN_BYTES
        )));
    }

    let nonce_bytes = wrapped.nonce_bytes()?;
    let ciphertext_bytes = wrapped.ciphertext_bytes()?;
    let tag_bytes = wrapped.tag_bytes()?;

    // Reconstruct ciphertext + tag buffer expected by aes-gcm crate
    let mut encrypted_payload = Vec::with_capacity(ciphertext_bytes.len() + tag_bytes.len());
    encrypted_payload.extend_from_slice(&ciphertext_bytes);
    encrypted_payload.extend_from_slice(&tag_bytes);

    let cipher_key = Key::<Aes256Gcm>::from_slice(kek.as_slice());
    let cipher = Aes256Gcm::new(cipher_key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let payload = Payload {
        msg: &encrypted_payload,
        aad: DEK_WRAP_ASSOCIATED_DATA,
    };

    let decrypted = cipher
        .decrypt(nonce, payload)
        .map_err(|_| CryptoError::AuthenticationFailed)?;

    if decrypted.len() != DEK_LEN_BYTES {
        return Err(CryptoError::KeyManagement(format!(
            "Unwrapped key length {} is invalid",
            decrypted.len()
        )));
    }

    Ok(SecretBytes::new(decrypted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn test_kdf_derivation_and_wrapping_roundtrip() {
        let password = "CorrectHorseBatteryStaple!";
        let salt = generate_salt();
        let mut config = KdfConfig::default_profile(salt);
        // Use smaller memory for test speed (16 MiB)
        config.memory_kib = 16 * 1024;
        config.iterations = 1;

        // 1. Derive KEK
        let kek = derive_kek(password, &config).expect("KEK derivation should succeed");
        assert_eq!(kek.len(), 32);

        // 2. Generate random DEK
        let dek = generate_dek();
        assert_eq!(dek.len(), 32);

        // 3. Wrap DEK
        let wrapped = wrap_dek(&dek, &kek).expect("DEK wrapping should succeed");
        wrapped.validate().expect("Wrapped key should be valid");

        // 4. Unwrap DEK with correct password
        let recovered_dek = unwrap_dek(&wrapped, &kek).expect("Unwrapping with correct KEK should succeed");
        assert_eq!(dek.as_slice(), recovered_dek.as_slice());

        // 5. Unwrap DEK with incorrect password -> MUST fail with AuthenticationFailed
        let wrong_kek = derive_kek("WrongPassword123!", &config).expect("Derivation of wrong key succeeds");
        let fail_res = unwrap_dek(&wrapped, &wrong_kek);
        assert_eq!(fail_res, Err(CryptoError::AuthenticationFailed));

        // 6. Unwrap DEK with tampered tag -> MUST fail with AuthenticationFailed
        let mut tampered_wrapped = wrapped.clone();
        let mut tampered_tag = tampered_wrapped.tag_bytes().unwrap();
        tampered_tag[0] ^= 0xFF; // Flip bits in auth tag
        tampered_wrapped.tag_base64 = base64::engine::general_purpose::STANDARD.encode(&tampered_tag);
        let tamper_res = unwrap_dek(&tampered_wrapped, &kek);
        assert_eq!(tamper_res, Err(CryptoError::AuthenticationFailed));
    }
}
