import React from "react";
import type { Tag } from "../../../../services/storage";
import { getFormatDisplayName } from "../../types";
import "./NoteMetadataPanel.css";

export interface NoteMetadataPanelProps {
  isOpen: boolean;
  onToggle: () => void;
  notebookPath?: string | null;
  noteTags?: Tag[];
  createdAt?: string | null;
  modifiedAt?: string | null;
  format?: string;
  isFavorite?: boolean;
  wordCount?: number;
  characterCount?: number;
  byteSize?: number;
  onMoveNote?: () => void;
  onToggleFavorite?: () => void;
}

function formatTimestamp(isoString?: string | null): string {
  if (!isoString) return "Unknown";
  try {
    const d = new Date(isoString);
    if (isNaN(d.getTime())) return isoString;
    return d.toLocaleString("en-GB", {
      day: "numeric",
      month: "short",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  } catch {
    return isoString;
  }
}

export const NoteMetadataPanel: React.FC<NoteMetadataPanelProps> = ({
  isOpen,
  onToggle,
  notebookPath = "Unfiled",
  noteTags = [],
  createdAt,
  modifiedAt,
  format = "txt",
  isFavorite = false,
  wordCount = 0,
  characterCount = 0,
  byteSize = 0,
  onMoveNote,
  onToggleFavorite,
}) => {
  return (
    <div className={`note-metadata-panel ${isOpen ? "is-open" : "is-collapsed"}`}>
      <button
        type="button"
        className="note-metadata-toggle-btn"
        onClick={onToggle}
        aria-expanded={isOpen}
        aria-label="Toggle note details"
      >
        <span className="note-metadata-header-title">NOTE DETAILS</span>
        <span className="note-metadata-chevron">{isOpen ? "▲" : "▼"}</span>
      </button>

      {isOpen && (
        <div className="note-metadata-content" role="region" aria-label="Note metadata details">
          <div className="note-metadata-grid">
            <div className="note-metadata-row">
              <span className="note-metadata-label">Notebook</span>
              <span className="note-metadata-value">
                {notebookPath || "Unfiled"}
                {onMoveNote && (
                  <button
                    type="button"
                    className="note-metadata-action-btn"
                    onClick={onMoveNote}
                    title="Move note to different notebook"
                    aria-label="Move note to different notebook"
                  >
                    Move
                  </button>
                )}
              </span>
            </div>

            <div className="note-metadata-row">
              <span className="note-metadata-label">Tags</span>
              <span className="note-metadata-value">
                {noteTags.length > 0 ? (
                  <span className="note-metadata-tags">
                    {noteTags.map((tag) => (
                      <span key={tag.id} className="note-metadata-tag-chip">
                        #{tag.name}
                      </span>
                    ))}
                  </span>
                ) : (
                  <span className="note-metadata-muted">None</span>
                )}
              </span>
            </div>

            <div className="note-metadata-row is-informational">
              <span className="note-metadata-label">Created</span>
              <span className="note-metadata-value">{formatTimestamp(createdAt)}</span>
            </div>

            <div className="note-metadata-row is-informational">
              <span className="note-metadata-label">Modified</span>
              <span className="note-metadata-value">{formatTimestamp(modifiedAt)}</span>
            </div>

            <div className="note-metadata-row is-informational">
              <span className="note-metadata-label">Format</span>
              <span className="note-metadata-value">
                {getFormatDisplayName(format)}
              </span>
            </div>

            <div className="note-metadata-row">
              <span className="note-metadata-label">Favorite</span>
              <span className="note-metadata-value">
                {isFavorite ? "Yes" : "No"}
                {onToggleFavorite && (
                  <button
                    type="button"
                    className="note-metadata-action-btn"
                    onClick={onToggleFavorite}
                    title={isFavorite ? "Remove from favorites" : "Add to favorites"}
                    aria-label={isFavorite ? "Remove from favorites" : "Add to favorites"}
                    aria-pressed={isFavorite}
                  >
                    {isFavorite ? "Unstar" : "Star"}
                  </button>
                )}
              </span>
            </div>

            <div className="note-metadata-row is-informational">
              <span className="note-metadata-label">Length</span>
              <span className="note-metadata-value">
                {wordCount} words, {characterCount} chars ({byteSize} bytes)
              </span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default NoteMetadataPanel;
