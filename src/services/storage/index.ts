import { invoke } from "@tauri-apps/api/core";
import { StorageInfo } from "./types";
import { notesStorage } from "./notes";
import { notebooksStorage } from "./notebooks";
import { tagsStorage } from "./tags";
import { settingsStorage } from "./settings";
import { searchStorage } from "./search";

export const storageService = {
  /**
   * Retrieves persistent storage directories and initialization state.
   */
  async getInfo(): Promise<StorageInfo> {
    return await invoke<StorageInfo>("get_storage_info");
  },

  notes: notesStorage,
  notebooks: notebooksStorage,
  tags: tagsStorage,
  settings: settingsStorage,
  search: searchStorage,
};

export * from "./types";
export { notesStorage } from "./notes";
export { notebooksStorage } from "./notebooks";
export { tagsStorage } from "./tags";
export { settingsStorage } from "./settings";
export { searchStorage } from "./search";
export default storageService;
