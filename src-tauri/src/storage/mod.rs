pub mod attachment_storage;
pub mod crypto;
pub mod database;
pub mod errors;
pub mod lifecycle;
pub mod limits;
pub mod migrations;
pub mod models;
pub mod paths;
pub mod repositories;
pub mod schema;
pub mod services;

pub use limits::*;
pub use lifecycle::{
    checkpoint_database_for_shutdown, initialize_application_storage, ApplicationLifecycleError,
    InitializedApplicationStorage,
};

pub use attachment_storage::{
    detect_mime_type, detect_mime_type_with_magic, sanitize_extension,
    sanitize_original_filename, AttachmentStorage, StoredAttachmentFile,
    ValidatedSourceFile, DEFAULT_MAX_ATTACHMENT_SIZE_BYTES,
};
pub use crypto::*;
pub use database::{
    apply_pragmas, checkpoint_wal, init_connection, init_connection_with_config, query_pragma_int,
    query_pragma_string, retry_on_busy, Database, SqliteConfig, DEFAULT_BUSY_TIMEOUT_MS,
    DEFAULT_JOURNAL_MODE, DEFAULT_SYNCHRONOUS,
};
pub use errors::{
    AttachmentError, BackupError, CryptoError, ErrorCategory, ImportExportError, StorageError,
    StructuredDiagnostic, TrashError,
};
pub use migrations::{run_migrations, run_migrations_custom, Migration, MIGRATIONS};
pub use models::*;
pub use paths::StoragePaths;
pub use repositories::*;
pub use schema::{apply_initial_schema, INITIAL_SCHEMA_SQL};
pub use services::{AttachmentService, ImportExportService, SearchService, TrashService};
