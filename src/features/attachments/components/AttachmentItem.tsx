import React from "react";
import type { Attachment } from "../../../services/storage/types.ts";
import { formatFileSize, getFileCategory, formatAttachmentDate } from "../utils/formatters.ts";
import { AttachmentCategoryIcon, ExternalLinkIcon, TrashIcon } from "./AttachmentIcons.tsx";

export interface AttachmentItemProps {
  attachment: Attachment;
  onOpen: (id: string) => Promise<unknown> | unknown;
  onRemove: (id: string) => Promise<unknown> | unknown;
  isBusy?: boolean;
}

export const AttachmentItem: React.FC<AttachmentItemProps> = ({
  attachment,
  onOpen,
  onRemove,
  isBusy = false,
}) => {
  const category = getFileCategory(attachment.file_name, attachment.mime_type);
  const sizeFormatted = formatFileSize(attachment.file_size);
  const dateFormatted = formatAttachmentDate(attachment.created_at);
  const [isConfirming, setIsConfirming] = React.useState(false);

  const handleConfirmRemove = async () => {
    setIsConfirming(false);
    await onRemove(attachment.id);
  };

  return (
    <div className="attachment-item" role="listitem">
      <div className="attachment-item-leading" title={`File type: ${category}`}>
        <span className={`attachment-type-badge type-${category}`}>
          <AttachmentCategoryIcon category={category} size={15} />
        </span>
      </div>

      <div className="attachment-item-info">
        <span
          className="attachment-item-name selectable-text"
          title={attachment.file_name}
        >
          {attachment.file_name}
        </span>
        <div className="attachment-item-submeta">
          <span className="attachment-item-size">{sizeFormatted}</span>
          {dateFormatted && (
            <>
              <span className="attachment-meta-dot" aria-hidden="true">
                •
              </span>
              <span className="attachment-item-date">{dateFormatted}</span>
            </>
          )}
        </div>
      </div>

      <div className="attachment-item-actions">
        {isConfirming ? (
          <div className="attachment-confirm-actions">
            <button
              type="button"
              className="attachment-action-btn btn-confirm-remove"
              title={`Confirm removal of ${attachment.file_name}`}
              aria-label={`Confirm remove ${attachment.file_name}`}
              disabled={isBusy}
              onClick={handleConfirmRemove}
            >
              <span className="action-text">Delete</span>
            </button>
            <button
              type="button"
              className="attachment-action-btn btn-cancel-remove"
              title="Cancel removal"
              aria-label="Cancel removal"
              disabled={isBusy}
              onClick={() => setIsConfirming(false)}
            >
              <span className="action-text">Cancel</span>
            </button>
          </div>
        ) : (
          <>
            <button
              type="button"
              className="attachment-action-btn btn-open"
              title={`Open ${attachment.file_name} with default application`}
              aria-label={`Open ${attachment.file_name}`}
              disabled={isBusy}
              onClick={() => onOpen(attachment.id)}
            >
              <ExternalLinkIcon size={13} />
              <span className="action-text">Open</span>
            </button>

            <button
              type="button"
              className="attachment-action-btn btn-remove"
              title={`Remove ${attachment.file_name}`}
              aria-label={`Remove ${attachment.file_name}`}
              disabled={isBusy}
              onClick={() => setIsConfirming(true)}
            >
              <TrashIcon size={13} />
              <span className="action-text">Remove</span>
            </button>
          </>
        )}
      </div>
    </div>
  );
};

export default AttachmentItem;
