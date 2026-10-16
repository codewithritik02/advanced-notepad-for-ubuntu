use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use rusqlite::backup::Backup;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::storage::errors::BackupError;
use crate::storage::models::BackupDatabaseInfo;

/// Computes the SHA-256 checksum and exact byte size of a file on disk using buffered streaming.
pub fn compute_file_sha256(path: &Path) -> Result<(String, u64), BackupError> {
    let file = File::open(path).map_err(BackupError::Io)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536]; // 64 KiB buffer
    let mut total_bytes = 0u64;

    loop {
        let bytes_read = reader.read(&mut buffer).map_err(BackupError::Io)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        total_bytes += bytes_read as u64;
    }

    let hash_hex = format!("{:x}", hasher.finalize());
    Ok((hash_hex, total_bytes))
}

/// Service responsible for creating and validating transactionally consistent SQLite snapshots (Phase 11, Task 5).
pub struct SnapshotManager;

impl SnapshotManager {
    /// Creates a consistent database snapshot using SQLite's native Online Backup API.
    ///
    /// Critical Safety Rules:
    /// 1. Never copies the live SQLite database file blindly via filesystem copy (avoids WAL truncation/dirty reads).
    /// 2. Uses `rusqlite::backup::Backup` to safely synchronize all committed WAL and database pages.
    /// 3. Writes initially to a temporary file (`<dest>.tmp.<uuid>`) and renames atomically upon completion.
    /// 4. Opens the resulting snapshot independently to verify `PRAGMA integrity_check` and schema sanity.
    /// 5. Computes SHA-256 cryptographic checksum and counts metadata.
    pub fn create_snapshot(
        src_conn: &Connection,
        dest_path: &Path,
    ) -> Result<BackupDatabaseInfo, BackupError> {
        if let Some(parent) = dest_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(BackupError::Io)?;
            }
        }

        // 1. Prepare temporary snapshot destination file
        let temp_filename = format!(
            "{}.tmp.{}",
            dest_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "snapshot.sqlite".to_string()),
            Uuid::new_v4()
        );
        let temp_path = dest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&temp_filename);

        // Ensure temp file does not already exist
        if temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
        }

        struct SnapshotTempGuard<'a> {
            path: &'a Path,
            committed: bool,
        }

        impl<'a> Drop for SnapshotTempGuard<'a> {
            fn drop(&mut self) {
                if !self.committed && self.path.exists() {
                    let _ = fs::remove_file(self.path);
                }
            }
        }

        let mut guard = SnapshotTempGuard {
            path: &temp_path,
            committed: false,
        };

        // 2. Perform online backup via SQLite Backup API inside an enclosed block to drop dest_conn
        {
            let mut dest_conn = Connection::open(&temp_path).map_err(BackupError::Database)?;

            dest_conn
                .execute_batch(
                    "PRAGMA busy_timeout = 5000;
                     PRAGMA synchronous = NORMAL;",
                )
                .map_err(BackupError::Database)?;

            let backup = Backup::new(src_conn, &mut dest_conn).map_err(BackupError::Database)?;

            // Run backup to completion in 100-page increments, sleeping 50ms if busy
            backup
                .run_to_completion(100, std::time::Duration::from_millis(50), None)
                .map_err(BackupError::Database)?;

            // dest_conn drops here, closing and flushing all locks
        }

        // 3. Atomically rename temporary snapshot to final destination path
        if dest_path.exists() {
            fs::remove_file(dest_path).map_err(BackupError::Io)?;
        }
        match fs::rename(&temp_path, dest_path) {
            Ok(_) => {
                guard.committed = true;
            }
            Err(rename_err) => {
                match fs::copy(&temp_path, dest_path) {
                    Ok(_) => {
                        let _ = fs::remove_file(&temp_path);
                        guard.committed = true;
                    }
                    Err(copy_err) => {
                        if dest_path.exists() {
                            let _ = fs::remove_file(dest_path);
                        }
                        if let Some(28) = copy_err.raw_os_error() {
                            return Err(BackupError::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "Insufficient disk space to complete database snapshot.",
                            )));
                        } else {
                            return Err(BackupError::Io(rename_err));
                        }
                    }
                }
            }
        }

        // 4. Validate the newly created snapshot independently
        Self::verify_snapshot_database(dest_path)
    }

    /// Independently opens a snapshot database, runs integrity checks, verifies schema version,
    /// counts notes/trash/notebooks/tags, and computes its SHA-256 hash.
    pub fn verify_snapshot_database(snapshot_path: &Path) -> Result<BackupDatabaseInfo, BackupError> {
        if !snapshot_path.is_file() {
            return Err(BackupError::ValidationFailed(format!(
                "Snapshot file does not exist at '{}'",
                snapshot_path.display()
            )));
        }

        // 1. Open snapshot independently
        let conn = Connection::open(snapshot_path).map_err(BackupError::Database)?;

        // 2. Run PRAGMA integrity_check
        let integrity_res: String = conn
            .query_row("PRAGMA integrity_check;", [], |row| row.get(0))
            .map_err(BackupError::Database)?;

        if integrity_res.to_lowercase() != "ok" {
            return Err(BackupError::ValidationFailed(format!(
                "Snapshot SQLite integrity check failed: '{integrity_res}'"
            )));
        }

        // 3. Verify required tables exist
        let tables_count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name IN ('notes', 'notebooks', 'tags', 'attachments', 'settings', '_migrations')",
                [],
                |row| row.get(0),
            )
            .map_err(BackupError::Database)?;

        if tables_count < 6 {
            return Err(BackupError::ValidationFailed(
                "Snapshot is missing required database tables.".to_string(),
            ));
        }

        // 4. Query highest schema migration version
        let schema_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // 5. Query counts
        let note_count: usize = conn
            .query_row(
                "SELECT COUNT(1) FROM notes WHERE is_deleted = 0",
                [],
                |row| row.get::<_, i64>(0).map(|c| c as usize),
            )
            .unwrap_or(0);

        let trash_count: usize = conn
            .query_row(
                "SELECT COUNT(1) FROM notes WHERE is_deleted = 1",
                [],
                |row| row.get::<_, i64>(0).map(|c| c as usize),
            )
            .unwrap_or(0);

        let notebook_count: usize = conn
            .query_row(
                "SELECT COUNT(1) FROM notebooks",
                [],
                |row| row.get::<_, i64>(0).map(|c| c as usize),
            )
            .unwrap_or(0);

        let tag_count: usize = conn
            .query_row(
                "SELECT COUNT(1) FROM tags",
                [],
                |row| row.get::<_, i64>(0).map(|c| c as usize),
            )
            .unwrap_or(0);

        // Drop connection before hashing to ensure no active SQLite reader handles
        drop(conn);

        // 6. Compute SHA-256 checksum and exact file size
        let (sha256, file_size) = compute_file_sha256(snapshot_path)?;

        Ok(BackupDatabaseInfo {
            entry_path: "database.sqlite".to_string(),
            file_size,
            sha256,
            schema_version,
            note_count,
            notebook_count,
            tag_count,
            trash_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;

    #[test]
    fn test_task_5_consistent_sqlite_snapshot_creation_and_wal_preservation() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_snap_{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("Create temp dir");
        let live_db_path = temp_dir.join("live_database.sqlite");
        let snapshot_path = temp_dir.join("backup_database.sqlite");

        // 1. Initialize live database and run all migrations (WAL mode enabled)
        let mut live_conn = init_connection(&live_db_path).expect("Init connection");
        run_migrations(&mut live_conn).expect("Run migrations");

        // Verify live DB is in WAL mode
        let journal_mode: String = live_conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .expect("Get journal mode");
        assert_eq!(journal_mode.to_lowercase(), "wal");

        // 2. Insert test notebooks, notes, and tags into live database
        let now = chrono::Utc::now().to_rfc3339();
        let nb_id = Uuid::new_v4().to_string();
        live_conn
            .execute(
                "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at) VALUES (?1, 'Work', NULL, ?2, ?2)",
                rusqlite::params![nb_id, now],
            )
            .expect("Insert notebook");

        let active_note_id = Uuid::new_v4().to_string();
        live_conn
            .execute(
                "INSERT INTO notes (id, title, content, format, notebook_id, created_at, modified_at, is_favorite, is_pinned, is_deleted)
                 VALUES (?1, 'Active Note Title', 'Content of active note', 'txt', ?2, ?3, ?3, 1, 0, 0)",
                rusqlite::params![active_note_id, nb_id, now],
            )
            .expect("Insert active note");

        let trashed_note_id = Uuid::new_v4().to_string();
        live_conn
            .execute(
                "INSERT INTO notes (id, title, content, format, notebook_id, created_at, modified_at, is_favorite, is_pinned, is_deleted, deleted_at)
                 VALUES (?1, 'Trashed Note', 'Content in trash', 'txt', NULL, ?2, ?2, 0, 0, 1, ?2)",
                rusqlite::params![trashed_note_id, now],
            )
            .expect("Insert trashed note");

        let tag_id = Uuid::new_v4().to_string();
        live_conn
            .execute(
                "INSERT INTO tags (id, name, created_at) VALUES (?1, 'urgent', ?2)",
                rusqlite::params![tag_id, now],
            )
            .expect("Insert tag");

        // 3. Create consistent snapshot via SnapshotManager
        let info = SnapshotManager::create_snapshot(&live_conn, &snapshot_path)
            .expect("Snapshot creation must succeed");

        // 4. Verify snapshot properties
        assert!(snapshot_path.is_file(), "Snapshot file must exist on disk");
        assert_eq!(info.entry_path, "database.sqlite");
        assert_eq!(info.note_count, 1, "Must contain 1 active note");
        assert_eq!(info.trash_count, 1, "Must contain 1 trashed note");
        assert_eq!(info.notebook_count, 1, "Must contain 1 notebook");
        assert_eq!(info.tag_count, 1, "Must contain 1 tag");
        assert_eq!(info.schema_version, 4, "Must match current schema version 4");
        assert!(info.file_size > 0, "File size must be positive");
        assert!(!info.sha256.is_empty(), "SHA-256 hash must not be empty");

        // 5. Verify the snapshot can be opened and queried independently
        let snap_conn = Connection::open(&snapshot_path).expect("Open snapshot connection");
        let active_title: String = snap_conn
            .query_row(
                "SELECT title FROM notes WHERE id = ?1",
                rusqlite::params![active_note_id],
                |r| r.get(0),
            )
            .expect("Query active note from snapshot");
        assert_eq!(active_title, "Active Note Title");

        let snap_integrity: String = snap_conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .expect("PRAGMA integrity_check");
        assert_eq!(snap_integrity.to_lowercase(), "ok");

        drop(snap_conn);

        // 6. Verify that creating a snapshot did NOT alter or close the live database
        let live_title: String = live_conn
            .query_row(
                "SELECT title FROM notes WHERE id = ?1",
                rusqlite::params![active_note_id],
                |r| r.get(0),
            )
            .expect("Live DB must still be completely functional");
        assert_eq!(live_title, "Active Note Title");

        // Clean up
        drop(live_conn);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
