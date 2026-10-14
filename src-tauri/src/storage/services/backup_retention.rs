use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};

use crate::storage::models::{BackupType, PRIMARY_BACKUP_EXTENSION, BACKUP_FORMAT_IDENTIFIER};
use crate::storage::services::manifest::ManifestManager;

/// Represents an eligible, verified automatic backup candidate discovered on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedAutomaticBackupCandidate {
    pub path: PathBuf,
    pub filename: String,
    pub created_at: DateTime<Utc>,
}

/// Structured outcome of a retention cleanup operation (Phase 11, Task 12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionCleanupResult {
    pub total_managed_found: usize,
    pub kept_count: usize,
    pub deleted_paths: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

/// Service managing the discovery, evaluation, and safe cleanup of automatic backup files (Phase 11, Task 12).
pub struct BackupRetentionManager;

impl BackupRetentionManager {
    /// Scans a directory for eligible managed automatic backups.
    ///
    /// Safety Invariants:
    /// 1. Only checks files with valid backup extensions (`.pnbackup` or `.zip`).
    /// 2. Never follows symbolic links.
    /// 3. Reads and validates the archive's internal `manifest.json`.
    /// 4. Confirms `manifest.format_identifier == "personal-notepad-backup"`.
    /// 5. Strictly confirms `manifest.backup_type == BackupType::Automatic`.
    ///    Manual backups and recovery checkpoints are NEVER considered eligible for automated cleanup.
    /// 6. Corrupt, unreadable, or foreign archives are ignored and never touched.
    pub fn find_eligible_automatic_backups(dir: &Path) -> Vec<ManagedAutomaticBackupCandidate> {
        let mut candidates = Vec::new();

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return candidates,
        };

        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            // Defense: never follow or delete through symlinks, never inspect subdirectories
            if file_type.is_symlink() || !file_type.is_file() {
                continue;
            }

            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext != PRIMARY_BACKUP_EXTENSION && ext != "zip" {
                // Never touch unrelated files (.txt, .sqlite, .docx, images, etc.)
                continue;
            }

            let filename = match path.file_name() {
                Some(name) => name.to_string_lossy().to_string(),
                None => continue,
            };

            // Inspect internal manifest for authoritative metadata
            let manifest = match ManifestManager::read_manifest_quick(&path) {
                Ok(m) => m,
                Err(_) => continue, // Corrupt, invalid, or non-zip file: leave intact
            };

            // Verify identity: must be Personal Notepad backup
            if manifest.format_identifier != BACKUP_FORMAT_IDENTIFIER {
                continue;
            }

            // Verify type: ONLY automatic backups are eligible for automatic retention!
            if manifest.backup_type != BackupType::Automatic {
                continue;
            }

            // Determine creation timestamp from manifest (fallback to file modification time)
            let created_at = match DateTime::parse_from_rfc3339(&manifest.created_at) {
                Ok(dt) => dt.with_timezone(&Utc),
                Err(_) => {
                    let fallback_mtime = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(DateTime::<Utc>::from)
                        .unwrap_or_else(Utc::now);
                    fallback_mtime
                }
            };

            candidates.push(ManagedAutomaticBackupCandidate {
                path,
                filename,
                created_at,
            });
        }

        candidates
    }

    /// Evaluates existing automatic backups and safely removes the oldest backups exceeding `retention_count`.
    ///
    /// Invariants:
    /// - `retention_count` specifies the total number of automatic backups to retain (minimum 1).
    /// - Sorts eligible automatic backups newest-to-oldest by creation timestamp.
    /// - `newly_created_path`: The path of the newly committed and verified backup, which is guaranteed
    ///   never to be deleted under any circumstances.
    /// - Failures to delete an old file do not fail the overall operation; warnings are recorded.
    pub fn apply_retention(
        dir: &Path,
        retention_count: usize,
        newly_created_path: Option<&Path>,
    ) -> RetentionCleanupResult {
        let mut candidates = Self::find_eligible_automatic_backups(dir);
        let total_managed_found = candidates.len();

        if total_managed_found == 0 {
            return RetentionCleanupResult {
                total_managed_found: 0,
                kept_count: 0,
                deleted_paths: Vec::new(),
                warnings: Vec::new(),
            };
        }

        // Sort descending: newest first, oldest last
        candidates.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // If total eligible backups is within retention bound, nothing to prune
        if candidates.len() <= retention_count {
            return RetentionCleanupResult {
                total_managed_found,
                kept_count: candidates.len(),
                deleted_paths: Vec::new(),
                warnings: Vec::new(),
            };
        }

        let mut deleted_paths = Vec::new();
        let mut warnings = Vec::new();

        // Candidates to keep: [0 .. retention_count]
        // Candidates to prune: [retention_count .. len]
        for candidate in &candidates[retention_count..] {
            // Absolute safety check: Never delete the newly created backup
            if let Some(new_path) = newly_created_path {
                if candidate.path == new_path {
                    continue;
                }
            }

            match fs::remove_file(&candidate.path) {
                Ok(()) => {
                    deleted_paths.push(candidate.path.clone());
                }
                Err(e) => {
                    warnings.push(format!(
                        "Failed to delete old automatic backup '{}': {e}",
                        candidate.filename
                    ));
                }
            }
        }

        let kept_count = total_managed_found - deleted_paths.len();

        RetentionCleanupResult {
            total_managed_found,
            kept_count,
            deleted_paths,
            warnings,
        }
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
    use crate::storage::models::{
        BackupAttachmentsSummary, BackupDatabaseInfo, BackupManifest, BackupType,
        CURRENT_BACKUP_FORMAT_VERSION,
    };

    /// Helper creating a minimal valid mock backup archive on disk
    fn create_mock_backup_archive(
        path: &Path,
        backup_type: BackupType,
        created_at: DateTime<Utc>,
    ) {
        let file = fs::File::create(path).expect("Create mock backup file");
        let mut zip = ZipWriter::new(file);

        let manifest = BackupManifest {
            format_identifier: BACKUP_FORMAT_IDENTIFIER.to_string(),
            format_version: CURRENT_BACKUP_FORMAT_VERSION,
            app_version: "0.1.0".to_string(),
            created_at: created_at.to_rfc3339(),
            backup_type,
            database: BackupDatabaseInfo {
                entry_path: "database.sqlite".to_string(),
                file_size: 100,
                sha256: "dummy-hash".to_string(),
                schema_version: 4,
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
            scope: crate::storage::models::BackupScopeDeclaration {
                includes_settings: true,
                includes_trash: true,
                includes_attachments: true,
            },
            checksums: std::collections::HashMap::new(),
        };

        let json = serde_json::to_vec_pretty(&manifest).expect("Serialize manifest");
        let options = SimpleFileOptions::default();
        zip.start_file("manifest.json", options).expect("Start manifest file");
        zip.write_all(&json).expect("Write manifest bytes");
        zip.finish().expect("Finish zip");
    }

    #[test]
    fn test_retention_prunes_oldest_excess_automatic_backups() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_retention_{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("Create temp dir");

        let base_time = Utc::now();

        // Create 5 automatic backups with different timestamps
        let mut file_paths = Vec::new();
        for i in 0..5 {
            let file_time = base_time - Duration::hours(5 - i); // oldest to newest
            let file_path = temp_dir.join(format!("personal-notepad-auto-backup-{i}.pnbackup"));
            create_mock_backup_archive(&file_path, BackupType::Automatic, file_time);
            file_paths.push(file_path);
        }

        // Apply retention = 3 (should keep 3 newest, delete 2 oldest)
        let newest_path = &file_paths[4];
        let result = BackupRetentionManager::apply_retention(&temp_dir, 3, Some(newest_path));

        assert_eq!(result.total_managed_found, 5);
        assert_eq!(result.kept_count, 3);
        assert_eq!(result.deleted_paths.len(), 2);
        assert!(result.warnings.is_empty());

        // Oldest two (0 and 1) should be deleted
        assert!(!file_paths[0].exists(), "Oldest backup 0 must be pruned");
        assert!(!file_paths[1].exists(), "Second oldest backup 1 must be pruned");

        // Newest three (2, 3, and 4) must still exist
        assert!(file_paths[2].exists(), "Backup 2 must be kept");
        assert!(file_paths[3].exists(), "Backup 3 must be kept");
        assert!(file_paths[4].exists(), "Newest backup 4 must be kept");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_retention_never_touches_manual_backups_or_checkpoints() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_retention_manual_{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("Create temp dir");

        let base_time = Utc::now();

        // 1. Create a manual backup (very old)
        let manual_path = temp_dir.join("personal-notepad-backup-manual.pnbackup");
        create_mock_backup_archive(&manual_path, BackupType::Manual, base_time - Duration::days(100));

        // 2. Create a checkpoint backup (old)
        let checkpoint_path = temp_dir.join("personal-notepad-checkpoint.pnbackup");
        create_mock_backup_archive(&checkpoint_path, BackupType::Checkpoint, base_time - Duration::days(50));

        // 3. Create 3 automatic backups
        let mut auto_paths = Vec::new();
        for i in 0..3 {
            let auto_path = temp_dir.join(format!("personal-notepad-auto-backup-{i}.pnbackup"));
            create_mock_backup_archive(&auto_path, BackupType::Automatic, base_time - Duration::hours(3 - i));
            auto_paths.push(auto_path);
        }

        // Apply retention = 2 on automatic backups
        let result = BackupRetentionManager::apply_retention(&temp_dir, 2, None);

        assert_eq!(result.total_managed_found, 3, "Only automatic backups are counted");
        assert_eq!(result.kept_count, 2);
        assert_eq!(result.deleted_paths.len(), 1);

        // Verify: Manual backup and checkpoint are completely untouched!
        assert!(manual_path.exists(), "Manual backups must NEVER be deleted by automatic retention");
        assert!(checkpoint_path.exists(), "Checkpoints must NEVER be deleted by automatic retention");

        // Auto backup 0 deleted, auto backups 1 and 2 kept
        assert!(!auto_paths[0].exists());
        assert!(auto_paths[1].exists());
        assert!(auto_paths[2].exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_retention_never_deletes_unrelated_files_or_arbitrary_data() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_retention_unrelated_{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("Create temp dir");

        // Create arbitrary user files
        let doc_file = temp_dir.join("financial_report.docx");
        fs::write(&doc_file, b"User confidential data").unwrap();

        let db_file = temp_dir.join("database.sqlite");
        fs::write(&db_file, b"SQLite format 3").unwrap();

        let txt_file = temp_dir.join("notes.txt");
        fs::write(&txt_file, b"Plain text note").unwrap();

        let foreign_zip = temp_dir.join("foreign_archive.zip");
        {
            let f = fs::File::create(&foreign_zip).unwrap();
            let mut zip = ZipWriter::new(f);
            zip.start_file("some_file.txt", SimpleFileOptions::default()).unwrap();
            zip.write_all(b"Hello").unwrap();
            zip.finish().unwrap();
        }

        // Apply retention = 1
        let result = BackupRetentionManager::apply_retention(&temp_dir, 1, None);
        assert_eq!(result.total_managed_found, 0);
        assert_eq!(result.deleted_paths.len(), 0);

        // Every unrelated file must be preserved
        assert!(doc_file.exists());
        assert!(db_file.exists());
        assert!(txt_file.exists());
        assert!(foreign_zip.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(unix)]
    #[test]
    fn test_retention_never_follows_symbolic_links() {
        use std::os::unix::fs::symlink;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_retention_symlink_{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("Create temp dir");

        // Target file in another directory
        let target_dir = std::env::temp_dir().join(format!("pn_test_symlink_target_{}", Uuid::new_v4()));
        fs::create_dir_all(&target_dir).unwrap();
        let target_file = target_dir.join("target_backup.pnbackup");
        create_mock_backup_archive(&target_file, BackupType::Automatic, Utc::now() - Duration::days(10));

        // Create symlink inside the retention directory pointing to target_file
        let symlink_path = temp_dir.join("personal-notepad-auto-backup-symlink.pnbackup");
        symlink(&target_file, &symlink_path).unwrap();

        // Apply retention = 0 (prune all eligible)
        let result = BackupRetentionManager::apply_retention(&temp_dir, 0, None);

        // Symlink must be ignored, target file must NEVER be deleted
        assert_eq!(result.total_managed_found, 0);
        assert!(target_file.exists(), "Target file pointed to by symlink must NEVER be touched");

        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&target_dir);
    }
}
