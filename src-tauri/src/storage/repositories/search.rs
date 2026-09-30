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

        // Check if FTS5 virtual table exists in the database
        let has_fts: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if has_fts {
            // Attempt FTS5 search first
            if let Ok(results) = Self::search_fts5(conn, trimmed, bounded_limit) {
                return Ok(results);
            }
        }

        // Fallback to parameterized LIKE search
        Self::search_like(conn, trimmed, bounded_limit)
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
            ORDER BY rank
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

    /// Parameterized LIKE search fallback matching title or content case-insensitively
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
              modified_at DESC
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

/// Transforms user search string into safe FTS5 query tokens
fn sanitize_fts5_query(query: &str) -> String {
    let words: Vec<String> = query
        .split_whitespace()
        .map(|w| {
            // Strip FTS5 operators and punctuation that could cause syntax errors
            let clean: String = w.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
            clean
        })
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"*", w))
        .collect();

    words.join(" ")
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
        let char_pos = content[..pos].chars().count();
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
    use crate::storage::models::CreateNoteDto;
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
}
