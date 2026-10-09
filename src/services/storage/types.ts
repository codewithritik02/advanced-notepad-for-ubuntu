/**
 * Strongly-typed domain models and DTOs corresponding to SQLite schema entities.
 */

export type NoteFormat = "txt" | "md";

/**
 * Strongly-typed lifecycle state of a note (Phase 8, Task 2)
 */
export type NoteLifecycle = "active" | "trashed";

/**
 * Derives the authoritative NoteLifecycle state from the is_deleted flag.
 */
export function getNoteLifecycle(note: { is_deleted: boolean }): NoteLifecycle {
  return note.is_deleted ? "trashed" : "active";
}

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

/**
 * Lightweight DTO representation for trashed notes listing (Phase 8, Task 10)
 */
export interface TrashedNoteItem {
  id: string;
  title: string;
  preview: string;
  notebookId: string | null;
  notebookName?: string | null;
  createdAt: string;
  modifiedAt: string;
  deletedAt: string | null;
  isFavorite: boolean;
  attachmentCount: number;
}

/**
 * Result report of a single permanent deletion operation (Phase 8, Task 7)
 */
export interface PermanentDeleteResult {
  noteId: string;
  success: boolean;
  attachmentCleanupFailed: boolean;
  errorMessage?: string | null;
}

/**
 * Summary report of an empty-trash batch operation (Phase 8, Task 8)
 */
export interface EmptyTrashResult {
  deletedCount: number;
  failedCount: number;
  attachmentCleanupFailures: string[];
  errors: string[];
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
  attachment_count?: number;
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

export interface SearchQuery {
  query: string;
  limit?: number;
}

export interface SearchResult {
  noteId: string;
  title: string;
  snippet?: string;
  modifiedAt: string;
  notebookId: string | null;
  favorite: boolean;
}

export interface Attachment {
  id: string;
  note_id: string;
  file_name: string;
  relative_path?: string;
  mime_type: string | null;
  file_size: number;
  created_at: string;
  modified_at?: string;
  // Convenience aliases for specification compatibility
  original_name?: string;
  size_bytes?: number;
}

export interface AddAttachmentInput {
  note_id: string;
  source_path: string;
}

export interface CreateAttachmentInput {
  note_id: string;
  file_name: string;
  relative_path: string;
  mime_type: string | null;
  file_size: number;
}

export interface MissingManagedFileIssue {
  attachment_id: string;
  note_id: string;
  file_name: string;
  relative_path: string;
}

export interface SizeMismatchIssue {
  attachment_id: string;
  note_id: string;
  file_name: string;
  relative_path: string;
  recorded_size: number;
  actual_size: number;
}

export interface AttachmentConsistencyReport {
  missing_managed_files: MissingManagedFileIssue[];
  unreferenced_managed_files: string[];
  abandoned_temp_files: string[];
  size_mismatches: SizeMismatchIssue[];
  total_attachments_checked: number;
  total_disk_files_checked: number;
  is_consistent: boolean;
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
