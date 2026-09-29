import { invoke } from "@tauri-apps/api/core";
import { Tag } from "./types";

export const tagsStorage = {
  /**
   * Retrieves all tags.
   */
  async list(): Promise<Tag[]> {
    return await invoke<Tag[]>("list_tags");
  },

  /**
   * Creates a new unique tag.
   */
  async create(name: string): Promise<Tag> {
    return await invoke<Tag>("create_tag", { name });
  },

  /**
   * Associates a tag with a note.
   */
  async addToNote(noteId: string, tagId: string): Promise<void> {
    await invoke<void>("add_note_tag", { noteId, tagId });
  },

  /**
   * Retrieves all tags associated with a specific note.
   */
  async getForNote(noteId: string): Promise<Tag[]> {
    return await invoke<Tag[]>("get_note_tags", { noteId });
  },
};
