/**
 * Pure helpers for user-adjustable display settings (interface zoom and editor text size).
 *
 * Zoom follows the browser convention (discrete levels, 100% default, Ctrl +/-/0).
 * Editor text size is a plain pixel value adjusted in 1px steps.
 */

export const ZOOM_LEVELS: readonly number[] = [
  50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300,
];
export const DEFAULT_ZOOM_PERCENT = 100;
export const MIN_ZOOM_PERCENT = ZOOM_LEVELS[0];
export const MAX_ZOOM_PERCENT = ZOOM_LEVELS[ZOOM_LEVELS.length - 1];

export const DEFAULT_EDITOR_FONT_SIZE_PX = 14;
export const MIN_EDITOR_FONT_SIZE_PX = 10;
export const MAX_EDITOR_FONT_SIZE_PX = 32;

export const ZOOM_SETTING_KEY = "ui_zoom_percent";
export const EDITOR_FONT_SIZE_SETTING_KEY = "editor_font_size_px";
export const MINIMAP_SETTING_KEY = "show_minimap";
export const DEFAULT_SHOW_MINIMAP = true;

/** Snaps any stored/typed value to the nearest supported zoom level. */
export function normalizeZoomPercent(value: unknown): number {
  const n = typeof value === "string" ? Number(value) : (value as number);
  if (typeof n !== "number" || !Number.isFinite(n)) return DEFAULT_ZOOM_PERCENT;
  let best = ZOOM_LEVELS[0];
  for (const level of ZOOM_LEVELS) {
    if (Math.abs(level - n) < Math.abs(best - n)) best = level;
  }
  return best;
}

/** Next zoom level in the given direction, clamped at the ends. */
export function stepZoomPercent(current: number, direction: 1 | -1): number {
  const snapped = normalizeZoomPercent(current);
  const index = ZOOM_LEVELS.indexOf(snapped);
  const next = Math.min(ZOOM_LEVELS.length - 1, Math.max(0, index + direction));
  return ZOOM_LEVELS[next];
}

/** Rounds to a whole pixel and clamps to the supported range. */
export function normalizeEditorFontSize(value: unknown): number {
  const n = typeof value === "string" ? Number(value) : (value as number);
  if (typeof n !== "number" || !Number.isFinite(n)) {
    return DEFAULT_EDITOR_FONT_SIZE_PX;
  }
  return Math.min(
    MAX_EDITOR_FONT_SIZE_PX,
    Math.max(MIN_EDITOR_FONT_SIZE_PX, Math.round(n)),
  );
}

/** Stored as the strings "true"/"false"; anything unreadable falls back to the default (on). */
export function normalizeShowMinimap(value: unknown): boolean {
  if (typeof value === "boolean") return value;
  if (value === "false" || value === "0") return false;
  if (value === "true" || value === "1") return true;
  return DEFAULT_SHOW_MINIMAP;
}

export function stepEditorFontSize(current: number, direction: 1 | -1): number {
  return normalizeEditorFontSize(current + direction);
}
