import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { storageService } from "../services/storage";
import {
  addRecent,
  parseRecent,
  removeRecent,
  syncRecentTitles,
  RECENT_SETTING_KEY,
  type RecentInput,
  type RecentNote,
} from "../utils/recentNotes";

const CACHE_KEY = "personal_notepad_recent_notes";

function readCache(): RecentNote[] {
  try {
    return parseRecent(localStorage.getItem(CACHE_KEY));
  } catch {
    return [];
  }
}

export interface UseRecentNotesReturn {
  recent: RecentNote[];
  record: (entry: RecentInput) => void;
  remove: (id: string) => void;
  clear: () => void;
  syncTitles: (notes: ReadonlyArray<{ id: string; title: string }>) => void;
}

/** Recently opened notes/files, remembered across restarts (local cache + SQLite settings). */
export function useRecentNotes(): UseRecentNotesReturn {
  const [recent, setRecent] = useState<RecentNote[]>(readCache);
  const listRef = useRef(recent);
  const touchedRef = useRef(false);

  const commit = useCallback((next: RecentNote[]) => {
    if (next === listRef.current) return;
    listRef.current = next;
    setRecent(next);
    const json = JSON.stringify(next);
    try {
      localStorage.setItem(CACHE_KEY, json);
    } catch {
      // Ignore cache write errors
    }
    storageService.settings.set(RECENT_SETTING_KEY, json).catch(() => {});
  }, []);

  // Hydrate from SQLite (the cache may be empty after a profile reset)
  useEffect(() => {
    let isMounted = true;
    storageService.settings
      .get(RECENT_SETTING_KEY)
      .then((stored) => {
        if (!isMounted || touchedRef.current || stored === null) return;
        const items = parseRecent(stored);
        listRef.current = items;
        setRecent(items);
      })
      .catch(() => {});
    return () => {
      isMounted = false;
    };
  }, []);

  const record = useCallback(
    (entry: RecentInput) => {
      touchedRef.current = true;
      commit(addRecent(listRef.current, entry, Date.now()));
    },
    [commit],
  );
  const remove = useCallback(
    (id: string) => {
      touchedRef.current = true;
      commit(removeRecent(listRef.current, id));
    },
    [commit],
  );
  const clear = useCallback(() => {
    touchedRef.current = true;
    commit([]);
  }, [commit]);
  const syncTitles = useCallback(
    (notes: ReadonlyArray<{ id: string; title: string }>) => {
      const next = syncRecentTitles(listRef.current, notes);
      if (next !== listRef.current) commit(next);
    },
    [commit],
  );

  return useMemo(
    () => ({ recent, record, remove, clear, syncTitles }),
    [recent, record, remove, clear, syncTitles],
  );
}

export default useRecentNotes;
