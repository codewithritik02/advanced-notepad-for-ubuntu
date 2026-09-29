import { useState, useEffect, useCallback } from "react";
import { ThemeMode } from "../types";
import { storageService } from "../services/storage";

const THEME_STORAGE_KEY = "personal_notepad_theme";

export interface UseThemeReturn {
  theme: ThemeMode;
  effectiveTheme: "light" | "dark";
  setTheme: (theme: ThemeMode) => void;
  toggleTheme: () => void;
}

export function useTheme(): UseThemeReturn {
  // Read initial theme preference from cache or default to "system"
  const [theme, setThemeState] = useState<ThemeMode>(() => {
    try {
      const stored = localStorage.getItem(THEME_STORAGE_KEY);
      if (stored === "light" || stored === "dark" || stored === "system") {
        return stored;
      }
    } catch {
      // Fallback silently if storage is inaccessible
    }
    return "system";
  });

  // Hydrate theme setting from SQLite storage on application mount
  useEffect(() => {
    let isMounted = true;
    storageService.settings
      .get("theme")
      .then((sqliteTheme) => {
        if (
          isMounted &&
          sqliteTheme &&
          (sqliteTheme === "light" ||
            sqliteTheme === "dark" ||
            sqliteTheme === "system")
        ) {
          setThemeState(sqliteTheme);
          try {
            localStorage.setItem(THEME_STORAGE_KEY, sqliteTheme);
          } catch {
            // Ignore cache errors
          }
        }
      })
      .catch(() => {
        // Gracefully retain cached/default theme if storage is still initializing
      });

    return () => {
      isMounted = false;
    };
  }, []);

  // Track system preference
  const [systemIsDark, setSystemIsDark] = useState<boolean>(() => {
    if (typeof window !== "undefined" && window.matchMedia) {
      return window.matchMedia("(prefers-color-scheme: dark)").matches;
    }
    return false;
  });

  // Determine the effective visual theme
  const effectiveTheme: "light" | "dark" =
    theme === "system" ? (systemIsDark ? "dark" : "light") : theme;

  // Listen to OS-level theme preference changes
  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;

    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = (e: MediaQueryListEvent) => {
      setSystemIsDark(e.matches);
    };

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, []);

  // Update DOM data-theme attribute whenever theme or system preference changes
  useEffect(() => {
    const root = document.documentElement;
    root.setAttribute("data-theme", effectiveTheme);
    root.setAttribute("data-theme-preference", theme);
  }, [effectiveTheme, theme]);

  // Set theme and persist to SQLite (and synchronous local cache for zero-flicker reload)
  const setTheme = useCallback(
    (newTheme: ThemeMode | ((prev: ThemeMode) => ThemeMode)) => {
      setThemeState((prev) => {
        const resolved =
          typeof newTheme === "function" ? newTheme(prev) : newTheme;
        try {
          localStorage.setItem(THEME_STORAGE_KEY, resolved);
        } catch {
          // Ignore cache write errors
        }
        storageService.settings.set("theme", resolved).catch(() => {});
        return resolved;
      });
    },
    []
  );

  // Cycle through: light -> dark -> system -> light
  const toggleTheme = useCallback(() => {
    setTheme((prev: ThemeMode) => {
      if (prev === "light") return "dark";
      if (prev === "dark") return "system";
      return "light";
    });
  }, [setTheme]);

  return {
    theme,
    effectiveTheme,
    setTheme,
    toggleTheme,
  };
}

export default useTheme;
