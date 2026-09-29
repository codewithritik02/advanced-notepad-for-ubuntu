import React, { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { Note, NoteFormat, EditorStatus } from "../../types";
import { storageService } from "../../../../services/storage";
import "./EditorPlaceholder.css";

export interface NoteEditorProps {
  noteId: string | null;
  onSaveNote?: (id: string, title: string, content: string, format?: NoteFormat) => Promise<void> | void;
  onNoteUpdated?: (updatedNote: Note) => void;
  onRegisterSave?: (saveFn: (() => Promise<void>) | null) => void;
}

// Inline lightweight SVG icons for editor toolbar
const SaveIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" />
    <polyline points="17 21 17 13 7 13 7 21" />
    <polyline points="7 3 7 8 15 8" />
  </svg>
);

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

const UndoIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M3 7v6h6" />
    <path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13" />
  </svg>
);

const RedoIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 7v6h-6" />
    <path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3L21 13" />
  </svg>
);

interface NoteSessionViewState {
  cursorStart: number;
  cursorEnd: number;
  scrollTop: number;
}

// Session-level memory for preserving cursor position and scroll position per note during the application session
const sessionViewCache = new Map<string, NoteSessionViewState>();

/**
 * Fast zero-allocation word counter for responsive performance with large notes (up to 1MB+).
 */
function countWords(text: string): number {
  let count = 0;
  let inWord = false;
  const len = text.length;
  for (let i = 0; i < len; i++) {
    const code = text.charCodeAt(i);
    if (code <= 32 || code === 160) {
      inWord = false;
    } else if (!inWord) {
      inWord = true;
      count++;
    }
  }
  return count;
}

export const NoteEditor: React.FC<NoteEditorProps> = ({
  noteId,
  onSaveNote,
  onNoteUpdated,
  onRegisterSave,
}) => {
  const [loadedNote, setLoadedNote] = useState<Note | null>(null);
  const [editorStatus, setEditorStatus] = useState<EditorStatus>("idle");
  const [loadErrorState, setLoadErrorState] = useState<{
    title: string;
    message: string;
    isNotFound: boolean;
  } | null>(null);
  const [lastSavedAt, setLastSavedAt] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  // In-memory editing state
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [format, setFormat] = useState<NoteFormat>("txt");
  const [isDirty, setIsDirty] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  // Memoized metrics avoiding full-string allocations on large notes
  const wordCount = useMemo(() => countWords(content), [content]);

  // Track active fetch request cancellation token
  const activeFetchIdRef = useRef<string | null>(null);
  const titleInputRef = useRef<HTMLInputElement>(null);
  const contentRef = useRef<HTMLTextAreaElement>(null);
  const scrollAreaRef = useRef<HTMLDivElement>(null);

  // Focus title input when opening an empty/newly created note
  useEffect(() => {
    if (
      loadedNote &&
      (loadedNote.title === "Untitled Note" || !loadedNote.title) &&
      !loadedNote.content
    ) {
      requestAnimationFrame(() => {
        titleInputRef.current?.focus();
        titleInputRef.current?.select();
      });
    }
  }, [loadedNote?.id]);

  // Load active note from SQLite whenever noteId changes
  const fetchNote = useCallback(async (id: string) => {
    activeFetchIdRef.current = id;
    setEditorStatus("loading");
    setLoadErrorState(null);
    setSaveError(null);

    try {
      const note = await storageService.notes.get(id);

      // Check if another note selection superseded this request
      if (activeFetchIdRef.current !== id) return;

      if (!note) {
        setLoadedNote(null);
        setEditorStatus("error");
        setLoadErrorState({
          title: "Note Not Found",
          message: "The selected note is no longer available.",
          isNotFound: true,
        });
        return;
      }

      setLoadedNote(note);
      setTitle(note.title);
      setContent(note.content);
      const resolvedFormat: NoteFormat = (note.format as NoteFormat) === "md" ? "md" : "txt";
      setFormat(resolvedFormat);
      setIsDirty(false);
      setEditorStatus("saved");
      setLastSavedAt(note.modified_at);

      // Restore session cursor and scroll position if returning to a note during this session
      requestAnimationFrame(() => {
        const savedView = sessionViewCache.get(id);
        if (savedView) {
          if (scrollAreaRef.current) {
            scrollAreaRef.current.scrollTop = savedView.scrollTop;
          }
          if (contentRef.current) {
            const maxLen = note.content.length;
            const start = Math.min(Math.max(savedView.cursorStart, 0), maxLen);
            const end = Math.min(Math.max(savedView.cursorEnd, 0), maxLen);
            contentRef.current.selectionStart = start;
            contentRef.current.selectionEnd = end;
          }
        } else {
          // Fresh note open: reset scroll to top and cursor to start
          if (scrollAreaRef.current) {
            scrollAreaRef.current.scrollTop = 0;
          }
          if (contentRef.current && note.content.length > 0) {
            contentRef.current.selectionStart = 0;
            contentRef.current.selectionEnd = 0;
          }
        }
      });
    } catch {
      if (activeFetchIdRef.current !== id) return;
      setLoadedNote(null);
      setEditorStatus("error");
      setLoadErrorState({
        title: "Unable to Load Note",
        message: "A storage error occurred while loading this note. Please try again.",
        isNotFound: false,
      });
    }
  }, []);

  useEffect(() => {
    if (!noteId) {
      setLoadedNote(null);
      setEditorStatus("idle");
      setLoadErrorState(null);
      setSaveError(null);
      setTitle("");
      setContent("");
      setIsDirty(false);
      return;
    }

    fetchNote(noteId);
  }, [noteId, fetchNote]);

  // Autosave engine refs
  const revisionRef = useRef(0);
  const latestPersistedRevisionRef = useRef(0);
  const debounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isSavingRef = useRef(false);
  const latestDataRef = useRef({ title: "", content: "", format: "txt" as NoteFormat });
  const prevNoteIdRef = useRef<string | null>(null);
  const isDirtyRef = useRef(false);

  useEffect(() => {
    isDirtyRef.current = isDirty;
  }, [isDirty]);

  // Keep latestDataRef current with latest user edits
  useEffect(() => {
    latestDataRef.current = { title, content, format };
  }, [title, content, format]);

  // Save on note switch: Preserve previous note view state and flush dirty changes before loading new note
  useEffect(() => {
    const previousId = prevNoteIdRef.current;
    if (previousId && previousId !== noteId) {
      // Preserve cursor and scroll position for previous note in session memory
      if (contentRef.current || scrollAreaRef.current) {
        sessionViewCache.set(previousId, {
          cursorStart: contentRef.current?.selectionStart ?? 0,
          cursorEnd: contentRef.current?.selectionEnd ?? 0,
          scrollTop: scrollAreaRef.current?.scrollTop ?? 0,
        });
      }

      if (isDirtyRef.current) {
        if (debounceTimerRef.current) {
          clearTimeout(debounceTimerRef.current);
          debounceTimerRef.current = null;
        }

        const dataToFlush = latestDataRef.current;
        const titleToFlush = dataToFlush.title.trim() ? dataToFlush.title : "Untitled Note";

        // Flush save to SQLite immediately
        storageService.notes
          .update(previousId, {
            title: titleToFlush,
            content: dataToFlush.content,
            format: dataToFlush.format,
          })
          .then((updated) => {
            onNoteUpdated?.(updated);
          })
          .catch(() => {
            // Graceful catch
          });
      }
    }

    prevNoteIdRef.current = noteId;
  }, [noteId, onNoteUpdated]);

  // Save before application close where supported
  useEffect(() => {
    const handleBeforeUnload = () => {
      if (noteId && isDirtyRef.current) {
        const data = latestDataRef.current;
        const titleToSave = data.title.trim() ? data.title : "Untitled Note";
        storageService.notes
          .update(noteId, {
            title: titleToSave,
            content: data.content,
            format: data.format,
          })
          .catch(() => {});
      }
    };

    window.addEventListener("beforeunload", handleBeforeUnload);

    // Tauri-specific close listener
    let unlistenTauriClose: (() => void) | undefined;
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => {
        getCurrentWindow()
          .onCloseRequested(async () => {
            if (noteId && isDirtyRef.current) {
              const data = latestDataRef.current;
              const titleToSave = data.title.trim() ? data.title : "Untitled Note";
              try {
                await storageService.notes.update(noteId, {
                  title: titleToSave,
                  content: data.content,
                  format: data.format,
                });
              } catch {
                // Ignore close-time persistence failure
              }
            }
          })
          .then((unlisten) => {
            unlistenTauriClose = unlisten;
          })
          .catch(() => {});
      })
      .catch(() => {});

    return () => {
      window.removeEventListener("beforeunload", handleBeforeUnload);
      unlistenTauriClose?.();
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }
    };
  }, [noteId]);

  // Auto-grow textarea to accommodate content without secondary scrollbars
  useEffect(() => {
    if (contentRef.current) {
      contentRef.current.style.height = "auto";
      contentRef.current.style.height = `${Math.max(contentRef.current.scrollHeight, 380)}px`;
    }
  }, [content]);

  // Core persistence worker function
  const executeSave = useCallback(
    async (targetNoteId: string, rev: number) => {
      if (isSavingRef.current) return;
      isSavingRef.current = true;
      setIsSaving(true);
      setEditorStatus("saving");
      setSaveError(null);

      const dataToSave = latestDataRef.current;
      const persistedTitle = dataToSave.title.trim() ? dataToSave.title : "Untitled Note";

      try {
        let updated: Note;
        if (onSaveNote) {
          await onSaveNote(targetNoteId, persistedTitle, dataToSave.content, dataToSave.format);
          const refreshed = await storageService.notes.get(targetNoteId);
          updated = refreshed || {
            ...(loadedNote as Note),
            id: targetNoteId,
            title: persistedTitle,
            content: dataToSave.content,
            format: dataToSave.format,
            modified_at: new Date().toISOString(),
          };
        } else {
          updated = await storageService.notes.update(targetNoteId, {
            title: persistedTitle,
            content: dataToSave.content,
            format: dataToSave.format,
          });
        }

        // Only commit revision if this response is not superseded by a newer save
        if (rev >= latestPersistedRevisionRef.current) {
          latestPersistedRevisionRef.current = rev;
          setLoadedNote(updated);
          onNoteUpdated?.(updated);
          setLastSavedAt(updated.modified_at);

          // If no new typing occurred since this save was initiated, mark as clean
          if (revisionRef.current === rev) {
            setIsDirty(false);
            setEditorStatus("saved");
          }
        }
      } catch {
        setIsDirty(true);
        setEditorStatus("error");
        setSaveError(
          "Unable to save changes. Your changes are still available locally in the editor."
        );
      } finally {
        isSavingRef.current = false;
        setIsSaving(false);

        // If the user continued typing while save was in-flight, schedule an immediate follow-up
        if (revisionRef.current > latestPersistedRevisionRef.current) {
          if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
          debounceTimerRef.current = setTimeout(() => {
            executeSave(targetNoteId, revisionRef.current);
          }, 800);
        }
      }
    },
    [onSaveNote, onNoteUpdated, loadedNote]
  );

  // Trigger autosave with 800ms debounce
  const scheduleAutosave = useCallback(
    (targetNoteId: string) => {
      revisionRef.current += 1;
      const currentRev = revisionRef.current;
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
      debounceTimerRef.current = setTimeout(() => {
        executeSave(targetNoteId, currentRev);
      }, 800);
    },
    [executeSave]
  );

  const updateDirtyState = (
    newTitle: string,
    newContent: string,
    newFormat: NoteFormat
  ) => {
    if (!loadedNote) return;
    const hasChanged =
      newTitle !== loadedNote.title ||
      newContent !== loadedNote.content ||
      newFormat !== ((loadedNote.format as NoteFormat) === "md" ? "md" : "txt");
    setIsDirty(hasChanged);
    if (hasChanged) {
      setEditorStatus("unsaved");
    } else {
      setEditorStatus("saved");
    }
  };

  const handleTitleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const newTitle = e.target.value;
    setTitle(newTitle);
    updateDirtyState(newTitle, content, format);
    if (noteId) scheduleAutosave(noteId);
  };

  const handleContentChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newContent = e.target.value;
    setContent(newContent);
    updateDirtyState(title, newContent, format);
    if (noteId) scheduleAutosave(noteId);
  };

  const handleRecordSelection = useCallback(() => {
    if (noteId && contentRef.current) {
      const existing = sessionViewCache.get(noteId) || { cursorStart: 0, cursorEnd: 0, scrollTop: 0 };
      sessionViewCache.set(noteId, {
        ...existing,
        cursorStart: contentRef.current.selectionStart,
        cursorEnd: contentRef.current.selectionEnd,
      });
    }
  }, [noteId]);

  const handleScroll = useCallback(() => {
    if (noteId && scrollAreaRef.current) {
      const existing = sessionViewCache.get(noteId) || { cursorStart: 0, cursorEnd: 0, scrollTop: 0 };
      sessionViewCache.set(noteId, {
        ...existing,
        scrollTop: scrollAreaRef.current.scrollTop,
      });
    }
  }, [noteId]);

  const handleContentKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Explicit Save shortcut (Ctrl/Cmd + S)
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      handleSave();
      return;
    }

    // Tab key inserts 2 spaces instead of moving focus away from the editor
    if (e.key === "Tab") {
      e.preventDefault();
      const textarea = e.currentTarget;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      const newContent = content.substring(0, start) + "  " + content.substring(end);
      setContent(newContent);
      updateDirtyState(title, newContent, format);
      if (noteId) scheduleAutosave(noteId);
      requestAnimationFrame(() => {
        textarea.selectionStart = textarea.selectionEnd = start + 2;
        handleRecordSelection();
      });
    }
  };

  const handleTitleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    // Explicit Save shortcut (Ctrl/Cmd + S)
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      handleSave();
      return;
    }

    if (e.key === "Enter") {
      e.preventDefault();
      contentRef.current?.focus();
    }
  };

  const handleBodyWrapClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget && contentRef.current) {
      contentRef.current.focus();
    }
  };

  const handleToggleFormat = () => {
    const nextFormat: NoteFormat = format === "txt" ? "md" : "txt";
    setFormat(nextFormat);
    updateDirtyState(title, content, nextFormat);
    if (noteId) scheduleAutosave(noteId);
  };

  const handleSave = useCallback(async () => {
    if (!noteId || isSavingRef.current) return;
    if (!isDirtyRef.current && editorStatus !== "error") {
      // Per spec Section 42: If there are no changes, do nothing or provide unobtrusive confirmation
      return;
    }
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
    revisionRef.current += 1;
    await executeSave(noteId, revisionRef.current);
  }, [noteId, editorStatus, executeSave]);

  useEffect(() => {
    if (onRegisterSave) {
      onRegisterSave(handleSave);
      return () => {
        onRegisterSave(null);
      };
    }
  }, [onRegisterSave, handleSave]);

  // 1. Empty State
  if (!noteId) {
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

  // 2. Loading State
  if (editorStatus === "loading") {
    return (
      <div className="editor-loading-state" aria-label="Loading note">
        <div className="editor-spinner" />
        <span>Loading note...</span>
      </div>
    );
  }

  // 3. Error / Not Found State
  if (editorStatus === "error" && loadErrorState) {
    return (
      <div className="editor-error-state" role="alert">
        <div className="editor-error-icon">
          <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
        </div>
        <h3 className="editor-error-title">{loadErrorState.title}</h3>
        <p className="editor-error-subtitle">{loadErrorState.message}</p>
        <button
          type="button"
          className="toolbar-btn-save"
          onClick={() => fetchNote(noteId)}
        >
          {loadErrorState.isNotFound ? "Check Again" : "Try Again"}
        </button>
      </div>
    );
  }

  const updatedDisplay = useMemo(() => {
    if (!loadedNote?.modified_at) return "Just now";
    try {
      const date = new Date(loadedNote.modified_at);
      if (isNaN(date.getTime())) return "Just now";
      return date.toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      });
    } catch {
      return "Just now";
    }
  }, [loadedNote?.modified_at]);

  const createdDisplay = useMemo(() => {
    if (!loadedNote?.created_at) return null;
    try {
      const date = new Date(loadedNote.created_at);
      if (isNaN(date.getTime())) return null;
      return date.toLocaleDateString(undefined, {
        month: "short",
        day: "numeric",
        year: "numeric",
      });
    } catch {
      return null;
    }
  }, [loadedNote?.created_at]);

  return (
    <div className="editor-container">
      {/* Editor Toolbar */}
      <div className="editor-toolbar" role="toolbar" aria-label="Formatting toolbar">
        <div className="toolbar-group">
          <button
            type="button"
            className="toolbar-btn"
            title="Undo (Ctrl+Z)"
            aria-label="Undo"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => document.execCommand("undo")}
          >
            <UndoIcon />
          </button>
          <button
            type="button"
            className="toolbar-btn"
            title="Redo (Ctrl+Y or Ctrl+Shift+Z)"
            aria-label="Redo"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => document.execCommand("redo")}
          >
            <RedoIcon />
          </button>
        </div>

        <div className="toolbar-divider" />

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
        </div>

        <div className="toolbar-spacer" />

        <button
          type="button"
          className="toolbar-btn-save"
          title="Save Note"
          disabled={isSaving || (!isDirty && editorStatus !== "error")}
          onClick={handleSave}
        >
          <SaveIcon />
          <span>
            {isSaving
              ? "Saving..."
              : editorStatus === "error"
              ? "Retry Save"
              : isDirty
              ? "Save"
              : "Saved"}
          </span>
        </button>

        <button
          type="button"
          className="toolbar-format-btn"
          title={`Click to switch format (current: ${format.toUpperCase() === "MD" ? "Markdown" : "Plain Text"}). Format switch updates note metadata without altering raw content.`}
          onClick={handleToggleFormat}
        >
          <span>{format.toUpperCase()}</span>
        </button>
      </div>

      {/* Save Error Banner (non-intrusive alert with retry action) */}
      {saveError && (
        <div className="editor-save-error-banner" role="alert">
          <span>⚠️ {saveError}</span>
          <button
            type="button"
            className="toolbar-btn-save"
            onClick={handleSave}
            disabled={isSaving}
          >
            Retry Save
          </button>
        </div>
      )}

      {/* Editor Main Content Area */}
      <div
        ref={scrollAreaRef}
        className="editor-scroll-area"
        onScroll={handleScroll}
      >
        <div className="editor-document">
          {/* Note Title Input */}
          <div className="editor-title-wrap">
            <input
              ref={titleInputRef}
              type="text"
              className="editor-title-input selectable-text"
              placeholder="Note title..."
              value={title}
              onChange={handleTitleChange}
              onKeyDown={handleTitleKeyDown}
              aria-label="Note title"
            />
          </div>

          {/* Note Metadata Strip */}
          <div className="editor-meta-strip">
            <span className="meta-item">
              <span className="meta-label">Updated:</span> {updatedDisplay}
            </span>
            {createdDisplay && (
              <span className="meta-item">
                <span className="meta-label">Created:</span> {createdDisplay}
              </span>
            )}
            <span className="meta-item">
              <span className="meta-label">Format:</span>{" "}
              {format === "md" ? "Markdown (.md)" : "Plain Text (.txt)"}
            </span>
          </div>

          <div className="editor-body-divider" />

          {/* Note Body Content */}
          <div className="editor-body-wrap" onClick={handleBodyWrapClick}>
            <textarea
              ref={contentRef}
              className="editor-content-area selectable-text"
              placeholder="Start writing your note..."
              value={content}
              onChange={handleContentChange}
              onKeyDown={handleContentKeyDown}
              onSelect={handleRecordSelection}
              onKeyUp={handleRecordSelection}
              onMouseUp={handleRecordSelection}
              aria-label="Note content"
            />
          </div>
        </div>
      </div>

      {/* Note Status Bar */}
      <footer className="editor-status-bar">
        <div className="status-item">
          <span>Words: {wordCount}</span>
          <span className="status-separator">•</span>
          <span>Characters: {content.length}</span>
        </div>

        <div className="status-item">
          <span
            className={`status-indicator-dot ${editorStatus === "saving" ? "is-pulsing" : ""}`}
            style={{
              backgroundColor:
                editorStatus === "saving"
                  ? "var(--color-accent-primary, #3b82f6)"
                  : editorStatus === "error"
                  ? "var(--color-danger-default, #ef4444)"
                  : isDirty
                  ? "var(--color-warning-default, #f59e0b)"
                  : "var(--color-success-default, #10b981)",
            }}
          />
          <span>
            {editorStatus === "saving"
              ? "Saving..."
              : editorStatus === "error"
              ? "Save failed — click to retry"
              : isDirty
              ? "Unsaved changes"
              : lastSavedAt
              ? `Saved (${new Date(lastSavedAt).toLocaleTimeString([], {
                  hour: "2-digit",
                  minute: "2-digit",
                })})`
              : "Saved"}
          </span>
        </div>
      </footer>
    </div>
  );
};

// Re-export as EditorPlaceholder for backwards compatibility
export const EditorPlaceholder = NoteEditor;
export default NoteEditor;
