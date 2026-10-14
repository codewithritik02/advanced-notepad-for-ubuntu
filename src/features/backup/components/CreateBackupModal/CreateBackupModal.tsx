import React, { useState } from "react";
import { Modal, Button, Badge } from "../../../../components/ui";
import {
  BackupCreateResult,
  BackupProgressEvent,
} from "../../../../services/storage/types";
import { backupStorage } from "../../../../services/storage/backup";
import {
  pickBackupDestinationFile,
  formatBackupBytes,
  getBackupStageDescription,
} from "../../services/backupDialog";
import "./CreateBackupModal.css";

export interface CreateBackupModalProps {
  isOpen: boolean;
  onClose: () => void;
  onFlushSave?: () => Promise<void>;
  onSuccess?: (result: BackupCreateResult) => void;
}

export const CreateBackupModal: React.FC<CreateBackupModalProps> = ({
  isOpen,
  onClose,
  onFlushSave,
  onSuccess,
}) => {
  const [status, setStatus] = useState<"idle" | "running" | "success" | "error">(
    "idle"
  );
  const [customDestination, setCustomDestination] = useState<string | null>(null);
  const [allowOverwrite, setAllowOverwrite] = useState(false);
  const [progress, setProgress] = useState<BackupProgressEvent | null>(null);
  const [result, setResult] = useState<BackupCreateResult | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  if (!isOpen) return null;

  const handlePickCustomDestination = async () => {
    const res = await pickBackupDestinationFile();
    if (!res.cancelled && res.path) {
      setCustomDestination(res.path);
    }
  };

  const handleClearCustomDestination = () => {
    setCustomDestination(null);
  };

  const handleClose = () => {
    if (status === "running") return;
    setStatus("idle");
    setProgress(null);
    setResult(null);
    setErrorMessage(null);
    onClose();
  };

  const handleStartBackup = async () => {
    setStatus("running");
    setErrorMessage(null);
    setProgress({
      stage: "starting",
      message: "Initializing backup snapshot...",
      percentage: 5,
    });

    try {
      // 1. Flush any active unsaved editor changes before creating snapshot
      if (onFlushSave) {
        await onFlushSave();
      }

      // 2. Trigger verified manual backup via Tauri IPC
      const res = await backupStorage.createBackup(
        {
          destinationPath: customDestination,
          backupType: "manual",
          allowOverwrite,
        },
        (p) => {
          setProgress(p);
        }
      );

      setResult(res);
      setStatus("success");
      if (onSuccess) {
        onSuccess(res);
      }
    } catch (err: unknown) {
      setStatus("error");
      const msg =
        err instanceof Error
          ? err.message
          : typeof err === "string"
          ? err
          : "Backup creation failed. Please check destination path and permissions.";
      setErrorMessage(msg);
    }
  };

  const renderFooter = () => {
    if (status === "running") {
      return (
        <Button variant="secondary" size="sm" disabled>
          Creating Backup...
        </Button>
      );
    }

    if (status === "success") {
      return (
        <Button variant="primary" size="sm" onClick={handleClose}>
          Done
        </Button>
      );
    }

    if (status === "error") {
      return (
        <div style={{ display: "flex", gap: "8px", width: "100%", justifyContent: "flex-end" }}>
          <Button variant="secondary" size="sm" onClick={handleClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" onClick={handleStartBackup}>
            Try Again
          </Button>
        </div>
      );
    }

    return (
      <div style={{ display: "flex", gap: "8px", width: "100%", justifyContent: "flex-end" }}>
        <Button variant="secondary" size="sm" onClick={handleClose}>
          Cancel
        </Button>
        <Button variant="primary" size="sm" onClick={handleStartBackup}>
          Start Backup
        </Button>
      </div>
    );
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={handleClose}
      title="Create Manual Backup"
      maxWidth="560px"
      footer={renderFooter()}
    >
      <div className="backup-modal-container">
        {status === "idle" && (
          <>
            {/* Information Banner */}
            <div className="backup-modal-banner">
              <span className="backup-modal-banner-icon" aria-hidden="true">
                🛡️
              </span>
              <div className="backup-modal-banner-content">
                <h4>Offline-First Application Snapshot</h4>
                <p>
                  Creates an authoritative, verified <code>.pnbackup</code> archive.
                  Unlike standard text export, this archive captures your complete
                  SQLite database, tags, trash, and attachments for zero-risk recovery.
                </p>
              </div>
            </div>

            {/* Inclusions Scope Checklist */}
            <div className="backup-scope-section">
              <div className="backup-section-header">
                <span className="backup-section-title">What's Included</span>
                <Badge variant="accent" size="sm">
                  Full Application Scope
                </Badge>
              </div>
              <div className="backup-scope-grid">
                <div className="backup-scope-item">
                  <span className="check-icon">✓</span>
                  <span>Active Notes & Content</span>
                </div>
                <div className="backup-scope-item">
                  <span className="check-icon">✓</span>
                  <span>Notebooks & Hierarchy</span>
                </div>
                <div className="backup-scope-item">
                  <span className="check-icon">✓</span>
                  <span>Tags & Tag Mappings</span>
                </div>
                <div className="backup-scope-item">
                  <span className="check-icon">✓</span>
                  <span>Trash & Deleted Notes</span>
                </div>
                <div className="backup-scope-item">
                  <span className="check-icon">✓</span>
                  <span>Managed File Attachments</span>
                </div>
                <div className="backup-scope-item">
                  <span className="check-icon">✓</span>
                  <span>Settings & Preferences</span>
                </div>
              </div>
            </div>

            {/* Destination Configuration */}
            <div className="backup-dest-section">
              <div className="backup-section-header">
                <span className="backup-section-title">Destination</span>
              </div>
              <div className="backup-dest-box">
                <div className="backup-dest-row">
                  <span
                    className="backup-dest-path-preview"
                    title={customDestination || "Managed Backups Folder"}
                  >
                    {customDestination || "Default App Backups Folder (Recommended)"}
                  </span>
                  <div className="backup-dest-actions">
                    {customDestination ? (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={handleClearCustomDestination}
                      >
                        Reset
                      </Button>
                    ) : (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={handlePickCustomDestination}
                      >
                        Choose File...
                      </Button>
                    )}
                  </div>
                </div>
                <p className="backup-dest-note">
                  {customDestination
                    ? "Backup will be written directly to the selected archive path."
                    : "Saved to Personal Notepad's local backups directory with a safe timestamped name."}
                </p>
                {customDestination && (
                  <label className="backup-overwrite-row">
                    <input
                      type="checkbox"
                      checked={allowOverwrite}
                      onChange={(e) => setAllowOverwrite(e.target.checked)}
                    />
                    <span>Allow overwrite if chosen file already exists</span>
                  </label>
                )}
              </div>
            </div>
          </>
        )}

        {status === "running" && (
          <div className="backup-progress-container">
            <div className="backup-progress-header">
              <div className="backup-progress-title">
                <div className="backup-spinner" />
                <span>
                  {progress?.stage
                    ? getBackupStageDescription(progress.stage)
                    : "Creating backup..."}
                </span>
              </div>
              {progress?.percentage !== undefined && progress.percentage !== null && (
                <Badge variant="accent" size="sm">
                  {Math.round(progress.percentage)}%
                </Badge>
              )}
            </div>

            <div className="backup-progress-track">
              <div
                className="backup-progress-bar"
                style={{
                  width: `${Math.min(
                    100,
                    Math.max(5, progress?.percentage ?? 10)
                  )}%`,
                }}
              />
            </div>

            <p className="backup-progress-status-msg">
              {progress?.message || "Please wait while the backup archive is constructed and verified..."}
            </p>
          </div>
        )}

        {status === "success" && result && (
          <div className="backup-success-container">
            <div className="backup-success-badge-wrap">
              <span className="success-icon" aria-hidden="true">
                ✓
              </span>
              <div>
                <h4>Backup Created & Verified</h4>
                <p>
                  Archive passed cryptographic SHA-256 integrity checks and SQLite database
                  validation.
                </p>
              </div>
            </div>

            <div className="backup-summary-grid">
              <div className="backup-summary-card">
                <span className="backup-summary-label">Notes Protected</span>
                <span className="backup-summary-val">{result.noteCount}</span>
              </div>
              <div className="backup-summary-card">
                <span className="backup-summary-label">Attachments</span>
                <span className="backup-summary-val">{result.attachmentCount}</span>
              </div>
              <div className="backup-summary-card">
                <span className="backup-summary-label">Archive Size</span>
                <span className="backup-summary-val">
                  {formatBackupBytes(result.fileSize)}
                </span>
              </div>
              <div className="backup-summary-card">
                <span className="backup-summary-label">Status</span>
                <span className="backup-summary-val" style={{ color: "#059669" }}>
                  100% Verified
                </span>
              </div>
              <div className="backup-summary-card backup-path-card">
                <span className="backup-summary-label">Archive File Location</span>
                <span className="backup-summary-val">{result.filePath}</span>
              </div>
            </div>

            {result.warningMessage && (
              <div className="backup-modal-banner" style={{ borderColor: "#fde68a", backgroundColor: "#fffbeb" }}>
                <span style={{ color: "#d97706" }}>⚠️</span>
                <p style={{ color: "#92400e", fontSize: "0.82rem", margin: 0 }}>
                  {result.warningMessage}
                </p>
              </div>
            )}
          </div>
        )}

        {status === "error" && (
          <div className="backup-error-banner">
            <span className="error-icon" aria-hidden="true">
              ⚠️
            </span>
            <div>
              <h4>Backup Could Not Be Completed</h4>
              <p>
                {errorMessage ||
                  "An error occurred while creating or validating the backup. Your active data remains untouched."}
              </p>
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
};

export default CreateBackupModal;
