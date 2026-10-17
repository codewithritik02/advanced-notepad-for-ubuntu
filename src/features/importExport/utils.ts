export type OperationOutcome =
  | "full_success"
  | "success_with_warnings"
  | "partial_success"
  | "cancelled"
  | "total_failure";

/**
 * Sanitizes a note title for safe use as a filename across Windows, macOS, and Linux.
 * Removes illegal filesystem characters, leading/trailing dots/spaces, and prefixes reserved Windows device names.
 */
export function sanitizeExportFilename(title: string): string {
  const illegal = /[/\\:*?"<>|]/g;
  let cleaned = (title || "").replace(illegal, "").trim();
  // Strip leading dots to prevent creating hidden files
  cleaned = cleaned.replace(/^\.+/, "");
  // Strip trailing dots and spaces
  cleaned = cleaned.replace(/[. ]+$/, "");
  if (!cleaned) {
    return "Untitled Note";
  }
  const upper = cleaned.toUpperCase();
  const isReserved = [
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
  ].includes(upper);
  const safeName = isReserved ? `_${cleaned}` : cleaned;
  return safeName.length > 180 ? safeName.slice(0, 180) : safeName;
}

/**
 * Returns a human-friendly display label for an import/export operation outcome.
 */
export function getOutcomeLabel(outcome?: OperationOutcome | string): string {
  switch (outcome) {
    case "full_success":
      return "Full Success";
    case "success_with_warnings":
      return "Completed with Warnings";
    case "partial_success":
      return "Partial Success";
    case "cancelled":
      return "Operation Cancelled";
    case "total_failure":
      return "Total Failure";
    default:
      return "Finished";
  }
}

/**
 * Returns the CSS class modifier associated with an operation outcome badge.
 */
export function getOutcomeBadgeClass(outcome?: OperationOutcome | string): string {
  switch (outcome) {
    case "full_success":
      return "outcome-full-success";
    case "success_with_warnings":
      return "outcome-warnings";
    case "partial_success":
      return "outcome-partial";
    case "cancelled":
      return "outcome-cancelled";
    case "total_failure":
      return "outcome-failure";
    default:
      return "outcome-default";
  }
}
