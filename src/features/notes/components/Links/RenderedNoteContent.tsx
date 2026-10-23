import React, { useMemo, useState } from "react";
import { tokenizeContentWithLinks } from "../../../../utils/linkParser";
import { splitAttachmentImages } from "../../../../utils/markdown";
import { AttachmentImage, ImageLightbox } from "../../../attachments";
import { InternalLinkPill } from "./InternalLinkPill";
import { RenderedMarkdown } from "./RenderedMarkdown";
import "./InternalLinks.css";

export interface RenderedNoteContentProps {
  content: string;
  onActivateLink?: (targetNoteId: string) => void;
  className?: string;
  placeholder?: string;
  /** Note format; Markdown notes are rendered as formatted content, plain text stays as typed. */
  format?: string;
}

export const RenderedNoteContent: React.FC<RenderedNoteContentProps> = ({
  content,
  onActivateLink,
  className = "",
  placeholder = "No content to preview.",
  format = "txt",
}) => {
  const segments = useMemo(() => {
    return tokenizeContentWithLinks(content);
  }, [content]);
  const [zoomed, setZoomed] = useState<{ id: string; alt: string } | null>(null);

  if (!content) {
    return (
      <div className={`editor-preview-area editor-preview-empty ${className}`}>
        {placeholder}
      </div>
    );
  }

  if (format === "md") {
    return (
      <div
        className={`editor-preview-area editor-preview-markdown selectable-text ${className}`}
        role="region"
        aria-label="Rendered Markdown preview"
      >
        <RenderedMarkdown content={content} onActivateLink={onActivateLink} />
      </div>
    );
  }

  return (
    <div
      className={`editor-preview-area selectable-text ${className}`}
      role="region"
      aria-label="Rendered note preview"
    >
      {segments.map((seg, index) => {
        if (seg.type === "link") {
          return (
            <InternalLinkPill
              key={`link-${seg.targetId}-${index}`}
              targetId={seg.targetId}
              label={seg.label}
              onActivate={onActivateLink}
            />
          );
        }
        // Plain-text notes show attached images too: their `![name](notepad://attachment/id)` lines
        return (
          <React.Fragment key={`text-${index}`}>
            {splitAttachmentImages(seg.text).map((part, partIndex) =>
              part.type === "attachment" ? (
                <span key={`image-${index}-${partIndex}`} className="plain-note-image">
                  <AttachmentImage
                    attachmentId={part.id}
                    alt={part.alt}
                    width={part.width}
                    variant="inline"
                    onOpen={() => setZoomed({ id: part.id, alt: part.alt })}
                  />
                </span>
              ) : (
                <React.Fragment key={`part-${index}-${partIndex}`}>{part.text}</React.Fragment>
              ),
            )}
          </React.Fragment>
        );
      })}
      {zoomed && (
        <ImageLightbox attachmentId={zoomed.id} alt={zoomed.alt} onClose={() => setZoomed(null)} />
      )}
    </div>
  );
};
