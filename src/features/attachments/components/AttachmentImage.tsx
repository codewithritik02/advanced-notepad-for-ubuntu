import React from "react";
import { useAttachmentImage } from "../hooks/useAttachmentImage.ts";
import "./AttachmentImage.css";

export interface AttachmentImageProps {
  attachmentId: string;
  alt?: string;
  /** inline = note preview, thumb = small square in lists/panels, full = lightbox */
  variant?: "inline" | "thumb" | "full";
  /** Requested display width in px (never wider than its container). */
  width?: number;
  onOpen?: () => void;
  /** Shown instead of the "removed" message when a thumbnail cannot load. */
  fallback?: React.ReactNode;
}

/** Shows an image attachment, or a clear placeholder when it was removed or cannot be read. */
export const AttachmentImage: React.FC<AttachmentImageProps> = ({
  attachmentId,
  alt = "",
  variant = "inline",
  width,
  onOpen,
  fallback,
}) => {
  const state = useAttachmentImage(attachmentId);

  if (state.status === "loading") {
    return (
      <span
        className={`attachment-image-skeleton is-${variant}`}
        role="status"
        aria-label="Loading image"
      />
    );
  }

  if (state.status === "error") {
    if (variant === "thumb" && fallback) return <>{fallback}</>;
    return (
      <span
        className={`attachment-image-missing is-${variant}`}
        role="img"
        aria-label={`Image unavailable${alt ? `: ${alt}` : ""}`}
        title={state.message}
      >
        🖼 Image removed or unavailable{alt ? ` (${alt})` : ""}
      </span>
    );
  }

  const interactive = Boolean(onOpen);
  return (
    <img
      className={`attachment-image is-${variant} ${interactive ? "is-zoomable" : ""}`}
      src={state.url}
      alt={alt}
      draggable={false}
      style={variant === "inline" && width ? { width: `${width}px` } : undefined}
      role={interactive ? "button" : undefined}
      tabIndex={interactive ? 0 : undefined}
      onClick={onOpen}
      onKeyDown={
        interactive
          ? (event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onOpen?.();
              }
            }
          : undefined
      }
    />
  );
};

export default AttachmentImage;
