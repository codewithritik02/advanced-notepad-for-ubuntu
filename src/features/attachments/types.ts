import type { Attachment, AddAttachmentInput } from "../../services/storage/types.ts";

export type { Attachment, AddAttachmentInput };

/**
 * Status of the attachments subsystem or individual attachment operations.
 */
export type AttachmentStatus =
  | "idle"
  | "loading"
  | "adding"
  | "removing"
  | "opening"
  | "error";

/**
 * Meaningful error categories matching the backend AttachmentError model (Phase 7, Task 2).
 */
export type AttachmentErrorKind =
  | "NoteNotFound"
  | "AttachmentNotFound"
  | "InvalidRequest"
  | "FileTooLarge"
  | "SourceUnavailable"
  | "PermissionDenied"
  | "InsufficientDiskSpace"
  | "ManagedFileMissing"
  | "FileOperationFailed"
  | "ExecutableBlocked"
  | "OpenFailed"
  | "Database"
  | "Unknown";

/**
 * Structured frontend attachment error representation.
 */
export interface AttachmentErrorState {
  kind: AttachmentErrorKind;
  message: string;
  attachmentId?: string;
}

/**
 * Props for rendering individual attachment items.
 */
export interface AttachmentItemViewProps {
  attachment: Attachment;
  onOpen: (id: string) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  isBusy?: boolean;
}
