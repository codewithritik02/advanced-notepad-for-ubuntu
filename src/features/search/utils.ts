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

import React from "react";
import type { SearchResult, SearchState } from "./types";
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
 * Examples: "1 result", "12 results", "0 results", "100+ results".
 */
export function formatResultCount(count: number, hasMore?: boolean): string {
  if (count <= 0) return "0 results";
  if (count === 1) return hasMore ? "1+ results" : "1 result";
  if (hasMore) return `${count}+ results`;
  return `${count} results`;
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

/**
 * Type guard for idle search state (Task 21).
 */
export function isSearchIdle(state: SearchState): state is { status: "idle" } {
  return state.status === "idle";
}

/**
 * Type guard for in-progress search state (Task 21).
 */
export function isSearchSearching(
  state: SearchState
): state is { status: "searching"; query: string } {
  return state.status === "searching";
}

/**
 * Type guard for successful search state (Task 21).
 */
export function isSearchSuccess(
  state: SearchState
): state is { status: "success"; query: string; results: SearchResult[] } {
  return state.status === "success";
}

/**
 * Type guard for error search state (Task 21).
 */
export function isSearchError(
  state: SearchState
): state is { status: "error"; query: string; message: string } {
  return state.status === "error";
}

/**
 * Safely highlights matching search query terms within text using pure React elements (Task 26 & 27).
 *
 * Guarantees:
 * - NEVER uses dangerouslySetInnerHTML or raw HTML injection.
 * - All text fragments are rendered as standard React text nodes with automatic escaping.
 * - Safely escapes regex special characters in search queries.
 * - Supports Unicode, multilingual text, and accented characters.
 */
export function highlightText(text: string, query?: string): React.ReactNode {
  if (!text) return "";
  if (!query) return text;

  const trimmedQuery = query.trim();
  if (!trimmedQuery) return text;

  // Split query into distinct non-empty tokens
  const tokens = trimmedQuery
    .split(/\s+/)
    .map((t) => t.trim())
    .filter((t) => t.length > 0);

  if (tokens.length === 0) return text;

  // Escape special regex characters
  const escapedTokens = tokens
    .map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join("|");

  try {
    const regex = new RegExp(`(${escapedTokens})`, "gi");
    const parts = text.split(regex);

    if (parts.length <= 1) return text;

    return parts.map((part, index) => {
      const isMatch = tokens.some((t) => t.toLowerCase() === part.toLowerCase());
      if (isMatch) {
        return React.createElement(
          "mark",
          { key: index, className: "search-highlight" },
          part
        );
      }
      return part;
    });
  } catch {
    return text;
  }
}
