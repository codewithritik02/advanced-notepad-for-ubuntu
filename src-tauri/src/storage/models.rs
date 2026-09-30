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

/// File attachment metadata model
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub note_id: String,
    pub file_name: String,
    pub relative_path: String,
    pub mime_type: String,
    pub file_size: i64,
    pub created_at: String,
    pub modified_at: String,
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

