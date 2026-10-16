mod common;

use common::TestAppDir;
use personal_notepad_lib::storage::{
    current_schema_version, init_connection, init_connection_with_config, run_migrations,
    run_migrations_custom, validate_schema_version_compatibility, CreateNoteDto, ErrorCategory,
    Migration, NoteRepository, SqliteConfig, StorageError, LATEST_SCHEMA_VERSION, MIGRATIONS,
};
use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::fs;
use std::time::{Duration, Instant};

fn object_names(connection: &Connection, object_type: &str) -> HashSet<String> {
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = ?1")
        .unwrap();
    statement
        .query_map([object_type], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn column_names(connection: &Connection, table: &str) -> HashSet<String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    statement
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn assert_database_healthy(connection: &Connection) {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    let foreign_key_violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(foreign_key_violations, 0);
}

#[test]
fn fresh_database_has_the_complete_schema_constraints_indexes_and_metadata() {
    let fixture = TestAppDir::with_label("schema-contract");
    let connection = fixture.open_migrated_database();

    let tables = object_names(&connection, "table");
    for table in [
        "_migrations",
        "notebooks",
        "notes",
        "tags",
        "note_tags",
        "attachments",
        "settings",
        "notes_fts",
    ] {
        assert!(tables.contains(table), "missing expected table {table}");
    }

    assert_eq!(
        column_names(&connection, "notes"),
        [
            "id",
            "title",
            "content",
            "format",
            "notebook_id",
            "created_at",
            "modified_at",
            "is_favorite",
            "is_pinned",
            "is_deleted",
            "deleted_at",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );
    assert_eq!(
        column_names(&connection, "attachments"),
        [
            "id",
            "note_id",
            "file_name",
            "relative_path",
            "mime_type",
            "file_size",
            "created_at",
            "modified_at",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );

    let indexes = object_names(&connection, "index");
    for index in [
        "idx_notebooks_parent_id",
        "idx_notes_modified_at",
        "idx_notes_notebook_id",
        "idx_notes_is_deleted",
        "idx_notes_is_favorite",
        "idx_notes_list_sort",
        "idx_notes_notebook_list",
        "idx_notes_favorite_list",
        "idx_note_tags_tag_id",
        "idx_attachments_note_id",
        "idx_notes_deleted_at",
        "idx_notes_trash_lifecycle",
    ] {
        assert!(indexes.contains(index), "missing expected index {index}");
    }

    let triggers = object_names(&connection, "trigger");
    for trigger in ["notes_fts_ai", "notes_fts_ad", "notes_fts_au"] {
        assert!(
            triggers.contains(trigger),
            "missing expected trigger {trigger}"
        );
    }

    let recorded: Vec<(i32, String)> = connection
        .prepare("SELECT version, name FROM _migrations ORDER BY version")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let expected: Vec<_> = MIGRATIONS
        .iter()
        .map(|migration| (migration.version, migration.name.to_string()))
        .collect();
    assert_eq!(recorded, expected);
    assert_eq!(
        current_schema_version(&connection).unwrap(),
        LATEST_SCHEMA_VERSION
    );
    assert_eq!(
        validate_schema_version_compatibility(&connection).unwrap(),
        4
    );
    assert_database_healthy(&connection);
}

#[test]
fn every_supported_historical_schema_upgrades_to_latest_without_data_loss() {
    for historical_version in 0..LATEST_SCHEMA_VERSION {
        let fixture = TestAppDir::with_label(&format!("migration-v{historical_version}"));
        let mut connection = init_connection(&fixture.paths.database_file).unwrap();
        if historical_version > 0 {
            run_migrations_custom(&mut connection, &MIGRATIONS[..historical_version as usize])
                .unwrap();
        }

        let seeded_ids = if historical_version > 0 {
            let suffix = historical_version.to_string();
            let notebook_id = format!("notebook-v{suffix}");
            let note_id = format!("note-v{suffix}");
            let tag_id = format!("tag-v{suffix}");
            let attachment_id = format!("attachment-v{suffix}");
            let now = "2026-10-16T10:00:00Z";
            connection
                .execute(
                    "INSERT INTO notebooks (id, name, created_at, modified_at) VALUES (?1, ?2, ?3, ?3)",
                    params![notebook_id, format!("Legacy notebook v{suffix}"), now],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO notes (id, title, content, format, notebook_id, created_at, modified_at)
                     VALUES (?1, ?2, ?3, 'txt', ?4, ?5, ?5)",
                    params![
                        note_id,
                        format!("Legacy note v{suffix}"),
                        format!("Preserved migration payload v{suffix} ✅"),
                        notebook_id,
                        now
                    ],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
                    params![tag_id, format!("legacy-v{suffix}"), now],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
                    params![note_id, tag_id],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO attachments
                     (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
                     VALUES (?1, ?2, 'legacy.txt', ?3, 'text/plain', 17, ?4, ?4)",
                    params![attachment_id, note_id, format!("{note_id}/legacy.txt"), now],
                )
                .unwrap();
            Some((note_id, tag_id, attachment_id, suffix))
        } else {
            None
        };

        personal_notepad_lib::storage::migrations::run_migrations_with_safety(
            &mut connection,
            Some(&fixture.paths.database_file),
        )
        .unwrap();
        assert_eq!(
            current_schema_version(&connection).unwrap(),
            LATEST_SCHEMA_VERSION
        );

        if let Some((note_id, tag_id, attachment_id, suffix)) = seeded_ids {
            let persisted: (String, String) = connection
                .query_row(
                    "SELECT title, content FROM notes WHERE id = ?1",
                    [&note_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(persisted.0, format!("Legacy note v{suffix}"));
            assert_eq!(
                persisted.1,
                format!("Preserved migration payload v{suffix} ✅")
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM note_tags WHERE note_id = ?1 AND tag_id = ?2",
                        params![note_id, tag_id],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap(),
                1
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT file_size FROM attachments WHERE id = ?1",
                        [attachment_id],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap(),
                17
            );
        }
        assert_database_healthy(&connection);
    }
}

#[test]
fn foreign_keys_unique_keys_checks_and_missing_records_are_enforced() {
    let fixture = TestAppDir::with_label("constraints");
    let connection = fixture.open_migrated_database();
    let now = "2026-10-16T11:00:00Z";

    connection
        .execute(
            "INSERT INTO notebooks (id, name, created_at, modified_at) VALUES ('parent', 'Parent', ?1, ?1)",
            [now],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at)
             VALUES ('child', 'Child', 'parent', ?1, ?1)",
            [now],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO notes (id, title, content, format, notebook_id, created_at, modified_at)
             VALUES ('note', 'Title', 'Body', 'txt', 'child', ?1, ?1)",
            [now],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO tags (id, name, created_at) VALUES ('tag', 'unique-name', ?1)",
            [now],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO note_tags (note_id, tag_id) VALUES ('note', 'tag')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO attachments
             (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES ('attachment', 'note', 'a.txt', 'note/a.txt', NULL, 1, ?1, ?1)",
            [now],
        )
        .unwrap();

    assert!(connection
        .execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at)
             VALUES ('note', 'Duplicate', '', 'txt', ?1, ?1)",
            [now],
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO tags (id, name, created_at) VALUES ('tag-2', 'unique-name', ?1)",
            [now],
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO note_tags (note_id, tag_id) VALUES ('missing', 'tag')",
            []
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO attachments
             (id, note_id, file_name, relative_path, file_size, created_at, modified_at)
             VALUES ('negative', 'note', 'bad', 'note/bad', -1, ?1, ?1)",
            [now],
        )
        .is_err());
    assert!(NoteRepository::get_by_id(&connection, "missing-note")
        .unwrap()
        .is_none());
    assert!(NoteRepository::update(&connection, "missing-note", Default::default()).is_err());

    connection
        .execute("DELETE FROM notebooks WHERE id = 'parent'", [])
        .unwrap();
    let child_parent: Option<String> = connection
        .query_row(
            "SELECT parent_id FROM notebooks WHERE id = 'child'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        child_parent, None,
        "parent deletion must SET NULL on child notebook"
    );

    connection
        .execute("DELETE FROM notebooks WHERE id = 'child'", [])
        .unwrap();
    let note_notebook: Option<String> = connection
        .query_row(
            "SELECT notebook_id FROM notes WHERE id = 'note'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        note_notebook, None,
        "notebook deletion must SET NULL on notes"
    );

    connection
        .execute("DELETE FROM notes WHERE id = 'note'", [])
        .unwrap();
    for (table, column) in [("note_tags", "note_id"), ("attachments", "note_id")] {
        let count: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE {column} = 'note'"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "note deletion must cascade through {table}");
    }
    assert_database_healthy(&connection);
}

#[test]
fn failed_migration_rolls_back_its_schema_changes_and_preserves_existing_rows() {
    let fixture = TestAppDir::with_label("migration-rollback");
    let mut connection = fixture.open_migrated_database();
    let note = NoteRepository::create(
        &connection,
        CreateNoteDto {
            title: Some("Before failed migration".to_string()),
            content: Some("Must remain intact".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let failing = [Migration {
        version: 5,
        name: "005_injected_failure",
        sql: "CREATE TABLE should_rollback (id TEXT PRIMARY KEY); INSERT INTO should_rollback VALUES ('x'); INVALID SQL;",
    }];

    assert!(run_migrations_custom(&mut connection, &failing).is_err());
    assert!(!object_names(&connection, "table").contains("should_rollback"));
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM _migrations WHERE version = 5",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        NoteRepository::get_by_id(&connection, &note.id)
            .unwrap()
            .unwrap()
            .content,
        "Must remain intact"
    );
    assert_database_healthy(&connection);
}

#[test]
fn unsupported_future_schema_is_rejected_before_mutation() {
    let fixture = TestAppDir::with_label("future-schema");
    let mut connection = fixture.open_migrated_database();
    let note = NoteRepository::create(
        &connection,
        CreateNoteDto {
            title: Some("Future-owned data".to_string()),
            content: Some("Do not downgrade or replace".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    connection
        .execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (999, 'future_schema', ?1)",
            ["2030-01-01T00:00:00Z"],
        )
        .unwrap();

    let error = personal_notepad_lib::storage::migrations::run_migrations_with_safety(
        &mut connection,
        Some(&fixture.paths.database_file),
    )
    .expect_err("future schema must be rejected");
    assert!(matches!(error, StorageError::Migration(_)));
    assert_eq!(error.category(), ErrorCategory::UnsupportedFormat);
    assert!(error
        .to_string()
        .contains("Unsupported database schema version 999"));
    assert_eq!(current_schema_version(&connection).unwrap(), 999);
    assert!(fs::read_dir(&fixture.dir)
        .unwrap()
        .filter_map(Result::ok)
        .all(|entry| !entry
            .file_name()
            .to_string_lossy()
            .contains(".pre_migration.")));
    assert_eq!(
        NoteRepository::get_by_id(&connection, &note.id)
            .unwrap()
            .unwrap()
            .content,
        "Do not downgrade or replace"
    );
    assert!(!object_names(&connection, "table").contains("should_rollback"));
    assert_database_healthy(&connection);
}

#[test]
fn injected_transaction_failure_rolls_back_all_related_writes() {
    let fixture = TestAppDir::with_label("transaction-rollback");
    let mut connection = fixture.open_migrated_database();
    let transaction = connection.transaction().unwrap();
    let now = "2026-10-16T12:00:00Z";
    transaction
        .execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at)
             VALUES ('atomic-note', 'Atomic', 'rollback me', 'txt', ?1, ?1)",
            [now],
        )
        .unwrap();
    transaction
        .execute(
            "INSERT INTO tags (id, name, created_at) VALUES ('atomic-tag', 'atomic', ?1)",
            [now],
        )
        .unwrap();
    let injected_failure = transaction.execute(
        "INSERT INTO note_tags (note_id, tag_id) VALUES ('atomic-note', 'missing-tag')",
        [],
    );
    assert!(injected_failure.is_err());
    transaction.rollback().unwrap();

    for (table, id) in [("notes", "atomic-note"), ("tags", "atomic-tag")] {
        let count: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE id = ?1"),
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "transaction leaked a row into {table}");
    }
    assert_database_healthy(&connection);
}

#[test]
fn committed_data_and_integrity_survive_connection_close_and_reopen() {
    let fixture = TestAppDir::with_label("reopen-integrity");
    let mut connection = fixture.open_migrated_database();
    let note = NoteRepository::create(
        &connection,
        CreateNoteDto {
            title: Some("Persisted across reopen".to_string()),
            content: Some("Unicode data: नमस्ते 🌍\nsecond line".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    drop(connection);

    connection = init_connection(&fixture.paths.database_file).unwrap();
    run_migrations(&mut connection).unwrap();
    let reopened = NoteRepository::get_by_id(&connection, &note.id)
        .unwrap()
        .unwrap();
    assert_eq!(reopened.title, "Persisted across reopen");
    assert_eq!(reopened.content, "Unicode data: नमस्ते 🌍\nsecond line");
    assert_database_healthy(&connection);
}

#[test]
fn real_write_contention_returns_retryable_busy_and_recovers_after_release() {
    let fixture = TestAppDir::with_label("database-busy");
    let setup = fixture.open_migrated_database();
    setup
        .execute(
            "INSERT INTO settings (key, value) VALUES ('seed', 'ready')",
            [],
        )
        .unwrap();
    drop(setup);

    let config = SqliteConfig {
        busy_timeout_ms: 30,
        ..SqliteConfig::default()
    };
    let writer = init_connection_with_config(&fixture.paths.database_file, &config).unwrap();
    let contender = init_connection_with_config(&fixture.paths.database_file, &config).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    writer
        .execute("UPDATE settings SET value = 'held' WHERE key = 'seed'", [])
        .unwrap();

    let started = Instant::now();
    let error = contender
        .execute(
            "INSERT INTO settings (key, value) VALUES ('contender', 'blocked')",
            [],
        )
        .map_err(StorageError::Database)
        .expect_err("second writer must encounter the held write lock");
    assert!(started.elapsed() >= Duration::from_millis(20));
    assert_eq!(error.category(), ErrorCategory::DatabaseBusy);
    assert!(error.is_retryable());

    writer.execute_batch("ROLLBACK").unwrap();
    contender
        .execute(
            "INSERT INTO settings (key, value) VALUES ('contender', 'saved')",
            [],
        )
        .unwrap();
    assert_eq!(
        contender
            .query_row(
                "SELECT value FROM settings WHERE key = 'contender'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "saved"
    );
    assert_database_healthy(&contender);
}

#[test]
fn corrupted_database_is_detected_without_modifying_or_replacing_original_bytes() {
    let fixture = TestAppDir::with_label("corrupt-preservation");
    let corrupt_bytes = b"SQLite format 3\0intentionally truncated test fixture";
    fs::write(&fixture.paths.database_file, corrupt_bytes).unwrap();

    let error = init_connection(&fixture.paths.database_file)
        .err()
        .expect("corrupt database must be rejected");
    assert_eq!(error.category(), ErrorCategory::Corruption);
    assert_eq!(
        fs::read(&fixture.paths.database_file).unwrap(),
        corrupt_bytes
    );
    assert!(fixture.paths.database_file.is_file());
}
