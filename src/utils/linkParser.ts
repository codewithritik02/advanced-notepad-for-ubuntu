/**
 * Representation of a parsed internal link token in note content
 */
export interface ParsedInternalLink {
  targetId: string;
  label: string;
  raw: string;
  startIndex: number;
  endIndex: number;
}

/**
 * Prefix protocol for internal note links.
 */
export const INTERNAL_LINK_PROTOCOL_PREFIX = "notepad://note/";

/**
 * Regular expression matching the canonical internal link pattern:
 * [label](notepad://note/<target-id>)
 *
 * Rules:
 * - Negative lookbehind for '!' to avoid matching markdown images like ![alt](...)
 * - Label: non-newline characters up to closing bracket ']'
 * - Protocol: case-insensitive 'notepad://note/'
 * - Target ID: alphanumeric, hyphen, underscore
 */
const INTERNAL_LINK_REGEX = /(?:^|[^!])\[((?:\\\]|[^\]\r\n])*)\]\(notepad:\/\/note\/([a-zA-Z0-9_-]+)\)/gi;

/**
 * Checks whether a given URI is an internal note link URI.
 */
export function isInternalLinkUri(uri: string): boolean {
  if (!uri || typeof uri !== "string") return false;
  return uri.trim().toLowerCase().startsWith(INTERNAL_LINK_PROTOCOL_PREFIX);
}

/**
 * Extracts the target note ID from a `notepad://note/<target-id>` URI.
 */
export function extractTargetIdFromUri(uri: string): string | null {
  if (!isInternalLinkUri(uri)) return null;
  const trimmed = uri.trim();
  const idPart = trimmed.slice(INTERNAL_LINK_PROTOCOL_PREFIX.length).trim();
  return idPart.length > 0 ? idPart : null;
}

/**
 * Sanitizes a link label so unescaped closing brackets or newlines do not corrupt the link syntax.
 */
export function sanitizeLinkLabel(label: string): string {
  if (!label) return "Untitled Note";
  // Replace newlines with spaces and escape unescaped closing brackets
  return label
    .replace(/[\r\n]+/g, " ")
    .replace(/\]/g, "\\]")
    .trim() || "Untitled Note";
}

/**
 * Formats a canonical internal note link string.
 * Example: `[Project Roadmap](notepad://note/uuid-123)`
 */
export function formatInternalLink(targetId: string, label?: string): string {
  const sanitizedId = targetId.trim();
  const safeLabel = sanitizeLinkLabel(label ?? "");
  return `[${safeLabel}](${INTERNAL_LINK_PROTOCOL_PREFIX}${sanitizedId})`;
}

const FAST_PREFIX_REGEX = /notepad:\/\/note\//i;

/**
 * Quickly checks if content contains any internal links without constructing full array.
 */
export function hasInternalLinks(content: string): boolean {
  if (!content || typeof content !== "string") return false;
  if (!FAST_PREFIX_REGEX.test(content)) return false;
  const regex = new RegExp(INTERNAL_LINK_REGEX.source, "i");
  return regex.test(content);
}

/**
 * Parses all valid internal note links within the provided content.
 * Guarantees linear-time parsing, Unicode label safety, and immunity to malformed syntax.
 */
export function parseInternalLinks(content: string): ParsedInternalLink[] {
  if (!content || typeof content !== "string") return [];

  // Fast pre-filter: Zero-allocation regex check avoids creating a full lowercase string copy
  if (!FAST_PREFIX_REGEX.test(content)) {
    return [];
  }

  const results: ParsedInternalLink[] = [];
  const regex = new RegExp(INTERNAL_LINK_REGEX.source, "gi");
  let match: RegExpExecArray | null;

  while ((match = regex.exec(content)) !== null) {
    const fullMatch = match[0];
    const rawLabel = match[1];
    const targetId = match[2];

    // Handle leading character when matched by (?:^|[^!])
    const hasLeadingChar = fullMatch.length > 0 && fullMatch[0] !== "[";
    const tokenStart = hasLeadingChar ? match.index + 1 : match.index;
    const rawToken = hasLeadingChar ? fullMatch.slice(1) : fullMatch;
    const tokenEnd = tokenStart + rawToken.length;

    // Unescape any escaped closing brackets in label
    const label = rawLabel.replace(/\\\]/g, "]");

    results.push({
      targetId,
      label,
      raw: rawToken,
      startIndex: tokenStart,
      endIndex: tokenEnd,
    });
  }

  return results;
}

export interface InsertLinkOptions {
  content: string;
  selectionStart?: number;
  selectionEnd?: number;
  targetNoteId: string;
  targetTitle?: string;
  customLabel?: string;
}

export interface InsertLinkResult {
  updatedContent: string;
  newCursorPosition: number;
  insertedText: string;
}

/**
 * Inserts an internal link into note content at the given selection or cursor position.
 *
 * Rules:
 * - If selection range has text and no customLabel is provided, uses selected text as label.
 * - If no text is selected, uses customLabel or targetTitle (defaults to "Untitled Note").
 * - Sanitizes label so closing brackets or newlines cannot corrupt the syntax.
 * - Protects Unicode, start/end of document, and empty content.
 * - Does NOT alter or convert the note's format.
 */
export function insertInternalLinkIntoContent(options: InsertLinkOptions): InsertLinkResult {
  const { content, targetNoteId, targetTitle, customLabel } = options;
  const rawContent = typeof content === "string" ? content : "";
  const len = rawContent.length;

  let start = options.selectionStart ?? len;
  let end = options.selectionEnd ?? start;

  // Clamp safely within bounds
  start = Math.max(0, Math.min(start, len));
  end = Math.max(0, Math.min(end, len));
  if (start > end) {
    const tmp = start;
    start = end;
    end = tmp;
  }

  // Determine label:
  // 1. Explicit customLabel if provided and non-empty
  // 2. Selected text if non-empty and non-whitespace
  // 3. targetTitle if non-empty
  // 4. Fallback to "Untitled Note"
  let chosenLabel: string;
  if (customLabel !== undefined && customLabel.trim().length > 0) {
    chosenLabel = customLabel.trim();
  } else if (start < end) {
    const selectedSlice = rawContent.slice(start, end).trim();
    if (selectedSlice.length > 0) {
      chosenLabel = selectedSlice;
    } else {
      chosenLabel = targetTitle?.trim() || "Untitled Note";
    }
  } else {
    chosenLabel = targetTitle?.trim() || "Untitled Note";
  }

  const linkToken = formatInternalLink(targetNoteId, chosenLabel);
  const before = rawContent.slice(0, start);
  const after = rawContent.slice(end);
  const updatedContent = before + linkToken + after;
  const newCursorPosition = start + linkToken.length;

  return {
    updatedContent,
    newCursorPosition,
    insertedText: linkToken,
  };
}

/**
 * Executes safe link insertion directly into an HTMLTextAreaElement.
 * Attempts document.execCommand('insertText') to preserve the native browser undo/redo history,
 * falling back to manual value setting and selection update.
 */
export function insertLinkIntoTextarea(
  textarea: HTMLTextAreaElement,
  targetNoteId: string,
  targetTitle?: string,
  customLabel?: string
): InsertLinkResult {
  const start = textarea.selectionStart;
  const end = textarea.selectionEnd;
  const content = textarea.value;

  const result = insertInternalLinkIntoContent({
    content,
    selectionStart: start,
    selectionEnd: end,
    targetNoteId,
    targetTitle,
    customLabel,
  });

  textarea.focus();

  let execSuccess = false;
  if (typeof document !== "undefined" && typeof document.execCommand === "function") {
    try {
      execSuccess = document.execCommand("insertText", false, result.insertedText);
    } catch {
      execSuccess = false;
    }
  }

  if (!execSuccess) {
    textarea.value = result.updatedContent;
    textarea.selectionStart = result.newCursorPosition;
    textarea.selectionEnd = result.newCursorPosition;
  }

  return result;
}

export interface ContentSegmentText {
  type: "text";
  text: string;
}

export interface ContentSegmentLink {
  type: "link";
  targetId: string;
  label: string;
  raw: string;
}

export type ContentSegment = ContentSegmentText | ContentSegmentLink;

/**
 * Splits note content into an ordered sequence of plain text and internal link segments.
 * Guarantees zero raw HTML injection, Unicode safety, and piecewise component rendering.
 */
export function tokenizeContentWithLinks(content: string): ContentSegment[] {
  if (!content || typeof content !== "string") {
    return [{ type: "text", text: "" }];
  }

  const links = parseInternalLinks(content);
  if (links.length === 0) {
    return [{ type: "text", text: content }];
  }

  const segments: ContentSegment[] = [];
  let lastIndex = 0;

  for (const link of links) {
    if (link.startIndex > lastIndex) {
      segments.push({
        type: "text",
        text: content.slice(lastIndex, link.startIndex),
      });
    }

    segments.push({
      type: "link",
      targetId: link.targetId,
      label: link.label,
      raw: link.raw,
    });

    lastIndex = link.endIndex;
  }

  if (lastIndex < content.length) {
    segments.push({
      type: "text",
      text: content.slice(lastIndex),
    });
  }

  return segments;
}

/**
 * Replaces occurrences of an internal link target ID with a new target note (Task 16).
 * Modifies only links that specifically point to oldTargetId.
 * Preserves the existing display label unless a non-empty replacement title is provided.
 */
export function replaceInternalLinkTarget(
  content: string,
  oldTargetId: string,
  newTargetId: string,
  newTitle?: string
): string {
  if (!content || !oldTargetId || !newTargetId) return content;
  const links = parseInternalLinks(content);
  const matching = links.filter((l) => l.targetId === oldTargetId);
  if (matching.length === 0) return content;

  let updated = content;
  // Replace from end to start to avoid invalidating character indices
  for (let i = matching.length - 1; i >= 0; i--) {
    const link = matching[i];
    const displayLabel = newTitle && newTitle.trim().length > 0 ? newTitle : link.label;
    const replacementToken = formatInternalLink(newTargetId, displayLabel);
    updated = updated.slice(0, link.startIndex) + replacementToken + updated.slice(link.endIndex);
  }
  return updated;
}

/**
 * Removes internal link markup for a given target ID while preserving its display label text (Task 16).
 */
export function removeInternalLink(
  content: string,
  targetId: string
): string {
  if (!content || !targetId) return content;
  const links = parseInternalLinks(content);
  const matching = links.filter((l) => l.targetId === targetId);
  if (matching.length === 0) return content;

  let updated = content;
  // Replace from end to start to preserve indices
  for (let i = matching.length - 1; i >= 0; i--) {
    const link = matching[i];
    updated = updated.slice(0, link.startIndex) + link.label + updated.slice(link.endIndex);
  }
  return updated;
}

