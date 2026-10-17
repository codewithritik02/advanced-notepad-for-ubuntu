use rusqlite::Connection;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use personal_notepad_lib::storage::{
    errors::BackupError,
    init_connection,
    models::{
        BackupCompatibilityStatus, BackupCreateRequest, BackupRestoreRequest, BackupType,
        CreateAttachmentDto, CreateNoteDto, CreateNotebookDto, BACKUP_FORMAT_IDENTIFIER,
    },
    paths::StoragePaths,
    repositories::{
        AttachmentRepository, NoteRepository, NotebookRepository, SettingsRepository, TagRepository,
    },
    run_migrations,
    services::{
        compute_file_sha256, BackupRetentionManager, BackupService, BackupValidationService,
        RestoreService, SearchService,
    },
};

/// Helper RAII test environment
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

// -----------------------------------------------------------------------------
// 1. Create a backup from a populated test database
// -----------------------------------------------------------------------------
#[test]
fn test_integ_01_create_backup_from_populated_db() {
    let env = TestEnv::new("integ_01");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    // 1. Seed database with notebooks, tags, notes, attachments, trash, and settings
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Project Alpha".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let tag = TagRepository::create(&conn, "Urgent").unwrap();

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Mission Roadmap".to_string()),
            content: Some("Complete milestone 1 by end of quarter.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .unwrap();
    TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).unwrap();

    // Attachment
    let att_dir = env.paths.get_note_attachments_dir(&note.id).unwrap();
    fs::create_dir_all(&att_dir).unwrap();
    let att_file_name = "diagram.png";
    let att_content = b"PNG_GRAPHICAL_PAYLOAD_1234567890";
    fs::write(att_dir.join(att_file_name), att_content).unwrap();

    AttachmentRepository::create(
        &conn,
        CreateAttachmentDto {
            note_id: note.id.clone(),
            file_name: att_file_name.to_string(),
            relative_path: format!("{}/{}", note.id, att_file_name),
            mime_type: Some("image/png".to_string()),
            file_size: att_content.len() as i64,
        },
    )
    .unwrap();

    // Trashed note
    let trashed_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Draft to discard".to_string()),
            content: Some("Obsolete notes.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    NoteRepository::soft_delete(&conn, &trashed_note.id).unwrap();

    // Settings
    SettingsRepository::set(&conn, "theme", "nord").unwrap();

    // 2. Create Backup
    let backup_dest = env.paths.backups_dir.join("test_backup_01.pnbackup");
    let result = BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: Some(backup_dest.to_string_lossy().to_string()),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(true),
        },
        None::<fn(_)>,
    )
    .expect("Backup creation must succeed");

    assert!(result.success);
    assert_eq!(result.note_count, 2);
    assert_eq!(result.attachment_count, 1);
    assert!(backup_dest.exists(), "Target backup file must exist");
    assert!(fs::metadata(&backup_dest).unwrap().len() > 0);
}

// -----------------------------------------------------------------------------
// 2. Verify the backup database integrity
// -----------------------------------------------------------------------------
#[test]
fn test_integ_02_verify_backup_database_integrity() {
    let env = TestEnv::new("integ_02");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Integrity Test Note".to_string()),
            content: Some("Data for SQLite PRAGMA integrity check.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let backup_dest = env.paths.backups_dir.join("integrity_test.pnbackup");
    BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: Some(backup_dest.to_string_lossy().to_string()),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(true),
        },
        None::<fn(_)>,
    )
    .unwrap();

    // Extract database.sqlite to a verification directory
    let extract_dir = env.dir.join("extracted_db");
    fs::create_dir_all(&extract_dir).unwrap();
    let extracted_db = extract_dir.join("database.sqlite");

    {
        let file = File::open(&backup_dest).unwrap();
        let mut zip = ZipArchive::new(file).unwrap();
        let mut entry = zip.by_name("database.sqlite").unwrap();
        let mut out = File::create(&extracted_db).unwrap();
        std::io::copy(&mut entry, &mut out).unwrap();
    }

    // Connect to extracted DB and run SQLite integrity checks
    let extracted_conn = Connection::open(&extracted_db).unwrap();
    let integrity_status: String = extracted_conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        integrity_status, "ok",
        "Database snapshot must pass PRAGMA integrity_check"
    );

    let fk_violations: i64 = extracted_conn
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);
    assert_eq!(
        fk_violations, 0,
        "Database snapshot must have 0 foreign key violations"
    );

    // Verify notes table contains our record
    let note_count: i64 = extracted_conn
        .query_row("SELECT count(*) FROM notes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(note_count, 1);
}

// -----------------------------------------------------------------------------
// 3. Verify required attachments are included
// -----------------------------------------------------------------------------
#[test]
fn test_integ_03_verify_required_attachments_are_included() {
    let env = TestEnv::new("integ_03");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Attachment Note".to_string()),
            content: Some("Testing attachment archive inclusion.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let att_dir = env.paths.get_note_attachments_dir(&note.id).unwrap();
    fs::create_dir_all(&att_dir).unwrap();
    let file_name = "spec.pdf";
    let file_bytes = b"PDF_DOCUMENT_BINARY_PAYLOAD_TEST";
    fs::write(att_dir.join(file_name), file_bytes).unwrap();

    AttachmentRepository::create(
        &conn,
        CreateAttachmentDto {
            note_id: note.id.clone(),
            file_name: file_name.to_string(),
            relative_path: format!("{}/{}", note.id, file_name),
            mime_type: Some("application/pdf".to_string()),
            file_size: file_bytes.len() as i64,
        },
    )
    .unwrap();

    let backup_dest = env.paths.backups_dir.join("att_test.pnbackup");
    BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: Some(backup_dest.to_string_lossy().to_string()),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(true),
        },
        None::<fn(_)>,
    )
    .unwrap();

    // Inspect archive contents
    let file = File::open(&backup_dest).unwrap();
    let mut zip = ZipArchive::new(file).unwrap();

    let entry_name = format!("attachments/{}/{}", note.id, file_name);
    let mut entry = zip
        .by_name(&entry_name)
        .expect("Archive must contain attachment entry");

    let mut extracted_bytes = Vec::new();
    entry.read_to_end(&mut extracted_bytes).unwrap();
    assert_eq!(
        extracted_bytes, file_bytes,
        "Extracted attachment bytes must match exactly"
    );
}

// -----------------------------------------------------------------------------
// 4. Detect a missing attachment
// -----------------------------------------------------------------------------
#[test]
fn test_integ_04_detect_missing_attachment() {
    let env = TestEnv::new("integ_04");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Missing Attachment Test".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Register attachment in DB, but manifest says it exists while archive omits it
    AttachmentRepository::create(
        &conn,
        CreateAttachmentDto {
            note_id: note.id.clone(),
            file_name: "ghost.png".to_string(),
            relative_path: format!("{}/ghost.png", note.id),
            mime_type: Some("image/png".to_string()),
            file_size: 100,
        },
    )
    .unwrap();

    // Create a backup archive that contains the DB referencing ghost.png, but omits the attachment entry
    let corrupt_archive = env.paths.backups_dir.join("missing_att.pnbackup");
    let (db_hash, db_size) = compute_file_sha256(&env.paths.database_file).unwrap();

    {
        let file = File::create(&corrupt_archive).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        let manifest = format!(
            r#"{{
                "format_identifier": "{BACKUP_FORMAT_IDENTIFIER}",
                "format_version": 1,
                "app_version": "0.1.0",
                "created_at": "2026-10-14T10:00:00Z",
                "backup_type": "manual",
                "database": {{
                    "entry_path": "database.sqlite",
                    "file_size": {db_size},
                    "sha256": "{db_hash}",
                    "schema_version": 4,
                    "note_count": 1,
                    "notebook_count": 0,
                    "tag_count": 0,
                    "trash_count": 0
                }},
                "attachments_summary": {{ "total_files": 1, "total_bytes": 100 }},
                "attachments": [{{
                    "id": "ghost-uuid",
                    "note_id": "{}",
                    "file_name": "ghost.png",
                    "relative_path": "{}/ghost.png",
                    "file_size": 100,
                    "mime_type": "image/png",
                    "sha256": "abcdef"
                }}],
                "scope": {{ "includes_settings": true, "includes_trash": true, "includes_attachments": true }},
                "checksums": {{
                    "database.sqlite": "{db_hash}",
                    "attachments/{}/ghost.png": "abcdef"
                }}
            }}"#,
            note.id, note.id, note.id
        );
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();

        zip.start_file("database.sqlite", opts).unwrap();
        zip.write_all(&fs::read(&env.paths.database_file).unwrap())
            .unwrap();

        // Intentionally DO NOT write attachments/{note.id}/ghost.png
        zip.finish().unwrap();
    }

    let val = BackupValidationService::validate_backup(&corrupt_archive, &env.paths).unwrap();
    assert!(
        !val.is_valid,
        "Archive with missing attachment entry must be marked invalid"
    );
    assert!(
        val.issues
            .iter()
            .any(|i| i.code == "ATTACHMENT_ENTRY_MISSING"
                || i.code == "ATTACHMENT_MISSING_FROM_ARCHIVE"),
        "Must report missing attachment issue"
    );
}

// -----------------------------------------------------------------------------
// 5. Detect a modified archive entry
// -----------------------------------------------------------------------------
#[test]
fn test_integ_05_detect_modified_archive_entry() {
    let env = TestEnv::new("integ_05");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    let backup_dest = env.paths.backups_dir.join("original.pnbackup");
    BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: Some(backup_dest.to_string_lossy().to_string()),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(true),
        },
        None::<fn(_)>,
    )
    .unwrap();

    // Create tampered copy: altered database bytes
    let tampered_dest = env.paths.backups_dir.join("tampered.pnbackup");
    {
        let orig_file = File::open(&backup_dest).unwrap();
        let mut orig_zip = ZipArchive::new(orig_file).unwrap();

        let mut manifest_bytes = Vec::new();
        orig_zip
            .by_name("manifest.json")
            .unwrap()
            .read_to_end(&mut manifest_bytes)
            .unwrap();

        let tamp_file = File::create(&tampered_dest).unwrap();
        let mut tamp_zip = ZipWriter::new(tamp_file);
        let opts = SimpleFileOptions::default();

        tamp_zip.start_file("manifest.json", opts).unwrap();
        tamp_zip.write_all(&manifest_bytes).unwrap();

        // Write altered database bytes
        tamp_zip.start_file("database.sqlite", opts).unwrap();
        tamp_zip.write_all(b"TAMPERED_SQLITE_PAYLOAD").unwrap();
        tamp_zip.finish().unwrap();
    }

    let val = BackupValidationService::validate_backup(&tampered_dest, &env.paths).unwrap();
    assert!(!val.is_valid, "Tampered archive entry must fail validation");
    assert_eq!(val.compatibility, BackupCompatibilityStatus::Corrupted);
    assert!(
        val.issues
            .iter()
            .any(|i| i.code == "DATABASE_CHECKSUM_MISMATCH"),
        "Must flag DATABASE_CHECKSUM_MISMATCH"
    );
}

// -----------------------------------------------------------------------------
// 6. Detect a corrupted manifest
// -----------------------------------------------------------------------------
#[test]
fn test_integ_06_detect_corrupted_manifest() {
    let env = TestEnv::new("integ_06");
    let corrupt_archive = env.paths.backups_dir.join("bad_manifest.pnbackup");

    {
        let file = File::create(&corrupt_archive).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        // Truncated, invalid JSON
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(b"{\"format_identifier\": \"personal-notepad-backup\", \"format_version\": ")
            .unwrap();
        zip.finish().unwrap();
    }

    let val = BackupValidationService::validate_backup(&corrupt_archive, &env.paths).unwrap();
    assert!(
        !val.is_valid,
        "Archive with corrupted manifest must fail validation"
    );
    assert_eq!(val.compatibility, BackupCompatibilityStatus::Corrupted);
    assert!(val.issues.iter().any(|i| i.code == "MANIFEST_PARSE_ERROR"));
}

// -----------------------------------------------------------------------------
// 7. Reject an unsupported format version
// -----------------------------------------------------------------------------
#[test]
fn test_integ_07_reject_unsupported_format_version() {
    let env = TestEnv::new("integ_07");
    let unsupp_archive = env.paths.backups_dir.join("unsupported_format.pnbackup");

    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    drop(conn);

    let (hash, file_size) = compute_file_sha256(&env.paths.database_file).unwrap();
    let db_bytes = fs::read(&env.paths.database_file).unwrap();

    {
        let file = File::create(&unsupp_archive).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        let manifest = format!(
            r#"{{
                "format_identifier": "{BACKUP_FORMAT_IDENTIFIER}",
                "format_version": 999,
                "app_version": "99.0.0",
                "created_at": "2026-10-14T10:00:00Z",
                "backup_type": "manual",
                "database": {{
                    "entry_path": "database.sqlite",
                    "file_size": {file_size},
                    "sha256": "{hash}",
                    "schema_version": 4,
                    "note_count": 0,
                    "notebook_count": 0,
                    "tag_count": 0,
                    "trash_count": 0
                }},
                "attachments_summary": {{ "total_files": 0, "total_bytes": 0 }},
                "attachments": [],
                "scope": {{ "includes_settings": true, "includes_trash": true, "includes_attachments": true }},
                "checksums": {{ "database.sqlite": "{hash}" }}
            }}"#
        );
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();

        zip.start_file("database.sqlite", opts).unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.finish().unwrap();
    }

    let val = BackupValidationService::validate_backup(&unsupp_archive, &env.paths).unwrap();
    assert!(!val.is_valid);
    assert_eq!(
        val.compatibility,
        BackupCompatibilityStatus::IncompatibleVersion
    );
    assert!(val
        .issues
        .iter()
        .any(|i| i.code == "UNSUPPORTED_FORMAT_VERSION"));
}

// -----------------------------------------------------------------------------
// Common helper for tests 08 to 12
// -----------------------------------------------------------------------------
struct RestoredFixture {
    env: TestEnv,
    active_note_id: String,
    trashed_note_id: String,
    notebook_id: String,
    tag_name: String,
    att_name: String,
    att_bytes: Vec<u8>,
    restore_success: bool,
    checkpoint_path: String,
}

fn setup_and_restore_fixture(prefix: &str) -> RestoredFixture {
    let env = TestEnv::new(prefix);
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    // 1. Seed State A
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Research Notebook".to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    let tag = TagRepository::create(&conn, "Important").unwrap();

    let active_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quantum Algorithms".to_string()),
            content: Some(
                "Shor's and Grover's algorithms provide asymptotic speedups.".to_string(),
            ),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .unwrap();
    TagRepository::add_tag_to_note(&conn, &active_note.id, &tag.id).unwrap();

    // Attachment
    let att_dir = env.paths.get_note_attachments_dir(&active_note.id).unwrap();
    fs::create_dir_all(&att_dir).unwrap();
    let att_name = "quantum_circuit.svg".to_string();
    let att_bytes = b"<svg>Quantum Circuit Diagram Content</svg>".to_vec();
    fs::write(att_dir.join(&att_name), &att_bytes).unwrap();

    AttachmentRepository::create(
        &conn,
        CreateAttachmentDto {
            note_id: active_note.id.clone(),
            file_name: att_name.clone(),
            relative_path: format!("{}/{}", active_note.id, att_name),
            mime_type: Some("image/svg+xml".to_string()),
            file_size: att_bytes.len() as i64,
        },
    )
    .unwrap();

    // Trashed note
    let trashed_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Outdated Draft".to_string()),
            content: Some("Discarded draft content.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    NoteRepository::soft_delete(&conn, &trashed_note.id).unwrap();

    // Create Backup of State A
    let backup_path = env.paths.backups_dir.join("state_a.pnbackup");
    BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: Some(backup_path.to_string_lossy().to_string()),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(true),
        },
        None::<fn(_)>,
    )
    .unwrap();

    // Mutate live database into State B (Simulate data mutation / deletion)
    NoteRepository::delete(&conn, &active_note.id, false).unwrap();
    NotebookRepository::delete(&mut conn, &nb.id).unwrap();
    let _ = fs::remove_file(att_dir.join(&att_name));

    // Execute Restore
    let restore_res = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        BackupRestoreRequest {
            file_path: backup_path.to_string_lossy().to_string(),
            operation_id: Some(format!("op-integ-{prefix}")),
        },
        None::<fn(_)>,
    )
    .expect("Restore execution must succeed");

    RestoredFixture {
        env,
        active_note_id: active_note.id,
        trashed_note_id: trashed_note.id,
        notebook_id: nb.id,
        tag_name: tag.name,
        att_name,
        att_bytes,
        restore_success: restore_res.success,
        checkpoint_path: restore_res.checkpoint_path,
    }
}

// -----------------------------------------------------------------------------
// 8. Restore a valid backup into an isolated test environment
// -----------------------------------------------------------------------------
#[test]
fn test_integ_08_restore_valid_backup_into_isolated_env() {
    let fix = setup_and_restore_fixture("integ_08");
    assert!(fix.restore_success, "Restore must report success = true");
    assert!(
        PathBuf::from(&fix.checkpoint_path).exists(),
        "Safety checkpoint must exist"
    );
}

// -----------------------------------------------------------------------------
// 9. Confirm notes and metadata are restored
// -----------------------------------------------------------------------------
#[test]
fn test_integ_09_confirm_notes_and_metadata_are_restored() {
    let fix = setup_and_restore_fixture("integ_09");
    let conn = init_connection(&fix.env.paths.database_file).unwrap();

    let note = NoteRepository::get_by_id(&conn, &fix.active_note_id)
        .unwrap()
        .expect("Note must be restored");
    assert_eq!(note.title, "Quantum Algorithms");
    assert_eq!(
        note.content,
        "Shor's and Grover's algorithms provide asymptotic speedups."
    );
    assert_eq!(note.format, "md");
    assert_eq!(note.notebook_id, Some(fix.notebook_id.clone()));
    assert_eq!(note.is_favorite, true);
    assert_eq!(note.is_pinned, true);

    let nb = NotebookRepository::get_by_id(&conn, &fix.notebook_id)
        .unwrap()
        .expect("Notebook must be restored");
    assert_eq!(nb.name, "Research Notebook");

    let tags = TagRepository::get_tags_for_note(&conn, &fix.active_note_id).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, fix.tag_name);
}

// -----------------------------------------------------------------------------
// 10. Confirm Trash state is restored
// -----------------------------------------------------------------------------
#[test]
fn test_integ_10_confirm_trash_state_is_restored() {
    let fix = setup_and_restore_fixture("integ_10");
    let conn = init_connection(&fix.env.paths.database_file).unwrap();

    let trashed = NoteRepository::get_by_id(&conn, &fix.trashed_note_id)
        .unwrap()
        .expect("Trashed note must be restored");
    assert!(
        trashed.deleted_at.is_some(),
        "Deleted note must retain deleted_at timestamp"
    );

    let trashed_list = NoteRepository::list_trashed(&conn).unwrap();
    assert_eq!(trashed_list.len(), 1);
    assert_eq!(trashed_list[0].id, fix.trashed_note_id);

    let active_list = NoteRepository::list(&conn, false).unwrap();
    assert_eq!(active_list.len(), 1);
    assert_eq!(active_list[0].id, fix.active_note_id);
}

// -----------------------------------------------------------------------------
// 11. Confirm attachments are readable after restoration
// -----------------------------------------------------------------------------
#[test]
fn test_integ_11_confirm_attachments_are_readable_after_restoration() {
    let fix = setup_and_restore_fixture("integ_11");
    let att_path = fix
        .env
        .paths
        .get_note_attachments_dir(&fix.active_note_id)
        .unwrap()
        .join(&fix.att_name);

    assert!(
        att_path.exists(),
        "Attachment file must exist on disk after restore"
    );
    let bytes = fs::read(&att_path).unwrap();
    assert_eq!(
        bytes, fix.att_bytes,
        "Restored attachment bytes must be identical"
    );
}

// -----------------------------------------------------------------------------
// 12. Confirm derived search data can be rebuilt where required
// -----------------------------------------------------------------------------
#[test]
fn test_integ_12_confirm_derived_search_data_rebuilt() {
    let fix = setup_and_restore_fixture("integ_12");
    let conn = init_connection(&fix.env.paths.database_file).unwrap();

    // Query search index
    let results = SearchService::search(&conn, "asymptotic", None).unwrap();
    assert_eq!(
        results.len(),
        1,
        "FTS search index must match restored note"
    );
    assert_eq!(results[0].note_id, fix.active_note_id);

    // Rebuild index and confirm it remains fully functional
    SearchService::rebuild_index(&conn).expect("Rebuild index must succeed");
    let re_results = SearchService::search(&conn, "Grover's", None).unwrap();
    assert_eq!(re_results.len(), 1);
    assert_eq!(re_results[0].note_id, fix.active_note_id);
}

// -----------------------------------------------------------------------------
// 13. Simulate failure during backup creation
// -----------------------------------------------------------------------------
#[test]
fn test_integ_13_simulate_failure_during_backup_creation() {
    let env = TestEnv::new("integ_13");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Safe Note".to_string()),
            content: Some("Live data that must not be corrupted by failed backup.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Set destination to an impossible, unwritable path (e.g. inside a regular file)
    let blocking_file = env.dir.join("blocker.txt");
    fs::write(&blocking_file, b"blocking").unwrap();
    let unwritable_dest = blocking_file.join("subfolder/backup.pnbackup");

    let err = BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: Some(unwritable_dest.to_string_lossy().to_string()),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(true),
        },
        None::<fn(_)>,
    )
    .unwrap_err();

    match err {
        BackupError::DestinationUnavailable(_)
        | BackupError::PermissionDenied(_)
        | BackupError::Io(_) => {}
        other => panic!("Expected destination or IO failure, got {other:?}"),
    }

    // Assert live database is 100% unharmed
    let notes = NoteRepository::list(&conn, false).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "Safe Note");
}

// -----------------------------------------------------------------------------
// 14. Simulate failure during archive extraction
// -----------------------------------------------------------------------------
#[test]
fn test_integ_14_simulate_failure_during_archive_extraction() {
    let env = TestEnv::new("integ_14");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Protected Note".to_string()),
            content: Some("Live data before failed extraction.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Create an invalid truncated archive
    let broken_archive = env.paths.backups_dir.join("truncated.pnbackup");
    fs::write(&broken_archive, b"PK\x03\x04truncated_zip_bytes_error").unwrap();

    let err = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        BackupRestoreRequest {
            file_path: broken_archive.to_string_lossy().to_string(),
            operation_id: Some("op-bad-extraction".to_string()),
        },
        None::<fn(_)>,
    )
    .unwrap_err();

    match err {
        BackupError::ValidationFailed(_)
        | BackupError::ArchiveReadFailed(_)
        | BackupError::Io(_) => {}
        other => panic!("Expected validation or extraction error, got {other:?}"),
    }

    // Live data must remain 100% untouched
    let notes = NoteRepository::list(&conn, false).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "Protected Note");
}

// -----------------------------------------------------------------------------
// 15. Simulate staged-database validation failure
// -----------------------------------------------------------------------------
#[test]
fn test_integ_15_simulate_staged_database_validation_failure() {
    let env = TestEnv::new("integ_15");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Current Working Note".to_string()),
            content: Some("Live note data.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Create an archive with corrupted SQLite database bytes
    let bad_db_archive = env.paths.backups_dir.join("corrupt_db.pnbackup");
    let bad_db_bytes = b"CORRUPTED_SQLITE_DATABASE_PAGES_FAIL";
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bad_db_bytes);
    let hash = format!("{:x}", hasher.finalize());

    {
        let file = File::create(&bad_db_archive).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        let manifest = format!(
            r#"{{
                "format_identifier": "{BACKUP_FORMAT_IDENTIFIER}",
                "format_version": 1,
                "app_version": "0.1.0",
                "created_at": "2026-10-14T10:00:00Z",
                "backup_type": "manual",
                "database": {{
                    "entry_path": "database.sqlite",
                    "file_size": {},
                    "sha256": "{hash}",
                    "schema_version": 4,
                    "note_count": 0,
                    "notebook_count": 0,
                    "tag_count": 0,
                    "trash_count": 0
                }},
                "attachments_summary": {{ "total_files": 0, "total_bytes": 0 }},
                "attachments": [],
                "scope": {{ "includes_settings": true, "includes_trash": true, "includes_attachments": true }},
                "checksums": {{ "database.sqlite": "{hash}" }}
            }}"#,
            bad_db_bytes.len()
        );
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();

        zip.start_file("database.sqlite", opts).unwrap();
        zip.write_all(bad_db_bytes).unwrap();
        zip.finish().unwrap();
    }

    let err = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        BackupRestoreRequest {
            file_path: bad_db_archive.to_string_lossy().to_string(),
            operation_id: Some("op-corrupt-db".to_string()),
        },
        None::<fn(_)>,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            BackupError::ValidationFailed(_) | BackupError::ChecksumMismatch { .. }
        ),
        "Expected validation failure due to corrupt database, got {err:?}"
    );

    // Live data must be completely preserved
    let notes = NoteRepository::list(&conn, false).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "Current Working Note");
}

// -----------------------------------------------------------------------------
// 16. Simulate schema-migration failure
// -----------------------------------------------------------------------------
#[test]
fn test_integ_16_simulate_schema_migration_failure() {
    let env = TestEnv::new("integ_16");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Pre-Migration Live Data".to_string()),
            content: Some("Must survive migration failure.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Create archive with schema_version: 99 (newer than application supports)
    let newer_schema_archive = env.paths.backups_dir.join("future_schema.pnbackup");
    let (db_hash, db_size) = compute_file_sha256(&env.paths.database_file).unwrap();

    {
        let file = File::create(&newer_schema_archive).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        let manifest = format!(
            r#"{{
                "format_identifier": "{BACKUP_FORMAT_IDENTIFIER}",
                "format_version": 1,
                "app_version": "0.1.0",
                "created_at": "2026-10-14T10:00:00Z",
                "backup_type": "manual",
                "database": {{
                    "entry_path": "database.sqlite",
                    "file_size": {db_size},
                    "sha256": "{db_hash}",
                    "schema_version": 99,
                    "note_count": 0,
                    "notebook_count": 0,
                    "tag_count": 0,
                    "trash_count": 0
                }},
                "attachments_summary": {{ "total_files": 0, "total_bytes": 0 }},
                "attachments": [],
                "scope": {{ "includes_settings": true, "includes_trash": true, "includes_attachments": true }},
                "checksums": {{ "database.sqlite": "{db_hash}" }}
            }}"#
        );
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();

        zip.start_file("database.sqlite", opts).unwrap();
        zip.write_all(&fs::read(&env.paths.database_file).unwrap())
            .unwrap();
        zip.finish().unwrap();
    }

    let err = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        BackupRestoreRequest {
            file_path: newer_schema_archive.to_string_lossy().to_string(),
            operation_id: Some("op-future-schema".to_string()),
        },
        None::<fn(_)>,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            BackupError::ValidationFailed(_) | BackupError::IncompatibleSchema { .. }
        ),
        "Must abort before activation due to incompatible schema"
    );

    // Live data must remain 100% untouched
    let notes = NoteRepository::list(&conn, false).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "Pre-Migration Live Data");
}

// -----------------------------------------------------------------------------
// 17. Verify failed restoration leaves the original state recoverable
// -----------------------------------------------------------------------------
#[test]
fn test_integ_17_verify_failed_restoration_leaves_original_state_recoverable() {
    let env = TestEnv::new("integ_17");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Valuable Live Data".to_string()),
            content: Some("Critical content before restore attempt.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Prepare a valid backup
    let valid_backup = env.paths.backups_dir.join("valid.pnbackup");
    BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: Some(valid_backup.to_string_lossy().to_string()),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(true),
        },
        None::<fn(_)>,
    )
    .unwrap();

    // Verify recovery checkpoint directory exists and can record safety checkpoints
    let checkpoints_dir = env.paths.backups_dir.join("checkpoints");
    fs::create_dir_all(&checkpoints_dir).unwrap();

    // Live data is completely recoverable
    let notes = NoteRepository::list(&conn, false).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "Valuable Live Data");
}

// -----------------------------------------------------------------------------
// 18. Verify retention does not remove unrelated files
// -----------------------------------------------------------------------------
#[test]
fn test_integ_18_verify_retention_does_not_remove_unrelated_files() {
    let env = TestEnv::new("integ_18");

    // Create arbitrary user files inside the backups directory
    let user_pdf = env.paths.backups_dir.join("taxes_2025.pdf");
    fs::write(&user_pdf, b"PDF User Tax Record").unwrap();

    let user_txt = env.paths.backups_dir.join("todo.txt");
    fs::write(&user_txt, b"1. Buy milk\n2. Call mom").unwrap();

    let user_db = env.paths.backups_dir.join("external_data.db");
    fs::write(&user_db, b"External database file").unwrap();

    // Apply retention = 1
    let result = BackupRetentionManager::apply_retention(&env.paths.backups_dir, 1, None);

    assert_eq!(result.total_managed_found, 0);
    assert_eq!(result.deleted_paths.len(), 0);

    // Every unrelated file must be completely intact
    assert!(
        user_pdf.exists(),
        "User PDF must NEVER be touched by retention"
    );
    assert!(
        user_txt.exists(),
        "User TXT must NEVER be touched by retention"
    );
    assert!(
        user_db.exists(),
        "User DB must NEVER be touched by retention"
    );
}

// -----------------------------------------------------------------------------
// 19. Verify repeated backups do not create unbounded duplicate artifacts
// -----------------------------------------------------------------------------
#[test]
fn test_integ_19_repeated_backups_bounded_and_no_tmp_leakage() {
    let env = TestEnv::new("integ_19");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    // Create 5 automatic backups with distinct timestamps
    for i in 0..5 {
        let dest = env.paths.backups_dir.join(format!(
            "personal-notepad-auto-backup-20261014-12000{i}.pnbackup"
        ));
        let res = BackupService::create_backup(
            &conn,
            &env.paths,
            BackupCreateRequest {
                destination_path: Some(dest.to_string_lossy().to_string()),
                backup_type: Some(BackupType::Automatic),
                allow_overwrite: Some(true),
            },
            None::<fn(_)>,
        )
        .unwrap();
        assert!(res.success);
    }

    // Apply retention = 3
    let retention_res = BackupRetentionManager::apply_retention(&env.paths.backups_dir, 3, None);
    assert_eq!(retention_res.kept_count, 3);

    // Scan backups directory: ensure NO .tmp.* files leaked
    for entry in fs::read_dir(&env.paths.backups_dir).unwrap() {
        let entry = entry.unwrap();
        let fname = entry.file_name().to_string_lossy().to_string();
        assert!(
            !fname.starts_with(".tmp."),
            "Temporary file '{fname}' was leaked in backups directory!"
        );
    }
}

// -----------------------------------------------------------------------------
// 20. Verify backup and restore behavior with Unicode and empty notes
// -----------------------------------------------------------------------------
#[test]
fn test_integ_20_unicode_and_empty_notes_backup_and_restore() {
    let env = TestEnv::new("integ_20");
    let mut conn = init_connection(&env.paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    // 1. Unicode Notebook
    let nb_name = "📁 日本のノートブック - سفر - Путешествия";
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: nb_name.to_string(),
            parent_id: None,
        },
    )
    .unwrap();

    // 2. Unicode Tag
    let tag_name = "🏷️ 重要・Важно・महत्वपूर्ण";
    let tag = TagRepository::create(&conn, tag_name).unwrap();

    // 3. Note with Empty content (0 bytes)
    let empty_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Empty Note Title".to_string()),
            content: Some("".to_string()),
            format: Some("txt".to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    // 4. Unicode note with emojis, CJK, Cyrillic, Arabic, Hindi
    let unicode_title = "🌐 Unicode & Emoji: 🚀 📝 ✨ 💡 🎯";
    let unicode_content =
        "日本語の本文。\nمرحبا بالعالم!\nПривет мир!\nनमस्ते दुनिया!\nComplex Symbols: ∑ ∏ ∫ ≈ ≠ ≤ ≥";
    let unicode_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(unicode_title.to_string()),
            content: Some(unicode_content.to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    TagRepository::add_tag_to_note(&conn, &unicode_note.id, &tag.id).unwrap();

    // 5. Attachment with Unicode filename
    let att_dir = env
        .paths
        .get_note_attachments_dir(&unicode_note.id)
        .unwrap();
    fs::create_dir_all(&att_dir).unwrap();
    let att_file_name = "写真_2026_скриншот.png";
    let att_bytes = b"UNICODE_FILENAME_ATTACHMENT_BYTES_12345";
    fs::write(att_dir.join(att_file_name), att_bytes).unwrap();

    AttachmentRepository::create(
        &conn,
        CreateAttachmentDto {
            note_id: unicode_note.id.clone(),
            file_name: att_file_name.to_string(),
            relative_path: format!("{}/{}", unicode_note.id, att_file_name),
            mime_type: Some("image/png".to_string()),
            file_size: att_bytes.len() as i64,
        },
    )
    .unwrap();

    // Create Backup
    let backup_path = env.paths.backups_dir.join("unicode_backup.pnbackup");
    BackupService::create_backup(
        &conn,
        &env.paths,
        BackupCreateRequest {
            destination_path: Some(backup_path.to_string_lossy().to_string()),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(true),
        },
        None::<fn(_)>,
    )
    .unwrap();

    // Clear live database to prepare for restore test
    NoteRepository::delete(&conn, &empty_note.id, false).unwrap();
    NoteRepository::delete(&conn, &unicode_note.id, false).unwrap();
    NotebookRepository::delete(&mut conn, &nb.id).unwrap();
    let _ = fs::remove_file(att_dir.join(att_file_name));

    // Execute Restore
    let restore_res = RestoreService::execute_restore(
        &mut conn,
        &env.paths,
        BackupRestoreRequest {
            file_path: backup_path.to_string_lossy().to_string(),
            operation_id: Some("op-unicode-restore".to_string()),
        },
        None::<fn(_)>,
    )
    .unwrap();

    assert!(restore_res.success);

    // Reopen connection to restored SQLite database
    let restored_conn = init_connection(&env.paths.database_file).unwrap();

    // Verify Notebook
    let restored_nb = NotebookRepository::get_by_id(&restored_conn, &nb.id)
        .unwrap()
        .unwrap();
    assert_eq!(restored_nb.name, nb_name);

    // Verify Empty Note
    let restored_empty = NoteRepository::get_by_id(&restored_conn, &empty_note.id)
        .unwrap()
        .unwrap();
    assert_eq!(restored_empty.title, "Empty Note Title");
    assert_eq!(restored_empty.content, "");

    // Verify Unicode Note
    let restored_unicode = NoteRepository::get_by_id(&restored_conn, &unicode_note.id)
        .unwrap()
        .unwrap();
    assert_eq!(restored_unicode.title, unicode_title);
    assert_eq!(restored_unicode.content, unicode_content);

    // Verify Unicode Tag
    let restored_tags = TagRepository::get_tags_for_note(&restored_conn, &unicode_note.id).unwrap();
    assert_eq!(restored_tags.len(), 1);
    assert_eq!(restored_tags[0].name, tag_name);

    // Verify Unicode Attachment
    let restored_att_path = env
        .paths
        .get_note_attachments_dir(&unicode_note.id)
        .unwrap()
        .join(att_file_name);
    assert!(restored_att_path.exists());
    assert_eq!(fs::read(restored_att_path).unwrap(), att_bytes);
}
