import React from "react";
import { Modal, Button } from "../../../../components/ui";
import { ResolvedLinkTarget } from "../../../../services/storage/types";
import "./InternalLinks.css";

export interface BrokenLinkDialogProps {
  target: ResolvedLinkTarget | null;
  onClose: () => void;
  onViewInTrash?: () => void;
  onRestore?: (targetId: string) => void;
  onRepairLink?: (targetId: string) => void;
  onRemoveLink?: (targetId: string) => void;
}

export const BrokenLinkDialog: React.FC<BrokenLinkDialogProps> = ({
  target,
  onClose,
  onViewInTrash,
  onRestore,
  onRepairLink,
  onRemoveLink,
}) => {
  if (!target) return null;

  const isTrashed = target.lifecycle === "trashed";
  const title = isTrashed ? "Linked Note is in Trash" : "Broken Link";
  const noteTitle = target.title || "Untitled Note";

  return (
    <Modal
      isOpen={true}
      onClose={onClose}
      title={title}
      maxWidth="480px"
      footer={
        <div
          className="modal-actions"
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: "8px",
          }}
        >
          <Button variant="secondary" onClick={onClose} data-testid="broken-link-close-btn">
            {isTrashed ? "Cancel" : "Close"}
          </Button>
          {isTrashed && onViewInTrash && (
            <Button variant="secondary" onClick={onViewInTrash} data-testid="broken-link-trash-btn">
              View in Trash
            </Button>
          )}
          {isTrashed && onRestore && (
            <Button
              variant="primary"
              onClick={() => onRestore(target.targetId)}
              data-testid="broken-link-restore-btn"
            >
              Restore Note
            </Button>
          )}
          {!isTrashed && onRemoveLink && (
            <Button
              variant="secondary"
              onClick={() => onRemoveLink(target.targetId)}
              data-testid="broken-link-remove-btn"
            >
              Remove Link
            </Button>
          )}
          {!isTrashed && onRepairLink && (
            <Button
              variant="primary"
              onClick={() => onRepairLink(target.targetId)}
              data-testid="broken-link-repair-btn"
            >
              Change Link...
            </Button>
          )}
        </div>
      }
    >
      <div
        className="broken-link-dialog-body"
        style={{ padding: "8px 0 16px 0", fontSize: "14px", lineHeight: "1.5" }}
      >
        {isTrashed ? (
          <div>
            <p>
              The linked note <strong>&ldquo;{noteTitle}&rdquo;</strong> is currently in Trash.
            </p>
            <p
              style={{
                color: "var(--color-text-secondary, #6b7280)",
                fontSize: "13px",
                marginTop: "8px",
              }}
            >
              You can restore the note to resume editing, inspect it inside the Trash folder, or cancel.
            </p>
          </div>
        ) : (
          <div>
            <p>
              The linked note could not be found (target ID: <code>{target.targetId}</code>).
              It may have been permanently deleted or moved.
            </p>
            <p
              style={{
                color: "var(--color-text-secondary, #6b7280)",
                fontSize: "13px",
                marginTop: "8px",
              }}
            >
              The original link text is preserved. You can safely close this notice, replace the link with another note, or remove the link markup.
            </p>
          </div>
        )}
      </div>
    </Modal>
  );
};

