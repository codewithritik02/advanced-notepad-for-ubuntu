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
        let deserialized_no_mime: Attachment = serde_json::from_str(&json_no_mime).expect("Deserialize null mime");
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

        assert_eq!(serde_json::to_string(&LinkTargetLifecycle::Active).unwrap(), "\"active\"");
        assert_eq!(serde_json::to_string(&LinkTargetLifecycle::Trashed).unwrap(), "\"trashed\"");
        assert_eq!(serde_json::to_string(&LinkTargetLifecycle::Missing).unwrap(), "\"missing\"");

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
}



