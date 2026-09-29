# Personal Notepad

Personal Notepad is a fast, offline-first desktop note-taking application designed for privacy, reliability, and cross-platform flexibility. It operates completely independently of cloud infrastructure, mandatory user accounts, telemetry, and external AI integrations.

---

## Current Status

**Phase 2 — Storage Architecture + SQLite (Completed)**

The storage foundation is fully established and operational across the Rust desktop runtime and the React frontend. Notes, nested notebooks, tags, note-tag associations, and theme settings are backed by an ACID-compliant local SQLite database with write-ahead logging (WAL), strict foreign key constraints, atomic transactions, parameterized queries, and idempotent migrations.

---

## Architecture

The application enforces a strict unidirectional storage flow. Presentation components never execute SQL or access the filesystem directly; all persistent interactions traverse a type-safe IPC boundary into Rust repositories.

### Runtime & Storage Stack

```text
┌────────────────────────────────────────────────────────┐
│               React 19 / TypeScript (Vite)             │
│   (App, Features: Notes, Notebooks, Tags, Settings)    │
└───────────────────────────┬────────────────────────────┘
                            │ Strongly Typed Calls
                            ▼
┌────────────────────────────────────────────────────────┐
│           Frontend Storage Service Layer               │
│        (src/services/storage/: notes, tags, etc.)      │
└───────────────────────────┬────────────────────────────┘
                            │ Tauri IPC (invoke)
                            ▼
┌────────────────────────────────────────────────────────┐
│             Tauri v2 IPC Command Layer                 │
│         (src-tauri/src/commands/storage.rs)            │
└───────────────────────────┬────────────────────────────┘
                            │ Direct Rust Calls
                            ▼
┌────────────────────────────────────────────────────────┐
│            Rust Storage & Repository Layer             │
│        (src-tauri/src/storage/repositories/)           │
│   - Connection Pool & WAL  - Schema & Idempotent Migr. │
│   - 100% Parameterized SQL - Atomic Multi-table Tx     │
│   - Path Traversal Shield  - User-Friendly Error Map   │
└───────────────────────────┬────────────────────────────┘
                            │
              +-------------+-------------+
              │                           │
              ▼                           ▼
┌───────────────────────────┐ ┌───────────────────────────┐
│   SQLite Database File    │ │     Local Filesystem      │
│     (database.sqlite)     │ │ (attachments/, backups/,  │
│  - notes, notebooks, tags │ │          config/)         │
│  - note_tags, settings    │ │                           │
└───────────────────────────┘ └───────────────────────────┘
```

### Security & Privacy Directives

1. **Zero Raw SQL Leaks**: The frontend has no generic `execute_sql` bridge. Every database operation is bound to a dedicated, strongly typed Rust IPC command.
2. **100% Parameterized Queries**: All SQL statements use positional or named parameters (`?1, ?2`). Zero string formatting or concatenation is permitted.
3. **Atomic Multi-Table Transactions**: Compound operations (e.g. creating a note with multiple tags) run inside SQLite transactions (`BEGIN TRANSACTION ... COMMIT`) with automatic rollback on any failure.
4. **Path Traversal Protection**: Note attachment directories and backup filenames reject path traversal patterns (`..`, `/`, `\`) before filesystem operations.
5. **No Cloud, No Telemetry, No AI**: 100% offline, local-first operation. No network telemetry, third-party analytics, or external LLM API calls.

---

## Storage & Filesystem Architecture

### User Data Directory Structure

On application launch, the desktop runtime resolves and idempotently initializes the following layout in the user's data directory:

- **Linux**: `~/.local/share/com.personalnotepad.app/` (via `$XDG_DATA_HOME`)
- **Windows**: `%APPDATA%\com.personalnotepad.app\` (or `%LOCALAPPDATA%`)
- **macOS**: `~/Library/Application Support/com.personalnotepad.app/`

```text
com.personalnotepad.app/
├── database.sqlite         # SQLite database file (WAL mode enabled)
├── database.sqlite-wal     # Write-Ahead Log
├── database.sqlite-shm     # Shared memory index
├── attachments/            # Note attachment directory (subdirectories by note_id)
│   └── <note-id>/
├── backups/                # Local database snapshots and exported archives
└── config/                 # Platform configuration overrides
```

### SQLite Schema (`v1`)

```sql
-- Migration tracking
CREATE TABLE _migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL
);

-- Notebooks (hierarchical with self-referential parent_id)
CREATE TABLE notebooks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    parent_id TEXT,
    created_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,
    FOREIGN KEY (parent_id) REFERENCES notebooks(id) ON DELETE CASCADE
);

-- Notes table
CREATE TABLE notes (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    format TEXT NOT NULL DEFAULT 'markdown',
    notebook_id TEXT,
    created_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,
    is_favorite INTEGER NOT NULL DEFAULT 0,
    is_pinned INTEGER NOT NULL DEFAULT 0,
    is_deleted INTEGER NOT NULL DEFAULT 0,
    deleted_at TEXT,
    FOREIGN KEY (notebook_id) REFERENCES notebooks(id) ON DELETE SET NULL
);

-- Tags
CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

-- Many-to-many relationship between Notes and Tags
CREATE TABLE note_tags (
    note_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (note_id, tag_id),
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

-- Attachments metadata
CREATE TABLE attachments (
    id TEXT PRIMARY KEY,
    note_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    mime_type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
);

-- Application settings key-value store
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

---

## Project Structure

```text
custom-notepad/
├── src/
│   ├── app/                    # Application root & shell orchestrator (App.tsx)
│   ├── components/             # Global layout & structural components
│   │   ├── Sidebar/            # Navigation sidebar (notes, notebooks, tags, trash)
│   │   ├── TopBar/             # Application top bar, search bar, and action triggers
│   │   └── ui/                 # Reusable UI primitives (Button, Modal, Badge, StateViews)
│   ├── features/               # Domain-specific feature modules
│   │   ├── notes/              # Notes list, card views, mock data fixture, and editor placeholder
│   │   ├── notebooks/          # Notebook organization
│   │   ├── tags/               # Tag organization
│   │   ├── search/             # Search infrastructure
│   │   ├── attachments/        # File attachment infrastructure
│   │   └── settings/           # Settings modal and theme controls
│   ├── layouts/                # Main 3-pane desktop layout (Sidebar - NotesList - Editor)
│   ├── pages/                  # Page-level containers
│   ├── services/
│   │   └── storage/            # Typed frontend storage API (notes, notebooks, tags, settings)
│   ├── hooks/                  # Custom React hooks (useTheme, useKeyboardShortcuts)
│   ├── styles/                 # Design tokens (tokens.css), reset (reset.css), globals (index.css)
│   ├── types/                  # Global TypeScript type definitions
│   ├── utils/                  # Utility functions & helpers
│   ├── shared/                 # Platform-agnostic application logic
│   ├── platform/
│   │   ├── linux/              # Linux-specific integrations
│   │   └── windows/            # Windows-specific integrations (future)
│   └── tests/                  # Test suites
├── src-tauri/
│   ├── src/
│   │   ├── commands/           # Tauri IPC commands (storage.rs)
│   │   ├── storage/            # Native storage engine
│   │   │   ├── database.rs     # SQLite connection pool, PRAGMA config (WAL, busy timeout)
│   │   │   ├── paths.rs        # User data path resolution & traversal defenses
│   │   │   ├── schema.rs       # DDL definitions & foreign key constraints
│   │   │   ├── migrations.rs   # Migration runner & version tracking
│   │   │   ├── models.rs       # Rust DTOs & domain models
│   │   │   ├── errors.rs       # Error types & user-facing error mappings
│   │   │   └── repositories/   # Entity repositories (notes, notebooks, tags, settings)
│   │   ├── lib.rs              # Tauri setup, command registration, and unit tests
│   │   └── main.rs             # Application binary entry point
│   ├── tests/                  # Integration test suite (storage_integration_test.rs)
│   ├── Cargo.toml              # Rust crate manifest & dependencies (rusqlite, uuid, chrono, thiserror)
│   └── tauri.conf.json         # Tauri v2 configuration (window sizes, security permissions)
├── docs/                       # Project specifications & architecture documentation
│   ├── storage-architecture.md # Phase 2 storage architectural specification
│   └── development/            # Phase-by-phase design specifications
├── public/                     # Static public assets
├── package.json                # Node dependencies & project scripts
├── tsconfig.json               # TypeScript configuration
└── vite.config.ts              # Vite configuration
```

---

## Development & Testing

### Prerequisites (Ubuntu / Linux)

1. **Node.js & npm**: Node.js (v18+) and npm (v9+)
2. **Rust & Cargo**: Latest stable Rust toolchain via `rustup`:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
3. **Tauri v2 System Dependencies (Ubuntu / Debian)**:
   ```bash
   sudo apt update
   sudo apt install -y build-essential curl wget file pkg-config libssl-dev \
     libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libwebkit2gtk-4.1-dev libxdo-dev
   ```

### Installation

```bash
# Clone the repository and enter the directory
cd custom-notepad

# Install frontend dependencies
npm install
```

### Verification & Test Commands

```bash
# 1. Run all unit and integration tests (19 tests)
cargo test --manifest-path src-tauri/Cargo.toml

# 2. Check Rust lints with clippy
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets

# 3. Type-check and build frontend production bundle
npm run build

# 4. Run desktop application with live reload
npm run tauri dev
```

---

## Feature Summary (Phases 1 & 2)

- **Desktop Window Management**: Configured in `tauri.conf.json` with 1400x850 default size, 900x600 minimum size, centered launch, and smooth resizing.
- **Three-Pane Layout**: Collapsible Navigation Sidebar, Notes List Column, and Active Editor Pane.
- **SQLite Storage Engine**: Embedded SQLite with WAL mode, foreign keys enforced, busy timeout (5000ms), and synchronous NORMAL.
- **Notes Persistence**: Full CRUD operations for notes with soft-deletion (`is_deleted`), favorites, and pin states.
- **Hierarchical Notebooks**: Multi-level nested notebooks (`parent_id` foreign key with cascade deletion).
- **Tag Organization**: Many-to-many note-tag association (`note_tags`) with cascade deletion on note/tag removal.
- **Settings Store**: Persistent key-value store for application settings (theme preference persisted across app restarts).
- **Filesystem Architecture**: Dedicated user data directories for `attachments/`, `backups/`, and `config/` with path traversal rejection.
- **Transaction Safety**: Atomic multi-table updates (`create_with_tags`) with automatic rollback on error.
- **Keyboard Navigation**:
  - `Ctrl/Cmd + N`: New Note action trigger.
  - `Ctrl/Cmd + F`: Focus & select search input.
  - `Ctrl/Cmd + ,`: Open Settings dialog.
  - `Escape`: Close open modal / clear and blur search input.

---

## Phase 2 Limitations (Scheduled for Subsequent Phases)

In strict adherence to the sequential milestone specification, the following capabilities are deferred to upcoming phases:

- ❌ **Phase 3 (Note Creation & Editing)**: Rich text / Markdown WYSIWYG editor, autosave debounce, active editing toolbar, and split-pane markdown preview.
- ❌ **Phase 4 (Notebook Management UI)**: Notebook tree navigation UI, folder expand/collapse state, nested drag-and-drop hierarchy management, and notebook rename dialogs.
- ❌ **Phase 5 (Tag Management UI)**: Tag selector dropdowns, inline note tag chips, tag deletion confirmations, and sidebar tag filtering.
- ❌ **Phase 6 (Full-Text Search Engine)**: SQLite FTS5 index, match highlight rendering, search operators, and search history.
- ❌ **Phase 7 (Attachments & Media UI)**: Local attachment file-picker dialog, drag-and-drop file insertion, image previewer, and file size quotas.
- ❌ **Phase 8 (Trash & Recovery UI)**: Trash bin list view, note restoration (`is_deleted = 0`), and permanent purge confirmation dialogs.
- ❌ **Phase 11 (Backup & Import/Export)**: Automated database snapshots, export to Markdown/PDF/JSON, and Joplin/Evernote (.enex) import routines.
- ❌ **Phase 12 (Local Vault Encryption)**: AES-GCM local database encryption.
- ❌ **Release Packaging**: Debian package (`.deb`) and Windows installer distribution deferred to Phase 6 and final packaging phases.
- ❌ **AI Integration**: Personal Notepad is and will remain free of third-party AI dependencies and LLM calls.
