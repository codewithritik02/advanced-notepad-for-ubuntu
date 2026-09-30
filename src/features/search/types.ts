import { SearchQuery, SearchResult } from "../../services/storage/types";

export type { SearchQuery, SearchResult };

/**
 * Explicit tagged union state model for search operations (Phase 6, Task 21).
 * Avoids boolean soup (e.g. isLoading, hasError, isSearching).
 */
export type SearchState =
  | { status: "idle" }
  | { status: "searching"; query: string }
  | { status: "success"; query: string; results: SearchResult[] }
  | { status: "error"; query: string; message: string };
