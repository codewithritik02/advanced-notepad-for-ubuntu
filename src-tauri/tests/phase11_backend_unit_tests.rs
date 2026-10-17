use chrono::Utc;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use personal_notepad_lib::storage::{
    errors::BackupError,
    init_connection,
    models::{
        BackupAttachmentsSummary, BackupCompatibilityStatus, BackupDatabaseInfo, BackupManifest,
        BackupScopeDeclaration, BackupSettings, BackupType, BACKUP_FORMAT_IDENTIFIER,
        CURRENT_BACKUP_FORMAT_VERSION, PRIMARY_BACKUP_EXTENSION,
    },
    paths::StoragePaths,
    run_migrations,
    services::{
        compute_file_sha256, generate_default_backup_filename, sanitize_backup_filename_stem,
        BackupRetentionManager, BackupSettingsService, ManifestManager, CURRENT_APP_SCHEMA_VERSION,
        MAX_ARCHIVE_ENTRY_COUNT, MAX_BACKUP_INTERVAL_MINUTES, MAX_BACKUP_RETENTION_COUNT,
        MAX_TOTAL_UNCOMPRESSED_BYTES, MIN_BACKUP_INTERVAL_MINUTES, MIN_BACKUP_RETENTION_COUNT,
    },
};

/// Helper RAII directory guard
struct TempTestDir(PathBuf);
impl TempTestDir {
    fn new(prefix: &str) -> Self {
        let p = std::env::temp_dir().join(format!("{}_{}", prefix, Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempTestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// -----------------------------------------------------------------------------
// 1. Manifest Serialization and Parsing
// -----------------------------------------------------------------------------
#[test]
fn test_unit_1_manifest_serialization_and_parsing() {
    let mut checksums = HashMap::new();
    checksums.insert("database.sqlite".to_string(), "abc123sha256".to_string());
    checksums.insert(
        "attachments/note-1/photo.png".to_string(),
        "def456sha256".to_string(),
    );

    let original_manifest = BackupManifest {
        format_identifier: BACKUP_FORMAT_IDENTIFIER.to_string(),
        format_version: CURRENT_BACKUP_FORMAT_VERSION,
        app_version: "0.1.0".to_string(),
        created_at: Utc::now().to_rfc3339(),
        backup_type: BackupType::Manual,
        database: BackupDatabaseInfo {
            entry_path: "database.sqlite".to_string(),
            file_size: 40960,
            sha256: "abc123sha256".to_string(),
            schema_version: CURRENT_APP_SCHEMA_VERSION,
            note_count: 12,
            notebook_count: 3,
            tag_count: 5,
            trash_count: 2,
        },
        attachments_summary: BackupAttachmentsSummary {
            total_files: 1,
            total_bytes: 1048576,
        },
        attachments: vec![
            personal_notepad_lib::storage::models::BackupAttachmentItem {
                id: "att-uuid-1".to_string(),
                note_id: "note-uuid-1".to_string(),
                file_name: "photo.png".to_string(),
                relative_path: "note-uuid-1/photo.png".to_string(),
                file_size: 1048576,
                mime_type: Some("image/png".to_string()),
                sha256: "def456sha256".to_string(),
            },
        ],
        scope: BackupScopeDeclaration {
            includes_settings: true,
            includes_trash: true,
            includes_attachments: true,
        },
        checksums,
        is_encrypted: false,
        encryption_algorithm: None,
    };

    // Serialize to JSON string and bytes
    let serialized_json = serde_json::to_string_pretty(&original_manifest)
        .expect("Manifest serialization must succeed");
    assert!(serialized_json.contains(BACKUP_FORMAT_IDENTIFIER));
    assert!(serialized_json.contains("database.sqlite"));

    // Parse back from JSON
    let parsed_manifest: BackupManifest =
        serde_json::from_str(&serialized_json).expect("Manifest parsing must succeed");

    assert_eq!(
        parsed_manifest.format_identifier,
        original_manifest.format_identifier
    );
    assert_eq!(
        parsed_manifest.format_version,
        original_manifest.format_version
    );
    assert_eq!(parsed_manifest.app_version, original_manifest.app_version);
    assert_eq!(parsed_manifest.backup_type, BackupType::Manual);
    assert_eq!(parsed_manifest.database.file_size, 40960);
    assert_eq!(
        parsed_manifest.database.schema_version,
        CURRENT_APP_SCHEMA_VERSION
    );
    assert_eq!(parsed_manifest.attachments.len(), 1);
    assert_eq!(parsed_manifest.attachments[0].file_name, "photo.png");
    assert_eq!(parsed_manifest.checksums.len(), 2);

    // Test malformed / invalid manifest JSON rejected
    let malformed_json =
        r#"{"format_identifier": "personal-notepad-backup", "format_version": "NOT_AN_INTEGER"}"#;
    assert!(serde_json::from_str::<BackupManifest>(malformed_json).is_err());
}

// -----------------------------------------------------------------------------
// 2. Backup Format-Version Validation
// -----------------------------------------------------------------------------
#[test]
fn test_unit_2_backup_format_version_validation() {
    let temp = TempTestDir::new("unit_format_ver");
    let archive_path = temp.path().join("format_test.pnbackup");

    let create_archive_with_format = |ident: &str, ver: u32| {
        let file = File::create(&archive_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        let db_payload = b"test";
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(db_payload);
        let db_hash = format!("{:x}", hasher.finalize());

        let manifest = format!(
            r#"{{
                "format_identifier": "{ident}",
                "format_version": {ver},
                "app_version": "0.1.0",
                "created_at": "2026-10-14T10:00:00Z",
                "backup_type": "manual",
                "database": {{
                    "entry_path": "database.sqlite",
                    "file_size": {},
                    "sha256": "{db_hash}",
                    "schema_version": 4,
                    "note_count": 0,
                    "notebook_count": 0,
                    "tag_count": 0,
                    "trash_count": 0
                }},
                "attachments_summary": {{ "total_files": 0, "total_bytes": 0 }},
                "attachments": [],
                "scope": {{ "includes_settings": true, "includes_trash": true, "includes_attachments": true }},
                "checksums": {{ "database.sqlite": "{db_hash}" }}
            }}"#,
            db_payload.len()
        );
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();

        zip.start_file("database.sqlite", opts).unwrap();
        zip.write_all(db_payload).unwrap();

        zip.finish().unwrap();
    };

    // Valid format identifier and version 1
    create_archive_with_format(BACKUP_FORMAT_IDENTIFIER, CURRENT_BACKUP_FORMAT_VERSION);
    let valid_res = ManifestManager::validate_archive(&archive_path).unwrap();
    assert!(valid_res.is_valid);
    assert_eq!(
        valid_res.compatibility,
        BackupCompatibilityStatus::Compatible
    );

    // Invalid format identifier
    create_archive_with_format("some-foreign-backup-format", 1);
    let bad_ident_res = ManifestManager::validate_archive(&archive_path).unwrap();
    assert!(!bad_ident_res.is_valid);
    assert_eq!(
        bad_ident_res.compatibility,
        BackupCompatibilityStatus::Corrupted
    );
    assert!(bad_ident_res
        .issues
        .iter()
        .any(|i| i.code == "INVALID_FORMAT_IDENTIFIER"));

    // Future unsupported format version (e.g. 99)
    create_archive_with_format(BACKUP_FORMAT_IDENTIFIER, 99);
    let bad_ver_res = ManifestManager::validate_archive(&archive_path).unwrap();
    assert!(!bad_ver_res.is_valid);
    assert_eq!(
        bad_ver_res.compatibility,
        BackupCompatibilityStatus::IncompatibleVersion
    );
    assert!(bad_ver_res
        .issues
        .iter()
        .any(|i| i.code == "UNSUPPORTED_FORMAT_VERSION"));
}

// -----------------------------------------------------------------------------
// 3. Entry Path Validation (Zip-Slip Defense)
// -----------------------------------------------------------------------------
#[test]
fn test_unit_3_entry_path_validation_and_zip_slip_defense() {
    let temp = TempTestDir::new("unit_zip_slip");
    let archive_path = temp.path().join("unsafe_archive.pnbackup");

    let unsafe_paths = [
        "../traversal.txt",
        "../../etc/shadow",
        "attachments/../../escaped.png",
        "/absolute/root/file.txt",
        "\\windows\\path.txt",
        "C:drive_letter.txt",
    ];

    for unsafe_path in unsafe_paths {
        let file = File::create(&archive_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(b"{}").unwrap();

        zip.start_file(unsafe_path, opts).unwrap();
        zip.write_all(b"malicious payload").unwrap();

        zip.finish().unwrap();

        let res = ManifestManager::validate_archive(&archive_path).unwrap();
        assert!(
            !res.is_valid,
            "Archive with path '{unsafe_path}' must fail validation"
        );
        assert!(
            res.issues.iter().any(|i| i.code == "UNSAFE_ARCHIVE_PATH"),
            "Expected UNSAFE_ARCHIVE_PATH for '{unsafe_path}'"
        );
    }

    // Duplicate entry defense at archive construction
    {
        let file = File::create(&archive_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        let first = zip.start_file("database.sqlite", opts);
        assert!(first.is_ok());
        let second = zip.start_file("database.sqlite", opts);
        assert!(
            second.is_err(),
            "ZipWriter enforces duplicate filename rejection at creation"
        );
    }
}

// -----------------------------------------------------------------------------
// 4. Checksum Generation and Verification
// -----------------------------------------------------------------------------
#[test]
fn test_unit_4_checksum_generation_and_verification() {
    let temp = TempTestDir::new("unit_checksum");
    let test_file = temp.path().join("data.bin");

    // Write known payload
    let payload = b"Personal Notepad Cryptographic Verification Test Bytes";
    fs::write(&test_file, payload).unwrap();

    let (computed_sha256, file_size) =
        compute_file_sha256(&test_file).expect("Compute sha256 failed");
    assert_eq!(file_size, payload.len() as u64);

    // Verify sha256 output matches standard sha2 crate directly
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let expected_hash = format!("{:x}", hasher.finalize());

    assert_eq!(computed_sha256, expected_hash);

    // 1-bit / 1-byte alteration causes checksum mismatch
    let mut altered = payload.to_vec();
    altered[0] ^= 0x01; // flip 1 bit
    let altered_file = temp.path().join("altered.bin");
    fs::write(&altered_file, &altered).unwrap();

    let (altered_sha256, _) = compute_file_sha256(&altered_file).unwrap();
    assert_ne!(computed_sha256, altered_sha256);
}

// -----------------------------------------------------------------------------
// 5. Filename Generation and Sanitization
// -----------------------------------------------------------------------------
#[test]
fn test_unit_5_filename_generation_and_sanitization() {
    // 1. Generation patterns
    let manual_name = generate_default_backup_filename(BackupType::Manual);
    assert!(manual_name.starts_with("personal-notepad-backup-"));
    assert!(manual_name.ends_with(&format!(".{PRIMARY_BACKUP_EXTENSION}")));

    let auto_name = generate_default_backup_filename(BackupType::Automatic);
    assert!(auto_name.starts_with("personal-notepad-auto-backup-"));
    assert!(auto_name.ends_with(&format!(".{PRIMARY_BACKUP_EXTENSION}")));

    let checkpoint_name = generate_default_backup_filename(BackupType::Checkpoint);
    assert!(checkpoint_name.starts_with("personal-notepad-checkpoint-"));
    assert!(checkpoint_name.ends_with(&format!(".{PRIMARY_BACKUP_EXTENSION}")));

    // 2. Filename Stem Sanitization
    assert_eq!(
        sanitize_backup_filename_stem("My Safe Backup 2026"),
        "My Safe Backup 2026"
    );

    // Directory traversal sequences and illegal punctuation stripped
    assert_eq!(
        sanitize_backup_filename_stem("../../bad:name?*<>|/\\"),
        "badname"
    );

    // Empty or whitespace falls back to default fallback stem
    assert_eq!(
        sanitize_backup_filename_stem("   "),
        "personal-notepad-backup"
    );
    assert_eq!(
        sanitize_backup_filename_stem("..."),
        "personal-notepad-backup"
    );

    // Length limit enforcement (120 characters max)
    let very_long = "a".repeat(150);
    let sanitized_long = sanitize_backup_filename_stem(&very_long);
    assert_eq!(sanitized_long.len(), 120);
}

// -----------------------------------------------------------------------------
// 6. Retention Selection
// -----------------------------------------------------------------------------
#[test]
fn test_unit_6_retention_selection_and_isolation() {
    let temp = TempTestDir::new("unit_retention");

    let create_mock_archive = |name: &str, b_type: BackupType, ts: &str| {
        let path = temp.path().join(name);
        let file = File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        let b_type_str = match b_type {
            BackupType::Automatic => "automatic",
            BackupType::Manual => "manual",
            BackupType::Checkpoint => "checkpoint",
        };

        let manifest = format!(
            r#"{{
                "format_identifier": "{BACKUP_FORMAT_IDENTIFIER}",
                "format_version": 1,
                "app_version": "0.1.0",
                "created_at": "{ts}",
                "backup_type": "{b_type_str}",
                "database": {{
                    "entry_path": "database.sqlite",
                    "file_size": 4,
                    "sha256": "9f83461663a617f2b11794293f4f6a6b820add8b7865f0887fadd11e2a4e8447",
                    "schema_version": 4,
                    "note_count": 0,
                    "notebook_count": 0,
                    "tag_count": 0,
                    "trash_count": 0
                }},
                "attachments_summary": {{ "total_files": 0, "total_bytes": 0 }},
                "attachments": [],
                "scope": {{ "includes_settings": true, "includes_trash": true, "includes_attachments": true }},
                "checksums": {{}}
            }}"#
        );
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.finish().unwrap();
        path
    };

    // Create 4 automatic backups
    let a1 = create_mock_archive(
        "auto-1.pnbackup",
        BackupType::Automatic,
        "2026-10-10T10:00:00Z",
    );
    let a2 = create_mock_archive(
        "auto-2.pnbackup",
        BackupType::Automatic,
        "2026-10-11T10:00:00Z",
    );
    let a3 = create_mock_archive(
        "auto-3.pnbackup",
        BackupType::Automatic,
        "2026-10-12T10:00:00Z",
    );
    let a4 = create_mock_archive(
        "auto-4.pnbackup",
        BackupType::Automatic,
        "2026-10-13T10:00:00Z",
    );

    // Create 1 manual backup and 1 checkpoint
    let m1 = create_mock_archive(
        "manual-1.pnbackup",
        BackupType::Manual,
        "2026-10-01T10:00:00Z",
    );
    let c1 = create_mock_archive(
        "checkpoint-1.pnbackup",
        BackupType::Checkpoint,
        "2026-10-02T10:00:00Z",
    );

    // Apply retention = 2 on automatic backups
    let res = BackupRetentionManager::apply_retention(temp.path(), 2, None);

    assert_eq!(res.total_managed_found, 4);
    assert_eq!(res.kept_count, 2);
    assert_eq!(res.deleted_paths.len(), 2);

    // Oldest auto backups deleted, newest kept
    assert!(!a1.exists(), "Oldest auto backup 1 should be pruned");
    assert!(!a2.exists(), "Second oldest auto backup 2 should be pruned");
    assert!(a3.exists(), "Newer auto backup 3 must be kept");
    assert!(a4.exists(), "Newest auto backup 4 must be kept");

    // Manual and Checkpoint must NEVER be deleted
    assert!(
        m1.exists(),
        "Manual backup must NEVER be deleted by retention"
    );
    assert!(c1.exists(), "Checkpoint must NEVER be deleted by retention");
}

// -----------------------------------------------------------------------------
// 7. Error Classification
// -----------------------------------------------------------------------------
#[test]
fn test_unit_7_error_classification_and_privacy_categories() {
    let err_dest = BackupError::DestinationUnavailable("disk path missing".to_string());
    assert_eq!(err_dest.category(), "DestinationUnavailable");
    assert!(err_dest
        .user_friendly_message()
        .contains("Backup destination is unavailable"));

    let err_perm = BackupError::PermissionDenied("/root/backups".to_string());
    assert_eq!(err_perm.category(), "PermissionDenied");
    assert!(err_perm
        .user_friendly_message()
        .contains("Permission denied"));

    let err_space = BackupError::InsufficientDiskSpace;
    assert_eq!(err_space.category(), "InsufficientDiskSpace");
    assert!(err_space
        .user_friendly_message()
        .contains("Insufficient disk space"));

    let err_db_snap = BackupError::DatabaseSnapshotFailed("busy".to_string());
    assert_eq!(err_db_snap.category(), "DatabaseSnapshotFailed");

    let err_archive_w = BackupError::ArchiveWriteFailed("disk full".to_string());
    assert_eq!(err_archive_w.category(), "ArchiveWriteFailed");

    let err_archive_r = BackupError::ArchiveReadFailed("corrupt zip".to_string());
    assert_eq!(err_archive_r.category(), "ArchiveReadFailed");

    let err_val = BackupError::ValidationFailed("missing table".to_string());
    assert_eq!(err_val.category(), "ValidationFailed");

    let err_manifest = BackupError::ManifestInvalid("bad json".to_string());
    assert_eq!(err_manifest.category(), "ManifestInvalid");

    let err_checksum = BackupError::ChecksumMismatch {
        entry: "database.sqlite".to_string(),
        expected: "abc".to_string(),
        actual: "def".to_string(),
    };
    assert_eq!(err_checksum.category(), "ChecksumMismatch");
    assert!(err_checksum
        .user_friendly_message()
        .contains("checksum mismatch for 'database.sqlite'"));

    let err_unsupp_ver = BackupError::UnsupportedVersion {
        version: 5,
        max_supported: 1,
    };
    assert_eq!(err_unsupp_ver.category(), "UnsupportedVersion");

    let err_cancel = BackupError::Cancelled;
    assert_eq!(err_cancel.category(), "Cancelled");
    assert!(err_cancel.user_friendly_message().contains("cancelled"));
}

// -----------------------------------------------------------------------------
// 8. Backup Settings Validation
// -----------------------------------------------------------------------------
#[test]
fn test_unit_8_backup_settings_validation_and_safety_bounds() {
    let temp = TempTestDir::new("unit_settings");
    let paths = StoragePaths::from_root(temp.path().to_path_buf());
    paths.ensure_directories().unwrap();

    let mut conn = init_connection(&paths.database_file).unwrap();
    run_migrations(&mut conn).unwrap();

    // Verify constants
    assert_eq!(MIN_BACKUP_INTERVAL_MINUTES, 15);
    assert_eq!(MAX_BACKUP_INTERVAL_MINUTES, 43200);
    assert_eq!(MIN_BACKUP_RETENTION_COUNT, 1);
    assert_eq!(MAX_BACKUP_RETENTION_COUNT, 50);

    // Valid default settings
    let def = BackupSettings::default();
    assert_eq!(def.auto_backup_enabled, true);
    assert_eq!(def.interval_minutes, 1440);
    assert_eq!(def.retention_count, 5);

    // Save valid settings
    let custom = BackupSettings {
        auto_backup_enabled: true,
        interval_minutes: 60,
        backup_destination_dir: None,
        retention_count: 10,
        backup_on_startup: true,
    };
    let saved = BackupSettingsService::save_settings(&conn, &paths, custom).unwrap();
    assert_eq!(saved.interval_minutes, 60);
    assert_eq!(saved.retention_count, 10);

    // Interval below min (10 < 15)
    let bad_interval_low = BackupSettings {
        interval_minutes: 10,
        ..BackupSettings::default()
    };
    assert!(BackupSettingsService::save_settings(&conn, &paths, bad_interval_low).is_err());

    // Interval above max (50,000 > 43,200)
    let bad_interval_high = BackupSettings {
        interval_minutes: 50_000,
        ..BackupSettings::default()
    };
    assert!(BackupSettingsService::save_settings(&conn, &paths, bad_interval_high).is_err());

    // Retention below min (0 < 1)
    let bad_ret_low = BackupSettings {
        retention_count: 0,
        ..BackupSettings::default()
    };
    assert!(BackupSettingsService::save_settings(&conn, &paths, bad_ret_low).is_err());

    // Retention above max (55 > 50)
    let bad_ret_high = BackupSettings {
        retention_count: 55,
        ..BackupSettings::default()
    };
    assert!(BackupSettingsService::save_settings(&conn, &paths, bad_ret_high).is_err());

    // Targeting active database path rejected
    let target_db = BackupSettings {
        backup_destination_dir: Some(paths.database_file.to_string_lossy().to_string()),
        ..BackupSettings::default()
    };
    assert!(BackupSettingsService::save_settings(&conn, &paths, target_db).is_err());

    // Targeting active attachments dir rejected
    let target_att = BackupSettings {
        backup_destination_dir: Some(paths.attachments_dir.to_string_lossy().to_string()),
        ..BackupSettings::default()
    };
    assert!(BackupSettingsService::save_settings(&conn, &paths, target_att).is_err());
}

// -----------------------------------------------------------------------------
// 9. Archive Entry Limits
// -----------------------------------------------------------------------------
#[test]
fn test_unit_9_archive_entry_limits_and_safety_caps() {
    assert_eq!(MAX_ARCHIVE_ENTRY_COUNT, 10_000);
    assert_eq!(MAX_TOTAL_UNCOMPRESSED_BYTES, 5 * 1024 * 1024 * 1024);

    let temp = TempTestDir::new("unit_limits");
    let archive_path = temp.path().join("limits_test.pnbackup");

    // Create a mock archive with small content
    let file = File::create(&archive_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default();

    zip.start_file("manifest.json", opts).unwrap();
    zip.write_all(b"{}").unwrap();
    zip.finish().unwrap();

    let val_res = ManifestManager::validate_archive(&archive_path).unwrap();
    // It will fail because manifest is empty {}, but entry limits did not trigger
    assert!(!val_res
        .issues
        .iter()
        .any(|i| i.code == "ARCHIVE_ENTRY_LIMIT_EXCEEDED"));
    assert!(!val_res
        .issues
        .iter()
        .any(|i| i.code == "ARCHIVE_SIZE_LIMIT_EXCEEDED"));
}

// -----------------------------------------------------------------------------
// 10. Unsupported-Version Handling
// -----------------------------------------------------------------------------
#[test]
fn test_unit_10_unsupported_version_handling() {
    let temp = TempTestDir::new("unit_unsupported");
    let archive_path = temp.path().join("unsupported.pnbackup");

    let file = File::create(&archive_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default();

    let db_payload = b"test";
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(db_payload);
    let db_hash = format!("{:x}", hasher.finalize());

    let manifest = format!(
        r#"{{
            "format_identifier": "{BACKUP_FORMAT_IDENTIFIER}",
            "format_version": 42,
            "app_version": "9.9.9",
            "created_at": "2026-10-14T10:00:00Z",
            "backup_type": "manual",
            "database": {{
                "entry_path": "database.sqlite",
                "file_size": {},
                "sha256": "{db_hash}",
                "schema_version": 4,
                "note_count": 0,
                "notebook_count": 0,
                "tag_count": 0,
                "trash_count": 0
            }},
            "attachments_summary": {{ "total_files": 0, "total_bytes": 0 }},
            "attachments": [],
            "scope": {{ "includes_settings": true, "includes_trash": true, "includes_attachments": true }},
            "checksums": {{ "database.sqlite": "{db_hash}" }}
        }}"#,
        db_payload.len()
    );
    zip.start_file("manifest.json", opts).unwrap();
    zip.write_all(manifest.as_bytes()).unwrap();

    zip.start_file("database.sqlite", opts).unwrap();
    zip.write_all(db_payload).unwrap();

    zip.finish().unwrap();

    let res = ManifestManager::validate_archive(&archive_path).unwrap();
    assert_eq!(
        res.compatibility,
        BackupCompatibilityStatus::IncompatibleVersion
    );
    assert!(!res.is_valid);
    assert!(res
        .issues
        .iter()
        .any(|i| i.code == "UNSUPPORTED_FORMAT_VERSION"));
}

// -----------------------------------------------------------------------------
// 11. Database Schema Compatibility Rules
// -----------------------------------------------------------------------------
#[test]
fn test_unit_11_database_schema_compatibility_rules() {
    assert_eq!(CURRENT_APP_SCHEMA_VERSION, 4);

    let temp = TempTestDir::new("unit_schema_compat");
    let paths = StoragePaths::from_root(temp.path().to_path_buf());
    paths.ensure_directories().unwrap();

    // Helper to create archive with specified schema version
    let create_archive = |name: &str, schema_v: i32| -> PathBuf {
        let p = temp.path().join(name);
        let db_p = temp.path().join(format!("{name}.sqlite"));
        {
            let conn = rusqlite::Connection::open(&db_p).unwrap();
            conn.execute_batch(
                "CREATE TABLE notes (id TEXT PRIMARY KEY, title TEXT, content TEXT, format TEXT, created_at TEXT, modified_at TEXT);
                 CREATE TABLE notebooks (id TEXT PRIMARY KEY, name TEXT, created_at TEXT, modified_at TEXT);
                 CREATE TABLE tags (id TEXT PRIMARY KEY, name TEXT, created_at TEXT);
                 CREATE TABLE note_tags (note_id TEXT, tag_id TEXT);
                 CREATE TABLE attachments (id TEXT PRIMARY KEY, note_id TEXT, file_name TEXT, relative_path TEXT, file_size INTEGER);
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
                 CREATE TABLE _migrations (version INTEGER PRIMARY KEY);"
            ).unwrap();
            conn.execute("INSERT INTO _migrations (version) VALUES (?1)", [schema_v])
                .unwrap();
        }

        let (db_hash, db_size) = compute_file_sha256(&db_p).unwrap();
        let file = File::create(&p).unwrap();
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
                    "schema_version": {schema_v},
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
        zip.write_all(&fs::read(&db_p).unwrap()).unwrap();
        zip.finish().unwrap();

        p
    };

    // 1. Current schema (v4): Compatible
    let v4_arch = create_archive("schema_v4.pnbackup", 4);
    let v4_res = personal_notepad_lib::storage::services::BackupValidationService::validate_backup(
        &v4_arch, &paths,
    )
    .unwrap();
    assert!(v4_res.is_valid);
    assert_eq!(v4_res.compatibility, BackupCompatibilityStatus::Compatible);

    // 2. Older schema (v2): RequiresMigration
    let v2_arch = create_archive("schema_v2.pnbackup", 2);
    let v2_res = personal_notepad_lib::storage::services::BackupValidationService::validate_backup(
        &v2_arch, &paths,
    )
    .unwrap();
    assert!(v2_res.is_valid);
    assert_eq!(
        v2_res.compatibility,
        BackupCompatibilityStatus::RequiresMigration
    );
    assert!(v2_res
        .issues
        .iter()
        .any(|i| i.code == "SCHEMA_MIGRATION_REQUIRED"));

    // 3. Newer unsupported schema (v99): IncompatibleVersion
    let v99_arch = create_archive("schema_v99.pnbackup", 99);
    let v99_res =
        personal_notepad_lib::storage::services::BackupValidationService::validate_backup(
            &v99_arch, &paths,
        )
        .unwrap();
    assert!(!v99_res.is_valid);
    assert_eq!(
        v99_res.compatibility,
        BackupCompatibilityStatus::IncompatibleVersion
    );
    assert!(v99_res
        .issues
        .iter()
        .any(|i| i.code == "INCOMPATIBLE_SCHEMA_VERSION"));
}
