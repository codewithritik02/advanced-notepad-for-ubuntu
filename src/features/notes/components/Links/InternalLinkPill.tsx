import React from "react";
import "./InternalLinks.css";

export interface InternalLinkPillProps {
  targetId: string;
  label: string;
  status?: "active" | "trashed" | "missing";
  onActivate?: (targetId: string) => void;
  className?: string;
}

const LinkIcon = () => (
  <svg
    width="12"
    height="12"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2.4"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
    <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
  </svg>
);

export const InternalLinkPill: React.FC<InternalLinkPillProps> = ({
  targetId,
  label,
  status = "active",
  onActivate,
  className = "",
}) => {
  const handleClick = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.preventDefault();
    e.stopPropagation();
    onActivate?.(targetId);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      onActivate?.(targetId);
    }
  };

  const getTitle = () => {
    if (status === "trashed") {
      return `Linked note in Trash: "${label}" (${targetId})`;
    }
    if (status === "missing") {
      return `Missing linked note: "${label}" (${targetId})`;
    }
    return `Open linked note: "${label}"`;
  };

  return (
    <button
      type="button"
      className={`internal-link-pill status-${status} ${className}`}
      data-target-id={targetId}
      data-link-status={status}
      title={getTitle()}
      aria-label={`Open linked note: ${label}${status !== "active" ? ` (${status})` : ""}`}
      tabIndex={0}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
    >
      <LinkIcon />
      <span className="internal-link-label">{label}</span>
      {status === "trashed" && <span className="internal-link-status-badge">Trash</span>}
      {status === "missing" && <span className="internal-link-status-badge">Missing</span>}
    </button>
  );
};
