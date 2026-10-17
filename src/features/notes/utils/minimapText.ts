/**
 * Lays a note's text out the way the editor wraps it and returns one small box per word, so the
 * minimap can draw a faithful miniature of the real text (word shapes, gaps, headings, code…)
 * instead of abstract bars. Pure and measured through an injected `measure`, so it is unit-testable.
 */

export type MinimapKind = "text" | "strong" | "code" | "link" | "quote" | "image" | "marker";

export interface WordBox {
  /** Offset from the left edge of the text area, in CSS px. */
  x: number;
  /** Offset from the top of the text area, in CSS px. */
  y: number;
  w: number;
  h: number;
  kind: MinimapKind;
}

export interface LayoutOptions {
  measure: (text: string) => number;
  availableWidth: number;
  lineHeight: number;
  maxLines?: number;
  maxWords?: number;
}

export interface LayoutResult {
  boxes: WordBox[];
  /** Number of visual rows the text occupies (blank lines count). */
  rows: number;
  /** True when the note was too big to lay out completely. */
  truncated: boolean;
}

const IMAGE_LINE = /^!\[[^\]]*\]\(notepad:\/\/attachment\/[^)]*\)\s*$/;
const FENCE = /^\s{0,3}(```|~~~)/;
const HEADING = /^\s{0,3}#{1,6}(\s|$)/;
const QUOTE = /^\s{0,3}>/;
const LIST_MARKER = /^\s*([-*+]|\d{1,9}[.)])\s/;

/** Colour group for each word, based on simple Markdown cues. */
function classifyWord(
  word: string,
  lineKind: MinimapKind,
  state: { code: boolean; link: boolean; bold: boolean },
): MinimapKind {
  if (lineKind !== "text") return lineKind;
  // inline `code`, **bold** and [links](…) can span several words
  const startsCode = word.startsWith("`");
  const endsCode = word.endsWith("`") && word.length > 1;
  if (startsCode) state.code = true;
  const isCode = state.code;
  if (endsCode) state.code = false;
  if (isCode) return "code";

  if (word.startsWith("[") || word.startsWith("![")) state.link = true;
  const isLink = state.link;
  if (/\]\([^)]*\)[.,;:!?]?$/.test(word) || word.endsWith(")")) state.link = false;
  if (isLink) return "link";

  if (word.startsWith("**")) state.bold = true;
  const isBold = state.bold;
  if (word.endsWith("**") && word.length > 2) state.bold = false;
  return isBold ? "strong" : "text";
}

export function layoutTextWords(source: string, options: LayoutOptions): LayoutResult {
  const { measure, availableWidth, lineHeight } = options;
  const maxLines = options.maxLines ?? 6000;
  const maxWords = options.maxWords ?? 12000;
  const width = Math.max(1, availableWidth);
  const boxes: WordBox[] = [];
  const widthCache = new Map<string, number>();
  const measureCached = (text: string): number => {
    const known = widthCache.get(text);
    if (known !== undefined) return known;
    const value = measure(text);
    widthCache.set(text, value);
    return value;
  };

  const lines = source.split("\n");
  let row = 0;
  let inFence = false;
  let truncated = lines.length > maxLines;

  for (const line of lines.slice(0, maxLines)) {
    if (boxes.length >= maxWords) {
      truncated = true;
      break;
    }

    let lineKind: MinimapKind = "text";
    if (FENCE.test(line)) {
      inFence = !inFence;
      lineKind = "code";
    } else if (inFence) {
      lineKind = "code";
    } else if (IMAGE_LINE.test(line.trim())) {
      lineKind = "image";
    } else if (HEADING.test(line)) {
      lineKind = "strong";
    } else if (QUOTE.test(line)) {
      lineKind = "quote";
    }

    if (line.trim().length === 0) {
      row += 1;
      continue;
    }

    if (lineKind === "image") {
      boxes.push({ x: 0, y: row * lineHeight, w: width, h: lineHeight * 1.2, kind: "image" });
      row += 1;
      continue;
    }

    const state = { code: false, link: false, bold: false };
    let x = 0;
    let rowOnLine = 0;
    const startRow = row;
    const marker = lineKind === "text" ? LIST_MARKER.exec(line) : null;
    const tokens = line.replace(/\t/g, "    ").split(/(\s+)/);
    let isFirstWord = true;

    for (const token of tokens) {
      if (token === "") continue;
      if (/^\s+$/.test(token)) {
        x += measureCached(token);
        continue;
      }
      const tokenWidth = measureCached(token);
      if (x > 0 && x + tokenWidth > width) {
        rowOnLine += 1;
        x = 0;
      }
      if (tokenWidth > width) {
        // very long word: wraps character-wise across several rows
        const fullRows = Math.floor(tokenWidth / width);
        for (let r = 0; r < fullRows; r += 1) {
          boxes.push({ x: 0, y: (startRow + rowOnLine) * lineHeight, w: width, h: lineHeight * 0.7, kind: classifyWord(token, lineKind, state) });
          rowOnLine += 1;
        }
        const rest = tokenWidth - fullRows * width;
        boxes.push({ x: 0, y: (startRow + rowOnLine) * lineHeight, w: Math.max(1, rest), h: lineHeight * 0.7, kind: "text" });
        x = rest;
      } else {
        const kind =
          isFirstWord && marker && token === marker[1] ? "marker" : classifyWord(token, lineKind, state);
        boxes.push({ x, y: (startRow + rowOnLine) * lineHeight, w: tokenWidth, h: lineHeight * 0.7, kind });
        x += tokenWidth;
      }
      isFirstWord = false;
      if (boxes.length >= maxWords) {
        truncated = true;
        break;
      }
    }
    row += rowOnLine + 1;
  }

  return { boxes, rows: row, truncated };
}
