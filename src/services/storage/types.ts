/**
 * Strongly-typed domain models and DTOs corresponding to SQLite schema entities.
 */

export type NoteFormat = "txt" | "md";

export interface Note {
  id: string;
  title: string;
  content: string;
  format: NoteFormat | string;
  notebook_id: string | null;
  created_at: string;
  modified_at: string;
  is_favorite: boolean;
  is_pinned: boolean;
  is_deleted: boolean;
  deleted_at: string | null;
}

export interface NoteMetadata {
  note_id: string;
  created_at: string;
  modified_at: string;
  format: string;
  notebook_id: string | null;
  notebook_name?: string | null;
  notebook_path?: string | null;
  is_favorite: boolean;
  is_pinned: boolean;
  tags: Tag[];
  word_count: number;
  character_count: number;
  byte_size: number;
}

export interface ListNotesOptions {
  includeDeleted?: boolean;
  notebookId?: string | null;
  unfiledOnly?: boolean;
  favoritesOnly?: boolean;
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

export interface UpdateNotebookInput {
  name: string;
}

export interface Tag {
  id: string;
  name: string;
  created_at: string;
}

export interface CreateTagInput {
  name: string;
}

export interface UpdateTagInput {
  name: string;
}

export interface NoteTagAssignment {
  note_id: string;
  tag_id: string;
}

export interface SetNoteTagsInput {
  note_id: string;
  tag_ids: string[];
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
