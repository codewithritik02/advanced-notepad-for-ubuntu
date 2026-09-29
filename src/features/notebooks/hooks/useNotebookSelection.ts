import { useState, useCallback } from "react";
import type { NoteLocation } from "../types";

export interface UseNotebookSelectionReturn {
  location: NoteLocation;
  selectedNotebookId: string | null;
  selectLocation: (loc: NoteLocation) => void;
  selectNotebook: (notebookId: string, allNotebooks?: Array<{ id: string; parent_id: string | null }>) => void;
  selectAllNotes: () => void;
  selectUnfiled: () => void;
  // Expand / collapse UI state (Task 9)
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
export function useNotebookSelection(
  initialLocation: NoteLocation = { type: "all" }
): UseNotebookSelectionReturn {
  const [location, setLocation] = useState<NoteLocation>(initialLocation);
  const [expandedNotebookIds, setExpandedNotebookIds] = useState<Set<string>>(
    () => new Set<string>()
  );

  const selectedNotebookId =
    location.type === "notebook" ? location.notebookId : null;

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
    selectLocation,
    selectNotebook,
    selectAllNotes,
    selectUnfiled,
    expandedNotebookIds,
    toggleExpand,
    expandNotebook,
    collapseNotebook,
    isExpanded,
  };
}

export default useNotebookSelection;
