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
        Self::list_filtered(conn, include_deleted, None, false)
    }

    pub fn list_by_notebook(conn: &Connection, notebook_id: &str) -> Result<Vec<Note>, StorageError> {
        Self::list_filtered(conn, false, Some(notebook_id), false)
    }

    pub fn list_unfiled(conn: &Connection) -> Result<Vec<Note>, StorageError> {
        Self::list_filtered(conn, false, None, true)
    }

    pub fn list_filtered(
        conn: &Connection,
        include_deleted: bool,
        notebook_id: Option<&str>,
        unfiled_only: bool,
    ) -> Result<Vec<Note>, StorageError> {
        let mut sql = String::from(
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             WHERE 1=1",
        );

        let mut params_vec: Vec<rusqlite::types::Value> = Vec::new();

        if !include_deleted {
            sql.push_str(" AND is_deleted = 0");
        }

        if unfiled_only {
            sql.push_str(" AND notebook_id IS NULL");
        } else if let Some(nb_id) = notebook_id {
            sql.push_str(" AND notebook_id = ?");
            params_vec.push(rusqlite::types::Value::Text(nb_id.to_string()));
        }

        sql.push_str(" ORDER BY is_pinned DESC, modified_at DESC");

        let mut stmt = conn.prepare(&sql)?;
        let note_iter = stmt.query_map(rusqlite::params_from_iter(params_vec), Self::map_row)?;

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

    pub fn restore(conn: &Connection, id: &str) -> Result<Note, StorageError> {
        Self::update(
            conn,
            id,
            UpdateNoteDto {
                is_deleted: Some(false),
                ..Default::default()
            },
        )
    }

    pub fn move_to_notebook(
        conn: &Connection,
        id: &str,
        notebook_id: Option<&str>,
    ) -> Result<Note, StorageError> {
        let existing = Self::get_by_id(conn, id)?
            .ok_or_else(|| StorageError::NotFound(format!("Note with id '{id}' not found")))?;

        if let Some(target_nb_id) = notebook_id {
            let nb_exists: bool = conn.query_row(
                "SELECT COUNT(1) FROM notebooks WHERE id = ?1",
                params![target_nb_id],
                |row| row.get::<_, i64>(0),
            )? > 0;

            if !nb_exists {
                return Err(StorageError::Validation(format!(
                    "Notebook with id '{target_nb_id}' does not exist"
                )));
            }
        }

        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE notes
             SET notebook_id = ?1, modified_at = ?2
             WHERE id = ?3",
            params![notebook_id, now, id],
        )?;

        let mut updated = existing;
        updated.notebook_id = notebook_id.map(|s| s.to_string());
        updated.modified_at = now;
        Ok(updated)
    }

    pub fn set_favorite(
        conn: &Connection,
        id: &str,
        is_favorite: bool,
    ) -> Result<Note, StorageError> {
        Self::update(
            conn,
            id,
            UpdateNoteDto {
                is_favorite: Some(is_favorite),
                ..Default::default()
            },
        )
    }

    pub fn toggle_favorite(
        conn: &Connection,
        id: &str,
    ) -> Result<Note, StorageError> {
        let note = Self::get_by_id(conn, id)?
            .ok_or_else(|| StorageError::NotFound(format!("Note with id '{id}' not found")))?;
        Self::set_favorite(conn, id, !note.is_favorite)
    }

    pub fn list_favorites(conn: &Connection) -> Result<Vec<Note>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             WHERE is_deleted = 0 AND is_favorite = 1
             ORDER BY is_pinned DESC, modified_at DESC",
        )?;
        let note_iter = stmt.query_map([], Self::map_row)?;
        let mut notes = Vec::new();
        for note in note_iter {
            notes.push(note?);
        }
        Ok(notes)
    }

    pub fn list_by_tag(conn: &Connection, tag_id: &str) -> Result<Vec<Note>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT n.id, n.title, n.content, n.format, n.notebook_id, n.created_at, n.modified_at,
                    n.is_favorite, n.is_pinned, n.is_deleted, n.deleted_at
             FROM notes n
             JOIN note_tags nt ON nt.note_id = n.id
             WHERE nt.tag_id = ?1
               AND n.is_deleted = 0
             ORDER BY n.is_pinned DESC, n.modified_at DESC",
        )?;

        let note_iter = stmt.query_map(params![tag_id], Self::map_row)?;
        let mut notes = Vec::new();
        for note in note_iter {
            notes.push(note?);
        }
        Ok(notes)
    }

    pub fn get_metadata(
        conn: &Connection,
        id: &str,
    ) -> Result<Option<crate::storage::models::NoteMetadata>, StorageError> {
        let note = match Self::get_by_id(conn, id)? {
            Some(n) => n,
            None => return Ok(None),
        };
        let tags = crate::storage::repositories::tags::TagRepository::get_tags_for_note(conn, id)?;
        let mut metadata = crate::storage::models::NoteMetadata::from_note_and_tags(&note, tags);
        if let Some(ref nb_id) = note.notebook_id {
            let path = crate::storage::repositories::notebooks::NotebookRepository::get_hierarchy_path(conn, nb_id)?;
            metadata.notebook_path = Some(path);
        }
        Ok(Some(metadata))
    }

    /// Returns the human-readable notebook path for a note (or "Unfiled" if unassigned).
    pub fn get_notebook_path(conn: &Connection, note_id: &str) -> Result<String, StorageError> {
        let note = Self::get_by_id(conn, note_id)?
            .ok_or_else(|| StorageError::NotFound(format!("Note with id '{note_id}' not found")))?;
        match note.notebook_id {
            Some(ref nb_id) => crate::storage::repositories::notebooks::NotebookRepository::get_hierarchy_path(conn, nb_id),
            None => Ok("Unfiled".to_string()),
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

    #[test]
    fn test_task_61_favorite_tests() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_t61_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Create a note (starts with is_favorite = false)
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Favorite Note 1".to_string()),
                content: Some("First note content".to_string()),
                ..Default::default()
            },
        ).expect("create note 1");
        assert!(!note1.is_favorite);

        // Favorite note
        let fav1 = NoteRepository::toggle_favorite(&conn, &note1.id).expect("favorite note 1");
        assert!(fav1.is_favorite, "1. note must be favorited");

        // 2. Unfavorite note
        let unfav1 = NoteRepository::toggle_favorite(&conn, &note1.id).expect("unfavorite note 1");
        assert!(!unfav1.is_favorite, "2. note must be unfavorited");

        // Re-favorite note 1 for persistence testing
        NoteRepository::toggle_favorite(&conn, &note1.id).expect("re-favorite note 1");

        // 3. Favorite persists across connection drop / reopen
        drop(conn);
        let conn2 = init_connection(&db_path).expect("Reopen failed");
        let note1_reloaded = NoteRepository::get_by_id(&conn2, &note1.id).unwrap().unwrap();
        assert!(note1_reloaded.is_favorite, "3. favorite must persist across connection reload");

        // 4. Favorite filtering
        let note2 = NoteRepository::create(
            &conn2,
            CreateNoteDto {
                title: Some("Standard Note 2".to_string()),
                content: Some("Second note content (not favorite)".to_string()),
                ..Default::default()
            },
        ).expect("create note 2");
        assert!(!note2.is_favorite);

        let note3 = NoteRepository::create(
            &conn2,
            CreateNoteDto {
                title: Some("Favorite Note 3".to_string()),
                content: Some("Third note content (favorite)".to_string()),
                ..Default::default()
            },
        ).expect("create note 3");
        NoteRepository::toggle_favorite(&conn2, &note3.id).expect("favorite note 3");

        let favorites = NoteRepository::list_favorites(&conn2).expect("4. list favorites");
        assert_eq!(favorites.len(), 2);
        assert!(favorites.iter().any(|n| n.id == note1.id));
        assert!(favorites.iter().any(|n| n.id == note3.id));
        assert!(!favorites.iter().any(|n| n.id == note2.id));

        // 5. Deleted note does not appear in Favorites
        NoteRepository::delete(&conn2, &note1.id, true).expect("soft delete note 1");
        let favorites_after_delete = NoteRepository::list_favorites(&conn2).expect("5. list favorites after delete");
        assert_eq!(favorites_after_delete.len(), 1);
        assert_eq!(favorites_after_delete[0].id, note3.id);

        drop(conn2);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_62_tag_filter_tests() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_t62_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Tag with no notes
        let empty_tag = TagRepository::create(&conn, "empty-tag").expect("create empty tag");
        let empty_notes = NoteRepository::list_by_tag(&conn, &empty_tag.id).expect("1. list empty tag notes");
        assert_eq!(empty_notes.len(), 0, "Tag with no notes must return empty vector");

        // 2. Tag with one note
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note 1".to_string()),
                content: Some("Content 1".to_string()),
                ..Default::default()
            },
        ).expect("create note 1");

        let solo_tag = TagRepository::create(&conn, "solo-tag").expect("create solo tag");
        TagRepository::add_tag_to_note(&conn, &note1.id, &solo_tag.id).expect("assign solo tag");

        let solo_notes = NoteRepository::list_by_tag(&conn, &solo_tag.id).expect("2. list solo tag notes");
        assert_eq!(solo_notes.len(), 1);
        assert_eq!(solo_notes[0].id, note1.id);

        // 3. Tag with multiple notes
        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note 2".to_string()),
                content: Some("Content 2".to_string()),
                ..Default::default()
            },
        ).expect("create note 2");

        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note 3".to_string()),
                content: Some("Content 3".to_string()),
                ..Default::default()
            },
        ).expect("create note 3");

        let shared_tag = TagRepository::create(&conn, "shared-tag").expect("create shared tag");
        TagRepository::add_tag_to_note(&conn, &note1.id, &shared_tag.id).expect("assign note 1");
        TagRepository::add_tag_to_note(&conn, &note2.id, &shared_tag.id).expect("assign note 2");
        TagRepository::add_tag_to_note(&conn, &note3.id, &shared_tag.id).expect("assign note 3");

        let shared_notes = NoteRepository::list_by_tag(&conn, &shared_tag.id).expect("3. list shared tag notes");
        assert_eq!(shared_notes.len(), 3);

        // 4. Deleted note excluded
        NoteRepository::delete(&conn, &note2.id, true).expect("soft delete note 2");
        let notes_after_soft_delete = NoteRepository::list_by_tag(&conn, &shared_tag.id).expect("4. list after soft delete");
        assert_eq!(notes_after_soft_delete.len(), 2, "Deleted note must be excluded from tag filter");
        assert!(notes_after_soft_delete.iter().any(|n| n.id == note1.id));
        assert!(notes_after_soft_delete.iter().any(|n| n.id == note3.id));
        assert!(!notes_after_soft_delete.iter().any(|n| n.id == note2.id));

        // 5. Multiple notes ordered correctly (is_pinned DESC, modified_at DESC)
        NoteRepository::update(
            &conn,
            &note3.id,
            UpdateNoteDto {
                is_pinned: Some(true),
                ..Default::default()
            },
        ).expect("pin note 3");
        let ordered_notes = NoteRepository::list_by_tag(&conn, &shared_tag.id).expect("5. list ordered notes");
        assert_eq!(ordered_notes.len(), 2);
        assert_eq!(ordered_notes[0].id, note3.id, "Pinned note must be ordered first");
        assert_eq!(ordered_notes[1].id, note1.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}


