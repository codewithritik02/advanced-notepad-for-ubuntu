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
import { storageService, Note, Notebook, Tag, NoteFormat } from "../services/storage";

export function App() {
  const { theme, setTheme, toggleTheme } = useTheme();
  const [activeNavId, setActiveNavId] = useState<NavItemId>("all-notes");
  const [searchQuery, setSearchQuery] = useState("");
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [notes, setNotes] = useState<Note[]>([]);
  const [notebooks, setNotebooks] = useState<Notebook[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [selectedNoteId, setSelectedNoteId] = useState<string | null>(null);
  const [isStorageReady, setIsStorageReady] = useState(false);
  const [status, setStatus] = useState<NotesListStatus>("loading");
  const [errorMessage, setErrorMessage] = useState("Unable to load notes.");

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

  // Load metadata (notebooks and tags) from SQLite
  const loadMetadata = useCallback(async () => {
    if (!isStorageReady) return;
    try {
      const [nbList, tagList] = await Promise.all([
        storageService.notebooks.list(),
        storageService.tags.list(),
      ]);
      setNotebooks(nbList);
      setTags(tagList);
    } catch {
      // Non-fatal error for metadata
    }
  }, [isStorageReady]);

  // Load persistent notes from SQLite
  const loadNotes = useCallback(async () => {
    if (!isStorageReady) return;
    setStatus("loading");
    try {
      const isTrash = activeNavId === "trash";
      const fetchedNotes = await storageService.notes.list(isTrash);
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
  }, [activeNavId, isStorageReady]);

  useEffect(() => {
    if (isStorageReady) {
      loadNotes();
      loadMetadata();
    }
  }, [isStorageReady, loadNotes, loadMetadata]);

  const isCreatingRef = useRef(false);

  // Persistent note creation
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

      const created = await storageService.notes.create({
        title: "Untitled Note",
        content: "",
        format: "txt",
      });
      setNotes((prev) => [created, ...prev]);
      setSelectedNoteId(created.id);
      setStatus("idle");
    } catch (err) {
      setErrorMessage(
        "Failed to create note: " +
          (err instanceof Error ? err.message : String(err))
      );
      setStatus("error");
    } finally {
      isCreatingRef.current = false;
    }
  }, [activeNavId, searchQuery]);

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

  // Filter notes based on active sidebar section and search query
  const filteredNotes = useMemo(() => {
    return notes.filter((n) => {
      if (activeNavId === "favorites" && !n.is_favorite) return false;
      if (activeNavId === "trash" && !n.is_deleted) return false;
      if (activeNavId !== "trash" && n.is_deleted) return false;

      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        return (
          n.title.toLowerCase().includes(q) ||
          n.content.toLowerCase().includes(q)
        );
      }
      return true;
    });
  }, [notes, activeNavId, searchQuery]);

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

  const getSectionTitle = () => {
    if (searchQuery.trim()) return `Search: "${searchQuery}"`;
    switch (activeNavId) {
      case "favorites":
        return "Favorites";
      case "notebooks":
        return notebooks.length > 0 ? `Notebooks (${notebooks.length})` : "Notebooks";
      case "tags":
        return tags.length > 0 ? `Tags (${tags.length})` : "Tags";
      case "trash":
        return "Trash";
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
    }
  };

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
            onSelectNav={(navId) => {
              setActiveNavId(navId);
              setSearchQuery("");
            }}
            onNewNoteClick={handleNewNoteAction}
          />
        }
        notesList={
          <NotesList
            title={getSectionTitle()}
            notes={noteListItems}
            selectedNoteId={selectedNote?.id ?? null}
            onSelectNote={setSelectedNoteId}
            status={status}
            errorMessage={errorMessage}
            onRetry={handleRetry}
            onNewNote={handleNewNoteAction}
          />
        }
        editor={
          <NoteEditor
            noteId={selectedNote?.id ?? null}
            onSaveNote={handleSaveNote}
            onRegisterSave={handleRegisterSave}
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
    </>
  );
}

export default App;
