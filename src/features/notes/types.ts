import { Note, CreateNoteInput, UpdateNoteInput, NoteFormat } from "../../services/storage";

// Re-export core storage domain models for convenient feature consumption
export type { Note, CreateNoteInput, UpdateNoteInput, NoteFormat };

export interface NoteListItem {
  id: string;
  title: string;
  preview: string;
  updatedAt: string;
  isFavorite?: boolean;
  notebookId?: string;
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
