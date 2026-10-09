import { useState, useCallback, useRef, useEffect } from "react";

export interface NoteNavigationHistoryHook {
  canGoBack: boolean;
  canGoForward: boolean;
  currentNoteId: string | null;
  historyStack: string[];
  currentIndex: number;
  recordNavigation: (targetNoteId: string, currentNoteId?: string | null) => void;
  goBack: () => string | null;
  goForward: () => string | null;
  removeNoteFromHistory: (noteId: string) => void;
  clearHistory: () => void;
}

const MAX_HISTORY_LENGTH = 100;

/**
 * In-memory, session-only navigation history for note-to-note transitions (Phase 9, Task 12).
 *
 * Supports predictable back/forward transitions across stable note IDs,
 * prevents duplicate consecutive entries, clears forward history upon branching,
 * and purges deleted note IDs to maintain lifecycle validity.
 */
export function useNoteNavigationHistory(
  initialNoteId?: string | null
): NoteNavigationHistoryHook {
  const [historyStack, setHistoryStack] = useState<string[]>(() =>
    initialNoteId ? [initialNoteId] : []
  );
  const [currentIndex, setCurrentIndex] = useState<number>(() =>
    initialNoteId ? 0 : -1
  );

  // Sync ref for immediate synchronous reads in callbacks
  const historyRef = useRef<{ stack: string[]; index: number }>({
    stack: initialNoteId ? [initialNoteId] : [],
    index: initialNoteId ? 0 : -1,
  });

  useEffect(() => {
    historyRef.current = { stack: historyStack, index: currentIndex };
  }, [historyStack, currentIndex]);

  // Seed history if empty and an initial note ID is selected
  useEffect(() => {
    if (initialNoteId && historyRef.current.stack.length === 0) {
      setHistoryStack([initialNoteId]);
      setCurrentIndex(0);
      historyRef.current = { stack: [initialNoteId], index: 0 };
    }
  }, [initialNoteId]);

  const canGoBack = currentIndex > 0;
  const canGoForward =
    currentIndex >= 0 && currentIndex < historyStack.length - 1;
  const currentNoteId =
    currentIndex >= 0 && currentIndex < historyStack.length
      ? historyStack[currentIndex]
      : null;

  /**
   * Records a successful navigation to targetNoteId.
   * If current note is not in stack, seeds it before appending targetNoteId.
   * Clears any forward history after currentIndex.
   */
  const recordNavigation = useCallback(
    (targetNoteId: string, sourceNoteId?: string | null) => {
      if (!targetNoteId) return;

      const { stack, index } = historyRef.current;

      // Avoid creating duplicate consecutive history entries
      if (index >= 0 && stack[index] === targetNoteId) {
        return;
      }

      let newStack: string[];
      let newIndex: number;

      if (stack.length === 0) {
        if (sourceNoteId && sourceNoteId !== targetNoteId) {
          newStack = [sourceNoteId, targetNoteId];
          newIndex = 1;
        } else {
          newStack = [targetNoteId];
          newIndex = 0;
        }
      } else {
        // Truncate any forward history
        const baseStack = stack.slice(0, index + 1);

        // If the base stack's top isn't the source note, ensure source note is recorded
        if (
          sourceNoteId &&
          sourceNoteId !== targetNoteId &&
          (baseStack.length === 0 || baseStack[baseStack.length - 1] !== sourceNoteId)
        ) {
          baseStack.push(sourceNoteId);
        }

        newStack = [...baseStack, targetNoteId];

        // Cap maximum history depth to prevent unbounded memory growth
        if (newStack.length > MAX_HISTORY_LENGTH) {
          newStack = newStack.slice(newStack.length - MAX_HISTORY_LENGTH);
        }

        newIndex = newStack.length - 1;
      }

      historyRef.current = { stack: newStack, index: newIndex };
      setHistoryStack(newStack);
      setCurrentIndex(newIndex);
    },
    []
  );

  /**
   * Steps back one position in history stack and returns the target note ID.
   */
  const goBack = useCallback((): string | null => {
    const { stack, index } = historyRef.current;
    if (index <= 0) return null;

    const newIndex = index - 1;
    const destinationNoteId = stack[newIndex];

    historyRef.current = { stack, index: newIndex };
    setCurrentIndex(newIndex);

    return destinationNoteId;
  }, []);

  /**
   * Steps forward one position in history stack and returns the target note ID.
   */
  const goForward = useCallback((): string | null => {
    const { stack, index } = historyRef.current;
    if (index < 0 || index >= stack.length - 1) return null;

    const newIndex = index + 1;
    const destinationNoteId = stack[newIndex];

    historyRef.current = { stack, index: newIndex };
    setCurrentIndex(newIndex);

    return destinationNoteId;
  }, []);

  /**
   * Removes all occurrences of a note ID from history (e.g. when permanently deleted)
   * and adjusts currentIndex safely.
   */
  const removeNoteFromHistory = useCallback((noteId: string) => {
    const { stack, index } = historyRef.current;
    if (!stack.includes(noteId)) return;

    let targetIndex = index;
    const newStack: string[] = [];

    for (let i = 0; i < stack.length; i++) {
      if (stack[i] === noteId) {
        if (i < index) {
          targetIndex--;
        }
      } else {
        newStack.push(stack[i]);
      }
    }

    let newIndex = targetIndex;
    if (newStack.length === 0) {
      newIndex = -1;
    } else if (newIndex >= newStack.length) {
      newIndex = newStack.length - 1;
    } else if (newIndex < 0) {
      newIndex = 0;
    }

    historyRef.current = { stack: newStack, index: newIndex };
    setHistoryStack(newStack);
    setCurrentIndex(newIndex);
  }, []);

  const clearHistory = useCallback(() => {
    historyRef.current = { stack: [], index: -1 };
    setHistoryStack([]);
    setCurrentIndex(-1);
  }, []);

  return {
    canGoBack,
    canGoForward,
    currentNoteId,
    historyStack,
    currentIndex,
    recordNavigation,
    goBack,
    goForward,
    removeNoteFromHistory,
    clearHistory,
  };
}

export default useNoteNavigationHistory;
