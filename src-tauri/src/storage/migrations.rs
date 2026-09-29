use rusqlite::Connection;
use crate::storage::errors::StorageError;
use crate::storage::schema::INITIAL_SCHEMA_SQL;

/// Represents a single discrete database migration step
pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    pub sql: &'static str,
}

/// Registry of all defined application migrations in strict sequential order
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "001_initial_schema",
        sql: INITIAL_SCHEMA_SQL,
    },
];

/// Creates the `_migrations` tracking table if it does not already exist.
fn ensure_migrations_table(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// Runs all pending migrations inside atomic transactions in ascending version order.
/// Idempotent: previously applied migrations are skipped, preserving all existing user data.
pub fn run_migrations(conn: &mut Connection) -> Result<(), StorageError> {
    ensure_migrations_table(conn)?;

    for migration in MIGRATIONS {
        let is_applied: bool = conn.query_row(
            "SELECT COUNT(1) FROM _migrations WHERE version = ?1",
            [migration.version],
            |row| {
                let count: i64 = row.get(0)?;
                Ok(count > 0)
            },
        )?;

        if !is_applied {
            let tx = conn.transaction()?;
            tx.execute_batch(migration.sql)?;
            
            let now = chrono::Utc::now().to_rfc3339();
            tx.execute(
                "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![migration.version, migration.name, now],
            )?;

            tx.commit()?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;

    #[test]
    fn test_migration_runner_fresh_and_idempotent() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_mig_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");

        // 1. Fresh database run
        let mut conn = init_connection(&db_path).expect("Initial connection failed");
        run_migrations(&mut conn).expect("First migration run should succeed");

        // Verify version 1 was recorded in _migrations
        let count: i64 = conn
            .query_row("SELECT COUNT(1) FROM _migrations WHERE version = 1", [], |r| r.get(0))
            .expect("Should query _migrations");
        assert_eq!(count, 1, "Migration 1 must be recorded");

        // Insert a record into notes to prove data persistence across subsequent migration runs
        let note_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at)
             VALUES (?1, 'Migration Test Note', 'Should survive second migration run', 'txt', ?2, ?2)",
            [&note_id, &now],
        ).expect("Insert note failed");

        // 2. Second migration run on existing database
        run_migrations(&mut conn).expect("Second migration run should succeed without error");

        // Verify note still exists intact
        let retrieved_title: String = conn
            .query_row("SELECT title FROM notes WHERE id = ?1", [&note_id], |r| r.get(0))
            .expect("Note must still exist");
        assert_eq!(retrieved_title, "Migration Test Note", "Existing data must remain intact");

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
