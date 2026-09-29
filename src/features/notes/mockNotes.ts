import { NoteListItem } from "./types";

/**
 * TEMPORARY MOCK DATA FOR UI DEVELOPMENT ONLY
 *
 * This mock data is strictly isolated to allow developing and testing the UI shell
 * without a database. It will be replaced entirely in Phase 2 by SQLite and local storage.
 */
export const MOCK_NOTES: NoteListItem[] = [
  {
    id: "note-1",
    title: "Welcome to Personal Notepad",
    preview: "This is your first note. Personal Notepad is an offline-first, private desktop workspace...",
    updatedAt: "Today, 10:45 AM",
    isFavorite: true,
    tags: ["getting-started"],
  },
  {
    id: "note-2",
    title: "Project Ideas & Architecture",
    preview: "1. Cross-platform Tauri desktop client\n2. Hybrid SQLite + filesystem storage\n3. Zero cloud dependencies...",
    updatedAt: "Yesterday",
    isFavorite: true,
    tags: ["architecture", "dev"],
  },
  {
    id: "note-3",
    title: "Weekly Grocery & Supplies",
    preview: "Milk, sourdough bread, organic eggs, green tea, dark roast coffee beans...",
    updatedAt: "Monday",
    isFavorite: false,
    tags: ["personal"],
  },
  {
    id: "note-4",
    title: "Design System Guidelines",
    preview: "Follow desktop-first UI principles. Maintain 3-pane layout integrity with clean contrast and typography...",
    updatedAt: "Sep 25",
    isFavorite: false,
    tags: ["design"],
  },
  {
    id: "note-5",
    title: "Meeting Notes: Sprint Planning",
    preview: "Discussed Phase 1 milestones: Project setup, application shell, navigation, notes list, editor placeholder...",
    updatedAt: "Sep 22",
    isFavorite: false,
    tags: ["work"],
  },
];
