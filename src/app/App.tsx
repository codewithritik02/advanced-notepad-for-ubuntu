import { useState, useRef, useCallback, useEffect, useMemo } from "react";
import { MainLayout } from "../layouts/MainLayout";
import { TopBar } from "../components/TopBar/TopBar";
import { Sidebar, NavItemId } from "../components/Sidebar/Sidebar";
import { NotesList } from "../features/notes/components/NotesList/NotesList";
import { NoteListItem, NotesListStatus } from "../features/notes/types";
import { NoteEditor } from "../features/notes/components/Editor/NoteEditor";
import { SettingsModal } from "../features/settings/components/SettingsModal/SettingsModal";
import { useTheme } from "../hooks/useTheme";
import { useKeyboardShortcuts } from "../hooks/useKeyboardShortcuts";
import { storageService, Note, Tag, NoteFormat } from "../services/storage";
import {
  useNotebooks,
  useNotebookSelection,
  CreateNotebookDialog,
  CreateNotebookModalState,
  RenameNotebookDialog,
  DeleteNotebookDialog,
  MoveNoteDialog,
  Notebook,
} from "../features/notebooks";

export function App() {
  const { theme, setTheme, toggleTheme } = useTheme();
  const [activeNavId, setActiveNavId] = useState<NavItemId>("all-notes");
  const [searchQuery, setSearchQuery] = useState("");
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [notes, setNotes] = useState<Note[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [selectedNoteId, setSelectedNoteId] = useState<string | null>(null);
  const [isStorageReady, setIsStorageReady] = useState(false);
  const [status, setStatus] = useState<NotesListStatus>("loading");
  const [errorMessage, setErrorMessage] = useState("Unable to load notes.");
  const [noteCounts, setNoteCounts] = useState<Record<string, number>>({});

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
    selectedNotebookId,
    selectNotebook,
    selectAllNotes,
    selectUnfiled,
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

  const deletingNotebookHasChildren = useMemo(() => {
    if (!deletingNotebook) return false;
    return notebooks.some((nb) => nb.parent_id === deletingNotebook.id);
  }, [deletingNotebook, notebooks]);

  const searchInputRef = useRef<HTMLInputElement>(null);

  // Check and verify storage readiness at startup
  const initStorage = useCallback(async () => {
    setStatus("loading");
    try {
      const info = await storageService.getInfo();
      if (info.is_initialized) {
        setIsStorageReady(true);
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

  // Load metadata (tags) from SQLite
  const loadMetadata = useCallback(async () => {
    if (!isStorageReady) return;
    try {
      const tagList = await storageService.tags.list();
      setTags(tagList);
    } catch {
      // Non-fatal error for metadata
    }
  }, [isStorageReady]);

  // Load global notebook note counts
  const refreshNoteCounts = useCallback(async () => {
    if (!isStorageReady) return;
    try {
      const allActiveNotes = await storageService.notes.list();
      const counts: Record<string, number> = {};
      for (const note of allActiveNotes) {
        if (!note.is_deleted && note.notebook_id) {
          counts[note.notebook_id] = (counts[note.notebook_id] || 0) + 1;
        }
      }
      setNoteCounts(counts);
    } catch {
      // Non-fatal for badge counters
    }
  }, [isStorageReady]);

  // Load persistent notes from SQLite
  const loadNotes = useCallback(
    async (overrideNavId?: NavItemId, overrideNotebookId?: string | null) => {
      if (!isStorageReady) return;
      setStatus("loading");
      try {
        const effectiveNav = overrideNavId !== undefined ? overrideNavId : activeNavId;
        const effectiveNotebook =
          overrideNotebookId !== undefined ? overrideNotebookId : selectedNotebookId;

        let options: { includeDeleted?: boolean; notebookId?: string | null; unfiledOnly?: boolean } = {};

        if (effectiveNav === "trash") {
          options = { includeDeleted: true };
        } else if (effectiveNav === "unfiled") {
          options = { unfiledOnly: true };
        } else if (effectiveNav === "notebook" && effectiveNotebook) {
          options = { notebookId: effectiveNotebook };
        }

        const fetchedNotes = await storageService.notes.list(options);
        setNotes(fetchedNotes);

        if (fetchedNotes.length > 0) {
          setSelectedNoteId((prev) =>
            prev && fetchedNotes.some((n) => n.id === prev) ? prev : fetchedNotes[0].id
          );
        } else {
          setSelectedNoteId(null);
        }
        setStatus(fetchedNotes.length === 0 ? "empty" : "idle");
      } catch (err) {
        setErrorMessage(
          "Unable to load notes: " +
            (err instanceof Error ? err.message : String(err))
        );
        setStatus("error");
      }
    },
    [activeNavId, selectedNotebookId, isStorageReady]
  );

  useEffect(() => {
    if (isStorageReady) {
      loadNotes();
      loadMetadata();
      refreshNoteCounts();
    }
  }, [isStorageReady, loadNotes, loadMetadata, refreshNoteCounts]);

  const isCreatingRef = useRef(false);

  // Persistent note creation (Task 18: Assign to selected notebook or unfiled)
  const handleNewNoteAction = useCallback(async () => {
    if (isCreatingRef.current) return;
    isCreatingRef.current = true;

    try {
      // If user was viewing trash or has an active search filter, reset to all-notes to reveal the new note
      if (activeNavId === "trash") {
        setActiveNavId("all-notes");
      }
      if (searchQuery) {
        setSearchQuery("");
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
      try {
        const updated = await storageService.notes.update(id, {
          title,
          content,
          format: format || "txt",
        });
        setNotes((prev) => prev.map((n) => (n.id === id ? updated : n)));
      } catch (err) {
        setErrorMessage(
          "Failed to save note: " +
            (err instanceof Error ? err.message : String(err))
        );
      }
    },
    []
  );

  const handleFocusSearch = useCallback(() => {
    searchInputRef.current?.focus();
    searchInputRef.current?.select();
  }, []);

  const handleOpenSettings = useCallback(() => {
    setIsSettingsOpen((prev) => !prev);
  }, []);

  const handleEscape = useCallback(() => {
    if (isSettingsOpen) {
      setIsSettingsOpen(false);
    } else if (searchQuery) {
      setSearchQuery("");
      searchInputRef.current?.blur();
    } else {
      searchInputRef.current?.blur();
    }
  }, [isSettingsOpen, searchQuery]);

  const editorSaveRef = useRef<(() => Promise<void> | void) | null>(null);

  const handleRegisterSave = useCallback(
    (saveFn: (() => Promise<void> | void) | null) => {
      editorSaveRef.current = saveFn;
    },
    []
  );

  useKeyboardShortcuts({
    onNewNote: handleNewNoteAction,
    onFocusSearch: handleFocusSearch,
    onOpenSettings: handleOpenSettings,
    onEscape: handleEscape,
    onSaveNote: () => {
      editorSaveRef.current?.();
    },
  });

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
    },
    [deleteNotebook, selectedNotebookId, selectUnfiled, loadNotes, loadNotebooks, refreshNoteCounts]
  );

  const handleMoveNote = useCallback(
    async (noteId: string, targetNotebookId: string | null) => {
      await editorSaveRef.current?.();
      await storageService.notes.moveToNotebook(noteId, targetNotebookId);
      await loadNotes();
      await loadNotebooks();
      await refreshNoteCounts();
    },
    [loadNotes, loadNotebooks, refreshNoteCounts]
  );

  // Filter notes based on active sidebar section and search query
  const filteredNotes = useMemo(() => {
    return notes.filter((n) => {
      if (activeNavId === "favorites" && !n.is_favorite) return false;
      if (activeNavId === "trash" && !n.is_deleted) return false;
      if (activeNavId !== "trash" && n.is_deleted) return false;
      if (activeNavId === "unfiled" && n.notebook_id !== null) return false;
      if (activeNavId === "notebook" && selectedNotebookId && n.notebook_id !== selectedNotebookId) return false;

      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        return (
          n.title.toLowerCase().includes(q) ||
          n.content.toLowerCase().includes(q)
        );
      }
      return true;
    });
  }, [notes, activeNavId, selectedNotebookId, searchQuery]);

  // Convert SQLite domain Notes to UI NoteListItems
  const noteListItems = useMemo<NoteListItem[]>(() => {
    return filteredNotes.map((n) => {
      // For large notes (100KB-1MB), avoid regex replacement over the full body
      const snippet = n.content.length > 300 ? n.content.slice(0, 300) : n.content;
      const cleanContent = snippet.replace(/\s+/g, " ").trim();
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

      return {
        id: n.id,
        title: n.title.trim() ? n.title : "Untitled Note",
        preview,
        updatedAt,
        isFavorite: n.is_favorite,
        notebookId: n.notebook_id ?? undefined,
      };
    });
  }, [filteredNotes]);

  const selectedNote = useMemo(() => {
    if (!selectedNoteId) return null;
    return filteredNotes.find((n) => n.id === selectedNoteId) ?? null;
  }, [filteredNotes, selectedNoteId]);

  const selectedNotebook = useMemo(() => {
    if (!selectedNotebookId) return null;
    return notebooks.find((nb) => nb.id === selectedNotebookId) ?? null;
  }, [notebooks, selectedNotebookId]);

  const selectedNoteNotebookName = useMemo(() => {
    if (!selectedNote || !selectedNote.notebook_id) return "Unfiled";
    const nb = notebooks.find((n) => n.id === selectedNote.notebook_id);
    return nb ? nb.name : "Unfiled";
  }, [selectedNote, notebooks]);

  const getSectionTitle = () => {
    if (searchQuery.trim()) return `Search: "${searchQuery}"`;
    switch (activeNavId) {
      case "favorites":
        return "Favorites";
      case "unfiled":
        return "Unfiled Notes";
      case "tags":
        return tags.length > 0 ? `Tags (${tags.length})` : "Tags";
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
      loadMetadata();
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
        title: "No favorite notes",
        description: "Star notes to see them here.",
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

  return (
    <>
      <MainLayout
        topBar={
          <TopBar
            searchInputRef={searchInputRef}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            theme={theme}
            onThemeToggle={toggleTheme}
            onNewNoteClick={handleNewNoteAction}
            onSettingsClick={() => setIsSettingsOpen(true)}
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
              setSearchQuery("");
              if (navId === "all-notes") {
                selectAllNotes();
              } else if (navId === "unfiled") {
                selectUnfiled();
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
              setSearchQuery("");
            }}
            onRetryNotebooks={loadNotebooks}
            onCreateNotebook={handleOpenCreateNotebook}
            onRenameNotebook={setRenamingNotebook}
            onDeleteNotebook={setDeletingNotebook}
            onDropNote={handleMoveNote}
            noteCounts={noteCounts}
          />
        }
        notesList={
          <NotesList
            title={getSectionTitle()}
            notes={noteListItems}
            selectedNoteId={selectedNote?.id ?? null}
            onSelectNote={async (noteId) => {
              if (noteId !== selectedNoteId) {
                if (editorSaveRef.current) {
                  await editorSaveRef.current();
                }
                setSelectedNoteId(noteId);
              }
            }}
            status={status}
            errorMessage={errorMessage}
            emptyTitle={emptyState.title}
            emptyDescription={emptyState.description}
            onRetry={handleRetry}
            onNewNote={handleNewNoteAction}
          />
        }
        editor={
          <NoteEditor
            noteId={selectedNote?.id ?? null}
            onSaveNote={handleSaveNote}
            onRegisterSave={handleRegisterSave}
            notebookName={selectedNoteNotebookName}
            onMoveNote={setMovingNote}
            onNoteUpdated={(updated) => {
              setNotes((prev) =>
                prev.map((n) => (n.id === updated.id ? updated : n))
              );
            }}
          />
        }
      />

      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        currentTheme={theme}
        onSelectTheme={setTheme}
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
    </>
  );
}

export default App;
