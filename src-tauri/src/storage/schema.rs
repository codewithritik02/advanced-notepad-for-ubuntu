use rusqlite::Connection;
use crate::storage::errors::StorageError;

/// SQL DDL for initial database schema (Migration 001)
pub const INITIAL_SCHEMA_SQL: &str = r#"
-- 1. Notebooks table (supports nested hierarchy via parent_id)
CREATE TABLE IF NOT EXISTS notebooks (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    parent_id TEXT,
    created_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,
    FOREIGN KEY (parent_id) REFERENCES notebooks(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_notebooks_parent_id ON notebooks(parent_id);

-- 2. Notes table
CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL DEFAULT 'Untitled Note',
    content TEXT NOT NULL DEFAULT '',
    format TEXT NOT NULL DEFAULT 'txt',
    notebook_id TEXT,
    created_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,
    is_favorite INTEGER NOT NULL DEFAULT 0,
    is_pinned INTEGER NOT NULL DEFAULT 0,
    is_deleted INTEGER NOT NULL DEFAULT 0,
    deleted_at TEXT,
    FOREIGN KEY (notebook_id) REFERENCES notebooks(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_notes_modified_at ON notes(modified_at DESC);
CREATE INDEX IF NOT EXISTS idx_notes_notebook_id ON notes(notebook_id);
CREATE INDEX IF NOT EXISTS idx_notes_is_deleted ON notes(is_deleted);
CREATE INDEX IF NOT EXISTS idx_notes_is_favorite ON notes(is_favorite);

-- 3. Tags table
CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

-- 4. Note Tags junction table (Many-to-Many)
CREATE TABLE IF NOT EXISTS note_tags (
    note_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (note_id, tag_id),
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_note_tags_tag_id ON note_tags(tag_id);

-- 5. Attachments metadata table
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY NOT NULL,
    note_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_attachments_note_id ON attachments(note_id);

-- 6. Settings table (Key/Value configuration storage)
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
"#;

/// SQL DDL for FTS5 full-text search index (Migration 002, Task 33)
pub const MIGRATION_002_FTS5_SQL: &str = r#"
-- 1. Create FTS5 virtual table referencing notes external content
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
    title,
    content,
    content='notes',
    content_rowid='rowid'
);

-- 2. Populate FTS5 index from all existing notes (Task 34)
INSERT INTO notes_fts(notes_fts) VALUES('rebuild');

-- 3. Automatic synchronization triggers (Task 35, 36, 37)
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
"#;

/// Applies the initial schema to the provided database connection.
pub fn apply_initial_schema(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(INITIAL_SCHEMA_SQL)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;

    #[test]
    fn test_apply_initial_schema_and_constraints() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_schema_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let conn = init_connection(&db_path).expect("Database initialization failed");

        // Apply schema
        apply_initial_schema(&conn).expect("Failed to apply initial schema");

        // 1. Verify foreign key constraint enforcement
        // Inserting into note_tags with non-existent note_id must fail
        let invalid_relation = conn.execute(
            "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            ["non-existent-note", "non-existent-tag"],
        );
        assert!(invalid_relation.is_err(), "Foreign key violation must be caught");

        // 2. Test notebook creation with hierarchy (parent_id)
        let parent_id = uuid::Uuid::new_v4().to_string();
        let child_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at) VALUES (?1, ?2, NULL, ?3, ?3)",
            [&parent_id, "Parent Notebook", &now],
        ).expect("Insert parent notebook failed");

        conn.execute(
            "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            [&child_id, "Child Notebook", &parent_id, &now],
        ).expect("Insert child notebook failed");

        // 3. Test note insertion with Unicode (Hindi + English + Emoji)
        let note_id = uuid::Uuid::new_v4().to_string();
        let title = "आज की मीटिंग के नोट्स 📝";
        let content = "नमस्ते दुनिया! Personal Notepad works completely offline with zero telemetry.";

        conn.execute(
            "INSERT INTO notes (id, title, content, format, notebook_id, created_at, modified_at, is_favorite, is_pinned, is_deleted)
             VALUES (?1, ?2, ?3, 'txt', ?4, ?5, ?5, 1, 0, 0)",
            [&note_id, title, content, &child_id, &now],
        ).expect("Insert note failed");

        let retrieved_title: String = conn.query_row(
            "SELECT title FROM notes WHERE id = ?1",
            [&note_id],
            |r| r.get(0),
        ).expect("Retrieve note failed");
        assert_eq!(retrieved_title, title, "Unicode characters must match exactly");

        // 4. Test Tag and Note-Tag relationship
        let tag_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
            [&tag_id, "महत्वपूर्ण-work", &now],
        ).expect("Insert tag failed");

        // Duplicate tag name should fail unique constraint
        let dup_tag = conn.execute(
            "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
            [&uuid::Uuid::new_v4().to_string(), "महत्वपूर्ण-work", &now],
        );
        assert!(dup_tag.is_err(), "Duplicate tag names must be rejected by UNIQUE constraint");

        conn.execute(
            "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            [&note_id, &tag_id],
        ).expect("Link note and tag failed");

        // Duplicate composite key should fail
        let dup_relation = conn.execute(
            "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            [&note_id, &tag_id],
        );
        assert!(dup_relation.is_err(), "Duplicate note-tag pair must fail composite primary key");

        // 5. Test Settings table (key/value)
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('theme', 'dark')",
            [],
        ).expect("Insert setting failed");

        let theme_val: String = conn.query_row(
            "SELECT value FROM settings WHERE key = 'theme'",
            [],
            |r| r.get(0),
        ).expect("Retrieve setting failed");
        assert_eq!(theme_val, "dark");

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
