import React from "react";
import "./Sidebar.css";

export type NavItemId =
  | "all-notes"
  | "favorites"
  | "notebooks"
  | "tags"
  | "trash";

export interface SidebarProps {
  activeNavId?: NavItemId;
  onSelectNav?: (navId: NavItemId) => void;
  onNewNoteClick?: () => void;
}

// Inline lightweight SVG icons for zero external dependencies
const PlusIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <line x1="12" y1="5" x2="12" y2="19" />
    <line x1="5" y1="12" x2="19" y2="12" />
  </svg>
);

const AllNotesIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
    <line x1="16" y1="13" x2="8" y2="13" />
    <line x1="16" y1="17" x2="8" y2="17" />
    <polyline points="10 9 9 9 8 9" />
  </svg>
);

const StarIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
  </svg>
);

const FolderIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);

const TagIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z" />
    <line x1="7" y1="7" x2="7.01" y2="7" />
  </svg>
);

const TrashIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="3 6 5 6 21 6" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
  </svg>
);

export const Sidebar: React.FC<SidebarProps> = ({
  activeNavId = "all-notes",
  onSelectNav,
  onNewNoteClick,
}) => {
  const handleNavClick = (navId: NavItemId) => {
    onSelectNav?.(navId);
  };

  return (
    <div className="sidebar-container">
      {/* Brand Header */}
      <div className="sidebar-header">
        <div className="sidebar-brand">
          <div className="sidebar-brand-icon">
            <AllNotesIcon />
          </div>
          <span className="sidebar-brand-name">Personal Notepad</span>
        </div>
      </div>

      {/* Action Area */}
      <div className="sidebar-actions">
        <button
          type="button"
          className="btn-new-note"
          onClick={onNewNoteClick}
          aria-label="Create new note"
        >
          <PlusIcon />
          <span>New Note</span>
        </button>
      </div>

      {/* Navigation Scrollable Area */}
      <div className="sidebar-nav-scroll">
        {/* Section: Notes */}
        <div className="sidebar-section">
          <div className="sidebar-section-title">Notes</div>
          <ul className="sidebar-nav-list" role="menu">
            <li>
              <button
                type="button"
                className={`sidebar-nav-item ${activeNavId === "all-notes" ? "is-active" : ""}`}
                onClick={() => handleNavClick("all-notes")}
                role="menuitem"
                aria-current={activeNavId === "all-notes" ? "page" : undefined}
              >
                <span className="nav-item-icon"><AllNotesIcon /></span>
                <span className="nav-item-label">All Notes</span>
              </button>
            </li>
            <li>
              <button
                type="button"
                className={`sidebar-nav-item ${activeNavId === "favorites" ? "is-active" : ""}`}
                onClick={() => handleNavClick("favorites")}
                role="menuitem"
                aria-current={activeNavId === "favorites" ? "page" : undefined}
              >
                <span className="nav-item-icon"><StarIcon /></span>
                <span className="nav-item-label">Favorites</span>
              </button>
            </li>
          </ul>
        </div>

        {/* Section: Organization */}
        <div className="sidebar-section">
          <div className="sidebar-section-title">Organization</div>
          <ul className="sidebar-nav-list" role="menu">
            <li>
              <button
                type="button"
                className={`sidebar-nav-item ${activeNavId === "notebooks" ? "is-active" : ""}`}
                onClick={() => handleNavClick("notebooks")}
                role="menuitem"
                aria-current={activeNavId === "notebooks" ? "page" : undefined}
              >
                <span className="nav-item-icon"><FolderIcon /></span>
                <span className="nav-item-label">Notebooks</span>
              </button>
            </li>
            <li>
              <button
                type="button"
                className={`sidebar-nav-item ${activeNavId === "tags" ? "is-active" : ""}`}
                onClick={() => handleNavClick("tags")}
                role="menuitem"
                aria-current={activeNavId === "tags" ? "page" : undefined}
              >
                <span className="nav-item-icon"><TagIcon /></span>
                <span className="nav-item-label">Tags</span>
              </button>
            </li>
          </ul>
        </div>

        {/* Section: System */}
        <div className="sidebar-section">
          <div className="sidebar-section-title">System</div>
          <ul className="sidebar-nav-list" role="menu">
            <li>
              <button
                type="button"
                className={`sidebar-nav-item ${activeNavId === "trash" ? "is-active" : ""}`}
                onClick={() => handleNavClick("trash")}
                role="menuitem"
                aria-current={activeNavId === "trash" ? "page" : undefined}
              >
                <span className="nav-item-icon"><TrashIcon /></span>
                <span className="nav-item-label">Trash</span>
              </button>
            </li>
          </ul>
        </div>
      </div>
    </div>
  );
};

export default Sidebar;
