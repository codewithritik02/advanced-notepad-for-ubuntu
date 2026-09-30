import { useState, useRef, useCallback, useEffect } from "react";
import { searchStorage } from "../../services/storage/search";
import { SearchResult, SearchState } from "./types";
import { sanitizeSearchError } from "./utils";

export interface UseSearchOptions {
  debounceMs?: number;
  limit?: number;
}

export interface UseSearchResult {
  query: string;
  setQuery: (query: string) => void;
  searchState: SearchState;
  results: SearchResult[];
  isSearchActive: boolean;
  clearSearch: () => void;
  searchNow: (overrideQuery?: string) => Promise<void>;
  cancelPendingDebounce: () => void;
}

/**
 * Custom React hook for robust, offline search state management (Phase 6).
 * Enforces:
 * - Empty query behavior: Empty or whitespace query immediately exits search mode (Task 7)
 * - Safe input trimming & normalization (Task 8)
 * - Debounced execution (Task 18)
 * - Immediate Enter execution (Task 19)
 * - Stale request cancellation via monotonically incrementing request IDs (Task 20)
 * - Explicit tagged union state machine (Task 21)
 */
export function useSearch(options: UseSearchOptions = {}): UseSearchResult {
  const debounceMs = Math.max(0, options.debounceMs ?? 200);
  const limit = Math.max(1, Math.min(options.limit ?? 50, 100));

  const [query, setQueryState] = useState<string>("");
  const [searchState, setSearchState] = useState<SearchState>({ status: "idle" });

  const requestIdRef = useRef<number>(0);
  const debounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const isSearchActive = query.trim().length > 0;

  const executeSearch = useCallback(
    async (text: string) => {
      const trimmed = text.trim();

      // Task 7: Empty or whitespace query must not run an expensive database search
      if (!trimmed) {
        requestIdRef.current += 1; // Invalidate any pending in-flight requests
        setSearchState({ status: "idle" });
        return;
      }

      const currentReqId = ++requestIdRef.current;
      setSearchState({ status: "searching", query: trimmed });

      try {
        const searchResults = await searchStorage.search(trimmed, limit);

        // Task 20: Stale request protection
        if (currentReqId === requestIdRef.current) {
          setSearchState({
            status: "success",
            query: trimmed,
            results: searchResults,
          });
        }
      } catch (err: unknown) {
        if (currentReqId === requestIdRef.current) {
          const { message } = sanitizeSearchError(err);
          setSearchState({
            status: "error",
            query: trimmed,
            message,
          });
        }
      }
    },
    [limit]
  );

  const setQuery = useCallback(
    (newQuery: string) => {
      setQueryState(newQuery);

      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }

      const trimmed = newQuery.trim();
      // Task 7: If query is empty or whitespace, exit search mode immediately without waiting for debounce
      if (!trimmed) {
        requestIdRef.current += 1;
        setSearchState({ status: "idle" });
        return;
      }

      // Debounce non-empty queries (Task 18)
      debounceTimerRef.current = setTimeout(() => {
        executeSearch(newQuery);
      }, debounceMs);
    },
    [debounceMs, executeSearch]
  );

  const searchNow = useCallback(
    async (overrideQuery?: string) => {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }
      const targetQuery = overrideQuery !== undefined ? overrideQuery : query;
      await executeSearch(targetQuery);
    },
    [query, executeSearch]
  );

  const cancelPendingDebounce = useCallback(() => {
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
  }, []);

  const clearSearch = useCallback(() => {
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
    requestIdRef.current += 1;
    setQueryState("");
    setSearchState({ status: "idle" });
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      requestIdRef.current += 1; // Invalidate any pending in-flight requests on unmount
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
    };
  }, []);

  const results = searchState.status === "success" ? searchState.results : [];

  return {
    query,
    setQuery,
    searchState,
    results,
    isSearchActive,
    clearSearch,
    searchNow,
    cancelPendingDebounce,
  };
}

export default useSearch;
