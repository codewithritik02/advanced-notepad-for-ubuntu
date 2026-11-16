import React, { useEffect, useRef } from "react";
import type { Notebook } from "../types";
import { MAX_NOTEBOOK_NESTING_DEPTH } from "../../../services/storage/limits";
import "./NotebookContextMenu.css";

export interface NotebookContextMenuProps {
  x: number;
  y: number;
  notebook: Notebook;
  /** False when the notebook already sits at the maximum nesting depth. */
  canCreateChild?: boolean;
  onClose: () => void;
  onNewNote?: (notebook: Notebook) => void;
  onNewChild: (notebook: Notebook) => void;
  onMove?: (notebook: Notebook) => void;
  onRename: (notebook: Notebook) => void;
  onDelete: (notebook: Notebook) => void;
  onExport?: (notebook: Notebook) => void;
}

const NewNoteIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
    <polyline points="14 3 14 8 19 8" />
    <line x1="12" y1="12" x2="12" y2="18" />
    <line x1="9" y1="15" x2="15" y2="15" />
  </svg>
);

const MoveIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
    <path d="M9 14h6" />
    <polyline points="12.5 11.5 15 14 12.5 16.5" />
  </svg>
);

const NewFolderIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
    <line x1="12" y1="11" x2="12" y2="17" />
    <line x1="9" y1="14" x2="15" y2="14" />
  </svg>
);

const RenameIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
  </svg>
);

const ExportIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" y1="15" x2="12" y2="3" />
  </svg>
);

const TrashIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="3 6 5 6 21 6" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
  </svg>
);

export const NotebookContextMenu: React.FC<NotebookContextMenuProps> = ({
  x,
  y,
  notebook,
  canCreateChild = true,
  onClose,
  onNewNote,
  onNewChild,
  onMove,
  onRename,
  onDelete,
  onExport,
}) => {
  const menuRef = useRef<HTMLDivElement>(null);

  // Close on outside click or escape
  useEffect(() => {
    const handlePointerDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };

    // Use mousedown in capture phase to reliably close when clicking outside
    window.addEventListener("mousedown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("mousedown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [onClose]);

  // Adjust position so context menu doesn't overflow screen bounds
  const menuWidth = 180;
  const menuHeight = 185;
  const adjustedX = Math.min(x, window.innerWidth - menuWidth - 8);
  const adjustedY = Math.min(y, window.innerHeight - menuHeight - 8);

  return (
    <div
      ref={menuRef}
      className="notebook-context-menu"
      style={{
        left: `${Math.max(8, adjustedX)}px`,
        top: `${Math.max(8, adjustedY)}px`,
      }}
      role="menu"
      aria-label={`Options for ${notebook.name}`}
    >
      <div className="notebook-context-menu-header">
        <span className="notebook-context-menu-title" title={notebook.name}>
          {notebook.name}
        </span>
      </div>

      {onNewNote && (
        <button
          type="button"
          className="notebook-context-menu-item"
          onClick={() => {
            onClose();
            onNewNote(notebook);
          }}
          role="menuitem"
        >
          <span className="context-menu-icon"><NewNoteIcon /></span>
          <span className="context-menu-label">New Note</span>
        </button>
      )}

      <button
        type="button"
        className="notebook-context-menu-item"
        disabled={!canCreateChild}
        title={
          canCreateChild
            ? undefined
            : `Maximum nesting depth (${MAX_NOTEBOOK_NESTING_DEPTH} levels) reached`
        }
        onClick={() => {
          if (!canCreateChild) return;
          onClose();
          onNewChild(notebook);
        }}
        role="menuitem"
      >
        <span className="context-menu-icon"><NewFolderIcon /></span>
        <span className="context-menu-label">New Sub-notebook</span>
      </button>

      {onMove && (
        <button
          type="button"
          className="notebook-context-menu-item"
          onClick={() => {
            onClose();
            onMove(notebook);
          }}
          role="menuitem"
        >
          <span className="context-menu-icon"><MoveIcon /></span>
          <span className="context-menu-label">Move to...</span>
        </button>
      )}

      <button
        type="button"
        className="notebook-context-menu-item"
        onClick={() => {
          onClose();
          onRename(notebook);
        }}
        role="menuitem"
      >
        <span className="context-menu-icon"><RenameIcon /></span>
        <span className="context-menu-label">Rename</span>
      </button>

      {onExport && (
        <button
          type="button"
          className="notebook-context-menu-item"
          onClick={() => {
            onClose();
            onExport(notebook);
          }}
          role="menuitem"
        >
          <span className="context-menu-icon"><ExportIcon /></span>
          <span className="context-menu-label">Export Notebook...</span>
        </button>
      )}

      <div className="notebook-context-menu-divider" />

      <button
        type="button"
        className="notebook-context-menu-item is-danger"
        onClick={() => {
          onClose();
          onDelete(notebook);
        }}
        role="menuitem"
      >
        <span className="context-menu-icon"><TrashIcon /></span>
        <span className="context-menu-label">Delete</span>
      </button>
    </div>
  );
};

export default NotebookContextMenu;
