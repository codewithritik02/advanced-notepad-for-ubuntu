import { attachmentsStorage } from "../../../services/storage/attachments.ts";
import type { Attachment } from "../../../services/storage/types.ts";
import type { AttachmentErrorState } from "../types.ts";

export interface AddAttachmentProgress {
  current: number;
  total: number;
  currentFileName: string;
}

export interface AddAttachmentWorkflowResult {
  targetNoteId: string;
  successful: Attachment[];
  failed: { path: string; error: string }[];
}

/**
 * Executes the add-attachment workflow for one or more files (Phase 7, Task 10).
 *
 * Guarantees:
 * 1. Resolves target note ID at invocation and anchors it throughout execution.
 * 2. Partial multi-file failures are isolated: one failing file never reverts or corrupts
 *    previously succeeded attachments.
 * 3. Notifies callers of progress as files are processed.
 * 4. Returns detailed results containing both successful attachments and any per-file failures.
 */
export async function executeAddAttachmentWorkflow(
  targetNoteId: string,
  filePaths: string[],
  onProgress?: (progress: AddAttachmentProgress) => void,
  storage: { addAttachment: (input: { noteId: string; sourcePath: string }) => Promise<Attachment> } = attachmentsStorage
): Promise<AddAttachmentWorkflowResult> {
  const trimmedNoteId = targetNoteId.trim();
  if (!trimmedNoteId) {
    throw new Error("Cannot add attachments without a valid note ID.");
  }

  if (filePaths.length === 0) {
    return {
      targetNoteId: trimmedNoteId,
      successful: [],
      failed: [],
    };
  }

  const successful: Attachment[] = [];
  const failed: { path: string; error: string }[] = [];

  for (let i = 0; i < filePaths.length; i++) {
    const rawPath = filePaths[i];
    const fileName = rawPath.split(/[/\\]/).pop() || rawPath;

    onProgress?.({
      current: i + 1,
      total: filePaths.length,
      currentFileName: fileName,
    });

    try {
      const created = await storage.addAttachment({
        noteId: trimmedNoteId,
        sourcePath: rawPath,
      });
      successful.push(created);
    } catch (err) {
      const errorMessage =
        err instanceof Error
          ? err.message
          : typeof err === "string"
          ? err
          : "Failed to attach file.";
      failed.push({
        path: rawPath,
        error: errorMessage,
      });
    }
  }

  return {
    targetNoteId: trimmedNoteId,
    successful,
    failed,
  };
}

/**
 * Maps arbitrary errors to structured AttachmentErrorState for clear UI feedback.
 */
export function mapToAttachmentErrorState(
  err: unknown,
  fallbackMessage = "An error occurred during attachment operation."
): AttachmentErrorState {
  const msg =
    err instanceof Error
      ? err.message
      : typeof err === "string"
      ? err
      : fallbackMessage;

  if (msg.includes("exceeds maximum permitted size") || msg.includes("FileTooLarge")) {
    return { kind: "FileTooLarge", message: msg };
  }
  if (msg.includes("referenced note could not be found") || msg.includes("NoteNotFound")) {
    return { kind: "NoteNotFound", message: msg };
  }
  if (msg.includes("deleted note") || msg.includes("cannot be empty")) {
    return { kind: "InvalidRequest", message: msg };
  }
  if (msg.includes("could not be read or does not exist") || msg.includes("SourceUnavailable")) {
    return { kind: "SourceUnavailable", message: msg };
  }
  if (msg.includes("missing from storage") || msg.includes("ManagedFileMissing")) {
    return { kind: "ManagedFileMissing", message: msg };
  }
  if (msg.includes("Permission denied") || msg.includes("PermissionDenied")) {
    return { kind: "PermissionDenied", message: msg };
  }
  if (
    msg.includes("Insufficient disk space") ||
    msg.includes("InsufficientDiskSpace") ||
    msg.toLowerCase().includes("disk space") ||
    msg.toLowerCase().includes("no space left")
  ) {
    return { kind: "InsufficientDiskSpace", message: msg };
  }
  if (msg.includes("Failed to open") || msg.includes("OpenFailed")) {
    return { kind: "OpenFailed", message: msg };
  }
  if (msg.includes("database error") || msg.includes("Database")) {
    return { kind: "Database", message: msg };
  }

  return { kind: "Unknown", message: msg };
}
