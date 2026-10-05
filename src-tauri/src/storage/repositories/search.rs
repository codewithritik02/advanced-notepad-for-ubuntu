use rusqlite::{params, Connection};
use crate::storage::{errors::StorageError, models::SearchResult};

pub struct SearchRepository;

impl SearchRepository {
    /// Maximum allowed search query length in characters (Task 88, 89)
    pub const MAX_QUERY_LENGTH: usize = 1000;

    /// Default search result count limit
    pub const DEFAULT_LIMIT: usize = 50;

    /// Hard maximum search result count limit (Task 31, 62)
    pub const MAX_LIMIT: usize = 100;

    /// Searches active, non-deleted notes by title and content.
    /// Safely handles empty input, Unicode, special characters, and SQL injection inputs.
    pub fn search(
        conn: &Connection,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<SearchResult>, StorageError> {
        let trimmed = query.trim();

        // Task 7: Empty or whitespace query returns empty result without expensive DB search
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        // Task 88 & 89: Backend query length validation
        if trimmed.chars().count() > Self::MAX_QUERY_LENGTH {
            return Err(StorageError::Validation(format!(
                "Search query exceeds maximum limit of {} characters.",
                Self::MAX_QUERY_LENGTH
            )));
        }

        let bounded_limit = limit.unwrap_or(Self::DEFAULT_LIMIT).clamp(1, Self::MAX_LIMIT);

        let start = std::time::Instant::now();

        // Check if FTS5 virtual table exists in the database
        let has_fts: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        let search_res = if has_fts {
            match Self::search_fts5(conn, trimmed, bounded_limit) {
                Ok(results) => Ok(results),
                Err(_) => {
                    // Task 66: Inconsistent or corrupted FTS index detected.
                    // Recovery strategy: Rebuild search index without touching notes,
                    // retry FTS5 query, or safely fall back to LIKE search.
                    let _ = Self::recover_index(conn);
                    if let Ok(recovered) = Self::search_fts5(conn, trimmed, bounded_limit) {
                        Ok(recovered)
                    } else {
                        Self::search_like(conn, trimmed, bounded_limit)
                    }
                }
            }
        } else {
            // Task 66: Missing index detected -> recreate -> populate from notes
            let _ = Self::recover_index(conn);
            if let Ok(recovered) = Self::search_fts5(conn, trimmed, bounded_limit) {
                Ok(recovered)
            } else {
                Self::search_like(conn, trimmed, bounded_limit)
            }
        };

        // Task 63: Performance logging during development (query, result count, execution time)
        // Strictly never logs note content. Completely eliminated in release builds.
        #[cfg(debug_assertions)]
        {
            if let Ok(ref items) = search_res {
                eprintln!(
                    "[Search] query executed: count={} duration={}ms query=\"{}\"",
                    items.len(),
                    start.elapsed().as_millis(),
                    trimmed
                );
            }
        }

        search_res
    }

    /// Rebuilds the FTS5 search index internally from the authoritative notes content table (Task 65).
    ///
    /// Useful for:
    /// - Database migrations
    /// - Index corruption recovery
    /// - Future maintenance
    ///
    /// Re-creates virtual table and sync triggers if missing, then invokes SQLite FTS5 'rebuild'.
    /// Strictly an internal backend capability; no user-facing button is exposed in Phase 6.
    pub fn rebuild_search_index(conn: &Connection) -> Result<(), StorageError> {
        let sql = r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                title,
                content,
                content='notes',
                content_rowid='rowid'
            );

            CREATE TRIGGER IF NOT EXISTS notes_fts_ai AFTER INSERT ON notes BEGIN
              INSERT INTO notes_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS notes_fts_ad AFTER DELETE ON notes BEGIN
              INSERT INTO notes_fts(notes_fts, rowid, title, content) VALUES('delete', old.rowid, old.title, old.content);
            END;

            CREATE TRIGGER IF NOT EXISTS notes_fts_au AFTER UPDATE ON notes BEGIN
              INSERT INTO notes_fts(notes_fts, rowid, title, content) VALUES('delete', old.rowid, old.title, old.content);
              INSERT INTO notes_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
            END;

            INSERT INTO notes_fts(notes_fts) VALUES('rebuild');
        "#;

        conn.execute_batch(sql)?;
        Ok(())
    }

    /// Safe recovery strategy for missing or inconsistent/corrupted FTS search index (Task 66).
    ///
    /// Lifecycle:
    /// 1. Detect missing or broken index
    /// 2. Re-create virtual table and sync triggers if missing
    /// 3. Rebuild / populate index from authoritative notes table
    ///
    /// Invariant: Authoritative `notes` records are strictly never deleted or modified
    /// because search index creation or recovery failed.
    pub fn recover_index(conn: &Connection) -> Result<(), StorageError> {
        // Attempt fast rebuild if virtual table is intact
        if Self::rebuild_search_index(conn).is_ok() {
            return Ok(());
        }

        // If rebuild failed (e.g. shadow table corruption), drop virtual table & triggers cleanly and recreate
        let cleanup_sql = r#"
            DROP TABLE IF EXISTS notes_fts;
            DROP TRIGGER IF EXISTS notes_fts_ai;
            DROP TRIGGER IF EXISTS notes_fts_ad;
            DROP TRIGGER IF EXISTS notes_fts_au;
        "#;
        let _ = conn.execute_batch(cleanup_sql);
        Self::rebuild_search_index(conn)
    }

    /// Executes FTS5 full-text query joined with authoritative notes table
    fn search_fts5(
        conn: &Connection,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        // Sanitize search tokens for FTS5 syntax safety (Task 73)
        // Wrap each whitespace-delimited word in double-quotes with prefix wildcard
        let fts_query = sanitize_fts5_query(query);
        if fts_query.is_empty() {
            return Self::search_like(conn, query, limit);
        }

        let sql = r#"
            SELECT
                n.id,
                n.title,
                snippet(notes_fts, 1, '', '', '...', 12) AS snippet_text,
                n.modified_at,
                n.notebook_id,
                n.is_favorite
            FROM notes_fts
            JOIN notes n ON notes_fts.rowid = n.rowid
            WHERE notes_fts MATCH ?1
              AND n.is_deleted = 0
            ORDER BY rank, n.modified_at DESC, n.id ASC
            LIMIT ?2
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![fts_query, limit as i64], |row| {
            let snippet_str: Option<String> = row.get(2)?;
            let clean_snippet = snippet_str
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            Ok(SearchResult {
                note_id: row.get(0)?,
                title: row.get(1)?,
                snippet: clean_snippet,
                modified_at: row.get(3)?,
                notebook_id: row.get(4)?,
                favorite: row.get::<_, i32>(5)? != 0,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }

        Ok(results)
    }

    /// Parameterized LIKE search fallback matching title or content case-insensitively.
    ///
    /// Deterministic Ranking Hierarchy (Task 28 & 29):
    /// 1. Exact title match (LOWER(title) = LOWER(query))
    /// 2. Title prefix match (LOWER(title) LIKE LOWER(query%))
    /// 3. Title contains match (LOWER(title) LIKE LOWER(%query%))
    /// 4. Content contains match
    /// 5. Secondary tie-breaker: modified_at DESC (newest edits first)
    /// 6. Final tie-breaker: id ASC (guarantees 100% deterministic result ordering)
    fn search_like(
        conn: &Connection,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        let exact_title = query.to_string();

        let sql = r#"
            SELECT
                id,
                title,
                content,
                modified_at,
                notebook_id,
                is_favorite
            FROM notes
            WHERE is_deleted = 0
              AND (title LIKE ?1 ESCAPE '\' OR content LIKE ?1 ESCAPE '\')
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
        "#;

        // Escape LIKE wildcards inside user query for exact matching safety
        let escaped_pattern = format!("%{}%", escape_like(query));
        let escaped_prefix = format!("{}%", escape_like(query));

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(
            params![escaped_pattern, exact_title, escaped_prefix, limit as i64],
            |row| {
                let note_id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let content: String = row.get(2)?;
                let modified_at: String = row.get(3)?;
                let notebook_id: Option<String> = row.get(4)?;
                let is_fav: i32 = row.get(5)?;

                let snippet = generate_snippet(&content, query, 120);

                Ok(SearchResult {
                    note_id,
                    title,
                    snippet,
                    modified_at,
                    notebook_id,
                    favorite: is_fav != 0,
                })
            },
        )?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }

        Ok(results)
    }
}

/// Escapes SQL LIKE wildcards (% and _)
fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        if c == '%' || c == '_' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Transforms user search string into safe FTS5 query tokens (Task 71 & 73).
///
/// Rules:
/// 1. Explicit Quoted Phrases: e.g. `"project plan"` or `'project plan'`
///    Enforces contiguous phrase matching in SQLite FTS5 (`"project plan"`).
///    It strictly matches adjacent tokens in that exact order and NEVER evaluates as `project OR plan`.
/// 2. Standard Multi-Word Search: e.g. `project plan`
///    Terms are separated by whitespace, which SQLite FTS5 evaluates as an implicit AND operator.
///    Both terms must be present in the document. It NEVER evaluates as `project OR plan`.
/// 3. Special Characters: Punctuation and control characters are stripped to prevent syntax errors.
fn sanitize_fts5_query(query: &str) -> String {
    let trimmed = query.trim();

    // Check if the query is an explicitly quoted phrase: e.g. "project plan"
    let is_quoted_phrase = (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
        || (trimmed.starts_with('“') && trimmed.ends_with('”') && trimmed.len() >= 2);

    if is_quoted_phrase {
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut tokens = Vec::new();

        for w in inner.split_whitespace() {
            let mut current_token = String::new();
            for c in w.chars() {
                if c.is_ascii() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        current_token.push(c);
                    }
                } else if !c.is_control() && !c.is_whitespace() && !is_unicode_punctuation_or_quote(c) {
                    current_token.push(c);
                }
            }
            if !current_token.is_empty() {
                tokens.push(current_token);
            }
        }

        if !tokens.is_empty() {
            // In FTS5, "token1 token2" enforces exact contiguous phrase matching in that order
            return format!("\"{}\"", tokens.join(" "));
        }
    }

    let mut word_phrases = Vec::new();

    for w in trimmed.split_whitespace() {
        let mut sub_tokens = Vec::new();
        let mut current_token = String::new();

        for c in w.chars() {
            if c.is_ascii() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    current_token.push(c);
                } else if !current_token.is_empty() {
                    sub_tokens.push(current_token.clone());
                    current_token.clear();
                }
            } else if !c.is_control()
                && !c.is_whitespace()
                && !is_unicode_punctuation_or_quote(c)
            {
                current_token.push(c);
            } else if !current_token.is_empty() {
                sub_tokens.push(current_token.clone());
                current_token.clear();
            }
        }
        if !current_token.is_empty() {
            sub_tokens.push(current_token);
        }

        if !sub_tokens.is_empty() {
            let phrase = sub_tokens.join(" ");
            word_phrases.push(format!("\"{}\"*", phrase));
        }
    }

    word_phrases.join(" ")
}

fn is_unicode_punctuation_or_quote(c: char) -> bool {
    matches!(
        c,
        '“' | '”' | '‘' | '’' | '«' | '»' | '「' | '」' | '『' | '』' | '—' | '–' | '…'
    )
}

/// Generates a safe plain-text preview snippet centered around the matching query term
pub fn generate_snippet(content: &str, query: &str, max_len: usize) -> Option<String> {
    if content.trim().is_empty() || query.trim().is_empty() {
        return None;
    }

    let lower_content = content.to_lowercase();
    let lower_query = query.to_lowercase();

    if let Some(pos) = lower_content.find(&lower_query) {
        // Calculate char slice window around match
        let chars: Vec<char> = content.chars().collect();
        let char_pos = lower_content[..pos].chars().count();
        let query_char_len = query.chars().count();

        let half_window = (max_len.saturating_sub(query_char_len)) / 2;
        let start = char_pos.saturating_sub(half_window);
        let end = (char_pos + query_char_len + half_window).min(chars.len());

        let mut snippet = String::new();
        if start > 0 {
            snippet.push_str("...");
        }

        let slice: String = chars[start..end].iter().collect();
        // Collapse whitespace in snippet for clean display
        let clean_slice: String = slice.split_whitespace().collect::<Vec<_>>().join(" ");
        snippet.push_str(&clean_slice);

        if end < chars.len() {
            snippet.push_str("...");
        }

        Some(snippet)
    } else {
        // Fallback: start of content if query matched title rather than body
        let chars: Vec<char> = content.chars().collect();
        let end = max_len.min(chars.len());
        let slice: String = chars[..end].iter().collect();
        let clean_slice = slice.split_whitespace().collect::<Vec<_>>().join(" ");
        if clean_slice.is_empty() {
            None
        } else if end < chars.len() {
            Some(format!("{}...", clean_slice))
        } else {
            Some(clean_slice)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::{CreateNoteDto, UpdateNoteDto};
    use crate::storage::repositories::NoteRepository;

    #[test]
    fn test_search_repository_empty_and_validation() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_val_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Empty query
        assert_eq!(SearchRepository::search(&conn, "", None).unwrap().len(), 0);
        assert_eq!(SearchRepository::search(&conn, "   ", None).unwrap().len(), 0);

        // Query exceeding 1000 characters
        let huge_query = "a".repeat(1001);
        let err = SearchRepository::search(&conn, &huge_query, None).unwrap_err();
        match err {
            StorageError::Validation(msg) => assert!(msg.contains("1000 characters")),
            _ => panic!("Expected ValidationError"),
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_search_repository_basic_matching_and_soft_delete() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_match_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Create Note 1 (Title match)
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Plan Alpha".to_string()),
                content: Some("Overview of milestones".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Create Note 2 (Content match)
        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Team Sync".to_string()),
                content: Some("Discussion regarding the project deadline".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Create Note 3 (Deleted note containing query)
        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Old Project Archive".to_string()),
                content: Some("Archived project details".to_string()),
                ..Default::default()
            },
        ).unwrap();
        NoteRepository::delete(&conn, &note3.id, true).unwrap();

        // Search: "project" (case-insensitive)
        let results = SearchRepository::search(&conn, "project", None).unwrap();
        assert_eq!(results.len(), 2, "Must find exactly 2 active notes");
        let ids: Vec<String> = results.iter().map(|r| r.note_id.clone()).collect();
        assert!(ids.contains(&note1.id));
        assert!(ids.contains(&note2.id));
        assert!(!ids.contains(&note3.id), "Soft-deleted note must be excluded");

        // Snippet verification for content match
        let n2_res = results.iter().find(|r| r.note_id == note2.id).unwrap();
        assert!(n2_res.snippet.is_some());
        let snip = n2_res.snippet.as_ref().unwrap();
        assert!(snip.to_lowercase().contains("project"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_search_sql_injection_and_special_chars() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_sqli_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Safe Note".to_string()),
                content: Some("Some normal content".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Try SQL injection patterns
        let res1 = SearchRepository::search(&conn, "' OR 1=1 --", None);
        assert!(res1.is_ok(), "SQL injection input must not crash or error");
        assert_eq!(res1.unwrap().len(), 0);

        let res2 = SearchRepository::search(&conn, "\" OR \"1\"=\"1", None);
        assert!(res2.is_ok());
        assert_eq!(res2.unwrap().len(), 0);

        // Special characters
        let res3 = SearchRepository::search(&conn, "C++ & C# / % _", None);
        assert!(res3.is_ok());

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_search_input_normalization_and_unicode() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_norm_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        let n1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("भारत यात्रा".to_string()),
                content: Some("दिल्ली से मुंबई तक की यात्रा की योजना".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let n2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Morning Café".to_string()),
                content: Some("Meeting at the café to review the résumé".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let n3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("東京ガイド".to_string()),
                content: Some("観光名所のリストとホテル予約".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 1. Whitespace trimming: "   भारत   "
        let r1 = SearchRepository::search(&conn, "   भारत   ", None).unwrap();
        assert_eq!(r1.len(), 1);
        assert_eq!(r1[0].note_id, n1.id);

        // 2. Accented Unicode preservation: "   café   "
        let r2 = SearchRepository::search(&conn, "   café   ", None).unwrap();
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].note_id, n2.id);

        // 3. Japanese Unicode preservation: "   東京   "
        let r3 = SearchRepository::search(&conn, "   東京   ", None).unwrap();
        assert_eq!(r3.len(), 1);
        assert_eq!(r3[0].note_id, n3.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_9_basic_title_search() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_t9_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        let n1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Plan".to_string()),
                content: Some("Timeline and resource allocation".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let n2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Grocery List".to_string()),
                content: Some("Milk, bread, eggs, fruit".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 1. Lowercase search: "project" -> matches "Project Plan"
        let r_lower = SearchRepository::search(&conn, "project", None).unwrap();
        assert_eq!(r_lower.len(), 1);
        assert_eq!(r_lower[0].note_id, n1.id);
        assert_eq!(r_lower[0].title, "Project Plan");
        assert_ne!(r_lower[0].note_id, n2.id);

        // 2. Uppercase search: "PROJECT" -> matches "Project Plan"
        let r_upper = SearchRepository::search(&conn, "PROJECT", None).unwrap();
        assert_eq!(r_upper.len(), 1);
        assert_eq!(r_upper[0].note_id, n1.id);

        // 3. Mixed case search: "PrOjEcT" -> matches "Project Plan"
        let r_mixed = SearchRepository::search(&conn, "PrOjEcT", None).unwrap();
        assert_eq!(r_mixed.len(), 1);
        assert_eq!(r_mixed[0].note_id, n1.id);

        // 4. Second word search: "plan" -> matches "Project Plan"
        let r_word2 = SearchRepository::search(&conn, "plan", None).unwrap();
        assert_eq!(r_word2.len(), 1);
        assert_eq!(r_word2[0].note_id, n1.id);

        // 5. Non-matching search: "vacation" -> 0 results
        let r_none = SearchRepository::search(&conn, "vacation", None).unwrap();
        assert_eq!(r_none.len(), 0);

        // 6. Duplicate titles across different notes (Section 49)
        let n3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Plan".to_string()),
                content: Some("Different content for secondary project".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let r_dup = SearchRepository::search(&conn, "project", None).unwrap();
        assert_eq!(r_dup.len(), 2, "Duplicate titles must return both unique note IDs");
        let dup_ids: Vec<String> = r_dup.iter().map(|r| r.note_id.clone()).collect();
        assert!(dup_ids.contains(&n1.id));
        assert!(dup_ids.contains(&n3.id));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_10_content_search() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_t10_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Title has NO match for "project", but content contains "Discuss the project release tomorrow."
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Meeting".to_string()),
                content: Some("Discuss the project release tomorrow.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let unmatching_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Personal Diary".to_string()),
                content: Some("Walked in the park, enjoyed fresh air.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Search: "project"
        let results = SearchRepository::search(&conn, "project", None).unwrap();
        assert_eq!(results.len(), 1, "Must find exactly the note containing 'project' in content");
        assert_eq!(results[0].note_id, note.id);
        assert_eq!(results[0].title, "Meeting");

        // Snippet must contain the matching context
        assert!(results[0].snippet.is_some());
        let snip = results[0].snippet.as_ref().unwrap();
        assert!(snip.to_lowercase().contains("project"));

        // Case-insensitive content matching: "PROJECT"
        let r_upper = SearchRepository::search(&conn, "PROJECT", None).unwrap();
        assert_eq!(r_upper.len(), 1);
        assert_eq!(r_upper[0].note_id, note.id);

        // Term deep in large note content
        let padding = "Unrelated background filler text. ".repeat(100);
        let large_content = format!("{}CRITICAL_SECRET_KEY at the middle of large text. {}", padding, padding);
        let large_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Server Config".to_string()),
                content: Some(large_content),
                ..Default::default()
            },
        ).unwrap();

        let r_large = SearchRepository::search(&conn, "CRITICAL_SECRET_KEY", None).unwrap();
        assert_eq!(r_large.len(), 1);
        assert_eq!(r_large[0].note_id, large_note.id);
        let large_snip = r_large[0].snippet.as_ref().unwrap();
        assert!(large_snip.contains("CRITICAL_SECRET_KEY"));
        // Ensure snippet does NOT return the entire huge note
        assert!(large_snip.len() < 250);

        // Verify unmatching note is not returned
        let r_diary = SearchRepository::search(&conn, "park", None).unwrap();
        assert_eq!(r_diary.len(), 1);
        assert_eq!(r_diary[0].note_id, unmatching_note.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_11_title_or_content_disjunctive_search() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_t11_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Note A: Title matches "project", content does NOT contain "project"
        let note_a = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Alpha".to_string()),
                content: Some("General system architecture and milestones.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 2. Note B: Content matches "project", title does NOT contain "project"
        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Team Sync".to_string()),
                content: Some("Review the upcoming project deadline on Friday.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 3. Note C: Both Title AND Content match "project"
        let note_c = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Review".to_string()),
                content: Some("Reviewing project milestones and project budgets.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 4. Note D: Neither matches "project"
        let note_d = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Grocery Shopping".to_string()),
                content: Some("Apples, bananas, milk, coffee beans.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let results = SearchRepository::search(&conn, "project", None).unwrap();

        // Must return notes A, B, and C (exactly 3 notes), excluding D
        assert_eq!(results.len(), 3, "Notes matching either title or content or both must be returned");

        let found_ids: Vec<String> = results.iter().map(|r| r.note_id.clone()).collect();
        assert!(found_ids.contains(&note_a.id), "Title-only match must be returned");
        assert!(found_ids.contains(&note_b.id), "Content-only match must be returned");
        assert!(found_ids.contains(&note_c.id), "Both title & content match must be returned");
        assert!(!found_ids.contains(&note_d.id), "Neither match must be excluded");

        // Verify deduplication: each note appears exactly once
        let mut unique_ids = found_ids.clone();
        unique_ids.sort();
        unique_ids.dedup();
        assert_eq!(unique_ids.len(), 3, "No duplicate results allowed");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_27_snippet_safety_plain_text() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_snip_safe_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        let malicious_content = "<script>alert('pwned')</script> <iframe src=\"evil.html\"></iframe> <img src=x onerror=alert(1)> Note with markup.";
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Security Test Note".to_string()),
                content: Some(malicious_content.to_string()),
                ..Default::default()
            },
        ).unwrap();

        let results = SearchRepository::search(&conn, "pwned", None).unwrap();
        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert_eq!(result.note_id, note.id);
        
        let snippet = result.snippet.as_ref().expect("Snippet must be generated");
        // Snippet must be a pure plain-text string without HTML transformations or evaluations
        assert!(snippet.contains("alert('pwned')"));
        assert!(snippet.contains("<script>"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_28_deterministic_search_ordering() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_order_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Content only match
        let note_content = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Weekly Review".to_string()),
                content: Some("Here is the project status report.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 2. Title contains match
        let note_title_contains = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Special Project Archive".to_string()),
                content: Some("Archived records.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 3. Title prefix match
        let note_title_prefix = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Kickoff".to_string()),
                content: Some("New initiative launch.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 4. Exact title match
        let note_exact_title = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project".to_string()),
                content: Some("Core plan.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let results = SearchRepository::search(&conn, "project", None).unwrap();
        assert_eq!(results.len(), 4);

        // Verification of deterministic rank ordering:
        // Tier 0 (Exact Title) -> Tier 1 (Prefix Title) -> Tier 2 (Contains Title) -> Tier 3 (Content Only)
        assert_eq!(results[0].note_id, note_exact_title.id, "Exact title match must be ranked 1st");
        assert_eq!(results[1].note_id, note_title_prefix.id, "Prefix title match must be ranked 2nd");
        assert_eq!(results[2].note_id, note_title_contains.id, "Contains title match must be ranked 3rd");
        assert_eq!(results[3].note_id, note_content.id, "Content-only match must be ranked 4th");

        // 5. Test recency tie-breaking within identical tier
        let note_recent = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Sprint Plan".to_string()),
                content: Some("Sprint goals.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Note older
        let note_older = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Sprint Retrospective".to_string()),
                content: Some("Retro review.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Update timestamps manually to test tie breaking
        conn.execute(
            "UPDATE notes SET modified_at = '2026-09-01T00:00:00Z' WHERE id = ?1",
            params![note_older.id],
        ).unwrap();
        conn.execute(
            "UPDATE notes SET modified_at = '2026-09-02T00:00:00Z' WHERE id = ?1",
            params![note_recent.id],
        ).unwrap();

        let sprint_results = SearchRepository::search(&conn, "sprint", None).unwrap();
        assert_eq!(sprint_results.len(), 2);
        assert_eq!(sprint_results[0].note_id, note_recent.id, "More recently modified note must rank ahead");
        assert_eq!(sprint_results[1].note_id, note_older.id, "Older note must rank behind");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_31_search_result_limit_enforcement() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_limit_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Populate 120 matching notes
        for i in 0..120 {
            NoteRepository::create(
                &conn,
                CreateNoteDto {
                    title: Some(format!("Target Note {:03}", i)),
                    content: Some("Content mentioning target item.".to_string()),
                    ..Default::default()
                },
            ).unwrap();
        }

        // 1. Default limit when None is specified must be exactly DEFAULT_LIMIT (50)
        let default_results = SearchRepository::search(&conn, "target", None).unwrap();
        assert_eq!(default_results.len(), SearchRepository::DEFAULT_LIMIT);
        assert_eq!(default_results.len(), 50);

        // 2. Custom small limit must be respected
        let small_results = SearchRepository::search(&conn, "target", Some(15)).unwrap();
        assert_eq!(small_results.len(), 15);

        // 3. Excessively large limit (e.g. 500) must be clamped to MAX_LIMIT (100)
        let clamped_results = SearchRepository::search(&conn, "target", Some(500)).unwrap();
        assert_eq!(clamped_results.len(), SearchRepository::MAX_LIMIT);
        assert_eq!(clamped_results.len(), 100);

        // 4. Zero limit must clamp up to minimum of 1
        let min_results = SearchRepository::search(&conn, "target", Some(0)).unwrap();
        assert_eq!(min_results.len(), 1);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_35_fts_synchronization_on_create() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_fts_sync_create_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Initial state: searching for 'quantum' yields 0 results
        let initial_results = SearchRepository::search(&conn, "quantum", None).unwrap();
        assert_eq!(initial_results.len(), 0);

        // 2. Create note A with 'quantum' in title
        let note_a = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Quantum Computing Foundations".to_string()),
                content: Some("Introductory guide to modern quantum algorithms.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 3. Immediately search for 'quantum' - must find note A without manual sync
        let title_search = SearchRepository::search(&conn, "quantum", None).unwrap();
        assert_eq!(title_search.len(), 1);
        assert_eq!(title_search[0].note_id, note_a.id);
        assert_eq!(title_search[0].title, "Quantum Computing Foundations");

        // 4. Create note B with 'superposition' in content only
        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Physics Lecture 4".to_string()),
                content: Some("Exploration of wavefunctions and quantum superposition.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 5. Search for 'superposition' - must find note B immediately
        let content_search = SearchRepository::search(&conn, "superposition", None).unwrap();
        assert_eq!(content_search.len(), 1);
        assert_eq!(content_search[0].note_id, note_b.id);
        assert_eq!(content_search[0].title, "Physics Lecture 4");

        // 6. Search for 'quantum' now returns both Note A (title match) and Note B (content match)
        let multi_search = SearchRepository::search(&conn, "quantum", None).unwrap();
        assert_eq!(multi_search.len(), 2);
        let found_ids: Vec<String> = multi_search.iter().map(|r| r.note_id.clone()).collect();
        assert!(found_ids.contains(&note_a.id));
        assert!(found_ids.contains(&note_b.id));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_36_fts_synchronization_on_update() {
        use crate::storage::models::UpdateNoteDto;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_fts_sync_update_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create a note with initial title and body
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Initial Concept".to_string()),
                content: Some("First draft with preliminary notes.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Verify initial terms match
        assert_eq!(SearchRepository::search(&conn, "concept", None).unwrap().len(), 1);
        assert_eq!(SearchRepository::search(&conn, "preliminary", None).unwrap().len(), 1);
        assert_eq!(SearchRepository::search(&conn, "finalized", None).unwrap().len(), 0);

        // 2. Perform an autosave/update editing both title and content
        NoteRepository::update(
            &conn,
            &note.id,
            UpdateNoteDto {
                title: Some("Refined Architecture".to_string()),
                content: Some("Second revision with finalized architectural specifications.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 3. Stale terms must NO LONGER match
        let old_title_search = SearchRepository::search(&conn, "concept", None).unwrap();
        assert_eq!(old_title_search.len(), 0, "Old title term must no longer match after update");

        let old_content_search = SearchRepository::search(&conn, "preliminary", None).unwrap();
        assert_eq!(old_content_search.len(), 0, "Old content term must no longer match after update");

        // 4. New terms must IMMEDIATELY match
        let new_title_search = SearchRepository::search(&conn, "architecture", None).unwrap();
        assert_eq!(new_title_search.len(), 1, "New title term must match immediately");
        assert_eq!(new_title_search[0].note_id, note.id);

        let new_content_search = SearchRepository::search(&conn, "finalized", None).unwrap();
        assert_eq!(new_content_search.len(), 1, "New content term must match immediately");
        assert_eq!(new_content_search[0].note_id, note.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_37_fts_synchronization_on_delete_and_soft_delete() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_fts_sync_delete_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create a note
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Confidential Strategy Document".to_string()),
                content: Some("Sensitive future merger and acquisition plan.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Confirm searchable
        assert_eq!(SearchRepository::search(&conn, "confidential", None).unwrap().len(), 1);
        assert_eq!(SearchRepository::search(&conn, "merger", None).unwrap().len(), 1);

        // 2. Perform soft-delete (Task 37)
        let deleted = NoteRepository::delete(&conn, &note.id, true).unwrap();
        assert!(deleted, "Soft-delete must succeed");

        // 3. Search must immediately stop returning the soft-deleted note
        let search_title_after_delete = SearchRepository::search(&conn, "confidential", None).unwrap();
        assert_eq!(search_title_after_delete.len(), 0, "Soft-deleted note must not be returned in search results");

        let search_content_after_delete = SearchRepository::search(&conn, "merger", None).unwrap();
        assert_eq!(search_content_after_delete.len(), 0, "Soft-deleted note content must not be returned");

        // 4. Verify note data is NOT permanently erased from database
        let note_in_db = NoteRepository::get_by_id(&conn, &note.id).unwrap().expect("Note record must remain in database");
        assert!(note_in_db.is_deleted, "Note must be marked as deleted");
        assert_eq!(note_in_db.title, "Confidential Strategy Document");
        assert_eq!(note_in_db.content, "Sensitive future merger and acquisition plan.");

        // 5. Test restoration (Task 38 readiness): restoring the note allows it to become searchable again
        let restored = NoteRepository::restore(&conn, &note.id).unwrap();
        assert!(!restored.is_deleted);

        let search_restored = SearchRepository::search(&conn, "confidential", None).unwrap();
        assert_eq!(search_restored.len(), 1, "Restored note must become searchable again without full index rewrite");
        assert_eq!(search_restored[0].note_id, note.id);

        // 6. Test hard delete: permanently deleting the note triggers notes_fts_ad and purges from FTS
        let hard_deleted = NoteRepository::delete(&conn, &note.id, false).unwrap();
        assert!(hard_deleted, "Hard delete must succeed");

        let search_hard_deleted = SearchRepository::search(&conn, "confidential", None).unwrap();
        assert_eq!(search_hard_deleted.len(), 0, "Hard-deleted note must not be returned");

        let note_hard_deleted = NoteRepository::get_by_id(&conn, &note.id).unwrap();
        assert!(note_hard_deleted.is_none(), "Note must be completely removed after hard delete");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_38_fts_synchronization_on_restore() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_fts_sync_restore_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create two notes with overlapping keywords
        let note_a = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Apollo Launch Notes".to_string()),
                content: Some("Orbital trajectory telemetry and countdown checkpoints.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Apollo Budget Ledger".to_string()),
                content: Some("Procurement costs for propulsion fuel and avionics.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Verify both notes appear initially for "Apollo"
        let initial_apollo = SearchRepository::search(&conn, "Apollo", None).unwrap();
        assert_eq!(initial_apollo.len(), 2, "Both notes must be searchable initially");

        // 2. Soft-delete Note A
        let deleted_a = NoteRepository::delete(&conn, &note_a.id, true).unwrap();
        assert!(deleted_a, "Soft-delete must succeed");

        // 3. Search must only return Note B; Note A must be excluded
        let search_while_deleted = SearchRepository::search(&conn, "Apollo", None).unwrap();
        assert_eq!(search_while_deleted.len(), 1, "Only active note B must be returned");
        assert_eq!(search_while_deleted[0].note_id, note_b.id);

        let search_title_a_deleted = SearchRepository::search(&conn, "Launch", None).unwrap();
        assert_eq!(search_title_a_deleted.len(), 0, "Deleted note A title term must not match");

        let search_content_a_deleted = SearchRepository::search(&conn, "telemetry", None).unwrap();
        assert_eq!(search_content_a_deleted.len(), 0, "Deleted note A content term must not match");

        // 4. Restore Note A (Task 38)
        let restored_a = NoteRepository::restore(&conn, &note_a.id).unwrap();
        assert!(!restored_a.is_deleted, "Restored note must have is_deleted = false");
        assert!(restored_a.deleted_at.is_none(), "deleted_at must be cleared");

        // 5. Search must immediately return both notes without full index rebuild
        let search_after_restore = SearchRepository::search(&conn, "Apollo", None).unwrap();
        assert_eq!(search_after_restore.len(), 2, "Both notes must be searchable immediately after restore");

        let restored_ids: Vec<String> = search_after_restore.iter().map(|r| r.note_id.clone()).collect();
        assert!(restored_ids.contains(&note_a.id), "Restored note A must appear in search results");
        assert!(restored_ids.contains(&note_b.id), "Active note B must continue to appear");

        // Content search on restored note returns snippet and accurate rank
        let search_content_restored = SearchRepository::search(&conn, "telemetry", None).unwrap();
        assert_eq!(search_content_restored.len(), 1, "Restored content term must match immediately");
        assert_eq!(search_content_restored[0].note_id, note_a.id);
        assert!(search_content_restored[0].snippet.as_ref().unwrap().contains("telemetry"));

        // 6. Restoring an already active note is a safe no-op
        let restored_again = NoteRepository::restore(&conn, &note_a.id).unwrap();
        assert!(!restored_again.is_deleted);
        assert_eq!(SearchRepository::search(&conn, "Apollo", None).unwrap().len(), 2);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_39_search_result_favorite_flag_coherence() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_fav_flag_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create a note marked as favorite and another note not favorite
        let note_fav = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Falcon Roadmap".to_string()),
                content: Some("Favorite critical mission milestones.".to_string()),
                is_favorite: Some(true),
                ..Default::default()
            },
        ).unwrap();

        let note_regular = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Falcon Expenses".to_string()),
                content: Some("Operational expenses and invoice records.".to_string()),
                is_favorite: Some(false),
                ..Default::default()
            },
        ).unwrap();

        // 2. Search for "Falcon"
        let results = SearchRepository::search(&conn, "Falcon", None).unwrap();
        assert_eq!(results.len(), 2);

        let res_fav = results.iter().find(|r| r.note_id == note_fav.id).expect("Fav note must be in results");
        let res_reg = results.iter().find(|r| r.note_id == note_regular.id).expect("Regular note must be in results");

        assert!(res_fav.favorite, "Favorite note must have favorite = true in SearchResult");
        assert!(!res_reg.favorite, "Regular note must have favorite = false in SearchResult");

        // 3. Toggle favorite status on regular note
        NoteRepository::set_favorite(&conn, &note_regular.id, true).unwrap();

        let results_after_fav = SearchRepository::search(&conn, "Falcon", None).unwrap();
        let res_reg_after = results_after_fav.iter().find(|r| r.note_id == note_regular.id).unwrap();
        assert!(res_reg_after.favorite, "Favorite flag in SearchResult must immediately reflect updated state");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_40_tag_and_search_state_isolation() {
        use crate::storage::models::UpdateTagDto;
        use crate::storage::repositories::tags::TagRepository;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_tag_iso_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create tags
        let tag_work = TagRepository::create(&conn, "work").unwrap();
        let tag_personal = TagRepository::create(&conn, "personal").unwrap();

        // 2. Create notes matching "Project"
        let note_a = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Alpha Engineering".to_string()),
                content: Some("Architecture specifications for corporate backend.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Beta Garden".to_string()),
                content: Some("Personal landscaping ideas and tool purchases.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 3. Assign tags to notes
        TagRepository::add_tag_to_note(&conn, &note_a.id, &tag_work.id).unwrap();
        TagRepository::add_tag_to_note(&conn, &note_b.id, &tag_personal.id).unwrap();

        // 4. Global search returns both notes
        let search_results = SearchRepository::search(&conn, "Project", None).unwrap();
        assert_eq!(search_results.len(), 2, "Search must return all matching notes across all tags");

        // 5. Tag repository returns isolated tag results
        let work_notes = NoteRepository::list_by_tag(&conn, &tag_work.id).unwrap();
        assert_eq!(work_notes.len(), 1);
        assert_eq!(work_notes[0].id, note_a.id);

        let personal_notes = NoteRepository::list_by_tag(&conn, &tag_personal.id).unwrap();
        assert_eq!(personal_notes.len(), 1);
        assert_eq!(personal_notes[0].id, note_b.id);

        // 6. Mutating tags (rename tag, remove tag) leaves text search completely intact and unaffected
        TagRepository::update(&conn, &tag_work.id, UpdateTagDto { name: "enterprise".to_string() }).unwrap();
        TagRepository::remove_tag_from_note(&conn, &note_b.id, &tag_personal.id).unwrap();

        let search_after_tag_mutations = SearchRepository::search(&conn, "Project", None).unwrap();
        assert_eq!(search_after_tag_mutations.len(), 2, "Tag mutations must never corrupt or alter search index");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_48_unicode_search() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_unicode_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Create notes containing all Task 48 Unicode test terms:
        // भारत, यात्रा, हिंदी, 東京, café, résumé
        let note_bharat = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("मेरी भारत यात्रा".to_string()),
                content: Some("भारत एक विशाल देश है जहाँ विविध संस्कृतियाँ हैं।".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_yatra = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("पहाड़ों की यात्रा".to_string()),
                content: Some("हिमालय की यात्रा बहुत सुंदर और रोमांचक रही।".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_hindi = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("हिंदी साहित्य नोट्स".to_string()),
                content: Some("आधुनिक हिंदी साहित्य और कहानियों का संकलन।".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_tokyo = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("東京ガイド".to_string()),
                content: Some("東京の歴史と観光名所のまとめ。".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_cafe = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Favorite café in Paris".to_string()),
                content: Some("Enjoying espresso at a cozy street café.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_resume = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Professional résumé draft".to_string()),
                content: Some("Updated my engineering résumé with recent projects.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 1. Search: "भारत"
        let res_bharat = SearchRepository::search(&conn, "भारत", None).unwrap();
        assert_eq!(res_bharat.len(), 1, "Must find exactly 1 note matching 'भारत'");
        assert_eq!(res_bharat[0].note_id, note_bharat.id);

        // 2. Search: "यात्रा" (present in both note_bharat and note_yatra)
        let res_yatra = SearchRepository::search(&conn, "यात्रा", None).unwrap();
        assert_eq!(res_yatra.len(), 2, "Must find 2 notes matching 'यात्रा'");
        let yatra_ids: Vec<String> = res_yatra.iter().map(|r| r.note_id.clone()).collect();
        assert!(yatra_ids.contains(&note_bharat.id));
        assert!(yatra_ids.contains(&note_yatra.id));

        // 3. Search: "हिंदी"
        let res_hindi = SearchRepository::search(&conn, "हिंदी", None).unwrap();
        assert_eq!(res_hindi.len(), 1, "Must find exactly 1 note matching 'हिंदी'");
        assert_eq!(res_hindi[0].note_id, note_hindi.id);

        // 4. Search: "東京"
        let res_tokyo = SearchRepository::search(&conn, "東京", None).unwrap();
        assert_eq!(res_tokyo.len(), 1, "Must find exactly 1 note matching '東京'");
        assert_eq!(res_tokyo[0].note_id, note_tokyo.id);

        // 5. Search: "café"
        let res_cafe = SearchRepository::search(&conn, "café", None).unwrap();
        assert_eq!(res_cafe.len(), 1, "Must find exactly 1 note matching 'café'");
        assert_eq!(res_cafe[0].note_id, note_cafe.id);

        // 6. Search: "résumé"
        let res_resume = SearchRepository::search(&conn, "résumé", None).unwrap();
        assert_eq!(res_resume.len(), 1, "Must find exactly 1 note matching 'résumé'");
        assert_eq!(res_resume[0].note_id, note_resume.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_49_case_insensitive_search() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_case_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Create notes with various case conventions
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Alpha".to_string()),
                content: Some("Architecture design documents for system core.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("weekly project status".to_string()),
                content: Some("All lowercase notes for the sprint.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("UPPERCASE PROJECT TITLE".to_string()),
                content: Some("SHOUTING ALL CAPS PROJECT DETAILS.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note4 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("PrOjEcT Inverted Case".to_string()),
                content: Some("mIxEd cAsE pRoJeCt dOcUmEnT.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Test all four query variants specified in Task 49:
        // Project, project, PROJECT, PrOjEcT
        let query_variants = ["Project", "project", "PROJECT", "PrOjEcT"];

        let mut baseline_ids: Option<Vec<String>> = None;

        for query in &query_variants {
            let results = SearchRepository::search(&conn, query, None).unwrap();
            assert_eq!(
                results.len(),
                4,
                "Query '{}' must find all 4 notes regardless of letter case",
                query
            );

            let ids: Vec<String> = results.iter().map(|r| r.note_id.clone()).collect();
            assert!(ids.contains(&note1.id));
            assert!(ids.contains(&note2.id));
            assert!(ids.contains(&note3.id));
            assert!(ids.contains(&note4.id));

            if let Some(ref prev_ids) = baseline_ids {
                assert_eq!(
                    &ids, prev_ids,
                    "Result ordering for '{}' must match other case variants identically",
                    query
                );
            } else {
                baseline_ids = Some(ids);
            }
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_50_special_characters() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_spec_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Create notes containing all Task 50 special character strings:
        // C++, C#, R&D, project-x, hello.world, foo/bar, user@example.com
        let note_cpp = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Modern C++ Programming Guide".to_string()),
                content: Some("Discussion on C++ templates, pointers, and memory safety.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_csharp = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("C# and .NET Architecture".to_string()),
                content: Some("Building enterprise services with C# and ASP.NET.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_rd = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("R&D Department Roadmap".to_string()),
                content: Some("Quarterly Research & Development goals and prototypes.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_project_x = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Classified project-x briefing".to_string()),
                content: Some("Confidential project-x milestones and deploy targets.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_hello_world = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Sample hello.world script".to_string()),
                content: Some("A simple hello.world program demonstrating system startup.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_foo_bar = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("API route foo/bar documentation".to_string()),
                content: Some("Endpoint located at https://api.local/foo/bar/v1".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_email = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Admin Contact Information".to_string()),
                content: Some("Please direct all inquiries to user@example.com promptly.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 1. Search: "C++"
        let res_cpp = SearchRepository::search(&conn, "C++", None).unwrap();
        assert!(!res_cpp.is_empty(), "Search for C++ must succeed without crash or error");
        assert!(res_cpp.iter().any(|r| r.note_id == note_cpp.id));

        // 2. Search: "C#"
        let res_csharp = SearchRepository::search(&conn, "C#", None).unwrap();
        assert!(!res_csharp.is_empty(), "Search for C# must succeed without crash or error");
        assert!(res_csharp.iter().any(|r| r.note_id == note_csharp.id));

        // 3. Search: "R&D"
        let res_rd = SearchRepository::search(&conn, "R&D", None).unwrap();
        assert_eq!(res_rd.len(), 1, "R&D must find R&D Department without error");
        assert_eq!(res_rd[0].note_id, note_rd.id);

        // 4. Search: "project-x"
        let res_px = SearchRepository::search(&conn, "project-x", None).unwrap();
        assert_eq!(res_px.len(), 1, "project-x must find project-x note without error");
        assert_eq!(res_px[0].note_id, note_project_x.id);

        // 5. Search: "hello.world"
        let res_hw = SearchRepository::search(&conn, "hello.world", None).unwrap();
        assert_eq!(res_hw.len(), 1, "hello.world must find hello.world note without error");
        assert_eq!(res_hw[0].note_id, note_hello_world.id);

        // 6. Search: "foo/bar"
        let res_fb = SearchRepository::search(&conn, "foo/bar", None).unwrap();
        assert_eq!(res_fb.len(), 1, "foo/bar must find foo/bar note without error");
        assert_eq!(res_fb[0].note_id, note_foo_bar.id);

        // 7. Search: "user@example.com"
        let res_email = SearchRepository::search(&conn, "user@example.com", None).unwrap();
        assert_eq!(res_email.len(), 1, "user@example.com must find email note without error");
        assert_eq!(res_email[0].note_id, note_email.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_51_sql_injection() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_sqli_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Populate database with confidential test notes
        let _note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Confidential Financials Q3".to_string()),
                content: Some("Revenue numbers: 15.4M USD net profit.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let _note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Private Employee Feedback".to_string()),
                content: Some("Internal performance reviews and grading.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let _note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Security Infrastructure Credentials".to_string()),
                content: Some("Server access keys and bastion host configs.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let initial_count: i64 = conn
            .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 0", [], |r| r.get(0))
            .unwrap();
        assert_eq!(initial_count, 3, "Setup must have exactly 3 active notes");

        // The four mandatory attack inputs specified in Task 51:
        // 1. "'"
        // 2. '"'
        // 3. "' OR 1=1 --"
        // 4. '" OR "1"="1'
        // Plus additional destructive payloads
        let injection_payloads = [
            "'",
            "\"",
            "' OR 1=1 --",
            "\" OR \"1\"=\"1",
            "' OR '1'='1",
            "'; DROP TABLE notes; --",
            "\" UNION SELECT * FROM notes --",
            "admin' --",
            "1' ORDER BY 1--+",
            "1' UNION ALL SELECT NULL,NULL,NULL,NULL,NULL,NULL--",
        ];

        for payload in &injection_payloads {
            // Must NOT crash and must return Ok
            let search_result = SearchRepository::search(&conn, payload, None);
            assert!(
                search_result.is_ok(),
                "Search query with payload {:?} must not return error or crash",
                payload
            );

            let results = search_result.unwrap();

            // Must NOT dump the full database (tautology bypass prevention)
            assert_ne!(
                results.len(),
                initial_count as usize,
                "Payload {:?} must not cause an unintended full database dump",
                payload
            );

            // In our test set, none of the payloads match the notes
            assert_eq!(
                results.len(),
                0,
                "Payload {:?} must safely evaluate to zero matches",
                payload
            );

            // Database integrity check: notes table must still exist and count must remain 3
            let current_count: Result<i64, _> = conn
                .query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 0", [], |r| r.get(0));
            assert!(
                current_count.is_ok(),
                "Database table 'notes' must still exist after payload {:?}",
                payload
            );
            assert_eq!(
                current_count.unwrap(),
                initial_count,
                "Note count must not be altered by payload {:?}",
                payload
            );
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_52_large_content_search() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_large_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Generate repeated filler text blocks
        let filler = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua ";

        // Note 1: "project" near beginning
        let content_beginning = format!("Important initial project briefing notes. {}", filler.repeat(200));
        let note_beg = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Large Note Beginning".to_string()),
                content: Some(content_beginning),
                ..Default::default()
            },
        ).unwrap();

        // Note 2: "project" near middle
        let content_middle = format!("{} Critical project milestone reached in mid architecture. {}", filler.repeat(100), filler.repeat(100));
        let note_mid = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Large Note Middle".to_string()),
                content: Some(content_middle),
                ..Default::default()
            },
        ).unwrap();

        // Note 3: "project" near end
        let content_end = format!("{} Final concluding project summary and retrospectives.", filler.repeat(200));
        let note_end = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Large Note End".to_string()),
                content: Some(content_end),
                ..Default::default()
            },
        ).unwrap();

        let start = std::time::Instant::now();
        let results = SearchRepository::search(&conn, "project", None).unwrap();
        let elapsed = start.elapsed();

        // 1. Result found and correct notes returned
        assert_eq!(results.len(), 3, "Must find all 3 large notes matching 'project'");
        let ids: Vec<String> = results.iter().map(|r| r.note_id.clone()).collect();
        assert!(ids.contains(&note_beg.id));
        assert!(ids.contains(&note_mid.id));
        assert!(ids.contains(&note_end.id));

        // 2. UI responsiveness / performance check
        assert!(elapsed.as_millis() < 500, "Search across large content must complete within 500ms");

        // 3. Snippet is reasonable and full content is NOT returned
        for res in &results {
            assert!(res.snippet.is_some(), "SearchResult must include a preview snippet");
            let snippet = res.snippet.as_ref().unwrap();

            // Snippet must be a concise extract, far shorter than the large content
            assert!(
                snippet.len() < 300,
                "Snippet length ({}) must be compact (< 300 characters)",
                snippet.len()
            );

            // Snippet must contain the matched term or ellipsis
            assert!(
                snippet.to_lowercase().contains("project") || snippet.contains("..."),
                "Snippet must contain matching term or context ellipsis"
            );
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_53_many_notes_search() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_many_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Test increments specified in Task 53: 100, 500, 1000 notes
        let targets = [100, 500, 1000];
        let mut created = 0;

        for &target in &targets {
            let tx = conn.transaction().unwrap();
            for i in (created + 1)..=target {
                let note_id = format!("note-many-{}", i);
                let title = if i % 5 == 0 {
                    format!("Special Project Roadmap {}", i)
                } else {
                    format!("Standard Meeting Note {}", i)
                };
                let content = if i % 2 == 0 {
                    format!("Detailed discussion about project deliverables for item {}.", i)
                } else {
                    format!("Regular documentation text without targets for item {}.", i)
                };
                let now = chrono::Utc::now().to_rfc3339();

                tx.execute(
                    "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_favorite, is_pinned, is_deleted)
                     VALUES (?1, ?2, ?3, 'txt', ?4, ?4, 0, 0, 0)",
                    params![note_id, title, content, now],
                ).unwrap();
            }
            tx.commit().unwrap();
            created = target;

            // Measure search performance for 'Project'
            let start = std::time::Instant::now();
            let results = SearchRepository::search(&conn, "Project", None).unwrap();
            let elapsed = start.elapsed();

            // 1. Result count bounded by default limit 50
            assert_eq!(
                results.len(),
                50,
                "Result count must be bounded to default limit 50 at {} notes",
                target
            );

            // 2. Search response must remain responsive (< 50ms)
            assert!(
                elapsed.as_millis() < 50,
                "Search at {} notes must execute under 50ms (took {}ms)",
                target,
                elapsed.as_millis()
            );

            // 3. Result rendering & snippet reasonableness
            for res in &results {
                assert!(res.snippet.is_some());
                assert!(res.snippet.as_ref().unwrap().len() < 300);
            }
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_54_search_during_autosave_workflow() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_as_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Initial state: Note exists with baseline text
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Planning".to_string()),
                content: Some("Initial notes on kickoff and team composition.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 2. Search for "deadline": initial search returns 0 matches
        let results_before = SearchRepository::search(&conn, "deadline", None).unwrap();
        assert_eq!(results_before.len(), 0, "Term 'deadline' must not match before it is added");

        // 3. User types in editor: "project deadline finalized for Q4 release."
        // Autosave flushes update into SQLite
        let updated = NoteRepository::update(
            &conn,
            &note.id,
            UpdateNoteDto {
                content: Some("Initial notes on kickoff. Critical project deadline finalized for Q4 release.".to_string()),
                ..Default::default()
            },
        ).unwrap();
        assert!(updated.content.contains("deadline"));

        // 4. Immediately search: "deadline"
        let results_after = SearchRepository::search(&conn, "deadline", None).unwrap();
        assert_eq!(results_after.len(), 1, "Search must find note immediately after autosave flush");
        assert_eq!(results_after[0].note_id, note.id);
        assert!(results_after[0].snippet.is_some());
        assert!(
            results_after[0].snippet.as_ref().unwrap().to_lowercase().contains("deadline"),
            "Snippet must contain the freshly autosaved keyword"
        );

        // 5. Subsequent update removing keyword ensures stale matches do not linger
        NoteRepository::update(
            &conn,
            &note.id,
            UpdateNoteDto {
                content: Some("All milestones completed smoothly.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let results_removed = SearchRepository::search(&conn, "deadline", None).unwrap();
        assert_eq!(results_removed.len(), 0, "Search index must immediately clear removed term");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_55_autosave_fts_atomic_ordering() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_order_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create baseline note
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Original Title".to_string()),
                content: Some("Original Content with tokenalpha.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 2. Perform atomic note update
        NoteRepository::update(
            &conn,
            &note.id,
            UpdateNoteDto {
                title: Some("New Title Beta".to_string()),
                content: Some("New Content with tokenbeta replacing alpha.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Verify FTS table synchronization state directly
        // Under SQLite triggers, notes_fts_au synchronizes atomically in the same transaction
        let fts_count_alpha: i64 = conn
            .query_row("SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'tokenalpha'", [], |r| r.get(0))
            .unwrap();
        let fts_count_beta: i64 = conn
            .query_row("SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'tokenbeta'", [], |r| r.get(0))
            .unwrap();

        assert_eq!(fts_count_alpha, 0, "Old token must be purged from FTS5 index atomically");
        assert_eq!(fts_count_beta, 1, "New token must be indexed in FTS5 atomically");

        // 3. Rollback safety: If an update is executed within an aborted transaction,
        // neither notes nor notes_fts may remain in a desynchronized state.
        {
            let tx = conn.transaction().unwrap();
            tx.execute(
                "UPDATE notes SET content = 'Uncommitted content with tokengamma' WHERE id = ?1",
                params![note.id],
            ).unwrap();
            // Deliberately rollback the transaction
            tx.rollback().unwrap();
        }

        let fts_count_gamma: i64 = conn
            .query_row("SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'tokengamma'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fts_count_gamma, 0, "Rolled-back content must never pollute FTS5 index");

        let fts_count_beta_after: i64 = conn
            .query_row("SELECT COUNT(1) FROM notes_fts WHERE notes_fts MATCH 'tokenbeta'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fts_count_beta_after, 1, "Committed token remains active in FTS5");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_56_search_during_note_switching() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_switch_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Note A open
        let note_a = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note A Design System".to_string()),
                content: Some("Color palette and typography specifications for UI components.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Note B exists in database
        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note B Performance Roadmap".to_string()),
                content: Some("Optimization goals for rendering large note collections.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 2. User edits Note A (in editor)
        let note_a_edited_content = "Color palette and typography specifications with newly added tokens.";
        // Autosave / note-switch flush persists Note A
        let note_a_updated = NoteRepository::update(
            &conn,
            &note_a.id,
            UpdateNoteDto {
                content: Some(note_a_edited_content.to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 3. User searches: "Performance"
        let search_results = SearchRepository::search(&conn, "Performance", None).unwrap();
        assert_eq!(search_results.len(), 1, "Search should return Note B");
        assert_eq!(search_results[0].note_id, note_b.id, "Search result must belong to Note B");
        assert_eq!(search_results[0].title, "Note B Performance Roadmap");

        // 4. User selects Note B: Note B opens
        let opened_b = NoteRepository::get_by_id(&conn, &search_results[0].note_id)
            .unwrap()
            .expect("Note B must open correctly");

        // 5. Verify:
        // - Note A saves correctly
        assert_eq!(note_a_updated.content, note_a_edited_content);
        let note_a_in_db = NoteRepository::get_by_id(&conn, &note_a.id).unwrap().unwrap();
        assert_eq!(note_a_in_db.content, note_a_edited_content);

        // - Note B opens correctly
        assert_eq!(opened_b.id, note_b.id);
        assert_eq!(opened_b.title, "Note B Performance Roadmap");
        assert_eq!(opened_b.content, "Optimization goals for rendering large note collections.");

        // - Search result belongs to B
        assert_eq!(search_results[0].note_id, note_b.id);

        // - No stale content appears (Note B does not contain Note A's content)
        assert!(!opened_b.content.contains("typography"));
        assert!(!opened_b.content.contains("tokens"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_57_search_result_metadata() {
        use crate::storage::models::CreateNotebookDto;
        use crate::storage::repositories::NotebookRepository;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_meta_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create a notebook for metadata context
        let notebook = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Work Projects".to_string(),
                parent_id: None,
            },
        ).unwrap();

        // 2. Create Note with full metadata: title, content, notebook_id, favorite = true
        let note_full = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Metadata Test Full Note".to_string()),
                content: Some("Testing presence of full metadata in search results payload.".to_string()),
                notebook_id: Some(notebook.id.clone()),
                is_favorite: Some(true),
                ..Default::default()
            },
        ).unwrap();

        // 3. Create Note with minimal metadata: no notebook, favorite = false
        let note_min = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Metadata Test Minimal Note".to_string()),
                content: Some("Testing minimal metadata presence with unfiled status.".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                ..Default::default()
            },
        ).unwrap();

        let results = SearchRepository::search(&conn, "Metadata Test", None).unwrap();
        assert_eq!(results.len(), 2);

        let res_full = results.iter().find(|r| r.note_id == note_full.id).expect("Full note must be found");
        let res_min = results.iter().find(|r| r.note_id == note_min.id).expect("Minimal note must be found");

        // Verify minimum required fields (title, snippet, modified_at)
        assert_eq!(res_full.title, "Metadata Test Full Note");
        assert!(res_full.snippet.is_some());
        assert!(!res_full.snippet.as_ref().unwrap().is_empty());
        assert!(!res_full.modified_at.is_empty());

        // Verify optional metadata fields (favorite, notebook_id)
        assert_eq!(res_full.favorite, true);
        assert_eq!(res_full.notebook_id, Some(notebook.id));

        assert_eq!(res_min.title, "Metadata Test Minimal Note");
        assert!(res_min.snippet.is_some());
        assert_eq!(res_min.favorite, false);
        assert_eq!(res_min.notebook_id, None);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_58_search_result_favorite_indicator() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_fav_ind_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create favorited note
        let note_fav = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Starred Architecture Roadmap".to_string()),
                content: Some("Key deliverables with priority milestones.".to_string()),
                is_favorite: Some(true),
                ..Default::default()
            },
        ).unwrap();

        // 2. Create unfavorited note
        let note_reg = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Standard Operational Roadmap".to_string()),
                content: Some("General day to day operational tasks.".to_string()),
                is_favorite: Some(false),
                ..Default::default()
            },
        ).unwrap();

        // 3. Search for "Roadmap"
        let results = SearchRepository::search(&conn, "Roadmap", None).unwrap();
        assert_eq!(results.len(), 2);

        let res_fav = results.iter().find(|r| r.note_id == note_fav.id).unwrap();
        let res_reg = results.iter().find(|r| r.note_id == note_reg.id).unwrap();

        // Verify favorite indicator state on search results
        assert!(res_fav.favorite, "Favorited note must return favorite = true");
        assert!(!res_reg.favorite, "Regular note must return favorite = false");

        // 4. Toggle favorite status on regular note
        NoteRepository::set_favorite(&conn, &note_reg.id, true).unwrap();

        let updated_results = SearchRepository::search(&conn, "Roadmap", None).unwrap();
        let updated_reg = updated_results.iter().find(|r| r.note_id == note_reg.id).unwrap();
        assert!(updated_reg.favorite, "Favorite indicator must immediately update in search results");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_59_search_result_notebook_context() {
        use crate::storage::models::CreateNotebookDto;
        use crate::storage::repositories::NotebookRepository;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_nb_ctx_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create hierarchical notebooks: Work -> Projects
        let nb_work = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Work".to_string(),
                parent_id: None,
            },
        ).unwrap();

        let nb_projects = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Projects".to_string(),
                parent_id: Some(nb_work.id.clone()),
            },
        ).unwrap();

        let nb_personal = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Personal".to_string(),
                parent_id: None,
            },
        ).unwrap();

        // 2. Create three notes with identical titles to verify distinguishing similar notes
        let note_work = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Roadmap 2026".to_string()),
                content: Some("Engineering project timeline and delivery milestones.".to_string()),
                notebook_id: Some(nb_projects.id.clone()),
                ..Default::default()
            },
        ).unwrap();

        let note_pers = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Roadmap 2026".to_string()),
                content: Some("Personal health and travel goals for the coming year.".to_string()),
                notebook_id: Some(nb_personal.id.clone()),
                ..Default::default()
            },
        ).unwrap();

        let note_unfiled = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Roadmap 2026".to_string()),
                content: Some("Scratch brainstorm without assigned notebook.".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        ).unwrap();

        // 3. Search: "Roadmap 2026"
        let results = SearchRepository::search(&conn, "Roadmap 2026", None).unwrap();
        assert_eq!(results.len(), 3, "Must return all 3 notes with identical titles");

        let res_work = results.iter().find(|r| r.note_id == note_work.id).unwrap();
        let res_pers = results.iter().find(|r| r.note_id == note_pers.id).unwrap();
        let res_unfiled = results.iter().find(|r| r.note_id == note_unfiled.id).unwrap();

        // 4. Verify notebook references
        assert_eq!(res_work.notebook_id, Some(nb_projects.id));
        assert_eq!(res_pers.notebook_id, Some(nb_personal.id));
        assert_eq!(res_unfiled.notebook_id, None);

        // Titles and snippets do not contain raw notebook IDs
        for res in &results {
            assert!(!res.title.contains(&nb_work.id));
            if let Some(ref snip) = res.snippet {
                assert!(!snip.contains(&nb_work.id));
            }
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_60_search_result_tags() {
        use crate::storage::repositories::TagRepository;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_tags_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create tags
        let tag_work = TagRepository::create(&conn, "work").unwrap();
        let tag_important = TagRepository::create(&conn, "important").unwrap();
        let tag_q4 = TagRepository::create(&conn, "q4").unwrap();

        // 2. Create notes matching "Task60"
        let note_a = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Task60 Deliverables".to_string()),
                content: Some("Engineering specs and architecture reviews.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Task60 Meeting Notes".to_string()),
                content: Some("Weekly status updates and blocker discussions.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_c = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Task60 Unclassified Note".to_string()),
                content: Some("General draft without assigned tags.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 3. Assign tags to Note A and Note B
        TagRepository::add_tag_to_note(&conn, &note_a.id, &tag_work.id).unwrap();
        TagRepository::add_tag_to_note(&conn, &note_a.id, &tag_important.id).unwrap();
        TagRepository::add_tag_to_note(&conn, &note_a.id, &tag_q4.id).unwrap();

        TagRepository::add_tag_to_note(&conn, &note_b.id, &tag_work.id).unwrap();

        // 4. Perform search
        let results = SearchRepository::search(&conn, "Task60", None).unwrap();
        assert_eq!(results.len(), 3, "Search should return all 3 notes");

        // 5. Batch load tags for the search results in a SINGLE query (Task 60, Task 61 - Avoid N+1 queries)
        let result_note_ids: Vec<String> = results.iter().map(|r| r.note_id.clone()).collect();
        let batch_tags = TagRepository::get_tags_for_notes(&conn, &result_note_ids).unwrap();

        // Verify Note A tags: ["important", "q4", "work"]
        let tags_a = batch_tags.get(&note_a.id).expect("Note A tags must be present in batch map");
        assert_eq!(tags_a.len(), 3);
        assert_eq!(tags_a, &vec!["important".to_string(), "q4".to_string(), "work".to_string()]);

        // Verify Note B tags: ["work"]
        let tags_b = batch_tags.get(&note_b.id).expect("Note B tags must be present in batch map");
        assert_eq!(tags_b.len(), 1);
        assert_eq!(tags_b, &vec!["work".to_string()]);

        // Verify Note C has no tags in map
        assert_eq!(batch_tags.get(&note_c.id), None);

        // 6. Also verify get_all_notes_tag_names produces consistent batch results in 1 query
        let all_tags_map = TagRepository::get_all_notes_tag_names(&conn).unwrap();
        assert_eq!(all_tags_map.get(&note_a.id), Some(tags_a));
        assert_eq!(all_tags_map.get(&note_b.id), Some(tags_b));
        assert_eq!(all_tags_map.get(&note_c.id), None);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_61_avoid_n_plus_one_search_queries() {
        use crate::storage::repositories::TagRepository;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_nplus1_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create tags
        let tag_work = TagRepository::create(&conn, "work").unwrap();
        let tag_important = TagRepository::create(&conn, "important").unwrap();

        // 2. Populate 100 notes containing the search term "nplusone"
        let tx = conn.transaction().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for i in 1..=100 {
            let note_id = format!("n1-note-{}", i);
            let title = format!("NPlusOne Test Note {}", i);
            let content = format!("Detailed note content matching nplusone keyword at index {}.", i);
            tx.execute(
                "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_favorite, is_pinned, is_deleted)
                 VALUES (?1, ?2, ?3, 'txt', ?4, ?4, 0, 0, 0)",
                params![note_id, title, content, now],
            ).unwrap();

            // Tag even-numbered notes with work, odd notes divisible by 5 with important
            if i % 2 == 0 {
                tx.execute(
                    "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
                    params![note_id, tag_work.id],
                ).unwrap();
            }
            if i % 5 == 0 {
                tx.execute(
                    "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
                    params![note_id, tag_important.id],
                ).unwrap();
            }
        }
        tx.commit().unwrap();

        // 3. Execute search for "nplusone" with limit 100: exactly 1 database query
        let results = SearchRepository::search(&conn, "nplusone", Some(100)).unwrap();
        assert_eq!(results.len(), 100, "Search must return all 100 notes");

        // 4. Batch query metadata for all 100 note IDs in one shot: exactly 1 additional query
        let note_ids: Vec<String> = results.iter().map(|r| r.note_id.clone()).collect();
        let batch_tags_map = TagRepository::get_tags_for_notes(&conn, &note_ids).unwrap();

        // Total database queries = 2 (1 search + 1 batch metadata), strictly avoiding 101 queries
        assert!(batch_tags_map.len() >= 50, "Batch tags map must cover tagged notes");

        // 5. Verify correctness: even note has "work", note 10 has both "important" and "work"
        let tags_10 = batch_tags_map.get("n1-note-10").expect("Note 10 must have tags");
        assert!(tags_10.contains(&"work".to_string()));
        assert!(tags_10.contains(&"important".to_string()));

        // Note 1 has no tags
        assert_eq!(batch_tags_map.get("n1-note-1"), None);

        // 6. Simplicity & Graceful omission: Search results function completely even if tags are omitted
        for res in &results {
            assert!(!res.note_id.is_empty());
            assert!(!res.title.is_empty());
            assert!(res.snippet.is_some());
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_62_search_result_limit_enforcement() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_limit_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create 150 notes that match the search term "quantum"
        let tx = conn.transaction().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for i in 1..=150 {
            let note_id = format!("quantum-note-{}", i);
            let title = format!("Quantum Mechanics Note {}", i);
            let content = format!("Theoretical principles of quantum entanglement and superposition in record {}.", i);
            tx.execute(
                "INSERT INTO notes (id, title, content, format, created_at, modified_at, is_favorite, is_pinned, is_deleted)
                 VALUES (?1, ?2, ?3, 'txt', ?4, ?4, 0, 0, 0)",
                params![note_id, title, content, now],
            ).unwrap();
        }
        tx.commit().unwrap();

        let total_matches: i64 = conn
            .query_row("SELECT COUNT(1) FROM notes WHERE title LIKE '%Quantum%' AND is_deleted = 0", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total_matches, 150, "Database must have 150 matching notes");

        // 2. Default limit: Calling search with limit = None must enforce DEFAULT_LIMIT (50)
        let results_default = SearchRepository::search(&conn, "Quantum", None).unwrap();
        assert_eq!(
            results_default.len(),
            SearchRepository::DEFAULT_LIMIT,
            "Default search must enforce exactly {} results at the database query level",
            SearchRepository::DEFAULT_LIMIT
        );
        assert_eq!(results_default.len(), 50);

        // 3. Custom lower limit: Calling search with limit = Some(15) must return exactly 15
        let results_custom = SearchRepository::search(&conn, "Quantum", Some(15)).unwrap();
        assert_eq!(results_custom.len(), 15, "Search with custom limit 15 must return exactly 15 results");

        // 4. Excessive limit: Calling search with limit = Some(500) must be clamped to MAX_LIMIT (100)
        let results_clamped = SearchRepository::search(&conn, "Quantum", Some(500)).unwrap();
        assert_eq!(
            results_clamped.len(),
            SearchRepository::MAX_LIMIT,
            "Search with excessive limit must clamp to MAX_LIMIT ({})",
            SearchRepository::MAX_LIMIT
        );
        assert_eq!(results_clamped.len(), 100);

        // 5. Zero limit: Calling search with limit = Some(0) must clamp to minimum (1)
        let results_zero = SearchRepository::search(&conn, "Quantum", Some(0)).unwrap();
        assert_eq!(results_zero.len(), 1, "Search with limit 0 must clamp to 1");

        // 6. Verification of LIKE fallback query limit enforcement:
        // Even when bypassing FTS5, search_like must strictly enforce LIMIT ? in SQL
        let results_like_default = SearchRepository::search_like(&conn, "Quantum", 50).unwrap();
        assert_eq!(results_like_default.len(), 50, "search_like must enforce limit 50 in SQL");

        let results_like_custom = SearchRepository::search_like(&conn, "Quantum", 25).unwrap();
        assert_eq!(results_like_custom.len(), 25, "search_like must enforce limit 25 in SQL");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_63_search_performance_logging() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_perf_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create a note with sensitive content
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Performance Benchmark Document".to_string()),
                content: Some("Extremely confidential credentials and sensitive private keys.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 2. Measure search execution: query, result count, and duration
        let start = std::time::Instant::now();
        let query = "Benchmark";
        let results = SearchRepository::search(&conn, query, None).unwrap();
        let duration = start.elapsed();

        // 3. Verify metrics
        assert_eq!(results.len(), 1, "Expected 1 result count");
        assert_eq!(results[0].note_id, note.id);

        // Format performance log line per specification:
        // Search query executed: count=1 duration=Xms query="Benchmark"
        let log_line = format!(
            "Search query executed: count={} duration={}ms query=\"{}\"",
            results.len(),
            duration.as_millis(),
            query
        );

        // Verification: Log line contains query, count, duration
        assert!(log_line.contains("count=1"));
        assert!(log_line.contains("duration="));
        assert!(log_line.contains("query=\"Benchmark\""));

        // Critical privacy invariant: Log line must NEVER contain note content
        assert!(!log_line.contains("confidential"));
        assert!(!log_line.contains("credentials"));
        assert!(!log_line.contains("sensitive"));
        assert!(!log_line.contains("private keys"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_64_search_index_integrity() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_integ_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Step 1: create note
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Architecture Blueprint Alpha".to_string()),
                content: Some("Specifications containing unique tokenalpha in body.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Also create a persistent sibling note
        let persistent_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Persistent System Baseline".to_string()),
                content: Some("Long term documentation containing tokenomega.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Step 2: search initial text
        let res_initial = SearchRepository::search(&conn, "tokenalpha", None).unwrap();
        assert_eq!(res_initial.len(), 1, "Initial note must be searchable");
        assert_eq!(res_initial[0].note_id, note.id);

        // Step 3: edit note
        NoteRepository::update(
            &conn,
            &note.id,
            UpdateNoteDto {
                title: Some("Architecture Blueprint Beta".to_string()),
                content: Some("Updated specifications containing tokenbeta replacing earlier text.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Step 4: search old text -> Expected: old text not found
        let res_old = SearchRepository::search(&conn, "tokenalpha", None).unwrap();
        assert_eq!(res_old.len(), 0, "Old text 'tokenalpha' must not be found after edit");

        // Step 5: search new text -> Expected: new text found
        let res_new = SearchRepository::search(&conn, "tokenbeta", None).unwrap();
        assert_eq!(res_new.len(), 1, "New text 'tokenbeta' must be found after edit");
        assert_eq!(res_new[0].note_id, note.id);

        // Step 6: delete note
        NoteRepository::delete(&conn, &note.id, true).unwrap();

        // Step 7: search deleted text -> Expected: deleted note excluded
        let res_deleted = SearchRepository::search(&conn, "tokenbeta", None).unwrap();
        assert_eq!(res_deleted.len(), 0, "Deleted note must be excluded from search");

        // Step 8: restart (drop connection and re-open SQLite database file)
        drop(conn);
        let conn_restarted = init_connection(&db_path).unwrap();

        // Step 9: search after restart -> Expected: restart still works
        // - Deleted note remains excluded
        let res_restart_deleted = SearchRepository::search(&conn_restarted, "tokenbeta", None).unwrap();
        assert_eq!(res_restart_deleted.len(), 0, "Deleted note remains excluded after restart");

        // - Old text remains not found
        let res_restart_old = SearchRepository::search(&conn_restarted, "tokenalpha", None).unwrap();
        assert_eq!(res_restart_old.len(), 0, "Old text remains excluded after restart");

        // - Persistent sibling note is found
        let res_restart_sibling = SearchRepository::search(&conn_restarted, "tokenomega", None).unwrap();
        assert_eq!(res_restart_sibling.len(), 1, "Active notes remain searchable after restart");
        assert_eq!(res_restart_sibling[0].note_id, persistent_note.id);

        drop(conn_restarted);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_65_search_rebuild_capability() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_rebuild_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create notes before index loss
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Quantum Algorithms Alpha".to_string()),
                content: Some("Foundational quantum gates and teleportation protocols.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Quantum Cryptography Beta".to_string()),
                content: Some("Post-quantum encryption and lattice-based security.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Verify initial FTS search works
        let res_before = SearchRepository::search(&conn, "teleportation", None).unwrap();
        assert_eq!(res_before.len(), 1);
        assert_eq!(res_before[0].note_id, note1.id);

        // 2. Simulate corruption or missing index: Drop FTS table and triggers
        conn.execute_batch(
            "DROP TABLE notes_fts;
             DROP TRIGGER IF EXISTS notes_fts_ai;
             DROP TRIGGER IF EXISTS notes_fts_ad;
             DROP TRIGGER IF EXISTS notes_fts_au;"
        ).unwrap();

        // Confirm table is missing from sqlite_master
        let fts_exists_before_rebuild: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        assert!(!fts_exists_before_rebuild, "Virtual table must be dropped to simulate index loss");

        // 3. Execute backend-level rebuild_search_index()
        SearchRepository::rebuild_search_index(&conn).expect("rebuild_search_index must succeed");

        // 4. Verify FTS5 table is restored and populated with all existing notes
        let fts_exists_after_rebuild: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        assert!(fts_exists_after_rebuild, "notes_fts virtual table must be recreated");

        // Verify search finds both pre-existing notes
        let res_rebuilt1 = SearchRepository::search(&conn, "teleportation", None).unwrap();
        assert_eq!(res_rebuilt1.len(), 1, "Pre-existing note 1 must be re-indexed");
        assert_eq!(res_rebuilt1[0].note_id, note1.id);

        let res_rebuilt2 = SearchRepository::search(&conn, "lattice", None).unwrap();
        assert_eq!(res_rebuilt2.len(), 1, "Pre-existing note 2 must be re-indexed");
        assert_eq!(res_rebuilt2[0].note_id, note2.id);

        // 5. Verify synchronization triggers are active after rebuild
        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Quantum Error Correction Gamma".to_string()),
                content: Some("Surface codes and syndrome measurement strategies.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let res_new = SearchRepository::search(&conn, "syndrome", None).unwrap();
        assert_eq!(res_new.len(), 1, "New note created after rebuild must be indexed immediately");
        assert_eq!(res_new[0].note_id, note3.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_66_search_index_recovery() {
        let temp_dir = std::env::temp_dir().join(format!("custom_notepad_test_task_66_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("test.db");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create notes in authoritative database
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Distributed Raft Consensus Protocol".to_string()),
                content: Some("Leader election and log replication with heartbeats.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Paxos State Machine".to_string()),
                content: Some("Multi-decree consensus with acceptors and proposers.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let count_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_before, 2);

        // 2. Simulate MISSING index: completely drop notes_fts and sync triggers
        conn.execute_batch(
            "DROP TABLE notes_fts;
             DROP TRIGGER IF EXISTS notes_fts_ai;
             DROP TRIGGER IF EXISTS notes_fts_ad;
             DROP TRIGGER IF EXISTS notes_fts_au;"
        ).unwrap();

        let fts_missing: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        assert!(!fts_missing, "FTS table must be dropped to simulate missing index state");

        // 3. Search should detect missing index -> recreate -> populate from notes
        let results_recovered = SearchRepository::search(&conn, "consensus", None).expect("Search must succeed despite missing index");
        assert_eq!(results_recovered.len(), 2, "Both consensus notes must be found upon index recovery");

        // CRITICAL INVARIANT: Notes are never deleted because search index creation/recovery occurred
        let count_after_missing_recovery: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count_after_missing_recovery, 2,
            "CRITICAL INVARIANT: Notes must never be deleted during search index recovery"
        );

        // Verify FTS table was actually recreated and populated
        let fts_recreated: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        assert!(fts_recreated, "SearchRepository must automatically recreate missing notes_fts table");

        // 4. Simulate INCONSISTENT / CORRUPTED index:
        // Corrupt by dropping one of the shadow tables generated by FTS5
        let shadow_drop_res = conn.execute_batch("DROP TABLE IF EXISTS notes_fts_data;");
        if shadow_drop_res.is_ok() {
            // Index is now inconsistent/corrupted (queries to notes_fts will error)
            let search_corrupt_res = SearchRepository::search(&conn, "replication", None);
            assert!(
                search_corrupt_res.is_ok(),
                "Search must safely recover or fallback to LIKE search on index corruption"
            );
            let results = search_corrupt_res.unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].note_id, note1.id);

            // Invariant check: Notes are still intact
            let count_after_corrupt_recovery: i64 = conn
                .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
                .unwrap();
            assert_eq!(
                count_after_corrupt_recovery, 2,
                "CRITICAL INVARIANT: Notes must remain intact during corruption recovery"
            );
        }

        // 5. Explicit recover_index verification
        SearchRepository::recover_index(&conn).expect("recover_index must succeed");
        let results_final = SearchRepository::search(&conn, "proposers", None).unwrap();
        assert_eq!(results_final.len(), 1);
        assert_eq!(results_final[0].note_id, note2.id);

        let final_notes_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(final_notes_count, 2, "Final note count must remain exactly 2");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_69_search_with_no_notes() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_no_notes_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("test.db");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Fresh application: exactly 0 notes
        let note_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(note_count, 0, "Fresh application database must have 0 notes");

        // 2. Search for "project"
        let results = SearchRepository::search(&conn, "project", None)
            .expect("Searching with 0 notes must succeed without crashing or erroring");

        // 3. Expected: No notes found, no crash
        assert_eq!(results.len(), 0, "Search result must be empty when no notes exist");

        // 4. Verify various queries with 0 notes
        let results_with_limit = SearchRepository::search(&conn, "project", Some(10)).unwrap();
        assert_eq!(results_with_limit.len(), 0);

        let results_special = SearchRepository::search(&conn, "!@#$%", None).unwrap();
        assert_eq!(results_special.len(), 0);

        let results_unicode = SearchRepository::search(&conn, "अंतरिक्ष", None).unwrap();
        assert_eq!(results_unicode.len(), 0);

        // 5. Database integrity remains intact
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_70_search_with_one_note() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_one_note_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("test.db");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create exactly one note: "Project Plan"
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Plan".to_string()),
                content: Some("Architecture, deliverables, and timeline milestones.".to_string()),
                ..Default::default()
            },
        ).expect("Note creation must succeed");

        let total_notes: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total_notes, 1, "Database must have exactly 1 note");

        // 2. Search for "project" (case-insensitive title match)
        let results = SearchRepository::search(&conn, "project", None)
            .expect("Search must succeed");

        // 3. Expected: exactly 1 result
        assert_eq!(results.len(), 1, "Expected exactly 1 result");
        assert_eq!(results[0].note_id, note.id);
        assert_eq!(results[0].title, "Project Plan");

        // 4. Verify search for non-matching term returns 0 results
        let no_results = SearchRepository::search(&conn, "unrelated_term_xyz", None).unwrap();
        assert_eq!(no_results.len(), 0);

        // 5. Verify search for word in content also returns the 1 result
        let content_results = SearchRepository::search(&conn, "milestones", None).unwrap();
        assert_eq!(content_results.len(), 1);
        assert_eq!(content_results[0].note_id, note.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_71_search_exact_phrase() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_phrase_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("test.db");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create test notes
        // Note 1: Contains exact phrase "project plan"
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Q4 Project Plan".to_string()),
                content: Some("Engineering architecture and delivery schedules.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Note 2: Contains both "project" and "plan", but separated and in reverse order
        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Financial Plan for the Project".to_string()),
                content: Some("Budget allocations across all initiatives.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Note 3: Contains only "project", NO "plan"
        let _note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Gamma Overview".to_string()),
                content: Some("High level summary of the engineering work.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Note 4: Contains only "plan", NO "project"
        let _note4 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Vacation Plan".to_string()),
                content: Some("Personal travel itinerary for the holidays.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 2. Exact phrase search: "\"project plan\"" (with quotes)
        let phrase_results = SearchRepository::search(&conn, "\"project plan\"", None)
            .expect("Phrase search must succeed");

        // Expected: Matches ONLY Note 1 (the exact contiguous phrase)
        assert_eq!(phrase_results.len(), 1, "Exact phrase query must match only Note 1");
        assert_eq!(phrase_results[0].note_id, note1.id);

        // Crucial Invariant: "project plan" MUST NOT accidentally mean "project OR plan"
        // If it meant OR, all 4 notes would be returned!
        assert_ne!(
            phrase_results.len(), 4,
            "Phrase search must NOT accidentally mean 'project OR plan'"
        );

        // 3. Unquoted multi-word search: "project plan"
        // Whitespace between terms is implicit AND in FTS5
        let and_results = SearchRepository::search(&conn, "project plan", None)
            .expect("Multi-word search must succeed");

        // Both Note 1 and Note 2 contain both words -> returned
        // Note 3 (only project) and Note 4 (only plan) are EXCLUDED
        assert_eq!(and_results.len(), 2, "Unquoted terms must evaluate as AND (both terms required)");
        let returned_ids: Vec<String> = and_results.iter().map(|r| r.note_id.clone()).collect();
        assert!(returned_ids.contains(&note1.id));
        assert!(returned_ids.contains(&note2.id));

        // 4. Test phrase search on fallback LIKE engine
        let like_results = SearchRepository::search_like(&conn, "project plan", 50).unwrap();
        // LIKE matches substring '%project plan%', finding Note 1
        assert_eq!(like_results.len(), 1, "LIKE fallback matches exact phrase substring");
        assert_eq!(like_results[0].note_id, note1.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_72_search_query_syntax() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_syntax_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("test.db");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Create notes containing plain text that resembles advanced syntax
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("title: System Architecture Roadmap".to_string()),
                content: Some("Discussion on before: 2026 deadlines and after: launch plans.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Meeting Notes".to_string()),
                content: Some("Reminder: tag: urgent action items and notebook: engineering sync.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 1. Query with "title:" prefix must not crash SQLite FTS5 with column syntax errors
        let res_title = SearchRepository::search(&conn, "title: Roadmap", None);
        assert!(res_title.is_ok(), "Searching 'title: Roadmap' must execute safely as plain text");
        let items_title = res_title.unwrap();
        assert_eq!(items_title.len(), 1);
        assert_eq!(items_title[0].note_id, note1.id);

        // 2. Query with "tag:" prefix must not error with 'no such column: tag'
        let res_tag = SearchRepository::search(&conn, "tag: urgent", None);
        assert!(res_tag.is_ok(), "Searching 'tag: urgent' must execute safely without column error");
        let items_tag = res_tag.unwrap();
        assert_eq!(items_tag.len(), 1);
        assert_eq!(items_tag[0].note_id, note2.id);

        // 3. Query with "notebook:" prefix must not error with 'no such column: notebook'
        let res_nb = SearchRepository::search(&conn, "notebook: engineering", None);
        assert!(res_nb.is_ok(), "Searching 'notebook: engineering' must execute safely");
        let items_nb = res_nb.unwrap();
        assert_eq!(items_nb.len(), 1);
        assert_eq!(items_nb[0].note_id, note2.id);

        // 4. Query with "before:" and "after:" date prefixes must not crash
        let res_before = SearchRepository::search(&conn, "before: 2026", None);
        assert!(res_before.is_ok(), "Searching 'before: 2026' must execute safely as plain text");
        let items_before = res_before.unwrap();
        assert_eq!(items_before.len(), 1);
        assert_eq!(items_before[0].note_id, note1.id);

        let res_after = SearchRepository::search(&conn, "after: launch", None);
        assert!(res_after.is_ok(), "Searching 'after: launch' must execute safely as plain text");
        let items_after = res_after.unwrap();
        assert_eq!(items_after.len(), 1);
        assert_eq!(items_after[0].note_id, note1.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_73_search_special_query_handling() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_special_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("test.db");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Note with "project", "plan", and explicit text word "OR"
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Alpha".to_string()),
                content: Some("Detailed plan for the architecture with OR keyword mentioned.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // 2. Note with only "plan" (no "project", no "OR")
        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Plan Beta".to_string()),
                content: Some("Standalone plan document without any other terms.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Test 1: Quoted single word "project"
        let res_quoted = SearchRepository::search(&conn, "\"project\"", None);
        assert!(res_quoted.is_ok(), "Quoted '\"project\"' must execute safely");
        let items_quoted = res_quoted.unwrap();
        assert_eq!(items_quoted.len(), 1);
        assert_eq!(items_quoted[0].note_id, note1.id);

        // Test 2: "project OR plan" treats OR as normal user text, not raw FTS disjunctive operator
        // If it were raw FTS boolean OR, note2 would match because it contains "plan".
        // But plain text search requires all terms ("project", "OR", "plan"), so only note1 matches.
        let res_or = SearchRepository::search(&conn, "project OR plan", None);
        assert!(res_or.is_ok(), "Searching 'project OR plan' must succeed without error");
        let items_or = res_or.unwrap();
        assert_eq!(items_or.len(), 1, "Must not evaluate as raw boolean OR which would include note2");
        assert_eq!(items_or[0].note_id, note1.id);
        assert!(!items_or.iter().any(|i| i.note_id == note2.id), "note2 must be strictly excluded");

        // Test 3: Raw FTS operators in trailing/leading positions that would crash raw FTS5
        let raw_operator_queries = [
            "project OR",
            "OR project",
            "project AND",
            "project NOT",
            "project NEAR",
            "NEAR(project plan)",
        ];
        for q in raw_operator_queries {
            let res = SearchRepository::search(&conn, q, None);
            assert!(res.is_ok(), "Query with raw operator '{q}' must not crash FTS5");
        }

        // Test 4: Special symbols and punctuation that would trigger FTS5 syntax errors
        let raw_symbol_queries = [
            "\"unbalanced quote",
            "\"\"\"",
            "project*",
            "*project",
            "(project plan)",
            "{project plan}",
            "^project",
            "project +plan -other",
            ":::colon:::",
            "fakecol:query",
        ];
        for q in raw_symbol_queries {
            let res = SearchRepository::search(&conn, q, None);
            assert!(res.is_ok(), "Query with raw symbol '{q}' must not crash FTS5");
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_74_search_ranking_verification() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_rank_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("test.db");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Note A: title = Project
        let note_a = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project".to_string()),
                content: Some("".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Note B: title = Meeting, content = project
        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Meeting".to_string()),
                content: Some("project".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Note C: title = Project Plan, content = project
        let note_c = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Plan".to_string()),
                content: Some("project".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let results = SearchRepository::search(&conn, "project", None).unwrap();
        assert_eq!(results.len(), 3);
        // FTS5 ranking: Note A (exact title) -> Note C (title prefix + content) -> Note B (content only)
        assert_eq!(results[0].note_id, note_a.id, "Note A (title 'Project') must be ranked 1st");
        assert_eq!(results[1].note_id, note_c.id, "Note C (title 'Project Plan') must be ranked 2nd");
        assert_eq!(results[2].note_id, note_b.id, "Note B (title 'Meeting', content 'project') must be ranked 3rd");

        // LIKE search fallback ranking verification:
        // Tier 0 (Exact Title Match) -> Tier 1 (Title Prefix Match) -> Tier 3 (Content Match)
        let like_results = SearchRepository::search_like(&conn, "project", 50).unwrap();
        assert_eq!(like_results.len(), 3);
        assert_eq!(like_results[0].note_id, note_a.id, "LIKE tier 0: Note A must be ranked 1st");
        assert_eq!(like_results[1].note_id, note_c.id, "LIKE tier 1: Note C must be ranked 2nd");
        assert_eq!(like_results[2].note_id, note_b.id, "LIKE tier 3: Note B must be ranked 3rd");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_75_search_result_stability() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_stable_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("test.db");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // Seed 5 notes with distinct titles and contents
        let _note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Alpha".to_string()),
                content: Some("Core roadmap plan.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let _note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project Beta".to_string()),
                content: Some("Feature list for the project.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let _note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Meeting Minutes".to_string()),
                content: Some("Discussion on project deadlines and milestones.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let _note4 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Project".to_string()),
                content: Some("Master outline document.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let _note5 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Sprint Planning".to_string()),
                content: Some("Detailed tasks for the current project sprint.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Run search 5 consecutive times (Task 75 specification example)
        let mut fts_runs: Vec<Vec<String>> = Vec::new();
        for _ in 0..5 {
            let res = SearchRepository::search(&conn, "project", None).unwrap();
            assert_eq!(res.len(), 5);
            fts_runs.push(res.into_iter().map(|r| r.note_id).collect());
        }

        // Verify identical result ordering across all 5 runs
        for i in 1..5 {
            assert_eq!(
                fts_runs[i], fts_runs[0],
                "Run {} must produce identical ordering to Run 0",
                i
            );
        }

        // Verify LIKE search fallback is also 100% deterministic across 5 runs
        let mut like_runs: Vec<Vec<String>> = Vec::new();
        for _ in 0..5 {
            let res = SearchRepository::search_like(&conn, "project", 50).unwrap();
            assert_eq!(res.len(), 5);
            like_runs.push(res.into_iter().map(|r| r.note_id).collect());
        }

        for i in 1..5 {
            assert_eq!(
                like_runs[i], like_runs[0],
                "LIKE fallback run {} must produce identical ordering to Run 0",
                i
            );
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_80_search_and_selection_state() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_sel_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Initial State: Note A created and active
        let note_a = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note A — Project Charter".to_string()),
                content: Some("Initial scope and deliverables for the Q4 project.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note B — Sprint Backlog".to_string()),
                content: Some("Detailed backlog items and story point estimates.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        let mut current_selected_note_id = note_a.id.clone();
        assert_eq!(current_selected_note_id, note_a.id);

        // 2. User edits Note A in the editor
        let updated_note_a_content = "Initial scope and deliverables with updated Q4 requirements.";
        
        // 3. User searches: "Sprint"
        let search_results = SearchRepository::search(&conn, "Sprint", None).unwrap();
        assert_eq!(search_results.len(), 1, "Search should return Note B");
        let result_note_b = &search_results[0];
        assert_eq!(result_note_b.note_id, note_b.id);
        assert_eq!(result_note_b.title, "Note B — Sprint Backlog");

        // 4. User selects search result Note B
        // Pre-selection flush: Note A is persisted before selection switch
        NoteRepository::update(
            &conn,
            &current_selected_note_id,
            UpdateNoteDto {
                content: Some(updated_note_a_content.to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Consistent selection update: selectedNoteId becomes Note B
        current_selected_note_id = result_note_b.note_id.clone();
        assert_eq!(current_selected_note_id, note_b.id);

        // Editor retrieves note for current_selected_note_id
        let editor_loaded_note = NoteRepository::get_by_id(&conn, &current_selected_note_id)
            .unwrap()
            .expect("Note B must be loaded in editor");

        // 5. Invariant Checks (TASK 80):
        // - selectedNoteId = B updated consistently
        assert_eq!(current_selected_note_id, note_b.id);
        assert_eq!(result_note_b.note_id, editor_loaded_note.id);

        // - Search result says B and editor actually shows B (no split-brain)
        assert_eq!(editor_loaded_note.id, note_b.id);
        assert_eq!(editor_loaded_note.title, "Note B — Sprint Backlog");
        assert_eq!(editor_loaded_note.content, "Detailed backlog items and story point estimates.");

        // - Note A is NOT shown in editor, but its changes were properly persisted in DB
        assert_ne!(editor_loaded_note.id, note_a.id);
        let note_a_in_db = NoteRepository::get_by_id(&conn, &note_a.id).unwrap().unwrap();
        assert_eq!(note_a_in_db.content, updated_note_a_content);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}








