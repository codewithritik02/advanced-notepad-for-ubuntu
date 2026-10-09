import React, { useState, useEffect } from "react";
import { Modal, Button } from "../../../components/ui";
import "./PermanentDeleteDialog.css";

export interface PermanentDeleteTarget {
  id: string;
  title: string;
}

export interface PermanentDeleteDialogProps {
  isOpen: boolean;
  note: PermanentDeleteTarget | null;
  onClose: () => void;
  onConfirm: (id: string) => Promise<void>;
}

export const PermanentDeleteDialog: React.FC<PermanentDeleteDialogProps> = ({
  isOpen,
  note,
  onClose,
  onConfirm,
}) => {
  const [isDeleting, setIsDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleConfirm = async () => {
    if (!note || isDeleting) return;
    setIsDeleting(true);
    setError(null);
    try {
      await onConfirm(note.id);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to permanently delete note.");
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
        handleConfirm();
      } else if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, note, isDeleting]);

  if (!note) return null;

  const displayTitle = note.title.trim() ? note.title.trim() : "Untitled Note";

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title={`Permanently delete "${displayTitle}"?`}
      maxWidth="440px"
      footer={
        <div className="permanent-delete-dialog-footer">
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
            onClick={handleConfirm}
            isLoading={isDeleting}
            disabled={isDeleting}
            type="button"
          >
            Permanently Delete
          </Button>
        </div>
      }
    >
      <div className="permanent-delete-warning">
        <p>
          Are you sure you want to permanently delete <strong>{displayTitle}</strong>?
        </p>
        <div className="permanent-delete-consequence">
          <strong>Warning:</strong> This operation cannot be undone through the Trash interface. The note and all associated attachments will be permanently removed.
        </div>
        {error && (
          <div className="permanent-delete-dialog-error" role="alert">
            {error}
          </div>
        )}
      </div>
    </Modal>
  );
};

export default PermanentDeleteDialog;
