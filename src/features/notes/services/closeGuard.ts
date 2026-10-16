export interface PreventableCloseRequest {
  preventDefault: () => void;
}

export interface GuardedCloseOptions {
  event: PreventableCloseRequest;
  waitForPendingSave?: () => Promise<void>;
  savePendingChanges: () => Promise<void>;
  destroyWindow: () => Promise<void>;
  reportFailure: (error: unknown) => void;
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
}: GuardedCloseOptions): Promise<boolean> {
  event.preventDefault();

  try {
    await waitForPendingSave?.();
    await savePendingChanges();
    await destroyWindow();
    return true;
  } catch (error) {
    reportFailure(error);
    return false;
  }
}
