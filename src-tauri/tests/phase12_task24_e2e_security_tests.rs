use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;
use uuid::Uuid;
use zip::ZipArchive;

use personal_notepad_lib::storage::{
    attachment_storage::AttachmentStorage,
    crypto::{
        is_encrypted_string, ChangePasswordDto, CryptoSession, EncryptionTransitionMarker,
        MigrationService, MigrationStatus, SetupEncryptionDto, StartupRecoveryDecision, UnlockDto,
        ENCRYPTION_METADATA_KEY,
    },
    errors::{CryptoError, StorageError},
    init_connection,
    models::{
        BackupCreateRequest, BackupRestoreRequest, BackupType, CreateNoteDto, CreateNotebookDto,
    },
    paths::StoragePaths,
    repositories::{
        AttachmentRepository, NoteRepository, NotebookRepository, SearchRepository,
        SettingsRepository, TagRepository,
    },
    run_migrations,
    services::{AttachmentService, BackupService, BackupValidationService, RestoreService},
};

/// Isolated test environment using a dedicated temporary directory.
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
// Scenario A — Enable Encryption (End-to-End Workflow)
// 1. Create several notes.
// 2. Create notebooks, tags, and favorites.
// 3. Add an attachment.
// 4. Create a verified recovery checkpoint.
// 5. Enable encryption.
// 6. Restart the application (cold boot simulation).
// 7. Unlock it.
// 8. Verify notes, metadata, and attachments.
// 9. Inspect protected storage files for unintended plaintext exposure.
// =============================================================================
#[test]
fn test_scenario_a_enable_encryption_end_to_end() {
    let env = TestEnv::new("pn_task24_scen_a");
    let master_password = "MasterPasswordScenarioA1!";

    let note1_id: String;
    let note2_id: String;
    let nb_id: String;
    let tag_id: String;
    let att_id: String;
    let att_original_bytes = b"Top secret classified blueprint data with confidential schematic.";

    // Part 1: Seed unencrypted data
    {
        let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1 & 2. Notebooks, Tags, Notes, Favorites
        let nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Aerospace Engineering".to_string(),
                parent_id: None,
            },
        )
        .expect("Create notebook failed");
        nb_id = nb.id;

        let tag = TagRepository::create(&conn, "propulsion").expect("Create tag failed");
        tag_id = tag.id;

        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Scramjet Hypersonic Propulsion".to_string()),
                content: Some(
                    "Supersonic combustion ramjet dynamics and thermal management.".to_string(),
                ),
                format: None,
                notebook_id: Some(nb_id.clone()),
                is_favorite: Some(true),
                is_pinned: Some(true),
            },
        )
        .expect("Create note 1 failed");
        note1_id = note1.id;
        TagRepository::add_tag_to_note(&conn, &note1_id, &tag_id).expect("Add tag failed");

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Cryogenic Fuel Cell Specs".to_string()),
                content: Some(
                    "Liquid hydrogen density and boil-off mitigation parameters.".to_string(),
                ),
                format: None,
                notebook_id: Some(nb_id.clone()),
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .expect("Create note 2 failed");
        note2_id = note2.id;

        // 3. Add an attachment
        let storage = AttachmentStorage::new(env.paths.clone());
        let source_file = env.dir.join("schematic.bin");
        fs::write(&source_file, att_original_bytes).expect("Write attachment source failed");

        let att = AttachmentService::add_attachment(&conn, &storage, &note1_id, &source_file)
            .expect("Add attachment failed");
        att_id = att.id;

        // 4 & 5. Create recovery checkpoint & enable encryption
        let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
        assert!(!session.is_encryption_enabled());

        let report = session
            .setup_encryption(
                &conn,
                &env.paths,
                SetupEncryptionDto {
                    password: master_password.to_string(),
                    confirm_password: master_password.to_string(),
                },
            )
            .expect("Setup encryption failed");

        assert!(report.enabled);
        assert_eq!(report.migration_status, MigrationStatus::Active);

        // Verify pre-encryption checkpoint exists on disk
        let checkpoints =
            RestoreService::list_recovery_checkpoints(&env.paths).expect("List checkpoints failed");
        assert!(
            !checkpoints.is_empty(),
            "Pre-migration recovery checkpoint must exist"
        );
    }

    // 6. Cold boot restart: close connection and re-open
    {
        let conn_cold = init_connection(&env.paths.database_file).expect("Cold DB init failed");
        let session_cold =
            CryptoSession::init_from_database(&conn_cold).expect("Cold session init failed");

        // Verify cold boot is locked
        assert!(session_cold.is_encryption_enabled());
        assert!(session_cold.is_locked());
        assert!(session_cold.get_active_dek().is_err());

        // 7. Unlock it with correct master password
        session_cold
            .unlock(UnlockDto {
                password: master_password.to_string(),
            })
            .expect("Cold unlock failed");
        assert!(!session_cold.is_locked());

        let dek = session_cold.get_active_dek().expect("Get DEK failed");

        // 8. Verify notes, metadata, and attachments are intact and decrypted
        let n1 = NoteRepository::get_by_id_with_key(&conn_cold, &note1_id, Some(&dek))
            .expect("Get note 1 failed")
            .expect("Note 1 must exist");
        assert_eq!(n1.title, "Scramjet Hypersonic Propulsion");
        assert_eq!(
            n1.content,
            "Supersonic combustion ramjet dynamics and thermal management."
        );
        assert_eq!(n1.notebook_id.as_deref(), Some(nb_id.as_str()));
        assert!(n1.is_favorite);
        assert!(n1.is_pinned);

        let n2 = NoteRepository::get_by_id_with_key(&conn_cold, &note2_id, Some(&dek))
            .expect("Get note 2 failed")
            .expect("Note 2 must exist");
        assert_eq!(n2.title, "Cryogenic Fuel Cell Specs");
        assert_eq!(
            n2.content,
            "Liquid hydrogen density and boil-off mitigation parameters."
        );

        // Verify tags
        let tags_n1 =
            TagRepository::get_tags_for_note(&conn_cold, &note1_id).expect("Get tags failed");
        assert_eq!(tags_n1.len(), 1);
        assert_eq!(tags_n1[0].id, tag_id);

        // Verify attachment
        let storage = AttachmentStorage::new(env.paths.clone());
        let open_path = AttachmentService::get_attachment_for_open_with_key(
            &conn_cold,
            &storage,
            &att_id,
            Some(&dek),
        )
        .expect("Open attachment failed");
        let read_bytes = fs::read(&open_path).expect("Read decrypted attachment failed");
        assert_eq!(read_bytes, att_original_bytes);

        // 9. Inspect protected storage files: verify zero plaintext leakage on disk
        let (raw_title, raw_content): (String, String) = conn_cold
            .query_row(
                "SELECT title, content FROM notes WHERE id = ?1",
                [&note1_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();

        assert!(is_encrypted_string(&raw_title));
        assert!(is_encrypted_string(&raw_content));
        assert!(!raw_title.contains("Scramjet"));
        assert!(!raw_content.contains("combustion"));

        let att_disk_entry =
            AttachmentRepository::get_by_id_with_key(&conn_cold, &att_id, Some(&dek))
                .unwrap()
                .unwrap();
        let att_disk_path = storage
            .resolve_safe_path(&att_disk_entry.relative_path)
            .unwrap();
        let att_disk_bytes = fs::read(&att_disk_path).unwrap();
        assert_eq!(att_disk_bytes[0], 0x01);
        let disk_text = String::from_utf8_lossy(&att_disk_bytes);
        assert!(!disk_text.contains("classified blueprint data"));
    }
}

// =============================================================================
// Scenario B — Incorrect Password (Rejection and Non-Destructive Invariant)
// 1. Lock the application.
// 2. Enter an incorrect password.
// 3. Confirm protected content remains inaccessible.
// 4. Confirm stored data has not been modified.
// 5. Enter the correct password.
// 6. Verify normal access is restored.
// =============================================================================
#[test]
fn test_scenario_b_incorrect_password_and_non_destructive_invariants() {
    let env = TestEnv::new("pn_task24_scen_b");
    let correct_password = "CorrectMasterPasswordScenarioB1!";
    let wrong_password = "WrongMasterPasswordScenarioB2!";

    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migrations failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: correct_password.to_string(),
                confirm_password: correct_password.to_string(),
            },
        )
        .expect("Setup failed");

    let dek = session.get_active_dek().expect("DEK failed");
    let note = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Top Secret Operations".to_string()),
            content: Some("Orbital rendezvous protocols".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
        Some(&dek),
    )
    .expect("Create note failed");

    // Capture exact ciphertext and metadata before lock
    let raw_ciphertext_before: String = conn
        .query_row("SELECT content FROM notes WHERE id = ?1", [&note.id], |r| {
            r.get(0)
        })
        .unwrap();
    let meta_before: String = SettingsRepository::get(&conn, ENCRYPTION_METADATA_KEY)
        .unwrap()
        .unwrap();

    // 1. Lock the application
    session.lock();
    assert!(session.is_locked());
    assert!(session.get_active_dek().is_err());

    // 2. Enter an incorrect password
    let unlock_err = session
        .unlock(UnlockDto {
            password: wrong_password.to_string(),
        })
        .expect_err("Unlock with wrong password must fail");

    assert_eq!(unlock_err, CryptoError::AuthenticationFailed);
    assert_eq!(
        unlock_err.user_friendly_message(),
        "Incorrect password. Please try again."
    );

    // 3. Confirm protected content remains inaccessible
    assert!(session.is_locked());
    assert!(NoteRepository::get_by_id_with_key(&conn, &note.id, None).is_err());

    // 4. Confirm stored data and metadata have not been modified or corrupted
    let raw_ciphertext_after: String = conn
        .query_row("SELECT content FROM notes WHERE id = ?1", [&note.id], |r| {
            r.get(0)
        })
        .unwrap();
    let meta_after: String = SettingsRepository::get(&conn, ENCRYPTION_METADATA_KEY)
        .unwrap()
        .unwrap();

    assert_eq!(
        raw_ciphertext_before, raw_ciphertext_after,
        "Ciphertext must remain unaltered after failed attempt"
    );
    assert_eq!(
        meta_before, meta_after,
        "Encryption metadata must remain unaltered after failed attempt"
    );

    // 5. Enter the correct password
    session
        .unlock(UnlockDto {
            password: correct_password.to_string(),
        })
        .expect("Unlock with correct password must succeed");
    assert!(!session.is_locked());

    // 6. Verify normal access is restored
    let dek_restored = session.get_active_dek().expect("Active DEK failed");
    let note_decrypted = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&dek_restored))
        .unwrap()
        .unwrap();

    assert_eq!(note_decrypted.title, "Top Secret Operations");
    assert_eq!(note_decrypted.content, "Orbital rendezvous protocols");
}

// =============================================================================
// Scenario C — Password Change (End-to-End Credential Modification)
// 1. Change the password.
// 2. Restart the application.
// 3. Confirm the new password works.
// 4. Confirm the old password no longer works according to credential policy.
// 5. Verify note and attachment contents remain intact.
// =============================================================================
#[test]
fn test_scenario_c_password_change_end_to_end() {
    let env = TestEnv::new("pn_task24_scen_c");
    let old_password = "OldMasterPassword123!";
    let new_password = "NewMasterPassword456!";

    let note_id: String;
    let att_id: String;
    let att_payload = b"Payload that survives master password credential change";

    // Setup encryption, add note and attachment
    {
        let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
        session
            .setup_encryption(
                &conn,
                &env.paths,
                SetupEncryptionDto {
                    password: old_password.to_string(),
                    confirm_password: old_password.to_string(),
                },
            )
            .expect("Setup failed");

        let dek = session.get_active_dek().expect("DEK failed");

        let note = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Credential Modification Note".to_string()),
                content: Some(
                    "Secret content to be preserved across password updates.".to_string(),
                ),
                format: None,
                notebook_id: None,
                is_favorite: None,
                is_pinned: None,
            },
            Some(&dek),
        )
        .expect("Create note failed");
        note_id = note.id;

        let storage = AttachmentStorage::new(env.paths.clone());
        let source_file = env.dir.join("att.bin");
        fs::write(&source_file, att_payload).unwrap();
        let att = AttachmentService::add_attachment_with_key(
            &conn,
            &storage,
            &note_id,
            &source_file,
            Some(&dek),
        )
        .expect("Add attachment failed");
        att_id = att.id;

        // 1. Change the password
        let change_dto = ChangePasswordDto {
            current_password: old_password.to_string(),
            new_password: new_password.to_string(),
            confirm_new_password: new_password.to_string(),
        };
        session
            .change_password(&conn, change_dto)
            .expect("Change password failed");
    }

    // 2. Restart the application (cold boot)
    {
        let conn_restarted =
            init_connection(&env.paths.database_file).expect("Restart DB init failed");
        let session_restarted = CryptoSession::init_from_database(&conn_restarted)
            .expect("Restart session init failed");

        assert!(session_restarted.is_locked());

        // 4. Confirm the old password no longer works
        let old_err = session_restarted
            .unlock(UnlockDto {
                password: old_password.to_string(),
            })
            .expect_err("Old password must be rejected");
        assert_eq!(old_err, CryptoError::AuthenticationFailed);
        assert!(session_restarted.is_locked());

        // 3. Confirm the new password works
        session_restarted
            .unlock(UnlockDto {
                password: new_password.to_string(),
            })
            .expect("New password must successfully unlock");
        assert!(!session_restarted.is_locked());

        // 5. Verify note and attachment contents remain intact
        let dek_restarted = session_restarted
            .get_active_dek()
            .expect("Active DEK failed");

        let recovered_note =
            NoteRepository::get_by_id_with_key(&conn_restarted, &note_id, Some(&dek_restarted))
                .unwrap()
                .unwrap();
        assert_eq!(recovered_note.title, "Credential Modification Note");
        assert_eq!(
            recovered_note.content,
            "Secret content to be preserved across password updates."
        );

        let storage = AttachmentStorage::new(env.paths.clone());
        let open_path = AttachmentService::get_attachment_for_open_with_key(
            &conn_restarted,
            &storage,
            &att_id,
            Some(&dek_restarted),
        )
        .expect("Open attachment failed");
        let recovered_bytes = fs::read(&open_path).unwrap();
        assert_eq!(recovered_bytes, att_payload);
    }
}

// =============================================================================
// Scenario D — Tampered Storage
// 1. Create a test encrypted payload.
// 2. Modify its ciphertext or authentication tag.
// 3. Attempt to read it.
// 4. Confirm authentication fails.
// 5. Confirm the application does not silently replace the content with empty data.
// =============================================================================
#[test]
fn test_scenario_d_tampered_storage_rejection() {
    let env = TestEnv::new("pn_task24_scen_d");
    let password = "TamperTestPassword777!";

    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migrations failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Setup failed");

    let dek = session.get_active_dek().expect("DEK failed");

    // 1. Create a test encrypted note
    let original_title = "Authentic Uncompromised Title";
    let original_content = "Authentic Uncompromised Note Content with critical figures.";
    let note = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some(original_title.to_string()),
            content: Some(original_content.to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
        Some(&dek),
    )
    .expect("Create note failed");

    // Read stored valid ciphertext
    let valid_enc: String = conn
        .query_row("SELECT content FROM notes WHERE id = ?1", [&note.id], |r| {
            r.get(0)
        })
        .unwrap();

    // 2. Modify its ciphertext payload
    let mut b64 = valid_enc.strip_prefix("ENC$v1$").unwrap().to_string();
    let last_char = if b64.ends_with('X') { 'Y' } else { 'X' };
    b64.pop();
    b64.push(last_char);
    let tampered_enc = format!("ENC$v1${b64}");

    conn.execute(
        "UPDATE notes SET content = ?1 WHERE id = ?2",
        [&tampered_enc, &note.id],
    )
    .unwrap();

    // 3. Attempt to read it
    let read_result = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&dek));

    // 4. Confirm authentication fails
    assert!(
        read_result.is_err(),
        "Reading tampered note must produce an error"
    );
    match read_result.unwrap_err() {
        StorageError::Crypto(msg) => {
            assert!(
                msg.contains("Security check failed")
                    || msg.contains("modified")
                    || msg.contains("damaged"),
                "Error message must indicate tampering: {msg}"
            );
        }
        other => panic!("Expected StorageError::Crypto, got {:?}", other),
    }

    // 5. Confirm the application does not silently replace the content with empty data
    // Query raw SQLite row: content must still contain the tampered ciphertext, NOT empty string or NULL
    let current_raw: String = conn
        .query_row("SELECT content FROM notes WHERE id = ?1", [&note.id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        current_raw, tampered_enc,
        "Damaged ciphertext must not be silently replaced or erased"
    );
    assert!(
        !current_raw.is_empty(),
        "Must not silently replace with empty data"
    );
}

// =============================================================================
// Scenario E — Interrupted Migration (Startup Recovery and Rollback)
// 1. Prepare an unencrypted test installation.
// 2. Start the migration.
// 3. Simulate failure at a controlled migration stage.
// 4. Restart the application.
// 5. Verify that the migration state is detected.
// 6. Verify that the documented recovery procedure restores a usable state.
// =============================================================================
#[test]
fn test_scenario_e_interrupted_migration_startup_recovery() {
    let env = TestEnv::new("pn_task24_scen_e");

    let note_id: String;
    let initial_title = "Pre-Migration Production Note";
    let initial_content = "Pre-migration content that must be restored if migration crashes.";

    // 1. Prepare an unencrypted test installation
    {
        let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(initial_title.to_string()),
                content: Some(initial_content.to_string()),
                format: None,
                notebook_id: None,
                is_favorite: None,
                is_pinned: None,
            },
        )
        .expect("Create note failed");
        note_id = note.id;

        // 2 & 3. Start migration, create recovery checkpoint, simulate crash at MigrationInProgress
        let checkpoint_path = MigrationService::create_recovery_checkpoint(&conn, &env.paths)
            .expect("Create checkpoint failed");

        let marker = EncryptionTransitionMarker {
            operation_id: "op-simulated-crash-01".to_string(),
            operation_type: "setup_migration".to_string(),
            stage: MigrationStatus::MigrationInProgress,
            checkpoint_path: checkpoint_path.to_string_lossy().to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        MigrationService::save_transition_marker(&env.paths, &marker).unwrap();

        // Mutate note to simulated half-migrated state
        conn.execute(
            "UPDATE notes SET content = 'ENC$v1$corrupted_mid_flight_state' WHERE id = ?1",
            [&note_id],
        )
        .unwrap();
    }

    // 4. Restart the application (cold boot)
    {
        let mut conn_reboot =
            init_connection(&env.paths.database_file).expect("Reboot DB init failed");

        // 5. Verify that the migration state is detected
        let decision = MigrationService::detect_and_recover_at_startup(&conn_reboot, &env.paths)
            .expect("Detect at startup failed");

        match decision {
            StartupRecoveryDecision::RestrictedRecoveryRequired {
                stage,
                checkpoint_path,
            } => {
                assert_eq!(stage, MigrationStatus::MigrationInProgress);
                assert!(checkpoint_path.is_some());
            }
            other => panic!("Expected RestrictedRecoveryRequired, got {:?}", other),
        }

        // 6. Verify that the documented recovery procedure restores a usable state
        let rollback_msg =
            MigrationService::rollback_interrupted_migration(&mut conn_reboot, &env.paths)
                .expect("Rollback failed");
        assert!(
            rollback_msg.contains("successfully")
                || rollback_msg.contains("restored")
                || rollback_msg.contains("checkpoint")
        );

        // Reconnect and verify authoritative unencrypted data state restored
        let conn_restored = init_connection(&env.paths.database_file).expect("Reconnect failed");
        let restored_note = NoteRepository::get_by_id(&conn_restored, &note_id)
            .unwrap()
            .unwrap();

        assert_eq!(restored_note.title, initial_title);
        assert_eq!(restored_note.content, initial_content);

        // Verify transition marker removed
        assert!(MigrationService::read_transition_marker(&env.paths)
            .unwrap()
            .is_none());
    }
}

// =============================================================================
// Scenario F — Encrypted Backup and Restore
// 1. Create a backup from the encrypted installation.
// 2. Validate the backup.
// 3. Restore it in an isolated test environment.
// 4. Unlock the restored application.
// 5. Verify notes, metadata, attachments, and search.
// 6. Confirm the backup does not expose sensitive plaintext beyond documented policy.
// =============================================================================
#[test]
fn test_scenario_f_encrypted_backup_and_restore() {
    let env_src = TestEnv::new("pn_task24_scen_f_src");
    let password = "EncryptedBackupPassword999!";

    let note_id: String;
    let nb_id: String;
    let att_id: String;
    let att_payload = b"Confidential document inside encrypted backup";

    let backup_dest = env_src.paths.backups_dir.join("prod_backup.pnbackup");

    // Setup encrypted source installation
    {
        let mut conn_src = init_connection(&env_src.paths.database_file).expect("DB init failed");
        run_migrations(&mut conn_src).expect("Migrations failed");

        let session_src =
            CryptoSession::init_from_database(&conn_src).expect("Session init failed");
        session_src
            .setup_encryption(
                &conn_src,
                &env_src.paths,
                SetupEncryptionDto {
                    password: password.to_string(),
                    confirm_password: password.to_string(),
                },
            )
            .expect("Setup failed");

        let dek_src = session_src.get_active_dek().expect("DEK failed");

        let nb = NotebookRepository::create(
            &conn_src,
            CreateNotebookDto {
                name: "Intelligence Briefs".to_string(),
                parent_id: None,
            },
        )
        .expect("Create notebook failed");
        nb_id = nb.id;

        let note = NoteRepository::create_with_key(
            &conn_src,
            CreateNoteDto {
                title: Some("Orbital Defense Systems".to_string()),
                content: Some(
                    "Target telemetry and radar frequency calibration protocols.".to_string(),
                ),
                format: None,
                notebook_id: Some(nb_id.clone()),
                is_favorite: Some(true),
                is_pinned: Some(false),
            },
            Some(&dek_src),
        )
        .expect("Create note failed");
        note_id = note.id;

        let storage_src = AttachmentStorage::new(env_src.paths.clone());
        let source_file = env_src.dir.join("brief.pdf");
        fs::write(&source_file, att_payload).unwrap();
        let att = AttachmentService::add_attachment_with_key(
            &conn_src,
            &storage_src,
            &note_id,
            &source_file,
            Some(&dek_src),
        )
        .expect("Add attachment failed");
        att_id = att.id;

        // 1. Create a backup from the encrypted installation
        let bk_res = BackupService::create_backup(
            &conn_src,
            &env_src.paths,
            BackupCreateRequest {
                destination_path: Some(backup_dest.to_string_lossy().to_string()),
                backup_type: Some(BackupType::Manual),
                allow_overwrite: Some(true),
            },
            None::<fn(personal_notepad_lib::storage::models::BackupProgressEvent)>,
        )
        .expect("Backup creation failed");

        assert!(
            bk_res.is_encrypted,
            "BackupCreateResult must report is_encrypted = true"
        );
        assert!(backup_dest.exists());

        // 6. Confirm the backup does not expose sensitive plaintext beyond documented policy
        let zip_file = File::open(&backup_dest).expect("Open ZIP failed");
        let mut archive = ZipArchive::new(zip_file).expect("Read ZIP failed");

        // Verify manifest declares encryption
        {
            let mut manifest_entry = archive.by_name("manifest.json").expect("manifest missing");
            let mut manifest_json = String::new();
            manifest_entry.read_to_string(&mut manifest_json).unwrap();
            let manifest: personal_notepad_lib::storage::models::BackupManifest =
                serde_json::from_str(&manifest_json).expect("Parse manifest failed");
            assert!(manifest.is_encrypted);
            assert_eq!(
                manifest.encryption_algorithm.as_deref(),
                Some("AES-256-GCM")
            );
        }

        // Verify database entry in ZIP contains ENC$v1$ and NO plaintext note content
        {
            let mut db_entry = archive.by_name("database.sqlite").expect("db missing");
            let mut db_bytes = Vec::new();
            db_entry.read_to_end(&mut db_bytes).unwrap();
            let db_str = String::from_utf8_lossy(&db_bytes);
            assert!(db_str.contains("ENC$v1$"));
            assert!(!db_str.contains("Target telemetry and radar frequency"));
        }
    }

    // 2. Validate the backup
    let val_res = BackupValidationService::validate_backup(&backup_dest, &env_src.paths)
        .expect("Validate failed");
    assert!(val_res.is_valid);

    // 3. Restore it in an isolated test environment
    let env_dest = TestEnv::new("pn_task24_scen_f_dest");
    {
        let mut conn_dest =
            init_connection(&env_dest.paths.database_file).expect("Dest DB init failed");
        run_migrations(&mut conn_dest).expect("Dest migrations failed");

        let restore_res = RestoreService::execute_restore(
            &mut conn_dest,
            &env_dest.paths,
            BackupRestoreRequest {
                file_path: backup_dest.to_string_lossy().to_string(),
                operation_id: Some("op-e2e-restore-scen-f".to_string()),
            },
            None::<fn(personal_notepad_lib::storage::models::BackupProgressEvent)>,
        )
        .expect("Restore failed");

        assert!(restore_res.success);
    }

    // 4. Unlock the restored application
    {
        let conn_after = init_connection(&env_dest.paths.database_file).expect("Reconnect failed");
        let session_dest =
            CryptoSession::init_from_database(&conn_after).expect("Session init failed");

        assert!(session_dest.is_encryption_enabled());
        assert!(session_dest.is_locked());

        session_dest
            .unlock(UnlockDto {
                password: password.to_string(),
            })
            .expect("Unlock restored session failed");
        assert!(!session_dest.is_locked());

        let dek_dest = session_dest.get_active_dek().expect("Get DEK failed");

        // 5. Verify notes, metadata, attachments, and search
        let restored_note =
            NoteRepository::get_by_id_with_key(&conn_after, &note_id, Some(&dek_dest))
                .unwrap()
                .unwrap();

        assert_eq!(restored_note.title, "Orbital Defense Systems");
        assert_eq!(
            restored_note.content,
            "Target telemetry and radar frequency calibration protocols."
        );
        assert_eq!(restored_note.notebook_id.as_deref(), Some(nb_id.as_str()));
        assert!(restored_note.is_favorite);

        let storage_dest = AttachmentStorage::new(env_dest.paths.clone());
        let open_path = AttachmentService::get_attachment_for_open_with_key(
            &conn_after,
            &storage_dest,
            &att_id,
            Some(&dek_dest),
        )
        .expect("Open attachment failed");
        let decrypted_bytes = fs::read(&open_path).unwrap();
        assert_eq!(decrypted_bytes, att_payload);

        // Search test: Search when unlocked matches decrypted content
        let search_results =
            SearchRepository::search_with_key(&conn_after, "telemetry", None, Some(&dek_dest))
                .expect("Search failed");
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].note_id, note_id);
    }
}
