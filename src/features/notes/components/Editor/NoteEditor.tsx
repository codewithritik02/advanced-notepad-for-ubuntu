import React, { useState, useEffect, useLayoutEffect, useRef, useCallback, useMemo, useDeferredValue } from "react";
import { Note, NoteFormat, EditorStatus, getFormatDisplayName } from "../../types";
import { storageService, Tag } from "../../../../services/storage";
import { TagPicker } from "../../../tags";
import { AttachmentList, InsertImagePanel, useAttachments } from "../../../attachments";
import type { Attachment } from "../../../../services/storage/types";
import { NoteMetadataPanel } from "./NoteMetadataPanel";
import { NotePickerModal } from "../NotePicker";
import { insertInternalLinkIntoContent, parseInternalLinks } from "../../../../utils/linkParser";
import { formatAttachmentImageMarkdown } from "../../../../utils/markdown";
import { ScrollToTopButton } from "./ScrollToTopButton";
import { NoteMinimap } from "./NoteMinimap";
import { RenderedNoteContent, NoteLinksBar, BacklinksSection } from "../Links";
import { ExportNoteModal } from "../../../importExport";
import { logger } from "../../../../utils/logger";
import { isExternalId } from "../../../../utils/externalDocuments";
import { closeAfterPendingWrites } from "../../services/closeGuard";
import "./EditorPlaceholder.css";

export interface NoteEditorProps {
  noteId: string | null;
  /** Show the overview strip (minimap) beside long notes. Defaults to on. */
  showMinimap?: boolean;
  onSaveNote?: (id: string, title: string, content: string, format?: NoteFormat) => Promise<void> | void;
  onNoteUpdated?: (updatedNote: Note) => void;
  onRegisterSave?: (saveFn: (() => Promise<void>) | null) => void;
  notebookName?: string | null;
  notebookPath?: string | null;
  onMoveNote?: (note: Note) => void;
  noteTags?: Tag[];
  availableTags?: Tag[];
  onAddTag?: (tagId: string) => Promise<void>;
  onRemoveTag?: (tagId: string) => Promise<void>;
  onCreateTag?: (name: string) => Promise<Tag | null>;
  isTrash?: boolean;
  onDeleteNote?: (noteId: string) => Promise<void> | void;
  onRestoreNote?: (noteId: string) => void;
  onPermanentlyDeleteNote?: (noteId: string) => void;
  onActivateLink?: (targetNoteId: string) => void;
  onNavigateToNote?: (noteId: string) => void;
  /** Closes the open note (back to the empty editor). For an external file it also unlists it. */
  onCloseNote?: () => void;
  /** Location of the file when the open document is an external file. */
  externalPath?: string | null;
  /** Open straight in edit mode (a note that was just created). Everything else opens as a preview. */
  startInEditMode?: boolean;
  /** Whether the open external file is pinned in the sidebar, and the action to toggle that. */
  externalPinned?: boolean;
  onTogglePinExternal?: () => void;
  /** Called after a regular note was pinned or unpinned, so the list can re-sort. */
  onPinChanged?: () => void;
}

// Inline lightweight SVG icons for editor toolbar
const LinkIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
    <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
  </svg>
);

const ImageInsertIcon = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <rect x="3" y="3" width="18" height="18" rx="2" />
    <circle cx="8.5" cy="8.5" r="1.5" />
    <path d="m21 15-5-5L5 21" />
  </svg>
);

const PinFileIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <line x1="12" y1="17" x2="12" y2="22" />
    <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z" />
  </svg>
);

/** "/home/me/Downloads/a/b.txt" -> ".../Downloads/a/b.txt" (the last three parts are enough). */
function shortenPath(path: string | null): string {
  if (!path) return "";
  const parts = path.split("/").filter(Boolean);
  return parts.length > 3 ? `.../${parts.slice(-3).join("/")}` : path;
}

const CloseNoteIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <line x1="18" y1="6" x2="6" y2="18" />
    <line x1="6" y1="6" x2="18" y2="18" />
  </svg>
);

const EyeIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
    <circle cx="12" cy="12" r="3" />
  </svg>
);

const EditIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
  </svg>
);

const InfoIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="12" cy="12" r="10" />
    <line x1="12" y1="16" x2="12" y2="12" />
    <line x1="12" y1="8" x2="12.01" y2="8" />
  </svg>
);

// Inline lightweight SVG icons for editor toolbar
const SaveIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" />
    <polyline points="17 21 17 13 7 13 7 21" />
    <polyline points="7 3 7 8 15 8" />
  </svg>
);

const FolderMoveIcon = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
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

const StarIcon = ({ filled }: { filled: boolean }) => (
  <svg
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill={filled ? "var(--color-warning-default, #eab308)" : "none"}
    stroke={filled ? "var(--color-warning-default, #eab308)" : "currentColor"}
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
  </svg>
);

const TrashIcon = () => (
  <svg
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <polyline points="3 6 5 6 21 6" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
  </svg>
);

const ExportIcon = () => (
  <svg
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" y1="15" x2="12" y2="3" />
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
  showMinimap = true,
  onSaveNote,
  onNoteUpdated,
  onRegisterSave,
  notebookName,
  notebookPath,
  onMoveNote,
  noteTags = [],
  availableTags = [],
  onAddTag,
  onRemoveTag,
  onCreateTag,
  isTrash = false,
  onDeleteNote,
  onRestoreNote,
  onPermanentlyDeleteNote,
  onActivateLink,
  onNavigateToNote,
  onCloseNote,
  externalPath = null,
  startInEditMode = false,
  externalPinned = false,
  onTogglePinExternal,
  onPinChanged,
}) => {
  // A file opened from outside the notebook: saved back to the file, no notebook/tags/attachments
  const isExternal = isExternalId(noteId);
  const [loadedNote, setLoadedNote] = useState<Note | null>(null);
  const isReadOnly = isTrash || !!loadedNote?.is_deleted;
  const [editorStatus, setEditorStatus] = useState<EditorStatus>("idle");
  const [isMetadataOpen, setIsMetadataOpen] = useState(false);
  const [loadErrorState, setLoadErrorState] = useState<{
    title: string;
    message: string;
    isNotFound: boolean;
  } | null>(null);
  const [lastSavedAt, setLastSavedAt] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [attachmentCount, setAttachmentCount] = useState<number>(0);
  const [isExportModalOpen, setIsExportModalOpen] = useState(false);

  // One attachment state shared by the Attachments list and the "Insert image" panel
  const attachmentsController = useAttachments(isExternal ? null : noteId);
  const [isImagePanelOpen, setIsImagePanelOpen] = useState(false);
  const imageInsertSelectionRef = useRef<{ start: number; end: number } | null>(null);
  // True once the user has placed the cursor in the text (click, key, selection)
  const cursorTouchedRef = useRef(false);

  // Fetch targeted attachment count for metadata panel (Phase 7, Task 19)
  useEffect(() => {
    let isCurrent = true;
    if (!noteId || isExternal) {
      setAttachmentCount(0);
      return;
    }
    storageService.attachments
      .getAttachmentCount(noteId)
      .then((count) => {
        if (isCurrent) setAttachmentCount(count);
      })
      .catch(() => {
        if (isCurrent) setAttachmentCount(0);
      });
    return () => {
      isCurrent = false;
    };
  }, [noteId, isExternal]);

  // In-memory editing state
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [format, setFormat] = useState<NoteFormat>("txt");
  const [isDirty, setIsDirty] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [viewMode, setViewMode] = useState<"edit" | "preview">("edit");

  // Defer expensive derived calculations off the critical typing path (React 19)
  const deferredContent = useDeferredValue(content);

  // Fast zero-allocation word counter for responsive performance with large notes (up to 1MB+)
  const wordCount = useMemo(() => countWords(deferredContent), [deferredContent]);

  // Outgoing internal links: debounced off the critical typing path for large notes
  const [parsedLinks, setParsedLinks] = useState(() => parseInternalLinks(content));
  const linksDebounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (linksDebounceTimerRef.current) {
      clearTimeout(linksDebounceTimerRef.current);
      linksDebounceTimerRef.current = null;
    }

    if (content.length < 20000) {
      setParsedLinks(parseInternalLinks(content));
    } else {
      linksDebounceTimerRef.current = setTimeout(() => {
        setParsedLinks(parseInternalLinks(content));
      }, 300);
    }

    return () => {
      if (linksDebounceTimerRef.current) {
        clearTimeout(linksDebounceTimerRef.current);
      }
    };
  }, [content, noteId]);

  // Fast zero-allocation UTF-8 byte counter; only computed when metadata panel is open
  const byteSize = useMemo(() => {
    if (!isMetadataOpen) return 0;
    let bytes = 0;
    const len = deferredContent.length;
    for (let i = 0; i < len; i++) {
      const code = deferredContent.charCodeAt(i);
      if (code <= 0x7f) bytes += 1;
      else if (code <= 0x7ff) bytes += 2;
      else if (code >= 0xd800 && code <= 0xdbff) {
        bytes += 4;
        i++;
      } else bytes += 3;
    }
    return bytes;
  }, [isMetadataOpen, deferredContent]);

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
      latestDataRef.current = { title: note.title, content: note.content, format: resolvedFormat };
      isDirtyRef.current = false;
      setIsDirty(false);
      setEditorStatus("saved");
      setLastSavedAt(note.modified_at);
      // An empty note has nothing to preview
      if (!note.content.trim()) setViewMode("edit");

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
    setViewMode(startInEditMode ? "edit" : "preview");

    if (!noteId) {
      setLoadedNote(null);
      setEditorStatus("idle");
      setLoadErrorState(null);
      setSaveError(null);
      setTitle("");
      setContent("");
      latestDataRef.current = { title: "", content: "", format: "txt" };
      isDirtyRef.current = false;
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
  const inFlightSavePromiseRef = useRef<Promise<void> | null>(null);
  const latestDataRef = useRef({ title: "", content: "", format: "txt" as NoteFormat });
  const prevNoteIdRef = useRef<string | null>(null);
  const isDirtyRef = useRef(false);
  const closeInProgressRef = useRef(false);
  const forceCloseArmedRef = useRef(false);

  useEffect(() => {
    isDirtyRef.current = isDirty;
  }, [isDirty]);

  useEffect(() => {
    // A force-close confirmation only applies to the note whose close-time save failed.
    forceCloseArmedRef.current = false;
  }, [noteId]);

  // Keep latestDataRef current with latest user edits
  useEffect(() => {
    latestDataRef.current = { title, content, format };
  }, [title, content, format]);

  // Save on note switch / unmount: Preserve previous note view state and flush dirty changes
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

        const dataToFlush = { ...latestDataRef.current };
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
          .catch((err) => {
            logger.error("Failed to flush note changes on note switch", err, { noteId: previousId });
          });
      }
    }

    prevNoteIdRef.current = noteId;
  }, [noteId, onNoteUpdated]);

  // Save before application close and flush on unmount
  useEffect(() => {
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      // A browser unload cannot reliably await an asynchronous SQLite command. Surface the
      // pending work instead; the Tauri close handler below performs the awaited native flow.
      if (noteId && (isDirtyRef.current || inFlightSavePromiseRef.current)) {
        event.preventDefault();
        event.returnValue = "";
      }
    };

    window.addEventListener("beforeunload", handleBeforeUnload);

    // Tauri-specific close listener
    let unlistenTauriClose: (() => void) | undefined;
    let closeListenerDisposed = false;
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => {
        const appWindow = getCurrentWindow();
        appWindow
          .onCloseRequested(async (event) => {
            if (forceCloseArmedRef.current) {
              event.preventDefault();
              closeInProgressRef.current = true;
              try {
                await appWindow.destroy();
              } catch (err) {
                closeInProgressRef.current = false;
                setEditorStatus("error");
                setSaveError("Unable to close the application. Please try again.");
                logger.error("Forced window close failed", err, { noteId });
              }
              return;
            }

            if (!noteId || (!isDirtyRef.current && !inFlightSavePromiseRef.current)) {
              return;
            }
            if (closeInProgressRef.current) {
              event.preventDefault();
              return;
            }

            closeInProgressRef.current = true;
            const closed = await closeAfterPendingWrites({
              event,
              waitForPendingSave: async () => {
                await inFlightSavePromiseRef.current;
              },
              savePendingChanges: async () => {
                if (!isDirtyRef.current) return;
                if (debounceTimerRef.current) {
                  clearTimeout(debounceTimerRef.current);
                  debounceTimerRef.current = null;
                }
                const data = { ...latestDataRef.current };
                const updated = await storageService.notes.update(noteId, {
                  title: data.title.trim() ? data.title : "Untitled Note",
                  content: data.content,
                  format: data.format,
                });
                isDirtyRef.current = false;
                setIsDirty(false);
                setEditorStatus("saved");
                setSaveError(null);
                onNoteUpdated?.(updated);
              },
              destroyWindow: () => appWindow.destroy(),
              reportFailure: (err) => {
                forceCloseArmedRef.current = true;
                setEditorStatus("error");
                setSaveError(
                  "Unable to finish saving. Close the window again to force quit; recent unsaved changes may be lost."
                );
                logger.error("Close-time note persistence failed; window kept open", err, { noteId });
              },
            });
            if (!closed) closeInProgressRef.current = false;
          })
          .then((unlisten) => {
            // The effect can be cleaned up while the dynamic import/listener registration is
            // still pending (especially under React Strict Mode). Immediately remove a late
            // listener so stale handlers cannot keep preventing future close requests.
            if (closeListenerDisposed) {
              unlisten();
            } else {
              unlistenTauriClose = unlisten;
            }
          })
          .catch(() => {});
      })
      .catch(() => {});

    return () => {
      closeListenerDisposed = true;
      window.removeEventListener("beforeunload", handleBeforeUnload);
      unlistenTauriClose?.();
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }

      // Flush uncommitted dirty edits on component unmount
      if (!closeInProgressRef.current && isDirtyRef.current && noteId) {
        const dataToFlush = { ...latestDataRef.current };
        const titleToFlush = dataToFlush.title.trim() ? dataToFlush.title : "Untitled Note";
        storageService.notes
          .update(noteId, {
            title: titleToFlush,
            content: dataToFlush.content,
            format: dataToFlush.format,
          })
          .then((updated) => {
            onNoteUpdated?.(updated);
          })
          .catch((err) => {
            logger.error("Failed to flush note changes on editor unmount", err, { noteId });
          });
      }

      // Cache cursor and scroll position for this note
      if (noteId && (contentRef.current || scrollAreaRef.current)) {
        sessionViewCache.set(noteId, {
          cursorStart: contentRef.current?.selectionStart ?? 0,
          cursorEnd: contentRef.current?.selectionEnd ?? 0,
          scrollTop: scrollAreaRef.current?.scrollTop ?? 0,
        });
      }
    };
  }, [noteId, onNoteUpdated]);

  // Auto-grow textarea to accommodate content without secondary scrollbars.
  // Measuring needs `height: auto`, which momentarily shrinks the whole document; the browser then
  // clamps the scroll position (usually to the top). The scroll offset is therefore restored, in a
  // layout effect so nothing visibly jumps.
  useLayoutEffect(() => {
    const textarea = contentRef.current;
    if (!textarea) return;
    const scroller = scrollAreaRef.current;
    const previousScrollTop = scroller ? scroller.scrollTop : 0;
    textarea.style.height = "auto";
    textarea.style.height = `${Math.max(textarea.scrollHeight, 380)}px`;
    if (scroller && scroller.scrollTop !== previousScrollTop) {
      scroller.scrollTop = previousScrollTop;
    }
  }, [content]);

  // Core persistence worker function with snapshot isolation & generation checks
  const executeSave = useCallback(
    async (
      targetNoteId: string,
      rev: number,
      snapshotData?: { title: string; content: string; format: NoteFormat }
    ) => {
      if (isSavingRef.current || isReadOnly) return;
      isSavingRef.current = true;
      setIsSaving(true);
      if (activeFetchIdRef.current === targetNoteId) {
        setEditorStatus("saving");
        setSaveError(null);
      }

      const dataToSave = snapshotData || { ...latestDataRef.current };
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

        // Only commit revision if this response is not superseded by a newer save FOR THIS NOTE
        if (rev >= latestPersistedRevisionRef.current) {
          latestPersistedRevisionRef.current = rev;
          onNoteUpdated?.(updated);

          // ONLY update active editor UI state if the user is still actively editing this note!
          if (activeFetchIdRef.current === targetNoteId) {
            setLoadedNote(updated);
            setLastSavedAt(updated.modified_at);

            // If no new typing occurred since this save was initiated, mark as clean
            if (revisionRef.current === rev) {
              setIsDirty(false);
              isDirtyRef.current = false;
              forceCloseArmedRef.current = false;
              setEditorStatus("saved");
            }
          }
        }
      } catch (err) {
        isDirtyRef.current = true;
        setEditorStatus("error");
        if (activeFetchIdRef.current === targetNoteId) {
          setIsDirty(true);
          setSaveError(
            "Unable to save changes. Your changes are still available locally in the editor."
          );
        }
        logger.error("Failed to execute save for note", err, { noteId: targetNoteId, revision: rev });
        throw err;
      } finally {
        isSavingRef.current = false;
        if (activeFetchIdRef.current === targetNoteId) {
          setIsSaving(false);
        }

        // If the user continued typing while save was in-flight, schedule an immediate follow-up
        if (activeFetchIdRef.current === targetNoteId && revisionRef.current > latestPersistedRevisionRef.current) {
          if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
          debounceTimerRef.current = setTimeout(() => {
            const followUpData = { ...latestDataRef.current };
            const followUpPromise = executeSave(targetNoteId, revisionRef.current, followUpData).catch(() => {});
            inFlightSavePromiseRef.current = followUpPromise;
            followUpPromise.finally(() => {
              if (inFlightSavePromiseRef.current === followUpPromise) {
                inFlightSavePromiseRef.current = null;
              }
            });
          }, 800);
        }
      }
    },
    [onSaveNote, onNoteUpdated, loadedNote, isReadOnly]
  );

  // Trigger autosave with 800ms debounce
  const scheduleAutosave = useCallback(
    (targetNoteId: string) => {
      revisionRef.current += 1;
      const currentRev = revisionRef.current;
      const currentSnapshot = { ...latestDataRef.current };
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
      debounceTimerRef.current = setTimeout(() => {
        const savePromise = executeSave(targetNoteId, currentRev, currentSnapshot).catch(() => {});
        inFlightSavePromiseRef.current = savePromise;
        savePromise.finally(() => {
          if (inFlightSavePromiseRef.current === savePromise) {
            inFlightSavePromiseRef.current = null;
          }
        });
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
      newFormat !== ((loadedNote.format as NoteFormat) === "md" ? "md" : "txt") ||
      newContent.length !== loadedNote.content.length ||
      newContent !== loadedNote.content;

    if (hasChanged) {
      if (!isDirtyRef.current) {
        setIsDirty(true);
        isDirtyRef.current = true;
        setEditorStatus("unsaved");
      }
    } else {
      if (isDirtyRef.current) {
        setIsDirty(false);
        isDirtyRef.current = false;
        setEditorStatus("saved");
      }
    }
  };

  const handleTitleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (isReadOnly) return;
    const newTitle = e.target.value;
    latestDataRef.current.title = newTitle;
    isDirtyRef.current = true;
    setTitle(newTitle);
    updateDirtyState(newTitle, content, format);
    if (noteId) scheduleAutosave(noteId);
  };

  const handleContentChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    if (isReadOnly) return;
    const newContent = e.target.value;
    latestDataRef.current.content = newContent;
    isDirtyRef.current = true;
    setContent(newContent);
    updateDirtyState(title, newContent, format);
    if (noteId) scheduleAutosave(noteId);
  };

  const handleRecordSelection = useCallback(() => {
    cursorTouchedRef.current = true;
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

  // Link Picker State & Handlers (Phase 9, Task 6 & 7)
  const [isLinkPickerOpen, setIsLinkPickerOpen] = useState(false);
  const [linkPickerInitialLabel, setLinkPickerInitialLabel] = useState("");
  const pendingInsertionSelectionRef = useRef<{ start: number; end: number; sourceNoteId: string } | null>(null);

  // Close and invalidate pending picker if note selection changes
  useEffect(() => {
    setIsLinkPickerOpen(false);
    pendingInsertionSelectionRef.current = null;
  }, [noteId]);

  const handleOpenLinkPicker = useCallback(() => {
    if (isReadOnly || !noteId) return;

    let start = content.length;
    let end = content.length;
    let selectedText = "";

    if (contentRef.current) {
      start = contentRef.current.selectionStart;
      end = contentRef.current.selectionEnd;
      if (start < end) {
        selectedText = content.slice(start, end).trim();
      }
    }

    pendingInsertionSelectionRef.current = {
      start,
      end,
      sourceNoteId: noteId,
    };
    setLinkPickerInitialLabel(selectedText);
    setIsLinkPickerOpen(true);
  }, [isReadOnly, noteId, content]);

  const handleSelectLinkTarget = useCallback(
    (target: { id: string; title: string; customLabel?: string }) => {
      setIsLinkPickerOpen(false);

      const pending = pendingInsertionSelectionRef.current;
      pendingInsertionSelectionRef.current = null;

      // Invalidate if note switched or editor is read-only
      if (!pending || pending.sourceNoteId !== noteId || !noteId || isReadOnly) {
        return;
      }

      const insertionResult = insertInternalLinkIntoContent({
        content: latestDataRef.current.content,
        selectionStart: pending.start,
        selectionEnd: pending.end,
        targetNoteId: target.id,
        targetTitle: target.title,
        customLabel: target.customLabel,
      });

      const newContent = insertionResult.updatedContent;
      latestDataRef.current.content = newContent;
      isDirtyRef.current = true;
      setContent(newContent);
      updateDirtyState(title, newContent, format);
      scheduleAutosave(noteId);

      // Restore focus and place cursor right after inserted link
      requestAnimationFrame(() => {
        if (contentRef.current) {
          contentRef.current.focus();
          contentRef.current.selectionStart = insertionResult.newCursorPosition;
          contentRef.current.selectionEnd = insertionResult.newCursorPosition;
          handleRecordSelection();
        }
      });
    },
    [noteId, isReadOnly, title, format, scheduleAutosave, handleRecordSelection]
  );

  const handleCloseLinkPicker = useCallback(() => {
    setIsLinkPickerOpen(false);
    pendingInsertionSelectionRef.current = null;
    requestAnimationFrame(() => {
      contentRef.current?.focus();
    });
  }, []);

  // ---- Insert attached images into the note text -----------------------------------------
  const closeImagePanel = useCallback(() => setIsImagePanelOpen(false), []);

  /**
   * Remembers where to put the image before a button steals focus: at the cursor if the user placed
   * one. Otherwise (never clicked in the text, or the note is shown in Preview) it goes where they
   * are looking: the paragraph break just below the middle of the visible area of the note.
   */
  const captureImageInsertionPoint = () => {
    const text = latestDataRef.current.content;
    const length = text.length;
    const textarea = contentRef.current;
    const cached = noteId ? sessionViewCache.get(noteId) : undefined;
    const hasCursor =
      Boolean(textarea) && (cursorTouchedRef.current || (cached?.cursorStart ?? 0) > 0);
    if (textarea && hasCursor) {
      imageInsertSelectionRef.current = {
        start: Math.min(textarea.selectionStart, length),
        end: Math.min(textarea.selectionEnd, length),
      };
      return;
    }

    const scroller = scrollAreaRef.current;
    if (!scroller || length === 0 || scroller.scrollHeight <= scroller.clientHeight) {
      imageInsertSelectionRef.current = { start: length, end: length };
      return;
    }
    // Map the middle of the visible area onto the text, then move to the next paragraph break
    const textTop = textarea ? textarea.offsetTop : 0;
    const textHeight = textarea ? textarea.offsetHeight : scroller.scrollHeight;
    const focusY = scroller.scrollTop + scroller.clientHeight * 0.5 - textTop;
    const ratio = Math.min(1, Math.max(0, focusY / Math.max(1, textHeight)));
    const guess = Math.floor(ratio * length);
    const nextBreak = text.indexOf("\n\n", guess);
    const point = nextBreak === -1 ? length : nextBreak;
    imageInsertSelectionRef.current = { start: point, end: point };
  };

  /** Places `![name](notepad://attachment/<id>)` blocks at the remembered cursor position. */
  const insertImagesIntoNote = (images: Attachment[], nextFormat: NoteFormat) => {
    if (!noteId || isReadOnly || images.length === 0) return;

    const current = latestDataRef.current.content;
    const point = imageInsertSelectionRef.current ?? { start: current.length, end: current.length };
    const start = Math.min(point.start, current.length);
    const end = Math.min(Math.max(point.end, start), current.length);
    const before = current.slice(0, start);
    const after = current.slice(end);

    // Each image sits in its own paragraph so Markdown renders it between the surrounding text.
    const block = images
      .map((image) => formatAttachmentImageMarkdown(image.id, image.file_name))
      .join("\n\n");
    const leading =
      before.length === 0 ? "" : before.endsWith("\n\n") ? "" : before.endsWith("\n") ? "\n" : "\n\n";
    const trailing =
      after.length === 0 ? "\n" : after.startsWith("\n\n") ? "" : after.startsWith("\n") ? "\n" : "\n\n";
    const inserted = `${leading}${block}${trailing}`;
    const newContent = before + inserted + after;
    const cursor = before.length + inserted.length;

    if (nextFormat !== format) {
      setFormat(nextFormat);
      latestDataRef.current.format = nextFormat;
    }
    latestDataRef.current.content = newContent;
    setContent(newContent);
    updateDirtyState(title, newContent, nextFormat);
    scheduleAutosave(noteId);
    imageInsertSelectionRef.current = null;

    // Put the cursor after the image but leave the view exactly where it was
    requestAnimationFrame(() => {
      if (contentRef.current) {
        contentRef.current.focus({ preventScroll: true });
        contentRef.current.selectionStart = cursor;
        contentRef.current.selectionEnd = cursor;
        handleRecordSelection();
      }
    });
  };

  const requestInsertImages = (images: Attachment[]) => {
    if (!noteId || isReadOnly || images.length === 0) return;
    setIsImagePanelOpen(false);
    // Both Markdown and plain-text notes show attached images in Preview, so the format stays as is.
    insertImagesIntoNote(images, format);
  };

  const handleToggleImagePanel = () => {
    if (isReadOnly || !noteId) return;
    if (!isImagePanelOpen) captureImageInsertionPoint();
    setIsImagePanelOpen((open) => !open);
  };

  // Keep the Note Details attachment count in step with the shared attachment list
  useEffect(() => {
    if (attachmentsController.status !== "loading") {
      setAttachmentCount(attachmentsController.attachments.length);
    }
  }, [attachmentsController.attachments.length, attachmentsController.status]);

  // Internal link activation handler (Phase 9, Task 8 & 10)
  const handleActivateLink = useCallback(
    (targetNoteId: string) => {
      if (onActivateLink) {
        onActivateLink(targetNoteId);
      } else if (onNavigateToNote) {
        onNavigateToNote(targetNoteId);
      } else {
        console.debug("[InternalLink] Activated link target:", targetNoteId);
      }
    },
    [onActivateLink, onNavigateToNote]
  );

  // Toggle Edit / Preview mode handler
  const handleToggleViewMode = useCallback(() => {
    setViewMode((prev) => {
      const next = prev === "edit" ? "preview" : "edit";
      if (next === "edit") {
        requestAnimationFrame(() => {
          contentRef.current?.focus();
        });
      }
      return next;
    });
  }, []);

  // Global window listener to toggle back to Edit mode from Preview mode (Ctrl+Shift+P or Ctrl+E)
  useEffect(() => {
    if (viewMode !== "preview") return;

    const handleWindowKeyDown = (e: KeyboardEvent) => {
      if (
        (e.ctrlKey || e.metaKey) &&
        ((e.key.toLowerCase() === "p" && e.shiftKey) || e.key.toLowerCase() === "e")
      ) {
        e.preventDefault();
        e.stopPropagation();
        handleToggleViewMode();
      }
    };

    window.addEventListener("keydown", handleWindowKeyDown);
    return () => window.removeEventListener("keydown", handleWindowKeyDown);
  }, [viewMode, handleToggleViewMode]);

  const handleContentKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Explicit Save shortcut (Ctrl/Cmd + S)
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      if (!isReadOnly) {
        handleSave();
      }
      return;
    }

    // Insert Link shortcut (Ctrl/Cmd + K)
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      e.stopPropagation();
      if (!isReadOnly && noteId) {
        handleOpenLinkPicker();
      }
      return;
    }

    // Toggle Edit / Preview mode shortcut (Ctrl/Cmd + Shift + P or Ctrl/Cmd + E)
    if (
      (e.ctrlKey || e.metaKey) &&
      ((e.key.toLowerCase() === "p" && e.shiftKey) || e.key.toLowerCase() === "e")
    ) {
      e.preventDefault();
      e.stopPropagation();
      handleToggleViewMode();
      return;
    }

    if (isReadOnly) return;

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
      if (!isReadOnly) {
        handleSave();
      }
      return;
    }

    if (isReadOnly) return;

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

  const handleOpenExport = async () => {
    if (loadedNote && isDirtyRef.current) {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }
      try {
        await executeSave(loadedNote.id, revisionRef.current);
      } catch {
        // Handled in executeSave error state
      }
    }
    setIsExportModalOpen(true);
  };

  const handleToggleFormat = () => {
    if (isReadOnly) return;
    const nextFormat: NoteFormat = format === "txt" ? "md" : "txt";
    setFormat(nextFormat);
    updateDirtyState(title, content, nextFormat);
    if (noteId) scheduleAutosave(noteId);
  };

  const handleToggleFavorite = useCallback(async () => {
    if (!noteId || !loadedNote || isReadOnly) return;
    const currentFav = loadedNote.is_favorite;
    const nextFav = !currentFav;
    setLoadedNote((prev) => (prev ? { ...prev, is_favorite: nextFav } : null));
    try {
      const updated = await storageService.notes.setFavorite(noteId, nextFav);
      setLoadedNote(updated);
      if (onNoteUpdated) {
        onNoteUpdated(updated);
      }
    } catch {
      setLoadedNote((prev) => (prev ? { ...prev, is_favorite: currentFav } : null));
    }
  }, [noteId, loadedNote, onNoteUpdated, isReadOnly]);

  const handleTogglePinNote = useCallback(async () => {
    if (!noteId || !loadedNote || isReadOnly || isExternal) return;
    const current = loadedNote.is_pinned;
    setLoadedNote((prev) => (prev ? { ...prev, is_pinned: !current } : null));
    try {
      const updated = await storageService.notes.setPinned(noteId, !current);
      setLoadedNote((prev) => (prev ? { ...prev, is_pinned: updated.is_pinned } : prev));
      onNoteUpdated?.(updated);
      onPinChanged?.();
    } catch {
      setLoadedNote((prev) => (prev ? { ...prev, is_pinned: current } : null));
    }
  }, [noteId, loadedNote, isReadOnly, isExternal, onNoteUpdated, onPinChanged]);

  const handleSave = useCallback(async () => {
    if (!noteId || isReadOnly) return;

    // Task 54: If an autosave is currently in-flight, await its completion first
    if (inFlightSavePromiseRef.current) {
      await inFlightSavePromiseRef.current;
    }

    if (!isDirtyRef.current && editorStatus !== "error") {
      // Per spec Section 42: If there are no changes, do nothing or provide unobtrusive confirmation
      return;
    }
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
    revisionRef.current += 1;
    const savePromise = executeSave(noteId, revisionRef.current);
    inFlightSavePromiseRef.current = savePromise;
    try {
      await savePromise;
    } finally {
      if (inFlightSavePromiseRef.current === savePromise) {
        inFlightSavePromiseRef.current = null;
      }
    }
  }, [noteId, editorStatus, executeSave]);

  useEffect(() => {
    if (onRegisterSave) {
      if (isReadOnly) {
        onRegisterSave(null);
      } else {
        onRegisterSave(handleSave);
      }
      return () => {
        onRegisterSave(null);
      };
    }
  }, [onRegisterSave, handleSave, isReadOnly]);

  const handleDelete = useCallback(async () => {
    if (isReadOnly || !loadedNote || !onDeleteNote) return;
    if (isDirtyRef.current) {
      await handleSave();
    }
    await onDeleteNote(loadedNote.id);
  }, [isReadOnly, loadedNote, onDeleteNote, handleSave]);

  // Hooks must run on every render, so these stay above the early returns below
  // (a hook after a conditional return crashes React with error #300/#310 when the
  // editor switches between its empty, loading, error and loaded states).
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

  return (
    <div className={`editor-container ${isReadOnly ? "is-read-only" : ""}`}>
      {/* Trashed Note Notification Banner (Phase 8, Task 13) */}
      {isReadOnly && loadedNote && (
        <div className="trashed-note-banner" role="status" aria-live="polite">
          <div className="trashed-note-banner-info">
            <span className="trashed-note-banner-icon" aria-hidden="true">
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
              </svg>
            </span>
            <div className="trashed-note-banner-text">
              <strong>This note is in Trash.</strong>
              <span>It is read-only. Restore this note to make changes.</span>
            </div>
          </div>
          <div className="trashed-note-banner-actions">
            {onRestoreNote && (
              <button
                type="button"
                className="btn-trash-banner btn-trash-banner--restore"
                onClick={() => onRestoreNote(loadedNote.id)}
                aria-label={`Restore "${loadedNote.title || 'Untitled Note'}"`}
              >
                Restore Note
              </button>
            )}
            {onPermanentlyDeleteNote && (
              <button
                type="button"
                className="btn-trash-banner btn-trash-banner--delete"
                onClick={() => onPermanentlyDeleteNote(loadedNote.id)}
                aria-label={`Permanently delete "${loadedNote.title || 'Untitled Note'}"`}
              >
                Delete Permanently
              </button>
            )}
          </div>
        </div>
      )}

      {isExternal && loadedNote && (
        <div className="external-note-banner" role="status">
          <strong>External file</strong>
          <span className="external-note-banner-path" title={externalPath ?? undefined}>
            {shortenPath(externalPath) || loadedNote.title}
          </span>
          <span>Edits are saved straight to this file. It is not part of your notes.</span>
        </div>
      )}

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
            disabled={isReadOnly}
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
            disabled={isReadOnly}
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

        <div className="toolbar-divider" />

        <div className="toolbar-group">
          <button
            type="button"
            className="toolbar-btn"
            title="Insert link (Ctrl+K)"
            aria-label="Insert link"
            disabled={isReadOnly || !noteId || isExternal}
            onMouseDown={(e) => {
              // Keep textarea selection intact when clicking button
              e.preventDefault();
            }}
            onClick={handleOpenLinkPicker}
          >
            <LinkIcon />
          </button>

          <div className="toolbar-popover-anchor">
            <button
              type="button"
              className={`toolbar-btn ${isImagePanelOpen ? "is-active" : ""}`}
              title="Insert image"
              aria-label="Insert image"
              aria-haspopup="dialog"
              aria-expanded={isImagePanelOpen}
              data-insert-image-toggle
              disabled={isReadOnly || !noteId || isExternal}
              onMouseDown={(e) => {
                // Keep textarea selection intact when clicking button
                e.preventDefault();
              }}
              onClick={handleToggleImagePanel}
            >
              <ImageInsertIcon />
            </button>
            {isImagePanelOpen && (
              <InsertImagePanel
                attachments={attachmentsController.attachments}
                isLoading={attachmentsController.status === "loading"}
                isBusy={attachmentsController.isBusy}
                onInsert={requestInsertImages}
                onPickFiles={attachmentsController.pickAndAddFiles}
                onClose={closeImagePanel}
              />
            )}
          </div>
        </div>

        <div className="toolbar-spacer" />

        {!isReadOnly && !isExternal && onMoveNote && loadedNote && (
          <button
            type="button"
            className="toolbar-notebook-badge"
            title={`Move note to notebook (current: ${notebookName || "Unfiled"})`}
            onClick={() => onMoveNote(loadedNote)}
          >
            <FolderMoveIcon />
            <span>{notebookName || "Unfiled"}</span>
          </button>
        )}

        {!isReadOnly && !isExternal && loadedNote && (
          <button
            type="button"
            className={`toolbar-btn-favorite ${loadedNote.is_favorite ? "is-favorite" : ""}`}
            title={loadedNote.is_favorite ? "Remove from favorites" : "Add to favorites"}
            aria-label={loadedNote.is_favorite ? "Remove from favorites" : "Add to favorites"}
            aria-pressed={loadedNote.is_favorite}
            onClick={handleToggleFavorite}
          >
            <StarIcon filled={loadedNote.is_favorite} />
            <span>{loadedNote.is_favorite ? "Favorited" : "Favorite"}</span>
          </button>
        )}

        {!isReadOnly && !isExternal && loadedNote && onDeleteNote && (
          <button
            type="button"
            className="toolbar-btn toolbar-btn-delete"
            title="Delete note (Move to Trash)"
            aria-label={`Delete "${loadedNote.title || 'Untitled Note'}"`}
            onClick={handleDelete}
          >
            <TrashIcon />
          </button>
        )}

        {isReadOnly ? (
          <span className="trashed-pill">Trash (Read-Only)</span>
        ) : (
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
        )}

        {loadedNote && (
          <button
            type="button"
            className={`toolbar-btn ${isMetadataOpen ? "is-active" : ""}`}
            title="Note Details (Metadata)"
            aria-label="Toggle note details"
            aria-pressed={isMetadataOpen}
            onClick={() => setIsMetadataOpen((prev) => !prev)}
          >
            <InfoIcon />
          </button>
        )}

        {loadedNote && !isTrash && !isExternal && (
          <button
            type="button"
            className="toolbar-btn"
            title="Export Note..."
            aria-label={`Export "${loadedNote.title || 'Untitled Note'}"`}
            onClick={handleOpenExport}
          >
            <ExportIcon />
          </button>
        )}

        <button
          type="button"
          className="toolbar-format-btn"
          title={`Click to switch format (current: ${format.toUpperCase() === "MD" ? "Markdown" : "Plain Text"}). Format switch updates note metadata without altering raw content.`}
          disabled={isExternal}
          onClick={handleToggleFormat}
        >
          <span>{format.toUpperCase()}</span>
        </button>

        <button
          type="button"
          className={`toolbar-btn ${viewMode === "preview" ? "is-active" : ""}`}
          title={
            viewMode === "preview"
              ? "Switch to Edit mode (Ctrl+Shift+P / Ctrl+E)"
              : "Switch to Reading/Preview mode (Ctrl+Shift+P / Ctrl+E)"
          }
          aria-label={
            viewMode === "preview"
              ? "Switch to Edit mode"
              : "Switch to Reading/Preview mode"
          }
          aria-pressed={viewMode === "preview"}
          disabled={!noteId}
          onClick={handleToggleViewMode}
        >
          {viewMode === "preview" ? <EditIcon /> : <EyeIcon />}
        </button>

        {isExternal && onTogglePinExternal && (
          <button
            type="button"
            className={`toolbar-btn ${externalPinned ? "is-active" : ""}`}
            title={externalPinned ? "Unpin from the sidebar" : "Pin to the sidebar (keep it for next time)"}
            aria-label={externalPinned ? "Unpin file" : "Pin file"}
            aria-pressed={externalPinned}
            onClick={onTogglePinExternal}
          >
            <PinFileIcon />
          </button>
        )}

        {!isExternal && !isReadOnly && loadedNote && (
          <button
            type="button"
            className={`toolbar-btn ${loadedNote.is_pinned ? "is-active" : ""}`}
            title={loadedNote.is_pinned ? "Unpin note (stops showing at the top)" : "Pin note to the top of the list"}
            aria-label={loadedNote.is_pinned ? "Unpin note" : "Pin note"}
            aria-pressed={!!loadedNote.is_pinned}
            onClick={handleTogglePinNote}
          >
            <PinFileIcon />
          </button>
        )}

        {onCloseNote && (
          <button
            type="button"
            className="toolbar-btn toolbar-btn-close-note"
            title={isExternal ? "Close this file (Ctrl+W)" : "Close note (Ctrl+W)"}
            aria-label={isExternal ? "Close file" : "Close note"}
            onClick={onCloseNote}
          >
            <CloseNoteIcon />
          </button>
        )}
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

      {/* Editor Main Content Area (scrolling column + optional minimap) */}
      <div className="editor-scroll-wrap">
      <div className="editor-scroll-column">
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
              placeholder={isReadOnly ? "Untitled Note" : "Note title..."}
              value={title}
              readOnly={isReadOnly || isExternal}
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
              {getFormatDisplayName(format)}
            </span>
          </div>

          {/* Note Tags Strip & Picker (Task 17) */}
          {!isExternal && (
          <TagPicker
            noteTags={noteTags}
            availableTags={availableTags}
            onAddTag={onAddTag}
            onRemoveTag={onRemoveTag}
            onCreateTag={onCreateTag}
            disabled={!noteId || isReadOnly}
          />
          )}

          {/* Note Metadata Details Panel (Task 30) */}
          <NoteMetadataPanel
            isOpen={isMetadataOpen}
            onToggle={() => setIsMetadataOpen((prev) => !prev)}
            notebookPath={isExternal ? "External file" : notebookPath || notebookName || "Unfiled"}
            noteTags={noteTags}
            createdAt={loadedNote?.created_at}
            modifiedAt={loadedNote?.modified_at}
            format={format}
            isFavorite={loadedNote?.is_favorite}
            wordCount={wordCount}
            characterCount={content.length}
            byteSize={byteSize}
            attachmentCount={attachmentCount}
            onMoveNote={!isReadOnly && !isExternal && onMoveNote && loadedNote ? () => onMoveNote(loadedNote) : undefined}
            onToggleFavorite={!isReadOnly && !isExternal ? handleToggleFavorite : undefined}
          />

          {/* Note File Attachments (Phase 7, Tasks 11-19) */}
          {/* <AttachmentList noteId={noteId} /> */}
          {!isExternal && (
          <AttachmentList
            noteId={noteId}
            isReadOnly={isReadOnly}
            controller={attachmentsController}
            onInsertImage={(attachment) => {
              captureImageInsertionPoint();
              requestInsertImages([attachment]);
            }}
          />
          )}

          {/* Outgoing Internal Links Strip (Phase 9, Task 8) */}
          <NoteLinksBar links={parsedLinks} onActivateLink={handleActivateLink} />

          <div className="editor-body-divider" />

          {/* Note Body Content */}
          <div
            className="editor-body-wrap"
            onClick={viewMode === "edit" ? handleBodyWrapClick : undefined}
          >
            {viewMode === "edit" ? (
              <textarea
                ref={contentRef}
                className="editor-content-area selectable-text"
                placeholder={isReadOnly ? "No content in this note." : "Start writing your note..."}
                value={content}
                readOnly={isReadOnly}
                onChange={handleContentChange}
                onKeyDown={handleContentKeyDown}
                onSelect={handleRecordSelection}
                onKeyUp={handleRecordSelection}
                onMouseUp={handleRecordSelection}
                aria-label="Note content"
              />
            ) : (
              <RenderedNoteContent
                content={content}
                format={format}
                onActivateLink={handleActivateLink}
                placeholder={isReadOnly ? "No content in this note." : "No content to preview."}
              />
            )}
          </div>

          {/* Incoming Links / Backlinks (Phase 9, Task 14 & 15) */}
          <BacklinksSection
            noteId={noteId}
            onActivateLink={handleActivateLink}
            isReadOnly={isReadOnly}
            refreshTrigger={lastSavedAt}
          />
        </div>
      </div>
      <ScrollToTopButton scrollRef={scrollAreaRef} />
      </div>
      {showMinimap && (
        <NoteMinimap scrollRef={scrollAreaRef} content={content} viewMode={viewMode} />
      )}
      </div>

      {/* Note Status Bar */}
      <footer className="editor-status-bar">
        <div className="status-item">
          <span>Words: {wordCount}</span>
          <span className="status-separator">•</span>
          <span>Characters: {content.length}</span>
        </div>

        <div
          className="status-item"
          role={editorStatus === "error" ? "button" : undefined}
          tabIndex={editorStatus === "error" ? 0 : undefined}
          aria-label={editorStatus === "error" ? "Save failed, activate to retry saving" : undefined}
          onClick={editorStatus === "error" ? () => handleSave() : undefined}
          onKeyDown={
            editorStatus === "error"
              ? (e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    handleSave();
                  }
                }
              : undefined
          }
          style={{ cursor: editorStatus === "error" ? "pointer" : "default" }}
        >
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

      {/* Internal Note Picker Modal (Phase 9, Task 6 & 7) */}
      <NotePickerModal
        isOpen={isLinkPickerOpen}
        onClose={handleCloseLinkPicker}
        onSelectNote={handleSelectLinkTarget}
        currentNoteId={noteId}
        initialLabel={linkPickerInitialLabel}
      />

      {/* Export Note Modal (Phase 10, Task 14) */}
      {loadedNote && (
        <ExportNoteModal
          isOpen={isExportModalOpen}
          onClose={() => setIsExportModalOpen(false)}
          note={{
            id: loadedNote.id,
            title: title || loadedNote.title,
            content: content || loadedNote.content,
            format: (format || loadedNote.format) as string,
          }}
          onFlushSave={async () => {
            if (loadedNote && isDirtyRef.current) {
              if (debounceTimerRef.current) {
                clearTimeout(debounceTimerRef.current);
                debounceTimerRef.current = null;
              }
              await executeSave(loadedNote.id, revisionRef.current);
            }
          }}
        />
      )}
    </div>
  );
};

// Re-export as EditorPlaceholder for backwards compatibility
export const EditorPlaceholder = NoteEditor;
export default NoteEditor;
