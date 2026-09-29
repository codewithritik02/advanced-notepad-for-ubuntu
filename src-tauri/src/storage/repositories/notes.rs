use rusqlite::{params, Connection, Row};
use crate::storage::errors::StorageError;
use crate::storage::models::{CreateNoteDto, Note, UpdateNoteDto};

pub struct NoteRepository;

impl NoteRepository {
    fn map_row(row: &Row) -> rusqlite::Result<Note> {
        let is_fav_int: i32 = row.get(7)?;
        let is_pin_int: i32 = row.get(8)?;
        let is_del_int: i32 = row.get(9)?;

        Ok(Note {
            id: row.get(0)?,
            title: row.get(1)?,
            content: row.get(2)?,
            format: row.get(3)?,
            notebook_id: row.get(4)?,
            created_at: row.get(5)?,
            modified_at: row.get(6)?,
            is_favorite: is_fav_int != 0,
            is_pinned: is_pin_int != 0,
            is_deleted: is_del_int != 0,
            deleted_at: row.get(10)?,
        })
    }

    pub fn create(conn: &Connection, dto: CreateNoteDto) -> Result<Note, StorageError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let title = dto.title.unwrap_or_else(|| "Untitled Note".to_string());
        let content = dto.content.unwrap_or_default();
        let format = dto.format.unwrap_or_else(|| "txt".to_string());
        let is_favorite = dto.is_favorite.unwrap_or(false);
        let is_pinned = dto.is_pinned.unwrap_or(false);

        conn.execute(
            "INSERT INTO notes (
                id, title, content, format, notebook_id, created_at, modified_at,
                is_favorite, is_pinned, is_deleted, deleted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, NULL)",
            params![
                id,
                title,
                content,
                format,
                dto.notebook_id,
                now,
                now,
                if is_favorite { 1 } else { 0 },
                if is_pinned { 1 } else { 0 },
            ],
        )?;

        Ok(Note {
            id,
            title,
            content,
            format,
            notebook_id: dto.notebook_id,
            created_at: now.clone(),
            modified_at: now,
            is_favorite,
            is_pinned,
            is_deleted: false,
            deleted_at: None,
        })
    }

    /// Atomically creates a note and its associated tag relationships within a single transaction.
    /// Automatically rolls back if any tag constraint fails.
    pub fn create_with_tags(
        conn: &mut Connection,
        dto: CreateNoteDto,
        tag_ids: &[String],
    ) -> Result<Note, StorageError> {
        let tx = conn.transaction()?;

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let title = dto.title.unwrap_or_else(|| "Untitled Note".to_string());
        let content = dto.content.unwrap_or_default();
        let format = dto.format.unwrap_or_else(|| "txt".to_string());
        let is_favorite = dto.is_favorite.unwrap_or(false);
        let is_pinned = dto.is_pinned.unwrap_or(false);

        tx.execute(
            "INSERT INTO notes (
                id, title, content, format, notebook_id, created_at, modified_at,
                is_favorite, is_pinned, is_deleted, deleted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, NULL)",
            params![
                id,
                title,
                content,
                format,
                dto.notebook_id,
                now,
                now,
                if is_favorite { 1 } else { 0 },
                if is_pinned { 1 } else { 0 },
            ],
        )?;

        for tag_id in tag_ids {
            tx.execute(
                "INSERT INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
                params![id, tag_id],
            )?;
        }

        tx.commit()?;

        Ok(Note {
            id,
            title,
            content,
            format,
            notebook_id: dto.notebook_id,
            created_at: now.clone(),
            modified_at: now,
            is_favorite,
            is_pinned,
            is_deleted: false,
            deleted_at: None,
        })
    }

    pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<Note>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::map_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list(conn: &Connection, include_deleted: bool) -> Result<Vec<Note>, StorageError> {
        let sql = if include_deleted {
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             ORDER BY is_pinned DESC, modified_at DESC"
        } else {
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             WHERE is_deleted = 0
             ORDER BY is_pinned DESC, modified_at DESC"
        };

        let mut stmt = conn.prepare(sql)?;
        let note_iter = stmt.query_map([], Self::map_row)?;

        let mut notes = Vec::new();
        for note in note_iter {
            notes.push(note?);
        }
        Ok(notes)
    }

    pub fn update(conn: &Connection, id: &str, dto: UpdateNoteDto) -> Result<Note, StorageError> {
        let existing = Self::get_by_id(conn, id)?
            .ok_or_else(|| StorageError::NotFound(format!("Note with id '{id}' not found")))?;

        let now = chrono::Utc::now().to_rfc3339();
        let title = dto.title.unwrap_or(existing.title);
        let content = dto.content.unwrap_or(existing.content);
        let format = dto.format.unwrap_or(existing.format);
        let notebook_id = dto.notebook_id.or(existing.notebook_id);
        let is_favorite = dto.is_favorite.unwrap_or(existing.is_favorite);
        let is_pinned = dto.is_pinned.unwrap_or(existing.is_pinned);
        let is_deleted = dto.is_deleted.unwrap_or(existing.is_deleted);
        let deleted_at = if is_deleted && !existing.is_deleted {
            Some(now.clone())
        } else if !is_deleted {
            None
        } else {
            existing.deleted_at
        };

        conn.execute(
            "UPDATE notes
             SET title = ?1, content = ?2, format = ?3, notebook_id = ?4,
                 modified_at = ?5, is_favorite = ?6, is_pinned = ?7,
                 is_deleted = ?8, deleted_at = ?9
             WHERE id = ?10",
            params![
                title,
                content,
                format,
                notebook_id,
                now,
                if is_favorite { 1 } else { 0 },
                if is_pinned { 1 } else { 0 },
                if is_deleted { 1 } else { 0 },
                deleted_at,
                id,
            ],
        )?;

        Ok(Note {
            id: id.to_string(),
            title,
            content,
            format,
            notebook_id,
            created_at: existing.created_at,
            modified_at: now,
            is_favorite,
            is_pinned,
            is_deleted,
            deleted_at,
        })
    }

    pub fn delete(conn: &Connection, id: &str, soft: bool) -> Result<bool, StorageError> {
        if soft {
            let now = chrono::Utc::now().to_rfc3339();
            let rows_affected = conn.execute(
                "UPDATE notes SET is_deleted = 1, deleted_at = ?1, modified_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            Ok(rows_affected > 0)
        } else {
            let rows_affected = conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
            Ok(rows_affected > 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::repositories::tags::TagRepository;

    #[test]
    fn test_atomic_transaction_and_rollback_on_failure() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_tx_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let tag = TagRepository::create(&conn, "valid-tag").expect("Create tag failed");

        // 1. Transaction failure: Attempt to create note with one valid tag and one non-existent tag
        let invalid_tag_id = "non-existent-tag-id".to_string();
        let res = NoteRepository::create_with_tags(
            &mut conn,
            CreateNoteDto {
                title: Some("Should Rollback".to_string()),
                content: Some("This note must not be persisted".to_string()),
                ..Default::default()
            },
            &[tag.id.clone(), invalid_tag_id],
        );
        assert!(res.is_err(), "Invalid tag must trigger error");

        // Verify that the note was NOT inserted into notes table
        let all_notes = NoteRepository::list(&conn, true).expect("List notes failed");
        assert_eq!(all_notes.len(), 0, "Failed transaction must roll back and leave 0 notes");

        // 2. Transaction success: Valid tags
        let success_res = NoteRepository::create_with_tags(
            &mut conn,
            CreateNoteDto {
                title: Some("Atomic Success".to_string()),
                content: Some("Both note and tag must be persisted".to_string()),
                ..Default::default()
            },
            std::slice::from_ref(&tag.id),
        ).expect("Valid transaction should succeed");

        let fetched_notes = NoteRepository::list(&conn, true).expect("List notes failed");
        assert_eq!(fetched_notes.len(), 1);
        assert_eq!(fetched_notes[0].id, success_res.id);

        let note_tags = TagRepository::get_tags_for_note(&conn, &success_res.id).expect("Get tags failed");
        assert_eq!(note_tags.len(), 1);
        assert_eq!(note_tags[0].id, tag.id);

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
