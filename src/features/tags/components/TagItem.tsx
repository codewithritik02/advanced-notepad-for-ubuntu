import React from "react";
import type { Tag } from "../types";
import "./TagItem.css";

export interface TagItemProps {
  tag: Tag;
  isSelected: boolean;
  count?: number;
  onSelect: (tagId: string) => void;
  onContextMenu: (e: React.MouseEvent, tag: Tag) => void;
}

const MoreIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
    <circle cx="12" cy="5" r="2" />
    <circle cx="12" cy="12" r="2" />
    <circle cx="12" cy="19" r="2" />
  </svg>
);

export const TagItem: React.FC<TagItemProps> = ({
  tag,
  isSelected,
  count,
  onSelect,
  onContextMenu,
}) => {
  const itemRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    if (isSelected && itemRef.current) {
      itemRef.current.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  }, [isSelected]);

  return (
    <div ref={itemRef} className="tag-item-container">
      <button
        type="button"
        className={`tag-item-btn ${isSelected ? "is-active" : ""}`}
        onClick={() => onSelect(tag.id)}
        onContextMenu={(e) => onContextMenu(e, tag)}
        role="menuitem"
        aria-current={isSelected ? "page" : undefined}
        title={tag.name}
      >
        <span className="tag-item-icon">#</span>
        <span className="tag-item-name">{tag.name}</span>
        {count !== undefined && count > 0 && (
          <span className="tag-item-badge">{count}</span>
        )}
      </button>

      <button
        type="button"
        className="tag-item-more-btn"
        onClick={(e) => {
          e.stopPropagation();
          onContextMenu(e, tag);
        }}
        aria-label={`Options for tag ${tag.name}`}
        title="Tag options"
      >
        <MoreIcon />
      </button>
    </div>
  );
};

export default TagItem;
