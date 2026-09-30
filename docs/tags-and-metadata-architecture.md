# Personal Notepad — Tags, Favorites & Metadata Architecture

This document defines the architecture, data models, validation rules, relational semantics, and user experience specifications for the Tags, Favorites, and Metadata systems in **Personal Notepad**.

---

## 1. Tags Architecture

### 1.1 Many-to-Many Relational Model
Tags and notes share a many-to-many relationship mediated by the `note_tags` junction table in SQLite:

```text
┌─────────────────┐       ┌─────────────────┐       ┌─────────────────┐
│      notes      │ 1   * │    note_tags    │ *   1 │      tags       │
├─────────────────┤───────├─────────────────┤───────├─────────────────┤
│ id (PK)         │       │ note_id (PK, FK)│       │ id (PK)         │
│ title           │       │ tag_id (PK, FK) │       │ name (UNIQUE)   │
│ content         │       └─────────────────┘       │ created_at      │
│ ...             │                                 └─────────────────┘
└─────────────────┘
```

- **Schema Definition**:
  ```sql
  CREATE TABLE IF NOT EXISTS tags (
      id TEXT PRIMARY KEY NOT NULL,
      name TEXT NOT NULL UNIQUE,
      created_at TEXT NOT NULL
  );

  CREATE TABLE IF NOT EXISTS note_tags (
      note_id TEXT NOT NULL,
      tag_id TEXT NOT NULL,
      PRIMARY KEY (note_id, tag_id),
      FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE,
      FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
  );

  CREATE INDEX IF NOT EXISTS idx_note_tags_tag_id ON note_tags(tag_id);
  ```

### 1.2 Naming Rules & Scalar Validation
- **Length Constraint**: Tag names must contain between 1 and 100 characters inclusive (`1 <= length <= 100`).
- **Whitespace Handling**: Leading and trailing whitespace is automatically trimmed. Empty strings or strings composed solely of whitespace are rejected with `StorageError::Validation`.
- **Character Support**:
  - Full Unicode code-point counting in both TypeScript (`Array.from(trimmed).length`) and Rust (`trimmed.chars().count()`).
  - Supports non-Latin scripts (e.g. `यात्रा`, `旅行`).
  - Supports accented characters (e.g. `café`).
  - Supports complex technical symbols, slashes, and punctuation (e.g. `C++`, `C#`, `R&D`, `Project / Planning`, `Q4 2026`).

### 1.3 Duplicate Handling & Deduplication
- **Case-Insensitive Uniqueness**: Tags are compared case-insensitively using `LOWER(name)`.
- **Creation Idempotency**: Calling `TagRepository::create` with an existing tag name (even with differing case, e.g., `Work` vs `work`) returns the existing tag record rather than creating a duplicate or failing.
- **Rename Conflict Prevention**: Renaming an existing tag to a name that already belongs to another tag returns `StorageError::Conflict("A tag with the name '...' already exists")`. The UI surfaces this as `"A tag with this name already exists."`.
- **Junction Deduplication**: Assigning a tag to a note uses `INSERT OR IGNORE INTO note_tags (note_id, tag_id)`, preventing duplicate pairings at both repository and database levels.

### 1.4 Deletion Behavior & Cascade Isolation
- **Atomic Transactional Deletion**: Tag deletion executes in an atomic SQLite transaction:
  ```sql
  BEGIN TRANSACTION;
      DELETE FROM note_tags WHERE tag_id = ?1;
      DELETE FROM tags WHERE id = ?1;
  COMMIT;
  ```
- **Note Preservation**: Deleting a tag **never** deletes or modifies any note rows. Notes simply lose that tag from their assigned tags list.
- **Hard Deletes**: Hard-deleting a note automatically cascade-deletes all corresponding records in `note_tags` without leaving orphan rows.
- **Soft Deletes**: Soft-deleted notes (`is_deleted = 1`) retain their tag assignments in the database but are strictly excluded from tag filter views and sidebar tag count badges.

---

## 2. Favorites Architecture

### 2.1 Schema & Storage
- Favorites are stored as a binary integer column on the `notes` table:
  ```sql
  is_favorite INTEGER NOT NULL DEFAULT 0
  ```
  Indexed via:
  ```sql
  CREATE INDEX IF NOT EXISTS idx_notes_is_favorite ON notes(is_favorite);
  ```
- Toggle action is atomic and toggles between `0` (false) and `1` (true).

### 2.2 Navigation & Filtering Behavior
- **Single Primary Navigation Context**: "Favorites" operates as a top-level sidebar navigation target alongside "All Notes", "Notebooks", and individual "Tags".
- **Query Filter**: Selecting "Favorites" queries `SELECT ... FROM notes WHERE is_deleted = 0 AND is_favorite = 1 ORDER BY is_pinned DESC, modified_at DESC`.
- **Empty State**: When no notes are favorited, a dedicated empty state displays:
  - Header: *"No Favorites Yet"*
  - Description: *"Click the star icon on any note to add it to your favorites for quick access."*
- **UI Interactions**:
  - Gold star button in editor header bar toggles favorite state with optimistic updates and ARIA accessibility labels (`"Add to favorites"` vs `"Remove from favorites"`).
  - Compact star button on NoteCard in the notes list allows toggling favorites directly without opening the note.

---

## 3. Note Metadata Architecture

### 3.1 Metadata Hierarchy & Model
The canonical metadata for a note is encapsulated by `NoteMetadata`:

```typescript
export interface NoteMetadata {
  id: string;
  title: string;
  notebookId: string | null;
  notebookPath: string; // e.g., "Work / Projects / Q4" or "Unfiled"
  tags: Tag[];
  isFavorite: boolean;
  format: "txt" | "md";
  createdAt: string;    // ISO-8601
  modifiedAt: string;   // ISO-8601
  wordCount: number;
  characterCount: number;
  byteSize: number;
}
```

### 3.2 Editable vs. Read-Only Fields
To prevent accidental data corruption and preserve system integrity, strict boundaries delineate editable and read-only metadata fields:

| Field | Nature | Modification Method |
|---|---|---|
| **Title** | Editable | Editor title input (autosaved on debounce / switch) |
| **Content** | Editable | Editor textarea (autosaved on debounce / switch) |
| **Notebook** | Editable | "Move" modal / notebook selector dropdown |
| **Tags** | Editable | `TagPicker` chip strip / modal manager |
| **Favorite** | Editable | Star toggle button in header / list card |
| **Created At** | **Read-Only** | Generated once on note insertion; immutable |
| **Modified At** | **Read-Only** | System-managed; updated on content/title changes |
| **Format** | **Read-Only** | Fixed on note creation (`Plain Text` or `Markdown`) |
| **Word Count** | **Read-Only** | Calculated dynamically via deterministic text tokenizer |
| **Character Count** | **Read-Only** | Calculated dynamically via Unicode scalar length |
| **Byte Size** | **Read-Only** | Calculated dynamically via UTF-8 byte encoder |

### 3.3 Timestamp Behavior
- `created_at`: Retains the exact ISO-8601 timestamp of note creation forever. It is never overwritten by updates, moves, renames, or tag modifications.
- `modified_at`: Updates automatically whenever the user edits note text or title. It is intentionally preserved when notes are moved between notebooks or when tags are attached/removed, ensuring note list sorting reflects actual user authoring activity.
- **Display Formatting**: Rendered in human-readable, locale-aware formats (e.g. `30 Sep 2026, 17:15`).
