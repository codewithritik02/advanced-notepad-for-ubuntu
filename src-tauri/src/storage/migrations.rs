use rusqlite::Connection;
use crate::storage::errors::StorageError;
use crate::storage::schema::{INITIAL_SCHEMA_SQL, MIGRATION_002_FTS5_SQL};

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
    Migration {
        version: 2,
        name: "002_fts5_search_index",
        sql: MIGRATION_002_FTS5_SQL,
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

    #[test]
    fn test_task_33_fts5_migration_creates_notes_fts() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_mig_fts5_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");

        // 1. Simulate an existing database that was on version 1 only
        let mut conn = init_connection(&db_path).expect("Initial connection failed");
        ensure_migrations_table(&conn).expect("Ensure migrations table");
        
        let tx = conn.transaction().expect("Tx");
        tx.execute_batch(INITIAL_SCHEMA_SQL).expect("Version 1 schema");
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (1, '001_initial_schema', ?1)",
            rusqlite::params![now],
        ).expect("Insert migration 1");
        tx.commit().expect("Commit v1");

        // Insert pre-existing note while at version 1
        let note_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at)
             VALUES (?1, 'Pre-existing Note Title', 'Pre-existing note body content', 'txt', ?2, ?2)",
            [&note_id, &now],
        ).expect("Insert note under v1");

        // Verify FTS table does NOT exist yet
        let has_fts_before: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(!has_fts_before, "notes_fts must not exist before migration 002");

        // 2. Run migration runner to apply pending migrations (applies 002_fts5_search_index)
        run_migrations(&mut conn).expect("Applying migration 002 must succeed");

        // 3. Verify version 2 is recorded in _migrations
        let v2_count: i64 = conn
            .query_row("SELECT COUNT(1) FROM _migrations WHERE version = 2", [], |r| r.get(0))
            .expect("Should query _migrations for v2");
        assert_eq!(v2_count, 1, "Migration 2 must be recorded in _migrations");

        // 4. Verify notes_fts virtual table exists
        let has_fts_after: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_fts_after, "notes_fts virtual table must exist after migration 002");

        // 5. Existing note must still exist intact
        let retrieved_title: String = conn
            .query_row("SELECT title FROM notes WHERE id = ?1", [&note_id], |r| r.get(0))
            .expect("Note must exist");
        assert_eq!(retrieved_title, "Pre-existing Note Title");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_34_initial_fts_population_makes_existing_notes_searchable() {
        use crate::storage::repositories::SearchRepository;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_mig_fts5_pop_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");

        // 1. Initialize DB at version 1 (before FTS5 existed)
        let mut conn = init_connection(&db_path).expect("Initial connection failed");
        ensure_migrations_table(&conn).expect("Ensure migrations table");

        let tx = conn.transaction().expect("Tx");
        tx.execute_batch(INITIAL_SCHEMA_SQL).expect("Version 1 schema");
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (1, '001_initial_schema', ?1)",
            rusqlite::params![now],
        ).expect("Insert migration 1");
        tx.commit().expect("Commit v1");

        // 2. Populate existing notes before migration 002
        let id_a = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_deleted)
             VALUES (?1, 'Legacy Project Architecture', 'Comprehensive roadmap and system design.', 'txt', ?2, ?2, 0)",
            [&id_a, &now],
        ).expect("Insert note A");

        let id_b = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_deleted)
             VALUES (?1, 'Weekly Sync Log', 'Discussion regarding legacy architecture and scalability.', 'txt', ?2, ?2, 0)",
            [&id_b, &now],
        ).expect("Insert note B");

        let id_c_deleted = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_deleted)
             VALUES (?1, 'Deleted Legacy Draft', 'Obsolete legacy draft content.', 'txt', ?2, ?2, 1)",
            [&id_c_deleted, &now],
        ).expect("Insert deleted note C");

        // 3. Run migration 002 (Task 34: populates FTS index from existing notes)
        run_migrations(&mut conn).expect("Run migrations 002");

        // 4. Perform search query for 'legacy'
        let results = SearchRepository::search(&conn, "legacy", None).expect("Search must succeed");

        // Verify existing active notes A & B are immediately searchable via FTS5
        assert_eq!(results.len(), 2, "Both active pre-existing notes must be found");
        let result_ids: Vec<String> = results.iter().map(|r| r.note_id.clone()).collect();
        assert!(result_ids.contains(&id_a), "Note A (title match) must be returned");
        assert!(result_ids.contains(&id_b), "Note B (content match) must be returned");
        assert!(!result_ids.contains(&id_c_deleted), "Deleted note must be excluded");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}


