import { invoke } from "@tauri-apps/api/core";
import { CreateNotebookInput, Notebook } from "./types";

export const notebooksStorage = {
  /**
   * Retrieves all notebooks.
   */
  async list(): Promise<Notebook[]> {
    return await invoke<Notebook[]>("list_notebooks");
  },

  /**
   * Creates a new notebook with optional parent_id for nesting.
   */
  async create(input: CreateNotebookInput): Promise<Notebook> {
    return await invoke<Notebook>("create_notebook", { dto: input });
  },
};
