import React, { useState, useEffect, useCallback, useRef } from "react";
import { BacklinkItem } from "../../../../services/storage/types";
import { storageService } from "../../../../services/storage";
import { LinkFromNotesModal } from "./LinkFromNotesModal";
import { Modal, Button } from "../../../../components/ui";
import { compactInternalLinkSnippet, removeInternalLink } from "../../../../utils/linkParser";
import "./InternalLinks.css";

export interface BacklinksSectionProps {
  noteId: string | null;
  onActivateLink?: (targetNoteId: string, customLabel?: string) => void | Promise<void>;
  isReadOnly?: boolean;
  refreshTrigger?: number | string | null;
  includeHidden?: boolean;
  /** Title of the open note, used as the link label inserted into other notes. */
  noteTitle?: string;
}

function formatBacklinkDate(dateStr?: string): string {
  if (!dateStr) return "";
  try {
    const d = new Date(dateStr);
    if (!isNaN(d.getTime())) {
      return d.toLocaleDateString(undefined, {
        month: "short",
        day: "numeric",
        year: d.getFullYear() !== new Date().getFullYear() ? "numeric" : undefined,
      });
    }
  } catch {
    // ignore parsing failure
  }
  return "";
}

export const BacklinksSection: React.FC<BacklinksSectionProps> = ({
  noteId,
  onActivateLink,
  isReadOnly = false,
  refreshTrigger,
  includeHidden = false,
  noteTitle = "",
}) => {
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [unlinkTarget, setUnlinkTarget] = useState<BacklinkItem | null>(null);
  const [isUnlinking, setIsUnlinking] = useState(false);
  const [unlinkError, setUnlinkError] = useState<string | null>(null);
  const [backlinks, setBacklinks] = useState<BacklinkItem[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isExpanded, setIsExpanded] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Request sequencing counter to discard out-of-order or stale backlink responses
  const activeRequestIdRef = useRef<number>(0);

  const fetchBacklinks = useCallback(async (targetId: string) => {
    const currentRequestId = ++activeRequestIdRef.current;
    setIsLoading(true);
    setError(null);
    try {
      const items = await storageService.links.getBacklinks(targetId, includeHidden);
      if (activeRequestIdRef.current === currentRequestId) {
        setBacklinks(items || []);
      }
    } catch (err) {
      if (activeRequestIdRef.current === currentRequestId) {
        console.error("[BacklinksSection] Failed to fetch backlinks:", err);
        setError("Unable to load backlinks.");
        setBacklinks([]);
      }
    } finally {
      if (activeRequestIdRef.current === currentRequestId) {
        setIsLoading(false);
      }
    }
  }, [includeHidden]);

  useEffect(() => {
    if (!noteId) {
      activeRequestIdRef.current += 1;
      setBacklinks([]);
      setIsLoading(false);
      setError(null);
      return;
    }

    fetchBacklinks(noteId);
  }, [noteId, refreshTrigger, fetchBacklinks]);

  // Invalidate any in-flight backlink requests on unmount
  useEffect(() => {
    return () => {
      activeRequestIdRef.current += 1;
    };
  }, []);

  const handleToggleExpand = () => {
    setIsExpanded((prev) => !prev);
  };

  const handleBacklinkClick = (item: BacklinkItem) => {
    if (onActivateLink) {
      onActivateLink(item.sourceNoteId, item.sourceNoteTitle);
    }
  };

  const handleConfirmUnlink = async () => {
    if (!unlinkTarget || !noteId || isUnlinking) return;
    setIsUnlinking(true);
    setUnlinkError(null);
    try {
      // Re-read the source note so newer edits are never overwritten.
      const fresh = await storageService.notes.get(unlinkTarget.sourceNoteId);
      if (fresh && !fresh.is_deleted) {
        const updated = removeInternalLink(fresh.content || "", noteId);
        if (updated !== fresh.content) {
          await storageService.notes.update(unlinkTarget.sourceNoteId, { content: updated });
        }
      }
      setUnlinkTarget(null);
      await fetchBacklinks(noteId);
    } catch (err) {
      console.error("[BacklinksSection] Failed to unlink:", err);
      setUnlinkError("Could not remove the link. Please try again.");
    } finally {
      setIsUnlinking(false);
    }
  };

  // If there's no note selected, don't display the backlinks section
  if (!noteId) {
    return null;
  }

  return (
    <section className="backlinks-section" data-testid="backlinks-section">
      <div className="backlinks-header">
        <button
          type="button"
          className="backlinks-toggle-btn"
          data-testid="backlinks-toggle"
          onClick={handleToggleExpand}
          aria-expanded={isExpanded}
          title={isExpanded ? "Collapse backlinks" : "Expand backlinks"}
        >
          <svg
            className={`backlinks-chevron ${isExpanded ? "is-expanded" : ""}`}
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <polyline points="9 18 15 12 9 6" />
          </svg>
          <svg
            className="backlinks-icon"
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
            <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
          </svg>
          <span className="backlinks-title">Linked from</span>
          <span className="backlinks-badge" data-testid="backlinks-count">
            {backlinks.length}
          </span>
        </button>

        {!isReadOnly && (
          <button
            type="button"
            className="backlinks-refresh-btn backlinks-add-btn"
            data-testid="backlinks-add-btn"
            onClick={() => setIsPickerOpen(true)}
            title="Link from other notes"
            aria-label="Link from other notes"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="12" y1="5" x2="12" y2="19" />
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
          </button>
        )}
        <button
          type="button"
          className="backlinks-refresh-btn backlinks-refresh-after-add"
          data-testid="backlinks-refresh-btn"
          onClick={() => fetchBacklinks(noteId)}
          title="Refresh incoming links"
          aria-label="Refresh incoming links"
          disabled={isLoading}
        >
          <svg
            className={isLoading ? "is-spinning" : ""}
            width="13"
            height="13"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67" />
          </svg>
        </button>
      </div>

      {isExpanded && (
        <div className="backlinks-content">
          {isLoading ? (
            <div className="backlinks-loading">
              <span className="backlinks-spinner" />
              <span>Loading incoming links...</span>
            </div>
          ) : error ? (
            <div className="backlinks-error">{error}</div>
          ) : backlinks.length === 0 ? (
            <div className="backlinks-empty" data-testid="backlinks-empty">
              <span>No incoming links to this note yet.</span>
            </div>
          ) : (
            <div className="backlinks-list" data-testid="backlinks-list">
              {backlinks.map((item) => (
                <div
                  key={item.sourceNoteId}
                  className="backlink-card"
                  data-testid={`backlink-item-${item.sourceNoteId}`}
                >
                  <div className="backlink-card-top">
                    <button
                      type="button"
                      className="backlink-nav-btn"
                      data-testid={`backlink-nav-${item.sourceNoteId}`}
                      onClick={() => handleBacklinkClick(item)}
                      title={`Open note "${item.sourceNoteTitle}"`}
                    >
                      <span className="backlink-source-title">{item.sourceNoteTitle}</span>
                    </button>
                    {item.sourceNotebookName && (
                      <span
                        className="backlink-notebook-badge"
                        title={`Notebook: ${item.sourceNotebookName}`}
                      >
                        📁 {item.sourceNotebookName}
                      </span>
                    )}
                    {item.modifiedAt && (
                      <span className="backlink-date">
                        {formatBacklinkDate(item.modifiedAt)}
                      </span>
                    )}
                    {!isReadOnly && (
                      <button
                        type="button"
                        className="backlink-unlink-btn"
                        data-testid={`backlink-unlink-${item.sourceNoteId}`}
                        onClick={() => {
                          setUnlinkError(null);
                          setUnlinkTarget(item);
                        }}
                        title="Remove this link"
                        aria-label={`Remove link from ${item.sourceNoteTitle}`}
                      >
                        ✕
                      </button>
                    )}
                  </div>
                  {item.labelSnippet && (
                    <div
                      className="backlink-snippet-preview"
                      onClick={() => handleBacklinkClick(item)}
                      title="Jump to note"
                    >
                      {compactInternalLinkSnippet(item.labelSnippet)}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
      {unlinkTarget && (
        <Modal
          isOpen
          onClose={() => !isUnlinking && setUnlinkTarget(null)}
          title="Remove link"
          maxWidth="440px"
          footer={
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
              <Button variant="secondary" type="button" disabled={isUnlinking} onClick={() => setUnlinkTarget(null)}>
                Cancel
              </Button>
              <Button variant="danger" type="button" disabled={isUnlinking} onClick={handleConfirmUnlink} data-testid="unlink-confirm">
                {isUnlinking ? "Removing..." : "Remove link"}
              </Button>
            </div>
          }
        >
          <p style={{ fontSize: 14, lineHeight: 1.5, margin: 0 }}>
            Remove the link to this note from “{unlinkTarget.sourceNoteTitle || "Untitled Note"}”? The link
            is turned back into plain text; nothing else in that note changes.
          </p>
          {unlinkError && (
            <p role="alert" style={{ color: "var(--color-danger-default, #ef4444)", fontSize: 13, marginTop: 8 }}>
              {unlinkError}
            </p>
          )}
        </Modal>
      )}
      <LinkFromNotesModal
        isOpen={isPickerOpen}
        onClose={() => setIsPickerOpen(false)}
        targetNoteId={noteId}
        targetTitle={noteTitle}
        includeHidden={includeHidden}
        onLinked={() => fetchBacklinks(noteId)}
      />
    </section>
  );
};

export default BacklinksSection;
