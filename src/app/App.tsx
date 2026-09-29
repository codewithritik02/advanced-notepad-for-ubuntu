import { useState, useRef, useCallback } from "react";
import { MainLayout } from "../layouts/MainLayout";
import { TopBar } from "../components/TopBar/TopBar";
import { Sidebar, NavItemId } from "../components/Sidebar/Sidebar";
import { NotesList } from "../features/notes/components/NotesList/NotesList";
import { EditorPlaceholder } from "../features/notes/components/Editor/EditorPlaceholder";
import { SettingsModal } from "../features/settings/components/SettingsModal/SettingsModal";
import { MOCK_NOTES } from "../features/notes/mockNotes";
import { useTheme } from "../hooks/useTheme";
import { useKeyboardShortcuts } from "../hooks/useKeyboardShortcuts";

export function App() {
  const { theme, setTheme, toggleTheme } = useTheme();
  const [activeNavId, setActiveNavId] = useState<NavItemId>("all-notes");
  const [searchQuery, setSearchQuery] = useState("");
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [selectedNoteId, setSelectedNoteId] = useState<string | null>(
    MOCK_NOTES[0]?.id ?? null
  );

  const searchInputRef = useRef<HTMLInputElement>(null);

  // Keyboard shortcut actions
  const handleNewNoteAction = useCallback(() => {
    // UI action placeholder for future note creation
  }, []);

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

  // Derive title and notes based on active sidebar navigation
  const getNavDetails = () => {
    switch (activeNavId) {
      case "favorites":
        return {
          title: "Favorites",
          notes: MOCK_NOTES.filter((n) => n.isFavorite),
        };
      case "notebooks":
        return {
          title: "Notebooks",
          notes: MOCK_NOTES,
        };
      case "tags":
        return {
          title: "Tags",
          notes: MOCK_NOTES,
        };
      case "trash":
        return {
          title: "Trash",
          notes: [],
        };
      case "all-notes":
      default:
        return {
          title: "All Notes",
          notes: MOCK_NOTES,
        };
    }
  };

  const { title, notes } = getNavDetails();

  // Visual search filtering on temporary mock data (UI state only)
  const displayedNotes = searchQuery.trim()
    ? notes.filter(
        (n) =>
          n.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
          n.preview.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : notes;

  const selectedNote =
    displayedNotes.find((n) => n.id === selectedNoteId) ??
    (displayedNotes.length > 0 ? displayedNotes[0] : null);

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
              if (navId === "trash") {
                setSelectedNoteId(null);
              } else if (navId === "favorites") {
                const firstFav = MOCK_NOTES.find((n) => n.isFavorite);
                setSelectedNoteId(firstFav?.id ?? null);
              } else {
                setSelectedNoteId(MOCK_NOTES[0]?.id ?? null);
              }
            }}
            onNewNoteClick={() => {
              // Visual action placeholder for future note creation
            }}
          />
        }
        notesList={
          <NotesList
            title={searchQuery.trim() ? `Search: "${searchQuery}"` : title}
            notes={displayedNotes}
            selectedNoteId={selectedNote?.id ?? null}
            onSelectNote={setSelectedNoteId}
          />
        }
        editor={<EditorPlaceholder selectedNote={selectedNote} />}
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
