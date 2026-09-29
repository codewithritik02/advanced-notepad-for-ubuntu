import React, { useState, useEffect } from "react";
import { Modal, Button } from "../../../components/ui";
import type { Notebook } from "../types";
import "./NotebookDialog.css";

export interface DeleteNotebookDialogProps {
  isOpen: boolean;
  notebook: Notebook | null;
  hasChildren: boolean;
  onClose: () => void;
  onDelete: (id: string) => Promise<void>;
}

export const DeleteNotebookDialog: React.FC<DeleteNotebookDialogProps> = ({
  isOpen,
  notebook,
  hasChildren,
  onClose,
  onDelete,
}) => {
  const [isDeleting, setIsDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleDelete = async () => {
    if (!notebook || hasChildren) return;
    setIsDeleting(true);
    setError(null);
    try {
      await onDelete(notebook.id);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to delete notebook.");
    } finally {
      setIsDeleting(false);
    }
  };

  useEffect(() => {
    if (!isOpen) return;

    setError(null);
    setIsDeleting(false);

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        if (hasChildren) {
          onClose();
        } else {
          handleDelete();
        }
      } else if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, hasChildren, notebook]);

  if (!notebook) return null;

  const title = hasChildren
    ? `Cannot Delete "${notebook.name}"`
    : `Delete "${notebook.name}"?`;

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title={title}
      maxWidth="440px"
      footer={
        <div className="notebook-dialog-footer">
          {hasChildren ? (
            <Button
              variant="secondary"
              onClick={onClose}
              type="button"
            >
              Close
            </Button>
          ) : (
            <>
              <Button
                variant="secondary"
                onClick={onClose}
                disabled={isDeleting}
                type="button"
              >
                Cancel
              </Button>
              <Button
                variant="danger"
                onClick={handleDelete}
                isLoading={isDeleting}
                type="button"
              >
                Delete Notebook
              </Button>
            </>
          )}
        </div>
      }
    >
      <div className="notebook-delete-warning">
        {hasChildren ? (
          <div className="notebook-delete-blocked" role="alert">
            This notebook contains sub-notebooks.
            <br />
            Move or delete the sub-notebooks before deleting this notebook.
          </div>
        ) : (
          <>
            <p>
              Are you sure you want to delete <strong>{notebook.name}</strong>?
            </p>
            <div className="notebook-delete-consequence">
              <strong>Warning:</strong> Notes inside this notebook will become
              unfiled. The notes themselves will not be deleted.
            </div>
            {error && (
              <div className="notebook-dialog-error" role="alert">
                {error}
              </div>
            )}
          </>
        )}
      </div>
    </Modal>
  );
};
