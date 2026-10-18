import { invoke } from "@tauri-apps/api/core";

/** A file opened from outside the notebook. It lives in the sidebar's External section. */
export interface ExternalDocument {
  id: string;
  path: string;
  fileName: string;
  format: string;
  openedAt: string;
  /** Pinned files stay in the External list after the app is restarted. */
  pinned: boolean;
}

/** A file the desktop asked us to open ("Open with", double click) and the document it became. */
export interface OpenedFile {
  path: string;
  fileName: string;
  /** Id of the external document (never a note id). */
  noteId: string;
  title: string;
  format: string;
  /** Kept for compatibility; always true now that files are listed as external documents. */
  created: boolean;
}

export interface FailedFile {
  path: string;
  fileName: string;
  message: string;
}

export interface OpenFilesReport {
  opened: OpenedFile[];
  failed: FailedFile[];
}

export const openFilesStorage = {
  /**
   * Opens the files waiting from the command line or a second launch as notes. While the app is
   * locked this rejects and the files stay queued until the user unlocks it.
   */
  async processPending(): Promise<OpenFilesReport> {
    return await invoke<OpenFilesReport>("process_pending_open_files");
  },

  /** Files opened from outside the notebook, newest first. */
  async listExternal(): Promise<ExternalDocument[]> {
    return await invoke<ExternalDocument[]>("list_external_documents");
  },

  /** Removes a document from the External list. The file on disk is not touched. */
  async closeExternal(id: string): Promise<boolean> {
    return await invoke<boolean>("close_external_document", { id });
  },

  /** Pins or unpins an external document. */
  async setExternalPinned(id: string, pinned: boolean): Promise<boolean> {
    return await invoke<boolean>("set_external_pinned", { id, pinned });
  },

  /** Opens one file by path (e.g. from the Recent menu) as an external document. */
  async openPath(path: string): Promise<OpenFilesReport> {
    return await invoke<OpenFilesReport>("open_external_file", { path });
  },
};
