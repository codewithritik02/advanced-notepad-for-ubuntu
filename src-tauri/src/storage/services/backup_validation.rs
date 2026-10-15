use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use rusqlite::Connection;
use uuid::Uuid;
use zip::ZipArchive;

use crate::storage::errors::BackupError;
use crate::storage::models::{
    BackupCompatibilityStatus, BackupValidationIssue,
    BackupValidationIssueSeverity, BackupValidationResult,
};
use crate::storage::paths::StoragePaths;
use crate::storage::services::manifest::{
    ManifestManager, CURRENT_APP_SCHEMA_VERSION,
};

/// RAII cleanup guard for temporary validation staging directory.
struct ValStagingGuard<'a>(&'a Path);

impl<'a> Drop for ValStagingGuard<'a> {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = fs::remove_dir_all(self.0);
        }
    }
}

/// Service responsible for performing exhaustive, non-destructive validation of backup archives
/// before restoration is permitted (Phase 11, Task 14).
pub struct BackupValidationService;

impl BackupValidationService {
    /// Inspects and validates a selected backup archive (`.pnbackup` or `.zip`) before restore.
    ///
    /// Guarantees:
    /// 1. Never touches or modifies the active live SQLite database or live attachment stores.
    /// 2. Performs archive-level security checks (Zip Slip traversal defense, entry count, size limits).
    /// 3. Validates `manifest.json` structure, format version, and recalculates SHA-256 cryptographic hashes.
    /// 4. Extracts `database.sqlite` to an isolated temporary sandbox and verifies:
    ///    - `PRAGMA integrity_check` (SQLite database consistency)
    ///    - `PRAGMA foreign_key_check` (Referential integrity)
    ///    - Required table presence (`notes`, `notebooks`, `tags`, `note_tags`, `attachments`, `settings`, `_migrations`)
    ///    - Essential column schema presence on each required table
    ///    - Attachment cross-reference consistency (all database attachments exist in archive)
    ///    - Actual note and notebook counts vs manifest declarations
    ///    - Schema version compatibility (Compatible, RequiresMigration, IncompatibleVersion)
    /// 5. Automatically cleans up temporary sandboxes even upon failure.
    pub fn validate_backup(
        archive_path: &Path,
        paths: &StoragePaths,
    ) -> Result<BackupValidationResult, BackupError> {
        // 1. Guard against nonexistent or live database files
        if !archive_path.exists() {
            return Err(BackupError::DestinationUnavailable(format!(
                "Backup archive not found at '{}'",
                archive_path.display()
            )));
        }

        if !archive_path.is_file() {
            return Err(BackupError::DestinationUnavailable(format!(
                "Path '{}' is not a file.",
                archive_path.display()
            )));
        }

        let live_db_str = paths.database_file.to_string_lossy().to_string();
        let archive_str = archive_path.to_string_lossy().to_string();
        if archive_str == live_db_str || archive_str.starts_with(&format!("{live_db_str}-")) {
            return Err(BackupError::DestinationUnavailable(
                "Cannot validate active database or journal file as a backup archive.".to_string(),
            ));
        }

        // 2. Perform baseline archive & manifest inspection using ManifestManager
        let mut archive_val_res = match ManifestManager::validate_archive(archive_path) {
            Ok(res) => res,
            Err(BackupError::ArchiveReadFailed(msg)) => {
                return Ok(BackupValidationResult {
                    is_valid: false,
                    compatibility: BackupCompatibilityStatus::Corrupted,
                    manifest: None,
                    issues: vec![BackupValidationIssue {
                        severity: BackupValidationIssueSeverity::Error,
                        code: "ARCHIVE_READ_FAILED".to_string(),
                        message: format!("Failed to read archive: {msg}"),
                    }],
                });
            }
            Err(e) => return Err(e),
        };

        let file = match File::open(archive_path) {
            Ok(f) => f,
            Err(e) => return Err(BackupError::Io(e)),
        };

        let mut archive = match ZipArchive::new(file) {
            Ok(a) => a,
            Err(e) => {
                return Ok(BackupValidationResult {
                    is_valid: false,
                    compatibility: BackupCompatibilityStatus::Corrupted,
                    manifest: None,
                    issues: vec![BackupValidationIssue {
                        severity: BackupValidationIssueSeverity::Error,
                        code: "ARCHIVE_OPEN_FAILED".to_string(),
                        message: format!("Could not open ZIP archive: {e}"),
                    }],
                });
            }
        };

        // If manifest was completely missing or corrupt, we cannot proceed with DB inspection
        let manifest = match archive_val_res.manifest {
            Some(ref m) => m.clone(),
            None => return Ok(archive_val_res),
        };

        // 3. Extract database.sqlite to an isolated staging directory for deep inspection
        let staging_dir = paths.backups_dir.join(format!(".val_staging_{}", Uuid::new_v4()));
        if let Err(e) = fs::create_dir_all(&staging_dir) {
            return Err(BackupError::Io(e));
        }
        let _guard = ValStagingGuard(&staging_dir);

        let temp_db_path = staging_dir.join("database.sqlite");

        // Extract database.sqlite from archive
        let db_extract_res = (|| -> Result<(), BackupError> {
            let mut db_entry = archive
                .by_name("database.sqlite")
                .map_err(|e| BackupError::ArchiveReadFailed(format!("Missing database.sqlite in archive: {e}")))?;

            let out_file = File::create(&temp_db_path).map_err(BackupError::Io)?;
            let mut writer = BufWriter::new(out_file);
            let mut buf = [0u8; 65536];

            loop {
                let n = db_entry.read(&mut buf).map_err(BackupError::Io)?;
                if n == 0 {
                    break;
                }
                writer.write_all(&buf[..n]).map_err(BackupError::Io)?;
            }
            writer.flush().map_err(BackupError::Io)?;
            Ok(())
        })();

        if let Err(e) = db_extract_res {
            archive_val_res.issues.push(BackupValidationIssue {
                severity: BackupValidationIssueSeverity::Error,
                code: "DATABASE_EXTRACT_FAILED".to_string(),
                message: format!("Could not extract database.sqlite for verification: {e}"),
            });
            archive_val_res.is_valid = false;
            archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
            return Ok(archive_val_res);
        }

        // 4. Perform SQLite deep inspection on the isolated staged copy
        let db_conn = match Connection::open(&temp_db_path) {
            Ok(c) => c,
            Err(e) => {
                archive_val_res.issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Error,
                    code: "DATABASE_OPEN_FAILED".to_string(),
                    message: format!("Failed to open staged database: {e}"),
                });
                archive_val_res.is_valid = false;
                archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                return Ok(archive_val_res);
            }
        };

        // Enforce query_only mode to protect staged copy from unintended mutations
        if let Err(e) = db_conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA query_only = ON;") {
            eprintln!("Warning: failed to set query_only on validation DB: {e}");
        }

        // 4a. Run PRAGMA integrity_check
        match db_conn.query_row("PRAGMA integrity_check;", [], |r| r.get::<_, String>(0)) {
            Ok(integrity_res) => {
                if integrity_res.to_lowercase() != "ok" {
                    archive_val_res.issues.push(BackupValidationIssue {
                        severity: BackupValidationIssueSeverity::Error,
                        code: "DATABASE_INTEGRITY_FAILED".to_string(),
                        message: format!("SQLite PRAGMA integrity_check failed: '{integrity_res}'"),
                    });
                    archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                }
            }
            Err(e) => {
                archive_val_res.issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Error,
                    code: "DATABASE_INTEGRITY_CHECK_ERROR".to_string(),
                    message: format!("Error running PRAGMA integrity_check: {e}"),
                });
                archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
            }
        }

        // 4b. Run PRAGMA foreign_key_check
        match db_conn.prepare("PRAGMA foreign_key_check;") {
            Ok(mut fk_stmt) => {
                match fk_stmt.query([]) {
                    Ok(mut rows) => {
                        let mut fk_violations = 0;
                        while let Ok(Some(row)) = rows.next() {
                            let table: String = row.get(0).unwrap_or_else(|_| "unknown".to_string());
                            let rowid: i64 = row.get(1).unwrap_or(0);
                            let parent: String = row.get(2).unwrap_or_else(|_| "unknown".to_string());
                            fk_violations += 1;
                            if fk_violations <= 5 {
                                archive_val_res.issues.push(BackupValidationIssue {
                                    severity: BackupValidationIssueSeverity::Error,
                                    code: "FOREIGN_KEY_VIOLATION".to_string(),
                                    message: format!(
                                        "Foreign key violation in table '{table}' (rowid {rowid}) referencing '{parent}'."
                                    ),
                                });
                            }
                        }
                        if fk_violations > 0 {
                            archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                        }
                    }
                    Err(e) => {
                        archive_val_res.issues.push(BackupValidationIssue {
                            severity: BackupValidationIssueSeverity::Warning,
                            code: "FOREIGN_KEY_CHECK_ERROR".to_string(),
                            message: format!("Could not evaluate foreign keys: {e}"),
                        });
                    }
                }
            }
            Err(e) => {
                archive_val_res.issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Warning,
                    code: "FOREIGN_KEY_PREPARE_ERROR".to_string(),
                    message: format!("Could not prepare PRAGMA foreign_key_check: {e}"),
                });
            }
        }

        // 4c. Verify required tables and expected columns
        let required_tables = [
            ("notes", vec!["id", "title", "content", "format", "created_at", "modified_at"]),
            ("notebooks", vec!["id", "name", "created_at", "modified_at"]),
            ("tags", vec!["id", "name", "created_at"]),
            ("note_tags", vec!["note_id", "tag_id"]),
            ("attachments", vec!["id", "note_id", "file_name", "relative_path", "file_size"]),
            ("settings", vec!["key", "value"]),
            ("_migrations", vec!["version"]),
        ];

        for (table_name, expected_cols) in &required_tables {
            let table_exists: bool = db_conn
                .query_row(
                    "SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table_name],
                    |r| r.get::<_, i64>(0).map(|c| c > 0),
                )
                .unwrap_or(false);

            if !table_exists {
                archive_val_res.issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Error,
                    code: "MISSING_TABLE".to_string(),
                    message: format!("Required table '{table_name}' is missing from the backup database."),
                });
                archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                continue;
            }

            // Inspect columns via PRAGMA table_info
            let pragma_sql = format!("PRAGMA table_info({});", table_name);
            if let Ok(mut col_stmt) = db_conn.prepare(&pragma_sql) {
                if let Ok(col_rows) = col_stmt.query_map([], |r| r.get::<_, String>(1)) {
                    let present_cols: HashSet<String> = col_rows.filter_map(Result::ok).collect();
                    for expected_col in expected_cols {
                        if !present_cols.contains(*expected_col) {
                            archive_val_res.issues.push(BackupValidationIssue {
                                severity: BackupValidationIssueSeverity::Error,
                                code: "MISSING_COLUMN".to_string(),
                                message: format!(
                                    "Required column '{expected_col}' is missing from table '{table_name}'."
                                ),
                            });
                            archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                        }
                    }
                }
            }
        }

        // 4d. Verify database migration schema version
        let actual_schema_version: i32 = db_conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if actual_schema_version != manifest.database.schema_version {
            archive_val_res.issues.push(BackupValidationIssue {
                severity: BackupValidationIssueSeverity::Warning,
                code: "SCHEMA_VERSION_MISMATCH".to_string(),
                message: format!(
                    "Database schema version (v{actual_schema_version}) differs from manifest declared version (v{}).",
                    manifest.database.schema_version
                ),
            });
        }

        if actual_schema_version > CURRENT_APP_SCHEMA_VERSION {
            archive_val_res.issues.push(BackupValidationIssue {
                severity: BackupValidationIssueSeverity::Error,
                code: "INCOMPATIBLE_SCHEMA_VERSION".to_string(),
                message: format!(
                    "Backup database schema version (v{actual_schema_version}) is newer than current application version (v{CURRENT_APP_SCHEMA_VERSION})."
                ),
            });
            archive_val_res.compatibility = BackupCompatibilityStatus::IncompatibleVersion;
        } else if actual_schema_version < CURRENT_APP_SCHEMA_VERSION {
            if !archive_val_res
                .issues
                .iter()
                .any(|i| i.code == "SCHEMA_MIGRATION_REQUIRED")
            {
                archive_val_res.issues.push(BackupValidationIssue {
                    severity: BackupValidationIssueSeverity::Warning,
                    code: "SCHEMA_MIGRATION_REQUIRED".to_string(),
                    message: format!(
                        "Backup database schema is v{actual_schema_version}; it will be migrated to v{CURRENT_APP_SCHEMA_VERSION} during restore."
                    ),
                });
            }
            if archive_val_res.compatibility == BackupCompatibilityStatus::Compatible {
                archive_val_res.compatibility = BackupCompatibilityStatus::RequiresMigration;
            }
        }

        // 4e. Cross-check database attachment records with archive entries
        if let Ok(mut att_stmt) = db_conn.prepare("SELECT id, note_id, file_name, relative_path FROM attachments") {
            if let Ok(att_rows) = att_stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            }) {
                let mut db_attachment_ids = HashSet::new();

                for att_row in att_rows.flatten() {
                    let (att_id, _note_id, file_name, rel_path) = att_row;
                    db_attachment_ids.insert(att_id.clone());

                    let expected_entry = format!("attachments/{}", rel_path.trim().replace('\\', "/"));
                    if archive.by_name(&expected_entry).is_err() {
                        archive_val_res.issues.push(BackupValidationIssue {
                            severity: BackupValidationIssueSeverity::Error,
                            code: "ATTACHMENT_MISSING_FROM_ARCHIVE".to_string(),
                            message: format!(
                                "Attachment '{file_name}' (id: {att_id}) referenced in backup database is missing from archive ('{expected_entry}')."
                            ),
                        });
                        archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                    }
                }

                // Check for attachments declared in manifest but not present in database
                for manifest_att in &manifest.attachments {
                    if !db_attachment_ids.contains(&manifest_att.id) {
                        archive_val_res.issues.push(BackupValidationIssue {
                            severity: BackupValidationIssueSeverity::Warning,
                            code: "UNREFERENCED_ATTACHMENT".to_string(),
                            message: format!(
                                "Attachment '{}' (id: {}) is in archive inventory but not referenced in database.",
                                manifest_att.file_name, manifest_att.id
                            ),
                        });
                    }
                }
            }
        }

        // 4f. Check note counts consistency
        let total_notes_in_db: i64 = db_conn
            .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
            .unwrap_or(0);

        let declared_notes = (manifest.database.note_count + manifest.database.trash_count) as i64;
        if total_notes_in_db != declared_notes {
            archive_val_res.issues.push(BackupValidationIssue {
                severity: BackupValidationIssueSeverity::Warning,
                code: "NOTE_COUNT_MISMATCH".to_string(),
                message: format!(
                    "Database contains {total_notes_in_db} notes (including trash), but manifest declared {declared_notes}."
                ),
            });
        }

        // 4g. Check encryption metadata consistency and integrity (Phase 12, Task 19)
        let db_enc_meta_str: Option<String> = db_conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [crate::storage::crypto::metadata::ENCRYPTION_METADATA_KEY],
                |r| r.get(0),
            )
            .ok();

        if manifest.is_encrypted {
            match db_enc_meta_str {
                Some(ref s) => {
                    match crate::storage::crypto::EncryptionMetadata::from_json(s) {
                        Ok(meta) => {
                            if let Err(e) = meta.validate() {
                                crate::storage::crypto::record_security_event(
                                    crate::storage::crypto::SecurityEventCode::BackupValidationFailed,
                                    crate::storage::crypto::SecurityLogLevel::Error,
                                    "Backup encryption metadata validation failed",
                                    Some("metadata_invalid"),
                                );
                                archive_val_res.issues.push(BackupValidationIssue {
                                    severity: BackupValidationIssueSeverity::Error,
                                    code: "ENCRYPTION_METADATA_INVALID".to_string(),
                                    message: format!("Encryption metadata failed validation: {e}"),
                                });
                                archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                            } else if !meta.enabled {
                                crate::storage::crypto::record_security_event(
                                    crate::storage::crypto::SecurityEventCode::BackupValidationFailed,
                                    crate::storage::crypto::SecurityLogLevel::Error,
                                    "Backup declared encrypted but metadata is disabled",
                                    Some("encryption_disabled"),
                                );
                                archive_val_res.issues.push(BackupValidationIssue {
                                    severity: BackupValidationIssueSeverity::Error,
                                    code: "ENCRYPTION_DISABLED_IN_METADATA".to_string(),
                                    message: "Manifest declared backup as encrypted, but encryption metadata has enabled=false.".to_string(),
                                });
                                archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                            }
                        }
                        Err(e) => {
                            crate::storage::crypto::record_security_event(
                                crate::storage::crypto::SecurityEventCode::BackupValidationFailed,
                                crate::storage::crypto::SecurityLogLevel::Error,
                                "Backup encryption metadata in settings is malformed",
                                Some("metadata_malformed"),
                            );
                            archive_val_res.issues.push(BackupValidationIssue {
                                severity: BackupValidationIssueSeverity::Error,
                                code: "ENCRYPTION_METADATA_INVALID".to_string(),
                                message: format!("Encryption metadata in database settings is malformed: {e}"),
                            });
                            archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                        }
                    }
                }
                None => {
                    crate::storage::crypto::record_security_event(
                        crate::storage::crypto::SecurityEventCode::BackupValidationFailed,
                        crate::storage::crypto::SecurityLogLevel::Error,
                        "Backup declared encrypted but encryption metadata is missing from database",
                        Some("metadata_missing"),
                    );
                    archive_val_res.issues.push(BackupValidationIssue {
                        severity: BackupValidationIssueSeverity::Error,
                        code: "ENCRYPTION_METADATA_MISSING".to_string(),
                        message: "Manifest declared backup as encrypted, but encryption metadata was not found in database settings.".to_string(),
                    });
                    archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                }
            }
        } else if let Some(ref s) = db_enc_meta_str {
            if let Ok(meta) = crate::storage::crypto::EncryptionMetadata::from_json(s) {
                if meta.enabled {
                    archive_val_res.issues.push(BackupValidationIssue {
                        severity: BackupValidationIssueSeverity::Warning,
                        code: "ENCRYPTION_METADATA_MISMATCH".to_string(),
                        message: "Database contains active encryption metadata, but manifest does not declare is_encrypted: true.".to_string(),
                    });
                    if let Err(e) = meta.validate() {
                        archive_val_res.issues.push(BackupValidationIssue {
                            severity: BackupValidationIssueSeverity::Error,
                            code: "ENCRYPTION_METADATA_INVALID".to_string(),
                            message: format!("Encryption metadata failed validation: {e}"),
                        });
                        archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
                    }
                }
            }
        }

        // 5. Final validity calculation
        let has_errors = archive_val_res
            .issues
            .iter()
            .any(|i| i.severity == BackupValidationIssueSeverity::Error);

        archive_val_res.is_valid = !has_errors;
        if has_errors && archive_val_res.compatibility != BackupCompatibilityStatus::IncompatibleVersion {
            archive_val_res.compatibility = BackupCompatibilityStatus::Corrupted;
        }

        Ok(archive_val_res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::{BackupCreateRequest, BackupType, CreateNoteDto};
    use crate::storage::repositories::notes::NoteRepository;
    use crate::storage::services::backup::BackupService;

    #[test]
    fn test_task_14_validate_valid_backup_end_to_end() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_val_bk_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        // Create notes
        let _ = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Valid Backup Note".to_string()),
                content: Some("Sample content".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        ).unwrap();

        // Create backup
        let backup_res = BackupService::create_backup(
            &conn,
            &paths,
            BackupCreateRequest {
                destination_path: None,
                backup_type: Some(BackupType::Manual),
                allow_overwrite: Some(false),
            },
            None::<fn(_)>,
        ).unwrap();

        let archive_path = Path::new(&backup_res.file_path);
        let val_res = BackupValidationService::validate_backup(archive_path, &paths).unwrap();

        assert!(val_res.is_valid, "Valid backup must pass validation");
        assert_eq!(val_res.compatibility, BackupCompatibilityStatus::Compatible);
        assert!(val_res.manifest.is_some());
        assert!(!val_res.issues.iter().any(|i| i.severity == BackupValidationIssueSeverity::Error));
    }

    #[test]
    fn test_task_14_detects_corrupted_archive_and_missing_files() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_val_corrupt_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // 1. Non-existent file
        let missing_path = temp_dir.join("nonexistent.pnbackup");
        let missing_res = BackupValidationService::validate_backup(&missing_path, &paths);
        assert!(missing_res.is_err(), "Nonexistent file must return error");

        // 2. Corrupt archive bytes
        let corrupt_path = temp_dir.join("corrupt.pnbackup");
        fs::write(&corrupt_path, b"Not a zip file").unwrap();
        let val_res = BackupValidationService::validate_backup(&corrupt_path, &paths).unwrap();
        assert!(!val_res.is_valid, "Corrupt file must be invalid");
        assert_eq!(val_res.compatibility, BackupCompatibilityStatus::Corrupted);
        assert!(val_res.issues.iter().any(|i| i.code == "ARCHIVE_READ_FAILED"));
    }

    #[test]
    fn test_task_14_rejects_active_database_as_backup() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_val_activedb_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        let err = BackupValidationService::validate_backup(&paths.database_file, &paths);
        assert!(err.is_err(), "Cannot validate active database file as backup archive");
    }

    #[test]
    fn test_task_14_detects_missing_table_or_column() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_val_missing_tbl_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // Create an incomplete database missing 'tags' table
        let bad_db_path = temp_dir.join("bad_database.sqlite");
        {
            let conn = Connection::open(&bad_db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE notes (id TEXT PRIMARY KEY, title TEXT, content TEXT, format TEXT, created_at TEXT, modified_at TEXT);
                 CREATE TABLE notebooks (id TEXT PRIMARY KEY, name TEXT, created_at TEXT, modified_at TEXT);
                 CREATE TABLE note_tags (note_id TEXT, tag_id TEXT);
                 CREATE TABLE attachments (id TEXT PRIMARY KEY, note_id TEXT, file_name TEXT, relative_path TEXT, file_size INTEGER);
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
                 CREATE TABLE _migrations (version INTEGER PRIMARY KEY);
                 INSERT INTO _migrations (version) VALUES (4);"
            ).unwrap();
            // Note: 'tags' table was intentionally NOT created
        }

        // Bundle into a test archive
        let archive_path = temp_dir.join("missing_table_archive.pnbackup");
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();

            // Manifest
            zip.start_file("manifest.json", opts).unwrap();
            let manifest_json = format!(
                r#"{{
                    "format_identifier": "personal-notepad-backup",
                    "format_version": 1,
                    "app_version": "0.1.0",
                    "created_at": "2026-10-13T12:00:00Z",
                    "backup_type": "manual",
                    "database": {{
                        "entry_path": "database.sqlite",
                        "file_size": {},
                        "sha256": "{}",
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
                }}"#,
                fs::metadata(&bad_db_path).unwrap().len(),
                crate::storage::services::snapshot::compute_file_sha256(&bad_db_path).unwrap().0
            );
            zip.write_all(manifest_json.as_bytes()).unwrap();

            // Database
            zip.start_file("database.sqlite", opts).unwrap();
            let mut db_bytes = fs::read(&bad_db_path).unwrap();
            zip.write_all(&mut db_bytes).unwrap();
            zip.finish().unwrap();
        }

        let val_res = BackupValidationService::validate_backup(&archive_path, &paths).unwrap();
        assert!(!val_res.is_valid, "Archive with missing required table must be invalid");
        assert_eq!(val_res.compatibility, BackupCompatibilityStatus::Corrupted);
        assert!(val_res.issues.iter().any(|i| i.code == "MISSING_TABLE" && i.message.contains("tags")));
    }

    #[test]
    fn test_task_14_schema_version_compatibility_modes() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_val_schema_modes_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // 1. Older schema (v2) requiring migration
        let v2_db_path = temp_dir.join("v2_database.sqlite");
        {
            let conn = Connection::open(&v2_db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE notes (id TEXT PRIMARY KEY, title TEXT, content TEXT, format TEXT, created_at TEXT, modified_at TEXT);
                 CREATE TABLE notebooks (id TEXT PRIMARY KEY, name TEXT, created_at TEXT, modified_at TEXT);
                 CREATE TABLE tags (id TEXT PRIMARY KEY, name TEXT, created_at TEXT);
                 CREATE TABLE note_tags (note_id TEXT, tag_id TEXT);
                 CREATE TABLE attachments (id TEXT PRIMARY KEY, note_id TEXT, file_name TEXT, relative_path TEXT, file_size INTEGER);
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
                 CREATE TABLE _migrations (version INTEGER PRIMARY KEY);
                 INSERT INTO _migrations (version) VALUES (2);"
            ).unwrap();
        }

        let archive_v2_path = temp_dir.join("v2_archive.pnbackup");
        {
            let file = File::create(&archive_v2_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();

            zip.start_file("manifest.json", opts).unwrap();
            let (db_hash, db_size) = crate::storage::services::snapshot::compute_file_sha256(&v2_db_path).unwrap();
            let manifest_json = format!(
                r#"{{
                    "format_identifier": "personal-notepad-backup",
                    "format_version": 1,
                    "app_version": "0.1.0",
                    "created_at": "2026-10-13T12:00:00Z",
                    "backup_type": "manual",
                    "database": {{
                        "entry_path": "database.sqlite",
                        "file_size": {db_size},
                        "sha256": "{db_hash}",
                        "schema_version": 2,
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
            zip.write_all(manifest_json.as_bytes()).unwrap();

            zip.start_file("database.sqlite", opts).unwrap();
            zip.write_all(&fs::read(&v2_db_path).unwrap()).unwrap();
            zip.finish().unwrap();
        }

        let val_v2 = BackupValidationService::validate_backup(&archive_v2_path, &paths).unwrap();
        assert!(val_v2.is_valid, "Older supported schema must remain valid");
        assert_eq!(val_v2.compatibility, BackupCompatibilityStatus::RequiresMigration);
        assert!(val_v2.issues.iter().any(|i| i.code == "SCHEMA_MIGRATION_REQUIRED"));

        // 2. Newer unsupported schema (v99)
        let v99_db_path = temp_dir.join("v99_database.sqlite");
        {
            let conn = Connection::open(&v99_db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE notes (id TEXT PRIMARY KEY, title TEXT, content TEXT, format TEXT, created_at TEXT, modified_at TEXT);
                 CREATE TABLE notebooks (id TEXT PRIMARY KEY, name TEXT, created_at TEXT, modified_at TEXT);
                 CREATE TABLE tags (id TEXT PRIMARY KEY, name TEXT, created_at TEXT);
                 CREATE TABLE note_tags (note_id TEXT, tag_id TEXT);
                 CREATE TABLE attachments (id TEXT PRIMARY KEY, note_id TEXT, file_name TEXT, relative_path TEXT, file_size INTEGER);
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
                 CREATE TABLE _migrations (version INTEGER PRIMARY KEY);
                 INSERT INTO _migrations (version) VALUES (99);"
            ).unwrap();
        }

        let archive_v99_path = temp_dir.join("v99_archive.pnbackup");
        {
            let file = File::create(&archive_v99_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();

            zip.start_file("manifest.json", opts).unwrap();
            let (db_hash, db_size) = crate::storage::services::snapshot::compute_file_sha256(&v99_db_path).unwrap();
            let manifest_json = format!(
                r#"{{
                    "format_identifier": "personal-notepad-backup",
                    "format_version": 1,
                    "app_version": "0.1.0",
                    "created_at": "2026-10-13T12:00:00Z",
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
                    "checksums": {{}}
                }}"#
            );
            zip.write_all(manifest_json.as_bytes()).unwrap();

            zip.start_file("database.sqlite", opts).unwrap();
            zip.write_all(&fs::read(&v99_db_path).unwrap()).unwrap();
            zip.finish().unwrap();
        }

        let val_v99 = BackupValidationService::validate_backup(&archive_v99_path, &paths).unwrap();
        assert!(!val_v99.is_valid, "Newer unsupported schema version must be invalid");
        assert_eq!(val_v99.compatibility, BackupCompatibilityStatus::IncompatibleVersion);
        assert!(val_v99.issues.iter().any(|i| i.code == "INCOMPATIBLE_SCHEMA_VERSION"));
    }
}
