import { invoke } from "@tauri-apps/api/core";
import { StorageInfo } from "./types";
import { notesStorage } from "./notes";
import { notebooksStorage } from "./notebooks";
import { tagsStorage } from "./tags";
import { settingsStorage } from "./settings";
import { searchStorage } from "./search";
import { attachmentsStorage } from "./attachments";
import { linksStorage } from "./links";
import { importExportStorage } from "./importExport";
import { backupStorage } from "./backup";
import { cryptoStorage } from "./crypto";

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
  attachments: attachmentsStorage,
  links: linksStorage,
  importExport: importExportStorage,
  backup: backupStorage,
  crypto: cryptoStorage,
};

export * from "./types";
export * from "./crypto";
export { notesStorage } from "./notes";
export { notebooksStorage } from "./notebooks";
export { tagsStorage } from "./tags";
export { settingsStorage } from "./settings";
export { searchStorage } from "./search";
export { attachmentsStorage } from "./attachments";
export { linksStorage } from "./links";
export { importExportStorage } from "./importExport";
export { backupStorage } from "./backup";
export { cryptoStorage } from "./crypto";
export default storageService;

