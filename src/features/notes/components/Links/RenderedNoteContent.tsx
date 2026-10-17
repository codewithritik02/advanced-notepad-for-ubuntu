import React, { useMemo } from "react";
import { tokenizeContentWithLinks } from "../../../../utils/linkParser";
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

  if (!content || content.trim().length === 0) {
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
        return <React.Fragment key={`text-${index}`}>{seg.text}</React.Fragment>;
      })}
    </div>
  );
};
