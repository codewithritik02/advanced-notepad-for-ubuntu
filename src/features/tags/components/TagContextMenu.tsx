import React, { useEffect, useRef } from "react";
import type { Tag } from "../types";
import "./TagContextMenu.css";

export interface TagContextMenuProps {
  x: number;
  y: number;
  tag: Tag;
  onClose: () => void;
  onRename: (tag: Tag) => void;
  onDelete: (tag: Tag) => void;
}

const RenameIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
  </svg>
);

const TrashIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="3 6 5 6 21 6" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
  </svg>
);

export const TagContextMenu: React.FC<TagContextMenuProps> = ({
  x,
  y,
  tag,
  onClose,
  onRename,
  onDelete,
}) => {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handlePointerDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };

    window.addEventListener("mousedown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("mousedown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [onClose]);

  const menuWidth = 160;
  const menuHeight = 90;
  const adjustedX = Math.min(x, window.innerWidth - menuWidth - 8);
  const adjustedY = Math.min(y, window.innerHeight - menuHeight - 8);

  return (
    <div
      ref={menuRef}
      className="tag-context-menu"
      style={{
        left: `${Math.max(8, adjustedX)}px`,
        top: `${Math.max(8, adjustedY)}px`,
      }}
      role="menu"
      aria-label={`Options for ${tag.name}`}
    >
      <div className="tag-context-menu-header">
        <span title={tag.name}>#{tag.name}</span>
      </div>

      <button
        type="button"
        className="tag-context-menu-item"
        onClick={() => {
          onClose();
          onRename(tag);
        }}
        role="menuitem"
      >
        <span className="context-menu-icon"><RenameIcon /></span>
        <span className="context-menu-label">Rename Tag</span>
      </button>

      <div className="tag-context-menu-divider" />

      <button
        type="button"
        className="tag-context-menu-item is-danger"
        onClick={() => {
          onClose();
          onDelete(tag);
        }}
        role="menuitem"
      >
        <span className="context-menu-icon"><TrashIcon /></span>
        <span className="context-menu-label">Delete Tag</span>
      </button>
    </div>
  );
};

export default TagContextMenu;
