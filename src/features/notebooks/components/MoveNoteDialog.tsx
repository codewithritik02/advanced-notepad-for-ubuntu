import React, { useState, useEffect } from "react";
import { Modal, Button } from "../../../components/ui";
import { buildNotebookTree } from "../utils/notebookTree";
import type { Notebook, NotebookTreeNode } from "../types";
import type { Note } from "../../../services/storage";
import "./NotebookDialog.css";

export interface MoveNoteDialogProps {
  isOpen: boolean;
  note: Note | null;
  notebooks: Notebook[];
  onClose: () => void;
  onMove: (noteId: string, targetNotebookId: string | null) => Promise<void>;
}

const UnfiledIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
    <line x1="9" y1="13" x2="15" y2="13" />
  </svg>
);

const FolderIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);

export const MoveNoteDialog: React.FC<MoveNoteDialogProps> = ({
  isOpen,
  note,
  notebooks,
  onClose,
  onMove,
}) => {
  const [selectedTargetId, setSelectedTargetId] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen && note) {
      setSelectedTargetId(note.notebook_id);
      setError(null);
      setIsSubmitting(false);
    }
  }, [isOpen, note]);

  if (!note) return null;

  const currentNotebookId = note.notebook_id;
  const isTargetChanged = selectedTargetId !== currentNotebookId;

  const handleSubmit = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!isTargetChanged) {
      onClose();
      return;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      await onMove(note.id, selectedTargetId);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to move note.");
      setIsSubmitting(false);
    }
  };

  const tree = buildNotebookTree(notebooks);

  const renderTreeNode = (node: NotebookTreeNode) => {
    const isSelected = selectedTargetId === node.id;
    const isCurrent = currentNotebookId === node.id;
    const paddingLeft = 12 + node.depth * 16;

    return (
      <React.Fragment key={node.id}>
        <label
          className={`move-dialog-item ${isSelected ? "is-selected" : ""}`}
          style={{ paddingLeft: `${paddingLeft}px` }}
        >
          <input
            type="radio"
            name="move-target-notebook"
            value={node.id}
            checked={isSelected}
            onChange={() => setSelectedTargetId(node.id)}
            className="move-dialog-radio"
          />
          <span className="move-dialog-icon">
            <FolderIcon />
          </span>
          <span className="move-dialog-name">{node.name}</span>
          {isCurrent && <span className="move-dialog-current-tag">Current</span>}
        </label>
        {node.children.map(renderTreeNode)}
      </React.Fragment>
    );
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title={`Move "${note.title || "Untitled Note"}"`}
      maxWidth="460px"
      footer={
        <div className="notebook-dialog-footer">
          <Button
            variant="secondary"
            onClick={onClose}
            disabled={isSubmitting}
            type="button"
          >
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
        <div className="move-dialog-list" role="radiogroup" aria-label="Select destination notebook">
          {/* Unfiled Option */}
          <label
            className={`move-dialog-item ${selectedTargetId === null ? "is-selected" : ""}`}
            style={{ paddingLeft: "12px" }}
          >
            <input
              type="radio"
              name="move-target-notebook"
              value=""
              checked={selectedTargetId === null}
              onChange={() => setSelectedTargetId(null)}
              className="move-dialog-radio"
            />
            <span className="move-dialog-icon">
              <UnfiledIcon />
            </span>
            <span className="move-dialog-name">Unfiled</span>
            {currentNotebookId === null && (
              <span className="move-dialog-current-tag">Current</span>
            )}
          </label>

          {/* Notebook Tree Options */}
          {tree.map(renderTreeNode)}
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
