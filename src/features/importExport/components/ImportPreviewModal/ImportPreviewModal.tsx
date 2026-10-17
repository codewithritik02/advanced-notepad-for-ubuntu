import React, { useState, useEffect, useMemo, useCallback } from "react";
import { Modal, Button } from "../../../../components/ui";
import {
  ImportCandidate,
  ImportPreview,
  ImportProgress,
  ImportRequest,
  ImportSummary,
  DuplicatePolicy,
  ImportCandidateOverride,
  Notebook,
} from "../../../../services/storage/types";
import { importExportStorage } from "../../../../services/storage/importExport";
import { cryptoStorage } from "../../../../services/storage/crypto";
import { OperationSummaryModal } from "../OperationSummaryModal/OperationSummaryModal";
import "./ImportPreviewModal.css";

export interface ImportPreviewModalProps {
  isOpen: boolean;
  onClose: () => void;
  preview: ImportPreview | null;
  isLoading?: boolean;
  onConfirmImport: (request: ImportRequest) => Promise<ImportSummary | void>;
  notebooks?: Notebook[];
  defaultNotebookId?: string | null;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export const ImportPreviewModal: React.FC<ImportPreviewModalProps> = ({
  isOpen,
  onClose,
  preview,
  isLoading = false,
  onConfirmImport,
  notebooks = [],
  defaultNotebookId = null,
}) => {
  const [candidates, setCandidates] = useState<ImportCandidate[]>([]);
  const [titleOverrides, setTitleOverrides] = useState<Record<string, string>>({});
  const [targetNotebookId, setTargetNotebookId] = useState<string>(defaultNotebookId || "");
  const [duplicatePolicy, setDuplicatePolicy] = useState<DuplicatePolicy>("import_as_new");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [activeOperationId, setActiveOperationId] = useState<string | null>(null);
  const [importProgress, setImportProgress] = useState<ImportProgress | null>(null);
  const [summary, setSummary] = useState<ImportSummary | null>(null);
  const [isCancelling, setIsCancelling] = useState(false);
  const [isEncryptionActive, setIsEncryptionActive] = useState(false);

  useEffect(() => {
    if (isOpen) {
      cryptoStorage
        .getEncryptionStatus()
        .then((st) => setIsEncryptionActive(st.enabled))
        .catch(() => setIsEncryptionActive(false));
    }
  }, [isOpen]);

  // Synchronize candidates when preview updates
  useEffect(() => {
    if (preview) {
      setCandidates(preview.candidates);
      const initialTitles: Record<string, string> = {};
      for (const c of preview.candidates) {
        initialTitles[c.id] = c.proposedTitle;
      }
      setTitleOverrides(initialTitles);
      setTargetNotebookId(defaultNotebookId || "");
      setDuplicatePolicy("import_as_new");
      setIsSubmitting(false);
    } else {
      setCandidates([]);
      setTitleOverrides({});
      setIsSubmitting(false);
    }
  }, [preview, defaultNotebookId]);

  const handleToggleSelect = useCallback((id: string) => {
    setCandidates((prev) =>
      prev.map((c) => (c.id === id ? { ...c, selected: !c.selected } : c))
    );
  }, []);

  const handleTitleChange = useCallback((id: string, value: string) => {
    setTitleOverrides((prev) => ({ ...prev, [id]: value }));
  }, []);

  const handleSelectAll = useCallback(() => {
    setCandidates((prev) =>
      prev.map((c) => ({
        ...c,
        selected: c.validationStatus !== "invalid",
      }))
    );
  }, []);

  const handleDeselectAll = useCallback(() => {
    setCandidates((prev) => prev.map((c) => ({ ...c, selected: false })));
  }, []);

  const selectedCandidates = useMemo(
    () => candidates.filter((c) => c.selected && c.validationStatus !== "invalid"),
    [candidates]
  );

  const canConfirm = selectedCandidates.length > 0 && !isSubmitting && !isLoading;

  const handleCancel = useCallback(async () => {
    if (!activeOperationId || isCancelling) return;
    setIsCancelling(true);
    try {
      await importExportStorage.cancelOperation(activeOperationId);
    } catch (err) {
      console.error("Failed to cancel import operation:", err);
    }
  }, [activeOperationId, isCancelling]);

  const handleConfirm = useCallback(async () => {
    if (!canConfirm) return;

    const opId = `import-${Date.now()}-${Math.random().toString(36).substring(2, 7)}`;
    setActiveOperationId(opId);
    setImportProgress(null);
    setSummary(null);
    setIsCancelling(false);
    setIsSubmitting(true);

    const unlisten = await importExportStorage.onImportProgress((p) => {
      if (!p.operationId || p.operationId === opId) {
        setImportProgress(p);
      }
    });

    const overrides: ImportCandidateOverride[] = candidates.map((c) => ({
      id: c.id,
      filePath: c.filePath,
      selected: c.selected && c.validationStatus !== "invalid",
      customTitle: titleOverrides[c.id]?.trim() || c.proposedTitle,
    }));

    const request: ImportRequest = {
      operationId: opId,
      candidates: overrides,
      options: {
        targetNotebookId: targetNotebookId ? targetNotebookId : null,
        duplicatePolicy,
      },
    };

    try {
      const res = await onConfirmImport(request);
      if (res) {
        setSummary(res);
      }
    } finally {
      setIsSubmitting(false);
      unlisten();
    }
  }, [canConfirm, candidates, titleOverrides, targetNotebookId, duplicatePolicy, onConfirmImport]);

  if (!isOpen) return null;

  // While importing, and once the result is shown, only the operation summary dialog is visible.
  // Keeping the preview dialog open underneath stacked two modals on top of each other.
  const isPreviewVisible = !isSubmitting && !summary;

  return (
    <>
    <Modal
      isOpen={isPreviewVisible}
      onClose={isSubmitting ? () => {} : onClose}
      title="Import Notes Preview"
      maxWidth="680px"
      footer={
        <div className="import-modal-footer">
          <div className="import-footer-left">
            <Button
              variant="secondary"
              size="sm"
              onClick={handleSelectAll}
              disabled={isSubmitting || isLoading}
            >
              Select All
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={handleDeselectAll}
              disabled={isSubmitting || isLoading}
            >
              Deselect All
            </Button>
          </div>
          <div className="import-footer-right">
            <Button
              variant="secondary"
              onClick={onClose}
              disabled={isSubmitting}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              onClick={handleConfirm}
              disabled={!canConfirm}
            >
              {isSubmitting
                ? "Importing..."
                : `Import ${selectedCandidates.length} ${
                    selectedCandidates.length === 1 ? "Note" : "Notes"
                  }`}
            </Button>
          </div>
        </div>
      }
    >
      <div className="import-preview-modal-body">
        {isLoading ? (
          <div className="import-progress-container">
            <div className="import-spinner" />
            <div className="import-progress-text">Scanning and validating selected files...</div>
          </div>
        ) : (
          <>
            {/* Options Bar */}
            <div className="import-preview-options-bar">
              <div className="import-option-group">
                <label htmlFor="import-target-notebook" className="import-option-label">
                  Notebook:
                </label>
                <select
                  id="import-target-notebook"
                  className="import-select-input"
                  value={targetNotebookId}
                  onChange={(e) => setTargetNotebookId(e.target.value)}
                  disabled={isSubmitting}
                >
                  <option value="">Unfiled (No notebook)</option>
                  {notebooks.map((nb) => (
                    <option key={nb.id} value={nb.id}>
                      {nb.name}
                    </option>
                  ))}
                </select>
              </div>

              <div className="import-option-group">
                <label htmlFor="import-duplicate-policy" className="import-option-label">
                  Duplicates:
                </label>
                <select
                  id="import-duplicate-policy"
                  className="import-select-input"
                  value={duplicatePolicy}
                  onChange={(e) => setDuplicatePolicy(e.target.value as DuplicatePolicy)}
                  disabled={isSubmitting}
                >
                  <option value="import_as_new">Import as new notes</option>
                  <option value="skip_exact">Skip exact duplicates</option>
                  <option value="ask">Ask before conflict</option>
                </select>
              </div>
            </div>

            {isEncryptionActive && (
              <div className="import-encrypted-storage-notice" role="status">
                <span className="import-encrypted-icon">🔒</span>
                <span>Protected Storage: Imported notes will be encrypted with your active encryption key before persisting to disk.</span>
              </div>
            )}

            {/* Metrics Header */}
            {preview && (
              <div className="import-preview-summary-header">
                <div>
                  {preview.totalFiles} file{preview.totalFiles === 1 ? "" : "s"} found (
                  {formatBytes(preview.totalSizeBytes)})
                </div>
                <div className="import-preview-batch-stats">
                  <span className="import-stat-badge valid">
                    {preview.validFiles} Valid
                  </span>
                  {preview.duplicateFiles > 0 && (
                    <span className="import-stat-badge warning">
                      {preview.duplicateFiles} Duplicate{preview.duplicateFiles === 1 ? "" : "s"}
                    </span>
                  )}
                  {preview.invalidFiles > 0 && (
                    <span className="import-stat-badge invalid">
                      {preview.invalidFiles} Invalid
                    </span>
                  )}
                </div>
              </div>
            )}

            {/* Candidate List */}
            <div className="import-candidates-list" role="list" aria-label="Import candidate files">
              {candidates.length === 0 ? (
                <div className="import-progress-container">
                  <div className="import-progress-text">No candidate files found to import.</div>
                </div>
              ) : (
                candidates.map((c) => {
                  const isInvalid = c.validationStatus === "invalid";
                  const currentTitle = titleOverrides[c.id] ?? c.proposedTitle;

                  return (
                    <div
                      key={c.id}
                      className={`import-candidate-card ${
                        isInvalid ? "invalid" : c.isDuplicate ? "warning" : ""
                      }`}
                      role="listitem"
                    >
                      <div className="import-candidate-main-row">
                        <input
                          type="checkbox"
                          className="import-candidate-checkbox"
                          checked={c.selected && !isInvalid}
                          disabled={isInvalid || isSubmitting}
                          onChange={() => handleToggleSelect(c.id)}
                          aria-label={`Select ${c.fileName} for import`}
                        />

                        <input
                          type="text"
                          className="import-candidate-title-input"
                          value={currentTitle}
                          disabled={isInvalid || isSubmitting}
                          onChange={(e) => handleTitleChange(c.id, e.target.value)}
                          placeholder="Note title"
                          title="Click to edit proposed note title"
                        />

                        <div className="import-candidate-meta">
                          <span className={`import-format-tag ${c.detectedFormat}`}>
                            {c.detectedFormat}
                          </span>
                          <span className="import-size-tag">
                            {formatBytes(c.fileSize)}
                          </span>
                          <span className={`import-status-pill ${c.validationStatus}`}>
                            {c.validationStatus}
                          </span>
                        </div>
                      </div>

                      <div className="import-candidate-messages">
                        <div className="import-candidate-file-path" title={c.filePath}>
                          {c.fileName}
                        </div>

                        {c.errorMessage && (
                          <div className="import-candidate-error-text">
                            ⚠️ {c.errorMessage}
                          </div>
                        )}

                        {c.warnings.map((w, idx) => (
                          <div key={idx} className="import-candidate-warning-text">
                            ℹ️ {w}
                          </div>
                        ))}
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </>
        )}
      </div>
    </Modal>

    <OperationSummaryModal
      isOpen={Boolean(isSubmitting || summary)}
      onClose={() => {
        setSummary(null);
        setIsSubmitting(false);
        onClose();
      }}
      type="import"
      summary={summary}
      progress={importProgress}
      isProcessing={isSubmitting}
      onCancel={handleCancel}
      isCancelling={isCancelling}
    />
    </>
  );
};
