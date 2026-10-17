use rusqlite::Connection;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::storage::errors::BackupError;
use crate::storage::models::{
    BackupCreateRequest, BackupCreateResult, BackupProgressEvent, BackupProgressStage, BackupType,
    PRIMARY_BACKUP_EXTENSION,
};
use crate::storage::paths::StoragePaths;
use crate::storage::services::attachment_capture::AttachmentCaptureManager;
use crate::storage::services::backup_destination::BackupDestinationManager;
use crate::storage::services::backup_retention::BackupRetentionManager;
use crate::storage::services::backup_settings::BackupSettingsService;
use crate::storage::services::manifest::ManifestManager;
use crate::storage::services::snapshot::SnapshotManager;

/// Default filename timestamp format (e.g. "personal-notepad-backup-20261013-173000.pnbackup")
pub fn generate_default_backup_filename(backup_type: BackupType) -> String {
    let now = chrono::Utc::now();
    let prefix = match backup_type {
        BackupType::Manual => "backup",
        BackupType::Automatic => "auto-backup",
        BackupType::Checkpoint => "checkpoint",
    };
    format!(
        "personal-notepad-{}-{}.{}",
        prefix,
        now.format("%Y%m%d-%H%M%S"),
        PRIMARY_BACKUP_EXTENSION
    )
}

/// Core service that orchestrates complete backup creation, packaging, and verification (Phase 11, Task 8).
pub struct BackupService;

impl BackupService {
    /// Creates a complete, verified manual or automatic backup of Personal Notepad data.
    ///
    /// The pipeline performs:
    /// 1. Destination path resolution & validation.
    /// 2. Consistent SQLite database snapshot using SQLite Online Backup API.
    /// 3. Cross-validated attachment capture & cryptographic hashing.
    /// 4. Authoritative manifest generation (`manifest.json`).
    /// 5. Streaming ZIP compression into a safe temporary file (`.tmp.<uuid>`).
    /// 6. Immediate post-write read-back verification against the manifest.
    /// 7. Atomic rename to the final target destination.
    /// 8. Temporary artifacts cleanup.
    pub fn create_backup<F>(
        conn: &Connection,
        paths: &StoragePaths,
        request: BackupCreateRequest,
        mut progress_cb: Option<F>,
    ) -> Result<BackupCreateResult, BackupError>
    where
        F: FnMut(BackupProgressEvent),
    {
        let backup_type = request.backup_type.unwrap_or(BackupType::Manual);

        let mut report_progress = |stage: BackupProgressStage,
                                   msg: &str,
                                   current: Option<usize>,
                                   total: Option<usize>,
                                   pct: Option<f32>| {
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage,
                    message: msg.to_string(),
                    current_item: current,
                    total_items: total,
                    percentage: pct,
                    cancellable: false,
                    operation_id: None,
                });
            }
        };

        report_progress(
            BackupProgressStage::Starting,
            "Initializing backup process...",
            None,
            None,
            Some(5.0),
        );

        // 1. Resolve final destination path safely (Task 9)
        let allow_overwrite = request.allow_overwrite.unwrap_or(false);
        let target_file_path: PathBuf = BackupDestinationManager::resolve_destination(
            request.destination_path.as_deref(),
            backup_type,
            allow_overwrite,
            paths,
        )?;

        let target_dir = target_file_path.parent().unwrap_or(&paths.backups_dir);
        if !target_dir.exists() {
            fs::create_dir_all(target_dir).map_err(BackupError::Io)?;
        }

        let temp_archive_path = target_dir.join(format!(
            "{}.tmp.{}",
            target_file_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "backup.pnbackup".to_string()),
            Uuid::new_v4()
        ));

        // Staging directory for temporary database snapshot
        let temp_staging_dir = paths
            .backups_dir
            .join(format!(".staging_{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_staging_dir).map_err(BackupError::Io)?;
        let temp_db_snapshot_path = temp_staging_dir.join("database.sqlite");

        struct BackupTempGuard<'a> {
            temp_archive: &'a PathBuf,
            temp_staging: &'a PathBuf,
            committed: bool,
        }

        impl<'a> Drop for BackupTempGuard<'a> {
            fn drop(&mut self) {
                if !self.committed && self.temp_archive.exists() {
                    let _ = fs::remove_file(self.temp_archive);
                }
                if self.temp_staging.exists() {
                    let _ = fs::remove_dir_all(self.temp_staging);
                }
            }
        }

        let mut temp_guard = BackupTempGuard {
            temp_archive: &temp_archive_path,
            temp_staging: &temp_staging_dir,
            committed: false,
        };

        let cleanup_temp_files = || {
            if temp_archive_path.exists() {
                let _ = fs::remove_file(&temp_archive_path);
            }
            if temp_staging_dir.exists() {
                let _ = fs::remove_dir_all(&temp_staging_dir);
            }
        };

        // 2. Consistent SQLite database snapshot (Task 5)
        report_progress(
            BackupProgressStage::SnapshottingDatabase,
            "Creating consistent database snapshot...",
            None,
            None,
            Some(15.0),
        );

        let db_info = match SnapshotManager::create_snapshot(conn, &temp_db_snapshot_path) {
            Ok(info) => info,
            Err(e) => {
                cleanup_temp_files();
                return Err(e);
            }
        };

        // 3. Capture attachments and cross-check with database (Task 6)
        report_progress(
            BackupProgressStage::CollectingAttachments,
            "Collecting and cross-checking attachments...",
            None,
            None,
            Some(35.0),
        );

        let (captured_attachments, attachments_summary) =
            match AttachmentCaptureManager::capture_attachments(conn, paths) {
                Ok(res) => res,
                Err(e) => {
                    cleanup_temp_files();
                    return Err(e);
                }
            };

        // 4. Build Authoritative Backup Manifest (Task 7 & Task 19)
        report_progress(
            BackupProgressStage::GeneratingManifest,
            "Generating backup manifest and cryptographic integrity inventory...",
            None,
            None,
            Some(50.0),
        );

        let (is_encrypted, encryption_algorithm) = ManifestManager::detect_encryption_from_db(conn);

        let manifest = ManifestManager::build_manifest(
            backup_type,
            db_info.clone(),
            &captured_attachments,
            attachments_summary,
            is_encrypted,
            encryption_algorithm,
        );

        // 5. Stream and write ZIP archive
        report_progress(
            BackupProgressStage::WritingArchive,
            "Writing and compressing backup archive...",
            None,
            None,
            Some(60.0),
        );

        let write_archive = || -> Result<(), BackupError> {
            let archive_file = File::create(&temp_archive_path).map_err(BackupError::Io)?;
            let mut zip = ZipWriter::new(BufWriter::new(archive_file));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

            // Add manifest.json
            zip.start_file("manifest.json", options).map_err(|e| {
                BackupError::ArchiveWriteFailed(format!(
                    "Failed to start manifest.json in archive: {e}"
                ))
            })?;
            let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|e| {
                BackupError::ArchiveWriteFailed(format!("Failed to serialize manifest: {e}"))
            })?;
            zip.write_all(&manifest_json).map_err(BackupError::Io)?;

            // Add database.sqlite snapshot
            zip.start_file("database.sqlite", options).map_err(|e| {
                BackupError::ArchiveWriteFailed(format!(
                    "Failed to start database.sqlite in archive: {e}"
                ))
            })?;
            let mut db_file =
                BufReader::new(File::open(&temp_db_snapshot_path).map_err(BackupError::Io)?);
            let mut buffer = [0u8; 65536];
            loop {
                let n = db_file.read(&mut buffer).map_err(BackupError::Io)?;
                if n == 0 {
                    break;
                }
                zip.write_all(&buffer[..n]).map_err(BackupError::Io)?;
            }

            // Add attachment files
            for captured in &captured_attachments {
                zip.start_file(&captured.archive_entry_path, options)
                    .map_err(|e| {
                        BackupError::ArchiveWriteFailed(format!(
                            "Failed to add '{}': {e}",
                            captured.archive_entry_path
                        ))
                    })?;

                let mut att_file =
                    BufReader::new(File::open(&captured.source_path).map_err(BackupError::Io)?);
                loop {
                    let n = att_file.read(&mut buffer).map_err(BackupError::Io)?;
                    if n == 0 {
                        break;
                    }
                    zip.write_all(&buffer[..n]).map_err(BackupError::Io)?;
                }
            }

            zip.finish().map_err(|e| {
                BackupError::ArchiveWriteFailed(format!("Failed to finalize archive: {e}"))
            })?;

            Ok(())
        };

        if let Err(e) = write_archive() {
            cleanup_temp_files();
            return Err(e);
        }

        // 6. Post-write read-back verification (Task 7)
        report_progress(
            BackupProgressStage::VerifyingArchive,
            "Performing post-write read-back verification...",
            None,
            None,
            Some(85.0),
        );

        let validation_res = match ManifestManager::validate_archive(&temp_archive_path) {
            Ok(res) => res,
            Err(e) => {
                cleanup_temp_files();
                return Err(e);
            }
        };

        if !validation_res.is_valid {
            cleanup_temp_files();
            let summary_issue = validation_res
                .issues
                .into_iter()
                .map(|i| i.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(BackupError::ValidationFailed(format!(
                "Backup archive failed post-creation validation check: {summary_issue}"
            )));
        }

        // 7. Atomic commit of final archive file
        if target_file_path.exists() {
            let _ = fs::remove_file(&target_file_path);
        }
        match fs::rename(&temp_archive_path, &target_file_path) {
            Ok(_) => {
                temp_guard.committed = true;
            }
            Err(rename_err) => match fs::copy(&temp_archive_path, &target_file_path) {
                Ok(_) => {
                    let _ = fs::remove_file(&temp_archive_path);
                    temp_guard.committed = true;
                }
                Err(copy_err) => {
                    if target_file_path.exists() {
                        let _ = fs::remove_file(&target_file_path);
                    }
                    cleanup_temp_files();
                    if let Some(28) = copy_err.raw_os_error() {
                        return Err(BackupError::Io(std::io::Error::other(
                            "Insufficient disk space to complete backup.",
                        )));
                    } else {
                        return Err(BackupError::Io(rename_err));
                    }
                }
            },
        }

        // Clean up temporary staging directory
        let _ = fs::remove_dir_all(&temp_staging_dir);

        let final_metadata = fs::metadata(&target_file_path).map_err(BackupError::Io)?;
        let file_name = target_file_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "backup.pnbackup".to_string());

        // 8. Apply Retention Cleanup ONLY to automatic backups (Phase 11, Task 12)
        let mut cleanup_warning: Option<String> = None;
        if backup_type == BackupType::Automatic {
            report_progress(
                BackupProgressStage::ApplyingRetention,
                "Applying automatic backup retention policies...",
                None,
                None,
                Some(95.0),
            );

            let settings = BackupSettingsService::get_settings(conn).unwrap_or_default();
            let retention_res = BackupRetentionManager::apply_retention(
                target_dir,
                settings.retention_count as usize,
                Some(&target_file_path),
            );

            if !retention_res.warnings.is_empty() {
                cleanup_warning = Some(format!(
                    "Automatic retention cleanup encountered warnings: {}",
                    retention_res.warnings.join("; ")
                ));
            }
        }

        report_progress(
            BackupProgressStage::Completed,
            "Backup created and verified successfully.",
            None,
            None,
            Some(100.0),
        );

        Ok(BackupCreateResult {
            success: true,
            file_path: target_file_path.to_string_lossy().to_string(),
            file_name,
            file_size: final_metadata.len(),
            created_at: manifest.created_at,
            backup_type,
            verified: true,
            note_count: db_info.note_count + db_info.trash_count,
            attachment_count: captured_attachments.len(),
            warning_message: cleanup_warning,
            is_encrypted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use std::io::Write;
    use std::path::Path;

    #[test]
    fn test_task_8_manual_backup_creation_end_to_end() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_bk_svc_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("Ensure directories");

        let mut conn = init_connection(&paths.database_file).expect("Init connection");
        run_migrations(&mut conn).expect("Run migrations");

        // 1. Populate with notes and an attachment
        let now = chrono::Utc::now().to_rfc3339();
        let note_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_favorite, is_pinned, is_deleted)
             VALUES (?1, 'Test Note for Backup', 'Content body text', 'txt', ?2, ?2, 1, 0, 0)",
            rusqlite::params![note_id, now],
        ).expect("Insert note");

        let note_att_dir = paths
            .get_note_attachments_dir(&note_id)
            .expect("Get note att dir");
        let stored_name = format!("{}.txt", Uuid::new_v4());
        let att_file_path = note_att_dir.join(&stored_name);
        let mut f = File::create(&att_file_path).expect("Create attachment file");
        f.write_all(b"Attachment raw data content")
            .expect("Write attachment bytes");
        drop(f);

        let att_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, ?2, 'notes.txt', ?3, 'text/plain', 27, ?4, ?4)",
            rusqlite::params![att_id, note_id, format!("{note_id}/{stored_name}"), now],
        ).expect("Insert attachment");

        // 2. Execute BackupService::create_backup
        let mut progress_events = Vec::new();
        let request = BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: None,
        };

        let result = BackupService::create_backup(
            &conn,
            &paths,
            request,
            Some(|evt: BackupProgressEvent| {
                progress_events.push(evt.stage);
            }),
        )
        .expect("create_backup should succeed");

        assert!(result.success);
        assert!(result.verified);
        assert_eq!(result.note_count, 1);
        assert_eq!(result.attachment_count, 1);
        assert!(Path::new(&result.file_path).is_file());
        assert!(!progress_events.is_empty());
        assert!(progress_events.contains(&BackupProgressStage::Starting));
        assert!(progress_events.contains(&BackupProgressStage::Completed));

        // 3. Validate archive independently using ManifestManager
        let val = ManifestManager::validate_archive(Path::new(&result.file_path))
            .expect("Validate archive");
        assert!(val.is_valid);
        assert_eq!(val.manifest.as_ref().unwrap().attachments.len(), 1);
        assert_eq!(val.manifest.as_ref().unwrap().database.note_count, 1);

        // 4. Verify live database is still active and unmodified
        let live_check: String = conn
            .query_row(
                "SELECT title FROM notes WHERE id = ?1",
                rusqlite::params![note_id],
                |r| r.get(0),
            )
            .expect("Query live db");
        assert_eq!(live_check, "Test Note for Backup");

        // Clean up
        drop(conn);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
