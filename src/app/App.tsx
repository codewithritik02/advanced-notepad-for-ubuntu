import { useState, useRef, useCallback, useEffect, useMemo } from "react";
import { MainLayout } from "../layouts/MainLayout";
import { TopBar } from "../components/TopBar/TopBar";
import { Sidebar, NavItemId } from "../components/Sidebar/Sidebar";
import { NotesList } from "../features/notes/components/NotesList/NotesList";
import { NoteListItem, NotesListStatus } from "../features/notes/types";
import { NoteEditor } from "../features/notes/components/Editor/NoteEditor";
import { SettingsModal } from "../features/settings/components/SettingsModal/SettingsModal";
import { useTheme } from "../hooks/useTheme";
import { useDisplaySettings } from "../hooks/useDisplaySettings";
import { markdownToPlainText } from "../utils/markdown";
import { useKeyboardShortcuts } from "../hooks/useKeyboardShortcuts";
import {
  storageService,
  Note,
  NoteFormat,
  ResolvedLinkTarget,
  DEFAULT_PAGE_SIZE,
} from "../services/storage";
import { logger } from "../utils/logger";
import {
  useNotebooks,
  useNotebookSelection,
  CreateNotebookDialog,
  CreateNotebookModalState,
  RenameNotebookDialog,
  DeleteNotebookDialog,
  MoveNoteDialog,
  Notebook,
  computeNotebookPath,
} from "../features/notebooks";
import {
  useTags,
  useNoteTags,
  CreateTagDialog,
  CreateTagModalState,
  RenameTagDialog,
  DeleteTagDialog,
  ManageTagsDialog,
  Tag,
} from "../features/tags";
import {
  useSearch,
  useSearchKeyboardNav,
  searchResultToNoteListItem,
  formatResultCount,
  scopeSearchResults,
  SearchErrorBoundary,
} from "../features/search";
import {
  PermanentDeleteDialog,
  PermanentDeleteTarget,
} from "../features/notes/components/PermanentDeleteDialog";
import { EmptyTrashDialog } from "../features/notes/components/EmptyTrashDialog";
import {
  ConfirmNoteActionDialog,
  type NoteConfirmTarget,
} from "../features/notes/components/ConfirmNoteActionDialog";
import { BrokenLinkDialog } from "../features/notes/components/Links";
import { NotePickerModal } from "../features/notes/components/NotePicker";
import { replaceInternalLinkTarget, removeInternalLink } from "../utils/linkParser";
import { useNoteNavigationHistory } from "../hooks/useNoteNavigationHistory";
import {
  ExportBatchModal,
  ImportPreviewModal,
  pickImportFiles,
} from "../features/importExport";
import {
  CreateBackupModal,
  BackupListModal,
  RestorePreviewModal,
} from "../features/backup";
import { backupStorage } from "../services/storage/backup";
import {
  BackupProgressEvent,
  ExportScope,
  ImportPreview,
  ImportRequest,
} from "../services/storage/types";
import { UnlockScreen } from "../features/security";
import { useAutoLock } from "../features/security";

interface NotePageContext {
  navId: NavItemId;
  notebookId: string | null;
  tagId: string | null;
}

function trashedItemToNote(item: Awaited<ReturnType<typeof storageService.notes.listTrashed>>[number]): Note {
  return {
    id: item.id,
    title: item.title,
    content: item.preview,
    format: "txt",
    notebook_id: item.notebookId,
    created_at: item.createdAt,
    modified_at: item.modifiedAt,
    is_favorite: item.isFavorite,
    is_pinned: false,
    is_deleted: true,
    deleted_at: item.deletedAt,
    attachment_count: item.attachmentCount,
  } as Note;
}

async function fetchNotePage(context: NotePageContext, offset: number): Promise<Note[]> {
  if (context.navId === "tags" && context.tagId) {
    return storageService.tags.getNotesForTag(
      context.tagId,
      300,
      DEFAULT_PAGE_SIZE,
      offset
    );
  }
  if (context.navId === "trash") {
    const items = await storageService.notes.listTrashed(DEFAULT_PAGE_SIZE, offset);
    return items.map(trashedItemToNote);
  }
  if (context.navId === "favorites") {
    return storageService.notes.listFavorites(300, DEFAULT_PAGE_SIZE, offset);
  }

  return storageService.notes.list({
    includeDeleted: false,
    notebookId: context.navId === "notebook" ? context.notebookId : undefined,
    unfiledOnly: context.navId === "unfiled",
    limit: DEFAULT_PAGE_SIZE,
    offset,
    contentLimit: 300,
  });
}

export function App() {
  const { theme, setTheme, toggleTheme } = useTheme();
  const display = useDisplaySettings();
  const [activeNavId, setActiveNavId] = useState<NavItemId>("all-notes");
  const editorSaveRef = useRef<(() => Promise<void> | void) | null>(null);

  const handleFlushPendingSave = useCallback(async () => {
    if (editorSaveRef.current) {
      await editorSaveRef.current();
    }
  }, []);

  const {
    query: searchQuery,
    setQuery: setSearchQuery,
    searchState,
    results: searchResults,
    isSearchActive,
    clearSearch,
    searchNow,
    cancelPendingDebounce,
    removeResult,
  } = useSearch({
    debounceMs: 200,
    limit: 50,
    onBeforeSearch: handleFlushPendingSave,
  });
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isCreateBackupOpen, setIsCreateBackupOpen] = useState(false);
  const [isBackupListOpen, setIsBackupListOpen] = useState(false);
  const [restorePreviewCandidate, setRestorePreviewCandidate] = useState<string | null>(null);
  const [isRestoringBackup, setIsRestoringBackup] = useState<boolean>(false);
  const [notes, setNotes] = useState<Note[]>([]);
  const [hasMoreNotes, setHasMoreNotes] = useState(false);
  const [isLoadingMoreNotes, setIsLoadingMoreNotes] = useState(false);
  const [selectedNoteId, setSelectedNoteId] = useState<string | null>(null);
  const navigationHistory = useNoteNavigationHistory(selectedNoteId);
  const [isStorageReady, setIsStorageReady] = useState(false);
  const [isAppLocked, setIsAppLocked] = useState<boolean>(false);
  const [isEncryptionEnabled, setIsEncryptionEnabled] = useState<boolean>(false);
  const [isInterruptedMigration, setIsInterruptedMigration] = useState<boolean>(false);
  const [status, setStatus] = useState<NotesListStatus>("loading");
  const [errorMessage, setErrorMessage] = useState("Unable to load notes.");
  const [noteCounts, setNoteCounts] = useState<Record<string, number>>({});
  const [tagCounts, setTagCounts] = useState<Record<string, number>>({});
  const [noteTagsMap, setNoteTagsMap] = useState<Record<string, string[]>>({});
  const [isManageTagsOpen, setIsManageTagsOpen] = useState(false);
  const [restoreFeedback, setRestoreFeedback] = useState<{
    type: "success" | "error" | "info" | "warning";
    message: string;
  } | null>(null);
  const [backupAlert, setBackupAlert] = useState<{
    type: "warning" | "error" | "critical" | "info";
    message: string;
    code?: string;
    title?: string;
  } | null>(null);
  const [dismissedBackupCodes, setDismissedBackupCodes] = useState<Set<string>>(new Set());
  const [restoreProgress, setRestoreProgress] = useState<BackupProgressEvent | null>(null);
  const [restoreOperationId, setRestoreOperationId] = useState<string | null>(null);
  const [isCancellingRestore, setIsCancellingRestore] = useState<boolean>(false);

  const handleCancelRestore = useCallback(async () => {
    if (restoreOperationId) {
      setIsCancellingRestore(true);
      try {
        await backupStorage.cancelRestore(restoreOperationId);
      } catch (err) {
        logger.error("Failed to cancel restore", err);
      }
    }
  }, [restoreOperationId]);
  const [linkNavigationTarget, setLinkNavigationTarget] = useState<ResolvedLinkTarget | null>(null);
  const [isResolvingLinkTarget, setIsResolvingLinkTarget] = useState<boolean>(false);
  const [repairingBrokenTargetId, setRepairingBrokenTargetId] = useState<string | null>(null);
  const [noteRefreshKey, setNoteRefreshKey] = useState<number>(0);
  const [exportBatchState, setExportBatchState] = useState<{
    isOpen: boolean;
    scope: ExportScope;
    notebookName?: string;
  } | null>(null);

  const handleLockApp = useCallback(async () => {
    // 1. Flush pending note edits to persistent encrypted store BEFORE purging keys (Task 13)
    try {
      await handleFlushPendingSave();
    } catch (saveErr) {
      logger.warn("Failed to flush pending save before locking:", { error: String(saveErr) });
    }

    // 2. Call backend lock command to purge DEK from memory and remove decrypted staging files
    try {
      await storageService.crypto.lockApplication();
    } catch (err) {
      logger.warn("Failed to lock backend session:", { error: String(err) });
    }

    // 3. Cancel debounced search and wipe active search queries/results
    cancelPendingDebounce();
    clearSearch();

    // 4. Close any open dialogs/modals
    setIsSettingsOpen(false);
    setIsCreateBackupOpen(false);
    setIsBackupListOpen(false);
    setIsManageTagsOpen(false);

    // 5. Clear sensitive in-memory note selection and cached notes
    setSelectedNoteId(null);
    setNotes([]);

    // 6. Transition to dedicated UnlockScreen
    setIsAppLocked(true);
  }, [handleFlushPendingSave, cancelPendingDebounce, clearSearch]);

  const {
    settings: autoLockSettings,
    updateSettings: handleUpdateAutoLockSettings,
  } = useAutoLock({
    isEncryptionEnabled,
    isAppLocked,
    onLock: handleLockApp,
  });

  const [importModalState, setImportModalState] = useState<{
    isOpen: boolean;
    preview: ImportPreview | null;
    isLoading: boolean;
  }>({
    isOpen: false,
    preview: null,
    isLoading: false,
  });

  const handleOpenExportNotebook = useCallback((notebook: Notebook) => {
    setExportBatchState({
      isOpen: true,
      scope: {
        type: "notebook",
        notebook_id: notebook.id,
        include_nested: true,
      },
      notebookName: notebook.name,
    });
  }, []);

  const handleOpenImport = useCallback(async () => {
    try {
      const pickerResult = await pickImportFiles();
      if (pickerResult.cancelled || pickerResult.paths.length === 0) {
        return;
      }
      setImportModalState({
        isOpen: true,
        preview: null,
        isLoading: true,
      });
      const preview = await storageService.importExport.scanImportFiles(pickerResult.paths);
      setImportModalState({
        isOpen: true,
        preview,
        isLoading: false,
      });
    } catch (err) {
      logger.error("Failed to scan import files:", err);
      setRestoreFeedback({
        type: "error",
        message: "Failed to read selected files for import.",
      });
      setImportModalState({
        isOpen: false,
        preview: null,
        isLoading: false,
      });
    }
  }, []);

  useEffect(() => {
    if (restoreFeedback) {
      const timer = setTimeout(() => {
        setRestoreFeedback(null);
      }, 4000);
      return () => clearTimeout(timer);
    }
  }, [restoreFeedback]);

  useEffect(() => {
    if (backupAlert) {
      const timer = setTimeout(() => {
        if (backupAlert.code) {
          setDismissedBackupCodes((prev) => new Set(prev).add(backupAlert.code!));
        }
        setBackupAlert(null);
      }, 6000);
      return () => clearTimeout(timer);
    }
  }, [backupAlert]);

  // Real SQLite tags managed through dedicated feature hook
  const {
    tags,
    status: _tagsStatus,
    error: _tagsError,
    loadTags,
    createTag,
    renameTag,
    deleteTag,
  } = useTags(isStorageReady);

  // Tags associated with the currently selected note (Task 16, 38)
  const {
    noteTags,
    loadNoteTags,
    addTag: addTagToSelectedNote,
    removeTag: removeTagFromSelectedNote,
  } = useNoteTags(selectedNoteId);

  // Rename Tag Modal State
  const [renamingTag, setRenamingTag] = useState<Tag | null>(null);

  // Delete Tag Modal State
  const [deletingTag, setDeletingTag] = useState<Tag | null>(null);

  // Create Tag Modal State
  const [createTagModal, setCreateTagModal] = useState<CreateTagModalState>({
    isOpen: false,
  });

  const handleOpenCreateTag = useCallback(() => {
    setCreateTagModal({ isOpen: true });
  }, []);

  const handleCreateTag = useCallback(
    async (name: string) => {
      await createTag(name);
    },
    [createTag]
  );

  // Real SQLite notebooks managed through dedicated feature hook
  const {
    notebooks,
    status: notebooksStatus,
    error: notebooksError,
    loadNotebooks,
    createNotebook,
    renameNotebook,
    deleteNotebook,
  } = useNotebooks(isStorageReady);

  // Notebook tree expansion and navigation state (Task 9 & 10)
  const {
    location,
    selectedNotebookId,
    selectedTagId,
    selectNotebook,
    selectAllNotes,
    selectUnfiled,
    selectFavorites,
    selectTag,
    selectTrash,
    expandedNotebookIds,
    toggleExpand: toggleExpandNotebook,
    expandNotebook,
  } = useNotebookSelection();

  // Create Notebook Modal State (Task 13)
  const [createModalState, setCreateModalState] = useState<CreateNotebookModalState>({
    isOpen: false,
    parentId: null,
    parentName: null,
  });

  // Rename Notebook Modal State (Task 14)
  const [renamingNotebook, setRenamingNotebook] = useState<Notebook | null>(null);

  // Delete Notebook Modal State (Task 15)
  const [deletingNotebook, setDeletingNotebook] = useState<Notebook | null>(null);

  // Move Note Modal State (Task 16)
  const [movingNote, setMovingNote] = useState<Note | null>(null);

  // Permanent Delete Modal State (Phase 8, Task 16)
  const [noteActionConfirm, setNoteActionConfirm] = useState<NoteConfirmTarget | null>(null);
  const [permanentDeleteTarget, setPermanentDeleteTarget] =
    useState<PermanentDeleteTarget | null>(null);

  // Empty Trash Modal State (Phase 8, Task 17)
  const [isEmptyTrashOpen, setIsEmptyTrashOpen] = useState(false);
  const [trashFeedback, setTrashFeedback] = useState<{
    type: "success" | "error" | "warning";
    message: string;
    cleanupNoteIds?: string[];
  } | null>(null);

  useEffect(() => {
    if (trashFeedback && !trashFeedback.cleanupNoteIds?.length) {
      const timer = setTimeout(() => {
        setTrashFeedback(null);
      }, 5000);
      return () => clearTimeout(timer);
    }
  }, [trashFeedback]);

  const deletingNotebookHasChildren = useMemo(() => {
    if (!deletingNotebook) return false;
    return notebooks.some((nb) => nb.parent_id === deletingNotebook.id);
  }, [deletingNotebook, notebooks]);

  const searchInputRef = useRef<HTMLInputElement>(null);
  const activeLoadRequestIdRef = useRef<number>(0);
  const notePageContextRef = useRef<NotePageContext>({
    navId: "all-notes",
    notebookId: null,
    tagId: null,
  });
  const activeLinkNavRequestIdRef = useRef<number>(0);
  const inFlightMutationsRef = useRef<Set<string>>(new Set());

  // Check and verify storage readiness at startup
  const initStorage = useCallback(async () => {
    setStatus("loading");
    try {
      const info = await storageService.getInfo();
      if (info.is_initialized) {
        setIsStorageReady(true);
        try {
          const encStatus = await storageService.crypto.getEncryptionStatus();
          setIsEncryptionEnabled(encStatus.enabled);
          setIsInterruptedMigration(Boolean(encStatus.is_interrupted));
          if ((encStatus.enabled && encStatus.is_locked) || encStatus.is_interrupted) {
            setIsAppLocked(true);
            setNotes([]);
            setSelectedNoteId(null);
          } else {
            setIsAppLocked(false);
          }
        } catch (encErr) {
          logger.warn("Could not retrieve encryption status:", { error: String(encErr) });
          setIsEncryptionEnabled(false);
          setIsInterruptedMigration(false);
          setIsAppLocked(false);
        }
      } else {
        setErrorMessage(
          "Unable to initialize local storage. Please check disk permissions."
        );
        setStatus("error");
      }
    } catch (err) {
      setErrorMessage(
        "Failed to connect to local database: " +
          (err instanceof Error ? err.message : String(err))
      );
      setStatus("error");
    }
  }, []);

  useEffect(() => {
    initStorage();
  }, [initStorage]);

  // Automatic backup lifecycle: evaluate on startup and on routine interval (Phase 11, Task 11 & Task 22)
  useEffect(() => {
    if (!isStorageReady || isAppLocked) return;

    const evaluateBackupHealthAndRun = async (isStartup: boolean) => {
      try {
        await backupStorage.checkAndRunAutomaticBackup(isStartup);
      } catch (err: unknown) {
        const errorMsg =
          err instanceof Error
            ? err.message
            : typeof err === "string"
            ? err
            : "Automatic backup failed.";
        logger.warn("Automatic backup check failed:", { error: errorMsg });
        setBackupAlert({
          type: "error",
          title: "Automatic Backup Failed",
          message: errorMsg,
          code: "AUTOMATIC_BACKUP_EXECUTION_FAILED",
        });
      }

      // Check overall backup health and notify about discoverable issues without repetitive spam
      try {
        const health = await backupStorage.getHealthStatus();
        if (health.warnings.length > 0) {
          // Find first un-dismissed warning that is warning or critical
          const actionableWarn = health.warnings.find(
            (w) =>
              (w.level === "warning" || w.level === "critical") &&
              !dismissedBackupCodes.has(w.code)
          );
          if (actionableWarn) {
            setBackupAlert({
              type: actionableWarn.level === "critical" ? "error" : "warning",
              title: "Backup Warning",
              message: `${actionableWarn.message} ${actionableWarn.detail || ""}`.trim(),
              code: actionableWarn.code,
            });
          }
        }
      } catch (healthErr) {
        logger.warn("Failed to check backup health status:", { error: String(healthErr) });
      }
    };

    // 1. Startup check: evaluates if backup is due on launch (honoring rapid restart protection)
    evaluateBackupHealthAndRun(true);

    // 2. Periodic background interval check every 60 seconds while application remains open
    const intervalId = window.setInterval(() => {
      evaluateBackupHealthAndRun(false);
    }, 60_000);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [isStorageReady, isAppLocked, dismissedBackupCodes]);

  // Load global notebook, section, and tag note counts
  const refreshNoteCounts = useCallback(async () => {
    if (!isStorageReady || isAppLocked) return;
    try {
      const noteCounts = await storageService.notes.getNoteCounts();
      const counts: Record<string, number> = {
        "all-notes": noteCounts.total,
        "favorites": noteCounts.favorites,
        "unfiled": noteCounts.unfiled,
        "trash": noteCounts.trash,
        ...noteCounts.byNotebook,
      };

      setNoteCounts(counts);

      const countsByTag = await storageService.tags.getTagNoteCounts();
      setTagCounts(countsByTag);

      const tagsMap = await storageService.tags.getAllNotesTags();
      setNoteTagsMap(tagsMap);
    } catch {
      // Non-fatal for badge counters
    }
  }, [isStorageReady]);

  const handleRenameTag = useCallback(
    async (id: string, newName: string) => {
      await renameTag(id, newName);
      await refreshNoteCounts();
      if (selectedNoteId) {
        await loadNoteTags();
      }
    },
    [renameTag, refreshNoteCounts, selectedNoteId, loadNoteTags]
  );

  const handleDeleteTag = useCallback(
    async (id: string) => {
      if (inFlightMutationsRef.current.has(id)) return;
      inFlightMutationsRef.current.add(id);
      try {
        await deleteTag(id);
        if (selectedTagId === id) {
          selectAllNotes();
          setActiveNavId("all-notes");
        }
        await refreshNoteCounts();
        if (selectedNoteId) {
          await loadNoteTags();
        }
      } finally {
        inFlightMutationsRef.current.delete(id);
      }
    },
    [deleteTag, selectedTagId, selectAllNotes, refreshNoteCounts, selectedNoteId, loadNoteTags]
  );

  // Load persistent notes from SQLite
  const loadNotes = useCallback(
    async (
      overrideNavId?: NavItemId,
      overrideNotebookId?: string | null,
      overrideTagId?: string | null
    ) => {
      if (!isStorageReady || isAppLocked) return;
      const requestId = ++activeLoadRequestIdRef.current;
      setStatus("loading");
      setIsLoadingMoreNotes(false);
      setHasMoreNotes(false);
      try {
        const effectiveNav = overrideNavId !== undefined ? overrideNavId : activeNavId;
        const effectiveNotebook =
          overrideNotebookId !== undefined ? overrideNotebookId : selectedNotebookId;
        const effectiveTag =
          overrideTagId !== undefined ? overrideTagId : selectedTagId;

        const context: NotePageContext = {
          navId: effectiveNav,
          notebookId: effectiveNotebook,
          tagId: effectiveTag,
        };
        const fetchedNotes =
          effectiveNav === "tags" && !effectiveTag
            ? []
            : await fetchNotePage(context, 0);

        // Stale response protection (Task 21: Discard response if newer load started or user navigated away)
        if (activeLoadRequestIdRef.current !== requestId) return;

        setNotes(fetchedNotes);
        notePageContextRef.current = context;
        setHasMoreNotes(fetchedNotes.length === DEFAULT_PAGE_SIZE);

        if (fetchedNotes.length > 0) {
          setSelectedNoteId((prev) =>
            prev && fetchedNotes.some((n) => n.id === prev) ? prev : fetchedNotes[0].id
          );
        } else {
          setSelectedNoteId(null);
        }
        setStatus(fetchedNotes.length === 0 ? "empty" : "idle");
      } catch (err) {
        if (activeLoadRequestIdRef.current !== requestId) return;
        setErrorMessage(
          "Unable to load notes: " +
            (err instanceof Error ? err.message : String(err))
        );
        setStatus("error");
      }
    },
    [activeNavId, selectedNotebookId, selectedTagId, isStorageReady, isAppLocked]
  );

  const loadMoreNotes = useCallback(async () => {
    if (isLoadingMoreNotes || !hasMoreNotes || isSearchActive) return;
    const requestId = activeLoadRequestIdRef.current;
    const context = notePageContextRef.current;
    const offset = notes.length;
    setIsLoadingMoreNotes(true);
    try {
      const page = await fetchNotePage(context, offset);
      if (activeLoadRequestIdRef.current !== requestId) return;
      setNotes((current) => {
        const existingIds = new Set(current.map((note) => note.id));
        return [...current, ...page.filter((note) => !existingIds.has(note.id))];
      });
      setHasMoreNotes(page.length === DEFAULT_PAGE_SIZE);
    } catch (err) {
      if (activeLoadRequestIdRef.current === requestId) {
        setErrorMessage(
          "Unable to load more notes: " +
            (err instanceof Error ? err.message : String(err))
        );
      }
    } finally {
      setIsLoadingMoreNotes(false);
    }
  }, [hasMoreNotes, isLoadingMoreNotes, isSearchActive, notes.length]);

  useEffect(() => {
    if (isStorageReady && !isAppLocked) {
      loadNotes();
      refreshNoteCounts();
    }
  }, [isStorageReady, isAppLocked, loadNotes, refreshNoteCounts]);

  const handleConfirmImport = useCallback(
    async (request: ImportRequest) => {
      try {
        const summary = await storageService.importExport.executeImport(request);
        // Immediately refresh note views, notebooks, tags, and counts
        await loadNotes(activeNavId, selectedNotebookId, selectedTagId);
        await loadNotebooks();
        await loadTags();
        await refreshNoteCounts();

        const successCount = summary.importedCount || 0;
        if (successCount > 0) {
          setRestoreFeedback({
            type: "success",
            message: `Successfully imported ${successCount} note${successCount === 1 ? "" : "s"}.`,
          });
        }
        return summary;
      } catch (err) {
        logger.error("Failed to execute import:", err);
        setRestoreFeedback({
          type: "error",
          message: "An error occurred while importing notes.",
        });
        throw err;
      }
    },
    [
      activeNavId,
      selectedNotebookId,
      selectedTagId,
      loadNotes,
      loadNotebooks,
      loadTags,
      refreshNoteCounts,
    ]
  );

  const isCreatingRef = useRef(false);

  // Persistent note creation (Task 18: Assign to selected notebook or unfiled; auto-tag if in tag view)
  const handleNewNoteAction = useCallback(async () => {
    if (isCreatingRef.current) return;
    isCreatingRef.current = true;

    try {
      // Flush in-flight/dirty edits from currently selected note before switching to the newly created note
      if (editorSaveRef.current) {
        try {
          await editorSaveRef.current();
        } catch (saveErr) {
          logger.error("AutoFlushOnNewNote", saveErr);
        }
      }

      // If user was viewing trash or has an active search filter, reset to all-notes to reveal the new note
      if (activeNavId === "trash") {
        setActiveNavId("all-notes");
        selectAllNotes();
      }
      if (searchQuery) {
        clearSearch();
      }

      const targetNotebookId =
        activeNavId === "notebook" && selectedNotebookId
          ? selectedNotebookId
          : undefined;

      const created = await storageService.notes.create({
        title: "Untitled Note",
        content: "",
        format: "txt",
        notebook_id: targetNotebookId,
      });

      if (activeNavId === "tags" && selectedTagId) {
        try {
          await storageService.tags.addToNote(created.id, selectedTagId);
        } catch (e) {
          logger.error("AutoTagNote", e);
        }
      }

      if (activeNavId === "favorites") {
        try {
          await storageService.notes.setFavorite(created.id, true);
          created.is_favorite = true;
        } catch (e) {
          logger.error("AutoFavoriteNote", e);
        }
      }

      setNotes((prev) => [created, ...prev]);
      setSelectedNoteId(created.id);
      setStatus("idle");
      refreshNoteCounts();
    } catch (err) {
      setErrorMessage(
        "Failed to create note: " +
          (err instanceof Error ? err.message : String(err))
      );
      setStatus("error");
    } finally {
      isCreatingRef.current = false;
    }
  }, [activeNavId, selectedNotebookId, searchQuery]);

  // Persistent note update
  const handleSaveNote = useCallback(
    async (id: string, title: string, content: string, format?: NoteFormat) => {
      // Do not allow ordinary active-note editing controls to silently mutate trashed notes (Task 12, Task 19)
      if (activeNavId === "trash") {
        return;
      }
      try {
        const existing = await storageService.notes.get(id);
        if (existing?.is_deleted) {
          return;
        }
        const updated = await storageService.notes.update(id, {
          title,
          content,
          format: format || (existing?.format as NoteFormat) || "txt",
        });
        setNotes((prev) => prev.map((n) => (n.id === id ? updated : n)));
      } catch (err) {
        setErrorMessage(
          "Failed to save note: " +
            (err instanceof Error ? err.message : String(err))
        );
        throw err;
      }
    },
    [activeNavId]
  );

  const handleRestoreNote = useCallback(
    async (noteId: string) => {
      if (inFlightMutationsRef.current.has(noteId)) return;
      inFlightMutationsRef.current.add(noteId);
      try {
        const restoredNote = await storageService.notes.restoreNote(noteId);
        if (selectedNoteId === noteId) {
          setSelectedNoteId(null);
        }
        await loadNotes("trash", null, null);
        await refreshNoteCounts();
        setRestoreFeedback({
          type: "success",
          message: `Restored "${restoredNote.title?.trim() || "Untitled Note"}"`,
        });
      } catch (err) {
        await loadNotes("trash", null, null);
        await refreshNoteCounts();
        const msg =
          "Failed to restore note: " +
          (err instanceof Error ? err.message : String(err));
        setErrorMessage(msg);
        setRestoreFeedback({
          type: "error",
          message: msg,
        });
      } finally {
        inFlightMutationsRef.current.delete(noteId);
      }
    },
    [selectedNoteId, loadNotes, refreshNoteCounts]
  );

  const handlePermanentlyDeleteNote = useCallback(
    (noteId: string) => {
      const note = notes.find((n) => n.id === noteId);
      const title = note?.title || "Untitled Note";
      setPermanentDeleteTarget({ id: noteId, title });
    },
    [notes]
  );

  const handleConfirmPermanentDelete = useCallback(
    async (noteId: string) => {
      if (inFlightMutationsRef.current.has(noteId)) return;
      inFlightMutationsRef.current.add(noteId);
      try {
        // If the selected note is no longer in Trash, reject the operation and refresh the UI
        const currentNote = await storageService.notes.get(noteId);
        if (!currentNote?.is_deleted) {
          await loadNotes("trash", null, null);
          await refreshNoteCounts();
          throw new Error("This note is no longer in the Trash.");
        }

        const result = await storageService.notes.permanentlyDelete(noteId);
        if (selectedNoteId === noteId) {
          setSelectedNoteId(null);
        }
        navigationHistory.removeNoteFromHistory(noteId);
        await loadNotes("trash", null, null);
        await refreshNoteCounts();
        if (result.attachmentCleanupFailed) {
          setTrashFeedback({
            type: "warning",
            message:
              "The note was deleted, but its attachment files could not be removed. You can retry cleanup safely.",
            cleanupNoteIds: [noteId],
          });
          logger.warn(
            "Note permanently deleted but some attachments could not be cleaned up",
            { error: result.errorMessage ?? "Unknown cleanup error" }
          );
        }
      } catch (err) {
        const msg =
          "Failed to permanently delete note: " +
          (err instanceof Error ? err.message : String(err));
        setErrorMessage(msg);
        throw err;
      } finally {
        inFlightMutationsRef.current.delete(noteId);
      }
    },
    [loadNotes, refreshNoteCounts, navigationHistory, selectedNoteId]
  );

  const handleOpenEmptyTrash = useCallback(() => {
    setIsEmptyTrashOpen(true);
  }, []);

  const handleConfirmEmptyTrash = useCallback(async () => {
    if (inFlightMutationsRef.current.has("__empty_trash__")) return;
    inFlightMutationsRef.current.add("__empty_trash__");
    try {
      const result = await storageService.notes.emptyTrash();

      if (activeNavId === "trash") {
        setSelectedNoteId(null);
      }
      navigationHistory.clearHistory();

      await loadNotes("trash", null, null);
      await refreshNoteCounts();

      if (result.failedCount === 0 && result.attachmentCleanupFailures.length === 0) {
        if (result.deletedCount === 0) {
          setTrashFeedback({
            type: "success",
            message: "Trash is already empty.",
          });
        } else {
          setTrashFeedback({
            type: "success",
            message: `Emptied Trash: ${result.deletedCount} ${
              result.deletedCount === 1 ? "note" : "notes"
            } permanently deleted.`,
          });
        }
      } else if (result.failedCount > 0 && result.deletedCount > 0) {
        setTrashFeedback({
          type: "warning",
          message: `${result.deletedCount} notes deleted, but ${result.failedCount} notes could not be deleted.`,
        });
      } else if (result.failedCount > 0 && result.deletedCount === 0) {
        const errorDetail = result.errors.length > 0 ? `: ${result.errors[0]}` : "";
        setTrashFeedback({
          type: "error",
          message: `Failed to empty trash${errorDetail}`,
        });
      } else if (result.attachmentCleanupFailures.length > 0) {
        setTrashFeedback({
          type: "warning",
          message: `${result.deletedCount} notes deleted, but some attachments could not be cleaned up.`,
          cleanupNoteIds: result.attachmentCleanupFailures,
        });
        logger.warn("Empty trash completed with attachment cleanup issues", {
          failures: result.attachmentCleanupFailures,
          errors: result.errors,
        });
      }
    } catch (err) {
      const msg =
        "Failed to empty trash: " +
        (err instanceof Error ? err.message : String(err));
      setErrorMessage(msg);
      setTrashFeedback({
        type: "error",
        message: msg,
      });
      throw err;
    } finally {
      inFlightMutationsRef.current.delete("__empty_trash__");
    }
  }, [activeNavId, loadNotes, refreshNoteCounts]);

  const handleRetryAttachmentCleanup = useCallback(async () => {
    const noteIds = trashFeedback?.cleanupNoteIds;
    if (!noteIds?.length) return;

    let results = await Promise.allSettled(
      noteIds.map((noteId) => storageService.attachments.retryNoteCleanup(noteId))
    );
    let failedIds = noteIds.filter((_, index) => results[index].status === "rejected");
    if (failedIds.length > 0) {
      try {
        await storageService.attachments.cleanupUnreferencedFiles();
      } catch (error) {
        logger.warn("Unreferenced attachment cleanup retry failed", {
          error: String(error),
        });
      }
      results = await Promise.allSettled(
        failedIds.map((noteId) => storageService.attachments.retryNoteCleanup(noteId))
      );
      failedIds = failedIds.filter((_, index) => results[index].status === "rejected");
    }
    if (failedIds.length === 0) {
      setTrashFeedback({
        type: "success",
        message: "Attachment cleanup completed successfully.",
      });
    } else {
      setTrashFeedback({
        type: "warning",
        message: `Cleanup still failed for ${failedIds.length} note${
          failedIds.length === 1 ? "" : "s"
        }. Check file permissions and retry.`,
        cleanupNoteIds: failedIds,
      });
    }
  }, [trashFeedback]);

  const handleDeleteNote = useCallback(
    async (noteId?: string) => {
      // 1. Capture the intended note ID
      const targetId = noteId || selectedNoteId;
      if (!targetId) return;

      // Ensure activeNavId is not trash (Trash uses permanent delete)
      if (activeNavId === "trash") return;

      if (inFlightMutationsRef.current.has(targetId)) return;
      inFlightMutationsRef.current.add(targetId);

      // 2. Ensure pending edits are saved or flushed according to existing autosave contract
      if (editorSaveRef.current && selectedNoteId === targetId) {
        try {
          await editorSaveRef.current();
        } catch {
          // Continue with deletion even if autosave encounters an issue
        }
      }

      if (selectedNoteId === targetId) {
        editorSaveRef.current = null;
      }

      try {
        // 3. Move the note to Trash via the soft-delete service
        await storageService.notes.softDelete(targetId);

        // 4. Remove it from the active note list
        setNotes((prev) => prev.filter((n) => n.id !== targetId));

        // When search is active, remove it immediately from search results to preserve consistency (Phase 13, Task 10)
        if (isSearchActive) {
          removeResult(targetId);
        }

        // 5. Update navigation and any Trash count
        await refreshNoteCounts();

        // 6. Clear or change the current selection safely
        if (selectedNoteId === targetId) {
          setSelectedNoteId(null);
        }
      } catch (err) {
        // If deletion fails, do not remove note from UI as though it were successfully deleted
        setErrorMessage(
          "Failed to delete note: " +
            (err instanceof Error ? err.message : String(err))
        );
      } finally {
        inFlightMutationsRef.current.delete(targetId);
      }
    },
    [selectedNoteId, activeNavId, refreshNoteCounts, isSearchActive, removeResult]
  );

  const handleFocusSearch = useCallback(async () => {
    if (editorSaveRef.current) {
      await editorSaveRef.current();
    }
    searchInputRef.current?.focus();
    searchInputRef.current?.select();
  }, []);

  const handleOpenSettings = useCallback(() => {
    setIsSettingsOpen((prev) => !prev);
  }, []);

  const handleEscape = useCallback(() => {
    if (isSettingsOpen) {
      setIsSettingsOpen(false);
    } else if (searchQuery || isSearchActive) {
      clearSearch();
      searchInputRef.current?.blur();
    } else {
      searchInputRef.current?.blur();
    }
  }, [isSettingsOpen, searchQuery, isSearchActive, clearSearch]);

  const handleRegisterSave = useCallback(
    (saveFn: (() => Promise<void> | void) | null) => {
      editorSaveRef.current = saveFn;
    },
    []
  );

  // Moving a note to the Trash and restoring it both ask for confirmation first; the actual
  // work stays in handleDeleteNote / handleRestoreNote and only runs after the user confirms.
  const requestDeleteNote = useCallback(
    (noteId?: string) => {
      const targetId = noteId || selectedNoteId;
      if (!targetId || activeNavId === "trash") return;
      if (inFlightMutationsRef.current.has(targetId)) return;
      const title =
        notes.find((n) => n.id === targetId)?.title ??
        searchResults.find((r) => r.noteId === targetId)?.title ??
        "";
      setNoteActionConfirm({ action: "trash", id: targetId, title });
    },
    [selectedNoteId, activeNavId, notes, searchResults]
  );

  const requestRestoreNote = useCallback(
    (noteId: string) => {
      if (inFlightMutationsRef.current.has(noteId)) return;
      const note = notes.find((n) => n.id === noteId);
      setNoteActionConfirm({ action: "restore", id: noteId, title: note?.title ?? "" });
    },
    [notes]
  );

  const handleConfirmNoteAction = useCallback(
    async (target: NoteConfirmTarget) => {
      if (target.action === "trash") {
        await handleDeleteNote(target.id);
      } else {
        await handleRestoreNote(target.id);
      }
    },
    [handleDeleteNote, handleRestoreNote]
  );

  useKeyboardShortcuts({
    enabled: !isAppLocked,
    onNewNote: handleNewNoteAction,
    onFocusSearch: handleFocusSearch,
    onOpenSettings: handleOpenSettings,
    onZoomIn: display.zoomIn,
    onZoomOut: display.zoomOut,
    onZoomReset: display.resetZoom,
    onEscape: handleEscape,
    onSaveNote: () => {
      editorSaveRef.current?.();
    },
    onDeleteNote: () => {
      if (selectedNoteId && activeNavId !== "trash") {
        requestDeleteNote(selectedNoteId);
      }
    },
    onNavigateBack: () => {
      handleNavigateBack();
    },
    onNavigateForward: () => {
      handleNavigateForward();
    },
    onLockApp: () => {
      if (isEncryptionEnabled && !isAppLocked) {
        handleLockApp();
      }
    },
  });

  /**
   * Task 45 — Search Result Keyboard Navigation
   *
   * Stable callback used by both the JSX onSelectNote handler and the
   * keyboard navigation hook so that clicking a card and pressing ↓/↑
   * produce identical save → select → blur behavior.
   */
  const handleKeyboardSelectNote = useCallback(
    async (noteId: string) => {
      cancelPendingDebounce();
      if (noteId !== selectedNoteId) {
        if (editorSaveRef.current) {
          await editorSaveRef.current();
        }
        setSelectedNoteId(noteId);
      }
      // Blur search input so focus moves to the editor (Task 44 / Task 45)
      if (isSearchActive) {
        searchInputRef.current?.blur();
      }
    },
    [cancelPendingDebounce, selectedNoteId, isSearchActive]
  );

  /**
   * Phase 9, Tasks 10 & 11 — Safe Internal-Link Activation & Unsaved Edits Protection
   *
   * Flushes pending debounced saves before navigating away, blocks navigation if save fails
   * to preserve in-memory user changes, guards against rapid click race conditions with
   * sequence IDs, and resolves target notes by stable UUID.
   */
  const handleActivateInternalLink = useCallback(
    async (targetNoteId: string) => {
      cancelPendingDebounce();
      const navRequestId = ++activeLinkNavRequestIdRef.current;
      const sourceNoteId = selectedNoteId;
      setIsResolvingLinkTarget(true);

      try {
        // 1. Flush pending debounced edits of source note BEFORE navigating away
        if (editorSaveRef.current) {
          try {
            await editorSaveRef.current();
          } catch (saveErr) {
            const msg =
              "Unable to save current note before navigating. Your edits have been kept safe in the editor: " +
              (saveErr instanceof Error ? saveErr.message : String(saveErr));
            setErrorMessage(msg);
            logger.error("Internal link navigation aborted due to save failure", saveErr);
            // Abort navigation to protect in-memory user changes
            return;
          }
        }

        // Stale navigation protection: check if a newer link activation started while saving
        if (activeLinkNavRequestIdRef.current !== navRequestId) return;

        // Check if note changed during asynchronous save
        if (selectedNoteId !== sourceNoteId) return;

        // If already on target note and active, avoid redundant navigation
        if (targetNoteId === selectedNoteId && activeNavId !== "trash") return;

        // 2. Resolve target note by stable ID
        let resolved: ResolvedLinkTarget;
        try {
          resolved = await storageService.links.resolveTarget(targetNoteId);
        } catch (resolveErr) {
          logger.error("Failed to resolve internal link target", resolveErr);
          if (activeLinkNavRequestIdRef.current !== navRequestId) return;
          setLinkNavigationTarget({
            targetId: targetNoteId,
            title: null,
            lifecycle: "missing",
            notebookId: null,
            notebookName: null,
          });
          return;
        }

        if (activeLinkNavRequestIdRef.current !== navRequestId) return;

        if (resolved.lifecycle === "active") {
          // If viewing trash, switch to all-notes so the active target is visible
          if (activeNavId === "trash") {
            selectAllNotes();
            setActiveNavId("all-notes");
            await loadNotes("all-notes", null, null);
          }

          if (activeLinkNavRequestIdRef.current !== navRequestId) return;

          // 3. Navigate safely to target note
          setSelectedNoteId(targetNoteId);
          navigationHistory.recordNavigation(targetNoteId, sourceNoteId);

          if (isSearchActive) {
            searchInputRef.current?.blur();
          }
        } else {
          // Trashed or Missing target
          setLinkNavigationTarget(resolved);
        }
      } catch (err) {
        logger.error("Unexpected error during internal link activation", err);
        setErrorMessage("Unable to open linked note. An unexpected error occurred.");
      } finally {
        if (activeLinkNavRequestIdRef.current === navRequestId) {
          setIsResolvingLinkTarget(false);
        }
      }
    },
    [
      cancelPendingDebounce,
      selectedNoteId,
      navigationHistory,
      activeNavId,
      isSearchActive,
      selectAllNotes,
      loadNotes,
    ]
  );

  const handleConfirmViewInTrash = useCallback(async () => {
    if (!linkNavigationTarget) return;
    const targetId = linkNavigationTarget.targetId;
    setLinkNavigationTarget(null);

    // Flush pending edits of source note
    if (editorSaveRef.current) {
      try {
        await editorSaveRef.current();
      } catch (saveErr) {
        const msg =
          "Unable to save current note before navigating to Trash. Your edits have been kept safe in the editor: " +
          (saveErr instanceof Error ? saveErr.message : String(saveErr));
        setErrorMessage(msg);
        logger.error("Navigation to trash aborted due to save failure", saveErr);
        return;
      }
    }

    // Navigate to Trash view and select the trashed note
    selectTrash();
    setActiveNavId("trash");
    await loadNotes("trash", null, null);
    setSelectedNoteId(targetId);
  }, [linkNavigationTarget, selectTrash, loadNotes]);

  const handleCloseLinkNavigationDialog = useCallback(() => {
    setLinkNavigationTarget(null);
  }, []);

  /**
   * Phase 9, Task 17 — Restore Trashed Link Target and Open
   */
  const handleRestoreAndOpenTargetNote = useCallback(
    async (targetId: string) => {
      setLinkNavigationTarget(null);
      try {
        const restoredNote = await storageService.notes.restoreNote(targetId);
        selectAllNotes();
        setActiveNavId("all-notes");
        await loadNotes("all-notes", null, null);
        await refreshNoteCounts();
        setSelectedNoteId(targetId);
        setRestoreFeedback({
          type: "success",
          message: `Restored and opened "${restoredNote.title?.trim() || "Untitled Note"}"`,
        });
      } catch (err) {
        const msg =
          "Failed to restore note: " +
          (err instanceof Error ? err.message : String(err));
        setErrorMessage(msg);
        setRestoreFeedback({
          type: "error",
          message: msg,
        });
      }
    },
    [selectAllNotes, loadNotes, refreshNoteCounts]
  );

  /**
   * Phase 9, Task 16 — Broken Link Repair and Removal Handlers
   */
  const handleInitiateRepairBrokenLink = useCallback((targetId: string) => {
    setLinkNavigationTarget(null);
    setRepairingBrokenTargetId(targetId);
  }, []);

  const handleRemoveBrokenLink = useCallback(
    async (targetId: string) => {
      setLinkNavigationTarget(null);
      if (!selectedNoteId) return;

      try {
        const currentNote = await storageService.notes.get(selectedNoteId);
        if (currentNote) {
          const updatedContent = removeInternalLink(currentNote.content, targetId);
          await storageService.notes.update(selectedNoteId, { content: updatedContent });
          setNoteRefreshKey((k) => k + 1);
          await loadNotes(activeNavId, selectedNotebookId, selectedTagId);
        }
      } catch (err) {
        console.error("[BrokenLink] Failed to remove broken link:", err);
      }
    },
    [selectedNoteId, activeNavId, selectedNotebookId, selectedTagId, loadNotes]
  );

  const handleConfirmRepairReplacement = useCallback(
    async (replacement: { id: string; title: string; customLabel?: string }) => {
      const oldId = repairingBrokenTargetId;
      setRepairingBrokenTargetId(null);
      if (!selectedNoteId || !oldId) return;

      try {
        const currentNote = await storageService.notes.get(selectedNoteId);
        if (currentNote) {
          const updatedContent = replaceInternalLinkTarget(
            currentNote.content,
            oldId,
            replacement.id,
            replacement.customLabel || replacement.title
          );
          await storageService.notes.update(selectedNoteId, { content: updatedContent });
          setNoteRefreshKey((k) => k + 1);
          await loadNotes(activeNavId, selectedNotebookId, selectedTagId);
        }
      } catch (err) {
        console.error("[BrokenLink] Failed to replace broken link:", err);
      }
    },
    [selectedNoteId, repairingBrokenTargetId, activeNavId, selectedNotebookId, selectedTagId, loadNotes]
  );

  /**
   * Phase 9, Task 12 — Back Navigation History
   */
  const handleNavigateBack = useCallback(async () => {
    if (!navigationHistory.canGoBack) return;

    // 1. Flush pending debounced edits of current note
    if (editorSaveRef.current) {
      try {
        await editorSaveRef.current();
      } catch (saveErr) {
        setErrorMessage(
          "Unable to save current note before navigating: " +
            (saveErr instanceof Error ? saveErr.message : String(saveErr))
        );
        return;
      }
    }

    const previousId = navigationHistory.goBack();
    if (!previousId) return;

    try {
      const resolved = await storageService.links.resolveTarget(previousId);
      if (resolved.lifecycle === "active") {
        if (activeNavId === "trash") {
          selectAllNotes();
          setActiveNavId("all-notes");
          await loadNotes("all-notes", null, null);
        }
        setSelectedNoteId(previousId);
      } else {
        // Trashed or missing note
        setLinkNavigationTarget(resolved);
      }
    } catch {
      setLinkNavigationTarget({
        targetId: previousId,
        title: null,
        lifecycle: "missing",
        notebookId: null,
        notebookName: null,
      });
    }
  }, [navigationHistory, activeNavId, selectAllNotes, loadNotes]);

  /**
   * Phase 9, Task 12 — Forward Navigation History
   */
  const handleNavigateForward = useCallback(async () => {
    if (!navigationHistory.canGoForward) return;

    // 1. Flush pending debounced edits of current note
    if (editorSaveRef.current) {
      try {
        await editorSaveRef.current();
      } catch (saveErr) {
        setErrorMessage(
          "Unable to save current note before navigating: " +
            (saveErr instanceof Error ? saveErr.message : String(saveErr))
        );
        return;
      }
    }

    const nextId = navigationHistory.goForward();
    if (!nextId) return;

    try {
      const resolved = await storageService.links.resolveTarget(nextId);
      if (resolved.lifecycle === "active") {
        if (activeNavId === "trash") {
          selectAllNotes();
          setActiveNavId("all-notes");
          await loadNotes("all-notes", null, null);
        }
        setSelectedNoteId(nextId);
      } else {
        // Trashed or missing note
        setLinkNavigationTarget(resolved);
      }
    } catch {
      setLinkNavigationTarget({
        targetId: nextId,
        title: null,
        lifecycle: "missing",
        notebookId: null,
        notebookName: null,
      });
    }
  }, [navigationHistory, activeNavId, selectAllNotes, loadNotes]);

  // Wire arrow-key navigation over scoped search results (Task 45)
  // resultNoteIds is derived below after scopedSearchResults is computed;
  // we pass a stable ref-based wrapper to avoid dependency ordering issues.

  // Notebook creation modal handlers (Task 13)
  const handleOpenCreateNotebook = useCallback(
    (requestedParentId?: string | null) => {
      let targetParentId: string | null = null;
      let targetParentName: string | null = null;

      if (requestedParentId !== undefined) {
        targetParentId = requestedParentId;
      } else if (activeNavId === "notebook" && selectedNotebookId) {
        targetParentId = selectedNotebookId;
      }

      if (targetParentId) {
        const parentNb = notebooks.find((nb) => nb.id === targetParentId);
        targetParentName = parentNb ? parentNb.name : null;
      }

      setCreateModalState({
        isOpen: true,
        parentId: targetParentId,
        parentName: targetParentName,
      });
    },
    [activeNavId, selectedNotebookId, notebooks]
  );

  const handleCreateNotebook = useCallback(
    async (name: string, parentId?: string | null) => {
      const created = await createNotebook(name, parentId);
      if (parentId) {
        expandNotebook(parentId);
      }
      selectNotebook(created.id, [...notebooks, created]);
      setActiveNavId("notebook");
    },
    [createNotebook, expandNotebook, selectNotebook, notebooks]
  );

  const handleRenameNotebook = useCallback(
    async (id: string, newName: string) => {
      await renameNotebook(id, newName);
    },
    [renameNotebook]
  );

  const handleDeleteNotebook = useCallback(
    async (id: string) => {
      if (inFlightMutationsRef.current.has(id)) return;
      inFlightMutationsRef.current.add(id);
      try {
        await editorSaveRef.current?.();
        await deleteNotebook(id);
        if (selectedNotebookId === id) {
          selectUnfiled();
          setActiveNavId("unfiled");
          await loadNotes("unfiled", null);
        } else {
          await loadNotes();
        }
        await loadNotebooks();
        await refreshNoteCounts();
      } finally {
        inFlightMutationsRef.current.delete(id);
      }
    },
    [deleteNotebook, selectedNotebookId, selectUnfiled, loadNotes, loadNotebooks, refreshNoteCounts]
  );

  const handleMoveNote = useCallback(
    async (noteId: string, targetNotebookId: string | null) => {
      if (inFlightMutationsRef.current.has(noteId)) return;
      inFlightMutationsRef.current.add(noteId);
      try {
        await editorSaveRef.current?.();
        await storageService.notes.moveToNotebook(noteId, targetNotebookId);
        await loadNotes();
        await loadNotebooks();
        await refreshNoteCounts();
      } finally {
        inFlightMutationsRef.current.delete(noteId);
      }
    },
    [loadNotes, loadNotebooks, refreshNoteCounts]
  );

  const handleToggleNoteFavorite = useCallback(
    async (noteId: string) => {
      if (inFlightMutationsRef.current.has(noteId)) return;
      inFlightMutationsRef.current.add(noteId);
      const note = notes.find((n) => n.id === noteId);
      if (!note) {
        inFlightMutationsRef.current.delete(noteId);
        return;
      }
      const nextFavorite = !note.is_favorite;
      // Optimistic state update
      setNotes((prev) =>
        prev.map((n) => (n.id === noteId ? { ...n, is_favorite: nextFavorite } : n))
      );
      try {
        const updated = await storageService.notes.setFavorite(noteId, nextFavorite);
        setNotes((prev) =>
          prev.map((n) => (n.id === noteId ? updated : n))
        );
        refreshNoteCounts();
      } catch {
        // Rollback on failure
        setNotes((prev) =>
          prev.map((n) => (n.id === noteId ? { ...n, is_favorite: note.is_favorite } : n))
        );
      } finally {
        inFlightMutationsRef.current.delete(noteId);
      }
    },
    [notes, refreshNoteCounts]
  );

  // Filter notes based on active sidebar navigation section
  const filteredNotes = useMemo(() => {
    return notes.filter((n) => {
      if (activeNavId === "favorites" && !n.is_favorite) return false;
      if (activeNavId === "trash" && !n.is_deleted) return false;
      if (activeNavId !== "trash" && n.is_deleted) return false;
      if (activeNavId === "unfiled" && n.notebook_id !== null) return false;
      if (activeNavId === "notebook" && selectedNotebookId && n.notebook_id !== selectedNotebookId) return false;
      return true;
    });
  }, [notes, activeNavId, selectedNotebookId]);

  // Precompute notebook path map to avoid repeated O(N) tree traversals per note (Task 9)
  const notebookPathMap = useMemo(() => {
    const map = new Map<string, string>();
    for (const nb of notebooks) {
      const p = computeNotebookPath(nb.id, notebooks);
      if (p && p !== "Unfiled") {
        map.set(nb.id, p);
      }
    }
    return map;
  }, [notebooks]);

  // Convert SQLite domain Notes to UI NoteListItems
  const noteListItems = useMemo<NoteListItem[]>(() => {
    return filteredNotes.map((n) => {
      // For large notes (100KB-1MB), avoid regex replacement over the full body
      const snippet = n.content.length > 300 ? n.content.slice(0, 300) : n.content;
      // Markdown notes show their text, not their markup, in the list ("# Title **bold**" -> "Title bold")
      const readableSnippet = n.format === "md" ? markdownToPlainText(snippet) : snippet;
      const cleanContent = readableSnippet.replace(/\s+/g, " ").trim();
      const preview =
        cleanContent.length > 0
          ? cleanContent.length > 120
            ? cleanContent.slice(0, 120) + "…"
            : cleanContent
          : "No content";

      let updatedAt = "Just now";
      if (n.modified_at) {
        try {
          const date = new Date(n.modified_at);
          if (!isNaN(date.getTime())) {
            updatedAt = date.toLocaleDateString(undefined, {
              month: "short",
              day: "numeric",
            });
          }
        } catch {
          // Graceful fallback to default
        }
      }

      let deletedAtDisplay: string | undefined = undefined;
      if (n.deleted_at) {
        try {
          const dDate = new Date(n.deleted_at);
          if (!isNaN(dDate.getTime())) {
            deletedAtDisplay = dDate.toLocaleDateString(undefined, {
              month: "short",
              day: "numeric",
              hour: "2-digit",
              minute: "2-digit",
            });
          }
        } catch {
          // Graceful fallback
        }
      }

      const notebookPath = n.notebook_id ? notebookPathMap.get(n.notebook_id) : undefined;

      return {
        id: n.id,
        title: n.title.trim() ? n.title : "Untitled Note",
        preview,
        updatedAt,
        isFavorite: n.is_favorite,
        notebookId: n.notebook_id ?? undefined,
        notebookPath,
        tags: noteTagsMap[n.id] ?? [],
        attachmentCount: (n as any).attachment_count,
        isDeleted: n.is_deleted,
        deletedAt: deletedAtDisplay ?? n.deleted_at ?? undefined,
      };
    });
  }, [filteredNotes, noteTagsMap, notebookPathMap]);

  const notesLookup = useMemo(() => {
    const map = new Map<string, { is_favorite: boolean; notebook_id?: string | null; title?: string }>();
    for (const n of notes) {
      map.set(n.id, { is_favorite: n.is_favorite, notebook_id: n.notebook_id, title: n.title });
    }
    return map;
  }, [notes]);

  // Scoped search results (Task 39: Favorites, Task 40: Tags, Task 41: Notebook, Task 42: NoteLocation model)
  const scopedSearchResults = useMemo(() => {
    if (!isSearchActive) return [];
    return scopeSearchResults(searchResults, {
      location,
      tags,
      noteTagsMap,
      notesLookup,
    });
  }, [isSearchActive, searchResults, location, tags, noteTagsMap, notesLookup]);

  // Task 45: Ordered IDs of the current scoped results for keyboard navigation
  const searchResultNoteIds = useMemo(
    () => scopedSearchResults.map((r) => r.noteId),
    [scopedSearchResults]
  );

  // Task 45: Arrow-key navigation through search results
  useSearchKeyboardNav({
    isSearchActive,
    resultNoteIds: searchResultNoteIds,
    selectedNoteId,
    onSelectNote: handleKeyboardSelectNote,
  });

  // Unified items for NotesList: real SQLite search results when searching, standard notes when browsing (Task 14, 39)
  const displayedNoteListItems = useMemo<NoteListItem[]>(() => {
    if (isSearchActive) {
      return scopedSearchResults.map((result) => {
        const liveNote = notesLookup.get(result.noteId);
        const nbPath = result.notebookId ? notebookPathMap.get(result.notebookId) : undefined;
        const item = searchResultToNoteListItem(result, nbPath, noteTagsMap[result.noteId]);
        if (liveNote) {
          if (liveNote.title !== undefined) {
            item.title = liveNote.title;
          }
          item.isFavorite = liveNote.is_favorite;
        }
        return item;
      });
    }
    return noteListItems;
  }, [isSearchActive, scopedSearchResults, notesLookup, notebookPathMap, noteTagsMap, noteListItems]);

  // Unified status for NotesList (idle | loading | error | empty)
  const displayedStatus = useMemo<NotesListStatus>(() => {
    if (isSearchActive) {
      if (searchState.status === "searching") return "loading";
      if (searchState.status === "error") return "error";
      if (searchState.status === "success") {
        return scopedSearchResults.length === 0 ? "empty" : "idle";
      }
      return "idle";
    }
    return status;
  }, [isSearchActive, searchState, scopedSearchResults.length, status]);

  // Automatically select the first search result on search completion if no valid selection exists
  useEffect(() => {
    if (isSearchActive && searchState.status === "success") {
      if (scopedSearchResults.length > 0) {
        setSelectedNoteId((prev) =>
          prev && scopedSearchResults.some((r) => r.noteId === prev)
            ? prev
            : scopedSearchResults[0].noteId
        );
      } else {
        setSelectedNoteId(null);
      }
    }
  }, [isSearchActive, searchState, scopedSearchResults]);

  /**
   * Task 43 — Search Clear Behavior
   *
   * Chosen behavior: Context-preserving clear.
   *
   * When the user clears the search field (via the × button, Escape key, or
   * navigating away), the UI returns to the note list for the **current
   * navigation context** — the same notebook, tag, or favorites view that was
   * active before searching. The `notes` state always holds the last-loaded
   * batch for that context, so no additional network/IPC round-trip is needed.
   *
   * Example:
   *   Before search:  Work / Projects notebook (5 notes)
   *   Search query:   "deadline"
   *   Clear search:   → Work / Projects notebook (5 notes, same as before)
   *
   * Selection restoration: If the note that was auto-selected during search
   * is no longer visible in the restored context (e.g. it belonged to a
   * different notebook), we fall back to the first note in `filteredNotes`.
   * If the selected note IS visible, it remains selected — the editor stays
   * open and the user loses no work.
   */
  useEffect(() => {
    if (isSearchActive) return; // Only run when search has just been cleared
    if (!selectedNoteId) return;
    const stillVisible = filteredNotes.some((n) => n.id === selectedNoteId);
    if (!stillVisible) {
      setSelectedNoteId(filteredNotes.length > 0 ? filteredNotes[0].id : null);
    }
  }, [isSearchActive, filteredNotes, selectedNoteId]);

  const selectedNote = useMemo(() => {
    if (!selectedNoteId) return null;
    const foundInFiltered = filteredNotes.find((n) => n.id === selectedNoteId);
    if (foundInFiltered) return foundInFiltered;
    const foundInNotes = notes.find((n) => n.id === selectedNoteId);
    if (foundInNotes) return foundInNotes;
    const foundInSearch = scopedSearchResults.find((r) => r.noteId === selectedNoteId);
    if (foundInSearch) {
      return {
        id: foundInSearch.noteId,
        title: foundInSearch.title,
        content: foundInSearch.snippet,
        notebook_id: foundInSearch.notebookId ?? null,
        format: "txt" as const,
        is_favorite: foundInSearch.favorite ? 1 : 0,
        is_deleted: 0,
        created_at: "",
        modified_at: foundInSearch.modifiedAt || "",
      };
    }
    return null;
  }, [filteredNotes, notes, selectedNoteId, scopedSearchResults]);

  const selectedNotebook = useMemo(() => {
    if (!selectedNotebookId) return null;
    return notebooks.find((nb) => nb.id === selectedNotebookId) ?? null;
  }, [notebooks, selectedNotebookId]);

  const selectedNoteNotebookName = useMemo(() => {
    if (!selectedNote || !selectedNote.notebook_id) return "Unfiled";
    const nb = notebooks.find((n) => n.id === selectedNote.notebook_id);
    return nb ? nb.name : "Unfiled";
  }, [selectedNote, notebooks]);

  const selectedNoteNotebookPath = useMemo(() => {
    return computeNotebookPath(selectedNote?.notebook_id, notebooks);
  }, [selectedNote?.notebook_id, notebooks]);

  const handleSelectTag = useCallback(
    async (tagId: string) => {
      if (editorSaveRef.current) {
        await editorSaveRef.current();
      }
      selectTag(tagId);
      setActiveNavId("tags");
      await loadNotes("tags", null, tagId);
    },
    [selectTag, loadNotes]
  );

  const getSectionTitle = () => {
    if (searchQuery.trim()) {
      if (activeNavId === "favorites") {
        return `Favorites — Search: "${searchQuery}"`;
      }
      if (activeNavId === "tags" && selectedTagId) {
        const t = tags.find((item) => item.id === selectedTagId);
        return t ? `#${t.name} — Search: "${searchQuery}"` : `Tags — Search: "${searchQuery}"`;
      }
      if (activeNavId === "notebook" && selectedNotebook) {
        return `${selectedNotebook.name} — Search: "${searchQuery}"`;
      }
      return `Search: "${searchQuery}"`;
    }
    switch (activeNavId) {
      case "favorites":
        return "Favorites";
      case "unfiled":
        return "Unfiled Notes";
      case "tags": {
        if (selectedTagId) {
          const t = tags.find((item) => item.id === selectedTagId);
          return t ? `#${t.name}` : "Tagged Notes";
        }
        return tags.length > 0 ? `Tags (${tags.length})` : "Tags";
      }
      case "trash":
        return "Trash";
      case "notebook":
        return selectedNotebook ? selectedNotebook.name : "Notebook";
      case "all-notes":
      default:
        return "All Notes";
    }
  };

  const handleRetry = () => {
    if (!isStorageReady) {
      initStorage();
    } else {
      loadNotes();
      loadTags();
      refreshNoteCounts();
    }
  };

  const getEmptyStateContent = () => {
    if (activeNavId === "notebook") {
      return {
        title: "No notes in this notebook.",
        description: "Create a new note to get started.",
      };
    }
    if (activeNavId === "unfiled") {
      return {
        title: "No unfiled notes",
        description: "All your notes are organized into notebooks.",
      };
    }
    if (activeNavId === "favorites") {
      return {
        title: "Favorites",
        description: "You haven't favorited any notes yet. Select a note and mark it as favorite.",
      };
    }
    if (activeNavId === "tags") {
      const tagName = selectedTagId ? tags.find((t) => t.id === selectedTagId)?.name : null;
      return {
        title: tagName ? `#${tagName}` : "No tagged notes",
        description: "No notes use this tag yet.",
      };
    }
    if (activeNavId === "trash") {
      return {
        title: "Trash is empty",
        description: "Deleted notes will appear here.",
      };
    }
    return {
      title: "No notes yet",
      description: "Create your first note to get started.",
    };
  };

  const emptyState = getEmptyStateContent();

  const handleUnlockSuccess = useCallback(async () => {
    setIsAppLocked(false);
    setIsEncryptionEnabled(true);
    setIsInterruptedMigration(false);
    await loadNotes();
    await refreshNoteCounts();
    await loadNotebooks();
    await loadTags();
  }, [loadNotes, refreshNoteCounts, loadNotebooks, loadTags]);

  const handleRollbackSuccess = useCallback(async () => {
    setIsAppLocked(false);
    setIsEncryptionEnabled(false);
    setIsInterruptedMigration(false);
    await loadNotes();
    await refreshNoteCounts();
    await loadNotebooks();
    await loadTags();
  }, [loadNotes, refreshNoteCounts, loadNotebooks, loadTags]);

  if (isAppLocked) {
    return (
      <div className="app-locked-wrapper" data-theme={theme}>
        {isInterruptedMigration ? (
          <UnlockScreen
            onUnlockSuccess={handleUnlockSuccess}
            isInterrupted={isInterruptedMigration}
            onRollbackSuccess={handleRollbackSuccess}
          />
        ) : (
          <UnlockScreen onUnlockSuccess={handleUnlockSuccess} />
        )}
      </div>
    );
  }

  return (
    <>
      <MainLayout
        topBar={
          <TopBar
            searchInputRef={searchInputRef}
            searchQuery={searchQuery}
            isSearching={searchState.status === "searching"}
            onSearchChange={setSearchQuery}
            onSearchSubmit={() => searchNow()}
            onSearchClear={clearSearch}
            onSearchFocus={handleFlushPendingSave}
            theme={theme}
            onThemeToggle={toggleTheme}
            onNewNoteClick={handleNewNoteAction}
            onSettingsClick={() => setIsSettingsOpen(true)}
            canGoBack={navigationHistory.canGoBack}
            canGoForward={navigationHistory.canGoForward}
            onNavigateBack={handleNavigateBack}
            onNavigateForward={handleNavigateForward}
            isEncryptionEnabled={isEncryptionEnabled}
            onLockClick={handleLockApp}
          />
        }
        sidebar={
          <Sidebar
            activeNavId={activeNavId}
            onSelectNav={async (navId) => {
              if (editorSaveRef.current) {
                await editorSaveRef.current();
              }
              setActiveNavId(navId);
              if (navId !== "favorites" && navId !== "all-notes") {
                if (searchQuery) {
                  clearSearch();
                }
              }
              if (navId === "all-notes") {
                selectAllNotes();
                await loadNotes("all-notes", null, null);
              } else if (navId === "unfiled") {
                selectUnfiled();
                await loadNotes("unfiled", null, null);
              } else if (navId === "favorites") {
                selectFavorites();
                await loadNotes("favorites", null, null);
              } else if (navId === "tags") {
                if (tags.length > 0) {
                  selectTag(tags[0].id);
                  await loadNotes("tags", null, tags[0].id);
                } else {
                  await loadNotes("tags", null, null);
                }
              } else if (navId === "trash") {
                selectTrash();
                await loadNotes("trash", null, null);
              }
            }}
            onNewNoteClick={handleNewNoteAction}
            notebooks={notebooks}
            notebooksStatus={notebooksStatus}
            notebooksError={notebooksError}
            selectedNotebookId={selectedNotebookId}
            expandedNotebookIds={expandedNotebookIds}
            onToggleExpandNotebook={toggleExpandNotebook}
            onSelectNotebook={async (nbId) => {
              if (editorSaveRef.current) {
                await editorSaveRef.current();
              }
              selectNotebook(nbId, notebooks);
              setActiveNavId("notebook");
              if (searchQuery) {
                clearSearch();
              }
              await loadNotes("notebook", nbId, null);
            }}
            onRetryNotebooks={loadNotebooks}
            onCreateNotebook={handleOpenCreateNotebook}
            onRenameNotebook={setRenamingNotebook}
            onDeleteNotebook={setDeletingNotebook}
            onExportNotebook={handleOpenExportNotebook}
            onImportNotes={handleOpenImport}
            onDropNote={handleMoveNote}
            noteCounts={noteCounts}
            tags={tags}
            selectedTagId={selectedTagId}
            onSelectTag={handleSelectTag}
            onCreateTag={handleOpenCreateTag}
            onRenameTag={setRenamingTag}
            onDeleteTag={setDeletingTag}
            tagCounts={tagCounts}
            onManageTags={() => setIsManageTagsOpen(true)}
          />
        }
        notesList={
          <SearchErrorBoundary
            onReset={() => {
              clearSearch();
            }}
          >
            <NotesList
              title={getSectionTitle()}
              notes={displayedNoteListItems}
              selectedNoteId={selectedNoteId}
              onSelectNote={handleKeyboardSelectNote}

              status={displayedStatus}
              errorTitle={
                isSearchActive
                  ? "Search failed"
                  : activeNavId === "trash"
                  ? "Failed to load trash"
                  : undefined
              }
              errorMessage={
                isSearchActive && searchState.status === "error"
                  ? searchState.message
                  : errorMessage
              }
              emptyTitle={
                isSearchActive
                  ? (activeNavId === "favorites"
                      ? "No favorite notes found"
                      : activeNavId === "tags"
                      ? "No tagged notes found"
                      : activeNavId === "notebook"
                      ? "No notes found in this notebook"
                      : "No notes found")
                  : activeNavId === "trash"
                  ? "Trash is empty."
                  : emptyState.title
              }
              emptyDescription={
                isSearchActive
                  ? (activeNavId === "favorites"
                      ? `No favorite notes match "${searchQuery}".`
                      : activeNavId === "tags" && selectedTagId
                      ? `No notes tagged #${tags.find((t) => t.id === selectedTagId)?.name ?? "selected tag"} match "${searchQuery}".`
                      : activeNavId === "notebook" && selectedNotebook
                      ? `No notes in "${selectedNotebook.name}" match "${searchQuery}".`
                      : `No notes match "${searchQuery}".`)
                  : activeNavId === "trash"
                  ? "Deleted notes will appear here."
                  : emptyState.description
              }
              mode={isSearchActive ? "search" : "normal"}
              isTrash={activeNavId === "trash"}
              searchQuery={searchQuery}
              resultCountLabel={
                isSearchActive
                  ? formatResultCount(scopedSearchResults.length)
                  : String(
                      activeNavId === "tags" && selectedTagId
                        ? tagCounts[selectedTagId] ?? displayedNoteListItems.length
                        : activeNavId === "notebook" && selectedNotebookId
                        ? noteCounts[selectedNotebookId] ?? displayedNoteListItems.length
                        : noteCounts[activeNavId] ?? displayedNoteListItems.length
                    )
              }
              listKey={`${activeNavId}:${selectedNotebookId ?? ""}:${selectedTagId ?? ""}`}
              hasMore={!isSearchActive && hasMoreNotes}
              isLoadingMore={isLoadingMoreNotes}
              onLoadMore={loadMoreNotes}
              onRetry={isSearchActive ? () => searchNow() : handleRetry}
              onNewNote={isSearchActive || activeNavId === "trash" ? undefined : handleNewNoteAction}
              onToggleFavorite={handleToggleNoteFavorite}
              onDeleteNote={activeNavId === "trash" ? undefined : requestDeleteNote}
              onRestoreNote={requestRestoreNote}
              onPermanentlyDeleteNote={handlePermanentlyDeleteNote}
              onEmptyTrash={handleOpenEmptyTrash}
              onExport={
                activeNavId === "notebook" && selectedNotebookId
                  ? () => {
                      const nb = notebooks.find((n) => n.id === selectedNotebookId);
                      if (nb) handleOpenExportNotebook(nb);
                    }
                  : activeNavId === "all-notes"
                  ? () => {
                      setExportBatchState({
                        isOpen: true,
                        scope: { type: "all_notes" },
                      });
                    }
                  : undefined
              }
              exportLabel={
                activeNavId === "notebook"
                  ? "Export Notebook"
                  : activeNavId === "all-notes"
                  ? "Export All"
                  : undefined
              }
              onImport={handleOpenImport}
            />
          </SearchErrorBoundary>
        }
        editor={
          <NoteEditor
            key={`${selectedNoteId || "empty"}-${noteRefreshKey}`}
            noteId={selectedNoteId}
            onSaveNote={handleSaveNote}
            onRegisterSave={handleRegisterSave}
            onActivateLink={handleActivateInternalLink}
            isTrash={activeNavId === "trash"}
            onDeleteNote={activeNavId === "trash" ? undefined : requestDeleteNote}
            onRestoreNote={requestRestoreNote}
            onPermanentlyDeleteNote={handlePermanentlyDeleteNote}
            notebookName={selectedNoteNotebookName}
            notebookPath={selectedNoteNotebookPath}
            onMoveNote={setMovingNote}
            noteTags={noteTags}
            availableTags={tags}
            onAddTag={async (tagId) => {
              await addTagToSelectedNote(tagId);
              refreshNoteCounts();
            }}
            onRemoveTag={async (tagId) => {
              await removeTagFromSelectedNote(tagId);
              refreshNoteCounts();
              if (activeNavId === "tags" && selectedTagId === tagId) {
                loadNotes();
              }
            }}
            onCreateTag={createTag}
            onNoteUpdated={(updated) => {
              setNotes((prev) => {
                const exists = prev.some((n) => n.id === updated.id);
                if (exists) {
                  return prev.map((n) => (n.id === updated.id ? updated : n));
                }
                return [updated, ...prev];
              });
            }}
          />
        }
      />

      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        currentTheme={theme}
        onSelectTheme={setTheme}
        zoomPercent={display.zoomPercent}
        onZoomIn={display.zoomIn}
        onZoomOut={display.zoomOut}
        onZoomReset={display.resetZoom}
        editorFontSize={display.editorFontSize}
        onEditorFontSizeChange={display.setEditorFontSize}
        onEditorFontSizeReset={display.resetEditorFontSize}
        onCreateBackup={() => {
          setIsSettingsOpen(false);
          setIsCreateBackupOpen(true);
        }}
        onViewBackups={() => {
          setIsSettingsOpen(false);
          setIsBackupListOpen(true);
        }}
        onLock={handleLockApp}
        onFlushPendingSave={handleFlushPendingSave}
        onEncryptionStatusChange={setIsEncryptionEnabled}
        autoLockSettings={autoLockSettings}
        onUpdateAutoLockSettings={handleUpdateAutoLockSettings}
      />

      <CreateNotebookDialog
        isOpen={createModalState.isOpen}
        parentId={createModalState.parentId}
        parentName={createModalState.parentName}
        onClose={() =>
          setCreateModalState((prev) => ({ ...prev, isOpen: false }))
        }
        onCreate={handleCreateNotebook}
      />

      <RenameNotebookDialog
        isOpen={renamingNotebook !== null}
        notebook={renamingNotebook}
        onClose={() => setRenamingNotebook(null)}
        onRename={handleRenameNotebook}
      />

      <DeleteNotebookDialog
        isOpen={deletingNotebook !== null}
        notebook={deletingNotebook}
        hasChildren={deletingNotebookHasChildren}
        onClose={() => setDeletingNotebook(null)}
        onDelete={handleDeleteNotebook}
      />

      <MoveNoteDialog
        isOpen={movingNote !== null}
        note={movingNote}
        notebooks={notebooks}
        onClose={() => setMovingNote(null)}
        onMove={handleMoveNote}
      />

      <CreateTagDialog
        isOpen={createTagModal.isOpen}
        initialName={createTagModal.initialName}
        onClose={() => setCreateTagModal({ isOpen: false })}
        onCreate={handleCreateTag}
      />

      <RenameTagDialog
        isOpen={renamingTag !== null}
        tag={renamingTag}
        onClose={() => setRenamingTag(null)}
        onRename={handleRenameTag}
      />

      <DeleteTagDialog
        isOpen={deletingTag !== null}
        tag={deletingTag}
        onClose={() => setDeletingTag(null)}
        onDelete={handleDeleteTag}
      />

      <ManageTagsDialog
        isOpen={isManageTagsOpen}
        tags={tags}
        tagCounts={tagCounts}
        onClose={() => setIsManageTagsOpen(false)}
        onCreateTag={handleCreateTag}
        onRenameTag={handleRenameTag}
        onDeleteTag={(tag) => {
          setDeletingTag(tag);
        }}
      />

      <ConfirmNoteActionDialog
        target={noteActionConfirm}
        onClose={() => setNoteActionConfirm(null)}
        onConfirm={handleConfirmNoteAction}
      />

      <PermanentDeleteDialog
        isOpen={permanentDeleteTarget !== null}
        note={permanentDeleteTarget}
        onClose={() => setPermanentDeleteTarget(null)}
        onConfirm={handleConfirmPermanentDelete}
      />

      <EmptyTrashDialog
        isOpen={isEmptyTrashOpen}
        noteCount={notes.length}
        onClose={() => setIsEmptyTrashOpen(false)}
        onConfirm={handleConfirmEmptyTrash}
      />

      <BrokenLinkDialog
        target={linkNavigationTarget}
        onClose={handleCloseLinkNavigationDialog}
        onViewInTrash={handleConfirmViewInTrash}
        onRestore={handleRestoreAndOpenTargetNote}
        onRepairLink={handleInitiateRepairBrokenLink}
        onRemoveLink={handleRemoveBrokenLink}
      />

      {repairingBrokenTargetId && (
        <NotePickerModal
          isOpen={true}
          onClose={() => setRepairingBrokenTargetId(null)}
          onSelectNote={handleConfirmRepairReplacement}
          currentNoteId={selectedNoteId}
        />
      )}

      {isResolvingLinkTarget && (
        <div
          className="link-resolving-indicator"
          role="status"
          aria-live="polite"
          data-testid="link-resolving-indicator"
          style={{
            position: "fixed",
            bottom: "20px",
            left: "50%",
            transform: "translateX(-50%)",
            backgroundColor: "var(--color-bg-surface)",
            color: "var(--color-text-primary)",
            padding: "8px 16px",
            borderRadius: "var(--radius-md)",
            border: "1px solid var(--color-border-subtle)",
            boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
            fontSize: "13px",
            zIndex: 9999,
            display: "flex",
            alignItems: "center",
            gap: "8px",
          }}
        >
          <span className="backlinks-spinner" style={{ width: 12, height: 12 }} />
          <span>Opening linked note...</span>
        </div>
      )}

      {restoreFeedback && (
        <div
          className={`restore-feedback-toast restore-feedback-toast--${restoreFeedback.type}`}
          role="status"
          aria-live="polite"
        >
          <span className="restore-feedback-icon" aria-hidden="true">
            {restoreFeedback.type === "success" ? "✓" : restoreFeedback.type === "info" ? "ℹ" : "⚠"}
          </span>
          <span className="restore-feedback-message">{restoreFeedback.message}</span>
          <button
            type="button"
            className="restore-feedback-close"
            onClick={() => setRestoreFeedback(null)}
            aria-label="Dismiss notification"
          >
            ✕
          </button>
        </div>
      )}

      {trashFeedback && (
        <div
          className={`trash-feedback-toast trash-feedback-toast--${trashFeedback.type}`}
          role="status"
          aria-live="polite"
        >
          <span className="trash-feedback-icon" aria-hidden="true">
            {trashFeedback.type === "success"
              ? "✓"
              : trashFeedback.type === "warning"
              ? "⚠"
              : "✕"}
          </span>
          <span className="trash-feedback-message">{trashFeedback.message}</span>
          {trashFeedback.cleanupNoteIds?.length ? (
            <button
              type="button"
              className="trash-feedback-retry"
              onClick={handleRetryAttachmentCleanup}
            >
              Retry cleanup
            </button>
          ) : null}
          <button
            type="button"
            className="trash-feedback-close"
            onClick={() => setTrashFeedback(null)}
            aria-label="Dismiss notification"
          >
            ✕
          </button>
        </div>
      )}

      {backupAlert && (
        <div
          className={`backup-alert-toast backup-alert-toast--${backupAlert.type}`}
          role="alert"
          aria-live="polite"
          data-testid="backup-alert-toast"
        >
          <span className="backup-alert-icon" aria-hidden="true">
            {backupAlert.type === "error" || backupAlert.type === "critical"
              ? "✕"
              : backupAlert.type === "warning"
              ? "⚠"
              : "ℹ"}
          </span>
          <div className="backup-alert-content">
            {backupAlert.title && <strong className="backup-alert-title">{backupAlert.title}: </strong>}
            <span className="backup-alert-message">{backupAlert.message}</span>
          </div>
          <button
            type="button"
            className="backup-alert-close"
            onClick={() => {
              if (backupAlert.code) {
                setDismissedBackupCodes((prev) => new Set(prev).add(backupAlert.code!));
              }
              setBackupAlert(null);
            }}
            aria-label="Dismiss notification"
          >
            ✕
          </button>
        </div>
      )}
      {exportBatchState?.isOpen && (
        <ExportBatchModal
          isOpen={exportBatchState.isOpen}
          onClose={() => setExportBatchState(null)}
          scope={exportBatchState.scope}
          notebookName={exportBatchState.notebookName}
          onFlushSave={handleFlushPendingSave}
          onSuccess={(summary) => {
            setRestoreFeedback({
              type: "success",
              message: `Successfully exported ${summary.exportedCount} note${summary.exportedCount === 1 ? "" : "s"}.`,
            });
          }}
        />
      )}
      {importModalState.isOpen && (
        <ImportPreviewModal
          isOpen={importModalState.isOpen}
          onClose={() =>
            setImportModalState({ isOpen: false, preview: null, isLoading: false })
          }
          preview={importModalState.preview}
          isLoading={importModalState.isLoading}
          onConfirmImport={handleConfirmImport}
          notebooks={notebooks}
          defaultNotebookId={selectedNotebookId}
        />
      )}
      {isCreateBackupOpen && (
        <CreateBackupModal
          isOpen={isCreateBackupOpen}
          onClose={() => setIsCreateBackupOpen(false)}
          onFlushSave={handleFlushPendingSave}
          onSuccess={(backupRes) => {
            setRestoreFeedback({
              type: "success",
              message: `Backup created and verified (${backupRes.noteCount} notes, ${backupRes.attachmentCount} attachments).`,
            });
          }}
        />
      )}
      {isBackupListOpen && (
        <BackupListModal
          isOpen={isBackupListOpen}
          onClose={() => setIsBackupListOpen(false)}
          onCreateNewBackup={() => {
            setIsBackupListOpen(false);
            setIsCreateBackupOpen(true);
          }}
          onSelectForRestore={(backup) => {
            setIsBackupListOpen(false);
            setRestorePreviewCandidate(backup.filePath);
          }}
        />
      )}
      {restorePreviewCandidate && (
        <RestorePreviewModal
          isOpen={Boolean(restorePreviewCandidate)}
          backupFilePath={restorePreviewCandidate}
          onClose={() => !isRestoringBackup && setRestorePreviewCandidate(null)}
          isRestoring={isRestoringBackup}
          restoreProgress={restoreProgress}
          onCancelRestore={handleCancelRestore}
          isCancelling={isCancellingRestore}
          onConfirmRestore={async (preview) => {
            logger.info(`Restore preview confirmed for ${preview.filePath}`);
            const opId =
              typeof crypto !== "undefined" && crypto.randomUUID
                ? crypto.randomUUID()
                : `restore-${Date.now()}`;
            setRestoreOperationId(opId);
            setIsRestoringBackup(true);
            setIsCancellingRestore(false);
            setRestoreProgress({
              stage: "starting",
              message: "Initiating controlled restore...",
              percentage: 5,
              cancellable: true,
              operationId: opId,
            });

            // Task 20: Pre-emptively disable in-flight saves from the old state
            // to ensure pending writes from the pre-restore database cannot overwrite restored data
            editorSaveRef.current = null;

            try {
              const result = await backupStorage.executeRestore(
                { filePath: preview.filePath, operationId: opId },
                (prog) => {
                  setRestoreProgress(prog);
                }
              );
              logger.info(`Restore successful: ${result.noteCount} notes, ${result.attachmentCount} attachments`);
              setRestorePreviewCandidate(null);
              if (result.warningMessage) {
                setRestoreFeedback({
                  type: "warning",
                  message: `Restore completed with warnings: ${result.warningMessage}`,
                });
              } else {
                setRestoreFeedback({
                  type: "success",
                  message: `Successfully restored backup (${result.noteCount} notes, ${result.notebookCount} notebooks). Pre-restore checkpoint preserved.`,
                });
              }

              // Task 20: Full application state reinitialization
              // 1. Clear stale in-memory editor caches by unmounting/remounting NoteEditor and clearing selection
              setNoteRefreshKey((prev) => prev + 1);
              setSelectedNoteId(null);

              // 2. Clear any active search query and cancel pending debounce
              cancelPendingDebounce();
              clearSearch();

              // 3. Reset navigation to all-notes view
              setActiveNavId("all-notes");
              selectAllNotes();

              // Phase 12, Task 19: Resynchronize runtime encryption status after restore
              let isEncryptedAndLocked = false;
              try {
                const encStatus = await storageService.crypto.getEncryptionStatus();
                setIsEncryptionEnabled(encStatus.enabled);
                setIsInterruptedMigration(Boolean(encStatus.is_interrupted));
                if ((encStatus.enabled && encStatus.is_locked) || encStatus.is_interrupted) {
                  setIsAppLocked(true);
                  setNotes([]);
                  setSelectedNoteId(null);
                  isEncryptedAndLocked = true;
                } else {
                  setIsAppLocked(false);
                }
              } catch (encErr) {
                logger.warn("Could not retrieve encryption status after restore:", { error: String(encErr) });
              }

              // 4. Reload domain data if app is not locked
              if (!isEncryptedAndLocked) {
                await Promise.all([
                  loadNotes("all-notes", null, null),
                  loadNotebooks(),
                  loadTags(),
                  refreshNoteCounts(),
                ]);
              }

              // 5. Refresh application settings (e.g. theme) from restored database
              try {
                const sqliteTheme = await storageService.settings.get("theme");
                if (sqliteTheme === "light" || sqliteTheme === "dark" || sqliteTheme === "system") {
                  setTheme(sqliteTheme);
                }
              } catch (e) {
                logger.error("Failed to refresh theme after restore", e);
              }
            } catch (err: unknown) {
              const msg = err instanceof Error ? err.message : String(err);
              logger.error(`Restore failed: ${msg}`);
              setRestorePreviewCandidate(null);

              const isCancelled = msg.toLowerCase().includes("cancel");
              setRestoreFeedback({
                type: isCancelled ? "info" : "error",
                message: isCancelled
                  ? "Restore operation was cancelled before activation. Your live data remains untouched."
                  : `Restore failed: ${msg}. Previous data was safely preserved.`,
              });

              // Reinitialize state against rolled-back pre-restore data
              editorSaveRef.current = null;
              setNoteRefreshKey((prev) => prev + 1);
              setSelectedNoteId(null);
              cancelPendingDebounce();
              clearSearch();
              setActiveNavId("all-notes");
              selectAllNotes();

              let rollbackLocked = false;
              try {
                const encStatus = await storageService.crypto.getEncryptionStatus();
                setIsEncryptionEnabled(encStatus.enabled);
                setIsInterruptedMigration(Boolean(encStatus.is_interrupted));
                if ((encStatus.enabled && encStatus.is_locked) || encStatus.is_interrupted) {
                  setIsAppLocked(true);
                  setNotes([]);
                  setSelectedNoteId(null);
                  rollbackLocked = true;
                } else {
                  setIsAppLocked(false);
                }
              } catch {
                // Ignore
              }

              if (!rollbackLocked) {
                await Promise.all([
                  loadNotes("all-notes", null, null),
                  loadNotebooks(),
                  loadTags(),
                  refreshNoteCounts(),
                ]);
              }
            } finally {
              setIsRestoringBackup(false);
              setIsCancellingRestore(false);
              setRestoreProgress(null);
              setRestoreOperationId(null);
            }
          }}
        />
      )}
    </>
  );
}

export default App;
