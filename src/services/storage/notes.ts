import { invoke } from "@tauri-apps/api/core";
import {
  CreateNoteInput,
  EmptyTrashResult,
  ListNotesOptions,
  Note,
  NoteCountsDto,
  NoteMetadata,
  PermanentDeleteResult,
  TrashedNoteItem,
  UpdateNoteInput,
} from "./types";

export const notesStorage = {
  /**
   * Retrieves notes list ordered by pinned status and last modification date.
   * Supports filtering by notebook or unfiled status, and bounded previews (Phase 13, Task 9).
   */
  async list(options?: ListNotesOptions | boolean): Promise<Note[]> {
    if (typeof options === "boolean") {
      return await invoke<Note[]>("list_notes", { includeDeleted: options });
    }
    const {
      includeDeleted = false,
      notebookId,
      unfiledOnly = false,
      limit,
      offset,
      contentLimit = 300,
    } = options || {};
    return await invoke<Note[]>("list_notes", {
      includeDeleted,
      notebookId: notebookId ?? null,
      unfiledOnly,
      limit: limit ?? null,
      offset: offset ?? null,
      contentLimit: contentLimit ?? null,
    });
  },

  /**
   * Retrieves aggregate counts of active, favorite, unfiled, trashed, and per-notebook notes without loading full records (Task 9).
   */
  async getNoteCounts(): Promise<NoteCountsDto> {
    return await invoke<NoteCountsDto>("get_note_counts");
  },

  /**
   * Retrieves a single note by unique ID.
   */
  async get(id: string): Promise<Note | null> {
    return await invoke<Note | null>("get_note", { id });
  },

  /**
   * Creates a new note in SQLite.
   */
  async create(input: CreateNoteInput = {}): Promise<Note> {
    return await invoke<Note>("create_note", { dto: input });
  },

  /**
   * Updates an existing note in SQLite.
   */
  async update(id: string, input: UpdateNoteInput): Promise<Note> {
    return await invoke<Note>("update_note", { id, dto: input });
  },

  /**
   * Deletes a note (defaults to soft delete).
   */
  async delete(id: string, soft = true): Promise<boolean> {
    return await invoke<boolean>("delete_note", { id, soft });
  },

  /**
   * Restores a soft-deleted note in SQLite.
   */
  async restore(id: string): Promise<Note> {
    return await invoke<Note>("restore_note", { id });
  },

  /**
   * Moves a note to a target notebook, or unfiles it when notebookId is null.
   */
  async moveToNotebook(id: string, notebookId: string | null): Promise<Note> {
    return await invoke<Note>("move_note_to_notebook", {
      id,
      notebookId,
    });
  },

  /**
   * Sets or unsets favorite status for a note in SQLite.
   */
  async setFavorite(id: string, isFavorite: boolean): Promise<Note> {
    return await invoke<Note>("set_note_favorite", { id, isFavorite });
  },

  /**
   * Toggles favorite status for a note in SQLite.
   */
  async toggleFavorite(id: string): Promise<Note> {
    return await invoke<Note>("toggle_note_favorite", { id });
  },

  /**
   * Lists all favorite, non-deleted notes ordered by modified date.
   */
  async listFavorites(contentLimit: number = 300): Promise<Note[]> {
    return await invoke<Note[]>("list_favorite_notes", {
      contentLimit: contentLimit ?? null,
    });
  },

  /**
   * Retrieves canonical note metadata including tags, notebook path, and content metrics (Task 29, 33).
   */
  async getMetadata(noteId: string): Promise<NoteMetadata | null> {
    return await invoke<NoteMetadata | null>("get_note_metadata", { noteId });
  },

  /**
   * Retrieves human-readable notebook path for a note (Task 33).
   */
  async getNotebookPath(noteId: string): Promise<string> {
    return await invoke<string>("get_note_notebook_path", { noteId });
  },

  /**
   * Retrieves lightweight trashed note items (Phase 8, Task 10).
   */
  async listTrashed(): Promise<TrashedNoteItem[]> {
    return await invoke<TrashedNoteItem[]>("list_trashed_notes");
  },

  /**
   * Soft-deletes a note by ID (Phase 8, Task 5 & 9).
   */
  async softDelete(id: string): Promise<Note> {
    return await invoke<Note>("soft_delete_note", { id });
  },

  /**
   * Restores a note by ID from Trash (Phase 8, Task 6 & 9).
   */
  async restoreNote(id: string): Promise<Note> {
    return await invoke<Note>("restore_note", { id });
  },

  /**
   * Permanently deletes a note by ID from Trash (Phase 8, Task 7 & 9).
   */
  async permanentlyDelete(id: string): Promise<PermanentDeleteResult> {
    return await invoke<PermanentDeleteResult>("permanently_delete_note", { id });
  },

  /**
   * Empties the entire Trash (Phase 8, Task 8 & 9).
   */
  async emptyTrash(): Promise<EmptyTrashResult> {
    return await invoke<EmptyTrashResult>("empty_trash");
  },
};

