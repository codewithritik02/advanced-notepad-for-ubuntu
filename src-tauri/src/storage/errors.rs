use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Filesystem IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Storage initialization failed: {0}")]
    InitializationFailed(String),
}

impl StorageError {
    /// Transforms technical database and IO errors into clean, user-friendly messages
    /// suitable for presentation in the UI without exposing internal SQL implementation details.
    pub fn user_friendly_message(&self) -> String {
        match self {
            StorageError::Database(rusqlite::Error::SqliteFailure(err, _)) => {
                match err.code {
                    rusqlite::ErrorCode::ConstraintViolation => {
                        "The requested operation could not be completed because a referenced item no longer exists or a unique constraint was violated.".to_string()
                    }
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                        "Local database is currently busy. Please try again in a moment.".to_string()
                    }
                    rusqlite::ErrorCode::CannotOpen | rusqlite::ErrorCode::ReadOnly => {
                        "Unable to open local database file. Please verify disk permissions.".to_string()
                    }
                    _ => "A database error occurred while processing your request.".to_string(),
                }
            }
            StorageError::Database(_) => "A database error occurred while processing your request.".to_string(),
            StorageError::Io(err) => match err.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    "Permission denied when accessing application storage directory.".to_string()
                }
                std::io::ErrorKind::NotFound => {
                    "Requested storage file or directory was not found.".to_string()
                }
                _ => "A filesystem IO error occurred.".to_string(),
            },
            StorageError::NotFound(msg) => msg.clone(),
            StorageError::Validation(msg) => msg.clone(),
            StorageError::Migration(msg) => format!("Database schema migration error: {msg}"),
            StorageError::InitializationFailed(msg) => format!("Storage initialization error: {msg}"),
        }
    }
}

impl serde::Serialize for StorageError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.user_friendly_message())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_friendly_error_messages() {
        let not_found = StorageError::NotFound("Note not found".to_string());
        assert_eq!(not_found.user_friendly_message(), "Note not found");

        let validation = StorageError::Validation("Title cannot be empty".to_string());
        assert_eq!(validation.user_friendly_message(), "Title cannot be empty");

        let io_err = StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Access denied",
        ));
        assert!(io_err.user_friendly_message().contains("Permission denied"));
    }
}
