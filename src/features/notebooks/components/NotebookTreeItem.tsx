import React from "react";
import type { Notebook, NotebookTreeNode } from "../types";

export interface NotebookTreeItemProps {
  node: NotebookTreeNode;
  selectedNotebookId?: string | null;
  expandedNotebookIds: Set<string>;
  onToggleExpand: (notebookId: string) => void;
  onSelectNotebook: (notebookId: string) => void;
  onContextMenu?: (e: React.MouseEvent, notebook: Notebook) => void;
  onCreateChild?: (parentId: string, parentName: string) => void;
  onRename?: (notebook: Notebook) => void;
  onDelete?: (notebook: Notebook) => void;
  onDropNote?: (noteId: string, destinationNotebookId: string | null) => void;
}

const ChevronIcon = ({ isExpanded }: { isExpanded: boolean }) => (
  <svg
    width="12"
    height="12"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    style={{
      transform: isExpanded ? "rotate(90deg)" : "rotate(0deg)",
      transition: "transform 0.15s ease",
    }}
  >
    <polyline points="9 18 15 12 9 6" />
  </svg>
);

const FolderIcon = () => (
  <svg
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.8"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);

const EditIcon = () => (
  <svg
    width="12"
    height="12"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
  </svg>
);

const TrashIcon = () => (
  <svg
    width="12"
    height="12"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <polyline points="3 6 5 6 21 6" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
  </svg>
);

export const NotebookTreeItem: React.FC<NotebookTreeItemProps> = ({
  node,
  selectedNotebookId,
  expandedNotebookIds,
  onToggleExpand,
  onSelectNotebook,
  onContextMenu,
  onCreateChild,
  onRename,
  onDelete,
  onDropNote,
}) => {
  const [isDragOver, setIsDragOver] = React.useState(false);
  const hasChildren = node.children.length > 0;
  const isExpanded = expandedNotebookIds.has(node.id);
  const isSelected = selectedNotebookId === node.id;

  const handleRowClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    onSelectNotebook(node.id);
  };

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    onRename?.(node);
  };

  const handleToggleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (hasChildren) {
      onToggleExpand(node.id);
    }
  };

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    onContextMenu?.(e, node);
  };

  const handleDragOver = (e: React.DragEvent) => {
    if (e.dataTransfer.types.includes("application/x-note-id")) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      if (!isDragOver) setIsDragOver(true);
    }
  };

  const handleDragLeave = (e: React.DragEvent) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setIsDragOver(false);
    }
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    const noteId = e.dataTransfer.getData("application/x-note-id");
    if (noteId && onDropNote) {
      onDropNote(noteId, node.id);
    }
  };

  // Indentation per depth level
  const paddingLeft = 8 + node.depth * 14;

  return (
    <li
      role="treeitem"
      aria-expanded={hasChildren ? isExpanded : undefined}
      aria-selected={isSelected}
      className="notebook-tree-li"
    >
      <div
        className={`notebook-tree-row ${isSelected ? "is-selected" : ""} ${isDragOver ? "is-drag-over" : ""}`}
        style={{ paddingLeft: `${paddingLeft}px` }}
        onClick={handleRowClick}
        onDoubleClick={handleDoubleClick}
        onContextMenu={handleContextMenu}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        title={`${node.name} (Double-click or F2 to rename)`}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onSelectNotebook(node.id);
          } else if (e.key === "F2") {
            e.preventDefault();
            onRename?.(node);
          } else if (e.key === "Delete") {
            e.preventDefault();
            onDelete?.(node);
          } else if (e.key === "ArrowRight" && hasChildren && !isExpanded) {
            e.preventDefault();
            onToggleExpand(node.id);
          } else if (e.key === "ArrowLeft" && hasChildren && isExpanded) {
            e.preventDefault();
            onToggleExpand(node.id);
          }
        }}
      >
        {/* Expand / Collapse Chevron */}
        <button
          type="button"
          className={`notebook-tree-chevron ${!hasChildren ? "is-hidden" : ""}`}
          onClick={handleToggleClick}
          aria-label={
            hasChildren
              ? isExpanded
                ? `Collapse ${node.name}`
                : `Expand ${node.name}`
              : undefined
          }
          tabIndex={-1}
        >
          {hasChildren ? <ChevronIcon isExpanded={isExpanded} /> : <span className="chevron-spacer" />}
        </button>

        {/* Folder Icon */}
        <span className="notebook-tree-icon">
          <FolderIcon />
        </span>

        {/* Notebook Name */}
        <span className="notebook-tree-name">{node.name}</span>

        {/* Note Count Badge */}
        {typeof node.noteCount === "number" && node.noteCount > 0 && (
          <span className="notebook-tree-count" aria-label={`${node.noteCount} notes`}>
            {node.noteCount}
          </span>
        )}

        {/* Row Actions (visible on hover / focus) */}
        <div className="notebook-tree-actions" onClick={(e) => e.stopPropagation()}>
          {onRename && (
            <button
              type="button"
              className="notebook-tree-action-btn"
              onClick={() => onRename(node)}
              title="Rename (F2)"
              aria-label={`Rename ${node.name}`}
              tabIndex={-1}
            >
              <EditIcon />
            </button>
          )}
          {onDelete && (
            <button
              type="button"
              className="notebook-tree-action-btn action-delete"
              onClick={() => onDelete(node)}
              title="Delete Notebook"
              aria-label={`Delete ${node.name}`}
              tabIndex={-1}
            >
              <TrashIcon />
            </button>
          )}
        </div>
      </div>

      {/* Render Nested Children if Expanded */}
      {hasChildren && isExpanded && (
        <ul className="notebook-tree-children" role="group">
          {node.children.map((child) => (
            <NotebookTreeItem
              key={child.id}
              node={child}
              selectedNotebookId={selectedNotebookId}
              expandedNotebookIds={expandedNotebookIds}
              onToggleExpand={onToggleExpand}
              onSelectNotebook={onSelectNotebook}
              onContextMenu={onContextMenu}
              onCreateChild={onCreateChild}
              onRename={onRename}
              onDelete={onDelete}
              onDropNote={onDropNote}
            />
          ))}
        </ul>
      )}
    </li>
  );
};
