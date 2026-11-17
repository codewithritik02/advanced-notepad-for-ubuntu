import React, { useEffect, useMemo, useState } from "react";
import { Modal, Button } from "../../../../components/ui";
import { storageService, Note, Notebook } from "../../../../services/storage";
import { appendInternalLinkToContent, parseInternalLinks } from "../../../../utils/linkParser";
import "../NotePicker/NotePickerModal.css";
import "./InternalLinks.css";

export interface LinkFromNotesModalProps {
  isOpen: boolean;
  onClose: () => void;
  /** The note that will be linked TO (the currently open note). */
  targetNoteId: string;
  targetTitle: string;
  includeHidden?: boolean;
  /** Called after at least one source note was updated. */
  onLinked?: (count: number) => void;
}

export const LinkFromNotesModal: React.FC<LinkFromNotesModalProps> = ({
  isOpen,
  onClose,
  targetNoteId,
  targetTitle,
  includeHidden = false,
  onLinked,
}) => {
  const [notes, setNotes] = useState<Note[]>([]);
  const [notebookMap, setNotebookMap] = useState<Map<string, string>>(new Map());
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) {
      setQuery("");
      setSelected(new Set());
      setError(null);
      return;
    }
    let active = true;
    setIsLoading(true);
    setError(null);
    Promise.all([
      storageService.notes.list({ includeDeleted: false, includeHidden }),
      storageService.notebooks.list().catch(() => [] as Notebook[]),
    ])
      .then(([all, notebooks]) => {
        if (!active) return;
        setNotebookMap(new Map(notebooks.map((nb) => [nb.id, nb.name])));
        setNotes(all.filter((n) => !n.is_deleted && n.id !== targetNoteId));
        setIsLoading(false);
      })
      .catch((err) => {
        if (!active) return;
        setError(err instanceof Error ? err.message : "Unable to load notes.");
        setIsLoading(false);
      });
    return () => {
      active = false;
    };
  }, [isOpen, targetNoteId, includeHidden]);

  const alreadyLinked = useMemo(() => {
    const ids = new Set<string>();
    const needle = targetNoteId.toLowerCase();
    for (const n of notes) {
      if (parseInternalLinks(n.content || "").some((l) => l.targetId.toLowerCase() === needle)) {
        ids.add(n.id);
      }
    }
    return ids;
  }, [notes, targetNoteId]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return notes;
    return notes.filter((n) => {
      const nb = n.notebook_id ? notebookMap.get(n.notebook_id)?.toLowerCase() ?? "" : "";
      return (n.title || "").toLowerCase().includes(q) || nb.includes(q);
    });
  }, [notes, query, notebookMap]);

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const handleConfirm = async () => {
    if (selected.size === 0 || isSaving) return;
    setIsSaving(true);
    setError(null);
    let done = 0;
    const failed: string[] = [];
    for (const id of selected) {
      try {
        // Re-read the note so we never overwrite newer content with a stale list copy.
        const fresh = await storageService.notes.get(id);
        if (!fresh || fresh.is_deleted) {
          failed.push(id);
          continue;
        }
        const updated = appendInternalLinkToContent(fresh.content || "", targetNoteId, targetTitle);
        if (updated !== null) {
          await storageService.notes.update(id, { content: updated });
        }
        done += 1;
      } catch (err) {
        console.error("[LinkFromNotes] Failed to link note", id, err);
        failed.push(id);
      }
    }
    setIsSaving(false);
    if (done > 0) onLinked?.(done);
    if (failed.length > 0) {
      setError(`Could not update ${failed.length} note(s). The others were linked.`);
      setSelected(new Set(failed));
      return;
    }
    onClose();
  };

  if (!isOpen) return null;

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Link from notes"
      maxWidth="520px"
      footer={
        <div className="note-picker-footer">
          <div className="note-picker-shortcut-hint">
            <span>{selected.size} selected</span>
          </div>
          <div className="note-picker-footer-actions">
            <Button variant="secondary" onClick={onClose} type="button" disabled={isSaving}>
              Cancel
            </Button>
            <Button
              variant="primary"
              type="button"
              disabled={selected.size === 0 || isSaving}
              onClick={handleConfirm}
              data-testid="link-from-confirm"
            >
              {isSaving ? "Linking..." : `Link ${selected.size || ""}`.trim()}
            </Button>
          </div>
        </div>
      }
    >
      <div className="note-picker-container">
        <p className="note-picker-label-text">
          Select the notes that should link to “{targetTitle.trim() || "Untitled Note"}”. A link is
          added at the end of each selected note.
        </p>
        <div className="note-picker-search-wrap">
          <input
            type="text"
            className="note-picker-search-input"
            placeholder="Search notes..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label="Search notes to link from"
            autoFocus
          />
        </div>
        <div className="note-picker-list" role="listbox" aria-multiselectable="true">
          {isLoading && <div className="note-picker-loading">Loading notes...</div>}
          {!isLoading && filtered.length === 0 && (
            <div className="note-picker-empty">
              {query.trim() ? `No notes found matching "${query}".` : "No other notes available."}
            </div>
          )}
          {!isLoading &&
            filtered.map((note) => {
              const linked = alreadyLinked.has(note.id);
              const checked = selected.has(note.id);
              const nb = note.notebook_id ? notebookMap.get(note.notebook_id) : null;
              return (
                <label
                  key={note.id}
                  className={`note-picker-item ${checked ? "is-selected" : ""}`}
                  style={linked ? { opacity: 0.55, cursor: "default" } : undefined}
                  data-testid={`link-from-option-${note.id}`}
                >
                  <div className="note-picker-item-header">
                    <span style={{ display: "inline-flex", alignItems: "center", gap: 8, minWidth: 0 }}>
                      <input
                        type="checkbox"
                        checked={checked || linked}
                        disabled={linked}
                        onChange={() => toggle(note.id)}
                      />
                      <span className="note-picker-item-title">
                        {note.title.trim() || "Untitled Note"}
                      </span>
                    </span>
                    {linked ? (
                      <span className="note-picker-notebook-tag">Already linked</span>
                    ) : (
                      nb && <span className="note-picker-notebook-tag">📁 {nb}</span>
                    )}
                  </div>
                </label>
              );
            })}
        </div>
        {error && (
          <div className="note-picker-error" role="alert">
            {error}
          </div>
        )}
      </div>
    </Modal>
  );
};

export default LinkFromNotesModal;
