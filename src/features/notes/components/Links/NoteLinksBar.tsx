import React from "react";
import { ParsedInternalLink } from "../../../../utils/linkParser";
import { InternalLinkPill } from "./InternalLinkPill";
import "./InternalLinks.css";

export interface NoteLinksBarProps {
  links: ParsedInternalLink[];
  onActivateLink?: (targetNoteId: string) => void;
  className?: string;
}

const LinkIcon = () => (
  <svg
    width="13"
    height="13"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2.2"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
    <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
  </svg>
);

export const NoteLinksBar: React.FC<NoteLinksBarProps> = ({
  links,
  onActivateLink,
  className = "",
}) => {
  if (!links || links.length === 0) return null;

  return (
    <div
      className={`note-links-bar ${className}`}
      role="region"
      aria-label="Outgoing links in this note"
    >
      <span className="note-links-bar-title">
        <LinkIcon />
        <span>Links ({links.length}):</span>
      </span>
      <div className="note-links-bar-list">
        {links.map((link, idx) => (
          <InternalLinkPill
            key={`bar-link-${link.targetId}-${idx}`}
            targetId={link.targetId}
            label={link.label}
            onActivate={onActivateLink}
          />
        ))}
      </div>
    </div>
  );
};
