# Personal Notepad

Personal Notepad is a fast, offline-first desktop note-taking application designed for privacy, reliability, and cross-platform flexibility. It operates completely independently of cloud infrastructure, mandatory user accounts, telemetry, and external AI integrations.

---

## Current Status

**Phase 15 — Ubuntu Packaging, Installation, and Distribution (Completed)**

Personal Notepad is packaged as an official, production-ready Debian package (`personal-notepad_0.1.0_amd64.deb`) for **Ubuntu 24.04 LTS (Noble Numbat)** and **Ubuntu 26.04 LTS** on `x86_64` (`amd64`). The package integrates cleanly into FreeDesktop application menus with high-resolution icons, adheres to canonical Linux filesystem separation (`/usr` for binaries vs `$XDG_DATA_HOME` for user data), guarantees 100% data preservation across upgrades and removals, and operates air-gapped with zero internet, cloud, account, or AI requirements.

See also:
- [Storage Architecture Documentation](docs/storage-architecture.md)
- [Note Editor Architecture (Phase 3)](docs/note-editor.md)
- [Notebook & Organization Architecture (Phase 4)](docs/notebook-architecture.md)
- [Full-Text Search Architecture (Phase 6)](docs/search-architecture.md)
- [Attachments & Files Architecture (Phase 7)](docs/attachments-architecture.md)
- [Trash & Recovery Architecture (Phase 8)](docs/trash-and-recovery-architecture.md)
- [Internal Links Architecture (Phase 9)](docs/internal-links-architecture.md)
- [Import & Export Architecture (Phase 10)](docs/import-export-architecture.md)
- [Linux Packaging & Distribution (Phase 15)](platform/linux/README.md)
- [Changelog](CHANGELOG.md)

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

## Installation & Linux Distribution

### Quick Install (Ubuntu 24.04+ / Debian amd64)
Download the latest `personal-notepad_0.1.0_amd64.deb` deliverable and install via APT:

```bash
# Recommended: installs the package and automatically resolves runtime dependencies
sudo apt install ./personal-notepad_0.1.0_amd64.deb
```

Or install via `dpkg`:
```bash
sudo dpkg -i personal-notepad_0.1.0_amd64.deb
sudo apt-get install -f # Fix any missing dependencies if needed
```

Launch from your desktop application launcher (search for **Personal Notepad**) or via the terminal:
```bash
personal-notepad
```

### User Data Safety & Uninstall Policy
- **Package Removal**: `sudo apt remove personal-notepad`
  Removes exclusively the application executable (`/usr/bin/personal-notepad`), desktop launcher, and system icons.
- **Data Preservation Guarantee**: User databases, notes, attachments, tags, backups, and encryption keys strictly reside in `$XDG_DATA_HOME/com.personalnotepad.app/` (`~/.local/share/com.personalnotepad.app/`). They are **never** deleted, modified, or overwritten during package installation, upgrade, or removal.

---

## Development & Testing

### Prerequisites (Ubuntu / Linux)

1. **Node.js & npm**: Node.js (`>= 20.x`, verified `v24.18.0`) and npm (`>= 10.x`, verified `11.16.0`).
2. **Rust & Cargo**: Latest stable Rust toolchain (`>= 1.80`, verified `1.98.1`):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
3. **Native Linux System Dependencies**:
   ```bash
   sudo apt update && sudo apt install -y \
     build-essential curl wget file pkg-config \
     libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev \
     libglib2.0-dev libcairo2-dev libgdk-pixbuf-2.0-dev libdbus-1-dev xdg-utils
   ```

### Setup & Build Commands

```bash
# Clone the repository
git clone https://github.com/codewithritik02/advanced-notepad-for-ubuntu.git
cd advanced-notepad-for-ubuntu

# Clean install of locked frontend dependencies
npm ci

# Run TypeScript compilation and frontend hygiene checks
npm run typecheck
npm run lint:frontend

# Run frontend test suite (1,068 tests across 384 suites)
npm run test:frontend

# Run all Rust test suites (256+ unit, integration, and security tests)
cargo test --manifest-path src-tauri/Cargo.toml --locked

# Build production Debian package (.deb deliverable)
npm run package:linux
```

The resulting package is generated at:
`src-tauri/target/release/bundle/deb/personal-notepad_0.1.0_amd64.deb`

---

## Core Feature Overview

- **100% Offline & Non-AI**: Zero telemetry, zero cloud synchronizations, zero user accounts, zero remote tracking.
- **Embedded SQLite Storage**: WAL (Write-Ahead Logging) journal mode, foreign-key relational integrity, and automated schema migrations.
- **Full-Text Search (FTS5)**: Fast tokenized search across titles, content, and tags with automated trigger synchronization.
- **Hierarchical Organization**: Multi-level nested notebooks, tags, pinned notes, and favorites.
- **Media Attachments**: Managed local file attachments with strict path traversal defenses and executable extension blocking.
- **Trash & Recovery**: Soft-delete trash bin with one-click restore and permanent purge confirmations.
- **Phase 12 Vault Encryption**: Optional client-side master password encryption using Argon2id and authenticated AES-256-GCM.
- **Full FreeDesktop Integration**: Standard `.desktop` launcher and high-resolution icons (32x32 to 512x512).
- **Windows Packaging (Phase 16)**: Windows installer distribution will follow in Phase 16.

