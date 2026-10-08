import { useState, useEffect, useRef, useCallback } from "react";
import type { Attachment } from "../../../services/storage/types.ts";
import { attachmentsStorage } from "../../../services/storage/attachments.ts";
import { pickAttachmentFiles } from "../services/fileDialog.ts";
import {
  executeAddAttachmentWorkflow,
  mapToAttachmentErrorState,
  type AddAttachmentProgress,
} from "../services/attachmentWorkflow.ts";
import type { AttachmentStatus, AttachmentErrorState } from "../types.ts";

export interface UseAttachmentsResult {
  attachments: Attachment[];
  status: AttachmentStatus;
  error: AttachmentErrorState | null;
  progress: AddAttachmentProgress | null;
  successMessage: string | null;
  isBusy: boolean;
  addFiles: (filePaths?: string[]) => Promise<Attachment[]>;
  pickAndAddFiles: () => Promise<Attachment[]>;
  removeAttachment: (id: string) => Promise<boolean>;
  openAttachment: (id: string) => Promise<void>;
  reload: () => Promise<void>;
  clearError: () => void;
  clearSuccess: () => void;
}

/**
 * Custom React hook for robust, local-first attachment state management (Phase 7, Tasks 10-18).
 *
 * Implements:
 * - Selection race protection: Guarantees that attachments are always linked to the note ID
 *   active when the operation was initiated, even if the user switches notes during file copy.
 * - Partial multi-file failure isolation: One failed file never corrupts successful attachments.
 * - Progress tracking during multi-file operations.
 * - Clean error and success mapping with accessible feedback.
 */
export function useAttachments(noteId: string | null): UseAttachmentsResult {
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [status, setStatus] = useState<AttachmentStatus>("idle");
  const [error, setError] = useState<AttachmentErrorState | null>(null);
  const [progress, setProgress] = useState<AddAttachmentProgress | null>(null);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);
  const successTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Keep a reference to the active noteId to detect concurrent note switches
  const currentNoteIdRef = useRef<string | null>(noteId);
  const prevNoteIdRef = useRef<string | null>(noteId);

  // Request sequencing counter to avoid stale list loads
  const listSequenceRef = useRef<number>(0);

  const clearSuccess = useCallback(() => {
    if (successTimerRef.current) {
      clearTimeout(successTimerRef.current);
      successTimerRef.current = null;
    }
    setSuccessMessage(null);
  }, []);

  const showSuccess = useCallback((msg: string) => {
    if (successTimerRef.current) {
      clearTimeout(successTimerRef.current);
    }
    setSuccessMessage(msg);
    successTimerRef.current = setTimeout(() => {
      setSuccessMessage(null);
      successTimerRef.current = null;
    }, 3500);
  }, []);

  const loadAttachments = useCallback(async () => {
    if (!noteId) {
      setAttachments([]);
      setStatus("idle");
      setError(null);
      setProgress(null);
      return;
    }

    const currentSequence = ++listSequenceRef.current;
    setStatus("loading");
    setError(null);

    try {
      const items = await attachmentsStorage.listAttachments(noteId);
      // Check for stale response
      if (currentSequence === listSequenceRef.current && currentNoteIdRef.current === noteId) {
        setAttachments(items);
        setStatus("idle");
      }
    } catch (err) {
      if (currentSequence === listSequenceRef.current && currentNoteIdRef.current === noteId) {
        setStatus("error");
        setError(mapToAttachmentErrorState(err, "Failed to load attachments for this note."));
      }
    }
  }, [noteId]);

  // Synchronize state immediately on note switch
  useEffect(() => {
    currentNoteIdRef.current = noteId;
    if (prevNoteIdRef.current !== noteId) {
      prevNoteIdRef.current = noteId;
      setAttachments([]);
      setError(null);
      setProgress(null);
      clearSuccess();
    }
    loadAttachments();
  }, [noteId, loadAttachments, clearSuccess]);

  // Teardown timer on unmount
  useEffect(() => {
    return () => {
      if (successTimerRef.current) {
        clearTimeout(successTimerRef.current);
      }
    };
  }, []);

  const addFiles = useCallback(
    async (overridePaths?: string[]): Promise<Attachment[]> => {
      // 1. Resolve and anchor target note ID at invocation (Selection Race Protection)
      const targetNoteId = currentNoteIdRef.current;
      if (!targetNoteId) {
        setError({
          kind: "InvalidRequest",
          message: "Please select an active note before attaching files.",
        });
        return [];
      }

      // 2. Resolve files to attach
      let pathsToAttach = overridePaths;
      if (!pathsToAttach) {
        pathsToAttach = await pickAttachmentFiles({ multiple: true });
      }

      if (!pathsToAttach || pathsToAttach.length === 0) {
        return [];
      }

      setStatus("adding");
      setError(null);
      clearSuccess();
      setProgress({
        current: 0,
        total: pathsToAttach.length,
        currentFileName: "",
      });

      try {
        const result = await executeAddAttachmentWorkflow(
          targetNoteId,
          pathsToAttach,
          (prog) => setProgress(prog)
        );

        // 3. Selection race check: only update local React list if user is still viewing target note
        if (currentNoteIdRef.current === targetNoteId) {
          if (result.successful.length > 0) {
            setAttachments((prev) => [...prev, ...result.successful]);
            showSuccess(
              `Attached ${result.successful.length} file${
                result.successful.length === 1 ? "" : "s"
              } successfully.`
            );
          }
        }

        // 4. Handle partial or total failure reporting
        if (result.failed.length > 0) {
          const failureSummary = result.failed
            .map((f) => `${f.path.split(/[/\\]/).pop()}: ${f.error}`)
            .join("; ");

          setError({
            kind: "FileOperationFailed",
            message:
              result.successful.length > 0
                ? `Some files could not be attached: ${failureSummary}`
                : failureSummary,
          });
          setStatus(result.successful.length > 0 ? "idle" : "error");
        } else {
          setStatus("idle");
        }

        return result.successful;
      } catch (err) {
        setStatus("error");
        setError(mapToAttachmentErrorState(err, "Failed to add attachment."));
        return [];
      } finally {
        setProgress(null);
      }
    },
    [clearSuccess, showSuccess]
  );

  const pickAndAddFiles = useCallback(async (): Promise<Attachment[]> => {
    return await addFiles();
  }, [addFiles]);

  const removeAttachment = useCallback(
    async (id: string): Promise<boolean> => {
      const targetNoteId = currentNoteIdRef.current;
      setStatus("removing");
      setError(null);
      clearSuccess();

      try {
        await attachmentsStorage.removeAttachment(id);
        if (currentNoteIdRef.current === targetNoteId) {
          setAttachments((prev) => prev.filter((a) => a.id !== id));
          setStatus("idle");
          showSuccess("Attachment removed successfully.");
        }
        return true;
      } catch (err) {
        if (currentNoteIdRef.current === targetNoteId) {
          setStatus("error");
          setError(mapToAttachmentErrorState(err, "Failed to remove attachment."));
        }
        return false;
      }
    },
    [clearSuccess, showSuccess]
  );

  const openAttachment = useCallback(
    async (id: string): Promise<void> => {
      const targetNoteId = currentNoteIdRef.current;
      setStatus("opening");
      setError(null);

      try {
        await attachmentsStorage.openAttachment(id);
        if (currentNoteIdRef.current === targetNoteId) {
          setStatus("idle");
        }
      } catch (err) {
        if (currentNoteIdRef.current === targetNoteId) {
          setStatus("error");
          setError(mapToAttachmentErrorState(err, "Failed to open attachment with default application."));
        }
      }
    },
    []
  );

  const clearError = useCallback(() => {
    setError(null);
    if (status === "error") {
      setStatus("idle");
    }
  }, [status]);

  return {
    attachments,
    status,
    error,
    progress,
    successMessage,
    isBusy: status === "loading" || status === "adding" || status === "removing" || status === "opening",
    addFiles,
    pickAndAddFiles,
    removeAttachment,
    openAttachment,
    reload: loadAttachments,
    clearError,
    clearSuccess,
  };
}

export default useAttachments;
