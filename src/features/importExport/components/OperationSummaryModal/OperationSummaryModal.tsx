import React from "react";
import { Modal, Button } from "../../../../components/ui";
import {
  ExportProgress,
  ExportSummary,
  ImportProgress,
  ImportSummary,
  OperationOutcome,
} from "../../../../services/storage/types";
import "./OperationSummaryModal.css";

export interface OperationSummaryModalProps {
  isOpen: boolean;
  onClose: () => void;
  type: "import" | "export";
  summary: ImportSummary | ExportSummary | null;
  progress?: ImportProgress | ExportProgress | null;
  isProcessing?: boolean;
  onCancel?: () => void;
  isCancelling?: boolean;
}

export function getOutcomeLabel(outcome?: OperationOutcome): string {
  switch (outcome) {
    case "full_success":
      return "Full Success";
    case "success_with_warnings":
      return "Completed with Warnings";
    case "partial_success":
      return "Partial Success";
    case "cancelled":
      return "Operation Cancelled";
    case "total_failure":
      return "Total Failure";
    default:
      return "Finished";
  }
}

export function getOutcomeBadgeClass(outcome?: OperationOutcome): string {
  switch (outcome) {
    case "full_success":
      return "outcome-full-success";
    case "success_with_warnings":
      return "outcome-warnings";
    case "partial_success":
      return "outcome-partial";
    case "cancelled":
      return "outcome-cancelled";
    case "total_failure":
      return "outcome-failure";
    default:
      return "outcome-default";
  }
}

export const OperationSummaryModal: React.FC<OperationSummaryModalProps> = ({
  isOpen,
  onClose,
  type,
  summary,
  progress,
  isProcessing = false,
  onCancel,
  isCancelling = false,
}) => {
  if (!isOpen) return null;

  const isImport = type === "import";
  const modalTitle = isProcessing
    ? isImport
      ? "Importing Notes..."
      : "Exporting Notes..."
    : isImport
    ? "Import Summary"
    : "Export Summary";

  // Derive progress stats
  const totalItems = progress?.total || (isImport ? (summary as ImportSummary)?.totalProcessed : (summary as ExportSummary)?.totalNotes) || 0;
  const currentIndex = progress?.currentIndex || 0;
  const progressPercent = totalItems > 0 ? Math.min(100, Math.round((currentIndex / totalItems) * 100)) : 0;
  const currentItemLabel = isImport
    ? (progress as ImportProgress)?.currentFile || ""
    : (progress as ExportProgress)?.currentTitle || "";

  // Derive counts from summary or progress
  const succeededCount = isProcessing
    ? (isImport ? (progress as ImportProgress)?.importedCount : (progress as ExportProgress)?.exportedCount) || 0
    : isImport
    ? (summary as ImportSummary)?.importedCount || 0
    : (summary as ExportSummary)?.exportedCount || 0;

  const skippedCount = isProcessing
    ? progress?.skippedCount || 0
    : summary?.skippedCount || 0;

  const failedCount = isProcessing
    ? progress?.failedCount || 0
    : summary?.failedCount || 0;

  const cancelledCount = isProcessing
    ? progress?.cancelledCount || 0
    : summary?.cancelledCount || 0;

  const outcome = summary?.outcome;

  return (
    <Modal
      isOpen={isOpen}
      onClose={isProcessing ? () => {} : onClose}
      title={modalTitle}
      maxWidth="640px"
      footer={
        <div className="op-summary-modal-footer">
          {isProcessing ? (
            <div className="op-progress-footer-actions">
              <Button
                variant="danger"
                size="sm"
                onClick={onCancel}
                disabled={isCancelling}
              >
                {isCancelling ? "Cancelling..." : "Cancel Operation"}
              </Button>
            </div>
          ) : (
            <div className="op-summary-footer-actions">
              <Button variant="primary" onClick={onClose}>
                Done
              </Button>
            </div>
          )}
        </div>
      }
    >
      <div className="op-summary-container">
        {isProcessing ? (
          <div className="op-live-progress-section">
            <div className="op-progress-bar-wrap">
              <div
                className="op-progress-bar-fill"
                style={{ width: `${progressPercent}%` }}
              />
            </div>
            <div className="op-progress-meta-row">
              <span className="op-progress-percentage">{progressPercent}%</span>
              <span className="op-progress-counter">
                {currentIndex} / {totalItems} items
              </span>
            </div>
            {currentItemLabel && (
              <div className="op-progress-current-item" title={currentItemLabel}>
                Processing: <span className="op-item-name">{currentItemLabel}</span>
              </div>
            )}
            <div className="op-metrics-grid mini">
              <div className="op-metric-chip success">
                <span className="op-metric-num">{succeededCount}</span>
                <span className="op-metric-label">{isImport ? "Imported" : "Exported"}</span>
              </div>
              <div className="op-metric-chip warning">
                <span className="op-metric-num">{skippedCount}</span>
                <span className="op-metric-label">Skipped</span>
              </div>
              <div className="op-metric-chip danger">
                <span className="op-metric-num">{failedCount}</span>
                <span className="op-metric-label">Failed</span>
              </div>
              {cancelledCount > 0 && (
                <div className="op-metric-chip cancelled">
                  <span className="op-metric-num">{cancelledCount}</span>
                  <span className="op-metric-label">Cancelled</span>
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="op-finished-summary-section">
            <div className="op-outcome-banner">
              <span className={`op-outcome-badge ${getOutcomeBadgeClass(outcome)}`}>
                {getOutcomeLabel(outcome)}
              </span>
              {summary && summary.durationMs > 0 && (
                <span className="op-duration-pill">
                  {summary.durationMs < 1000
                    ? `${summary.durationMs}ms`
                    : `${(summary.durationMs / 1000).toFixed(2)}s`}
                </span>
              )}
            </div>

            <div className="op-metrics-grid">
              <div className="op-metric-chip">
                <span className="op-metric-num">{totalItems}</span>
                <span className="op-metric-label">Total</span>
              </div>
              <div className="op-metric-chip success">
                <span className="op-metric-num">{succeededCount}</span>
                <span className="op-metric-label">{isImport ? "Imported" : "Exported"}</span>
              </div>
              <div className="op-metric-chip warning">
                <span className="op-metric-num">{skippedCount}</span>
                <span className="op-metric-label">Skipped</span>
              </div>
              <div className="op-metric-chip danger">
                <span className="op-metric-num">{failedCount}</span>
                <span className="op-metric-label">Failed</span>
              </div>
              <div className="op-metric-chip cancelled">
                <span className="op-metric-num">{cancelledCount}</span>
                <span className="op-metric-label">Cancelled</span>
              </div>
            </div>

            {/* Itemized File Results List */}
            <div className="op-results-list-wrap">
              <h4 className="op-results-title">Itemized Results</h4>
              <div className="op-results-list">
                {summary &&
                  summary.results &&
                  summary.results.map((res: any, idx: number) => {
                    const itemName = isImport ? res.fileName : res.noteTitle;
                    const status = res.status;
                    return (
                      <div key={idx} className={`op-result-row status-${status}`}>
                        <div className="op-result-info">
                          <span className="op-result-name" title={itemName}>
                            {itemName}
                          </span>
                          {res.message && (
                            <span className="op-result-message" title={res.message}>
                              {res.message}
                            </span>
                          )}
                        </div>
                        <span className={`op-result-status-pill status-${status}`}>
                          {status}
                        </span>
                      </div>
                    );
                  })}
              </div>
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
};

export default OperationSummaryModal;
