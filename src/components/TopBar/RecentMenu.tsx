import React, { useEffect, useRef, useState } from "react";
import {
  describeFilePath,
  formatRelativeTime,
  type RecentNote,
} from "../../utils/recentNotes";
import "./RecentMenu.css";

export interface RecentMenuProps {
  items: RecentNote[];
  onOpen: (noteId: string) => void;
  onRemove?: (noteId: string) => void;
  onClear?: () => void;
}

const ClockIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <circle cx="12" cy="12" r="9" />
    <polyline points="12 7 12 12 15.5 14" />
  </svg>
);

const FileIcon = () => (
  <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
  </svg>
);

/** Dropdown in the top bar listing the notes and files opened most recently. */
export const RecentMenu: React.FC<RecentMenuProps> = ({ items, onOpen, onRemove, onClear }) => {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!open) return;
    setNow(Date.now());
    const onMouseDown = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        setOpen(false);
        buttonRef.current?.focus();
      }
    };
    document.addEventListener("mousedown", onMouseDown);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [open]);

  const moveFocus = (direction: 1 | -1) => {
    const entries = Array.from(
      rootRef.current?.querySelectorAll<HTMLElement>(".recent-menu-item") ?? [],
    );
    if (entries.length === 0) return;
    const index = entries.indexOf(document.activeElement as HTMLElement);
    const next = index === -1 ? (direction === 1 ? 0 : entries.length - 1) : (index + direction + entries.length) % entries.length;
    entries[next].focus();
  };

  return (
    <div className="recent-menu" ref={rootRef}>
      <button
        ref={buttonRef}
        type="button"
        className={`topbar-icon-btn recent-menu-toggle ${open ? "is-open" : ""}`}
        onClick={() => setOpen((value) => !value)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown") {
            event.preventDefault();
            setOpen(true);
            window.setTimeout(() => moveFocus(1), 0);
          }
        }}
        title="Recently opened notes and files"
        aria-label="Recently opened"
        aria-haspopup="menu"
        aria-expanded={open}
      >
        <ClockIcon />
      </button>

      {open && (
        <div
          className="recent-menu-panel"
          role="menu"
          aria-label="Recently opened"
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              moveFocus(1);
            } else if (event.key === "ArrowUp") {
              event.preventDefault();
              moveFocus(-1);
            }
          }}
        >
          <div className="recent-menu-header">
            <span>Recently opened</span>
            {items.length > 0 && onClear && (
              <button type="button" className="recent-menu-clear" onClick={onClear}>
                Clear
              </button>
            )}
          </div>

          {items.length === 0 ? (
            <div className="recent-menu-empty">
              Notes and files you open will appear here.
            </div>
          ) : (
            <ul className="recent-menu-list">
              {items.map((item) => {
                const file = item.file ? describeFilePath(item.file) : null;
                return (
                  <li key={item.id} className="recent-menu-row">
                    <button
                      type="button"
                      role="menuitem"
                      className="recent-menu-item"
                      onClick={() => {
                        setOpen(false);
                        onOpen(item.id);
                      }}
                      title={item.file ?? item.title}
                    >
                      <span className="recent-menu-title">{item.title}</span>
                      <span className="recent-menu-meta">
                        {file && (
                          <span className="recent-menu-file">
                            <FileIcon />
                            {file.folder ? `${file.folder}/` : ""}
                            {file.name}
                          </span>
                        )}
                        <span>{formatRelativeTime(item.openedAt, now)}</span>
                      </span>
                    </button>
                    {onRemove && (
                      <button
                        type="button"
                        className="recent-menu-remove"
                        aria-label={`Remove ${item.title} from recent`}
                        title="Remove from list"
                        onClick={() => onRemove(item.id)}
                      >
                        ×
                      </button>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </div>
  );
};

export default RecentMenu;
