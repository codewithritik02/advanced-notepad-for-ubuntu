import React from "react";
import { Modal, Button, Badge, Divider } from "../../../../components/ui";
import { ThemeMode } from "../../../../types";
import "./SettingsModal.css";

export interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  currentTheme: ThemeMode;
  onSelectTheme: (theme: ThemeMode) => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  currentTheme,
  onSelectTheme,
}) => {
  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Settings & Application Info"
      maxWidth="480px"
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

        {/* Application Info Section */}
        <div className="settings-section">
          <h4 className="settings-section-title">About Personal Notepad</h4>
          <div className="settings-info-grid">
            <div className="info-row">
              <span className="info-label">Application:</span>
              <span className="info-value">Personal Notepad</span>
            </div>
            <div className="info-row">
              <span className="info-label">Version:</span>
              <span className="info-value">0.1.0</span>
            </div>
            <div className="info-row">
              <span className="info-label">Phase:</span>
              <Badge variant="accent" size="sm">
                Phase 1 Shell
              </Badge>
            </div>
            <div className="info-row">
              <span className="info-label">Target Platform:</span>
              <span className="info-value">Ubuntu / Linux</span>
            </div>
            <div className="info-row">
              <span className="info-label">Privacy & AI:</span>
              <Badge variant="success" size="sm">
                100% Offline & Non-AI
              </Badge>
            </div>
          </div>
        </div>
      </div>
    </Modal>
  );
};

export default SettingsModal;
