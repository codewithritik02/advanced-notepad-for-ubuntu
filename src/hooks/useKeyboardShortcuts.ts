import { useEffect } from "react";

export interface KeyboardShortcutHandlers {
  onNewNote?: () => void;
  onFocusSearch?: () => void;
  onOpenSettings?: () => void;
  onEscape?: () => void;
  onSaveNote?: () => void;
}

/**
 * Centralized keyboard shortcuts listener for Personal Notepad.
 *
 * Handles cross-platform desktop modifier keys (Ctrl on Linux/Windows, Cmd on macOS)
 * without scattering listeners across individual components.
 */
export function useKeyboardShortcuts({
  onNewNote,
  onFocusSearch,
  onOpenSettings,
  onEscape,
  onSaveNote,
}: KeyboardShortcutHandlers): void {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const isModifier = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();

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
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onNewNote, onFocusSearch, onOpenSettings, onEscape, onSaveNote]);
}

export default useKeyboardShortcuts;
