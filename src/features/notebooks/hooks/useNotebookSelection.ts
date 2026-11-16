import { useState, useCallback, useEffect } from "react";
import type { NoteLocation } from "../types";
import { locationToNavId } from "../types";
export { locationToNavId };

export interface UseNotebookSelectionReturn {
  /** The authoritative navigation location (Task 42). */
  location: NoteLocation;
  selectedNotebookId: string | null;
  selectedTagId: string | null;
  selectLocation: (loc: NoteLocation) => void;
  selectNotebook: (notebookId: string, allNotebooks?: Array<{ id: string; parent_id: string | null }>) => void;
  selectAllNotes: () => void;
  selectUnfiled: () => void;
  selectFavorites: () => void;
  selectTag: (tagId: string) => void;
  selectTrash: () => void;
  // Expand / collapse UI state (Task 9, remembered across restarts)
  expandedNotebookIds: Set<string>;
  toggleExpand: (notebookId: string) => void;
  expandNotebook: (notebookId: string) => void;
  collapseNotebook: (notebookId: string) => void;
  isExpanded: (notebookId: string) => boolean;
}

/**
 * Hook to manage active notebook navigation state and tree expansion UI state.
 * Complies with Task 9 and Task 10:
 * - Expansion state is purely React UI state (not stored in SQLite).
 * - Navigation state uses the explicit NoteLocation model to avoid ambiguous null states.
 */
const EXPANDED_STORAGE_KEY = "notepad.expandedNotebookIds";

/** Reads the remembered expanded folders; any storage/parse problem simply means "none". */
function loadExpandedIds(): Set<string> {
  try {
    const raw = localStorage.getItem(EXPANDED_STORAGE_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return new Set(
      Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string") : []
    );
  } catch {
    return new Set<string>();
  }
}

export function useNotebookSelection(
  initialLocation: NoteLocation = { type: "all" }
): UseNotebookSelectionReturn {
  const [location, setLocation] = useState<NoteLocation>(initialLocation);
  // Which folders are open is remembered across restarts (UI preference only, never in SQLite).
  const [expandedNotebookIds, setExpandedNotebookIds] = useState<Set<string>>(loadExpandedIds);

  useEffect(() => {
    try {
      localStorage.setItem(EXPANDED_STORAGE_KEY, JSON.stringify([...expandedNotebookIds]));
    } catch {
      // Storage unavailable (private mode / quota): expansion just won't persist.
    }
  }, [expandedNotebookIds]);

  const selectedNotebookId =
    location.type === "notebook" ? location.notebookId : null;

  const selectedTagId =
    location.type === "tag" ? location.tagId : null;

  const selectLocation = useCallback((loc: NoteLocation) => {
    setLocation(loc);
  }, []);

  const selectNotebook = useCallback((notebookId: string, allNotebooks?: Array<{ id: string; parent_id: string | null }>) => {
    setLocation({ type: "notebook", notebookId });
    if (allNotebooks && allNotebooks.length > 0) {
      const ancestorsToExpand = new Set<string>();
      let current = allNotebooks.find((nb) => nb.id === notebookId);
      while (current && current.parent_id) {
        ancestorsToExpand.add(current.parent_id);
        current = allNotebooks.find((nb) => nb.id === current!.parent_id);
      }
      if (ancestorsToExpand.size > 0) {
        setExpandedNotebookIds((prev) => {
          const next = new Set(prev);
          for (const pid of ancestorsToExpand) {
            next.add(pid);
          }
          return next;
        });
      }
    }
  }, []);

  const selectAllNotes = useCallback(() => {
    setLocation({ type: "all" });
  }, []);

  const selectUnfiled = useCallback(() => {
    setLocation({ type: "unfiled" });
  }, []);

  const selectFavorites = useCallback(() => {
    setLocation({ type: "favorites" });
  }, []);

  const selectTag = useCallback((tagId: string) => {
    setLocation({ type: "tag", tagId });
  }, []);

  const selectTrash = useCallback(() => {
    setLocation({ type: "trash" });
  }, []);

  // Expand / Collapse toggling (Task 9)
  const toggleExpand = useCallback((notebookId: string) => {
    setExpandedNotebookIds((prev) => {
      const next = new Set(prev);
      if (next.has(notebookId)) {
        next.delete(notebookId);
      } else {
        next.add(notebookId);
      }
      return next;
    });
  }, []);

  const expandNotebook = useCallback((notebookId: string) => {
    setExpandedNotebookIds((prev) => {
      if (prev.has(notebookId)) return prev;
      const next = new Set(prev);
      next.add(notebookId);
      return next;
    });
  }, []);

  const collapseNotebook = useCallback((notebookId: string) => {
    setExpandedNotebookIds((prev) => {
      if (!prev.has(notebookId)) return prev;
      const next = new Set(prev);
      next.delete(notebookId);
      return next;
    });
  }, []);

  const isExpanded = useCallback(
    (notebookId: string) => {
      return expandedNotebookIds.has(notebookId);
    },
    [expandedNotebookIds]
  );

  return {
    location,
    selectedNotebookId,
    selectedTagId,
    selectLocation,
    selectNotebook,
    selectAllNotes,
    selectUnfiled,
    selectFavorites,
    selectTag,
    selectTrash,
    expandedNotebookIds,
    toggleExpand,
    expandNotebook,
    collapseNotebook,
    isExpanded,
  };
}

export default useNotebookSelection;
