#![allow(dead_code)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use personal_notepad_lib::storage::{
    init_connection,
    models::{Attachment, CreateNoteDto, CreateNotebookDto, Note, Notebook, Tag},
    paths::{
        validate_isolated_test_root, TEST_ROOT_MARKER_CONTENT, TEST_ROOT_MARKER_FILE,
        TEST_ROOT_PREFIX,
    },
    repositories::{NoteRepository, NotebookRepository, TagRepository},
    run_migrations, AttachmentService, AttachmentStorage, StoragePaths,
};
use rusqlite::Connection;
use uuid::Uuid;

static PROCESS_GLOBAL_STATE_LOCK: Mutex<()> = Mutex::new(());

/// Serializes only tests that exercise production process-global coordinators such as the
/// security auditor sink or backup/restore concurrency flags. Filesystem/database fixtures remain
/// parallel; callers must hold this guard for the complete global-state test.
pub fn lock_process_global_test_state() -> MutexGuard<'static, ()> {
    PROCESS_GLOBAL_STATE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Canonical isolated application-data fixture for Rust integration and desktop tests.
///
/// Every fixture is a unique direct child of the system temporary directory and carries a
/// marker that must be validated before cleanup. It never resolves the production app-data path.
pub struct TestAppDir {
    pub dir: PathBuf,
    pub paths: StoragePaths,
    cleaned: bool,
}

impl TestAppDir {
    pub fn new() -> Self {
        Self::with_label("fixture")
    }

    pub fn with_label(label: &str) -> Self {
        let safe_label: String = label
            .chars()
            .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
            .take(32)
            .collect();
        let safe_label = if safe_label.is_empty() {
            "fixture"
        } else {
            safe_label.as_str()
        };
        let dir = std::env::temp_dir().join(format!(
            "{TEST_ROOT_PREFIX}{safe_label}-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));

        fs::create_dir(&dir).expect("create isolated test root");
        fs::write(dir.join(TEST_ROOT_MARKER_FILE), TEST_ROOT_MARKER_CONTENT)
            .expect("write isolated test-root marker");
        let dir = validate_isolated_test_root(&dir).expect("validate isolated test root");
        let paths = StoragePaths::from_root(dir.clone());
        paths
            .ensure_directories()
            .expect("create isolated application-data directories");

        Self {
            dir,
            paths,
            cleaned: false,
        }
    }

    /// Opens a real SQLite database and initializes it through the production migration runner.
    pub fn open_migrated_database(&self) -> Connection {
        let mut connection =
            init_connection(&self.paths.database_file).expect("initialize isolated test database");
        run_migrations(&mut connection).expect("run real migrations for isolated test database");
        connection
    }

    /// Environment value for a debug Tauri child process. The application independently validates
    /// the marker and location before accepting this path.
    pub fn desktop_data_dir(&self) -> &Path {
        &self.dir
    }

    /// Explicit cleanup is available to tests that need to assert teardown behavior.
    pub fn cleanup(mut self) -> io::Result<()> {
        self.cleanup_inner()
    }

    fn cleanup_inner(&mut self) -> io::Result<()> {
        if self.cleaned || !self.dir.exists() {
            self.cleaned = true;
            return Ok(());
        }

        validate_isolated_test_root(&self.dir)
            .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error.to_string()))?;
        fs::remove_dir_all(&self.dir)?;
        self.cleaned = true;
        Ok(())
    }
}

impl Drop for TestAppDir {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup_inner() {
            eprintln!(
                "TEST CLEANUP FAILURE: isolated root '{}' was not removed: {error}",
                self.dir.display()
            );
            if !std::thread::panicking() {
                panic!(
                    "failed to clean isolated test root '{}': {error}",
                    self.dir.display()
                );
            }
        }
    }
}

/// Real migrated database plus deterministic entity helpers for integration tests.
pub struct TestDatabase {
    environment: TestAppDir,
    connection: Option<Connection>,
}

impl TestDatabase {
    pub fn new() -> Self {
        let environment = TestAppDir::with_label("database");
        let connection = environment.open_migrated_database();
        Self {
            environment,
            connection: Some(connection),
        }
    }

    pub fn connection(&self) -> &Connection {
        self.connection
            .as_ref()
            .expect("test database connection must be open")
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        self.connection
            .as_mut()
            .expect("test database connection must be open")
    }

    pub fn paths(&self) -> &StoragePaths {
        &self.environment.paths
    }

    pub fn root(&self) -> &Path {
        &self.environment.dir
    }

    pub fn close_and_reopen(&mut self) {
        self.connection.take();
        self.connection = Some(self.environment.open_migrated_database());
    }

    pub fn create_notebook(&self, name: &str, parent_id: Option<&str>) -> Notebook {
        NotebookRepository::create(
            self.connection(),
            CreateNotebookDto {
                name: name.to_string(),
                parent_id: parent_id.map(str::to_string),
            },
        )
        .expect("create notebook fixture")
    }

    pub fn create_note(&self, title: &str, content: &str, notebook_id: Option<&str>) -> Note {
        NoteRepository::create(
            self.connection(),
            CreateNoteDto {
                title: Some(title.to_string()),
                content: Some(content.to_string()),
                notebook_id: notebook_id.map(str::to_string),
                ..Default::default()
            },
        )
        .expect("create note fixture")
    }

    pub fn create_tag(&self, name: &str) -> Tag {
        TagRepository::create(self.connection(), name).expect("create tag fixture")
    }

    pub fn assign_tag(&self, note_id: &str, tag_id: &str) {
        TagRepository::add_tag_to_note(self.connection(), note_id, tag_id)
            .expect("assign tag fixture");
    }

    pub fn attach_bytes(&self, note_id: &str, file_name: &str, bytes: &[u8]) -> Attachment {
        let source_dir = self.environment.dir.join("fixture-sources");
        fs::create_dir_all(&source_dir).expect("create attachment source fixture directory");
        let safe_name: String = file_name
            .chars()
            .filter(|character| {
                character.is_alphanumeric() || matches!(*character, '.' | '-' | '_' | ' ')
            })
            .collect();
        assert!(
            !safe_name.is_empty(),
            "attachment fixture name must be usable"
        );
        let source_path = source_dir.join(safe_name);
        fs::write(&source_path, bytes).expect("write attachment source fixture");

        let storage = AttachmentStorage::new(self.environment.paths.clone());
        AttachmentService::add_attachment(self.connection(), &storage, note_id, &source_path)
            .expect("add attachment fixture")
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        // Close SQLite before TestAppDir attempts directory removal. This matters on Windows and
        // makes the fixture portable even though Phase 14 currently targets Ubuntu.
        self.connection.take();
    }
}
