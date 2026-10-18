import { open, save } from "@tauri-apps/plugin-dialog";
import { ExportFormat } from "../../../services/storage/types";

export interface FilePickerResult {
  cancelled: boolean;
  paths: string[];
}

export interface SavePickerResult {
  cancelled: boolean;
  path: string | null;
}

/**
 * Native file picker for selecting one or multiple notes to import.
 * Plain text, Markdown and JSON files are offered. JSON is imported as plain text only, never parsed.
 * Handles user cancellation gracefully without throwing.
 */
export async function pickImportFiles(): Promise<FilePickerResult> {
  try {
    const selected = await open({
      title: "Select Notes to Import",
      multiple: true,
      directory: false,
      filters: [
        {
          name: "Notes (*.txt, *.md, *.markdown, *.json)",
          extensions: ["txt", "md", "markdown", "json"],
        },
        {
          name: "Markdown Files (*.md, *.markdown)",
          extensions: ["md", "markdown"],
        },
        {
          name: "Plain Text Files (*.txt)",
          extensions: ["txt"],
        },
        {
          name: "JSON as text (*.json)",
          extensions: ["json"],
        },
      ],
    });

    if (!selected) {
      return { cancelled: true, paths: [] };
    }

    const paths = Array.isArray(selected) ? selected : [selected];
    if (paths.length === 0) {
      return { cancelled: true, paths: [] };
    }

    return { cancelled: false, paths };
  } catch (err) {
    console.warn("File selection cancelled or failed:", err);
    return { cancelled: true, paths: [] };
  }
}

/**
 * Native directory picker for importing all supported notes from a folder.
 */
export async function pickImportFolder(): Promise<FilePickerResult> {
  try {
    const selected = await open({
      title: "Select Folder Containing Notes to Import",
      multiple: false,
      directory: true,
    });

    if (!selected) {
      return { cancelled: true, paths: [] };
    }

    const path = typeof selected === "string" ? selected : selected[0];
    if (!path) {
      return { cancelled: true, paths: [] };
    }

    return { cancelled: false, paths: [path] };
  } catch (err) {
    console.warn("Folder selection cancelled or failed:", err);
    return { cancelled: true, paths: [] };
  }
}

/**
 * Native save picker for choosing an export destination for a single note or structured JSON.
 */
export async function pickExportDestinationFile(
  defaultName: string,
  format: ExportFormat
): Promise<SavePickerResult> {
  try {
    const filter =
      format === "md"
        ? [{ name: "Markdown (*.md)", extensions: ["md"] }]
        : format === "json"
        ? [{ name: "Interchange JSON (*.json)", extensions: ["json"] }]
        : [{ name: "Plain Text (*.txt)", extensions: ["txt"] }];

    const selected = await save({
      title: "Export Note",
      defaultPath: defaultName,
      filters: filter,
    });

    if (!selected) {
      return { cancelled: true, path: null };
    }

    return { cancelled: false, path: selected };
  } catch (err) {
    console.warn("Save destination selection cancelled or failed:", err);
    return { cancelled: true, path: null };
  }
}

/**
 * Native directory picker for selecting an output destination for multi-note batch export.
 */
export async function pickExportDestinationFolder(): Promise<SavePickerResult> {
  try {
    const selected = await open({
      title: "Select Export Destination Folder",
      multiple: false,
      directory: true,
    });

    if (!selected) {
      return { cancelled: true, path: null };
    }

    const path = typeof selected === "string" ? selected : selected[0];
    return { cancelled: false, path: path || null };
  } catch (err) {
    console.warn("Folder destination selection cancelled or failed:", err);
    return { cancelled: true, path: null };
  }
}
