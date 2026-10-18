use serde::{Deserialize, Serialize};

/// Strongly typed lifecycle state of a note (Phase 8, Task 2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteLifecycle {
    Active,
    Trashed,
}

impl NoteLifecycle {
    pub fn is_active(&self) -> bool {
        matches!(self, NoteLifecycle::Active)
    }

    pub fn is_trashed(&self) -> bool {
        matches!(self, NoteLifecycle::Trashed)
    }
}

/// Persistent Note model
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub format: String,
    pub notebook_id: Option<String>,
    pub created_at: String,
    pub modified_at: String,
    pub is_favorite: bool,
    pub is_pinned: bool,
    pub is_deleted: bool,
    pub deleted_at: Option<String>,
}

impl Note {
    /// Returns the authoritative NoteLifecycle state derived from is_deleted
    pub fn lifecycle(&self) -> NoteLifecycle {
        if self.is_deleted {
            NoteLifecycle::Trashed
        } else {
            NoteLifecycle::Active
        }
    }
}

/// DTO for creating a new note
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateNoteDto {
    pub title: Option<String>,
    pub content: Option<String>,
    pub format: Option<String>,
    pub notebook_id: Option<String>,
    pub is_favorite: Option<bool>,
    pub is_pinned: Option<bool>,
}

/// DTO for updating an existing note
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateNoteDto {
    pub title: Option<String>,
    pub content: Option<String>,
    pub format: Option<String>,
    pub notebook_id: Option<String>,
    pub is_favorite: Option<bool>,
    pub is_pinned: Option<bool>,
    pub is_deleted: Option<bool>,
}

/// Lightweight aggregate note counts DTO (Phase 13, Task 9)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteCountsDto {
    pub total: usize,
    pub favorites: usize,
    pub unfiled: usize,
    pub trash: usize,
    pub by_notebook: std::collections::HashMap<String, usize>,
}

/// Query options for bounded, paginated note listing (Phase 13, Task 9)
#[derive(Debug, Clone, Default)]
pub struct ListNotesQueryOptions<'a> {
    pub include_deleted: bool,
    pub notebook_id: Option<&'a str>,
    pub unfiled_only: bool,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub content_limit: Option<usize>,
}

/// Persistent Notebook model
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notebook {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub modified_at: String,
}

/// DTO for creating a notebook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotebookDto {
    pub name: String,
    pub parent_id: Option<String>,
}

/// DTO for updating a notebook (rename)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNotebookDto {
    pub name: String,
}

/// Persistent Tag model
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

/// DTO for creating a tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTagDto {
    pub name: String,
}

/// DTO for updating a tag (rename)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTagDto {
    pub name: String,
}

/// Many-to-many link between Note and Tag
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteTag {
    pub note_id: String,
    pub tag_id: String,
}

/// File attachment metadata model (Phase 7, Task 2)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub note_id: String,
    pub file_name: String,
    pub relative_path: String,
    pub mime_type: Option<String>,
    pub file_size: i64,
    pub created_at: String,
    pub modified_at: String,
}

impl Attachment {
    /// Validates essential attachment invariants.
    pub fn is_valid(&self) -> bool {
        !self.id.trim().is_empty()
            && !self.note_id.trim().is_empty()
            && !self.file_name.trim().is_empty()
            && !self.relative_path.trim().is_empty()
            && self.file_size >= 0
    }
}

/// DTO for requesting an attachment to be added to a note
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddAttachmentDto {
    pub note_id: String,
    pub source_path: String,
}

/// DTO for inserting an attachment record into the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAttachmentDto {
    pub note_id: String,
    pub file_name: String,
    pub relative_path: String,
    pub mime_type: Option<String>,
    pub file_size: i64,
}

/// Public frontend-safe attachment metadata DTO
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentMetadataDto {
    pub id: String,
    pub note_id: String,
    pub file_name: String,
    pub mime_type: Option<String>,
    pub file_size: i64,
    pub created_at: String,
    pub modified_at: String,
}

impl From<Attachment> for AttachmentMetadataDto {
    fn from(att: Attachment) -> Self {
        Self {
            id: att.id,
            note_id: att.note_id,
            file_name: att.file_name,
            mime_type: att.mime_type,
            file_size: att.file_size,
            created_at: att.created_at,
            modified_at: att.modified_at,
        }
    }
}

/// Detailed issue when a database attachment points to a missing file on disk (Phase 7, Task 21)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingManagedFileIssue {
    pub attachment_id: String,
    pub note_id: String,
    pub file_name: String,
    pub relative_path: String,
}

/// Detailed issue when recorded attachment size does not match byte size on disk (Phase 7, Task 21)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizeMismatchIssue {
    pub attachment_id: String,
    pub note_id: String,
    pub file_name: String,
    pub relative_path: String,
    pub recorded_size: i64,
    pub actual_size: u64,
}

/// Comprehensive report produced by storage consistency and orphan checks (Phase 7, Task 21)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentConsistencyReport {
    pub missing_managed_files: Vec<MissingManagedFileIssue>,
    pub unreferenced_managed_files: Vec<String>,
    pub abandoned_temp_files: Vec<String>,
    pub size_mismatches: Vec<SizeMismatchIssue>,
    pub total_attachments_checked: usize,
    pub total_disk_files_checked: usize,
    pub is_consistent: bool,
}

/// Application setting key-value pair
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

/// Canonical note metadata representation (Task 29)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub note_id: String,
    pub created_at: String,
    pub modified_at: String,
    pub format: String,
    pub notebook_id: Option<String>,
    pub notebook_path: Option<String>,
    pub is_favorite: bool,
    pub is_pinned: bool,
    pub tags: Vec<Tag>,
    pub word_count: usize,
    pub character_count: usize,
    pub byte_size: usize,
}

impl NoteMetadata {
    pub fn from_note_and_tags(note: &Note, tags: Vec<Tag>) -> Self {
        let content = &note.content;
        let word_count = content.split_whitespace().count();
        let character_count = content.chars().count();
        let byte_size = content.len();

        Self {
            note_id: note.id.clone(),
            created_at: note.created_at.clone(),
            modified_at: note.modified_at.clone(),
            format: note.format.clone(),
            notebook_id: note.notebook_id.clone(),
            notebook_path: None,
            is_favorite: note.is_favorite,
            is_pinned: note.is_pinned,
            tags,
            word_count,
            character_count,
            byte_size,
        }
    }

    /// Returns the user-facing format display name (Task 32: "Plain Text" vs "Markdown").
    pub fn format_display(&self) -> &'static str {
        format_display_name(&self.format)
    }
}

/// Helper returning user-facing display name for note format code (Task 32).
pub fn format_display_name(format: &str) -> &'static str {
    match format.to_ascii_lowercase().as_str() {
        "md" => "Markdown",
        _ => "Plain Text",
    }
}

/// Canonical Search Result model (Phase 6, Task 4, 12, 13)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub note_id: String,
    pub title: String,
    pub snippet: Option<String>,
    pub modified_at: String,
    pub notebook_id: Option<String>,
    pub favorite: bool,
}

/// Search index diagnostic and health status (Phase 13, Task 10)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexDiagnostic {
    pub is_encrypted: bool,
    pub fts_table_exists: bool,
    pub triggers_intact: bool,
    pub integrity_check_passed: bool,
    pub active_notes_count: usize,
    pub fts_entries_count: usize,
    pub is_consistent: bool,
}

/// Lightweight item representation for trashed notes listing (Phase 8, Task 10)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashedNoteItem {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub notebook_id: Option<String>,
    pub notebook_name: Option<String>,
    pub created_at: String,
    pub modified_at: String,
    pub deleted_at: Option<String>,
    pub is_favorite: bool,
    pub attachment_count: usize,
}

/// Typed report of a single permanent deletion operation (Phase 8, Task 7)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermanentDeleteResult {
    pub note_id: String,
    pub success: bool,
    pub attachment_cleanup_failed: bool,
    pub error_message: Option<String>,
}

/// Typed summary of an empty-trash batch operation (Phase 8, Task 8)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyTrashResult {
    pub deleted_count: usize,
    pub failed_count: usize,
    pub attachment_cleanup_failures: Vec<String>,
    pub errors: Vec<String>,
}

/// Strongly typed lifecycle status of an internal link target (Phase 9, Task 3)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkTargetLifecycle {
    Active,
    Trashed,
    Missing,
}

impl LinkTargetLifecycle {
    pub fn is_active(&self) -> bool {
        matches!(self, LinkTargetLifecycle::Active)
    }

    pub fn is_trashed(&self) -> bool {
        matches!(self, LinkTargetLifecycle::Trashed)
    }

    pub fn is_missing(&self) -> bool {
        matches!(self, LinkTargetLifecycle::Missing)
    }
}

/// Representation of a parsed internal link token in note content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedInternalLink {
    pub target_id: String,
    pub label: String,
    pub raw: String,
    pub start_index: usize,
    pub end_index: usize,
}

/// Resolved target metadata for following or inspecting an internal link
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLinkTarget {
    pub target_id: String,
    pub title: Option<String>,
    pub lifecycle: LinkTargetLifecycle,
    pub notebook_id: Option<String>,
    pub notebook_name: Option<String>,
}

/// Lightweight item for searching/picking notes to link to
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkableNoteItem {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub notebook_id: Option<String>,
    pub notebook_name: Option<String>,
    pub modified_at: String,
}

/// Backlink item indicating a source note that contains a link to the target note
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklinkItem {
    pub source_note_id: String,
    pub source_note_title: String,
    pub source_notebook_id: Option<String>,
    pub source_notebook_name: Option<String>,
    pub label_snippet: Option<String>,
    pub modified_at: String,
}

/// Supported formats for importing notes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportFormat {
    Txt,
    Md,
    Json,
}

/// Supported formats for exporting notes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Txt,
    Md,
    Json,
    /// One ZIP archive: a folder per note with its attachments in `assets/`
    Zip,
}

/// Duplicate resolution policy during import
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    #[default]
    ImportAsNew,
    SkipExact,
    Ask,
}

/// Detailed classification of duplicate check results against active database notes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DuplicateMatch {
    ExactMatch {
        existing_id: String,
        existing_title: String,
    },
    ContentMatch {
        existing_id: String,
        existing_title: String,
    },
    TitleCollisionOnly {
        existing_id: String,
        existing_title: String,
    },
    None,
}

/// Validation status of an import candidate file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationStatus {
    Valid,
    Warning,
    Invalid,
}

/// A scanned candidate file ready for user inspection in preview
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub detected_format: ImportFormat,
    pub proposed_title: String,
    pub validation_status: ValidationStatus,
    pub warnings: Vec<String>,
    pub error_message: Option<String>,
    pub is_duplicate: bool,
    pub duplicate_note_id: Option<String>,
    pub duplicate_note_title: Option<String>,
    pub selected: bool,
}

/// Batch preview result returned after scanning candidate files
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub candidates: Vec<ImportCandidate>,
    pub total_files: usize,
    pub valid_files: usize,
    pub invalid_files: usize,
    pub duplicate_files: usize,
    pub total_size_bytes: u64,
}

/// Options configured by the user in the import preview modal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    pub target_notebook_id: Option<String>,
    pub duplicate_policy: DuplicatePolicy,
}

/// Per-candidate override submitted in the execution request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidateOverride {
    pub id: String,
    #[serde(default)]
    pub file_path: String,
    pub selected: bool,
    pub custom_title: Option<String>,
}

/// Overall operation outcome category per Phase 10, Task 19 specification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    FullSuccess,
    SuccessWithWarnings,
    PartialSuccess,
    Cancelled,
    TotalFailure,
}

/// Progress notification emitted during an import operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub operation_id: Option<String>,
    pub total: usize,
    pub current_index: usize,
    pub current_file: String,
    pub imported_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub cancelled_count: usize,
}

/// Progress notification emitted during an export operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub operation_id: Option<String>,
    pub total: usize,
    pub current_index: usize,
    pub current_title: String,
    pub exported_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub cancelled_count: usize,
}

/// Full execution request payload for committing an import
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    #[serde(default)]
    pub operation_id: Option<String>,
    pub candidates: Vec<ImportCandidateOverride>,
    pub options: ImportOptions,
}

/// Status of an individual imported file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportItemStatus {
    Imported,
    Skipped,
    Rejected,
    Cancelled,
    Failed,
}

/// Result of importing an individual file
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileResult {
    pub candidate_id: String,
    pub file_name: String,
    pub status: ImportItemStatus,
    pub created_note_id: Option<String>,
    pub message: Option<String>,
}

/// Overall execution summary report of an import batch
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub total_processed: usize,
    pub imported_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    #[serde(default)]
    pub cancelled_count: usize,
    #[serde(default)]
    pub outcome: Option<OperationOutcome>,
    pub results: Vec<ImportFileResult>,
    pub duration_ms: u64,
}

impl ImportSummary {
    pub fn calculate_outcome(&self) -> OperationOutcome {
        if self.cancelled_count > 0 {
            OperationOutcome::Cancelled
        } else if self.total_processed > 0 && self.failed_count == self.total_processed {
            OperationOutcome::TotalFailure
        } else if self.failed_count > 0 {
            OperationOutcome::PartialSuccess
        } else if self.skipped_count > 0 || self.results.iter().any(|r| r.message.is_some()) {
            OperationOutcome::SuccessWithWarnings
        } else {
            OperationOutcome::FullSuccess
        }
    }
}

/// Request to export a single note
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSingleNoteRequest {
    pub note_id: String,
    pub format: ExportFormat,
    pub destination_path: String,
    #[serde(default)]
    pub overwrite: bool,
}

/// Scope of notes selected for export (Phase 10, Task 15)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExportScope {
    SingleNote {
        note_id: String,
    },
    SelectedNotes {
        note_ids: Vec<String>,
    },
    Notebook {
        notebook_id: String,
        include_nested: bool,
    },
    AllNotes,
}

/// Lightweight metadata item for notes resolved in an export selection preview
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportNoteMetadataItem {
    pub id: String,
    pub title: String,
    pub format: String,
    pub notebook_id: Option<String>,
    pub notebook_path: Option<String>,
    pub is_favorite: bool,
    pub is_pinned: bool,
    pub tag_count: usize,
    pub content_length_bytes: usize,
    pub modified_at: String,
}

/// Preview and validation report of a proposed export selection
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSelectionPreview {
    pub total_requested: usize,
    pub total_included: usize,
    pub skipped_trashed_count: usize,
    pub skipped_missing_count: usize,
    pub notes: Vec<ExportNoteMetadataItem>,
    pub included_notebook_count: usize,
}

/// Request to export multiple notes or an entire notebook
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBatchRequest {
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub scope: Option<ExportScope>,
    #[serde(default)]
    pub note_ids: Vec<String>,
    pub format: ExportFormat,
    #[serde(default)]
    pub destination_dir: String,
    pub destination_file: Option<String>,
    #[serde(default)]
    pub include_metadata: bool,
    #[serde(default)]
    pub overwrite: bool,
}

/// Status of an individual exported note
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportItemStatus {
    Exported,
    Skipped,
    Cancelled,
    Failed,
}

/// Result of exporting an individual note
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFileResult {
    pub note_id: String,
    pub note_title: String,
    pub file_path: String,
    pub status: ExportItemStatus,
    pub message: Option<String>,
}

/// Overall execution summary report of an export batch
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub total_notes: usize,
    pub exported_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    #[serde(default)]
    pub cancelled_count: usize,
    #[serde(default)]
    pub outcome: Option<OperationOutcome>,
    pub results: Vec<ExportFileResult>,
    pub duration_ms: u64,
}

impl ExportSummary {
    pub fn calculate_outcome(&self) -> OperationOutcome {
        if self.cancelled_count > 0 {
            OperationOutcome::Cancelled
        } else if self.total_notes > 0 && self.failed_count == self.total_notes {
            OperationOutcome::TotalFailure
        } else if self.failed_count > 0 {
            OperationOutcome::PartialSuccess
        } else if self.skipped_count > 0 || self.results.iter().any(|r| r.message.is_some()) {
            OperationOutcome::SuccessWithWarnings
        } else {
            OperationOutcome::FullSuccess
        }
    }
}

/// Version 1.0 Portable Interchange Document Schema
fn default_format_identifier() -> String {
    "personal-notepad-interchange".to_string()
}

fn default_interchange_format() -> String {
    "txt".to_string()
}

/// Representation of an internal note link reference inside an interchange note
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterchangeInternalLink {
    pub label: String,
    pub target_export_id: String,
    pub is_package_internal: bool,
}

/// Version 1.0 Portable Interchange Document Schema
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterchangeDocument {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none", default)]
    pub schema: Option<String>,
    pub app: String,
    #[serde(default = "default_format_identifier")]
    pub format: String,
    pub version: String,
    #[serde(default)]
    pub exported_at: String,
    #[serde(default)]
    pub note_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_link_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_policy: Option<String>,
    pub notes: Vec<InterchangeNote>,
}

/// Individual note representation in the portable interchange document
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterchangeNote {
    pub export_id: String,
    pub title: String,
    pub content: String,
    #[serde(default = "default_interchange_format")]
    pub format: String,
    #[serde(default)]
    pub notebook_path: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub modified_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments_omitted_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub internal_links: Vec<InterchangeInternalLink>,
}

// =========================================================================
// Phase 11 — Backup & Data Safety Domain Models and DTOs (Task 4)
// =========================================================================

pub const CURRENT_BACKUP_FORMAT_VERSION: u32 = 1;
pub const BACKUP_FORMAT_IDENTIFIER: &str = "personal-notepad-backup";
pub const PRIMARY_BACKUP_EXTENSION: &str = "pnbackup";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupType {
    Manual,
    Automatic,
    Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupDatabaseInfo {
    pub entry_path: String,
    pub file_size: u64,
    pub sha256: String,
    pub schema_version: i32,
    pub note_count: usize,
    pub notebook_count: usize,
    pub tag_count: usize,
    pub trash_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupAttachmentsSummary {
    pub total_files: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupAttachmentItem {
    pub id: String,
    pub note_id: String,
    pub file_name: String,
    pub relative_path: String,
    pub file_size: u64,
    pub mime_type: Option<String>,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupScopeDeclaration {
    pub includes_settings: bool,
    pub includes_trash: bool,
    pub includes_attachments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupManifest {
    pub format_identifier: String,
    pub format_version: u32,
    pub app_version: String,
    pub created_at: String,
    pub backup_type: BackupType,
    pub database: BackupDatabaseInfo,
    pub attachments_summary: BackupAttachmentsSummary,
    #[serde(default)]
    pub attachments: Vec<BackupAttachmentItem>,
    pub scope: BackupScopeDeclaration,
    #[serde(default)]
    pub checksums: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub is_encrypted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption_algorithm: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupCreateRequest {
    pub destination_path: Option<String>,
    pub backup_type: Option<BackupType>,
    pub allow_overwrite: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupCreateResult {
    pub success: bool,
    pub file_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub created_at: String,
    pub backup_type: BackupType,
    pub verified: bool,
    pub note_count: usize,
    pub attachment_count: usize,
    pub warning_message: Option<String>,
    #[serde(default)]
    pub is_encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupListItem {
    pub file_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub created_at: String,
    pub backup_type: BackupType,
    pub format_version: Option<u32>,
    pub schema_version: Option<i32>,
    pub note_count: Option<usize>,
    pub notebook_count: Option<usize>,
    pub attachment_count: Option<usize>,
    pub is_verified: bool,
    pub is_valid: bool,
    pub issue_summary: Option<String>,
    #[serde(default)]
    pub is_encrypted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupValidationIssueSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupValidationIssue {
    pub severity: BackupValidationIssueSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupCompatibilityStatus {
    Compatible,
    RequiresMigration,
    IncompatibleVersion,
    Corrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupValidationResult {
    pub is_valid: bool,
    pub compatibility: BackupCompatibilityStatus,
    pub manifest: Option<BackupManifest>,
    pub issues: Vec<BackupValidationIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreviewResult {
    pub file_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub created_at: String,
    pub backup_type: BackupType,
    pub format_version: u32,
    pub schema_version: i32,
    pub note_count: usize,
    pub notebook_count: usize,
    pub tag_count: usize,
    pub trash_count: usize,
    pub attachment_count: usize,
    pub attachment_total_bytes: u64,
    pub includes_settings: bool,
    pub compatibility: BackupCompatibilityStatus,
    pub warnings: Vec<String>,
    pub can_restore: bool,
    #[serde(default)]
    pub is_encrypted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption_algorithm: Option<String>,
}

/// Authoritative metadata for a pre-restore recovery checkpoint (Phase 11, Task 16).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCheckpoint {
    pub checkpoint_id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub created_at: String,
    pub note_count: usize,
    pub trash_count: usize,
    pub notebook_count: usize,
    pub tag_count: usize,
    pub attachment_count: usize,
    pub attachment_total_bytes: u64,
    pub schema_version: i32,
    pub is_verified: bool,
    #[serde(default)]
    pub is_encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreRequest {
    pub file_path: String,
    #[serde(default)]
    pub operation_id: Option<String>,
}

/// Detailed post-restore validation report confirming data and schema integrity across all subsystems (Phase 11, Task 20).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PostRestoreValidationReport {
    pub is_valid: bool,
    pub integrity_check_passed: bool,
    pub foreign_key_check_passed: bool,
    pub schema_version: i32,
    pub note_count: usize,
    pub notebook_count: usize,
    pub tag_count: usize,
    pub trash_count: usize,
    pub attachment_count: usize,
    pub attachments_verified_on_disk: bool,
    pub fts_rebuilt_and_queryable: bool,
    pub internal_links_verified: bool,
    pub settings_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreResult {
    pub success: bool,
    pub restored_at: String,
    pub note_count: usize,
    pub notebook_count: usize,
    pub tag_count: usize,
    pub trash_count: usize,
    pub attachment_count: usize,
    pub checkpoint_path: String,
    pub warning_message: Option<String>,
    #[serde(default)]
    pub validation_report: Option<PostRestoreValidationReport>,
}

/// Authoritative summary of an isolated staged archive extraction (Phase 11, Task 17).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StagedRestoreSummary {
    pub staging_id: String,
    pub staging_dir: String,
    pub database_path: String,
    pub manifest_present: bool,
    pub total_extracted_files: usize,
    pub total_extracted_bytes: u64,
    pub note_count: usize,
    pub notebook_count: usize,
    pub tag_count: usize,
    pub trash_count: usize,
    pub attachment_count: usize,
    pub initial_schema_version: i32,
    pub is_valid: bool,
}

/// Result of running database schema migrations on a staged database before activation (Phase 11, Task 18).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StagedMigrationResult {
    pub staging_id: String,
    pub staging_dir: String,
    pub previous_schema_version: i32,
    pub migrated_schema_version: i32,
    pub migrations_applied: usize,
    pub note_count: usize,
    pub trash_count: usize,
    pub notebook_count: usize,
    pub tag_count: usize,
    pub attachment_count: usize,
    pub is_verified: bool,
}

/// Lifecycle phases of a controlled restore activation operation (Phase 11, Task 19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RestoreActivationPhase {
    Initiated,
    CheckpointCreated,
    FilesSwapped,
    PostVerificationStarted,
    RollbackInProgress,
    RolledBack,
    Completed,
}

/// Durable marker payload tracking multi-component restore activation state across crashes (Phase 11, Task 19).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreActivationMarker {
    pub operation_id: String,
    pub started_at: String,
    pub candidate_archive_path: String,
    pub staging_dir: String,
    pub checkpoint_path: String,
    pub live_db_backup_path: String,
    pub live_attachments_backup_path: String,
    pub phase: RestoreActivationPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupProgressStage {
    Starting,
    SnapshottingDatabase,
    CollectingAttachments,
    GeneratingManifest,
    WritingArchive,
    VerifyingArchive,
    ApplyingRetention,
    ExtractingStaging,
    PreparingStagingData,
    CheckingDatabaseIntegrity,
    CheckingAttachments,
    ValidatingBackup,
    ValidatingStagedData,
    CreatingCheckpoint,
    PreparingActivation,
    ActivatingRestoredData,
    VerifyingRestoredState,
    RebuildingSearchIndex,
    Completed,
    Failed,
    FailedWithRecoveryAvailable,
    FailedAndRecoveryRequired,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupProgressEvent {
    pub stage: BackupProgressStage,
    pub message: String,
    pub current_item: Option<usize>,
    pub total_items: Option<usize>,
    pub percentage: Option<f32>,
    #[serde(default)]
    pub cancellable: bool,
    #[serde(default)]
    pub operation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettings {
    pub auto_backup_enabled: bool,
    pub interval_minutes: u32,
    pub backup_destination_dir: Option<String>,
    pub retention_count: u32,
    pub backup_on_startup: bool,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            auto_backup_enabled: true,
            interval_minutes: 1440,
            backup_destination_dir: None,
            retention_count: 5,
            backup_on_startup: true,
        }
    }
}

/// Runtime and persistent scheduler execution state (Phase 11, Task 11 & Task 22)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupSchedulerState {
    #[serde(default)]
    pub last_attempt_at: Option<String>,
    #[serde(default)]
    pub last_successful_at: Option<String>,
    #[serde(default)]
    pub last_error_message: Option<String>,
    #[serde(default)]
    pub last_failure_category: Option<String>,
    #[serde(default)]
    pub last_retention_warning: Option<String>,
    #[serde(default)]
    pub failure_count: u32,
}

/// Overall health level classification of the backup subsystem (Phase 11, Task 22)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupHealthLevel {
    Healthy,
    Warning,
    Critical,
    Disabled,
}

/// Granular, discoverable backup health warning or notification item (Phase 11, Task 22)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupHealthWarning {
    pub code: String,
    pub level: String,
    pub message: String,
    pub detail: Option<String>,
}

/// Authoritative snapshot of backup health and operational readiness (Phase 11, Task 22)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupHealthStatus {
    pub health_level: BackupHealthLevel,
    pub auto_backup_enabled: bool,
    pub destination_available: bool,
    pub destination_path: String,
    pub is_same_disk: bool,
    pub last_attempt_at: Option<String>,
    pub last_successful_at: Option<String>,
    pub last_failure_category: Option<String>,
    pub last_error_message: Option<String>,
    pub last_retention_warning: Option<String>,
    pub failure_count: u32,
    pub managed_backup_count: usize,
    pub has_verified_backup: bool,
    pub latest_backup_name: Option<String>,
    pub latest_backup_verified: bool,
    pub warnings: Vec<BackupHealthWarning>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attachment_model_validation_and_serialization() {
        let att = Attachment {
            id: "att-123".to_string(),
            note_id: "note-456".to_string(),
            file_name: "Project Document.pdf".to_string(),
            relative_path: "note-456/uuid.pdf".to_string(),
            mime_type: Some("application/pdf".to_string()),
            file_size: 1048576,
            created_at: "2026-10-08T12:00:00Z".to_string(),
            modified_at: "2026-10-08T12:00:00Z".to_string(),
        };

        assert!(att.is_valid());

        // Test JSON serialization & deserialization round-trip
        let json = serde_json::to_string(&att).expect("Serialize attachment");
        assert!(json.contains("\"file_name\":\"Project Document.pdf\""));
        assert!(json.contains("\"mime_type\":\"application/pdf\""));
        assert!(json.contains("\"file_size\":1048576"));

        let deserialized: Attachment = serde_json::from_str(&json).expect("Deserialize attachment");
        assert_eq!(att, deserialized);

        // Test nullable MIME type serialization
        let mut att_no_mime = att.clone();
        att_no_mime.mime_type = None;
        let json_no_mime = serde_json::to_string(&att_no_mime).expect("Serialize null mime");
        assert!(json_no_mime.contains("\"mime_type\":null"));
        let deserialized_no_mime: Attachment =
            serde_json::from_str(&json_no_mime).expect("Deserialize null mime");
        assert_eq!(deserialized_no_mime.mime_type, None);

        // Test negative size validation rejection
        let mut invalid_att = att.clone();
        invalid_att.file_size = -1;
        assert!(!invalid_att.is_valid());

        // Test empty id validation rejection
        let mut empty_id_att = att.clone();
        empty_id_att.id = "   ".to_string();
        assert!(!empty_id_att.is_valid());

        // Test AttachmentMetadataDto conversion
        let dto: AttachmentMetadataDto = att.into();
        assert_eq!(dto.id, "att-123");
        assert_eq!(dto.file_name, "Project Document.pdf");
        assert_eq!(dto.file_size, 1048576);
    }

    #[test]
    fn test_note_lifecycle_and_trash_domain_models() {
        let active_note = Note {
            id: "n-1".to_string(),
            title: "Active Note".to_string(),
            content: "Hello world".to_string(),
            format: "txt".to_string(),
            notebook_id: None,
            created_at: "2026-10-08T12:00:00Z".to_string(),
            modified_at: "2026-10-08T12:00:00Z".to_string(),
            is_favorite: false,
            is_pinned: false,
            is_deleted: false,
            deleted_at: None,
        };

        assert_eq!(active_note.lifecycle(), NoteLifecycle::Active);
        assert!(active_note.lifecycle().is_active());
        assert!(!active_note.lifecycle().is_trashed());

        let trashed_note = Note {
            id: "n-2".to_string(),
            title: "Deleted Note".to_string(),
            content: "Old content".to_string(),
            format: "md".to_string(),
            notebook_id: Some("nb-1".to_string()),
            created_at: "2026-10-08T12:00:00Z".to_string(),
            modified_at: "2026-10-08T12:30:00Z".to_string(),
            is_favorite: true,
            is_pinned: false,
            is_deleted: true,
            deleted_at: Some("2026-10-08T12:30:00Z".to_string()),
        };

        assert_eq!(trashed_note.lifecycle(), NoteLifecycle::Trashed);
        assert!(trashed_note.lifecycle().is_trashed());
        assert!(!trashed_note.lifecycle().is_active());

        // Test serde serialization of NoteLifecycle
        let json_active = serde_json::to_string(&NoteLifecycle::Active).unwrap();
        assert_eq!(json_active, "\"active\"");
        let json_trashed = serde_json::to_string(&NoteLifecycle::Trashed).unwrap();
        assert_eq!(json_trashed, "\"trashed\"");

        let de_active: NoteLifecycle = serde_json::from_str("\"active\"").unwrap();
        assert_eq!(de_active, NoteLifecycle::Active);
        let de_trashed: NoteLifecycle = serde_json::from_str("\"trashed\"").unwrap();
        assert_eq!(de_trashed, NoteLifecycle::Trashed);

        // Test TrashedNoteItem serialization
        let item = TrashedNoteItem {
            id: "n-2".to_string(),
            title: "Deleted Note".to_string(),
            preview: "Old content".to_string(),
            notebook_id: Some("nb-1".to_string()),
            notebook_name: Some("Work".to_string()),
            created_at: "2026-10-08T12:00:00Z".to_string(),
            modified_at: "2026-10-08T12:30:00Z".to_string(),
            deleted_at: Some("2026-10-08T12:30:00Z".to_string()),
            is_favorite: true,
            attachment_count: 2,
        };
        let item_json = serde_json::to_string(&item).unwrap();
        assert!(item_json.contains("\"attachmentCount\":2"));
        assert!(item_json.contains("\"deletedAt\":\"2026-10-08T12:30:00Z\""));

        // Test PermanentDeleteResult serialization
        let perm_res = PermanentDeleteResult {
            note_id: "n-2".to_string(),
            success: true,
            attachment_cleanup_failed: false,
            error_message: None,
        };
        let perm_json = serde_json::to_string(&perm_res).unwrap();
        assert!(perm_json.contains("\"attachmentCleanupFailed\":false"));

        // Test EmptyTrashResult serialization
        let empty_res = EmptyTrashResult {
            deleted_count: 5,
            failed_count: 1,
            attachment_cleanup_failures: vec!["file-err-1".to_string()],
            errors: vec!["IO error".to_string()],
        };
        let empty_json = serde_json::to_string(&empty_res).unwrap();
        assert!(empty_json.contains("\"deletedCount\":5"));
        assert!(empty_json.contains("\"failedCount\":1"));
    }

    #[test]
    fn test_internal_link_models_and_serde() {
        // Test LinkTargetLifecycle
        assert!(LinkTargetLifecycle::Active.is_active());
        assert!(!LinkTargetLifecycle::Active.is_trashed());
        assert!(!LinkTargetLifecycle::Active.is_missing());

        assert!(LinkTargetLifecycle::Trashed.is_trashed());
        assert!(!LinkTargetLifecycle::Trashed.is_active());

        assert!(LinkTargetLifecycle::Missing.is_missing());
        assert!(!LinkTargetLifecycle::Missing.is_active());

        assert_eq!(
            serde_json::to_string(&LinkTargetLifecycle::Active).unwrap(),
            "\"active\""
        );
        assert_eq!(
            serde_json::to_string(&LinkTargetLifecycle::Trashed).unwrap(),
            "\"trashed\""
        );
        assert_eq!(
            serde_json::to_string(&LinkTargetLifecycle::Missing).unwrap(),
            "\"missing\""
        );

        // Test ParsedInternalLink camelCase serialization
        let parsed = ParsedInternalLink {
            target_id: "target-uuid-1".to_string(),
            label: "Meeting Plan".to_string(),
            raw: "[Meeting Plan](notepad://note/target-uuid-1)".to_string(),
            start_index: 0,
            end_index: 44,
        };
        let parsed_json = serde_json::to_string(&parsed).unwrap();
        assert!(parsed_json.contains("\"targetId\":\"target-uuid-1\""));
        assert!(parsed_json.contains("\"startIndex\":0"));
        assert!(parsed_json.contains("\"endIndex\":44"));

        // Test ResolvedLinkTarget camelCase serialization
        let resolved = ResolvedLinkTarget {
            target_id: "target-uuid-1".to_string(),
            title: Some("Meeting Plan".to_string()),
            lifecycle: LinkTargetLifecycle::Active,
            notebook_id: Some("nb-1".to_string()),
            notebook_name: Some("Work".to_string()),
        };
        let resolved_json = serde_json::to_string(&resolved).unwrap();
        assert!(resolved_json.contains("\"targetId\":\"target-uuid-1\""));
        assert!(resolved_json.contains("\"notebookName\":\"Work\""));
        assert!(resolved_json.contains("\"lifecycle\":\"active\""));

        // Test LinkableNoteItem camelCase serialization
        let item = LinkableNoteItem {
            id: "target-uuid-1".to_string(),
            title: "Meeting Plan".to_string(),
            preview: "Details about...".to_string(),
            notebook_id: Some("nb-1".to_string()),
            notebook_name: Some("Work".to_string()),
            modified_at: "2026-10-09T10:00:00Z".to_string(),
        };
        let item_json = serde_json::to_string(&item).unwrap();
        assert!(item_json.contains("\"modifiedAt\":\"2026-10-09T10:00:00Z\""));

        // Test BacklinkItem camelCase serialization
        let backlink = BacklinkItem {
            source_note_id: "src-uuid-1".to_string(),
            source_note_title: "Project Overview".to_string(),
            source_notebook_id: None,
            source_notebook_name: None,
            label_snippet: Some("See [Meeting Plan]".to_string()),
            modified_at: "2026-10-09T10:05:00Z".to_string(),
        };
        let backlink_json = serde_json::to_string(&backlink).unwrap();
        assert!(backlink_json.contains("\"sourceNoteId\":\"src-uuid-1\""));
        assert!(backlink_json.contains("\"sourceNoteTitle\":\"Project Overview\""));
        assert!(backlink_json.contains("\"labelSnippet\":\"See [Meeting Plan]\""));
    }

    #[test]
    fn test_import_export_models_serde() {
        // Test ImportFormat, ExportFormat, DuplicatePolicy, ValidationStatus
        assert_eq!(
            serde_json::to_string(&ImportFormat::Txt).unwrap(),
            "\"txt\""
        );
        assert_eq!(serde_json::to_string(&ImportFormat::Md).unwrap(), "\"md\"");
        assert_eq!(
            serde_json::to_string(&ImportFormat::Json).unwrap(),
            "\"json\""
        );

        assert_eq!(
            serde_json::to_string(&ExportFormat::Txt).unwrap(),
            "\"txt\""
        );
        assert_eq!(serde_json::to_string(&ExportFormat::Md).unwrap(), "\"md\"");
        assert_eq!(
            serde_json::to_string(&ExportFormat::Json).unwrap(),
            "\"json\""
        );

        assert_eq!(
            serde_json::to_string(&DuplicatePolicy::ImportAsNew).unwrap(),
            "\"import_as_new\""
        );
        assert_eq!(
            serde_json::to_string(&DuplicatePolicy::SkipExact).unwrap(),
            "\"skip_exact\""
        );
        assert_eq!(
            serde_json::to_string(&DuplicatePolicy::Ask).unwrap(),
            "\"ask\""
        );

        assert_eq!(
            serde_json::to_string(&ValidationStatus::Valid).unwrap(),
            "\"valid\""
        );
        assert_eq!(
            serde_json::to_string(&ValidationStatus::Warning).unwrap(),
            "\"warning\""
        );
        assert_eq!(
            serde_json::to_string(&ValidationStatus::Invalid).unwrap(),
            "\"invalid\""
        );

        // Test ImportCandidate camelCase serialization
        let candidate = ImportCandidate {
            id: "c-1".to_string(),
            file_path: "/tmp/notes/test.md".to_string(),
            file_name: "test.md".to_string(),
            file_size: 1024,
            detected_format: ImportFormat::Md,
            proposed_title: "test".to_string(),
            validation_status: ValidationStatus::Valid,
            warnings: vec![],
            error_message: None,
            is_duplicate: false,
            duplicate_note_id: None,
            duplicate_note_title: None,
            selected: true,
        };
        let c_json = serde_json::to_string(&candidate).unwrap();
        assert!(c_json.contains("\"filePath\":\"/tmp/notes/test.md\""));
        assert!(c_json.contains("\"fileName\":\"test.md\""));
        assert!(c_json.contains("\"detectedFormat\":\"md\""));
        assert!(c_json.contains("\"proposedTitle\":\"test\""));
        assert!(c_json.contains("\"validationStatus\":\"valid\""));

        // Test ImportPreview serialization
        let preview = ImportPreview {
            candidates: vec![candidate],
            total_files: 1,
            valid_files: 1,
            invalid_files: 0,
            duplicate_files: 0,
            total_size_bytes: 1024,
        };
        let p_json = serde_json::to_string(&preview).unwrap();
        assert!(p_json.contains("\"totalFiles\":1"));
        assert!(p_json.contains("\"totalSizeBytes\":1024"));

        // Test ImportSummary & ImportFileResult
        let summary = ImportSummary {
            total_processed: 1,
            imported_count: 1,
            skipped_count: 0,
            failed_count: 0,
            cancelled_count: 0,
            outcome: Some(OperationOutcome::FullSuccess),
            results: vec![ImportFileResult {
                candidate_id: "c-1".to_string(),
                file_name: "test.md".to_string(),
                status: ImportItemStatus::Imported,
                created_note_id: Some("note-new-uuid".to_string()),
                message: None,
            }],
            duration_ms: 45,
        };
        let s_json = serde_json::to_string(&summary).unwrap();
        assert!(s_json.contains("\"importedCount\":1"));
        assert!(s_json.contains("\"status\":\"imported\""));
        assert!(s_json.contains("\"createdNoteId\":\"note-new-uuid\""));

        // Test ExportSummary & ExportFileResult
        let exp_summary = ExportSummary {
            total_notes: 1,
            exported_count: 1,
            skipped_count: 0,
            failed_count: 0,
            cancelled_count: 0,
            outcome: Some(OperationOutcome::FullSuccess),
            results: vec![ExportFileResult {
                note_id: "note-1".to_string(),
                note_title: "Project".to_string(),
                file_path: "/out/Project.md".to_string(),
                status: ExportItemStatus::Exported,
                message: None,
            }],
            duration_ms: 12,
        };
        let es_json = serde_json::to_string(&exp_summary).unwrap();
        assert!(es_json.contains("\"exportedCount\":1"));
        assert!(es_json.contains("\"filePath\":\"/out/Project.md\""));

        // Test InterchangeDocument schema
        let doc = InterchangeDocument {
            schema: Some("https://personal-notepad.local/schemas/interchange-v1.json".to_string()),
            app: "personal-notepad".to_string(),
            format: "personal-notepad-interchange".to_string(),
            version: "1.0".to_string(),
            exported_at: "2026-10-10T10:00:00Z".to_string(),
            note_count: 1,
            internal_link_policy: Some(
                "Package-scoped internal links map export_id to new local IDs.".to_string(),
            ),
            attachment_policy: Some(
                "Attachments omitted from portable JSON interchange.".to_string(),
            ),
            notes: vec![InterchangeNote {
                export_id: "exp-1".to_string(),
                title: "Doc Note".to_string(),
                content: "Content with [Target Note](notepad://note/exp-2)".to_string(),
                format: "md".to_string(),
                notebook_path: Some("Work/Projects".to_string()),
                tags: vec!["tag1".to_string()],
                is_favorite: true,
                is_pinned: false,
                created_at: Some("2026-10-01T10:00:00Z".to_string()),
                modified_at: Some("2026-10-05T10:00:00Z".to_string()),
                attachments_omitted_count: Some(2),
                internal_links: vec![InterchangeInternalLink {
                    label: "Target Note".to_string(),
                    target_export_id: "exp-2".to_string(),
                    is_package_internal: false,
                }],
            }],
        };
        let doc_json = serde_json::to_string(&doc).unwrap();
        assert!(doc_json.contains("\"$schema\":"));
        assert!(doc_json.contains("\"app\":\"personal-notepad\""));
        assert!(doc_json.contains("\"format\":\"personal-notepad-interchange\""));
        assert!(doc_json.contains("\"version\":\"1.0\""));
        assert!(doc_json.contains("\"internal_link_policy\":"));
        assert!(doc_json.contains("\"attachment_policy\":"));
        assert!(doc_json.contains("\"attachments_omitted_count\":2"));
        assert!(doc_json.contains("\"is_package_internal\":false"));
        assert!(doc_json.contains("\"notebook_path\":\"Work/Projects\""));

        // Round-trip parse test
        let parsed_doc: InterchangeDocument = serde_json::from_str(&doc_json).unwrap();
        assert_eq!(parsed_doc.notes.len(), 1);
        assert_eq!(parsed_doc.notes[0].attachments_omitted_count, Some(2));
        assert_eq!(parsed_doc.notes[0].internal_links.len(), 1);
        assert!(!parsed_doc.notes[0].internal_links[0].is_package_internal);
    }

    #[test]
    fn test_backup_domain_models_and_dtos() {
        // 1. Test BackupType serialization
        assert_eq!(
            serde_json::to_string(&BackupType::Manual).unwrap(),
            "\"manual\""
        );
        assert_eq!(
            serde_json::to_string(&BackupType::Automatic).unwrap(),
            "\"automatic\""
        );
        assert_eq!(
            serde_json::to_string(&BackupType::Checkpoint).unwrap(),
            "\"checkpoint\""
        );

        // 2. Test BackupManifest round-trip
        let mut checksums = std::collections::HashMap::new();
        checksums.insert("database.sqlite".to_string(), "abc123sha256".to_string());
        checksums.insert(
            "attachments/note-1/doc.pdf".to_string(),
            "def456sha256".to_string(),
        );

        let manifest = BackupManifest {
            format_identifier: BACKUP_FORMAT_IDENTIFIER.to_string(),
            format_version: CURRENT_BACKUP_FORMAT_VERSION,
            app_version: "0.1.0".to_string(),
            created_at: "2026-10-13T12:00:00Z".to_string(),
            backup_type: BackupType::Manual,
            database: BackupDatabaseInfo {
                entry_path: "database.sqlite".to_string(),
                file_size: 1048576,
                sha256: "abc123sha256".to_string(),
                schema_version: 4,
                note_count: 42,
                notebook_count: 5,
                tag_count: 10,
                trash_count: 3,
            },
            attachments_summary: BackupAttachmentsSummary {
                total_files: 1,
                total_bytes: 524288,
            },
            attachments: vec![BackupAttachmentItem {
                id: "att-1".to_string(),
                note_id: "note-1".to_string(),
                file_name: "doc.pdf".to_string(),
                relative_path: "attachments/note-1/doc.pdf".to_string(),
                file_size: 524288,
                mime_type: Some("application/pdf".to_string()),
                sha256: "def456sha256".to_string(),
            }],
            scope: BackupScopeDeclaration {
                includes_settings: true,
                includes_trash: true,
                includes_attachments: true,
            },
            checksums,
            is_encrypted: false,
            encryption_algorithm: None,
        };

        let manifest_json = serde_json::to_string(&manifest).expect("Serialize manifest");
        assert!(manifest_json.contains("\"format_identifier\":\"personal-notepad-backup\""));
        assert!(manifest_json.contains("\"format_version\":1"));
        assert!(manifest_json.contains("\"schema_version\":4"));
        assert!(manifest_json.contains("\"note_count\":42"));
        assert!(manifest_json.contains("\"trash_count\":3"));

        let deserialized_manifest: BackupManifest =
            serde_json::from_str(&manifest_json).expect("Deserialize manifest");
        assert_eq!(manifest, deserialized_manifest);

        // 3. Test BackupCreateResult camelCase serialization
        let create_res = BackupCreateResult {
            success: true,
            file_path: "/home/user/backups/backup.pnbackup".to_string(),
            file_name: "backup.pnbackup".to_string(),
            file_size: 1572864,
            created_at: "2026-10-13T12:00:00Z".to_string(),
            backup_type: BackupType::Manual,
            verified: true,
            note_count: 42,
            attachment_count: 1,
            warning_message: None,
            is_encrypted: false,
        };
        let create_json = serde_json::to_string(&create_res).unwrap();
        assert!(create_json.contains("\"filePath\":"));
        assert!(create_json.contains("\"fileName\":"));
        assert!(create_json.contains("\"fileSize\":"));
        assert!(create_json.contains("\"noteCount\":42"));
        assert!(create_json.contains("\"backupType\":\"manual\""));

        // 4. Test BackupListItem camelCase serialization
        let list_item = BackupListItem {
            file_path: "/home/user/backups/auto.pnbackup".to_string(),
            file_name: "auto.pnbackup".to_string(),
            file_size: 1048576,
            created_at: "2026-10-13T11:00:00Z".to_string(),
            backup_type: BackupType::Automatic,
            format_version: Some(1),
            schema_version: Some(4),
            note_count: Some(15),
            notebook_count: Some(2),
            attachment_count: Some(0),
            is_verified: true,
            is_valid: true,
            issue_summary: None,
            is_encrypted: false,
        };
        let list_json = serde_json::to_string(&list_item).unwrap();
        assert!(list_json.contains("\"isVerified\":true"));
        assert!(list_json.contains("\"isValid\":true"));
        assert!(list_json.contains("\"schemaVersion\":4"));

        // 5. Test BackupValidationResult and Compatibility status
        let val_res = BackupValidationResult {
            is_valid: true,
            compatibility: BackupCompatibilityStatus::Compatible,
            manifest: Some(manifest.clone()),
            issues: vec![BackupValidationIssue {
                severity: BackupValidationIssueSeverity::Warning,
                code: "ATTACHMENT_OPTIONAL_NOTICE".to_string(),
                message: "No issues found".to_string(),
            }],
        };
        let val_json = serde_json::to_string(&val_res).unwrap();
        assert!(val_json.contains("\"isValid\":true"));
        assert!(val_json.contains("\"compatibility\":\"compatible\""));
        assert!(val_json.contains("\"severity\":\"warning\""));

        // 6. Test BackupSettings default values and serialization
        let default_settings = BackupSettings::default();
        assert!(default_settings.auto_backup_enabled);
        assert_eq!(default_settings.interval_minutes, 1440);
        assert_eq!(default_settings.retention_count, 5);
        assert!(default_settings.backup_on_startup);
        let settings_json = serde_json::to_string(&default_settings).unwrap();
        assert!(settings_json.contains("\"autoBackupEnabled\":true"));
        assert!(settings_json.contains("\"retentionCount\":5"));

        // 7. Test BackupProgressEvent serialization
        let prog = BackupProgressEvent {
            stage: BackupProgressStage::WritingArchive,
            message: "Writing backup entries...".to_string(),
            current_item: Some(3),
            total_items: Some(10),
            percentage: Some(30.0),
            cancellable: true,
            operation_id: Some("op-test-123".to_string()),
        };
        let prog_json = serde_json::to_string(&prog).unwrap();
        assert!(prog_json.contains("\"stage\":\"writingArchive\""));
        assert!(prog_json.contains("\"percentage\":30.0"));
        assert!(prog_json.contains("\"cancellable\":true"));
        assert!(prog_json.contains("\"operationId\":\"op-test-123\""));

        let stage_cancelled = BackupProgressStage::Cancelled;
        let stage_cancelled_json = serde_json::to_string(&stage_cancelled).unwrap();
        assert_eq!(stage_cancelled_json, "\"cancelled\"");

        let stage_prep_act = BackupProgressStage::PreparingActivation;
        let stage_prep_act_json = serde_json::to_string(&stage_prep_act).unwrap();
        assert_eq!(stage_prep_act_json, "\"preparingActivation\"");

        // 8. Test RecoveryCheckpoint serialization & round-trip (Phase 11, Task 16)
        let checkpoint = RecoveryCheckpoint {
            checkpoint_id: "chk-uuid-1".to_string(),
            file_path:
                "/data/backups/checkpoints/personal-notepad-checkpoint-20261013-120000.pnbackup"
                    .to_string(),
            file_name: "personal-notepad-checkpoint-20261013-120000.pnbackup".to_string(),
            file_size: 409600,
            created_at: "2026-10-13T12:00:00Z".to_string(),
            note_count: 10,
            trash_count: 2,
            notebook_count: 3,
            tag_count: 5,
            attachment_count: 4,
            attachment_total_bytes: 1048576,
            schema_version: 4,
            is_verified: true,
            is_encrypted: false,
        };
        let chk_json = serde_json::to_string(&checkpoint).unwrap();
        assert!(chk_json.contains("\"checkpointId\":\"chk-uuid-1\""));
        assert!(chk_json.contains("\"attachmentTotalBytes\":1048576"));
        assert!(chk_json.contains("\"isVerified\":true"));
        let de_chk: RecoveryCheckpoint = serde_json::from_str(&chk_json).unwrap();
        assert_eq!(checkpoint, de_chk);

        // 9. Test StagedRestoreSummary serialization (Phase 11, Task 17)
        let staged_summary = StagedRestoreSummary {
            staging_id: "stg-123".to_string(),
            staging_dir: "/data/backups/.restore_staging_stg-123".to_string(),
            database_path: "/data/backups/.restore_staging_stg-123/database.sqlite".to_string(),
            manifest_present: true,
            total_extracted_files: 5,
            total_extracted_bytes: 2048000,
            note_count: 12,
            notebook_count: 2,
            tag_count: 4,
            trash_count: 1,
            attachment_count: 2,
            initial_schema_version: 3,
            is_valid: true,
        };
        let summary_json = serde_json::to_string(&staged_summary).unwrap();
        assert!(summary_json.contains("\"stagingId\":\"stg-123\""));
        assert!(summary_json.contains("\"totalExtractedBytes\":2048000"));
        assert!(summary_json.contains("\"initialSchemaVersion\":3"));
        let de_summary: StagedRestoreSummary = serde_json::from_str(&summary_json).unwrap();
        assert_eq!(staged_summary, de_summary);

        // 10. Test StagedMigrationResult serialization (Phase 11, Task 18)
        let migration_res = StagedMigrationResult {
            staging_id: "stg-123".to_string(),
            staging_dir: "/data/backups/.restore_staging_stg-123".to_string(),
            previous_schema_version: 3,
            migrated_schema_version: 4,
            migrations_applied: 1,
            note_count: 12,
            trash_count: 1,
            notebook_count: 2,
            tag_count: 4,
            attachment_count: 2,
            is_verified: true,
        };
        let mig_json = serde_json::to_string(&migration_res).unwrap();
        assert!(mig_json.contains("\"previousSchemaVersion\":3"));
        assert!(mig_json.contains("\"migratedSchemaVersion\":4"));
        assert!(mig_json.contains("\"migrationsApplied\":1"));
        let de_mig: StagedMigrationResult = serde_json::from_str(&mig_json).unwrap();
        assert_eq!(migration_res, de_mig);

        // 11. Test RestoreActivationMarker & RestoreActivationPhase serialization (Phase 11, Task 19)
        let marker = RestoreActivationMarker {
            operation_id: "op-uuid-1".to_string(),
            started_at: "2026-10-14T10:00:00Z".to_string(),
            candidate_archive_path: "/data/backups/backup.pnbackup".to_string(),
            staging_dir: "/data/backups/.restore_staging_op-uuid-1".to_string(),
            checkpoint_path: "/data/backups/checkpoints/chk-1.pnbackup".to_string(),
            live_db_backup_path: "/data/.db_pre_restore_op-uuid-1.sqlite".to_string(),
            live_attachments_backup_path: "/data/.att_pre_restore_op-uuid-1".to_string(),
            phase: RestoreActivationPhase::FilesSwapped,
        };
        let marker_json = serde_json::to_string(&marker).unwrap();
        assert!(marker_json.contains("\"operationId\":\"op-uuid-1\""));
        assert!(marker_json.contains("\"phase\":\"filesSwapped\""));
        let de_marker: RestoreActivationMarker = serde_json::from_str(&marker_json).unwrap();
        assert_eq!(marker, de_marker);

        // 12. Test PostRestoreValidationReport serialization (Phase 11, Task 20)
        let report = PostRestoreValidationReport {
            is_valid: true,
            integrity_check_passed: true,
            foreign_key_check_passed: true,
            schema_version: 4,
            note_count: 25,
            notebook_count: 4,
            tag_count: 6,
            trash_count: 2,
            attachment_count: 3,
            attachments_verified_on_disk: true,
            fts_rebuilt_and_queryable: true,
            internal_links_verified: true,
            settings_verified: true,
        };
        let report_json = serde_json::to_string(&report).unwrap();
        assert!(report_json.contains("\"isValid\":true"));
        assert!(report_json.contains("\"integrityCheckPassed\":true"));
        assert!(report_json.contains("\"ftsRebuiltAndQueryable\":true"));
        assert!(report_json.contains("\"attachmentsVerifiedOnDisk\":true"));
        let de_report: PostRestoreValidationReport = serde_json::from_str(&report_json).unwrap();
        assert_eq!(report, de_report);
    }
}
