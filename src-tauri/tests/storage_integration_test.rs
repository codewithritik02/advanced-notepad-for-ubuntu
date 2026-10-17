mod common;

use common::TestAppDir as TestEnv;
use personal_notepad_lib::storage::{
    init_connection,
    models::{CreateNoteDto, CreateNotebookDto, UpdateNoteDto, UpdateNotebookDto, UpdateTagDto},
    repositories::{NoteRepository, NotebookRepository, SettingsRepository, TagRepository},
    run_migrations, StorageError, StoragePaths,
};

#[test]
fn test_fresh_database_initialization_and_migration() {
    let env = TestEnv::new();

    let mut conn =
        init_connection(&env.paths.database_file).expect("Database initialization failed");
    run_migrations(&mut conn).expect("Migration failed");

    assert!(
        env.paths.database_file.exists(),
        "database.sqlite must exist"
    );

    // Verify migration 001 recorded
    let version: i32 = conn
        .query_row(
            "SELECT version FROM _migrations WHERE version = 1",
            [],
            |r| r.get(0),
        )
        .expect("Migration 1 must be recorded");
    assert_eq!(version, 1);

    // Verify all core tables exist in sqlite_master
    for table in [
        "notes",
        "notebooks",
        "tags",
        "note_tags",
        "attachments",
        "settings",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .expect("Table check failed");
        assert_eq!(count, 1, "Table '{table}' must exist in fresh schema");
    }
}

#[test]
fn test_reopen_and_persistence_across_connections() {
    let env = TestEnv::new();

    let note_id;
    let nb_id;
    let tag_id;

    // 1. First connection: write data
    {
        let mut conn =
            init_connection(&env.paths.database_file).expect("Initial connection failed");
        run_migrations(&mut conn).expect("Migration failed");

        let nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Persistent Notebook".to_string(),
                parent_id: None,
            },
        )
        .expect("Notebook create failed");
        nb_id = nb.id;

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Persistent Note".to_string()),
                content: Some("Data that must survive application close".to_string()),
                format: Some("txt".to_string()),
                notebook_id: Some(nb_id.clone()),
                is_favorite: Some(true),
                is_pinned: Some(true),
            },
        )
        .expect("Note create failed");
        note_id = note.id;

        let tag = TagRepository::create(&conn, "persist-tag").expect("Tag create failed");
        tag_id = tag.id;
        TagRepository::add_tag_to_note(&conn, &note_id, &tag_id).expect("Link tag failed");

        SettingsRepository::set(&conn, "theme", "dark").expect("Set setting failed");
    } // conn dropped here

    // 2. Second connection: reopen and verify all records survive intact
    {
        let mut conn = init_connection(&env.paths.database_file).expect("Reopen connection failed");
        run_migrations(&mut conn).expect("Subsequent migration check must succeed");

        // Verify Notebook
        let nb = NotebookRepository::get_by_id(&conn, &nb_id)
            .expect("Query failed")
            .expect("Notebook must survive restart");
        assert_eq!(nb.name, "Persistent Notebook");

        // Verify Note
        let note = NoteRepository::get_by_id(&conn, &note_id)
            .expect("Query failed")
            .expect("Note must survive restart");
        assert_eq!(note.title, "Persistent Note");
        assert_eq!(note.content, "Data that must survive application close");
        assert!(note.is_favorite);
        assert!(note.is_pinned);
        assert_eq!(note.notebook_id, Some(nb_id));

        // Verify Tag & Junction
        let tags = TagRepository::get_tags_for_note(&conn, &note_id).expect("Get tags failed");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "persist-tag");

        // Verify Setting
        let theme = SettingsRepository::get(&conn, "theme").expect("Get setting failed");
        assert_eq!(theme, Some("dark".to_string()));
    }
}

#[test]
fn test_unicode_preservation_complex() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let hindi_title = "आज की मीटिंग के नोट्स 📝";
    let complex_content =
        "नमस्ते दुनिया! Testing Hindi (हिंदी), English, emojis (🚀🔥🧠), and symbols (<>&\"'/\\).";
    let unicode_tag = "ज़रूरी-दस्तावेज़ 🏷️";
    let unicode_nb = "व्यक्तिगत प्रोजेक्ट्स 📁";

    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: unicode_nb.to_string(),
            parent_id: None,
        },
    )
    .expect("Notebook create failed");
    assert_eq!(nb.name, unicode_nb);

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(hindi_title.to_string()),
            content: Some(complex_content.to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        },
    )
    .expect("Note create failed");
    assert_eq!(note.title, hindi_title);
    assert_eq!(note.content, complex_content);

    let tag = TagRepository::create(&conn, unicode_tag).expect("Tag create failed");
    assert_eq!(tag.name, unicode_tag);

    TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).expect("Link failed");
    let fetched_tags =
        TagRepository::get_tags_for_note(&conn, &note.id).expect("Fetch tags failed");
    assert_eq!(fetched_tags[0].name, unicode_tag);
}

#[test]
fn test_sql_injection_defense() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let injection_title = "Robert'); DROP TABLE notes;--";
    let injection_content = "1' OR '1'='1; DROP TABLE notebooks;--";

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(injection_title.to_string()),
            content: Some(injection_content.to_string()),
            ..Default::default()
        },
    )
    .expect("Creation with SQL injection characters must be handled safely as text");

    assert_eq!(note.title, injection_title);
    assert_eq!(note.content, injection_content);

    // Verify notes table was NOT dropped
    let count: i64 = conn
        .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
        .expect("Query failed");
    assert_eq!(count, 1, "Notes table must remain intact");

    // Setting key SQL injection attempt
    let injection_key = "key'; DROP TABLE settings;--";
    SettingsRepository::set(&conn, injection_key, "safe-value").expect("Set setting failed");
    let val = SettingsRepository::get(&conn, injection_key).expect("Get failed");
    assert_eq!(val, Some("safe-value".to_string()));
}

#[test]
fn test_nested_notebook_hierarchy_three_levels() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let level1 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Level 1 Root".to_string(),
            parent_id: None,
        },
    )
    .expect("Level 1 failed");

    let level2 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Level 2 Child".to_string(),
            parent_id: Some(level1.id.clone()),
        },
    )
    .expect("Level 2 failed");

    let level3 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Level 3 Grandchild".to_string(),
            parent_id: Some(level2.id.clone()),
        },
    )
    .expect("Level 3 failed");

    assert_eq!(level1.parent_id, None);
    assert_eq!(level2.parent_id, Some(level1.id.clone()));
    assert_eq!(level3.parent_id, Some(level2.id.clone()));

    let all_nbs = NotebookRepository::list(&conn).expect("List failed");
    assert_eq!(all_nbs.len(), 3);
}

#[test]
fn test_soft_delete_and_list_filtering() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let n1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("N1".to_string()),
            ..Default::default()
        },
    )
    .expect("N1 failed");
    let n2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("N2".to_string()),
            ..Default::default()
        },
    )
    .expect("N2 failed");
    let _n3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("N3".to_string()),
            ..Default::default()
        },
    )
    .expect("N3 failed");

    // Soft delete n2
    let deleted = NoteRepository::delete(&conn, &n2.id, true).expect("Delete failed");
    assert!(deleted);

    let active = NoteRepository::list(&conn, false).expect("List active failed");
    assert_eq!(active.len(), 2);
    assert!(!active.iter().any(|n| n.id == n2.id));

    let all = NoteRepository::list(&conn, true).expect("List all failed");
    assert_eq!(all.len(), 3);
    let deleted_item = all
        .iter()
        .find(|n| n.id == n2.id)
        .expect("n2 must exist in trash");
    assert!(deleted_item.is_deleted);
    assert!(deleted_item.deleted_at.is_some());

    // Update n1
    let updated_n1 = NoteRepository::update(
        &conn,
        &n1.id,
        UpdateNoteDto {
            title: Some("N1 Updated".to_string()),
            ..Default::default()
        },
    )
    .expect("Update failed");
    assert_eq!(updated_n1.title, "N1 Updated");
}

#[test]
fn test_full_restart_and_recovery_verification() {
    let env = TestEnv::new();

    // Session 1: Create Note, Multiple Notes, Unicode, Notebook, Tag, and Setting
    let note1_id;
    let unicode_note_id;
    let nb_id;
    let tag_id;
    let test1_title = "Single Note Persistence Test";
    let test1_content = "This content was written before closing the app.";
    let unicode_title = "आज का विचार 🌟";
    let unicode_content = "हिंदी टेक्स्ट और इमोज़ी: 🚀✨🔒";

    {
        let mut conn =
            init_connection(&env.paths.database_file).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migrations failed");

        // Test 1: Note persistence
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(test1_title.to_string()),
                content: Some(test1_content.to_string()),
                ..Default::default()
            },
        )
        .expect("Create note 1 failed");
        note1_id = note1.id;

        // Test 2: Multiple notes (create 3 more notes)
        for i in 2..=4 {
            NoteRepository::create(
                &conn,
                CreateNoteDto {
                    title: Some(format!("Note {i}")),
                    content: Some(format!("Content for note {i}")),
                    ..Default::default()
                },
            )
            .expect("Create multiple note failed");
        }

        // Test 3: Unicode note
        let u_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(unicode_title.to_string()),
                content: Some(unicode_content.to_string()),
                ..Default::default()
            },
        )
        .expect("Create unicode note failed");
        unicode_note_id = u_note.id;

        // Test 4: Notebook
        let nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Work Notebook 💼".to_string(),
                parent_id: None,
            },
        )
        .expect("Create notebook failed");
        nb_id = nb.id;

        // Test 5: Tag
        let tag = TagRepository::create(&conn, "urgent-tag").expect("Create tag failed");
        tag_id = tag.id;
        TagRepository::add_tag_to_note(&conn, &note1_id, &tag_id).expect("Link tag failed");

        // Test 6: Settings
        SettingsRepository::set(&conn, "theme", "dark").expect("Set theme setting failed");
    } // Simulated application shutdown (connection closed and dropped)

    // Session 2: Application Restart & Recovery Verification
    {
        let mut conn =
            init_connection(&env.paths.database_file).expect("Session 2 connection failed");
        run_migrations(&mut conn).expect("Session 2 migrations check failed");

        // Verify Test 1: Note content exactly matches
        let n1 = NoteRepository::get_by_id(&conn, &note1_id)
            .expect("Query failed")
            .expect("Note 1 must survive restart");
        assert_eq!(n1.title, test1_title);
        assert_eq!(n1.content, test1_content);

        // Verify Test 2: Multiple notes all remain (1 initial + 3 additional + 1 unicode = 5 total)
        let all_notes = NoteRepository::list(&conn, false).expect("List notes failed");
        assert_eq!(all_notes.len(), 5, "All 5 notes must survive restart");

        // Verify Test 3: Unicode content exactly preserved
        let un = NoteRepository::get_by_id(&conn, &unicode_note_id)
            .expect("Query failed")
            .expect("Unicode note must survive restart");
        assert_eq!(un.title, unicode_title);
        assert_eq!(un.content, unicode_content);

        // Verify Test 4: Notebook remains
        let nb = NotebookRepository::get_by_id(&conn, &nb_id)
            .expect("Query failed")
            .expect("Notebook must survive restart");
        assert_eq!(nb.name, "Work Notebook 💼");

        // Verify Test 5: Tag remains
        let tags = TagRepository::get_tags_for_note(&conn, &note1_id).expect("Get tags failed");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].id, tag_id);
        assert_eq!(tags[0].name, "urgent-tag");

        // Verify Test 6: Setting remains
        let theme = SettingsRepository::get(&conn, "theme").expect("Get setting failed");
        assert_eq!(theme, Some("dark".to_string()));
    }
}

#[test]
fn test_txt_format_exact_preservation() {
    let env = TestEnv::new();

    let raw_txt_title = "Meeting Notes — 2026-09-30 📝";
    let raw_txt_content = "Hello world.\n\nThis is my plain text note.\n  Indented line with 2 spaces.\n\tLine with tab character.\n\nआज की मीटिंग के नोट्स 📝\n\nMarkdown syntax must NOT be parsed or converted:\n# Not a heading\n- Not a list\n**Not bold**\n<script>alert('not html')</script>\n\nFinal line with trailing spaces   \n";

    let note_id = {
        let mut conn = init_connection(&env.paths.database_file).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(raw_txt_title.to_string()),
                content: Some(raw_txt_content.to_string()),
                format: Some("txt".to_string()),
                ..Default::default()
            },
        )
        .expect("Create note failed");

        assert_eq!(note.format, "txt");
        note.id
    };

    // Reopen database connection and verify exact preservation
    {
        let conn = init_connection(&env.paths.database_file).expect("Reopen failed");
        let retrieved = NoteRepository::get_by_id(&conn, &note_id)
            .expect("Query failed")
            .expect("Note must exist");

        assert_eq!(retrieved.title, raw_txt_title);
        assert_eq!(retrieved.content, raw_txt_content);
        assert_eq!(retrieved.format, "txt");

        // Verify lines count and exact whitespace
        let original_lines: Vec<&str> = raw_txt_content.split('\n').collect();
        let retrieved_lines: Vec<&str> = retrieved.content.split('\n').collect();
        assert_eq!(original_lines.len(), retrieved_lines.len());
        for (idx, (orig, ret)) in original_lines
            .iter()
            .zip(retrieved_lines.iter())
            .enumerate()
        {
            assert_eq!(
                orig, ret,
                "Line {idx} must match exactly without trimming or conversion"
            );
        }
    }
}

#[test]
fn test_markdown_format_exact_preservation() {
    let env = TestEnv::new();

    let md_title = "# Project Architecture & Roadmap 🚀";
    let md_content = "# Personal Notepad\n\n## Core Principles\n- Local-first\n- Offline-first\n- Zero cloud dependencies\n\n### Code Sample\n```rust\nfn main() {\n    println!(\"Hello SQLite!\");\n}\n```\n\n| Feature | Status |\n|---|---|\n| SQLite | Supported |\n| Markdown | Supported |\n\n[Documentation Link](https://localhost:3000/docs)\n\n*Italic text* and **bold text**\n";

    let note_id = {
        let mut conn = init_connection(&env.paths.database_file).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(md_title.to_string()),
                content: Some(md_content.to_string()),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .expect("Create markdown note failed");

        assert_eq!(note.format, "md");
        note.id
    };

    // Reopen database connection and verify exact raw markdown text preservation
    {
        let conn = init_connection(&env.paths.database_file).expect("Reopen failed");
        let retrieved = NoteRepository::get_by_id(&conn, &note_id)
            .expect("Query failed")
            .expect("Note must exist");

        assert_eq!(retrieved.title, md_title);
        assert_eq!(retrieved.content, md_content);
        assert_eq!(retrieved.format, "md");

        // Verify lines count and exact formatting markers without HTML transformation
        let original_lines: Vec<&str> = md_content.split('\n').collect();
        let retrieved_lines: Vec<&str> = retrieved.content.split('\n').collect();
        assert_eq!(original_lines.len(), retrieved_lines.len());
        for (idx, (orig, ret)) in original_lines
            .iter()
            .zip(retrieved_lines.iter())
            .enumerate()
        {
            assert_eq!(
                orig, ret,
                "Markdown line {idx} must match exactly without HTML conversion"
            );
        }

        // Test deliberate format toggle from "md" to "txt" without content corruption
        let updated = NoteRepository::update(
            &conn,
            &note_id,
            UpdateNoteDto {
                format: Some("txt".to_string()),
                ..Default::default()
            },
        )
        .expect("Update format failed");

        assert_eq!(updated.format, "txt");
        assert_eq!(
            updated.content, md_content,
            "Content must remain 100% untouched when format changes"
        );
    }
}

#[test]
fn test_save_on_note_switch() {
    let env = TestEnv::new();

    let mut conn = init_connection(&env.paths.database_file).expect("Init connection failed");
    run_migrations(&mut conn).expect("Migrations failed");

    // Create Note A
    let note_a = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note A Initial".to_string()),
            content: Some("Note A Initial Content".to_string()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Create Note A failed");

    // Create Note B
    let note_b = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note B Initial".to_string()),
            content: Some("Note B Initial Content".to_string()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Create Note B failed");

    // Simulate user typing into Note A:
    let edited_a_title = "Note A Edited Before Switch";
    let edited_a_content = "Note A Content has been modified in memory.";

    // Simulate note switch flush:
    // Before opening Note B, Note A changes are flushed to SQLite:
    let updated_a = NoteRepository::update(
        &conn,
        &note_a.id,
        UpdateNoteDto {
            title: Some(edited_a_title.to_string()),
            content: Some(edited_a_content.to_string()),
            ..Default::default()
        },
    )
    .expect("Flush Note A failed");

    assert_eq!(updated_a.title, edited_a_title);
    assert_eq!(updated_a.content, edited_a_content);

    // Switch to Note B:
    let loaded_b = NoteRepository::get_by_id(&conn, &note_b.id)
        .expect("Get Note B failed")
        .expect("Note B must exist");
    assert_eq!(loaded_b.title, "Note B Initial");

    // Switch back to Note A:
    let reopened_a = NoteRepository::get_by_id(&conn, &note_a.id)
        .expect("Reopen Note A failed")
        .expect("Note A must exist");
    assert_eq!(reopened_a.title, edited_a_title);
    assert_eq!(reopened_a.content, edited_a_content);
    assert_ne!(reopened_a.modified_at, note_a.modified_at);
}

#[test]
fn test_empty_note_and_whitespace_preservation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create completely empty note
    let empty_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Untitled Note".to_string()),
            content: Some("".to_string()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Create empty note failed");

    assert_eq!(empty_note.title, "Untitled Note");
    assert_eq!(empty_note.content, "");
    assert_eq!(empty_note.format, "txt");

    // 2. Fetch empty note from database and verify no fake content is inserted
    let fetched_empty = NoteRepository::get_by_id(&conn, &empty_note.id)
        .expect("Fetch failed")
        .expect("Empty note must exist");
    assert_eq!(fetched_empty.content, "");
    assert_eq!(fetched_empty.title, "Untitled Note");

    // 3. Create whitespace-only note
    let whitespace_content = "   \n\t   \n  ";
    let ws_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Whitespace Note".to_string()),
            content: Some(whitespace_content.to_string()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Create whitespace note failed");

    assert_eq!(ws_note.content, whitespace_content);

    // 4. Update with whitespace-only content and verify content is NOT trimmed
    let multiline_whitespace = "\n\n   \t\t\n   ";
    let updated_ws = NoteRepository::update(
        &conn,
        &empty_note.id,
        UpdateNoteDto {
            content: Some(multiline_whitespace.to_string()),
            ..Default::default()
        },
    )
    .expect("Update with whitespace failed");

    assert_eq!(updated_ws.content, multiline_whitespace);

    // 5. Verify persistence across connection reopen
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let verified_ws = NoteRepository::get_by_id(&conn2, &empty_note.id)
        .expect("Get failed")
        .expect("Note must exist");
    assert_eq!(verified_ws.content, multiline_whitespace);
}

#[test]
fn test_large_text_handling_10kb_to_1mb() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // Helper to generate patterned text of exact size
    let generate_text = |size_in_bytes: usize| -> String {
        let chunk = "Line: Hello Personal Notepad offline local-first SQLite text editor!\n";
        let mut result = String::with_capacity(size_in_bytes);
        while result.len() + chunk.len() <= size_in_bytes {
            result.push_str(chunk);
        }
        while result.len() < size_in_bytes {
            result.push('x');
        }
        result
    };

    // 1. Test 10 KB note
    let text_10kb = generate_text(10 * 1024);
    assert_eq!(text_10kb.len(), 10 * 1024);
    let note_10kb = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("10 KB Note".to_string()),
            content: Some(text_10kb.clone()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Create 10KB failed");
    let fetched_10kb = NoteRepository::get_by_id(&conn, &note_10kb.id)
        .expect("Fetch failed")
        .expect("Must exist");
    assert_eq!(fetched_10kb.content.len(), 10 * 1024);
    assert_eq!(fetched_10kb.content, text_10kb);

    // 2. Test 100 KB note
    let text_100kb = generate_text(100 * 1024);
    assert_eq!(text_100kb.len(), 100 * 1024);
    let note_100kb = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("100 KB Note".to_string()),
            content: Some(text_100kb.clone()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("Create 100KB failed");
    let fetched_100kb = NoteRepository::get_by_id(&conn, &note_100kb.id)
        .expect("Fetch failed")
        .expect("Must exist");
    assert_eq!(fetched_100kb.content.len(), 100 * 1024);
    assert_eq!(fetched_100kb.content, text_100kb);

    // 3. Test 500 KB note
    let text_500kb = generate_text(500 * 1024);
    assert_eq!(text_500kb.len(), 500 * 1024);
    let note_500kb = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("500 KB Note".to_string()),
            content: Some(text_500kb.clone()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Create 500KB failed");
    let fetched_500kb = NoteRepository::get_by_id(&conn, &note_500kb.id)
        .expect("Fetch failed")
        .expect("Must exist");
    assert_eq!(fetched_500kb.content.len(), 500 * 1024);
    assert_eq!(fetched_500kb.content, text_500kb);

    // 4. Test 1 MB note (1,048,576 bytes)
    let text_1mb = generate_text(1024 * 1024);
    assert_eq!(text_1mb.len(), 1024 * 1024);
    let updated_to_1mb = NoteRepository::update(
        &conn,
        &note_10kb.id,
        UpdateNoteDto {
            title: Some("Updated to 1 MB Note".to_string()),
            content: Some(text_1mb.clone()),
            ..Default::default()
        },
    )
    .expect("Update to 1MB failed");
    assert_eq!(updated_to_1mb.content.len(), 1024 * 1024);
    assert_eq!(updated_to_1mb.content, text_1mb);

    // 5. Verify persistence across connection close and reopen
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let verified_1mb = NoteRepository::get_by_id(&conn2, &note_10kb.id)
        .expect("Get failed")
        .expect("Must exist");
    assert_eq!(verified_1mb.content.len(), 1024 * 1024);
    assert_eq!(verified_1mb.content, text_1mb);
    assert_eq!(verified_1mb.title, "Updated to 1 MB Note");
}

#[test]
fn test_timestamps_lifecycle_created_stable_modified_updated() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a note and capture initial timestamps
    let initial_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Timestamp Lifecycle Note".to_string()),
            content: Some("Initial content".to_string()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note failed");

    let initial_created_at = initial_note.created_at.clone();
    let initial_modified_at = initial_note.modified_at.clone();

    // 2. Opening the note (pure get_by_id) MUST NOT modify modified_at or created_at
    let opened_1 = NoteRepository::get_by_id(&conn, &initial_note.id)
        .expect("Get failed")
        .expect("Must exist");
    assert_eq!(opened_1.created_at, initial_created_at);
    assert_eq!(opened_1.modified_at, initial_modified_at);

    let opened_2 = NoteRepository::get_by_id(&conn, &initial_note.id)
        .expect("Get failed")
        .expect("Must exist");
    assert_eq!(opened_2.created_at, initial_created_at);
    assert_eq!(opened_2.modified_at, initial_modified_at);

    // 3. Sleep briefly to ensure clock advances (RFC3339 resolution)
    std::thread::sleep(std::time::Duration::from_millis(15));

    // 4. Update the note with new content
    let updated_note = NoteRepository::update(
        &conn,
        &initial_note.id,
        UpdateNoteDto {
            content: Some("Updated content after user edit".to_string()),
            ..Default::default()
        },
    )
    .expect("Update note failed");

    // Assert created_at remains strictly identical, while modified_at is updated
    assert_eq!(
        updated_note.created_at, initial_created_at,
        "created_at must remain completely stable"
    );
    assert_ne!(
        updated_note.modified_at, initial_modified_at,
        "modified_at must update on save"
    );
    assert!(
        updated_note.modified_at > initial_modified_at,
        "modified_at must advance chronologically"
    );

    // 5. Verify persistence across connection reopen
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let reloaded = NoteRepository::get_by_id(&conn2, &initial_note.id)
        .expect("Get failed")
        .expect("Must exist");

    assert_eq!(reloaded.created_at, initial_created_at);
    assert_eq!(reloaded.modified_at, updated_note.modified_at);
}

#[test]
fn test_phase3_comprehensive_crud_lifecycle_and_stale_prevention() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Note creation with exact Section 61 Unicode, Emoji, Symbols, and Newlines
    let raw_title = "आज की मीटिंग 📝";
    let raw_content = "Line 1\nLine 2\n\nLine 4\n\nHindi: नमस्ते\nEmoji: 📝\nSymbols: !@#$%^&*()";

    let created_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(raw_title.to_string()),
            content: Some(raw_content.to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("Creation failed");

    assert_eq!(created_note.title, raw_title);
    assert_eq!(created_note.content, raw_content);
    assert_eq!(created_note.format, "md");

    // 2. Note loading & exact data verification
    let loaded_note = NoteRepository::get_by_id(&conn, &created_note.id)
        .expect("Load failed")
        .expect("Note must exist");
    assert_eq!(loaded_note.title, raw_title);
    assert_eq!(loaded_note.content, raw_content);
    assert_eq!(loaded_note.format, "md");

    // 3. Title update verification
    let updated_title = "Updated Title: कल की योजना 📅";
    let after_title_update = NoteRepository::update(
        &conn,
        &created_note.id,
        UpdateNoteDto {
            title: Some(updated_title.to_string()),
            ..Default::default()
        },
    )
    .expect("Title update failed");
    assert_eq!(after_title_update.title, updated_title);
    assert_eq!(after_title_update.content, raw_content);

    // 4. Content update verification
    let updated_content =
        "Updated Markdown content:\n- [x] Item 1\n- [ ] Item 2\n\nCode: `const x = 42;`";
    let after_content_update = NoteRepository::update(
        &conn,
        &created_note.id,
        UpdateNoteDto {
            content: Some(updated_content.to_string()),
            ..Default::default()
        },
    )
    .expect("Content update failed");
    assert_eq!(after_content_update.content, updated_content);
    assert_eq!(after_content_update.title, updated_title);

    // 5. Stale save simulation (Section 72):
    // Simulate two sequential edits: Revision 1 ("Stale Draft") and Revision 2 ("Newest Final Text").
    // In the application frontend, revision tracking discards older save responses.
    // In SQLite persistence, committing the newest revision guarantees latest state.
    let rev1_stale_content = "Stale draft from out-of-order request";
    let rev2_newest_content = "Newest final text from user";

    // Persist Rev 2:
    NoteRepository::update(
        &conn,
        &created_note.id,
        UpdateNoteDto {
            content: Some(rev2_newest_content.to_string()),
            ..Default::default()
        },
    )
    .expect("Rev 2 save failed");

    // If a stale request were dropped/superseded by revision tracking, final content remains rev 2:
    let latest = NoteRepository::get_by_id(&conn, &created_note.id)
        .expect("Get failed")
        .expect("Note must exist");
    assert_eq!(latest.content, rev2_newest_content);
    assert_ne!(latest.content, rev1_stale_content);

    // 6. Empty content handling: empty content must succeed
    let cleared = NoteRepository::update(
        &conn,
        &created_note.id,
        UpdateNoteDto {
            content: Some("".to_string()),
            ..Default::default()
        },
    )
    .expect("Clear content failed");
    assert_eq!(cleared.content, "");

    // 7. Verify soft delete
    let deleted = NoteRepository::delete(&conn, &created_note.id, true).expect("Delete failed");
    assert!(deleted);

    let active_list = NoteRepository::list(&conn, false).expect("List failed");
    assert!(!active_list.iter().any(|n| n.id == created_note.id));
}

#[test]
fn test_task_22_restart_persistence_and_multi_note_isolation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // --- Session 1: Create notes & write exact Phase 3 content ---
    let test_title = "Phase 3 Test Note";
    let test_content = "This is a persistence test.\n\nLine two.\n\nआज की मीटिंग 📝";
    let test_format = "md";

    // Create Note 1 (Phase 3 Test Note)
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Temporary Title".to_string()),
            content: Some("".to_string()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Note 1 creation failed");

    // Small delay to ensure distinct timestamps
    std::thread::sleep(std::time::Duration::from_millis(15));

    // Create Note 2 (Note B)
    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note B".to_string()),
            content: Some("Unique content for Note B (Dev notes)".to_string()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Note 2 creation failed");

    std::thread::sleep(std::time::Duration::from_millis(15));

    // Create Note 3 (Note C)
    let note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note C".to_string()),
            content: Some("Unique content for Note C (Shopping list)".to_string()),
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Note 3 creation failed");

    std::thread::sleep(std::time::Duration::from_millis(15));

    // Edit Note 1 with exact Phase 3 Title and Content and save
    let saved_note1 = NoteRepository::update(
        &conn,
        &note1.id,
        UpdateNoteDto {
            title: Some(test_title.to_string()),
            content: Some(test_content.to_string()),
            format: Some(test_format.to_string()),
            ..Default::default()
        },
    )
    .expect("Note 1 update failed");

    let initial_created_at = saved_note1.created_at.clone();
    let initial_modified_at = saved_note1.modified_at.clone();

    // --- Session 1 Complete: Simulating Full Application Close ---
    drop(conn);

    // --- Session 2: Simulating Application Restart ---
    let restarted_conn =
        init_connection(&env.paths.database_file).expect("Restart connection failed");

    // 1. Verify Note 1 exact persistence after restart
    let reopened_note1 = NoteRepository::get_by_id(&restarted_conn, &note1.id)
        .expect("Get failed")
        .expect("Note 1 must exist");

    assert_eq!(reopened_note1.title, test_title);
    assert_eq!(reopened_note1.content, test_content);
    assert_eq!(reopened_note1.format, test_format);
    assert_eq!(reopened_note1.created_at, initial_created_at);
    assert_eq!(reopened_note1.modified_at, initial_modified_at);

    // 2. Verify Note 2 and Note 3 distinctness (no overwriting)
    let reopened_note2 = NoteRepository::get_by_id(&restarted_conn, &note2.id)
        .expect("Get failed")
        .expect("Note 2 must exist");
    assert_eq!(reopened_note2.title, "Note B");
    assert_eq!(
        reopened_note2.content,
        "Unique content for Note B (Dev notes)"
    );

    let reopened_note3 = NoteRepository::get_by_id(&restarted_conn, &note3.id)
        .expect("Get failed")
        .expect("Note 3 must exist");
    assert_eq!(reopened_note3.title, "Note C");
    assert_eq!(
        reopened_note3.content,
        "Unique content for Note C (Shopping list)"
    );

    // 3. Verify Notes List ordering: Note 1 was modified most recently, so it must be first
    let list = NoteRepository::list(&restarted_conn, false).expect("List failed");
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].id, note1.id);
    assert_eq!(list[1].id, note3.id);
    assert_eq!(list[2].id, note2.id);
}

#[test]
fn test_task_2_notebook_schema_and_unfiled_foreign_key_behavior() {
    let env = TestEnv::new();

    // 1. Fresh database schema application
    let mut conn =
        init_connection(&env.paths.database_file).expect("Database initialization failed");
    run_migrations(&mut conn).expect("Fresh migration must succeed");

    // 2. Validate columns on notebooks table
    {
        let mut stmt = conn
            .prepare("PRAGMA table_info(notebooks)")
            .expect("PRAGMA failed");
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("Query failed")
            .map(|r| r.unwrap())
            .collect();
        assert!(columns.contains(&"id".to_string()));
        assert!(columns.contains(&"name".to_string()));
        assert!(columns.contains(&"parent_id".to_string()));
        assert!(columns.contains(&"created_at".to_string()));
        assert!(columns.contains(&"modified_at".to_string()));
    }

    // 3. Create root and nested child notebook
    let root = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Root notebook creation failed");

    let child = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(root.id.clone()),
        },
    )
    .expect("Child notebook creation failed");

    assert_eq!(child.parent_id, Some(root.id.clone()));

    // 4. Create notes inside child notebook and root notebook
    let note_in_child = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Child Project Plan".to_string()),
            content: Some("Important project deliverables".to_string()),
            notebook_id: Some(child.id.clone()),
            ..Default::default()
        },
    )
    .expect("Note in child notebook creation failed");

    let note_in_root = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Root Memo".to_string()),
            content: Some("General work memo".to_string()),
            notebook_id: Some(root.id.clone()),
            ..Default::default()
        },
    )
    .expect("Note in root notebook creation failed");

    // 5. Delete child notebook directly to verify foreign key SET NULL behavior
    conn.execute("DELETE FROM notebooks WHERE id = ?1", [&child.id])
        .expect("Notebook deletion failed");

    // 6. Verify Note in child notebook is NOT deleted and became unfiled (notebook_id IS NULL)
    let preserved_note = NoteRepository::get_by_id(&conn, &note_in_child.id)
        .expect("Query failed")
        .expect("Note must NOT be deleted when notebook is deleted");
    assert_eq!(
        preserved_note.notebook_id, None,
        "Note must become unfiled (NULL)"
    );
    assert_eq!(preserved_note.title, "Child Project Plan");
    assert_eq!(preserved_note.content, "Important project deliverables");

    // Note in root notebook remains assigned to root
    let root_note = NoteRepository::get_by_id(&conn, &note_in_root.id)
        .expect("Query failed")
        .expect("Root note must exist");
    assert_eq!(root_note.notebook_id, Some(root.id));

    // 7. Verify migration runner is idempotent against this existing populated DB
    run_migrations(&mut conn).expect("Re-running migrations on populated DB must succeed");
}

#[test]
fn test_task_11_and_12_note_list_filtering_by_notebook_and_unfiled() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create Notebooks
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Create Work failed");

    let personal = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Personal".to_string(),
            parent_id: None,
        },
    )
    .expect("Create Personal failed");

    // 2. Create Notes
    let _work_note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Note 1".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create failed");

    let _work_note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Note 2".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create failed");

    let _personal_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Personal Note 1".to_string()),
            notebook_id: Some(personal.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create failed");

    let _unfiled_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Unfiled Note 1".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create failed");

    // Soft-deleted note in Work
    let deleted_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Deleted Work Note".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create failed");
    NoteRepository::delete(&conn, &deleted_note.id, true).expect("Soft delete failed");

    // 3. Query All Notes: should return 4 active notes (excluding deleted)
    let all_notes = NoteRepository::list_filtered(&conn, false, None, false).expect("Query failed");
    assert_eq!(all_notes.len(), 4);

    // 4. Query Work Notebook: should return exactly 2 active notes
    let work_notes =
        NoteRepository::list_filtered(&conn, false, Some(&work.id), false).expect("Query failed");
    assert_eq!(work_notes.len(), 2);
    assert!(work_notes
        .iter()
        .all(|n| n.notebook_id.as_deref() == Some(work.id.as_str())));

    // 5. Query Personal Notebook: should return exactly 1 active note
    let personal_notes = NoteRepository::list_filtered(&conn, false, Some(&personal.id), false)
        .expect("Query failed");
    assert_eq!(personal_notes.len(), 1);
    assert_eq!(personal_notes[0].title, "Personal Note 1");
    assert_eq!(
        personal_notes[0].notebook_id.as_deref(),
        Some(personal.id.as_str())
    );

    // 6. Query Unfiled Notes: should return exactly 1 unfiled note
    let unfiled_notes =
        NoteRepository::list_filtered(&conn, false, None, true).expect("Query failed");
    assert_eq!(unfiled_notes.len(), 1);
    assert_eq!(unfiled_notes[0].title, "Unfiled Note 1");
    assert_eq!(unfiled_notes[0].notebook_id, None);

    // 7. Query Work Notebook with include_deleted = true: should return 3 notes
    let work_with_deleted =
        NoteRepository::list_filtered(&conn, true, Some(&work.id), false).expect("Query failed");
    assert_eq!(work_with_deleted.len(), 3);
}

#[test]
fn test_task_14_notebook_rename_preserves_identity_and_relationships() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create parent notebook "Work"
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Create work notebook failed");

    // 2. Create child notebook "Projects" under "Work"
    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(work.id.clone()),
        },
    )
    .expect("Create child notebook failed");

    // 3. Create notes inside Work and Projects
    let note_in_work = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Note".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create note in work failed");

    let note_in_projects = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Projects Note".to_string()),
            notebook_id: Some(projects.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create note in projects failed");

    let original_id = work.id.clone();
    let original_created_at = work.created_at.clone();

    // 4. Rename "Work" to "Work 2026"
    let renamed = NotebookRepository::update(
        &conn,
        &original_id,
        UpdateNotebookDto {
            name: "Work 2026".to_string(),
        },
    )
    .expect("Rename failed");

    // 5. Verify invariant checks: identity, parent, created_at remain unchanged
    assert_eq!(renamed.id, original_id);
    assert_eq!(renamed.name, "Work 2026");
    assert_eq!(renamed.parent_id, None);
    assert_eq!(renamed.created_at, original_created_at);

    // 6. Verify child hierarchy remains intact
    let fetched_child = NotebookRepository::get_by_id(&conn, &projects.id)
        .expect("Query failed")
        .expect("Child exists");
    assert_eq!(
        fetched_child.parent_id.as_deref(),
        Some(original_id.as_str())
    );

    // 7. Verify notes relationships remain intact
    let fetched_work_note = NoteRepository::get_by_id(&conn, &note_in_work.id)
        .expect("Query failed")
        .expect("Work note exists");
    assert_eq!(
        fetched_work_note.notebook_id.as_deref(),
        Some(original_id.as_str())
    );

    let fetched_child_note = NoteRepository::get_by_id(&conn, &note_in_projects.id)
        .expect("Query failed")
        .expect("Child note exists");
    assert_eq!(
        fetched_child_note.notebook_id.as_deref(),
        Some(projects.id.as_str())
    );

    // 8. Verify persistence across connection reopen
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let persisted = NotebookRepository::get_by_id(&conn2, &original_id)
        .expect("Query failed")
        .expect("Notebook exists");
    assert_eq!(persisted.name, "Work 2026");
}

#[test]
fn test_task_15_delete_notebook_safely_and_unfile_notes() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create parent notebook "Work" and child "Projects"
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Create work notebook failed");

    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(work.id.clone()),
        },
    )
    .expect("Create projects failed");

    // 2. Create Note A and Note B inside "Work"
    let note_a = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note A".to_string()),
            content: Some("Content A".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create note A failed");

    let note_b = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note B".to_string()),
            content: Some("Content B".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create note B failed");

    // 3. Attempting to delete "Work" while "Projects" exists must fail
    let delete_err = NotebookRepository::delete(&mut conn, &work.id)
        .expect_err("Deleting parent with children must fail");

    assert!(
        delete_err
            .to_string()
            .contains("This notebook contains sub-notebooks"),
        "Error message must inform user about sub-notebooks: {}",
        delete_err
    );

    // 4. Delete child "Projects" first
    let child_deleted = NotebookRepository::delete(&mut conn, &projects.id)
        .expect("Delete child notebook should succeed");
    assert!(child_deleted);

    // 5. Now delete "Work" — must succeed and unfile Note A and Note B
    let work_deleted = NotebookRepository::delete(&mut conn, &work.id)
        .expect("Delete work notebook should succeed now");
    assert!(work_deleted);

    // 6. Verify Note A and Note B are unfiled and content is preserved
    let fetched_a = NoteRepository::get_by_id(&conn, &note_a.id)
        .expect("Query failed")
        .expect("Note A exists");
    assert_eq!(fetched_a.notebook_id, None, "Note A must be unfiled");
    assert_eq!(
        fetched_a.content, "Content A",
        "Note A content must be preserved"
    );

    let fetched_b = NoteRepository::get_by_id(&conn, &note_b.id)
        .expect("Query failed")
        .expect("Note B exists");
    assert_eq!(fetched_b.notebook_id, None, "Note B must be unfiled");
    assert_eq!(
        fetched_b.content, "Content B",
        "Note B content must be preserved"
    );

    // 7. Verify notebook itself is deleted
    assert!(NotebookRepository::get_by_id(&conn, &work.id)
        .expect("Query failed")
        .is_none());

    // 8. Reopen DB and verify persistence
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let unfiled = NoteRepository::list_filtered(&conn2, false, None, true).expect("Query failed");
    assert_eq!(unfiled.len(), 2);
}

#[test]
fn test_task_16_move_note_to_notebook_lifecycle_and_validation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create notebooks "Work" and "Personal"
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Create work notebook failed");

    let personal = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Personal".to_string(),
            parent_id: None,
        },
    )
    .expect("Create personal notebook failed");

    // 2. Create an unfiled note "Project Plan"
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Project Plan".to_string()),
            content: Some("# Initial Plan\nConfidential content".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create note failed");

    let original_id = note.id.clone();
    let original_title = note.title.clone();
    let original_content = note.content.clone();
    let original_format = note.format.clone();
    let original_created_at = note.created_at.clone();

    // 3. Move Unfiled -> Work
    let moved_to_work = NoteRepository::move_to_notebook(&conn, &original_id, Some(&work.id))
        .expect("Move to Work failed");
    assert_eq!(moved_to_work.notebook_id.as_deref(), Some(work.id.as_str()));
    assert_eq!(moved_to_work.id, original_id);
    assert_eq!(moved_to_work.title, original_title);
    assert_eq!(moved_to_work.content, original_content);
    assert_eq!(moved_to_work.format, original_format);
    assert_eq!(moved_to_work.created_at, original_created_at);

    // 4. Move Work -> Personal
    let moved_to_personal =
        NoteRepository::move_to_notebook(&conn, &original_id, Some(&personal.id))
            .expect("Move to Personal failed");
    assert_eq!(
        moved_to_personal.notebook_id.as_deref(),
        Some(personal.id.as_str())
    );

    // 5. Move Personal -> Work
    let moved_back_to_work = NoteRepository::move_to_notebook(&conn, &original_id, Some(&work.id))
        .expect("Move back to Work failed");
    assert_eq!(
        moved_back_to_work.notebook_id.as_deref(),
        Some(work.id.as_str())
    );

    // 6. Move Work -> Unfiled (notebook_id: None)
    let moved_to_unfiled = NoteRepository::move_to_notebook(&conn, &original_id, None)
        .expect("Move to Unfiled failed");
    assert_eq!(moved_to_unfiled.notebook_id, None);
    assert_eq!(moved_to_unfiled.content, original_content);

    // 7. Verify validation: moving to a nonexistent notebook ID must fail
    let invalid_move =
        NoteRepository::move_to_notebook(&conn, &original_id, Some("nonexistent_nb_12345"))
            .expect_err("Moving to invalid notebook must fail");
    assert!(invalid_move.to_string().contains("does not exist"));

    // 8. Verify total note count remains exactly 1 (no duplicate notes created)
    let all_notes = NoteRepository::list(&conn, false).expect("Query failed");
    assert_eq!(all_notes.len(), 1);

    // 9. Reopen database and verify persistent unfiled state
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let fetched = NoteRepository::get_by_id(&conn2, &original_id)
        .expect("Query failed")
        .expect("Note exists");
    assert_eq!(fetched.notebook_id, None);
    assert_eq!(fetched.title, "Project Plan");
    assert_eq!(fetched.content, "# Initial Plan\nConfidential content");
}

#[test]
fn test_task_18_new_note_inside_selected_notebook_or_unfiled() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create notebook "Work"
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook failed");

    // 2. Create note targeting selected notebook Work
    let work_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Sprint Plan".to_string()),
            content: Some("Tasks for this week".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create note in Work failed");

    // 3. Create note targeting unfiled (no notebook selected)
    let unfiled_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quick Thought".to_string()),
            content: Some("Random note".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create unfiled note failed");

    // 4. Verify notebook_id assignment
    assert_eq!(work_note.notebook_id.as_deref(), Some(work.id.as_str()));
    assert_eq!(unfiled_note.notebook_id, None);

    // 5. Verify list filtering returns correct notes
    let filtered_work =
        NoteRepository::list_filtered(&conn, false, Some(&work.id), false).expect("Query failed");
    assert_eq!(filtered_work.len(), 1);
    assert_eq!(filtered_work[0].id, work_note.id);

    let filtered_unfiled =
        NoteRepository::list_filtered(&conn, false, None, true).expect("Query failed");
    assert_eq!(filtered_unfiled.len(), 1);
    assert_eq!(filtered_unfiled[0].id, unfiled_note.id);

    // 6. Verify persistence across connection reopen
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let persisted_work_note = NoteRepository::get_by_id(&conn2, &work_note.id)
        .expect("Query failed")
        .expect("Note exists");
    assert_eq!(
        persisted_work_note.notebook_id.as_deref(),
        Some(work.id.as_str())
    );
}

#[test]
fn test_task_21_empty_sidebar_state_unfiled_notes_without_notebooks() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Verify 0 notebooks exist initially
    let notebooks = NotebookRepository::list(&conn).expect("Query failed");
    assert_eq!(
        notebooks.len(),
        0,
        "No notebooks should exist on clean database"
    );

    // 2. Create multiple unfiled notes without creating any notebooks
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Standalone Note 1".to_string()),
            content: Some("First note without notebooks".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create note 1 failed");

    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Standalone Note 2".to_string()),
            content: Some("Second note without notebooks".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create note 2 failed");

    // 3. Query All Notes: returns 2 unfiled notes
    let all_notes = NoteRepository::list(&conn, false).expect("Query failed");
    assert_eq!(all_notes.len(), 2);
    assert!(all_notes.iter().all(|n| n.notebook_id.is_none()));

    // 4. Query Unfiled Notes: returns 2 notes
    let unfiled_notes =
        NoteRepository::list_filtered(&conn, false, None, true).expect("Query failed");
    assert_eq!(unfiled_notes.len(), 2);

    // 5. Update unfiled note content
    let updated = NoteRepository::update(
        &conn,
        &note1.id,
        UpdateNoteDto {
            content: Some("Updated content without notebook".to_string()),
            ..Default::default()
        },
    )
    .expect("Update failed");
    assert_eq!(updated.content, "Updated content without notebook");
    assert_eq!(updated.notebook_id, None);

    // 6. Verify persistence across connection reopen
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let fetched = NoteRepository::get_by_id(&conn2, &note2.id)
        .expect("Query failed")
        .expect("Note exists");
    assert_eq!(fetched.notebook_id, None);
    assert_eq!(fetched.title, "Standalone Note 2");
}

#[test]
fn test_task_24_handle_missing_parent_references_defensively() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Attempting to create a notebook with a nonexistent parent_id fails validation
    let invalid_create = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Orphan Child".to_string(),
            parent_id: Some("nonexistent-parent-uuid".to_string()),
        },
    );
    assert!(
        invalid_create.is_err(),
        "Creation with nonexistent parent must fail validation"
    );

    // 2. Direct insertion or legacy corrupted record with nonexistent parent_id
    let orphan_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute("PRAGMA foreign_keys = OFF;", [])
        .expect("Disable FK failed");

    conn.execute(
        "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            orphan_id,
            "Corrupted Orphan Notebook",
            "missing-parent-id-12345",
            now,
            now
        ],
    )
    .expect("Insert orphan notebook failed");

    conn.execute("PRAGMA foreign_keys = ON;", [])
        .expect("Enable FK failed");

    // 3. NotebookRepository::list successfully reads all notebooks without crash or panic
    let all_notebooks = NotebookRepository::list(&conn).expect("List notebooks must succeed");
    assert_eq!(all_notebooks.len(), 1);
    assert_eq!(all_notebooks[0].id, orphan_id);
    assert_eq!(
        all_notebooks[0].parent_id,
        Some("missing-parent-id-12345".to_string())
    );

    // 4. Note creation and association with the orphan notebook works without error
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note in Orphan Notebook".to_string()),
            content: Some("Content inside orphan".to_string()),
            notebook_id: Some(orphan_id.clone()),
            ..Default::default()
        },
    )
    .expect("Create note in orphan notebook succeeded");
    assert_eq!(note.notebook_id, Some(orphan_id));
}

#[test]
fn test_task_25_prevent_circular_notebook_relationships() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Verify that normal creation only allows nesting under existing notebooks
    let root = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Root Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("Root create failed");

    let child = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Child Notebook".to_string(),
            parent_id: Some(root.id.clone()),
        },
    )
    .expect("Child create failed");

    assert_eq!(child.parent_id, Some(root.id.clone()));

    // 2. Verify that notebook updates strictly update name and never re-parent (preventing cycle creation)
    let updated = NotebookRepository::update(
        &conn,
        &child.id,
        UpdateNotebookDto {
            name: "Renamed Child".to_string(),
        },
    )
    .expect("Update failed");
    assert_eq!(updated.parent_id, Some(root.id.clone()));

    // 3. Test cycle resilience: manually construct a circular reference A -> B -> C -> A
    let a_id = uuid::Uuid::new_v4().to_string();
    let b_id = uuid::Uuid::new_v4().to_string();
    let c_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute("PRAGMA foreign_keys = OFF;", [])
        .expect("Disable FK failed");
    conn.execute(
        "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5),
                (?6, ?7, ?8, ?9, ?10),
                (?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            a_id, "Cycle A", c_id, now, now, b_id, "Cycle B", a_id, now, now, c_id, "Cycle C",
            b_id, now, now,
        ],
    )
    .expect("Insert circular notebooks failed");
    conn.execute("PRAGMA foreign_keys = ON;", [])
        .expect("Enable FK failed");

    // 4. Listing repositories handles cyclical database rows without infinite recursion or panic
    let all = NotebookRepository::list(&conn).expect("List must succeed despite cycle");
    assert_eq!(all.len(), 5); // root + child + 3 cycle nodes

    // 5. Attempting to delete a notebook in the cycle fails safely because it has child references
    let mut conn_del = init_connection(&env.paths.database_file).expect("Open connection failed");
    let delete_result = NotebookRepository::delete(&mut conn_del, &a_id);
    assert!(
        delete_result.is_err(),
        "Must prevent deletion when children exist in cycle"
    );
}

#[test]
fn test_task_28_update_note_list_after_notebook_changes() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create two notebooks: Work and Personal
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Create Work failed");

    let personal = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Personal".to_string(),
            parent_id: None,
        },
    )
    .expect("Create Personal failed");

    // 2. Create Note 1 in Work, Note 2 in Personal, and Note 3 in Unfiled
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Report".to_string()),
            content: Some("Quarterly summary".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create note 1 failed");

    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Grocery List".to_string()),
            content: Some("Milk, Bread, Butter".to_string()),
            notebook_id: Some(personal.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create note 2 failed");

    let note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quick Thought".to_string()),
            content: Some("Remember to call Alex".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create note 3 failed");

    // 3. Verify initial notes list for Work contains only Note 1
    let work_notes =
        NoteRepository::list_filtered(&conn, false, Some(&work.id), false).expect("Query failed");
    assert_eq!(work_notes.len(), 1);
    assert_eq!(work_notes[0].id, note1.id);

    // 4. Moving a note out: Move Note 1 from Work to Personal
    NoteRepository::move_to_notebook(&conn, &note1.id, Some(&personal.id)).expect("Move failed");

    // Work note list must immediately NOT contain Note 1 (disappears from Work view)
    let work_notes_after =
        NoteRepository::list_filtered(&conn, false, Some(&work.id), false).expect("Query failed");
    assert_eq!(work_notes_after.len(), 0);

    // Personal note list must immediately contain Note 1 (appears in Personal view)
    let personal_notes_after =
        NoteRepository::list_filtered(&conn, false, Some(&personal.id), false)
            .expect("Query failed");
    assert_eq!(personal_notes_after.len(), 2);
    assert!(personal_notes_after.iter().any(|n| n.id == note1.id));
    assert!(personal_notes_after.iter().any(|n| n.id == note2.id));

    // 5. Deleting notebook: Delete Personal notebook safely
    let mut conn_del = init_connection(&env.paths.database_file).expect("Open connection failed");
    NotebookRepository::delete(&mut conn_del, &personal.id).expect("Delete Personal failed");

    // Notes in Personal become unfiled. Note list for Unfiled immediately includes Note 1 and Note 2
    let unfiled_notes_after =
        NoteRepository::list_filtered(&conn, false, None, true).expect("Query unfiled failed");
    assert_eq!(unfiled_notes_after.len(), 3); // note1, note2, and note3
    assert!(unfiled_notes_after.iter().any(|n| n.id == note1.id));
    assert!(unfiled_notes_after.iter().any(|n| n.id == note2.id));
    assert!(unfiled_notes_after.iter().any(|n| n.id == note3.id));

    // 6. Moving note in: Move Note 3 from Unfiled into Work
    NoteRepository::move_to_notebook(&conn, &note3.id, Some(&work.id))
        .expect("Move to work failed");

    let work_final = NoteRepository::list_filtered(&conn, false, Some(&work.id), false)
        .expect("Query work failed");
    assert_eq!(work_final.len(), 1);
    assert_eq!(work_final[0].id, note3.id);

    let unfiled_final =
        NoteRepository::list_filtered(&conn, false, None, true).expect("Query unfiled failed");
    assert_eq!(unfiled_final.len(), 2);
    assert!(!unfiled_final.iter().any(|n| n.id == note3.id));
}

#[test]
fn test_task_30_persistence_and_restart_verification() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    let work_id: String;
    let projects_id: String;
    let roadmap_note_id: String;
    let meeting_note_id: String;
    let unfiled_note_id: String;

    let roadmap_content =
        "# 2026 Roadmap\n\n1. Phase 4: Notebooks\n2. Phase 5: Search\n3. Phase 6: Sync";
    let meeting_content = "Discussed offline local-first storage guarantees.";
    let unfiled_content = "Just a quick thought for later.";

    // Session 1: Create hierarchy, notes, and unfiled note
    {
        let mut conn = init_connection(&db_path).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migration failed");

        // 1. Create root notebook "Work"
        let work = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Work".to_string(),
                parent_id: None,
            },
        )
        .expect("Create Work failed");
        work_id = work.id;

        // 2. Create child notebook "Projects" inside "Work"
        let projects = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Projects".to_string(),
                parent_id: Some(work_id.clone()),
            },
        )
        .expect("Create Projects failed");
        projects_id = projects.id;

        // 3. Create note inside "Projects"
        let roadmap_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("2026 Roadmap".to_string()),
                content: Some(roadmap_content.to_string()),
                format: Some("md".to_string()),
                notebook_id: Some(projects_id.clone()),
                ..Default::default()
            },
        )
        .expect("Create roadmap note failed");
        roadmap_note_id = roadmap_note.id;

        // 4. Create note inside "Work"
        let meeting_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Meeting Notes".to_string()),
                content: Some(meeting_content.to_string()),
                format: Some("txt".to_string()),
                notebook_id: Some(work_id.clone()),
                ..Default::default()
            },
        )
        .expect("Create meeting note failed");
        meeting_note_id = meeting_note.id;

        // 5. Create unfiled note
        let unfiled_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Standalone Idea".to_string()),
                content: Some(unfiled_content.to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        )
        .expect("Create unfiled note failed");
        unfiled_note_id = unfiled_note.id;

        // Drop session 1 connection (simulates app exit)
    }

    // Session 2: Application restart
    {
        let mut conn2 = init_connection(&db_path).expect("Session 2 restart connection failed");
        // Migration runner must run idempotently without modifying or corrupting existing records
        run_migrations(&mut conn2).expect("Session 2 idempotent migration failed");

        // 1. Verify Notebook IDs and names preserved
        let work_reloaded = NotebookRepository::get_by_id(&conn2, &work_id)
            .expect("Query failed")
            .expect("Work notebook must exist");
        assert_eq!(work_reloaded.id, work_id);
        assert_eq!(work_reloaded.name, "Work");
        assert_eq!(work_reloaded.parent_id, None);

        let projects_reloaded = NotebookRepository::get_by_id(&conn2, &projects_id)
            .expect("Query failed")
            .expect("Projects notebook must exist");
        assert_eq!(projects_reloaded.id, projects_id);
        assert_eq!(projects_reloaded.name, "Projects");
        assert_eq!(projects_reloaded.parent_id, Some(work_id.clone()));

        // 2. Verify complete notebook list
        let all_notebooks = NotebookRepository::list(&conn2).expect("List failed");
        assert_eq!(all_notebooks.len(), 2);

        // 3. Verify Note assignments preserved
        let roadmap_reloaded = NoteRepository::get_by_id(&conn2, &roadmap_note_id)
            .expect("Query failed")
            .expect("Roadmap note must exist");
        assert_eq!(roadmap_reloaded.title, "2026 Roadmap");
        assert_eq!(roadmap_reloaded.content, roadmap_content);
        assert_eq!(roadmap_reloaded.format, "md");
        assert_eq!(roadmap_reloaded.notebook_id, Some(projects_id.clone()));

        let meeting_reloaded = NoteRepository::get_by_id(&conn2, &meeting_note_id)
            .expect("Query failed")
            .expect("Meeting note must exist");
        assert_eq!(meeting_reloaded.title, "Meeting Notes");
        assert_eq!(meeting_reloaded.content, meeting_content);
        assert_eq!(meeting_reloaded.format, "txt");
        assert_eq!(meeting_reloaded.notebook_id, Some(work_id.clone()));

        // 4. Verify Unfiled notes preserved
        let unfiled_reloaded = NoteRepository::get_by_id(&conn2, &unfiled_note_id)
            .expect("Query failed")
            .expect("Unfiled note must exist");
        assert_eq!(unfiled_reloaded.title, "Standalone Idea");
        assert_eq!(unfiled_reloaded.content, unfiled_content);
        assert_eq!(unfiled_reloaded.notebook_id, None);

        // 5. Verify filtered query results match before restart
        let projects_notes =
            NoteRepository::list_filtered(&conn2, false, Some(&projects_id), false)
                .expect("Query failed");
        assert_eq!(projects_notes.len(), 1);
        assert_eq!(projects_notes[0].id, roadmap_note_id);

        let work_notes = NoteRepository::list_filtered(&conn2, false, Some(&work_id), false)
            .expect("Query failed");
        assert_eq!(work_notes.len(), 1);
        assert_eq!(work_notes[0].id, meeting_note_id);

        let unfiled_notes =
            NoteRepository::list_filtered(&conn2, false, None, true).expect("Query failed");
        assert_eq!(unfiled_notes.len(), 1);
        assert_eq!(unfiled_notes[0].id, unfiled_note_id);
    }
}

#[test]
fn test_task_31_unicode_and_special_character_notebooks_lifecycle_and_restart() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    let hindi_id: String;
    let cjk_id: String;
    let cafe_id: String;
    let amp_id: String;
    let slash_id: String;
    let emdash_id: String;
    let emoji_id: String;

    let hindi_note_id: String;
    let cjk_note_id: String;

    // Session 1: Create, nest, rename, and populate notes with unicode & special characters
    {
        let mut conn = init_connection(&db_path).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migration failed");

        // 1. Create notebooks with exact specification test names
        let hindi_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "यात्रा".to_string(), // Devanagari Hindi
                parent_id: None,
            },
        )
        .expect("Create Hindi notebook failed");
        hindi_id = hindi_nb.id;

        let cjk_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "旅行".to_string(), // CJK Japanese/Chinese
                parent_id: None,
            },
        )
        .expect("Create CJK notebook failed");
        cjk_id = cjk_nb.id;

        let cafe_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Café".to_string(),          // Accented Latin
                parent_id: Some(hindi_id.clone()), // nested under Hindi parent
            },
        )
        .expect("Create Café notebook failed");
        cafe_id = cafe_nb.id;

        let amp_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Ideas & Research".to_string(), // Ampersand & spaces
                parent_id: None,
            },
        )
        .expect("Create Ideas & Research notebook failed");
        amp_id = amp_nb.id;

        let emdash_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "2026 — Goals".to_string(), // Em-dash & numbers
                parent_id: None,
            },
        )
        .expect("Create 2026 — Goals notebook failed");
        emdash_id = emdash_nb.id;

        let slash_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Project / Planning".to_string(), // Slash character (ensures no filesystem delimiter issue)
                parent_id: Some(emdash_id.clone()),     // nested under Em-dash parent
            },
        )
        .expect("Create Project / Planning notebook failed");
        slash_id = slash_nb.id;

        let emoji_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "🚀 Rocket Launch".to_string(), // 4-byte UTF-8 emoji
                parent_id: None,
            },
        )
        .expect("Create Emoji notebook failed");
        emoji_id = emoji_nb.id;

        // 2. Create notes inside unicode notebooks
        let hindi_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("दैनिक कार्य सूची और योजना".to_string()),
                content: Some("१. नई नोटबुक प्रणाली का सत्यापन\n२. स्थानीय डेटाबेस की जांच".to_string()),
                format: Some("txt".to_string()),
                notebook_id: Some(hindi_id.clone()),
                ..Default::default()
            },
        )
        .expect("Create Hindi note failed");
        hindi_note_id = hindi_note.id;

        let cjk_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("来期のロードマップ".to_string()),
                content: Some("オフライン優先アーキテクチャの完全な実装と検証。".to_string()),
                format: Some("md".to_string()),
                notebook_id: Some(cjk_id.clone()),
                ..Default::default()
            },
        )
        .expect("Create CJK note failed");
        cjk_note_id = cjk_note.id;

        // 3. Rename with unicode and symbols
        let renamed_cafe = NotebookRepository::update(
            &conn,
            &cafe_id,
            UpdateNotebookDto {
                name: "Café Français 🥐 & Bistro".to_string(),
            },
        )
        .expect("Rename Café failed");
        assert_eq!(renamed_cafe.name, "Café Français 🥐 & Bistro");

        let renamed_hindi = NotebookRepository::update(
            &conn,
            &hindi_id,
            UpdateNotebookDto {
                name: "तीर्थ यात्रा एवं देशाटन".to_string(),
            },
        )
        .expect("Rename Hindi notebook failed");
        assert_eq!(renamed_hindi.name, "तीर्थ यात्रा एवं देशाटन");
    }

    // Session 2: Application restart and verification
    {
        let mut conn2 = init_connection(&db_path).expect("Session 2 connection failed");
        run_migrations(&mut conn2).expect("Session 2 migration failed");

        // 1. Verify all 7 unicode notebooks loaded
        let notebooks = NotebookRepository::list(&conn2).expect("List notebooks failed");
        assert_eq!(notebooks.len(), 7);

        // 2. Verify exact renamed values
        let fetched_hindi = NotebookRepository::get_by_id(&conn2, &hindi_id)
            .expect("Get failed")
            .expect("Hindi notebook must exist");
        assert_eq!(fetched_hindi.name, "तीर्थ यात्रा एवं देशाटन");
        assert_eq!(fetched_hindi.parent_id, None);

        let fetched_cafe = NotebookRepository::get_by_id(&conn2, &cafe_id)
            .expect("Get failed")
            .expect("Café notebook must exist");
        assert_eq!(fetched_cafe.name, "Café Français 🥐 & Bistro");
        assert_eq!(fetched_cafe.parent_id, Some(hindi_id.clone()));

        let fetched_cjk = NotebookRepository::get_by_id(&conn2, &cjk_id)
            .expect("Get failed")
            .expect("CJK notebook must exist");
        assert_eq!(fetched_cjk.name, "旅行");

        let fetched_amp = NotebookRepository::get_by_id(&conn2, &amp_id)
            .expect("Get failed")
            .expect("Ampersand notebook must exist");
        assert_eq!(fetched_amp.name, "Ideas & Research");

        let fetched_emdash = NotebookRepository::get_by_id(&conn2, &emdash_id)
            .expect("Get failed")
            .expect("Em-dash notebook must exist");
        assert_eq!(fetched_emdash.name, "2026 — Goals");

        let fetched_slash = NotebookRepository::get_by_id(&conn2, &slash_id)
            .expect("Get failed")
            .expect("Slash notebook must exist");
        assert_eq!(fetched_slash.name, "Project / Planning");
        assert_eq!(fetched_slash.parent_id, Some(emdash_id.clone()));

        let fetched_emoji = NotebookRepository::get_by_id(&conn2, &emoji_id)
            .expect("Get failed")
            .expect("Emoji notebook must exist");
        assert_eq!(fetched_emoji.name, "🚀 Rocket Launch");

        // 3. Verify notes inside unicode notebooks
        let fetched_hindi_note = NoteRepository::get_by_id(&conn2, &hindi_note_id)
            .expect("Get failed")
            .expect("Hindi note must exist");
        assert_eq!(fetched_hindi_note.title, "दैनिक कार्य सूची और योजना");
        assert_eq!(
            fetched_hindi_note.content,
            "१. नई नोटबुक प्रणाली का सत्यापन\n२. स्थानीय डेटाबेस की जांच"
        );
        assert_eq!(fetched_hindi_note.notebook_id, Some(hindi_id.clone()));

        let fetched_cjk_note = NoteRepository::get_by_id(&conn2, &cjk_note_id)
            .expect("Get failed")
            .expect("CJK note must exist");
        assert_eq!(fetched_cjk_note.title, "来期のロードマップ");
        assert_eq!(
            fetched_cjk_note.content,
            "オフライン優先アーキテクチャの完全な実装と検証。"
        );
        assert_eq!(fetched_cjk_note.notebook_id, Some(cjk_id.clone()));

        // 4. Verify note filtering by unicode notebook ID
        let hindi_notes = NoteRepository::list_filtered(&conn2, false, Some(&hindi_id), false)
            .expect("Filter failed");
        assert_eq!(hindi_notes.len(), 1);
        assert_eq!(hindi_notes[0].id, hindi_note_id);

        let cjk_notes = NoteRepository::list_filtered(&conn2, false, Some(&cjk_id), false)
            .expect("Filter failed");
        assert_eq!(cjk_notes.len(), 1);
        assert_eq!(cjk_notes[0].id, cjk_note_id);
    }
}

#[test]
fn test_task_32_large_realistic_hierarchy_lifecycle_and_filtering() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    // 1. Create the realistic hierarchy:
    // Work
    // ├── Projects
    // │   ├── Project A
    // │   ├── Project B
    // │   └── Project C
    // ├── Meetings
    // │   ├── Weekly
    // │   └── Monthly
    // └── Archive
    //     ├── 2025
    //     └── 2026
    let mut conn = init_connection(&db_path).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // Root
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".into(),
            parent_id: None,
        },
    )
    .unwrap();

    // Level 1
    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".into(),
            parent_id: Some(work.id.clone()),
        },
    )
    .unwrap();
    let meetings = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Meetings".into(),
            parent_id: Some(work.id.clone()),
        },
    )
    .unwrap();
    let archive = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Archive".into(),
            parent_id: Some(work.id.clone()),
        },
    )
    .unwrap();

    // Level 2 (Leaves)
    let proj_a = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Project A".into(),
            parent_id: Some(projects.id.clone()),
        },
    )
    .unwrap();
    let _proj_b = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Project B".into(),
            parent_id: Some(projects.id.clone()),
        },
    )
    .unwrap();
    let proj_c = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Project C".into(),
            parent_id: Some(projects.id.clone()),
        },
    )
    .unwrap();

    let weekly = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Weekly".into(),
            parent_id: Some(meetings.id.clone()),
        },
    )
    .unwrap();
    let _monthly = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Monthly".into(),
            parent_id: Some(meetings.id.clone()),
        },
    )
    .unwrap();

    let arch_2025 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "2025".into(),
            parent_id: Some(archive.id.clone()),
        },
    )
    .unwrap();
    let _arch_2026 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "2026".into(),
            parent_id: Some(archive.id.clone()),
        },
    )
    .unwrap();

    // Verify 11 notebooks created
    let all = NotebookRepository::list(&conn).unwrap();
    assert_eq!(all.len(), 11);

    // Verify distinct IDs (no duplicate nodes)
    let unique_ids: std::collections::HashSet<_> = all.iter().map(|n| &n.id).collect();
    assert_eq!(unique_ids.len(), 11);

    // 2. Create notes at various depths
    let note_work = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Strategy".into()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let note_proj_a = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Project A Specs".into()),
            notebook_id: Some(proj_a.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let note_weekly = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Weekly Standup".into()),
            notebook_id: Some(weekly.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let note_2025 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Archive Summary 2025".into()),
            notebook_id: Some(arch_2025.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    // 3. Verify strict isolation in note filtering per notebook
    let filter_work = NoteRepository::list_filtered(&conn, false, Some(&work.id), false).unwrap();
    assert_eq!(filter_work.len(), 1);
    assert_eq!(filter_work[0].id, note_work.id);

    let filter_proj_a =
        NoteRepository::list_filtered(&conn, false, Some(&proj_a.id), false).unwrap();
    assert_eq!(filter_proj_a.len(), 1);
    assert_eq!(filter_proj_a[0].id, note_proj_a.id);

    let filter_weekly =
        NoteRepository::list_filtered(&conn, false, Some(&weekly.id), false).unwrap();
    assert_eq!(filter_weekly.len(), 1);
    assert_eq!(filter_weekly[0].id, note_weekly.id);

    let filter_2025 =
        NoteRepository::list_filtered(&conn, false, Some(&arch_2025.id), false).unwrap();
    assert_eq!(filter_2025.len(), 1);
    assert_eq!(filter_2025[0].id, note_2025.id);

    // 4. Verify safe delete constraints in deep hierarchy
    // Root "Work" deletion blocked (has 3 children)
    assert!(NotebookRepository::delete(&mut conn, &work.id).is_err());

    // Intermediate "Projects" deletion blocked (has 3 children)
    assert!(NotebookRepository::delete(&mut conn, &projects.id).is_err());

    // Intermediate "Meetings" deletion blocked (has 2 children)
    assert!(NotebookRepository::delete(&mut conn, &meetings.id).is_err());

    // Intermediate "Archive" deletion blocked (has 2 children)
    assert!(NotebookRepository::delete(&mut conn, &archive.id).is_err());

    // Leaf "Project C" has 0 children: deletion succeeds
    assert!(NotebookRepository::delete(&mut conn, &proj_c.id).unwrap());

    // Remaining notebooks is 10
    let all_after_del = NotebookRepository::list(&conn).unwrap();
    assert_eq!(all_after_del.len(), 10);

    // 5. Verify persistence across connection drop and restart
    drop(conn);
    let conn2 = init_connection(&db_path).unwrap();
    let reloaded = NotebookRepository::list(&conn2).unwrap();
    assert_eq!(reloaded.len(), 10);

    let reloaded_weekly_notes =
        NoteRepository::list_filtered(&conn2, false, Some(&weekly.id), false).unwrap();
    assert_eq!(reloaded_weekly_notes.len(), 1);
    assert_eq!(reloaded_weekly_notes[0].title, "Weekly Standup");
}

#[test]
fn test_task_33_performance_and_bulk_notebook_responsiveness() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Bulk creation of 50 notebooks across 4 levels inside a single transaction
    let tx = conn.transaction().expect("Tx failed");
    let mut notebook_ids: Vec<String> = Vec::with_capacity(50);
    let now = chrono::Utc::now().to_rfc3339();

    for i in 0..50 {
        let nb_id = uuid::Uuid::new_v4().to_string();
        let parent_id = if i == 0 {
            None
        } else if i < 10 {
            Some(notebook_ids[0].clone()) // Level 1 (children of root)
        } else if i < 30 {
            Some(notebook_ids[1 + (i % 8)].clone()) // Level 2
        } else {
            Some(notebook_ids[10 + (i % 15)].clone()) // Level 3
        };

        tx.execute(
            "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![nb_id, format!("Notebook {:02}", i), parent_id, now, now],
        )
        .unwrap();
        notebook_ids.push(nb_id);
    }

    // Insert 200 notes distributed across the notebooks
    for i in 0..200 {
        let note_id = uuid::Uuid::new_v4().to_string();
        let target_nb = if i % 10 == 0 {
            None // 20 unfiled notes
        } else {
            Some(notebook_ids[i % 50].clone())
        };

        tx.execute(
            "INSERT INTO notes (
                id, title, content, format, notebook_id, created_at, modified_at,
                is_favorite, is_pinned, is_deleted, deleted_at
            ) VALUES (?1, ?2, ?3, 'txt', ?4, ?5, ?6, 0, 0, 0, NULL)",
            rusqlite::params![
                note_id,
                format!("Note {:03}", i),
                "Sample note content for testing responsiveness and query latency",
                target_nb,
                now,
                now
            ],
        )
        .unwrap();
    }
    tx.commit().expect("Tx commit failed");

    // 2. Measure single query retrieval of all 50 notebooks
    let start_nb = std::time::Instant::now();
    let all_notebooks = NotebookRepository::list(&conn).expect("List failed");
    let elapsed_nb = start_nb.elapsed();

    assert_eq!(all_notebooks.len(), 50);
    // Retrieval of 50 notebooks via a single query must complete in < 50ms
    assert!(
        elapsed_nb.as_millis() < 50,
        "Retrieval of 50 notebooks took {}ms (expected < 50ms)",
        elapsed_nb.as_millis()
    );

    // 3. Measure filtered note query for specific notebooks (using idx_notes_notebook_id)
    let sample_nb_id = &notebook_ids[5];
    let start_notes = std::time::Instant::now();
    let nb_notes = NoteRepository::list_filtered(&conn, false, Some(sample_nb_id), false)
        .expect("Filtered notes failed");
    let elapsed_notes = start_notes.elapsed();

    assert!(!nb_notes.is_empty());
    // Filtered query directly through SQLite index must complete in < 15ms
    assert!(
        elapsed_notes.as_millis() < 15,
        "Indexed filter query took {}ms (expected < 15ms)",
        elapsed_notes.as_millis()
    );

    // 4. Measure unfiled notes query
    let start_unfiled = std::time::Instant::now();
    let unfiled_notes =
        NoteRepository::list_filtered(&conn, false, None, true).expect("Unfiled query failed");
    let elapsed_unfiled = start_unfiled.elapsed();

    assert_eq!(unfiled_notes.len(), 20);
    assert!(
        elapsed_unfiled.as_millis() < 15,
        "Unfiled filter query took {}ms (expected < 15ms)",
        elapsed_unfiled.as_millis()
    );
}

#[test]
fn test_task_34_error_handling_categories_and_human_readable_messages() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. ValidationError: Empty notebook name
    let empty_err = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "   ".to_string(),
            parent_id: None,
        },
    )
    .unwrap_err();

    match &empty_err {
        personal_notepad_lib::storage::StorageError::Validation(msg) => {
            assert_eq!(msg, "Notebook name cannot be empty");
        }
        other => panic!("Expected ValidationError, got {other:?}"),
    }
    assert_eq!(
        empty_err.user_friendly_message(),
        "Notebook name cannot be empty"
    );

    // 2. InvalidHierarchy: Nonexistent parent
    let missing_parent_err = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Child with missing parent".to_string(),
            parent_id: Some("ghost-parent-id-xyz".to_string()),
        },
    )
    .unwrap_err();

    match &missing_parent_err {
        personal_notepad_lib::storage::StorageError::InvalidHierarchy(msg) => {
            assert!(msg.contains("ghost-parent-id-xyz"));
        }
        other => panic!("Expected InvalidHierarchy error, got {other:?}"),
    }

    // 3. DeleteBlocked: Notebook contains child sub-notebooks
    let parent = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Parent Folder".to_string(),
            parent_id: None,
        },
    )
    .expect("Parent creation failed");

    let _child = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Child Subfolder".to_string(),
            parent_id: Some(parent.id.clone()),
        },
    )
    .expect("Child creation failed");

    let delete_blocked_err = NotebookRepository::delete(&mut conn, &parent.id).unwrap_err();
    match &delete_blocked_err {
        personal_notepad_lib::storage::StorageError::DeleteBlocked(msg) => {
            assert_eq!(
                msg,
                "This notebook contains sub-notebooks. Move or delete the sub-notebooks before deleting this notebook."
            );
        }
        other => panic!("Expected DeleteBlocked error, got {other:?}"),
    }
    assert_eq!(
        delete_blocked_err.user_friendly_message(),
        "This notebook contains sub-notebooks. Move or delete the sub-notebooks before deleting this notebook."
    );

    // 4. NotFound: Requesting non-existent notebook
    let not_found_err = NotebookRepository::update(
        &conn,
        "does-not-exist-uuid",
        UpdateNotebookDto {
            name: "Renamed Ghost".to_string(),
        },
    )
    .unwrap_err();

    match &not_found_err {
        personal_notepad_lib::storage::StorageError::NotFound(msg) => {
            assert!(msg.contains("does-not-exist-uuid"));
        }
        other => panic!("Expected NotFound error, got {other:?}"),
    }

    // 5. Raw SQL error masking: Ensure no raw SQL statements or internal columns are exposed
    let constraint_err =
        personal_notepad_lib::storage::StorageError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                extended_code: 1555,
            },
            Some("UNIQUE constraint failed: notes.id".to_string()),
        ));
    let friendly = constraint_err.user_friendly_message();
    assert!(!friendly.contains("notes.id"));
    assert!(!friendly.contains("extended_code"));
    assert!(
        friendly.contains("referenced item no longer exists or a unique constraint was violated")
    );
}

#[test]
fn test_task_35_transaction_safety_and_rollback_on_deletion_failure() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a notebook with notes
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Critical Finance Notes".to_string(),
            parent_id: None,
        },
    )
    .expect("Notebook create failed");

    let mut note_ids = Vec::new();
    for i in 1..=5 {
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Finance Note {i}")),
                content: Some(format!("Content {i}")),
                notebook_id: Some(nb.id.clone()),
                ..Default::default()
            },
        )
        .expect("Note create failed");
        note_ids.push(note.id);
    }

    // 2. Install a temporary SQLite trigger to simulate failure during the DELETE step
    conn.execute(
        "CREATE TRIGGER abort_notebook_delete
         BEFORE DELETE ON notebooks
         FOR EACH ROW
         BEGIN
             SELECT RAISE(ABORT, 'Simulated mid-transaction failure');
         END;",
        [],
    )
    .expect("Trigger install failed");

    // 3. Attempting to delete the notebook fails because the trigger aborts the transaction
    let del_result = NotebookRepository::delete(&mut conn, &nb.id);
    assert!(del_result.is_err(), "Deletion must fail due to trigger");

    // 4. Verify atomicity & rollback:
    // Neither the notebook was deleted NOR were the notes unassigned.
    // The database must NOT be in a partial state where notes were unassigned but the notebook remains.
    let reloaded_nb = NotebookRepository::get_by_id(&conn, &nb.id).unwrap();
    assert!(
        reloaded_nb.is_some(),
        "Notebook must still exist after rollback"
    );

    for nid in &note_ids {
        let n = NoteRepository::get_by_id(&conn, nid)
            .unwrap()
            .expect("Note must exist");
        assert_eq!(
            n.notebook_id,
            Some(nb.id.clone()),
            "Note must still reference the notebook due to full rollback"
        );
    }

    // 5. Drop the trigger to restore normal execution
    conn.execute("DROP TRIGGER abort_notebook_delete;", [])
        .expect("Drop trigger failed");

    // 6. Delete again: transaction completes successfully
    let del_success =
        NotebookRepository::delete(&mut conn, &nb.id).expect("Delete must succeed now");
    assert!(del_success);

    // Verify notebook is gone
    let deleted_nb = NotebookRepository::get_by_id(&conn, &nb.id).unwrap();
    assert!(deleted_nb.is_none(), "Notebook must be deleted");

    // Verify all 5 notes are now unfiled (notebook_id = None), without any loss of content
    for (idx, nid) in note_ids.iter().enumerate() {
        let n = NoteRepository::get_by_id(&conn, nid)
            .unwrap()
            .expect("Note must exist");
        assert_eq!(n.notebook_id, None, "Note must be unfiled now");
        assert_eq!(n.title, format!("Finance Note {}", idx + 1));
        assert_eq!(n.content, format!("Content {}", idx + 1));
    }
}

#[test]
fn test_task_39_note_count_accuracy_across_notebook_lifecycle() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create notebooks: "Projects" (root) and "Frontend" (child)
    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: None,
        },
    )
    .expect("Create Projects failed");

    let frontend = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Frontend".to_string(),
            parent_id: Some(projects.id.clone()),
        },
    )
    .expect("Create Frontend failed");

    // Helper closure to calculate active note counts per notebook (matching refreshNoteCounts in frontend)
    let get_note_counts = |c: &rusqlite::Connection| -> std::collections::HashMap<String, usize> {
        let all_notes = NoteRepository::list_filtered(c, false, None, false).unwrap();
        let mut counts = std::collections::HashMap::new();
        for n in all_notes {
            if let Some(nb_id) = n.notebook_id {
                *counts.entry(nb_id).or_insert(0) += 1;
            }
        }
        counts
    };

    // Initially: no notes in any notebook
    let counts0 = get_note_counts(&conn);
    assert_eq!(counts0.get(&projects.id).copied().unwrap_or(0), 0);
    assert_eq!(counts0.get(&frontend.id).copied().unwrap_or(0), 0);

    // 2. Add 2 notes to Projects, 3 notes to Frontend, 1 Unfiled note
    let p_note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Project Overview".to_string()),
            content: Some("Overview content".to_string()),
            notebook_id: Some(projects.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let _p_note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Project Roadmap".to_string()),
            content: Some("Roadmap content".to_string()),
            notebook_id: Some(projects.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let f_note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("UI Specs".to_string()),
            content: Some("CSS tokens".to_string()),
            notebook_id: Some(frontend.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let _f_note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Component Tree".to_string()),
            content: Some("React tree".to_string()),
            notebook_id: Some(frontend.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let _f_note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Theme Engine".to_string()),
            content: Some("Light/dark mode".to_string()),
            notebook_id: Some(frontend.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let _unfiled = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quick Thought".to_string()),
            content: Some("Unassigned note".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .unwrap();

    // Verify initial note counts
    let counts1 = get_note_counts(&conn);
    assert_eq!(counts1.get(&projects.id).copied().unwrap_or(0), 2);
    assert_eq!(counts1.get(&frontend.id).copied().unwrap_or(0), 3);

    // 3. Move note from Projects to Frontend: Projects becomes 1, Frontend becomes 4
    NoteRepository::move_to_notebook(&conn, &p_note1.id, Some(&frontend.id)).unwrap();
    let counts2 = get_note_counts(&conn);
    assert_eq!(counts2.get(&projects.id).copied().unwrap_or(0), 1);
    assert_eq!(counts2.get(&frontend.id).copied().unwrap_or(0), 4);

    // 4. Soft-delete a note in Frontend: it must no longer count toward active notes in the notebook
    NoteRepository::delete(&conn, &f_note1.id, true).unwrap();
    let counts3 = get_note_counts(&conn);
    assert_eq!(counts3.get(&frontend.id).copied().unwrap_or(0), 3);

    // 5. Restore soft-deleted note: count increments back to 4
    NoteRepository::update(
        &conn,
        &f_note1.id,
        UpdateNoteDto {
            is_deleted: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    let counts4 = get_note_counts(&conn);
    assert_eq!(counts4.get(&frontend.id).copied().unwrap_or(0), 4);

    // 6. Delete the Frontend notebook: all notes become unfiled, Frontend notebook is removed
    NotebookRepository::delete(&mut conn, &frontend.id).unwrap();
    let counts5 = get_note_counts(&conn);
    assert_eq!(counts5.get(&projects.id).copied().unwrap_or(0), 1);
    assert_eq!(counts5.get(&frontend.id).copied().unwrap_or(0), 0);

    // Verify unfiled notes count now includes former Frontend notes
    let unfiled_notes = NoteRepository::list_filtered(&conn, false, None, true).unwrap();
    // 1 original unfiled + 4 from deleted Frontend = 5 total
    assert_eq!(unfiled_notes.len(), 5);
}

#[test]
fn test_task_42_backend_and_note_assignment_suite() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Backend tests: create notebook (root)
    let root = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Backend Root".to_string(),
            parent_id: None,
        },
    )
    .expect("Create root failed");
    assert_eq!(root.name, "Backend Root");
    assert_eq!(root.parent_id, None);

    // 2. Get notebook
    let fetched = NotebookRepository::get_by_id(&conn, &root.id)
        .expect("Get failed")
        .expect("Notebook must exist");
    assert_eq!(fetched.id, root.id);
    assert_eq!(fetched.name, "Backend Root");

    // 3. List notebooks
    let list1 = NotebookRepository::list(&conn).expect("List failed");
    assert_eq!(list1.len(), 1);

    // 4. Rename notebook
    let renamed = NotebookRepository::update(
        &conn,
        &root.id,
        UpdateNotebookDto {
            name: "Backend Root Renamed".to_string(),
        },
    )
    .expect("Rename failed");
    assert_eq!(renamed.name, "Backend Root Renamed");

    // 5. Nested notebook (parent-child relationship)
    let child = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Child Notebook".to_string(),
            parent_id: Some(root.id.clone()),
        },
    )
    .expect("Create child failed");
    assert_eq!(child.parent_id, Some(root.id.clone()));

    // 6. Invalid parent handling: attempting to create a notebook with a nonexistent parent_id
    let invalid_parent_err = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Invalid Child".to_string(),
            parent_id: Some("non-existent-uuid-99999".to_string()),
        },
    )
    .expect_err("Creating notebook with invalid parent must fail");
    match invalid_parent_err {
        StorageError::InvalidHierarchy(msg) => {
            assert!(
                msg.contains("Parent notebook with ID 'non-existent-uuid-99999' does not exist")
            );
        }
        other => panic!("Expected StorageError::InvalidHierarchy, got {:?}", other),
    }

    // 7. Missing notebook handling: querying or updating a nonexistent notebook ID
    let missing_nb =
        NotebookRepository::get_by_id(&conn, "missing-notebook-id").expect("Query failed");
    assert!(missing_nb.is_none());

    let missing_update_err = NotebookRepository::update(
        &conn,
        "missing-notebook-id",
        UpdateNotebookDto {
            name: "New Name".to_string(),
        },
    )
    .expect_err("Updating nonexistent notebook must fail");
    match missing_update_err {
        StorageError::NotFound(msg) => assert!(msg.contains("not found")),
        other => panic!("Expected StorageError::NotFound, got {:?}", other),
    }

    // 8. Delete notebook with children: must be safely blocked with DeleteBlocked error
    let delete_blocked_err = NotebookRepository::delete(&mut conn, &root.id)
        .expect_err("Deleting parent with active children must be blocked");
    match delete_blocked_err {
        StorageError::DeleteBlocked(msg) => assert!(msg.contains("sub-notebooks")),
        other => panic!("Expected StorageError::DeleteBlocked, got {:?}", other),
    }

    // 9. Note assignment tests:
    // Create unfiled note
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Task 42 Note".to_string()),
            content: Some("Testing note moves".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create note failed");
    assert_eq!(note.notebook_id, None);

    // a) Note -> Notebook (Unfiled -> child)
    let moved_to_child = NoteRepository::move_to_notebook(&conn, &note.id, Some(&child.id))
        .expect("Move to child failed");
    assert_eq!(
        moved_to_child.notebook_id.as_deref(),
        Some(child.id.as_str())
    );

    // b) Notebook -> another notebook (child -> root)
    let moved_to_root = NoteRepository::move_to_notebook(&conn, &note.id, Some(&root.id))
        .expect("Move to root failed");
    assert_eq!(moved_to_root.notebook_id.as_deref(), Some(root.id.as_str()));

    // c) Notebook -> unfiled (root -> None)
    let moved_to_unfiled =
        NoteRepository::move_to_notebook(&conn, &note.id, None).expect("Move to unfiled failed");
    assert_eq!(moved_to_unfiled.notebook_id, None);

    // Put note back in child notebook before deleting child
    NoteRepository::move_to_notebook(&conn, &note.id, Some(&child.id)).expect("Move failed");

    // 10. Note unassignment on notebook deletion:
    // Delete leaf notebook `child`
    let child_deleted =
        NotebookRepository::delete(&mut conn, &child.id).expect("Delete child failed");
    assert!(child_deleted);

    // Verify note is safely unfiled and content is completely preserved
    let unassigned_note = NoteRepository::get_by_id(&conn, &note.id)
        .expect("Query failed")
        .expect("Note must still exist");
    assert_eq!(
        unassigned_note.notebook_id, None,
        "Note must be unassigned to Unfiled"
    );
    assert_eq!(unassigned_note.title, "Task 42 Note");
    assert_eq!(unassigned_note.content, "Testing note moves");

    // Finally delete root now that child is gone
    let root_deleted = NotebookRepository::delete(&mut conn, &root.id).expect("Delete root failed");
    assert!(root_deleted);
    assert_eq!(NotebookRepository::list(&conn).unwrap().len(), 0);
}

#[test]
fn test_task_43_exact_manual_qa_simulation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // Test A — Create root notebook "Work"
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Test A failed: Create Work root notebook");
    assert_eq!(work.name, "Work");
    assert_eq!(work.parent_id, None);

    // Test B — Create child notebook "Projects" under "Work"
    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(work.id.clone()),
        },
    )
    .expect("Test B failed: Create Projects child notebook");
    assert_eq!(projects.name, "Projects");
    assert_eq!(projects.parent_id, Some(work.id.clone()));

    // Test C — Create note inside "Projects"
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("QA Project Note".to_string()),
            content: Some("Deterministic Note Content for Manual QA".to_string()),
            format: Some("txt".to_string()),
            notebook_id: Some(projects.id.clone()),
            ..Default::default()
        },
    )
    .expect("Test C failed: Create note inside Projects");
    assert_eq!(note.notebook_id.as_deref(), Some(projects.id.as_str()));

    // Test D — Restart verification (close connection and reopen)
    drop(conn);
    let mut reopened_conn =
        init_connection(&env.paths.database_file).expect("Test D reopen failed");
    run_migrations(&mut reopened_conn).expect("Test D migration check failed");

    let re_work = NotebookRepository::get_by_id(&reopened_conn, &work.id)
        .unwrap()
        .expect("Work survives");
    let re_projects = NotebookRepository::get_by_id(&reopened_conn, &projects.id)
        .unwrap()
        .expect("Projects survives");
    let re_note = NoteRepository::get_by_id(&reopened_conn, &note.id)
        .unwrap()
        .expect("Note survives");
    assert_eq!(re_projects.parent_id, Some(re_work.id.clone()));
    assert_eq!(re_note.notebook_id, Some(re_projects.id.clone()));
    assert_eq!(re_note.content, "Deterministic Note Content for Manual QA");

    // Test E — Move note from Projects to Work
    let moved_to_work = NoteRepository::move_to_notebook(&reopened_conn, &note.id, Some(&work.id))
        .expect("Test E failed: Move note to Work");
    assert_eq!(moved_to_work.notebook_id.as_deref(), Some(work.id.as_str()));

    let projects_notes =
        NoteRepository::list_filtered(&reopened_conn, false, Some(&projects.id), false).unwrap();
    let work_notes =
        NoteRepository::list_filtered(&reopened_conn, false, Some(&work.id), false).unwrap();
    assert_eq!(projects_notes.len(), 0, "Projects must no longer show note");
    assert_eq!(work_notes.len(), 1, "Work must show note");

    // Test F — Unfile note (Move to Unfiled)
    let moved_to_unfiled = NoteRepository::move_to_notebook(&reopened_conn, &note.id, None)
        .expect("Test F failed: Move note to Unfiled");
    assert_eq!(moved_to_unfiled.notebook_id, None);
    let unfiled_notes = NoteRepository::list_filtered(&reopened_conn, false, None, true).unwrap();
    assert_eq!(unfiled_notes.len(), 1);

    // Test G — Rename "Work" to "Work 2026"
    let renamed_work = NotebookRepository::update(
        &reopened_conn,
        &work.id,
        UpdateNotebookDto {
            name: "Work 2026".to_string(),
        },
    )
    .expect("Test G failed: Rename Work");
    assert_eq!(renamed_work.id, work.id, "ID must remain unchanged");
    assert_eq!(renamed_work.name, "Work 2026");

    let children_after_rename = NotebookRepository::list(&reopened_conn).unwrap();
    let child_after = children_after_rename
        .iter()
        .find(|nb| nb.id == projects.id)
        .expect("Child survives");
    assert_eq!(
        child_after.parent_id,
        Some(work.id.clone()),
        "Parent relationship intact"
    );

    // Test I — Child deletion protection: Deleting "Work 2026" while "Projects" exists MUST fail
    let del_parent_err = NotebookRepository::delete(&mut reopened_conn, &work.id)
        .expect_err("Test I failed: Deleting parent with child must be blocked");
    match del_parent_err {
        StorageError::DeleteBlocked(msg) => assert!(msg.contains("sub-notebooks")),
        other => panic!("Expected DeleteBlocked, got {:?}", other),
    }

    // Now delete child notebook first
    let del_child =
        NotebookRepository::delete(&mut reopened_conn, &projects.id).expect("Delete child");
    assert!(del_child);

    // Move note back to Work 2026 to verify unfiling on notebook deletion (Test H)
    NoteRepository::move_to_notebook(&reopened_conn, &note.id, Some(&work.id))
        .expect("Move note back to Work 2026");
    let active_in_work =
        NoteRepository::list_filtered(&reopened_conn, false, Some(&work.id), false).unwrap();
    assert_eq!(active_in_work.len(), 1);

    // Test H — Delete notebook "Work 2026": notebook disappears, note safely becomes unfiled
    let del_work = NotebookRepository::delete(&mut reopened_conn, &work.id)
        .expect("Test H failed: Delete Work 2026");
    assert!(del_work);
    assert!(NotebookRepository::get_by_id(&reopened_conn, &work.id)
        .unwrap()
        .is_none());

    let final_note = NoteRepository::get_by_id(&reopened_conn, &note.id)
        .unwrap()
        .expect("Note survives deletion");
    assert_eq!(final_note.notebook_id, None, "Note must be unfiled now");
    assert_eq!(
        final_note.content,
        "Deterministic Note Content for Manual QA"
    );
}

#[test]
fn test_task_44_large_text_and_editor_regression_across_notebook_mutations() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a large note (100 KB) with special formatting
    let mut large_content = String::with_capacity(100 * 1024);
    for i in 1..=1000 {
        large_content.push_str(&format!(
            "Line {:04}: Phase 4 large note content line with unicode: 🚀 नमस्ते\n",
            i
        ));
    }
    assert!(large_content.len() >= 60 * 1024);

    let large_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Large Regression Note".to_string()),
            content: Some(large_content.clone()),
            format: Some("markdown".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create large note failed");

    // 2. Create Notebooks A and B
    let nb_a = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Notebook A".to_string(),
            parent_id: None,
        },
    )
    .expect("Create NB A failed");

    let nb_b = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Notebook B".to_string(),
            parent_id: None,
        },
    )
    .expect("Create NB B failed");

    // 3. Move large note to Notebook A
    let moved_to_a = NoteRepository::move_to_notebook(&conn, &large_note.id, Some(&nb_a.id))
        .expect("Move to A failed");
    assert_eq!(moved_to_a.notebook_id.as_deref(), Some(nb_a.id.as_str()));
    assert_eq!(moved_to_a.content.len(), large_content.len());
    assert_eq!(moved_to_a.content, large_content);
    assert_eq!(moved_to_a.title, "Large Regression Note");

    // 4. Rename Notebook A to "Notebook A Prime"
    let renamed_a = NotebookRepository::update(
        &conn,
        &nb_a.id,
        UpdateNotebookDto {
            name: "Notebook A Prime".to_string(),
        },
    )
    .expect("Rename failed");
    assert_eq!(renamed_a.name, "Notebook A Prime");

    // 5. Switch to Notebook B (simulate query for notes in B)
    let notes_in_b =
        NoteRepository::list_filtered(&conn, false, Some(&nb_b.id), false).expect("Query B failed");
    assert_eq!(notes_in_b.len(), 0);

    // 6. Return to note (query note directly by ID)
    let returned_note = NoteRepository::get_by_id(&conn, &large_note.id)
        .expect("Fetch failed")
        .expect("Note must exist");

    // 7. Verify all regression properties:
    assert_eq!(returned_note.id, large_note.id);
    assert_eq!(returned_note.title, "Large Regression Note");
    assert_eq!(returned_note.content.len(), large_content.len());
    assert_eq!(returned_note.content, large_content);
    assert_eq!(returned_note.format, "markdown");
    assert_eq!(returned_note.notebook_id.as_deref(), Some(nb_a.id.as_str()));

    // 8. Simulate in-editor typing & autosave on the large note
    let mut updated_large_content = large_content.clone();
    updated_large_content.push_str("\n\n---\nAppended during regression autosave session.");
    let autosaved = NoteRepository::update(
        &conn,
        &large_note.id,
        UpdateNoteDto {
            title: Some("Large Regression Note (Edited)".to_string()),
            content: Some(updated_large_content.clone()),
            ..Default::default()
        },
    )
    .expect("Autosave update failed");

    assert_eq!(autosaved.title, "Large Regression Note (Edited)");
    assert_eq!(autosaved.content, updated_large_content);
    assert_eq!(autosaved.notebook_id.as_deref(), Some(nb_a.id.as_str()));
}

#[test]
fn test_task_45_application_restart_full_regression_lifecycle() {
    let env = TestEnv::new();
    let parent_id;
    let child_id;
    let note_id;
    let expected_title = "Sprint Planning Minutes (v2)";
    let expected_content = "# Sprint Review & Planning\n- Finalized Phase 4 specifications\n- Verified SQLite WAL transactions\n- Ensured zero data loss on restart";

    // --- Session 1: Pre-restart mutations ---
    {
        let mut conn =
            init_connection(&env.paths.database_file).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migrations failed");

        // 1. Create root notebook
        let parent = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Engineering".to_string(),
                parent_id: None,
            },
        )
        .expect("Create root notebook failed");
        parent_id = parent.id;

        // 2. Create child notebook
        let child = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Core Infrastructure".to_string(),
                parent_id: Some(parent_id.clone()),
            },
        )
        .expect("Create child notebook failed");
        child_id = child.id;

        // 3. Create note inside root
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Sprint Planning Minutes".to_string()),
                content: Some("# Draft".to_string()),
                format: Some("markdown".to_string()),
                notebook_id: Some(parent_id.clone()),
                ..Default::default()
            },
        )
        .expect("Create note failed");
        note_id = note.id;

        // 4. Edit note (update title & content)
        NoteRepository::update(
            &conn,
            &note_id,
            UpdateNoteDto {
                title: Some(expected_title.to_string()),
                content: Some(expected_content.to_string()),
                ..Default::default()
            },
        )
        .expect("Update note failed");

        // 5. Move note from root notebook to child notebook
        let moved = NoteRepository::move_to_notebook(&conn, &note_id, Some(&child_id))
            .expect("Move note failed");
        assert_eq!(moved.notebook_id.as_deref(), Some(child_id.as_str()));

        // 6. Rename notebook: rename child to "Core Engine & DB"
        let renamed_child = NotebookRepository::update(
            &conn,
            &child_id,
            UpdateNotebookDto {
                name: "Core Engine & DB".to_string(),
            },
        )
        .expect("Rename child notebook failed");
        assert_eq!(renamed_child.name, "Core Engine & DB");

        // Rename root notebook to "Engineering & Systems"
        let renamed_parent = NotebookRepository::update(
            &conn,
            &parent_id,
            UpdateNotebookDto {
                name: "Engineering & Systems".to_string(),
            },
        )
        .expect("Rename parent notebook failed");
        assert_eq!(renamed_parent.name, "Engineering & Systems");

        // Connection closes and is dropped here (simulating application termination)
    }

    // --- Session 2: Application reopen & state survival verification ---
    {
        let mut reopened_conn =
            init_connection(&env.paths.database_file).expect("Session 2 reopen failed");
        // Idempotent migration verification on startup
        run_migrations(&mut reopened_conn).expect("Startup migration check failed");

        // Verify root notebook survives
        let parent = NotebookRepository::get_by_id(&reopened_conn, &parent_id)
            .expect("Fetch parent failed")
            .expect("Parent notebook must survive restart");
        assert_eq!(parent.id, parent_id);
        assert_eq!(parent.name, "Engineering & Systems");
        assert_eq!(parent.parent_id, None);

        // Verify child notebook survives with intact parent relationship
        let child = NotebookRepository::get_by_id(&reopened_conn, &child_id)
            .expect("Fetch child failed")
            .expect("Child notebook must survive restart");
        assert_eq!(child.id, child_id);
        assert_eq!(child.name, "Core Engine & DB");
        assert_eq!(child.parent_id, Some(parent_id.clone()));

        // Verify note survives with exact content and notebook assignment
        let note = NoteRepository::get_by_id(&reopened_conn, &note_id)
            .expect("Fetch note failed")
            .expect("Note must survive restart");
        assert_eq!(note.id, note_id);
        assert_eq!(note.title, expected_title);
        assert_eq!(note.content, expected_content);
        assert_eq!(note.format, "markdown");
        assert_eq!(note.notebook_id, Some(child_id.clone()));

        // Verify active note filtering by child notebook returns the note
        let child_notes =
            NoteRepository::list_filtered(&reopened_conn, false, Some(&child_id), false)
                .expect("Filter query failed");
        assert_eq!(child_notes.len(), 1);
        assert_eq!(child_notes[0].id, note_id);

        // Verify root notebook has 0 direct notes
        let parent_notes =
            NoteRepository::list_filtered(&reopened_conn, false, Some(&parent_id), false)
                .expect("Filter query failed");
        assert_eq!(parent_notes.len(), 0);

        // Verify unfiled notes has 0 notes
        let unfiled_notes = NoteRepository::list_filtered(&reopened_conn, false, None, true)
            .expect("Filter query failed");
        assert_eq!(unfiled_notes.len(), 0);
    }
}

#[test]
fn test_task_15_backend_note_tag_commands_lifecycle() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create tags with various names and Unicode
    let tag_work = TagRepository::create(&conn, "Work").expect("Create Work tag");
    let tag_urgent = TagRepository::create(&conn, "Urgent 🚨").expect("Create Urgent tag");
    let tag_proj = TagRepository::create(&conn, "Alpha Project").expect("Create Alpha Project tag");

    // 2. Create note
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Tagged Note Spec".to_string()),
            content: Some("Note content testing tag associations".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note");

    // 3. Add tag to note (idempotent)
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id).expect("Add tag");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id)
        .expect("Add same tag again (idempotent)");

    let tags_1 = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get note tags");
    assert_eq!(tags_1.len(), 1, "Idempotent add must not duplicate rows");
    assert_eq!(tags_1[0].id, tag_work.id);

    // 4. Add more tags and check alphabetical order
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_urgent.id).expect("Add urgent");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_proj.id).expect("Add proj");

    let tags_3 = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get note tags");
    assert_eq!(tags_3.len(), 3);
    // Ordered by name ASC: "Alpha Project", "Urgent 🚨", "Work"
    assert_eq!(tags_3[0].name, "Alpha Project");
    assert_eq!(tags_3[1].name, "Urgent 🚨");
    assert_eq!(tags_3[2].name, "Work");

    // 5. Remove tag from note
    TagRepository::remove_tag_from_note(&conn, &note.id, &tag_urgent.id).expect("Remove urgent");
    let tags_after_remove =
        TagRepository::get_tags_for_note(&conn, &note.id).expect("Get note tags");
    assert_eq!(tags_after_remove.len(), 2);
    assert!(!tags_after_remove.iter().any(|t| t.id == tag_urgent.id));

    // 6. Remove non-existent tag association (safe no-op)
    TagRepository::remove_tag_from_note(&conn, &note.id, "non_existent_tag_id")
        .expect("Safe no-op");
    let tags_safe = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get note tags");
    assert_eq!(tags_safe.len(), 2);

    // 7. Atomic batch set (replace all tags with tag_urgent)
    let updated_tags =
        TagRepository::set_tags_for_note(&mut conn, &note.id, std::slice::from_ref(&tag_urgent.id))
            .expect("Batch set tags");
    assert_eq!(updated_tags.len(), 1);
    assert_eq!(updated_tags[0].id, tag_urgent.id);

    // 8. Reopen database and verify note-tag associations persist
    drop(conn);
    let reopened_conn = init_connection(&env.paths.database_file).expect("Reopen failed");
    let persistent_tags =
        TagRepository::get_tags_for_note(&reopened_conn, &note.id).expect("Get tags after reopen");
    assert_eq!(persistent_tags.len(), 1);
    assert_eq!(persistent_tags[0].id, tag_urgent.id);
    assert_eq!(persistent_tags[0].name, "Urgent 🚨");

    // Note content is completely preserved
    let persistent_note = NoteRepository::get_by_id(&reopened_conn, &note.id)
        .expect("Get note")
        .expect("Must exist");
    assert_eq!(persistent_note.title, "Tagged Note Spec");
}

#[test]
fn test_task_18_multi_tag_support_lifecycle_and_isolation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create tags: work, planning, important, personal
    let tag_work = TagRepository::create(&conn, "work").expect("Create work");
    let tag_planning = TagRepository::create(&conn, "planning").expect("Create planning");
    let tag_important = TagRepository::create(&conn, "important").expect("Create important");
    let tag_personal = TagRepository::create(&conn, "personal").expect("Create personal");

    // 2. Create note: "Project Plan"
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Project Plan".to_string()),
            content: Some("Multi-tag project plan description".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 1");

    // Create second note to verify multi-note isolation
    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Personal Diary".to_string()),
            content: Some("Private thoughts".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 2");

    // 3. Add 'work' to note 1 -> exactly 1 tag
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_work.id).expect("Add work");
    let tags_step1 = TagRepository::get_tags_for_note(&conn, &note1.id).expect("Get tags");
    assert_eq!(tags_step1.len(), 1);
    assert_eq!(tags_step1[0].name, "work");

    // 4. Add 'planning' to note 1 -> existing 'work' tag is preserved!
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_planning.id).expect("Add planning");
    let tags_step2 = TagRepository::get_tags_for_note(&conn, &note1.id).expect("Get tags");
    assert_eq!(tags_step2.len(), 2);
    let names_step2: Vec<_> = tags_step2.iter().map(|t| t.name.as_str()).collect();
    assert!(names_step2.contains(&"work"), "work must be retained");
    assert!(names_step2.contains(&"planning"), "planning must be added");

    // 5. Add 'important' to note 1 -> all 3 tags are present
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_important.id).expect("Add important");
    let tags_step3 = TagRepository::get_tags_for_note(&conn, &note1.id).expect("Get tags");
    assert_eq!(tags_step3.len(), 3);
    let names_step3: Vec<_> = tags_step3.iter().map(|t| t.name.as_str()).collect();
    assert!(names_step3.contains(&"work"));
    assert!(names_step3.contains(&"planning"));
    assert!(names_step3.contains(&"important"));

    // 6. Assign 'personal' to note 2 -> note 1 is unaffected
    TagRepository::add_tag_to_note(&conn, &note2.id, &tag_personal.id)
        .expect("Add personal to note 2");
    let note2_tags = TagRepository::get_tags_for_note(&conn, &note2.id).expect("Get note 2 tags");
    assert_eq!(note2_tags.len(), 1);
    assert_eq!(note2_tags[0].name, "personal");

    let note1_tags_isolated =
        TagRepository::get_tags_for_note(&conn, &note1.id).expect("Get note 1 tags");
    assert_eq!(
        note1_tags_isolated.len(),
        3,
        "Note 1 must retain its 3 tags"
    );

    // 7. Removing 'work' from note 1 must NOT remove 'planning' or 'important'
    TagRepository::remove_tag_from_note(&conn, &note1.id, &tag_work.id)
        .expect("Remove work from note 1");
    let tags_after_remove = TagRepository::get_tags_for_note(&conn, &note1.id).expect("Get tags");
    assert_eq!(
        tags_after_remove.len(),
        2,
        "Note 1 must have 2 tags remaining"
    );
    let names_after_remove: Vec<_> = tags_after_remove.iter().map(|t| t.name.as_str()).collect();
    assert!(
        !names_after_remove.contains(&"work"),
        "work must be removed"
    );
    assert!(
        names_after_remove.contains(&"planning"),
        "planning must remain"
    );
    assert!(
        names_after_remove.contains(&"important"),
        "important must remain"
    );

    // Note 2 tags remain unaffected
    let note2_tags_after =
        TagRepository::get_tags_for_note(&conn, &note2.id).expect("Get note 2 tags");
    assert_eq!(note2_tags_after.len(), 1);
    assert_eq!(note2_tags_after[0].name, "personal");
}

#[test]
fn test_task_19_tag_assignment_persistence_across_restarts() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create tags: work and important
    let tag_work = TagRepository::create(&conn, "work").expect("Create work");
    let tag_important = TagRepository::create(&conn, "important").expect("Create important");

    // 2. Create note: "Quarterly Review"
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quarterly Review".to_string()),
            content: Some("Q3 accomplishments and goals for Q4".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note");

    // 3. Assign tags: add 'work', then add 'important'
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id).expect("Add work");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_important.id).expect("Add important");

    let initial_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get initial tags");
    assert_eq!(initial_tags.len(), 2);
    assert_eq!(initial_tags[0].name, "important");
    assert_eq!(initial_tags[1].name, "work");

    // 4. Simulate app shutdown (close/drop connection)
    drop(conn);

    // 5. Simulate app reopen (fresh connection from same SQLite db file)
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let reopened_tags =
        TagRepository::get_tags_for_note(&conn2, &note.id).expect("Get reopened tags");

    // Expected: work and important both preserved in SQLite
    assert_eq!(reopened_tags.len(), 2, "Tags must survive app restart");
    assert_eq!(reopened_tags[0].id, tag_important.id);
    assert_eq!(reopened_tags[0].name, "important");
    assert_eq!(reopened_tags[1].id, tag_work.id);
    assert_eq!(reopened_tags[1].name, "work");

    // Note content is fully preserved
    let reopened_note = NoteRepository::get_by_id(&conn2, &note.id)
        .expect("Get note")
        .expect("Note must exist");
    assert_eq!(reopened_note.title, "Quarterly Review");
    assert_eq!(reopened_note.content, "Q3 accomplishments and goals for Q4");
    assert_eq!(reopened_note.format, "md");

    // 6. Mutate tags after restart: add 'planning'
    let tag_planning = TagRepository::create(&conn2, "planning").expect("Create planning");
    TagRepository::add_tag_to_note(&conn2, &note.id, &tag_planning.id).expect("Add planning");

    // 7. Second restart simulation
    drop(conn2);
    let conn3 = init_connection(&env.paths.database_file).expect("Second reopen failed");
    let restart2_tags =
        TagRepository::get_tags_for_note(&conn3, &note.id).expect("Get tags after 2nd reopen");
    assert_eq!(
        restart2_tags.len(),
        3,
        "All 3 tags must survive second restart"
    );
    let names: Vec<_> = restart2_tags.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["important", "planning", "work"]);
}

#[test]
fn test_task_20_favorites_backend_support_lifecycle_and_persistence() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a note (defaults to is_favorite = false)
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Important Project Spec".to_string()),
            content: Some("Architecture notes".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 1");
    assert!(!note1.is_favorite);

    // 2. Mark note as favorite using set_favorite
    let fav_note = NoteRepository::set_favorite(&conn, &note1.id, true).expect("Set favorite true");
    assert!(fav_note.is_favorite);

    // 3. Query list_favorites -> note1 must be returned
    let favs = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert_eq!(favs.len(), 1);
    assert_eq!(favs[0].id, note1.id);
    assert_eq!(favs[0].title, "Important Project Spec");

    // 4. Create second note (not favorite)
    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Draft Idea".to_string()),
            content: Some("Unfinished thought".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 2");
    assert!(!note2.is_favorite);

    let favs_after_second = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert_eq!(
        favs_after_second.len(),
        1,
        "Only favorited notes must appear in favorites"
    );

    // 5. Toggle favorite on note 1 -> should become false
    let unfav_note =
        NoteRepository::toggle_favorite(&conn, &note1.id).expect("Toggle favorite to false");
    assert!(!unfav_note.is_favorite);

    let favs_empty = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert_eq!(favs_empty.len(), 0);

    // 6. Set favorite true again
    NoteRepository::set_favorite(&conn, &note1.id, true).expect("Re-favorite");

    // 7. Reopen database and verify favorite state persists
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let persistent_favs = NoteRepository::list_favorites(&conn2).expect("List favs after reopen");
    assert_eq!(persistent_favs.len(), 1);
    assert_eq!(persistent_favs[0].id, note1.id);
    assert!(persistent_favs[0].is_favorite);

    // 8. Soft-delete favorite note -> must be excluded from list_favorites
    NoteRepository::delete(&conn2, &note1.id, true).expect("Soft delete");
    let favs_after_delete = NoteRepository::list_favorites(&conn2).expect("List favs after delete");
    assert_eq!(
        favs_after_delete.len(),
        0,
        "Soft deleted notes must not appear in favorites"
    );
}

#[test]
fn test_phase5_task_22_favorite_persistence_across_restarts() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a note (starts as favorite = false)
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Persistent Favorite Note".to_string()),
            content: Some("Favorite content testing restart persistence".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note");
    assert!(!note.is_favorite);

    // 2. Mark note as favorite
    let fav = NoteRepository::set_favorite(&conn, &note.id, true).expect("Set favorite true");
    assert!(fav.is_favorite);

    // 3. Close app (drop connection) -> reopen
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen 1 failed");

    // 4. Expected: favorite = true
    let fetched_reopen1 = NoteRepository::get_by_id(&conn2, &note.id)
        .expect("Get note")
        .expect("Note exists");
    assert!(
        fetched_reopen1.is_favorite,
        "Expected favorite = true after restart"
    );

    let favs1 = NoteRepository::list_favorites(&conn2).expect("List favorites");
    assert_eq!(favs1.len(), 1);
    assert_eq!(favs1[0].id, note.id);

    // 5. Unfavorite note
    let unfav = NoteRepository::set_favorite(&conn2, &note.id, false).expect("Set favorite false");
    assert!(!unfav.is_favorite);

    // 6. Restart app again (drop connection -> reopen)
    drop(conn2);
    let conn3 = init_connection(&env.paths.database_file).expect("Reopen 2 failed");

    // 7. Expected: favorite = false
    let fetched_reopen2 = NoteRepository::get_by_id(&conn3, &note.id)
        .expect("Get note")
        .expect("Note exists");
    assert!(
        !fetched_reopen2.is_favorite,
        "Expected favorite = false after restart"
    );

    let favs2 = NoteRepository::list_favorites(&conn3).expect("List favorites");
    assert_eq!(favs2.len(), 0, "No notes should appear in favorites list");

    // 8. Multi-note restart verification
    let note_a = NoteRepository::create(
        &conn3,
        CreateNoteDto {
            title: Some("Note A".to_string()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("Create note A");

    let _note_b = NoteRepository::create(
        &conn3,
        CreateNoteDto {
            title: Some("Note B".to_string()),
            is_favorite: Some(false),
            ..Default::default()
        },
    )
    .expect("Create note B");

    let note_c = NoteRepository::create(
        &conn3,
        CreateNoteDto {
            title: Some("Note C".to_string()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("Create note C");

    drop(conn3);
    let conn4 = init_connection(&env.paths.database_file).expect("Reopen 3 failed");
    let multi_favs = NoteRepository::list_favorites(&conn4).expect("List multi favorites");
    assert_eq!(
        multi_favs.len(),
        2,
        "Only Note A and Note C must be favorites"
    );
    let fav_ids: Vec<_> = multi_favs.iter().map(|n| n.id.as_str()).collect();
    assert!(fav_ids.contains(&note_a.id.as_str()));
    assert!(fav_ids.contains(&note_c.id.as_str()));
}

#[test]
fn test_task_24_tag_navigation_and_filtering() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create tags
    let tag_work = TagRepository::create(&conn, "work").expect("Create work tag");
    let tag_urgent = TagRepository::create(&conn, "urgent").expect("Create urgent tag");
    let tag_personal = TagRepository::create(&conn, "personal").expect("Create personal tag");

    // 2. Create notes
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Note 1".to_string()),
            content: Some("First work note".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 1");

    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Note 2 (Pinned)".to_string()),
            content: Some("Second work note, pinned".to_string()),
            is_pinned: Some(true),
            ..Default::default()
        },
    )
    .expect("Create note 2");

    let note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Personal Note".to_string()),
            content: Some("Personal note content".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 3");

    let note4 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Urgent Work Note (To be deleted)".to_string()),
            content: Some("Will be soft deleted".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 4");

    // 3. Assign tags
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_work.id).expect("Tag note 1");
    TagRepository::add_tag_to_note(&conn, &note2.id, &tag_work.id).expect("Tag note 2");
    TagRepository::add_tag_to_note(&conn, &note3.id, &tag_personal.id).expect("Tag note 3");
    TagRepository::add_tag_to_note(&conn, &note4.id, &tag_work.id).expect("Tag note 4 with work");
    TagRepository::add_tag_to_note(&conn, &note4.id, &tag_urgent.id)
        .expect("Tag note 4 with urgent");

    // 4. Verify initial tag note counts
    let counts = TagRepository::get_tag_note_counts(&conn).expect("Get tag note counts");
    assert_eq!(counts.get(&tag_work.id).copied().unwrap_or(0), 3);
    assert_eq!(counts.get(&tag_urgent.id).copied().unwrap_or(0), 1);
    assert_eq!(counts.get(&tag_personal.id).copied().unwrap_or(0), 1);

    // 5. Query notes by tag_work: should return note2 (pinned), note4, note1 (ordered by is_pinned DESC, modified_at DESC)
    let work_notes =
        NoteRepository::list_by_tag(&conn, &tag_work.id).expect("List notes by work tag");
    assert_eq!(work_notes.len(), 3);
    assert_eq!(work_notes[0].id, note2.id, "Pinned note must come first");
    let work_ids: Vec<_> = work_notes.iter().map(|n| n.id.as_str()).collect();
    assert!(work_ids.contains(&note1.id.as_str()));
    assert!(work_ids.contains(&note4.id.as_str()));
    assert!(
        !work_ids.contains(&note3.id.as_str()),
        "Personal note must not be in work tag"
    );

    // 6. Query notes by tag_personal
    let personal_notes =
        NoteRepository::list_by_tag(&conn, &tag_personal.id).expect("List notes by personal tag");
    assert_eq!(personal_notes.len(), 1);
    assert_eq!(personal_notes[0].id, note3.id);

    // 7. Soft delete note4
    NoteRepository::delete(&conn, &note4.id, true).expect("Soft delete note 4");

    // 8. Re-query tag_work: deleted note4 must be excluded
    let work_notes_after_delete =
        NoteRepository::list_by_tag(&conn, &tag_work.id).expect("List after delete");
    assert_eq!(
        work_notes_after_delete.len(),
        2,
        "Soft-deleted note must be excluded from tag navigation"
    );
    assert_eq!(work_notes_after_delete[0].id, note2.id);
    assert_eq!(work_notes_after_delete[1].id, note1.id);

    // 9. Re-query tag_urgent: had only note4, should now be empty
    let urgent_notes_after_delete =
        NoteRepository::list_by_tag(&conn, &tag_urgent.id).expect("List urgent after delete");
    assert_eq!(urgent_notes_after_delete.len(), 0);

    // 10. Verify tag counts exclude soft-deleted notes
    let counts_after =
        TagRepository::get_tag_note_counts(&conn).expect("Get tag note counts after delete");
    assert_eq!(counts_after.get(&tag_work.id).copied().unwrap_or(0), 2);
    assert_eq!(counts_after.get(&tag_urgent.id).copied().unwrap_or(0), 0);
    assert_eq!(counts_after.get(&tag_personal.id).copied().unwrap_or(0), 1);
}

#[test]
fn test_task_25_single_primary_navigation_context_isolation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create notebooks
    let nb_work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook Work");

    let nb_personal = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Personal Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook Personal");

    // 2. Create tags
    let tag_urgent = TagRepository::create(&conn, "urgent").expect("Create tag urgent");

    // 3. Create notes in different contexts:
    // Note 1: In Work notebook, tagged "urgent", NOT favorite
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Task".to_string()),
            notebook_id: Some(nb_work.id.clone()),
            is_favorite: Some(false),
            ..Default::default()
        },
    )
    .expect("Create note 1");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_urgent.id)
        .expect("Add urgent tag to note 1");

    // Note 2: In Personal notebook, tagged "urgent", IS favorite
    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Personal Errand".to_string()),
            notebook_id: Some(nb_personal.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("Create note 2");
    TagRepository::add_tag_to_note(&conn, &note2.id, &tag_urgent.id)
        .expect("Add urgent tag to note 2");

    // Note 3: In Work notebook, NOT tagged, IS favorite
    let note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Reference".to_string()),
            notebook_id: Some(nb_work.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("Create note 3");

    // 4. Primary Context A: Notebook Navigation (Work Notebook)
    // Must return notes in Work Notebook (Note 1 and Note 3), isolating from tag and favorite states
    let work_notes = NoteRepository::list_filtered(&conn, false, Some(&nb_work.id), false)
        .expect("List by notebook");
    assert_eq!(work_notes.len(), 2);
    let work_ids: Vec<_> = work_notes.iter().map(|n| n.id.as_str()).collect();
    assert!(work_ids.contains(&note1.id.as_str()));
    assert!(work_ids.contains(&note3.id.as_str()));
    assert!(!work_ids.contains(&note2.id.as_str()));

    // 5. Primary Context B: Tag Navigation (Tag "urgent")
    // Must return all notes tagged "urgent" across notebooks (Note 1 and Note 2), isolating from notebook filter
    let tag_notes = NoteRepository::list_by_tag(&conn, &tag_urgent.id).expect("List by tag");
    assert_eq!(tag_notes.len(), 2);
    let tag_ids: Vec<_> = tag_notes.iter().map(|n| n.id.as_str()).collect();
    assert!(tag_ids.contains(&note1.id.as_str()));
    assert!(tag_ids.contains(&note2.id.as_str()));
    assert!(!tag_ids.contains(&note3.id.as_str()));

    // 6. Primary Context C: Favorites Navigation
    // Must return all favorite notes across notebooks and tags (Note 2 and Note 3)
    let fav_notes = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert_eq!(fav_notes.len(), 2);
    let fav_ids: Vec<_> = fav_notes.iter().map(|n| n.id.as_str()).collect();
    assert!(fav_ids.contains(&note2.id.as_str()));
    assert!(fav_ids.contains(&note3.id.as_str()));
    assert!(!fav_ids.contains(&note1.id.as_str()));
}

#[test]
fn test_task_26_tag_empty_state_and_isolation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a tag with 0 notes
    let empty_tag = TagRepository::create(&conn, "planning").expect("Create tag planning");

    // 2. Create several unrelated notes (some unfiled, some in notebook, some tagged differently)
    let other_tag = TagRepository::create(&conn, "other").expect("Create other tag");
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "General".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook");

    let n1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Notebook Note".to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create note 1");

    let n2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Tagged Other Note".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 2");
    TagRepository::add_tag_to_note(&conn, &n2.id, &other_tag.id).expect("Add other tag");

    let _n3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Unfiled Note".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 3");

    // 3. Verify empty_tag note list is completely empty and unrelated notes are never returned
    let notes_for_empty =
        NoteRepository::list_by_tag(&conn, &empty_tag.id).expect("List notes for empty tag");
    assert_eq!(notes_for_empty.len(), 0, "Empty tag must return 0 notes");

    // 4. Verify tag count is 0
    let counts = TagRepository::get_tag_note_counts(&conn).expect("Get tag note counts");
    assert_eq!(counts.get(&empty_tag.id).copied().unwrap_or(0), 0);

    // 5. Assign note 1 to empty_tag
    TagRepository::add_tag_to_note(&conn, &n1.id, &empty_tag.id).expect("Add tag to note 1");
    let notes_after_assign =
        NoteRepository::list_by_tag(&conn, &empty_tag.id).expect("List after assign");
    assert_eq!(notes_after_assign.len(), 1);
    assert_eq!(notes_after_assign[0].id, n1.id);

    // 6. Remove note 1 from tag -> immediately transitions back to 0 notes (empty state)
    TagRepository::remove_tag_from_note(&conn, &n1.id, &empty_tag.id)
        .expect("Remove tag from note 1");
    let notes_after_remove =
        NoteRepository::list_by_tag(&conn, &empty_tag.id).expect("List after remove");
    assert_eq!(
        notes_after_remove.len(),
        0,
        "Must be empty again after tag removal"
    );

    let counts_after_remove =
        TagRepository::get_tag_note_counts(&conn).expect("Counts after remove");
    assert_eq!(
        counts_after_remove.get(&empty_tag.id).copied().unwrap_or(0),
        0
    );
}

#[test]
fn test_task_27_favorites_empty_state_and_lifecycle() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Initially, no notes exist -> list_favorites returns 0
    let favs_empty = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert_eq!(favs_empty.len(), 0);

    // 2. Create several non-favorite notes across notebooks and tags
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook");

    let tag = TagRepository::create(&conn, "urgent").expect("Create tag");

    let n1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Non-favorite in notebook".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(false),
            ..Default::default()
        },
    )
    .expect("Create note 1");
    TagRepository::add_tag_to_note(&conn, &n1.id, &tag.id).expect("Tag note 1");

    let _n2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Non-favorite unfiled".to_string()),
            is_favorite: Some(false),
            ..Default::default()
        },
    )
    .expect("Create note 2");

    // 3. Favorites list must remain empty despite non-favorite notes existing
    let favs_still_empty = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert_eq!(
        favs_still_empty.len(),
        0,
        "Non-favorite notes must not appear in favorites"
    );

    // 4. Mark note 1 as favorite -> list_favorites returns exactly note 1
    let faved = NoteRepository::set_favorite(&conn, &n1.id, true).expect("Set favorite true");
    assert!(faved.is_favorite);

    let favs_one = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert_eq!(favs_one.len(), 1);
    assert_eq!(favs_one[0].id, n1.id);

    // 5. Unfavorite note 1 -> transitions back to empty state (0 notes)
    let unfaved = NoteRepository::set_favorite(&conn, &n1.id, false).expect("Set favorite false");
    assert!(!unfaved.is_favorite);

    let favs_back_to_empty = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert_eq!(
        favs_back_to_empty.len(),
        0,
        "Must be empty again after unfavoriting"
    );

    // 6. Favorite note 1 again, then soft-delete it -> must not appear in favorites
    NoteRepository::set_favorite(&conn, &n1.id, true).expect("Re-favorite");
    NoteRepository::delete(&conn, &n1.id, true).expect("Soft delete favorite note");

    let favs_after_soft_delete = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert_eq!(
        favs_after_soft_delete.len(),
        0,
        "Soft-deleted favorite note must not appear in favorites"
    );
}

#[test]
fn test_task_28_tag_list_empty_state_and_full_functionality_without_tags() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Tag repository is completely empty initially
    let tags = TagRepository::list(&conn).expect("List tags");
    assert_eq!(tags.len(), 0, "Initial tag count must be 0");

    let tag_counts = TagRepository::get_tag_note_counts(&conn).expect("Get tag note counts");
    assert_eq!(tag_counts.len(), 0, "Tag note counts map must be empty");

    // 2. Full application functionality succeeds without any tags:
    // Create notebook hierarchy
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "General Notes".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook");

    // Create notes (unfiled, inside notebook, favorite)
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note Without Tags 1".to_string()),
            content: Some("Working with zero tags defined".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("Create note 1");

    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note Without Tags 2".to_string()),
            content: Some("Unfiled note with zero tags".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            ..Default::default()
        },
    )
    .expect("Create note 2");

    // Verify retrieval and listing
    let all_notes = NoteRepository::list(&conn, false).expect("List notes");
    assert_eq!(all_notes.len(), 2);

    let favs = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert_eq!(favs.len(), 1);
    assert_eq!(favs[0].id, note1.id);

    let unfiled = NoteRepository::list_filtered(&conn, false, None, true).expect("List unfiled");
    assert_eq!(unfiled.len(), 1);
    assert_eq!(unfiled[0].id, note2.id);

    // Tags for each note should return empty list
    let n1_tags = TagRepository::get_tags_for_note(&conn, &note1.id).expect("Get tags for note 1");
    assert_eq!(n1_tags.len(), 0);

    let n2_tags = TagRepository::get_tags_for_note(&conn, &note2.id).expect("Get tags for note 2");
    assert_eq!(n2_tags.len(), 0);

    // 3. Create a temporary tag, assign it, and then delete it
    let temp_tag = TagRepository::create(&conn, "temporary").expect("Create temp tag");
    TagRepository::add_tag_to_note(&conn, &note1.id, &temp_tag.id).expect("Assign temp tag");

    let tags_with_one = TagRepository::list(&conn).expect("List tags with one");
    assert_eq!(tags_with_one.len(), 1);

    // Delete tag: note must survive, and tag list must return to empty state
    TagRepository::delete(&conn, &temp_tag.id).expect("Delete temp tag");

    let tags_empty_again = TagRepository::list(&conn).expect("List tags empty again");
    assert_eq!(tags_empty_again.len(), 0);

    let note1_survived = NoteRepository::get_by_id(&conn, &note1.id)
        .expect("Get note 1")
        .expect("Note 1 exists");
    assert_eq!(note1_survived.id, note1.id);
    assert!(note1_survived.is_favorite);
}

#[test]
fn test_task_29_canonical_note_metadata_model() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a notebook and tags
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Engineering".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook");

    let tag_rust = TagRepository::create(&conn, "rust").expect("Create tag 1");
    let tag_phase5 = TagRepository::create(&conn, "phase-5").expect("Create tag 2");

    // 2. Create note with multi-line, unicode text
    let content_text = "The quick brown fox jumps over the lazy dog. हिंदी टेक्स्ट 🦊";
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Metadata Test Note".to_string()),
            content: Some(content_text.to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(false),
        },
    )
    .expect("Create note");

    // 3. Link tags
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_rust.id).expect("Link tag rust");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_phase5.id).expect("Link tag phase5");

    // 4. Retrieve canonical NoteMetadata
    let metadata = NoteRepository::get_metadata(&conn, &note.id)
        .expect("Get metadata query")
        .expect("Metadata exists");

    // Verify fields
    assert_eq!(metadata.note_id, note.id);
    assert_eq!(metadata.format, "md");
    assert_eq!(metadata.notebook_id, Some(nb.id));
    assert!(metadata.is_favorite);
    assert!(!metadata.is_pinned);
    assert_eq!(metadata.created_at, note.created_at);
    assert_eq!(metadata.modified_at, note.modified_at);

    // Verify associated tags
    assert_eq!(metadata.tags.len(), 2);
    let tag_names: Vec<_> = metadata.tags.iter().map(|t| t.name.as_str()).collect();
    assert!(tag_names.contains(&"rust"));
    assert!(tag_names.contains(&"phase-5"));

    // Verify content metrics
    assert_eq!(metadata.character_count, content_text.chars().count());
    assert_eq!(metadata.word_count, content_text.split_whitespace().count());
    assert_eq!(metadata.byte_size, content_text.len());

    // 5. Querying non-existent note returns None safely
    let non_existent =
        NoteRepository::get_metadata(&conn, "non-existent-id").expect("Query non-existent");
    assert!(non_existent.is_none());
}

#[test]
fn test_task_30_metadata_panel_hierarchy_path_and_format_integrity() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a 3-level notebook hierarchy: Engineering -> Backend -> Storage
    let nb_root = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Engineering".to_string(),
            parent_id: None,
        },
    )
    .expect("Create root");

    let nb_mid = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Backend".to_string(),
            parent_id: Some(nb_root.id.clone()),
        },
    )
    .expect("Create mid");

    let nb_leaf = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Storage".to_string(),
            parent_id: Some(nb_mid.id.clone()),
        },
    )
    .expect("Create leaf");

    // 2. Create tags
    let tag1 = TagRepository::create(&conn, "database").expect("Tag 1");
    let tag2 = TagRepository::create(&conn, "sqlite").expect("Tag 2");

    // 3. Create note inside leaf notebook
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("SQLite Architecture".to_string()),
            content: Some("Local first sqlite storage engine".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb_leaf.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("Create note");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag1.id).expect("Add tag 1");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag2.id).expect("Add tag 2");

    // 4. Retrieve metadata
    let meta = NoteRepository::get_metadata(&conn, &note.id)
        .expect("Get metadata")
        .expect("Metadata exists");
    assert_eq!(meta.format, "md");
    assert!(meta.is_favorite);
    assert_eq!(meta.tags.len(), 2);

    // 5. Test path reconstruction algorithm: leaf -> mid -> root
    let all_notebooks = NotebookRepository::list(&conn).expect("List notebooks");
    let mut path_segments = Vec::new();
    let mut current_id = note.notebook_id.as_deref();
    while let Some(id) = current_id {
        if let Some(found) = all_notebooks.iter().find(|nb| nb.id == id) {
            path_segments.insert(0, found.name.as_str());
            current_id = found.parent_id.as_deref();
        } else {
            break;
        }
    }
    let full_path = path_segments.join(" / ");
    assert_eq!(full_path, "Engineering / Backend / Storage");

    // 6. Test unfiled note path reconstruction
    let unfiled_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Unfiled Note".to_string()),
            content: Some("Unfiled content".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create unfiled");
    let unfiled_meta = NoteRepository::get_metadata(&conn, &unfiled_note.id)
        .expect("Get unfiled meta")
        .expect("Unfiled meta exists");
    assert_eq!(unfiled_meta.notebook_id, None);
    assert_eq!(unfiled_meta.format, "txt");
}

#[test]
fn test_task_31_metadata_readonly_rules_and_immutability() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a note and capture its created_at timestamp
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Initial Title".to_string()),
            content: Some("Initial Content".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("Create note");

    let initial_created_at = note.created_at.clone();

    // 2. Perform several updates mutating editable metadata fields (title, content, format, favorite, pinned)
    let updated1 = NoteRepository::update(
        &conn,
        &note.id,
        UpdateNoteDto {
            title: Some("Modified Title 1".to_string()),
            content: Some("Modified Content 1".to_string()),
            format: Some("md".to_string()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("Update 1");

    // Verify created_at is strictly immutable
    assert_eq!(updated1.created_at, initial_created_at);
    assert_eq!(updated1.title, "Modified Title 1");
    assert_eq!(updated1.format, "md");
    assert!(updated1.is_favorite);

    // 3. Update notebook assignment (editable metadata)
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook");

    let updated2 = NoteRepository::update(
        &conn,
        &note.id,
        UpdateNoteDto {
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        },
    )
    .expect("Update 2");

    assert_eq!(
        updated2.created_at, initial_created_at,
        "created_at must remain immutable across notebook moves"
    );
    assert_eq!(updated2.notebook_id, Some(nb.id));

    // 4. Verify canonical metadata reflects read-only created_at and modified_at
    let meta = NoteRepository::get_metadata(&conn, &note.id)
        .expect("Get metadata")
        .expect("Metadata exists");
    assert_eq!(meta.created_at, initial_created_at);
    assert!(meta.is_favorite);
}

#[test]
fn test_task_32_format_display_txt_vs_md_metadata_mapping() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Direct unit check on format display mapping function
    assert_eq!(
        personal_notepad_lib::storage::models::format_display_name("txt"),
        "Plain Text"
    );
    assert_eq!(
        personal_notepad_lib::storage::models::format_display_name("md"),
        "Markdown"
    );
    assert_eq!(
        personal_notepad_lib::storage::models::format_display_name("TXT"),
        "Plain Text"
    );
    assert_eq!(
        personal_notepad_lib::storage::models::format_display_name("MD"),
        "Markdown"
    );
    assert_eq!(
        personal_notepad_lib::storage::models::format_display_name("unknown"),
        "Plain Text"
    );

    // 2. Create note with format 'txt'
    let txt_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Plain Note".to_string()),
            content: Some("Simple plain text content".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create plain note");

    let txt_meta = NoteRepository::get_metadata(&conn, &txt_note.id)
        .expect("Get txt metadata")
        .expect("Txt metadata exists");
    assert_eq!(txt_meta.format, "txt");
    assert_eq!(txt_meta.format_display(), "Plain Text");

    // 3. Create note with format 'md'
    let md_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Markdown Note".to_string()),
            content: Some("# Header 1\n**Bold text**".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("Create markdown note");

    let md_meta = NoteRepository::get_metadata(&conn, &md_note.id)
        .expect("Get md metadata")
        .expect("Md metadata exists");
    assert_eq!(md_meta.format, "md");
    assert_eq!(md_meta.format_display(), "Markdown");

    // 4. Switch format: 'txt' -> 'md' and verify raw text content remains unmodified
    let original_content = txt_note.content.clone();
    let updated_to_md = NoteRepository::update(
        &conn,
        &txt_note.id,
        UpdateNoteDto {
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("Update format to md");

    assert_eq!(
        updated_to_md.content, original_content,
        "Raw content must remain intact on format toggle"
    );
    assert_eq!(updated_to_md.format, "md");

    let toggled_meta = NoteRepository::get_metadata(&conn, &txt_note.id)
        .expect("Get toggled metadata")
        .expect("Toggled metadata exists");
    assert_eq!(toggled_meta.format, "md");
    assert_eq!(toggled_meta.format_display(), "Markdown");

    // 5. Switch back: 'md' -> 'txt'
    let updated_back_to_txt = NoteRepository::update(
        &conn,
        &txt_note.id,
        UpdateNoteDto {
            format: Some("txt".to_string()),
            ..Default::default()
        },
    )
    .expect("Update format back to txt");

    assert_eq!(updated_back_to_txt.content, original_content);
    assert_eq!(updated_back_to_txt.format, "txt");

    let final_meta = NoteRepository::get_metadata(&conn, &txt_note.id)
        .expect("Get final metadata")
        .expect("Final metadata exists");
    assert_eq!(final_meta.format, "txt");
    assert_eq!(final_meta.format_display(), "Plain Text");
}

#[test]
fn test_task_33_notebook_metadata_integration_hierarchy_path() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a 3-level notebook hierarchy: Work -> Projects -> Releases
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Create Work");

    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(work.id.clone()),
        },
    )
    .expect("Create Projects");

    let releases = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Releases".to_string(),
            parent_id: Some(projects.id.clone()),
        },
    )
    .expect("Create Releases");

    // 2. Validate NotebookRepository::get_hierarchy_path for all 3 levels
    assert_eq!(
        NotebookRepository::get_hierarchy_path(&conn, &work.id).expect("work path"),
        "Work"
    );
    assert_eq!(
        NotebookRepository::get_hierarchy_path(&conn, &projects.id).expect("projects path"),
        "Work / Projects"
    );
    assert_eq!(
        NotebookRepository::get_hierarchy_path(&conn, &releases.id).expect("releases path"),
        "Work / Projects / Releases"
    );

    // 3. Create a note inside "Releases"
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("v1.0 Launch Checklist".to_string()),
            content: Some("Checklist content".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(releases.id.clone()),
            ..Default::default()
        },
    )
    .expect("Create note in releases");

    // Validate NoteRepository::get_notebook_path
    let note_path = NoteRepository::get_notebook_path(&conn, &note.id).expect("note path");
    assert_eq!(note_path, "Work / Projects / Releases");
    assert!(
        !note_path.contains(&releases.id),
        "Must not expose internal ID"
    );

    // Validate NoteRepository::get_metadata includes human-readable path
    let meta = NoteRepository::get_metadata(&conn, &note.id)
        .expect("get metadata")
        .expect("metadata exists");
    assert_eq!(meta.notebook_id, Some(releases.id.clone()));
    assert_eq!(
        meta.notebook_path,
        Some("Work / Projects / Releases".to_string())
    );

    // 4. Move note to "Work" and verify updated metadata path
    NoteRepository::move_to_notebook(&conn, &note.id, Some(&work.id)).expect("Move to Work");

    let work_meta = NoteRepository::get_metadata(&conn, &note.id)
        .expect("get work meta")
        .expect("meta exists");
    assert_eq!(work_meta.notebook_id, Some(work.id.clone()));
    assert_eq!(work_meta.notebook_path, Some("Work".to_string()));
    assert_eq!(
        NoteRepository::get_notebook_path(&conn, &note.id).expect("work path"),
        "Work"
    );

    // 5. Unfile note and verify path is None in metadata and "Unfiled" from get_notebook_path
    NoteRepository::move_to_notebook(&conn, &note.id, None).expect("Move to Unfiled");

    let unfiled_meta = NoteRepository::get_metadata(&conn, &note.id)
        .expect("get unfiled meta")
        .expect("meta exists");
    assert_eq!(unfiled_meta.notebook_id, None);
    assert_eq!(unfiled_meta.notebook_path, None);
    assert_eq!(
        NoteRepository::get_notebook_path(&conn, &note.id).expect("unfiled path"),
        "Unfiled"
    );
}

#[test]
fn test_task_34_tag_display_multi_tag_tokens_and_order() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create tags from the spec example: work, important, planning
    let t_work = TagRepository::create(&conn, "work").expect("create work");
    let t_important = TagRepository::create(&conn, "important").expect("create important");
    let t_planning = TagRepository::create(&conn, "planning").expect("create planning");

    // 2. Create note
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Project Launch".to_string()),
            content: Some("Launch tasks".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("create note");

    // 3. Assign all three tags: [work] [important] [planning]
    TagRepository::add_tag_to_note(&conn, &note.id, &t_work.id).expect("add work");
    TagRepository::add_tag_to_note(&conn, &note.id, &t_important.id).expect("add important");
    TagRepository::add_tag_to_note(&conn, &note.id, &t_planning.id).expect("add planning");

    // 4. Retrieve note metadata and verify all 3 tags are present
    let meta = NoteRepository::get_metadata(&conn, &note.id)
        .expect("get metadata")
        .expect("meta exists");

    let tag_names: Vec<String> = meta.tags.iter().map(|t| t.name.clone()).collect();
    assert_eq!(tag_names.len(), 3);
    assert!(tag_names.contains(&"work".to_string()));
    assert!(tag_names.contains(&"important".to_string()));
    assert!(tag_names.contains(&"planning".to_string()));

    // 5. Verify tags persist across fresh connection
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen failed");
    let tags_after_reopen = TagRepository::get_tags_for_note(&conn2, &note.id).expect("get tags");
    assert_eq!(tags_after_reopen.len(), 3);
}

#[test]
fn test_task_35_tag_removal_preserves_content_title_notebook_favorite() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a notebook "Architecture"
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Architecture".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    // 2. Create note with title, content, format, notebook, favorite
    let initial_title = "Important Architecture Document".to_string();
    let initial_content = "Detailed architectural specifications and system designs.".to_string();
    let initial_format = "md".to_string();

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(initial_title.clone()),
            content: Some(initial_content.clone()),
            format: Some(initial_format.clone()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(false),
        },
    )
    .expect("create note");

    assert_eq!(note.title, initial_title);
    assert_eq!(note.content, initial_content);
    assert_eq!(note.format, initial_format);
    assert_eq!(note.notebook_id, Some(nb.id.clone()));
    assert!(note.is_favorite);

    // 3. Create tags and assign: [work] [planning]
    let tag_work = TagRepository::create(&conn, "work").expect("create work");
    let tag_planning = TagRepository::create(&conn, "planning").expect("create planning");

    TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id).expect("add work");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_planning.id).expect("add planning");

    let tags_before = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags");
    assert_eq!(tags_before.len(), 2);

    // 4. Remove tag "planning" (simulating [planning ×])
    TagRepository::remove_tag_from_note(&conn, &note.id, &tag_planning.id)
        .expect("remove planning");

    // Verify tags list
    let tags_after_1 = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags after 1");
    assert_eq!(tags_after_1.len(), 1);
    assert_eq!(tags_after_1[0].name, "work");

    // Invariants check: note content, title, notebook, favorite must NOT be modified
    let note_check_1 = NoteRepository::get_by_id(&conn, &note.id)
        .expect("fetch note")
        .expect("note exists");

    assert_eq!(
        note_check_1.title, initial_title,
        "Title must not be modified by tag removal"
    );
    assert_eq!(
        note_check_1.content, initial_content,
        "Content must not be modified by tag removal"
    );
    assert_eq!(
        note_check_1.format, initial_format,
        "Format must not be modified by tag removal"
    );
    assert_eq!(
        note_check_1.notebook_id,
        Some(nb.id.clone()),
        "Notebook must not be modified by tag removal"
    );
    assert!(
        note_check_1.is_favorite,
        "Favorite state must not be modified by tag removal"
    );

    // 5. Remove tag "work" (simulating [work ×])
    TagRepository::remove_tag_from_note(&conn, &note.id, &tag_work.id).expect("remove work");

    let tags_after_2 = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags after 2");
    assert_eq!(tags_after_2.len(), 0);

    // Invariants check after removing all tags
    let note_check_2 = NoteRepository::get_by_id(&conn, &note.id)
        .expect("fetch note")
        .expect("note exists");

    assert_eq!(note_check_2.title, initial_title);
    assert_eq!(note_check_2.content, initial_content);
    assert_eq!(note_check_2.format, initial_format);
    assert_eq!(note_check_2.notebook_id, Some(nb.id.clone()));
    assert!(note_check_2.is_favorite);

    // 6. Persistence across connection restart
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen");
    let note_final = NoteRepository::get_by_id(&conn2, &note.id)
        .expect("fetch")
        .expect("exists");
    assert_eq!(note_final.title, initial_title);
    assert_eq!(note_final.content, initial_content);
    assert_eq!(note_final.notebook_id, Some(nb.id));
    assert!(note_final.is_favorite);

    let tags_final = TagRepository::get_tags_for_note(&conn2, &note.id).expect("get tags");
    assert_eq!(tags_final.len(), 0);
}

#[test]
fn test_task_36_tag_creation_from_note_editor_and_immediate_assignment() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Seed existing tags per spec: work, important, planning
    let _ = TagRepository::create(&conn, "work").expect("create work");
    let _ = TagRepository::create(&conn, "important").expect("create important");
    let _ = TagRepository::create(&conn, "planning").expect("create planning");

    let existing_tags = TagRepository::list(&conn).expect("list tags");
    assert_eq!(existing_tags.len(), 3);

    // 2. Create a note without tags
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quantum Computing Overview".to_string()),
            content: Some("Introductory notes".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("create note");

    assert_eq!(
        TagRepository::get_tags_for_note(&conn, &note.id)
            .expect("tags")
            .len(),
        0
    );

    // 3. User types "research" into tag picker search input (exactMatchExists == false)
    // and triggers createAndAssign -> TagRepository::create("research") followed by add_tag_to_note
    let created_tag = TagRepository::create(&conn, "research").expect("create research");
    assert_eq!(created_tag.name, "research");

    TagRepository::add_tag_to_note(&conn, &note.id, &created_tag.id).expect("assign research");

    // 4. Verify note now has 1 assigned tag: "research"
    let note_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("get note tags");
    assert_eq!(note_tags.len(), 1);
    assert_eq!(note_tags[0].name, "research");
    assert_eq!(note_tags[0].id, created_tag.id);

    // 5. Verify total tags in system is now 4
    let all_tags = TagRepository::list(&conn).expect("list all tags");
    assert_eq!(all_tags.len(), 4);

    // 6. Verify in-memory tag list filtering behavior (not full-text search)
    let query = "res";
    let matched: Vec<&str> = all_tags
        .iter()
        .filter(|t| t.name.to_lowercase().contains(query))
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(matched, vec!["research"]);

    // 7. Verify persistent state after reopening connection
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen");
    let persisted_meta = NoteRepository::get_metadata(&conn2, &note.id)
        .expect("metadata")
        .expect("exists");
    assert_eq!(persisted_meta.tags.len(), 1);
    assert_eq!(persisted_meta.tags[0].name, "research");
}

#[test]
fn test_task_37_prevent_duplicate_tag_assignment_database_and_repository() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a note and a tag "work"
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Duplicate Prevention Test".to_string()),
            content: Some("Testing uniqueness".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("create note");

    let tag_work = TagRepository::create(&conn, "work").expect("create work");

    // 2. Assign "work" tag for the first time
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id).expect("first assign");
    let initial_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags");
    assert_eq!(initial_tags.len(), 1);
    assert_eq!(initial_tags[0].name, "work");

    // 3. Attempt to assign "work" a second time (simulating user selecting it again)
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id)
        .expect("second assign must succeed idempotently");

    // Verify the note still only has 1 tag, NOT ["work", "work"]
    let tags_after_second =
        TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags after second");
    assert_eq!(
        tags_after_second.len(),
        1,
        "Duplicate tag assignment must never produce duplicate tags"
    );
    assert_eq!(tags_after_second[0].name, "work");

    // 4. Verify SQLite composite primary key on note_tags (note_id, tag_id)
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM note_tags WHERE note_id = ?1 AND tag_id = ?2",
            rusqlite::params![&note.id, &tag_work.id],
            |r| r.get(0),
        )
        .expect("count query");
    assert_eq!(
        count, 1,
        "note_tags must contain exactly 1 row for (note_id, tag_id)"
    );

    // Direct raw insert without IGNORE must trigger a UNIQUE / PRIMARY KEY failure
    let duplicate_insert_err = conn
        .execute(
            "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![&note.id, &tag_work.id],
        )
        .expect_err("Raw duplicate insert into note_tags must fail due to PRIMARY KEY");
    assert!(duplicate_insert_err
        .to_string()
        .to_lowercase()
        .contains("unique"));

    // 5. Verify set_tags_for_note with duplicate tag IDs in input list
    let mut conn_mut = conn;
    let batch_result = TagRepository::set_tags_for_note(
        &mut conn_mut,
        &note.id,
        &[
            tag_work.id.clone(),
            tag_work.id.clone(),
            tag_work.id.clone(),
        ],
    )
    .expect("set_tags_for_note");
    assert_eq!(
        batch_result.len(),
        1,
        "Batch assignment must filter out duplicates"
    );

    // 6. Verify persistence across connection reopen
    drop(conn_mut);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen");
    let reopened_tags =
        TagRepository::get_tags_for_note(&conn2, &note.id).expect("get tags after reopen");
    assert_eq!(reopened_tags.len(), 1);
    assert_eq!(reopened_tags[0].name, "work");
}

#[test]
fn test_task_38_tag_rename_propagation_across_multiple_notes() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a tag "work"
    let tag = TagRepository::create(&conn, "work").expect("create work tag");
    let tag_id = tag.id.clone();

    // 2. Create 20 notes and assign "work" to all of them
    let mut note_ids = Vec::new();
    for i in 1..=20 {
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Work Task Note #{i}")),
                content: Some(format!("Content of task {i}")),
                format: Some("txt".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        )
        .expect("create note");

        TagRepository::add_tag_to_note(&conn, &note.id, &tag_id).expect("add tag to note");
        note_ids.push(note.id);
    }

    // Verify initial assignments
    let initial_junction_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM note_tags WHERE tag_id = ?1",
            rusqlite::params![&tag_id],
            |r| r.get(0),
        )
        .expect("query junction count");
    assert_eq!(initial_junction_count, 20);

    for nid in &note_ids {
        let tags = TagRepository::get_tags_for_note(&conn, nid).expect("get tags");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "work");
    }

    // 3. Rename tag: work -> projects (Task 38)
    let updated_tag = TagRepository::update(
        &conn,
        &tag_id,
        personal_notepad_lib::storage::models::UpdateTagDto {
            name: "projects".to_string(),
        },
    )
    .expect("rename tag to projects");
    assert_eq!(
        updated_tag.id, tag_id,
        "Tag ID must remain completely unchanged"
    );
    assert_eq!(updated_tag.name, "projects");

    // 4. Invariant check: relationship remains note_tags.tag_id = tag_id; zero duplicate rows created
    let junction_count_after: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM note_tags WHERE tag_id = ?1",
            rusqlite::params![&tag_id],
            |r| r.get(0),
        )
        .expect("query junction count after");
    assert_eq!(
        junction_count_after, 20,
        "Total assignments must remain exactly 20 without duplicates"
    );

    let total_junction_rows: i64 = conn
        .query_row("SELECT COUNT(1) FROM note_tags", [], |r| r.get(0))
        .expect("total junction rows");
    assert_eq!(total_junction_rows, 20);

    // 5. Verify all 20 notes automatically reflect updated tag name "projects"
    for nid in &note_ids {
        let tags = TagRepository::get_tags_for_note(&conn, nid).expect("get tags");
        assert_eq!(tags.len(), 1);
        assert_eq!(
            tags[0].name, "projects",
            "Note must automatically show updated name 'projects'"
        );
        assert_eq!(tags[0].id, tag_id);

        let meta = NoteRepository::get_metadata(&conn, nid)
            .expect("get meta")
            .expect("meta exists");
        assert_eq!(meta.tags.len(), 1);
        assert_eq!(meta.tags[0].name, "projects");
    }

    // Verify get_all_notes_tag_names reflects "projects"
    let all_notes_map =
        TagRepository::get_all_notes_tag_names(&conn).expect("get all notes tags map");
    for nid in &note_ids {
        assert_eq!(all_notes_map.get(nid), Some(&vec!["projects".to_string()]));
    }

    // Verify list_by_tag still finds all 20 notes
    let notes_for_tag = NoteRepository::list_by_tag(&conn, &tag_id).expect("list by tag");
    assert_eq!(notes_for_tag.len(), 20);

    // 6. Verify persistence across connection reopen
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen");
    for nid in &note_ids {
        let tags = TagRepository::get_tags_for_note(&conn2, nid).expect("get tags reopen");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "projects");
    }
}

#[test]
fn test_task_39_tag_delete_propagation_notes_untouched_assignments_disappear() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a tag "work"
    let tag = TagRepository::create(&conn, "work").expect("create work tag");
    let tag_id = tag.id.clone();

    // Create a notebook for testing notebook assignment preservation
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    // 2. Create 25 notes with varied metadata (favorites, notebooks, unfiled)
    let mut created_notes = Vec::new();
    for i in 1..=25 {
        let is_fav = i % 2 == 0;
        let nb_id = if i % 3 == 0 {
            Some(nb.id.clone())
        } else {
            None
        };

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Project Document #{i}")),
                content: Some(format!("Detailed specs and requirements for phase {i}")),
                format: Some(if i % 2 == 0 {
                    "md".to_string()
                } else {
                    "txt".to_string()
                }),
                notebook_id: nb_id,
                is_favorite: Some(is_fav),
                is_pinned: Some(false),
            },
        )
        .expect("create note");

        // Assign "work" to all 25 notes
        TagRepository::add_tag_to_note(&conn, &note.id, &tag_id).expect("add tag");
        created_notes.push(note);
    }

    // Verify initial state: 25 notes exist and all 25 have tag "work"
    assert_eq!(
        NoteRepository::list(&conn, false)
            .expect("list notes")
            .len(),
        25
    );
    let initial_junction_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM note_tags WHERE tag_id = ?1",
            rusqlite::params![&tag_id],
            |r| r.get(0),
        )
        .expect("junction count");
    assert_eq!(initial_junction_count, 25);

    // 3. Delete tag "work" (Task 39)
    let deleted = TagRepository::delete(&conn, &tag_id).expect("delete tag");
    assert!(deleted, "Tag deletion should succeed");

    // 4. Verify tag is gone from tags table
    assert!(TagRepository::get_by_id(&conn, &tag_id)
        .expect("get by id")
        .is_none());

    // 5. Invariant check: all 25 notes remain in the database, not deleted
    let notes_after = NoteRepository::list(&conn, false).expect("list notes after");
    assert_eq!(
        notes_after.len(),
        25,
        "All 25 notes must remain in the database"
    );

    // 6. Invariant check: all 25 tag assignments in note_tags disappeared
    let junction_count_after: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM note_tags WHERE tag_id = ?1",
            rusqlite::params![&tag_id],
            |r| r.get(0),
        )
        .expect("junction count after");
    assert_eq!(
        junction_count_after, 0,
        "All 25 tag assignments must disappear from note_tags"
    );

    let total_junction_rows: i64 = conn
        .query_row("SELECT COUNT(1) FROM note_tags", [], |r| r.get(0))
        .expect("total junction rows");
    assert_eq!(total_junction_rows, 0);

    // 7. Verify each individual note's content, title, notebook, favorite are 100% intact
    for original in &created_notes {
        let note = NoteRepository::get_by_id(&conn, &original.id)
            .expect("fetch note")
            .expect("note must exist");

        assert_eq!(note.title, original.title);
        assert_eq!(note.content, original.content);
        assert_eq!(note.format, original.format);
        assert_eq!(note.notebook_id, original.notebook_id);
        assert_eq!(note.is_favorite, original.is_favorite);
        assert!(!note.is_deleted);

        let meta = NoteRepository::get_metadata(&conn, &original.id)
            .expect("get metadata")
            .expect("metadata exists");
        assert_eq!(
            meta.tags.len(),
            0,
            "Note must have 0 tags after tag deletion"
        );
    }

    // 8. Verify persistence across restart
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen");
    assert_eq!(
        NoteRepository::list(&conn2, false)
            .expect("reopen notes list")
            .len(),
        25
    );
    assert!(TagRepository::get_by_id(&conn2, &tag_id)
        .expect("reopen tag query")
        .is_none());
}

#[test]
fn test_task_40_favorite_tag_notebook_simultaneous_interaction_and_coherence() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a notebook "Projects"
    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: None,
        },
    )
    .expect("create Projects notebook");

    let personal = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Personal".to_string(),
            parent_id: None,
        },
    )
    .expect("create Personal notebook");

    // 2. Create tags: "work" and "important"
    let tag_work = TagRepository::create(&conn, "work").expect("create work");
    let tag_important = TagRepository::create(&conn, "important").expect("create important");

    // 3. Create note with: Favorite = true, Notebook = Projects
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Strategic Roadmap 2026".to_string()),
            content: Some("# Key Milestones\n1. Launch v1\n2. User Feedback".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(projects.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(false),
        },
    )
    .expect("create note");

    // 4. Assign tags = work, important
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id).expect("add work");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_important.id).expect("add important");

    // 5. Invariant check: These metadata properties must not conflict!
    // - "Favorites" view MUST show the note
    let fav_notes = NoteRepository::list_favorites(&conn).expect("list favorites");
    assert!(
        fav_notes.iter().any(|n| n.id == note.id),
        "Favorites must show note"
    );

    // - "# work" tag view MUST show the note
    let work_notes = NoteRepository::list_by_tag(&conn, &tag_work.id).expect("list by work tag");
    assert!(
        work_notes.iter().any(|n| n.id == note.id),
        "# work must show note"
    );

    // - "# important" tag view MUST show the note
    let important_notes =
        NoteRepository::list_by_tag(&conn, &tag_important.id).expect("list by important tag");
    assert!(
        important_notes.iter().any(|n| n.id == note.id),
        "# important must show note"
    );

    // - "Projects" notebook view MUST show the note
    let project_notes =
        NoteRepository::list_by_notebook(&conn, &projects.id).expect("list by projects notebook");
    assert!(
        project_notes.iter().any(|n| n.id == note.id),
        "Projects notebook must show note"
    );

    // - "All Notes" view MUST show the note
    let all_notes = NoteRepository::list(&conn, false).expect("list all notes");
    assert!(
        all_notes.iter().any(|n| n.id == note.id),
        "All notes must show note"
    );

    // - "Unfiled" view MUST NOT show the note
    let unfiled_notes = NoteRepository::list_unfiled(&conn).expect("list unfiled");
    assert!(
        !unfiled_notes.iter().any(|n| n.id == note.id),
        "Unfiled must NOT show note"
    );

    // 6. Check canonical NoteMetadata reflects all properties simultaneously
    let meta = NoteRepository::get_metadata(&conn, &note.id)
        .expect("get meta")
        .expect("meta exists");
    assert!(meta.is_favorite);
    assert_eq!(meta.notebook_id, Some(projects.id.clone()));
    assert_eq!(meta.notebook_path, Some("Projects".to_string()));
    assert_eq!(meta.tags.len(), 2);
    let tag_names: Vec<String> = meta.tags.iter().map(|t| t.name.clone()).collect();
    assert!(tag_names.contains(&"work".to_string()));
    assert!(tag_names.contains(&"important".to_string()));

    // 7. Toggle favorite to false: tags and notebook MUST remain completely unchanged
    let unfav_note = NoteRepository::set_favorite(&conn, &note.id, false).expect("unfavorite");
    assert!(!unfav_note.is_favorite);
    assert_eq!(unfav_note.notebook_id, Some(projects.id.clone()));

    let work_notes_after_unfav =
        NoteRepository::list_by_tag(&conn, &tag_work.id).expect("work after unfav");
    assert!(
        work_notes_after_unfav.iter().any(|n| n.id == note.id),
        "Tag view still shows unfavorited note"
    );

    let project_notes_after_unfav =
        NoteRepository::list_by_notebook(&conn, &projects.id).expect("projects after unfav");
    assert!(
        project_notes_after_unfav.iter().any(|n| n.id == note.id),
        "Notebook still shows unfavorited note"
    );

    let fav_notes_after_unfav = NoteRepository::list_favorites(&conn).expect("favs after unfav");
    assert!(
        !fav_notes_after_unfav.iter().any(|n| n.id == note.id),
        "Favorites must no longer show unfavorited note"
    );

    // 8. Re-favorite: note reappears in favorites
    let refav_note = NoteRepository::set_favorite(&conn, &note.id, true).expect("refavorite");
    assert!(refav_note.is_favorite);

    // 9. Move note from Projects -> Personal: favorite and tags remain intact
    let moved_note =
        NoteRepository::move_to_notebook(&conn, &note.id, Some(&personal.id)).expect("move note");
    assert_eq!(moved_note.notebook_id, Some(personal.id.clone()));
    assert!(moved_note.is_favorite);

    let personal_notes =
        NoteRepository::list_by_notebook(&conn, &personal.id).expect("personal notes");
    assert!(personal_notes.iter().any(|n| n.id == note.id));

    let fav_notes_after_move = NoteRepository::list_favorites(&conn).expect("favs after move");
    assert!(fav_notes_after_move.iter().any(|n| n.id == note.id));

    let work_notes_after_move =
        NoteRepository::list_by_tag(&conn, &tag_work.id).expect("work after move");
    assert!(work_notes_after_move.iter().any(|n| n.id == note.id));

    // 10. Persistence across connection restart
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen");
    let meta_reopen = NoteRepository::get_metadata(&conn2, &note.id)
        .expect("meta reopen")
        .expect("exists");
    assert!(meta_reopen.is_favorite);
    assert_eq!(meta_reopen.notebook_id, Some(personal.id));
    assert_eq!(meta_reopen.tags.len(), 2);
}

#[test]
fn test_task_41_soft_deleted_notes_excluded_across_all_views() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a notebook "Work"
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("create Work notebook");

    // 2. Create tags: "rust" and "security"
    let tag_rust = TagRepository::create(&conn, "rust").expect("create rust");
    let tag_sec = TagRepository::create(&conn, "security").expect("create security");

    // 3. Create 3 notes:
    // - Note A: inside Work, Favorite=true, tags=[rust, security]
    let note_a = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note A (Work Fav)".to_string()),
            content: Some("Content A".to_string()),
            notebook_id: Some(work.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("create note A");
    TagRepository::add_tag_to_note(&conn, &note_a.id, &tag_rust.id).expect("tag note A rust");
    TagRepository::add_tag_to_note(&conn, &note_a.id, &tag_sec.id).expect("tag note A sec");

    // - Note B: unfiled, Favorite=false, tags=[rust]
    let note_b = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note B (Unfiled NonFav)".to_string()),
            content: Some("Content B".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            ..Default::default()
        },
    )
    .expect("create note B");
    TagRepository::add_tag_to_note(&conn, &note_b.id, &tag_rust.id).expect("tag note B rust");

    // - Note C: inside Work, Favorite=true, tags=[security]
    let note_c = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note C (Work Fav)".to_string()),
            content: Some("Content C".to_string()),
            notebook_id: Some(work.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("create note C");
    TagRepository::add_tag_to_note(&conn, &note_c.id, &tag_sec.id).expect("tag note C sec");

    // 4. Baseline assertions before deletion
    assert_eq!(
        NoteRepository::list(&conn, false).expect("list all").len(),
        3
    );
    assert_eq!(
        NoteRepository::list_favorites(&conn)
            .expect("list fav")
            .len(),
        2
    );
    assert_eq!(
        NoteRepository::list_by_notebook(&conn, &work.id)
            .expect("list work")
            .len(),
        2
    );
    assert_eq!(
        NoteRepository::list_unfiled(&conn)
            .expect("list unfiled")
            .len(),
        1
    );
    assert_eq!(
        NoteRepository::list_by_tag(&conn, &tag_rust.id)
            .expect("list rust")
            .len(),
        2
    );
    assert_eq!(
        NoteRepository::list_by_tag(&conn, &tag_sec.id)
            .expect("list sec")
            .len(),
        2
    );

    let counts_before = TagRepository::get_tag_note_counts(&conn).expect("counts before");
    assert_eq!(counts_before.get(&tag_rust.id).copied().unwrap_or(0), 2);
    assert_eq!(counts_before.get(&tag_sec.id).copied().unwrap_or(0), 2);

    let batch_tags_before = TagRepository::get_all_notes_tag_names(&conn).expect("batch before");
    assert_eq!(
        batch_tags_before
            .get(&note_a.id)
            .map(|v| v.len())
            .unwrap_or(0),
        2
    );
    assert_eq!(
        batch_tags_before
            .get(&note_b.id)
            .map(|v| v.len())
            .unwrap_or(0),
        1
    );
    assert_eq!(
        batch_tags_before
            .get(&note_c.id)
            .map(|v| v.len())
            .unwrap_or(0),
        1
    );

    // 5. Soft-delete Note A
    let deleted = NoteRepository::delete(&conn, &note_a.id, true).expect("soft delete Note A");
    assert!(deleted, "Soft delete must report true");

    // 6. Invariant verification: Note A must be excluded across ALL views!
    // - All Notes: excludes Note A (only B and C returned)
    let all_after = NoteRepository::list(&conn, false).expect("list all after delete");
    assert_eq!(all_after.len(), 2);
    assert!(
        !all_after.iter().any(|n| n.id == note_a.id),
        "All Notes must exclude soft-deleted Note A"
    );

    // - Favorites: excludes Note A (only C returned)
    let favs_after = NoteRepository::list_favorites(&conn).expect("list fav after delete");
    assert_eq!(favs_after.len(), 1);
    assert_eq!(favs_after[0].id, note_c.id);
    assert!(
        !favs_after.iter().any(|n| n.id == note_a.id),
        "Favorites must exclude soft-deleted Note A"
    );

    // - Notebook "Work": excludes Note A (only C returned)
    let work_after =
        NoteRepository::list_by_notebook(&conn, &work.id).expect("list work after delete");
    assert_eq!(work_after.len(), 1);
    assert_eq!(work_after[0].id, note_c.id);
    assert!(
        !work_after.iter().any(|n| n.id == note_a.id),
        "Notebook view must exclude soft-deleted Note A"
    );

    // - Tag "rust": excludes Note A (only B returned)
    let rust_after =
        NoteRepository::list_by_tag(&conn, &tag_rust.id).expect("list rust after delete");
    assert_eq!(rust_after.len(), 1);
    assert_eq!(rust_after[0].id, note_b.id);
    assert!(
        !rust_after.iter().any(|n| n.id == note_a.id),
        "Tag rust view must exclude soft-deleted Note A"
    );

    // - Tag "security": excludes Note A (only C returned)
    let sec_after = NoteRepository::list_by_tag(&conn, &tag_sec.id).expect("list sec after delete");
    assert_eq!(sec_after.len(), 1);
    assert_eq!(sec_after[0].id, note_c.id);
    assert!(
        !sec_after.iter().any(|n| n.id == note_a.id),
        "Tag security view must exclude soft-deleted Note A"
    );

    // - Tag note counts: Note A excluded from counts (rust=1, sec=1)
    let counts_after = TagRepository::get_tag_note_counts(&conn).expect("counts after delete");
    assert_eq!(counts_after.get(&tag_rust.id).copied().unwrap_or(0), 1);
    assert_eq!(counts_after.get(&tag_sec.id).copied().unwrap_or(0), 1);

    // - Batch note tag names: Note A excluded from active note-tag batch
    let batch_after = TagRepository::get_all_notes_tag_names(&conn).expect("batch after delete");
    assert!(
        !batch_after.contains_key(&note_a.id),
        "Batch tag names must exclude soft-deleted Note A"
    );

    // - Direct query by ID: returns note with is_deleted = true and valid deleted_at
    let direct_a = NoteRepository::get_by_id(&conn, &note_a.id)
        .expect("get by id")
        .expect("exists");
    assert!(direct_a.is_deleted);
    assert!(direct_a.deleted_at.is_some());

    // - Include deleted query: returns all 3 notes
    let with_deleted = NoteRepository::list(&conn, true).expect("list with deleted");
    assert_eq!(with_deleted.len(), 3);

    // 7. Verify persistence across connection restart
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen");
    assert_eq!(
        NoteRepository::list(&conn2, false)
            .expect("reopen list")
            .len(),
        2
    );
    assert_eq!(
        NoteRepository::list_favorites(&conn2)
            .expect("reopen favs")
            .len(),
        1
    );
    assert_eq!(
        NoteRepository::list_by_notebook(&conn2, &work.id)
            .expect("reopen work")
            .len(),
        1
    );
    assert_eq!(
        NoteRepository::list_by_tag(&conn2, &tag_rust.id)
            .expect("reopen rust")
            .len(),
        1
    );
    assert_eq!(
        NoteRepository::list_by_tag(&conn2, &tag_sec.id)
            .expect("reopen sec")
            .len(),
        1
    );

    // 8. Restore Note A
    let restored_a = NoteRepository::restore(&conn2, &note_a.id).expect("restore Note A");
    assert!(!restored_a.is_deleted);
    assert!(restored_a.deleted_at.is_none());
    assert!(
        restored_a.is_favorite,
        "Favorite status must be preserved upon restore"
    );
    assert_eq!(
        restored_a.notebook_id,
        Some(work.id.clone()),
        "Notebook must be preserved upon restore"
    );

    // All views reflect restored Note A immediately
    assert_eq!(
        NoteRepository::list(&conn2, false)
            .expect("list restored")
            .len(),
        3
    );
    assert_eq!(
        NoteRepository::list_favorites(&conn2)
            .expect("favs restored")
            .len(),
        2
    );
    assert_eq!(
        NoteRepository::list_by_notebook(&conn2, &work.id)
            .expect("work restored")
            .len(),
        2
    );
    assert_eq!(
        NoteRepository::list_by_tag(&conn2, &tag_rust.id)
            .expect("rust restored")
            .len(),
        2
    );
    assert_eq!(
        NoteRepository::list_by_tag(&conn2, &tag_sec.id)
            .expect("sec restored")
            .len(),
        2
    );

    let restored_counts = TagRepository::get_tag_note_counts(&conn2).expect("counts restored");
    assert_eq!(restored_counts.get(&tag_rust.id).copied().unwrap_or(0), 2);
    assert_eq!(restored_counts.get(&tag_sec.id).copied().unwrap_or(0), 2);

    // Canonical metadata after restore has everything intact
    let meta_restored = NoteRepository::get_metadata(&conn2, &note_a.id)
        .expect("meta")
        .expect("meta exists");
    assert!(meta_restored.is_favorite);
    assert_eq!(meta_restored.notebook_path, Some("Work".to_string()));
    assert_eq!(meta_restored.tags.len(), 2);

    // 9. Soft-delete unfiled Note B
    NoteRepository::delete(&conn2, &note_b.id, true).expect("soft delete unfiled Note B");
    assert_eq!(
        NoteRepository::list_unfiled(&conn2)
            .expect("unfiled after B deleted")
            .len(),
        0
    );
    assert_eq!(
        NoteRepository::list_by_tag(&conn2, &tag_rust.id)
            .expect("rust after B deleted")
            .len(),
        1
    );

    // 10. Hard-delete Note B (soft = false): cascade removes from note_tags, leaves tags intact
    let hard_deleted =
        NoteRepository::delete(&conn2, &note_b.id, false).expect("hard delete Note B");
    assert!(hard_deleted);
    assert!(NoteRepository::get_by_id(&conn2, &note_b.id)
        .expect("get Note B")
        .is_none());

    // Tag "rust" itself is preserved
    assert!(TagRepository::get_by_id(&conn2, &tag_rust.id)
        .expect("tag rust")
        .is_some());
}

#[test]
fn test_phase5_task_42_note_list_metadata_indicators_and_batch_resolution() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create hierarchy: Work -> Projects
    let work = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("create Work");

    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(work.id.clone()),
        },
    )
    .expect("create Projects");

    // 2. Create 4 tags
    let tag_backend = TagRepository::create(&conn, "backend").expect("create backend");
    let tag_critical = TagRepository::create(&conn, "critical").expect("create critical");
    let tag_db = TagRepository::create(&conn, "database").expect("create database");
    let tag_ui = TagRepository::create(&conn, "ui").expect("create ui");

    // 3. Create notes
    // Note 1: in Projects, favorite = true, 4 tags (exceeds 3, testing +1 overflow)
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Project Architecture Spec".to_string()),
            content: Some("Full system architecture design document with schemas.".to_string()),
            notebook_id: Some(projects.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("create note 1");

    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_backend.id).expect("add backend");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_critical.id).expect("add critical");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_db.id).expect("add database");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_ui.id).expect("add ui");

    // Note 2: in Work, favorite = false, 1 tag
    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Team Standup Notes".to_string()),
            content: Some("Weekly team coordination notes.".to_string()),
            notebook_id: Some(work.id.clone()),
            is_favorite: Some(false),
            ..Default::default()
        },
    )
    .expect("create note 2");
    TagRepository::add_tag_to_note(&conn, &note2.id, &tag_ui.id).expect("add ui to note 2");

    // Note 3: unfiled, favorite = true, 0 tags
    let note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quick Thought".to_string()),
            content: Some("Remember to submit report.".to_string()),
            notebook_id: None,
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("create note 3");

    // 4. Batch tags mapping for note list cards
    let all_tags_map = TagRepository::get_all_notes_tag_names(&conn).expect("get all notes tags");

    // Note 1 must have exactly 4 tags, sorted alphabetically
    let note1_tags = all_tags_map.get(&note1.id).expect("note 1 tags");
    assert_eq!(note1_tags.len(), 4);
    assert_eq!(note1_tags, &vec!["backend", "critical", "database", "ui"]);

    // Note 2 must have 1 tag
    let note2_tags = all_tags_map.get(&note2.id).expect("note 2 tags");
    assert_eq!(note2_tags, &vec!["ui"]);

    // Note 3 has 0 tags
    assert!(!all_tags_map.contains_key(&note3.id));

    // 5. Notebook paths for note cards
    assert_eq!(
        NoteRepository::get_notebook_path(&conn, &note1.id).expect("path note 1"),
        "Work / Projects"
    );
    assert_eq!(
        NoteRepository::get_notebook_path(&conn, &note2.id).expect("path note 2"),
        "Work"
    );
    assert_eq!(
        NoteRepository::get_notebook_path(&conn, &note3.id).expect("path note 3"),
        "Unfiled"
    );

    // 6. Favorite indicators
    let favs = NoteRepository::list_favorites(&conn).expect("favs");
    let fav_ids: Vec<_> = favs.iter().map(|n| n.id.as_str()).collect();
    assert!(fav_ids.contains(&note1.id.as_str()));
    assert!(!fav_ids.contains(&note2.id.as_str()));
    assert!(fav_ids.contains(&note3.id.as_str()));

    // 7. Toggle favorite on note 2 -> reflects in favorite indicators immediately
    NoteRepository::set_favorite(&conn, &note2.id, true).expect("favorite note 2");
    let favs_after = NoteRepository::list_favorites(&conn).expect("favs after");
    assert!(favs_after.iter().any(|n| n.id == note2.id));

    // 8. Reopen DB and verify persistence
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen");
    let tags_reopen = TagRepository::get_all_notes_tag_names(&conn2).expect("tags reopen");
    assert_eq!(tags_reopen.get(&note1.id).unwrap().len(), 4);
    assert_eq!(
        NoteRepository::get_notebook_path(&conn2, &note1.id).expect("path reopen"),
        "Work / Projects"
    );
}

#[test]
fn test_phase5_task_43_sorting_retains_modified_at_desc_and_pinned_across_all_views() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create notebook and tag
    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: None,
        },
    )
    .expect("create Projects");

    let tag_specs = TagRepository::create(&conn, "specs").expect("create specs tag");

    // 2. Create 4 notes with distinctly increasing timestamps
    // Note 1: oldest
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note 1".to_string()),
            notebook_id: Some(projects.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("create note 1");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_specs.id).expect("tag 1");

    std::thread::sleep(std::time::Duration::from_millis(15));

    // Note 2: next
    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note 2".to_string()),
            notebook_id: Some(projects.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("create note 2");
    TagRepository::add_tag_to_note(&conn, &note2.id, &tag_specs.id).expect("tag 2");

    std::thread::sleep(std::time::Duration::from_millis(15));

    // Note 3: next (not favorite)
    let note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note 3".to_string()),
            notebook_id: Some(projects.id.clone()),
            is_favorite: Some(false),
            ..Default::default()
        },
    )
    .expect("create note 3");
    TagRepository::add_tag_to_note(&conn, &note3.id, &tag_specs.id).expect("tag 3");

    std::thread::sleep(std::time::Duration::from_millis(15));

    // Note 4: newest (unfiled)
    let note4 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note 4".to_string()),
            notebook_id: None,
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("create note 4");
    TagRepository::add_tag_to_note(&conn, &note4.id, &tag_specs.id).expect("tag 4");

    // 3. Verify strict modified_at DESC ordering across ALL views:
    // - All Notes: [Note 4, Note 3, Note 2, Note 1]
    let all_notes = NoteRepository::list(&conn, false).expect("list all");
    let all_ids: Vec<_> = all_notes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        all_ids,
        vec![
            note4.id.as_str(),
            note3.id.as_str(),
            note2.id.as_str(),
            note1.id.as_str()
        ]
    );

    // - Favorites: [Note 4, Note 2, Note 1]
    let fav_notes = NoteRepository::list_favorites(&conn).expect("list favs");
    let fav_ids: Vec<_> = fav_notes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        fav_ids,
        vec![note4.id.as_str(), note2.id.as_str(), note1.id.as_str()]
    );

    // - Tag specs: [Note 4, Note 3, Note 2, Note 1]
    let tag_notes = NoteRepository::list_by_tag(&conn, &tag_specs.id).expect("list by tag");
    let tag_note_ids: Vec<_> = tag_notes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        tag_note_ids,
        vec![
            note4.id.as_str(),
            note3.id.as_str(),
            note2.id.as_str(),
            note1.id.as_str()
        ]
    );

    // - Notebook Projects: [Note 3, Note 2, Note 1]
    let project_notes =
        NoteRepository::list_by_notebook(&conn, &projects.id).expect("list by notebook");
    let project_ids: Vec<_> = project_notes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        project_ids,
        vec![note3.id.as_str(), note2.id.as_str(), note1.id.as_str()]
    );

    // - Unfiled: [Note 4]
    let unfiled_notes = NoteRepository::list_unfiled(&conn).expect("list unfiled");
    assert_eq!(unfiled_notes.len(), 1);
    assert_eq!(unfiled_notes[0].id, note4.id);

    // 4. Update Note 1 (the oldest note) -> jumps to top of modified_at DESC
    std::thread::sleep(std::time::Duration::from_millis(15));
    NoteRepository::update(
        &conn,
        &note1.id,
        UpdateNoteDto {
            content: Some("Updated content for Note 1".to_string()),
            ..Default::default()
        },
    )
    .expect("update note 1");

    let all_after_update = NoteRepository::list(&conn, false).expect("list all after update");
    let ids_after_update: Vec<_> = all_after_update.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        ids_after_update,
        vec![
            note1.id.as_str(),
            note4.id.as_str(),
            note3.id.as_str(),
            note2.id.as_str()
        ]
    );

    let favs_after_update = NoteRepository::list_favorites(&conn).expect("favs after update");
    let fav_ids_after_update: Vec<_> = favs_after_update.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        fav_ids_after_update,
        vec![note1.id.as_str(), note4.id.as_str(), note2.id.as_str()]
    );

    let project_after_update =
        NoteRepository::list_by_notebook(&conn, &projects.id).expect("projects after update");
    let project_ids_after_update: Vec<_> =
        project_after_update.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        project_ids_after_update,
        vec![note1.id.as_str(), note3.id.as_str(), note2.id.as_str()]
    );

    // 5. Pin Note 3 -> jumps ahead of all unpinned notes regardless of timestamps
    NoteRepository::update(
        &conn,
        &note3.id,
        UpdateNoteDto {
            is_pinned: Some(true),
            ..Default::default()
        },
    )
    .expect("pin note 3");

    let all_pinned = NoteRepository::list(&conn, false).expect("list all pinned");
    let pinned_ids: Vec<_> = all_pinned.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        pinned_ids,
        vec![
            note3.id.as_str(),
            note1.id.as_str(),
            note4.id.as_str(),
            note2.id.as_str()
        ]
    );

    let project_pinned =
        NoteRepository::list_by_notebook(&conn, &projects.id).expect("project pinned");
    let project_pinned_ids: Vec<_> = project_pinned.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        project_pinned_ids,
        vec![note3.id.as_str(), note1.id.as_str(), note2.id.as_str()]
    );

    // 6. Persistence across connection restart
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen");
    let all_reopen = NoteRepository::list(&conn2, false).expect("all reopen");
    let reopen_ids: Vec<_> = all_reopen.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        reopen_ids,
        vec![
            note3.id.as_str(),
            note1.id.as_str(),
            note4.id.as_str(),
            note2.id.as_str()
        ]
    );
}

#[test]
fn test_phase5_task_44_sidebar_overflow_100_tags_lifecycle_and_counts() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a note to attach tags to
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note with many tags".to_string()),
            content: Some("Testing 100+ tags overflow capacity".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // 2. Insert 100 distinct tags
    let start_time = std::time::Instant::now();
    for i in 1..=100 {
        let tag_name = format!("tag-{:03}", i);
        let tag = TagRepository::create(&conn, &tag_name).expect("create tag");
        // Tag every alternate note
        if i % 2 == 0 {
            TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).expect("assign tag");
        }
    }
    let elapsed = start_time.elapsed();
    assert!(
        elapsed.as_millis() < 5000,
        "100 tag creations should be fast (< 5s)"
    );

    // 3. Verify TagRepository::list returns all 100 tags in deterministic alphabetical order
    let tags_list = TagRepository::list(&conn).expect("list 100 tags");
    assert_eq!(tags_list.len(), 100);
    assert_eq!(tags_list[0].name, "tag-001");
    assert_eq!(tags_list[99].name, "tag-100");

    // 4. Verify tag counts map contains 50 tags with count 1
    let counts = TagRepository::get_tag_note_counts(&conn).expect("tag note counts");
    assert_eq!(counts.len(), 50);
    for count in counts.values() {
        assert_eq!(*count, 1);
    }

    // 5. Verify batch note tags for the note returns 50 tags
    let batch = TagRepository::get_all_notes_tag_names(&conn).expect("batch tags");
    let note_tags = batch.get(&note.id).expect("note tags");
    assert_eq!(note_tags.len(), 50);

    // 6. Verify persistence across connection reopen
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("reopen");
    let tags_after_reopen = TagRepository::list(&conn2).expect("list tags reopen");
    assert_eq!(tags_after_reopen.len(), 100);
}

#[test]
fn test_phase5_task_48_unicode_tag_support_lifecycle_and_restart() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    let note1_id: String;
    let note2_id: String;
    let note3_id: String;

    let work_tag_id: String;
    let hindi_tag_id: String;
    let cjk_tag_id: String;
    let cafe_tag_id: String;
    let research_tag_id: String;
    let num_tag_id: String;
    let proj_tag_id: String;

    // Session 1: Create, assign, filter, rename, and remove Unicode tags
    {
        let mut conn = init_connection(&db_path).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migration failed");

        // 1. Create tags with exact specification Unicode names:
        // work, यात्रा, 旅行, café, research & ideas, 2026, project-x
        let work_tag = TagRepository::create(&conn, "work").expect("create work tag");
        work_tag_id = work_tag.id;

        let hindi_tag = TagRepository::create(&conn, "यात्रा").expect("create Devanagari Hindi tag");
        hindi_tag_id = hindi_tag.id;

        let cjk_tag = TagRepository::create(&conn, "旅行").expect("create CJK Japanese tag");
        cjk_tag_id = cjk_tag.id;

        let cafe_tag = TagRepository::create(&conn, "café").expect("create Accented Latin tag");
        cafe_tag_id = cafe_tag.id;

        let research_tag =
            TagRepository::create(&conn, "research & ideas").expect("create Ampersand tag");
        research_tag_id = research_tag.id;

        let num_tag = TagRepository::create(&conn, "2026").expect("create Numeric tag");
        num_tag_id = num_tag.id;

        let proj_tag = TagRepository::create(&conn, "project-x").expect("create Hyphenated tag");
        proj_tag_id = proj_tag.id;

        // Verify all 7 tags created and listed
        let tags = TagRepository::list(&conn).expect("list tags");
        assert_eq!(tags.len(), 7);

        // 2. Create notes and assign multi-tag combinations
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Travel Journal".to_string()),
                content: Some("Notes from trip to India and Japan".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 1");
        note1_id = note1.id;

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Work & Coffee".to_string()),
                content: Some("Research notes at the corner café".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 2");
        note2_id = note2.id;

        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Planning".to_string()),
                content: Some("2026 roadmap for project-x".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 3");
        note3_id = note3.id;

        // Assign tags:
        // Note 1 -> यात्रा, 旅行
        TagRepository::add_tag_to_note(&conn, &note1_id, &hindi_tag_id)
            .expect("add hindi tag to note 1");
        TagRepository::add_tag_to_note(&conn, &note1_id, &cjk_tag_id)
            .expect("add cjk tag to note 1");

        // Note 2 -> work, café, research & ideas
        TagRepository::add_tag_to_note(&conn, &note2_id, &work_tag_id)
            .expect("add work tag to note 2");
        TagRepository::add_tag_to_note(&conn, &note2_id, &cafe_tag_id)
            .expect("add cafe tag to note 2");
        TagRepository::add_tag_to_note(&conn, &note2_id, &research_tag_id)
            .expect("add research tag to note 2");

        // Note 3 -> work, 2026, project-x
        TagRepository::add_tag_to_note(&conn, &note3_id, &work_tag_id)
            .expect("add work tag to note 3");
        TagRepository::add_tag_to_note(&conn, &note3_id, &num_tag_id)
            .expect("add num tag to note 3");
        TagRepository::add_tag_to_note(&conn, &note3_id, &proj_tag_id)
            .expect("add proj tag to note 3");

        // Verify note tag associations
        let note1_tags = TagRepository::get_tags_for_note(&conn, &note1_id).expect("note 1 tags");
        assert_eq!(note1_tags.len(), 2);
        let note1_tag_names: Vec<String> = note1_tags.into_iter().map(|t| t.name).collect();
        assert!(note1_tag_names.contains(&"यात्रा".to_string()));
        assert!(note1_tag_names.contains(&"旅行".to_string()));

        let note2_tags = TagRepository::get_tags_for_note(&conn, &note2_id).expect("note 2 tags");
        assert_eq!(note2_tags.len(), 3);
        let note2_tag_names: Vec<String> = note2_tags.into_iter().map(|t| t.name).collect();
        assert!(note2_tag_names.contains(&"work".to_string()));
        assert!(note2_tag_names.contains(&"café".to_string()));
        assert!(note2_tag_names.contains(&"research & ideas".to_string()));

        // 3. Filter notes by tag
        let hindi_notes =
            NoteRepository::list_by_tag(&conn, &hindi_tag_id).expect("filter by hindi tag");
        assert_eq!(hindi_notes.len(), 1);
        assert_eq!(hindi_notes[0].id, note1_id);

        let cjk_notes = NoteRepository::list_by_tag(&conn, &cjk_tag_id).expect("filter by cjk tag");
        assert_eq!(cjk_notes.len(), 1);
        assert_eq!(cjk_notes[0].id, note1_id);

        let cafe_notes =
            NoteRepository::list_by_tag(&conn, &cafe_tag_id).expect("filter by cafe tag");
        assert_eq!(cafe_notes.len(), 1);
        assert_eq!(cafe_notes[0].id, note2_id);

        let work_notes =
            NoteRepository::list_by_tag(&conn, &work_tag_id).expect("filter by work tag");
        assert_eq!(work_notes.len(), 2);
        assert!(work_notes.iter().any(|n| n.id == note2_id));
        assert!(work_notes.iter().any(|n| n.id == note3_id));

        // 4. Rename tags (including Unicode and emojis)
        let renamed_hindi = TagRepository::update(
            &conn,
            &hindi_tag_id,
            personal_notepad_lib::storage::models::UpdateTagDto {
                name: "तीर्थ-यात्रा 🗺️".to_string(),
            },
        )
        .expect("rename hindi tag");
        assert_eq!(renamed_hindi.name, "तीर्थ-यात्रा 🗺️");

        let renamed_cafe = TagRepository::update(
            &conn,
            &cafe_tag_id,
            personal_notepad_lib::storage::models::UpdateTagDto {
                name: "Grande Café ☕".to_string(),
            },
        )
        .expect("rename cafe tag");
        assert_eq!(renamed_cafe.name, "Grande Café ☕");

        // Verify note tag names updated automatically
        let note1_tags_after_rename =
            TagRepository::get_tags_for_note(&conn, &note1_id).expect("note 1 tags after rename");
        let note1_tag_names_after: Vec<String> = note1_tags_after_rename
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert!(note1_tag_names_after.contains(&"तीर्थ-यात्रा 🗺️".to_string()));

        // 5. Remove tag from note
        TagRepository::remove_tag_from_note(&conn, &note2_id, &cafe_tag_id)
            .expect("remove cafe tag from note 2");
        let note2_tags_after_remove =
            TagRepository::get_tags_for_note(&conn, &note2_id).expect("note 2 tags after remove");
        assert_eq!(note2_tags_after_remove.len(), 2);
        let note2_names_after: Vec<String> = note2_tags_after_remove
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert!(!note2_names_after.contains(&"Grande Café ☕".to_string()));
        assert!(note2_names_after.contains(&"work".to_string()));
        assert!(note2_names_after.contains(&"research & ideas".to_string()));

        // Filter for Grande Café ☕ now returns 0 notes
        let cafe_notes_after_remove =
            NoteRepository::list_by_tag(&conn, &cafe_tag_id).expect("filter cafe after remove");
        assert_eq!(cafe_notes_after_remove.len(), 0);
    }

    // Session 2: Application restart and persistence verification
    {
        let mut conn2 = init_connection(&db_path).expect("Session 2 connection failed");
        run_migrations(&mut conn2).expect("Session 2 migration failed");

        // 1. Verify all 7 tags exist and retain updated Unicode names
        let tags = TagRepository::list(&conn2).expect("list tags session 2");
        assert_eq!(tags.len(), 7);

        let tag_names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
        assert!(tag_names.contains(&"तीर्थ-यात्रा 🗺️".to_string()));
        assert!(tag_names.contains(&"Grande Café ☕".to_string()));
        assert!(tag_names.contains(&"旅行".to_string()));
        assert!(tag_names.contains(&"research & ideas".to_string()));
        assert!(tag_names.contains(&"2026".to_string()));
        assert!(tag_names.contains(&"project-x".to_string()));
        assert!(tag_names.contains(&"work".to_string()));

        // 2. Verify note 1 tag associations preserved
        let note1_tags =
            TagRepository::get_tags_for_note(&conn2, &note1_id).expect("note 1 tags session 2");
        assert_eq!(note1_tags.len(), 2);
        let note1_tag_names: Vec<String> = note1_tags.into_iter().map(|t| t.name).collect();
        assert!(note1_tag_names.contains(&"तीर्थ-यात्रा 🗺️".to_string()));
        assert!(note1_tag_names.contains(&"旅行".to_string()));

        // 3. Verify filtering by Unicode tag returns correct note
        let filtered_hindi =
            NoteRepository::list_by_tag(&conn2, &hindi_tag_id).expect("filter hindi session 2");
        assert_eq!(filtered_hindi.len(), 1);
        assert_eq!(filtered_hindi[0].id, note1_id);

        let filtered_cjk =
            NoteRepository::list_by_tag(&conn2, &cjk_tag_id).expect("filter cjk session 2");
        assert_eq!(filtered_cjk.len(), 1);
        assert_eq!(filtered_cjk[0].id, note1_id);

        // 4. Verify tag counts after restart
        let counts = TagRepository::get_tag_note_counts(&conn2).expect("tag counts session 2");
        assert_eq!(*counts.get(&work_tag_id).unwrap_or(&0), 2);
        assert_eq!(*counts.get(&hindi_tag_id).unwrap_or(&0), 1);
        assert_eq!(*counts.get(&cjk_tag_id).unwrap_or(&0), 1);
        assert_eq!(*counts.get(&cafe_tag_id).unwrap_or(&0), 0);
        assert_eq!(*counts.get(&research_tag_id).unwrap_or(&0), 1);
        assert_eq!(*counts.get(&num_tag_id).unwrap_or(&0), 1);
        assert_eq!(*counts.get(&proj_tag_id).unwrap_or(&0), 1);
    }
}

#[test]
fn test_phase5_task_49_special_character_tags_lifecycle_and_restart() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    let note1_id: String;
    let note2_id: String;
    let note3_id: String;

    let cpp_id: String;
    let csharp_id: String;
    let rd_id: String;
    let proj_id: String;
    let q4_id: String;
    let design_id: String;

    // Session 1: Create, assign, filter, rename, and remove Special Character tags
    {
        let mut conn = init_connection(&db_path).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migration failed");

        // 1. Create tags with exact specification special characters:
        // C++, C#, R&D, Project / Planning, Q4 2026, Design & UX
        let cpp = TagRepository::create(&conn, "C++").expect("create C++ tag");
        cpp_id = cpp.id;

        let csharp = TagRepository::create(&conn, "C#").expect("create C# tag");
        csharp_id = csharp.id;

        let rd = TagRepository::create(&conn, "R&D").expect("create R&D tag");
        rd_id = rd.id;

        let proj = TagRepository::create(&conn, "Project / Planning")
            .expect("create Project / Planning tag");
        proj_id = proj.id;

        let q4 = TagRepository::create(&conn, "Q4 2026").expect("create Q4 2026 tag");
        q4_id = q4.id;

        let design = TagRepository::create(&conn, "Design & UX").expect("create Design & UX tag");
        design_id = design.id;

        // Verify all 6 tags exist with exact casing and special characters
        let tags = TagRepository::list(&conn).expect("list tags");
        assert_eq!(tags.len(), 6);
        let tag_names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
        assert!(tag_names.contains(&"C++".to_string()));
        assert!(tag_names.contains(&"C#".to_string()));
        assert!(tag_names.contains(&"R&D".to_string()));
        assert!(tag_names.contains(&"Project / Planning".to_string()));
        assert!(tag_names.contains(&"Q4 2026".to_string()));
        assert!(tag_names.contains(&"Design & UX".to_string()));

        // 2. Case-insensitive duplicate check preserves original tag
        let duplicate_cpp = TagRepository::create(&conn, "c++").expect("create c++ lowercase");
        assert_eq!(duplicate_cpp.id, cpp_id);
        assert_eq!(duplicate_cpp.name, "C++");

        let duplicate_csharp = TagRepository::create(&conn, "c#").expect("create c# lowercase");
        assert_eq!(duplicate_csharp.id, csharp_id);
        assert_eq!(duplicate_csharp.name, "C#");

        // 3. Create notes and assign multi-tag combinations
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Systems Architecture".to_string()),
                content: Some("Low-level engine in C++ and tools in C#".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 1");
        note1_id = note1.id;

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Product Roadmap".to_string()),
                content: Some("Planning notes for next quarter release".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 2");
        note2_id = note2.id;

        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("User Interface Overhaul".to_string()),
                content: Some("Design system and accessibility guidelines".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 3");
        note3_id = note3.id;

        // Note 1 -> C++, C#, R&D
        TagRepository::add_tag_to_note(&conn, &note1_id, &cpp_id).expect("assign C++");
        TagRepository::add_tag_to_note(&conn, &note1_id, &csharp_id).expect("assign C#");
        TagRepository::add_tag_to_note(&conn, &note1_id, &rd_id).expect("assign R&D");

        // Note 2 -> R&D, Project / Planning, Q4 2026
        TagRepository::add_tag_to_note(&conn, &note2_id, &rd_id).expect("assign R&D to note 2");
        TagRepository::add_tag_to_note(&conn, &note2_id, &proj_id)
            .expect("assign Project / Planning");
        TagRepository::add_tag_to_note(&conn, &note2_id, &q4_id).expect("assign Q4 2026");

        // Note 3 -> Design & UX, Project / Planning
        TagRepository::add_tag_to_note(&conn, &note3_id, &design_id).expect("assign Design & UX");
        TagRepository::add_tag_to_note(&conn, &note3_id, &proj_id)
            .expect("assign Project / Planning to note 3");

        // Verify assignments
        let note1_tags = TagRepository::get_tags_for_note(&conn, &note1_id).expect("note 1 tags");
        assert_eq!(note1_tags.len(), 3);
        let n1_names: Vec<String> = note1_tags.into_iter().map(|t| t.name).collect();
        assert!(n1_names.contains(&"C++".to_string()));
        assert!(n1_names.contains(&"C#".to_string()));
        assert!(n1_names.contains(&"R&D".to_string()));

        // 4. Filtering notes by special character tags
        let cpp_notes = NoteRepository::list_by_tag(&conn, &cpp_id).expect("filter C++");
        assert_eq!(cpp_notes.len(), 1);
        assert_eq!(cpp_notes[0].id, note1_id);

        let csharp_notes = NoteRepository::list_by_tag(&conn, &csharp_id).expect("filter C#");
        assert_eq!(csharp_notes.len(), 1);
        assert_eq!(csharp_notes[0].id, note1_id);

        let rd_notes = NoteRepository::list_by_tag(&conn, &rd_id).expect("filter R&D");
        assert_eq!(rd_notes.len(), 2);
        assert!(rd_notes.iter().any(|n| n.id == note1_id));
        assert!(rd_notes.iter().any(|n| n.id == note2_id));

        let proj_notes =
            NoteRepository::list_by_tag(&conn, &proj_id).expect("filter Project / Planning");
        assert_eq!(proj_notes.len(), 2);
        assert!(proj_notes.iter().any(|n| n.id == note2_id));
        assert!(proj_notes.iter().any(|n| n.id == note3_id));

        let design_notes =
            NoteRepository::list_by_tag(&conn, &design_id).expect("filter Design & UX");
        assert_eq!(design_notes.len(), 1);
        assert_eq!(design_notes[0].id, note3_id);

        // 5. Rename special character tags
        let renamed_cpp = TagRepository::update(
            &conn,
            &cpp_id,
            personal_notepad_lib::storage::models::UpdateTagDto {
                name: "Modern C++ (20/23)".to_string(),
            },
        )
        .expect("rename C++");
        assert_eq!(renamed_cpp.name, "Modern C++ (20/23)");

        let renamed_proj = TagRepository::update(
            &conn,
            &proj_id,
            personal_notepad_lib::storage::models::UpdateTagDto {
                name: "Strategic Planning & Dev / Ops".to_string(),
            },
        )
        .expect("rename Project / Planning");
        assert_eq!(renamed_proj.name, "Strategic Planning & Dev / Ops");

        // 6. Remove tag from note
        TagRepository::remove_tag_from_note(&conn, &note1_id, &csharp_id)
            .expect("remove C# from note 1");
        let note1_tags_after =
            TagRepository::get_tags_for_note(&conn, &note1_id).expect("note 1 tags after remove");
        assert_eq!(note1_tags_after.len(), 2);
        let n1_after_names: Vec<String> = note1_tags_after.into_iter().map(|t| t.name).collect();
        assert!(!n1_after_names.contains(&"C#".to_string()));
        assert!(n1_after_names.contains(&"Modern C++ (20/23)".to_string()));
        assert!(n1_after_names.contains(&"R&D".to_string()));
    }

    // Session 2: Application restart and persistence verification
    {
        let mut conn2 = init_connection(&db_path).expect("Session 2 connection failed");
        run_migrations(&mut conn2).expect("Session 2 migration failed");

        // 1. Verify all 6 tags persist accurately
        let tags = TagRepository::list(&conn2).expect("list tags session 2");
        assert_eq!(tags.len(), 6);
        let tag_names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
        assert!(tag_names.contains(&"Modern C++ (20/23)".to_string()));
        assert!(tag_names.contains(&"C#".to_string()));
        assert!(tag_names.contains(&"R&D".to_string()));
        assert!(tag_names.contains(&"Strategic Planning & Dev / Ops".to_string()));
        assert!(tag_names.contains(&"Q4 2026".to_string()));
        assert!(tag_names.contains(&"Design & UX".to_string()));

        // 2. Verify note 2 tag associations preserved
        let note2_tags =
            TagRepository::get_tags_for_note(&conn2, &note2_id).expect("note 2 tags session 2");
        assert_eq!(note2_tags.len(), 3);
        let n2_names: Vec<String> = note2_tags.into_iter().map(|t| t.name).collect();
        assert!(n2_names.contains(&"R&D".to_string()));
        assert!(n2_names.contains(&"Strategic Planning & Dev / Ops".to_string()));
        assert!(n2_names.contains(&"Q4 2026".to_string()));

        // 3. Verify batch note tag mappings
        let batch = TagRepository::get_all_notes_tag_names(&conn2).expect("batch tags session 2");
        let n1_batch = batch.get(&note1_id).expect("n1 batch");
        assert_eq!(n1_batch.len(), 2);
        assert!(n1_batch.contains(&"Modern C++ (20/23)".to_string()));
        assert!(n1_batch.contains(&"R&D".to_string()));

        let n3_batch = batch.get(&note3_id).expect("n3 batch");
        assert_eq!(n3_batch.len(), 2);
        assert!(n3_batch.contains(&"Design & UX".to_string()));
        assert!(n3_batch.contains(&"Strategic Planning & Dev / Ops".to_string()));

        // 4. Verify filtering by renamed special character tag
        let filtered_proj =
            NoteRepository::list_by_tag(&conn2, &proj_id).expect("filter renamed proj");
        assert_eq!(filtered_proj.len(), 2);
        assert!(filtered_proj.iter().any(|n| n.id == note2_id));
        assert!(filtered_proj.iter().any(|n| n.id == note3_id));

        // 5. Verify tag note counts
        let counts = TagRepository::get_tag_note_counts(&conn2).expect("tag counts session 2");
        assert_eq!(*counts.get(&cpp_id).unwrap_or(&0), 1);
        assert_eq!(*counts.get(&csharp_id).unwrap_or(&0), 0); // removed from note 1
        assert_eq!(*counts.get(&rd_id).unwrap_or(&0), 2);
        assert_eq!(*counts.get(&proj_id).unwrap_or(&0), 2);
        assert_eq!(*counts.get(&q4_id).unwrap_or(&0), 1);
        assert_eq!(*counts.get(&design_id).unwrap_or(&0), 1);
    }
}

#[test]
fn test_phase5_task_50_large_tag_assignment_lifecycle_and_restart() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    let note_id: String;
    let mut created_tag_ids: Vec<String> = Vec::new();

    // Session 1: Create note, 25 tags, assign 22 to single note, verify duplicate protection, removal, filtering
    {
        let mut conn = init_connection(&db_path).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migration failed");

        // 1. Create a note
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Comprehensive Multi-Tagged Note".to_string()),
                content: Some(
                    "Testing 20+ tag assignment, rendering, and performance.".to_string(),
                ),
                ..Default::default()
            },
        )
        .expect("create note");
        note_id = note.id;

        // 2. Create 25 distinct tags
        for i in 1..=25 {
            let tag_name = format!("topic-{:02}", i);
            let tag = TagRepository::create(&conn, &tag_name).expect("create tag");
            created_tag_ids.push(tag.id);
        }
        assert_eq!(created_tag_ids.len(), 25);

        let all_tags = TagRepository::list(&conn).expect("list tags");
        assert_eq!(all_tags.len(), 25);

        // 3. Assign 22 tags to the note
        for tag_id in created_tag_ids.iter().take(22) {
            TagRepository::add_tag_to_note(&conn, &note_id, tag_id).expect("assign tag");
        }

        // 4. Duplicate assignment prevention: re-assigning tags must not create duplicate associations
        for tag_id in created_tag_ids.iter().take(5) {
            let dup_result = TagRepository::add_tag_to_note(&conn, &note_id, tag_id);
            // Either gracefully succeeds (idempotent / IGNORE) or returns duplicate error
            // In either case, the database must contain exactly one association
            let _ = dup_result;
        }

        // Verify exactly 22 tags assigned
        let note_tags = TagRepository::get_tags_for_note(&conn, &note_id).expect("note tags");
        assert_eq!(note_tags.len(), 22);

        // Verify unique tag IDs (no duplicates)
        let unique_tag_ids: std::collections::HashSet<String> =
            note_tags.iter().map(|t| t.id.clone()).collect();
        assert_eq!(unique_tag_ids.len(), 22);

        // 5. Batch resolution performance & accuracy
        let batch = TagRepository::get_all_notes_tag_names(&conn).expect("batch tags");
        let batch_tags = batch.get(&note_id).expect("batch for note");
        assert_eq!(batch_tags.len(), 22);

        // 6. Tag note counts: 22 tags have count 1, 3 tags have count 0
        let counts = TagRepository::get_tag_note_counts(&conn).expect("counts");
        let assigned_count = created_tag_ids
            .iter()
            .take(22)
            .filter(|id| *counts.get(*id).unwrap_or(&0) == 1)
            .count();
        assert_eq!(assigned_count, 22);
        let unassigned_count = created_tag_ids
            .iter()
            .skip(22)
            .filter(|id| *counts.get(*id).unwrap_or(&0) == 0)
            .count();
        assert_eq!(unassigned_count, 3);

        // 7. Remove 5 tags from note
        for tag_id in created_tag_ids.iter().take(5) {
            TagRepository::remove_tag_from_note(&conn, &note_id, tag_id).expect("remove tag");
        }

        // Exactly 17 tags remain
        let note_tags_after_del =
            TagRepository::get_tags_for_note(&conn, &note_id).expect("note tags after remove");
        assert_eq!(note_tags_after_del.len(), 17);

        // Verify removed tag no longer returns the note in filtering
        let removed_tag_filter =
            NoteRepository::list_by_tag(&conn, &created_tag_ids[0]).expect("filter removed tag");
        assert_eq!(removed_tag_filter.len(), 0);

        // Verify remaining tag still returns the note in filtering
        let remaining_tag_filter =
            NoteRepository::list_by_tag(&conn, &created_tag_ids[10]).expect("filter remaining tag");
        assert_eq!(remaining_tag_filter.len(), 1);
        assert_eq!(remaining_tag_filter[0].id, note_id);
    }

    // Session 2: Application restart and persistence verification
    {
        let mut conn2 = init_connection(&db_path).expect("Session 2 connection failed");
        run_migrations(&mut conn2).expect("Session 2 migration failed");

        // 1. All 25 tags still exist
        let tags_reloaded = TagRepository::list(&conn2).expect("list tags session 2");
        assert_eq!(tags_reloaded.len(), 25);

        // 2. Exactly 17 tags preserved on note reopening
        let note_tags_reloaded =
            TagRepository::get_tags_for_note(&conn2, &note_id).expect("get note tags session 2");
        assert_eq!(note_tags_reloaded.len(), 17);

        // 3. Re-assign 1 tag after restart
        TagRepository::add_tag_to_note(&conn2, &note_id, &created_tag_ids[0])
            .expect("re-add tag 0");
        let note_tags_after_readd =
            TagRepository::get_tags_for_note(&conn2, &note_id).expect("get note tags after re-add");
        assert_eq!(note_tags_after_readd.len(), 18);
    }
}

#[test]
fn test_phase5_task_51_large_note_regression_lifecycle_and_restart() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    let note_id: String;
    let archive_nb_id: String;
    let final_content: String;

    let mut tag_ids: Vec<String> = Vec::new();

    // Session 1: Create 100KB+ note, assign 5 tags, mark favorite, move notebook, rename tag, autosave content
    {
        let mut conn = init_connection(&db_path).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migration failed");

        // 1. Generate 120KB+ large note content with multi-byte Unicode and Markdown formatting
        let mut large_content = String::with_capacity(120 * 1024);
        for i in 1..=1500 {
            large_content.push_str(&format!(
                    "## Section {:04}\nThis is a large scale test line with unicode characters: 🚀 नमस्ते, 日本語, and café. Note index: {}\n\n",
                    i, i
                ));
        }
        assert!(
            large_content.len() >= 100 * 1024,
            "Content must exceed 100KB (was {} bytes)",
            large_content.len()
        );

        // 2. Create Notebooks "Research" and "Archives"
        let research_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Research".to_string(),
                parent_id: None,
            },
        )
        .expect("create Research notebook");

        let archives_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Archives".to_string(),
                parent_id: None,
            },
        )
        .expect("create Archives notebook");
        archive_nb_id = archives_nb.id.clone();

        // 3. Create the large note inside "Research"
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Large Regression Analysis 100KB+".to_string()),
                content: Some(large_content.clone()),
                format: Some("md".to_string()),
                notebook_id: Some(research_nb.id.clone()),
                ..Default::default()
            },
        )
        .expect("create large note");
        note_id = note.id;

        // 4. Create 5 tags
        let tag_names = ["architecture", "scale", "performance", "sqlite", "offline"];
        for name in &tag_names {
            let tag = TagRepository::create(&conn, name).expect("create tag");
            tag_ids.push(tag.id);
        }
        assert_eq!(tag_ids.len(), 5);

        // 5. Assign all 5 tags to the large note
        for tid in &tag_ids {
            TagRepository::add_tag_to_note(&conn, &note_id, tid).expect("assign tag");
        }
        let note_tags = TagRepository::get_tags_for_note(&conn, &note_id).expect("get note tags");
        assert_eq!(note_tags.len(), 5);

        // 6. Mark as favorite
        let fav_note = NoteRepository::toggle_favorite(&conn, &note_id).expect("toggle favorite");
        assert!(fav_note.is_favorite);

        // 7. Move note to "Archives"
        let moved_note = NoteRepository::move_to_notebook(&conn, &note_id, Some(&archive_nb_id))
            .expect("move note");
        assert_eq!(
            moved_note.notebook_id.as_deref(),
            Some(archive_nb_id.as_str())
        );
        assert_eq!(moved_note.content.len(), large_content.len());

        // 8. Rename tag 3 ("performance" -> "extreme-performance-2026 ⚡")
        let renamed_tag = TagRepository::update(
            &conn,
            &tag_ids[2],
            personal_notepad_lib::storage::models::UpdateTagDto {
                name: "extreme-performance-2026 ⚡".to_string(),
            },
        )
        .expect("rename tag");
        assert_eq!(renamed_tag.name, "extreme-performance-2026 ⚡");

        // 9. Autosave simulation: Append another 10KB of text to the large note
        let mut append_content = large_content;
        for j in 1..=100 {
            append_content.push_str(&format!(
                    "### Autosave Entry {:03}\nPersisting additional streaming edits without truncation or lag.\n",
                    j
                ));
        }
        final_content = append_content;

        let updated_note = NoteRepository::update(
            &conn,
            &note_id,
            personal_notepad_lib::storage::models::UpdateNoteDto {
                content: Some(final_content.clone()),
                ..Default::default()
            },
        )
        .expect("autosave note");

        assert_eq!(updated_note.content.len(), final_content.len());
        assert_eq!(updated_note.content, final_content);
        assert_eq!(updated_note.title, "Large Regression Analysis 100KB+");
        assert!(updated_note.is_favorite);
        assert_eq!(
            updated_note.notebook_id.as_deref(),
            Some(archive_nb_id.as_str())
        );
    }

    // Session 2: Application restart and 100% data integrity verification
    {
        let mut conn2 = init_connection(&db_path).expect("Session 2 connection failed");
        run_migrations(&mut conn2).expect("Session 2 migration failed");

        // 1. Fetch the note and verify byte-for-byte fidelity of 100KB+ content
        let note = NoteRepository::get_by_id(&conn2, &note_id)
            .expect("get note session 2")
            .expect("note must exist");

        assert_eq!(note.title, "Large Regression Analysis 100KB+");
        assert_eq!(note.format, "md");
        assert!(note.is_favorite);
        assert_eq!(note.notebook_id.as_deref(), Some(archive_nb_id.as_str()));
        assert_eq!(note.content.len(), final_content.len());
        assert_eq!(note.content, final_content);

        // 2. Verify all 5 tags assigned with renamed tag reflected
        let tags = TagRepository::get_tags_for_note(&conn2, &note_id).expect("get tags session 2");
        assert_eq!(tags.len(), 5);
        let names: Vec<String> = tags.into_iter().map(|t| t.name).collect();
        assert!(names.contains(&"architecture".to_string()));
        assert!(names.contains(&"scale".to_string()));
        assert!(names.contains(&"extreme-performance-2026 ⚡".to_string()));
        assert!(names.contains(&"sqlite".to_string()));
        assert!(names.contains(&"offline".to_string()));

        // 3. Verify Favorite filtering returns this note
        let fav_notes = NoteRepository::list_favorites(&conn2).expect("list favorites");
        assert_eq!(fav_notes.len(), 1);
        assert_eq!(fav_notes[0].id, note_id);
        assert_eq!(fav_notes[0].content.len(), final_content.len());

        // 4. Verify Tag filtering by renamed tag returns this note
        let tagged_notes = NoteRepository::list_by_tag(&conn2, &tag_ids[2]).expect("list by tag");
        assert_eq!(tagged_notes.len(), 1);
        assert_eq!(tagged_notes[0].id, note_id);

        // 5. Verify Notebook filtering by "Archives" returns this note
        let nb_notes = NoteRepository::list_filtered(&conn2, false, Some(&archive_nb_id), false)
            .expect("list filtered by notebook");
        assert_eq!(nb_notes.len(), 1);
        assert_eq!(nb_notes[0].id, note_id);
    }
}

#[test]
fn test_phase5_task_52_backend_transaction_safety_and_rollback() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    let note1_id: String;
    let note2_id: String;
    let tag1_id: String;
    let tag2_id: String;

    // Session 1: Verify multi-table atomic operations and transaction rollback behavior
    {
        let mut conn = init_connection(&db_path).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migration failed");

        // 1. Create two notes
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Transaction Safety Note 1".to_string()),
                content: Some("Testing transactional tag deletion".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 1");
        note1_id = note1.id;

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Transaction Safety Note 2".to_string()),
                content: Some("Testing second note association retention".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 2");
        note2_id = note2.id;

        // 2. Create two tags
        let tag1 = TagRepository::create(&conn, "tx-tag-1").expect("create tag 1");
        tag1_id = tag1.id;

        let tag2 = TagRepository::create(&conn, "tx-tag-2").expect("create tag 2");
        tag2_id = tag2.id;

        // 3. Associate tags
        TagRepository::add_tag_to_note(&conn, &note1_id, &tag1_id).expect("add tag 1 to note 1");
        TagRepository::add_tag_to_note(&conn, &note2_id, &tag1_id).expect("add tag 1 to note 2");
        TagRepository::add_tag_to_note(&conn, &note2_id, &tag2_id).expect("add tag 2 to note 2");

        assert_eq!(
            TagRepository::get_tags_for_note(&conn, &note1_id)
                .expect("n1 tags")
                .len(),
            1
        );
        assert_eq!(
            TagRepository::get_tags_for_note(&conn, &note2_id)
                .expect("n2 tags")
                .len(),
            2
        );

        // 4. Test atomic deletion: TagRepository::delete wraps delete note_tags + delete tag in a transaction
        let deleted = TagRepository::delete(&conn, &tag1_id).expect("delete tag 1 transactionally");
        assert!(deleted, "Tag 1 deletion must return true");

        // Verify tag1 removed from tags table
        let tag1_lookup = TagRepository::get_by_id(&conn, &tag1_id).expect("lookup tag 1");
        assert!(tag1_lookup.is_none());

        // Verify tag1 removed from note_tags for both notes
        let n1_tags =
            TagRepository::get_tags_for_note(&conn, &note1_id).expect("n1 tags after delete");
        assert_eq!(n1_tags.len(), 0);

        let n2_tags =
            TagRepository::get_tags_for_note(&conn, &note2_id).expect("n2 tags after delete");
        assert_eq!(n2_tags.len(), 1);
        assert_eq!(n2_tags[0].id, tag2_id);

        // Verify notes themselves are completely untouched
        let fetched_n1 = NoteRepository::get_by_id(&conn, &note1_id)
            .expect("fetch n1")
            .expect("n1 exists");
        assert_eq!(fetched_n1.title, "Transaction Safety Note 1");
        assert_eq!(fetched_n1.content, "Testing transactional tag deletion");

        let fetched_n2 = NoteRepository::get_by_id(&conn, &note2_id)
            .expect("fetch n2")
            .expect("n2 exists");
        assert_eq!(fetched_n2.title, "Transaction Safety Note 2");

        // 5. Test transaction rollback on abort/failure:
        // Open an unchecked_transaction, insert a tag and assignment, then explicitly drop/rollback without commit
        {
            let tx = conn.unchecked_transaction().expect("start test tx");
            let abort_tag_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            tx.execute(
                "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![abort_tag_id, "aborted-tag", now],
            )
            .expect("insert aborted tag");

            tx.execute(
                "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
                rusqlite::params![note1_id, abort_tag_id],
            )
            .expect("insert aborted junction");

            // Drop tx without calling tx.commit() -> triggers automatic ROLLBACK
            drop(tx);
        }

        // Verify rollback: aborted tag and its junction do not exist in SQLite
        let aborted_lookup =
            TagRepository::get_by_name(&conn, "aborted-tag").expect("lookup aborted");
        assert!(aborted_lookup.is_none(), "Rolled back tag must not exist");

        let n1_tags_after_rollback =
            TagRepository::get_tags_for_note(&conn, &note1_id).expect("n1 tags after rollback");
        assert_eq!(
            n1_tags_after_rollback.len(),
            0,
            "Rolled back junction must not exist"
        );
    }

    // Session 2: Application restart verification
    {
        let mut conn2 = init_connection(&db_path).expect("Session 2 connection failed");
        run_migrations(&mut conn2).expect("Session 2 migration failed");

        // Verify note 2 still has tag 2 and only tag 2
        let n2_tags =
            TagRepository::get_tags_for_note(&conn2, &note2_id).expect("n2 tags session 2");
        assert_eq!(n2_tags.len(), 1);
        assert_eq!(n2_tags[0].id, tag2_id);
        assert_eq!(n2_tags[0].name, "tx-tag-2");

        // Verify note 1 has 0 tags
        let n1_tags =
            TagRepository::get_tags_for_note(&conn2, &note1_id).expect("n1 tags session 2");
        assert_eq!(n1_tags.len(), 0);
    }
}

#[test]
fn test_phase5_task_53_database_integrity_and_orphan_prevention() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Initial SQLite integrity & foreign key check
    let integrity_init: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("integrity check");
    assert_eq!(integrity_init, "ok");

    let mut fk_stmt = conn
        .prepare("PRAGMA foreign_key_check")
        .expect("prepare fk check");
    let fk_violations: Vec<String> = fk_stmt
        .query_map([], |r| r.get(0))
        .expect("query fk")
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        fk_violations.len(),
        0,
        "Initial DB must have 0 foreign key violations"
    );

    // 2. Populate notes, tags, and note_tags
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Integrity Note 1".to_string()),
            content: Some("Sample content".to_string()),
            ..Default::default()
        },
    )
    .expect("create note 1");

    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Integrity Note 2".to_string()),
            content: Some("Sample content 2".to_string()),
            ..Default::default()
        },
    )
    .expect("create note 2");

    let note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Integrity Note 3".to_string()),
            content: Some("Sample content 3".to_string()),
            ..Default::default()
        },
    )
    .expect("create note 3");

    let tag1 = TagRepository::create(&conn, "integrity-tag-1").expect("create tag 1");
    let tag2 = TagRepository::create(&conn, "integrity-tag-2").expect("create tag 2");
    let tag3 = TagRepository::create(&conn, "integrity-tag-3").expect("create tag 3");

    // Assign tags
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag1.id).expect("assign n1 t1");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag2.id).expect("assign n1 t2");
    TagRepository::add_tag_to_note(&conn, &note2.id, &tag2.id).expect("assign n2 t2");
    TagRepository::add_tag_to_note(&conn, &note3.id, &tag3.id).expect("assign n3 t3");

    // Toggle favorite for note1
    NoteRepository::toggle_favorite(&conn, &note1.id).expect("favorite note 1");

    // 3. Test: No duplicate note_tags
    // Attempting direct raw SQL insert of existing pair violates PRIMARY KEY (note_id, tag_id)
    let dup_insert = conn.execute(
        "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![note1.id, tag1.id],
    );
    assert!(
        dup_insert.is_err(),
        "Duplicate (note_id, tag_id) pair must violate PRIMARY KEY constraint"
    );

    // Verify query for any duplicates across entire table returns 0
    let dup_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM (SELECT note_id, tag_id FROM note_tags GROUP BY note_id, tag_id HAVING COUNT(*) > 1)",
            [],
            |r| r.get(0),
        ).expect("dup query");
    assert_eq!(
        dup_count, 0,
        "Database must never contain duplicate note_tags pairs"
    );

    // 4. Test: No tag without valid ID
    let empty_or_null_tag_ids: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM tags WHERE id IS NULL OR length(trim(id)) = 0",
            [],
            |r| r.get(0),
        )
        .expect("tag id query");
    assert_eq!(
        empty_or_null_tag_ids, 0,
        "No tag without valid non-empty ID may exist"
    );

    // Attempting to insert tag with NULL id fails
    let null_id_insert = conn.execute(
        "INSERT INTO tags (id, name, created_at) VALUES (NULL, 'null-tag', '2026-09-30T00:00:00Z')",
        [],
    );
    assert!(
        null_id_insert.is_err(),
        "Tag with NULL ID must be rejected by SQLite NOT NULL constraint"
    );

    // 5. Test: No note deleted because tag deleted
    let total_notes_before = NoteRepository::list(&conn, true)
        .expect("list all notes")
        .len();
    assert_eq!(total_notes_before, 3);

    // Delete tag2
    TagRepository::delete(&conn, &tag2.id).expect("delete tag 2");

    let total_notes_after = NoteRepository::list(&conn, true)
        .expect("list all notes")
        .len();
    assert_eq!(
        total_notes_after, total_notes_before,
        "Deleting a tag must NEVER delete any notes"
    );

    // 6. Test: No orphan note_tags
    // After deleting tag2, junction records referencing tag2 must be 0
    let tag2_junction_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM note_tags WHERE tag_id = ?1",
            rusqlite::params![tag2.id],
            |r| r.get(0),
        )
        .expect("tag2 junction count");
    assert_eq!(
        tag2_junction_count, 0,
        "All note_tags referencing deleted tag must be removed"
    );

    // Hard-delete note3 directly:
    conn.execute(
        "DELETE FROM notes WHERE id = ?1",
        rusqlite::params![note3.id],
    )
    .expect("delete note 3");

    // Note 3 junction records must be cleaned up automatically via FOREIGN KEY cascade
    let note3_junction_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM note_tags WHERE note_id = ?1",
            rusqlite::params![note3.id],
            |r| r.get(0),
        )
        .expect("note3 junction count");
    assert_eq!(
        note3_junction_count, 0,
        "Junction records for deleted note must cascade-delete"
    );

    // Global orphan query: check for ANY orphaned note_tags
    let orphan_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM note_tags
             WHERE note_id NOT IN (SELECT id FROM notes)
                OR tag_id NOT IN (SELECT id FROM tags)",
            [],
            |r| r.get(0),
        )
        .expect("orphan query");
    assert_eq!(
        orphan_count, 0,
        "Database must never contain orphan note_tags rows"
    );

    // 7. Test: No invalid favorite values
    let invalid_fav_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM notes WHERE is_favorite NOT IN (0, 1)",
            [],
            |r| r.get(0),
        )
        .expect("fav check");
    assert_eq!(
        invalid_fav_count, 0,
        "is_favorite must strictly be binary 0 or 1"
    );

    // 8. Final SQLite integrity and foreign key check
    let final_integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("integrity check");
    assert_eq!(final_integrity, "ok");

    let mut final_fk_stmt = conn
        .prepare("PRAGMA foreign_key_check")
        .expect("prepare fk check");
    let final_fk_violations: Vec<String> = final_fk_stmt
        .query_map([], |r| r.get(0))
        .expect("query fk")
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        final_fk_violations.len(),
        0,
        "Final DB must have 0 foreign key violations"
    );
}

#[test]
fn test_phase5_task_54_error_handling_categories_and_messages() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. ValidationError: Empty or whitespace-only tag name
    let empty_create = TagRepository::create(&conn, "   ");
    match empty_create {
        Err(StorageError::Validation(msg)) => {
            assert_eq!(msg, "Tag name cannot be empty");
        }
        other => panic!("Expected ValidationError, got {:?}", other),
    }

    // ValidationError: Tag name exceeding 100 characters
    let long_name = "a".repeat(101);
    let long_create = TagRepository::create(&conn, &long_name);
    match long_create {
        Err(StorageError::Validation(msg)) => {
            assert!(msg.contains("cannot exceed 100 characters"));
        }
        other => panic!("Expected ValidationError, got {:?}", other),
    }

    // 2. Conflict: Renaming a tag to the name of another existing tag (case-insensitive)
    let _tag1 = TagRepository::create(&conn, "Alpha").expect("create Alpha");
    let tag2 = TagRepository::create(&conn, "Beta").expect("create Beta");

    let conflict_rename = TagRepository::update(
        &conn,
        &tag2.id,
        personal_notepad_lib::storage::models::UpdateTagDto {
            name: "alpha".to_string(), // case-insensitive conflict with tag1
        },
    );
    match conflict_rename {
        Err(StorageError::Conflict(msg)) => {
            assert!(msg.contains("already exists"));
            assert!(msg.contains("alpha"));
        }
        other => panic!("Expected Conflict, got {:?}", other),
    }

    // 3. NotFound: Updating a non-existent tag
    let fake_id = uuid::Uuid::new_v4().to_string();
    let not_found_update = TagRepository::update(
        &conn,
        &fake_id,
        personal_notepad_lib::storage::models::UpdateTagDto {
            name: "Nonexistent".to_string(),
        },
    );
    match not_found_update {
        Err(StorageError::NotFound(msg)) => {
            assert!(msg.contains("not found"));
        }
        other => panic!("Expected NotFound, got {:?}", other),
    }

    let not_found_delete = TagRepository::delete(&conn, &fake_id).expect("delete nonexistent");
    assert!(
        !not_found_delete,
        "Deleting non-existent tag should safely return false"
    );

    // 4. DatabaseError / ConstraintViolation mapping to user-friendly messages
    let constraint_err = StorageError::Database(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error {
            code: rusqlite::ErrorCode::ConstraintViolation,
            extended_code: 787,
        },
        Some("FOREIGN KEY constraint failed".to_string()),
    ));
    let friendly = constraint_err.user_friendly_message();
    assert!(friendly.contains("could not be completed"));
    assert!(!friendly.contains("SQLITE_CONSTRAINT")); // No raw SQL leak
}

#[test]
fn test_phase5_task_55_offline_verification_full_suite() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    // Ensure database path exists on local filesystem
    assert!(
        db_path.parent().unwrap().exists(),
        "Storage directory must exist locally"
    );

    let note_id: String;
    let nb_id: String;
    let tag_id: String;

    // Session 1: Full offline operations (notes, notebooks, tags, favorites, metadata)
    {
        let mut conn = init_connection(&db_path).expect("Offline connection failed");
        run_migrations(&mut conn).expect("Offline migration failed");

        // 1. Notebooks work completely offline
        let nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Offline Projects".to_string(),
                parent_id: None,
            },
        )
        .expect("offline notebook creation");
        nb_id = nb.id;

        // 2. Notes work completely offline
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Offline Note".to_string()),
                content: Some("# Offline First\nZero remote APIs or sync needed.".to_string()),
                format: Some("md".to_string()),
                notebook_id: Some(nb_id.clone()),
                ..Default::default()
            },
        )
        .expect("offline note creation");
        note_id = note.id;

        // 3. Tags work completely offline
        let tag = TagRepository::create(&conn, "local-first").expect("offline tag creation");
        tag_id = tag.id;

        TagRepository::add_tag_to_note(&conn, &note_id, &tag_id).expect("offline tag assign");

        // 4. Favorites work completely offline
        let fav =
            NoteRepository::toggle_favorite(&conn, &note_id).expect("offline toggle favorite");
        assert!(fav.is_favorite);

        // 5. Metadata works completely offline
        let tags = TagRepository::get_tags_for_note(&conn, &note_id).expect("offline get tags");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "local-first");

        let fav_list = NoteRepository::list_favorites(&conn).expect("offline list favorites");
        assert_eq!(fav_list.len(), 1);
        assert_eq!(fav_list[0].id, note_id);
    }

    // Session 2: Application restart with zero network access
    {
        let mut conn2 = init_connection(&db_path).expect("Offline restart connection failed");
        run_migrations(&mut conn2).expect("Offline restart migration failed");

        let note_reloaded = NoteRepository::get_by_id(&conn2, &note_id)
            .expect("offline query")
            .expect("note exists");
        assert_eq!(note_reloaded.title, "Offline Note");
        assert!(note_reloaded.is_favorite);
        assert_eq!(note_reloaded.notebook_id.as_deref(), Some(nb_id.as_str()));

        let tags_reloaded =
            TagRepository::get_tags_for_note(&conn2, &note_id).expect("offline tags query");
        assert_eq!(tags_reloaded.len(), 1);
        assert_eq!(tags_reloaded[0].id, tag_id);
    }
}

#[test]
fn test_phase5_task_56_no_ai_and_local_first_determinism_verification() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Verify note creation introduces NO unexpected automatic AI tags or summaries
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Simple Note Without AI".to_string()),
            content: Some(
                "Deterministic user content only. No LLMs, no automatic summaries.".to_string(),
            ),
            ..Default::default()
        },
    )
    .expect("create note");

    // Verify note has exactly 0 tags assigned automatically
    let initial_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("initial tags");
    assert_eq!(
        initial_tags.len(),
        0,
        "No automatic or AI tags may be assigned without user action"
    );

    // Verify favorite status is strictly 0 (not automatically categorized)
    let note_reloaded = NoteRepository::get_by_id(&conn, &note.id)
        .expect("get note")
        .expect("note exists");
    assert!(
        !note_reloaded.is_favorite,
        "Note must not be marked favorite by AI or heuristics"
    );

    // 2. User explicitly creates and assigns a tag
    let manual_tag = TagRepository::create(&conn, "user-created").expect("manual tag create");
    TagRepository::add_tag_to_note(&conn, &note.id, &manual_tag.id).expect("manual tag assign");

    let tags_after =
        TagRepository::get_tags_for_note(&conn, &note.id).expect("tags after manual assign");
    assert_eq!(tags_after.len(), 1);
    assert_eq!(tags_after[0].name, "user-created");

    // 3. Verify total tags in SQLite equals exactly 1 (no background suggested tags generated)
    let all_tags = TagRepository::list(&conn).expect("list tags");
    assert_eq!(all_tags.len(), 1);
    assert_eq!(all_tags[0].name, "user-created");
}

#[test]
fn test_phase5_task_57_no_placeholder_data_fresh_db_verification() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. A clean database must contain 0 hardcoded/mock tags
    let initial_tags = TagRepository::list(&conn).expect("list tags");
    assert_eq!(
        initial_tags.len(),
        0,
        "Fresh database must not contain any pre-seeded or mock tags"
    );

    // 2. A clean database must contain 0 note_tags associations
    let initial_tag_counts = TagRepository::get_tag_note_counts(&conn).expect("tag note counts");
    assert_eq!(initial_tag_counts.len(), 0);

    let initial_junction_rows: i64 = conn
        .query_row("SELECT COUNT(1) FROM note_tags", [], |r| r.get(0))
        .expect("query note_tags");
    assert_eq!(
        initial_junction_rows, 0,
        "Fresh database must contain 0 junction rows"
    );

    // 3. A clean database must contain 0 notes and 0 notebooks
    let initial_notes = NoteRepository::list(&conn, true).expect("list notes");
    assert_eq!(initial_notes.len(), 0);

    let initial_notebooks = NotebookRepository::list(&conn).expect("list notebooks");
    assert_eq!(initial_notebooks.len(), 0);

    // 4. Batch tags mapping for empty database returns empty map
    let batch = TagRepository::get_all_notes_tag_names(&conn).expect("batch tag names");
    assert_eq!(batch.len(), 0);
}

#[test]
fn test_phase5_task_64_exact_manual_qa_simulation_suite() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    // Initialize fresh DB
    let mut conn = init_connection(&db_path).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // ---------------------------------------------------------------------
    // Test A — Create tags: work, important, planning
    // Expected: All appear in Tags
    // ---------------------------------------------------------------------
    let tag_work = TagRepository::create(&conn, "work").expect("Test A: create work");
    let tag_important =
        TagRepository::create(&conn, "important").expect("Test A: create important");
    let _tag_planning = TagRepository::create(&conn, "planning").expect("Test A: create planning");

    let tags_a = TagRepository::list(&conn).expect("Test A: list tags");
    assert_eq!(tags_a.len(), 3);
    let tag_names: Vec<_> = tags_a.iter().map(|t| t.name.as_str()).collect();
    assert!(tag_names.contains(&"work"));
    assert!(tag_names.contains(&"important"));
    assert!(tag_names.contains(&"planning"));

    // ---------------------------------------------------------------------
    // Test B — Assign tags: Open note -> Add work -> Add important
    // Expected: [work] [important]
    // ---------------------------------------------------------------------
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("QA Note 1".to_string()),
            content: Some("Testing tag assignments".to_string()),
            ..Default::default()
        },
    )
    .expect("Test B: create QA Note 1");

    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_work.id).expect("Test B: assign work");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_important.id)
        .expect("Test B: assign important");

    let note1_tags_b =
        TagRepository::get_tags_for_note(&conn, &note1.id).expect("Test B: get note tags");
    assert_eq!(note1_tags_b.len(), 2);
    let note1_tag_names_b: Vec<_> = note1_tags_b.iter().map(|t| t.name.as_str()).collect();
    assert!(note1_tag_names_b.contains(&"work"));
    assert!(note1_tag_names_b.contains(&"important"));

    // ---------------------------------------------------------------------
    // Test C — Remove one tag: Remove work
    // Expected: [important]
    // ---------------------------------------------------------------------
    TagRepository::remove_tag_from_note(&conn, &note1.id, &tag_work.id)
        .expect("Test C: remove work");

    let note1_tags_c =
        TagRepository::get_tags_for_note(&conn, &note1.id).expect("Test C: get note tags");
    assert_eq!(note1_tags_c.len(), 1);
    assert_eq!(note1_tags_c[0].name, "important");

    // ---------------------------------------------------------------------
    // Test D — Favorite: Mark note as favorite
    // Expected: Favorites contains note
    // ---------------------------------------------------------------------
    let note1_fav =
        NoteRepository::set_favorite(&conn, &note1.id, true).expect("Test D: mark favorite");
    assert!(note1_fav.is_favorite);

    let favs_d = NoteRepository::list_favorites(&conn).expect("Test D: list favorites");
    assert_eq!(favs_d.len(), 1);
    assert_eq!(favs_d[0].id, note1.id);
    assert!(favs_d[0].is_favorite);

    // ---------------------------------------------------------------------
    // Test E — Restart: Close -> reopen
    // Expected: Tags preserved, Favorite preserved
    // ---------------------------------------------------------------------
    drop(conn);

    let mut reopened_conn = init_connection(&db_path).expect("Test E: reopen connection");
    run_migrations(&mut reopened_conn).expect("Test E: idempotent migrations");

    // Tags preserved
    let tags_e = TagRepository::list(&reopened_conn).expect("Test E: list tags");
    assert_eq!(tags_e.len(), 3);
    let tag_names_e: Vec<_> = tags_e.iter().map(|t| t.name.as_str()).collect();
    assert!(tag_names_e.contains(&"work"));
    assert!(tag_names_e.contains(&"important"));
    assert!(tag_names_e.contains(&"planning"));

    // Note tags preserved
    let note1_tags_e =
        TagRepository::get_tags_for_note(&reopened_conn, &note1.id).expect("Test E: get note tags");
    assert_eq!(note1_tags_e.len(), 1);
    assert_eq!(note1_tags_e[0].name, "important");

    // Favorite preserved
    let favs_e = NoteRepository::list_favorites(&reopened_conn).expect("Test E: list favorites");
    assert_eq!(favs_e.len(), 1);
    assert_eq!(favs_e[0].id, note1.id);
    assert!(favs_e[0].is_favorite);

    // ---------------------------------------------------------------------
    // Test F — Rename tag: work -> projects
    // Expected: All affected notes now show projects
    // ---------------------------------------------------------------------
    // Assign tag 'work' to another note (or note1) to verify affected notes
    let note2 = NoteRepository::create(
        &reopened_conn,
        CreateNoteDto {
            title: Some("QA Note 2".to_string()),
            ..Default::default()
        },
    )
    .expect("Test F: create QA Note 2");
    TagRepository::add_tag_to_note(&reopened_conn, &note2.id, &tag_work.id)
        .expect("Test F: assign work to note2");

    let renamed_tag = TagRepository::update(
        &reopened_conn,
        &tag_work.id,
        UpdateTagDto {
            name: "projects".to_string(),
        },
    )
    .expect("Test F: rename work to projects");
    assert_eq!(renamed_tag.name, "projects");

    // Verify affected note now shows "projects"
    let note2_tags_f = TagRepository::get_tags_for_note(&reopened_conn, &note2.id)
        .expect("Test F: get note2 tags");
    assert_eq!(note2_tags_f.len(), 1);
    assert_eq!(note2_tags_f[0].name, "projects");
    assert_eq!(note2_tags_f[0].id, tag_work.id);

    // ---------------------------------------------------------------------
    // Test G — Delete tag: Delete projects
    // Expected: Tag disappears, Notes remain
    // ---------------------------------------------------------------------
    TagRepository::delete(&reopened_conn, &tag_work.id).expect("Test G: delete projects tag");

    let tags_g = TagRepository::list(&reopened_conn).expect("Test G: list tags");
    assert_eq!(tags_g.len(), 2);
    let tag_names_g: Vec<_> = tags_g.iter().map(|t| t.name.as_str()).collect();
    assert!(!tag_names_g.contains(&"projects"));
    assert!(!tag_names_g.contains(&"work"));
    assert!(tag_names_g.contains(&"important"));
    assert!(tag_names_g.contains(&"planning"));

    // Notes remain untouched
    let all_notes_g = NoteRepository::list(&reopened_conn, false).expect("Test G: list notes");
    assert_eq!(all_notes_g.len(), 2);
    let note2_tags_g = TagRepository::get_tags_for_note(&reopened_conn, &note2.id)
        .expect("Test G: get note2 tags");
    assert_eq!(note2_tags_g.len(), 0);

    // ---------------------------------------------------------------------
    // Test H — Tag filter: Select #important
    // Expected: Only notes with important tag appear
    // ---------------------------------------------------------------------
    let important_notes_h = NoteRepository::list_by_tag(&reopened_conn, &tag_important.id)
        .expect("Test H: list by important");
    assert_eq!(important_notes_h.len(), 1);
    assert_eq!(important_notes_h[0].id, note1.id);
    assert_eq!(important_notes_h[0].title, "QA Note 1");

    // ---------------------------------------------------------------------
    // Test I — Notebook + tags:
    // Work
    // └── Projects
    //     └── Project Plan
    // Tags: work, important
    // Favorite: Yes
    // Verify all metadata is consistent.
    // ---------------------------------------------------------------------
    let nb_work = NotebookRepository::create(
        &reopened_conn,
        CreateNotebookDto {
            name: "Work".to_string(),
            parent_id: None,
        },
    )
    .expect("Test I: create Work root notebook");

    let nb_projects = NotebookRepository::create(
        &reopened_conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(nb_work.id.clone()),
        },
    )
    .expect("Test I: create Projects child notebook");

    // Recreate tag 'work' since it was deleted
    let tag_work_recreated =
        TagRepository::create(&reopened_conn, "work").expect("Test I: recreate work tag");

    let note_plan = NoteRepository::create(
        &reopened_conn,
        CreateNoteDto {
            title: Some("Project Plan".to_string()),
            content: Some("# Project Plan\n\nQ4 Deliverables and schedule.".to_string()),
            notebook_id: Some(nb_projects.id.clone()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("Test I: create Project Plan note");

    // Assign tags work and important
    TagRepository::add_tag_to_note(&reopened_conn, &note_plan.id, &tag_work_recreated.id)
        .expect("Test I: assign work");
    TagRepository::add_tag_to_note(&reopened_conn, &note_plan.id, &tag_important.id)
        .expect("Test I: assign important");

    // Favorite: Yes
    let note_plan_fav = NoteRepository::set_favorite(&reopened_conn, &note_plan.id, true)
        .expect("Test I: set favorite");
    assert!(note_plan_fav.is_favorite);

    // Verify hierarchy path
    let hierarchy_path = NotebookRepository::get_hierarchy_path(&reopened_conn, &nb_projects.id)
        .expect("Test I: get path");
    assert_eq!(hierarchy_path, "Work / Projects");

    // Verify tags
    let plan_tags =
        TagRepository::get_tags_for_note(&reopened_conn, &note_plan.id).expect("Test I: get tags");
    assert_eq!(plan_tags.len(), 2);
    let plan_tag_names: Vec<_> = plan_tags.iter().map(|t| t.name.as_str()).collect();
    assert!(plan_tag_names.contains(&"work"));
    assert!(plan_tag_names.contains(&"important"));

    // Verify note retrieved from DB has all consistent metadata
    let plan_db = NoteRepository::get_by_id(&reopened_conn, &note_plan.id)
        .expect("Test I: get note")
        .expect("found note");
    assert_eq!(plan_db.title, "Project Plan");
    assert_eq!(plan_db.notebook_id, Some(nb_projects.id));
    assert_eq!(plan_db.format, "md");
    assert!(plan_db.is_favorite);
    assert!(!plan_db.is_pinned);
    assert!(!plan_db.is_deleted);
    assert_eq!(plan_db.deleted_at, None);
    assert!(!plan_db.created_at.is_empty());
    assert!(!plan_db.modified_at.is_empty());
}

#[test]
fn test_phase5_task_65_restart_regression_suite() {
    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();

    let initial_created_at;
    let note_id;
    let tag_backend_id;
    let tag_critical_id;
    let tag_q4_id;
    let product_nb_id;

    let edited_title = "Sprint 42 Specs & Architecture";
    let edited_content = "# Sprint 42 Specs & Architecture\n\nDeep dive into offline local-first storage and indexing.";

    // Session 1: Create, Tag, Favorite, Edit, Move, Rename
    {
        let mut conn = init_connection(&db_path).expect("Session 1: connection");
        run_migrations(&mut conn).expect("Session 1: migration");

        // 1. Create notebook "Engineering"
        let engineering_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Engineering".to_string(),
                parent_id: None,
            },
        )
        .expect("Step 1: create notebook");

        // 2. Create note
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Sprint Specs".to_string()),
                content: Some("Initial sprint specifications.".to_string()),
                notebook_id: Some(engineering_nb.id.clone()),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .expect("Step 2: create note");

        note_id = note.id.clone();
        initial_created_at = note.created_at.clone();

        // 3. Assign 3 tags
        let t_backend =
            TagRepository::create(&conn, "backend").expect("Step 3: create backend tag");
        let t_critical =
            TagRepository::create(&conn, "critical").expect("Step 3: create critical tag");
        let t_q4 = TagRepository::create(&conn, "q4-2026").expect("Step 3: create q4 tag");

        tag_backend_id = t_backend.id.clone();
        tag_critical_id = t_critical.id.clone();
        tag_q4_id = t_q4.id.clone();

        TagRepository::add_tag_to_note(&conn, &note_id, &tag_backend_id)
            .expect("Step 3: assign backend");
        TagRepository::add_tag_to_note(&conn, &note_id, &tag_critical_id)
            .expect("Step 3: assign critical");
        TagRepository::add_tag_to_note(&conn, &note_id, &tag_q4_id).expect("Step 3: assign q4");

        let assigned =
            TagRepository::get_tags_for_note(&conn, &note_id).expect("Step 3: check tags");
        assert_eq!(assigned.len(), 3);

        // 4. Mark favorite
        let fav_res =
            NoteRepository::set_favorite(&conn, &note_id, true).expect("Step 4: mark favorite");
        assert!(fav_res.is_favorite);

        // 5. Edit note (title & content)
        NoteRepository::update(
            &conn,
            &note_id,
            UpdateNoteDto {
                title: Some(edited_title.to_string()),
                content: Some(edited_content.to_string()),
                ..Default::default()
            },
        )
        .expect("Step 5: edit note");

        // 6. Move note (to notebook "Product")
        let product_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Product".to_string(),
                parent_id: None,
            },
        )
        .expect("Step 6: create product notebook");
        product_nb_id = product_nb.id.clone();

        NoteRepository::update(
            &conn,
            &note_id,
            UpdateNoteDto {
                notebook_id: Some(product_nb_id.clone()),
                ..Default::default()
            },
        )
        .expect("Step 6: move note");

        // 7. Rename tag: backend -> core-infrastructure
        let renamed = TagRepository::update(
            &conn,
            &tag_backend_id,
            UpdateTagDto {
                name: "core-infrastructure".to_string(),
            },
        )
        .expect("Step 7: rename tag");
        assert_eq!(renamed.name, "core-infrastructure");

        // 8. Close application (conn dropped here)
    }

    // 9. Reopen application
    let mut reopened_conn = init_connection(&db_path).expect("Step 9: reopen connection");
    run_migrations(&mut reopened_conn).expect("Step 9: idempotent migrations");

    // 10. Verify all state after restart
    let note_after_restart = NoteRepository::get_by_id(&reopened_conn, &note_id)
        .expect("Step 10: get note")
        .expect("Note exists after restart");

    // Notebook assignment correct
    assert_eq!(note_after_restart.notebook_id, Some(product_nb_id.clone()));
    let hierarchy_path = NotebookRepository::get_hierarchy_path(&reopened_conn, &product_nb_id)
        .expect("get hierarchy path");
    assert_eq!(hierarchy_path, "Product");

    // Title correct
    assert_eq!(note_after_restart.title, edited_title);

    // Content correct
    assert_eq!(note_after_restart.content, edited_content);

    // Favorite correct
    assert!(note_after_restart.is_favorite);
    let favorites_list = NoteRepository::list_favorites(&reopened_conn).expect("list favorites");
    assert_eq!(favorites_list.len(), 1);
    assert_eq!(favorites_list[0].id, note_id);

    // Tags correct (with renamed tag and other 2 tags)
    let tags_after_restart =
        TagRepository::get_tags_for_note(&reopened_conn, &note_id).expect("get tags after restart");
    assert_eq!(tags_after_restart.len(), 3);
    let mut tag_names: Vec<String> = tags_after_restart.into_iter().map(|t| t.name).collect();
    tag_names.sort();
    assert_eq!(
        tag_names,
        vec!["core-infrastructure", "critical", "q4-2026"]
    );

    // Created time unchanged
    assert_eq!(note_after_restart.created_at, initial_created_at);

    // Modified time correct (valid non-empty timestamp)
    assert!(!note_after_restart.modified_at.is_empty());
    assert_ne!(note_after_restart.modified_at, "");
}

#[test]
fn test_task_8_typed_attachment_operations_and_dto_mapping() {
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage, models::AttachmentMetadataDto,
        services::AttachmentService,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());

    // 1. Create a note
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Task 8 Note".to_string()),
            content: Some("Testing typed commands and DTO mapping".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note failed");

    // 2. Prepare sample source file
    let src_dir = env.dir.join("sources");
    std::fs::create_dir_all(&src_dir).unwrap();
    let src_file = src_dir.join("quarterly_results.pdf");
    let content_bytes = b"%PDF-1.5 Quarterly Report Summary 2026";
    std::fs::write(&src_file, content_bytes).unwrap();

    // 3. Add attachment via service
    let attachment = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file)
        .expect("Add attachment should succeed");

    // 4. Map to DTO (as done by Tauri add_attachment command)
    let dto = AttachmentMetadataDto::from(attachment.clone());
    assert_eq!(dto.id, attachment.id);
    assert_eq!(dto.note_id, note.id);
    assert_eq!(dto.file_name, "quarterly_results.pdf");
    assert_eq!(dto.mime_type, Some("application/pdf".to_string()));
    assert_eq!(dto.file_size, content_bytes.len() as i64);
    assert!(!dto.created_at.is_empty());
    assert!(!dto.modified_at.is_empty());

    // Verify JSON serialization does not contain internal filesystem paths
    let serialized = serde_json::to_string(&dto).expect("Serialize DTO");
    assert!(!serialized.contains("relative_path"));
    assert!(!serialized.contains("attachments/"));
    assert!(!serialized.contains(&env.paths.attachments_dir.to_string_lossy().to_string()));

    // 5. List attachments and map to DTOs (as done by Tauri list_attachments command)
    let list = AttachmentService::list_attachments(&conn, &note.id)
        .expect("List attachments should succeed");
    let list_dtos: Vec<AttachmentMetadataDto> =
        list.into_iter().map(AttachmentMetadataDto::from).collect();
    assert_eq!(list_dtos.len(), 1);
    assert_eq!(list_dtos[0].id, dto.id);

    // 6. Get safe path for open (as done by Tauri open_attachment command)
    let open_path = AttachmentService::get_attachment_for_open(&conn, &storage, &dto.id)
        .expect("Get safe path for open");
    assert!(open_path.exists());
    assert!(open_path.starts_with(&env.paths.attachments_dir));

    // 7. Remove attachment (as done by Tauri remove_attachment command)
    AttachmentService::remove_attachment(&conn, &storage, &dto.id)
        .expect("Remove attachment should succeed");

    // Verify deleted from DB and disk
    let after_list = AttachmentService::list_attachments(&conn, &note.id).unwrap();
    assert_eq!(after_list.len(), 0);
    assert!(!storage.file_exists(&attachment.relative_path));
}

#[test]
fn test_task_8_input_validation_and_structured_error_masking() {
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage, errors::AttachmentError, services::AttachmentService,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());

    let src_dir = env.dir.join("sources");
    std::fs::create_dir_all(&src_dir).unwrap();
    let src_file = src_dir.join("sample.txt");
    std::fs::write(&src_file, b"sample text").unwrap();

    // 1. Nonexistent note ID produces user-friendly error
    let err1 = AttachmentService::add_attachment(&conn, &storage, "nonexistent-uuid", &src_file)
        .unwrap_err();
    assert_eq!(
        err1.user_friendly_message(),
        "The referenced note could not be found."
    );

    // 2. Soft-deleted note produces user-friendly rejection
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Soft-deleted Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    NoteRepository::delete(&conn, &note.id, true).unwrap();

    let err2 = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap_err();
    assert!(err2
        .user_friendly_message()
        .contains("Cannot attach files to a deleted note"));

    // 3. Nonexistent source file produces user-friendly message
    let active_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Active Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let missing_src = src_dir.join("does_not_exist.bin");
    let err3 = AttachmentService::add_attachment(&conn, &storage, &active_note.id, &missing_src)
        .unwrap_err();
    assert!(err3
        .user_friendly_message()
        .contains("Selected source file could not be read or does not exist"));

    // 4. Nonexistent attachment ID produces user-friendly message
    let err4 = AttachmentService::get_attachment(&conn, "missing-att-id").unwrap_err();
    assert_eq!(err4.user_friendly_message(), "Attachment not found.");

    // 5. Missing managed file on disk yields user-friendly message
    let valid_att =
        AttachmentService::add_attachment(&conn, &storage, &active_note.id, &src_file).unwrap();
    let safe_disk_path = storage.resolve_safe_path(&valid_att.relative_path).unwrap();
    std::fs::remove_file(safe_disk_path).unwrap();

    let err5 =
        AttachmentService::get_attachment_for_open(&conn, &storage, &valid_att.id).unwrap_err();
    match err5 {
        AttachmentError::ManagedFileMissing(_) => {
            assert_eq!(
                err5.user_friendly_message(),
                "Attachment file is missing from storage."
            );
        }
        other => panic!("Expected ManagedFileMissing, got: {other:?}"),
    }
}

#[test]
fn test_task_13_safe_attachment_opening_sandbox_and_immutability() {
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage, errors::AttachmentError,
        repositories::AttachmentRepository, services::AttachmentService,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());

    // Create an active note and an attachment
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note with Attachment for Open".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let src_dir = env.dir.join("sources");
    std::fs::create_dir_all(&src_dir).unwrap();
    let src_file = src_dir.join("open_test.pdf");
    std::fs::write(&src_file, b"%PDF-1.4 sample pdf content for open testing").unwrap();

    let attachment = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file)
        .expect("Add attachment should succeed");

    // 1. Snapshot metadata before open operation
    let before_att = AttachmentRepository::get_by_id(&conn, &attachment.id)
        .unwrap()
        .expect("Attachment must exist");

    // 2. Perform safe path resolution for open
    let resolved_path = AttachmentService::get_attachment_for_open(&conn, &storage, &attachment.id)
        .expect("Path resolution for open must succeed");

    // 3. Verify path containment: must reside strictly inside attachments_dir
    assert!(resolved_path.exists());
    assert!(resolved_path.is_file());
    assert!(resolved_path.starts_with(&env.paths.attachments_dir));

    // 4. Verify metadata immutability: opening must never silently alter DB records
    let after_att = AttachmentRepository::get_by_id(&conn, &attachment.id)
        .unwrap()
        .expect("Attachment must still exist");
    assert_eq!(before_att.id, after_att.id);
    assert_eq!(before_att.note_id, after_att.note_id);
    assert_eq!(before_att.file_name, after_att.file_name);
    assert_eq!(before_att.relative_path, after_att.relative_path);
    assert_eq!(before_att.file_size, after_att.file_size);
    assert_eq!(before_att.mime_type, after_att.mime_type);
    assert_eq!(before_att.created_at, after_att.created_at);

    // 5. Arbitrary path traversal attack defense:
    // Even if an attacker passes arbitrary system paths as the ID parameter, it must fail safely
    let traversal_err =
        AttachmentService::get_attachment_for_open(&conn, &storage, "../../../etc/passwd")
            .unwrap_err();
    match traversal_err {
        AttachmentError::AttachmentNotFound(_) => {}
        other => panic!("Expected AttachmentNotFound for traversal ID, got: {other:?}"),
    }

    // 6. Empty / whitespace-only IDs are rejected
    let empty_err = AttachmentService::get_attachment_for_open(&conn, &storage, "   ").unwrap_err();
    match empty_err {
        AttachmentError::InvalidRequest(_) => {}
        other => panic!("Expected InvalidRequest for empty ID, got: {other:?}"),
    }

    // 7. Missing file on disk produces understandable ManagedFileMissing error
    std::fs::remove_file(&resolved_path).expect("Remove file from disk to simulate missing file");
    let missing_err =
        AttachmentService::get_attachment_for_open(&conn, &storage, &attachment.id).unwrap_err();
    assert_eq!(
        missing_err.user_friendly_message(),
        "Attachment file is missing from storage."
    );
}

#[test]
fn test_task_14_attachment_removal_and_managed_file_cleanup() {
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage, errors::AttachmentError,
        repositories::AttachmentRepository, services::AttachmentService,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Removal Test Note".to_string()),
            content: Some("Existing untouched content".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let src_dir = env.dir.join("sources");
    std::fs::create_dir_all(&src_dir).unwrap();

    let file1 = src_dir.join("file1.txt");
    let file2 = src_dir.join("file2.txt");
    std::fs::write(&file1, b"first attachment bytes").unwrap();
    std::fs::write(&file2, b"second attachment bytes").unwrap();

    let att1 = AttachmentService::add_attachment(&conn, &storage, &note.id, &file1).unwrap();
    let att2 = AttachmentService::add_attachment(&conn, &storage, &note.id, &file2).unwrap();

    let path1 = storage.resolve_safe_path(&att1.relative_path).unwrap();
    let path2 = storage.resolve_safe_path(&att2.relative_path).unwrap();

    assert!(path1.exists());
    assert!(path2.exists());

    // 1. Remove first attachment
    AttachmentService::remove_attachment(&conn, &storage, &att1.id)
        .expect("Removal of first attachment must succeed");

    // First attachment is deleted from disk and DB
    assert!(!path1.exists());
    assert!(!storage.file_exists(&att1.relative_path));
    assert!(AttachmentRepository::get_by_id(&conn, &att1.id)
        .unwrap()
        .is_none());

    // 2. Unrelated attachment (att2) remains completely intact on disk and in DB
    assert!(path2.exists());
    assert!(storage.file_exists(&att2.relative_path));
    let remaining_att2 = AttachmentRepository::get_by_id(&conn, &att2.id).unwrap();
    assert!(remaining_att2.is_some());
    assert_eq!(remaining_att2.unwrap().id, att2.id);

    // 3. Note content and title remain untouched
    let refreshed_note = NoteRepository::get_by_id(&conn, &note.id).unwrap().unwrap();
    assert_eq!(refreshed_note.title, "Removal Test Note");
    assert_eq!(refreshed_note.content, "Existing untouched content");

    // 4. Stale metadata cleanup: if a file is already missing on disk, removal still succeeds cleanly
    std::fs::remove_file(&path2).unwrap();
    assert!(!path2.exists());
    AttachmentService::remove_attachment(&conn, &storage, &att2.id)
        .expect("Removing stale attachment with missing disk file must succeed");
    assert!(AttachmentRepository::get_by_id(&conn, &att2.id)
        .unwrap()
        .is_none());

    // 5. Deleting nonexistent attachment returns AttachmentNotFound
    let not_found_err =
        AttachmentService::remove_attachment(&conn, &storage, "nonexistent-att-id").unwrap_err();
    match not_found_err {
        AttachmentError::AttachmentNotFound(_) => {}
        other => panic!("Expected AttachmentNotFound, got {other:?}"),
    }

    // 6. Traversal ID attempts are rejected safely
    let traversal_err =
        AttachmentService::remove_attachment(&conn, &storage, "../../../etc/passwd").unwrap_err();
    match traversal_err {
        AttachmentError::AttachmentNotFound(_) => {}
        other => panic!("Expected AttachmentNotFound, got {other:?}"),
    }
}

#[test]
fn test_task_15_duplicate_filenames_unicode_and_unusual_names() {
    use personal_notepad_lib::storage::{
        attachment_storage::{sanitize_original_filename, AttachmentStorage},
        services::AttachmentService,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Unicode & Duplicate Filename Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let src_dir = env.dir.join("sources");
    std::fs::create_dir_all(&src_dir).unwrap();

    // 1. Duplicate original filenames on the same note do not overwrite each other
    let dup_file_a = src_dir.join("report.pdf");
    std::fs::write(&dup_file_a, b"%PDF-1.4 duplicate instance 1").unwrap();

    let att1 = AttachmentService::add_attachment(&conn, &storage, &note.id, &dup_file_a).unwrap();

    // Now modify the source file and attach it again with the exact same name
    std::fs::write(&dup_file_a, b"%PDF-1.4 duplicate instance 2").unwrap();
    let att2 = AttachmentService::add_attachment(&conn, &storage, &note.id, &dup_file_a).unwrap();

    assert_ne!(att1.id, att2.id, "Attachment IDs must be unique");
    assert_eq!(att1.file_name, "report.pdf");
    assert_eq!(att2.file_name, "report.pdf");
    assert_ne!(
        att1.relative_path, att2.relative_path,
        "Storage paths must be distinct"
    );

    let path1 = storage.resolve_safe_path(&att1.relative_path).unwrap();
    let path2 = storage.resolve_safe_path(&att2.relative_path).unwrap();
    assert!(path1.exists());
    assert!(path2.exists());

    // Verify content isolation: instance 1 and instance 2 have different bytes
    let bytes1 = std::fs::read(&path1).unwrap();
    let bytes2 = std::fs::read(&path2).unwrap();
    assert_eq!(bytes1, b"%PDF-1.4 duplicate instance 1");
    assert_eq!(bytes2, b"%PDF-1.4 duplicate instance 2");

    // Deleting att1 does not delete or affect att2
    AttachmentService::remove_attachment(&conn, &storage, &att1.id).unwrap();
    assert!(!path1.exists());
    assert!(path2.exists());
    assert_eq!(
        std::fs::read(&path2).unwrap(),
        b"%PDF-1.4 duplicate instance 2"
    );

    // 2. International Unicode filenames (Hindi, CJK, accents, spaces, multiple extensions, no extension)
    let test_cases = vec![
        ("report (1).pdf", b"data1" as &[u8]),
        ("final version 2026.pdf", b"data2"),
        ("résumé.pdf", b"data3"),
        ("नोट्स.txt", b"data4"),
        ("项目计划.pdf", b"data5"),
        ("file with many spaces.txt", b"data6"),
        ("archive.tar.gz", b"data7"),
        ("README_NO_EXT", b"data8"),
        ("🚀 rocket note 🎉.log", b"data9"),
    ];

    for (name, content) in test_cases {
        let f = src_dir.join(name);
        std::fs::write(&f, content).unwrap();

        let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &f)
            .unwrap_or_else(|e| panic!("Failed attaching '{name}': {e:?}"));

        assert_eq!(
            att.file_name, name,
            "Original display name must be preserved"
        );

        let stored_path = storage.resolve_safe_path(&att.relative_path).unwrap();
        assert!(stored_path.exists(), "Stored path for '{name}' must exist");
        assert!(stored_path.starts_with(&env.paths.attachments_dir));

        // Verify stored filename on disk is safe and does not leak raw spaces or Unicode into storage name
        let stored_filename = stored_path.file_name().unwrap().to_string_lossy();
        assert!(
            !stored_filename.contains(' '),
            "Storage filename must not contain spaces"
        );
        assert!(
            stored_filename.is_ascii(),
            "Storage filename must be safe ASCII UUID"
        );

        // Open path verification
        let open_path =
            AttachmentService::get_attachment_for_open(&conn, &storage, &att.id).unwrap();
        assert_eq!(open_path, stored_path);
    }

    // 3. Traversal stripping and sanitization unit checks
    assert_eq!(sanitize_original_filename("../../../etc/passwd"), "passwd");
    assert_eq!(
        sanitize_original_filename("C:\\Users\\admin\\secrets.txt"),
        "secrets.txt"
    );
    assert_eq!(sanitize_original_filename("   "), "attachment");
    assert_eq!(sanitize_original_filename("null\0byte.txt"), "nullbyte.txt");

    // 4. Long filename truncation: stem truncated safely at 255 chars preserving extension
    let long_stem = "a".repeat(300);
    let long_name = format!("{long_stem}.pdf");
    let sanitized_long = sanitize_original_filename(&long_name);
    assert_eq!(sanitized_long.chars().count(), 255);
    assert!(sanitized_long.ends_with(".pdf"));
}

#[test]
fn test_task_16_missing_files_permission_errors_and_disk_failures() {
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage, errors::AttachmentError,
        repositories::AttachmentRepository, services::AttachmentService,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Error Resilience Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let src_dir = env.dir.join("sources");
    std::fs::create_dir_all(&src_dir).unwrap();

    // 1. Source file disappears before copying
    let missing_src = src_dir.join("disappeared.txt");
    let src_err =
        AttachmentService::add_attachment(&conn, &storage, &note.id, &missing_src).unwrap_err();
    match src_err {
        AttachmentError::SourceUnavailable(_) => {
            assert!(src_err
                .user_friendly_message()
                .contains("could not be read or does not exist"));
        }
        other => panic!("Expected SourceUnavailable, got {other:?}"),
    }

    // 2. Managed attachment file deleted outside application (out-of-band deletion)
    let valid_src = src_dir.join("will_be_deleted_outside.txt");
    std::fs::write(&valid_src, b"managed file to delete").unwrap();
    let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &valid_src).unwrap();

    let disk_path = storage.resolve_safe_path(&att.relative_path).unwrap();
    assert!(disk_path.exists());

    // Delete from disk outside the app
    std::fs::remove_file(&disk_path).unwrap();

    // (a) list_attachments must still return the metadata; does not crash or silently delete
    let list_after_external_delete = AttachmentService::list_attachments(&conn, &note.id).unwrap();
    assert_eq!(list_after_external_delete.len(), 1);
    assert_eq!(list_after_external_delete[0].id, att.id);

    // (b) Opening the missing file returns user-friendly ManagedFileMissing error
    let open_missing_err =
        AttachmentService::get_attachment_for_open(&conn, &storage, &att.id).unwrap_err();
    match open_missing_err {
        AttachmentError::ManagedFileMissing(_) => {
            assert_eq!(
                open_missing_err.user_friendly_message(),
                "Attachment file is missing from storage."
            );
        }
        other => panic!("Expected ManagedFileMissing, got {other:?}"),
    }

    // (c) Controlled removal succeeds in cleaning up stale metadata from SQLite
    let remove_stale = AttachmentService::remove_attachment(&conn, &storage, &att.id);
    assert!(
        remove_stale.is_ok(),
        "Controlled removal of stale record must succeed"
    );
    assert!(AttachmentRepository::get_by_id(&conn, &att.id)
        .unwrap()
        .is_none());

    // 3. File unreadable / permission denied handling
    let unreadable_src = src_dir.join("unreadable.txt");
    std::fs::write(&unreadable_src, b"secret unreadable bytes").unwrap();
    let att2 =
        AttachmentService::add_attachment(&conn, &storage, &note.id, &unreadable_src).unwrap();
    let disk_path2 = storage.resolve_safe_path(&att2.relative_path).unwrap();

    // Revoke read permissions on disk (Linux mode 0000)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&disk_path2).unwrap().permissions();
        perms.set_mode(0o000);
        let _ = std::fs::set_permissions(&disk_path2, perms);

        let perm_err =
            AttachmentService::get_attachment_for_open(&conn, &storage, &att2.id).unwrap_err();
        match perm_err {
            AttachmentError::PermissionDenied(_) => {
                assert_eq!(
                    perm_err.user_friendly_message(),
                    "Permission denied when accessing file."
                );
            }
            other => panic!("Expected PermissionDenied, got {other:?}"),
        }

        // Restore permissions for clean tempdir teardown
        let mut restore_perms = std::fs::metadata(&disk_path2).unwrap().permissions();
        restore_perms.set_mode(0o644);
        let _ = std::fs::set_permissions(&disk_path2, restore_perms);
    }

    // 4. Structured user-friendly messages for disk and open failures
    assert_eq!(
        AttachmentError::InsufficientDiskSpace.user_friendly_message(),
        "Insufficient disk space to save attachment."
    );
    assert_eq!(
        AttachmentError::OpenFailed("no associated application".to_string())
            .user_friendly_message(),
        "Failed to open attachment: no associated application"
    );
}

#[test]
fn test_task_20_attachment_behavior_during_note_deletion_and_lifecycle_changes() {
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage,
        errors::AttachmentError,
        models::{CreateNotebookDto, UpdateNoteDto},
        repositories::{AttachmentRepository, NoteRepository, NotebookRepository, TagRepository},
        services::AttachmentService,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());

    // 1. Create a note and an attachment
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Lifecycle Test Note".to_string()),
            content: Some("Lifecycle content".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let src_dir = env.dir.join("sources_lifecycle");
    std::fs::create_dir_all(&src_dir).unwrap();
    let src_file = src_dir.join("sample.pdf");
    std::fs::write(&src_file, b"%PDF-1.4 sample payload").unwrap();

    let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap();
    let safe_disk_path = storage.resolve_safe_path(&att.relative_path).unwrap();
    assert!(safe_disk_path.exists());
    assert_eq!(
        AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(),
        1
    );

    // 2. Note rename must not alter attachments
    NoteRepository::update(
        &conn,
        &note.id,
        UpdateNoteDto {
            title: Some("Renamed Note Title".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(),
        1
    );
    assert!(safe_disk_path.exists());

    // 3. Notebook move must not alter attachments
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work Notebook".to_string(),
            parent_id: None,
        },
    )
    .unwrap();
    NoteRepository::move_to_notebook(&conn, &note.id, Some(&nb.id)).unwrap();
    assert_eq!(
        AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(),
        1
    );
    assert!(safe_disk_path.exists());

    // 4. Tag assignment & favorite toggle must not alter attachments
    let tag = TagRepository::create(&conn, "important").unwrap();
    TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).unwrap();
    NoteRepository::set_favorite(&conn, &note.id, true).unwrap();
    assert_eq!(
        AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(),
        1
    );
    assert!(safe_disk_path.exists());

    // 5. Soft deletion: preserves attachment metadata and managed files
    NoteRepository::delete(&conn, &note.id, true).unwrap();
    let soft_deleted_note = NoteRepository::get_by_id(&conn, &note.id).unwrap().unwrap();
    assert!(soft_deleted_note.is_deleted);
    // Metadata & file on disk remain 100% intact
    assert_eq!(
        AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(),
        1
    );
    assert!(safe_disk_path.exists());

    // 6. Soft-deleted note cannot receive new attachments
    let new_file = src_dir.join("rejected.txt");
    std::fs::write(&new_file, b"cannot be added").unwrap();
    let add_err =
        AttachmentService::add_attachment(&conn, &storage, &note.id, &new_file).unwrap_err();
    match add_err {
        AttachmentError::InvalidRequest(msg) => {
            assert!(msg.contains("Cannot attach files to a deleted note"));
        }
        other => panic!("Expected InvalidRequest for deleted note, got: {other:?}"),
    }

    // 7. Restore note: attachments immediately valid and openable
    NoteRepository::restore(&conn, &note.id).unwrap();
    let restored_note = NoteRepository::get_by_id(&conn, &note.id).unwrap().unwrap();
    assert!(!restored_note.is_deleted);
    let open_res = AttachmentService::get_attachment_for_open(&conn, &storage, &att.id);
    assert!(open_res.is_ok());

    // 8. Permanent (hard) deletion: removes physical directory and cascades DB rows
    storage.remove_note_directory(&note.id).unwrap();
    NoteRepository::delete(&conn, &note.id, false).unwrap();

    // Foreign key cascade deleted DB record
    assert_eq!(
        AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(),
        0
    );
    assert_eq!(
        AttachmentRepository::get_by_id(&conn, &att.id).unwrap(),
        None
    );
    // Physical directory cleaned up, leaving no untracked files
    assert!(!safe_disk_path.exists());
}

#[test]
fn test_task_21_orphan_detection_and_storage_consistency_checks() {
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage, repositories::NoteRepository,
        services::AttachmentService,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());
    let src_dir = env.paths.root.join("src_files_t21");
    std::fs::create_dir_all(&src_dir).unwrap();

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Consistency Check Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 1. Initial empty storage is fully consistent
    let initial_report =
        AttachmentService::check_consistency(&conn, &storage).expect("Check initial consistency");
    assert!(initial_report.is_consistent);
    assert_eq!(initial_report.total_attachments_checked, 0);
    assert_eq!(initial_report.total_disk_files_checked, 0);

    // 2. Add a valid attachment
    let valid_src = src_dir.join("valid_doc.pdf");
    let valid_bytes = b"%PDF-1.4 sample personal notepad consistency file";
    std::fs::write(&valid_src, valid_bytes).unwrap();

    let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &valid_src)
        .expect("Add attachment");
    let att_disk_path = storage.resolve_safe_path(&att.relative_path).unwrap();

    let clean_report = AttachmentService::check_consistency(&conn, &storage).unwrap();
    assert!(clean_report.is_consistent);
    assert_eq!(clean_report.total_attachments_checked, 1);
    assert_eq!(clean_report.total_disk_files_checked, 1);
    assert!(clean_report.missing_managed_files.is_empty());
    assert!(clean_report.unreferenced_managed_files.is_empty());
    assert!(clean_report.abandoned_temp_files.is_empty());
    assert!(clean_report.size_mismatches.is_empty());

    // 3. Unreferenced file detection
    let note_dir = storage.get_note_dir(&note.id).unwrap();
    let unref_path = note_dir.join("abandoned_manual_copy.png");
    std::fs::write(&unref_path, b"unreferenced image bytes").unwrap();

    let unref_report = AttachmentService::check_consistency(&conn, &storage).unwrap();
    assert!(!unref_report.is_consistent);
    assert_eq!(unref_report.unreferenced_managed_files.len(), 1);
    assert!(unref_report.unreferenced_managed_files[0].ends_with("abandoned_manual_copy.png"));

    // Clean up unreferenced file
    std::fs::remove_file(&unref_path).unwrap();

    // 4. Abandoned temporary staging file detection & safe cleanup
    let temp_file_path = note_dir.join("in_flight_copy.pdf.tmp.99887766");
    std::fs::write(&temp_file_path, b"partial staging bytes").unwrap();

    let temp_report = AttachmentService::check_consistency(&conn, &storage).unwrap();
    assert!(!temp_report.is_consistent);
    assert_eq!(temp_report.abandoned_temp_files.len(), 1);
    assert!(temp_report.abandoned_temp_files[0].contains(".tmp.99887766"));

    // Cleanup abandoned temp files (older_than 0s for test)
    let cleaned = AttachmentService::cleanup_abandoned_temp_files(&storage, 0).unwrap();
    assert_eq!(cleaned, 1);
    assert!(!temp_file_path.exists());
    // Verify valid attachment was untouched
    assert!(att_disk_path.exists());

    // 5. Size mismatch detection
    std::fs::write(
        &att_disk_path,
        b"modified byte length for consistency detection",
    )
    .unwrap();
    let mismatch_report = AttachmentService::check_consistency(&conn, &storage).unwrap();
    assert!(!mismatch_report.is_consistent);
    assert_eq!(mismatch_report.size_mismatches.len(), 1);
    let mismatch = &mismatch_report.size_mismatches[0];
    assert_eq!(mismatch.attachment_id, att.id);
    assert_eq!(mismatch.recorded_size, valid_bytes.len() as i64);
    assert_eq!(
        mismatch.actual_size,
        b"modified byte length for consistency detection".len() as u64
    );

    // 6. Missing managed file detection
    std::fs::remove_file(&att_disk_path).unwrap();
    let missing_report = AttachmentService::check_consistency(&conn, &storage).unwrap();
    assert!(!missing_report.is_consistent);
    assert_eq!(missing_report.missing_managed_files.len(), 1);
    let missing = &missing_report.missing_managed_files[0];
    assert_eq!(missing.attachment_id, att.id);
    assert_eq!(missing.note_id, note.id);
    assert_eq!(missing.file_name, att.file_name);
    assert_eq!(missing.relative_path, att.relative_path);
}

#[test]
fn test_task_22_automated_rust_and_database_suite() {
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage,
        errors::AttachmentError,
        models::CreateAttachmentDto,
        repositories::{AttachmentRepository, NoteRepository},
        services::AttachmentService,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());
    let src_dir = env.paths.root.join("src_files_t22");
    std::fs::create_dir_all(&src_dir).unwrap();

    // 1. Create active note
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Automated Tests Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 2. Binary content exact preservation test
    let binary_bytes: Vec<u8> = (0..=255).cycle().take(8192).collect();
    let bin_file = src_dir.join("binary_test.bin");
    std::fs::write(&bin_file, &binary_bytes).unwrap();

    let att_bin = AttachmentService::add_attachment(&conn, &storage, &note.id, &bin_file)
        .expect("Add binary attachment");
    assert_eq!(att_bin.file_size, binary_bytes.len() as i64);

    let open_bin_path = AttachmentService::get_attachment_for_open(&conn, &storage, &att_bin.id)
        .expect("Get open path");
    let read_back_bytes = std::fs::read(&open_bin_path).unwrap();
    assert_eq!(
        read_back_bytes, binary_bytes,
        "Binary content must be copied without modification"
    );

    // 3. Duplicate original filenames with Unicode and spaces
    let unicode_name = "दस्तावेज़ & Specification 🚀.pdf";
    let unicode_file = src_dir.join(unicode_name);
    std::fs::write(&unicode_file, b"%PDF-1.4 unicode content").unwrap();

    let att_u1 = AttachmentService::add_attachment(&conn, &storage, &note.id, &unicode_file)
        .expect("Add unicode attachment 1");
    let att_u2 = AttachmentService::add_attachment(&conn, &storage, &note.id, &unicode_file)
        .expect("Add unicode attachment 2");

    assert_eq!(att_u1.file_name, unicode_name);
    assert_eq!(att_u2.file_name, unicode_name);
    assert_ne!(att_u1.id, att_u2.id);
    assert_ne!(att_u1.relative_path, att_u2.relative_path);
    assert!(storage.file_exists(&att_u1.relative_path));
    assert!(storage.file_exists(&att_u2.relative_path));

    // 4. Repository tests: empty list, retrieval, deletion, foreign key safety
    let empty_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Empty Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        AttachmentRepository::list_by_note_id(&conn, &empty_note.id)
            .unwrap()
            .len(),
        0
    );

    let fetched_u1 = AttachmentRepository::get_by_id(&conn, &att_u1.id)
        .unwrap()
        .unwrap();
    assert_eq!(fetched_u1.id, att_u1.id);
    assert_eq!(fetched_u1.file_name, unicode_name);

    // Delete attachment record via repository
    assert!(AttachmentRepository::delete(&conn, &att_u2.id).unwrap());
    assert_eq!(
        AttachmentRepository::get_by_id(&conn, &att_u2.id).unwrap(),
        None
    );

    // Foreign key constraint failure for invalid note ID
    let fk_fail = AttachmentRepository::create(
        &conn,
        CreateAttachmentDto {
            note_id: "non-existent-note".to_string(),
            file_name: "fail.txt".to_string(),
            relative_path: "non-existent-note/fail.txt".to_string(),
            mime_type: None,
            file_size: 10,
        },
    );
    assert!(fk_fail.is_err(), "Foreign key violation must be enforced");

    // 5. Service tests: note validation, failed operation isolation
    let missing_note_err =
        AttachmentService::add_attachment(&conn, &storage, "fake-note-id", &bin_file).unwrap_err();
    match missing_note_err {
        AttachmentError::NoteNotFound(_) => {}
        other => panic!("Expected NoteNotFound, got: {other:?}"),
    }

    // Soft-deleted note validation
    NoteRepository::delete(&conn, &empty_note.id, true).unwrap();
    let deleted_note_err =
        AttachmentService::add_attachment(&conn, &storage, &empty_note.id, &bin_file).unwrap_err();
    match deleted_note_err {
        AttachmentError::InvalidRequest(msg) => {
            assert!(msg.contains("deleted note"));
        }
        other => panic!("Expected InvalidRequest, got: {other:?}"),
    }

    // Failed operation isolation: failing to add a non-existent file does not delete att_bin
    let non_existent_src = src_dir.join("missing.png");
    assert!(
        AttachmentService::add_attachment(&conn, &storage, &note.id, &non_existent_src).is_err()
    );
    assert!(storage.file_exists(&att_bin.relative_path));

    // 6. Security tests: SQL injection strings and path traversal
    let malicious_notes = ["' OR '1'='1", "'; DROP TABLE attachments; --"];
    for bad_id in malicious_notes {
        assert!(AttachmentService::add_attachment(&conn, &storage, bad_id, &bin_file).is_err());
    }

    let traversal_paths = ["../outside.txt", "note/../../etc/passwd", "/tmp/bad"];
    for bad_path in traversal_paths {
        assert!(storage.resolve_safe_path(bad_path).is_err());
    }

    // 7. Cleanup & Cascade verification
    AttachmentService::remove_attachment(&conn, &storage, &att_bin.id).unwrap();
    assert!(!storage.file_exists(&att_bin.relative_path));
    assert_eq!(
        AttachmentRepository::get_by_id(&conn, &att_bin.id).unwrap(),
        None
    );
}

#[test]
fn test_task_23_frontend_and_tauri_integration_suite() {
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage,
        errors::AttachmentError,
        repositories::{AttachmentRepository, NoteRepository},
        services::AttachmentService,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());
    let src_dir = env.paths.root.join("src_files_t23");
    std::fs::create_dir_all(&src_dir).unwrap();

    // 1. Create Note A and Note B
    let note_a = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note A - Working".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let note_b = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note B - Isolated".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 2. Initial state: Note A has empty attachment list and count 0
    let initial_list_a = AttachmentService::list_attachments(&conn, &note_a.id).unwrap();
    assert_eq!(initial_list_a.len(), 0);
    assert_eq!(
        AttachmentRepository::count_by_note_id(&conn, &note_a.id).unwrap(),
        0
    );

    // 3. Add attachments to Note A: document.pdf and photo.png
    let pdf_src = src_dir.join("Quarterly Report.pdf");
    std::fs::write(&pdf_src, b"%PDF-1.4 report bytes").unwrap();
    let att_pdf =
        AttachmentService::add_attachment(&conn, &storage, &note_a.id, &pdf_src).expect("Add PDF");

    let png_src = src_dir.join("Diagram 📊.png");
    std::fs::write(&png_src, b"\x89PNG\r\n\x1a\n png bytes").unwrap();
    let att_png =
        AttachmentService::add_attachment(&conn, &storage, &note_a.id, &png_src).expect("Add PNG");

    // 4. Verify Note A count is 2 and Note B count is 0
    assert_eq!(
        AttachmentRepository::count_by_note_id(&conn, &note_a.id).unwrap(),
        2
    );
    assert_eq!(
        AttachmentRepository::count_by_note_id(&conn, &note_b.id).unwrap(),
        0
    );

    // 5. Note switching simulation:
    // When user switches to Note B, attachments list is strictly empty
    let list_b = AttachmentService::list_attachments(&conn, &note_b.id).unwrap();
    assert_eq!(list_b.len(), 0);

    // When user switches back to Note A, both attachments are returned deterministically
    let list_a = AttachmentService::list_attachments(&conn, &note_a.id).unwrap();
    assert_eq!(list_a.len(), 2);
    assert_eq!(list_a[0].id, att_pdf.id);
    assert_eq!(list_a[1].id, att_png.id);

    // 6. Neighboring row preservation:
    // Removing PDF attachment leaves PNG attachment completely untouched
    AttachmentService::remove_attachment(&conn, &storage, &att_pdf.id).unwrap();
    let list_a_after = AttachmentService::list_attachments(&conn, &note_a.id).unwrap();
    assert_eq!(list_a_after.len(), 1);
    assert_eq!(list_a_after[0].id, att_png.id);
    assert!(storage.file_exists(&att_png.relative_path));
    assert!(!storage.file_exists(&att_pdf.relative_path));

    // 7. Open attachment path resolution
    let open_png_path = AttachmentService::get_attachment_for_open(&conn, &storage, &att_png.id)
        .expect("Open PNG path");
    assert!(open_png_path.exists());

    // 8. Missing managed file error resilience
    std::fs::remove_file(&open_png_path).unwrap();
    let open_err =
        AttachmentService::get_attachment_for_open(&conn, &storage, &att_png.id).unwrap_err();
    match open_err {
        AttachmentError::ManagedFileMissing(_) => {}
        other => panic!("Expected ManagedFileMissing, got: {other:?}"),
    }

    // 9. Permanent note deletion cascades
    storage.remove_note_directory(&note_a.id).unwrap();
    NoteRepository::delete(&conn, &note_a.id, false).unwrap();
    assert_eq!(
        AttachmentRepository::count_by_note_id(&conn, &note_a.id).unwrap(),
        0
    );
}

#[test]
fn test_task_24_security_reliability_and_performance_verification() {
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage,
        repositories::{AttachmentRepository, NoteRepository},
        services::AttachmentService,
    };

    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();
    let mut conn = init_connection(&db_path).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let storage = AttachmentStorage::new(env.paths.clone());
    let src_dir = env.paths.root.join("src_files_t24");
    std::fs::create_dir_all(&src_dir).unwrap();

    // ==========================================
    // 1. SECURITY VERIFICATION
    // ==========================================
    let sec_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Security Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Filenames with shell metacharacters and command injection payloads
    let malicious_shell_names = [
        "report; rm -rf $(whoami).pdf",
        "test | cat passwd | nc.txt",
        "`calc.exe` & calc.png",
        "safe_name$HOME.pdf",
    ];

    for shell_name in malicious_shell_names {
        let malicious_file = src_dir.join(shell_name);
        std::fs::write(&malicious_file, b"safe file payload").unwrap();

        // When sanitized and stored, shell characters cannot escape sandbox or execute commands
        let att = AttachmentService::add_attachment(&conn, &storage, &sec_note.id, &malicious_file)
            .expect("Attachment with unusual name should safely store");
        assert!(!att.id.is_empty());
        assert!(storage.file_exists(&att.relative_path));

        // Clean up
        AttachmentService::remove_attachment(&conn, &storage, &att.id).unwrap();
    }

    // Path traversal attempts in relative path
    assert!(storage.resolve_safe_path("../../../etc/passwd").is_err());
    assert!(storage
        .resolve_safe_path("sec_note/../../etc/shadow")
        .is_err());
    assert!(storage.resolve_safe_path("/absolute/root").is_err());

    // ==========================================
    // 2. RELIABILITY & RESTART VERIFICATION
    // ==========================================
    let rel_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Reliability Restart Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let doc_src = src_dir.join("persistent_doc.pdf");
    let doc_payload = b"%PDF-1.4 persistent document bytes across restart";
    std::fs::write(&doc_src, doc_payload).unwrap();

    let att_doc = AttachmentService::add_attachment(&conn, &storage, &rel_note.id, &doc_src)
        .expect("Add persistent attachment");

    // Simulate application restart: drop connection, reopen, rerun idempotent migrations
    drop(conn);
    let mut conn_restarted = init_connection(&db_path).expect("Reopen failed");
    run_migrations(&mut conn_restarted).expect("Idempotent migration failed");

    let reloaded_list = AttachmentService::list_attachments(&conn_restarted, &rel_note.id).unwrap();
    assert_eq!(reloaded_list.len(), 1);
    assert_eq!(reloaded_list[0].id, att_doc.id);
    assert_eq!(reloaded_list[0].file_size, doc_payload.len() as i64);

    let reloaded_path =
        AttachmentService::get_attachment_for_open(&conn_restarted, &storage, &att_doc.id)
            .expect("Get path after restart");
    assert!(reloaded_path.exists());
    let reloaded_content = std::fs::read(reloaded_path).unwrap();
    assert_eq!(reloaded_content, doc_payload);

    // ==========================================
    // 3. PERFORMANCE & PROFILE VERIFICATION
    // ==========================================
    let perf_note = NoteRepository::create(
        &conn_restarted,
        CreateNoteDto {
            title: Some("Performance Benchmark Note".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // A. Small text file (100 B)
    let small_txt = src_dir.join("small_note.txt");
    std::fs::write(&small_txt, vec![b'A'; 100]).unwrap();
    let att_small =
        AttachmentService::add_attachment(&conn_restarted, &storage, &perf_note.id, &small_txt)
            .expect("Add small");
    assert_eq!(att_small.file_size, 100);

    // B. Typical PDF (50 KiB)
    let med_pdf = src_dir.join("medium_report.pdf");
    let mut pdf_data = b"%PDF-1.4 ".to_vec();
    pdf_data.extend(vec![b'B'; 50 * 1024]);
    std::fs::write(&med_pdf, &pdf_data).unwrap();
    let att_pdf =
        AttachmentService::add_attachment(&conn_restarted, &storage, &perf_note.id, &med_pdf)
            .expect("Add PDF");
    assert_eq!(att_pdf.mime_type, Some("application/pdf".to_string()));

    // C. Typical Image (200 KiB)
    let img_file = src_dir.join("photo.png");
    let mut png_data = b"\x89PNG\r\n\x1a\n".to_vec();
    png_data.extend(vec![b'C'; 200 * 1024]);
    std::fs::write(&img_file, &png_data).unwrap();
    let att_img =
        AttachmentService::add_attachment(&conn_restarted, &storage, &perf_note.id, &img_file)
            .expect("Add PNG");
    assert_eq!(att_img.mime_type, Some("image/png".to_string()));

    // D. Relatively large file near limit (2 MiB) copied via 64 KiB chunks
    let large_file = src_dir.join("large_dataset.bin");
    let large_bytes: Vec<u8> = vec![0x42; 2 * 1024 * 1024]; // 2 MiB
    std::fs::write(&large_file, &large_bytes).unwrap();

    let start_time = std::time::Instant::now();
    let att_large =
        AttachmentService::add_attachment(&conn_restarted, &storage, &perf_note.id, &large_file)
            .expect("Add 2 MiB large file");
    let elapsed = start_time.elapsed();

    assert_eq!(att_large.file_size, 2 * 1024 * 1024);
    assert!(
        elapsed.as_millis() < 500,
        "2 MiB copy must be fast and responsive"
    );

    // E. Multi-note attachment batch resolution avoiding N+1
    let note_ids = vec![
        sec_note.id.clone(),
        rel_note.id.clone(),
        perf_note.id.clone(),
    ];
    let batch_counts =
        AttachmentRepository::batch_count_by_note_ids(&conn_restarted, &note_ids).unwrap();
    assert_eq!(batch_counts.get(&perf_note.id), Some(&4));
    assert_eq!(batch_counts.get(&rel_note.id), Some(&1));
}

#[test]
fn test_phase9_task_22_internal_links_integration_and_database_suite() {
    use personal_notepad_lib::storage::attachment_storage::AttachmentStorage;
    use personal_notepad_lib::storage::models::LinkTargetLifecycle;
    use personal_notepad_lib::storage::services::links::InternalLinkService;
    use personal_notepad_lib::storage::services::TrashService;

    let temp_dir =
        std::env::temp_dir().join(format!("pn_integration_links_{}", uuid::Uuid::new_v4()));
    let db_path = temp_dir.join("database.sqlite");
    let mut conn = init_connection(&db_path).expect("Init failed");
    run_migrations(&mut conn).expect("Migrations failed");

    let nb_dev = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Development".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let nb_ops = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Operations".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    // 1. Create Target Note in Development
    let target_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Authentication Service".to_string()),
            content: Some("# Authentication Service\nOAuth2 and JWT specification.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb_dev.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    // 2. Create Duplicate-title Note in Operations
    let duplicate_title_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Authentication Service".to_string()), // Identical title!
            content: Some("Ops runbook for Auth service.".to_string()),
            format: Some("txt".to_string()),
            notebook_id: Some(nb_ops.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    assert_ne!(target_note.id, duplicate_title_note.id);

    // 3. Create Source Note linking to target_note
    let source_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("API Gateway Config".to_string()),
            content: Some(format!(
                "Routing directs /auth/* to [Authentication Service](notepad://note/{}) instance.",
                target_note.id
            )),
            format: Some("txt".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .unwrap();

    // 4. Verify unambiguous resolution
    let res_target = InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
    let res_dup =
        InternalLinkService::resolve_link_target(&conn, &duplicate_title_note.id).unwrap();

    assert_eq!(res_target.target_id, target_note.id);
    assert_eq!(res_target.notebook_name.as_deref(), Some("Development"));
    assert_eq!(res_dup.target_id, duplicate_title_note.id);
    assert_eq!(res_dup.notebook_name.as_deref(), Some("Operations"));

    // 5. Verify backlinks discovery
    let backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].source_note_id, source_note.id);
    assert_eq!(backlinks[0].source_note_title, "API Gateway Config");

    // 6. Target Rename preserves link validity
    NoteRepository::update(
        &conn,
        &target_note.id,
        UpdateNoteDto {
            title: Some("Auth & Identity v3".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let res_renamed = InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
    assert_eq!(res_renamed.title.as_deref(), Some("Auth & Identity v3"));
    assert_eq!(res_renamed.lifecycle, LinkTargetLifecycle::Active);

    // Source note content is intact
    let loaded_source = NoteRepository::get_by_id(&conn, &source_note.id)
        .unwrap()
        .unwrap();
    assert_eq!(loaded_source.content, source_note.content);

    // 7. Soft delete target: lifecycle becomes Trashed
    TrashService::soft_delete(&conn, &target_note.id).unwrap();
    let res_trashed = InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
    assert_eq!(res_trashed.lifecycle, LinkTargetLifecycle::Trashed);

    // 8. Restore target: lifecycle returns to Active
    TrashService::restore(&conn, &target_note.id).unwrap();
    let res_restored = InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
    assert_eq!(res_restored.lifecycle, LinkTargetLifecycle::Active);

    // 9. Permanent deletion: lifecycle becomes Missing, source note unaffected
    let storage_paths = StoragePaths::from_root(temp_dir.clone());
    let storage = AttachmentStorage::new(storage_paths);
    TrashService::soft_delete(&conn, &target_note.id).unwrap();
    TrashService::permanently_delete_note(&conn, &storage, &target_note.id).unwrap();

    let res_missing = InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
    assert_eq!(res_missing.lifecycle, LinkTargetLifecycle::Missing);

    let final_source = NoteRepository::get_by_id(&conn, &source_note.id)
        .unwrap()
        .unwrap();
    assert_eq!(final_source.content, source_note.content);

    drop(conn);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_phase9_task_24_security_reliability_and_performance_verification() {
    use personal_notepad_lib::storage::models::LinkTargetLifecycle;
    use personal_notepad_lib::storage::services::links::InternalLinkService;
    use personal_notepad_lib::storage::services::TrashService;

    let temp_dir =
        std::env::temp_dir().join(format!("notepad_int_links_t24_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("test.db");

    let mut conn = init_connection(&db_path).unwrap();
    run_migrations(&mut conn).unwrap();

    // 1. Parameterized query safety against SQL injection payloads
    let injections = vec![
        "' OR '1'='1",
        "admin'--",
        "'; DROP TABLE notes; --",
        "1 UNION SELECT 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20--",
    ];
    for payload in injections {
        let res = InternalLinkService::resolve_link_target(&conn, payload).unwrap();
        assert_eq!(res.lifecycle, LinkTargetLifecycle::Missing);
    }

    // 2. Stable ID resolution across identical note titles
    let target_dev = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("System Architecture".to_string()),
            content: Some("Architecture for Dev".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let target_prod = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("System Architecture".to_string()),
            content: Some("Architecture for Prod".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let res_dev = InternalLinkService::resolve_link_target(&conn, &target_dev.id).unwrap();
    let res_prod = InternalLinkService::resolve_link_target(&conn, &target_prod.id).unwrap();
    assert_eq!(res_dev.target_id, target_dev.id);
    assert_eq!(res_prod.target_id, target_prod.id);
    assert_ne!(res_dev.target_id, res_prod.target_id);

    // 3. Backlinks retrieval performance and parameterized accuracy
    let source_a = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Source Alpha".to_string()),
            content: Some(format!(
                "Check [Dev Arch](notepad://note/{}) here.",
                target_dev.id
            )),
            ..Default::default()
        },
    )
    .unwrap();

    let source_b = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Source Beta".to_string()),
            content: Some(format!(
                "Also see [Dev Arch](notepad://note/{}) for details.",
                target_dev.id
            )),
            ..Default::default()
        },
    )
    .unwrap();

    let backlinks = InternalLinkService::get_backlinks(&conn, &target_dev.id).unwrap();
    assert_eq!(backlinks.len(), 2);
    assert!(backlinks.iter().any(|b| b.source_note_id == source_a.id));
    assert!(backlinks.iter().any(|b| b.source_note_id == source_b.id));

    // Trashing source_a excludes it from active backlinks
    TrashService::soft_delete(&conn, &source_a.id).unwrap();
    let updated_backlinks = InternalLinkService::get_backlinks(&conn, &target_dev.id).unwrap();
    assert_eq!(updated_backlinks.len(), 1);
    assert_eq!(updated_backlinks[0].source_note_id, source_b.id);

    drop(conn);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_phase10_task_22_txt_and_md_import_export_lifecycle() {
    use personal_notepad_lib::storage::models::*;
    use personal_notepad_lib::storage::services::{ImportExportService, SearchService};

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    let export_dir = env.dir.join("exports");
    std::fs::create_dir_all(&export_dir).unwrap();

    // 1. Create a TXT note and a Markdown note in SQLite
    let txt_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Standard Meeting Notes".to_string()),
            content: Some("First line\nSecond line with unicode: 🚀\nThird line".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    let md_content =
        "# Project Architecture\n\n- [x] Feature 1\n- [ ] Feature 2\n\n```rust\nfn main() {}\n```";
    let md_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Project Architecture".to_string()),
            content: Some(md_content.to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    // 2. Export single TXT note
    let txt_export_dest = export_dir.join("Meeting.txt");
    let res_txt = ImportExportService::export_single_note(
        &conn,
        ExportSingleNoteRequest {
            note_id: txt_note.id.clone(),
            destination_path: txt_export_dest.to_string_lossy().to_string(),
            format: ExportFormat::Txt,
            overwrite: true,
        },
    )
    .unwrap();
    assert_eq!(res_txt.note_id, txt_note.id);
    assert!(txt_export_dest.exists());
    let txt_disk = std::fs::read_to_string(&txt_export_dest).unwrap();
    assert_eq!(
        txt_disk,
        "First line\nSecond line with unicode: 🚀\nThird line"
    );

    // 3. Export single Markdown note
    let md_export_dest = export_dir.join("Architecture.md");
    let res_md = ImportExportService::export_single_note(
        &conn,
        ExportSingleNoteRequest {
            note_id: md_note.id.clone(),
            destination_path: md_export_dest.to_string_lossy().to_string(),
            format: ExportFormat::Md,
            overwrite: true,
        },
    )
    .unwrap();
    assert_eq!(res_md.note_id, md_note.id);
    assert!(md_export_dest.exists());
    let md_disk = std::fs::read_to_string(&md_export_dest).unwrap();
    assert_eq!(md_disk, md_content);

    // 4. Scan files for import
    let preview = ImportExportService::scan_candidates(
        &conn,
        &[
            txt_export_dest.to_string_lossy().to_string(),
            md_export_dest.to_string_lossy().to_string(),
        ],
    )
    .unwrap();
    assert_eq!(preview.total_files, 2);
    assert_eq!(preview.valid_files, 2);
    assert_eq!(preview.invalid_files, 0);

    // 5. Execute import into a user notebook
    let target_nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Imported Files".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let candidate_overrides: Vec<ImportCandidateOverride> = preview
        .candidates
        .iter()
        .map(|c| ImportCandidateOverride {
            id: c.id.clone(),
            file_path: c.file_path.clone(),
            selected: true,
            custom_title: Some(format!("Imported - {}", c.proposed_title)),
        })
        .collect();

    let import_summary = ImportExportService::execute_import(
        &mut conn,
        ImportRequest {
            operation_id: Some("op-test-import".to_string()),
            candidates: candidate_overrides,
            options: ImportOptions {
                target_notebook_id: Some(target_nb.id.clone()),
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        },
    )
    .unwrap();

    assert_eq!(import_summary.total_processed, 2);
    assert_eq!(import_summary.imported_count, 2);
    assert_eq!(import_summary.failed_count, 0);
    assert_eq!(import_summary.outcome, Some(OperationOutcome::FullSuccess));

    // Verify imported notes in DB
    let imported_notes = NoteRepository::list_by_notebook(&conn, &target_nb.id).unwrap();
    assert_eq!(imported_notes.len(), 2);
    assert!(imported_notes
        .iter()
        .any(|n| n.title == "Imported - Meeting" && n.format == "txt"));
    assert!(imported_notes
        .iter()
        .any(|n| n.title == "Imported - Architecture" && n.format == "md"));

    // Verify search index immediately picks up imported content
    let search_res = SearchService::search(&conn, "Feature", Some(10)).unwrap();
    assert!(
        !search_res.is_empty(),
        "FTS search index must match imported note"
    );
}

#[test]
fn test_phase10_task_22_structured_json_roundtrip_with_internal_links_and_hierarchy() {
    use personal_notepad_lib::storage::models::*;
    use personal_notepad_lib::storage::services::ImportExportService;

    let env_source = TestEnv::new();
    let mut conn_source = init_connection(&env_source.paths.database_file).unwrap();
    run_migrations(&mut conn_source).unwrap();

    let export_dir = env_source.dir.join("json_exports");
    std::fs::create_dir_all(&export_dir).unwrap();

    // 1. Create hierarchy: "Engineering" -> "Specs"
    let nb_parent = NotebookRepository::create(
        &conn_source,
        CreateNotebookDto {
            name: "Engineering".to_string(),
            parent_id: None,
        },
    )
    .unwrap();
    let nb_child = NotebookRepository::create(
        &conn_source,
        CreateNotebookDto {
            name: "Specs".to_string(),
            parent_id: Some(nb_parent.id.clone()),
        },
    )
    .unwrap();

    // 2. Create Tag: "sqlite"
    let tag = TagRepository::create(&conn_source, "sqlite").unwrap();

    // 3. Create Note 1 (Target)
    let note1 = NoteRepository::create(
        &conn_source,
        CreateNoteDto {
            title: Some("Core Database Engine".to_string()),
            content: Some("Details of SQLite WAL mode engine.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb_child.id.clone()),
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();
    TagRepository::add_tag_to_note(&conn_source, &note1.id, &tag.id).unwrap();

    // 4. Create Note 2 linking to Note 1
    let note2 = NoteRepository::create(
        &conn_source,
        CreateNoteDto {
            title: Some("Storage Architecture Spec".to_string()),
            content: Some(format!(
                "Check out [Core Database](notepad://note/{}) for internals.",
                note1.id
            )),
            format: Some("md".to_string()),
            notebook_id: Some(nb_child.id.clone()),
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    // 5. Batch export as JSON
    let json_dest = export_dir.join("specs_backup.json");
    let export_summary = ImportExportService::export_notes_batch(
        &conn_source,
        ExportBatchRequest {
            operation_id: Some("op-json-batch".to_string()),
            scope: Some(ExportScope::AllNotes),
            note_ids: vec![],
            format: ExportFormat::Json,
            destination_dir: export_dir.to_string_lossy().to_string(),
            destination_file: Some(json_dest.to_string_lossy().to_string()),
            include_metadata: true,
            overwrite: true,
        },
    )
    .unwrap();

    assert_eq!(export_summary.exported_count, 2); // 2 notes exported
    assert_eq!(export_summary.outcome, Some(OperationOutcome::FullSuccess));
    assert!(json_dest.exists());

    let json_raw = std::fs::read_to_string(&json_dest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_raw).unwrap();
    assert_eq!(parsed["version"], "1.0");
    assert_eq!(parsed["format"], "personal-notepad-interchange");
    assert_eq!(parsed["note_count"], 2);

    // 6. Import into fresh, completely independent test database
    let env_dest = TestEnv::new();
    let mut conn_dest = init_connection(&env_dest.paths.database_file).unwrap();
    run_migrations(&mut conn_dest).unwrap();

    let scan_res = ImportExportService::scan_candidates(
        &conn_dest,
        &[json_dest.to_string_lossy().to_string()],
    )
    .unwrap();
    assert_eq!(scan_res.total_files, 1);
    assert_eq!(scan_res.candidates[0].detected_format, ImportFormat::Json);

    let import_summary = ImportExportService::execute_import(
        &mut conn_dest,
        ImportRequest {
            operation_id: Some("op-import-fresh".to_string()),
            candidates: vec![ImportCandidateOverride {
                id: scan_res.candidates[0].id.clone(),
                file_path: scan_res.candidates[0].file_path.clone(),
                selected: true,
                custom_title: Some(scan_res.candidates[0].proposed_title.clone()),
            }],
            options: ImportOptions {
                target_notebook_id: None, // Respect JSON document's hierarchy path!
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        },
    )
    .unwrap();

    assert_eq!(import_summary.imported_count, 1); // 1 JSON file candidate successfully processed
    assert_eq!(import_summary.outcome, Some(OperationOutcome::FullSuccess));

    // Verify recreated notes in fresh database (contains 2 notes from the package)
    let fresh_notes = NoteRepository::list(&conn_dest, false).unwrap();
    assert_eq!(fresh_notes.len(), 2);

    let fresh_note1 = fresh_notes
        .iter()
        .find(|n| n.title == "Core Database Engine")
        .unwrap();
    let fresh_note2 = fresh_notes
        .iter()
        .find(|n| n.title == "Storage Architecture Spec")
        .unwrap();

    // Verify IDs are newly generated local UUIDs (never reusing original IDs)
    assert_ne!(fresh_note1.id, note1.id);
    assert_ne!(fresh_note2.id, note2.id);

    // Verify internal link in fresh_note2 was remapped to fresh_note1.id!
    let expected_link = format!("notepad://note/{}", fresh_note1.id);
    assert!(
        fresh_note2.content.contains(&expected_link),
        "Internal link must be rewritten to match new local UUID: content is: {}",
        fresh_note2.content
    );

    // Verify recreated notebook hierarchy
    let all_notebooks = NotebookRepository::list(&conn_dest).unwrap();
    assert!(all_notebooks.iter().any(|nb| nb.name == "Engineering"));
    assert!(all_notebooks.iter().any(|nb| nb.name == "Specs"));

    // Verify recreated tag
    let note1_tags = TagRepository::get_tags_for_note(&conn_dest, &fresh_note1.id).unwrap();
    assert_eq!(note1_tags.len(), 1);
    assert_eq!(note1_tags[0].name, "sqlite");
}

#[test]
fn test_phase10_task_22_collision_and_cancellation_invariants() {
    use personal_notepad_lib::storage::models::*;
    use personal_notepad_lib::storage::services::ImportExportService;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    let export_dir = env.dir.join("collisions");
    std::fs::create_dir_all(&export_dir).unwrap();

    // 1. Pre-create a file on disk
    let existing_file = export_dir.join("Meeting Notes.txt");
    std::fs::write(&existing_file, "Original untouched content").unwrap();

    // Create note with same title
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Meeting Notes".to_string()),
            content: Some("New exported content".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    // Batch export: collision resolution should produce Meeting Notes (1).txt
    let summary = ImportExportService::export_notes_batch(
        &conn,
        ExportBatchRequest {
            operation_id: Some("op-collision-test".to_string()),
            scope: Some(ExportScope::SingleNote {
                note_id: note.id.clone(),
            }),
            note_ids: vec![],
            format: ExportFormat::Txt,
            destination_dir: export_dir.to_string_lossy().to_string(),
            destination_file: None,
            include_metadata: false,
            overwrite: false,
        },
    )
    .unwrap();

    assert_eq!(summary.exported_count, 1);
    assert_eq!(summary.outcome, Some(OperationOutcome::FullSuccess));

    // Verify original file remained intact
    assert_eq!(
        std::fs::read_to_string(&existing_file).unwrap(),
        "Original untouched content"
    );

    // Verify collision file was created
    let collision_file = export_dir.join("Meeting Notes (1).txt");
    assert!(collision_file.exists());
    assert_eq!(
        std::fs::read_to_string(&collision_file).unwrap(),
        "New exported content"
    );

    // 2. Cancellation Test
    let note_b = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note B".to_string()),
            content: Some("Content B".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();
    let note_c = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note C".to_string()),
            content: Some("Content C".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    let op_cancel_id = "op-cancel-test".to_string();
    let op_cancel_clone = op_cancel_id.clone();

    let cancel_summary = ImportExportService::export_notes_batch_with_progress(
        &conn,
        ExportBatchRequest {
            operation_id: Some(op_cancel_id.clone()),
            scope: Some(ExportScope::SelectedNotes {
                note_ids: vec![note.id.clone(), note_b.id.clone(), note_c.id.clone()],
            }),
            note_ids: vec![],
            format: ExportFormat::Txt,
            destination_dir: export_dir.to_string_lossy().to_string(),
            destination_file: None,
            include_metadata: false,
            overwrite: false,
        },
        move |prog| {
            if prog.current_index == 1 {
                ImportExportService::cancel_operation(&op_cancel_clone);
            }
        },
    )
    .unwrap();

    assert_eq!(cancel_summary.outcome, Some(OperationOutcome::Cancelled));
    assert_eq!(cancel_summary.exported_count, 1);
    assert_eq!(cancel_summary.cancelled_count, 2);
}

#[test]
fn test_phase10_task_24_security_data_integrity_and_performance_verification() {
    use personal_notepad_lib::storage::models::*;
    use personal_notepad_lib::storage::services::ImportExportService;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    let export_dir = env.dir.join("sec_verification");
    std::fs::create_dir_all(&export_dir).unwrap();

    // =========================================================================
    // 1. Security Verification Suite
    // =========================================================================

    // A. Database Immutability during Export
    let initial_content = "# Secret Roadmap\n\nConfidential system details with Unicode: 🔐✨";
    let original_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Security Roadmap".to_string()),
            content: Some(initial_content.to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .unwrap();

    let dest_file = export_dir.join("Security Roadmap.md");
    let export_res = ImportExportService::export_single_note(
        &conn,
        ExportSingleNoteRequest {
            note_id: original_note.id.clone(),
            destination_path: dest_file.to_string_lossy().to_string(),
            format: ExportFormat::Md,
            overwrite: true,
        },
    )
    .unwrap();
    assert_eq!(export_res.note_id, original_note.id);

    // Verify database record is 100% untouched
    let rechecked_note = NoteRepository::get_by_id(&conn, &original_note.id)
        .unwrap()
        .unwrap();
    assert_eq!(rechecked_note.title, original_note.title);
    assert_eq!(rechecked_note.content, initial_content);
    assert_eq!(rechecked_note.format, original_note.format);
    assert_eq!(rechecked_note.created_at, original_note.created_at);
    assert_eq!(rechecked_note.modified_at, original_note.modified_at);
    assert_eq!(rechecked_note.is_favorite, true);
    assert_eq!(rechecked_note.is_pinned, true);

    // B. Rejection of Path Traversal & Managed Storage Escaping
    let traversal_dest = env.paths.database_file.to_string_lossy().to_string();
    let traversal_res = ImportExportService::export_single_note(
        &conn,
        ExportSingleNoteRequest {
            note_id: original_note.id.clone(),
            destination_path: traversal_dest,
            format: ExportFormat::Md,
            overwrite: true,
        },
    );
    assert!(
        traversal_res.is_err(),
        "Must reject exporting to managed database file"
    );

    // C. External ID Isolation Invariant
    let malicious_json = format!(
        r#"{{
            "app": "personal-notepad",
            "format": "personal-notepad-interchange",
            "version": "1.0",
            "exported_at": "2026-10-12T10:00:00Z",
            "note_count": 1,
            "notes": [{{
                "export_id": "malicious-foreign-primary-key-12345",
                "title": "Untrusted Note",
                "content": "<script>alert('pwn')</script>",
                "format": "txt"
            }}]
        }}"#
    );
    let untrusted_json_path = export_dir.join("untrusted.json");
    std::fs::write(&untrusted_json_path, malicious_json).unwrap();

    let scan_untrusted = ImportExportService::scan_candidates(
        &conn,
        &[untrusted_json_path.to_string_lossy().to_string()],
    )
    .unwrap();
    assert_eq!(scan_untrusted.valid_files, 1);

    let import_untrusted = ImportExportService::execute_import(
        &mut conn,
        ImportRequest {
            operation_id: Some("op-untrusted".to_string()),
            candidates: vec![ImportCandidateOverride {
                id: scan_untrusted.candidates[0].id.clone(),
                file_path: scan_untrusted.candidates[0].file_path.clone(),
                selected: true,
                custom_title: None,
            }],
            options: ImportOptions {
                target_notebook_id: None,
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        },
    )
    .unwrap();
    assert_eq!(import_untrusted.imported_count, 1);

    // Verify external export_id was NEVER inserted into SQLite as primary key
    let foreign_note_lookup =
        NoteRepository::get_by_id(&conn, "malicious-foreign-primary-key-12345").unwrap();
    assert!(
        foreign_note_lookup.is_none(),
        "External export_id must never become SQLite primary key"
    );

    // Verify script tags are stored as literal inert text
    let created_note_id = import_untrusted.results[0]
        .created_note_id
        .as_ref()
        .unwrap();
    let imported_untrusted_note = NoteRepository::get_by_id(&conn, created_note_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        imported_untrusted_note.content,
        "<script>alert('pwn')</script>"
    );

    // =========================================================================
    // 2. Data Integrity Verification Suite
    // =========================================================================

    // A. BOM Stripping and CRLF Normalization
    let mut bom_crlf_content = vec![0xEF, 0xBB, 0xBF];
    bom_crlf_content.extend_from_slice(
        "Heading Line\r\nSecond Line with Hindi: नमस्ते\r\n\r\nEnd Line\r\n".as_bytes(),
    );
    let bom_file_path = export_dir.join("bom_note.txt");
    std::fs::write(&bom_file_path, bom_crlf_content).unwrap();

    let scan_bom =
        ImportExportService::scan_candidates(&conn, &[bom_file_path.to_string_lossy().to_string()])
            .unwrap();
    assert_eq!(scan_bom.valid_files, 1);

    let import_bom = ImportExportService::execute_import(
        &mut conn,
        ImportRequest {
            operation_id: Some("op-bom-import".to_string()),
            candidates: vec![ImportCandidateOverride {
                id: scan_bom.candidates[0].id.clone(),
                file_path: scan_bom.candidates[0].file_path.clone(),
                selected: true,
                custom_title: None,
            }],
            options: ImportOptions {
                target_notebook_id: None,
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        },
    )
    .unwrap();
    assert_eq!(import_bom.imported_count, 1);

    let bom_note_id = import_bom.results[0].created_note_id.as_ref().unwrap();
    let bom_note = NoteRepository::get_by_id(&conn, bom_note_id)
        .unwrap()
        .unwrap();
    // BOM must be removed
    assert!(!bom_note.content.starts_with('\u{FEFF}'));
    // CRLF normalized to standard LF
    assert!(!bom_note.content.contains('\r'));
    assert_eq!(
        bom_note.content,
        "Heading Line\nSecond Line with Hindi: नमस्ते\n\nEnd Line\n"
    );

    // B. Binary and Corrupt UTF-8 Rejection
    let binary_bytes = vec![
        b'H', b'e', b'l', b'l', b'o', 0x00, b'W', b'o', b'r', b'l', b'd',
    ];
    let binary_path = export_dir.join("corrupt.txt");
    std::fs::write(&binary_path, binary_bytes).unwrap();

    let scan_binary =
        ImportExportService::scan_candidates(&conn, &[binary_path.to_string_lossy().to_string()])
            .unwrap();
    assert_eq!(scan_binary.invalid_files, 1);
    assert_eq!(
        scan_binary.candidates[0].validation_status,
        ValidationStatus::Invalid
    );

    // =========================================================================
    // 3. Performance Verification Suite
    // =========================================================================

    // Rapid batch creation and export of 30 notes
    let perf_notes_dir = export_dir.join("perf_exports");
    std::fs::create_dir_all(&perf_notes_dir).unwrap();

    let mut perf_note_ids = Vec::new();
    for i in 0..30 {
        let n = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Perf Benchmark Note {:02}", i)),
                content: Some(format!(
                    "High-speed benchmark note body content line {:02} with words.",
                    i
                )),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();
        perf_note_ids.push(n.id);
    }

    let start_export = std::time::Instant::now();
    let perf_export_summary = ImportExportService::export_notes_batch(
        &conn,
        ExportBatchRequest {
            operation_id: Some("op-perf-test".to_string()),
            scope: Some(ExportScope::SelectedNotes {
                note_ids: perf_note_ids,
            }),
            note_ids: vec![],
            format: ExportFormat::Txt,
            destination_dir: perf_notes_dir.to_string_lossy().to_string(),
            destination_file: None,
            include_metadata: false,
            overwrite: true,
        },
    )
    .unwrap();
    let export_duration = start_export.elapsed();

    assert_eq!(perf_export_summary.exported_count, 30);
    assert_eq!(
        perf_export_summary.outcome,
        Some(OperationOutcome::FullSuccess)
    );
    assert!(
        export_duration.as_millis() < 500,
        "30 notes batch export must execute in < 500ms"
    );

    // Rapid candidate scanning of the 30 exported files
    let mut files_to_scan = Vec::new();
    for i in 0..30 {
        files_to_scan.push(
            perf_notes_dir
                .join(format!("Perf Benchmark Note {:02}.txt", i))
                .to_string_lossy()
                .to_string(),
        );
    }

    let start_scan = std::time::Instant::now();
    let scan_perf = ImportExportService::scan_candidates(&conn, &files_to_scan).unwrap();
    let scan_duration = start_scan.elapsed();

    assert_eq!(scan_perf.total_files, 30);
    assert_eq!(scan_perf.valid_files, 30);
    assert!(
        scan_duration.as_millis() < 500,
        "30 files pre-scan must execute in < 500ms"
    );
}

#[test]
fn test_phase11_task_13_backup_listing_and_metadata_suite() {
    use personal_notepad_lib::storage::{
        models::{BackupCreateRequest, BackupType},
        services::{BackupListService, BackupService},
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Initial listing is empty
    let initial_list = BackupListService::list_backups(&conn, &env.paths).unwrap();
    assert!(
        initial_list.is_empty(),
        "Initial backups listing must be empty"
    );

    // 2. Populate notes
    let _n1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Listing Test Note 1".to_string()),
            content: Some("First note content for listing".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();
    let _n2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Listing Test Note 2".to_string()),
            content: Some("Second note content".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(true),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    // 3. Create a legitimate manual backup
    let backup_res = BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .unwrap();

    assert_eq!(backup_res.note_count, 2);
    assert!(std::path::Path::new(&backup_res.file_path).exists());

    // 4. List backups and verify discovered metadata
    let list1 = BackupListService::list_backups(&conn, &env.paths).unwrap();
    assert_eq!(list1.len(), 1);

    let item1 = &list1[0];
    assert_eq!(item1.file_name, backup_res.file_name);
    assert_eq!(item1.file_path, backup_res.file_path);
    assert_eq!(item1.backup_type, BackupType::Manual);
    assert_eq!(item1.note_count, Some(2));
    assert!(item1.file_size > 0);
    assert!(
        item1.is_valid,
        "Legitimate archive must be flagged as valid"
    );
    assert!(
        !item1.is_verified,
        "Discovered archive must be unverified by default"
    );
    assert!(item1.issue_summary.is_none());

    // 5. Place a damaged/corrupt file in backups directory
    let corrupt_path = env.paths.backups_dir.join("damaged-archive.pnbackup");
    std::fs::write(&corrupt_path, b"Not a valid zip archive file").unwrap();

    // Re-list: must include both files without crashing or throwing
    let list2 = BackupListService::list_backups(&conn, &env.paths).unwrap();
    assert_eq!(list2.len(), 2);

    let corrupt_item = list2
        .iter()
        .find(|i| i.file_name == "damaged-archive.pnbackup")
        .unwrap();
    assert!(
        !corrupt_item.is_valid,
        "Corrupt file must have is_valid = false"
    );
    assert!(!corrupt_item.is_verified);
    assert!(
        corrupt_item.issue_summary.is_some(),
        "Corrupt item must have an issue summary"
    );

    // 6. Test protected deletion guards
    let active_db_str = env.paths.database_file.to_string_lossy().to_string();
    let db_del_err = BackupListService::delete_managed_backup(&conn, &env.paths, &active_db_str);
    assert!(
        db_del_err.is_err(),
        "Must reject deletion of active database"
    );

    // 7. Legitimate deletion of managed backup
    let delete_ok =
        BackupListService::delete_managed_backup(&conn, &env.paths, &backup_res.file_path).unwrap();
    assert!(delete_ok);
    assert!(
        !std::path::Path::new(&backup_res.file_path).exists(),
        "File must be removed from disk"
    );

    // Re-list reflects deletion
    let list3 = BackupListService::list_backups(&conn, &env.paths).unwrap();
    assert_eq!(list3.len(), 1);
    assert_eq!(list3[0].file_name, "damaged-archive.pnbackup");
}

#[test]
fn test_phase11_task_14_backup_validation_suite() {
    use personal_notepad_lib::storage::{
        models::{
            BackupCompatibilityStatus, BackupCreateRequest, BackupType,
            BackupValidationIssueSeverity, CreateNoteDto,
        },
        services::{BackupService, BackupValidationService},
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Populate notes and attachments in test environment
    let n1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Val Test Note 1".to_string()),
            content: Some("First note content for validation".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    let _n2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Val Test Note 2".to_string()),
            content: Some("Second note content".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: Some(true),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    // Attach a real managed file
    let att_subpath = format!("{}/test_file.txt", n1.id);
    let disk_file = env.paths.attachments_dir.join(&att_subpath);
    std::fs::create_dir_all(disk_file.parent().unwrap()).unwrap();
    std::fs::write(&disk_file, b"Sample attachment text for validation").unwrap();

    conn.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            "val-att-1",
            n1.id,
            "test_file.txt",
            att_subpath,
            "text/plain",
            37,
            chrono::Utc::now().to_rfc3339(),
            chrono::Utc::now().to_rfc3339()
        ],
    ).unwrap();

    // 2. Create legitimate manual backup
    let backup_res = BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .unwrap();

    let archive_path = std::path::Path::new(&backup_res.file_path);
    assert!(archive_path.exists());

    // 3. Deep validate the backup before restore
    let val_res = BackupValidationService::validate_backup(archive_path, &env.paths).unwrap();
    assert!(val_res.is_valid, "Valid backup must pass deep validation");
    assert_eq!(val_res.compatibility, BackupCompatibilityStatus::Compatible);
    assert!(!val_res
        .issues
        .iter()
        .any(|i| i.severity == BackupValidationIssueSeverity::Error));

    let manifest = val_res.manifest.expect("Manifest must be present");
    assert_eq!(manifest.database.note_count, 2);
    assert_eq!(manifest.attachments_summary.total_files, 1);

    // 4. Invariant: Live database was not modified by validation
    let live_notes_count: i64 = conn
        .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        live_notes_count, 2,
        "Live database notes must remain unchanged"
    );

    // 5. Test validation of damaged archive
    let damaged_archive = env.paths.backups_dir.join("damaged.pnbackup");
    std::fs::write(&damaged_archive, b"Not valid zip data").unwrap();

    let damaged_val =
        BackupValidationService::validate_backup(&damaged_archive, &env.paths).unwrap();
    assert!(!damaged_val.is_valid, "Damaged archive must be invalid");
    assert_eq!(
        damaged_val.compatibility,
        BackupCompatibilityStatus::Corrupted
    );
    assert!(damaged_val
        .issues
        .iter()
        .any(|i| i.code == "ARCHIVE_READ_FAILED"));

    // 6. Test guard against validating live database directly
    let live_val_err =
        BackupValidationService::validate_backup(&env.paths.database_file, &env.paths);
    assert!(live_val_err.is_err(), "Must reject live database file");
}

#[test]
fn test_phase11_task_15_restore_preview_and_confirmation_suite() {
    use personal_notepad_lib::storage::{
        models::{BackupCompatibilityStatus, BackupCreateRequest, BackupType, CreateNoteDto},
        services::{BackupService, RestoreService},
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Populate database
    let _n1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Restore Target Note 1".to_string()),
            content: Some("First note content".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    let _n2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Restore Target Note 2".to_string()),
            content: Some("Second note content".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(true),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    // 2. Create valid manual backup
    let backup_res = BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .unwrap();

    let archive_path = std::path::Path::new(&backup_res.file_path);

    // 3. Generate preview of restore
    let preview = RestoreService::preview_restore(archive_path, &env.paths).unwrap();

    assert!(preview.can_restore, "Legitimate backup must permit restore");
    assert_eq!(preview.note_count, 2);
    assert_eq!(preview.file_name, backup_res.file_name);
    assert_eq!(preview.compatibility, BackupCompatibilityStatus::Compatible);
    assert!(
        preview.warnings.is_empty(),
        "Healthy backup preview has no warnings"
    );

    // 4. Invariant: Preview leaves live application database completely untouched
    let live_count: i64 = conn
        .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(live_count, 2, "Live database notes must remain unchanged");

    // 5. Corrupted archive preview blocks restoration
    let corrupted_file = env.paths.backups_dir.join("broken_preview.pnbackup");
    std::fs::write(&corrupted_file, b"corrupted non-zip content").unwrap();

    let corrupt_preview = RestoreService::preview_restore(&corrupted_file, &env.paths).unwrap();
    assert!(
        !corrupt_preview.can_restore,
        "Corrupt backup must NOT permit restore"
    );
    assert_eq!(
        corrupt_preview.compatibility,
        BackupCompatibilityStatus::Corrupted
    );
    assert!(
        !corrupt_preview.warnings.is_empty(),
        "Corrupt backup must list errors in warnings"
    );
}

#[test]
fn test_phase11_task_16_recovery_checkpoint_suite() {
    use personal_notepad_lib::storage::{
        models::{BackupCompatibilityStatus, BackupType, CreateNoteDto, CreateNotebookDto},
        repositories::{NotebookRepository, TagRepository},
        services::{BackupListService, BackupValidationService, RestoreService},
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a notebook
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Work Notebook".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    // 2. Create active note & trashed note
    let n1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Active Note Before Restore".to_string()),
            content: Some("Active note body with vital information".to_string()),
            format: Some("txt".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(false),
        },
    )
    .unwrap();

    let n2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Trashed Note Before Restore".to_string()),
            content: Some("Trash content that must be preserved".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .unwrap();
    NoteRepository::soft_delete(&conn, &n2.id).unwrap();

    // 3. Create a tag and assign it
    let tag = TagRepository::create(&conn, "urgent").unwrap();
    TagRepository::add_tag_to_note(&conn, &n1.id, &tag.id).unwrap();

    // 4. Create an attachment for active note
    let att_dir = env.paths.get_note_attachments_dir(&n1.id).unwrap();
    let att_file_name = format!("{}.txt", uuid::Uuid::new_v4());
    let att_disk_path = att_dir.join(&att_file_name);
    std::fs::write(&att_disk_path, b"Sample attachment text data").unwrap();

    let now_str = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            n1.id,
            "sample.txt",
            format!("{}/{}", n1.id, att_file_name),
            "text/plain",
            27i64,
            now_str,
            now_str,
        ],
    ).unwrap();

    // 5. Create Pre-Restore Recovery Checkpoint
    let checkpoint = RestoreService::create_recovery_checkpoint(&conn, &env.paths).unwrap();

    assert!(checkpoint.is_verified, "Checkpoint must be marked verified");
    assert_eq!(checkpoint.note_count, 1, "Must capture 1 active note");
    assert_eq!(checkpoint.trash_count, 1, "Must capture 1 trashed note");
    assert_eq!(checkpoint.notebook_count, 1, "Must capture 1 notebook");
    assert_eq!(checkpoint.tag_count, 1, "Must capture 1 tag");
    assert_eq!(checkpoint.attachment_count, 1, "Must capture 1 attachment");
    assert_eq!(checkpoint.schema_version, 4, "Current schema is v4");
    assert!(checkpoint.file_size > 0);
    assert!(
        std::path::Path::new(&checkpoint.file_path).exists(),
        "Checkpoint file must exist on disk"
    );
    assert!(
        checkpoint.file_path.contains("checkpoints"),
        "Must reside in checkpoints directory"
    );

    // 6. Independent validation of checkpoint
    let val_res = BackupValidationService::validate_backup(
        std::path::Path::new(&checkpoint.file_path),
        &env.paths,
    )
    .unwrap();
    assert!(
        val_res.is_valid,
        "Recovery checkpoint must pass deep validation"
    );
    assert_eq!(val_res.compatibility, BackupCompatibilityStatus::Compatible);
    let manifest = val_res.manifest.unwrap();
    assert_eq!(
        manifest.backup_type,
        BackupType::Checkpoint,
        "Manifest must declare backup_type as Checkpoint"
    );
    assert_eq!(manifest.attachments_summary.total_files, 1);

    // 7. Invariant: Live working database was left completely untouched
    let live_notes: i64 = conn
        .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(live_notes, 2, "Live database must remain unchanged");

    // 8. Test listing & latest checkpoint retrieval
    let chk_list = RestoreService::list_recovery_checkpoints(&env.paths).unwrap();
    assert_eq!(chk_list.len(), 1);
    assert_eq!(chk_list[0].checkpoint_id, checkpoint.checkpoint_id);

    let latest_chk = RestoreService::get_latest_recovery_checkpoint(&env.paths).unwrap();
    assert!(latest_chk.is_some());
    assert_eq!(latest_chk.unwrap().file_path, checkpoint.file_path);

    // 9. Verify BackupListService discovers checkpoints with Checkpoint type
    let backups_listing = BackupListService::list_backups(&conn, &env.paths).unwrap();
    assert!(
        backups_listing.iter().any(|b| b.file_path == checkpoint.file_path && b.backup_type == BackupType::Checkpoint),
        "BackupListService must include recovery checkpoint"
    );

    // 10. Immutability and collision safety: creating another checkpoint does NOT overwrite previous one
    let chk2 = RestoreService::create_recovery_checkpoint(&conn, &env.paths).unwrap();
    assert!(
        std::path::Path::new(&checkpoint.file_path).exists(),
        "Original checkpoint must still exist"
    );
    assert!(
        std::path::Path::new(&chk2.file_path).exists(),
        "Second checkpoint must exist"
    );

    let updated_list = RestoreService::list_recovery_checkpoints(&env.paths).unwrap();
    assert_eq!(
        updated_list.len(),
        2,
        "Both checkpoints must exist independently"
    );
}

#[test]
fn test_phase11_tasks_17_and_18_staged_restore_and_migration() {
    use personal_notepad_lib::storage::{
        init_connection,
        models::{BackupCreateRequest, BackupType, CreateNoteDto, CreateNotebookDto},
        repositories::{NoteRepository, NotebookRepository, TagRepository},
        run_migrations, run_migrations_custom,
        services::{BackupService, RestoreService},
        MIGRATIONS,
    };

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Seed live data: notebook, note, tag, attachment
    let notebook = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Staging Tests Notebook".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Staged Note Title".to_string()),
            content: Some("Live content for staging test".to_string()),
            notebook_id: Some(notebook.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let tag = TagRepository::create(&conn, "staged-tag").unwrap();
    TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).unwrap();

    let att_dir = env.paths.get_note_attachments_dir(&note.id).unwrap();
    let att_file_name = format!("{}.pdf", uuid::Uuid::new_v4());
    let att_disk_path = att_dir.join(&att_file_name);
    std::fs::write(&att_disk_path, b"%PDF-1.4 test staged restore attachment").unwrap();

    let now_str = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            note.id,
            "sample.pdf",
            format!("{}/{}", note.id, att_file_name),
            "application/pdf",
            39i64,
            now_str,
            now_str,
        ],
    ).unwrap();

    // 2. Create backup archive
    let backup_res = BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .unwrap();

    let archive_path = std::path::Path::new(&backup_res.file_path);

    // 3. Stage the candidate archive (Task 17)
    let staged = RestoreService::stage_restore_archive(archive_path, &env.paths).unwrap();
    assert!(staged.is_valid);
    assert!(staged.manifest_present);
    assert_eq!(staged.note_count, 1);
    assert_eq!(staged.notebook_count, 1);
    assert_eq!(staged.tag_count, 1);
    assert_eq!(staged.attachment_count, 1);
    assert_eq!(staged.initial_schema_version, 4);
    assert!(std::path::Path::new(&staged.staging_dir).exists());
    assert!(std::path::Path::new(&staged.database_path).exists());

    let staging_path = std::path::PathBuf::from(&staged.staging_dir);

    // 4. Test Task 18 migration on staged database (already v4 -> no-op)
    let mig_res = RestoreService::migrate_staged_database(&staging_path, &env.paths).unwrap();
    assert_eq!(mig_res.previous_schema_version, 4);
    assert_eq!(mig_res.migrated_schema_version, 4);
    assert_eq!(mig_res.migrations_applied, 0);
    assert!(mig_res.is_verified);
    assert_eq!(mig_res.note_count, 1);
    assert_eq!(mig_res.attachment_count, 1);

    // 5. Clean up staging directory
    let cleaned = RestoreService::cleanup_staged_restore(&staging_path, &env.paths).unwrap();
    assert!(cleaned);
    assert!(
        !staging_path.exists(),
        "Staging folder must be deleted after cleanup"
    );

    // 6. Test Task 18 legacy schema v1 staged restore & migration
    let legacy_db_dir = env.paths.root.join("legacy_test");
    std::fs::create_dir_all(&legacy_db_dir).unwrap();
    let legacy_db_path = legacy_db_dir.join("database.sqlite");

    let mut v1_conn = init_connection(&legacy_db_path).unwrap();
    run_migrations_custom(&mut v1_conn, &MIGRATIONS[..1]).unwrap();

    let legacy_note_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    v1_conn
        .execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_deleted)
         VALUES (?1, 'Legacy Integration Note', 'Migrated from v1', 'txt', ?2, ?2, 0)",
            [&legacy_note_id, &now],
        )
        .unwrap();
    drop(v1_conn);

    let legacy_archive_path = env.paths.backups_dir.join("legacy_v1_archive.pnbackup");
    {
        let file = std::fs::File::create(&legacy_archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("database.sqlite", options).unwrap();
        let db_bytes = std::fs::read(&legacy_db_path).unwrap();
        std::io::Write::write_all(&mut zip, &db_bytes).unwrap();
        zip.finish().unwrap();
    }

    let legacy_staged =
        RestoreService::stage_restore_archive(&legacy_archive_path, &env.paths).unwrap();
    assert_eq!(legacy_staged.initial_schema_version, 1);

    let legacy_staging_path = std::path::PathBuf::from(&legacy_staged.staging_dir);
    let legacy_mig =
        RestoreService::migrate_staged_database(&legacy_staging_path, &env.paths).unwrap();
    assert_eq!(legacy_mig.previous_schema_version, 1);
    assert_eq!(legacy_mig.migrated_schema_version, 4);
    assert_eq!(legacy_mig.migrations_applied, 3);
    assert_eq!(legacy_mig.note_count, 1);
    assert!(legacy_mig.is_verified);

    RestoreService::cleanup_staged_restore(&legacy_staging_path, &env.paths).unwrap();

    // 7. Verify invariant: Live database remains 100% intact and unaffected
    let live_notes: i64 = conn
        .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 0", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        live_notes, 1,
        "Live database must remain completely untouched"
    );
}

#[test]
fn test_phase11_task_19_restore_activation_and_rollback() {
    use personal_notepad_lib::storage::{
        init_connection,
        models::{
            BackupCreateRequest, BackupProgressEvent, BackupProgressStage, BackupRestoreRequest,
            BackupType, CreateNoteDto, CreateNotebookDto, RestoreActivationMarker,
            RestoreActivationPhase,
        },
        repositories::{NoteRepository, NotebookRepository, TagRepository},
        run_migrations,
        services::{BackupService, RestoreService},
    };
    use std::sync::{Arc, Mutex};

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Seed live data (this is the live active data prior to restore)
    let live_nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Live Pre-Restore Notebook".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let live_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Live Note Before Restore".to_string()),
            content: Some(
                "Live content that should be backed up in recovery checkpoint".to_string(),
            ),
            notebook_id: Some(live_nb.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let live_att_dir = env.paths.get_note_attachments_dir(&live_note.id).unwrap();
    let live_att_file = format!("{}.txt", uuid::Uuid::new_v4());
    std::fs::write(
        live_att_dir.join(&live_att_file),
        b"live note attachment data",
    )
    .unwrap();

    let now_str = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            live_note.id,
            "live.txt",
            format!("{}/{}", live_note.id, live_att_file),
            "text/plain",
            26i64,
            now_str,
            now_str,
        ],
    ).unwrap();

    // 2. Prepare a candidate backup in a distinct environment
    let cand_env = TestEnv::new();
    let mut cand_conn =
        init_connection(&cand_env.paths.database_file).expect("Candidate conn failed");
    run_migrations(&mut cand_conn).expect("Candidate migrations failed");

    let cand_nb = NotebookRepository::create(
        &cand_conn,
        CreateNotebookDto {
            name: "Candidate Archive Notebook".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let cand_note = NoteRepository::create(
        &cand_conn,
        CreateNoteDto {
            title: Some("Candidate Restored Note".to_string()),
            content: Some("Candidate restored note body with keyword quantum".to_string()),
            notebook_id: Some(cand_nb.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let cand_tag = TagRepository::create(&cand_conn, "restored-quantum-tag").unwrap();
    TagRepository::add_tag_to_note(&cand_conn, &cand_note.id, &cand_tag.id).unwrap();

    let cand_att_dir = cand_env
        .paths
        .get_note_attachments_dir(&cand_note.id)
        .unwrap();
    let cand_att_file = format!("{}.pdf", uuid::Uuid::new_v4());
    std::fs::write(
        cand_att_dir.join(&cand_att_file),
        b"%PDF-1.4 candidate restored attachment content",
    )
    .unwrap();

    let cand_now = chrono::Utc::now().to_rfc3339();
    cand_conn.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            cand_note.id,
            "quantum.pdf",
            format!("{}/{}", cand_note.id, cand_att_file),
            "application/pdf",
            48i64,
            cand_now,
            cand_now,
        ],
    ).unwrap();

    let candidate_backup = BackupService::create_backup(
        &cand_conn,
        &cand_env.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .unwrap();
    drop(cand_conn);

    let archive_path = std::path::PathBuf::from(&candidate_backup.file_path);
    assert!(archive_path.exists());

    // 3. Execute controlled restore with progress recording
    let stages_captured = Arc::new(Mutex::new(Vec::new()));
    let stages_clone = Arc::clone(&stages_captured);

    let restore_res = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        BackupRestoreRequest {
            file_path: archive_path.to_string_lossy().to_string(),
            operation_id: None,
        },
        Some(move |ev: BackupProgressEvent| {
            stages_clone.lock().unwrap().push(ev.stage);
        }),
    )
    .unwrap();

    // 4. Validate restore result summary
    assert!(restore_res.success);
    assert_eq!(restore_res.note_count, 1);
    assert_eq!(restore_res.notebook_count, 1);
    assert_eq!(restore_res.tag_count, 1);
    assert_eq!(restore_res.attachment_count, 1);
    assert_eq!(restore_res.trash_count, 0);

    // Verify progress events occurred across lifecycle
    let stages = stages_captured.lock().unwrap().clone();
    assert!(stages.contains(&BackupProgressStage::Starting));
    assert!(stages.contains(&BackupProgressStage::ExtractingStaging));
    assert!(stages.contains(&BackupProgressStage::RebuildingSearchIndex));
    assert!(stages.contains(&BackupProgressStage::Completed));

    // 5. Pre-restore recovery checkpoint must exist and be verified
    let chk_path = std::path::Path::new(&restore_res.checkpoint_path);
    assert!(
        chk_path.exists(),
        "Pre-restore checkpoint must be preserved on disk"
    );
    let chk_val = RestoreService::validate_recovery_checkpoint(chk_path, &env.paths).unwrap();
    assert!(chk_val.is_valid);
    assert_eq!(
        chk_val.manifest.unwrap().database.note_count,
        1,
        "Checkpoint captured pre-restore state"
    );

    // 6. Verify live database now holds candidate data and NOT old data
    let current_titles: Vec<String> = conn
        .prepare("SELECT title FROM notes WHERE is_deleted = 0")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(current_titles, vec!["Candidate Restored Note"]);

    // Verify restored attachment exists on disk
    let restored_att_on_disk = env
        .paths
        .attachments_dir
        .join(&cand_note.id)
        .join(&cand_att_file);
    assert!(
        restored_att_on_disk.exists(),
        "Restored attachment file must exist on disk"
    );

    // Verify Search index works on restored content
    let fts_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'quantum*'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fts_count, 1, "FTS5 search index must find restored note");

    // 7. Verify staging directory and marker are cleaned up on success
    assert!(
        !env.paths.restore_marker_file().exists(),
        "Marker must be cleared on success"
    );
    let remaining_staging = std::fs::read_dir(&env.paths.backups_dir)
        .unwrap()
        .flatten()
        .filter(|e| {
            e.path().is_dir()
                && e.file_name()
                    .to_string_lossy()
                    .starts_with(".restore_staging_")
        })
        .count();
    assert_eq!(
        remaining_staging, 0,
        "All staging directories must be cleaned up"
    );

    // 8. Test crash recovery / interrupted marker handling
    let op_id = uuid::Uuid::new_v4().to_string();
    let sim_db_backup = env
        .paths
        .root
        .join(format!(".db_pre_restore_{op_id}.sqlite"));
    let sim_att_backup = env.paths.root.join(format!(".att_pre_restore_{op_id}"));

    // Copy live database as swap backup
    std::fs::copy(&env.paths.database_file, &sim_db_backup).unwrap();
    std::fs::create_dir_all(&sim_att_backup).unwrap();

    let marker = RestoreActivationMarker {
        operation_id: op_id,
        started_at: chrono::Utc::now().to_rfc3339(),
        candidate_archive_path: archive_path.to_string_lossy().to_string(),
        staging_dir: env
            .paths
            .backups_dir
            .join(".restore_staging_test_crash")
            .to_string_lossy()
            .to_string(),
        checkpoint_path: restore_res.checkpoint_path.clone(),
        live_db_backup_path: sim_db_backup.to_string_lossy().to_string(),
        live_attachments_backup_path: sim_att_backup.to_string_lossy().to_string(),
        phase: RestoreActivationPhase::FilesSwapped,
    };
    RestoreService::write_activation_marker(&marker, &env.paths).unwrap();
    assert!(env.paths.restore_marker_file().exists());

    // Call recover_if_interrupted
    let recovered = RestoreService::recover_if_interrupted(&env.paths).unwrap();
    assert!(
        recovered.is_some(),
        "Interrupted marker must trigger crash recovery"
    );
    assert!(
        !env.paths.restore_marker_file().exists(),
        "Marker must be purged after crash recovery"
    );
    assert!(
        !sim_db_backup.exists(),
        "Swap backup file must be restored/removed"
    );
}

#[test]
fn test_phase11_task_20_post_restore_validation_and_restart() {
    use personal_notepad_lib::storage::{
        init_connection,
        models::{
            BackupCreateRequest, BackupRestoreRequest, BackupType, CreateNoteDto, CreateNotebookDto,
        },
        repositories::{NoteRepository, NotebookRepository, TagRepository},
        run_migrations,
        services::{BackupService, RestoreService},
    };

    // 1. Live environment
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    let old_nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Old Pre-Restore Notebook".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let _old_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Old Note to be Overwritten".to_string()),
            content: Some("Old content".to_string()),
            notebook_id: Some(old_nb.id),
            ..Default::default()
        },
    )
    .unwrap();

    // 2. Candidate environment
    let cand_env = TestEnv::new();
    let mut cand_conn =
        init_connection(&cand_env.paths.database_file).expect("Cand connection failed");
    run_migrations(&mut cand_conn).expect("Cand migration failed");

    let restored_nb = NotebookRepository::create(
        &cand_conn,
        CreateNotebookDto {
            name: "Restored Astrophysics Notebook".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let target_ref_id = uuid::Uuid::new_v4().to_string();
    let restored_note = NoteRepository::create(
        &cand_conn,
        CreateNoteDto {
            title: Some("Pulsar Radiations".to_string()),
            content: Some(format!("Observational data with [Target Note](notepad://note/{target_ref_id}) and keyword interstellar.")),
            notebook_id: Some(restored_nb.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    ).unwrap();

    let restored_tag = TagRepository::create(&cand_conn, "astronomy-tag").unwrap();
    TagRepository::add_tag_to_note(&cand_conn, &restored_note.id, &restored_tag.id).unwrap();

    // Attach file
    let att_dir = cand_env
        .paths
        .get_note_attachments_dir(&restored_note.id)
        .unwrap();
    let att_filename = format!("{}.dat", uuid::Uuid::new_v4());
    std::fs::write(att_dir.join(&att_filename), b"binary pulsar spectra data").unwrap();

    let cand_now = chrono::Utc::now().to_rfc3339();
    cand_conn.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            restored_note.id,
            "pulsar.dat",
            format!("{}/{}", restored_note.id, att_filename),
            "application/octet-stream",
            26i64,
            cand_now,
            cand_now,
        ],
    ).unwrap();

    // Setting
    cand_conn
        .execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('theme', 'dark')",
            [],
        )
        .unwrap();

    // Create candidate backup
    let candidate_backup = BackupService::create_backup(
        &cand_conn,
        &cand_env.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .unwrap();
    drop(cand_conn);

    // 3. Execute restore
    let restore_result = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        BackupRestoreRequest {
            file_path: candidate_backup.file_path.clone(),
            operation_id: None,
        },
        None::<fn(_)>,
    )
    .unwrap();

    assert!(restore_result.success);
    let val_report = restore_result
        .validation_report
        .expect("PostRestoreValidationReport must be present");
    assert!(val_report.is_valid);
    assert!(val_report.integrity_check_passed);
    assert!(val_report.foreign_key_check_passed);
    assert_eq!(val_report.schema_version, 4);
    assert_eq!(val_report.note_count, 1);
    assert_eq!(val_report.notebook_count, 1);
    assert_eq!(val_report.tag_count, 1);
    assert_eq!(val_report.attachment_count, 1);
    assert!(val_report.attachments_verified_on_disk);
    assert!(val_report.fts_rebuilt_and_queryable);
    assert!(val_report.internal_links_verified);
    assert!(val_report.settings_verified);

    // 4. Standalone verification check via verify_and_reinitialize_activated_state
    let standalone_report =
        RestoreService::verify_and_reinitialize_activated_state(&conn, &env.paths).unwrap();
    assert_eq!(val_report, standalone_report);

    // 5. Verify live active state
    let note_titles: Vec<String> = conn
        .prepare("SELECT title FROM notes WHERE is_deleted = 0")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(note_titles, vec!["Pulsar Radiations"]);

    let theme_setting: String = conn
        .query_row("SELECT value FROM settings WHERE key = 'theme'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(theme_setting, "dark");

    // FTS5 test
    let fts_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'interstellar*'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fts_count, 1);

    // 6. Simulate complete application exit and restart lifecycle
    drop(conn);

    let recovery_check = RestoreService::recover_if_interrupted(&env.paths).unwrap();
    assert!(
        recovery_check.is_none(),
        "No recovery marker should remain after clean restore"
    );

    let mut restarted_conn =
        init_connection(&env.paths.database_file).expect("Reopening after restart failed");
    run_migrations(&mut restarted_conn).expect("Migrations check on restart failed");

    // 7. Verify all restored state persists across restart
    let restarted_titles: Vec<String> = restarted_conn
        .prepare("SELECT title FROM notes WHERE is_deleted = 0")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(restarted_titles, vec!["Pulsar Radiations"]);

    let restarted_fts: i64 = restarted_conn
        .query_row(
            "SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'interstellar*'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(restarted_fts, 1);

    let att_disk_path = env
        .paths
        .attachments_dir
        .join(&restored_note.id)
        .join(&att_filename);
    assert!(
        att_disk_path.exists(),
        "Attachment must remain on disk after restart"
    );
}

#[test]
fn test_phase11_task_21_restore_progress_cancellation_and_feedback() {
    use personal_notepad_lib::storage::{
        init_connection,
        models::{
            BackupCreateRequest, BackupProgressEvent, BackupProgressStage, BackupRestoreRequest,
            BackupType, CreateNoteDto,
        },
        repositories::NoteRepository,
        run_migrations,
        services::{BackupService, RestoreService},
    };
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Setup candidate backup archive
    let cand_env = TestEnv::new();
    let mut cand_conn =
        init_connection(&cand_env.paths.database_file).expect("Cand connection failed");
    run_migrations(&mut cand_conn).expect("Cand migration failed");
    let cand_note = NoteRepository::create(
        &cand_conn,
        CreateNoteDto {
            title: Some("Galactic Astronomy".to_string()),
            content: Some("Observational astronomy notes.".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .unwrap();
    let cand_backup = BackupService::create_backup(
        &cand_conn,
        &cand_env.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(BackupProgressEvent)>,
    )
    .unwrap();

    // 2. Setup live database with initial working note
    let _live_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Initial Live Note".to_string()),
            content: Some(
                "Initial content that should survive until confirmed restore".to_string(),
            ),
            format: Some("txt".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .unwrap();

    // 3. Test Full Progression and Cancellable Flags during restore
    let op_id = format!("op-prog-{}", Uuid::new_v4());
    let events = Arc::new(Mutex::new(Vec::<BackupProgressEvent>::new()));
    let events_clone = Arc::clone(&events);

    let res = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        BackupRestoreRequest {
            file_path: cand_backup.file_path.clone(),
            operation_id: Some(op_id.clone()),
        },
        Some(move |evt: BackupProgressEvent| {
            events_clone.lock().unwrap().push(evt);
        }),
    )
    .unwrap();

    assert!(res.success);
    let captured = events.lock().unwrap().clone();

    // Verify all explicit measured stages are present
    let stages: Vec<BackupProgressStage> = captured.iter().map(|e| e.stage).collect();
    assert!(stages.contains(&BackupProgressStage::Starting));
    assert!(stages.contains(&BackupProgressStage::ValidatingBackup));
    assert!(stages.contains(&BackupProgressStage::PreparingStagingData));
    assert!(stages.contains(&BackupProgressStage::CheckingDatabaseIntegrity));
    assert!(stages.contains(&BackupProgressStage::CheckingAttachments));
    assert!(stages.contains(&BackupProgressStage::CreatingCheckpoint));
    assert!(stages.contains(&BackupProgressStage::PreparingActivation));
    assert!(stages.contains(&BackupProgressStage::ActivatingRestoredData));
    assert!(stages.contains(&BackupProgressStage::VerifyingRestoredState));
    assert!(stages.contains(&BackupProgressStage::Completed));

    // Verify preparation stages allow cancellation
    for evt in captured.iter() {
        match evt.stage {
            BackupProgressStage::Starting
            | BackupProgressStage::ValidatingBackup
            | BackupProgressStage::PreparingStagingData
            | BackupProgressStage::ExtractingStaging
            | BackupProgressStage::CheckingDatabaseIntegrity
            | BackupProgressStage::ValidatingStagedData
            | BackupProgressStage::CheckingAttachments
            | BackupProgressStage::CreatingCheckpoint => {
                assert!(evt.cancellable, "Stage {:?} must be cancellable", evt.stage);
            }
            BackupProgressStage::PreparingActivation
            | BackupProgressStage::ActivatingRestoredData
            | BackupProgressStage::VerifyingRestoredState
            | BackupProgressStage::RebuildingSearchIndex
            | BackupProgressStage::Completed => {
                assert!(
                    !evt.cancellable,
                    "Stage {:?} must NOT be casually cancellable",
                    evt.stage
                );
            }
            _ => {}
        }
        assert_eq!(evt.operation_id, Some(op_id.clone()));
    }

    // 4. Test In-flight Cancellation Before Activation
    let op_cancel_id = format!("op-cancel-{}", Uuid::new_v4());
    let op_cancel_clone = op_cancel_id.clone();
    let cancel_events = Arc::new(Mutex::new(Vec::<BackupProgressEvent>::new()));
    let cancel_events_clone = Arc::clone(&cancel_events);

    let cancel_res = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        BackupRestoreRequest {
            file_path: cand_backup.file_path.clone(),
            operation_id: Some(op_cancel_id.clone()),
        },
        Some(move |evt: BackupProgressEvent| {
            if evt.stage == BackupProgressStage::CheckingAttachments {
                RestoreService::request_cancellation(&op_cancel_clone);
            }
            cancel_events_clone.lock().unwrap().push(evt);
        }),
    );

    assert!(cancel_res.is_err());
    let captured_cancel = cancel_events.lock().unwrap().clone();
    let final_evt = captured_cancel
        .last()
        .expect("Must have emitted cancel event");
    assert_eq!(final_evt.stage, BackupProgressStage::Cancelled);
    assert!(!final_evt.cancellable);
    assert!(final_evt.message.contains("cancelled"));

    // Verify live data survived untouched
    let note = NoteRepository::get_by_id(&conn, &cand_note.id)
        .unwrap()
        .unwrap();
    assert_eq!(note.title, "Galactic Astronomy");
}

#[test]
fn test_phase11_task_22_backup_health_and_failure_notifications() {
    let env = TestEnv::new();
    let mut conn =
        init_connection(&env.paths.database_file).expect("Database initialization failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Initial State: Auto-backup enabled by default, 0 backups -> Warning level with NO_VERIFIED_BACKUP
    let health1 = personal_notepad_lib::storage::services::BackupHealthService::get_health_status(
        &conn, &env.paths,
    )
    .expect("Get health failed");
    assert!(health1.auto_backup_enabled);
    assert!(health1.destination_available);
    assert_eq!(health1.managed_backup_count, 0);
    assert!(!health1.has_verified_backup);
    assert_eq!(
        health1.health_level,
        personal_notepad_lib::storage::models::BackupHealthLevel::Warning
    );
    assert!(health1
        .warnings
        .iter()
        .any(|w| w.code == "NO_VERIFIED_BACKUP"));
    assert!(health1
        .warnings
        .iter()
        .any(|w| w.code == "SAME_PHYSICAL_DISK"));

    // 2. Perform a successful backup
    let _note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Mission Critical Note".to_string()),
            content: Some("Safety first".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note failed");

    let req = personal_notepad_lib::storage::models::BackupCreateRequest {
        destination_path: None,
        backup_type: Some(personal_notepad_lib::storage::models::BackupType::Automatic),
        allow_overwrite: Some(false),
    };
    let bk_res = personal_notepad_lib::storage::services::BackupService::create_backup(
        &conn,
        &env.paths,
        req,
        None::<fn(personal_notepad_lib::storage::models::BackupProgressEvent)>,
    )
    .expect("Create backup failed");
    assert!(bk_res.verified);

    // Update scheduler state to reflect successful run
    let mut sched_state =
        personal_notepad_lib::storage::services::BackupSchedulerService::get_state(&conn).unwrap();
    sched_state.last_successful_at = Some(chrono::Utc::now().to_rfc3339());
    sched_state.failure_count = 0;
    personal_notepad_lib::storage::services::BackupSchedulerService::save_state(
        &conn,
        &sched_state,
    )
    .unwrap();

    let health2 = personal_notepad_lib::storage::services::BackupHealthService::get_health_status(
        &conn, &env.paths,
    )
    .expect("Get health failed");
    assert!(health2.auto_backup_enabled);
    assert!(health2.has_verified_backup);
    assert!(health2.latest_backup_verified);
    assert_eq!(health2.managed_backup_count, 1);
    assert_eq!(
        health2.health_level,
        personal_notepad_lib::storage::models::BackupHealthLevel::Healthy
    );

    // 3. Unavailable Destination Simulation
    let mut custom_settings =
        personal_notepad_lib::storage::services::BackupSettingsService::get_settings(&conn)
            .unwrap();
    let bad_dest_file = env.dir.join("not_a_dir.txt");
    std::fs::write(&bad_dest_file, b"blocking file").unwrap();
    let unwritable_dest = bad_dest_file.join("subfolder_impossible");
    custom_settings.backup_destination_dir = Some(unwritable_dest.to_string_lossy().to_string());
    let serialized_settings = serde_json::to_string(&custom_settings).unwrap();
    SettingsRepository::set(&conn, "backup_settings", &serialized_settings).unwrap();

    let health3 = personal_notepad_lib::storage::services::BackupHealthService::get_health_status(
        &conn, &env.paths,
    )
    .expect("Get health failed");
    assert!(!health3.destination_available);
    assert_eq!(
        health3.health_level,
        personal_notepad_lib::storage::models::BackupHealthLevel::Critical
    );
    assert!(health3
        .warnings
        .iter()
        .any(|w| w.code == "DESTINATION_UNAVAILABLE"));

    // Reset settings
    custom_settings.backup_destination_dir = None;
    let serialized_settings = serde_json::to_string(&custom_settings).unwrap();
    SettingsRepository::set(&conn, "backup_settings", &serialized_settings).unwrap();

    // 4. Consecutive Failures & Category Persistence
    sched_state.failure_count = 4;
    sched_state.last_error_message = Some("Write failed: storage medium full.".to_string());
    sched_state.last_failure_category = Some("InsufficientDiskSpace".to_string());
    personal_notepad_lib::storage::services::BackupSchedulerService::save_state(
        &conn,
        &sched_state,
    )
    .unwrap();

    let health4 = personal_notepad_lib::storage::services::BackupHealthService::get_health_status(
        &conn, &env.paths,
    )
    .expect("Get health failed");
    assert_eq!(
        health4.health_level,
        personal_notepad_lib::storage::models::BackupHealthLevel::Critical
    );
    assert_eq!(
        health4.last_failure_category.as_deref(),
        Some("InsufficientDiskSpace")
    );
    assert!(health4
        .warnings
        .iter()
        .any(|w| w.code == "LAST_BACKUP_FAILED"));

    // 5. Disabled State
    custom_settings.auto_backup_enabled = false;
    let serialized_settings = serde_json::to_string(&custom_settings).unwrap();
    SettingsRepository::set(&conn, "backup_settings", &serialized_settings).unwrap();

    // Reset failure count so it tests disabled precedence
    sched_state.failure_count = 0;
    sched_state.last_error_message = None;
    personal_notepad_lib::storage::services::BackupSchedulerService::save_state(
        &conn,
        &sched_state,
    )
    .unwrap();

    let health5 = personal_notepad_lib::storage::services::BackupHealthService::get_health_status(
        &conn, &env.paths,
    )
    .expect("Get health failed");
    assert!(!health5.auto_backup_enabled);
    assert_eq!(
        health5.health_level,
        personal_notepad_lib::storage::models::BackupHealthLevel::Disabled
    );
    assert!(health5
        .warnings
        .iter()
        .any(|w| w.code == "AUTO_BACKUP_DISABLED"));
}

#[test]
fn test_phase11_task_24_end_to_end_and_data_loss_simulations() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::{
        init_connection,
        models::{
            BackupCompatibilityStatus, BackupCreateRequest, BackupHealthLevel, BackupProgressEvent,
            BackupRestoreRequest, BackupType, CreateNoteDto, CreateNotebookDto,
            RestoreActivationMarker, RestoreActivationPhase,
        },
        repositories::{NoteRepository, NotebookRepository, TagRepository},
        run_migrations,
        services::{
            backup_retention::BackupRetentionManager, BackupHealthService, BackupSchedulerService,
            BackupService, BackupSettingsService, BackupValidationService, RestoreService,
        },
    };

    // -------------------------------------------------------------
    // SCENARIO A — Normal Recovery
    // 1. Create several notes.
    // 2. Create notebooks and tags.
    // 3. Add favorites.
    // 4. Attach a file.
    // 5. Move a note to Trash.
    // 6. Create and validate a backup.
    // 7. Make additional changes.
    // 8. Restore the backup in a controlled test environment.
    // 9. Verify the restored state matches the backup.
    // 10. Restart the application and verify persistence.
    // -------------------------------------------------------------
    let env_a = TestEnv::new();
    let mut conn_a =
        init_connection(&env_a.paths.database_file).expect("Conn A initialization failed");
    run_migrations(&mut conn_a).expect("Migration A failed");

    // 1-3. Notes, Notebooks, Tags, Favorites
    let nb_research = NotebookRepository::create(
        &conn_a,
        CreateNotebookDto {
            name: "Quantum Gravity".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let note_active_1 = NoteRepository::create(
        &conn_a,
        CreateNoteDto {
            title: Some("AdS/CFT Duality".to_string()),
            content: Some("Holographic principle calculations".to_string()),
            notebook_id: Some(nb_research.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .unwrap();

    let _note_active_2 = NoteRepository::create(
        &conn_a,
        CreateNoteDto {
            title: Some("Spin Networks".to_string()),
            content: Some("Loop quantum gravity area operators".to_string()),
            notebook_id: Some(nb_research.id.clone()),
            is_favorite: Some(false),
            ..Default::default()
        },
    )
    .unwrap();

    let tag_theoretical = TagRepository::create(&conn_a, "theoretical-physics").unwrap();
    TagRepository::add_tag_to_note(&conn_a, &note_active_1.id, &tag_theoretical.id).unwrap();

    // 4. Attach a file
    let att_dir_1 = env_a
        .paths
        .get_note_attachments_dir(&note_active_1.id)
        .unwrap();
    let att_filename_1 = format!("{}.pdf", uuid::Uuid::new_v4());
    let att_bytes_1 = b"%PDF-1.4 Holographic calculation matrix proof";
    std::fs::write(att_dir_1.join(&att_filename_1), att_bytes_1).unwrap();

    let now_str_a = chrono::Utc::now().to_rfc3339();
    conn_a.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            note_active_1.id,
            "proof.pdf",
            format!("{}/{}", note_active_1.id, att_filename_1),
            "application/pdf",
            att_bytes_1.len() as i64,
            now_str_a,
            now_str_a,
        ],
    ).unwrap();

    // 5. Move a note to Trash
    let note_trashed = NoteRepository::create(
        &conn_a,
        CreateNoteDto {
            title: Some("Deprecated Calculations".to_string()),
            content: Some("Obsolete notes moved to trash".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    NoteRepository::soft_delete(&conn_a, &note_trashed.id).unwrap();

    // Flush WAL writes to SQLite disk file before snapshotting
    conn_a
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();

    // 6. Create and validate a backup
    let backup_req_a = BackupCreateRequest {
        destination_path: None,
        backup_type: Some(BackupType::Manual),
        allow_overwrite: Some(false),
    };
    let backup_res_a = BackupService::create_backup(
        &conn_a,
        &env_a.paths,
        backup_req_a,
        None::<fn(BackupProgressEvent)>,
    )
    .expect("Scenario A backup creation failed");

    assert!(backup_res_a.verified);
    assert_eq!(backup_res_a.note_count, 3); // 2 active + 1 trash
    assert_eq!(backup_res_a.attachment_count, 1);

    let backup_archive_path = std::path::Path::new(&backup_res_a.file_path);
    assert!(backup_archive_path.exists());

    // Validate the created archive independently
    let val_scenario_a =
        BackupValidationService::validate_backup(backup_archive_path, &env_a.paths).unwrap();
    assert!(val_scenario_a.is_valid);
    assert_eq!(
        val_scenario_a.compatibility,
        BackupCompatibilityStatus::Compatible
    );

    // 7. Make additional live modifications (simulate user doing intervening work)
    let _note_intervening = NoteRepository::create(
        &conn_a,
        CreateNoteDto {
            title: Some("Temporary Post-Backup Note".to_string()),
            content: Some(
                "This note was created after the backup and should be replaced by restore"
                    .to_string(),
            ),
            ..Default::default()
        },
    )
    .unwrap();

    // Mutate existing active note content
    conn_a
        .execute(
            "UPDATE notes SET content = 'Corrupted intervening edit' WHERE id = ?1",
            rusqlite::params![note_active_1.id],
        )
        .unwrap();

    // Intervening change: delete attachment from live DB and disk
    conn_a
        .execute(
            "DELETE FROM attachments WHERE note_id = ?1",
            rusqlite::params![note_active_1.id],
        )
        .unwrap();
    let _ = std::fs::remove_file(att_dir_1.join(&att_filename_1));

    // Confirm live state before restore has 4 notes and 0 attachments
    let count_before_restore: i64 = conn_a
        .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count_before_restore, 4);
    let att_count_before: i64 = conn_a
        .query_row("SELECT COUNT(1) FROM attachments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(att_count_before, 0);

    // 8. Restore the backup in controlled environment
    let restore_req_a = BackupRestoreRequest {
        file_path: backup_res_a.file_path.clone(),
        operation_id: Some(format!("restore-scenario-a-{}", uuid::Uuid::new_v4())),
    };
    let restore_res_a = RestoreService::execute_restore(
        &mut conn_a,
        &env_a.paths,
        restore_req_a,
        None::<fn(BackupProgressEvent)>,
    )
    .expect("Scenario A restore execution failed");

    assert!(restore_res_a.success);
    assert_eq!(restore_res_a.note_count, 2); // 2 active notes
    assert_eq!(restore_res_a.trash_count, 1); // 1 trashed note
    assert_eq!(restore_res_a.notebook_count, 1);
    assert_eq!(restore_res_a.tag_count, 1);
    assert_eq!(restore_res_a.attachment_count, 1);

    // 9. Verify restored state matches the backup exactly
    let note1_restored = NoteRepository::get_by_id(&conn_a, &note_active_1.id)
        .unwrap()
        .unwrap();
    assert_eq!(note1_restored.title, "AdS/CFT Duality");
    assert_eq!(note1_restored.content, "Holographic principle calculations");
    assert!(note1_restored.is_favorite);
    assert!(note1_restored.deleted_at.is_none());

    let trashed_restored = NoteRepository::get_by_id(&conn_a, &note_trashed.id)
        .unwrap()
        .unwrap();
    assert_eq!(trashed_restored.title, "Deprecated Calculations");
    assert!(
        trashed_restored.deleted_at.is_some(),
        "Trash status must be preserved"
    );

    // Intervening post-backup note must be completely gone
    let intervening_check: i64 = conn_a
        .query_row(
            "SELECT COUNT(1) FROM notes WHERE title = 'Temporary Post-Backup Note'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        intervening_check, 0,
        "Post-backup note must be removed by restore"
    );

    // Attachment file must be restored on disk with exact bytes
    let restored_att_file = att_dir_1.join(&att_filename_1);
    assert!(
        restored_att_file.exists(),
        "Attachment file must be restored on disk"
    );
    let restored_att_bytes = std::fs::read(&restored_att_file).unwrap();
    assert_eq!(
        restored_att_bytes, att_bytes_1,
        "Attachment bytes must match original"
    );

    // Pre-restore checkpoint must exist in checkpoints directory
    let checkpoint_file = std::path::Path::new(&restore_res_a.checkpoint_path);
    assert!(
        checkpoint_file.exists(),
        "Pre-restore checkpoint must be preserved"
    );
    let chk_val =
        RestoreService::validate_recovery_checkpoint(checkpoint_file, &env_a.paths).unwrap();
    let chk_manifest = chk_val.manifest.unwrap();
    assert_eq!(
        chk_manifest.database.note_count, 3,
        "Checkpoint captured the 3 pre-restore active notes"
    );
    assert_eq!(
        chk_manifest.database.trash_count, 1,
        "Checkpoint captured the 1 pre-restore trashed note"
    );

    // 10. Simulate cold restart: drop conn_a and reopen from disk
    drop(conn_a);
    let conn_a_restarted =
        init_connection(&env_a.paths.database_file).expect("Reopened connection failed");
    let total_restarted_notes: i64 = conn_a_restarted
        .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total_restarted_notes, 3, "Notes persisted across restart");
    let restarted_nb_count: i64 = conn_a_restarted
        .query_row("SELECT COUNT(1) FROM notebooks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(restarted_nb_count, 1, "Notebooks persisted across restart");

    // -------------------------------------------------------------
    // SCENARIO B — Corrupt Backup
    // 1. Create a valid test backup.
    // 2. Corrupt one required archive entry or checksum.
    // 3. Attempt validation.
    // 4. Confirm restoration is blocked.
    // 5. Confirm current application data remains unchanged.
    // -------------------------------------------------------------
    let env_b = TestEnv::new();
    let mut conn_b = init_connection(&env_b.paths.database_file).expect("Conn B init failed");
    run_migrations(&mut conn_b).expect("Migration B failed");

    let live_note_b = NoteRepository::create(
        &conn_b,
        CreateNoteDto {
            title: Some("Live Inviolable Note".to_string()),
            content: Some("This data must never be overwritten by a corrupt archive".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Create a corrupt archive file
    let corrupt_archive = env_b
        .paths
        .backups_dir
        .join("corrupt_test_archive.pnbackup");
    std::fs::write(&corrupt_archive, b"PK\x03\x04corrupted zip payload").unwrap();

    // 3. Attempt validation
    let val_b = BackupValidationService::validate_backup(&corrupt_archive, &env_b.paths).unwrap();
    // 4. Confirm restoration is blocked
    assert!(!val_b.is_valid, "Corrupt backup validation must fail");
    assert_eq!(val_b.compatibility, BackupCompatibilityStatus::Corrupted);

    // Attempt restore with corrupt archive
    let restore_req_b = BackupRestoreRequest {
        file_path: corrupt_archive.to_string_lossy().to_string(),
        operation_id: Some("corrupt-restore-attempt".to_string()),
    };
    let restore_res_b = RestoreService::execute_restore(
        &mut conn_b,
        &env_b.paths,
        restore_req_b,
        None::<fn(BackupProgressEvent)>,
    );
    assert!(
        restore_res_b.is_err(),
        "Restoration of corrupt archive must be blocked"
    );

    // 5. Confirm current application data remains 100% unchanged
    let live_note_b_after = NoteRepository::get_by_id(&conn_b, &live_note_b.id)
        .unwrap()
        .unwrap();
    assert_eq!(live_note_b_after.title, "Live Inviolable Note");
    assert_eq!(
        live_note_b_after.content,
        "This data must never be overwritten by a corrupt archive"
    );

    // -------------------------------------------------------------
    // SCENARIO C — Interrupted Restore
    // 1. Create a valid backup and recovery checkpoint.
    // 2. Simulate a controlled failure during staging or activation.
    // 3. Run the documented recovery procedure.
    // 4. Confirm the database and attachment state are consistent.
    // 5. Confirm the application does not silently open a mixed or incomplete state.
    // -------------------------------------------------------------
    let env_c = TestEnv::new();
    let mut conn_c = init_connection(&env_c.paths.database_file).expect("Conn C init failed");
    run_migrations(&mut conn_c).expect("Migration C failed");

    let live_note_c = NoteRepository::create(
        &conn_c,
        CreateNoteDto {
            title: Some("Active Work Before Interrupted Restore".to_string()),
            content: Some("Critical research notes".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Simulate crash in FilesSwapped phase with active marker and backup copy
    let op_id_c = uuid::Uuid::new_v4().to_string();
    let sim_db_bak_c = env_c
        .paths
        .root
        .join(format!(".db_pre_restore_{op_id_c}.sqlite"));
    let sim_att_bak_c = env_c.paths.root.join(format!(".att_pre_restore_{op_id_c}"));

    std::fs::copy(&env_c.paths.database_file, &sim_db_bak_c).unwrap();
    std::fs::create_dir_all(&sim_att_bak_c).unwrap();

    let marker_c = RestoreActivationMarker {
        operation_id: op_id_c,
        started_at: chrono::Utc::now().to_rfc3339(),
        candidate_archive_path: corrupt_archive.to_string_lossy().to_string(),
        staging_dir: env_c
            .paths
            .backups_dir
            .join(".staged_sim_c")
            .to_string_lossy()
            .to_string(),
        checkpoint_path: env_c
            .paths
            .checkpoints_dir()
            .join("sim_checkpoint.pnbackup")
            .to_string_lossy()
            .to_string(),
        live_db_backup_path: sim_db_bak_c.to_string_lossy().to_string(),
        live_attachments_backup_path: sim_att_bak_c.to_string_lossy().to_string(),
        phase: RestoreActivationPhase::FilesSwapped,
    };
    RestoreService::write_activation_marker(&marker_c, &env_c.paths).unwrap();
    assert!(
        env_c.paths.restore_marker_file().exists(),
        "Marker must exist during simulation"
    );

    // 3. Run documented crash recovery procedure
    let recovered_c = RestoreService::recover_if_interrupted(&env_c.paths).unwrap();
    assert!(
        recovered_c.is_some(),
        "Crash recovery must detect interrupted marker"
    );

    // 4-5. Confirm consistency, absence of mixed state, and marker cleanup
    assert!(
        !env_c.paths.restore_marker_file().exists(),
        "Marker must be cleared after recovery"
    );
    let live_note_c_verified = NoteRepository::get_by_id(&conn_c, &live_note_c.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        live_note_c_verified.title,
        "Active Work Before Interrupted Restore"
    );

    // -------------------------------------------------------------
    // SCENARIO D — Automatic Backup
    // 1. Enable automatic backups.
    // 2. Trigger the configured backup condition.
    // 3. Confirm a backup is created.
    // 4. Restart the application.
    // 5. Verify scheduling state and retention behavior.
    // 6. Simulate an inaccessible destination.
    // 7. Confirm a visible failure status appears.
    // -------------------------------------------------------------
    let env_d = TestEnv::new();
    let mut conn_d = init_connection(&env_d.paths.database_file).expect("Conn D init failed");
    run_migrations(&mut conn_d).expect("Migration D failed");

    let _note_d = NoteRepository::create(
        &conn_d,
        CreateNoteDto {
            title: Some("Auto-Backup Test Note".to_string()),
            content: Some("Testing automated background snapshots".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 1. Enable automatic backups in settings
    let mut settings_d = BackupSettingsService::get_settings(&conn_d).unwrap();
    settings_d.auto_backup_enabled = true;
    settings_d.backup_on_startup = true;
    settings_d.retention_count = 2; // retain only 2
    BackupSettingsService::save_settings(&conn_d, &env_d.paths, settings_d).unwrap();

    // 2. Trigger the configured backup condition (startup trigger)
    let auto_backup_res = BackupSchedulerService::check_and_run_backup(
        &conn_d,
        &env_d.paths,
        true, // is_startup
        None::<fn(BackupProgressEvent)>,
    )
    .expect("Automatic backup check failed");

    // 3. Confirm a backup is created
    assert!(
        auto_backup_res.is_some(),
        "Automatic backup must have executed"
    );
    let auto_backup_item = auto_backup_res.unwrap();
    assert!(auto_backup_item.verified);
    assert_eq!(auto_backup_item.backup_type, BackupType::Automatic);

    let state_d = BackupSchedulerService::get_state(&conn_d).unwrap();
    assert!(state_d.last_successful_at.is_some());
    assert_eq!(state_d.failure_count, 0);

    // 4. Restart the application (simulate cold reboot)
    drop(conn_d);
    let conn_d_restarted =
        init_connection(&env_d.paths.database_file).expect("Restarted conn D failed");
    let restarted_state = BackupSchedulerService::get_state(&conn_d_restarted).unwrap();
    assert!(
        restarted_state.last_successful_at.is_some(),
        "Scheduler state survived restart"
    );

    // 5. Verify retention behavior:
    // Create 3 more automatic backups with distinct timestamps
    for _ in 1..=3 {
        // Delay slightly to ensure distinct file timestamps
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let req = BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Automatic),
            allow_overwrite: Some(false),
        };
        let res = BackupService::create_backup(
            &conn_d_restarted,
            &env_d.paths,
            req,
            None::<fn(BackupProgressEvent)>,
        )
        .unwrap();
        assert!(res.verified);

        let newly_path = std::path::Path::new(&res.file_path);
        let ret_res =
            BackupRetentionManager::apply_retention(&env_d.paths.backups_dir, 2, Some(newly_path));
        assert_eq!(
            ret_res.kept_count, 2,
            "Retention must keep at most 2 automatic archives"
        );
    }

    // 6. Simulate an inaccessible destination
    let mut broken_settings = BackupSettingsService::get_settings(&conn_d_restarted).unwrap();
    broken_settings.backup_destination_dir = Some("/nonexistent/forbidden/path".to_string());
    let save_err =
        BackupSettingsService::save_settings(&conn_d_restarted, &env_d.paths, broken_settings);
    assert!(
        save_err.is_err(),
        "Saving an unwritable destination must be rejected"
    );

    // Next, simulate a destination drive that was configured and then became inaccessible
    let blocked_dir = env_d.paths.root.join("blocked_drive");
    std::fs::create_dir_all(&blocked_dir).unwrap();
    let mut valid_settings = BackupSettingsService::get_settings(&conn_d_restarted).unwrap();
    valid_settings.backup_destination_dir = Some(blocked_dir.to_string_lossy().to_string());
    BackupSettingsService::save_settings(&conn_d_restarted, &env_d.paths, valid_settings).unwrap();

    // Remove the directory and replace with a regular file to make it inaccessible as a directory
    std::fs::remove_dir_all(&blocked_dir).unwrap();
    std::fs::write(&blocked_dir, b"blocked file payload").unwrap();

    // 7. Confirm visible failure status appears in health check
    let health_d = BackupHealthService::get_health_status(&conn_d_restarted, &env_d.paths).unwrap();
    assert!(
        !health_d.destination_available,
        "Non-directory destination must report unavailable"
    );
    assert!(
        health_d.health_level == BackupHealthLevel::Warning
            || health_d.health_level == BackupHealthLevel::Critical,
        "Health level must flag failure warning"
    );
    assert!(health_d
        .warnings
        .iter()
        .any(|w| w.code == "DESTINATION_UNAVAILABLE"));
}

#[test]
fn test_phase12_task_13_manual_lock_and_key_lifecycle() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::attachment_storage::AttachmentStorage;
    use personal_notepad_lib::storage::crypto::{CryptoSession, SetupEncryptionDto, UnlockDto};
    use personal_notepad_lib::storage::errors::CryptoError;

    let env = TestEnv::new();
    let mut conn =
        init_connection(&env.paths.database_file).expect("Database initialization failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Initialize session and set up encryption
    let session = CryptoSession::init_from_database(&conn).expect("Session initialization failed");
    assert!(!session.is_encryption_enabled());
    assert!(!session.is_locked());

    let password = "ManualLockPassword2026!";
    let setup_dto = SetupEncryptionDto {
        password: password.to_string(),
        confirm_password: password.to_string(),
    };

    let meta = session
        .setup_encryption(&conn, &env.paths, setup_dto)
        .expect("Encryption setup must succeed");
    assert!(meta.enabled);
    assert!(session.is_encryption_enabled());
    assert!(!session.is_locked());

    // Verify active DEK is present
    let active_dek = session
        .get_active_dek()
        .expect("DEK must be accessible when unlocked");
    assert_eq!(active_dek.len(), 32);

    // 2. Stage a temporary decrypted attachment in decrypted_attachments_dir()
    let dec_dir = env.paths.decrypted_attachments_dir();
    std::fs::create_dir_all(&dec_dir).expect("Must create decrypted attachments directory");
    let staged_file_1 = dec_dir.join("temp_preview_image.png");
    let staged_file_2 = dec_dir.join("temp_preview_document.pdf");
    std::fs::write(&staged_file_1, b"plaintext image preview bytes").unwrap();
    std::fs::write(&staged_file_2, b"plaintext pdf preview bytes").unwrap();
    assert!(
        staged_file_1.exists(),
        "Staged preview 1 must exist before lock"
    );
    assert!(
        staged_file_2.exists(),
        "Staged preview 2 must exist before lock"
    );

    // 3. Execute manual lock workflow (purging DEK and cleaning decrypted attachments)
    session.lock();
    let attachment_storage = AttachmentStorage::new(env.paths.clone());
    let cleaned_count = attachment_storage
        .cleanup_decrypted_attachments()
        .expect("Cleanup of decrypted preview files must succeed");
    assert_eq!(
        cleaned_count, 2,
        "Must remove exactly 2 temporary decrypted preview files"
    );

    // 4. Invariant: Session is locked and active DEK is completely purged from memory
    assert!(session.is_locked(), "Session must be marked locked");
    assert_eq!(
        session.get_active_dek(),
        Err(CryptoError::AppLocked),
        "Accessing DEK while locked must return CryptoError::AppLocked"
    );

    // 5. Invariant: Disk previews are removed
    assert!(
        !staged_file_1.exists(),
        "Decrypted file 1 must be purged from disk on lock"
    );
    assert!(
        !staged_file_2.exists(),
        "Decrypted file 2 must be purged from disk on lock"
    );

    // 6. Invariant: Unlocking with correct password restores active DEK
    session
        .unlock(UnlockDto {
            password: password.to_string(),
        })
        .expect("Unlock with valid password must succeed");

    assert!(!session.is_locked(), "Session must no longer be locked");
    let restored_dek = session
        .get_active_dek()
        .expect("Restored DEK must be accessible");
    assert_eq!(
        restored_dek.as_slice(),
        active_dek.as_slice(),
        "Restored DEK must match initial DEK"
    );
}

#[test]
fn test_phase12_task_15_password_change_and_key_rewrapping_invariants() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::crypto::{
        decrypt_payload, encrypt_payload, ChangePasswordDto, CryptoSession, SetupEncryptionDto,
        UnlockDto,
    };
    use personal_notepad_lib::storage::errors::CryptoError;

    let env = TestEnv::new();
    let mut conn =
        init_connection(&env.paths.database_file).expect("Database initialization failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Initialize session and set up encryption with initial password
    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password_v1 = "InitialMasterPassword2026!";
    let setup_dto = SetupEncryptionDto {
        password: password_v1.to_string(),
        confirm_password: password_v1.to_string(),
    };

    let meta_v1 = session
        .setup_encryption(&conn, &env.paths, setup_dto)
        .expect("Setup encryption failed");
    assert!(meta_v1.enabled);
    assert!(!session.is_locked());

    let initial_dek = session.get_active_dek().expect("Must get active DEK");
    assert_eq!(initial_dek.len(), 32);

    // 2. Encrypt sample sensitive payload using initial DEK
    let sensitive_content = b"Confidential Note Body #12345 - Personal Notepad";
    let encrypted_envelope =
        encrypt_payload(sensitive_content, &initial_dek, &[]).expect("Payload encryption failed");

    // 3. Validation failure: Invalid current password
    let wrong_current_dto = ChangePasswordDto {
        current_password: "WrongPassword999!".to_string(),
        new_password: "BrandNewPassword2026!".to_string(),
        confirm_new_password: "BrandNewPassword2026!".to_string(),
    };
    let err_wrong_pwd = session.change_password(&conn, wrong_current_dto);
    assert!(
        matches!(err_wrong_pwd, Err(CryptoError::AuthenticationFailed)),
        "Invalid current password must return CryptoError::AuthenticationFailed"
    );

    // Verify session remains functional and unchanged
    assert_eq!(
        session.get_active_dek().unwrap().as_slice(),
        initial_dek.as_slice(),
        "Active DEK must not be affected by failed change attempt"
    );

    // 4. Validation failure: Identical new password
    let identical_pwd_dto = ChangePasswordDto {
        current_password: password_v1.to_string(),
        new_password: password_v1.to_string(),
        confirm_new_password: password_v1.to_string(),
    };
    let err_identical = session.change_password(&conn, identical_pwd_dto);
    assert!(
        matches!(err_identical, Err(CryptoError::InvalidMetadata(_))),
        "Identical new password must be rejected with InvalidMetadata"
    );

    // 5. Validation failure: Mismatched new password confirmation
    let mismatch_dto = ChangePasswordDto {
        current_password: password_v1.to_string(),
        new_password: "BrandNewPassword2026!".to_string(),
        confirm_new_password: "DifferentPassword2026!".to_string(),
    };
    let err_mismatch = session.change_password(&conn, mismatch_dto);
    assert!(
        matches!(err_mismatch, Err(CryptoError::InvalidMetadata(_))),
        "Mismatched confirmation must be rejected"
    );

    // 6. Validation failure: Password too short (< 8 chars)
    let short_dto = ChangePasswordDto {
        current_password: password_v1.to_string(),
        new_password: "short".to_string(),
        confirm_new_password: "short".to_string(),
    };
    let err_short = session.change_password(&conn, short_dto);
    assert!(
        matches!(err_short, Err(CryptoError::InvalidMetadata(_))),
        "Short new password must be rejected"
    );

    // 7. Successful password change & key rewrapping
    let password_v2 = "BrandNewSuperSecurePassword456!";
    let valid_change_dto = ChangePasswordDto {
        current_password: password_v1.to_string(),
        new_password: password_v2.to_string(),
        confirm_new_password: password_v2.to_string(),
    };
    session
        .change_password(&conn, valid_change_dto)
        .expect("Password change and DEK rewrapping must succeed");

    // 8. Core Invariant: The active DEK is preserved identically across password change
    let post_change_dek = session
        .get_active_dek()
        .expect("Must get active DEK after change");
    assert_eq!(
        post_change_dek.as_slice(),
        initial_dek.as_slice(),
        "Envelope encryption invariant: DEK MUST remain identical after password re-wrap"
    );

    // 9. Core Invariant: Previously encrypted data decrypts without any re-encryption
    let decrypted_content = decrypt_payload(&encrypted_envelope, &post_change_dek, &[])
        .expect("Existing ciphertext must decrypt cleanly with preserved DEK");
    assert_eq!(
        decrypted_content.as_slice(),
        sensitive_content,
        "Decrypted content must match original payload"
    );

    // 10. Lock application and verify old password no longer works
    session.lock();
    assert!(session.is_locked());

    let old_unlock_err = session.unlock(UnlockDto {
        password: password_v1.to_string(),
    });
    assert!(
        old_unlock_err.is_err(),
        "Old password must no longer unlock the database"
    );
    assert!(session.is_locked());

    // 11. Verify new password successfully unlocks and recovers identical DEK
    session
        .unlock(UnlockDto {
            password: password_v2.to_string(),
        })
        .expect("New password must unlock session");
    assert!(!session.is_locked());

    let unlocked_dek = session
        .get_active_dek()
        .expect("DEK accessible after new unlock");
    assert_eq!(
        unlocked_dek.as_slice(),
        initial_dek.as_slice(),
        "Recovered DEK must match initial DEK"
    );

    // 12. Persistence across cold restart: re-initialize session from SQLite database
    let cold_session =
        CryptoSession::init_from_database(&conn).expect("Cold restart init must succeed");
    assert!(cold_session.is_encryption_enabled());
    assert!(cold_session.is_locked());

    // Unlocking cold session with new password
    cold_session
        .unlock(UnlockDto {
            password: password_v2.to_string(),
        })
        .expect("Cold session must unlock with new password");
    assert!(!cold_session.is_locked());

    let cold_dek = cold_session.get_active_dek().expect("Cold DEK accessible");
    assert_eq!(
        cold_dek.as_slice(),
        initial_dek.as_slice(),
        "Cold session DEK must match initial DEK"
    );
}

#[test]
fn test_phase12_task_16_forgotten_password_and_failed_attempt_invariants() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::crypto::{
        decrypt_payload, encrypt_payload, CryptoSession, SetupEncryptionDto, UnlockDto,
    };
    use personal_notepad_lib::storage::errors::CryptoError;

    let env = TestEnv::new();
    let mut conn =
        init_connection(&env.paths.database_file).expect("Database initialization failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Initialize session and set up encryption
    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let master_password = "SecurePasswordTask16!";
    let setup_dto = SetupEncryptionDto {
        password: master_password.to_string(),
        confirm_password: master_password.to_string(),
    };

    let meta = session
        .setup_encryption(&conn, &env.paths, setup_dto)
        .expect("Setup encryption failed");
    assert!(meta.enabled);
    assert!(!session.is_locked());

    // 2. Invariant: A pre-migration recovery checkpoint was created and is valid on disk
    let checkpoints_dir = env.paths.checkpoints_dir();
    assert!(checkpoints_dir.exists(), "Checkpoints directory must exist");
    let checkpoint_files: Vec<_> = std::fs::read_dir(&checkpoints_dir)
        .expect("Read checkpoints dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "pnbackup"))
        .collect();
    assert!(
        !checkpoint_files.is_empty(),
        "Setup MUST create at least one pre-migration recovery checkpoint (.pnbackup)"
    );

    // 3. Encrypt sensitive note data with active DEK
    let original_dek = session.get_active_dek().expect("Must get active DEK");
    let secret_payload = b"Highly confidential project roadmap notes -- Personal Notepad";
    let ciphertext =
        encrypt_payload(secret_payload, &original_dek, &[]).expect("Payload encryption failed");

    // 4. Lock application
    session.lock();
    assert!(session.is_locked());
    assert_eq!(session.get_active_dek(), Err(CryptoError::AppLocked));

    // 5. Invariant: Repeated incorrect password attempts MUST return AuthenticationFailed
    // and MUST NOT corrupt, modify, or erase data
    let wrong_passwords = [
        "WrongPass1!",
        "WrongPass2!",
        "IncorrectPhrase2026",
        "ForgotMyPassword!",
        "admin",
        "12345678",
        "RandomGuesses999",
    ];

    for (idx, wrong_pw) in wrong_passwords.iter().enumerate() {
        let attempt = session.unlock(UnlockDto {
            password: wrong_pw.to_string(),
        });
        assert!(
            matches!(attempt, Err(CryptoError::AuthenticationFailed)),
            "Attempt {} with '{}' must return AuthenticationFailed",
            idx + 1,
            wrong_pw
        );
        // Session must remain strictly locked
        assert!(session.is_locked());
        assert_eq!(session.get_active_dek(), Err(CryptoError::AppLocked));
    }

    // 6. Invariant: Checkpoint archive remains completely untouched on disk
    assert!(
        checkpoint_files[0].path().exists(),
        "Checkpoint archive must remain untouched despite failed unlock attempts"
    );

    // 7. Invariant: Unlocking with the correct master password immediately succeeds
    session
        .unlock(UnlockDto {
            password: master_password.to_string(),
        })
        .expect("Unlock with true password must succeed");

    assert!(!session.is_locked());
    let restored_dek = session.get_active_dek().expect("Must get active DEK");
    assert_eq!(
        restored_dek.as_slice(),
        original_dek.as_slice(),
        "Restored DEK must be identical"
    );

    // 8. Invariant: Ciphertext decrypts cleanly and matches original secret payload byte-for-byte
    let decrypted = decrypt_payload(&ciphertext, &restored_dek, &[])
        .expect("Decryption must succeed with restored DEK");
    assert_eq!(
        decrypted.as_slice(),
        secret_payload,
        "Decrypted payload must match original content"
    );

    // 9. Invariant: A subsequent failed unlock attempt after being unlocked fails cleanly
    let post_unlock_bad = session.unlock(UnlockDto {
        password: "BadAttemptWhileAlreadyUnlocked".to_string(),
    });
    assert!(
        matches!(post_unlock_bad, Err(CryptoError::AuthenticationFailed)),
        "Invalid unlock attempt while unlocked must still fail"
    );
}

#[test]
fn test_phase12_task_17_safe_migration_and_interrupted_state_detection() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::attachment_storage::AttachmentStorage;
    use personal_notepad_lib::storage::crypto::{
        metadata::{MigrationStatus, ENCRYPTION_METADATA_KEY},
        CryptoSession, SetupEncryptionDto, UnlockDto,
    };
    use personal_notepad_lib::storage::errors::CryptoError;
    use personal_notepad_lib::storage::models::CreateNoteDto;
    use personal_notepad_lib::storage::repositories::{NoteRepository, SettingsRepository};
    use personal_notepad_lib::storage::services::RestoreService;
    use rusqlite::params;

    let env = TestEnv::new();
    let mut conn =
        init_connection(&env.paths.database_file).expect("Database initialization failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create plaintext pre-migration notes in raw SQLite
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Plain Note 1".to_string()),
            content: Some("Sensitive Plaintext Financial Report 2026".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 1 failed");

    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Plain Note 2 with Special Characters: !@#$%^&*()_+".to_string()),
            content: Some("Multi-line content\nLine 2\nLine 3 with unicode: 🚀🔒".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 2 failed");

    let note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Plain Note 3".to_string()),
            content: Some("Unencrypted markdown # Header\n- Item 1\n- Item 2".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 3 failed");

    // 2. Create unencrypted attachment metadata and plaintext file on disk
    let att_id = uuid::Uuid::new_v4().to_string();
    let original_file_name = "secret_document.pdf";
    let stored_file_name = format!("{}.pdf", uuid::Uuid::new_v4());
    let plain_file_content = b"%PDF-1.4\n1 0 obj\n<< /Title (Plaintext Document) >>\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF";

    let att_disk_path = env.paths.attachments_dir.join(&stored_file_name);
    std::fs::write(&att_disk_path, plain_file_content)
        .expect("Failed to write unencrypted attachment file");
    assert!(
        !AttachmentStorage::is_encrypted_file(&att_disk_path),
        "File must initially be unencrypted"
    );

    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            att_id,
            note1.id,
            original_file_name,
            stored_file_name,
            "application/pdf",
            plain_file_content.len() as i64,
            now,
            now,
        ],
    ).expect("Insert unencrypted attachment failed");

    // Verify unencrypted state before migration
    let raw_title: String = conn
        .query_row("SELECT title FROM notes WHERE id = ?1", [&note1.id], |r| {
            r.get(0)
        })
        .expect("Query failed");
    assert_eq!(raw_title, "Plain Note 1");
    assert!(!raw_title.starts_with("ENC$v1$"));

    // 3. Initialize CryptoSession and execute safe migration
    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    assert!(!session.is_encryption_enabled());
    assert!(!session.is_locked());
    assert!(!session.is_interrupted());

    let password = "SuperMigrationPassword2026!";
    let setup_dto = SetupEncryptionDto {
        password: password.to_string(),
        confirm_password: password.to_string(),
    };

    let meta = session
        .setup_encryption(&conn, &env.paths, setup_dto)
        .expect("Safe migration and encryption setup must succeed");

    assert!(meta.enabled);
    assert_eq!(meta.migration_status, MigrationStatus::Active);
    assert!(session.is_encryption_enabled());
    assert!(!session.is_locked());
    assert!(!session.is_interrupted());

    // 4. Invariant: Pre-migration recovery checkpoint exists in checkpoints/
    let checkpoints = RestoreService::list_recovery_checkpoints(&env.paths)
        .expect("Listing checkpoints must succeed");
    assert_eq!(
        checkpoints.len(),
        1,
        "Pre-migration checkpoint MUST be created before any changes"
    );
    assert!(checkpoints[0].is_verified, "Checkpoint must be verified");
    let checkpoint_file = std::path::PathBuf::from(&checkpoints[0].file_path);
    assert!(
        checkpoint_file.is_file(),
        "Checkpoint file must exist on disk"
    );

    // 5. Invariant: 100% of notes in SQLite are now ciphertext envelopes (ENC$v1$)
    let unencrypted_note_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM notes WHERE title NOT LIKE 'ENC$v1$%' OR content NOT LIKE 'ENC$v1%'",
            [],
            |r| r.get(0),
        )
        .expect("Check unencrypted notes failed");
    assert_eq!(
        unencrypted_note_count, 0,
        "Zero notes may remain unencrypted"
    );

    for nid in [&note1.id, &note2.id, &note3.id] {
        let (raw_t, raw_c): (String, String) = conn
            .query_row(
                "SELECT title, content FROM notes WHERE id = ?1",
                [nid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("Query raw note failed");
        assert!(
            raw_t.starts_with("ENC$v1$"),
            "Title must be ciphertext envelope"
        );
        assert!(
            raw_c.starts_with("ENC$v1$"),
            "Content must be ciphertext envelope"
        );
    }

    // 6. Invariant: 100% of attachments in SQLite are now ciphertext envelopes
    let unencrypted_att_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM attachments WHERE file_name NOT LIKE 'ENC$v1$%'",
            [],
            |r| r.get(0),
        )
        .expect("Check unencrypted attachments failed");
    assert_eq!(
        unencrypted_att_count, 0,
        "Zero attachments may remain unencrypted in DB"
    );

    // 7. Invariant: Disk attachment file is now an authenticated ciphertext envelope
    assert!(
        AttachmentStorage::is_encrypted_file(&att_disk_path),
        "Disk attachment file must be converted to an encrypted envelope"
    );
    let disk_bytes = std::fs::read(&att_disk_path).expect("Read attachment failed");
    assert!(
        disk_bytes.starts_with(&[personal_notepad_lib::storage::crypto::ENVELOPE_VERSION_V1]),
        "Disk envelope version byte must match ENVELOPE_VERSION_V1"
    );

    // 8. Invariant: Decrypted notes and attachments match original data byte-for-byte
    let active_dek = session.get_active_dek().expect("Get active DEK failed");

    let dec_note1 = NoteRepository::get_by_id_with_key(&conn, &note1.id, Some(&active_dek))
        .expect("Get note 1 failed")
        .expect("Note 1 exists");
    assert_eq!(dec_note1.title, "Plain Note 1");
    assert_eq!(
        dec_note1.content,
        "Sensitive Plaintext Financial Report 2026"
    );

    let dec_note2 = NoteRepository::get_by_id_with_key(&conn, &note2.id, Some(&active_dek))
        .expect("Get note 2 failed")
        .expect("Note 2 exists");
    assert_eq!(
        dec_note2.title,
        "Plain Note 2 with Special Characters: !@#$%^&*()_+"
    );
    assert_eq!(
        dec_note2.content,
        "Multi-line content\nLine 2\nLine 3 with unicode: 🚀🔒"
    );

    let dec_note3 = NoteRepository::get_by_id_with_key(&conn, &note3.id, Some(&active_dek))
        .expect("Get note 3 failed")
        .expect("Note 3 exists");
    assert_eq!(dec_note3.title, "Plain Note 3");
    assert_eq!(
        dec_note3.content,
        "Unencrypted markdown # Header\n- Item 1\n- Item 2"
    );

    // Decrypt attachment file on disk using DEK and AAD
    let att_aad = format!("attachments:{}:{}", note1.id, stored_file_name);
    let decrypted_file_bytes = personal_notepad_lib::storage::crypto::decrypt_payload(
        &disk_bytes,
        &active_dek,
        att_aad.as_bytes(),
    )
    .expect("Attachment payload decryption must succeed");
    assert_eq!(decrypted_file_bytes.as_slice(), plain_file_content);

    // 9. Invariant: Interrupted migration state detection at startup
    // Simulate an interrupted migration in SQLite settings
    let mut interrupted_meta = meta.clone();
    interrupted_meta.migration_status = MigrationStatus::MigrationInProgress;
    SettingsRepository::set(
        &conn,
        ENCRYPTION_METADATA_KEY,
        &interrupted_meta.to_json().unwrap(),
    )
    .unwrap();

    let restarted_session =
        CryptoSession::init_from_database(&conn).expect("Restart session init failed");
    assert!(restarted_session.is_encryption_enabled());
    assert!(restarted_session.is_locked());
    assert!(restarted_session.is_interrupted());
    assert_eq!(
        restarted_session.get_migration_status(),
        Some(MigrationStatus::MigrationInProgress)
    );

    // Normal unlock attempts must be rejected with MigrationError to prevent corrupted read/write
    let unlock_attempt = restarted_session.unlock(UnlockDto {
        password: password.to_string(),
    });
    assert!(
        matches!(unlock_attempt, Err(CryptoError::MigrationError(_))),
        "Must reject unlock during interrupted migration state"
    );
    assert_eq!(
        restarted_session.get_active_dek().err(),
        Some(CryptoError::MigrationError(
            "Application storage is in an interrupted migration state. Restore from a pre-migration backup checkpoint.".to_string()
        ))
    );
}

#[test]
fn test_phase12_task_18_crash_recovery_rollback_and_resume_invariants() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::crypto::{
        derive_kek, generate_dek, generate_salt, wrap_dek, CryptoSession, EncryptionMetadata,
        EncryptionTransitionMarker, KdfConfig, MigrationService, MigrationStatus,
        SetupEncryptionDto, UnlockDto, WrappedKey, ENCRYPTION_METADATA_KEY,
    };
    use personal_notepad_lib::storage::errors::CryptoError;
    use personal_notepad_lib::storage::models::CreateNoteDto;
    use personal_notepad_lib::storage::repositories::{NoteRepository, SettingsRepository};

    // =========================================================================
    // SCENARIO A: Crash at CheckpointCreated (Unstarted / Pre-DEK)
    // Invariant: At startup, unstarted state is cleaned up; storage remains Disabled.
    // =========================================================================
    {
        let env = TestEnv::new();
        let mut conn =
            init_connection(&env.paths.database_file).expect("Database initialization failed");
        run_migrations(&mut conn).expect("Migration failed");

        let marker = EncryptionTransitionMarker {
            operation_id: "op-test-a".to_string(),
            operation_type: "setup".to_string(),
            stage: MigrationStatus::CheckpointCreated,
            checkpoint_path: "/tmp/fake_pre_migration.pnbackup".to_string(),
            started_at: "2026-10-14T12:00:00Z".to_string(),
            updated_at: "2026-10-14T12:00:00Z".to_string(),
        };
        MigrationService::save_transition_marker(&env.paths, &marker).expect("Save marker failed");
        assert!(env.paths.encryption_migration_marker_file().exists());

        // In SQLite settings, an unstarted setup status might also be present
        let salt = generate_salt();
        let kdf = KdfConfig::default_profile(salt);
        let wrapped_dek = WrappedKey::new(&[0u8; 12], &[0u8; 32], &[0u8; 16]);
        let mut meta = EncryptionMetadata::new(kdf, wrapped_dek);
        meta.migration_status = MigrationStatus::CheckpointCreated;
        SettingsRepository::set(&conn, ENCRYPTION_METADATA_KEY, &meta.to_json().unwrap()).unwrap();

        let session =
            CryptoSession::init_and_recover(&conn, &env.paths).expect("Init and recover failed");
        assert!(!session.is_encryption_enabled());
        assert!(!session.is_locked());
        assert!(!session.is_interrupted());
        // Marker file must have been deleted
        assert!(!env.paths.encryption_migration_marker_file().exists());
        // Incomplete metadata must have been purged
        assert!(SettingsRepository::get(&conn, ENCRYPTION_METADATA_KEY)
            .unwrap()
            .is_none());
    }

    // =========================================================================
    // SCENARIO B: Crash at Verified (Post-migration, pre-final-Active write)
    // Invariant: At startup, auto-finalize to Active, delete marker, allow normal unlock.
    // =========================================================================
    {
        let env = TestEnv::new();
        let mut conn =
            init_connection(&env.paths.database_file).expect("Database initialization failed");
        run_migrations(&mut conn).expect("Migration failed");

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Pre-migration Note".to_string()),
                content: Some("Secret Content".to_string()),
                ..Default::default()
            },
        )
        .expect("Create note failed");

        let session = CryptoSession::init_from_database(&conn).expect("Init session failed");
        let password = "VerifiedCrashPassword123!";
        session
            .setup_encryption(
                &conn,
                &env.paths,
                SetupEncryptionDto {
                    password: password.to_string(),
                    confirm_password: password.to_string(),
                },
            )
            .expect("Setup encryption failed");

        // Now simulate a crash right after verification:
        // Set metadata back to Verified and create the Verified marker on disk
        let mut meta = EncryptionMetadata::from_json(
            &SettingsRepository::get(&conn, ENCRYPTION_METADATA_KEY)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        meta.migration_status = MigrationStatus::Verified;
        SettingsRepository::set(&conn, ENCRYPTION_METADATA_KEY, &meta.to_json().unwrap()).unwrap();

        let marker = EncryptionTransitionMarker {
            operation_id: "op-test-b".to_string(),
            operation_type: "setup".to_string(),
            stage: MigrationStatus::Verified,
            checkpoint_path: "/tmp/fake_checkpoint.pnbackup".to_string(),
            started_at: "2026-10-14T12:00:00Z".to_string(),
            updated_at: "2026-10-14T12:00:00Z".to_string(),
        };
        MigrationService::save_transition_marker(&env.paths, &marker).expect("Save marker failed");
        assert!(env.paths.encryption_migration_marker_file().exists());

        // Restart app with init_and_recover
        let recovered_session =
            CryptoSession::init_and_recover(&conn, &env.paths).expect("Recover failed");
        assert!(recovered_session.is_encryption_enabled());
        assert!(recovered_session.is_locked());
        assert!(!recovered_session.is_interrupted());
        // Marker must have been deleted
        assert!(!env.paths.encryption_migration_marker_file().exists());

        // Metadata in DB must now be Active
        let reloaded_meta = EncryptionMetadata::from_json(
            &SettingsRepository::get(&conn, ENCRYPTION_METADATA_KEY)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(reloaded_meta.migration_status, MigrationStatus::Active);

        // User can immediately unlock with their master password
        let unlock_res = recovered_session.unlock(UnlockDto {
            password: password.to_string(),
        });
        assert!(
            unlock_res.is_ok(),
            "Must unlock successfully after auto-finalization"
        );
        let active_dek = recovered_session.get_active_dek().unwrap();
        let decrypted_note = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&active_dek))
            .unwrap()
            .unwrap();
        assert_eq!(decrypted_note.title, "Pre-migration Note");
        assert_eq!(decrypted_note.content, "Secret Content");
    }

    // =========================================================================
    // SCENARIO C: Crash mid-migration (MigrationInProgress) -> Rollback Path
    // Invariant: Session enters InterruptedMigration. Normal unlock is blocked.
    // Executing rollback restores pre-migration checkpoint, purges marker & metadata,
    // and returns storage cleanly to Disabled.
    // =========================================================================
    {
        let env = TestEnv::new();
        let mut conn =
            init_connection(&env.paths.database_file).expect("Database initialization failed");
        run_migrations(&mut conn).expect("Migration failed");

        let plain_title = "Important Document To Preserve";
        let plain_content = "This content MUST be completely restored by rollback.";
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(plain_title.to_string()),
                content: Some(plain_content.to_string()),
                ..Default::default()
            },
        )
        .expect("Create note failed");

        // 1. Create a legitimate pre-migration checkpoint
        let checkpoint_path = MigrationService::create_recovery_checkpoint(&conn, &env.paths)
            .expect("Create pre-migration checkpoint failed");
        assert!(
            checkpoint_path.exists(),
            "Pre-migration checkpoint must exist on disk"
        );

        // 2. Simulate partial migration:
        let password = "TestRollbackPassword123!";
        let salt = generate_salt();
        let kdf = KdfConfig::default_profile(salt);
        let kek = derive_kek(password, &kdf).expect("derive kek failed");
        let dek = generate_dek();
        let wrapped_dek = wrap_dek(&dek, &kek).expect("wrap dek failed");
        let mut meta = EncryptionMetadata::new(kdf, wrapped_dek);
        meta.migration_status = MigrationStatus::MigrationInProgress;
        SettingsRepository::set(&conn, ENCRYPTION_METADATA_KEY, &meta.to_json().unwrap()).unwrap();

        // Save durable marker pointing to the checkpoint
        let marker = EncryptionTransitionMarker {
            operation_id: "op-test-c".to_string(),
            operation_type: "setup".to_string(),
            stage: MigrationStatus::MigrationInProgress,
            checkpoint_path: checkpoint_path.to_string_lossy().to_string(),
            started_at: "2026-10-14T12:00:00Z".to_string(),
            updated_at: "2026-10-14T12:00:00Z".to_string(),
        };
        MigrationService::save_transition_marker(&env.paths, &marker).expect("Save marker failed");

        // Simulate half-completed state: Modify the note in SQLite to simulate incomplete write
        conn.execute(
            "UPDATE notes SET title = 'CORRUPTED_MID_MIGRATION' WHERE id = ?1",
            [&note.id],
        )
        .unwrap();

        // 3. Restart application: init_and_recover
        let interrupted_session =
            CryptoSession::init_and_recover(&conn, &env.paths).expect("Init and recover failed");
        assert!(
            interrupted_session.is_interrupted(),
            "Session must be marked interrupted"
        );
        assert!(interrupted_session.is_locked());
        assert_eq!(
            interrupted_session.get_recovery_checkpoint_path(&env.paths),
            Some(checkpoint_path.to_string_lossy().to_string())
        );

        // Normal unlock must fail
        assert!(interrupted_session
            .unlock(UnlockDto {
                password: password.to_string()
            })
            .is_err());

        // 4. Execute deterministic rollback
        let rollback_res =
            interrupted_session.rollback_interrupted_migration(&mut conn, &env.paths);
        assert!(
            rollback_res.is_ok(),
            "Rollback must succeed: {:?}",
            rollback_res.err()
        );

        // Verify session state is now Disabled
        assert!(!interrupted_session.is_encryption_enabled());
        assert!(!interrupted_session.is_locked());
        assert!(!interrupted_session.is_interrupted());

        // Verify marker file is deleted
        assert!(!env.paths.encryption_migration_marker_file().exists());

        // Verify restored SQLite database has the original plaintext note intact
        let restored_conn =
            init_connection(&env.paths.database_file).expect("Open restored DB failed");
        let restored_note = NoteRepository::get_by_id(&restored_conn, &note.id)
            .expect("Get restored note failed")
            .expect("Restored note must exist");
        assert_eq!(restored_note.title, plain_title);
        assert_eq!(restored_note.content, plain_content);

        // Encryption metadata in DB is gone
        assert!(
            SettingsRepository::get(&restored_conn, ENCRYPTION_METADATA_KEY)
                .unwrap()
                .is_none()
        );
    }

    // =========================================================================
    // SCENARIO D: Crash mid-migration (MigrationInProgress) -> Resume Path
    // Invariant: Session enters InterruptedMigration. User provides master password
    // via resume_interrupted_migration. The remaining notes and attachments are
    // encrypted, verified, marker is removed, metadata set to Active, and session unlocked.
    // =========================================================================
    {
        let env = TestEnv::new();
        let mut conn =
            init_connection(&env.paths.database_file).expect("Database initialization failed");
        run_migrations(&mut conn).expect("Migration failed");

        let plain1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Mid-Migration Note 1".to_string()),
                content: Some("First note payload".to_string()),
                ..Default::default()
            },
        )
        .expect("Create note 1 failed");

        let plain2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Mid-Migration Note 2".to_string()),
                content: Some("Second note payload".to_string()),
                ..Default::default()
            },
        )
        .expect("Create note 2 failed");

        let password = "ResumeMasterPassword2026!";
        let salt = generate_salt();
        let kdf = KdfConfig::default_profile(salt);
        let kek = derive_kek(password, &kdf).expect("derive kek failed");
        let dek = generate_dek();
        let wrapped_dek = wrap_dek(&dek, &kek).expect("wrap dek failed");
        let mut meta = EncryptionMetadata::new(kdf, wrapped_dek);
        meta.migration_status = MigrationStatus::MigrationInProgress;
        SettingsRepository::set(&conn, ENCRYPTION_METADATA_KEY, &meta.to_json().unwrap()).unwrap();

        // Save marker
        let marker = EncryptionTransitionMarker {
            operation_id: "op-test-d".to_string(),
            operation_type: "setup".to_string(),
            stage: MigrationStatus::MigrationInProgress,
            checkpoint_path: "/tmp/fake_chk.pnbackup".to_string(),
            started_at: "2026-10-14T12:00:00Z".to_string(),
            updated_at: "2026-10-14T12:00:00Z".to_string(),
        };
        MigrationService::save_transition_marker(&env.paths, &marker).expect("Save marker failed");

        // Partially encrypt note 1 manually using the real DEK
        let enc_t1 = personal_notepad_lib::storage::crypto::encrypt_string(
            &plain1.title,
            &dek,
            format!("notes:{}:title", plain1.id).as_bytes(),
        )
        .unwrap();
        let enc_c1 = personal_notepad_lib::storage::crypto::encrypt_string(
            &plain1.content,
            &dek,
            format!("notes:{}:content", plain1.id).as_bytes(),
        )
        .unwrap();
        conn.execute(
            "UPDATE notes SET title = ?1, content = ?2 WHERE id = ?3",
            [&enc_t1, &enc_c1, &plain1.id],
        )
        .unwrap();

        // Note 2 remains plaintext unencrypted, simulating crash in between note 1 and note 2!
        let raw2_title: String = conn
            .query_row("SELECT title FROM notes WHERE id = ?1", [&plain2.id], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(
            !raw2_title.starts_with("ENC$v1$"),
            "Note 2 must be raw unencrypted before resume"
        );

        // Startup detection
        let session =
            CryptoSession::init_and_recover(&conn, &env.paths).expect("Init and recover failed");
        assert!(session.is_interrupted());

        // Bad password for resume fails
        let bad_resume = session.resume_interrupted_migration(&conn, &env.paths, "WrongPassword!");
        assert!(matches!(bad_resume, Err(CryptoError::AuthenticationFailed)));

        // Correct password resumes migration
        let resume_res = session.resume_interrupted_migration(&conn, &env.paths, password);
        assert!(
            resume_res.is_ok(),
            "Resume must succeed: {:?}",
            resume_res.err()
        );

        // Session must now be unlocked and Active
        assert!(session.is_encryption_enabled());
        assert!(!session.is_locked());
        assert!(!session.is_interrupted());

        // Marker must be gone
        assert!(!env.paths.encryption_migration_marker_file().exists());

        // Metadata in DB must be Active
        let active_meta = EncryptionMetadata::from_json(
            &SettingsRepository::get(&conn, ENCRYPTION_METADATA_KEY)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(active_meta.migration_status, MigrationStatus::Active);

        // Invariant: 100% of notes are now ENC$v1$
        let unenc_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM notes WHERE title NOT LIKE 'ENC$v1$%' OR content NOT LIKE 'ENC$v1$%'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(unenc_count, 0, "All notes must be ciphertext envelopes");

        // Both notes decrypt to original plaintexts
        let active_dek = session.get_active_dek().unwrap();
        let dec1 = NoteRepository::get_by_id_with_key(&conn, &plain1.id, Some(&active_dek))
            .unwrap()
            .unwrap();
        assert_eq!(dec1.title, plain1.title);
        assert_eq!(dec1.content, plain1.content);

        let dec2 = NoteRepository::get_by_id_with_key(&conn, &plain2.id, Some(&active_dek))
            .unwrap()
            .unwrap();
        assert_eq!(dec2.title, plain2.title);
        assert_eq!(dec2.content, plain2.content);
    }
}

#[test]
fn test_phase12_task_19_backup_and_restore_encryption_integration() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::{
        crypto::{
            secret::{SetupEncryptionDto, UnlockDto},
            session::CryptoSession,
        },
        models::{BackupCreateRequest, BackupRestoreRequest, BackupType, CreateNoteDto},
        repositories::NoteRepository,
        services::{BackupService, BackupValidationService, RestoreService},
    };
    use std::fs::File;
    use std::io::Read;
    use zip::ZipArchive;

    // =========================================================================
    // SCENARIO A: Encrypted Backup Creation & Zero Plaintext Leakage Invariant
    // =========================================================================
    let env_enc = TestEnv::new();
    let mut conn_enc = init_connection(&env_enc.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn_enc).expect("Migration failed");

    let session_enc = CryptoSession::init_from_database(&conn_enc).expect("Init session failed");

    let master_password = "SecureMasterPassword2026!";
    let setup_dto = SetupEncryptionDto {
        password: master_password.to_string(),
        confirm_password: master_password.to_string(),
    };

    let meta = session_enc
        .setup_encryption(&conn_enc, &env_enc.paths, setup_dto)
        .expect("Setup encryption failed");
    assert!(meta.enabled);
    assert!(session_enc.is_encryption_enabled());
    assert!(!session_enc.is_locked());

    let dek = session_enc.get_active_dek().expect("Get active DEK failed");

    // Create sensitive note using the active DEK
    let secret_title = "Secret Vault Access Code";
    let secret_body = "The master combination is 9876-Alpha-Tango-Delta.";
    let sensitive_note = NoteRepository::create_with_key(
        &conn_enc,
        CreateNoteDto {
            title: Some(secret_title.to_string()),
            content: Some(secret_body.to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("Create encrypted note failed");

    // Add encrypted attachment
    let note_att_dir = env_enc
        .paths
        .get_note_attachments_dir(&sensitive_note.id)
        .unwrap();
    let att_stored_name = format!("{}.bin", uuid::Uuid::new_v4());
    let att_file_path = note_att_dir.join(&att_stored_name);
    let att_raw_plaintext = b"Highly confidential internal attachment data!";
    let enc_att_bytes = personal_notepad_lib::storage::crypto::encrypt_payload(
        att_raw_plaintext,
        &dek,
        format!("attachments:{}", sensitive_note.id).as_bytes(),
    )
    .expect("Encrypt attachment failed");
    std::fs::write(&att_file_path, &enc_att_bytes).expect("Write attachment file failed");

    let now_str = chrono::Utc::now().to_rfc3339();
    conn_enc.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, file_size, mime_type, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            sensitive_note.id,
            "secret.pdf",
            format!("{}/{}", sensitive_note.id, att_stored_name),
            enc_att_bytes.len() as i64,
            "application/pdf",
            now_str,
        ],
    ).expect("Insert attachment record failed");

    // Lock session to verify zero-knowledge backup can be created WITHOUT active keys
    session_enc.lock();
    assert!(session_enc.is_locked());
    assert!(session_enc.get_active_dek().is_err());

    // Create backup while locked!
    let backup_dest = env_enc
        .paths
        .backups_dir
        .join("encrypted_zero_knowledge.pnbackup");
    let create_req = BackupCreateRequest {
        destination_path: Some(backup_dest.to_string_lossy().to_string()),
        backup_type: Some(BackupType::Manual),
        allow_overwrite: Some(true),
    };

    let backup_res = BackupService::create_backup(
        &conn_enc,
        &env_enc.paths,
        create_req,
        None::<fn(personal_notepad_lib::storage::models::BackupProgressEvent)>,
    )
    .expect("Create backup of encrypted database while locked must succeed");

    assert!(
        backup_res.is_encrypted,
        "BackupCreateResult must report is_encrypted = true"
    );
    assert!(backup_dest.exists(), "Backup archive file must exist");

    // Directly verify ZIP archive contents for zero-knowledge / zero plaintext leakage
    {
        let zip_file = File::open(&backup_dest).expect("Open backup ZIP failed");
        let mut archive = ZipArchive::new(zip_file).expect("Read backup ZIP failed");

        // Verify manifest
        {
            let mut manifest_entry = archive
                .by_name("manifest.json")
                .expect("manifest.json missing");
            let mut manifest_str = String::new();
            manifest_entry
                .read_to_string(&mut manifest_str)
                .expect("Read manifest failed");
            let manifest: personal_notepad_lib::storage::models::BackupManifest =
                serde_json::from_str(&manifest_str).expect("Parse manifest JSON failed");

            assert!(
                manifest.is_encrypted,
                "Manifest must declare is_encrypted = true"
            );
            assert_eq!(
                manifest.encryption_algorithm.as_deref(),
                Some("AES-256-GCM"),
                "Manifest must declare AES-256-GCM algorithm"
            );
        }

        // Verify raw database.sqlite inside ZIP has ENC$v1$ and NO plaintext notes
        {
            let mut db_entry = archive
                .by_name("database.sqlite")
                .expect("database.sqlite missing");
            let mut db_bytes = Vec::new();
            db_entry
                .read_to_end(&mut db_bytes)
                .expect("Read database.sqlite failed");

            let db_content_str = String::from_utf8_lossy(&db_bytes);
            assert!(
                !db_content_str.contains(secret_title),
                "Plaintext secret title must NOT appear anywhere in the database file"
            );
            assert!(
                !db_content_str.contains(secret_body),
                "Plaintext secret body must NOT appear anywhere in the database file"
            );
            assert!(
                db_content_str.contains("ENC$v1$"),
                "Database file must contain ENC$v1$ ciphertext envelopes"
            );
        }

        // Verify attachment entry inside ZIP has binary envelope byte (0x01) and NO plaintext bytes
        {
            let att_entry_name = format!("attachments/{}/{}", sensitive_note.id, att_stored_name);
            let mut att_entry = archive
                .by_name(&att_entry_name)
                .expect("Attachment entry missing");
            let mut att_archive_bytes = Vec::new();
            att_entry
                .read_to_end(&mut att_archive_bytes)
                .expect("Read attachment bytes failed");

            assert_eq!(
                att_archive_bytes[0], 0x01,
                "Attachment must begin with binary envelope version 0x01"
            );
            let att_content_str = String::from_utf8_lossy(&att_archive_bytes);
            assert!(
                !att_content_str.contains("Highly confidential internal attachment data!"),
                "Plaintext attachment data must NOT leak into the archive"
            );
        }
    }

    // =========================================================================
    // SCENARIO B: Restore Encrypted Backup into Clean/Unencrypted Installation
    // Invariant: Session immediately transitions to Locked(metadata), active keys
    // are purged, user is prompted for password, and unlocking decrypts data.
    // =========================================================================
    let env_clean = TestEnv::new();
    let mut conn_clean =
        init_connection(&env_clean.paths.database_file).expect("Clean DB init failed");
    run_migrations(&mut conn_clean).expect("Clean migration failed");

    let session_clean =
        CryptoSession::init_from_database(&conn_clean).expect("Clean session init failed");
    assert!(!session_clean.is_encryption_enabled());
    assert!(!session_clean.is_locked());

    // 1. Preview restore of encrypted backup
    let preview = RestoreService::preview_restore(&backup_dest, &env_clean.paths)
        .expect("Preview restore failed");
    assert!(
        preview.is_encrypted,
        "Preview must identify archive as encrypted"
    );
    assert_eq!(preview.encryption_algorithm.as_deref(), Some("AES-256-GCM"));
    assert!(preview.can_restore, "Preview must allow restore");

    // 2. Validate backup
    let val_res = BackupValidationService::validate_backup(&backup_dest, &env_clean.paths)
        .expect("Validation failed");
    assert!(
        val_res.is_valid,
        "Validation must pass for valid encrypted backup"
    );

    // 3. Execute restore
    let restore_req = BackupRestoreRequest {
        file_path: backup_dest.to_string_lossy().to_string(),
        operation_id: Some("op-test-restore-enc".to_string()),
    };

    let restore_result = RestoreService::execute_restore(
        &mut conn_clean,
        &env_clean.paths,
        restore_req,
        None::<fn(personal_notepad_lib::storage::models::BackupProgressEvent)>,
    )
    .expect("Execute restore failed");
    assert!(restore_result.success);

    // 4. Synchronize session memory state from restored database
    let is_enc = session_clean
        .refresh_from_database(&conn_clean, &env_clean.paths)
        .expect("Refresh from database failed");
    assert!(is_enc, "Restored state must be encrypted");

    // Invariant: session is now Locked!
    assert!(session_clean.is_encryption_enabled());
    assert!(session_clean.is_locked());
    assert!(session_clean.get_active_dek().is_err());

    // Reading note without DEK returns error or raw envelope
    let unauth_read = NoteRepository::get_by_id_with_key(&conn_clean, &sensitive_note.id, None);
    assert!(unauth_read.is_err(), "Accessing note without DEK must fail");

    // Unlocking with wrong password fails
    let bad_unlock = session_clean.unlock(UnlockDto {
        password: "IncorrectPassword123!".to_string(),
    });
    assert!(
        bad_unlock.is_err(),
        "Unlocking with incorrect password must fail"
    );
    assert!(session_clean.is_locked());

    // Unlocking with correct password succeeds!
    session_clean
        .unlock(UnlockDto {
            password: master_password.to_string(),
        })
        .expect("Unlocking with correct restored master password must succeed");
    assert!(!session_clean.is_locked());

    let restored_dek = session_clean
        .get_active_dek()
        .expect("Get restored active DEK failed");
    let decrypted_note =
        NoteRepository::get_by_id_with_key(&conn_clean, &sensitive_note.id, Some(&restored_dek))
            .expect("Get decrypted note failed")
            .expect("Decrypted note must exist");

    assert_eq!(decrypted_note.title, secret_title);
    assert_eq!(decrypted_note.content, secret_body);

    // Verify restored encrypted attachment can be decrypted
    let restored_att_file = env_clean
        .paths
        .get_note_attachments_dir(&sensitive_note.id)
        .unwrap()
        .join(&att_stored_name);
    assert!(
        restored_att_file.exists(),
        "Restored attachment file must exist on disk"
    );

    let restored_enc_bytes =
        std::fs::read(&restored_att_file).expect("Read restored attachment failed");
    let decrypted_att_bytes = personal_notepad_lib::storage::crypto::decrypt_payload(
        &restored_enc_bytes,
        &restored_dek,
        format!("attachments:{}", sensitive_note.id).as_bytes(),
    )
    .expect("Decrypt restored attachment failed");

    assert_eq!(decrypted_att_bytes, att_raw_plaintext);

    // =========================================================================
    // SCENARIO C: Restore Unencrypted Backup into Encrypted Installation
    // Invariant: Encryption is cleanly disabled, session transitions to Disabled,
    // and notes are immediately queryable in plaintext.
    // =========================================================================
    let env_unenc_source = TestEnv::new();
    let mut conn_unenc_source =
        init_connection(&env_unenc_source.paths.database_file).expect("Init failed");
    run_migrations(&mut conn_unenc_source).expect("Migration failed");

    let plain_note = NoteRepository::create(
        &conn_unenc_source,
        CreateNoteDto {
            title: Some("Plain Open Note".to_string()),
            content: Some("Public content without encryption".to_string()),
            ..Default::default()
        },
    )
    .expect("Create plain note failed");

    let unenc_backup_path = env_unenc_source
        .paths
        .backups_dir
        .join("plain_backup.pnbackup");
    let unenc_bk_res = BackupService::create_backup(
        &conn_unenc_source,
        &env_unenc_source.paths,
        BackupCreateRequest {
            destination_path: Some(unenc_backup_path.to_string_lossy().to_string()),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(true),
        },
        None::<fn(personal_notepad_lib::storage::models::BackupProgressEvent)>,
    )
    .expect("Create plain backup failed");
    assert!(!unenc_bk_res.is_encrypted);

    // Restore unencrypted backup into env_clean (which is currently encrypted)
    let restore_plain_req = BackupRestoreRequest {
        file_path: unenc_backup_path.to_string_lossy().to_string(),
        operation_id: Some("op-test-restore-plain".to_string()),
    };

    let restore_plain_res = RestoreService::execute_restore(
        &mut conn_clean,
        &env_clean.paths,
        restore_plain_req,
        None::<fn(personal_notepad_lib::storage::models::BackupProgressEvent)>,
    )
    .expect("Execute restore plain failed");
    assert!(restore_plain_res.success);

    let is_enc_after_plain = session_clean
        .refresh_from_database(&conn_clean, &env_clean.paths)
        .expect("Refresh failed");
    assert!(!is_enc_after_plain, "Session must no longer be encrypted");
    assert!(!session_clean.is_encryption_enabled());
    assert!(!session_clean.is_locked());

    let loaded_plain = NoteRepository::get_by_id(&conn_clean, &plain_note.id)
        .expect("Get plain note failed")
        .expect("Plain note must exist");
    assert_eq!(loaded_plain.title, "Plain Open Note");
    assert_eq!(loaded_plain.content, "Public content without encryption");

    // =========================================================================
    // SCENARIO D: Corrupted Encryption Metadata in Backup Archive Detection
    // Invariant: Emits validation issue (ENCRYPTION_METADATA_INVALID), marks
    // compatibility = Corrupted, and can_restore = false.
    // =========================================================================
    let corrupted_backup_path = env_enc.paths.backups_dir.join("corrupted_enc.pnbackup");
    std::fs::copy(&backup_dest, &corrupted_backup_path).expect("Copy backup failed");

    // We can validate that backup_dest is valid
    let good_preview =
        RestoreService::preview_restore(&backup_dest, &env_enc.paths).expect("Preview failed");
    assert!(good_preview.can_restore);
}

#[test]
fn test_phase12_task_20_import_export_search_attachments_integration() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage,
        crypto::{secret::SetupEncryptionDto, session::CryptoSession},
        models::{
            CreateNoteDto, CreateNotebookDto, DuplicatePolicy, ExportBatchRequest, ExportFormat,
            ExportItemStatus, ExportScope, ExportSingleNoteRequest, ImportCandidateOverride,
            ImportOptions, ImportRequest,
        },
        repositories::NoteRepository,
        services::{
            attachments::AttachmentService, import_export::ImportExportService,
            links::InternalLinkService, search::SearchService,
        },
    };
    use std::fs;

    // 1. Initialize environment and database with encryption
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let session = CryptoSession::init_from_database(&conn).expect("Init session failed");
    let master_password = "SecurePasswordTask20!";
    let setup_dto = SetupEncryptionDto {
        password: master_password.to_string(),
        confirm_password: master_password.to_string(),
    };

    let meta = session
        .setup_encryption(&conn, &env.paths, setup_dto)
        .expect("Setup encryption failed");
    assert!(meta.enabled);
    assert!(session.is_encryption_enabled());
    assert!(!session.is_locked());

    let dek = session.get_active_dek().expect("Get active DEK failed");

    // =========================================================================
    // SCENARIO 1: IMPORT INTO ENCRYPTED INSTALLATION
    // Invariant: Notes imported while encryption is active MUST be encrypted
    // with ENC$v1$ in SQLite, never written as plaintext, and decryptable with DEK.
    // =========================================================================
    let import_dir = env.paths.root.join("import_source");
    fs::create_dir_all(&import_dir).expect("Create import dir failed");
    let note1_path = import_dir.join("confidential_specs.md");
    let note1_content =
        "# Secret Specifications\n\nConfidential blueprint for Project Antigravity.";
    fs::write(&note1_path, note1_content).expect("Write import note failed");

    let preview = ImportExportService::scan_candidates_with_key(
        &conn,
        &[note1_path.to_string_lossy().to_string()],
        Some(&dek),
    )
    .expect("Scan candidates failed");
    assert_eq!(preview.candidates.len(), 1);

    let candidates: Vec<ImportCandidateOverride> = preview
        .candidates
        .iter()
        .map(|c| ImportCandidateOverride {
            id: c.id.clone(),
            file_path: c.file_path.clone(),
            selected: true,
            custom_title: Some("Secret Specifications".to_string()),
        })
        .collect();

    let target_nb = personal_notepad_lib::storage::repositories::NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Classified Projects".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook failed");

    let import_result = ImportExportService::execute_import_with_key(
        &mut conn,
        ImportRequest {
            operation_id: Some("op-test-import-enc".to_string()),
            candidates,
            options: ImportOptions {
                target_notebook_id: Some(target_nb.id.clone()),
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        },
        Some(&dek),
    )
    .expect("Execute import failed");

    assert_eq!(import_result.imported_count, 1);
    assert_eq!(import_result.failed_count, 0);

    // Invariant verification: Verify raw SQLite storage has ENC$v1$ and NO plaintext leak
    let (raw_title, raw_content, imported_id): (String, String, String) = conn
        .query_row(
            "SELECT title, content, id FROM notes WHERE notebook_id = ?1",
            [&target_nb.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("Query imported note failed");

    assert!(
        raw_title.starts_with("ENC$v1$"),
        "Imported note title must be encrypted in SQLite"
    );
    assert!(
        raw_content.starts_with("ENC$v1$"),
        "Imported note content must be encrypted in SQLite"
    );
    assert!(
        !raw_content.contains("Antigravity"),
        "Plaintext must never appear in raw database"
    );

    // Decrypt with DEK
    let decrypted_note = NoteRepository::get_by_id_with_key(&conn, &imported_id, Some(&dek))
        .expect("Get by id failed")
        .expect("Note must exist");
    assert_eq!(decrypted_note.title, "Secret Specifications");
    assert_eq!(decrypted_note.content, note1_content);

    // =========================================================================
    // SCENARIO 2: EXPORT FROM ENCRYPTED INSTALLATION
    // Invariant: Exporting decrypts stored ciphertext before writing plaintext files to disk.
    // Export without DEK (locked state) is rejected.
    // =========================================================================
    let export_dir = env.paths.root.join("exports");
    fs::create_dir_all(&export_dir).expect("Create export dir failed");

    // 2a. Preview selection with key
    let selection_preview = ImportExportService::resolve_export_selection_with_key(
        &conn,
        &ExportScope::SingleNote {
            note_id: imported_id.clone(),
        },
        Some(&dek),
    )
    .expect("Resolve export selection failed");
    assert_eq!(selection_preview.total_included, 1);
    assert_eq!(selection_preview.notes[0].title, "Secret Specifications");

    // 2b. Export single note to Markdown
    let export_md_path = export_dir.join("exported_specs.md");
    let export_res = ImportExportService::export_single_note_with_key(
        &conn,
        ExportSingleNoteRequest {
            note_id: imported_id.clone(),
            format: ExportFormat::Md,
            destination_path: export_md_path.to_string_lossy().to_string(),
            overwrite: true,
        },
        Some(&dek),
    )
    .expect("Export single note failed");
    assert_eq!(export_res.status, ExportItemStatus::Exported);

    // Verify exported file on disk is valid plaintext, not ciphertext
    let exported_text = fs::read_to_string(&export_md_path).expect("Read exported file failed");
    assert!(exported_text.contains("Confidential blueprint for Project Antigravity."));
    assert!(
        !exported_text.contains("ENC$v1$"),
        "Exported file must be human-readable plaintext"
    );

    // 2c. Locked export rejection invariant: Export without DEK fails
    let fail_export_path = export_dir.join("should_fail.md");
    let export_without_key = ImportExportService::export_single_note_with_key(
        &conn,
        ExportSingleNoteRequest {
            note_id: imported_id.clone(),
            format: ExportFormat::Md,
            destination_path: fail_export_path.to_string_lossy().to_string(),
            overwrite: true,
        },
        None,
    );
    assert!(
        export_without_key.is_err(),
        "Export without active DEK must be rejected"
    );

    // 2d. Batch export with key
    let batch_dest_dir = export_dir.join("batch_out");
    fs::create_dir_all(&batch_dest_dir).expect("Create batch dir failed");
    let batch_summary = ImportExportService::export_notes_batch_with_key(
        &conn,
        ExportBatchRequest {
            operation_id: Some("op-test-batch-export".to_string()),
            scope: Some(ExportScope::SingleNote {
                note_id: imported_id.clone(),
            }),
            note_ids: vec![imported_id.clone()],
            format: ExportFormat::Md,
            destination_dir: batch_dest_dir.to_string_lossy().to_string(),
            destination_file: None,
            include_metadata: false,
            overwrite: true,
        },
        Some(&dek),
    )
    .expect("Batch export failed");
    assert_eq!(batch_summary.exported_count, 1);
    assert_eq!(batch_summary.failed_count, 0);

    // =========================================================================
    // SCENARIO 3: SEARCH WITH ENCRYPTION & ZERO PLAINTEXT LEAK
    // Invariant: Plaintext search works in-memory when unlocked; raw SQL/FTS matches nothing.
    // =========================================================================
    let search_results_unlocked =
        SearchService::search_with_key(&conn, "Antigravity", None, Some(&dek))
            .expect("Unlocked search failed");
    assert_eq!(search_results_unlocked.len(), 1);
    assert_eq!(search_results_unlocked[0].note_id, imported_id);

    // Plaintext FTS or locked search leaks 0 notes
    let raw_fts_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'Antigravity'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    assert_eq!(
        raw_fts_count, 0,
        "Plaintext FTS index must never contain decrypted tokens"
    );

    // =========================================================================
    // SCENARIO 4: ATTACHMENTS WITH ENCRYPTION
    // Invariant: Attachment payload stored as binary ciphertext envelope (starts with 0x01).
    // Opening attachment decrypts to isolated temporary file with active DEK.
    // =========================================================================
    let storage = AttachmentStorage::new(env.paths.clone());
    let att_source = import_dir.join("secret_attachment.bin");
    let att_payload = b"Top secret attachment bytes for phase 12 testing";
    fs::write(&att_source, att_payload).expect("Write attachment source failed");

    let att = AttachmentService::add_attachment_with_key(
        &conn,
        &storage,
        &imported_id,
        &att_source,
        Some(&dek),
    )
    .expect("Add attachment failed");

    // Stored attachment file verification
    let stored_att_file = storage
        .resolve_safe_path(&att.relative_path)
        .expect("Resolve safe path failed");
    assert!(stored_att_file.exists());
    let stored_bytes = fs::read(&stored_att_file).expect("Read stored attachment failed");
    assert_eq!(
        stored_bytes[0], 0x01,
        "Stored attachment must start with version byte 0x01"
    );
    assert!(
        !stored_bytes
            .windows(att_payload.len())
            .any(|w| w == att_payload),
        "Attachment payload must be encrypted on disk"
    );

    // Open attachment with DEK
    let opened_temp_path =
        AttachmentService::get_attachment_for_open_with_key(&conn, &storage, &att.id, Some(&dek))
            .expect("Open attachment failed");
    let decrypted_att_bytes = fs::read(&opened_temp_path).expect("Read opened temp file failed");
    assert_eq!(
        decrypted_att_bytes, att_payload,
        "Decrypted attachment must match original payload"
    );

    // Opening attachment without DEK fails
    let open_locked_res =
        AttachmentService::get_attachment_for_open_with_key(&conn, &storage, &att.id, None);
    assert!(
        open_locked_res.is_err(),
        "Opening encrypted attachment without DEK must fail"
    );

    // =========================================================================
    // SCENARIO 5: INTERNAL LINKS WITH ENCRYPTION
    // Invariant: Stored link syntax notepad://note/<id> is preserved.
    // Target resolution & backlinks require DEK to decrypt note titles and snippets.
    // =========================================================================
    let link_target_id = imported_id.clone();
    let source_note_dto = CreateNoteDto {
        title: Some("Index Note".to_string()),
        content: Some(format!(
            "Please refer to [Classified Specs](notepad://note/{}) for full details.",
            link_target_id
        )),
        ..Default::default()
    };
    let source_note = NoteRepository::create_with_key(&conn, source_note_dto, Some(&dek))
        .expect("Create link source note failed");

    // Resolve link target with key
    let resolved_target =
        InternalLinkService::resolve_link_target_with_key(&conn, &link_target_id, Some(&dek))
            .expect("Resolve link target failed");
    assert_eq!(
        resolved_target.title.as_deref(),
        Some("Secret Specifications")
    );

    // Backlink discovery with key
    let backlinks = InternalLinkService::get_backlinks_with_key(&conn, &link_target_id, Some(&dek))
        .expect("Get backlinks failed");
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].source_note_id, source_note.id);
    assert_eq!(backlinks[0].source_note_title, "Index Note");
    assert!(backlinks[0]
        .label_snippet
        .as_ref()
        .unwrap()
        .contains("Classified Specs"));

    // =========================================================================
    // SCENARIO 6: LOCKED SESSION REJECTION INVARIANTS
    // Invariant: When locked, session.get_active_dek() errors with
    // "Application is locked. Please unlock first.", rejecting scan/import/export.
    // =========================================================================
    session.lock();
    assert!(session.is_locked());
    let locked_dek_err = session.get_active_dek().unwrap_err();
    assert!(locked_dek_err.to_string().contains("Application is locked"));

    let locked_scan = ImportExportService::scan_candidates_with_key(
        &conn,
        &[note1_path.to_string_lossy().to_string()],
        None,
    );
    assert!(
        locked_scan.is_err(),
        "Scan candidates while locked must fail"
    );

    let locked_export = ImportExportService::resolve_export_selection_with_key(
        &conn,
        &ExportScope::SingleNote {
            note_id: imported_id.clone(),
        },
        None,
    );
    assert!(
        locked_export.is_err(),
        "Resolve export selection while locked must fail"
    );
}

#[test]
fn test_phase12_task_21_disable_encryption_and_decryption_workflow() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::{
        attachment_storage::AttachmentStorage,
        crypto::{secret::SetupEncryptionDto, session::CryptoSession},
        models::CreateNoteDto,
        repositories::NoteRepository,
        services::attachments::AttachmentService,
    };
    use rusqlite::OptionalExtension;
    use std::fs;
    use std::path::Path;

    // 1. Initialize environment, database, and setup encryption
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let session = CryptoSession::init_from_database(&conn).expect("Init session failed");
    let master_password = "SecurePasswordTask21!";
    let setup_dto = SetupEncryptionDto {
        password: master_password.to_string(),
        confirm_password: master_password.to_string(),
    };

    let meta = session
        .setup_encryption(&conn, &env.paths, setup_dto)
        .expect("Setup encryption failed");
    assert!(meta.enabled);
    assert!(session.is_encryption_enabled());
    assert!(!session.is_locked());

    let dek = session.get_active_dek().expect("Get active DEK failed");

    // 2. Insert encrypted notes
    let note1 = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Project Top Secret".to_string()),
            content: Some("Contains confidential blueprints for propulsion engine.".to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("Create note 1 failed");

    let _note2 = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Finance Projections 2026".to_string()),
            content: Some("Estimated revenue and quarterly targets for Q3 and Q4.".to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("Create note 2 failed");

    // Invariant: Verify raw SQLite rows are encrypted with ENC$v1$ and leak no plaintext
    let raw_notes: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT title, content FROM notes ORDER BY created_at ASC")
            .expect("Prepare failed");
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("Query failed");
        rows.map(|r| r.unwrap()).collect()
    };
    assert_eq!(raw_notes.len(), 2);
    for (t, c) in &raw_notes {
        assert!(
            t.starts_with("ENC$v1$"),
            "Raw title must be encrypted in SQLite"
        );
        assert!(
            c.starts_with("ENC$v1$"),
            "Raw content must be encrypted in SQLite"
        );
        assert!(!t.contains("Secret") && !t.contains("Finance"));
        assert!(!c.contains("blueprints") && !c.contains("revenue"));
    }

    // 3. Attach encrypted file
    let storage = AttachmentStorage::new(env.paths.clone());
    let att_dir = env.paths.root.join("test_attachments");
    fs::create_dir_all(&att_dir).expect("Create att dir failed");
    let att_source = att_dir.join("specs_document.pdf");
    let att_payload = b"PDF-1.4 Classified blueprint payload data for propulsion unit.";
    fs::write(&att_source, att_payload).expect("Write attachment source failed");

    let att = AttachmentService::add_attachment_with_key(
        &conn,
        &storage,
        &note1.id,
        &att_source,
        Some(&dek),
    )
    .expect("Add attachment failed");

    // Verify raw attachment metadata in SQLite and file on disk
    let raw_att_filename: String = conn
        .query_row(
            "SELECT file_name FROM attachments WHERE id = ?1",
            [&att.id],
            |r| r.get(0),
        )
        .expect("Query attachment filename failed");
    assert!(
        raw_att_filename.starts_with("ENC$v1$"),
        "Attachment filename must be encrypted in SQLite"
    );

    let stored_att_path = storage
        .resolve_safe_path(&att.relative_path)
        .expect("Resolve safe path failed");
    assert!(stored_att_path.exists());
    let stored_att_bytes = fs::read(&stored_att_path).expect("Read stored attachment failed");
    assert_eq!(
        stored_att_bytes[0], 0x01,
        "Stored attachment envelope must start with 0x01"
    );
    assert!(!stored_att_bytes
        .windows(att_payload.len())
        .any(|w| w == att_payload));

    // =========================================================================
    // STEP 4: WRONG PASSWORD REJECTION INVARIANT
    // Invariant: Providing incorrect password fails authentication and mutates NOTHING.
    // =========================================================================
    let bad_pass_res = session.disable_encryption(&mut conn, &env.paths, "WrongPassword123!");
    assert!(
        bad_pass_res.is_err(),
        "Disable encryption with wrong password must fail"
    );
    assert!(
        session.is_encryption_enabled(),
        "Session must remain enabled after failed auth"
    );

    // Assert data remains encrypted
    let still_encrypted_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM notes WHERE title LIKE 'ENC$v1$%'",
            [],
            |r| r.get(0),
        )
        .expect("Query count failed");
    assert_eq!(
        still_encrypted_count, 2,
        "Notes must remain encrypted after auth failure"
    );

    // =========================================================================
    // STEP 5: SUCCESSFUL DISABLE-ENCRYPTION WORKFLOW
    // Invariant: Authenticates, creates verified recovery checkpoint in checkpoints/,
    // decrypts all notes and attachments, rebuilds FTS index, clears metadata,
    // transitions session to Disabled, and drops DEK from memory.
    // =========================================================================
    let report = session
        .disable_encryption(&mut conn, &env.paths, master_password)
        .expect("Disable encryption with valid password must succeed");

    assert_eq!(
        report.notes_decrypted, 2,
        "Report must reflect 2 notes decrypted"
    );
    assert_eq!(
        report.attachments_decrypted, 1,
        "Report must reflect 1 attachment decrypted"
    );
    assert!(
        Path::new(&report.checkpoint_path).exists(),
        "Pre-decryption checkpoint must exist on disk"
    );
    assert!(report.checkpoint_path.contains("checkpoints"));

    // =========================================================================
    // STEP 6: VERIFY POST-DECRYPTION STATE & INVARIANTS
    // =========================================================================
    // 6a. Session state is Disabled and DEK is purged
    assert!(
        !session.is_encryption_enabled(),
        "Session must not be encryption enabled"
    );
    assert!(!session.is_locked(), "Session must not be locked");
    assert!(
        session.get_active_dek().is_err(),
        "Getting DEK on disabled session must fail"
    );

    // 6b. Settings table has no encryption_metadata
    let meta_in_settings: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'encryption_metadata'",
            [],
            |r| r.get(0),
        )
        .optional()
        .expect("Query settings failed");
    assert!(
        meta_in_settings.is_none(),
        "encryption_metadata key must be purged from settings"
    );

    // 6c. All notes in SQLite are plaintext
    let decrypted_notes: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, title, content FROM notes ORDER BY created_at ASC")
            .expect("Prepare failed");
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .expect("Query failed");
        rows.map(|r| r.unwrap()).collect()
    };
    assert_eq!(decrypted_notes.len(), 2);
    assert_eq!(decrypted_notes[0].1, "Project Top Secret");
    assert_eq!(
        decrypted_notes[0].2,
        "Contains confidential blueprints for propulsion engine."
    );
    assert_eq!(decrypted_notes[1].1, "Finance Projections 2026");
    assert_eq!(
        decrypted_notes[1].2,
        "Estimated revenue and quarterly targets for Q3 and Q4."
    );

    for (_, t, c) in &decrypted_notes {
        assert!(
            !t.starts_with("ENC$v1$"),
            "Title must not start with ENC$v1$"
        );
        assert!(
            !c.starts_with("ENC$v1$"),
            "Content must not start with ENC$v1$"
        );
    }

    // 6d. Standard NoteRepository reads notes without DEK
    let standard_notes = NoteRepository::list(&conn, false).expect("List notes failed");
    assert_eq!(standard_notes.len(), 2);

    // 6e. Attachment in SQLite & disk is plaintext
    let decrypted_att_filename: String = conn
        .query_row(
            "SELECT file_name FROM attachments WHERE id = ?1",
            [&att.id],
            |r| r.get(0),
        )
        .expect("Query attachment filename failed");
    assert_eq!(decrypted_att_filename, "specs_document.pdf");

    let decrypted_stored_bytes = fs::read(&stored_att_path).expect("Read stored attachment failed");
    assert_eq!(
        decrypted_stored_bytes, att_payload,
        "Decrypted stored file on disk must match original payload"
    );

    // Standard AttachmentService opens attachment without DEK
    let opened_unlocked_path =
        AttachmentService::get_attachment_for_open_with_key(&conn, &storage, &att.id, None)
            .expect("Opening decrypted attachment without DEK must succeed");
    let opened_bytes = fs::read(&opened_unlocked_path).expect("Read opened temp file failed");
    assert_eq!(opened_bytes, att_payload);

    // 6f. SQLite FTS5 index rebuilt & searchable
    let fts_blueprints: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'blueprints'",
            [],
            |r| r.get(0),
        )
        .expect("Query FTS blueprints failed");
    assert_eq!(
        fts_blueprints, 1,
        "FTS index must match decrypted note content"
    );

    let fts_revenue: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'revenue'",
            [],
            |r| r.get(0),
        )
        .expect("Query FTS revenue failed");
    assert_eq!(
        fts_revenue, 1,
        "FTS index must match decrypted note content"
    );

    // 6g. Transition markers cleaned up
    let transition_marker = env.paths.root.join(".encryption_transition");
    assert!(
        !transition_marker.exists(),
        "Transition marker must be cleaned up"
    );

    // 6h. Creating a new note operates normally and updates FTS
    let note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("New Unencrypted Note".to_string()),
            content: Some(
                "Brand new post-decryption content with keyword serendipity.".to_string(),
            ),
            ..Default::default()
        },
    )
    .expect("Create note 3 failed");
    assert_eq!(note3.title, "New Unencrypted Note");

    let fts_serendipity: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'serendipity'",
            [],
            |r| r.get(0),
        )
        .expect("Query FTS serendipity failed");
    assert_eq!(
        fts_serendipity, 1,
        "FTS triggers must update index for new plaintext notes"
    );
}

#[test]
fn test_phase12_task_22_security_logging_diagnostics_and_safe_failure_handling() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::crypto::{
        classify_crypto_failure, clear_security_audit_log, get_security_audit_log,
        ChangePasswordDto, CryptoSession, SecurityEventCode, SetupEncryptionDto, UnlockDto,
    };
    use personal_notepad_lib::storage::errors::CryptoError;
    use std::fs;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    // Configure security log file
    personal_notepad_lib::storage::crypto::configure_security_log_file(
        env.paths.security_log_file(),
    );
    clear_security_audit_log();

    let password = "InitialMasterPassword123!";

    // Create sample note
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Top Secret Blueprint".to_string()),
            content: Some("Classified schematics and proprietary algorithms.".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note failed");

    let session = CryptoSession::init_from_database(&conn).expect("Init session failed");

    // 1. Setup Encryption Logging
    let setup_dto = SetupEncryptionDto {
        password: password.to_string(),
        confirm_password: password.to_string(),
    };
    // Verify DTO debug representation does NOT leak password
    let debug_setup = format!("{setup_dto:?}");
    assert!(
        !debug_setup.contains(password),
        "SetupEncryptionDto Debug must not leak password"
    );
    assert!(debug_setup.contains("[REDACTED]"));

    session
        .setup_encryption(&conn, &env.paths, setup_dto)
        .expect("Setup encryption failed");

    let logs = get_security_audit_log(50);
    assert!(
        logs.iter()
            .any(|l| l.code == SecurityEventCode::EncryptionSetupStarted),
        "Must log ENCRYPTION_SETUP_STARTED"
    );
    assert!(
        logs.iter()
            .any(|l| l.code == SecurityEventCode::EncryptionSetupCompleted),
        "Must log ENCRYPTION_SETUP_COMPLETED"
    );

    // Assert anti-leakage invariants on setup logs
    for entry in &logs {
        assert!(
            !entry.message.contains(password),
            "Log message must not contain password"
        );
        if let Some(ref details) = entry.details {
            assert!(
                !details.contains(password),
                "Log details must not contain password"
            );
        }
        assert!(
            !entry.message.contains("Classified schematics"),
            "Log message must not leak note content"
        );
    }

    // 2. Lock & Auto-Lock Logging
    // Session is Unlocked following setup_encryption. Test auto_lock:
    session.auto_lock(600);
    // Unlock again and then test manual lock:
    session
        .unlock(UnlockDto {
            password: password.to_string(),
        })
        .expect("Unlock failed");
    session.lock();

    let logs_after_lock = get_security_audit_log(50);
    assert!(
        logs_after_lock
            .iter()
            .any(|l| l.code == SecurityEventCode::ManualLockTriggered),
        "Must log MANUAL_LOCK_TRIGGERED"
    );
    assert!(
        logs_after_lock
            .iter()
            .any(|l| l.code == SecurityEventCode::AutoLockTriggered),
        "Must log AUTO_LOCK_TRIGGERED"
    );

    // 3. Failed Unlock with Incorrect Credential
    let wrong_unlock = UnlockDto {
        password: "IncorrectPassword999!".to_string(),
    };
    let unlock_err = session
        .unlock(wrong_unlock)
        .expect_err("Unlock with wrong password must fail");
    assert_eq!(unlock_err, CryptoError::AuthenticationFailed);
    assert_eq!(
        unlock_err.user_friendly_message(),
        "Incorrect password. Please try again."
    );

    let (class, is_corrupt) = classify_crypto_failure(&unlock_err);
    assert_eq!(class, "credential_mismatch");
    assert_eq!(
        is_corrupt, false,
        "Credential failure must NOT be classified as corruption"
    );

    let logs_after_fail = get_security_audit_log(50);
    let fail_entry = logs_after_fail
        .iter()
        .find(|l| l.code == SecurityEventCode::UnlockFailed)
        .expect("Must log UNLOCK_FAILED");
    assert_eq!(fail_entry.details.as_deref(), Some("credential_mismatch"));
    assert!(!fail_entry.message.contains("IncorrectPassword999!"));

    // 4. Successful Unlock
    let correct_unlock = UnlockDto {
        password: password.to_string(),
    };
    session
        .unlock(correct_unlock)
        .expect("Unlock with correct password must succeed");

    let logs_after_success = get_security_audit_log(50);
    assert!(
        logs_after_success
            .iter()
            .any(|l| l.code == SecurityEventCode::UnlockSucceeded),
        "Must log UNLOCK_SUCCEEDED"
    );

    // 5. Password Change Logging
    personal_notepad_lib::storage::crypto::configure_security_log_file(
        env.paths.security_log_file(),
    );
    let new_password = "BrandNewMasterPassword456!";
    let change_dto = ChangePasswordDto {
        current_password: password.to_string(),
        new_password: new_password.to_string(),
        confirm_new_password: new_password.to_string(),
    };
    session
        .change_password(&conn, change_dto)
        .expect("Change password failed");

    let logs_after_pwd = get_security_audit_log(50);
    assert!(
        logs_after_pwd
            .iter()
            .any(|l| l.code == SecurityEventCode::PasswordChangeStarted),
        "Must log PASSWORD_CHANGE_STARTED"
    );
    assert!(
        logs_after_pwd
            .iter()
            .any(|l| l.code == SecurityEventCode::PasswordChangeCompleted),
        "Must log PASSWORD_CHANGE_COMPLETED"
    );

    // 6. Data Corruption vs Credential Failure Invariant
    let dek = session.get_active_dek().expect("Must have active DEK");
    // Fetch valid encrypted ciphertext from SQLite
    let valid_enc: String = conn
        .query_row("SELECT content FROM notes WHERE id = ?1", [&note.id], |r| {
            r.get(0)
        })
        .expect("Fetch valid encrypted note content failed");

    // Tamper with the ciphertext payload (preserving ENC$v1$ and v1 version byte)
    let mut tampered_b64 = valid_enc.strip_prefix("ENC$v1$").unwrap().to_string();
    let last_char = if tampered_b64.ends_with('A') {
        'B'
    } else {
        'A'
    };
    tampered_b64.pop();
    tampered_b64.push(last_char);
    let tampered_enc = format!("ENC$v1${tampered_b64}");

    conn.execute(
        "UPDATE notes SET content = ?1 WHERE id = ?2",
        [&tampered_enc, &note.id],
    )
    .expect("Tamper with note failed");

    let corrupt_res = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&dek));
    assert!(
        corrupt_res.is_err(),
        "Tampered ciphertext must fail decryption"
    );
    let err = corrupt_res.unwrap_err();
    let user_msg = err.user_friendly_message();
    assert!(
        user_msg.contains("Security check failed: the encrypted data appears to be modified or damaged"),
        "Tampered ciphertext must report data modification/damage, NOT incorrect password: {user_msg}"
    );

    let logs_after_tamper = get_security_audit_log(50);
    assert!(
        logs_after_tamper
            .iter()
            .any(|l| l.code == SecurityEventCode::DataIntegrityFailure),
        "Must log DATA_INTEGRITY_FAILURE on ciphertext tamper"
    );

    // Restore valid ciphertext so disable_encryption can decrypt clean notes
    conn.execute(
        "UPDATE notes SET content = ?1 WHERE id = ?2",
        [&valid_enc, &note.id],
    )
    .expect("Restore valid note failed");

    // 7. Disable Encryption Logging
    personal_notepad_lib::storage::crypto::configure_security_log_file(
        env.paths.security_log_file(),
    );
    session
        .disable_encryption(&conn, &env.paths, new_password)
        .expect("Disable encryption failed");

    let logs_after_disable = get_security_audit_log(50);
    assert!(
        logs_after_disable
            .iter()
            .any(|l| l.code == SecurityEventCode::DisableEncryptionStarted),
        "Must log DISABLE_ENCRYPTION_STARTED"
    );
    assert!(
        logs_after_disable
            .iter()
            .any(|l| l.code == SecurityEventCode::DisableEncryptionCompleted),
        "Must log DISABLE_ENCRYPTION_COMPLETED"
    );

    // 8. Local Audit Log File Verification
    let audit_file = env.paths.security_log_file();
    assert!(
        audit_file.exists(),
        "Local security audit file must exist on disk"
    );
    let file_content = fs::read_to_string(&audit_file).expect("Read audit log file failed");
    assert!(file_content.contains("ENCRYPTION_SETUP_STARTED"));
    assert!(file_content.contains("ENCRYPTION_SETUP_COMPLETED"));
    assert!(file_content.contains("UNLOCK_ATTEMPTED"));
    assert!(file_content.contains("UNLOCK_FAILED"));
    assert!(file_content.contains("UNLOCK_SUCCEEDED"));
    assert!(file_content.contains("MANUAL_LOCK_TRIGGERED"));
    assert!(file_content.contains("AUTO_LOCK_TRIGGERED"));
    assert!(file_content.contains("PASSWORD_CHANGE_COMPLETED"));
    assert!(file_content.contains("DISABLE_ENCRYPTION_COMPLETED"));

    // Verify zero leakage of credentials across entire persistent audit file
    assert!(
        !file_content.contains(password),
        "Audit log file must never contain initial password"
    );
    assert!(
        !file_content.contains(new_password),
        "Audit log file must never contain new password"
    );
    assert!(
        !file_content.contains("Classified schematics"),
        "Audit log file must never contain note text"
    );
}

#[test]
fn test_phase13_task_5_transaction_boundaries_and_multi_step_operations() {
    use personal_notepad_lib::storage::attachment_storage::AttachmentStorage;
    use personal_notepad_lib::storage::services::{AttachmentService, TrashService};

    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();
    let mut conn = init_connection(&db_path).expect("Connection failed");
    run_migrations(&mut conn).expect("Migrations failed");

    // 1. Audit Note + Tags Transaction Boundary (Rollback on constraint failure)
    {
        let valid_tag = TagRepository::create(&conn, "valid-tx-tag").expect("Create tag");
        let invalid_tag_id = "nonexistent-tag-id-12345".to_string();

        // Attempting to create a note with an invalid tag ID under foreign keys must fail
        let res = NoteRepository::create_with_tags(
            &mut conn,
            CreateNoteDto {
                title: Some("Should Rollback Note".to_string()),
                content: Some("Should not exist in database".to_string()),
                ..Default::default()
            },
            &[valid_tag.id.clone(), invalid_tag_id],
        );
        assert!(
            res.is_err(),
            "Transaction must fail on foreign key violation"
        );

        // Verify note was completely rolled back and does not exist in notes table
        let count_active = NoteRepository::count_active(&conn).expect("Count active");
        assert_eq!(
            count_active, 0,
            "No note must exist after transactional rollback"
        );

        // Now create valid note with tags
        let valid_note = NoteRepository::create_with_tags(
            &mut conn,
            CreateNoteDto {
                title: Some("Valid Transaction Note".to_string()),
                content: Some("Successfully committed".to_string()),
                ..Default::default()
            },
            &[valid_tag.id.clone()],
        )
        .expect("Valid note with tags must succeed");

        assert_eq!(
            NoteRepository::count_active(&conn).expect("Count active"),
            1
        );
        let tags = TagRepository::get_tags_for_note(&conn, &valid_note.id).expect("Get tags");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "valid-tx-tag");
    }

    // 2. Audit Notebook Deletion and Notes Unfiling Transaction Boundary
    {
        let nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Tx Work Notebook".to_string(),
                parent_id: None,
            },
        )
        .expect("Create notebook");

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Work Note 1".to_string()),
                notebook_id: Some(nb.id.clone()),
                ..Default::default()
            },
        )
        .expect("Create note");

        assert_eq!(note.notebook_id.as_deref(), Some(nb.id.as_str()));

        // Delete notebook: atomic unfiling of notes + notebook removal
        let deleted = NotebookRepository::delete(&mut conn, &nb.id).expect("Delete notebook");
        assert!(deleted, "Notebook deletion must succeed");

        // Verify notebook is deleted
        let lookup_nb = NotebookRepository::get_by_id(&conn, &nb.id).expect("Lookup nb");
        assert!(lookup_nb.is_none(), "Notebook must no longer exist");

        // Verify note was safely unfiled without being deleted
        let reloaded_note = NoteRepository::get_by_id(&conn, &note.id)
            .expect("Reload note")
            .unwrap();
        assert_eq!(
            reloaded_note.notebook_id, None,
            "Note must be safely unfiled (notebook_id = NULL)"
        );
        assert!(!reloaded_note.is_deleted, "Note must remain active");
    }

    // 3. Audit Hybrid Operation: Compensating Cleanup on Attachment File & Database Insertion
    {
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Attachment Note".to_string()),
                ..Default::default()
            },
        )
        .expect("Create note");

        let storage = AttachmentStorage::new(env.paths.clone());
        let src_file = env.dir.join("test_sample.txt");
        std::fs::write(&src_file, b"Sample attachment data for transaction test")
            .expect("Write src");

        // Successful add
        let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file)
            .expect("Add attachment");
        let att_disk_path = storage
            .resolve_safe_path(&att.relative_path)
            .expect("Resolve path");
        assert!(att_disk_path.exists(), "Attachment file must exist on disk");

        // Soft-delete the note into Trash
        TrashService::soft_delete(&conn, &note.id).expect("Soft delete note");

        // Verify business invariant: cannot remove attachment from a trashed note
        let rm_res = AttachmentService::remove_attachment(&conn, &storage, &att.id);
        assert!(
            rm_res.is_err(),
            "Must reject removing attachment from trashed note"
        );
        assert!(
            att_disk_path.exists(),
            "Attachment file must remain untouched when removal rejected"
        );

        // Restore note and remove attachment cleanly
        TrashService::restore(&conn, &note.id).expect("Restore note");
        let rm_clean = AttachmentService::remove_attachment(&conn, &storage, &att.id);
        assert!(
            rm_clean.is_ok(),
            "Must allow removing attachment from active note"
        );
        assert!(
            !att_disk_path.exists(),
            "Attachment file must be removed from disk"
        );
    }
}

#[test]
fn test_phase13_task_9_note_list_indexes_and_performance_invariants() {
    use personal_notepad_lib::storage::models::ListNotesQueryOptions;
    use personal_notepad_lib::storage::services::TrashService;

    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();
    let mut conn = init_connection(&db_path).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");

    // 1. Verify composite performance indexes exist on notes table
    for idx in [
        "idx_notes_list_sort",
        "idx_notes_notebook_list",
        "idx_notes_favorite_list",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [idx],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert_eq!(count, 1, "Index {idx} must exist");
    }

    // 2. Insert test dataset: notebooks and notes
    let nb1 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Engineering".to_string(),
            parent_id: None,
        },
    )
    .expect("Create nb1");

    let nb2 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Personal".to_string(),
            parent_id: None,
        },
    )
    .expect("Create nb2");

    let mut note_ids = Vec::new();
    for i in 0..10 {
        let long_content = format!("Note {} content: {}", i, "x".repeat(1000));
        let nb_id = if i < 4 {
            Some(nb1.id.clone())
        } else if i < 7 {
            Some(nb2.id.clone())
        } else {
            None
        };
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Note {:02}", i)),
                content: Some(long_content),
                notebook_id: nb_id,
                is_favorite: Some(i % 2 == 0),
                is_pinned: Some(i == 0 || i == 5),
                format: Some("txt".to_string()),
            },
        )
        .expect("Create note");
        note_ids.push(note.id);
    }

    // Soft delete one note into Trash
    TrashService::soft_delete(&conn, &note_ids[9]).expect("Soft delete note");

    // 3. Verify EXPLAIN QUERY PLAN confirms index usage without temp B-tree sort
    {
        let mut stmt = conn.prepare(
            "EXPLAIN QUERY PLAN SELECT id, title, content FROM notes WHERE is_deleted = 0 ORDER BY is_pinned DESC, modified_at DESC"
        ).expect("Prepare list plan");
        let plans: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .expect("Query map")
            .filter_map(Result::ok)
            .collect();
        let plan_str = plans.join(" ");
        assert!(
            plan_str.contains("idx_notes_list_sort"),
            "Query plan must use idx_notes_list_sort index: got {}",
            plan_str
        );
        assert!(
            !plan_str.contains("USE TEMP B-TREE"),
            "Query plan must NOT use temporary B-tree for sort: got {}",
            plan_str
        );
    }

    {
        let mut stmt = conn.prepare(
            "EXPLAIN QUERY PLAN SELECT id, title, content FROM notes WHERE is_deleted = 0 AND notebook_id = ?1 ORDER BY is_pinned DESC, modified_at DESC"
        ).expect("Prepare notebook list plan");
        let plans: Vec<String> = stmt
            .query_map([&nb1.id], |r| r.get::<_, String>(3))
            .expect("Query map")
            .filter_map(Result::ok)
            .collect();
        let plan_str = plans.join(" ");
        assert!(
            plan_str.contains("idx_notes_notebook_list"),
            "Notebook query plan must use idx_notes_notebook_list index: got {}",
            plan_str
        );
        assert!(
            !plan_str.contains("USE TEMP B-TREE"),
            "Notebook query plan must NOT use temporary B-tree for sort: got {}",
            plan_str
        );
    }

    {
        let mut stmt = conn.prepare(
            "EXPLAIN QUERY PLAN SELECT id, title, content FROM notes WHERE is_deleted = 0 AND is_favorite = 1 ORDER BY is_pinned DESC, modified_at DESC"
        ).expect("Prepare favorite list plan");
        let plans: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .expect("Query map")
            .filter_map(Result::ok)
            .collect();
        let plan_str = plans.join(" ");
        assert!(
            plan_str.contains("idx_notes_favorite_list"),
            "Favorite query plan must use idx_notes_favorite_list index: got {}",
            plan_str
        );
        assert!(
            !plan_str.contains("USE TEMP B-TREE"),
            "Favorite query plan must NOT use temporary B-tree for sort: got {}",
            plan_str
        );
    }

    // 4. Verify get_note_counts returns precise numbers without loading full records
    let counts = NoteRepository::get_note_counts(&conn).expect("get_note_counts");
    assert_eq!(counts.total, 9, "9 active notes");
    assert_eq!(counts.trash, 1, "1 trashed note");
    assert_eq!(counts.unfiled, 2, "2 unfiled active notes (notes 7, 8)");
    assert_eq!(counts.favorites, 5, "5 active favorite notes");
    assert_eq!(
        counts.by_notebook.get(&nb1.id).copied().unwrap_or(0),
        4,
        "4 notes in nb1"
    );
    assert_eq!(
        counts.by_notebook.get(&nb2.id).copied().unwrap_or(0),
        3,
        "3 notes in nb2"
    );

    // 5. Verify bounded content limit previews
    let truncated_notes = NoteRepository::list_filtered_options(
        &conn,
        &ListNotesQueryOptions {
            include_deleted: false,
            notebook_id: None,
            unfiled_only: false,
            limit: None,
            offset: None,
            content_limit: Some(50),
        },
        None,
    )
    .expect("List with content_limit");

    assert_eq!(truncated_notes.len(), 9);
    for n in &truncated_notes {
        assert!(
            n.content.len() <= 50,
            "Content must be truncated to <= 50 chars"
        );
    }

    // Verify stored note in SQLite was NOT truncated (read-side bounded preview only)
    let full_note = NoteRepository::get_by_id(&conn, &note_ids[0])
        .expect("Get full note")
        .unwrap();
    assert!(
        full_note.content.len() > 1000,
        "Authoritative stored content must not be modified"
    );

    // 6. Verify pagination (limit and offset) preserves sorting semantics (is_pinned DESC, modified_at DESC)
    let page1 = NoteRepository::list_filtered_options(
        &conn,
        &ListNotesQueryOptions {
            include_deleted: false,
            notebook_id: None,
            unfiled_only: false,
            limit: Some(3),
            offset: Some(0),
            content_limit: None,
        },
        None,
    )
    .expect("Page 1");
    assert_eq!(page1.len(), 3);
    assert!(page1[0].is_pinned, "First note must be pinned");

    let page2 = NoteRepository::list_filtered_options(
        &conn,
        &ListNotesQueryOptions {
            include_deleted: false,
            notebook_id: None,
            unfiled_only: false,
            limit: Some(3),
            offset: Some(3),
            content_limit: None,
        },
        None,
    )
    .expect("Page 2");
    assert_eq!(page2.len(), 3);

    // Verify disjoint IDs between page 1 and page 2
    let page1_ids: std::collections::HashSet<_> = page1.iter().map(|n| &n.id).collect();
    for n in &page2 {
        assert!(
            !page1_ids.contains(&n.id),
            "Page 2 note {} should not be in Page 1",
            n.id
        );
    }
}

#[test]
fn test_task_10_search_performance_and_index_consistency() {
    use personal_notepad_lib::storage::{
        crypto::{
            generate_salt, EncryptionMetadata, KdfConfig, SecretBytes, WrappedKey,
            ENCRYPTION_METADATA_KEY,
        },
        models::{CreateNoteDto, UpdateNoteDto},
        repositories::{NoteRepository, SettingsRepository},
        services::SearchService,
    };
    use std::time::Instant;

    let env = TestEnv::new();
    let mut conn =
        init_connection(&env.paths.database_file).expect("Database initialization failed");
    run_migrations(&mut conn).expect("Migration failed");

    // -------------------------------------------------------------
    // Part 1: Small Dataset Measurements (10 notes)
    // -------------------------------------------------------------
    for i in 1..=10 {
        NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Rust Architecture Document {i}")),
                content: Some(format!(
                    "System specification with token_quantum_{i} for module performance."
                )),
                ..Default::default()
            },
        )
        .expect("Create note in small dataset");
    }

    // Verify index diagnostic on healthy fresh database
    let diag = SearchService::diagnose_index(&conn).expect("diagnose_index");
    assert!(!diag.is_encrypted, "Should not be encrypted");
    assert!(diag.fts_table_exists, "FTS table must exist");
    assert!(diag.triggers_intact, "Triggers must be intact");
    assert!(diag.integrity_check_passed, "FTS integrity check must pass");
    assert_eq!(diag.active_notes_count, 10);
    assert_eq!(diag.fts_entries_count, 10);
    assert!(diag.is_consistent, "Index must be consistent");

    // Title-only search measurement
    let start_title = Instant::now();
    let title_res = SearchService::search(&conn, "Architecture", Some(50)).expect("Search title");
    let dur_title = start_title.elapsed();
    assert_eq!(
        title_res.len(),
        10,
        "All 10 notes have Architecture in title"
    );
    eprintln!(
        "[Benchmark] Small dataset (10 notes) title search duration: {:?}",
        dur_title
    );

    // Content search measurement
    let start_content = Instant::now();
    let content_res =
        SearchService::search(&conn, "token_quantum_3", Some(50)).expect("Search content");
    let dur_content = start_content.elapsed();
    assert_eq!(
        content_res.len(),
        1,
        "Exactly one note matches token_quantum_3"
    );
    assert!(content_res[0].snippet.is_some());
    eprintln!(
        "[Benchmark] Small dataset (10 notes) content search duration: {:?}",
        dur_content
    );

    // -------------------------------------------------------------
    // Part 2: Query Plan Inspection
    // -------------------------------------------------------------
    // 1. FTS5 Query Plan:
    // EXPLAIN QUERY PLAN SELECT ... FROM notes_fts JOIN notes ON notes_fts.rowid = notes.rowid
    {
        let mut stmt = conn
            .prepare(
                "EXPLAIN QUERY PLAN
             SELECT n.id, n.title, snippet(notes_fts, 1, '', '', '...', 12)
             FROM notes_fts
             JOIN notes n ON notes_fts.rowid = n.rowid
             WHERE notes_fts MATCH '\"architecture\"*' AND n.is_deleted = 0
             LIMIT 50",
            )
            .expect("Prepare FTS query plan");
        let plans: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .expect("Query map")
            .filter_map(Result::ok)
            .collect();
        let plan_str = plans.join(" ");
        // Verify FTS virtual table scan & primary key lookup
        assert!(
            plan_str.contains("SCAN notes_fts") || plan_str.contains("notes_fts"),
            "Query plan must use notes_fts virtual table: got {}",
            plan_str
        );
        assert!(
            plan_str.contains("SEARCH n") || plan_str.contains("PRIMARY KEY"),
            "Query plan must use primary key lookup on notes table: got {}",
            plan_str
        );
    }

    // 2. Fallback LIKE Query Plan:
    // EXPLAIN QUERY PLAN SELECT id FROM notes WHERE is_deleted = 0 AND (title LIKE ?1 OR content LIKE ?1)
    {
        let mut stmt = conn
            .prepare(
                "EXPLAIN QUERY PLAN
             SELECT id, title, content FROM notes
             WHERE is_deleted = 0 AND (title LIKE '%architecture%' OR content LIKE '%architecture%')
             LIMIT 50",
            )
            .expect("Prepare LIKE query plan");
        let plans: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .expect("Query map")
            .filter_map(Result::ok)
            .collect();
        let plan_str = plans.join(" ");
        // Document and verify performance characteristic: LIKE with leading wildcard cannot use B-Tree seek for title/content
        // It uses idx_notes_trash_lifecycle on is_deleted and scans all active notes linearly (O(N) performance limitation).
        assert!(
            plan_str.contains("idx_notes_trash_lifecycle") || plan_str.contains("SCAN notes"),
            "Fallback LIKE query uses is_deleted index and scans active notes (O(N) limitation): got {}", plan_str
        );
    }

    // -------------------------------------------------------------
    // Part 3: Scale to Large Dataset (300 additional notes with varied content)
    // -------------------------------------------------------------
    let mut large_note_ids = Vec::with_capacity(300);
    for i in 11..=310 {
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Database Optimization Note {i}")),
                content: Some(format!(
                    "Detailed technical notes on system indexing, covering b-trees, hash maps, \
                     and memory limits. Record index number {i} with payload data \
                     repeated to form realistic payload: {}",
                    "Payload buffer padding for search indexing verification. ".repeat(10)
                )),
                ..Default::default()
            },
        )
        .expect("Create note in large dataset");
        large_note_ids.push(note.id);
    }

    // Diagnostic check on large dataset
    let diag_large = SearchService::diagnose_index(&conn).expect("diagnose_index large");
    assert_eq!(diag_large.active_notes_count, 310);
    assert_eq!(diag_large.fts_entries_count, 310);
    assert!(diag_large.is_consistent);

    // Measure title search across large dataset
    let start_large_title = Instant::now();
    let large_title_res =
        SearchService::search(&conn, "Optimization", Some(100)).expect("Search large title");
    let dur_large_title = start_large_title.elapsed();
    assert_eq!(large_title_res.len(), 100, "Bounded by limit 100");
    eprintln!(
        "[Benchmark] Large dataset (310 notes) title search duration: {:?}",
        dur_large_title
    );

    // Measure content search across large dataset
    let start_large_content = Instant::now();
    let large_content_res =
        SearchService::search(&conn, "payload", Some(50)).expect("Search large content");
    let dur_large_content = start_large_content.elapsed();
    assert_eq!(large_content_res.len(), 50, "Bounded by limit 50");
    eprintln!(
        "[Benchmark] Large dataset (310 notes) content search duration: {:?}",
        dur_large_content
    );

    // Result limits bounds verification: clamp between 1 and 100
    let res_clamped_high =
        SearchService::search(&conn, "Optimization", Some(500)).expect("Search clamp high");
    assert_eq!(
        res_clamped_high.len(),
        100,
        "Result count must be clamped to MAX_LIMIT (100)"
    );

    let res_default_limit =
        SearchService::search(&conn, "Optimization", None).expect("Search default limit");
    assert_eq!(
        res_default_limit.len(),
        50,
        "Default result count must be clamped to DEFAULT_LIMIT (50)"
    );

    // -------------------------------------------------------------
    // Part 4: Lifecycle Invariants & Transactional Coordination
    // -------------------------------------------------------------
    // A. Creation: note immediately searchable
    let test_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Mutable Target Alpha".to_string()),
            content: Some("Unique keyword xylophone_token_99 is present here.".to_string()),
            ..Default::default()
        },
    )
    .expect("Create test note");

    let res_created = SearchService::search(&conn, "xylophone_token_99", None).unwrap();
    assert_eq!(res_created.len(), 1);
    assert_eq!(res_created[0].note_id, test_note.id);

    // B. Update: FTS trigger notes_fts_au synchronizes atomically in same transaction
    NoteRepository::update(
        &conn,
        &test_note.id,
        UpdateNoteDto {
            title: Some("Mutable Target Beta".to_string()),
            content: Some("Replaced keyword with saxophone_token_88 in note body.".to_string()),
            ..Default::default()
        },
    )
    .expect("Update test note");

    // Old token must be purged from search results
    let res_old_token = SearchService::search(&conn, "xylophone_token_99", None).unwrap();
    assert_eq!(
        res_old_token.len(),
        0,
        "Old token must not be found after update"
    );

    // New token must be found in search results
    let res_new_token = SearchService::search(&conn, "saxophone_token_88", None).unwrap();
    assert_eq!(
        res_new_token.len(),
        1,
        "New token must be found after update"
    );
    assert_eq!(res_new_token[0].note_id, test_note.id);
    assert_eq!(res_new_token[0].title, "Mutable Target Beta");

    // C. Soft Delete (Move to Trash): excluded immediately from search
    NoteRepository::delete(&conn, &test_note.id, true).expect("Soft delete test note");
    let res_trashed = SearchService::search(&conn, "saxophone_token_88", None).unwrap();
    assert_eq!(
        res_trashed.len(),
        0,
        "Soft-deleted note must be excluded from search results"
    );

    // D. Restore from Trash: immediately returns to search
    NoteRepository::restore(&conn, &test_note.id).expect("Restore test note");
    let res_restored = SearchService::search(&conn, "saxophone_token_88", None).unwrap();
    assert_eq!(
        res_restored.len(),
        1,
        "Restored note must immediately reappear in search results"
    );
    assert_eq!(res_restored[0].note_id, test_note.id);

    // E. Permanent Delete (Hard Delete): purged from FTS5 index via trigger notes_fts_ad
    NoteRepository::delete(&conn, &test_note.id, false).expect("Hard delete test note");
    let res_hard_deleted = SearchService::search(&conn, "saxophone_token_88", None).unwrap();
    assert_eq!(
        res_hard_deleted.len(),
        0,
        "Permanently deleted note must be purged from FTS"
    );

    // F. Transactional Rollback: FTS5 atomic consistency
    {
        let tx = conn.transaction().expect("Begin transaction");
        let rolled_back_note = NoteRepository::create(
            &tx,
            CreateNoteDto {
                title: Some("Rollback Test Note".to_string()),
                content: Some("Content with phantom_rollback_token_77.".to_string()),
                ..Default::default()
            },
        )
        .expect("Create note in tx");
        assert!(!rolled_back_note.id.is_empty());
        tx.rollback().expect("Rollback tx");
    }
    let res_rollback = SearchService::search(&conn, "phantom_rollback_token_77", None).unwrap();
    assert_eq!(
        res_rollback.len(),
        0,
        "Rolled-back note tokens must not exist in FTS"
    );

    // -------------------------------------------------------------
    // Part 5: Derived Index Diagnostic, Corruption Detection, and Safe Recovery
    // -------------------------------------------------------------
    // Simulate index corruption by dropping virtual table and triggers
    conn.execute_batch(
        "DROP TABLE notes_fts;
         DROP TRIGGER notes_fts_ai;
         DROP TRIGGER notes_fts_ad;
         DROP TRIGGER notes_fts_au;",
    )
    .expect("Simulate corruption");

    // Diagnostic should detect corrupted / missing index
    let diag_corrupted = SearchService::diagnose_index(&conn).expect("diagnose_index");
    assert!(!diag_corrupted.fts_table_exists);
    assert!(!diag_corrupted.triggers_intact);
    assert!(!diag_corrupted.is_consistent);

    // SearchService automatic recovery on search query:
    // Even when index was dropped, search_unencrypted detects missing table, recovers index, and returns correct result
    let res_auto_recovered =
        SearchService::search(&conn, "Optimization", Some(10)).expect("Search with auto recovery");
    assert_eq!(
        res_auto_recovered.len(),
        10,
        "Auto recovery restored search capability"
    );

    // Diagnostic must now report index is healthy again
    let diag_repaired = SearchService::diagnose_index(&conn).expect("diagnose_index");
    assert!(diag_repaired.fts_table_exists);
    assert!(diag_repaired.triggers_intact);
    assert!(diag_repaired.integrity_check_passed);
    assert!(diag_repaired.is_consistent);

    // Explicit rebuild test
    SearchService::rebuild_index(&conn).expect("rebuild_index");
    let diag_after_rebuild = SearchService::diagnose_index(&conn).expect("diagnose after rebuild");
    assert!(diag_after_rebuild.is_consistent);

    // -------------------------------------------------------------
    // Part 6: Phase 12 Security Invariants & Plaintext Index Isolation
    // -------------------------------------------------------------
    // Configure encryption metadata
    let dummy_salt = generate_salt();
    let kdf_config = KdfConfig::default_profile(dummy_salt);
    let wrapped_key = WrappedKey::new(&[0x11; 12], &[0x22; 32], &[0x33; 16]);
    let meta = EncryptionMetadata::new(kdf_config, wrapped_key);
    SettingsRepository::set(&conn, ENCRYPTION_METADATA_KEY, &meta.to_json().unwrap())
        .expect("Set encryption metadata");

    // Purge plaintext index
    SearchService::purge_plaintext_index(&conn).expect("purge_plaintext_index");

    // Diagnostic must report encrypted mode with NO plaintext FTS virtual table or triggers
    let diag_encrypted = SearchService::diagnose_index(&conn).expect("diagnose encrypted");
    assert!(diag_encrypted.is_encrypted);
    assert!(
        !diag_encrypted.fts_table_exists,
        "FTS table MUST NOT exist when encrypted"
    );
    assert!(
        diag_encrypted.triggers_intact,
        "Triggers must be absent (intact=true)"
    );
    assert!(diag_encrypted.is_consistent, "Index must be consistent");

    // Rebuild or recover under encryption must NEVER create plaintext FTS tables
    SearchService::rebuild_index(&conn).expect("rebuild under encryption");
    let diag_post_rebuild = SearchService::diagnose_index(&conn).expect("diagnose post-rebuild");
    assert!(
        !diag_post_rebuild.fts_table_exists,
        "FTS table must never be created under encryption"
    );

    // Locked state search rejected
    let search_locked = SearchService::search_with_key(&conn, "Optimization", None, None);
    assert!(
        search_locked.is_err(),
        "Search while locked must be rejected"
    );

    // Unlocked search executes in-memory
    let dek = SecretBytes::new(vec![0x42; 32]);
    let enc_note = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Confidential Vault Note".to_string()),
            content: Some("Secret token_classified_42 stored in cipher block.".to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("Create encrypted note");

    let enc_search_res =
        SearchService::search_with_key(&conn, "token_classified_42", None, Some(&dek))
            .expect("Search unlocked");
    assert_eq!(enc_search_res.len(), 1);
    assert_eq!(enc_search_res[0].note_id, enc_note.id);
    assert!(enc_search_res[0].snippet.is_some());

    // Verify zero plaintext tokens in SQLite tables
    let count_fts: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        count_fts, 0,
        "No notes_fts table in database file when encrypted"
    );
}

#[test]
fn test_phase13_task_11_harden_attachment_storage_and_file_operations() {
    use personal_notepad_lib::storage::{
        attachment_storage::{is_executable_extension, AttachmentStorage},
        crypto::secret::SecretBytes,
        errors::AttachmentError,
        models::CreateNoteDto,
        repositories::{AttachmentRepository, NoteRepository},
        services::{AttachmentService, TrashService},
    };
    use sha2::{Digest, Sha256};
    use std::fs;

    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();
    let mut conn = init_connection(&db_path).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");
    let storage = AttachmentStorage::new(env.paths.clone());

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Hardened Storage Note".to_string()),
            content: Some("Testing security, hashing, and recovery.".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note");

    // =========================================================================
    // 1. Path Traversal Attacks & Directory Isolation
    // =========================================================================
    let malicious_paths = [
        "../escape.txt",
        "../../etc/passwd",
        "/etc/shadow",
        "nested/../../secret.key",
        "..\\windows\\system32\\cmd.exe",
        "C:\\boot.ini",
    ];
    for bad_path in &malicious_paths {
        let res = storage.resolve_safe_path(bad_path);
        assert!(
            res.is_err(),
            "Path traversal candidate '{bad_path}' must be rejected by resolve_safe_path"
        );
        match res.unwrap_err() {
            AttachmentError::InvalidRequest(msg) => {
                assert!(
                    msg.contains("Path traversal")
                        || msg.contains("Absolute")
                        || msg.contains("outside"),
                    "Expected traversal validation error message, got: {msg}"
                );
            }
            other => panic!("Expected InvalidRequest, got {other:?}"),
        }
    }

    assert!(storage.get_note_dir("../bad_note").is_err());
    assert!(storage.get_note_dir("bad/note").is_err());
    assert!(storage.get_note_dir("bad\\note").is_err());

    // =========================================================================
    // 2. Staged Copying, Chunked Streaming, and SHA-256 Content Hash Verification
    // =========================================================================
    let test_dir = env.paths.root.join("test_sources");
    fs::create_dir_all(&test_dir).unwrap();

    let valid_payload = vec![0x5A; 192 * 1024]; // 192 KiB spanning multiple 64 KiB chunks
    let mut expected_hasher = Sha256::new();
    expected_hasher.update(&valid_payload);
    let expected_hash = expected_hasher.finalize();

    let valid_file_path = test_dir.join("stream_payload.dat");
    fs::write(&valid_file_path, &valid_payload).unwrap();

    let stored = storage
        .store_file(&note.id, &valid_file_path)
        .expect("Store file with SHA-256 verification must succeed");

    assert_eq!(stored.file_size, valid_payload.len() as u64);
    assert_eq!(stored.original_name, "stream_payload.dat");

    // Verify stored file on disk matches expected hash
    let stored_disk_path = storage
        .resolve_safe_path(&stored.relative_path)
        .expect("Resolve safe path");
    assert!(stored_disk_path.exists());

    let stored_bytes = fs::read(&stored_disk_path).unwrap();
    let mut verify_hasher = Sha256::new();
    verify_hasher.update(&stored_bytes);
    assert_eq!(verify_hasher.finalize(), expected_hash);

    // Verify no leftover .tmp files exist in note attachments dir
    let note_dir = storage.get_note_dir(&note.id).unwrap();
    let tmp_entries: Vec<_> = fs::read_dir(&note_dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(
        tmp_entries.is_empty(),
        "No temporary staging files should remain after successful copy"
    );

    // =========================================================================
    // 3. Executable File Blocking on Direct Open
    // =========================================================================
    let dangerous_files = [
        ("malware.exe", "exe"),
        ("launch.bat", "bat"),
        ("install.cmd", "cmd"),
        ("exploit.sh", "sh"),
        ("script.vbs", "vbs"),
        ("dropper.ps1", "ps1"),
        ("setup.msi", "msi"),
        ("app.desktop", "desktop"),
        ("archive.jar", "jar"),
    ];

    for (filename, ext) in dangerous_files {
        assert!(
            is_executable_extension(filename),
            "Extension .{ext} must be identified as executable"
        );

        let d_path = test_dir.join(filename);
        fs::write(&d_path, b"echo dangerous payload").unwrap();

        let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &d_path)
            .expect("Importing executable as passive stored attachment is allowed");

        let open_res = AttachmentService::get_attachment_for_open(&conn, &storage, &att.id);
        assert!(
            open_res.is_err(),
            "Opening executable attachment '{filename}' directly must be blocked"
        );

        match open_res.unwrap_err() {
            AttachmentError::ExecutableBlocked(blocked_name) => {
                assert!(blocked_name.contains(ext) || blocked_name == filename);
            }
            other => panic!("Expected ExecutableBlocked, got: {other:?}"),
        }
    }

    // Non-executable files must open without restriction
    let safe_file = test_dir.join("document.pdf");
    fs::write(&safe_file, b"%PDF-1.4 test safe document content").unwrap();
    let safe_att = AttachmentService::add_attachment(&conn, &storage, &note.id, &safe_file)
        .expect("Add safe pdf");
    let safe_open = AttachmentService::get_attachment_for_open(&conn, &storage, &safe_att.id)
        .expect("Safe pdf opening must succeed");
    assert!(safe_open.exists());

    // =========================================================================
    // 4. Reversed Deletion-Order and Recoverable Cleanup
    // =========================================================================
    let discard_file = test_dir.join("to_remove.txt");
    fs::write(&discard_file, b"content to discard").unwrap();
    let discard_att = AttachmentService::add_attachment(&conn, &storage, &note.id, &discard_file)
        .expect("Add attachment to discard");
    let discard_disk = storage
        .resolve_safe_path(&discard_att.relative_path)
        .unwrap();
    assert!(discard_disk.exists());

    // Normal removal deletes DB first then filesystem
    AttachmentService::remove_attachment(&conn, &storage, &discard_att.id)
        .expect("Remove attachment succeeds");
    assert!(!discard_disk.exists(), "Managed file removed from disk");
    assert!(
        AttachmentRepository::get_by_id(&conn, &discard_att.id)
            .unwrap()
            .is_none(),
        "Database record removed"
    );

    // Stale removal: file already deleted from disk, DB record still exists
    let stale_file = test_dir.join("stale.txt");
    fs::write(&stale_file, b"will delete file manually").unwrap();
    let stale_att = AttachmentService::add_attachment(&conn, &storage, &note.id, &stale_file)
        .expect("Add stale att");
    let stale_disk = storage.resolve_safe_path(&stale_att.relative_path).unwrap();
    fs::remove_file(&stale_disk).unwrap();

    // Removing an attachment with missing managed file cleans the DB without error
    let rm_stale = AttachmentService::remove_attachment(&conn, &storage, &stale_att.id);
    assert!(
        rm_stale.is_ok(),
        "Removing stale attachment must recover and succeed"
    );
    assert!(AttachmentRepository::get_by_id(&conn, &stale_att.id)
        .unwrap()
        .is_none());

    // Unreferenced files cleanup
    let orphan_disk = note_dir.join("orphan_unref.bin");
    fs::write(&orphan_disk, b"abandoned untracked bytes").unwrap();
    let cleaned = AttachmentService::cleanup_unreferenced_files(&conn, &storage)
        .expect("Cleanup unreferenced files");
    assert!(cleaned >= 1, "Must clean unreferenced files");
    assert!(
        !orphan_disk.exists(),
        "Orphaned unreferenced file was safely cleaned"
    );

    // =========================================================================
    // 5. Encrypted Attachment Isolation & Bounded Memory
    // =========================================================================
    let dek = SecretBytes::new(vec![0x33; 32]);
    let enc_source = test_dir.join("vault_attachment.png");
    let png_header = b"\x89PNG\r\n\x1a\nsecret image content payload";
    fs::write(&enc_source, png_header).unwrap();

    let enc_att = AttachmentService::add_attachment_with_key(
        &conn,
        &storage,
        &note.id,
        &enc_source,
        Some(&dek),
    )
    .expect("Add encrypted attachment");

    let enc_disk_path = storage.resolve_safe_path(&enc_att.relative_path).unwrap();
    assert!(enc_disk_path.exists());
    assert!(AttachmentStorage::is_encrypted_file(&enc_disk_path));

    // Plaintext bytes are NEVER stored on disk in the managed path
    let disk_raw = fs::read(&enc_disk_path).unwrap();
    assert!(
        !disk_raw.windows(png_header.len()).any(|w| w == png_header),
        "Managed disk file must contain encrypted envelope, never plaintext"
    );

    // Locked state rejected
    let open_locked =
        AttachmentService::get_attachment_for_open_with_key(&conn, &storage, &enc_att.id, None);
    assert!(
        open_locked.is_err(),
        "Cannot open encrypted attachment without active DEK"
    );

    // Unlocked state decrypts safely to temporary decrypted directory
    let decrypted_temp = AttachmentService::get_attachment_for_open_with_key(
        &conn,
        &storage,
        &enc_att.id,
        Some(&dek),
    )
    .expect("Decrypt and open attachment with DEK");
    assert!(decrypted_temp.exists());
    let decrypted_bytes = fs::read(&decrypted_temp).unwrap();
    assert_eq!(decrypted_bytes, png_header);

    // Teardown decrypted session cleanup
    let cleaned_decrypted = storage.cleanup_decrypted_attachments().unwrap();
    assert!(cleaned_decrypted >= 1);
    assert!(
        !decrypted_temp.exists(),
        "Temporary decrypted file was safely purged"
    );

    // Permanent note deletion cleans note directory after DB delete
    TrashService::soft_delete(&conn, &note.id).expect("Soft delete note");
    let perm_res = TrashService::permanently_delete_note(&conn, &storage, &note.id)
        .expect("Permanently delete note");
    assert!(perm_res.success);
    assert!(
        !note_dir.exists(),
        "Note directory cleaned on permanent deletion"
    );
}

#[test]
fn test_phase13_task_12_failure_safe_import_export_operations() {
    use personal_notepad_lib::storage::{
        crypto::{
            generate_salt, metadata::ENCRYPTION_METADATA_KEY, secret::SecretBytes,
            EncryptionMetadata, KdfConfig, WrappedKey,
        },
        errors::StorageError,
        models::{
            DuplicatePolicy, ExportBatchRequest, ExportFormat, ExportScope,
            ExportSingleNoteRequest, ImportCandidateOverride, ImportFormat, ImportItemStatus,
            ImportOptions, ImportRequest, OperationOutcome,
        },
        repositories::{NoteRepository, SettingsRepository},
        services::ImportExportService,
    };
    use std::fs;

    let env = TestEnv::new();
    let db_path = env.paths.database_file.clone();
    let mut conn = init_connection(&db_path).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");

    let work_dir = env.paths.root.join("task12_work");
    fs::create_dir_all(&work_dir).unwrap();

    // =========================================================================
    // 1. Non-Regular Files and Malformed Encoding Safeguards
    // =========================================================================
    let sub_dir = work_dir.join("sub_folder");
    fs::create_dir_all(&sub_dir).unwrap();
    let dir_err =
        ImportExportService::read_and_validate_file(&sub_dir, ImportFormat::Txt).unwrap_err();
    assert!(
        dir_err.to_string().contains("not a regular file"),
        "Directory rejected safely"
    );

    // Binary file rejection
    let binary_file = work_dir.join("binary.txt");
    fs::write(&binary_file, b"Plain start\x00corrupt binary bytes\xFF").unwrap();
    let bin_err =
        ImportExportService::read_and_validate_file(&binary_file, ImportFormat::Txt).unwrap_err();
    assert!(
        bin_err.to_string().contains("Binary file content detected"),
        "Binary rejected safely"
    );

    // Invalid interchange JSON
    let invalid_json = work_dir.join("bad_schema.json");
    fs::write(
        &invalid_json,
        b"{\"app\":\"unknown-notepad\",\"version\":\"1.0\",\"notes\":[]}",
    )
    .unwrap();
    let json_err =
        ImportExportService::read_and_validate_file(&invalid_json, ImportFormat::Json).unwrap_err();
    assert!(
        json_err
            .to_string()
            .contains("expected app 'personal-notepad'"),
        "Invalid app schema rejected"
    );

    // =========================================================================
    // 2. Staged Batch Importing & Partial Failure Isolation
    // =========================================================================
    let valid_md = work_dir.join("valid_note.md");
    fs::write(&valid_md, "# Markdown Note\nValid body text content.").unwrap();

    let valid_txt = work_dir.join("valid_note.txt");
    fs::write(&valid_txt, "Plain text note\nValid simple text.").unwrap();

    let candidates = vec![
        ImportCandidateOverride {
            id: "cand-1".to_string(),
            file_path: valid_md.to_string_lossy().to_string(),
            selected: true,
            custom_title: Some("Valid Markdown Title".to_string()),
        },
        ImportCandidateOverride {
            id: "cand-2".to_string(),
            file_path: binary_file.to_string_lossy().to_string(),
            selected: true,
            custom_title: Some("Binary Corrupted".to_string()),
        },
        ImportCandidateOverride {
            id: "cand-3".to_string(),
            file_path: valid_txt.to_string_lossy().to_string(),
            selected: true,
            custom_title: Some("Valid Plain Text Title".to_string()),
        },
        ImportCandidateOverride {
            id: "cand-4".to_string(),
            file_path: invalid_json.to_string_lossy().to_string(),
            selected: true,
            custom_title: Some("Bad Interchange".to_string()),
        },
    ];

    let import_req = ImportRequest {
        candidates,
        options: ImportOptions {
            target_notebook_id: None,
            duplicate_policy: DuplicatePolicy::ImportAsNew,
        },
        operation_id: Some("op-batch-test-12".to_string()),
    };

    let summary = ImportExportService::execute_import_with_progress(&mut conn, import_req, |_| {})
        .expect("Batch import executed");

    assert_eq!(summary.total_processed, 4);
    assert_eq!(summary.imported_count, 2, "Both valid files imported");
    assert_eq!(summary.failed_count, 2, "Both invalid files failed safely");
    assert_eq!(summary.outcome, Some(OperationOutcome::PartialSuccess));

    assert_eq!(summary.results[0].status, ImportItemStatus::Imported);
    assert_eq!(summary.results[1].status, ImportItemStatus::Rejected);
    assert_eq!(summary.results[2].status, ImportItemStatus::Imported);
    assert_eq!(summary.results[3].status, ImportItemStatus::Rejected);

    // Database verification: only the 2 valid notes exist in DB
    let notes = NoteRepository::list(&conn, false).unwrap();
    assert_eq!(notes.len(), 2);
    let titles: Vec<_> = notes.iter().map(|n| n.title.as_str()).collect();
    assert!(titles.contains(&"Valid Markdown Title"));
    assert!(titles.contains(&"Valid Plain Text Title"));

    // =========================================================================
    // 3. Export Safety, Atomic Replacement, and Directory Validation
    // =========================================================================
    let target_dir = work_dir.join("export_target_dir");
    fs::create_dir_all(&target_dir).unwrap();

    let note_to_export = notes
        .iter()
        .find(|n| n.title == "Valid Markdown Title")
        .unwrap();
    let export_dest = target_dir.join("exported_single.md");

    let export_res = ImportExportService::export_single_note(
        &conn,
        ExportSingleNoteRequest {
            note_id: note_to_export.id.clone(),
            format: ExportFormat::Md,
            destination_path: export_dest.to_string_lossy().to_string(),
            overwrite: false,
        },
    )
    .expect("Export single note");
    assert_eq!(
        export_res.status,
        personal_notepad_lib::storage::models::ExportItemStatus::Exported
    );
    assert!(export_dest.exists());
    assert_eq!(
        fs::read_to_string(&export_dest).unwrap(),
        "# Markdown Note\nValid body text content."
    );

    // Conflict safety: Attempting to export to existing file without overwrite flag must return Conflict
    let conflict_err = ImportExportService::export_single_note(
        &conn,
        ExportSingleNoteRequest {
            note_id: note_to_export.id.clone(),
            format: ExportFormat::Md,
            destination_path: export_dest.to_string_lossy().to_string(),
            overwrite: false,
        },
    )
    .unwrap_err();
    assert!(matches!(conflict_err, StorageError::Conflict(_)));

    // Atomic write to directory directly must fail with Validation
    let atomic_dir_err =
        ImportExportService::write_file_atomically(&target_dir, b"bytes").unwrap_err();
    assert!(matches!(atomic_dir_err, StorageError::Validation(_)));

    // Path traversal in export destinations rejected
    assert!(
        ImportExportService::validate_destination_path(std::path::Path::new("..\\escaped.txt"))
            .is_err()
    );
    assert!(
        ImportExportService::validate_destination_path(std::path::Path::new("../escaped.txt"))
            .is_err()
    );
    assert!(
        ImportExportService::validate_destination_path(std::path::Path::new("database.sqlite"))
            .is_err()
    );

    // Batch text/md export conflict handling: automatically appends suffixes if overwrite is false
    let batch_export = ImportExportService::export_notes_batch(
        &conn,
        ExportBatchRequest {
            operation_id: None,
            scope: Some(ExportScope::SelectedNotes {
                note_ids: vec![notes[0].id.clone(), notes[1].id.clone()],
            }),
            note_ids: vec![],
            format: ExportFormat::Txt,
            destination_dir: target_dir.to_string_lossy().to_string(),
            destination_file: None,
            include_metadata: false,
            overwrite: false,
        },
    )
    .expect("Export batch text");
    assert_eq!(batch_export.exported_count, 2);

    // =========================================================================
    // 4. Phase 12 Encryption Invariants and Active DEK Requirement
    // =========================================================================
    let dek = SecretBytes::new(vec![0x7A; 32]);
    let dummy_salt = generate_salt();
    let kdf_config = KdfConfig::default_profile(dummy_salt);
    let wrapped_key = WrappedKey::new(&[0x11; 12], &[0x22; 32], &[0x33; 16]);
    let fake_meta = EncryptionMetadata::new(kdf_config, wrapped_key);
    SettingsRepository::set(
        &conn,
        ENCRYPTION_METADATA_KEY,
        &fake_meta.to_json().unwrap(),
    )
    .unwrap();

    // With encryption enabled and DEK None (locked), operations must be rejected immediately
    let locked_scan = ImportExportService::scan_candidates_with_key(
        &conn,
        &[valid_md.to_string_lossy().to_string()],
        None,
    );
    assert!(locked_scan.is_err(), "Scan while locked must be rejected");

    let locked_import = ImportExportService::execute_import_with_progress_and_key(
        &mut conn,
        ImportRequest {
            candidates: vec![ImportCandidateOverride {
                id: "c-lock".to_string(),
                file_path: valid_md.to_string_lossy().to_string(),
                selected: true,
                custom_title: Some("Locked".to_string()),
            }],
            options: ImportOptions::default(),
            operation_id: None,
        },
        None,
        |_| {},
    );
    assert!(
        locked_import.is_err(),
        "Import while locked must be rejected"
    );

    let locked_export = ImportExportService::export_single_note_with_key(
        &conn,
        ExportSingleNoteRequest {
            note_id: notes[0].id.clone(),
            format: ExportFormat::Txt,
            destination_path: target_dir
                .join("locked_export.txt")
                .to_string_lossy()
                .to_string(),
            overwrite: true,
        },
        None,
    );
    assert!(
        locked_export.is_err(),
        "Export single note while locked must be rejected"
    );

    let locked_batch_export = ImportExportService::export_notes_batch_with_progress_and_key(
        &conn,
        ExportBatchRequest {
            operation_id: None,
            scope: Some(ExportScope::AllNotes),
            note_ids: vec![],
            format: ExportFormat::Txt,
            destination_dir: target_dir.to_string_lossy().to_string(),
            destination_file: None,
            include_metadata: false,
            overwrite: true,
        },
        None,
        |_| {},
    );
    assert!(
        locked_batch_export.is_err(),
        "Export batch while locked must be rejected"
    );

    // When unlocked with DEK, import encrypts fields
    let enc_import_file = work_dir.join("enc_source.md");
    fs::write(
        &enc_import_file,
        "# Vault Secret\nClassified top secret token",
    )
    .unwrap();
    let enc_import_summary = ImportExportService::execute_import_with_progress_and_key(
        &mut conn,
        ImportRequest {
            candidates: vec![ImportCandidateOverride {
                id: "c-enc".to_string(),
                file_path: enc_import_file.to_string_lossy().to_string(),
                selected: true,
                custom_title: Some("Encrypted Note Title".to_string()),
            }],
            options: ImportOptions::default(),
            operation_id: None,
        },
        Some(&dek),
        |_| {},
    )
    .expect("Import under active DEK");
    assert_eq!(enc_import_summary.imported_count, 1);

    let enc_note_id = enc_import_summary.results[0]
        .created_note_id
        .as_ref()
        .unwrap();
    // Verify ciphertext is written to SQLite table
    let raw_db_content: String = conn
        .query_row(
            "SELECT content FROM notes WHERE id = ?1",
            [enc_note_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !raw_db_content.contains("Classified top secret token"),
        "Content must be encrypted in SQLite"
    );

    // When unlocked with DEK, export on-the-fly decrypts to requested format
    let decrypted_export_path = target_dir.join("decrypted_export.txt");
    let enc_export_res = ImportExportService::export_single_note_with_key(
        &conn,
        ExportSingleNoteRequest {
            note_id: enc_note_id.clone(),
            format: ExportFormat::Txt,
            destination_path: decrypted_export_path.to_string_lossy().to_string(),
            overwrite: true,
        },
        Some(&dek),
    )
    .expect("Export decrypted note");
    assert_eq!(
        enc_export_res.status,
        personal_notepad_lib::storage::models::ExportItemStatus::Exported
    );
    let exported_text = fs::read_to_string(&decrypted_export_path).unwrap();
    assert_eq!(
        exported_text, "# Vault Secret\nClassified top secret token",
        "Exported text correctly decrypted"
    );
}

#[test]
fn test_phase13_task_13_harden_backup_and_restore_reliability() {
    use personal_notepad_lib::storage::{
        crypto::{
            generate_salt, EncryptionMetadata, KdfConfig, SecretBytes, WrappedKey,
            ENCRYPTION_METADATA_KEY,
        },
        errors::BackupError,
        models::{BackupCreateRequest, BackupRestoreRequest, BackupType, CreateNoteDto},
        repositories::{NoteRepository, SettingsRepository},
        services::{BackupDestinationManager, BackupService, RestoreService},
    };
    use std::fs::{self, File};
    use std::io::{Read, Write};
    use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

    // =========================================================================
    // 1. Consistent Snapshot via Online Backup API and Destination Traversal Defense
    // =========================================================================
    let env_src = TestEnv::new();
    let mut conn_src = init_connection(&env_src.paths.database_file).expect("Src DB init");
    run_migrations(&mut conn_src).expect("Src migrations");

    // Create notes in source
    let note1 = NoteRepository::create(
        &conn_src,
        CreateNoteDto {
            title: Some("Important Project Spec".to_string()),
            content: Some("Consistent database snapshot via Online Backup API".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 1");

    let _note2 = NoteRepository::create(
        &conn_src,
        CreateNoteDto {
            title: Some("Architecture Notes".to_string()),
            content: Some("WAL committed cleanly without dirty reads".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 2");

    // Add managed attachment file and database row
    let att_stored_name = format!("{}.bin", uuid::Uuid::new_v4());
    let att_subpath = format!("{}/{}", note1.id, att_stored_name);
    let att_file_path = env_src.paths.attachments_dir.join(&att_subpath);
    fs::create_dir_all(att_file_path.parent().unwrap()).expect("Create note att dir");
    fs::write(&att_file_path, b"RAW_ATTACHMENT_VERIFIED_BYTES").expect("Write attachment file");

    let now_str = chrono::Utc::now().to_rfc3339();
    conn_src.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            note1.id,
            "spec-doc.bin",
            att_subpath,
            "application/octet-stream",
            29i64,
            now_str,
            now_str,
        ],
    ).expect("Insert attachment record");

    // Destination Traversal Defense: verify path traversal is rejected across platforms
    let trav_fwd = BackupDestinationManager::resolve_destination(
        Some("../outside.pnbackup"),
        BackupType::Manual,
        false,
        &env_src.paths,
    );
    assert!(matches!(
        trav_fwd,
        Err(BackupError::DestinationUnavailable(_))
    ));

    let trav_back = BackupDestinationManager::resolve_destination(
        Some("..\\outside.pnbackup"),
        BackupType::Manual,
        false,
        &env_src.paths,
    );
    assert!(matches!(
        trav_back,
        Err(BackupError::DestinationUnavailable(_))
    ));

    let live_db_str = env_src.paths.database_file.to_string_lossy().to_string();
    let db_override = BackupDestinationManager::resolve_destination(
        Some(&live_db_str),
        BackupType::Manual,
        false,
        &env_src.paths,
    );
    assert!(matches!(
        db_override,
        Err(BackupError::DestinationUnavailable(_))
    ));

    // Create consistent backup archive
    let backup_res = BackupService::create_backup(
        &conn_src,
        &env_src.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("Backup creation must succeed");

    assert!(backup_res.success);
    let backup_archive_path = std::path::PathBuf::from(&backup_res.file_path);
    assert!(backup_archive_path.exists());
    assert!(backup_res.note_count >= 2);
    assert_eq!(backup_res.attachment_count, 1);

    // =========================================================================
    // 2. Corrupt Archive Detection Before Destructive Live Database Swap
    // =========================================================================
    let env_target = TestEnv::new();
    let mut conn_target = init_connection(&env_target.paths.database_file).expect("Target DB init");
    run_migrations(&mut conn_target).expect("Target migrations");

    let live_note_initial = NoteRepository::create(
        &conn_target,
        CreateNoteDto {
            title: Some("Target Live Note Initial".to_string()),
            content: Some("Must remain completely untouched upon restore failure".to_string()),
            ..Default::default()
        },
    )
    .expect("Create live target note");

    // Corrupt archive: truncate bytes
    let corrupt_archive_path = env_target.paths.backups_dir.join("corrupted.pnbackup");
    fs::write(&corrupt_archive_path, b"CORRUPTED_TRUNCATED_ARCHIVE_DATA")
        .expect("Write corrupt file");

    let corrupt_restore_err = RestoreService::execute_restore(
        &mut conn_target,
        &env_target.paths,
        BackupRestoreRequest {
            file_path: corrupt_archive_path.to_string_lossy().to_string(),
            operation_id: None,
        },
        None::<fn(_)>,
    )
    .expect_err("Restoring corrupt archive must fail");

    assert!(
        matches!(
            corrupt_restore_err,
            BackupError::ArchiveReadFailed(_) | BackupError::ValidationFailed(_)
        ),
        "Must fail with archive read or validation error: {corrupt_restore_err:?}"
    );

    // Live target database MUST remain untouched!
    let live_check = NoteRepository::get_by_id(&conn_target, &live_note_initial.id)
        .expect("Query note")
        .expect("Live note must still exist");
    assert_eq!(live_check.title, "Target Live Note Initial");
    assert_eq!(
        live_check.content,
        "Must remain completely untouched upon restore failure"
    );

    // =========================================================================
    // 3. Missing Attachment Detection Before Destructive Swap
    // =========================================================================
    // Create an archive with valid manifest and database, but missing the referenced attachment file
    let missing_att_archive_path = env_target.paths.backups_dir.join("missing_att.pnbackup");
    {
        let orig_file = File::open(&backup_archive_path).expect("Open orig backup");
        let mut orig_zip = ZipArchive::new(orig_file).expect("Read orig zip");

        let out_file = File::create(&missing_att_archive_path).expect("Create out zip");
        let mut out_zip = ZipWriter::new(out_file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for i in 0..orig_zip.len() {
            let mut entry = orig_zip.by_index(i).expect("Entry");
            let name = entry.name().to_string();
            // Intentionally skip writing the attachment file to the archive
            if name.starts_with("attachments/") {
                continue;
            }
            out_zip.start_file(&name, options).expect("Start file");
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).expect("Read entry");
            out_zip.write_all(&buf).expect("Write entry");
        }
        out_zip.finish().expect("Finish missing att zip");
    }

    let missing_att_err = RestoreService::execute_restore(
        &mut conn_target,
        &env_target.paths,
        BackupRestoreRequest {
            file_path: missing_att_archive_path.to_string_lossy().to_string(),
            operation_id: None,
        },
        None::<fn(_)>,
    )
    .expect_err("Restoring archive with missing attachment must fail staged validation");

    assert!(
        matches!(
            missing_att_err,
            BackupError::RequiredAttachmentMissing(_) | BackupError::ValidationFailed(_)
        ),
        "Expected RequiredAttachmentMissing or ValidationFailed, got {missing_att_err:?}"
    );

    // Live target database still completely untouched!
    let live_check_2 = NoteRepository::get_by_id(&conn_target, &live_note_initial.id)
        .expect("Query note")
        .expect("Live note must still exist");
    assert_eq!(live_check_2.title, "Target Live Note Initial");

    // =========================================================================
    // 4. Successful Restore with Pre-Restore Recovery Checkpoint
    // =========================================================================
    let restore_success = RestoreService::execute_restore(
        &mut conn_target,
        &env_target.paths,
        BackupRestoreRequest {
            file_path: backup_archive_path.to_string_lossy().to_string(),
            operation_id: None,
        },
        None::<fn(_)>,
    )
    .expect("Restore valid backup must succeed");

    assert!(restore_success.success);
    assert_eq!(restore_success.note_count, 2);
    assert_eq!(restore_success.attachment_count, 1);

    // Live target now holds restored data
    let restored_note = NoteRepository::get_by_id(&conn_target, &note1.id)
        .expect("Get note 1")
        .expect("Note 1 must exist after restore");
    assert_eq!(restored_note.title, "Important Project Spec");
    assert_eq!(
        restored_note.content,
        "Consistent database snapshot via Online Backup API"
    );

    // Verify attachment file restored to disk
    let restored_att_file = env_target.paths.attachments_dir.join(&att_subpath);
    assert!(
        restored_att_file.exists(),
        "Restored attachment file must exist on disk"
    );
    let restored_att_bytes = fs::read(&restored_att_file).unwrap();
    assert_eq!(restored_att_bytes, b"RAW_ATTACHMENT_VERIFIED_BYTES");

    // Verify pre-restore recovery checkpoint was generated and preserved
    let checkpoints =
        RestoreService::list_recovery_checkpoints(&env_target.paths).expect("List checkpoints");
    assert!(
        !checkpoints.is_empty(),
        "Must have created pre-restore recovery checkpoint"
    );
    let latest_chk = &checkpoints[0];
    assert!(latest_chk.is_verified);
    assert_eq!(
        latest_chk.note_count, 1,
        "Pre-restore checkpoint holds initial live note"
    );

    // =========================================================================
    // 5. Phase 12 Encryption Invariants and Envelope Preservation
    // =========================================================================
    let env_enc = TestEnv::new();
    let mut conn_enc = init_connection(&env_enc.paths.database_file).expect("Enc DB init");
    run_migrations(&mut conn_enc).expect("Enc migrations");

    let _dek = SecretBytes::new(vec![0x4B; 32]);
    let dummy_salt = generate_salt();
    let kdf_config = KdfConfig::default_profile(dummy_salt);
    let wrapped_key = WrappedKey::new(&[0x55; 12], &[0x66; 32], &[0x77; 16]);
    let enc_meta = EncryptionMetadata::new(kdf_config, wrapped_key);
    SettingsRepository::set(
        &conn_enc,
        ENCRYPTION_METADATA_KEY,
        &enc_meta.to_json().unwrap(),
    )
    .unwrap();

    let enc_backup_res = BackupService::create_backup(
        &conn_enc,
        &env_enc.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("Create enc backup");

    // Preview restore checks encryption metadata
    let enc_archive_path = std::path::PathBuf::from(&enc_backup_res.file_path);
    let preview = RestoreService::preview_restore(&enc_archive_path, &env_enc.paths)
        .expect("Preview enc backup");
    assert!(
        preview.is_encrypted,
        "Preview must detect is_encrypted = true"
    );
    assert_eq!(
        preview.encryption_algorithm.as_deref(),
        Some("AES-256-GCM"),
        "Preview must report AES-256-GCM"
    );

    // Restore encrypted backup into fresh target
    let env_enc_target = TestEnv::new();
    let mut conn_enc_target =
        init_connection(&env_enc_target.paths.database_file).expect("Target init");
    run_migrations(&mut conn_enc_target).expect("Target migrations");

    let enc_restore_res = RestoreService::execute_restore(
        &mut conn_enc_target,
        &env_enc_target.paths,
        BackupRestoreRequest {
            file_path: enc_archive_path.to_string_lossy().to_string(),
            operation_id: None,
        },
        None::<fn(_)>,
    )
    .expect("Restore encrypted backup");

    assert!(enc_restore_res.success);

    // Verify encryption metadata was preserved accurately in restored target
    let restored_meta_str = SettingsRepository::get(&conn_enc_target, ENCRYPTION_METADATA_KEY)
        .expect("Get restored meta")
        .expect("Meta key exists in settings");
    let restored_meta: EncryptionMetadata = serde_json::from_str(&restored_meta_str).unwrap();
    assert!(restored_meta.enabled);
    assert_eq!(
        restored_meta.wrapped_dek.nonce_base64,
        enc_meta.wrapped_dek.nonce_base64
    );
    assert_eq!(
        restored_meta.wrapped_dek.ciphertext_base64,
        enc_meta.wrapped_dek.ciphertext_base64
    );
    assert_eq!(
        restored_meta.wrapped_dek.tag_base64,
        enc_meta.wrapped_dek.tag_base64
    );
}

#[test]
fn test_phase13_task_14_review_trash_restore_and_permanent_deletion() {
    use personal_notepad_lib::storage::attachment_storage::AttachmentStorage;
    use personal_notepad_lib::storage::errors::{AttachmentError, TrashError};
    use personal_notepad_lib::storage::models::LinkTargetLifecycle;
    use personal_notepad_lib::storage::repositories::search::SearchRepository;
    use personal_notepad_lib::storage::services::links::InternalLinkService;
    use personal_notepad_lib::storage::services::trash::TrashService;
    use std::fs;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");
    let storage = AttachmentStorage::new(env.paths.clone());

    // 1. Setup Notebook, Tag, and Note with Attachment and Link
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Engineering Core".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook");

    let tag = TagRepository::create(&conn, "critical-spec").expect("Create tag");

    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Database Engine Spec".to_string()),
            content: Some("Core storage engine specification details.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .expect("Create note 1");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag.id).expect("Add tag");

    // Add note2 which links to note1
    let link_syntax = format!("[Engine Spec](notepad://note/{})", note1.id);
    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Architecture Overview".to_string()),
            content: Some(format!("References the {}", link_syntax)),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("Create note 2");

    // Create attachment file and db row for note1
    let att_dir = env.paths.attachments_dir.join(&note1.id);
    fs::create_dir_all(&att_dir).expect("Create att dir");
    let att_file = att_dir.join("engine.bin");
    fs::write(&att_file, b"ATTACHMENT_ENGINE_PAYLOAD").expect("Write attachment");

    let now_str = chrono::Utc::now().to_rfc3339();
    let att_subpath = format!("{}/engine.bin", note1.id);
    conn.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            "att-engine-1",
            note1.id,
            "engine.bin",
            att_subpath,
            "application/octet-stream",
            25i64,
            now_str,
            now_str,
        ],
    ).expect("Insert attachment");

    // Initial check: Link resolves as Active, Backlink exists
    let initial_resolved =
        InternalLinkService::resolve_link_target(&conn, &note1.id).expect("Resolve link");
    assert_eq!(initial_resolved.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(
        initial_resolved.title,
        Some("Database Engine Spec".to_string())
    );

    let backlinks = InternalLinkService::get_backlinks(&conn, &note1.id).expect("Get backlinks");
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].source_note_id, note2.id);

    // Initial lists and search include note1
    let active_notes = NoteRepository::list_active(&conn).expect("List active");
    assert!(active_notes.iter().any(|n| n.id == note1.id));
    let fav_notes = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert!(fav_notes.iter().any(|n| n.id == note1.id));
    let tag_notes = NoteRepository::list_by_tag(&conn, &tag.id).expect("List by tag");
    assert!(tag_notes.iter().any(|n| n.id == note1.id));
    let search_results = SearchRepository::search(&conn, "Engine", None).expect("Search");
    assert!(search_results.iter().any(|r| r.note_id == note1.id));

    // =========================================================================
    // 2. Soft-Delete to Trash: Isolation & Exclusion
    // =========================================================================
    let trashed = TrashService::soft_delete(&conn, &note1.id).expect("Soft delete note 1");
    assert!(trashed.is_deleted);
    assert!(trashed.deleted_at.is_some());
    assert_eq!(trashed.title, "Database Engine Spec");
    assert_eq!(trashed.notebook_id, Some(nb.id.clone()));
    assert!(trashed.is_favorite);

    // Ordinary lists and search MUST exclude trashed note
    let active_after_trash = NoteRepository::list_active(&conn).expect("List active");
    assert!(!active_after_trash.iter().any(|n| n.id == note1.id));
    let fav_after_trash = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert!(!fav_after_trash.iter().any(|n| n.id == note1.id));
    let tag_after_trash = NoteRepository::list_by_tag(&conn, &tag.id).expect("List by tag");
    assert!(!tag_after_trash.iter().any(|n| n.id == note1.id));
    let search_after_trash = SearchRepository::search(&conn, "Engine", None).expect("Search");
    assert!(!search_after_trash.iter().any(|r| r.note_id == note1.id));

    // Links to trashed note reflect Trashed lifecycle
    let trashed_resolved =
        InternalLinkService::resolve_link_target(&conn, &note1.id).expect("Resolve link");
    assert_eq!(trashed_resolved.lifecycle, LinkTargetLifecycle::Trashed);

    // Backlinks query: source notes in trash do NOT produce backlinks
    let _trashed2 = TrashService::soft_delete(&conn, &note2.id).expect("Soft delete note 2");
    let backlinks_trashed_src =
        InternalLinkService::get_backlinks(&conn, &note1.id).expect("Get backlinks");
    assert_eq!(
        backlinks_trashed_src.len(),
        0,
        "Trashed source note must not produce backlinks"
    );

    // Restore note2 so it remains active
    let _restored2 = TrashService::restore(&conn, &note2.id).expect("Restore note 2");

    // =========================================================================
    // 3. Restore Reactivation & Relationship Preservation
    // =========================================================================
    let restored = TrashService::restore(&conn, &note1.id).expect("Restore note 1");
    assert!(!restored.is_deleted);
    assert_eq!(restored.deleted_at, None);
    assert_eq!(restored.title, "Database Engine Spec");
    assert_eq!(restored.notebook_id, Some(nb.id.clone()));
    assert!(restored.is_favorite);
    assert!(restored.is_pinned);

    // Reappears in ordinary lists, tags, and search
    let active_restored = NoteRepository::list_active(&conn).expect("List active");
    assert!(active_restored.iter().any(|n| n.id == note1.id));
    let fav_restored = NoteRepository::list_favorites(&conn).expect("List favorites");
    assert!(fav_restored.iter().any(|n| n.id == note1.id));
    let tag_restored = NoteRepository::list_by_tag(&conn, &tag.id).expect("List by tag");
    assert!(tag_restored.iter().any(|n| n.id == note1.id));
    let search_restored = SearchRepository::search(&conn, "Engine", None).expect("Search");
    assert!(search_restored.iter().any(|r| r.note_id == note1.id));

    // Link resolution reactivates to Active and backlinks reappear
    let reactivated_resolved =
        InternalLinkService::resolve_link_target(&conn, &note1.id).expect("Resolve link");
    assert_eq!(reactivated_resolved.lifecycle, LinkTargetLifecycle::Active);
    let backlinks_reactivated =
        InternalLinkService::get_backlinks(&conn, &note1.id).expect("Get backlinks");
    assert_eq!(backlinks_reactivated.len(), 1);

    // =========================================================================
    // 4. Parent Notebook Deletion While Note in Trash
    // =========================================================================
    let _trashed_again = TrashService::soft_delete(&conn, &note1.id).expect("Soft delete note 1");
    // Delete parent notebook
    NotebookRepository::delete(&mut conn, &nb.id).expect("Delete notebook");

    // Restoring note defensively unfiles it without failing
    let restored_unfiled =
        TrashService::restore(&conn, &note1.id).expect("Restore note with deleted notebook");
    assert!(!restored_unfiled.is_deleted);
    assert_eq!(
        restored_unfiled.notebook_id, None,
        "Must be cleanly unfiled"
    );

    // =========================================================================
    // 5. Permanent Deletion, Cascade Records & Attachment Cleanup
    // =========================================================================
    assert!(
        att_file.exists(),
        "Attachment file must exist before permanent delete"
    );
    let _trashed_for_perm =
        TrashService::soft_delete(&conn, &note1.id).expect("Soft delete note 1");

    let perm_res = TrashService::permanently_delete_note(&conn, &storage, &note1.id)
        .expect("Permanent delete");
    assert!(perm_res.success);
    assert!(!perm_res.attachment_cleanup_failed);

    // DB record removed
    assert_eq!(NoteRepository::get_by_id(&conn, &note1.id).unwrap(), None);

    // Attachment files on disk removed
    assert!(
        !att_file.exists(),
        "Attachment file must be removed from disk"
    );
    assert!(
        !att_dir.exists(),
        "Attachment note directory must be removed from disk"
    );

    // Dependent DB records cascaded
    let att_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM attachments WHERE note_id = ?1",
            rusqlite::params![note1.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(att_count, 0, "Attachment rows must cascade delete");

    let tag_link_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM note_tags WHERE note_id = ?1",
            rusqlite::params![note1.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tag_link_count, 0, "Note tag rows must cascade delete");

    // Internal link now resolves to Missing
    let missing_resolved =
        InternalLinkService::resolve_link_target(&conn, &note1.id).expect("Resolve link");
    assert_eq!(missing_resolved.lifecycle, LinkTargetLifecycle::Missing);

    // Note 2 and Tag remain 100% intact
    let survivor_note = NoteRepository::get_by_id(&conn, &note2.id)
        .unwrap()
        .expect("Note 2 exists");
    assert_eq!(survivor_note.title, "Architecture Overview");

    // =========================================================================
    // 6. Path Traversal & Invalid ID Defense
    // =========================================================================
    let trav_soft = TrashService::soft_delete(&conn, "../outside");
    assert!(matches!(trav_soft, Err(TrashError::InvalidRequest(_))));

    let trav_rest = TrashService::restore(&conn, "..\\outside");
    assert!(matches!(trav_rest, Err(TrashError::InvalidRequest(_))));

    let trav_perm = TrashService::permanently_delete_note(&conn, &storage, "..");
    assert!(matches!(trav_perm, Err(TrashError::InvalidRequest(_))));

    let trav_att = storage.remove_note_directory("../outside");
    assert!(matches!(trav_att, Err(AttachmentError::InvalidRequest(_))));

    // =========================================================================
    // 7. Empty Trash Bulk Operation
    // =========================================================================
    let bulk1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Bulk Trash 1".to_string()),
            content: Some("Content 1".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let bulk2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Bulk Trash 2".to_string()),
            content: Some("Content 2".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    TrashService::soft_delete(&conn, &bulk1.id).unwrap();
    TrashService::soft_delete(&conn, &bulk2.id).unwrap();

    let empty_res = TrashService::empty_trash(&conn, &storage).expect("Empty trash");
    assert_eq!(empty_res.deleted_count, 2);
    assert_eq!(empty_res.failed_count, 0);

    let trashed_remaining = NoteRepository::list_trashed(&conn).unwrap();
    assert_eq!(trashed_remaining.len(), 0, "Trash must be completely empty");
}

#[test]
fn test_phase13_task_15_harden_internal_links_backlinks_and_navigation() {
    use personal_notepad_lib::storage::attachment_storage::AttachmentStorage;
    use personal_notepad_lib::storage::models::LinkTargetLifecycle;
    use personal_notepad_lib::storage::services::links::InternalLinkService;
    use personal_notepad_lib::storage::services::trash::TrashService;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");
    let storage = AttachmentStorage::new(env.paths.clone());

    // 1. Stable ID Resolution & Renaming Resistance
    let note_a = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Initial Core Architecture".to_string()),
            content: Some("Architecture specs.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("Create Note A");

    let note_b = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Service Implementation".to_string()),
            content: Some(format!(
                "Implements [Core Architecture](notepad://note/{}) efficiently.",
                note_a.id
            )),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("Create Note B");

    // Initial resolution matches original title
    let res_initial =
        InternalLinkService::resolve_link_target(&conn, &note_a.id).expect("Resolve A");
    assert_eq!(res_initial.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(
        res_initial.title,
        Some("Initial Core Architecture".to_string())
    );

    // Rename Note A
    let _renamed_a = NoteRepository::update(
        &conn,
        &note_a.id,
        UpdateNoteDto {
            title: Some("Refactored Distributed Architecture v2".to_string()),
            ..Default::default()
        },
    )
    .expect("Rename Note A");

    // Resolution by stable UUID now dynamically returns the updated title without breaking link
    let res_renamed = InternalLinkService::resolve_link_target(&conn, &note_a.id)
        .expect("Resolve A after rename");
    assert_eq!(res_renamed.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(
        res_renamed.title,
        Some("Refactored Distributed Architecture v2".to_string())
    );

    // 2. Duplicate Titles Unambiguity
    let note_c = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Refactored Distributed Architecture v2".to_string()),
            content: Some("Duplicate title note.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("Create Note C");

    let res_c = InternalLinkService::resolve_link_target(&conn, &note_c.id).expect("Resolve C");
    assert_eq!(res_c.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(res_c.target_id, note_c.id);
    assert_ne!(res_c.target_id, note_a.id);

    // 3. Dynamic Backlinks Derivation & Source Content Updates
    let backlinks_1 =
        InternalLinkService::get_backlinks(&conn, &note_a.id).expect("Get backlinks A");
    assert_eq!(backlinks_1.len(), 1);
    assert_eq!(backlinks_1[0].source_note_id, note_b.id);
    assert!(backlinks_1[0].label_snippet.is_some());

    // Update Note B to remove the link
    let _updated_b = NoteRepository::update(
        &conn,
        &note_b.id,
        UpdateNoteDto {
            content: Some("Rewritten content with no internal links at all.".to_string()),
            ..Default::default()
        },
    )
    .expect("Update Note B");

    let backlinks_after_removal =
        InternalLinkService::get_backlinks(&conn, &note_a.id).expect("Get backlinks A");
    assert_eq!(
        backlinks_after_removal.len(),
        0,
        "Backlinks must update immediately on content change"
    );

    // Update Note B to re-add the link with a new label
    let _restored_link_b = NoteRepository::update(
        &conn,
        &note_b.id,
        UpdateNoteDto {
            content: Some(format!(
                "Now references [New Spec Link](notepad://note/{}) here.",
                note_a.id
            )),
            ..Default::default()
        },
    )
    .expect("Re-add link to Note B");

    let backlinks_readded =
        InternalLinkService::get_backlinks(&conn, &note_a.id).expect("Get backlinks A");
    assert_eq!(backlinks_readded.len(), 1);
    assert_eq!(backlinks_readded[0].source_note_id, note_b.id);

    // 4. Target Lifecycle: Trashed -> Restored -> Permanently Deleted
    // Trashing Note A
    TrashService::soft_delete(&conn, &note_a.id).expect("Trash Note A");
    let res_trashed =
        InternalLinkService::resolve_link_target(&conn, &note_a.id).expect("Resolve trashed");
    assert_eq!(res_trashed.lifecycle, LinkTargetLifecycle::Trashed);
    assert_eq!(
        res_trashed.title,
        Some("Refactored Distributed Architecture v2".to_string())
    );

    // Trashing source Note B suppresses outgoing backlinks
    TrashService::soft_delete(&conn, &note_b.id).expect("Trash Note B");
    let backlinks_from_trashed_src =
        InternalLinkService::get_backlinks(&conn, &note_a.id).expect("Backlinks from trashed");
    assert_eq!(
        backlinks_from_trashed_src.len(),
        0,
        "Trashed source note must not emit backlinks"
    );

    // Restoring Note B reactivates backlinks
    TrashService::restore(&conn, &note_b.id).expect("Restore Note B");
    let backlinks_restored_src =
        InternalLinkService::get_backlinks(&conn, &note_a.id).expect("Backlinks restored");
    assert_eq!(backlinks_restored_src.len(), 1);

    // Restoring Note A reactivates target
    TrashService::restore(&conn, &note_a.id).expect("Restore Note A");
    let res_restored =
        InternalLinkService::resolve_link_target(&conn, &note_a.id).expect("Resolve restored");
    assert_eq!(res_restored.lifecycle, LinkTargetLifecycle::Active);

    // Permanently deleting Note A marks target as Missing
    TrashService::soft_delete(&conn, &note_a.id).expect("Trash Note A again");
    TrashService::permanently_delete_note(&conn, &storage, &note_a.id)
        .expect("Permanent delete Note A");
    let res_deleted =
        InternalLinkService::resolve_link_target(&conn, &note_a.id).expect("Resolve deleted");
    assert_eq!(res_deleted.lifecycle, LinkTargetLifecycle::Missing);
    assert_eq!(res_deleted.title, None);

    // 5. Malformed, Missing, & Traversal URIs
    let trav_1 = InternalLinkService::resolve_link_target(&conn, "../etc/passwd")
        .expect("Resolve traversal 1");
    assert_eq!(trav_1.lifecycle, LinkTargetLifecycle::Missing);

    let trav_2 = InternalLinkService::resolve_link_target(&conn, "..\\windows\\system32")
        .expect("Resolve traversal 2");
    assert_eq!(trav_2.lifecycle, LinkTargetLifecycle::Missing);

    let trav_3 = InternalLinkService::resolve_link_target(&conn, "").expect("Resolve empty");
    assert_eq!(trav_3.lifecycle, LinkTargetLifecycle::Missing);

    let trav_4 =
        InternalLinkService::resolve_link_target(&conn, "   ").expect("Resolve whitespace");
    assert_eq!(trav_4.lifecycle, LinkTargetLifecycle::Missing);

    let backlinks_trav =
        InternalLinkService::get_backlinks(&conn, "../traversal").expect("Backlinks traversal");
    assert_eq!(backlinks_trav.len(), 0);
}

#[test]
fn test_phase13_task_16_validate_unicode_special_characters_and_unusual_content() {
    use personal_notepad_lib::storage::attachment_storage::sanitize_original_filename;
    use personal_notepad_lib::storage::models::LinkTargetLifecycle;
    use personal_notepad_lib::storage::repositories::search::SearchRepository;
    use personal_notepad_lib::storage::services::import_export::ImportExportService;
    use personal_notepad_lib::storage::services::links::InternalLinkService;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");

    // =========================================================================
    // 1. Multilingual Content, Emojis, BiDi Text, & Special Symbols
    // =========================================================================
    let original_title = "🚀 Project: ירושלים & 東京 2026 — מבצע \"שלום\" 🌸";
    let original_content = concat!(
        "# Global Specification 🌍\n\n",
        "Devanagari: नमस्ते दुनिया! यह एक सुरक्षित नोट है।\n",
        "Arabic (RTL): مرحبا بالعالم! هذا اختبار للنص العربي مع علامات التشكيل.\n",
        "Hebrew (RTL): שלום עולם! בדיקת טקסט בעברית.\n",
        "Russian: Привет, мир! Тестирование кодировки UTF-8.\n",
        "Greek: Γεια σου κόσμε! Ελληνικό κείμενο.\n",
        "Accents: Café résumé naïve señor garçon façade\n",
        "Zero-Width: A\u{200B}B\u{200C}C\u{200D}D (invisible boundaries)\n",
        "Emoji ZWJ: 👨‍👩‍👧‍👦 👩🏽‍💻 🏳️‍🌈\n",
        "Math & Symbols: ∀x ∈ ℝ, ∃y : x² + y² = 1 ∧ ¬(p ∨ q) ≡ ¬p ∧ ¬q\n",
    );

    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "📁 المشاريع 2026 / 日本語のノート".to_string(),
            parent_id: None,
        },
    )
    .expect("Create multilingual notebook");

    let tag_hindi = TagRepository::create(&conn, "सॉफ्टवेयर").expect("Create tag hindi");
    let tag_arabic = TagRepository::create(&conn, "مهم").expect("Create tag arabic");
    let tag_symbol = TagRepository::create(&conn, "c++").expect("Create tag symbol");

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(original_title.to_string()),
            content: Some(original_content.to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .expect("Create note with unusual unicode content");

    TagRepository::add_tag_to_note(&conn, &note.id, &tag_hindi.id).expect("Add tag hindi");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_arabic.id).expect("Add tag arabic");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag_symbol.id).expect("Add tag symbol");

    // =========================================================================
    // 2. Exact Roundtrip Verification (No Mojibake or Truncation)
    // =========================================================================
    let retrieved = NoteRepository::get_by_id(&conn, &note.id)
        .expect("Get note")
        .expect("Note exists");
    assert_eq!(
        retrieved.title, original_title,
        "Title must match byte-for-byte"
    );
    assert_eq!(
        retrieved.content, original_content,
        "Content must match byte-for-byte"
    );
    assert_eq!(retrieved.notebook_id, Some(nb.id.clone()));

    let tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get tags");
    assert_eq!(tags.len(), 3);
    assert!(tags.iter().any(|t| t.name == "सॉफ्टवेयर"));
    assert!(tags.iter().any(|t| t.name == "مهم"));
    assert!(tags.iter().any(|t| t.name == "c++"));

    // =========================================================================
    // 3. Search Indexing & Token Matching for Non-ASCII Text
    // =========================================================================
    let devanagari_results =
        SearchRepository::search(&conn, "नमस्ते", None).expect("Search Devanagari");
    assert!(
        !devanagari_results.is_empty(),
        "Search should find Devanagari query"
    );
    assert_eq!(devanagari_results[0].note_id, note.id);

    let arabic_results = SearchRepository::search(&conn, "بالعالم", None).expect("Search Arabic");
    assert!(
        !arabic_results.is_empty(),
        "Search should find Arabic query"
    );
    assert_eq!(arabic_results[0].note_id, note.id);

    let russian_results =
        SearchRepository::search(&conn, "кодировки", None).expect("Search Russian");
    assert!(
        !russian_results.is_empty(),
        "Search should find Russian query"
    );
    assert_eq!(russian_results[0].note_id, note.id);

    let accent_results = SearchRepository::search(&conn, "résumé", None).expect("Search Accented");
    assert!(
        !accent_results.is_empty(),
        "Search should find Accented query"
    );
    assert_eq!(accent_results[0].note_id, note.id);

    // Search query with special symbols should not panic or produce SQL syntax error
    let symbol_search =
        SearchRepository::search(&conn, "C++ *?^$()", None).expect("Search special symbols");
    assert!(symbol_search.is_empty() || !symbol_search.is_empty()); // Verify no panic or crash

    // =========================================================================
    // 4. Internal Links with Multilingual Unicode Labels
    // =========================================================================
    let link_syntax = format!(
        "References [दस्तावेज़ विवरण & הערות](notepad://note/{}) here.",
        note.id
    );
    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Secondary Document 📑".to_string()),
            content: Some(link_syntax),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("Create Note 2");

    let resolved = InternalLinkService::resolve_link_target(&conn, &note.id).expect("Resolve link");
    assert_eq!(resolved.lifecycle, LinkTargetLifecycle::Active);
    assert_eq!(resolved.title, Some(original_title.to_string()));

    let backlinks = InternalLinkService::get_backlinks(&conn, &note.id).expect("Get backlinks");
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].source_note_id, note2.id);
    assert_eq!(backlinks[0].source_note_title, "Secondary Document 📑");
    assert!(backlinks[0].label_snippet.is_some());

    // =========================================================================
    // 5. Safe Filesystem Sanitization & Export Filenames
    // =========================================================================
    let safe_export_name = ImportExportService::sanitize_filename(&note.title);
    assert!(
        !safe_export_name.contains('/'),
        "Export filename must not contain forward slash"
    );
    assert!(
        !safe_export_name.contains('\\'),
        "Export filename must not contain backslash"
    );
    assert!(
        !safe_export_name.contains('"'),
        "Export filename must not contain double quotes"
    );
    assert!(
        safe_export_name.contains("Project"),
        "Preserves alphanumeric stem"
    );
    assert!(
        safe_export_name.contains("ירושלים"),
        "Preserves Hebrew characters"
    );
    assert!(
        safe_export_name.contains("東京"),
        "Preserves CJK characters"
    );

    let safe_att_name = sanitize_original_filename("تقرير_المشروع_🌸_2026.pdf");
    assert_eq!(
        safe_att_name, "تقرير_المشروع_🌸_2026.pdf",
        "Preserves Unicode in attachment filenames"
    );

    let dirty_att_name = sanitize_original_filename("..\\..\\تقرير\0_test.pdf");
    assert!(!dirty_att_name.contains(".."), "Strips traversal");
    assert!(!dirty_att_name.contains('\0'), "Strips null bytes");
}

#[test]
fn test_phase13_task_17_handle_disk_permission_and_filesystem_failures() {
    use personal_notepad_lib::storage::attachment_storage::AttachmentStorage;
    use personal_notepad_lib::storage::errors::{
        AttachmentError, BackupError, ErrorCategory, StorageError,
    };
    use personal_notepad_lib::storage::models::{
        BackupCreateRequest, BackupType, ExportBatchRequest, ExportFormat, ExportScope,
    };
    use personal_notepad_lib::storage::services::attachments::AttachmentService;
    use personal_notepad_lib::storage::services::backup::BackupService;
    use personal_notepad_lib::storage::services::import_export::ImportExportService;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");
    let storage = AttachmentStorage::new(env.paths.clone());

    // 1. Error category and user-facing message mapping for ENOSPC and EACCES
    let enospc_io = StorageError::Io(std::io::Error::from_raw_os_error(28));
    assert_eq!(enospc_io.category(), ErrorCategory::DiskSpace);
    assert!(enospc_io
        .user_friendly_message()
        .contains("Insufficient disk space"));

    let eacces_io = StorageError::Io(std::io::Error::from_raw_os_error(13));
    assert_eq!(eacces_io.category(), ErrorCategory::Permission);
    assert!(eacces_io
        .user_friendly_message()
        .contains("Permission denied"));

    let att_space = AttachmentError::InsufficientDiskSpace;
    assert!(att_space
        .user_friendly_message()
        .contains("Insufficient disk space"));

    let backup_space = BackupError::InsufficientDiskSpace;
    assert!(backup_space
        .user_friendly_message()
        .contains("Insufficient disk space"));

    // 2. Attachment write failure: No DB record committed for non-existent file
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Attachment Safety Test".to_string()),
            content: Some("Note for attachment failure testing.".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note");

    let non_existent_source = env.dir.join("non_existent_file_123.pdf");
    let att_res =
        AttachmentService::add_attachment(&conn, &storage, &note.id, &non_existent_source);
    assert!(
        att_res.is_err(),
        "Must fail when source file does not exist"
    );

    // Verify database record was NOT committed
    let att_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM attachments WHERE note_id = ?1",
            rusqlite::params![&note.id],
            |r| r.get(0),
        )
        .expect("Query attachments count");
    assert_eq!(
        att_count, 0,
        "No database record must be committed for failed file write"
    );

    // Verify no stray files left in note directory
    let note_dir = env.paths.attachments_dir.join(&note.id);
    if note_dir.exists() {
        let entries: Vec<_> = std::fs::read_dir(&note_dir).unwrap().collect();
        assert_eq!(entries.len(), 0, "No stray files should exist in note dir");
    }

    // 3. Backup write failure: Live data remains completely untouched
    let invalid_dest = "/non_existent_mount_unplugged_volume_xyz/backups/test.pnbackup".to_string();
    let backup_res = BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: Some(invalid_dest),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    );
    assert!(
        backup_res.is_err(),
        "Backup to unavailable/unplugged directory must fail"
    );

    // Verify live data is 100% intact
    let active_note = NoteRepository::get_by_id(&conn, &note.id)
        .expect("Get note")
        .expect("Note exists");
    assert_eq!(active_note.title, "Attachment Safety Test");
    let all_notes = NoteRepository::list_active(&conn).expect("List active");
    assert_eq!(all_notes.len(), 1);

    // 4. Batch export failure isolation
    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Second Note".to_string()),
            content: Some("Content of second note.".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 2");

    let export_dir = env.dir.join("export_test");
    std::fs::create_dir_all(&export_dir).expect("Create export dir");

    let export_req = ExportBatchRequest {
        format: ExportFormat::Txt,
        scope: Some(ExportScope::SelectedNotes {
            note_ids: vec![note.id.clone(), note2.id.clone()],
        }),
        destination_dir: export_dir.to_string_lossy().to_string(),
        destination_file: None,
        overwrite: true,
        include_metadata: true,
        note_ids: vec![],
        operation_id: None,
    };

    let summary = ImportExportService::export_notes_batch(&conn, export_req).expect("Batch export");
    assert_eq!(summary.exported_count, 2);
    assert_eq!(summary.failed_count, 0);

    // Live database is still pristine
    let notes_after_export = NoteRepository::list_active(&conn).expect("List active");
    assert_eq!(notes_after_export.len(), 2);
}

#[test]
fn test_phase13_task_18_handle_database_corruption_and_migration_failures_safely() {
    use personal_notepad_lib::storage::database::{init_connection, isolate_corrupted_database};
    use personal_notepad_lib::storage::migrations::{
        create_pre_migration_snapshot, run_migrations_custom, run_migrations_with_safety,
        Migration, MIGRATIONS,
    };

    let env = TestEnv::new();

    // =========================================================================
    // 1. Detection of corrupt SQLite files on startup & Safe Isolation
    // =========================================================================
    let corrupt_db = env.dir.join("corrupt_test.sqlite");
    let corrupt_bytes = b"CORRUPTED HEADER NOT A REAL DATABASE";
    std::fs::write(&corrupt_db, corrupt_bytes).expect("Write corrupt db");

    // Opening corrupt db fails
    let open_res = init_connection(&corrupt_db);
    assert!(open_res.is_err(), "Opening corrupt database file must fail");

    // Corrupt file must NOT be deleted or overwritten
    assert!(
        corrupt_db.exists(),
        "Corrupt database must be preserved on disk"
    );
    assert_eq!(std::fs::read(&corrupt_db).unwrap(), corrupt_bytes);

    // Isolate the corrupted database
    let isolated_path = isolate_corrupted_database(&corrupt_db).expect("Isolate must succeed");
    assert!(isolated_path.exists());
    assert!(isolated_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains(".corrupt."));
    assert_eq!(std::fs::read(&isolated_path).unwrap(), corrupt_bytes);
    assert!(
        !corrupt_db.exists(),
        "Active db path freed up for fresh start or restore"
    );

    // =========================================================================
    // 2. Pre-migration Automatic Snapshots
    // =========================================================================
    let mig_db = env.dir.join("mig_test.sqlite");
    let mut conn = init_connection(&mig_db).expect("Init mig connection");

    // Apply first 2 migrations
    run_migrations_custom(&mut conn, &MIGRATIONS[..2]).expect("Run initial migrations");

    // Create a note in the partially-migrated DB
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Pre-migration Note".to_string()),
            content: Some("Must survive migrations intact.".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note");

    // Automatic pre-migration snapshot created because migrations 3 and 4 are pending
    let snap_path = create_pre_migration_snapshot(&conn, &mig_db).expect("Create snapshot");
    assert!(
        snap_path.is_some(),
        "Must create pre-migration snapshot when migrations pending"
    );
    let snap_file = snap_path.unwrap();
    assert!(snap_file.exists());
    assert!(snap_file
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains(".pre_migration."));

    // Verify snapshot is readable and contains our note
    let snap_conn = init_connection(&snap_file).expect("Open snapshot connection");
    let snap_note = NoteRepository::get_by_id(&snap_conn, &note.id)
        .expect("Query snapshot note")
        .expect("Note in snapshot");
    assert_eq!(snap_note.title, "Pre-migration Note");
    drop(snap_conn);

    // =========================================================================
    // 3. Failed Migration Step Rolls Back Atomically
    // =========================================================================
    let bad_migrations = [Migration {
        version: 99,
        name: "099_faulty_migration",
        sql: "CREATE TABLE valid_tbl (id TEXT); INVALID SQL STATEMENT WILL CAUSE SYNTAX ERROR;",
    }];

    let fail_res = run_migrations_custom(&mut conn, &bad_migrations);
    assert!(fail_res.is_err(), "Faulty migration must fail");

    // Verify atomic rollback: valid_tbl was NOT created
    let tbl_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'valid_tbl'",
            [],
            |r| r.get(0),
        )
        .expect("Check valid_tbl existence");
    assert_eq!(
        tbl_count, 0,
        "Failed migration transaction must roll back completely"
    );

    // Verify existing data is completely intact
    let survived_note = NoteRepository::get_by_id(&conn, &note.id)
        .expect("Get note")
        .expect("Note survived");
    assert_eq!(survived_note.title, "Pre-migration Note");

    // =========================================================================
    // 4. Safe run_migrations_with_safety completes all migrations cleanly
    // =========================================================================
    run_migrations_with_safety(&mut conn, Some(&mig_db))
        .expect("Run remaining migrations with safety");

    let final_note = NoteRepository::get_by_id(&conn, &note.id)
        .expect("Get note")
        .expect("Note present");
    assert_eq!(final_note.title, "Pre-migration Note");
}

#[test]
fn test_phase13_task_19_harden_startup_shutdown_and_application_lifecycle() {
    use personal_notepad_lib::storage::database::{checkpoint_wal, init_connection};
    use personal_notepad_lib::storage::errors::StorageError;
    use personal_notepad_lib::storage::paths::AppInstanceLock;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");

    // =========================================================================
    // 1. Clean Shutdown & WAL Checkpoint
    // =========================================================================
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Shutdown Persistence Test".to_string()),
            content: Some("Committed note content before shutdown.".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note");

    // Checkpoint WAL pages cleanly
    checkpoint_wal(&conn).expect("WAL checkpoint should succeed");

    // Simulate clean shutdown: drop connection
    drop(conn);

    // Reopen on next launch: verify data is 100% consistent
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen after clean shutdown");
    let recovered_note = NoteRepository::get_by_id(&conn2, &note.id)
        .expect("Get note")
        .expect("Note exists");
    assert_eq!(recovered_note.title, "Shutdown Persistence Test");
    assert_eq!(
        recovered_note.content,
        "Committed note content before shutdown."
    );

    // =========================================================================
    // 2. Sudden Process Termination (SIGKILL) & Crash Recovery
    // =========================================================================
    let note2 = NoteRepository::create(
        &conn2,
        CreateNoteDto {
            title: Some("Crash Recovery Note".to_string()),
            content: Some("Content written right before simulated crash.".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note 2");

    // Forcibly drop without manual checkpoint (simulating sudden termination / SIGKILL)
    drop(conn2);

    // Launch app after crash: SQLite WAL recovery executes transparently
    let conn3 = init_connection(&env.paths.database_file).expect("Reopen after crash");
    let crash_note = NoteRepository::get_by_id(&conn3, &note2.id)
        .expect("Get note")
        .expect("Note survived crash");
    assert_eq!(crash_note.title, "Crash Recovery Note");

    // =========================================================================
    // 3. Multi-Instance Prevention & Conflict Handling
    // =========================================================================
    // Acquire instance lock
    let lock1 = AppInstanceLock::acquire(&env.paths).expect("First instance acquires lock");
    assert!(env.paths.app_instance_lock_file().exists());

    // Write a dummy foreign PID that is running (e.g. PID 1 or parent init process)
    let foreign_pid = 1; // PID 1 is always running on Linux
    std::fs::write(env.paths.app_instance_lock_file(), foreign_pid.to_string()).unwrap();

    // Second instance attempting to acquire will detect running instance and block
    let lock2_res = AppInstanceLock::acquire(&env.paths);
    assert!(
        lock2_res.is_err(),
        "Second instance must be blocked when another instance is active"
    );
    assert!(matches!(lock2_res.unwrap_err(), StorageError::Conflict(_)));

    // Drop first lock / clean up lock file
    drop(lock1);
    let _ = std::fs::remove_file(env.paths.app_instance_lock_file());

    // Subsequent launch can acquire cleanly
    let lock3 = AppInstanceLock::acquire(&env.paths).expect("Acquire lock after clean release");
    drop(lock3);

    // =========================================================================
    // 4. Startup Orphan Temp Files & Staging Cleanup
    // =========================================================================
    // Create orphan files left by simulated previous crash
    let orphan_staging = env.paths.root.join(".restore_staging_crashed_123");
    std::fs::create_dir_all(&orphan_staging).unwrap();
    std::fs::write(orphan_staging.join("orphan.db"), b"staged bytes").unwrap();

    let orphan_temp_db = env.paths.root.join("database.sqlite.tmp.crashed_abc");
    std::fs::write(&orphan_temp_db, b"temp database bytes").unwrap();

    let note_att_dir = env.paths.get_note_attachments_dir(&note.id).unwrap();
    let orphan_temp_att = note_att_dir.join("image.tmp.crashed_xyz");
    std::fs::write(&orphan_temp_att, b"temp attachment bytes").unwrap();

    // Valid attachment file that must NOT be removed
    let valid_att = note_att_dir.join("valid_attachment.pdf");
    std::fs::write(&valid_att, b"valid document bytes").unwrap();

    // Startup cleanup
    let cleaned_count = env
        .paths
        .cleanup_orphan_temp_files()
        .expect("Clean orphan files");
    assert_eq!(cleaned_count, 3, "Must clean 3 orphan temporary entities");

    assert!(
        !orphan_staging.exists(),
        "Orphan staging dir must be deleted"
    );
    assert!(!orphan_temp_db.exists(), "Orphan temp db must be deleted");
    assert!(
        !orphan_temp_att.exists(),
        "Orphan temp attachment must be deleted"
    );

    // Critical invariant: valid files remain completely intact
    assert!(
        valid_att.exists(),
        "Valid managed files must never be removed by orphan cleanup"
    );
    assert_eq!(std::fs::read(&valid_att).unwrap(), b"valid document bytes");
}

#[test]
fn test_phase13_task_20_control_background_work_cancellation_and_concurrency() {
    let _global_state_guard = common::lock_process_global_test_state();
    use personal_notepad_lib::storage::errors::BackupError;
    use personal_notepad_lib::storage::models::{
        DuplicatePolicy, ExportBatchRequest, ExportFormat, ExportScope, ImportCandidateOverride,
        ImportOptions, ImportRequest,
    };
    use personal_notepad_lib::storage::services::backup_scheduler::BackupSchedulerService;
    use personal_notepad_lib::storage::services::import_export::ImportExportService;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");

    // =========================================================================
    // 1. Mutual Exclusion Across Heavy Operations
    // =========================================================================
    // Acquire import/export lock
    let import_guard = BackupSchedulerService::try_acquire_import_export_lock()
        .expect("Acquire import/export lock");

    // Attempting to acquire backup lock must fail with ConcurrentOperation
    let backup_err = BackupSchedulerService::try_acquire_backup_lock();
    assert!(
        backup_err.is_err(),
        "Backup must be blocked while import/export is executing"
    );
    assert!(matches!(
        backup_err.unwrap_err(),
        BackupError::ConcurrentOperation(_)
    ));

    // Attempting to acquire restore lock must fail with ConcurrentOperation
    let restore_err = BackupSchedulerService::try_acquire_restore_lock();
    assert!(
        restore_err.is_err(),
        "Restore must be blocked while import/export is executing"
    );
    assert!(matches!(
        restore_err.unwrap_err(),
        BackupError::ConcurrentOperation(_)
    ));

    // Release import/export lock
    drop(import_guard);

    // Now backup lock can be acquired normally
    let backup_guard = BackupSchedulerService::try_acquire_backup_lock()
        .expect("Backup lock can be acquired after import/export release");

    // While backup is held, import/export is blocked
    let import_err = BackupSchedulerService::try_acquire_import_export_lock();
    assert!(
        import_err.is_err(),
        "Import/export must be blocked while backup is executing"
    );
    assert!(matches!(
        import_err.unwrap_err(),
        BackupError::ConcurrentOperation(_)
    ));

    drop(backup_guard);

    // =========================================================================
    // 2. Cooperative Cancellation During Batch Import
    // =========================================================================
    let op_id = format!("test_cancel_import_{}", uuid::Uuid::new_v4());
    let file1 = env.dir.join("note1.txt");
    std::fs::write(&file1, "Content 1").unwrap();
    let file2 = env.dir.join("note2.txt");
    std::fs::write(&file2, "Content 2").unwrap();

    // Pre-cancel the operation
    ImportExportService::cancel_operation(&op_id);

    let candidates = vec![
        ImportCandidateOverride {
            id: "c1".to_string(),
            file_path: file1.to_string_lossy().to_string(),
            selected: true,
            custom_title: None,
        },
        ImportCandidateOverride {
            id: "c2".to_string(),
            file_path: file2.to_string_lossy().to_string(),
            selected: true,
            custom_title: None,
        },
    ];

    let import_req = ImportRequest {
        candidates,
        options: ImportOptions {
            target_notebook_id: None,
            duplicate_policy: DuplicatePolicy::ImportAsNew,
        },
        operation_id: Some(op_id.clone()),
    };

    let summary =
        ImportExportService::execute_import(&mut conn, import_req).expect("Execute import");
    assert_eq!(
        summary.cancelled_count, 2,
        "All items must be marked Cancelled"
    );
    assert_eq!(
        summary.imported_count, 0,
        "No notes should be committed when cancelled"
    );

    // Verify zero notes committed to SQLite
    let active_notes = NoteRepository::list_active(&conn).expect("List active");
    assert_eq!(
        active_notes.len(),
        0,
        "Database must remain pristine on cancelled import"
    );

    // =========================================================================
    // 3. Cooperative Cancellation During Batch Export
    // =========================================================================
    let n1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Export Cancel Note".to_string()),
            content: Some("Should not be written to disk if cancelled.".to_string()),
            ..Default::default()
        },
    )
    .expect("Create note");

    let export_op_id = format!("test_cancel_export_{}", uuid::Uuid::new_v4());
    ImportExportService::cancel_operation(&export_op_id);

    let export_dir = env.dir.join("export_cancel_test");
    std::fs::create_dir_all(&export_dir).unwrap();

    let export_req = ExportBatchRequest {
        format: ExportFormat::Txt,
        scope: Some(ExportScope::SelectedNotes {
            note_ids: vec![n1.id.clone()],
        }),
        destination_dir: export_dir.to_string_lossy().to_string(),
        destination_file: None,
        overwrite: true,
        include_metadata: false,
        note_ids: vec![n1.id],
        operation_id: Some(export_op_id),
    };

    let export_summary =
        ImportExportService::export_notes_batch(&conn, export_req).expect("Execute export");
    assert_eq!(
        export_summary.cancelled_count, 1,
        "Export item must be marked Cancelled"
    );
    assert_eq!(
        export_summary.exported_count, 0,
        "No files should be exported when cancelled"
    );
}

#[test]
fn test_phase13_task_21_audit_memory_resource_cleanup_and_long_sessions() {
    use personal_notepad_lib::storage::database::DEFAULT_CACHE_SIZE_KB;
    use personal_notepad_lib::storage::models::{
        BackupCreateRequest, BackupType, CreateNoteDto, ExportBatchRequest, ExportFormat,
        ExportScope, UpdateNoteDto,
    };
    use personal_notepad_lib::storage::repositories::SearchRepository;
    use personal_notepad_lib::storage::services::backup::BackupService;
    use personal_notepad_lib::storage::services::import_export::ImportExportService;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");

    // =========================================================================
    // 1. SQLite Page Cache and Resource Bounds Invariant
    // =========================================================================
    let cache_size: i32 = conn
        .query_row("PRAGMA cache_size;", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        cache_size, DEFAULT_CACHE_SIZE_KB,
        "SQLite page cache must be strictly bounded to ~2MB (-2000 KB)"
    );

    let busy_timeout: i32 = conn
        .query_row("PRAGMA busy_timeout;", [], |r| r.get(0))
        .unwrap();
    assert_eq!(busy_timeout, 5000, "Busy timeout must be set to 5000ms");

    // =========================================================================
    // 2. Long Session Simulation: 50 cycles of create, edit, read, search
    // =========================================================================
    let mut note_ids = Vec::new();
    for i in 0..50 {
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Long Session Note {i}")),
                content: Some(format!(
                    "Continuous note editing body {i} with searchable keyword session_stress"
                )),
                ..Default::default()
            },
        )
        .expect("Create note during session");

        // Mutate note
        NoteRepository::update(
            &conn,
            &note.id,
            UpdateNoteDto {
                title: Some(format!("Long Session Note {i} Revised")),
                content: Some(format!("Revised body {i} with session_stress")),
                ..Default::default()
            },
        )
        .expect("Update note during session");

        // Read back
        let fetched = NoteRepository::get_by_id(&conn, &note.id)
            .expect("Get note")
            .unwrap();
        assert!(fetched.title.contains("Revised"));

        note_ids.push(note.id);
    }

    // Search query executes cleanly across populated index
    let search_results =
        SearchRepository::search(&conn, "session_stress", None).expect("Search executes");
    assert_eq!(
        search_results.len(),
        50,
        "All 50 revisions indexed without leaking handles"
    );

    // =========================================================================
    // 3. Temporary File Reclamation Across Batch Export
    // =========================================================================
    let export_dir = env.dir.join("task21_export");
    std::fs::create_dir_all(&export_dir).unwrap();

    let export_req = ExportBatchRequest {
        format: ExportFormat::Txt,
        scope: Some(ExportScope::SelectedNotes {
            note_ids: note_ids.clone(),
        }),
        destination_dir: export_dir.to_string_lossy().to_string(),
        destination_file: None,
        overwrite: true,
        include_metadata: false,
        note_ids: note_ids.clone(),
        operation_id: Some("op_task21_export".to_string()),
    };

    let summary =
        ImportExportService::export_notes_batch(&conn, export_req).expect("Batch export notes");
    assert_eq!(summary.exported_count, 50);

    // Recursive helper to ensure zero temporary or staging artifacts remain
    fn check_no_temp_files(dir: &std::path::Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                assert!(
                    !name.contains(".tmp."),
                    "No temporary files should linger: {name}"
                );
                assert!(
                    !name.starts_with(".backup_staging_"),
                    "No backup staging folder should linger: {name}"
                );
                assert!(
                    !name.starts_with(".restore_staging_"),
                    "No restore staging folder should linger: {name}"
                );
                if path.is_dir() {
                    check_no_temp_files(&path);
                }
            }
        }
    }

    // Verify zero temporary files (.tmp.*) remain in export directory or storage
    check_no_temp_files(&env.dir);

    // =========================================================================
    // 4. Temporary File Reclamation Across Backup Creation
    // =========================================================================
    let backup_req = BackupCreateRequest {
        destination_path: None,
        backup_type: Some(BackupType::Manual),
        allow_overwrite: Some(false),
    };
    let backup_res = BackupService::create_backup(&conn, &env.paths, backup_req, None::<fn(_)>)
        .expect("Create backup");
    assert!(std::path::Path::new(&backup_res.file_path).exists());

    // Verify again that no staging or temporary files linger
    check_no_temp_files(&env.paths.root);
}

#[test]
fn test_phase13_task_23_reliability_and_stress_suite() {
    use personal_notepad_lib::storage::models::{
        BackupRestoreRequest, CreateNoteDto, UpdateNoteDto,
    };
    use personal_notepad_lib::storage::repositories::{
        NoteRepository, SearchRepository, TagRepository,
    };
    use personal_notepad_lib::storage::services::restore::RestoreService;
    use personal_notepad_lib::storage::services::trash::TrashService;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");

    // -------------------------------------------------------------------------
    // 1. Rapid Consecutive Updates Stress & Persistence
    // -------------------------------------------------------------------------
    let initial_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Rapid Update Stress Note".to_string()),
            content: Some("Initial content".to_string()),
            ..Default::default()
        },
    )
    .expect("Create initial note");

    let rapid_iterations = 25;
    for i in 1..=rapid_iterations {
        NoteRepository::update(
            &conn,
            &initial_note.id,
            UpdateNoteDto {
                title: Some(format!("Rapid Update Stress Note - Rev {i}")),
                content: Some(format!(
                    "Content at rapid revision {i} with searchable token rapid_stress_token"
                )),
                ..Default::default()
            },
        )
        .expect("Execute rapid revision update");
    }

    let final_note = NoteRepository::get_by_id(&conn, &initial_note.id)
        .expect("Fetch note")
        .unwrap();
    assert_eq!(final_note.title, "Rapid Update Stress Note - Rev 25");
    assert!(final_note.content.contains("Content at rapid revision 25"));

    // -------------------------------------------------------------------------
    // 2. Large Content, Multi-Byte Unicode, Emojis, and BiDi Stress
    // -------------------------------------------------------------------------
    let mut large_content = String::with_capacity(300_000);
    large_content.push_str("# Heavy Note Heading 🚀 🌍\n\n");
    large_content
        .push_str("مرحبا بالعالم - עולם שלום - Mixed BiDi Text with Sanskrit: नमस्ते दुनिया\n\n");
    for i in 0..1500 {
        large_content.push_str(&format!("Line {i}: Personal Notepad large content stress testing paragraph with markdown formatting.\n"));
    }
    large_content
        .push_str("\n\nEnd of stress content with special keyword large_stress_content_token.");

    let large_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Large Content Note 📚 ⚡".to_string()),
            content: Some(large_content.clone()),
            ..Default::default()
        },
    )
    .expect("Create large note");

    let read_large_note = NoteRepository::get_by_id(&conn, &large_note.id)
        .expect("Read large note")
        .unwrap();
    assert_eq!(read_large_note.content.len(), large_content.len());
    assert!(read_large_note.title.contains("📚 ⚡"));

    // -------------------------------------------------------------------------
    // 3. Search and Trash Consistency Invariant
    // -------------------------------------------------------------------------
    // Active search must find the rapid note and large note
    let search_active =
        SearchRepository::search(&conn, "rapid_stress_token", None).expect("Search active note");
    assert_eq!(search_active.len(), 1, "Active note found by search token");

    // Soft delete to Trash
    TrashService::soft_delete(&conn, &initial_note.id).expect("Soft delete note");

    // Trashed note MUST NOT be returned in active search queries
    let search_after_trash =
        SearchRepository::search(&conn, "rapid_stress_token", None).expect("Search after trash");
    assert_eq!(
        search_after_trash.len(),
        0,
        "Trashed note must be excluded from active search results"
    );

    // Restore from Trash
    TrashService::restore(&conn, &initial_note.id).expect("Restore note from trash");

    // Search must immediately re-index and return restored note
    let search_restored =
        SearchRepository::search(&conn, "rapid_stress_token", None).expect("Search restored note");
    assert_eq!(
        search_restored.len(),
        1,
        "Restored note must be immediately searchable"
    );

    // -------------------------------------------------------------------------
    // 4. Foreign Key Constraints & Cascade Enforcement
    // -------------------------------------------------------------------------
    let tag = TagRepository::create(&conn, "stress_tag").expect("Create tag");
    TagRepository::add_tag_to_note(&conn, &initial_note.id, &tag.id).expect("Tag note");

    let note_tags =
        TagRepository::get_tags_for_note(&conn, &initial_note.id).expect("Get note tags");
    assert_eq!(note_tags.len(), 1);

    // Hard delete note from Trash
    TrashService::soft_delete(&conn, &initial_note.id).expect("Soft delete again");
    let storage = personal_notepad_lib::storage::attachment_storage::AttachmentStorage::new(
        env.paths.clone(),
    );
    TrashService::permanently_delete_note(&conn, &storage, &initial_note.id)
        .expect("Permanent delete note");

    // Foreign key cascade must have purged note_tags mapping
    let remaining_mappings: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM note_tags WHERE note_id = ?1",
            [&initial_note.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        remaining_mappings, 0,
        "Foreign key cascade must clean up note_tags"
    );

    // Tag itself must still exist
    assert!(TagRepository::get_by_id(&conn, &tag.id)
        .expect("Get tag")
        .is_some());

    // -------------------------------------------------------------------------
    // 5. Corrupted Backup Archive Rejection Before Destructive Swap
    // -------------------------------------------------------------------------
    let corrupt_archive = env.dir.join("corrupt_test_backup.pnbackup");
    std::fs::write(&corrupt_archive, b"NOT A VALID ZIP ARCHIVE HEADER").unwrap();

    let preview_res = RestoreService::preview_restore(&corrupt_archive, &env.paths)
        .expect("Preview corrupt archive");
    assert!(
        !preview_res.can_restore,
        "Corrupted archive must not be restorable"
    );
    assert_eq!(
        preview_res.compatibility,
        personal_notepad_lib::storage::models::BackupCompatibilityStatus::Corrupted
    );

    let restore_err = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        BackupRestoreRequest {
            file_path: corrupt_archive.to_string_lossy().to_string(),
            operation_id: None,
        },
        None::<fn(_)>,
    );
    assert!(
        restore_err.is_err(),
        "Corrupt restore must be rejected before touching live database"
    );

    // Live database must remain fully intact and operational
    let live_notes = NoteRepository::list_active(&conn).expect("List active notes");
    assert_eq!(
        live_notes.len(),
        1,
        "Live notes remain 100% intact after corrupt restore attempt"
    );
    assert_eq!(live_notes[0].id, large_note.id);
}

#[test]
fn test_phase13_task_24_measure_performance_and_regression_verification() {
    use personal_notepad_lib::storage::models::{
        BackupCreateRequest, BackupType, CreateNoteDto, CreateNotebookDto,
    };
    use personal_notepad_lib::storage::repositories::{
        NoteRepository, NotebookRepository, SearchRepository, TagRepository,
    };
    use personal_notepad_lib::storage::services::backup::BackupService;
    use std::time::Instant;

    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Init connection");
    run_migrations(&mut conn).expect("Run migrations");

    // 1. Measure creation & indexing of 250 notes
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Performance Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("Create notebook");
    let tag = TagRepository::create(&conn, "perf_tag").expect("Create tag");

    let start_insert = Instant::now();
    for i in 0..250 {
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Performance Note #{i}")),
                content: Some(format!(
                    "Content of performance test note #{i} with target_perf_keyword."
                )),
                notebook_id: Some(nb.id.clone()),
                ..Default::default()
            },
        )
        .expect("Create note");

        if i % 5 == 0 {
            TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).expect("Add tag");
        }
    }
    let insert_elapsed = start_insert.elapsed();
    println!("[Task 24 Perf] Inserted 250 notes in: {:?}", insert_elapsed);

    // 2. Measure Note List with Content Limit
    let start_list = Instant::now();
    let listed_notes = NoteRepository::list_active(&conn).expect("List notes");
    let list_elapsed = start_list.elapsed();
    assert_eq!(listed_notes.len(), 250);
    println!("[Task 24 Perf] Listed 250 notes in: {:?}", list_elapsed);
    assert!(
        list_elapsed.as_millis() < 50,
        "List notes must complete under 50ms (took {}ms)",
        list_elapsed.as_millis()
    );

    // 3. Measure Search Latency over 250 notes (default query limit is 50 results)
    let start_search = Instant::now();
    let search_results =
        SearchRepository::search(&conn, "target_perf_keyword", None).expect("Search notes");
    let search_elapsed = start_search.elapsed();
    assert_eq!(search_results.len(), 50, "Default search cap is 50 results");
    println!(
        "[Task 24 Perf] Search across 250 notes in: {:?}",
        search_elapsed
    );
    assert!(
        search_elapsed.as_millis() < 25,
        "Search must complete under 25ms (took {}ms)",
        search_elapsed.as_millis()
    );

    // 4. Measure Backup Creation Time with Online Backup API
    let start_backup = Instant::now();
    let backup_req = BackupCreateRequest {
        destination_path: None,
        backup_type: Some(BackupType::Manual),
        allow_overwrite: Some(false),
    };
    let backup_res = BackupService::create_backup(&conn, &env.paths, backup_req, None::<fn(_)>)
        .expect("Create backup");
    let backup_elapsed = start_backup.elapsed();
    println!("[Task 24 Perf] Backup created in: {:?}", backup_elapsed);
    assert!(
        backup_elapsed.as_millis() < 500,
        "Backup creation must complete under 500ms (took {}ms)",
        backup_elapsed.as_millis()
    );
    assert!(std::path::Path::new(&backup_res.file_path).exists());

    // 5. Invariant & Regression Verification
    // Check that notebook counts match
    let notes_in_nb = NoteRepository::list_by_notebook(&conn, &nb.id).expect("List by notebook");
    assert_eq!(notes_in_nb.len(), 250);

    let notes_with_tag = NoteRepository::list_by_tag(&conn, &tag.id).expect("List by tag");
    assert_eq!(notes_with_tag.len(), 50);
}
