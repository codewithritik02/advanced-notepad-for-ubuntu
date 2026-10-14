import React, { useCallback, useEffect, useState } from "react";
import { Button } from "../../../../components/ui";
import { BackupHealthStatus, BackupSettings } from "../../../../services/storage/types";
import { backupStorage } from "../../../../services/storage/backup";
import { pickBackupDestinationFolder } from "../../services/backupDialog";
import "./BackupSettingsPanel.css";

export interface BackupSettingsPanelProps {
  onCreateManualBackup?: () => void;
  onViewBackups?: () => void;
}

export const BackupSettingsPanel: React.FC<BackupSettingsPanelProps> = ({
  onCreateManualBackup,
  onViewBackups,
}) => {
  const [settings, setSettings] = useState<BackupSettings | null>(null);
  const [health, setHealth] = useState<BackupHealthStatus | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isRefreshingHealth, setIsRefreshingHealth] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [feedback, setFeedback] = useState<{
    type: "saved" | "error";
    text: string;
  } | null>(null);

  const loadHealth = useCallback(async () => {
    try {
      setIsRefreshingHealth(true);
      const res = await backupStorage.getHealthStatus();
      setHealth(res);
    } catch (err) {
      console.warn("Failed to retrieve backup health status:", err);
    } finally {
      setIsRefreshingHealth(false);
    }
  }, []);

  useEffect(() => {
    let isMounted = true;
    Promise.all([backupStorage.getSettings(), backupStorage.getHealthStatus()])
      .then(([loadedSettings, loadedHealth]) => {
        if (isMounted) {
          setSettings(loadedSettings);
          setHealth(loadedHealth);
          setIsLoading(false);
        }
      })
      .catch((err) => {
        if (isMounted) {
          setIsLoading(false);
          setFeedback({
            type: "error",
            text: err instanceof Error ? err.message : String(err),
          });
        }
      });

    return () => {
      isMounted = false;
    };
  }, []);

  const saveSettings = async (nextSettings: BackupSettings) => {
    setSettings(nextSettings);
    setIsSaving(true);
    setFeedback(null);

    try {
      const persisted = await backupStorage.updateSettings(nextSettings);
      setSettings(persisted);
      setIsSaving(false);
      setFeedback({ type: "saved", text: "Settings saved successfully." });
      loadHealth();
      setTimeout(() => setFeedback(null), 3000);
    } catch (err: unknown) {
      setIsSaving(false);
      const msg =
        err instanceof Error
          ? err.message
          : typeof err === "string"
          ? err
          : "Failed to update backup settings.";
      setFeedback({ type: "error", text: msg });
    }
  };

  const handleToggleAutoBackup = (enabled: boolean) => {
    if (!settings) return;
    saveSettings({ ...settings, autoBackupEnabled: enabled });
  };

  const handleIntervalChange = (valStr: string) => {
    if (!settings) return;
    const minutes = parseInt(valStr, 10);
    if (!isNaN(minutes)) {
      saveSettings({ ...settings, intervalMinutes: minutes });
    }
  };

  const handleRetentionChange = (valStr: string) => {
    if (!settings) return;
    const count = parseInt(valStr, 10);
    if (!isNaN(count) && count >= 1 && count <= 50) {
      saveSettings({ ...settings, retentionCount: count });
    }
  };

  const handleToggleStartup = (enabled: boolean) => {
    if (!settings) return;
    saveSettings({ ...settings, backupOnStartup: enabled });
  };

  const handlePickFolder = async () => {
    if (!settings) return;
    const pickerRes = await pickBackupDestinationFolder();
    if (!pickerRes.cancelled && pickerRes.path) {
      saveSettings({ ...settings, backupDestinationDir: pickerRes.path });
    }
  };

  const handleResetFolder = () => {
    if (!settings) return;
    saveSettings({ ...settings, backupDestinationDir: null });
  };

  if (isLoading) {
    return <div className="backup-settings-loading">Loading backup configuration...</div>;
  }

  if (!settings) {
    return (
      <div className="backup-settings-panel">
        <p className="backup-config-desc">Failed to load backup settings.</p>
      </div>
    );
  }

  return (
    <div className="backup-settings-panel">
      {/* Backup Health & Status Indicator Card (Phase 11, Task 22) */}
      {health && (
        <div
          className={`backup-health-card backup-health-card--${health.healthLevel}`}
          data-testid="backup-health-card"
        >
          <div className="backup-health-header">
            <div className="backup-health-title-group">
              <span className="backup-health-title">Backup Health & Status</span>
              <span
                className={`backup-health-badge backup-health-badge--${health.healthLevel}`}
                data-testid="backup-health-badge"
              >
                {health.healthLevel === "healthy" && "✓ System Healthy"}
                {health.healthLevel === "warning" && "⚠ Attention Needed"}
                {health.healthLevel === "critical" && "✕ Critical Failure"}
                {health.healthLevel === "disabled" && "⏸ Backups Disabled"}
              </span>
            </div>
            <Button
              variant="secondary"
              size="sm"
              onClick={loadHealth}
              disabled={isRefreshingHealth}
              title="Refresh backup health status"
            >
              ↻ Refresh
            </Button>
          </div>

          <div className="backup-health-grid">
            <div className="backup-health-stat">
              <span className="backup-health-stat-label">Last Successful Backup</span>
              <span className="backup-health-stat-value" data-testid="health-last-successful">
                {health.lastSuccessfulAt
                  ? new Date(health.lastSuccessfulAt).toLocaleString()
                  : "Never"}
              </span>
            </div>
            <div className="backup-health-stat">
              <span className="backup-health-stat-label">Last Attempt</span>
              <span className="backup-health-stat-value" data-testid="health-last-attempt">
                {health.lastAttemptAt
                  ? new Date(health.lastAttemptAt).toLocaleString()
                  : "No recorded attempts"}
                {health.lastFailureCategory && (
                  <span className="backup-health-failure-tag"> ({health.lastFailureCategory})</span>
                )}
              </span>
            </div>
            <div className="backup-health-stat">
              <span className="backup-health-stat-label">Destination Status</span>
              <span
                className={`backup-health-stat-value ${
                  health.destinationAvailable ? "is-available" : "is-unavailable"
                }`}
                data-testid="health-destination-status"
              >
                {health.destinationAvailable ? "✓ Available & Writable" : "✕ Unavailable / Unwritable"}
              </span>
            </div>
            <div className="backup-health-stat">
              <span className="backup-health-stat-label">Verified Recoverability</span>
              <span
                className={`backup-health-stat-value ${
                  health.hasVerifiedBackup ? "is-verified" : "is-unverified"
                }`}
                data-testid="health-recoverability-status"
              >
                {health.managedBackupCount} archives (
                {health.hasVerifiedBackup ? "Verified Recoverable" : "Unverified / No verified backups"}
                )
              </span>
            </div>
            <div className="backup-health-stat backup-health-stat--wide">
              <span className="backup-health-stat-label">Drive Isolation</span>
              <span
                className="backup-health-stat-value"
                data-testid="health-drive-isolation"
              >
                {health.isSameDisk
                  ? "⚠️ Primary System Disk (External drive recommended for physical hardware failure protection)"
                  : "✓ Isolated to separate storage media"}
              </span>
            </div>
          </div>

          {/* Active Health Warnings List */}
          {health.warnings.length > 0 && (
            <div className="backup-health-warnings-list" data-testid="backup-health-warnings-list">
              {health.warnings.map((warn) => (
                <div
                  key={warn.code}
                  className={`backup-health-warning-item backup-health-warning-item--${warn.level}`}
                  data-testid={`backup-warning-${warn.code}`}
                >
                  <span className="backup-health-warning-icon" aria-hidden="true">
                    {warn.level === "critical" ? "✕" : warn.level === "warning" ? "⚠" : "ℹ"}
                  </span>
                  <div className="backup-health-warning-content">
                    <span className="backup-health-warning-msg">{warn.message}</span>
                    {warn.detail && (
                      <p className="backup-health-warning-detail">{warn.detail}</p>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Enable/Disable Toggle */}
      <div className="backup-settings-toggle-row">
        <div className="backup-settings-toggle-label">
          <span className="backup-settings-toggle-title">Automatic Backups</span>
          <p className="backup-settings-toggle-desc">
            Automatically create local snapshots on schedule.
          </p>
        </div>
        <label className="backup-settings-switch">
          <input
            type="checkbox"
            checked={settings.autoBackupEnabled}
            onChange={(e) => handleToggleAutoBackup(e.target.checked)}
          />
          <span className="backup-settings-slider" />
        </label>
      </div>

      {/* Interval, Retention, and Lifecycle configuration */}
      <div className="backup-config-grid">
        <div className="backup-config-row">
          <div className="backup-config-row-label">
            <span className="backup-config-title">Backup Frequency</span>
            <p className="backup-config-desc">
              How often an automatic backup should be taken while the app is active.
            </p>
          </div>
          <select
            className="backup-select"
            value={settings.intervalMinutes}
            disabled={!settings.autoBackupEnabled}
            onChange={(e) => handleIntervalChange(e.target.value)}
          >
            <option value="60">Every 1 hour</option>
            <option value="360">Every 6 hours</option>
            <option value="720">Every 12 hours</option>
            <option value="1440">Daily (24 hours) — Recommended</option>
            <option value="10080">Weekly (7 days)</option>
          </select>
        </div>

        <div className="backup-config-row">
          <div className="backup-config-row-label">
            <span className="backup-config-title">Retention Policy</span>
            <p className="backup-config-desc">
              Keep the last X automatic backups. Older automatic archives are pruned safely.
            </p>
          </div>
          <input
            type="number"
            min={1}
            max={50}
            className="backup-number-input"
            value={settings.retentionCount}
            disabled={!settings.autoBackupEnabled}
            onChange={(e) => handleRetentionChange(e.target.value)}
          />
        </div>

        <div className="backup-config-row">
          <div className="backup-config-row-label">
            <span className="backup-config-title">Backup on Startup</span>
            <p className="backup-config-desc">
              Perform an automatic backup when opening the app if due.
            </p>
          </div>
          <label className="backup-settings-switch">
            <input
              type="checkbox"
              checked={settings.backupOnStartup}
              disabled={!settings.autoBackupEnabled}
              onChange={(e) => handleToggleStartup(e.target.checked)}
            />
            <span className="backup-settings-slider" />
          </label>
        </div>
      </div>

      {/* Destination Folder Selector */}
      <div className="backup-folder-box">
        <div className="backup-folder-header">
          <div className="backup-config-row-label">
            <span className="backup-config-title">Automatic Backup Destination</span>
            <p className="backup-config-desc">
              Local directory where scheduled backups are archived.
            </p>
          </div>
          <div className="backup-folder-actions">
            {settings.backupDestinationDir ? (
              <Button variant="secondary" size="sm" onClick={handleResetFolder}>
                Default
              </Button>
            ) : (
              <Button
                variant="secondary"
                size="sm"
                onClick={handlePickFolder}
                disabled={!settings.autoBackupEnabled}
              >
                Browse...
              </Button>
            )}
          </div>
        </div>
        <div
          className="backup-folder-path"
          title={settings.backupDestinationDir || "Default Application Backups Directory"}
        >
          {settings.backupDestinationDir || "Default: ~/.local/share/personal-notepad/backups/"}
        </div>
      </div>

      {/* Backup Actions */}
      {(onCreateManualBackup || onViewBackups) && (
        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: "8px",
            marginTop: 4,
            flexWrap: "wrap",
          }}
        >
          {onViewBackups && (
            <Button variant="secondary" size="sm" onClick={onViewBackups}>
              📂 View Backups...
            </Button>
          )}
          {onCreateManualBackup && (
            <Button variant="secondary" size="sm" onClick={onCreateManualBackup}>
              🛡️ Create Manual Backup...
            </Button>
          )}
        </div>
      )}

      {/* Saving or Feedback Status */}
      {isSaving && (
        <div className="backup-settings-feedback" style={{ color: "var(--color-text-secondary)" }}>
          Saving settings...
        </div>
      )}
      {!isSaving && feedback && (
        <div
          className={`backup-settings-feedback ${
            feedback.type === "saved" ? "is-saved" : "is-error"
          }`}
          role="status"
        >
          {feedback.text}
        </div>
      )}
    </div>
  );
};

export default BackupSettingsPanel;
