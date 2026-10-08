import React, { useState, useEffect } from "react";
import { useAttachments } from "../hooks/useAttachments.ts";
import { AttachmentItem } from "./AttachmentItem.tsx";
import { PaperclipIcon, PlusIcon, ChevronDownIcon } from "./AttachmentIcons.tsx";
import "./AttachmentList.css";

export interface AttachmentListProps {
  noteId: string | null;
  isReadOnly?: boolean;
  defaultExpanded?: boolean;
  onAttachmentCountChange?: (count: number) => void;
}

/**
 * Robust Attachment List component for active notes (Phase 7, Tasks 11-14).
 *
 * Responsibilities:
 * - Displays attachments linked to the current active note using `useAttachments(noteId)`.
 * - Safely truncates filenames, presents human-readable sizes and category badges.
 * - Disables file attachment actions when no note is active or when in read-only mode.
 * - Handles asynchronous states (loading, adding with progress, removing, opening).
 * - Preserves note layout via collapsible panel design conforming to project design tokens.
 */
export const AttachmentList: React.FC<AttachmentListProps> = ({
  noteId,
  isReadOnly = false,
  defaultExpanded = true,
  onAttachmentCountChange,
}) => {
  const [isExpanded, setIsExpanded] = useState<boolean>(defaultExpanded);

  const {
    attachments,
    status,
    error,
    progress,
    successMessage,
    isBusy,
    pickAndAddFiles,
    removeAttachment,
    openAttachment,
    clearError,
    clearSuccess,
  } = useAttachments(noteId);

  // Notify parent of attachment count changes
  useEffect(() => {
    onAttachmentCountChange?.(attachments.length);
  }, [attachments.length, onAttachmentCountChange]);

  const handleToggleExpand = () => {
    setIsExpanded((prev) => !prev);
  };

  const handleAddFiles = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!noteId || isBusy || isReadOnly) return;
    pickAndAddFiles();
  };

  // If there is no active note, render disabled placeholder
  if (!noteId) {
    return (
      <section className="attachments-section" aria-label="Attachments">
        <div className="attachments-header">
          <div className="attachments-header-left" style={{ cursor: "default" }}>
            <span className="attachments-header-title">
              <PaperclipIcon size={14} />
              <span>Attachments</span>
            </span>
            <span className="attachments-count-badge">0</span>
          </div>
          <div className="attachments-header-actions">
            <button
              type="button"
              className="attachments-add-btn"
              disabled
              title="Select a note to attach files"
            >
              <PlusIcon size={12} />
              <span>Attach File</span>
            </button>
          </div>
        </div>
      </section>
    );
  }

  const count = attachments.length;

  return (
    <section className="attachments-section" aria-label="Attachments">
      <div className="attachments-header">
        <div
          className="attachments-header-left"
          onClick={handleToggleExpand}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              handleToggleExpand();
            }
          }}
          aria-expanded={isExpanded}
          aria-controls="attachments-panel-content"
          title={isExpanded ? "Collapse attachments" : "Expand attachments"}
        >
          <ChevronDownIcon size={13} isExpanded={isExpanded} />
          <span className="attachments-header-title">
            <PaperclipIcon size={14} />
            <span>Attachments</span>
          </span>
          <span
            className={`attachments-count-badge ${count > 0 ? "has-items" : ""}`}
            title={`${count} attachment${count === 1 ? "" : "s"}`}
          >
            {count}
          </span>
        </div>

        <div className="attachments-header-actions">
          <button
            type="button"
            className="attachments-add-btn"
            disabled={isBusy || isReadOnly}
            title={
              isReadOnly
                ? "Cannot attach files in read-only mode"
                : isBusy
                ? "Attachment operation in progress"
                : "Attach files to this note"
            }
            onClick={handleAddFiles}
          >
            {status === "adding" ? (
              <span className="attachments-spinner" aria-hidden="true" />
            ) : (
              <PlusIcon size={12} />
            )}
            <span>{status === "adding" ? "Attaching..." : "Attach File"}</span>
          </button>
        </div>
      </div>

      {isExpanded && (
        <div id="attachments-panel-content">
          {/* Progress notification banner during multi-file operations */}
          {status === "adding" && progress && (
            <div className="attachments-progress-banner" role="status" aria-live="polite">
              <span className="attachments-spinner" aria-hidden="true" />
              <span>
                Attaching{" "}
                {progress.currentFileName ? `"${progress.currentFileName}"` : "files"}{" "}
                ({progress.current + 1} of {progress.total})...
              </span>
            </div>
          )}

          {/* Accessible Success Notification Banner */}
          {successMessage && (
            <div className="attachments-success-banner" role="status" aria-live="polite">
              <span>✓ {successMessage}</span>
              <button
                type="button"
                className="attachments-success-close"
                title="Dismiss notification"
                aria-label="Dismiss notification"
                onClick={clearSuccess}
              >
                ×
              </button>
            </div>
          )}

          {/* Dismissible Error Banner */}
          {error && (
            <div className="attachments-error-banner" role="alert">
              <span>⚠️ {error.message}</span>
              <button
                type="button"
                className="attachments-error-close"
                title="Dismiss error"
                aria-label="Dismiss error"
                onClick={clearError}
              >
                ×
              </button>
            </div>
          )}

          {/* Attachments List / Empty / Loading State */}
          {status === "loading" && count === 0 ? (
            <div className="attachments-loading-state" role="status" aria-live="polite">
              <span className="attachments-spinner" aria-hidden="true" />
              <span>Loading attachments...</span>
            </div>
          ) : count === 0 ? (
            <div className="attachments-empty-state" role="status">
              No attachments yet. Click &quot;Attach File&quot; to link files to this note.
            </div>
          ) : (
            <div className="attachments-list" role="list">
              {attachments.map((attachment) => (
                <AttachmentItem
                  key={attachment.id}
                  attachment={attachment}
                  onOpen={openAttachment}
                  onRemove={removeAttachment}
                  isBusy={isBusy}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
};

export default AttachmentList;
