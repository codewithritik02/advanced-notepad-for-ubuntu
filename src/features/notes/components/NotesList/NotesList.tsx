import React from "react";
import { NoteListItem, NotesListStatus } from "../../types";
import { NoteCard } from "./NoteCard";
import { Badge, EmptyState, ErrorState, LoadingState } from "../../../../components/ui";
import { SearchErrorBoundary } from "../../../search";
import "./NotesList.css";

export interface NotesListProps {
  notes?: NoteListItem[];
  selectedNoteId?: string | null;
  onSelectNote?: (noteId: string) => void;
  status?: NotesListStatus;
  title?: string;
  errorMessage?: string;
  errorTitle?: string;
  emptyTitle?: string;
  emptyDescription?: string;
  onRetry?: () => void;
  onNewNote?: () => void;
  onToggleFavorite?: (noteId: string) => void;
  mode?: "normal" | "search" | "trash";
  isTrash?: boolean;
  searchQuery?: string;
  resultCountLabel?: string;
  onDeleteNote?: (noteId: string) => void;
  onRestoreNote?: (noteId: string) => void;
  onPermanentlyDeleteNote?: (noteId: string) => void;
  onEmptyTrash?: () => void;
}

const EmptyNotesIcon = () => (
  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
    <line x1="9" y1="13" x2="15" y2="13" />
    <line x1="9" y1="17" x2="13" y2="17" />
  </svg>
);

const EmptyTrashIcon = () => (
  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
    <polyline points="3 6 5 6 21 6" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
  </svg>
);

export const NotesList: React.FC<NotesListProps> = ({
  notes = [],
  selectedNoteId = null,
  onSelectNote = () => {},
  status = "idle",
  title = "All Notes",
  errorMessage,
  errorTitle,
  emptyTitle,
  emptyDescription,
  onRetry,
  onNewNote,
  onToggleFavorite,
  mode = "normal",
  isTrash = false,
  searchQuery = "",
  resultCountLabel,
  onDeleteNote,
  onRestoreNote,
  onPermanentlyDeleteNote,
  onEmptyTrash,
}) => {
  const isSearchMode = mode === "search";
  const isTrashMode = isTrash || mode === "trash";

  const resolvedErrorTitle =
    errorTitle ??
    (isTrashMode
      ? "Failed to load trash"
      : isSearchMode
      ? "Search failed"
      : "Failed to load notes");

  const resolvedErrorMessage =
    errorMessage ??
    (isTrashMode
      ? "Unable to load deleted notes."
      : isSearchMode
      ? "Unable to search notes."
      : "Unable to load notes.");

  const resolvedEmptyTitle =
    emptyTitle ??
    (isTrashMode
      ? "Trash is empty."
      : isSearchMode
      ? "No notes found"
      : "No notes yet");

  const resolvedEmptyDescription =
    emptyDescription ??
    (isTrashMode
      ? "Deleted notes will appear here."
      : isSearchMode
      ? `No notes match "${searchQuery}".`
      : "Create your first note to get started.");

  const effectiveActionText =
    isSearchMode || isTrashMode ? undefined : onNewNote ? "+ New Note" : undefined;
  const effectiveOnAction =
    isSearchMode || isTrashMode ? undefined : onNewNote;

  const countBadgeText = resultCountLabel ?? String(notes.length);

  return (
    <div
      className={`notes-list-container ${isSearchMode ? "is-search-mode" : ""} ${isTrashMode ? "is-trash-mode" : ""}`}
    >
      {/* Panel Header */}
      <div className="notes-list-header">
        <div className="notes-list-title-wrap">
          <h2 className="notes-list-title">{title}</h2>
          {isSearchMode && status === "loading" ? (
            <Badge variant="default" size="sm" aria-live="polite">
              Searching...
            </Badge>
          ) : (
            status === "idle" && (
              <Badge variant={isTrashMode ? "danger" : "default"} size="sm" aria-live="polite">
                {countBadgeText}
              </Badge>
            )
          )}
        </div>
        {isTrashMode && onEmptyTrash && (
          <button
            type="button"
            className="notes-list-empty-trash-btn"
            onClick={onEmptyTrash}
            disabled={notes.length === 0}
            title={notes.length === 0 ? "Trash is already empty" : "Empty Trash"}
            aria-label="Empty Trash"
          >
            Empty Trash
          </button>
        )}
      </div>

      {/* Panel Body */}
      <div className="notes-list-content">
        {/* Loading State */}
        {status === "loading" && (
          <LoadingState count={4} message={isSearchMode ? "Searching..." : undefined} />
        )}

        {/* Error State */}
        {status === "error" && (
          <ErrorState
            title={resolvedErrorTitle}
            message={resolvedErrorMessage}
            onRetry={onRetry}
          />
        )}

        {/* Empty State */}
        {(status === "empty" || (status === "idle" && notes.length === 0)) && (
          <EmptyState
            icon={isTrashMode ? <EmptyTrashIcon /> : <EmptyNotesIcon />}
            title={resolvedEmptyTitle}
            description={resolvedEmptyDescription}
            actionText={effectiveActionText}
            onAction={effectiveOnAction}
          />
        )}

        {/* Normal / Search / Trash Results State */}
        {status === "idle" && notes.length > 0 && (
          <SearchErrorBoundary
            onReset={onRetry}
            fallback={
              <ErrorState
                title="Display error"
                message="Unable to render note list. Your notes are safe."
                onRetry={onRetry}
              />
            }
          >
            <div
              className="notes-list-items"
              role="region"
              aria-label={isTrashMode ? "Trash notes list" : isSearchMode ? "Search results" : "Notes list"}
            >
              {notes.map((note) => (
                <NoteCard
                  key={note.id}
                  note={note}
                  isSelected={note.id === selectedNoteId}
                  onSelect={onSelectNote}
                  onToggleFavorite={onToggleFavorite}
                  searchQuery={isSearchMode ? searchQuery : undefined}
                  mode={mode}
                  onDelete={onDeleteNote}
                  onRestore={onRestoreNote}
                  onPermanentlyDelete={onPermanentlyDeleteNote}
                />
              ))}
            </div>
          </SearchErrorBoundary>
        )}
      </div>
    </div>
  );
};

export default NotesList;
