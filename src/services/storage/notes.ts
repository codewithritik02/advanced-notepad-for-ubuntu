import { invoke } from "@tauri-apps/api/core";
import { CreateNoteInput, ListNotesOptions, Note, UpdateNoteInput } from "./types";

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
   * Moves a note to a target notebook, or unfiles it when notebookId is null.
   */
  async moveToNotebook(id: string, notebookId: string | null): Promise<Note> {
    return await invoke<Note>("move_note_to_notebook", {
      id,
      notebookId,
    });
  },
};
