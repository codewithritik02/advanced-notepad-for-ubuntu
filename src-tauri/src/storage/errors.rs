use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Standard failure classification categories across Personal Notepad subsystems (Phase 13, Task 3).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCategory {
    /// Validation errors (e.g. empty names, oversized payloads, invalid formats). Non-retryable without user edit.
    Validation,
    /// Missing records, entities, or files.
    NotFound,
    /// Permission denied, read-only filesystem, access violations.
    Permission,
    /// Disk full or quota exceeded.
    DiskSpace,
    /// Transient database contention, locked connection, or concurrent background lock. Retryable.
    DatabaseBusy,
    /// Cryptographic session is locked or password required.
    Locked,
    /// Data integrity failures, corrupted databases, checksum mismatches, or tampered ciphertext.
    Corruption,
    /// Incompatible schema or archive format version.
    UnsupportedFormat,
    /// User or system initiated cancellation (not a crash).
    Cancelled,
    /// Unique constraint violation, naming collision, or hierarchy conflict.
    Conflict,
    /// Unexpected internal runtime or database failure.
    Internal,
}

/// Structured diagnostic error details safe for developer inspection and UI error classification (Phase 13, Task 3).
/// Guarantees that sensitive data (passwords, encryption keys, decrypted secrets, note bodies) are never included.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredDiagnostic {
    pub code: &'static str,
    pub category: ErrorCategory,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
}

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

    #[error("Import/Export error: {0}")]
    ImportExport(String),

    #[error("Backup error: {0}")]
    Backup(String),

    #[error("Crypto error: {0}")]
    Crypto(String),
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

    #[error("Opening executable attachments is blocked for security: {0}")]
    ExecutableBlocked(String),

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
            AttachmentError::ExecutableBlocked(_) => {
                "Opening executable attachment files directly is blocked for security reasons.".to_string()
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

    #[error("Encryption error: {0}")]
    Crypto(String),
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
            InternalLinkError::Crypto(msg) => msg.clone(),
        }
    }
}

impl From<InternalLinkError> for StorageError {
    fn from(err: InternalLinkError) -> Self {
        StorageError::InternalLink(err.user_friendly_message())
    }
}

/// Explicit errors occurring during Phase 10 Import/Export operations
#[derive(Debug, Error)]
pub enum ImportExportError {
    #[error("Operation cancelled by user.")]
    Cancelled,

    #[error("Unsupported file format: {0}")]
    UnsupportedFormat(String),

    #[error("Invalid character encoding: {0}")]
    InvalidEncoding(String),

    #[error("File exceeds maximum allowed size ({size_bytes} bytes > {max_bytes} bytes).")]
    FileTooLarge { size_bytes: u64, max_bytes: u64 },

    #[error("Cannot read file: {0}")]
    CannotReadFile(String),

    #[error("Destination directory is not writable: {0}")]
    DestinationNotWritable(String),

    #[error("Destination file collision: {0}")]
    DestinationCollision(String),

    #[error("Database error during import/export: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Serialization failed: {0}")]
    Serialization(String),

    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("Partial failure during batch operation: {0}")]
    PartialFailure(String),

    #[error("Filesystem IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl ImportExportError {
    pub fn user_friendly_message(&self) -> String {
        match self {
            ImportExportError::Cancelled => "Operation was cancelled.".to_string(),
            ImportExportError::UnsupportedFormat(ext) => {
                format!("Unsupported file format '{ext}'. Choose a TXT, Markdown, or JSON file.")
            }
            ImportExportError::InvalidEncoding(_) => {
                "This file contains invalid UTF-8 encoding and cannot be imported safely.".to_string()
            }
            ImportExportError::FileTooLarge { .. } => {
                "The selected file exceeds the maximum allowed size limit.".to_string()
            }
            ImportExportError::CannotReadFile(_) => {
                "This file could not be read. Please check that it exists and you have read permission.".to_string()
            }
            ImportExportError::DestinationNotWritable(_) => {
                "The export destination is not writable. Please choose another location.".to_string()
            }
            ImportExportError::DestinationCollision(f) => {
                format!("A file named '{f}' already exists in the destination folder.")
            }
            ImportExportError::Database(_) => {
                "A database error occurred while importing or exporting notes.".to_string()
            }
            ImportExportError::Serialization(msg) => {
                format!("Failed to format export content: {msg}")
            }
            ImportExportError::Validation(msg) => msg.clone(),
            ImportExportError::PartialFailure(msg) => msg.clone(),
            ImportExportError::Io(err) => match err.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    "Permission denied when accessing file or directory.".to_string()
                }
                std::io::ErrorKind::NotFound => "The specified file was not found.".to_string(),
                _ => "A filesystem error occurred.".to_string(),
            },
        }
    }
}

impl From<ImportExportError> for StorageError {
    fn from(err: ImportExportError) -> Self {
        StorageError::ImportExport(err.user_friendly_message())
    }
}

/// Strongly-typed backup and data safety operation errors (Phase 11, Task 4)
#[derive(Debug, Error)]
pub enum BackupError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Filesystem IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Destination unavailable: {0}")]
    DestinationUnavailable(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Insufficient disk space")]
    InsufficientDiskSpace,

    #[error("Database snapshot failed: {0}")]
    DatabaseSnapshotFailed(String),

    #[error("Archive write failed: {0}")]
    ArchiveWriteFailed(String),

    #[error("Archive read failed: {0}")]
    ArchiveReadFailed(String),

    #[error("Backup validation failed: {0}")]
    ValidationFailed(String),

    #[error("Manifest invalid: {0}")]
    ManifestInvalid(String),

    #[error("Checksum mismatch for {entry}: expected {expected}, actual {actual}")]
    ChecksumMismatch {
        entry: String,
        expected: String,
        actual: String,
    },

    #[error("Required attachment missing: {0}")]
    RequiredAttachmentMissing(String),

    #[error("Unsupported backup version: {version} (maximum supported is {max_supported})")]
    UnsupportedVersion {
        version: u32,
        max_supported: u32,
    },

    #[error("Database schema incompatible: backup schema {backup_schema} exceeds current application schema {current_schema}")]
    IncompatibleSchema {
        backup_schema: i32,
        current_schema: i32,
    },

    #[error("Restore checkpoint failed: {0}")]
    RestoreCheckpointFailed(String),

    #[error("Restore operation failed: {0}")]
    RestoreFailed(String),

    #[error("Rollback failed after restore error: {0}")]
    RollbackFailed(String),

    #[error("Post-restore validation failed: {0}")]
    PostRestoreValidationFailed(String),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Partial cleanup warning: {0}")]
    PartialCleanupWarning(String),

    #[error("Concurrent operation error: {0}")]
    ConcurrentOperation(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

impl BackupError {
    pub fn user_friendly_message(&self) -> String {
        match self {
            BackupError::ConcurrentOperation(msg) => format!("Cannot proceed: {msg}"),
            BackupError::DestinationUnavailable(msg) => format!("Backup destination is unavailable: {msg}"),
            BackupError::PermissionDenied(msg) => format!("Permission denied accessing backup location: {msg}"),
            BackupError::InsufficientDiskSpace => "Insufficient disk space to complete backup operation.".to_string(),
            BackupError::DatabaseSnapshotFailed(msg) => format!("Failed to create consistent database snapshot: {msg}"),
            BackupError::ArchiveWriteFailed(msg) => format!("Failed to write backup archive: {msg}"),
            BackupError::ArchiveReadFailed(msg) => format!("Failed to read backup archive: {msg}"),
            BackupError::ValidationFailed(msg) => format!("Backup validation failed: {msg}"),
            BackupError::ManifestInvalid(msg) => format!("Backup manifest is invalid: {msg}"),
            BackupError::ChecksumMismatch { entry, .. } => format!("Integrity check failed: checksum mismatch for '{entry}'."),
            BackupError::RequiredAttachmentMissing(name) => format!("Backup is incomplete: required attachment '{name}' is missing."),
            BackupError::UnsupportedVersion { version, max_supported } => {
                format!("Unsupported backup version {version}. This application supports up to version {max_supported}.")
            }
            BackupError::IncompatibleSchema { backup_schema, current_schema } => {
                format!("Backup database schema (v{backup_schema}) is newer than supported by this application (v{current_schema}). Please update Personal Notepad.")
            }
            BackupError::RestoreCheckpointFailed(msg) => format!("Safety checkpoint could not be created before restore: {msg}. Restore aborted to protect current data."),
            BackupError::RestoreFailed(msg) => format!("Restore operation failed: {msg}"),
            BackupError::RollbackFailed(msg) => format!("Restore failed and automatic rollback encountered an issue: {msg}"),
            BackupError::PostRestoreValidationFailed(msg) => format!("Post-restore verification failed: {msg}"),
            BackupError::Cancelled => "Backup or restore operation was cancelled.".to_string(),
            BackupError::PartialCleanupWarning(msg) => format!("Warning during cleanup: {msg}"),
            BackupError::InvalidRequest(msg) => msg.clone(),
            BackupError::Database(_) => "A database error occurred during backup or restore.".to_string(),
            BackupError::Io(err) => match err.kind() {
                std::io::ErrorKind::PermissionDenied => "Permission denied when performing backup filesystem operation.".to_string(),
                std::io::ErrorKind::NotFound => "Requested backup file or directory was not found.".to_string(),
                _ => "A filesystem IO error occurred during backup or restore.".to_string(),
            },
        }
    }

    /// Returns a coarse, privacy-safe classification string for operational logging and health tracking.
    pub fn category(&self) -> &'static str {
        match self {
            BackupError::ConcurrentOperation(_) => "ConcurrentOperation",
            BackupError::DestinationUnavailable(_) => "DestinationUnavailable",
            BackupError::PermissionDenied(_) => "PermissionDenied",
            BackupError::InsufficientDiskSpace => "InsufficientDiskSpace",
            BackupError::DatabaseSnapshotFailed(_) => "DatabaseSnapshotFailed",
            BackupError::ArchiveWriteFailed(_) => "ArchiveWriteFailed",
            BackupError::ArchiveReadFailed(_) => "ArchiveReadFailed",
            BackupError::ValidationFailed(_) => "ValidationFailed",
            BackupError::ManifestInvalid(_) => "ManifestInvalid",
            BackupError::ChecksumMismatch { .. } => "ChecksumMismatch",
            BackupError::RequiredAttachmentMissing(_) => "RequiredAttachmentMissing",
            BackupError::UnsupportedVersion { .. } => "UnsupportedVersion",
            BackupError::IncompatibleSchema { .. } => "IncompatibleSchema",
            BackupError::RestoreCheckpointFailed(_) => "RestoreCheckpointFailed",
            BackupError::RestoreFailed(_) => "RestoreFailed",
            BackupError::RollbackFailed(_) => "RollbackFailed",
            BackupError::PostRestoreValidationFailed(_) => "PostRestoreValidationFailed",
            BackupError::Cancelled => "Cancelled",
            BackupError::PartialCleanupWarning(_) => "PartialCleanupWarning",
            BackupError::InvalidRequest(_) => "InvalidRequest",
            BackupError::Database(_) => "Database",
            BackupError::Io(err) => match err.kind() {
                std::io::ErrorKind::PermissionDenied => "PermissionDenied",
                std::io::ErrorKind::NotFound => "NotFound",
                _ => "Io",
            },
        }
    }
}

impl From<BackupError> for StorageError {
    fn from(err: BackupError) -> Self {
        StorageError::Backup(err.user_friendly_message())
    }
}

/// Strongly-typed cryptographic, encryption, and key-management errors (Phase 12, Task 6).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("Encryption is not enabled on this installation")]
    NotEnabled,

    #[error("Unsupported encryption metadata format version: {version} (maximum supported: {max_supported})")]
    UnsupportedFormatVersion { version: u32, max_supported: u32 },

    #[error("Invalid encryption metadata: {0}")]
    InvalidMetadata(String),

    #[error("Invalid KDF configuration: {0}")]
    InvalidKdfConfig(String),

    #[error("Authentication failed: incorrect password or damaged key material")]
    AuthenticationFailed,

    #[error("Data tampering detected or corrupted ciphertext: {0}")]
    TamperedOrCorrupted(String),

    #[error("Encryption operation failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption operation failed: {0}")]
    DecryptionFailed(String),

    #[error("Application is locked. Access to protected data is restricted.")]
    AppLocked,

    #[error("Key management error: {0}")]
    KeyManagement(String),

    #[error("Migration error: {0}")]
    MigrationError(String),
}

impl CryptoError {
    pub fn user_friendly_message(&self) -> String {
        match self {
            CryptoError::NotEnabled => "Encryption is not enabled on this application.".to_string(),
            CryptoError::UnsupportedFormatVersion { version, .. } => {
                format!("This data was encrypted with a newer format version ({version}) and cannot be opened by this version of Personal Notepad.")
            }
            CryptoError::InvalidMetadata(_) => {
                "The encryption configuration for this application is invalid or corrupted.".to_string()
            }
            CryptoError::InvalidKdfConfig(msg) => {
                format!("Invalid encryption key derivation parameters: {msg}")
            }
            CryptoError::AuthenticationFailed => {
                "Incorrect password. Please try again.".to_string()
            }
            CryptoError::TamperedOrCorrupted(_) => {
                "Security check failed: the encrypted data appears to be modified or damaged.".to_string()
            }
            CryptoError::EncryptionFailed(_) => "Failed to encrypt data.".to_string(),
            CryptoError::DecryptionFailed(_) => "Failed to decrypt protected content.".to_string(),
            CryptoError::AppLocked => {
                "Personal Notepad is locked. Please enter your password to unlock.".to_string()
            }
            CryptoError::KeyManagement(_) => "Key management error occurred.".to_string(),
            CryptoError::MigrationError(msg) => format!("Encryption migration failed: {msg}"),
        }
    }
}

impl From<CryptoError> for StorageError {
    fn from(err: CryptoError) -> Self {
        StorageError::Crypto(err.user_friendly_message())
    }
}

impl StorageError {
    /// Transforms technical database and IO errors into clean, user-friendly messages
    /// suitable for presentation in the UI without exposing internal SQL implementation details.
    pub fn user_friendly_message(&self) -> String {
        match self {
            StorageError::Crypto(msg) => msg.clone(),
            StorageError::SearchError(_) => "Unable to search notes.".to_string(),
            StorageError::AttachmentError(msg) => msg.clone(),
            StorageError::InternalLink(msg) => msg.clone(),
            StorageError::ImportExport(msg) => msg.clone(),
            StorageError::Backup(msg) => msg.clone(),
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
                    "Permission denied when accessing application storage directory. Please check file permissions.".to_string()
                }
                std::io::ErrorKind::NotFound => {
                    "Requested storage file or directory was not found.".to_string()
                }
                _ if err.raw_os_error() == Some(28) => {
                    "Insufficient disk space to complete the operation. Please free disk space and try again.".to_string()
                }
                _ if err.raw_os_error() == Some(13) || err.raw_os_error() == Some(1) => {
                    "Permission denied when accessing application storage directory. Please check file permissions.".to_string()
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

    /// Classifies the failure into standard semantic category (Phase 13, Task 3).
    pub fn category(&self) -> ErrorCategory {
        match self {
            StorageError::Validation(_) | StorageError::InvalidHierarchy(_) => ErrorCategory::Validation,
            StorageError::NotFound(_) => ErrorCategory::NotFound,
            StorageError::Conflict(_) | StorageError::DeleteBlocked(_) => ErrorCategory::Conflict,
            StorageError::Database(rusqlite::Error::SqliteFailure(err, _)) => match err.code {
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => ErrorCategory::DatabaseBusy,
                rusqlite::ErrorCode::CannotOpen | rusqlite::ErrorCode::ReadOnly => ErrorCategory::Permission,
                rusqlite::ErrorCode::ConstraintViolation => ErrorCategory::Validation,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase => ErrorCategory::Corruption,
                _ => ErrorCategory::Internal,
            },
            StorageError::Database(_) => ErrorCategory::Internal,
            StorageError::Io(err) => match err.kind() {
                std::io::ErrorKind::PermissionDenied => ErrorCategory::Permission,
                std::io::ErrorKind::NotFound => ErrorCategory::NotFound,
                _ if err.raw_os_error() == Some(28) => ErrorCategory::DiskSpace,
                _ if err.raw_os_error() == Some(13) || err.raw_os_error() == Some(1) => ErrorCategory::Permission,
                _ => ErrorCategory::Internal,
            },
            StorageError::Crypto(msg) => {
                let lower = msg.to_lowercase();
                if lower.contains("locked") {
                    ErrorCategory::Locked
                } else if lower.contains("authentication failed") || lower.contains("tampered") || lower.contains("corrupt") {
                    ErrorCategory::Corruption
                } else {
                    ErrorCategory::Internal
                }
            }
            StorageError::AttachmentError(msg) => {
                let lower = msg.to_lowercase();
                if lower.contains("disk space") {
                    ErrorCategory::DiskSpace
                } else if lower.contains("permission") {
                    ErrorCategory::Permission
                } else if lower.contains("not found") {
                    ErrorCategory::NotFound
                } else if lower.contains("exceeds") || lower.contains("too large") {
                    ErrorCategory::Validation
                } else {
                    ErrorCategory::Internal
                }
            }
            StorageError::InternalLink(_) => ErrorCategory::Validation,
            StorageError::ImportExport(msg) => {
                let lower = msg.to_lowercase();
                if lower.contains("cancelled") {
                    ErrorCategory::Cancelled
                } else if lower.contains("unsupported") {
                    ErrorCategory::UnsupportedFormat
                } else if lower.contains("permission") {
                    ErrorCategory::Permission
                } else if lower.contains("collision") || lower.contains("already exists") {
                    ErrorCategory::Conflict
                } else {
                    ErrorCategory::Validation
                }
            }
            StorageError::Backup(msg) => {
                let lower = msg.to_lowercase();
                if lower.contains("cancelled") {
                    ErrorCategory::Cancelled
                } else if lower.contains("disk space") {
                    ErrorCategory::DiskSpace
                } else if lower.contains("concurrent") {
                    ErrorCategory::DatabaseBusy
                } else if lower.contains("unsupported") || lower.contains("schema") {
                    ErrorCategory::UnsupportedFormat
                } else if lower.contains("checksum") || lower.contains("corrupt") || lower.contains("invalid") {
                    ErrorCategory::Corruption
                } else {
                    ErrorCategory::Internal
                }
            }
            StorageError::Migration(_) => ErrorCategory::Internal,
            StorageError::StorageUnavailable(_) => ErrorCategory::Internal,
            StorageError::InitializationFailed(msg) => {
                let lower = msg.to_lowercase();
                if lower.contains("corrupt") || lower.contains("integrity") {
                    ErrorCategory::Corruption
                } else if lower.contains("busy") || lower.contains("locked") {
                    ErrorCategory::DatabaseBusy
                } else {
                    ErrorCategory::Internal
                }
            }
            StorageError::SearchError(_) => ErrorCategory::Internal,
        }
    }

    /// Determines whether the error is transient and safe to retry automatically.
    pub fn is_retryable(&self) -> bool {
        matches!(self.category(), ErrorCategory::DatabaseBusy)
    }

    /// Stable machine-readable error code.
    pub fn error_code(&self) -> &'static str {
        match self.category() {
            ErrorCategory::Validation => "VALIDATION_FAILED",
            ErrorCategory::NotFound => "NOT_FOUND",
            ErrorCategory::Permission => "PERMISSION_DENIED",
            ErrorCategory::DiskSpace => "INSUFFICIENT_DISK_SPACE",
            ErrorCategory::DatabaseBusy => "DATABASE_BUSY",
            ErrorCategory::Locked => "APP_LOCKED",
            ErrorCategory::Corruption => "DATA_CORRUPTION",
            ErrorCategory::UnsupportedFormat => "UNSUPPORTED_FORMAT",
            ErrorCategory::Cancelled => "OPERATION_CANCELLED",
            ErrorCategory::Conflict => "RESOURCE_CONFLICT",
            ErrorCategory::Internal => "INTERNAL_ERROR",
        }
    }

    /// Produces a safe, structured diagnostic representation free from sensitive secrets or note bodies.
    pub fn to_diagnostic(&self, operation: Option<&str>, safe_entity_id: Option<&str>) -> StructuredDiagnostic {
        StructuredDiagnostic {
            code: self.error_code(),
            category: self.category(),
            message: self.user_friendly_message(),
            retryable: self.is_retryable(),
            safe_entity_id: safe_entity_id.map(|s| s.to_string()),
            operation: operation.map(|s| s.to_string()),
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

        let enospc_err = StorageError::Io(std::io::Error::from_raw_os_error(28));
        assert!(enospc_err.user_friendly_message().contains("Insufficient disk space"));
        assert_eq!(enospc_err.category(), ErrorCategory::DiskSpace);

        let eacces_err = StorageError::Io(std::io::Error::from_raw_os_error(13));
        assert!(eacces_err.user_friendly_message().contains("Permission denied"));
        assert_eq!(eacces_err.category(), ErrorCategory::Permission);

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

    #[test]
    fn test_import_export_error_user_friendly_messages() {
        let cancel = ImportExportError::Cancelled;
        assert_eq!(cancel.user_friendly_message(), "Operation was cancelled.");

        let unsupp = ImportExportError::UnsupportedFormat("docx".to_string());
        assert!(unsupp.user_friendly_message().contains("Unsupported file format 'docx'"));

        let encoding = ImportExportError::InvalidEncoding("bad bytes".to_string());
        assert!(encoding.user_friendly_message().contains("invalid UTF-8 encoding"));

        let too_large = ImportExportError::FileTooLarge {
            size_bytes: 20 * 1024 * 1024,
            max_bytes: 10 * 1024 * 1024,
        };
        assert!(too_large.user_friendly_message().contains("size limit"));

        let collision = ImportExportError::DestinationCollision("Roadmap.md".to_string());
        assert!(collision.user_friendly_message().contains("Roadmap.md"));

        let converted: StorageError = unsupp.into();
        assert!(converted.user_friendly_message().contains("Unsupported file format"));
    }

    #[test]
    fn test_backup_error_user_friendly_messages() {
        let cancel = BackupError::Cancelled;
        assert_eq!(cancel.user_friendly_message(), "Backup or restore operation was cancelled.");

        let unsupp = BackupError::UnsupportedVersion {
            version: 2,
            max_supported: 1,
        };
        assert!(unsupp.user_friendly_message().contains("Unsupported backup version 2"));

        let mismatch = BackupError::ChecksumMismatch {
            entry: "database.sqlite".to_string(),
            expected: "abc".to_string(),
            actual: "def".to_string(),
        };
        assert!(mismatch.user_friendly_message().contains("checksum mismatch for 'database.sqlite'"));

        let missing_att = BackupError::RequiredAttachmentMissing("photo.png".to_string());
        assert!(missing_att.user_friendly_message().contains("photo.png"));

        let space = BackupError::InsufficientDiskSpace;
        assert!(space.user_friendly_message().contains("Insufficient disk space"));

        let checkpoint = BackupError::RestoreCheckpointFailed("disk write error".to_string());
        assert!(checkpoint.user_friendly_message().contains("Safety checkpoint could not be created"));

        let converted: StorageError = unsupp.into();
        assert!(converted.user_friendly_message().contains("Unsupported backup version 2"));
    }

    #[test]
    fn test_error_classification_and_diagnostics() {
        let val_err = StorageError::Validation("Too long".to_string());
        assert_eq!(val_err.category(), ErrorCategory::Validation);
        assert!(!val_err.is_retryable());
        assert_eq!(val_err.error_code(), "VALIDATION_FAILED");

        let not_found = StorageError::NotFound("Missing note".to_string());
        assert_eq!(not_found.category(), ErrorCategory::NotFound);
        assert!(!not_found.is_retryable());

        let busy_err = StorageError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            Some("database is locked".to_string()),
        ));
        assert_eq!(busy_err.category(), ErrorCategory::DatabaseBusy);
        assert!(busy_err.is_retryable());
        assert_eq!(busy_err.error_code(), "DATABASE_BUSY");

        let diag = val_err.to_diagnostic(Some("create_note"), Some("note-123"));
        assert_eq!(diag.code, "VALIDATION_FAILED");
        assert_eq!(diag.category, ErrorCategory::Validation);
        assert_eq!(diag.operation.as_deref(), Some("create_note"));
        assert_eq!(diag.safe_entity_id.as_deref(), Some("note-123"));
        assert!(!diag.retryable);
    }
}


