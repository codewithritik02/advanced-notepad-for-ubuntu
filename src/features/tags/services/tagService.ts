import { tagsStorage, Note } from "../../../services/storage";
import { validateTagName } from "../utils/tagUtils";
import type { Tag, UpdateTagInput } from "../types";

/**
 * Feature-layer Tag Service providing high-level typed operations for UI components.
 * Consolidates all Tag Tauri command invocations into a single consistent API.
 */
export const tagService = {
  /**
   * Retrieves all tags from SQLite, ordered alphabetically.
   */
  async list(): Promise<Tag[]> {
    return await tagsStorage.list();
  },

  /**
   * Retrieves a single tag by ID.
   */
  async get(id: string): Promise<Tag | null> {
    return await tagsStorage.get(id);
  },

  /**
   * Creates a new tag with client-side trimming and validation.
   * If a tag with the same name already exists (case-insensitively), SQLite returns the existing tag.
   */
  async create(name: string): Promise<Tag> {
    const validation = validateTagName(name);
    if (!validation.valid) {
      throw new Error(validation.error || "Invalid tag name");
    }
    return await tagsStorage.create(validation.trimmed);
  },

  /**
   * Renames a tag by ID with client-side validation.
   */
  async rename(id: string, name: string): Promise<Tag> {
    const validation = validateTagName(name);
    if (!validation.valid) {
      throw new Error(validation.error || "Invalid tag name");
    }
    return await tagsStorage.rename(id, validation.trimmed);
  },

  /**
   * Updates an existing tag with partial input.
   */
  async update(id: string, input: UpdateTagInput): Promise<Tag> {
    const validation = validateTagName(input.name);
    if (!validation.valid) {
      throw new Error(validation.error || "Invalid tag name");
    }
    return await tagsStorage.update(id, { name: validation.trimmed });
  },

  /**
   * Deletes a tag by ID.
   * Cascade-deletes associations in note_tags while leaving notes completely intact.
   */
  async delete(id: string): Promise<boolean> {
    return await tagsStorage.delete(id);
  },

  /**
   * Associates a tag with a note.
   */
  async addToNote(noteId: string, tagId: string): Promise<void> {
    await tagsStorage.addToNote(noteId, tagId);
  },

  /**
   * Removes an association between a tag and a note.
   */
  async removeFromNote(noteId: string, tagId: string): Promise<void> {
    await tagsStorage.removeFromNote(noteId, tagId);
  },

  /**
   * Retrieves all tags associated with a specific note.
   */
  async getForNote(noteId: string): Promise<Tag[]> {
    return await tagsStorage.getForNote(noteId);
  },

  /**
   * Replaces all tags for a note atomically with the provided tag IDs.
   */
  async setTagsForNote(noteId: string, tagIds: string[]): Promise<Tag[]> {
    return await tagsStorage.setForNote(noteId, tagIds);
  },

  /**
   * Retrieves all non-deleted notes associated with a specific tag (Task 24).
   */
  async getNotesForTag(tagId: string): Promise<Note[]> {
    return await tagsStorage.getNotesForTag(tagId);
  },

  /**
   * Retrieves active note counts per tag.
   */
  async getTagNoteCounts(): Promise<Record<string, number>> {
    return await tagsStorage.getTagNoteCounts();
  },
};

export default tagService;
