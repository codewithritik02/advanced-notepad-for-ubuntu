import React, { useState } from "react";
import type { Tag } from "../types";
import { TagItem } from "./TagItem";
import { TagContextMenu } from "./TagContextMenu";
import "./TagList.css";

export interface TagListProps {
  tags: Tag[];
  selectedTagId?: string | null;
  onSelectTag?: (tagId: string) => void;
  onCreateTag?: () => void;
  onRenameTag?: (tag: Tag) => void;
  onDeleteTag?: (tag: Tag) => void;
  tagCounts?: Record<string, number>;
}

const PlusIcon = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <line x1="12" y1="5" x2="12" y2="19" />
    <line x1="5" y1="12" x2="19" y2="12" />
  </svg>
);

export const TagList: React.FC<TagListProps> = ({
  tags,
  selectedTagId,
  onSelectTag = () => {},
  onCreateTag,
  onRenameTag,
  onDeleteTag,
  tagCounts,
}) => {
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    tag: Tag;
  } | null>(null);

  const [filterQuery, setFilterQuery] = useState("");

  const visibleTags = React.useMemo(() => {
    if (!filterQuery.trim()) return tags;
    const q = filterQuery.toLowerCase().trim();
    return tags.filter((t) => t.name.toLowerCase().includes(q));
  }, [tags, filterQuery]);

  const handleContextMenu = (e: React.MouseEvent, tag: Tag) => {
    e.preventDefault();
    if (!onRenameTag && !onDeleteTag) return;
    setContextMenu({
      x: e.clientX,
      y: e.clientY,
      tag,
    });
  };

  const handleCloseContextMenu = () => {
    setContextMenu(null);
  };

  return (
    <div className="sidebar-tags-container">
      {tags.length > 8 && (
        <div className="sidebar-tags-filter-wrap">
          <input
            type="text"
            className="sidebar-tags-filter-input"
            placeholder="Filter tags..."
            value={filterQuery}
            onChange={(e) => setFilterQuery(e.target.value)}
            aria-label="Filter tags"
          />
          {filterQuery && (
            <button
              type="button"
              className="sidebar-tags-filter-clear"
              onClick={() => setFilterQuery("")}
              aria-label="Clear tag filter"
            >
              ×
            </button>
          )}
        </div>
      )}

      {tags.length === 0 ? (
        <div className="sidebar-tags-empty">
          <span>No tags yet.</span>
        </div>
      ) : visibleTags.length === 0 ? (
        <div className="sidebar-tags-empty">
          <span>No matching tags.</span>
        </div>
      ) : (
        <div className="sidebar-tags-scroll">
          <ul className="sidebar-tags-list" role="menu">
            {visibleTags.map((tag) => (
              <li key={tag.id}>
                <TagItem
                  tag={tag}
                  isSelected={selectedTagId === tag.id}
                  count={tagCounts ? tagCounts[tag.id] : undefined}
                  onSelect={onSelectTag}
                  onContextMenu={handleContextMenu}
                />
              </li>
            ))}
          </ul>
        </div>
      )}

      {onCreateTag && (
        <button
          type="button"
          className="sidebar-new-tag-btn"
          onClick={onCreateTag}
          aria-label="Create new tag"
        >
          <PlusIcon />
          <span>New Tag</span>
        </button>
      )}

      {contextMenu && onRenameTag && onDeleteTag && (
        <TagContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          tag={contextMenu.tag}
          onClose={handleCloseContextMenu}
          onRename={onRenameTag}
          onDelete={onDeleteTag}
        />
      )}
    </div>
  );
};

export default TagList;
