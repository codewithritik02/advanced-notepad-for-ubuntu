# PERSONAL NOTEPAD

# PHASE 2 — STORAGE ARCHITECTURE + SQLITE

---

# 1. PHASE OBJECTIVE

Implement the real **local-first storage architecture** for **Personal Notepad**.

Phase 1 established:

* Tauri desktop shell
* React + TypeScript frontend
* Rust/Tauri native layer
* three-pane UI
* navigation structure
* theme foundation
* reusable UI components
* mock note data
* basic application states

Phase 2 now replaces the temporary/mock data architecture with a **real persistent local data layer**.

The primary objectives of Phase 2 are:

1. Introduce SQLite as the local structured database.
2. Establish the application's permanent local data architecture.
3. Create the initial database schema.
4. Create database initialization logic.
5. Create database migration/versioning infrastructure.
6. Establish the application data directory.
7. Establish the filesystem attachment directory.
8. Establish backup directory structure for future phases.
9. Establish configuration directory structure where appropriate.
10. Create a clean Rust storage layer.
11. Create a frontend service/API layer for storage communication.
12. Remove dependency on mock note data for persistent application state.
13. Verify that data survives application restart.
14. Keep the architecture compatible with future Windows support.
15. Keep the architecture compatible with future search, attachments, backup, encryption and sync features.

This phase is about **storage infrastructure**, not about implementing the complete note editor.

---

# 2. PHASE 2 SCOPE

Phase 2 includes:

```text
SQLite database
Database initialization
Database schema
Database migrations
Database versioning
Application data directory
Database file
Attachments directory
Backups directory
Config directory
Storage abstraction
Rust storage layer
Tauri storage commands/API
Frontend storage service
Basic note persistence
Basic notebook persistence
Basic tag persistence
Settings persistence foundation
Database integrity verification
Safe database writes
Application restart persistence
Storage error handling
Storage path handling
Automated storage tests
```

Phase 2 may implement the minimum CRUD operations required to prove that the storage architecture actually works.

However, Phase 2 must NOT turn into full note-management implementation.

---

# 3. IMPORTANT PRODUCT RULES

The product name is:

```text
Personal Notepad
```

The application is:

```text
Offline-first
Local-first
Desktop-first
Cross-platform by architecture
```

The application must work without internet access.

Do NOT introduce:

* cloud storage
* cloud databases
* online accounts
* authentication
* sync servers
* AI
* OpenAI APIs
* LLMs
* telemetry services
* remote APIs
* mandatory internet services

The database must be local.

User data must remain on the user's machine.

---

# 4. PHASE 1 MUST NOT BE BROKEN

Phase 2 is an extension of Phase 1.

Do NOT unnecessarily redesign or replace:

* the three-pane UI
* sidebar architecture
* notes list layout
* editor placeholder
* top bar
* theme system
* reusable UI components
* application navigation
* existing keyboard foundation
* existing project structure

Only modify Phase 1 code where required to connect it to the new storage layer.

The visual appearance should remain substantially consistent unless a small change is necessary for storage integration.

---

# 5. STORAGE ARCHITECTURE PRINCIPLE

The most important rule of Phase 2 is:

> React must NOT directly manage SQLite.

The desired architecture is:

```text
React / TypeScript UI
        │
        ▼
Frontend Storage Service
        │
        ▼
Tauri Commands / IPC
        │
        ▼
Rust Application Layer
        │
        ▼
Storage Layer
        │
        ├── SQLite
        │
        └── Filesystem
```

The frontend must communicate with the Rust layer through a controlled interface.

Do NOT place SQL queries inside React components.

Do NOT allow UI components to know where the SQLite database is located.

Do NOT hardcode Linux filesystem paths in React.

---

# 6. CROSS-PLATFORM ARCHITECTURE

The application currently targets Ubuntu/Linux.

Windows support will be implemented in a future phase.

However, Phase 2 must avoid architecture that prevents Windows support.

The conceptual architecture should be:

```text
                 Personal Notepad
                         │
                Application Services
                         │
                  Storage Abstraction
                         │
             ┌───────────┴───────────┐
             │                       │
         SQLite Layer           Filesystem Layer
             │                       │
             └───────────┬───────────┘
                         │
                Platform Path Provider
                    ┌────┴────┐
                    │         │
                  Linux     Windows
                  future     future
```

The business logic should remain shared.

Platform-specific path resolution should use the appropriate Tauri/OS APIs rather than manually constructing Linux paths.

Do not implement fake Windows functionality.

Do not create duplicated storage logic for Linux and Windows.

---

# 7. TASK BREAKDOWN

Phase 2 must be completed in small independent tasks.

Do NOT implement all tasks at once.

Complete:

```text
TASK 1
Environment and Phase 1 inspection

TASK 2
Storage architecture design

TASK 3
SQLite dependency and database bootstrap

TASK 4
Application data directory management

TASK 5
Initial database schema

TASK 6
Database migration/versioning system

TASK 7
Rust repository/storage layer

TASK 8
Tauri storage API

TASK 9
Frontend storage service

TASK 10
Basic persistent note storage

TASK 11
Notebook storage foundation

TASK 12
Tag and note-tag storage foundation

TASK 13
Settings storage foundation

TASK 14
Filesystem directory architecture

TASK 15
Database integrity and safe-write handling

TASK 16
Replace mock-data dependency where appropriate

TASK 17
Storage error/loading states integration

TASK 18
Automated tests

TASK 19
Restart/recovery verification

TASK 20
Documentation and final Phase 2 verification
```

Each task must be performed separately.

After every task:

1. Inspect changes.
2. Compile.
3. Run relevant tests.
4. Launch the application when appropriate.
5. Fix errors.
6. Report exactly what changed.
7. Continue only after verification.

---

# 8. TASK 1 — ENVIRONMENT & PHASE 1 INSPECTION

## Objective

Understand the current Phase 1 project before modifying anything.

## Requirements

First inspect:

```text
package.json
src/
src-tauri/
README.md
tsconfig.json
Vite configuration
Tauri configuration
Cargo.toml
Rust source
existing tests
existing styles
existing services
existing types
```

Determine:

* current Tauri version
* current React version
* current TypeScript version
* current Rust version
* current package manager
* existing dependencies
* existing architecture
* current application entry point
* current Tauri command structure
* current state-management approach, if any
* current mock-data location
* current UI state architecture

Do NOT recreate the project.

Do NOT blindly overwrite files.

Do NOT upgrade unrelated dependencies.

Do NOT change the application name.

Do NOT change Phase 1 architecture unless technically necessary.

## Acceptance criteria

The agent can clearly identify:

* where frontend code lives
* where Rust code lives
* where Tauri commands live
* where services live
* where mock data lives
* where tests live
* how the current application launches

---

# 9. TASK 2 — STORAGE ARCHITECTURE DESIGN

## Objective

Define the storage boundaries before writing database code.

Create or update documentation describing:

```text
Frontend
   ↓
Storage Service
   ↓
Tauri IPC
   ↓
Rust Application Layer
   ↓
Repository / Storage Layer
   ↓
SQLite + Filesystem
```

Define responsibilities.

### React

Responsible for:

* displaying data
* collecting user input
* UI state
* calling frontend services

React must NOT:

* execute SQL
* know SQLite file paths
* manipulate the application's database directly

### Frontend Storage Service

Responsible for:

* calling Tauri storage commands
* converting IPC responses into TypeScript types
* exposing clean methods to UI/features

Example conceptual interface:

```text
storage.initialize()
storage.notes.list()
storage.notes.get(id)
storage.notes.create(...)
storage.notes.update(...)
storage.notes.delete(...)
storage.notebooks.list()
storage.tags.list()
storage.settings.get(...)
storage.settings.set(...)
```

Exact API names may be adjusted to fit the existing architecture.

### Rust Application Layer

Responsible for:

* validating requests
* coordinating repositories
* handling storage errors
* exposing safe Tauri commands

### Repository Layer

Responsible for:

* SQL queries
* database operations
* transactions
* mapping database rows to Rust models

### Filesystem Layer

Responsible for:

* application data directories
* attachments directory
* backups directory
* config directory

Do not mix filesystem path logic into UI code.

---

# 10. TASK 3 — SQLITE DEPENDENCY & DATABASE BOOTSTRAP

## Objective

Introduce SQLite in the Rust/Tauri layer.

Choose a mature Rust SQLite solution appropriate for the current Tauri/Rust versions.

Prefer a lightweight, actively maintained library.

The agent must inspect the current dependency tree before adding dependencies.

Avoid unnecessary database libraries.

## Requirements

Add the required Rust dependency/dependencies.

The database must be:

```text
SQLite
```

It must operate locally.

No external database server is allowed.

## Database bootstrap

Create a database initialization process.

Conceptual flow:

```text
Application startup
       ↓
Resolve application data directory
       ↓
Create required directories
       ↓
Open database.sqlite
       ↓
Enable appropriate SQLite settings
       ↓
Run migrations
       ↓
Validate database
       ↓
Application ready
```

The database initialization must be safe to run repeatedly.

Running initialization multiple times must NOT destroy existing data.

## Acceptance criteria

On first application startup:

```text
database.sqlite
```

is created automatically.

On subsequent startups:

* existing database is reused
* existing data remains intact
* migrations are checked
* data is not recreated or destroyed

---

# 11. TASK 4 — APPLICATION DATA DIRECTORY

## Objective

Create a proper user-data directory.

Do NOT store user data inside:

```text
src/
src-tauri/
installation directory
project directory
build directory
```

Do not use the current working directory for permanent user data.

Use the appropriate OS-specific application-data directory provided by the Tauri/platform APIs.

The application should conceptually maintain:

```text
Personal Notepad/
│
├── database.sqlite
│
├── attachments/
│
├── backups/
│
└── config/
```

The actual absolute path must be platform-appropriate.

Linux and Windows must resolve their data locations independently through the platform abstraction.

## Requirements

Create the directories safely.

The operation must be idempotent.

Running it repeatedly must not cause errors merely because directories already exist.

## Acceptance criteria

The application can report/verify its resolved data directory.

The database is stored there.

The application does not write persistent user data into its installation directory.

---

# 12. TASK 5 — INITIAL DATABASE SCHEMA

## Objective

Create the first production-oriented SQLite schema.

The initial schema must support the architecture defined in the overall Personal Notepad roadmap.

Required entities:

```text
notes
notebooks
tags
note_tags
attachments
settings
```

---

# 13. NOTES TABLE

Create a `notes` table.

Required conceptual fields:

```text
id
title
content
format
notebook_id
created_at
modified_at
is_favorite
is_pinned
is_deleted
deleted_at
```

Recommended semantics:

### id

Unique stable identifier.

Prefer UUID/text or another stable cross-platform identifier.

Do not use unstable UI indexes as IDs.

### title

Note title.

Must support Unicode.

### content

Actual note content.

Phase 2 should store content in SQLite.

The future filesystem architecture may evolve, but do not prematurely move normal note text to separate files.

### format

Supported values initially:

```text
txt
md
```

TXT is the primary format.

Markdown is supported as a secondary structured format.

### notebook_id

Nullable foreign-key reference to the notebook.

A note may exist without a notebook if the application design permits this.

### created_at

Creation timestamp.

### modified_at

Last modification timestamp.

### is_favorite

Boolean-like SQLite value.

### is_pinned

Boolean-like SQLite value.

### is_deleted

Boolean-like SQLite value.

### deleted_at

Nullable timestamp.

Trash behavior will be fully implemented in Phase 8, but the schema must be able to support it.

Do not implement complete Trash UI in Phase 2.

---

# 14. NOTEBOOKS TABLE

Create a `notebooks` table.

Fields:

```text
id
name
parent_id
created_at
modified_at
```

`parent_id` supports nested notebooks.

The schema must allow:

```text
Projects
   ├── Personal
   ├── Work
   └── Archive
```

Do not implement complete drag-and-drop notebook management in Phase 2.

## Important

The database design must prevent invalid hierarchy where practical.

Future phases will implement full hierarchy validation.

---

# 15. TAGS TABLE

Create:

```text
tags
```

Fields:

```text
id
name
created_at
```

Tags must support Unicode.

Tag management UI belongs to later phases.

---

# 16. NOTE_TAGS TABLE

Create the many-to-many relationship:

```text
note_tags
```

Conceptual fields:

```text
note_id
tag_id
```

Use a composite primary key:

```text
(note_id, tag_id)
```

Add appropriate foreign keys.

Prevent duplicate note-tag relationships.

---

# 17. ATTACHMENTS TABLE

Create:

```text
attachments
```

Conceptual fields:

```text
id
note_id
file_name
relative_path
mime_type
file_size
created_at
modified_at
```

The actual binary file must NOT be stored as a large SQLite BLOB by default.

SQLite stores attachment metadata.

The actual file will live under:

```text
attachments/<note-id>/
```

Full attachment functionality belongs to Phase 7.

Phase 2 only establishes the schema and filesystem foundation.

---

# 18. SETTINGS TABLE

Create:

```text
settings
```

Conceptual structure:

```text
key
value
```

Use a unique key.

The settings table will eventually store things such as:

```text
theme
editor_preferences
window_preferences
application_preferences
```

Do not implement every setting now.

At minimum, the architecture must support:

```text
theme
```

if Phase 1 theme persistence is migrated from temporary frontend storage.

---

# 19. DATABASE CONSTRAINTS

Use appropriate:

* primary keys
* foreign keys
* unique constraints
* NOT NULL constraints
* indexes
* defaults where appropriate

Enable SQLite foreign-key enforcement where supported.

Do not blindly add indexes everywhere.

Indexes should support actual current/future access patterns.

At minimum, consider indexes for:

```text
notes.modified_at
notes.notebook_id
notes.is_deleted
notes.is_favorite
note_tags.tag_id
attachments.note_id
notebooks.parent_id
```

The exact indexing strategy may be adjusted after reviewing the actual queries.

---

# 20. TIMESTAMP STRATEGY

Choose one consistent timestamp representation.

The same representation must be used throughout the application.

It must be:

* sortable
* unambiguous
* timezone-safe
* easy to convert into frontend dates

Prefer storing UTC timestamps.

Do not mix:

```text
local time
UTC
Unix seconds
Unix milliseconds
formatted display strings
```

without a clear conversion boundary.

Database values should represent actual timestamps, not UI-formatted strings.

---

# 21. TASK 6 — DATABASE MIGRATION & VERSIONING SYSTEM

## Objective

Create a migration architecture before the schema becomes large.

Do NOT rely on manually editing the production database schema.

Create an explicit migration system.

Conceptual:

```text
Migration 001
Initial schema

Migration 002
Future change

Migration 003
Future change
```

The exact implementation depends on the selected SQLite library.

## Requirements

Migrations must:

* run in order
* run only when needed
* be safe to execute on startup
* preserve existing data
* record migration/version state
* fail clearly if a migration cannot be applied

Never silently ignore a failed migration.

## Important

Do not create future migrations just for the sake of filling files.

Only create the initial migration required for the current schema.

## Acceptance criteria

Fresh database:

```text
migration 001
```

runs successfully.

Existing database:

* migration 001 is not unnecessarily reapplied
* existing data remains intact

---

# 22. TASK 7 — RUST STORAGE / REPOSITORY LAYER

## Objective

Create a clean Rust storage layer.

Avoid putting all database logic inside `main.rs`.

The exact folder structure can be adapted to the existing project, for example:

```text
src-tauri/src/
├── main.rs
├── commands/
├── storage/
│   ├── mod.rs
│   ├── database.rs
│   ├── migrations.rs
│   ├── models.rs
│   ├── repositories/
│   │   ├── notes.rs
│   │   ├── notebooks.rs
│   │   ├── tags.rs
│   │   ├── attachments.rs
│   │   └── settings.rs
│   └── paths.rs
├── errors.rs
└── state.rs
```

This is an example, not a mandatory exact structure.

Do not create unnecessary modules.

---

# 23. STORAGE MODELS

Define Rust models for database entities.

At minimum:

```text
Note
Notebook
Tag
NoteTag
Attachment
Setting
```

Models must have appropriate serialization support for Tauri IPC.

Do not expose raw SQL rows directly to React.

Create stable application-level response types.

---

# 24. REPOSITORY RESPONSIBILITIES

Repositories should encapsulate SQL.

Example conceptual API:

```text
NoteRepository

create()
get_by_id()
list()
update()
delete()
```

For Phase 2, implement only the operations required to verify persistence.

Do not implement every future feature.

Future phases can extend the repository layer.

---

# 25. DATABASE CONNECTION MANAGEMENT

Create a controlled database state/connection strategy.

Avoid opening a new uncontrolled database connection for every UI component.

The database must be initialized once as part of application startup or managed through a controlled state layer.

The exact connection-pooling strategy should be appropriate for the selected SQLite library.

The architecture must support:

* concurrent read requests where safe
* serialized writes where necessary
* transactions
* clean shutdown

Do not over-engineer a large database service.

---

# 26. TASK 8 — TAURI STORAGE API

## Objective

Expose a safe storage API to React.

Tauri commands should be explicit and typed.

Conceptual examples:

```text
initialize_storage
get_storage_info
create_note
get_note
list_notes
update_note
delete_note
list_notebooks
list_tags
get_setting
set_setting
```

Only expose commands that are actually required.

Do NOT expose a generic command such as:

```text
execute_sql
```

to the frontend.

The frontend must never be able to send arbitrary SQL.

---

# 27. IPC DESIGN RULES

Tauri commands must:

* validate inputs
* return structured responses
* return structured errors
* avoid panics for expected user/data errors
* avoid leaking internal SQL details unnecessarily

Do not return raw database driver errors directly to the UI.

Create application-level error categories where appropriate.

Example conceptual categories:

```text
StorageUnavailable
DatabaseInitializationFailed
DatabaseMigrationFailed
RecordNotFound
ValidationError
FilesystemError
PermissionDenied
UnknownStorageError
```

Exact naming can differ.

---

# 28. TASK 9 — FRONTEND STORAGE SERVICE

## Objective

Create a TypeScript service layer.

The UI must not directly call low-level Tauri commands everywhere.

Instead use something conceptually like:

```text
src/services/storage/
```

Possible structure:

```text
storage/
├── index.ts
├── notes.ts
├── notebooks.ts
├── tags.ts
└── settings.ts
```

Or another structure consistent with Phase 1.

The service should expose clean functions.

Example:

```text
notes.list()
notes.get(id)
notes.create(input)
notes.update(id, input)
notes.delete(id)
```

The UI should consume this service rather than knowing how IPC works.

---

# 29. TYPESCRIPT TYPES

Create frontend types corresponding to backend responses.

Do not duplicate types unnecessarily.

At minimum define types for:

```text
Note
Notebook
Tag
Attachment
Setting
```

Use strict TypeScript typing.

Avoid:

```text
any
```

unless there is a genuinely unavoidable boundary.

---

# 30. TASK 10 — BASIC PERSISTENT NOTE STORAGE

## Objective

Prove that notes can now be persisted.

This is the first point where real note data may be written to SQLite.

Implement the minimum functionality needed to demonstrate:

```text
Create note
       ↓
SQLite
       ↓
Read note
       ↓
Display note
```

A minimal note must contain:

```text
id
title
content
format
created_at
modified_at
is_favorite
is_pinned
is_deleted
```

Use sensible defaults.

Example default:

```text
title = "Untitled Note"
content = ""
format = "txt"
is_favorite = false
is_pinned = false
is_deleted = false
```

Exact UX may remain consistent with Phase 1.

---

# 31. NOTE PERSISTENCE TEST

The most important Phase 2 proof:

```text
Create note
   ↓
Enter content
   ↓
Save
   ↓
Close application
   ↓
Restart application
   ↓
Read database
   ↓
Note still exists
   ↓
Content is unchanged
```

This must work.

If Phase 1's editor is still only a placeholder and does not support actual editing, implement only the minimum temporary persistence test mechanism required to validate the storage architecture.

Do NOT turn Phase 2 into the complete editor implementation.

Complete note editing belongs to Phase 3.

---

# 32. TASK 11 — NOTEBOOK STORAGE FOUNDATION

## Objective

Create the database foundation for notebooks.

Implement the minimum repository functionality required to prove:

```text
create notebook
list notebooks
get notebook
```

Nested hierarchy must be representable through:

```text
parent_id
```

Do not implement:

* drag/drop
* full notebook tree UI
* move-note UI
* hierarchy editor
* notebook deletion workflows

Those belong to Phase 4.

---

# 33. TASK 12 — TAG STORAGE FOUNDATION

## Objective

Create the database foundation for tags.

Implement minimum repository functionality for:

```text
create tag
list tags
```

Support the note-tag relationship.

Do not implement complete tag management UI.

Do not implement filtering UI.

Do not implement search by tags.

Those belong to later phases.

---

# 34. TASK 13 — SETTINGS STORAGE FOUNDATION

## Objective

Create the foundation for persistent application settings.

The `settings` table should support key/value storage.

At minimum verify:

```text
set setting
get setting
update setting
```

If the Phase 1 theme currently uses temporary frontend persistence, it may now be migrated to SQLite.

However, do not force a full settings redesign.

The architecture should support future settings without requiring schema redesign.

---

# 35. TASK 14 — FILESYSTEM DIRECTORY ARCHITECTURE

## Objective

Establish the filesystem structure required by future phases.

The application data directory should conceptually contain:

```text
Personal Notepad/
│
├── database.sqlite
│
├── attachments/
│
├── backups/
│
└── config/
```

Create these directories safely.

## Attachments

The future structure is:

```text
attachments/
└── <note-id>/
    ├── image.png
    ├── document.pdf
    └── file.ext
```

Do NOT implement attachment upload/open/remove functionality in Phase 2.

Only establish the directory architecture.

## Backups

Future structure:

```text
backups/
├── backup-YYYYMMDD-HHMMSS.sqlite
└── ...
```

Do NOT implement the complete backup system yet.

## Config

Reserve:

```text
config/
```

for future application configuration where appropriate.

Do not duplicate data that belongs in SQLite without a reason.

---

# 36. FILESYSTEM SECURITY RULES

The application must never blindly accept arbitrary absolute filesystem paths from the UI for its internal storage.

Internal storage paths should be derived from:

```text
application data directory
```

and controlled identifiers such as:

```text
note id
attachment id
```

Prevent path traversal.

Reject or safely normalize values such as:

```text
../../file
../../../etc/passwd
absolute external paths
```

when constructing internal application paths.

Do not allow a note title or user-provided filename to determine an unrestricted filesystem location.

---

# 37. TASK 15 — DATABASE INTEGRITY & SAFE WRITES

## Objective

Make the storage layer reliable.

Implement appropriate SQLite configuration.

Where appropriate, use:

```text
transactions
foreign keys
safe commits
```

Do not sacrifice data safety for convenience.

## Safe write behavior

A write should not leave the database in a partially modified logical state.

For operations affecting multiple tables, use transactions.

Example:

```text
Create note
   ↓
Insert note
   ↓
Insert related records
   ↓
Commit
```

If an operation fails:

```text
Rollback
```

where appropriate.

---

# 38. DATABASE INITIALIZATION FAILURE

If database initialization fails:

Do NOT silently start the application as though storage works.

Show a clear application error state.

Example:

```text
Unable to initialize local storage.

Personal Notepad could not open its local database.

Please check that the application has permission to access its data directory.
```

Do not expose raw SQL internals to normal users.

For development builds, useful technical details may be logged appropriately.

Do not leave permanent debug logs in production code.

---

# 39. PERMISSION ERRORS

Handle cases where the application cannot:

* create the data directory
* create the database
* open the database
* write to the database
* create attachment directories

The application should return a structured error.

Do not crash because of expected filesystem permission failures.

---

# 40. DISK FULL / WRITE FAILURE

The storage layer must propagate write failures cleanly.

Do not assume every write succeeds.

The UI should be able to distinguish a storage failure from a successful save.

Full user-facing recovery behavior will be hardened in Phase 13.

Phase 2 only needs a reliable error propagation path.

---

# 41. TASK 16 — MOCK DATA TRANSITION

## Objective

Remove the architectural dependency on Phase 1 mock data.

Phase 1 mock notes may have been used for UI development.

Now:

```text
SQLite
```

must become the source of truth for persisted notes.

However, do not unnecessarily rewrite the entire UI.

The preferred flow is:

```text
UI
 ↓
Feature/service
 ↓
Storage service
 ↓
Tauri
 ↓
Rust
 ↓
SQLite
```

Mock data may remain in isolated development/test fixtures if useful.

It must NOT be used as production application data.

---

# 42. INITIAL APPLICATION DATA

The application may create a minimal initial state on first launch.

For example:

```text
No notes yet
```

or optionally a welcome note if that is consistent with the product decision.

If a welcome note is created automatically:

* it must be inserted through the normal storage layer
* it must not be hardcoded as fake UI data
* it must not be recreated every time the app starts

Prefer avoiding unnecessary default content unless needed for testing.

---

# 43. TASK 17 — STORAGE LOADING / ERROR STATES

## Objective

Connect the Phase 1 application states to real storage.

The application must support:

```text
Loading
Success
Empty
Error
```

For example:

### Loading

```text
Loading notes...
```

### Empty

```text
No notes yet

Create your first note to get started.
```

### Error

```text
Unable to load notes.

Please try again.
```

Do not display technical database errors directly to normal users.

---

# 44. STORAGE INITIALIZATION STATE

At application startup:

```text
Application launches
       ↓
Storage initializes
       ↓
Database migrations run
       ↓
Storage ready
       ↓
UI loads persistent data
```

The UI must not assume storage is ready before initialization completes.

Avoid race conditions such as:

```text
UI requests notes
```

before:

```text
database initialization
```

has finished.

---

# 45. TASK 18 — AUTOMATED TESTING

## Objective

Create automated tests for the storage layer.

Testing is mandatory.

At minimum test:

### Database initialization

```text
Fresh database initializes successfully.
```

### Migration

```text
Initial migration runs successfully.
```

### Repeated initialization

```text
Initializing twice does not destroy data.
```

### Note creation

```text
Create note → record exists.
```

### Note retrieval

```text
Create note → retrieve same note.
```

### Note update

```text
Create note → update → retrieve → new data exists.
```

### Note deletion foundation

```text
Delete/soft-delete operation behaves according to current schema contract.
```

Do not implement full Trash behavior here.

### Notebook

```text
Create notebook → retrieve notebook.
```

### Nested notebook

```text
Parent notebook
      ↓
Child notebook
```

`parent_id` relationship is preserved.

### Tags

```text
Create tag → retrieve tag.
```

### Note-tag relation

```text
Note ↔ Tag
```

relationship is correctly stored.

### Settings

```text
Set setting → get setting → correct value.
```

### Unicode

Test:

```text
Hindi
English
emoji
special characters
```

Example test content:

```text
आज की मीटिंग के नोट्स 📝
```

The database must preserve it exactly.

### Persistence

Write data.

Close database/application context.

Reopen.

Verify data still exists.

---

# 46. TEST DATABASE ISOLATION

Tests must NOT modify the user's real Personal Notepad database.

Use:

```text
temporary database
```

or another isolated test storage mechanism.

Never run automated tests against the user's production database.

Clean test data after tests where appropriate.

---

# 47. TASK 19 — RESTART & RECOVERY VERIFICATION

## Objective

Prove the core promise of Phase 2:

> Local data survives application restart.

Perform a manual verification.

### Test 1 — Note persistence

```text
Create a note.
Write unique content.
Save.
Close application.
Restart.
Open note.
Verify exact content.
```

### Test 2 — Multiple notes

Create several notes.

Restart.

Verify all remain.

### Test 3 — Unicode

Create content containing:

```text
Hindi
English
emoji
symbols
```

Restart.

Verify exact content.

### Test 4 — Notebook

Create a notebook.

Restart.

Verify it remains.

### Test 5 — Tag

Create a tag.

Restart.

Verify it remains.

### Test 6 — Settings

Change the theme setting if theme persistence was migrated.

Restart.

Verify the stored setting is correctly read.

---

# 48. DATABASE FILE VERIFICATION

Verify that:

```text
database.sqlite
```

exists in the application data directory.

Verify that the database is a valid SQLite database.

Do not claim success merely because the application window opened.

Actually verify:

```text
database exists
schema exists
migration exists
records can be written
records can be read
records survive restart
```

---

# 49. STORAGE LOGGING

Use logging only where useful.

Development logs may include:

```text
Storage initialized
Database opened
Migration completed
Storage error
```

Do NOT log:

* note content unnecessarily
* sensitive user data
* credentials
* future encryption keys
* arbitrary private content

Do not leave noisy debug logging in production builds.

---

# 50. SECURITY CONSIDERATIONS

Phase 2 is not the encryption phase.

Do NOT implement full database encryption yet.

However, the architecture must prepare for future security work.

## Required now

* Keep database local.
* Do not expose SQLite directly to frontend.
* Do not expose arbitrary SQL execution.
* Validate IPC inputs.
* Prevent path traversal.
* Use application data directories.
* Avoid logging private note content.
* Handle filesystem permissions correctly.
* Use parameterized SQL queries.
* Never construct SQL by string-concatenating user input.

Example conceptually:

```text
GOOD

SELECT ...
WHERE id = ?

BAD

SELECT ...
WHERE id = '<user input>'
```

Use the parameterization facilities provided by the selected SQLite library.

---

# 51. SQL INJECTION PREVENTION

Every user-provided value must be parameterized.

This includes:

* note title
* note content
* notebook name
* tag name
* setting key/value
* filenames
* IDs received from UI

Do not dynamically concatenate user input into SQL statements.

---

# 52. DATA DIRECTORY SECURITY

Do not assume the installation directory is writable.

Do not store:

```text
database.sqlite
```

next to the application executable.

This is especially important for:

* `.deb` installation
* future Windows installation

User data belongs in the OS-appropriate user data location.

---

# 53. NO CLOUD DEPENDENCY

Phase 2 must function completely offline.

Do not add:

```text
Firebase
Supabase
PostgreSQL server
MongoDB cloud
remote API
analytics backend
```

The only storage dependency is local storage.

---

# 54. PERFORMANCE REQUIREMENTS

Phase 2 does not need the final performance optimization from Phase 13.

However:

* do not load the entire database repeatedly for every UI component
* avoid opening unnecessary database connections
* use indexes where justified
* use transactions for multi-step writes
* keep IPC payloads reasonable
* avoid storing huge binary attachments in SQLite

The architecture should remain capable of handling:

```text
1000+
```

notes later.

---

# 55. DATABASE SCHEMA EVOLUTION

Do not make schema assumptions that prevent future phases.

The schema must be extensible for:

```text
search
attachments
trash
internal links
backup
encryption
sync
```

Future features will extend the schema through migrations.

Never manually modify an installed user's database schema without a migration.

---

# 56. FUTURE COMPATIBILITY

Phase 2 must leave clear extension points for:

## Phase 3

Note creation/editing:

```text
notes repository
```

## Phase 4

Notebook hierarchy:

```text
parent_id
```

## Phase 5

Tags/favorites/metadata:

```text
tags
note_tags
is_favorite
is_pinned
```

## Phase 6

Search:

```text
notes
tags
notebooks
```

and potentially a search index later.

Do not implement the search engine now.

## Phase 7

Attachments:

```text
attachments
attachments/<note-id>/
```

## Phase 8

Trash:

```text
is_deleted
deleted_at
```

## Phase 9

Internal links:

future tables/relations through migrations.

## Phase 11

Backups:

```text
backups/
```

## Phase 12

Encryption:

storage architecture must remain replaceable/extensible.

## Phase 16

Windows:

platform-specific path handling must remain isolated.

---

# 57. DO NOT IMPLEMENT IN PHASE 2

This section is extremely important.

Do NOT implement:

```text
❌ Full note editor
❌ Rich text editor
❌ Markdown editor
❌ Markdown parser
❌ TXT file export
❌ Markdown export
❌ HTML export
❌ PDF export
❌ Print
❌ Full autosave behavior
❌ Advanced undo/redo
❌ Full notebook UI
❌ Drag-and-drop notebooks
❌ Full tag management UI
❌ Search engine
❌ Search indexing
❌ Attachment upload UI
❌ Attachment open/remove workflow
❌ Full Trash UI
❌ Restore workflow
❌ Permanent delete workflow
❌ Internal links
❌ Backlinks
❌ Import
❌ Export
❌ Backup system
❌ Backup rotation
❌ Database encryption
❌ Encryption key management
❌ Sync
❌ Accounts
❌ Cloud services
❌ Windows implementation
❌ Windows installer
❌ .deb packaging
❌ AI
```

Phase 2 is fundamentally:

```text
REAL LOCAL STORAGE
```

not the complete notes application.

---

# 58. ERROR HANDLING REQUIREMENTS

The storage layer must handle at minimum:

```text
Database cannot be opened
Database cannot be created
Migration failure
Database locked
Database read failure
Database write failure
Record not found
Invalid input
Invalid ID
Filesystem directory creation failure
Permission denied
Disk/write failure
Malformed data
```

Expected errors must not cause uncontrolled application crashes.

---

# 59. USER-FACING ERROR PRINCIPLE

Technical errors should be converted into understandable messages.

Bad:

```text
SQLITE_CONSTRAINT_FOREIGNKEY: FOREIGN KEY constraint failed
```

Better:

```text
The requested item could not be saved because one of its related items no longer exists.
```

Technical details may remain available in development diagnostics.

---

# 60. DATA VALIDATION

Validate at the storage boundary.

Examples:

### Note title

Allow:

```text
Unicode
spaces
normal punctuation
```

Do not impose unnecessarily restrictive rules.

### Note ID

Must be a valid expected identifier.

### Notebook name

Must not be an invalid empty value if the product requires a name.

### Tag name

Same principle.

### Settings

Validate recognized setting keys where appropriate.

Do not blindly accept arbitrary malformed data from the frontend.

---

# 61. TRANSACTION REQUIREMENTS

Use transactions when an operation involves multiple related writes.

Example:

```text
Create note
+
Create note-tag relations
```

should be atomic if implemented together.

Either:

```text
everything succeeds
```

or:

```text
everything rolls back
```

Do not leave partially completed relationships.

---

# 62. FOREIGN KEY REQUIREMENTS

Enable foreign keys.

Verify relationships such as:

```text
notes.notebook_id
note_tags.note_id
note_tags.tag_id
attachments.note_id
notebooks.parent_id
```

behave consistently.

Be deliberate about deletion behavior.

Do not implement destructive cascade behavior without understanding future Trash requirements.

For example, do not create a schema where deleting a notebook unexpectedly destroys all notes.

Phase 4 will define notebook deletion semantics.

---

# 63. NOTE DELETION IN PHASE 2

The schema already contains:

```text
is_deleted
deleted_at
```

because future Trash functionality requires it.

However, Phase 2 must NOT implement the complete Trash feature.

If a repository delete method is required for testing, clearly define whether it is:

```text
soft delete
```

or an internal test operation.

Prefer the architecture that allows Phase 8 to implement proper Trash behavior without schema replacement.

---

# 64. DATABASE CONNECTION LIFECYCLE

The application must have a clear lifecycle:

```text
Application starts
      ↓
Resolve paths
      ↓
Create directories
      ↓
Open database
      ↓
Run migrations
      ↓
Validate database
      ↓
Register storage state
      ↓
Register Tauri commands
      ↓
Start UI
```

If storage initialization fails:

```text
Storage unavailable
      ↓
Show appropriate error state
```

Do not silently continue as if the database exists.

---

# 65. FRONTEND STARTUP FLOW

The frontend should conceptually behave like:

```text
App starts
   ↓
Check storage readiness
   ↓
Loading
   ↓
Storage ready
   ↓
Load persistent application data
   ↓
Render UI
```

The UI must not flash misleading mock data while real storage is loading.

---

# 66. NO DUPLICATE SOURCE OF TRUTH

After Phase 2 integration:

The production application must NOT maintain separate:

```text
mock notes
+
SQLite notes
```

as two competing sources of truth.

SQLite is the source of truth for persisted notes.

Temporary test fixtures are allowed only in isolated tests/development tooling.

---

# 67. README DOCUMENTATION

Update:

```text
README.md
```

with:

```text
Personal Notepad
```

Current status:

```text
Phase 2 — Storage Architecture + SQLite
```

Document:

## Storage

```text
React
 ↓
Tauri IPC
 ↓
Rust storage layer
 ↓
SQLite
```

## User data

Explain that persistent data is stored in the operating system's application-data location.

Conceptually:

```text
Personal Notepad/
├── database.sqlite
├── attachments/
├── backups/
└── config/
```

## Database

Document:

* SQLite
* migrations
* schema
* local-first design

## Offline

State clearly:

```text
The application does not require an internet connection for local note storage.
```

---

# 68. ARCHITECTURE DOCUMENTATION

Create or update an appropriate document under:

```text
docs/
```

For example:

```text
docs/storage-architecture.md
```

Document:

```text
Frontend
   ↓
Storage Service
   ↓
Tauri Commands
   ↓
Rust Application Layer
   ↓
Repositories
   ↓
SQLite / Filesystem
```

Also document the distinction between:

```text
shared logic
```

and:

```text
platform-specific path handling
```

---

# 69. DATABASE SCHEMA DOCUMENTATION

Document the initial entities:

```text
notes
notebooks
tags
note_tags
attachments
settings
```

Include their purpose and relationships.

A simple conceptual relationship diagram is encouraged:

```text
Notebook
   │
   └──────< Notes
               │
               ├──────< Note Tags >────── Tag
               │
               └──────< Attachments
```

Do not claim that future relationships already exist.

---

# 70. DEPENDENCY RULES

Before adding any dependency:

1. Check whether the existing stack already provides the required capability.
2. Check whether the dependency is necessary.
3. Check whether it is compatible with the current Rust/Tauri versions.
4. Prefer mature and maintained libraries.
5. Avoid adding multiple libraries for the same purpose.

Do not install unrelated packages.

Do not add a frontend database library if SQLite is being managed by Rust.

---

# 71. CODE QUALITY REQUIREMENTS

Before declaring Phase 2 complete, inspect for:

```text
unused imports
dead code
unnecessary dependencies
duplicated storage logic
duplicated types
unsafe SQL construction
hardcoded paths
debug prints
unhandled errors
unnecessary abstractions
```

Rust code should be idiomatic.

TypeScript should remain strict.

Do not suppress compiler errors merely to get a build passing.

Do not use:

```text
@ts-ignore
```

or equivalent workarounds without a genuine reason.

---

# 72. TESTING MATRIX

At minimum verify:

| Test                        | Expected                  |
| --------------------------- | ------------------------- |
| Fresh install/start         | Database created          |
| Second startup              | Existing database reused  |
| Migration                   | Runs successfully         |
| Create note                 | Stored                    |
| Read note                   | Correct data              |
| Update note                 | Updated data              |
| Restart                     | Data remains              |
| Unicode                     | Exact content preserved   |
| Notebook                    | Stored                    |
| Nested notebook             | Parent relation preserved |
| Tag                         | Stored                    |
| Note-tag                    | Relationship preserved    |
| Settings                    | Stored/retrieved          |
| Attachment directory        | Created                   |
| Backup directory            | Created                   |
| Config directory            | Created                   |
| Permission failure          | Graceful error            |
| Invalid ID                  | Graceful error            |
| SQL input containing quotes | Stored safely             |
| Database unavailable        | Error state               |

---

# 73. SECURITY TESTS

Verify that a note containing SQL-like content such as:

```text
Robert'); DROP TABLE notes;--
```

is treated as normal text.

It must NOT execute SQL.

Also test:

```text
../../something
```

where path-related input is relevant.

It must not escape the application's data directory.

---

# 74. PERFORMANCE SANITY TEST

Phase 2 does not require the full Phase 13 performance suite.

However, perform a basic sanity check with a reasonable number of records.

For example:

```text
100 notes
500 notes
1000 notes
```

Verify that:

* database initialization remains reasonable
* listing notes remains functional
* application does not obviously freeze
* database queries use appropriate indexes where needed

Do not spend Phase 2 implementing advanced search optimization.

---

# 75. ACCEPTANCE CRITERIA

Phase 2 is complete only when all of the following are true.

## Application

* Personal Notepad launches successfully.
* Phase 1 UI still works.
* Tauri desktop shell still works.
* React UI still works.
* TypeScript compiles.
* Rust compiles.
* No critical console errors.

## Database

* SQLite is integrated.
* Database is created automatically.
* Database is stored outside the installation/project directory.
* Initial schema exists.
* Migrations work.
* Database versioning exists.
* Foreign keys are enabled appropriately.
* Parameterized queries are used.

## Storage

* Application data directory is resolved correctly.
* `database.sqlite` is created there.
* `attachments/` directory exists.
* `backups/` directory exists.
* `config/` directory exists.
* Filesystem paths are platform-aware.

## Data

The application can persist the minimum required data:

```text
Notes
Notebooks
Tags
Note-tag relationships
Settings
```

Attachments have database metadata/storage architecture established but full attachment functionality is NOT required.

## Persistence

The following must survive application restart:

```text
notes
notebooks
tags
settings
```

where the corresponding test path has been implemented.

## Architecture

* React does not execute SQL.
* React does not know database paths.
* Tauri IPC provides the boundary.
* Rust owns SQLite operations.
* SQL is isolated in repositories/storage modules.
* Platform-specific path handling is isolated.
* Future Windows support does not require duplicating business logic.

## Reliability

* Database initialization is idempotent.
* Migration failure is handled.
* Storage errors are propagated.
* Permission failures are handled.
* Write failures are handled.
* No uncontrolled application crashes for expected storage errors.

## Security

* No arbitrary SQL IPC command exists.
* SQL uses parameters.
* User input is validated.
* Internal paths prevent traversal.
* Sensitive note content is not unnecessarily logged.
* No cloud dependency exists.

## Testing

Automated tests exist for:

* initialization
* migration
* note persistence
* notebook persistence
* tag persistence
* relationships
* settings
* Unicode
* restart/reopen persistence
* relevant error conditions

---

# 76. WHAT PHASE 2 SHOULD LOOK LIKE AT THE END

Conceptually:

```text
Ubuntu
   ↓
Personal Notepad
   ↓
Tauri Desktop App
   ↓
React + TypeScript UI
   ↓
Storage Service
   ↓
Tauri IPC
   ↓
Rust Storage Layer
   ↓
SQLite
   │
   ├── notes
   ├── notebooks
   ├── tags
   ├── note_tags
   ├── attachments
   └── settings
```

Filesystem:

```text
Personal Notepad/
│
├── database.sqlite
│
├── attachments/
│   └── <future note-id folders>
│
├── backups/
│
└── config/
```

The exact OS-specific parent directory is determined by the platform.

---

# 77. PHASE 2 LIMITATIONS

At the end of Phase 2, the following may still be incomplete:

```text
Full note editor
Autosave
Advanced undo/redo
Full notebook management
Drag/drop
Tag management UI
Search
Attachments UI
Trash UI
Internal links
Import/export
Backup automation
Encryption
Sync
Windows build
.deb packaging
```

This is expected.

Phase 2 establishes the storage foundation required by those later phases.

---

# 78. IMPORTANT AI CODING AGENT RULES

When implementing Phase 2, follow these rules strictly.

## Rule 1 — Inspect before changing

Before modifying any file:

1. Inspect the existing Phase 1 project.
2. Understand the current architecture.
3. Identify existing functionality.
4. Reuse existing structures where appropriate.
5. Avoid recreating working functionality.

Never blindly overwrite working files.

---

## Rule 2 — One task at a time

Do NOT implement Task 1 through Task 20 together.

Complete:

```text
Task
↓
Verification
↓
Next Task
```

---

## Rule 3 — Preserve Phase 1

Do not unnecessarily rewrite:

* UI
* layout
* theme
* navigation
* reusable components
* keyboard handling

Storage integration should be additive where possible.

---

## Rule 4 — SQLite belongs to Rust

Do not put SQLite logic in:

```text
React
TypeScript components
browser storage
```

Rust/Tauri owns the database.

---

## Rule 5 — No arbitrary SQL from frontend

Never expose:

```text
execute_sql(query)
```

to React.

Only expose explicit application operations.

---

## Rule 6 — No speculative future implementation

Do not implement:

```text
search
backup engine
encryption
sync
attachments UI
Trash UI
Windows
```

just because Phase 2 prepares for them.

Prepare architecture only.

---

## Rule 7 — Minimal dependencies

Do not add dependencies without justification.

Do not introduce multiple overlapping libraries.

---

## Rule 8 — Offline-first

The entire Phase 2 storage system must work without internet.

---

## Rule 9 — Security by default

Use:

```text
parameterized SQL
validated IPC
safe filesystem paths
controlled application directories
```

---

## Rule 10 — Preserve user data

Never run destructive commands against an existing production database during normal development.

Do not automatically:

```text
DROP DATABASE
DROP TABLE
DELETE ALL DATA
```

to solve migration/development problems.

If a schema change is required, use migrations.

---

## Rule 11 — No data loss during migration

Migrations must preserve existing user data unless a deliberate future migration explicitly defines a safe transformation.

---

## Rule 12 — No fake success

Do not report:

```text
PASS
```

unless the check was actually performed.

Do not claim:

```text
database persistence works
```

unless restart/reopen persistence has been verified.

---

## Rule 13 — Test after every task

After each task:

1. Run relevant tests.
2. Run TypeScript checks.
3. Run Rust checks.
4. Build when appropriate.
5. Launch the application when appropriate.
6. Fix errors.
7. Confirm previous functionality still works.

---

## Rule 14 — Do not over-engineer

The storage architecture must be clean but understandable.

Do not create:

```text
20 abstraction layers
```

for simple CRUD operations.

Use abstractions where they provide real value.

---

## Rule 15 — No AI

Do not add any:

```text
AI package
LLM
chatbot
assistant
AI API
OpenAI integration
AI autocomplete
AI summarization
AI writing
```

---

# 79. REQUIRED RESPONSE FORMAT FOR THE AI CODING AGENT

After completing EACH task, report exactly:

```text
TASK COMPLETED:

Task X — <name>

OBJECTIVE:

- ...

CHANGES:

- ...
- ...
- ...

FILES CREATED:

- ...

FILES MODIFIED:

- ...

DATABASE CHANGES:

- ...

DEPENDENCIES ADDED:

- None

or

- ...

VERIFICATION:

- TypeScript: PASS/FAIL
- Rust: PASS/FAIL
- Tests: PASS/FAIL
- Build: PASS/FAIL
- Application launch: PASS/FAIL
- Database initialization: PASS/FAIL
- Persistence test: PASS/FAIL

STORAGE LOCATION:

- Resolved application data directory: PASS/FAIL
- database.sqlite: PASS/FAIL

SECURITY CHECK:

- Parameterized SQL: PASS/FAIL
- Path validation: PASS/FAIL
- No arbitrary SQL IPC: PASS/FAIL

NOT IMPLEMENTED YET:

- ...

PHASE 1 PRESERVATION:

- Existing Phase 1 behavior preserved: YES/NO

NOTES FOR NEXT TASK:

- ...
```

Do not omit failed checks.

Do not mark untested items as PASS.

---

# 80. FINAL PHASE 2 VERIFICATION COMMANDS

The exact commands depend on the project's package manager and Tauri version.

The agent must inspect the project and use the correct commands.

At minimum verify equivalent checks for:

```text
Frontend dependency installation
TypeScript type checking
Frontend build
Rust cargo check
Rust tests
Tauri build
Application launch
Database initialization
Database migration
Persistence
```

Do not invent commands that do not exist in the project.

---

# 81. FINAL PHASE 2 RESULT

At the end of Phase 2:

```text
Personal Notepad
        │
        ▼
Tauri Desktop Application
        │
        ▼
React + TypeScript
        │
        ▼
Frontend Storage Service
        │
        ▼
Tauri IPC
        │
        ▼
Rust Storage Layer
        │
        ├───────────────┐
        ▼               ▼
     SQLite         Filesystem
        │               │
        ├── notes       ├── attachments/
        ├── notebooks   ├── backups/
        ├── tags        └── config/
        ├── note_tags
        ├── attachments
        └── settings
```

The critical achievement is:

```text
Personal Notepad now has a real,
persistent, local SQLite storage foundation.
```

The application must be able to close and reopen without losing persisted local data.

---

# 82. NEXT PHASE

After Phase 2 is completely verified, the next phase is:

```text
PHASE 3 — NOTE CREATION & EDITING
```

Phase 3 will build the actual note creation/editing experience on top of the storage foundation created here.

Phase 3 will introduce:

* real note creation
* real note editing
* TXT primary format
* Markdown secondary format
* autosave
* save on close
* undo/redo
* keyboard shortcuts
* cursor preservation
* empty-note handling
* large-text handling

Do NOT start Phase 3 implementation during Phase 2.

---

# PHASE 2 END STATE

The final architecture after Phase 2 should be:

```text
Ubuntu
   ↓
Personal Notepad
   ↓
Tauri
   ↓
React + TypeScript
   ↓
Three-pane UI
   ↓
Storage Service
   ↓
Tauri IPC
   ↓
Rust
   ↓
SQLite + Filesystem
   ↓
Persistent Local Data
```

The application is now ready for Phase 3 to build the real note-editing workflow on top of the storage foundation.
