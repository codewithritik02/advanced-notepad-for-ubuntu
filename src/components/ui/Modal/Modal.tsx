import React, { useEffect, useRef } from "react";
import { IconButton } from "../Button/Button";
import "./Modal.css";

export interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children: React.ReactNode;
  footer?: React.ReactNode;
  maxWidth?: string;
  /** "page" renders full-screen below the top bar instead of a floating dialog. */
  variant?: "dialog" | "page";
}

const CloseIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
    <line x1="18" y1="6" x2="6" y2="18" />
    <line x1="6" y1="6" x2="18" y2="18" />
  </svg>
);

export const Modal: React.FC<ModalProps> = ({
  isOpen,
  onClose,
  title,
  children,
  footer,
  maxWidth = "520px",
  variant = "dialog",
}) => {
  const modalRef = useRef<HTMLDivElement>(null);

  // Close on Escape key
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const isPage = variant === "page";

  return (
    <div className={`app-modal-backdrop${isPage ? " is-page" : ""}`} onClick={isPage ? undefined : onClose} role="dialog" aria-modal="true" aria-labelledby="modal-title">
      <div
        className={`app-modal-window${isPage ? " is-page" : ""}`}
        style={isPage ? undefined : { maxWidth }}
        onClick={(e) => e.stopPropagation()}
        ref={modalRef}
      >
        <div className="app-modal-header">
          <h2 id="modal-title" className="app-modal-title">
            {title}
          </h2>
          <IconButton
            icon={<CloseIcon />}
            title="Close dialog (Esc)"
            size="sm"
            onClick={onClose}
          />
        </div>

        <div className="app-modal-body">{children}</div>

        {footer && <div className="app-modal-footer">{footer}</div>}
      </div>
    </div>
  );
};
