import React from "react";
import { NoteListItem } from "../../types";
import { Tag } from "../../../../components/ui";

export interface NoteCardProps {
  note: NoteListItem;
  isSelected: boolean;
  onSelect: (noteId: string) => void;
}

export const NoteCard: React.FC<NoteCardProps> = ({
  note,
  isSelected,
  onSelect,
}) => {
  return (
    <article
      className={`note-card ${isSelected ? "is-selected" : ""}`}
      onClick={() => onSelect(note.id)}
      role="button"
      tabIndex={0}
      draggable={true}
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
      aria-selected={isSelected}
    >
      <div className="note-card-header">
        <h3 className="note-card-title">{note.title || "Untitled Note"}</h3>
        {note.isFavorite && (
          <span className="note-card-favorite" title="Favorite">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
              <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
            </svg>
          </span>
        )}
      </div>

      <p className="note-card-preview">{note.preview}</p>

      <div className="note-card-footer">
        <span className="note-card-date">{note.updatedAt}</span>
        {note.tags && note.tags.length > 0 && (
          <div className="note-card-tags">
            {note.tags.slice(0, 2).map((tag) => (
              <Tag key={tag} name={tag} />
            ))}
          </div>
        )}
      </div>
    </article>
  );
};

export default NoteCard;
