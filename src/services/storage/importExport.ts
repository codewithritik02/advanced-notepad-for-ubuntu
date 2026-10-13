import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  ExportBatchRequest,
  ExportFileResult,
  ExportProgress,
  ExportScope,
  ExportSelectionPreview,
  ExportSingleNoteRequest,
  ExportSummary,
  ImportPreview,
  ImportProgress,
  ImportRequest,
  ImportSummary,
} from "./types";

export const importExportStorage = {
  /**
   * Scans candidate file/folder paths, inspects format/size, checks duplicates,
   * and returns a structured ImportPreview report.
   * Safe cancellation: empty paths returns an empty preview cleanly without invoking backend.
   */
  async scanImportFiles(paths: string[]): Promise<ImportPreview> {
    if (!paths || paths.length === 0) {
      return {
        candidates: [],
        totalFiles: 0,
        validFiles: 0,
        invalidFiles: 0,
        duplicateFiles: 0,
        totalSizeBytes: 0,
      };
    }
    return await invoke<ImportPreview>("scan_import_files", { paths });
  },

  /**
   * Commits the selected import candidates within atomic backend transactions.
   * Enforces chosen duplicate policy, preserves raw text/markdown syntax verbatim,
   * and returns an itemized ImportSummary report.
   */
  async executeImport(request: ImportRequest): Promise<ImportSummary> {
    return await invoke<ImportSummary>("execute_import", { request });
  },

  /**
   * Cancels an in-flight import or export operation by its unique operationId.
   */
  async cancelOperation(operationId: string): Promise<boolean> {
    return await invoke<boolean>("cancel_import_export_operation", { operationId });
  },

  /**
   * Subscribes to live progress events emitted during import processing.
   */
  async onImportProgress(callback: (progress: ImportProgress) => void): Promise<UnlistenFn> {
    return await listen<ImportProgress>("import-progress", (event) => {
      callback(event.payload);
    });
  },

  /**
   * Subscribes to live progress events emitted during batch export processing.
   */
  async onExportProgress(callback: (progress: ExportProgress) => void): Promise<UnlistenFn> {
    return await listen<ExportProgress>("export-progress", (event) => {
      callback(event.payload);
    });
  },

  /**
   * Exports an individual note to a user-selected destination file.
   * Reads the latest persisted note content and writes verbatim UTF-8 / JSON.
   */
  async exportSingleNote(request: ExportSingleNoteRequest): Promise<ExportFileResult> {
    return await invoke<ExportFileResult>("export_single_note", { request });
  },

  /**
   * Previews a proposed export selection, calculating counts and lightweight metadata without loading full content.
   */
  async previewExportSelection(scope: ExportScope): Promise<ExportSelectionPreview> {
    return await invoke<ExportSelectionPreview>("preview_export_selection", { scope });
  },

  /**
   * Executes a batch export for multiple notes or notebook selection.
   */
  async exportNotesBatch(request: ExportBatchRequest): Promise<ExportSummary> {
    return await invoke<ExportSummary>("export_notes_batch", { request });
  },
};
