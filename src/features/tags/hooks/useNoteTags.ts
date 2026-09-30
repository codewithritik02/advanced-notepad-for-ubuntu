import { useState, useEffect, useCallback, useRef } from "react";
import { tagService } from "../services/tagService";
import type { Tag, TagsStatus } from "../types";
import { logger } from "../../../utils/logger";

export interface UseNoteTagsReturn {
  noteTags: Tag[];
  status: TagsStatus;
  error: string | null;
  loadNoteTags: () => Promise<void>;
  addTag: (tagId: string) => Promise<void>;
  removeTag: (tagId: string) => Promise<void>;
  setTags: (tagIds: string[]) => Promise<Tag[]>;
}

/**
 * Hook to manage tags for the currently selected note.
 * Automatically clears tags immediately on note switch to prevent stale tag UI flash.
 * Uses request synchronization to discard out-of-order responses.
 */
export function useNoteTags(noteId: string | null): UseNoteTagsReturn {
  const [noteTags, setNoteTags] = useState<Tag[]>([]);
  const [status, setStatus] = useState<TagsStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  // Active noteId ref to prevent stale response race conditions
  const currentNoteIdRef = useRef<string | null>(noteId);
  currentNoteIdRef.current = noteId;

  // Active noteTags ref to prevent duplicate assignments without extra re-renders
  const noteTagsRef = useRef<Tag[]>(noteTags);
  noteTagsRef.current = noteTags;

  const loadNoteTags = useCallback(async () => {
    const targetId = currentNoteIdRef.current;
    if (!targetId) {
      setNoteTags([]);
      setStatus("idle");
      return;
    }

    setStatus("loading");
    setError(null);
    try {
      const data = await tagService.getForNote(targetId);
      // Guard against race conditions if user switched note while fetching
      if (currentNoteIdRef.current === targetId) {
        setNoteTags(data);
        setStatus("idle");
      }
    } catch (err) {
      if (currentNoteIdRef.current === targetId) {
        logger.error("Failed to load tags for note", err, { noteId: targetId });
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg || "Failed to load note tags");
        setStatus("error");
      }
    }
  }, []);

  // When noteId changes, immediately clear tags to avoid stale UI flash
  useEffect(() => {
    setNoteTags([]);
    setError(null);
    if (noteId) {
      loadNoteTags();
    } else {
      setStatus("idle");
    }
  }, [noteId, loadNoteTags]);

  const addTag = useCallback(
    async (tagId: string) => {
      const targetId = currentNoteIdRef.current;
      if (!targetId) return;

      // Defensive check: prevent duplicate assignment calls (Task 37)
      if (noteTagsRef.current.some((t) => t.id === tagId)) {
        return;
      }

      try {
        await tagService.addToNote(targetId, tagId);
        // Refresh tags for the current note
        if (currentNoteIdRef.current === targetId) {
          const updated = await tagService.getForNote(targetId);
          if (currentNoteIdRef.current === targetId) {
            setNoteTags(updated);
          }
        }
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        throw new Error(msg);
      }
    },
    []
  );

  const removeTag = useCallback(
    async (tagId: string) => {
      const targetId = currentNoteIdRef.current;
      if (!targetId) return;

      try {
        await tagService.removeFromNote(targetId, tagId);
        if (currentNoteIdRef.current === targetId) {
          setNoteTags((prev) => prev.filter((t) => t.id !== tagId));
        }
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        throw new Error(msg);
      }
    },
    []
  );

  const setTags = useCallback(
    async (tagIds: string[]) => {
      const targetId = currentNoteIdRef.current;
      if (!targetId) return [];

      try {
        const updated = await tagService.setTagsForNote(targetId, tagIds);
        if (currentNoteIdRef.current === targetId) {
          setNoteTags(updated);
        }
        return updated;
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        throw new Error(msg);
      }
    },
    []
  );

  return {
    noteTags,
    status,
    error,
    loadNoteTags,
    addTag,
    removeTag,
    setTags,
  };
}

export default useNoteTags;
