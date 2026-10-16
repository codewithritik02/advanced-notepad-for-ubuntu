use rusqlite::Connection;
use crate::storage::errors::StorageError;
use crate::storage::schema::{
    INITIAL_SCHEMA_SQL, MIGRATION_002_FTS5_SQL, MIGRATION_003_ATTACHMENTS_SCHEMA_SQL,
    MIGRATION_004_TRASH_INDEXES_SQL, NOTE_LIST_INDEXES_SQL,
};

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
    Migration {
        version: 3,
        name: "003_attachments_schema_refinement",
        sql: MIGRATION_003_ATTACHMENTS_SCHEMA_SQL,
    },
    Migration {
        version: 4,
        name: "004_trash_indexes",
        sql: MIGRATION_004_TRASH_INDEXES_SQL,
    },
];

/// Ensures note list composite indexes are present on the database (Phase 13, Task 9)
pub fn ensure_performance_indexes(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(NOTE_LIST_INDEXES_SQL)?;
    Ok(())
}

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

/// Creates an automatic pre-migration snapshot if there are pending migrations on an existing database (Phase 13, Task 18).
pub fn create_pre_migration_snapshot(conn: &Connection, db_path: &std::path::Path) -> Result<Option<std::path::PathBuf>, StorageError> {
    if !db_path.exists() {
        return Ok(None);
    }

    // Check if the database has any applied migrations (table `_migrations` exists)
    let has_tables: bool = conn.query_row(
        "SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name='_migrations'",
        [],
        |r| r.get::<_, i64>(0),
    ).map(|c| c > 0).unwrap_or(false);

    if !has_tables {
        return Ok(None);
    }

    // Check if there are pending migrations
    let mut has_pending = false;
    for migration in MIGRATIONS {
        let is_applied: bool = conn.query_row(
            "SELECT COUNT(1) FROM _migrations WHERE version = ?1",
            [migration.version],
            |row| {
                let count: i64 = row.get(0)?;
                Ok(count > 0)
            },
        ).unwrap_or(false);

        if !is_applied {
            has_pending = true;
            break;
        }
    }

    if !has_pending {
        return Ok(None);
    }

    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let file_name = db_path.file_name().unwrap_or_default().to_string_lossy();
    let snapshot_name = format!("{file_name}.pre_migration.{timestamp}");
    let snapshot_path = db_path.with_file_name(&snapshot_name);

    match crate::storage::services::snapshot::SnapshotManager::create_snapshot(conn, &snapshot_path) {
        Ok(_) => Ok(Some(snapshot_path)),
        Err(e) => {
            eprintln!("[Migration Warning] Failed to create pre-migration snapshot: {e}");
            Err(StorageError::Backup(format!("Pre-migration snapshot failed: {e}")))
        }
    }
}

/// Runs all pending migrations inside atomic transactions in ascending version order.
/// Idempotent: previously applied migrations are skipped, preserving all existing user data.
pub fn run_migrations(conn: &mut Connection) -> Result<(), StorageError> {
    run_migrations_custom(conn, MIGRATIONS)
}

/// Runs all pending migrations inside atomic transactions with optional pre-migration snapshot (Phase 13, Task 18).
pub fn run_migrations_with_safety(conn: &mut Connection, db_path: Option<&std::path::Path>) -> Result<(), StorageError> {
    if let Some(path) = db_path {
        let _ = create_pre_migration_snapshot(conn, path)?;
    }
    run_migrations(conn)
}

/// Runs a custom or registered list of migrations using atomic per-migration transactions.
///
/// Task 67 — Search Migration Failure Safety:
/// If a search migration fails (e.g. FTS5 syntax, extension missing, or corrupted index):
/// - Follows existing migration transaction architecture: rolls back the failed transaction cleanly.
/// - Does NOT corrupt the notes database or delete any notes data.
/// - Database remains completely usable for reading, editing, and creating notes.
/// - Search gracefully falls back to LIKE search.
pub fn run_migrations_custom(conn: &mut Connection, migrations: &[Migration]) -> Result<(), StorageError> {
    ensure_migrations_table(conn)?;

    for migration in migrations {
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
            match tx.execute_batch(migration.sql) {
                Ok(_) => {
                    let now = chrono::Utc::now().to_rfc3339();
                    tx.execute(
                        "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                        rusqlite::params![migration.version, migration.name, now],
                    )?;
                    tx.commit()?;
                }
                Err(err) => {
                    // Task 67: If search migration fails, database remains usable must be the priority.
                    // Follow existing migration transaction architecture: roll back transaction.
                    // Do not corrupt the notes database.
                    let is_search_migration = migration.name.contains("fts") || migration.name.contains("search");
                    let _ = tx.rollback();

                    if is_search_migration {
                        eprintln!(
                            "[Migration Warning] Search migration '{}' (v{}) failed: {}. Transaction rolled back. Notes database remains intact and usable; search will fallback to LIKE search.",
                            migration.name, migration.version, err
                        );
                    } else {
                        return Err(StorageError::Database(err));
                    }
                }
            }
        }
    }

    ensure_performance_indexes(conn)?;

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

    #[test]
    fn test_task_67_search_migration_failure_safety() {
        use crate::storage::repositories::{NoteRepository, SearchRepository};
        use crate::storage::models::CreateNoteDto;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_mig_safety_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");

        // 1. Initialize DB at version 1 (healthy initial schema)
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

        // 2. Insert vital user notes prior to search migration
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Mission Critical Financial Plan".to_string()),
                content: Some("Q4 budget allocation and cash flow analysis.".to_string()),
                ..Default::default()
            },
        ).expect("Create note 1");

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Confidential System Architecture".to_string()),
                content: Some("Zero trust security model and access controls.".to_string()),
                ..Default::default()
            },
        ).expect("Create note 2");

        let count_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_before, 2);

        // 3. Define a failing search migration (simulating corrupt FTS extension / broken FTS migration)
        let failing_search_migration = Migration {
            version: 2,
            name: "002_fts5_search_index",
            sql: "CREATE VIRTUAL TABLE broken_fts USING non_existent_extension_fts(foo); SYNTAX ERROR;",
        };

        // 4. Run migration runner with failing search migration
        // Invariant: Must return Ok(()) so database remains usable and app can start
        let migration_res = run_migrations_custom(&mut conn, &[failing_search_migration]);
        assert!(
            migration_res.is_ok(),
            "Search migration failure must not crash migration runner; priority is database remaining usable"
        );

        // 5. Invariant: Transaction rolled back; _migrations does NOT record version 2
        let v2_applied: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM _migrations WHERE version = 2",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        assert!(!v2_applied, "Failed search migration must NOT be marked as applied in _migrations");

        // 6. Invariant: Database remains completely usable and uncorrupted
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .expect("Integrity check must succeed");
        assert_eq!(integrity, "ok", "Database file must not be corrupted by failed migration");

        // 7. Invariant: All pre-existing user notes are 100% intact
        let count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_after, 2, "Notes must never be deleted or lost on migration failure");

        let fetched_note1 = NoteRepository::get_by_id(&conn, &note1.id).unwrap().expect("Note 1 must exist");
        assert_eq!(fetched_note1.title, "Mission Critical Financial Plan");
        assert_eq!(fetched_note1.content, "Q4 budget allocation and cash flow analysis.");

        let fetched_note2 = NoteRepository::get_by_id(&conn, &note2.id).unwrap().expect("Note 2 must exist");
        assert_eq!(fetched_note2.title, "Confidential System Architecture");

        // 8. Invariant: Database CRUD operations remain functional
        let _note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("New Note After Failed Search Migration".to_string()),
                content: Some("User can continue creating notes without interruption.".to_string()),
                ..Default::default()
            },
        ).expect("Creating notes after search migration failure must succeed");

        let list_notes = NoteRepository::list(&conn, false).unwrap();
        assert_eq!(list_notes.len(), 3, "Database must remain usable for listing notes");

        // 9. Invariant: Search remains functional through fallback
        let search_results = SearchRepository::search(&conn, "financial", None).expect("Search must succeed");
        assert_eq!(search_results.len(), 1, "Search must return results via fallback mechanism");
        assert_eq!(search_results[0].note_id, note1.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_3_attachments_migration_schema_and_integrity() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_mig_att_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");

        // 1. Simulate an existing database that has migrations 1 and 2 applied
        let mut conn = init_connection(&db_path).expect("Initial connection failed");
        ensure_migrations_table(&conn).expect("Ensure migrations table");

        let tx = conn.transaction().expect("Tx");
        tx.execute_batch(INITIAL_SCHEMA_SQL).expect("Apply v1 schema");
        tx.execute_batch(MIGRATION_002_FTS5_SQL).expect("Apply v2 schema");
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (1, '001_initial_schema', ?1)",
            rusqlite::params![now],
        ).expect("Insert migration 1");
        tx.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (2, '002_fts5_search_index', ?1)",
            rusqlite::params![now],
        ).expect("Insert migration 2");

        // Insert a pre-existing note and attachment
        let note_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at)
             VALUES (?1, 'Note with Attachment', 'Content', 'txt', ?2, ?2)",
            [&note_id, &now],
        ).expect("Insert note");

        let att_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, ?2, 'test.pdf', 'note/test.pdf', 'application/pdf', 1024, ?3, ?3)",
            [&att_id, &note_id, &now],
        ).expect("Insert pre-existing attachment");
        tx.commit().expect("Commit pre-migration data");

        // 2. Run migrations (which applies Migration 3)
        run_migrations(&mut conn).expect("Run migrations including v3");

        // 3. Verify Migration 3 was recorded
        let v3_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM _migrations WHERE version = 3",
            [],
            |r| r.get(0),
        ).expect("Query _migrations for v3");
        assert_eq!(v3_count, 1, "Migration 3 must be recorded in _migrations");

        // 4. Verify pre-existing data was preserved
        let fetched_att_name: String = conn.query_row(
            "SELECT file_name FROM attachments WHERE id = ?1",
            [&att_id],
            |r| r.get(0),
        ).expect("Pre-existing attachment must exist");
        assert_eq!(fetched_att_name, "test.pdf");

        // 5. Verify nullable mime_type support (inserting NULL mime_type)
        let att_null_mime_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, ?2, 'unknown_file.bin', 'note/bin.dat', NULL, 2048, ?3, ?3)",
            [&att_null_mime_id, &note_id, &now],
        ).expect("Inserting attachment with NULL mime_type must succeed");

        let retrieved_mime: Option<String> = conn.query_row(
            "SELECT mime_type FROM attachments WHERE id = ?1",
            [&att_null_mime_id],
            |r| r.get(0),
        ).expect("Retrieve null mime");
        assert_eq!(retrieved_mime, None);

        // 6. Verify non-negative file_size constraint rejection
        let invalid_size_res = conn.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, ?2, 'bad.dat', 'note/bad.dat', 'application/octet-stream', -5, ?3, ?3)",
            [uuid::Uuid::new_v4().to_string(), note_id.clone(), now.clone()],
        );
        assert!(invalid_size_res.is_err(), "Negative file_size must be rejected by CHECK constraint");

        // 7. Verify foreign key enforcement
        let invalid_fk_res = conn.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, 'non-existent-note-id', 'bad.dat', 'note/bad.dat', 'application/octet-stream', 100, ?2, ?2)",
            [uuid::Uuid::new_v4().to_string(), now.clone()],
        );
        assert!(invalid_fk_res.is_err(), "Non-existent note_id must be rejected by foreign key constraint");

        // 8. Verify CASCADE deletion on hard note delete
        conn.execute("DELETE FROM notes WHERE id = ?1", [&note_id]).expect("Delete note");
        let remaining_att_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM attachments WHERE note_id = ?1",
            [&note_id],
            |r| r.get(0),
        ).expect("Count attachments");
        assert_eq!(remaining_att_count, 0, "Hard note deletion must cascade to attachments in database");

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_migration_004_trash_indexes_upgrade_and_preservation() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_mig_004_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection");

        // 1. Manually setup DB with v1, v2, v3 applied
        ensure_migrations_table(&conn).expect("Ensure _migrations");
        let now = chrono::Utc::now().to_rfc3339();

        let v1_to_v3 = &MIGRATIONS[..3];
        run_migrations_custom(&mut conn, v1_to_v3).expect("Run migrations 1..=3");

        // Insert active notes, notebook, tag, attachment to ensure zero data loss during upgrade
        let nb_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at) VALUES (?1, 'Work', NULL, ?2, ?2)",
            [&nb_id, &now],
        ).expect("Insert notebook");

        let active_note_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO notes (id, title, content, format, notebook_id, created_at, modified_at, is_favorite, is_pinned, is_deleted, deleted_at)
             VALUES (?1, 'Active Note', 'Content', 'txt', ?2, ?3, ?3, 1, 1, 0, NULL)",
            [&active_note_id, &nb_id, &now],
        ).expect("Insert active note");

        let trashed_note_id = uuid::Uuid::new_v4().to_string();
        let deleted_time = "2026-10-08T12:00:00Z";
        conn.execute(
            "INSERT INTO notes (id, title, content, format, notebook_id, created_at, modified_at, is_favorite, is_pinned, is_deleted, deleted_at)
             VALUES (?1, 'Trashed Note', 'Old content', 'txt', ?2, ?3, ?3, 0, 0, 1, ?4)",
            [&trashed_note_id, &nb_id, &now, deleted_time],
        ).expect("Insert trashed note");

        // 2. Run all migrations (applies Migration 4)
        run_migrations(&mut conn).expect("Run migrations including v4");

        // 3. Verify Migration 4 recorded
        let v4_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM _migrations WHERE version = 4",
            [],
            |r| r.get(0),
        ).expect("Query _migrations for v4");
        assert_eq!(v4_count, 1, "Migration 4 must be recorded in _migrations");

        // 4. Verify indexes exist
        let idx_deleted_at_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type = 'index' AND name = 'idx_notes_deleted_at'",
            [],
            |r| r.get(0),
        ).expect("Check idx_notes_deleted_at");
        assert_eq!(idx_deleted_at_count, 1, "idx_notes_deleted_at must exist");

        let idx_lifecycle_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type = 'index' AND name = 'idx_notes_trash_lifecycle'",
            [],
            |r| r.get(0),
        ).expect("Check idx_notes_trash_lifecycle");
        assert_eq!(idx_lifecycle_count, 1, "idx_notes_trash_lifecycle must exist");

        // 5. Verify all existing data is 100% intact
        let active_title: String = conn.query_row(
            "SELECT title FROM notes WHERE id = ?1 AND is_deleted = 0",
            [&active_note_id],
            |r| r.get(0),
        ).expect("Active note must remain intact");
        assert_eq!(active_title, "Active Note");

        let trashed_title: String = conn.query_row(
            "SELECT title FROM notes WHERE id = ?1 AND is_deleted = 1",
            [&trashed_note_id],
            |r| r.get(0),
        ).expect("Trashed note must remain intact");
        assert_eq!(trashed_title, "Trashed Note");

        // 6. Test fresh database initialization with all migrations
        let fresh_temp_dir = std::env::temp_dir().join(format!("pn_test_fresh_mig_{}", uuid::Uuid::new_v4()));
        let fresh_db_path = fresh_temp_dir.join("fresh.sqlite");
        let mut fresh_conn = init_connection(&fresh_db_path).expect("Fresh DB init");
        run_migrations(&mut fresh_conn).expect("Run migrations on fresh DB");

        let total_migrations: i64 = fresh_conn.query_row(
            "SELECT COUNT(1) FROM _migrations",
            [],
            |r| r.get(0),
        ).expect("Count migrations");
        assert_eq!(total_migrations, 4, "Fresh database must have all 4 migrations applied");

        drop(conn);
        drop(fresh_conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&fresh_temp_dir);
    }

    #[test]
    fn test_note_list_performance_indexes_ensured() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_perf_idx_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection");

        // 1. Run migrations 1..=4
        run_migrations(&mut conn).expect("Run migrations");

        // 2. Verify composite indexes exist
        for idx in ["idx_notes_list_sort", "idx_notes_notebook_list", "idx_notes_favorite_list"] {
            let count: i64 = conn.query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [idx],
                |r| r.get(0),
            ).unwrap_or(0);
            assert_eq!(count, 1, "Index {idx} must exist after ensure_performance_indexes");
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_pre_migration_snapshot_creation() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_pre_mig_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection");

        // 1. Run subset of migrations (e.g. migration 1 and 2 only)
        run_migrations_custom(&mut conn, &MIGRATIONS[..2]).expect("Run initial migrations");

        // 2. create_pre_migration_snapshot should create a snapshot because migration 3 and 4 are pending
        let snapshot_path = create_pre_migration_snapshot(&conn, &db_path).expect("Create pre-migration snapshot");
        assert!(snapshot_path.is_some(), "Must create a snapshot when pending migrations exist");
        let path = snapshot_path.unwrap();
        assert!(path.exists(), "Snapshot file must exist on disk");
        assert!(path.file_name().unwrap().to_string_lossy().contains(".pre_migration."));

        // 3. Now run all migrations with safety
        run_migrations_with_safety(&mut conn, Some(&db_path)).expect("Run remaining migrations");

        // 4. Running again when no pending migrations exist should return None
        let second_snapshot = create_pre_migration_snapshot(&conn, &db_path).expect("Check when no migrations pending");
        assert!(second_snapshot.is_none(), "Must not create snapshot when no migrations are pending");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}


