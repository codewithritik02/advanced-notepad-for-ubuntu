import React, { useState, useEffect, useRef } from "react";
import { Modal, Button } from "../../../components/ui";
import "./ConfirmNoteActionDialog.css";

export type NoteConfirmAction = "trash" | "restore";

export interface NoteConfirmTarget {
  action: NoteConfirmAction;
  id: string;
  title: string;
}

export interface ConfirmNoteActionDialogProps {
  target: NoteConfirmTarget | null;
  onClose: () => void;
  onConfirm: (target: NoteConfirmTarget) => Promise<void> | void;
}

const COPY: Record<
  NoteConfirmAction,
  { heading: string; body: string; hint: string; confirmLabel: string; variant: "danger" | "primary" }
> = {
  trash: {
    heading: "Move to Trash?",
    body: "This note will be moved to the Trash.",
    hint: "You can restore it from the Trash any time until you delete it permanently.",
    confirmLabel: "Move to Trash",
    variant: "danger",
  },
  restore: {
    heading: "Restore this note?",
    body: "This note will be moved out of the Trash and back into your notes.",
    hint: "It returns to its original notebook if that notebook still exists, otherwise it becomes unfiled.",
    confirmLabel: "Restore",
    variant: "primary",
  },
};

/**
 * Confirmation step for moving a note to the Trash and for restoring it from the Trash.
 * Enter confirms, Escape / Cancel dismisses. The action only runs after an explicit confirm.
 */
export const ConfirmNoteActionDialog: React.FC<ConfirmNoteActionDialogProps> = ({
  target,
  onClose,
  onConfirm,
}) => {
  const [isWorking, setIsWorking] = useState(false);
  const workingRef = useRef(false);
  const targetRef = useRef(target);
  targetRef.current = target;

  const confirm = async () => {
    const current = targetRef.current;
    if (!current || workingRef.current) return;
    workingRef.current = true;
    setIsWorking(true);
    try {
      await onConfirm(current);
    } finally {
      workingRef.current = false;
      setIsWorking(false);
      onClose();
    }
  };

  const isOpen = target !== null;

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      // `repeat` ignores a held-down Enter that opened the dialog from a focused button.
      if (event.key === "Enter" && !event.repeat) {
        event.preventDefault();
        void confirm();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, target?.id, target?.action]);

  if (!target) return null;

  const copy = COPY[target.action];
  const displayTitle = target.title.trim() ? target.title.trim() : "Untitled Note";

  return (
    <Modal
      isOpen
      onClose={isWorking ? () => {} : onClose}
      title={copy.heading}
      maxWidth="440px"
      footer={
        <div className="note-action-dialog-footer">
          <Button variant="secondary" type="button" onClick={onClose} disabled={isWorking}>
            Cancel
          </Button>
          <Button
            variant={copy.variant}
            type="button"
            onClick={() => void confirm()}
            isLoading={isWorking}
            disabled={isWorking}
          >
            {copy.confirmLabel}
          </Button>
        </div>
      }
    >
      <div className="note-action-dialog-body">
        <p className="note-action-dialog-title" title={displayTitle}>
          {displayTitle}
        </p>
        <p>{copy.body}</p>
        <p className="note-action-dialog-hint">{copy.hint}</p>
      </div>
    </Modal>
  );
};

export default ConfirmNoteActionDialog;
