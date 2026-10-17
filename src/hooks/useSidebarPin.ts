import { useState, useEffect, useCallback, useRef } from "react";
import { storageService } from "../services/storage";

const CACHE_KEY = "personal_notepad_sidebar_pinned";
export const SIDEBAR_PIN_SETTING_KEY = "sidebar_pinned";

/** The sidebar starts collapsed to an icon rail; pinning keeps it open. */
export const DEFAULT_SIDEBAR_PINNED = false;

export function normalizeSidebarPinned(value: unknown): boolean {
  if (typeof value === "boolean") return value;
  if (value === "true" || value === "1") return true;
  if (value === "false" || value === "0") return false;
  return DEFAULT_SIDEBAR_PINNED;
}

export interface UseSidebarPinReturn {
  isPinned: boolean;
  toggle: () => void;
}

/** Remembers whether the sidebar is pinned open (local cache for a flicker-free start + SQLite). */
export function useSidebarPin(): UseSidebarPinReturn {
  const [isPinned, setPinned] = useState<boolean>(() => {
    try {
      const cached = localStorage.getItem(CACHE_KEY);
      return cached === null ? DEFAULT_SIDEBAR_PINNED : normalizeSidebarPinned(cached);
    } catch {
      return DEFAULT_SIDEBAR_PINNED;
    }
  });
  const pinnedRef = useRef(isPinned);
  const touchedRef = useRef(false);

  useEffect(() => {
    let isMounted = true;
    storageService.settings
      .get(SIDEBAR_PIN_SETTING_KEY)
      .then((stored) => {
        if (!isMounted || stored === null || touchedRef.current) return;
        const value = normalizeSidebarPinned(stored);
        pinnedRef.current = value;
        setPinned(value);
        try {
          localStorage.setItem(CACHE_KEY, String(value));
        } catch {
          // Ignore cache write errors
        }
      })
      .catch(() => {});
    return () => {
      isMounted = false;
    };
  }, []);

  const toggle = useCallback(() => {
    touchedRef.current = true;
    const next = !pinnedRef.current;
    pinnedRef.current = next;
    setPinned(next);
    try {
      localStorage.setItem(CACHE_KEY, String(next));
    } catch {
      // Ignore cache write errors
    }
    storageService.settings.set(SIDEBAR_PIN_SETTING_KEY, String(next)).catch(() => {});
  }, []);

  return { isPinned, toggle };
}

export default useSidebarPin;
