# Personal Notepad — Search Architecture & Deterministic Ranking

This document defines the search engine architecture, deterministic ranking hierarchy, query normalization, and performance bounds for **Personal Notepad** (Phase 6, Task 28).

---

## 1. Architectural Principles

1. **Local-First & Offline**: All searches execute strictly within the local SQLite database via Tauri Rust IPC. No remote network calls or external search engines.
2. **Zero-AI & Deterministic**: Search produces identical results for identical queries. There is no heuristic drift, LLM summarization, or semantic hallucination.
3. **Defense in Depth**:
   - Queries are bounded to 1000 characters.
   - User tokens are escaped against SQL injection and regex syntax breakage.
   - Result counts are hard-capped to 100 entries (default 50).
   - Snippets and highlighted matches are rendered as pure plain text and safe React elements, strictly prohibiting `dangerouslySetInnerHTML`.

---

## 2. Deterministic Result Ordering (Task 28 & Task 29)

### 2.1 Parameterized LIKE Search Ranking

When FTS5 is unavailable or for exact token matching, results are prioritized by a strictly ordered deterministic tier list:

```text
┌────────────────────────────────────────────────────────┐
│ 1. Exact Title Match (LOWER(title) = LOWER(query))      │
├────────────────────────────────────────────────────────┤
│ 2. Title Prefix Match (LOWER(title) LIKE 'query%')     │
├────────────────────────────────────────────────────────┤
│ 3. Title Contains Match (LOWER(title) LIKE '%query%')  │
├────────────────────────────────────────────────────────┤
│ 4. Content Contains Match (content LIKE '%query%')     │
└────────────────────────────────────────────────────────┘
                           │
                           ▼ (Tie-Breaker 1)
             modified_at DESC (Newest notes first)
                           │
                           ▼ (Tie-Breaker 2)
                 id ASC (Deterministic UUID order)
```

#### SQL Implementation:
```sql
ORDER BY
  CASE
    WHEN LOWER(title) = LOWER(?2) THEN 0
    WHEN LOWER(title) LIKE LOWER(?3) THEN 1
    WHEN LOWER(title) LIKE LOWER(?1) THEN 2
    ELSE 3
  END,
  modified_at DESC,
  id ASC
LIMIT ?4
```

### 2.2 FTS5 Full-Text Search Ranking

When the FTS5 virtual table (`notes_fts`) is active:
1. **Primary Relevance**: BM25 `rank` (lower scores denote higher token density/relevance).
2. **Secondary Tie-Breaker**: `modified_at DESC` (ensuring recency takes precedence when relevance is identical).
3. **Final Tie-Breaker**: `id ASC` (ensuring 100% deterministic result sequencing without OS/database driver ordering variance).

#### SQL Implementation:
```sql
ORDER BY rank, n.modified_at DESC, n.id ASC
LIMIT ?2
```

---

## 3. Snippet Generation & Formatting

- Extracted from note content surrounding the first matching term.
- Bounded to 120 characters with `...` prepended and appended where applicable.
- All consecutive whitespace is collapsed to single spaces for tidy visual layout.
- If only the note title matched, the first 120 characters of content serve as preview.

---

## 4. Search Result Pagination Strategy (Task 32)

### 4.1 Simple Bounded Result Set
In Personal Notepad, search follows a **simple bounded result set strategy**:
- Searches execute with an initial window limit of **50 items** (`DEFAULT_LIMIT`), clamped to a strict maximum of **100 items** (`MAX_LIMIT`).
- For personal desktop note-taking, users locate notes rapidly by refining search terms rather than paging through voluminous lists.
- A bounded result set guarantees:
  1. **Instant Response Times**: SQLite queries with bounded `LIMIT` avoid deep index scans and excessive payload serialization across the Tauri IPC bridge.
  2. **Predictable Memory Footprint**: Frontend memory remains constant regardless of the total note count in the database.
  3. **Zero Layout Shifts**: The list renders cleanly in a single pass without scroll jumping or async page insertion glitches.

### 4.2 Explicit Non-Goals (Strictly Prohibited for Phase 6)
- **No Infinite Scrolling**: Infinite scrolling causes unexpected focus jumps, scroll position loss when returning from the note editor, and heavy background IPC churn.
- **No Cursor-Based Pagination**: Adds complex state synchronization, bidirectional cursor invalidation, and unnecessary database index overhead.
- **No Complex Pagination UI**: Numerical pagination bars (`< 1 2 3 ... 10 >`) clutter the compact sidebar/list panel and degrade rapid keyboard-driven navigation (`ArrowUp` / `ArrowDown`).

---

## 5. FTS5 Index Synchronization Lifecycle (Tasks 33–38)

The FTS5 search index (`notes_fts`) is configured as an external content table (`content='notes', content_rowid='rowid'`) to eliminate content duplication and guarantee atomic synchronicity across database transactions.

### 5.1 Initial Index Population (Task 34)
- Executed during Migration `002_fts5_search_index` via `INSERT INTO notes_fts(notes_fts) VALUES('rebuild');`.
- Indexes all pre-existing notes without requiring user interaction or external batch scripts.

### 5.2 Synchronization on Note Creation (Task 35)
- Automated via SQLite trigger `notes_fts_ai`:
  ```sql
  CREATE TRIGGER IF NOT EXISTS notes_fts_ai AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
  END;
  ```
- Any newly created note is immediately indexed and searchable in the next query with zero cache lag.

### 5.3 Synchronization on Note Update / Autosave (Task 36)
- Automated via SQLite trigger `notes_fts_au`:
  ```sql
  CREATE TRIGGER IF NOT EXISTS notes_fts_au AFTER UPDATE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, content) VALUES('delete', old.rowid, old.title, old.content);
    INSERT INTO notes_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
  END;
  ```
- Replaces stale tokens with fresh tokens atomically on every title edit and autosave tick.

### 5.4 Synchronization on Soft-Delete & Hard-Delete (Task 37)
- **Soft-Delete (`is_deleted = 1`)**:
  - Soft-deletions set `is_deleted = 1` and `deleted_at = timestamp`.
  - Search queries enforce `AND n.is_deleted = 0` against the joined `notes` table.
  - Soft-deleted notes are immediately omitted from search results without permanently destroying note text or FTS index references.
- **Hard-Delete (`DELETE FROM notes`)**:
  - Automated via SQLite trigger `notes_fts_ad`:
    ```sql
    CREATE TRIGGER IF NOT EXISTS notes_fts_ad AFTER DELETE ON notes BEGIN
      INSERT INTO notes_fts(notes_fts, rowid, title, content) VALUES('delete', old.rowid, old.title, old.content);
    END;
    ```
  - Purges tokens cleanly from the virtual index.

### 5.5 Synchronization on Restore (Task 38 Readiness)
- When a note is restored (`is_deleted = 0`, `deleted_at = NULL`), the joined query filter `AND n.is_deleted = 0` immediately re-qualifies the note.
- Restored notes become instantly searchable without requiring an expensive full-table index rebuild (`rebuild`).

---

## 6. Search + Navigation Context Composition (Tasks 39–42)

### 6.1 Search + Favorites Composition Model (Task 39)
Personal Notepad implements the recommended **navigation context composition model**:
```text
search query + current navigation filter
```

#### Behavioral Contract:
1. **Scoped Query (`activeNavId === "favorites"`)**:
   - Matches: `favorite = true AND (title MATCH query OR content MATCH query)`
   - Every `SearchResult` emitted by the SQLite engine includes `favorite: boolean`.
   - Results are scoped using [`filterSearchResultsByFavorite`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/features/search/utils.ts), preserving deterministic ranking hierarchy (BM25 or LIKE relevance tiers).
2. **Context Switching**:
   - If the user searches `"project"` on *All Notes* and then selects *Favorites*, the query string is preserved and the displayed results smoothly scope down to favorited notes matching `"project"`.
   - Selecting *All Notes* unscopes the list back to all active notes matching `"project"`.
   - Selecting *Trash* resets the search query because deleted notes are explicitly excluded from active search.
3. **Optimistic Updates**:
   - Toggling the favorite star on a search result updates state optimistically; the result list instantly reflects the updated favorite filter without requiring an expensive database query re-execution.
4. **Header & Empty States**:
   - Header title: `Favorites — Search: "${searchQuery}"`.
   - Result count badge: formatted via `formatResultCount(scopedResults.length)`.
   - Empty state description: `No favorite notes match "${searchQuery}".`

### 6.2 Search + Tags Composition Model (Task 40)
Personal Notepad implements clean composition between search and tag navigation:
```text
tag = <selected_tag> AND (title MATCH query OR content MATCH query)
```

#### Behavioral Contract:
1. **Scoped Query (`location.type === "tag"`)**:
   - Matches only notes associated with the selected tag whose title or content matches the search tokens.
   - Evaluated via [`scopeSearchResults`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/features/search/utils.ts) matching note tag assignments case-insensitively against `noteTagsMap`.
   - Preserves deterministic search ranking (exact BM25 or LIKE tier order).
2. **State Non-Corruption Guarantee**:
   - Navigation state (`location`) and search state (`searchQuery`, `searchState`) are strictly orthogonal.
   - Tag creation, rename, or deletion operations never corrupt search state or in-flight debounces.
   - Exiting search (via `Escape` or clear button `x`) preserves the active tag filter, immediately returning the user to the complete list of notes for that tag.
   - Selecting a tag while searching preserves the active search query and seamlessly scopes results down to the newly selected tag.
3. **Header & Empty States**:
   - Header title: `#<tag_name> — Search: "${searchQuery}"`.
   - Result count badge: formatted via `formatResultCount(scopedResults.length)`.
   - Empty state description: `No notes tagged #<tag_name> match "${searchQuery}".`

### 6.3 Search + Notebook Composition Model (Task 41)
Personal Notepad implements notebook-scoped search through the same composition layer:
```text
notebook = <selected_notebook_id> AND (title MATCH query OR content MATCH query)
```

#### Behavioral Contract:
1. **Scoped Query (`location.type === "notebook"`)**:
   - Results from the global FTS engine are post-filtered to those where `result.notebookId === location.notebookId`.
   - Pure O(n) frontend filter — no additional SQL round-trip required.
2. **Header & Empty States**:
   - Header title: `<notebook_name> — Search: "${searchQuery}"`.
   - Empty state title: `No notes found in this notebook`.
   - Empty state description: `No notes in "<notebook_name>" match "${searchQuery}".`

---

## 7. Search Navigation Model (Task 42)

### 7.1 NoteLocation — Single Source of Navigation Truth

The navigation context is represented by the `NoteLocation` discriminated union type:

```ts
type NoteLocation =
  | { type: "all" }
  | { type: "unfiled" }
  | { type: "favorites" }
  | { type: "notebook"; notebookId: string }
  | { type: "tag"; tagId: string }
  | { type: "trash" };
```

Source: [`src/features/notebooks/types/notebook.ts`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/features/notebooks/types/notebook.ts)

### 7.2 State Separation Invariant

Search state is **always** stored separately from navigation state:

```text
state.location     ← NoteLocation (where am I in the sidebar?)
state.searchQuery  ← string       (what am I searching for?)
```

These two values are **composited** at display time by `scopeSearchResults()`.

**Do NOT create:**
```text
tagSearchLocation       ← FORBIDDEN
favoriteSearchLocation  ← FORBIDDEN
notebookSearchLocation  ← FORBIDDEN
```
These would duplicate state, create inconsistency bugs, and prevent clean state restoration when search is cleared.

### 7.3 Composition Pipeline (Task 42)

```
searchResults (global FTS, up to 50 results)
      │
      ▼
scopeSearchResults(results, { location, tags, noteTagsMap, notesLookup })
      │
      ├─ location.type === "favorites"  → filter by is_favorite
      ├─ location.type === "tag"        → filter by tag membership
      ├─ location.type === "notebook"   → filter by notebookId
      └─ otherwise                      → pass-through (no filter)
      │
      ▼
scopedSearchResults (displayed in NotesList)
```

### 7.4 Bridge Utility

[`locationToNavId(loc: NoteLocation)`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/features/notebooks/types/notebook.ts) maps the canonical `NoteLocation` to the `NavItemId` string used by the `Sidebar` presentation component, ensuring the component never needs to know about the full type hierarchy.

---

## 8. Search Clear Behavior (Task 43)

### 8.1 Chosen Strategy: Context-Preserving Clear

When the user clears the search query, the UI returns to the **current navigation context** — the same notebook, tag, favorites, or All Notes list that was active before searching.

```text
Before search:  Work / Projects notebook
Search query:   "deadline"
Clear search:   → Work / Projects notebook  (same 5 notes as before)
```

This is possible because `notes` state is always loaded for the active `NoteLocation` and is never replaced by search results. Clearing search simply sets `isSearchActive = false`, which switches `displayedNoteListItems` back from `scopedSearchResults` to `noteListItems` — no extra IPC round-trip needed.

### 8.2 Clear Entry Points

| Entry Point | Mechanism | Focus after clear |
|---|---|---|
| `×` button in TopBar | `clearSearch()` → blur | Input loses focus |
| `Escape` key in search input | `clearSearch()` → blur | Input loses focus |
| Global `Escape` handler | `clearSearch()` → blur | Input loses focus |
| Sidebar navigation click | `clearSearch()` (inline) | Sidebar item |

All three paths are consistent: after clearing, the search input is **blurred** and focus returns to the content area. (Prior to Task 43 the `×` button incorrectly re-focused the input.)

### 8.3 Note Selection Restoration

During search, the auto-select `useEffect` may change `selectedNoteId` to a search result from any navigation context. When search is cleared:

- If the previously selected note **is visible** in the restored context → selection is preserved, editor stays open, no work is lost.
- If the previously selected note **is not visible** (e.g. it was from a different notebook) → `selectedNoteId` falls back to `filteredNotes[0].id`, or `null` if the list is empty.

This restoration is handled by a dedicated `useEffect` in [`App.tsx`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/app/App.tsx) that fires whenever `isSearchActive` transitions to `false`.

### 8.4 What Is NOT Done

- Search does **not** navigate away from the current context on clear.
- Clearing search does **not** reload notes from SQLite — the pre-search note list is already in memory.
- There are **no** separate `searchLocation` state variants. The composition model (§7) handles all scoping.

---

## 9. Search Result Selection Behavior (Task 44)

### 9.1 Chosen Strategy: Search Remains Active After Selection

When a search result is clicked (or activated via keyboard), the search query **stays active**. The UI transitions to:

```text
Search query:  "deadline"              ← still visible in TopBar
NotesList:     search results (scoped) ← still showing
Editor:        selected note opens     ← new
Focus:         editor pane             ← shifted from search input
```

This allows the user to browse through multiple results without retyping the query. The three-pane layout comfortably supports this pattern.

### 9.2 Focus Contract

| Action | Focus after |
|---|---|
| Click a search result | Search input blurred → editor receives focus |
| Keyboard Enter/Space on result card | Search input blurred → editor receives focus |
| Re-click already-selected result | Search input blurred (no selection change, no save round-trip) |

The blur happens unconditionally after any `onSelectNote` call while `isSearchActive` is true. This ensures the user can immediately type in the editor without the search bar capturing keystrokes.

### 9.3 Explicit Exit Required

Search is **not** cleared automatically on result selection. To exit search mode the user must:
- Click the `×` button
- Press `Escape`
- Navigate to a different sidebar section

This is consistent with the behavior documented in §8 (Search Clear Behavior).

### 9.4 Re-selection No-Op

Clicking a result that is already the active selection:
- Does **not** trigger a save round-trip
- Does **not** reload the note from SQLite
- **Does** blur the search input (so user gets focus in editor)

The guard `if (noteId !== selectedNoteId)` in [`App.tsx`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/app/App.tsx) prevents the unnecessary save and reload while the blur still fires.

---

## 10. Search Result Keyboard Navigation (Task 45)

### 10.1 Supported Keys

| Key | Action |
|---|---|
| `↓` Arrow Down | Select next result (wraps to first at end) |
| `↑` Arrow Up | Select previous result (wraps to last at start) |
| `Enter` / `Space` | Open focused card (handled by NoteCard's own `onKeyDown`) |
| `Escape` | Clear search and return to nav context (handled by global `handleEscape`) |

### 10.2 Editor Non-Interference Guarantee (Task 45 requirement)

Arrow keys inside the editor must scroll text and move the cursor as normal. The hook [`useSearchKeyboardNav`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/features/search/useSearchKeyboardNav.ts) guards against this via `isEditorFocused()`:

```ts
// Skips if active element is inside .app-editor-pane,
// a bare <textarea>, a bare <input>, or a contenteditable element.
function isEditorFocused(): boolean { ... }
```

This check runs on every keydown before the arrow key is consumed, ensuring zero interference with the editor, modals, or any other text-input context.

### 10.3 Navigation Wrapping Behavior

```text
Results: [A, B, C, D]   selected: D
↓  →  A  (wrap to first)

Results: [A, B, C, D]   selected: A
↑  →  D  (wrap to last)

Results: [A, B, C, D]   nothing selected
↓  →  A  (start at first)
↑  →  D  (start at last)
```

### 10.4 Scroll Into View

After moving the selection, the hook queries `[data-note-id="<id>"]` on the DOM and calls `.scrollIntoView({ block: "nearest", behavior: "smooth" })` inside a `requestAnimationFrame` so React has flushed the selection state before the scroll fires. `NoteCard` renders the `data-note-id` attribute to support this.

### 10.5 Unified Selection Callback

Both mouse clicks and arrow-key navigation go through the same `handleKeyboardSelectNote` callback in `App.tsx`:

```text
cancelPendingDebounce()
if noteId changed → save current note → setSelectedNoteId
if search is active → blur search input
```

This ensures the editor always gets focus after any selection, regardless of how the selection was triggered.

### 10.6 Hook Architecture

[`useSearchKeyboardNav`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/features/search/useSearchKeyboardNav.ts) is a standalone `window.addEventListener('keydown')` hook, separate from `useKeyboardShortcuts`. This keeps arrow navigation logic self-contained and avoids polluting the global shortcuts registry with search-specific behavior.

---

## 11. Search Result Accessibility (Task 46)

### 11.1 Accessibility Checklist & Implementation

Task 46 specifies six core requirements for every search result card:

| Requirement | Implementation in `NoteCard` | Assistive Tech Exposure |
|---|---|---|
| **Keyboard focus** | `tabIndex={0}` + `.note-card:focus-visible` outline | Focusable via `Tab` and `↓`/`↑` arrows; clear 2px accent outline |
| **Accessible name** | `aria-label={displayTitle}` where `displayTitle = note.title?.trim() \|\| "Untitled Note"` | Cleanly announces the note title instead of concatenating child content |
| **Selected state** | `aria-selected={isSelected}` + `aria-current={isSelected ? "true" : undefined}` | Announces whether card is currently selected / active |
| **Clear title** | `<h3 className="note-card-title" id={`note-card-title-${note.id}`}>` | Visual and semantic heading with search match highlighting |
| **Optional snippet** | `aria-describedby={note.preview ? `note-card-preview-${note.id}` : undefined}` on card; `<p id={`note-card-preview-${note.id}`} ...>` on preview | Screen readers announce snippet as description only when present |
| **Favorite indicator** | `<button>` with `aria-label`, `title`, `aria-pressed={!!note.isFavorite}`, and SVG `aria-hidden="true"` | Accessible toggle button announcing state and action |

### 11.2 Container Semantics & Proper ARIA Roles

The specification mandates: *"If using a listbox-style pattern, follow proper ARIA semantics. Do not add ARIA roles incorrectly."*

- **Why `role="region"` is used rather than `role="listbox"`**:
  Under WAI-ARIA 1.2 §5.2.7, elements with `role="option"` specify `Children Presentational: True`. Screen readers suppress the interactivity of child controls inside an `option`. Because `NoteCard` contains an interactive favorite toggle button, placing it inside `role="listbox"` with `role="option"` creates an ARIA semantics violation that prevents screen reader users from accessing the favorite button.
- **Region Landmark**:
  `NotesList` wraps results in:
  ```tsx
  <div
    className="notes-list-items"
    role="region"
    aria-label={isSearchMode ? "Search results" : "Notes list"}
  >
  ```
  This creates a valid ARIA landmark without restricting interactive descendants.
- **Live Announcements**:
  The count badge in the header renders `aria-live="polite"`:
  ```tsx
  <Badge variant="default" size="sm" aria-live="polite">
    {countBadgeText}
  </Badge>
  ```
  This ensures screen readers announce result count changes (e.g. "3 results", "Searching...") without interrupting active speech.

### 11.3 Focus Management During Keyboard Navigation

In [`useSearchKeyboardNav`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/features/search/useSearchKeyboardNav.ts):
- When navigating with `↓` and `↑`, the hook smooth-scrolls the active result into view and calls `card?.focus({ preventScroll: true })`.
- Focus lands directly on the selected card, ensuring immediate screen reader feedback.
- Arrow navigation from the TopBar search input is enabled, allowing seamless transition from typing to result browsing.
- Editor non-interference guard remains strictly enforced when focus is inside `.app-editor-pane`.

---

## 12. Search Input Accessibility (Task 47)

### 12.1 Explicit Accessible Naming vs. Placeholder Text

Task 47 mandates:
- `aria-label="Search notes"` (or visible label equivalent).
- Placeholder: `Search notes...`.
- *Do not rely solely on placeholder text for accessibility.*

#### Why Placeholders Alone Fail Accessibility
1. **Disappearance on Input**: When the user enters text or focuses some browsers, placeholder text vanishes. Screen readers that inspect the input during editing lose the context if only placeholder text was provided.
2. **Low Contrast & Browser Inconsistencies**: Native browser placeholders default to low contrast ratios that fail WCAG 2.1 AA (1.4.3 Contrast Minimum).
3. **Calculation Priority**: Under W3C Accessible Name and Description Computation 1.2, `aria-label` takes explicit precedence over `placeholder`. Supplying `aria-label="Search notes"` guarantees the input's accessible name remains `"Search notes"` regardless of whether the field is empty, focused, or populated.

### 12.2 Search Landmark & Auxiliary Controls

Implemented in [`TopBar.tsx`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/components/TopBar/TopBar.tsx):

- **Search Landmark**:
  ```tsx
  <div className="topbar-search-container" role="search">
  ```
  Provides the standard WAI-ARIA `search` landmark for assistive technology screen reader navigation menus.
- **Search Input Element**:
  ```tsx
  <input
    ref={searchInputRef}
    type="text"
    className="search-input selectable-text"
    placeholder="Search notes..."
    value={searchQuery}
    onChange={(e) => onSearchChange?.(e.target.value)}
    aria-label="Search notes"
    autoComplete="off"
    spellCheck={false}
  />
  ```
- **Visual Shortcut Hint**:
  When `searchQuery` is empty, a subtle `<kbd className="search-shortcut-hint" aria-hidden="true">Ctrl+K</kbd>` badge is rendered. Because it is marked `aria-hidden="true"`, visual users see the keyboard shortcut while screen readers are not burdened with spurious shortcut announcements during speech synthesis.
- **Clear Button**:
  Exposes both `title="Clear search"` and `aria-label="Clear search"`.

---

## 13. Unicode Search (Task 48)

### 13.1 Requirements & Test Terms

Task 48 requires exact Unicode search across non-Latin scripts, combining vowel marks, CJK ideographs, and Latin characters with diacritics:

| Script / Type | Test Terms | Example Context |
|---|---|---|
| **Devanagari (Hindi)** | `भारत`, `यात्रा`, `हिंदी` | Indic script using dependent vowel signs/matras (`ा`, `ि`) and anusvara (`ं`) |
| **CJK Ideographs** | `東京` | Logographic script without inter-word whitespace |
| **Accented Latin** | `café`, `résumé` | Extended Latin with acute accents (`é`) |

### 13.2 Backend FTS5 Tokenizer & Sanitization Architecture

1. **SQLite FTS5 Tokenizer**:
   `notes_fts` uses the standard `unicode61` tokenizer. This tokenizer natively recognizes Unicode letter sequences and performs Latin case folding while leaving non-Latin scripts intact.

2. **Crucial Sanitization Invariant for Combining Marks**:
   In Rust, `char::is_alphanumeric()` only checks whether a character belongs to Unicode `Letter` or `Number` categories.
   - Devanagari vowel signs/matras (e.g. `ा` U+093E, `ि` U+093F) belong to category `Mc` (Spacing Mark).
   - Anusvara (`ं` U+0902) belongs to category `Mn` (Nonspacing Mark).
   - A naive `c.is_alphanumeric()` filter erroneously stripped all vowel signs, turning `"भारत"` into `"भरत"` and `"हिंदी"` into `"हद"`.
   
   In [`search.rs`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src-tauri/src/storage/repositories/search.rs), `sanitize_fts5_query()` was updated to preserve all valid non-ASCII characters (including combining marks, accents, and ideographs) while stripping only FTS5 syntax control operators and quotation characters:
   ```rust
   let clean: String = w
       .chars()
       .filter(|c| {
           if c.is_ascii() {
               c.is_ascii_alphanumeric() || *c == '_'
           } else {
               !c.is_control()
                   && !c.is_whitespace()
                   && !is_unicode_punctuation_or_quote(*c)
           }
       })
       .collect();
   ```

3. **LIKE Search Fallback**:
   When falling back to SQL `LIKE`, SQLite natively matches UTF-8 byte sequences without requiring ASCII normalization.

### 13.3 Frontend Highlighting & State Handling

1. **`highlightText`**:
   Uses standard JavaScript Unicode regex with case-insensitive token matching:
   ```ts
   const regex = new RegExp(`(${escapedTokens})`, "gi");
   ```
   Correctly wraps matched Unicode substrings in `<mark className="search-highlight">` without splitting combining mark clusters.

2. **Section Title & UI Display**:
   Formatting utilities in `src/features/search/utils.ts` treat Unicode strings as first-class text, ensuring clean presentation in the search header, badge counts, and empty-state messaging.

---

## 14. Case-Insensitive Search & Engine Limitations (Task 49)

### 14.1 Verified Behavior

Task 49 requires testing across all casing permutations:
```text
Project
project
PROJECT
PrOjEcT
```

All 4 queries produce identical result sets and identical deterministic rank ordering across both backend FTS5 and frontend state management.

### 14.2 Architecture Across the Stack

1. **FTS5 `unicode61` Tokenizer**:
   During index population and query execution, the `unicode61` tokenizer folds ASCII uppercase to lowercase tokens. Querying `"Project"*`, `"project"*`, `"PROJECT"*`, or `"PrOjEcT"*` all resolve to the same underlying inverted index key (`"project"`).
2. **LIKE Fallback Strategy**:
   The SQL fallback uses `title LIKE ?1 ESCAPE '\' OR content LIKE ?1 ESCAPE '\'` where SQLite evaluates ASCII characters case-insensitively. The `ORDER BY CASE WHEN LOWER(title) = LOWER(?2)` hierarchy uses `LOWER()` to rank exact matches first regardless of input casing.
3. **Frontend Token Highlighting**:
   `highlightText()` uses `new RegExp(tokens, "gi")` and compares `token.toLowerCase() === part.toLowerCase()`, ensuring consistent highlight wrapping for any casing mix.

### 14.3 Unavoidable SQLite / FTS Unicode Case-Folding Limitations

Per Task 49 guidance: *"Document any unavoidable SQLite/FTS Unicode case-folding limitations. Do not falsely claim universal Unicode case folding if the underlying engine does not provide it."*

The notepad application accurately documents the following engine-level boundaries:

| Layer / Mechanism | Case-Insensitivity Scope | Known Boundary / Limitation |
|---|---|---|
| **SQLite built-in `LIKE`** | ASCII characters (`A-Z` ↔ `a-z`) | **Case-sensitive for non-ASCII Unicode**. For example, in vanilla SQLite without ICU, `'café' LIKE 'CAFÉ'` evaluates to `0` (false). |
| **SQLite built-in `LOWER()` / `UPPER()`** | ASCII characters only | `LOWER('CAFÉ')` produces `'cafÉ'` (the accented `É` is not converted to `é`). |
| **FTS5 `unicode61` Tokenizer** | ASCII + Unicode 6.1 static tables (Latin diacritics, Cyrillic, Greek) | Folds Latin diacritics (`CAFÉ` matches `café`), but **does not perform locale-sensitive case folding** (e.g. Turkish dotted `İ` vs dotless `ı`, or German `ß` vs `SS`). |
| **JavaScript Frontend** | Full Unicode ECMAScript 2024 | Browser/Node JS engine handles case-insensitive RegExp matching across standard Unicode scripts. |

---

## 15. Special Characters & Parameterized Queries (Task 50)

### 15.1 Special Character Test Suite

Task 50 specifies robust handling for user queries containing arbitrary punctuation, programming syntax, email addresses, and URLs:

| Query Input | Test Context | Expected Engine Behavior |
|---|---|---|
| `C++` | Language name with unary operators | Safely tokenized to `"C"*` in FTS / matches exact `LIKE '%C++%'` |
| `C#` | Language name with hash symbol | Safely tokenized to `"C"*` in FTS / matches exact `LIKE '%C#%'` |
| `R&D` | Ampersand abbreviation | Segmented to FTS phrase `("R"* + "D"*)` matching "Research & Development" |
| `project-x` | Hyphenated slug | Segmented to FTS phrase `("project"* + "x"*)` |
| `hello.world` | Dot-separated identifier | Segmented to FTS phrase `("hello"* + "world"*)` |
| `foo/bar` | Slash-separated path | Segmented to FTS phrase `("foo"* + "bar"*)` |
| `user@example.com` | Email address with `@` and `.` | Segmented to FTS phrase `("user"* + "example"* + "com"*)` |

### 15.2 Safety Guarantees & Contract

Search adheres strictly to the four core constraints of Task 50:

1. **Zero Crashes**: All query transformations use bounded, safe string operations. No unwrap panics occur on malformed symbols.
2. **No Malformed SQL**: Raw search terms are **never concatenated** into SQL strings.
   - For FTS5 queries, terms are sanitized via `sanitize_fts5_query()` and passed into `notes_fts MATCH ?1` via parameter binding.
   - For LIKE queries, wildcards (`%`, `_`, `\`) are escaped via `escape_like()` and bound via `rusqlite::params![escaped_pattern, ...]`.
3. **Zero SQL Interpretation**: User inputs (even those containing SQL fragments like `' OR 1=1 --`) are treated as literal text tokens. They cannot alter query logic or execute arbitrary commands.
4. **Zero Error Exposure**: The frontend receives structured `SearchState` discriminated unions (`idle`, `loading`, `success`, `error`). Internal database connection strings or SQLite syntax errors are never leaked to user-facing UI.
5. **Parameterized Queries are Mandatory**: All queries across the repository use strictly parameterized statements (`?1`, `?2`, etc.) bound at runtime by the SQLite engine driver.

### 15.3 Frontend Highlighting Safety

In [`utils.ts`](file:///home/ritiksaini/Desktop/localhost/own/custom-notepad/src/features/search/utils.ts):
```ts
const escapedTokens = tokens
  .map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
  .join("|");

try {
  const regex = new RegExp(`(${escapedTokens})`, "gi");
  ...
} catch {
  return text; // Graceful fallback if dynamic regex fails
}
```
All special regex characters are safely escaped prior to compilation, ensuring queries like `C++`, `hello.world`, and `foo/bar` highlight without throwing `SyntaxError`.












