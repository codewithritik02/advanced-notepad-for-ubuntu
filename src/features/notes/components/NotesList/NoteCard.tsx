import React from "react";
import { NoteListItem } from "../../types";
import { Tag } from "../../../../components/ui";
import { highlightText } from "../../../search/utils";

export interface NoteCardProps {
  note: NoteListItem;
  isSelected: boolean;
  onSelect: (noteId: string) => void;
  onToggleFavorite?: (noteId: string) => void;
  searchQuery?: string;
}

export const NoteCard: React.FC<NoteCardProps> = ({
  note,
  isSelected,
  onSelect,
  onToggleFavorite,
  searchQuery,
}) => {
  const displayTitle = note.title?.trim() || "Untitled Note";

  return (
    <article
      id={`note-card-${note.id}`}
      className={`note-card ${isSelected ? "is-selected" : ""}`}
      onClick={() => onSelect(note.id)}
      role="button"
      tabIndex={0}
      draggable={true}
      data-note-id={note.id}
      aria-label={displayTitle}
      aria-selected={isSelected}
      aria-current={isSelected ? "true" : undefined}
      aria-describedby={note.preview ? `note-card-preview-${note.id}` : undefined}
      onDragStart={(e) => {
        e.dataTransfer.setData("application/x-note-id", note.id);
        e.dataTransfer.setData("text/plain", note.id);
        e.dataTransfer.effectAllowed = "move";
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect(note.id);
        }
      }}
    >
      <div className="note-card-header">
        <h3 className="note-card-title" id={`note-card-title-${note.id}`}>
          {highlightText(displayTitle, searchQuery)}
        </h3>
        <button
          type="button"
          className={`note-card-favorite-btn ${note.isFavorite ? "is-favorite" : ""}`}
          title={note.isFavorite ? "Favorite note" : "Add to favorites"}
          aria-label={note.isFavorite ? "Favorite note" : "Add to favorites"}
          aria-pressed={!!note.isFavorite}
          onClick={(e) => {
            e.stopPropagation();
            if (onToggleFavorite) {
              onToggleFavorite(note.id);
            }
          }}
        >
          {note.isFavorite && <span className="sr-only">Favorite note</span>}
          <svg
            width="13"
            height="13"
            viewBox="0 0 24 24"
            fill={note.isFavorite ? "currentColor" : "none"}
            stroke="currentColor"
            strokeWidth="2"
            aria-hidden="true"
          >
            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
          </svg>
        </button>
      </div>

      {note.notebookPath && (
        <div
          className="note-card-notebook"
          title={`Notebook: ${note.notebookPath}`}
          aria-label={`Notebook: ${note.notebookPath}`}
        >
          <svg
            width="11"
            height="11"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            className="note-card-notebook-icon"
            aria-hidden="true"
          >
            <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
            <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
          </svg>
          <span className="note-card-notebook-path">{note.notebookPath}</span>
        </div>
      )}

      {note.preview ? (
        <p className="note-card-preview" id={`note-card-preview-${note.id}`}>
          {highlightText(note.preview, searchQuery)}
        </p>
      ) : null}

      <div className="note-card-footer">
        <span className="note-card-date" aria-label={`Updated ${note.updatedAt}`}>
          {note.updatedAt}
        </span>
        {note.tags && note.tags.length > 0 && (
          <div className="note-card-tags" aria-label="Tags">
            {note.tags.slice(0, 3).map((tag) => (
              <Tag key={tag} name={tag} />
            ))}
            {note.tags.length > 3 && (
              <span className="note-card-tag-overflow">+{note.tags.length - 3}</span>
            )}
          </div>
        )}
      </div>
    </article>
  );
};

export default NoteCard;
