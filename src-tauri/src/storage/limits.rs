use crate::storage::errors::StorageError;

/// Centralized Resource Limits and Reliability Invariants (Phase 13, Task 2)
///
/// Defines practical limits for potentially expensive operations across the backend
/// and provides centralized validation to prevent resource exhaustion, unbounded memory usage,
/// and silent data truncation.

// --- Note Content & Title Bounds ---
/// Maximum note content size in bytes: 10 MiB (10,485,760 bytes).
/// Realistic notes rarely exceed 100 KiB. 10 MiB supports very large plain text / markdown files
/// while guarding against multi-gigabyte memory exhaustion during AES-GCM encryption / IPC transfer.
pub const MAX_NOTE_CONTENT_BYTES: usize = 10 * 1024 * 1024;

/// Maximum note title length in Unicode characters: 500 characters.
pub const MAX_NOTE_TITLE_CHARS: usize = 500;

/// Maximum tag name length in Unicode characters: 100 characters.
pub const MAX_TAG_NAME_CHARS: usize = 100;

/// Maximum notebook name length in Unicode characters: 200 characters.
pub const MAX_NOTEBOOK_NAME_CHARS: usize = 200;

/// Maximum notebook hierarchy depth: 3 levels.
pub const MAX_NOTEBOOK_NESTING_DEPTH: usize = 3;

// --- Attachment Bounds ---
/// Default maximum attachment file size: 100 MiB (104,857,600 bytes).
pub const MAX_ATTACHMENT_SIZE_BYTES: u64 = 100 * 1024 * 1024;

// --- Import / Export Bounds ---
/// Maximum individual import file size: 10 MiB.
pub const MAX_IMPORT_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;

/// Maximum aggregate import batch size: 100 MiB.
pub const MAX_IMPORT_BATCH_SIZE_BYTES: u64 = 100 * 1024 * 1024;

/// Maximum number of files in an import or export batch: 500 items.
pub const MAX_BATCH_FILE_COUNT: usize = 500;

// --- Backup & Restore Bounds ---
/// Maximum total uncompressed backup archive size: 5 GiB.
pub const MAX_BACKUP_UNCOMPRESSED_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// Maximum number of entries inside a backup archive: 10,000 entries.
pub const MAX_BACKUP_ENTRY_COUNT: usize = 10_000;

/// Maximum number of recovery checkpoints retained before pruning: 10.
pub const MAX_CHECKPOINT_COUNT: usize = 10;

// --- Search Bounds ---
/// Maximum search query length in Unicode characters: 1,000 characters.
pub const MAX_SEARCH_QUERY_LENGTH: usize = 1000;

/// Default search result limit.
pub const DEFAULT_SEARCH_LIMIT: usize = 50;

/// Maximum search result limit.
pub const MAX_SEARCH_LIMIT: usize = 100;

// --- Pagination & Bulk Operations Bounds ---
/// Default pagination page size for list queries: 50.
pub const DEFAULT_PAGE_SIZE: usize = 50;

/// Maximum allowed page size for paginated queries: 200.
pub const MAX_PAGE_SIZE: usize = 200;

/// Maximum items permitted in a single bulk operation: 500.
pub const MAX_BULK_OPERATION_ITEMS: usize = 500;

/// Maximum concurrent background backup or restore operations: 1.
pub const MAX_CONCURRENT_BACKUP_OPERATIONS: usize = 1;

// --- Validation Functions ---

/// Validates note title length against `MAX_NOTE_TITLE_CHARS`.
/// Never truncates silently; returns an actionable error if violated.
pub fn validate_note_title(title: Option<&str>) -> Result<(), StorageError> {
    if let Some(t) = title {
        let char_count = t.chars().count();
        if char_count > MAX_NOTE_TITLE_CHARS {
            return Err(StorageError::Validation(format!(
                "Note title exceeds the maximum allowed limit of {MAX_NOTE_TITLE_CHARS} characters (received {char_count})."
            )));
        }
    }
    Ok(())
}

/// Validates note content size in bytes against `MAX_NOTE_CONTENT_BYTES`.
/// Never truncates silently; returns an actionable error if violated.
pub fn validate_note_content(content: Option<&str>) -> Result<(), StorageError> {
    if let Some(c) = content {
        let byte_len = c.len();
        if byte_len > MAX_NOTE_CONTENT_BYTES {
            let max_mb = MAX_NOTE_CONTENT_BYTES / (1024 * 1024);
            let actual_mb = byte_len as f64 / (1024.0 * 1024.0);
            return Err(StorageError::Validation(format!(
                "Note content size ({actual_mb:.2} MB) exceeds the maximum allowed limit of {max_mb} MB."
            )));
        }
    }
    Ok(())
}

/// Validates tag name length against `MAX_TAG_NAME_CHARS`.
pub fn validate_tag_name(name: &str) -> Result<(), StorageError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(StorageError::Validation("Tag name cannot be empty".to_string()));
    }
    let char_count = trimmed.chars().count();
    if char_count > MAX_TAG_NAME_CHARS {
        return Err(StorageError::Validation(format!(
            "Tag name cannot exceed {MAX_TAG_NAME_CHARS} characters"
        )));
    }
    Ok(())
}

/// Validates notebook name length against `MAX_NOTEBOOK_NAME_CHARS`.
pub fn validate_notebook_name(name: &str) -> Result<(), StorageError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(StorageError::Validation("Notebook name cannot be empty".to_string()));
    }
    let char_count = trimmed.chars().count();
    if char_count > MAX_NOTEBOOK_NAME_CHARS {
        return Err(StorageError::Validation(format!(
            "Notebook name exceeds the maximum allowed limit of {MAX_NOTEBOOK_NAME_CHARS} characters (received {char_count})."
        )));
    }
    Ok(())
}

/// Validates requested bulk item count.
pub fn validate_bulk_operation_count(count: usize) -> Result<(), StorageError> {
    if count == 0 {
        return Err(StorageError::Validation("No items selected for bulk operation.".to_string()));
    }
    if count > MAX_BULK_OPERATION_ITEMS {
        return Err(StorageError::Validation(format!(
            "Bulk operation item count ({count}) exceeds maximum allowed limit of {MAX_BULK_OPERATION_ITEMS} items."
        )));
    }
    Ok(())
}

/// Validates and clamps pagination parameters.
pub fn sanitize_pagination_bounds(limit: Option<usize>) -> usize {
    match limit {
        Some(l) if l > 0 => l.min(MAX_PAGE_SIZE),
        _ => DEFAULT_PAGE_SIZE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_note_title_within_and_exceeding_limit() {
        assert!(validate_note_title(Some("Short title")).is_ok());
        assert!(validate_note_title(None).is_ok());

        let exactly_500 = "a".repeat(500);
        assert!(validate_note_title(Some(&exactly_500)).is_ok());

        let exceed_501 = "a".repeat(501);
        let res = validate_note_title(Some(&exceed_501));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("exceeds the maximum allowed limit of 500 characters"));
    }

    #[test]
    fn test_validate_note_content_size() {
        assert!(validate_note_content(Some("Hello world")).is_ok());
        assert!(validate_note_content(None).is_ok());

        let max_content = vec![b'x'; 1024]; // 1 KB
        let content_str = String::from_utf8(max_content).unwrap();
        assert!(validate_note_content(Some(&content_str)).is_ok());

        // Note content exceeding 10 MiB
        let oversized = "x".repeat(MAX_NOTE_CONTENT_BYTES + 1);
        let res = validate_note_content(Some(&oversized));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("exceeds the maximum allowed limit of 10 MB"));
    }

    #[test]
    fn test_validate_tag_name() {
        assert!(validate_tag_name("rust").is_ok());
        assert!(validate_tag_name("").is_err());
        assert!(validate_tag_name("   ").is_err());

        let exactly_100 = "t".repeat(100);
        assert!(validate_tag_name(&exactly_100).is_ok());

        let exceed_101 = "t".repeat(101);
        assert!(validate_tag_name(&exceed_101).is_err());
    }

    #[test]
    fn test_validate_notebook_name() {
        assert!(validate_notebook_name("Personal").is_ok());
        assert!(validate_notebook_name("").is_err());

        let exactly_200 = "n".repeat(200);
        assert!(validate_notebook_name(&exactly_200).is_ok());

        let exceed_201 = "n".repeat(201);
        assert!(validate_notebook_name(&exceed_201).is_err());
    }

    #[test]
    fn test_validate_bulk_operation_count() {
        assert!(validate_bulk_operation_count(1).is_ok());
        assert!(validate_bulk_operation_count(500).is_ok());
        assert!(validate_bulk_operation_count(0).is_err());
        assert!(validate_bulk_operation_count(501).is_err());
    }

    #[test]
    fn test_sanitize_pagination_bounds() {
        assert_eq!(sanitize_pagination_bounds(None), DEFAULT_PAGE_SIZE);
        assert_eq!(sanitize_pagination_bounds(Some(0)), DEFAULT_PAGE_SIZE);
        assert_eq!(sanitize_pagination_bounds(Some(25)), 25);
        assert_eq!(sanitize_pagination_bounds(Some(200)), 200);
        assert_eq!(sanitize_pagination_bounds(Some(500)), MAX_PAGE_SIZE);
    }
}
