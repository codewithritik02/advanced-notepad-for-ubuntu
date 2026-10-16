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
  onExport?: () => void;
  exportLabel?: string;
  onImport?: () => void;
}

const ExportIcon = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" y1="15" x2="12" y2="3" />
  </svg>
);

const ImportIcon = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" y1="15" x2="12" y2="3" />
  </svg>
);

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

export const NotesList = React.memo<NotesListProps>(({
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
  onExport,
  exportLabel,
  onImport,
}) => {
  const isSearchMode = mode === "search";
  const isTrashMode = isTrash || mode === "trash";

  const PAGE_SIZE = 50;
  const [renderedCount, setRenderedCount] = React.useState(PAGE_SIZE);

  // Reset rendered limit when notes collection, search query, or mode changes (Task 9)
  React.useEffect(() => {
    setRenderedCount(PAGE_SIZE);
  }, [notes, searchQuery, mode]);

  // Ensure active/selected note is always rendered and visible (Task 9 selection stability)
  const effectiveCount = React.useMemo(() => {
    if (!selectedNoteId) return renderedCount;
    const idx = notes.findIndex((n) => n.id === selectedNoteId);
    return idx >= 0 ? Math.max(renderedCount, idx + 1) : renderedCount;
  }, [renderedCount, selectedNoteId, notes]);

  const visibleNotes = React.useMemo(() => {
    return notes.slice(0, effectiveCount);
  }, [notes, effectiveCount]);

  const handleScroll = React.useCallback(
    (e: React.UIEvent<HTMLDivElement>) => {
      const target = e.currentTarget;
      if (
        effectiveCount < notes.length &&
        target.scrollTop + target.clientHeight >= target.scrollHeight - 250
      ) {
        setRenderedCount((prev) => Math.min(prev + PAGE_SIZE, notes.length));
      }
    },
    [effectiveCount, notes.length]
  );

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
        <div className="notes-list-header-actions">
          {onImport && !isTrashMode && !isSearchMode && (
            <button
              type="button"
              className="notes-list-action-btn notes-list-import-btn"
              onClick={onImport}
              title="Import notes"
              aria-label="Import notes"
            >
              <ImportIcon />
              <span>Import</span>
            </button>
          )}
          {onExport && !isTrashMode && !isSearchMode && notes.length > 0 && (
            <button
              type="button"
              className="notes-list-action-btn notes-list-export-btn"
              onClick={onExport}
              title={exportLabel || "Export notes"}
              aria-label={exportLabel || "Export notes"}
            >
              <ExportIcon />
              <span>{exportLabel || "Export"}</span>
            </button>
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
      <div className="notes-list-content" onScroll={handleScroll}>
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
              {visibleNotes.map((note) => (
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
});

NotesList.displayName = "NotesList";

export default NotesList;
