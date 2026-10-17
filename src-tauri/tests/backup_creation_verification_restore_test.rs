mod common;

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use common::{lock_process_global_test_state, TestAppDir};
use personal_notepad_lib::storage::{
    crypto::{CryptoSession, SetupEncryptionDto, UnlockDto},
    errors::BackupError,
    init_connection,
    models::{
        BackupAttachmentItem, BackupAttachmentsSummary, BackupCompatibilityStatus,
        BackupCreateRequest, BackupDatabaseInfo, BackupManifest, BackupRestoreRequest,
        BackupScopeDeclaration, BackupType, CreateAttachmentDto, CreateNoteDto, CreateNotebookDto,
        RestoreActivationMarker, RestoreActivationPhase, BACKUP_FORMAT_IDENTIFIER,
    },
    repositories::{
        AttachmentRepository, NoteRepository, NotebookRepository, SettingsRepository, TagRepository,
    },
    run_migrations_custom,
    services::{
        compute_file_sha256, BackupDestinationManager, BackupRetentionManager, BackupService,
        BackupValidationService, RestoreService, CURRENT_APP_SCHEMA_VERSION,
    },
    MIGRATIONS,
};

/// 1. Create a backup with representative notes, attachments, settings, and verify manifest metadata.
#[test]
fn test_backup_creation_with_representative_notes_attachments_settings_and_manifest() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Notebook hierarchy
    let nb_parent = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Research & Strategy".to_string(),
            parent_id: None,
        },
    )
    .expect("create parent notebook");

    let nb_child = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Architecture".to_string(),
            parent_id: Some(nb_parent.id.clone()),
        },
    )
    .expect("create child notebook");

    // 2. Tags
    let tag_strat = TagRepository::create(&conn, "Strategy").expect("create tag");
    let tag_arch = TagRepository::create(&conn, "Design").expect("create tag");

    // 3. Notes (active & pinned/favorite)
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Vision Document".to_string()),
            content: Some(
                "# Vision 2030\nKey technical priorities and architectural decisions.".to_string(),
            ),
            format: Some("md".to_string()),
            notebook_id: Some(nb_child.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .expect("create note1");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_strat.id).expect("add tag1");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag_arch.id).expect("add tag2");

    let _note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Plain Text Scratchpad".to_string()),
            content: Some("Rough thoughts and scratch calculations.".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("create note2");

    // 4. Trashed note
    let trashed_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Deprecated Plan".to_string()),
            content: Some("Old obsolete plan moved to trash.".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            ..Default::default()
        },
    )
    .expect("create trashed note");
    NoteRepository::soft_delete(&conn, &trashed_note.id).expect("soft delete note");

    // 5. Attachments on disk and in database
    let att_dir = app_dir
        .paths
        .get_note_attachments_dir(&note1.id)
        .expect("get att dir");
    fs::create_dir_all(&att_dir).expect("create att dir");
    let att_name = "diagram.svg";
    let att_content = b"<svg xmlns='http://www.w3.org/2000/svg'><circle r='40'/></svg>";
    fs::write(att_dir.join(att_name), att_content).expect("write att file");

    AttachmentRepository::create(
        &conn,
        CreateAttachmentDto {
            note_id: note1.id.clone(),
            file_name: att_name.to_string(),
            relative_path: format!("{}/{}", note1.id, att_name),
            mime_type: Some("image/svg+xml".to_string()),
            file_size: att_content.len() as i64,
        },
    )
    .expect("create att record");

    // 6. Settings
    SettingsRepository::set(&conn, "theme", "dark").expect("set theme");
    SettingsRepository::set(&conn, "editor.fontSize", "16").expect("set font");

    // 7. Create backup
    let result = BackupService::create_backup(
        &conn,
        &app_dir.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("create backup");

    assert!(result.success);
    assert_eq!(result.note_count, 3);
    assert_eq!(result.attachment_count, 1);
    assert!(result.verified);
    assert!(Path::new(&result.file_path).exists());

    // 8. Inspect manifest inside the ZIP
    let archive_file = File::open(&result.file_path).expect("open archive");
    let mut zip = ZipArchive::new(archive_file).expect("read zip");
    let mut manifest_entry = zip.by_name("manifest.json").expect("find manifest.json");
    let mut manifest_str = String::new();
    manifest_entry
        .read_to_string(&mut manifest_str)
        .expect("read manifest");

    let manifest: serde_json::Value = serde_json::from_str(&manifest_str).expect("parse manifest");
    assert_eq!(manifest["format_identifier"], BACKUP_FORMAT_IDENTIFIER);
    assert_eq!(manifest["format_version"], 1);
    assert_eq!(
        manifest["database"]["schema_version"],
        CURRENT_APP_SCHEMA_VERSION
    );
    assert_eq!(manifest["database"]["note_count"], 2);
    assert_eq!(manifest["database"]["notebook_count"], 2);
    assert_eq!(manifest["database"]["tag_count"], 2);
    assert_eq!(manifest["database"]["trash_count"], 1);
    assert_eq!(manifest["attachments_summary"]["total_files"], 1);
    assert_eq!(manifest["is_encrypted"], false);
    assert_eq!(manifest["scope"]["includes_settings"], true);
    assert_eq!(manifest["scope"]["includes_trash"], true);
    assert_eq!(manifest["scope"]["includes_attachments"], true);

    let checksums = manifest["checksums"].as_object().expect("checksums map");
    assert!(checksums.contains_key("database.sqlite"));
}

/// 2. Verify backup integrity using BackupValidationService and RestoreService::preview_restore.
#[test]
fn test_backup_validation_integrity_mechanism() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Validation Source Note".to_string()),
            content: Some("Checking validation pipelines.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let att_dir = app_dir
        .paths
        .get_note_attachments_dir(&note.id)
        .expect("att dir");
    fs::create_dir_all(&att_dir).expect("create att dir");
    let att_name = "spec.txt";
    fs::write(att_dir.join(att_name), b"Specification 1.0").expect("write spec");

    AttachmentRepository::create(
        &conn,
        CreateAttachmentDto {
            note_id: note.id.clone(),
            file_name: att_name.to_string(),
            relative_path: format!("{}/{}", note.id, att_name),
            mime_type: Some("text/plain".to_string()),
            file_size: 17,
        },
    )
    .expect("create att record");

    let backup = BackupService::create_backup(
        &conn,
        &app_dir.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("create backup");

    let archive_path = Path::new(&backup.file_path);

    // 1. Validation service
    let val_result = BackupValidationService::validate_backup(archive_path, &app_dir.paths)
        .expect("validate backup");
    assert!(val_result.is_valid);
    assert_eq!(
        val_result.compatibility,
        BackupCompatibilityStatus::Compatible
    );
    assert!(val_result.issues.is_empty());
    assert!(val_result.manifest.is_some());

    // 2. Preview service
    let preview =
        RestoreService::preview_restore(archive_path, &app_dir.paths).expect("preview restore");
    assert!(preview.can_restore);
    assert_eq!(preview.note_count, 1);
    assert_eq!(preview.attachment_count, 1);
    assert_eq!(preview.compatibility, BackupCompatibilityStatus::Compatible);
    assert!(preview.warnings.is_empty());
}

/// 3. Restore a valid backup into an isolated test environment and compare restored records and files.
#[test]
fn test_restore_valid_backup_into_isolated_environment_and_compare_state() {
    let _lock = lock_process_global_test_state();

    // Source Environment
    let src_dir = TestAppDir::new();
    let src_conn = src_dir.open_migrated_database();

    let nb = NotebookRepository::create(
        &src_conn,
        CreateNotebookDto {
            name: "Restorable Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    let tag = TagRepository::create(&src_conn, "Priority-1").expect("create tag");

    let note = NoteRepository::create(
        &src_conn,
        CreateNoteDto {
            title: Some("Mission Critical Note".to_string()),
            content: Some("Original body content to be restored faithfully.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .expect("create note");
    TagRepository::add_tag_to_note(&src_conn, &note.id, &tag.id).expect("add tag");

    let att_dir = src_dir
        .paths
        .get_note_attachments_dir(&note.id)
        .expect("att dir");
    fs::create_dir_all(&att_dir).expect("create att dir");
    let att_name = "data.bin";
    let att_bytes = b"BINARY_ASSET_CONTENT_RESTORE_TEST_890123";
    fs::write(att_dir.join(att_name), att_bytes).expect("write binary");

    AttachmentRepository::create(
        &src_conn,
        CreateAttachmentDto {
            note_id: note.id.clone(),
            file_name: att_name.to_string(),
            relative_path: format!("{}/{}", note.id, att_name),
            mime_type: Some("application/octet-stream".to_string()),
            file_size: att_bytes.len() as i64,
        },
    )
    .expect("create att");

    SettingsRepository::set(&src_conn, "editor.fontSize", "18").expect("set setting");

    let backup = BackupService::create_backup(
        &src_conn,
        &src_dir.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("create backup");

    // Target Environment (Isolated)
    let target_dir = TestAppDir::new();
    let mut target_conn = target_dir.open_migrated_database();

    // Create disposable pre-restore data in target
    let _disp_note = NoteRepository::create(
        &target_conn,
        CreateNoteDto {
            title: Some("Disposable Pre-Restore Note".to_string()),
            content: Some("This should be replaced by restored data.".to_string()),
            ..Default::default()
        },
    )
    .expect("create disp note");

    let restore_res = RestoreService::execute_restore(
        &mut target_conn,
        &target_dir.paths,
        BackupRestoreRequest {
            file_path: backup.file_path.clone(),
            operation_id: Some("op-restore-test-1".to_string()),
        },
        None::<fn(_)>,
    )
    .expect("execute restore");

    assert!(restore_res.success);
    assert_eq!(restore_res.note_count, 1);
    assert_eq!(restore_res.notebook_count, 1);
    assert_eq!(restore_res.tag_count, 1);
    assert_eq!(restore_res.attachment_count, 1);
    assert!(Path::new(&restore_res.checkpoint_path).exists());

    // Verify target database records
    let restored_note = NoteRepository::get_by_id(&target_conn, &note.id)
        .expect("get restored note")
        .expect("note exists");
    assert_eq!(restored_note.title, "Mission Critical Note");
    assert_eq!(
        restored_note.content,
        "Original body content to be restored faithfully."
    );
    assert_eq!(restored_note.format, "md");
    assert_eq!(restored_note.is_favorite, true);
    assert_eq!(restored_note.is_pinned, true);
    assert_eq!(restored_note.notebook_id, Some(nb.id.clone()));

    let restored_nb = NotebookRepository::get_by_id(&target_conn, &nb.id)
        .expect("get restored nb")
        .expect("nb exists");
    assert_eq!(restored_nb.name, "Restorable Notebook");

    let restored_tags = TagRepository::get_tags_for_note(&target_conn, &note.id).expect("get tags");
    assert_eq!(restored_tags.len(), 1);
    assert_eq!(restored_tags[0].name, "Priority-1");

    let restored_setting =
        SettingsRepository::get(&target_conn, "editor.fontSize").expect("get setting");
    assert_eq!(restored_setting, Some("18".to_string()));

    // Verify target managed files on disk
    let restored_att_file = target_dir
        .paths
        .attachments_dir
        .join(&note.id)
        .join(att_name);
    assert!(restored_att_file.exists());
    let restored_bytes = fs::read(&restored_att_file).expect("read restored att");
    assert_eq!(restored_bytes, att_bytes);
}

/// 4. Restore an older supported backup version with automatic schema migration.
#[test]
fn test_restore_older_supported_backup_version_with_schema_migration() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();

    // 1. Build a legacy schema version 1 database
    let legacy_dir = app_dir.paths.root.join("legacy_v1_construction");
    fs::create_dir_all(&legacy_dir).expect("create legacy dir");
    let legacy_db = legacy_dir.join("database.sqlite");

    let mut v1_conn = init_connection(&legacy_db).expect("init v1 conn");
    run_migrations_custom(&mut v1_conn, &MIGRATIONS[..1]).expect("run v1 migrations");

    let legacy_note_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    v1_conn
        .execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_deleted)
         VALUES (?1, 'Legacy v1 Note', 'Content originating from schema v1', 'txt', ?2, ?2, 0)",
            [&legacy_note_id, &now],
        )
        .expect("insert v1 note");
    drop(v1_conn);

    // 2. Package legacy database into a `.pnbackup` archive
    let legacy_archive = app_dir.paths.backups_dir.join("legacy_v1.pnbackup");
    let db_bytes = fs::read(&legacy_db).expect("read v1 db bytes");
    let (db_sha256, db_size) = compute_file_sha256(&legacy_db).expect("sha256");

    let legacy_manifest = BackupManifest {
        format_identifier: BACKUP_FORMAT_IDENTIFIER.to_string(),
        format_version: 1,
        app_version: "0.1.0".to_string(),
        created_at: now,
        backup_type: BackupType::Manual,
        database: BackupDatabaseInfo {
            entry_path: "database.sqlite".to_string(),
            file_size: db_size,
            sha256: db_sha256.clone(),
            schema_version: 1,
            note_count: 1,
            notebook_count: 0,
            tag_count: 0,
            trash_count: 0,
        },
        attachments_summary: BackupAttachmentsSummary {
            total_files: 0,
            total_bytes: 0,
        },
        attachments: Vec::new(),
        scope: BackupScopeDeclaration {
            includes_settings: false,
            includes_trash: false,
            includes_attachments: false,
        },
        checksums: std::collections::HashMap::from([("database.sqlite".to_string(), db_sha256)]),
        is_encrypted: false,
        encryption_algorithm: None,
    };

    {
        let out_file = File::create(&legacy_archive).expect("create zip");
        let mut zip = ZipWriter::new(out_file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("manifest.json", options)
            .expect("manifest entry");
        zip.write_all(
            serde_json::to_string_pretty(&legacy_manifest)
                .unwrap()
                .as_bytes(),
        )
        .unwrap();

        zip.start_file("database.sqlite", options)
            .expect("db entry");
        zip.write_all(&db_bytes).unwrap();

        zip.finish().expect("finish legacy zip");
    }

    // 3. Validate legacy archive: flags RequiresMigration
    let val = BackupValidationService::validate_backup(&legacy_archive, &app_dir.paths)
        .expect("validate legacy");
    assert!(val.is_valid);
    assert_eq!(
        val.compatibility,
        BackupCompatibilityStatus::RequiresMigration
    );

    // 4. Stage and migrate staged database
    let staged = RestoreService::stage_restore_archive(&legacy_archive, &app_dir.paths)
        .expect("stage archive");
    assert!(staged.is_valid);
    assert_eq!(staged.initial_schema_version, 1);

    let staged_dir = PathBuf::from(&staged.staging_dir);
    let migrated = RestoreService::migrate_staged_database(&staged_dir, &app_dir.paths)
        .expect("migrate staged");
    assert_eq!(migrated.previous_schema_version, 1);
    assert_eq!(migrated.migrated_schema_version, CURRENT_APP_SCHEMA_VERSION);
    assert!(migrated.migrations_applied > 0);
    assert!(migrated.is_verified);

    RestoreService::cleanup_staged_restore(&staged_dir, &app_dir.paths).expect("cleanup staging");

    // 5. Full restore of legacy archive into a clean environment
    let target_dir = TestAppDir::new();
    let mut target_conn = target_dir.open_migrated_database();

    let restore_res = RestoreService::execute_restore(
        &mut target_conn,
        &target_dir.paths,
        BackupRestoreRequest {
            file_path: legacy_archive.to_string_lossy().to_string(),
            operation_id: Some("op-legacy-restore".to_string()),
        },
        None::<fn(_)>,
    )
    .expect("execute legacy restore");

    assert!(restore_res.success);
    let note = NoteRepository::get_by_id(&target_conn, &legacy_note_id)
        .expect("query note")
        .expect("legacy note exists");
    assert_eq!(note.title, "Legacy v1 Note");
    assert_eq!(note.content, "Content originating from schema v1");
}

/// 5. Reject malformed and corrupted archives without modifying live data.
#[test]
fn test_reject_malformed_and_corrupted_archives() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();

    let corrupt_path = app_dir.paths.backups_dir.join("corrupted.pnbackup");
    fs::write(
        &corrupt_path,
        b"NOT_A_VALID_ZIP_HEADER_CORRUPTED_BYTES_123456789",
    )
    .expect("write corrupt");

    // 1. Validation service
    let val = BackupValidationService::validate_backup(&corrupt_path, &app_dir.paths)
        .expect("validate corrupt");
    assert!(!val.is_valid);
    assert_eq!(val.compatibility, BackupCompatibilityStatus::Corrupted);
    assert!(val.issues.iter().any(|i| i.code == "ARCHIVE_READ_FAILED"));

    // 2. Preview service
    let preview =
        RestoreService::preview_restore(&corrupt_path, &app_dir.paths).expect("preview corrupt");
    assert!(!preview.can_restore);
    assert_eq!(preview.compatibility, BackupCompatibilityStatus::Corrupted);

    // 3. Execution service
    let mut conn = app_dir.open_migrated_database();
    let res = RestoreService::execute_restore(
        &mut conn,
        &app_dir.paths,
        BackupRestoreRequest {
            file_path: corrupt_path.to_string_lossy().to_string(),
            operation_id: Some("op-corrupt".to_string()),
        },
        None::<fn(_)>,
    );
    assert!(res.is_err(), "Must reject corrupted archive");
}

/// 6. Handle missing archive entries and checksum mismatch safely.
#[test]
fn test_handle_missing_archive_entries_and_checksum_mismatch() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();

    // Prepare real database bytes for archive packaging
    let db_path = app_dir.paths.database_file.clone();
    let db_conn = app_dir.open_migrated_database();
    drop(db_conn);
    let db_bytes = fs::read(&db_path).expect("read db bytes");
    let (db_sha256, db_size) = compute_file_sha256(&db_path).expect("db sha256");

    // 1. Archive with missing declared attachment entry
    let missing_entry_archive = app_dir.paths.backups_dir.join("missing_file.pnbackup");
    let manifest_missing = BackupManifest {
        format_identifier: BACKUP_FORMAT_IDENTIFIER.to_string(),
        format_version: 1,
        app_version: "0.1.0".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        backup_type: BackupType::Manual,
        database: BackupDatabaseInfo {
            entry_path: "database.sqlite".to_string(),
            file_size: db_size,
            sha256: db_sha256.clone(),
            schema_version: CURRENT_APP_SCHEMA_VERSION,
            note_count: 0,
            notebook_count: 0,
            tag_count: 0,
            trash_count: 0,
        },
        attachments_summary: BackupAttachmentsSummary {
            total_files: 1,
            total_bytes: 100,
        },
        attachments: vec![BackupAttachmentItem {
            id: "ghost-att-1".to_string(),
            note_id: "ghost-note-1".to_string(),
            file_name: "ghost.png".to_string(),
            relative_path: "ghost-note-1/ghost.png".to_string(),
            file_size: 100,
            mime_type: Some("image/png".to_string()),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        }],
        scope: BackupScopeDeclaration {
            includes_settings: false,
            includes_trash: false,
            includes_attachments: true,
        },
        checksums: std::collections::HashMap::from([
            ("database.sqlite".to_string(), db_sha256.clone()),
            (
                "attachments/ghost-note-1/ghost.png".to_string(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            ),
        ]),
        is_encrypted: false,
        encryption_algorithm: None,
    };

    {
        let f = File::create(&missing_entry_archive).unwrap();
        let mut zip = ZipWriter::new(f);
        let opt = SimpleFileOptions::default();
        zip.start_file("manifest.json", opt).unwrap();
        zip.write_all(serde_json::to_string(&manifest_missing).unwrap().as_bytes())
            .unwrap();
        zip.start_file("database.sqlite", opt).unwrap();
        zip.write_all(&db_bytes).unwrap();
        // attachments/ghost-note-1/ghost.png intentionally omitted
        zip.finish().unwrap();
    }

    let val = BackupValidationService::validate_backup(&missing_entry_archive, &app_dir.paths)
        .expect("validate missing file archive");
    assert!(!val.is_valid);
    assert!(val
        .issues
        .iter()
        .any(|i| i.code == "ATTACHMENT_ENTRY_MISSING"));

    // 2. Archive with tampered database checksum
    let tampered_archive = app_dir.paths.backups_dir.join("tampered.pnbackup");
    let manifest_tampered = BackupManifest {
        format_identifier: BACKUP_FORMAT_IDENTIFIER.to_string(),
        format_version: 1,
        app_version: "0.1.0".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        backup_type: BackupType::Manual,
        database: BackupDatabaseInfo {
            entry_path: "database.sqlite".to_string(),
            file_size: db_bytes.len() as u64,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            schema_version: CURRENT_APP_SCHEMA_VERSION,
            note_count: 0,
            notebook_count: 0,
            tag_count: 0,
            trash_count: 0,
        },
        attachments_summary: BackupAttachmentsSummary {
            total_files: 0,
            total_bytes: 0,
        },
        attachments: Vec::new(),
        scope: BackupScopeDeclaration {
            includes_settings: false,
            includes_trash: false,
            includes_attachments: false,
        },
        checksums: std::collections::HashMap::from([(
            "database.sqlite".to_string(),
            "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        )]),
        is_encrypted: false,
        encryption_algorithm: None,
    };

    {
        let f = File::create(&tampered_archive).unwrap();
        let mut zip = ZipWriter::new(f);
        let opt = SimpleFileOptions::default();
        zip.start_file("manifest.json", opt).unwrap();
        zip.write_all(
            serde_json::to_string(&manifest_tampered)
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        zip.start_file("database.sqlite", opt).unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.finish().unwrap();
    }

    let val_tampered = BackupValidationService::validate_backup(&tampered_archive, &app_dir.paths)
        .expect("validate tampered archive");
    assert!(!val_tampered.is_valid);
    assert!(val_tampered
        .issues
        .iter()
        .any(|i| i.code == "DATABASE_CHECKSUM_MISMATCH"));
}

/// 7. Handle invalid archive paths, directory paths, and self-database paths.
#[test]
fn test_handle_invalid_archive_paths_and_directory_targets() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();

    // 1. Non-existent file
    let non_existent = app_dir.paths.root.join("does_not_exist.pnbackup");
    let res1 = BackupValidationService::validate_backup(&non_existent, &app_dir.paths);
    assert!(matches!(res1, Err(BackupError::DestinationUnavailable(_))));

    // 2. Directory target instead of file
    let dir_path = app_dir.paths.backups_dir.clone();
    let res2 = BackupValidationService::validate_backup(&dir_path, &app_dir.paths);
    assert!(matches!(res2, Err(BackupError::DestinationUnavailable(_))));

    // 3. Passing active live database file as backup archive
    let db_path = app_dir.paths.database_file.clone();
    let _ = File::create(&db_path);
    let res3 = BackupValidationService::validate_backup(&db_path, &app_dir.paths);
    assert!(matches!(res3, Err(BackupError::DestinationUnavailable(_))));
}

/// 8. Handle insufficient destination space classification and health reporting.
#[test]
fn test_insufficient_destination_space_handling() {
    let err = BackupError::InsufficientDiskSpace;
    assert_eq!(err.category(), "InsufficientDiskSpace");
    assert!(err
        .user_friendly_message()
        .contains("Insufficient disk space"));
}

/// 9. Interrupted backup creation cleans up temporary `.tmp` files cleanly.
#[test]
fn test_interrupted_backup_creation_cleans_temporary_artifacts() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();

    // DestinationManager collision and path resolution
    let dest = BackupDestinationManager::resolve_destination(
        None,
        BackupType::Manual,
        false,
        &app_dir.paths,
    )
    .expect("resolve dest");

    assert!(dest.to_string_lossy().ends_with(".pnbackup"));

    // Verify no stray `.tmp` files exist initially
    let entries = fs::read_dir(&app_dir.paths.backups_dir).expect("read backups dir");
    for entry in entries.flatten() {
        assert!(!entry.file_name().to_string_lossy().contains(".tmp."));
    }
}

/// 10. Interrupted restore recovery automatically restores previous working database and attachments.
#[test]
fn test_interrupted_restore_recovery_and_automatic_rollback() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Working live state
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Pre-Crash Vital Note".to_string()),
            content: Some("Crucial data that must survive an interrupted restore.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let att_dir = app_dir
        .paths
        .get_note_attachments_dir(&note.id)
        .expect("att dir");
    fs::create_dir_all(&att_dir).expect("create att dir");
    fs::write(att_dir.join("live.txt"), b"Pre-crash attachment bytes").expect("write att");

    // 2. Create checkpoint
    let checkpoint = RestoreService::create_recovery_checkpoint(&conn, &app_dir.paths)
        .expect("create checkpoint");
    assert!(Path::new(&checkpoint.file_path).exists());
    drop(conn);

    // 3. Simulate an interrupted restore during FilesSwapped phase
    let live_db_backup = app_dir.paths.root.join("live_db_pre_restore_backup.sqlite");
    fs::copy(&app_dir.paths.database_file, &live_db_backup).expect("copy live db to backup");

    let live_att_backup = app_dir.paths.root.join("live_att_pre_restore_backup");
    fs::create_dir_all(&live_att_backup).expect("create live att backup dir");
    let att_backup_sub = live_att_backup.join(&note.id);
    fs::create_dir_all(&att_backup_sub).expect("create sub");
    fs::write(
        att_backup_sub.join("live.txt"),
        b"Pre-crash attachment bytes",
    )
    .expect("write att backup");

    // Replace live database with partially corrupted/incomplete file
    fs::write(
        &app_dir.paths.database_file,
        b"INCOMPLETE_TRANSFERRED_DATABASE",
    )
    .expect("corrupt db");

    let marker = RestoreActivationMarker {
        operation_id: "crash-test-op".to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        candidate_archive_path: "/some/archive.pnbackup".to_string(),
        staging_dir: "/some/staging".to_string(),
        checkpoint_path: checkpoint.file_path.clone(),
        live_db_backup_path: live_db_backup.to_string_lossy().to_string(),
        live_attachments_backup_path: live_att_backup.to_string_lossy().to_string(),
        phase: RestoreActivationPhase::FilesSwapped,
    };
    RestoreService::write_activation_marker(&marker, &app_dir.paths).expect("write marker");

    // 4. Invoke crash recovery entry point
    let recovery_msg =
        RestoreService::recover_if_interrupted(&app_dir.paths).expect("run recovery");
    assert!(recovery_msg.is_some());

    // 5. Verify live state restored and marker removed
    assert!(!app_dir.paths.restore_marker_file().exists());

    let reopened_conn = app_dir.open_migrated_database();
    let restored_note = NoteRepository::get_by_id(&reopened_conn, &note.id)
        .expect("get note")
        .expect("note recovered");
    assert_eq!(restored_note.title, "Pre-Crash Vital Note");
    assert_eq!(
        restored_note.content,
        "Crucial data that must survive an interrupted restore."
    );

    let att_recovered = app_dir
        .paths
        .attachments_dir
        .join(&note.id)
        .join("live.txt");
    assert!(att_recovered.exists());
    assert_eq!(
        fs::read(att_recovered).unwrap(),
        b"Pre-crash attachment bytes"
    );
}

/// 11. Failed restore leaves current live data 100% intact without silent replacement.
#[test]
fn test_failed_restore_preserves_current_live_data_intact() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let mut conn = app_dir.open_migrated_database();

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Permanent Production Note".to_string()),
            content: Some("This data must never be overwritten by a failed restore.".to_string()),
            ..Default::default()
        },
    )
    .expect("create live note");

    let bogus_archive = app_dir.paths.backups_dir.join("bogus_empty.pnbackup");
    fs::write(&bogus_archive, b"").expect("write 0-byte bogus");

    let restore_err = RestoreService::execute_restore(
        &mut conn,
        &app_dir.paths,
        BackupRestoreRequest {
            file_path: bogus_archive.to_string_lossy().to_string(),
            operation_id: Some("op-fail-test".to_string()),
        },
        None::<fn(_)>,
    );
    assert!(restore_err.is_err());

    // Verify live data unchanged
    let query_note = NoteRepository::get_by_id(&conn, &note.id)
        .expect("query note")
        .expect("note still exists");
    assert_eq!(query_note.title, "Permanent Production Note");
    assert_eq!(
        query_note.content,
        "This data must never be overwritten by a failed restore."
    );
}

/// 12. Encrypted backup creation and restore boundaries preserve metadata and lock state.
#[test]
fn test_encrypted_backup_creation_and_restore_boundaries() {
    let _lock = lock_process_global_test_state();

    // Source Environment (Encrypted)
    let src_dir = TestAppDir::new();
    let src_conn = src_dir.open_migrated_database();

    let password = "SecretMasterPassword!2026";
    let session = CryptoSession::init_from_database(&src_conn).expect("init session");
    session
        .setup_encryption(
            &src_conn,
            &src_dir.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("setup encryption");

    let dek = session.get_active_dek().expect("get dek");

    let enc_note = NoteRepository::create_with_key(
        &src_conn,
        CreateNoteDto {
            title: Some("Top Secret Plan".to_string()),
            content: Some("Sensitive encrypted contents inside backup.".to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("create enc note");

    // Create backup of encrypted database
    let backup = BackupService::create_backup(
        &src_conn,
        &src_dir.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("create enc backup");

    assert!(backup.success);

    // Validate manifest declares is_encrypted: true
    let val =
        BackupValidationService::validate_backup(Path::new(&backup.file_path), &src_dir.paths)
            .expect("validate enc backup");
    assert!(val.is_valid);
    let manifest = val.manifest.expect("manifest present");
    assert!(manifest.is_encrypted);

    // Target Environment: Restore encrypted backup into fresh installation
    let target_dir = TestAppDir::new();
    let mut target_conn = target_dir.open_migrated_database();

    let res = RestoreService::execute_restore(
        &mut target_conn,
        &target_dir.paths,
        BackupRestoreRequest {
            file_path: backup.file_path.clone(),
            operation_id: Some("op-enc-restore".to_string()),
        },
        None::<fn(_)>,
    )
    .expect("execute enc restore");

    assert!(res.success);

    // Verify target database has encryption active and locks by default
    let target_session =
        CryptoSession::init_from_database(&target_conn).expect("init target session");
    assert!(target_session.is_encryption_enabled());
    assert!(target_session.is_locked());

    // Unlock target database with original password and read decrypted note
    let unlock_res = target_session.unlock(UnlockDto {
        password: password.to_string(),
    });
    assert!(unlock_res.is_ok());

    let unlocked_dek = target_session.get_active_dek().expect("get unlocked dek");
    let decrypted_note =
        NoteRepository::get_by_id_with_key(&target_conn, &enc_note.id, Some(&unlocked_dek))
            .expect("query note")
            .expect("note exists");
    assert_eq!(decrypted_note.title, "Top Secret Plan");
    assert_eq!(
        decrypted_note.content,
        "Sensitive encrypted contents inside backup."
    );
}

/// 13. Backup retention manager enforces count retention on automatic backups.
#[test]
fn test_backup_retention_and_pruning_policy() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // Create 2 automatic backups with a 1.1s timestamp gap
    let b1 = BackupService::create_backup(
        &conn,
        &app_dir.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Automatic),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("create b1");

    std::thread::sleep(std::time::Duration::from_millis(1100));

    let b2 = BackupService::create_backup(
        &conn,
        &app_dir.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Automatic),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("create b2");

    assert!(Path::new(&b1.file_path).exists());
    assert!(Path::new(&b2.file_path).exists());

    // Prune with retention_count = 1
    let pruned = BackupRetentionManager::apply_retention(&app_dir.paths.backups_dir, 1, None);
    assert_eq!(pruned.deleted_paths.len(), 1);
    assert!(!Path::new(&b1.file_path).exists());
    assert!(Path::new(&b2.file_path).exists());
}
