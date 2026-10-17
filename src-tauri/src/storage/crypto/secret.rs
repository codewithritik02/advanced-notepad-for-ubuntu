use crate::storage::errors::CryptoError;
use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const MIN_PASSWORD_LENGTH: usize = 8;
pub const MAX_PASSWORD_LENGTH: usize = 1024;

/// Validates password length and entropy requirements without arbitrary complexity rules (Task 7).
pub fn validate_password(password: &str) -> Result<(), CryptoError> {
    let char_count = password.chars().count();
    if char_count < MIN_PASSWORD_LENGTH {
        return Err(CryptoError::InvalidMetadata(format!(
            "Password must be at least {MIN_PASSWORD_LENGTH} characters long (provided: {char_count})."
        )));
    }
    if char_count > MAX_PASSWORD_LENGTH {
        return Err(CryptoError::InvalidMetadata(format!(
            "Password exceeds maximum allowed limit of {MAX_PASSWORD_LENGTH} characters."
        )));
    }
    if password.trim().is_empty() {
        return Err(CryptoError::InvalidMetadata(
            "Password cannot consist solely of whitespace.".to_string(),
        ));
    }
    Ok(())
}

/// Secure in-memory String container that zeroizes its contents upon drop (Task 7).
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(val: String) -> Self {
        Self(val)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for SecretString {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for SecretString {
    fn from(s: &str) -> Self {
        Self::new(s.to_string())
    }
}

// Redacted Debug implementation prevents accidental secret exposure in logs and traces
impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED SECRET STRING]")
    }
}

/// Secure in-memory Byte buffer container that zeroizes its contents upon drop (Task 7).
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for SecretBytes {
    fn from(b: Vec<u8>) -> Self {
        Self::new(b)
    }
}

// Redacted Debug implementation prevents accidental secret exposure in logs and traces
impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED SECRET BYTES (len={})]", self.0.len())
    }
}

/// DTO for initial encryption setup request with redacted debug logging (Task 7, 8).
#[derive(Clone, Serialize, Deserialize)]
pub struct SetupEncryptionDto {
    pub password: String,
    pub confirm_password: String,
}

impl fmt::Debug for SetupEncryptionDto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetupEncryptionDto")
            .field("password", &"[REDACTED]")
            .field("confirm_password", &"[REDACTED]")
            .finish()
    }
}

/// DTO for unlocking the application with redacted debug logging (Task 7, 12).
#[derive(Clone, Serialize, Deserialize)]
pub struct UnlockDto {
    pub password: String,
}

impl fmt::Debug for UnlockDto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnlockDto")
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// DTO for changing the encryption password with redacted debug logging (Task 7, 15).
#[derive(Clone, Serialize, Deserialize)]
pub struct ChangePasswordDto {
    pub current_password: String,
    pub new_password: String,
    pub confirm_new_password: String,
}

impl fmt::Debug for ChangePasswordDto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChangePasswordDto")
            .field("current_password", &"[REDACTED]")
            .field("new_password", &"[REDACTED]")
            .field("confirm_new_password", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_password_bounds() {
        assert!(
            validate_password("short").is_err(),
            "Password < 8 chars must be rejected"
        );
        assert!(
            validate_password("        ").is_err(),
            "Whitespace-only password must be rejected"
        );
        assert!(
            validate_password("correct-horse-battery-staple").is_ok(),
            "Legitimate long password must be accepted"
        );
        assert!(
            validate_password("12345678").is_ok(),
            "8 chars boundary password must be accepted"
        );

        let huge_pwd = "a".repeat(1025);
        assert!(
            validate_password(&huge_pwd).is_err(),
            "Password > 1024 chars must be rejected"
        );
    }

    #[test]
    fn test_secret_string_redacted_debug() {
        let sec = SecretString::new("SuperSecret123!".to_string());
        let debug_str = format!("{sec:?}");
        assert!(
            !debug_str.contains("SuperSecret123!"),
            "Secret must never appear in Debug output"
        );
        assert!(debug_str.contains("[REDACTED SECRET STRING]"));
    }

    #[test]
    fn test_secret_bytes_redacted_debug() {
        let sec = SecretBytes::new(vec![1, 2, 3, 4, 5]);
        let debug_str = format!("{sec:?}");
        assert!(!debug_str.contains("1, 2, 3, 4, 5"));
        assert!(debug_str.contains("[REDACTED SECRET BYTES (len=5)]"));
    }

    #[test]
    fn test_dto_redacted_debug() {
        let setup = SetupEncryptionDto {
            password: "MySecretPassword".to_string(),
            confirm_password: "MySecretPassword".to_string(),
        };
        let debug_str = format!("{setup:?}");
        assert!(
            !debug_str.contains("MySecretPassword"),
            "DTO Debug must not leak passwords"
        );
        assert!(debug_str.contains("[REDACTED]"));
    }
}
