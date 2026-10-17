use crate::storage::crypto::SecretBytes;
use crate::storage::errors::InternalLinkError;
use crate::storage::models::{
    BacklinkItem, LinkTargetLifecycle, ParsedInternalLink, ResolvedLinkTarget,
};
use crate::storage::repositories::NoteRepository;
use rusqlite::{params, Connection};

pub const INTERNAL_LINK_PROTOCOL_PREFIX: &str = "notepad://note/";

pub struct LinkParser;

impl LinkParser {
    /// Returns true if the provided URI begins with `notepad://note/` (case-insensitive).
    pub fn is_internal_link_uri(uri: &str) -> bool {
        let trimmed = uri.trim();
        if trimmed.len() < INTERNAL_LINK_PROTOCOL_PREFIX.len() {
            return false;
        }
        trimmed[..INTERNAL_LINK_PROTOCOL_PREFIX.len()]
            .eq_ignore_ascii_case(INTERNAL_LINK_PROTOCOL_PREFIX)
    }

    /// Extracts the target note ID from a `notepad://note/<target-id>` URI.
    pub fn extract_target_id_from_uri(uri: &str) -> Option<&str> {
        let trimmed = uri.trim();
        if Self::is_internal_link_uri(trimmed) {
            let id = trimmed[INTERNAL_LINK_PROTOCOL_PREFIX.len()..].trim();
            if !id.is_empty() {
                return Some(id);
            }
        }
        None
    }

    /// Sanitizes a label for embedding in a markdown link.
    pub fn sanitize_label(label: &str) -> String {
        let cleaned = label
            .replace(['\r', '\n'], " ")
            .replace(']', "\\]")
            .trim()
            .to_string();
        if cleaned.is_empty() {
            "Untitled Note".to_string()
        } else {
            cleaned
        }
    }

    /// Formats a canonical internal link string.
    /// Example: `[Meeting Plan](notepad://note/uuid-123)`
    pub fn format_link(target_id: &str, label: Option<&str>) -> String {
        let safe_label = Self::sanitize_label(label.unwrap_or(""));
        format!(
            "[{}]({}{})",
            safe_label,
            INTERNAL_LINK_PROTOCOL_PREFIX,
            target_id.trim()
        )
    }

    /// Parses all valid internal note links within the provided content.
    ///
    /// Guarantees:
    /// - Linear-time zero-backtracking scanning.
    /// - Ignores markdown images `![alt](notepad://note/...)`.
    /// - Handles arbitrary Unicode labels safely.
    /// - Tolerates malformed or incomplete syntax without panicking.
    pub fn parse_internal_links(content: &str) -> Vec<ParsedInternalLink> {
        let mut results = Vec::new();
        if content.is_empty() {
            return results;
        }

        // Fast pre-filter: if text doesn't contain protocol substring, return immediately
        let lower = content.to_ascii_lowercase();
        if !lower.contains(INTERNAL_LINK_PROTOCOL_PREFIX) {
            return results;
        }

        let bytes = content.as_bytes();
        let len = bytes.len();
        let proto_pattern = b"](notepad://note/";
        let proto_len = proto_pattern.len();

        let mut pos = 0;
        while pos + proto_len <= len {
            // Check if slice matches "](notepad://note/" case-insensitively
            let is_match = bytes[pos..pos + proto_len]
                .iter()
                .zip(proto_pattern.iter())
                .all(|(b, p)| b.to_ascii_lowercase() == *p);

            if !is_match {
                pos += 1;
                continue;
            }

            // pos is index of ']'
            let close_bracket_idx = pos;
            let id_start_idx = pos + proto_len;

            // 1. Scan forward from id_start_idx to find closing ')'
            let mut close_paren_idx = None;
            for i in id_start_idx..len {
                let b = bytes[i];
                if b == b')' {
                    close_paren_idx = Some(i);
                    break;
                }
                // Target ID should only contain alphanumeric, hyphen, underscore
                if !(b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
                    break;
                }
            }

            let Some(end_paren) = close_paren_idx else {
                pos += 1;
                continue;
            };

            let target_id_str = &content[id_start_idx..end_paren];
            if target_id_str.is_empty() {
                pos += 1;
                continue;
            }

            // 2. Scan backward from close_bracket_idx to find matching '['
            let mut open_bracket_idx = None;
            let mut back_i = close_bracket_idx;
            while back_i > 0 {
                back_i -= 1;
                let b = bytes[back_i];
                if b == b'\n' || b == b'\r' {
                    // Internal links do not span across newlines
                    break;
                }
                if b == b'[' {
                    // Check if escaped: e.g. "\["
                    if back_i > 0 && bytes[back_i - 1] == b'\\' {
                        continue;
                    }
                    // Check if preceded by '!': image syntax like "![image](...)"
                    if back_i > 0 && bytes[back_i - 1] == b'!' {
                        // Skip markdown image
                        break;
                    }
                    open_bracket_idx = Some(back_i);
                    break;
                }
            }

            let Some(start_bracket) = open_bracket_idx else {
                pos = end_paren + 1;
                continue;
            };

            let raw_token = &content[start_bracket..end_paren + 1];
            let raw_label = &content[start_bracket + 1..close_bracket_idx];
            let unescaped_label = raw_label.replace("\\]", "]");

            results.push(ParsedInternalLink {
                target_id: target_id_str.to_string(),
                label: unescaped_label,
                raw: raw_token.to_string(),
                start_index: start_bracket,
                end_index: end_paren + 1,
            });

            pos = end_paren + 1;
        }

        results
    }

    /// Inserts an internal link token into content at the specified range.
    /// Returns: (updated_content, new_cursor_position, inserted_token)
    pub fn insert_link_into_content(
        content: &str,
        selection_start: Option<usize>,
        selection_end: Option<usize>,
        target_id: &str,
        target_title: Option<&str>,
        custom_label: Option<&str>,
    ) -> (String, usize, String) {
        let len = content.len();
        let mut start = selection_start.unwrap_or(len).min(len);
        let mut end = selection_end.unwrap_or(start).min(len);
        if start > end {
            std::mem::swap(&mut start, &mut end);
        }

        let chosen_label = if let Some(custom) = custom_label {
            if !custom.trim().is_empty() {
                custom.trim().to_string()
            } else if start < end && !content[start..end].trim().is_empty() {
                content[start..end].trim().to_string()
            } else {
                target_title.unwrap_or("Untitled Note").trim().to_string()
            }
        } else if start < end && !content[start..end].trim().is_empty() {
            content[start..end].trim().to_string()
        } else {
            target_title.unwrap_or("Untitled Note").trim().to_string()
        };

        let link_token = Self::format_link(target_id, Some(&chosen_label));
        let mut updated = String::with_capacity(content.len() + link_token.len());
        updated.push_str(&content[..start]);
        updated.push_str(&link_token);
        updated.push_str(&content[end..]);
        let new_cursor = start + link_token.len();

        (updated, new_cursor, link_token)
    }
}

/// Domain service coordinating internal note link resolution and navigation checks (Phase 9)
pub struct InternalLinkService;

impl InternalLinkService {
    /// Resolves an internal link target by its stable UUID / ID (Phase 9, Task 9).
    ///
    /// Resolution rules:
    /// - Strictly identifies targets by stable ID (never title-only matching).
    /// - Validates note ID according to existing identifier rules.
    /// - Uses parameterized SQL (`WHERE n.id = ?1`).
    /// - Distinguishes between:
    ///   - Active target (`status: LinkTargetLifecycle::Active`, `is_available: true`)
    ///   - Trashed target (`status: LinkTargetLifecycle::Trashed`, `is_available: false`)
    ///   - Missing target (`status: LinkTargetLifecycle::Missing`, `is_available: false`)
    /// - Resolves note metadata and parent notebook context name via LEFT JOIN.
    /// - Notes that have been renamed or share duplicate titles resolve unambiguously.
    /// - Returns typed `ResolvedLinkTarget`.
    pub fn resolve_link_target(
        conn: &Connection,
        target_id: &str,
    ) -> Result<ResolvedLinkTarget, InternalLinkError> {
        Self::resolve_link_target_with_key(conn, target_id, None)
    }

    /// Resolves an internal link target by its stable note ID with optional DEK (Phase 9, Task 9; Phase 12, Task 12).
    /// Decrypts note title if encrypted and DEK is provided.
    pub fn resolve_link_target_with_key(
        conn: &Connection,
        target_id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<ResolvedLinkTarget, InternalLinkError> {
        let trimmed_id = target_id.trim();
        if trimmed_id.is_empty()
            || trimmed_id.contains('/')
            || trimmed_id.contains('\\')
            || trimmed_id.contains("..")
        {
            return Ok(ResolvedLinkTarget {
                target_id: target_id.to_string(),
                title: None,
                lifecycle: LinkTargetLifecycle::Missing,
                notebook_id: None,
                notebook_name: None,
            });
        }

        let mut stmt = conn.prepare(
            "SELECT n.id, n.title, n.notebook_id, nb.name, n.is_deleted
             FROM notes n
             LEFT JOIN notebooks nb ON n.notebook_id = nb.id
             WHERE n.id = ?1",
        )?;

        let mut rows = stmt.query(params![trimmed_id])?;
        if let Some(row) = rows.next()? {
            let note_id: String = row.get(0)?;
            let raw_title: String = row.get(1)?;
            let title =
                NoteRepository::decrypt_field_if_encrypted(&raw_title, &note_id, "title", dek)
                    .map_err(|e| InternalLinkError::Crypto(e.to_string()))?;
            let notebook_id: Option<String> = row.get(2)?;
            let notebook_name: Option<String> = row.get(3)?;
            let is_deleted: i64 = row.get(4)?;

            if is_deleted != 0 {
                Ok(ResolvedLinkTarget {
                    target_id: note_id,
                    title: Some(title),
                    lifecycle: LinkTargetLifecycle::Trashed,
                    notebook_id,
                    notebook_name,
                })
            } else {
                Ok(ResolvedLinkTarget {
                    target_id: note_id,
                    title: Some(title),
                    lifecycle: LinkTargetLifecycle::Active,
                    notebook_id,
                    notebook_name,
                })
            }
        } else {
            Ok(ResolvedLinkTarget {
                target_id: trimmed_id.to_string(),
                title: None,
                lifecycle: LinkTargetLifecycle::Missing,
                notebook_id: None,
                notebook_name: None,
            })
        }
    }

    /// Finds all incoming links / backlinks pointing to the target note ID (Phase 9, Task 14).
    ///
    /// Rules:
    /// - Only includes active source notes (`is_deleted = 0`).
    /// - Excludes self-referencing links (`n.id != ?1`).
    /// - Uses candidate filter with parameterized SQL for efficiency (`n.content LIKE ?2`).
    /// - Accurately verifies link existence with `LinkParser::parse_internal_links`.
    /// - Extracts display label and contextual snippet.
    /// - Orders backlinks by source note modified_at DESC.
    pub fn get_backlinks(
        conn: &Connection,
        target_note_id: &str,
    ) -> Result<Vec<BacklinkItem>, InternalLinkError> {
        Self::get_backlinks_with_key(conn, target_note_id, None)
    }

    /// Finds all incoming links / backlinks pointing to target note ID with optional DEK (Phase 12, Task 12).
    /// If DEK is provided, decrypts titles and contents in memory to find backlinks and extract previews.
    pub fn get_backlinks_with_key(
        conn: &Connection,
        target_note_id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<BacklinkItem>, InternalLinkError> {
        let trimmed_id = target_note_id.trim();
        if trimmed_id.is_empty()
            || trimmed_id.contains('/')
            || trimmed_id.contains('\\')
            || trimmed_id.contains("..")
        {
            return Ok(Vec::new());
        }

        let mut backlinks = Vec::new();

        if let Some(key) = dek {
            let mut stmt = conn.prepare(
                "SELECT n.id, n.title, n.content, n.notebook_id, nb.name, n.modified_at
                 FROM notes n
                 LEFT JOIN notebooks nb ON n.notebook_id = nb.id
                 WHERE n.is_deleted = 0
                   AND n.id != ?1
                 ORDER BY n.modified_at DESC",
            )?;

            let mut rows = stmt.query(params![trimmed_id])?;
            while let Some(row) = rows.next()? {
                let source_id: String = row.get(0)?;
                let raw_title: String = row.get(1)?;
                let raw_content: String = row.get(2)?;
                let notebook_id: Option<String> = row.get(3)?;
                let notebook_name: Option<String> = row.get(4)?;
                let modified_at: String = row.get(5)?;

                let title = NoteRepository::decrypt_field_if_encrypted(
                    &raw_title,
                    &source_id,
                    "title",
                    Some(key),
                )
                .map_err(|e| InternalLinkError::Crypto(e.to_string()))?;
                let content = NoteRepository::decrypt_field_if_encrypted(
                    &raw_content,
                    &source_id,
                    "content",
                    Some(key),
                )
                .map_err(|e| InternalLinkError::Crypto(e.to_string()))?;

                let parsed_links = LinkParser::parse_internal_links(&content);
                if let Some(matching_link) =
                    parsed_links.into_iter().find(|l| l.target_id == trimmed_id)
                {
                    let snippet = Self::extract_snippet(
                        &content,
                        matching_link.start_index,
                        matching_link.end_index,
                    );
                    backlinks.push(BacklinkItem {
                        source_note_id: source_id,
                        source_note_title: if title.trim().is_empty() {
                            "Untitled Note".to_string()
                        } else {
                            title
                        },
                        source_notebook_id: notebook_id,
                        source_notebook_name: notebook_name,
                        label_snippet: Some(snippet),
                        modified_at,
                    });
                }
            }
        } else {
            let like_pattern = format!("%notepad://note/{}%", trimmed_id);

            let mut stmt = conn.prepare(
                "SELECT n.id, n.title, n.content, n.notebook_id, nb.name, n.modified_at
                 FROM notes n
                 LEFT JOIN notebooks nb ON n.notebook_id = nb.id
                 WHERE n.is_deleted = 0
                   AND n.id != ?1
                   AND n.content LIKE ?2
                 ORDER BY n.modified_at DESC",
            )?;

            let mut rows = stmt.query(params![trimmed_id, like_pattern])?;

            while let Some(row) = rows.next()? {
                let source_id: String = row.get(0)?;
                let source_title: String = row.get(1)?;
                let content: String = row.get(2)?;
                let notebook_id: Option<String> = row.get(3)?;
                let notebook_name: Option<String> = row.get(4)?;
                let modified_at: String = row.get(5)?;

                // Verify with canonical LinkParser
                let parsed_links = LinkParser::parse_internal_links(&content);
                if let Some(matching_link) =
                    parsed_links.into_iter().find(|l| l.target_id == trimmed_id)
                {
                    let snippet = Self::extract_snippet(
                        &content,
                        matching_link.start_index,
                        matching_link.end_index,
                    );
                    backlinks.push(BacklinkItem {
                        source_note_id: source_id,
                        source_note_title: if source_title.trim().is_empty() {
                            "Untitled Note".to_string()
                        } else {
                            source_title
                        },
                        source_notebook_id: notebook_id,
                        source_notebook_name: notebook_name,
                        label_snippet: Some(snippet),
                        modified_at,
                    });
                }
            }
        }

        Ok(backlinks)
    }

    /// Extracts a safe snippet of text around a link for backlink preview.
    fn extract_snippet(content: &str, start_idx: usize, end_idx: usize) -> String {
        const SNIPPET_RADIUS: usize = 40;
        let char_indices: Vec<(usize, char)> = content.char_indices().collect();
        if char_indices.is_empty() {
            return String::new();
        }

        let start_char_pos = char_indices
            .iter()
            .position(|&(b, _)| b >= start_idx)
            .unwrap_or(0);
        let end_char_pos = char_indices
            .iter()
            .position(|&(b, _)| b >= end_idx)
            .unwrap_or(char_indices.len());

        let prefix_start = start_char_pos.saturating_sub(SNIPPET_RADIUS);
        let suffix_end = (end_char_pos + SNIPPET_RADIUS).min(char_indices.len());

        let prefix: String = char_indices[prefix_start..start_char_pos]
            .iter()
            .map(|&(_, c)| c)
            .collect();
        let link_text: String = char_indices[start_char_pos..end_char_pos]
            .iter()
            .map(|&(_, c)| c)
            .collect();
        let suffix: String = char_indices[end_char_pos..suffix_end]
            .iter()
            .map(|&(_, c)| c)
            .collect();

        let mut snippet = String::new();
        if prefix_start > 0 {
            snippet.push_str("… ");
        }
        snippet.push_str(prefix.trim_start());
        snippet.push_str(&link_text);
        snippet.push_str(suffix.trim_end());
        if suffix_end < char_indices.len() {
            snippet.push_str(" …");
        }

        snippet.replace(['\r', '\n'], " ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_single_link() {
        let content = "Check out [Project Roadmap](notepad://note/uuid-1234) for details.";
        let links = LinkParser::parse_internal_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "uuid-1234");
        assert_eq!(links[0].label, "Project Roadmap");
        assert_eq!(links[0].raw, "[Project Roadmap](notepad://note/uuid-1234)");
        assert_eq!(links[0].start_index, 10);
        assert_eq!(links[0].end_index, 53);
        assert_eq!(
            &content[links[0].start_index..links[0].end_index],
            links[0].raw
        );
    }

    #[test]
    fn test_parse_multiple_links_and_case_insensitive_scheme() {
        let content = "First [Note A](notepad://note/id-aaa) then [Note B](NOTEPAD://NOTE/id-bbb).";
        let links = LinkParser::parse_internal_links(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_id, "id-aaa");
        assert_eq!(links[0].label, "Note A");
        assert_eq!(links[1].target_id, "id-bbb");
        assert_eq!(links[1].label, "Note B");
    }

    #[test]
    fn test_parse_unicode_labels() {
        let content = "योजना [वार्षिक रिपोर्ट २०२६](notepad://note/uuid-hindi-1) एवं [Café Études](notepad://note/uuid-french-2)";
        let links = LinkParser::parse_internal_links(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_id, "uuid-hindi-1");
        assert_eq!(links[0].label, "वार्षिक रिपोर्ट २०२६");
        assert_eq!(links[1].target_id, "uuid-french-2");
        assert_eq!(links[1].label, "Café Études");
        assert_eq!(
            &content[links[0].start_index..links[0].end_index],
            links[0].raw
        );
        assert_eq!(
            &content[links[1].start_index..links[1].end_index],
            links[1].raw
        );
    }

    #[test]
    fn test_ignore_markdown_images_and_external_links() {
        let content = "Image ![Preview](notepad://note/img-id) and external [Google](https://google.com) and valid [Doc](notepad://note/valid-id)";
        let links = LinkParser::parse_internal_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "valid-id");
        assert_eq!(links[0].label, "Doc");
    }

    #[test]
    fn test_handle_malformed_and_edge_cases() {
        let content = "Empty id [](notepad://note/) broken [Unclosed](notepad://note/123 ordinary text [bracket] here";
        let links = LinkParser::parse_internal_links(content);
        // Only valid links returned, never panics
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_format_and_sanitize_link() {
        let formatted = LinkParser::format_link("uuid-123", Some("Project Plan"));
        assert_eq!(formatted, "[Project Plan](notepad://note/uuid-123)");

        let with_brackets = LinkParser::format_link("uuid-456", Some("Plan [Q1]"));
        assert_eq!(with_brackets, "[Plan [Q1\\]](notepad://note/uuid-456)");

        let empty_label = LinkParser::format_link("uuid-789", None);
        assert_eq!(empty_label, "[Untitled Note](notepad://note/uuid-789)");
    }

    #[test]
    fn test_uri_helpers() {
        assert!(LinkParser::is_internal_link_uri("notepad://note/abc"));
        assert!(LinkParser::is_internal_link_uri("NOTEPAD://NOTE/xyz"));
        assert!(!LinkParser::is_internal_link_uri("https://example.com"));
        assert!(!LinkParser::is_internal_link_uri(""));

        assert_eq!(
            LinkParser::extract_target_id_from_uri("notepad://note/abc-123"),
            Some("abc-123")
        );
        assert_eq!(
            LinkParser::extract_target_id_from_uri("https://notepad://note/123"),
            None
        );
    }

    #[test]
    fn test_insert_link_into_content() {
        // 1. Insert at end of document
        let (res1, cursor1, token1) = LinkParser::insert_link_into_content(
            "Hello world",
            None,
            None,
            "id-100",
            Some("Target Note"),
            None,
        );
        assert_eq!(token1, "[Target Note](notepad://note/id-100)");
        assert_eq!(res1, "Hello world[Target Note](notepad://note/id-100)");
        assert_eq!(cursor1, 11 + token1.len());

        // 2. Insert replacing selected text
        let (res2, cursor2, token2) = LinkParser::insert_link_into_content(
            "Check the roadmap for dates.",
            Some(10),
            Some(17),
            "id-200",
            Some("2026 Roadmap"),
            None,
        );
        assert_eq!(token2, "[roadmap](notepad://note/id-200)");
        assert_eq!(
            res2,
            "Check the [roadmap](notepad://note/id-200) for dates."
        );
        assert_eq!(cursor2, 10 + token2.len());

        // 3. Insert with custom label
        let (res3, _cursor3, token3) = LinkParser::insert_link_into_content(
            "",
            None,
            None,
            "id-300",
            Some("Default Title"),
            Some("Custom Alias"),
        );
        assert_eq!(token3, "[Custom Alias](notepad://note/id-300)");
        assert_eq!(res3, "[Custom Alias](notepad://note/id-300)");
    }

    #[test]
    fn test_resolve_link_target_lifecycle_and_invariants() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_links_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = crate::storage::database::init_connection(&db_path).expect("Init failed");
        crate::storage::migrations::run_migrations(&mut conn).expect("Migrations failed");

        // 1. Create notebook
        let nb = crate::storage::repositories::NotebookRepository::create(
            &conn,
            crate::storage::models::CreateNotebookDto {
                name: "Engineering".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        // 2. Create active note A in notebook
        let note_a = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("System Architecture".to_string()),
                content: Some("Architecture specs".to_string()),
                notebook_id: Some(nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        // 3. Create active note B with duplicate title in unfiled
        let note_b = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("System Architecture".to_string()),
                content: Some("Duplicate title note".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        )
        .unwrap();

        // 4. Create active note C to be trashed
        let note_c = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Deprecated Guide".to_string()),
                content: Some("Old guide".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        crate::storage::services::TrashService::soft_delete(&conn, &note_c.id).unwrap();

        // Verify Note A resolves as Active with notebook name
        let res_a = InternalLinkService::resolve_link_target(&conn, &note_a.id).unwrap();
        assert_eq!(res_a.target_id, note_a.id);
        assert_eq!(res_a.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(res_a.title.as_deref(), Some("System Architecture"));
        assert_eq!(res_a.notebook_id, Some(nb.id.clone()));
        assert_eq!(res_a.notebook_name.as_deref(), Some("Engineering"));

        // Verify Note B (duplicate title) resolves to Note B's exact ID, unfiled
        let res_b = InternalLinkService::resolve_link_target(&conn, &note_b.id).unwrap();
        assert_eq!(res_b.target_id, note_b.id);
        assert_ne!(res_b.target_id, res_a.target_id);
        assert_eq!(res_b.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(res_b.notebook_id, None);
        assert_eq!(res_b.notebook_name, None);

        // Verify Note C resolves as Trashed
        let res_c = InternalLinkService::resolve_link_target(&conn, &note_c.id).unwrap();
        assert_eq!(res_c.target_id, note_c.id);
        assert_eq!(res_c.lifecycle, LinkTargetLifecycle::Trashed);
        assert_eq!(res_c.title.as_deref(), Some("Deprecated Guide"));

        // Rename Note A and verify resolution still works by stable UUID
        conn.execute(
            "UPDATE notes SET title = ?1 WHERE id = ?2",
            rusqlite::params!["Modern Cloud Architecture", &note_a.id],
        )
        .unwrap();
        let res_a_renamed = InternalLinkService::resolve_link_target(&conn, &note_a.id).unwrap();
        assert_eq!(res_a_renamed.target_id, note_a.id);
        assert_eq!(
            res_a_renamed.title.as_deref(),
            Some("Modern Cloud Architecture")
        );
        assert_eq!(res_a_renamed.lifecycle, LinkTargetLifecycle::Active);

        // Missing target
        let res_missing =
            InternalLinkService::resolve_link_target(&conn, "nonexistent-uuid-999").unwrap();
        assert_eq!(res_missing.target_id, "nonexistent-uuid-999");
        assert_eq!(res_missing.lifecycle, LinkTargetLifecycle::Missing);
        assert_eq!(res_missing.title, None);

        // Empty target ID
        let res_empty = InternalLinkService::resolve_link_target(&conn, "   ").unwrap();
        assert_eq!(res_empty.lifecycle, LinkTargetLifecycle::Missing);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_backlinks_discovery_and_filtering() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_backlinks_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = crate::storage::database::init_connection(&db_path).expect("Init failed");
        crate::storage::migrations::run_migrations(&mut conn).expect("Migrations failed");

        // Notebook
        let nb = crate::storage::repositories::NotebookRepository::create(
            &conn,
            crate::storage::models::CreateNotebookDto {
                name: "Research".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        // Target note
        let target_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Quantum Computing".to_string()),
                content: Some("Core concepts and quantum gates".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        )
        .unwrap();

        // 1. Source Note 1 (in Research notebook) linking to target note
        let source_note_1 = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Physics Lecture Notes".to_string()),
                content: Some(format!(
                    "Check out [Quantum Computing](notepad://note/{}) for foundational theory.",
                    target_note.id
                )),
                notebook_id: Some(nb.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        // 2. Source Note 2 (unfiled) linking to target note
        let _source_note_2 = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Reading List 2026".to_string()),
                content: Some(format!(
                    "High priority: [Quantum intro](notepad://note/{})",
                    target_note.id
                )),
                notebook_id: None,
                ..Default::default()
            },
        )
        .unwrap();

        // 3. Self-referencing link in target note itself (should be excluded)
        conn.execute(
            "UPDATE notes SET content = ?1 WHERE id = ?2",
            rusqlite::params![
                format!("Self reference: [Self](notepad://note/{})", target_note.id),
                &target_note.id
            ],
        )
        .unwrap();

        // 4. Source Note 3 linking to target note, but trashed (should be excluded)
        let source_note_trashed = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Old Draft".to_string()),
                content: Some(format!(
                    "Old link: [Quantum](notepad://note/{})",
                    target_note.id
                )),
                notebook_id: None,
                ..Default::default()
            },
        )
        .unwrap();
        crate::storage::services::TrashService::soft_delete(&conn, &source_note_trashed.id)
            .unwrap();

        // Query backlinks for target_note.id
        let backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();

        // Should return exactly 2 active backlinks (source_note_1 and source_note_2)
        assert_eq!(backlinks.len(), 2);

        // Find source 1 in results
        let backlink_1 = backlinks
            .iter()
            .find(|b| b.source_note_id == source_note_1.id)
            .expect("Source 1 missing");
        assert_eq!(backlink_1.source_note_title, "Physics Lecture Notes");
        assert_eq!(backlink_1.source_notebook_id, Some(nb.id.clone()));
        assert_eq!(backlink_1.source_notebook_name.as_deref(), Some("Research"));
        assert!(backlink_1
            .label_snippet
            .as_ref()
            .unwrap()
            .contains("Quantum Computing"));

        // Verify empty query returns empty list
        let empty_backlinks = InternalLinkService::get_backlinks(&conn, "   ").unwrap();
        assert!(empty_backlinks.is_empty());

        // Verify note with no incoming links returns empty list
        let no_links = InternalLinkService::get_backlinks(&conn, &source_note_1.id).unwrap();
        assert!(no_links.is_empty());

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_15_link_metadata_synchronized_lifecycle_and_responsiveness() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_sync_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = crate::storage::database::init_connection(&db_path).expect("Init failed");
        crate::storage::migrations::run_migrations(&mut conn).expect("Migrations failed");

        // 1. Create Target Note
        let target_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Quantum Computing".to_string()),
                content: Some("Foundational quantum mechanics".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // 2. Note creation with link
        let source_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Lecture 1".to_string()),
                content: Some(format!(
                    "Introductory reading: [Quantum](notepad://note/{})",
                    target_note.id
                )),
                ..Default::default()
            },
        )
        .unwrap();

        let backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_note_id, source_note.id);
        assert!(backlinks[0]
            .label_snippet
            .as_ref()
            .unwrap()
            .contains("Quantum"));

        // 3. Note content update: label changed
        crate::storage::repositories::NoteRepository::update(
            &conn,
            &source_note.id,
            crate::storage::models::UpdateNoteDto {
                content: Some(format!(
                    "Introductory reading: [Advanced Qubits](notepad://note/{})",
                    target_note.id
                )),
                ..Default::default()
            },
        )
        .unwrap();

        let backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(backlinks.len(), 1);
        assert!(backlinks[0]
            .label_snippet
            .as_ref()
            .unwrap()
            .contains("Advanced Qubits"));

        // 4. Note content update: link removed
        crate::storage::repositories::NoteRepository::update(
            &conn,
            &source_note.id,
            crate::storage::models::UpdateNoteDto {
                content: Some("Introductory reading: general classical physics only.".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(
            backlinks.len(),
            0,
            "Obsolete link must immediately disappear from backlinks"
        );

        // 5. Link re-inserted into note content
        crate::storage::repositories::NoteRepository::update(
            &conn,
            &source_note.id,
            crate::storage::models::UpdateNoteDto {
                content: Some(format!(
                    "Reference: [Quantum Mechanics](notepad://note/{})",
                    target_note.id
                )),
                ..Default::default()
            },
        )
        .unwrap();

        let backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(backlinks.len(), 1);

        // 6. Source note soft deletion (trashed)
        crate::storage::services::TrashService::soft_delete(&conn, &source_note.id).unwrap();
        let backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(
            backlinks.len(),
            0,
            "Trashed source note must be excluded from backlinks"
        );

        // 7. Source note restoration
        crate::storage::services::TrashService::restore(&conn, &source_note.id).unwrap();
        let backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(
            backlinks.len(),
            1,
            "Restored source note must immediately re-appear in backlinks"
        );

        // 8. Source note permanent deletion
        let storage_paths = crate::storage::StoragePaths::from_root(temp_dir.clone());
        let storage = crate::storage::attachment_storage::AttachmentStorage::new(storage_paths);
        crate::storage::services::TrashService::soft_delete(&conn, &source_note.id).unwrap();
        crate::storage::services::TrashService::permanently_delete_note(
            &conn,
            &storage,
            &source_note.id,
        )
        .unwrap();

        let backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(
            backlinks.len(),
            0,
            "Permanently deleted source note must be completely purged"
        );

        // 9. Responsiveness test across multiple notes
        for i in 0..50 {
            crate::storage::repositories::NoteRepository::create(
                &conn,
                crate::storage::models::CreateNoteDto {
                    title: Some(format!("Chapter {}", i)),
                    content: Some(format!(
                        "Topic includes [Quantum](notepad://note/{}) in chapter {}.",
                        target_note.id, i
                    )),
                    ..Default::default()
                },
            )
            .unwrap();
        }

        let start = std::time::Instant::now();
        let bulk_backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(bulk_backlinks.len(), 50);
        assert!(
            elapsed.as_millis() < 100,
            "Query time must remain sub-100ms across 50 candidate notes (actual: {:?})",
            elapsed
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_16_broken_links_and_missing_targets_safety() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_broken_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = crate::storage::database::init_connection(&db_path).expect("Init failed");
        crate::storage::migrations::run_migrations(&mut conn).expect("Migrations failed");

        // 1. Create a note and then permanently delete it
        let note_to_delete = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Temporary Guide".to_string()),
                content: Some("Some content".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // Also create a second note with the EXACT same title
        let duplicate_title_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Temporary Guide".to_string()),
                content: Some("Another note with duplicate title".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let storage_paths = crate::storage::StoragePaths::from_root(temp_dir.clone());
        let storage = crate::storage::attachment_storage::AttachmentStorage::new(storage_paths);
        crate::storage::services::TrashService::soft_delete(&conn, &note_to_delete.id).unwrap();
        crate::storage::services::TrashService::permanently_delete_note(
            &conn,
            &storage,
            &note_to_delete.id,
        )
        .unwrap();

        // 2. Resolve link to permanently deleted note ID
        let resolved = InternalLinkService::resolve_link_target(&conn, &note_to_delete.id).unwrap();

        // Must be Missing, NEVER silently resolved to duplicate_title_note
        assert_eq!(resolved.lifecycle, LinkTargetLifecycle::Missing);
        assert_eq!(resolved.title, None);
        assert_ne!(resolved.target_id, duplicate_title_note.id);

        // 3. Resolve completely arbitrary non-existent UUID
        let nonexistent =
            InternalLinkService::resolve_link_target(&conn, "arbitrary-nonexistent-uuid").unwrap();
        assert_eq!(nonexistent.lifecycle, LinkTargetLifecycle::Missing);
        assert_eq!(nonexistent.title, None);

        // 4. Verify duplicate title note still resolves cleanly to its own distinct UUID
        let duplicate_resolved =
            InternalLinkService::resolve_link_target(&conn, &duplicate_title_note.id).unwrap();
        assert_eq!(duplicate_resolved.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(duplicate_resolved.target_id, duplicate_title_note.id);
        assert_eq!(duplicate_resolved.title.as_deref(), Some("Temporary Guide"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_17_trash_and_recovery_link_lifecycle_integration() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_trash_lifecycle_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = crate::storage::database::init_connection(&db_path).expect("Init failed");
        crate::storage::migrations::run_migrations(&mut conn).expect("Migrations failed");

        // 1. Create Target Note T
        let target_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Project Roadmap 2026".to_string()),
                content: Some("Milestones and delivery timelines".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // 2. Create Source Note S linking to T
        let initial_source_content = format!(
            "Foundational plan: [Roadmap](notepad://note/{}) - please review.",
            target_note.id
        );
        let source_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Team Sync".to_string()),
                content: Some(initial_source_content.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        // 3. Move Target Note T into Trash
        crate::storage::services::TrashService::soft_delete(&conn, &target_note.id).unwrap();

        // Target resolves as Trashed
        let resolved_trashed =
            InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
        assert_eq!(resolved_trashed.lifecycle, LinkTargetLifecycle::Trashed);
        assert_eq!(
            resolved_trashed.title.as_deref(),
            Some("Project Roadmap 2026")
        );

        // Crucial invariant: Source note content is completely preserved and never rewritten
        let fetched_source =
            crate::storage::repositories::NoteRepository::get_by_id(&conn, &source_note.id)
                .unwrap()
                .unwrap();
        assert_eq!(fetched_source.content, initial_source_content);

        // 4. Restore Target Note T from Trash
        crate::storage::services::TrashService::restore(&conn, &target_note.id).unwrap();

        // Target immediately resolves as Active without requiring link re-creation
        let resolved_restored =
            InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
        assert_eq!(resolved_restored.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(
            resolved_restored.title.as_deref(),
            Some("Project Roadmap 2026")
        );

        // 5. Move Source Note S into Trash
        crate::storage::services::TrashService::soft_delete(&conn, &source_note.id).unwrap();

        // Trashed source note is safely excluded from incoming backlinks
        let backlinks_trashed_source =
            InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(backlinks_trashed_source.len(), 0);

        // Source note content is preserved while in trash
        let source_in_trash =
            crate::storage::repositories::NoteRepository::get_by_id(&conn, &source_note.id)
                .unwrap()
                .unwrap();
        assert!(source_in_trash.is_deleted);
        assert_eq!(source_in_trash.content, initial_source_content);

        // 6. Restore Source Note S from Trash
        crate::storage::services::TrashService::restore(&conn, &source_note.id).unwrap();

        // Restored source note immediately re-appears in target note's backlinks
        let backlinks_restored_source =
            InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(backlinks_restored_source.len(), 1);
        assert_eq!(backlinks_restored_source[0].source_note_id, source_note.id);

        // 7. Permanently delete Target Note T
        let storage_paths = crate::storage::StoragePaths::from_root(temp_dir.clone());
        let storage = crate::storage::attachment_storage::AttachmentStorage::new(storage_paths);
        crate::storage::services::TrashService::soft_delete(&conn, &target_note.id).unwrap();
        crate::storage::services::TrashService::permanently_delete_note(
            &conn,
            &storage,
            &target_note.id,
        )
        .unwrap();

        // Target resolves as Missing
        let resolved_perm_deleted =
            InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
        assert_eq!(
            resolved_perm_deleted.lifecycle,
            LinkTargetLifecycle::Missing
        );
        assert_eq!(resolved_perm_deleted.title, None);

        // Source note content remains completely intact (no cascade deletion of source content)
        let source_after_target_deleted =
            crate::storage::repositories::NoteRepository::get_by_id(&conn, &source_note.id)
                .unwrap()
                .unwrap();
        assert_eq!(source_after_target_deleted.content, initial_source_content);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_18_renames_duplicate_titles_unicode_and_moves() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_renames_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = crate::storage::database::init_connection(&db_path).expect("Init failed");
        crate::storage::migrations::run_migrations(&mut conn).expect("Migrations failed");

        // Notebooks
        let nb_engineering = crate::storage::repositories::NotebookRepository::create(
            &conn,
            crate::storage::models::CreateNotebookDto {
                name: "Engineering".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        let nb_archive = crate::storage::repositories::NotebookRepository::create(
            &conn,
            crate::storage::models::CreateNotebookDto {
                name: "Archive".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        // 1. Scenario: Target Note Renamed
        let target_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Initial Specs".to_string()),
                content: Some("Specification body".to_string()),
                notebook_id: Some(nb_engineering.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let source_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Overview".to_string()),
                content: Some(format!(
                    "See [Initial Specs](notepad://note/{}) for details.",
                    target_note.id
                )),
                ..Default::default()
            },
        )
        .unwrap();

        // Rename the target note in SQLite
        crate::storage::repositories::NoteRepository::update(
            &conn,
            &target_note.id,
            crate::storage::models::UpdateNoteDto {
                title: Some("Refactored Distributed Architecture v2".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // Resolution continues to work seamlessly by UUID, yielding the updated title
        let resolved_renamed =
            InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
        assert_eq!(resolved_renamed.target_id, target_note.id);
        assert_eq!(resolved_renamed.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(
            resolved_renamed.title.as_deref(),
            Some("Refactored Distributed Architecture v2")
        );
        assert_eq!(
            resolved_renamed.notebook_name.as_deref(),
            Some("Engineering")
        );

        // Source note was NOT modified or rewritten on target rename
        let source_unaltered =
            crate::storage::repositories::NoteRepository::get_by_id(&conn, &source_note.id)
                .unwrap()
                .unwrap();
        assert_eq!(
            source_unaltered.content,
            format!(
                "See [Initial Specs](notepad://note/{}) for details.",
                target_note.id
            )
        );

        // 2. Scenario: Two notes share the EXACT same title
        let note_a = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Quarterly Review".to_string()),
                content: Some("Review in Engineering".to_string()),
                notebook_id: Some(nb_engineering.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let note_b = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Quarterly Review".to_string()), // Identical title!
                content: Some("Review in Unfiled".to_string()),
                notebook_id: None,
                ..Default::default()
            },
        )
        .unwrap();

        assert_ne!(note_a.id, note_b.id);

        let res_a = InternalLinkService::resolve_link_target(&conn, &note_a.id).unwrap();
        let res_b = InternalLinkService::resolve_link_target(&conn, &note_b.id).unwrap();

        assert_eq!(res_a.target_id, note_a.id);
        assert_eq!(res_a.notebook_name.as_deref(), Some("Engineering"));
        assert_eq!(res_b.target_id, note_b.id);
        assert_eq!(res_b.notebook_name, None);

        // 3. Scenario: Note title contains Unicode and international characters
        let unicode_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("योजना एवं अनुसंधान २०२६ (Résumé & Café)".to_string()),
                content: Some("Unicode research content".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let res_unicode =
            InternalLinkService::resolve_link_target(&conn, &unicode_note.id).unwrap();
        assert_eq!(res_unicode.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(
            res_unicode.title.as_deref(),
            Some("योजना एवं अनुसंधान २०२६ (Résumé & Café)")
        );

        // 4. Scenario: Note title contains Markdown punctuation (*, _, #, -, :, ())
        let punctuated_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Release v3.0.0 (Final) - #1: High_Priority*".to_string()),
                content: Some("Release notes content".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let formatted_link = LinkParser::format_link(
            &punctuated_note.id,
            Some("Release v3.0.0 (Final) - #1: High_Priority*"),
        );
        let parsed_links = LinkParser::parse_internal_links(&formatted_link);
        assert_eq!(parsed_links.len(), 1);
        assert_eq!(parsed_links[0].target_id, punctuated_note.id);
        assert_eq!(
            parsed_links[0].label,
            "Release v3.0.0 (Final) - #1: High_Priority*"
        );

        // Empty title handling
        let empty_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: None,
                content: Some("Untitled content".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let res_empty = InternalLinkService::resolve_link_target(&conn, &empty_note.id).unwrap();
        assert_eq!(res_empty.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(res_empty.title.as_deref(), Some("Untitled Note"));

        // 5. Scenario: Target Note Moves to Another Notebook
        crate::storage::repositories::NoteRepository::update(
            &conn,
            &target_note.id,
            crate::storage::models::UpdateNoteDto {
                notebook_id: Some(nb_archive.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let res_moved = InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
        assert_eq!(res_moved.target_id, target_note.id);
        assert_eq!(res_moved.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(res_moved.notebook_id, Some(nb_archive.id.clone()));
        assert_eq!(res_moved.notebook_name.as_deref(), Some("Archive"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_19_txt_and_md_note_formats() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_links_formats_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = crate::storage::database::init_connection(&db_path).expect("Init failed");
        crate::storage::migrations::run_migrations(&mut conn).expect("Migrations failed");

        // Target Note (Markdown format)
        let target_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("System Architecture".to_string()),
                content: Some(
                    "# System Architecture\n\nDetailed breakdown of components.".to_string(),
                ),
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(target_note.format, "md");

        // Source Note 1: TXT Note linking to target
        let txt_source = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Plain Notes".to_string()),
                content: Some(format!(
                    "Plain text with brackets [TODO] and reference [System Architecture](notepad://note/{}) here.",
                    target_note.id
                )),
                format: Some("txt".to_string()),
                ..Default::default()
            },
        ).unwrap();
        assert_eq!(txt_source.format, "txt");

        // Source Note 2: Markdown Note linking to target with complex Markdown features
        let md_source = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Sprint Backlog".to_string()),
                content: Some(format!(
                    "# Sprint Backlog\n\n- Task 1: Check ![Diagram](notepad://assets/diag.png)\n- Task 2: See [Architecture](notepad://note/{}) for details\n\n```rust\nlet x = [1, 2, 3];\n```",
                    target_note.id
                )),
                format: Some("md".to_string()),
                ..Default::default()
            },
        ).unwrap();
        assert_eq!(md_source.format, "md");

        // Verify backlinks are discovered across both TXT and MD formats
        let backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(
            backlinks.len(),
            2,
            "Backlinks must discover incoming links from both TXT and MD notes"
        );

        // Verify format switching does not affect link integrity
        let updated_txt = crate::storage::repositories::NoteRepository::update(
            &conn,
            &txt_source.id,
            crate::storage::models::UpdateNoteDto {
                format: Some("md".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated_txt.format, "md");

        let backlinks_after_toggle =
            InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(backlinks_after_toggle.len(), 2);

        // Verify plain-text note parsing does not falsely match brackets [TODO] or code brackets
        let raw_parsed = LinkParser::parse_internal_links(&txt_source.content);
        assert_eq!(
            raw_parsed.len(),
            1,
            "Only notepad://note/ URI should be parsed as internal link"
        );
        assert_eq!(raw_parsed[0].target_id, target_note.id);
        assert_eq!(raw_parsed[0].label, "System Architecture");

        let md_parsed = LinkParser::parse_internal_links(&md_source.content);
        assert_eq!(
            md_parsed.len(),
            1,
            "Markdown images and code brackets must not match as internal links"
        );
        assert_eq!(md_parsed[0].target_id, target_note.id);
        assert_eq!(md_parsed[0].label, "Architecture");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_22_automated_parser_service_and_database_suite() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_task22_suite_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = crate::storage::database::init_connection(&db_path).expect("Init failed");
        crate::storage::migrations::run_migrations(&mut conn).expect("Migrations failed");

        // 1. Parsing: Performance test with 100KB+ note content containing multiple embedded links
        let mut large_content = String::with_capacity(120_000);
        for i in 0..1500 {
            large_content.push_str("Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt. ");
            if i % 150 == 0 {
                large_content.push_str(&format!(
                    "[Reference Link {}](notepad://note/target-perf-{}) ",
                    i, i
                ));
            }
        }
        assert!(large_content.len() > 100_000, "Content must exceed 100KB");

        let parse_start = std::time::Instant::now();
        let parsed_large = LinkParser::parse_internal_links(&large_content);
        let parse_elapsed = parse_start.elapsed();

        assert_eq!(
            parsed_large.len(),
            10,
            "Must correctly identify all 10 embedded links in 100KB text"
        );
        assert!(
            parse_elapsed.as_millis() < 500,
            "100KB linear parsing must complete without pathological time (actual: {:?})",
            parse_elapsed
        );

        // 2. Parsing: Malformed and boundary cases
        let malformed_content = "\
            [empty-scheme]() \
            [empty-id](notepad://note/) \
            [whitespace-id](notepad://note/   ) \
            [wrong-scheme](https://notepad://note/valid-id) \
            ![image](notepad://note/img-target) \
            [valid link](notepad://note/valid-target-uuid) \
            [unclosed](notepad://note/valid-target \
            regular text [brackets without scheme] more text";
        let malformed_parsed = LinkParser::parse_internal_links(malformed_content);
        assert_eq!(
            malformed_parsed.len(),
            1,
            "Only exact [label](notepad://note/<valid-id>) should be parsed"
        );
        assert_eq!(malformed_parsed[0].target_id, "valid-target-uuid");
        assert_eq!(malformed_parsed[0].label, "valid link");

        // 3. Insertion: Boundary insertion positions
        // A. Insert into empty content
        let (res_empty, cursor_empty, token_empty) = LinkParser::insert_link_into_content(
            "",
            Some(0),
            Some(0),
            "target-id-1",
            Some("Initial Title"),
            None,
        );
        assert_eq!(res_empty, "[Initial Title](notepad://note/target-id-1)");
        assert_eq!(cursor_empty, token_empty.len());

        // B. Insert at very beginning of note
        let (res_start, _, _) = LinkParser::insert_link_into_content(
            "Existing trailing content.",
            Some(0),
            Some(0),
            "target-id-start",
            Some("Header Link"),
            None,
        );
        assert!(res_start.starts_with("[Header Link](notepad://note/target-id-start)Existing"));

        // C. Insert replacing selected range
        let (res_range, _, _) = LinkParser::insert_link_into_content(
            "Select this word for link.",
            Some(7),
            Some(16), // "this word"
            "target-id-word",
            Some("Default Title"),
            Some("Custom Word"),
        );
        assert_eq!(
            res_range,
            "Select [Custom Word](notepad://note/target-id-word) for link."
        );

        // 4. Resolution: Security & Malicious Identifier Rejection
        let injection_attempts = vec![
            "' OR 1=1 --",
            "../../etc/passwd",
            "<script>alert(1)</script>",
            "   ",
            "",
            "null",
            "non-existent-uuid-9999",
        ];
        for bad_id in injection_attempts {
            let res = InternalLinkService::resolve_link_target(&conn, bad_id).unwrap();
            assert_eq!(
                res.lifecycle,
                LinkTargetLifecycle::Missing,
                "Bad/missing ID '{}' must safely resolve as Missing",
                bad_id
            );
        }

        // 5. Database & Service Integration: Target lifecycle and backlink synchronization
        let target_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Database Engine".to_string()),
                content: Some("Core storage engine implementation.".to_string()),
                format: Some("txt".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let source_note = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Service Layer Architecture".to_string()),
                content: Some(format!(
                    "Depends on [Database Engine](notepad://note/{}) for queries.",
                    target_note.id
                )),
                format: Some("txt".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // Verify active target resolution and initial backlinks
        let res_target = InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
        assert_eq!(res_target.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(res_target.title.as_deref(), Some("Database Engine"));

        let initial_backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(initial_backlinks.len(), 1);
        assert_eq!(initial_backlinks[0].source_note_id, source_note.id);
        assert_eq!(
            initial_backlinks[0].source_note_title,
            "Service Layer Architecture"
        );

        // Target Rename: Link remains active, title updates, source content untouched
        crate::storage::repositories::NoteRepository::update(
            &conn,
            &target_note.id,
            crate::storage::models::UpdateNoteDto {
                title: Some("SQLite Database Engine v2".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let res_renamed = InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
        assert_eq!(res_renamed.lifecycle, LinkTargetLifecycle::Active);
        assert_eq!(
            res_renamed.title.as_deref(),
            Some("SQLite Database Engine v2")
        );

        let current_source =
            crate::storage::repositories::NoteRepository::get_by_id(&conn, &source_note.id)
                .unwrap()
                .unwrap();
        assert!(
            current_source
                .content
                .contains("Depends on [Database Engine](notepad://note/"),
            "Source note content must remain unchanged"
        );

        // Target Trashed: lifecycle becomes Trashed
        crate::storage::services::TrashService::soft_delete(&conn, &target_note.id).unwrap();
        let res_trashed = InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
        assert_eq!(res_trashed.lifecycle, LinkTargetLifecycle::Trashed);

        // Target Restored: lifecycle returns to Active
        crate::storage::services::TrashService::restore(&conn, &target_note.id).unwrap();
        let res_restored =
            InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
        assert_eq!(res_restored.lifecycle, LinkTargetLifecycle::Active);

        // Target Permanently Deleted: lifecycle becomes Missing, source note content untouched
        let storage_paths = crate::storage::StoragePaths::from_root(temp_dir.clone());
        let storage = crate::storage::attachment_storage::AttachmentStorage::new(storage_paths);
        crate::storage::services::TrashService::soft_delete(&conn, &target_note.id).unwrap();
        crate::storage::services::TrashService::permanently_delete_note(
            &conn,
            &storage,
            &target_note.id,
        )
        .unwrap();

        let res_deleted = InternalLinkService::resolve_link_target(&conn, &target_note.id).unwrap();
        assert_eq!(res_deleted.lifecycle, LinkTargetLifecycle::Missing);

        let final_source =
            crate::storage::repositories::NoteRepository::get_by_id(&conn, &source_note.id)
                .unwrap()
                .unwrap();
        assert_eq!(
            final_source.content, source_note.content,
            "Source note content must never be corrupted by target deletion"
        );

        // When source note removes the link, backlinks for target note becomes 0
        crate::storage::repositories::NoteRepository::update(
            &conn,
            &source_note.id,
            crate::storage::models::UpdateNoteDto {
                content: Some("Architecture notes without links.".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let final_backlinks = InternalLinkService::get_backlinks(&conn, &target_note.id).unwrap();
        assert_eq!(
            final_backlinks.len(),
            0,
            "Backlinks must update when source content removes link"
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_24_security_reliability_and_performance_verification() {
        let temp_dir =
            std::env::temp_dir().join(format!("notepad_test_links_t24_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("test.db");

        let mut conn = crate::storage::init_connection(&db_path).unwrap();
        crate::storage::run_migrations(&mut conn).unwrap();

        // 1. Security: Parameterized query safety & SQL injection payloads
        let malicious_ids = vec![
            "' OR '1'='1",
            "1; DROP TABLE notes; --",
            "UNION SELECT * FROM notes --",
            "<script>alert('xss')</script>",
            "../../etc/passwd",
            "\0nullbyte",
        ];
        for malicious_id in malicious_ids {
            let res = InternalLinkService::resolve_link_target(&conn, malicious_id).unwrap();
            assert_eq!(res.lifecycle, LinkTargetLifecycle::Missing);
        }

        // 2. Security: Malformed & adversarial parser syntax
        let adversarial_content = "Start {{ [unclosed(notepad://note/bad) [nested [inner]](notepad://note/123) [xss](javascript:alert(1)) [file](file:///etc/hosts) End";
        let parsed = LinkParser::parse_internal_links(adversarial_content);
        for link in &parsed {
            assert!(!link.target_id.contains("javascript"));
            assert!(!link.target_id.contains("file://"));
        }

        // 3. Security: Title collision & redirection immunity
        let note_a = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Confidential Blueprint".to_string()),
                content: Some("Target A".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let note_b = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Confidential Blueprint".to_string()),
                content: Some("Target B".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // Resolving by ID resolves strictly to correct note
        let res_a = InternalLinkService::resolve_link_target(&conn, &note_a.id).unwrap();
        assert_eq!(res_a.target_id, note_a.id);
        let res_b = InternalLinkService::resolve_link_target(&conn, &note_b.id).unwrap();
        assert_eq!(res_b.target_id, note_b.id);

        // Soft-deleting note_b does NOT redirect to note_a
        crate::storage::services::TrashService::soft_delete(&conn, &note_b.id).unwrap();
        let res_b_trashed = InternalLinkService::resolve_link_target(&conn, &note_b.id).unwrap();
        assert_eq!(res_b_trashed.lifecycle, LinkTargetLifecycle::Trashed);
        assert_eq!(res_b_trashed.target_id, note_b.id);

        // 4. Reliability & Performance: Zero links note short-circuit
        let empty_content =
            "This is a note with no links whatsoever. Just regular text.".repeat(50);
        let parsed_empty = LinkParser::parse_internal_links(&empty_content);
        assert_eq!(parsed_empty.len(), 0);

        // 5. Performance: 100KB note with 100 links parses linearly
        let mut large_content = String::with_capacity(120_000);
        for i in 0..100 {
            large_content.push_str(&format!(
                "Paragraph {} discussing [Note {}](notepad://note/uuid-target-{}) in detail. ",
                i, i, i
            ));
            large_content
                .push_str(&"Filler text without links for performance testing. ".repeat(18));
        }
        let start_time = std::time::Instant::now();
        let parsed_large = LinkParser::parse_internal_links(&large_content);
        let elapsed = start_time.elapsed();
        assert_eq!(parsed_large.len(), 100);
        assert!(
            elapsed.as_millis() < 500,
            "100KB note parsing took too long: {:?}",
            elapsed
        );

        // 6. Reliability: Backlink query handles multiple source notes
        let target_shared = crate::storage::repositories::NoteRepository::create(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Shared Hub".to_string()),
                content: Some("Hub body".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        for i in 0..15 {
            crate::storage::repositories::NoteRepository::create(
                &conn,
                crate::storage::models::CreateNoteDto {
                    title: Some(format!("Source Note {}", i)),
                    content: Some(format!(
                        "Link to hub: [Hub](notepad://note/{})",
                        target_shared.id
                    )),
                    ..Default::default()
                },
            )
            .unwrap();
        }

        let backlinks = InternalLinkService::get_backlinks(&conn, &target_shared.id).unwrap();
        assert_eq!(
            backlinks.len(),
            15,
            "All 15 source notes must be indexed as backlinks"
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_internal_links_encrypted_resolution_and_backlinks() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_links_enc_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = crate::storage::database::init_connection(&db_path).unwrap();
        crate::storage::migrations::run_migrations(&mut conn).unwrap();

        let dek = crate::storage::crypto::generate_dek();

        // 1. Create target note with encryption
        let target_note = NoteRepository::create_with_key(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Top Secret Target".to_string()),
                content: Some("Target secret body".to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .unwrap();

        // 2. Resolve link target WITH DEK -> resolves decrypted title
        let resolved_unlocked =
            InternalLinkService::resolve_link_target_with_key(&conn, &target_note.id, Some(&dek))
                .unwrap();
        assert_eq!(
            resolved_unlocked.title.as_deref(),
            Some("Top Secret Target")
        );
        assert_eq!(resolved_unlocked.lifecycle, LinkTargetLifecycle::Active);

        // 3. Resolve link target WITHOUT DEK -> rejects because note title is encrypted
        let resolved_locked_err =
            InternalLinkService::resolve_link_target_with_key(&conn, &target_note.id, None)
                .unwrap_err();
        assert!(matches!(resolved_locked_err, InternalLinkError::Crypto(_)));

        // 4. Create source note with encrypted content pointing to target
        let link_uri = format!("notepad://note/{}", target_note.id);
        let _source_note = NoteRepository::create_with_key(
            &conn,
            crate::storage::models::CreateNoteDto {
                title: Some("Source Document".to_string()),
                content: Some(format!(
                    "Please see confidential document [Target Note]({}) for details.",
                    link_uri
                )),
                ..Default::default()
            },
            Some(&dek),
        )
        .unwrap();

        // 5. Discover backlinks WITH DEK -> decrypts content and extracts snippet
        let backlinks =
            InternalLinkService::get_backlinks_with_key(&conn, &target_note.id, Some(&dek))
                .unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_note_title, "Source Document");
        assert!(backlinks[0].label_snippet.is_some());
        assert!(backlinks[0]
            .label_snippet
            .as_ref()
            .unwrap()
            .contains("confidential document"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
