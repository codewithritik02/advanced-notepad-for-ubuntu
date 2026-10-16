/**
 * Typed Failure Classification and Safe Error Handling (Phase 13, Task 3)
 *
 * Provides frontend failure classification, actionable messaging, retryability detection,
 * and safe diagnostics without exposing raw SQL, secret material, or note bodies.
 */

export type ErrorCategory =
  | "VALIDATION"
  | "NOT_FOUND"
  | "PERMISSION"
  | "DISK_SPACE"
  | "DATABASE_BUSY"
  | "LOCKED"
  | "CORRUPTION"
  | "UNSUPPORTED_FORMAT"
  | "CANCELLED"
  | "CONFLICT"
  | "INTERNAL";

export interface ClassifiedError {
  /** Machine-readable error code */
  code: string;
  /** Semantic failure classification */
  category: ErrorCategory;
  /** Safe, human-readable message suitable for UI display */
  message: string;
  /** Indicates whether the failure is transient and safe to retry automatically or via user button */
  retryable: boolean;
  /** Safe entity ID for contextual reporting (never contains passwords or note content) */
  safeEntityId?: string;
  /** Operation name during which failure occurred */
  operation?: string;
}

/**
 * Classifies any error (Tauri string rejection, Error instance, or object) into a typed ClassifiedError.
 */
export function classifyError(
  error: unknown,
  context?: { operation?: string; entityId?: string }
): ClassifiedError {
  const operation = context?.operation;
  const safeEntityId = context?.entityId;

  // 1. If error is already a structured diagnostic object
  if (
    typeof error === "object" &&
    error !== null &&
    "category" in error &&
    "code" in error &&
    "message" in error
  ) {
    const errObj = error as {
      code: string;
      category: ErrorCategory;
      message: string;
      retryable?: boolean;
    };
    return {
      code: errObj.code,
      category: errObj.category,
      message: errObj.message,
      retryable: Boolean(errObj.retryable),
      safeEntityId,
      operation,
    };
  }

  // 2. Extract string message
  const rawMsg =
    typeof error === "string"
      ? error
      : error instanceof Error
      ? error.message
      : typeof error === "object" && error !== null && "toString" in error
      ? String(error)
      : "An unexpected error occurred.";

  const lower = rawMsg.toLowerCase();

  // 3. Classify based on recognized patterns
  if (
    lower.includes("operation was cancelled") ||
    lower.includes("cancelled") ||
    lower.includes("canceled") ||
    lower.includes("abort")
  ) {
    return {
      code: "OPERATION_CANCELLED",
      category: "CANCELLED",
      message: "The operation was cancelled.",
      retryable: false,
      safeEntityId,
      operation,
    };
  }

  if (
    lower.includes("database is currently busy") ||
    lower.includes("sqlite_busy") ||
    lower.includes("database is locked") ||
    lower.includes("database busy") ||
    lower.includes("another backup operation is currently executing")
  ) {
    return {
      code: "DATABASE_BUSY",
      category: "DATABASE_BUSY",
      message: "The database is currently busy. Please try again in a moment.",
      retryable: true,
      safeEntityId,
      operation,
    };
  }

  if (
    lower.includes("application is locked") ||
    lower.includes("unlock first") ||
    lower.includes("session locked")
  ) {
    return {
      code: "APP_LOCKED",
      category: "LOCKED",
      message: "Personal Notepad is locked. Please unlock with your password to proceed.",
      retryable: false,
      safeEntityId,
      operation,
    };
  }

  if (
    lower.includes("insufficient disk space") ||
    lower.includes("disk full") ||
    lower.includes("enospc")
  ) {
    return {
      code: "INSUFFICIENT_DISK_SPACE",
      category: "DISK_SPACE",
      message: "Insufficient disk space to complete this operation. Please free up storage space and try again.",
      retryable: false,
      safeEntityId,
      operation,
    };
  }

  if (
    lower.includes("permission denied") ||
    lower.includes("access denied") ||
    lower.includes("not writable") ||
    lower.includes("readonly")
  ) {
    return {
      code: "PERMISSION_DENIED",
      category: "PERMISSION",
      message: "Permission denied while accessing application storage or files. Please check filesystem permissions.",
      retryable: false,
      safeEntityId,
      operation,
    };
  }

  if (
    lower.includes("checksum mismatch") ||
    lower.includes("tampered") ||
    lower.includes("corrupt") ||
    lower.includes("malformed") ||
    lower.includes("decryption failed") ||
    lower.includes("authentication failed")
  ) {
    return {
      code: "DATA_CORRUPTION",
      category: "CORRUPTION",
      message: rawMsg.includes("Authentication failed")
        ? "Incorrect password. Please verify your password and try again."
        : "Data integrity verification failed. Storage data or archive may be damaged.",
      retryable: false,
      safeEntityId,
      operation,
    };
  }

  if (
    lower.includes("unsupported") ||
    lower.includes("newer than supported") ||
    lower.includes("schema version")
  ) {
    return {
      code: "UNSUPPORTED_FORMAT",
      category: "UNSUPPORTED_FORMAT",
      message: rawMsg,
      retryable: false,
      safeEntityId,
      operation,
    };
  }

  if (
    lower.includes("not found") ||
    lower.includes("no longer available") ||
    lower.includes("does not exist") ||
    lower.includes("missing")
  ) {
    return {
      code: "NOT_FOUND",
      category: "NOT_FOUND",
      message: rawMsg,
      retryable: false,
      safeEntityId,
      operation,
    };
  }

  if (
    lower.includes("already exists") ||
    lower.includes("collision") ||
    lower.includes("unique constraint") ||
    lower.includes("duplicate")
  ) {
    return {
      code: "RESOURCE_CONFLICT",
      category: "CONFLICT",
      message: rawMsg,
      retryable: false,
      safeEntityId,
      operation,
    };
  }

  if (
    lower.includes("validation") ||
    lower.includes("exceeds the maximum") ||
    lower.includes("cannot exceed") ||
    lower.includes("cannot be empty") ||
    lower.includes("invalid") ||
    lower.includes("too large")
  ) {
    return {
      code: "VALIDATION_FAILED",
      category: "VALIDATION",
      message: rawMsg,
      retryable: false,
      safeEntityId,
      operation,
    };
  }

  // 4. Default Internal Failure
  return {
    code: "INTERNAL_ERROR",
    category: "INTERNAL",
    message: rawMsg,
    retryable: false,
    safeEntityId,
    operation,
  };
}

/**
 * Convenience helper to determine whether an error is transient and can be retried.
 */
export function isRetryableError(error: unknown): boolean {
  return classifyError(error).retryable;
}

/**
 * Convenience helper to detect whether an error was an intentional cancellation (not a crash).
 */
export function isCancellationError(error: unknown): boolean {
  return classifyError(error).category === "CANCELLED";
}
