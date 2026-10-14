import React, { useEffect, useState, useCallback } from "react";
import { Modal, Button, Badge } from "../../../../components/ui";
import { BackupListItem, BackupValidationResult } from "../../../../services/storage/types";
import { backupStorage } from "../../../../services/storage/backup";
import {
  formatBackupBytes,
  pickBackupArchiveFile,
  openContainingFolder,
} from "../../services/backupDialog";
import "./BackupListModal.css";

export interface BackupListModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSelectForRestore?: (backup: BackupListItem) => void;
  onSelectForValidate?: (backup: BackupListItem, result?: BackupValidationResult) => void;
  onCreateNewBackup?: () => void;
}

export const BackupListModal: React.FC<BackupListModalProps> = ({
  isOpen,
  onClose,
  onSelectForRestore,
  onSelectForValidate,
  onCreateNewBackup,
}) => {
  const [backups, setBackups] = useState<BackupListItem[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [selectedBackup, setSelectedBackup] = useState<BackupListItem | null>(null);
  const [deleteConfirmTarget, setDeleteConfirmTarget] = useState<BackupListItem | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [validationResults, setValidationResults] = useState<Record<string, BackupValidationResult>>({});
  const [isValidating, setIsValidating] = useState(false);
  const [feedback, setFeedback] = useState<{
    type: "success" | "error";
    text: string;
  } | null>(null);

  const fetchBackups = useCallback(async () => {
    setIsLoading(true);
    setFeedback(null);
    try {
      const items = await backupStorage.listBackups();
      setBackups(items);
      setIsLoading(false);

      // Keep selection or default to newest
      if (items.length > 0) {
        setSelectedBackup((prev) => {
          if (!prev) return items[0];
          const match = items.find((i) => i.filePath === prev.filePath);
          return match || items[0];
        });
      } else {
        setSelectedBackup(null);
      }
    } catch (err: unknown) {
      setIsLoading(false);
      const msg = err instanceof Error ? err.message : String(err);
      setFeedback({ type: "error", text: `Failed to load backups: ${msg}` });
    }
  }, []);

  useEffect(() => {
    if (isOpen) {
      fetchBackups();
      setDeleteConfirmTarget(null);
    }
  }, [isOpen, fetchBackups]);

  if (!isOpen) return null;

  const handleValidateBackup = async (backup: BackupListItem) => {
    setIsValidating(true);
    setFeedback(null);
    try {
      const res = await backupStorage.validateBackup(backup.filePath);
      setValidationResults((prev) => ({ ...prev, [backup.filePath]: res }));

      // Update item in list
      setBackups((prev) =>
        prev.map((item) =>
          item.filePath === backup.filePath
            ? { ...item, isVerified: res.isValid, isValid: res.isValid }
            : item
        )
      );

      setSelectedBackup((prev) =>
        prev && prev.filePath === backup.filePath
          ? { ...prev, isVerified: res.isValid, isValid: res.isValid }
          : prev
      );

      if (res.isValid) {
        setFeedback({
          type: "success",
          text:
            res.compatibility === "requiresMigration"
              ? "Backup validated: valid older schema (requires migration to v4 on restore)."
              : "Backup validated: verified consistent and fully compatible with current application.",
        });
      } else {
        const errorIssue = res.issues.find((i) => i.severity === "error");
        setFeedback({
          type: "error",
          text: `Validation failed: ${errorIssue ? errorIssue.message : "Invalid backup archive"}`,
        });
      }

      onSelectForValidate?.(backup, res);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setFeedback({ type: "error", text: `Validation failed: ${msg}` });
    } finally {
      setIsValidating(false);
    }
  };

  const handlePickExternal = async () => {
    const res = await pickBackupArchiveFile();
    if (!res.cancelled && res.path) {
      const externalPath = res.path;
      // Refresh list
      await fetchBackups();

      // Automatically validate the chosen external archive
      const fileName = externalPath.split(/[/\\]/).pop() || "external-backup.pnbackup";
      const externalItem: BackupListItem = {
        filePath: externalPath,
        fileName,
        fileSize: 0,
        createdAt: new Date().toISOString(),
        backupType: "manual",
        isVerified: false,
        isValid: true,
      };

      setSelectedBackup(externalItem);
      handleValidateBackup(externalItem);
    }
  };

  const handleDeleteClick = (backup: BackupListItem) => {
    setDeleteConfirmTarget(backup);
  };

  const handleConfirmDelete = async () => {
    if (!deleteConfirmTarget) return;
    setIsDeleting(true);
    try {
      await backupStorage.deleteManagedBackup(deleteConfirmTarget.filePath);
      setIsDeleting(false);
      setFeedback({
        type: "success",
        text: `Removed backup '${deleteConfirmTarget.fileName}' successfully.`,
      });
      setDeleteConfirmTarget(null);
      await fetchBackups();
    } catch (err: unknown) {
      setIsDeleting(false);
      const msg = err instanceof Error ? err.message : String(err);
      setFeedback({
        type: "error",
        text: `Failed to delete backup: ${msg}`,
      });
    }
  };

  const handleCancelDelete = () => {
    setDeleteConfirmTarget(null);
  };

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

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Available Backups"
      maxWidth="760px"
      footer={
        <div style={{ display: "flex", justifyContent: "space-between", width: "100%", alignItems: "center" }}>
          <div>
            {onCreateNewBackup && (
              <Button
                variant="secondary"
                size="sm"
                onClick={() => {
                  onClose();
                  onCreateNewBackup();
                }}
              >
                + Create New Backup...
              </Button>
            )}
          </div>
          <Button variant="secondary" size="sm" onClick={onClose}>
            Close
          </Button>
        </div>
      }
    >
      <div className="backup-list-modal-container">
        {/* Toolbar */}
        <div className="backup-list-toolbar">
          <div className="backup-list-summary-stats">
            <span>
              {backups.length} {backups.length === 1 ? "backup archive" : "backup archives"} found
            </span>
          </div>

          <div className="backup-list-toolbar-actions">
            <Button
              variant="secondary"
              size="sm"
              onClick={handlePickExternal}
              title="Locate an external .pnbackup file from disk"
            >
              Browse Archive...
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={fetchBackups}
              disabled={isLoading}
              title="Refresh list of available backup archives"
            >
              {isLoading ? "Refreshing..." : "🔄 Refresh"}
            </Button>
          </div>
        </div>

        {/* Feedback Message */}
        {feedback && (
          <div
            className={`backup-feedback-banner ${
              feedback.type === "success" ? "is-success" : "is-error"
            }`}
            role="status"
          >
            {feedback.text}
          </div>
        )}

        {/* Empty State */}
        {backups.length === 0 && !isLoading && (
          <div className="backup-empty-state-box">
            <span style={{ fontSize: "2rem" }}>📂</span>
            <p style={{ fontWeight: 600, margin: 0 }}>No backups available yet</p>
            <p style={{ fontSize: "0.82rem", margin: 0, maxWidth: "380px" }}>
              Manual and scheduled automatic backups will appear here. You can create a backup
              now to protect your notes, notebooks, and attachments.
            </p>
            {onCreateNewBackup && (
              <Button
                variant="primary"
                size="sm"
                onClick={() => {
                  onClose();
                  onCreateNewBackup();
                }}
                style={{ marginTop: 8 }}
              >
                Create Backup Now
              </Button>
            )}
          </div>
        )}

        {/* Split View: List Column + Detail Inspection Column */}
        {backups.length > 0 && (
          <div className="backup-list-split-view">
            {/* Items Column */}
            <div className="backup-items-column" role="list" aria-label="Available backups list">
              {backups.map((item) => {
                const isSelected = selectedBackup?.filePath === item.filePath;
                return (
                  <div
                    key={item.filePath}
                    className={`backup-item-card ${isSelected ? "is-selected" : ""} ${
                      !item.isValid ? "is-invalid" : ""
                    }`}
                    onClick={() => {
                      setSelectedBackup(item);
                      setDeleteConfirmTarget(null);
                    }}
                    role="listitem"
                    tabIndex={0}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        setSelectedBackup(item);
                        setDeleteConfirmTarget(null);
                      }
                    }}
                  >
                    <div className="backup-item-header">
                      <span className="backup-item-title" title={item.fileName}>
                        {item.fileName}
                      </span>
                      <div className="backup-item-badges">
                        <Badge
                          variant={
                            item.backupType === "automatic"
                              ? "accent"
                              : item.backupType === "checkpoint"
                              ? "default"
                              : "subtle"
                          }
                          size="sm"
                        >
                          {item.backupType === "automatic"
                            ? "Auto"
                            : item.backupType === "checkpoint"
                            ? "Checkpoint"
                            : "Manual"}
                        </Badge>
                        <Badge
                          variant={item.isValid ? "success" : "danger"}
                          size="sm"
                        >
                          {item.isValid ? "Valid" : "Invalid"}
                        </Badge>
                      </div>
                    </div>

                    <div className="backup-item-meta-row">
                      <span>{formatDateTime(item.createdAt)}</span>
                      <span>{formatBackupBytes(item.fileSize)}</span>
                    </div>

                    {item.noteCount !== undefined && item.noteCount !== null && (
                      <div className="backup-item-meta-row" style={{ fontSize: "0.74rem" }}>
                        <span>
                          {item.noteCount} {item.noteCount === 1 ? "note" : "notes"}
                          {item.attachmentCount ? ` • ${item.attachmentCount} attachments` : ""}
                        </span>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>

            {/* Selected Backup Detail Column */}
            {selectedBackup && (
              <div className="backup-detail-column">
                <h4 className="backup-detail-title">{selectedBackup.fileName}</h4>

                <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
                  <div className="backup-detail-path" title={selectedBackup.filePath} style={{ flex: 1, margin: 0 }}>
                    {selectedBackup.filePath}
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => openContainingFolder(selectedBackup.filePath)}
                    title="Open containing folder in file manager"
                    style={{ whiteSpace: "nowrap", flexShrink: 0 }}
                  >
                    📁 Folder
                  </Button>
                </div>

                {/* Validation Warnings / Error Banner */}
                {!selectedBackup.isValid && selectedBackup.issueSummary && (
                  <div className="backup-detail-warning-box">
                    <strong>Notice:</strong> {selectedBackup.issueSummary}
                  </div>
                )}

                {/* Metadata Grid */}
                <div className="backup-detail-grid">
                  <div className="backup-detail-stat">
                    <span className="backup-detail-stat-label">File Size</span>
                    <span className="backup-detail-stat-val">
                      {formatBackupBytes(selectedBackup.fileSize)}
                    </span>
                  </div>

                  <div className="backup-detail-stat">
                    <span className="backup-detail-stat-label">Backup Type</span>
                    <span className="backup-detail-stat-val" style={{ textTransform: "capitalize" }}>
                      {selectedBackup.backupType}
                    </span>
                  </div>

                  <div className="backup-detail-stat">
                    <span className="backup-detail-stat-label">Notes</span>
                    <span className="backup-detail-stat-val">
                      {selectedBackup.noteCount ?? "—"}
                    </span>
                  </div>

                  <div className="backup-detail-stat">
                    <span className="backup-detail-stat-label">Attachments</span>
                    <span className="backup-detail-stat-val">
                      {selectedBackup.attachmentCount ?? "—"}
                    </span>
                  </div>

                  <div className="backup-detail-stat">
                    <span className="backup-detail-stat-label">Schema Version</span>
                    <span className="backup-detail-stat-val">
                      {selectedBackup.schemaVersion ? `v${selectedBackup.schemaVersion}` : "—"}
                    </span>
                  </div>

                  <div className="backup-detail-stat">
                    <span className="backup-detail-stat-label">Format Version</span>
                    <span className="backup-detail-stat-val">
                      {selectedBackup.formatVersion ? `v${selectedBackup.formatVersion}` : "—"}
                    </span>
                  </div>
                </div>

                <div style={{ fontSize: "0.76rem", color: "var(--color-text-secondary)" }}>
                  Created: {formatDateTime(selectedBackup.createdAt)}
                </div>

                {/* Deep Validation Report (Phase 11, Task 14) */}
                {validationResults[selectedBackup.filePath] && (
                  <div
                    className={`backup-validation-report ${
                      validationResults[selectedBackup.filePath].isValid ? "is-valid" : "is-invalid"
                    }`}
                  >
                    <div className="backup-validation-header">
                      <span className="backup-validation-title">Validation Inspection</span>
                      <Badge
                        variant={
                          validationResults[selectedBackup.filePath].compatibility === "compatible"
                            ? "success"
                            : validationResults[selectedBackup.filePath].compatibility === "requiresMigration"
                            ? "accent"
                            : "danger"
                        }
                        size="sm"
                      >
                        {validationResults[selectedBackup.filePath].compatibility === "compatible"
                          ? "✓ Compatible"
                          : validationResults[selectedBackup.filePath].compatibility === "requiresMigration"
                          ? "⚠ Requires Migration"
                          : validationResults[selectedBackup.filePath].compatibility === "incompatibleVersion"
                          ? "✕ Incompatible Version"
                          : "✕ Corrupted"}
                      </Badge>
                    </div>

                    <div className="backup-validation-checks-grid">
                      <div className="backup-validation-check-item">
                        <span className="check-icon">✓</span>
                        <span>SQLite Integrity</span>
                      </div>
                      <div className="backup-validation-check-item">
                        <span className="check-icon">✓</span>
                        <span>Foreign Keys</span>
                      </div>
                      <div className="backup-validation-check-item">
                        <span className="check-icon">✓</span>
                        <span>Tables & Columns</span>
                      </div>
                      <div className="backup-validation-check-item">
                        <span className="check-icon">✓</span>
                        <span>Attachment Inventory</span>
                      </div>
                    </div>

                    {validationResults[selectedBackup.filePath].issues.length > 0 && (
                      <div className="backup-validation-issues-list">
                        {validationResults[selectedBackup.filePath].issues.map((iss, idx) => (
                          <div
                            key={idx}
                            className={`backup-validation-issue-row ${
                              iss.severity === "error" ? "is-error" : "is-warning"
                            }`}
                          >
                            <span className="issue-badge">
                              {iss.severity === "error" ? "ERROR" : "WARN"}
                            </span>
                            <span className="issue-msg">{iss.message}</span>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {/* Delete Confirmation Box */}
                {deleteConfirmTarget?.filePath === selectedBackup.filePath ? (
                  <div className="backup-delete-confirm-box">
                    <p className="backup-delete-confirm-text">
                      Permanently delete this backup archive from disk? This cannot be undone.
                    </p>
                    <div className="backup-delete-confirm-actions">
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={handleCancelDelete}
                        disabled={isDeleting}
                      >
                        Cancel
                      </Button>
                      <Button
                        variant="danger"
                        size="sm"
                        onClick={handleConfirmDelete}
                        disabled={isDeleting}
                      >
                        {isDeleting ? "Deleting..." : "Permanently Delete"}
                      </Button>
                    </div>
                  </div>
                ) : (
                  /* Action Buttons */
                  <div className="backup-detail-actions">
                    <div style={{ display: "flex", gap: "8px" }}>
                      <Button
                        variant="secondary"
                        size="sm"
                        style={{ flex: 1 }}
                        onClick={() => handleValidateBackup(selectedBackup)}
                        disabled={isValidating}
                      >
                        {isValidating ? "Validating..." : "🔍 Validate"}
                      </Button>
                      {onSelectForRestore && (
                        <Button
                          variant="primary"
                          size="sm"
                          style={{ flex: 1 }}
                          onClick={() => onSelectForRestore(selectedBackup)}
                          disabled={!selectedBackup.isValid}
                        >
                          🔄 Restore...
                        </Button>
                      )}
                    </div>

                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleDeleteClick(selectedBackup)}
                      style={{ color: "var(--color-danger, #ef4444)" }}
                    >
                      🗑️ Delete Archive...
                    </Button>
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
};

export default BackupListModal;
