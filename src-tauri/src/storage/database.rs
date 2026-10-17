use crate::storage::errors::StorageError;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;

/// Default busy timeout for SQLite connections (in milliseconds).
/// 5000ms gives concurrent background operations (like snapshotting or WAL checkpointing)
/// adequate time to release locks without throwing immediate SQLITE_BUSY to UI commands.
pub const DEFAULT_BUSY_TIMEOUT_MS: u32 = 5000;

/// Default SQLite journal mode: WAL (Write-Ahead Logging).
/// In WAL mode, SQLite allows concurrent readers without blocking writers,
/// and writers do not block readers. This is critical for desktop UI responsiveness.
pub const DEFAULT_JOURNAL_MODE: &str = "WAL";

/// Default SQLite synchronous mode: NORMAL.
/// In WAL mode, synchronous=NORMAL guarantees full ACID durability and zero corruption
/// across OS crashes, while eliminating per-transaction fsync stalls.
pub const DEFAULT_SYNCHRONOUS: &str = "NORMAL";

/// Foreign keys enforcement: ON.
/// SQLite defaults foreign keys to OFF. Setting this to ON ensures all relational
/// cascades (e.g. note_tags, attachments, notebook hierarchy) are strictly enforced.
pub const DEFAULT_FOREIGN_KEYS: &str = "ON";

/// Cache size in kibibytes (-2000 = ~2MB).
pub const DEFAULT_CACHE_SIZE_KB: i32 = -2000;

/// Central thread-safe Database state for Tauri application
pub struct Database {
    pub conn: Mutex<Connection>,
}

impl Database {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }
}

/// Structured SQLite Configuration for desktop reliability and performance
#[derive(Debug, Clone)]
pub struct SqliteConfig {
    pub busy_timeout_ms: u32,
    pub journal_mode: &'static str,
    pub synchronous: &'static str,
    pub foreign_keys: bool,
    pub cache_size_kb: i32,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            busy_timeout_ms: DEFAULT_BUSY_TIMEOUT_MS,
            journal_mode: DEFAULT_JOURNAL_MODE,
            synchronous: DEFAULT_SYNCHRONOUS,
            foreign_keys: true,
            cache_size_kb: DEFAULT_CACHE_SIZE_KB,
        }
    }
}

/// Applies standard application pragmas to a SQLite connection.
pub fn apply_pragmas(conn: &Connection, config: &SqliteConfig) -> Result<(), StorageError> {
    let fk = if config.foreign_keys { "ON" } else { "OFF" };
    conn.execute_batch(&format!(
        "PRAGMA foreign_keys = {};
         PRAGMA journal_mode = {};
         PRAGMA synchronous = {};
         PRAGMA busy_timeout = {};
         PRAGMA cache_size = {};",
        fk, config.journal_mode, config.synchronous, config.busy_timeout_ms, config.cache_size_kb
    ))?;
    Ok(())
}

/// Opens an existing SQLite database or creates a new one at `db_path` using default config.
pub fn init_connection(db_path: &Path) -> Result<Connection, StorageError> {
    init_connection_with_config(db_path, &SqliteConfig::default())
}

/// Opens an existing SQLite database or creates a new one at `db_path` with custom config.
///
/// Reliability Guarantee:
/// If `db_path` exists on disk but is invalid or corrupted, this function will FAIL
/// and return a typed StorageError without deleting, truncating, or replacing the file.
/// Original user files are strictly preserved for recovery.
pub fn init_connection_with_config(
    db_path: &Path,
    config: &SqliteConfig,
) -> Result<Connection, StorageError> {
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let file_existed_before_open = db_path.is_file();

    let conn = Connection::open(db_path)?;

    // Apply required pragmas
    apply_pragmas(&conn, config)?;

    // Validate database connection is functional
    let check: i32 = conn.query_row("SELECT 1", [], |row| row.get(0))?;
    if check != 1 {
        return Err(StorageError::InitializationFailed(
            "SQLite sanity check failed".to_string(),
        ));
    }

    // If opening an existing database, run quick_check to verify page header integrity
    if file_existed_before_open {
        let quick_check: Result<String, _> =
            conn.query_row("PRAGMA quick_check(1);", [], |r| r.get(0));
        match quick_check {
            Ok(ref status) if status.to_lowercase() == "ok" => {}
            Ok(ref status) => {
                return Err(StorageError::InitializationFailed(format!(
                    "Database file at '{}' failed quick integrity check (corrupt): {}. Original file preserved.",
                    db_path.display(),
                    status
                )));
            }
            Err(e) => {
                return Err(StorageError::InitializationFailed(format!(
                    "Database file at '{}' is unreadable (corrupt): {}. Original file preserved.",
                    db_path.display(),
                    e
                )));
            }
        }
    }

    Ok(conn)
}

/// Isolates a corrupted database file by renaming it to `database.sqlite.corrupt.<timestamp>`
/// along with any associated WAL/SHM files, preserving it safely without deletion or overwrite (Phase 13, Task 18).
pub fn isolate_corrupted_database(db_path: &Path) -> Result<PathBuf, StorageError> {
    if !db_path.exists() {
        return Err(StorageError::NotFound(
            "Database file does not exist to isolate".to_string(),
        ));
    }

    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let corrupt_name = format!(
        "{}.corrupt.{timestamp}",
        db_path.file_name().unwrap().to_string_lossy()
    );
    let corrupt_path = db_path.with_file_name(&corrupt_name);

    // Atomically rename database file
    std::fs::rename(db_path, &corrupt_path)?;

    // Also isolate WAL and SHM if present
    let wal_path = db_path.with_file_name(format!(
        "{}-wal",
        db_path.file_name().unwrap().to_string_lossy()
    ));
    if wal_path.exists() {
        let corrupt_wal = db_path.with_file_name(format!(
            "{}-wal.corrupt.{timestamp}",
            db_path.file_name().unwrap().to_string_lossy()
        ));
        let _ = std::fs::rename(wal_path, corrupt_wal);
    }

    let shm_path = db_path.with_file_name(format!(
        "{}-shm",
        db_path.file_name().unwrap().to_string_lossy()
    ));
    if shm_path.exists() {
        let corrupt_shm = db_path.with_file_name(format!(
            "{}-shm.corrupt.{timestamp}",
            db_path.file_name().unwrap().to_string_lossy()
        ));
        let _ = std::fs::rename(shm_path, corrupt_shm);
    }

    Ok(corrupt_path)
}

/// Triggers a WAL checkpoint on the given connection to flush committed pages to disk.
pub fn checkpoint_wal(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);")?;
    Ok(())
}

/// Executes an operation with bounded retry and exponential backoff on transient SQLite contention.
///
/// SAFETY INVARIANT:
/// This retry helper must ONLY be used with idempotent operations or fresh transactions.
/// NEVER retry partially executed non-idempotent operations.
pub fn retry_on_busy<T, F>(
    max_attempts: usize,
    initial_backoff: Duration,
    mut op: F,
) -> Result<T, StorageError>
where
    F: FnMut() -> Result<T, StorageError>,
{
    let attempts = max_attempts.max(1);
    let mut current_backoff = initial_backoff;

    for attempt in 1..=attempts {
        match op() {
            Ok(val) => return Ok(val),
            Err(err) => {
                // Only retry if error is transient / retryable (e.g. SQLITE_BUSY or SQLITE_LOCKED)
                // and we have remaining attempts.
                if err.is_retryable() && attempt < attempts {
                    sleep(current_backoff);
                    // Exponential backoff capped at 2 seconds
                    current_backoff = (current_backoff * 2).min(Duration::from_millis(2000));
                } else {
                    return Err(err);
                }
            }
        }
    }

    Err(StorageError::InitializationFailed(
        "Max retry attempts exhausted for busy operation".to_string(),
    ))
}

/// Helper to query an integer pragma value (e.g. `busy_timeout`, `foreign_keys`).
pub fn query_pragma_int(conn: &Connection, pragma_name: &str) -> Result<i64, StorageError> {
    let val: i64 = conn.query_row(&format!("PRAGMA {pragma_name};"), [], |r| r.get(0))?;
    Ok(val)
}

/// Helper to query a string pragma value (e.g. `journal_mode`, `synchronous`).
pub fn query_pragma_string(conn: &Connection, pragma_name: &str) -> Result<String, StorageError> {
    let val: String = conn.query_row(&format!("PRAGMA {pragma_name};"), [], |r| r.get(0))?;
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_connection_fresh_and_reopen() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_db_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");

        // 1. Initial creation
        let conn = init_connection(&db_path).expect("Initial connection should succeed");
        assert!(db_path.exists(), "database.sqlite must exist on disk");

        // Verify foreign keys pragma is ON
        let fk: i32 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .expect("Should query foreign_keys");
        assert_eq!(fk, 1, "Foreign keys must be enabled");

        // Verify journal mode is WAL
        let jm: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .expect("Should query journal_mode");
        assert_eq!(jm.to_lowercase(), "wal", "Journal mode must be WAL");

        // Create a test table and write a record
        conn.execute("CREATE TABLE test_data (id TEXT PRIMARY KEY, val TEXT)", [])
            .expect("Create table failed");
        conn.execute(
            "INSERT INTO test_data (id, val) VALUES (?1, ?2)",
            ["1", "initial value"],
        )
        .expect("Insert failed");
        drop(conn);

        // 2. Reopen existing database
        let conn2 = init_connection(&db_path).expect("Reopening connection should succeed");
        let val: String = conn2
            .query_row("SELECT val FROM test_data WHERE id = ?1", ["1"], |r| {
                r.get(0)
            })
            .expect("Should retrieve existing data");
        assert_eq!(
            val, "initial value",
            "Existing data must survive reconnection"
        );

        // Clean up
        drop(conn2);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_sqlite_pragmas_verification() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_pragmas_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");

        let conn = init_connection(&db_path).expect("Connection should succeed");

        let fk = query_pragma_int(&conn, "foreign_keys").expect("foreign_keys pragma");
        assert_eq!(fk, 1, "Foreign keys must be 1 (ON)");

        let jm = query_pragma_string(&conn, "journal_mode").expect("journal_mode pragma");
        assert_eq!(jm.to_lowercase(), "wal", "Journal mode must be wal");

        let busy = query_pragma_int(&conn, "busy_timeout").expect("busy_timeout pragma");
        assert_eq!(busy, 5000, "Busy timeout must be 5000ms");

        // synchronous NORMAL in SQLite pragma is reported as 1 (OFF=0, NORMAL=1, FULL=2, EXTRA=3)
        let sync_mode = query_pragma_int(&conn, "synchronous").expect("synchronous pragma");
        assert_eq!(sync_mode, 1, "Synchronous mode must be 1 (NORMAL)");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_foreign_keys_enforcement_in_action() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_fk_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");

        let conn = init_connection(&db_path).expect("Connection should succeed");

        conn.execute_batch(
            "CREATE TABLE parent (id TEXT PRIMARY KEY NOT NULL);
             CREATE TABLE child (
                 id TEXT PRIMARY KEY NOT NULL,
                 parent_id TEXT NOT NULL,
                 FOREIGN KEY (parent_id) REFERENCES parent(id) ON DELETE CASCADE
             );",
        )
        .expect("Tables creation failed");

        // Insert valid parent
        conn.execute("INSERT INTO parent (id) VALUES ('p1')", [])
            .expect("Parent insert");

        // Insert valid child referencing parent
        conn.execute("INSERT INTO child (id, parent_id) VALUES ('c1', 'p1')", [])
            .expect("Child insert");

        // Attempt to insert invalid child referencing nonexistent parent
        let res = conn.execute(
            "INSERT INTO child (id, parent_id) VALUES ('c2', 'nonexistent')",
            [],
        );
        assert!(
            res.is_err(),
            "Foreign key constraint must prevent invalid relationship"
        );

        // Deleting parent should cascade delete child
        conn.execute("DELETE FROM parent WHERE id = 'p1'", [])
            .expect("Delete parent");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM child", [], |r| r.get(0))
            .expect("Count children");
        assert_eq!(
            count, 0,
            "Cascade delete must remove child when parent is deleted"
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_database_open_failure_preserves_corrupted_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_corrupt_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("database.sqlite");

        // Write non-SQLite garbage bytes
        let garbage = b"NOT A VALID SQLITE DATABASE FILE - CORRUPTION SIMULATION";
        std::fs::write(&db_path, garbage).expect("Write garbage file");

        // Attempting to init connection must fail
        let res = init_connection(&db_path);
        assert!(res.is_err(), "Opening invalid file must fail");

        // CRITICAL INVARIANT: The corrupted file MUST NOT be deleted or overwritten
        assert!(
            db_path.exists(),
            "Corrupted database file must be preserved on disk"
        );
        let on_disk_bytes = std::fs::read(&db_path).expect("Read database file");
        assert_eq!(
            on_disk_bytes, garbage,
            "Corrupted database content must remain unmodified"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_retry_on_busy_retries_transient_and_aborts_on_fatal() {
        // 1. Transient error that succeeds on 3rd attempt
        let mut attempts = 0;
        let res = retry_on_busy(3, Duration::from_millis(5), || {
            attempts += 1;
            if attempts < 3 {
                Err(StorageError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error {
                        code: rusqlite::ErrorCode::DatabaseBusy,
                        extended_code: 5,
                    },
                    Some("database is locked".to_string()),
                )))
            } else {
                Ok(42)
            }
        });
        assert_eq!(res.unwrap(), 42);
        assert_eq!(
            attempts, 3,
            "Should have retried until 3rd attempt succeeded"
        );

        // 2. Non-retryable error aborts immediately on first attempt
        let mut attempts_fatal = 0;
        let res_fatal: Result<i32, StorageError> =
            retry_on_busy(5, Duration::from_millis(5), || {
                attempts_fatal += 1;
                Err(StorageError::NotFound("Note not found".to_string()))
            });
        assert!(res_fatal.is_err());
        assert_eq!(
            attempts_fatal, 1,
            "Non-retryable error must fail fast on attempt 1"
        );
    }

    #[test]
    fn test_checkpoint_wal_execution() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_wal_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");

        let conn = init_connection(&db_path).expect("Connection should succeed");
        conn.execute("CREATE TABLE foo (x TEXT)", []).unwrap();
        conn.execute("INSERT INTO foo VALUES ('bar')", []).unwrap();

        let checkpoint_res = checkpoint_wal(&conn);
        assert!(checkpoint_res.is_ok(), "WAL checkpoint must succeed");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_isolate_corrupted_database() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_isolate_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("database.sqlite");
        let wal_path = temp_dir.join("database.sqlite-wal");
        let shm_path = temp_dir.join("database.sqlite-shm");

        let garbage = b"CORRUPTED BYTES";
        std::fs::write(&db_path, garbage).unwrap();
        std::fs::write(&wal_path, b"WAL BYTES").unwrap();
        std::fs::write(&shm_path, b"SHM BYTES").unwrap();

        let isolated_path = isolate_corrupted_database(&db_path).expect("Isolation must succeed");
        assert!(isolated_path.exists());
        assert!(isolated_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".corrupt."));
        assert_eq!(std::fs::read(&isolated_path).unwrap(), garbage);

        // Original database file must no longer be at `database.sqlite`
        assert!(!db_path.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
