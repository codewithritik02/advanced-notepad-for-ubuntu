mod common;

use common::{lock_process_global_test_state, TestAppDir};
use personal_notepad_lib::storage::crypto::{
    SetupEncryptionDto, UnlockDto, ENCRYPTION_METADATA_KEY,
};
use personal_notepad_lib::storage::lifecycle::{
    checkpoint_database_for_shutdown, initialize_application_storage, ApplicationLifecycleError,
    InitializedApplicationStorage,
};
use personal_notepad_lib::storage::models::{
    CreateNoteDto, RestoreActivationMarker, RestoreActivationPhase, UpdateNoteDto,
};
use personal_notepad_lib::storage::repositories::{NoteRepository, SettingsRepository};
use personal_notepad_lib::storage::services::RestoreService;
use personal_notepad_lib::storage::{
    init_connection, run_migrations_custom, StoragePaths, MIGRATIONS,
};
use std::fs;

fn shutdown(runtime: InitializedApplicationStorage) {
    checkpoint_database_for_shutdown(&runtime.database).expect("checkpoint database on shutdown");
    drop(runtime);
}

#[test]
fn first_launch_creates_and_migrates_a_new_database() {
    let _global_state = lock_process_global_test_state();
    let environment = TestAppDir::with_label("first-launch");
    assert!(!environment.paths.database_file.exists());

    let runtime = initialize_application_storage(environment.paths.clone())
        .expect("first launch should initialize storage");
    let connection = runtime.database.conn.lock().unwrap();
    let migration_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
        .unwrap();
    assert_eq!(migration_count, MIGRATIONS.len() as i64);
    assert!(!runtime.crypto_session.is_encryption_enabled());
    drop(connection);

    shutdown(runtime);
    assert!(!environment.paths.app_instance_lock_file().exists());
}

#[test]
fn existing_database_edits_survive_clean_shutdown_and_relaunch() {
    let _global_state = lock_process_global_test_state();
    let environment = TestAppDir::with_label("clean-relaunch");
    let runtime = initialize_application_storage(environment.paths.clone()).unwrap();

    let note = {
        let connection = runtime.database.conn.lock().unwrap();
        let note = NoteRepository::create(
            &connection,
            CreateNoteDto {
                title: Some("Before edit".to_string()),
                content: Some("Initial persisted content".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        NoteRepository::update(
            &connection,
            &note.id,
            UpdateNoteDto {
                title: Some("After edit".to_string()),
                content: Some("Latest successfully saved content ✅".to_string()),
                ..Default::default()
            },
        )
        .unwrap()
    };
    shutdown(runtime);

    let relaunched = initialize_application_storage(environment.paths.clone()).unwrap();
    let connection = relaunched.database.conn.lock().unwrap();
    let persisted = NoteRepository::get_by_id(&connection, &note.id)
        .unwrap()
        .expect("edited note should exist after relaunch");
    assert_eq!(persisted.title, "After edit");
    assert_eq!(persisted.content, "Latest successfully saved content ✅");
    drop(connection);
    shutdown(relaunched);
}

#[test]
fn startup_upgrades_an_older_supported_schema_without_losing_notes() {
    let _global_state = lock_process_global_test_state();
    let environment = TestAppDir::with_label("older-schema");
    let mut connection = init_connection(&environment.paths.database_file).unwrap();
    run_migrations_custom(&mut connection, &MIGRATIONS[..1]).unwrap();
    let note = NoteRepository::create(
        &connection,
        CreateNoteDto {
            title: Some("Version one note".to_string()),
            content: Some("Must survive migration".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    drop(connection);

    let runtime = initialize_application_storage(environment.paths.clone()).unwrap();
    let connection = runtime.database.conn.lock().unwrap();
    let versions: Vec<i32> = connection
        .prepare("SELECT version FROM _migrations ORDER BY version")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        versions,
        MIGRATIONS
            .iter()
            .map(|migration| migration.version)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        NoteRepository::get_by_id(&connection, &note.id)
            .unwrap()
            .unwrap()
            .content,
        "Must survive migration"
    );
    drop(connection);
    shutdown(runtime);
}

#[test]
fn startup_creates_a_missing_data_directory_and_reports_an_unusable_path() {
    let _global_state = lock_process_global_test_state();
    let environment = TestAppDir::with_label("data-directory");
    let missing_paths = StoragePaths::from_root(environment.dir.join("missing-app-data"));
    assert!(!missing_paths.root.exists());
    let runtime = initialize_application_storage(missing_paths.clone()).unwrap();
    assert!(missing_paths.database_file.is_file());
    shutdown(runtime);

    let blocking_file = environment.dir.join("not-a-directory");
    fs::write(&blocking_file, b"this path is intentionally a file").unwrap();
    let error = initialize_application_storage(StoragePaths::from_root(blocking_file))
        .err()
        .expect("a file cannot be used as the application data directory");
    assert!(matches!(
        error,
        ApplicationLifecycleError::PrepareDirectories(_)
    ));
    assert!(error.to_string().contains("could not be prepared"));
}

#[test]
fn database_initialization_failure_preserves_the_existing_file_exactly() {
    let _global_state = lock_process_global_test_state();
    let environment = TestAppDir::with_label("invalid-database");
    let original = b"not a sqlite database; preserve these recovery bytes";
    fs::write(&environment.paths.database_file, original).unwrap();

    let error = initialize_application_storage(environment.paths.clone())
        .err()
        .expect("invalid database must fail startup");
    assert!(matches!(error, ApplicationLifecycleError::OpenDatabase(_)));
    assert!(error.to_string().contains("could not be opened safely"));
    assert_eq!(
        fs::read(&environment.paths.database_file).unwrap(),
        original
    );
    assert!(!environment.paths.app_instance_lock_file().exists());
}

#[test]
fn invalid_encryption_configuration_is_reported_without_replacing_user_data() {
    let _global_state = lock_process_global_test_state();
    let environment = TestAppDir::with_label("invalid-config");
    let mut connection = environment.open_migrated_database();
    let note = NoteRepository::create(
        &connection,
        CreateNoteDto {
            title: Some("Keep me".to_string()),
            content: Some("Configuration errors must not erase notes".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    SettingsRepository::set(&connection, ENCRYPTION_METADATA_KEY, "{unsupported-json").unwrap();
    drop(connection);

    let error = initialize_application_storage(environment.paths.clone())
        .err()
        .expect("malformed encryption metadata must fail startup");
    assert!(matches!(
        error,
        ApplicationLifecycleError::InitializeEncryption(_)
    ));
    assert!(error
        .to_string()
        .contains("encryption configuration is invalid"));
    assert!(!environment.paths.app_instance_lock_file().exists());

    connection = init_connection(&environment.paths.database_file).unwrap();
    SettingsRepository::delete(&connection, ENCRYPTION_METADATA_KEY).unwrap();
    assert_eq!(
        NoteRepository::get_by_id(&connection, &note.id)
            .unwrap()
            .unwrap()
            .content,
        "Configuration errors must not erase notes"
    );
}

#[test]
fn startup_recovers_an_interrupted_restore_entry_point_before_opening_the_database() {
    let _global_state = lock_process_global_test_state();
    let environment = TestAppDir::with_label("interrupted-restore");
    let running = initialize_application_storage(environment.paths.clone()).unwrap();
    let note = {
        let connection = running.database.conn.lock().unwrap();
        NoteRepository::create(
            &connection,
            CreateNoteDto {
                title: Some("Live note".to_string()),
                content: Some("Still live after interrupted operation".to_string()),
                ..Default::default()
            },
        )
        .unwrap()
    };

    let staging_dir = environment.paths.root.join(".restore_staging_task4");
    fs::create_dir_all(&staging_dir).unwrap();
    fs::write(staging_dir.join("partial.sqlite"), b"partial operation").unwrap();
    RestoreService::write_activation_marker(
        &RestoreActivationMarker {
            operation_id: "task4-interrupted".to_string(),
            started_at: "2026-10-16T00:00:00Z".to_string(),
            candidate_archive_path: environment
                .dir
                .join("candidate.pnbackup")
                .to_string_lossy()
                .into(),
            staging_dir: staging_dir.to_string_lossy().into(),
            checkpoint_path: String::new(),
            live_db_backup_path: String::new(),
            live_attachments_backup_path: String::new(),
            phase: RestoreActivationPhase::Initiated,
        },
        &environment.paths,
    )
    .unwrap();

    // Simulate a supported native close after an operation has durably marked itself in progress.
    // The next launch must treat the marker as interrupted work and recover before opening SQLite.
    shutdown(running);
    assert!(environment.paths.restore_marker_file().exists());

    let runtime = initialize_application_storage(environment.paths.clone()).unwrap();
    assert!(!environment.paths.restore_marker_file().exists());
    assert!(!staging_dir.exists());
    let connection = runtime.database.conn.lock().unwrap();
    assert_eq!(
        NoteRepository::get_by_id(&connection, &note.id)
            .unwrap()
            .unwrap()
            .content,
        "Still live after interrupted operation"
    );
    drop(connection);
    shutdown(runtime);
}

#[test]
fn encrypted_session_restarts_locked_and_supports_lock_unlock_transitions() {
    let _global_state = lock_process_global_test_state();
    let environment = TestAppDir::with_label("encrypted-relaunch");
    let runtime = initialize_application_storage(environment.paths.clone()).unwrap();
    let password = "Task4-Strong-Password!";
    {
        let connection = runtime.database.conn.lock().unwrap();
        runtime
            .crypto_session
            .setup_encryption(
                &connection,
                &environment.paths,
                SetupEncryptionDto {
                    password: password.to_string(),
                    confirm_password: password.to_string(),
                },
            )
            .unwrap();
    }
    assert!(!runtime.crypto_session.is_locked());
    runtime.crypto_session.lock();
    assert!(runtime.crypto_session.is_locked());
    runtime
        .crypto_session
        .unlock(UnlockDto {
            password: password.to_string(),
        })
        .unwrap();
    assert!(!runtime.crypto_session.is_locked());
    shutdown(runtime);

    let relaunched = initialize_application_storage(environment.paths.clone()).unwrap();
    assert!(relaunched.crypto_session.is_encryption_enabled());
    assert!(relaunched.crypto_session.is_locked());
    relaunched
        .crypto_session
        .unlock(UnlockDto {
            password: password.to_string(),
        })
        .unwrap();
    assert!(!relaunched.crypto_session.is_locked());
    shutdown(relaunched);
}

#[test]
fn startup_fails_safely_when_app_data_parent_is_unwritable() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _global_state = lock_process_global_test_state();
        let environment = TestAppDir::with_label("unwritable-parent");
        let ro_parent = environment.dir.join("ro-parent");
        fs::create_dir_all(&ro_parent).unwrap();

        // Make parent read-only (555 = r-xr-xr-x)
        fs::set_permissions(&ro_parent, fs::Permissions::from_mode(0o555)).unwrap();

        let forbidden_paths = StoragePaths::from_root(ro_parent.join("forbidden-child"));
        let error = initialize_application_storage(forbidden_paths)
            .err()
            .expect("startup must fail when target directory cannot be created");

        assert!(matches!(
            error,
            ApplicationLifecycleError::PrepareDirectories(_)
        ));

        // Verify parent permissions were NOT modified or bypassed
        let meta = fs::metadata(&ro_parent).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o555);

        // Restore permissions for clean test teardown
        fs::set_permissions(&ro_parent, fs::Permissions::from_mode(0o755)).unwrap();
    }
}
