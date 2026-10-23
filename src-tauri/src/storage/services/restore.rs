use rusqlite::Connection;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

fn cancelled_restores_set() -> &'static Mutex<HashSet<String>> {
    static CANCELLED_RESTORES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    CANCELLED_RESTORES.get_or_init(|| Mutex::new(HashSet::new()))
}

use crate::storage::database::init_connection;
use crate::storage::errors::BackupError;
use crate::storage::models::{
    BackupCompatibilityStatus, BackupManifest, BackupProgressEvent, BackupProgressStage,
    BackupRestoreRequest, BackupRestoreResult, BackupType, PostRestoreValidationReport,
    RecoveryCheckpoint, RestoreActivationMarker, RestoreActivationPhase, RestorePreviewResult,
    StagedMigrationResult, StagedRestoreSummary, PRIMARY_BACKUP_EXTENSION,
};
use crate::storage::paths::StoragePaths;
use crate::storage::services::attachment_capture::AttachmentCaptureManager;
use crate::storage::services::backup_destination::BackupDestinationManager;
use crate::storage::services::backup_scheduler::BackupSchedulerService;
use crate::storage::services::backup_validation::BackupValidationService;
use crate::storage::services::links::LinkParser;
use crate::storage::services::manifest::{ManifestManager, CURRENT_APP_SCHEMA_VERSION};
use crate::storage::services::search::SearchService;
use crate::storage::services::snapshot::{compute_file_sha256, SnapshotManager};

/// Service responsible for orchestrating safe restore operations, pre-restore previews,
/// staging verification, checkpoints, and controlled activation (Phase 11, Tasks 15-19).
pub struct RestoreService;

impl RestoreService {
    /// Maximum number of pre-restore recovery checkpoints retained automatically.
    pub const MAX_RETAINED_CHECKPOINTS: usize = 5;

    /// Maximum allowed entries in candidate archive during staged extraction (decompression bomb protection, Task 17).
    pub const MAX_RESTORE_ENTRY_COUNT: usize = 10_000;

    /// Maximum total decompressed size across all entries (5 GiB limit, Task 17).
    pub const MAX_RESTORE_DECOMPRESSED_BYTES: u64 = 5 * 1024 * 1024 * 1024;

    /// Maximum allowed decompressed size for a single entry file (2 GiB limit, Task 17).
    pub const MAX_RESTORE_SINGLE_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

    /// Requests cancellation of an in-flight restore operation by registering its operation_id (Phase 11, Task 21).
    pub fn request_cancellation(operation_id: &str) -> bool {
        if let Ok(mut set) = cancelled_restores_set().lock() {
            set.insert(operation_id.to_string());
            true
        } else {
            false
        }
    }

    /// Checks whether a restore operation has received a cancellation request (Phase 11, Task 21).
    pub fn is_cancellation_requested(operation_id: Option<&str>) -> bool {
        if let Some(id) = operation_id {
            if let Ok(set) = cancelled_restores_set().lock() {
                return set.contains(id);
            }
        }
        false
    }

    /// Clears any registered cancellation marker for a restore operation once finalized (Phase 11, Task 21).
    pub fn clear_cancellation(operation_id: Option<&str>) {
        if let Some(id) = operation_id {
            if let Ok(mut set) = cancelled_restores_set().lock() {
                set.remove(id);
            }
        }
    }

    /// Generates a comprehensive, non-destructive restore preview for a candidate backup archive (Task 15).
    ///
    /// Safety Invariants:
    /// 1. Revalidates the backup archive immediately using `BackupValidationService`.
    /// 2. Does not touch or modify the active live database or active attachments directory.
    /// 3. Extracts accurate counts of active notes, trash, notebooks, tags, and attachments.
    /// 4. Determines compatibility status (`Compatible`, `RequiresMigration`, `IncompatibleVersion`, `Corrupted`).
    /// 5. Collects all validation warnings and errors for transparent user disclosure.
    /// 6. Sets `can_restore = false` if validation fails or schema/format is incompatible.
    pub fn preview_restore(
        archive_path: &Path,
        paths: &StoragePaths,
    ) -> Result<RestorePreviewResult, BackupError> {
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
                "Cannot preview restore targeting active live database or journals.".to_string(),
            ));
        }

        // 1. Revalidate candidate backup archive
        let val_res = BackupValidationService::validate_backup(archive_path, paths)?;

        let file_name = archive_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "backup.pnbackup".to_string());
        let file_size = fs::metadata(archive_path).map(|m| m.len()).unwrap_or(0);

        // 2. Extract metadata from manifest or fallback
        let (
            note_count,
            trash_count,
            notebook_count,
            tag_count,
            attachment_count,
            attachment_total_bytes,
            includes_settings,
            created_at,
            backup_type,
            format_version,
            schema_version,
            is_encrypted,
            encryption_algorithm,
        ) = if let Some(ref m) = val_res.manifest {
            (
                m.database.note_count,
                m.database.trash_count,
                m.database.notebook_count,
                m.database.tag_count,
                m.attachments_summary.total_files,
                m.attachments_summary.total_bytes,
                m.scope.includes_settings,
                m.created_at.clone(),
                m.backup_type,
                m.format_version,
                m.database.schema_version,
                m.is_encrypted,
                m.encryption_algorithm.clone(),
            )
        } else {
            let fallback_mtime = fs::metadata(archive_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(chrono::DateTime::<chrono::Utc>::from)
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339();

            (
                0,
                0,
                0,
                0,
                0,
                0,
                false,
                fallback_mtime,
                BackupType::Manual,
                0,
                0,
                false,
                None,
            )
        };

        // 3. Assemble disclosure warnings and issues
        let mut warnings = Vec::new();
        for issue in &val_res.issues {
            warnings.push(issue.message.clone());
        }

        // 4. Calculate can_restore status
        let can_restore = val_res.is_valid
            && val_res.compatibility != BackupCompatibilityStatus::IncompatibleVersion
            && val_res.compatibility != BackupCompatibilityStatus::Corrupted;

        Ok(RestorePreviewResult {
            file_path: archive_path.to_string_lossy().to_string(),
            file_name,
            file_size,
            created_at,
            backup_type,
            format_version,
            schema_version,
            note_count,
            notebook_count,
            tag_count,
            trash_count,
            attachment_count,
            attachment_total_bytes,
            includes_settings,
            compatibility: val_res.compatibility,
            warnings,
            can_restore,
            is_encrypted,
            encryption_algorithm,
        })
    }

    /// Creates a pre-restore recovery checkpoint capturing the currently working live database
    /// and attachments before any destructive restoration changes begin (Phase 11, Task 16).
    ///
    /// Safety Guarantees & Invariants:
    /// 1. Transactionally consistent snapshot of SQLite database using SQLite Online Backup API.
    /// 2. Complete cross-validated capture of live attachment files.
    /// 3. Authoritative manifest with `backup_type == BackupType::Checkpoint`.
    /// 4. Atomic archive creation (`.tmp.<uuid>` commit via atomic rename).
    /// 5. Immediate post-write read-back verification against manifest and SQLite integrity check.
    /// 6. Stored in durable `<userData>/backups/checkpoints/` directory (never in /tmp).
    /// 7. Collision-free naming prevents overwriting existing valid checkpoints.
    /// 8. If creation or verification fails, aborts immediately without touching active live data.
    /// 9. Prunes excess checkpoints according to retention policy (keeping newest 5).
    pub fn create_recovery_checkpoint(
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<RecoveryCheckpoint, BackupError> {
        // 1. Acquire execution lock (with retry wait for concurrent operations to clear)
        let mut acquired = None;
        for _ in 0..100 {
            if let Ok(guard) = BackupSchedulerService::try_acquire_backup_lock() {
                acquired = Some(guard);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let _lock_guard = match acquired {
            Some(g) => g,
            None => {
                return Err(BackupError::ConcurrentOperation(
                    "Another backup or restore operation is currently in progress.".to_string(),
                ));
            }
        };

        Self::create_recovery_checkpoint_internal(conn, paths)
    }

    /// Internal checkpoint creation implementation without acquiring BackupScheduler lock (Phase 11, Tasks 16 & 19).
    pub fn create_recovery_checkpoint_internal(
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<RecoveryCheckpoint, BackupError> {
        // 2. Ensure persistent checkpoints directory exists
        let checkpoints_dir = paths.checkpoints_dir();
        fs::create_dir_all(&checkpoints_dir).map_err(BackupError::Io)?;

        let now = chrono::Utc::now();
        let checkpoint_uuid = Uuid::new_v4();
        let default_filename = format!(
            "personal-notepad-checkpoint-{}.{}",
            now.format("%Y%m%d-%H%M%S"),
            PRIMARY_BACKUP_EXTENSION
        );

        let candidate_path = checkpoints_dir.join(&default_filename);
        let target_checkpoint_path = if candidate_path.exists() {
            BackupDestinationManager::find_collision_free_path(&checkpoints_dir, &candidate_path)?
        } else {
            candidate_path
        };

        // 3. Staging and temporary paths
        let temp_archive_path = checkpoints_dir.join(format!(
            "{}.tmp.{}",
            target_checkpoint_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "checkpoint.pnbackup".to_string()),
            checkpoint_uuid
        ));

        let temp_staging_dir = paths
            .backups_dir
            .join(format!(".chk_staging_{}", checkpoint_uuid));
        fs::create_dir_all(&temp_staging_dir).map_err(BackupError::Io)?;
        let temp_db_snapshot_path = temp_staging_dir.join("database.sqlite");

        let cleanup_temp = || {
            if temp_archive_path.exists() {
                let _ = fs::remove_file(&temp_archive_path);
            }
            if temp_staging_dir.exists() {
                let _ = fs::remove_dir_all(&temp_staging_dir);
            }
        };

        // 4. Consistent SQLite snapshot
        let db_info = match SnapshotManager::create_snapshot(conn, &temp_db_snapshot_path) {
            Ok(info) => info,
            Err(e) => {
                cleanup_temp();
                return Err(e);
            }
        };

        // 5. Capture attachments
        let (captured_attachments, attachments_summary) =
            match AttachmentCaptureManager::capture_attachments(conn, paths) {
                Ok(res) => res,
                Err(e) => {
                    cleanup_temp();
                    return Err(e);
                }
            };

        let (is_encrypted, encryption_algorithm) = ManifestManager::detect_encryption_from_db(conn);

        // 6. Generate authoritative manifest
        let manifest = ManifestManager::build_manifest(
            BackupType::Checkpoint,
            db_info,
            &captured_attachments,
            attachments_summary,
            is_encrypted,
            encryption_algorithm,
        );

        // 7. Write ZIP archive
        let write_archive = || -> Result<(), BackupError> {
            let archive_file = File::create(&temp_archive_path).map_err(BackupError::Io)?;
            let mut zip = ZipWriter::new(BufWriter::new(archive_file));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

            // manifest.json
            zip.start_file("manifest.json", options).map_err(|e| {
                BackupError::ArchiveWriteFailed(format!("Failed to start manifest: {e}"))
            })?;
            let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| {
                BackupError::ArchiveWriteFailed(format!("Failed to serialize manifest: {e}"))
            })?;
            zip.write_all(&manifest_bytes).map_err(BackupError::Io)?;

            // database.sqlite
            zip.start_file("database.sqlite", options).map_err(|e| {
                BackupError::ArchiveWriteFailed(format!("Failed to start database: {e}"))
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

            // attachments
            for captured in &captured_attachments {
                zip.start_file(&captured.archive_entry_path, options)
                    .map_err(|e| {
                        BackupError::ArchiveWriteFailed(format!(
                            "Failed to add attachment entry: {e}"
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
                BackupError::ArchiveWriteFailed(format!("Failed to finalize checkpoint zip: {e}"))
            })?;
            Ok(())
        };

        if let Err(e) = write_archive() {
            cleanup_temp();
            return Err(e);
        }

        // 8. Immediate read-back verification
        let val_res = match ManifestManager::validate_archive(&temp_archive_path) {
            Ok(v) => v,
            Err(e) => {
                cleanup_temp();
                return Err(e);
            }
        };

        if !val_res.is_valid {
            cleanup_temp();
            let issues_summary = val_res
                .issues
                .into_iter()
                .map(|i| i.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(BackupError::ValidationFailed(format!(
                "Recovery checkpoint failed post-write verification: {issues_summary}"
            )));
        }

        // 9. Atomic commit
        if target_checkpoint_path.exists() {
            let _ = fs::remove_file(&target_checkpoint_path);
        }
        if let Err(e) = fs::rename(&temp_archive_path, &target_checkpoint_path) {
            cleanup_temp();
            return Err(BackupError::Io(e));
        }

        // Cleanup temporary snapshot staging directory
        let _ = fs::remove_dir_all(&temp_staging_dir);

        // 10. Checkpoint retention policy: prune excess older checkpoints
        let _ = Self::prune_excess_checkpoints(&checkpoints_dir, Self::MAX_RETAINED_CHECKPOINTS);

        let file_size = fs::metadata(&target_checkpoint_path)
            .map(|m| m.len())
            .unwrap_or(0);
        let final_filename = target_checkpoint_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(default_filename);
        let checkpoint_id = format!("checkpoint-{}", final_filename);

        Ok(RecoveryCheckpoint {
            checkpoint_id,
            file_path: target_checkpoint_path.to_string_lossy().to_string(),
            file_name: final_filename,
            file_size,
            created_at: manifest.created_at,
            note_count: manifest.database.note_count,
            trash_count: manifest.database.trash_count,
            notebook_count: manifest.database.notebook_count,
            tag_count: manifest.database.tag_count,
            attachment_count: manifest.attachments_summary.total_files,
            attachment_total_bytes: manifest.attachments_summary.total_bytes,
            schema_version: manifest.database.schema_version,
            is_verified: true,
            is_encrypted: manifest.is_encrypted,
        })
    }

    /// Discovers and lists all pre-restore recovery checkpoints in descending chronological order (Task 16).
    pub fn list_recovery_checkpoints(
        paths: &StoragePaths,
    ) -> Result<Vec<RecoveryCheckpoint>, BackupError> {
        let checkpoints_dir = paths.checkpoints_dir();
        if !checkpoints_dir.exists() || !checkpoints_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut list = Vec::new();
        let entries = match fs::read_dir(&checkpoints_dir) {
            Ok(e) => e,
            Err(e) => return Err(BackupError::Io(e)),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext != PRIMARY_BACKUP_EXTENSION && ext != "zip" {
                continue;
            }
            let file_name = match path.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };
            if file_name.contains(".tmp.") {
                continue;
            }

            let file_size = entry.metadata().map(|m| m.len()).unwrap_or(0);

            if let Ok(manifest) = ManifestManager::read_manifest_quick(&path) {
                if manifest.backup_type == BackupType::Checkpoint {
                    list.push(RecoveryCheckpoint {
                        checkpoint_id: format!("checkpoint-{}", file_name),
                        file_path: path.to_string_lossy().to_string(),
                        file_name,
                        file_size,
                        created_at: manifest.created_at,
                        note_count: manifest.database.note_count,
                        trash_count: manifest.database.trash_count,
                        notebook_count: manifest.database.notebook_count,
                        tag_count: manifest.database.tag_count,
                        attachment_count: manifest.attachments_summary.total_files,
                        attachment_total_bytes: manifest.attachments_summary.total_bytes,
                        schema_version: manifest.database.schema_version,
                        is_verified: true,
                        is_encrypted: manifest.is_encrypted,
                    });
                }
            }
        }

        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(list)
    }

    /// Retrieves the most recently created recovery checkpoint, if one exists (Task 16).
    pub fn get_latest_recovery_checkpoint(
        paths: &StoragePaths,
    ) -> Result<Option<RecoveryCheckpoint>, BackupError> {
        let list = Self::list_recovery_checkpoints(paths)?;
        Ok(list.into_iter().next())
    }

    /// Prunes excess older checkpoints exceeding `max_keep` bound (Task 16 retention policy).
    pub fn prune_excess_checkpoints(
        checkpoints_dir: &Path,
        max_keep: usize,
    ) -> Result<Vec<PathBuf>, BackupError> {
        if !checkpoints_dir.exists() || !checkpoints_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut checkpoints = Vec::new();
        let entries = match fs::read_dir(checkpoints_dir) {
            Ok(e) => e,
            Err(e) => return Err(BackupError::Io(e)),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext != PRIMARY_BACKUP_EXTENSION && ext != "zip" {
                continue;
            }
            let file_name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if file_name.contains(".tmp.") {
                continue;
            }

            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::now());

            let created_at = ManifestManager::read_manifest_quick(&path)
                .ok()
                .and_then(|m| chrono::DateTime::parse_from_rfc3339(&m.created_at).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from(mtime));

            checkpoints.push((path, created_at));
        }

        checkpoints.sort_by_key(|checkpoint| std::cmp::Reverse(checkpoint.1));

        let mut deleted = Vec::new();
        if checkpoints.len() > max_keep {
            for (old_path, _) in checkpoints.into_iter().skip(max_keep) {
                if fs::remove_file(&old_path).is_ok() {
                    deleted.push(old_path);
                }
            }
        }

        Ok(deleted)
    }

    /// Independently validates a recovery checkpoint archive (Task 16).
    pub fn validate_recovery_checkpoint(
        checkpoint_path: &Path,
        paths: &StoragePaths,
    ) -> Result<crate::storage::models::BackupValidationResult, BackupError> {
        BackupValidationService::validate_backup(checkpoint_path, paths)
    }

    /// Strictly sanitizes an archive entry relative path against Zip-Slip, path traversal,
    /// absolute paths, and operating system hazards (Phase 11, Task 17).
    pub fn sanitize_archive_entry_path(entry_name: &str) -> Result<PathBuf, BackupError> {
        let trimmed = entry_name.trim();
        if trimmed.is_empty() {
            return Err(BackupError::ArchiveReadFailed(
                "Archive contains empty or whitespace-only entry name".to_string(),
            ));
        }
        if trimmed.contains('\0') {
            return Err(BackupError::ArchiveReadFailed(
                "Archive entry name contains forbidden null byte".to_string(),
            ));
        }

        let normalized = trimmed.replace('\\', "/");
        if normalized.starts_with('/') {
            return Err(BackupError::ArchiveReadFailed(format!(
                "Zip-Slip defense: absolute path rejected ('{entry_name}')"
            )));
        }

        // Reject Windows drive prefix (e.g. C:)
        if normalized.len() >= 2 && normalized.chars().nth(1) == Some(':') {
            return Err(BackupError::ArchiveReadFailed(format!(
                "Zip-Slip defense: drive prefix rejected ('{entry_name}')"
            )));
        }

        let path = Path::new(&normalized);
        for comp in path.components() {
            match comp {
                std::path::Component::ParentDir => {
                    return Err(BackupError::ArchiveReadFailed(format!(
                        "Zip-Slip defense: parent directory traversal '..' rejected ('{entry_name}')"
                    )));
                }
                std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                    return Err(BackupError::ArchiveReadFailed(format!(
                        "Zip-Slip defense: root/prefix component rejected ('{entry_name}')"
                    )));
                }
                std::path::Component::CurDir => {}
                std::path::Component::Normal(segment) => {
                    let seg_str = segment.to_string_lossy();
                    if seg_str.ends_with('.') || seg_str.ends_with(' ') {
                        return Err(BackupError::ArchiveReadFailed(format!(
                            "Unsafe trailing dot or space in entry name component ('{seg_str}')"
                        )));
                    }
                }
            }
        }

        Ok(PathBuf::from(normalized))
    }

    /// Extracts a candidate backup archive into an isolated, unique staging directory outside live data,
    /// enforcing Zip-Slip defenses, entry count quotas, decompression byte limits, duplicate rejection,
    /// symlink prohibition, and independent staged verification (Phase 11, Task 17).
    pub fn stage_restore_archive(
        archive_path: &Path,
        paths: &StoragePaths,
    ) -> Result<StagedRestoreSummary, BackupError> {
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
                "Cannot stage restore from active live database or journals.".to_string(),
            ));
        }

        paths
            .ensure_directories()
            .map_err(|e| BackupError::Io(std::io::Error::other(e.to_string())))?;

        let staging_id = Uuid::new_v4().to_string();
        let staging_dir = paths
            .backups_dir
            .join(format!(".restore_staging_{}", staging_id));
        fs::create_dir_all(&staging_dir).map_err(BackupError::Io)?;

        // Safe extraction with fail-fast cleanup
        let extract_result = (|| -> Result<(u64, usize), BackupError> {
            let file = File::open(archive_path).map_err(BackupError::Io)?;
            let mut archive = ZipArchive::new(file).map_err(|e| {
                BackupError::ArchiveReadFailed(format!("Failed to open zip archive: {e}"))
            })?;

            if archive.len() > Self::MAX_RESTORE_ENTRY_COUNT {
                return Err(BackupError::ArchiveReadFailed(format!(
                    "Archive entry count ({}) exceeds safety quota limit ({})",
                    archive.len(),
                    Self::MAX_RESTORE_ENTRY_COUNT
                )));
            }

            let mut extracted_names = HashSet::new();
            let mut total_decompressed: u64 = 0;
            let mut total_files: usize = 0;

            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).map_err(|e| {
                    BackupError::ArchiveReadFailed(format!(
                        "Failed to access archive entry index {i}: {e}"
                    ))
                })?;

                // Prohibit symbolic links
                let unix_mode = entry.unix_mode().unwrap_or(0);
                if (unix_mode & 0o170000) == 0o120000 {
                    return Err(BackupError::ArchiveReadFailed(format!(
                        "Prohibited symbolic link entry in backup archive: '{}'",
                        entry.name()
                    )));
                }

                let clean_rel = Self::sanitize_archive_entry_path(entry.name())?;

                // Prevent duplicate archive entries
                if !extracted_names.insert(clean_rel.clone()) {
                    return Err(BackupError::ArchiveReadFailed(format!(
                        "Duplicate archive entry detected: '{}'",
                        clean_rel.display()
                    )));
                }

                let dest_path = staging_dir.join(&clean_rel);
                if !dest_path.starts_with(&staging_dir) {
                    return Err(BackupError::ArchiveReadFailed(format!(
                        "Path escaping staging directory: '{}'",
                        clean_rel.display()
                    )));
                }

                if entry.is_dir() || clean_rel.to_string_lossy().ends_with('/') {
                    fs::create_dir_all(&dest_path).map_err(BackupError::Io)?;
                } else {
                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent).map_err(BackupError::Io)?;
                    }

                    let mut out_file =
                        BufWriter::new(File::create(&dest_path).map_err(BackupError::Io)?);
                    let mut buffer = [0u8; 65536];
                    let mut file_decompressed: u64 = 0;

                    loop {
                        let n = entry.read(&mut buffer).map_err(BackupError::Io)?;
                        if n == 0 {
                            break;
                        }
                        file_decompressed += n as u64;
                        total_decompressed += n as u64;

                        if file_decompressed > Self::MAX_RESTORE_SINGLE_FILE_BYTES {
                            return Err(BackupError::ArchiveReadFailed(format!(
                                "Single file decompressed size exceeded quota limit of {} bytes ('{}')",
                                Self::MAX_RESTORE_SINGLE_FILE_BYTES,
                                clean_rel.display()
                            )));
                        }

                        if total_decompressed > Self::MAX_RESTORE_DECOMPRESSED_BYTES {
                            return Err(BackupError::ArchiveReadFailed(format!(
                                "Total archive decompressed size exceeded quota limit of {} bytes",
                                Self::MAX_RESTORE_DECOMPRESSED_BYTES
                            )));
                        }

                        out_file.write_all(&buffer[..n]).map_err(BackupError::Io)?;
                    }

                    out_file.flush().map_err(BackupError::Io)?;
                    total_files += 1;
                }
            }

            Ok((total_decompressed, total_files))
        })();

        let (total_decompressed_bytes, total_extracted_files) = match extract_result {
            Ok(res) => res,
            Err(e) => {
                let _ = fs::remove_dir_all(&staging_dir);
                return Err(e);
            }
        };

        // Independent verification of extracted staged data
        match Self::validate_staged_data(
            &staging_dir,
            &staging_id,
            total_decompressed_bytes,
            total_extracted_files,
        ) {
            Ok(summary) => Ok(summary),
            Err(e) => {
                let _ = fs::remove_dir_all(&staging_dir);
                Err(e)
            }
        }
    }

    /// Independently validates staged SQLite database and attachment inventory against manifest (Task 17).
    pub fn validate_staged_data(
        staging_dir: &Path,
        staging_id: &str,
        total_extracted_bytes: u64,
        total_extracted_files: usize,
    ) -> Result<StagedRestoreSummary, BackupError> {
        let staged_db_path = staging_dir.join("database.sqlite");
        if !staged_db_path.exists() || !staged_db_path.is_file() {
            return Err(BackupError::ValidationFailed(
                "Staged archive does not contain 'database.sqlite'".to_string(),
            ));
        }

        // 1. Inspect database in read-only mode
        let conn = rusqlite::Connection::open_with_flags(
            &staged_db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(BackupError::Database)?;
        conn.pragma_update(None, "query_only", "ON")
            .map_err(BackupError::Database)?;

        // PRAGMA integrity_check
        let integrity_res: String = conn
            .query_row("PRAGMA integrity_check;", [], |r| r.get(0))
            .map_err(BackupError::Database)?;
        if integrity_res != "ok" {
            return Err(BackupError::ValidationFailed(format!(
                "Staged database integrity check failed: {integrity_res}"
            )));
        }

        // PRAGMA foreign_key_check
        {
            let mut fk_stmt = conn
                .prepare("PRAGMA foreign_key_check;")
                .map_err(BackupError::Database)?;
            let mut fk_rows = fk_stmt.query([]).map_err(BackupError::Database)?;
            if fk_rows.next().map_err(BackupError::Database)?.is_some() {
                return Err(BackupError::ValidationFailed(
                    "Staged database violates foreign key integrity constraints".to_string(),
                ));
            }
        }

        // Verify core required tables
        let required_tables = [
            "notes",
            "notebooks",
            "tags",
            "note_tags",
            "attachments",
            "_migrations",
        ];
        for table in required_tables {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .map_err(BackupError::Database)?;
            if count == 0 {
                return Err(BackupError::ValidationFailed(format!(
                    "Staged database is missing required table: '{table}'"
                )));
            }
        }

        let initial_schema_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let note_count: usize = conn
            .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 0", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize;
        let trash_count: usize = conn
            .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 1", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize;
        let notebook_count: usize = conn
            .query_row("SELECT COUNT(1) FROM notebooks", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize;
        let tag_count: usize = conn
            .query_row("SELECT COUNT(1) FROM tags", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize;
        let attachment_count: usize = conn
            .query_row("SELECT COUNT(1) FROM attachments", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize;

        drop(conn);

        // 2. Validate manifest and attachment inventory if manifest is present
        let manifest_path = staging_dir.join("manifest.json");
        let manifest_present = manifest_path.exists();

        if manifest_present {
            let manifest_bytes = fs::read(&manifest_path).map_err(BackupError::Io)?;
            let manifest: BackupManifest =
                serde_json::from_slice(&manifest_bytes).map_err(|e| {
                    BackupError::ManifestInvalid(format!(
                        "Failed to parse staged manifest.json: {e}"
                    ))
                })?;

            let (db_sha256, _) = compute_file_sha256(&staged_db_path)?;
            if db_sha256 != manifest.database.sha256 {
                return Err(BackupError::ChecksumMismatch {
                    entry: "database.sqlite".to_string(),
                    expected: manifest.database.sha256,
                    actual: db_sha256,
                });
            }

            for item in &manifest.attachments {
                let clean_rel = item.relative_path.trim().replace('\\', "/");
                let staged_rel = if clean_rel.starts_with("attachments/") {
                    clean_rel
                } else {
                    format!("attachments/{clean_rel}")
                };
                let att_path = staging_dir.join(&staged_rel);
                if !att_path.exists() {
                    return Err(BackupError::RequiredAttachmentMissing(
                        item.file_name.clone(),
                    ));
                }

                let (att_sha256, att_size) = compute_file_sha256(&att_path)?;
                if att_size != item.file_size || att_sha256 != item.sha256 {
                    return Err(BackupError::ChecksumMismatch {
                        entry: staged_rel,
                        expected: item.sha256.clone(),
                        actual: att_sha256,
                    });
                }
            }
        }

        Ok(StagedRestoreSummary {
            staging_id: staging_id.to_string(),
            staging_dir: staging_dir.to_string_lossy().to_string(),
            database_path: staged_db_path.to_string_lossy().to_string(),
            manifest_present,
            total_extracted_files,
            total_extracted_bytes,
            note_count,
            notebook_count,
            tag_count,
            trash_count,
            attachment_count,
            initial_schema_version,
            is_valid: true,
        })
    }

    /// Safely cleans up an isolated restore staging directory with strict boundary checks (Task 17).
    pub fn cleanup_staged_restore(
        staging_dir: &Path,
        paths: &StoragePaths,
    ) -> Result<bool, BackupError> {
        let dir_name = staging_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if !dir_name.starts_with(".restore_staging_") {
            return Err(BackupError::InvalidRequest(
                "Refusing to remove directory that is not a designated restore staging folder"
                    .to_string(),
            ));
        }

        if !staging_dir.exists() {
            return Ok(false);
        }

        if !staging_dir.is_dir() {
            return Err(BackupError::InvalidRequest(
                "Staging path is not a directory".to_string(),
            ));
        }

        if !staging_dir.starts_with(&paths.backups_dir) && !staging_dir.starts_with(&paths.root) {
            return Err(BackupError::InvalidRequest(
                "Staging directory path is outside application boundaries".to_string(),
            ));
        }

        if staging_dir == paths.root
            || staging_dir == paths.backups_dir
            || staging_dir == paths.attachments_dir
            || staging_dir == paths.checkpoints_dir()
            || staging_dir == paths.config_dir
        {
            return Err(BackupError::InvalidRequest(
                "Attempted deletion of primary application directory blocked".to_string(),
            ));
        }

        fs::remove_dir_all(staging_dir).map_err(BackupError::Io)?;
        Ok(true)
    }

    /// Removes any abandoned or orphaned restore staging directories found in the backups root (Task 17).
    pub fn cleanup_stale_staging_directories(paths: &StoragePaths) -> Result<usize, BackupError> {
        if !paths.backups_dir.exists() || !paths.backups_dir.is_dir() {
            return Ok(0);
        }

        let mut cleaned_count = 0;
        let entries = match fs::read_dir(&paths.backups_dir) {
            Ok(e) => e,
            Err(e) => return Err(BackupError::Io(e)),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if name.starts_with(".restore_staging_") && fs::remove_dir_all(&path).is_ok() {
                    cleaned_count += 1;
                }
            }
        }

        Ok(cleaned_count)
    }

    /// Applies schema migrations directly to the staged database before activation (Phase 11, Task 18).
    ///
    /// Safety Guarantees & Invariants:
    /// 1. Runs migrations strictly on the staged database, NEVER the active live database.
    /// 2. Older supported backups (v1, v2, v3) are upgraded transactionally to CURRENT_APP_SCHEMA_VERSION (v4).
    /// 3. Backups with newer unsupported schemas (v > 4) are rejected immediately.
    /// 4. If migration fails, staging data is cleaned up and current live data remains 100% untouched.
    /// 5. Validates schema integrity, foreign keys, and content retention post-migration.
    pub fn migrate_staged_database(
        staging_dir: &Path,
        paths: &StoragePaths,
    ) -> Result<StagedMigrationResult, BackupError> {
        if !staging_dir.exists() || !staging_dir.is_dir() {
            return Err(BackupError::RestoreFailed(format!(
                "Staging directory not found: '{}'",
                staging_dir.display()
            )));
        }

        let dir_name = staging_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if !dir_name.starts_with(".restore_staging_") {
            return Err(BackupError::InvalidRequest(
                "Target is not a valid restore staging folder".to_string(),
            ));
        }

        let staged_db_path = staging_dir.join("database.sqlite");
        if !staged_db_path.exists() {
            return Err(BackupError::RestoreFailed(
                "Staged database file 'database.sqlite' does not exist.".to_string(),
            ));
        }

        let live_db_str = paths.database_file.to_string_lossy().to_string();
        let staged_db_str = staged_db_path.to_string_lossy().to_string();
        if staged_db_str == live_db_str {
            return Err(BackupError::RestoreFailed(
                "Safety guard: attempted to run staging migration on live database file."
                    .to_string(),
            ));
        }

        let mut conn = Connection::open(&staged_db_path).map_err(BackupError::Database)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(BackupError::Database)?;

        let previous_schema_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if previous_schema_version > CURRENT_APP_SCHEMA_VERSION {
            drop(conn);
            let _ = fs::remove_dir_all(staging_dir);
            return Err(BackupError::IncompatibleSchema {
                backup_schema: previous_schema_version,
                current_schema: CURRENT_APP_SCHEMA_VERSION,
            });
        }

        if previous_schema_version < 1 {
            drop(conn);
            let _ = fs::remove_dir_all(staging_dir);
            return Err(BackupError::UnsupportedVersion {
                version: 0,
                max_supported: CURRENT_APP_SCHEMA_VERSION as u32,
            });
        }

        if previous_schema_version == CURRENT_APP_SCHEMA_VERSION {
            // Already at latest schema version
            let note_count: usize = conn
                .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 0", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap_or(0) as usize;
            let trash_count: usize = conn
                .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 1", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap_or(0) as usize;
            let notebook_count: usize = conn
                .query_row("SELECT COUNT(1) FROM notebooks", [], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as usize;
            let tag_count: usize = conn
                .query_row("SELECT COUNT(1) FROM tags", [], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as usize;
            let attachment_count: usize = conn
                .query_row("SELECT COUNT(1) FROM attachments", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap_or(0) as usize;

            let staging_id = dir_name.trim_start_matches(".restore_staging_").to_string();

            return Ok(StagedMigrationResult {
                staging_id,
                staging_dir: staging_dir.to_string_lossy().to_string(),
                previous_schema_version,
                migrated_schema_version: previous_schema_version,
                migrations_applied: 0,
                note_count,
                trash_count,
                notebook_count,
                tag_count,
                attachment_count,
                is_verified: true,
            });
        }

        // Apply migrations to staged database
        if let Err(e) = crate::storage::migrations::run_migrations(&mut conn) {
            drop(conn);
            let _ = fs::remove_dir_all(staging_dir);
            return Err(BackupError::RestoreFailed(format!(
                "Failed to run schema migrations on staged database from v{previous_schema_version} to v{CURRENT_APP_SCHEMA_VERSION}: {e}"
            )));
        }

        let migrated_schema_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |r| r.get(0),
            )
            .map_err(BackupError::Database)?;

        if migrated_schema_version != CURRENT_APP_SCHEMA_VERSION {
            drop(conn);
            let _ = fs::remove_dir_all(staging_dir);
            return Err(BackupError::PostRestoreValidationFailed(format!(
                "Schema migration completed but version is {migrated_schema_version}, expected {CURRENT_APP_SCHEMA_VERSION}"
            )));
        }

        // Verify post-migration integrity
        let integrity: String = conn
            .query_row("PRAGMA integrity_check;", [], |r| r.get(0))
            .map_err(BackupError::Database)?;
        if integrity != "ok" {
            drop(conn);
            let _ = fs::remove_dir_all(staging_dir);
            return Err(BackupError::PostRestoreValidationFailed(format!(
                "Migrated staged database failed integrity check: {integrity}"
            )));
        }

        {
            let mut fk_stmt = conn
                .prepare("PRAGMA foreign_key_check;")
                .map_err(BackupError::Database)?;
            let mut fk_rows = fk_stmt.query([]).map_err(BackupError::Database)?;
            if fk_rows.next().map_err(BackupError::Database)?.is_some() {
                drop(fk_rows);
                drop(fk_stmt);
                drop(conn);
                let _ = fs::remove_dir_all(staging_dir);
                return Err(BackupError::PostRestoreValidationFailed(
                    "Migrated staged database violates foreign key constraints".to_string(),
                ));
            }
        }

        let note_count: usize = conn
            .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 0", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize;
        let trash_count: usize = conn
            .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 1", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize;
        let notebook_count: usize = conn
            .query_row("SELECT COUNT(1) FROM notebooks", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize;
        let tag_count: usize = conn
            .query_row("SELECT COUNT(1) FROM tags", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize;
        let attachment_count: usize = conn
            .query_row("SELECT COUNT(1) FROM attachments", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize;

        let migrations_applied = (migrated_schema_version - previous_schema_version) as usize;
        let staging_id = dir_name.trim_start_matches(".restore_staging_").to_string();

        Ok(StagedMigrationResult {
            staging_id,
            staging_dir: staging_dir.to_string_lossy().to_string(),
            previous_schema_version,
            migrated_schema_version,
            migrations_applied,
            note_count,
            trash_count,
            notebook_count,
            tag_count,
            attachment_count,
            is_verified: true,
        })
    }

    /// Persists recovery marker payload tracking activation state across potential crashes (Task 19).
    pub fn write_activation_marker(
        marker: &RestoreActivationMarker,
        paths: &StoragePaths,
    ) -> Result<(), BackupError> {
        let marker_path = paths.restore_marker_file();
        let json_bytes = serde_json::to_vec_pretty(marker).map_err(|e| {
            BackupError::RestoreFailed(format!("Failed to serialize restore marker: {e}"))
        })?;
        fs::write(&marker_path, json_bytes).map_err(BackupError::Io)?;
        Ok(())
    }

    /// Reads and deserializes active recovery marker, if present on disk (Task 19).
    pub fn read_activation_marker(
        paths: &StoragePaths,
    ) -> Result<Option<RestoreActivationMarker>, BackupError> {
        let marker_path = paths.restore_marker_file();
        if !marker_path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&marker_path).map_err(BackupError::Io)?;
        let marker = serde_json::from_slice(&bytes).map_err(|e| {
            BackupError::RestoreFailed(format!("Failed to parse restore marker: {e}"))
        })?;
        Ok(Some(marker))
    }

    /// Deletes recovery marker once restore activation or rollback has safely completed (Task 19).
    pub fn remove_activation_marker(paths: &StoragePaths) -> Result<bool, BackupError> {
        let marker_path = paths.restore_marker_file();
        if marker_path.exists() {
            fs::remove_file(&marker_path).map_err(BackupError::Io)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Roll back to the pre-restore state or recovery checkpoint if activation or post-verification fails (Task 19).
    #[allow(clippy::too_many_arguments)] // Explicit paths document every rollback artifact.
    pub fn perform_rollback(
        conn: &mut Connection,
        paths: &StoragePaths,
        live_db_backup: &Path,
        live_wal_backup: &Path,
        live_shm_backup: &Path,
        live_att_backup: &Path,
        checkpoint_path: &str,
        marker: &mut RestoreActivationMarker,
    ) -> Result<(), BackupError> {
        marker.phase = RestoreActivationPhase::RollbackInProgress;
        let _ = Self::write_activation_marker(marker, paths);

        // 1. Ensure file connection is closed before swapping files back
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        *conn = Connection::open_in_memory().map_err(BackupError::Database)?;

        // 2. Remove any swapped database and attachment files
        if paths.database_file.exists() {
            let _ = fs::remove_file(&paths.database_file);
        }
        let wal_live = paths.root.join("database.sqlite-wal");
        if wal_live.exists() {
            let _ = fs::remove_file(wal_live);
        }
        let shm_live = paths.root.join("database.sqlite-shm");
        if shm_live.exists() {
            let _ = fs::remove_file(shm_live);
        }
        if paths.attachments_dir.exists() {
            let _ = fs::remove_dir_all(&paths.attachments_dir);
        }

        // 3. Restore from pre-restore backups if available
        if live_db_backup.exists() {
            fs::rename(live_db_backup, &paths.database_file).map_err(BackupError::Io)?;
            if live_wal_backup.exists() {
                let _ = fs::rename(live_wal_backup, paths.root.join("database.sqlite-wal"));
            }
            if live_shm_backup.exists() {
                let _ = fs::rename(live_shm_backup, paths.root.join("database.sqlite-shm"));
            }
            if live_att_backup.exists() {
                fs::rename(live_att_backup, &paths.attachments_dir).map_err(BackupError::Io)?;
            } else {
                let _ = fs::create_dir_all(&paths.attachments_dir);
            }
        } else {
            // Fallback: restore from recovery checkpoint archive
            let chk_path = Path::new(checkpoint_path);
            if chk_path.exists() {
                let staged = Self::stage_restore_archive(chk_path, paths)?;
                let staged_dir = PathBuf::from(&staged.staging_dir);
                let staged_db = staged_dir.join("database.sqlite");
                if staged_db.exists() {
                    fs::rename(&staged_db, &paths.database_file).map_err(BackupError::Io)?;
                }
                let staged_att = staged_dir.join("attachments");
                if staged_att.exists() {
                    fs::rename(&staged_att, &paths.attachments_dir).map_err(BackupError::Io)?;
                } else {
                    let _ = fs::create_dir_all(&paths.attachments_dir);
                }
                let _ = Self::cleanup_staged_restore(&staged_dir, paths);
            } else {
                return Err(BackupError::RollbackFailed(
                    "Neither pre-restore backup files nor recovery checkpoint archive could be found for rollback.".to_string(),
                ));
            }
        }

        // 4. Reopen connection to restored pre-restore database
        match init_connection(&paths.database_file) {
            Ok(new_conn) => *conn = new_conn,
            Err(e) => {
                return Err(BackupError::RollbackFailed(format!(
                    "Failed to reconnect to restored database during rollback: {e}"
                )));
            }
        }

        // 5. Clean up staging directory and marker
        marker.phase = RestoreActivationPhase::RolledBack;
        let _ = Self::write_activation_marker(marker, paths);
        let _ = Self::cleanup_staged_restore(&PathBuf::from(&marker.staging_dir), paths);
        let _ = Self::remove_activation_marker(paths);

        Ok(())
    }

    /// Verifies post-activation database integrity, schema version, tables, attachments,
    /// internal link structures, settings, and rebuilds search indexes (Phase 11, Tasks 19 & 20).
    pub fn verify_and_reinitialize_activated_state(
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<PostRestoreValidationReport, BackupError> {
        // 1. PRAGMA integrity_check
        let integrity: String = conn
            .query_row("PRAGMA integrity_check;", [], |r| r.get(0))
            .map_err(BackupError::Database)?;
        if integrity != "ok" {
            return Err(BackupError::PostRestoreValidationFailed(format!(
                "Restored database failed integrity check: {integrity}"
            )));
        }

        // 2. PRAGMA foreign_key_check
        {
            let mut fk_stmt = conn
                .prepare("PRAGMA foreign_key_check;")
                .map_err(BackupError::Database)?;
            let mut fk_rows = fk_stmt.query([]).map_err(BackupError::Database)?;
            if fk_rows.next().map_err(BackupError::Database)?.is_some() {
                return Err(BackupError::PostRestoreValidationFailed(
                    "Restored database violates foreign key constraints".to_string(),
                ));
            }
        }

        // 3. Required tables
        let required_tables = [
            "notes",
            "notebooks",
            "tags",
            "note_tags",
            "attachments",
            "settings",
            "_migrations",
        ];
        for table in required_tables {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .map_err(BackupError::Database)?;
            if count == 0 {
                return Err(BackupError::PostRestoreValidationFailed(format!(
                    "Restored database is missing required table: '{table}'"
                )));
            }
        }

        // 4. Schema version
        let schema_ver: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |r| r.get(0),
            )
            .map_err(BackupError::Database)?;
        if schema_ver != CURRENT_APP_SCHEMA_VERSION {
            return Err(BackupError::PostRestoreValidationFailed(format!(
                "Restored database schema version {schema_ver} does not match application version {CURRENT_APP_SCHEMA_VERSION}"
            )));
        }

        // 5. Query counts & verify records can be read
        let note_count: usize = conn
            .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 0", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize;
        let trash_count: usize = conn
            .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 1", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize;
        let notebook_count: usize = conn
            .query_row("SELECT COUNT(1) FROM notebooks", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize;
        let tag_count: usize = conn
            .query_row("SELECT COUNT(1) FROM tags", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize;
        let attachment_count: usize = conn
            .query_row("SELECT COUNT(1) FROM attachments", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize;

        // Verify notes and trash records can be read
        {
            let mut note_stmt = conn.prepare(
                "SELECT id, title, content, format, notebook_id, is_favorite, is_pinned, is_deleted FROM notes"
            ).map_err(BackupError::Database)?;
            let mut note_rows = note_stmt.query([]).map_err(BackupError::Database)?;
            while let Some(row) = note_rows.next().map_err(BackupError::Database)? {
                let _id: String = row.get(0).map_err(BackupError::Database)?;
            }
        }

        // 6. Ensure attachments directory exists and cross-check attachment files
        if !paths.attachments_dir.exists() {
            fs::create_dir_all(&paths.attachments_dir).map_err(BackupError::Io)?;
        }

        {
            let mut att_stmt = conn
                .prepare("SELECT note_id, file_name, relative_path FROM attachments")
                .map_err(BackupError::Database)?;
            let att_rows = att_stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })
                .map_err(BackupError::Database)?;
            for item in att_rows {
                let (note_id, file_name, relative_path) = item.map_err(BackupError::Database)?;
                let clean_rel = relative_path.trim().replace('\\', "/");
                let att_path = if clean_rel.starts_with("attachments/") {
                    paths.root.join(&clean_rel)
                } else if clean_rel.starts_with(&format!("{note_id}/"))
                    || paths.attachments_dir.join(&clean_rel).exists()
                {
                    paths.attachments_dir.join(&clean_rel)
                } else {
                    paths.attachments_dir.join(&note_id).join(&clean_rel)
                };
                if !att_path.exists() {
                    return Err(BackupError::PostRestoreValidationFailed(format!(
                        "Restored attachment file is missing on disk: '{file_name}' ('{relative_path}') for note '{note_id}'"
                    )));
                }
            }
        }

        // 7. Verify internal-link source content remains intact
        {
            let mut link_stmt = conn
                .prepare("SELECT id, content FROM notes WHERE content LIKE '%notepad://note/%'")
                .map_err(BackupError::Database)?;
            let mut link_rows = link_stmt.query([]).map_err(BackupError::Database)?;
            while let Some(row) = link_rows.next().map_err(BackupError::Database)? {
                let note_id: String = row.get(0).map_err(BackupError::Database)?;
                let content: Option<String> = row.get(1).map_err(BackupError::Database)?;
                if let Some(text) = content {
                    let parsed_links = LinkParser::parse_internal_links(&text);
                    for link in parsed_links {
                        if link.target_id.trim().is_empty() {
                            return Err(BackupError::PostRestoreValidationFailed(format!(
                                "Restored note '{note_id}' has malformed internal link target"
                            )));
                        }
                    }
                }
            }
        }

        // 8. Verify application settings table can be read
        {
            let mut settings_stmt = conn
                .prepare("SELECT key, value FROM settings")
                .map_err(BackupError::Database)?;
            let mut settings_rows = settings_stmt.query([]).map_err(BackupError::Database)?;
            while let Some(_row) = settings_rows.next().map_err(BackupError::Database)? {
                // Settings rows read cleanly
            }
        }

        // 9. Rebuild FTS5 search index and verify queryability
        SearchService::rebuild_index(conn).map_err(|e| {
            BackupError::PostRestoreValidationFailed(format!("Failed to rebuild search index: {e}"))
        })?;

        let is_encrypted =
            crate::storage::repositories::SearchRepository::is_encryption_enabled(conn);
        if !is_encrypted {
            let fts_check: Result<i64, _> =
                conn.query_row("SELECT COUNT(1) FROM notes_fts", [], |r| r.get(0));
            if let Err(e) = fts_check {
                return Err(BackupError::PostRestoreValidationFailed(format!(
                    "FTS5 full-text index query check failed after rebuild: {e}"
                )));
            }
        }

        Ok(PostRestoreValidationReport {
            is_valid: true,
            integrity_check_passed: true,
            foreign_key_check_passed: true,
            schema_version: schema_ver,
            note_count,
            notebook_count,
            tag_count,
            trash_count,
            attachment_count,
            attachments_verified_on_disk: true,
            fts_rebuilt_and_queryable: true,
            internal_links_verified: true,
            settings_verified: true,
        })
    }

    /// Safely replaces active application data with verified backup contents, automatically rolling back
    /// to the pre-restore state if activation or post-restore verification fails (Phase 11, Task 19).
    pub fn execute_restore<F>(
        conn: &mut Connection,
        paths: &StoragePaths,
        request: BackupRestoreRequest,
        mut progress_cb: Option<F>,
    ) -> Result<BackupRestoreResult, BackupError>
    where
        F: FnMut(BackupProgressEvent),
    {
        let op_id = request
            .operation_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        // Check if pre-emptive cancellation was already requested
        if Self::is_cancellation_requested(Some(&op_id)) {
            Self::clear_cancellation(Some(&op_id));
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage: BackupProgressStage::Cancelled,
                    message: "Restore operation was cancelled before execution started. Live data remains untouched.".to_string(),
                    current_item: None,
                    total_items: None,
                    percentage: None,
                    cancellable: false,
                    operation_id: Some(op_id.clone()),
                });
            }
            return Err(BackupError::Cancelled);
        }

        // Progress: Starting
        if let Some(ref mut cb) = progress_cb {
            cb(BackupProgressEvent {
                stage: BackupProgressStage::Starting,
                message: "Initiating controlled restore...".to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(5.0),
                cancellable: true,
                operation_id: Some(op_id.clone()),
            });
        }

        // 1. Validate inputs and candidate path
        let candidate_path = Path::new(request.file_path.trim());
        if !candidate_path.exists() || !candidate_path.is_file() {
            return Err(BackupError::DestinationUnavailable(format!(
                "Candidate backup archive '{}' not found or is not a file.",
                candidate_path.display()
            )));
        }

        let live_db_str = paths.database_file.to_string_lossy().to_string();
        let candidate_str = candidate_path.to_string_lossy().to_string();
        if candidate_str == live_db_str || candidate_str.starts_with(&format!("{live_db_str}-")) {
            return Err(BackupError::DestinationUnavailable(
                "Cannot restore directly targeting active live database or journals.".to_string(),
            ));
        }

        // Acquire exclusive restore lock
        let mut acquired = None;
        for _ in 0..100 {
            if let Ok(guard) = BackupSchedulerService::try_acquire_restore_lock() {
                acquired = Some(guard);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let _restore_lock_guard = match acquired {
            Some(g) => g,
            None => {
                return Err(BackupError::ConcurrentOperation(
                    "Another backup or restore operation is currently in progress.".to_string(),
                ));
            }
        };

        // Recover previous interrupted restore if present
        let _ = Self::recover_if_interrupted(paths);

        // Progress: ValidatingBackup
        if let Some(ref mut cb) = progress_cb {
            cb(BackupProgressEvent {
                stage: BackupProgressStage::ValidatingBackup,
                message: "Validating backup archive structure and integrity...".to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(10.0),
                cancellable: true,
                operation_id: Some(op_id.clone()),
            });
        }

        // Cancellation checkpoint 1: before validation
        if Self::is_cancellation_requested(Some(&op_id)) {
            Self::clear_cancellation(Some(&op_id));
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage: BackupProgressStage::Cancelled,
                    message: "Restore operation was cancelled before validation. Live data remains untouched.".to_string(),
                    current_item: None,
                    total_items: None,
                    percentage: None,
                    cancellable: false,
                    operation_id: Some(op_id.clone()),
                });
            }
            return Err(BackupError::Cancelled);
        }

        // Validate candidate archive
        let val_res = BackupValidationService::validate_backup(candidate_path, paths)?;
        if !val_res.is_valid {
            let issues_summary = val_res
                .issues
                .iter()
                .map(|i| i.message.clone())
                .collect::<Vec<_>>()
                .join("; ");
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage: BackupProgressStage::FailedWithRecoveryAvailable,
                    message: format!("Backup validation failed: {issues_summary}. Live data remains active and untouched."),
                    current_item: None,
                    total_items: None,
                    percentage: None,
                    cancellable: false,
                    operation_id: Some(op_id.clone()),
                });
            }
            return Err(BackupError::ValidationFailed(format!(
                "Candidate backup archive failed validation: {issues_summary}"
            )));
        }
        if val_res.compatibility == BackupCompatibilityStatus::IncompatibleVersion
            || val_res.compatibility == BackupCompatibilityStatus::Corrupted
        {
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage: BackupProgressStage::FailedWithRecoveryAvailable,
                    message: format!("Candidate backup archive is incompatible or corrupted: {:?}. Live data remains active and untouched.", val_res.compatibility),
                    current_item: None,
                    total_items: None,
                    percentage: None,
                    cancellable: false,
                    operation_id: Some(op_id.clone()),
                });
            }
            return Err(BackupError::ValidationFailed(format!(
                "Candidate backup archive is incompatible or corrupted: {:?}",
                val_res.compatibility
            )));
        }

        // 2. Extract candidate archive to isolated staging (Task 17)
        if let Some(ref mut cb) = progress_cb {
            cb(BackupProgressEvent {
                stage: BackupProgressStage::PreparingStagingData,
                message: "Extracting backup to isolated staging directory...".to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(20.0),
                cancellable: true,
                operation_id: Some(op_id.clone()),
            });
            cb(BackupProgressEvent {
                stage: BackupProgressStage::ExtractingStaging,
                message: "Extracting backup to isolated staging directory...".to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(20.0),
                cancellable: true,
                operation_id: Some(op_id.clone()),
            });
        }
        let staged_summary = Self::stage_restore_archive(candidate_path, paths)?;
        let staging_dir = PathBuf::from(&staged_summary.staging_dir);

        // Cancellation checkpoint 2: after staging extraction
        if Self::is_cancellation_requested(Some(&op_id)) {
            let _ = Self::cleanup_staged_restore(&staging_dir, paths);
            Self::clear_cancellation(Some(&op_id));
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage: BackupProgressStage::Cancelled,
                    message: "Restore operation was cancelled during staging. Staging files purged; live data remains active and untouched.".to_string(),
                    current_item: None,
                    total_items: None,
                    percentage: None,
                    cancellable: false,
                    operation_id: Some(op_id.clone()),
                });
            }
            return Err(BackupError::Cancelled);
        }

        // 3. Staged database schema compatibility & migrations (Task 18)
        if let Some(ref mut cb) = progress_cb {
            cb(BackupProgressEvent {
                stage: BackupProgressStage::CheckingDatabaseIntegrity,
                message: "Checking database integrity and migrating schema if required..."
                    .to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(40.0),
                cancellable: true,
                operation_id: Some(op_id.clone()),
            });
            cb(BackupProgressEvent {
                stage: BackupProgressStage::ValidatingStagedData,
                message: "Checking schema compatibility and migrating staged database..."
                    .to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(40.0),
                cancellable: true,
                operation_id: Some(op_id.clone()),
            });
        }
        if let Err(e) = Self::migrate_staged_database(&staging_dir, paths) {
            let _ = Self::cleanup_staged_restore(&staging_dir, paths);
            Self::clear_cancellation(Some(&op_id));
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage: BackupProgressStage::FailedWithRecoveryAvailable,
                    message: format!("Staged database migration failed: {e}. Live data remains active and untouched."),
                    current_item: None,
                    total_items: None,
                    percentage: None,
                    cancellable: false,
                    operation_id: Some(op_id.clone()),
                });
            }
            return Err(e);
        }

        // Cancellation checkpoint 3: after database migration
        if Self::is_cancellation_requested(Some(&op_id)) {
            let _ = Self::cleanup_staged_restore(&staging_dir, paths);
            Self::clear_cancellation(Some(&op_id));
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage: BackupProgressStage::Cancelled,
                    message: "Restore operation was cancelled after database verification. Staging files purged; live data remains active and untouched.".to_string(),
                    current_item: None,
                    total_items: None,
                    percentage: None,
                    cancellable: false,
                    operation_id: Some(op_id.clone()),
                });
            }
            return Err(BackupError::Cancelled);
        }

        // 4. Checking attachments
        if let Some(ref mut cb) = progress_cb {
            cb(BackupProgressEvent {
                stage: BackupProgressStage::CheckingAttachments,
                message: "Verifying staged attachments integrity...".to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(50.0),
                cancellable: true,
                operation_id: Some(op_id.clone()),
            });
        }
        let staged_attachments_dir = staging_dir.join("attachments");
        if staged_attachments_dir.exists() && !staged_attachments_dir.is_dir() {
            let _ = Self::cleanup_staged_restore(&staging_dir, paths);
            Self::clear_cancellation(Some(&op_id));
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage: BackupProgressStage::FailedWithRecoveryAvailable,
                    message: "Staged attachments entry is invalid. Live data remains active and untouched.".to_string(),
                    current_item: None,
                    total_items: None,
                    percentage: None,
                    cancellable: false,
                    operation_id: Some(op_id.clone()),
                });
            }
            return Err(BackupError::ValidationFailed(
                "Staged attachments entry is not a directory".to_string(),
            ));
        }

        // Cancellation checkpoint 4: after attachments check
        if Self::is_cancellation_requested(Some(&op_id)) {
            let _ = Self::cleanup_staged_restore(&staging_dir, paths);
            Self::clear_cancellation(Some(&op_id));
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage: BackupProgressStage::Cancelled,
                    message: "Restore operation was cancelled during attachment checking. Staging files purged; live data remains active and untouched.".to_string(),
                    current_item: None,
                    total_items: None,
                    percentage: None,
                    cancellable: false,
                    operation_id: Some(op_id.clone()),
                });
            }
            return Err(BackupError::Cancelled);
        }

        // 5. Create pre-restore recovery checkpoint of current working data (Task 16)
        if let Some(ref mut cb) = progress_cb {
            cb(BackupProgressEvent {
                stage: BackupProgressStage::CreatingCheckpoint,
                message: "Creating pre-restore recovery checkpoint...".to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(60.0),
                cancellable: true,
                operation_id: Some(op_id.clone()),
            });
        }
        let checkpoint = match Self::create_recovery_checkpoint_internal(conn, paths) {
            Ok(chk) => chk,
            Err(e) => {
                let _ = Self::cleanup_staged_restore(&staging_dir, paths);
                Self::clear_cancellation(Some(&op_id));
                if let Some(ref mut cb) = progress_cb {
                    cb(BackupProgressEvent {
                        stage: BackupProgressStage::FailedWithRecoveryAvailable,
                        message: format!("Failed to create pre-restore recovery checkpoint: {e}. Live data remains active and untouched."),
                        current_item: None,
                        total_items: None,
                        percentage: None,
                        cancellable: false,
                        operation_id: Some(op_id.clone()),
                    });
                }
                return Err(BackupError::RestoreCheckpointFailed(format!(
                    "Failed to create pre-restore recovery checkpoint: {e}"
                )));
            }
        };
        let checkpoint_path = checkpoint.file_path.clone();

        // Cancellation checkpoint 5: AFTER checkpoint creation, strictly BEFORE activation begins
        // Checkpoint is preserved, staging is cleaned up, live data is untouched!
        if Self::is_cancellation_requested(Some(&op_id)) {
            let _ = Self::cleanup_staged_restore(&staging_dir, paths);
            Self::clear_cancellation(Some(&op_id));
            if let Some(ref mut cb) = progress_cb {
                cb(BackupProgressEvent {
                    stage: BackupProgressStage::Cancelled,
                    message: "Restore operation was cancelled before activation. Live data remains untouched. Pre-restore checkpoint was safely preserved.".to_string(),
                    current_item: None,
                    total_items: None,
                    percentage: None,
                    cancellable: false,
                    operation_id: Some(op_id.clone()),
                });
            }
            return Err(BackupError::Cancelled);
        }

        // 6. Point of No Casual Cancellation: Prepare activation marker and swap paths
        if let Some(ref mut cb) = progress_cb {
            cb(BackupProgressEvent {
                stage: BackupProgressStage::PreparingActivation,
                message: "Preparing activation (cancellation locked to protect data integrity)..."
                    .to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(70.0),
                cancellable: false,
                operation_id: Some(op_id.clone()),
            });
        }

        let live_db_backup = paths.root.join(format!(".db_pre_restore_{op_id}.sqlite"));
        let live_wal_backup = paths
            .root
            .join(format!(".db_pre_restore_{op_id}.sqlite-wal"));
        let live_shm_backup = paths
            .root
            .join(format!(".db_pre_restore_{op_id}.sqlite-shm"));
        let live_att_backup = paths.root.join(format!(".att_pre_restore_{op_id}"));

        let mut marker = RestoreActivationMarker {
            operation_id: op_id.clone(),
            started_at: chrono::Utc::now().to_rfc3339(),
            candidate_archive_path: candidate_path.to_string_lossy().to_string(),
            staging_dir: staging_dir.to_string_lossy().to_string(),
            checkpoint_path: checkpoint_path.clone(),
            live_db_backup_path: live_db_backup.to_string_lossy().to_string(),
            live_attachments_backup_path: live_att_backup.to_string_lossy().to_string(),
            phase: RestoreActivationPhase::CheckpointCreated,
        };
        Self::write_activation_marker(&marker, paths)?;

        // Flush SQLite WAL and release file locks
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        *conn = Connection::open_in_memory().map_err(BackupError::Database)?;

        if let Some(ref mut cb) = progress_cb {
            cb(BackupProgressEvent {
                stage: BackupProgressStage::ActivatingRestoredData,
                message: "Activating restored database and attachments...".to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(75.0),
                cancellable: false,
                operation_id: Some(op_id.clone()),
            });
        }

        // 7. File Swap with guarded error handling
        let swap_err = (|| -> Result<(), BackupError> {
            if paths.database_file.exists() {
                fs::rename(&paths.database_file, &live_db_backup).map_err(BackupError::Io)?;
            }
            let live_wal = paths.root.join("database.sqlite-wal");
            if live_wal.exists() {
                let _ = fs::rename(live_wal, &live_wal_backup);
            }
            let live_shm = paths.root.join("database.sqlite-shm");
            if live_shm.exists() {
                let _ = fs::rename(live_shm, &live_shm_backup);
            }

            if paths.attachments_dir.exists() {
                fs::rename(&paths.attachments_dir, &live_att_backup).map_err(BackupError::Io)?;
            }

            let staged_db = staging_dir.join("database.sqlite");
            if !staged_db.exists() {
                return Err(BackupError::RestoreFailed(
                    "Staged database file missing before swap".to_string(),
                ));
            }
            fs::rename(&staged_db, &paths.database_file).map_err(BackupError::Io)?;

            let staged_att = staging_dir.join("attachments");
            if staged_att.exists() {
                fs::rename(&staged_att, &paths.attachments_dir).map_err(BackupError::Io)?;
            } else {
                fs::create_dir_all(&paths.attachments_dir).map_err(BackupError::Io)?;
            }

            Ok(())
        })();

        if let Err(e) = swap_err {
            let rollback_res = Self::perform_rollback(
                conn,
                paths,
                &live_db_backup,
                &live_wal_backup,
                &live_shm_backup,
                &live_att_backup,
                &checkpoint_path,
                &mut marker,
            );
            Self::clear_cancellation(Some(&op_id));
            if rollback_res.is_ok() {
                if let Some(ref mut cb) = progress_cb {
                    cb(BackupProgressEvent {
                        stage: BackupProgressStage::FailedWithRecoveryAvailable,
                        message: format!("Restore activation swap failed: {e}. Active state was cleanly rolled back to previous data; pre-restore checkpoint is preserved."),
                        current_item: None,
                        total_items: None,
                        percentage: None,
                        cancellable: false,
                        operation_id: Some(op_id.clone()),
                    });
                }
                return Err(BackupError::RestoreFailed(format!(
                    "Swap failed: {e}. Active state was cleanly rolled back."
                )));
            } else {
                if let Some(ref mut cb) = progress_cb {
                    cb(BackupProgressEvent {
                        stage: BackupProgressStage::FailedAndRecoveryRequired,
                        message: format!("Restore activation swap failed: {e}, and automatic rollback encountered an issue. Manual recovery from checkpoint '{checkpoint_path}' is required."),
                        current_item: None,
                        total_items: None,
                        percentage: None,
                        cancellable: false,
                        operation_id: Some(op_id.clone()),
                    });
                }
                return Err(BackupError::RollbackFailed(format!(
                    "Swap failed: {e}. Rollback also failed."
                )));
            }
        }

        marker.phase = RestoreActivationPhase::FilesSwapped;
        let _ = Self::write_activation_marker(&marker, paths);

        // 8. Reopen database connection
        match init_connection(&paths.database_file) {
            Ok(new_conn) => *conn = new_conn,
            Err(e) => {
                let rollback_res = Self::perform_rollback(
                    conn,
                    paths,
                    &live_db_backup,
                    &live_wal_backup,
                    &live_shm_backup,
                    &live_att_backup,
                    &checkpoint_path,
                    &mut marker,
                );
                Self::clear_cancellation(Some(&op_id));
                if rollback_res.is_ok() {
                    if let Some(ref mut cb) = progress_cb {
                        cb(BackupProgressEvent {
                            stage: BackupProgressStage::FailedWithRecoveryAvailable,
                            message: format!("Failed to reopen restored database: {e}. Active state was cleanly rolled back to previous data; pre-restore checkpoint is preserved."),
                            current_item: None,
                            total_items: None,
                            percentage: None,
                            cancellable: false,
                            operation_id: Some(op_id.clone()),
                        });
                    }
                    return Err(BackupError::PostRestoreValidationFailed(format!(
                        "Failed to reopen restored database: {e}. Active state was cleanly rolled back."
                    )));
                } else {
                    if let Some(ref mut cb) = progress_cb {
                        cb(BackupProgressEvent {
                            stage: BackupProgressStage::FailedAndRecoveryRequired,
                            message: format!("Failed to reopen restored database: {e}, and automatic rollback encountered an issue. Manual recovery from checkpoint '{checkpoint_path}' is required."),
                            current_item: None,
                            total_items: None,
                            percentage: None,
                            cancellable: false,
                            operation_id: Some(op_id.clone()),
                        });
                    }
                    return Err(BackupError::RollbackFailed(format!(
                        "Failed to reopen restored database: {e}. Rollback also failed."
                    )));
                }
            }
        }

        // 9. Post-activation verification & index rebuilding
        marker.phase = RestoreActivationPhase::PostVerificationStarted;
        let _ = Self::write_activation_marker(&marker, paths);

        if let Some(ref mut cb) = progress_cb {
            cb(BackupProgressEvent {
                stage: BackupProgressStage::VerifyingRestoredState,
                message: "Verifying restored state and reinitializing search indexes..."
                    .to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(85.0),
                cancellable: false,
                operation_id: Some(op_id.clone()),
            });
            cb(BackupProgressEvent {
                stage: BackupProgressStage::RebuildingSearchIndex,
                message: "Verifying restored data and rebuilding search indexes...".to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(85.0),
                cancellable: false,
                operation_id: Some(op_id.clone()),
            });
        }

        let val_report = match Self::verify_and_reinitialize_activated_state(conn, paths) {
            Ok(report) => report,
            Err(e) => {
                let rollback_res = Self::perform_rollback(
                    conn,
                    paths,
                    &live_db_backup,
                    &live_wal_backup,
                    &live_shm_backup,
                    &live_att_backup,
                    &checkpoint_path,
                    &mut marker,
                );
                Self::clear_cancellation(Some(&op_id));
                if rollback_res.is_ok() {
                    if let Some(ref mut cb) = progress_cb {
                        cb(BackupProgressEvent {
                            stage: BackupProgressStage::FailedWithRecoveryAvailable,
                            message: format!("Post-restore verification failed: {e}. Active state was cleanly rolled back to previous data; pre-restore checkpoint is preserved."),
                            current_item: None,
                            total_items: None,
                            percentage: None,
                            cancellable: false,
                            operation_id: Some(op_id.clone()),
                        });
                    }
                    return Err(BackupError::PostRestoreValidationFailed(format!(
                        "Post-restore verification failed: {e}. Active state was cleanly rolled back."
                    )));
                } else {
                    if let Some(ref mut cb) = progress_cb {
                        cb(BackupProgressEvent {
                            stage: BackupProgressStage::FailedAndRecoveryRequired,
                            message: format!("Post-restore verification failed: {e}, and automatic rollback encountered an issue. Manual recovery from checkpoint '{checkpoint_path}' is required."),
                            current_item: None,
                            total_items: None,
                            percentage: None,
                            cancellable: false,
                            operation_id: Some(op_id.clone()),
                        });
                    }
                    return Err(BackupError::RollbackFailed(format!(
                        "Post-restore verification failed: {e}. Rollback also failed."
                    )));
                }
            }
        };

        // 10. Commit success!
        if live_db_backup.exists() {
            let _ = fs::remove_file(&live_db_backup);
        }
        if live_wal_backup.exists() {
            let _ = fs::remove_file(&live_wal_backup);
        }
        if live_shm_backup.exists() {
            let _ = fs::remove_file(&live_shm_backup);
        }
        if live_att_backup.exists() {
            let _ = fs::remove_dir_all(&live_att_backup);
        }

        let _ = Self::cleanup_staged_restore(&staging_dir, paths);
        marker.phase = RestoreActivationPhase::Completed;
        let _ = Self::remove_activation_marker(paths);
        Self::clear_cancellation(Some(&op_id));

        if let Some(ref mut cb) = progress_cb {
            cb(BackupProgressEvent {
                stage: BackupProgressStage::Completed,
                message: "Restore completed and verified successfully.".to_string(),
                current_item: None,
                total_items: None,
                percentage: Some(100.0),
                cancellable: false,
                operation_id: Some(op_id.clone()),
            });
        }

        Ok(BackupRestoreResult {
            success: true,
            restored_at: chrono::Utc::now().to_rfc3339(),
            note_count: val_report.note_count,
            notebook_count: val_report.notebook_count,
            tag_count: val_report.tag_count,
            trash_count: val_report.trash_count,
            attachment_count: val_report.attachment_count,
            checkpoint_path,
            warning_message: None,
            validation_report: Some(val_report),
        })
    }

    /// Checks for and resolves an interrupted restore operation after an unexpected application crash (Phase 11, Task 19).
    pub fn recover_if_interrupted(paths: &StoragePaths) -> Result<Option<String>, BackupError> {
        let marker_path = paths.restore_marker_file();
        if !marker_path.exists() {
            return Ok(None);
        }

        let marker_bytes = fs::read(&marker_path).map_err(BackupError::Io)?;
        let marker: RestoreActivationMarker = match serde_json::from_slice(&marker_bytes) {
            Ok(m) => m,
            Err(_) => {
                let _ = fs::remove_file(&marker_path);
                return Ok(None);
            }
        };

        match marker.phase {
            RestoreActivationPhase::Completed | RestoreActivationPhase::RolledBack => {
                let _ = fs::remove_file(&marker_path);
                let _ = Self::cleanup_staged_restore(&PathBuf::from(&marker.staging_dir), paths);
                Ok(None)
            }
            RestoreActivationPhase::Initiated | RestoreActivationPhase::CheckpointCreated => {
                let _ = Self::cleanup_staged_restore(&PathBuf::from(&marker.staging_dir), paths);
                let _ = fs::remove_file(&marker_path);
                Ok(Some(
                    "Cleaned up interrupted restore initiated prior to file modification."
                        .to_string(),
                ))
            }
            RestoreActivationPhase::FilesSwapped
            | RestoreActivationPhase::PostVerificationStarted
            | RestoreActivationPhase::RollbackInProgress => {
                let live_db_backup = PathBuf::from(&marker.live_db_backup_path);
                let live_att_backup = PathBuf::from(&marker.live_attachments_backup_path);

                if live_db_backup.exists() {
                    if paths.database_file.exists() {
                        let _ = fs::remove_file(&paths.database_file);
                    }
                    let wal = paths.root.join("database.sqlite-wal");
                    if wal.exists() {
                        let _ = fs::remove_file(wal);
                    }
                    let shm = paths.root.join("database.sqlite-shm");
                    if shm.exists() {
                        let _ = fs::remove_file(shm);
                    }
                    if paths.attachments_dir.exists() {
                        let _ = fs::remove_dir_all(&paths.attachments_dir);
                    }

                    fs::rename(&live_db_backup, &paths.database_file).map_err(BackupError::Io)?;
                    let wal_backup = PathBuf::from(format!("{}-wal", marker.live_db_backup_path));
                    if wal_backup.exists() {
                        let _ = fs::rename(wal_backup, paths.root.join("database.sqlite-wal"));
                    }
                    let shm_backup = PathBuf::from(format!("{}-shm", marker.live_db_backup_path));
                    if shm_backup.exists() {
                        let _ = fs::rename(shm_backup, paths.root.join("database.sqlite-shm"));
                    }
                    if live_att_backup.exists() {
                        let _ = fs::rename(&live_att_backup, &paths.attachments_dir);
                    } else {
                        let _ = fs::create_dir_all(&paths.attachments_dir);
                    }

                    let _ =
                        Self::cleanup_staged_restore(&PathBuf::from(&marker.staging_dir), paths);
                    let _ = fs::remove_file(&marker_path);
                    Ok(Some("Recovered application state from pre-restore backup files after interrupted restore.".to_string()))
                } else {
                    let chk_path = PathBuf::from(&marker.checkpoint_path);
                    if chk_path.exists() {
                        let staged = Self::stage_restore_archive(&chk_path, paths)?;
                        let staged_dir = PathBuf::from(&staged.staging_dir);
                        if paths.database_file.exists() {
                            let _ = fs::remove_file(&paths.database_file);
                        }
                        if paths.attachments_dir.exists() {
                            let _ = fs::remove_dir_all(&paths.attachments_dir);
                        }
                        let staged_db = staged_dir.join("database.sqlite");
                        if staged_db.exists() {
                            fs::rename(&staged_db, &paths.database_file)
                                .map_err(BackupError::Io)?;
                        }
                        let staged_att = staged_dir.join("attachments");
                        if staged_att.exists() {
                            fs::rename(&staged_att, &paths.attachments_dir)
                                .map_err(BackupError::Io)?;
                        } else {
                            let _ = fs::create_dir_all(&paths.attachments_dir);
                        }
                        let _ = Self::cleanup_staged_restore(&staged_dir, paths);
                        let _ = Self::cleanup_staged_restore(
                            &PathBuf::from(&marker.staging_dir),
                            paths,
                        );
                        let _ = Self::remove_activation_marker(paths);
                        Ok(Some("Recovered application state from recovery checkpoint archive after interrupted restore.".to_string()))
                    } else {
                        let _ = fs::remove_file(&marker_path);
                        Err(BackupError::RollbackFailed(
                            "Crash recovery failed: Neither pre-restore backup files nor recovery checkpoint archive could be found.".to_string(),
                        ))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::{BackupCreateRequest, CreateNoteDto, CreateNotebookDto};
    use crate::storage::repositories::notebooks::NotebookRepository;
    use crate::storage::repositories::notes::NoteRepository;
    use crate::storage::repositories::tags::TagRepository;
    use crate::storage::services::backup::BackupService;
    use uuid::Uuid;

    #[test]
    fn test_task_15_preview_restore_valid_archive() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_prev_valid_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        // Populate notes
        let _n1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Restore Preview Note 1".to_string()),
                content: Some("First content".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        let _n2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Restore Preview Note 2".to_string()),
                content: Some("Second content".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(true),
                is_pinned: Some(false),
            },
        )
        .unwrap();

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
        )
        .unwrap();

        let archive_path = Path::new(&backup_res.file_path);

        // Preview restore
        let preview = RestoreService::preview_restore(archive_path, &paths).unwrap();

        assert!(preview.can_restore, "Valid backup must be restorable");
        assert_eq!(preview.note_count, 2);
        assert_eq!(preview.compatibility, BackupCompatibilityStatus::Compatible);
        assert_eq!(preview.file_name, backup_res.file_name);
        assert!(preview.file_size > 0);
        assert!(
            preview.warnings.is_empty(),
            "Healthy backup should have no warnings"
        );

        // Invariant: live database remains unchanged
        let note_count_live: i64 = conn
            .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(note_count_live, 2);
    }

    #[test]
    fn test_task_15_preview_restore_corrupted_archive_blocks_restore() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_prev_corrupt_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let corrupt_path = temp_dir.join("corrupt_preview.pnbackup");
        fs::write(&corrupt_path, b"Not valid zip archive").unwrap();

        let preview = RestoreService::preview_restore(&corrupt_path, &paths).unwrap();

        assert!(
            !preview.can_restore,
            "Corrupt backup must NOT be restorable"
        );
        assert_eq!(preview.compatibility, BackupCompatibilityStatus::Corrupted);
        assert!(
            !preview.warnings.is_empty(),
            "Corrupt backup must list errors in warnings"
        );
    }

    #[test]
    fn test_task_15_preview_restore_rejects_active_database() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_prev_activedb_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        let err = RestoreService::preview_restore(&paths.database_file, &paths);
        assert!(err.is_err(), "Must reject live database file");
    }

    #[test]
    fn test_task_16_create_recovery_checkpoint_success() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_chk_valid_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Populate database with notes
        let _n1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Checkpoint Note 1".to_string()),
                content: Some("First note content before restore".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        let _n2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Checkpoint Note 2".to_string()),
                content: Some("Second note content before restore".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(true),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        // 2. Create pre-restore recovery checkpoint
        let checkpoint = RestoreService::create_recovery_checkpoint(&conn, &paths).unwrap();

        assert!(checkpoint.is_verified, "Checkpoint must be verified");
        assert_eq!(checkpoint.note_count, 2);
        assert_eq!(checkpoint.schema_version, CURRENT_APP_SCHEMA_VERSION);
        assert!(checkpoint.file_size > 0);
        assert!(
            Path::new(&checkpoint.file_path).exists(),
            "Checkpoint file must exist on disk"
        );
        assert!(
            checkpoint.file_path.contains("checkpoints"),
            "Must be stored in checkpoints directory"
        );

        // 3. Independently validate the checkpoint
        let val =
            RestoreService::validate_recovery_checkpoint(Path::new(&checkpoint.file_path), &paths)
                .unwrap();
        assert!(val.is_valid, "Checkpoint must pass independent validation");
        assert_eq!(val.compatibility, BackupCompatibilityStatus::Compatible);

        // 4. Invariant: Live database was not modified or corrupted
        let live_notes: i64 = conn
            .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            live_notes, 2,
            "Live database must remain completely unchanged"
        );

        // 5. Check listing and latest
        let checkpoints_list = RestoreService::list_recovery_checkpoints(&paths).unwrap();
        assert_eq!(checkpoints_list.len(), 1);
        assert_eq!(checkpoints_list[0].checkpoint_id, checkpoint.checkpoint_id);

        let latest = RestoreService::get_latest_recovery_checkpoint(&paths).unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().file_path, checkpoint.file_path);
    }

    #[test]
    fn test_task_16_checkpoint_collision_and_immutability() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_chk_coll_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        let _n1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note".to_string()),
                content: Some("Body".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        // Create first checkpoint
        let chk1 = RestoreService::create_recovery_checkpoint(&conn, &paths).unwrap();

        // Create a file with same name to force collision resolution
        let chk2 = RestoreService::create_recovery_checkpoint(&conn, &paths).unwrap();

        assert!(
            Path::new(&chk1.file_path).exists(),
            "First checkpoint must still exist"
        );
        assert!(
            Path::new(&chk2.file_path).exists(),
            "Second checkpoint must exist"
        );

        let list = RestoreService::list_recovery_checkpoints(&paths).unwrap();
        assert_eq!(
            list.len(),
            2,
            "Both checkpoints must exist without overwriting"
        );
    }

    #[test]
    fn test_task_16_checkpoint_retention_pruning() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_chk_prune_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let checkpoints_dir = paths.checkpoints_dir();

        // Create dummy checkpoint files
        for i in 1..=8 {
            let filename = format!("personal-notepad-checkpoint-20261013-10000{}.pnbackup", i);
            let file_path = checkpoints_dir.join(&filename);
            fs::write(&file_path, b"test dummy data").unwrap();
        }

        let deleted = RestoreService::prune_excess_checkpoints(&checkpoints_dir, 5).unwrap();
        assert_eq!(
            deleted.len(),
            3,
            "Must prune 3 oldest checkpoints to retain 5"
        );

        let remaining = fs::read_dir(&checkpoints_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_file())
            .count();
        assert_eq!(remaining, 5, "Exactly 5 checkpoints must remain");
    }

    #[test]
    fn test_task_16_create_recovery_checkpoint_aborts_on_failure_leaves_live_db_untouched() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_chk_fail_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        let _n1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Valuable Live Note".to_string()),
                content: Some("Must be preserved at all costs".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(true),
                is_pinned: Some(true),
            },
        )
        .unwrap();

        // Remove checkpoints_dir and replace with a regular file to cause checkpoint creation to fail safely
        let _ = fs::remove_dir_all(paths.checkpoints_dir());
        fs::write(
            paths.checkpoints_dir(),
            b"blocking file preventing directory creation",
        )
        .unwrap();

        let res = RestoreService::create_recovery_checkpoint(&conn, &paths);
        assert!(res.is_err(), "Must fail when checkpoint creation fails");

        // Live data must remain 100% intact
        let live_notes: i64 = conn
            .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(live_notes, 1, "Live database notes must remain unchanged");
    }

    #[test]
    fn test_task_17_sanitize_archive_entry_path_rules() {
        // Valid paths
        assert_eq!(
            RestoreService::sanitize_archive_entry_path("database.sqlite").unwrap(),
            PathBuf::from("database.sqlite")
        );
        assert_eq!(
            RestoreService::sanitize_archive_entry_path("attachments/note-1/image.png").unwrap(),
            PathBuf::from("attachments/note-1/image.png")
        );
        assert_eq!(
            RestoreService::sanitize_archive_entry_path("manifest.json").unwrap(),
            PathBuf::from("manifest.json")
        );

        // Path traversal rejected
        assert!(RestoreService::sanitize_archive_entry_path("../evil.txt").is_err());
        assert!(RestoreService::sanitize_archive_entry_path("dir/../../evil.txt").is_err());
        assert!(RestoreService::sanitize_archive_entry_path("..\\evil.txt").is_err());

        // Absolute paths rejected
        assert!(RestoreService::sanitize_archive_entry_path("/etc/passwd").is_err());
        assert!(RestoreService::sanitize_archive_entry_path("\\Windows\\System32").is_err());
        assert!(RestoreService::sanitize_archive_entry_path("C:\\autoexec.bat").is_err());

        // Null bytes or blank names rejected
        assert!(RestoreService::sanitize_archive_entry_path("").is_err());
        assert!(RestoreService::sanitize_archive_entry_path("   ").is_err());
        assert!(RestoreService::sanitize_archive_entry_path("foo\0bar.txt").is_err());
    }

    #[test]
    fn test_task_17_stage_restore_valid_archive_and_cleanup() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_stg_val_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        let _n1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Staged Restore Note 1".to_string()),
                content: Some("First staged content".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(true),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        let backup_res = BackupService::create_backup(
            &conn,
            &paths,
            BackupCreateRequest {
                destination_path: None,
                backup_type: Some(BackupType::Manual),
                allow_overwrite: Some(false),
            },
            None::<fn(_)>,
        )
        .unwrap();

        let archive_path = Path::new(&backup_res.file_path);

        // 1. Stage the backup archive
        let staged = RestoreService::stage_restore_archive(archive_path, &paths).unwrap();
        assert!(staged.is_valid);
        assert_eq!(staged.note_count, 1);
        assert!(staged.manifest_present);
        assert!(staged.total_extracted_files >= 2); // database.sqlite + manifest.json
        assert!(staged.total_extracted_bytes > 0);
        assert!(
            Path::new(&staged.staging_dir).exists(),
            "Staging dir must exist"
        );
        assert!(
            Path::new(&staged.database_path).exists(),
            "Staged database must exist"
        );

        // 2. Cleanup staging directory
        let staging_path = Path::new(&staged.staging_dir);
        let cleaned = RestoreService::cleanup_staged_restore(staging_path, &paths).unwrap();
        assert!(cleaned);
        assert!(
            !staging_path.exists(),
            "Staging directory must be removed after cleanup"
        );

        // Invariant: live database is untouched
        let live_notes: i64 = conn
            .query_row("SELECT COUNT(1) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(live_notes, 1);
    }

    #[test]
    fn test_task_17_duplicate_entry_rejection() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_dup_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let archive_path = temp_dir.join("duplicate_entry.pnbackup");
        let file = File::create(&archive_path).unwrap();
        let mut zip = ZipWriter::new(file);

        let options = SimpleFileOptions::default();
        zip.start_file("note.txt", options).unwrap();
        zip.write_all(b"first file content").unwrap();

        // Add duplicate entry via alternate normalized representation (e.g. ./note.txt)
        zip.start_file("./note.txt", options).unwrap();
        zip.write_all(b"second duplicate content").unwrap();

        zip.finish().unwrap();

        let res = RestoreService::stage_restore_archive(&archive_path, &paths);
        assert!(
            res.is_err(),
            "Must reject archive containing duplicate entries"
        );

        // No staging directory should remain
        let staging_dirs_left = fs::read_dir(&paths.backups_dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".restore_staging_")
            })
            .count();
        assert_eq!(
            staging_dirs_left, 0,
            "Failed staging must leave no orphaned directories"
        );
    }

    #[test]
    fn test_task_17_cleanup_staged_restore_guard_rails() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_guards_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // 1. Live database path cannot be passed
        let res = RestoreService::cleanup_staged_restore(&paths.database_file, &paths);
        assert!(res.is_err());

        // 2. Root directory cannot be passed
        let res = RestoreService::cleanup_staged_restore(&paths.root, &paths);
        assert!(res.is_err());

        // 3. Backups directory cannot be passed
        let res = RestoreService::cleanup_staged_restore(&paths.backups_dir, &paths);
        assert!(res.is_err());

        // 4. Arbitrary folder without .restore_staging_ prefix cannot be passed
        let normal_folder = paths.backups_dir.join("normal_folder");
        fs::create_dir_all(&normal_folder).unwrap();
        let res = RestoreService::cleanup_staged_restore(&normal_folder, &paths);
        assert!(res.is_err());
        assert!(
            normal_folder.exists(),
            "Protected folder must not be deleted"
        );
    }

    #[test]
    fn test_task_18_migrate_staged_database_v1_to_v4() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_mig_v1_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // 1. Build a v1 legacy SQLite database
        let legacy_db_dir = temp_dir.join("legacy_v1");
        fs::create_dir_all(&legacy_db_dir).unwrap();
        let legacy_db_path = legacy_db_dir.join("database.sqlite");

        let mut v1_conn = init_connection(&legacy_db_path).unwrap();
        // Run only migration 1
        crate::storage::migrations::run_migrations_custom(
            &mut v1_conn,
            &crate::storage::migrations::MIGRATIONS[..1],
        )
        .unwrap();

        // Verify version is 1
        let ver: i32 = v1_conn
            .query_row("SELECT MAX(version) FROM _migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver, 1);

        // Insert legacy note
        let note_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        v1_conn
            .execute(
                "INSERT INTO notes (id, title, content, format, created_at, modified_at)
             VALUES (?1, 'Legacy v1 Note', 'Content from ancient version 1', 'txt', ?2, ?2)",
                [&note_id, &now],
            )
            .unwrap();
        drop(v1_conn);

        // Package into a zip archive
        let archive_path = temp_dir.join("legacy_v1_backup.pnbackup");
        let file = File::create(&archive_path).unwrap();
        let mut zip = ZipWriter::new(file);

        let options = SimpleFileOptions::default();
        zip.start_file("database.sqlite", options).unwrap();
        let db_bytes = fs::read(&legacy_db_path).unwrap();
        zip.write_all(&db_bytes).unwrap();
        zip.finish().unwrap();

        // Stage the archive
        let staged = RestoreService::stage_restore_archive(&archive_path, &paths).unwrap();
        assert_eq!(
            staged.initial_schema_version, 1,
            "Staged database must start at schema v1"
        );

        let staging_path = PathBuf::from(&staged.staging_dir);

        // 2. Run Task 18 staged migration
        let mig_result = RestoreService::migrate_staged_database(&staging_path, &paths).unwrap();
        assert_eq!(mig_result.previous_schema_version, 1);
        assert_eq!(
            mig_result.migrated_schema_version,
            CURRENT_APP_SCHEMA_VERSION
        );
        assert_eq!(
            mig_result.migrations_applied,
            (CURRENT_APP_SCHEMA_VERSION - 1) as usize
        );
        assert!(mig_result.is_verified);
        assert_eq!(
            mig_result.note_count, 1,
            "Legacy note must survive migration intact"
        );

        // Verify migrated database directly
        let migrated_conn = init_connection(&staging_path.join("database.sqlite")).unwrap();
        let migrated_title: String = migrated_conn
            .query_row("SELECT title FROM notes WHERE id = ?1", [&note_id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(migrated_title, "Legacy v1 Note");
        drop(migrated_conn);

        // Clean up
        let _ = RestoreService::cleanup_staged_restore(&staging_path, &paths);
    }

    #[test]
    fn test_task_18_staged_migration_incompatible_newer_schema_rejected() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_mig_new_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // Build a simulated future database (v99)
        let future_dir = paths
            .backups_dir
            .join(format!(".restore_staging_{}", Uuid::new_v4()));
        fs::create_dir_all(&future_dir).unwrap();
        let future_db = future_dir.join("database.sqlite");

        let mut conn = init_connection(&future_db).unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute("INSERT INTO _migrations (version, name, applied_at) VALUES (99, '099_future_spec', '2030-01-01')", []).unwrap();
        drop(conn);

        // Attempt staged migration
        let res = RestoreService::migrate_staged_database(&future_dir, &paths);
        assert!(res.is_err(), "Must reject newer unsupported schema version");

        match res.unwrap_err() {
            BackupError::IncompatibleSchema {
                backup_schema,
                current_schema,
            } => {
                assert_eq!(backup_schema, 99);
                assert_eq!(current_schema, CURRENT_APP_SCHEMA_VERSION);
            }
            other => panic!("Expected IncompatibleSchema error, got: {other:?}"),
        }

        // Staging directory must be cleaned up on rejection
        assert!(
            !future_dir.exists(),
            "Incompatible staging directory must be purged"
        );
    }

    #[test]
    fn test_task_18_staged_migration_already_latest_schema_noop() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_mig_noop_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let latest_staging = paths
            .backups_dir
            .join(format!(".restore_staging_{}", Uuid::new_v4()));
        fs::create_dir_all(&latest_staging).unwrap();
        let latest_db = latest_staging.join("database.sqlite");

        let mut conn = init_connection(&latest_db).unwrap();
        run_migrations(&mut conn).unwrap();
        drop(conn);

        let res = RestoreService::migrate_staged_database(&latest_staging, &paths).unwrap();
        assert_eq!(res.previous_schema_version, CURRENT_APP_SCHEMA_VERSION);
        assert_eq!(res.migrated_schema_version, CURRENT_APP_SCHEMA_VERSION);
        assert_eq!(
            res.migrations_applied, 0,
            "No migrations should be run on already-latest database"
        );
        assert!(res.is_verified);

        let _ = RestoreService::cleanup_staged_restore(&latest_staging, &paths);
    }

    #[test]
    fn test_task_19_execute_restore_happy_path_success() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_rest_happy_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // 1. Initialize live database with "Active Original Note A"
        let mut live_conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut live_conn).unwrap();
        NoteRepository::create(
            &live_conn,
            CreateNoteDto {
                title: Some("Active Original Note A".to_string()),
                content: Some("Content before restore".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        // 2. Build candidate backup archive in a separate location with "Restored Note B" and "Restored Note C"
        let candidate_root = temp_dir.join("candidate_builder");
        let candidate_paths = StoragePaths::from_root(candidate_root);
        candidate_paths.ensure_directories().unwrap();

        let mut candidate_conn = init_connection(&candidate_paths.database_file).unwrap();
        run_migrations(&mut candidate_conn).unwrap();
        NoteRepository::create(
            &candidate_conn,
            CreateNoteDto {
                title: Some("Restored Note B".to_string()),
                content: Some("Backup content B".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();
        NoteRepository::create(
            &candidate_conn,
            CreateNoteDto {
                title: Some("Restored Note C".to_string()),
                content: Some("Backup content C".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        let candidate_archive = BackupService::create_backup(
            &candidate_conn,
            &candidate_paths,
            BackupCreateRequest {
                destination_path: None,
                backup_type: None,
                allow_overwrite: Some(false),
            },
            None::<fn(BackupProgressEvent)>,
        )
        .unwrap();
        drop(candidate_conn);

        // 3. Execute controlled restore on live_conn
        let restore_res = RestoreService::execute_restore(
            &mut live_conn,
            &paths,
            BackupRestoreRequest {
                file_path: candidate_archive.file_path.clone(),
                operation_id: None,
            },
            None::<fn(BackupProgressEvent)>,
        )
        .unwrap();

        assert!(restore_res.success);
        assert_eq!(
            restore_res.note_count, 2,
            "Restored state must contain 2 notes"
        );
        assert_eq!(restore_res.trash_count, 0);

        // Pre-restore checkpoint must exist on disk
        let chk_path = PathBuf::from(&restore_res.checkpoint_path);
        assert!(
            chk_path.exists(),
            "Pre-restore checkpoint must be preserved on disk"
        );

        // Verify active database contains restored notes and NO longer Note A
        let titles: Vec<String> = live_conn
            .prepare("SELECT title FROM notes ORDER BY title ASC")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(titles, vec!["Restored Note B", "Restored Note C"]);

        // Verify pre-restore checkpoint contains Original Note A
        let chk_val = RestoreService::validate_recovery_checkpoint(&chk_path, &paths).unwrap();
        assert_eq!(
            chk_val.manifest.unwrap().database.note_count,
            1,
            "Checkpoint must have saved original 1 note"
        );

        // Verify marker and staging directories are gone
        assert!(
            !paths.restore_marker_file().exists(),
            "Restore marker file must be deleted on commit"
        );

        let staging_dirs_count = fs::read_dir(&paths.backups_dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.path().is_dir()
                    && e.file_name()
                        .to_string_lossy()
                        .starts_with(".restore_staging_")
            })
            .count();
        assert_eq!(
            staging_dirs_count, 0,
            "No staging directories should remain after successful restore"
        );
    }

    #[test]
    fn test_task_19_perform_rollback_restores_pre_restore_state() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_rest_rollback_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // 1. Initialize live database with "Original Preserved Note"
        let mut live_conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut live_conn).unwrap();
        NoteRepository::create(
            &live_conn,
            CreateNoteDto {
                title: Some("Original Preserved Note".to_string()),
                content: Some("Data that must survive rollback".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        // 2. Create pre-restore checkpoint
        let checkpoint = RestoreService::create_recovery_checkpoint(&live_conn, &paths).unwrap();

        // 3. Simulate file swap with a corrupted database
        let op_id = Uuid::new_v4().to_string();
        let live_db_backup = paths.root.join(format!(".db_pre_restore_{op_id}.sqlite"));
        let live_wal_backup = paths
            .root
            .join(format!(".db_pre_restore_{op_id}.sqlite-wal"));
        let live_shm_backup = paths
            .root
            .join(format!(".db_pre_restore_{op_id}.sqlite-shm"));
        let live_att_backup = paths.root.join(format!(".att_pre_restore_{op_id}"));

        // Flush and close file connection before moving files
        let _ = live_conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        live_conn = Connection::open_in_memory().unwrap();

        fs::rename(&paths.database_file, &live_db_backup).unwrap();
        fs::create_dir_all(&live_att_backup).unwrap();

        // Put bad database in place
        fs::write(&paths.database_file, b"corrupted file that causes error").unwrap();

        let mut marker = RestoreActivationMarker {
            operation_id: op_id,
            started_at: chrono::Utc::now().to_rfc3339(),
            candidate_archive_path: "/dummy/candidate.pnbackup".to_string(),
            staging_dir: paths
                .backups_dir
                .join(".restore_staging_dummy")
                .to_string_lossy()
                .to_string(),
            checkpoint_path: checkpoint.file_path.clone(),
            live_db_backup_path: live_db_backup.to_string_lossy().to_string(),
            live_attachments_backup_path: live_att_backup.to_string_lossy().to_string(),
            phase: RestoreActivationPhase::FilesSwapped,
        };
        RestoreService::write_activation_marker(&marker, &paths).unwrap();

        // 4. Trigger perform_rollback
        RestoreService::perform_rollback(
            &mut live_conn,
            &paths,
            &live_db_backup,
            &live_wal_backup,
            &live_shm_backup,
            &live_att_backup,
            &checkpoint.file_path,
            &mut marker,
        )
        .unwrap();

        // 5. Verify rollback restored live database cleanly
        assert!(
            !paths.restore_marker_file().exists(),
            "Marker must be cleared after rollback"
        );
        assert!(
            !live_db_backup.exists(),
            "Pre-restore database swap backup must be moved back"
        );

        let title: String = live_conn
            .query_row("SELECT title FROM notes LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            title, "Original Preserved Note",
            "Rollback must restore original live note intact"
        );

        // Pre-restore checkpoint must still exist for manual recovery option
        let checkpoints = RestoreService::list_recovery_checkpoints(&paths).unwrap();
        assert_eq!(
            checkpoints.len(),
            1,
            "Recovery checkpoint created before swap must remain safe and accessible"
        );
    }

    #[test]
    fn test_task_19_crash_recovery_from_marker_during_interrupted_restore() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_rest_crash_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // 1. Create a simulated live database with Note X
        let mut live_conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut live_conn).unwrap();
        NoteRepository::create(
            &live_conn,
            CreateNoteDto {
                title: Some("Survivor Note X".to_string()),
                content: Some("Content that must survive crash".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();
        drop(live_conn);

        // 2. Simulate crash midway through restore:
        // Rename database to pre_restore swap backup
        let op_id = Uuid::new_v4().to_string();
        let live_db_backup = paths.root.join(format!(".db_pre_restore_{op_id}.sqlite"));
        let live_att_backup = paths.root.join(format!(".att_pre_restore_{op_id}"));
        fs::rename(&paths.database_file, &live_db_backup).unwrap();
        fs::create_dir_all(&live_att_backup).unwrap();

        // Put corrupted/broken file as active database
        fs::write(&paths.database_file, b"corrupted partial database contents").unwrap();

        // Write marker file indicating crash occurred at FilesSwapped phase
        let marker = RestoreActivationMarker {
            operation_id: op_id,
            started_at: chrono::Utc::now().to_rfc3339(),
            candidate_archive_path: "/dummy/candidate.pnbackup".to_string(),
            staging_dir: paths
                .backups_dir
                .join(".restore_staging_dummy")
                .to_string_lossy()
                .to_string(),
            checkpoint_path: "/dummy/chk.pnbackup".to_string(),
            live_db_backup_path: live_db_backup.to_string_lossy().to_string(),
            live_attachments_backup_path: live_att_backup.to_string_lossy().to_string(),
            phase: RestoreActivationPhase::FilesSwapped,
        };
        RestoreService::write_activation_marker(&marker, &paths).unwrap();
        assert!(paths.restore_marker_file().exists());

        // 3. Trigger crash recovery
        let recovery_res = RestoreService::recover_if_interrupted(&paths).unwrap();
        assert!(
            recovery_res.is_some(),
            "Recovery must report successful action"
        );

        // 4. Verify live database is restored and operational
        assert!(
            !paths.restore_marker_file().exists(),
            "Marker must be cleared after crash recovery"
        );
        assert!(
            !live_db_backup.exists(),
            "Temporary pre-restore backup must be moved back"
        );

        let conn = init_connection(&paths.database_file).unwrap();
        let title: String = conn
            .query_row("SELECT title FROM notes LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            title, "Survivor Note X",
            "Live data must be fully restored from crash"
        );
    }

    #[test]
    fn test_task_19_execute_restore_aborts_on_invalid_backup_leaving_live_data_untouched() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_rest_abort_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // Initialize live database with note
        let mut live_conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut live_conn).unwrap();
        NoteRepository::create(
            &live_conn,
            CreateNoteDto {
                title: Some("Protected Live Note".to_string()),
                content: Some("Live content".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        // 1. Pass nonexistent path
        let err1 = RestoreService::execute_restore(
            &mut live_conn,
            &paths,
            BackupRestoreRequest {
                file_path: temp_dir
                    .join("nonexistent.pnbackup")
                    .to_string_lossy()
                    .to_string(),
                operation_id: None,
            },
            None::<fn(BackupProgressEvent)>,
        );
        assert!(err1.is_err());

        // 2. Pass corrupt file
        let corrupt_path = temp_dir.join("corrupt.pnbackup");
        fs::write(&corrupt_path, b"not a zip archive").unwrap();
        let err2 = RestoreService::execute_restore(
            &mut live_conn,
            &paths,
            BackupRestoreRequest {
                file_path: corrupt_path.to_string_lossy().to_string(),
                operation_id: None,
            },
            None::<fn(BackupProgressEvent)>,
        );
        assert!(err2.is_err());

        // 3. Verify live database is completely intact and healthy
        let title: String = live_conn
            .query_row("SELECT title FROM notes LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "Protected Live Note");
        assert!(!paths.restore_marker_file().exists());
    }

    #[test]
    fn test_task_20_verify_and_reinitialize_activated_state_success() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_rest_t20_val_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        // Seed notebook, note, internal link, attachment, and setting
        let nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Science Notes".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Quantum Teleportation".to_string()),
                content: Some("Reference to [Physics Note](notepad://note/phys-101)".to_string()),
                format: Some("md".to_string()),
                notebook_id: Some(nb.id.clone()),
                is_favorite: Some(true),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        let tag = TagRepository::create(&conn, "physics-tag").unwrap();
        TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).unwrap();

        let att_dir = paths.get_note_attachments_dir(&note.id).unwrap();
        let att_file = format!("{}.pdf", Uuid::new_v4());
        fs::write(att_dir.join(&att_file), b"%PDF-1.4 sample content").unwrap();

        let now_str = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                Uuid::new_v4().to_string(),
                note.id,
                "quantum.pdf",
                format!("{}/{}", note.id, att_file),
                "application/pdf",
                25i64,
                now_str,
                now_str,
            ],
        ).unwrap();

        // Run post-restore validation
        let report =
            RestoreService::verify_and_reinitialize_activated_state(&conn, &paths).unwrap();
        assert!(report.is_valid);
        assert!(report.integrity_check_passed);
        assert!(report.foreign_key_check_passed);
        assert_eq!(report.schema_version, CURRENT_APP_SCHEMA_VERSION);
        assert_eq!(report.note_count, 1);
        assert_eq!(report.notebook_count, 1);
        assert_eq!(report.tag_count, 1);
        assert_eq!(report.attachment_count, 1);
        assert!(report.attachments_verified_on_disk);
        assert!(report.fts_rebuilt_and_queryable);
        assert!(report.internal_links_verified);
        assert!(report.settings_verified);
    }

    #[test]
    fn test_task_20_verify_and_reinitialize_fails_on_missing_attachment_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_rest_t20_fail_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note Missing Attachment".to_string()),
                content: Some("Body".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        // Insert attachment record pointing to file that does NOT exist on disk
        let now_str = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                Uuid::new_v4().to_string(),
                note.id,
                "missing.pdf",
                format!("{}/nonexistent.pdf", note.id),
                "application/pdf",
                100i64,
                now_str,
                now_str,
            ],
        ).unwrap();

        let val_err =
            RestoreService::verify_and_reinitialize_activated_state(&conn, &paths).unwrap_err();
        match val_err {
            BackupError::PostRestoreValidationFailed(msg) => {
                assert!(msg.contains("Restored attachment file is missing on disk"));
            }
            other => panic!("Expected PostRestoreValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_task_20_restored_database_survives_controlled_restart() {
        let live_dir =
            std::env::temp_dir().join(format!("pn_test_rest_t20_restart_live_{}", Uuid::new_v4()));
        let live_paths = StoragePaths::from_root(live_dir.clone());
        live_paths.ensure_directories().unwrap();

        let mut live_conn = init_connection(&live_paths.database_file).unwrap();
        run_migrations(&mut live_conn).unwrap();
        NoteRepository::create(
            &live_conn,
            CreateNoteDto {
                title: Some("Initial Old Note".to_string()),
                content: Some("Old live data".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        // Create candidate backup environment
        let cand_dir =
            std::env::temp_dir().join(format!("pn_test_rest_t20_restart_cand_{}", Uuid::new_v4()));
        let cand_paths = StoragePaths::from_root(cand_dir.clone());
        cand_paths.ensure_directories().unwrap();

        let mut cand_conn = init_connection(&cand_paths.database_file).unwrap();
        run_migrations(&mut cand_conn).unwrap();

        let cand_note = NoteRepository::create(
            &cand_conn,
            CreateNoteDto {
                title: Some("Restored Durable Note".to_string()),
                content: Some(
                    "Persists across full application restarts with keyword supernova".to_string(),
                ),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(true),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        let cand_att_dir = cand_paths.get_note_attachments_dir(&cand_note.id).unwrap();
        let cand_att_file = format!("{}.txt", Uuid::new_v4());
        fs::write(
            cand_att_dir.join(&cand_att_file),
            b"supernova durable attachment content",
        )
        .unwrap();

        let cand_now = chrono::Utc::now().to_rfc3339();
        cand_conn.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                Uuid::new_v4().to_string(),
                cand_note.id,
                "supernova.txt",
                format!("{}/{}", cand_note.id, cand_att_file),
                "text/plain",
                37i64,
                cand_now,
                cand_now,
            ],
        ).unwrap();

        let candidate_backup = BackupService::create_backup(
            &cand_conn,
            &cand_paths,
            BackupCreateRequest {
                destination_path: None,
                backup_type: None,
                allow_overwrite: Some(false),
            },
            None::<fn(BackupProgressEvent)>,
        )
        .unwrap();
        drop(cand_conn);

        // 1. Execute restore
        let restore_res = RestoreService::execute_restore(
            &mut live_conn,
            &live_paths,
            BackupRestoreRequest {
                file_path: candidate_backup.file_path.clone(),
                operation_id: None,
            },
            None::<fn(BackupProgressEvent)>,
        )
        .unwrap();

        assert!(restore_res.success);
        let report = restore_res
            .validation_report
            .expect("Validation report must be present");
        assert!(report.is_valid);
        assert!(report.attachments_verified_on_disk);
        assert!(report.fts_rebuilt_and_queryable);

        // 2. Close active connection to simulate complete application exit
        drop(live_conn);

        // 3. Simulate application startup lifecycle (setup in lib.rs)
        let recovered = RestoreService::recover_if_interrupted(&live_paths).unwrap();
        assert!(
            recovered.is_none(),
            "Normal shutdown after completed restore must have no marker to recover"
        );

        let mut restarted_conn = init_connection(&live_paths.database_file).unwrap();
        run_migrations(&mut restarted_conn).unwrap();

        // 4. Verify data persistence and search queryability across restart
        let titles: Vec<String> = restarted_conn
            .prepare("SELECT title FROM notes WHERE is_deleted = 0")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(titles, vec!["Restored Durable Note"]);

        let fts_count: i64 = restarted_conn
            .query_row(
                "SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'supernova*'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            fts_count, 1,
            "FTS5 search index must function immediately after restart"
        );

        let att_disk_path = live_paths
            .attachments_dir
            .join(&cand_note.id)
            .join(&cand_att_file);
        assert!(
            att_disk_path.exists(),
            "Restored attachment file must exist on disk across restarts"
        );
    }

    #[test]
    fn test_task_21_restore_cancellation_during_preparation_purges_staging_and_preserves_live_data()
    {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_rest_cancel_prep_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // 1. Create a candidate backup archive
        let cand_temp = std::env::temp_dir().join(format!("pn_cand_{}", Uuid::new_v4()));
        let cand_paths = StoragePaths::from_root(cand_temp.clone());
        cand_paths.ensure_directories().unwrap();
        let mut cand_conn = init_connection(&cand_paths.database_file).unwrap();
        run_migrations(&mut cand_conn).unwrap();
        NoteRepository::create(
            &cand_conn,
            CreateNoteDto {
                title: Some("Candidate Note".to_string()),
                content: Some("Candidate Content".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        )
        .unwrap();
        let cand_backup = BackupService::create_backup(
            &cand_conn,
            &cand_paths,
            BackupCreateRequest {
                destination_path: None,
                backup_type: Some(BackupType::Manual),
                allow_overwrite: Some(false),
            },
            None::<fn(BackupProgressEvent)>,
        )
        .unwrap();

        // 2. Setup live database with current working note
        let mut live_conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut live_conn).unwrap();
        let live_note = NoteRepository::create(
            &live_conn,
            CreateNoteDto {
                title: Some("Live Working Note Must Survive".to_string()),
                content: Some("Live Content".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        )
        .unwrap();

        // 3. Register cancellation for op_id before restore runs
        let op_id = format!("op-cancel-test-{}", Uuid::new_v4());
        RestoreService::request_cancellation(&op_id);

        let mut events: Vec<BackupProgressEvent> = Vec::new();
        let res = RestoreService::execute_restore(
            &mut live_conn,
            &paths,
            BackupRestoreRequest {
                file_path: cand_backup.file_path.clone(),
                operation_id: Some(op_id.clone()),
            },
            Some(|evt| events.push(evt)),
        );

        // 4. Invariants check
        assert!(res.is_err(), "Restore must fail with cancellation error");
        match res.err().unwrap() {
            BackupError::Cancelled => {}
            other => panic!("Expected BackupError::Cancelled, got {:?}", other),
        }

        // Live data must remain 100% intact
        let retrieved = NoteRepository::get_by_id(&live_conn, &live_note.id)
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.title, "Live Working Note Must Survive");

        // Staging folders must be completely cleaned up
        let staging_entries = fs::read_dir(&paths.backups_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".restore_staging_")
            })
            .count();
        assert_eq!(
            staging_entries, 0,
            "All staging directories must be purged on cancellation"
        );

        // Final event must report Cancelled and safe live data status
        let last_evt = events.last().expect("Events must have been emitted");
        assert_eq!(last_evt.stage, BackupProgressStage::Cancelled);
        assert!(!last_evt.cancellable);
    }

    #[test]
    fn test_task_21_cancellation_after_checkpoint_preserves_checkpoint_and_locks_activation() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_rest_cancel_chk_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // 1. Create a candidate backup archive
        let cand_temp = std::env::temp_dir().join(format!("pn_cand_chk_{}", Uuid::new_v4()));
        let cand_paths = StoragePaths::from_root(cand_temp.clone());
        cand_paths.ensure_directories().unwrap();
        let mut cand_conn = init_connection(&cand_paths.database_file).unwrap();
        run_migrations(&mut cand_conn).unwrap();
        NoteRepository::create(
            &cand_conn,
            CreateNoteDto {
                title: Some("Candidate Note".to_string()),
                content: Some("Candidate Content".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        )
        .unwrap();
        let cand_backup = BackupService::create_backup(
            &cand_conn,
            &cand_paths,
            BackupCreateRequest {
                destination_path: None,
                backup_type: Some(BackupType::Manual),
                allow_overwrite: Some(false),
            },
            None::<fn(BackupProgressEvent)>,
        )
        .unwrap();

        // 2. Setup live database with current working note
        let mut live_conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut live_conn).unwrap();
        let live_note = NoteRepository::create(
            &live_conn,
            CreateNoteDto {
                title: Some("Live Note Before Checkpoint".to_string()),
                content: Some("Content Before Checkpoint".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        )
        .unwrap();

        let op_id = format!("op-chk-cancel-{}", Uuid::new_v4());
        let op_id_clone = op_id.clone();

        let mut events: Vec<BackupProgressEvent> = Vec::new();
        // Request cancellation when CreatingCheckpoint stage is emitted
        let res = RestoreService::execute_restore(
            &mut live_conn,
            &paths,
            BackupRestoreRequest {
                file_path: cand_backup.file_path.clone(),
                operation_id: Some(op_id.clone()),
            },
            Some(|evt: BackupProgressEvent| {
                if evt.stage == BackupProgressStage::CreatingCheckpoint {
                    RestoreService::request_cancellation(&op_id_clone);
                }
                events.push(evt);
            }),
        );

        assert!(res.is_err());
        match res.err().unwrap() {
            BackupError::Cancelled => {}
            other => panic!("Expected BackupError::Cancelled, got {:?}", other),
        }

        // Live data must remain 100% intact
        let retrieved = NoteRepository::get_by_id(&live_conn, &live_note.id)
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.title, "Live Note Before Checkpoint");

        // The pre-restore recovery checkpoint must be safely preserved!
        let checkpoints = RestoreService::list_recovery_checkpoints(&paths).unwrap();
        assert_eq!(
            checkpoints.len(),
            1,
            "Pre-restore checkpoint created before cancellation must be preserved"
        );

        // Checkpoint file must exist on disk and be verified
        assert!(Path::new(&checkpoints[0].file_path).exists());
        assert!(checkpoints[0].is_verified);

        // Events emitted during preparation must have had cancellable: true
        let prep_events: Vec<_> = events
            .iter()
            .filter(|e| {
                e.stage == BackupProgressStage::Starting
                    || e.stage == BackupProgressStage::ValidatingBackup
                    || e.stage == BackupProgressStage::PreparingStagingData
                    || e.stage == BackupProgressStage::CheckingDatabaseIntegrity
                    || e.stage == BackupProgressStage::CreatingCheckpoint
            })
            .collect();
        assert!(!prep_events.is_empty());
        for e in prep_events {
            assert!(
                e.cancellable,
                "Preparation stage {:?} must be marked cancellable",
                e.stage
            );
        }

        // Cancellation event must confirm checkpoint preservation in message
        let cancel_event = events
            .iter()
            .find(|e| e.stage == BackupProgressStage::Cancelled)
            .unwrap();
        assert!(cancel_event
            .message
            .contains("checkpoint was safely preserved"));
    }
}
