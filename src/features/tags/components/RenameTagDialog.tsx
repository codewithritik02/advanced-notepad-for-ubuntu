import React, { useState, useEffect, useRef } from "react";
import { Modal, Button } from "../../../components/ui";
import type { Tag } from "../types";
import { validateTagName } from "../utils/tagUtils";
import "./TagDialog.css";

export interface RenameTagDialogProps {
  isOpen: boolean;
  tag: Tag | null;
  onClose: () => void;
  onRename: (id: string, newName: string) => Promise<void>;
}

export const RenameTagDialog: React.FC<RenameTagDialogProps> = ({
  isOpen,
  tag,
  onClose,
  onRename,
}) => {
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen && tag) {
      setName(tag.name);
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
  }, [isOpen, tag]);

  const handleSubmit = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!tag) return;

    const validation = validateTagName(name);
    if (!validation.valid) {
      setError(validation.error || "Tag name cannot be empty.");
      inputRef.current?.focus();
      return;
    }

    // If unchanged, simply close without extra IPC call
    if (validation.trimmed === tag.name) {
      onClose();
      return;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      await onRename(tag.id, validation.trimmed);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.toLowerCase().includes("already exists") || msg.toLowerCase().includes("conflict")) {
        setError("A tag with this name already exists.");
      } else {
        setError(msg || "Failed to rename tag.");
      }
    } finally {
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
      title="Rename Tag"
      maxWidth="380px"
      footer={
        <div className="tag-dialog-footer">
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
            disabled={!name.trim() || isSubmitting}
            type="button"
          >
            Save
          </Button>
        </div>
      }
    >
      <form onSubmit={handleSubmit} className="tag-dialog-form">
        <label htmlFor="rename-tag-name-input" className="tag-dialog-label">
          <span>Name</span>
        </label>
        <div className="tag-dialog-input-wrapper">
          <span className="tag-dialog-prefix">#</span>
          <input
            id="rename-tag-name-input"
            ref={inputRef}
            type="text"
            className={`tag-dialog-input ${error ? "has-error" : ""}`}
            placeholder="Tag name"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
              if (error) setError(null);
            }}
            onKeyDown={handleKeyDown}
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

export default RenameTagDialog;
