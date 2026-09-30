/**
 * Search input normalization utility (Phase 6, Task 8).
 * 
 * Rules:
 * - Trims leading and trailing whitespace.
 * - Preserves meaningful Unicode, accents, non-Latin scripts (Hindi, Japanese, Cyrillic, etc.).
 * - Does not perform destructive lowercasing on display strings.
 * - Does not aggressively strip punctuation that may be part of search tokens.
 */
export function normalizeSearchQuery(query: string): string {
  if (!query) return "";
  return query.trim();
}

/**
 * Checks if normalized search query is active and searchable.
 */
export function isValidSearchQuery(query: string): boolean {
  return normalizeSearchQuery(query).length > 0;
}

import type { SearchResult } from "./types";
import type { NoteListItem } from "../notes/types";

/**
 * Transforms a SearchResult domain object into a NoteListItem for unified display in NotesList (Task 14).
 */
export function searchResultToNoteListItem(
  result: SearchResult,
  notebookPath?: string,
  tags?: string[]
): NoteListItem {
  let updatedAt = "Recently";
  if (result.modifiedAt) {
    try {
      const date = new Date(result.modifiedAt);
      if (!isNaN(date.getTime())) {
        updatedAt = date.toLocaleDateString(undefined, {
          month: "short",
          day: "numeric",
        });
      }
    } catch {
      updatedAt = "Recently";
    }
  }

  return {
    id: result.noteId,
    title: result.title || "Untitled Note",
    preview: result.snippet || "No snippet available",
    updatedAt,
    isFavorite: result.favorite,
    notebookId: result.notebookId ?? undefined,
    notebookPath: notebookPath && notebookPath !== "Unfiled" ? notebookPath : undefined,
    tags,
  };
}

/**
 * Generates user-facing result count text with proper singular/plural grammar (Task 30).
 * Examples: "1 result", "12 results", "0 results".
 */
export function formatResultCount(count: number): string {
  return `${count} ${count === 1 ? "result" : "results"}`;
}

/**
 * Sanitizes errors from the search / database layer (Task 17).
 * Ensures raw database syntax/engine errors (e.g. SQLite error: near "...")
 * are not leaked to normal users, while logging diagnostics to the console.
 */
export function sanitizeSearchError(err: unknown): { title: string; message: string } {
  // Always log raw error to development console
  if (typeof console !== "undefined" && console.error) {
    console.error("[Search] SQLite/backend search operation failed:", err);
  }

  return {
    title: "Search failed",
    message: "Unable to search notes right now. Please try again.",
  };
}
