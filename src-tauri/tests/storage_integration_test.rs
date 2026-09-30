use personal_notepad_lib::storage::{
    init_connection, run_migrations,
    models::{CreateNotebookDto, UpdateNotebookDto, CreateNoteDto, UpdateNoteDto, UpdateTagDto},
    repositories::{NotebookRepository, NoteRepository, SettingsRepository, TagRepository},
    StorageError, StoragePaths,
};

/// Helper to create an isolated, unique temporary database environment for tests
struct TestEnv {
    dir: std::path::PathBuf,
    paths: StoragePaths,
}

impl TestEnv {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("pn_integration_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(dir.clone());
        paths.ensure_directories().expect("Failed to create test directories");
        Self { dir, paths }
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn test_fresh_database_initialization_and_migration() {
    let env = TestEnv::new();

    let mut conn = init_connection(&env.paths.database_file).expect("Database initialization failed");
    run_migrations(&mut conn).expect("Migration failed");

    assert!(env.paths.database_file.exists(), "database.sqlite must exist");

    // Verify migration 001 recorded
    let version: i32 = conn
        .query_row("SELECT version FROM _migrations WHERE version = 1", [], |r| r.get(0))
        .expect("Migration 1 must be recorded");
    assert_eq!(version, 1);

    // Verify all core tables exist in sqlite_master
    for table in ["notes", "notebooks", "tags", "note_tags", "attachments", "settings"] {
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
        let mut conn = init_connection(&env.paths.database_file).expect("Initial connection failed");
        run_migrations(&mut conn).expect("Migration failed");

        let nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Persistent Notebook".to_string(),
                parent_id: None,
            },
        ).expect("Notebook create failed");
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
        ).expect("Note create failed");
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
    let complex_content = "नमस्ते दुनिया! Testing Hindi (हिंदी), English, emojis (🚀🔥🧠), and symbols (<>&\"'/\\).";
    let unicode_tag = "ज़रूरी-दस्तावेज़ 🏷️";
    let unicode_nb = "व्यक्तिगत प्रोजेक्ट्स 📁";

    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: unicode_nb.to_string(),
            parent_id: None,
        },
    ).expect("Notebook create failed");
    assert_eq!(nb.name, unicode_nb);

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(hindi_title.to_string()),
            content: Some(complex_content.to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        },
    ).expect("Note create failed");
    assert_eq!(note.title, hindi_title);
    assert_eq!(note.content, complex_content);

    let tag = TagRepository::create(&conn, unicode_tag).expect("Tag create failed");
    assert_eq!(tag.name, unicode_tag);

    TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).expect("Link failed");
    let fetched_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("Fetch tags failed");
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
    ).expect("Creation with SQL injection characters must be handled safely as text");

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
    ).expect("Level 1 failed");

    let level2 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Level 2 Child".to_string(),
            parent_id: Some(level1.id.clone()),
        },
    ).expect("Level 2 failed");

    let level3 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Level 3 Grandchild".to_string(),
            parent_id: Some(level2.id.clone()),
        },
    ).expect("Level 3 failed");

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

    let n1 = NoteRepository::create(&conn, CreateNoteDto { title: Some("N1".to_string()), ..Default::default() }).expect("N1 failed");
    let n2 = NoteRepository::create(&conn, CreateNoteDto { title: Some("N2".to_string()), ..Default::default() }).expect("N2 failed");
    let _n3 = NoteRepository::create(&conn, CreateNoteDto { title: Some("N3".to_string()), ..Default::default() }).expect("N3 failed");

    // Soft delete n2
    let deleted = NoteRepository::delete(&conn, &n2.id, true).expect("Delete failed");
    assert!(deleted);

    let active = NoteRepository::list(&conn, false).expect("List active failed");
    assert_eq!(active.len(), 2);
    assert!(!active.iter().any(|n| n.id == n2.id));

    let all = NoteRepository::list(&conn, true).expect("List all failed");
    assert_eq!(all.len(), 3);
    let deleted_item = all.iter().find(|n| n.id == n2.id).expect("n2 must exist in trash");
    assert!(deleted_item.is_deleted);
    assert!(deleted_item.deleted_at.is_some());

    // Update n1
    let updated_n1 = NoteRepository::update(&conn, &n1.id, UpdateNoteDto {
        title: Some("N1 Updated".to_string()),
        ..Default::default()
    }).expect("Update failed");
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
        let mut conn = init_connection(&env.paths.database_file).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migrations failed");

        // Test 1: Note persistence
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(test1_title.to_string()),
                content: Some(test1_content.to_string()),
                ..Default::default()
            },
        ).expect("Create note 1 failed");
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
            ).expect("Create multiple note failed");
        }

        // Test 3: Unicode note
        let u_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(unicode_title.to_string()),
                content: Some(unicode_content.to_string()),
                ..Default::default()
            },
        ).expect("Create unicode note failed");
        unicode_note_id = u_note.id;

        // Test 4: Notebook
        let nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Work Notebook 💼".to_string(),
                parent_id: None,
            },
        ).expect("Create notebook failed");
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
        let mut conn = init_connection(&env.paths.database_file).expect("Session 2 connection failed");
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
            ).expect("Create note failed");

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
            for (idx, (orig, ret)) in original_lines.iter().zip(retrieved_lines.iter()).enumerate() {
                assert_eq!(orig, ret, "Line {idx} must match exactly without trimming or conversion");
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
            ).expect("Create markdown note failed");

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
            for (idx, (orig, ret)) in original_lines.iter().zip(retrieved_lines.iter()).enumerate() {
                assert_eq!(orig, ret, "Markdown line {idx} must match exactly without HTML conversion");
            }

            // Test deliberate format toggle from "md" to "txt" without content corruption
            let updated = NoteRepository::update(
                &conn,
                &note_id,
                UpdateNoteDto {
                    format: Some("txt".to_string()),
                    ..Default::default()
                },
            ).expect("Update format failed");

            assert_eq!(updated.format, "txt");
            assert_eq!(updated.content, md_content, "Content must remain 100% untouched when format changes");
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
        ).expect("Create Note A failed");

        // Create Note B
        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note B Initial".to_string()),
                content: Some("Note B Initial Content".to_string()),
                format: Some("txt".to_string()),
                ..Default::default()
            },
        ).expect("Create Note B failed");

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
        ).expect("Flush Note A failed");

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
        ).expect("Create empty note failed");

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
        ).expect("Create whitespace note failed");

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
        ).expect("Update with whitespace failed");

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
        ).expect("Create 10KB failed");
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
        ).expect("Create 100KB failed");
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
        ).expect("Create 500KB failed");
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
        ).expect("Update to 1MB failed");
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
        ).expect("Create note failed");

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
        ).expect("Update note failed");

        // Assert created_at remains strictly identical, while modified_at is updated
        assert_eq!(updated_note.created_at, initial_created_at, "created_at must remain completely stable");
        assert_ne!(updated_note.modified_at, initial_modified_at, "modified_at must update on save");
        assert!(updated_note.modified_at > initial_modified_at, "modified_at must advance chronologically");

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
        ).expect("Creation failed");

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
        ).expect("Title update failed");
        assert_eq!(after_title_update.title, updated_title);
        assert_eq!(after_title_update.content, raw_content);

        // 4. Content update verification
        let updated_content = "Updated Markdown content:\n- [x] Item 1\n- [ ] Item 2\n\nCode: `const x = 42;`";
        let after_content_update = NoteRepository::update(
            &conn,
            &created_note.id,
            UpdateNoteDto {
                content: Some(updated_content.to_string()),
                ..Default::default()
            },
        ).expect("Content update failed");
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
        ).expect("Rev 2 save failed");

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
        ).expect("Clear content failed");
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
        ).expect("Note 1 creation failed");

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
        ).expect("Note 2 creation failed");

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
        ).expect("Note 3 creation failed");

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
        ).expect("Note 1 update failed");

        let initial_created_at = saved_note1.created_at.clone();
        let initial_modified_at = saved_note1.modified_at.clone();

        // --- Session 1 Complete: Simulating Full Application Close ---
        drop(conn);

        // --- Session 2: Simulating Application Restart ---
        let restarted_conn = init_connection(&env.paths.database_file).expect("Restart connection failed");

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
        assert_eq!(reopened_note2.content, "Unique content for Note B (Dev notes)");

        let reopened_note3 = NoteRepository::get_by_id(&restarted_conn, &note3.id)
            .expect("Get failed")
            .expect("Note 3 must exist");
        assert_eq!(reopened_note3.title, "Note C");
        assert_eq!(reopened_note3.content, "Unique content for Note C (Shopping list)");

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
    let mut conn = init_connection(&env.paths.database_file).expect("Database initialization failed");
    run_migrations(&mut conn).expect("Fresh migration must succeed");

    // 2. Validate columns on notebooks table
    {
        let mut stmt = conn.prepare("PRAGMA table_info(notebooks)").expect("PRAGMA failed");
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
    ).expect("Root notebook creation failed");

    let child = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(root.id.clone()),
        },
    ).expect("Child notebook creation failed");

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
    ).expect("Note in child notebook creation failed");

    let note_in_root = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Root Memo".to_string()),
            content: Some("General work memo".to_string()),
            notebook_id: Some(root.id.clone()),
            ..Default::default()
        },
    ).expect("Note in root notebook creation failed");

    // 5. Delete child notebook directly to verify foreign key SET NULL behavior
    conn.execute("DELETE FROM notebooks WHERE id = ?1", [&child.id])
        .expect("Notebook deletion failed");

    // 6. Verify Note in child notebook is NOT deleted and became unfiled (notebook_id IS NULL)
    let preserved_note = NoteRepository::get_by_id(&conn, &note_in_child.id)
        .expect("Query failed")
        .expect("Note must NOT be deleted when notebook is deleted");
    assert_eq!(preserved_note.notebook_id, None, "Note must become unfiled (NULL)");
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
    ).expect("Create Work failed");

    let personal = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Personal".to_string(),
            parent_id: None,
        },
    ).expect("Create Personal failed");

    // 2. Create Notes
    let _work_note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Note 1".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    ).expect("Create failed");

    let _work_note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Note 2".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    ).expect("Create failed");

    let _personal_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Personal Note 1".to_string()),
            notebook_id: Some(personal.id.clone()),
            ..Default::default()
        },
    ).expect("Create failed");

    let _unfiled_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Unfiled Note 1".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    ).expect("Create failed");

    // Soft-deleted note in Work
    let deleted_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Deleted Work Note".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    ).expect("Create failed");
    NoteRepository::delete(&conn, &deleted_note.id, true).expect("Soft delete failed");

    // 3. Query All Notes: should return 4 active notes (excluding deleted)
    let all_notes = NoteRepository::list_filtered(&conn, false, None, false).expect("Query failed");
    assert_eq!(all_notes.len(), 4);

    // 4. Query Work Notebook: should return exactly 2 active notes
    let work_notes = NoteRepository::list_filtered(&conn, false, Some(&work.id), false).expect("Query failed");
    assert_eq!(work_notes.len(), 2);
    assert!(work_notes.iter().all(|n| n.notebook_id.as_deref() == Some(work.id.as_str())));

    // 5. Query Personal Notebook: should return exactly 1 active note
    let personal_notes = NoteRepository::list_filtered(&conn, false, Some(&personal.id), false).expect("Query failed");
    assert_eq!(personal_notes.len(), 1);
    assert_eq!(personal_notes[0].title, "Personal Note 1");
    assert_eq!(personal_notes[0].notebook_id.as_deref(), Some(personal.id.as_str()));

    // 6. Query Unfiled Notes: should return exactly 1 unfiled note
    let unfiled_notes = NoteRepository::list_filtered(&conn, false, None, true).expect("Query failed");
    assert_eq!(unfiled_notes.len(), 1);
    assert_eq!(unfiled_notes[0].title, "Unfiled Note 1");
    assert_eq!(unfiled_notes[0].notebook_id, None);

    // 7. Query Work Notebook with include_deleted = true: should return 3 notes
    let work_with_deleted = NoteRepository::list_filtered(&conn, true, Some(&work.id), false).expect("Query failed");
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
    ).expect("Create work notebook failed");

    // 2. Create child notebook "Projects" under "Work"
    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(work.id.clone()),
        },
    ).expect("Create child notebook failed");

    // 3. Create notes inside Work and Projects
    let note_in_work = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Note".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    ).expect("Create note in work failed");

    let note_in_projects = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Projects Note".to_string()),
            notebook_id: Some(projects.id.clone()),
            ..Default::default()
        },
    ).expect("Create note in projects failed");

    let original_id = work.id.clone();
    let original_created_at = work.created_at.clone();

    // 4. Rename "Work" to "Work 2026"
    let renamed = NotebookRepository::update(
        &conn,
        &original_id,
        UpdateNotebookDto {
            name: "Work 2026".to_string(),
        },
    ).expect("Rename failed");

    // 5. Verify invariant checks: identity, parent, created_at remain unchanged
    assert_eq!(renamed.id, original_id);
    assert_eq!(renamed.name, "Work 2026");
    assert_eq!(renamed.parent_id, None);
    assert_eq!(renamed.created_at, original_created_at);

    // 6. Verify child hierarchy remains intact
    let fetched_child = NotebookRepository::get_by_id(&conn, &projects.id)
        .expect("Query failed")
        .expect("Child exists");
    assert_eq!(fetched_child.parent_id.as_deref(), Some(original_id.as_str()));

    // 7. Verify notes relationships remain intact
    let fetched_work_note = NoteRepository::get_by_id(&conn, &note_in_work.id)
        .expect("Query failed")
        .expect("Work note exists");
    assert_eq!(fetched_work_note.notebook_id.as_deref(), Some(original_id.as_str()));

    let fetched_child_note = NoteRepository::get_by_id(&conn, &note_in_projects.id)
        .expect("Query failed")
        .expect("Child note exists");
    assert_eq!(fetched_child_note.notebook_id.as_deref(), Some(projects.id.as_str()));

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
    ).expect("Create work notebook failed");

    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(work.id.clone()),
        },
    ).expect("Create projects failed");

    // 2. Create Note A and Note B inside "Work"
    let note_a = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note A".to_string()),
            content: Some("Content A".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    ).expect("Create note A failed");

    let note_b = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note B".to_string()),
            content: Some("Content B".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    ).expect("Create note B failed");

    // 3. Attempting to delete "Work" while "Projects" exists must fail
    let delete_err = NotebookRepository::delete(&mut conn, &work.id)
        .expect_err("Deleting parent with children must fail");

    assert!(
        delete_err.to_string().contains("This notebook contains sub-notebooks"),
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
    assert_eq!(fetched_a.content, "Content A", "Note A content must be preserved");

    let fetched_b = NoteRepository::get_by_id(&conn, &note_b.id)
        .expect("Query failed")
        .expect("Note B exists");
    assert_eq!(fetched_b.notebook_id, None, "Note B must be unfiled");
    assert_eq!(fetched_b.content, "Content B", "Note B content must be preserved");

    // 7. Verify notebook itself is deleted
    assert!(NotebookRepository::get_by_id(&conn, &work.id).expect("Query failed").is_none());

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
    ).expect("Create work notebook failed");

    let personal = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Personal".to_string(),
            parent_id: None,
        },
    ).expect("Create personal notebook failed");

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
    ).expect("Create note failed");

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
    let moved_to_personal = NoteRepository::move_to_notebook(&conn, &original_id, Some(&personal.id))
        .expect("Move to Personal failed");
    assert_eq!(moved_to_personal.notebook_id.as_deref(), Some(personal.id.as_str()));

    // 5. Move Personal -> Work
    let moved_back_to_work = NoteRepository::move_to_notebook(&conn, &original_id, Some(&work.id))
        .expect("Move back to Work failed");
    assert_eq!(moved_back_to_work.notebook_id.as_deref(), Some(work.id.as_str()));

    // 6. Move Work -> Unfiled (notebook_id: None)
    let moved_to_unfiled = NoteRepository::move_to_notebook(&conn, &original_id, None)
        .expect("Move to Unfiled failed");
    assert_eq!(moved_to_unfiled.notebook_id, None);
    assert_eq!(moved_to_unfiled.content, original_content);

    // 7. Verify validation: moving to a nonexistent notebook ID must fail
    let invalid_move = NoteRepository::move_to_notebook(&conn, &original_id, Some("nonexistent_nb_12345"))
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
    ).expect("Create notebook failed");

    // 2. Create note targeting selected notebook Work
    let work_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Sprint Plan".to_string()),
            content: Some("Tasks for this week".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    ).expect("Create note in Work failed");

    // 3. Create note targeting unfiled (no notebook selected)
    let unfiled_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quick Thought".to_string()),
            content: Some("Random note".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    ).expect("Create unfiled note failed");

    // 4. Verify notebook_id assignment
    assert_eq!(work_note.notebook_id.as_deref(), Some(work.id.as_str()));
    assert_eq!(unfiled_note.notebook_id, None);

    // 5. Verify list filtering returns correct notes
    let filtered_work = NoteRepository::list_filtered(&conn, false, Some(&work.id), false)
        .expect("Query failed");
    assert_eq!(filtered_work.len(), 1);
    assert_eq!(filtered_work[0].id, work_note.id);

    let filtered_unfiled = NoteRepository::list_filtered(&conn, false, None, true)
        .expect("Query failed");
    assert_eq!(filtered_unfiled.len(), 1);
    assert_eq!(filtered_unfiled[0].id, unfiled_note.id);

    // 6. Verify persistence across connection reopen
    drop(conn);
    let conn2 = init_connection(&env.paths.database_file).expect("Reopen failed");
    let persisted_work_note = NoteRepository::get_by_id(&conn2, &work_note.id)
        .expect("Query failed")
        .expect("Note exists");
    assert_eq!(persisted_work_note.notebook_id.as_deref(), Some(work.id.as_str()));
}

#[test]
fn test_task_21_empty_sidebar_state_unfiled_notes_without_notebooks() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Verify 0 notebooks exist initially
    let notebooks = NotebookRepository::list(&conn).expect("Query failed");
    assert_eq!(notebooks.len(), 0, "No notebooks should exist on clean database");

    // 2. Create multiple unfiled notes without creating any notebooks
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Standalone Note 1".to_string()),
            content: Some("First note without notebooks".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    ).expect("Create note 1 failed");

    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Standalone Note 2".to_string()),
            content: Some("Second note without notebooks".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    ).expect("Create note 2 failed");

    // 3. Query All Notes: returns 2 unfiled notes
    let all_notes = NoteRepository::list(&conn, false).expect("Query failed");
    assert_eq!(all_notes.len(), 2);
    assert!(all_notes.iter().all(|n| n.notebook_id.is_none()));

    // 4. Query Unfiled Notes: returns 2 notes
    let unfiled_notes = NoteRepository::list_filtered(&conn, false, None, true).expect("Query failed");
    assert_eq!(unfiled_notes.len(), 2);

    // 5. Update unfiled note content
    let updated = NoteRepository::update(
        &conn,
        &note1.id,
        UpdateNoteDto {
            content: Some("Updated content without notebook".to_string()),
            ..Default::default()
        },
    ).expect("Update failed");
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
    assert!(invalid_create.is_err(), "Creation with nonexistent parent must fail validation");

    // 2. Direct insertion or legacy corrupted record with nonexistent parent_id
    let orphan_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "PRAGMA foreign_keys = OFF;",
        [],
    ).expect("Disable FK failed");

    conn.execute(
        "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![orphan_id, "Corrupted Orphan Notebook", "missing-parent-id-12345", now, now],
    ).expect("Insert orphan notebook failed");

    conn.execute(
        "PRAGMA foreign_keys = ON;",
        [],
    ).expect("Enable FK failed");

    // 3. NotebookRepository::list successfully reads all notebooks without crash or panic
    let all_notebooks = NotebookRepository::list(&conn).expect("List notebooks must succeed");
    assert_eq!(all_notebooks.len(), 1);
    assert_eq!(all_notebooks[0].id, orphan_id);
    assert_eq!(all_notebooks[0].parent_id, Some("missing-parent-id-12345".to_string()));

    // 4. Note creation and association with the orphan notebook works without error
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note in Orphan Notebook".to_string()),
            content: Some("Content inside orphan".to_string()),
            notebook_id: Some(orphan_id.clone()),
            ..Default::default()
        },
    ).expect("Create note in orphan notebook succeeded");
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
    ).expect("Root create failed");

    let child = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Child Notebook".to_string(),
            parent_id: Some(root.id.clone()),
        },
    ).expect("Child create failed");

    assert_eq!(child.parent_id, Some(root.id.clone()));

    // 2. Verify that notebook updates strictly update name and never re-parent (preventing cycle creation)
    let updated = NotebookRepository::update(
        &conn,
        &child.id,
        UpdateNotebookDto {
            name: "Renamed Child".to_string(),
        },
    ).expect("Update failed");
    assert_eq!(updated.parent_id, Some(root.id.clone()));

    // 3. Test cycle resilience: manually construct a circular reference A -> B -> C -> A
    let a_id = uuid::Uuid::new_v4().to_string();
    let b_id = uuid::Uuid::new_v4().to_string();
    let c_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute("PRAGMA foreign_keys = OFF;", []).expect("Disable FK failed");
    conn.execute(
        "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5),
                (?6, ?7, ?8, ?9, ?10),
                (?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            a_id, "Cycle A", c_id, now, now,
            b_id, "Cycle B", a_id, now, now,
            c_id, "Cycle C", b_id, now, now,
        ],
    ).expect("Insert circular notebooks failed");
    conn.execute("PRAGMA foreign_keys = ON;", []).expect("Enable FK failed");

    // 4. Listing repositories handles cyclical database rows without infinite recursion or panic
    let all = NotebookRepository::list(&conn).expect("List must succeed despite cycle");
    assert_eq!(all.len(), 5); // root + child + 3 cycle nodes

    // 5. Attempting to delete a notebook in the cycle fails safely because it has child references
    let mut conn_del = init_connection(&env.paths.database_file).expect("Open connection failed");
    let delete_result = NotebookRepository::delete(&mut conn_del, &a_id);
    assert!(delete_result.is_err(), "Must prevent deletion when children exist in cycle");
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
    ).expect("Create Work failed");

    let personal = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Personal".to_string(),
            parent_id: None,
        },
    ).expect("Create Personal failed");

    // 2. Create Note 1 in Work, Note 2 in Personal, and Note 3 in Unfiled
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Work Report".to_string()),
            content: Some("Quarterly summary".to_string()),
            notebook_id: Some(work.id.clone()),
            ..Default::default()
        },
    ).expect("Create note 1 failed");

    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Grocery List".to_string()),
            content: Some("Milk, Bread, Butter".to_string()),
            notebook_id: Some(personal.id.clone()),
            ..Default::default()
        },
    ).expect("Create note 2 failed");

    let note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quick Thought".to_string()),
            content: Some("Remember to call Alex".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    ).expect("Create note 3 failed");

    // 3. Verify initial notes list for Work contains only Note 1
    let work_notes = NoteRepository::list_filtered(&conn, false, Some(&work.id), false).expect("Query failed");
    assert_eq!(work_notes.len(), 1);
    assert_eq!(work_notes[0].id, note1.id);

    // 4. Moving a note out: Move Note 1 from Work to Personal
    NoteRepository::move_to_notebook(&conn, &note1.id, Some(&personal.id)).expect("Move failed");

    // Work note list must immediately NOT contain Note 1 (disappears from Work view)
    let work_notes_after = NoteRepository::list_filtered(&conn, false, Some(&work.id), false).expect("Query failed");
    assert_eq!(work_notes_after.len(), 0);

    // Personal note list must immediately contain Note 1 (appears in Personal view)
    let personal_notes_after = NoteRepository::list_filtered(&conn, false, Some(&personal.id), false).expect("Query failed");
    assert_eq!(personal_notes_after.len(), 2);
    assert!(personal_notes_after.iter().any(|n| n.id == note1.id));
    assert!(personal_notes_after.iter().any(|n| n.id == note2.id));

    // 5. Deleting notebook: Delete Personal notebook safely
    let mut conn_del = init_connection(&env.paths.database_file).expect("Open connection failed");
    NotebookRepository::delete(&mut conn_del, &personal.id).expect("Delete Personal failed");

    // Notes in Personal become unfiled. Note list for Unfiled immediately includes Note 1 and Note 2
    let unfiled_notes_after = NoteRepository::list_filtered(&conn, false, None, true).expect("Query unfiled failed");
    assert_eq!(unfiled_notes_after.len(), 3); // note1, note2, and note3
    assert!(unfiled_notes_after.iter().any(|n| n.id == note1.id));
    assert!(unfiled_notes_after.iter().any(|n| n.id == note2.id));
    assert!(unfiled_notes_after.iter().any(|n| n.id == note3.id));

    // 6. Moving note in: Move Note 3 from Unfiled into Work
    NoteRepository::move_to_notebook(&conn, &note3.id, Some(&work.id)).expect("Move to work failed");

    let work_final = NoteRepository::list_filtered(&conn, false, Some(&work.id), false).expect("Query work failed");
    assert_eq!(work_final.len(), 1);
    assert_eq!(work_final[0].id, note3.id);

    let unfiled_final = NoteRepository::list_filtered(&conn, false, None, true).expect("Query unfiled failed");
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

    let roadmap_content = "# 2026 Roadmap\n\n1. Phase 4: Notebooks\n2. Phase 5: Search\n3. Phase 6: Sync";
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
        ).expect("Create Work failed");
        work_id = work.id;

        // 2. Create child notebook "Projects" inside "Work"
        let projects = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Projects".to_string(),
                parent_id: Some(work_id.clone()),
            },
        ).expect("Create Projects failed");
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
        ).expect("Create roadmap note failed");
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
        ).expect("Create meeting note failed");
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
        ).expect("Create unfiled note failed");
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
        let projects_notes = NoteRepository::list_filtered(&conn2, false, Some(&projects_id), false)
            .expect("Query failed");
        assert_eq!(projects_notes.len(), 1);
        assert_eq!(projects_notes[0].id, roadmap_note_id);

        let work_notes = NoteRepository::list_filtered(&conn2, false, Some(&work_id), false)
            .expect("Query failed");
        assert_eq!(work_notes.len(), 1);
        assert_eq!(work_notes[0].id, meeting_note_id);

        let unfiled_notes = NoteRepository::list_filtered(&conn2, false, None, true)
            .expect("Query failed");
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
        ).expect("Create Hindi notebook failed");
        hindi_id = hindi_nb.id;

        let cjk_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "旅行".to_string(), // CJK Japanese/Chinese
                parent_id: None,
            },
        ).expect("Create CJK notebook failed");
        cjk_id = cjk_nb.id;

        let cafe_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Café".to_string(), // Accented Latin
                parent_id: Some(hindi_id.clone()), // nested under Hindi parent
            },
        ).expect("Create Café notebook failed");
        cafe_id = cafe_nb.id;

        let amp_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Ideas & Research".to_string(), // Ampersand & spaces
                parent_id: None,
            },
        ).expect("Create Ideas & Research notebook failed");
        amp_id = amp_nb.id;

        let emdash_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "2026 — Goals".to_string(), // Em-dash & numbers
                parent_id: None,
            },
        ).expect("Create 2026 — Goals notebook failed");
        emdash_id = emdash_nb.id;

        let slash_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Project / Planning".to_string(), // Slash character (ensures no filesystem delimiter issue)
                parent_id: Some(emdash_id.clone()), // nested under Em-dash parent
            },
        ).expect("Create Project / Planning notebook failed");
        slash_id = slash_nb.id;

        let emoji_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "🚀 Rocket Launch".to_string(), // 4-byte UTF-8 emoji
                parent_id: None,
            },
        ).expect("Create Emoji notebook failed");
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
        ).expect("Create Hindi note failed");
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
        ).expect("Create CJK note failed");
        cjk_note_id = cjk_note.id;

        // 3. Rename with unicode and symbols
        let renamed_cafe = NotebookRepository::update(
            &conn,
            &cafe_id,
            UpdateNotebookDto {
                name: "Café Français 🥐 & Bistro".to_string(),
            },
        ).expect("Rename Café failed");
        assert_eq!(renamed_cafe.name, "Café Français 🥐 & Bistro");

        let renamed_hindi = NotebookRepository::update(
            &conn,
            &hindi_id,
            UpdateNotebookDto {
                name: "तीर्थ यात्रा एवं देशाटन".to_string(),
            },
        ).expect("Rename Hindi notebook failed");
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
        assert_eq!(fetched_hindi_note.content, "१. नई नोटबुक प्रणाली का सत्यापन\n२. स्थानीय डेटाबेस की जांच");
        assert_eq!(fetched_hindi_note.notebook_id, Some(hindi_id.clone()));

        let fetched_cjk_note = NoteRepository::get_by_id(&conn2, &cjk_note_id)
            .expect("Get failed")
            .expect("CJK note must exist");
        assert_eq!(fetched_cjk_note.title, "来期のロードマップ");
        assert_eq!(fetched_cjk_note.content, "オフライン優先アーキテクチャの完全な実装と検証。");
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
    let work = NotebookRepository::create(&conn, CreateNotebookDto { name: "Work".into(), parent_id: None }).unwrap();

    // Level 1
    let projects = NotebookRepository::create(&conn, CreateNotebookDto { name: "Projects".into(), parent_id: Some(work.id.clone()) }).unwrap();
    let meetings = NotebookRepository::create(&conn, CreateNotebookDto { name: "Meetings".into(), parent_id: Some(work.id.clone()) }).unwrap();
    let archive = NotebookRepository::create(&conn, CreateNotebookDto { name: "Archive".into(), parent_id: Some(work.id.clone()) }).unwrap();

    // Level 2 (Leaves)
    let proj_a = NotebookRepository::create(&conn, CreateNotebookDto { name: "Project A".into(), parent_id: Some(projects.id.clone()) }).unwrap();
    let _proj_b = NotebookRepository::create(&conn, CreateNotebookDto { name: "Project B".into(), parent_id: Some(projects.id.clone()) }).unwrap();
    let proj_c = NotebookRepository::create(&conn, CreateNotebookDto { name: "Project C".into(), parent_id: Some(projects.id.clone()) }).unwrap();

    let weekly = NotebookRepository::create(&conn, CreateNotebookDto { name: "Weekly".into(), parent_id: Some(meetings.id.clone()) }).unwrap();
    let _monthly = NotebookRepository::create(&conn, CreateNotebookDto { name: "Monthly".into(), parent_id: Some(meetings.id.clone()) }).unwrap();

    let arch_2025 = NotebookRepository::create(&conn, CreateNotebookDto { name: "2025".into(), parent_id: Some(archive.id.clone()) }).unwrap();
    let _arch_2026 = NotebookRepository::create(&conn, CreateNotebookDto { name: "2026".into(), parent_id: Some(archive.id.clone()) }).unwrap();

    // Verify 11 notebooks created
    let all = NotebookRepository::list(&conn).unwrap();
    assert_eq!(all.len(), 11);

    // Verify distinct IDs (no duplicate nodes)
    let unique_ids: std::collections::HashSet<_> = all.iter().map(|n| &n.id).collect();
    assert_eq!(unique_ids.len(), 11);

    // 2. Create notes at various depths
    let note_work = NoteRepository::create(&conn, CreateNoteDto {
        title: Some("Work Strategy".into()),
        notebook_id: Some(work.id.clone()),
        ..Default::default()
    }).unwrap();

    let note_proj_a = NoteRepository::create(&conn, CreateNoteDto {
        title: Some("Project A Specs".into()),
        notebook_id: Some(proj_a.id.clone()),
        ..Default::default()
    }).unwrap();

    let note_weekly = NoteRepository::create(&conn, CreateNoteDto {
        title: Some("Weekly Standup".into()),
        notebook_id: Some(weekly.id.clone()),
        ..Default::default()
    }).unwrap();

    let note_2025 = NoteRepository::create(&conn, CreateNoteDto {
        title: Some("Archive Summary 2025".into()),
        notebook_id: Some(arch_2025.id.clone()),
        ..Default::default()
    }).unwrap();

    // 3. Verify strict isolation in note filtering per notebook
    let filter_work = NoteRepository::list_filtered(&conn, false, Some(&work.id), false).unwrap();
    assert_eq!(filter_work.len(), 1);
    assert_eq!(filter_work[0].id, note_work.id);

    let filter_proj_a = NoteRepository::list_filtered(&conn, false, Some(&proj_a.id), false).unwrap();
    assert_eq!(filter_proj_a.len(), 1);
    assert_eq!(filter_proj_a[0].id, note_proj_a.id);

    let filter_weekly = NoteRepository::list_filtered(&conn, false, Some(&weekly.id), false).unwrap();
    assert_eq!(filter_weekly.len(), 1);
    assert_eq!(filter_weekly[0].id, note_weekly.id);

    let filter_2025 = NoteRepository::list_filtered(&conn, false, Some(&arch_2025.id), false).unwrap();
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

    let reloaded_weekly_notes = NoteRepository::list_filtered(&conn2, false, Some(&weekly.id), false).unwrap();
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
        ).unwrap();
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
        ).unwrap();
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
    let unfiled_notes = NoteRepository::list_filtered(&conn, false, None, true)
        .expect("Unfiled query failed");
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
    ).unwrap_err();

    match &empty_err {
        personal_notepad_lib::storage::StorageError::Validation(msg) => {
            assert_eq!(msg, "Notebook name cannot be empty");
        }
        other => panic!("Expected ValidationError, got {other:?}"),
    }
    assert_eq!(empty_err.user_friendly_message(), "Notebook name cannot be empty");

    // 2. InvalidHierarchy: Nonexistent parent
    let missing_parent_err = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Child with missing parent".to_string(),
            parent_id: Some("ghost-parent-id-xyz".to_string()),
        },
    ).unwrap_err();

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
    ).expect("Parent creation failed");

    let _child = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Child Subfolder".to_string(),
            parent_id: Some(parent.id.clone()),
        },
    ).expect("Child creation failed");

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
    ).unwrap_err();

    match &not_found_err {
        personal_notepad_lib::storage::StorageError::NotFound(msg) => {
            assert!(msg.contains("does-not-exist-uuid"));
        }
        other => panic!("Expected NotFound error, got {other:?}"),
    }

    // 5. Raw SQL error masking: Ensure no raw SQL statements or internal columns are exposed
    let constraint_err = personal_notepad_lib::storage::StorageError::Database(
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                extended_code: 1555,
            },
            Some("UNIQUE constraint failed: notes.id".to_string()),
        ),
    );
    let friendly = constraint_err.user_friendly_message();
    assert!(!friendly.contains("notes.id"));
    assert!(!friendly.contains("extended_code"));
    assert!(friendly.contains("referenced item no longer exists or a unique constraint was violated"));
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
    ).expect("Notebook create failed");

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
        ).expect("Note create failed");
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
    ).expect("Trigger install failed");

    // 3. Attempting to delete the notebook fails because the trigger aborts the transaction
    let del_result = NotebookRepository::delete(&mut conn, &nb.id);
    assert!(del_result.is_err(), "Deletion must fail due to trigger");

    // 4. Verify atomicity & rollback:
    // Neither the notebook was deleted NOR were the notes unassigned.
    // The database must NOT be in a partial state where notes were unassigned but the notebook remains.
    let reloaded_nb = NotebookRepository::get_by_id(&conn, &nb.id).unwrap();
    assert!(reloaded_nb.is_some(), "Notebook must still exist after rollback");

    for nid in &note_ids {
        let n = NoteRepository::get_by_id(&conn, nid).unwrap().expect("Note must exist");
        assert_eq!(
            n.notebook_id,
            Some(nb.id.clone()),
            "Note must still reference the notebook due to full rollback"
        );
    }

    // 5. Drop the trigger to restore normal execution
    conn.execute("DROP TRIGGER abort_notebook_delete;", []).expect("Drop trigger failed");

    // 6. Delete again: transaction completes successfully
    let del_success = NotebookRepository::delete(&mut conn, &nb.id).expect("Delete must succeed now");
    assert!(del_success);

    // Verify notebook is gone
    let deleted_nb = NotebookRepository::get_by_id(&conn, &nb.id).unwrap();
    assert!(deleted_nb.is_none(), "Notebook must be deleted");

    // Verify all 5 notes are now unfiled (notebook_id = None), without any loss of content
    for (idx, nid) in note_ids.iter().enumerate() {
        let n = NoteRepository::get_by_id(&conn, nid).unwrap().expect("Note must exist");
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
    ).expect("Create Projects failed");

    let frontend = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Frontend".to_string(),
            parent_id: Some(projects.id.clone()),
        },
    ).expect("Create Frontend failed");

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
    ).unwrap();

    let _p_note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Project Roadmap".to_string()),
            content: Some("Roadmap content".to_string()),
            notebook_id: Some(projects.id.clone()),
            ..Default::default()
        },
    ).unwrap();

    let f_note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("UI Specs".to_string()),
            content: Some("CSS tokens".to_string()),
            notebook_id: Some(frontend.id.clone()),
            ..Default::default()
        },
    ).unwrap();

    let _f_note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Component Tree".to_string()),
            content: Some("React tree".to_string()),
            notebook_id: Some(frontend.id.clone()),
            ..Default::default()
        },
    ).unwrap();

    let _f_note3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Theme Engine".to_string()),
            content: Some("Light/dark mode".to_string()),
            notebook_id: Some(frontend.id.clone()),
            ..Default::default()
        },
    ).unwrap();

    let _unfiled = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quick Thought".to_string()),
            content: Some("Unassigned note".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    ).unwrap();

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
    NoteRepository::update(&conn, &f_note1.id, UpdateNoteDto {
        is_deleted: Some(false),
        ..Default::default()
    }).unwrap();
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
    ).expect("Create root failed");
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
    ).expect("Rename failed");
    assert_eq!(renamed.name, "Backend Root Renamed");

    // 5. Nested notebook (parent-child relationship)
    let child = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Child Notebook".to_string(),
            parent_id: Some(root.id.clone()),
        },
    ).expect("Create child failed");
    assert_eq!(child.parent_id, Some(root.id.clone()));

    // 6. Invalid parent handling: attempting to create a notebook with a nonexistent parent_id
    let invalid_parent_err = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Invalid Child".to_string(),
            parent_id: Some("non-existent-uuid-99999".to_string()),
        },
    ).expect_err("Creating notebook with invalid parent must fail");
    match invalid_parent_err {
        StorageError::InvalidHierarchy(msg) => {
            assert!(msg.contains("Parent notebook with ID 'non-existent-uuid-99999' does not exist"));
        }
        other => panic!("Expected StorageError::InvalidHierarchy, got {:?}", other),
    }

    // 7. Missing notebook handling: querying or updating a nonexistent notebook ID
    let missing_nb = NotebookRepository::get_by_id(&conn, "missing-notebook-id").expect("Query failed");
    assert!(missing_nb.is_none());

    let missing_update_err = NotebookRepository::update(
        &conn,
        "missing-notebook-id",
        UpdateNotebookDto {
            name: "New Name".to_string(),
        },
    ).expect_err("Updating nonexistent notebook must fail");
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
    ).expect("Create note failed");
    assert_eq!(note.notebook_id, None);

    // a) Note -> Notebook (Unfiled -> child)
    let moved_to_child = NoteRepository::move_to_notebook(&conn, &note.id, Some(&child.id))
        .expect("Move to child failed");
    assert_eq!(moved_to_child.notebook_id.as_deref(), Some(child.id.as_str()));

    // b) Notebook -> another notebook (child -> root)
    let moved_to_root = NoteRepository::move_to_notebook(&conn, &note.id, Some(&root.id))
        .expect("Move to root failed");
    assert_eq!(moved_to_root.notebook_id.as_deref(), Some(root.id.as_str()));

    // c) Notebook -> unfiled (root -> None)
    let moved_to_unfiled = NoteRepository::move_to_notebook(&conn, &note.id, None)
        .expect("Move to unfiled failed");
    assert_eq!(moved_to_unfiled.notebook_id, None);

    // Put note back in child notebook before deleting child
    NoteRepository::move_to_notebook(&conn, &note.id, Some(&child.id)).expect("Move failed");

    // 10. Note unassignment on notebook deletion:
    // Delete leaf notebook `child`
    let child_deleted = NotebookRepository::delete(&mut conn, &child.id).expect("Delete child failed");
    assert!(child_deleted);

    // Verify note is safely unfiled and content is completely preserved
    let unassigned_note = NoteRepository::get_by_id(&conn, &note.id)
        .expect("Query failed")
        .expect("Note must still exist");
    assert_eq!(unassigned_note.notebook_id, None, "Note must be unassigned to Unfiled");
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
    ).expect("Test A failed: Create Work root notebook");
    assert_eq!(work.name, "Work");
    assert_eq!(work.parent_id, None);

    // Test B — Create child notebook "Projects" under "Work"
    let projects = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Projects".to_string(),
            parent_id: Some(work.id.clone()),
        },
    ).expect("Test B failed: Create Projects child notebook");
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
    ).expect("Test C failed: Create note inside Projects");
    assert_eq!(note.notebook_id.as_deref(), Some(projects.id.as_str()));

    // Test D — Restart verification (close connection and reopen)
    drop(conn);
    let mut reopened_conn = init_connection(&env.paths.database_file).expect("Test D reopen failed");
    run_migrations(&mut reopened_conn).expect("Test D migration check failed");

    let re_work = NotebookRepository::get_by_id(&reopened_conn, &work.id).unwrap().expect("Work survives");
    let re_projects = NotebookRepository::get_by_id(&reopened_conn, &projects.id).unwrap().expect("Projects survives");
    let re_note = NoteRepository::get_by_id(&reopened_conn, &note.id).unwrap().expect("Note survives");
    assert_eq!(re_projects.parent_id, Some(re_work.id.clone()));
    assert_eq!(re_note.notebook_id, Some(re_projects.id.clone()));
    assert_eq!(re_note.content, "Deterministic Note Content for Manual QA");

    // Test E — Move note from Projects to Work
    let moved_to_work = NoteRepository::move_to_notebook(&reopened_conn, &note.id, Some(&work.id))
        .expect("Test E failed: Move note to Work");
    assert_eq!(moved_to_work.notebook_id.as_deref(), Some(work.id.as_str()));

    let projects_notes = NoteRepository::list_filtered(&reopened_conn, false, Some(&projects.id), false).unwrap();
    let work_notes = NoteRepository::list_filtered(&reopened_conn, false, Some(&work.id), false).unwrap();
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
    ).expect("Test G failed: Rename Work");
    assert_eq!(renamed_work.id, work.id, "ID must remain unchanged");
    assert_eq!(renamed_work.name, "Work 2026");

    let children_after_rename = NotebookRepository::list(&reopened_conn).unwrap();
    let child_after = children_after_rename.iter().find(|nb| nb.id == projects.id).expect("Child survives");
    assert_eq!(child_after.parent_id, Some(work.id.clone()), "Parent relationship intact");

    // Test I — Child deletion protection: Deleting "Work 2026" while "Projects" exists MUST fail
    let del_parent_err = NotebookRepository::delete(&mut reopened_conn, &work.id)
        .expect_err("Test I failed: Deleting parent with child must be blocked");
    match del_parent_err {
        StorageError::DeleteBlocked(msg) => assert!(msg.contains("sub-notebooks")),
        other => panic!("Expected DeleteBlocked, got {:?}", other),
    }

    // Now delete child notebook first
    let del_child = NotebookRepository::delete(&mut reopened_conn, &projects.id).expect("Delete child");
    assert!(del_child);

    // Move note back to Work 2026 to verify unfiling on notebook deletion (Test H)
    NoteRepository::move_to_notebook(&reopened_conn, &note.id, Some(&work.id)).expect("Move note back to Work 2026");
    let active_in_work = NoteRepository::list_filtered(&reopened_conn, false, Some(&work.id), false).unwrap();
    assert_eq!(active_in_work.len(), 1);

    // Test H — Delete notebook "Work 2026": notebook disappears, note safely becomes unfiled
    let del_work = NotebookRepository::delete(&mut reopened_conn, &work.id).expect("Test H failed: Delete Work 2026");
    assert!(del_work);
    assert!(NotebookRepository::get_by_id(&reopened_conn, &work.id).unwrap().is_none());

    let final_note = NoteRepository::get_by_id(&reopened_conn, &note.id).unwrap().expect("Note survives deletion");
    assert_eq!(final_note.notebook_id, None, "Note must be unfiled now");
    assert_eq!(final_note.content, "Deterministic Note Content for Manual QA");
}

#[test]
fn test_task_44_large_text_and_editor_regression_across_notebook_mutations() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Create a large note (100 KB) with special formatting
    let mut large_content = String::with_capacity(100 * 1024);
    for i in 1..=1000 {
        large_content.push_str(&format!("Line {:04}: Phase 4 large note content line with unicode: 🚀 नमस्ते\n", i));
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
    ).expect("Create large note failed");

    // 2. Create Notebooks A and B
    let nb_a = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Notebook A".to_string(),
            parent_id: None,
        },
    ).expect("Create NB A failed");

    let nb_b = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Notebook B".to_string(),
            parent_id: None,
        },
    ).expect("Create NB B failed");

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
    ).expect("Rename failed");
    assert_eq!(renamed_a.name, "Notebook A Prime");

    // 5. Switch to Notebook B (simulate query for notes in B)
    let notes_in_b = NoteRepository::list_filtered(&conn, false, Some(&nb_b.id), false)
        .expect("Query B failed");
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
    ).expect("Autosave update failed");

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
        let mut conn = init_connection(&env.paths.database_file).expect("Session 1 connection failed");
        run_migrations(&mut conn).expect("Session 1 migrations failed");

        // 1. Create root notebook
        let parent = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Engineering".to_string(),
                parent_id: None,
            },
        ).expect("Create root notebook failed");
        parent_id = parent.id;

        // 2. Create child notebook
        let child = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Core Infrastructure".to_string(),
                parent_id: Some(parent_id.clone()),
            },
        ).expect("Create child notebook failed");
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
        ).expect("Create note failed");
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
        ).expect("Update note failed");

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
        ).expect("Rename child notebook failed");
        assert_eq!(renamed_child.name, "Core Engine & DB");

        // Rename root notebook to "Engineering & Systems"
        let renamed_parent = NotebookRepository::update(
            &conn,
            &parent_id,
            UpdateNotebookDto {
                name: "Engineering & Systems".to_string(),
            },
        ).expect("Rename parent notebook failed");
        assert_eq!(renamed_parent.name, "Engineering & Systems");

        // Connection closes and is dropped here (simulating application termination)
    }

    // --- Session 2: Application reopen & state survival verification ---
    {
        let mut reopened_conn = init_connection(&env.paths.database_file).expect("Session 2 reopen failed");
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
        let child_notes = NoteRepository::list_filtered(&reopened_conn, false, Some(&child_id), false)
            .expect("Filter query failed");
        assert_eq!(child_notes.len(), 1);
        assert_eq!(child_notes[0].id, note_id);

        // Verify root notebook has 0 direct notes
        let parent_notes = NoteRepository::list_filtered(&reopened_conn, false, Some(&parent_id), false)
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
        ).expect("Create note");

        // 3. Add tag to note (idempotent)
        TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id).expect("Add tag");
        TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id).expect("Add same tag again (idempotent)");

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
        let tags_after_remove = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get note tags");
        assert_eq!(tags_after_remove.len(), 2);
        assert!(!tags_after_remove.iter().any(|t| t.id == tag_urgent.id));

        // 6. Remove non-existent tag association (safe no-op)
        TagRepository::remove_tag_from_note(&conn, &note.id, "non_existent_tag_id").expect("Safe no-op");
        let tags_safe = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get note tags");
        assert_eq!(tags_safe.len(), 2);

        // 7. Atomic batch set (replace all tags with tag_urgent)
        let updated_tags = TagRepository::set_tags_for_note(&mut conn, &note.id, std::slice::from_ref(&tag_urgent.id))
            .expect("Batch set tags");
        assert_eq!(updated_tags.len(), 1);
        assert_eq!(updated_tags[0].id, tag_urgent.id);

        // 8. Reopen database and verify note-tag associations persist
        drop(conn);
        let reopened_conn = init_connection(&env.paths.database_file).expect("Reopen failed");
        let persistent_tags = TagRepository::get_tags_for_note(&reopened_conn, &note.id)
            .expect("Get tags after reopen");
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
        ).expect("Create note 1");

        // Create second note to verify multi-note isolation
        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Personal Diary".to_string()),
                content: Some("Private thoughts".to_string()),
                ..Default::default()
            },
        ).expect("Create note 2");

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
        TagRepository::add_tag_to_note(&conn, &note2.id, &tag_personal.id).expect("Add personal to note 2");
        let note2_tags = TagRepository::get_tags_for_note(&conn, &note2.id).expect("Get note 2 tags");
        assert_eq!(note2_tags.len(), 1);
        assert_eq!(note2_tags[0].name, "personal");

        let note1_tags_isolated = TagRepository::get_tags_for_note(&conn, &note1.id).expect("Get note 1 tags");
        assert_eq!(note1_tags_isolated.len(), 3, "Note 1 must retain its 3 tags");

        // 7. Removing 'work' from note 1 must NOT remove 'planning' or 'important'
        TagRepository::remove_tag_from_note(&conn, &note1.id, &tag_work.id).expect("Remove work from note 1");
        let tags_after_remove = TagRepository::get_tags_for_note(&conn, &note1.id).expect("Get tags");
        assert_eq!(tags_after_remove.len(), 2, "Note 1 must have 2 tags remaining");
        let names_after_remove: Vec<_> = tags_after_remove.iter().map(|t| t.name.as_str()).collect();
        assert!(!names_after_remove.contains(&"work"), "work must be removed");
        assert!(names_after_remove.contains(&"planning"), "planning must remain");
        assert!(names_after_remove.contains(&"important"), "important must remain");

        // Note 2 tags remain unaffected
        let note2_tags_after = TagRepository::get_tags_for_note(&conn, &note2.id).expect("Get note 2 tags");
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
        ).expect("Create note");

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
        let reopened_tags = TagRepository::get_tags_for_note(&conn2, &note.id).expect("Get reopened tags");

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
        let restart2_tags = TagRepository::get_tags_for_note(&conn3, &note.id).expect("Get tags after 2nd reopen");
        assert_eq!(restart2_tags.len(), 3, "All 3 tags must survive second restart");
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
        ).expect("Create note 1");
        assert!(!note1.is_favorite);

        // 2. Mark note as favorite using set_favorite
        let fav_note = NoteRepository::set_favorite(&conn, &note1.id, true)
            .expect("Set favorite true");
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
        ).expect("Create note 2");
        assert!(!note2.is_favorite);

        let favs_after_second = NoteRepository::list_favorites(&conn).expect("List favorites");
        assert_eq!(favs_after_second.len(), 1, "Only favorited notes must appear in favorites");

        // 5. Toggle favorite on note 1 -> should become false
        let unfav_note = NoteRepository::toggle_favorite(&conn, &note1.id)
            .expect("Toggle favorite to false");
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
        assert_eq!(favs_after_delete.len(), 0, "Soft deleted notes must not appear in favorites");
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
        ).expect("Create note");
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
        assert!(fetched_reopen1.is_favorite, "Expected favorite = true after restart");

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
        assert!(!fetched_reopen2.is_favorite, "Expected favorite = false after restart");

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
        ).expect("Create note A");

        let _note_b = NoteRepository::create(
            &conn3,
            CreateNoteDto {
                title: Some("Note B".to_string()),
                is_favorite: Some(false),
                ..Default::default()
            },
        ).expect("Create note B");

        let note_c = NoteRepository::create(
            &conn3,
            CreateNoteDto {
                title: Some("Note C".to_string()),
                is_favorite: Some(true),
                ..Default::default()
            },
        ).expect("Create note C");

        drop(conn3);
        let conn4 = init_connection(&env.paths.database_file).expect("Reopen 3 failed");
        let multi_favs = NoteRepository::list_favorites(&conn4).expect("List multi favorites");
        assert_eq!(multi_favs.len(), 2, "Only Note A and Note C must be favorites");
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
        ).expect("Create note 1");

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Work Note 2 (Pinned)".to_string()),
                content: Some("Second work note, pinned".to_string()),
                is_pinned: Some(true),
                ..Default::default()
            },
        ).expect("Create note 2");

        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Personal Note".to_string()),
                content: Some("Personal note content".to_string()),
                ..Default::default()
            },
        ).expect("Create note 3");

        let note4 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Urgent Work Note (To be deleted)".to_string()),
                content: Some("Will be soft deleted".to_string()),
                ..Default::default()
            },
        ).expect("Create note 4");

        // 3. Assign tags
        TagRepository::add_tag_to_note(&conn, &note1.id, &tag_work.id).expect("Tag note 1");
        TagRepository::add_tag_to_note(&conn, &note2.id, &tag_work.id).expect("Tag note 2");
        TagRepository::add_tag_to_note(&conn, &note3.id, &tag_personal.id).expect("Tag note 3");
        TagRepository::add_tag_to_note(&conn, &note4.id, &tag_work.id).expect("Tag note 4 with work");
        TagRepository::add_tag_to_note(&conn, &note4.id, &tag_urgent.id).expect("Tag note 4 with urgent");

        // 4. Verify initial tag note counts
        let counts = TagRepository::get_tag_note_counts(&conn).expect("Get tag note counts");
        assert_eq!(counts.get(&tag_work.id).copied().unwrap_or(0), 3);
        assert_eq!(counts.get(&tag_urgent.id).copied().unwrap_or(0), 1);
        assert_eq!(counts.get(&tag_personal.id).copied().unwrap_or(0), 1);

        // 5. Query notes by tag_work: should return note2 (pinned), note4, note1 (ordered by is_pinned DESC, modified_at DESC)
        let work_notes = NoteRepository::list_by_tag(&conn, &tag_work.id).expect("List notes by work tag");
        assert_eq!(work_notes.len(), 3);
        assert_eq!(work_notes[0].id, note2.id, "Pinned note must come first");
        let work_ids: Vec<_> = work_notes.iter().map(|n| n.id.as_str()).collect();
        assert!(work_ids.contains(&note1.id.as_str()));
        assert!(work_ids.contains(&note4.id.as_str()));
        assert!(!work_ids.contains(&note3.id.as_str()), "Personal note must not be in work tag");

        // 6. Query notes by tag_personal
        let personal_notes = NoteRepository::list_by_tag(&conn, &tag_personal.id).expect("List notes by personal tag");
        assert_eq!(personal_notes.len(), 1);
        assert_eq!(personal_notes[0].id, note3.id);

        // 7. Soft delete note4
        NoteRepository::delete(&conn, &note4.id, true).expect("Soft delete note 4");

        // 8. Re-query tag_work: deleted note4 must be excluded
        let work_notes_after_delete = NoteRepository::list_by_tag(&conn, &tag_work.id).expect("List after delete");
        assert_eq!(work_notes_after_delete.len(), 2, "Soft-deleted note must be excluded from tag navigation");
        assert_eq!(work_notes_after_delete[0].id, note2.id);
        assert_eq!(work_notes_after_delete[1].id, note1.id);

        // 9. Re-query tag_urgent: had only note4, should now be empty
        let urgent_notes_after_delete = NoteRepository::list_by_tag(&conn, &tag_urgent.id).expect("List urgent after delete");
        assert_eq!(urgent_notes_after_delete.len(), 0);

        // 10. Verify tag counts exclude soft-deleted notes
        let counts_after = TagRepository::get_tag_note_counts(&conn).expect("Get tag note counts after delete");
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
        ).expect("Create notebook Work");

        let nb_personal = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Personal Notebook".to_string(),
                parent_id: None,
            },
        ).expect("Create notebook Personal");

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
        ).expect("Create note 1");
        TagRepository::add_tag_to_note(&conn, &note1.id, &tag_urgent.id).expect("Add urgent tag to note 1");

        // Note 2: In Personal notebook, tagged "urgent", IS favorite
        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Personal Errand".to_string()),
                notebook_id: Some(nb_personal.id.clone()),
                is_favorite: Some(true),
                ..Default::default()
            },
        ).expect("Create note 2");
        TagRepository::add_tag_to_note(&conn, &note2.id, &tag_urgent.id).expect("Add urgent tag to note 2");

        // Note 3: In Work notebook, NOT tagged, IS favorite
        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Work Reference".to_string()),
                notebook_id: Some(nb_work.id.clone()),
                is_favorite: Some(true),
                ..Default::default()
            },
        ).expect("Create note 3");

        // 4. Primary Context A: Notebook Navigation (Work Notebook)
        // Must return notes in Work Notebook (Note 1 and Note 3), isolating from tag and favorite states
        let work_notes = NoteRepository::list_filtered(&conn, false, Some(&nb_work.id), false).expect("List by notebook");
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
        ).expect("Create notebook");

        let n1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Notebook Note".to_string()),
                notebook_id: Some(nb.id.clone()),
                ..Default::default()
            },
        ).expect("Create note 1");

        let n2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Tagged Other Note".to_string()),
                ..Default::default()
            },
        ).expect("Create note 2");
        TagRepository::add_tag_to_note(&conn, &n2.id, &other_tag.id).expect("Add other tag");

        let _n3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Unfiled Note".to_string()),
                ..Default::default()
            },
        ).expect("Create note 3");

        // 3. Verify empty_tag note list is completely empty and unrelated notes are never returned
        let notes_for_empty = NoteRepository::list_by_tag(&conn, &empty_tag.id).expect("List notes for empty tag");
        assert_eq!(notes_for_empty.len(), 0, "Empty tag must return 0 notes");

        // 4. Verify tag count is 0
        let counts = TagRepository::get_tag_note_counts(&conn).expect("Get tag note counts");
        assert_eq!(counts.get(&empty_tag.id).copied().unwrap_or(0), 0);

        // 5. Assign note 1 to empty_tag
        TagRepository::add_tag_to_note(&conn, &n1.id, &empty_tag.id).expect("Add tag to note 1");
        let notes_after_assign = NoteRepository::list_by_tag(&conn, &empty_tag.id).expect("List after assign");
        assert_eq!(notes_after_assign.len(), 1);
        assert_eq!(notes_after_assign[0].id, n1.id);

        // 6. Remove note 1 from tag -> immediately transitions back to 0 notes (empty state)
        TagRepository::remove_tag_from_note(&conn, &n1.id, &empty_tag.id).expect("Remove tag from note 1");
        let notes_after_remove = NoteRepository::list_by_tag(&conn, &empty_tag.id).expect("List after remove");
        assert_eq!(notes_after_remove.len(), 0, "Must be empty again after tag removal");

        let counts_after_remove = TagRepository::get_tag_note_counts(&conn).expect("Counts after remove");
        assert_eq!(counts_after_remove.get(&empty_tag.id).copied().unwrap_or(0), 0);
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
        ).expect("Create notebook");

        let tag = TagRepository::create(&conn, "urgent").expect("Create tag");

        let n1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Non-favorite in notebook".to_string()),
                notebook_id: Some(nb.id.clone()),
                is_favorite: Some(false),
                ..Default::default()
            },
        ).expect("Create note 1");
        TagRepository::add_tag_to_note(&conn, &n1.id, &tag.id).expect("Tag note 1");

        let _n2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Non-favorite unfiled".to_string()),
                is_favorite: Some(false),
                ..Default::default()
            },
        ).expect("Create note 2");

        // 3. Favorites list must remain empty despite non-favorite notes existing
        let favs_still_empty = NoteRepository::list_favorites(&conn).expect("List favorites");
        assert_eq!(favs_still_empty.len(), 0, "Non-favorite notes must not appear in favorites");

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
        assert_eq!(favs_back_to_empty.len(), 0, "Must be empty again after unfavoriting");

        // 6. Favorite note 1 again, then soft-delete it -> must not appear in favorites
        NoteRepository::set_favorite(&conn, &n1.id, true).expect("Re-favorite");
        NoteRepository::delete(&conn, &n1.id, true).expect("Soft delete favorite note");

        let favs_after_soft_delete = NoteRepository::list_favorites(&conn).expect("List favorites");
        assert_eq!(favs_after_soft_delete.len(), 0, "Soft-deleted favorite note must not appear in favorites");
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
        ).expect("Create notebook");

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
        ).expect("Create note 1");

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note Without Tags 2".to_string()),
                content: Some("Unfiled note with zero tags".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                ..Default::default()
            },
        ).expect("Create note 2");

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

        let note1_survived = NoteRepository::get_by_id(&conn, &note1.id).expect("Get note 1").expect("Note 1 exists");
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
        ).expect("Create notebook");

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
        ).expect("Create note");

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
        let non_existent = NoteRepository::get_metadata(&conn, "non-existent-id")
            .expect("Query non-existent");
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
        ).expect("Create root");

        let nb_mid = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Backend".to_string(),
                parent_id: Some(nb_root.id.clone()),
            },
        ).expect("Create mid");

        let nb_leaf = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Storage".to_string(),
                parent_id: Some(nb_mid.id.clone()),
            },
        ).expect("Create leaf");

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
        ).expect("Create note");
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
        ).expect("Create unfiled");
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
        ).expect("Create note");

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
        ).expect("Update 1");

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
        ).expect("Create notebook");

        let updated2 = NoteRepository::update(
            &conn,
            &note.id,
            UpdateNoteDto {
                notebook_id: Some(nb.id.clone()),
                ..Default::default()
            },
        ).expect("Update 2");

        assert_eq!(updated2.created_at, initial_created_at, "created_at must remain immutable across notebook moves");
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
        assert_eq!(personal_notepad_lib::storage::models::format_display_name("txt"), "Plain Text");
        assert_eq!(personal_notepad_lib::storage::models::format_display_name("md"), "Markdown");
        assert_eq!(personal_notepad_lib::storage::models::format_display_name("TXT"), "Plain Text");
        assert_eq!(personal_notepad_lib::storage::models::format_display_name("MD"), "Markdown");
        assert_eq!(personal_notepad_lib::storage::models::format_display_name("unknown"), "Plain Text");

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
        ).expect("Create plain note");

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
        ).expect("Create markdown note");

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
        ).expect("Update format to md");

        assert_eq!(updated_to_md.content, original_content, "Raw content must remain intact on format toggle");
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
        ).expect("Update format back to txt");

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
        ).expect("Create Work");

        let projects = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Projects".to_string(),
                parent_id: Some(work.id.clone()),
            },
        ).expect("Create Projects");

        let releases = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Releases".to_string(),
                parent_id: Some(projects.id.clone()),
            },
        ).expect("Create Releases");

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
        ).expect("Create note in releases");

        // Validate NoteRepository::get_notebook_path
        let note_path = NoteRepository::get_notebook_path(&conn, &note.id).expect("note path");
        assert_eq!(note_path, "Work / Projects / Releases");
        assert!(!note_path.contains(&releases.id), "Must not expose internal ID");

        // Validate NoteRepository::get_metadata includes human-readable path
        let meta = NoteRepository::get_metadata(&conn, &note.id)
            .expect("get metadata")
            .expect("metadata exists");
        assert_eq!(meta.notebook_id, Some(releases.id.clone()));
        assert_eq!(meta.notebook_path, Some("Work / Projects / Releases".to_string()));

        // 4. Move note to "Work" and verify updated metadata path
        NoteRepository::move_to_notebook(&conn, &note.id, Some(&work.id)).expect("Move to Work");

        let work_meta = NoteRepository::get_metadata(&conn, &note.id)
            .expect("get work meta")
            .expect("meta exists");
        assert_eq!(work_meta.notebook_id, Some(work.id.clone()));
        assert_eq!(work_meta.notebook_path, Some("Work".to_string()));
        assert_eq!(NoteRepository::get_notebook_path(&conn, &note.id).expect("work path"), "Work");

        // 5. Unfile note and verify path is None in metadata and "Unfiled" from get_notebook_path
        NoteRepository::move_to_notebook(&conn, &note.id, None).expect("Move to Unfiled");

        let unfiled_meta = NoteRepository::get_metadata(&conn, &note.id)
            .expect("get unfiled meta")
            .expect("meta exists");
        assert_eq!(unfiled_meta.notebook_id, None);
        assert_eq!(unfiled_meta.notebook_path, None);
        assert_eq!(NoteRepository::get_notebook_path(&conn, &note.id).expect("unfiled path"), "Unfiled");
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
        ).expect("create note");

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
        ).expect("create notebook");

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
        ).expect("create note");

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
        TagRepository::remove_tag_from_note(&conn, &note.id, &tag_planning.id).expect("remove planning");

        // Verify tags list
        let tags_after_1 = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags after 1");
        assert_eq!(tags_after_1.len(), 1);
        assert_eq!(tags_after_1[0].name, "work");

        // Invariants check: note content, title, notebook, favorite must NOT be modified
        let note_check_1 = NoteRepository::get_by_id(&conn, &note.id)
            .expect("fetch note")
            .expect("note exists");

        assert_eq!(note_check_1.title, initial_title, "Title must not be modified by tag removal");
        assert_eq!(note_check_1.content, initial_content, "Content must not be modified by tag removal");
        assert_eq!(note_check_1.format, initial_format, "Format must not be modified by tag removal");
        assert_eq!(note_check_1.notebook_id, Some(nb.id.clone()), "Notebook must not be modified by tag removal");
        assert!(note_check_1.is_favorite, "Favorite state must not be modified by tag removal");

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
        ).expect("create note");

        assert_eq!(TagRepository::get_tags_for_note(&conn, &note.id).expect("tags").len(), 0);

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
        let matched: Vec<&str> = all_tags.iter()
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
        ).expect("create note");

        let tag_work = TagRepository::create(&conn, "work").expect("create work");

        // 2. Assign "work" tag for the first time
        TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id).expect("first assign");
        let initial_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags");
        assert_eq!(initial_tags.len(), 1);
        assert_eq!(initial_tags[0].name, "work");

        // 3. Attempt to assign "work" a second time (simulating user selecting it again)
        TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id).expect("second assign must succeed idempotently");

        // Verify the note still only has 1 tag, NOT ["work", "work"]
        let tags_after_second = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags after second");
        assert_eq!(tags_after_second.len(), 1, "Duplicate tag assignment must never produce duplicate tags");
        assert_eq!(tags_after_second[0].name, "work");

        // 4. Verify SQLite composite primary key on note_tags (note_id, tag_id)
        let count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags WHERE note_id = ?1 AND tag_id = ?2",
            rusqlite::params![&note.id, &tag_work.id],
            |r| r.get(0),
        ).expect("count query");
        assert_eq!(count, 1, "note_tags must contain exactly 1 row for (note_id, tag_id)");

        // Direct raw insert without IGNORE must trigger a UNIQUE / PRIMARY KEY failure
        let duplicate_insert_err = conn.execute(
            "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![&note.id, &tag_work.id],
        ).expect_err("Raw duplicate insert into note_tags must fail due to PRIMARY KEY");
        assert!(duplicate_insert_err.to_string().to_lowercase().contains("unique"));

        // 5. Verify set_tags_for_note with duplicate tag IDs in input list
        let mut conn_mut = conn;
        let batch_result = TagRepository::set_tags_for_note(
            &mut conn_mut,
            &note.id,
            &[tag_work.id.clone(), tag_work.id.clone(), tag_work.id.clone()],
        ).expect("set_tags_for_note");
        assert_eq!(batch_result.len(), 1, "Batch assignment must filter out duplicates");

        // 6. Verify persistence across connection reopen
        drop(conn_mut);
        let conn2 = init_connection(&env.paths.database_file).expect("reopen");
        let reopened_tags = TagRepository::get_tags_for_note(&conn2, &note.id).expect("get tags after reopen");
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
            ).expect("create note");

            TagRepository::add_tag_to_note(&conn, &note.id, &tag_id).expect("add tag to note");
            note_ids.push(note.id);
        }

        // Verify initial assignments
        let initial_junction_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags WHERE tag_id = ?1",
            rusqlite::params![&tag_id],
            |r| r.get(0),
        ).expect("query junction count");
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
        ).expect("rename tag to projects");
        assert_eq!(updated_tag.id, tag_id, "Tag ID must remain completely unchanged");
        assert_eq!(updated_tag.name, "projects");

        // 4. Invariant check: relationship remains note_tags.tag_id = tag_id; zero duplicate rows created
        let junction_count_after: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags WHERE tag_id = ?1",
            rusqlite::params![&tag_id],
            |r| r.get(0),
        ).expect("query junction count after");
        assert_eq!(junction_count_after, 20, "Total assignments must remain exactly 20 without duplicates");

        let total_junction_rows: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags",
            [],
            |r| r.get(0),
        ).expect("total junction rows");
        assert_eq!(total_junction_rows, 20);

        // 5. Verify all 20 notes automatically reflect updated tag name "projects"
        for nid in &note_ids {
            let tags = TagRepository::get_tags_for_note(&conn, nid).expect("get tags");
            assert_eq!(tags.len(), 1);
            assert_eq!(tags[0].name, "projects", "Note must automatically show updated name 'projects'");
            assert_eq!(tags[0].id, tag_id);

            let meta = NoteRepository::get_metadata(&conn, nid).expect("get meta").expect("meta exists");
            assert_eq!(meta.tags.len(), 1);
            assert_eq!(meta.tags[0].name, "projects");
        }

        // Verify get_all_notes_tag_names reflects "projects"
        let all_notes_map = TagRepository::get_all_notes_tag_names(&conn).expect("get all notes tags map");
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
        ).expect("create notebook");

        // 2. Create 25 notes with varied metadata (favorites, notebooks, unfiled)
        let mut created_notes = Vec::new();
        for i in 1..=25 {
            let is_fav = i % 2 == 0;
            let nb_id = if i % 3 == 0 { Some(nb.id.clone()) } else { None };

            let note = NoteRepository::create(
                &conn,
                CreateNoteDto {
                    title: Some(format!("Project Document #{i}")),
                    content: Some(format!("Detailed specs and requirements for phase {i}")),
                    format: Some(if i % 2 == 0 { "md".to_string() } else { "txt".to_string() }),
                    notebook_id: nb_id,
                    is_favorite: Some(is_fav),
                    is_pinned: Some(false),
                },
            ).expect("create note");

            // Assign "work" to all 25 notes
            TagRepository::add_tag_to_note(&conn, &note.id, &tag_id).expect("add tag");
            created_notes.push(note);
        }

        // Verify initial state: 25 notes exist and all 25 have tag "work"
        assert_eq!(NoteRepository::list(&conn, false).expect("list notes").len(), 25);
        let initial_junction_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags WHERE tag_id = ?1",
            rusqlite::params![&tag_id],
            |r| r.get(0),
        ).expect("junction count");
        assert_eq!(initial_junction_count, 25);

        // 3. Delete tag "work" (Task 39)
        let deleted = TagRepository::delete(&conn, &tag_id).expect("delete tag");
        assert!(deleted, "Tag deletion should succeed");

        // 4. Verify tag is gone from tags table
        assert!(TagRepository::get_by_id(&conn, &tag_id).expect("get by id").is_none());

        // 5. Invariant check: all 25 notes remain in the database, not deleted
        let notes_after = NoteRepository::list(&conn, false).expect("list notes after");
        assert_eq!(notes_after.len(), 25, "All 25 notes must remain in the database");

        // 6. Invariant check: all 25 tag assignments in note_tags disappeared
        let junction_count_after: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags WHERE tag_id = ?1",
            rusqlite::params![&tag_id],
            |r| r.get(0),
        ).expect("junction count after");
        assert_eq!(junction_count_after, 0, "All 25 tag assignments must disappear from note_tags");

        let total_junction_rows: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags",
            [],
            |r| r.get(0),
        ).expect("total junction rows");
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
            assert_eq!(meta.tags.len(), 0, "Note must have 0 tags after tag deletion");
        }

        // 8. Verify persistence across restart
        drop(conn);
        let conn2 = init_connection(&env.paths.database_file).expect("reopen");
        assert_eq!(NoteRepository::list(&conn2, false).expect("reopen notes list").len(), 25);
        assert!(TagRepository::get_by_id(&conn2, &tag_id).expect("reopen tag query").is_none());
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
        ).expect("create Projects notebook");

        let personal = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Personal".to_string(),
                parent_id: None,
            },
        ).expect("create Personal notebook");

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
        ).expect("create note");

        // 4. Assign tags = work, important
        TagRepository::add_tag_to_note(&conn, &note.id, &tag_work.id).expect("add work");
        TagRepository::add_tag_to_note(&conn, &note.id, &tag_important.id).expect("add important");

        // 5. Invariant check: These metadata properties must not conflict!
        // - "Favorites" view MUST show the note
        let fav_notes = NoteRepository::list_favorites(&conn).expect("list favorites");
        assert!(fav_notes.iter().any(|n| n.id == note.id), "Favorites must show note");

        // - "# work" tag view MUST show the note
        let work_notes = NoteRepository::list_by_tag(&conn, &tag_work.id).expect("list by work tag");
        assert!(work_notes.iter().any(|n| n.id == note.id), "# work must show note");

        // - "# important" tag view MUST show the note
        let important_notes = NoteRepository::list_by_tag(&conn, &tag_important.id).expect("list by important tag");
        assert!(important_notes.iter().any(|n| n.id == note.id), "# important must show note");

        // - "Projects" notebook view MUST show the note
        let project_notes = NoteRepository::list_by_notebook(&conn, &projects.id).expect("list by projects notebook");
        assert!(project_notes.iter().any(|n| n.id == note.id), "Projects notebook must show note");

        // - "All Notes" view MUST show the note
        let all_notes = NoteRepository::list(&conn, false).expect("list all notes");
        assert!(all_notes.iter().any(|n| n.id == note.id), "All notes must show note");

        // - "Unfiled" view MUST NOT show the note
        let unfiled_notes = NoteRepository::list_unfiled(&conn).expect("list unfiled");
        assert!(!unfiled_notes.iter().any(|n| n.id == note.id), "Unfiled must NOT show note");

        // 6. Check canonical NoteMetadata reflects all properties simultaneously
        let meta = NoteRepository::get_metadata(&conn, &note.id).expect("get meta").expect("meta exists");
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

        let work_notes_after_unfav = NoteRepository::list_by_tag(&conn, &tag_work.id).expect("work after unfav");
        assert!(work_notes_after_unfav.iter().any(|n| n.id == note.id), "Tag view still shows unfavorited note");

        let project_notes_after_unfav = NoteRepository::list_by_notebook(&conn, &projects.id).expect("projects after unfav");
        assert!(project_notes_after_unfav.iter().any(|n| n.id == note.id), "Notebook still shows unfavorited note");

        let fav_notes_after_unfav = NoteRepository::list_favorites(&conn).expect("favs after unfav");
        assert!(!fav_notes_after_unfav.iter().any(|n| n.id == note.id), "Favorites must no longer show unfavorited note");

        // 8. Re-favorite: note reappears in favorites
        let refav_note = NoteRepository::set_favorite(&conn, &note.id, true).expect("refavorite");
        assert!(refav_note.is_favorite);

        // 9. Move note from Projects -> Personal: favorite and tags remain intact
        let moved_note = NoteRepository::move_to_notebook(&conn, &note.id, Some(&personal.id)).expect("move note");
        assert_eq!(moved_note.notebook_id, Some(personal.id.clone()));
        assert!(moved_note.is_favorite);

        let personal_notes = NoteRepository::list_by_notebook(&conn, &personal.id).expect("personal notes");
        assert!(personal_notes.iter().any(|n| n.id == note.id));

        let fav_notes_after_move = NoteRepository::list_favorites(&conn).expect("favs after move");
        assert!(fav_notes_after_move.iter().any(|n| n.id == note.id));

        let work_notes_after_move = NoteRepository::list_by_tag(&conn, &tag_work.id).expect("work after move");
        assert!(work_notes_after_move.iter().any(|n| n.id == note.id));

        // 10. Persistence across connection restart
        drop(conn);
        let conn2 = init_connection(&env.paths.database_file).expect("reopen");
        let meta_reopen = NoteRepository::get_metadata(&conn2, &note.id).expect("meta reopen").expect("exists");
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
        ).expect("create Work notebook");

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
        ).expect("create note A");
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
        ).expect("create note B");
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
        ).expect("create note C");
        TagRepository::add_tag_to_note(&conn, &note_c.id, &tag_sec.id).expect("tag note C sec");

        // 4. Baseline assertions before deletion
        assert_eq!(NoteRepository::list(&conn, false).expect("list all").len(), 3);
        assert_eq!(NoteRepository::list_favorites(&conn).expect("list fav").len(), 2);
        assert_eq!(NoteRepository::list_by_notebook(&conn, &work.id).expect("list work").len(), 2);
        assert_eq!(NoteRepository::list_unfiled(&conn).expect("list unfiled").len(), 1);
        assert_eq!(NoteRepository::list_by_tag(&conn, &tag_rust.id).expect("list rust").len(), 2);
        assert_eq!(NoteRepository::list_by_tag(&conn, &tag_sec.id).expect("list sec").len(), 2);

        let counts_before = TagRepository::get_tag_note_counts(&conn).expect("counts before");
        assert_eq!(counts_before.get(&tag_rust.id).copied().unwrap_or(0), 2);
        assert_eq!(counts_before.get(&tag_sec.id).copied().unwrap_or(0), 2);

        let batch_tags_before = TagRepository::get_all_notes_tag_names(&conn).expect("batch before");
        assert_eq!(batch_tags_before.get(&note_a.id).map(|v| v.len()).unwrap_or(0), 2);
        assert_eq!(batch_tags_before.get(&note_b.id).map(|v| v.len()).unwrap_or(0), 1);
        assert_eq!(batch_tags_before.get(&note_c.id).map(|v| v.len()).unwrap_or(0), 1);

        // 5. Soft-delete Note A
        let deleted = NoteRepository::delete(&conn, &note_a.id, true).expect("soft delete Note A");
        assert!(deleted, "Soft delete must report true");

        // 6. Invariant verification: Note A must be excluded across ALL views!
        // - All Notes: excludes Note A (only B and C returned)
        let all_after = NoteRepository::list(&conn, false).expect("list all after delete");
        assert_eq!(all_after.len(), 2);
        assert!(!all_after.iter().any(|n| n.id == note_a.id), "All Notes must exclude soft-deleted Note A");

        // - Favorites: excludes Note A (only C returned)
        let favs_after = NoteRepository::list_favorites(&conn).expect("list fav after delete");
        assert_eq!(favs_after.len(), 1);
        assert_eq!(favs_after[0].id, note_c.id);
        assert!(!favs_after.iter().any(|n| n.id == note_a.id), "Favorites must exclude soft-deleted Note A");

        // - Notebook "Work": excludes Note A (only C returned)
        let work_after = NoteRepository::list_by_notebook(&conn, &work.id).expect("list work after delete");
        assert_eq!(work_after.len(), 1);
        assert_eq!(work_after[0].id, note_c.id);
        assert!(!work_after.iter().any(|n| n.id == note_a.id), "Notebook view must exclude soft-deleted Note A");

        // - Tag "rust": excludes Note A (only B returned)
        let rust_after = NoteRepository::list_by_tag(&conn, &tag_rust.id).expect("list rust after delete");
        assert_eq!(rust_after.len(), 1);
        assert_eq!(rust_after[0].id, note_b.id);
        assert!(!rust_after.iter().any(|n| n.id == note_a.id), "Tag rust view must exclude soft-deleted Note A");

        // - Tag "security": excludes Note A (only C returned)
        let sec_after = NoteRepository::list_by_tag(&conn, &tag_sec.id).expect("list sec after delete");
        assert_eq!(sec_after.len(), 1);
        assert_eq!(sec_after[0].id, note_c.id);
        assert!(!sec_after.iter().any(|n| n.id == note_a.id), "Tag security view must exclude soft-deleted Note A");

        // - Tag note counts: Note A excluded from counts (rust=1, sec=1)
        let counts_after = TagRepository::get_tag_note_counts(&conn).expect("counts after delete");
        assert_eq!(counts_after.get(&tag_rust.id).copied().unwrap_or(0), 1);
        assert_eq!(counts_after.get(&tag_sec.id).copied().unwrap_or(0), 1);

        // - Batch note tag names: Note A excluded from active note-tag batch
        let batch_after = TagRepository::get_all_notes_tag_names(&conn).expect("batch after delete");
        assert!(!batch_after.contains_key(&note_a.id), "Batch tag names must exclude soft-deleted Note A");

        // - Direct query by ID: returns note with is_deleted = true and valid deleted_at
        let direct_a = NoteRepository::get_by_id(&conn, &note_a.id).expect("get by id").expect("exists");
        assert!(direct_a.is_deleted);
        assert!(direct_a.deleted_at.is_some());

        // - Include deleted query: returns all 3 notes
        let with_deleted = NoteRepository::list(&conn, true).expect("list with deleted");
        assert_eq!(with_deleted.len(), 3);

        // 7. Verify persistence across connection restart
        drop(conn);
        let conn2 = init_connection(&env.paths.database_file).expect("reopen");
        assert_eq!(NoteRepository::list(&conn2, false).expect("reopen list").len(), 2);
        assert_eq!(NoteRepository::list_favorites(&conn2).expect("reopen favs").len(), 1);
        assert_eq!(NoteRepository::list_by_notebook(&conn2, &work.id).expect("reopen work").len(), 1);
        assert_eq!(NoteRepository::list_by_tag(&conn2, &tag_rust.id).expect("reopen rust").len(), 1);
        assert_eq!(NoteRepository::list_by_tag(&conn2, &tag_sec.id).expect("reopen sec").len(), 1);

        // 8. Restore Note A
        let restored_a = NoteRepository::restore(&conn2, &note_a.id).expect("restore Note A");
        assert!(!restored_a.is_deleted);
        assert!(restored_a.deleted_at.is_none());
        assert!(restored_a.is_favorite, "Favorite status must be preserved upon restore");
        assert_eq!(restored_a.notebook_id, Some(work.id.clone()), "Notebook must be preserved upon restore");

        // All views reflect restored Note A immediately
        assert_eq!(NoteRepository::list(&conn2, false).expect("list restored").len(), 3);
        assert_eq!(NoteRepository::list_favorites(&conn2).expect("favs restored").len(), 2);
        assert_eq!(NoteRepository::list_by_notebook(&conn2, &work.id).expect("work restored").len(), 2);
        assert_eq!(NoteRepository::list_by_tag(&conn2, &tag_rust.id).expect("rust restored").len(), 2);
        assert_eq!(NoteRepository::list_by_tag(&conn2, &tag_sec.id).expect("sec restored").len(), 2);

        let restored_counts = TagRepository::get_tag_note_counts(&conn2).expect("counts restored");
        assert_eq!(restored_counts.get(&tag_rust.id).copied().unwrap_or(0), 2);
        assert_eq!(restored_counts.get(&tag_sec.id).copied().unwrap_or(0), 2);

        // Canonical metadata after restore has everything intact
        let meta_restored = NoteRepository::get_metadata(&conn2, &note_a.id).expect("meta").expect("meta exists");
        assert!(meta_restored.is_favorite);
        assert_eq!(meta_restored.notebook_path, Some("Work".to_string()));
        assert_eq!(meta_restored.tags.len(), 2);

        // 9. Soft-delete unfiled Note B
        NoteRepository::delete(&conn2, &note_b.id, true).expect("soft delete unfiled Note B");
        assert_eq!(NoteRepository::list_unfiled(&conn2).expect("unfiled after B deleted").len(), 0);
        assert_eq!(NoteRepository::list_by_tag(&conn2, &tag_rust.id).expect("rust after B deleted").len(), 1);

        // 10. Hard-delete Note B (soft = false): cascade removes from note_tags, leaves tags intact
        let hard_deleted = NoteRepository::delete(&conn2, &note_b.id, false).expect("hard delete Note B");
        assert!(hard_deleted);
        assert!(NoteRepository::get_by_id(&conn2, &note_b.id).expect("get Note B").is_none());

        // Tag "rust" itself is preserved
        assert!(TagRepository::get_by_id(&conn2, &tag_rust.id).expect("tag rust").is_some());
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
        ).expect("create Work");

        let projects = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Projects".to_string(),
                parent_id: Some(work.id.clone()),
            },
        ).expect("create Projects");

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
        ).expect("create note 1");

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
        ).expect("create note 2");
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
        ).expect("create note 3");

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
        ).expect("create Projects");

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
        ).expect("create note 1");
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
        ).expect("create note 2");
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
        ).expect("create note 3");
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
        ).expect("create note 4");
        TagRepository::add_tag_to_note(&conn, &note4.id, &tag_specs.id).expect("tag 4");

        // 3. Verify strict modified_at DESC ordering across ALL views:
        // - All Notes: [Note 4, Note 3, Note 2, Note 1]
        let all_notes = NoteRepository::list(&conn, false).expect("list all");
        let all_ids: Vec<_> = all_notes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(all_ids, vec![note4.id.as_str(), note3.id.as_str(), note2.id.as_str(), note1.id.as_str()]);

        // - Favorites: [Note 4, Note 2, Note 1]
        let fav_notes = NoteRepository::list_favorites(&conn).expect("list favs");
        let fav_ids: Vec<_> = fav_notes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(fav_ids, vec![note4.id.as_str(), note2.id.as_str(), note1.id.as_str()]);

        // - Tag specs: [Note 4, Note 3, Note 2, Note 1]
        let tag_notes = NoteRepository::list_by_tag(&conn, &tag_specs.id).expect("list by tag");
        let tag_note_ids: Vec<_> = tag_notes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(tag_note_ids, vec![note4.id.as_str(), note3.id.as_str(), note2.id.as_str(), note1.id.as_str()]);

        // - Notebook Projects: [Note 3, Note 2, Note 1]
        let project_notes = NoteRepository::list_by_notebook(&conn, &projects.id).expect("list by notebook");
        let project_ids: Vec<_> = project_notes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(project_ids, vec![note3.id.as_str(), note2.id.as_str(), note1.id.as_str()]);

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
        ).expect("update note 1");

        let all_after_update = NoteRepository::list(&conn, false).expect("list all after update");
        let ids_after_update: Vec<_> = all_after_update.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids_after_update, vec![note1.id.as_str(), note4.id.as_str(), note3.id.as_str(), note2.id.as_str()]);

        let favs_after_update = NoteRepository::list_favorites(&conn).expect("favs after update");
        let fav_ids_after_update: Vec<_> = favs_after_update.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(fav_ids_after_update, vec![note1.id.as_str(), note4.id.as_str(), note2.id.as_str()]);

        let project_after_update = NoteRepository::list_by_notebook(&conn, &projects.id).expect("projects after update");
        let project_ids_after_update: Vec<_> = project_after_update.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(project_ids_after_update, vec![note1.id.as_str(), note3.id.as_str(), note2.id.as_str()]);

        // 5. Pin Note 3 -> jumps ahead of all unpinned notes regardless of timestamps
        NoteRepository::update(
            &conn,
            &note3.id,
            UpdateNoteDto {
                is_pinned: Some(true),
                ..Default::default()
            },
        ).expect("pin note 3");

        let all_pinned = NoteRepository::list(&conn, false).expect("list all pinned");
        let pinned_ids: Vec<_> = all_pinned.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(pinned_ids, vec![note3.id.as_str(), note1.id.as_str(), note4.id.as_str(), note2.id.as_str()]);

        let project_pinned = NoteRepository::list_by_notebook(&conn, &projects.id).expect("project pinned");
        let project_pinned_ids: Vec<_> = project_pinned.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(project_pinned_ids, vec![note3.id.as_str(), note1.id.as_str(), note2.id.as_str()]);

        // 6. Persistence across connection restart
        drop(conn);
        let conn2 = init_connection(&env.paths.database_file).expect("reopen");
        let all_reopen = NoteRepository::list(&conn2, false).expect("all reopen");
        let reopen_ids: Vec<_> = all_reopen.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(reopen_ids, vec![note3.id.as_str(), note1.id.as_str(), note4.id.as_str(), note2.id.as_str()]);
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
        ).expect("create note");

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
        assert!(elapsed.as_millis() < 5000, "100 tag creations should be fast (< 5s)");

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

            let research_tag = TagRepository::create(&conn, "research & ideas").expect("create Ampersand tag");
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
            ).expect("create note 1");
            note1_id = note1.id;

            let note2 = NoteRepository::create(
                &conn,
                CreateNoteDto {
                    title: Some("Work & Coffee".to_string()),
                    content: Some("Research notes at the corner café".to_string()),
                    ..Default::default()
                },
            ).expect("create note 2");
            note2_id = note2.id;

            let note3 = NoteRepository::create(
                &conn,
                CreateNoteDto {
                    title: Some("Project Planning".to_string()),
                    content: Some("2026 roadmap for project-x".to_string()),
                    ..Default::default()
                },
            ).expect("create note 3");
            note3_id = note3.id;

            // Assign tags:
            // Note 1 -> यात्रा, 旅行
            TagRepository::add_tag_to_note(&conn, &note1_id, &hindi_tag_id).expect("add hindi tag to note 1");
            TagRepository::add_tag_to_note(&conn, &note1_id, &cjk_tag_id).expect("add cjk tag to note 1");

            // Note 2 -> work, café, research & ideas
            TagRepository::add_tag_to_note(&conn, &note2_id, &work_tag_id).expect("add work tag to note 2");
            TagRepository::add_tag_to_note(&conn, &note2_id, &cafe_tag_id).expect("add cafe tag to note 2");
            TagRepository::add_tag_to_note(&conn, &note2_id, &research_tag_id).expect("add research tag to note 2");

            // Note 3 -> work, 2026, project-x
            TagRepository::add_tag_to_note(&conn, &note3_id, &work_tag_id).expect("add work tag to note 3");
            TagRepository::add_tag_to_note(&conn, &note3_id, &num_tag_id).expect("add num tag to note 3");
            TagRepository::add_tag_to_note(&conn, &note3_id, &proj_tag_id).expect("add proj tag to note 3");

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
            let hindi_notes = NoteRepository::list_by_tag(&conn, &hindi_tag_id).expect("filter by hindi tag");
            assert_eq!(hindi_notes.len(), 1);
            assert_eq!(hindi_notes[0].id, note1_id);

            let cjk_notes = NoteRepository::list_by_tag(&conn, &cjk_tag_id).expect("filter by cjk tag");
            assert_eq!(cjk_notes.len(), 1);
            assert_eq!(cjk_notes[0].id, note1_id);

            let cafe_notes = NoteRepository::list_by_tag(&conn, &cafe_tag_id).expect("filter by cafe tag");
            assert_eq!(cafe_notes.len(), 1);
            assert_eq!(cafe_notes[0].id, note2_id);

            let work_notes = NoteRepository::list_by_tag(&conn, &work_tag_id).expect("filter by work tag");
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
            ).expect("rename hindi tag");
            assert_eq!(renamed_hindi.name, "तीर्थ-यात्रा 🗺️");

            let renamed_cafe = TagRepository::update(
                &conn,
                &cafe_tag_id,
                personal_notepad_lib::storage::models::UpdateTagDto {
                    name: "Grande Café ☕".to_string(),
                },
            ).expect("rename cafe tag");
            assert_eq!(renamed_cafe.name, "Grande Café ☕");

            // Verify note tag names updated automatically
            let note1_tags_after_rename = TagRepository::get_tags_for_note(&conn, &note1_id).expect("note 1 tags after rename");
            let note1_tag_names_after: Vec<String> = note1_tags_after_rename.into_iter().map(|t| t.name).collect();
            assert!(note1_tag_names_after.contains(&"तीर्थ-यात्रा 🗺️".to_string()));

            // 5. Remove tag from note
            TagRepository::remove_tag_from_note(&conn, &note2_id, &cafe_tag_id).expect("remove cafe tag from note 2");
            let note2_tags_after_remove = TagRepository::get_tags_for_note(&conn, &note2_id).expect("note 2 tags after remove");
            assert_eq!(note2_tags_after_remove.len(), 2);
            let note2_names_after: Vec<String> = note2_tags_after_remove.into_iter().map(|t| t.name).collect();
            assert!(!note2_names_after.contains(&"Grande Café ☕".to_string()));
            assert!(note2_names_after.contains(&"work".to_string()));
            assert!(note2_names_after.contains(&"research & ideas".to_string()));

            // Filter for Grande Café ☕ now returns 0 notes
            let cafe_notes_after_remove = NoteRepository::list_by_tag(&conn, &cafe_tag_id).expect("filter cafe after remove");
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
            let note1_tags = TagRepository::get_tags_for_note(&conn2, &note1_id).expect("note 1 tags session 2");
            assert_eq!(note1_tags.len(), 2);
            let note1_tag_names: Vec<String> = note1_tags.into_iter().map(|t| t.name).collect();
            assert!(note1_tag_names.contains(&"तीर्थ-यात्रा 🗺️".to_string()));
            assert!(note1_tag_names.contains(&"旅行".to_string()));

            // 3. Verify filtering by Unicode tag returns correct note
            let filtered_hindi = NoteRepository::list_by_tag(&conn2, &hindi_tag_id).expect("filter hindi session 2");
            assert_eq!(filtered_hindi.len(), 1);
            assert_eq!(filtered_hindi[0].id, note1_id);

            let filtered_cjk = NoteRepository::list_by_tag(&conn2, &cjk_tag_id).expect("filter cjk session 2");
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

            let proj = TagRepository::create(&conn, "Project / Planning").expect("create Project / Planning tag");
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
            ).expect("create note 1");
            note1_id = note1.id;

            let note2 = NoteRepository::create(
                &conn,
                CreateNoteDto {
                    title: Some("Product Roadmap".to_string()),
                    content: Some("Planning notes for next quarter release".to_string()),
                    ..Default::default()
                },
            ).expect("create note 2");
            note2_id = note2.id;

            let note3 = NoteRepository::create(
                &conn,
                CreateNoteDto {
                    title: Some("User Interface Overhaul".to_string()),
                    content: Some("Design system and accessibility guidelines".to_string()),
                    ..Default::default()
                },
            ).expect("create note 3");
            note3_id = note3.id;

            // Note 1 -> C++, C#, R&D
            TagRepository::add_tag_to_note(&conn, &note1_id, &cpp_id).expect("assign C++");
            TagRepository::add_tag_to_note(&conn, &note1_id, &csharp_id).expect("assign C#");
            TagRepository::add_tag_to_note(&conn, &note1_id, &rd_id).expect("assign R&D");

            // Note 2 -> R&D, Project / Planning, Q4 2026
            TagRepository::add_tag_to_note(&conn, &note2_id, &rd_id).expect("assign R&D to note 2");
            TagRepository::add_tag_to_note(&conn, &note2_id, &proj_id).expect("assign Project / Planning");
            TagRepository::add_tag_to_note(&conn, &note2_id, &q4_id).expect("assign Q4 2026");

            // Note 3 -> Design & UX, Project / Planning
            TagRepository::add_tag_to_note(&conn, &note3_id, &design_id).expect("assign Design & UX");
            TagRepository::add_tag_to_note(&conn, &note3_id, &proj_id).expect("assign Project / Planning to note 3");

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

            let proj_notes = NoteRepository::list_by_tag(&conn, &proj_id).expect("filter Project / Planning");
            assert_eq!(proj_notes.len(), 2);
            assert!(proj_notes.iter().any(|n| n.id == note2_id));
            assert!(proj_notes.iter().any(|n| n.id == note3_id));

            let design_notes = NoteRepository::list_by_tag(&conn, &design_id).expect("filter Design & UX");
            assert_eq!(design_notes.len(), 1);
            assert_eq!(design_notes[0].id, note3_id);

            // 5. Rename special character tags
            let renamed_cpp = TagRepository::update(
                &conn,
                &cpp_id,
                personal_notepad_lib::storage::models::UpdateTagDto {
                    name: "Modern C++ (20/23)".to_string(),
                },
            ).expect("rename C++");
            assert_eq!(renamed_cpp.name, "Modern C++ (20/23)");

            let renamed_proj = TagRepository::update(
                &conn,
                &proj_id,
                personal_notepad_lib::storage::models::UpdateTagDto {
                    name: "Strategic Planning & Dev / Ops".to_string(),
                },
            ).expect("rename Project / Planning");
            assert_eq!(renamed_proj.name, "Strategic Planning & Dev / Ops");

            // 6. Remove tag from note
            TagRepository::remove_tag_from_note(&conn, &note1_id, &csharp_id).expect("remove C# from note 1");
            let note1_tags_after = TagRepository::get_tags_for_note(&conn, &note1_id).expect("note 1 tags after remove");
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
            let note2_tags = TagRepository::get_tags_for_note(&conn2, &note2_id).expect("note 2 tags session 2");
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
            let filtered_proj = NoteRepository::list_by_tag(&conn2, &proj_id).expect("filter renamed proj");
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
                    content: Some("Testing 20+ tag assignment, rendering, and performance.".to_string()),
                    ..Default::default()
                },
            ).expect("create note");
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
            let unique_tag_ids: std::collections::HashSet<String> = note_tags.iter().map(|t| t.id.clone()).collect();
            assert_eq!(unique_tag_ids.len(), 22);

            // 5. Batch resolution performance & accuracy
            let batch = TagRepository::get_all_notes_tag_names(&conn).expect("batch tags");
            let batch_tags = batch.get(&note_id).expect("batch for note");
            assert_eq!(batch_tags.len(), 22);

            // 6. Tag note counts: 22 tags have count 1, 3 tags have count 0
            let counts = TagRepository::get_tag_note_counts(&conn).expect("counts");
            let assigned_count = created_tag_ids.iter().take(22).filter(|id| *counts.get(*id).unwrap_or(&0) == 1).count();
            assert_eq!(assigned_count, 22);
            let unassigned_count = created_tag_ids.iter().skip(22).filter(|id| *counts.get(*id).unwrap_or(&0) == 0).count();
            assert_eq!(unassigned_count, 3);

            // 7. Remove 5 tags from note
            for tag_id in created_tag_ids.iter().take(5) {
                TagRepository::remove_tag_from_note(&conn, &note_id, tag_id).expect("remove tag");
            }

            // Exactly 17 tags remain
            let note_tags_after_del = TagRepository::get_tags_for_note(&conn, &note_id).expect("note tags after remove");
            assert_eq!(note_tags_after_del.len(), 17);

            // Verify removed tag no longer returns the note in filtering
            let removed_tag_filter = NoteRepository::list_by_tag(&conn, &created_tag_ids[0]).expect("filter removed tag");
            assert_eq!(removed_tag_filter.len(), 0);

            // Verify remaining tag still returns the note in filtering
            let remaining_tag_filter = NoteRepository::list_by_tag(&conn, &created_tag_ids[10]).expect("filter remaining tag");
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
            let note_tags_reloaded = TagRepository::get_tags_for_note(&conn2, &note_id).expect("get note tags session 2");
            assert_eq!(note_tags_reloaded.len(), 17);

            // 3. Re-assign 1 tag after restart
            TagRepository::add_tag_to_note(&conn2, &note_id, &created_tag_ids[0]).expect("re-add tag 0");
            let note_tags_after_readd = TagRepository::get_tags_for_note(&conn2, &note_id).expect("get note tags after re-add");
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
            assert!(large_content.len() >= 100 * 1024, "Content must exceed 100KB (was {} bytes)", large_content.len());

            // 2. Create Notebooks "Research" and "Archives"
            let research_nb = NotebookRepository::create(
                &conn,
                CreateNotebookDto {
                    name: "Research".to_string(),
                    parent_id: None,
                },
            ).expect("create Research notebook");

            let archives_nb = NotebookRepository::create(
                &conn,
                CreateNotebookDto {
                    name: "Archives".to_string(),
                    parent_id: None,
                },
            ).expect("create Archives notebook");
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
            ).expect("create large note");
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
            let moved_note = NoteRepository::move_to_notebook(&conn, &note_id, Some(&archive_nb_id)).expect("move note");
            assert_eq!(moved_note.notebook_id.as_deref(), Some(archive_nb_id.as_str()));
            assert_eq!(moved_note.content.len(), large_content.len());

            // 8. Rename tag 3 ("performance" -> "extreme-performance-2026 ⚡")
            let renamed_tag = TagRepository::update(
                &conn,
                &tag_ids[2],
                personal_notepad_lib::storage::models::UpdateTagDto {
                    name: "extreme-performance-2026 ⚡".to_string(),
                },
            ).expect("rename tag");
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
            ).expect("autosave note");

            assert_eq!(updated_note.content.len(), final_content.len());
            assert_eq!(updated_note.content, final_content);
            assert_eq!(updated_note.title, "Large Regression Analysis 100KB+");
            assert!(updated_note.is_favorite);
            assert_eq!(updated_note.notebook_id.as_deref(), Some(archive_nb_id.as_str()));
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
            ).expect("create note 1");
            note1_id = note1.id;

            let note2 = NoteRepository::create(
                &conn,
                CreateNoteDto {
                    title: Some("Transaction Safety Note 2".to_string()),
                    content: Some("Testing second note association retention".to_string()),
                    ..Default::default()
                },
            ).expect("create note 2");
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

            assert_eq!(TagRepository::get_tags_for_note(&conn, &note1_id).expect("n1 tags").len(), 1);
            assert_eq!(TagRepository::get_tags_for_note(&conn, &note2_id).expect("n2 tags").len(), 2);

            // 4. Test atomic deletion: TagRepository::delete wraps delete note_tags + delete tag in a transaction
            let deleted = TagRepository::delete(&conn, &tag1_id).expect("delete tag 1 transactionally");
            assert!(deleted, "Tag 1 deletion must return true");

            // Verify tag1 removed from tags table
            let tag1_lookup = TagRepository::get_by_id(&conn, &tag1_id).expect("lookup tag 1");
            assert!(tag1_lookup.is_none());

            // Verify tag1 removed from note_tags for both notes
            let n1_tags = TagRepository::get_tags_for_note(&conn, &note1_id).expect("n1 tags after delete");
            assert_eq!(n1_tags.len(), 0);

            let n2_tags = TagRepository::get_tags_for_note(&conn, &note2_id).expect("n2 tags after delete");
            assert_eq!(n2_tags.len(), 1);
            assert_eq!(n2_tags[0].id, tag2_id);

            // Verify notes themselves are completely untouched
            let fetched_n1 = NoteRepository::get_by_id(&conn, &note1_id).expect("fetch n1").expect("n1 exists");
            assert_eq!(fetched_n1.title, "Transaction Safety Note 1");
            assert_eq!(fetched_n1.content, "Testing transactional tag deletion");

            let fetched_n2 = NoteRepository::get_by_id(&conn, &note2_id).expect("fetch n2").expect("n2 exists");
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
                ).expect("insert aborted tag");

                tx.execute(
                    "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
                    rusqlite::params![note1_id, abort_tag_id],
                ).expect("insert aborted junction");

                // Drop tx without calling tx.commit() -> triggers automatic ROLLBACK
                drop(tx);
            }

            // Verify rollback: aborted tag and its junction do not exist in SQLite
            let aborted_lookup = TagRepository::get_by_name(&conn, "aborted-tag").expect("lookup aborted");
            assert!(aborted_lookup.is_none(), "Rolled back tag must not exist");

            let n1_tags_after_rollback = TagRepository::get_tags_for_note(&conn, &note1_id).expect("n1 tags after rollback");
            assert_eq!(n1_tags_after_rollback.len(), 0, "Rolled back junction must not exist");
        }

        // Session 2: Application restart verification
        {
            let mut conn2 = init_connection(&db_path).expect("Session 2 connection failed");
            run_migrations(&mut conn2).expect("Session 2 migration failed");

            // Verify note 2 still has tag 2 and only tag 2
            let n2_tags = TagRepository::get_tags_for_note(&conn2, &note2_id).expect("n2 tags session 2");
            assert_eq!(n2_tags.len(), 1);
            assert_eq!(n2_tags[0].id, tag2_id);
            assert_eq!(n2_tags[0].name, "tx-tag-2");

            // Verify note 1 has 0 tags
            let n1_tags = TagRepository::get_tags_for_note(&conn2, &note1_id).expect("n1 tags session 2");
            assert_eq!(n1_tags.len(), 0);
        }
    }

    #[test]
    fn test_phase5_task_53_database_integrity_and_orphan_prevention() {
        let env = TestEnv::new();
        let mut conn = init_connection(&env.paths.database_file).expect("Connection failed");
        run_migrations(&mut conn).expect("Migration failed");

        // 1. Initial SQLite integrity & foreign key check
        let integrity_init: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).expect("integrity check");
        assert_eq!(integrity_init, "ok");

        let mut fk_stmt = conn.prepare("PRAGMA foreign_key_check").expect("prepare fk check");
        let fk_violations: Vec<String> = fk_stmt.query_map([], |r| r.get(0)).expect("query fk").map(|r| r.unwrap()).collect();
        assert_eq!(fk_violations.len(), 0, "Initial DB must have 0 foreign key violations");

        // 2. Populate notes, tags, and note_tags
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Integrity Note 1".to_string()),
                content: Some("Sample content".to_string()),
                ..Default::default()
            },
        ).expect("create note 1");

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Integrity Note 2".to_string()),
                content: Some("Sample content 2".to_string()),
                ..Default::default()
            },
        ).expect("create note 2");

        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Integrity Note 3".to_string()),
                content: Some("Sample content 3".to_string()),
                ..Default::default()
            },
        ).expect("create note 3");

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
        assert!(dup_insert.is_err(), "Duplicate (note_id, tag_id) pair must violate PRIMARY KEY constraint");

        // Verify query for any duplicates across entire table returns 0
        let dup_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM (SELECT note_id, tag_id FROM note_tags GROUP BY note_id, tag_id HAVING COUNT(*) > 1)",
            [],
            |r| r.get(0),
        ).expect("dup query");
        assert_eq!(dup_count, 0, "Database must never contain duplicate note_tags pairs");

        // 4. Test: No tag without valid ID
        let empty_or_null_tag_ids: i64 = conn.query_row(
            "SELECT COUNT(1) FROM tags WHERE id IS NULL OR length(trim(id)) = 0",
            [],
            |r| r.get(0),
        ).expect("tag id query");
        assert_eq!(empty_or_null_tag_ids, 0, "No tag without valid non-empty ID may exist");

        // Attempting to insert tag with NULL id fails
        let null_id_insert = conn.execute(
            "INSERT INTO tags (id, name, created_at) VALUES (NULL, 'null-tag', '2026-09-30T00:00:00Z')",
            [],
        );
        assert!(null_id_insert.is_err(), "Tag with NULL ID must be rejected by SQLite NOT NULL constraint");

        // 5. Test: No note deleted because tag deleted
        let total_notes_before = NoteRepository::list(&conn, true).expect("list all notes").len();
        assert_eq!(total_notes_before, 3);

        // Delete tag2
        TagRepository::delete(&conn, &tag2.id).expect("delete tag 2");

        let total_notes_after = NoteRepository::list(&conn, true).expect("list all notes").len();
        assert_eq!(total_notes_after, total_notes_before, "Deleting a tag must NEVER delete any notes");

        // 6. Test: No orphan note_tags
        // After deleting tag2, junction records referencing tag2 must be 0
        let tag2_junction_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags WHERE tag_id = ?1",
            rusqlite::params![tag2.id],
            |r| r.get(0),
        ).expect("tag2 junction count");
        assert_eq!(tag2_junction_count, 0, "All note_tags referencing deleted tag must be removed");

        // Hard-delete note3 directly:
        conn.execute("DELETE FROM notes WHERE id = ?1", rusqlite::params![note3.id]).expect("delete note 3");

        // Note 3 junction records must be cleaned up automatically via FOREIGN KEY cascade
        let note3_junction_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags WHERE note_id = ?1",
            rusqlite::params![note3.id],
            |r| r.get(0),
        ).expect("note3 junction count");
        assert_eq!(note3_junction_count, 0, "Junction records for deleted note must cascade-delete");

        // Global orphan query: check for ANY orphaned note_tags
        let orphan_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags
             WHERE note_id NOT IN (SELECT id FROM notes)
                OR tag_id NOT IN (SELECT id FROM tags)",
            [],
            |r| r.get(0),
        ).expect("orphan query");
        assert_eq!(orphan_count, 0, "Database must never contain orphan note_tags rows");

        // 7. Test: No invalid favorite values
        let invalid_fav_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM notes WHERE is_favorite NOT IN (0, 1)",
            [],
            |r| r.get(0),
        ).expect("fav check");
        assert_eq!(invalid_fav_count, 0, "is_favorite must strictly be binary 0 or 1");

        // 8. Final SQLite integrity and foreign key check
        let final_integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).expect("integrity check");
        assert_eq!(final_integrity, "ok");

        let mut final_fk_stmt = conn.prepare("PRAGMA foreign_key_check").expect("prepare fk check");
        let final_fk_violations: Vec<String> = final_fk_stmt.query_map([], |r| r.get(0)).expect("query fk").map(|r| r.unwrap()).collect();
        assert_eq!(final_fk_violations.len(), 0, "Final DB must have 0 foreign key violations");
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
        assert!(!not_found_delete, "Deleting non-existent tag should safely return false");

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
        assert!(db_path.parent().unwrap().exists(), "Storage directory must exist locally");

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
            ).expect("offline notebook creation");
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
            ).expect("offline note creation");
            note_id = note.id;

            // 3. Tags work completely offline
            let tag = TagRepository::create(&conn, "local-first").expect("offline tag creation");
            tag_id = tag.id;

            TagRepository::add_tag_to_note(&conn, &note_id, &tag_id).expect("offline tag assign");

            // 4. Favorites work completely offline
            let fav = NoteRepository::toggle_favorite(&conn, &note_id).expect("offline toggle favorite");
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

            let tags_reloaded = TagRepository::get_tags_for_note(&conn2, &note_id).expect("offline tags query");
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
                content: Some("Deterministic user content only. No LLMs, no automatic summaries.".to_string()),
                ..Default::default()
            },
        ).expect("create note");

        // Verify note has exactly 0 tags assigned automatically
        let initial_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("initial tags");
        assert_eq!(initial_tags.len(), 0, "No automatic or AI tags may be assigned without user action");

        // Verify favorite status is strictly 0 (not automatically categorized)
        let note_reloaded = NoteRepository::get_by_id(&conn, &note.id).expect("get note").expect("note exists");
        assert!(!note_reloaded.is_favorite, "Note must not be marked favorite by AI or heuristics");

        // 2. User explicitly creates and assigns a tag
        let manual_tag = TagRepository::create(&conn, "user-created").expect("manual tag create");
        TagRepository::add_tag_to_note(&conn, &note.id, &manual_tag.id).expect("manual tag assign");

        let tags_after = TagRepository::get_tags_for_note(&conn, &note.id).expect("tags after manual assign");
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
        assert_eq!(initial_tags.len(), 0, "Fresh database must not contain any pre-seeded or mock tags");

        // 2. A clean database must contain 0 note_tags associations
        let initial_tag_counts = TagRepository::get_tag_note_counts(&conn).expect("tag note counts");
        assert_eq!(initial_tag_counts.len(), 0);

        let initial_junction_rows: i64 = conn.query_row(
            "SELECT COUNT(1) FROM note_tags",
            [],
            |r| r.get(0),
        ).expect("query note_tags");
        assert_eq!(initial_junction_rows, 0, "Fresh database must contain 0 junction rows");

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
        let tag_important = TagRepository::create(&conn, "important").expect("Test A: create important");
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
        ).expect("Test B: create QA Note 1");

        TagRepository::add_tag_to_note(&conn, &note1.id, &tag_work.id).expect("Test B: assign work");
        TagRepository::add_tag_to_note(&conn, &note1.id, &tag_important.id).expect("Test B: assign important");

        let note1_tags_b = TagRepository::get_tags_for_note(&conn, &note1.id).expect("Test B: get note tags");
        assert_eq!(note1_tags_b.len(), 2);
        let note1_tag_names_b: Vec<_> = note1_tags_b.iter().map(|t| t.name.as_str()).collect();
        assert!(note1_tag_names_b.contains(&"work"));
        assert!(note1_tag_names_b.contains(&"important"));

        // ---------------------------------------------------------------------
        // Test C — Remove one tag: Remove work
        // Expected: [important]
        // ---------------------------------------------------------------------
        TagRepository::remove_tag_from_note(&conn, &note1.id, &tag_work.id).expect("Test C: remove work");

        let note1_tags_c = TagRepository::get_tags_for_note(&conn, &note1.id).expect("Test C: get note tags");
        assert_eq!(note1_tags_c.len(), 1);
        assert_eq!(note1_tags_c[0].name, "important");

        // ---------------------------------------------------------------------
        // Test D — Favorite: Mark note as favorite
        // Expected: Favorites contains note
        // ---------------------------------------------------------------------
        let note1_fav = NoteRepository::set_favorite(&conn, &note1.id, true).expect("Test D: mark favorite");
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
        let note1_tags_e = TagRepository::get_tags_for_note(&reopened_conn, &note1.id).expect("Test E: get note tags");
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
        ).expect("Test F: create QA Note 2");
        TagRepository::add_tag_to_note(&reopened_conn, &note2.id, &tag_work.id).expect("Test F: assign work to note2");

        let renamed_tag = TagRepository::update(&reopened_conn, &tag_work.id, UpdateTagDto { name: "projects".to_string() }).expect("Test F: rename work to projects");
        assert_eq!(renamed_tag.name, "projects");

        // Verify affected note now shows "projects"
        let note2_tags_f = TagRepository::get_tags_for_note(&reopened_conn, &note2.id).expect("Test F: get note2 tags");
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
        let note2_tags_g = TagRepository::get_tags_for_note(&reopened_conn, &note2.id).expect("Test G: get note2 tags");
        assert_eq!(note2_tags_g.len(), 0);

        // ---------------------------------------------------------------------
        // Test H — Tag filter: Select #important
        // Expected: Only notes with important tag appear
        // ---------------------------------------------------------------------
        let important_notes_h = NoteRepository::list_by_tag(&reopened_conn, &tag_important.id).expect("Test H: list by important");
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
        ).expect("Test I: create Work root notebook");

        let nb_projects = NotebookRepository::create(
            &reopened_conn,
            CreateNotebookDto {
                name: "Projects".to_string(),
                parent_id: Some(nb_work.id.clone()),
            },
        ).expect("Test I: create Projects child notebook");

        // Recreate tag 'work' since it was deleted
        let tag_work_recreated = TagRepository::create(&reopened_conn, "work").expect("Test I: recreate work tag");

        let note_plan = NoteRepository::create(
            &reopened_conn,
            CreateNoteDto {
                title: Some("Project Plan".to_string()),
                content: Some("# Project Plan\n\nQ4 Deliverables and schedule.".to_string()),
                notebook_id: Some(nb_projects.id.clone()),
                format: Some("md".to_string()),
                ..Default::default()
            },
        ).expect("Test I: create Project Plan note");

        // Assign tags work and important
        TagRepository::add_tag_to_note(&reopened_conn, &note_plan.id, &tag_work_recreated.id).expect("Test I: assign work");
        TagRepository::add_tag_to_note(&reopened_conn, &note_plan.id, &tag_important.id).expect("Test I: assign important");

        // Favorite: Yes
        let note_plan_fav = NoteRepository::set_favorite(&reopened_conn, &note_plan.id, true).expect("Test I: set favorite");
        assert!(note_plan_fav.is_favorite);

        // Verify hierarchy path
        let hierarchy_path = NotebookRepository::get_hierarchy_path(&reopened_conn, &nb_projects.id).expect("Test I: get path");
        assert_eq!(hierarchy_path, "Work / Projects");

        // Verify tags
        let plan_tags = TagRepository::get_tags_for_note(&reopened_conn, &note_plan.id).expect("Test I: get tags");
        assert_eq!(plan_tags.len(), 2);
        let plan_tag_names: Vec<_> = plan_tags.iter().map(|t| t.name.as_str()).collect();
        assert!(plan_tag_names.contains(&"work"));
        assert!(plan_tag_names.contains(&"important"));

        // Verify note retrieved from DB has all consistent metadata
        let plan_db = NoteRepository::get_by_id(&reopened_conn, &note_plan.id).expect("Test I: get note").expect("found note");
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
            ).expect("Step 1: create notebook");

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
            ).expect("Step 2: create note");

            note_id = note.id.clone();
            initial_created_at = note.created_at.clone();

            // 3. Assign 3 tags
            let t_backend = TagRepository::create(&conn, "backend").expect("Step 3: create backend tag");
            let t_critical = TagRepository::create(&conn, "critical").expect("Step 3: create critical tag");
            let t_q4 = TagRepository::create(&conn, "q4-2026").expect("Step 3: create q4 tag");

            tag_backend_id = t_backend.id.clone();
            tag_critical_id = t_critical.id.clone();
            tag_q4_id = t_q4.id.clone();

            TagRepository::add_tag_to_note(&conn, &note_id, &tag_backend_id).expect("Step 3: assign backend");
            TagRepository::add_tag_to_note(&conn, &note_id, &tag_critical_id).expect("Step 3: assign critical");
            TagRepository::add_tag_to_note(&conn, &note_id, &tag_q4_id).expect("Step 3: assign q4");

            let assigned = TagRepository::get_tags_for_note(&conn, &note_id).expect("Step 3: check tags");
            assert_eq!(assigned.len(), 3);

            // 4. Mark favorite
            let fav_res = NoteRepository::set_favorite(&conn, &note_id, true).expect("Step 4: mark favorite");
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
            ).expect("Step 5: edit note");

            // 6. Move note (to notebook "Product")
            let product_nb = NotebookRepository::create(
                &conn,
                CreateNotebookDto {
                    name: "Product".to_string(),
                    parent_id: None,
                },
            ).expect("Step 6: create product notebook");
            product_nb_id = product_nb.id.clone();

            NoteRepository::update(
                &conn,
                &note_id,
                UpdateNoteDto {
                    notebook_id: Some(product_nb_id.clone()),
                    ..Default::default()
                },
            ).expect("Step 6: move note");

            // 7. Rename tag: backend -> core-infrastructure
            let renamed = TagRepository::update(
                &conn,
                &tag_backend_id,
                UpdateTagDto {
                    name: "core-infrastructure".to_string(),
                },
            ).expect("Step 7: rename tag");
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
        let tags_after_restart = TagRepository::get_tags_for_note(&reopened_conn, &note_id)
            .expect("get tags after restart");
        assert_eq!(tags_after_restart.len(), 3);
        let mut tag_names: Vec<String> = tags_after_restart.into_iter().map(|t| t.name).collect();
        tag_names.sort();
        assert_eq!(tag_names, vec!["core-infrastructure", "critical", "q4-2026"]);

        // Created time unchanged
        assert_eq!(note_after_restart.created_at, initial_created_at);

        // Modified time correct (valid non-empty timestamp)
        assert!(!note_after_restart.modified_at.is_empty());
        assert_ne!(note_after_restart.modified_at, "");
    }




























