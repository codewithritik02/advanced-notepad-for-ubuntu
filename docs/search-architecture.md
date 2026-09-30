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
