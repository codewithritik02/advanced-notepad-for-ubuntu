import {
  Tag,
  CreateTagInput,
  UpdateTagInput,
  NoteTagAssignment,
  SetNoteTagsInput,
} from "../../../services/storage";

// Re-export core storage domain models for feature consumption
export type {
  Tag,
  CreateTagInput,
  UpdateTagInput,
  NoteTagAssignment,
  SetNoteTagsInput,
};

/**
 * Status of tag loading and operations.
 */
export type TagsStatus = "idle" | "loading" | "error";

/**
 * Dialog state for creating a new tag.
 */
export interface CreateTagModalState {
  isOpen: boolean;
  initialName?: string;
}

/**
 * Dialog state for renaming an existing tag.
 */
export interface RenameTagModalState {
  isOpen: boolean;
  tag: Tag | null;
}

/**
 * Dialog state for deleting an existing tag safely.
 */
export interface DeleteTagModalState {
  isOpen: boolean;
  tag: Tag | null;
  noteCount?: number;
}

