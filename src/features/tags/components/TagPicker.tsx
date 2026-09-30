import React, { useState, useRef, useEffect, useMemo, useCallback } from "react";
import { Tag } from "../types";
import "./TagPicker.css";

export interface TagPickerProps {
  noteTags: Tag[];
  availableTags: Tag[];
  onAddTag?: (tagId: string) => Promise<void> | void;
  onRemoveTag?: (tagId: string) => Promise<void> | void;
  onCreateTag?: (name: string) => Promise<Tag | null>;
  disabled?: boolean;
}

export const TagPicker: React.FC<TagPickerProps> = ({
  noteTags = [],
  availableTags = [],
  onAddTag,
  onRemoveTag,
  onCreateTag,
  disabled = false,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [isProcessing, setIsProcessing] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Close dropdown on outside click
  useEffect(() => {
    if (!isOpen) return;

    const handlePointerDown = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };

    document.addEventListener("mousedown", handlePointerDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
    };
  }, [isOpen]);

  // Focus search input when popover opens
  useEffect(() => {
    if (isOpen) {
      setSearchQuery("");
      // Small tick to ensure DOM is mounted
      const timer = setTimeout(() => {
        searchInputRef.current?.focus();
      }, 30);
      return () => clearTimeout(timer);
    }
  }, [isOpen]);

  // Set of assigned tag IDs for fast lookup
  const assignedTagIdSet = useMemo(() => {
    return new Set(noteTags.map((t) => t.id));
  }, [noteTags]);

  // Filtered available tags based on search query
  const filteredTags = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return availableTags;
    return availableTags.filter((t) => t.name.toLowerCase().includes(q));
  }, [availableTags, searchQuery]);

  // Check if current search query matches an existing tag name exactly (case-insensitive)
  const exactMatchExists = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return true;
    return availableTags.some((t) => t.name.toLowerCase() === q);
  }, [availableTags, searchQuery]);

  const handleToggleTag = useCallback(
    async (tag: Tag) => {
      if (disabled || isProcessing) return;
      setIsProcessing(true);
      try {
        if (assignedTagIdSet.has(tag.id)) {
          if (onRemoveTag) {
            await onRemoveTag(tag.id);
          }
        } else {
          if (onAddTag) {
            await onAddTag(tag.id);
          }
        }
      } finally {
        setIsProcessing(false);
      }
    },
    [assignedTagIdSet, disabled, isProcessing, onAddTag, onRemoveTag]
  );

  const handleCreateAndAssign = useCallback(async () => {
    const trimmed = searchQuery.trim();
    if (!trimmed || !onCreateTag || disabled || isProcessing) return;
    setIsProcessing(true);
    try {
      const created = await onCreateTag(trimmed);
      if (created && onAddTag) {
        await onAddTag(created.id);
      }
      setSearchQuery("");
    } catch {
      // Errors handled by parent/dialog
    } finally {
      setIsProcessing(false);
    }
  }, [searchQuery, onCreateTag, disabled, isProcessing, onAddTag]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      setIsOpen(false);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (!exactMatchExists && searchQuery.trim() && onCreateTag) {
        handleCreateAndAssign();
      } else if (filteredTags.length > 0) {
        handleToggleTag(filteredTags[0]);
      }
    }
  };

  return (
    <div className="note-tags-container" aria-label="Note tags">
      <span className="note-tags-label">Tags:</span>

      {/* Assigned Tags List */}
      {noteTags.map((tag) => (
        <span key={tag.id} className="note-tag-chip" title={`Tag: #${tag.name}`}>
          <span className="note-tag-chip-hash">#</span>
          <span className="note-tag-chip-name">{tag.name}</span>
          {!disabled && onRemoveTag && (
            <button
              type="button"
              className="note-tag-chip-remove"
              title={`Remove tag #${tag.name}`}
              aria-label={`Remove tag ${tag.name}`}
              onClick={(e) => {
                e.stopPropagation();
                onRemoveTag(tag.id);
              }}
            >
              ×
            </button>
          )}
        </span>
      ))}

      {/* Add Tag Button & Dropdown */}
      {!disabled && (
        <div className="tag-picker-wrap" ref={containerRef}>
          <button
            type="button"
            className={`tag-picker-add-btn ${isOpen ? "is-active" : ""}`}
            onClick={() => setIsOpen((prev) => !prev)}
            title="Add or remove tags for this note"
            aria-label="Add or remove tags"
            aria-expanded={isOpen}
            aria-haspopup="true"
          >
            <span>+</span>
          </button>

          {isOpen && (
            <div className="tag-picker-popover" role="dialog" aria-label="Select tags">
              <div className="tag-picker-popover-header">
                <span className="tag-picker-popover-title">Search or create tag</span>
                <button
                  type="button"
                  className="tag-picker-popover-close"
                  onClick={() => setIsOpen(false)}
                  aria-label="Close tag picker"
                >
                  ✕
                </button>
              </div>

              <input
                ref={searchInputRef}
                type="text"
                className="tag-picker-search-input"
                placeholder="Search or create tag..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                onKeyDown={handleKeyDown}
                maxLength={100}
                aria-label="Search or create tag"
              />

              {filteredTags.length > 0 && (
                <div className="tag-picker-section-label">Existing:</div>
              )}

              <ul className="tag-picker-list" role="listbox">
                {filteredTags.length === 0 && exactMatchExists && (
                  <li className="tag-picker-empty">No tags found</li>
                )}

                {filteredTags.map((tag) => {
                  const isChecked = assignedTagIdSet.has(tag.id);
                  return (
                    <li
                      key={tag.id}
                      role="option"
                      tabIndex={0}
                      aria-selected={isChecked}
                      className={`tag-picker-item ${isChecked ? "is-selected" : ""}`}
                      onClick={() => handleToggleTag(tag)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          handleToggleTag(tag);
                        }
                      }}
                    >
                      <div className={`tag-picker-checkbox ${isChecked ? "is-checked" : ""}`}>
                        {isChecked && (
                          <svg className="tag-picker-check-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3">
                            <polyline points="20 6 9 17 4 12" />
                          </svg>
                        )}
                      </div>
                      <span className="note-tag-chip-hash">#</span>
                      <span className="tag-picker-item-name">{tag.name}</span>
                    </li>
                  );
                })}
              </ul>

              {!exactMatchExists && searchQuery.trim() && onCreateTag && (
                <button
                  type="button"
                  className="tag-picker-create-btn"
                  onClick={handleCreateAndAssign}
                  disabled={isProcessing}
                >
                  <span>+</span>
                  <span>Create &ldquo;{searchQuery.trim()}&rdquo;</span>
                </button>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
