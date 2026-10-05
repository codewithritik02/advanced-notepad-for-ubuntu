import { Component, ErrorInfo, ReactNode } from "react";
import { logger } from "../../utils/logger";
import "./SearchErrorBoundary.css";

export interface SearchErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode | ((error: Error, reset: () => void) => ReactNode);
  onReset?: () => void;
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
}

export interface SearchErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

/**
 * Task 68 — Search Error Boundary
 *
 * Catches search-related UI and rendering errors to ensure that a search failure
 * is isolated and never crashes:
 * - sidebar
 * - note list
 * - editor
 *
 * Allows the user to clear search and safely continue browsing existing notes.
 */
export class SearchErrorBoundary extends Component<
  SearchErrorBoundaryProps,
  SearchErrorBoundaryState
> {
  public state: SearchErrorBoundaryState = {
    hasError: false,
    error: null,
  };

  public static getDerivedStateFromError(error: Error): SearchErrorBoundaryState {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    // Log error securely without exposing note content or user data
    logger.error("SearchErrorBoundary caught an isolated error:", error?.message ?? "Unknown error");
    if (this.props.onError) {
      this.props.onError(error, errorInfo);
    }
  }

  public handleReset = (): void => {
    this.setState({ hasError: false, error: null });
    if (this.props.onReset) {
      this.props.onReset();
    }
  };

  public render(): ReactNode {
    if (this.state.hasError) {
      if (typeof this.props.fallback === "function") {
        return this.props.fallback(
          this.state.error ?? new Error("Unknown search error"),
          this.handleReset
        );
      }
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <div
          className="search-error-boundary"
          role="alert"
          aria-live="assertive"
          data-testid="search-error-boundary"
        >
          <div className="search-error-boundary-content">
            <div className="search-error-boundary-icon" aria-hidden="true">
              <svg
                width="28"
                height="28"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <circle cx="11" cy="11" r="8" />
                <line x1="21" y1="21" x2="16.65" y2="16.65" />
                <line x1="11" y1="8" x2="11" y2="12" />
                <line x1="11" y1="16" x2="11.01" y2="16" />
              </svg>
            </div>
            <h3 className="search-error-boundary-title">Search Error</h3>
            <p className="search-error-boundary-message">
              Search encountered an unexpected display error. Your notes and editor remain safe and intact.
            </p>
            <div className="search-error-boundary-actions">
              <button
                type="button"
                className="search-error-boundary-btn"
                onClick={this.handleReset}
              >
                Clear Search & Browse Notes
              </button>
            </div>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

export default SearchErrorBoundary;
