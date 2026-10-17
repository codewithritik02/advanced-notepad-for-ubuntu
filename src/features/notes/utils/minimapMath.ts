/**
 * Geometry for the note minimap (the small overview strip next to the editor, like VS Code's).
 * Everything here is pure so it can be unit-tested without a DOM.
 */

/** A page never gets drawn larger than this fraction of its real size. */
export const MINIMAP_MAX_SCALE = 0.22;
export const MINIMAP_MIN_SLIDER_PX = 16;

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Uniform vertical scale that fits the whole document into the strip (never above the cap). */
export function minimapScale(scrollHeight: number, mapHeight: number): number {
  if (scrollHeight <= 0 || mapHeight <= 0) return MINIMAP_MAX_SCALE;
  return Math.min(MINIMAP_MAX_SCALE, mapHeight / scrollHeight);
}

/** Position and height of the draggable "you are here" window. */
export function sliderRect(
  scrollTop: number,
  clientHeight: number,
  scale: number,
): { top: number; height: number } {
  const height = Math.max(MINIMAP_MIN_SLIDER_PX, clientHeight * scale);
  return { top: Math.max(0, scrollTop * scale), height };
}

/**
 * Scroll offset for a pointer position in the strip.
 * Without `grabOffset` the clicked point becomes the centre of the viewport (a click jumps there);
 * with it (dragging the window) the window keeps the same grip point under the pointer.
 */
export function pointerToScrollTop(options: {
  pointerY: number;
  clientHeight: number;
  scrollHeight: number;
  scale: number;
  grabOffset?: number;
}): number {
  const { pointerY, clientHeight, scrollHeight, scale, grabOffset } = options;
  const maxTop = Math.max(0, scrollHeight - clientHeight);
  if (scale <= 0) return 0;
  const top =
    grabOffset === undefined
      ? pointerY / scale - clientHeight / 2
      : (pointerY - grabOffset) / scale;
  return clamp(top, 0, maxTop);
}

/** Number of visual rows a logical line occupies when wrapped to `availableWidth`. */
export function estimateRows(lineWidths: number[], availableWidth: number): number[] {
  const width = Math.max(1, availableWidth);
  return lineWidths.map((w) => Math.max(1, Math.ceil(w / width)));
}
