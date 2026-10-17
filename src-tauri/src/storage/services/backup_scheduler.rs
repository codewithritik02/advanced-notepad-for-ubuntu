use chrono::{DateTime, Utc};
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::storage::errors::BackupError;
use crate::storage::models::{
    BackupCreateRequest, BackupCreateResult, BackupProgressEvent, BackupSchedulerState,
    BackupSettings, BackupType,
};
use crate::storage::paths::StoragePaths;
use crate::storage::repositories::settings::SettingsRepository;
use crate::storage::services::backup::BackupService;
use crate::storage::services::backup_settings::BackupSettingsService;

/// Database settings key used to persist runtime scheduler state.
pub const BACKUP_SCHEDULER_STATE_KEY: &str = "backup_scheduler_state";

/// Maximum consecutive failures permitted before automatic retries are paused
/// to protect the application from infinite thrashing against unwritable disks.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// Base retry backoff interval in minutes following a failed backup attempt.
pub const MIN_RETRY_BACKOFF_MINUTES: i64 = 15;

/// Global atomic flag preventing concurrent backup operations across threads/tasks.
static BACKUP_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Global atomic flag signaling that a database restore operation is currently in progress.
static RESTORE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Global atomic flag signaling that an import or batch export operation is currently in progress (Phase 13, Task 20).
static IMPORT_EXPORT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// RAII Guard ensuring that the `BACKUP_IN_PROGRESS` flag is safely reset upon drop,
/// even in the event of an early exit or error.
#[derive(Debug)]
pub struct BackupExecutionGuard;

impl Drop for BackupExecutionGuard {
    fn drop(&mut self) {
        BACKUP_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

/// RAII Guard ensuring that the `RESTORE_IN_PROGRESS` flag is safely reset upon drop.
#[derive(Debug)]
pub struct RestoreExecutionGuard;

impl Drop for RestoreExecutionGuard {
    fn drop(&mut self) {
        RESTORE_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

/// RAII Guard ensuring that the `IMPORT_EXPORT_IN_PROGRESS` flag is safely reset upon drop (Phase 13, Task 20).
#[derive(Debug)]
pub struct ImportExportExecutionGuard;

impl Drop for ImportExportExecutionGuard {
    fn drop(&mut self) {
        IMPORT_EXPORT_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

/// Service managing the scheduling, lifecycle evaluation, concurrency controls,
/// and automated execution of background backups (Phase 11, Task 11; Phase 13, Task 20).
pub struct BackupSchedulerService;

impl BackupSchedulerService {
    /// Returns whether any backup operation is currently executing.
    pub fn is_backup_in_progress() -> bool {
        BACKUP_IN_PROGRESS.load(Ordering::SeqCst)
    }

    /// Returns whether a database restore operation is currently in progress.
    pub fn is_restore_in_progress() -> bool {
        RESTORE_IN_PROGRESS.load(Ordering::SeqCst)
    }

    /// Returns whether an import or export operation is currently in progress (Phase 13, Task 20).
    pub fn is_import_export_in_progress() -> bool {
        IMPORT_EXPORT_IN_PROGRESS.load(Ordering::SeqCst)
    }

    /// Sets or clears the active restore operation flag.
    pub fn set_restore_in_progress(active: bool) {
        RESTORE_IN_PROGRESS.store(active, Ordering::SeqCst);
    }

    /// Sets or clears the active import/export operation flag (Phase 13, Task 20).
    pub fn set_import_export_in_progress(active: bool) {
        IMPORT_EXPORT_IN_PROGRESS.store(active, Ordering::SeqCst);
    }

    /// Attempts to acquire an exclusive lock for backup execution.
    /// Fails if a restore, import/export, or another backup is already in progress.
    pub fn try_acquire_backup_lock() -> Result<BackupExecutionGuard, BackupError> {
        if Self::is_restore_in_progress() {
            return Err(BackupError::ConcurrentOperation(
                "A database restore operation is currently in progress.".to_string(),
            ));
        }

        if Self::is_import_export_in_progress() {
            return Err(BackupError::ConcurrentOperation(
                "An import or export operation is currently in progress.".to_string(),
            ));
        }

        if BACKUP_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(BackupError::ConcurrentOperation(
                "Another backup operation is currently executing.".to_string(),
            ));
        }

        Ok(BackupExecutionGuard)
    }

    /// Attempts to acquire an exclusive lock for restore execution.
    /// Fails if a backup, import/export, or another restore is already in progress.
    pub fn try_acquire_restore_lock() -> Result<RestoreExecutionGuard, BackupError> {
        if Self::is_backup_in_progress() {
            return Err(BackupError::ConcurrentOperation(
                "A database backup operation is currently executing.".to_string(),
            ));
        }

        if Self::is_import_export_in_progress() {
            return Err(BackupError::ConcurrentOperation(
                "An import or export operation is currently in progress.".to_string(),
            ));
        }

        if RESTORE_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(BackupError::ConcurrentOperation(
                "Another restore operation is currently executing.".to_string(),
            ));
        }

        Ok(RestoreExecutionGuard)
    }

    /// Attempts to acquire an exclusive lock for import or batch export execution (Phase 13, Task 20).
    /// Fails if a backup, restore, or another import/export is already in progress.
    pub fn try_acquire_import_export_lock() -> Result<ImportExportExecutionGuard, BackupError> {
        if Self::is_restore_in_progress() {
            return Err(BackupError::ConcurrentOperation(
                "A database restore operation is currently in progress.".to_string(),
            ));
        }

        if Self::is_backup_in_progress() {
            return Err(BackupError::ConcurrentOperation(
                "A database backup operation is currently executing.".to_string(),
            ));
        }

        if IMPORT_EXPORT_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(BackupError::ConcurrentOperation(
                "Another import or export operation is currently executing.".to_string(),
            ));
        }

        Ok(ImportExportExecutionGuard)
    }

    /// Loads the persistent scheduler state from SQLite settings.
    pub fn get_state(conn: &Connection) -> Result<BackupSchedulerState, BackupError> {
        let stored_json = SettingsRepository::get(conn, BACKUP_SCHEDULER_STATE_KEY)
            .map_err(|e| BackupError::DatabaseSnapshotFailed(e.to_string()))?;

        match stored_json {
            Some(raw) => match serde_json::from_str::<BackupSchedulerState>(&raw) {
                Ok(state) => Ok(state),
                Err(e) => {
                    eprintln!("Warning: Malformed backup scheduler state JSON: {e}. Falling back to default.");
                    Ok(BackupSchedulerState::default())
                }
            },
            None => Ok(BackupSchedulerState::default()),
        }
    }

    /// Persists the scheduler execution state into SQLite settings.
    pub fn save_state(conn: &Connection, state: &BackupSchedulerState) -> Result<(), BackupError> {
        let serialized = serde_json::to_string_pretty(state).map_err(|e| {
            BackupError::InvalidRequest(format!("Failed to serialize scheduler state: {e}"))
        })?;

        SettingsRepository::set(conn, BACKUP_SCHEDULER_STATE_KEY, &serialized)
            .map_err(|e| BackupError::DatabaseSnapshotFailed(e.to_string()))?;

        Ok(())
    }

    /// Evaluates whether an automatic backup is currently due according to policy:
    ///
    /// 1. User Preference: If `auto_backup_enabled` is false, backups never run.
    /// 2. Failure Threshold: If `failure_count >= MAX_CONSECUTIVE_FAILURES`, pause retries
    ///    to prevent infinite resource exhaustion.
    /// 3. Failure Backoff: If a previous attempt failed, require an elapsed backoff of
    ///    `MIN_RETRY_BACKOFF_MINUTES * failure_count` before retrying.
    /// 4. Rapid Restart Protection: If `last_successful_at` exists and elapsed minutes
    ///    since that backup is less than `interval_minutes`, NO backup is performed,
    ///    preventing duplicate archives on quick app reloads.
    /// 5. Interval Expiration: If elapsed minutes >= `interval_minutes`, backup is due.
    /// 6. First Run / Startup: If no successful backup exists:
    ///    - If startup and `backup_on_startup` is true, backup is due immediately.
    ///    - If routine interval check, backup is due if never attempted or backoff elapsed.
    pub fn is_backup_due(
        settings: &BackupSettings,
        state: &BackupSchedulerState,
        now: DateTime<Utc>,
        is_startup: bool,
    ) -> bool {
        // 1. User setting: disabled
        if !settings.auto_backup_enabled {
            return false;
        }

        // 2. Max consecutive failures threshold
        if state.failure_count >= MAX_CONSECUTIVE_FAILURES {
            return false;
        }

        // 3. Failure backoff check
        if state.failure_count > 0 {
            if let Some(ref attempt_str) = state.last_attempt_at {
                if let Ok(last_attempt) = DateTime::parse_from_rfc3339(attempt_str) {
                    let last_attempt_utc = last_attempt.with_timezone(&Utc);
                    let backoff = MIN_RETRY_BACKOFF_MINUTES * (state.failure_count as i64);
                    let elapsed = (now - last_attempt_utc).num_minutes();
                    if elapsed < backoff {
                        return false;
                    }
                }
            }
        }

        // 4 & 5. Rapid restart protection & elapsed interval check
        if let Some(ref success_str) = state.last_successful_at {
            if let Ok(last_success) = DateTime::parse_from_rfc3339(success_str) {
                let last_success_utc = last_success.with_timezone(&Utc);
                if now < last_success_utc {
                    // Clock skew protection: current time is before recorded success
                    return false;
                }

                let elapsed_minutes = (now - last_success_utc).num_minutes();
                return elapsed_minutes >= (settings.interval_minutes as i64);
            }
        }

        // 6. No prior successful backup
        if is_startup {
            settings.backup_on_startup
        } else if let Some(ref attempt_str) = state.last_attempt_at {
            if let Ok(last_attempt) = DateTime::parse_from_rfc3339(attempt_str) {
                let last_attempt_utc = last_attempt.with_timezone(&Utc);
                let elapsed = (now - last_attempt_utc).num_minutes();
                elapsed >= (settings.interval_minutes as i64)
            } else {
                true
            }
        } else {
            true
        }
    }

    /// Evaluates scheduling state and executes an automatic backup if due.
    ///
    /// Guarantees:
    /// - Thread-safe concurrency: never runs duplicate or simultaneous backup jobs.
    /// - Restore safety: aborts cleanly if a restore operation is currently active.
    /// - Non-blocking: skips cleanly without error if conditions are not met.
    /// - State tracking: updates `last_attempt_at`, `last_successful_at`, and `failure_count`.
    pub fn check_and_run_backup<F>(
        conn: &Connection,
        paths: &StoragePaths,
        is_startup: bool,
        progress_cb: Option<F>,
    ) -> Result<Option<BackupCreateResult>, BackupError>
    where
        F: FnMut(BackupProgressEvent),
    {
        // 1. Quick exit if restore or backup already running
        if Self::is_restore_in_progress() || Self::is_backup_in_progress() {
            return Ok(None);
        }

        // 2. Load settings and scheduler state
        let settings = BackupSettingsService::get_settings(conn)?;
        let mut state = Self::get_state(conn)?;
        let now = Utc::now();

        // 3. Determine if backup is due
        if !Self::is_backup_due(&settings, &state, now, is_startup) {
            return Ok(None);
        }

        // 4. Acquire exclusive execution lock
        let guard = match Self::try_acquire_backup_lock() {
            Ok(g) => g,
            Err(_) => return Ok(None),
        };

        // 5. Record attempt timestamp
        state.last_attempt_at = Some(now.to_rfc3339());
        let _ = Self::save_state(conn, &state);

        // 6. Formulate automatic backup request
        let request = BackupCreateRequest {
            destination_path: settings.backup_destination_dir.clone(),
            backup_type: Some(BackupType::Automatic),
            allow_overwrite: Some(false),
        };

        // 7. Perform backup creation and verification
        match BackupService::create_backup(conn, paths, request, progress_cb) {
            Ok(result) => {
                state.last_successful_at = Some(now.to_rfc3339());
                state.failure_count = 0;
                state.last_error_message = None;
                state.last_failure_category = None;
                state.last_retention_warning = result.warning_message.clone();
                let _ = Self::save_state(conn, &state);
                drop(guard);
                Ok(Some(result))
            }
            Err(err) => {
                state.failure_count += 1;
                state.last_error_message = Some(err.user_friendly_message());
                state.last_failure_category = Some(err.category().to_string());
                let _ = Self::save_state(conn, &state);
                drop(guard);
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use chrono::Duration;

    #[test]
    fn test_scheduler_state_defaults_and_roundtrip_persistence() {
        let mut conn = init_connection(std::path::Path::new(":memory:")).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Initial state is default
        let initial = BackupSchedulerService::get_state(&conn).unwrap();
        assert_eq!(initial, BackupSchedulerState::default());
        assert_eq!(initial.failure_count, 0);
        assert_eq!(initial.last_successful_at, None);

        // 2. Save state
        let state = BackupSchedulerState {
            last_attempt_at: Some("2026-10-13T10:00:00Z".to_string()),
            last_successful_at: Some("2026-10-13T10:05:00Z".to_string()),
            last_error_message: None,
            last_failure_category: None,
            last_retention_warning: None,
            failure_count: 0,
        };
        BackupSchedulerService::save_state(&conn, &state).unwrap();

        // 3. Reload and verify equality
        let loaded = BackupSchedulerService::get_state(&conn).unwrap();
        assert_eq!(loaded, state);
    }

    #[test]
    fn test_check_and_run_backup_lifecycle() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_sched_run_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Initial run on startup (backup_on_startup: true) -> runs backup
        let result = BackupSchedulerService::check_and_run_backup(
            &conn,
            &paths,
            true,
            None::<fn(BackupProgressEvent)>,
        )
        .unwrap();
        assert!(result.is_some());
        let backup_info = result.unwrap();
        assert!(backup_info.verified);
        assert_eq!(backup_info.backup_type, BackupType::Automatic);

        // Verify state was recorded
        let state = BackupSchedulerService::get_state(&conn).unwrap();
        assert!(state.last_successful_at.is_some());
        assert_eq!(state.failure_count, 0);

        // 2. Second rapid run on startup immediately after -> skipped (rapid restart protection!)
        let result2 = BackupSchedulerService::check_and_run_backup(
            &conn,
            &paths,
            true,
            None::<fn(BackupProgressEvent)>,
        )
        .unwrap();
        assert!(result2.is_none());

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_is_backup_due_user_disabled_invariant() {
        let settings = BackupSettings {
            auto_backup_enabled: false,
            ..BackupSettings::default()
        };
        let state = BackupSchedulerState::default();
        let now = Utc::now();

        assert!(!BackupSchedulerService::is_backup_due(
            &settings, &state, now, true
        ));
        assert!(!BackupSchedulerService::is_backup_due(
            &settings, &state, now, false
        ));
    }

    #[test]
    fn test_is_backup_due_first_run_startup_behavior() {
        let mut settings = BackupSettings {
            auto_backup_enabled: true,
            backup_on_startup: true,
            ..BackupSettings::default()
        };
        let state = BackupSchedulerState::default();
        let now = Utc::now();

        // When backup_on_startup is true and no backup exists, it is due on startup
        assert!(BackupSchedulerService::is_backup_due(
            &settings, &state, now, true
        ));

        // When backup_on_startup is false and no backup exists, it is NOT due on startup
        settings.backup_on_startup = false;
        assert!(!BackupSchedulerService::is_backup_due(
            &settings, &state, now, true
        ));
    }

    #[test]
    fn test_rapid_restart_protection_prevents_duplicate_backups() {
        let settings = BackupSettings {
            auto_backup_enabled: true,
            interval_minutes: 1440, // 24 hours
            backup_on_startup: true,
            ..Default::default()
        };

        let now = Utc::now();
        // A successful backup finished 10 minutes ago
        let ten_minutes_ago = now - Duration::minutes(10);
        let state = BackupSchedulerState {
            last_successful_at: Some(ten_minutes_ago.to_rfc3339()),
            last_attempt_at: Some(ten_minutes_ago.to_rfc3339()),
            last_error_message: None,
            failure_count: 0,
            ..Default::default()
        };

        // On startup 10 minutes later, backup is NOT due despite backup_on_startup: true!
        assert!(!BackupSchedulerService::is_backup_due(
            &settings, &state, now, true
        ));

        // Routine check 10 minutes later is also NOT due
        assert!(!BackupSchedulerService::is_backup_due(
            &settings, &state, now, false
        ));
    }

    #[test]
    fn test_backup_due_after_interval_elapsed() {
        let settings = BackupSettings {
            auto_backup_enabled: true,
            interval_minutes: 60, // 1 hour
            backup_on_startup: true,
            ..Default::default()
        };

        let now = Utc::now();
        // Last backup was 65 minutes ago
        let past = now - Duration::minutes(65);
        let state = BackupSchedulerState {
            last_successful_at: Some(past.to_rfc3339()),
            last_attempt_at: Some(past.to_rfc3339()),
            last_error_message: None,
            failure_count: 0,
            ..Default::default()
        };

        // Both startup and routine checks recognize interval has elapsed
        assert!(BackupSchedulerService::is_backup_due(
            &settings, &state, now, true
        ));
        assert!(BackupSchedulerService::is_backup_due(
            &settings, &state, now, false
        ));
    }

    #[test]
    fn test_failure_backoff_and_max_failure_cap() {
        let settings = BackupSettings {
            auto_backup_enabled: true,
            interval_minutes: 60,
            backup_on_startup: true,
            ..Default::default()
        };

        let now = Utc::now();
        let past = now - Duration::minutes(120);

        // 1. Single failure 5 minutes ago: within 15-minute backoff window -> NOT due
        let state_backoff = BackupSchedulerState {
            last_successful_at: Some(past.to_rfc3339()),
            last_attempt_at: Some((now - Duration::minutes(5)).to_rfc3339()),
            last_error_message: Some("Disk full".to_string()),
            failure_count: 1,
            ..Default::default()
        };
        assert!(!BackupSchedulerService::is_backup_due(
            &settings,
            &state_backoff,
            now,
            false
        ));

        // 2. Single failure 20 minutes ago: exceeded 15-minute backoff window -> retry is due
        let state_retriable = BackupSchedulerState {
            last_successful_at: Some(past.to_rfc3339()),
            last_attempt_at: Some((now - Duration::minutes(20)).to_rfc3339()),
            last_error_message: Some("Disk full".to_string()),
            failure_count: 1,
            ..Default::default()
        };
        assert!(BackupSchedulerService::is_backup_due(
            &settings,
            &state_retriable,
            now,
            false
        ));

        // 3. Max consecutive failures reached (5) -> paused, NOT due
        let state_max_failures = BackupSchedulerState {
            last_successful_at: Some(past.to_rfc3339()),
            last_attempt_at: Some((now - Duration::minutes(200)).to_rfc3339()),
            last_error_message: Some("Persistent IO error".to_string()),
            failure_count: MAX_CONSECUTIVE_FAILURES,
            ..Default::default()
        };
        assert!(!BackupSchedulerService::is_backup_due(
            &settings,
            &state_max_failures,
            now,
            false
        ));
    }

    #[test]
    fn test_concurrency_lock_and_raii_release() {
        assert!(!BackupSchedulerService::is_backup_in_progress());

        // 1. Acquire lock
        let guard1 = BackupSchedulerService::try_acquire_backup_lock().unwrap();
        assert!(BackupSchedulerService::is_backup_in_progress());

        // 2. Second attempt while locked must fail
        let guard2_err = BackupSchedulerService::try_acquire_backup_lock().unwrap_err();
        match guard2_err {
            BackupError::ConcurrentOperation(_) => {}
            _ => panic!("Expected ConcurrentOperation error"),
        }

        // 3. Drop guard releases the lock cleanly
        drop(guard1);
        assert!(!BackupSchedulerService::is_backup_in_progress());

        // 4. Can acquire again after drop
        let _guard3 = BackupSchedulerService::try_acquire_backup_lock().unwrap();
        assert!(BackupSchedulerService::is_backup_in_progress());
    }

    #[test]
    fn test_restore_in_progress_blocks_backup() {
        BackupSchedulerService::set_restore_in_progress(true);
        assert!(BackupSchedulerService::is_restore_in_progress());

        // Backup acquisition must fail while restore is running
        let err = BackupSchedulerService::try_acquire_backup_lock().unwrap_err();
        match err {
            BackupError::ConcurrentOperation(msg) => {
                assert!(msg.contains("restore"));
            }
            _ => panic!("Expected ConcurrentOperation error due to restore"),
        }

        BackupSchedulerService::set_restore_in_progress(false);
        assert!(!BackupSchedulerService::is_restore_in_progress());
    }
}
