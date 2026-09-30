import { useEffect, useCallback } from "react";

/**
 * useSearchKeyboardNav — Task 45: Search Result Keyboard Navigation
 *
 * Handles ArrowDown / ArrowUp within the search results list:
 *   ↓  Arrow Down  → select next result
 *   ↑  Arrow Up    → select previous result
 *
 * Escape and Enter are deliberately NOT handled here:
 *   - Escape is handled by the global useKeyboardShortcuts hook (App.tsx) and
 *     by TopBar's onKeyDown on the search input. Both correctly call clearSearch().
 *   - Enter/Space on a focused NoteCard is handled by NoteCard's own onKeyDown,
 *     which calls onSelect(noteId). No duplication needed.
 *
 * Editor non-interference guarantee:
 *   Arrow keys inside the editor (textarea, input, contenteditable) must scroll
 *   text/move the cursor as normal. This hook checks document.activeElement and
 *   skips if it is inside .app-editor-pane, a textarea, an input, or a
 *   contenteditable element.
 *
 * Scroll-into-view:
 *   After moving the selection, the newly selected NoteCard is scrolled into view
 *   by querying [data-note-id="<id>"] on the DOM. NoteCard must render this
 *   attribute for this to work.
 */

export interface UseSearchKeyboardNavOptions {
  /** Whether search mode is currently active. Navigation only runs when true. */
  isSearchActive: boolean;
  /** Ordered array of note IDs in the current scoped search results. */
  resultNoteIds: string[];
  /** Currently selected note ID. */
  selectedNoteId: string | null;
  /** Called when keyboard navigation changes the selection. */
  onSelectNote: (noteId: string) => void;
}

/** CSS selector that matches any editable/interactive element inside the editor pane. */
const EDITOR_PANE_SELECTOR = ".app-editor-pane";

/**
 * Returns true if the currently focused element is inside the editor pane,
 * a bare textarea, a bare input, or a contenteditable — any context where
 * arrow keys have native meaning that must not be overridden.
 */
function isEditorFocused(): boolean {
  const el = document.activeElement;
  if (!el) return false;
  // Inside the editor pane (title input, content textarea, toolbar buttons)
  if (el.closest(EDITOR_PANE_SELECTOR)) return true;
  // Standalone textarea or input outside the editor (modals, dialogs)
  const tag = el.tagName.toLowerCase();
  if (tag === "textarea" || tag === "input") {
    // Allow TopBar search input to navigate results with ArrowDown / ArrowUp
    if (
      el.classList.contains("search-input") ||
      el.closest(".topbar-search-container")
    ) {
      return false;
    }
    return true;
  }
  // ContentEditable
  if ((el as HTMLElement).isContentEditable) return true;
  return false;
}

export function useSearchKeyboardNav({
  isSearchActive,
  resultNoteIds,
  selectedNoteId,
  onSelectNote,
}: UseSearchKeyboardNavOptions): void {
  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      // Only active when search results are showing
      if (!isSearchActive || resultNoteIds.length === 0) return;

      const key = event.key;
      if (key !== "ArrowDown" && key !== "ArrowUp") return;

      // Do not interfere with editor keyboard behavior (Task 45 requirement)
      if (isEditorFocused()) return;

      event.preventDefault();

      const currentIndex = selectedNoteId
        ? resultNoteIds.indexOf(selectedNoteId)
        : -1;

      let nextIndex: number;
      if (key === "ArrowDown") {
        // If nothing is selected or selection is at the end, wrap to first
        nextIndex =
          currentIndex < 0 || currentIndex >= resultNoteIds.length - 1
            ? 0
            : currentIndex + 1;
      } else {
        // ArrowUp: if nothing selected or at start, wrap to last
        nextIndex =
          currentIndex <= 0
            ? resultNoteIds.length - 1
            : currentIndex - 1;
      }

      const nextId = resultNoteIds[nextIndex];
      if (!nextId) return;

      onSelectNote(nextId);

      // Scroll the newly selected card into view and focus it (Task 46 accessibility)
      // NoteCard renders data-note-id={note.id} so we can query it here.
      requestAnimationFrame(() => {
        const card = document.querySelector<HTMLElement>(
          `[data-note-id="${CSS.escape(nextId)}"]`
        );
        card?.scrollIntoView({ block: "nearest", behavior: "smooth" });
        card?.focus({ preventScroll: true });
      });
    },
    [isSearchActive, resultNoteIds, selectedNoteId, onSelectNote]
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);
}

export default useSearchKeyboardNav;
