use serde::{Deserialize, Serialize};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use crate::storage::errors::CryptoError;

pub const ENCRYPTION_FORMAT_IDENTIFIER: &str = "personal-notepad-encryption";
pub const CURRENT_ENCRYPTION_FORMAT_VERSION: u32 = 1;
pub const DEFAULT_CIPHER_ALGORITHM: &str = "AES-256-GCM";
pub const DEFAULT_KEY_WRAP_ALGORITHM: &str = "AES-256-GCM";
pub const DEFAULT_KDF_ALGORITHM: &str = "argon2id";
pub const DEFAULT_PAYLOAD_FORMAT_VERSION: u32 = 1;
pub const ENCRYPTION_METADATA_KEY: &str = "encryption_metadata";

// Defensive limits against DoS attacks via malformed metadata
pub const MIN_KDF_MEMORY_KIB: u32 = 8 * 1024;      // 8 MiB
pub const MAX_KDF_MEMORY_KIB: u32 = 256 * 1024;    // 256 MiB
pub const MIN_KDF_ITERATIONS: u32 = 1;
pub const MAX_KDF_ITERATIONS: u32 = 10;
pub const MIN_KDF_PARALLELISM: u32 = 1;
pub const MAX_KDF_PARALLELISM: u32 = 16;
pub const REQUIRED_SALT_LEN_BYTES: usize = 16;
pub const NONCE_LEN_BYTES: usize = 12;
pub const DEK_LEN_BYTES: usize = 32;
pub const AEAD_TAG_LEN_BYTES: usize = 16;

/// Operational migration and encryption status of storage (Task 6, 17, 18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    Unencrypted,
    SetupRequested,
    CheckpointCreated,
    MigrationInProgress,
    StagingPrepared,
    Verified,
    Active,
    RolledBack,
    Failed(String),
}

impl MigrationStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, MigrationStatus::Active)
    }

    pub fn is_interrupted(&self) -> bool {
        matches!(
            self,
            MigrationStatus::SetupRequested
                | MigrationStatus::CheckpointCreated
                | MigrationStatus::MigrationInProgress
                | MigrationStatus::StagingPrepared
                | MigrationStatus::Verified
        )
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, MigrationStatus::Failed(_))
    }
}

/// Versioned Key Derivation Function parameters (Argon2id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfConfig {
    pub version: u32,
    pub algorithm: String,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub salt_base64: String,
}

impl KdfConfig {
    pub fn default_profile(salt: [u8; REQUIRED_SALT_LEN_BYTES]) -> Self {
        Self {
            version: 1,
            algorithm: DEFAULT_KDF_ALGORITHM.to_string(),
            memory_kib: 65536, // 64 MiB
            iterations: 3,
            parallelism: 4,
            salt_base64: BASE64_STANDARD.encode(salt),
        }
    }

    pub fn validate(&self) -> Result<(), CryptoError> {
        if self.algorithm != DEFAULT_KDF_ALGORITHM {
            return Err(CryptoError::InvalidKdfConfig(format!(
                "Unsupported KDF algorithm '{}', expected '{}'",
                self.algorithm, DEFAULT_KDF_ALGORITHM
            )));
        }

        if self.memory_kib < MIN_KDF_MEMORY_KIB || self.memory_kib > MAX_KDF_MEMORY_KIB {
            return Err(CryptoError::InvalidKdfConfig(format!(
                "Memory cost {} KiB is outside allowed range ({} to {} KiB)",
                self.memory_kib, MIN_KDF_MEMORY_KIB, MAX_KDF_MEMORY_KIB
            )));
        }

        if self.iterations < MIN_KDF_ITERATIONS || self.iterations > MAX_KDF_ITERATIONS {
            return Err(CryptoError::InvalidKdfConfig(format!(
                "Iterations {} is outside allowed range ({} to {})",
                self.iterations, MIN_KDF_ITERATIONS, MAX_KDF_ITERATIONS
            )));
        }

        if self.parallelism < MIN_KDF_PARALLELISM || self.parallelism > MAX_KDF_PARALLELISM {
            return Err(CryptoError::InvalidKdfConfig(format!(
                "Parallelism {} is outside allowed range ({} to {})",
                self.parallelism, MIN_KDF_PARALLELISM, MAX_KDF_PARALLELISM
            )));
        }

        let salt_bytes = self.salt_bytes()?;
        if salt_bytes.len() != REQUIRED_SALT_LEN_BYTES {
            return Err(CryptoError::InvalidKdfConfig(format!(
                "Salt length is {} bytes, expected {} bytes",
                salt_bytes.len(),
                REQUIRED_SALT_LEN_BYTES
            )));
        }

        Ok(())
    }

    pub fn salt_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        BASE64_STANDARD.decode(&self.salt_base64).map_err(|e| {
            CryptoError::InvalidKdfConfig(format!("Malformed base64 salt: {e}"))
        })
    }
}

/// Encrypted Data Encryption Key material (Wrapped Key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKey {
    pub nonce_base64: String,
    pub ciphertext_base64: String,
    pub tag_base64: String,
}

impl WrappedKey {
    pub fn new(nonce: &[u8], ciphertext: &[u8], tag: &[u8]) -> Self {
        Self {
            nonce_base64: BASE64_STANDARD.encode(nonce),
            ciphertext_base64: BASE64_STANDARD.encode(ciphertext),
            tag_base64: BASE64_STANDARD.encode(tag),
        }
    }

    pub fn validate(&self) -> Result<(), CryptoError> {
        let nonce = self.nonce_bytes()?;
        if nonce.len() != NONCE_LEN_BYTES {
            return Err(CryptoError::InvalidMetadata(format!(
                "Wrapped key nonce length is {} bytes, expected {}",
                nonce.len(), NONCE_LEN_BYTES
            )));
        }

        let ciphertext = self.ciphertext_bytes()?;
        if ciphertext.len() != DEK_LEN_BYTES {
            return Err(CryptoError::InvalidMetadata(format!(
                "Wrapped key ciphertext length is {} bytes, expected {}",
                ciphertext.len(), DEK_LEN_BYTES
            )));
        }

        let tag = self.tag_bytes()?;
        if tag.len() != AEAD_TAG_LEN_BYTES {
            return Err(CryptoError::InvalidMetadata(format!(
                "Wrapped key authentication tag length is {} bytes, expected {}",
                tag.len(), AEAD_TAG_LEN_BYTES
            )));
        }

        Ok(())
    }

    pub fn nonce_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        BASE64_STANDARD.decode(&self.nonce_base64).map_err(|e| {
            CryptoError::InvalidMetadata(format!("Invalid wrapped key nonce base64: {e}"))
        })
    }

    pub fn ciphertext_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        BASE64_STANDARD.decode(&self.ciphertext_base64).map_err(|e| {
            CryptoError::InvalidMetadata(format!("Invalid wrapped key ciphertext base64: {e}"))
        })
    }

    pub fn tag_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        BASE64_STANDARD.decode(&self.tag_base64).map_err(|e| {
            CryptoError::InvalidMetadata(format!("Invalid wrapped key tag base64: {e}"))
        })
    }
}

/// Authoritative versioned record describing the encryption configuration without exposing secrets (Phase 12, Task 6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionMetadata {
    pub format_identifier: String,
    pub format_version: u32,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub cipher_algorithm: String,
    pub key_wrap_algorithm: String,
    pub kdf: KdfConfig,
    pub wrapped_dek: WrappedKey,
    pub migration_status: MigrationStatus,
    pub payload_format_version: u32,
}

impl EncryptionMetadata {
    pub fn new(kdf: KdfConfig, wrapped_dek: WrappedKey) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            format_identifier: ENCRYPTION_FORMAT_IDENTIFIER.to_string(),
            format_version: CURRENT_ENCRYPTION_FORMAT_VERSION,
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
            cipher_algorithm: DEFAULT_CIPHER_ALGORITHM.to_string(),
            key_wrap_algorithm: DEFAULT_KEY_WRAP_ALGORITHM.to_string(),
            kdf,
            wrapped_dek,
            migration_status: MigrationStatus::Active,
            payload_format_version: DEFAULT_PAYLOAD_FORMAT_VERSION,
        }
    }

    pub fn validate(&self) -> Result<(), CryptoError> {
        if self.format_identifier != ENCRYPTION_FORMAT_IDENTIFIER {
            return Err(CryptoError::InvalidMetadata(format!(
                "Invalid format identifier '{}', expected '{}'",
                self.format_identifier, ENCRYPTION_FORMAT_IDENTIFIER
            )));
        }

        if self.format_version > CURRENT_ENCRYPTION_FORMAT_VERSION {
            return Err(CryptoError::UnsupportedFormatVersion {
                version: self.format_version,
                max_supported: CURRENT_ENCRYPTION_FORMAT_VERSION,
            });
        }

        if self.format_version == 0 {
            return Err(CryptoError::InvalidMetadata(
                "Format version 0 is invalid".to_string(),
            ));
        }

        if self.cipher_algorithm != DEFAULT_CIPHER_ALGORITHM {
            return Err(CryptoError::InvalidMetadata(format!(
                "Unsupported cipher algorithm '{}', expected '{}'",
                self.cipher_algorithm, DEFAULT_CIPHER_ALGORITHM
            )));
        }

        if self.key_wrap_algorithm != DEFAULT_KEY_WRAP_ALGORITHM {
            return Err(CryptoError::InvalidMetadata(format!(
                "Unsupported key wrapping algorithm '{}', expected '{}'",
                self.key_wrap_algorithm, DEFAULT_KEY_WRAP_ALGORITHM
            )));
        }

        self.kdf.validate()?;
        self.wrapped_dek.validate()?;

        Ok(())
    }

    pub fn to_json(&self) -> Result<String, CryptoError> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|e| {
            CryptoError::InvalidMetadata(format!("Failed to serialize encryption metadata: {e}"))
        })
    }

    pub fn from_json(json_str: &str) -> Result<Self, CryptoError> {
        let meta: Self = serde_json::from_str(json_str).map_err(|e| {
            CryptoError::InvalidMetadata(format!("Failed to parse encryption metadata JSON: {e}"))
        })?;
        meta.validate()?;
        Ok(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_valid_metadata() -> EncryptionMetadata {
        let salt = [42u8; REQUIRED_SALT_LEN_BYTES];
        let kdf = KdfConfig::default_profile(salt);
        let wrapped_key = WrappedKey::new(
            &[1u8; NONCE_LEN_BYTES],
            &[2u8; DEK_LEN_BYTES],
            &[3u8; AEAD_TAG_LEN_BYTES],
        );
        EncryptionMetadata::new(kdf, wrapped_key)
    }

    #[test]
    fn test_valid_metadata_roundtrip() {
        let meta = dummy_valid_metadata();
        let json = meta.to_json().expect("Serialization should succeed");

        // Verify secrets are NOT present
        assert!(!json.contains("password"), "JSON must never contain password field");
        assert!(json.contains("personal-notepad-encryption"));
        assert!(json.contains("argon2id"));
        assert!(json.contains("AES-256-GCM"));

        let deserialized = EncryptionMetadata::from_json(&json).expect("Deserialization should succeed");
        assert_eq!(meta, deserialized);
    }

    #[test]
    fn test_reject_unsupported_future_version() {
        let mut meta = dummy_valid_metadata();
        meta.format_version = 2; // Newer version than supported (1)
        let json = serde_json::to_string(&meta).unwrap();

        let err = EncryptionMetadata::from_json(&json).unwrap_err();
        assert_eq!(
            err,
            CryptoError::UnsupportedFormatVersion {
                version: 2,
                max_supported: CURRENT_ENCRYPTION_FORMAT_VERSION
            }
        );
    }

    #[test]
    fn test_reject_invalid_format_identifier() {
        let mut meta = dummy_valid_metadata();
        meta.format_identifier = "malicious-app-encryption".to_string();
        let json = serde_json::to_string(&meta).unwrap();

        let err = EncryptionMetadata::from_json(&json).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidMetadata(_)));
    }

    #[test]
    fn test_reject_kdf_memory_bounds_violation() {
        let mut meta = dummy_valid_metadata();
        meta.kdf.memory_kib = 1024; // Less than 8192 KiB minimum
        let json = serde_json::to_string(&meta).unwrap();

        let err = EncryptionMetadata::from_json(&json).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidKdfConfig(_)));

        meta.kdf.memory_kib = 512 * 1024; // More than 256 MiB maximum
        let json = serde_json::to_string(&meta).unwrap();
        let err2 = EncryptionMetadata::from_json(&json).unwrap_err();
        assert!(matches!(err2, CryptoError::InvalidKdfConfig(_)));
    }

    #[test]
    fn test_reject_kdf_iteration_bounds_violation() {
        let mut meta = dummy_valid_metadata();
        meta.kdf.iterations = 0;
        let json = serde_json::to_string(&meta).unwrap();
        let err = EncryptionMetadata::from_json(&json).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidKdfConfig(_)));

        meta.kdf.iterations = 100; // Excessive iterations
        let json = serde_json::to_string(&meta).unwrap();
        let err2 = EncryptionMetadata::from_json(&json).unwrap_err();
        assert!(matches!(err2, CryptoError::InvalidKdfConfig(_)));
    }

    #[test]
    fn test_reject_invalid_salt_length() {
        let mut meta = dummy_valid_metadata();
        meta.kdf.salt_base64 = BASE64_STANDARD.encode(b"short"); // 5 bytes instead of 16
        let json = serde_json::to_string(&meta).unwrap();

        let err = EncryptionMetadata::from_json(&json).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidKdfConfig(_)));
    }

    #[test]
    fn test_reject_invalid_wrapped_key_lengths() {
        let mut meta = dummy_valid_metadata();
        meta.wrapped_dek.nonce_base64 = BASE64_STANDARD.encode(&[1u8; 8]); // 8 bytes instead of 12
        let json = serde_json::to_string(&meta).unwrap();

        let err = EncryptionMetadata::from_json(&json).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidMetadata(_)));
    }
}
