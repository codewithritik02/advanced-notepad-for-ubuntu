import React, { useState, useEffect, useCallback } from "react";
import { PasswordInput } from "../PasswordInput/PasswordInput";
import { Button } from "../../../../components/ui/Button/Button";
import { cryptoStorage } from "../../../../services/storage";
import { ForgottenPasswordModal } from "../ForgottenPasswordModal/ForgottenPasswordModal";
import "./UnlockScreen.css";

export interface UnlockScreenProps {
  onUnlockSuccess?: () => void;
  isInterrupted?: boolean;
  onRollbackSuccess?: () => void;
}

export const UnlockScreen: React.FC<UnlockScreenProps> = ({
  onUnlockSuccess,
  isInterrupted = false,
  onRollbackSuccess,
}) => {
  const [password, setPassword] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [infoMessage, setInfoMessage] = useState<string | null>(null);
  const [shake, setShake] = useState(false);
  const [isRecoveryModalOpen, setIsRecoveryModalOpen] = useState(false);
  const [showResumeForm, setShowResumeForm] = useState(false);

  // Auto-focus input upon mounting
  useEffect(() => {
    const timer = setTimeout(() => {
      const inputEl = document.getElementById(
        isInterrupted ? "recovery-resume-input" : "unlock-password-input"
      ) as HTMLInputElement | null;
      inputEl?.focus();
    }, 50);
    return () => clearTimeout(timer);
  }, [isInterrupted]);

  const handleUnlock = useCallback(async (e?: React.FormEvent) => {
    if (e) {
      e.preventDefault();
    }

    const trimmed = password.trim();
    if (!trimmed) {
      setErrorMessage("Please enter your encryption password.");
      setShake(true);
      setTimeout(() => setShake(false), 500);
      return;
    }

    setIsSubmitting(true);
    setErrorMessage(null);

    try {
      if (isInterrupted) {
        await cryptoStorage.resumeInterruptedMigration({ password: trimmed });
      } else {
        await cryptoStorage.unlockApplication({ password: trimmed });
      }
      setPassword("");
      if (onUnlockSuccess) {
        onUnlockSuccess();
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      const friendlyMsg =
        msg.includes("Authentication failed") || msg.includes("invalid passphrase")
          ? "Incorrect encryption password. Please try again."
          : `Operation failed: ${msg}`;
      setErrorMessage(friendlyMsg);
      setShake(true);
      setTimeout(() => setShake(false), 500);
      const inputId = isInterrupted ? "recovery-resume-input" : "unlock-password-input";
      const inputEl = document.getElementById(inputId) as HTMLInputElement | null;
      inputEl?.focus();
      inputEl?.select();
    } finally {
      setIsSubmitting(false);
    }
  }, [password, isInterrupted, onUnlockSuccess]);

  const handleRollback = useCallback(async () => {
    setIsSubmitting(true);
    setErrorMessage(null);
    setInfoMessage("Rolling back storage to pre-migration backup checkpoint...");

    try {
      const resultMsg = await cryptoStorage.rollbackInterruptedMigration();
      setInfoMessage(resultMsg);
      setTimeout(() => {
        if (onRollbackSuccess) {
          onRollbackSuccess();
        }
      }, 800);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setErrorMessage(`Rollback failed: ${msg}`);
      setInfoMessage(null);
      setShake(true);
      setTimeout(() => setShake(false), 500);
    } finally {
      setIsSubmitting(false);
    }
  }, [onRollbackSuccess]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && !isSubmitting) {
      e.preventDefault();
      handleUnlock();
    }
  };

  return (
    <main
      className="unlock-screen-wrapper"
      role="main"
      aria-label={isInterrupted ? "Restricted Recovery Mode View" : "Application Locked View"}
      id="unlock-screen-view"
    >
      <div className={`unlock-card ${shake ? "unlock-shake" : ""} ${isInterrupted ? "unlock-card-recovery" : ""}`}>
        {/* Header Badge */}
        <div className="unlock-header">
          <div
            className={`unlock-icon-badge ${isInterrupted ? "unlock-icon-badge-recovery" : ""}`}
            aria-hidden="true"
          >
            {isInterrupted ? (
              <svg
                width="36"
                height="36"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                className="unlock-icon"
              >
                <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
                <line x1="12" y1="9" x2="12" y2="13" />
                <line x1="12" y1="17" x2="12.01" y2="17" />
              </svg>
            ) : (
              <svg
                width="36"
                height="36"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                className="unlock-icon"
              >
                <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                <path d="M7 11V7a5 5 0 0 1 10 0v4" />
              </svg>
            )}
          </div>
          <h1 id={isInterrupted ? "recovery-heading" : "unlock-heading"} className="unlock-title">
            {isInterrupted ? "Interrupted Migration Recovery" : "Application Locked"}
          </h1>
          <p id={isInterrupted ? "recovery-subtitle" : "unlock-subtitle"} className="unlock-subtitle">
            {isInterrupted
              ? "An interrupted encryption migration was detected (e.g. from an application crash or power loss). To prevent data corruption, normal editing is suspended."
              : "Enter your master encryption password to unlock your notes and protected attachments."}
          </p>
        </div>

        {/* Info Banner */}
        {infoMessage && (
          <div role="status" className="unlock-info-alert" id="unlock-info-alert">
            <span className="unlock-info-text">{infoMessage}</span>
          </div>
        )}

        {/* Error Alert Banner */}
        {errorMessage && (
          <div
            role="alert"
            className="unlock-error-alert"
            id="unlock-error-alert"
            aria-live="assertive"
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="unlock-error-icon"
              aria-hidden="true"
            >
              <circle cx="12" cy="12" r="10" />
              <line x1="12" y1="8" x2="12" y2="12" />
              <line x1="12" y1="16" x2="12.01" y2="16" />
            </svg>
            <span className="unlock-error-text">{errorMessage}</span>
          </div>
        )}

        {/* Interrupted Recovery Form */}
        {isInterrupted ? (
          <div className="recovery-actions-container" id="recovery-actions-view">
            <div className="recovery-option-card">
              <h3 className="recovery-option-title">Option 1: Roll Back (Recommended)</h3>
              <p className="recovery-option-desc">
                Revert all notes and attachments to the verified unencrypted pre-migration backup checkpoint created before setup began.
              </p>
              <Button
                id="recovery-rollback-btn"
                type="button"
                variant="primary"
                size="md"
                disabled={isSubmitting}
                onClick={handleRollback}
              >
                {isSubmitting && !showResumeForm ? "Rolling Back Storage..." : "Roll Back to Pre-Migration State"}
              </Button>
            </div>

            <div className="recovery-divider">
              <span>OR</span>
            </div>

            <div className="recovery-option-card">
              <h3 className="recovery-option-title">Option 2: Resume Migration</h3>
              <p className="recovery-option-desc">
                Provide your master password to resume encrypting remaining notes and finalize migration.
              </p>
              {!showResumeForm ? (
                <Button
                  id="recovery-show-resume-btn"
                  type="button"
                  variant="secondary"
                  size="md"
                  disabled={isSubmitting}
                  onClick={() => setShowResumeForm(true)}
                >
                  Enter Password to Resume...
                </Button>
              ) : (
                <form onSubmit={handleUnlock} className="recovery-resume-form" noValidate>
                  <PasswordInput
                    id="recovery-resume-input"
                    label="Master Password"
                    value={password}
                    onChange={(val) => {
                      setPassword(val);
                      if (errorMessage) setErrorMessage(null);
                    }}
                    placeholder="Enter password..."
                    disabled={isSubmitting}
                    autoFocus
                    onKeyDown={handleKeyDown}
                    showToggle
                    required
                  />
                  <div className="recovery-resume-actions">
                    <Button
                      id="recovery-resume-btn"
                      type="submit"
                      variant="primary"
                      size="md"
                      disabled={isSubmitting || !password.trim()}
                    >
                      {isSubmitting ? "Resuming..." : "Resume & Complete"}
                    </Button>
                    <Button
                      type="button"
                      variant="secondary"
                      size="md"
                      disabled={isSubmitting}
                      onClick={() => setShowResumeForm(false)}
                    >
                      Cancel
                    </Button>
                  </div>
                </form>
              )}
            </div>
          </div>
        ) : (
          /* Normal Unlock Form */
          <form onSubmit={handleUnlock} className="unlock-form" noValidate>
            <div className="unlock-input-container">
              <PasswordInput
                id="unlock-password-input"
                label="Master Password"
                value={password}
                onChange={(val) => {
                  setPassword(val);
                  if (errorMessage) setErrorMessage(null);
                }}
                placeholder="Enter master password..."
                disabled={isSubmitting}
                autoFocus
                autoComplete="current-password"
                onKeyDown={handleKeyDown}
                showToggle
                required
              />
            </div>

            <div className="unlock-actions">
              <Button
                id="unlock-submit-button"
                type="submit"
                variant="primary"
                size="lg"
                disabled={isSubmitting || !password.trim()}
              >
                {isSubmitting ? (
                  <span className="unlock-button-loading">
                    <span className="unlock-spinner" aria-hidden="true" />
                    Unlocking...
                  </span>
                ) : (
                  "Unlock Application"
                )}
              </Button>
            </div>

            <div className="unlock-secondary-actions">
              <button
                type="button"
                id="unlock-forgot-password-btn"
                className="unlock-forgot-link"
                onClick={() => setIsRecoveryModalOpen(true)}
              >
                Forgotten password or lost access?
              </button>
            </div>
          </form>
        )}

        {/* Security & Offline Privacy Assurance */}
        <footer className="unlock-footer">
          <div className="unlock-security-pill">
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
            </svg>
            <span>AES-256-GCM Zero-Knowledge Security</span>
          </div>
          <p className="unlock-security-detail">
            Decryption is strictly local via Argon2id key derivation. Your notes remain protected in memory and at rest.
          </p>
        </footer>
      </div>

      <ForgottenPasswordModal
        isOpen={isRecoveryModalOpen}
        onClose={() => setIsRecoveryModalOpen(false)}
      />
    </main>
  );
};

export default UnlockScreen;
