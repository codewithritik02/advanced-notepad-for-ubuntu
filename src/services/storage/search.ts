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

    if (typeof queryOrOptions === "string") {
      queryText = queryOrOptions;
    } else {
      queryText = queryOrOptions.query;
      searchLimit = queryOrOptions.limit ?? limit;
    }

    const trimmed = queryText.trim();
    if (!trimmed) {
      return [];
    }

    return await invoke<SearchResult[]>("search_notes", {
      query: trimmed,
      limit: searchLimit ?? null,
    });
  },
};

export default searchStorage;
