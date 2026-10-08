use serde::{Deserialize, Serialize};

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
}


