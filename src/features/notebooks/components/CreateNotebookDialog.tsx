import React, { useState, useEffect, useRef } from "react";
import { Modal, Button } from "../../../components/ui";
import "./NotebookDialog.css";

export interface CreateNotebookDialogProps {
  isOpen: boolean;
  onClose: () => void;
  parentId?: string | null;
  parentName?: string | null;
  onCreate: (name: string, parentId?: string | null) => Promise<void>;
}

export const CreateNotebookDialog: React.FC<CreateNotebookDialogProps> = ({
  isOpen,
  onClose,
  parentId = null,
  parentName = null,
  onCreate,
}) => {
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Reset state and focus input when modal opens
  useEffect(() => {
    if (isOpen) {
      setName("");
      setError(null);
      setIsSubmitting(false);
      // Short delay for modal DOM mount
      setTimeout(() => {
        inputRef.current?.focus();
      }, 50);
    }
  }, [isOpen]);

  const handleSubmit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    const trimmed = name.trim();

    if (!trimmed) {
      setError("Notebook name cannot be empty.");
      inputRef.current?.focus();
      return;
    }

    if (trimmed.length > 100) {
      setError("Notebook name cannot exceed 100 characters.");
      return;
    }

    try {
      setIsSubmitting(true);
      setError(null);
      await onCreate(trimmed, parentId);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to create notebook.");
    } finally {
      setIsSubmitting(false);
    }
  };

  const dialogTitle = parentName
    ? `New Sub-notebook in "${parentName}"`
    : "Create Notebook";

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title={dialogTitle}
      maxWidth="420px"
      footer={
        <div className="notebook-dialog-footer">
          <Button variant="secondary" onClick={onClose} disabled={isSubmitting}>
            Cancel
          </Button>
          <Button
            variant="primary"
            onClick={() => handleSubmit()}
            isLoading={isSubmitting}
            disabled={!name.trim() || isSubmitting}
            type="submit"
          >
            Create
          </Button>
        </div>
      }
    >
      <form onSubmit={handleSubmit} className="notebook-dialog-form">
        <label className="notebook-dialog-label" htmlFor="notebook-name-input">
          <span>Name</span>
          {parentName && (
            <span className="notebook-dialog-parent-hint">
              Parent: <strong>{parentName}</strong>
            </span>
          )}
        </label>
        <input
          id="notebook-name-input"
          ref={inputRef}
          type="text"
          className={`notebook-dialog-input ${error ? "has-error" : ""}`}
          placeholder="e.g. Work, Travel, Ideas & Notes"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            if (error) setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              handleSubmit();
            } else if (e.key === "Escape") {
              e.preventDefault();
              onClose();
            }
          }}
          disabled={isSubmitting}
          autoComplete="off"
        />
        {error && <span className="notebook-dialog-error" role="alert">{error}</span>}
      </form>
    </Modal>
  );
};

export default CreateNotebookDialog;
