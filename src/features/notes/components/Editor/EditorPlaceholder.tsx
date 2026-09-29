import React from "react";
import { NoteListItem } from "../../types";
import "./EditorPlaceholder.css";

export interface EditorPlaceholderProps {
  selectedNote?: NoteListItem | null;
}

// Inline lightweight SVG icons for editor toolbar placeholders
const BoldIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z" />
    <path d="M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z" />
  </svg>
);

const ItalicIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <line x1="19" y1="4" x2="10" y2="4" />
    <line x1="14" y1="20" x2="5" y2="20" />
    <line x1="15" y1="4" x2="9" y2="20" />
  </svg>
);

const HeadingIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M6 12h12M4 4v16M20 4v16" />
  </svg>
);

const ListIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <line x1="8" y1="6" x2="21" y2="6" />
    <line x1="8" y1="12" x2="21" y2="12" />
    <line x1="8" y1="18" x2="21" y2="18" />
    <line x1="3" y1="6" x2="3.01" y2="6" />
    <line x1="3" y1="12" x2="3.01" y2="12" />
    <line x1="3" y1="18" x2="3.01" y2="18" />
  </svg>
);

const CodeIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="16 18 22 12 16 6" />
    <polyline points="8 6 2 12 8 18" />
  </svg>
);

const LinkIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
    <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
  </svg>
);

export const EditorPlaceholder: React.FC<EditorPlaceholderProps> = ({
  selectedNote,
}) => {
  if (!selectedNote) {
    return (
      <div className="editor-empty-state">
        <div className="editor-empty-icon">
          <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
            <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
          </svg>
        </div>
        <h3 className="editor-empty-title">No Note Selected</h3>
        <p className="editor-empty-subtitle">
          Select a note from the list or create a new note to start writing.
        </p>
      </div>
    );
  }

  return (
    <div className="editor-container">
      {/* Editor Toolbar Placeholder */}
      <div className="editor-toolbar" role="toolbar" aria-label="Formatting toolbar placeholder">
        <div className="toolbar-group">
          <button type="button" className="toolbar-btn" title="Bold (Ctrl+B)" disabled>
            <BoldIcon />
          </button>
          <button type="button" className="toolbar-btn" title="Italic (Ctrl+I)" disabled>
            <ItalicIcon />
          </button>
          <button type="button" className="toolbar-btn" title="Heading" disabled>
            <HeadingIcon />
          </button>
        </div>

        <div className="toolbar-divider" />

        <div className="toolbar-group">
          <button type="button" className="toolbar-btn" title="Bullet List" disabled>
            <ListIcon />
          </button>
          <button type="button" className="toolbar-btn" title="Code Block" disabled>
            <CodeIcon />
          </button>
          <button type="button" className="toolbar-btn" title="Link" disabled>
            <LinkIcon />
          </button>
        </div>

        <div className="toolbar-spacer" />

        <div className="toolbar-info-badge">
          <span>Editor Preview</span>
        </div>
      </div>

      {/* Editor Main Content Area */}
      <div className="editor-scroll-area">
        <div className="editor-document">
          {/* Note Title Input Placeholder */}
          <div className="editor-title-wrap">
            <input
              type="text"
              className="editor-title-input selectable-text"
              placeholder="Note title..."
              value={selectedNote.title}
              readOnly
              aria-label="Note title"
            />
          </div>

          {/* Note Metadata Strip */}
          <div className="editor-meta-strip">
            <span className="meta-item">
              <span className="meta-label">Updated:</span> {selectedNote.updatedAt}
            </span>
            {selectedNote.tags && selectedNote.tags.length > 0 && (
              <span className="meta-item">
                <span className="meta-label">Tags:</span>{" "}
                {selectedNote.tags.map((t) => `#${t}`).join(", ")}
              </span>
            )}
          </div>

          <div className="editor-body-divider" />

          {/* Note Body Content Placeholder */}
          <div className="editor-body-wrap">
            <textarea
              className="editor-content-area selectable-text"
              placeholder="Start writing your note..."
              value={selectedNote.preview}
              readOnly
              aria-label="Note content preview"
            />
          </div>
        </div>
      </div>

      {/* Note Status Bar */}
      <footer className="editor-status-bar">
        <div className="status-item">
          <span>Words: {selectedNote.preview.split(/\s+/).filter(Boolean).length}</span>
          <span className="status-separator">•</span>
          <span>Characters: {selectedNote.preview.length}</span>
        </div>

        <div className="status-item">
          <span className="status-indicator-dot" />
          <span>Ready (Offline)</span>
        </div>
      </footer>
    </div>
  );
};

export default EditorPlaceholder;
