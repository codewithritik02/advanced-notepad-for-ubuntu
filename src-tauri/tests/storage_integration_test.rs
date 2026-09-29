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
