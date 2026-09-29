# PERSONAL NOTEPAD

# PHASE 4 — NOTEBOOK / FOLDER SYSTEM

---

## 1. PHASE OBJECTIVE

Phase 4 ka objective Personal Notepad mein **Notebook / Folder System** implement karna hai.

Is phase ke complete hone ke baad user:

* notebooks create kar sake
* notebooks rename kar sake
* notebooks delete kar sake
* nested notebooks create kar sake
* notes ko notebook ke andar move kar sake
* notes ko root level par rakh sake
* sidebar mein notebook hierarchy dekh sake
* notebook select karke uske notes dekh sake
* selected notebook ke andar new note create kar sake
* notebook selection ke baad note list correctly filter ho
* application restart ke baad notebook hierarchy aur note assignments preserve rahen

Notebook system ko SQLite-backed hona chahiye.

Phase 2 mein establish ki gayi database/storage architecture ko reuse karo.

Phase 3 mein implement ki gayi note creation/editing/autosave functionality ko preserve karo.

Is phase mein notebook/folder functionality add karni hai — note editor ko rewrite nahi karna.

---

# 2. CURRENT PROJECT STATE

Expected project state:

### Phase 1

Application shell available:

* Tauri
* React
* TypeScript
* Vite
* three-pane desktop layout
* sidebar
* note list
* editor
* top bar
* theme system
* reusable UI components
* loading/empty/error states
* keyboard foundation

### Phase 2

Persistent local storage available:

* SQLite
* migration/versioning foundation
* notes table
* notebooks table
* tags-related schema if already created
* attachments-related schema if already created
* settings table if already created
* platform-specific application data directory

### Phase 3

Real notes available:

* create note
* open note
* edit title
* edit content
* autosave
* save on close
* undo/redo
* keyboard shortcuts
* cursor preservation
* soft delete foundation
* TXT primary format
* optional MD format metadata

Phase 4 must build on all of the above.

---

# 3. IMPORTANT PRODUCT RULES

These rules are mandatory.

### Rule 1 — Local-first

Notebook data must remain local.

No:

* cloud notebook storage
* account requirement
* remote API
* online database
* synchronization

---

### Rule 2 — SQLite is the source of truth

Notebook hierarchy and note-to-notebook relationships must be persisted in SQLite.

Do not use:

* localStorage as the primary database
* sessionStorage
* JSON files as the primary notebook database
* in-memory-only notebook state

React state is only UI state.

---

### Rule 3 — Preserve Phase 2 schema

Before modifying the database schema:

1. inspect the existing Phase 2 migrations
2. inspect the existing `notebooks` table
3. inspect the existing `notes.notebook_id`
4. inspect migration/versioning implementation

Do not create a second competing notebook table.

If Phase 2 already created the required schema, reuse it.

---

### Rule 4 — Preserve Phase 3

Do not rewrite:

* note editor
* autosave engine
* note CRUD
* undo/redo
* cursor handling
* save-on-close

Notebook functionality must integrate with the existing note system.

---

### Rule 5 — No advanced organization features yet

Do NOT implement:

* tags
* advanced search
* saved searches
* favorites filtering
* smart folders
* backlinks
* attachments
* trash UI
* import/export
* backup UI
* encryption
* sync

Those belong to later phases.

---

### Rule 6 — Original UI

The interface can follow familiar desktop note-taking conventions, but must remain an original Personal Notepad implementation.

Do not copy another application's UI pixel-for-pixel.

---

### Rule 7 — Nested notebooks are required

The data model must support:

```text
Root
├── Work
│   ├── Projects
│   └── Meetings
├── Personal
│   └── Travel
└── Ideas
```

The hierarchy must not be limited to one level.

---

# 4. PHASE 4 SCOPE

Implement:

### Notebook CRUD

* create notebook
* rename notebook
* delete notebook

### Hierarchy

* root notebooks
* child notebooks
* arbitrary practical nesting depth
* expand/collapse

### Note assignment

* move note into notebook
* move note back to root
* change note's notebook

### Sidebar

* notebook tree
* selected notebook state
* expand/collapse state
* notebook context actions

### Note filtering

When notebook is selected:

```text
Selected notebook
        ↓
load notes belonging to notebook
        ↓
display notes in note list
```

When root/all notes is selected:

```text
Root/All Notes
        ↓
load appropriate notes
```

The exact semantics between "All Notes" and "Unfiled Notes" must be kept distinct.

Recommended:

```text
All Notes
    = every non-deleted note

Unfiled
    = notes where notebook_id IS NULL
```

---

# 5. EXPLICIT NON-GOALS

Do NOT implement in Phase 4:

* tag creation
* tag assignment
* tag filtering
* search engine
* search ranking
* attachment handling
* trash/recovery UI
* permanent deletion
* import/export
* backup
* encryption
* sync
* Windows-specific behavior
* Ubuntu packaging
* AI functionality

---

# 6. ARCHITECTURE

Use this conceptual architecture:

```text
React UI
   ↓
Notebook Feature Layer
   ↓
Notebook Service / Tauri Bridge
   ↓
Tauri Commands
   ↓
Notebook Service / Repository
   ↓
SQLite
```

For note movement:

```text
React
   ↓
Note Service
   ↓
notes_update / note repository
   ↓
UPDATE notes
SET notebook_id = ...
```

The frontend should never directly execute SQL.

---

# 7. PROJECT STRUCTURE

First inspect the existing project.

Do not blindly create these files if equivalent files already exist.

Use existing Phase 2/3 architecture where possible.

A recommended organization is:

```text
src/
├── features/
│   ├── notes/
│   │   ├── components/
│   │   ├── hooks/
│   │   ├── services/
│   │   ├── types/
│   │   └── utils/
│   │
│   └── notebooks/
│       ├── components/
│       │   ├── NotebookTree.tsx
│       │   ├── NotebookTreeItem.tsx
│       │   ├── NotebookContextMenu.tsx
│       │   ├── CreateNotebookDialog.tsx
│       │   └── RenameNotebookDialog.tsx
│       │
│       ├── hooks/
│       │   ├── useNotebooks.ts
│       │   └── useNotebookSelection.ts
│       │
│       ├── services/
│       │   └── notebookService.ts
│       │
│       ├── types/
│       │   └── notebook.ts
│       │
│       └── utils/
│           └── notebookTree.ts
│
├── services/
│   └── tauri.ts
│
└── ...
```

Rust side may use:

```text
src-tauri/
└── src/
    ├── commands/
    │   └── notebooks.rs
    │
    ├── services/
    │   └── notebooks.rs
    │
    ├── repositories/
    │   └── notebooks.rs
    │
    └── ...
```

But:

> If Phase 2 already has a storage/repository architecture, extend that architecture instead of creating duplicate repository layers.

---

# 8. DATA MODEL

The expected notebook model is:

```text
Notebook
├── id
├── name
├── parent_id
├── created_at
└── modified_at
```

Recommended conceptual schema:

```sql
notebooks
---------
id
name
parent_id
created_at
modified_at
```

Where:

```text
parent_id = NULL
```

means:

```text
Root notebook
```

And:

```text
parent_id = <another notebook ID>
```

means:

```text
Child notebook
```

---

# 9. NOTE RELATIONSHIP

The existing `notes` table should contain:

```text
notebook_id
```

Relationship:

```text
Notebook 1 ──────── * Notes
```

A note may have:

```text
notebook_id = notebook ID
```

or:

```text
notebook_id = NULL
```

for an unfiled/root-level note.

Do not duplicate notes when moving them between notebooks.

Only update the relationship.

---

# 10. NOTEBOOK ID RULES

Notebook IDs must be stable.

Use the ID strategy already established in Phase 2.

If Phase 2 uses UUIDs:

```text
use UUID
```

If Phase 2 uses another stable ID strategy:

```text
continue using it
```

Do not change the ID strategy in Phase 4.

---

# 11. NOTEBOOK NAME RULES

Notebook name must:

* be a string
* be trimmed appropriately
* not be only whitespace
* have a reasonable maximum length
* preserve Unicode
* support Hindi and other languages
* support normal punctuation

Example:

```text
Work
Personal
Projects 2026
यात्रा
Ideas & Research
```

Avoid overly restrictive ASCII-only validation.

---

# 12. DUPLICATE NOTEBOOK NAMES

Sibling notebooks may use the same name unless the existing product rules explicitly prohibit it.

Recommended:

```text
Work
Work
```

can exist if they have different IDs/parents.

However:

```text
same parent + exact same name
```

may be rejected if the implementation chooses uniqueness.

Whichever behavior is selected must be documented and consistent.

Recommended MVP behavior:

> Allow duplicate names, because notebook identity is based on ID rather than name.

---

# 13. TASK BREAKDOWN

Complete the following tasks **one at a time**.

Do not implement the entire phase in one uncontrolled change.

---

# TASK 1 — Inspect Phase 2 and Phase 3 Architecture

## Objective

Understand the existing storage and note architecture before changing anything.

## Inspect

Find:

* SQLite initialization
* migration system
* `notebooks` table
* `notes` table
* `notebook_id`
* note repository/service
* Tauri commands
* frontend Tauri bridge
* note list loading
* note selection state
* sidebar implementation
* current mock/sidebar data
* existing state management

## Verify

Determine:

```text
Where does notebook data currently live?
Where are notes loaded?
How is notebook_id represented?
How are Tauri commands registered?
How are migrations handled?
```

## Important

Do not modify code unnecessarily.

## Verification

```text
TASK COMPLETED:
Task 1 — Inspect Phase 2 and Phase 3 Architecture

CHANGES:
- ...

FILES CREATED:
- None

FILES MODIFIED:
- None

DEPENDENCIES ADDED:
- None

VERIFICATION:
- Existing SQLite architecture inspected: PASS/FAIL
- Existing notebook schema inspected: PASS/FAIL
- Existing note relationship inspected: PASS/FAIL
- Existing Tauri command architecture inspected: PASS/FAIL
- Existing sidebar architecture inspected: PASS/FAIL

NOT IMPLEMENTED YET:
- Notebook functionality

NOTES FOR NEXT TASK:
- ...
```

---

# TASK 2 — Verify / Complete Notebook Database Schema

## Objective

Ensure the database can represent nested notebooks.

Expected:

```text
notebooks
---------
id
name
parent_id
created_at
modified_at
```

And:

```text
notes.notebook_id
```

must exist.

## Rules

If schema already exists:

```text
reuse it
```

If it is incomplete:

```text
create a new migration
```

Never rewrite an already-applied migration just to add a feature.

## Foreign key behavior

Decide carefully what should happen when a notebook is deleted.

Recommended:

```text
deleting notebook
        ↓
notes become unfiled
        ↓
child notebooks are handled safely
```

Do NOT cascade-delete notes.

A notebook deletion must never accidentally delete note content.

## Recommended behavior

For notebook deletion:

```text
Notebook A
├── Note 1
├── Note 2
└── Child Notebook
```

Deleting Notebook A should not delete:

```text
Note 1
Note 2
Child Notebook
```

Instead, the implementation should explicitly define a safe strategy.

Recommended MVP:

1. Prevent deletion if children exist, OR
2. Require explicit confirmation and safely re-parent children.

Prefer the simpler and safer option:

> Do not allow deletion of a notebook that still contains child notebooks until the children are moved/deleted.

For notes:

> Notes become unfiled.

## Verification

Test migration from a clean database.

Also test migration against an existing Phase 3 database.

---

# TASK 3 — Create Notebook Domain Types

## Objective

Create a single typed representation of a notebook.

Example conceptual TypeScript type:

```ts
type Notebook = {
  id: string;
  name: string;
  parentId: string | null;
  createdAt: string;
  modifiedAt: string;
};
```

Use the actual naming convention already established by Phase 2/3.

Do not create multiple incompatible notebook models.

## Requirements

Notebook type must support:

* stable ID
* name
* parent relationship
* timestamps

Optional derived frontend properties such as:

```text
children
depth
expanded
```

should not be stored in SQLite.

They are UI/tree state.

---

# TASK 4 — Implement Notebook Backend CRUD

## Objective

Implement safe backend operations.

Minimum commands:

```text
notebooks_list
notebooks_get
notebooks_create
notebooks_update
notebooks_delete
```

Possible exact naming:

```text
notebook_list
notebook_get
notebook_create
notebook_update
notebook_delete
```

Use the project's existing command naming convention.

---

## Create

Input:

```text
name
parentId
```

Output:

```text
Notebook
```

Behavior:

* validate name
* validate parent
* generate ID
* create timestamps
* insert into SQLite
* return created notebook

---

## Get

Input:

```text
id
```

Output:

```text
Notebook
```

If not found:

```text
NotFound
```

---

## List

Return all notebooks needed to build the hierarchy.

Recommended:

```text
ORDER BY parent_id, name
```

But hierarchy ordering should ultimately be handled deterministically by the frontend tree builder.

---

## Update

Phase 4 primarily needs:

```text
rename notebook
```

Input:

```text
id
name
```

Update:

```text
name
modified_at
```

Do not update `parent_id` through rename.

---

## Delete

Delete only the notebook record after validation.

Before deleting:

* verify notebook exists
* verify no child notebooks remain
* update notes assigned to it to `NULL`

Do not delete note content.

Use a transaction.

Conceptually:

```text
BEGIN

UPDATE notes
SET notebook_id = NULL
WHERE notebook_id = ?

DELETE FROM notebooks
WHERE id = ?

COMMIT
```

If any operation fails:

```text
ROLLBACK
```

---

# TASK 5 — Add Notebook Service / Frontend Bridge

## Objective

Create a typed frontend service between React and Tauri.

Example:

```text
notebookService.list()
notebookService.get(id)
notebookService.create(name, parentId)
notebookService.rename(id, name)
notebookService.delete(id)
```

The React components should not directly scatter Tauri `invoke()` calls everywhere.

Use one service abstraction.

---

# TASK 6 — Load Real Notebooks

## Objective

Replace any placeholder notebook/sidebar data with SQLite-backed notebooks.

Application startup:

```text
App starts
   ↓
load notebooks
   ↓
build notebook tree
   ↓
render sidebar
```

No hard-coded notebook list should remain as the source of truth.

---

# TASK 7 — Build Notebook Tree

## Objective

Transform flat database results into a nested UI tree.

Database returns:

```text
A parent=NULL
B parent=A
C parent=A
D parent=B
```

UI should produce:

```text
A
├── B
│   └── D
└── C
```

Create a deterministic tree-building utility.

Requirements:

* handle root notebooks
* handle child notebooks
* handle multiple levels
* preserve stable IDs
* sort deterministically
* detect malformed relationships
* avoid infinite recursion

---

# TASK 8 — Add Notebook Sidebar UI

## Objective

Add the notebook tree to the existing Phase 1 sidebar.

Example:

```text
SIDEBAR

All Notes
Unfiled

NOTEBOOKS
▾ Work
    Projects
    Meetings
▸ Personal
Ideas

+ New Notebook
```

Do not redesign the entire sidebar.

Integrate into the existing layout.

---

# TASK 9 — Expand / Collapse Notebook Tree

## Objective

Users should be able to expand and collapse nested notebooks.

Example:

```text
▾ Work
    Projects
    Meetings
```

Collapsed:

```text
▸ Work
```

Expansion state is UI state.

Do not store expansion state in SQLite.

---

## Persistence

For Phase 4 MVP:

Expansion state may reset when the application restarts.

Do not build a complex persistence system unless the existing architecture makes it trivial.

---

# TASK 10 — Notebook Selection

## Objective

Selecting a notebook should update the active application context.

State should conceptually contain:

```text
selectedNotebookId
```

Possible values:

```text
null
```

or:

```text
notebook ID
```

But keep the meaning explicit.

Recommended navigation model:

```text
All Notes
    selectedNotebookId = special/all state

Unfiled
    selectedNotebookId = null
    filter = unfiled

Notebook
    selectedNotebookId = notebook ID
```

Do not overload `null` to mean both "All Notes" and "Unfiled."

Use an explicit navigation type.

Example:

```ts
type NoteLocation =
  | { type: "all" }
  | { type: "unfiled" }
  | { type: "notebook"; notebookId: string };
```

This prevents ambiguous state.

---

# TASK 11 — Filter Note List by Notebook

## Objective

When the user selects a notebook, the note list must show only notes assigned to that notebook.

Example:

```text
Work
```

selected:

```text
Notes list
----------------
Project Plan
Meeting Notes
Release Checklist
```

Do not load all notes and filter blindly in React if the dataset can grow.

Prefer a database query:

```sql
SELECT ...
FROM notes
WHERE notebook_id = ?
  AND deleted = ...
ORDER BY modified_at DESC
```

This keeps filtering efficient.

---

# TASK 12 — Add All Notes and Unfiled Navigation

## Objective

Clearly separate:

### All Notes

Shows:

```text
all non-deleted notes
```

regardless of notebook.

### Unfiled

Shows:

```text
notebook_id IS NULL
```

### Specific notebook

Shows:

```text
notebook_id = selectedNotebookId
```

These three navigation states must not be confused.

---

# TASK 13 — Create Notebook

## Objective

Allow the user to create a notebook.

Entry points may include:

```text
+ New Notebook
```

and optionally:

```text
Notebook context menu → New Child Notebook
```

Recommended behavior:

If user selects:

```text
Work
```

and chooses:

```text
New Notebook
```

the new notebook should become:

```text
Work
└── New Notebook
```

If no notebook is selected:

```text
New Notebook
```

becomes a root notebook.

---

## Dialog

Use a small reusable dialog/modal.

Example:

```text
Create Notebook

Name
[_____________________]

[Cancel] [Create]
```

Requirements:

* focus input automatically
* Enter submits
* Escape cancels
* empty name rejected
* whitespace-only rejected
* trim unnecessary outer whitespace
* show validation error clearly

---

# TASK 14 — Rename Notebook

## Objective

Allow notebook renaming.

Possible entry points:

```text
double click
context menu
keyboard shortcut if appropriate
```

Keep interaction simple.

Example:

```text
Work
```

becomes:

```text
Work 2026
```

Only the notebook name should change.

Its:

* ID
* children
* notes
* parent
* creation time

must remain unchanged.

---

# TASK 15 — Delete Notebook Safely

## Objective

Implement safe notebook deletion.

Example:

```text
Delete "Work"?
```

Warning:

```text
Notes inside this notebook will become unfiled.
The notes themselves will not be deleted.
```

This distinction is critical.

---

## Child notebooks

Recommended behavior:

If:

```text
Work
└── Projects
```

then deleting `Work` should be blocked while `Projects` exists.

Message:

```text
This notebook contains sub-notebooks.
Move or delete the sub-notebooks before deleting this notebook.
```

Do not silently restructure hierarchy.

---

## Notes

If:

```text
Work
├── Note A
└── Note B
```

and Work is deleted:

```text
Note A → unfiled
Note B → unfiled
```

The note content must remain intact.

---

# TASK 16 — Move Note to Notebook

## Objective

Allow a note to be moved between notebooks.

Minimum required operations:

```text
Unfiled → Work
Work → Personal
Personal → Work
Work → Unfiled
```

---

## UI

A simple implementation may use:

```text
Move to Notebook
```

menu/dialog.

Example:

```text
Move "Project Plan"

○ Unfiled
○ Work
  ○ Projects
  ○ Meetings
○ Personal
```

Then:

```text
[Cancel] [Move]
```

---

## Requirements

Moving a note must:

```text
UPDATE notes
SET notebook_id = ?
```

It must not:

* duplicate the note
* create a new note
* modify content
* modify title
* reset created_at
* reset undo history unnecessarily

`modified_at` may be updated because metadata changed.

---

# TASK 17 — Optional Drag-and-Drop Note Movement

This task is optional only if it can be implemented cleanly without destabilizing the app.

Possible UX:

```text
Note
  ↓ drag
Notebook
```

Do not add a large drag-and-drop dependency for this feature.

If native/simple React drag-and-drop can be implemented safely:

```text
implement
```

Otherwise:

```text
skip
```

and retain the explicit "Move to Notebook" action.

Do not allow optional UX work to delay core notebook functionality.

---

# TASK 18 — New Note Inside Selected Notebook

## Objective

Integrate Phase 3 note creation with notebook selection.

If:

```text
Work
```

is selected and user clicks:

```text
New Note
```

the note should automatically receive:

```text
notebook_id = Work.id
```

Flow:

```text
Selected Notebook
       ↓
New Note
       ↓
Create note with notebook_id
       ↓
Open editor
       ↓
Autosave
```

If:

```text
Unfiled
```

is selected:

```text
notebook_id = NULL
```

If:

```text
All Notes
```

is selected:

```text
new note behavior should follow the existing Phase 3 default
```

Recommended default:

```text
notebook_id = NULL
```

because "All Notes" is not a notebook.

---

# TASK 19 — Preserve Note Selection and Editor State

## Objective

Notebook navigation must not unnecessarily destroy editor state.

When selecting another notebook:

1. save/flush the current note
2. update note list
3. select appropriate note
4. preserve editor stability

Do not cause:

```text
cursor jumps
```

or:

```text
editor remounts
```

during normal notebook operations.

---

# TASK 20 — Empty Notebook State

If a notebook has no notes:

```text
Work

No notes in this notebook.

Create a new note to get started.
```

Provide a clear action:

```text
+ New Note
```

Do not display unrelated global notes.

---

# TASK 21 — Empty Sidebar State

If no notebooks exist:

```text
NOTEBOOKS

No notebooks yet.

+ New Notebook
```

The rest of the application must continue working normally.

A user must not be forced to create a notebook.

Notes can exist unfiled.

---

# TASK 22 — Notebook Loading State

While notebooks are loading:

```text
NOTEBOOKS
Loading...
```

Do not make the entire application unusable if notebook loading is slow.

The editor and other stable UI should remain structurally intact where possible.

---

# TASK 23 — Notebook Error State

If notebook loading fails:

```text
Could not load notebooks.

[Retry]
```

Do not silently show fake notebook data.

Do not silently fall back to hard-coded notebooks.

---

# TASK 24 — Handle Missing Parent References

The database should normally maintain valid parent relationships.

However, defensive frontend/backend handling should exist.

If a notebook references a missing parent:

```text
parent_id = nonexistent ID
```

the tree builder must not crash or recurse infinitely.

Possible safe behavior:

```text
treat malformed node as root
```

and log a development diagnostic.

Do not expose database internals to the user.

---

# TASK 25 — Prevent Circular Notebook Relationships

A notebook hierarchy must be a tree.

Invalid:

```text
A → B
B → C
C → A
```

Phase 4 does not need a complex hierarchy editor, but backend validation must prevent circular parent relationships if parent changes are introduced.

Since Phase 4 MVP may not expose arbitrary "move notebook" functionality, the simplest safe approach is:

> Do not implement notebook re-parenting yet.

Create notebooks with a valid parent and allow nesting through creation.

Notebook re-parenting can be added later if required.

---

# TASK 26 — Keyboard Navigation

Support practical keyboard interaction.

At minimum:

```text
Enter
```

for focused dialog submission.

```text
Escape
```

for closing dialogs.

Existing note shortcuts must remain functional:

```text
Ctrl/Cmd + N
Ctrl/Cmd + Z
Ctrl/Cmd + Shift + Z
Ctrl/Cmd + S
```

Do not hijack existing editor shortcuts.

If notebook-specific shortcuts are added, document them.

Do not create shortcuts merely for feature completeness.

---

# TASK 27 — Notebook Context Menu

Add a lightweight context menu if the existing UI system supports it.

Possible actions:

```text
New Child Notebook
Rename
Delete
```

Do not add:

* duplicate
* export
* share
* sync
* AI
* advanced sorting

Those are outside Phase 4.

---

# TASK 28 — Update Note List After Notebook Changes

Examples:

### Moving a note out

If current notebook is:

```text
Work
```

and note is moved to:

```text
Personal
```

the note should disappear from Work's list immediately after successful save.

### Moving a note into current notebook

The note should appear immediately after successful move.

Avoid requiring an application restart.

---

# TASK 29 — Update Sidebar After Notebook Changes

After:

```text
create
rename
delete
```

the notebook tree should update without application restart.

Prefer targeted state updates where simple.

A complete reload is acceptable for MVP if:

* reliable
* fast
* does not reset unrelated editor state

Do not overengineer caching.

---

# TASK 30 — Persistence / Restart Verification

Restart the application.

Verify:

```text
Notebook hierarchy preserved
Notebook names preserved
Notebook IDs preserved
Note assignments preserved
Unfiled notes preserved
Selected note content preserved
```

Example:

Before restart:

```text
Work
├── Projects
│   └── Project Plan
└── Meetings
    └── Weekly Meeting

Personal
└── Travel Plan
```

After restart:

```text
same hierarchy
same note assignments
same content
```

---

# TASK 31 — Unicode and Special Characters

Test notebook names such as:

```text
Work
Personal
यात्रा
旅行
Café
Ideas & Research
Project / Planning
2026 — Goals
```

Verify:

* creation
* display
* rename
* persistence
* selection
* restart

No ASCII-only assumptions.

---

# TASK 32 — Large Hierarchy Testing

Create a realistic hierarchy:

```text
Work
├── Projects
│   ├── Project A
│   ├── Project B
│   └── Project C
├── Meetings
│   ├── Weekly
│   └── Monthly
└── Archive
    ├── 2025
    └── 2026
```

Verify:

* tree renders correctly
* expansion works
* selection works
* note filtering works
* no duplicate nodes
* no recursive rendering crash

Do not create an artificial enormous benchmark unless needed.

---

# TASK 33 — Performance Check

Notebook operations should remain responsive.

Avoid:

```text
N database queries for N tree nodes
```

Prefer:

```text
one notebook list query
        ↓
frontend tree construction
```

For note filtering:

```text
one SQL query for selected notebook
```

Do not load every note into React just to filter one notebook if the dataset can grow.

---

# TASK 34 — Error Handling

Define clear error categories.

Possible categories:

```text
ValidationError
NotFound
DatabaseError
Conflict
InvalidHierarchy
DeleteBlocked
StorageUnavailable
```

Frontend messages should be human-readable.

Example:

Backend:

```text
DeleteBlocked
```

UI:

```text
This notebook contains sub-notebooks.
Move them first before deleting this notebook.
```

Never show raw SQL errors to users.

---

# TASK 35 — Transaction Safety

Notebook deletion must be transactional.

Example:

```text
BEGIN
    unassign notes
    delete notebook
COMMIT
```

If failure:

```text
ROLLBACK
```

Never leave the database in a partial state such as:

```text
notebook deleted
but notes still referencing invalid ID
```

---

# TASK 36 — Logging Rules

Development diagnostics may log:

```text
Notebook creation failed
Notebook deletion failed
Invalid hierarchy
Database operation failed
```

Do not log:

```text
full note content
```

Do not log sensitive user data unnecessarily.

---

# TASK 37 — Preserve Theme and UI System

Notebook components must support existing:

```text
Light
Dark
System
```

Do not introduce hard-coded colors that break themes.

Use existing design tokens/components.

---

# TASK 38 — Preserve Desktop Layout

Do not redesign the three-pane layout.

Existing structure:

```text
┌──────────────────────────────────────────────┐
│                 TOP BAR                      │
├──────────┬────────────────┬──────────────────┤
│ SIDEBAR  │ NOTE LIST      │ EDITOR           │
│          │                │                  │
│ Notebook │ Notes          │ Title            │
│ Tree     │                │ Content          │
│          │                │                  │
└──────────┴────────────────┴──────────────────┘
```

Notebook tree belongs primarily in the sidebar.

Do not turn notebooks into a new full-screen page.

---

# TASK 39 — Note Count

Optional but recommended:

Display a small note count beside notebooks.

Example:

```text
Work        12
Personal     5
```

Important:

This should only be implemented if it can be done efficiently.

Do not issue one SQL query per notebook.

If note counts are added:

Prefer one aggregated query or existing backend support.

If implementation becomes unnecessarily complex:

Skip note counts for Phase 4.

---

# TASK 40 — Cleanup Placeholder Notebook Data

After real notebook functionality is working:

Remove obsolete:

* hard-coded notebook arrays
* fake notebook data
* placeholder notebook selection logic
* unused mock notebook utilities

Do not remove mock note data if Phase 3 still uses it elsewhere without verifying.

By the end of Phase 4, notebook-related UI must use the real database.

---

# TASK 41 — Update Documentation

Document:

* notebook data model
* parent/child relationship
* note-to-notebook relationship
* deletion behavior
* All Notes semantics
* Unfiled semantics
* notebook creation behavior
* limitations

Especially document:

```text
Deleting a notebook does not delete its notes.
```

---

# TASK 42 — Automated Tests

Add tests using the existing testing infrastructure.

Do not introduce a large testing framework solely for this phase unless necessary.

### Backend tests

Test:

```text
create notebook
get notebook
list notebooks
rename notebook
delete notebook
nested notebook
invalid parent
missing notebook
delete notebook with children
note unassignment on notebook deletion
```

### Note assignment tests

Test:

```text
note → notebook
notebook → another notebook
notebook → unfiled
```

### Tree tests

Test:

```text
flat list → tree
multiple roots
multiple children
deep nesting
missing parent
empty list
```

---

# TASK 43 — Manual QA

Perform the following exact workflow.

## Test A — Create root notebook

```text
Launch app
→ New Notebook
→ Work
→ Create
```

Expected:

```text
Work appears in sidebar.
```

---

## Test B — Create child notebook

```text
Select Work
→ New Child Notebook
→ Projects
```

Expected:

```text
Work
└── Projects
```

---

## Test C — Create note inside notebook

```text
Select Projects
→ New Note
→ type content
```

Expected:

```text
note.notebook_id = Projects.id
```

---

## Test D — Restart

```text
Close app
→ reopen
```

Expected:

```text
Work
└── Projects
    └── note
```

and exact note content remains.

---

## Test E — Move note

```text
Projects
→ Move note
→ Work
```

Expected:

```text
Projects no longer shows note
Work shows note
```

---

## Test F — Unfile note

```text
Move note
→ Unfiled
```

Expected:

```text
note appears in Unfiled
```

---

## Test G — Rename

```text
Work
→ Rename
→ Work 2026
```

Expected:

```text
children remain
notes remain
note assignments remain
ID remains unchanged
```

---

## Test H — Delete notebook

```text
Delete Work 2026
```

Expected:

```text
notebook disappears
notes remain
notes become unfiled
```

---

## Test I — Child deletion protection

If:

```text
Work
└── Projects
```

attempt:

```text
Delete Work
```

Expected:

```text
operation blocked
```

No data loss.

---

# TASK 44 — Large Text / Editor Regression Test

Phase 4 must not break Phase 3.

Create a large note.

Then:

```text
Move note to notebook
Rename notebook
Switch notebook
Return to note
```

Verify:

* content unchanged
* title unchanged
* cursor behavior remains correct
* autosave remains functional
* undo/redo remains functional

---

# TASK 45 — Application Restart Regression Test

Test:

```text
Create notebook
Create child notebook
Create note
Edit note
Move note
Rename notebook
Close app
Reopen
```

Everything must survive.

---

# TASK 46 — Build and Type Verification

Run the project's actual commands.

At minimum:

```text
npm / pnpm / yarn typecheck
```

depending on project.

Rust:

```text
cargo check
```

Tests:

```text
cargo test
```

Frontend tests if configured:

```text
npm test
```

or equivalent.

Build:

```text
npm run build
```

and the project's Tauri build/check command.

Do not invent commands if the repository uses different scripts.

Inspect:

```text
package.json
src-tauri/Cargo.toml
```

and use the project's actual commands.

---

# TASK 47 — Final Cleanup

Before declaring Phase 4 complete:

Remove:

* unused notebook imports
* dead mock data
* duplicate notebook types
* duplicate service functions
* debug logs
* temporary UI
* unused dependencies
* commented-out experimental code

Do not perform unrelated refactoring.

---

# 13. NOTEBOOK API CONTRACT

The exact names may follow the existing project convention.

Conceptual API:

## Create

```text
notebook_create
```

Input:

```json
{
  "name": "Work",
  "parentId": null
}
```

Output:

```json
{
  "id": "...",
  "name": "Work",
  "parentId": null,
  "createdAt": "...",
  "modifiedAt": "..."
}
```

---

## List

```text
notebook_list
```

Output:

```json
[
  {
    "id": "...",
    "name": "Work",
    "parentId": null,
    "createdAt": "...",
    "modifiedAt": "..."
  }
]
```

---

## Get

```text
notebook_get
```

Input:

```json
{
  "id": "..."
}
```

---

## Rename

```text
notebook_update
```

Input:

```json
{
  "id": "...",
  "name": "Work 2026"
}
```

---

## Delete

```text
notebook_delete
```

Input:

```json
{
  "id": "..."
}
```

---

# 14. NOTE API ADDITION

Existing Phase 3 note API should be extended only where required.

For example:

```text
notes_create
```

should support:

```json
{
  "title": "Project Plan",
  "content": "",
  "format": "txt",
  "notebookId": "..."
}
```

Existing note update should support:

```text
notebookId
```

for moving a note.

Do not create a separate duplicate "move note" database model.

A move is simply:

```text
update note.notebook_id
```

---

# 15. STATE MODEL

Avoid ambiguous state such as:

```ts
selectedNotebookId: string | null
```

being used for everything.

Prefer:

```ts
type NoteLocation =
  | { type: "all" }
  | { type: "unfiled" }
  | { type: "notebook"; notebookId: string };
```

Then:

```text
All Notes
→ { type: "all" }

Unfiled
→ { type: "unfiled" }

Work
→ { type: "notebook", notebookId: "..." }
```

This prevents accidental bugs.

---

# 16. NOTE LIST QUERY BEHAVIOR

### All Notes

Conceptually:

```sql
SELECT ...
FROM notes
WHERE deleted = 0
ORDER BY modified_at DESC;
```

### Unfiled

```sql
SELECT ...
FROM notes
WHERE deleted = 0
AND notebook_id IS NULL
ORDER BY modified_at DESC;
```

### Notebook

```sql
SELECT ...
FROM notes
WHERE deleted = 0
AND notebook_id = ?
ORDER BY modified_at DESC;
```

Use the actual deleted-state representation established in Phase 2/3.

---

# 17. DELETE SEMANTICS

This distinction is extremely important.

Phase 4 notebook deletion:

```text
Delete notebook
        ↓
Notebook record removed
        ↓
Notes are NOT deleted
        ↓
Notes become unfiled
```

Phase 3 note deletion:

```text
Delete note
        ↓
Existing soft-delete behavior
```

Phase 8 later:

```text
Trash
Recovery
Permanent deletion
```

Do not implement the full Trash system here.

---

# 18. UI BEHAVIOR

Recommended sidebar:

```text
────────────────────

ALL NOTES

Unfiled

NOTEBOOKS

▾ Work
    ▸ Projects
    Meetings

▸ Personal

Ideas

────────────────────

+ New Notebook
```

Selecting:

```text
Work
```

updates the note list.

Selecting:

```text
Projects
```

updates the note list.

Selecting:

```text
All Notes
```

shows all non-deleted notes.

Selecting:

```text
Unfiled
```

shows notes without a notebook.

---

# 19. CREATE NOTE BEHAVIOR

### Current location = notebook

```text
Create Note
    ↓
notebook_id = selected notebook
```

### Current location = Unfiled

```text
Create Note
    ↓
notebook_id = NULL
```

### Current location = All Notes

```text
Create Note
    ↓
notebook_id = NULL
```

This should remain predictable.

---

# 20. UI CONFIRMATION RULES

Do not ask unnecessary confirmations for:

```text
create notebook
rename notebook
move note
```

Confirmation should be used for destructive actions:

```text
delete notebook
```

And clearly state the consequence:

```text
The notebook will be deleted.
Its notes will become unfiled.
The notes themselves will not be deleted.
```

---

# 21. ACCESSIBILITY

Notebook UI should support:

* keyboard focus
* visible focus state
* Enter activation
* Escape to close dialogs
* accessible labels
* buttons with meaningful names
* tree items with understandable semantics

Where practical, use ARIA tree semantics:

```text
role="tree"
role="treeitem"
aria-expanded
aria-selected
```

Do not add accessibility attributes blindly; ensure they match actual behavior.

---

# 22. PERFORMANCE RULES

Do not:

* fetch the entire database repeatedly
* reload all notes on every tree expansion
* create one query per notebook
* recreate the whole application state unnecessarily
* remount the editor unnecessarily

Preferred:

```text
load notebooks once
       ↓
build tree locally
```

And:

```text
select notebook
       ↓
query only relevant notes
```

---

# 23. SECURITY / DATA SAFETY

All SQL queries must be parameterized.

Do not construct:

```text
SQL = "DELETE ... WHERE id = '" + id + "'"
```

Use parameters.

Validate IDs before database operations.

Do not trust arbitrary frontend input.

Do not expose raw database errors to the UI.

Do not log note content.

No network calls are required.

---

# 24. DEPENDENCY RULE

Avoid adding dependencies unless necessary.

Do NOT add a large tree library merely to render:

```text
5–50 notebooks
```

A recursive React component is sufficient.

Do NOT add a state-management library solely for notebook selection if existing React state/context is sufficient.

Do NOT add drag-and-drop libraries unless the feature genuinely requires them.

Prefer existing:

* React
* TypeScript
* existing UI components
* existing state architecture
* existing Tauri bridge
* existing SQLite layer

---

# 25. WHAT MUST NOT CHANGE FROM PHASE 1

Preserve:

* application name
* window configuration
* Tauri setup
* three-pane layout
* sidebar foundation
* note list foundation
* editor foundation
* theme system
* light/dark/system modes
* reusable UI components
* desktop behavior
* keyboard foundation

---

# 26. WHAT MUST NOT CHANGE FROM PHASE 2

Preserve:

* SQLite
* migration system
* data directory
* database location
* database connection architecture
* existing tables
* existing ID strategy
* existing timestamp strategy
* existing storage abstractions

Only add migrations when genuinely required.

---

# 27. WHAT MUST NOT CHANGE FROM PHASE 3

Preserve:

* note CRUD
* note editor
* title editing
* content editing
* autosave
* save-on-close
* undo
* redo
* cursor preservation
* TXT support
* MD metadata support
* note soft-delete behavior

Notebook integration must not regress note editing.

---

# 28. PHASE 4 FINAL ACCEPTANCE CRITERIA

Phase 4 is complete only when all of the following are true.

### Notebook creation

* [ ] User can create a root notebook.
* [ ] User can create a child notebook.
* [ ] Notebook is stored in SQLite.
* [ ] Notebook appears immediately.

### Notebook hierarchy

* [ ] Nested notebooks work.
* [ ] Multiple root notebooks work.
* [ ] Expand/collapse works.
* [ ] Tree does not recurse infinitely.
* [ ] Hierarchy survives restart.

### Notebook rename

* [ ] Notebook can be renamed.
* [ ] ID remains unchanged.
* [ ] Child notebooks remain attached.
* [ ] Notes remain attached.

### Notebook deletion

* [ ] Delete requires confirmation.
* [ ] Notebook with child notebooks cannot be accidentally deleted.
* [ ] Notes are not deleted.
* [ ] Notes become unfiled.
* [ ] Database remains consistent.

### Note assignment

* [ ] Note can move into notebook.
* [ ] Note can move to another notebook.
* [ ] Note can become unfiled.
* [ ] Note content remains unchanged.
* [ ] Note title remains unchanged.

### Note filtering

* [ ] All Notes works.
* [ ] Unfiled works.
* [ ] Notebook filtering works.
* [ ] Deleted notes remain excluded according to Phase 3 rules.

### New note

* [ ] New note inside notebook is assigned automatically.
* [ ] New note in Unfiled remains unfiled.
* [ ] New note in All Notes follows defined default behavior.

### Persistence

* [ ] Close/reopen preserves notebooks.
* [ ] Close/reopen preserves hierarchy.
* [ ] Close/reopen preserves note assignments.
* [ ] Close/reopen preserves note content.

### Editor regression

* [ ] Autosave still works.
* [ ] Save-on-close still works.
* [ ] Undo/redo still works.
* [ ] Cursor does not unexpectedly jump.
* [ ] Large notes still work.

### UI

* [ ] Light theme works.
* [ ] Dark theme works.
* [ ] System theme works.
* [ ] Empty states work.
* [ ] Loading states work.
* [ ] Error states work.
* [ ] Existing three-pane layout remains intact.

### Code quality

* [ ] No duplicate notebook architecture.
* [ ] No obsolete mock notebook data.
* [ ] No unnecessary dependency.
* [ ] No raw SQL from frontend.
* [ ] No raw database errors shown to users.
* [ ] No note content logged.
* [ ] No network dependency introduced.

---

# 29. REQUIRED TEST SCENARIO

Run this complete scenario before declaring the phase complete.

```text
1. Launch Personal Notepad.

2. Create:
   Work

3. Inside Work create:
   Projects

4. Inside Projects create:
   Project A

5. Create a note:
   Project Plan

6. Enter substantial content.

7. Verify autosave.

8. Close and reopen the app.

9. Verify:
   Work
      └── Projects
           └── Project Plan

10. Rename:
    Work → Work 2026

11. Verify Project A / Project Plan remains correctly related.

12. Move Project Plan:
    Projects → Work 2026

13. Verify it disappears from Projects.

14. Verify it appears in Work 2026.

15. Move Project Plan:
    Work 2026 → Unfiled

16. Verify it appears in Unfiled.

17. Delete Work 2026.

18. Verify:
    notebook deleted
    note still exists
    note remains unfiled

19. Create:
    Personal

20. Create:
    Personal → Travel

21. Create a note inside Travel.

22. Restart application.

23. Verify all notebook hierarchy and notes.

24. Test dark mode.

25. Test light mode.

26. Test system mode.

27. Test keyboard shortcuts.

28. Test Unicode notebook names.

29. Test large note content.

30. Run all available tests and builds.
```

---

# 30. FINAL PHASE ARCHITECTURE

After Phase 4, the application should conceptually work like this:

```text
                  PERSONAL NOTEPAD
                         │
                         ▼
                 Tauri Desktop App
                         │
             ┌───────────┴───────────┐
             ▼                       ▼
        React UI                  Rust Core
             │                       │
      ┌──────┴──────┐          ┌─────┴─────┐
      ▼             ▼          ▼           ▼
 Notebook Tree   Note List   Notebook     Notes
      │             │        Service      Service
      └─────────────┴────────────┬──────────┘
                                 ▼
                              SQLite
                                 │
             ┌───────────────────┼──────────────────┐
             ▼                   ▼                  ▼
         Notebooks             Notes             Settings
             │                   │
             │                   │
             └────────────┬──────┘
                          ▼
                    notebook_id
```

---

# 31. FINAL USER FLOW

After Phase 4:

```text
Open Personal Notepad
        ↓
See sidebar
        ↓
See All Notes
        ↓
See Unfiled
        ↓
See Notebook tree
        ↓
Create Notebook
        ↓
Create Child Notebook
        ↓
Select Notebook
        ↓
Create Note
        ↓
Note automatically belongs to selected notebook
        ↓
Edit Note
        ↓
Autosave
        ↓
Move Note
        ↓
Rename Notebook
        ↓
Restart App
        ↓
Everything persists
```

---

# 32. PHASE 4 COMPLETION STATE

At the end of Phase 4:

```text
PHASE 1
Application shell
        +
PHASE 2
SQLite persistence
        +
PHASE 3
Real note creation/editing
        +
PHASE 4
Notebook / folder organization
        =
Functional local note-taking application
```

The user can now organize notes into nested notebooks while keeping all note content locally persisted.

---

# 33. FEATURES STILL NOT IMPLEMENTED

The following must remain outside Phase 4:

```text
Tags
Favorites
Advanced note metadata
Search
Attachments
Trash UI
Recovery UI
Permanent deletion
Internal links
Backlinks
Import
Export
Backup
Encryption
Sync
Windows support
Ubuntu packaging
AI
```

Do not implement them as "small extras".

They belong to later phases.

---

# 34. AI CODING AGENT RULES

While implementing Phase 4, follow these rules strictly.

### Rule A

Inspect existing code before modifying it.

### Rule B

Complete one task at a time.

### Rule C

After each task:

```text
typecheck
```

or the smallest relevant verification should be performed.

### Rule D

Do not rewrite working Phase 1–3 systems.

### Rule E

Do not create duplicate storage architecture.

### Rule F

Do not introduce unnecessary dependencies.

### Rule G

Do not add features from later phases.

### Rule H

Do not use internet/network services.

### Rule I

Do not add AI features.

### Rule J

Do not claim a task is complete without verification.

### Rule K

If an existing Phase 2/3 implementation differs from this document:

1. inspect it
2. preserve the established architecture
3. adapt the Phase 4 implementation
4. do not blindly replace working code

### Rule L

If a design decision is ambiguous:

Prefer:

```text
simplest
local
offline
testable
maintainable
reversible
```

implementation.

---

# 35. REQUIRED PER-TASK REPORT FORMAT

After every task, report exactly in this structure:

```text
TASK COMPLETED:
Task X — <task name>

CHANGES:
- ...

FILES CREATED:
- ...
or
- None

FILES MODIFIED:
- ...
or
- None

DEPENDENCIES ADDED:
- None
or
- ...

DATABASE CHANGES:
- ...
or
- None

VERIFICATION:
- TypeScript: PASS/FAIL
- Rust: PASS/FAIL
- Database migration: PASS/FAIL/N/A
- Tests: PASS/FAIL/N/A
- Build: PASS/FAIL
- Application launch: PASS/FAIL

NOT IMPLEMENTED YET:
- ...

NOTES FOR NEXT TASK:
- ...
```

Never report:

```text
PASS
```

unless the relevant check actually ran successfully.

---

# 36. FINAL PHASE REPORT

After all Phase 4 tasks are complete, provide:

```text
PHASE 4 COMPLETE

IMPLEMENTED:
- Notebook CRUD
- Nested notebooks
- Notebook tree
- Notebook selection
- All Notes
- Unfiled
- Note filtering
- Note movement
- New note inside selected notebook
- Safe notebook deletion
- Notebook persistence
- Restart recovery

DATABASE:
- SQLite-backed
- Migration-safe
- Notes preserved when notebooks are deleted

EDITOR:
- Phase 3 behavior preserved

UI:
- Three-pane layout preserved
- Theme system preserved
- Sidebar extended with notebook tree

TESTING:
- TypeScript: PASS/FAIL
- Rust: PASS/FAIL
- Database tests: PASS/FAIL
- Application build: PASS/FAIL
- Application launch: PASS/FAIL
- Manual QA: PASS/FAIL

NOT IMPLEMENTED:
- Tags
- Favorites
- Search
- Attachments
- Trash UI
- Internal links
- Import/export
- Backup
- Encryption
- Sync
- Windows
- Packaging
- AI

READY FOR:
PHASE 5 — Tags, Favorites & Note Metadata
```

---

# 37. PHASE 4 SUCCESS DEFINITION

Phase 4 is successful when this statement is true:

> **A user can create a nested notebook hierarchy, organize notes into those notebooks, move notes between notebooks or back to Unfiled, rename and safely delete notebooks, and close/reopen Personal Notepad without losing notebook hierarchy, note assignments, or note content.**

Nothing beyond that definition is required for Phase 4.
