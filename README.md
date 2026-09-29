# Personal Notepad

Personal Notepad is a fast, offline-first desktop note-taking application designed for privacy, reliability, and cross-platform flexibility. It operates completely independently of cloud infrastructure, mandatory user accounts, telemetry, and external AI integrations.

---

## Current Status

**Phase 1 — Project Foundation & UI Shell (Completed)**

The desktop foundation, window configuration, design token system, responsive 3-pane layout, navigation, editor placeholder, theme switching, reusable UI components, and keyboard navigation foundations are implemented and verified. All data displayed in Phase 1 uses temporary, self-contained mock models for UI verification prior to Phase 2 database integration.

---

## Architecture

The application is structured to maximize shared code across platforms while maintaining strict boundaries between presentation, desktop runtime, native capabilities, and platform-specific implementations.

### Runtime Stack

```text
React 19 / TypeScript (Vite)
            ↓
       Tauri v2 IPC
            ↓
       Rust Backend
```

### Platform Architecture

```text
       Personal Notepad Frontend & Shared Logic
                          |
             +------------+------------+
             |                         |
       Linux Platform           Windows Platform
    (libwebkit2gtk / GTK)        (WebView2 - future)
```

- **Frontend (`src/`)**: Pure React 19 with TypeScript and custom CSS design tokens. Zero third-party UI library bloat, ensuring sub-second startup times and low memory overhead.
- **Desktop Runtime (`src-tauri/`)**: Tauri v2 in Rust managing secure system webview windows, lifecycle, and native system bindings.
- **Platform Separation**: Dedicated directories for Linux-specific (`platform/linux/`) and Windows-specific (`platform/windows/`) capabilities, isolated from shared business logic (`shared/`).

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
│   │   ├── notes/              # Notes list, card views, mock data, and editor placeholder
│   │   ├── notebooks/          # Notebook organization (Phase 2 foundation)
│   │   ├── tags/               # Tag organization (Phase 2 foundation)
│   │   ├── search/             # Search infrastructure (Phase 2 foundation)
│   │   ├── attachments/        # File attachment infrastructure (Phase 2 foundation)
│   │   └── settings/           # Settings modal and theme controls
│   ├── layouts/                # Main 3-pane desktop layout (Sidebar - NotesList - Editor)
│   ├── pages/                  # Page-level containers
│   ├── services/               # Native bridge & upcoming database services
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
│   ├── src/                    # Rust backend source (main.rs, lib.rs)
│   ├── Cargo.toml              # Rust crate manifest & dependencies
│   └── tauri.conf.json         # Tauri v2 configuration (window sizes, security permissions)
├── docs/                       # Project specifications & architecture documentation
├── public/                     # Static public assets
├── package.json                # Node dependencies & project scripts
├── tsconfig.json               # TypeScript configuration
└── vite.config.ts              # Vite configuration
```

---

## Development & Prerequisites

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

### Development Commands

```bash
# Run web preview in browser (Vite dev server)
npm run dev

# Run desktop application with hot reload (Tauri + Vite)
npm run tauri dev
```

### Build Commands

```bash
# Build & verify frontend (TypeScript compile + Vite production bundle)
npm run build

# Check Rust backend compilation
cargo check --manifest-path src-tauri/Cargo.toml

# Build production Tauri desktop bundle
npm run tauri build
```

---

## Phase 1 Feature Summary

- **Desktop Window Management**: Configured in `tauri.conf.json` with 1400x850 default size, 900x600 minimum size, centered launch, and smooth resizing.
- **Three-Pane Layout**: Clean desktop layout composed of Collapsible Navigation Sidebar, Notes List Column, and Active Editor Pane.
- **Visual Search & Filtering**: Instant client-side search filtering across titles and preview contents in mock data.
- **Theme System**: Full support for Light, Dark, and System preference with automatic OS-level change detection via `prefers-color-scheme`.
- **Reusable UI Primitives**: Accessible Button, IconButton, Modal, Badge, Tag, Divider, and State Views (Empty, Loading, Error).
- **Keyboard Navigation**: Centralized cross-platform shortcut foundation:
  - `Ctrl/Cmd + N`: New Note action trigger.
  - `Ctrl/Cmd + F`: Focus & select search input.
  - `Ctrl/Cmd + ,`: Open Settings dialog.
  - `Escape`: Close open modal / clear and blur search input.

---

## Phase 1 Limitations (Not Implemented Yet)

In strict adherence to the sequential milestone specification, the following features are deliberately **NOT** implemented in Phase 1 and are scheduled for subsequent phases:

- ❌ **SQLite Database**: No database schema, migrations, or local SQLite query engine.
- ❌ **Real Note Persistence**: No notes are written to disk or permanent storage yet.
- ❌ **TXT / Markdown Storage**: No raw filesystem note export or disk reading.
- ❌ **Full-Text Search Engine**: No SQLite FTS5 index or advanced search operators.
- ❌ **Attachments & Media**: No local attachment directory or media embedding pipeline.
- ❌ **Trash & Favorite Persistence**: Navigating sections filters mock data only; deletions are not stored.
- ❌ **Import / Export**: No Joplin, Evernote, or plain text import/export routines.
- ❌ **Backup & Encryption**: No database snapshots or AES-GCM local vault encryption.
- ❌ **Cloud Sync**: 100% offline, zero cloud syncing, accounts, or telemetry.
- ❌ **Windows Production Build**: Windows targeting is planned for post-Linux stabilization.
- ❌ **`.deb` Package Distribution**: Release packaging deferred to Phase 6.
- ❌ **AI Integration**: Personal Notepad is and will remain free of third-party AI dependencies and LLM calls.
