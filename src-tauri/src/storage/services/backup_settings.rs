use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

use crate::storage::errors::BackupError;
use crate::storage::models::BackupSettings;
use crate::storage::paths::StoragePaths;
use crate::storage::repositories::settings::SettingsRepository;

/// Minimum allowed automatic backup interval in minutes (15 minutes).
pub const MIN_BACKUP_INTERVAL_MINUTES: u32 = 15;

/// Maximum allowed automatic backup interval in minutes (30 days = 43,200 minutes).
pub const MAX_BACKUP_INTERVAL_MINUTES: u32 = 43_200;

/// Minimum allowed retention count of automatic backups (1).
pub const MIN_BACKUP_RETENTION_COUNT: u32 = 1;

/// Maximum allowed retention count of automatic backups (50).
pub const MAX_BACKUP_RETENTION_COUNT: u32 = 50;

/// Database settings key used to persist automatic backup configuration.
pub const BACKUP_SETTINGS_KEY: &str = "backup_settings";

/// Service responsible for managing, persisting, and validating Automatic Backup Settings (Phase 11, Task 10).
pub struct BackupSettingsService;

impl BackupSettingsService {
    /// Loads persistent `BackupSettings` from the database.
    /// Returns default settings if no configuration is stored or if corrupted.
    pub fn get_settings(conn: &Connection) -> Result<BackupSettings, BackupError> {
        let stored_json = SettingsRepository::get(conn, BACKUP_SETTINGS_KEY)
            .map_err(|e| BackupError::DatabaseSnapshotFailed(e.to_string()))?;

        match stored_json {
            Some(raw) => match serde_json::from_str::<BackupSettings>(&raw) {
                Ok(settings) => Ok(settings),
                Err(e) => {
                    eprintln!("Warning: Malformed backup settings JSON in database: {e}. Falling back to default.");
                    Ok(BackupSettings::default())
                }
            },
            None => Ok(BackupSettings::default()),
        }
    }

    /// Validates and persists `BackupSettings` into the SQLite database.
    ///
    /// Validation Invariants:
    /// 1. `interval_minutes` must be bounded between 15 minutes and 30 days to prevent unbounded loops.
    /// 2. `retention_count` must be bounded between 1 and 50 to prevent unbounded disk usage or instant deletion.
    /// 3. `backup_destination_dir`:
    ///    - If provided, cannot point to the active SQLite database file or journal.
    ///    - Cannot point to the active attachments directory.
    ///    - Must exist or be creatable, and have write permissions.
    pub fn save_settings(
        conn: &Connection,
        paths: &StoragePaths,
        mut settings: BackupSettings,
    ) -> Result<BackupSettings, BackupError> {
        // 1. Validate interval_minutes
        if settings.interval_minutes < MIN_BACKUP_INTERVAL_MINUTES {
            return Err(BackupError::InvalidRequest(format!(
                "Automatic backup interval must be at least {MIN_BACKUP_INTERVAL_MINUTES} minutes (got {}).",
                settings.interval_minutes
            )));
        }
        if settings.interval_minutes > MAX_BACKUP_INTERVAL_MINUTES {
            return Err(BackupError::InvalidRequest(format!(
                "Automatic backup interval cannot exceed {MAX_BACKUP_INTERVAL_MINUTES} minutes (got {}).",
                settings.interval_minutes
            )));
        }

        // 2. Validate retention_count
        if settings.retention_count < MIN_BACKUP_RETENTION_COUNT {
            return Err(BackupError::InvalidRequest(format!(
                "Retention count must be at least {MIN_BACKUP_RETENTION_COUNT} (got {}).",
                settings.retention_count
            )));
        }
        if settings.retention_count > MAX_BACKUP_RETENTION_COUNT {
            return Err(BackupError::InvalidRequest(format!(
                "Retention count cannot exceed {MAX_BACKUP_RETENTION_COUNT} (got {}).",
                settings.retention_count
            )));
        }

        // 3. Validate backup_destination_dir
        if let Some(ref dir_str) = settings.backup_destination_dir {
            let trimmed = dir_str.trim();
            if trimmed.is_empty() {
                settings.backup_destination_dir = None;
            } else {
                let dest_path = PathBuf::from(trimmed);

                // Reject targeting active database
                let live_db_str = paths.database_file.to_string_lossy().to_string();
                if trimmed == live_db_str || trimmed.starts_with(&format!("{live_db_str}-")) {
                    return Err(BackupError::DestinationUnavailable(
                        "Backup destination directory cannot target the active SQLite database or journal.".to_string(),
                    ));
                }

                // Reject targeting attachments folder
                if dest_path == paths.attachments_dir {
                    return Err(BackupError::DestinationUnavailable(
                        "Backup destination cannot be set directly to the active attachments directory.".to_string(),
                    ));
                }

                // Ensure directory exists or can be created
                if !dest_path.exists() {
                    fs::create_dir_all(&dest_path).map_err(|e| {
                        BackupError::DestinationUnavailable(format!(
                            "Cannot create backup destination directory '{}': {e}",
                            dest_path.display()
                        ))
                    })?;
                }

                // Check directory is writable via probe file
                Self::verify_directory_writable(&dest_path)?;

                settings.backup_destination_dir = Some(trimmed.to_string());
            }
        }

        // 4. Persist serialized JSON
        let serialized = serde_json::to_string_pretty(&settings).map_err(|e| {
            BackupError::InvalidRequest(format!("Failed to serialize settings: {e}"))
        })?;

        SettingsRepository::set(conn, BACKUP_SETTINGS_KEY, &serialized)
            .map_err(|e| BackupError::DatabaseSnapshotFailed(e.to_string()))?;

        Ok(settings)
    }

    /// Verifies directory writability by creating and removing a probe file
    fn verify_directory_writable(dir: &Path) -> Result<(), BackupError> {
        let probe_path = dir.join(format!(".probe_settings_{}", uuid::Uuid::new_v4()));
        match fs::File::create(&probe_path) {
            Ok(file) => {
                drop(file);
                let _ = fs::remove_file(&probe_path);
                Ok(())
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    Err(BackupError::PermissionDenied(format!(
                        "Directory '{}' is not writable: Permission denied.",
                        dir.display()
                    )))
                }
                _ => Err(BackupError::Io(e)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use uuid::Uuid;

    #[test]
    fn test_task_10_backup_settings_defaults_and_roundtrip() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_bsettings_{}", Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection");
        run_migrations(&mut conn).expect("Run migrations");
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("Ensure directories");

        // 1. Initial state returns defaults
        let initial = BackupSettingsService::get_settings(&conn).expect("Get default settings");
        assert_eq!(initial, BackupSettings::default());
        assert_eq!(initial.auto_backup_enabled, true);
        assert_eq!(initial.interval_minutes, 1440);
        assert_eq!(initial.retention_count, 5);
        assert_eq!(initial.backup_destination_dir, None);

        // 2. Save custom valid settings
        let custom_dir = temp_dir.join("my_backups");
        let updated = BackupSettings {
            auto_backup_enabled: false,
            interval_minutes: 360,
            backup_destination_dir: Some(custom_dir.to_string_lossy().to_string()),
            retention_count: 10,
            backup_on_startup: false,
        };

        let saved = BackupSettingsService::save_settings(&conn, &paths, updated.clone())
            .expect("Save custom settings");
        assert_eq!(saved.auto_backup_enabled, false);
        assert_eq!(saved.interval_minutes, 360);
        assert_eq!(saved.retention_count, 10);
        assert!(custom_dir.is_dir());

        // 3. Re-read from database persists across calls
        let reloaded = BackupSettingsService::get_settings(&conn).expect("Reload settings");
        assert_eq!(reloaded, updated);

        // Clean up
        drop(conn);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_10_backup_settings_validation_bounds() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_bsettings_bounds_{}", Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection");
        run_migrations(&mut conn).expect("Run migrations");
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("Ensure directories");

        // Interval below minimum (14 < 15)
        let invalid_low_interval = BackupSettings {
            interval_minutes: 10,
            ..BackupSettings::default()
        };
        assert!(BackupSettingsService::save_settings(&conn, &paths, invalid_low_interval).is_err());

        // Interval above maximum (50,000 > 43,200)
        let invalid_high_interval = BackupSettings {
            interval_minutes: 50_000,
            ..BackupSettings::default()
        };
        assert!(
            BackupSettingsService::save_settings(&conn, &paths, invalid_high_interval).is_err()
        );

        // Retention below minimum (0 < 1)
        let invalid_low_retention = BackupSettings {
            retention_count: 0,
            ..BackupSettings::default()
        };
        assert!(
            BackupSettingsService::save_settings(&conn, &paths, invalid_low_retention).is_err()
        );

        // Retention above maximum (51 > 50)
        let invalid_high_retention = BackupSettings {
            retention_count: 55,
            ..BackupSettings::default()
        };
        assert!(
            BackupSettingsService::save_settings(&conn, &paths, invalid_high_retention).is_err()
        );

        // Clean up
        drop(conn);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_10_backup_settings_destination_safety_guards() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_bsettings_guards_{}", Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection");
        run_migrations(&mut conn).expect("Run migrations");
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("Ensure directories");

        // Cannot point destination to active SQLite database
        let live_db_str = paths.database_file.to_string_lossy().to_string();
        let target_live_db = BackupSettings {
            backup_destination_dir: Some(live_db_str),
            ..BackupSettings::default()
        };
        assert!(BackupSettingsService::save_settings(&conn, &paths, target_live_db).is_err());

        // Cannot point destination to active attachments directory
        let att_dir_str = paths.attachments_dir.to_string_lossy().to_string();
        let target_att_dir = BackupSettings {
            backup_destination_dir: Some(att_dir_str),
            ..BackupSettings::default()
        };
        assert!(BackupSettingsService::save_settings(&conn, &paths, target_att_dir).is_err());

        // Empty string destination gracefully normalizes to None (default managed folder)
        let empty_dest = BackupSettings {
            backup_destination_dir: Some("   ".to_string()),
            ..BackupSettings::default()
        };
        let saved = BackupSettingsService::save_settings(&conn, &paths, empty_dest)
            .expect("Save empty destination");
        assert_eq!(saved.backup_destination_dir, None);

        // Clean up
        drop(conn);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
