import React, { useState, useEffect, useRef, useMemo, useCallback } from "react";
import { Modal, Button } from "../../../../components/ui";
import { storageService, Note, Notebook } from "../../../../services/storage";
import "./NotePickerModal.css";

export interface NotePickerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSelectNote: (target: { id: string; title: string; customLabel?: string }) => void;
  currentNoteId?: string | null;
  initialLabel?: string;
}

// Inline search & folder icons for the picker
const SearchIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="11" cy="11" r="8" />
    <line x1="21" y1="21" x2="16.65" y2="16.65" />
  </svg>
);

const FolderIcon = () => (
  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);

export const NotePickerModal: React.FC<NotePickerModalProps> = ({
  isOpen,
  onClose,
  onSelectNote,
  currentNoteId,
  initialLabel = "",
}) => {
  const [searchQuery, setSearchQuery] = useState("");
  const [customLabel, setCustomLabel] = useState(initialLabel);
  const [notes, setNotes] = useState<Note[]>([]);
  const [notebookMap, setNotebookMap] = useState<Map<string, string>>(new Map());
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedIndex, setSelectedIndex] = useState(0);

  const searchInputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Load eligible notes and notebooks when picker opens
  useEffect(() => {
    if (!isOpen) {
      setSearchQuery("");
      setCustomLabel("");
      setSelectedIndex(0);
      return;
    }

    setCustomLabel(initialLabel);
    setSearchQuery("");
    setSelectedIndex(0);
    setIsLoading(true);
    setError(null);

    let isSubscribed = true;

    Promise.all([
      storageService.notes.list({ includeDeleted: false }),
      storageService.notebooks.list().catch(() => [] as Notebook[]),
    ])
      .then(([allNotes, allNotebooks]) => {
        if (!isSubscribed) return;

        // Build notebook ID -> Name map for duplicate-title differentiation
        const map = new Map<string, string>();
        for (const nb of allNotebooks) {
          map.set(nb.id, nb.name);
        }
        setNotebookMap(map);

        // Filter out trashed notes and source note (prevent self-links by default)
        const eligible = allNotes.filter(
          (n) => !n.is_deleted && (!currentNoteId || n.id !== currentNoteId)
        );
        setNotes(eligible);
        setIsLoading(false);
      })
      .catch((err) => {
        if (!isSubscribed) return;
        setError(err instanceof Error ? err.message : "Unable to load notes.");
        setIsLoading(false);
      });

    return () => {
      isSubscribed = false;
    };
  }, [isOpen, currentNoteId, initialLabel]);

  // Focus search input whenever modal opens
  useEffect(() => {
    if (isOpen) {
      requestAnimationFrame(() => {
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
      });
    }
  }, [isOpen]);

  // Filter notes client-side based on search query
  const filteredNotes = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return notes;

    return notes.filter((n) => {
      const titleMatch = (n.title || "").toLowerCase().includes(q);
      const contentMatch = (n.content || "").toLowerCase().includes(q);
      const nbName = n.notebook_id ? notebookMap.get(n.notebook_id)?.toLowerCase() : "";
      const notebookMatch = nbName ? nbName.includes(q) : false;
      return titleMatch || notebookMatch || contentMatch;
    });
  }, [notes, searchQuery, notebookMap]);

  // Reset selected index when filtered list changes
  useEffect(() => {
    setSelectedIndex(0);
  }, [filteredNotes.length, searchQuery]);

  const handleConfirmSelect = useCallback(
    (note: Note) => {
      onSelectNote({
        id: note.id,
        title: note.title.trim() ? note.title.trim() : "Untitled Note",
        customLabel: customLabel.trim() || undefined,
      });
      onClose();
    },
    [onSelectNote, customLabel, onClose]
  );

  // Keyboard navigation inside the picker
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
        return;
      }

      if (filteredNotes.length === 0) return;

      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev + 1) % filteredNotes.length);
        // Ensure scroll into view
        requestAnimationFrame(() => {
          const selectedElem = listRef.current?.children[
            (selectedIndex + 1) % filteredNotes.length
          ] as HTMLElement | undefined;
          selectedElem?.scrollIntoView({ block: "nearest" });
        });
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((prev) =>
          prev - 1 < 0 ? filteredNotes.length - 1 : prev - 1
        );
        requestAnimationFrame(() => {
          const nextIdx = selectedIndex - 1 < 0 ? filteredNotes.length - 1 : selectedIndex - 1;
          const selectedElem = listRef.current?.children[nextIdx] as HTMLElement | undefined;
          selectedElem?.scrollIntoView({ block: "nearest" });
        });
      } else if (e.key === "Enter") {
        e.preventDefault();
        const highlighted = filteredNotes[selectedIndex];
        if (highlighted) {
          handleConfirmSelect(highlighted);
        }
      }
    },
    [filteredNotes, selectedIndex, handleConfirmSelect, onClose]
  );

  if (!isOpen) return null;

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Link to Note"
      maxWidth="500px"
      footer={
        <div className="note-picker-footer">
          <div className="note-picker-shortcut-hint">
            <span>↑↓ to navigate</span>
            <span>•</span>
            <span>Enter to select</span>
            <span>•</span>
            <span>Esc to cancel</span>
          </div>
          <div className="note-picker-footer-actions">
            <Button variant="secondary" onClick={onClose} type="button">
              Cancel
            </Button>
            <Button
              variant="primary"
              disabled={filteredNotes.length === 0}
              onClick={() => {
                const highlighted = filteredNotes[selectedIndex];
                if (highlighted) handleConfirmSelect(highlighted);
              }}
              type="button"
            >
              Insert Link
            </Button>
          </div>
        </div>
      }
    >
      <div className="note-picker-container" onKeyDown={handleKeyDown}>
        {/* Search Input Filter */}
        <div className="note-picker-search-wrap">
          <span className="note-picker-search-icon" aria-hidden="true">
            <SearchIcon />
          </span>
          <input
            ref={searchInputRef}
            type="text"
            className="note-picker-search-input"
            placeholder="Search notes to link..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            aria-label="Search notes to link"
          />
        </div>

        {/* Display Text Label Input */}
        <div className="note-picker-label-field">
          <label className="note-picker-label-text" htmlFor="note-picker-custom-label">
            Link text (display label):
          </label>
          <input
            id="note-picker-custom-label"
            type="text"
            className="note-picker-label-input"
            placeholder="Display label (defaults to target title)..."
            value={customLabel}
            onChange={(e) => setCustomLabel(e.target.value)}
          />
        </div>

        {/* Notes List */}
        <div
          ref={listRef}
          className="note-picker-list"
          role="listbox"
          aria-label="Select target note"
        >
          {isLoading && (
            <div className="note-picker-loading">
              <span>Loading notes...</span>
            </div>
          )}

          {!isLoading && error && (
            <div className="note-picker-error" role="alert">
              <span>{error}</span>
            </div>
          )}

          {!isLoading && !error && filteredNotes.length === 0 && (
            <div className="note-picker-empty">
              <span>
                {searchQuery.trim()
                  ? `No notes found matching "${searchQuery}".`
                  : "No other active notes available to link."}
              </span>
            </div>
          )}

          {!isLoading &&
            !error &&
            filteredNotes.map((note, index) => {
              const isSelected = index === selectedIndex;
              const nbName = note.notebook_id ? notebookMap.get(note.notebook_id) : null;
              const title = note.title.trim() ? note.title.trim() : "Untitled Note";
              const snippet = note.content ? note.content.slice(0, 80).replace(/\s+/g, " ") : "";

              return (
                <button
                  key={note.id}
                  type="button"
                  className={`note-picker-item ${isSelected ? "is-selected" : ""}`}
                  role="option"
                  aria-selected={isSelected}
                  onClick={() => handleConfirmSelect(note)}
                  onMouseEnter={() => setSelectedIndex(index)}
                >
                  <div className="note-picker-item-header">
                    <span className="note-picker-item-title">{title}</span>
                    {nbName && (
                      <span className="note-picker-notebook-tag" title={`Notebook: ${nbName}`}>
                        <FolderIcon />
                        <span>{nbName}</span>
                      </span>
                    )}
                  </div>
                  {snippet && <span className="note-picker-item-preview">{snippet}</span>}
                </button>
              );
            })}
        </div>
      </div>
    </Modal>
  );
};

export default NotePickerModal;
