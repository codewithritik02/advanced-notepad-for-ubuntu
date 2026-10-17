import { invoke } from "@tauri-apps/api/core";
import type { Attachment, AddAttachmentInput, AttachmentConsistencyReport } from "./types.ts";

/**
 * Storage service providing typed IPC access to SQLite attachment metadata
 * and managed attachment storage (Phase 7, Tasks 8 & 10).
 */
export const attachmentsStorage = {
  /**
   * Adds an attachment to a note by copying the source file into managed storage
   * and creating a persistent database record.
   */
  async addAttachment(
    input: AddAttachmentInput | { noteId: string; sourcePath: string }
  ): Promise<Attachment> {
    const noteId = "note_id" in input ? input.note_id : input.noteId;
    const sourcePath = "source_path" in input ? input.source_path : input.sourcePath;

    return await invoke<Attachment>("add_attachment", {
      noteId: noteId.trim(),
      sourcePath: sourcePath.trim(),
    });
  },

  /**
   * Lists all attachments for a specific note, ordered deterministically.
   */
  async listAttachments(noteId: string): Promise<Attachment[]> {
    return await invoke<Attachment[]>("list_attachments", {
      noteId: noteId.trim(),
    });
  },

  /**
   * Removes an attachment record and safely deletes its managed file from disk.
   */
  async removeAttachment(id: string): Promise<boolean> {
    return await invoke<boolean>("remove_attachment", {
      id: id.trim(),
    });
  },

  /**
   * Opens an attachment using the operating system's default viewer via the Tauri opener.
   */
  async openAttachment(id: string): Promise<void> {
    await invoke<void>("open_attachment", {
      id: id.trim(),
    });
  },

  /**
   * Returns the count of attachments associated with a note (Phase 7, Task 19).
   */
  async getAttachmentCount(noteId: string): Promise<number> {
    const trimmed = noteId.trim();
    if (!trimmed) return 0;
    return await invoke<number>("get_attachment_count", {
      noteId: trimmed,
    });
  },

  /**
   * Inspects attachment consistency and returns a detailed report (Phase 7, Task 21).
   */
  async checkConsistency(): Promise<AttachmentConsistencyReport> {
    return await invoke<AttachmentConsistencyReport>("check_attachment_consistency");
  },

  /**
   * Cleans up abandoned temporary staging files older than specified seconds (Phase 7, Task 21).
   */
  async cleanupAbandonedTempFiles(olderThanSeconds?: number): Promise<number> {
    return await invoke<number>("cleanup_abandoned_temp_files", {
      olderThanSeconds: olderThanSeconds ?? 60,
    });
  },

  /** Retries removal of a deleted note's managed attachment directory. */
  async retryNoteCleanup(noteId: string): Promise<boolean> {
    return await invoke<boolean>("retry_note_attachment_cleanup", {
      noteId: noteId.trim(),
    });
  },

  /** Removes verified managed files that no longer have attachment metadata. */
  async cleanupUnreferencedFiles(): Promise<number> {
    return await invoke<number>("cleanup_unreferenced_attachment_files");
  },
};

export default attachmentsStorage;
