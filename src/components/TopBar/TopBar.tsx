import React, { useRef } from "react";
import { ThemeMode } from "../../types";
import "./TopBar.css";

export interface TopBarProps {
  searchQuery?: string;
  isSearching?: boolean;
  onSearchChange?: (query: string) => void;
  onSearchSubmit?: () => void;
  onSearchClear?: () => void;
  onNewNoteClick?: () => void;
  onThemeToggle?: () => void;
  onSettingsClick?: () => void;
  theme?: ThemeMode;
  searchInputRef?: React.RefObject<HTMLInputElement | null>;
}

// Lightweight inline SVG icons
const SearchIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="11" cy="11" r="8" />
    <line x1="21" y1="21" x2="16.65" y2="16.65" />
  </svg>
);

const SearchSpinnerIcon = () => (
  <svg className="search-spinner-icon" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" aria-label="Searching">
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" />
    <path d="M12 3a9 9 0 0 1 9 9" stroke="currentColor" />
  </svg>
);

const ClearIcon = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
    <line x1="18" y1="6" x2="6" y2="18" />
    <line x1="6" y1="6" x2="18" y2="18" />
  </svg>
);

const PlusIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <line x1="12" y1="5" x2="12" y2="19" />
    <line x1="5" y1="12" x2="19" y2="12" />
  </svg>
);

const SunIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="12" cy="12" r="5" />
    <line x1="12" y1="1" x2="12" y2="3" />
    <line x1="12" y1="21" x2="12" y2="23" />
    <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
    <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
    <line x1="1" y1="12" x2="3" y2="12" />
    <line x1="21" y1="12" x2="23" y2="12" />
    <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
    <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
  </svg>
);

const MoonIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
  </svg>
);

const SystemIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
    <line x1="8" y1="21" x2="16" y2="21" />
    <line x1="12" y1="17" x2="12" y2="21" />
  </svg>
);

const SettingsIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
  </svg>
);

export const TopBar: React.FC<TopBarProps> = ({
  searchQuery = "",
  isSearching = false,
  onSearchChange,
  onSearchSubmit,
  onSearchClear,
  onNewNoteClick,
  onThemeToggle,
  onSettingsClick,
  theme = "system",
  searchInputRef: externalSearchRef,
}) => {
  const internalSearchRef = useRef<HTMLInputElement>(null);
  const searchInputRef = externalSearchRef ?? internalSearchRef;

  const handleClearSearch = () => {
    if (onSearchClear) {
      onSearchClear();
    } else {
      onSearchChange?.("");
    }
    searchInputRef.current?.focus();
  };

  const renderThemeIcon = () => {
    switch (theme) {
      case "light":
        return <SunIcon />;
      case "dark":
        return <MoonIcon />;
      case "system":
      default:
        return <SystemIcon />;
    }
  };

  return (
    <div className="topbar-content">
      {/* Left: App Identity Branding */}
      <div className="topbar-brand">
        <span className="topbar-brand-title">Personal Notepad</span>
      </div>

      {/* Middle: Visual Search Placeholder Input */}
      <div className="topbar-search-container">
        <div
          className="topbar-search-bar"
          onClick={() => searchInputRef.current?.focus()}
        >
          <span className="search-icon" title={isSearching ? "Searching..." : "Search"}>
            {isSearching ? <SearchSpinnerIcon /> : <SearchIcon />}
          </span>
          <input
            ref={searchInputRef}
            type="text"
            className="search-input selectable-text"
            placeholder="Search notes... (Ctrl+K)"
            value={searchQuery}
            onChange={(e) => onSearchChange?.(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                onSearchSubmit?.();
              } else if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                if (onSearchClear) {
                  onSearchClear();
                } else {
                  onSearchChange?.("");
                }
                searchInputRef.current?.blur();
              }
            }}
            aria-label="Search notes"
          />
          {searchQuery && (
            <button
              type="button"
              className="search-clear-btn"
              onClick={handleClearSearch}
              title="Clear search"
              aria-label="Clear search"
            >
              <ClearIcon />
            </button>
          )}
        </div>
      </div>

      {/* Right: Global Actions Area */}
      <div className="topbar-actions">
        <button
          type="button"
          className="topbar-action-btn topbar-btn-new-note"
          onClick={onNewNoteClick}
          title="Create New Note (Ctrl+N)"
          aria-label="Create New Note"
        >
          <PlusIcon />
          <span className="btn-label">New Note</span>
        </button>

        <button
          type="button"
          className="topbar-icon-btn topbar-theme-btn"
          onClick={onThemeToggle}
          title={`Theme: ${theme.charAt(0).toUpperCase() + theme.slice(1)} (Click to switch)`}
          aria-label={`Theme: ${theme}`}
        >
          {renderThemeIcon()}
        </button>

        <button
          type="button"
          className="topbar-icon-btn"
          onClick={onSettingsClick}
          title="Settings"
          aria-label="Settings"
        >
          <SettingsIcon />
        </button>
      </div>
    </div>
  );
};

export default TopBar;
