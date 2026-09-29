import {
  Notebook,
  CreateNotebookInput,
  UpdateNotebookInput,
} from "../../../services/storage";

// Re-export core storage domain models for feature consumption
export type { Notebook, CreateNotebookInput, UpdateNotebookInput };

/**
 * Derived hierarchical tree node representation for the UI.
 * Hierarchy, depth, and children are UI-computed structures, never persisted directly to SQLite.
 */
export interface NotebookTreeNode extends Notebook {
  children: NotebookTreeNode[];
  depth: number;
  noteCount?: number;
}

/**
 * Explicit application navigation model separating All Notes, Unfiled Notes,
 * and specific Notebook views to avoid ambiguous null states.
 */
export type NoteLocation =
  | { type: "all" }
  | { type: "unfiled" }
  | { type: "notebook"; notebookId: string };

/**
 * Loading and error lifecycle status for notebooks.
 */
export type NotebooksStatus = "idle" | "loading" | "error";

/**
 * Dialog state for creating a new root or child notebook.
 */
export interface CreateNotebookModalState {
  isOpen: boolean;
  parentId?: string | null;
  parentName?: string | null;
}

/**
 * Dialog state for renaming an existing notebook.
 */
export interface RenameNotebookModalState {
  isOpen: boolean;
  notebook: Notebook | null;
}

/**
 * Dialog state for deleting an existing notebook safely.
 */
export interface DeleteNotebookModalState {
  isOpen: boolean;
  notebook: Notebook | null;
  hasChildren: boolean;
}

/**
 * Dialog state for moving a note between notebooks or unfiled.
 */
export interface MoveNoteModalState {
  isOpen: boolean;
  noteId: string | null;
  noteTitle: string | null;
  currentNotebookId: string | null;
}

/**
 * Structured error categories defined in Task 34 specification.
 */
export type NotebookErrorKind =
  | "ValidationError"
  | "NotFound"
  | "DatabaseError"
  | "Conflict"
  | "InvalidHierarchy"
  | "DeleteBlocked"
  | "StorageUnavailable";

export interface NotebookError {
  kind: NotebookErrorKind;
  message: string;
}

