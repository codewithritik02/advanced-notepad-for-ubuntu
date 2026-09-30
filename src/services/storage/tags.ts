import { invoke } from "@tauri-apps/api/core";
import { Tag, UpdateTagInput, Note } from "./types";

export const tagsStorage = {
  /**
   * Retrieves all tags.
   */
  async list(): Promise<Tag[]> {
    return await invoke<Tag[]>("list_tags");
  },

  /**
   * Retrieves a single tag by ID.
   */
  async get(id: string): Promise<Tag | null> {
    return await invoke<Tag | null>("get_tag", { id });
  },

  /**
   * Creates a new unique tag or returns existing if name matches case-insensitively.
   */
  async create(name: string): Promise<Tag> {
    return await invoke<Tag>("create_tag", { name });
  },

  /**
   * Updates/renames a tag by ID.
   */
  async update(id: string, dto: UpdateTagInput): Promise<Tag> {
    return await invoke<Tag>("update_tag", { id, dto });
  },

  /**
   * Renames a tag by ID.
   */
  async rename(id: string, name: string): Promise<Tag> {
    return await invoke<Tag>("update_tag", { id, dto: { name } });
  },

  /**
   * Deletes a tag by ID.
   */
  async delete(id: string): Promise<boolean> {
    return await invoke<boolean>("delete_tag", { id });
  },

  /**
   * Associates a tag with a note.
   */
  async addToNote(noteId: string, tagId: string): Promise<void> {
    await invoke<void>("add_note_tag", { noteId, tagId });
  },

  /**
   * Removes association between a tag and a note.
   */
  async removeFromNote(noteId: string, tagId: string): Promise<void> {
    await invoke<void>("remove_note_tag", { noteId, tagId });
  },

  /**
   * Retrieves all tags associated with a specific note.
   */
  async getForNote(noteId: string): Promise<Tag[]> {
    return await invoke<Tag[]>("get_note_tags", { noteId });
  },

  /**
   * Sets all tags for a note atomically, replacing existing tag assignments.
   */
  async setForNote(noteId: string, tagIds: string[]): Promise<Tag[]> {
    return await invoke<Tag[]>("set_note_tags", { noteId, tagIds });
  },

  /**
   * Retrieves all non-deleted notes associated with a specific tag (Task 24).
   */
  async getNotesForTag(tagId: string): Promise<Note[]> {
    return await invoke<Note[]>("get_notes_for_tag", { tagId });
  },

  /**
   * Retrieves note count mapping for each tag.
   */
  async getTagNoteCounts(): Promise<Record<string, number>> {
    return await invoke<Record<string, number>>("get_tag_note_counts");
  },

  /**
   * Retrieves all non-deleted notes' tag names map (note_id -> string[]).
   */
  async getAllNotesTags(): Promise<Record<string, string[]>> {
    return await invoke<Record<string, string[]>>("get_all_notes_tags");
  },

  /**
   * Retrieves tag names for a specific batch of note IDs (Task 60, Task 61).
   * Avoids N+1 queries by issuing a single batch query.
   */
  async getTagsForNotes(noteIds: string[]): Promise<Record<string, string[]>> {
    if (!noteIds || noteIds.length === 0) return {};
    return await invoke<Record<string, string[]>>("get_tags_for_notes", { noteIds });
  },
};


