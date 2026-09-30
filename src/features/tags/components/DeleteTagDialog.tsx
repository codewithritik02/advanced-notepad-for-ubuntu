import React, { useState, useEffect } from "react";
import { Modal, Button } from "../../../components/ui";
import type { Tag } from "../types";
import "./TagDialog.css";

export interface DeleteTagDialogProps {
  isOpen: boolean;
  tag: Tag | null;
  noteCount?: number;
  onClose: () => void;
  onDelete: (id: string) => Promise<void>;
}

export const DeleteTagDialog: React.FC<DeleteTagDialogProps> = ({
  isOpen,
  tag,
  noteCount,
  onClose,
  onDelete,
}) => {
  const [isDeleting, setIsDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleDelete = async () => {
    if (!tag) return;
    setIsDeleting(true);
    setError(null);
    try {
      await onDelete(tag.id);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to delete tag.");
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
        handleDelete();
      } else if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, tag]);

  if (!tag) return null;

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title={`Delete tag "${tag.name}"?`}
      maxWidth="420px"
      footer={
        <div className="tag-dialog-footer">
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
            Delete Tag
          </Button>
        </div>
      }
    >
      <div className="tag-delete-warning">
        <p>
          Are you sure you want to delete the tag <strong>#{tag.name}</strong>?
        </p>
        {noteCount !== undefined && noteCount > 0 && (
          <p>
            This tag is currently assigned to {noteCount}{" "}
            {noteCount === 1 ? "note" : "notes"}.
          </p>
        )}
        <div className="tag-delete-consequence">
          <strong>Notice:</strong> The tag will be removed from notes, but the
          notes themselves will not be deleted.
        </div>
        {error && (
          <div className="tag-dialog-error" role="alert">
            {error}
          </div>
        )}
      </div>
    </Modal>
  );
};

export default DeleteTagDialog;
