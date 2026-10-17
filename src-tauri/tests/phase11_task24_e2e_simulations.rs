use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use personal_notepad_lib::storage::{
    init_connection,
    models::{
        BackupCompatibilityStatus, BackupCreateRequest, BackupHealthLevel, BackupProgressEvent,
        BackupRestoreRequest, BackupType, CreateNoteDto, CreateNotebookDto,
        RestoreActivationMarker, RestoreActivationPhase,
    },
    paths::StoragePaths,
    repositories::{NoteRepository, NotebookRepository, TagRepository},
    run_migrations,
    services::{
        BackupHealthService, BackupRetentionManager, BackupSchedulerService, BackupService,
        BackupSettingsService, BackupValidationService, RestoreService,
    },
};

/// Helper RAII test environment providing an isolated temporary directory tree.
struct TestEnv {
    dir: PathBuf,
    paths: StoragePaths,
}

impl TestEnv {
    fn new(prefix: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("{}_{}", prefix, Uuid::new_v4()));
        let paths = StoragePaths::from_root(dir.clone());
        paths
            .ensure_directories()
            .expect("Failed to create test directories");
        Self { dir, paths }
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

// =============================================================================
// Scenario A — Normal Recovery
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
// =============================================================================
#[test]
fn test_scenario_a_normal_recovery_lifecycle() {
    let env = TestEnv::new("pn_e2e_scen_a");
    let mut conn = init_connection(&env.paths.database_file).expect("Conn init failed");
    run_migrations(&mut conn).expect("Migrations failed");

    // 1-3. Notes, Notebooks, Tags, Favorites
    let nb_physics = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Theoretical Astrophysics".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let note_active_1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Gravitational Wave Interferometry".to_string()),
            content: Some("Chirp signals and black hole merger dynamics".to_string()),
            notebook_id: Some(nb_physics.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .unwrap();

    let _note_active_2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Hawking Radiation Formalism".to_string()),
            content: Some("Bogoliubov transformation in curved spacetime".to_string()),
            notebook_id: Some(nb_physics.id.clone()),
            is_favorite: Some(false),
            ..Default::default()
        },
    )
    .unwrap();

    let tag_grav = TagRepository::create(&conn, "relativity").unwrap();
    TagRepository::add_tag_to_note(&conn, &note_active_1.id, &tag_grav.id).unwrap();

    // 4. Attach a file
    let att_dir = env
        .paths
        .get_note_attachments_dir(&note_active_1.id)
        .unwrap();
    let att_filename = format!("{}.pdf", Uuid::new_v4());
    let att_bytes = b"%PDF-1.4 Chirp Strain Data Amplitude FFT";
    fs::write(att_dir.join(&att_filename), att_bytes).unwrap();

    let now_str = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            note_active_1.id,
            "waveform.pdf",
            format!("{}/{}", note_active_1.id, att_filename),
            "application/pdf",
            att_bytes.len() as i64,
            now_str,
            now_str,
        ],
    ).unwrap();

    // 5. Move a note to Trash
    let note_trashed = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Superseded Numerical Simulation".to_string()),
            content: Some("Old draft moved to trash".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    NoteRepository::soft_delete(&conn, &note_trashed.id).unwrap();

    // Flush WAL writes to SQLite disk file
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();

    // 6. Create and validate a backup
    let backup_req = BackupCreateRequest {
        destination_path: None,
        backup_type: Some(BackupType::Manual),
        allow_overwrite: Some(false),
    };
    let backup_res = BackupService::create_backup(
        &conn,
        &env.paths,
        backup_req,
        None::<fn(BackupProgressEvent)>,
    )
    .expect("Backup creation failed");

    assert!(backup_res.verified);
    assert_eq!(backup_res.note_count, 3); // 2 active + 1 trash
    assert_eq!(backup_res.attachment_count, 1);

    let archive_path = Path::new(&backup_res.file_path);
    assert!(archive_path.exists());

    let val = BackupValidationService::validate_backup(archive_path, &env.paths).unwrap();
    assert!(val.is_valid);
    assert_eq!(val.compatibility, BackupCompatibilityStatus::Compatible);

    // 7. Make additional changes in live app
    let _note_intervening = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Ephemeral Work Post-Backup".to_string()),
            content: Some("Intervening draft that should be replaced by restore".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    conn.execute(
        "UPDATE notes SET content = 'Corrupted intervening edit' WHERE id = ?1",
        rusqlite::params![note_active_1.id],
    )
    .unwrap();

    // Delete attachment from live database and disk
    conn.execute(
        "DELETE FROM attachments WHERE note_id = ?1",
        rusqlite::params![note_active_1.id],
    )
    .unwrap();
    let _ = fs::remove_file(att_dir.join(&att_filename));

    // Confirm state before restore
    let total_notes_before: i64 = conn
        .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total_notes_before, 4);

    // 8. Restore the backup in controlled environment
    let restore_req = BackupRestoreRequest {
        file_path: backup_res.file_path.clone(),
        operation_id: Some(format!("e2e-restore-{}", Uuid::new_v4())),
    };
    let restore_res = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        restore_req,
        None::<fn(BackupProgressEvent)>,
    )
    .expect("Restore execution failed");

    assert!(restore_res.success);
    assert_eq!(restore_res.note_count, 2);
    assert_eq!(restore_res.trash_count, 1);
    assert_eq!(restore_res.attachment_count, 1);

    // 9. Verify restored state matches the backup exactly
    let note1_restored = NoteRepository::get_by_id(&conn, &note_active_1.id)
        .unwrap()
        .unwrap();
    assert_eq!(note1_restored.title, "Gravitational Wave Interferometry");
    assert_eq!(
        note1_restored.content,
        "Chirp signals and black hole merger dynamics"
    );
    assert!(note1_restored.is_favorite);
    assert!(note1_restored.deleted_at.is_none());

    let trashed_restored = NoteRepository::get_by_id(&conn, &note_trashed.id)
        .unwrap()
        .unwrap();
    assert_eq!(trashed_restored.title, "Superseded Numerical Simulation");
    assert!(
        trashed_restored.deleted_at.is_some(),
        "Trash status must be preserved"
    );

    // Intervening draft must be removed
    let post_backup_check: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM notes WHERE title = 'Ephemeral Work Post-Backup'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(post_backup_check, 0);

    // Restored attachment bytes on disk
    let restored_att_file = att_dir.join(&att_filename);
    assert!(restored_att_file.exists());
    let restored_bytes = fs::read(&restored_att_file).unwrap();
    assert_eq!(restored_bytes, att_bytes);

    // Pre-restore checkpoint preserved
    let checkpoint_path = Path::new(&restore_res.checkpoint_path);
    assert!(checkpoint_path.exists());
    let chk_val =
        RestoreService::validate_recovery_checkpoint(checkpoint_path, &env.paths).unwrap();
    assert!(chk_val.is_valid);
    let chk_manifest = chk_val.manifest.unwrap();
    assert_eq!(chk_manifest.database.note_count, 3);
    assert_eq!(chk_manifest.database.trash_count, 1);

    // 10. Restart application (cold connection reopening)
    drop(conn);
    let conn_restarted = init_connection(&env.paths.database_file).expect("Reopen failed");
    let total_restarted: i64 = conn_restarted
        .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total_restarted, 3);
}

// =============================================================================
// Scenario B — Corrupt Backup
// 1. Create a valid test backup.
// 2. Corrupt one required archive entry or checksum.
// 3. Attempt validation.
// 4. Confirm restoration is blocked.
// 5. Confirm current application data remains unchanged.
// =============================================================================
#[test]
fn test_scenario_b_corrupt_backup_detection_and_defense() {
    let env = TestEnv::new("pn_e2e_scen_b");
    let mut conn = init_connection(&env.paths.database_file).expect("Conn init failed");
    run_migrations(&mut conn).expect("Migrations failed");

    let live_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Critical Inviolable Note".to_string()),
            content: Some("Live data that must be defended against corrupt archives".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 1-2. Corrupted archive file
    let corrupt_archive = env.paths.backups_dir.join("corrupted_payload.pnbackup");
    fs::write(&corrupt_archive, b"PK\x03\x04corrupted zip payload").unwrap();

    // 3. Attempt validation
    let val = BackupValidationService::validate_backup(&corrupt_archive, &env.paths).unwrap();

    // 4. Confirm restoration is blocked
    assert!(
        !val.is_valid,
        "Corrupt backup validation must report invalid"
    );
    assert_eq!(val.compatibility, BackupCompatibilityStatus::Corrupted);

    let restore_req = BackupRestoreRequest {
        file_path: corrupt_archive.to_string_lossy().to_string(),
        operation_id: Some("corrupt-restore-attempt".to_string()),
    };
    let restore_res = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        restore_req,
        None::<fn(BackupProgressEvent)>,
    );
    assert!(
        restore_res.is_err(),
        "Restoration must fail fast on corrupt archive"
    );

    // 5. Confirm current application data remains unchanged
    let note_after = NoteRepository::get_by_id(&conn, &live_note.id)
        .unwrap()
        .unwrap();
    assert_eq!(note_after.title, "Critical Inviolable Note");
    assert_eq!(
        note_after.content,
        "Live data that must be defended against corrupt archives"
    );
}

// =============================================================================
// Scenario C — Interrupted Restore
// 1. Create a valid backup and recovery checkpoint.
// 2. Simulate a controlled failure during staging or activation.
// 3. Run the documented recovery procedure.
// 4. Confirm the database and attachment state are consistent.
// 5. Confirm the application does not silently open a mixed or incomplete state.
// =============================================================================
#[test]
fn test_scenario_c_interrupted_restore_and_rollback_recovery() {
    let env = TestEnv::new("pn_e2e_scen_c");
    let mut conn = init_connection(&env.paths.database_file).expect("Conn init failed");
    run_migrations(&mut conn).expect("Migrations failed");

    let live_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Important Active Draft".to_string()),
            content: Some("Must survive crash recovery".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 2. Simulate failure during activation: seed active marker and pre-restore swap files
    let op_id = Uuid::new_v4().to_string();
    let sim_db_bak = env
        .paths
        .root
        .join(format!(".db_pre_restore_{op_id}.sqlite"));
    let sim_att_bak = env.paths.root.join(format!(".att_pre_restore_{op_id}"));

    fs::copy(&env.paths.database_file, &sim_db_bak).unwrap();
    fs::create_dir_all(&sim_att_bak).unwrap();

    let marker = RestoreActivationMarker {
        operation_id: op_id,
        started_at: chrono::Utc::now().to_rfc3339(),
        candidate_archive_path: "candidate.pnbackup".to_string(),
        staging_dir: env
            .paths
            .backups_dir
            .join(".staged_sim_c")
            .to_string_lossy()
            .to_string(),
        checkpoint_path: env
            .paths
            .checkpoints_dir()
            .join("checkpoint_sim_c.pnbackup")
            .to_string_lossy()
            .to_string(),
        live_db_backup_path: sim_db_bak.to_string_lossy().to_string(),
        live_attachments_backup_path: sim_att_bak.to_string_lossy().to_string(),
        phase: RestoreActivationPhase::FilesSwapped,
    };
    RestoreService::write_activation_marker(&marker, &env.paths).unwrap();
    assert!(env.paths.restore_marker_file().exists());

    // 3. Run documented crash recovery procedure
    let recovered = RestoreService::recover_if_interrupted(&env.paths).unwrap();
    assert!(
        recovered.is_some(),
        "Crash recovery must detect active marker"
    );

    // 4-5. Confirm consistency, clean marker clearance, and intact data
    assert!(
        !env.paths.restore_marker_file().exists(),
        "Marker must be cleared"
    );
    let verified_note = NoteRepository::get_by_id(&conn, &live_note.id)
        .unwrap()
        .unwrap();
    assert_eq!(verified_note.title, "Important Active Draft");
}

// =============================================================================
// Scenario D — Automatic Backup
// 1. Enable automatic backups.
// 2. Trigger the configured backup condition.
// 3. Confirm a backup is created.
// 4. Restart the application.
// 5. Verify scheduling state and retention behavior.
// 6. Simulate an inaccessible destination.
// 7. Confirm a visible failure status appears.
// =============================================================================
#[test]
fn test_scenario_d_automatic_backup_and_inaccessible_destination() {
    let env = TestEnv::new("pn_e2e_scen_d");
    let mut conn = init_connection(&env.paths.database_file).expect("Conn init failed");
    run_migrations(&mut conn).expect("Migrations failed");

    let _note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Automated Snapshot Target".to_string()),
            content: Some("Testing background scheduler".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 1. Enable automatic backups in settings
    let mut settings = BackupSettingsService::get_settings(&conn).unwrap();
    settings.auto_backup_enabled = true;
    settings.backup_on_startup = true;
    settings.retention_count = 2;
    BackupSettingsService::save_settings(&conn, &env.paths, settings).unwrap();

    // 2. Trigger configured backup condition (startup trigger)
    let auto_res = BackupSchedulerService::check_and_run_backup(
        &conn,
        &env.paths,
        true,
        None::<fn(BackupProgressEvent)>,
    )
    .expect("Automatic backup check failed");

    // 3. Confirm backup created
    assert!(auto_res.is_some());
    let auto_item = auto_res.unwrap();
    assert!(auto_item.verified);
    assert_eq!(auto_item.backup_type, BackupType::Automatic);

    let state = BackupSchedulerService::get_state(&conn).unwrap();
    assert!(state.last_successful_at.is_some());
    assert_eq!(state.failure_count, 0);

    // 4. Restart application
    drop(conn);
    let conn_restarted = init_connection(&env.paths.database_file).expect("Reopen failed");
    let state_restarted = BackupSchedulerService::get_state(&conn_restarted).unwrap();
    assert!(
        state_restarted.last_successful_at.is_some(),
        "Scheduler state survived restart"
    );

    // 5. Verify retention behavior: create 3 more automatic archives
    for _ in 1..=3 {
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let req = BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Automatic),
            allow_overwrite: Some(false),
        };
        let res = BackupService::create_backup(
            &conn_restarted,
            &env.paths,
            req,
            None::<fn(BackupProgressEvent)>,
        )
        .unwrap();
        assert!(res.verified);

        let newly_path = Path::new(&res.file_path);
        let ret_res =
            BackupRetentionManager::apply_retention(&env.paths.backups_dir, 2, Some(newly_path));
        assert_eq!(
            ret_res.kept_count, 2,
            "Retention must keep at most 2 archives"
        );
    }

    // 6. Simulate inaccessible destination
    let mut broken_settings = BackupSettingsService::get_settings(&conn_restarted).unwrap();
    broken_settings.backup_destination_dir = Some("/nonexistent/forbidden/path".to_string());
    let save_err =
        BackupSettingsService::save_settings(&conn_restarted, &env.paths, broken_settings);
    assert!(
        save_err.is_err(),
        "Saving an unwritable destination must be rejected"
    );

    let blocked_dir = env.paths.root.join("blocked_media");
    fs::create_dir_all(&blocked_dir).unwrap();
    let mut valid_settings = BackupSettingsService::get_settings(&conn_restarted).unwrap();
    valid_settings.backup_destination_dir = Some(blocked_dir.to_string_lossy().to_string());
    BackupSettingsService::save_settings(&conn_restarted, &env.paths, valid_settings).unwrap();

    fs::remove_dir_all(&blocked_dir).unwrap();
    fs::write(&blocked_dir, b"blocked file payload").unwrap();

    // 7. Confirm visible failure status appears in health check
    let health = BackupHealthService::get_health_status(&conn_restarted, &env.paths).unwrap();
    assert!(!health.destination_available);
    assert!(
        health.health_level == BackupHealthLevel::Warning
            || health.health_level == BackupHealthLevel::Critical,
    );
    assert!(health
        .warnings
        .iter()
        .any(|w| w.code == "DESTINATION_UNAVAILABLE"));
}
