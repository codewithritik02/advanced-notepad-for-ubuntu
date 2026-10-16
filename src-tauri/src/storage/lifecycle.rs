use crate::storage::crypto::CryptoSession;
use crate::storage::database::{checkpoint_wal, init_connection, Database};
use crate::storage::errors::{BackupError, CryptoError, StorageError};
use crate::storage::migrations::run_migrations_with_safety;
use crate::storage::paths::{AppInstanceLock, StoragePaths};
use crate::storage::services::RestoreService;

/// Fully initialized storage state produced by the same lifecycle entry point used by Tauri.
pub struct InitializedApplicationStorage {
    pub paths: StoragePaths,
    pub database: Database,
    pub crypto_session: CryptoSession,
    pub instance_lock: AppInstanceLock,
}

/// Stage-specific startup/shutdown failures suitable for both logs and user-visible startup errors.
#[derive(Debug, thiserror::Error)]
pub enum ApplicationLifecycleError {
    #[error("Application data directories could not be prepared: {0}")]
    PrepareDirectories(StorageError),
    #[error("Interrupted temporary work could not be cleaned up safely: {0}")]
    CleanupTemporaryWork(StorageError),
    #[error("Application data is already in use or could not be locked: {0}")]
    AcquireInstanceLock(StorageError),
    #[error("An interrupted restore could not be recovered safely: {0}")]
    RecoverInterruptedRestore(BackupError),
    #[error("The application database could not be opened safely: {0}")]
    OpenDatabase(StorageError),
    #[error("The application database could not be upgraded safely: {0}")]
    RunMigrations(StorageError),
    #[error("The encryption configuration is invalid or could not be recovered: {0}")]
    InitializeEncryption(CryptoError),
    #[error("The application database could not be flushed before shutdown: {0}")]
    CheckpointDatabase(StorageError),
}

/// Initializes application storage in the required recovery-before-open order.
///
/// No failed stage replaces or isolates an existing database. The instance lock is released
/// automatically if a later stage fails because it is owned by the returned runtime state.
pub fn initialize_application_storage(
    paths: StoragePaths,
) -> Result<InitializedApplicationStorage, ApplicationLifecycleError> {
    paths
        .ensure_directories()
        .map_err(ApplicationLifecycleError::PrepareDirectories)?;
    paths
        .cleanup_orphan_temp_files()
        .map_err(ApplicationLifecycleError::CleanupTemporaryWork)?;

    let instance_lock =
        AppInstanceLock::acquire(&paths).map_err(ApplicationLifecycleError::AcquireInstanceLock)?;

    RestoreService::recover_if_interrupted(&paths)
        .map_err(ApplicationLifecycleError::RecoverInterruptedRestore)?;

    let mut connection =
        init_connection(&paths.database_file).map_err(ApplicationLifecycleError::OpenDatabase)?;
    run_migrations_with_safety(&mut connection, Some(&paths.database_file))
        .map_err(ApplicationLifecycleError::RunMigrations)?;
    let crypto_session = CryptoSession::init_and_recover(&connection, &paths)
        .map_err(ApplicationLifecycleError::InitializeEncryption)?;

    Ok(InitializedApplicationStorage {
        paths,
        database: Database::new(connection),
        crypto_session,
        instance_lock,
    })
}

/// Flushes committed WAL frames before the native window is destroyed.
pub fn checkpoint_database_for_shutdown(
    database: &Database,
) -> Result<(), ApplicationLifecycleError> {
    let connection = database.conn.lock().map_err(|_| {
        ApplicationLifecycleError::CheckpointDatabase(StorageError::InitializationFailed(
            "database connection lock was poisoned".to_string(),
        ))
    })?;
    checkpoint_wal(&connection).map_err(ApplicationLifecycleError::CheckpointDatabase)
}
