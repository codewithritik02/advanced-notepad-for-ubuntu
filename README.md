# Personal Notepad

Personal Notepad is a fast, private note-taking app for your desktop. Everything you write stays on your own computer. There is no cloud, no account, no tracking and no AI service. It works fully offline.

It is **free and open source**, and **anyone can use it and contribute to it**.

---

## Platform support

| Platform | Status |
|---|---|
| **Linux** (Ubuntu / Debian, `amd64`) | Available now. Install with the `.deb` package. |
| **Windows** | **In progress.** The app is built with Tauri, so it is designed to run on Windows too. The Windows installer is not released yet. |
| macOS | Not a target right now. |

Today only Linux is released. Work on Windows is ongoing, and Windows contributions are very welcome (see [Contributing](#contributing)).

---

## Features

**Writing**
- Plain text (`.txt`) and Markdown (`.md`) notes with a live preview.
- Autosave, undo / redo, word and character count.
- Images inside notes: attach a file, then insert it into the text.
- Interface zoom and light / dark themes.

**Organizing**
- Notebooks nested up to 7 levels deep. Move notes and notebooks between notebooks.
- Tags, favorites, pinned notes and hidden notes.
- Breadcrumb path for the current notebook; the app remembers which folders you left open.
- Trash with restore and permanent delete.

**Finding and connecting**
- Fast full-text search across titles, content and tags.
- Links between notes. Each note shows **Linked from**: the notes that link to it. Use the **+** button there to pick one or more notes and add a link to the current note in each of them.
- Back / forward navigation between notes you opened.

**Files, import and export**
- Attach any file to a note.
- Import `.txt`, `.md` and `.json` files.
- Export one note or many as `.txt`, `.md`, `.json` or a `.zip` that keeps your notebook folders and attachments.
- Open `.txt` and `.md` files directly with the app ("Open with").

**Safety**
- Backups: create them by hand or let the app create them on a schedule, check them, and restore from them.
- Optional password lock with strong encryption (Argon2id key derivation and AES-256-GCM). Lock the app at any time.
- Your data is never deleted when you install, upgrade or uninstall the app.

---

## Install on Linux (Ubuntu / Debian, amd64)

Download the latest `personal-notepad_<version>_amd64.deb` from the project's releases, then install it:

```bash
sudo apt install ./personal-notepad_<version>_amd64.deb
```

Start it from your application menu (search for **Personal Notepad**) or run:

```bash
personal-notepad
```

To upgrade, install the newer `.deb` the same way. Your notes stay untouched.

### Where your data is stored

| OS | Location |
|---|---|
| Linux | `~/.local/share/com.personalnotepad.app/` |
| Windows (when released) | `%APPDATA%\com.personalnotepad.app\` |

This folder holds your database (`database.sqlite`), attachments and backups. It is separate from the installed program, so installing, upgrading or removing the app never deletes it.

To remove the app and keep your notes:

```bash
sudo apt remove personal-notepad
```

To delete your notes too, remove the data folder yourself. Back it up first if you might need it.

---

## Keyboard shortcuts

Use `Ctrl` on Linux and Windows (`Cmd` on macOS).

| Shortcut | Action |
|---|---|
| `Ctrl + N` | New note |
| `Ctrl + S` | Save now |
| `Ctrl + K` or `Ctrl + F` | Search |
| `Ctrl + ,` | Settings |
| `Ctrl + Shift + L` | Lock the app |
| `Ctrl + +` / `Ctrl + -` / `Ctrl + 0` | Zoom in / out / reset |

---

## How it works

Personal Notepad is a desktop app made of two parts:

- **Frontend**: React 19 and TypeScript, built with Vite. It only draws the interface.
- **Backend**: Rust with an SQLite database. It owns all data and file access.

The two parts talk through typed Tauri commands. The interface never runs SQL and never touches files directly.

```text
React + TypeScript  ──►  storage service  ──►  Tauri commands  ──►  Rust  ──►  SQLite + files
   (src/)                (src/services)       (src-tauri/src/commands)
```

Design rules:
- Every database command is a fixed, typed function. There is no generic "run SQL" call.
- All SQL uses bound parameters, and multi-step changes run in transactions.
- File and backup names are checked so they cannot escape the data folder.
- The app makes no network calls.

---

## Project structure

```text
custom-notepad/
├── src/                        # Frontend (React + TypeScript)
│   ├── app/                    # App shell and top-level state
│   ├── components/             # Sidebar, top bar, shared UI (Button, Modal, ...)
│   ├── features/               # One folder per feature
│   │   ├── notes/              #   note list, editor, links, backlinks
│   │   ├── notebooks/          #   notebook tree, move, breadcrumb
│   │   ├── tags/               #   tags
│   │   ├── search/             #   search bar and results
│   │   ├── attachments/        #   attachments and inline images
│   │   ├── importExport/       #   import and export pages
│   │   ├── backup/             #   backup, restore, schedule
│   │   ├── security/           #   password lock and encryption screens
│   │   └── settings/           #   settings page
│   ├── hooks/                  # Shared React hooks (theme, shortcuts, ...)
│   ├── layouts/                # Main layout: sidebar, note list, editor
│   ├── services/storage/       # Typed calls from the frontend to the backend
│   ├── styles/                 # Design tokens and global styles
│   └── utils/                  # Helpers (markdown, link parsing, ...)
├── src-tauri/                  # Backend (Rust + Tauri 2)
│   ├── src/
│   │   ├── commands/           # Commands the frontend can call
│   │   └── storage/            # Database, migrations, repositories,
│   │                           #   backup, import/export, encryption
│   ├── tests/                  # Backend integration tests
│   ├── icons/                  # App icons
│   └── tauri.conf.json         # App name, version, window and package config
├── platform/
│   ├── linux/                  # Linux packaging notes
│   └── windows/                # Windows support (in progress)
├── scripts/                    # Lint, test-safety and release helper scripts
├── tests/                      # Frontend tests
├── CHANGELOG.md                # What changed in each version
└── package.json                # Scripts and frontend dependencies
```

---

## Run it from source

### What you need

- Node.js 20 or newer, and npm
- Rust (stable), installed with [rustup](https://rustup.rs)
- On Ubuntu / Debian, the system libraries Tauri needs:

```bash
sudo apt update && sudo apt install -y \
  build-essential curl wget file pkg-config \
  libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev \
  libglib2.0-dev libcairo2-dev libgdk-pixbuf-2.0-dev libdbus-1-dev xdg-utils
```

### Steps

```bash
git clone https://github.com/codewithritik02/advanced-notepad-for-ubuntu.git
cd advanced-notepad-for-ubuntu
npm ci
npm run tauri dev
```

### Use a separate data folder while developing

A development run can read and change real notes. To keep your own notes safe, point it at a throw-away folder. In a debug build, `PERSONAL_NOTEPAD_TEST_DATA_DIR` selects the data folder. It must be a new folder inside your system temp directory, named `personal-notepad-test-<something>`, and it must contain a marker file named `.personal-notepad-test-root` whose content is exactly `personal-notepad-isolated-test-root-v1` followed by a newline:

```bash
D=$(mktemp -d /tmp/personal-notepad-test-XXXXXX)
printf 'personal-notepad-isolated-test-root-v1\n' > "$D/.personal-notepad-test-root"
PERSONAL_NOTEPAD_TEST_DATA_DIR="$D" npm run tauri dev
```

### Checks

```bash
npm run typecheck        # TypeScript
npm run lint             # Frontend lint and hygiene rules
npm test                 # Frontend tests
npm run test:safety      # Makes sure tests never touch real user data
npm run test:rust        # Backend tests
npm run lint:rust        # Rust clippy
npm run format:rust:check
```

### Build the Linux package

```bash
npm run package:linux
```

The package is created in `src-tauri/target/release/bundle/deb/`.

---

## Contributing

Personal Notepad is open source and **everyone is welcome to contribute**: code, bug reports, ideas, translations, documentation and testing.

Good places to start:
- **Windows support**: installer, testing on Windows, and fixes for anything that behaves differently there.
- Bug fixes and small improvements.
- Tests and documentation.

How to contribute:
1. Fork the repository and create a branch for your change.
2. Make your change. Keep it focused on one thing.
3. Run the checks listed above and make sure they pass.
4. Open a pull request. Describe what changed and why, and add screenshots for interface changes.

Found a bug or have an idea? Open an issue and explain what you expected and what happened.

Please do not add anything that sends data over the network, adds tracking, or requires an account. Keeping the app private and offline is a core goal.

---

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for what changed in every version.
