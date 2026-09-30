export const MAX_TAG_NAME_LENGTH = 100;

export interface TagValidationResult {
  valid: boolean;
  error?: string;
  trimmed: string;
}

/**
 * Validates and trims a tag name.
 * - Rejects empty or whitespace-only names.
 * - Enforces maximum length limit (100 characters, counting Unicode code points).
 * - Trims leading and trailing whitespace.
 */
export function validateTagName(name: string): TagValidationResult {
  const trimmed = name.trim();
  if (trimmed.length === 0) {
    return {
      valid: false,
      error: "Tag name cannot be empty",
      trimmed: "",
    };
  }
  // Count Unicode code points correctly
  const charCount = Array.from(trimmed).length;
  if (charCount > MAX_TAG_NAME_LENGTH) {
    return {
      valid: false,
      error: `Tag name cannot exceed ${MAX_TAG_NAME_LENGTH} characters`,
      trimmed,
    };
  }
  return {
    valid: true,
    trimmed,
  };
}

/**
 * Normalizes a tag name for case-insensitive duplicate checking.
 */
export function normalizeTagName(name: string): string {
  return name.trim().toLowerCase();
}

/**
 * Checks if two tag names are equal under case-insensitive comparison.
 */
export function areTagNamesEqual(a: string, b: string): boolean {
  return normalizeTagName(a) === normalizeTagName(b);
}

/**
 * Finds an existing tag in a list by case-insensitive name match.
 */
export function findTagByName<T extends { name: string }>(
  tags: T[],
  name: string
): T | undefined {
  const normalized = normalizeTagName(name);
  return tags.find((t) => normalizeTagName(t.name) === normalized);
}

