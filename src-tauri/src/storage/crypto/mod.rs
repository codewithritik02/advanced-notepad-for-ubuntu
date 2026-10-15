pub mod cipher;
pub mod diagnostics;
pub mod kdf;
pub mod metadata;
pub mod migration;
pub mod secret;
pub mod session;

pub use diagnostics::{
    classify_crypto_failure, clear_security_audit_log, configure_security_log_file,
    get_security_audit_log, record_security_event, sanitize_security_message,
    SecurityAuditor, SecurityEventCode, SecurityLogEntry, SecurityLogLevel, MAX_AUDIT_LOGS,
};

pub use migration::{
    DisableEncryptionReport, EncryptionTransitionMarker, MigrationReport, MigrationService,
    StartupRecoveryDecision,
};

pub use cipher::{
    decrypt_file_to_file, decrypt_payload, decrypt_string, encrypt_file_to_file, encrypt_payload,
    encrypt_string, is_encrypted_string, ENCRYPTED_PREFIX_V1, ENVELOPE_VERSION_V1,
    MIN_ENVELOPE_LEN,
};
pub use kdf::{derive_kek, generate_dek, generate_salt, unwrap_dek, wrap_dek, DEK_WRAP_ASSOCIATED_DATA};
pub use metadata::{
    EncryptionMetadata, KdfConfig, MigrationStatus, WrappedKey, CURRENT_ENCRYPTION_FORMAT_VERSION,
    DEFAULT_CIPHER_ALGORITHM, DEFAULT_KDF_ALGORITHM, DEFAULT_KEY_WRAP_ALGORITHM,
    DEFAULT_PAYLOAD_FORMAT_VERSION, DEK_LEN_BYTES, ENCRYPTION_FORMAT_IDENTIFIER,
    ENCRYPTION_METADATA_KEY, MAX_KDF_ITERATIONS, MAX_KDF_MEMORY_KIB, MAX_KDF_PARALLELISM,
    MIN_KDF_ITERATIONS, MIN_KDF_MEMORY_KIB, MIN_KDF_PARALLELISM, NONCE_LEN_BYTES,
    REQUIRED_SALT_LEN_BYTES,
};
pub use secret::{
    validate_password, ChangePasswordDto, SecretBytes, SecretString, SetupEncryptionDto, UnlockDto,
    MAX_PASSWORD_LENGTH, MIN_PASSWORD_LENGTH,
};
pub use session::{CryptoSession, SessionState};
