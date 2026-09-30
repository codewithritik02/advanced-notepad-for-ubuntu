import type { Note, CreateNoteInput, UpdateNoteInput, NoteFormat, NoteMetadata, Tag } from "../../services/storage";

// Re-export core storage domain models for convenient feature consumption
export type { Note, CreateNoteInput, UpdateNoteInput, NoteFormat, NoteMetadata, Tag };

/**
 * Computes canonical NoteMetadata from note entity, associated tags, and optional notebook hierarchy context.
 */
export function computeNoteMetadata(
  note: Note,
  tags: Tag[] = [],
  notebookName?: string | null,
  notebookPath?: string | null
): NoteMetadata {
  const content = note.content || "";
  const trimmed = content.trim();
  const word_count = trimmed.length === 0 ? 0 : trimmed.split(/\s+/).length;
  const character_count = content.length;
  const byte_size = new TextEncoder().encode(content).length;

  return {
    note_id: note.id,
    created_at: note.created_at,
    modified_at: note.modified_at,
    format: note.format,
    notebook_id: note.notebook_id,
    notebook_name: notebookName ?? null,
    notebook_path: notebookPath ?? notebookName ?? null,
    is_favorite: note.is_favorite,
    is_pinned: note.is_pinned,
    tags,
    word_count,
    character_count,
    byte_size,
  };
}

/**
 * Returns human-readable format display name per Phase 5 Task 32 requirements:
 * 'txt' -> 'Plain Text'
 * 'md'  -> 'Markdown'
 */
export function getFormatDisplayName(format?: string | null): "Markdown" | "Plain Text" {
  return format?.toLowerCase() === "md" ? "Markdown" : "Plain Text";
}

export interface NoteListItem {
  id: string;
  title: string;
  preview: string;
  updatedAt: string;
  isFavorite?: boolean;
  notebookId?: string;
  notebookPath?: string;
  tags?: string[];
}

export type NotesListStatus = "idle" | "loading" | "error" | "empty";

export interface EditorSelection {
  start: number;
  end: number;
}

export type EditorStatus =
  | "idle"
  | "loading"
  | "saving"
  | "saved"
  | "unsaved"
  | "error";

export interface EditorState {
  noteId: string | null;
  title: string;
  content: string;
  format: NoteFormat;
  isDirty: boolean;
  isSaving: boolean;
  lastSavedAt: string | null;
  error: string | null;
  cursorPosition: number;
  selection: EditorSelection;
}
