import { open } from "@tauri-apps/plugin-dialog";

export interface FileDialogOptions {
  title?: string;
  multiple?: boolean;
}

/**
 * Opens the native desktop file picker to select local files for attachment (Phase 7, Task 5).
 * - Multi-selection supported by default.
 * - User cancellation gracefully returns an empty array `[]` without error or mutation.
 * - Does not read or load file bytes into frontend memory.
 * - Returns source file paths for handoff to the backend attachment service.
 */
export async function pickAttachmentFiles(
  options: FileDialogOptions = {}
): Promise<string[]> {
  try {
    const selected = await open({
      title: options.title ?? "Select files to attach",
      multiple: options.multiple ?? true,
      directory: false,
    });

    if (!selected) {
      return [];
    }

    if (Array.isArray(selected)) {
      return selected;
    }

    return [selected];
  } catch (err) {
    // Cancellation or environment failure returns empty list cleanly
    return [];
  }
}
