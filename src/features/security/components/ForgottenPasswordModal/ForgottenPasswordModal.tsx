import React from "react";
import { Modal, Button } from "../../../../components/ui";
import "./ForgottenPasswordModal.css";

export interface ForgottenPasswordModalProps {
  isOpen: boolean;
  onClose: () => void;
}

/**
 * Forgotten Password & Recovery Policy Modal (Phase 12, Task 16).
 *
 * Implements the honest Option A (No Recovery Key / Zero-Knowledge Loss Policy):
 * - Plainly discloses that local zero-knowledge encryption prevents password recovery.
 * - Explains that no cloud master keys, backdoors, or recovery escrows exist.
 * - Clarifies that failed attempts do not damage, corrupt, or erase protected data.
 * - Warns that resetting the application creates an empty workspace but cannot recover encrypted notes.
 * - Explains the pre-migration recovery checkpoint and prior unencrypted backup restore options.
 */
export const ForgottenPasswordModal: React.FC<ForgottenPasswordModalProps> = ({
  isOpen,
  onClose,
}) => {
  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Forgotten Password & Recovery Policy"
      maxWidth="560px"
      footer={
        <div className="forgotten-password-footer">
          <Button
            variant="primary"
            size="sm"
            id="forgotten-password-close-btn"
            onClick={onClose}
          >
            I Understand
          </Button>
        </div>
      }
    >
      <div className="forgotten-password-content" id="forgotten-password-modal">
        {/* Core Policy Warning Banner */}
        <div className="forgotten-password-warning-box" role="alert">
          <div className="forgotten-password-warning-icon">
            <svg
              width="24"
              height="24"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
              <line x1="12" y1="9" x2="12" y2="13" />
              <line x1="12" y1="17" x2="12.01" y2="17" />
            </svg>
          </div>
          <div className="forgotten-password-warning-body">
            <h4>No Password Recovery or Cloud Backdoor Exists</h4>
            <p>
              Personal Notepad is 100% offline and zero-knowledge. Your encryption keys are derived
              directly from your master password using Argon2id. Without your password, your encrypted
              notes <strong>cannot be recovered by anyone</strong>.
            </p>
          </div>
        </div>

        {/* Informational Sections */}
        <div className="forgotten-password-sections">
          <div className="forgotten-password-section-item">
            <div className="section-item-header">
              <span className="section-item-icon">🔒</span>
              <h5>Failed Attempts Do Not Damage Data</h5>
            </div>
            <p>
              Entering wrong passwords will never erase, corrupt, or alter your stored notes or
              encrypted database. Your data remains safely encrypted on disk. If you remember your
              password later, you can unlock all your notes without penalty.
            </p>
          </div>

          <div className="forgotten-password-section-item">
            <div className="section-item-header">
              <span className="section-item-icon">⚠️</span>
              <h5>Resetting Is Not Recovery</h5>
            </div>
            <p>
              Personal Notepad intentionally does not provide a fake "Reset Password" button.
              Uninstalling the app, deleting the SQLite database, or starting over will create a fresh,
              empty workspace, but <strong>will never decrypt or salvage existing encrypted notes</strong>.
            </p>
          </div>

          <div className="forgotten-password-section-item">
            <div className="section-item-header">
              <span className="section-item-icon">🛡️</span>
              <h5>Pre-Migration Checkpoint & Prior Backups</h5>
            </div>
            <p>
              When encryption was first enabled on this device, Personal Notepad automatically created
              a verified, unencrypted recovery checkpoint in your application checkpoints directory.
              If you lose access permanently, you can restore that checkpoint or any prior
              unencrypted <code>.pnbackup</code> archive to recover your notes as they existed before
              encryption was activated.
            </p>
          </div>
        </div>
      </div>
    </Modal>
  );
};

export default ForgottenPasswordModal;
