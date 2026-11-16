import React, { useState, useEffect, useMemo } from "react";
import { Modal, Button } from "../../../components/ui";
import { MAX_NOTEBOOK_NESTING_DEPTH } from "../../../services/storage/limits";
import { buildNotebookTree } from "../utils/notebookTree";
import type { Notebook, NotebookTreeNode } from "../types";
import "./NotebookDialog.css";

export interface MoveNotebookDialogProps {
  isOpen: boolean;
  notebook: Notebook | null;
  notebooks: Notebook[];
  onClose: () => void;
  onMove: (notebookId: string, newParentId: string | null) => Promise<void>;
}

const FolderIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);

/** Number of levels in a subtree (a lone notebook has height 1). */
function subtreeHeight(node: NotebookTreeNode): number {
  return 1 + node.children.reduce((max, c) => Math.max(max, subtreeHeight(c)), 0);
}

function findNode(nodes: NotebookTreeNode[], id: string): NotebookTreeNode | null {
  for (const n of nodes) {
    if (n.id === id) return n;
    const found = findNode(n.children, id);
    if (found) return found;
  }
  return null;
}

function collectIds(node: NotebookTreeNode, out: Set<string>) {
  out.add(node.id);
  node.children.forEach((c) => collectIds(c, out));
}

export const MoveNotebookDialog: React.FC<MoveNotebookDialogProps> = ({
  isOpen,
  notebook,
  notebooks,
  onClose,
  onMove,
}) => {
  const [selectedTargetId, setSelectedTargetId] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen && notebook) {
      setSelectedTargetId(notebook.parent_id);
      setError(null);
      setIsSubmitting(false);
    }
  }, [isOpen, notebook]);

  const tree = useMemo(() => buildNotebookTree(notebooks), [notebooks]);

  // Targets that would be invalid: itself, its own descendants, or anything too deep for its subtree.
  const { blocked, height } = useMemo(() => {
    const blockedIds = new Map<string, string>();
    if (!notebook) return { blocked: blockedIds, height: 1 };
    const self = findNode(tree, notebook.id);
    if (!self) return { blocked: blockedIds, height: 1 };
    const h = subtreeHeight(self);
    const own = new Set<string>();
    collectIds(self, own);
    own.forEach((id) => blockedIds.set(id, "Inside the notebook being moved"));
    const walk = (nodes: NotebookTreeNode[]) => {
      for (const n of nodes) {
        if (!blockedIds.has(n.id) && n.depth + 1 + h > MAX_NOTEBOOK_NESTING_DEPTH) {
          blockedIds.set(n.id, `Too deep (max ${MAX_NOTEBOOK_NESTING_DEPTH} levels)`);
        }
        walk(n.children);
      }
    };
    walk(tree);
    return { blocked: blockedIds, height: h };
  }, [tree, notebook]);

  if (!notebook) return null;

  const isTargetChanged = selectedTargetId !== notebook.parent_id;

  const handleSubmit = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!isTargetChanged) {
      onClose();
      return;
    }
    setIsSubmitting(true);
    setError(null);
    try {
      await onMove(notebook.id, selectedTargetId);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to move notebook.");
      setIsSubmitting(false);
    }
  };

  const renderNode = (node: NotebookTreeNode) => {
    const reason = blocked.get(node.id);
    const isCurrentParent = notebook.parent_id === node.id;
    return (
      <React.Fragment key={node.id}>
        <label
          className={`move-dialog-item ${selectedTargetId === node.id ? "is-selected" : ""} ${reason ? "is-disabled" : ""}`}
          style={{ paddingLeft: `${12 + node.depth * 16}px`, opacity: reason ? 0.45 : undefined, cursor: reason ? "not-allowed" : undefined }}
          title={reason}
        >
          <input
            type="radio"
            name="move-target-parent"
            value={node.id}
            checked={selectedTargetId === node.id}
            disabled={Boolean(reason)}
            onChange={() => setSelectedTargetId(node.id)}
            className="move-dialog-radio"
          />
          <span className="move-dialog-icon"><FolderIcon /></span>
          <span className="move-dialog-name">{node.name}</span>
          {isCurrentParent && <span className="move-dialog-current-tag">Current</span>}
        </label>
        {node.children.map(renderNode)}
      </React.Fragment>
    );
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title={`Move "${notebook.name}"${height > 1 ? ` (with sub-notebooks)` : ""}`}
      maxWidth="460px"
      footer={
        <div className="notebook-dialog-footer">
          <Button variant="secondary" onClick={onClose} disabled={isSubmitting} type="button">
            Cancel
          </Button>
          <Button
            variant="primary"
            onClick={() => handleSubmit()}
            isLoading={isSubmitting}
            disabled={!isTargetChanged || isSubmitting}
            type="submit"
          >
            Move
          </Button>
        </div>
      }
    >
      <form
        onSubmit={handleSubmit}
        className="notebook-dialog-form"
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            handleSubmit();
          } else if (e.key === "Escape") {
            e.preventDefault();
            onClose();
          }
        }}
      >
        <div className="move-dialog-list" role="radiogroup" aria-label="Select new parent notebook">
          <label
            className={`move-dialog-item ${selectedTargetId === null ? "is-selected" : ""}`}
            style={{ paddingLeft: "12px" }}
          >
            <input
              type="radio"
              name="move-target-parent"
              value=""
              checked={selectedTargetId === null}
              onChange={() => setSelectedTargetId(null)}
              className="move-dialog-radio"
            />
            <span className="move-dialog-icon"><FolderIcon /></span>
            <span className="move-dialog-name">Top level (no parent)</span>
            {notebook.parent_id === null && <span className="move-dialog-current-tag">Current</span>}
          </label>
          {tree.map(renderNode)}
        </div>
        {error && (
          <div className="notebook-dialog-error" role="alert">
            {error}
          </div>
        )}
      </form>
    </Modal>
  );
};
