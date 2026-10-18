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
  mode?: "normal" | "search" | "trash";
  onDelete?: (noteId: string) => void;
  onRestore?: (noteId: string) => void;
  onPermanentlyDelete?: (noteId: string) => void;
}

export const NoteCard = React.memo<NoteCardProps>(({
  note,
  isSelected,
  onSelect,
  onToggleFavorite,
  searchQuery,
  mode = "normal",
  onDelete,
  onRestore,
  onPermanentlyDelete,
}) => {
  const displayTitle = note.title?.trim() || "Untitled Note";
  const isTrashMode = mode === "trash" || !!note.isDeleted;

  return (
    <article
      id={`note-card-${note.id}`}
      className={`note-card ${isSelected ? "is-selected" : ""} ${isTrashMode ? "is-trash-card" : ""}`}
      onClick={() => onSelect(note.id)}
      role="button"
      tabIndex={0}
      draggable={!isTrashMode}
      data-note-id={note.id}
      aria-label={displayTitle}
      aria-selected={isSelected}
      aria-current={isSelected ? "true" : undefined}
      aria-describedby={note.preview ? `note-card-preview-${note.id}` : undefined}
      onDragStart={(e) => {
        if (isTrashMode) {
          e.preventDefault();
          return;
        }
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
        {note.isPinned && !isTrashMode && (
          <span className="note-card-pinned-indicator" title="Pinned note" aria-label="Pinned note">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <line x1="12" y1="17" x2="12" y2="22" />
              <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z" />
            </svg>
          </span>
        )}
        {isTrashMode ? (
          note.isFavorite ? (
            <span
              className="note-card-favorite-indicator"
              title="Favorite note"
              aria-label="Favorite note"
            >
              <svg
                width="13"
                height="13"
                viewBox="0 0 24 24"
                fill="currentColor"
                stroke="currentColor"
                strokeWidth="2"
                aria-hidden="true"
              >
                <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
              </svg>
            </span>
          ) : null
        ) : (
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
        )}
        {!isTrashMode && onDelete && (
          <button
            type="button"
            className="note-card-delete-btn"
            title="Move note to Trash"
            aria-label={`Delete "${displayTitle}"`}
            onClick={(e) => {
              e.stopPropagation();
              onDelete(note.id);
            }}
          >
            <svg
              width="13"
              height="13"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <polyline points="3 6 5 6 21 6" />
              <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
            </svg>
          </button>
        )}
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
        <span
          className="note-card-date"
          aria-label={note.deletedAt ? `Deleted ${note.deletedAt}` : `Updated ${note.updatedAt}`}
        >
          {note.deletedAt ? `Deleted ${note.deletedAt}` : note.updatedAt}
        </span>
        {typeof note.attachmentCount === "number" && note.attachmentCount > 0 && (
          <span
            className="note-card-attachment-indicator"
            title={`${note.attachmentCount} attachment${note.attachmentCount === 1 ? "" : "s"}`}
            aria-label={`${note.attachmentCount} attachment${note.attachmentCount === 1 ? "" : "s"}`}
          >
            <svg
              width="11"
              height="11"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.49-8.49a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
            </svg>
            <span className="note-card-attachment-count">{note.attachmentCount}</span>
          </span>
        )}
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

      {isTrashMode && (onRestore || onPermanentlyDelete) && (
        <div className="note-card-trash-actions" onClick={(e) => e.stopPropagation()}>
          {onRestore && (
            <button
              type="button"
              className="note-card-trash-action-btn note-card-trash-action-btn--restore"
              title="Restore note"
              aria-label={`Restore "${displayTitle}"`}
              onClick={() => onRestore(note.id)}
            >
              <svg
                width="12"
                height="12"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2.2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <polyline points="1 4 1 10 7 10" />
                <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
              </svg>
              <span>Restore</span>
            </button>
          )}
          {onPermanentlyDelete && (
            <button
              type="button"
              className="note-card-trash-action-btn note-card-trash-action-btn--delete"
              title="Permanently delete note"
              aria-label={`Permanently delete "${displayTitle}"`}
              onClick={() => onPermanentlyDelete(note.id)}
            >
              <svg
                width="12"
                height="12"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2.2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
              </svg>
              <span>Delete</span>
            </button>
          )}
        </div>
      )}
    </article>
  );
});

NoteCard.displayName = "NoteCard";

export default NoteCard;
