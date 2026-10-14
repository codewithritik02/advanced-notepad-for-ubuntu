use std::fs;
use std::path::{Path, PathBuf};

use crate::storage::errors::BackupError;
use crate::storage::models::{BackupType, PRIMARY_BACKUP_EXTENSION};
use crate::storage::paths::StoragePaths;
use crate::storage::services::backup::generate_default_backup_filename;

/// Sanitizes a user-provided or generated backup filename stem to prevent path traversal
/// and filesystem encoding violations across operating systems (Phase 11, Task 9).
pub fn sanitize_backup_filename_stem(raw_name: &str) -> String {
    // 1. Strip illegal path and device characters
    let filtered: String = raw_name
        .chars()
        .filter(|c| {
            !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'..='\x1F')
        })
        .collect();

    // 2. Trim whitespace and leading/trailing dots (prevents hidden/traversal paths)
    let trimmed = filtered
        .trim()
        .trim_matches('.')
        .trim();

    if trimmed.is_empty() {
        return "personal-notepad-backup".to_string();
    }

    // 3. Neutralize Windows reserved device names
    let uppercase = trimmed.to_ascii_uppercase();
    let is_reserved = matches!(
        uppercase.as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
            | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    );

    let neutralized = if is_reserved {
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    };

    // 4. Bound length to 120 characters
    if neutralized.chars().count() > 120 {
        neutralized.chars().take(120).collect()
    } else {
        neutralized
    }
}

/// Service responsible for validating backup destinations and handling safe filename collisions.
pub struct BackupDestinationManager;

impl BackupDestinationManager {
    /// Resolves and validates a safe, non-destructive destination file path for a proposed backup.
    ///
    /// Safety Guarantees:
    /// 1. Forbids saving a backup over the active SQLite database (`database.sqlite`, `-wal`, `-shm`).
    /// 2. Forbids overwriting active attachment files in `attachments/`.
    /// 3. Never silently overwrites an existing backup file unless `allow_overwrite` is explicitly true.
    /// 4. If destination is a directory, generates a timestamped default filename and handles collision counter.
    /// 5. Validates directory writability before starting the backup.
    pub fn resolve_destination(
        custom_path: Option<&str>,
        backup_type: BackupType,
        allow_overwrite: bool,
        paths: &StoragePaths,
    ) -> Result<PathBuf, BackupError> {
        let requested_raw = custom_path.unwrap_or("").trim();

        let target_path: PathBuf = if requested_raw.is_empty() {
            // Default destination inside managed backups directory
            paths
                .ensure_directories()
                .map_err(|e| BackupError::DestinationUnavailable(format!("Failed to create backups directory: {e}")))?;
            let default_name = generate_default_backup_filename(backup_type);
            let candidate = paths.backups_dir.join(default_name);

            if candidate.exists() && !allow_overwrite {
                Self::find_collision_free_path(&paths.backups_dir, &candidate)?
            } else {
                candidate
            }
        } else {
            let requested = PathBuf::from(requested_raw);

            // Guard against writing over the live database
            let live_db_str = paths.database_file.to_string_lossy().to_string();
            let req_str = requested.to_string_lossy().to_string();

            if req_str == live_db_str
                || req_str.starts_with(&format!("{live_db_str}-"))
            {
                return Err(BackupError::DestinationUnavailable(
                    "Cannot write backup archive directly to the active SQLite database file or journal.".to_string(),
                ));
            }

            // Guard against writing directly as root user data directory
            if requested == paths.root {
                return Self::resolve_destination(None, backup_type, allow_overwrite, paths);
            }

            // Guard against writing directly as the attachments directory
            if requested == paths.attachments_dir {
                return Err(BackupError::DestinationUnavailable(
                    "Cannot overwrite the active attachments directory with a backup file.".to_string(),
                ));
            }

            // If the path is an existing directory (or ends with a slash)
            if requested.is_dir() || requested_raw.ends_with('/') || requested_raw.ends_with('\\') {
                if !requested.exists() {
                    fs::create_dir_all(&requested).map_err(BackupError::Io)?;
                }

                let default_name = generate_default_backup_filename(backup_type);
                let candidate = requested.join(default_name);

                if candidate.exists() && !allow_overwrite {
                    Self::find_collision_free_path(&requested, &candidate)?
                } else {
                    candidate
                }
            } else {
                // Requested path is a specific file path
                // If requested has no parent or parent is empty, resolve safely inside paths.backups_dir
                let parent_dir: PathBuf = match requested.parent() {
                    Some(p) if !p.as_os_str().is_empty() => {
                        if !p.exists() {
                            fs::create_dir_all(p).map_err(BackupError::Io)?;
                        }
                        p.to_path_buf()
                    }
                    _ => {
                        paths
                            .ensure_directories()
                            .map_err(|e| BackupError::DestinationUnavailable(format!("Failed to create backups directory: {e}")))?;
                        paths.backups_dir.clone()
                    }
                };

                // Extract and sanitize filename
                let raw_filename = requested
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| generate_default_backup_filename(backup_type));

                let (stem, has_valid_ext) = if raw_filename.ends_with(&format!(".{PRIMARY_BACKUP_EXTENSION}")) {
                    let s = &raw_filename[..raw_filename.len() - (PRIMARY_BACKUP_EXTENSION.len() + 1)];
                    (s, true)
                } else if raw_filename.ends_with(".zip") {
                    let s = &raw_filename[..raw_filename.len() - 4];
                    (s, true)
                } else {
                    (raw_filename.as_str(), false)
                };

                let sanitized_stem = sanitize_backup_filename_stem(stem);
                let final_filename = if has_valid_ext && raw_filename.ends_with(".zip") {
                    format!("{sanitized_stem}.zip")
                } else {
                    format!("{sanitized_stem}.{PRIMARY_BACKUP_EXTENSION}")
                };

                let final_path = parent_dir.join(final_filename);

                if final_path.exists() && !allow_overwrite {
                    return Err(BackupError::DestinationUnavailable(format!(
                        "A backup file already exists at '{}'. Explicit replacement confirmation is required.",
                        final_path.display()
                    )));
                }

                final_path
            }
        };

        // Validate that destination directory is writable
        let dest_dir = target_path.parent().unwrap_or(Path::new("."));
        Self::verify_directory_writable(dest_dir)?;

        Ok(target_path)
    }

    /// Finds a collision-free filename by appending sequential counters: `name (1).ext`, `name (2).ext`
    pub fn find_collision_free_path(dir: &Path, original_path: &Path) -> Result<PathBuf, BackupError> {
        let file_name = original_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "backup.pnbackup".to_string());

        let (stem, ext) = if let Some(dot_idx) = file_name.rfind('.') {
            (&file_name[..dot_idx], &file_name[dot_idx + 1..])
        } else {
            (file_name.as_str(), PRIMARY_BACKUP_EXTENSION)
        };

        let mut counter = 1;
        loop {
            let candidate_name = format!("{stem} ({counter}).{ext}");
            let candidate_path = dir.join(candidate_name);
            if !candidate_path.exists() {
                return Ok(candidate_path);
            }
            counter += 1;
            if counter > 9999 {
                return Err(BackupError::DestinationUnavailable(
                    "Excessive file collision count in destination directory.".to_string(),
                ));
            }
        }
    }

    /// Tests writability of the destination directory using a probe file
    fn verify_directory_writable(dir: &Path) -> Result<(), BackupError> {
        let probe_name = format!(".probe_write_{}", uuid::Uuid::new_v4());
        let probe_path = dir.join(probe_name);

        match fs::File::create(&probe_path) {
            Ok(file) => {
                drop(file);
                let _ = fs::remove_file(&probe_path);
                Ok(())
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::PermissionDenied => Err(BackupError::PermissionDenied(format!(
                    "Cannot write to directory '{}': Permission denied",
                    dir.display()
                ))),
                _ => Err(BackupError::Io(e)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_task_9_filename_sanitization_and_reserved_names() {
        assert_eq!(
            sanitize_backup_filename_stem("My:Backup / 2026? *Final* <v1>|Draft"),
            "MyBackup  2026 Final v1Draft"
        );
        assert_eq!(sanitize_backup_filename_stem("...dots..."), "dots");
        assert_eq!(sanitize_backup_filename_stem("   "), "personal-notepad-backup");
        assert_eq!(sanitize_backup_filename_stem("CON"), "_CON");
        assert_eq!(sanitize_backup_filename_stem("prn"), "_prn");
        assert_eq!(sanitize_backup_filename_stem("NUL"), "_NUL");
        assert_eq!(sanitize_backup_filename_stem("ValidBackupStem"), "ValidBackupStem");
    }

    #[test]
    fn test_task_9_destination_resolution_and_collision_prevention() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_dest_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("Ensure directories");

        // 1. Default destination resolves to backups directory with .pnbackup
        let default_res = BackupDestinationManager::resolve_destination(
            None,
            BackupType::Manual,
            false,
            &paths,
        )
        .expect("Default resolution");
        assert_eq!(default_res.parent().unwrap(), paths.backups_dir);
        assert!(default_res.to_string_lossy().ends_with(".pnbackup"));

        // 2. Reject destination targeting live database file
        let live_db_str = paths.database_file.to_string_lossy().to_string();
        let err_live = BackupDestinationManager::resolve_destination(
            Some(&live_db_str),
            BackupType::Manual,
            false,
            &paths,
        )
        .expect_err("Must reject writing over live DB");
        match err_live {
            BackupError::DestinationUnavailable(msg) => {
                assert!(msg.contains("active SQLite database"));
            }
            other => panic!("Expected DestinationUnavailable, got {other:?}"),
        }

        // 3. Collision handling without overwrite creates (1).pnbackup
        let fixed_name = "test-collision.pnbackup";
        let fixed_path = paths.backups_dir.join(fixed_name);
        fs::write(&fixed_path, b"existing").expect("Write dummy existing");

        let custom_str = fixed_path.to_string_lossy().to_string();
        // File target with allow_overwrite = false must reject replacement
        let err_exist = BackupDestinationManager::resolve_destination(
            Some(&custom_str),
            BackupType::Manual,
            false,
            &paths,
        )
        .expect_err("Must reject silent overwrite of custom file");
        match err_exist {
            BackupError::DestinationUnavailable(msg) => {
                assert!(msg.contains("already exists"));
            }
            other => panic!("Expected DestinationUnavailable, got {other:?}"),
        }

        // With allow_overwrite = true, replacement is permitted
        let overwrite_res = BackupDestinationManager::resolve_destination(
            Some(&custom_str),
            BackupType::Manual,
            true,
            &paths,
        )
        .expect("Must allow overwrite when confirmed");
        assert_eq!(overwrite_res, fixed_path);

        // Directory target with allow_overwrite = false automatically produces numbered collision-free path
        let dir_str = paths.backups_dir.to_string_lossy().to_string();
        let col_res = BackupDestinationManager::resolve_destination(
            Some(&dir_str),
            BackupType::Manual,
            false,
            &paths,
        )
        .expect("Directory target handles collisions gracefully");
        assert!(col_res.is_absolute());

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_9_unicode_preservation_and_length_bounding() {
        // Unicode preservation (Hindi, Japanese, emojis)
        let unicode_name = "नोट्स-बैकअप-2026-🚀-ノート";
        assert_eq!(sanitize_backup_filename_stem(unicode_name), unicode_name);

        // Bounded length to 120 characters
        let long_name = "a".repeat(200);
        let sanitized = sanitize_backup_filename_stem(&long_name);
        assert_eq!(sanitized.chars().count(), 120);
        assert_eq!(sanitized, "a".repeat(120));
    }

    #[test]
    fn test_task_9_bare_relative_filename_resolves_inside_backups_dir() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_bare_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("Ensure directories");

        // Bare filename without directories must resolve inside paths.backups_dir
        let resolved = BackupDestinationManager::resolve_destination(
            Some("my-custom-notes.pnbackup"),
            BackupType::Manual,
            false,
            &paths,
        )
        .expect("Resolve bare filename");

        assert_eq!(resolved.parent().unwrap(), paths.backups_dir);
        assert_eq!(resolved.file_name().unwrap(), "my-custom-notes.pnbackup");

        // Bare stem without extension gets .pnbackup appended
        let resolved_stem = BackupDestinationManager::resolve_destination(
            Some("notes-snapshot"),
            BackupType::Manual,
            false,
            &paths,
        )
        .expect("Resolve bare stem");

        assert_eq!(resolved_stem.parent().unwrap(), paths.backups_dir);
        assert_eq!(resolved_stem.file_name().unwrap(), "notes-snapshot.pnbackup");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_9_protect_attachments_and_root_user_directory() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_protect_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("Ensure directories");

        // Reject writing directly over attachments directory
        let att_str = paths.attachments_dir.to_string_lossy().to_string();
        let att_err = BackupDestinationManager::resolve_destination(
            Some(&att_str),
            BackupType::Manual,
            false,
            &paths,
        )
        .expect_err("Reject writing over attachments dir");

        match att_err {
            BackupError::DestinationUnavailable(msg) => {
                assert!(msg.contains("attachments"));
            }
            other => panic!("Expected DestinationUnavailable, got {other:?}"),
        }

        // Passing root user data directory safely redirects into paths.backups_dir
        let root_str = paths.root.to_string_lossy().to_string();
        let root_res = BackupDestinationManager::resolve_destination(
            Some(&root_str),
            BackupType::Manual,
            false,
            &paths,
        )
        .expect("Root directory redirects to backups dir");

        assert_eq!(root_res.parent().unwrap(), paths.backups_dir);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_9_sequential_collision_increments() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_col_seq_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("Ensure directories");

        let base_name = "archive.pnbackup";
        let base_path = paths.backups_dir.join(base_name);
        fs::write(&base_path, b"base").expect("write base");

        let col1_path = paths.backups_dir.join("archive (1).pnbackup");
        fs::write(&col1_path, b"col1").expect("write col1");

        let col2_path = paths.backups_dir.join("archive (2).pnbackup");
        fs::write(&col2_path, b"col2").expect("write col2");

        let found = BackupDestinationManager::find_collision_free_path(&paths.backups_dir, &base_path)
            .expect("find collision free path");

        assert_eq!(found.file_name().unwrap(), "archive (3).pnbackup");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
