import React, { useState, useEffect, useRef } from "react";
import { Modal, Button } from "../../../components/ui";
import type { Notebook } from "../types";
import "./NotebookDialog.css";

export interface RenameNotebookDialogProps {
  isOpen: boolean;
  notebook: Notebook | null;
  onClose: () => void;
  onRename: (id: string, newName: string) => Promise<void>;
}

export const RenameNotebookDialog: React.FC<RenameNotebookDialogProps> = ({
  isOpen,
  notebook,
  onClose,
  onRename,
}) => {
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Initialize input with current notebook name and auto-select text
  useEffect(() => {
    if (isOpen && notebook) {
      setName(notebook.name);
      setError(null);
      setIsSubmitting(false);

      const timer = setTimeout(() => {
        if (inputRef.current) {
          inputRef.current.focus();
          inputRef.current.select();
        }
      }, 50);
      return () => clearTimeout(timer);
    }
  }, [isOpen, notebook]);

  const handleSubmit = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!notebook) return;

    const trimmed = name.trim();
    if (!trimmed) {
      setError("Notebook name cannot be empty.");
      inputRef.current?.focus();
      return;
    }

    // If unchanged, simply close without network/IPC trip
    if (trimmed === notebook.name) {
      onClose();
      return;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      await onRename(notebook.id, trimmed);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to rename notebook.");
      setIsSubmitting(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleSubmit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Rename Notebook"
      maxWidth="420px"
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
            type="button"
          >
            Save
          </Button>
        </div>
      }
    >
      <form onSubmit={handleSubmit} className="notebook-dialog-form">
        <label htmlFor="rename-notebook-name" className="notebook-dialog-label">
          Name
        </label>
        <input
          id="rename-notebook-name"
          ref={inputRef}
          type="text"
          className={`notebook-dialog-input ${error ? "has-error" : ""}`}
          placeholder="Notebook name"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            if (error) setError(null);
          }}
          onKeyDown={handleKeyDown}
          maxLength={100}
          disabled={isSubmitting}
        />

        {error && (
          <div className="notebook-dialog-error" role="alert">
            {error}
          </div>
        )}
      </form>
    </Modal>
  );
};
