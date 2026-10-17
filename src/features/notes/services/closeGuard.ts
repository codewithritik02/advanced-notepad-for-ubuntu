export interface PreventableCloseRequest {
  preventDefault: () => void;
}

export interface GuardedCloseOptions {
  event: PreventableCloseRequest;
  waitForPendingSave?: () => Promise<void>;
  savePendingChanges: () => Promise<void>;
  destroyWindow: () => Promise<void>;
  reportFailure: (error: unknown) => void;
  persistenceTimeoutMs?: number;
}

export const DEFAULT_CLOSE_PERSISTENCE_TIMEOUT_MS = 5_000;

export class ClosePersistenceTimeoutError extends Error {
  constructor(timeoutMs: number) {
    super(`Saving changes before close timed out after ${timeoutMs}ms`);
    this.name = "ClosePersistenceTimeoutError";
  }
}

async function waitWithTimeout<T>(operation: Promise<T>, timeoutMs: number): Promise<T> {
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_resolve, reject) => {
    timeoutId = setTimeout(() => reject(new ClosePersistenceTimeoutError(timeoutMs)), timeoutMs);
  });

  try {
    return await Promise.race([operation, timeout]);
  } finally {
    if (timeoutId !== undefined) clearTimeout(timeoutId);
  }
}

/**
 * Keeps the native window open until pending writes have completed successfully.
 * A failed write is surfaced and deliberately leaves the window open for retry.
 */
export async function closeAfterPendingWrites({
  event,
  waitForPendingSave,
  savePendingChanges,
  destroyWindow,
  reportFailure,
  persistenceTimeoutMs = DEFAULT_CLOSE_PERSISTENCE_TIMEOUT_MS,
}: GuardedCloseOptions): Promise<boolean> {
  event.preventDefault();

  try {
    await waitWithTimeout(
      (async () => {
        await waitForPendingSave?.();
        await savePendingChanges();
      })(),
      Math.max(1, persistenceTimeoutMs)
    );
    await destroyWindow();
    return true;
  } catch (error) {
    reportFailure(error);
    return false;
  }
}
