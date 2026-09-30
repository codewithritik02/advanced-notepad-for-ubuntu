# PERSONAL NOTEPAD

# PHASE 6 — SEARCH

---

# 1. PHASE OBJECTIVE

Phase 6 ka objective Personal Notepad mein **real, persistent, fast, offline-first search system** implement karna hai.

Phase 5 ke baad application mein:

* Notes
* Note editing
* Autosave
* Notebooks
* Nested notebooks
* Tags
* Favorites
* Note metadata
* All Notes
* Unfiled
* Notebook navigation
* Tag navigation
* Favorites navigation

available hain.

Phase 6 mein user ko application ke andar existing notes ko quickly search karne ki capability milegi.

Search must work against the user's **local SQLite database**.

No internet, cloud service, external search API, or AI service is allowed.

---

# 2. PHASE 6 END STATE

Phase 6 ke baad:

```text
User types:
"project"

        ↓

Search engine

        ↓

Searches:
- Note title
- Note content

        ↓

Matching notes

        ↓

Results displayed in Notes List

        ↓

User selects result

        ↓

Exact note opens in editor
```

Example:

```text
Search:
project

Results:

Project Plan
"Planning the next project release..."

Release Notes
"Project release version 2.1..."

Meeting Notes
"We discussed the project timeline..."
```

Search must be:

```text
Local
Fast
Deterministic
Persistent-data based
Offline
Safe
Case-insensitive
Unicode-aware
```

---

# 3. IMPORTANT PRODUCT RULE

Search must search **real database data**.

Do NOT implement:

```text
search mock data
search only currently loaded notes
search only visible notes
search browser DOM
search only React state
```

The database is the source of truth.

---

# 4. PHASE 6 SEARCH SCOPE

The primary Phase 6 search scope is:

### Searchable

* Note title
* Note content

### Search metadata/filtering

* Notebook
* Tag
* Favorite
* Unfiled

These filters should integrate with the existing Phase 5 navigation model.

---

# 5. BASIC SEARCH BEHAVIOR

Input:

```text
project
```

Should match:

```text
Project Plan
project release
PROJECT discussion
```

Search should be case-insensitive.

---

# 6. SEARCH MUST NOT BE AI-BASED

Do NOT implement:

* AI semantic search
* embeddings
* vector database
* LLM search
* AI-generated queries
* "smart search"
* automatic intent detection
* AI ranking
* semantic similarity APIs

Search must remain deterministic and local.

---

# 7. SEARCH TECHNOLOGY DIRECTION

The preferred architecture is:

```text
React Search UI
        ↓
Search Service
        ↓
Tauri Command
        ↓
Rust Search Service
        ↓
SQLite Search Layer
        ↓
SQLite
```

For the initial implementation:

> Prefer SQLite-native search rather than loading every note into JavaScript.

---

# 8. SQLITE SEARCH STRATEGY

The coding agent must first inspect the existing SQLite schema and data volume.

Use the simplest architecture that provides good local performance.

Preferred options:

### Option A — SQLite FTS5

If available and compatible with the current SQLite environment:

```text
notes
   ↓
FTS5 index
   ↓
search
```

This is the preferred architecture for a real local note application because it scales better than repeatedly scanning large content strings.

### Option B — Parameterized LIKE search

If FTS5 integration would create unnecessary complexity or compatibility problems:

```sql
WHERE title LIKE ?
   OR content LIKE ?
```

can be used for the initial implementation.

However:

> Do not assume LIKE is the final architecture if the note database can become large.

The agent must inspect actual Phase 2 SQLite configuration before deciding.

---

# 9. IMPORTANT FTS5 RULE

If FTS5 is used:

Do not make the FTS table the authoritative source of note data.

The authoritative source remains:

```text
notes
```

FTS is an index.

Conceptually:

```text
notes
  │
  └── authoritative data
          │
          ▼
       FTS index
```

If the FTS index is ever rebuilt, notes must remain intact.

---

# 10. SEARCH INDEX CONTENT

If using FTS5, index at minimum:

```text
title
content
```

Do not index:

```text
attachments
raw file paths
database IDs
internal metadata
```

unless a later phase explicitly requires it.

---

# 11. DELETED NOTES

Deleted notes must never appear in normal search results.

Search must respect Phase 3 soft-delete behavior.

Conceptually:

```text
deleted = false
```

must be required.

If using FTS5, the query must join/filter against the authoritative `notes` table.

Do not rely solely on FTS content.

---

# 12. SEARCH RESULT MODEL

Create one canonical result type.

Conceptually:

```ts
type SearchResult = {
  noteId: string;
  title: string;
  snippet?: string;
  modifiedAt: string;
  notebookId: string | null;
  favorite: boolean;
};
```

If tags are needed for display:

```ts
tags?: Tag[];
```

Do not return the entire note content for every search result.

---

# 13. SEARCH RESULT CONTENT

Search results should normally return:

```text
id
title
snippet
modified_at
notebook_id
favorite
```

Not:

```text
entire huge note content
```

This is important for large notes.

---

# 14. SEARCH RESULT SNIPPET

A result may display a short matching-content preview.

Example:

```text
Project Plan

...finalize the project release timeline before Friday...
```

The snippet should:

* be short
* show relevant context
* not expose the entire note
* support Unicode
* avoid breaking layout

If SQLite FTS5 snippet/highlight functionality is available and appropriate, use it.

Otherwise generate a safe preview from the matching content.

---

# 15. SEARCH UI

The Phase 1 top search UI should become functional.

Example:

```text
┌────────────────────────────────────────────────────┐
│ 🔍 Search notes...                         Ctrl+K   │
└────────────────────────────────────────────────────┘
```

When typing:

```text
project
```

the application should display:

```text
Search results for "project"

12 notes
```

---

# 16. SEARCH LOCATION

Search should remain available from the existing top bar.

Do not create an unnecessary separate search application/window.

---

# 17. TASK BREAKDOWN

Complete these tasks one at a time.

Do not implement the whole search system in one uncontrolled change.

---

# TASK 1 — Inspect Existing Search UI

Inspect:

* Phase 1 search component
* top bar
* keyboard shortcuts
* existing navigation state
* note list
* note service
* Tauri bridge
* SQLite repository
* Phase 5 filters

Determine:

```text
Where does search state currently live?
How is note list state represented?
How are navigation contexts represented?
How are Tauri commands organized?
```

Do not immediately rewrite the UI.

---

# TASK 2 — Inspect SQLite Search Capabilities

Determine:

```text
SQLite version
FTS5 availability
current SQLite crate
existing migrations
database connection architecture
transaction architecture
```

Do not add a second database library.

Reuse the existing Phase 2 database implementation.

---

# TASK 3 — Choose Search Implementation

Based on the inspection, choose:

```text
FTS5
```

if it is practical and compatible.

Otherwise:

```text
parameterized SQLite search
```

for the initial implementation.

Document the decision.

Required report:

```text
SEARCH ENGINE DECISION:

Strategy:
- FTS5 / LIKE

Reason:
- ...

Expected limitations:
- ...

Future migration path:
- ...
```

---

# TASK 4 — Create Search Domain Types

Create a canonical search model.

Conceptually:

```ts
type SearchQuery = {
  text: string;
};

type SearchResult = {
  noteId: string;
  title: string;
  snippet?: string;
  modifiedAt: string;
  notebookId: string | null;
  favorite: boolean;
};
```

Adapt naming to the existing project conventions.

---

# TASK 5 — Create Search Backend Service

Create a dedicated Rust search service/repository.

Conceptually:

```text
SearchService
    ↓
SearchRepository
    ↓
SQLite
```

Do not put large SQL queries directly inside Tauri command handlers.

---

# TASK 6 — Create Search Tauri Command

Create a command conceptually equivalent to:

```text
search_notes
```

Input:

```json
{
  "query": "project"
}
```

Output:

```json
[
  {
    "noteId": "...",
    "title": "Project Plan",
    "snippet": "...project release...",
    "modifiedAt": "...",
    "notebookId": "...",
    "favorite": false
  }
]
```

Use the project's existing command naming conventions.

---

# TASK 7 — Empty Query Behavior

Search query:

```text
""
```

or:

```text
"   "
```

must not run an expensive database search.

Recommended behavior:

```text
empty query
    ↓
exit search mode
or
show default note list
```

The exact behavior should follow the existing UI architecture.

Do not return every note unnecessarily from the search endpoint.

---

# TASK 8 — Normalize Search Input

Frontend may trim:

```text
"   project   "
```

to:

```text
"project"
```

Backend should also validate/normalize.

Do not alter meaningful Unicode characters.

Do not aggressively normalize user text without a reason.

---

# TASK 9 — Implement Basic Title Search

Search must match title.

Examples:

```text
Title:
Project Plan

Query:
project
```

matches.

Case-insensitive matching is required.

---

# TASK 10 — Implement Content Search

Search must match note content.

Example:

```text
Title:
Meeting

Content:
Discuss the project release tomorrow.

Query:
project
```

must return the note.

---

# TASK 11 — Title + Content Search

A note must be returned if either:

```text
title matches
```

or:

```text
content matches
```

Do not require both.

---

# TASK 12 — Exact Note Selection

Clicking a search result must:

```text
select note
load note
open editor
preserve current navigation/search context appropriately
```

The selected note's exact:

* title
* content
* notebook
* tags
* favorite state

must load.

---

# TASK 13 — Search Result Click Behavior

When user clicks:

```text
Project Plan
```

the application should not create a copy.

It should open the existing note by ID.

Do not use title as the primary identifier.

Always use:

```text
noteId
```

---

# TASK 14 — Search Result List

Create or extend the existing note list so it can represent:

```text
normal note list
```

and:

```text
search result list
```

without duplicating the entire component.

Preferred:

```text
NoteList
    ↓
mode/location determines data source
```

rather than:

```text
NoteList
SearchNoteList
AnotherSearchList
```

with duplicated UI.

---

# TASK 15 — Search Result Empty State

For:

```text
xyzabcdef
```

show:

```text
No notes found

No notes match "xyzabcdef".
```

Do not show a blank list with no explanation.

---

# TASK 16 — Search Loading State

During search:

```text
Searching...
```

or an equivalent subtle loading indicator may appear.

Do not block the entire application UI.

---

# TASK 17 — Search Error State

If the database/search layer fails:

```text
Search failed

Unable to search notes right now.
Please try again.
```

Do not expose:

```text
SQLite error:
near "..."
```

to normal users.

Detailed errors can remain in development logs if appropriate.

---

# TASK 18 — Debounced Search

Do not execute a database query for every raw keystroke without control.

Implement a short debounce.

Recommended:

```text
150–300 ms
```

Example:

```text
p
pr
pro
proj
proje
projec
project
```

should result in a controlled number of search requests.

---

# TASK 19 — Do Not Debounce Search Submission Excessively

When the user presses:

```text
Enter
```

search should execute immediately.

Similarly, if the user explicitly selects a suggestion/result, do not wait for another debounce cycle.

---

# TASK 20 — Cancel Stale Search Results

Important.

Suppose user types:

```text
pro
```

then immediately:

```text
project
```

The `pro` request might return after the `project` request.

Do not allow stale results to overwrite newer results.

Use a request ID or cancellation mechanism.

Conceptually:

```text
request 1 = "pro"
request 2 = "project"

if request 1 returns after request 2:
    ignore request 1
```

---

# TASK 21 — Search State Model

Use explicit search state.

Example:

```ts
type SearchState =
  | { status: "idle" }
  | { status: "searching"; query: string }
  | { status: "success"; query: string; results: SearchResult[] }
  | { status: "error"; query: string; message: string };
```

Adapt to existing state architecture.

Avoid many unrelated booleans such as:

```text
isSearching
hasResults
hasError
isLoading
searchComplete
```

unless the project already follows that style.

---

# TASK 22 — Search Keyboard Shortcut

If Phase 1 established:

```text
Ctrl/Cmd + K
```

use it.

Behavior:

```text
Ctrl/Cmd + K
    ↓
focus search input
```

If another shortcut was already established, preserve it.

Do not break existing shortcuts.

---

# TASK 23 — Escape Search

Recommended:

```text
Escape
```

while search input is focused:

```text
clear/exit search
```

Use the least surprising behavior consistent with the existing navigation architecture.

Do not accidentally close the application window.

---

# TASK 24 — Search Input Focus

When search is activated:

```text
input.focus()
```

Cursor should be placed in the search field.

When search is cleared:

```text
focus behavior
```

should remain predictable.

---

# TASK 25 — Preserve Editor Focus

Search interactions must not unnecessarily steal editor focus.

Example:

```text
User editing note
→ opens search
→ searches
→ selects result
```

is fine.

But:

```text
autosave
→ search rerender
→ editor loses focus
```

must not happen.

---

# TASK 26 — Search Result Highlighting

Search results may highlight matching text.

Example:

```text
Project Plan

...project release timeline...
```

Possible:

```text
**project**
```

visual highlighting.

Requirements:

* do not inject unsafe HTML
* escape user text
* handle Unicode
* handle special characters
* do not break markup

If highlighting becomes unnecessarily complex, use plain snippets without highlighting.

Correctness is more important than visual decoration.

---

# TASK 27 — Search Snippet Safety

Never render raw note content as HTML.

Do not use:

```text
dangerouslySetInnerHTML
```

unless there is a very specific sanitized implementation.

Plain text rendering is preferred.

---

# TASK 28 — Search Result Ordering

Define deterministic ordering.

If using FTS5:

Use a reasonable relevance-based ranking.

If using LIKE:

Recommended:

```text
title matches first
then content matches
then modified_at DESC
```

But do not create an overly complicated ranking algorithm.

The exact ranking must be documented.

---

# TASK 29 — Basic Relevance Rules

Recommended initial behavior:

```text
Title exact/prefix match
    ↓
Title contains match
    ↓
Content contains match
    ↓
Modified time
```

If FTS5 BM25 ranking is used, use the database's ranking consistently instead of manually reproducing relevance.

Do not claim semantic relevance.

---

# TASK 30 — Search Result Count

Display:

```text
12 results
```

or:

```text
1 result
```

Use correct singular/plural wording.

Do not display misleading counts if pagination/limits are introduced.

---

# TASK 31 — Search Result Limit

Do not return an unlimited number of full result objects.

Recommended initial limit:

```text
50–100 results
```

depending on performance.

The backend should enforce a reasonable limit.

If a future phase needs pagination, the API can be extended.

Do not implement infinite scrolling unless necessary.

---

# TASK 32 — Search Result Pagination Strategy

For Phase 6:

> A simple bounded result set is sufficient.

Do not implement:

* infinite scrolling
* cursor pagination
* complex pagination UI

unless testing demonstrates the need.

---

# TASK 33 — Search Index Migration

If using FTS5, create a migration.

Do not modify existing migrations that have already been applied.

Example conceptual migration:

```text
migration:
create notes_fts
```

The actual implementation must match the existing migration system.

---

# TASK 34 — Initial FTS Population

If an FTS index is introduced after notes already exist:

```text
existing notes
    ↓
populate FTS index
```

All existing non-deleted notes must become searchable.

Do not create an empty FTS index and assume future notes are enough.

---

# TASK 35 — FTS Synchronization on Create

When a note is created:

```text
notes INSERT
    ↓
FTS updated
```

The search index must contain the new note.

---

# TASK 36 — FTS Synchronization on Update

When:

```text
title changes
```

or:

```text
content changes
```

the search index must update.

This is especially important because Phase 3 autosaves frequently.

Do not let the FTS index become stale after edits.

---

# TASK 37 — FTS Synchronization on Delete

When a note is soft-deleted:

```text
deleted = true
```

search must stop returning it.

If using an FTS index that retains deleted content, filter it through the main `notes` table.

Optionally remove deleted notes from the index if the architecture makes that safe.

Do not permanently erase note data as part of search indexing.

---

# TASK 38 — FTS Synchronization on Restore

If a future phase restores notes:

The search architecture should allow the restored note to become searchable again.

Do not implement Trash UI now.

Design the search index so restoration will not require a complete rewrite.

---

# TASK 39 — Search + Favorites

When the user searches:

```text
project
```

then selects:

```text
Favorites
```

the search result behavior should be predictable.

Recommended model:

```text
search query + current navigation filter
```

can work together.

Example:

```text
Favorites
+
search: project
```

means:

```text
favorite = true
AND
title/content matches "project"
```

This should be implemented only if the existing navigation architecture can support it cleanly.

If it would require a large redesign, keep Phase 6 search global and clearly reset/exit the navigation filter when search begins.

Do not create an inconsistent half-working filtering system.

---

# TASK 40 — Search + Tags

Similarly:

```text
#work
+
search: project
```

can conceptually mean:

```text
tag = work
AND
title/content matches project
```

But:

> Do not implement advanced filter combinations unless the architecture supports them cleanly.

At minimum, tag search and text search must not corrupt one another's state.

---

# TASK 41 — Search + Notebook

If the existing Phase 4/5 navigation architecture supports filter composition:

```text
Notebook = Work
Search = project
```

can be supported.

Otherwise search should remain global.

Do not introduce complicated query-builder logic in Phase 6.

---

# TASK 42 — Search Navigation Model

Extend the existing `NoteLocation`/navigation model only if necessary.

Possible conceptual model:

```ts
type NoteLocation =
  | { type: "all" }
  | { type: "unfiled" }
  | { type: "favorites" }
  | { type: "notebook"; notebookId: string }
  | { type: "tag"; tagId: string };
```

Search state should remain separate:

```ts
searchQuery: string
```

Do not create:

```text
tagSearchLocation
favoriteSearchLocation
notebookSearchLocation
```

as separate unrelated systems.

---

# TASK 43 — Search Clear Behavior

When user clears the search field:

```text
query = ""
```

the UI should return to the appropriate previous note list.

Example:

```text
Before search:
Work / Projects

Search:
project

Clear search:
Work / Projects
```

If the chosen architecture treats search as a global mode, return to All Notes.

Choose one consistent behavior and document it.

---

# TASK 44 — Search Result Selection + Clear

When a result is selected:

Possible behavior:

```text
Search remains active
```

or:

```text
Search closes
```

Recommended for desktop notes:

> Keep the search query visible while the selected note opens, unless the current UI becomes too crowded.

This allows users to move between search results without retyping.

However, if the existing three-pane design cannot comfortably support this, closing search after selection is acceptable.

Choose one behavior and keep it consistent.

---

# TASK 45 — Search Result Keyboard Navigation

Support:

```text
Arrow Down
Arrow Up
Enter
Escape
```

when search results are active.

Expected:

```text
Arrow Down
    ↓
next result

Arrow Up
    ↓
previous result

Enter
    ↓
open selected result

Escape
    ↓
exit/clear search
```

Do not interfere with editor keyboard behavior when the editor has focus.

---

# TASK 46 — Search Result Accessibility

Each result should have:

* keyboard focus
* accessible name
* selected state
* clear title
* optional snippet
* favorite indicator with accessible label

If using a listbox-style pattern, follow proper ARIA semantics.

Do not add ARIA roles incorrectly.

---

# TASK 47 — Search Input Accessibility

Search input should have:

```text
aria-label="Search notes"
```

or visible label equivalent.

Placeholder:

```text
Search notes...
```

Do not rely solely on placeholder text for accessibility.

---

# TASK 48 — Unicode Search

Test:

```text
भारत
यात्रा
हिंदी
東京
café
résumé
```

Create notes containing these strings.

Search exact Unicode terms.

Expected:

```text
correct matches
```

Do not assume ASCII-only behavior.

---

# TASK 49 — Case-Insensitive Search

Test:

```text
Project
project
PROJECT
PrOjEcT
```

All should behave according to the selected SQLite search strategy.

Document any unavoidable SQLite/FTS Unicode case-folding limitations.

Do not falsely claim universal Unicode case folding if the underlying engine does not provide it.

---

# TASK 50 — Special Characters

Test:

```text
C++
C#
R&D
project-x
hello.world
foo/bar
user@example.com
```

Search must not:

* crash
* produce malformed SQL
* interpret arbitrary input as SQL
* expose internal database errors

Parameterized queries are mandatory.

---

# TASK 51 — SQL Injection Testing

Try search inputs such as:

```text
'
"
' OR 1=1 --
" OR "1"="1
```

Expected:

```text
no crash
no database corruption
no unintended full database dump
```

Search input must always be parameterized.

---

# TASK 52 — Large Content Search

Create a large note.

Include:

```text
project
```

near:

```text
beginning
middle
end
```

Search for:

```text
project
```

Verify:

* result found
* correct note returned
* snippet is reasonable
* UI remains responsive
* full content is not unnecessarily returned

---

# TASK 53 — Many Notes Search

Create enough notes to simulate realistic usage.

At minimum test:

```text
100 notes
500 notes
1000 notes
```

if practical.

Measure:

* search response
* UI responsiveness
* memory use
* result rendering

Do not optimize based solely on theoretical concerns.

---

# TASK 54 — Search During Autosave

Important regression test.

Workflow:

```text
Open note
↓
Type:
project deadline
↓
Immediately search:
deadline
```

Search should eventually find the latest saved content.

If the editor has a pending autosave:

```text
flush/save before search
```

or otherwise ensure search consistency.

Do not allow search to permanently miss recent content because the FTS index is stale.

---

# TASK 55 — Autosave + FTS Ordering

If Phase 3 autosave works like:

```text
React state
   ↓
debounce
   ↓
SQLite UPDATE
```

then search indexing must happen atomically or in a controlled sequence:

```text
content update
    ↓
SQLite note update
    ↓
FTS update
```

Prefer a transaction when practical.

Do not expose a state where the database says:

```text
new content
```

but the FTS index permanently contains:

```text
old content
```

---

# TASK 56 — Search During Note Switching

Test:

```text
Note A open
→ edit
→ search
→ select Note B
```

Verify:

* Note A saves correctly
* Note B opens correctly
* search result belongs to B
* no stale content appears

---

# TASK 57 — Search Result Metadata

Results should optionally show:

```text
favorite
modified time
notebook
tags
```

Do not display everything if it makes the list too dense.

At minimum:

```text
title
snippet
modified time
```

should be sufficient.

---

# TASK 58 — Search Result Favorite Indicator

If a result is favorite:

```text
★
```

can be displayed.

Accessibility:

```text
Favorite note
```

Do not make the result icon the only way to understand the state.

---

# TASK 59 — Search Result Notebook Context

If practical, show:

```text
Work / Projects
```

under the title.

This helps distinguish similar notes.

Do not show internal notebook IDs.

---

# TASK 60 — Search Result Tags

Tags may be shown compactly:

```text
[work] [important]
```

Do not load tags using one SQL query per result.

Avoid N+1 queries.

If tags are displayed, use an efficient batch query or appropriate joined result.

---

# TASK 61 — Avoid N+1 Search Queries

Do not implement:

```text
search
→ result 1 → query tags
→ result 2 → query tags
→ result 3 → query tags
→ ...
```

For 100 results this becomes:

```text
101 database queries
```

Instead:

```text
search
+
batch metadata
```

or simply omit tags from search results.

Correctness and simplicity are more important than adding metadata.

---

# TASK 62 — Search Result Limit Enforcement

Backend must enforce the maximum result count.

Do not trust frontend:

```text
results.slice(0, 50)
```

as the only protection.

Database query should limit rows.

---

# TASK 63 — Search Performance Logging

During development, optionally measure:

```text
query
result count
execution time
```

Do not log note content.

Example:

```text
Search query executed:
count=12
duration=8ms
```

Remove noisy debug logging before release.

---

# TASK 64 — Search Index Integrity

If FTS5 is used, test:

```text
create note
search
edit note
search old text
search new text
delete note
search deleted text
restart
search
```

Expected:

```text
new text found
old text not found
deleted note excluded
restart still works
```

---

# TASK 65 — Search Rebuild Capability

If FTS5 is introduced, create a backend-level way to rebuild the index internally.

This does NOT need a user-facing button in Phase 6.

Conceptually:

```text
rebuild_search_index()
```

Useful for:

* migrations
* corruption recovery
* future maintenance

Do not expose this as a normal user feature yet.

---

# TASK 66 — Search Index Recovery

If the FTS index is missing or inconsistent:

The application should have a safe recovery strategy.

Possible:

```text
detect missing index
→ recreate
→ populate from notes
```

Do not delete notes because search index creation failed.

---

# TASK 67 — Search Migration Failure Safety

If search migration fails:

```text
database remains usable
```

must be the priority.

Do not corrupt the notes database.

Follow the existing migration transaction architecture.

---

# TASK 68 — Search Error Boundary

Search UI errors should not crash:

```text
sidebar
note list
editor
```

A search failure must be isolated.

The user should still be able to browse existing notes.

---

# TASK 69 — Search With No Notes

Fresh application:

```text
0 notes
```

Search:

```text
project
```

Expected:

```text
No notes found
```

No crash.

---

# TASK 70 — Search With One Note

Create:

```text
Project Plan
```

Search:

```text
project
```

Expected:

```text
1 result
```

---

# TASK 71 — Search Exact Phrase

If using FTS5, define phrase behavior clearly.

Example:

```text
"project plan"
```

must not accidentally mean:

```text
project OR plan
```

if the UI claims to support phrase search.

For Phase 6 MVP:

> Do not advertise advanced phrase/query syntax unless explicitly implemented.

Basic text search is sufficient.

---

# TASK 72 — Search Query Syntax

Do not expose advanced syntax such as:

```text
title:
tag:
notebook:
before:
after:
```

in Phase 6 unless deliberately implemented.

Those belong to a future advanced search design.

---

# TASK 73 — Search Special Query Handling

If FTS5 is used, raw FTS syntax can cause errors.

Example:

```text
project OR plan
```

or:

```text
"project"
```

The search layer must decide whether user input is:

```text
plain user text
```

or:

```text
FTS query syntax
```

Recommended Phase 6:

> Treat the search field as normal user text, not raw FTS query syntax.

Escape/transform input as necessary for safe FTS searching.

Do not expose raw FTS operators accidentally.

---

# TASK 74 — Search Ranking Verification

Create:

```text
Note A:
title = Project

Note B:
title = Meeting
content = project

Note C:
title = Project Plan
content = project
```

Verify the ranking follows the documented search strategy.

Do not call a result "best" in product copy.

Use neutral relevance behavior.

---

# TASK 75 — Search Result Stability

Repeated identical searches should produce deterministic ordering.

Example:

```text
project
```

run five times.

Expected:

```text
same ordering
```

unless note modification times changed.

---

# TASK 76 — Search and Theme

Verify search UI in:

```text
Light
Dark
System
```

Ensure:

* input readable
* result text readable
* selected result visible
* highlight visible
* empty state readable
* loading state readable

Use existing theme tokens.

Do not introduce independent hard-coded theme colors.

---

# TASK 77 — Search and Responsive Layout

Verify:

```text
normal desktop width
narrow desktop width
large window
```

Search must not:

* overlap sidebar
* hide editor controls
* overflow horizontally
* break note list

---

# TASK 78 — Search and Existing Three-Pane Layout

Preserve:

```text
Sidebar
Notes List
Editor
```

Search should integrate with the existing architecture.

Do not replace the entire three-pane UI with a search page.

---

# TASK 79 — Search and Navigation State

Verify:

```text
All Notes
Unfiled
Favorites
Notebook
Tag
```

remain functional after entering and leaving search.

Search must not permanently corrupt the selected navigation context.

---

# TASK 80 — Search and Selection State

If current note is:

```text
Note A
```

and search result:

```text
Note B
```

is selected:

```text
selectedNoteId = B
```

must update consistently.

Do not leave:

```text
search result says B
editor still shows A
```

---

# TASK 81 — Search and Unsaved Changes

Before changing from Note A to a search result Note B:

```text
pending autosave
```

must be handled using the Phase 3 save/flush mechanism.

Do not lose edits.

---

# TASK 82 — Search Clear After Note Selection

Test the chosen behavior.

Example:

```text
search:
project

select:
Project Plan

clear search
```

Expected:

```text
predictable note list
```

Document the chosen behavior.

---

# TASK 83 — Search Input State After Restart

Search query should normally NOT persist across application restarts.

On startup:

```text
searchQuery = ""
```

This keeps the app simple.

Do not add persistent search history in Phase 6.

---

# TASK 84 — No Search History

Do not implement:

* recent searches
* search history
* saved searches
* suggestions based on past searches

These are outside Phase 6.

---

# TASK 85 — No Search Suggestions

Do not implement:

```text
AI suggestions
recent queries
people suggestions
automatic tags
```

The search box is a simple text search interface.

---

# TASK 86 — Search Data Privacy

Search queries and note content remain local.

Do not send:

```text
search query
note title
note content
```

to external services.

Do not add telemetry for search content.

---

# TASK 87 — Search Logging Privacy

Never log:

```text
full note content
```

Never log:

```text
complete search query
```

in production if it could reveal sensitive user information.

Development logging should be minimal.

---

# TASK 88 — Backend Validation

Backend must validate:

* query length
* malformed input
* database availability
* result limit

Use a reasonable maximum query length to avoid pathological requests.

Do not use an unnecessarily small limit.

---

# TASK 89 — Search Query Size

A practical example:

```text
query <= 1000 characters
```

may be reasonable.

The exact value should follow the application's existing validation philosophy.

If query is too large:

```text
Search query is too long.
```

Do not crash.

---

# TASK 90 — Search Database Error Handling

If SQLite returns an error:

Backend:

```text
SearchError
```

Frontend:

```text
Unable to search notes.
```

Do not expose SQL implementation details.

---

# TASK 91 — Search Service Separation

Keep:

```text
SearchService
```

separate from:

```text
NoteService
```

where practical.

Do not put the entire search engine inside:

```text
NoteList.tsx
```

---

# TASK 92 — Search Query Repository

SQL should live in the Rust storage/repository layer.

Not inside:

```text
React
Tauri command handler
```

Recommended:

```text
Tauri Command
    ↓
Search Service
    ↓
Search Repository
    ↓
SQLite
```

---

# TASK 93 — Search Result DTO Mapping

Do not expose raw database rows directly to React if the project uses DTO mapping.

Use:

```text
SQLite row
    ↓
Rust domain model
    ↓
DTO
    ↓
TypeScript model
```

Maintain the existing architecture.

---

# TASK 94 — Search Tests

Add backend tests for:

```text
title match
content match
case-insensitive search
empty query
no result
multiple results
deleted note
special characters
Unicode
large content
ranking
result limit
```

---

# TASK 95 — FTS Tests

If FTS5 is used, additionally test:

```text
index creation
index population
create synchronization
update synchronization
soft-delete behavior
rebuild
restart
```

---

# TASK 96 — Frontend Search Tests

Test:

```text
input rendering
debounce
loading state
success state
empty state
error state
result selection
keyboard navigation
Escape
Ctrl/Cmd + K
stale request protection
```

Use existing frontend test infrastructure.

Do not add unnecessary dependencies.

---

# TASK 97 — Search Regression Tests

Verify Phase 1–5 still work:

```text
theme
sidebar
notebooks
nested notebooks
note creation
note editing
autosave
undo/redo
tags
favorites
metadata
```

Search must not break them.

---

# TASK 98 — Build Verification

Run the project's actual commands for:

```text
TypeScript
Rust
Tests
Production build
Tauri build/check
```

Do not invent commands.

Inspect:

```text
package.json
Cargo.toml
```

first.

---

# TASK 99 — Final Manual Search QA

Perform:

```text
Create 10 notes.

Use different titles and contents.

Search:
project

Verify:
all expected notes appear.

Search:
PROJECT

Verify:
same logical matches.

Search:
nonexistent-term

Verify:
No notes found.

Edit a matching note.

Search new content.

Verify:
new content appears.

Soft-delete a matching note.

Search again.

Verify:
deleted note does not appear.

Restart.

Search again.

Verify:
results still work.
```

---

# TASK 100 — Final Phase Cleanup

Remove:

* mock search data
* temporary logging
* unused imports
* duplicate search types
* duplicate search services
* unnecessary dependencies
* debug UI
* dead code

Do not perform unrelated refactoring.

---

# 17. SEARCH API CONTRACT

The exact command names should follow the project's existing convention.

Conceptually:

## Search Notes

```text
search_notes
```

Input:

```json
{
  "query": "project",
  "limit": 50
}
```

Output:

```json
{
  "query": "project",
  "results": [
    {
      "noteId": "note-123",
      "title": "Project Plan",
      "snippet": "...project release...",
      "modifiedAt": "2026-09-30T09:30:00Z",
      "notebookId": "notebook-123",
      "favorite": true
    }
  ],
  "total": 1
}
```

If the application does not need a `total` field, do not add it unnecessarily.

---

# 18. SEARCH SERVICE CONTRACT

Conceptually:

```ts
searchService.search({
  query,
  limit
});
```

Return:

```ts
SearchResult[];
```

Do not let components call Tauri directly.

---

# 19. SEARCH QUERY FLOW

The complete flow should be:

```text
User types
    ↓
SearchInput
    ↓
debounce
    ↓
searchService.search()
    ↓
Tauri invoke
    ↓
search_notes
    ↓
SearchService
    ↓
SearchRepository
    ↓
SQLite / FTS5
    ↓
SearchResult DTO
    ↓
React state
    ↓
Search result list
```

---

# 20. FTS5 ARCHITECTURE — IF USED

Conceptually:

```text
notes
├── id
├── title
├── content
├── deleted
└── ...

notes_fts
├── title
├── content
└── note_id/reference
```

The exact FTS schema depends on the selected synchronization strategy.

Possible architecture:

```text
External content FTS5
```

or:

```text
Contentless FTS5
```

The coding agent must choose the simplest reliable strategy compatible with the existing database architecture.

---

# 21. FTS SYNCHRONIZATION STRATEGY

If using triggers:

```text
notes INSERT
    ↓
FTS insert

notes UPDATE
    ↓
FTS update

notes DELETE
    ↓
FTS delete
```

If the application already uses repository-level synchronization instead:

```text
note repository
    ↓
transaction
    ↓
notes update
    ↓
FTS update
```

Choose one authoritative strategy.

Do not simultaneously create:

```text
triggers
+
manual indexing
```

unless there is a documented reason.

---

# 22. SEARCH TRANSACTION RULE

For note content updates:

Preferred:

```text
BEGIN TRANSACTION

UPDATE notes
UPDATE search index

COMMIT
```

This avoids permanent divergence.

If SQLite/FTS architecture makes this unnecessary or handles it through triggers, reuse that mechanism.

---

# 23. SEARCH RESULT SNIPPET RULE

Snippet must be treated as:

```text
plain text
```

Never as trusted HTML.

Example:

```text
...project release is scheduled for Friday...
```

not:

```html
...<mark>project</mark> release...
```

unless highlighting is implemented safely.

---

# 24. SEARCH RESULT UI

Recommended:

```text
┌────────────────────────────────────────────┐
│ Search results for "project"               │
│ 12 results                                 │
├────────────────────────────────────────────┤
│ ★ Project Plan                             │
│   Work / Projects                          │
│   ...project release timeline...           │
│   2 minutes ago                            │
├────────────────────────────────────────────┤
│ Release Notes                              │
│   Work                                     │
│   ...project release version...            │
│   Yesterday                                │
└────────────────────────────────────────────┘
```

Follow existing design tokens.

---

# 25. SEARCH EMPTY STATE

For no query:

```text
Search notes...
```

For no results:

```text
No notes found

Try a different search term.
```

For search error:

```text
Search unavailable

Please try again.
```

---

# 26. SEARCH PERFORMANCE TARGET

For normal local usage:

```text
small database:
near-immediate

medium database:
responsive

large database:
search should remain usable
```

Do not promise a specific millisecond number unless measured.

The target is:

> Search must feel immediate for normal desktop note usage.

---

# 27. SEARCH MEMORY RULE

Do not load all note contents into React.

Bad:

```text
SELECT * FROM notes
↓
send all notes to frontend
↓
filter in JavaScript
```

Good:

```text
search query
↓
SQLite
↓
only matching results
↓
frontend
```

This is especially important for large notes.

---

# 28. SEARCH SECURITY RULES

Always use:

```text
parameterized SQL
```

Never:

```text
string concatenation
```

Validate:

```text
query
limit
IDs
```

Do not allow search input to control arbitrary SQL.

---

# 29. SEARCH INDEX SECURITY

FTS index must never become a second uncontrolled source of truth.

If:

```text
FTS index corrupted
```

the notes database must remain recoverable.

The application should be able to rebuild the index from:

```text
notes
```

---

# 30. NO CLOUD SEARCH

Do not implement:

```text
Google search
Algolia
Elastic Cloud
Supabase
Firebase
remote API
```

Search must work with:

```text
internet = OFF
```

---

# 31. NO EMBEDDINGS

Do not introduce:

```text
vector database
embeddings
semantic vectors
AI ranking
```

These are outside the product scope.

---

# 32. NO SEARCH ANALYTICS

Do not add:

```text
search analytics
query tracking
user behavior tracking
```

Personal Notepad is offline-first.

---

# 33. NO SEARCH HISTORY

Do not save:

```text
recent searches
```

in SQLite during Phase 6.

---

# 34. NO SEARCH SUGGESTIONS

Do not implement:

```text
suggested query
AI query completion
recent query dropdown
```

The search field should remain predictable and simple.

---

# 35. SEARCH + NOTE FORMAT

Search both:

```text
txt
md
```

notes.

Format must not prevent search.

For Markdown:

```text
# Project Plan
```

searching:

```text
project
```

must find the note.

Do not strip Markdown content from the searchable database.

---

# 36. SEARCH + ATTACHMENTS

Attachments are not searchable in Phase 6.

Do not search:

```text
PDF contents
image OCR
document attachments
file names
```

Attachment functionality belongs to Phase 7.

---

# 37. SEARCH + TRASH

Trash UI belongs to Phase 8.

Phase 6 only needs to respect the existing soft-delete flag.

Do not create:

```text
Trash search
Restore search
Permanent deletion
```

features.

---

# 38. SEARCH + INTERNAL LINKS

Do not implement:

```text
backlink search
link graph
internal-link indexing
```

Phase 9 handles internal links.

---

# 39. SEARCH + IMPORT/EXPORT

Do not implement import/export in Phase 6.

Search must work on existing local notes only.

---

# 40. SEARCH + BACKUP

Do not implement backup.

Search index should be treated as rebuildable derived data if FTS5 is used.

This becomes important for Phase 11 backup architecture.

---

# 41. SEARCH + ENCRYPTION FUTURE COMPATIBILITY

Do not implement encryption now.

However, keep search architecture isolated.

Future encryption may affect:

```text
database search
```

and therefore Phase 12 may require a different search strategy.

Do not prematurely implement encryption-compatible search.

---

# 42. SEARCH + FUTURE SYNC

Do not implement sync.

However:

```text
SearchService
```

should not assume that every future data source is SQLite forever.

Keep:

```text
SearchService
```

as an abstraction over the local storage layer.

This helps future Windows/sync work.

---

# 43. CROSS-PLATFORM RULE

Do not add Ubuntu-specific search code.

Search should remain:

```text
React
+
Rust
+
SQLite
```

and therefore reusable on:

```text
Ubuntu
Windows
```

later.

---

# 44. DEPENDENCY RULES

Do not add a large frontend search library unless clearly justified.

Avoid:

```text
heavy client-side fuzzy search
```

because the source of truth is already SQLite.

Prefer database-native search.

---

# 45. UI STATE RULE

Search state should not be persisted in the database.

Do not create:

```text
settings.searchQuery
```

for normal runtime search.

Search query is transient UI state.

---

# 46. SEARCH ERROR RECOVERY

If search fails:

```text
error
↓
show message
↓
user changes query / retries
↓
search works again
```

The entire application should remain usable.

---

# 47. SEARCH RESULT STALE DATA

A search result may become stale if the note changes after the query.

When opening the result:

```text
load current note by ID
```

Do not rely on the result's title/content as the authoritative note.

---

# 48. SEARCH RESULT NOTE ID

Every result must contain:

```text
noteId
```

This is mandatory.

Do not use:

```text
title
```

as an identifier.

Two notes can have the same title.

---

# 49. DUPLICATE TITLE TEST

Create:

```text
Meeting
```

and:

```text
Meeting
```

with different content.

Search:

```text
Meeting
```

Both must appear.

Selecting either must open the correct note.

---

# 50. DUPLICATE CONTENT TEST

Create two notes with identical content but different IDs.

Search should return both.

Do not deduplicate based on content.

---

# 51. SEARCH RESULT LIMIT TEST

Create more notes than the configured limit.

Verify:

```text
backend result limit
```

is respected.

Do not silently render thousands of results.

---

# 52. SEARCH RESULT ORDER TEST

Run the same search repeatedly.

Expected:

```text
stable order
```

unless data changes.

---

# 53. SEARCH AFTER TAG RENAME

Rename a tag.

Search note content.

Verify search remains unaffected.

Tags are metadata and should not corrupt note text search.

---

# 54. SEARCH AFTER NOTEBOOK MOVE

Move note:

```text
Personal
→
Work / Projects
```

Search by title/content.

The note must still be found.

Notebook movement must not break search indexing.

---

# 55. SEARCH AFTER FAVORITE TOGGLE

Favorite/unfavorite a note.

Search title/content.

The note must remain searchable.

Favorite state is metadata.

---

# 56. SEARCH AFTER NOTE TITLE CHANGE

Important.

Before:

```text
Old Project
```

Search:

```text
Old
```

must find it.

Rename:

```text
New Project
```

Then:

```text
Old
```

should no longer match because of the old title.

Search:

```text
New
```

must match.

This verifies index synchronization.

---

# 57. SEARCH AFTER CONTENT CHANGE

Before:

```text
Discuss Alpha
```

Search:

```text
Alpha
```

finds note.

Change content:

```text
Discuss Beta
```

Now:

```text
Alpha
```

should not match based solely on old content.

```text
Beta
```

must match.

---

# 58. SEARCH AFTER SOFT DELETE

Before:

```text
Search:
project
```

finds note.

Soft-delete note.

Search again.

Expected:

```text
note absent
```

Do not permanently delete the note.

---

# 59. SEARCH AFTER RESTART

Workflow:

```text
Create note
Edit note
Close application
Reopen
Search
```

Expected:

```text
note found
```

This is a mandatory Phase 6 acceptance test.

---

# 60. SEARCH WITH OFFLINE MODE

Disable network.

Launch application.

Search:

```text
project
```

Expected:

```text
search works normally
```

---

# 61. SEARCH WITH SPECIAL UNICODE CONTENT

Create:

```text
Title:
योजना

Content:
यह project की योजना है।
```

Search:

```text
योजना
```

and:

```text
project
```

Verify appropriate matches according to the selected SQLite search behavior.

Document engine limitations where applicable.

---

# 62. SEARCH UI PERFORMANCE

Typing into the search box must remain responsive.

Do not:

```text
block React render
```

while a database request is running.

Search should be asynchronous.

---

# 63. SEARCH REQUEST CONTROL

Recommended frontend logic:

```text
input changes
    ↓
debounce
    ↓
increment request ID
    ↓
execute search
    ↓
if request ID is current:
    apply results
else:
    ignore results
```

This is sufficient for local desktop search.

---

# 64. SEARCH LOADING INDICATOR

Do not show a full-screen spinner.

Use:

```text
small spinner
```

or:

```text
Searching...
```

inside the search/results area.

---

# 65. SEARCH RESULT TRANSITIONS

Do not use heavy animations.

Search results should update quickly.

Avoid animation that causes:

```text
layout jumping
```

---

# 66. SEARCH FOCUS REGRESSION

Test:

```text
Ctrl/Cmd + K
→ type
→ Enter
→ open result
→ editor
```

Then:

```text
Ctrl/Cmd + K
```

again.

Search must regain focus correctly.

---

# 67. SEARCH ESCAPE REGRESSION

Test:

```text
Ctrl/Cmd + K
→ type
→ Escape
```

Expected:

```text
search exits/clears according to chosen behavior
```

Application must not close.

---

# 68. SEARCH ENTER REGRESSION

When result is highlighted:

```text
Enter
```

must open it.

When no result is highlighted:

```text
Enter
```

must not create a new note or perform an unrelated action.

---

# 69. SEARCH ARROW REGRESSION

Arrow keys should navigate search results only when search results have focus.

When editor has focus:

```text
Arrow keys
```

must remain normal editor cursor controls.

---

# 70. SEARCH AND GLOBAL NEW NOTE

Existing:

```text
Ctrl/Cmd + N
```

must continue creating a new note.

Search must not hijack it.

---

# 71. SEARCH AND SAVE

Existing:

```text
Ctrl/Cmd + S
```

must continue working in editor.

Search must not hijack it when editor has focus.

---

# 72. SEARCH AND UNDO/REDO

Existing:

```text
Ctrl/Cmd + Z
Ctrl/Cmd + Shift + Z
```

must continue working in editor.

Search UI must not interfere.

---

# 73. SEARCH RESULT METADATA LOADING

If the search result needs:

```text
notebook path
tags
```

do not block the entire search result list waiting for every metadata query.

Prefer:

```text
search result core data
```

first.

Additional metadata can be loaded efficiently if needed.

---

# 74. SEARCH LIST VIRTUALIZATION

Do not add virtualization in Phase 6 unless testing shows it is necessary.

With a bounded result limit:

```text
50–100
```

normal React rendering should be sufficient.

---

# 75. SEARCH INDEX REBUILD TEST

If FTS5:

```text
Create notes
↓
Build index
↓
Verify search
↓
Rebuild index
↓
Verify search
```

Results must remain correct.

---

# 76. SEARCH DATABASE BACKUP COMPATIBILITY

If FTS is a derived index:

Document:

```text
FTS index can be rebuilt from notes
```

This will help Phase 11 backup/recovery design.

Do not implement backup now.

---

# 77. SEARCH MIGRATION DOCUMENTATION

Document:

```text
Search engine:
FTS5 / LIKE

Search fields:
title
content

Deleted behavior:
excluded

Ranking:
...

Result limit:
...

Query normalization:
...

Index rebuild:
...
```

This is required for future maintenance.

---

# 78. FINAL SEARCH UI STATE

The final UI should support:

```text
Normal mode:

All Notes
Unfiled
Favorites
Notebooks
Tags


Search mode:

Search: project

Results:
Project Plan
Release Notes
Meeting Notes
...
```

---

# 79. FINAL ARCHITECTURE

After Phase 6:

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
        ┌───────────┼────────────┐          │
        ▼           ▼            ▼          ▼
      Notes      Navigation    Search     Storage
        │           │            │          │
        │           │            │          │
        └───────────┼────────────┼──────────┘
                    │            │
                    ▼            ▼
                  SQLite       Search Index
                    │            │
              ┌─────┼─────┐      │
              ▼     ▼     ▼      ▼
            notes notebooks tags FTS5
                    │
                 note_tags
```

---

# 80. PHASE 6 SUCCESS DEFINITION

Phase 6 is successful when this statement is true:

> **A user can type a search query and quickly find existing local notes by title or content, open the exact matching note, and continue using notebooks, tags, favorites, and editing without losing data or breaking existing functionality.**

Search must remain:

```text
Offline-first
SQLite-backed
Fast
Deterministic
Safe
Unicode-aware
AI-free
Cloud-free
Cross-platform friendly
```

---

# 81. FEATURES STILL NOT IMPLEMENTED

The following remain outside Phase 6:

```text
Attachments
Attachment content search
OCR
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
Windows packaging
Ubuntu packaging
AI
Semantic search
Vector search
Search history
Saved searches
Advanced query language
```

---

# 82. REQUIRED PER-TASK REPORT FORMAT

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

SEARCH ENGINE:
- FTS5 / LIKE / Existing
- Reason: ...

VERIFICATION:
- TypeScript: PASS/FAIL
- Rust: PASS/FAIL
- Database migration: PASS/FAIL/N/A
- Search tests: PASS/FAIL/N/A
- Frontend tests: PASS/FAIL/N/A
- Build: PASS/FAIL
- Application launch: PASS/FAIL
- Manual verification: PASS/FAIL/N/A

NOT IMPLEMENTED YET:
- ...

NOTES FOR NEXT TASK:
- ...
```

Never claim:

```text
PASS
```

without actually performing the relevant verification.

---

# 83. FINAL PHASE REPORT

After all Phase 6 tasks are complete:

```text
PHASE 6 COMPLETE

SEARCH:
- Local note search implemented
- Title search implemented
- Content search implemented
- Case-insensitive behavior verified
- Unicode behavior verified
- Search result snippets implemented
- Search result selection implemented
- Search result limit implemented
- Debounced search implemented
- Stale request protection implemented
- Search loading state implemented
- Search empty state implemented
- Search error state implemented
- Keyboard navigation implemented

DATABASE:
- SQLite-backed search
- FTS5 / LIKE strategy documented
- Search index migration: PASS/FAIL/N/A
- Existing notes indexed: PASS/FAIL/N/A
- Search synchronization verified

NOTE INTEGRATION:
- Note creation searchable
- Note editing searchable
- Title changes searchable
- Content changes searchable
- Autosave interaction verified
- Soft-deleted notes excluded
- Restart recovery verified

PHASE 5 INTEGRATION:
- Tags preserved
- Favorites preserved
- Notebook filtering preserved
- Metadata preserved

EDITOR:
- Autosave preserved
- Undo/redo preserved
- Cursor preservation preserved

UI:
- Search bar functional
- Three-pane layout preserved
- Light theme preserved
- Dark theme preserved
- System theme preserved

TESTING:
- TypeScript: PASS/FAIL
- Rust: PASS/FAIL
- Search tests: PASS/FAIL
- Frontend tests: PASS/FAIL/N/A
- Build: PASS/FAIL
- Application launch: PASS/FAIL
- Manual QA: PASS/FAIL

NOT IMPLEMENTED:
- Attachments
- OCR
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
- Semantic search

READY FOR:
PHASE 7 — ATTACHMENTS & FILES
```

---

# 84. FINAL PHASE 6 CHECKLIST

Before declaring Phase 6 complete:

```text
[ ] Search UI is real, not mock.
[ ] Search uses SQLite.
[ ] Search does not load all note content into React.
[ ] Title search works.
[ ] Content search works.
[ ] Deleted notes are excluded.
[ ] Search result IDs are stable note IDs.
[ ] Clicking a result opens the correct note.
[ ] Duplicate note titles work correctly.
[ ] Unicode search tested.
[ ] Special characters tested.
[ ] SQL injection inputs tested.
[ ] Empty query handled.
[ ] No-result state implemented.
[ ] Loading state implemented.
[ ] Error state implemented.
[ ] Search is debounced.
[ ] Stale results cannot overwrite newer results.
[ ] Keyboard navigation works.
[ ] Ctrl/Cmd + K works.
[ ] Escape behavior works.
[ ] Existing editor shortcuts still work.
[ ] Autosave still works.
[ ] Search reflects edited title/content.
[ ] Search survives application restart.
[ ] FTS index is synchronized if FTS5 is used.
[ ] FTS index can be rebuilt if applicable.
[ ] No cloud service introduced.
[ ] No AI introduced.
[ ] No unnecessary dependencies introduced.
[ ] Phase 1–5 functionality still works.
[ ] TypeScript verified.
[ ] Rust verified.
[ ] Tests verified.
[ ] Production build verified.
[ ] Ubuntu application launch verified.
```

# END OF PHASE 6
