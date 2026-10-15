import React, { useState } from "react";
import { Modal, Button, Badge } from "../../../../components/ui";
import { PasswordInput } from "../PasswordInput/PasswordInput";
import { validatePasswordMatch } from "../../utils/passwordValidation";
import { cryptoStorage, EncryptionSetupResult } from "../../../../services/storage";
import "./EncryptionSetupModal.css";

export interface EncryptionSetupModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: (result: EncryptionSetupResult) => void;
}

export const EncryptionSetupModal: React.FC<EncryptionSetupModalProps> = ({
  isOpen,
  onClose,
  onSuccess,
}) => {
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [successResult, setSuccessResult] = useState<EncryptionSetupResult | null>(null);

  const handleReset = () => {
    setPassword("");
    setConfirmPassword("");
    setErrorMessage(null);
    setSuccessResult(null);
    setIsSubmitting(false);
  };

  const handleClose = () => {
    if (isSubmitting) return;
    handleReset();
    onClose();
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setErrorMessage(null);

    const validation = validatePasswordMatch(password, confirmPassword);
    if (!validation.isValid) {
      setErrorMessage(validation.errorMessage || "Please enter matching valid passwords.");
      return;
    }

    setIsSubmitting(true);

    try {
      const result = await cryptoStorage.setupEncryption({
        password,
        confirm_password: confirmPassword,
      });

      setSuccessResult(result);
      setIsSubmitting(false);
      onSuccess(result);
    } catch (err: unknown) {
      setIsSubmitting(false);
      const msg = err instanceof Error ? err.message : String(err);
      setErrorMessage(msg || "Failed to enable encryption. Please try again.");
    }
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={handleClose}
      title="Set Up Personal Notepad Encryption"
      maxWidth="560px"
      footer={
        successResult ? (
          <Button variant="primary" size="sm" onClick={handleClose}>
            Done
          </Button>
        ) : (
          <div className="encryption-setup-footer-btns">
            <Button
              variant="secondary"
              size="sm"
              onClick={handleClose}
              disabled={isSubmitting}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={handleSubmit}
              disabled={isSubmitting || !password || !confirmPassword}
              isLoading={isSubmitting}
            >
              {isSubmitting ? "Enabling Encryption..." : "Enable Encryption"}
            </Button>
          </div>
        )
      }
    >
      <div className="encryption-setup-content">
        {successResult ? (
          <div className="encryption-success-banner" role="status">
            <div className="encryption-success-icon">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                <polyline points="22 4 12 14.01 9 11.01" />
              </svg>
            </div>
            <div className="encryption-success-details">
              <h4>Encryption Successfully Enabled</h4>
              <p>{successResult.message}</p>
              <div className="encryption-spec-badges">
                <Badge variant="accent" size="sm">
                  {successResult.algorithm}
                </Badge>
                <Badge variant="default" size="sm">
                  Argon2id (64MB)
                </Badge>
                <Badge variant="success" size="sm">
                  Verified Checkpoint Saved
                </Badge>
              </div>
            </div>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="encryption-setup-form">
            {/* Protection Scope Overview */}
            <div className="encryption-scope-box">
              <h5 className="encryption-scope-title">What Encryption Protects</h5>
              <ul className="encryption-scope-list">
                <li>
                  <strong>Note Content & Titles:</strong> Encrypted at rest using AES-256-GCM.
                </li>
                <li>
                  <strong>Attachments:</strong> Files on disk are encrypted using unique per-file nonces.
                </li>
                <li>
                  <strong>Notebooks & Tags:</strong> Organizational taxonomy is kept private.
                </li>
              </ul>
            </div>

            {/* Critical Password Loss Warning */}
            <div className="encryption-warning-box" role="alert">
              <div className="encryption-warning-icon">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
                  <line x1="12" y1="9" x2="12" y2="13" />
                  <line x1="12" y1="17" x2="12.01" y2="17" />
                </svg>
              </div>
              <div className="encryption-warning-text">
                <strong>Important: No Password Reset Possible</strong>
                <p>
                  Personal Notepad is 100% offline with zero cloud backdoors. If you forget your
                  password, your notes <strong>cannot</strong> be recovered by anyone.
                </p>
              </div>
            </div>

            {/* Pre-Migration Checkpoint Notice */}
            <div className="encryption-checkpoint-notice">
              <span className="checkpoint-icon">🛡️</span>
              <span className="checkpoint-text">
                A verified backup checkpoint of your existing unencrypted data will be automatically
                created before encryption begins.
              </span>
            </div>

            {/* Password Inputs */}
            <div className="encryption-input-fields">
              <PasswordInput
                id="setup-encryption-password"
                label="Create Encryption Password"
                value={password}
                onChange={setPassword}
                placeholder="Minimum 8 characters"
                helpText="Choose a strong, memorable passphrase."
                autoFocus
                disabled={isSubmitting}
                autoComplete="new-password"
                required
              />

              <PasswordInput
                id="setup-encryption-confirm"
                label="Confirm Password"
                value={confirmPassword}
                onChange={setConfirmPassword}
                placeholder="Re-enter password"
                disabled={isSubmitting}
                autoComplete="new-password"
                required
              />
            </div>

            {errorMessage && (
              <div className="encryption-error-alert" role="alert">
                {errorMessage}
              </div>
            )}
          </form>
        )}
      </div>
    </Modal>
  );
};
