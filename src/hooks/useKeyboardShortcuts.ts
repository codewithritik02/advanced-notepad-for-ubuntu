import { useEffect } from "react";

export interface KeyboardShortcutHandlers {
  enabled?: boolean;
  onNewNote?: () => void;
  onFocusSearch?: () => void;
  onOpenSettings?: () => void;
  onEscape?: () => void;
  onSaveNote?: () => void;
  onDeleteNote?: () => void;
  onNavigateBack?: () => void;
  onNavigateForward?: () => void;
  onLockApp?: () => void;
  onZoomIn?: () => void;
  onZoomOut?: () => void;
  onZoomReset?: () => void;
}

/**
 * Centralized keyboard shortcuts listener for Personal Notepad.
 *
 * Handles cross-platform desktop modifier keys (Ctrl on Linux/Windows, Cmd on macOS)
 * without scattering listeners across individual components.
 */
export function useKeyboardShortcuts({
  enabled = true,
  onNewNote,
  onFocusSearch,
  onOpenSettings,
  onEscape,
  onSaveNote,
  onDeleteNote,
  onNavigateBack,
  onNavigateForward,
  onLockApp,
  onZoomIn,
  onZoomOut,
  onZoomReset,
}: KeyboardShortcutHandlers): void {
  useEffect(() => {
    if (!enabled) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      const isModifier = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();

      // Alt + Left Arrow — History Back (Phase 9, Tasks 13 & 20)
      // On macOS, Option+Left is a text-editing command (word jump). Do not intercept when editing text.
      if (event.altKey && event.key === "ArrowLeft") {
        const isMac =
          typeof navigator !== "undefined" &&
          /Mac|iPhone|iPod|iPad/.test(navigator.platform || navigator.userAgent);
        const activeEl = document.activeElement;
        const isEditing =
          activeEl &&
          (activeEl.tagName === "INPUT" ||
            activeEl.tagName === "TEXTAREA" ||
            (activeEl as HTMLElement).isContentEditable);

        if (isMac && isEditing) {
          return;
        }

        event.preventDefault();
        onNavigateBack?.();
        return;
      }

      // Alt + Right Arrow — History Forward (Phase 9, Tasks 13 & 20)
      // On macOS, Option+Right is a text-editing command (word jump). Do not intercept when editing text.
      if (event.altKey && event.key === "ArrowRight") {
        const isMac =
          typeof navigator !== "undefined" &&
          /Mac|iPhone|iPod|iPad/.test(navigator.platform || navigator.userAgent);
        const activeEl = document.activeElement;
        const isEditing =
          activeEl &&
          (activeEl.tagName === "INPUT" ||
            activeEl.tagName === "TEXTAREA" ||
            (activeEl as HTMLElement).isContentEditable);

        if (isMac && isEditing) {
          return;
        }

        event.preventDefault();
        onNavigateForward?.();
        return;
      }

      // Escape — Global dismiss / blur
      if (event.key === "Escape") {
        onEscape?.();
        return;
      }

      // Ctrl/Cmd + S — Explicit Save Current Note
      if (isModifier && key === "s") {
        event.preventDefault();
        onSaveNote?.();
        return;
      }

      // Ctrl/Cmd + '+' / '=' / '-' / '0' — Zoom interface in / out / reset (browser convention).
      // Intentionally active while typing, as in browsers and editors.
      if (isModifier && !event.altKey) {
        if (event.key === "+" || event.key === "=") {
          event.preventDefault();
          onZoomIn?.();
          return;
        }
        if (event.key === "-" || event.key === "_") {
          event.preventDefault();
          onZoomOut?.();
          return;
        }
        if (event.key === "0") {
          event.preventDefault();
          onZoomReset?.();
          return;
        }
      }

      // Ctrl/Cmd + N — Create New Note
      if (isModifier && key === "n") {
        event.preventDefault();
        onNewNote?.();
        return;
      }

      // Ctrl/Cmd + K or Ctrl/Cmd + F — Focus Search (Phase 6, Task 22)
      if (isModifier && (key === "k" || key === "f")) {
        event.preventDefault();
        onFocusSearch?.();
        return;
      }

      // Ctrl/Cmd + , — Open Settings
      if (isModifier && event.key === ",") {
        event.preventDefault();
        onOpenSettings?.();
        return;
      }

      // Ctrl/Cmd + Alt + L or Ctrl/Cmd + Shift + L — Lock Application (Phase 12, Task 13)
      if (isModifier && (event.altKey || event.shiftKey) && key === "l") {
        event.preventDefault();
        onLockApp?.();
        return;
      }

      // Ctrl/Cmd + Backspace or Delete — Delete active note (Phase 8, Task 14)
      if (isModifier && (key === "backspace" || key === "delete")) {
        const activeEl = document.activeElement;
        const isEditing =
          activeEl &&
          (activeEl.tagName === "INPUT" ||
            activeEl.tagName === "TEXTAREA" ||
            (activeEl as HTMLElement).isContentEditable);
        if (!isEditing) {
          event.preventDefault();
          onDeleteNote?.();
          return;
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    onZoomIn,
    onZoomOut,
    onZoomReset,
    onNewNote,
    onFocusSearch,
    onOpenSettings,
    onEscape,
    onSaveNote,
    onDeleteNote,
    onNavigateBack,
    onNavigateForward,
    onLockApp,
  ]);
}

export default useKeyboardShortcuts;
