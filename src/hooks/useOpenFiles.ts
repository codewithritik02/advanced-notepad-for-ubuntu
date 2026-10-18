import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { storageService } from "../services/storage";
import type { OpenFilesReport } from "../services/storage/openFiles";

/** Sent by the backend when a second launch queued more files. */
export const OPEN_FILES_QUEUED_EVENT = "open-files-queued";

/**
 * Opens files the desktop handed to the app ("Open with Personal Notepad", double click, or a
 * second launch). Files queued at startup are picked up once the app is ready and unlocked; later
 * ones arrive through an event. `onReport` receives what was opened and what failed.
 */
export function useOpenFiles(
  enabled: boolean,
  onReport: (report: OpenFilesReport) => void | Promise<void>,
): void {
  const onReportRef = useRef(onReport);
  onReportRef.current = onReport;

  useEffect(() => {
    if (!enabled) return;
    let disposed = false;
    let running = false;
    let again = false;
    let unlisten: (() => void) | undefined;

    const run = async () => {
      // Never run two at once: a second request while busy just asks for one more pass.
      if (running) {
        again = true;
        return;
      }
      running = true;
      try {
        do {
          again = false;
          const report = await storageService.openFiles.processPending();
          if (!disposed && (report.opened.length > 0 || report.failed.length > 0)) {
            await onReportRef.current(report);
          }
        } while (again && !disposed);
      } catch {
        // Locked or the backend is not ready: the files stay queued for the next attempt
      } finally {
        running = false;
      }
    };

    void run();
    listen(OPEN_FILES_QUEUED_EVENT, () => void run())
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [enabled]);
}

export default useOpenFiles;
