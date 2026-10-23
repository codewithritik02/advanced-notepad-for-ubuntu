/**
 * Small, dependency-free Markdown parser for note previews.
 *
 * Output is a plain AST that React renders as elements, so raw HTML in a note is never
 * injected into the page (it is shown as text). Supported: headings, paragraphs (single newlines
 * are line breaks), emphasis, strong, strikethrough, inline code, fenced code, block quotes,
 * ordered/unordered/task lists (nested), horizontal rules, GFM tables, links, autolinks, images
 * (shown as text - nothing is fetched from the network) and internal `notepad://note/<id>` links.
 *
 * Parsing is line/linear oriented and bounded by size limits so hostile or huge notes cannot
 * freeze the UI.
 */

export type MdInline =
  | { t: "text"; v: string }
  | { t: "code"; v: string }
  | { t: "strong"; c: MdInline[] }
  | { t: "em"; c: MdInline[] }
  | { t: "del"; c: MdInline[] }
  | { t: "link"; href: string; c: MdInline[] }
  | { t: "internal"; id: string; label: string }
  | { t: "image"; alt: string }
  | { t: "attachment"; id: string; alt: string; width?: number }
  | { t: "br" };

export type MdAlign = "left" | "center" | "right" | null;

export interface MdListItem {
  checked: boolean | null;
  c: MdBlock[];
}

export type MdBlock =
  | { t: "blank"; lines: number }
  | { t: "heading"; level: number; c: MdInline[] }
  | { t: "paragraph"; c: MdInline[] }
  | { t: "code"; lang: string; v: string }
  | { t: "quote"; c: MdBlock[] }
  | { t: "list"; ordered: boolean; start: number; items: MdListItem[] }
  | { t: "hr" }
  | { t: "table"; align: MdAlign[]; head: MdInline[][]; rows: MdInline[][][] };

/** Above this size the preview falls back to plain text. */
export const MAX_MARKDOWN_CHARS = 500_000;
/** Lines longer than this are not inline-parsed (keeps worst-case parsing linear-ish). */
const MAX_INLINE_CHARS = 20_000;
const MAX_NESTING = 8;
const MAX_LABEL_SCAN = 2_000;
const INTERNAL_PREFIX = "notepad://note/";
export const ATTACHMENT_URI_PREFIX = "notepad://attachment/";
const MIN_IMAGE_WIDTH = 40;
const MAX_IMAGE_WIDTH = 2000;

/**
 * Markdown line that places an attached image in a note: `![name](notepad://attachment/<id>)`.
 * The optional width (px) is stored after a pipe in the alt text: `![name|400](...)`.
 */
export function formatAttachmentImageMarkdown(
  attachmentId: string,
  fileName: string,
  width?: number,
): string {
  const base = fileName.replace(/\.[^./\\]+$/, "");
  const cleaned = base.replace(/[\[\]()|\\\r\n]/g, " ").replace(/\s+/g, " ").trim() || "image";
  const sized =
    width !== undefined && Number.isFinite(width)
      ? `|${Math.min(MAX_IMAGE_WIDTH, Math.max(MIN_IMAGE_WIDTH, Math.round(width)))}`
      : "";
  return `![${cleaned}${sized}](${ATTACHMENT_URI_PREFIX}${attachmentId.trim()})`;
}

const SAFE_URL = /^(https?:\/\/|mailto:)/i;

/** Returns the URL when it is safe to open (http, https, mailto), otherwise null. */
export function sanitizeMarkdownUrl(raw: string): string | null {
  const url = raw.trim();
  if (!url || /[\u0000-\u001f\u007f\s]/.test(url)) return null;
  return SAFE_URL.test(url) ? url : null;
}

// ---------------------------------------------------------------------------------------------
// Inline parsing
// ---------------------------------------------------------------------------------------------

const PUNCT = /[!-/:-@[-`{-~]/;

function isWhitespace(ch: string | undefined): boolean {
  return ch === undefined || ch === "" || /\s/.test(ch);
}

function isWordChar(ch: string | undefined): boolean {
  return ch !== undefined && ch !== "" && /[\p{L}\p{N}]/u.test(ch);
}

function pushText(out: MdInline[], v: string): void {
  if (!v) return;
  const last = out[out.length - 1];
  if (last && last.t === "text") last.v += v;
  else out.push({ t: "text", v });
}

/** Finds the `]` that closes the `[` at `open`, honouring nesting and escapes. */
function findClosingBracket(src: string, open: number): number {
  let depth = 0;
  // Link/image labels are short; a bounded window keeps unmatched "[" runs from going quadratic.
  const limit = Math.min(src.length, open + MAX_LABEL_SCAN);
  for (let i = open; i < limit; i++) {
    const ch = src[i];
    if (ch === "\\") {
      i++;
    } else if (ch === "`") {
      const run = /^`+/.exec(src.slice(i))![0];
      const end = src.indexOf(run, i + run.length);
      if (end !== -1) i = end + run.length - 1;
    } else if (ch === "[") {
      depth++;
    } else if (ch === "]") {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/** Parses `(url "optional title")` starting at `open` (which must be `(`). */
function parseLinkDestination(
  src: string,
  open: number,
): { url: string; end: number } | null {
  let i = open + 1;
  while (src[i] === " ") i++;
  let url = "";
  if (src[i] === "<") {
    const close = src.indexOf(">", i + 1);
    if (close === -1) return null;
    url = src.slice(i + 1, close);
    i = close + 1;
  } else {
    let depth = 0;
    const start = i;
    while (i < src.length) {
      const ch = src[i];
      if (ch === "\\") {
        i += 2;
        continue;
      }
      if (ch === "(") depth++;
      else if (ch === ")") {
        if (depth === 0) break;
        depth--;
      } else if (/\s/.test(ch)) break;
      i++;
    }
    url = src.slice(start, i);
  }
  while (src[i] === " ") i++;
  if (src[i] === '"' || src[i] === "'") {
    const quote = src[i];
    const close = src.indexOf(quote, i + 1);
    if (close === -1) return null;
    i = close + 1;
    while (src[i] === " ") i++;
  }
  if (src[i] !== ")") return null;
  return { url: url.replace(/\\([!-/:-@[-`{-~])/g, "$1"), end: i + 1 };
}

function plainText(nodes: MdInline[]): string {
  return nodes
    .map((n) => {
      switch (n.t) {
        case "text":
        case "code":
          return n.v;
        case "br":
          return " ";
        case "image":
        case "attachment":
          return n.alt;
        case "internal":
          return n.label;
        default:
          return plainText(n.c);
      }
    })
    .join("");
}

/** Finds a closing emphasis delimiter run `delim` after `from` that can close an opener. */
function findCloser(src: string, delim: string, from: number): number {
  let i = from;
  while (i < src.length) {
    const ch = src[i];
    if (ch === "\\") {
      i += 2;
      continue;
    }
    if (ch === "`") {
      const run = /^`+/.exec(src.slice(i))![0];
      const end = src.indexOf(run, i + run.length);
      i = end === -1 ? i + run.length : end + run.length;
      continue;
    }
    if (src.startsWith(delim, i)) {
      const before = src[i - 1];
      const after = src[i + delim.length];
      const leftFlanked = !isWhitespace(after);
      const rightFlanked = !isWhitespace(before);
      // For `_` require a real word boundary so snake_case_names stay intact.
      const intraword = delim[0] === "_" && isWordChar(after);
      // Skip longer runs that merely start with this delimiter (e.g. `**` while looking for `*`).
      const longerRun = src[i + delim.length] === delim[0] && delim.length === 1;
      if (rightFlanked && !intraword && !longerRun && i > from) return i;
      if (leftFlanked && !rightFlanked) {
        // Opens a nested run instead of closing; skip over it.
        i += delim.length;
        continue;
      }
    }
    i++;
  }
  return -1;
}

export function parseInline(src: string): MdInline[] {
  if (src.length > MAX_INLINE_CHARS) return [{ t: "text", v: src }];
  return parseInlineRange(src, 0);
}

function parseInlineRange(src: string, depth: number): MdInline[] {
  const out: MdInline[] = [];
  if (depth > MAX_NESTING) {
    pushText(out, src);
    return out;
  }
  let i = 0;
  let textStart = 0;
  const flush = (end: number) => {
    if (end > textStart) pushText(out, src.slice(textStart, end));
  };

  while (i < src.length) {
    const ch = src[i];

    // Escapes
    if (ch === "\\" && i + 1 < src.length) {
      const next = src[i + 1];
      if (next === "\n") {
        flush(i);
        out.push({ t: "br" });
        i += 2;
        textStart = i;
        continue;
      }
      if (PUNCT.test(next)) {
        flush(i);
        pushText(out, next);
        i += 2;
        textStart = i;
        continue;
      }
    }

    // Line breaks (every newline is a visible break in notes)
    if (ch === "\n") {
      flush(i);
      out.push({ t: "br" });
      i++;
      textStart = i;
      continue;
    }

    // Inline code
    if (ch === "`") {
      const run = /^`+/.exec(src.slice(i))![0];
      const end = src.indexOf(run, i + run.length);
      if (end !== -1) {
        flush(i);
        let code = src.slice(i + run.length, end).replace(/\n/g, " ");
        if (code.length > 2 && code.startsWith(" ") && code.endsWith(" ") && code.trim()) {
          code = code.slice(1, -1);
        }
        out.push({ t: "code", v: code });
        i = end + run.length;
        textStart = i;
        continue;
      }
      i += run.length;
      continue;
    }

    // Images and links
    if (ch === "!" && src[i + 1] === "[") {
      const close = findClosingBracket(src, i + 1);
      if (close !== -1 && src[close + 1] === "(") {
        const dest = parseLinkDestination(src, close + 1);
        if (dest) {
          flush(i);
          const labelSrc = src.slice(i + 2, close);
          const attachmentId = dest.url.toLowerCase().startsWith(ATTACHMENT_URI_PREFIX)
            ? dest.url.slice(ATTACHMENT_URI_PREFIX.length).trim()
            : "";
          if (/^[a-zA-Z0-9_-]+$/.test(attachmentId)) {
            // Attached image from this note: `![name](notepad://attachment/<id>)`, optional `|width`
            const sizeMatch = /^(.*)\|(\d{2,4})$/s.exec(labelSrc);
            const altText = plainText(
              parseInlineRange(sizeMatch ? sizeMatch[1] : labelSrc, depth + 1),
            ).trim();
            const width = sizeMatch
              ? Math.min(MAX_IMAGE_WIDTH, Math.max(MIN_IMAGE_WIDTH, parseInt(sizeMatch[2], 10)))
              : undefined;
            out.push(
              width === undefined
                ? { t: "attachment", id: attachmentId, alt: altText }
                : { t: "attachment", id: attachmentId, alt: altText, width },
            );
          } else {
            const alt = plainText(parseInlineRange(labelSrc, depth + 1));
            out.push({ t: "image", alt });
          }
          i = dest.end;
          textStart = i;
          continue;
        }
      }
    }
    if (ch === "[") {
      const close = findClosingBracket(src, i);
      if (close !== -1 && src[close + 1] === "(") {
        const dest = parseLinkDestination(src, close + 1);
        if (dest) {
          const labelSrc = src.slice(i + 1, close);
          if (dest.url.toLowerCase().startsWith(INTERNAL_PREFIX)) {
            const id = dest.url.slice(INTERNAL_PREFIX.length).trim();
            if (/^[a-zA-Z0-9_-]+$/.test(id)) {
              flush(i);
              out.push({
                t: "internal",
                id,
                label: labelSrc.replace(/\\\]/g, "]").trim() || "Untitled Note",
              });
              i = dest.end;
              textStart = i;
              continue;
            }
          } else {
            const safe = sanitizeMarkdownUrl(dest.url);
            flush(i);
            const label = parseInlineRange(labelSrc, depth + 1);
            if (safe) out.push({ t: "link", href: safe, c: label });
            else for (const node of label) out.push(node);
            i = dest.end;
            textStart = i;
            continue;
          }
        }
      }
    }

    // Autolinks <https://...>
    if (ch === "<") {
      const m = /^<((?:https?:\/\/|mailto:)[^\s<>]+)>/i.exec(src.slice(i, i + 2048));
      if (m) {
        flush(i);
        out.push({ t: "link", href: m[1], c: [{ t: "text", v: m[1] }] });
        i += m[0].length;
        textStart = i;
        continue;
      }
    }

    // Bare URLs
    if ((ch === "h" || ch === "H") && /^https?:\/\//i.test(src.slice(i, i + 8)) && !isWordChar(src[i - 1])) {
      const m = /^https?:\/\/[^\s<>]+/i.exec(src.slice(i, i + 2048));
      if (m) {
        let url = m[0].replace(/[.,;:!?'"]+$/, "");
        // Do not swallow an unbalanced closing parenthesis, e.g. "(see https://x.com)"
        while (url.endsWith(")") && (url.match(/\(/g)?.length ?? 0) < (url.match(/\)/g)?.length ?? 0)) {
          url = url.slice(0, -1);
        }
        flush(i);
        out.push({ t: "link", href: url, c: [{ t: "text", v: url }] });
        i += url.length;
        textStart = i;
        continue;
      }
    }

    // Emphasis family
    if (ch === "*" || ch === "_" || ch === "~") {
      let run = 1;
      while (src[i + run] === ch) run++;
      const candidates: string[] = [];
      if (ch === "~") {
        if (run >= 2) candidates.push("~~");
      } else {
        if (run >= 2) candidates.push(ch + ch);
        candidates.push(ch);
      }
      let handled = false;
      for (const delim of candidates) {
        const after = src[i + delim.length];
        const before = src[i - 1];
        if (isWhitespace(after)) continue;
        if (ch === "_" && isWordChar(before)) continue;
        const end = findCloser(src, delim, i + delim.length);
        if (end === -1) continue;
        const inner = src.slice(i + delim.length, end);
        if (!inner.trim()) continue;
        flush(i);
        const children = parseInlineRange(inner, depth + 1);
        out.push(
          delim === "~~"
            ? { t: "del", c: children }
            : delim.length === 2
              ? { t: "strong", c: children }
              : { t: "em", c: children },
        );
        i = end + delim.length;
        textStart = i;
        handled = true;
        break;
      }
      if (handled) continue;
      i += run;
      continue;
    }

    i++;
  }
  flush(src.length);
  return out;
}

// ---------------------------------------------------------------------------------------------
// Block parsing
// ---------------------------------------------------------------------------------------------

const FENCE_RE = /^ {0,3}(`{3,}|~{3,})\s*([^\s`]*)[^`]*$/;
const HEADING_RE = /^ {0,3}(#{1,6})(?:\s+(.*?))?(?:\s+#+)?\s*$/;
const HR_RE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const QUOTE_RE = /^ {0,3}> ?/;
const LIST_RE = /^( *)([-*+]|\d{1,9}[.)])[ \t]+(.*)$/;
const LIST_EMPTY_RE = /^( *)([-*+]|\d{1,9}[.)])[ \t]*$/;
const TABLE_SEP_RE = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;
const SETEXT_RE = /^ {0,3}(=+|-+)[ \t]*$/;

function indentOf(line: string): number {
  let n = 0;
  for (const ch of line) {
    if (ch === " ") n++;
    else if (ch === "\t") n += 4;
    else break;
  }
  return n;
}

function splitTableRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const cells: string[] = [];
  let cur = "";
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "\\" && s[i + 1] === "|") {
      cur += "|";
      i++;
    } else if (s[i] === "|") {
      cells.push(cur.trim());
      cur = "";
    } else cur += s[i];
  }
  cells.push(cur.trim());
  return cells;
}

function isTableStart(lines: string[], i: number): boolean {
  return (
    i + 1 < lines.length &&
    lines[i].includes("|") &&
    lines[i + 1].includes("-") &&
    TABLE_SEP_RE.test(lines[i + 1]) &&
    splitTableRow(lines[i]).length === splitTableRow(lines[i + 1]).length
  );
}

function startsBlock(lines: string[], i: number): boolean {
  const line = lines[i];
  return (
    FENCE_RE.test(line) ||
    (HEADING_RE.test(line) && /^ {0,3}#/.test(line)) ||
    HR_RE.test(line) ||
    (QUOTE_RE.test(line) && line.trimStart().startsWith(">")) ||
    LIST_RE.test(line) ||
    isTableStart(lines, i)
  );
}

function parseBlocksFromLines(lines: string[], depth: number): MdBlock[] {
  const blocks: MdBlock[] = [];
  if (depth > MAX_NESTING) {
    blocks.push({ t: "paragraph", c: [{ t: "text", v: lines.join("\n") }] });
    return blocks;
  }
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    if (!line.trim()) {
      const start = i;
      while (i < lines.length && !lines[i].trim()) i++;
      blocks.push({ t: "blank", lines: i - start });
      continue;
    }

    // Fenced code
    const fence = FENCE_RE.exec(line);
    if (fence) {
      const marker = fence[1];
      const ch = marker[0];
      const body: string[] = [];
      let j = i + 1;
      const closeRe = new RegExp(`^ {0,3}\\${ch}{${marker.length},}[ \\t]*$`);
      while (j < lines.length && !closeRe.test(lines[j])) {
        body.push(lines[j]);
        j++;
      }
      blocks.push({ t: "code", lang: fence[2] || "", v: body.join("\n") });
      i = j < lines.length ? j + 1 : j;
      continue;
    }

    // Heading
    const heading = HEADING_RE.exec(line);
    if (heading && /^ {0,3}#/.test(line)) {
      blocks.push({ t: "heading", level: heading[1].length, c: parseInline((heading[2] ?? "").trim()) });
      i++;
      continue;
    }

    // Horizontal rule
    if (HR_RE.test(line)) {
      blocks.push({ t: "hr" });
      i++;
      continue;
    }

    // Block quote
    if (QUOTE_RE.test(line) && line.trimStart().startsWith(">")) {
      const inner: string[] = [];
      while (i < lines.length && lines[i].trimStart().startsWith(">") && QUOTE_RE.test(lines[i])) {
        inner.push(lines[i].replace(QUOTE_RE, ""));
        i++;
      }
      blocks.push({ t: "quote", c: parseBlocksFromLines(inner, depth + 1) });
      continue;
    }

    // Table
    if (isTableStart(lines, i)) {
      const headCells = splitTableRow(lines[i]);
      const align: MdAlign[] = splitTableRow(lines[i + 1]).map((c) => {
        const left = c.startsWith(":");
        const right = c.endsWith(":");
        return left && right ? "center" : right ? "right" : left ? "left" : null;
      });
      const rows: MdInline[][][] = [];
      let j = i + 2;
      while (j < lines.length && lines[j].trim() && lines[j].includes("|")) {
        const cells = splitTableRow(lines[j]);
        rows.push(headCells.map((_, k) => parseInline(cells[k] ?? "")));
        j++;
      }
      blocks.push({ t: "table", align, head: headCells.map((c) => parseInline(c)), rows });
      i = j;
      continue;
    }

    // List
    const listStart = LIST_RE.exec(line) || LIST_EMPTY_RE.exec(line);
    if (listStart) {
      const baseIndent = listStart[1].length;
      const ordered = /\d/.test(listStart[2][0]);
      const start = ordered ? parseInt(listStart[2], 10) : 1;
      const items: MdListItem[] = [];
      while (i < lines.length) {
        const m = LIST_RE.exec(lines[i]) || LIST_EMPTY_RE.exec(lines[i]);
        if (!m || m[1].length !== baseIndent || /\d/.test(m[2][0]) !== ordered) break;
        const first = m[3] ?? "";
        const itemLines: string[] = [first];
        i++;
        while (i < lines.length) {
          const cur = lines[i];
          if (!cur.trim()) {
            // a blank line continues the item only when the next non-blank line is indented deeper
            let k = i + 1;
            while (k < lines.length && !lines[k].trim()) k++;
            if (k < lines.length && indentOf(lines[k]) > baseIndent) {
              itemLines.push("");
              i++;
              continue;
            }
            break;
          }
          if (indentOf(cur) > baseIndent) {
            itemLines.push(cur);
            i++;
            continue;
          }
          break;
        }
        // De-indent continuation lines by their common indent
        const rest = itemLines.slice(1);
        const nonBlank = rest.filter((l) => l.trim());
        const strip = nonBlank.length ? Math.min(...nonBlank.map(indentOf)) : 0;
        const body = [first, ...rest.map((l) => (l.trim() ? l.replace(new RegExp(`^[ \\t]{0,${strip}}`), "") : ""))];
        let checked: boolean | null = null;
        const task = /^\[([ xX])\][ \t]+/.exec(body[0]);
        if (task) {
          checked = task[1] !== " ";
          body[0] = body[0].slice(task[0].length);
        }
        items.push({ checked, c: parseBlocksFromLines(body, depth + 1) });
      }
      blocks.push({ t: "list", ordered, start, items });
      continue;
    }

    // Paragraph (with setext heading support)
    const para: string[] = [line];
    i++;
    let setext = 0;
    while (i < lines.length && lines[i].trim()) {
      const sm = SETEXT_RE.exec(lines[i]);
      if (sm) {
        setext = sm[1][0] === "=" ? 1 : 2;
        i++;
        break;
      }
      if (startsBlock(lines, i)) break;
      para.push(lines[i]);
      i++;
    }
    const text = para.join("\n");
    blocks.push(
      setext
        ? { t: "heading", level: setext, c: parseInline(text.replace(/\n/g, " ")) }
        : { t: "paragraph", c: parseInline(text) },
    );
  }
  return blocks;
}

/** Parses Markdown into blocks. Returns null when the note is too large to render safely. */
export function parseMarkdown(source: string): MdBlock[] | null {
  if (typeof source !== "string") return [];
  if (source.length > MAX_MARKDOWN_CHARS) return null;
  const lines = source.replace(/\r\n?/g, "\n").replace(/\t/g, "    ").split("\n");
  return parseBlocksFromLines(lines, 0);
}

function inlineToPlain(nodes: MdInline[]): string {
  return nodes
    .map((n) => {
      switch (n.t) {
        case "text":
        case "code":
          return n.v;
        case "br":
          return " ";
        case "image":
        case "attachment":
          return n.alt;
        case "internal":
          return n.label;
        default:
          return inlineToPlain(n.c);
      }
    })
    .join("");
}

function blocksToPlain(blocks: MdBlock[], out: string[]): void {
  for (const block of blocks) {
    switch (block.t) {
      case "heading":
      case "paragraph":
        out.push(inlineToPlain(block.c));
        break;
      case "code":
        out.push(block.v);
        break;
      case "quote":
        blocksToPlain(block.c, out);
        break;
      case "list":
        for (const item of block.items) blocksToPlain(item.c, out);
        break;
      case "table":
        out.push(block.head.map(inlineToPlain).join(" "));
        for (const row of block.rows) out.push(row.map(inlineToPlain).join(" "));
        break;
      default:
        break;
    }
  }
}

/**
 * Plain-text version of Markdown (no `#`, `**`, backticks, ...) for note list snippets.
 * Falls back to the original text when the input is too large to parse.
 */
export function markdownToPlainText(source: string): string {
  const blocks = parseMarkdown(source);
  if (blocks === null) return source;
  const parts: string[] = [];
  blocksToPlain(blocks, parts);
  return parts.join(" ");
}

export type PlainTextPart =
  | { type: "text"; text: string }
  | { type: "attachment"; id: string; alt: string; width?: number };

const ATTACHMENT_IMAGE_REGEX = /!\[([^\]\n]*)\]\(notepad:\/\/attachment\/([a-zA-Z0-9_-]+)\)/g;

/**
 * Splits plain text into text and attached-image parts, so a plain-text (TXT) note can show its
 * `![name](notepad://attachment/<id>)` lines as pictures without being Markdown.
 * Text segments retain all whitespace around the inline image.
 */
export function splitAttachmentImages(text: string): PlainTextPart[] {
  const parts: PlainTextPart[] = [];
  let last = 0;
  for (const match of text.matchAll(ATTACHMENT_IMAGE_REGEX)) {
    const index = match.index ?? 0;
    if (index > last) parts.push({ type: "text", text: text.slice(last, index) });
    const sizeMatch = /^(.*)\|(\d{2,4})$/s.exec(match[1]);
    const width = sizeMatch
      ? Math.min(MAX_IMAGE_WIDTH, Math.max(MIN_IMAGE_WIDTH, parseInt(sizeMatch[2], 10)))
      : undefined;
    const alt = (sizeMatch ? sizeMatch[1] : match[1]).trim();
    parts.push(
      width === undefined
        ? { type: "attachment", id: match[2], alt }
        : { type: "attachment", id: match[2], alt, width },
    );
    last = index + match[0].length;
  }
  if (last < text.length) parts.push({ type: "text", text: text.slice(last) });

  return parts;
}
