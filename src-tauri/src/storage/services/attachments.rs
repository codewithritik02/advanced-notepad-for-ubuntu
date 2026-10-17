use rusqlite::Connection;
use std::path::{Path, PathBuf};

use crate::storage::attachment_storage::{
    sanitize_original_filename, AttachmentStorage, ManagedFileInfo,
};
use crate::storage::crypto::SecretBytes;
use crate::storage::errors::AttachmentError;
use crate::storage::models::{
    Attachment, AttachmentConsistencyReport, CreateAttachmentDto, MissingManagedFileIssue,
    SizeMismatchIssue,
};
use crate::storage::repositories::AttachmentRepository;

/// High-level domain service that coordinates attachment lifecycle operations between
/// Tauri IPC commands, managed filesystem storage, and SQLite metadata.
pub struct AttachmentService;

impl AttachmentService {
    /// Adds a file as an attachment to an existing active note.
    pub fn add_attachment(
        conn: &Connection,
        storage: &AttachmentStorage,
        note_id: &str,
        source_path: &Path,
    ) -> Result<Attachment, AttachmentError> {
        Self::add_attachment_with_key(conn, storage, note_id, source_path, None)
    }

    pub fn add_attachment_with_key(
        conn: &Connection,
        storage: &AttachmentStorage,
        note_id: &str,
        source_path: &Path,
        dek: Option<&SecretBytes>,
    ) -> Result<Attachment, AttachmentError> {
        let trimmed_note_id = note_id.trim();
        if trimmed_note_id.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Note ID cannot be empty".to_string(),
            ));
        }

        // 1. Note validation: Verify the note exists and is active
        AttachmentRepository::verify_note_eligible(conn, trimmed_note_id)?;

        // 2-5. Source validation & managed storage copy (encrypted if DEK present)
        let stored = storage.store_file_with_key(trimmed_note_id, source_path, dek)?;

        // 6. Database insertion
        let create_dto = CreateAttachmentDto {
            note_id: trimmed_note_id.to_string(),
            file_name: stored.original_name,
            relative_path: stored.relative_path.clone(),
            mime_type: stored.mime_type,
            file_size: stored.file_size as i64,
        };

        match AttachmentRepository::create_with_key(conn, create_dto, dek) {
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
    pub fn list_attachments(
        conn: &Connection,
        note_id: &str,
    ) -> Result<Vec<Attachment>, AttachmentError> {
        Self::list_attachments_with_key(conn, note_id, None)
    }

    pub fn list_attachments_with_key(
        conn: &Connection,
        note_id: &str,
        dek: Option<&SecretBytes>,
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

        let attachments =
            AttachmentRepository::list_by_note_id_with_key(conn, trimmed_note_id, dek)?;
        Ok(attachments)
    }

    /// Retrieves attachment metadata by ID.
    pub fn get_attachment(conn: &Connection, id: &str) -> Result<Attachment, AttachmentError> {
        Self::get_attachment_with_key(conn, id, None)
    }

    pub fn get_attachment_with_key(
        conn: &Connection,
        id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Attachment, AttachmentError> {
        let trimmed_id = id.trim();
        if trimmed_id.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Attachment ID cannot be empty".to_string(),
            ));
        }

        match AttachmentRepository::get_by_id_with_key(conn, trimmed_id, dek)? {
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

        // Task 20: Do not allow removing attachments from trashed notes
        let is_parent_deleted: bool = conn
            .query_row(
                "SELECT is_deleted FROM notes WHERE id = ?1",
                rusqlite::params![&attachment.note_id],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if is_parent_deleted {
            return Err(AttachmentError::InvalidRequest(
                "Cannot remove attachments from a deleted note. Restore it first.".to_string(),
            ));
        }

        // 1. Remove from database first to prevent orphaned database records pointing to missing files
        AttachmentRepository::delete(conn, trimmed_id)?;

        // 2. Remove managed file from storage. If filesystem removal fails (e.g. locked file),
        // log a warning but do not fail the completed database deletion, leaving the file for deferred orphan cleanup.
        if let Err(cleanup_err) = storage.remove_file(&attachment.relative_path) {
            eprintln!(
                "Warning: Attachment record '{}' removed from DB, but filesystem cleanup failed for '{}': {}",
                trimmed_id, attachment.relative_path, cleanup_err
            );
        }

        Ok(())
    }

    /// Resolves and verifies the safe filesystem path for an attachment so it can be opened.
    pub fn get_attachment_for_open(
        conn: &Connection,
        storage: &AttachmentStorage,
        id: &str,
    ) -> Result<PathBuf, AttachmentError> {
        Self::get_attachment_for_open_with_key(conn, storage, id, None)
    }

    pub fn get_attachment_for_open_with_key(
        conn: &Connection,
        storage: &AttachmentStorage,
        id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<PathBuf, AttachmentError> {
        let trimmed_id = id.trim();
        if trimmed_id.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Attachment ID cannot be empty".to_string(),
            ));
        }

        let attachment = match AttachmentRepository::get_by_id_with_key(conn, trimmed_id, dek)? {
            Some(att) => att,
            None => {
                return Err(AttachmentError::AttachmentNotFound(format!(
                    "Attachment with id '{trimmed_id}' not found"
                )))
            }
        };

        // Enforce executable blocking policy: never execute or open untrusted executables
        if AttachmentStorage::is_executable_extension(&attachment.file_name) {
            return Err(AttachmentError::ExecutableBlocked(
                attachment.file_name.clone(),
            ));
        }

        let safe_path = storage.resolve_safe_path(&attachment.relative_path)?;
        if !safe_path.exists() {
            return Err(AttachmentError::ManagedFileMissing(
                attachment.relative_path,
            ));
        }

        // Secondary check on safe_path file name
        if let Some(disk_name) = safe_path.file_name().and_then(|s| s.to_str()) {
            if AttachmentStorage::is_executable_extension(disk_name) {
                return Err(AttachmentError::ExecutableBlocked(
                    attachment.file_name.clone(),
                ));
            }
        }

        // Determine whether file is encrypted (without relying on file extensions)
        if AttachmentStorage::is_encrypted_file(&safe_path) {
            let key = match dek {
                Some(k) => k,
                None => {
                    return Err(AttachmentError::InvalidRequest(
                        "Application is locked. Cannot open encrypted attachment without active key.".to_string(),
                    ));
                }
            };

            let decrypted_dir = storage.decrypted_attachments_dir();
            std::fs::create_dir_all(&decrypted_dir).map_err(|e| {
                AttachmentError::FileOperationFailed(format!(
                    "Failed to create decrypted temporary directory: {e}"
                ))
            })?;

            let stored_name = attachment
                .relative_path
                .split('/')
                .next_back()
                .unwrap_or(&attachment.relative_path);
            let aad = format!("attachments:{}:{stored_name}", attachment.note_id.trim());

            let temp_file_name = format!(
                "{}_{}",
                uuid::Uuid::new_v4(),
                sanitize_original_filename(&attachment.file_name)
            );
            let temp_decrypted_path = decrypted_dir.join(temp_file_name);

            crate::storage::crypto::decrypt_file_to_file(
                &safe_path,
                &temp_decrypted_path,
                key,
                aad.as_bytes(),
            )
            .map_err(|e| {
                AttachmentError::FileOperationFailed(format!("Attachment decryption failed: {e}"))
            })?;

            Ok(temp_decrypted_path)
        } else {
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
    }

    /// Reads an image attachment into memory for in-app previews.
    ///
    /// Safety rules:
    /// - only real images are returned (PNG, JPEG, GIF, WebP, SVG); the type is detected from the
    ///   file *content*, never from the file name alone;
    /// - encrypted attachments are decrypted in memory only (no plaintext temp file);
    /// - files larger than `max_bytes` are refused so a preview can never exhaust memory.
    pub fn read_image_attachment_bytes_with_key(
        conn: &Connection,
        storage: &AttachmentStorage,
        id: &str,
        dek: Option<&SecretBytes>,
        max_bytes: u64,
    ) -> Result<Vec<u8>, AttachmentError> {
        let trimmed_id = id.trim();
        if trimmed_id.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Attachment ID cannot be empty".to_string(),
            ));
        }

        let attachment = AttachmentRepository::get_by_id_with_key(conn, trimmed_id, dek)?
            .ok_or_else(|| {
                AttachmentError::AttachmentNotFound(format!(
                    "Attachment with id '{trimmed_id}' not found"
                ))
            })?;

        if attachment.file_size < 0 || attachment.file_size as u64 > max_bytes {
            return Err(AttachmentError::FileTooLarge {
                size_bytes: attachment.file_size.max(0) as u64,
                max_bytes,
            });
        }

        let safe_path = storage.resolve_safe_path(&attachment.relative_path)?;
        if !safe_path.exists() {
            return Err(AttachmentError::ManagedFileMissing(
                attachment.relative_path,
            ));
        }

        let bytes = if AttachmentStorage::is_encrypted_file(&safe_path) {
            let key = dek.ok_or_else(|| {
                AttachmentError::InvalidRequest(
                    "Application is locked. Cannot read encrypted attachment without active key."
                        .to_string(),
                )
            })?;
            let stored_name = attachment
                .relative_path
                .split('/')
                .next_back()
                .unwrap_or(&attachment.relative_path);
            let aad = format!("attachments:{}:{stored_name}", attachment.note_id.trim());
            crate::storage::crypto::decrypt_file_to_vec(&safe_path, key, aad.as_bytes()).map_err(
                |e| {
                    AttachmentError::FileOperationFailed(format!(
                        "Attachment decryption failed: {e}"
                    ))
                },
            )?
        } else {
            std::fs::read(&safe_path).map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    AttachmentError::PermissionDenied(attachment.relative_path.clone())
                }
                _ => AttachmentError::FileOperationFailed(format!(
                    "Failed to read attachment file: {e}"
                )),
            })?
        };

        if bytes.len() as u64 > max_bytes {
            return Err(AttachmentError::FileTooLarge {
                size_bytes: bytes.len() as u64,
                max_bytes,
            });
        }

        if detect_image_mime(&bytes).is_none() {
            return Err(AttachmentError::InvalidRequest(format!(
                "'{}' is not a supported image (PNG, JPEG, GIF, WebP or SVG).",
                attachment.file_name
            )));
        }

        Ok(bytes)
    }

    /// Checks storage consistency across database attachment records and managed disk files (Phase 7, Task 21).
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
                    let expected_size = att.file_size as u64;
                    let matches_plaintext = disk_file.file_size == expected_size;
                    let matches_encrypted = disk_file.file_size
                        == expected_size + crate::storage::crypto::MIN_ENVELOPE_LEN as u64;

                    if !matches_plaintext && !matches_encrypted {
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

    /// Cleans up unreferenced managed files identified during consistency checks.
    pub fn cleanup_unreferenced_files(
        conn: &Connection,
        storage: &AttachmentStorage,
    ) -> Result<usize, AttachmentError> {
        let report = Self::check_consistency(conn, storage)?;
        let mut count = 0;
        let mut failures = Vec::new();
        for rel_path in report.unreferenced_managed_files {
            match storage.remove_file(&rel_path) {
                Ok(()) => count += 1,
                Err(error) => failures.push(error.user_friendly_message()),
            }
        }
        if !failures.is_empty() {
            return Err(AttachmentError::FileOperationFailed(format!(
                "Failed to clean {} unreferenced attachment file(s): {}",
                failures.len(),
                failures[0]
            )));
        }
        Ok(count)
    }
}

/// Detects a previewable image type from file *content* (magic bytes). Returns the MIME type.
pub fn detect_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    // SVG is text: only accept it when the document really starts like an SVG. It is always shown
    // through an <img> element, where scripts never run.
    let head = &bytes[..bytes.len().min(2048)];
    if let Ok(text) = std::str::from_utf8(head) {
        let lower = text
            .trim_start_matches('\u{feff}')
            .trim_start()
            .to_ascii_lowercase();
        if lower.starts_with("<svg")
            || ((lower.starts_with("<?xml") || lower.starts_with("<!doctype svg"))
                && lower.contains("<svg"))
        {
            return Some("image/svg+xml");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::CreateNoteDto;
    use crate::storage::paths::StoragePaths;
    use crate::storage::repositories::NoteRepository;
    use std::fs;
    use uuid::Uuid;

    fn setup_test_env() -> (
        Connection,
        AttachmentStorage,
        PathBuf,
        tempfile_guard::TempDirGuard,
    ) {
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
        let fetched =
            AttachmentService::get_attachment(&conn, &attachment.id).expect("Get attachment");
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

    // Smallest valid 1x1 PNG
    const TINY_PNG: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn test_detect_image_mime_uses_content_not_extension() {
        assert_eq!(detect_image_mime(&TINY_PNG), Some("image/png"));
        assert_eq!(
            detect_image_mime(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]),
            Some("image/jpeg")
        );
        assert_eq!(detect_image_mime(b"GIF89a\x01\x00"), Some("image/gif"));
        assert_eq!(
            detect_image_mime(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some("image/webp")
        );
        assert_eq!(
            detect_image_mime(
                b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>"
            ),
            Some("image/svg+xml")
        );
        assert_eq!(
            detect_image_mime(b"<svg width=\"1\"></svg>"),
            Some("image/svg+xml")
        );
        // Not images, even if a caller names them like images.
        assert_eq!(detect_image_mime(b"%PDF-1.4"), None);
        assert_eq!(detect_image_mime(b"MZ\x90\x00 executable"), None);
        assert_eq!(detect_image_mime(b"<html><svg></svg></html>"), None);
        assert_eq!(detect_image_mime(b""), None);
    }

    #[test]
    fn test_read_image_attachment_bytes_plaintext_roundtrip() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();
        let note = NoteRepository::create(&conn, CreateNoteDto::default()).expect("Create note");

        let src_file = sources_dir.join("pic.png");
        fs::write(&src_file, TINY_PNG).unwrap();
        let attachment =
            AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap();

        let bytes = AttachmentService::read_image_attachment_bytes_with_key(
            &conn,
            &storage,
            &attachment.id,
            None,
            1024 * 1024,
        )
        .expect("image bytes");
        assert_eq!(bytes, TINY_PNG);
    }

    #[test]
    fn test_read_image_attachment_bytes_rejects_non_images_even_with_image_name() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();
        let note = NoteRepository::create(&conn, CreateNoteDto::default()).expect("Create note");

        // Text pretending to be a PNG
        let src_file = sources_dir.join("fake.png");
        fs::write(&src_file, b"just some text, not an image").unwrap();
        let attachment =
            AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap();

        let err = AttachmentService::read_image_attachment_bytes_with_key(
            &conn,
            &storage,
            &attachment.id,
            None,
            1024 * 1024,
        )
        .expect_err("non-image must be refused");
        assert!(matches!(err, AttachmentError::InvalidRequest(_)), "{err:?}");
    }

    #[test]
    fn test_read_image_attachment_bytes_enforces_size_limit_and_unknown_id() {
        let (conn, storage, sources_dir, _guard) = setup_test_env();
        let note = NoteRepository::create(&conn, CreateNoteDto::default()).expect("Create note");

        let src_file = sources_dir.join("pic.png");
        fs::write(&src_file, TINY_PNG).unwrap();
        let attachment =
            AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap();

        let too_big = AttachmentService::read_image_attachment_bytes_with_key(
            &conn,
            &storage,
            &attachment.id,
            None,
            10,
        )
        .expect_err("size limit");
        assert!(
            matches!(too_big, AttachmentError::FileTooLarge { .. }),
            "{too_big:?}"
        );

        let missing = AttachmentService::read_image_attachment_bytes_with_key(
            &conn,
            &storage,
            "does-not-exist",
            None,
            1024,
        )
        .expect_err("unknown id");
        assert!(
            matches!(missing, AttachmentError::AttachmentNotFound(_)),
            "{missing:?}"
        );

        let empty = AttachmentService::read_image_attachment_bytes_with_key(
            &conn, &storage, "  ", None, 1024,
        )
        .expect_err("blank id");
        assert!(
            matches!(empty, AttachmentError::InvalidRequest(_)),
            "{empty:?}"
        );
    }

    #[test]
    fn test_read_image_attachment_bytes_encrypted_is_decrypted_in_memory_only() {
        use crate::storage::crypto::generate_dek;

        let (conn, storage, sources_dir, _guard) = setup_test_env();
        let note = NoteRepository::create(&conn, CreateNoteDto::default()).expect("Create note");
        let dek = generate_dek();

        let src_file = sources_dir.join("secret.png");
        fs::write(&src_file, TINY_PNG).unwrap();
        let attachment = AttachmentService::add_attachment_with_key(
            &conn,
            &storage,
            &note.id,
            &src_file,
            Some(&dek),
        )
        .unwrap();

        // On disk the file is encrypted (does not start with the PNG signature).
        let on_disk = fs::read(
            storage
                .resolve_safe_path(&attachment.relative_path)
                .unwrap(),
        )
        .unwrap();
        assert!(!on_disk.starts_with(&[0x89, b'P', b'N', b'G']));

        let decrypted_dir = storage.decrypted_attachments_dir();
        let before = fs::read_dir(&decrypted_dir).map(|d| d.count()).unwrap_or(0);

        let bytes = AttachmentService::read_image_attachment_bytes_with_key(
            &conn,
            &storage,
            &attachment.id,
            Some(&dek),
            1024 * 1024,
        )
        .expect("decrypted image bytes");
        assert_eq!(bytes, TINY_PNG);

        // No plaintext temp file was written for the preview.
        let after = fs::read_dir(&decrypted_dir).map(|d| d.count()).unwrap_or(0);
        assert_eq!(
            before, after,
            "preview must not create plaintext temp files"
        );

        // Without the key the encrypted image can't be read.
        let locked = AttachmentService::read_image_attachment_bytes_with_key(
            &conn,
            &storage,
            &attachment.id,
            None,
            1024 * 1024,
        );
        assert!(locked.is_err());
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

        let err_del =
            AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap_err();
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
        assert_eq!(
            report_mismatch.size_mismatches[0].recorded_size,
            b"valid contents".len() as i64
        );
        assert_eq!(
            report_mismatch.size_mismatches[0].actual_size,
            b"tampered longer contents".len() as u64
        );

        // 6. Simulate missing disk file
        fs::remove_file(&disk_att1).unwrap();
        let report_missing = AttachmentService::check_consistency(&conn, &storage).unwrap();
        assert!(!report_missing.is_consistent);
        assert_eq!(report_missing.missing_managed_files.len(), 1);
        assert_eq!(
            report_missing.missing_managed_files[0].attachment_id,
            att1.id
        );
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
            let res_malicious_note =
                AttachmentService::add_attachment(&conn, &storage, payload, &malicious_src);
            assert!(res_malicious_note.is_err());

            // Attaching file with SQL injection in filename to valid note is sanitized safely
            let att_sanitized =
                AttachmentService::add_attachment(&conn, &storage, &note.id, &malicious_src)
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
            let res_traversal =
                AttachmentService::add_attachment(&conn, &storage, bad_id, &valid_src);
            assert!(res_traversal.is_err());
        }

        // 5. Open attachment validation
        let open_valid =
            AttachmentService::get_attachment_for_open(&conn, &storage, &att1.id).unwrap();
        assert!(open_valid.exists());

        // Opening nonexistent ID fails with AttachmentNotFound
        let open_missing_id =
            AttachmentService::get_attachment_for_open(&conn, &storage, "nonexistent-id");
        match open_missing_id {
            Err(AttachmentError::AttachmentNotFound(_)) => {}
            other => panic!("Expected AttachmentNotFound, got: {other:?}"),
        }
    }

    #[test]
    fn test_task_10_attachment_encryption_lifecycle_and_security() {
        use crate::storage::crypto::SecretBytes;

        let (conn, storage, sources_dir, _guard) = setup_test_env();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Secure Note for Attachments".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let src_file = sources_dir.join("financial_statement.pdf");
        let plaintext_payload = b"%PDF-1.4 Confidentially encrypted financial payload for Q3 2026!";
        fs::write(&src_file, plaintext_payload).unwrap();

        let dek = SecretBytes::new(vec![0x7A; 32]);

        // 1. Add attachment with DEK
        let attachment = AttachmentService::add_attachment_with_key(
            &conn,
            &storage,
            &note.id,
            &src_file,
            Some(&dek),
        )
        .expect("Encrypted attachment creation should succeed");

        assert_eq!(attachment.file_name, "financial_statement.pdf");
        assert_eq!(attachment.file_size, plaintext_payload.len() as i64);
        assert_eq!(attachment.mime_type, Some("application/pdf".to_string()));

        // 2. Verify SQLite encryption at rest
        let raw_db_filename: String = conn
            .query_row(
                "SELECT file_name FROM attachments WHERE id = ?",
                [&attachment.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            raw_db_filename.starts_with("ENC$v1$"),
            "SQLite file_name column MUST be encrypted with ENC$v1$, but got: {raw_db_filename}"
        );
        assert!(
            !raw_db_filename.contains("financial_statement.pdf"),
            "SQLite file_name column MUST NOT leak plaintext filename!"
        );

        // 3. Verify disk file encryption at rest
        let stored_path = storage
            .resolve_safe_path(&attachment.relative_path)
            .unwrap();
        assert!(stored_path.exists());
        let disk_bytes = fs::read(&stored_path).unwrap();

        assert_eq!(
            disk_bytes[0],
            crate::storage::crypto::cipher::ENVELOPE_VERSION_V1
        );
        assert_eq!(disk_bytes.len(), plaintext_payload.len() + 29);
        assert!(!disk_bytes
            .windows(plaintext_payload.len())
            .any(|w| w == plaintext_payload));
        assert!(AttachmentStorage::is_encrypted_file(&stored_path));

        // 4. Listing attachments
        // With DEK
        let list_unlocked =
            AttachmentService::list_attachments_with_key(&conn, &note.id, Some(&dek)).unwrap();
        assert_eq!(list_unlocked.len(), 1);
        assert_eq!(list_unlocked[0].file_name, "financial_statement.pdf");

        // Without DEK (locked) - must reject with locked error
        let list_locked_err =
            AttachmentService::list_attachments_with_key(&conn, &note.id, None).unwrap_err();
        match list_locked_err {
            AttachmentError::FileOperationFailed(msg) => {
                assert!(
                    msg.contains("locked"),
                    "Expected locked error message, got: {msg}"
                );
            }
            other => panic!("Expected locked error, got: {other:?}"),
        }

        // 5. Opening attachment when locked vs unlocked
        // Locked
        let locked_res = AttachmentService::get_attachment_for_open_with_key(
            &conn,
            &storage,
            &attachment.id,
            None,
        );
        let locked_err = locked_res.unwrap_err();
        let err_msg = format!("{locked_err:?}");
        assert!(
            err_msg.contains("locked"),
            "Expected locked error message, got: {err_msg}"
        );

        // Unlocked
        let opened_path = AttachmentService::get_attachment_for_open_with_key(
            &conn,
            &storage,
            &attachment.id,
            Some(&dek),
        )
        .expect("Unlocked get_attachment_for_open should succeed");

        assert!(opened_path.exists());
        assert!(opened_path.starts_with(storage.decrypted_attachments_dir()));
        let opened_bytes = fs::read(&opened_path).unwrap();
        assert_eq!(opened_bytes, plaintext_payload);

        // 6. Tamper detection on disk
        let mut tampered_bytes = disk_bytes.clone();
        tampered_bytes[15] ^= 0xFF; // Flip bit in ciphertext
        fs::write(&stored_path, &tampered_bytes).unwrap();

        let tamper_res = AttachmentService::get_attachment_for_open_with_key(
            &conn,
            &storage,
            &attachment.id,
            Some(&dek),
        );
        assert!(
            tamper_res.is_err(),
            "Tampered ciphertext must fail tag validation!"
        );
        // Restore original valid ciphertext
        fs::write(&stored_path, &disk_bytes).unwrap();

        // 7. AAD Splicing Defense (Note context binding)
        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Victim Note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // Tamper note_id in DB to point to note2
        conn.execute(
            "UPDATE attachments SET note_id = ? WHERE id = ?",
            [&note2.id, &attachment.id],
        )
        .unwrap();

        let splice_res = AttachmentService::get_attachment_for_open_with_key(
            &conn,
            &storage,
            &attachment.id,
            Some(&dek),
        );
        assert!(
            splice_res.is_err(),
            "Splicing attachment to a different note must fail AAD validation!"
        );
        // Restore note_id
        conn.execute(
            "UPDATE attachments SET note_id = ? WHERE id = ?",
            [&note.id, &attachment.id],
        )
        .unwrap();

        // 8. Consistency check tolerates encrypted file size
        let report = AttachmentService::check_consistency(&conn, &storage).unwrap();
        assert!(
            report.is_consistent,
            "Check consistency should pass for encrypted attachments (report: {report:?})"
        );

        // 9. Cleanup temporary decrypted attachments
        assert!(opened_path.exists());
        storage.cleanup_decrypted_attachments().unwrap();
        assert!(
            !opened_path.exists(),
            "Decrypted temp file should be deleted after cleanup"
        );
    }
}
