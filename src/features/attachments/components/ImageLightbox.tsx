import React, { useEffect, useRef } from "react";
import { AttachmentImage } from "./AttachmentImage.tsx";
import "./AttachmentImage.css";

export interface ImageLightboxProps {
  attachmentId: string;
  alt?: string;
  onClose: () => void;
}

/** Full-size view of an attached image. Esc, the close button, or a click outside closes it. */
export const ImageLightbox: React.FC<ImageLightboxProps> = ({ attachmentId, alt, onClose }) => {
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [onClose]);

  return (
    <div
      className="image-lightbox-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={alt ? `Image: ${alt}` : "Image preview"}
      onClick={onClose}
    >
      <div className="image-lightbox-content" onClick={(event) => event.stopPropagation()}>
        <button
          ref={closeRef}
          type="button"
          className="image-lightbox-close"
          aria-label="Close image"
          title="Close (Esc)"
          onClick={onClose}
        >
          ×
        </button>
        <AttachmentImage attachmentId={attachmentId} alt={alt} variant="full" />
        {alt ? <div className="image-lightbox-caption">{alt}</div> : null}
      </div>
    </div>
  );
};

export default ImageLightbox;
