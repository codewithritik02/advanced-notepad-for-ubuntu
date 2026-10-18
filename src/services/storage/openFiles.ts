import { invoke } from "@tauri-apps/api/core";

/** A file the desktop asked us to open ("Open with", double click) and the note it became. */
export interface OpenedFile {
  path: string;
  fileName: string;
  noteId: string;
  title: string;
  format: string;
  /** False when the same, unchanged file was already open as a note and that note was reused. */
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
};
