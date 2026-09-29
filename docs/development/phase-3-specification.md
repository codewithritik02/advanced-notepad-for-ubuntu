# PERSONAL NOTEPAD

# PHASE 3 — NOTE CREATION & EDITING

---

# 1. PHASE OBJECTIVE

Implement the real **note creation and editing experience** for **Personal Notepad**.

Phase 1 established the desktop application shell and UI architecture.

Phase 2 established:

* SQLite database
* database schema
* migrations
* storage layer
* Rust repositories
* Tauri storage commands
* frontend storage service
* persistent notes foundation
* notebooks foundation
* tags foundation
* settings foundation
* application data directories

Phase 3 now turns the previously non-functional editor placeholder into a real local note editor.

The primary objectives are:

1. Create real notes.
2. Open existing notes.
3. Edit note titles.
4. Edit note content.
5. Persist changes to SQLite.
6. Support TXT as the primary note format.
7. Support Markdown as a secondary format.
8. Implement autosave.
9. Save changes safely when closing/switching notes.
10. Implement undo/redo behavior.
11. Preserve cursor/selection where practical.
12. Handle empty notes correctly.
13. Handle large text correctly.
14. Add appropriate keyboard shortcuts.
15. Keep editor logic independent from SQLite implementation details.
16. Maintain Phase 1 UI architecture.
17. Maintain Phase 2 storage architecture.
18. Verify exact content survives application restart.

This phase is about **real note creation and editing**.

---

# 2. PHASE 3 SCOPE

Phase 3 includes:

```text
Real note creation
Real note selection
Real note opening
Real note title editing
Real note content editing
TXT format
Markdown format
Format awareness
Autosave
Explicit save where appropriate
Save on note switch
Save before application close where possible
Dirty/unsaved state
Undo
Redo
Cursor preservation
Selection preservation where practical
Keyboard shortcuts
Empty note handling
Large text handling
Note timestamps
Basic note metadata display
Persistent note loading
Persistent note updating
Basic note deletion foundation where already supported
Editor loading state
Editor error state
Save error state
Storage integration
Editor testing
Persistence testing
```

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
```

The application must work without internet access.

Do NOT introduce:

* AI
* AI writing
* AI autocomplete
* AI suggestions
* AI summarization
* AI chatbot
* LLMs
* OpenAI APIs
* cloud APIs
* online accounts
* mandatory internet
* cloud synchronization

The note editor must work entirely with local data.

---

# 4. PHASE 2 MUST NOT BE BROKEN

Phase 3 must use the storage architecture created in Phase 2.

Do NOT replace SQLite with:

```text
localStorage
IndexedDB
browser files
JSON files
in-memory storage
```

for production note persistence.

SQLite remains the source of truth.

The architecture remains:

```text
React Editor
      ↓
Note Feature / Editor Service
      ↓
Frontend Storage Service
      ↓
Tauri IPC
      ↓
Rust Note Repository
      ↓
SQLite
```

The editor must NOT:

* execute SQL
* open SQLite directly
* know the database path
* manipulate database files
* bypass the storage service

---

# 5. ARCHITECTURAL PRINCIPLE

The editor should be separated into logical responsibilities.

Conceptually:

```text
Note Feature
│
├── Note List
│
├── Note Editor
│
├── Editor State
│
├── Note Persistence
│
└── Note Formatting
```

The editor should maintain temporary editing state separately from persisted database state.

For example:

```text
Persisted Note
      │
      ▼
Editor State
      │
      ├── title
      ├── content
      ├── format
      ├── cursor
      ├── selection
      └── dirty state
      │
      ▼
Save
      │
      ▼
SQLite
```

Do not write to SQLite on every keystroke unless there is a clear performance-safe debounce mechanism.

---

# 6. NOTE DATA MODEL

Use the Phase 2 `notes` schema.

Conceptually:

```text
Note
├── id
├── title
├── content
├── format
├── notebook_id
├── created_at
├── modified_at
├── is_favorite
├── is_pinned
├── is_deleted
└── deleted_at
```

Do not redesign the database schema unless an actual technical requirement is discovered.

If a schema modification is genuinely required, create a proper migration.

Never manually modify an existing user's database.

---

# 7. NOTE FORMAT RULE

The primary note format is:

```text
txt
```

The secondary format is:

```text
md
```

TXT is the default format for newly created notes unless the product's existing UI explicitly chooses otherwise.

The editor must know which format a note uses.

Example:

```text
format = "txt"
```

or:

```text
format = "md"
```

---

# 8. TXT FORMAT

TXT is plain text.

The editor must preserve the user's text exactly as much as practical.

Do not automatically:

* convert text to HTML
* inject Markdown
* modify punctuation
* normalize user content unnecessarily
* add formatting markers
* strip meaningful whitespace

Example:

```text
Hello world.

This is my note.

Line three.
```

must remain equivalent after saving and reopening.

Unicode must work correctly.

Example:

```text
आज की मीटिंग के नोट्स 📝
```

must be preserved.

---

# 9. MARKDOWN FORMAT

Markdown is a secondary structured text format.

Phase 3 does NOT require a sophisticated WYSIWYG Markdown editor.

The initial implementation may use a plain-text editor that is format-aware.

For example:

```text
# Project Plan

## Tasks

- Task one
- Task two
- Task three
```

should be stored as Markdown content without unwanted conversion.

Do NOT introduce a full Markdown rendering engine unless it is genuinely needed for the current UX.

Markdown preview/editing can be expanded later if required.

---

# 10. TASK BREAKDOWN

Phase 3 must be completed in small independent tasks.

Do NOT implement all tasks at once.

Complete:

```text
TASK 1
Phase 2 inspection and editor architecture

TASK 2
Note domain model and frontend types

TASK 3
Persistent note loading

TASK 4
Real note selection and opening

TASK 5
New note creation

TASK 6
Title editing

TASK 7
Content editing

TASK 8
TXT format handling

TASK 9
Markdown format handling

TASK 10
Dirty state and save lifecycle

TASK 11
Autosave

TASK 12
Save on note switch and application close

TASK 13
Undo and redo

TASK 14
Cursor and selection preservation

TASK 15
Keyboard shortcuts

TASK 16
Empty note handling

TASK 17
Large text handling

TASK 18
Editor metadata/status UI

TASK 19
Loading and error states

TASK 20
Editor/storage integration cleanup

TASK 21
Automated testing

TASK 22
Restart and persistence verification

TASK 23
Documentation and final Phase 3 verification
```

Each task must be completed and verified separately.

---

# 11. TASK 1 — PHASE 2 INSPECTION & EDITOR ARCHITECTURE

## Objective

Understand the Phase 2 implementation before modifying the editor.

Inspect:

```text
src/
src-tauri/
services/
features/
types/
components/
layouts/
styles/
tests/
README.md
storage architecture
note repository
Tauri commands
```

Identify:

* existing Note type
* existing note repository
* existing note commands
* existing storage service
* existing mock note data
* existing notes list
* existing editor placeholder
* existing application state
* existing keyboard handling
* existing theme handling

Do NOT recreate storage functionality.

Do NOT introduce a second note repository.

Do NOT create duplicate Note models unless there is a genuine boundary requiring one.

## Acceptance criteria

The agent understands exactly how the editor will communicate with the existing Phase 2 storage layer.

---

# 12. TASK 2 — NOTE DOMAIN MODEL & FRONTEND TYPES

## Objective

Establish a clean TypeScript representation of notes.

At minimum:

```text
Note
NoteFormat
CreateNoteInput
UpdateNoteInput
```

Conceptually:

```text
type NoteFormat = "txt" | "md";
```

The Note type should represent the persisted entity.

Example conceptual structure:

```text
Note {
    id
    title
    content
    format
    notebookId
    createdAt
    modifiedAt
    isFavorite
    isPinned
    isDeleted
    deletedAt
}
```

Use the naming convention already established by Phase 2.

Do not create conflicting naming conventions.

Avoid `any`.

---

# 13. NOTE EDITOR STATE

Create a dedicated editor state model.

Conceptually:

```text
EditorState
├── noteId
├── title
├── content
├── format
├── isDirty
├── isSaving
├── lastSavedAt
├── error
├── cursorPosition
└── selection
```

The exact implementation may use React state, context, hooks, or another existing project pattern.

Do not introduce a large state-management framework without necessity.

---

# 14. TASK 3 — PERSISTENT NOTE LOADING

## Objective

Load real notes from SQLite.

Replace production use of Phase 1 mock note data.

The flow must be:

```text
Application
   ↓
Storage initialized
   ↓
List notes
   ↓
SQLite
   ↓
Note list
```

The notes list must display persisted notes.

Do not hardcode notes such as:

```text
Welcome to Personal Notepad
Project Ideas
Shopping List
```

as production data.

Mock data may remain in isolated tests only.

---

# 15. NOTE LIST LOADING

The notes list should display at minimum:

```text
title
preview
modified time/date
favorite state where applicable
```

The exact presentation can remain consistent with Phase 1.

The list must update when notes change.

Avoid unnecessarily refetching the entire database after every small editor state update.

Use a reasonable refresh/update strategy.

---

# 16. TASK 4 — REAL NOTE SELECTION & OPENING

## Objective

Allow the user to select a note from the notes list and open it in the editor.

Flow:

```text
User clicks note
      ↓
Note ID selected
      ↓
Storage service
      ↓
SQLite
      ↓
Note loaded
      ↓
Editor populated
```

When a note opens:

* title must match database
* content must match database
* format must match database
* metadata must match database

No mock content should overwrite persisted content.

---

# 17. EDITOR LOADING STATE

When opening a note:

Display an appropriate loading state.

Example:

```text
Loading note...
```

Do not display stale content as though it belongs to the newly selected note.

Where practical:

```text
Old note
   ↓
Selection changed
   ↓
Loading
   ↓
New note
```

---

# 18. TASK 5 — NEW NOTE CREATION

## Objective

Implement real note creation.

The existing Phase 1:

```text
+ New Note
```

action must now create a real SQLite-backed note.

Flow:

```text
New Note
   ↓
Create note request
   ↓
Rust
   ↓
SQLite INSERT
   ↓
New Note returned
   ↓
Editor opens new note
```

Default values:

```text
title = "Untitled Note"
content = ""
format = "txt"
is_favorite = false
is_pinned = false
is_deleted = false
```

Use the project's established ID/timestamp strategy.

---

# 19. NEW NOTE UX

After creating a note:

* it should appear in the note list
* it should become the active note
* editor should open it
* cursor should be positioned appropriately
* title/content should be editable

Do not create duplicate notes because of repeated rendering.

Do not create notes merely because a React component mounted.

Note creation must be triggered by an intentional user action or explicit application command.

---

# 20. EMPTY NOTE BEHAVIOR

Creating a new note must NOT require fake content.

An empty note is valid.

For example:

```text
title = "Untitled Note"
content = ""
```

is acceptable.

Do not prevent saving merely because content is empty.

Future product rules may change this, but Phase 3 should support empty notes.

---

# 21. TASK 6 — TITLE EDITING

## Objective

Make the note title editable.

The editor should provide a clear title input.

Example:

```text
┌─────────────────────────────┐
│ My Meeting Notes            │
├─────────────────────────────┤
│ Note content...             │
```

Changes to title must eventually persist to SQLite.

Do not write on every keystroke without debounce.

Title changes should update:

* editor state
* persisted note
* notes list title
* modified timestamp

after save.

---

# 22. TITLE VALIDATION

Titles should support:

* Unicode
* spaces
* punctuation
* numbers
* normal symbols

Do not impose unnecessary restrictions.

An empty title may be temporarily allowed during editing.

Before persistence, use a sensible fallback such as:

```text
Untitled Note
```

if the product behavior requires a non-empty title.

Do not use filenames as the title.

---

# 23. TASK 7 — CONTENT EDITING

## Objective

Replace the Phase 1 editor placeholder with a real editable text area/editor.

The editor must support:

* typing
* deleting
* line breaks
* copy
* cut
* paste
* select all
* cursor movement
* selection
* normal keyboard text editing

The initial implementation should remain lightweight.

Do not introduce a heavy editor framework unless there is a genuine requirement.

A native `<textarea>` or lightweight text-editing implementation is acceptable for the initial version.

---

# 24. CONTENT PERSISTENCE

Content changes must eventually reach SQLite.

The flow:

```text
User types
   ↓
Editor state changes
   ↓
Dirty state
   ↓
Autosave debounce / explicit save
   ↓
Storage service
   ↓
Tauri
   ↓
Rust
   ↓
SQLite
```

Do not block the UI on every keystroke.

Do not freeze the editor during saves.

---

# 25. TASK 8 — TXT FORMAT HANDLING

## Objective

Ensure TXT notes behave as plain text.

TXT notes must:

* preserve line breaks
* preserve Unicode
* preserve normal whitespace
* preserve user content
* remain readable after restart

Do not interpret Markdown syntax in TXT mode.

For example:

```text
# Heading
```

in TXT mode is simply text.

---

# 26. TASK 9 — MARKDOWN FORMAT HANDLING

## Objective

Support Markdown as a note format.

Markdown content must be stored exactly as Markdown text.

Example:

```text
# Meeting Notes

- Discuss project
- Review timeline
- Assign tasks
```

must remain valid Markdown after save/reopen.

Do not silently convert Markdown into HTML.

Do not introduce automatic formatting unless explicitly required.

---

# 27. FORMAT INDICATOR

The editor should expose the note format in an appropriate subtle UI location.

For example:

```text
TXT
```

or:

```text
MD
```

The exact design can follow the existing Phase 1 style.

Do not clutter the editor.

---

# 28. FORMAT CHANGING

Phase 3 may provide a format selector if the existing product architecture supports it.

If implemented:

```text
TXT ↔ Markdown
```

must not silently corrupt content.

Changing format should be deliberate.

The application should treat the operation as a format metadata change unless conversion is explicitly implemented.

Do NOT implement complex content conversion in Phase 3.

If there is no strong UX requirement yet, it is acceptable to expose the format as read-only metadata and defer format switching UX.

---

# 29. TASK 10 — DIRTY STATE

## Objective

Track whether the editor has unsaved changes.

Conceptually:

```text
isDirty = false
```

after successful load/save.

When the user changes:

```text
title
content
format
```

set:

```text
isDirty = true
```

After successful persistence:

```text
isDirty = false
```

---

# 30. SAVE STATUS

The UI should communicate save state without being distracting.

Possible states:

```text
Saved
Saving...
Unsaved changes
Save failed
```

Use the existing status-bar design where appropriate.

Do not display noisy notifications for every successful autosave.

---

# 31. TASK 11 — AUTOSAVE

## Objective

Implement reliable autosave.

Autosave is part of Phase 3.

It must NOT write to SQLite on every individual keystroke.

Use a debounce strategy.

Conceptually:

```text
User types
   ↓
Dirty
   ↓
Wait short debounce period
   ↓
No further changes
   ↓
Save
```

The exact debounce duration should be selected based on usability and performance.

A reasonable initial range is approximately:

```text
500ms – 1500ms
```

Do not treat this range as mandatory.

---

# 32. AUTOSAVE RULES

Autosave must:

* avoid excessive database writes
* avoid race conditions
* save the latest content
* update modified timestamp
* clear dirty state after success
* expose an error after failure

If the user continues typing while a save is in progress, the application must not accidentally overwrite newer content with older content.

This is critical.

---

# 33. AUTOSAVE RACE CONDITION

Example:

```text
Content A
   ↓
Save request A

User continues typing

Content B
   ↓
Save request B
```

The system must ensure:

```text
B
```

cannot be accidentally replaced by:

```text
A
```

after B was created.

Use an appropriate request/version/debounce strategy.

---

# 34. SAVE ERROR

If autosave fails:

The application must:

```text
keep editor content in memory
keep isDirty = true
show save failure state
```

Do NOT discard user input.

Example:

```text
Unable to save changes.

Your changes are still in the editor.
```

The application may retry according to a sensible strategy.

Do not create an infinite aggressive retry loop.

---

# 35. TASK 12 — SAVE ON NOTE SWITCH

## Objective

Prevent accidental loss when switching between notes.

When:

```text
Current note is dirty
```

and the user selects another note:

The application should ensure current changes are persisted before switching.

Preferred flow:

```text
Current note dirty
      ↓
Save
      ↓
Save successful
      ↓
Switch note
```

If saving fails:

```text
Save failed
      ↓
Do not silently discard changes
```

The UI should provide an appropriate error/retry path.

Do not simply switch away and lose the user's changes.

---

# 36. SAVE ON APPLICATION CLOSE

## Objective

Attempt to preserve unsaved changes when the application is closing.

Where Tauri lifecycle APIs permit:

```text
Application close requested
       ↓
Check dirty state
       ↓
Save pending changes
       ↓
Allow close
```

Do not assume an asynchronous save will always finish after the application has already terminated.

Use the appropriate Tauri lifecycle mechanism.

If reliable close interception is limited by the current framework, document the limitation rather than claiming guaranteed behavior.

---

# 37. TASK 13 — UNDO / REDO

## Objective

Provide normal text-editor undo/redo behavior.

At minimum support:

```text
Ctrl/Cmd + Z
```

Undo

and:

```text
Ctrl/Cmd + Shift + Z
```

or the platform-appropriate redo shortcut.

If native text input behavior already provides correct undo/redo, prefer using it rather than implementing a custom history engine.

Do not unnecessarily build a custom undo stack.

---

# 38. UNDO / REDO RULES

Undo/redo should:

* work naturally while editing
* not accidentally trigger application-level navigation
* not corrupt persisted content
* preserve normal text-editor expectations

Autosave must not destroy undo/redo history.

Saving a note is a persistence operation, not an editing operation.

---

# 39. TASK 14 — CURSOR & SELECTION PRESERVATION

## Objective

Preserve cursor position and selection when appropriate.

Important scenario:

```text
User editing note
   ↓
Autosave
   ↓
UI remains stable
```

The cursor must NOT jump to the beginning/end unexpectedly.

Do not re-render the entire editor in a way that resets:

* cursor position
* selection
* scroll position

after every save.

---

# 40. NOTE SWITCH CURSOR BEHAVIOR

When opening a different note:

The new note may initialize its cursor at:

```text
start
```

or another sensible location.

When returning to a previously edited note during the same session, preserving cursor position is preferred where practical.

Do not create a complicated persistence system for cursor position yet.

---

# 41. TASK 15 — KEYBOARD SHORTCUTS

Implement safe note-editor shortcuts.

At minimum:

```text
Ctrl/Cmd + N
```

New Note

```text
Ctrl/Cmd + F
```

Focus Search

Normal editing shortcuts must remain functional.

Do not hijack:

```text
Ctrl/Cmd + C
Ctrl/Cmd + V
Ctrl/Cmd + X
Ctrl/Cmd + A
Ctrl/Cmd + Z
```

unless absolutely necessary.

---

# 42. SAVE SHORTCUT

Implement:

```text
Ctrl/Cmd + S
```

for explicit save.

When triggered:

```text
save current note
```

If there are no changes:

```text
do nothing
```

or provide an unobtrusive confirmation.

Do not produce unnecessary UI notifications.

---

# 43. KEYBOARD EVENT ARCHITECTURE

Keyboard handling should remain centralized where practical.

Do not attach duplicate global listeners to every component.

Avoid memory leaks.

Ensure listeners are cleaned up appropriately.

Do not prevent default browser/Tauri behavior unless the shortcut is intentionally handled by the application.

---

# 44. TASK 16 — EMPTY NOTE HANDLING

Empty notes are valid.

The application must handle:

```text
title = Untitled Note
content = ""
```

without errors.

The editor should remain usable.

The notes list should display a sensible preview such as:

```text
No content
```

or another subtle empty preview.

Do not insert fake content into empty notes.

---

# 45. WHITESPACE-ONLY NOTES

A note containing only spaces/newlines should not crash or become corrupted.

Example:

```text
     
   
```

The content must be preserved if the user intentionally entered it.

Do not automatically trim the entire note content during save.

Title normalization may be handled separately.

---

# 46. TASK 17 — LARGE TEXT HANDLING

## Objective

Ensure the editor can handle large notes reasonably.

Test with:

```text
10 KB
100 KB
500 KB
1 MB
```

or larger where practical.

The application should:

* remain responsive
* save correctly
* reopen correctly
* preserve content
* not duplicate content unnecessarily in memory

Phase 3 does NOT require advanced virtualized editors.

If very large notes expose performance problems, document them and fix only issues relevant to the current implementation.

---

# 47. LARGE TEXT SAFETY

Do not impose an arbitrary tiny maximum such as:

```text
10 KB
50 KB
```

unless there is a genuine technical reason.

SQLite TEXT can store substantially larger content.

The editor should avoid accidental truncation.

---

# 48. TASK 18 — EDITOR METADATA / STATUS UI

## Objective

Display useful note metadata without clutter.

Possible information:

```text
Saved
Last saved: 9:42 PM
TXT
Words: 125
Characters: 742
```

Word/character counts may be included if lightweight.

The final metadata feature can be expanded later.

At minimum the editor should communicate:

```text
format
save status
```

and optionally:

```text
modified time
```

---

# 49. MODIFIED TIMESTAMP

When note content/title changes are successfully persisted:

```text
modified_at
```

must update.

Do not update `modified_at` merely because the note was opened.

Opening a note is not a modification.

---

# 50. CREATED TIMESTAMP

`created_at` must remain stable.

Editing a note must NOT change:

```text
created_at
```

Only `modified_at` changes.

---

# 51. TASK 19 — LOADING & ERROR STATES

The editor must support:

```text
Loading
Loaded
Saving
Saved
Unsaved
Error
```

Examples:

### Loading

```text
Loading note...
```

### Saving

```text
Saving...
```

### Saved

```text
Saved
```

### Unsaved

```text
Unsaved changes
```

### Error

```text
Unable to save changes.
Your changes are still available locally in the editor.
```

Do not expose raw Rust/SQLite errors directly to normal users.

---

# 52. NOTE NOT FOUND

If the selected note no longer exists:

Display:

```text
Note not found

The selected note is no longer available.
```

Do not crash.

Do not leave the editor showing another note while claiming it is the missing note.

Return the UI to a valid state.

---

# 53. STORAGE ERROR DURING LOAD

If loading fails:

```text
Unable to load note.
```

Keep the application shell functional where possible.

Do not destroy local state.

---

# 54. STORAGE ERROR DURING SAVE

If saving fails:

* keep current editor state
* keep dirty state
* show error
* do not discard content
* allow retry

This is especially important for autosave.

---

# 55. TASK 20 — EDITOR / STORAGE INTEGRATION CLEANUP

Review the complete flow:

```text
New Note
   ↓
SQLite
   ↓
Note List
   ↓
Open Note
   ↓
Edit
   ↓
Dirty
   ↓
Autosave
   ↓
SQLite
   ↓
Updated Note List
```

Remove any remaining production dependency on mock data.

Ensure there is one source of truth.

SQLite remains the persisted source of truth.

The editor's in-memory state is temporary editing state.

---

# 56. NOTE LIST UPDATE STRATEGY

When a note changes:

The notes list should reflect:

* new title
* modified timestamp
* preview
* favorite state if changed

Avoid unnecessarily reloading everything after every character.

A reasonable approach is:

```text
Editor save succeeds
       ↓
Notify note feature
       ↓
Update affected note in list
```

or another efficient mechanism consistent with the existing architecture.

---

# 57. ACTIVE NOTE STATE

There must be one clear active note ID.

Conceptually:

```text
activeNoteId
```

When no note is selected:

```text
activeNoteId = null
```

The UI should show an appropriate empty editor state.

Do not accidentally open multiple editors for the same note.

---

# 58. NEW NOTE DUPLICATION PREVENTION

Avoid duplicate creation caused by:

* React Strict Mode
* component re-rendering
* effect dependencies
* keyboard listeners
* double-clicks
* repeated event handlers

A new note must only be created when the action is intentionally triggered.

---

# 59. AUTOSAVE DUPLICATION PREVENTION

Ensure a single editor change does not cause multiple identical saves due to:

* duplicated effects
* duplicate listeners
* stale state
* repeated timers

Cancel/replace previous debounce timers where appropriate.

---

# 60. STALE SAVE PREVENTION

Consider:

```text
Save request #1
Save request #2
```

where #2 contains newer content.

The final persisted state must correspond to the newest editor state.

Use one of:

```text
debounce
request version
save queue
serialization
revision number
```

or another appropriate mechanism.

Do not over-engineer.

The important requirement is:

> Older save operations must not overwrite newer changes.

---

# 61. NOTE CONTENT EXACTNESS

The following content:

```text
Line 1
Line 2

Line 4

Hindi: नमस्ते
Emoji: 📝
Symbols: !@#$%^&*()
```

must remain equivalent after:

```text
save
close
restart
reopen
```

Do not accidentally:

* remove line breaks
* convert encoding
* strip Unicode
* collapse whitespace
* modify characters

---

# 62. CLIPBOARD SUPPORT

The editor must support standard desktop clipboard operations:

```text
Copy
Cut
Paste
Select All
```

Prefer native browser/Tauri text input behavior.

Do not build a custom clipboard implementation.

---

# 63. EDITOR SCROLLING

The editor must support vertical scrolling for long notes.

Avoid:

```text
entire application scrolling
```

when the editor itself should scroll.

The three-pane layout should remain stable.

---

# 64. TITLE / CONTENT SCROLLING

Title should remain visually accessible.

For long content:

```text
editor content scrolls
```

without causing:

```text
sidebar
notes list
top bar
```

to move unexpectedly.

---

# 65. PHASE 1 THEME COMPATIBILITY

The editor must support:

```text
Light
Dark
System
```

using the existing theme token architecture.

Do not hardcode separate dark-mode values directly inside the editor.

The editor must remain readable in every supported theme.

---

# 66. ACCESSIBILITY

The editor must have:

* accessible title input
* accessible content editor
* visible focus state
* keyboard accessibility
* sensible labels
* buttons with accessible names
* no color-only indication of save state

For example, do not communicate only:

```text
green = saved
red = error
```

Text/icon/state should also communicate meaning.

---

# 67. SECURITY CONSIDERATIONS

Phase 3 is not the encryption phase.

Do NOT implement encryption here.

However:

* never execute note content as code
* never inject raw note content into HTML unsafely
* never treat Markdown content as trusted HTML
* never send note content to external services
* never log full note content unnecessarily
* use existing storage validation
* use parameterized SQL through Phase 2 storage layer

The application is offline-first.

---

# 68. NOTE CONTENT AND XSS

Even though the application is local:

Do not dangerously inject user note content into the DOM.

Avoid unnecessary:

```text
dangerouslySetInnerHTML
```

for note editing.

A plain text editor should treat note content as text.

If Markdown preview is added later, rendering must be handled safely.

---

# 69. TASK 21 — AUTOMATED TESTING

Testing is mandatory.

Create or extend tests for:

## Note creation

```text
Create note
→ note exists
```

## Note loading

```text
Create note
→ load note
→ exact data returned
```

## Title update

```text
Create
→ change title
→ save
→ retrieve
→ title matches
```

## Content update

```text
Create
→ change content
→ save
→ retrieve
→ content matches
```

## Empty content

```text
content = ""
```

must work.

## Unicode

Test:

```text
आज की मीटिंग 📝
```

## Special characters

Test:

```text
!@#$%^&*()
```

## Newlines

Test multi-line content.

## Markdown

Test Markdown content preservation.

## TXT

Test plain text preservation.

## Modified timestamp

Verify:

```text
modified_at
```

changes after a successful modification.

## Created timestamp

Verify:

```text
created_at
```

does not change during editing.

---

# 70. EDITOR STATE TESTS

Test:

```text
initial load → not dirty
edit → dirty
successful save → not dirty
save failure → dirty
```

Test that unsaved editor content is not lost after a failed save.

---

# 71. AUTOSAVE TESTS

Test:

```text
change content
wait for debounce
verify save occurred
```

Also test:

```text
rapid changes
```

and ensure unnecessary duplicate saves are not generated.

---

# 72. STALE SAVE TEST

Simulate:

```text
old content save
new content save
```

Verify that the final database content is:

```text
new content
```

not old content.

---

# 73. NOTE SWITCH TEST

Test:

```text
Open Note A
Edit Note A
Switch to Note B
Reopen Note A
```

Expected:

```text
Note A changes are preserved.
```

---

# 74. RESTART PERSISTENCE TEST

Test:

```text
Create note
Edit title
Edit content
Save
Close app
Restart app
Open note
```

Expected:

```text
title exact
content exact
format exact
created_at preserved
modified_at valid
```

---

# 75. LARGE TEXT TEST

Test at least:

```text
100 KB
500 KB
1 MB
```

where practical.

Verify:

* editor remains functional
* save succeeds
* reload succeeds
* exact content is preserved

---

# 76. TASK 22 — RESTART & PERSISTENCE VERIFICATION

This is the most important manual verification of Phase 3.

Perform the following.

## Test 1 — Create

```text
Click New Note
```

Verify a real database-backed note is created.

## Test 2 — Title

Enter:

```text
Phase 3 Test Note
```

## Test 3 — Content

Enter:

```text
This is a persistence test.

Line two.

आज की मीटिंग 📝
```

## Test 4 — Save

Use:

```text
Ctrl/Cmd + S
```

Verify save status.

## Test 5 — Close

Close the application completely.

## Test 6 — Restart

Launch Personal Notepad again.

## Test 7 — Open

Open the note.

## Test 8 — Verify

Verify:

```text
title exact
content exact
format correct
```

No data must be lost.

---

# 77. MULTIPLE NOTE VERIFICATION

Create:

```text
Note A
Note B
Note C
```

Give each unique content.

Restart.

Verify all notes remain distinct.

No note may overwrite another.

---

# 78. NOTE ORDER

The notes list should use the existing application's intended ordering strategy.

For the current phase, a sensible default is:

```text
modified_at DESC
```

unless Phase 1 already established another ordering.

Do not implement advanced sorting/filtering.

That belongs to later phases.

---

# 79. NO SEARCH IMPLEMENTATION

Do NOT implement database search.

The Phase 1 search field may remain a placeholder or non-searching UI if Phase 6 has not arrived.

Do not connect editor work to full-text search.

---

# 80. NO NOTEBOOK MANAGEMENT

Phase 3 may preserve the `notebook_id` field.

Do NOT implement:

* notebook tree management
* drag/drop
* moving notes
* notebook deletion
* notebook hierarchy editing

Those belong to Phase 4.

---

# 81. NO TAG MANAGEMENT

Do NOT implement:

* tag editor
* tag filtering
* tag management
* tag UI

Those belong to Phase 5.

The existing database relationship may remain untouched.

---

# 82. NO TRASH FEATURE

Do NOT implement:

* Trash UI
* restore
* empty trash
* permanent delete workflows

Those belong to Phase 8.

If note deletion is required for an existing API, preserve the Phase 2 contract and do not expand it into the complete Trash system.

---

# 83. NO ATTACHMENTS

Do NOT implement:

* file picker
* attachment upload
* attachment preview
* attachment open
* attachment remove
* attachment rename

Those belong to Phase 7.

---

# 84. NO IMPORT / EXPORT

Do NOT implement:

```text
TXT export
Markdown export
HTML export
PDF
Print
Joplin import
```

Those belong to Phase 10.

---

# 85. NO BACKUP SYSTEM

Do NOT implement:

```text
automatic backups
backup rotation
restore
backup scheduling
```

Those belong to Phase 11.

---

# 86. NO ENCRYPTION

Do NOT implement:

```text
database encryption
key management
encrypted attachments
password protection
```

Those belong to Phase 12.

The architecture should remain compatible with future security work.

---

# 87. NO SYNC

Do NOT implement:

```text
cloud sync
accounts
server
device synchronization
conflict resolution
```

Those belong to Phase 17.

---

# 88. NO WINDOWS IMPLEMENTATION

Do not implement Windows-specific functionality.

The code must remain cross-platform in architecture, but Windows support belongs to Phase 16.

Do not create fake Windows code merely to satisfy architectural diagrams.

---

# 89. NO .DEB PACKAGING

Do not implement Ubuntu release packaging in Phase 3.

`.deb` belongs to Phase 15.

---

# 90. NO AI

Absolutely do not add:

```text
AI
LLM
chatbot
assistant
autocomplete
AI writing
AI summarization
AI suggestions
OpenAI API
cloud AI
```

---

# 91. PERFORMANCE REQUIREMENTS

The editor must feel responsive during normal typing.

Avoid:

* database writes on every keystroke
* full application rerenders on every keystroke
* reloading all notes on every character
* recreating the editor DOM unnecessarily
* excessive IPC traffic

Use debounced persistence.

Keep editor state local and responsive.

---

# 92. MEMORY REQUIREMENTS

Do not maintain unnecessary duplicate copies of the same large note.

For example, avoid simultaneously storing:

```text
full note content
+
multiple full serialized copies
+
multiple full preview copies
```

unless necessary.

For normal note sizes, a straightforward React state model is acceptable.

---

# 93. APPLICATION CLOSE SAFETY

Where supported by Tauri:

* detect close
* check pending save
* complete save
* then close

Do not create a close handler that can deadlock the application.

If a close save cannot be guaranteed in a particular environment, document the limitation and ensure autosave reduces the risk.

---

# 94. ERROR RECOVERY

If save fails:

The editor must remain usable.

The user should be able to:

```text
retry
continue editing
attempt save again
```

Do not reset the editor to the last saved state automatically.

Never silently discard unsaved content.

---

# 95. SAVE CONFLICT ARCHITECTURE

Phase 3 is local-only and does not include multi-device synchronization.

Therefore there is no need to implement distributed conflict resolution.

However, the code should avoid local race conditions between:

```text
editor state
autosave
note switching
application close
```

---

# 96. DATABASE MIGRATION RULE

If Phase 3 discovers that the existing Phase 2 schema genuinely requires a change:

1. Do not manually alter the database.
2. Add a new migration.
3. Make the migration backward/data-safe where possible.
4. Test fresh database creation.
5. Test existing database migration.
6. Verify existing notes are preserved.

Do not create a migration merely because a cleaner schema seems possible.

---

# 97. STORAGE API RULE

Reuse Phase 2 storage APIs.

If an API is insufficient, extend it cleanly.

For example:

```text
notes.create()
notes.get()
notes.list()
notes.update()
```

Do not create a second storage mechanism.

Do not allow components to bypass the frontend storage service.

---

# 98. RUST RULES

Rust remains responsible for:

* SQLite
* persistence
* validation
* database transactions
* storage errors
* Tauri commands

React remains responsible for:

* editor UI
* temporary editor state
* keyboard interaction
* display
* user interaction

Keep this boundary clean.

---

# 99. FRONTEND SERVICE RULES

The frontend storage service should provide a clean interface.

Example conceptual API:

```text
notes.create()
notes.get(id)
notes.list()
notes.update(id, data)
```

The editor should not need to know:

```text
Tauri invoke()
SQLite
SQL
filesystem path
```

Keep those details below the service boundary.

---

# 100. APPLICATION STATE RULES

There should be clear separation between:

```text
Application state
```

and:

```text
Editor state
```

Application state:

```text
active note
sidebar section
theme
```

Editor state:

```text
title
content
format
dirty
saving
cursor
selection
```

Do not mix unrelated state into one giant object.

---

# 101. NOTE IDENTITY

The editor must always associate its current state with a stable:

```text
noteId
```

Do not identify notes by:

```text
array index
title
position
```

Titles can change.

IDs must remain stable.

---

# 102. TITLE CHANGE SAFETY

Changing:

```text
Project Plan
```

to:

```text
Project Plan 2026
```

must not create a new note.

It updates the existing note.

The ID must remain unchanged.

---

# 103. NOTE CONTENT CHANGE SAFETY

Editing content must update the existing note.

Do not create a new database row for every edit.

The database should contain one logical note record.

---

# 104. TIMESTAMP ACCURACY

When saving:

```text
modified_at
```

should reflect the successful modification.

If an autosave fails:

Do not pretend the modification was persisted.

The UI should distinguish:

```text
editor changed
```

from:

```text
database successfully saved
```

---

# 105. SAVE STATUS PRINCIPLE

Never show:

```text
Saved
```

before the storage operation actually succeeds.

Correct:

```text
User edits
↓
Unsaved changes
↓
Save request
↓
SQLite success
↓
Saved
```

Incorrect:

```text
User edits
↓
Saved
```

before persistence.

---

# 106. UI FEEDBACK PRINCIPLE

Save feedback should be subtle.

Do not display a popup every time autosave succeeds.

Preferred:

```text
Saved
Saving...
Unsaved changes
```

in the editor/status area.

Errors may use stronger visual feedback.

---

# 107. ACCESSIBILITY TESTING

Verify:

* keyboard can reach title
* keyboard can reach content editor
* New Note button is accessible
* Save action is accessible
* focus is visible
* shortcuts do not make the editor unusable
* screen-reader labels exist where needed

---

# 108. DOCUMENTATION

Update:

```text
README.md
```

Current status:

```text
Phase 3 — Note Creation & Editing
```

Document that the application now supports:

```text
Real local notes
TXT notes
Markdown notes
Persistent editing
Autosave
Undo/redo
Save status
```

Document that these are still future work:

```text
Search
Attachments
Trash
Import/export
Backup
Encryption
Sync
Windows
.deb
```

---

# 109. EDITOR ARCHITECTURE DOCUMENTATION

Create or update an appropriate document under:

```text
docs/
```

For example:

```text
docs/note-editor.md
```

Document:

```text
Editor UI
   ↓
Editor State
   ↓
Note Service
   ↓
Storage Service
   ↓
Tauri IPC
   ↓
Rust
   ↓
SQLite
```

Explain:

* dirty state
* autosave
* save lifecycle
* note identity
* format handling
* error behavior

---

# 110. TESTING MATRIX

At minimum:

| Test               | Expected                   |
| ------------------ | -------------------------- |
| New Note           | Creates real SQLite note   |
| Open Note          | Loads correct content      |
| Title Edit         | Persists                   |
| Content Edit       | Persists                   |
| Empty Note         | Works                      |
| TXT                | Exact text preserved       |
| Markdown           | Exact Markdown preserved   |
| Unicode            | Preserved                  |
| Special chars      | Preserved                  |
| Newlines           | Preserved                  |
| Autosave           | Works                      |
| Manual Save        | Works                      |
| Save Failure       | Content retained           |
| Note Switch        | Changes preserved          |
| Restart            | Changes preserved          |
| Undo               | Works                      |
| Redo               | Works                      |
| Cursor             | Does not unexpectedly jump |
| Large text         | Remains usable             |
| Created timestamp  | Stable                     |
| Modified timestamp | Updates after save         |

---

# 111. SECURITY TESTING

Verify note content such as:

```text
Robert'); DROP TABLE notes;--
```

is stored as normal text.

Verify:

```text
<script>alert('test')</script>
```

is treated as text and not executed.

Verify Markdown-looking content is not automatically treated as executable HTML.

---

# 112. REGRESSION TESTING

Before completing Phase 3, verify Phase 1 and Phase 2 still work.

### Phase 1

* application launches
* three-pane layout works
* sidebar works
* theme works
* top bar works
* search UI remains present
* responsive desktop layout works

### Phase 2

* SQLite initializes
* migrations work
* database remains accessible
* persistent data remains available
* storage errors are handled

Do not declare Phase 3 complete if Phase 1 or Phase 2 is broken.

---

# 113. ACCEPTANCE CRITERIA

Phase 3 is complete only when all of the following are true.

## Application

* Personal Notepad launches successfully.
* Tauri works.
* React works.
* TypeScript compiles.
* Rust compiles.
* No critical console errors.

## Note Creation

* New Note creates a real SQLite-backed note.
* New note opens in editor.
* New note appears in list.
* New note has a stable ID.
* Empty note is valid.

## Editing

* Title can be edited.
* Content can be edited.
* TXT notes work.
* Markdown notes work.
* Unicode works.
* Newlines work.
* Special characters work.
* Large text works reasonably.

## Persistence

* Changes save to SQLite.
* Autosave works.
* Manual save works.
* Save-on-switch works.
* Application-close save is attempted where supported.
* Changes survive restart.

## Save State

* Dirty state works.
* Saving state works.
* Saved state is accurate.
* Save errors are visible.
* Failed saves do not discard editor content.

## Editing Behavior

* Undo works.
* Redo works.
* Copy works.
* Cut works.
* Paste works.
* Select all works.
* Cursor does not unexpectedly jump.
* Editor remains responsive.

## Data Integrity

* `created_at` remains stable.
* `modified_at` updates after successful modification.
* Note ID remains stable.
* Content is not silently altered.

## Architecture

* React does not access SQLite directly.
* Editor does not know database paths.
* Storage service remains the frontend boundary.
* Rust owns persistence.
* SQLite remains source of truth.
* No duplicate production mock-data source exists.

## Security

* Parameterized SQL remains in use.
* Note content is not executed as HTML/JS.
* No external service receives note content.
* No sensitive note content is unnecessarily logged.

## Regression

* Phase 1 behavior remains functional.
* Phase 2 storage remains functional.

---

# 114. WHAT MUST NOT BE IMPLEMENTED IN PHASE 3

This section is extremely important.

Do NOT implement:

```text
❌ Full-text search
❌ Search indexing
❌ Advanced search filters
❌ Notebook management
❌ Nested notebook UI
❌ Drag/drop notebooks
❌ Tag management
❌ Tag filtering
❌ Full Favorites system
❌ Trash UI
❌ Restore
❌ Permanent delete workflow
❌ Attachment upload
❌ Attachment preview
❌ Attachment management
❌ Internal links
❌ Backlinks
❌ Import
❌ Export
❌ HTML export
❌ PDF export
❌ Print
❌ Backup automation
❌ Backup rotation
❌ Restore backup UI
❌ Database encryption
❌ Password protection
❌ Sync
❌ Accounts
❌ Cloud storage
❌ Windows implementation
❌ Windows installer
❌ .deb packaging
❌ AI
```

Only note creation/editing functionality belongs here.

---

# 115. IMPORTANT AI CODING AGENT RULES

When implementing Phase 3, follow these rules strictly.

## Rule 1 — Inspect before changing

Before modifying any file:

1. Inspect Phase 1.
2. Inspect Phase 2.
3. Understand the existing storage service.
4. Understand the existing Note model.
5. Reuse existing functionality.
6. Avoid unnecessary rewrites.

Never blindly overwrite working files.

---

## Rule 2 — One task at a time

Do NOT implement Task 1 through Task 23 together.

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

Do not unnecessarily redesign:

* three-pane layout
* sidebar
* notes list
* top bar
* theme
* reusable components

Only evolve the editor from placeholder to functional editor.

---

## Rule 4 — Preserve Phase 2

Do not replace:

```text
SQLite
Rust storage
Tauri IPC
frontend storage service
```

with another persistence mechanism.

---

## Rule 5 — SQLite remains source of truth

The editor's React state is temporary.

SQLite is persistent truth.

---

## Rule 6 — No fake persistence

Do not claim notes are saved unless the storage operation actually succeeds.

---

## Rule 7 — No silent data loss

Never discard dirty editor content because:

* note switching
* save failure
* component remount
* application error
* autosave failure

If a save fails, preserve the user's current content.

---

## Rule 8 — No excessive database writes

Do not save on every keystroke.

Use a sensible debounce or save strategy.

---

## Rule 9 — Prevent stale saves

Older asynchronous save requests must not overwrite newer editor content.

---

## Rule 10 — No speculative features

Do not implement later-phase features.

Prepare extension points only where useful.

---

## Rule 11 — Minimal dependencies

Do not add a large editor framework merely because it is convenient.

Use the simplest editor implementation that satisfies Phase 3.

---

## Rule 12 — Offline-first

No network is required for note creation/editing.

---

## Rule 13 — No AI

Do not add any AI package, API, model, assistant, autocomplete or cloud AI.

---

## Rule 14 — No arbitrary HTML rendering

Treat note content as user text.

Do not execute or dangerously inject it.

---

## Rule 15 — Verify every task

After each task:

1. Run appropriate checks.
2. Run tests.
3. Build where appropriate.
4. Launch application where appropriate.
5. Verify previous functionality.
6. Fix errors.
7. Report exactly what happened.

---

# 116. REQUIRED RESPONSE FORMAT FOR THE AI CODING AGENT

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

- None

or

- ...

DEPENDENCIES ADDED:

- None

or

- ...

EDITOR CHANGES:

- ...

STORAGE CHANGES:

- ...

VERIFICATION:

- TypeScript: PASS/FAIL
- Rust: PASS/FAIL
- Tests: PASS/FAIL
- Build: PASS/FAIL
- Application launch: PASS/FAIL
- Note creation: PASS/FAIL
- Note loading: PASS/FAIL
- Note editing: PASS/FAIL
- Persistence: PASS/FAIL
- Autosave: PASS/FAIL

DATA INTEGRITY:

- Content preserved: PASS/FAIL
- Unicode preserved: PASS/FAIL
- Created timestamp preserved: PASS/FAIL
- Modified timestamp updated correctly: PASS/FAIL

SECURITY CHECK:

- Parameterized SQL: PASS/FAIL
- User content treated as text: PASS/FAIL
- No unsafe HTML injection: PASS/FAIL

PHASE 1 PRESERVATION:

- Existing Phase 1 behavior preserved: YES/NO

PHASE 2 PRESERVATION:

- Existing Phase 2 storage preserved: YES/NO

NOT IMPLEMENTED YET:

- ...

NOTES FOR NEXT TASK:

- ...
```

Do not mark something as PASS without actually verifying it.

---

# 117. FINAL PHASE 3 VERIFICATION

Before declaring Phase 3 complete, perform all appropriate checks.

At minimum:

```text
TypeScript type check
Frontend build
Rust cargo check
Rust tests
Frontend tests
Tauri build
Application launch
Create note
Edit title
Edit content
Autosave
Manual save
Switch notes
Restart application
Reopen note
Unicode verification
Markdown verification
TXT verification
Large-text verification
Save failure verification
Undo/redo verification
```

Use the project's actual commands.

Do not invent commands.

---

# 118. FINAL MANUAL TEST SCRIPT

Perform this exact end-to-end scenario.

```text
1. Launch Personal Notepad.

2. Click "+ New Note".

3. Confirm a new note appears.

4. Change title to:

Phase 3 Persistence Test

5. Enter content:

Personal Notepad Phase 3

Line 2

आज की मीटिंग 📝

Markdown:
# Test
- One
- Two

Special:
!@#$%^&*()

6. Wait for autosave.

7. Verify "Saved" state.

8. Close the application.

9. Restart Personal Notepad.

10. Open "Phase 3 Persistence Test".

11. Verify title exactly.

12. Verify content exactly.

13. Verify Unicode exactly.

14. Verify line breaks.

15. Verify special characters.

16. Verify Markdown text.

17. Edit the content again.

18. Verify dirty state.

19. Press Ctrl/Cmd + S.

20. Verify saved state.

21. Switch to another note.

22. Return to the test note.

23. Verify changes remain.

24. Test undo.

25. Test redo.

26. Test copy/paste.

27. Test a large note.

28. Restart again.

29. Verify all persisted content.
```

Do not skip failed steps.

---

# 119. FINAL PHASE 3 RESULT

At the end of Phase 3, Personal Notepad should have:

```text
Ubuntu
   ↓
Personal Notepad
   ↓
Tauri Desktop App
   ↓
React + TypeScript
   ↓
Three-pane UI
   ↓
Real Note Editor
   ↓
Note Service
   ↓
Storage Service
   ↓
Tauri IPC
   ↓
Rust
   ↓
SQLite
```

The user can now:

```text
Create note
      ↓
Edit title
      ↓
Edit content
      ↓
TXT / Markdown
      ↓
Autosave
      ↓
Close application
      ↓
Restart
      ↓
Open note
      ↓
Content remains intact
```

---

# 120. PHASE 3 END STATE

The critical achievement of Phase 3 is:

```text
Personal Notepad is no longer only a UI shell.

It now provides a real,
local,
persistent,
editable note-taking workflow.
```

The editor is backed by the Phase 2 SQLite storage system.

The application remains:

```text
Offline-first
Local-first
No AI
No cloud dependency
No mandatory account
```

The next phase is:

```text
PHASE 4 — NOTEBOOK / FOLDER SYSTEM
```

Phase 4 will build the actual notebook/folder management system on top of the storage architecture already established.

Do NOT start Phase 4 implementation during Phase 3.
