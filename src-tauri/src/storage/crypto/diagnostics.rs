use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::storage::errors::CryptoError;

pub const MAX_AUDIT_LOGS: usize = 200;
pub const MAX_AUDIT_LOG_FILE_BYTES: u64 = 1024 * 1024;

/// Standardized security event codes for audit trail and diagnostics (Phase 12, Task 22).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SecurityEventCode {
    EncryptionSetupStarted,
    EncryptionSetupCompleted,
    EncryptionSetupFailed,
    EncryptionMigrationStarted,
    EncryptionMigrationCompleted,
    EncryptionMigrationFailed,
    UnlockAttempted,
    UnlockSucceeded,
    UnlockFailed,
    ManualLockTriggered,
    AutoLockTriggered,
    PasswordChangeStarted,
    PasswordChangeCompleted,
    PasswordChangeFailed,
    DisableEncryptionStarted,
    DisableEncryptionCompleted,
    DisableEncryptionFailed,
    StorageValidationFailed,
    BackupValidationFailed,
    RestoreRecoveryTriggered,
    MigrationRecoveryTriggered,
    UnsupportedVersionEncountered,
    DataIntegrityFailure,
}

impl SecurityEventCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SecurityEventCode::EncryptionSetupStarted => "ENCRYPTION_SETUP_STARTED",
            SecurityEventCode::EncryptionSetupCompleted => "ENCRYPTION_SETUP_COMPLETED",
            SecurityEventCode::EncryptionSetupFailed => "ENCRYPTION_SETUP_FAILED",
            SecurityEventCode::EncryptionMigrationStarted => "ENCRYPTION_MIGRATION_STARTED",
            SecurityEventCode::EncryptionMigrationCompleted => "ENCRYPTION_MIGRATION_COMPLETED",
            SecurityEventCode::EncryptionMigrationFailed => "ENCRYPTION_MIGRATION_FAILED",
            SecurityEventCode::UnlockAttempted => "UNLOCK_ATTEMPTED",
            SecurityEventCode::UnlockSucceeded => "UNLOCK_SUCCEEDED",
            SecurityEventCode::UnlockFailed => "UNLOCK_FAILED",
            SecurityEventCode::ManualLockTriggered => "MANUAL_LOCK_TRIGGERED",
            SecurityEventCode::AutoLockTriggered => "AUTO_LOCK_TRIGGERED",
            SecurityEventCode::PasswordChangeStarted => "PASSWORD_CHANGE_STARTED",
            SecurityEventCode::PasswordChangeCompleted => "PASSWORD_CHANGE_COMPLETED",
            SecurityEventCode::PasswordChangeFailed => "PASSWORD_CHANGE_FAILED",
            SecurityEventCode::DisableEncryptionStarted => "DISABLE_ENCRYPTION_STARTED",
            SecurityEventCode::DisableEncryptionCompleted => "DISABLE_ENCRYPTION_COMPLETED",
            SecurityEventCode::DisableEncryptionFailed => "DISABLE_ENCRYPTION_FAILED",
            SecurityEventCode::StorageValidationFailed => "STORAGE_VALIDATION_FAILED",
            SecurityEventCode::BackupValidationFailed => "BACKUP_VALIDATION_FAILED",
            SecurityEventCode::RestoreRecoveryTriggered => "RESTORE_RECOVERY_TRIGGERED",
            SecurityEventCode::MigrationRecoveryTriggered => "MIGRATION_RECOVERY_TRIGGERED",
            SecurityEventCode::UnsupportedVersionEncountered => "UNSUPPORTED_VERSION_ENCOUNTERED",
            SecurityEventCode::DataIntegrityFailure => "DATA_INTEGRITY_FAILURE",
        }
    }
}

/// Security audit log levels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum SecurityLogLevel {
    Info,
    Warning,
    Error,
    Critical,
}

impl SecurityLogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            SecurityLogLevel::Info => "INFO",
            SecurityLogLevel::Warning => "WARN",
            SecurityLogLevel::Error => "ERROR",
            SecurityLogLevel::Critical => "CRITICAL",
        }
    }
}

/// Structured, privacy-preserving audit log entry (Phase 12, Task 22).
/// Contains zero passwords, keys, or plaintext note contents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityLogEntry {
    pub timestamp: String,
    pub code: SecurityEventCode,
    pub level: SecurityLogLevel,
    pub message: String,
    pub details: Option<String>,
}

/// Sanitizes arbitrary error or diagnostic messages to guarantee that secrets, keys,
/// ciphertext envelopes, and private user text are strictly redacted (Task 22).
pub fn sanitize_security_message(raw: &str) -> String {
    let mut sanitized = raw.to_string();

    // 1. Redact ciphertext envelope strings (e.g. ENC$v1$...)
    if sanitized.contains("ENC$v1$") {
        let parts: Vec<&str> = sanitized.split("ENC$v1$").collect();
        let mut reconstructed = parts[0].to_string();
        for part in &parts[1..] {
            // Ciphertext base64 tokens typically end at whitespace, quote, or paren
            let token_end = part
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == ',')
                .unwrap_or(part.len());
            reconstructed.push_str("[REDACTED_CIPHERTEXT]");
            reconstructed.push_str(&part[token_end..]);
        }
        sanitized = reconstructed;
    }

    // 2. Redact password-bearing key-value pairs (e.g. password: "...", pwd=...)
    let sensitive_patterns = [
        "password",
        "current_password",
        "new_password",
        "confirm_password",
        "secret",
        "dek",
        "kek",
        "master_key",
    ];

    for pattern in sensitive_patterns {
        let lower = sanitized.to_lowercase();
        if let Some(pos) = lower.find(pattern) {
            // Check if followed by colon or equals
            let remainder = &sanitized[pos + pattern.len()..];
            if let Some(assign_pos) = remainder.find([':', '=']) {
                if assign_pos <= 2 {
                    let after_assign = remainder[assign_pos + 1..].trim_start();
                    let val_end = after_assign
                        .find(|c: char| c.is_whitespace() || c == ',' || c == '}' || c == ')')
                        .unwrap_or(after_assign.len());
                    if val_end > 0 {
                        let target = &after_assign[..val_end];
                        sanitized = sanitized.replace(target, "\"[REDACTED]\"");
                    }
                }
            }
        }
    }

    // 3. Strip raw note body / multi-line text dumps
    if sanitized.lines().count() > 3 {
        let lines: Vec<&str> = sanitized.lines().collect();
        sanitized = format!("{} ... [REDACTED MULTILINE CONTENT]", lines[0]);
    }

    sanitized
}

/// Internal classification differentiating credential errors from data corruption (Task 22).
pub fn classify_crypto_failure(err: &CryptoError) -> (&'static str, bool) {
    match err {
        CryptoError::AuthenticationFailed => ("credential_mismatch", false),
        CryptoError::TamperedOrCorrupted(_) => ("data_corruption_or_tampering", true),
        CryptoError::InvalidMetadata(_) => ("metadata_corruption", true),
        CryptoError::UnsupportedFormatVersion { .. } => ("unsupported_version", false),
        CryptoError::InvalidKdfConfig(_) => ("kdf_parameter_error", true),
        CryptoError::AppLocked => ("session_locked", false),
        CryptoError::NotEnabled => ("encryption_disabled", false),
        CryptoError::MigrationError(_) => ("migration_interrupted", true),
        CryptoError::KeyManagement(_) => ("key_management_error", true),
        CryptoError::EncryptionFailed(_) => ("encryption_operation_failure", false),
        CryptoError::DecryptionFailed(_) => ("decryption_operation_failure", true),
    }
}

/// Thread-safe circular audit logger and diagnostic manager.
pub struct SecurityAuditor {
    entries: Mutex<VecDeque<SecurityLogEntry>>,
    file_path: Mutex<Option<PathBuf>>,
}

impl Default for SecurityAuditor {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityAuditor {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_AUDIT_LOGS)),
            file_path: Mutex::new(None),
        }
    }

    pub fn set_file_sink(&self, path: PathBuf) {
        let mut guard = self.file_path.lock().unwrap();
        *guard = Some(path);
    }

    pub fn record(
        &self,
        code: SecurityEventCode,
        level: SecurityLogLevel,
        message: &str,
        details: Option<&str>,
    ) {
        let timestamp = Utc::now().to_rfc3339();
        let sanitized_msg = sanitize_security_message(message);
        let sanitized_details = details.map(sanitize_security_message);

        let entry = SecurityLogEntry {
            timestamp,
            code,
            level,
            message: sanitized_msg,
            details: sanitized_details,
        };

        // 1. In-memory circular buffer
        {
            let mut guard = self.entries.lock().unwrap();
            if guard.len() >= MAX_AUDIT_LOGS {
                guard.pop_front();
            }
            guard.push_back(entry.clone());
        }

        // 2. Append to local audit file if configured (strictly local, zero telemetry)
        {
            let path_guard = self.file_path.lock().unwrap();
            if let Some(ref path) = *path_guard {
                let _ = rotate_security_log_if_needed(path);
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                    if let Ok(json_line) = serde_json::to_string(&entry) {
                        let _ = writeln!(file, "{json_line}");
                    }
                }
            }
        }
    }

    pub fn get_entries(&self, limit: usize) -> Vec<SecurityLogEntry> {
        let guard = self.entries.lock().unwrap();
        let count = limit.min(guard.len());
        guard.iter().rev().take(count).cloned().collect()
    }

    pub fn clear(&self) {
        let mut guard = self.entries.lock().unwrap();
        guard.clear();
    }
}

fn rotate_security_log_if_needed(path: &PathBuf) -> std::io::Result<()> {
    if fs::metadata(path)
        .map(|metadata| metadata.len() < MAX_AUDIT_LOG_FILE_BYTES)
        .unwrap_or(true)
    {
        return Ok(());
    }

    let mut rotated_name = path.as_os_str().to_os_string();
    rotated_name.push(".1");
    let rotated_path = PathBuf::from(rotated_name);
    if rotated_path.exists() {
        fs::remove_file(&rotated_path)?;
    }
    fs::rename(path, rotated_path)
}

static AUDITOR: OnceLock<SecurityAuditor> = OnceLock::new();

pub fn get_auditor() -> &'static SecurityAuditor {
    AUDITOR.get_or_init(SecurityAuditor::new)
}

/// Convenience function to record a security event in the global audit trail.
pub fn record_security_event(
    code: SecurityEventCode,
    level: SecurityLogLevel,
    message: &str,
    details: Option<&str>,
) {
    get_auditor().record(code, level, message, details);
}

/// Configures local file sink for the security audit logger.
pub fn configure_security_log_file(path: PathBuf) {
    get_auditor().set_file_sink(path);
}

/// Retrieves the most recent security events from the in-memory circular buffer.
pub fn get_security_audit_log(limit: usize) -> Vec<SecurityLogEntry> {
    get_auditor().get_entries(limit)
}

/// Clears the in-memory audit log (primarily for isolated test assertions).
pub fn clear_security_audit_log() {
    get_auditor().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_event_recording_and_circular_buffer() {
        let auditor = SecurityAuditor::new();
        for i in 0..MAX_AUDIT_LOGS + 20 {
            auditor.record(
                SecurityEventCode::UnlockAttempted,
                SecurityLogLevel::Info,
                &format!("Unlock attempt {i}"),
                None,
            );
        }

        let entries = auditor.get_entries(300);
        assert_eq!(
            entries.len(),
            MAX_AUDIT_LOGS,
            "Buffer must cap at MAX_AUDIT_LOGS"
        );
        // Most recent should be unlock attempt (MAX_AUDIT_LOGS + 19)
        assert!(entries[0]
            .message
            .contains(&format!("Unlock attempt {}", MAX_AUDIT_LOGS + 19)));
    }

    #[test]
    fn test_sanitization_redacts_passwords_and_ciphertext() {
        let raw_with_pwd = "User login failed with password: MySecretPassword123! in request";
        let cleaned = sanitize_security_message(raw_with_pwd);
        assert!(!cleaned.contains("MySecretPassword123!"));
        assert!(cleaned.contains("[REDACTED]"));

        let raw_with_cipher = "Error decrypting note ENC$v1$dGhpcyBpcyBhIHRlc3Q= in database";
        let cleaned_cipher = sanitize_security_message(raw_with_cipher);
        assert!(!cleaned_cipher.contains("ENC$v1$dGhpcyBpcyBhIHRlc3Q="));
        assert!(cleaned_cipher.contains("[REDACTED_CIPHERTEXT]"));
    }

    #[test]
    fn test_failure_classification_distinguishes_credentials_from_corruption() {
        let auth_err = CryptoError::AuthenticationFailed;
        let (auth_class, auth_is_corrupt) = classify_crypto_failure(&auth_err);
        assert_eq!(auth_class, "credential_mismatch");
        assert!(
            !auth_is_corrupt,
            "Auth failure must not be classified as corruption"
        );

        let corrupt_err = CryptoError::TamperedOrCorrupted("Tag mismatch".to_string());
        let (corrupt_class, corrupt_is_corrupt) = classify_crypto_failure(&corrupt_err);
        assert_eq!(corrupt_class, "data_corruption_or_tampering");
        assert!(
            corrupt_is_corrupt,
            "Data tampering must be classified as corruption"
        );

        let meta_err = CryptoError::InvalidMetadata("Invalid JSON".to_string());
        let (meta_class, meta_is_corrupt) = classify_crypto_failure(&meta_err);
        assert_eq!(meta_class, "metadata_corruption");
        assert!(
            meta_is_corrupt,
            "Metadata error must be classified as corruption"
        );
    }

    #[test]
    fn test_local_file_logging() {
        let temp_dir = std::env::temp_dir().join(format!("sec_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let log_file = temp_dir.join("security_test.log");

        let auditor = SecurityAuditor::new();
        auditor.set_file_sink(log_file.clone());

        auditor.record(
            SecurityEventCode::EncryptionSetupCompleted,
            SecurityLogLevel::Info,
            "Encryption successfully configured",
            Some("version=1"),
        );

        assert!(log_file.exists());
        let content = std::fs::read_to_string(&log_file).unwrap();
        assert!(content.contains("ENCRYPTION_SETUP_COMPLETED"));
        assert!(content.contains("Encryption successfully configured"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_local_file_logging_rotates_at_size_limit() {
        let temp_dir = std::env::temp_dir().join(format!("sec_rotate_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let log_file = temp_dir.join("security_test.log");
        std::fs::write(&log_file, vec![b'x'; MAX_AUDIT_LOG_FILE_BYTES as usize]).unwrap();

        let auditor = SecurityAuditor::new();
        auditor.set_file_sink(log_file.clone());
        auditor.record(
            SecurityEventCode::ManualLockTriggered,
            SecurityLogLevel::Info,
            "Application locked",
            None,
        );

        let mut rotated_name = log_file.as_os_str().to_os_string();
        rotated_name.push(".1");
        let rotated_file = PathBuf::from(rotated_name);
        assert_eq!(
            std::fs::metadata(&rotated_file).unwrap().len(),
            MAX_AUDIT_LOG_FILE_BYTES
        );
        assert!(std::fs::metadata(&log_file).unwrap().len() < MAX_AUDIT_LOG_FILE_BYTES);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
