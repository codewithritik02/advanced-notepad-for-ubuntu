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
 * Favorites, Notebook, Tag, and Trash views to avoid ambiguous null states.
 *
 * This is the single source of truth for navigation context in the application.
 * Search state (searchQuery) is kept entirely separate from this model —
 * do NOT create tagSearchLocation / favoriteSearchLocation / notebookSearchLocation
 * variants. Use the composition layer in scopeSearchResults instead (Task 42).
 */
export type NoteLocation =
  | { type: "all" }
  | { type: "unfiled" }
  | { type: "favorites" }
  | { type: "notebook"; notebookId: string }
  | { type: "tag"; tagId: string }
  | { type: "trash" };

/**
 * Maps a NoteLocation to its NavItemId string equivalent used by the Sidebar
 * component. This bridge avoids duplicating the location model inside
 * presentation-layer components.
 */
export function locationToNavId(
  loc: NoteLocation
): "all-notes" | "unfiled" | "favorites" | "notebook" | "tags" | "trash" {
  switch (loc.type) {
    case "all":      return "all-notes";
    case "unfiled":  return "unfiled";
    case "favorites":return "favorites";
    case "notebook": return "notebook";
    case "tag":      return "tags";
    case "trash":    return "trash";
  }
}

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

