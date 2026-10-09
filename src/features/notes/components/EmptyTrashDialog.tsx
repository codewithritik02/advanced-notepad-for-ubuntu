import React, { useState, useEffect } from "react";
import { Modal, Button } from "../../../components/ui";
import "./EmptyTrashDialog.css";

export interface EmptyTrashDialogProps {
  isOpen: boolean;
  noteCount: number;
  onClose: () => void;
  onConfirm: () => Promise<void>;
}

export const EmptyTrashDialog: React.FC<EmptyTrashDialogProps> = ({
  isOpen,
  noteCount,
  onClose,
  onConfirm,
}) => {
  const [isDeleting, setIsDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleConfirm = async () => {
    if (isDeleting) return;
    setIsDeleting(true);
    setError(null);
    try {
      await onConfirm();
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to empty trash.");
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
  }, [isOpen, isDeleting]);

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Empty Trash?"
      maxWidth="440px"
      footer={
        <div className="empty-trash-dialog-footer">
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
            Empty Trash
          </Button>
        </div>
      }
    >
      <div className="empty-trash-warning">
        {noteCount > 0 ? (
          <p>
            Are you sure you want to permanently delete all{" "}
            <strong>
              {noteCount} {noteCount === 1 ? "note" : "notes"}
            </strong>{" "}
            in the Trash?
          </p>
        ) : (
          <p>Are you sure you want to empty the Trash? Trash is already empty.</p>
        )}
        <div className="empty-trash-consequence">
          <strong>Warning:</strong> This cannot be undone. Notes in Trash will NOT be restored and will be permanently destroyed along with all associated attachments.
        </div>
        {error && (
          <div className="empty-trash-dialog-error" role="alert">
            {error}
          </div>
        )}
      </div>
    </Modal>
  );
};

export default EmptyTrashDialog;
