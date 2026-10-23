mod common;

use std::fs;

use common::{TestAppDir, TestDatabase};
use personal_notepad_lib::storage::{
    paths::{
        validate_isolated_test_root, TEST_ROOT_MARKER_CONTENT, TEST_ROOT_MARKER_FILE,
        TEST_ROOT_PREFIX,
    },
    repositories::{NoteRepository, TagRepository},
};

#[test]
fn creates_complete_isolated_app_data_tree_with_real_migrations() {
    let fixture = TestAppDir::with_label("layout");

    assert!(fixture.dir.starts_with(std::env::temp_dir()));
    assert!(fixture
        .dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with(TEST_ROOT_PREFIX));
    assert_eq!(
        fs::read_to_string(fixture.dir.join(TEST_ROOT_MARKER_FILE)).unwrap(),
        TEST_ROOT_MARKER_CONTENT
    );
    assert!(fixture.paths.attachments_dir.is_dir());
    assert!(fixture.paths.backups_dir.is_dir());
    assert!(fixture.paths.config_dir.is_dir());
    assert!(fixture.paths.checkpoints_dir().is_dir());

    let connection = fixture.open_migrated_database();
    let migration_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        migration_count,
        personal_notepad_lib::storage::migrations::LATEST_SCHEMA_VERSION as i64
    );
    drop(connection);
}

#[test]
fn fixtures_are_unique_and_parallel_safe() {
    let roots: Vec<_> = (0..8)
        .map(|index| {
            std::thread::spawn(move || {
                let fixture = TestAppDir::with_label(&format!("parallel-{index}"));
                let connection = fixture.open_migrated_database();
                connection
                    .execute(
                        "INSERT INTO settings (key, value) VALUES ('owner', ?1)",
                        [index.to_string()],
                    )
                    .unwrap();
                drop(connection);
                fixture.dir.clone()
            })
        })
        .map(|thread| thread.join().unwrap())
        .collect();

    let unique: std::collections::HashSet<_> = roots.iter().collect();
    assert_eq!(unique.len(), roots.len());
    assert!(roots.iter().all(|root| !root.exists()));
}

#[test]
fn entity_helpers_persist_notes_notebooks_tags_and_attachments_across_reopen() {
    let mut database = TestDatabase::new();
    let notebook = database.create_notebook("QA Notebook 🧪", None);
    let note = database.create_note(
        "Persistent fixture note",
        "Unicode fixture content: नमस्ते 🚀",
        Some(&notebook.id),
    );
    let tag = database.create_tag("qa-fixture");
    database.assign_tag(&note.id, &tag.id);
    let attachment = database.attach_bytes(&note.id, "fixture résumé.txt", b"safe fixture bytes");

    assert!(database
        .paths()
        .attachments_dir
        .join(&attachment.relative_path)
        .is_file());

    database.close_and_reopen();
    let persisted = NoteRepository::get_by_id(database.connection(), &note.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.content, "Unicode fixture content: नमस्ते 🚀");
    assert_eq!(persisted.notebook_id, Some(notebook.id));
    let persisted_tags = TagRepository::get_tags_for_note(database.connection(), &note.id).unwrap();
    assert_eq!(persisted_tags, vec![tag]);
}

#[test]
fn cleanup_guard_rejects_markerless_or_out_of_scope_directories() {
    let markerless = std::env::temp_dir().join(format!(
        "{TEST_ROOT_PREFIX}markerless-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir(&markerless).unwrap();
    assert!(validate_isolated_test_root(&markerless).is_err());
    fs::remove_dir(&markerless).unwrap();

    let workspace_like = std::env::current_dir().unwrap();
    assert!(validate_isolated_test_root(&workspace_like).is_err());
}

#[test]
fn explicit_cleanup_removes_only_the_owned_fixture() {
    let fixture = TestAppDir::with_label("cleanup");
    let fixture_root = fixture.dir.clone();
    let sibling = std::env::temp_dir().join(format!("unrelated-sibling-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&sibling).unwrap();
    fs::write(sibling.join("keep.txt"), b"must survive").unwrap();

    fixture.cleanup().unwrap();

    assert!(!fixture_root.exists());
    assert_eq!(fs::read(sibling.join("keep.txt")).unwrap(), b"must survive");
    fs::remove_dir_all(sibling).unwrap();
}

#[test]
fn desktop_data_root_must_pass_the_same_isolation_guard() {
    let fixture = TestAppDir::with_label("desktop");
    let validated = validate_isolated_test_root(fixture.desktop_data_dir()).unwrap();
    assert_eq!(validated, fixture.dir);
}
