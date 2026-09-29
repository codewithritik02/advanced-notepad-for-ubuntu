# PERSONAL NOTEPAD

## PHASE 1 — PROJECT FOUNDATION & UI SHELL

---

# 1. PHASE OBJECTIVE

Build the initial desktop application foundation for **Personal Notepad**, an offline-first, cross-platform note-taking application.

The current target platform is:

* Ubuntu/Linux
* Linux package: `.deb` will be handled in a later phase

The application must be architected from the beginning so that Windows support can be added later without rewriting the entire application.

This phase is ONLY about:

1. Project initialization
2. Technology setup
3. Application architecture
4. Shared/core separation
5. Basic Tauri desktop shell
6. Initial React/TypeScript UI
7. Original Joplin-inspired three-pane layout
8. Navigation structure
9. Theme foundation
10. Reusable UI components
11. Basic application states
12. Development documentation

---

# 2. IMPORTANT PRODUCT RULES

The product name is:

**Personal Notepad**

The application is a traditional notes application.

DO NOT add AI functionality.

The following are explicitly NOT part of this phase:

* AI chatbot
* AI assistant
* AI writing
* AI summarization
* AI autocomplete
* AI suggestions
* Cloud AI APIs
* OpenAI APIs
* LLM integrations

The application must work without an internet connection.

Do not introduce a mandatory account system.

Do not introduce cloud dependencies.

---

# 3. TECHNOLOGY DECISION

Use the following stack unless there is a genuine technical blocker:

### Desktop framework

**Tauri**

### Backend/native layer

**Rust**

### Frontend

**React + TypeScript**

### Build tooling

Use the current stable tooling appropriate for the selected Tauri version.

Prefer:

* Vite
* TypeScript
* React

### Styling

Use a maintainable CSS architecture.

A component-oriented styling approach is preferred.

Do not introduce a huge UI framework unless there is a strong reason.

The application should remain lightweight.

---

# 4. ARCHITECTURAL PRINCIPLE

The most important architecture rule is:

> Keep application logic shared and keep operating-system-specific code isolated.

We currently support Ubuntu/Linux.

Windows will be added later.

Do NOT create two separate applications.

The desired conceptual architecture is:

```text
                    PERSONAL NOTEPAD
                           |
              +------------+------------+
              |                         |
          Frontend                  Native Layer
       React/TypeScript                Rust
              |                         |
              +------------+------------+
                           |
                     Shared Core
                           |
             +-------------+-------------+
             |             |             |
          Storage       Search       Application
          (future)       (future)       Logic
```

Platform-specific functionality should eventually be isolated behind abstractions.

For example:

```text
Platform
├── Linux
└── Windows
```

Do not duplicate business logic between Linux and Windows.

---

# 5. EXPECTED PROJECT STRUCTURE

The exact structure can be adjusted if the chosen tooling requires it, but maintain this architectural separation:

```text
personal-notepad/
│
├── src/
│   ├── app/
│   ├── components/
│   ├── layouts/
│   ├── pages/
│   ├── features/
│   ├── hooks/
│   ├── services/
│   ├── types/
│   ├── styles/
│   └── main.tsx
│
├── src-tauri/
│   ├── src/
│   ├── icons/
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── shared/
│   └── README.md
│
├── platform/
│   ├── linux/
│   │   └── README.md
│   │
│   └── windows/
│       └── README.md
│
├── assets/
│
├── docs/
│
├── tests/
│
├── package.json
├── tsconfig.json
├── vite.config.*
└── README.md
```

Do not create unnecessary files just to satisfy this example.

If some folders are not needed yet, it is acceptable to create only the relevant structure.

However, the architecture must clearly distinguish:

* frontend
* native/backend
* shared logic
* platform-specific logic

---

# 6. PHASE 1 TASK BREAKDOWN

Phase 1 must be completed in the following small tasks.

Do NOT attempt all tasks at once.

## TASK 1

Development environment verification and project initialization.

## TASK 2

Tauri + React + TypeScript application shell.

## TASK 3

Project architecture and folder organization.

## TASK 4

Application window configuration and desktop identity.

## TASK 5

Global UI foundation and design tokens.

## TASK 6

Main application layout.

## TASK 7

Sidebar / navigation system.

## TASK 8

Notes list panel.

## TASK 9

Editor placeholder panel.

## TASK 10

Top bar, search placeholder, and global actions.

## TASK 11

Theme system: Light / Dark / System.

## TASK 12

Reusable UI components and application states.

## TASK 13

Responsive desktop behavior and layout refinement.

## TASK 14

Keyboard/navigation foundation.

## TASK 15

Code cleanup, documentation, and Phase 1 verification.

Each task should be performed separately.

---

# TASK 1 — DEVELOPMENT ENVIRONMENT & PROJECT INITIALIZATION

## Objective

Prepare the development environment and initialize the Personal Notepad project.

## Requirements

Verify that the development machine has:

* Node.js
* npm or compatible package manager
* Rust
* Cargo
* Tauri prerequisites
* Git

Do not install unrelated tools.

Check versions before changing anything.

The agent must first inspect the existing environment.

Do not blindly reinstall software.

## Project initialization

Create the Personal Notepad project using:

* Tauri
* React
* TypeScript
* Vite

Application name:

```text
Personal Notepad
```

Use a clean package/application identifier.

Suggested identifier:

```text
com.personalnotepad.app
```

If the project already exists, do NOT recreate it.

Inspect the existing project first.

## Acceptance criteria

The project must:

1. Install dependencies successfully.
2. Start in development mode.
3. Open a Tauri desktop window.
4. Display a basic React interface.
5. Compile TypeScript without errors.
6. Compile Rust without errors.
7. Have a clean Git-friendly structure.

Do not implement notes, database, search, attachments, sync, encryption, or advanced UI in this task.

---

# TASK 2 — TAURI + REACT APPLICATION SHELL

## Objective

Create the minimum functional desktop shell.

The application should launch as:

```text
Personal Notepad
```

The window must contain a simple placeholder screen.

Example:

```text
Personal Notepad

Application shell is ready.
```

This is temporary.

## Requirements

Configure:

* Tauri application
* React entry point
* TypeScript
* Vite
* Rust backend
* development commands
* production build command

Do not implement business logic.

Do not add database dependencies.

Do not add filesystem storage yet.

Do not add search.

Do not add editor functionality.

## Acceptance criteria

Running the development command must open the application successfully.

There must be no:

* TypeScript errors
* Rust compilation errors
* console errors caused by the application
* broken imports

---

# TASK 3 — PROJECT ARCHITECTURE & FOLDER ORGANIZATION

## Objective

Create a maintainable project structure before UI development becomes large.

Use feature-oriented organization where practical.

Frontend should have a structure similar to:

```text
src/
├── app/
├── components/
├── features/
├── layouts/
├── pages/
├── services/
├── hooks/
├── types/
├── styles/
└── utils/
```

Possible feature folders:

```text
features/
├── notes/
├── notebooks/
├── tags/
├── search/
├── attachments/
└── settings/
```

These can initially contain placeholders.

Do NOT implement those features yet.

## Native layer

Keep Tauri/Rust code separate.

```text
src-tauri/
└── src/
```

Future native services should not leak directly into React components.

## Platform structure

Create conceptual platform separation:

```text
platform/
├── linux/
└── windows/
```

Windows can contain only documentation/placeholder information for now.

Do not write fake Windows implementation.

## Shared logic

If a piece of logic is platform independent, keep it shared.

Do not copy the same logic into:

```text
platform/linux/
platform/windows/
```

## Acceptance criteria

A new developer should be able to understand:

* where UI lives
* where feature logic lives
* where Rust code lives
* where platform-specific code will live
* where shared code belongs

without reading the entire project.

---

# TASK 4 — APPLICATION WINDOW & DESKTOP IDENTITY

## Objective

Configure the desktop application identity and basic window behavior.

## Requirements

Application name:

```text
Personal Notepad
```

Configure:

* application title
* window title
* sensible default width
* sensible default height
* minimum width
* minimum height
* resizable window
* normal desktop behavior

Recommended initial dimensions:

```text
width: 1400
height: 850
```

Minimum:

```text
width: 900
height: 600
```

These are starting values and may be adjusted after UI testing.

Do not make the application fullscreen by default.

Do not force maximization.

Do not implement custom OS-specific window behavior unless necessary.

## Acceptance criteria

The application launches as a normal desktop application with sensible dimensions.

The title should clearly identify:

```text
Personal Notepad
```

---

# TASK 5 — GLOBAL UI FOUNDATION & DESIGN TOKENS

## Objective

Create the visual foundation before implementing individual UI sections.

The UI should be:

* clean
* modern
* desktop-first
* distraction-free
* readable
* lightweight
* consistent

The application can take inspiration from applications such as Joplin, Obsidian, VS Code, and traditional note applications, but DO NOT copy their UI exactly.

The final design must be original.

## Design tokens

Create centralized tokens for:

* background colors
* surface colors
* border colors
* primary text
* secondary text
* muted text
* hover state
* active state
* danger state
* accent color
* spacing
* border radius
* font sizes
* font weights
* shadows
* transitions

Example conceptual structure:

```text
--color-background
--color-surface
--color-surface-hover
--color-border
--color-text
--color-text-secondary
--color-text-muted
--color-accent
--color-danger

--spacing-xs
--spacing-sm
--spacing-md
--spacing-lg
--spacing-xl

--radius-sm
--radius-md
--radius-lg
```

Do not hardcode the same color repeatedly throughout components.

## Typography

Use a clean system-friendly font stack.

Avoid downloading external fonts unnecessarily.

The application should work offline.

## Acceptance criteria

Changing a central design token should update the relevant UI consistently.

No major component should contain unnecessary duplicated styling values.

---

# TASK 6 — MAIN APPLICATION LAYOUT

## Objective

Implement the main desktop layout.

Use a three-pane concept:

```text
┌──────────────────────────────────────────────────────────────┐
│                         Top Bar                              │
├───────────────┬──────────────────────┬───────────────────────┤
│               │                      │                       │
│   Sidebar     │     Notes List       │       Editor          │
│               │                      │                       │
│               │                      │                       │
│               │                      │                       │
│               │                      │                       │
└───────────────┴──────────────────────┴───────────────────────┘
```

## Pane responsibilities

### Left pane

Navigation:

* All Notes
* Notebooks
* Tags
* Favorites
* Trash

### Middle pane

Note list.

### Right pane

Editor area.

At this stage the editor is only a placeholder.

## Layout behavior

The panes should have:

* independent widths
* clear separation
* consistent borders
* proper overflow behavior

The editor should receive the majority of available width.

## Acceptance criteria

The layout should feel like a real desktop note application even though actual note functionality is not implemented yet.

---

# TASK 7 — SIDEBAR & NAVIGATION

## Objective

Implement the left navigation panel.

## Sidebar sections

At minimum:

```text
PERSONAL NOTEPAD

+ New Note

Notes
  All Notes
  Favorites

Organization
  Notebooks
  Tags

System
  Trash
```

Exact naming can be refined during implementation.

## Requirements

* Icons should be used where useful.
* Every item should have a hover state.
* Active item should be visually distinguishable.
* Sidebar should scroll if necessary.
* Sidebar width should be adjustable in future architecture.
* New Note button should exist visually, but actual note creation will be implemented later.

## Navigation state

For now, clicking an item may update the active navigation state.

Do not implement real database filtering yet.

## Acceptance criteria

The user can visually navigate between sidebar sections.

The active section is clearly identifiable.

No fake data persistence is required.

---

# TASK 8 — NOTES LIST PANEL

## Objective

Create the middle notes-list area.

Use temporary mock data ONLY for UI development.

Example:

```text
Welcome to Personal Notepad
Today
This is your first note...

Project Ideas
Yesterday
Some project ideas...

Shopping List
Monday
Milk, bread...
```

Clearly isolate mock data so it can later be replaced by the real database layer.

## Note list requirements

Each note item can show:

* title
* short preview
* modified date/time
* optional favorite indicator

Example:

```text
Project Ideas
Some project ideas for later...
Yesterday
```

## States

Implement:

### Normal state

Notes displayed.

### Empty state

```text
No notes yet

Create your first note to get started.
```

### Loading state

Placeholder/skeleton.

### Error state

Basic error presentation.

Real database loading is NOT part of this task.

## Acceptance criteria

The notes list looks realistic but remains clearly based on temporary UI data.

---

# TASK 9 — EDITOR PLACEHOLDER

## Objective

Create the right-side editor area without implementing the actual editor.

The editor must visually communicate where note content will appear.

Example:

```text
┌─────────────────────────────────────┐
│ Note title                          │
├─────────────────────────────────────┤
│                                     │
│ Start writing your note...          │
│                                     │
│                                     │
└─────────────────────────────────────┘
```

## Include

* Title placeholder
* Content placeholder
* Editor toolbar placeholder if appropriate
* Note metadata/status area

Do NOT implement:

* Markdown parsing
* TXT file saving
* autosave
* database persistence
* formatting engine

Those belong to later phases.

## Acceptance criteria

The editor area visually looks like a real note editor while remaining non-functional.

---

# TASK 10 — TOP BAR, SEARCH & GLOBAL ACTIONS

## Objective

Implement the top application toolbar.

Concept:

```text
┌────────────────────────────────────────────────────────────┐
│ Personal Notepad   🔍 Search...        + New Note   ⚙      │
└────────────────────────────────────────────────────────────┘
```

## Elements

* Application identity where appropriate
* Search field
* New Note action
* Theme/settings access
* Optional additional global action space

## Search

The search field is only UI at this stage.

It must NOT query the database because the database does not exist yet.

It may accept text visually.

## New Note

Button exists but should only trigger a placeholder action or temporary UI behavior.

Do not implement persistence.

## Acceptance criteria

The top bar is functional at the UI-state level and visually consistent with the rest of the application.

---

# TASK 11 — LIGHT / DARK / SYSTEM THEME

## Objective

Implement the application's theme foundation.

Support:

```text
Light
Dark
System
```

## Requirements

Theme selection must be centralized.

Do not hardcode dark-mode colors inside individual components.

Use theme variables/tokens.

The application should respect the operating system preference when:

```text
System
```

is selected.

Persisting the setting to a database is NOT required yet.

If persistence is needed temporarily, use the simplest appropriate frontend mechanism, but do not build the final settings storage system in this phase.

## Acceptance criteria

Switching themes updates the entire UI consistently.

No major component should remain unreadable in either theme.

---

# TASK 12 — REUSABLE UI COMPONENTS & APPLICATION STATES

## Objective

Create reusable primitives instead of duplicating UI.

Possible components:

```text
Button
IconButton
Input
SearchInput
Panel
Divider
Tooltip
Modal
Dropdown
Menu
EmptyState
LoadingState
ErrorState
Badge
Tag
```

Only create components that are actually useful.

Do not create an enormous component library.

## Requirements

Components should have:

* clear props
* TypeScript types
* consistent styling
* predictable states

Buttons should support states such as:

```text
default
hover
active
disabled
```

## Acceptance criteria

Common UI behavior is reusable.

Avoid duplicated button/input/modal implementations.

---

# TASK 13 — DESKTOP RESPONSIVENESS & LAYOUT REFINEMENT

## Objective

Make the desktop interface robust across different window sizes.

Test at:

```text
900 × 600
1024 × 768
1280 × 720
1366 × 768
1440 × 900
1920 × 1080
```

## Requirements

The UI must:

* avoid accidental horizontal overflow
* preserve usable editor width
* preserve sidebar usability
* handle narrow desktop windows gracefully
* handle large monitors gracefully

The three-pane layout should remain usable.

Do not turn the app into a mobile-first UI.

Mobile support is not a current requirement.

---

# TASK 14 — KEYBOARD & NAVIGATION FOUNDATION

## Objective

Prepare keyboard interaction architecture.

Implement only safe foundational shortcuts that do not conflict with future functionality.

Examples:

```text
Ctrl/Cmd + N
```

for New Note UI action.

```text
Ctrl/Cmd + F
```

for focusing search.

Do not implement complex editor shortcuts yet.

Do not interfere with browser/Tauri native shortcuts unnecessarily.

## Requirements

Keyboard handling should be centralized where practical.

Avoid attaching global keyboard listeners to every component.

## Acceptance criteria

Basic global navigation shortcuts work reliably.

---

# TASK 15 — CLEANUP, DOCUMENTATION & FINAL VERIFICATION

## Objective

Finish Phase 1 cleanly.

## Code cleanup

Check:

* unused imports
* dead code
* unnecessary dependencies
* duplicated styles
* duplicated components
* TypeScript errors
* Rust warnings
* console errors
* broken routes/state
* accessibility problems

Do not leave temporary debugging logs.

Mock data must be clearly identifiable.

## Documentation

Update README with:

### Project

```text
Personal Notepad
```

### Current status

```text
Phase 1 — Project Foundation & UI Shell
```

### Development

Document:

* prerequisites
* install dependencies
* development command
* build command
* project structure

### Architecture

Explain:

```text
React/TypeScript
        ↓
Tauri
        ↓
Rust
```

and:

```text
Shared application logic
        ↓
Platform-specific implementations
```

## Phase 1 limitations

Explicitly document that these are NOT implemented yet:

* SQLite
* real note persistence
* TXT storage
* Markdown storage
* search engine
* attachments
* trash persistence
* import/export
* backup
* encryption
* sync
* Windows build
* `.deb` packaging

---

# 7. PHASE 1 ACCEPTANCE CRITERIA

Phase 1 is complete only when all of the following are true:

## Application

* Personal Notepad launches successfully on Ubuntu.
* Tauri desktop window works.
* React UI renders correctly.
* TypeScript compiles.
* Rust compiles.
* No critical console errors.

## Architecture

* Frontend and native code are separated.
* Shared logic has a defined location.
* Platform-specific architecture is defined.
* Linux-specific code can be isolated.
* Windows can be added later without duplicating the entire app.

## UI

The application contains:

* top bar
* sidebar
* notes list
* editor area
* navigation
* search UI
* new note action
* theme control
* empty/loading/error states

## Theme

* Light works.
* Dark works.
* System preference works.

## Layout

* Three-pane layout works.
* Window resizing works.
* No major overflow issues.
* UI remains usable on supported desktop resolutions.

## Code quality

* No unnecessary dependencies.
* No obvious duplicated code.
* No debug logs.
* Components are reasonably reusable.
* TypeScript types are defined properly.

---

# 8. WHAT MUST NOT BE IMPLEMENTED IN PHASE 1

This is extremely important.

Do NOT implement these features yet:

```text
DATABASE
❌ SQLite
❌ Database schema
❌ Database migrations

STORAGE
❌ TXT persistence
❌ Markdown persistence
❌ Final storage directory
❌ Attachment storage

NOTES
❌ Real note creation
❌ Real note editing
❌ Autosave
❌ Delete persistence

SEARCH
❌ Full-text search
❌ Search indexing

ORGANIZATION
❌ Real notebooks
❌ Real tags
❌ Real favorites
❌ Real trash

SECURITY
❌ Encryption implementation

SYNC
❌ Cloud sync
❌ Accounts
❌ Servers

IMPORT/EXPORT
❌ TXT export
❌ Markdown export
❌ PDF export
❌ Joplin import

PACKAGING
❌ .deb release packaging
❌ Windows .exe

AI
❌ Any AI integration
```

These features belong to later phases.

---

# 9. IMPORTANT AI CODING AGENT RULES

When implementing Phase 1, follow these rules strictly.

## Rule 1 — Inspect before changing

Before modifying any file:

1. Inspect the existing project.
2. Understand the current structure.
3. Identify what already exists.
4. Avoid recreating existing functionality.

Never blindly overwrite working files.

## Rule 2 — One task at a time

Do not implement Task 1 through Task 15 together.

Complete one task, verify it, then move to the next.

## Rule 3 — Preserve previous work

When implementing a later task:

> Do not unnecessarily rewrite or replace previous functionality.

Only modify files required for the current task.

## Rule 4 — No speculative features

Do not implement future features just because their architecture is being prepared.

For example:

Preparing for SQLite does NOT mean installing and implementing SQLite now.

Preparing for Windows does NOT mean creating Windows-specific functionality now.

## Rule 5 — Keep dependencies minimal

Before adding a dependency, determine whether it is actually necessary.

Do not add libraries simply for convenience.

## Rule 6 — Offline-first

Do not introduce a service that requires internet access for the application to work.

## Rule 7 — No AI

Never add an AI-related package, API, service, model, chatbot, or AI feature.

## Rule 8 — Do not copy Joplin

The product can use familiar note-taking patterns, but the UI and implementation should be original.

## Rule 9 — Don't over-engineer

Use clean architecture, but don't create unnecessary abstractions.

The code should be understandable to another developer.

## Rule 10 — Verify after every task

After each task:

1. Run appropriate checks.
2. Build/compile.
3. Fix errors.
4. Confirm the application still launches.
5. Summarize exactly what changed.

---

# 10. REQUIRED RESPONSE FORMAT FOR THE AI CODING AGENT

After completing each task, report:

```text
TASK COMPLETED:
Task X — <name>

CHANGES:
- ...
- ...
- ...

FILES CREATED:
- ...

FILES MODIFIED:
- ...

DEPENDENCIES ADDED:
- None
or
- ...

VERIFICATION:
- TypeScript: PASS/FAIL
- Rust: PASS/FAIL
- Build: PASS/FAIL
- Application launch: PASS/FAIL

NOT IMPLEMENTED YET:
- ...

NOTES FOR NEXT TASK:
- ...
```

Do not claim something is working without actually verifying it.

---

# 11. FINAL PHASE 1 RESULT

At the end of Phase 1, Personal Notepad should look and behave approximately like a real desktop note application shell:

```text
┌──────────────────────────────────────────────────────────────┐
│ Personal Notepad    Search...          + New Note    ⚙       │
├────────────────┬──────────────────────┬──────────────────────┤
│                │                      │                      │
│ All Notes      │ Welcome to Personal │ Welcome to Personal  │
│ Favorites      │ Notepad              │ Notepad              │
│                │                      │                      │
│ Notebooks      │ Project Ideas        │ Start writing...     │
│ Tags           │ Shopping List        │                      │
│                │                      │                      │
│ Trash          │                      │                      │
│                │                      │                      │
│                │                      │                      │
└────────────────┴──────────────────────┴──────────────────────┘
```

The UI may differ visually from this example.

The important thing is that the architecture and interaction model are established.

---

# PHASE 1 END STATE

After completing Phase 1:

```text
Ubuntu
   ↓
Personal Notepad
   ↓
Tauri Desktop App
   ↓
React + TypeScript UI
   ↓
Three-pane note-taking interface
   ↓
Light/Dark/System theme
   ↓
Clean shared/platform architecture
```

The next phase will introduce the **real local storage architecture using SQLite + filesystem**, but that work must NOT be started during Phase 1.
