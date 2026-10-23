import { invoke } from "@tauri-apps/api/core";
import { SearchQuery, SearchResult } from "./types";

export const searchStorage = {
  /**
   * Searches active, non-deleted notes by title and content (Phase 6, Task 6, 18).
   * Executes against SQLite backend without client-side scanning.
   */
  async search(queryOrOptions: string | SearchQuery, limit?: number): Promise<SearchResult[]> {
    let queryText = "";
    let searchLimit = limit;
    let includeHidden = false;

    if (typeof queryOrOptions === "string") {
      queryText = queryOrOptions;
    } else {
      queryText = queryOrOptions.query;
      searchLimit = queryOrOptions.limit ?? limit;
      includeHidden = queryOrOptions.includeHidden ?? false;
    }

    const trimmed = queryText.trim();
    if (!trimmed) {
      return [];
    }

    return await invoke<SearchResult[]>("search_notes", {
      query: trimmed,
      limit: searchLimit ?? null,
      includeHidden,
    });
  },

  /**
   * Safely rebuilds the full-text search index (Phase 13, Task 10).
   */
  async rebuildIndex(): Promise<void> {
    return await invoke<void>("rebuild_search_index");
  },

  /**
   * Diagnoses search index health, triggers, and consistency (Phase 13, Task 10).
   */
  async diagnoseIndex(): Promise<import("./types").SearchIndexDiagnostic> {
    return await invoke<import("./types").SearchIndexDiagnostic>("diagnose_search_index");
  },
};

export default searchStorage;
