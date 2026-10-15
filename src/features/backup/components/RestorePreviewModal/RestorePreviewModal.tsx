import React, { useEffect, useState } from "react";
import { Modal, Button, Badge } from "../../../../components/ui";
import {
  BackupProgressEvent,
  BackupProgressStage,
  RestorePreviewResult,
} from "../../../../services/storage/types";
import { backupStorage } from "../../../../services/storage/backup";
import {
  formatBackupBytes,
  getBackupStageDescription,
} from "../../services/backupDialog";
import "./RestorePreviewModal.css";

export interface RestorePreviewModalProps {
  isOpen: boolean;
  backupFilePath: string | null;
  onClose: () => void;
  onConfirmRestore: (preview: RestorePreviewResult) => void;
  isRestoring?: boolean;
  restoreProgress?: BackupProgressEvent | null;
  onCancelRestore?: () => void;
  isCancelling?: boolean;
}

function formatStageBadge(stage: BackupProgressStage): string {
  switch (stage) {
    case "starting":
      return "Starting";
    case "validatingBackup":
      return "Validating";
    case "preparingStagingData":
    case "extractingStaging":
      return "Staging";
    case "checkingDatabaseIntegrity":
    case "validatingStagedData":
      return "Database";
    case "checkingAttachments":
      return "Attachments";
    case "creatingCheckpoint":
      return "Checkpoint";
    case "preparingActivation":
      return "Preparing Activation";
    case "activatingRestoredData":
      return "Activating";
    case "verifyingRestoredState":
    case "rebuildingSearchIndex":
      return "Verifying";
    case "completed":
      return "Completed";
    case "cancelled":
      return "Cancelled";
    case "failedWithRecoveryAvailable":
      return "Failed (Recoverable)";
    case "failedAndRecoveryRequired":
      return "Failed (Recovery Required)";
    case "failed":
      return "Failed";
    default:
      return "Restoring";
  }
}

export const RestorePreviewModal: React.FC<RestorePreviewModalProps> = ({
  isOpen,
  backupFilePath,
  onClose,
  onConfirmRestore,
  isRestoring = false,
  restoreProgress = null,
  onCancelRestore,
  isCancelling = false,
}) => {
  const [preview, setPreview] = useState<RestorePreviewResult | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isConfirmed, setIsConfirmed] = useState(false);

  useEffect(() => {
    if (isOpen && backupFilePath) {
      setIsLoading(true);
      setError(null);
      setIsConfirmed(false);

      backupStorage
        .previewRestore(backupFilePath)
        .then((res) => {
          setPreview(res);
          setIsLoading(false);
        })
        .catch((err: unknown) => {
          const msg = err instanceof Error ? err.message : String(err);
          setError(msg);
          setIsLoading(false);
        });
    } else {
      setPreview(null);
      setError(null);
      setIsConfirmed(false);
    }
  }, [isOpen, backupFilePath]);

  if (!isOpen) return null;

  const formatDateTime = (isoString: string) => {
    try {
      const date = new Date(isoString);
      return date.toLocaleString(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      });
    } catch {
      return isoString;
    }
  };

  const handleProceed = () => {
    if (!preview || !preview.canRestore || !isConfirmed || isRestoring) return;
    onConfirmRestore(preview);
  };

  const isCancellable = restoreProgress?.cancellable !== false;

  return (
    <Modal
      isOpen={isOpen}
      onClose={isRestoring ? () => {} : onClose}
      title="Restore Preview & Confirmation"
      maxWidth="620px"
      footer={
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 10, width: "100%" }}>
          {isRestoring ? (
            <>
              {onCancelRestore && isCancellable ? (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={onCancelRestore}
                  disabled={isCancelling}
                >
                  {isCancelling ? "Cancelling..." : "Cancel Restore"}
                </Button>
              ) : (
                <Button
                  variant="secondary"
                  size="sm"
                  disabled
                  title="Casual cancellation is locked out during active database replacement to prevent corruption"
                >
                  Cancellation Locked
                </Button>
              )}
              <Button
                variant="danger"
                size="sm"
                disabled
              >
                Restoring...
              </Button>
            </>
          ) : (
            <>
              <Button
                variant="secondary"
                size="sm"
                onClick={onClose}
              >
                Cancel
              </Button>
              <Button
                variant="danger"
                size="sm"
                onClick={handleProceed}
                disabled={!preview?.canRestore || !isConfirmed || isLoading}
              >
                Restore From Backup
              </Button>
            </>
          )}
        </div>
      }
    >
      <div className="restore-preview-container">
        {isLoading && (
          <div className="restore-preview-loading">
            <div className="restore-preview-spinner" />
            <span>Revalidating backup archive and analyzing contents...</span>
          </div>
        )}

        {error && (
          <div className="restore-preview-danger-banner">
            <h5 className="restore-preview-danger-title">Unable to Preview Restore</h5>
            <p className="restore-preview-danger-desc">{error}</p>
          </div>
        )}

        {preview && !isLoading && (
          <>
            {/* Header info */}
            <div className="restore-preview-header">
              <span className="restore-preview-filename">{preview.fileName}</span>
              <div className="restore-preview-meta-row">
                <span>Created: {formatDateTime(preview.createdAt)}</span>
                <span>•</span>
                <span>Size: {formatBackupBytes(preview.fileSize)}</span>
                <span>•</span>
                {preview.isEncrypted ? (
                  <Badge variant="accent" size="sm" title="This backup contains end-to-end encrypted data">
                    🔒 Encrypted
                  </Badge>
                ) : (
                  <Badge variant="default" size="sm" title="This backup is unencrypted">
                    Unencrypted
                  </Badge>
                )}
                <span>•</span>
                <Badge
                  variant={
                    preview.compatibility === "compatible"
                      ? "success"
                      : preview.compatibility === "requiresMigration"
                      ? "accent"
                      : "danger"
                  }
                  size="sm"
                >
                  {preview.compatibility === "compatible"
                    ? "✓ Compatible"
                    : preview.compatibility === "requiresMigration"
                    ? "⚠ Requires Migration"
                    : preview.compatibility === "incompatibleVersion"
                    ? "✕ Incompatible Version"
                    : "✕ Corrupted"}
                </Badge>
              </div>
            </div>

            {/* Content summary grid */}
            <div className="restore-preview-grid">
              <div className="restore-preview-stat-card">
                <span className="restore-preview-stat-label">Encryption</span>
                <span className="restore-preview-stat-value">
                  {preview.isEncrypted ? "🔒 Encrypted" : "Disabled"}
                </span>
              </div>
              <div className="restore-preview-stat-card">
                <span className="restore-preview-stat-label">Active Notes</span>
                <span className="restore-preview-stat-value">{preview.noteCount}</span>
              </div>

              <div className="restore-preview-stat-card">
                <span className="restore-preview-stat-label">Notebooks</span>
                <span className="restore-preview-stat-value">{preview.notebookCount}</span>
              </div>

              <div className="restore-preview-stat-card">
                <span className="restore-preview-stat-label">Tags</span>
                <span className="restore-preview-stat-value">{preview.tagCount}</span>
              </div>

              <div className="restore-preview-stat-card">
                <span className="restore-preview-stat-label">Trash Notes</span>
                <span className="restore-preview-stat-value">{preview.trashCount}</span>
              </div>

              <div className="restore-preview-stat-card">
                <span className="restore-preview-stat-label">Attachments</span>
                <span className="restore-preview-stat-value">
                  {preview.attachmentCount} ({formatBackupBytes(preview.attachmentTotalBytes)})
                </span>
              </div>

              <div className="restore-preview-stat-card">
                <span className="restore-preview-stat-label">Schema</span>
                <span className="restore-preview-stat-value">
                  v{preview.schemaVersion}
                  {preview.compatibility === "requiresMigration" ? " → v4" : ""}
                </span>
              </div>
            </div>

            {/* Warnings list */}
            {preview.warnings.length > 0 && (
              <div className="restore-preview-warnings-box">
                <div className="restore-preview-warnings-title">
                  <span>⚠</span>
                  <span>Inspection Notices ({preview.warnings.length})</span>
                </div>
                {preview.warnings.map((w, idx) => (
                  <p key={idx} className="restore-preview-warning-item">
                    • {w}
                  </p>
                ))}
              </div>
            )}

            {isRestoring && (
              <div className="restore-progress-container">
                <div className="restore-progress-header">
                  <div className="restore-progress-badge-group">
                    <span className="restore-progress-title">Restoration in Progress</span>
                    <Badge
                      variant={
                        restoreProgress?.stage === "completed"
                          ? "success"
                          : restoreProgress?.stage === "cancelled"
                          ? "default"
                          : restoreProgress?.stage === "failed" ||
                            restoreProgress?.stage === "failedAndRecoveryRequired" ||
                            restoreProgress?.stage === "failedWithRecoveryAvailable"
                          ? "danger"
                          : "accent"
                      }
                      size="sm"
                    >
                      {formatStageBadge(restoreProgress?.stage ?? "starting")}
                    </Badge>
                  </div>
                  <span className="restore-progress-status-text">
                    {restoreProgress?.message || getBackupStageDescription(restoreProgress?.stage ?? "starting")}
                  </span>
                </div>

                {/* Progress bar */}
                <div className="restore-progress-bar-track">
                  <div
                    className="restore-progress-bar-fill"
                    style={{
                      width: `${restoreProgress?.percentage ?? 50}%`,
                    }}
                  />
                </div>
                {restoreProgress?.percentage !== undefined && restoreProgress?.percentage !== null && (
                  <span className="restore-progress-percentage-label">
                    {Math.round(restoreProgress.percentage)}%
                  </span>
                )}

                {/* Cancellation Status & Actions */}
                <div className="restore-cancellation-section">
                  {isCancellable ? (
                    <div className="restore-cancellable-banner">
                      <div className="restore-cancellable-info">
                        <span className="restore-cancellable-indicator">● Safe Cancellation Window Active</span>
                        <p className="restore-cancellable-desc">
                          Current working data is untouched. You can cancel safely without modifying any live data.
                        </p>
                      </div>
                      {onCancelRestore && (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={onCancelRestore}
                          disabled={isCancelling}
                          className="restore-cancel-action-button"
                        >
                          {isCancelling ? "Cancelling..." : "Cancel Restore"}
                        </Button>
                      )}
                    </div>
                  ) : (
                    <div className="restore-lockout-banner">
                      <div className="restore-lockout-title">
                        <span>🔒</span>
                        <span>Activation Lockout Active</span>
                      </div>
                      <p className="restore-lockout-desc">
                        Active files are currently being swapped or validated. Casual cancellation is locked out to prevent database corruption. If a failure occurs, automatic fail-safe rollback will restore your previous state.
                      </p>
                    </div>
                  )}
                </div>
              </div>
            )}

            {!isRestoring && (
              <>
                {/* Danger alert notice */}
                <div className="restore-preview-danger-banner">
                  <h5 className="restore-preview-danger-title">
                    <span>⚠</span>
                    <span>Restoration Consequence Notice</span>
                  </h5>
                  <p className="restore-preview-danger-desc">
                    Restoring this backup will replace your current notes, notebooks, tags, and
                    attachments with the archived state from {formatDateTime(preview.createdAt)}. Any
                    modifications made after this backup date will be overwritten.
                  </p>
                  <p className="restore-preview-danger-desc" style={{ fontSize: "0.78rem", opacity: 0.9 }}>
                    A pre-restore recovery checkpoint of your current active data will be created automatically
                    before any files are replaced.
                  </p>
                </div>

                {/* Encrypted backup notice */}
                {preview.isEncrypted && (
                  <div
                    className="restore-preview-encrypted-banner"
                    style={{
                      padding: "10px 14px",
                      background: "rgba(59, 130, 246, 0.12)",
                      border: "1px solid rgba(59, 130, 246, 0.25)",
                      borderRadius: "6px",
                      marginTop: "10px",
                      color: "var(--color-text-main, #e5e7eb)",
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "6px",
                        fontWeight: 600,
                        fontSize: "0.85rem",
                        marginBottom: "4px",
                      }}
                    >
                      <span>🔒</span>
                      <span>Encrypted Backup Restore Notice</span>
                    </div>
                    <p style={{ margin: 0, fontSize: "0.8rem", lineHeight: 1.4, opacity: 0.9 }}>
                      This backup is protected with end-to-end encryption. Once restored, the application
                      will immediately lock, and you will need the master password used when this backup
                      was created to unlock and view your notes.
                    </p>
                  </div>
                )}

                {/* Mandatory confirmation checkbox */}
                {preview.canRestore && (
                  <label className="restore-preview-confirm-group">
                    <input
                      type="checkbox"
                      className="restore-preview-checkbox"
                      checked={isConfirmed}
                      onChange={(e) => setIsConfirmed(e.target.checked)}
                      disabled={isRestoring}
                    />
                    <span className="restore-preview-confirm-label">
                      I understand that my current notes and application data will be replaced by this backup.
                    </span>
                  </label>
                )}
              </>
            )}

            {!preview.canRestore && (
              <div className="restore-preview-danger-banner" style={{ background: "transparent" }}>
                <strong>Restoration Blocked:</strong> This backup archive is invalid or incompatible
                with the current version of Personal Notepad.
              </div>
            )}
          </>
        )}
      </div>
    </Modal>
  );
};

export default RestorePreviewModal;
