import { notebooksStorage } from "../../../services/storage";
import type { Notebook, UpdateNotebookInput } from "../types";

/**
 * Feature-layer notebook service providing high-level typed operations for UI components.
 * Consolidates all Tauri command invocations into a single consistent API.
 */
export const notebookService = {
  /**
   * Retrieves all notebooks from SQLite.
   */
  async list(): Promise<Notebook[]> {
    return await notebooksStorage.list();
  },

  /**
   * Retrieves a single notebook by ID.
   */
  async get(id: string): Promise<Notebook | null> {
    return await notebooksStorage.get(id);
  },

  /**
   * Creates a new root or nested notebook.
   */
  async create(name: string, parentId?: string | null): Promise<Notebook> {
    return await notebooksStorage.create({
      name,
      parent_id: parentId ?? null,
    });
  },

  /**
   * Renames a notebook by ID.
   */
  async rename(id: string, name: string): Promise<Notebook> {
    return await notebooksStorage.rename(id, name);
  },

  /**
   * Updates an existing notebook with partial input.
   */
  async update(id: string, input: UpdateNotebookInput): Promise<Notebook> {
    return await notebooksStorage.update(id, input);
  },

  /**
   * Deletes a notebook by ID safely.
   * Fails if sub-notebooks still exist. Notes inside are reset to unfiled (NULL).
   */
  async delete(id: string): Promise<boolean> {
    return await notebooksStorage.delete(id);
  },
};

export default notebookService;
