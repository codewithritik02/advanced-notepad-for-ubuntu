use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::storage::errors::AttachmentError;
use crate::storage::paths::StoragePaths;

/// Default maximum allowed file size for attachments: 100 MiB
pub const DEFAULT_MAX_ATTACHMENT_SIZE_BYTES: u64 = 100 * 1024 * 1024;

/// Result of storing a source file in managed attachment storage
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAttachmentFile {
    /// Safe relative path inside `attachments_dir` (e.g. `<note_id>/<stored_name>`)
    pub relative_path: String,
    /// Collision-resistant stored filename (e.g. `<uuid>.<ext>`)
    pub stored_name: String,
    /// Sanitized user-facing original filename (e.g. `report.pdf`)
    pub original_name: String,
    /// Verified stored file size in bytes
    pub file_size: u64,
    /// Safely inferred MIME type
    pub mime_type: Option<String>,
}

/// Result of inspecting and validating a source file prior to storage (Phase 7, Task 6)
#[derive(Debug)]
pub struct ValidatedSourceFile {
    pub file: File,
    pub original_name: String,
    pub file_size: u64,
    pub mime_type: Option<String>,
}

/// Dedicated abstraction for managing attachment files in the application's data directory.
#[derive(Debug, Clone)]
pub struct AttachmentStorage {
    paths: StoragePaths,
    max_file_size: u64,
}

impl AttachmentStorage {
    /// Constructs a new `AttachmentStorage` with the default 100 MiB limit.
    pub fn new(paths: StoragePaths) -> Self {
        Self {
            paths,
            max_file_size: DEFAULT_MAX_ATTACHMENT_SIZE_BYTES,
        }
    }

    /// Constructs an `AttachmentStorage` with a custom maximum file size.
    pub fn with_max_size(paths: StoragePaths, max_file_size: u64) -> Self {
        Self {
            paths,
            max_file_size,
        }
    }

    /// Returns the configured maximum attachment size in bytes.
    pub fn max_file_size(&self) -> u64 {
        self.max_file_size
    }

    /// Resolves the absolute directory for a specific note's attachments (`attachments/<note_id>/`).
    /// Strictly rejects path traversal or invalid note identifiers.
    pub fn get_note_dir(&self, note_id: &str) -> Result<PathBuf, AttachmentError> {
        let trimmed = note_id.trim();
        if trimmed.is_empty()
            || trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.contains("..")
        {
            return Err(AttachmentError::InvalidRequest(format!(
                "Invalid note identifier for attachment storage: '{note_id}'"
            )));
        }

        self.paths
            .get_note_attachments_dir(trimmed)
            .map_err(|e| match e {
                crate::storage::errors::StorageError::Io(io_err) => match io_err.kind() {
                    io::ErrorKind::PermissionDenied => {
                        AttachmentError::PermissionDenied("Permission denied accessing attachments directory".to_string())
                    }
                    _ if io_err.raw_os_error() == Some(28) => AttachmentError::InsufficientDiskSpace,
                    _ => AttachmentError::Io(io_err),
                },
                other => AttachmentError::FileOperationFailed(other.to_string()),
            })
    }

    /// Generates a collision-resistant, unpredictable storage filename (UUID-based).
    /// If an extension is present in `original_filename`, it is strictly sanitized to alphanumeric characters.
    pub fn generate_stored_name(original_filename: &str) -> String {
        let unique_id = Uuid::new_v4().to_string();
        if let Some(ext) = sanitize_extension(original_filename) {
            format!("{unique_id}.{ext}")
        } else {
            unique_id
        }
    }

    /// Resolves and validates a safe absolute path for a stored attachment within `attachments_dir`.
    /// Strictly prevents path traversal attacks (`../`).
    pub fn resolve_safe_path(&self, relative_path: &str) -> Result<PathBuf, AttachmentError> {
        let trimmed = relative_path.trim();
        if trimmed.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Attachment relative path cannot be empty".to_string(),
            ));
        }

        let full_path = StoragePaths::sanitize_subpath(
            &self.paths.attachments_dir,
            Path::new(trimmed),
        )
        .map_err(|e| AttachmentError::InvalidRequest(e.to_string()))?;

        Ok(full_path)
    }

    /// Verifies whether a managed attachment file exists on disk.
    pub fn file_exists(&self, relative_path: &str) -> bool {
        match self.resolve_safe_path(relative_path) {
            Ok(path) => path.is_file(),
            Err(_) => false,
        }
    }

    /// Reads metadata of a managed attachment file.
    pub fn get_metadata(&self, relative_path: &str) -> Result<fs::Metadata, AttachmentError> {
        let full_path = self.resolve_safe_path(relative_path)?;
        if !full_path.is_file() {
            return Err(AttachmentError::ManagedFileMissing(relative_path.to_string()));
        }

        fs::metadata(&full_path).map_err(|e| match e.kind() {
            io::ErrorKind::PermissionDenied => AttachmentError::PermissionDenied(relative_path.to_string()),
            io::ErrorKind::NotFound => AttachmentError::ManagedFileMissing(relative_path.to_string()),
            _ => AttachmentError::Io(e),
        })
    }

    /// Copies a source file into managed attachment storage safely using a temporary file strategy:
    /// 1. Inspects and validates source file (existence, regular file, size limit, readability).
    /// 2. Uses already-opened file handle to avoid TOCTOU races.
    /// 3. Generates collision-resistant unique storage filename.
    /// 4. Copies bytes to a temporary staging file (`<stored_name>.tmp.<uuid>`).
    /// 5. Verifies byte size and integrity.
    /// 6. Atomically renames the temporary file to its final destination.
    /// 7. Automatically cleans up temporary files on any failure.
    pub fn store_file(
        &self,
        note_id: &str,
        source_path: &Path,
    ) -> Result<StoredAttachmentFile, AttachmentError> {
        let mut validated = self.validate_source_file(source_path)?;

        let note_dir = self.get_note_dir(note_id)?;
        let stored_name = Self::generate_stored_name(&validated.original_name);
        let final_path = note_dir.join(&stored_name);

        let relative_path = format!("{}/{}", note_id.trim(), stored_name);

        let temp_filename = format!("{stored_name}.tmp.{}", Uuid::new_v4());
        let temp_path = note_dir.join(temp_filename);

        let copy_result = (|| -> Result<u64, AttachmentError> {
            let mut tmp_file = File::create(&temp_path).map_err(|e| match e.kind() {
                io::ErrorKind::PermissionDenied => AttachmentError::PermissionDenied(
                    "Permission denied creating managed temporary file".to_string(),
                ),
                _ if e.raw_os_error() == Some(28) => AttachmentError::InsufficientDiskSpace,
                _ => AttachmentError::FileOperationFailed(format!(
                    "Failed to create temporary file: {e}"
                )),
            })?;

            let mut buffer = [0u8; 64 * 1024]; // 64 KiB chunk buffer
            let mut bytes_copied: u64 = 0;

            loop {
                let read_bytes = validated.file.read(&mut buffer).map_err(|e| {
                    AttachmentError::FileOperationFailed(format!("Error reading source file: {e}"))
                })?;

                if read_bytes == 0 {
                    break;
                }

                bytes_copied += read_bytes as u64;
                if bytes_copied > self.max_file_size {
                    return Err(AttachmentError::FileTooLarge {
                        size_bytes: bytes_copied,
                        max_bytes: self.max_file_size,
                    });
                }

                tmp_file.write_all(&buffer[..read_bytes]).map_err(|e| {
                    if e.raw_os_error() == Some(28) {
                        AttachmentError::InsufficientDiskSpace
                    } else {
                        AttachmentError::FileOperationFailed(format!(
                            "Error writing to temporary attachment file: {e}"
                        ))
                    }
                })?;
            }

            tmp_file.flush().map_err(|e| {
                if e.raw_os_error() == Some(28) {
                    AttachmentError::InsufficientDiskSpace
                } else {
                    AttachmentError::FileOperationFailed(format!("Error flushing attachment file: {e}"))
                }
            })?;

            Ok(bytes_copied)
        })();

        let written_bytes = match copy_result {
            Ok(bytes) => bytes,
            Err(err) => {
                let _ = fs::remove_file(&temp_path);
                return Err(err);
            }
        };

        if written_bytes != validated.file_size {
            let _ = fs::remove_file(&temp_path);
            return Err(AttachmentError::FileOperationFailed(format!(
                "File size mismatch after copy: expected {} bytes, wrote {written_bytes} bytes",
                validated.file_size
            )));
        }

        if let Err(e) = fs::rename(&temp_path, &final_path) {
            let _ = fs::remove_file(&temp_path);
            return Err(AttachmentError::FileOperationFailed(format!(
                "Failed to finalize attachment file: {e}"
            )));
        }

        Ok(StoredAttachmentFile {
            relative_path,
            stored_name,
            original_name: validated.original_name,
            file_size: written_bytes,
            mime_type: validated.mime_type,
        })
    }

    /// Inspects and validates a source file before storing (Phase 7, Task 6):
    /// - Avoids TOCTOU validation-then-use race by opening the handle directly.
    /// - Checks existence, regular file, readability.
    /// - Verifies size limit without overflow.
    /// - Sanitizes user-facing original filename for metadata representation.
    /// - Detects MIME type via magic bytes + sanitized extension fallback.
    pub fn validate_source_file(
        &self,
        source_path: &Path,
    ) -> Result<ValidatedSourceFile, AttachmentError> {
        if !source_path.exists() {
            return Err(AttachmentError::SourceUnavailable(
                "Source file does not exist".to_string(),
            ));
        }

        let mut file = File::open(source_path).map_err(|e| match e.kind() {
            io::ErrorKind::PermissionDenied => AttachmentError::PermissionDenied(
                "Permission denied while reading source file".to_string(),
            ),
            io::ErrorKind::NotFound => AttachmentError::SourceUnavailable(
                "Source file does not exist".to_string(),
            ),
            _ => AttachmentError::SourceUnavailable(e.to_string()),
        })?;

        let meta = file.metadata().map_err(|e| match e.kind() {
            io::ErrorKind::PermissionDenied => AttachmentError::PermissionDenied(
                "Permission denied while inspecting source file metadata".to_string(),
            ),
            _ => AttachmentError::SourceUnavailable(e.to_string()),
        })?;

        if !meta.is_file() {
            return Err(AttachmentError::SourceUnavailable(
                "Selected source is not a regular file".to_string(),
            ));
        }

        let file_size = meta.len();
        if file_size > self.max_file_size {
            return Err(AttachmentError::FileTooLarge {
                size_bytes: file_size,
                max_bytes: self.max_file_size,
            });
        }

        let raw_filename = source_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "attachment".to_string());

        let original_name = sanitize_original_filename(&raw_filename);

        // Read magic bytes header (up to 32 bytes) for content inspection
        let mut magic_header = [0u8; 32];
        let bytes_read = file.read(&mut magic_header).unwrap_or(0);
        let mime_type = detect_mime_type_with_magic(&original_name, &magic_header[..bytes_read]);

        // Rewind file cursor to start so subsequent reads stream from the beginning
        use std::io::Seek;
        file.rewind().map_err(|e| {
            AttachmentError::FileOperationFailed(format!("Failed to rewind source file cursor: {e}"))
        })?;

        Ok(ValidatedSourceFile {
            file,
            original_name,
            file_size,
            mime_type,
        })
    }

    /// Safely removes a managed attachment file from disk.
    /// Strictly ensures the file is within `attachments_dir` before deleting.
    /// Returns successfully if the file was already missing or was safely removed.
    pub fn remove_file(&self, relative_path: &str) -> Result<(), AttachmentError> {
        let full_path = self.resolve_safe_path(relative_path)?;

        if full_path.exists() {
            fs::remove_file(&full_path).map_err(|e| match e.kind() {
                io::ErrorKind::PermissionDenied => AttachmentError::PermissionDenied(
                    "Permission denied removing attachment file".to_string(),
                ),
                _ => AttachmentError::FileOperationFailed(format!(
                    "Failed to remove attachment file: {e}"
                )),
            })?;
        }

        // Optional cleanup: if the parent note folder is empty, attempt to remove it
        if let Some(parent) = full_path.parent() {
            if parent != self.paths.attachments_dir {
                if let Ok(mut entries) = fs::read_dir(parent) {
                    if entries.next().is_none() {
                        let _ = fs::remove_dir(parent);
                    }
                }
            }
        }

        Ok(())
    }

    /// Safely removes an entire note attachment directory from managed storage (Phase 7, Task 20).
    /// Used during permanent note deletion to ensure no untracked files are left behind.
    pub fn remove_note_directory(&self, note_id: &str) -> Result<(), AttachmentError> {
        let trimmed = note_id.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        let note_dir = self.paths.attachments_dir.join(trimmed);
        if note_dir.exists() && note_dir.is_dir() {
            if let Ok(canonical) = note_dir.canonicalize() {
                if let Ok(canonical_root) = self.paths.attachments_dir.canonicalize() {
                    if canonical.starts_with(&canonical_root) && canonical != canonical_root {
                        let _ = fs::remove_dir_all(&canonical);
                    }
                }
            }
        }

        Ok(())
    }

    /// Safely scans all files inside the managed `attachments_dir` (Phase 7, Task 21).
    /// Safety guarantees:
    /// - Strictly bound to `self.paths.attachments_dir`
    /// - Ignores/skips symbolic links
    /// - Safely identifies temporary staging files (`.tmp.`)
    pub fn scan_managed_files(&self) -> Result<Vec<ManagedFileInfo>, AttachmentError> {
        let root = &self.paths.attachments_dir;
        if !root.exists() {
            return Ok(Vec::new());
        }

        let canonical_root = match root.canonicalize() {
            Ok(c) => c,
            Err(_) => return Ok(Vec::new()),
        };
        let mut results = Vec::new();

        let note_dirs = match fs::read_dir(&canonical_root) {
            Ok(dirs) => dirs,
            Err(e) => return Err(AttachmentError::Io(e)),
        };

        for dir_entry in note_dirs.flatten() {
            let file_type = match dir_entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            // Safety rule: skip symlinks
            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_dir() {
                let note_dir_path = dir_entry.path();
                // Ensure containment
                if let Ok(canon_note_dir) = note_dir_path.canonicalize() {
                    if !canon_note_dir.starts_with(&canonical_root) {
                        continue;
                    }
                    if let Ok(files) = fs::read_dir(&canon_note_dir) {
                        for file_entry in files.flatten() {
                            let ft = match file_entry.file_type() {
                                Ok(ft) => ft,
                                Err(_) => continue,
                            };
                            if ft.is_symlink() || !ft.is_file() {
                                continue;
                            }

                            let file_name = file_entry.file_name().to_string_lossy().to_string();
                            let meta = match file_entry.metadata() {
                                Ok(m) => m,
                                Err(_) => continue,
                            };

                            let note_folder_name = dir_entry.file_name().to_string_lossy().to_string();
                            let relative_path = format!("{}/{}", note_folder_name, file_name);
                            let is_temp_file = file_name.contains(".tmp.");

                            results.push(ManagedFileInfo {
                                relative_path,
                                file_name,
                                file_size: meta.len(),
                                is_temp_file,
                                modified_system_time: meta.modified().ok(),
                            });
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(results)
    }

    /// Cleans up abandoned temporary staging files left behind by interrupted operations (Phase 7, Task 21).
    /// Safety rules:
    /// - Only cleans files matching `.tmp.`
    /// - Only removes files older than `older_than_seconds` to never touch active in-flight copies
    pub fn cleanup_abandoned_temp_files(&self, older_than_seconds: u64) -> Result<usize, AttachmentError> {
        let scanned = self.scan_managed_files()?;
        let now = std::time::SystemTime::now();
        let mut cleaned_count = 0;

        for file in scanned {
            if file.is_temp_file {
                let is_abandoned = match file.modified_system_time {
                    Some(mtime) => match now.duration_since(mtime) {
                        Ok(dur) => dur.as_secs() >= older_than_seconds,
                        Err(_) => true,
                    },
                    None => true,
                };

                if is_abandoned {
                    let _ = self.remove_file(&file.relative_path);
                    cleaned_count += 1;
                }
            }
        }

        Ok(cleaned_count)
    }
}

/// Information about a file residing inside managed attachment storage (Phase 7, Task 21)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedFileInfo {
    pub relative_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub is_temp_file: bool,
    pub modified_system_time: Option<std::time::SystemTime>,
}

/// Sanitizes a file extension, allowing only 1–10 lowercase ASCII alphanumeric characters.
pub fn sanitize_extension(filename: &str) -> Option<String> {
    let path = Path::new(filename);
    let ext_str = path.extension()?.to_str()?;

    if ext_str.is_empty() || ext_str.len() > 10 || ext_str.contains(char::is_whitespace) {
        return None;
    }

    let lower = ext_str.to_ascii_lowercase();
    if lower.chars().all(|c| c.is_ascii_alphanumeric()) {
        Some(lower)
    } else {
        None
    }
}

/// Infers common MIME types based on sanitized file extension.
pub fn detect_mime_type(filename: &str) -> Option<String> {
    let ext = sanitize_extension(filename)?;

    let mime = match ext.as_str() {
        // Documents
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "xml" => "application/xml",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        // Images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        // Audio & Video
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        // Archives
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "7z" => "application/x-7z-compressed",
        _ => "application/octet-stream",
    };

    Some(mime.to_string())
}

/// Sanitizes a user-facing original filename for safe storage in metadata (Phase 7, Tasks 6 & 15):
/// - Strips directory traversal or path prefixes across both POSIX (`/`) and Windows (`\`) separators.
/// - Strips dangerous control characters (\0, \r, \n) while preserving all Unicode (Hindi, CJK, accents, emojis, spaces).
/// - Safely bounds unusually long filenames to 255 characters while preserving the extension.
/// - Falls back to "attachment" if the resulting name is blank.
pub fn sanitize_original_filename(raw_name: &str) -> String {
    let trimmed = raw_name.trim();
    if trimmed.is_empty() {
        return "attachment".to_string();
    }

    // Strip path prefixes across both POSIX (/) and Windows (\) conventions
    let base_name = trimmed
        .rsplit('/')
        .next()
        .unwrap_or(trimmed)
        .rsplit('\\')
        .next()
        .unwrap_or(trimmed);

    let sanitized: String = base_name
        .chars()
        .filter(|c| !c.is_control())
        .collect();

    let final_name = sanitized.trim();
    if final_name.is_empty() {
        return "attachment".to_string();
    }

    // Safely bound unusually long filenames (POSIX/Windows 255 char boundary), preserving extension
    if final_name.chars().count() > 255 {
        if let Some(dot_idx) = final_name.rfind('.') {
            let ext = &final_name[dot_idx..];
            let ext_char_count = ext.chars().count();
            if ext_char_count < 255 {
                let max_stem_chars = 255 - ext_char_count;
                let stem: String = final_name[..dot_idx].chars().take(max_stem_chars).collect();
                return format!("{stem}{ext}");
            }
        }
        final_name.chars().take(255).collect()
    } else {
        final_name.to_string()
    }
}

/// Detects MIME type safely combining magic bytes header inspection and extension inference (Phase 7, Task 6).
pub fn detect_mime_type_with_magic(filename: &str, header: &[u8]) -> Option<String> {
    // 1. Magic byte sniffing
    if header.starts_with(b"%PDF-") {
        return Some("application/pdf".to_string());
    }
    if header.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png".to_string());
    }
    if header.starts_with(b"\xFF\xD8\xFF") {
        return Some("image/jpeg".to_string());
    }
    if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        return Some("image/gif".to_string());
    }
    if header.len() >= 12 && &header[0..4] == b"RIFF" && &header[8..12] == b"WEBP" {
        return Some("image/webp".to_string());
    }
    if header.starts_with(b"\x1f\x8b") {
        return Some("application/gzip".to_string());
    }

    // 2. Extension inference fallback
    detect_mime_type(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_storage() -> (AttachmentStorage, PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!("pn_att_store_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().expect("Ensure directories");
        let storage = AttachmentStorage::new(paths);
        (storage, temp_dir)
    }

    #[test]
    fn test_extension_sanitization_and_mime_detection() {
        assert_eq!(sanitize_extension("document.pdf"), Some("pdf".to_string()));
        assert_eq!(sanitize_extension("image.PNG"), Some("png".to_string()));
        assert_eq!(sanitize_extension("archive.tar.gz"), Some("gz".to_string()));
        assert_eq!(sanitize_extension("no_extension"), None);
        assert_eq!(sanitize_extension("trailing_dot."), None);
        assert_eq!(sanitize_extension("invalid_symbols.p@f"), None);
        assert_eq!(sanitize_extension("too_long.verylongextension123"), None);
        assert_eq!(sanitize_extension("space in.e xt"), None);

        assert_eq!(detect_mime_type("report.pdf"), Some("application/pdf".to_string()));
        assert_eq!(detect_mime_type("photo.jpg"), Some("image/jpeg".to_string()));
        assert_eq!(detect_mime_type("notes.md"), Some("text/markdown".to_string()));
        assert_eq!(detect_mime_type("binary.xyz123"), Some("application/octet-stream".to_string()));
        assert_eq!(detect_mime_type("unknown"), None);
    }

    #[test]
    fn test_store_file_atomic_copy_and_verification() {
        let (storage, temp_dir) = setup_test_storage();
        let note_id = Uuid::new_v4().to_string();

        // Create a dummy source file
        let src_path = temp_dir.join("sample_contract.pdf");
        let sample_data = b"Personal Notepad Attachment Test Content \xf0\x9f\x9a\x80";
        fs::write(&src_path, sample_data).expect("Write source file");

        // Store file
        let stored = storage
            .store_file(&note_id, &src_path)
            .expect("Store file should succeed");

        assert_eq!(stored.file_size, sample_data.len() as u64);
        assert_eq!(stored.original_name, "sample_contract.pdf");
        assert_eq!(stored.mime_type, Some("application/pdf".to_string()));
        assert!(stored.stored_name.ends_with(".pdf"));
        assert!(stored.relative_path.starts_with(&note_id));

        // Verify stored file exists in managed storage
        assert!(storage.file_exists(&stored.relative_path));

        let meta = storage
            .get_metadata(&stored.relative_path)
            .expect("Get metadata");
        assert_eq!(meta.len(), sample_data.len() as u64);

        // Verify stored content matches byte-for-byte
        let resolved = storage
            .resolve_safe_path(&stored.relative_path)
            .expect("Resolve safe path");
        let read_back = fs::read(resolved).expect("Read stored file");
        assert_eq!(read_back, sample_data);

        // Clean up stored file
        storage
            .remove_file(&stored.relative_path)
            .expect("Remove file should succeed");
        assert!(!storage.file_exists(&stored.relative_path));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_duplicate_original_filename_isolation() {
        let (storage, temp_dir) = setup_test_storage();
        let note_id = Uuid::new_v4().to_string();

        let src_path = temp_dir.join("duplicate.txt");
        fs::write(&src_path, b"Version 1").expect("Write src");

        let stored1 = storage.store_file(&note_id, &src_path).expect("Store 1");

        // Write different content to same source path and store again
        fs::write(&src_path, b"Version 2 - Completely Different").expect("Write src 2");
        let stored2 = storage.store_file(&note_id, &src_path).expect("Store 2");

        // Storage names must be collision-resistant and distinct
        assert_ne!(stored1.stored_name, stored2.stored_name);
        assert_ne!(stored1.relative_path, stored2.relative_path);

        // Both files must co-exist peacefully in managed storage
        assert!(storage.file_exists(&stored1.relative_path));
        assert!(storage.file_exists(&stored2.relative_path));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_oversized_file_rejection() {
        let (temp_storage, temp_dir) = setup_test_storage();
        // Storage with 1 KiB max limit
        let storage = AttachmentStorage::with_max_size(temp_storage.paths, 1024);
        let note_id = Uuid::new_v4().to_string();

        let src_path = temp_dir.join("large_data.bin");
        let large_bytes = vec![0u8; 2048]; // 2 KiB
        fs::write(&src_path, &large_bytes).expect("Write large src");

        let res = storage.store_file(&note_id, &src_path);
        match res {
            Err(AttachmentError::FileTooLarge { size_bytes, max_bytes }) => {
                assert_eq!(size_bytes, 2048);
                assert_eq!(max_bytes, 1024);
            }
            other => panic!("Expected FileTooLarge, got: {:?}", other),
        }

        // Verify no leftover partial files in note directory
        let note_dir = storage.get_note_dir(&note_id).expect("Get note dir");
        let count = fs::read_dir(note_dir).map(|r| r.count()).unwrap_or(0);
        assert_eq!(count, 0, "No partial or temporary files should remain");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_path_traversal_rejection() {
        let (storage, temp_dir) = setup_test_storage();

        // Note ID traversal
        assert!(storage.get_note_dir("../../etc").is_err());
        assert!(storage.get_note_dir("").is_err());
        assert!(storage.get_note_dir("folder/sub").is_err());

        // Relative path traversal
        assert!(storage.resolve_safe_path("../../../etc/passwd").is_err());
        assert!(storage.resolve_safe_path("/absolute/path").is_err());
        assert!(storage.resolve_safe_path("").is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_6_validate_source_file_checks() {
        let (storage, temp_dir) = setup_test_storage();

        // 1. Non-existent file rejection
        let non_existent = temp_dir.join("does_not_exist.txt");
        assert!(storage.validate_source_file(&non_existent).is_err());

        // 2. Directory rejection
        let test_sub_dir = temp_dir.join("some_folder");
        fs::create_dir_all(&test_sub_dir).expect("Create dir");
        match storage.validate_source_file(&test_sub_dir) {
            Err(AttachmentError::SourceUnavailable(msg)) => {
                assert!(msg.contains("not a regular file"));
            }
            other => panic!("Expected regular file rejection, got: {:?}", other),
        }

        // 3. Valid regular file
        let valid_file = temp_dir.join("valid_note.txt");
        fs::write(&valid_file, b"Simple text attachment").expect("Write valid");
        let validated = storage.validate_source_file(&valid_file).expect("Validate file");
        assert_eq!(validated.original_name, "valid_note.txt");
        assert_eq!(validated.file_size, 22);
        assert_eq!(validated.mime_type, Some("text/plain".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_6_sanitize_original_filename_unicode_and_controls() {
        // Unicode with Hindi and Emoji preserved
        let unicode_name = "नोट्स और दस्तावेज़ 🚀.pdf";
        assert_eq!(sanitize_original_filename(unicode_name), unicode_name);

        // Control characters stripped
        let control_chars = "evil\0file\r\nname.txt";
        assert_eq!(sanitize_original_filename(control_chars), "evilfilename.txt");

        // Path stripping
        let path_name = "/some/nested/path/to/my_file.png";
        assert_eq!(sanitize_original_filename(path_name), "my_file.png");

        // Blank/spaces fallback
        assert_eq!(sanitize_original_filename("   "), "attachment");
    }

    #[test]
    fn test_task_6_detect_mime_type_with_magic_bytes() {
        // PDF magic header
        let pdf_header = b"%PDF-1.7\r\n...";
        assert_eq!(
            detect_mime_type_with_magic("unknown_extension", pdf_header),
            Some("application/pdf".to_string())
        );

        // PNG magic header
        let png_header = b"\x89PNG\r\n\x1a\n\x00\x00";
        assert_eq!(
            detect_mime_type_with_magic("file_without_ext", png_header),
            Some("image/png".to_string())
        );

        // JPEG magic header
        let jpeg_header = b"\xFF\xD8\xFF\xE0\x00\x10JFIF";
        assert_eq!(
            detect_mime_type_with_magic("file", jpeg_header),
            Some("image/jpeg".to_string())
        );

        // Extension fallback when magic header is plain
        let plain_bytes = b"Just plain text file";
        assert_eq!(
            detect_mime_type_with_magic("notes.md", plain_bytes),
            Some("text/markdown".to_string())
        );

        // Unknown extension with non-matching header returns None (nullable)
        assert_eq!(
            detect_mime_type_with_magic("custom_binary", plain_bytes),
            None
        );
    }

    #[test]
    fn test_task_22_storage_comprehensive_tests() {
        let (storage, temp_dir) = setup_test_storage();
        let note_id = Uuid::new_v4().to_string();

        // 1. Copy binary content without modification (null bytes, high bytes, exact match)
        let binary_payload: Vec<u8> = (0..=255).cycle().take(4096).collect();
        let bin_src = temp_dir.join("binary_test.dat");
        fs::write(&bin_src, &binary_payload).expect("Write binary test file");

        let stored_bin = storage
            .store_file(&note_id, &bin_src)
            .expect("Store binary file");
        assert_eq!(stored_bin.file_size, binary_payload.len() as u64);

        let read_back_bin = fs::read(storage.resolve_safe_path(&stored_bin.relative_path).unwrap())
            .expect("Read stored binary");
        assert_eq!(read_back_bin, binary_payload);

        // 2. Unicode and spaces: original filename with spaces & Hindi & emojis
        let complex_unicode_name = "वार्षिक रिपोर्ट & Strategy 🚀 2026.pdf";
        let unicode_src = temp_dir.join(complex_unicode_name);
        fs::write(&unicode_src, b"%PDF-1.4 sample unicode content").expect("Write unicode src");

        let stored_u1 = storage
            .store_file(&note_id, &unicode_src)
            .expect("Store unicode file 1");
        assert_eq!(stored_u1.original_name, complex_unicode_name);
        assert!(stored_u1.stored_name.ends_with(".pdf"));

        // Store duplicate original name again: must produce distinct storage names
        let stored_u2 = storage
            .store_file(&note_id, &unicode_src)
            .expect("Store unicode file 2");
        assert_eq!(stored_u2.original_name, complex_unicode_name);
        assert_ne!(stored_u1.stored_name, stored_u2.stored_name);
        assert_ne!(stored_u1.relative_path, stored_u2.relative_path);
        assert!(storage.file_exists(&stored_u1.relative_path));
        assert!(storage.file_exists(&stored_u2.relative_path));

        // 3. Unreadable source file permission handling (Unix test)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let unreadable_src = temp_dir.join("unreadable.txt");
            fs::write(&unreadable_src, b"classified").unwrap();
            let mut perms = fs::metadata(&unreadable_src).unwrap().permissions();
            perms.set_mode(0o000);
            let _ = fs::set_permissions(&unreadable_src, perms);

            let res = storage.store_file(&note_id, &unreadable_src);
            // Restore permissions so cleanup works
            let mut restore_perms = fs::metadata(&unreadable_src).unwrap().permissions();
            restore_perms.set_mode(0o644);
            let _ = fs::set_permissions(&unreadable_src, restore_perms);

            match res {
                Err(AttachmentError::PermissionDenied(_)) => {}
                other => panic!("Expected PermissionDenied, got: {other:?}"),
            }
        }

        // 4. Missing source file handling
        let missing_src = temp_dir.join("non_existent_file.png");
        let res_missing = storage.store_file(&note_id, &missing_src);
        match res_missing {
            Err(AttachmentError::SourceUnavailable(_)) => {}
            other => panic!("Expected SourceUnavailable, got: {other:?}"),
        }

        // 5. Clean up partial copies when operation fails
        let small_storage = AttachmentStorage::with_max_size(storage.paths.clone(), 500);
        let oversized_src = temp_dir.join("oversized.bin");
        fs::write(&oversized_src, vec![0u8; 1000]).unwrap();
        let res_over = small_storage.store_file(&note_id, &oversized_src);
        assert!(res_over.is_err());

        // Verify zero temp files remaining in note directory
        let note_dir = storage.get_note_dir(&note_id).unwrap();
        let entries = fs::read_dir(&note_dir).unwrap();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.contains(".tmp."), "Temporary file was not cleaned up: {name}");
        }

        // 6. Refuse paths that attempt to escape managed directory
        assert!(storage.resolve_safe_path("../secret.txt").is_err());
        assert!(storage.resolve_safe_path("note/../../etc/shadow").is_err());
        assert!(storage.resolve_safe_path("/absolute/path").is_err());
        assert!(storage.get_note_dir("..").is_err());
        assert!(storage.get_note_dir("../note").is_err());
        assert!(storage.get_note_dir("folder/sub").is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

