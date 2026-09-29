/**
 * Strongly-typed domain models and DTOs corresponding to SQLite schema entities.
 */

export interface Note {
  id: string;
  title: string;
  content: string;
  format: "txt" | "md" | string;
  notebook_id: string | null;
  created_at: string;
  modified_at: string;
  is_favorite: boolean;
  is_pinned: boolean;
  is_deleted: boolean;
  deleted_at: string | null;
}

export interface CreateNoteInput {
  title?: string;
  content?: string;
  format?: "txt" | "md";
  notebook_id?: string | null;
  is_favorite?: boolean;
  is_pinned?: boolean;
}

export interface UpdateNoteInput {
  title?: string;
  content?: string;
  format?: "txt" | "md";
  notebook_id?: string | null;
  is_favorite?: boolean;
  is_pinned?: boolean;
  is_deleted?: boolean;
}

export interface Notebook {
  id: string;
  name: string;
  parent_id: string | null;
  created_at: string;
  modified_at: string;
}

export interface CreateNotebookInput {
  name: string;
  parent_id?: string | null;
}

export interface Tag {
  id: string;
  name: string;
  created_at: string;
}

export interface Attachment {
  id: string;
  note_id: string;
  file_name: string;
  relative_path: string;
  mime_type: string;
  file_size: number;
  created_at: string;
  modified_at: string;
}

export interface Setting {
  key: string;
  value: string;
}

export interface StorageInfo {
  data_dir: string;
  database_file: string;
  is_initialized: boolean;
}
