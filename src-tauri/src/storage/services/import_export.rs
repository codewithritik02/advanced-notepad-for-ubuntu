use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::storage::crypto::SecretBytes;
use crate::storage::errors::{ImportExportError, StorageError};
#[cfg(test)]
use crate::storage::models::{CreateAttachmentDto, ImportCandidateOverride, ImportOptions};
use crate::storage::models::{
    CreateNoteDto, CreateNotebookDto, DuplicateMatch, DuplicatePolicy, ExportBatchRequest,
    ExportFileResult, ExportFormat, ExportItemStatus, ExportNoteMetadataItem, ExportProgress,
    ExportScope, ExportSelectionPreview, ExportSingleNoteRequest, ExportSummary, ImportCandidate,
    ImportFileResult, ImportFormat, ImportItemStatus, ImportPreview, ImportProgress, ImportRequest,
    ImportSummary, InterchangeDocument, InterchangeInternalLink, InterchangeNote, Note,
    ValidationStatus,
};
use crate::storage::repositories::{
    AttachmentRepository, NoteRepository, NotebookRepository, SearchRepository, TagRepository,
};
use crate::storage::services::links::LinkParser;

fn cancelled_set() -> &'static Mutex<HashSet<String>> {
    static CANCELLED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    CANCELLED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Maximum allowed file size for an individual imported file (10 MB per specification)
pub const MAX_INDIVIDUAL_FILE_SIZE: u64 = crate::storage::limits::MAX_IMPORT_FILE_SIZE_BYTES;

/// Maximum aggregate byte size for an entire batch of imports (100 MB per specification)
pub const MAX_BATCH_AGGREGATE_SIZE: u64 = crate::storage::limits::MAX_IMPORT_BATCH_SIZE_BYTES;

/// Maximum number of files processed in a single batch operation
pub const MAX_BATCH_FILE_COUNT: usize = crate::storage::limits::MAX_BATCH_FILE_COUNT;

/// Maximum notebook hierarchy nesting depth supported during structured import
pub const MAX_NOTEBOOK_NESTING_DEPTH: usize = crate::storage::limits::MAX_NOTEBOOK_NESTING_DEPTH;

/// Container for verified, safe-to-import file content
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedFileContent {
    pub text: String,
    pub format: ImportFormat,
    pub is_empty: bool,
    pub interchange_doc: Option<InterchangeDocument>,
}

pub struct ImportExportService;

impl ImportExportService {
    /// Cancels an in-flight operation by registering its operation_id.
    pub fn cancel_operation(operation_id: &str) -> bool {
        if let Ok(mut set) = cancelled_set().lock() {
            set.insert(operation_id.to_string());
            true
        } else {
            false
        }
    }

    /// Checks whether an operation has received a cancellation request.
    pub fn is_operation_cancelled(operation_id: Option<&str>) -> bool {
        if let Some(id) = operation_id {
            if let Ok(set) = cancelled_set().lock() {
                return set.contains(id);
            }
        }
        false
    }

    /// Clears any registered cancellation marker for an operation once finalized.
    pub fn clear_operation(operation_id: Option<&str>) {
        if let Some(id) = operation_id {
            if let Ok(mut set) = cancelled_set().lock() {
                set.remove(id);
            }
        }
    }

    /// Detects the expected note format from a file path based on its extension.
    pub fn detect_format_from_path(path: &Path) -> Option<ImportFormat> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "txt" => Some(ImportFormat::Txt),
            "md" | "markdown" => Some(ImportFormat::Md),
            "json" => Some(ImportFormat::Json),
            _ => None,
        }
    }

    /// Sanitizes an arbitrary title string by stripping control characters and trimming whitespace,
    /// falling back to "Untitled Note" if the result is empty.
    pub fn sanitize_title(raw_title: &str) -> String {
        let cleaned: String = raw_title.chars().filter(|c| !c.is_control()).collect();

        let final_title = cleaned.trim();
        if final_title.is_empty() || final_title == "." {
            "Untitled Note".to_string()
        } else {
            final_title.to_string()
        }
    }

    /// Derives an initial note title from a filename, stripping recognized extensions and trimming whitespace.
    pub fn derive_proposed_title(file_name: &str) -> String {
        let trimmed = file_name.trim();
        if trimmed.is_empty() {
            return "Untitled Note".to_string();
        }

        let path = Path::new(trimmed);
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => {
                let lower = s.to_lowercase();
                if lower == ".txt"
                    || lower == ".md"
                    || lower == ".markdown"
                    || lower == ".json"
                    || lower == "txt"
                    || lower == "md"
                    || lower == "markdown"
                    || lower == "json"
                {
                    if trimmed.starts_with('.') && path.extension().is_none() {
                        ""
                    } else {
                        s
                    }
                } else {
                    s
                }
            }
            None => trimmed,
        };

        Self::sanitize_title(stem)
    }

    /// Strips UTF-8 Byte Order Mark (0xEF, 0xBB, 0xBF) if present at the start of bytes.
    pub fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &bytes[3..]
        } else {
            bytes
        }
    }

    /// Normalizes Windows CRLF line endings to standard Unix LF without altering other whitespace.
    pub fn normalize_line_endings(text: &str) -> String {
        if text.contains("\r\n") {
            text.replace("\r\n", "\n")
        } else {
            text.to_string()
        }
    }

    /// Scans a buffer to detect invalid binary content (e.g. null bytes).
    pub fn is_binary_content(sample_bytes: &[u8]) -> bool {
        sample_bytes.contains(&0)
    }

    /// Resolves an existing notebook by slash-delimited path (e.g. "Work/Projects")
    /// or creates the missing hierarchy levels idempotently (capped at 3 levels deep).
    pub fn resolve_or_create_notebook_path(
        conn: &Connection,
        path_str: &str,
    ) -> Result<String, StorageError> {
        let raw_segments: Vec<&str> = path_str
            .split('/')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if raw_segments.is_empty() {
            return Err(StorageError::Validation("Empty notebook path".to_string()));
        }

        let segments = if raw_segments.len() > MAX_NOTEBOOK_NESTING_DEPTH {
            &raw_segments[0..MAX_NOTEBOOK_NESTING_DEPTH]
        } else {
            &raw_segments[..]
        };

        let mut current_parent_id: Option<String> = None;

        for segment in segments {
            let name = Self::sanitize_title(segment);
            let existing_id: Option<String> = if let Some(ref pid) = current_parent_id {
                let mut stmt =
                    conn.prepare("SELECT id FROM notebooks WHERE name = ?1 AND parent_id = ?2")?;
                let mut rows = stmt.query(params![name, pid])?;
                if let Some(row) = rows.next()? {
                    Some(row.get(0)?)
                } else {
                    None
                }
            } else {
                let mut stmt =
                    conn.prepare("SELECT id FROM notebooks WHERE name = ?1 AND parent_id IS NULL")?;
                let mut rows = stmt.query(params![name])?;
                if let Some(row) = rows.next()? {
                    Some(row.get(0)?)
                } else {
                    None
                }
            };

            if let Some(id) = existing_id {
                current_parent_id = Some(id);
            } else {
                let created = NotebookRepository::create(
                    conn,
                    CreateNotebookDto {
                        name,
                        parent_id: current_parent_id,
                    },
                )?;
                current_parent_id = Some(created.id);
            }
        }

        current_parent_id
            .ok_or_else(|| StorageError::Validation("Failed to resolve notebook path".to_string()))
    }

    /// Inspects and expands paths submitted from the file picker.
    /// Safely handles directories (depth 1), single files, and multiple files.
    pub fn expand_and_validate_paths(raw_paths: &[String]) -> Vec<PathBuf> {
        let mut resolved: Vec<PathBuf> = Vec::new();

        for raw in raw_paths {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }

            let path = PathBuf::from(trimmed);
            if !path.exists() {
                // Keep the path so scanner can generate an explicit invalid candidate
                resolved.push(path);
                continue;
            }

            if path.is_dir() {
                // Safely enumerate directory contents without recursive traversal
                if let Ok(entries) = std::fs::read_dir(&path) {
                    for entry in entries.flatten() {
                        let child_path = entry.path();
                        if child_path.is_file() {
                            if let Some(file_name) = child_path.file_name().and_then(|n| n.to_str())
                            {
                                if !file_name.starts_with('.')
                                    && Self::detect_format_from_path(&child_path).is_some()
                                {
                                    resolved.push(child_path);
                                    if resolved.len() >= MAX_BATCH_FILE_COUNT {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                resolved.push(path);
            }

            if resolved.len() >= MAX_BATCH_FILE_COUNT {
                break;
            }
        }

        resolved
    }

    /// Fully reads and validates a file's content according to format policies, encoding rules, and size limits.
    pub fn read_and_validate_file(
        path: &Path,
        expected_format: ImportFormat,
    ) -> Result<ValidatedFileContent, ImportExportError> {
        if !path.exists() {
            return Err(ImportExportError::CannotReadFile(format!(
                "File '{}' does not exist or was moved.",
                path.display()
            )));
        }

        let metadata = std::fs::metadata(path).map_err(|e| {
            ImportExportError::CannotReadFile(format!(
                "Cannot read metadata for '{}': {e}",
                path.display()
            ))
        })?;

        if !metadata.is_file() {
            return Err(ImportExportError::CannotReadFile(format!(
                "Path '{}' is not a regular file (directories, devices, and special files cannot be imported directly).",
                path.display()
            )));
        }

        let file_size = metadata.len();
        if file_size > MAX_INDIVIDUAL_FILE_SIZE {
            return Err(ImportExportError::FileTooLarge {
                size_bytes: file_size,
                max_bytes: MAX_INDIVIDUAL_FILE_SIZE,
            });
        }

        let mut raw_bytes = Vec::new();
        let mut file = File::open(path).map_err(|e| {
            ImportExportError::CannotReadFile(format!("Cannot open file '{}': {e}", path.display()))
        })?;
        file.read_to_end(&mut raw_bytes).map_err(|e| {
            ImportExportError::CannotReadFile(format!(
                "Cannot read bytes from '{}': {e}",
                path.display()
            ))
        })?;

        // Empty file handling
        if raw_bytes.is_empty() {
            return Ok(ValidatedFileContent {
                text: String::new(),
                format: expected_format,
                is_empty: true,
                interchange_doc: None,
            });
        }

        // BOM stripping
        let bytes_without_bom = Self::strip_utf8_bom(&raw_bytes);

        // Binary content rejection
        if Self::is_binary_content(bytes_without_bom) {
            return Err(ImportExportError::Validation(
                "Binary file content detected. Only text files can be imported.".to_string(),
            ));
        }

        // Strict UTF-8 validation
        let utf8_str = std::str::from_utf8(bytes_without_bom).map_err(|_| {
            ImportExportError::InvalidEncoding(
                "File contains invalid UTF-8 encoding and cannot be imported safely.".to_string(),
            )
        })?;

        // Normalize line endings
        let normalized = Self::normalize_line_endings(utf8_str);

        // If JSON interchange, validate schema
        let mut interchange_doc = None;
        if expected_format == ImportFormat::Json {
            let doc: InterchangeDocument = serde_json::from_str(&normalized).map_err(|e| {
                ImportExportError::Validation(format!("Malformed interchange JSON document: {e}"))
            })?;

            if doc.app != "personal-notepad" {
                return Err(ImportExportError::Validation(format!(
                    "Invalid interchange document: expected app 'personal-notepad', found '{}'.",
                    doc.app
                )));
            }

            if doc.version != "1.0" {
                return Err(ImportExportError::Validation(format!(
                    "Unsupported interchange version '{}'. Supported version is '1.0'.",
                    doc.version
                )));
            }

            if doc.notes.len() > MAX_BATCH_FILE_COUNT {
                return Err(ImportExportError::Validation(format!(
                    "Interchange document contains {} notes, exceeding max limit of {} notes.",
                    doc.notes.len(),
                    MAX_BATCH_FILE_COUNT
                )));
            }

            // Validate maximum notebook nesting depth
            for note in &doc.notes {
                if let Some(ref nb_path) = note.notebook_path {
                    let depth = nb_path.split('/').filter(|s| !s.trim().is_empty()).count();
                    if depth > MAX_NOTEBOOK_NESTING_DEPTH {
                        return Err(ImportExportError::Validation(format!(
                            "Interchange note '{}' specifies notebook path '{}' exceeding maximum nesting depth of {} levels.",
                            note.title, nb_path, MAX_NOTEBOOK_NESTING_DEPTH
                        )));
                    }
                }
            }

            interchange_doc = Some(doc);
        }

        Ok(ValidatedFileContent {
            text: normalized,
            format: expected_format,
            is_empty: false,
            interchange_doc,
        })
    }

    /// Checks an incoming note's title and content against existing active notes in SQLite.
    ///
    /// Distinguishes between:
    /// 1. `ExactMatch`: Both title and content match an active note.
    /// 2. `ContentMatch`: Identical content matches an active note under a different title.
    /// 3. `TitleCollisionOnly`: Identical title matches an active note with DIFFERENT content.
    /// 4. `None`: No collision with any active note.
    ///
    /// Invariant: Title equality alone does NOT prove notes are duplicates.
    pub fn check_duplicate(
        conn: &Connection,
        title: &str,
        content: &str,
    ) -> Result<DuplicateMatch, StorageError> {
        Self::check_duplicate_with_key(conn, title, content, None)
    }

    /// Checks for duplicate notes, correctly decrypting active notes when protected by encryption.
    pub fn check_duplicate_with_key(
        conn: &Connection,
        title: &str,
        content: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<DuplicateMatch, StorageError> {
        if SearchRepository::is_encryption_enabled(conn) {
            let Some(key) = dek else {
                return Err(StorageError::Crypto(
                    "Application is locked. Please unlock first.".to_string(),
                ));
            };
            let active_notes = NoteRepository::list_active_with_key(conn, Some(key))?;
            for n in &active_notes {
                if n.title == title && n.content == content {
                    return Ok(DuplicateMatch::ExactMatch {
                        existing_id: n.id.clone(),
                        existing_title: n.title.clone(),
                    });
                }
            }
            if !content.trim().is_empty() {
                for n in &active_notes {
                    if n.content == content {
                        return Ok(DuplicateMatch::ContentMatch {
                            existing_id: n.id.clone(),
                            existing_title: n.title.clone(),
                        });
                    }
                }
            }
            for n in &active_notes {
                if n.title == title {
                    return Ok(DuplicateMatch::TitleCollisionOnly {
                        existing_id: n.id.clone(),
                        existing_title: n.title.clone(),
                    });
                }
            }
            return Ok(DuplicateMatch::None);
        }

        // 1. Check for exact title and exact content match
        let mut exact_stmt = conn.prepare(
            "SELECT id, title FROM notes WHERE title = ?1 AND content = ?2 AND is_deleted = 0 LIMIT 1",
        )?;
        let mut exact_rows = exact_stmt.query(params![title, content])?;
        if let Some(row) = exact_rows.next()? {
            let existing_id: String = row.get(0)?;
            let existing_title: String = row.get(1)?;
            return Ok(DuplicateMatch::ExactMatch {
                existing_id,
                existing_title,
            });
        }

        // 2. Check for exact content match (if content is non-empty)
        if !content.trim().is_empty() {
            let mut content_stmt = conn.prepare(
                "SELECT id, title FROM notes WHERE content = ?1 AND is_deleted = 0 LIMIT 1",
            )?;
            let mut content_rows = content_stmt.query(params![content])?;
            if let Some(row) = content_rows.next()? {
                let existing_id: String = row.get(0)?;
                let existing_title: String = row.get(1)?;
                return Ok(DuplicateMatch::ContentMatch {
                    existing_id,
                    existing_title,
                });
            }
        }

        // 3. Check for title collision only (different content)
        let mut title_stmt = conn
            .prepare("SELECT id, title FROM notes WHERE title = ?1 AND is_deleted = 0 LIMIT 1")?;
        let mut title_rows = title_stmt.query(params![title])?;
        if let Some(row) = title_rows.next()? {
            let existing_id: String = row.get(0)?;
            let existing_title: String = row.get(1)?;
            return Ok(DuplicateMatch::TitleCollisionOnly {
                existing_id,
                existing_title,
            });
        }

        Ok(DuplicateMatch::None)
    }

    /// Evaluates duplicate policy and user selection to determine if an import candidate should be skipped.
    ///
    /// Invariant: Title equality alone does NOT cause a note to be skipped under `SkipExact`.
    /// Invariant: Existing notes are NEVER overwritten.
    pub fn should_skip_candidate(
        policy: DuplicatePolicy,
        dup_match: &DuplicateMatch,
        user_selected: bool,
    ) -> (bool, Option<String>) {
        if !user_selected {
            return (true, Some("Deselected by user.".to_string()));
        }

        match policy {
            DuplicatePolicy::ImportAsNew => (false, None),
            DuplicatePolicy::SkipExact => match dup_match {
                DuplicateMatch::ExactMatch { existing_title, .. } => (
                    true,
                    Some(format!(
                        "Skipped exact duplicate note matching active note '{existing_title}'."
                    )),
                ),
                DuplicateMatch::ContentMatch { existing_title, .. } => (
                    true,
                    Some(format!(
                        "Skipped duplicate content matching active note '{existing_title}'."
                    )),
                ),
                DuplicateMatch::TitleCollisionOnly { .. } => (false, None),
                DuplicateMatch::None => (false, None),
            },
            DuplicatePolicy::Ask => (false, None),
        }
    }

    /// Scans candidate files, evaluates validations, detects format, checks duplicate notes in SQLite,
    /// and constructs a comprehensive preview report for the user.
    pub fn scan_candidates(
        conn: &Connection,
        raw_paths: &[String],
    ) -> Result<ImportPreview, StorageError> {
        Self::scan_candidates_with_key(conn, raw_paths, None)
    }

    /// Scans candidate files, evaluating duplicate notes with active encryption key if protected.
    pub fn scan_candidates_with_key(
        conn: &Connection,
        raw_paths: &[String],
        dek: Option<&SecretBytes>,
    ) -> Result<ImportPreview, StorageError> {
        if SearchRepository::is_encryption_enabled(conn) && dek.is_none() {
            return Err(StorageError::Crypto(
                "Application is locked. Please unlock first.".to_string(),
            ));
        }

        // Safe cancellation handling: an empty selection returns an empty preview without mutating data
        if raw_paths.is_empty() {
            return Ok(ImportPreview {
                candidates: Vec::new(),
                total_files: 0,
                valid_files: 0,
                invalid_files: 0,
                duplicate_files: 0,
                total_size_bytes: 0,
            });
        }

        let paths = Self::expand_and_validate_paths(raw_paths);
        let mut candidates = Vec::new();
        let mut total_size_bytes: u64 = 0;
        let mut valid_count = 0;
        let mut invalid_count = 0;
        let mut duplicate_count = 0;

        for path in paths {
            let id = uuid::Uuid::new_v4().to_string();
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            let file_path = path.to_string_lossy().to_string();
            let mut proposed_title = Self::derive_proposed_title(&file_name);

            // 1. Check file existence
            if !path.exists() {
                invalid_count += 1;
                candidates.push(ImportCandidate {
                    id,
                    file_path,
                    file_name,
                    file_size: 0,
                    detected_format: ImportFormat::Txt,
                    proposed_title,
                    validation_status: ValidationStatus::Invalid,
                    warnings: Vec::new(),
                    error_message: Some(
                        "File does not exist or is no longer accessible.".to_string(),
                    ),
                    is_duplicate: false,
                    duplicate_note_id: None,
                    duplicate_note_title: None,
                    selected: false,
                });
                continue;
            }

            // 2. Check file metadata and permissions
            let metadata = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    invalid_count += 1;
                    candidates.push(ImportCandidate {
                        id,
                        file_path,
                        file_name,
                        file_size: 0,
                        detected_format: ImportFormat::Txt,
                        proposed_title,
                        validation_status: ValidationStatus::Invalid,
                        warnings: Vec::new(),
                        error_message: Some(format!("Cannot read file permissions: {e}")),
                        is_duplicate: false,
                        duplicate_note_id: None,
                        duplicate_note_title: None,
                        selected: false,
                    });
                    continue;
                }
            };

            if !metadata.is_file() {
                invalid_count += 1;
                candidates.push(ImportCandidate {
                    id,
                    file_path,
                    file_name,
                    file_size: 0,
                    detected_format: ImportFormat::Txt,
                    proposed_title,
                    validation_status: ValidationStatus::Invalid,
                    warnings: Vec::new(),
                    error_message: Some("Path is not a regular file (directories, FIFOs, and special devices cannot be imported).".to_string()),
                    is_duplicate: false,
                    duplicate_note_id: None,
                    duplicate_note_title: None,
                    selected: false,
                });
                continue;
            }

            let file_size = metadata.len();
            total_size_bytes += file_size;

            // 3. Detect format by extension
            let detected_format = match Self::detect_format_from_path(&path) {
                Some(f) => f,
                None => {
                    invalid_count += 1;
                    candidates.push(ImportCandidate {
                        id,
                        file_path,
                        file_name,
                        file_size,
                        detected_format: ImportFormat::Txt,
                        proposed_title,
                        validation_status: ValidationStatus::Invalid,
                        warnings: Vec::new(),
                        error_message: Some(
                            "Unsupported file extension. Only .txt, .md, and .json are supported."
                                .to_string(),
                        ),
                        is_duplicate: false,
                        duplicate_note_id: None,
                        duplicate_note_title: None,
                        selected: false,
                    });
                    continue;
                }
            };

            // 4. Validate file size limits
            if file_size > MAX_INDIVIDUAL_FILE_SIZE {
                invalid_count += 1;
                candidates.push(ImportCandidate {
                    id,
                    file_path,
                    file_name,
                    file_size,
                    detected_format,
                    proposed_title,
                    validation_status: ValidationStatus::Invalid,
                    warnings: Vec::new(),
                    error_message: Some(format!(
                        "File exceeds 10 MB size limit ({:.2} MB).",
                        file_size as f64 / (1024.0 * 1024.0)
                    )),
                    is_duplicate: false,
                    duplicate_note_id: None,
                    duplicate_note_title: None,
                    selected: false,
                });
                continue;
            }

            // 5. Read and validate file content
            let mut warnings = Vec::new();
            let validated = match Self::read_and_validate_file(&path, detected_format) {
                Ok(v) => v,
                Err(e) => {
                    invalid_count += 1;
                    candidates.push(ImportCandidate {
                        id,
                        file_path,
                        file_name,
                        file_size,
                        detected_format,
                        proposed_title,
                        validation_status: ValidationStatus::Invalid,
                        warnings: Vec::new(),
                        error_message: Some(e.to_string()),
                        is_duplicate: false,
                        duplicate_note_id: None,
                        duplicate_note_title: None,
                        selected: false,
                    });
                    continue;
                }
            };

            if validated.is_empty {
                warnings
                    .push("File is empty (0 bytes). An empty note will be created.".to_string());
            }

            // 6. Check for duplicate note in SQLite
            // Distinguish between ExactMatch, ContentMatch, TitleCollisionOnly, and None.
            // Invariant: Title equality alone does not treat two notes as duplicates.
            let dup_match = if let Some(ref doc) = validated.interchange_doc {
                let total_omitted_att: usize = doc
                    .notes
                    .iter()
                    .filter_map(|n| n.attachments_omitted_count)
                    .sum();
                if total_omitted_att > 0 {
                    warnings.push(format!("Source package notes omit {total_omitted_att} attachment(s) not bundled in JSON interchange."));
                }
                let total_ext_links: usize = doc
                    .notes
                    .iter()
                    .map(|n| {
                        n.internal_links
                            .iter()
                            .filter(|l| !l.is_package_internal)
                            .count()
                    })
                    .sum();
                if total_ext_links > 0 {
                    warnings.push(format!(
                        "Package contains {total_ext_links} link(s) to notes outside this export."
                    ));
                }

                if doc.notes.len() == 1 {
                    proposed_title = Self::sanitize_title(&doc.notes[0].title);
                    Self::check_duplicate_with_key(
                        conn,
                        &proposed_title,
                        &doc.notes[0].content,
                        dek,
                    )?
                } else {
                    warnings.push(format!(
                        "Structured package containing {} note(s).",
                        doc.notes.len()
                    ));
                    DuplicateMatch::None
                }
            } else {
                Self::check_duplicate_with_key(conn, &proposed_title, &validated.text, dek)?
            };

            let (is_duplicate, duplicate_note_id, duplicate_note_title) = match dup_match {
                DuplicateMatch::ExactMatch {
                    existing_id,
                    existing_title,
                } => {
                    duplicate_count += 1;
                    warnings.push(format!("Exact duplicate note detected: identical to active note '{existing_title}'."));
                    (true, Some(existing_id), Some(existing_title))
                }
                DuplicateMatch::ContentMatch {
                    existing_id,
                    existing_title,
                } => {
                    duplicate_count += 1;
                    warnings.push(format!("Duplicate content detected: identical to active note '{existing_title}' with different title."));
                    (true, Some(existing_id), Some(existing_title))
                }
                DuplicateMatch::TitleCollisionOnly {
                    existing_id,
                    existing_title,
                } => {
                    // Crucial: Title equality alone is NOT an exact duplicate!
                    warnings.push(format!("Title collision: an existing note '{existing_title}' shares this title with different content. Both notes will be kept safely."));
                    (false, Some(existing_id), Some(existing_title))
                }
                DuplicateMatch::None => (false, None, None),
            };

            valid_count += 1;
            let validation_status = if !warnings.is_empty() {
                ValidationStatus::Warning
            } else {
                ValidationStatus::Valid
            };

            candidates.push(ImportCandidate {
                id,
                file_path,
                file_name,
                file_size,
                detected_format,
                proposed_title,
                validation_status,
                warnings,
                error_message: None,
                is_duplicate,
                duplicate_note_id,
                duplicate_note_title,
                selected: true,
            });
        }

        // Check aggregate batch limit
        if total_size_bytes > MAX_BATCH_AGGREGATE_SIZE {
            // Flag preview with aggregate size warning
            for c in &mut candidates {
                c.warnings.push(format!(
                    "Batch aggregate size ({:.2} MB) exceeds recommended 100 MB limit.",
                    total_size_bytes as f64 / (1024.0 * 1024.0)
                ));
            }
        }

        Ok(ImportPreview {
            total_files: candidates.len(),
            valid_files: valid_count,
            invalid_files: invalid_count,
            duplicate_files: duplicate_count,
            total_size_bytes,
            candidates,
        })
    }

    /// Executes the committing of imported notes within atomic per-file transactions.
    ///
    /// Requirements fulfilled:
    /// - Uses established `NoteRepository` and `TagRepository`.
    /// - Employs per-file transaction boundaries: a failure rolls back that file cleanly
    ///   without corrupting the database or interrupting other files in the batch.
    /// - Strictly preserves original text and Markdown syntax verbatim.
    /// - Enforces chosen `DuplicatePolicy` and never silently overwrites existing notes.
    /// - Validates optional target notebook existence before assignment.
    /// - Generates persistent UUID v4 identifiers.
    /// - Returns a comprehensive `ImportSummary` detailing results for all candidate files.
    pub fn execute_import(
        conn: &mut Connection,
        request: ImportRequest,
    ) -> Result<ImportSummary, StorageError> {
        Self::execute_import_with_progress_and_key(conn, request, None, |_| {})
    }

    /// Executes the committing of imported notes with an active encryption key if protected.
    pub fn execute_import_with_key(
        conn: &mut Connection,
        request: ImportRequest,
        dek: Option<&SecretBytes>,
    ) -> Result<ImportSummary, StorageError> {
        Self::execute_import_with_progress_and_key(conn, request, dek, |_| {})
    }

    /// Executes the committing of imported notes within atomic per-file transactions with progress emission and cancellation checks.
    pub fn execute_import_with_progress<F>(
        conn: &mut Connection,
        request: ImportRequest,
        on_progress: F,
    ) -> Result<ImportSummary, StorageError>
    where
        F: FnMut(&ImportProgress),
    {
        Self::execute_import_with_progress_and_key(conn, request, None, on_progress)
    }

    /// Executes the committing of imported notes within atomic per-file transactions with encryption support, progress emission, and cancellation checks.
    pub fn execute_import_with_progress_and_key<F>(
        conn: &mut Connection,
        request: ImportRequest,
        dek: Option<&SecretBytes>,
        mut on_progress: F,
    ) -> Result<ImportSummary, StorageError>
    where
        F: FnMut(&ImportProgress),
    {
        if SearchRepository::is_encryption_enabled(conn) && dek.is_none() {
            return Err(StorageError::Crypto(
                "Application is locked. Please unlock first.".to_string(),
            ));
        }

        let start_time = std::time::Instant::now();
        let mut results = Vec::new();
        let mut imported_count = 0;
        let mut skipped_count = 0;
        let mut failed_count = 0;
        let mut cancelled_count = 0;

        let operation_id = request.operation_id.clone();
        let total = request.candidates.len();

        if total > MAX_BATCH_FILE_COUNT {
            Self::clear_operation(operation_id.as_deref());
            return Err(StorageError::Validation(format!(
                "Import batch item count ({total}) exceeds maximum allowed limit of {MAX_BATCH_FILE_COUNT} files."
            )));
        }

        // 1. Validate target notebook existence if provided
        if let Some(ref target_nb_id) = request.options.target_notebook_id {
            if NotebookRepository::get_by_id(conn, target_nb_id)?.is_none() {
                Self::clear_operation(operation_id.as_deref());
                return Err(StorageError::Validation(format!(
                    "Target notebook '{target_nb_id}' does not exist or was deleted."
                )));
            }
        }

        // 2. Process each candidate file
        let mut is_cancelled = false;
        for (idx, candidate) in request.candidates.into_iter().enumerate() {
            let file_name = Path::new(&candidate.file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            // Check cancellation before processing next item
            if is_cancelled || Self::is_operation_cancelled(operation_id.as_deref()) {
                is_cancelled = true;
                results.push(ImportFileResult {
                    candidate_id: candidate.id,
                    file_name: file_name.clone(),
                    status: ImportItemStatus::Cancelled,
                    created_note_id: None,
                    message: Some("Import cancelled by user.".to_string()),
                });
                cancelled_count += 1;
                on_progress(&ImportProgress {
                    operation_id: operation_id.clone(),
                    total,
                    current_index: idx + 1,
                    current_file: file_name,
                    imported_count,
                    skipped_count,
                    failed_count,
                    cancelled_count,
                });
                continue;
            }

            // Emit progress on start of candidate processing
            on_progress(&ImportProgress {
                operation_id: operation_id.clone(),
                total,
                current_index: idx + 1,
                current_file: file_name.clone(),
                imported_count,
                skipped_count,
                failed_count,
                cancelled_count,
            });

            // A. Explicit user deselection
            if !candidate.selected {
                results.push(ImportFileResult {
                    candidate_id: candidate.id,
                    file_name: file_name.clone(),
                    status: ImportItemStatus::Skipped,
                    created_note_id: None,
                    message: Some("Deselected by user.".to_string()),
                });
                skipped_count += 1;
                on_progress(&ImportProgress {
                    operation_id: operation_id.clone(),
                    total,
                    current_index: idx + 1,
                    current_file: file_name,
                    imported_count,
                    skipped_count,
                    failed_count,
                    cancelled_count,
                });
                continue;
            }

            // B. Verify file path presence and accessibility
            let path = Path::new(&candidate.file_path);
            if !path.exists() {
                results.push(ImportFileResult {
                    candidate_id: candidate.id,
                    file_name: file_name.clone(),
                    status: ImportItemStatus::Failed,
                    created_note_id: None,
                    message: Some("File does not exist or is no longer accessible.".to_string()),
                });
                failed_count += 1;
                on_progress(&ImportProgress {
                    operation_id: operation_id.clone(),
                    total,
                    current_index: idx + 1,
                    current_file: file_name,
                    imported_count,
                    skipped_count,
                    failed_count,
                    cancelled_count,
                });
                continue;
            }

            // C. Detect format
            let detected_format = match Self::detect_format_from_path(path) {
                Some(f) => f,
                None => {
                    results.push(ImportFileResult {
                        candidate_id: candidate.id,
                        file_name: file_name.clone(),
                        status: ImportItemStatus::Rejected,
                        created_note_id: None,
                        message: Some("Unsupported file format.".to_string()),
                    });
                    failed_count += 1;
                    on_progress(&ImportProgress {
                        operation_id: operation_id.clone(),
                        total,
                        current_index: idx + 1,
                        current_file: file_name,
                        imported_count,
                        skipped_count,
                        failed_count,
                        cancelled_count,
                    });
                    continue;
                }
            };

            // D. Read, decode, and validate file content
            let validated = match Self::read_and_validate_file(path, detected_format) {
                Ok(v) => v,
                Err(e) => {
                    results.push(ImportFileResult {
                        candidate_id: candidate.id,
                        file_name: file_name.clone(),
                        status: ImportItemStatus::Rejected,
                        created_note_id: None,
                        message: Some(e.user_friendly_message()),
                    });
                    failed_count += 1;
                    on_progress(&ImportProgress {
                        operation_id: operation_id.clone(),
                        total,
                        current_index: idx + 1,
                        current_file: file_name,
                        imported_count,
                        skipped_count,
                        failed_count,
                        cancelled_count,
                    });
                    continue;
                }
            };

            // E. Process note creation depending on format
            if let Some(doc) = validated.interchange_doc {
                // Structured JSON package: contains 1 or more notes
                let tx_result = (|| -> Result<(Vec<String>, Vec<String>), StorageError> {
                    let tx = conn.transaction()?;
                    let mut created_ids = Vec::new();
                    let mut omitted_att_notices = Vec::new();

                    // Pre-generate unique local UUIDs for all notes in package to enable internal link remapping
                    let id_mapping: std::collections::HashMap<String, String> = doc
                        .notes
                        .iter()
                        .map(|n| (n.export_id.clone(), uuid::Uuid::new_v4().to_string()))
                        .collect();

                    for interchange_note in doc.notes {
                        let note_title = Self::sanitize_title(&interchange_note.title);
                        let note_format = if interchange_note.format.eq_ignore_ascii_case("md")
                            || interchange_note.format.eq_ignore_ascii_case("markdown")
                        {
                            "md".to_string()
                        } else {
                            "txt".to_string()
                        };

                        // Check duplicate under chosen policy
                        let dup_match = Self::check_duplicate_with_key(
                            &tx,
                            &note_title,
                            &interchange_note.content,
                            dek,
                        )?;
                        let (skip, _) = Self::should_skip_candidate(
                            request.options.duplicate_policy,
                            &dup_match,
                            true,
                        );
                        if skip {
                            continue;
                        }

                        // Determine destination notebook:
                        // 1. If user explicitly specified target_notebook_id in import options, use that.
                        // 2. Otherwise, if interchange note has a notebook_path (e.g. "Work/Projects"), resolve or create hierarchy.
                        // 3. Otherwise, unfiled (None).
                        let resolved_notebook_id = if request.options.target_notebook_id.is_some() {
                            request.options.target_notebook_id.clone()
                        } else if let Some(ref path_str) = interchange_note.notebook_path {
                            if !path_str.trim().is_empty() {
                                Self::resolve_or_create_notebook_path(&tx, path_str).ok()
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        // Rewrite internal links pointing to notes within this package
                        let mut content = interchange_note.content;
                        for (old_export_id, new_local_id) in &id_mapping {
                            let old_canonical = format!("notepad://note/{old_export_id}");
                            let new_canonical = format!("notepad://note/{new_local_id}");
                            content = content.replace(&old_canonical, &new_canonical);

                            let old_shorthand = format!("note:{old_export_id}");
                            let new_shorthand = format!("note:{new_local_id}");
                            content = content.replace(&old_shorthand, &new_shorthand);
                        }

                        if let Some(omitted_count) = interchange_note.attachments_omitted_count {
                            if omitted_count > 0 {
                                omitted_att_notices.push(format!(
                                    "Note '{}' originally had {} attachment(s) omitted from JSON package.",
                                    interchange_note.title, omitted_count
                                ));
                            }
                        }

                        // Tags creation / deduplication
                        let mut tag_ids = Vec::new();
                        for tag_name in &interchange_note.tags {
                            if let Ok(tag) = TagRepository::create(&tx, tag_name) {
                                tag_ids.push(tag.id);
                            }
                        }

                        let target_note_id = id_mapping
                            .get(&interchange_note.export_id)
                            .cloned()
                            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                        let now = chrono::Utc::now().to_rfc3339();
                        let created_at = interchange_note
                            .created_at
                            .as_ref()
                            .filter(|ts| chrono::DateTime::parse_from_rfc3339(ts).is_ok())
                            .cloned()
                            .unwrap_or_else(|| now.clone());
                        let modified_at = interchange_note
                            .modified_at
                            .as_ref()
                            .filter(|ts| chrono::DateTime::parse_from_rfc3339(ts).is_ok())
                            .cloned()
                            .unwrap_or_else(|| now.clone());

                        let stored_title = NoteRepository::encrypt_field_if_needed(
                            &note_title,
                            &target_note_id,
                            "title",
                            dek,
                        )?;
                        let stored_content = NoteRepository::encrypt_field_if_needed(
                            &content,
                            &target_note_id,
                            "content",
                            dek,
                        )?;

                        tx.execute(
                            "INSERT INTO notes (
                                id, title, content, format, notebook_id, created_at, modified_at,
                                is_favorite, is_pinned, is_deleted, deleted_at
                            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, NULL)",
                            params![
                                target_note_id,
                                stored_title,
                                stored_content,
                                note_format,
                                resolved_notebook_id,
                                created_at,
                                modified_at,
                                if interchange_note.is_favorite { 1 } else { 0 },
                                if interchange_note.is_pinned { 1 } else { 0 },
                            ],
                        )?;

                        for tag_id in &tag_ids {
                            tx.execute(
                                "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
                                params![target_note_id, tag_id],
                            )?;
                        }
                        created_ids.push(target_note_id);
                    }

                    tx.commit()?;
                    Ok((created_ids, omitted_att_notices))
                })();

                match tx_result {
                    Ok((ids, notices)) => {
                        if ids.is_empty() {
                            results.push(ImportFileResult {
                                candidate_id: candidate.id,
                                file_name: file_name.clone(),
                                status: ImportItemStatus::Skipped,
                                created_note_id: None,
                                message: Some(
                                    "All notes in structured package were skipped as duplicates."
                                        .to_string(),
                                ),
                            });
                            skipped_count += 1;
                        } else {
                            let msg = if notices.is_empty() {
                                None
                            } else {
                                Some(notices.join("; "))
                            };
                            results.push(ImportFileResult {
                                candidate_id: candidate.id,
                                file_name: file_name.clone(),
                                status: ImportItemStatus::Imported,
                                created_note_id: ids.first().cloned(),
                                message: msg,
                            });
                            imported_count += 1;
                        }
                    }
                    Err(e) => {
                        results.push(ImportFileResult {
                            candidate_id: candidate.id,
                            file_name: file_name.clone(),
                            status: ImportItemStatus::Failed,
                            created_note_id: None,
                            message: Some(format!("Database error during package import: {e}")),
                        });
                        failed_count += 1;
                    }
                }
                on_progress(&ImportProgress {
                    operation_id: operation_id.clone(),
                    total,
                    current_index: idx + 1,
                    current_file: file_name,
                    imported_count,
                    skipped_count,
                    failed_count,
                    cancelled_count,
                });
            } else {
                // Single TXT or Markdown note
                let final_title = candidate
                    .custom_title
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| Self::sanitize_title(&s))
                    .unwrap_or_else(|| {
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .map(Self::derive_proposed_title)
                            .unwrap_or_else(|| "Untitled Note".to_string())
                    });

                // Duplicate policy check
                let dup_match =
                    Self::check_duplicate_with_key(conn, &final_title, &validated.text, dek)?;
                let (skip, skip_reason) =
                    Self::should_skip_candidate(request.options.duplicate_policy, &dup_match, true);
                if skip {
                    results.push(ImportFileResult {
                        candidate_id: candidate.id,
                        file_name: file_name.clone(),
                        status: ImportItemStatus::Skipped,
                        created_note_id: None,
                        message: skip_reason,
                    });
                    skipped_count += 1;
                    on_progress(&ImportProgress {
                        operation_id: operation_id.clone(),
                        total,
                        current_index: idx + 1,
                        current_file: file_name,
                        imported_count,
                        skipped_count,
                        failed_count,
                        cancelled_count,
                    });
                    continue;
                }

                // Execute atomic note creation within its transaction
                let note_format = if detected_format == ImportFormat::Md {
                    "md".to_string()
                } else {
                    "txt".to_string()
                };

                let tx_result = (|| -> Result<Note, StorageError> {
                    let tx = conn.transaction()?;
                    let note = NoteRepository::create_with_key(
                        &tx,
                        CreateNoteDto {
                            title: Some(final_title.clone()),
                            content: Some(validated.text),
                            format: Some(note_format),
                            notebook_id: request.options.target_notebook_id.clone(),
                            is_favorite: Some(false),
                            is_pinned: Some(false),
                        },
                        dek,
                    )?;
                    tx.commit()?;
                    Ok(note)
                })();

                match tx_result {
                    Ok(created_note) => {
                        results.push(ImportFileResult {
                            candidate_id: candidate.id,
                            file_name: file_name.clone(),
                            status: ImportItemStatus::Imported,
                            created_note_id: Some(created_note.id),
                            message: None,
                        });
                        imported_count += 1;
                    }
                    Err(e) => {
                        results.push(ImportFileResult {
                            candidate_id: candidate.id,
                            file_name: file_name.clone(),
                            status: ImportItemStatus::Failed,
                            created_note_id: None,
                            message: Some(format!("Database insert failed: {e}")),
                        });
                        failed_count += 1;
                    }
                }
                on_progress(&ImportProgress {
                    operation_id: operation_id.clone(),
                    total,
                    current_index: idx + 1,
                    current_file: file_name,
                    imported_count,
                    skipped_count,
                    failed_count,
                    cancelled_count,
                });
            }
        }

        Self::clear_operation(operation_id.as_deref());

        let duration_ms = start_time.elapsed().as_millis() as u64;
        let total_processed = results.len();

        let mut summary = ImportSummary {
            total_processed,
            imported_count,
            skipped_count,
            failed_count,
            cancelled_count,
            outcome: None,
            results,
            duration_ms,
        };
        summary.outcome = Some(summary.calculate_outcome());

        Ok(summary)
    }

    /// Sanitizes a note title to produce a safe filesystem filename.
    /// Strips illegal filesystem characters and control characters, strips leading dots (preventing hidden files),
    /// strips trailing dots/spaces, defensively prefixes Windows reserved device names, and caps length at 180 chars.
    pub fn sanitize_filename(title: &str) -> String {
        let illegal = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        let cleaned: String = title
            .chars()
            .filter(|c| !c.is_control() && !illegal.contains(c))
            .collect();
        let trimmed = cleaned
            .trim()
            .trim_start_matches('.')
            .trim_end_matches(['.', ' ']);
        if trimmed.is_empty() {
            "Untitled Note".to_string()
        } else {
            let upper = trimmed.to_uppercase();
            let is_reserved = matches!(
                upper.as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            );
            let safe_name = if is_reserved {
                format!("_{trimmed}")
            } else {
                trimmed.to_string()
            };

            if safe_name.chars().count() > 180 {
                safe_name.chars().take(180).collect()
            } else {
                safe_name
            }
        }
    }

    /// Validates an export destination path against path traversal, empty paths,
    /// and ensures export destination is distinct from internal managed storage.
    pub fn validate_destination_path(path: &Path) -> Result<(), StorageError> {
        let path_str = path.to_string_lossy();
        if path_str.trim().is_empty() {
            return Err(StorageError::Validation(
                "Destination path cannot be empty".to_string(),
            ));
        }

        // Cross-platform check: handle Windows-style '\' as well as Unix '/'
        let normalized = path_str.replace('\\', "/");
        if normalized.split('/').any(|comp| comp == "..") {
            return Err(StorageError::Validation(
                "Path traversal ('..') is not permitted in export destinations".to_string(),
            ));
        }

        for component in path.components() {
            if let std::path::Component::ParentDir = component {
                return Err(StorageError::Validation(
                    "Path traversal ('..') is not permitted in export destinations".to_string(),
                ));
            }
        }

        if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
            if file_name.eq_ignore_ascii_case("database.sqlite")
                || file_name.eq_ignore_ascii_case("database.sqlite-wal")
                || file_name.eq_ignore_ascii_case("database.sqlite-shm")
            {
                return Err(StorageError::Validation(
                    "Destination cannot target application database files".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Safely writes content to `dest_path` using an atomic write pattern (Phase 10, Task 18):
    /// 1. Verifies that `dest_path` is not a directory.
    /// 2. Ensures parent directory exists and is accessible.
    /// 3. Writes payload to a hidden temporary file in the destination directory (`.tmp_<file_name>_<uuid>.part`).
    /// 4. Flushes and syncs data to disk before committing.
    /// 5. Atomically replaces `dest_path` using `std::fs::rename` (with safe fallback if atomic replace is unavailable).
    /// 6. Utilizes RAII cleanup guard to guarantee temporary files are removed on error, cancellation, or failure.
    /// 7. Never truncates or damages an existing file before replacement data is completely prepared and committed.
    pub fn write_file_atomically(dest_path: &Path, content: &[u8]) -> Result<(), StorageError> {
        if dest_path.is_dir() {
            return Err(StorageError::Validation(format!(
                "Destination path '{}' is a directory, not a file",
                dest_path.display()
            )));
        }

        let parent = dest_path.parent().unwrap_or_else(|| Path::new("."));
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }

        let file_name = dest_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("export");

        let temp_filename = format!(".tmp_{}_{}.part", file_name, uuid::Uuid::new_v4());
        let temp_path = parent.join(temp_filename);

        struct TempCleanup<'a> {
            path: &'a Path,
            committed: bool,
        }

        impl<'a> Drop for TempCleanup<'a> {
            fn drop(&mut self) {
                if !self.committed && self.path.exists() {
                    let _ = std::fs::remove_file(self.path);
                }
            }
        }

        let mut cleanup = TempCleanup {
            path: &temp_path,
            committed: false,
        };

        // Write content to temporary file
        {
            let mut file = std::fs::File::create(&temp_path).map_err(|e| {
                if let Some(28) = e.raw_os_error() {
                    StorageError::Io(std::io::Error::other(
                        "Insufficient disk space to complete export.",
                    ))
                } else {
                    StorageError::Io(e)
                }
            })?;
            file.write_all(content).map_err(|e| {
                if let Some(28) = e.raw_os_error() {
                    StorageError::Io(std::io::Error::other(
                        "Insufficient disk space to complete export.",
                    ))
                } else {
                    StorageError::Io(e)
                }
            })?;
            file.flush().map_err(|e| {
                if let Some(28) = e.raw_os_error() {
                    StorageError::Io(std::io::Error::other(
                        "Insufficient disk space to complete export.",
                    ))
                } else {
                    StorageError::Io(e)
                }
            })?;
            let _ = file.sync_all(); // best effort fsync to commit data to disk
        }

        // Atomically rename temporary file to destination path
        match std::fs::rename(&temp_path, dest_path) {
            Ok(_) => {
                cleanup.committed = true;
                Ok(())
            }
            Err(rename_err) => {
                // If rename failed (e.g. cross-filesystem or OS restriction), attempt safe copy fallback
                match std::fs::copy(&temp_path, dest_path) {
                    Ok(_) => {
                        let _ = std::fs::remove_file(&temp_path);
                        cleanup.committed = true;
                        Ok(())
                    }
                    Err(copy_err) => {
                        // Crucial: Clean up partially written dest_path so no corrupted destination file remains!
                        if dest_path.exists() {
                            let _ = std::fs::remove_file(dest_path);
                        }
                        if let Some(28) = copy_err.raw_os_error() {
                            Err(StorageError::Io(std::io::Error::other(
                                "Insufficient disk space to complete export.",
                            )))
                        } else {
                            Err(StorageError::Io(rename_err))
                        }
                    }
                }
            }
        }
    }

    /// Exports an individual note to a destination file chosen by the user.
    ///
    /// Requirements fulfilled:
    /// - Reads latest persisted note content.
    /// - Preserves intended format (verbatim UTF-8 for txt/md, structured v1.0 for JSON).
    /// - Never mutates source note.
    /// - Creates parent directory if missing.
    /// - Prevents destination collisions unless explicit overwrite is granted.
    /// - Validates destination against path traversal and internal managed files.
    pub fn export_single_note(
        conn: &Connection,
        request: ExportSingleNoteRequest,
    ) -> Result<ExportFileResult, StorageError> {
        Self::export_single_note_with_key(conn, request, None)
    }

    /// Exports an individual note to a destination file chosen by the user, with encryption support.
    pub fn export_single_note_with_key(
        conn: &Connection,
        request: ExportSingleNoteRequest,
        dek: Option<&SecretBytes>,
    ) -> Result<ExportFileResult, StorageError> {
        if SearchRepository::is_encryption_enabled(conn) && dek.is_none() {
            return Err(StorageError::Crypto(
                "Application is locked. Please unlock first.".to_string(),
            ));
        }

        let note =
            NoteRepository::get_by_id_with_key(conn, &request.note_id, dek)?.ok_or_else(|| {
                StorageError::NotFound(format!("Note with id '{}' not found", request.note_id))
            })?;

        let dest_path = Path::new(&request.destination_path);
        Self::validate_destination_path(dest_path)?;

        if dest_path.is_dir() {
            return Err(StorageError::Validation(format!(
                "Destination path '{}' is a directory, not a file",
                request.destination_path
            )));
        }

        if dest_path.exists() && !request.overwrite {
            return Err(StorageError::Conflict(format!(
                "Destination file '{}' already exists and overwrite was not permitted",
                request.destination_path
            )));
        }

        if let Some(parent) = dest_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let content_bytes: Vec<u8> = match request.format {
            ExportFormat::Txt | ExportFormat::Md => {
                // Export note content verbatim as UTF-8
                note.content.as_bytes().to_vec()
            }
            ExportFormat::Json => {
                // Structured Version 1.0 interchange document
                let mut tags: Vec<String> = TagRepository::get_tags_for_note(conn, &note.id)?
                    .into_iter()
                    .map(|t| t.name)
                    .collect();
                tags.sort(); // Deterministic sorting

                let notebook_path = if let Some(ref nb_id) = note.notebook_id {
                    NotebookRepository::get_hierarchy_path(conn, nb_id).ok()
                } else {
                    None
                };

                let att_count = AttachmentRepository::count_by_note_id(conn, &note.id).unwrap_or(0);
                let attachments_omitted_count = if att_count > 0 { Some(att_count) } else { None };

                let mut internal_links = Vec::new();
                let parsed_links = LinkParser::parse_internal_links(&note.content);
                for pl in parsed_links {
                    let is_internal = pl.target_id == note.id;
                    internal_links.push(InterchangeInternalLink {
                        label: pl.label,
                        target_export_id: pl.target_id,
                        is_package_internal: is_internal,
                    });
                }

                let interchange_note = InterchangeNote {
                    export_id: note.id.clone(),
                    title: note.title.clone(),
                    content: note.content.clone(),
                    format: note.format.clone(),
                    notebook_path,
                    tags,
                    is_favorite: note.is_favorite,
                    is_pinned: note.is_pinned,
                    created_at: Some(note.created_at.clone()),
                    modified_at: Some(note.modified_at.clone()),
                    attachments_omitted_count,
                    internal_links,
                };

                let doc = InterchangeDocument {
                    schema: Some("https://personal-notepad.local/schemas/interchange-v1.json".to_string()),
                    app: "personal-notepad".to_string(),
                    format: "personal-notepad-interchange".to_string(),
                    version: "1.0".to_string(),
                    exported_at: chrono::Utc::now().to_rfc3339(),
                    note_count: 1,
                    internal_link_policy: Some(
                        "Package-scoped internal links map export_id to new local IDs upon import; external or unexported note links remain unmapped as broken/missing references."
                            .to_string(),
                    ),
                    attachment_policy: Some(
                        "Note attachments are omitted from portable JSON interchange documents. Attachment metadata counts are tracked but binaries are not bundled."
                            .to_string(),
                    ),
                    notes: vec![interchange_note],
                };

                let json_bytes = serde_json::to_vec_pretty(&doc).map_err(|e| {
                    StorageError::Validation(format!("Failed to serialize note to JSON: {e}"))
                })?;

                // Roundtrip validation before committing to disk
                serde_json::from_slice::<InterchangeDocument>(&json_bytes).map_err(|e| {
                    StorageError::Validation(format!(
                        "Generated interchange JSON failed roundtrip validation: {e}"
                    ))
                })?;

                json_bytes
            }
        };

        Self::write_file_atomically(dest_path, &content_bytes)?;

        Ok(ExportFileResult {
            note_id: note.id,
            note_title: note.title,
            file_path: request.destination_path,
            status: ExportItemStatus::Exported,
            message: None,
        })
    }

    /// Recursively retrieves all descendant notebook IDs (including root_id)
    pub fn get_descendant_notebook_ids(
        conn: &Connection,
        root_id: &str,
    ) -> Result<Vec<String>, StorageError> {
        let all = NotebookRepository::list(conn)?;
        let mut descendants = Vec::new();
        let mut queue = vec![root_id.to_string()];
        descendants.push(root_id.to_string());
        let mut visited = std::collections::HashSet::new();
        visited.insert(root_id.to_string());

        while let Some(current) = queue.pop() {
            for nb in &all {
                if let Some(ref pid) = nb.parent_id {
                    if pid == &current && !visited.contains(&nb.id) {
                        visited.insert(nb.id.clone());
                        descendants.push(nb.id.clone());
                        queue.push(nb.id.clone());
                    }
                }
            }
        }
        Ok(descendants)
    }

    /// Resolves an export selection scope, validating note IDs, filtering out trashed notes,
    /// avoiding loading large full content into memory/preview, and reporting accurate counts.
    pub fn resolve_export_selection(
        conn: &Connection,
        scope: &ExportScope,
    ) -> Result<ExportSelectionPreview, StorageError> {
        Self::resolve_export_selection_with_key(conn, scope, None)
    }

    /// Resolves an export selection scope, validating note IDs, filtering out trashed notes,
    /// avoiding loading large full content into memory/preview, and reporting accurate counts.
    /// Safely decrypts metadata when protected by encryption.
    pub fn resolve_export_selection_with_key(
        conn: &Connection,
        scope: &ExportScope,
        dek: Option<&SecretBytes>,
    ) -> Result<ExportSelectionPreview, StorageError> {
        if SearchRepository::is_encryption_enabled(conn) && dek.is_none() {
            return Err(StorageError::Crypto(
                "Application is locked. Please unlock first.".to_string(),
            ));
        }

        match scope {
            ExportScope::SingleNote { note_id } => {
                let note_opt = NoteRepository::get_by_id_with_key(conn, note_id, dek)?;
                match note_opt {
                    None => Ok(ExportSelectionPreview {
                        total_requested: 1,
                        total_included: 0,
                        skipped_trashed_count: 0,
                        skipped_missing_count: 1,
                        notes: Vec::new(),
                        included_notebook_count: 0,
                    }),
                    Some(note) => {
                        if note.is_deleted {
                            Ok(ExportSelectionPreview {
                                total_requested: 1,
                                total_included: 0,
                                skipped_trashed_count: 1,
                                skipped_missing_count: 0,
                                notes: Vec::new(),
                                included_notebook_count: 0,
                            })
                        } else {
                            let tag_count = TagRepository::get_tags_for_note(conn, &note.id)?.len();
                            let notebook_path = if let Some(ref nb_id) = note.notebook_id {
                                NotebookRepository::get_hierarchy_path(conn, nb_id).ok()
                            } else {
                                None
                            };
                            let item = ExportNoteMetadataItem {
                                id: note.id,
                                title: note.title,
                                format: note.format,
                                notebook_id: note.notebook_id,
                                notebook_path,
                                is_favorite: note.is_favorite,
                                is_pinned: note.is_pinned,
                                tag_count,
                                content_length_bytes: note.content.len(),
                                modified_at: note.modified_at,
                            };
                            let included_notebook_count =
                                if item.notebook_id.is_some() { 1 } else { 0 };
                            Ok(ExportSelectionPreview {
                                total_requested: 1,
                                total_included: 1,
                                skipped_trashed_count: 0,
                                skipped_missing_count: 0,
                                notes: vec![item],
                                included_notebook_count,
                            })
                        }
                    }
                }
            }
            ExportScope::SelectedNotes { note_ids } => {
                let mut seen_ids = std::collections::HashSet::new();
                let mut unique_ids = Vec::new();
                for id in note_ids {
                    if seen_ids.insert(id.clone()) {
                        unique_ids.push(id.clone());
                    }
                }

                let total_requested = unique_ids.len();
                let mut skipped_trashed_count = 0;
                let mut skipped_missing_count = 0;
                let mut notes = Vec::new();
                let mut notebook_set = std::collections::HashSet::new();

                for id in unique_ids {
                    match NoteRepository::get_by_id_with_key(conn, &id, dek)? {
                        None => {
                            skipped_missing_count += 1;
                        }
                        Some(note) => {
                            if note.is_deleted {
                                skipped_trashed_count += 1;
                            } else {
                                if let Some(ref nb_id) = note.notebook_id {
                                    notebook_set.insert(nb_id.clone());
                                }
                                let tag_count =
                                    TagRepository::get_tags_for_note(conn, &note.id)?.len();
                                let notebook_path = if let Some(ref nb_id) = note.notebook_id {
                                    NotebookRepository::get_hierarchy_path(conn, nb_id).ok()
                                } else {
                                    None
                                };
                                notes.push(ExportNoteMetadataItem {
                                    id: note.id,
                                    title: note.title,
                                    format: note.format,
                                    notebook_id: note.notebook_id,
                                    notebook_path,
                                    is_favorite: note.is_favorite,
                                    is_pinned: note.is_pinned,
                                    tag_count,
                                    content_length_bytes: note.content.len(),
                                    modified_at: note.modified_at,
                                });
                            }
                        }
                    }
                }

                Ok(ExportSelectionPreview {
                    total_requested,
                    total_included: notes.len(),
                    skipped_trashed_count,
                    skipped_missing_count,
                    included_notebook_count: notebook_set.len(),
                    notes,
                })
            }
            ExportScope::Notebook {
                notebook_id,
                include_nested,
            } => {
                let _target_nb =
                    NotebookRepository::get_by_id(conn, notebook_id)?.ok_or_else(|| {
                        StorageError::NotFound(format!(
                            "Notebook with id '{notebook_id}' not found"
                        ))
                    })?;

                let target_notebook_ids = if *include_nested {
                    Self::get_descendant_notebook_ids(conn, notebook_id)?
                } else {
                    vec![notebook_id.clone()]
                };

                let mut notes = Vec::new();
                let mut skipped_trashed_count = 0;

                for nb_id in &target_notebook_ids {
                    let active_notes = NoteRepository::list_filtered_with_key(
                        conn,
                        false,
                        Some(nb_id),
                        false,
                        dek,
                    )?;
                    let trashed_in_nb: usize = conn
                        .query_row(
                            "SELECT COUNT(1) FROM notes WHERE notebook_id = ?1 AND is_deleted = 1",
                            params![nb_id],
                            |r| r.get::<_, i64>(0).map(|c| c as usize),
                        )
                        .unwrap_or(0);
                    skipped_trashed_count += trashed_in_nb;

                    for note in active_notes {
                        let tag_count = TagRepository::get_tags_for_note(conn, &note.id)?.len();
                        let notebook_path = if let Some(ref nid) = note.notebook_id {
                            NotebookRepository::get_hierarchy_path(conn, nid).ok()
                        } else {
                            None
                        };
                        notes.push(ExportNoteMetadataItem {
                            id: note.id,
                            title: note.title,
                            format: note.format,
                            notebook_id: note.notebook_id,
                            notebook_path,
                            is_favorite: note.is_favorite,
                            is_pinned: note.is_pinned,
                            tag_count,
                            content_length_bytes: note.content.len(),
                            modified_at: note.modified_at,
                        });
                    }
                }

                let total_included = notes.len();
                let total_requested = total_included + skipped_trashed_count;

                Ok(ExportSelectionPreview {
                    total_requested,
                    total_included,
                    skipped_trashed_count,
                    skipped_missing_count: 0,
                    notes,
                    included_notebook_count: target_notebook_ids.len(),
                })
            }
            ExportScope::AllNotes => {
                let active_notes = NoteRepository::list_active_with_key(conn, dek)?;
                let skipped_trashed_count: usize = conn
                    .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 1", [], |r| {
                        r.get::<_, i64>(0).map(|c| c as usize)
                    })
                    .unwrap_or(0);

                let mut notes = Vec::new();
                let mut notebook_set = std::collections::HashSet::new();

                for note in active_notes {
                    if let Some(ref nid) = note.notebook_id {
                        notebook_set.insert(nid.clone());
                    }
                    let tag_count = TagRepository::get_tags_for_note(conn, &note.id)?.len();
                    let notebook_path = if let Some(ref nid) = note.notebook_id {
                        NotebookRepository::get_hierarchy_path(conn, nid).ok()
                    } else {
                        None
                    };
                    notes.push(ExportNoteMetadataItem {
                        id: note.id,
                        title: note.title,
                        format: note.format,
                        notebook_id: note.notebook_id,
                        notebook_path,
                        is_favorite: note.is_favorite,
                        is_pinned: note.is_pinned,
                        tag_count,
                        content_length_bytes: note.content.len(),
                        modified_at: note.modified_at,
                    });
                }

                let total_included = notes.len();
                let total_requested = total_included + skipped_trashed_count;

                Ok(ExportSelectionPreview {
                    total_requested,
                    total_included,
                    skipped_trashed_count,
                    skipped_missing_count: 0,
                    notes,
                    included_notebook_count: notebook_set.len(),
                })
            }
        }
    }

    /// Executes a batch export for multiple notes or notebook selection according to format policies.
    pub fn export_notes_batch(
        conn: &Connection,
        request: ExportBatchRequest,
    ) -> Result<ExportSummary, StorageError> {
        Self::export_notes_batch_with_progress_and_key(conn, request, None, |_| {})
    }

    /// Executes a batch export for multiple notes or notebook selection with active encryption key.
    pub fn export_notes_batch_with_key(
        conn: &Connection,
        request: ExportBatchRequest,
        dek: Option<&SecretBytes>,
    ) -> Result<ExportSummary, StorageError> {
        Self::export_notes_batch_with_progress_and_key(conn, request, dek, |_| {})
    }

    /// Executes a batch export for multiple notes or notebook selection with progress emission and cancellation support.
    pub fn export_notes_batch_with_progress<F>(
        conn: &Connection,
        request: ExportBatchRequest,
        on_progress: F,
    ) -> Result<ExportSummary, StorageError>
    where
        F: FnMut(&ExportProgress),
    {
        Self::export_notes_batch_with_progress_and_key(conn, request, None, on_progress)
    }

    /// Executes a batch export for multiple notes or notebook selection with encryption support, progress emission, and cancellation support.
    pub fn export_notes_batch_with_progress_and_key<F>(
        conn: &Connection,
        request: ExportBatchRequest,
        dek: Option<&SecretBytes>,
        mut on_progress: F,
    ) -> Result<ExportSummary, StorageError>
    where
        F: FnMut(&ExportProgress),
    {
        if SearchRepository::is_encryption_enabled(conn) && dek.is_none() {
            return Err(StorageError::Crypto(
                "Application is locked. Please unlock first.".to_string(),
            ));
        }

        let start_time = std::time::Instant::now();
        let operation_id = request.operation_id.clone();

        // 1. Resolve scope
        let scope = request.scope.unwrap_or(ExportScope::SelectedNotes {
            note_ids: request.note_ids,
        });

        // 2. Resolve preview to obtain eligible note IDs and skip counts
        let preview = Self::resolve_export_selection_with_key(conn, &scope, dek)?;
        let total = preview.notes.len();

        if total > MAX_BATCH_FILE_COUNT {
            Self::clear_operation(operation_id.as_deref());
            return Err(StorageError::Validation(format!(
                "Export batch size ({total}) exceeds maximum allowed limit of {MAX_BATCH_FILE_COUNT} notes."
            )));
        }

        // 3. Process export based on format
        let mut results = Vec::new();
        let mut exported_count = 0;
        let mut failed_count = 0;
        let mut cancelled_count = 0;

        match request.format {
            ExportFormat::Json => {
                // Structured JSON: export all notes into a single InterchangeDocument
                let exported_ids: std::collections::HashSet<String> =
                    preview.notes.iter().map(|m| m.id.clone()).collect();

                let mut interchange_notes = Vec::new();
                for meta in &preview.notes {
                    if let Some(note) = NoteRepository::get_by_id_with_key(conn, &meta.id, dek)? {
                        let mut tags: Vec<String> =
                            TagRepository::get_tags_for_note(conn, &note.id)?
                                .into_iter()
                                .map(|t| t.name)
                                .collect();
                        tags.sort(); // Deterministic ordering

                        let att_count =
                            AttachmentRepository::count_by_note_id(conn, &note.id).unwrap_or(0);
                        let attachments_omitted_count =
                            if att_count > 0 { Some(att_count) } else { None };

                        let mut internal_links = Vec::new();
                        let parsed_links = LinkParser::parse_internal_links(&note.content);
                        for pl in parsed_links {
                            let is_internal = exported_ids.contains(&pl.target_id);
                            internal_links.push(InterchangeInternalLink {
                                label: pl.label,
                                target_export_id: pl.target_id,
                                is_package_internal: is_internal,
                            });
                        }

                        interchange_notes.push(InterchangeNote {
                            export_id: note.id.clone(),
                            title: note.title.clone(),
                            content: note.content.clone(),
                            format: note.format.clone(),
                            notebook_path: meta.notebook_path.clone(),
                            tags,
                            is_favorite: note.is_favorite,
                            is_pinned: note.is_pinned,
                            created_at: Some(note.created_at),
                            modified_at: Some(note.modified_at),
                            attachments_omitted_count,
                            internal_links,
                        });
                    }
                }

                let target_file_path = if let Some(ref dest_file) = request.destination_file {
                    PathBuf::from(dest_file)
                } else {
                    let dir = Path::new(&request.destination_dir);
                    if dir.extension().is_some_and(|ext| ext == "json") {
                        dir.to_path_buf()
                    } else {
                        dir.join("export.json")
                    }
                };

                Self::validate_destination_path(&target_file_path)?;

                if target_file_path.is_dir() {
                    return Err(StorageError::Validation(format!(
                        "Destination path '{}' is a directory, not a file",
                        target_file_path.display()
                    )));
                }

                let final_target_path = if target_file_path.exists() && !request.overwrite {
                    if request.destination_file.is_some() {
                        return Err(StorageError::Conflict(format!(
                            "Destination file '{}' already exists and overwrite was not permitted",
                            target_file_path.display()
                        )));
                    } else {
                        let parent = target_file_path.parent().unwrap_or_else(|| Path::new(""));
                        let mut counter = 1;
                        let mut candidate = parent.join(format!("export ({counter}).json"));
                        while candidate.exists() {
                            counter += 1;
                            candidate = parent.join(format!("export ({counter}).json"));
                        }
                        candidate
                    }
                } else {
                    target_file_path
                };

                if let Some(parent) = final_target_path.parent() {
                    if !parent.as_os_str().is_empty() && !parent.exists() {
                        std::fs::create_dir_all(parent)?;
                    }
                }

                let doc = InterchangeDocument {
                    schema: Some("https://personal-notepad.local/schemas/interchange-v1.json".to_string()),
                    app: "personal-notepad".to_string(),
                    format: "personal-notepad-interchange".to_string(),
                    version: "1.0".to_string(),
                    exported_at: chrono::Utc::now().to_rfc3339(),
                    note_count: interchange_notes.len(),
                    internal_link_policy: Some(
                        "Package-scoped internal links map export_id to new local IDs upon import; external or unexported note links remain unmapped as broken/missing references."
                            .to_string(),
                    ),
                    attachment_policy: Some(
                        "Note attachments are omitted from portable JSON interchange documents. Attachment metadata counts are tracked but binaries are not bundled."
                            .to_string(),
                    ),
                    notes: interchange_notes,
                };

                let json_bytes = serde_json::to_vec_pretty(&doc).map_err(|e| {
                    StorageError::Validation(format!(
                        "Failed to serialize notes batch to JSON: {e}"
                    ))
                })?;

                // Roundtrip validation before committing to disk
                serde_json::from_slice::<InterchangeDocument>(&json_bytes).map_err(|e| {
                    StorageError::Validation(format!(
                        "Generated interchange JSON failed roundtrip validation: {e}"
                    ))
                })?;

                if Self::is_operation_cancelled(operation_id.as_deref()) {
                    for (idx, meta) in preview.notes.iter().enumerate() {
                        results.push(ExportFileResult {
                            note_id: meta.id.clone(),
                            note_title: meta.title.clone(),
                            file_path: "".to_string(),
                            status: ExportItemStatus::Cancelled,
                            message: Some("Export cancelled by user.".to_string()),
                        });
                        cancelled_count += 1;
                        on_progress(&ExportProgress {
                            operation_id: operation_id.clone(),
                            total,
                            current_index: idx + 1,
                            current_title: meta.title.clone(),
                            exported_count,
                            skipped_count: preview.skipped_trashed_count
                                + preview.skipped_missing_count,
                            failed_count,
                            cancelled_count,
                        });
                    }
                } else {
                    Self::write_file_atomically(&final_target_path, &json_bytes)?;

                    let target_path_str = final_target_path.to_string_lossy().to_string();
                    for (idx, meta) in preview.notes.iter().enumerate() {
                        results.push(ExportFileResult {
                            note_id: meta.id.clone(),
                            note_title: meta.title.clone(),
                            file_path: target_path_str.clone(),
                            status: ExportItemStatus::Exported,
                            message: None,
                        });
                        exported_count += 1;
                        on_progress(&ExportProgress {
                            operation_id: operation_id.clone(),
                            total,
                            current_index: idx + 1,
                            current_title: meta.title.clone(),
                            exported_count,
                            skipped_count: preview.skipped_trashed_count
                                + preview.skipped_missing_count,
                            failed_count,
                            cancelled_count,
                        });
                    }
                }
            }
            ExportFormat::Txt | ExportFormat::Md => {
                let dest_dir = Path::new(&request.destination_dir);
                Self::validate_destination_path(dest_dir)?;

                if !dest_dir.exists() {
                    std::fs::create_dir_all(dest_dir)?;
                }

                let ext = if request.format == ExportFormat::Md {
                    "md"
                } else {
                    "txt"
                };
                let mut used_filenames = std::collections::HashSet::new();
                let skipped_init = preview.skipped_trashed_count + preview.skipped_missing_count;
                let mut is_cancelled = false;

                for (idx, meta) in preview.notes.iter().enumerate() {
                    if is_cancelled || Self::is_operation_cancelled(operation_id.as_deref()) {
                        is_cancelled = true;
                        results.push(ExportFileResult {
                            note_id: meta.id.clone(),
                            note_title: meta.title.clone(),
                            file_path: "".to_string(),
                            status: ExportItemStatus::Cancelled,
                            message: Some("Export cancelled by user.".to_string()),
                        });
                        cancelled_count += 1;
                        on_progress(&ExportProgress {
                            operation_id: operation_id.clone(),
                            total,
                            current_index: idx + 1,
                            current_title: meta.title.clone(),
                            exported_count,
                            skipped_count: skipped_init,
                            failed_count,
                            cancelled_count,
                        });
                        continue;
                    }

                    on_progress(&ExportProgress {
                        operation_id: operation_id.clone(),
                        total,
                        current_index: idx + 1,
                        current_title: meta.title.clone(),
                        exported_count,
                        skipped_count: skipped_init,
                        failed_count,
                        cancelled_count,
                    });

                    if let Some(note) = NoteRepository::get_by_id_with_key(conn, &meta.id, dek)? {
                        let base_stem = Self::sanitize_filename(&note.title);
                        let mut filename = format!("{base_stem}.{ext}");
                        let mut counter = 1;
                        while used_filenames.contains(&filename)
                            || (!request.overwrite && dest_dir.join(&filename).exists())
                        {
                            filename = format!("{base_stem} ({counter}).{ext}");
                            counter += 1;
                        }
                        used_filenames.insert(filename.clone());

                        let out_path = dest_dir.join(&filename);
                        match Self::write_file_atomically(&out_path, note.content.as_bytes()) {
                            Ok(_) => {
                                exported_count += 1;
                                results.push(ExportFileResult {
                                    note_id: note.id,
                                    note_title: note.title,
                                    file_path: out_path.to_string_lossy().to_string(),
                                    status: ExportItemStatus::Exported,
                                    message: None,
                                });
                            }
                            Err(e) => {
                                failed_count += 1;
                                results.push(ExportFileResult {
                                    note_id: note.id,
                                    note_title: note.title,
                                    file_path: out_path.to_string_lossy().to_string(),
                                    status: ExportItemStatus::Failed,
                                    message: Some(e.to_string()),
                                });
                            }
                        }
                    } else {
                        failed_count += 1;
                        results.push(ExportFileResult {
                            note_id: meta.id.clone(),
                            note_title: meta.title.clone(),
                            file_path: "".to_string(),
                            status: ExportItemStatus::Failed,
                            message: Some("Note could not be retrieved from database.".to_string()),
                        });
                    }

                    on_progress(&ExportProgress {
                        operation_id: operation_id.clone(),
                        total,
                        current_index: idx + 1,
                        current_title: meta.title.clone(),
                        exported_count,
                        skipped_count: skipped_init,
                        failed_count,
                        cancelled_count,
                    });
                }
            }
        }

        Self::clear_operation(operation_id.as_deref());

        let skipped_count = preview.skipped_trashed_count + preview.skipped_missing_count;

        let mut summary = ExportSummary {
            total_notes: preview.total_requested,
            exported_count,
            skipped_count,
            failed_count,
            cancelled_count,
            outcome: None,
            results,
            duration_ms: start_time.elapsed().as_millis() as u64,
        };
        summary.outcome = Some(summary.calculate_outcome());

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs::File;
    use std::io::Write;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE notes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                format TEXT NOT NULL DEFAULT 'txt',
                notebook_id TEXT,
                created_at TEXT NOT NULL,
                modified_at TEXT NOT NULL,
                is_favorite INTEGER NOT NULL DEFAULT 0,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                is_deleted INTEGER NOT NULL DEFAULT 0,
                deleted_at TEXT
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_strip_utf8_bom() {
        let with_bom = [0xEF, 0xBB, 0xBF, b'H', b'e', b'l', b'l', b'o'];
        assert_eq!(ImportExportService::strip_utf8_bom(&with_bom), b"Hello");

        let without_bom = b"Hello";
        assert_eq!(ImportExportService::strip_utf8_bom(without_bom), b"Hello");
    }

    #[test]
    fn test_normalize_line_endings_crlf_and_lf() {
        let crlf = "Line 1\r\nLine 2\r\n\r\nLine 3\r\n";
        let normalized = ImportExportService::normalize_line_endings(crlf);
        assert_eq!(normalized, "Line 1\nLine 2\n\nLine 3\n");
        assert!(!normalized.contains('\r'));

        let lf = "Line 1\nLine 2\n";
        assert_eq!(ImportExportService::normalize_line_endings(lf), lf);
    }

    #[test]
    fn test_read_and_validate_empty_and_very_short_files() {
        let dir = std::env::temp_dir().join(format!("test_ie_edge_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // 1. Empty file (0 bytes)
        let empty_path = dir.join("empty.txt");
        File::create(&empty_path).unwrap();
        let res_empty =
            ImportExportService::read_and_validate_file(&empty_path, ImportFormat::Txt).unwrap();
        assert!(res_empty.is_empty);
        assert_eq!(res_empty.text, "");

        // 2. Very short file (1 byte)
        let short_path = dir.join("short.md");
        let mut f_short = File::create(&short_path).unwrap();
        f_short.write_all(b"#").unwrap();
        let res_short =
            ImportExportService::read_and_validate_file(&short_path, ImportFormat::Md).unwrap();
        assert!(!res_short.is_empty);
        assert_eq!(res_short.text, "#");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_and_validate_unicode_emojis_and_long_lines() {
        let dir = std::env::temp_dir().join(format!("test_ie_uni_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // Unicode + Emojis in body and title
        let uni_path = dir.join("✨ Unicode Notes 🚀.md");
        let mut f_uni = File::create(&uni_path).unwrap();
        let uni_text =
            "# नमस्ते Duniya!\n\nThis note has emojis 🦀 🚀 ✨ and Japanese: こんにちは世界。\n";
        f_uni.write_all(uni_text.as_bytes()).unwrap();

        let res_uni =
            ImportExportService::read_and_validate_file(&uni_path, ImportFormat::Md).unwrap();
        assert_eq!(res_uni.text, uni_text);

        // Long single line (100,000 characters)
        let long_path = dir.join("long_line.txt");
        let mut f_long = File::create(&long_path).unwrap();
        let long_line: String = "A".repeat(100_000);
        f_long.write_all(long_line.as_bytes()).unwrap();

        let res_long =
            ImportExportService::read_and_validate_file(&long_path, ImportFormat::Txt).unwrap();
        assert_eq!(res_long.text.len(), 100_000);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_and_validate_utf8_bom() {
        let dir = std::env::temp_dir().join(format!("test_ie_bom_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let bom_path = dir.join("with_bom.txt");
        let mut f_bom = File::create(&bom_path).unwrap();
        let mut content = vec![0xEF, 0xBB, 0xBF];
        content.extend_from_slice(b"BOM content without pollution");
        f_bom.write_all(&content).unwrap();

        let res =
            ImportExportService::read_and_validate_file(&bom_path, ImportFormat::Txt).unwrap();
        assert_eq!(res.text, "BOM content without pollution");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_and_validate_invalid_utf8_rejected() {
        let dir = std::env::temp_dir().join(format!("test_ie_badutf_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let bad_path = dir.join("bad_utf8.txt");
        let mut f = File::create(&bad_path).unwrap();
        // Invalid UTF-8 sequence [0xFF, 0xFE]
        f.write_all(&[0xFF, 0xFE, 0x41, 0x42]).unwrap();

        let err =
            ImportExportService::read_and_validate_file(&bad_path, ImportFormat::Txt).unwrap_err();
        match err {
            ImportExportError::InvalidEncoding(msg) => {
                assert!(msg.contains("invalid UTF-8"));
            }
            _ => panic!("Expected InvalidEncoding error, got: {err:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_and_validate_interchange_json() {
        let dir = std::env::temp_dir().join(format!("test_ie_json_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // 1. Valid interchange JSON
        let valid_json_path = dir.join("valid.json");
        let mut f_v = File::create(&valid_json_path).unwrap();
        let doc = InterchangeDocument {
            schema: None,
            app: "personal-notepad".to_string(),
            format: "personal-notepad-interchange".to_string(),
            version: "1.0".to_string(),
            exported_at: "2026-10-10T10:00:00Z".to_string(),
            note_count: 1,
            internal_link_policy: None,
            attachment_policy: None,
            notes: vec![InterchangeNote {
                export_id: "id-1".to_string(),
                title: "Test".to_string(),
                content: "Content".to_string(),
                format: "txt".to_string(),
                notebook_path: None,
                tags: vec![],
                is_favorite: false,
                is_pinned: false,
                created_at: None,
                modified_at: None,
                attachments_omitted_count: None,
                internal_links: vec![],
            }],
        };
        f_v.write_all(serde_json::to_string(&doc).unwrap().as_bytes())
            .unwrap();

        let res = ImportExportService::read_and_validate_file(&valid_json_path, ImportFormat::Json)
            .unwrap();
        assert!(res.interchange_doc.is_some());
        assert_eq!(res.interchange_doc.unwrap().notes.len(), 1);

        // 2. Malformed JSON
        let malformed_path = dir.join("malformed.json");
        let mut f_m = File::create(&malformed_path).unwrap();
        f_m.write_all(b"{ not valid json ...").unwrap();

        let err_m =
            ImportExportService::read_and_validate_file(&malformed_path, ImportFormat::Json)
                .unwrap_err();
        match err_m {
            ImportExportError::Validation(msg) => {
                assert!(msg.contains("Malformed interchange JSON"));
            }
            _ => panic!("Expected Validation error, got: {err_m:?}"),
        }

        // 3. Wrong app
        let wrong_app_path = dir.join("wrong_app.json");
        let mut f_w = File::create(&wrong_app_path).unwrap();
        let mut wrong_doc = doc.clone();
        wrong_doc.app = "other-app".to_string();
        f_w.write_all(serde_json::to_string(&wrong_doc).unwrap().as_bytes())
            .unwrap();

        let err_w =
            ImportExportService::read_and_validate_file(&wrong_app_path, ImportFormat::Json)
                .unwrap_err();
        match err_w {
            ImportExportError::Validation(msg) => {
                assert!(msg.contains("expected app 'personal-notepad'"));
            }
            _ => panic!("Expected Validation error, got: {err_w:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_task_8_title_derivation_suite() {
        // Standard extensions
        assert_eq!(
            ImportExportService::derive_proposed_title("Sprint Plan.md"),
            "Sprint Plan"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("Notes.txt"),
            "Notes"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("export.markdown"),
            "export"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("archive.json"),
            "archive"
        );

        // Multiple dots in filename
        assert_eq!(
            ImportExportService::derive_proposed_title("Report.2026.Q3.final.md"),
            "Report.2026.Q3.final"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("my.notes.txt"),
            "my.notes"
        );

        // Unicode and emoji preservation
        assert_eq!(
            ImportExportService::derive_proposed_title("✨ Rocket Notes 🚀.md"),
            "✨ Rocket Notes 🚀"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("दस्तावेज़.txt"),
            "दस्तावेज़"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("日本語ノート.markdown"),
            "日本語ノート"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("Plan d'action & résumé.txt"),
            "Plan d'action & résumé"
        );

        // Whitespace and control character stripping
        assert_eq!(
            ImportExportService::derive_proposed_title("   Leading and Trailing   .md"),
            "Leading and Trailing"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("\t\nNote with controls\r\n.txt"),
            "Note with controls"
        );

        // Degenerate, empty, or dotfile names fallback to "Untitled Note"
        assert_eq!(
            ImportExportService::derive_proposed_title(""),
            "Untitled Note"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("     "),
            "Untitled Note"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title(".txt"),
            "Untitled Note"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title(".md"),
            "Untitled Note"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title(".json"),
            "Untitled Note"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("."),
            "Untitled Note"
        );
        assert_eq!(
            ImportExportService::derive_proposed_title("\x00\x01\x02.txt"),
            "Untitled Note"
        );

        // sanitize_title direct checks
        assert_eq!(
            ImportExportService::sanitize_title("Clean Title"),
            "Clean Title"
        );
        assert_eq!(
            ImportExportService::sanitize_title("  \t\n  "),
            "Untitled Note"
        );
        assert_eq!(
            ImportExportService::sanitize_title("🚀 Custom Emoji Title 🎉"),
            "🚀 Custom Emoji Title 🎉"
        );
        assert_eq!(
            ImportExportService::sanitize_title("\x07\x08Title with controls\x1F"),
            "Title with controls"
        );

        // Structured JSON single note title derivation in scan_candidates
        let mut conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&mut conn).unwrap();
        let dir = std::env::temp_dir().join(format!("test_t8_title_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let single_note_doc = serde_json::json!({
            "app": "personal-notepad",
            "version": "1.0",
            "exported_at": "2026-10-10T10:00:00Z",
            "note_count": 1,
            "notes": [
                {
                    "export_id": "exp-1",
                    "title": "Document Title From Structured Interchange",
                    "content": "Hello",
                    "format": "markdown",
                    "notebook_path": null,
                    "tags": [],
                    "is_favorite": false,
                    "is_pinned": false,
                    "created_at": "2026-10-10T10:00:00Z",
                    "modified_at": "2026-10-10T10:00:00Z"
                }
            ]
        });
        let json_path = dir.join("random_filename.json");
        std::fs::write(&json_path, serde_json::to_string(&single_note_doc).unwrap()).unwrap();

        let preview =
            ImportExportService::scan_candidates(&conn, &[json_path.to_string_lossy().to_string()])
                .unwrap();
        assert_eq!(preview.candidates.len(), 1);
        assert_eq!(
            preview.candidates[0].proposed_title,
            "Document Title From Structured Interchange"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_task_9_duplicate_detection_and_conflict_policies_suite() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&mut conn).unwrap();

        // 1. Seed active notes in database
        conn.execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_deleted)
             VALUES ('note-1', 'Roadmap 2026', 'Quarterly roadmap details and milestones.', 'md', '2026-10-10T10:00:00Z', '2026-10-10T10:00:00Z', 0)",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_deleted)
             VALUES ('note-2', 'Archived Spec', 'Legacy content that was deleted.', 'txt', '2026-10-10T10:00:00Z', '2026-10-10T10:00:00Z', 1)",
            [],
        ).unwrap();

        // 2. Test check_duplicate classifications
        // Exact match (both title and content match active note)
        let dup1 = ImportExportService::check_duplicate(
            &conn,
            "Roadmap 2026",
            "Quarterly roadmap details and milestones.",
        )
        .unwrap();
        assert_eq!(
            dup1,
            DuplicateMatch::ExactMatch {
                existing_id: "note-1".to_string(),
                existing_title: "Roadmap 2026".to_string(),
            }
        );

        // Content match (different title, exact content)
        let dup2 = ImportExportService::check_duplicate(
            &conn,
            "New Filename For Same Roadmap",
            "Quarterly roadmap details and milestones.",
        )
        .unwrap();
        assert_eq!(
            dup2,
            DuplicateMatch::ContentMatch {
                existing_id: "note-1".to_string(),
                existing_title: "Roadmap 2026".to_string(),
            }
        );

        // Title collision only (matching title, DIFFERENT content)
        // Invariant: Title equality alone MUST NOT be treated as exact duplicate
        let dup3 = ImportExportService::check_duplicate(
            &conn,
            "Roadmap 2026",
            "Entirely different quarterly objectives and budgets.",
        )
        .unwrap();
        assert_eq!(
            dup3,
            DuplicateMatch::TitleCollisionOnly {
                existing_id: "note-1".to_string(),
                existing_title: "Roadmap 2026".to_string(),
            }
        );

        // Soft-deleted note must NOT trigger duplicate detection
        let dup_trashed = ImportExportService::check_duplicate(
            &conn,
            "Archived Spec",
            "Legacy content that was deleted.",
        )
        .unwrap();
        assert_eq!(dup_trashed, DuplicateMatch::None);

        // Entirely unique note
        let dup_unique = ImportExportService::check_duplicate(
            &conn,
            "Completely New Note",
            "Fresh unique note content.",
        )
        .unwrap();
        assert_eq!(dup_unique, DuplicateMatch::None);

        // 3. Test should_skip_candidate under all 3 duplicate policies
        // A. ImportAsNew: creates new notes regardless of possible content matches
        let (skip_new_exact, _) =
            ImportExportService::should_skip_candidate(DuplicatePolicy::ImportAsNew, &dup1, true);
        assert!(
            !skip_new_exact,
            "ImportAsNew must not skip exact duplicates"
        );

        let (skip_new_col, _) =
            ImportExportService::should_skip_candidate(DuplicatePolicy::ImportAsNew, &dup3, true);
        assert!(!skip_new_col, "ImportAsNew must not skip title collisions");

        // B. SkipExact: skips exact matches and content matches, but DOES NOT skip title collision only
        let (skip_exact_exact, msg_exact) =
            ImportExportService::should_skip_candidate(DuplicatePolicy::SkipExact, &dup1, true);
        assert!(skip_exact_exact, "SkipExact must skip exact duplicate");
        assert!(msg_exact.unwrap().contains("Skipped exact duplicate note"));

        let (skip_exact_content, msg_content) =
            ImportExportService::should_skip_candidate(DuplicatePolicy::SkipExact, &dup2, true);
        assert!(skip_exact_content, "SkipExact must skip content duplicate");
        assert!(msg_content.unwrap().contains("Skipped duplicate content"));

        // Critical Invariant: Title collision with different content MUST NOT be skipped!
        let (skip_exact_col, _) =
            ImportExportService::should_skip_candidate(DuplicatePolicy::SkipExact, &dup3, true);
        assert!(
            !skip_exact_col,
            "SkipExact must NOT skip title collisions with different content"
        );

        // Unique note must not be skipped
        let (skip_exact_unique, _) = ImportExportService::should_skip_candidate(
            DuplicatePolicy::SkipExact,
            &dup_unique,
            true,
        );
        assert!(!skip_exact_unique, "SkipExact must not skip unique note");

        // C. Ask: respects user selection; if selected, does not skip
        let (skip_ask_selected, _) =
            ImportExportService::should_skip_candidate(DuplicatePolicy::Ask, &dup1, true);
        assert!(!skip_ask_selected, "Ask must not skip selected note");

        // User deselection always skips
        let (skip_deselected, msg_desel) =
            ImportExportService::should_skip_candidate(DuplicatePolicy::ImportAsNew, &dup1, false);
        assert!(skip_deselected, "Deselected candidate must be skipped");
        assert_eq!(msg_desel.unwrap(), "Deselected by user.");

        // 4. Test scan_candidates integration for exact duplicate vs title collision
        let dir = std::env::temp_dir().join(format!("test_t9_dup_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // Exact duplicate file
        let exact_file = dir.join("Roadmap 2026.md");
        std::fs::write(&exact_file, "Quarterly roadmap details and milestones.").unwrap();

        // Title collision file (same title, different content)
        // Using proposed title override test via file named Roadmap 2026 but in subdir or another file:
        let collision_dir = dir.join("sub");
        std::fs::create_dir_all(&collision_dir).unwrap();
        let collision_file2 = collision_dir.join("Roadmap 2026.md");
        std::fs::write(&collision_file2, "Different quarterly objectives.").unwrap();

        let preview = ImportExportService::scan_candidates(
            &conn,
            &[
                exact_file.to_string_lossy().to_string(),
                collision_file2.to_string_lossy().to_string(),
            ],
        )
        .unwrap();

        assert_eq!(preview.candidates.len(), 2);
        // Candidate 0: Exact duplicate
        assert!(preview.candidates[0].is_duplicate);
        assert_eq!(
            preview.candidates[0].duplicate_note_id,
            Some("note-1".to_string())
        );
        assert_eq!(
            preview.candidates[0].duplicate_note_title,
            Some("Roadmap 2026".to_string())
        );
        assert!(preview.candidates[0]
            .warnings
            .iter()
            .any(|w| w.contains("Exact duplicate note detected")));

        // Candidate 1: Title collision with different content (NOT flagged as exact duplicate to skip)
        assert!(!preview.candidates[1].is_duplicate);
        assert_eq!(
            preview.candidates[1].duplicate_note_id,
            Some("note-1".to_string())
        );
        assert_eq!(
            preview.candidates[1].duplicate_note_title,
            Some("Roadmap 2026".to_string())
        );
        assert!(preview.candidates[1]
            .warnings
            .iter()
            .any(|w| w.contains("Title collision")));

        assert_eq!(preview.duplicate_files, 1);
        assert_eq!(preview.valid_files, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_task_10_safe_note_creation_and_import_transactions_suite() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&mut conn).unwrap();

        // 1. Setup temporary directory with test files
        let dir = std::env::temp_dir().join(format!("test_t10_create_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // Target notebook creation
        let nb = NotebookRepository::create(
            &conn,
            crate::storage::models::CreateNotebookDto {
                name: "Work Projects".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        // Create a Markdown note file
        let md_file = dir.join("Architecture Notes.md");
        let md_content = "# Architecture\n\n- Offline-first SQLite\n- Local Tauri v2";
        std::fs::write(&md_file, md_content).unwrap();

        // Create a TXT note file
        let txt_file = dir.join("Todo List.txt");
        let txt_content = "1. Finish Task 10\n2. Run full regression test";
        std::fs::write(&txt_file, txt_content).unwrap();

        // Create a structured JSON package with tags
        let json_file = dir.join("Structured Package.json");
        let json_doc = serde_json::json!({
            "app": "personal-notepad",
            "version": "1.0",
            "exported_at": "2026-10-10T10:00:00Z",
            "note_count": 1,
            "notes": [
                {
                    "export_id": "external-export-uuid-999",
                    "title": "Tagged Note From JSON",
                    "content": "Content inside structured interchange package.",
                    "format": "md",
                    "notebook_path": null,
                    "tags": ["rust", "tauri", "sqlite"],
                    "is_favorite": true,
                    "is_pinned": false,
                    "created_at": "2026-10-10T10:00:00Z",
                    "modified_at": "2026-10-10T10:00:00Z"
                }
            ]
        });
        std::fs::write(&json_file, serde_json::to_string(&json_doc).unwrap()).unwrap();

        // Non-existent file for partial failure testing
        let missing_file = dir.join("missing.txt");

        // 2. Execute import with notebook assignment and partial failure
        let request = ImportRequest {
            operation_id: None,
            candidates: vec![
                ImportCandidateOverride {
                    id: "cand-1".to_string(),
                    file_path: md_file.to_string_lossy().to_string(),
                    selected: true,
                    custom_title: Some("Custom Architecture Title".to_string()),
                },
                ImportCandidateOverride {
                    id: "cand-2".to_string(),
                    file_path: txt_file.to_string_lossy().to_string(),
                    selected: false, // Deselected: should be skipped
                    custom_title: None,
                },
                ImportCandidateOverride {
                    id: "cand-3".to_string(),
                    file_path: missing_file.to_string_lossy().to_string(),
                    selected: true, // Missing file: should fail cleanly
                    custom_title: None,
                },
                ImportCandidateOverride {
                    id: "cand-4".to_string(),
                    file_path: json_file.to_string_lossy().to_string(),
                    selected: true, // JSON package with tags: should succeed
                    custom_title: None,
                },
            ],
            options: ImportOptions {
                target_notebook_id: Some(nb.id.clone()),
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        };

        let summary = ImportExportService::execute_import(&mut conn, request).unwrap();

        // 3. Verify granular summary report
        assert_eq!(summary.total_processed, 4);
        assert_eq!(summary.imported_count, 2); // md_file and json_file
        assert_eq!(summary.skipped_count, 1); // txt_file deselected
        assert_eq!(summary.failed_count, 1); // missing_file

        assert_eq!(summary.results[0].status, ImportItemStatus::Imported);
        assert_eq!(summary.results[0].file_name, "Architecture Notes.md");
        let created_md_id = summary.results[0].created_note_id.as_ref().unwrap();

        assert_eq!(summary.results[1].status, ImportItemStatus::Skipped);
        assert_eq!(
            summary.results[1].message.as_deref(),
            Some("Deselected by user.")
        );

        assert_eq!(summary.results[2].status, ImportItemStatus::Failed);
        assert!(summary.results[2]
            .message
            .as_ref()
            .unwrap()
            .contains("does not exist"));

        assert_eq!(summary.results[3].status, ImportItemStatus::Imported);
        let created_json_id = summary.results[3].created_note_id.as_ref().unwrap();

        // 4. Verify database persistence and invariants
        let md_note = NoteRepository::get_by_id(&conn, created_md_id)
            .unwrap()
            .unwrap();
        assert_eq!(md_note.title, "Custom Architecture Title");
        assert_eq!(md_note.content, md_content);
        assert_eq!(md_note.format, "md");
        assert_eq!(md_note.notebook_id, Some(nb.id.clone()));
        assert!(!md_note.is_deleted);

        let json_note = NoteRepository::get_by_id(&conn, created_json_id)
            .unwrap()
            .unwrap();
        assert_eq!(json_note.title, "Tagged Note From JSON");
        assert_eq!(
            json_note.content,
            "Content inside structured interchange package."
        );
        assert_eq!(json_note.format, "md");
        assert!(json_note.is_favorite);
        assert_eq!(json_note.notebook_id, Some(nb.id.clone()));
        // Invariant: external export_id was NOT used as note primary key
        assert_ne!(json_note.id, "external-export-uuid-999");

        // Verify tags were properly created and linked
        let note_tags =
            crate::storage::repositories::TagRepository::get_tags_for_note(&conn, created_json_id)
                .unwrap();
        assert_eq!(note_tags.len(), 3);
        let tag_names: Vec<String> = note_tags.into_iter().map(|t| t.name).collect();
        assert!(tag_names.contains(&"rust".to_string()));
        assert!(tag_names.contains(&"tauri".to_string()));
        assert!(tag_names.contains(&"sqlite".to_string()));

        // 5. Verify target notebook validation error
        let invalid_nb_request = ImportRequest {
            operation_id: None,
            candidates: vec![ImportCandidateOverride {
                id: "cand-x".to_string(),
                file_path: md_file.to_string_lossy().to_string(),
                selected: true,
                custom_title: None,
            }],
            options: ImportOptions {
                target_notebook_id: Some("non-existent-notebook-uuid".to_string()),
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        };

        let err = ImportExportService::execute_import(&mut conn, invalid_nb_request).unwrap_err();
        match err {
            StorageError::Validation(msg) => {
                assert!(msg.contains("Target notebook 'non-existent-notebook-uuid' does not exist"));
            }
            _ => panic!("Expected StorageError::Validation, got: {err:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_task_11_markdown_import_without_unnecessary_conversion_suite() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&mut conn).unwrap();
        let dir = std::env::temp_dir().join(format!("test_t11_md_import_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // 1. Rich markdown content with headings, code blocks, tables, blockquotes, lists, links, line breaks, and raw HTML
        let rich_markdown_content = "# Technical Architecture & Specification

## 1. Introduction
This is a comprehensive overview of the offline-first personal notepad.

### Features
* Offline SQLite database
* High-speed full-text search
* Zero unnecessary syntax conversion

> **Note:** Markdown source is preserved *verbatim*.

| Component | Responsibility | Tech Stack |
| :--- | :--- | :--- |
| Core UI | React + Vite | React 19, TypeScript |
| Storage | SQLite local | rusqlite, FTS5 |

```rust
pub fn calculate_checksum(bytes: &[u8]) -> u32 {
    bytes.iter().map(|&b| b as u32).sum()
}
```

Check out our related note: [System Design](note:550e8400-e29b-41d4-a716-446655440000).

<div class=\"custom-note-badge\">
    <p>Raw HTML must not be executed or stripped</p>
    <script>alert(\"Untrusted script must stay plain text\");</script>
</div>
";

        let md_file = dir.join("Architecture_Spec.md");
        std::fs::write(&md_file, rich_markdown_content).unwrap();

        // 2. Scan candidate - verify format detection as Md
        let preview =
            ImportExportService::scan_candidates(&conn, &[md_file.to_string_lossy().to_string()])
                .unwrap();
        assert_eq!(preview.candidates.len(), 1);
        assert_eq!(preview.candidates[0].detected_format, ImportFormat::Md);
        assert_eq!(preview.candidates[0].proposed_title, "Architecture_Spec");

        // 3. Execute import
        let request = ImportRequest {
            operation_id: None,
            candidates: vec![ImportCandidateOverride {
                id: preview.candidates[0].id.clone(),
                file_path: md_file.to_string_lossy().to_string(),
                selected: true,
                custom_title: Some("Preserved Architecture Spec".to_string()),
            }],
            options: ImportOptions {
                target_notebook_id: None,
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        };

        let summary = ImportExportService::execute_import(&mut conn, request).unwrap();
        assert_eq!(summary.imported_count, 1);
        let created_note_id = summary.results[0].created_note_id.as_ref().unwrap();

        // 4. Verify database record
        let imported_note = NoteRepository::get_by_id(&conn, created_note_id)
            .unwrap()
            .unwrap();
        assert_eq!(imported_note.title, "Preserved Architecture Spec");
        // Verify format is explicitly set to "md"
        assert_eq!(imported_note.format, "md");
        // Verify raw Markdown source syntax is completely intact without HTML conversion or construct stripping
        assert_eq!(imported_note.content, rich_markdown_content);
        assert!(imported_note
            .content
            .contains("# Technical Architecture & Specification"));
        assert!(imported_note.content.contains("```rust"));
        assert!(imported_note
            .content
            .contains("| Component | Responsibility | Tech Stack |"));
        assert!(imported_note
            .content
            .contains("<script>alert(\"Untrusted script must stay plain text\");</script>"));

        // 5. Verify editing and updating note preserves "md" format and does not silently convert to "txt"
        let updated_note = NoteRepository::update(
            &conn,
            created_note_id,
            crate::storage::models::UpdateNoteDto {
                title: Some("Preserved Architecture Spec (Edited)".to_string()),
                content: Some(format!(
                    "{}

## 2. Updated Appendix",
                    rich_markdown_content
                )),
                format: None, // format is omitted, must preserve existing "md" format
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated_note.format, "md");
        assert_eq!(updated_note.title, "Preserved Architecture Spec (Edited)");
        assert!(updated_note.content.contains("## 2. Updated Appendix"));

        // 6. Verify structured JSON import with "markdown" / "md" also sets format="md"
        let json_file = dir.join("Structured_Markdown.json");
        let sample_json_content = r#"# Headings & Fences

```json
{"test": true}
```"#;
        let json_doc = serde_json::json!({
            "app": "personal-notepad",
            "version": "1.0",
            "exported_at": "2026-10-10T10:00:00Z",
            "note_count": 1,
            "notes": [
                {
                    "export_id": "exp-md-99",
                    "title": "Structured Markdown Note",
                    "content": sample_json_content,
                    "format": "markdown",
                    "notebook_path": null,
                    "tags": ["docs"],
                    "is_favorite": false,
                    "is_pinned": false,
                    "created_at": "2026-10-10T10:00:00Z",
                    "modified_at": "2026-10-10T10:00:00Z"
                }
            ]
        });
        std::fs::write(&json_file, serde_json::to_string(&json_doc).unwrap()).unwrap();

        let json_request = ImportRequest {
            operation_id: None,
            candidates: vec![ImportCandidateOverride {
                id: "cand-struct-md".to_string(),
                file_path: json_file.to_string_lossy().to_string(),
                selected: true,
                custom_title: None,
            }],
            options: ImportOptions {
                target_notebook_id: None,
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        };

        let json_summary = ImportExportService::execute_import(&mut conn, json_request).unwrap();
        assert_eq!(json_summary.imported_count, 1);
        let struct_note_id = json_summary.results[0].created_note_id.as_ref().unwrap();
        let struct_note = NoteRepository::get_by_id(&conn, struct_note_id)
            .unwrap()
            .unwrap();
        assert_eq!(struct_note.format, "md");
        assert_eq!(struct_note.content, sample_json_content);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_task_12_plain_text_import_and_encoding_behavior_suite() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&mut conn).unwrap();
        let dir =
            std::env::temp_dir().join(format!("test_t12_txt_import_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // 1. Plain text with meaningful whitespace, blank lines, tabs, and Windows CRLF
        let raw_txt_with_crlf = "First Line\r\n\r\n    Indented line with 4 spaces\r\n\tTabbed line\r\nTrailing line with trailing space   \r\n\r\n";
        let txt_file = dir.join("Daily_Notes.txt");
        std::fs::write(&txt_file, raw_txt_with_crlf).unwrap();

        // 2. Multilingual Unicode note with emojis, accents, and symbols
        let unicode_txt = "🌟 Multilingual Document 🚀\n\n日本語: こんにちは世界\nहिन्दी: नमस्ते दुनिया\nالعربية: مرحباً بالعالم\nFrançais: Café & Crème Brûlée\nDeutsch: Übergrößenträger\nMath: ∀x ∈ ℝ, x² ≥ 0 ∑ π";
        let unicode_file = dir.join("Unicode_Doc.txt");
        std::fs::write(&unicode_file, unicode_txt).unwrap();

        // 3. Completely empty text file (0 bytes)
        let empty_file = dir.join("Empty_Doc.txt");
        std::fs::write(&empty_file, "").unwrap();

        // 4. Invalid UTF-8 binary file masquerading as .txt
        let invalid_utf8_file = dir.join("Corrupt_Encoding.txt");
        std::fs::write(&invalid_utf8_file, [0xFF, 0xFE, 0xFD, 0x01]).unwrap();

        // 5. Binary file containing null bytes masquerading as .txt
        let binary_file = dir.join("Binary_Doc.txt");
        std::fs::write(&binary_file, b"Hello\x00World\x00NullBytes").unwrap();

        // Scan all candidates
        let preview = ImportExportService::scan_candidates(
            &conn,
            &[
                txt_file.to_string_lossy().to_string(),
                unicode_file.to_string_lossy().to_string(),
                empty_file.to_string_lossy().to_string(),
                invalid_utf8_file.to_string_lossy().to_string(),
                binary_file.to_string_lossy().to_string(),
            ],
        )
        .unwrap();

        assert_eq!(preview.candidates.len(), 5);
        // Candidate 0: Daily_Notes.txt (Valid)
        assert_eq!(preview.candidates[0].detected_format, ImportFormat::Txt);
        assert_eq!(
            preview.candidates[0].validation_status,
            ValidationStatus::Valid
        );

        // Candidate 1: Unicode_Doc.txt (Valid)
        assert_eq!(preview.candidates[1].detected_format, ImportFormat::Txt);
        assert_eq!(
            preview.candidates[1].validation_status,
            ValidationStatus::Valid
        );

        // Candidate 2: Empty_Doc.txt (Valid with warning)
        assert_eq!(preview.candidates[2].detected_format, ImportFormat::Txt);
        assert_eq!(
            preview.candidates[2].validation_status,
            ValidationStatus::Warning
        );
        assert!(preview.candidates[2]
            .warnings
            .iter()
            .any(|w| w.contains("File is empty (0 bytes)")));

        // Candidate 3: Corrupt_Encoding.txt (Invalid)
        assert_eq!(
            preview.candidates[3].validation_status,
            ValidationStatus::Invalid
        );
        assert!(preview.candidates[3]
            .error_message
            .as_ref()
            .unwrap()
            .contains("invalid UTF-8 encoding"));

        // Candidate 4: Binary_Doc.txt (Invalid)
        assert_eq!(
            preview.candidates[4].validation_status,
            ValidationStatus::Invalid
        );
        assert!(preview.candidates[4]
            .error_message
            .as_ref()
            .unwrap()
            .contains("Binary file content detected"));

        // Execute import on the candidates
        let request = ImportRequest {
            operation_id: None,
            candidates: vec![
                ImportCandidateOverride {
                    id: preview.candidates[0].id.clone(),
                    file_path: txt_file.to_string_lossy().to_string(),
                    selected: true,
                    custom_title: None,
                },
                ImportCandidateOverride {
                    id: preview.candidates[1].id.clone(),
                    file_path: unicode_file.to_string_lossy().to_string(),
                    selected: true,
                    custom_title: None,
                },
                ImportCandidateOverride {
                    id: preview.candidates[2].id.clone(),
                    file_path: empty_file.to_string_lossy().to_string(),
                    selected: true,
                    custom_title: None,
                },
                ImportCandidateOverride {
                    id: preview.candidates[3].id.clone(),
                    file_path: invalid_utf8_file.to_string_lossy().to_string(),
                    selected: true, // Should fail safely at execution
                    custom_title: None,
                },
            ],
            options: ImportOptions {
                target_notebook_id: None,
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        };

        let summary = ImportExportService::execute_import(&mut conn, request).unwrap();
        assert_eq!(summary.imported_count, 3);
        assert_eq!(summary.failed_count, 1);

        // Verify Daily_Notes.txt: CRLF was normalized to LF, but all blank lines, indentation, tabs, and spaces are preserved
        let id_0 = summary.results[0].created_note_id.as_ref().unwrap();
        let note_0 = NoteRepository::get_by_id(&conn, id_0).unwrap().unwrap();
        assert_eq!(note_0.format, "txt");
        let expected_normalized_crlf = "First Line\n\n    Indented line with 4 spaces\n\tTabbed line\nTrailing line with trailing space   \n\n";
        assert_eq!(note_0.content, expected_normalized_crlf);
        assert_eq!(note_0.title, "Daily_Notes");

        // Verify Unicode_Doc.txt: multi-script Unicode, emojis, math symbols match exactly without lossy replacement
        let id_1 = summary.results[1].created_note_id.as_ref().unwrap();
        let note_1 = NoteRepository::get_by_id(&conn, id_1).unwrap().unwrap();
        assert_eq!(note_1.format, "txt");
        assert_eq!(note_1.content, unicode_txt);
        assert!(
            !note_1.content.contains('\u{FFFD}'),
            "Must never contain replacement character"
        );

        // Verify Empty_Doc.txt: note created successfully with empty content
        let id_2 = summary.results[2].created_note_id.as_ref().unwrap();
        let note_2 = NoteRepository::get_by_id(&conn, id_2).unwrap().unwrap();
        assert_eq!(note_2.format, "txt");
        assert_eq!(note_2.content, "");
        assert_eq!(note_2.title, "Empty_Doc");

        // Verify Corrupt_Encoding.txt: rejected with clean failure and no note created
        assert_eq!(summary.results[3].status, ImportItemStatus::Rejected);
        assert!(summary.results[3].created_note_id.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_task_13_notebook_and_metadata_handling_for_structured_imports_suite() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&mut conn).unwrap();
        let dir =
            std::env::temp_dir().join(format!("test_t13_struct_import_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // 1. Version rejection test: version 2.0 must be rejected with actionable error
        let v2_file = dir.join("Unsupported_v2.json");
        let v2_doc = serde_json::json!({
            "app": "personal-notepad",
            "version": "2.0",
            "exported_at": "2026-10-12T10:00:00Z",
            "notes": []
        });
        std::fs::write(&v2_file, serde_json::to_string(&v2_doc).unwrap()).unwrap();
        let v2_preview =
            ImportExportService::scan_candidates(&conn, &[v2_file.to_string_lossy().to_string()])
                .unwrap();
        assert_eq!(
            v2_preview.candidates[0].validation_status,
            ValidationStatus::Invalid
        );
        assert!(v2_preview.candidates[0]
            .error_message
            .as_ref()
            .unwrap()
            .contains("Unsupported interchange version '2.0'"));

        // 2. Structured JSON package with:
        // - 2 notes linking to each other via internal links (canonical + shorthand)
        // - Nested notebook hierarchy "Engineering / Backend / Storage"
        // - Duplicate and case-insensitive tags
        // - Explicit RFC 3339 timestamps
        // - Forward compatible missing optional fields
        let package_file = dir.join("Full_Interchange_Package.json");
        let package_doc = serde_json::json!({
            "app": "personal-notepad",
            "version": "1.0",
            "exported_at": "2026-10-12T12:00:00Z",
            "note_count": 2,
            "notes": [
                {
                    "export_id": "ext-arch-100",
                    "title": "Architecture Overview",
                    "content": "Refer to [Storage Engine](notepad://note/ext-store-200) and [Legacy Ref](note:ext-store-200). External: [Docs](notepad://note/external-unimported-id)",
                    "format": "md",
                    "notebook_path": "Engineering/Backend/Storage",
                    "tags": ["rust", "database", "Rust", "DATABASE"],
                    "is_favorite": true,
                    "is_pinned": true,
                    "created_at": "2026-01-01T08:00:00Z",
                    "modified_at": "2026-06-01T12:00:00Z"
                },
                {
                    "export_id": "ext-store-200",
                    "title": "Storage Engine",
                    "content": "Backlink to [Architecture Overview](notepad://note/ext-arch-100).",
                    "format": "md",
                    "notebook_path": "Engineering/Backend/Storage",
                    "tags": ["database"],
                    "is_favorite": false,
                    "is_pinned": false
                }
            ]
        });
        std::fs::write(&package_file, serde_json::to_string(&package_doc).unwrap()).unwrap();

        // 3. Scan candidate
        let preview = ImportExportService::scan_candidates(
            &conn,
            &[package_file.to_string_lossy().to_string()],
        )
        .unwrap();
        assert_eq!(preview.candidates.len(), 1);
        assert_eq!(
            preview.candidates[0].validation_status,
            ValidationStatus::Warning
        );
        assert!(preview.candidates[0]
            .warnings
            .iter()
            .any(|w| w.contains("Structured package containing 2 note(s)")));

        // 4. Execute import without target_notebook_id override (should preserve notebook_path hierarchy)
        let request = ImportRequest {
            operation_id: None,
            candidates: vec![ImportCandidateOverride {
                id: preview.candidates[0].id.clone(),
                file_path: package_file.to_string_lossy().to_string(),
                selected: true,
                custom_title: None,
            }],
            options: ImportOptions {
                target_notebook_id: None,
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        };

        let summary = ImportExportService::execute_import(&mut conn, request).unwrap();
        assert_eq!(summary.imported_count, 1);
        assert_eq!(summary.results[0].status, ImportItemStatus::Imported);

        // 5. Verify database records
        // Check created notebook hierarchy
        let all_notebooks = NotebookRepository::list(&conn).unwrap();
        let eng_nb = all_notebooks
            .iter()
            .find(|n| n.name == "Engineering")
            .expect("Engineering notebook");
        assert!(eng_nb.parent_id.is_none());

        let backend_nb = all_notebooks
            .iter()
            .find(|n| n.name == "Backend")
            .expect("Backend notebook");
        assert_eq!(backend_nb.parent_id, Some(eng_nb.id.clone()));

        let storage_nb = all_notebooks
            .iter()
            .find(|n| n.name == "Storage")
            .expect("Storage notebook");
        assert_eq!(storage_nb.parent_id, Some(backend_nb.id.clone()));

        // Hierarchy path check
        let path = NotebookRepository::get_hierarchy_path(&conn, &storage_nb.id).unwrap();
        assert_eq!(path, "Engineering / Backend / Storage");

        // Verify notes created
        let all_notes = NoteRepository::list_active(&conn).unwrap();
        assert_eq!(all_notes.len(), 2);

        let arch_note = all_notes
            .iter()
            .find(|n| n.title == "Architecture Overview")
            .expect("Arch note");
        let store_note = all_notes
            .iter()
            .find(|n| n.title == "Storage Engine")
            .expect("Store note");

        // Verify assigned notebook
        assert_eq!(arch_note.notebook_id, Some(storage_nb.id.clone()));
        assert_eq!(store_note.notebook_id, Some(storage_nb.id.clone()));

        // Invariant: External IDs were NEVER used as SQLite primary keys
        assert_ne!(arch_note.id, "ext-arch-100");
        assert_ne!(store_note.id, "ext-store-200");

        // Invariant: Internal links remapped accurately to new local UUIDs
        let expected_canonical_link = format!("notepad://note/{}", store_note.id);
        let expected_shorthand_link = format!("note:{}", store_note.id);
        assert!(arch_note.content.contains(&expected_canonical_link));
        assert!(arch_note.content.contains(&expected_shorthand_link));
        // External reference not in package remained untouched
        assert!(arch_note
            .content
            .contains("notepad://note/external-unimported-id"));

        let expected_backlink = format!("notepad://note/{}", arch_note.id);
        assert!(store_note.content.contains(&expected_backlink));

        // Invariant: Timestamps preserved when valid RFC 3339
        assert_eq!(arch_note.created_at, "2026-01-01T08:00:00Z");
        assert_eq!(arch_note.modified_at, "2026-06-01T12:00:00Z");

        // Invariant: Flags preserved
        assert!(arch_note.is_favorite);
        assert!(arch_note.is_pinned);
        assert!(!store_note.is_favorite);
        assert!(!store_note.is_pinned);

        // Invariant: Tags deduplicated and case-insensitively resolved
        let arch_tags = TagRepository::get_tags_for_note(&conn, &arch_note.id).unwrap();
        assert_eq!(arch_tags.len(), 2); // "rust" and "database" (duplicates discarded)

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_task_14_individual_note_export_suite() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&mut conn).unwrap();
        let dir = std::env::temp_dir().join(format!("test_t14_export_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // 1. Filename sanitization tests
        assert_eq!(
            ImportExportService::sanitize_filename(
                "Project / Plan : 2026 * Final ? <v1> | \"Draft\""
            ),
            "Project  Plan  2026  Final  v1  Draft"
        );
        assert_eq!(
            ImportExportService::sanitize_filename("   ...   "),
            "Untitled Note"
        );
        assert_eq!(
            ImportExportService::sanitize_filename("Valid Note Name"),
            "Valid Note Name"
        );

        // 2. Create sample notes: 1 Markdown note, 1 Plain text note
        let md_content = "# Project Architecture\n\n- Local-first SQLite\n- Rich Markdown syntax\n\n```rust\nfn main() {}\n```";
        let md_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Architecture".to_string()),
                content: Some(md_content.to_string()),
                format: Some("md".to_string()),
                notebook_id: None,
                is_favorite: Some(true),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        // Add a tag to md_note
        let tag = TagRepository::create(&conn, "architecture").unwrap();
        conn.execute(
            "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            params![md_note.id, tag.id],
        )
        .unwrap();

        let txt_content = "1. Plain text line\n2. Another text line with spaces   \n";
        let txt_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Todo List".to_string()),
                content: Some(txt_content.to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        // 3. Export md_note as Markdown (.md)
        let md_dest = dir.join("exported_notes/architecture.md");
        let md_export_res = ImportExportService::export_single_note(
            &conn,
            ExportSingleNoteRequest {
                note_id: md_note.id.clone(),
                format: ExportFormat::Md,
                destination_path: md_dest.to_string_lossy().to_string(),
                overwrite: false,
            },
        )
        .unwrap();

        assert_eq!(md_export_res.status, ExportItemStatus::Exported);
        assert_eq!(md_export_res.note_id, md_note.id);
        let read_md_file = std::fs::read_to_string(&md_dest).unwrap();
        assert_eq!(read_md_file, md_content);

        // 4. Export md_note as Plain Text (.txt) - must export source text containing markdown syntax verbatim without rendering
        let txt_dest_for_md = dir.join("exported_notes/architecture_as_txt.txt");
        let txt_md_res = ImportExportService::export_single_note(
            &conn,
            ExportSingleNoteRequest {
                note_id: md_note.id.clone(),
                format: ExportFormat::Txt,
                destination_path: txt_dest_for_md.to_string_lossy().to_string(),
                overwrite: false,
            },
        )
        .unwrap();

        assert_eq!(txt_md_res.status, ExportItemStatus::Exported);
        let read_txt_md_file = std::fs::read_to_string(&txt_dest_for_md).unwrap();
        assert_eq!(read_txt_md_file, md_content);
        assert!(read_txt_md_file.contains("```rust"));

        // 5. Export txt_note as Plain Text (.txt)
        let txt_dest = dir.join("exported_notes/todo.txt");
        let txt_export_res = ImportExportService::export_single_note(
            &conn,
            ExportSingleNoteRequest {
                note_id: txt_note.id.clone(),
                format: ExportFormat::Txt,
                destination_path: txt_dest.to_string_lossy().to_string(),
                overwrite: false,
            },
        )
        .unwrap();

        assert_eq!(txt_export_res.status, ExportItemStatus::Exported);
        let read_txt_file = std::fs::read_to_string(&txt_dest).unwrap();
        assert_eq!(read_txt_file, txt_content);

        // 6. Export md_note as Structured JSON (.json)
        let json_dest = dir.join("exported_notes/architecture.json");
        let json_export_res = ImportExportService::export_single_note(
            &conn,
            ExportSingleNoteRequest {
                note_id: md_note.id.clone(),
                format: ExportFormat::Json,
                destination_path: json_dest.to_string_lossy().to_string(),
                overwrite: false,
            },
        )
        .unwrap();

        assert_eq!(json_export_res.status, ExportItemStatus::Exported);
        let read_json_file = std::fs::read_to_string(&json_dest).unwrap();
        let parsed_doc: InterchangeDocument = serde_json::from_str(&read_json_file).unwrap();
        assert_eq!(parsed_doc.app, "personal-notepad");
        assert_eq!(parsed_doc.version, "1.0");
        assert_eq!(parsed_doc.notes.len(), 1);
        assert_eq!(parsed_doc.notes[0].title, "Project Architecture");
        assert_eq!(parsed_doc.notes[0].tags, vec!["architecture".to_string()]);
        assert!(parsed_doc.notes[0].is_favorite);

        // 7. Verify source note in SQLite remained untouched
        let note_in_db = NoteRepository::get_by_id(&conn, &md_note.id)
            .unwrap()
            .unwrap();
        assert_eq!(note_in_db.title, md_note.title);
        assert_eq!(note_in_db.content, md_note.content);
        assert_eq!(note_in_db.format, md_note.format);
        assert_eq!(note_in_db.created_at, md_note.created_at);
        assert_eq!(note_in_db.modified_at, md_note.modified_at);

        // 8. Non-existent note returns NotFound
        let not_found_res = ImportExportService::export_single_note(
            &conn,
            ExportSingleNoteRequest {
                note_id: "non-existent-note-id".to_string(),
                format: ExportFormat::Txt,
                destination_path: dir.join("error.txt").to_string_lossy().to_string(),
                overwrite: false,
            },
        );
        assert!(matches!(not_found_res, Err(StorageError::NotFound(_))));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_task_15_multiple_note_and_notebook_export_selection_suite() {
        use crate::storage::database::init_connection;
        use crate::storage::migrations::run_migrations;
        use crate::storage::models::{CreateNoteDto, CreateNotebookDto};
        use crate::storage::repositories::TagRepository;

        let mut conn = init_connection(std::path::Path::new(":memory:")).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create notebook hierarchy: Engineering -> Backend -> Storage
        let root_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Engineering".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        let child_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Backend".to_string(),
                parent_id: Some(root_nb.id.clone()),
            },
        )
        .unwrap();

        let grandchild_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Storage".to_string(),
                parent_id: Some(child_nb.id.clone()),
            },
        )
        .unwrap();

        let unassigned_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Marketing".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        // 2. Create notes across notebooks
        let note_root = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Engineering Overview".to_string()),
                content: Some("# Overview\nEngineering guidelines.".to_string()),
                notebook_id: Some(root_nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let note_child = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Backend Architecture".to_string()),
                content: Some("REST and gRPC specs.".to_string()),
                notebook_id: Some(child_nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let _note_grandchild = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Storage Engine".to_string()),
                content: Some("SQLite implementation details.".to_string()),
                notebook_id: Some(grandchild_nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let _note_marketing = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Campaign 2026".to_string()),
                content: Some("Launch timeline.".to_string()),
                notebook_id: Some(unassigned_nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let note_trashed = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Deprecated Backend Plan".to_string()),
                content: Some("Old notes.".to_string()),
                notebook_id: Some(child_nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        // Soft-delete the note into trash
        NoteRepository::soft_delete(&conn, &note_trashed.id).unwrap();

        // Assign tags
        let tag_eng = TagRepository::create(&conn, "engineering").unwrap();
        conn.execute(
            "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            params![note_root.id, tag_eng.id],
        )
        .unwrap();

        // 3. Test Preview: Single Note Scope
        let preview_single = ImportExportService::resolve_export_selection(
            &conn,
            &ExportScope::SingleNote {
                note_id: note_root.id.clone(),
            },
        )
        .unwrap();
        assert_eq!(preview_single.total_requested, 1);
        assert_eq!(preview_single.total_included, 1);
        assert_eq!(preview_single.skipped_trashed_count, 0);
        assert_eq!(preview_single.skipped_missing_count, 0);
        assert_eq!(preview_single.notes.len(), 1);
        assert_eq!(preview_single.notes[0].title, "Engineering Overview");
        assert_eq!(preview_single.notes[0].tag_count, 1);
        assert_eq!(
            preview_single.notes[0].notebook_path.as_deref(),
            Some("Engineering")
        );

        // Single note trashed -> reported as skipped_trashed
        let preview_single_trashed = ImportExportService::resolve_export_selection(
            &conn,
            &ExportScope::SingleNote {
                note_id: note_trashed.id.clone(),
            },
        )
        .unwrap();
        assert_eq!(preview_single_trashed.total_requested, 1);
        assert_eq!(preview_single_trashed.total_included, 0);
        assert_eq!(preview_single_trashed.skipped_trashed_count, 1);

        // 4. Test Preview: Selected Notes Scope with valid, missing, and trashed notes
        let preview_selected = ImportExportService::resolve_export_selection(
            &conn,
            &ExportScope::SelectedNotes {
                note_ids: vec![
                    note_root.id.clone(),
                    note_child.id.clone(),
                    note_trashed.id.clone(),
                    "missing-random-uuid".to_string(),
                ],
            },
        )
        .unwrap();
        assert_eq!(preview_selected.total_requested, 4);
        assert_eq!(preview_selected.total_included, 2);
        assert_eq!(preview_selected.skipped_trashed_count, 1);
        assert_eq!(preview_selected.skipped_missing_count, 1);
        assert_eq!(preview_selected.notes.len(), 2);

        // 5. Test Preview: Notebook Scope (Shallow: include_nested = false)
        let preview_nb_shallow = ImportExportService::resolve_export_selection(
            &conn,
            &ExportScope::Notebook {
                notebook_id: root_nb.id.clone(),
                include_nested: false,
            },
        )
        .unwrap();
        assert_eq!(preview_nb_shallow.total_included, 1); // Only root note
        assert_eq!(preview_nb_shallow.notes[0].id, note_root.id);
        assert_eq!(preview_nb_shallow.included_notebook_count, 1);

        // 6. Test Preview: Notebook Scope (Hierarchical: include_nested = true)
        let preview_nb_nested = ImportExportService::resolve_export_selection(
            &conn,
            &ExportScope::Notebook {
                notebook_id: root_nb.id.clone(),
                include_nested: true,
            },
        )
        .unwrap();
        // Root + Child + Grandchild = 3 active notes (trashed note excluded!)
        assert_eq!(preview_nb_nested.total_included, 3);
        assert_eq!(preview_nb_nested.skipped_trashed_count, 1); // 1 trashed note in child_nb
        assert_eq!(preview_nb_nested.included_notebook_count, 3); // 3 descendant notebooks

        // Verify hierarchy paths are present without misrepresenting structure
        let paths: Vec<Option<String>> = preview_nb_nested
            .notes
            .iter()
            .map(|n| n.notebook_path.clone())
            .collect();
        assert!(paths.contains(&Some("Engineering".to_string())));
        assert!(paths.contains(&Some("Engineering / Backend".to_string())));
        assert!(paths.contains(&Some("Engineering / Backend / Storage".to_string())));

        // 7. Test Batch Export: Structured JSON
        let temp_dir =
            std::env::temp_dir().join(format!("test_task15_export_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let json_dest_file = temp_dir.join("engineering_notes.json");

        let batch_json_res = ImportExportService::export_notes_batch(
            &conn,
            ExportBatchRequest {
                operation_id: None,
                scope: Some(ExportScope::Notebook {
                    notebook_id: root_nb.id.clone(),
                    include_nested: true,
                }),
                note_ids: vec![],
                format: ExportFormat::Json,
                destination_dir: temp_dir.to_string_lossy().to_string(),
                destination_file: Some(json_dest_file.to_string_lossy().to_string()),
                include_metadata: true,
                overwrite: false,
            },
        )
        .unwrap();

        assert_eq!(batch_json_res.exported_count, 3);
        assert_eq!(batch_json_res.skipped_count, 1); // 1 trashed skipped
        assert!(json_dest_file.exists());

        let read_json = std::fs::read_to_string(&json_dest_file).unwrap();
        let parsed: InterchangeDocument = serde_json::from_str(&read_json).unwrap();
        assert_eq!(parsed.app, "personal-notepad");
        assert_eq!(parsed.version, "1.0");
        assert_eq!(parsed.note_count, 3);
        assert_eq!(parsed.notes.len(), 3);

        // 8. Test Batch Export: Plain text files to folder with collision handling
        // Create duplicate note with same title
        let _note_duplicate = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Engineering Overview".to_string()),
                content: Some("Duplicate title content.".to_string()),
                notebook_id: Some(root_nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let txt_dest_dir = temp_dir.join("txt_exports");
        let batch_txt_res = ImportExportService::export_notes_batch(
            &conn,
            ExportBatchRequest {
                operation_id: None,
                scope: Some(ExportScope::Notebook {
                    notebook_id: root_nb.id.clone(),
                    include_nested: false,
                }),
                note_ids: vec![],
                format: ExportFormat::Txt,
                destination_dir: txt_dest_dir.to_string_lossy().to_string(),
                destination_file: None,
                include_metadata: true,
                overwrite: false,
            },
        )
        .unwrap();

        assert_eq!(batch_txt_res.exported_count, 2);
        // Verify collision was handled predictably with (1) suffix
        assert!(txt_dest_dir.join("Engineering Overview.txt").exists());
        assert!(txt_dest_dir.join("Engineering Overview (1).txt").exists());

        // 9. Zero Side Effects: verify database notes are intact and not modified
        let orig_root_note = NoteRepository::get_by_id(&conn, &note_root.id)
            .unwrap()
            .unwrap();
        assert_eq!(orig_root_note.title, note_root.title);
        assert_eq!(orig_root_note.content, note_root.content);
        assert!(!orig_root_note.is_deleted);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_16_structured_multi_note_export_suite() {
        use crate::storage::database::init_connection;
        use crate::storage::migrations::run_migrations;

        let mut conn = init_connection(std::path::Path::new(":memory:")).unwrap();
        run_migrations(&mut conn).unwrap();

        let temp_dir =
            std::env::temp_dir().join(format!("notepad_task_16_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // 1. Setup Notebooks and Tags
        let arch_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Architecture".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        let design_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Design".to_string(),
                parent_id: Some(arch_nb.id.clone()),
            },
        )
        .unwrap();

        let tag_specs = TagRepository::create(&conn, "specs").unwrap();
        let tag_api = TagRepository::create(&conn, "api").unwrap();

        // 2. Setup Note 2 (Database Engine)
        let note_db = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Database Engine".to_string()),
                content: Some("Database engine implementation using SQLite rusqlite.".to_string()),
                notebook_id: Some(design_nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        TagRepository::add_tag_to_note(&conn, &note_db.id, &tag_specs.id).unwrap();

        // 3. Setup Note 1 (System Design)
        // Links to Note 2 (included in batch) and an unexported note (external ID)
        let external_uuid = uuid::Uuid::new_v4().to_string();
        let note_sys = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("System Design".to_string()),
                content: Some(format!(
                    "System overview linking to [DB](notepad://note/{}) and unexported [Archive](notepad://note/{})",
                    note_db.id, external_uuid
                )),
                notebook_id: Some(design_nb.id.clone()),
                ..Default::default()
            },
        ).unwrap();
        TagRepository::add_tag_to_note(&conn, &note_sys.id, &tag_specs.id).unwrap();
        TagRepository::add_tag_to_note(&conn, &note_sys.id, &tag_api.id).unwrap();

        // 4. Attachments for Note 1
        AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note_sys.id.clone(),
                file_name: "architecture_diagram.png".to_string(),
                relative_path: "sys/architecture_diagram.png".to_string(),
                mime_type: Some("image/png".to_string()),
                file_size: 2048,
            },
        )
        .unwrap();
        AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note_sys.id.clone(),
                file_name: "spec_sheet.pdf".to_string(),
                relative_path: "sys/spec_sheet.pdf".to_string(),
                mime_type: Some("application/pdf".to_string()),
                file_size: 8192,
            },
        )
        .unwrap();

        // 5. Test Batch Export: Structured JSON
        let json_dest_file = temp_dir.join("structured_export.json");
        let batch_res = ImportExportService::export_notes_batch(
            &conn,
            ExportBatchRequest {
                operation_id: None,
                scope: Some(ExportScope::SelectedNotes {
                    note_ids: vec![note_sys.id.clone(), note_db.id.clone()],
                }),
                note_ids: vec![],
                format: ExportFormat::Json,
                destination_dir: temp_dir.to_string_lossy().to_string(),
                destination_file: Some(json_dest_file.to_string_lossy().to_string()),
                include_metadata: true,
                overwrite: false,
            },
        )
        .unwrap();

        assert_eq!(batch_res.exported_count, 2);
        assert_eq!(batch_res.failed_count, 0);
        assert!(json_dest_file.exists());

        // 6. Verify Document Structure & Schema Invariants
        let json_bytes = std::fs::read(&json_dest_file).unwrap();
        let doc: InterchangeDocument = serde_json::from_slice(&json_bytes).unwrap();

        assert_eq!(doc.app, "personal-notepad");
        assert_eq!(doc.format, "personal-notepad-interchange");
        assert_eq!(doc.version, "1.0");
        assert_eq!(doc.note_count, 2);
        assert!(doc
            .internal_link_policy
            .as_ref()
            .unwrap()
            .contains("Package-scoped internal links"));
        assert!(doc
            .attachment_policy
            .as_ref()
            .unwrap()
            .contains("attachments are omitted"));

        // Locate Note 1 and Note 2 in exported JSON
        let exp_note_sys = doc
            .notes
            .iter()
            .find(|n| n.export_id == note_sys.id)
            .unwrap();
        let exp_note_db = doc
            .notes
            .iter()
            .find(|n| n.export_id == note_db.id)
            .unwrap();

        // 7. Verify Deterministic Sorting of Tags
        assert_eq!(
            exp_note_sys.tags,
            vec!["api".to_string(), "specs".to_string()]
        );
        assert_eq!(exp_note_db.tags, vec!["specs".to_string()]);

        // 8. Verify Omitted Attachments Accounting
        assert_eq!(exp_note_sys.attachments_omitted_count, Some(2));
        assert_eq!(exp_note_db.attachments_omitted_count, None);

        // 9. Verify Internal Links Analysis
        // Note 1 has 2 links: one to Note 2 (in package), one to external_uuid (outside package)
        assert_eq!(exp_note_sys.internal_links.len(), 2);
        let link_to_db = exp_note_sys
            .internal_links
            .iter()
            .find(|l| l.target_export_id == note_db.id)
            .unwrap();
        assert!(
            link_to_db.is_package_internal,
            "Link to note in package must be marked package-internal"
        );

        let link_to_ext = exp_note_sys
            .internal_links
            .iter()
            .find(|l| l.target_export_id == external_uuid)
            .unwrap();
        assert!(
            !link_to_ext.is_package_internal,
            "Link to note outside package must be marked external (not portable)"
        );

        // 10. Verify Single Note Export Internal Link Isolation
        // When Note 1 is exported individually, its link to Note 2 MUST NOT be marked package-internal
        let single_dest = temp_dir.join("single_sys.json");
        ImportExportService::export_single_note(
            &conn,
            ExportSingleNoteRequest {
                note_id: note_sys.id.clone(),
                format: ExportFormat::Json,
                destination_path: single_dest.to_string_lossy().to_string(),
                overwrite: false,
            },
        )
        .unwrap();

        let single_bytes = std::fs::read(&single_dest).unwrap();
        let single_doc: InterchangeDocument = serde_json::from_slice(&single_bytes).unwrap();
        let single_note = &single_doc.notes[0];
        let link_db_in_single = single_note
            .internal_links
            .iter()
            .find(|l| l.target_export_id == note_db.id)
            .unwrap();
        assert!(
            !link_db_in_single.is_package_internal,
            "In single-note export, link to another note must not be marked internal"
        );

        // 11. Verify Roundtrip Validation & Import into Fresh Database
        let mut fresh_conn = init_connection(std::path::Path::new(":memory:")).unwrap();
        run_migrations(&mut fresh_conn).unwrap();
        let validated =
            ImportExportService::read_and_validate_file(&json_dest_file, ImportFormat::Json)
                .unwrap();
        assert!(validated.interchange_doc.is_some());

        let raw_paths = vec![json_dest_file.to_string_lossy().to_string()];
        let preview = ImportExportService::scan_candidates(&fresh_conn, &raw_paths).unwrap();
        assert_eq!(preview.candidates.len(), 1);
        // Candidate preview should reflect warnings about omitted attachments and external links
        assert!(preview.candidates[0]
            .warnings
            .iter()
            .any(|w| w.contains("omit 2 attachment(s)")));
        assert!(preview.candidates[0]
            .warnings
            .iter()
            .any(|w| w.contains("1 link(s) to notes outside")));

        let candidate_overrides: Vec<ImportCandidateOverride> = preview
            .candidates
            .iter()
            .map(|c| ImportCandidateOverride {
                id: c.id.clone(),
                file_path: c.file_path.clone(),
                selected: true,
                custom_title: None,
            })
            .collect();

        let import_summary = ImportExportService::execute_import(
            &mut fresh_conn,
            ImportRequest {
                operation_id: None,
                candidates: candidate_overrides,
                options: ImportOptions {
                    target_notebook_id: None,
                    duplicate_policy: DuplicatePolicy::ImportAsNew,
                },
            },
        )
        .unwrap();

        assert_eq!(import_summary.imported_count, 1);
        let imported_notes = NoteRepository::list(&fresh_conn, false).unwrap();
        assert_eq!(imported_notes.len(), 2);

        // Find re-imported notes in fresh database
        let fresh_sys = imported_notes
            .iter()
            .find(|n| n.title == "System Design")
            .unwrap();
        let fresh_db = imported_notes
            .iter()
            .find(|n| n.title == "Database Engine")
            .unwrap();

        // Check internal link rewriting: link to Note 2 rewritten to fresh_db.id!
        assert!(fresh_sys
            .content
            .contains(&format!("notepad://note/{}", fresh_db.id)));
        // External link remains unmapped:
        assert!(fresh_sys
            .content
            .contains(&format!("notepad://note/{}", external_uuid)));

        // 12. Verify Nesting Depth Enforcement (Max 3 levels)
        let deep_json = temp_dir.join("deep_nesting.json");
        let invalid_deep_doc = InterchangeDocument {
            schema: None,
            app: "personal-notepad".to_string(),
            format: "personal-notepad-interchange".to_string(),
            version: "1.0".to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            note_count: 1,
            internal_link_policy: None,
            attachment_policy: None,
            notes: vec![InterchangeNote {
                export_id: "deep-1".to_string(),
                title: "Deep Note".to_string(),
                content: "Content".to_string(),
                format: "txt".to_string(),
                notebook_path: Some("Level1/Level2/Level3/Level4".to_string()),
                tags: vec![],
                is_favorite: false,
                is_pinned: false,
                created_at: None,
                modified_at: None,
                attachments_omitted_count: None,
                internal_links: vec![],
            }],
        };
        std::fs::write(
            &deep_json,
            serde_json::to_vec_pretty(&invalid_deep_doc).unwrap(),
        )
        .unwrap();

        let deep_err = ImportExportService::read_and_validate_file(&deep_json, ImportFormat::Json)
            .unwrap_err();
        assert!(deep_err
            .to_string()
            .contains("exceeding maximum nesting depth of 3 levels"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_17_safe_filenames_and_collision_handling_suite() {
        use crate::storage::database::init_connection;
        use crate::storage::migrations::run_migrations;
        use crate::storage::models::{CreateNoteDto, CreateNotebookDto};

        let mut conn = init_connection(std::path::Path::new(":memory:")).unwrap();
        run_migrations(&mut conn).unwrap();

        let temp_dir =
            std::env::temp_dir().join(format!("notepad_task_17_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // 1. Sanitization: Illegal Characters & Control Characters
        let dirty_title =
            "Report\u{0000}\u{001F} <2026> : \"Confidential\" / *Draft* | Status? \\ Final";
        let sanitized_dirty = ImportExportService::sanitize_filename(dirty_title);
        assert_eq!(
            sanitized_dirty,
            "Report 2026  Confidential  Draft  Status  Final"
        );

        // 2. Sanitization: Leading & Trailing Dots and Spaces
        assert_eq!(
            ImportExportService::sanitize_filename(".hidden_note"),
            "hidden_note"
        );
        assert_eq!(
            ImportExportService::sanitize_filename("...triple_dot"),
            "triple_dot"
        );
        assert_eq!(
            ImportExportService::sanitize_filename("note_trailing_dot."),
            "note_trailing_dot"
        );
        assert_eq!(
            ImportExportService::sanitize_filename("note_trailing_dots...   "),
            "note_trailing_dots"
        );
        assert_eq!(ImportExportService::sanitize_filename("."), "Untitled Note");
        assert_eq!(
            ImportExportService::sanitize_filename(".."),
            "Untitled Note"
        );
        assert_eq!(
            ImportExportService::sanitize_filename("..."),
            "Untitled Note"
        );
        assert_eq!(
            ImportExportService::sanitize_filename("   "),
            "Untitled Note"
        );
        assert_eq!(ImportExportService::sanitize_filename(""), "Untitled Note");

        // 3. Sanitization: Windows Reserved Device Names
        assert_eq!(ImportExportService::sanitize_filename("CON"), "_CON");
        assert_eq!(ImportExportService::sanitize_filename("con"), "_con");
        assert_eq!(ImportExportService::sanitize_filename("PRN"), "_PRN");
        assert_eq!(ImportExportService::sanitize_filename("aux"), "_aux");
        assert_eq!(ImportExportService::sanitize_filename("NUL"), "_NUL");
        assert_eq!(ImportExportService::sanitize_filename("COM1"), "_COM1");
        assert_eq!(ImportExportService::sanitize_filename("com9"), "_com9");
        assert_eq!(ImportExportService::sanitize_filename("lpt1"), "_lpt1");
        assert_eq!(ImportExportService::sanitize_filename("LPT9"), "_LPT9");
        // Non-reserved names that happen to start or end with reserved substrings must not be altered
        assert_eq!(
            ImportExportService::sanitize_filename("Conference Notes"),
            "Conference Notes"
        );
        assert_eq!(ImportExportService::sanitize_filename("Pronto"), "Pronto");
        assert_eq!(
            ImportExportService::sanitize_filename("Auxiliary Motor"),
            "Auxiliary Motor"
        );
        assert_eq!(ImportExportService::sanitize_filename("Nullify"), "Nullify");

        // 4. Sanitization: Unicode Preservation
        let unicode_title = "Notes 🚀 2026 日本語 हिन्दी Русский";
        assert_eq!(
            ImportExportService::sanitize_filename(unicode_title),
            unicode_title
        );

        // 5. Sanitization: Length Limit Truncation (180 chars)
        let long_title = "a".repeat(250);
        let sanitized_long = ImportExportService::sanitize_filename(&long_title);
        assert_eq!(sanitized_long.chars().count(), 180);

        // 6. Path Validation: Traversal & Empty & Internal Database
        assert!(matches!(
            ImportExportService::validate_destination_path(Path::new("")),
            Err(StorageError::Validation(_))
        ));
        assert!(matches!(
            ImportExportService::validate_destination_path(Path::new("   ")),
            Err(StorageError::Validation(_))
        ));
        assert!(matches!(
            ImportExportService::validate_destination_path(Path::new("folder/../escape.txt")),
            Err(StorageError::Validation(_))
        ));
        assert!(matches!(
            ImportExportService::validate_destination_path(Path::new("../escape.txt")),
            Err(StorageError::Validation(_))
        ));
        assert!(matches!(
            ImportExportService::validate_destination_path(Path::new("folder\\..\\escape.txt")),
            Err(StorageError::Validation(_))
        ));
        assert!(matches!(
            ImportExportService::validate_destination_path(Path::new("..\\escape.txt")),
            Err(StorageError::Validation(_))
        ));
        assert!(matches!(
            ImportExportService::validate_destination_path(Path::new("database.sqlite")),
            Err(StorageError::Validation(_))
        ));
        assert!(matches!(
            ImportExportService::validate_destination_path(Path::new("/some/path/database.sqlite")),
            Err(StorageError::Validation(_))
        ));
        assert!(matches!(
            ImportExportService::validate_destination_path(Path::new(
                "/some/path/database.sqlite-wal"
            )),
            Err(StorageError::Validation(_))
        ));
        assert!(
            ImportExportService::validate_destination_path(Path::new("/valid/path/note.txt"))
                .is_ok()
        );

        // 7. Single Note Export: Destination Collision Handling
        let sample_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Collision Note".to_string()),
                content: Some("First version content.".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .unwrap();

        let single_file_path = temp_dir.join("single_collision.txt");
        std::fs::write(&single_file_path, "Pre-existing content on disk").unwrap();

        // 7a. Overwrite = false returns Conflict error
        let conflict_err = ImportExportService::export_single_note(
            &conn,
            ExportSingleNoteRequest {
                note_id: sample_note.id.clone(),
                format: ExportFormat::Txt,
                destination_path: single_file_path.to_string_lossy().to_string(),
                overwrite: false,
            },
        )
        .unwrap_err();
        assert!(matches!(conflict_err, StorageError::Conflict(_)));
        assert_eq!(
            std::fs::read_to_string(&single_file_path).unwrap(),
            "Pre-existing content on disk"
        );

        // 7b. Overwrite = true succeeds and replaces existing file
        let overwrite_res = ImportExportService::export_single_note(
            &conn,
            ExportSingleNoteRequest {
                note_id: sample_note.id.clone(),
                format: ExportFormat::Txt,
                destination_path: single_file_path.to_string_lossy().to_string(),
                overwrite: true,
            },
        )
        .unwrap();
        assert_eq!(overwrite_res.status, ExportItemStatus::Exported);
        assert_eq!(
            std::fs::read_to_string(&single_file_path).unwrap(),
            "First version content."
        );

        // 7c. Export to directory path returns Validation error
        let dir_err = ImportExportService::export_single_note(
            &conn,
            ExportSingleNoteRequest {
                note_id: sample_note.id.clone(),
                format: ExportFormat::Txt,
                destination_path: temp_dir.to_string_lossy().to_string(),
                overwrite: true,
            },
        )
        .unwrap_err();
        assert!(matches!(dir_err, StorageError::Validation(_)));

        // 8. Batch Export TXT/MD: In-Batch and On-Disk Collision Avoidance
        let batch_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "BatchNB".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        let b_note_1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Team Meeting".to_string()),
                content: Some("First team meeting".to_string()),
                format: Some("txt".to_string()),
                notebook_id: Some(batch_nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let b_note_2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Team Meeting".to_string()),
                content: Some("Second team meeting with same title".to_string()),
                format: Some("txt".to_string()),
                notebook_id: Some(batch_nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let b_note_traversal = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("../../escape_plan".to_string()),
                content: Some("Traversal attempt content".to_string()),
                format: Some("txt".to_string()),
                notebook_id: Some(batch_nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let txt_batch_dir = temp_dir.join("batch_collision_txt");
        std::fs::create_dir_all(&txt_batch_dir).unwrap();
        // Create pre-existing file on disk for Team Meeting
        std::fs::write(
            txt_batch_dir.join("Team Meeting.txt"),
            "Pre-existing on disk",
        )
        .unwrap();

        let batch_txt_summary = ImportExportService::export_notes_batch(
            &conn,
            ExportBatchRequest {
                operation_id: None,
                scope: Some(ExportScope::SelectedNotes {
                    note_ids: vec![
                        b_note_1.id.clone(),
                        b_note_2.id.clone(),
                        b_note_traversal.id.clone(),
                    ],
                }),
                note_ids: vec![],
                format: ExportFormat::Txt,
                destination_dir: txt_batch_dir.to_string_lossy().to_string(),
                destination_file: None,
                include_metadata: false,
                overwrite: false,
            },
        )
        .unwrap();

        assert_eq!(batch_txt_summary.exported_count, 3);
        // Pre-existing file on disk remains intact:
        assert_eq!(
            std::fs::read_to_string(txt_batch_dir.join("Team Meeting.txt")).unwrap(),
            "Pre-existing on disk"
        );
        // Note 1 gets (1) suffix because disk file exists
        assert!(txt_batch_dir.join("Team Meeting (1).txt").exists());
        assert_eq!(
            std::fs::read_to_string(txt_batch_dir.join("Team Meeting (1).txt")).unwrap(),
            "First team meeting"
        );
        // Note 2 gets (2) suffix because Note 1 took (1)
        assert!(txt_batch_dir.join("Team Meeting (2).txt").exists());
        assert_eq!(
            std::fs::read_to_string(txt_batch_dir.join("Team Meeting (2).txt")).unwrap(),
            "Second team meeting with same title"
        );
        // Traversal attempt is sanitized to escape_plan.txt inside dest_dir
        assert!(txt_batch_dir.join("escape_plan.txt").exists());
        assert!(
            !temp_dir.join("escape_plan.txt").exists(),
            "Must not have escaped destination directory"
        );

        // 9. Batch Export JSON: File Collision Handling
        let json_target_file = temp_dir.join("batch_export.json");
        std::fs::write(&json_target_file, "Pre-existing JSON").unwrap();

        // 9a. Explicit destination_file already exists and overwrite = false -> Conflict
        let json_conflict_err = ImportExportService::export_notes_batch(
            &conn,
            ExportBatchRequest {
                operation_id: None,
                scope: Some(ExportScope::SelectedNotes {
                    note_ids: vec![sample_note.id.clone()],
                }),
                note_ids: vec![],
                format: ExportFormat::Json,
                destination_dir: temp_dir.to_string_lossy().to_string(),
                destination_file: Some(json_target_file.to_string_lossy().to_string()),
                include_metadata: true,
                overwrite: false,
            },
        )
        .unwrap_err();
        assert!(matches!(json_conflict_err, StorageError::Conflict(_)));
        assert_eq!(
            std::fs::read_to_string(&json_target_file).unwrap(),
            "Pre-existing JSON"
        );

        // 9b. Explicit destination_file already exists and overwrite = true -> Replaced
        let json_overwrite_res = ImportExportService::export_notes_batch(
            &conn,
            ExportBatchRequest {
                operation_id: None,
                scope: Some(ExportScope::SelectedNotes {
                    note_ids: vec![sample_note.id.clone()],
                }),
                note_ids: vec![],
                format: ExportFormat::Json,
                destination_dir: temp_dir.to_string_lossy().to_string(),
                destination_file: Some(json_target_file.to_string_lossy().to_string()),
                include_metadata: true,
                overwrite: true,
            },
        )
        .unwrap();
        assert_eq!(json_overwrite_res.exported_count, 1);
        let overwritten_json = std::fs::read_to_string(&json_target_file).unwrap();
        assert!(overwritten_json.contains("Collision Note"));

        // 9c. Default export.json in directory already exists -> Automatically increments to export (1).json
        let auto_json_dir = temp_dir.join("auto_json_dir");
        std::fs::create_dir_all(&auto_json_dir).unwrap();
        std::fs::write(
            auto_json_dir.join("export.json"),
            "Pre-existing default export",
        )
        .unwrap();

        let auto_json_res = ImportExportService::export_notes_batch(
            &conn,
            ExportBatchRequest {
                operation_id: None,
                scope: Some(ExportScope::SelectedNotes {
                    note_ids: vec![sample_note.id.clone()],
                }),
                note_ids: vec![],
                format: ExportFormat::Json,
                destination_dir: auto_json_dir.to_string_lossy().to_string(),
                destination_file: None,
                include_metadata: true,
                overwrite: false,
            },
        )
        .unwrap();
        assert_eq!(auto_json_res.exported_count, 1);
        assert_eq!(
            std::fs::read_to_string(auto_json_dir.join("export.json")).unwrap(),
            "Pre-existing default export"
        );
        assert!(auto_json_dir.join("export (1).json").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_18_safe_and_reliable_file_writing_suite() {
        use crate::storage::database::init_connection;
        use crate::storage::migrations::run_migrations;
        use crate::storage::models::CreateNoteDto;

        let mut conn = init_connection(std::path::Path::new(":memory:")).unwrap();
        run_migrations(&mut conn).unwrap();

        let temp_dir =
            std::env::temp_dir().join(format!("notepad_task_18_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // 1. Direct write_file_atomically: creates file and parent directories
        let nested_dest = temp_dir.join("nested/folder/atomic_note.txt");
        let initial_payload = b"Atomic export content version 1";
        ImportExportService::write_file_atomically(&nested_dest, initial_payload).unwrap();
        assert_eq!(std::fs::read(&nested_dest).unwrap(), initial_payload);

        // Verify no .tmp_* or *.part files remain
        let parent_dir = nested_dest.parent().unwrap();
        let entries: Vec<_> = std::fs::read_dir(parent_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], "atomic_note.txt");
        assert!(!entries.iter().any(|name| name.starts_with(".tmp_")));

        // 2. Atomic replacement: overwrites existing file safely
        let updated_payload = b"Atomic export content version 2 (updated)";
        ImportExportService::write_file_atomically(&nested_dest, updated_payload).unwrap();
        assert_eq!(std::fs::read(&nested_dest).unwrap(), updated_payload);

        // 3. Error safety: existing file is preserved if write fails
        let protected_file = temp_dir.join("protected_note.txt");
        let safe_original_content = "Critical original document content.";
        std::fs::write(&protected_file, safe_original_content).unwrap();

        // Attempting to treat a file as a directory should fail
        let invalid_dest = protected_file.join("sub_invalid.txt");
        let err =
            ImportExportService::write_file_atomically(&invalid_dest, b"Should fail").unwrap_err();
        assert!(matches!(
            err,
            StorageError::Io(_) | StorageError::Validation(_)
        ));
        // Original file must remain completely intact
        assert_eq!(
            std::fs::read_to_string(&protected_file).unwrap(),
            safe_original_content
        );

        // 4. Reject directory destinations
        let dir_dest_err =
            ImportExportService::write_file_atomically(&temp_dir, b"Data").unwrap_err();
        assert!(matches!(dir_dest_err, StorageError::Validation(_)));

        // 5. Single Note Export via Service: verifies end-to-end atomic write
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Reliable Note".to_string()),
                content: Some("# Reliable Note\n\nVerbatim export content.".to_string()),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let single_dest = temp_dir.join("reliable_note.md");
        let export_res = ImportExportService::export_single_note(
            &conn,
            ExportSingleNoteRequest {
                note_id: note.id.clone(),
                format: ExportFormat::Md,
                destination_path: single_dest.to_string_lossy().to_string(),
                overwrite: false,
            },
        )
        .unwrap();

        assert_eq!(export_res.status, ExportItemStatus::Exported);
        assert_eq!(
            std::fs::read_to_string(&single_dest).unwrap(),
            "# Reliable Note\n\nVerbatim export content."
        );

        // 6. Database Immutability Invariant:
        // Verifies the database is NEVER mutated merely because an export file was written
        let note_in_db = NoteRepository::get_by_id(&conn, &note.id).unwrap().unwrap();
        assert_eq!(note_in_db.title, note.title);
        assert_eq!(note_in_db.content, note.content);
        assert_eq!(note_in_db.modified_at, note.modified_at);
        assert_eq!(note_in_db.created_at, note.created_at);

        // 7. Multi-Note Batch Export: Independent commitment
        let note_2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Second Reliable Note".to_string()),
                content: Some("Second note content.".to_string()),
                format: Some("txt".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let batch_dest_dir = temp_dir.join("batch_reliable_txt");
        let batch_res = ImportExportService::export_notes_batch(
            &conn,
            ExportBatchRequest {
                operation_id: None,
                scope: Some(ExportScope::SelectedNotes {
                    note_ids: vec![note.id.clone(), note_2.id.clone()],
                }),
                note_ids: vec![],
                format: ExportFormat::Txt,
                destination_dir: batch_dest_dir.to_string_lossy().to_string(),
                destination_file: None,
                include_metadata: false,
                overwrite: false,
            },
        )
        .unwrap();

        assert_eq!(batch_res.exported_count, 2);
        assert_eq!(batch_res.failed_count, 0);
        assert!(batch_dest_dir.join("Reliable Note.txt").exists());
        assert!(batch_dest_dir.join("Second Reliable Note.txt").exists());

        // Verify no leftover .tmp_* files
        let batch_entries: Vec<_> = std::fs::read_dir(&batch_dest_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(batch_entries.len(), 2);
        assert!(!batch_entries.iter().any(|name| name.starts_with(".tmp_")));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_19_progress_cancellation_and_summaries_suite() {
        use crate::storage::database::init_connection;
        use crate::storage::migrations::run_migrations;
        use crate::storage::models::{
            CreateNoteDto, DuplicatePolicy, ExportBatchRequest, ExportFormat, ExportItemStatus,
            ExportScope, ImportCandidateOverride, ImportItemStatus, ImportOptions, ImportRequest,
            ImportSummary, OperationOutcome,
        };

        // 1. Cancellation registry functions test
        let op_id = "op-cancel-test-123";
        assert!(!ImportExportService::is_operation_cancelled(Some(op_id)));
        assert!(ImportExportService::cancel_operation(op_id));
        assert!(ImportExportService::is_operation_cancelled(Some(op_id)));
        ImportExportService::clear_operation(Some(op_id));
        assert!(!ImportExportService::is_operation_cancelled(Some(op_id)));

        // 2. OperationOutcome calculation categorization tests
        // Full Success
        let s_full = ImportSummary {
            total_processed: 2,
            imported_count: 2,
            skipped_count: 0,
            failed_count: 0,
            cancelled_count: 0,
            outcome: None,
            results: vec![],
            duration_ms: 10,
        };
        assert_eq!(s_full.calculate_outcome(), OperationOutcome::FullSuccess);

        // Success with warnings
        let s_warn = ImportSummary {
            total_processed: 2,
            imported_count: 1,
            skipped_count: 1,
            failed_count: 0,
            cancelled_count: 0,
            outcome: None,
            results: vec![],
            duration_ms: 10,
        };
        assert_eq!(
            s_warn.calculate_outcome(),
            OperationOutcome::SuccessWithWarnings
        );

        // Partial Success
        let s_partial = ImportSummary {
            total_processed: 2,
            imported_count: 1,
            skipped_count: 0,
            failed_count: 1,
            cancelled_count: 0,
            outcome: None,
            results: vec![],
            duration_ms: 10,
        };
        assert_eq!(
            s_partial.calculate_outcome(),
            OperationOutcome::PartialSuccess
        );

        // Total Failure
        let s_fail = ImportSummary {
            total_processed: 2,
            imported_count: 0,
            skipped_count: 0,
            failed_count: 2,
            cancelled_count: 0,
            outcome: None,
            results: vec![],
            duration_ms: 10,
        };
        assert_eq!(s_fail.calculate_outcome(), OperationOutcome::TotalFailure);

        // Cancelled
        let s_cancel = ImportSummary {
            total_processed: 2,
            imported_count: 1,
            skipped_count: 0,
            failed_count: 0,
            cancelled_count: 1,
            outcome: None,
            results: vec![],
            duration_ms: 10,
        };
        assert_eq!(s_cancel.calculate_outcome(), OperationOutcome::Cancelled);

        // 3. Import cancellation and progress tracking test
        let mut conn = init_connection(std::path::Path::new(":memory:")).unwrap();
        run_migrations(&mut conn).unwrap();

        let temp_dir =
            std::env::temp_dir().join(format!("notepad_task_19_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_1 = temp_dir.join("file_1.txt");
        let file_2 = temp_dir.join("file_2.txt");
        let file_3 = temp_dir.join("file_3.txt");
        std::fs::write(&file_1, "Note 1 content").unwrap();
        std::fs::write(&file_2, "Note 2 content").unwrap();
        std::fs::write(&file_3, "Note 3 content").unwrap();

        let import_op_id = format!("import-op-{}", uuid::Uuid::new_v4());
        let mut import_progress_events = Vec::new();

        let import_req = ImportRequest {
            operation_id: Some(import_op_id.clone()),
            candidates: vec![
                ImportCandidateOverride {
                    id: "cand-1".to_string(),
                    file_path: file_1.to_string_lossy().to_string(),
                    selected: true,
                    custom_title: Some("Imported First".to_string()),
                },
                ImportCandidateOverride {
                    id: "cand-2".to_string(),
                    file_path: file_2.to_string_lossy().to_string(),
                    selected: true,
                    custom_title: Some("Imported Second".to_string()),
                },
                ImportCandidateOverride {
                    id: "cand-3".to_string(),
                    file_path: file_3.to_string_lossy().to_string(),
                    selected: true,
                    custom_title: Some("Imported Third".to_string()),
                },
            ],
            options: ImportOptions {
                target_notebook_id: None,
                duplicate_policy: DuplicatePolicy::ImportAsNew,
            },
        };

        let op_clone = import_op_id.clone();
        let import_summary =
            ImportExportService::execute_import_with_progress(&mut conn, import_req, |prog| {
                import_progress_events.push(prog.clone());
                // Trigger cancellation after first file is processed
                if prog.current_index == 1 && prog.imported_count == 1 {
                    ImportExportService::cancel_operation(&op_clone);
                }
            })
            .unwrap();

        assert_eq!(import_summary.imported_count, 1);
        assert_eq!(import_summary.cancelled_count, 2);
        assert_eq!(import_summary.failed_count, 0);
        assert_eq!(import_summary.outcome, Some(OperationOutcome::Cancelled));
        assert_eq!(import_summary.results.len(), 3);
        assert_eq!(import_summary.results[0].status, ImportItemStatus::Imported);
        assert_eq!(
            import_summary.results[1].status,
            ImportItemStatus::Cancelled
        );
        assert_eq!(
            import_summary.results[2].status,
            ImportItemStatus::Cancelled
        );
        assert!(!import_progress_events.is_empty());

        // Safe boundary verification: Note 1 must remain intact and committed in database
        let imported_note = NoteRepository::get_by_id(
            &conn,
            import_summary.results[0].created_note_id.as_ref().unwrap(),
        )
        .unwrap();
        assert!(imported_note.is_some());
        assert_eq!(imported_note.unwrap().title, "Imported First");

        // Notes 2 and 3 must not exist in DB
        let all_notes = NoteRepository::list(&conn, false).unwrap();
        assert_eq!(all_notes.len(), 1);

        // 4. Batch export cancellation and progress tracking test
        let note_a = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note Alpha".to_string()),
                content: Some("Alpha content".to_string()),
                format: Some("txt".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note Beta".to_string()),
                content: Some("Beta content".to_string()),
                format: Some("txt".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let note_c = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note Gamma".to_string()),
                content: Some("Gamma content".to_string()),
                format: Some("txt".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let export_dest_dir = temp_dir.join("export_cancel_dir");
        let export_op_id = format!("export-op-{}", uuid::Uuid::new_v4());
        let mut export_progress_events = Vec::new();

        let op_clone_exp = export_op_id.clone();
        let export_summary = ImportExportService::export_notes_batch_with_progress(
            &conn,
            ExportBatchRequest {
                operation_id: Some(export_op_id),
                scope: Some(ExportScope::SelectedNotes {
                    note_ids: vec![note_a.id.clone(), note_b.id.clone(), note_c.id.clone()],
                }),
                note_ids: vec![],
                format: ExportFormat::Txt,
                destination_dir: export_dest_dir.to_string_lossy().to_string(),
                destination_file: None,
                include_metadata: false,
                overwrite: false,
            },
            |prog| {
                export_progress_events.push(prog.clone());
                // Trigger cancellation after first note is exported
                if prog.current_index == 1 && prog.exported_count == 1 {
                    ImportExportService::cancel_operation(&op_clone_exp);
                }
            },
        )
        .unwrap();

        assert_eq!(export_summary.exported_count, 1);
        assert_eq!(export_summary.cancelled_count, 2);
        assert_eq!(export_summary.outcome, Some(OperationOutcome::Cancelled));
        assert_eq!(export_summary.results[0].status, ExportItemStatus::Exported);
        assert_eq!(
            export_summary.results[1].status,
            ExportItemStatus::Cancelled
        );
        assert_eq!(
            export_summary.results[2].status,
            ExportItemStatus::Cancelled
        );

        // Safe boundary verification: Note Alpha file remains intact on disk
        assert!(export_dest_dir.join("Note Alpha.txt").exists());
        assert_eq!(
            std::fs::read_to_string(export_dest_dir.join("Note Alpha.txt")).unwrap(),
            "Alpha content"
        );

        // Cancelled notes must NOT exist on disk
        assert!(!export_dest_dir.join("Note Beta.txt").exists());
        assert!(!export_dest_dir.join("Note Gamma.txt").exists());

        // Database remains unmutated
        assert!(NoteRepository::get_by_id(&conn, &note_b.id)
            .unwrap()
            .is_some());
        assert!(NoteRepository::get_by_id(&conn, &note_c.id)
            .unwrap()
            .is_some());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_12_failure_safe_unit_invariants() {
        let mut conn = setup_test_db();

        let temp_dir =
            std::env::temp_dir().join(format!("test_task12_unit_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // 1. Non-regular file rejection (directories, FIFOs, special devices)
        let sub_dir = temp_dir.join("a_directory");
        std::fs::create_dir_all(&sub_dir).unwrap();
        let read_res = ImportExportService::read_and_validate_file(&sub_dir, ImportFormat::Txt);
        assert!(read_res.is_err());
        assert!(read_res
            .unwrap_err()
            .to_string()
            .contains("not a regular file"));

        // 2. Batch limit check in execute_import (> 500 candidates)
        let oversized_candidates: Vec<ImportCandidateOverride> = (0..501)
            .map(|i| ImportCandidateOverride {
                id: format!("cand-{i}"),
                file_path: format!("/tmp/fake_{i}.txt"),
                selected: true,
                custom_title: Some(format!("Fake {i}")),
            })
            .collect();

        let req = ImportRequest {
            candidates: oversized_candidates,
            options: ImportOptions::default(),
            operation_id: None,
        };
        let err = ImportExportService::execute_import(&mut conn, req).unwrap_err();
        assert!(matches!(err, StorageError::Validation(_)));
        assert!(err
            .to_string()
            .contains("exceeds maximum allowed limit of 500 files"));

        // 3. Destination atomic write to directory error
        let dir_dest = temp_dir.join("dir_target");
        std::fs::create_dir_all(&dir_dest).unwrap();
        let atomic_err =
            ImportExportService::write_file_atomically(&dir_dest, b"test content").unwrap_err();
        assert!(matches!(atomic_err, StorageError::Validation(_)));
        assert!(atomic_err
            .to_string()
            .contains("is a directory, not a file"));

        // 4. Validate destination path traversal across separators
        assert!(
            ImportExportService::validate_destination_path(Path::new("normal/dest/note.txt"))
                .is_ok()
        );
        assert!(
            ImportExportService::validate_destination_path(Path::new("..\\outside.txt")).is_err()
        );
        assert!(
            ImportExportService::validate_destination_path(Path::new("../outside.txt")).is_err()
        );
        assert!(
            ImportExportService::validate_destination_path(Path::new("sub\\..\\outside.txt"))
                .is_err()
        );
        assert!(
            ImportExportService::validate_destination_path(Path::new("sub/../outside.txt"))
                .is_err()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
