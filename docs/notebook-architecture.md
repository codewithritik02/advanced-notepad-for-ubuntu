# Notebook & Organization Architecture (Phase 4)

This document describes the architectural design, data models, lifecycle operations, and safety guarantees for the **Notebook / Folder System** in Personal Notepad.

---

## 1. Architectural Overview & Boundaries

The Notebook system integrates cleanly into Personal Notepad's local-first, offline architecture. Presentation components in React never manipulate database records directly; all notebook hierarchy calculations and state modifications traverse typed IPC boundaries into Rust repositories backed by SQLite with Write-Ahead Logging (WAL).

```text
┌────────────────────────────────────────────────────────┐
│            React UI & Sidebar Navigation               │
│   (NotebookTree, NotebookTreeItem, ContextMenu, Modals) │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│               Frontend Notebook Feature                │
│    - useNotebooks & useNotebookSelection hooks         │
│    - buildNotebookTree (cycle break, orphan-to-root)   │
│    - Privacy-safe logging utility                      │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│            Frontend Storage Service Layer              │
│            (src/services/storage/notebooks.ts)         │
└───────────────────────────┬────────────────────────────┘
                            │ Tauri IPC (invoke)
                            ▼
┌────────────────────────────────────────────────────────┐
│              Tauri v2 IPC Command Layer                │
│          (src-tauri/src/commands/storage.rs)           │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│             Rust Notebook & Note Repositories          │
│       (Atomic transactions, cycle checks, unfiling)    │
└───────────────────────────┬────────────────────────────┘
                            │ 100% Parameterized SQL
                            ▼
┌────────────────────────────────────────────────────────┐
│                  SQLite Database File                  │
│       (notebooks table, notes table foreign key)       │
└────────────────────────────────────────────────────────┘
```

---

## 2. Notebook Data Model & Schema

Notebooks are modeled as hierarchical nodes stored in SQLite with parent-pointer adjacency:

```sql
CREATE TABLE IF NOT EXISTS notebooks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE,
    parent_id TEXT,
    created_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,
    FOREIGN KEY (parent_id) REFERENCES notebooks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_notebooks_parent_id ON notebooks(parent_id);
```

### TypeScript Domain Model
```typescript
export interface Notebook {
  id: string;
  name: string;
  parent_id: string | null;
  created_at: string;
  modified_at: string;
}

export interface NotebookTreeNode extends Notebook {
  children: NotebookTreeNode[];
  depth: number;
  noteCount?: number;
}
```

---

## 3. Parent / Child Relationships & Nesting

- **Root Notebooks**: Have `parent_id = NULL`.
- **Nested Child Notebooks**: Point to an existing notebook via `parent_id = <parent_notebook_id>`.
- **Arbitrary Depth**: The hierarchy supports arbitrary nesting depths (Level 1 Root → Level 2 Child → Level 3 Grandchild → ...).
- **Sorting**: Notebooks are sorted deterministically at each level using Unicode locale-aware comparison (`localeCompare(..., { sensitivity: "base", numeric: true })`), cleanly supporting international scripts (e.g. Hindi, Japanese, accented characters, emojis).
- **Cycle Prevention**: The backend and frontend enforce acyclic graph constraints. Parent mutations that would create a circular dependency are rejected. In the frontend, defensive cycle detection (`wouldCreateCycle`) breaks any corrupted cycles by safely promoting loop nodes to root level.
- **Orphan Protection**: If a child references a nonexistent `parent_id`, `buildNotebookTree` safely promotes the node to root level to guarantee zero data invisibility.

---

## 4. Note-to-Notebook Relationship

Notes reference notebooks via a nullable foreign key on the `notes` table:

```sql
ALTER TABLE notes ADD COLUMN notebook_id TEXT REFERENCES notebooks(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_notes_notebook_id ON notes(notebook_id);
```

- When `notebook_id` is set, the note belongs to that specific notebook.
- When `notebook_id` is `NULL`, the note is categorized as **Unfiled**.
- Moving a note executes `UPDATE notes SET notebook_id = ? WHERE id = ?`. Moving preserves all other attributes (title, content, format, created_at, tags, favorite status) without modification.

---

## 5. Deletion Behavior & Note Safety Guarantee

> [!IMPORTANT]
> **Deleting a notebook NEVER deletes its notes.**

When a notebook is deleted:
1. An atomic SQLite transaction begins.
2. All notes belonging to the deleted notebook (or any of its cascaded child sub-notebooks) are reassigned to `notebook_id = NULL` (Unfiled):
   ```sql
   UPDATE notes SET notebook_id = NULL WHERE notebook_id = ?;
   ```
3. The notebook record is removed from `notebooks`. If foreign key cascade is active, child notebooks are cleaned up while their notes are also safely unfiled.
4. The transaction commits. If any step fails, the entire transaction is rolled back, preventing any partial or corrupt state.
5. In the UI, former child notes remain immediately accessible under the **Unfiled** navigation section.

---

## 6. Navigation Semantics: All Notes vs. Unfiled vs. Notebooks

The navigation state model differentiates between three primary views to avoid ambiguous `null` states:

1. **All Notes (`activeNavId === "all-notes"`)**:
   - Queries and displays all non-deleted notes across the entire application, regardless of whether they belong to a notebook or are unfiled.
2. **Unfiled Notes (`activeNavId === "unfiled"`)**:
   - Queries and displays only notes where `notebook_id IS NULL`.
   - Serves as the default staging inbox for new notes or notes freed by notebook deletions.
3. **Specific Notebook (`activeNavId === "notebook"` & `selectedNotebookId === "<uuid>"`)**:
   - Displays only notes directly assigned to that notebook (`notebook_id = ?`).
   - The editor displays a notebook badge in the toolbar reflecting the notebook's name and allowing 1-click moves.

---

## 7. Notebook Creation Behavior

- **Root Notebook**: Created by clicking the `+` action button in the Sidebar "Notebooks" section header, or choosing "New Notebook" when no parent is selected.
- **Sub-Notebook**: Created by right-clicking any existing notebook in the tree and selecting "New Sub-notebook", or using the modal with `parentId` pre-populated.
- **Validation**:
  - Name must be non-empty after trimming (1 to 255 characters).
  - Trailing and leading whitespace is trimmed automatically.
  - Slashes, dots, unicode, and special symbols are valid notebook names.
- **Creation Context**: When a notebook is selected and the user creates a new note, that new note is automatically assigned to the active notebook.

---

## 8. Limitations & Scope Boundaries (Phase 4)

- **Local-First & Offline Only**: Notebook metadata and notes are stored strictly on the user's local disk in SQLite. There is no cloud sync, remote backup, or third-party telemetric transmission.
- **Folder / Notebook Focus**: Notebooks serve as organizational containers. Tagging and full-text search remain complementary orthogonal systems.
- **Single Notebook Membership**: Each note belongs to at most one notebook (`notebook_id` is a scalar reference, not a many-to-many relationship). Multi-categorization is supported via Tags.
