import React, { useState, useEffect, useRef } from "react";
import { Modal, Button } from "../../../components/ui";
import { validateTagName } from "../utils/tagUtils";
import "./TagDialog.css";

export interface CreateTagDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onCreate: (name: string) => Promise<void>;
  initialName?: string;
}

export const CreateTagDialog: React.FC<CreateTagDialogProps> = ({
  isOpen,
  onClose,
  onCreate,
  initialName = "",
}) => {
  const [name, setName] = useState(initialName);
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen) {
      setName(initialName);
      setError(null);
      setIsSubmitting(false);
      setTimeout(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      }, 50);
    }
  }, [isOpen, initialName]);

  const handleSubmit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    const validation = validateTagName(name);

    if (!validation.valid) {
      setError(validation.error || "Tag name cannot be empty.");
      inputRef.current?.focus();
      return;
    }

    try {
      setIsSubmitting(true);
      setError(null);
      await onCreate(validation.trimmed);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Failed to create tag.");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Create Tag"
      maxWidth="380px"
      footer={
        <div className="tag-dialog-footer">
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
      <form onSubmit={handleSubmit} className="tag-dialog-form">
        <label className="tag-dialog-label" htmlFor="tag-name-input">
          <span>Name</span>
        </label>
        <div className="tag-dialog-input-wrapper">
          <span className="tag-dialog-prefix">#</span>
          <input
            id="tag-name-input"
            ref={inputRef}
            type="text"
            className={`tag-dialog-input ${error ? "has-error" : ""}`}
            placeholder="e.g. work, important, project-x"
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
        </div>
        {error && (
          <span className="tag-dialog-error" role="alert">
            {error}
          </span>
        )}
      </form>
    </Modal>
  );
};

export default CreateTagDialog;
