import { useState, useCallback, useEffect } from "react";
import { notebookService } from "../services/notebookService";
import type { Notebook, NotebooksStatus } from "../types";
import { logger } from "../../../utils/logger";

export interface UseNotebooksReturn {
  notebooks: Notebook[];
  status: NotebooksStatus;
  error: string | null;
  loadNotebooks: () => Promise<void>;
  createNotebook: (name: string, parentId?: string | null) => Promise<Notebook>;
  renameNotebook: (id: string, name: string) => Promise<Notebook>;
  deleteNotebook: (id: string) => Promise<boolean>;
}

/**
 * Hook to manage SQLite-persisted notebooks state and lifecycle.
 */
export function useNotebooks(isStorageReady = true): UseNotebooksReturn {
  const [notebooks, setNotebooks] = useState<Notebook[]>([]);
  const [status, setStatus] = useState<NotebooksStatus>("loading");
  const [error, setError] = useState<string | null>(null);

  const loadNotebooks = useCallback(async () => {
    if (!isStorageReady) return;
    setStatus("loading");
    setError(null);
    try {
      const data = await notebookService.list();
      setNotebooks(data);
      setStatus("idle");
    } catch (err) {
      logger.notebook.databaseFailed("list notebooks", err);
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to load notebooks");
      setStatus("error");
    }
  }, [isStorageReady]);

  useEffect(() => {
    if (isStorageReady) {
      loadNotebooks();
    }
  }, [isStorageReady, loadNotebooks]);

  const createNotebook = useCallback(
    async (name: string, parentId?: string | null) => {
      try {
        const created = await notebookService.create(name, parentId);
        setNotebooks((prev) => [...prev, created]);
        return created;
      } catch (err) {
        logger.notebook.creationFailed(name, err);
        const msg = err instanceof Error ? err.message : String(err);
        throw new Error(msg);
      }
    },
    []
  );

  const renameNotebook = useCallback(async (id: string, name: string) => {
    try {
      const updated = await notebookService.rename(id, name);
      setNotebooks((prev) =>
        prev.map((nb) => (nb.id === id ? updated : nb))
      );
      return updated;
    } catch (err) {
      logger.error("Notebook rename failed", err, { notebookId: id });
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(msg);
    }
  }, []);

  const deleteNotebook = useCallback(async (id: string) => {
    try {
      const success = await notebookService.delete(id);
      if (success) {
        setNotebooks((prev) => prev.filter((nb) => nb.id !== id));
      }
      return success;
    } catch (err) {
      logger.notebook.deletionFailed(id, err);
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(msg);
    }
  }, []);

  return {
    notebooks,
    status,
    error,
    loadNotebooks,
    createNotebook,
    renameNotebook,
    deleteNotebook,
  };
}
