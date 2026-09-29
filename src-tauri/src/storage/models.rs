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

/// Persistent Tag model
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub created_at: String,
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
