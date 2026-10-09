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

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Invalid hierarchy: {0}")]
    InvalidHierarchy(String),

    #[error("Deletion blocked: {0}")]
    DeleteBlocked(String),

    #[error("Storage unavailable: {0}")]
    StorageUnavailable(String),

    #[error("Storage initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Search error: {0}")]
    SearchError(String),

    #[error("Attachment error: {0}")]
    AttachmentError(String),

    #[error("Internal link error: {0}")]
    InternalLink(String),
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Search execution failed: {0}")]
    Execution(String),
}

impl SearchError {
    pub fn user_friendly_message(&self) -> String {
        match self {
            SearchError::Validation(msg) => msg.clone(),
            _ => "Unable to search notes.".to_string(),
        }
    }
}

/// Strongly-typed attachment operation errors (Phase 7, Task 2)
#[derive(Debug, Error)]
pub enum AttachmentError {
    #[error("Note not found: {0}")]
    NoteNotFound(String),

    #[error("Attachment not found: {0}")]
    AttachmentNotFound(String),

    #[error("Invalid attachment request: {0}")]
    InvalidRequest(String),

    #[error("File size ({size_bytes} bytes) exceeds maximum limit of {max_bytes} bytes")]
    FileTooLarge { size_bytes: u64, max_bytes: u64 },

    #[error("Source file unavailable or invalid: {0}")]
    SourceUnavailable(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Insufficient disk space")]
    InsufficientDiskSpace,

    #[error("Managed attachment file missing on disk: {0}")]
    ManagedFileMissing(String),

    #[error("File operation failed: {0}")]
    FileOperationFailed(String),

    #[error("Failed to open file with default application: {0}")]
    OpenFailed(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Filesystem IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl AttachmentError {
    pub fn user_friendly_message(&self) -> String {
        match self {
            AttachmentError::NoteNotFound(_) => "The referenced note could not be found.".to_string(),
            AttachmentError::AttachmentNotFound(_) => "Attachment not found.".to_string(),
            AttachmentError::InvalidRequest(msg) => msg.clone(),
            AttachmentError::FileTooLarge { size_bytes: _, max_bytes } => {
                let max_mb = max_bytes / (1024 * 1024);
                format!("File exceeds maximum permitted size of {max_mb} MB.")
            }
            AttachmentError::SourceUnavailable(msg) => {
                format!("Selected source file could not be read or does not exist: {msg}")
            }
            AttachmentError::PermissionDenied(_) => {
                "Permission denied when accessing file.".to_string()
            }
            AttachmentError::InsufficientDiskSpace => {
                "Insufficient disk space to save attachment.".to_string()
            }
            AttachmentError::ManagedFileMissing(_) => {
                "Attachment file is missing from storage.".to_string()
            }
            AttachmentError::FileOperationFailed(msg) => {
                format!("File operation failed: {msg}")
            }
            AttachmentError::OpenFailed(msg) => {
                format!("Failed to open attachment: {msg}")
            }
            AttachmentError::Database(_) => {
                "A database error occurred while processing attachment.".to_string()
            }
            AttachmentError::Io(err) => match err.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    "Permission denied when accessing attachment file.".to_string()
                }
                std::io::ErrorKind::NotFound => {
                    "Attachment file was not found.".to_string()
                }
                _ => "A filesystem error occurred while processing attachment.".to_string(),
            },
        }
    }
}

impl From<AttachmentError> for StorageError {
    fn from(err: AttachmentError) -> Self {
        StorageError::AttachmentError(err.user_friendly_message())
    }
}

impl From<StorageError> for AttachmentError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::Database(e) => AttachmentError::Database(e),
            StorageError::Io(e) => AttachmentError::Io(e),
            StorageError::NotFound(msg) => AttachmentError::AttachmentNotFound(msg),
            StorageError::Validation(msg) => AttachmentError::InvalidRequest(msg),
            StorageError::AttachmentError(msg) => AttachmentError::FileOperationFailed(msg),
            _ => AttachmentError::FileOperationFailed(err.to_string()),
        }
    }
}

/// Strongly-typed trash operation errors (Phase 8, Task 5)
#[derive(Debug, Error)]
pub enum TrashError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Note not found: {0}")]
    NoteNotFound(String),

    #[error("Invalid lifecycle state: {0}")]
    InvalidState(String),

    #[error("Attachment cleanup failure: {0}")]
    AttachmentCleanupFailure(String),

    #[error("Attachment error: {0}")]
    Attachment(#[from] AttachmentError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

impl TrashError {
    pub fn user_friendly_message(&self) -> String {
        match self {
            TrashError::InvalidRequest(msg) => msg.clone(),
            TrashError::NoteNotFound(msg) => msg.clone(),
            TrashError::InvalidState(msg) => msg.clone(),
            TrashError::AttachmentCleanupFailure(msg) => {
                format!("Note was removed, but attachment cleanup encountered an issue: {msg}")
            }
            TrashError::Attachment(err) => err.user_friendly_message(),
            TrashError::Storage(err) => err.user_friendly_message(),
            TrashError::Database(_) => "A database error occurred during the trash operation.".to_string(),
        }
    }
}

/// Strongly-typed internal note link operation errors (Phase 9, Task 3)
#[derive(Debug, Error)]
pub enum InternalLinkError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Invalid note ID: {0}")]
    InvalidNoteId(String),

    #[error("Target note not found: {0}")]
    TargetNotFound(String),

    #[error("Target note is in trash: {0}")]
    TargetInTrash(String),

    #[error("Source note not found: {0}")]
    SourceNotFound(String),

    #[error("Malformed internal link syntax: {0}")]
    MalformedLink(String),
}

impl InternalLinkError {
    pub fn user_friendly_message(&self) -> String {
        match self {
            InternalLinkError::InvalidNoteId(_) => "Invalid note link identifier.".to_string(),
            InternalLinkError::TargetNotFound(_) => "The linked note could not be found.".to_string(),
            InternalLinkError::TargetInTrash(_) => "The linked note is in Trash.".to_string(),
            InternalLinkError::SourceNotFound(_) => "The source note could not be found.".to_string(),
            InternalLinkError::MalformedLink(_) => "Malformed internal note link.".to_string(),
            InternalLinkError::Database(_) => "A database error occurred while resolving the link.".to_string(),
        }
    }
}

impl From<InternalLinkError> for StorageError {
    fn from(err: InternalLinkError) -> Self {
        StorageError::InternalLink(err.user_friendly_message())
    }
}

impl StorageError {
    /// Transforms technical database and IO errors into clean, user-friendly messages
    /// suitable for presentation in the UI without exposing internal SQL implementation details.
    pub fn user_friendly_message(&self) -> String {
        match self {
            StorageError::SearchError(_) => "Unable to search notes.".to_string(),
            StorageError::AttachmentError(msg) => msg.clone(),
            StorageError::InternalLink(msg) => msg.clone(),
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
            StorageError::Conflict(msg) => msg.clone(),
            StorageError::InvalidHierarchy(msg) => msg.clone(),
            StorageError::DeleteBlocked(msg) => msg.clone(),
            StorageError::StorageUnavailable(msg) => msg.clone(),
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

        let att_err = StorageError::AttachmentError("Attachment not found.".to_string());
        assert_eq!(att_err.user_friendly_message(), "Attachment not found.");
    }

    #[test]
    fn test_attachment_error_user_friendly_messages() {
        let not_found = AttachmentError::AttachmentNotFound("att-1".to_string());
        assert!(not_found.user_friendly_message().contains("Attachment not found"));

        let too_large = AttachmentError::FileTooLarge {
            size_bytes: 150 * 1024 * 1024,
            max_bytes: 100 * 1024 * 1024,
        };
        assert!(too_large.user_friendly_message().contains("100 MB"));

        let perm = AttachmentError::PermissionDenied("protected.pdf".to_string());
        assert!(perm.user_friendly_message().contains("Permission denied"));

        let space = AttachmentError::InsufficientDiskSpace;
        assert!(space.user_friendly_message().contains("Insufficient disk space"));

        let missing = AttachmentError::ManagedFileMissing("att-1.pdf".to_string());
        assert!(missing.user_friendly_message().contains("missing from storage"));

        let converted: StorageError = too_large.into();
        assert!(converted.user_friendly_message().contains("100 MB"));
    }

    #[test]
    fn test_internal_link_error_user_friendly_messages() {
        let invalid_id = InternalLinkError::InvalidNoteId("bad-id".to_string());
        assert_eq!(invalid_id.user_friendly_message(), "Invalid note link identifier.");

        let target_not_found = InternalLinkError::TargetNotFound("n-123".to_string());
        assert_eq!(target_not_found.user_friendly_message(), "The linked note could not be found.");

        let target_in_trash = InternalLinkError::TargetInTrash("n-123".to_string());
        assert_eq!(target_in_trash.user_friendly_message(), "The linked note is in Trash.");

        let source_not_found = InternalLinkError::SourceNotFound("src-1".to_string());
        assert_eq!(source_not_found.user_friendly_message(), "The source note could not be found.");

        let malformed = InternalLinkError::MalformedLink("[[invalid]]".to_string());
        assert_eq!(malformed.user_friendly_message(), "Malformed internal note link.");

        let converted: StorageError = target_not_found.into();
        assert_eq!(converted.user_friendly_message(), "The linked note could not be found.");
    }
}


