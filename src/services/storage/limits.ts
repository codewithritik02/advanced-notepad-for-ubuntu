/**
 * Centralized Resource Limits & Reliability Invariants (Phase 13, Task 2)
 *
 * Frontend mirror of the backend limits defined in `src-tauri/src/storage/limits.rs`.
 * Centralizes bounds across notes, attachments, imports/exports, search, and bulk operations.
 */

// --- Note Content & Title Bounds ---
/** Maximum note content size in bytes: 10 MiB (10,485,760 bytes). */
export const MAX_NOTE_CONTENT_BYTES = 10 * 1024 * 1024;

/** Maximum note title length in Unicode characters: 500 characters. */
export const MAX_NOTE_TITLE_CHARS = 500;

/** Maximum tag name length in characters: 100 characters. */
export const MAX_TAG_NAME_CHARS = 100;

/** Maximum notebook name length in characters: 200 characters. */
export const MAX_NOTEBOOK_NAME_CHARS = 200;

/** Maximum notebook hierarchy nesting depth: 3 levels. */
export const MAX_NOTEBOOK_NESTING_DEPTH = 3;

// --- Attachment Bounds ---
/** Default maximum attachment file size: 100 MiB (104,857,600 bytes). */
export const MAX_ATTACHMENT_SIZE_BYTES = 100 * 1024 * 1024;

// --- Import / Export Bounds ---
/** Maximum individual import file size: 10 MiB. */
export const MAX_IMPORT_FILE_SIZE_BYTES = 10 * 1024 * 1024;

/** Maximum aggregate import batch size: 100 MiB. */
export const MAX_IMPORT_BATCH_SIZE_BYTES = 100 * 1024 * 1024;

/** Maximum number of files processed in an import or export batch: 500 items. */
export const MAX_BATCH_FILE_COUNT = 500;

// --- Backup & Restore Bounds ---
/** Maximum total uncompressed backup archive size: 5 GiB. */
export const MAX_BACKUP_UNCOMPRESSED_BYTES = 5 * 1024 * 1024 * 1024;

/** Maximum number of entries inside a backup archive: 10,000 entries. */
export const MAX_BACKUP_ENTRY_COUNT = 10_000;

/** Maximum number of recovery checkpoints retained before pruning: 10. */
export const MAX_CHECKPOINT_COUNT = 10;

// --- Search Bounds ---
/** Maximum search query length in Unicode characters: 1,000 characters. */
export const MAX_SEARCH_QUERY_LENGTH = 1000;

/** Default search result limit: 50. */
export const DEFAULT_SEARCH_LIMIT = 50;

/** Maximum search result limit: 100. */
export const MAX_SEARCH_LIMIT = 100;

// --- Pagination & Bulk Operations Bounds ---
/** Default pagination page size for list queries: 50. */
export const DEFAULT_PAGE_SIZE = 50;

/** Maximum allowed page size for paginated queries: 200. */
export const MAX_PAGE_SIZE = 200;

/** Maximum items permitted in a single bulk operation: 500. */
export const MAX_BULK_OPERATION_ITEMS = 500;

/**
 * Validates note title length on the frontend.
 * Returns null if valid, or an actionable error message if invalid.
 */
export function validateNoteTitle(title?: string | null): string | null {
  if (!title) return null;
  const count = Array.from(title).length;
  if (count > MAX_NOTE_TITLE_CHARS) {
    return `Note title exceeds the maximum allowed limit of ${MAX_NOTE_TITLE_CHARS} characters (received ${count}).`;
  }
  return null;
}

/**
 * Validates note content byte size on the frontend.
 * Returns null if valid, or an actionable error message if invalid.
 */
export function validateNoteContent(content?: string | null): string | null {
  if (!content) return null;
  const byteLength = new TextEncoder().encode(content).length;
  if (byteLength > MAX_NOTE_CONTENT_BYTES) {
    const maxMb = MAX_NOTE_CONTENT_BYTES / (1024 * 1024);
    const actualMb = (byteLength / (1024 * 1024)).toFixed(2);
    return `Note content size (${actualMb} MB) exceeds the maximum allowed limit of ${maxMb} MB.`;
  }
  return null;
}
