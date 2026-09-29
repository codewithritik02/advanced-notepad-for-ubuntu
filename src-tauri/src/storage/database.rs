use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;
use crate::storage::errors::StorageError;

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

/// Opens an existing SQLite database or creates a new one at `db_path`.
/// Applies required pragmas: foreign keys, WAL journal mode, synchronous normal, and busy timeout.
pub fn init_connection(db_path: &Path) -> Result<Connection, StorageError> {
    if let Some(parent) = db_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let conn = Connection::open(db_path)?;

    // Configure SQLite pragmas for local desktop reliability and performance
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 5000;",
    )?;

    // Validate database connection is functional
    let check: i32 = conn.query_row("SELECT 1", [], |row| row.get(0))?;
    if check != 1 {
        return Err(StorageError::InitializationFailed(
            "SQLite sanity check failed".to_string(),
        ));
    }

    Ok(conn)
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
        assert_eq!(val, "initial value", "Existing data must survive reconnection");

        // Clean up
        drop(conn2);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
