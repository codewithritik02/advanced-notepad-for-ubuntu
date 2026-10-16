import React, { useState, useEffect, useCallback, useRef } from "react";
import { BacklinkItem } from "../../../../services/storage/types";
import { storageService } from "../../../../services/storage";
import "./InternalLinks.css";

export interface BacklinksSectionProps {
  noteId: string | null;
  onActivateLink?: (targetNoteId: string, customLabel?: string) => void | Promise<void>;
  isReadOnly?: boolean;
  refreshTrigger?: number | string | null;
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
  refreshTrigger,
}) => {
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
      const items = await storageService.links.getBacklinks(targetId);
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
  }, []);

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

        <button
          type="button"
          className="backlinks-refresh-btn"
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
                  </div>
                  {item.labelSnippet && (
                    <div
                      className="backlink-snippet-preview"
                      onClick={() => handleBacklinkClick(item)}
                      title="Jump to note"
                    >
                      {item.labelSnippet}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
};

export default BacklinksSection;
