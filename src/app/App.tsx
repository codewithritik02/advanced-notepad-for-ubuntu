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
import { storageService, Note, NoteFormat } from "../services/storage";
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
  searchResultToNoteListItem,
  formatResultCount,
} from "../features/search";

export function App() {
  const { theme, setTheme, toggleTheme } = useTheme();
  const [activeNavId, setActiveNavId] = useState<NavItemId>("all-notes");
  const {
    query: searchQuery,
    setQuery: setSearchQuery,
    searchState,
    results: searchResults,
    isSearchActive,
    clearSearch,
    searchNow,
    cancelPendingDebounce,
  } = useSearch({ debounceMs: 200, limit: 50 });
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [notes, setNotes] = useState<Note[]>([]);
  const [selectedNoteId, setSelectedNoteId] = useState<string | null>(null);
  const [isStorageReady, setIsStorageReady] = useState(false);
  const [status, setStatus] = useState<NotesListStatus>("loading");
  const [errorMessage, setErrorMessage] = useState("Unable to load notes.");
  const [noteCounts, setNoteCounts] = useState<Record<string, number>>({});
  const [tagCounts, setTagCounts] = useState<Record<string, number>>({});
  const [noteTagsMap, setNoteTagsMap] = useState<Record<string, string[]>>({});
  const [isManageTagsOpen, setIsManageTagsOpen] = useState(false);

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
    selectedNotebookId,
    selectedTagId,
    selectNotebook,
    selectAllNotes,
    selectUnfiled,
    selectFavorites,
    selectTag,
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

  // Load global notebook, section, and tag note counts
  const refreshNoteCounts = useCallback(async () => {
    if (!isStorageReady) return;
    try {
      const allActiveNotes = await storageService.notes.list();
      const counts: Record<string, number> = {};
      let favCount = 0;
      let unfiledCount = 0;
      let allCount = 0;
      for (const note of allActiveNotes) {
        if (!note.is_deleted) {
          allCount++;
          if (note.is_favorite) favCount++;
          if (!note.notebook_id) unfiledCount++;
          if (note.notebook_id) {
            counts[note.notebook_id] = (counts[note.notebook_id] || 0) + 1;
          }
        }
      }
      counts["favorites"] = favCount;
      counts["unfiled"] = unfiledCount;
      counts["all-notes"] = allCount;
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
      await deleteTag(id);
      if (selectedTagId === id) {
        selectAllNotes();
        setActiveNavId("all-notes");
      }
      await refreshNoteCounts();
      if (selectedNoteId) {
        await loadNoteTags();
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
      if (!isStorageReady) return;
      setStatus("loading");
      try {
        const effectiveNav = overrideNavId !== undefined ? overrideNavId : activeNavId;
        const effectiveNotebook =
          overrideNotebookId !== undefined ? overrideNotebookId : selectedNotebookId;
        const effectiveTag =
          overrideTagId !== undefined ? overrideTagId : selectedTagId;

        let fetchedNotes: Note[] = [];

        if (effectiveNav === "tags" && effectiveTag) {
          fetchedNotes = await storageService.tags.getNotesForTag(effectiveTag);
        } else {
          let options: {
            includeDeleted?: boolean;
            notebookId?: string | null;
            unfiledOnly?: boolean;
            favoritesOnly?: boolean;
          } = {};

          if (effectiveNav === "trash") {
            options = { includeDeleted: true };
          } else if (effectiveNav === "unfiled") {
            options = { unfiledOnly: true };
          } else if (effectiveNav === "favorites") {
            options = { favoritesOnly: true };
          } else if (effectiveNav === "notebook" && effectiveNotebook) {
            options = { notebookId: effectiveNotebook };
          }

          fetchedNotes = await storageService.notes.list(options);
        }

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
    [activeNavId, selectedNotebookId, selectedTagId, isStorageReady]
  );

  useEffect(() => {
    if (isStorageReady) {
      loadNotes();
      refreshNoteCounts();
    }
  }, [isStorageReady, loadNotes, refreshNoteCounts]);

  const isCreatingRef = useRef(false);

  // Persistent note creation (Task 18: Assign to selected notebook or unfiled; auto-tag if in tag view)
  const handleNewNoteAction = useCallback(async () => {
    if (isCreatingRef.current) return;
    isCreatingRef.current = true;

    try {
      // If user was viewing trash or has an active search filter, reset to all-notes to reveal the new note
      if (activeNavId === "trash") {
        setActiveNavId("all-notes");
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
      clearSearch();
      searchInputRef.current?.blur();
    } else {
      searchInputRef.current?.blur();
    }
  }, [isSettingsOpen, searchQuery, clearSearch]);

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

  const handleToggleNoteFavorite = useCallback(
    async (noteId: string) => {
      const note = notes.find((n) => n.id === noteId);
      if (!note) return;
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

      const notebookPath = n.notebook_id
        ? computeNotebookPath(n.notebook_id, notebooks)
        : undefined;

      return {
        id: n.id,
        title: n.title.trim() ? n.title : "Untitled Note",
        preview,
        updatedAt,
        isFavorite: n.is_favorite,
        notebookId: n.notebook_id ?? undefined,
        notebookPath: notebookPath && notebookPath !== "Unfiled" ? notebookPath : undefined,
        tags: noteTagsMap[n.id] ?? [],
      };
    });
  }, [filteredNotes, noteTagsMap, notebooks]);

  // Unified items for NotesList: real SQLite search results when searching, standard notes when browsing (Task 14)
  const displayedNoteListItems = useMemo<NoteListItem[]>(() => {
    if (isSearchActive) {
      return searchResults.map((result) => {
        const nbPath = computeNotebookPath(result.notebookId, notebooks);
        return searchResultToNoteListItem(result, nbPath, noteTagsMap[result.noteId]);
      });
    }
    return noteListItems;
  }, [isSearchActive, searchResults, noteListItems, notebooks, noteTagsMap]);

  // Unified status for NotesList (idle | loading | error | empty)
  const displayedStatus = useMemo<NotesListStatus>(() => {
    if (isSearchActive) {
      if (searchState.status === "searching") return "loading";
      if (searchState.status === "error") return "error";
      if (searchState.status === "success") {
        return searchResults.length === 0 ? "empty" : "idle";
      }
      return "idle";
    }
    return status;
  }, [isSearchActive, searchState, searchResults.length, status]);

  // Automatically select the first search result on search completion if no valid selection exists
  useEffect(() => {
    if (isSearchActive && searchState.status === "success") {
      if (searchResults.length > 0) {
        setSelectedNoteId((prev) =>
          prev && searchResults.some((r) => r.noteId === prev) ? prev : searchResults[0].noteId
        );
      } else {
        setSelectedNoteId(null);
      }
    }
  }, [isSearchActive, searchState, searchResults]);

  const selectedNote = useMemo(() => {
    if (!selectedNoteId) return null;
    return (
      filteredNotes.find((n) => n.id === selectedNoteId) ??
      notes.find((n) => n.id === selectedNoteId) ??
      null
    );
  }, [filteredNotes, notes, selectedNoteId]);

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
      setSearchQuery("");
      await loadNotes("tags", null, tagId);
    },
    [selectTag, loadNotes]
  );

  const getSectionTitle = () => {
    if (searchQuery.trim()) return `Search: "${searchQuery}"`;
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
              setSearchQuery("");
              await loadNotes("notebook", nbId, null);
            }}
            onRetryNotebooks={loadNotebooks}
            onCreateNotebook={handleOpenCreateNotebook}
            onRenameNotebook={setRenamingNotebook}
            onDeleteNotebook={setDeletingNotebook}
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
          <NotesList
            title={getSectionTitle()}
            notes={displayedNoteListItems}
            selectedNoteId={selectedNote?.id ?? null}
            onSelectNote={async (noteId) => {
              cancelPendingDebounce();
              if (noteId !== selectedNoteId) {
                if (editorSaveRef.current) {
                  await editorSaveRef.current();
                }
                setSelectedNoteId(noteId);
              }
            }}
            status={displayedStatus}
            errorTitle={isSearchActive ? "Search failed" : undefined}
            errorMessage={
              isSearchActive && searchState.status === "error"
                ? searchState.message
                : errorMessage
            }
            emptyTitle={isSearchActive ? "No notes found" : emptyState.title}
            emptyDescription={
              isSearchActive
                ? `No notes match "${searchQuery}".`
                : emptyState.description
            }
            mode={isSearchActive ? "search" : "normal"}
            searchQuery={searchQuery}
            resultCountLabel={
              isSearchActive ? formatResultCount(searchResults.length) : undefined
            }
            onRetry={isSearchActive ? () => searchNow() : handleRetry}
            onNewNote={isSearchActive ? undefined : handleNewNoteAction}
            onToggleFavorite={handleToggleNoteFavorite}
          />
        }
        editor={
          <NoteEditor
            noteId={selectedNote?.id ?? null}
            onSaveNote={handleSaveNote}
            onRegisterSave={handleRegisterSave}
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
    </>
  );
}

export default App;
