/**
 * File size and type presentation formatters (Phase 7, Tasks 11 & 12).
 *
 * Design and Security Guarantees:
 * - Deterministic, human-readable file-size formatting across all byte ranges (0 B to Petabytes).
 * - Safe presentation of unknown or unusual file types with clean generic fallback.
 * - SECURITY NOTICE: File category matching and extension inspection is purely presentational
 *   for UI badges and accessibility labels. It does NOT certify or imply that a file is safe,
 *   benign, or non-executable. Operating system dispatch in Task 13 relies on strict managed
 *   storage sandbox verification rather than UI category heuristics.
 */

export type FileCategory =
  | "pdf"
  | "image"
  | "text"
  | "archive"
  | "audio"
  | "video"
  | "code"
  | "generic";

/**
 * Converts a byte count into a consistent, human-readable string using binary-multiples (1024 base).
 *
 * Examples:
 * - 0 bytes -> "0 B"
 * - 512 bytes -> "512 B"
 * - 1,024 bytes -> "1.0 KB"
 * - 1,536 bytes -> "1.5 KB"
 * - 1,048,576 bytes -> "1.0 MB"
 * - 1,073,741,824 bytes -> "1.0 GB"
 *
 * Handles negative, non-finite, and extremely large numbers safely without throwing.
 */
export function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }

  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const unitIndex = Math.min(Math.max(i, 0), units.length - 1);

  if (unitIndex === 0) {
    return `${Math.round(bytes)} B`;
  }

  const value = bytes / Math.pow(k, unitIndex);
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}

/**
 * Infers a file category for presentation and icon selection based on filename and MIME type.
 * Gracefully falls back to "generic" for unknown, unmapped, or missing extensions.
 */
export function getFileCategory(
  fileName: string,
  mimeType?: string | null
): FileCategory {
  const normalizedMime = (mimeType || "").trim().toLowerCase();
  const ext = (fileName.split(".").pop() || "").trim().toLowerCase();

  // 1. PDF Documents
  if (normalizedMime === "application/pdf" || ext === "pdf") {
    return "pdf";
  }

  // 2. Images
  if (
    normalizedMime.startsWith("image/") ||
    [
      "png",
      "jpg",
      "jpeg",
      "gif",
      "webp",
      "svg",
      "bmp",
      "ico",
      "tiff",
      "tif",
      "avif",
      "heic",
    ].includes(ext)
  ) {
    return "image";
  }

  // 3. Plain & Formatted Text Documents
  if (
    normalizedMime.startsWith("text/plain") ||
    normalizedMime.startsWith("text/markdown") ||
    normalizedMime === "text/csv" ||
    normalizedMime === "text/tab-separated-values" ||
    ["txt", "md", "markdown", "csv", "tsv", "log", "rtf", "rst", "adoc"].includes(ext)
  ) {
    return "text";
  }

  // 4. Archives & Compressed Bundles
  if (
    normalizedMime.includes("zip") ||
    normalizedMime.includes("tar") ||
    normalizedMime.includes("compressed") ||
    normalizedMime === "application/x-7z-compressed" ||
    normalizedMime === "application/x-rar-compressed" ||
    ["zip", "tar", "gz", "tgz", "7z", "rar", "bz2", "xz", "zst", "iso"].includes(ext)
  ) {
    return "archive";
  }

  // 5. Audio Media
  if (
    normalizedMime.startsWith("audio/") ||
    ["mp3", "wav", "ogg", "flac", "aac", "m4a", "wma", "opus"].includes(ext)
  ) {
    return "audio";
  }

  // 6. Video Media
  if (
    normalizedMime.startsWith("video/") ||
    ["mp4", "mkv", "webm", "avi", "mov", "wmv", "flv", "m4v"].includes(ext)
  ) {
    return "video";
  }

  // 7. Source Code & Markup
  if (
    normalizedMime.startsWith("text/") ||
    normalizedMime === "application/json" ||
    normalizedMime === "application/xml" ||
    normalizedMime === "application/javascript" ||
    normalizedMime === "application/typescript" ||
    [
      "js",
      "jsx",
      "ts",
      "tsx",
      "rs",
      "py",
      "json",
      "html",
      "css",
      "scss",
      "less",
      "c",
      "cpp",
      "h",
      "hpp",
      "go",
      "java",
      "sh",
      "bash",
      "zsh",
      "toml",
      "yaml",
      "yml",
      "sql",
      "xml",
      "graphql",
      "lua",
      "rb",
      "php",
    ].includes(ext)
  ) {
    return "code";
  }

  // 8. Generic Default Fallback
  return "generic";
}

/**
 * Formats ISO timestamp for attachment creation display.
 */
export function formatAttachmentDate(isoString?: string | null): string {
  if (!isoString) return "";
  try {
    const d = new Date(isoString);
    if (isNaN(d.getTime())) return "";
    return d.toLocaleDateString("en-GB", {
      day: "numeric",
      month: "short",
      year: "numeric",
    });
  } catch {
    return "";
  }
}
