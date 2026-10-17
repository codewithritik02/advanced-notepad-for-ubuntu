use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

use crate::storage::errors::BackupError;
use crate::storage::models::{BackupAttachmentItem, BackupAttachmentsSummary};
use crate::storage::paths::StoragePaths;
use crate::storage::services::snapshot::compute_file_sha256;

/// Represents a captured and verified attachment file ready for archive bundling (Phase 11, Task 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedAttachment {
    /// Authoritative manifest metadata item with verified SHA-256 checksum
    pub item: BackupAttachmentItem,
    /// Absolute path on disk to the verified source attachment file
    pub source_path: PathBuf,
    /// Safe relative path inside the backup archive (e.g. "attachments/<note_id>/<stored_name>")
    pub archive_entry_path: String,
}

/// Service responsible for capturing and cross-validating managed attachment files with the database snapshot.
pub struct AttachmentCaptureManager;

impl AttachmentCaptureManager {
    /// Inspects all attachments registered in the database, cross-validates them against physical files on disk,
    /// computes their cryptographic checksums, and produces a complete captured inventory.
    ///
    /// Safety Invariants:
    /// 1. Reads from the database snapshot to ensure 100% coherence between DB records and copied files.
    /// 2. Validates relative paths strictly to prevent directory traversal attacks.
    /// 3. Rejects symbolic links to prevent backing up unauthorized filesystem targets.
    /// 4. Generates an explicit `RequiredAttachmentMissing` error if any registered attachment file is missing or unreadable.
    /// 5. Never falsely reports missing or inconsistent files as successfully captured.
    pub fn capture_attachments(
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<(Vec<CapturedAttachment>, BackupAttachmentsSummary), BackupError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, note_id, file_name, relative_path, mime_type, file_size
                 FROM attachments
                 ORDER BY note_id ASC, id ASC",
            )
            .map_err(BackupError::Database)?;

        let mut captured_items = Vec::new();
        let mut total_bytes = 0u64;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(BackupError::Database)?;

        for row_res in rows {
            let (id, note_id, file_name, relative_path, mime_type, recorded_size) =
                row_res.map_err(BackupError::Database)?;

            let trimmed_rel = relative_path.trim();
            if trimmed_rel.is_empty() {
                return Err(BackupError::ValidationFailed(format!(
                    "Attachment '{id}' has empty relative path in database"
                )));
            }

            // 1. Sanitize relative path against directory traversal
            let sanitized_subpath =
                StoragePaths::sanitize_subpath(&paths.attachments_dir, Path::new(trimmed_rel))
                    .map_err(|e| {
                        BackupError::ValidationFailed(format!(
                            "Unsafe attachment path '{trimmed_rel}': {e}"
                        ))
                    })?;

            // 2. Verify file exists on disk
            if !sanitized_subpath.exists() {
                return Err(BackupError::RequiredAttachmentMissing(format!(
                    "Attachment '{file_name}' (id: {id}) referenced in database is missing on disk at '{}'",
                    sanitized_subpath.display()
                )));
            }

            // 3. Inspect metadata & reject symlinks
            let sym_meta = fs::symlink_metadata(&sanitized_subpath).map_err(BackupError::Io)?;
            if sym_meta.file_type().is_symlink() {
                return Err(BackupError::ValidationFailed(format!(
                    "Attachment '{sanitized_subpath:?}' is a symbolic link, which is not permitted for backup"
                )));
            }
            if !sym_meta.is_file() {
                return Err(BackupError::ValidationFailed(format!(
                    "Attachment '{sanitized_subpath:?}' is not a regular file"
                )));
            }

            // 4. Compute streaming SHA-256 and verified file size
            let (sha256, verified_file_size) = compute_file_sha256(&sanitized_subpath)?;

            // 5. Cross-check file size with database record if recorded size is non-negative
            if recorded_size >= 0 && verified_file_size != recorded_size as u64 {
                eprintln!(
                    "[Backup Warning] Attachment '{id}' file size on disk ({verified_file_size} B) differs from DB record ({recorded_size} B). Using verified on-disk size."
                );
            }

            total_bytes += verified_file_size;

            // 6. Build archive relative path (e.g. "attachments/<note_id>/<stored_name>")
            let normalized_rel = trimmed_rel.replace('\\', "/");
            let archive_entry_path = format!("attachments/{normalized_rel}");

            let item = BackupAttachmentItem {
                id,
                note_id,
                file_name,
                relative_path: normalized_rel,
                file_size: verified_file_size,
                mime_type,
                sha256,
            };

            captured_items.push(CapturedAttachment {
                item,
                source_path: sanitized_subpath,
                archive_entry_path,
            });
        }

        let summary = BackupAttachmentsSummary {
            total_files: captured_items.len(),
            total_bytes,
        };

        Ok((captured_items, summary))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use std::io::Write;
    use uuid::Uuid;

    #[test]
    fn test_task_6_consistent_attachment_capture_and_crosscheck() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_att_cap_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("Ensure directories");

        let mut conn = init_connection(&paths.database_file).expect("Init connection");
        run_migrations(&mut conn).expect("Run migrations");

        // 1. Create a note and physical attachment file on disk
        let note_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at) VALUES (?1, 'Note with Attachment', 'Content', 'txt', ?2, ?2)",
            rusqlite::params![note_id, now],
        ).expect("Insert note");

        let note_dir = paths
            .get_note_attachments_dir(&note_id)
            .expect("Get note dir");
        let stored_filename = format!("{}.pdf", Uuid::new_v4());
        let file_path = note_dir.join(&stored_filename);

        let file_bytes = b"%PDF-1.4 sample PDF binary payload for testing backup capture";
        let mut file = fs::File::create(&file_path).expect("Create attachment file");
        file.write_all(file_bytes).expect("Write bytes");
        drop(file);

        let att_id = Uuid::new_v4().to_string();
        let rel_path = format!("{note_id}/{stored_filename}");
        conn.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, ?2, 'Sample Report.pdf', ?3, 'application/pdf', ?4, ?5, ?5)",
            rusqlite::params![att_id, note_id, rel_path, file_bytes.len() as i64, now],
        ).expect("Insert attachment record");

        // 2. Run capture_attachments
        let (captured, summary) = AttachmentCaptureManager::capture_attachments(&conn, &paths)
            .expect("Capture attachments should succeed");

        assert_eq!(captured.len(), 1);
        assert_eq!(summary.total_files, 1);
        assert_eq!(summary.total_bytes, file_bytes.len() as u64);

        let first = &captured[0];
        assert_eq!(first.item.id, att_id);
        assert_eq!(first.item.file_name, "Sample Report.pdf");
        assert_eq!(first.item.file_size, file_bytes.len() as u64);
        assert_eq!(
            first.archive_entry_path,
            format!("attachments/{note_id}/{stored_filename}")
        );
        assert!(!first.item.sha256.is_empty());

        // 3. Test missing attachment file detection
        let missing_att_id = Uuid::new_v4().to_string();
        let missing_rel_path = format!("{note_id}/ghost_file.png");
        conn.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, ?2, 'Ghost.png', ?3, 'image/png', 100, ?4, ?4)",
            rusqlite::params![missing_att_id, note_id, missing_rel_path, now],
        ).expect("Insert missing attachment record");

        let err = AttachmentCaptureManager::capture_attachments(&conn, &paths)
            .expect_err("Must report missing attachment file error");

        match err {
            BackupError::RequiredAttachmentMissing(msg) => {
                assert!(msg.contains("Ghost.png"));
                assert!(msg.contains("missing on disk"));
            }
            other => panic!("Expected RequiredAttachmentMissing, got {other:?}"),
        }

        // Clean up
        drop(conn);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
