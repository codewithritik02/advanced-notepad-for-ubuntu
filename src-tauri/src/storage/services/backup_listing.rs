use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::storage::errors::BackupError;
use crate::storage::models::{
    BackupListItem, BackupType, PRIMARY_BACKUP_EXTENSION, BACKUP_FORMAT_IDENTIFIER,
};
use crate::storage::paths::StoragePaths;
use crate::storage::services::backup_scheduler::BackupSchedulerService;
use crate::storage::services::backup_settings::BackupSettingsService;
use crate::storage::services::manifest::ManifestManager;

/// Service managing the inspection, listing, and explicit deletion of local backup archives (Phase 11, Task 13).
pub struct BackupListService;

impl BackupListService {
    /// Discovers and lists all available backup archives across default and configured backup destinations.
    ///
    /// Requirements & Guarantees:
    /// - Scans only configured/default backup directories.
    /// - Validates paths before filesystem inspection.
    /// - Does not modify or lock backups during listing.
    /// - Inspects archive headers safely without reading entire payload or attachments into memory.
    /// - Corrupted or unreadable archives are cataloged with `is_valid: false` and `issue_summary`
    ///   without failing the overall listing operation or crashing.
    /// - `is_verified` is initialized to false (distinguishing a discovered archive from a verified backup).
    /// - Results are sorted in descending order by `created_at` (newest first).
    pub fn list_backups(
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<Vec<BackupListItem>, BackupError> {
        let settings = BackupSettingsService::get_settings(conn).unwrap_or_default();

        let mut scan_dirs = Vec::new();
        scan_dirs.push(paths.backups_dir.clone());
        let checkpoints_dir = paths.checkpoints_dir();
        if checkpoints_dir.exists() && checkpoints_dir.is_dir() {
            scan_dirs.push(checkpoints_dir);
        }

        if let Some(ref custom_dir_str) = settings.backup_destination_dir {
            let custom_path = PathBuf::from(custom_dir_str);
            if custom_path != paths.backups_dir && custom_path.is_dir() {
                scan_dirs.push(custom_path);
            }
        }

        let mut discovered = Vec::new();
        let mut seen_paths = HashSet::new();

        for dir in scan_dirs {
            if !dir.exists() || !dir.is_dir() {
                continue;
            }

            let entries = match fs::read_dir(&dir) {
                Ok(rd) => rd,
                Err(e) => {
                    eprintln!("Warning: Failed to read backup directory '{}': {e}", dir.display());
                    continue;
                }
            };

            for entry in entries.flatten() {
                let file_type = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };

                // Skip directories and symlinks
                if file_type.is_symlink() || !file_type.is_file() {
                    continue;
                }

                let path = entry.path();
                let canonical_key = path.to_string_lossy().to_string();
                if seen_paths.contains(&canonical_key) {
                    continue;
                }
                seen_paths.insert(canonical_key);

                // Filter extensions
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if ext != PRIMARY_BACKUP_EXTENSION && ext != "zip" {
                    continue;
                }

                let file_name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                // Skip temporary staging/in-flight archives (.tmp.*)
                if file_name.contains(".tmp.") {
                    continue;
                }

                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let file_size = metadata.len();
                let fallback_mtime = metadata
                    .modified()
                    .ok()
                    .map(DateTime::<Utc>::from)
                    .unwrap_or_else(Utc::now);

                // Safe header inspection via ManifestManager
                match ManifestManager::read_manifest_quick(&path) {
                    Ok(manifest) => {
                        let is_our_format = manifest.format_identifier == BACKUP_FORMAT_IDENTIFIER;
                        if !is_our_format {
                            // Archive exists but belongs to another format
                            discovered.push(BackupListItem {
                                file_path: path.to_string_lossy().to_string(),
                                file_name,
                                file_size,
                                created_at: fallback_mtime.to_rfc3339(),
                                backup_type: infer_backup_type_from_filename(&path),
                                format_version: None,
                                schema_version: None,
                                note_count: None,
                                notebook_count: None,
                                attachment_count: None,
                                is_verified: false,
                                is_valid: false,
                                issue_summary: Some("Archive format identifier does not match Personal Notepad.".to_string()),
                            });
                        } else {
                            discovered.push(BackupListItem {
                                file_path: path.to_string_lossy().to_string(),
                                file_name,
                                file_size,
                                created_at: manifest.created_at,
                                backup_type: manifest.backup_type,
                                format_version: Some(manifest.format_version),
                                schema_version: Some(manifest.database.schema_version),
                                note_count: Some(manifest.database.note_count + manifest.database.trash_count),
                                notebook_count: Some(manifest.database.notebook_count),
                                attachment_count: Some(manifest.attachments_summary.total_files),
                                is_verified: false, // Discovered archive is unverified until deep validation
                                is_valid: true,
                                issue_summary: None,
                            });
                        }
                    }
                    Err(err) => {
                        // Unreadable, corrupt, or invalid archive: populate honestly without crashing
                        discovered.push(BackupListItem {
                            file_path: path.to_string_lossy().to_string(),
                            file_name,
                            file_size,
                            created_at: fallback_mtime.to_rfc3339(),
                            backup_type: infer_backup_type_from_filename(&path),
                            format_version: None,
                            schema_version: None,
                            note_count: None,
                            notebook_count: None,
                            attachment_count: None,
                            is_verified: false,
                            is_valid: false,
                            issue_summary: Some(err.user_friendly_message()),
                        });
                    }
                }
            }
        }

        // Sort descending: newest first
        discovered.sort_by(|a, b| {
            let dt_a = DateTime::parse_from_rfc3339(&a.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let dt_b = DateTime::parse_from_rfc3339(&b.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            dt_b.cmp(&dt_a)
        });

        Ok(discovered)
    }

    /// Safely deletes a managed backup archive upon explicit user action.
    ///
    /// Safety Invariants:
    /// - Fails if a database restore operation is currently executing.
    /// - Rejects attempts targeting active database files or live attachment stores.
    /// - Confirms target path is located inside a designated backup directory.
    /// - Rejects arbitrary non-backup files.
    pub fn delete_managed_backup(
        conn: &Connection,
        paths: &StoragePaths,
        file_path_str: &str,
    ) -> Result<bool, BackupError> {
        if BackupSchedulerService::is_restore_in_progress() {
            return Err(BackupError::ConcurrentOperation(
                "Cannot delete backups while a database restore operation is in progress.".to_string(),
            ));
        }

        let target_path = PathBuf::from(file_path_str.trim());
        if !target_path.is_file() {
            return Err(BackupError::DestinationUnavailable(format!(
                "Backup archive not found at '{}'",
                target_path.display()
            )));
        }

        // Protect active database and journals
        let live_db_str = paths.database_file.to_string_lossy().to_string();
        let target_str = target_path.to_string_lossy().to_string();
        if target_str == live_db_str || target_str.starts_with(&format!("{live_db_str}-")) {
            return Err(BackupError::PermissionDenied(
                "Cannot delete active database files.".to_string(),
            ));
        }

        // Protect attachments directory
        if target_path.starts_with(&paths.attachments_dir) {
            return Err(BackupError::PermissionDenied(
                "Cannot delete files from the active attachments repository.".to_string(),
            ));
        }

        // Validate location: must reside in backups_dir or configured destination
        let settings = BackupSettingsService::get_settings(conn).unwrap_or_default();
        let in_default_dir = target_path.starts_with(&paths.backups_dir);
        let in_custom_dir = settings
            .backup_destination_dir
            .as_ref()
            .map(|d| target_path.starts_with(d))
            .unwrap_or(false);

        if !in_default_dir && !in_custom_dir {
            return Err(BackupError::PermissionDenied(
                "Only backup archives located within designated backup directories can be removed.".to_string(),
            ));
        }

        // Validate extension
        let ext = target_path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext != PRIMARY_BACKUP_EXTENSION && ext != "zip" {
            return Err(BackupError::InvalidRequest(
                "Only files with backup extensions (.pnbackup or .zip) can be removed.".to_string(),
            ));
        }

        fs::remove_file(&target_path).map_err(BackupError::Io)?;

        Ok(true)
    }
}

/// Infers the backup type from standard filename naming conventions when manifest is unavailable
fn infer_backup_type_from_filename(path: &Path) -> BackupType {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    if name.contains("auto-backup") {
        BackupType::Automatic
    } else if name.contains("checkpoint") {
        BackupType::Checkpoint
    } else {
        BackupType::Manual
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use chrono::Duration;
    use uuid::Uuid;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::{
        BackupAttachmentsSummary, BackupDatabaseInfo, BackupManifest, BackupScopeDeclaration,
        CURRENT_BACKUP_FORMAT_VERSION,
    };

    fn create_test_archive(
        path: &Path,
        backup_type: BackupType,
        created_at: DateTime<Utc>,
        note_count: usize,
    ) {
        let file = fs::File::create(path).expect("Create test file");
        let mut zip = ZipWriter::new(file);

        let manifest = BackupManifest {
            format_identifier: BACKUP_FORMAT_IDENTIFIER.to_string(),
            format_version: CURRENT_BACKUP_FORMAT_VERSION,
            app_version: "0.1.0".to_string(),
            created_at: created_at.to_rfc3339(),
            backup_type,
            database: BackupDatabaseInfo {
                entry_path: "database.sqlite".to_string(),
                file_size: 2048,
                sha256: "hash123".to_string(),
                schema_version: 4,
                note_count,
                notebook_count: 1,
                tag_count: 2,
                trash_count: 0,
            },
            attachments_summary: BackupAttachmentsSummary {
                total_files: 3,
                total_bytes: 4096,
            },
            attachments: Vec::new(),
            scope: BackupScopeDeclaration {
                includes_settings: true,
                includes_trash: true,
                includes_attachments: true,
            },
            checksums: std::collections::HashMap::new(),
        };

        let json = serde_json::to_vec_pretty(&manifest).unwrap();
        zip.start_file("manifest.json", SimpleFileOptions::default()).unwrap();
        zip.write_all(&json).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn test_list_backups_empty_and_populated_directories() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_bk_list_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Empty directory listing
        let empty_list = BackupListService::list_backups(&conn, &paths).unwrap();
        assert!(empty_list.is_empty());

        // 2. Populate with two valid archives and one corrupt file
        let now = Utc::now();
        let file1 = paths.backups_dir.join("personal-notepad-backup-1.pnbackup");
        create_test_archive(&file1, BackupType::Manual, now - Duration::hours(1), 5);

        let file2 = paths.backups_dir.join("personal-notepad-auto-backup-2.pnbackup");
        create_test_archive(&file2, BackupType::Automatic, now + Duration::hours(1), 10);

        // Corrupt archive (non-zip text file)
        let corrupt_file = paths.backups_dir.join("corrupt_backup.pnbackup");
        fs::write(&corrupt_file, b"This is not a zip file").unwrap();

        // 3. List backups
        let list = BackupListService::list_backups(&conn, &paths).unwrap();
        assert_eq!(list.len(), 3);

        // Verify valid automatic item
        let auto_item = list.iter().find(|i| i.file_name == "personal-notepad-auto-backup-2.pnbackup").unwrap();
        assert_eq!(auto_item.backup_type, BackupType::Automatic);
        assert_eq!(auto_item.note_count, Some(10));
        assert!(auto_item.is_valid);
        assert!(!auto_item.is_verified); // Unverified initially

        // Verify valid manual item
        let manual_item = list.iter().find(|i| i.file_name == "personal-notepad-backup-1.pnbackup").unwrap();
        assert_eq!(manual_item.backup_type, BackupType::Manual);
        assert_eq!(manual_item.note_count, Some(5));
        assert!(manual_item.is_valid);

        // Corrupt entry is handled gracefully with is_valid: false and issue_summary
        let corrupt_item = list.iter().find(|i| i.file_name == "corrupt_backup.pnbackup").unwrap();
        assert!(!corrupt_item.is_valid);
        assert!(corrupt_item.issue_summary.is_some());

        // 4. Safe deletion of managed backup
        let delete_res = BackupListService::delete_managed_backup(&conn, &paths, &file1.to_string_lossy()).unwrap();
        assert!(delete_res);
        assert!(!file1.exists());

        // Re-listing reflects deletion
        let list_after_delete = BackupListService::list_backups(&conn, &paths).unwrap();
        assert_eq!(list_after_delete.len(), 2);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_managed_backup_safety_guards() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_bk_del_guards_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Attempting to delete active database must be rejected
        let db_err = BackupListService::delete_managed_backup(
            &conn,
            &paths,
            &paths.database_file.to_string_lossy(),
        ).unwrap_err();
        match db_err {
            BackupError::PermissionDenied(msg) => assert!(msg.contains("active database")),
            _ => panic!("Expected PermissionDenied on active database"),
        }

        // 2. Attempting to delete from attachments must be rejected
        let dummy_att = paths.attachments_dir.join("sample.pdf");
        fs::write(&dummy_att, b"pdf data").unwrap();
        let att_err = BackupListService::delete_managed_backup(
            &conn,
            &paths,
            &dummy_att.to_string_lossy(),
        ).unwrap_err();
        match att_err {
            BackupError::PermissionDenied(msg) => assert!(msg.contains("attachments")),
            _ => panic!("Expected PermissionDenied on attachments"),
        }

        // 3. Attempting to delete file outside backup directories must be rejected
        let outside_file = temp_dir.join("outside_file.pnbackup");
        fs::write(&outside_file, b"data").unwrap();
        let outside_err = BackupListService::delete_managed_backup(
            &conn,
            &paths,
            &outside_file.to_string_lossy(),
        ).unwrap_err();
        match outside_err {
            BackupError::PermissionDenied(msg) => assert!(msg.contains("designated backup directories")),
            _ => panic!("Expected PermissionDenied on outside file"),
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
