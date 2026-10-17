use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::storage::errors::BackupError;
use crate::storage::models::{BackupHealthLevel, BackupHealthStatus, BackupHealthWarning};
use crate::storage::paths::StoragePaths;
use crate::storage::services::backup_listing::BackupListService;
use crate::storage::services::backup_scheduler::BackupSchedulerService;
use crate::storage::services::backup_settings::BackupSettingsService;

/// Service evaluating backup operational health, destination availability,
/// failure categories, disk isolation status, and active warnings (Phase 11, Task 22).
pub struct BackupHealthService;

impl BackupHealthService {
    /// Determines whether the specified backup destination directory is accessible and writable.
    pub fn check_destination_available(dest: &Path) -> bool {
        if !dest.exists() && fs::create_dir_all(dest).is_err() {
            return false;
        }

        if !dest.is_dir() {
            return false;
        }

        // Test writability using a lightweight probe file
        let probe_path = dest.join(format!(".probe_{}.tmp", Uuid::new_v4()));
        match fs::write(&probe_path, b"probe") {
            Ok(()) => {
                let _ = fs::remove_file(&probe_path);
                true
            }
            Err(_) => false,
        }
    }

    /// Evaluates whether the backup destination resides on the same physical drive / filesystem
    /// as the active application database to provide accurate hardware disaster recovery guidance.
    pub fn check_is_same_disk(dest: &Path, db_file: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let dest_probe = if dest.exists() {
                dest.to_path_buf()
            } else {
                dest.parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."))
            };

            let db_probe = if db_file.exists() {
                db_file.to_path_buf()
            } else {
                db_file
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."))
            };

            if let (Ok(m1), Ok(m2)) = (fs::metadata(&dest_probe), fs::metadata(&db_probe)) {
                return m1.dev() == m2.dev();
            }
        }

        // Fallback for non-unix or when metadata cannot be retrieved
        true
    }

    /// Compiles an authoritative, privacy-safe `BackupHealthStatus` snapshot.
    ///
    /// Guarantees:
    /// - Does not store or expose note contents, titles, or sensitive user text in health reports.
    /// - Checks destination accessibility non-blockingly.
    /// - Distinguishes discovered backup availability from verified recoverability.
    /// - Accurately reports whether backups are isolated to a separate storage volume.
    pub fn get_health_status(
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<BackupHealthStatus, BackupError> {
        let settings = BackupSettingsService::get_settings(conn).unwrap_or_default();
        let state = BackupSchedulerService::get_state(conn).unwrap_or_default();

        let dest_dir = settings
            .backup_destination_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.backups_dir.clone());

        let destination_path = dest_dir.to_string_lossy().to_string();
        let destination_available = Self::check_destination_available(&dest_dir);
        let is_same_disk = Self::check_is_same_disk(&dest_dir, &paths.database_file);

        // Discover backups across configured destinations
        let discovered = BackupListService::list_backups(conn, paths).unwrap_or_default();
        let managed_backup_count = discovered.len();

        let latest_backup = discovered.first();
        let latest_backup_name = latest_backup.map(|b| b.file_name.clone());
        let latest_backup_verified = latest_backup.map(|b| b.is_valid).unwrap_or(false);
        let has_verified_backup = discovered.iter().any(|b| b.is_valid);

        let mut warnings = Vec::new();

        // 1. Automatic backups disabled
        if !settings.auto_backup_enabled {
            warnings.push(BackupHealthWarning {
                code: "AUTO_BACKUP_DISABLED".to_string(),
                level: "warning".to_string(),
                message: "Automatic backups are disabled.".to_string(),
                detail: Some(
                    "Your notes are not being backed up automatically. Enable automatic backups in Settings to ensure regular snapshots are created."
                        .to_string(),
                ),
            });
        }

        // 2. Configured destination unavailable
        if !destination_available {
            warnings.push(BackupHealthWarning {
                code: "DESTINATION_UNAVAILABLE".to_string(),
                level: "critical".to_string(),
                message: "Configured backup destination is unavailable or unwritable.".to_string(),
                detail: Some(format!(
                    "The directory '{}' could not be written to. Check path permissions or connect your storage device.",
                    destination_path
                )),
            });
        }

        // 3. Automatic backup failure
        if state.failure_count > 0 || state.last_error_message.is_some() {
            let cat = state
                .last_failure_category
                .as_deref()
                .unwrap_or("BackupFailed");
            warnings.push(BackupHealthWarning {
                code: "LAST_BACKUP_FAILED".to_string(),
                level: "critical".to_string(),
                message: format!("Last automatic backup attempt failed: {cat}."),
                detail: state.last_error_message.clone(),
            });
        }

        // 4. Retention cleanup failure
        if let Some(ref ret_warning) = state.last_retention_warning {
            warnings.push(BackupHealthWarning {
                code: "RETENTION_CLEANUP_FAILED".to_string(),
                level: "warning".to_string(),
                message: "Automatic retention cleanup encountered issues.".to_string(),
                detail: Some(ret_warning.clone()),
            });
        }

        // 5. Latest backup cannot be verified
        if managed_backup_count > 0 && !latest_backup_verified {
            warnings.push(BackupHealthWarning {
                code: "LATEST_BACKUP_UNVERIFIED".to_string(),
                level: "warning".to_string(),
                message: "The most recent backup file failed verification or is corrupted."
                    .to_string(),
                detail: latest_backup_name.clone(),
            });
        }

        // 6. User has no verified backup and automatic backup is configured
        if settings.auto_backup_enabled && !has_verified_backup {
            warnings.push(BackupHealthWarning {
                code: "NO_VERIFIED_BACKUP".to_string(),
                level: "warning".to_string(),
                message: "No verified backup exists yet.".to_string(),
                detail: Some(
                    "An automatic backup has not completed successfully yet. Create a manual backup or wait for the scheduled backup job."
                        .to_string(),
                ),
            });
        }

        // 7. Same physical disk warning
        if is_same_disk {
            warnings.push(BackupHealthWarning {
                code: "SAME_PHYSICAL_DISK".to_string(),
                level: "info".to_string(),
                message: "Backup destination is on the primary system drive.".to_string(),
                detail: Some(
                    "Backups on the same disk protect against software corruption and accidental deletion, but do not protect against physical drive failure. An external drive is recommended."
                        .to_string(),
                ),
            });
        }

        // Determine overall health level
        let health_level = if !destination_available
            || (state.failure_count >= 3 && state.last_error_message.is_some())
        {
            BackupHealthLevel::Critical
        } else if !settings.auto_backup_enabled {
            BackupHealthLevel::Disabled
        } else if warnings
            .iter()
            .any(|w| w.level == "warning" || w.level == "critical")
        {
            BackupHealthLevel::Warning
        } else {
            BackupHealthLevel::Healthy
        };

        Ok(BackupHealthStatus {
            health_level,
            auto_backup_enabled: settings.auto_backup_enabled,
            destination_available,
            destination_path,
            is_same_disk,
            last_attempt_at: state.last_attempt_at,
            last_successful_at: state.last_successful_at,
            last_failure_category: state.last_failure_category,
            last_error_message: state.last_error_message,
            last_retention_warning: state.last_retention_warning,
            failure_count: state.failure_count,
            managed_backup_count,
            has_verified_backup,
            latest_backup_name,
            latest_backup_verified,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::BackupCreateRequest;
    use crate::storage::models::{BackupSettings, BackupType};
    use crate::storage::services::backup::BackupService;

    #[test]
    fn test_backup_health_status_on_fresh_database() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_health_{}", Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Fresh database with auto_backup_enabled = true (default) but no backups yet
        let health = BackupHealthService::get_health_status(&conn, &paths).unwrap();
        assert!(health.auto_backup_enabled);
        assert!(health.destination_available);
        assert!(!health.has_verified_backup);
        assert_eq!(health.managed_backup_count, 0);
        assert_eq!(health.health_level, BackupHealthLevel::Warning);

        // Contains NO_VERIFIED_BACKUP warning
        assert!(health
            .warnings
            .iter()
            .any(|w| w.code == "NO_VERIFIED_BACKUP"));

        // Clean up
        drop(conn);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_backup_health_status_healthy_after_backup_creation() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_health_ok_{}", Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Create an automatic backup
        let req = BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Automatic),
            allow_overwrite: Some(false),
        };
        let res = BackupService::create_backup(
            &conn,
            &paths,
            req,
            None::<fn(crate::storage::models::BackupProgressEvent)>,
        )
        .unwrap();
        assert!(res.verified);

        // Update scheduler state to reflect success
        let now_str = chrono::Utc::now().to_rfc3339();
        let mut state = BackupSchedulerService::get_state(&conn).unwrap();
        state.last_successful_at = Some(now_str);
        state.failure_count = 0;
        BackupSchedulerService::save_state(&conn, &state).unwrap();

        let health = BackupHealthService::get_health_status(&conn, &paths).unwrap();
        assert!(health.auto_backup_enabled);
        assert!(health.has_verified_backup);
        assert!(health.latest_backup_verified);
        assert_eq!(health.managed_backup_count, 1);
        assert!(health.last_successful_at.is_some());
        // Since it's on same disk, info warning exists, but level is healthy
        assert_eq!(health.health_level, BackupHealthLevel::Healthy);

        // Clean up
        drop(conn);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_backup_health_status_disabled_state() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_health_dis_{}", Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Disable automatic backups
        let settings = BackupSettings {
            auto_backup_enabled: false,
            ..BackupSettings::default()
        };
        BackupSettingsService::save_settings(&conn, &paths, settings).unwrap();

        let health = BackupHealthService::get_health_status(&conn, &paths).unwrap();
        assert!(!health.auto_backup_enabled);
        assert_eq!(health.health_level, BackupHealthLevel::Disabled);
        assert!(health
            .warnings
            .iter()
            .any(|w| w.code == "AUTO_BACKUP_DISABLED"));

        // Clean up
        drop(conn);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_backup_health_status_records_failure_category() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_health_fail_{}", Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Record a failure in scheduler state
        let mut state = BackupSchedulerService::get_state(&conn).unwrap();
        state.failure_count = 3;
        state.last_error_message = Some("Disk quota reached.".to_string());
        state.last_failure_category = Some("InsufficientDiskSpace".to_string());
        BackupSchedulerService::save_state(&conn, &state).unwrap();

        let health = BackupHealthService::get_health_status(&conn, &paths).unwrap();
        assert_eq!(health.health_level, BackupHealthLevel::Critical);
        assert_eq!(
            health.last_failure_category,
            Some("InsufficientDiskSpace".to_string())
        );
        assert!(health
            .warnings
            .iter()
            .any(|w| w.code == "LAST_BACKUP_FAILED"));

        // Clean up
        drop(conn);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
