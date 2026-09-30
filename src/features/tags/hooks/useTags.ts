import { useState, useCallback, useEffect } from "react";
import { tagService } from "../services/tagService";
import type { Tag, TagsStatus } from "../types";
import { logger } from "../../../utils/logger";

export interface UseTagsReturn {
  tags: Tag[];
  status: TagsStatus;
  error: string | null;
  loadTags: () => Promise<void>;
  createTag: (name: string) => Promise<Tag>;
  renameTag: (id: string, name: string) => Promise<Tag>;
  deleteTag: (id: string) => Promise<boolean>;
}

/**
 * Hook to manage SQLite-persisted tags state, retrieval, and lifecycle.
 */
export function useTags(isStorageReady = true): UseTagsReturn {
  const [tags, setTags] = useState<Tag[]>([]);
  const [status, setStatus] = useState<TagsStatus>("loading");
  const [error, setError] = useState<string | null>(null);

  const loadTags = useCallback(async () => {
    if (!isStorageReady) return;
    setStatus("loading");
    setError(null);
    try {
      const data = await tagService.list();
      setTags(data);
      setStatus("idle");
    } catch (err) {
      logger.error("Failed to load tags from SQLite", err);
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to load tags");
      setStatus("error");
    }
  }, [isStorageReady]);

  useEffect(() => {
    if (isStorageReady) {
      loadTags();
    }
  }, [isStorageReady, loadTags]);

  const createTag = useCallback(async (name: string) => {
    try {
      const created = await tagService.create(name);
      setTags((prev) => {
        const exists = prev.find((t) => t.id === created.id);
        if (exists) return prev;
        return [...prev, created].sort((a, b) => a.name.localeCompare(b.name));
      });
      return created;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(msg);
    }
  }, []);

  const renameTag = useCallback(async (id: string, name: string) => {
    try {
      const updated = await tagService.rename(id, name);
      setTags((prev) =>
        prev
          .map((t) => (t.id === id ? updated : t))
          .sort((a, b) => a.name.localeCompare(b.name))
      );
      return updated;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(msg);
    }
  }, []);

  const deleteTag = useCallback(async (id: string) => {
    try {
      const success = await tagService.delete(id);
      if (success) {
        setTags((prev) => prev.filter((t) => t.id !== id));
      }
      return success;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(msg);
    }
  }, []);

  return {
    tags,
    status,
    error,
    loadTags,
    createTag,
    renameTag,
    deleteTag,
  };
}

export default useTags;
