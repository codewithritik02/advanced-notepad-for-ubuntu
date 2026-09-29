import { useState, useRef, useCallback, useEffect, useMemo } from "react";
import { MainLayout } from "../layouts/MainLayout";
import { TopBar } from "../components/TopBar/TopBar";
import { Sidebar, NavItemId } from "../components/Sidebar/Sidebar";
import { NotesList } from "../features/notes/components/NotesList/NotesList";
import { NoteListItem, NotesListStatus } from "../features/notes/types";
import { EditorPlaceholder } from "../features/notes/components/Editor/EditorPlaceholder";
import { SettingsModal } from "../features/settings/components/SettingsModal/SettingsModal";
import { useTheme } from "../hooks/useTheme";
import { useKeyboardShortcuts } from "../hooks/useKeyboardShortcuts";
import { storageService, Note, Notebook, Tag } from "../services/storage";

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

  // Persistent note creation
  const handleNewNoteAction = useCallback(async () => {
    try {
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
    }
  }, []);

  // Persistent note update
  const handleSaveNote = useCallback(
    async (id: string, title: string, content: string) => {
      try {
        const updated = await storageService.notes.update(id, { title, content });
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

  useKeyboardShortcuts({
    onNewNote: handleNewNoteAction,
    onFocusSearch: handleFocusSearch,
    onOpenSettings: handleOpenSettings,
    onEscape: handleEscape,
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
    return filteredNotes.map((n) => ({
      id: n.id,
      title: n.title || "Untitled Note",
      preview: n.content.trim() ? n.content.slice(0, 120) : "No additional text",
      updatedAt: n.modified_at
        ? new Date(n.modified_at).toLocaleDateString([], {
            month: "short",
            day: "numeric",
          })
        : "Just now",
      isFavorite: n.is_favorite,
      notebookId: n.notebook_id ?? undefined,
    }));
  }, [filteredNotes]);

  const selectedNote = useMemo(() => {
    return (
      filteredNotes.find((n) => n.id === selectedNoteId) ??
      (filteredNotes.length > 0 ? filteredNotes[0] : null)
    );
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
          />
        }
        editor={
          <EditorPlaceholder
            selectedNote={selectedNote}
            onSaveNote={handleSaveNote}
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
