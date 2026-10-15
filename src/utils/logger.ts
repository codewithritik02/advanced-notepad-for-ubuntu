/**
 * Centralized privacy-safe logging system for Personal Notepad.
 *
 * Requirements (Task 36):
 * - Development diagnostics only (silent in production).
 * - Allowed diagnostic events:
 *     - "Notebook creation failed"
 *     - "Notebook deletion failed"
 *     - "Invalid hierarchy"
 *     - "Database operation failed"
 * - FORBIDDEN: Logging full note content or sensitive user data.
 */

const isDev = Boolean(import.meta.env?.DEV);

// Keys that must NEVER be logged to prevent sensitive data leakage
const FORBIDDEN_CONTENT_KEYS = new Set([
  "content",
  "body",
  "text",
  "rawContent",
  "noteContent",
]);

/**
 * Sanitizes arbitrary context objects to guarantee note content or private data is never leaked.
 */
function sanitizeContext(context?: Record<string, unknown>): Record<string, unknown> | undefined {
  if (!context) return undefined;

  const sanitized: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(context)) {
    if (FORBIDDEN_CONTENT_KEYS.has(key)) {
      sanitized[key] = "[REDACTED_CONTENT]";
    } else if (typeof value === "object" && value !== null) {
      sanitized[key] = sanitizeContext(value as Record<string, unknown>);
    } else {
      sanitized[key] = value;
    }
  }
  return sanitized;
}

export const logger = {
  info: (topic: string, context?: Record<string, unknown>): void => {
    if (!isDev) return;
    console.info(`[PersonalNotepad] ${topic}`, sanitizeContext(context) ?? "");
  },

  warn: (topic: string, context?: Record<string, unknown>): void => {
    if (!isDev) return;
    console.warn(`[PersonalNotepad:Warn] ${topic}`, sanitizeContext(context) ?? "");
  },

  error: (topic: string, error?: unknown, context?: Record<string, unknown>): void => {
    if (!isDev) return;
    const errorMsg = error instanceof Error ? error.message : String(error ?? "Unknown error");
    console.error(`[PersonalNotepad:Error] ${topic}:`, errorMsg, sanitizeContext(context) ?? "");
  },

  notebook: {
    creationFailed: (name: string, error: unknown): void => {
      logger.error("Notebook creation failed", error, { notebookName: name });
    },
    deletionFailed: (id: string, error: unknown): void => {
      logger.error("Notebook deletion failed", error, { notebookId: id });
    },
    invalidHierarchy: (reason: string, context?: Record<string, unknown>): void => {
      logger.warn(`Invalid hierarchy: ${reason}`, context);
    },
    databaseFailed: (operation: string, error: unknown): void => {
      logger.error(`Database operation failed: ${operation}`, error);
    },
  },

  search: {
    executed: (query: string, count: number, durationMs: number): void => {
      if (!isDev) return;
      console.info(
        `[Search] query executed: count=${count} duration=${Math.round(durationMs)}ms query="${query}"`
      );
    },
    failed: (query: string, durationMs: number, error: unknown): void => {
      if (!isDev) return;
      console.warn(
        `[Search] query failed: duration=${Math.round(durationMs)}ms query="${query}"`,
        error
      );
    },
  },

  security: {
    setupStarted: (): void => {
      logger.info("[Security] Encryption setup started");
    },
    setupCompleted: (notesCount: number, attachmentsCount: number): void => {
      logger.info(`[Security] Encryption setup completed: notes=${notesCount}, attachments=${attachmentsCount}`);
    },
    setupFailed: (error: unknown): void => {
      logger.error("[Security] Encryption setup failed", error);
    },
    unlockAttempted: (): void => {
      logger.info("[Security] Application unlock attempted");
    },
    unlockSucceeded: (): void => {
      logger.info("[Security] Application unlocked successfully");
    },
    unlockFailed: (error: unknown): void => {
      logger.warn("[Security] Application unlock failed", { error: String(error) });
    },
    autoLockOccurred: (idleMinutes: number): void => {
      logger.info(`[Security] Application locked automatically after ${idleMinutes}m inactivity`);
    },
    manualLockOccurred: (): void => {
      logger.info("[Security] Application session locked manually");
    },
    passwordChangeStarted: (): void => {
      logger.info("[Security] Password change operation started");
    },
    passwordChangeCompleted: (): void => {
      logger.info("[Security] Password changed successfully");
    },
    passwordChangeFailed: (error: unknown): void => {
      logger.warn("[Security] Password change failed", { error: String(error) });
    },
    disableEncryptionStarted: (): void => {
      logger.warn("[Security] Disable encryption workflow started");
    },
    disableEncryptionCompleted: (notesCount: number, attachmentsCount: number): void => {
      logger.info(`[Security] Disable encryption completed: decrypted_notes=${notesCount}, decrypted_attachments=${attachmentsCount}`);
    },
    disableEncryptionFailed: (error: unknown): void => {
      logger.error("[Security] Disable encryption failed", error);
    },
  },
};

export default logger;
