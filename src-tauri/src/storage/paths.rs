use std::path::{Path, PathBuf};
use crate::storage::errors::StorageError;

/// Centralized management of application storage directories and paths
#[derive(Debug, Clone)]
pub struct StoragePaths {
    /// Root user-data directory for Personal Notepad
    pub root: PathBuf,
    /// Absolute path to the SQLite database file
    pub database_file: PathBuf,
    /// Directory for file attachments
    pub attachments_dir: PathBuf,
    /// Directory reserved for future database backups
    pub backups_dir: PathBuf,
    /// Directory reserved for future application configuration
    pub config_dir: PathBuf,
}

impl StoragePaths {
    /// Constructs a `StoragePaths` instance derived from a given root data directory.
    pub fn from_root(root: PathBuf) -> Self {
        let database_file = root.join("database.sqlite");
        let attachments_dir = root.join("attachments");
        let backups_dir = root.join("backups");
        let config_dir = root.join("config");

        Self {
            root,
            database_file,
            attachments_dir,
            backups_dir,
            config_dir,
        }
    }

    /// Idempotently creates all required application directories.
    /// Safe to call repeatedly without error.
    pub fn ensure_directories(&self) -> Result<(), StorageError> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(&self.attachments_dir)?;
        std::fs::create_dir_all(&self.backups_dir)?;
        std::fs::create_dir_all(self.checkpoints_dir())?;
        std::fs::create_dir_all(self.logs_dir())?;
        std::fs::create_dir_all(&self.config_dir)?;
        let decrypted_dir = self.decrypted_attachments_dir();
        if decrypted_dir.exists() {
            let _ = std::fs::remove_dir_all(&decrypted_dir);
        }
        std::fs::create_dir_all(&decrypted_dir)?;
        Ok(())
    }

    /// Dedicated directory for security audit logs and diagnostics (Phase 12, Task 22).
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Path to local security audit log file (strictly privacy-preserving, zero-telemetry).
    pub fn security_log_file(&self) -> PathBuf {
        self.logs_dir().join("security_audit.log")
    }

    /// Dedicated directory for temporary decrypted attachment files viewed during active unlocked sessions.
    pub fn decrypted_attachments_dir(&self) -> PathBuf {
        self.root.join("temp").join("decrypted_attachments")
    }

    /// Dedicated directory for pre-restore recovery checkpoints (Phase 11, Task 16).
    pub fn checkpoints_dir(&self) -> PathBuf {
        self.backups_dir.join("checkpoints")
    }

    /// Path to marker file tracking in-progress multi-component restore activation (Phase 11, Task 19).
    pub fn restore_marker_file(&self) -> PathBuf {
        self.root.join(".restore_in_progress.json")
    }

    /// Path to marker file tracking in-progress encryption migration or state transition (Phase 12, Task 18).
    pub fn encryption_migration_marker_file(&self) -> PathBuf {
        self.root.join(".encryption_migration_marker.json")
    }

    /// Resolves and ensures the attachments directory for a specific note (`attachments/<note_id>/`).
    /// Strictly validates that `note_id` is a safe identifier without path separators or traversal sequences.
    pub fn get_note_attachments_dir(&self, note_id: &str) -> Result<PathBuf, StorageError> {
        let trimmed = note_id.trim();
        if trimmed.is_empty()
            || trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.contains("..")
        {
            return Err(StorageError::Validation(format!(
                "Invalid note identifier for attachments path: '{note_id}'"
            )));
        }

        let dir = self.attachments_dir.join(trimmed);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Resolves a backup file path in the backups directory (`backups/backup-<tag>.sqlite`).
    pub fn get_backup_file_path(&self, tag: &str) -> Result<PathBuf, StorageError> {
        let trimmed = tag.trim();
        if trimmed.is_empty()
            || trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.contains("..")
        {
            return Err(StorageError::Validation(format!(
                "Invalid tag identifier for backup path: '{tag}'"
            )));
        }

        let file_name = format!("backup-{trimmed}.sqlite");
        Ok(self.backups_dir.join(file_name))
    }

    /// Validates that a requested subpath resides strictly within the specified base directory,
    /// preventing directory traversal attacks (`../` or `..\`).
    pub fn sanitize_subpath(base: &Path, user_relative_path: &Path) -> Result<PathBuf, StorageError> {
        let path_str = user_relative_path.to_string_lossy();

        // Check for Windows drive letter prefix (e.g., C:) regardless of host OS
        if path_str.len() >= 2 && path_str.as_bytes()[1] == b':' && path_str.as_bytes()[0].is_ascii_alphabetic() {
            return Err(StorageError::Validation(
                "Absolute paths or drive prefixes are not permitted".to_string(),
            ));
        }

        // Normalize Windows backslashes so traversal inspection is robust across both POSIX and Windows
        let normalized = path_str.replace('\\', "/");
        let normalized_path = Path::new(&normalized);

        for component in normalized_path.components() {
            match component {
                std::path::Component::ParentDir => {
                    return Err(StorageError::Validation(
                        "Path traversal (ParentDir '..') is not permitted".to_string(),
                    ));
                }
                std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                    return Err(StorageError::Validation(
                        "Absolute paths or drive prefixes are not permitted".to_string(),
                    ));
                }
                std::path::Component::Normal(_) | std::path::Component::CurDir => {}
            }
        }

        let full_path = base.join(user_relative_path);
        Ok(full_path)
    }

    /// Dedicated lock file path to detect and block concurrent application instances (Phase 13, Task 19).
    pub fn app_instance_lock_file(&self) -> PathBuf {
        self.root.join(".app_instance.pid")
    }

    /// Cleans up orphan temporary files and abandoned staging directories left behind by crashed or interrupted runs (Phase 13, Task 19).
    pub fn cleanup_orphan_temp_files(&self) -> Result<usize, StorageError> {
        let mut count = 0;

        // 1. Root directory: clean .restore_staging_* dirs and *.tmp.* files
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();
                if path.is_dir() && file_name.starts_with(".restore_staging_") {
                    let _ = std::fs::remove_dir_all(&path);
                    count += 1;
                } else if path.is_file() && (file_name.contains(".tmp.") || file_name.ends_with(".tmp")) {
                    let _ = std::fs::remove_file(&path);
                    count += 1;
                }
            }
        }

        // 2. Backups directory: clean *.tmp.* files and staging dirs
        if let Ok(entries) = std::fs::read_dir(&self.backups_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();
                if path.is_dir() && (file_name.starts_with(".backup_staging_") || file_name.starts_with(".tmp.")) {
                    let _ = std::fs::remove_dir_all(&path);
                    count += 1;
                } else if path.is_file() && (file_name.contains(".tmp.") || file_name.ends_with(".tmp")) {
                    let _ = std::fs::remove_file(&path);
                    count += 1;
                }
            }
        }

        // 3. Attachments directory: clean *.tmp.* files across all note folders
        if let Ok(note_dirs) = std::fs::read_dir(&self.attachments_dir) {
            for note_entry in note_dirs.flatten() {
                let note_path = note_entry.path();
                if note_path.is_dir() {
                    if let Ok(files) = std::fs::read_dir(&note_path) {
                        for file_entry in files.flatten() {
                            let fpath = file_entry.path();
                            let fname = file_entry.file_name().to_string_lossy().to_string();
                            if fpath.is_file() && (fname.contains(".tmp.") || fname.ends_with(".tmp")) {
                                let _ = std::fs::remove_file(&fpath);
                                count += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(count)
    }
}

/// Structure managing application single-instance locking via PID tracking (Phase 13, Task 19).
#[derive(Debug)]
pub struct AppInstanceLock {
    pub lock_path: PathBuf,
}

impl AppInstanceLock {
    pub fn acquire(paths: &StoragePaths) -> Result<Self, StorageError> {
        let lock_path = paths.app_instance_lock_file();
        let current_pid = std::process::id();

        if lock_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&lock_path) {
                if let Ok(existing_pid) = content.trim().parse::<u32>() {
                    // Check if existing process is actually running on Linux via /proc
                    let is_running = std::path::Path::new(&format!("/proc/{existing_pid}")).exists();
                    if is_running && existing_pid != current_pid {
                        return Err(StorageError::Conflict(format!(
                            "Another instance of Personal Notepad is already running (PID {existing_pid}). Concurrent access blocked."
                        )));
                    }
                }
            }
        }

        std::fs::write(&lock_path, current_pid.to_string())?;
        Ok(Self { lock_path })
    }
}

impl Drop for AppInstanceLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_paths_creation_and_idempotence() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_paths_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());

        assert_eq!(paths.database_file, temp_dir.join("database.sqlite"));
        assert_eq!(paths.attachments_dir, temp_dir.join("attachments"));
        assert_eq!(paths.backups_dir, temp_dir.join("backups"));
        assert_eq!(paths.config_dir, temp_dir.join("config"));

        // 1. Initial creation
        paths.ensure_directories().expect("ensure_directories should succeed");
        assert!(paths.root.is_dir());
        assert!(paths.attachments_dir.is_dir());
        assert!(paths.backups_dir.is_dir());
        assert!(paths.checkpoints_dir().is_dir());
        assert!(paths.config_dir.is_dir());

        // 2. Idempotent re-run
        paths.ensure_directories().expect("ensure_directories should be idempotent");

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_note_attachments_dir_and_backup_paths() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_subpaths_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("ensure_directories failed");

        // 1. Safe note attachments directory creation
        let note_id = uuid::Uuid::new_v4().to_string();
        let note_att_dir = paths.get_note_attachments_dir(&note_id).expect("Should resolve note attachment dir");
        assert!(note_att_dir.is_dir());
        assert_eq!(note_att_dir, paths.attachments_dir.join(&note_id));

        // 2. Malicious note ID traversal rejection
        assert!(paths.get_note_attachments_dir("../../etc").is_err());
        assert!(paths.get_note_attachments_dir("note/sub").is_err());
        assert!(paths.get_note_attachments_dir("note\\sub").is_err());
        assert!(paths.get_note_attachments_dir("").is_err());

        // 3. Backup file path resolution
        let backup_path = paths.get_backup_file_path("20260929-223000").expect("Should resolve backup path");
        assert_eq!(backup_path, paths.backups_dir.join("backup-20260929-223000.sqlite"));

        // 4. Malicious backup tag traversal rejection
        assert!(paths.get_backup_file_path("../evil").is_err());
        assert!(paths.get_backup_file_path("/root").is_err());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_path_sanitization_and_traversal_prevention() {
        let base = PathBuf::from("/tmp/app_data/attachments");

        // Valid safe relative subpath
        let safe_subpath = Path::new("note-123/image.png");
        let resolved = StoragePaths::sanitize_subpath(&base, safe_subpath).expect("Safe path should resolve");
        assert_eq!(resolved, base.join("note-123/image.png"));

        // Rejection of parent directory traversal
        let dangerous_path1 = Path::new("../../etc/passwd");
        assert!(StoragePaths::sanitize_subpath(&base, dangerous_path1).is_err());

        let dangerous_path2 = Path::new("note-123/../../../shadow");
        assert!(StoragePaths::sanitize_subpath(&base, dangerous_path2).is_err());

        // Rejection of absolute paths
        let absolute_path = Path::new("/etc/hosts");
        assert!(StoragePaths::sanitize_subpath(&base, absolute_path).is_err());
    }

    #[test]
    fn test_cleanup_orphan_temp_files() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_orphan_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // Create orphan staging dir and files
        let staging_dir = paths.root.join(".restore_staging_12345");
        std::fs::create_dir_all(&staging_dir).unwrap();
        std::fs::write(staging_dir.join("temp.db"), b"staged").unwrap();

        let temp_db = paths.root.join("database.sqlite.tmp.uuid");
        std::fs::write(&temp_db, b"temp db").unwrap();

        let note_dir = paths.get_note_attachments_dir("n1").unwrap();
        let temp_att = note_dir.join("att.tmp.uuid");
        std::fs::write(&temp_att, b"temp att").unwrap();

        let cleaned = paths.cleanup_orphan_temp_files().unwrap();
        assert_eq!(cleaned, 3, "Must clean 3 orphan temporary entities");

        assert!(!staging_dir.exists());
        assert!(!temp_db.exists());
        assert!(!temp_att.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_app_instance_lock_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_lock_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        // 1. Acquire lock
        let lock = AppInstanceLock::acquire(&paths).expect("Acquire lock");
        assert!(paths.app_instance_lock_file().exists());

        // 2. Drop lock (clean exit)
        drop(lock);
        assert!(!paths.app_instance_lock_file().exists(), "Lock file must be removed on clean drop");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
