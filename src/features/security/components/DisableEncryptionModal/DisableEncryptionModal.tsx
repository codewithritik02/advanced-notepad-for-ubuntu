import React, { useState } from "react";
import { Modal, Button } from "../../../../components/ui";
import { PasswordInput } from "../PasswordInput/PasswordInput";
import { cryptoStorage } from "../../../../services/storage";
import "./DisableEncryptionModal.css";

export interface DisableEncryptionModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess?: () => void;
  onFlushSave?: () => Promise<void>;
}

/**
 * Disable Encryption and Decrypt Storage Modal (Phase 12, Task 21).
 *
 * Implements safe, deliberate, and recoverable decryption of all application data:
 * 1. Authenticates current master password.
 * 2. Clearly discloses security consequences of removing at-rest encryption.
 * 3. Flushes any pending writes before executing reverse migration.
 * 4. Automatically creates and verifies a pre-decryption backup recovery checkpoint.
 * 5. Atomically decrypts all notes and attachments back to plain text.
 * 6. Rebuilds SQLite FTS search index and purges encryption metadata.
 */
export const DisableEncryptionModal: React.FC<DisableEncryptionModalProps> = ({
  isOpen,
  onClose,
  onSuccess,
  onFlushSave,
}) => {
  const [password, setPassword] = useState("");
  const [isConfirmed, setIsConfirmed] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [isSuccess, setIsSuccess] = useState(false);

  const handleReset = () => {
    setPassword("");
    setIsConfirmed(false);
    setErrorMessage(null);
    setIsSuccess(false);
    setIsSubmitting(false);
  };

  const handleClose = () => {
    if (isSubmitting) return;
    handleReset();
    onClose();
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!password.trim()) {
      setErrorMessage("Please enter your current master password.");
      return;
    }
    if (!isConfirmed) {
      setErrorMessage("Please confirm that you understand the security implications.");
      return;
    }

    setIsSubmitting(true);
    setErrorMessage(null);

    try {
      if (onFlushSave) {
        await onFlushSave();
      }

      await cryptoStorage.disableEncryption({ password });

      setIsSuccess(true);
      setIsSubmitting(false);
      onSuccess?.();
    } catch (err: unknown) {
      setIsSubmitting(false);
      const msg = err instanceof Error ? err.message : String(err);
      if (
        msg.includes("Authentication failed") ||
        msg.includes("tag verification failed") ||
        msg.includes("incorrect key")
      ) {
        setErrorMessage("Incorrect master password. Please verify and try again.");
      } else {
        setErrorMessage(msg || "Failed to disable encryption. Storage remains unchanged.");
      }
    }
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={handleClose}
      title="Disable Encryption & Decrypt Storage"
      maxWidth="540px"
      footer={
        isSuccess ? (
          <Button variant="primary" size="sm" onClick={handleClose}>
            Done
          </Button>
        ) : (
          <div className="disable-encryption-footer-btns">
            <Button
              variant="secondary"
              size="sm"
              id="disable-encryption-cancel-btn"
              onClick={handleClose}
              disabled={isSubmitting}
            >
              Cancel
            </Button>
            <Button
              variant="danger"
              size="sm"
              id="disable-encryption-confirm-btn"
              onClick={handleSubmit}
              disabled={isSubmitting || !password.trim() || !isConfirmed}
            >
              {isSubmitting ? "Decrypting Storage..." : "Decrypt & Disable Encryption"}
            </Button>
          </div>
        )
      }
    >
      <div className="disable-encryption-content">
        {isSuccess ? (
          <div className="disable-encryption-success" role="status">
            <div className="disable-encryption-success-icon">🔓</div>
            <h4>Encryption Successfully Disabled</h4>
            <p>
              All notes and attachments have been decrypted back to standard plain text.
              A pre-decryption recovery checkpoint was saved in your checkpoints folder.
            </p>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="disable-encryption-form">
            <div className="disable-encryption-warning-banner" role="alert">
              <span className="warning-icon">⚠️</span>
              <div className="warning-text">
                <strong>Critical Security Notice</strong>
                <p>
                  Disabling encryption permanently removes cryptographic protection from your
                  database and file attachments. All notes, metadata, and files will reside on
                  disk as readable plain text.
                </p>
              </div>
            </div>

            <div className="disable-encryption-checkpoint-info">
              <span className="info-icon">🛡️</span>
              <p>
                A verified full backup recovery checkpoint will automatically be created in your{" "}
                <code>checkpoints/</code> folder before decryption begins.
              </p>
            </div>

            <PasswordInput
              id="disable-encryption-password-input"
              label="Current Master Password"
              value={password}
              onChange={setPassword}
              disabled={isSubmitting}
              autoFocus
              autoComplete="current-password"
            />

            <label className="disable-encryption-ack-label">
              <input
                type="checkbox"
                id="disable-encryption-ack-checkbox"
                checked={isConfirmed}
                onChange={(e) => setIsConfirmed(e.target.checked)}
                disabled={isSubmitting}
              />
              <span>
                I understand that disabling encryption removes all at-rest cryptographic
                protection for my notes and attachments.
              </span>
            </label>

            {errorMessage && (
              <div className="disable-encryption-error" role="alert">
                {errorMessage}
              </div>
            )}
          </form>
        )}
      </div>
    </Modal>
  );
};
