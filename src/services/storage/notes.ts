import { invoke } from "@tauri-apps/api/core";
import { CreateNoteInput, ListNotesOptions, Note, NoteMetadata, UpdateNoteInput } from "./types";

export const notesStorage = {
  /**
   * Retrieves notes list ordered by pinned status and last modification date.
   * Supports filtering by notebook or unfiled status.
   */
  async list(options?: ListNotesOptions | boolean): Promise<Note[]> {
    if (typeof options === "boolean") {
      return await invoke<Note[]>("list_notes", { includeDeleted: options });
    }
    const { includeDeleted = false, notebookId, unfiledOnly = false } = options || {};
    return await invoke<Note[]>("list_notes", {
      includeDeleted,
      notebookId: notebookId ?? null,
      unfiledOnly,
    });
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
  async listFavorites(): Promise<Note[]> {
    return await invoke<Note[]>("list_favorite_notes");
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
};

