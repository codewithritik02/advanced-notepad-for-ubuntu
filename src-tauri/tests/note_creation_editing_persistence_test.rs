mod common;

use chrono::{DateTime, Utc};
use common::TestDatabase;
use personal_notepad_lib::storage::{
    models::{CreateNoteDto, UpdateNoteDto},
    repositories::NoteRepository,
    ErrorCategory, StorageError, MAX_NOTE_CONTENT_BYTES, MAX_NOTE_TITLE_CHARS,
};
use rusqlite::Connection;
use std::thread;
use std::time::Duration;

fn parse_rfc3339(timestamp: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(timestamp)
        .expect("timestamp must be valid RFC3339")
        .with_timezone(&Utc)
}

fn assert_database_healthy(connection: &Connection) {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("PRAGMA integrity_check query");
    assert_eq!(integrity, "ok", "SQLite database must pass integrity check");

    let foreign_key_violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .expect("foreign key check");
    assert_eq!(
        foreign_key_violations, 0,
        "No foreign key violations allowed"
    );
}

#[test]
fn note_creation_with_standard_title_and_content_sets_authoritative_metadata() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let title = "Architecture Design Notes";
    let content = "Detailed breakdown of the offline-first personal notepad architecture.";

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some(title.to_string()),
            content: Some(content.to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("note creation must succeed");

    assert!(!note.id.is_empty(), "Note must have generated UUID");
    assert_eq!(note.title, title);
    assert_eq!(note.content, content);
    assert_eq!(note.format, "md");
    assert_eq!(note.notebook_id, None);
    assert!(!note.is_favorite);
    assert!(!note.is_pinned);
    assert!(!note.is_deleted);
    assert_eq!(note.deleted_at, None);

    let created_dt = parse_rfc3339(&note.created_at);
    assert_eq!(
        note.created_at, note.modified_at,
        "created_at and modified_at must be identical on initial note creation"
    );
    assert!(
        (Utc::now() - created_dt).num_seconds().abs() < 60,
        "Timestamp must be recent UTC"
    );

    // Verify persistence by querying the database directly
    let retrieved = NoteRepository::get_by_id(conn, &note.id)
        .expect("query note by id")
        .expect("note must exist in database");
    assert_eq!(retrieved, note);

    assert_database_healthy(conn);
}

#[test]
fn note_creation_with_empty_and_whitespace_content_is_preserved_verbatim() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    // 1. Note with explicit empty content and default title fallback
    let empty_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: None, // Omitting title must fallback to "Untitled Note"
            content: Some(String::new()),
            ..Default::default()
        },
    )
    .expect("creation of note with empty content must succeed");

    assert_eq!(empty_note.title, "Untitled Note");
    assert_eq!(empty_note.content, "");
    assert_eq!(empty_note.format, "txt");

    let retrieved_empty = NoteRepository::get_by_id(conn, &empty_note.id)
        .expect("get empty note")
        .expect("empty note must exist");
    assert_eq!(retrieved_empty.content, "");
    assert_eq!(retrieved_empty.title, "Untitled Note");

    // 2. Note with whitespace-only content (spaces, tabs, newlines)
    let whitespace_content = "   \t\r\n  \n\t   ";
    let whitespace_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Whitespace Note".to_string()),
            content: Some(whitespace_content.to_string()),
            ..Default::default()
        },
    )
    .expect("creation of note with whitespace content must succeed");

    assert_eq!(whitespace_note.content, whitespace_content);

    let retrieved_whitespace = NoteRepository::get_by_id(conn, &whitespace_note.id)
        .expect("get whitespace note")
        .expect("whitespace note must exist");
    assert_eq!(
        retrieved_whitespace.content, whitespace_content,
        "Whitespace content must NOT be stripped or collapsed"
    );

    assert_database_healthy(conn);
}

#[test]
fn renaming_note_updates_title_preserves_content_and_advances_modified_at() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let initial_content = "Body content that must remain untouched when title is modified.";
    let initial_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Initial Draft Title".to_string()),
            content: Some(initial_content.to_string()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("create initial note");

    let initial_created_dt = parse_rfc3339(&initial_note.created_at);

    // Sleep briefly to guarantee chronological timestamp difference
    thread::sleep(Duration::from_millis(20));

    let updated_title = "Final Approved Title";
    let renamed_note = NoteRepository::update(
        conn,
        &initial_note.id,
        UpdateNoteDto {
            title: Some(updated_title.to_string()),
            ..Default::default()
        },
    )
    .expect("rename note update must succeed");

    assert_eq!(renamed_note.id, initial_note.id);
    assert_eq!(renamed_note.title, updated_title);
    assert_eq!(
        renamed_note.content, initial_content,
        "Note content must remain completely unchanged when only renaming"
    );
    assert_eq!(
        renamed_note.created_at, initial_note.created_at,
        "created_at must remain immutable across renames"
    );

    let renamed_modified_dt = parse_rfc3339(&renamed_note.modified_at);
    assert!(
        renamed_modified_dt >= initial_created_dt,
        "modified_at must be greater than or equal to original created_at"
    );

    // Query back from database to verify persistence
    let re_read = NoteRepository::get_by_id(conn, &initial_note.id)
        .expect("query renamed note")
        .expect("renamed note must exist");
    assert_eq!(re_read.title, updated_title);
    assert_eq!(re_read.content, initial_content);
    assert_eq!(re_read.created_at, initial_note.created_at);
    assert_eq!(re_read.modified_at, renamed_note.modified_at);

    assert_database_healthy(conn);
}

#[test]
fn repeated_edits_advance_modified_at_monotonically_and_preserve_created_at() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let title = "Monotonic Revision Test";
    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some(title.to_string()),
            content: Some("Revision 0".to_string()),
            ..Default::default()
        },
    )
    .expect("create note for repeated edits");

    let original_created_at = note.created_at.clone();
    let mut last_modified_dt = parse_rfc3339(&note.modified_at);

    let revisions = [
        "Revision 1: Added introduction.",
        "Revision 2: Added detailed technical specs.",
        "Revision 3: Added conclusion and summary.",
    ];

    for (index, revision_content) in revisions.iter().enumerate() {
        thread::sleep(Duration::from_millis(15));

        let updated = NoteRepository::update(
            conn,
            &note.id,
            UpdateNoteDto {
                content: Some(revision_content.to_string()),
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("failed to save revision {index}: {e}"));

        assert_eq!(
            updated.created_at, original_created_at,
            "created_at must not change on revision {index}"
        );
        assert_eq!(updated.title, title, "title must be preserved");
        assert_eq!(updated.content, *revision_content);

        let current_modified_dt = parse_rfc3339(&updated.modified_at);
        assert!(
            current_modified_dt >= last_modified_dt,
            "modified_at must advance monotonically on revision {index}"
        );
        last_modified_dt = current_modified_dt;
    }

    // Verify final state in database
    let final_note = NoteRepository::get_by_id(conn, &note.id)
        .expect("fetch final note")
        .expect("final note must exist");
    assert_eq!(final_note.content, revisions[2]);
    assert_eq!(final_note.created_at, original_created_at);

    assert_database_healthy(conn);
}

#[test]
fn long_note_editing_and_resource_limits_enforcement() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    // 1. Large 100 KiB note with realistic paragraphs
    let paragraph = "Personal Notepad is designed with strict offline-first guarantees, SQLite WAL mode, and zero cloud sync.\n";
    let target_100kb_bytes = 100 * 1024;
    let mut content_100k = String::with_capacity(target_100kb_bytes + 1024);
    while content_100k.len() < target_100kb_bytes {
        content_100k.push_str(paragraph);
    }

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("100KB Stress Note".to_string()),
            content: Some(content_100k.clone()),
            ..Default::default()
        },
    )
    .expect("100KB note creation must succeed");

    let read_100k = NoteRepository::get_by_id(conn, &note.id)
        .expect("read 100KB note")
        .expect("note must exist");
    assert_eq!(read_100k.content.len(), content_100k.len());
    assert_eq!(read_100k.content, content_100k);

    // 2. Large 1 MiB note update
    let target_1mb_bytes = 1024 * 1024;
    let mut content_1mb = String::with_capacity(target_1mb_bytes + 1024);
    while content_1mb.len() < target_1mb_bytes {
        content_1mb.push_str("Stress testing 1MB note persistence across SQLite boundary.\n");
    }

    let updated_1mb = NoteRepository::update(
        conn,
        &note.id,
        UpdateNoteDto {
            content: Some(content_1mb.clone()),
            ..Default::default()
        },
    )
    .expect("1MB note update must succeed");
    assert_eq!(updated_1mb.content.len(), content_1mb.len());

    let read_1mb = NoteRepository::get_by_id(conn, &note.id)
        .expect("read 1MB note")
        .expect("note must exist");
    assert_eq!(read_1mb.content, content_1mb);

    // 3. Validation limit rejection: note title exceeding MAX_NOTE_TITLE_CHARS (500 chars)
    let oversize_title = "T".repeat(MAX_NOTE_TITLE_CHARS + 1);
    let title_err = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some(oversize_title.clone()),
            content: Some("valid content".to_string()),
            ..Default::default()
        },
    );
    match title_err {
        Err(StorageError::Validation(msg)) => {
            assert!(msg.contains("exceeds the maximum allowed limit of 500 characters"));
        }
        other => panic!("expected Validation error for oversize title, got: {other:?}"),
    }

    // 4. Validation limit rejection: note content exceeding MAX_NOTE_CONTENT_BYTES (10 MiB)
    let oversize_content = "X".repeat(MAX_NOTE_CONTENT_BYTES + 1);
    let content_err = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Valid Title".to_string()),
            content: Some(oversize_content),
            ..Default::default()
        },
    );
    match content_err {
        Err(StorageError::Validation(msg)) => {
            assert!(msg.contains("exceeds the maximum allowed limit of 10 MB"));
        }
        other => panic!("expected Validation error for oversize content, got: {other:?}"),
    }

    assert_database_healthy(conn);
}

#[test]
fn unicode_emoji_multilingual_scripts_and_special_characters_survive_roundtrip() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let complex_title = "Multilingual 🚀 📝 Notes — 日本語 & हिन्दी & العربية";
    let complex_content = concat!(
        "English: Offline-first note taking.\n",
        "Hindi: नमस्ते दुनिया! यह एक व्यापक परीक्षण नोट है।\n",
        "Arabic (RTL): مرحبا بالعالم! هذا اختبار نصوص باللغة العربية مع اتجاه من اليمين إلى اليسار.\n",
        "Japanese: こんにちは世界！オフライン専用ノートパッド。\n",
        "Russian: Привет, мир! Тестирование сохранения Unicode.\n",
        "German: Größenmaßstäbe und Überprüfung von Umlauten (ä, ö, ü, ß).\n",
        "Mathematical symbols: ∑_{i=1}^n x_i = ∫_0^∞ f(t) dt ≠ ∅ ∧ ∀x ∈ ℝ.\n",
        "Emojis & Surrogates: 🎉 💻 🦀 🛡️ ⚡ 📝 🔍 ✨ 🔥 🚀 🧩\n",
        "Combining Diacritics: é, à, Z͑ͫ̓ͪ̂ͫ̽͏̴̙̤̞͉͚̯̞a͉͕l̠̞g͍͔o̬\n",
        "SQL injection payloads (should remain pure text): ' OR '1'='1'; DROP TABLE notes; --\n",
        "HTML/XML tags: <div class=\"container\" id=\"test\"><script>alert('xss');</script></div>\n",
        "JSON strings: {\"id\": \"not-a-uuid\", \"title\": \"injected\", \"is_deleted\": true}\n",
        "Line break mix:\r\nWindows CRLF\nLinux LF\r\n\r\nMultiple blank lines.\n"
    );

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some(complex_title.to_string()),
            content: Some(complex_content.to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create note with complex Unicode must succeed");

    assert_eq!(note.title, complex_title);
    assert_eq!(note.content, complex_content);

    // Read back and assert byte-for-byte fidelity
    let retrieved = NoteRepository::get_by_id(conn, &note.id)
        .expect("read note")
        .expect("note must exist");

    assert_eq!(
        retrieved.title, complex_title,
        "Title with multilingual scripts and emoji must match byte-for-byte"
    );
    assert_eq!(
        retrieved.content, complex_content,
        "Content with RTL, mathematical symbols, SQL injection patterns, and CRLF must match byte-for-byte"
    );

    assert_database_healthy(conn);
}

#[test]
fn closing_and_reopening_database_restores_last_successfully_persisted_state() {
    let mut db = TestDatabase::new();

    let initial_title = "Persistence Check Note";
    let initial_content = "State 0: Written before close.";
    let note = NoteRepository::create(
        db.connection_mut(),
        CreateNoteDto {
            title: Some(initial_title.to_string()),
            content: Some(initial_content.to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let note_id = note.id.clone();

    // Perform an update to state 1
    thread::sleep(Duration::from_millis(15));
    let final_content = "State 1: Updated and committed prior to application shutdown.";
    let updated = NoteRepository::update(
        db.connection_mut(),
        &note_id,
        UpdateNoteDto {
            content: Some(final_content.to_string()),
            ..Default::default()
        },
    )
    .expect("update note");

    let saved_modified_at = updated.modified_at.clone();
    let saved_created_at = updated.created_at.clone();

    // Simulate complete application restart by closing and reopening the SQLite database
    db.close_and_reopen();
    let conn = db.connection();

    assert_database_healthy(conn);

    let restored = NoteRepository::get_by_id(conn, &note_id)
        .expect("query note after reopen")
        .expect("note must exist after application reopen");

    assert_eq!(restored.id, note_id);
    assert_eq!(restored.title, initial_title);
    assert_eq!(restored.content, final_content);
    assert_eq!(restored.created_at, saved_created_at);
    assert_eq!(restored.modified_at, saved_modified_at);
    assert!(!restored.is_deleted);
    assert_eq!(restored.deleted_at, None);
}

#[test]
fn deletion_model_lifecycle_soft_delete_trash_isolation_and_restoration() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Lifecycle Deletion Note".to_string()),
            content: Some("Content that will be trashed and restored.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let note_id = note.id.clone();

    // 1. Note is initially active
    assert!(
        NoteRepository::get_active_by_id(conn, &note_id)
            .expect("get active")
            .is_some(),
        "Note must be present in active query"
    );
    let active_list = NoteRepository::list_active(conn).expect("list active");
    assert!(active_list.iter().any(|n| n.id == note_id));

    // 2. Soft delete moves note to Trash
    thread::sleep(Duration::from_millis(15));
    let trashed_note =
        NoteRepository::soft_delete(conn, &note_id).expect("soft delete must succeed");
    assert!(trashed_note.is_deleted);
    assert!(trashed_note.deleted_at.is_some());

    // Active queries must now exclude the note
    let active_after_trash =
        NoteRepository::get_active_by_id(conn, &note_id).expect("get active after trash");
    assert!(
        active_after_trash.is_none(),
        "Trashed note must not be returned by get_active_by_id"
    );
    let active_list_after_trash =
        NoteRepository::list_active(conn).expect("list active after trash");
    assert!(!active_list_after_trash.iter().any(|n| n.id == note_id));

    // Trashed queries must include the note with its content intact
    let trashed_retrieved = NoteRepository::get_trashed_by_id(conn, &note_id)
        .expect("get trashed by id")
        .expect("trashed note must exist in trashed query");
    assert_eq!(trashed_retrieved.content, note.content);
    assert!(trashed_retrieved.is_deleted);

    let trash_list = NoteRepository::list_trashed(conn).expect("list trashed");
    assert!(trash_list.iter().any(|n| n.id == note_id));

    // 3. Invariant: Redundant soft-delete must return Conflict error
    let double_delete = NoteRepository::soft_delete(conn, &note_id);
    match double_delete {
        Err(StorageError::Conflict(msg)) => {
            assert!(msg.contains("is already in Trash"));
        }
        other => panic!("expected Conflict error on redundant soft delete, got: {other:?}"),
    }

    // 4. Invariant: Updating a note while in Trash is strictly forbidden
    let update_trashed = NoteRepository::update(
        conn,
        &note_id,
        UpdateNoteDto {
            title: Some("Illegal edit while trashed".to_string()),
            ..Default::default()
        },
    );
    match update_trashed {
        Err(StorageError::Validation(msg)) => {
            assert!(msg.contains("Cannot update a note that is in the trash. Restore it first."));
        }
        other => panic!("expected Validation error when editing trashed note, got: {other:?}"),
    }

    // 5. Restore note from Trash
    let restored = NoteRepository::restore(conn, &note_id).expect("restore from trash");
    assert!(!restored.is_deleted);
    assert_eq!(restored.deleted_at, None);

    // Active queries now return the restored note
    let active_after_restore = NoteRepository::get_active_by_id(conn, &note_id)
        .expect("get active after restore")
        .expect("restored note must be active");
    assert_eq!(active_after_restore.content, note.content);

    // Edits now succeed once restored
    let post_restore_update = NoteRepository::update(
        conn,
        &note_id,
        UpdateNoteDto {
            content: Some("Successfully edited after restoration.".to_string()),
            ..Default::default()
        },
    )
    .expect("update after restore must succeed");
    assert_eq!(
        post_restore_update.content,
        "Successfully edited after restoration."
    );

    assert_database_healthy(conn);
}

#[test]
fn failed_save_simulation_preserves_committed_database_state() {
    let mut db = TestDatabase::new();
    let conn = db.connection_mut();

    let committed_title = "Pristine Original Title";
    let committed_content = "Committed note content that must NEVER be corrupted by failed saves.";
    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some(committed_title.to_string()),
            content: Some(committed_content.to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let note_id = note.id.clone();
    let committed_modified_at = note.modified_at.clone();

    // 1. Injected failure: Title exceeds length limit
    let invalid_title = "OversizeTitle_".repeat(50);
    let failed_title_update = NoteRepository::update(
        conn,
        &note_id,
        UpdateNoteDto {
            title: Some(invalid_title),
            ..Default::default()
        },
    );
    assert!(failed_title_update.is_err());
    assert_eq!(
        failed_title_update.unwrap_err().category(),
        ErrorCategory::Validation
    );

    // Verify database record was NOT corrupted
    let note_check_1 = NoteRepository::get_by_id(conn, &note_id)
        .expect("get note")
        .expect("note must exist");
    assert_eq!(note_check_1.title, committed_title);
    assert_eq!(note_check_1.content, committed_content);
    assert_eq!(note_check_1.modified_at, committed_modified_at);

    // 2. Injected failure: Content exceeds 10 MiB limit
    let oversize_content = "Z".repeat(MAX_NOTE_CONTENT_BYTES + 1024);
    let failed_content_update = NoteRepository::update(
        conn,
        &note_id,
        UpdateNoteDto {
            content: Some(oversize_content),
            ..Default::default()
        },
    );
    assert!(failed_content_update.is_err());
    assert_eq!(
        failed_content_update.unwrap_err().category(),
        ErrorCategory::Validation
    );

    // Verify database record was NOT corrupted
    let note_check_2 = NoteRepository::get_by_id(conn, &note_id)
        .expect("get note")
        .expect("note must exist");
    assert_eq!(note_check_2.title, committed_title);
    assert_eq!(note_check_2.content, committed_content);
    assert_eq!(note_check_2.modified_at, committed_modified_at);

    // 3. Injected failure: Updating a non-existent note ID
    let missing_id = "00000000-0000-0000-0000-000000000000";
    let failed_missing_update = NoteRepository::update(
        conn,
        missing_id,
        UpdateNoteDto {
            content: Some("Ghost content".to_string()),
            ..Default::default()
        },
    );
    assert!(failed_missing_update.is_err());
    assert_eq!(
        failed_missing_update.unwrap_err().category(),
        ErrorCategory::NotFound
    );

    assert_database_healthy(conn);
}
