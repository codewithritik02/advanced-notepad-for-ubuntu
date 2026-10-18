import React from "react";
import "./Sidebar.css";
import { NotebookTree } from "../../features/notebooks/components/NotebookTree";
import type { Notebook, NotebooksStatus } from "../../features/notebooks/types";
import { TagList } from "../../features/tags/components/TagList";
import type { Tag } from "../../features/tags/types";
import { useSidebarState } from "../../layouts/sidebarState";
import type { ExternalDocument } from "../../services/storage/openFiles";

export type NavItemId =
  | "all-notes"
  | "unfiled"
  | "favorites"
  | "notebook"
  | "tags"
  | "trash";

export interface SidebarProps {
  activeNavId?: NavItemId;
  onSelectNav?: (navId: NavItemId) => void;
  onNewNoteClick?: () => void;
  // Notebook integration props
  notebooks?: Notebook[];
  notebooksStatus?: NotebooksStatus;
  notebooksError?: string | null;
  selectedNotebookId?: string | null;
  expandedNotebookIds?: Set<string>;
  onToggleExpandNotebook?: (notebookId: string) => void;
  onSelectNotebook?: (notebookId: string) => void;
  onCreateNotebook?: (parentId?: string | null) => void;
  onRenameNotebook?: (notebook: Notebook) => void;
  onDeleteNotebook?: (notebook: Notebook) => void;
  onExportNotebook?: (notebook: Notebook) => void;
  onImportNotes?: () => void;
  onDropNote?: (noteId: string, destinationNotebookId: string | null) => void;
  onRetryNotebooks?: () => void;
  noteCounts?: Record<string, number>;
  // Tags integration props
  tags?: Tag[];
  selectedTagId?: string | null;
  onSelectTag?: (tagId: string) => void;
  onCreateTag?: () => void;
  onRenameTag?: (tag: Tag) => void;
  onDeleteTag?: (tag: Tag) => void;
  tagCounts?: Record<string, number>;
  onManageTags?: () => void;
  // Files opened from outside the notebook ("Open with"); kept apart from the notes
  externalDocuments?: ExternalDocument[];
  selectedExternalId?: string | null;
  onSelectExternal?: (id: string) => void;
  onCloseExternal?: (id: string) => void;
  onTogglePinExternal?: (id: string) => void;
}

// Inline lightweight SVG icons for zero external dependencies
const PinIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <line x1="12" y1="17" x2="12" y2="22" />
    <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z" />
  </svg>
);

const PlusIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <line x1="12" y1="5" x2="12" y2="19" />
    <line x1="5" y1="12" x2="19" y2="12" />
  </svg>
);

const ImportIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" y1="15" x2="12" y2="3" />
  </svg>
);

const SettingsGearIcon = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
  </svg>
);

const AllNotesIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
    <line x1="16" y1="13" x2="8" y2="13" />
    <line x1="16" y1="17" x2="8" y2="17" />
    <polyline points="10 9 9 9 8 9" />
  </svg>
);

const UnfiledIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
    <line x1="9" y1="13" x2="15" y2="13" />
  </svg>
);

const StarIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
  </svg>
);


const TrashIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="3 6 5 6 21 6" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
  </svg>
);

const ExternalFileIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
    <polyline points="15 3 21 3 21 9" />
    <line x1="10" y1="14" x2="21" y2="3" />
  </svg>
);

const CloseIcon = () => (
  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <line x1="18" y1="6" x2="6" y2="18" />
    <line x1="6" y1="6" x2="18" y2="18" />
  </svg>
);

export const Sidebar: React.FC<SidebarProps> = ({
  activeNavId = "all-notes",
  onSelectNav,
  onNewNoteClick,
  notebooks = [],
  notebooksStatus = "idle",
  notebooksError = null,
  selectedNotebookId = null,
  expandedNotebookIds = new Set(),
  onToggleExpandNotebook = () => {},
  onSelectNotebook = () => {},
  onCreateNotebook,
  onRenameNotebook,
  onDeleteNotebook,
  onExportNotebook,
  onImportNotes,
  onDropNote,
  onRetryNotebooks,
  noteCounts,
  tags = [],
  selectedTagId = null,
  onSelectTag,
  onCreateTag,
  onRenameTag,
  onDeleteTag,
  tagCounts,
  onManageTags,
  externalDocuments = [],
  selectedExternalId = null,
  onSelectExternal,
  onCloseExternal,
  onTogglePinExternal,
}) => {
  const [isUnfiledDragOver, setIsUnfiledDragOver] = React.useState(false);
  const { isCompact, isPinned, togglePin } = useSidebarState();

  const handleNavClick = (navId: NavItemId) => {
    onSelectNav?.(navId);
  };

  const handleUnfiledDragOver = (e: React.DragEvent) => {
    if (e.dataTransfer.types.includes("application/x-note-id")) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      if (!isUnfiledDragOver) setIsUnfiledDragOver(true);
    }
  };

  const handleUnfiledDragLeave = (e: React.DragEvent) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setIsUnfiledDragOver(false);
    }
  };

  const handleUnfiledDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsUnfiledDragOver(false);
    const noteId = e.dataTransfer.getData("application/x-note-id");
    if (noteId && onDropNote) {
      onDropNote(noteId, null);
    }
  };

  return (
    <div className={`sidebar-container ${isCompact ? "is-compact" : ""}`}>
      {/* Brand Header */}
      <div className="sidebar-header">
        <div className="sidebar-brand">
          <div className="sidebar-brand-icon">
            <AllNotesIcon />
          </div>
          <span className="sidebar-brand-name">Personal Notepad</span>
        </div>
        {togglePin && !isCompact && (
          <button
            type="button"
            className={`sidebar-pin-btn ${isPinned ? "is-pinned" : ""}`}
            onClick={togglePin}
            aria-pressed={isPinned}
            aria-label={isPinned ? "Unpin sidebar" : "Pin sidebar open"}
            title={
              isPinned
                ? "Unpin sidebar (it collapses when you move away)"
                : "Pin sidebar open"
            }
          >
            <PinIcon />
          </button>
        )}
      </div>

      {/* Action Area */}
      <div className="sidebar-actions">
        <button
          type="button"
          className="btn-new-note"
          onClick={onNewNoteClick}
          aria-label="Create new note"
          title="New Note"
        >
          <PlusIcon />
          <span>New Note</span>
        </button>
        {onImportNotes && (
          <button
            type="button"
            className="btn-import-notes"
            onClick={onImportNotes}
            aria-label="Import notes"
            title="Import Notes..."
          >
            <ImportIcon />
            <span>Import</span>
          </button>
        )}
      </div>

      {/* Navigation Scrollable Area */}
      <div className="sidebar-nav-scroll">
        {/* Section: Notes */}
        <div className="sidebar-section">
          <div className="sidebar-section-title">Notes</div>
          <ul className="sidebar-nav-list" role="menu">
            <li>
              <button
                type="button"
                className={`sidebar-nav-item ${activeNavId === "all-notes" ? "is-active" : ""}`}
                onClick={() => handleNavClick("all-notes")}
                title="All Notes"
                role="menuitem"
                aria-current={activeNavId === "all-notes" ? "page" : undefined}
              >
                <span className="nav-item-icon"><AllNotesIcon /></span>
                <span className="nav-item-label">All Notes</span>
                {noteCounts?.["all-notes"] !== undefined && noteCounts["all-notes"] > 0 && (
                  <span className="nav-item-count">{noteCounts["all-notes"]}</span>
                )}
              </button>
            </li>
            <li>
              <button
                type="button"
                className={`sidebar-nav-item ${activeNavId === "unfiled" ? "is-active" : ""} ${isUnfiledDragOver ? "is-drag-over" : ""}`}
                onClick={() => handleNavClick("unfiled")}
                onDragOver={handleUnfiledDragOver}
                onDragLeave={handleUnfiledDragLeave}
                onDrop={handleUnfiledDrop}
                title="Unfiled"
                role="menuitem"
                aria-current={activeNavId === "unfiled" ? "page" : undefined}
              >
                <span className="nav-item-icon"><UnfiledIcon /></span>
                <span className="nav-item-label">Unfiled</span>
                {noteCounts?.["unfiled"] !== undefined && noteCounts["unfiled"] > 0 && (
                  <span className="nav-item-count">{noteCounts["unfiled"]}</span>
                )}
              </button>
            </li>
            <li>
              <button
                type="button"
                className={`sidebar-nav-item ${activeNavId === "favorites" ? "is-active" : ""}`}
                onClick={() => handleNavClick("favorites")}
                title="Favorites"
                role="menuitem"
                aria-current={activeNavId === "favorites" ? "page" : undefined}
              >
                <span className="nav-item-icon"><StarIcon /></span>
                <span className="nav-item-label">Favorites</span>
                {noteCounts?.["favorites"] !== undefined && noteCounts["favorites"] > 0 && (
                  <span className="nav-item-count">{noteCounts["favorites"]}</span>
                )}
              </button>
            </li>
          </ul>
        </div>

        {/* Section: External (files opened with "Open with", not part of the notes list) */}
        {externalDocuments.length > 0 && (
          <div className="sidebar-section sidebar-section--external">
            <div className="sidebar-section-title">External</div>
            <ul className="sidebar-nav-list sidebar-external-list">
              {externalDocuments.map((doc) => (
                <li key={doc.id} className="sidebar-external-row">
                  <button
                    type="button"
                    className={`sidebar-nav-item ${selectedExternalId === doc.id ? "is-active" : ""}`}
                    onClick={() => onSelectExternal?.(doc.id)}
                    title={doc.path}
                    aria-current={selectedExternalId === doc.id ? "true" : undefined}
                  >
                    <span className="nav-item-icon"><ExternalFileIcon /></span>
                    <span className="nav-item-label">{doc.fileName}</span>
                  </button>
                  {onTogglePinExternal && (
                    <button
                      type="button"
                      className={`sidebar-external-pin ${doc.pinned ? "is-pinned" : ""}`}
                      onClick={() => onTogglePinExternal(doc.id)}
                      aria-pressed={doc.pinned}
                      aria-label={doc.pinned ? `Unpin ${doc.fileName}` : `Pin ${doc.fileName}`}
                      title={doc.pinned ? "Unpin (it will be cleared on restart)" : "Pin (keep it here for next time)"}
                    >
                      <PinIcon />
                    </button>
                  )}
                  {onCloseExternal && (
                    <button
                      type="button"
                      className="sidebar-external-close"
                      onClick={() => onCloseExternal(doc.id)}
                      aria-label={`Close ${doc.fileName}`}
                      title="Close (the file stays on disk)"
                    >
                      <CloseIcon />
                    </button>
                  )}
                </li>
              ))}
            </ul>
          </div>
        )}

        {/* Section: Notebooks Tree */}
        <div className="sidebar-section sidebar-section--notebooks">
          <div className="sidebar-section-header">
            <span className="sidebar-section-title">Notebooks</span>
            {onCreateNotebook && (
              <button
                type="button"
                className="sidebar-section-action-btn"
                onClick={() => onCreateNotebook(null)}
                aria-label="Create root notebook"
                title="New Notebook"
              >
                <PlusIcon />
              </button>
            )}
          </div>
          <NotebookTree
            notebooks={notebooks}
            status={notebooksStatus}
            error={notebooksError}
            selectedNotebookId={selectedNotebookId}
            expandedNotebookIds={expandedNotebookIds}
            onToggleExpand={onToggleExpandNotebook}
            onSelectNotebook={onSelectNotebook}
            onCreateNotebook={onCreateNotebook}
            onRenameNotebook={onRenameNotebook}
            onDeleteNotebook={onDeleteNotebook}
            onExportNotebook={onExportNotebook}
            onDropNote={onDropNote}
            onRetry={onRetryNotebooks}
            noteCounts={noteCounts}
          />
        </div>

        {/* Section: Tags */}
        <div className="sidebar-section sidebar-section--tags">
          <div className="sidebar-section-header">
            <span
              className="sidebar-section-title"
              style={{ cursor: "pointer" }}
              onClick={() => handleNavClick("tags")}
              title="Show all tagged notes"
            >
              Tags
            </span>
            <div className="sidebar-section-actions">
              {onManageTags && (
                <button
                  type="button"
                  className="sidebar-section-action-btn"
                  onClick={onManageTags}
                  aria-label="Manage tags"
                  title="Manage Tags"
                >
                  <SettingsGearIcon />
                </button>
              )}
              {onCreateTag && (
                <button
                  type="button"
                  className="sidebar-section-action-btn"
                  onClick={onCreateTag}
                  aria-label="Create tag"
                  title="New Tag"
                >
                  <PlusIcon />
                </button>
              )}
            </div>
          </div>
          <TagList
            tags={tags}
            selectedTagId={selectedTagId}
            onSelectTag={(tagId) => {
              if (onSelectTag) {
                onSelectTag(tagId);
              } else {
                handleNavClick("tags");
              }
            }}
            onCreateTag={onCreateTag}
            onRenameTag={onRenameTag}
            onDeleteTag={onDeleteTag}
            tagCounts={tagCounts}
          />
        </div>

        {/* Section: System */}
        <div className="sidebar-section">
          <div className="sidebar-section-title">System</div>
          <ul className="sidebar-nav-list" role="menu">
            <li>
              <button
                type="button"
                className={`sidebar-nav-item sidebar-nav-item--trash ${activeNavId === "trash" ? "is-active" : ""}`}
                onClick={() => handleNavClick("trash")}
                title="Trash"
                role="menuitem"
                aria-current={activeNavId === "trash" ? "page" : undefined}
              >
                <span className="nav-item-icon"><TrashIcon /></span>
                <span className="nav-item-label">Trash</span>
                {noteCounts?.["trash"] !== undefined && noteCounts["trash"] > 0 && (
                  <span className="nav-item-count nav-item-count--trash">{noteCounts["trash"]}</span>
                )}
              </button>
            </li>
          </ul>
        </div>
      </div>
    </div>
  );
};

export default Sidebar;
