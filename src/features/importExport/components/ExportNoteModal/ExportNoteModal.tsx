import React, { useState } from "react";
import { ExportFormat } from "../../../../services/storage/types";
import { importExportStorage } from "../../../../services/storage/importExport";
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

export function sanitizeExportFilename(title: string): string {
  const illegal = /[/\\:*?"<>|]/g;
  let cleaned = (title || "").replace(illegal, "").trim();
  // Strip leading dots to prevent creating hidden files
  cleaned = cleaned.replace(/^\.+/, "");
  // Strip trailing dots and spaces
  cleaned = cleaned.replace(/[. ]+$/, "");
  if (!cleaned) {
    return "Untitled Note";
  }
  const upper = cleaned.toUpperCase();
  const isReserved = [
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
  ].includes(upper);
  const safeName = isReserved ? `_${cleaned}` : cleaned;
  return safeName.length > 180 ? safeName.slice(0, 180) : safeName;
}

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
