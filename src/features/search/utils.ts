/**
 * Search input normalization utility (Phase 6, Task 8).
 * 
 * Rules:
 * - Trims leading and trailing whitespace.
 * - Preserves meaningful Unicode, accents, non-Latin scripts (Hindi, Japanese, Cyrillic, etc.).
 * - Does not perform destructive lowercasing on display strings.
 * - Does not aggressively strip punctuation that may be part of search tokens.
 */
export function normalizeSearchQuery(query: string): string {
  if (!query) return "";
  return query.trim();
}

/**
 * Checks if normalized search query is active and searchable.
 */
export function isValidSearchQuery(query: string): boolean {
  return normalizeSearchQuery(query).length > 0;
}
