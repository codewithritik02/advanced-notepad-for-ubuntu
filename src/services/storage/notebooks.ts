import { invoke } from "@tauri-apps/api/core";
import { CreateNotebookInput, Notebook, UpdateNotebookInput } from "./types";

export const notebooksStorage = {
  /**
   * Retrieves all notebooks.
   */
  async list(): Promise<Notebook[]> {
    return await invoke<Notebook[]>("list_notebooks");
  },

  /**
   * Retrieves a single notebook by unique ID.
   */
  async get(id: string): Promise<Notebook | null> {
    return await invoke<Notebook | null>("get_notebook", { id });
  },

  /**
   * Creates a new notebook with optional parent_id for nesting.
   */
  async create(input: CreateNotebookInput): Promise<Notebook> {
    return await invoke<Notebook>("create_notebook", { dto: input });
  },

  /**
   * Updates an existing notebook (rename).
   */
  async update(id: string, input: UpdateNotebookInput): Promise<Notebook> {
    return await invoke<Notebook>("update_notebook", { id, dto: input });
  },

  /**
   * Convenience alias to rename a notebook.
   */
  async rename(id: string, name: string): Promise<Notebook> {
    return await this.update(id, { name });
  },

  /**
   * Deletes a notebook safely. Fails if child notebooks exist.
   * Associated notes automatically have notebook_id unassigned to NULL (unfiled).
   */
  async delete(id: string): Promise<boolean> {
    return await invoke<boolean>("delete_notebook", { id });
  },
};
