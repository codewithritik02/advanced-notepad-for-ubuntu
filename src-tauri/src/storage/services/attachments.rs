use std::path::{Path, PathBuf};
use rusqlite::Connection;

use crate::storage::attachment_storage::{AttachmentStorage, ManagedFileInfo};
use crate::storage::errors::AttachmentError;
use crate::storage::models::{
    Attachment, AttachmentConsistencyReport, CreateAttachmentDto, MissingManagedFileIssue,
    SizeMismatchIssue,
};
use crate::storage::repositories::AttachmentRepository;

/// High-level domain service that coordinates attachment lifecycle operations between
/// Tauri IPC commands, managed filesystem storage, and SQLite metadata.
///
/// Implements Phase 7, Task 7 requirements:
/// 1. Note validation
/// 2. Source-file validation
/// 3. Storage-name generation
/// 4. Managed file copying
/// 5. File-size and metadata verification
/// 6. Database insertion
/// 7. Error handling and cleanup (rollback on partial failure)
pub struct AttachmentService;

impl AttachmentService {
    /// Adds a file as an attachment to an existing active note.
    ///
    /// Pipeline:
    /// 1. Note validation: Ensures note exists and is not soft-deleted.
    /// 2. Storage operations: Inspects source file, validates bounds, generates collision-resistant
    ///    storage name, writes staged temporary copy, verifies written size, atomically renames.
    /// 3. Database insertion: Persists metadata linking attachment to note.
    /// 4. Rollback safety: If database insert fails, automatically removes the newly created
    ///    managed file to prevent storage leaks and orphans.
    pub fn add_attachment(
        conn: &Connection,
        storage: &AttachmentStorage,
        note_id: &str,
        source_path: &Path,
    ) -> Result<Attachment, AttachmentError> {
        let trimmed_note_id = note_id.trim();
        if trimmed_note_id.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Note ID cannot be empty".to_string(),
            ));
        }

        // 1. Note validation: Verify the note exists and is active
        AttachmentRepository::verify_note_eligible(conn, trimmed_note_id)?;

        // 2-5. Source validation & managed storage copy
        let stored = storage.store_file(trimmed_note_id, source_path)?;

        // 6. Database insertion
        let create_dto = CreateAttachmentDto {
            note_id: trimmed_note_id.to_string(),
            file_name: stored.original_name,
            relative_path: stored.relative_path.clone(),
            mime_type: stored.mime_type,
            file_size: stored.file_size as i64,
        };

        match AttachmentRepository::create(conn, create_dto) {
            Ok(attachment) => Ok(attachment),
            Err(db_err) => {
                // 7. Rollback safety: remove newly created managed file if DB insertion failed
                if let Err(cleanup_err) = storage.remove_file(&stored.relative_path) {
                    eprintln!(
                        "Warning: Failed to clean up managed attachment file '{}' after DB insert error: {}",
                        stored.relative_path, cleanup_err
                    );
                }
                Err(AttachmentError::from(db_err))
            }
        }
    }

    /// Lists attachments for a specific note, verifying the note exists.
    /// Returns results ordered deterministically by creation timestamp and ID.
    pub fn list_attachments(
        conn: &Connection,
        note_id: &str,
    ) -> Result<Vec<Attachment>, AttachmentError> {
        let trimmed_note_id = note_id.trim();
        if trimmed_note_id.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Note ID cannot be empty".to_string(),
            ));
        }

        // Ensure the note exists in the database
        if !AttachmentRepository::verify_note_exists(conn, trimmed_note_id)? {
            return Err(AttachmentError::NoteNotFound(format!(
                "Note with id '{trimmed_note_id}' does not exist"
            )));
        }

        let attachments = AttachmentRepository::list_by_note_id(conn, trimmed_note_id)?;
        Ok(attachments)
    }

    /// Retrieves attachment metadata by ID.
    pub fn get_attachment(conn: &Connection, id: &str) -> Result<Attachment, AttachmentError> {
        let trimmed_id = id.trim();
        if trimmed_id.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Attachment ID cannot be empty".to_string(),
            ));
        }

        match AttachmentRepository::get_by_id(conn, trimmed_id)? {
            Some(attachment) => Ok(attachment),
            None => Err(AttachmentError::AttachmentNotFound(format!(
                "Attachment with id '{trimmed_id}' not found"
            ))),
        }
    }

    /// Removes an attachment:
    /// 1. Verifies attachment metadata exists.
    /// 2. Deletes database record.
    /// 3. Removes managed file from storage.
    ///
    /// If the file was already missing from storage, the database deletion still succeeds cleanly.
    pub fn remove_attachment(
        conn: &Connection,
        storage: &AttachmentStorage,
        id: &str,
    ) -> Result<(), AttachmentError> {
        let trimmed_id = id.trim();
        if trimmed_id.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Attachment ID cannot be empty".to_string(),
            ));
        }

        let attachment = match AttachmentRepository::get_by_id(conn, trimmed_id)? {
            Some(att) => att,
            None => {
                return Err(AttachmentError::AttachmentNotFound(format!(
                    "Attachment with id '{trimmed_id}' not found"
                )))
            }
        };

        // 1. Remove managed file from storage first.
        // If the file is already missing from storage, remove_file returns Ok(()) so stale records can be cleaned up.
        // If filesystem errors (e.g. permission denied) occur, we fail fast without deleting the database record,
        // preserving metadata for retry and diagnostics.
        storage.remove_file(&attachment.relative_path)?;

        // 2. Remove from database
        AttachmentRepository::delete(conn, trimmed_id)?;

        Ok(())
    }

    /// Resolves and verifies the safe filesystem path for an attachment so it can be opened.
    /// Confirms that the file actually exists in managed storage before returning.
    pub fn get_attachment_for_open(
        conn: &Connection,
        storage: &AttachmentStorage,
        id: &str,
    ) -> Result<PathBuf, AttachmentError> {
        let trimmed_id = id.trim();
        if trimmed_id.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Attachment ID cannot be empty".to_string(),
            ));
        }

        let attachment = match AttachmentRepository::get_by_id(conn, trimmed_id)? {
            Some(att) => att,
            None => {
                return Err(AttachmentError::AttachmentNotFound(format!(
                    "Attachment with id '{trimmed_id}' not found"
                )))
            }
        };

        let safe_path = storage.resolve_safe_path(&attachment.relative_path)?;
        if !safe_path.exists() {
            return Err(AttachmentError::ManagedFileMissing(attachment.relative_path));
        }

        // Verify that the file exists, is a regular file, and is accessible for reading
        match std::fs::File::open(&safe_path) {
            Ok(_) => Ok(safe_path),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(AttachmentError::PermissionDenied(attachment.relative_path))
            }
            Err(e) => Err(AttachmentError::FileOperationFailed(format!(
                "Failed to access attachment file: {e}"
            ))),
        }
    }

    /// Checks storage consistency across database attachment records and managed disk files (Phase 7, Task 21).
    /// Detects:
    /// - Missing managed files: DB record references a file not present on disk.
    /// - Unreferenced managed files: File exists on disk without any matching DB record.
    /// - Abandoned temporary staging files: Staging files (`.tmp.`) left behind by interrupted operations.
    /// - Size mismatches: Recorded file size in SQLite differs from byte size on disk.
    pub fn check_consistency(
        conn: &Connection,
        storage: &AttachmentStorage,
    ) -> Result<AttachmentConsistencyReport, AttachmentError> {
        let db_attachments = AttachmentRepository::list_all(conn)?;
        let disk_files = storage.scan_managed_files()?;

        let mut disk_map: std::collections::HashMap<String, &ManagedFileInfo> =
            std::collections::HashMap::new();
        let mut abandoned_temp_files = Vec::new();

        for file in &disk_files {
            if file.is_temp_file {
                abandoned_temp_files.push(file.relative_path.clone());
            } else {
                disk_map.insert(file.relative_path.clone(), file);
            }
        }

        let mut missing_managed_files = Vec::new();
        let mut size_mismatches = Vec::new();
        let mut referenced_paths = std::collections::HashSet::new();

        for att in &db_attachments {
            referenced_paths.insert(att.relative_path.clone());

            match disk_map.get(&att.relative_path) {
                None => {
                    missing_managed_files.push(MissingManagedFileIssue {
                        attachment_id: att.id.clone(),
                        note_id: att.note_id.clone(),
                        file_name: att.file_name.clone(),
                        relative_path: att.relative_path.clone(),
                    });
                }
                Some(disk_file) => {
                    if disk_file.file_size != att.file_size as u64 {
                        size_mismatches.push(SizeMismatchIssue {
                            attachment_id: att.id.clone(),
                            note_id: att.note_id.clone(),
                            file_name: att.file_name.clone(),
                            relative_path: att.relative_path.clone(),
                            recorded_size: att.file_size,
                            actual_size: disk_file.file_size,
                        });
                    }
                }
            }
        }

        let mut unreferenced_managed_files = Vec::new();
        for file in &disk_files {
            if !file.is_temp_file && !referenced_paths.contains(&file.relative_path) {
                unreferenced_managed_files.push(file.relative_path.clone());
            }
        }

        let is_consistent = missing_managed_files.is_empty()
            && unreferenced_managed_files.is_empty()
            && abandoned_temp_files.is_empty()
            && size_mismatches.is_empty();

        Ok(AttachmentConsistencyReport {
            missing_managed_files,
            unreferenced_managed_files,
            abandoned_temp_files,
            size_mismatches,
            total_attachments_checked: db_attachments.len(),
            total_disk_files_checked: disk_files.len(),
            is_consistent,
        })
    }

    /// Cleans up abandoned temporary staging files left behind by interrupted operations (Phase 7, Task 21).
    pub fn cleanup_abandoned_temp_files(
        storage: &AttachmentStorage,
        older_than_seconds: u64,
    ) -> Result<usize, AttachmentError> {
        storage.cleanup_abandoned_temp_files(older_than_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::CreateNoteDto;
    use crate::storage::paths::StoragePaths;
    use crate::storage::repositories::NoteRepository;

    fn setup_test_env() -> (Connection, AttachmentStorage, PathBuf, tempfile_guard::TempDirGuard) {
        let unique = Uuid::new_v4();
        let base_dir = std::env::temp_dir().join(format!("pn_test_att_svc_{unique}"));
        let paths = StoragePaths::from_root(base_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        let sources_dir = base_dir.join("sources");
        fs::create_dir_all(&sources_dir).unwrap();

        let storage = AttachmentStorage::new(paths);
        let guard = tempfile_guard::TempDirGuard(base_dir);

        (conn, storage, sources_dir, guard)
    }

    mod tempfile_guard {
        use std::path::PathBuf;
        pub struct TempDirGuard(pub PathBuf);
        impl Drop for TempDirGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn test_add_attachment_success_lifecycle() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Service Attachment Note".to_string()),
                ..Default::default()
            },
        )
        .expect("Create note");

        // Create a temporary source file
        let src_file = sources_dir.join("sample_report.pdf");
        let sample_bytes = b"%PDF-1.4 sample content for personal notepad";
        fs::write(&src_file, sample_bytes).expect("Write source file");

        // 1. Add attachment
        let attachment = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file)
            .expect("Add attachment should succeed");

        assert_eq!(attachment.note_id, note.id);
        assert_eq!(attachment.file_name, "sample_report.pdf");
        assert_eq!(attachment.file_size, sample_bytes.len() as i64);
        assert_eq!(attachment.mime_type, Some("application/pdf".to_string()));
        assert!(attachment.is_valid());

        // Verify file exists on disk
        assert!(storage.file_exists(&attachment.relative_path));

        // 2. List attachments
        let list = AttachmentService::list_attachments(&conn, &note.id).expect("List attachments");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, attachment.id);

        // 3. Get attachment by ID
        let fetched = AttachmentService::get_attachment(&conn, &attachment.id).expect("Get attachment");
        assert_eq!(fetched.id, attachment.id);
        assert_eq!(fetched.file_name, "sample_report.pdf");

        // 4. Get path for open
        let open_path = AttachmentService::get_attachment_for_open(&conn, &storage, &attachment.id)
            .expect("Get path for open");
        assert!(open_path.exists());
        let content = fs::read(&open_path).expect("Read stored attachment");
        assert_eq!(content, sample_bytes);

        // 5. Remove attachment
        AttachmentService::remove_attachment(&conn, &storage, &attachment.id)
            .expect("Remove attachment");

        // Verify DB record is gone
        let list_after = AttachmentService::list_attachments(&conn, &note.id).unwrap();
        assert_eq!(list_after.len(), 0);

        // Verify file is gone from storage
        assert!(!storage.file_exists(&attachment.relative_path));
    }

    #[test]
    fn test_add_attachment_note_validations() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();

        let src_file = sources_dir.join("test.txt");
        fs::write(&src_file, b"content").unwrap();

        // 1. Nonexistent note
        let err = AttachmentService::add_attachment(&conn, &storage, "nonexistent-id", &src_file)
            .unwrap_err();
        match err {
            AttachmentError::NoteNotFound(_) => {}
            other => panic!("Expected NoteNotFound, got: {other:?}"),
        }

        // 2. Soft-deleted note
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Deleted Note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        NoteRepository::delete(&conn, &note.id, true).unwrap();

        let err_del = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file)
            .unwrap_err();
        match err_del {
            AttachmentError::InvalidRequest(msg) => {
                assert!(msg.contains("deleted note"));
            }
            other => panic!("Expected InvalidRequest, got: {other:?}"),
        }
    }

    #[test]
    fn test_add_attachment_source_file_missing_does_not_create_db_record() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Active Note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let missing_file = sources_dir.join("missing.png");

        let err = AttachmentService::add_attachment(&conn, &storage, &note.id, &missing_file)
            .unwrap_err();

        match err {
            AttachmentError::SourceUnavailable(_) => {}
            other => panic!("Expected SourceUnavailable, got: {other:?}"),
        }

        // DB should have 0 attachments
        let list = AttachmentService::list_attachments(&conn, &note.id).unwrap();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_add_attachment_rollback_on_db_error() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Rollback Note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let src_file = sources_dir.join("test_rollback.txt");
        fs::write(&src_file, b"rollback file bytes").unwrap();

        // We simulate a database failure by dropping the `attachments` table
        conn.execute("DROP TABLE attachments", []).unwrap();

        let result = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file);
        assert!(result.is_err());

        // Crucial verification: Check that NO orphaned file remains in the note directory
        let note_dir = storage.get_note_dir(&note.id).unwrap();
        let entries: Vec<_> = fs::read_dir(&note_dir)
            .map(|rd| rd.filter_map(|e| e.ok()).collect())
            .unwrap_or_default();

        assert_eq!(
            entries.len(),
            0,
            "Expected 0 files in note dir after rollback, but found: {:?}",
            entries.iter().map(|e| e.file_name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_remove_attachment_when_file_already_missing() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Missing File Note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let src_file = sources_dir.join("manual_delete.txt");
        fs::write(&src_file, b"content to delete manually").unwrap();

        let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap();

        // Manually delete the managed file from disk
        let full_path = storage.resolve_safe_path(&att.relative_path).unwrap();
        fs::remove_file(&full_path).unwrap();

        // Removing attachment should still succeed in cleaning up the database record
        let res = AttachmentService::remove_attachment(&conn, &storage, &att.id);
        assert!(res.is_ok());

        // Verify DB record is deleted
        let list = AttachmentService::list_attachments(&conn, &note.id).unwrap();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_get_attachment_for_open_fails_when_file_missing_on_disk() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Missing Disk Note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let src_file = sources_dir.join("will_disappear.txt");
        fs::write(&src_file, b"will disappear").unwrap();

        let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap();

        // Remove disk file
        let full_path = storage.resolve_safe_path(&att.relative_path).unwrap();
        fs::remove_file(&full_path).unwrap();

        let res = AttachmentService::get_attachment_for_open(&conn, &storage, &att.id);
        match res {
            Err(AttachmentError::ManagedFileMissing(_)) => {}
            other => panic!("Expected ManagedFileMissing, got: {other:?}"),
        }
    }

    #[test]
    fn test_task_21_service_consistency_checks_and_temp_cleanup() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Consistency Note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // 1. Initially empty => consistent
        let initial_report =
            AttachmentService::check_consistency(&conn, &storage).expect("Initial check");
        assert!(initial_report.is_consistent);
        assert_eq!(initial_report.total_attachments_checked, 0);
        assert_eq!(initial_report.total_disk_files_checked, 0);

        // 2. Add a valid attachment
        let src_file = sources_dir.join("valid.txt");
        fs::write(&src_file, b"valid contents").unwrap();
        let att1 = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap();

        let report1 = AttachmentService::check_consistency(&conn, &storage).unwrap();
        assert!(report1.is_consistent);
        assert_eq!(report1.total_attachments_checked, 1);
        assert_eq!(report1.total_disk_files_checked, 1);

        // 3. Add an unreferenced file in note dir
        let note_dir = storage.get_note_dir(&note.id).unwrap();
        let unref_path = note_dir.join("orphan_unref.bin");
        fs::write(&unref_path, b"unreferenced").unwrap();

        let report_unref = AttachmentService::check_consistency(&conn, &storage).unwrap();
        assert!(!report_unref.is_consistent);
        assert_eq!(report_unref.unreferenced_managed_files.len(), 1);
        assert!(report_unref.unreferenced_managed_files[0].contains("orphan_unref.bin"));

        // Clean up orphan file
        fs::remove_file(&unref_path).unwrap();

        // 4. Create an abandoned temp file in note dir
        let temp_path = note_dir.join("in_progress.txt.tmp.12345");
        fs::write(&temp_path, b"staging bytes").unwrap();

        let report_temp = AttachmentService::check_consistency(&conn, &storage).unwrap();
        assert!(!report_temp.is_consistent);
        assert_eq!(report_temp.abandoned_temp_files.len(), 1);
        assert!(report_temp.abandoned_temp_files[0].contains(".tmp.12345"));

        // Cleanup abandoned temp files with 0 age threshold
        let cleaned = AttachmentService::cleanup_abandoned_temp_files(&storage, 0).unwrap();
        assert_eq!(cleaned, 1);
        assert!(!temp_path.exists());

        // 5. Simulate a size mismatch (modify disk file size)
        let disk_att1 = storage.resolve_safe_path(&att1.relative_path).unwrap();
        fs::write(&disk_att1, b"tampered longer contents").unwrap();

        let report_mismatch = AttachmentService::check_consistency(&conn, &storage).unwrap();
        assert!(!report_mismatch.is_consistent);
        assert_eq!(report_mismatch.size_mismatches.len(), 1);
        assert_eq!(report_mismatch.size_mismatches[0].attachment_id, att1.id);
        assert_eq!(report_mismatch.size_mismatches[0].recorded_size, b"valid contents".len() as i64);
        assert_eq!(report_mismatch.size_mismatches[0].actual_size, b"tampered longer contents".len() as u64);

        // 6. Simulate missing disk file
        fs::remove_file(&disk_att1).unwrap();
        let report_missing = AttachmentService::check_consistency(&conn, &storage).unwrap();
        assert!(!report_missing.is_consistent);
        assert_eq!(report_missing.missing_managed_files.len(), 1);
        assert_eq!(report_missing.missing_managed_files[0].attachment_id, att1.id);
    }

    #[test]
    fn test_task_22_service_and_security_tests() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Service Security Note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // 1. Add valid initial attachment
        let valid_src = sources_dir.join("initial_safe.pdf");
        fs::write(&valid_src, b"%PDF-1.4 initial attachment").unwrap();
        let att1 = AttachmentService::add_attachment(&conn, &storage, &note.id, &valid_src)
            .expect("Add att1");

        // 2. Failed operation on another attachment does not affect or delete att1
        let invalid_src = sources_dir.join("non_existent_fail.txt");
        let failed_res = AttachmentService::add_attachment(&conn, &storage, &note.id, &invalid_src);
        assert!(failed_res.is_err());

        // att1 remains intact both in DB and on disk
        let list_after_fail = AttachmentService::list_attachments(&conn, &note.id).unwrap();
        assert_eq!(list_after_fail.len(), 1);
        assert_eq!(list_after_fail[0].id, att1.id);
        assert!(storage.file_exists(&att1.relative_path));

        // 3. Security: SQL injection strings in note_id and filenames
        let sql_injection_strings = [
            "'; DROP TABLE attachments; --",
            "' OR '1'='1",
            "1 UNION SELECT * FROM notes --",
            "\" OR \"\"=\"",
        ];

        for payload in sql_injection_strings {
            let malicious_src = sources_dir.join(format!("{payload}.txt"));
            fs::write(&malicious_src, b"safe payload").unwrap();

            // Attaching to malicious note_id fails safely with NoteNotFound or InvalidRequest
            let res_malicious_note = AttachmentService::add_attachment(&conn, &storage, payload, &malicious_src);
            assert!(res_malicious_note.is_err());

            // Attaching file with SQL injection in filename to valid note is sanitized safely
            let att_sanitized = AttachmentService::add_attachment(&conn, &storage, &note.id, &malicious_src)
                .expect("Sanitized attachment should succeed");
            assert!(!att_sanitized.id.is_empty());
            assert!(storage.file_exists(&att_sanitized.relative_path));

            // Clean up sanitized attachment
            AttachmentService::remove_attachment(&conn, &storage, &att_sanitized.id).unwrap();
        }

        // Table must still exist and att1 still intact
        let count_after_sql = AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap();
        assert_eq!(count_after_sql, 1);

        // 4. Security: Path traversal in note_id or relative path rejected
        let traversal_note_ids = ["../etc", "../../root", "/tmp", "sub/folder"];
        for bad_id in traversal_note_ids {
            let res_traversal = AttachmentService::add_attachment(&conn, &storage, bad_id, &valid_src);
            assert!(res_traversal.is_err());
        }

        // 5. Open attachment validation
        let open_valid = AttachmentService::get_attachment_for_open(&conn, &storage, &att1.id).unwrap();
        assert!(open_valid.exists());

        // Opening nonexistent ID fails with AttachmentNotFound
        let open_missing_id = AttachmentService::get_attachment_for_open(&conn, &storage, "nonexistent-id");
        match open_missing_id {
            Err(AttachmentError::AttachmentNotFound(_)) => {}
            other => panic!("Expected AttachmentNotFound, got: {other:?}"),
        }
    }
}
