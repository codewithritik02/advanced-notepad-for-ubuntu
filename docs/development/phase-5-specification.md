# PERSONAL NOTEPAD

# PHASE 5 — TAGS, FAVORITES & NOTE METADATA

---

# 1. PHASE OBJECTIVE

Phase 5 ka objective Personal Notepad mein **note organization aur metadata system** implement karna hai.

Phase 4 ke baad application mein:

* notes
* notebooks
* nested notebooks
* note movement
* note editing
* autosave
* persistence

available hain.

Phase 5 mein notes ko organize karne ke liye:

* Tags
* Multiple tags per note
* Tag creation
* Tag rename
* Tag deletion
* Tag assignment/removal
* Favorite notes
* Note metadata
* Metadata display
* Favorite filtering/navigation

implement kiya jayega.

---

# 2. PHASE 5 END STATE

Phase 5 ke baad user ye kar sake:

```text
Create Note
    ↓
Edit Note
    ↓
Assign Notebook
    ↓
Assign one or more Tags
    ↓
Mark as Favorite
    ↓
See note metadata
    ↓
Filter notes by Favorites
    ↓
Filter notes by Tag
    ↓
Remove tags
    ↓
Rename/delete tags
    ↓
Restart application
    ↓
Everything persists
```

Example:

```text
NOTE

Project Plan

Notebook:
Work / Projects

Tags:
work
important
planning

Favorite:
Yes

Created:
30 Sep 2026

Modified:
30 Sep 2026
```

---

# 3. IMPORTANT PRODUCT RULES

## Rule 1 — Local-first

Everything must remain local.

Do not introduce:

* cloud tags
* online metadata
* remote APIs
* account-based organization
* sync

---

## Rule 2 — SQLite remains source of truth

Tags and favorites must persist in SQLite.

Do not use:

* localStorage as the primary data store
* sessionStorage
* JSON files as the primary tag database
* in-memory-only tag assignments

---

## Rule 3 — Preserve Phase 1–4

Do not rewrite:

* three-pane UI
* theme system
* SQLite architecture
* note editor
* autosave
* notebook tree
* notebook selection
* note movement
* note CRUD

Phase 5 extends the existing architecture.

---

# 4. PHASE 5 SCOPE

Implement:

## Tags

* create tag
* list tags
* rename tag
* delete tag
* assign tag to note
* remove tag from note
* multiple tags per note
* tag filtering
* tag persistence

## Favorites

* mark note as favorite
* remove favorite
* favorite navigation/filter
* favorite persistence

## Metadata

* created date
* modified date
* notebook
* tags
* favorite state
* format
* basic note information

---

# 5. EXPLICIT NON-GOALS

Do NOT implement:

* full-text search
* advanced search operators
* tag hierarchy
* nested tags
* tag colors unless already supported by the architecture and trivial
* tag aliases
* tag suggestions based on AI
* automatic AI-generated tags
* attachments
* image metadata
* backlinks
* internal links
* trash/recovery UI
* permanent deletion
* import/export
* backup
* encryption
* sync
* Windows support
* packaging
* AI features

Search belongs to Phase 6.

---

# 6. RELATIONSHIP TO PREVIOUS PHASES

## Phase 2

Provides:

```text
SQLite
notes
tags
note_tags
settings
migration system
```

If Phase 2 already created:

```text
tags
note_tags
```

reuse those tables.

Do not create duplicate tables.

---

## Phase 3

Provides:

```text
note CRUD
note editing
autosave
save-on-close
format
timestamps
soft delete
```

Do not rewrite note editing.

---

## Phase 4

Provides:

```text
notebooks
nested notebooks
notebook selection
note movement
All Notes
Unfiled
```

Phase 5 must integrate with those.

---

# 7. ARCHITECTURE

Use:

```text
React UI
   ↓
Tag / Metadata Feature Layer
   ↓
Tauri Service / Bridge
   ↓
Tauri Commands
   ↓
Rust Service / Repository
   ↓
SQLite
```

For tag assignment:

```text
Note UI
   ↓
tagService
   ↓
Tauri command
   ↓
note_tags
   ↓
SQLite
```

For favorites:

```text
Favorite Button
   ↓
noteService
   ↓
UPDATE notes
SET favorite = ...
```

---

# 8. EXPECTED PROJECT STRUCTURE

Inspect the existing project first.

Do not duplicate existing architecture.

Recommended structure:

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
│   ├── notebooks/
│   │   ├── components/
│   │   ├── hooks/
│   │   ├── services/
│   │   └── types/
│   │
│   └── tags/
│       ├── components/
│       │   ├── TagList.tsx
│       │   ├── TagItem.tsx
│       │   ├── TagPicker.tsx
│       │   ├── TagInput.tsx
│       │   └── TagManager.tsx
│       │
│       ├── hooks/
│       │   ├── useTags.ts
│       │   └── useNoteTags.ts
│       │
│       ├── services/
│       │   └── tagService.ts
│       │
│       ├── types/
│       │   └── tag.ts
│       │
│       └── utils/
│           └── tagUtils.ts
│
└── ...
```

Rust:

```text
src-tauri/
└── src/
    ├── commands/
    │   ├── notes.rs
    │   ├── notebooks.rs
    │   └── tags.rs
    │
    ├── services/
    │   ├── notes.rs
    │   ├── notebooks.rs
    │   └── tags.rs
    │
    └── repositories/
        ├── notes.rs
        ├── notebooks.rs
        └── tags.rs
```

If Phase 2/3 uses a different architecture:

> Extend the existing architecture instead of introducing these exact directories.

---

# 9. DATA MODEL

Expected tag model:

```text
Tag
├── id
├── name
├── created_at
└── modified_at
```

Expected many-to-many relationship:

```text
Note
  │
  ├──── NoteTag ──── Tag
  │
  └──── NoteTag ──── Tag
```

A note can have:

```text
0 tags
1 tag
2 tags
10 tags
```

A tag can belong to:

```text
0 notes
1 note
many notes
```

---

# 10. EXPECTED DATABASE STRUCTURE

If Phase 2 already has these tables, reuse them.

Conceptually:

```sql
tags
----
id
name
created_at
modified_at
```

and:

```sql
note_tags
---------
note_id
tag_id
```

Recommended constraint:

```text
(note_id, tag_id)
```

must be unique.

This prevents:

```text
same tag assigned to same note twice
```

---

# 11. FOREIGN KEY SAFETY

If the existing SQLite configuration supports foreign keys, ensure the relationship is safe.

Recommended behavior:

```text
Delete note
    ↓
note_tags assignments disappear
```

and:

```text
Delete tag
    ↓
tag assignments disappear
    ↓
notes remain untouched
```

Never delete note content because a tag was deleted.

---

# 12. TAG NAMING RULES

Tag names should:

* support Unicode
* support spaces if desired
* support punctuation where reasonable
* reject empty names
* reject whitespace-only names
* be trimmed
* have a reasonable maximum length

Examples:

```text
work
important
planning
2026
project-x
personal
यात्रा
research & ideas
```

Do not force ASCII-only tags.

---

# 13. TAG CASE HANDLING

Decide and document how duplicate names work.

Recommended MVP:

> Tag names are case-insensitive for uniqueness but preserve the user's chosen display casing.

Example:

```text
Work
```

and:

```text
work
```

should not create two different tags.

Instead:

```text
Work
```

is considered the same tag.

Use normalized comparison internally.

Do not change existing tag casing unexpectedly during normal display.

---

# 14. TASK BREAKDOWN

Complete tasks one at a time.

Do not implement the entire phase in one uncontrolled change.

---

# TASK 1 — Inspect Existing Phase 2–4 Tag/Favorite Schema

## Objective

Before writing code, inspect:

* `tags`
* `note_tags`
* `notes.favorite`
* migrations
* note repository
* notebook repository
* Tauri command registration
* existing sidebar
* note list
* note metadata UI

Determine:

```text
Does tags table already exist?
Does note_tags already exist?
How is favorite stored?
What is the existing boolean representation?
How are timestamps stored?
```

Do not modify anything during inspection unless a clear Phase 5 blocker is found.

## Verification

```text
TASK COMPLETED:
Task 1 — Inspect Existing Phase 2–4 Tag/Favorite Schema

CHANGES:
- ...

FILES CREATED:
- None

FILES MODIFIED:
- None

DEPENDENCIES ADDED:
- None

DATABASE CHANGES:
- None

VERIFICATION:
- Tags schema inspected: PASS/FAIL
- note_tags schema inspected: PASS/FAIL
- Favorite field inspected: PASS/FAIL
- Existing note architecture inspected: PASS/FAIL

NOT IMPLEMENTED YET:
- ...

NOTES FOR NEXT TASK:
- ...
```

---

# TASK 2 — Verify / Complete Tag Database Schema

Ensure the database supports:

```text
tags
note_tags
```

If already present:

```text
reuse
```

If incomplete:

```text
create a new migration
```

Never rewrite an already-applied migration.

---

## Required properties

### Tags

```text
id
name
created_at
modified_at
```

### Note tags

```text
note_id
tag_id
```

Recommended:

```text
PRIMARY KEY(note_id, tag_id)
```

---

# TASK 3 — Verify Favorite Field

Phase 2/3 should already have a favorite/pinned field according to the roadmap.

Inspect the actual schema.

Do not create:

```text
favorite_notes
```

if a boolean field already exists.

Preferred:

```text
notes.favorite
```

or whatever Phase 2 established.

Use the existing representation.

---

# TASK 4 — Create Tag Domain Types

Create one canonical TypeScript type.

Conceptually:

```ts
type Tag = {
  id: string;
  name: string;
  createdAt: string;
  modifiedAt: string;
};
```

Do not create multiple incompatible tag types.

---

# TASK 5 — Create Tag Backend Commands

Implement:

```text
tags_list
tags_get
tags_create
tags_update
tags_delete
```

Use the project's existing naming convention.

---

## Create

Input:

```json
{
  "name": "work"
}
```

Output:

```json
{
  "id": "...",
  "name": "work",
  "createdAt": "...",
  "modifiedAt": "..."
}
```

---

## Get

Input:

```json
{
  "id": "..."
}
```

Return:

```text
Tag
```

or:

```text
NotFound
```

---

## List

Return all tags.

Recommended ordering:

```text
name ASC
```

with deterministic case-insensitive comparison.

---

## Update

Primarily rename:

```json
{
  "id": "...",
  "name": "important"
}
```

Do not change:

```text
id
created_at
```

---

## Delete

Delete the tag.

Its note associations should be removed.

Notes must remain.

---

# TASK 6 — Tag Validation

Backend must validate tag names.

Reject:

```text
""
"   "
```

Trim:

```text
"  work  "
```

to:

```text
"work"
```

Reasonable maximum:

```text
for example 100–150 characters
```

Use a practical limit rather than an unnecessarily tiny one.

Do not impose arbitrary restrictions such as:

```text
only [a-zA-Z0-9]
```

---

# TASK 7 — Prevent Duplicate Tags

Before creating:

```text
Work
```

check normalized name.

If:

```text
work
```

already exists:

Do not create a duplicate tag.

Return either:

```text
existing tag
```

or:

```text
Conflict
```

depending on the architecture.

Recommended UI behavior:

> If the tag already exists, use the existing tag rather than creating another one.

---

# TASK 8 — Create Tag Frontend Service

Create a typed service abstraction.

Conceptually:

```ts
tagService.list()
tagService.get(id)
tagService.create(name)
tagService.rename(id, name)
tagService.delete(id)
```

Do not scatter direct Tauri calls across components.

---

# TASK 9 — Load Real Tags

At application startup or when the tag UI is opened:

```text
SQLite
   ↓
tags_list
   ↓
React state
```

No hard-coded tags should remain as the source of truth.

---

# TASK 10 — Add Tags Section to Sidebar

Extend the Phase 4 sidebar.

Possible structure:

```text
ALL NOTES

Unfiled
Favorites

NOTEBOOKS

▾ Work
    Projects
    Meetings

TAGS

# work
# important
# planning

+ New Tag
```

Do not overcrowd the sidebar.

If the tag list becomes long, use:

```text
scroll
```

or:

```text
Show all tags
```

rather than making the entire sidebar unusable.

---

# TASK 11 — Create Tag UI

Add:

```text
+ New Tag
```

Dialog:

```text
Create Tag

Name
[________________]

[Cancel] [Create]
```

Requirements:

* autofocus input
* Enter submits
* Escape cancels
* empty name rejected
* whitespace rejected
* duplicate handled
* Unicode supported

---

# TASK 12 — Rename Tag

Allow:

```text
Tag context menu
→ Rename
```

or another simple existing UI pattern.

Example:

```text
# work
```

becomes:

```text
# Work
```

Renaming a tag must update the tag record only.

All notes assigned to that tag must continue using the same tag ID.

Do not recreate the tag and reassign every note unless the existing architecture requires it.

---

# TASK 13 — Delete Tag

Allow tag deletion.

Confirmation recommended:

```text
Delete tag "work"?

The tag will be removed from notes,
but the notes themselves will not be deleted.

[Cancel] [Delete]
```

After deletion:

```text
Tag disappears
Assignments disappear
Notes remain
```

---

# TASK 14 — Create Note Tag Assignment Model

Implement operations:

```text
addTagToNote(noteId, tagId)
removeTagFromNote(noteId, tagId)
getTagsForNote(noteId)
```

Optional:

```text
setTagsForNote(noteId, tagIds)
```

A `setTagsForNote` operation can be useful when editing a complete tag selection.

---

# TASK 15 — Backend Note Tag Commands

Recommended commands:

```text
note_tags_list
note_tag_add
note_tag_remove
```

or a project-consistent equivalent.

---

## Add

Input:

```json
{
  "noteId": "...",
  "tagId": "..."
}
```

Must be idempotent.

Calling:

```text
add(note, tag)
```

twice should not create duplicate rows.

---

## Remove

Input:

```json
{
  "noteId": "...",
  "tagId": "..."
}
```

If assignment does not exist:

```text
safe no-op
```

or a clear NotFound response.

---

# TASK 16 — Load Tags for Selected Note

When the user selects a note:

```text
Select note
    ↓
load note
    ↓
load note tags
    ↓
display tags
```

Avoid stale tag UI.

If user changes notes:

```text
old note tags
```

must not temporarily appear as the new note's tags.

---

# TASK 17 — Add Tag Picker to Note UI

Add a tag area in the editor or metadata section.

Example:

```text
Tags

[work] [important] [+ Add tag]
```

Click:

```text
+ Add tag
```

opens:

```text
Select tags

☑ work
☐ important
☐ planning
☐ personal

[Create new tag]
```

The exact UI can vary, but it should remain simple.

---

# TASK 18 — Multi-Tag Support

A note must be able to have multiple tags.

Example:

```text
Project Plan

Tags:
work
planning
important
```

Adding one tag must not remove existing tags.

Removing one tag must not remove others.

---

# TASK 19 — Tag Assignment Persistence

Perform:

```text
Note
→ add work
→ add important
→ close app
→ reopen
```

Expected:

```text
Tags:
work
important
```

No tag assignment should be memory-only.

---

# TASK 20 — Add Favorites Backend Support

Use the existing note favorite field.

Implement a safe update operation.

Conceptually:

```text
set_note_favorite(noteId, true)
set_note_favorite(noteId, false)
```

or extend existing note update:

```json
{
  "id": "...",
  "favorite": true
}
```

Prefer extending the existing note update API if that architecture already supports partial note metadata updates.

Do not create a separate favorites table unless Phase 2 explicitly requires one.

---

# TASK 21 — Favorite Toggle UI

Add a favorite control.

Possible placement:

```text
Editor top bar
```

or:

```text
Note list item
```

Recommended:

```text
☆ Not favorite
★ Favorite
```

Use accessible labels:

```text
Add to favorites
Remove from favorites
```

Do not rely only on icon meaning.

---

# TASK 22 — Favorite Persistence

Test:

```text
Mark note favorite
→ close app
→ reopen
```

Expected:

```text
favorite = true
```

Then:

```text
unfavorite
→ restart
```

Expected:

```text
favorite = false
```

---

# TASK 23 — Favorites Navigation

Add:

```text
Favorites
```

to the sidebar.

Selecting Favorites should show:

```text
all non-deleted notes
WHERE favorite = true
```

This must not be confused with a notebook.

Use explicit navigation state.

Recommended:

```ts
type NoteLocation =
  | { type: "all" }
  | { type: "unfiled" }
  | { type: "favorites" }
  | { type: "notebook"; notebookId: string }
  | { type: "tag"; tagId: string };
```

Adapt the Phase 4 `NoteLocation` type instead of creating a competing navigation state.

---

# TASK 24 — Tag Navigation

Selecting a tag:

```text
# work
```

should display all non-deleted notes assigned to that tag.

Conceptually:

```sql
SELECT n.*
FROM notes n
JOIN note_tags nt
  ON nt.note_id = n.id
WHERE nt.tag_id = ?
  AND n.deleted = 0
ORDER BY n.modified_at DESC;
```

Use the actual schema/column names from the project.

---

# TASK 25 — Combine Tag Filtering with Notebook Selection

Do NOT implement advanced multi-filter search yet.

Phase 5 should support one primary navigation context at a time.

Examples:

```text
Work notebook
```

or:

```text
# work
```

or:

```text
Favorites
```

Do not build:

```text
Work + #important + Favorite + modified-after
```

filter expressions.

That belongs conceptually closer to Search/advanced filtering.

---

# TASK 26 — Tag Empty State

If a tag has no notes:

```text
# planning

No notes use this tag yet.
```

Do not show unrelated notes.

---

# TASK 27 — Favorites Empty State

If there are no favorites:

```text
Favorites

You haven't favorited any notes yet.
```

Optionally:

```text
Select a note and mark it as favorite.
```

---

# TASK 28 — Tag List Empty State

If there are no tags:

```text
TAGS

No tags yet.

+ New Tag
```

The application must remain fully usable without tags.

Tags are optional organization metadata.

---

# TASK 29 — Note Metadata Model

Define a canonical metadata representation.

Conceptually:

```ts
type NoteMetadata = {
  createdAt: string;
  modifiedAt: string;
  format: "txt" | "md";
  notebookId: string | null;
  favorite: boolean;
  tags: Tag[];
};
```

Do not duplicate fields already available on the main `Note` type unless necessary.

Prefer composition:

```ts
type Note = {
  ...
  favorite: boolean;
  notebookId: string | null;
};

type NoteMetadata = {
  ...
};
```

---

# TASK 30 — Metadata Panel

Add a lightweight metadata display.

Possible placement:

```text
Editor
    ↓
Metadata section
```

Example:

```text
NOTE DETAILS

Notebook
Work / Projects

Tags
work
planning

Created
30 Sep 2026, 01:45

Modified
30 Sep 2026, 02:01

Format
Plain Text

Favorite
Yes
```

Do not turn this into a complex inspector panel.

---

# TASK 31 — Metadata Read-Only Rules

Phase 5 metadata display should distinguish:

### Editable metadata

* title
* tags
* favorite
* notebook assignment

### Informational metadata

* created time
* modified time
* format

Do not allow direct editing of:

```text
created_at
```

unless a future feature explicitly requires it.

---

# TASK 32 — Format Display

If note format is:

```text
txt
```

display:

```text
Plain Text
```

If:

```text
md
```

display:

```text
Markdown
```

Do not add a full Markdown preview/editor in Phase 5.

---

# TASK 33 — Notebook Metadata Integration

The metadata panel should display:

```text
Notebook
Work / Projects
```

rather than only:

```text
notebookId: abc123
```

Use the actual notebook hierarchy to produce a human-readable path.

Example:

```text
Work / Projects / Releases
```

Do not expose internal IDs to users.

---

# TASK 34 — Tag Display

Tags should be visually distinct but compact.

Example:

```text
[work] [important] [planning]
```

Do not use excessive visual decoration.

Use the existing design tokens.

---

# TASK 35 — Tag Removal

Each assigned tag should have a clear removal action.

Example:

```text
[work ×]
[planning ×]
```

Accessibility label:

```text
Remove tag work
```

Removing a tag must not modify:

* note content
* title
* notebook
* favorite state

---

# TASK 36 — Tag Creation from Note Editor

Recommended UX:

```text
Tags
[work] [important] [+]
```

Click:

```text
+
```

Then:

```text
Search or create tag
[________________]

Existing:
work
important
planning

Create "research"
```

However:

> Do not implement a full search engine here.

This is only tag-name filtering within the currently loaded tag list.

---

# TASK 37 — Prevent Duplicate Assignment

If:

```text
work
```

is already assigned to a note:

Selecting it again must not produce:

```text
work
work
```

The UI should show it as selected.

Database uniqueness should also protect against duplication.

---

# TASK 38 — Tag Rename Propagation

Suppose:

```text
Tag ID = 123
Name = work
```

is assigned to 100 notes.

Rename:

```text
work → projects
```

All 100 notes should automatically show:

```text
projects
```

because their relationship remains:

```text
note_tags.tag_id = 123
```

Do not duplicate or recreate assignments.

---

# TASK 39 — Tag Delete Propagation

Suppose:

```text
work
```

is assigned to 100 notes.

Delete the tag.

Expected:

```text
100 notes remain
100 tag assignments disappear
tag disappears
```

No note should be deleted.

---

# TASK 40 — Favorite + Tag Interaction

A note may simultaneously be:

```text
Favorite = true
Tags = work, important
Notebook = Projects
```

These metadata properties must not conflict.

Example:

```text
Favorites
```

must still show the note.

```text
# work
```

must still show the note.

```text
Projects
```

must still show the note.

---

# TASK 41 — Deleted Notes

Respect Phase 3 soft-delete semantics.

By default:

```text
All Notes
Favorites
Tags
Notebook lists
Unfiled
```

must not show deleted notes.

Do not build Trash UI in Phase 5.

If Phase 3 uses a different deleted-state implementation, reuse it.

---

# TASK 42 — Note List Metadata Indicators

Optionally show compact metadata in the note list.

Example:

```text
Project Plan
Work / Projects
[work] [important]
★
Modified 2 min ago
```

Do not overcrowd the list.

At minimum, favorite state should be visible somewhere obvious.

Tags may be displayed if space permits.

---

# TASK 43 — Sorting

For Phase 5, retain the existing note list sorting.

Recommended:

```text
modified_at DESC
```

Do not add:

* sort by tag
* sort by favorite
* sort by created date
* custom sort rules

unless already part of the existing architecture.

Advanced sorting is not required.

---

# TASK 44 — Sidebar Overflow

If there are many tags:

```text
100 tags
```

do not allow the sidebar to grow indefinitely.

Possible solution:

```text
TAGS
# work
# important
# planning
# ...
Show all
```

or:

```text
scrollable tag section
```

Choose the simplest approach compatible with the existing sidebar.

---

# TASK 45 — Tag Manager

A dedicated tag management UI may be added if the sidebar context menu becomes too crowded.

Example:

```text
Manage Tags

# work       Rename   Delete
# personal   Rename   Delete
# planning   Rename   Delete

[+ New Tag]
```

Keep it lightweight.

Do not turn it into a separate complex application screen.

---

# TASK 46 — Keyboard Behavior

Preserve existing shortcuts:

```text
Ctrl/Cmd + N
Ctrl/Cmd + S
Ctrl/Cmd + Z
Ctrl/Cmd + Shift + Z
```

Do not break editor shortcuts when tag controls receive focus.

For dialogs:

```text
Enter = submit
Escape = cancel
```

Do not assign aggressive global shortcuts for tags.

---

# TASK 47 — Accessibility

Tag UI must support:

* keyboard focus
* visible focus
* accessible buttons
* meaningful labels
* keyboard removal
* dialog keyboard handling

Favorite button:

```text
aria-label="Add to favorites"
```

or:

```text
aria-label="Remove from favorites"
```

depending on state.

---

# TASK 48 — Unicode Testing

Test tags:

```text
work
यात्रा
旅行
café
research & ideas
2026
project-x
```

Test:

* create
* assign
* rename
* filter
* remove
* restart

---

# TASK 49 — Special Character Testing

Test:

```text
C++
C#
R&D
Project / Planning
Q4 2026
Design & UX
```

Ensure:

* SQL remains parameterized
* UI remains stable
* tag filtering works
* names persist exactly

---

# TASK 50 — Large Tag Assignment Test

Create:

```text
20+ tags
```

Assign several to one note.

Verify:

* picker remains usable
* duplicate assignments do not occur
* removal works
* note reopening loads exact assignments
* restart preserves assignments

Do not optimize prematurely unless actual performance problems appear.

---

# TASK 51 — Large Note Regression

Take a large note from Phase 3.

Assign:

```text
5 tags
```

Mark favorite.

Move notebook.

Rename a tag.

Restart.

Verify:

* note content unchanged
* title unchanged
* autosave works
* undo/redo works
* cursor behavior remains intact
* metadata is correct

---

# TASK 52 — Backend Transaction Safety

Operations affecting multiple tables should be transactional where necessary.

For tag deletion:

```text
BEGIN
    delete note_tags
    delete tag
COMMIT
```

If foreign-key cascade is configured correctly, the database may handle the association cleanup.

Do not duplicate cleanup unnecessarily.

---

# TASK 53 — Database Integrity

Verify:

```text
No orphan note_tags
No duplicate note_tags
No tag without valid ID
No note deleted because tag deleted
No invalid favorite values
```

Run integrity tests after database operations.

---

# TASK 54 — Error Handling

Define useful errors:

```text
ValidationError
NotFound
Conflict
DatabaseError
TagInUse
```

`TagInUse` is optional if deletion is always allowed.

User-facing messages should be simple.

Example:

Backend:

```text
Conflict
```

UI:

```text
A tag with this name already exists.
```

---

# TASK 55 — Offline Verification

Disable network access if practical.

Verify:

* notes work
* notebooks work
* tags work
* favorites work
* metadata works

No Phase 5 feature should require internet access.

---

# TASK 56 — No AI Verification

Search the project for accidental AI integrations.

Do not introduce:

```text
OpenAI
LLM
AI tagging
automatic summaries
suggested tags
smart categorization
```

No AI behavior belongs in this phase.

---

# TASK 57 — Remove Placeholder Data

Remove:

* mock tags
* fake favorite state
* fake metadata
* hard-coded tag lists

where real database-backed functionality now exists.

Do not remove unrelated test fixtures.

---

# TASK 58 — Update Documentation

Document:

## Tags

* many-to-many relationship
* naming rules
* duplicate handling
* deletion behavior

## Favorites

* favorite field
* filtering behavior

## Metadata

* editable fields
* read-only fields
* timestamp behavior

---

# TASK 59 — Backend Unit Tests

Test:

```text
create tag
get tag
list tags
rename tag
delete tag
duplicate tag
Unicode tag
empty tag
whitespace tag
long tag
```

---

# TASK 60 — Note Tag Tests

Test:

```text
assign tag
assign same tag twice
remove tag
remove missing tag
multiple tags
delete tag
note survives tag deletion
```

---

# TASK 61 — Favorite Tests

Test:

```text
favorite note
unfavorite note
favorite persists
favorite filtering
deleted note does not appear in Favorites
```

---

# TASK 62 — Tag Filter Tests

Test:

```text
tag with no notes
tag with one note
tag with multiple notes
deleted note excluded
multiple notes ordered correctly
```

---

# TASK 63 — Frontend Tests

Where existing frontend testing infrastructure is available, test:

```text
TagPicker
Tag display
Tag removal
Favorite toggle
Favorites navigation
Tag navigation
Metadata panel
Empty states
```

Do not add a heavy testing framework just for these components if the project does not already have one.

---

# TASK 64 — Manual QA

Perform this exact workflow.

## Test A — Create tags

```text
Create:
work
important
planning
```

Expected:

```text
All appear in Tags.
```

---

## Test B — Assign tags

```text
Open note
→ Add work
→ Add important
```

Expected:

```text
[work] [important]
```

---

## Test C — Remove one tag

```text
Remove work
```

Expected:

```text
[important]
```

---

## Test D — Favorite

```text
Mark note as favorite
```

Expected:

```text
Favorites contains note
```

---

## Test E — Restart

```text
Close
→ reopen
```

Expected:

```text
Tags preserved
Favorite preserved
```

---

## Test F — Rename tag

```text
work → projects
```

Expected:

```text
All affected notes now show projects.
```

---

## Test G — Delete tag

```text
Delete projects
```

Expected:

```text
Tag disappears.
Notes remain.
```

---

## Test H — Tag filter

```text
Select #important
```

Expected:

```text
Only notes with important tag appear.
```

---

## Test I — Notebook + tags

```text
Work
└── Projects
    └── Project Plan

Tags:
work
important

Favorite:
Yes
```

Verify all metadata is consistent.

---

# TASK 65 — Restart Regression Test

Complete:

```text
Create notebook
Create note
Assign 3 tags
Mark favorite
Edit note
Move note
Rename tag
Close application
Reopen
```

Expected:

```text
Notebook assignment correct
Tags correct
Favorite correct
Content correct
Title correct
Created time unchanged
Modified time correct
```

---

# TASK 66 — Build Verification

Use the repository's actual scripts.

Inspect:

```text
package.json
src-tauri/Cargo.toml
```

Run appropriate:

```text
TypeScript/typecheck
Rust cargo check
Rust tests
Frontend tests
Production build
Tauri build/check
```

Do not invent commands.

---

# TASK 67 — Final Cleanup

Remove:

* duplicate tag types
* duplicate tag services
* unused imports
* debug logs
* mock tag data
* dead code
* temporary UI
* unnecessary dependencies

Do not perform unrelated refactoring.

---

# 15. TAG API CONTRACT

Exact names should follow the existing project convention.

## Create

```text
tag_create
```

Input:

```json
{
  "name": "work"
}
```

Output:

```json
{
  "id": "...",
  "name": "work",
  "createdAt": "...",
  "modifiedAt": "..."
}
```

---

## List

```text
tag_list
```

Output:

```json
[
  {
    "id": "...",
    "name": "work",
    "createdAt": "...",
    "modifiedAt": "..."
  }
]
```

---

## Rename

```text
tag_update
```

Input:

```json
{
  "id": "...",
  "name": "important"
}
```

---

## Delete

```text
tag_delete
```

Input:

```json
{
  "id": "..."
}
```

---

# 16. NOTE TAG API

Conceptually:

```text
note_tags_list
```

Input:

```json
{
  "noteId": "..."
}
```

---

```text
note_tag_add
```

Input:

```json
{
  "noteId": "...",
  "tagId": "..."
}
```

---

```text
note_tag_remove
```

Input:

```json
{
  "noteId": "...",
  "tagId": "..."
}
```

---

# 17. FAVORITE API

Prefer extending the existing note update mechanism.

Conceptually:

```json
{
  "id": "...",
  "favorite": true
}
```

or:

```text
note_set_favorite
```

if the existing architecture uses dedicated commands.

Do not create two different favorite update mechanisms.

---

# 18. NOTE LOCATION STATE

Extend Phase 4 navigation state.

Recommended:

```ts
type NoteLocation =
  | { type: "all" }
  | { type: "unfiled" }
  | { type: "favorites" }
  | { type: "notebook"; notebookId: string }
  | { type: "tag"; tagId: string };
```

This allows the note list to know exactly what it represents.

Avoid:

```ts
selectedNotebookId = null
```

meaning multiple different things.

---

# 19. QUERY BEHAVIOR

## All Notes

```sql
WHERE deleted = 0
```

---

## Unfiled

```sql
WHERE deleted = 0
AND notebook_id IS NULL
```

---

## Favorites

```sql
WHERE deleted = 0
AND favorite = 1
```

---

## Notebook

```sql
WHERE deleted = 0
AND notebook_id = ?
```

---

## Tag

Use the many-to-many relationship:

```sql
JOIN note_tags
ON note_tags.note_id = notes.id
WHERE note_tags.tag_id = ?
AND notes.deleted = 0
```

Use parameterized queries.

---

# 20. TAG NORMALIZATION

Use a normalization strategy for uniqueness.

Conceptually:

```text
display name:
"Work"

normalized:
"work"
```

Store:

```text
name = "Work"
```

but compare:

```text
normalized(name)
```

for uniqueness.

If implementing normalized storage:

Do so consistently.

Do not create inconsistent rules between frontend and backend.

Backend must remain authoritative.

---

# 21. FAVORITE UX

Favorite should be fast.

Recommended:

```text
Click star
    ↓
Immediate visual update
    ↓
Persist to SQLite
```

If persistence fails:

```text
revert visual state
show error
```

Do not show:

```text
favorite = true
```

permanently if the database rejected the operation.

---

# 22. TAG UX

Tag assignment can also be optimistic if carefully implemented.

However, because local SQLite operations are expected to be fast:

> Prefer a simple request → successful update → UI update flow rather than building a complicated optimistic synchronization system.

Simplicity is more important than theoretical latency optimization.

---

# 23. METADATA UPDATE RULES

When changing:

```text
favorite
tag assignment
notebook assignment
```

the application may update:

```text
modified_at
```

depending on the product semantics.

Recommended:

* content/title edit → definitely updates modified time
* notebook move → updates modified time
* favorite toggle → may update modified time
* tag assignment → may update modified time

Use one consistent policy.

Document it.

Do not make timestamp behavior unpredictable.

---

# 24. CREATED VS MODIFIED

`created_at`:

> Never changes after note creation.

`modified_at`:

> Changes when note data or meaningful metadata changes according to the project's chosen policy.

Example:

```text
Create note
created = 10:00
modified = 10:00

Edit content
created = 10:00
modified = 10:05

Add tag
created = 10:00
modified = 10:06
```

If metadata changes are not treated as modification events in the existing architecture, preserve that existing policy instead.

---

# 25. METADATA DISPLAY FORMATTING

Store timestamps using the existing Phase 2 format.

Convert to user-friendly display only in the UI.

Example:

```text
Created
30 Sep 2026, 10:30 PM

Modified
30 Sep 2026, 10:45 PM
```

Do not store formatted strings such as:

```text
"2 minutes ago"
```

in SQLite.

---

# 26. TIMEZONE RULE

Use the existing Phase 2 timestamp strategy.

Recommended:

```text
database = UTC
UI = local timezone
```

Do not introduce a second timestamp convention.

---

# 27. TAG DELETE SAFETY

Deleting a tag must never cause:

```text
note deletion
```

or:

```text
notebook deletion
```

Only the tag and its associations are removed.

---

# 28. FAVORITES + SOFT DELETE

If a note is soft-deleted:

```text
favorite = true
```

must not cause it to appear in Favorites.

The deleted state has priority.

Example:

```text
deleted = true
favorite = true
```

must be excluded from:

```text
Favorites
Tags
Notebooks
All Notes
Unfiled
```

according to existing Phase 3 visibility rules.

---

# 29. SIDEBAR FINAL STRUCTURE

Recommended:

```text
┌─────────────────────────┐
│ ALL NOTES               │
│                         │
│ Unfiled                 │
│ Favorites               │
│                         │
│ NOTEBOOKS               │
│                         │
│ ▾ Work                  │
│   ▸ Projects            │
│   Meetings              │
│                         │
│ ▸ Personal              │
│                         │
│ TAGS                    │
│                         │
│ # work                  │
│ # important             │
│ # planning              │
│                         │
│ + New Notebook          │
│ + New Tag               │
└─────────────────────────┘
```

The actual visual design should follow the existing Personal Notepad design system.

---

# 30. NOTE EDITOR FINAL METADATA AREA

Recommended:

```text
┌──────────────────────────────────────┐
│ Project Plan                  ★      │
├──────────────────────────────────────┤
│                                      │
│ Note content...                      │
│                                      │
│                                      │
├──────────────────────────────────────┤
│ Notebook: Work / Projects             │
│ Tags: [work] [important] [+]         │
│ Created: 30 Sep 2026                  │
│ Modified: 30 Sep 2026                 │
│ Format: Plain Text                    │
└──────────────────────────────────────┘
```

Do not make the metadata section consume most of the editor.

---

# 31. PERFORMANCE RULES

Avoid:

```text
one SQL query per tag
```

For sidebar:

```text
load all tags once
```

For note:

```text
load tags for selected note
```

For tag filtering:

```text
one indexed relationship query
```

Consider indexes where justified:

```text
note_tags.note_id
note_tags.tag_id
notes.favorite
```

Do not add indexes blindly.

Use SQLite query patterns and existing schema conventions.

---

# 32. DATABASE INDEXES

If performance testing or schema inspection indicates need, consider:

```sql
CREATE INDEX ...
ON note_tags(note_id);
```

and:

```sql
CREATE INDEX ...
ON note_tags(tag_id);
```

The composite primary key may already provide one useful index.

Do not create redundant indexes.

---

# 33. DATA INTEGRITY RULES

The following must always be true:

```text
Every note_tag.note_id points to a valid note.
Every note_tag.tag_id points to a valid tag.
The same note/tag pair appears at most once.
A tag deletion does not delete a note.
A note deletion does not unexpectedly delete unrelated tags.
Favorite is always a valid boolean representation.
```

---

# 34. SECURITY RULES

Use parameterized SQL.

Never concatenate:

```text
tag names
note IDs
tag IDs
```

into SQL strings.

Validate all IDs.

Do not trust UI validation alone.

Backend must validate again.

Do not expose raw SQLite errors.

Do not log full note content.

---

# 35. DEPENDENCY RULES

Do not add:

* large tagging libraries
* cloud services
* analytics
* tracking
* remote APIs
* AI services
* unnecessary state management libraries

Existing React state/context is sufficient for Phase 5 unless the repository already uses another state architecture.

---

# 36. TEST MATRIX

Minimum test matrix:

| Feature    | Test                 |
| ---------- | -------------------- |
| Create tag | Valid name           |
| Create tag | Empty name           |
| Create tag | Whitespace           |
| Create tag | Duplicate            |
| Create tag | Unicode              |
| Rename tag | Valid                |
| Rename tag | Duplicate            |
| Delete tag | Unassigned           |
| Delete tag | Assigned             |
| Assign tag | One tag              |
| Assign tag | Multiple tags        |
| Assign tag | Duplicate assignment |
| Remove tag | Existing             |
| Remove tag | Missing              |
| Favorite   | Mark                 |
| Favorite   | Unmark               |
| Favorite   | Persistence          |
| Tag filter | One note             |
| Tag filter | Many notes           |
| Tag filter | Deleted note         |
| Metadata   | Correct timestamps   |
| Metadata   | Notebook path        |
| Metadata   | Format               |
| Restart    | All data             |
| Theme      | Light                |
| Theme      | Dark                 |
| Theme      | System               |

---

# 37. FINAL ACCEPTANCE CRITERIA

Phase 5 is complete only when:

## Tags

* [ ] Tags are stored in SQLite.
* [ ] Tags can be created.
* [ ] Tags can be renamed.
* [ ] Tags can be deleted.
* [ ] Duplicate tags are prevented.
* [ ] Unicode tags work.
* [ ] Multiple tags can belong to one note.
* [ ] One tag can belong to multiple notes.
* [ ] Tags persist across restart.
* [ ] Tag filtering works.

## Favorites

* [ ] Notes can be favorited.
* [ ] Notes can be unfavorited.
* [ ] Favorite state persists.
* [ ] Favorites navigation works.
* [ ] Deleted notes do not appear in Favorites.

## Metadata

* [ ] Created time is shown.
* [ ] Modified time is shown.
* [ ] Notebook is shown.
* [ ] Tags are shown.
* [ ] Favorite state is shown.
* [ ] Format is shown.
* [ ] Internal IDs are not shown.

## Integration

* [ ] Phase 4 notebooks continue working.
* [ ] Note movement continues working.
* [ ] Phase 3 autosave continues working.
* [ ] Note editor continues working.
* [ ] Undo/redo continues working.
* [ ] Cursor preservation continues working.
* [ ] Three-pane layout remains intact.
* [ ] Theme system remains intact.

---

# 38. REQUIRED COMPLETE QA WORKFLOW

Run this exact scenario before declaring Phase 5 complete:

```text
1. Launch Personal Notepad.

2. Create notebook:
   Work

3. Create child:
   Projects

4. Create note:
   Project Plan

5. Put note in:
   Work / Projects

6. Create tags:
   work
   important
   planning

7. Assign:
   work
   important
   planning

8. Mark note as favorite.

9. Verify metadata:
   Notebook = Work / Projects
   Tags = work, important, planning
   Favorite = Yes
   Format = Plain Text
   Created = correct
   Modified = correct

10. Select:
    Favorites

11. Verify Project Plan appears.

12. Select:
    #work

13. Verify Project Plan appears.

14. Select:
    #important

15. Verify Project Plan appears.

16. Remove:
    planning

17. Verify:
    work
    important

18. Rename:
    work → projects

19. Verify affected notes now show:
    projects

20. Delete:
    projects

21. Verify:
    Project Plan still exists.

22. Mark Project Plan as not favorite.

23. Verify it disappears from Favorites.

24. Close application.

25. Reopen application.

26. Verify:
    Work / Projects
    Project Plan
    important tag
    favorite state
    note content
    timestamps

27. Test Unicode tag:
    यात्रा

28. Assign it.

29. Restart again.

30. Verify Unicode tag persists.

31. Test Light theme.

32. Test Dark theme.

33. Test System theme.

34. Test large note.

35. Test undo/redo.

36. Test autosave.

37. Run automated tests.

38. Run TypeScript/typecheck.

39. Run Rust check/tests.

40. Run production/Tauri build.
```

---

# 39. FINAL PHASE ARCHITECTURE

After Phase 5:

```text
                         PERSONAL NOTEPAD
                                │
                                ▼
                         Tauri Desktop App
                                │
                    ┌───────────┴───────────┐
                    ▼                       ▼
                 React UI                Rust Core
                    │                       │
       ┌────────────┼─────────────┐         │
       ▼            ▼             ▼         │
    Notes       Notebooks       Tags        │
       │            │             │         │
       │            │             │         │
       └────────────┼─────────────┘         │
                    │                       │
                    ▼                       ▼
                  SQLite Database
                    │
       ┌────────────┼───────────────┐
       ▼            ▼               ▼
     notes      notebooks          tags
       │                            │
       │                            │
       └──────── note_tags ─────────┘
```

---

# 40. USER EXPERIENCE AFTER PHASE 5

The application should now support:

```text
Create note
     ↓
Edit note
     ↓
Choose notebook
     ↓
Add tags
     ↓
Mark favorite
     ↓
View metadata
     ↓
Navigate by:
     ├── All Notes
     ├── Unfiled
     ├── Favorites
     ├── Notebook
     └── Tag
```

This creates the core organizational layer of Personal Notepad.

---

# 41. FEATURES STILL NOT IMPLEMENTED

The following must remain outside Phase 5:

```text
Advanced Search
Full-text Search
Search Ranking
Attachments
Trash UI
Recovery
Permanent Deletion
Internal Links
Backlinks
Import
Export
Backup
Encryption
Sync
Windows
Ubuntu Packaging
AI
```

Do not implement them as extra improvements.

---

# 42. AI CODING AGENT RULES

While implementing Phase 5:

### Rule A

Inspect before changing.

### Rule B

Complete one task at a time.

### Rule C

Reuse Phase 2 database architecture.

### Rule D

Reuse Phase 3 note architecture.

### Rule E

Reuse Phase 4 notebook/navigation architecture.

### Rule F

Do not duplicate types or services.

### Rule G

Do not add unnecessary dependencies.

### Rule H

Do not implement Search.

### Rule I

Do not implement AI.

### Rule J

Do not implement cloud/sync.

### Rule K

Do not rewrite the editor.

### Rule L

Do not claim success without actual verification.

### Rule M

If an existing implementation differs from this brief:

```text
inspect
→ preserve working architecture
→ adapt Phase 5
→ avoid unnecessary rewrites
```

---

# 43. REQUIRED PER-TASK REPORT FORMAT

After every task, report:

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

Never claim:

```text
PASS
```

without actually running the relevant verification.

---

# 44. FINAL PHASE REPORT

After all Phase 5 tasks are complete:

```text
PHASE 5 COMPLETE

IMPLEMENTED:
- Tag CRUD
- Multiple tags per note
- Tag assignment/removal
- Tag filtering
- Favorite notes
- Favorites navigation
- Note metadata
- Notebook metadata display
- Tag persistence
- Favorite persistence

DATABASE:
- SQLite-backed
- Migration-safe
- note_tags relationship protected
- No duplicate assignments
- Notes preserved when tags are deleted

EDITOR:
- Phase 3 behavior preserved
- Autosave preserved
- Undo/redo preserved
- Cursor behavior preserved

NOTEBOOKS:
- Phase 4 hierarchy preserved
- Note movement preserved
- Notebook filtering preserved

UI:
- Sidebar extended
- Tags available
- Favorites available
- Metadata available
- Three-pane layout preserved
- Theme system preserved

TESTING:
- TypeScript: PASS/FAIL
- Rust: PASS/FAIL
- Database tests: PASS/FAIL
- Frontend tests: PASS/FAIL/N/A
- Build: PASS/FAIL
- Application launch: PASS/FAIL
- Manual QA: PASS/FAIL

NOT IMPLEMENTED:
- Search
- Attachments
- Trash UI
- Recovery
- Internal links
- Import/export
- Backup
- Encryption
- Sync
- Windows
- Packaging
- AI

READY FOR:
PHASE 6 — SEARCH
```

---

# 45. PHASE 5 SUCCESS DEFINITION

Phase 5 is successful when this statement is true:

> **A user can organize every note using notebooks, multiple tags, and favorites; navigate notes through All Notes, Unfiled, Favorites, notebooks, and tags; inspect note metadata; and close/reopen Personal Notepad without losing any organizational state or note content.**

The implementation must remain:

```text
Local
Offline-first
SQLite-backed
Stable
Simple
Maintainable
Dependency-light
Cross-platform friendly
AI-free
```
