import React, { useState, useEffect } from "react";
import { ExportFormat } from "../../../../services/storage/types";
import { importExportStorage } from "../../../../services/storage/importExport";
import { cryptoStorage } from "../../../../services/storage/crypto";
import { pickExportDestinationFile } from "../../services/fileDialog";
import "./ExportNoteModal.css";

export interface ExportNoteModalProps {
  isOpen: boolean;
  onClose: () => void;
  note: {
    id: string;
    title: string;
    content: string;
    format: string;
  };
  onFlushSave?: () => Promise<void>;
  onSuccess?: (filePath: string) => void;
}

import { sanitizeExportFilename } from "../../utils";
export { sanitizeExportFilename };

export const ExportNoteModal: React.FC<ExportNoteModalProps> = ({
  isOpen,
  onClose,
  note,
  onFlushSave,
  onSuccess,
}) => {
  const defaultFormat: ExportFormat = note.format === "md" ? "md" : "txt";
  const [selectedFormat, setSelectedFormat] = useState<ExportFormat>(defaultFormat);
  const [isExporting, setIsExporting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [isEncryptionActive, setIsEncryptionActive] = useState(false);

  useEffect(() => {
    if (isOpen) {
      cryptoStorage
        .getEncryptionStatus()
        .then((st) => setIsEncryptionActive(st.enabled))
        .catch(() => setIsEncryptionActive(false));
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const baseFileName = sanitizeExportFilename(note.title);
  const extension = selectedFormat === "md" ? ".md" : selectedFormat === "json" ? ".json" : ".txt";
  const defaultFileName = `${baseFileName}${extension}`;

  const handleExport = async () => {
    setIsExporting(true);
    setErrorMessage(null);

    try {
      // 1. Flush any pending unsaved editor changes before export
      if (onFlushSave) {
        await onFlushSave();
      }

      // 2. Open native save file dialog
      const pickerResult = await pickExportDestinationFile(defaultFileName, selectedFormat);
      if (pickerResult.cancelled || !pickerResult.path) {
        // User cancelled dialog cleanly without reporting error
        setIsExporting(false);
        return;
      }

      // 3. Execute backend export
      const result = await importExportStorage.exportSingleNote({
        noteId: note.id,
        format: selectedFormat,
        destinationPath: pickerResult.path,
        overwrite: true,
      });

      setIsExporting(false);
      onClose();
      if (onSuccess) {
        onSuccess(result.filePath);
      }
    } catch (err) {
      setIsExporting(false);
      setErrorMessage(
        err instanceof Error ? err.message : String(err) || "Failed to export note."
      );
    }
  };

  return (
    <div className="export-note-modal-overlay" role="dialog" aria-modal="true" aria-labelledby="export-modal-title">
      <div className="export-note-modal-dialog">
        <div className="export-note-modal-header">
          <h3 id="export-modal-title">Export Note</h3>
          <button
            type="button"
            className="export-note-modal-close-btn"
            aria-label="Close export dialog"
            onClick={onClose}
          >
            ✕
          </button>
        </div>

        <div className="export-note-modal-body">
          <div className="export-note-info-row">
            <span className="export-note-info-label">Source Note</span>
            <div className="export-note-title-display">{note.title?.trim() || "Untitled Note"}</div>
          </div>

          <div className="export-note-info-row">
            <span className="export-note-info-label">Choose Export Format</span>
            <div className="export-format-options-group" role="radiogroup" aria-label="Export format selection">
              <label className={`export-format-card ${selectedFormat === "md" ? "is-selected" : ""}`}>
                <input
                  type="radio"
                  name="exportFormat"
                  value="md"
                  checked={selectedFormat === "md"}
                  onChange={() => setSelectedFormat("md")}
                />
                <div className="export-format-details">
                  <span className="export-format-title">Markdown (.md)</span>
                  <span className="export-format-desc">
                    Exports note source as standard Markdown text, preserving headings, lists, tables, and links.
                  </span>
                </div>
              </label>

              <label className={`export-format-card ${selectedFormat === "txt" ? "is-selected" : ""}`}>
                <input
                  type="radio"
                  name="exportFormat"
                  value="txt"
                  checked={selectedFormat === "txt"}
                  onChange={() => setSelectedFormat("txt")}
                />
                <div className="export-format-details">
                  <span className="export-format-title">Plain Text (.txt)</span>
                  <span className="export-format-desc">
                    Universal UTF-8 text file compatible with any editor.
                  </span>
                  {note.format === "md" && (
                    <div className="export-format-warning">
                      ℹ️ Note: Stored Markdown source will be exported as raw text. Markdown characters (like #, *, []) will be preserved as text characters; output is not rendered into plain prose.
                    </div>
                  )}
                </div>
              </label>

              <label className={`export-format-card ${selectedFormat === "json" ? "is-selected" : ""}`}>
                <input
                  type="radio"
                  name="exportFormat"
                  value="json"
                  checked={selectedFormat === "json"}
                  onChange={() => setSelectedFormat("json")}
                />
                <div className="export-format-details">
                  <span className="export-format-title">Structured Interchange (.json)</span>
                  <span className="export-format-desc">
                    Portable JSON document preserving notebook hierarchy, tags, favorites, and links.
                  </span>
                </div>
              </label>
            </div>
          </div>

          <div className="export-destination-preview">
            <span className="export-note-info-label">Target Filename</span>
            <span className="export-filename-badge">{defaultFileName}</span>
          </div>

          <div className="export-security-notice" role="note">
            <span className="export-security-icon">ℹ️</span>
            <span>
              {isEncryptionActive
                ? "Security Notice: Database encryption is active. Exporting creates unencrypted files at the chosen destination. Ensure destination folders are properly secured."
                : "Security Notice: Exporting creates unencrypted files at the chosen destination."}
            </span>
          </div>

          {errorMessage && (
            <div className="export-note-modal-error" role="alert">
              {errorMessage}
            </div>
          )}
        </div>

        <div className="export-note-modal-footer">
          <button
            type="button"
            className="export-modal-cancel-btn"
            onClick={onClose}
            disabled={isExporting}
          >
            Cancel
          </button>
          <button
            type="button"
            className="export-modal-submit-btn"
            onClick={handleExport}
            disabled={isExporting}
          >
            {isExporting ? "Exporting..." : "Choose Destination & Export..."}
          </button>
        </div>
      </div>
    </div>
  );
};
