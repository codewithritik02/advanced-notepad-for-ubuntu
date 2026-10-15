import React, { useState } from "react";
import { Modal, Button } from "../../../../components/ui";
import { PasswordInput } from "../PasswordInput/PasswordInput";
import { validateChangePassword } from "../../utils/passwordValidation";
import { cryptoStorage } from "../../../../services/storage";
import "./ChangePasswordModal.css";

export interface ChangePasswordModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess?: () => void;
}

/**
 * Change Password and Key Rewrapping Modal (Phase 12, Task 15).
 *
 * Implements safe master password modification:
 * - Authenticates current password.
 * - Enforces minimum 8-character and maximum 1024-character bounds.
 * - Enforces difference between new password and current password.
 * - Rewraps existing Data Encryption Key (DEK) with newly derived Key Encryption Key (KEK).
 * - Avoids re-encrypting existing note ciphertexts.
 */
export const ChangePasswordModal: React.FC<ChangePasswordModalProps> = ({
  isOpen,
  onClose,
  onSuccess,
}) => {
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmNewPassword, setConfirmNewPassword] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [isSuccess, setIsSuccess] = useState(false);

  const handleReset = () => {
    setCurrentPassword("");
    setNewPassword("");
    setConfirmNewPassword("");
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
    setErrorMessage(null);

    const validation = validateChangePassword(currentPassword, newPassword, confirmNewPassword);
    if (!validation.isValid) {
      setErrorMessage(validation.errorMessage || "Please enter valid password details.");
      return;
    }

    setIsSubmitting(true);

    try {
      await cryptoStorage.changePassword({
        current_password: currentPassword,
        new_password: newPassword,
        confirm_new_password: confirmNewPassword,
      });

      setIsSuccess(true);
      setIsSubmitting(false);
      onSuccess?.();
    } catch (err: unknown) {
      setIsSubmitting(false);
      const msg = err instanceof Error ? err.message : String(err);
      setErrorMessage(
        msg.includes("Authentication failed")
          ? "Incorrect current password. Please try again."
          : msg || "Failed to update encryption password."
      );
    }
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={handleClose}
      title="Change Encryption Password"
      maxWidth="500px"
      footer={
        isSuccess ? (
          <Button variant="primary" size="sm" onClick={handleClose}>
            Done
          </Button>
        ) : (
          <div className="change-password-footer-btns">
            <Button
              variant="secondary"
              size="sm"
              id="change-password-cancel-btn"
              onClick={handleClose}
              disabled={isSubmitting}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              size="sm"
              id="change-password-submit-btn"
              onClick={handleSubmit}
              disabled={isSubmitting || !currentPassword || !newPassword || !confirmNewPassword}
              isLoading={isSubmitting}
            >
              {isSubmitting ? "Changing Password..." : "Update Password"}
            </Button>
          </div>
        )
      }
    >
      <div className="change-password-content">
        {isSuccess ? (
          <div className="change-password-success-banner" role="status">
            <div className="change-password-success-icon">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                <polyline points="22 4 12 14.01 9 11.01" />
              </svg>
            </div>
            <div className="change-password-success-details">
              <h4>Password Changed Successfully</h4>
              <p>
                Your master encryption password has been updated. Your Data Encryption Key (DEK)
                was safely re-wrapped with fresh Argon2id key derivation parameters.
              </p>
            </div>
          </div>
        ) : (
          <form className="change-password-form" onSubmit={handleSubmit}>
            <p className="change-password-intro">
              Update your master password. This re-encrypts your key-encryption key (KEK) using fresh
              salt and Argon2id parameters without requiring re-encryption of existing notes.
            </p>

            {errorMessage && (
              <div className="change-password-error-alert" role="alert">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <line x1="12" y1="8" x2="12" y2="12" />
                  <line x1="12" y1="16" x2="12.01" y2="16" />
                </svg>
                <span>{errorMessage}</span>
              </div>
            )}

            <PasswordInput
              id="current-password-input"
              label="Current Password"
              value={currentPassword}
              onChange={setCurrentPassword}
              placeholder="Enter current password"
              disabled={isSubmitting}
              autoFocus
            />

            <PasswordInput
              id="new-password-input"
              label="New Password"
              value={newPassword}
              onChange={setNewPassword}
              placeholder="Enter new password (min. 8 characters)"
              helpText="Must be at least 8 characters long and different from current password."
              disabled={isSubmitting}
            />

            <PasswordInput
              id="confirm-new-password-input"
              label="Confirm New Password"
              value={confirmNewPassword}
              onChange={setConfirmNewPassword}
              placeholder="Re-enter new password"
              disabled={isSubmitting}
            />
          </form>
        )}
      </div>
    </Modal>
  );
};

export default ChangePasswordModal;
