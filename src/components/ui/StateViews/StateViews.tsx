import React from "react";
import { Button } from "../Button/Button";
import "./StateViews.css";

/* ==========================================================================
   Empty State
   ========================================================================== */
export interface EmptyStateProps {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  actionText?: string;
  onAction?: () => void;
}

export const EmptyState: React.FC<EmptyStateProps> = ({
  icon,
  title,
  description,
  actionText,
  onAction,
}) => {
  return (
    <div className="state-view state-view-empty">
      {icon && <div className="state-view-icon">{icon}</div>}
      <h3 className="state-view-title">{title}</h3>
      {description && <p className="state-view-description">{description}</p>}
      {actionText && onAction && (
        <Button variant="primary" size="sm" onClick={onAction}>
          {actionText}
        </Button>
      )}
    </div>
  );
};

/* ==========================================================================
   Error State
   ========================================================================== */
export interface ErrorStateProps {
  title?: string;
  message: string;
  onRetry?: () => void;
}

export const ErrorState: React.FC<ErrorStateProps> = ({
  title = "Something went wrong",
  message,
  onRetry,
}) => {
  return (
    <div className="state-view state-view-error">
      <div className="state-view-icon state-view-icon-error">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="8" x2="12" y2="12" />
          <line x1="12" y1="16" x2="12.01" y2="16" />
        </svg>
      </div>
      <h3 className="state-view-title">{title}</h3>
      <p className="state-view-description">{message}</p>
      {onRetry && (
        <Button variant="secondary" size="sm" onClick={onRetry}>
          Try Again
        </Button>
      )}
    </div>
  );
};

/* ==========================================================================
   Loading State (Skeleton)
   ========================================================================== */
export interface LoadingStateProps {
  count?: number;
}

export const LoadingState: React.FC<LoadingStateProps> = ({ count = 3 }) => {
  return (
    <div className="state-view-loading" aria-label="Loading content">
      {Array.from({ length: count }).map((_, i) => (
        <div key={i} className="state-skeleton-card">
          <div className="state-skeleton-line state-skeleton-title" />
          <div className="state-skeleton-line state-skeleton-body" />
          <div className="state-skeleton-line state-skeleton-meta" />
        </div>
      ))}
    </div>
  );
};
