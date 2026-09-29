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
        std::fs::create_dir_all(&self.config_dir)?;
        Ok(())
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
    /// preventing directory traversal attacks (`../`).
    pub fn sanitize_subpath(base: &Path, user_relative_path: &Path) -> Result<PathBuf, StorageError> {
        for component in user_relative_path.components() {
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
}
