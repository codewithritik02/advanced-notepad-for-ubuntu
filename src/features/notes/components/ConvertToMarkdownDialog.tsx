import React, { useEffect } from "react";
import { Modal, Button } from "../../../components/ui";
import "./ConfirmNoteActionDialog.css";

export interface ConvertToMarkdownDialogProps {
  isOpen: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

/** Images only render in Markdown notes, so inserting one into a plain-text note asks first. */
export const ConvertToMarkdownDialog: React.FC<ConvertToMarkdownDialogProps> = ({
  isOpen,
  onCancel,
  onConfirm,
}) => {
  useEffect(() => {
    if (!isOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Enter" && !event.repeat) {
        event.preventDefault();
        onConfirm();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isOpen, onConfirm]);

  if (!isOpen) return null;

  return (
    <Modal
      isOpen
      onClose={onCancel}
      title="Convert to Markdown?"
      maxWidth="440px"
      footer={
        <div className="note-action-dialog-footer">
          <Button variant="secondary" type="button" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="primary" type="button" onClick={onConfirm}>
            Convert &amp; insert
          </Button>
        </div>
      }
    >
      <div className="note-action-dialog-body">
        <p>Images are only shown in Markdown notes. This note is plain text.</p>
        <p className="note-action-dialog-hint">
          Converting only changes the note&apos;s format label. Your text stays exactly as it is, and
          you can switch back with the TXT/MD button.
        </p>
      </div>
    </Modal>
  );
};

export default ConvertToMarkdownDialog;
