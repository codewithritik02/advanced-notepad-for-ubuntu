mod common;

use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use personal_notepad_lib::storage::{
    attachment_storage::AttachmentStorage,
    crypto::{CryptoSession, EncryptionTransitionMarker, MigrationService, MigrationStatus},
    errors::{AttachmentError, BackupError, CryptoError, ErrorCategory, StorageError},
    models::{
        BackupCompatibilityStatus, BackupCreateRequest, BackupRestoreRequest, BackupType,
        CreateNoteDto, ExportBatchRequest, ExportFormat, ExportScope, OperationOutcome,
        UpdateNoteDto,
    },
    repositories::{NoteRepository, SearchRepository},
    services::{
        AttachmentService, BackupService, ImportExportService, RestoreService, TrashService,
    },
};

use common::{lock_process_global_test_state, TestAppDir};

// =============================================================================
// Task 17 — Test Failure Injection and Recovery
// =============================================================================

/// 1. Injected database write failure during multi-field note update rolls back atomically.
#[test]
fn test_injected_database_write_failure_and_transaction_rollback() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Create a valid initial note
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Original Title".to_string()),
            content: Some("Original Content that must remain unchanged.".to_string()),
            ..Default::default()
        },
    )
    .expect("create original note");

    // 2. Install a temporary SQLite trigger to inject write failure on update
    conn.execute_batch(
        r#"
        CREATE TRIGGER injected_update_failure
        BEFORE UPDATE ON notes
        BEGIN
            SELECT RAISE(ABORT, 'Simulated mid-transaction database write failure');
        END;
        "#,
    )
    .expect("install trigger");

    // 3. Attempt update; must fail due to injected trigger
    let update_res = NoteRepository::update(
        &conn,
        &note.id,
        UpdateNoteDto {
            title: Some("Corrupted Update Title".to_string()),
            content: Some("Corrupted Content Body".to_string()),
            ..Default::default()
        },
    );

    assert!(update_res.is_err(), "Update must fail when trigger aborts");
    let err = update_res.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("Simulated mid-transaction database write failure"),
        "Error must cite the injected failure: {err_str}"
    );

    // 4. Assert prior committed note data is 100% intact (zero partial writes)
    let fetched = NoteRepository::get_by_id(&conn, &note.id)
        .expect("get note")
        .expect("note exists");
    assert_eq!(fetched.title, "Original Title");
    assert_eq!(
        fetched.content,
        "Original Content that must remain unchanged."
    );

    // 5. Drop trigger; subsequent update succeeds cleanly
    conn.execute_batch("DROP TRIGGER injected_update_failure;")
        .expect("drop trigger");

    let clean_update = NoteRepository::update(
        &conn,
        &note.id,
        UpdateNoteDto {
            title: Some("Clean Updated Title".to_string()),
            ..Default::default()
        },
    )
    .expect("clean update succeeds");

    assert_eq!(clean_update.title, "Clean Updated Title");
}

/// 2. SQLite busy contention and locked database errors are handled with clean retryable semantics.
#[test]
fn test_sqlite_busy_and_locked_contention_handling() {
    let app_dir = TestAppDir::new();
    let conn1 = app_dir.open_migrated_database();
    let conn2 = rusqlite::Connection::open(&app_dir.paths.database_file).expect("open conn2");

    // Connection 2 takes exclusive lock without timeout
    conn2
        .execute_batch("PRAGMA busy_timeout = 0; BEGIN EXCLUSIVE;")
        .expect("begin exclusive in conn2");
    conn2
        .execute(
            "INSERT INTO settings (key, value) VALUES ('lock_holder', 'conn2')",
            [],
        )
        .expect("insert in conn2");

    // Connection 1 attempts to write with 0 busy timeout
    conn1
        .execute_batch("PRAGMA busy_timeout = 0;")
        .expect("set timeout 0 on conn1");
    let write_res = conn1.execute(
        "INSERT INTO settings (key, value) VALUES ('lock_contender', 'conn1')",
        [],
    );

    assert!(
        write_res.is_err(),
        "Write must fail while exclusive lock is held by another connection"
    );
    let err = write_res.unwrap_err();
    match &err {
        rusqlite::Error::SqliteFailure(sqlite_err, _) => {
            assert!(
                sqlite_err.code == rusqlite::ErrorCode::DatabaseBusy
                    || sqlite_err.code == rusqlite::ErrorCode::DatabaseLocked,
                "Must return DatabaseBusy or DatabaseLocked, got {:?}",
                sqlite_err.code
            );
        }
        other => panic!("Expected SqliteFailure, got {other:?}"),
    }

    let storage_err = StorageError::Database(err);
    assert_eq!(
        storage_err.category(),
        ErrorCategory::DatabaseBusy,
        "StorageError must classify contention as DatabaseBusy"
    );
    assert!(
        storage_err
            .to_diagnostic(Some("insert_setting"), None)
            .retryable,
        "DatabaseBusy must be classified as retryable"
    );

    // Release lock on Connection 2
    conn2.execute_batch("ROLLBACK;").expect("rollback conn2");

    // Connection 1 retries write and succeeds cleanly without corruption
    conn1
        .execute(
            "INSERT INTO settings (key, value) VALUES ('lock_contender', 'conn1')",
            [],
        )
        .expect("retry write succeeds");

    let val: String = conn1
        .query_row(
            "SELECT value FROM settings WHERE key = 'lock_contender'",
            [],
            |r| r.get(0),
        )
        .expect("query setting");
    assert_eq!(val, "conn1");
}

/// 3. Filesystem permission denial (EACCES) is handled safely without leaking uncommitted state.
#[test]
fn test_filesystem_permission_denial_handling() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let storage = AttachmentStorage::new(app_dir.paths.clone());

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Permission Denial Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let src_file = app_dir.dir.join("sample_asset.bin");
    fs::write(&src_file, b"attachment file content").expect("write asset");

    let note_dir = storage.get_note_dir(&note.id).expect("get note dir");
    fs::create_dir_all(&note_dir).expect("create note dir");

    // Make note directory read-only (mode 0555 on POSIX)
    fs::set_permissions(&note_dir, Permissions::from_mode(0o555)).expect("set read-only");

    let att_res = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file);
    assert!(
        att_res.is_err(),
        "Add attachment must fail when directory is unwritable"
    );

    let err = att_res.unwrap_err();
    let cat = StorageError::from(err).category();
    assert_eq!(
        cat,
        ErrorCategory::Permission,
        "Must classify error as Permission"
    );

    // Assert no attachment record was written to the database
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM attachments WHERE note_id = ?1",
            rusqlite::params![&note.id],
            |r| r.get(0),
        )
        .expect("count attachments");
    assert_eq!(
        count, 0,
        "No database record must exist after permission failure"
    );

    // Restore permissions
    fs::set_permissions(&note_dir, Permissions::from_mode(0o755)).expect("restore perms");

    // Retry succeeds cleanly
    let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file)
        .expect("retry add attachment succeeds");
    assert_eq!(att.file_name, "sample_asset.bin");
}

/// 4. Disk-space exhaustion simulation (ENOSPC / OS error 28) maps to structured diagnostics.
#[test]
fn test_disk_space_exhaustion_simulation_and_classification() {
    // Simulate raw OS error 28 (ENOSPC on Linux / POSIX)
    let enospc_io_err = std::io::Error::from_raw_os_error(28);
    let storage_err = StorageError::from(enospc_io_err);

    assert_eq!(
        storage_err.category(),
        ErrorCategory::DiskSpace,
        "OS error 28 must classify as ErrorCategory::DiskSpace"
    );
    assert!(
        storage_err
            .user_friendly_message()
            .to_lowercase()
            .contains("insufficient disk space"),
        "User friendly message must cite insufficient disk space"
    );

    let diagnostic = storage_err.to_diagnostic(Some("save_attachment"), None);
    assert_eq!(diagnostic.code, "INSUFFICIENT_DISK_SPACE");
    assert!(
        !diagnostic.retryable,
        "DiskSpace is not retryable without freeing space"
    );

    // Subsystem specific error variants
    assert_eq!(
        AttachmentError::InsufficientDiskSpace.user_friendly_message(),
        "Insufficient disk space to save attachment."
    );
    assert_eq!(
        BackupError::InsufficientDiskSpace.user_friendly_message(),
        "Insufficient disk space to complete backup operation."
    );
}

/// 5. Interrupted attachment copy or missing source leaves zero orphaned files and clean database.
#[test]
fn test_interrupted_attachment_copy_and_consistency_cleanup() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let storage = AttachmentStorage::new(app_dir.paths.clone());

    let note = NoteRepository::create(&conn, CreateNoteDto::default()).expect("create note");

    let missing_path = app_dir.dir.join("nonexistent_source_file.dat");
    let res = AttachmentService::add_attachment(&conn, &storage, &note.id, &missing_path);
    assert!(res.is_err(), "Must fail when source file is missing");

    // Storage consistency check must pass with 0 missing and 0 unreferenced files
    let report = AttachmentService::check_consistency(&conn, &storage).expect("consistency check");
    assert!(report.is_consistent, "Storage must be consistent");
    assert_eq!(report.missing_managed_files.len(), 0);
    assert_eq!(report.unreferenced_managed_files.len(), 0);
    assert_eq!(report.size_mismatches.len(), 0);

    let att_list = AttachmentService::list_attachments(&conn, &note.id).expect("list");
    assert_eq!(att_list.len(), 0);
}

/// 6. Interrupted export cancellation cleanly aborts without altering database notes.
#[test]
fn test_interrupted_export_cancellation_and_isolation() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // Create 3 notes
    for i in 1..=3 {
        NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Export Target Note {i}")),
                content: Some(format!("Content for note {i}")),
                ..Default::default()
            },
        )
        .expect("create note");
    }

    let export_dir = app_dir.dir.join("export_cancelled_test");
    fs::create_dir_all(&export_dir).expect("create export dir");

    let op_id = "op-cancel-test-123".to_string();

    // Pre-cancel operation
    ImportExportService::cancel_operation(&op_id);

    let request = ExportBatchRequest {
        operation_id: Some(op_id.clone()),
        scope: Some(ExportScope::AllNotes),
        note_ids: Vec::new(),
        format: ExportFormat::Md,
        destination_dir: export_dir.to_string_lossy().to_string(),
        destination_file: None,
        include_metadata: true,
        overwrite: true,
    };

    let summary = ImportExportService::export_notes_batch(&conn, request).expect("execute export");
    assert_eq!(summary.calculate_outcome(), OperationOutcome::Cancelled);
    assert_eq!(summary.cancelled_count, 3);
    assert_eq!(summary.exported_count, 0);

    // Database notes are 100% untouched
    let all_notes = NoteRepository::list(&conn, false).expect("list notes");
    assert_eq!(all_notes.len(), 3);

    ImportExportService::clear_operation(Some(&op_id));
}

/// 7. Failed backup creation targeting unwritable path cleanly cleans all temporary artifacts.
#[test]
fn test_failed_backup_creation_cleans_temporary_artifacts() {
    let _guard = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Backup Live Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // Attempt backup to an invalid/impossible destination path
    let impossible_target = Path::new("/dev/null/forbidden_subpath/impossible_backup.pnbackup");
    let req = BackupCreateRequest {
        destination_path: Some(impossible_target.to_string_lossy().to_string()),
        backup_type: Some(BackupType::Manual),
        allow_overwrite: Some(false),
    };

    let backup_res = BackupService::create_backup(&conn, &app_dir.paths, req, None::<fn(_)>);
    assert!(backup_res.is_err(), "Backup to impossible path must fail");

    // Verify backups directory has no leaked .tmp_* files
    let backups_dir = &app_dir.paths.backups_dir;
    if backups_dir.exists() {
        let entries: Vec<_> = fs::read_dir(backups_dir)
            .expect("read backups")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(
            entries.is_empty(),
            "Backups directory must be clean on backup failure"
        );
    }

    // Database integrity remains OK
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("integrity check");
    assert_eq!(integrity, "ok");
}

/// 8. Failed restore validation (corrupted archive) preserves live database untouched.
#[test]
fn test_failed_restore_validation_preserves_live_database() {
    let _guard = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let mut conn = app_dir.open_migrated_database();

    // 1. Seed 3 authoritative live notes
    for i in 1..=3 {
        NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Authoritative Live Note {i}")),
                content: Some(format!("Preserved content {i}")),
                ..Default::default()
            },
        )
        .expect("create live note");
    }

    // 2. Create a corrupted backup file
    let corrupt_archive = app_dir.dir.join("corrupted_payload.pnbackup");
    fs::write(&corrupt_archive, b"CORRUPTED_NON_ZIP_BYTES_12345").expect("write corrupt archive");

    // 3. Preview must detect corruption and refuse restore
    let preview = RestoreService::preview_restore(&corrupt_archive, &app_dir.paths)
        .expect("preview corrupt archive");
    assert!(
        !preview.can_restore,
        "Must not allow restore of corrupted archive"
    );
    assert_eq!(preview.compatibility, BackupCompatibilityStatus::Corrupted);

    // 4. Execute restore must fail
    let restore_res = RestoreService::execute_restore(
        &mut conn,
        &app_dir.paths,
        BackupRestoreRequest {
            file_path: corrupt_archive.to_string_lossy().to_string(),
            operation_id: Some("op-fail-restore-test".to_string()),
        },
        None::<fn(_)>,
    );
    assert!(
        restore_res.is_err(),
        "Execute restore of corrupt archive must fail"
    );

    let notes = NoteRepository::list(&conn, false).expect("list notes");
    assert_eq!(notes.len(), 3);
    assert!(notes.iter().any(|n| n.title == "Authoritative Live Note 1"));
    assert!(notes.iter().any(|n| n.title == "Authoritative Live Note 2"));
    assert!(notes.iter().any(|n| n.title == "Authoritative Live Note 3"));
}

/// 9. Pre-restore recovery checkpoint enables complete rollback of interrupted restore.
#[test]
fn test_restore_atomic_rollback_from_recovery_checkpoint() {
    let _guard = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let mut conn = app_dir.open_migrated_database();

    // 1. Seed authoritative data
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Pre-Restore Authoritative Note".to_string()),
            content: Some("Authoritative body before checkpoint.".to_string()),
            ..Default::default()
        },
    )
    .expect("create authoritative note");

    // 2. Create recovery checkpoint
    let checkpoint = RestoreService::create_recovery_checkpoint(&conn, &app_dir.paths)
        .expect("create checkpoint");
    assert!(Path::new(&checkpoint.file_path).exists());

    // 3. Mutate database (simulate undesired or damaged restore state)
    NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Damaged Restore State".to_string()),
            ..Default::default()
        },
    )
    .expect("create damaged state note");
    NoteRepository::update(
        &conn,
        &note.id,
        UpdateNoteDto {
            title: Some("Tampered Title".to_string()),
            ..Default::default()
        },
    )
    .expect("tamper title");

    // 4. Restore using the pre-restore recovery checkpoint
    let rollback_res = RestoreService::execute_restore(
        &mut conn,
        &app_dir.paths,
        BackupRestoreRequest {
            file_path: checkpoint.file_path.clone(),
            operation_id: Some("op-rollback-test".to_string()),
        },
        None::<fn(_)>,
    )
    .expect("execute rollback from checkpoint");

    assert!(rollback_res.success);

    // 5. Verify authoritative state restored 100%
    let notes = NoteRepository::list(&conn, false).expect("list");
    assert_eq!(
        notes.len(),
        1,
        "Only the original note must exist after rollback"
    );
    assert_eq!(notes[0].title, "Pre-Restore Authoritative Note");
    assert_eq!(notes[0].content, "Authoritative body before checkpoint.");
}

/// 10. FTS5 full-text search index corruption is diagnosed and reconstituted automatically.
#[test]
fn test_fts5_search_index_corruption_detection_and_automatic_recovery() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quantum Cryptography Documentation".to_string()),
            content: Some("Key distribution protocol details and security theorems.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // 1. Search works initially
    let initial_results = SearchRepository::search(&conn, "Quantum", None).expect("search");
    assert_eq!(initial_results.len(), 1);

    // 2. Injected corruption: Drop the FTS5 virtual table
    conn.execute_batch("DROP TABLE notes_fts;")
        .expect("drop fts table");

    // 3. Diagnose index reports inconsistency
    let diag = SearchRepository::diagnose_index(&conn).expect("diagnose");
    assert!(!diag.is_consistent, "Index must be flagged inconsistent");
    assert!(!diag.fts_table_exists, "FTS table must be missing");

    // 4. Execute safe recovery
    SearchRepository::recover_index(&conn).expect("recover index");

    // 5. Verify index reconstituted and consistent
    let diag_after = SearchRepository::diagnose_index(&conn).expect("diagnose after");
    assert!(
        diag_after.is_consistent,
        "Index must be consistent after recovery"
    );
    assert!(diag_after.fts_table_exists, "FTS table must be restored");

    // 6. Search works again
    let recovered_results =
        SearchRepository::search(&conn, "Quantum", None).expect("search recovered");
    assert_eq!(recovered_results.len(), 1);
    assert_eq!(
        recovered_results[0].title,
        "Quantum Cryptography Documentation"
    );
}

/// 11. Filesystem failure during attachment deletion is reported honestly without aborting note deletion.
#[test]
fn test_honest_reporting_of_filesystem_partial_failure_during_attachment_deletion() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let storage = AttachmentStorage::new(app_dir.paths.clone());

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Locked Attachment Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let src_file = app_dir.dir.join("partial_cleanup_file.txt");
    fs::write(&src_file, b"attachment to delete").expect("write src");
    AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).expect("add att");

    let note_dir = storage.get_note_dir(&note.id).expect("get dir");
    assert!(note_dir.exists());

    // Soft-delete to Trash
    TrashService::soft_delete(&conn, &note.id).expect("soft delete");

    // Make directory read-only so file deletion fails
    fs::set_permissions(&note_dir, Permissions::from_mode(0o555)).expect("set read-only");

    let res = TrashService::permanently_delete_note(&conn, &storage, &note.id)
        .expect("permanently delete note");

    // Database record was successfully removed
    assert!(res.success);
    assert!(NoteRepository::get_by_id(&conn, &note.id)
        .expect("get")
        .is_none());

    // Filesystem cleanup failure is surfaced honestly
    assert!(
        res.attachment_cleanup_failed,
        "Attachment cleanup failure must be reported honestly"
    );
    assert!(
        res.error_message.is_some(),
        "Must supply descriptive error message for partial cleanup failure"
    );

    // Restore permissions for fixture cleanup
    let _ = fs::set_permissions(&note_dir, Permissions::from_mode(0o755));
}

/// 12. Abandoned temporary files left by ungraceful shutdown are purged without touching active files.
#[test]
fn test_abandoned_temp_files_cleanup_on_restart() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let storage = AttachmentStorage::new(app_dir.paths.clone());

    // 1. Create a legitimate note with active attachment
    let note = NoteRepository::create(&conn, CreateNoteDto::default()).expect("create note");
    let src = app_dir.dir.join("real_asset.bin");
    fs::write(&src, b"valid attachment data").expect("write src");
    let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &src).expect("add att");
    assert!(storage.file_exists(&att.relative_path));

    // 2. Simulate abandoned temporary staging files from interrupted operations
    let note_dir = storage.get_note_dir(&note.id).expect("get note dir");
    let abandoned_1 = note_dir.join(format!("staged_upload_1.tmp.{}", uuid::Uuid::new_v4()));
    let abandoned_2 = note_dir.join(format!("staged_upload_2.tmp.{}", uuid::Uuid::new_v4()));
    fs::write(&abandoned_1, b"partial upload bytes 1").expect("write tmp1");
    fs::write(&abandoned_2, b"partial upload bytes 2").expect("write tmp2");

    assert!(abandoned_1.exists());
    assert!(abandoned_2.exists());

    // 3. Run cleanup with 0-second age threshold
    let cleaned = AttachmentService::cleanup_abandoned_temp_files(&storage, 0).expect("clean temp");
    assert_eq!(cleaned, 2, "Both abandoned temporary files must be deleted");

    assert!(!abandoned_1.exists());
    assert!(!abandoned_2.exists());

    // 4. Verify valid note attachment remains 100% intact
    assert!(
        storage.file_exists(&att.relative_path),
        "Active attachment file must remain intact"
    );
}

/// 13. Interrupted encryption migration detects transition marker and executes safe rollback.
#[test]
fn test_interrupted_encryption_migration_rollback_recovery() {
    let app_dir = TestAppDir::new();
    let mut conn = app_dir.open_migrated_database();

    // 1. Seed unencrypted note
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Important Plaintext Note".to_string()),
            content: Some("Original body before interrupted migration.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // 2. Create pre-migration checkpoint
    let checkpoint = RestoreService::create_recovery_checkpoint(&conn, &app_dir.paths)
        .expect("create checkpoint");

    // 3. Write transition marker simulating interruption at MigrationInProgress
    let marker = EncryptionTransitionMarker {
        operation_id: "op-test-interrupted-mig".to_string(),
        operation_type: "setup_migration".to_string(),
        stage: MigrationStatus::MigrationInProgress,
        checkpoint_path: checkpoint.file_path.clone(),
        started_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    MigrationService::save_transition_marker(&app_dir.paths, &marker).expect("save marker");

    // 4. Initialize session and detect recovery decision
    let session = CryptoSession::init_and_recover(&conn, &app_dir.paths).expect("init and recover");
    assert!(
        session.is_interrupted(),
        "Session must recognize interrupted migration state"
    );

    // 5. Execute safe rollback from recovery checkpoint
    let rollback_msg = session
        .rollback_interrupted_migration(&mut conn, &app_dir.paths)
        .expect("rollback interrupted migration");

    assert!(rollback_msg.to_lowercase().contains("successfully"));
    assert!(!session.is_interrupted());
    assert!(!session.is_encryption_enabled());

    // 6. Verify transition marker was deleted
    let marker_check =
        MigrationService::read_transition_marker(&app_dir.paths).expect("read marker");
    assert!(
        marker_check.is_none(),
        "Transition marker must be removed after rollback"
    );

    // 7. Verify original unencrypted note remains intact and accessible
    let fetched = NoteRepository::get_by_id(&conn, &note.id)
        .expect("get note")
        .expect("note exists");
    assert_eq!(fetched.title, "Important Plaintext Note");
    assert_eq!(
        fetched.content,
        "Original body before interrupted migration."
    );
}

/// 14. Error diagnostics privacy guarantee: sensitive tokens, passwords, and note bodies never leaked.
#[test]
fn test_privacy_guarantee_no_sensitive_data_in_error_diagnostics() {
    let secret_password = "SuperSecretMasterPassword#2026!";
    let secret_dek = "d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1";

    // 1. StorageError derived from typed CryptoError never exposes raw password or secrets
    let auth_error = StorageError::from(CryptoError::AuthenticationFailed);
    let diagnostic = auth_error.to_diagnostic(Some("unlock"), None);

    assert_eq!(diagnostic.code, "INTERNAL_ERROR");
    assert_eq!(diagnostic.category, ErrorCategory::Internal);
    assert_eq!(diagnostic.message, "Incorrect password. Please try again.");
    assert!(!diagnostic.message.contains(secret_password));
    assert!(!diagnostic.message.contains(secret_dek));

    // Tampering error maps to DATA_CORRUPTION
    let tamper_error =
        StorageError::Crypto("Data tampering detected or corrupted ciphertext".to_string());
    let tamper_diag = tamper_error.to_diagnostic(Some("decrypt"), None);
    assert_eq!(tamper_diag.code, "DATA_CORRUPTION");
    assert_eq!(tamper_diag.category, ErrorCategory::Corruption);

    // 2. sanitize_security_message redacts password and secret keys
    let sanitized = personal_notepad_lib::storage::crypto::sanitize_security_message(&format!(
        "Authentication failed for password: {secret_password}, dek: {secret_dek}"
    ));
    assert!(!sanitized.contains(secret_password));
    assert!(!sanitized.contains(secret_dek));
    assert!(sanitized.contains("[REDACTED]"));

    let validation_error = StorageError::Validation("Invalid input parameters".to_string());
    let diag_val = validation_error.to_diagnostic(Some("create_note"), Some("note-uuid-123"));
    assert_eq!(diag_val.code, "VALIDATION_FAILED");
    assert_eq!(diag_val.category, ErrorCategory::Validation);
    assert_eq!(diag_val.safe_entity_id.as_deref(), Some("note-uuid-123"));
}
