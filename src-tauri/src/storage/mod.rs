pub mod attachment_storage;
pub mod database;
pub mod errors;
pub mod migrations;
pub mod models;
pub mod paths;
pub mod repositories;
pub mod schema;
pub mod services;

pub use attachment_storage::{
    detect_mime_type, detect_mime_type_with_magic, sanitize_extension,
    sanitize_original_filename, AttachmentStorage, StoredAttachmentFile,
    ValidatedSourceFile, DEFAULT_MAX_ATTACHMENT_SIZE_BYTES,
};
pub use database::{init_connection, Database};
pub use errors::{AttachmentError, StorageError, TrashError};
pub use migrations::{run_migrations, run_migrations_custom, Migration, MIGRATIONS};
pub use models::*;
pub use paths::StoragePaths;
pub use repositories::*;
pub use schema::{apply_initial_schema, INITIAL_SCHEMA_SQL};
pub use services::{AttachmentService, SearchService, TrashService};

