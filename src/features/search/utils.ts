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
import type { NoteLocation } from "../notebooks/types";

/**
 * Transforms a SearchResult domain object into a NoteListItem for unified display in NotesList (Task 14).
 */
export function searchResultToNoteListItem(
  result: SearchResult,
  notebookPath?: string,
  tags?: string[]
): NoteListItem {
  let updatedAt = "Recently";
  const modDate = result.modifiedAt || (result as unknown as { modified_at?: string }).modified_at;
  if (modDate) {
    try {
      const date = new Date(modDate);
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

  const raw = result as unknown as { note_id?: string; notebook_id?: string | null };
  return {
    id: result.noteId || raw.note_id || "",
    title: result.title || "Untitled Note",
    preview: result.snippet || "No snippet available",
    updatedAt,
    isFavorite: result.favorite,
    notebookId: (result.notebookId ?? raw.notebook_id) ?? undefined,
    notebookPath: notebookPath && notebookPath !== "Unfiled" ? notebookPath : undefined,
    tags,
  };
}

/**
 * Filters search results when Favorites navigation is active (Task 39).
 * Preserves deterministic result ordering while scoping to favorited notes.
 */
export function filterSearchResultsByFavorite(
  results: SearchResult[],
  isFavoritesActive: boolean,
  notesLookup?: Map<string, { is_favorite: boolean }>
): SearchResult[] {
  if (!isFavoritesActive) return results;
  return results.filter((result) => {
    if (notesLookup && notesLookup.has(result.noteId)) {
      return notesLookup.get(result.noteId)!.is_favorite;
    }
    return result.favorite;
  });
}

export interface SearchScopeOptions {
  /**
   * The current navigation location (Task 42).
   * Carries the full discriminated navigation context so scoping
   * does not require separate activeNavId + selectedTagId + selectedNotebookId
   * properties that can fall out of sync.
   */
  location: NoteLocation;
  tags?: Array<{ id: string; name: string }>;
  noteTagsMap?: Record<string, string[]>;
  notesLookup?: Map<string, { is_favorite: boolean }>;
}

/**
 * Scopes search results based on the active NoteLocation (Task 42).
 *
 * - { type: "favorites" }  → Task 39: filter to favorited notes only.
 * - { type: "tag" }        → Task 40: filter to notes that carry the selected tag.
 * - { type: "notebook" }   → Task 41: filter to notes that belong to the selected notebook.
 * - { type: "all" | "unfiled" | "trash" } → no post-filter (SQL handles these).
 *
 * Search state (searchQuery) is intentionally kept separate from NoteLocation.
 * Do NOT merge them into a combined location variant. Use this composition layer instead.
 */
export function scopeSearchResults(
  results: SearchResult[],
  options: SearchScopeOptions
): SearchResult[] {
  const { location, tags, noteTagsMap, notesLookup } = options;

  if (location.type === "favorites") {
    return filterSearchResultsByFavorite(results, true, notesLookup);
  }

  if (location.type === "tag" && tags && noteTagsMap) {
    const selectedTag = tags.find((t) => t.id === location.tagId);
    if (!selectedTag) return results;
    const targetTagName = selectedTag.name.toLowerCase();
    return results.filter((result) => {
      const noteTags = noteTagsMap[result.noteId];
      if (!noteTags || noteTags.length === 0) return false;
      return noteTags.some((t) => t.toLowerCase() === targetTagName);
    });
  }

  // Task 41: Notebook — scope results to the currently selected notebook.
  if (location.type === "notebook") {
    return results.filter((result) => result.notebookId === location.notebookId);
  }

  return results;
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
