# Note Editor Architecture

This document describes the architectural design and data flow for the **Note Creation & Editing Experience** in Personal Notepad (Phase 3).

---

## 1. Architectural Overview & Boundaries

The editor strictly follows the unidirectional architecture established in Phase 1 and Phase 2. Presentation components never execute SQL or access the filesystem directly; all persistent interactions traverse a type-safe IPC boundary into Rust repositories.

```text
┌────────────────────────────────────────────────────────┐
│               React Editor Component                   │
│      (Title Input, Native Textarea, Format Strip)      │
└───────────────────────────┬────────────────────────────┘
                            │ Temporary In-Memory Edits
                            ▼
┌────────────────────────────────────────────────────────┐
│            Editor State & Autosave Hook                │
│    - Title, Content, Format, Cursor & Selection        │
│    - Dirty State Tracking (isDirty: boolean)           │
│    - Autosave Debounce (500ms - 1500ms)                │
│    - Version / Sequential Request Guard against Stale  │
└───────────────────────────┬────────────────────────────┘
                            │ Strongly Typed Calls
                            ▼
┌────────────────────────────────────────────────────────┐
│            Frontend Storage Service Layer              │
│               (src/services/storage/notes.ts)          │
└───────────────────────────┬────────────────────────────┘
                            │ Tauri IPC (invoke)
                            ▼
┌────────────────────────────────────────────────────────┐
│              Tauri v2 IPC Command Layer                │
│          (src-tauri/src/commands/storage.rs)           │
└───────────────────────────┬────────────────────────────┘
                            │ Direct Rust Repository Call
                            ▼
┌────────────────────────────────────────────────────────┐
│               Rust Note Repository Layer               │
│      (src-tauri/src/storage/repositories/notes.rs)     │
└───────────────────────────┬────────────────────────────┘
                            │ 100% Parameterized SQL
                            ▼
┌────────────────────────────────────────────────────────┐
│                  SQLite Database File                  │
│       (database.sqlite - notes table in WAL mode)      │
└────────────────────────────────────────────────────────┘
```

---

## 2. In-Memory Editor State vs. Persistent SQLite State

To ensure a responsive typing experience without excessive database writes or frame drops:
- **Temporary State**: Maintained locally in React during typing (`title`, `content`, `format`, `isDirty`, `isSaving`, `lastSavedAt`, `saveError`).
- **Persistent State**: SQLite is the single source of truth for saved notes.
- **Autosave Debounce**: Text changes update local React state immediately and trigger a debounced persistence task.
- **Stale Save Protection**: Each save operation tracks an incremental revision token. If a later edit occurs while an asynchronous IPC save is in flight, older responses cannot overwrite newer content.
- **Safe Error Recovery**: If an IPC or SQLite error occurs during save, the dirty changes remain intact in memory; the UI reflects an unobtrusive save-error indicator with a retry action. Content is never discarded.

---

## 3. Note Data Model & Format Support

The editor interfaces with the Phase 2 `notes` table schema:

```text
Note {
    id: string;               // UUID v4
    title: string;            // Default "Untitled Note"
    content: string;          // Plain text / Markdown content
    format: "txt" | "md";     // "txt" default, "md" secondary
    notebook_id: string | null;
    created_at: string;       // ISO 8601 UTC timestamp (stable)
    modified_at: string;      // ISO 8601 UTC timestamp (updates on save)
    is_favorite: boolean;
    is_pinned: boolean;
    is_deleted: boolean;
    deleted_at: string | null;
}
```

### Formats:
- **TXT (`txt`)**: Plain text. Preserves whitespace, indentation, newlines, and Unicode characters exactly as typed without formatting or HTML transformation.
- **Markdown (`md`)**: Structured text. Stored as raw Markdown without forced conversion to HTML.

---

## 4. Lifecycle & Interaction Sequences

### A. New Note Creation
1. User clicks `+ New Note` (or presses `Ctrl/Cmd + N`).
2. Handler dispatches `storageService.notes.create({ title: "Untitled Note", content: "", format: "txt" })`.
3. Rust generates a UUID, stamps `created_at` and `modified_at`, and inserts the record into SQLite.
4. The newly created `Note` is prepended to the notes list and immediately set as the active note.
5. The editor mounts with the new note and focuses the title/content.

### B. Typing & Autosave Flow
1. User types in the note title or content.
2. In-memory editor state updates; `isDirty` becomes `true`.
3. Autosave timer is debounced (e.g. 800ms).
4. When timer fires:
   - `isSaving` becomes `true`.
   - Dispatch `storageService.notes.update(noteId, { title, content, format })`.
   - On success: `isDirty` becomes `false`, `isSaving` becomes `false`, `lastSavedAt` updates, and the notes list item is updated in place.
   - On error: `isSaving` becomes `false`, `isDirty` remains `true`, and an error banner is presented. In-memory text is preserved.

### C. Note Switching Safety & View State Memory
1. User selects Note B while Note A has `isDirty = true`.
2. Editor immediately captures Note A's cursor selection (`selectionStart`, `selectionEnd`) and scroll offset (`scrollTop`) into session view cache.
3. Editor flushes/saves Note A to SQLite via Tauri IPC.
4. The editor loads Note B, restoring Note B's previous cursor and scroll position if visited earlier in the session.
5. No unsaved changes or cursor positions are lost.

### D. Undo / Redo & Native Editing
1. Native WebKit `<input>` and `<textarea>` undo/redo histories are fully preserved across autosaves by avoiding unnecessary DOM value resets.
2. Application-level shortcut listeners in `useKeyboardShortcuts` do not capture or intercept `Ctrl+Z`, `Ctrl+Y`, or `Ctrl+Shift+Z`.
3. Toolbar contains dedicated interactive Undo and Redo buttons utilizing `document.execCommand` with `preventDefault` on mousedown to preserve focus.

### E. Large Text Handling & Zero-Allocation Word Counting
1. Supports large notes (10 KB, 100 KB, 500 KB, 1 MB+) without memory exhaustion or input lag.
2. Uses an $O(N)$ single-pass character scanning word counter (`countWords(text)`) wrapped in `useMemo`, eliminating heap string allocations on keystrokes.
3. List item preview generation slices the first 300 characters prior to whitespace normalization, preventing multi-megabyte string duplications.

---

## 5. Non-Scope Boundaries for Phase 3

As strictly defined in the product roadmap:
- ❌ **No AI / LLMs**: 100% offline, local-first operation.
- ❌ **No Cloud / Sync**: Zero network calls, telemetry, or remote sync.
- ❌ **No Search Engine**: FTS5 search indexing is deferred to Phase 6.
- ❌ **No Notebook Tree UI**: Folder hierarchy editing is deferred to Phase 4.
- ❌ **No Tag Management UI**: Tag selector and chip editing are deferred to Phase 5.
- ❌ **No Attachments / Media**: Deferred to Phase 7.
- ❌ **No Trash Recovery UI**: Deferred to Phase 8.

---

## 6. Verification Status & Test Suite

All 23 sequential tasks of Phase 3 are 100% completed and verified:
- **Rust Automated Test Suite**: 27 automated tests (12 unit + 15 integration) passing in `src-tauri`.
- **Frontend Build**: Zero TypeScript errors (`tsc && vite build`) passing cleanly.
- **SQLite Persistence**: Verified across connection drops, app restarts, note switching, and large payloads up to 1 MB.
