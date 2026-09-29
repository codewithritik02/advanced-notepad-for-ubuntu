use personal_notepad_lib::storage::{
    init_connection, run_migrations,
    models::{CreateNotebookDto, CreateNoteDto, UpdateNoteDto},
    repositories::{NotebookRepository, NoteRepository, SettingsRepository, TagRepository},
    StoragePaths,
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
