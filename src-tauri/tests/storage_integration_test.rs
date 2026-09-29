use personal_notepad_lib::storage::{
    init_connection, run_migrations,
    models::{CreateNotebookDto, UpdateNotebookDto, CreateNoteDto, UpdateNoteDto},
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

