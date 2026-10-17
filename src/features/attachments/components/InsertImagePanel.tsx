import React, { useEffect, useRef, useState } from "react";
import type { Attachment } from "../../../services/storage/types.ts";
import { getFileCategory } from "../utils/formatters.ts";
import { AttachmentImage } from "./AttachmentImage.tsx";
import "./InsertImagePanel.css";

export interface InsertImagePanelProps {
  attachments: Attachment[];
  isLoading?: boolean;
  isBusy?: boolean;
  /** Called with the chosen image attachments; the caller places them in the note. */
  onInsert: (images: Attachment[]) => void;
  /** Opens the file picker, attaches the chosen files and resolves with the new attachments. */
  onPickFiles: () => Promise<Attachment[]>;
  onClose: () => void;
}

export const isImageAttachment = (attachment: Attachment): boolean =>
  getFileCategory(attachment.file_name, attachment.mime_type) === "image";

/** Popover listing this note's attached images, with a shortcut to attach new ones. */
export const InsertImagePanel: React.FC<InsertImagePanelProps> = ({
  attachments,
  isLoading = false,
  isBusy = false,
  onInsert,
  onPickFiles,
  onClose,
}) => {
  const panelRef = useRef<HTMLDivElement>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [isPicking, setIsPicking] = useState(false);
  const images = attachments.filter(isImageAttachment);

  useEffect(() => {
    const onMouseDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (!target || panelRef.current?.contains(target)) return;
      // The toolbar button toggles the panel itself.
      if ((target as Element).closest?.("[data-insert-image-toggle]")) return;
      onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    document.addEventListener("mousedown", onMouseDown);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [onClose]);

  const handlePick = async () => {
    if (isPicking || isBusy) return;
    setIsPicking(true);
    setMessage(null);
    try {
      const added = await onPickFiles();
      const newImages = added.filter(isImageAttachment);
      if (newImages.length > 0) {
        onInsert(newImages);
      } else if (added.length > 0) {
        setMessage("Only images can be placed in the note. The file was attached below.");
      }
    } finally {
      setIsPicking(false);
    }
  };

  return (
    <div
      ref={panelRef}
      className="insert-image-panel"
      role="dialog"
      aria-label="Insert image"
    >
      <div className="insert-image-panel-title">Insert image</div>

      {isLoading && images.length === 0 ? (
        <div className="insert-image-panel-empty">Loading images…</div>
      ) : images.length === 0 ? (
        <div className="insert-image-panel-empty">
          No images attached to this note yet.
        </div>
      ) : (
        <div className="insert-image-grid" role="list">
          {images.map((image) => (
            <button
              key={image.id}
              type="button"
              role="listitem"
              className="insert-image-tile"
              title={`Insert ${image.file_name}`}
              aria-label={`Insert ${image.file_name}`}
              onClick={() => onInsert([image])}
            >
              <span className="insert-image-thumb">
                <AttachmentImage
                  attachmentId={image.id}
                  alt={image.file_name}
                  variant="thumb"
                  fallback={<span aria-hidden="true">🖼</span>}
                />
              </span>
              <span className="insert-image-name">{image.file_name}</span>
            </button>
          ))}
        </div>
      )}

      {message && (
        <div className="insert-image-panel-message" role="status">
          {message}
        </div>
      )}

      <div className="insert-image-panel-footer">
        <button
          type="button"
          className="insert-image-pick-btn"
          onClick={() => void handlePick()}
          disabled={isPicking || isBusy}
        >
          {isPicking ? "Attaching…" : "Choose from computer…"}
        </button>
        <span className="insert-image-hint">Images show in Preview (eye icon)</span>
      </div>
    </div>
  );
};

export default InsertImagePanel;
