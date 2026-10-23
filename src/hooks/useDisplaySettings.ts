import { useState, useEffect, useCallback, useRef } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { storageService } from "../services/storage";
import {
  DEFAULT_ZOOM_PERCENT,
  DEFAULT_EDITOR_FONT_SIZE_PX,
  ZOOM_SETTING_KEY,
  EDITOR_FONT_SIZE_SETTING_KEY,
  MINIMAP_SETTING_KEY,
  DEFAULT_SHOW_MINIMAP,
  SHOW_HIDDEN_NOTES_SETTING_KEY,
  DEFAULT_SHOW_HIDDEN_NOTES,
  normalizeShowHiddenNotes,
  normalizeShowMinimap,
  normalizeZoomPercent,
  stepZoomPercent,
  normalizeEditorFontSize,
  stepEditorFontSize,
} from "../features/settings/utils/displaySettings";

const ZOOM_CACHE_KEY = "personal_notepad_zoom_percent";
const FONT_CACHE_KEY = "personal_notepad_editor_font_size";
const MINIMAP_CACHE_KEY = "personal_notepad_show_minimap";
const HIDDEN_NOTES_CACHE_KEY = "personal_notepad_show_hidden_notes";
const WHEEL_ZOOM_INTERVAL_MS = 120;

export interface UseDisplaySettingsReturn {
  zoomPercent: number;
  editorFontSize: number;
  showMinimap: boolean;
  setShowMinimap: (show: boolean) => void;
  showHiddenNotes: boolean;
  setShowHiddenNotes: (show: boolean | ((previous: boolean) => boolean)) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  resetZoom: () => void;
  setEditorFontSize: (px: number) => void;
  increaseEditorFontSize: () => void;
  decreaseEditorFontSize: () => void;
  resetEditorFontSize: () => void;
}

function readCache(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeCache(key: string, value: number): void {
  try {
    localStorage.setItem(key, String(value));
  } catch {
    // Ignore cache write errors
  }
}

/** Applies zoom through the native webview; falls back to CSS zoom outside Tauri. */
async function applyZoom(percent: number): Promise<void> {
  const factor = percent / 100;
  try {
    await getCurrentWebview().setZoom(factor);
    document.documentElement.style.removeProperty("zoom");
  } catch {
    document.documentElement.style.setProperty("zoom", String(factor));
  }
}

/**
 * User display preferences: interface zoom (Ctrl +/-/0, Ctrl+wheel) and editor
 * text size in pixels. Cached in localStorage for flicker-free startup and
 * persisted to SQLite settings, mirroring `useTheme`.
 */
export function useDisplaySettings(): UseDisplaySettingsReturn {
  const [zoomPercent, setZoomState] = useState<number>(() => {
    const cached = readCache(ZOOM_CACHE_KEY);
    return cached === null ? DEFAULT_ZOOM_PERCENT : normalizeZoomPercent(cached);
  });
  const [editorFontSize, setFontState] = useState<number>(() => {
    const cached = readCache(FONT_CACHE_KEY);
    return cached === null
      ? DEFAULT_EDITOR_FONT_SIZE_PX
      : normalizeEditorFontSize(cached);
  });

  const [showMinimap, setShowMinimapState] = useState<boolean>(() => {
    const cached = readCache(MINIMAP_CACHE_KEY);
    return cached === null ? DEFAULT_SHOW_MINIMAP : normalizeShowMinimap(cached);
  });
  const minimapRef = useRef(showMinimap);
  const minimapTouchedRef = useRef(false);

  const [showHiddenNotes, setShowHiddenNotesState] = useState<boolean>(() => {
    const cached = readCache(HIDDEN_NOTES_CACHE_KEY);
    return cached === null
      ? DEFAULT_SHOW_HIDDEN_NOTES
      : normalizeShowHiddenNotes(cached);
  });
  const hiddenNotesRef = useRef(showHiddenNotes);
  const hiddenNotesTouchedRef = useRef(false);

  // Latest values, so updates stay pure and persistence runs exactly once per change
  const zoomRef = useRef(zoomPercent);
  const fontRef = useRef(editorFontSize);
  // Set once the user changes a value, so a slow SQLite hydrate cannot overwrite it
  const zoomTouchedRef = useRef(false);
  const fontTouchedRef = useRef(false);

  // Hydrate from SQLite on mount
  useEffect(() => {
    let isMounted = true;
    storageService.settings
      .get(ZOOM_SETTING_KEY)
      .then((v) => {
        if (isMounted && v !== null && !zoomTouchedRef.current) {
          const zoom = normalizeZoomPercent(v);
          zoomRef.current = zoom;
          setZoomState(zoom);
          writeCache(ZOOM_CACHE_KEY, zoom);
        }
      })
      .catch(() => {});
    storageService.settings
      .get(EDITOR_FONT_SIZE_SETTING_KEY)
      .then((v) => {
        if (isMounted && v !== null && !fontTouchedRef.current) {
          const size = normalizeEditorFontSize(v);
          fontRef.current = size;
          setFontState(size);
          writeCache(FONT_CACHE_KEY, size);
        }
      })
      .catch(() => {});
    storageService.settings
      .get(MINIMAP_SETTING_KEY)
      .then((v) => {
        if (isMounted && v !== null && !minimapTouchedRef.current) {
          const show = normalizeShowMinimap(v);
          minimapRef.current = show;
          setShowMinimapState(show);
          try {
            localStorage.setItem(MINIMAP_CACHE_KEY, String(show));
          } catch {
            // Ignore cache write errors
          }
        }
      })
      .catch(() => {});
    storageService.settings
      .get(SHOW_HIDDEN_NOTES_SETTING_KEY)
      .then((v) => {
        if (isMounted && v !== null && !hiddenNotesTouchedRef.current) {
          const show = normalizeShowHiddenNotes(v);
          hiddenNotesRef.current = show;
          setShowHiddenNotesState(show);
          try {
            localStorage.setItem(HIDDEN_NOTES_CACHE_KEY, String(show));
          } catch {
            // Ignore cache write errors
          }
        }
      })
      .catch(() => {});
    return () => {
      isMounted = false;
    };
  }, []);

  useEffect(() => {
    void applyZoom(zoomPercent);
  }, [zoomPercent]);

  useEffect(() => {
    document.documentElement.style.setProperty(
      "--editor-font-size",
      `${editorFontSize}px`,
    );
  }, [editorFontSize]);

  const commitZoom = useCallback((next: number) => {
    if (next === zoomRef.current) return;
    zoomTouchedRef.current = true;
    zoomRef.current = next;
    setZoomState(next);
    writeCache(ZOOM_CACHE_KEY, next);
    storageService.settings
      .set(ZOOM_SETTING_KEY, String(next))
      .catch(() => {});
  }, []);

  const commitFont = useCallback((next: number) => {
    if (next === fontRef.current) return;
    fontTouchedRef.current = true;
    fontRef.current = next;
    setFontState(next);
    writeCache(FONT_CACHE_KEY, next);
    storageService.settings
      .set(EDITOR_FONT_SIZE_SETTING_KEY, String(next))
      .catch(() => {});
  }, []);

  const setShowMinimap = useCallback((show: boolean) => {
    if (show === minimapRef.current) return;
    minimapTouchedRef.current = true;
    minimapRef.current = show;
    setShowMinimapState(show);
    try {
      localStorage.setItem(MINIMAP_CACHE_KEY, String(show));
    } catch {
      // Ignore cache write errors
    }
    storageService.settings.set(MINIMAP_SETTING_KEY, String(show)).catch(() => {});
  }, []);

  const setShowHiddenNotes = useCallback((value: boolean | ((previous: boolean) => boolean)) => {
    const show = typeof value === "function" ? value(hiddenNotesRef.current) : value;
    if (show === hiddenNotesRef.current) return;
    hiddenNotesTouchedRef.current = true;
    hiddenNotesRef.current = show;
    setShowHiddenNotesState(show);
    try {
      localStorage.setItem(HIDDEN_NOTES_CACHE_KEY, String(show));
    } catch {
      // Ignore cache write errors
    }
    storageService.settings.set(SHOW_HIDDEN_NOTES_SETTING_KEY, String(show)).catch(() => {});
  }, []);

  const zoomIn = useCallback(
    () => commitZoom(stepZoomPercent(zoomRef.current, 1)),
    [commitZoom],
  );
  const zoomOut = useCallback(
    () => commitZoom(stepZoomPercent(zoomRef.current, -1)),
    [commitZoom],
  );
  const resetZoom = useCallback(
    () => commitZoom(DEFAULT_ZOOM_PERCENT),
    [commitZoom],
  );
  const setEditorFontSize = useCallback(
    (px: number) => commitFont(normalizeEditorFontSize(px)),
    [commitFont],
  );
  const increaseEditorFontSize = useCallback(
    () => commitFont(stepEditorFontSize(fontRef.current, 1)),
    [commitFont],
  );
  const decreaseEditorFontSize = useCallback(
    () => commitFont(stepEditorFontSize(fontRef.current, -1)),
    [commitFont],
  );
  const resetEditorFontSize = useCallback(
    () => commitFont(DEFAULT_EDITOR_FONT_SIZE_PX),
    [commitFont],
  );

  // Ctrl/Cmd + mouse wheel zooms the interface, like a browser. Throttled so a
  // trackpad pinch or fast wheel spin moves one level at a time, not many.
  useEffect(() => {
    let lastWheelZoom = 0;
    const onWheel = (event: WheelEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.deltaY === 0) return;
      event.preventDefault();
      const now = Date.now();
      if (now - lastWheelZoom < WHEEL_ZOOM_INTERVAL_MS) return;
      lastWheelZoom = now;
      if (event.deltaY < 0) zoomIn();
      else zoomOut();
    };
    window.addEventListener("wheel", onWheel, { passive: false });
    return () => window.removeEventListener("wheel", onWheel);
  }, [zoomIn, zoomOut]);

  return {
    zoomPercent,
    editorFontSize,
    showMinimap,
    setShowMinimap,
    showHiddenNotes,
    setShowHiddenNotes,
    zoomIn,
    zoomOut,
    resetZoom,
    setEditorFontSize,
    increaseEditorFontSize,
    decreaseEditorFontSize,
    resetEditorFontSize,
  };
}

export default useDisplaySettings;
