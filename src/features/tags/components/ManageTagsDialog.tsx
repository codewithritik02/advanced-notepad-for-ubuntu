import React, { useState, useMemo, useRef, useEffect } from "react";
import { Modal, Button } from "../../../components/ui";
import type { Tag } from "../types";
import { validateTagName } from "../utils/tagUtils";
import "./ManageTagsDialog.css";

export interface ManageTagsDialogProps {
  isOpen: boolean;
  tags: Tag[];
  tagCounts?: Record<string, number>;
  onClose: () => void;
  onCreateTag?: (name: string) => Promise<Tag | null | void>;
  onRenameTag?: (id: string, newName: string) => Promise<void>;
  onDeleteTag?: (tag: Tag) => void;
}

export const ManageTagsDialog: React.FC<ManageTagsDialogProps> = ({
  isOpen,
  tags,
  tagCounts,
  onClose,
  onCreateTag,
  onRenameTag,
  onDeleteTag,
}) => {
  const [filterQuery, setFilterQuery] = useState("");
  const [newTagName, setNewTagName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);

  // Inline rename state
  const [editingTagId, setEditingTagId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [editError, setEditError] = useState<string | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);

  const editInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editingTagId && editInputRef.current) {
      editInputRef.current.focus();
      editInputRef.current.select();
    }
  }, [editingTagId]);

  useEffect(() => {
    if (!isOpen) {
      setFilterQuery("");
      setNewTagName("");
      setCreateError(null);
      setEditingTagId(null);
      setEditingName("");
      setEditError(null);
    }
  }, [isOpen]);

  const visibleTags = useMemo(() => {
    if (!filterQuery.trim()) return tags;
    const q = filterQuery.toLowerCase().trim();
    return tags.filter((t) => t.name.toLowerCase().includes(q));
  }, [tags, filterQuery]);

  const handleCreateSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!onCreateTag || isCreating) return;

    const validation = validateTagName(newTagName);
    if (!validation.valid) {
      setCreateError(validation.error || "Tag name cannot be empty.");
      return;
    }

    setIsCreating(true);
    setCreateError(null);
    try {
      await onCreateTag(validation.trimmed);
      setNewTagName("");
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsCreating(false);
    }
  };

  const handleStartRename = (tag: Tag) => {
    setEditingTagId(tag.id);
    setEditingName(tag.name);
    setEditError(null);
  };

  const handleCancelRename = () => {
    setEditingTagId(null);
    setEditingName("");
    setEditError(null);
  };

  const handleSaveRename = async (tag: Tag) => {
    if (!onRenameTag || isRenaming) return;

    const validation = validateTagName(editingName);
    if (!validation.valid) {
      setEditError(validation.error || "Tag name cannot be empty.");
      editInputRef.current?.focus();
      return;
    }

    if (validation.trimmed === tag.name) {
      handleCancelRename();
      return;
    }

    setIsRenaming(true);
    setEditError(null);
    try {
      await onRenameTag(tag.id, validation.trimmed);
      handleCancelRename();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.toLowerCase().includes("already exists") || msg.toLowerCase().includes("conflict")) {
        setEditError("A tag with this name already exists.");
      } else {
        setEditError(msg || "Failed to rename tag.");
      }
    } finally {
      setIsRenaming(false);
    }
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Manage Tags"
      maxWidth="480px"
      footer={
        <Button variant="secondary" onClick={onClose}>
          Done
        </Button>
      }
    >
      <div className="manage-tags-container">
        {/* Top bar: Create New Tag */}
        <div className="manage-tags-top-bar">
          {onCreateTag && (
            <form onSubmit={handleCreateSubmit} className="manage-tags-create-form">
              <input
                type="text"
                className="manage-tags-create-input"
                placeholder="Create new tag..."
                value={newTagName}
                onChange={(e) => {
                  setNewTagName(e.target.value);
                  if (createError) setCreateError(null);
                }}
                disabled={isCreating}
                aria-label="New tag name"
              />
              <Button
                type="submit"
                variant="primary"
                size="sm"
                disabled={!newTagName.trim() || isCreating}
              >
                {isCreating ? "Adding..." : "+ Add"}
              </Button>
            </form>
          )}

          {createError && <p className="manage-tags-error">{createError}</p>}

          {/* Quick Filter if list is long */}
          {tags.length > 5 && (
            <input
              type="text"
              className="manage-tags-filter-input"
              placeholder="Filter tags..."
              value={filterQuery}
              onChange={(e) => setFilterQuery(e.target.value)}
              aria-label="Filter tags in manager"
            />
          )}
        </div>

        {/* Tag List */}
        <div className="manage-tags-list-wrap">
          {tags.length === 0 ? (
            <div className="manage-tags-empty">
              <span>No tags created yet.</span>
            </div>
          ) : visibleTags.length === 0 ? (
            <div className="manage-tags-empty">
              <span>No tags match &quot;{filterQuery}&quot;.</span>
            </div>
          ) : (
            <ul className="manage-tags-list" role="list">
              {visibleTags.map((tag) => {
                const isEditing = editingTagId === tag.id;
                const count = tagCounts ? tagCounts[tag.id] ?? 0 : 0;

                return (
                  <li key={tag.id} className="manage-tags-row">
                    {isEditing ? (
                      <div className="manage-tags-inline-edit">
                        <input
                          ref={editInputRef}
                          type="text"
                          className="manage-tags-edit-input"
                          value={editingName}
                          onChange={(e) => setEditingName(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") {
                              e.preventDefault();
                              handleSaveRename(tag);
                            } else if (e.key === "Escape") {
                              e.preventDefault();
                              handleCancelRename();
                            }
                          }}
                          disabled={isRenaming}
                          aria-label={`Edit name for tag ${tag.name}`}
                        />
                        <button
                          type="button"
                          className="manage-tags-action-btn"
                          onClick={() => handleSaveRename(tag)}
                          disabled={isRenaming}
                        >
                          Save
                        </button>
                        <button
                          type="button"
                          className="manage-tags-action-btn"
                          onClick={handleCancelRename}
                          disabled={isRenaming}
                        >
                          Cancel
                        </button>
                      </div>
                    ) : (
                      <>
                        <div className="manage-tags-info">
                          <span className="manage-tags-name" title={tag.name}>
                            #{tag.name}
                          </span>
                          <span className="manage-tags-count">
                            {count} {count === 1 ? "note" : "notes"}
                          </span>
                        </div>

                        <div className="manage-tags-actions">
                          {onRenameTag && (
                            <button
                              type="button"
                              className="manage-tags-action-btn"
                              onClick={() => handleStartRename(tag)}
                              aria-label={`Rename tag ${tag.name}`}
                            >
                              Rename
                            </button>
                          )}
                          {onDeleteTag && (
                            <button
                              type="button"
                              className="manage-tags-action-btn is-delete"
                              onClick={() => onDeleteTag(tag)}
                              aria-label={`Delete tag ${tag.name}`}
                            >
                              Delete
                            </button>
                          )}
                        </div>
                      </>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        {editError && <p className="manage-tags-error">{editError}</p>}
      </div>
    </Modal>
  );
};

export default ManageTagsDialog;
