import React, { useState, useEffect } from "react";
import { Modal, Button, Badge, Divider } from "../../../../components/ui";
import { ThemeMode } from "../../../../types";
import { BackupSettingsPanel } from "../../../backup";
import { cryptoStorage, EncryptionStatusDto, EncryptionSetupResult } from "../../../../services/storage";
import {
  EncryptionSetupModal,
  AutoLockSettings,
  ChangePasswordModal,
  ForgottenPasswordModal,
  DisableEncryptionModal,
} from "../../../security";
import {
  MIN_ZOOM_PERCENT,
  MAX_ZOOM_PERCENT,
  DEFAULT_ZOOM_PERCENT,
  MIN_EDITOR_FONT_SIZE_PX,
  MAX_EDITOR_FONT_SIZE_PX,
  DEFAULT_EDITOR_FONT_SIZE_PX,
} from "../../utils/displaySettings";
import "./SettingsModal.css";

export interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  currentTheme: ThemeMode;
  onSelectTheme: (theme: ThemeMode) => void;
  zoomPercent: number;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onZoomReset: () => void;
  editorFontSize: number;
  onEditorFontSizeChange: (px: number) => void;
  onEditorFontSizeReset: () => void;
  onCreateBackup?: () => void;
  onViewBackups?: () => void;
  onLock?: () => Promise<void> | void;
  onFlushPendingSave?: () => Promise<void>;
  onEncryptionStatusChange?: (enabled: boolean) => void;
  autoLockSettings?: AutoLockSettings;
  onUpdateAutoLockSettings?: (settings: Partial<AutoLockSettings>) => Promise<void>;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  currentTheme,
  onSelectTheme,
  zoomPercent,
  onZoomIn,
  onZoomOut,
  onZoomReset,
  editorFontSize,
  onEditorFontSizeChange,
  onEditorFontSizeReset,
  onCreateBackup,
  onViewBackups,
  onLock,
  onFlushPendingSave,
  onEncryptionStatusChange,
  autoLockSettings,
  onUpdateAutoLockSettings,
}) => {
  const [encryptionStatus, setEncryptionStatus] = useState<EncryptionStatusDto | null>(null);
  const [isSetupModalOpen, setIsSetupModalOpen] = useState(false);
  const [isChangePasswordOpen, setIsChangePasswordOpen] = useState(false);
  const [isRecoveryPolicyOpen, setIsRecoveryPolicyOpen] = useState(false);
  const [isDisableEncryptionOpen, setIsDisableEncryptionOpen] = useState(false);
  const [isLocking, setIsLocking] = useState(false);
  const [encryptionMessage, setEncryptionMessage] = useState<string | null>(null);

  const fetchEncryptionStatus = async () => {
    try {
      const status = await cryptoStorage.getEncryptionStatus();
      setEncryptionStatus(status);
      onEncryptionStatusChange?.(status.enabled);
    } catch {
      // In mock/test environments or if backend not initialized, keep default
    }
  };

  useEffect(() => {
    if (isOpen) {
      fetchEncryptionStatus();
    }
  }, [isOpen]);

  const handleLock = async () => {
    setIsLocking(true);
    setEncryptionMessage(null);
    try {
      if (onLock) {
        await onLock();
      } else {
        await cryptoStorage.lockApplication();
        await fetchEncryptionStatus();
      }
      setEncryptionMessage("Application locked successfully.");
      onClose();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setEncryptionMessage(`Failed to lock: ${msg}`);
    } finally {
      setIsLocking(false);
    }
  };

  const handleSetupSuccess = async (_result: EncryptionSetupResult) => {
    await fetchEncryptionStatus();
    setEncryptionMessage("Encryption successfully configured and unlocked.");
    onEncryptionStatusChange?.(true);
  };

  return (
    <>
      <Modal
        isOpen={isOpen}
        onClose={onClose}
        title="Settings"
        maxWidth="540px"
        footer={
          <Button variant="secondary" size="sm" onClick={onClose}>
            Done
          </Button>
        }
      >
        <div className="settings-content">
          {/* Appearance Section */}
          <div className="settings-section">
            <h4 className="settings-section-title">Appearance</h4>
            <p className="settings-section-desc">
              Choose your preferred interface theme.
            </p>
            <div className="theme-toggle-group">
              <Button
                variant={currentTheme === "light" ? "primary" : "secondary"}
                size="sm"
                onClick={() => onSelectTheme("light")}
              >
                Light
              </Button>
              <Button
                variant={currentTheme === "dark" ? "primary" : "secondary"}
                size="sm"
                onClick={() => onSelectTheme("dark")}
              >
                Dark
              </Button>
              <Button
                variant={currentTheme === "system" ? "primary" : "secondary"}
                size="sm"
                onClick={() => onSelectTheme("system")}
              >
                System Default
              </Button>
            </div>
          </div>

          <Divider spacing="md" />

          {/* Display Section */}
          <div className="settings-section">
            <h4 className="settings-section-title">Display</h4>
            <p className="settings-section-desc">
              Make the whole app larger or smaller. Shortcuts: Ctrl + / Ctrl −
              to zoom, Ctrl 0 to reset (Ctrl + mouse wheel also works).
            </p>
            <div className="display-setting-row">
              <span className="display-setting-label">Interface zoom</span>
              <div className="display-stepper">
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={onZoomOut}
                  disabled={zoomPercent <= MIN_ZOOM_PERCENT}
                  aria-label="Zoom out"
                >
                  −
                </Button>
                <span className="display-stepper-value" aria-live="polite">
                  {zoomPercent}%
                </span>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={onZoomIn}
                  disabled={zoomPercent >= MAX_ZOOM_PERCENT}
                  aria-label="Zoom in"
                >
                  +
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={onZoomReset}
                  disabled={zoomPercent === DEFAULT_ZOOM_PERCENT}
                >
                  Reset
                </Button>
              </div>
            </div>
            <div className="display-setting-row">
              <label className="display-setting-label" htmlFor="editor-font-size">
                Note text size
              </label>
              <div className="display-stepper">
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => onEditorFontSizeChange(editorFontSize - 1)}
                  disabled={editorFontSize <= MIN_EDITOR_FONT_SIZE_PX}
                  aria-label="Decrease note text size by 1 pixel"
                >
                  −
                </Button>
                <input
                  id="editor-font-size"
                  className="display-slider"
                  type="range"
                  min={MIN_EDITOR_FONT_SIZE_PX}
                  max={MAX_EDITOR_FONT_SIZE_PX}
                  step={1}
                  value={editorFontSize}
                  onChange={(e) => onEditorFontSizeChange(Number(e.target.value))}
                />
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => onEditorFontSizeChange(editorFontSize + 1)}
                  disabled={editorFontSize >= MAX_EDITOR_FONT_SIZE_PX}
                  aria-label="Increase note text size by 1 pixel"
                >
                  +
                </Button>
                <span className="display-stepper-value" aria-live="polite">
                  {editorFontSize} px
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={onEditorFontSizeReset}
                  disabled={editorFontSize === DEFAULT_EDITOR_FONT_SIZE_PX}
                >
                  Reset
                </Button>
              </div>
            </div>
          </div>

          <Divider spacing="md" />

          {/* Backup & Data Safety Section */}
          <div className="settings-section">
            <h4 className="settings-section-title">Backup & Data Safety</h4>
            <p className="settings-section-desc">
              Configure automatic scheduled snapshots and create on-demand manual archives.
            </p>
            <BackupSettingsPanel
              onCreateManualBackup={onCreateBackup}
              onViewBackups={onViewBackups}
            />
          </div>

          <Divider spacing="md" />

          {/* Security & Encryption Section */}
          <div className="settings-section">
            <div className="settings-section-header">
              <h4 className="settings-section-title">Security & Encryption</h4>
              {encryptionStatus && (
                <Badge
                  variant={
                    !encryptionStatus.enabled
                      ? "default"
                      : !encryptionStatus.is_locked
                      ? "success"
                      : "warning"
                  }
                  size="sm"
                >
                  {!encryptionStatus.enabled
                    ? "Plaintext (Not Encrypted)"
                    : !encryptionStatus.is_locked
                    ? "Enabled & Unlocked"
                    : "Enabled & Locked"}
                </Badge>
              )}
            </div>
            <p className="settings-section-desc">
              Protect notes, attachments, notebooks, and tags at rest using authenticated
              AES-256-GCM encryption with Argon2id key derivation.
            </p>

            {encryptionMessage && (
              <p
                style={{
                  fontSize: "12px",
                  color: "var(--color-accent-primary, #3b82f6)",
                  marginBottom: "8px",
                }}
              >
                {encryptionMessage}
              </p>
            )}

            {!encryptionStatus?.enabled ? (
              <div className="encryption-settings-actions">
                <Button
                  variant="primary"
                  size="sm"
                  onClick={() => setIsSetupModalOpen(true)}
                >
                  Enable Encryption...
                </Button>
              </div>
            ) : (
              <div className="encryption-active-panel">
                <div className="encryption-spec-info">
                  <span>
                    <strong>Cipher:</strong> {encryptionStatus.algorithm || "AES-256-GCM"}
                  </span>
                  <span>
                    <strong>KDF:</strong> {encryptionStatus.kdf_algorithm || "Argon2id"}
                  </span>
                  <span>
                    <strong>State:</strong>{" "}
                    {!encryptionStatus.is_locked ? "Active Session (Unlocked)" : "Locked"}
                  </span>
                </div>

                {autoLockSettings && onUpdateAutoLockSettings && (
                  <div className="security-autolock-config">
                    <div className="security-autolock-divider" />

                    {/* Auto-Lock Toggle */}
                    <div className="security-toggle-row">
                      <div className="security-toggle-label">
                        <span className="security-toggle-title">Auto-Lock on Inactivity</span>
                        <p className="security-toggle-desc">
                          Automatically lock application and scrub encryption keys when inactive.
                        </p>
                      </div>
                      <label className="security-switch">
                        <input
                          type="checkbox"
                          id="security-autolock-toggle"
                          checked={autoLockSettings.autoLockEnabled}
                          onChange={(e) =>
                            onUpdateAutoLockSettings({ autoLockEnabled: e.target.checked })
                          }
                        />
                        <span className="security-slider" />
                      </label>
                    </div>

                    {/* Timeout Duration */}
                    <div className="security-config-row">
                      <div className="security-config-label">
                        <span className="security-config-title">Inactivity Timeout</span>
                        <p className="security-config-desc">
                          Duration of inactivity before automatically locking the workspace.
                        </p>
                      </div>
                      <select
                        id="security-timeout-select"
                        className="security-select"
                        value={autoLockSettings.inactivityTimeoutMinutes}
                        disabled={!autoLockSettings.autoLockEnabled}
                        onChange={(e) =>
                          onUpdateAutoLockSettings({
                            inactivityTimeoutMinutes: Number(e.target.value),
                          })
                        }
                      >
                        <option value="1">1 minute (Strict)</option>
                        <option value="5">5 minutes</option>
                        <option value="15">15 minutes (Default)</option>
                        <option value="30">30 minutes</option>
                        <option value="60">60 minutes (1 hour)</option>
                      </select>
                    </div>

                    {/* Lock on Sleep / Resume */}
                    <div className="security-toggle-row">
                      <div className="security-toggle-label">
                        <span className="security-toggle-title">Lock on System Sleep / Resume</span>
                        <p className="security-toggle-desc">
                          Lock immediately when the operating system enters or wakes from sleep.
                        </p>
                      </div>
                      <label className="security-switch">
                        <input
                          type="checkbox"
                          id="security-sleep-toggle"
                          checked={autoLockSettings.lockOnSleep}
                          disabled={!autoLockSettings.autoLockEnabled}
                          onChange={(e) =>
                            onUpdateAutoLockSettings({ lockOnSleep: e.target.checked })
                          }
                        />
                        <span className="security-slider" />
                      </label>
                    </div>
                  </div>
                )}

                <div className="encryption-active-actions">
                  {!encryptionStatus.is_locked && (
                    <>
                      <Button
                        variant="secondary"
                        size="sm"
                        id="settings-change-password-btn"
                        onClick={() => setIsChangePasswordOpen(true)}
                      >
                        Change Password...
                      </Button>
                      <Button
                        variant="secondary"
                        size="sm"
                        id="settings-recovery-policy-btn"
                        onClick={() => setIsRecoveryPolicyOpen(true)}
                      >
                        Recovery Policy...
                      </Button>
                      <Button
                        variant="secondary"
                        size="sm"
                        id="settings-disable-encryption-btn"
                        onClick={() => setIsDisableEncryptionOpen(true)}
                      >
                        Disable Encryption...
                      </Button>
                      <Button
                        variant="secondary"
                        size="sm"
                        id="settings-lock-btn"
                        onClick={handleLock}
                        disabled={isLocking}
                      >
                        {isLocking ? "Locking..." : "Lock Application"}
                      </Button>
                    </>
                  )}
                </div>
              </div>
            )}
          </div>

          <Divider spacing="md" />

          {/* About Section */}
          <div className="settings-section">
            <h4 className="settings-section-title">About</h4>
            <p className="settings-about-version">Personal Notepad · Version 0.1.0</p>
          </div>
      </div>
    </Modal>

    <EncryptionSetupModal
      isOpen={isSetupModalOpen}
      onClose={() => setIsSetupModalOpen(false)}
      onSuccess={handleSetupSuccess}
    />

    <ChangePasswordModal
      isOpen={isChangePasswordOpen}
      onClose={() => setIsChangePasswordOpen(false)}
      onSuccess={async () => {
        await fetchEncryptionStatus();
        setEncryptionMessage("Password changed successfully.");
      }}
    />

    <ForgottenPasswordModal
      isOpen={isRecoveryPolicyOpen}
      onClose={() => setIsRecoveryPolicyOpen(false)}
    />

    <DisableEncryptionModal
      isOpen={isDisableEncryptionOpen}
      onClose={() => setIsDisableEncryptionOpen(false)}
      onFlushSave={onFlushPendingSave}
      onSuccess={async () => {
        setEncryptionMessage("Encryption has been successfully disabled. Storage is now stored as plain text.");
        await fetchEncryptionStatus();
      }}
    />
  </>
  );
};

export default SettingsModal;
