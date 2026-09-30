import React from "react";
import { NoteListItem, NotesListStatus } from "../../types";
import { NoteCard } from "./NoteCard";
import { Badge, EmptyState, ErrorState, LoadingState } from "../../../../components/ui";
import "./NotesList.css";

export interface NotesListProps {
  notes?: NoteListItem[];
  selectedNoteId?: string | null;
  onSelectNote?: (noteId: string) => void;
  status?: NotesListStatus;
  title?: string;
  errorMessage?: string;
  emptyTitle?: string;
  emptyDescription?: string;
  onRetry?: () => void;
  onNewNote?: () => void;
  onToggleFavorite?: (noteId: string) => void;
}

const EmptyNotesIcon = () => (
  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
    <line x1="9" y1="13" x2="15" y2="13" />
    <line x1="9" y1="17" x2="13" y2="17" />
  </svg>
);

export const NotesList: React.FC<NotesListProps> = ({
  notes = [],
  selectedNoteId = null,
  onSelectNote = () => {},
  status = "idle",
  title = "All Notes",
  errorMessage = "Unable to load notes.",
  emptyTitle = "No notes yet",
  emptyDescription = "Create your first note to get started.",
  onRetry,
  onNewNote,
  onToggleFavorite,
}) => {
  return (
    <div className="notes-list-container">
      {/* Panel Header */}
      <div className="notes-list-header">
        <div className="notes-list-title-wrap">
          <h2 className="notes-list-title">{title}</h2>
          {status === "idle" && (
            <Badge variant="default" size="sm">
              {notes.length}
            </Badge>
          )}
        </div>
      </div>

      {/* Panel Body */}
      <div className="notes-list-content">
        {/* Loading State */}
        {status === "loading" && <LoadingState count={4} />}

        {/* Error State */}
        {status === "error" && (
          <ErrorState
            title="Failed to load notes"
            message={errorMessage}
            onRetry={onRetry}
          />
        )}

        {/* Empty State */}
        {(status === "empty" || (status === "idle" && notes.length === 0)) && (
          <EmptyState
            icon={<EmptyNotesIcon />}
            title={emptyTitle}
            description={emptyDescription}
            actionText={onNewNote ? "+ New Note" : undefined}
            onAction={onNewNote}
          />
        )}

        {/* Normal State */}
        {status === "idle" && notes.length > 0 && (
          <div className="notes-list-items" role="feed">
            {notes.map((note) => (
              <NoteCard
                key={note.id}
                note={note}
                isSelected={note.id === selectedNoteId}
                onSelect={onSelectNote}
                onToggleFavorite={onToggleFavorite}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default NotesList;
