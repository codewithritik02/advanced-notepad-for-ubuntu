import React, { useMemo, useState } from "react";
import { NotebookTreeItem } from "./NotebookTreeItem";
import { NotebookContextMenu } from "./NotebookContextMenu";
import { MAX_NOTEBOOK_NESTING_DEPTH } from "../../../services/storage/limits";
import { buildNotebookTree } from "../utils/notebookTree";
import type { Notebook, NotebooksStatus } from "../types";
import "./NotebookTree.css";

export interface NotebookTreeProps {
  notebooks: Notebook[];
  status?: NotebooksStatus;
  error?: string | null;
  selectedNotebookId?: string | null;
  expandedNotebookIds: Set<string>;
  onToggleExpand: (notebookId: string) => void;
  onSelectNotebook: (notebookId: string) => void;
  onContextMenu?: (e: React.MouseEvent, notebook: Notebook) => void;
  onCreateNotebook?: (parentId?: string | null) => void;
  onRenameNotebook?: (notebook: Notebook) => void;
  onDeleteNotebook?: (notebook: Notebook) => void;
  onExportNotebook?: (notebook: Notebook) => void;
  onCreateNote?: (notebookId: string) => void;
  onMoveNotebook?: (notebook: Notebook) => void;
  onDropNote?: (noteId: string, destinationNotebookId: string | null) => void;
  onRetry?: () => void;
  noteCounts?: Record<string, number>;
}

export const NotebookTree: React.FC<NotebookTreeProps> = ({
  notebooks,
  status = "idle",
  error,
  selectedNotebookId,
  expandedNotebookIds,
  onToggleExpand,
  onSelectNotebook,
  onContextMenu,
  onCreateNotebook,
  onRenameNotebook,
  onDeleteNotebook,
  onExportNotebook,
  onCreateNote,
  onMoveNotebook,
  onDropNote,
  onRetry,
  noteCounts,
}) => {
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    notebook: Notebook;
  } | null>(null);

  const tree = useMemo(() => {
    return buildNotebookTree(notebooks, noteCounts);
  }, [notebooks, noteCounts]);

  // Depth (1-based) of every notebook, used to stop offering sub-notebooks at the limit.
  const depthById = useMemo(() => {
    const map = new Map<string, number>();
    const walk = (nodes: typeof tree) => {
      for (const n of nodes) {
        map.set(n.id, n.depth + 1);
        walk(n.children);
      }
    };
    walk(tree);
    return map;
  }, [tree]);

  const handleContextMenu = (e: React.MouseEvent, notebook: Notebook) => {
    e.preventDefault();
    e.stopPropagation();
    onContextMenu?.(e, notebook);
    setContextMenu({
      x: e.clientX,
      y: e.clientY,
      notebook,
    });
  };

  // Loading state (Task 22)
  if (status === "loading" && notebooks.length === 0) {
    return (
      <div className="notebook-tree-state notebook-tree-loading" aria-live="polite">
        <span className="notebook-tree-spinner" />
        <span>Loading...</span>
      </div>
    );
  }

  // Error state (Task 23)
  if (status === "error" && notebooks.length === 0) {
    return (
      <div className="notebook-tree-state notebook-tree-error" role="alert">
        <span className="notebook-tree-error-msg" title={error || undefined}>
          Could not load notebooks.
        </span>
        {onRetry && (
          <button type="button" className="notebook-tree-retry-btn" onClick={onRetry}>
            Retry
          </button>
        )}
      </div>
    );
  }

  // Empty state (Task 21)
  if (notebooks.length === 0) {
    return (
      <div className="notebook-tree-empty">
        <p className="notebook-tree-empty-text">No notebooks yet.</p>
        {onCreateNotebook && (
          <button
            type="button"
            className="notebook-tree-create-btn"
            onClick={() => onCreateNotebook(null)}
          >
            + New Notebook
          </button>
        )}
      </div>
    );
  }

  return (
    <>
      <ul className="notebook-tree-root" role="tree" aria-label="Notebooks">
        {tree.map((rootNode) => (
          <NotebookTreeItem
            key={rootNode.id}
            node={rootNode}
            selectedNotebookId={selectedNotebookId}
            expandedNotebookIds={expandedNotebookIds}
            onToggleExpand={onToggleExpand}
            onSelectNotebook={onSelectNotebook}
            onContextMenu={handleContextMenu}
            onCreateChild={(parentId) => onCreateNotebook?.(parentId)}
            onRename={onRenameNotebook}
            onDelete={onDeleteNotebook}
            onDropNote={onDropNote}
          />
        ))}
      </ul>
      {contextMenu && (
        <NotebookContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          notebook={contextMenu.notebook}
          canCreateChild={(depthById.get(contextMenu.notebook.id) ?? 1) < MAX_NOTEBOOK_NESTING_DEPTH}
          onClose={() => setContextMenu(null)}
          onNewNote={onCreateNote ? (nb) => onCreateNote(nb.id) : undefined}
          onNewChild={(nb) => onCreateNotebook?.(nb.id)}
          onMove={onMoveNotebook}
          onRename={(nb) => onRenameNotebook?.(nb)}
          onDelete={(nb) => onDeleteNotebook?.(nb)}
          onExport={onExportNotebook}
        />
      )}
    </>
  );
};

