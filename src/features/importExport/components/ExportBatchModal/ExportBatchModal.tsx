import React, { useEffect, useState, useMemo } from "react";
import {
  ExportBatchRequest,
  ExportFormat,
  ExportProgress,
  ExportScope,
  ExportSelectionPreview,
  ExportSummary,
} from "../../../../services/storage/types";
import { importExportStorage } from "../../../../services/storage/importExport";
import {
  pickExportDestinationFile,
  pickExportDestinationFolder,
} from "../../services/fileDialog";
import { sanitizeExportFilename } from "../ExportNoteModal/ExportNoteModal";
import { OperationSummaryModal } from "../OperationSummaryModal/OperationSummaryModal";
import "./ExportBatchModal.css";

export interface ExportBatchModalProps {
  isOpen: boolean;
  onClose: () => void;
  scope: ExportScope;
  title?: string;
  notebookName?: string;
  onFlushSave?: () => Promise<void>;
  onSuccess?: (summary: ExportSummary) => void;
}

export const ExportBatchModal: React.FC<ExportBatchModalProps> = ({
  isOpen,
  onClose,
  scope,
  title,
  notebookName,
  onFlushSave,
  onSuccess,
}) => {
  const [includeNested, setIncludeNested] = useState(true);
  const [selectedFormat, setSelectedFormat] = useState<ExportFormat>("json");
  const [preview, setPreview] = useState<ExportSelectionPreview | null>(null);
  const [isLoadingPreview, setIsLoadingPreview] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [activeOperationId, setActiveOperationId] = useState<string | null>(null);
  const [exportProgress, setExportProgress] = useState<ExportProgress | null>(null);
  const [summary, setSummary] = useState<ExportSummary | null>(null);
  const [isCancelling, setIsCancelling] = useState(false);

  // Compute active effective scope based on includeNested toggle
  const effectiveScope: ExportScope = useMemo(() => {
    if (scope.type === "notebook") {
      return {
        type: "notebook",
        notebook_id: scope.notebook_id,
        include_nested: includeNested,
      };
    }
    return scope;
  }, [scope, includeNested]);

  // Load selection preview from backend whenever modal opens or effective scope changes
  useEffect(() => {
    if (!isOpen) {
      setPreview(null);
      setErrorMessage(null);
      return;
    }

    let isMounted = true;
    setIsLoadingPreview(true);
    setErrorMessage(null);

    importExportStorage
      .previewExportSelection(effectiveScope)
      .then((data) => {
        if (isMounted) {
          setPreview(data);
          setIsLoadingPreview(false);
        }
      })
      .catch((err) => {
        if (isMounted) {
          setIsLoadingPreview(false);
          setErrorMessage(
            err instanceof Error ? err.message : String(err) || "Failed to load export preview."
          );
        }
      });

    return () => {
      isMounted = false;
    };
  }, [isOpen, effectiveScope]);

  if (!isOpen) return null;

  const resolvedTitle =
    title ||
    (scope.type === "notebook"
      ? `Export Notebook: ${notebookName || "Notebook"}`
      : scope.type === "all_notes"
      ? "Export All Notes"
      : "Export Selected Notes");

  const defaultBaseName = sanitizeExportFilename(
    notebookName || (scope.type === "all_notes" ? "all-notes" : "notes-export")
  );

  const hasMarkdownNotes = preview?.notes.some((n) => n.format === "md") ?? false;
  const showMarkdownTxtWarning = selectedFormat === "txt" && hasMarkdownNotes;

  const handleCancel = async () => {
    if (!activeOperationId || isCancelling) return;
    setIsCancelling(true);
    try {
      await importExportStorage.cancelOperation(activeOperationId);
    } catch (err) {
      console.error("Failed to cancel export operation:", err);
    }
  };

  const handleExport = async () => {
    if (!preview || preview.totalIncluded === 0) return;

    setErrorMessage(null);

    try {
      // 1. Flush any pending unsaved editor changes before export
      if (onFlushSave) {
        await onFlushSave();
      }

      let destinationDir = "";
      let destinationFile: string | null = null;

      if (selectedFormat === "json") {
        // Structured interchange: pick single target JSON file
        const defaultFileName = `${defaultBaseName}.json`;
        const pickerRes = await pickExportDestinationFile(defaultFileName, "json");
        if (pickerRes.cancelled || !pickerRes.path) {
          return;
        }
        destinationFile = pickerRes.path;
      } else {
        // Multi-file export (.txt or .md): pick output destination folder
        const pickerRes = await pickExportDestinationFolder();
        if (pickerRes.cancelled || !pickerRes.path) {
          return;
        }
        destinationDir = pickerRes.path;
      }

      const opId = `export-${Date.now()}-${Math.random().toString(36).substring(2, 7)}`;
      setActiveOperationId(opId);
      setExportProgress(null);
      setSummary(null);
      setIsCancelling(false);
      setIsExporting(true);

      const unlisten = await importExportStorage.onExportProgress((p) => {
        if (!p.operationId || p.operationId === opId) {
          setExportProgress(p);
        }
      });

      // 2. Execute backend batch export
      const request: ExportBatchRequest = {
        operationId: opId,
        scope: effectiveScope,
        format: selectedFormat,
        destinationDir,
        destinationFile,
        includeMetadata: true,
        overwrite: selectedFormat === "json",
      };

      try {
        const res = await importExportStorage.exportNotesBatch(request);
        setSummary(res);
      } finally {
        setIsExporting(false);
        unlisten();
      }
    } catch (err) {
      setIsExporting(false);
      setErrorMessage(
        err instanceof Error ? err.message : String(err) || "Failed to export notes batch."
      );
    }
  };

  return (
    <div
      className="export-batch-modal-overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="export-batch-modal-title"
    >
      <div className="export-batch-modal-dialog">
        <div className="export-batch-modal-header">
          <div className="export-batch-header-title-wrap">
            <h3 id="export-batch-modal-title">{resolvedTitle}</h3>
            {scope.type === "notebook" && (
              <span className="export-batch-scope-badge">Notebook Export</span>
            )}
          </div>
          <button
            type="button"
            className="export-batch-modal-close-btn"
            aria-label="Close export dialog"
            onClick={onClose}
          >
            ✕
          </button>
        </div>

        <div className="export-batch-modal-body">
          {/* Options row for Notebook scope: Include nested */}
          {scope.type === "notebook" && (
            <div className="export-batch-option-row">
              <label className="export-batch-checkbox-label">
                <input
                  type="checkbox"
                  checked={includeNested}
                  onChange={(e) => setIncludeNested(e.target.checked)}
                  disabled={isLoadingPreview || isExporting}
                />
                <span className="export-batch-checkbox-text">
                  Include notes from nested sub-notebooks
                </span>
              </label>
              <span className="export-batch-option-hint">
                {includeNested
                  ? "Includes all child and grandchild notebooks."
                  : "Exports notes located directly in this notebook only."}
              </span>
            </div>
          )}

          {/* Selection Counts & Status Banner */}
          {isLoadingPreview ? (
            <div className="export-batch-loading">Calculating export selection...</div>
          ) : preview ? (
            <div className="export-batch-stats-banner">
              <div className="export-batch-stat-item">
                <span className="stat-count">{preview.totalIncluded}</span>
                <span className="stat-label">Notes included</span>
              </div>
              {preview.skippedTrashedCount > 0 && (
                <div className="export-batch-stat-item is-skipped">
                  <span className="stat-count">{preview.skippedTrashedCount}</span>
                  <span className="stat-label">Trashed (excluded)</span>
                </div>
              )}
              {preview.skippedMissingCount > 0 && (
                <div className="export-batch-stat-item is-skipped">
                  <span className="stat-count">{preview.skippedMissingCount}</span>
                  <span className="stat-label">Unavailable</span>
                </div>
              )}
              {scope.type === "notebook" && preview.includedNotebookCount > 1 && (
                <div className="export-batch-stat-item">
                  <span className="stat-count">{preview.includedNotebookCount}</span>
                  <span className="stat-label">Notebooks</span>
                </div>
              )}
            </div>
          ) : null}

          {/* Notes Preview List */}
          {preview && preview.notes.length > 0 && (
            <div className="export-batch-notes-preview-container">
              <div className="export-batch-notes-preview-header">
                <span>Included Notes ({preview.notes.length})</span>
              </div>
              <div className="export-batch-notes-list" role="list">
                {preview.notes.map((note) => (
                  <div key={note.id} className="export-batch-note-item" role="listitem">
                    <div className="export-batch-note-main">
                      <span className="export-batch-note-title">
                        {note.title.trim() ? note.title : "Untitled Note"}
                      </span>
                      {note.notebookPath && (
                        <span className="export-batch-note-path">{note.notebookPath}</span>
                      )}
                    </div>
                    <div className="export-batch-note-meta">
                      {note.tagCount > 0 && (
                        <span className="export-batch-meta-tag">{note.tagCount} tags</span>
                      )}
                      <span className="export-batch-format-pill">{note.format.toUpperCase()}</span>
                      <span className="export-batch-size-pill">
                        {(note.contentLengthBytes / 1024).toFixed(1)} KB
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {preview && preview.totalIncluded === 0 && !isLoadingPreview && (
            <div className="export-batch-empty-notice">
              No active notes found in this selection to export. Trashed notes are excluded.
            </div>
          )}

          {/* Format Selection Chips */}
          <div className="export-batch-section">
            <span className="export-batch-section-label">Export Format</span>
            <div className="export-batch-format-chips">
              <button
                type="button"
                className={`export-format-chip ${selectedFormat === "json" ? "is-selected" : ""}`}
                onClick={() => setSelectedFormat("json")}
                disabled={isExporting}
              >
                <div className="format-chip-header">
                  <span className="format-chip-ext">.json</span>
                  <span className="format-chip-name">Structured Interchange</span>
                </div>
                <span className="format-chip-desc">
                  Exports notes with hierarchy, tags, metadata, and portable links.
                </span>
              </button>

              <button
                type="button"
                className={`export-format-chip ${selectedFormat === "md" ? "is-selected" : ""}`}
                onClick={() => setSelectedFormat("md")}
                disabled={isExporting}
              >
                <div className="format-chip-header">
                  <span className="format-chip-ext">.md</span>
                  <span className="format-chip-name">Markdown Files</span>
                </div>
                <span className="format-chip-desc">
                  Exports separate Markdown documents into a folder.
                </span>
              </button>

              <button
                type="button"
                className={`export-format-chip ${selectedFormat === "txt" ? "is-selected" : ""}`}
                onClick={() => setSelectedFormat("txt")}
                disabled={isExporting}
              >
                <div className="format-chip-header">
                  <span className="format-chip-ext">.txt</span>
                  <span className="format-chip-name">Plain Text Files</span>
                </div>
                <span className="format-chip-desc">
                  Exports separate raw plain text documents into a folder.
                </span>
              </button>
            </div>
          </div>

          {/* Structured JSON Interchange Note */}
          {selectedFormat === "json" && (
            <div className="export-batch-info-banner" role="note">
              <span className="info-icon">ℹ️</span>
              <span className="info-text">
                Structured Interchange (v1.0) bundles notes into a single portable JSON document with
                hierarchy paths, tags, timestamps, and portable internal link remapping. File
                attachments are omitted from JSON interchange and must be managed separately. This
                format is an interchange document, not a full database backup.
              </span>
            </div>
          )}

          {/* Markdown-as-TXT Warning */}
          {showMarkdownTxtWarning && (
            <div className="export-batch-warning-banner" role="alert">
              <span className="warning-icon">ℹ️</span>
              <span className="warning-text">
                Stored Markdown source will be exported as raw text. Markdown characters (like #,
                *, []) will be preserved as text characters; output is not rendered into plain
                prose.
              </span>
            </div>
          )}

          {/* Error Message */}
          {errorMessage && (
            <div className="export-batch-error-banner" role="alert">
              <span className="error-icon">⚠️</span>
              <span className="error-text">{errorMessage}</span>
            </div>
          )}
        </div>

        <div className="export-batch-modal-footer">
          <button
            type="button"
            className="export-batch-cancel-btn"
            onClick={onClose}
            disabled={isExporting}
          >
            Cancel
          </button>
          <button
            type="button"
            className="export-batch-submit-btn"
            onClick={handleExport}
            disabled={
              isExporting ||
              isLoadingPreview ||
              !preview ||
              preview.totalIncluded === 0
            }
          >
            {isExporting ? "Exporting..." : `Export ${preview?.totalIncluded || 0} Notes`}
          </button>
        </div>
      </div>

      <OperationSummaryModal
        isOpen={Boolean(isExporting || summary)}
        onClose={() => {
          const finalSummary = summary;
          setSummary(null);
          setIsExporting(false);
          onClose();
          if (finalSummary && onSuccess) {
            onSuccess(finalSummary);
          }
        }}
        type="export"
        summary={summary}
        progress={exportProgress}
        isProcessing={isExporting}
        onCancel={handleCancel}
        isCancelling={isCancelling}
      />
    </div>
  );
};

export default ExportBatchModal;
