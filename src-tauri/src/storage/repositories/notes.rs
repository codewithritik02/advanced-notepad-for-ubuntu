use crate::storage::crypto::{decrypt_string, encrypt_string, is_encrypted_string, SecretBytes};
use crate::storage::errors::StorageError;
use crate::storage::models::{
    CreateNoteDto, ListNotesQueryOptions, Note, NoteCountsDto, TrashedNoteItem, UpdateNoteDto,
};
use rusqlite::{params, Connection, Row};

pub struct NoteRepository;

impl NoteRepository {
    /// Decrypts a stored database field if it is encrypted with the versioned prefix (Task 9).
    /// If unencrypted, returns the raw plaintext string as-is.
    /// If encrypted and no DEK is supplied (e.g. application locked), returns an explicit error.
    pub fn decrypt_field_if_encrypted(
        raw_val: &str,
        note_id: &str,
        field_name: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<String, StorageError> {
        if !is_encrypted_string(raw_val) {
            return Ok(raw_val.to_string());
        }

        match dek {
            Some(key) => {
                let aad = format!("notes:{}:{}", note_id, field_name);
                let decrypted = decrypt_string(raw_val, key, aad.as_bytes())
                    .map_err(|e| StorageError::Crypto(e.user_friendly_message()))?;
                Ok(decrypted)
            }
            None => Err(StorageError::Crypto(
                "Application is locked. Please unlock to access encrypted notes.".to_string(),
            )),
        }
    }

    /// Encrypts a field value if an active DEK is provided (Task 9).
    /// Binds the note ID and field name as authenticated Associated Data (AAD) to prevent ciphertext splicing.
    pub fn encrypt_field_if_needed(
        val: &str,
        note_id: &str,
        field_name: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<String, StorageError> {
        match dek {
            Some(key) => {
                let aad = format!("notes:{}:{}", note_id, field_name);
                let encrypted = encrypt_string(val, key, aad.as_bytes())
                    .map_err(|e| StorageError::Crypto(e.user_friendly_message()))?;
                Ok(encrypted)
            }
            None => Ok(val.to_string()),
        }
    }

    fn map_row_with_key(row: &Row, dek: Option<&SecretBytes>) -> Result<Note, StorageError> {
        let id: String = row.get(0)?;
        let raw_title: String = row.get(1)?;
        let raw_content: String = row.get(2)?;
        let format: String = row.get(3)?;
        let notebook_id: Option<String> = row.get(4)?;
        let created_at: String = row.get(5)?;
        let modified_at: String = row.get(6)?;
        let is_fav_int: i32 = row.get(7)?;
        let is_pin_int: i32 = row.get(8)?;
        let is_del_int: i32 = row.get(9)?;
        let deleted_at: Option<String> = row.get(10)?;

        let title = Self::decrypt_field_if_encrypted(&raw_title, &id, "title", dek)?;
        let content = Self::decrypt_field_if_encrypted(&raw_content, &id, "content", dek)?;

        Ok(Note {
            id,
            title,
            content,
            format,
            notebook_id,
            created_at,
            modified_at,
            is_favorite: is_fav_int != 0,
            is_pinned: is_pin_int != 0,
            is_deleted: is_del_int != 0,
            deleted_at,
        })
    }

    pub fn create(conn: &Connection, dto: CreateNoteDto) -> Result<Note, StorageError> {
        Self::create_with_key(conn, dto, None)
    }

    pub fn create_with_key(
        conn: &Connection,
        dto: CreateNoteDto,
        dek: Option<&SecretBytes>,
    ) -> Result<Note, StorageError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let title = dto.title.unwrap_or_else(|| "Untitled Note".to_string());
        let content = dto.content.unwrap_or_default();
        crate::storage::limits::validate_note_title(Some(&title))?;
        crate::storage::limits::validate_note_content(Some(&content))?;
        let format = dto.format.unwrap_or_else(|| "txt".to_string());
        let is_favorite = dto.is_favorite.unwrap_or(false);
        let is_pinned = dto.is_pinned.unwrap_or(false);

        let stored_title = Self::encrypt_field_if_needed(&title, &id, "title", dek)?;
        let stored_content = Self::encrypt_field_if_needed(&content, &id, "content", dek)?;

        conn.execute(
            "INSERT INTO notes (
                id, title, content, format, notebook_id, created_at, modified_at,
                is_favorite, is_pinned, is_deleted, deleted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, NULL)",
            params![
                id,
                stored_title,
                stored_content,
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
        Self::create_with_tags_and_key(conn, dto, tag_ids, None)
    }

    pub fn create_with_tags_and_key(
        conn: &mut Connection,
        dto: CreateNoteDto,
        tag_ids: &[String],
        dek: Option<&SecretBytes>,
    ) -> Result<Note, StorageError> {
        let tx = conn.transaction()?;

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let title = dto.title.unwrap_or_else(|| "Untitled Note".to_string());
        let content = dto.content.unwrap_or_default();
        crate::storage::limits::validate_note_title(Some(&title))?;
        crate::storage::limits::validate_note_content(Some(&content))?;
        let format = dto.format.unwrap_or_else(|| "txt".to_string());
        let is_favorite = dto.is_favorite.unwrap_or(false);
        let is_pinned = dto.is_pinned.unwrap_or(false);

        let stored_title = Self::encrypt_field_if_needed(&title, &id, "title", dek)?;
        let stored_content = Self::encrypt_field_if_needed(&content, &id, "content", dek)?;

        tx.execute(
            "INSERT INTO notes (
                id, title, content, format, notebook_id, created_at, modified_at,
                is_favorite, is_pinned, is_deleted, deleted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, NULL)",
            params![
                id,
                stored_title,
                stored_content,
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
        Self::get_by_id_with_key(conn, id, None)
    }

    pub fn get_by_id_with_key(
        conn: &Connection,
        id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Option<Note>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::map_row_with_key(row, dek)?))
        } else {
            Ok(None)
        }
    }

    /// Retrieves a single active (non-deleted) note by ID (Phase 8, Task 4).
    pub fn get_active_by_id(conn: &Connection, id: &str) -> Result<Option<Note>, StorageError> {
        Self::get_active_by_id_with_key(conn, id, None)
    }

    pub fn get_active_by_id_with_key(
        conn: &Connection,
        id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Option<Note>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             WHERE id = ?1 AND is_deleted = 0",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::map_row_with_key(row, dek)?))
        } else {
            Ok(None)
        }
    }

    /// Retrieves a single trashed note by ID (Phase 8, Task 4).
    pub fn get_trashed_by_id(conn: &Connection, id: &str) -> Result<Option<Note>, StorageError> {
        Self::get_trashed_by_id_with_key(conn, id, None)
    }

    pub fn get_trashed_by_id_with_key(
        conn: &Connection,
        id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Option<Note>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             WHERE id = ?1 AND is_deleted = 1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::map_row_with_key(row, dek)?))
        } else {
            Ok(None)
        }
    }

    /// Lists only active (non-deleted) notes ordered by is_pinned DESC, modified_at DESC (Phase 8, Task 4).
    pub fn list_active(conn: &Connection) -> Result<Vec<Note>, StorageError> {
        Self::list_active_with_key(conn, None)
    }

    pub fn list_active_with_key(
        conn: &Connection,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<Note>, StorageError> {
        Self::list_filtered_with_key(conn, false, None, false, dek)
    }

    /// Lists only notes currently in Trash, ordered by deleted_at DESC, id ASC (Phase 8, Task 4 & 10).
    pub fn list_trashed(conn: &Connection) -> Result<Vec<Note>, StorageError> {
        Self::list_trashed_with_key(conn, None)
    }

    pub fn list_trashed_with_key(
        conn: &Connection,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<Note>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             WHERE is_deleted = 1
             ORDER BY deleted_at DESC, id ASC",
        )?;

        let mut rows = stmt.query([])?;
        let mut notes = Vec::new();
        while let Some(row) = rows.next()? {
            notes.push(Self::map_row_with_key(row, dek)?);
        }
        Ok(notes)
    }

    /// Lists lightweight TrashedNoteItem DTOs for the Trash UI (Phase 8, Task 10).
    pub fn list_trashed_items(conn: &Connection) -> Result<Vec<TrashedNoteItem>, StorageError> {
        Self::list_trashed_items_with_key(conn, None)
    }

    pub fn list_trashed_items_with_key(
        conn: &Connection,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<TrashedNoteItem>, StorageError> {
        Self::list_trashed_items_page_with_key(conn, None, None, dek)
    }

    pub fn list_trashed_items_page_with_key(
        conn: &Connection,
        limit: Option<usize>,
        offset: Option<usize>,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<TrashedNoteItem>, StorageError> {
        let mut sql = String::from(
            r#"
            SELECT
                n.id,
                n.title,
                n.content,
                n.notebook_id,
                nb.name AS notebook_name,
                n.created_at,
                n.modified_at,
                n.deleted_at,
                n.is_favorite,
                COALESCE(att.att_count, 0) AS attachment_count
            FROM notes n
            LEFT JOIN notebooks nb ON n.notebook_id = nb.id
            LEFT JOIN (
                SELECT note_id, COUNT(*) AS att_count
                FROM attachments
                GROUP BY note_id
            ) att ON n.id = att.note_id
            WHERE n.is_deleted = 1
            ORDER BY COALESCE(n.deleted_at, n.modified_at) DESC, n.id ASC
        "#,
        );

        if limit.is_some() {
            sql.push_str(" LIMIT ?1 OFFSET ?2");
        }

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = if let Some(limit) = limit {
            stmt.query(params![limit as i64, offset.unwrap_or(0) as i64])?
        } else {
            stmt.query([])?
        };
        let mut items = Vec::new();

        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let raw_title: String = row.get(1)?;
            let raw_content: String = row.get(2)?;
            let notebook_id: Option<String> = row.get(3)?;
            let notebook_name: Option<String> = row.get(4)?;
            let created_at: String = row.get(5)?;
            let modified_at: String = row.get(6)?;
            let deleted_at: Option<String> = row.get(7)?;
            let is_fav_int: i32 = row.get(8)?;
            let att_count: i64 = row.get(9)?;

            let title = Self::decrypt_field_if_encrypted(&raw_title, &id, "title", dek)?;
            let full_content = Self::decrypt_field_if_encrypted(&raw_content, &id, "content", dek)?;
            let preview = full_content.chars().take(200).collect::<String>();

            items.push(TrashedNoteItem {
                id,
                title,
                preview,
                notebook_id,
                notebook_name,
                created_at,
                modified_at,
                deleted_at,
                is_favorite: is_fav_int != 0,
                attachment_count: att_count.max(0) as usize,
            });
        }
        Ok(items)
    }

    /// Counts notes currently in Trash (Phase 8, Task 4 & 11).
    pub fn count_trashed(conn: &Connection) -> Result<usize, StorageError> {
        let mut stmt = conn.prepare("SELECT COUNT(1) FROM notes WHERE is_deleted = 1")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        Ok(count.max(0) as usize)
    }

    /// Counts active (non-deleted) notes (Phase 8, Task 4).
    pub fn count_active(conn: &Connection) -> Result<usize, StorageError> {
        let mut stmt = conn.prepare("SELECT COUNT(1) FROM notes WHERE is_deleted = 0")?;
        let count: i64 = stmt.query_row([], |r| r.get(0))?;
        Ok(count.max(0) as usize)
    }

    /// Returns aggregate note counts without loading or decrypting note records (Phase 13, Task 9)
    pub fn get_note_counts(conn: &Connection) -> Result<NoteCountsDto, StorageError> {
        let (total, favorites, unfiled): (i64, i64, i64) = conn.query_row(
            "SELECT 
                COUNT(1),
                COALESCE(SUM(CASE WHEN is_favorite = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN notebook_id IS NULL THEN 1 ELSE 0 END), 0)
             FROM notes 
             WHERE is_deleted = 0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;

        let mut nb_stmt = conn.prepare(
            "SELECT notebook_id, COUNT(1) 
             FROM notes 
             WHERE is_deleted = 0 AND notebook_id IS NOT NULL 
             GROUP BY notebook_id",
        )?;
        let mut by_notebook = std::collections::HashMap::new();
        let mut nb_rows = nb_stmt.query([])?;
        while let Some(row) = nb_rows.next()? {
            let nb_id: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            by_notebook.insert(nb_id, count.max(0) as usize);
        }

        let trash: i64 =
            conn.query_row("SELECT COUNT(1) FROM notes WHERE is_deleted = 1", [], |r| {
                r.get(0)
            })?;

        Ok(NoteCountsDto {
            total: total.max(0) as usize,
            favorites: favorites.max(0) as usize,
            unfiled: unfiled.max(0) as usize,
            trash: trash.max(0) as usize,
            by_notebook,
        })
    }

    pub fn list(conn: &Connection, include_deleted: bool) -> Result<Vec<Note>, StorageError> {
        Self::list_filtered(conn, include_deleted, None, false)
    }

    pub fn list_by_notebook(
        conn: &Connection,
        notebook_id: &str,
    ) -> Result<Vec<Note>, StorageError> {
        Self::list_filtered_with_key(conn, false, Some(notebook_id), false, None)
    }

    pub fn list_unfiled(conn: &Connection) -> Result<Vec<Note>, StorageError> {
        Self::list_filtered_with_key(conn, false, None, true, None)
    }

    pub fn list_filtered(
        conn: &Connection,
        include_deleted: bool,
        notebook_id: Option<&str>,
        unfiled_only: bool,
    ) -> Result<Vec<Note>, StorageError> {
        Self::list_filtered_with_key(conn, include_deleted, notebook_id, unfiled_only, None)
    }

    pub fn list_filtered_options(
        conn: &Connection,
        options: &ListNotesQueryOptions,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<Note>, StorageError> {
        let mut sql = String::from(
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             WHERE 1=1",
        );

        let mut params_vec: Vec<rusqlite::types::Value> = Vec::new();

        if !options.include_deleted {
            sql.push_str(" AND is_deleted = 0");
        }

        if options.unfiled_only {
            sql.push_str(" AND notebook_id IS NULL");
        } else if let Some(nb_id) = options.notebook_id {
            sql.push_str(" AND notebook_id = ?");
            params_vec.push(rusqlite::types::Value::Text(nb_id.to_string()));
        }

        sql.push_str(" ORDER BY is_pinned DESC, modified_at DESC, id ASC");

        if let Some(limit) = options.limit {
            sql.push_str(" LIMIT ? OFFSET ?");
            params_vec.push(rusqlite::types::Value::Integer(limit as i64));
            params_vec.push(rusqlite::types::Value::Integer(
                options.offset.unwrap_or(0) as i64
            ));
        }

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec))?;

        let mut notes = Vec::new();
        while let Some(row) = rows.next()? {
            let mut note = Self::map_row_with_key(row, dek)?;
            if let Some(max_len) = options.content_limit {
                if note.content.chars().count() > max_len {
                    note.content = note.content.chars().take(max_len).collect();
                }
            }
            notes.push(note);
        }
        Ok(notes)
    }

    pub fn list_filtered_with_key(
        conn: &Connection,
        include_deleted: bool,
        notebook_id: Option<&str>,
        unfiled_only: bool,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<Note>, StorageError> {
        Self::list_filtered_options(
            conn,
            &ListNotesQueryOptions {
                include_deleted,
                notebook_id,
                unfiled_only,
                limit: None,
                offset: None,
                content_limit: None,
            },
            dek,
        )
    }

    pub fn update(conn: &Connection, id: &str, dto: UpdateNoteDto) -> Result<Note, StorageError> {
        Self::update_with_key(conn, id, dto, None)
    }

    pub fn update_with_key(
        conn: &Connection,
        id: &str,
        dto: UpdateNoteDto,
        dek: Option<&SecretBytes>,
    ) -> Result<Note, StorageError> {
        let existing = Self::get_by_id_with_key(conn, id, dek)?
            .ok_or_else(|| StorageError::NotFound(format!("Note with id '{id}' not found")))?;

        if existing.is_deleted && dto.is_deleted != Some(false) {
            return Err(StorageError::Validation(
                "Cannot update a note that is in the trash. Restore it first.".to_string(),
            ));
        }

        let now = chrono::Utc::now().to_rfc3339();
        let title = dto.title.unwrap_or(existing.title);
        let content = dto.content.unwrap_or(existing.content);
        crate::storage::limits::validate_note_title(Some(&title))?;
        crate::storage::limits::validate_note_content(Some(&content))?;
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

        let stored_title = Self::encrypt_field_if_needed(&title, id, "title", dek)?;
        let stored_content = Self::encrypt_field_if_needed(&content, id, "content", dek)?;

        conn.execute(
            "UPDATE notes
             SET title = ?1, content = ?2, format = ?3, notebook_id = ?4,
                 modified_at = ?5, is_favorite = ?6, is_pinned = ?7,
                 is_deleted = ?8, deleted_at = ?9
             WHERE id = ?10",
            params![
                stored_title,
                stored_content,
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

    /// Soft deletes an active note, transitioning its lifecycle state to Trashed (Phase 8, Task 5).
    pub fn soft_delete(conn: &Connection, id: &str) -> Result<Note, StorageError> {
        Self::soft_delete_with_key(conn, id, None)
    }

    pub fn soft_delete_with_key(
        conn: &Connection,
        id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Note, StorageError> {
        let existing = Self::get_by_id_with_key(conn, id, dek)?
            .ok_or_else(|| StorageError::NotFound(format!("Note with id '{id}' not found")))?;

        if existing.is_deleted {
            return Err(StorageError::Conflict(format!(
                "Note with id '{id}' is already in Trash"
            )));
        }

        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE notes SET is_deleted = 1, deleted_at = ?1, modified_at = ?1 WHERE id = ?2 AND is_deleted = 0",
            params![now, id],
        )?;

        let mut updated = existing;
        updated.is_deleted = true;
        updated.deleted_at = Some(now.clone());
        updated.modified_at = now;
        Ok(updated)
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

    /// Restores a trashed note back to active state (Phase 8, Task 6).
    pub fn restore(conn: &Connection, id: &str) -> Result<Note, StorageError> {
        Self::restore_with_key(conn, id, None)
    }

    pub fn restore_with_key(
        conn: &Connection,
        id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Note, StorageError> {
        let existing = Self::get_by_id_with_key(conn, id, dek)?
            .ok_or_else(|| StorageError::NotFound(format!("Note with id '{id}' not found")))?;

        if !existing.is_deleted {
            // Already active: safe idempotent operation (Task 38 & Task 6)
            return Ok(existing);
        }

        let now = chrono::Utc::now().to_rfc3339();

        // Defensive check: if original notebook was removed while note was trashed, unfile safely
        let effective_notebook_id = if let Some(ref nb_id) = existing.notebook_id {
            let exists: bool = conn.query_row(
                "SELECT COUNT(1) FROM notebooks WHERE id = ?1",
                params![nb_id],
                |r| r.get::<_, i64>(0),
            )? > 0;
            if exists {
                Some(nb_id.clone())
            } else {
                None
            }
        } else {
            None
        };

        conn.execute(
            "UPDATE notes
             SET is_deleted = 0, deleted_at = NULL, notebook_id = ?1, modified_at = ?2
             WHERE id = ?3 AND is_deleted = 1",
            params![effective_notebook_id, now, id],
        )?;

        let mut restored = existing;
        restored.is_deleted = false;
        restored.deleted_at = None;
        restored.notebook_id = effective_notebook_id;
        restored.modified_at = now;
        Ok(restored)
    }

    pub fn move_to_notebook(
        conn: &Connection,
        id: &str,
        notebook_id: Option<&str>,
    ) -> Result<Note, StorageError> {
        Self::move_to_notebook_with_key(conn, id, notebook_id, None)
    }

    pub fn move_to_notebook_with_key(
        conn: &Connection,
        id: &str,
        notebook_id: Option<&str>,
        dek: Option<&SecretBytes>,
    ) -> Result<Note, StorageError> {
        let existing = Self::get_by_id_with_key(conn, id, dek)?
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
        Self::set_favorite_with_key(conn, id, is_favorite, None)
    }

    pub fn set_favorite_with_key(
        conn: &Connection,
        id: &str,
        is_favorite: bool,
        dek: Option<&SecretBytes>,
    ) -> Result<Note, StorageError> {
        Self::update_with_key(
            conn,
            id,
            UpdateNoteDto {
                is_favorite: Some(is_favorite),
                ..Default::default()
            },
            dek,
        )
    }

    pub fn toggle_favorite(conn: &Connection, id: &str) -> Result<Note, StorageError> {
        Self::toggle_favorite_with_key(conn, id, None)
    }

    pub fn toggle_favorite_with_key(
        conn: &Connection,
        id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Note, StorageError> {
        let note = Self::get_by_id_with_key(conn, id, dek)?
            .ok_or_else(|| StorageError::NotFound(format!("Note with id '{id}' not found")))?;
        Self::set_favorite_with_key(conn, id, !note.is_favorite, dek)
    }

    pub fn set_pinned(conn: &Connection, id: &str, is_pinned: bool) -> Result<Note, StorageError> {
        Self::set_pinned_with_key(conn, id, is_pinned, None)
    }

    pub fn set_pinned_with_key(
        conn: &Connection,
        id: &str,
        is_pinned: bool,
        dek: Option<&SecretBytes>,
    ) -> Result<Note, StorageError> {
        Self::update_with_key(
            conn,
            id,
            UpdateNoteDto {
                is_pinned: Some(is_pinned),
                ..Default::default()
            },
            dek,
        )
    }

    pub fn toggle_pinned(conn: &Connection, id: &str) -> Result<Note, StorageError> {
        Self::toggle_pinned_with_key(conn, id, None)
    }

    pub fn toggle_pinned_with_key(
        conn: &Connection,
        id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Note, StorageError> {
        let note = Self::get_by_id_with_key(conn, id, dek)?
            .ok_or_else(|| StorageError::NotFound(format!("Note with id '{id}' not found")))?;
        Self::set_pinned_with_key(conn, id, !note.is_pinned, dek)
    }

    pub fn list_favorites(conn: &Connection) -> Result<Vec<Note>, StorageError> {
        Self::list_favorites_with_key(conn, None)
    }

    pub fn list_favorites_with_key(
        conn: &Connection,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<Note>, StorageError> {
        Self::list_favorites_with_options(conn, None, dek)
    }

    pub fn list_favorites_with_options(
        conn: &Connection,
        content_limit: Option<usize>,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<Note>, StorageError> {
        Self::list_favorites_page_with_options(conn, None, None, content_limit, dek)
    }

    pub fn list_favorites_page_with_options(
        conn: &Connection,
        limit: Option<usize>,
        offset: Option<usize>,
        content_limit: Option<usize>,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<Note>, StorageError> {
        let mut sql = String::from(
            "SELECT id, title, content, format, notebook_id, created_at, modified_at,
                    is_favorite, is_pinned, is_deleted, deleted_at
             FROM notes
             WHERE is_deleted = 0 AND is_favorite = 1
             ORDER BY is_pinned DESC, modified_at DESC, id ASC",
        );
        if limit.is_some() {
            sql.push_str(" LIMIT ?1 OFFSET ?2");
        }
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = if let Some(limit) = limit {
            stmt.query(params![limit as i64, offset.unwrap_or(0) as i64])?
        } else {
            stmt.query([])?
        };
        let mut notes = Vec::new();
        while let Some(row) = rows.next()? {
            let mut note = Self::map_row_with_key(row, dek)?;
            if let Some(max_len) = content_limit {
                if note.content.chars().count() > max_len {
                    note.content = note.content.chars().take(max_len).collect();
                }
            }
            notes.push(note);
        }
        Ok(notes)
    }

    pub fn list_by_tag(conn: &Connection, tag_id: &str) -> Result<Vec<Note>, StorageError> {
        Self::list_by_tag_with_key(conn, tag_id, None)
    }

    pub fn list_by_tag_with_key(
        conn: &Connection,
        tag_id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<Note>, StorageError> {
        Self::list_by_tag_with_options(conn, tag_id, None, dek)
    }

    pub fn list_by_tag_with_options(
        conn: &Connection,
        tag_id: &str,
        content_limit: Option<usize>,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<Note>, StorageError> {
        Self::list_by_tag_page_with_options(conn, tag_id, None, None, content_limit, dek)
    }

    pub fn list_by_tag_page_with_options(
        conn: &Connection,
        tag_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
        content_limit: Option<usize>,
        dek: Option<&SecretBytes>,
    ) -> Result<Vec<Note>, StorageError> {
        let mut sql = String::from(
            "SELECT n.id, n.title, n.content, n.format, n.notebook_id, n.created_at, n.modified_at,
                    n.is_favorite, n.is_pinned, n.is_deleted, n.deleted_at
             FROM notes n
             JOIN note_tags nt ON nt.note_id = n.id
             WHERE nt.tag_id = ?1
               AND n.is_deleted = 0
             ORDER BY n.is_pinned DESC, n.modified_at DESC, n.id ASC",
        );
        if limit.is_some() {
            sql.push_str(" LIMIT ?2 OFFSET ?3");
        }

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = if let Some(limit) = limit {
            stmt.query(params![tag_id, limit as i64, offset.unwrap_or(0) as i64])?
        } else {
            stmt.query(params![tag_id])?
        };
        let mut notes = Vec::new();
        while let Some(row) = rows.next()? {
            let mut note = Self::map_row_with_key(row, dek)?;
            if let Some(max_len) = content_limit {
                if note.content.chars().count() > max_len {
                    note.content = note.content.chars().take(max_len).collect();
                }
            }
            notes.push(note);
        }
        Ok(notes)
    }

    pub fn get_metadata(
        conn: &Connection,
        id: &str,
    ) -> Result<Option<crate::storage::models::NoteMetadata>, StorageError> {
        Self::get_metadata_with_key(conn, id, None)
    }

    pub fn get_metadata_with_key(
        conn: &Connection,
        id: &str,
        dek: Option<&SecretBytes>,
    ) -> Result<Option<crate::storage::models::NoteMetadata>, StorageError> {
        let note = match Self::get_by_id_with_key(conn, id, dek)? {
            Some(n) => n,
            None => return Ok(None),
        };
        let tags = crate::storage::repositories::tags::TagRepository::get_tags_for_note(conn, id)?;
        let mut metadata = crate::storage::models::NoteMetadata::from_note_and_tags(&note, tags);
        if let Some(ref nb_id) = note.notebook_id {
            let path =
                crate::storage::repositories::notebooks::NotebookRepository::get_hierarchy_path(
                    conn, nb_id,
                )?;
            metadata.notebook_path = Some(path);
        }
        Ok(Some(metadata))
    }

    /// Returns the human-readable notebook path for a note (or "Unfiled" if unassigned).
    pub fn get_notebook_path(conn: &Connection, note_id: &str) -> Result<String, StorageError> {
        let mut stmt = conn.prepare("SELECT notebook_id FROM notes WHERE id = ?1")?;
        let nb_id_opt: Option<String> =
            stmt.query_row(params![note_id], |r| r.get(0))
                .map_err(|e| {
                    if e == rusqlite::Error::QueryReturnedNoRows {
                        StorageError::NotFound(format!("Note with id '{note_id}' not found"))
                    } else {
                        StorageError::Database(e)
                    }
                })?;
        match nb_id_opt {
            Some(ref nb_id) => {
                crate::storage::repositories::notebooks::NotebookRepository::get_hierarchy_path(
                    conn, nb_id,
                )
            }
            None => Ok("Unfiled".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::attachment_storage::AttachmentStorage;
    use crate::storage::crypto::{generate_dek, ENCRYPTED_PREFIX_V1};
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::{CreateAttachmentDto, CreateNotebookDto};
    use crate::storage::paths::StoragePaths;
    use crate::storage::repositories::attachments::AttachmentRepository;
    use crate::storage::repositories::notebooks::NotebookRepository;
    use crate::storage::repositories::tags::TagRepository;
    use crate::storage::services::TrashService;

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
        assert_eq!(
            all_notes.len(),
            0,
            "Failed transaction must roll back and leave 0 notes"
        );

        // 2. Transaction success: Valid tags
        let success_res = NoteRepository::create_with_tags(
            &mut conn,
            CreateNoteDto {
                title: Some("Atomic Success".to_string()),
                content: Some("Both note and tag must be persisted".to_string()),
                ..Default::default()
            },
            std::slice::from_ref(&tag.id),
        )
        .expect("Valid transaction should succeed");

        let fetched_notes = NoteRepository::list(&conn, true).expect("List notes failed");
        assert_eq!(fetched_notes.len(), 1);
        assert_eq!(fetched_notes[0].id, success_res.id);

        let note_tags =
            TagRepository::get_tags_for_note(&conn, &success_res.id).expect("Get tags failed");
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
        )
        .expect("create note 1");
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
        let note1_reloaded = NoteRepository::get_by_id(&conn2, &note1.id)
            .unwrap()
            .unwrap();
        assert!(
            note1_reloaded.is_favorite,
            "3. favorite must persist across connection reload"
        );

        // 4. Favorite filtering
        let note2 = NoteRepository::create(
            &conn2,
            CreateNoteDto {
                title: Some("Standard Note 2".to_string()),
                content: Some("Second note content (not favorite)".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 2");
        assert!(!note2.is_favorite);

        let note3 = NoteRepository::create(
            &conn2,
            CreateNoteDto {
                title: Some("Favorite Note 3".to_string()),
                content: Some("Third note content (favorite)".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 3");
        NoteRepository::toggle_favorite(&conn2, &note3.id).expect("favorite note 3");

        let favorites = NoteRepository::list_favorites(&conn2).expect("4. list favorites");
        assert_eq!(favorites.len(), 2);
        assert!(favorites.iter().any(|n| n.id == note1.id));
        assert!(favorites.iter().any(|n| n.id == note3.id));
        assert!(!favorites.iter().any(|n| n.id == note2.id));

        // 5. Deleted note does not appear in Favorites
        NoteRepository::delete(&conn2, &note1.id, true).expect("soft delete note 1");
        let favorites_after_delete =
            NoteRepository::list_favorites(&conn2).expect("5. list favorites after delete");
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
        let empty_notes =
            NoteRepository::list_by_tag(&conn, &empty_tag.id).expect("1. list empty tag notes");
        assert_eq!(
            empty_notes.len(),
            0,
            "Tag with no notes must return empty vector"
        );

        // 2. Tag with one note
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note 1".to_string()),
                content: Some("Content 1".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 1");

        let solo_tag = TagRepository::create(&conn, "solo-tag").expect("create solo tag");
        TagRepository::add_tag_to_note(&conn, &note1.id, &solo_tag.id).expect("assign solo tag");

        let solo_notes =
            NoteRepository::list_by_tag(&conn, &solo_tag.id).expect("2. list solo tag notes");
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
        )
        .expect("create note 2");

        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note 3".to_string()),
                content: Some("Content 3".to_string()),
                ..Default::default()
            },
        )
        .expect("create note 3");

        let shared_tag = TagRepository::create(&conn, "shared-tag").expect("create shared tag");
        TagRepository::add_tag_to_note(&conn, &note1.id, &shared_tag.id).expect("assign note 1");
        TagRepository::add_tag_to_note(&conn, &note2.id, &shared_tag.id).expect("assign note 2");
        TagRepository::add_tag_to_note(&conn, &note3.id, &shared_tag.id).expect("assign note 3");

        let shared_notes =
            NoteRepository::list_by_tag(&conn, &shared_tag.id).expect("3. list shared tag notes");
        assert_eq!(shared_notes.len(), 3);

        // 4. Deleted note excluded
        NoteRepository::delete(&conn, &note2.id, true).expect("soft delete note 2");
        let notes_after_soft_delete =
            NoteRepository::list_by_tag(&conn, &shared_tag.id).expect("4. list after soft delete");
        assert_eq!(
            notes_after_soft_delete.len(),
            2,
            "Deleted note must be excluded from tag filter"
        );
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
        )
        .expect("pin note 3");
        let ordered_notes =
            NoteRepository::list_by_tag(&conn, &shared_tag.id).expect("5. list ordered notes");
        assert_eq!(ordered_notes.len(), 2);
        assert_eq!(
            ordered_notes[0].id, note3.id,
            "Pinned note must be ordered first"
        );
        assert_eq!(ordered_notes[1].id, note1.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_centralized_active_and_trashed_query_rules() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_query_rules_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("init failed");
        run_migrations(&mut conn).expect("migrations failed");

        // 1. Initial empty states: empty list is NOT an error
        assert_eq!(NoteRepository::list_active(&conn).unwrap().len(), 0);
        assert_eq!(NoteRepository::list_trashed(&conn).unwrap().len(), 0);
        assert_eq!(NoteRepository::count_active(&conn).unwrap(), 0);
        assert_eq!(NoteRepository::count_trashed(&conn).unwrap(), 0);

        // 2. Create active notes
        let active1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Active 1".to_string()),
                content: Some("First active note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let active2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Active 2".to_string()),
                content: Some("Second active note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // 3. Move active2 to Trash via soft delete
        NoteRepository::delete(&conn, &active2.id, true).unwrap();

        // 4. Verify count separation
        assert_eq!(NoteRepository::count_active(&conn).unwrap(), 1);
        assert_eq!(NoteRepository::count_trashed(&conn).unwrap(), 1);

        // 5. Verify single-note queries respect lifecycle state
        assert!(NoteRepository::get_active_by_id(&conn, &active1.id)
            .unwrap()
            .is_some());
        assert!(NoteRepository::get_trashed_by_id(&conn, &active1.id)
            .unwrap()
            .is_none());

        assert!(NoteRepository::get_active_by_id(&conn, &active2.id)
            .unwrap()
            .is_none());
        assert!(NoteRepository::get_trashed_by_id(&conn, &active2.id)
            .unwrap()
            .is_some());

        // get_by_id returns both for administrative/lifecycle inspection
        assert!(NoteRepository::get_by_id(&conn, &active1.id)
            .unwrap()
            .is_some());
        assert!(NoteRepository::get_by_id(&conn, &active2.id)
            .unwrap()
            .is_some());

        // Non-existent note returns None cleanly
        assert!(NoteRepository::get_active_by_id(&conn, "non-existent")
            .unwrap()
            .is_none());
        assert!(NoteRepository::get_trashed_by_id(&conn, "non-existent")
            .unwrap()
            .is_none());

        // 6. Verify list_active and list_trashed separation
        let active_list = NoteRepository::list_active(&conn).unwrap();
        assert_eq!(active_list.len(), 1);
        assert_eq!(active_list[0].id, active1.id);

        let trashed_list = NoteRepository::list_trashed(&conn).unwrap();
        assert_eq!(trashed_list.len(), 1);
        assert_eq!(trashed_list[0].id, active2.id);
        assert!(trashed_list[0].is_deleted);
        assert!(trashed_list[0].deleted_at.is_some());

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_list_trashed_items_dto_mapping_and_ordering() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_trashed_items_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. When empty, list_trashed_items returns an empty vector
        let empty_list = NoteRepository::list_trashed_items(&conn).unwrap();
        assert!(empty_list.is_empty());

        // 2. Setup: 2 notebooks
        let nb_alpha = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Alpha Notebook".to_string(),
                parent_id: None,
            },
        )
        .unwrap();
        let nb_beta = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Beta Notebook".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

        // 3. Active Note (must be excluded from trashed items)
        let _active_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Active Note".to_string()),
                content: Some("Active content that must not appear in trash".to_string()),
                notebook_id: Some(nb_alpha.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        // 4. Trashed Note 1 (in Alpha, with long content > 200 chars, favorite, attachment)
        let long_content = "A".repeat(350);
        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Long Trashed Note 1".to_string()),
                content: Some(long_content.clone()),
                notebook_id: Some(nb_alpha.id.clone()),
                is_favorite: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        // Soft delete note1 with an earlier deleted timestamp
        let time_early = "2026-10-01T10:00:00Z";
        conn.execute(
            "UPDATE notes SET is_deleted = 1, deleted_at = ?1 WHERE id = ?2",
            params![time_early, note1.id],
        )
        .unwrap();

        // Add 2 attachments to note1
        AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note1.id.clone(),
                file_name: "file1.pdf".to_string(),
                relative_path: "note1/file1.pdf".to_string(),
                mime_type: Some("application/pdf".to_string()),
                file_size: 1024,
            },
        )
        .unwrap();
        AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note1.id.clone(),
                file_name: "file2.png".to_string(),
                relative_path: "note1/file2.png".to_string(),
                mime_type: Some("image/png".to_string()),
                file_size: 2048,
            },
        )
        .unwrap();

        // 5. Trashed Note 2 (in Beta, deleted more recently, no attachments)
        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Recent Trashed Note 2".to_string()),
                content: Some("Short preview content".to_string()),
                notebook_id: Some(nb_beta.id.clone()),
                is_favorite: Some(false),
                ..Default::default()
            },
        )
        .unwrap();

        let time_recent = "2026-10-08T12:00:00Z";
        conn.execute(
            "UPDATE notes SET is_deleted = 1, deleted_at = ?1 WHERE id = ?2",
            params![time_recent, note2.id],
        )
        .unwrap();

        // 6. Query list_trashed_items
        let items = NoteRepository::list_trashed_items(&conn).unwrap();
        assert_eq!(
            items.len(),
            2,
            "Must only return the 2 trashed notes, active note excluded"
        );

        // Verify ordering: most recently deleted first (note2, then note1)
        assert_eq!(items[0].id, note2.id);
        assert_eq!(items[0].title, "Recent Trashed Note 2");
        assert_eq!(items[0].preview, "Short preview content");
        assert_eq!(items[0].notebook_id, Some(nb_beta.id.clone()));
        assert_eq!(items[0].notebook_name, Some("Beta Notebook".to_string()));
        assert_eq!(items[0].attachment_count, 0);
        assert!(!items[0].is_favorite);
        assert_eq!(items[0].deleted_at, Some(time_recent.to_string()));

        assert_eq!(items[1].id, note1.id);
        assert_eq!(items[1].title, "Long Trashed Note 1");
        // Verify preview truncation to max 200 characters without loading full 350-char body
        assert_eq!(items[1].preview.len(), 200);
        assert_eq!(items[1].preview, "A".repeat(200));
        assert_eq!(items[1].notebook_id, Some(nb_alpha.id.clone()));
        assert_eq!(items[1].notebook_name, Some("Alpha Notebook".to_string()));
        assert_eq!(items[1].attachment_count, 2);
        assert!(items[1].is_favorite);
        assert_eq!(items[1].deleted_at, Some(time_early.to_string()));

        // 7. Verify notebook deletion resilience:
        // Delete Beta notebook: note2's notebook_name should safely resolve to None
        NotebookRepository::delete(&mut conn, &nb_beta.id).unwrap();
        let items_after_nb_del = NoteRepository::list_trashed_items(&conn).unwrap();
        assert_eq!(items_after_nb_del.len(), 2);
        assert_eq!(items_after_nb_del[0].id, note2.id);
        assert_eq!(items_after_nb_del[0].notebook_name, None);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_encrypted_note_storage_roundtrip_and_db_inspection() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_enc_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let dek = generate_dek();
        let plain_title = "Secret Vault Access Codes";
        let plain_content = "Master password is: SuperSecretToken1234!";

        // 1. Create encrypted note
        let created = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some(plain_title.to_string()),
                content: Some(plain_content.to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .expect("Creation with key failed");

        assert_eq!(created.title, plain_title);
        assert_eq!(created.content, plain_content);

        // 2. Direct SQLite inspection: Raw database must NOT contain plaintext strings!
        let (raw_title, raw_content): (String, String) = conn
            .query_row(
                "SELECT title, content FROM notes WHERE id = ?1",
                params![created.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("Raw query failed");

        assert!(
            raw_title.starts_with(ENCRYPTED_PREFIX_V1),
            "Raw title in SQLite must start with ENC$v1$"
        );
        assert!(
            raw_content.starts_with(ENCRYPTED_PREFIX_V1),
            "Raw content in SQLite must start with ENC$v1$"
        );
        assert!(
            !raw_title.contains("Vault"),
            "Raw SQLite title leaked plaintext!"
        );
        assert!(
            !raw_content.contains("SuperSecretToken1234"),
            "Raw SQLite content leaked plaintext!"
        );

        // 3. Read back with active key
        let fetched = NoteRepository::get_by_id_with_key(&conn, &created.id, Some(&dek))
            .expect("Read with key failed")
            .expect("Note not found");

        assert_eq!(fetched.title, plain_title);
        assert_eq!(fetched.content, plain_content);

        // 4. Update encrypted note
        let updated_title = "Updated Secret Vault";
        let updated_content = "New secret content 987654";
        let updated = NoteRepository::update_with_key(
            &conn,
            &created.id,
            UpdateNoteDto {
                title: Some(updated_title.to_string()),
                content: Some(updated_content.to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .expect("Update with key failed");

        assert_eq!(updated.title, updated_title);
        assert_eq!(updated.content, updated_content);

        // Raw DB inspection after update
        let (raw_title2, raw_content2): (String, String) = conn
            .query_row(
                "SELECT title, content FROM notes WHERE id = ?1",
                params![created.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("Raw query failed");

        assert!(raw_title2.starts_with(ENCRYPTED_PREFIX_V1));
        assert!(raw_content2.starts_with(ENCRYPTED_PREFIX_V1));
        assert!(!raw_title2.contains("Updated Secret"));
        assert!(!raw_content2.contains("987654"));

        // Read again with active key
        let fetched2 = NoteRepository::get_by_id_with_key(&conn, &created.id, Some(&dek))
            .expect("Read with key failed")
            .expect("Note not found");
        assert_eq!(fetched2.title, updated_title);
        assert_eq!(fetched2.content, updated_content);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_encrypted_note_rejection_when_locked() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_lock_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let dek = generate_dek();

        let created = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Confidential Note".to_string()),
                content: Some("Classified Information".to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .expect("Creation failed");

        // 1. Calling get_by_id_with_key without DEK (locked state) must return StorageError::Crypto
        let err = NoteRepository::get_by_id_with_key(&conn, &created.id, None)
            .expect_err("Must reject reading encrypted note without DEK");
        assert!(matches!(err, StorageError::Crypto(_)));
        assert!(err.to_string().contains("locked"));

        // 2. Calling list_filtered_with_key without DEK must also fail with StorageError::Crypto
        let list_err = NoteRepository::list_filtered_with_key(&conn, false, None, false, None)
            .expect_err("Must reject listing encrypted notes without DEK");
        assert!(matches!(list_err, StorageError::Crypto(_)));

        // 3. Trashing note without DEK: soft_delete_with_key must fail because it reads existing note first
        let soft_err = NoteRepository::soft_delete_with_key(&conn, &created.id, None)
            .expect_err("Must reject soft deleting encrypted note without DEK");
        assert!(matches!(soft_err, StorageError::Crypto(_)));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ciphertext_tamper_detection() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_tamper_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let dek = generate_dek();

        let created = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Authentic Title".to_string()),
                content: Some("Authentic Content".to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .expect("Creation failed");

        // Tamper with content in SQLite by modifying a character in the ciphertext
        let raw_content: String = conn
            .query_row(
                "SELECT content FROM notes WHERE id = ?1",
                params![created.id],
                |r| r.get(0),
            )
            .unwrap();

        // Mutate a byte in base64 envelope
        let mut bytes = raw_content.into_bytes();
        let last_idx = bytes.len() - 2;
        bytes[last_idx] = if bytes[last_idx] == b'A' { b'B' } else { b'A' };
        let tampered_content = String::from_utf8(bytes).unwrap();

        conn.execute(
            "UPDATE notes SET content = ?1 WHERE id = ?2",
            params![tampered_content, created.id],
        )
        .unwrap();

        // Attempt to decrypt tampered note
        let err = NoteRepository::get_by_id_with_key(&conn, &created.id, Some(&dek))
            .expect_err("Decryption of tampered ciphertext must fail tag authentication");
        assert!(matches!(err, StorageError::Crypto(_)));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_aad_splicing_defense() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_aad_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let dek = generate_dek();

        // Note A
        let note_a = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Note A Title".to_string()),
                content: Some("Note A Sensitive Body".to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .unwrap();

        // Note B
        let note_b = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Note B Title".to_string()),
                content: Some("Note B Ordinary Body".to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .unwrap();

        // Attacker attempts ciphertext splicing: copy Note A's content ciphertext into Note B's content row
        let raw_content_a: String = conn
            .query_row(
                "SELECT content FROM notes WHERE id = ?1",
                params![note_a.id],
                |r| r.get(0),
            )
            .unwrap();

        conn.execute(
            "UPDATE notes SET content = ?1 WHERE id = ?2",
            params![raw_content_a, note_b.id],
        )
        .unwrap();

        // Loading Note B with valid DEK must fail because AAD was notes:<note_a_id>:content
        let err = NoteRepository::get_by_id_with_key(&conn, &note_b.id, Some(&dek))
            .expect_err("Spliced ciphertext from Note A must fail AEAD verification on Note B");
        assert!(matches!(err, StorageError::Crypto(_)));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mixed_plaintext_and_encrypted_notes_backward_compatibility() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_mixed_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let dek = generate_dek();

        // 1. Plaintext legacy note (created without DEK)
        let note_legacy = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Plaintext Legacy Note".to_string()),
                content: Some("Plaintext Body".to_string()),
                ..Default::default()
            },
            None,
        )
        .unwrap();

        // 2. Encrypted note (created with DEK)
        let note_encrypted = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Encrypted Modern Note".to_string()),
                content: Some("Encrypted Body".to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .unwrap();

        // Verify SQLite raw storage
        let raw_legacy_title: String = conn
            .query_row(
                "SELECT title FROM notes WHERE id = ?1",
                params![note_legacy.id],
                |r| r.get(0),
            )
            .unwrap();
        let raw_enc_title: String = conn
            .query_row(
                "SELECT title FROM notes WHERE id = ?1",
                params![note_encrypted.id],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(raw_legacy_title, "Plaintext Legacy Note");
        assert!(raw_enc_title.starts_with(ENCRYPTED_PREFIX_V1));

        // Reading legacy note without DEK works
        let fetched_legacy = NoteRepository::get_by_id_with_key(&conn, &note_legacy.id, None)
            .unwrap()
            .unwrap();
        assert_eq!(fetched_legacy.title, "Plaintext Legacy Note");

        // Reading both with DEK active
        let all_notes =
            NoteRepository::list_filtered_with_key(&conn, false, None, false, Some(&dek)).unwrap();
        assert_eq!(all_notes.len(), 2);

        let titles: Vec<String> = all_notes.into_iter().map(|n| n.title).collect();
        assert!(titles.contains(&"Plaintext Legacy Note".to_string()));
        assert!(titles.contains(&"Encrypted Modern Note".to_string()));

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_trashed_and_permanent_deletion_resilience_without_dek() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_trash_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let dek = generate_dek();

        // 1. Create encrypted note
        let note = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Trashable Encrypted Note".to_string()),
                content: Some("To be deleted permanently".to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .unwrap();

        // 2. Soft delete while unlocked
        let trashed = TrashService::soft_delete_with_key(&conn, &note.id, Some(&dek)).unwrap();
        assert!(trashed.is_deleted);

        // 3. Permanently delete WITHOUT DEK (e.g., user empties trash or purges without decrypting body)
        let storage_paths = StoragePaths::from_root(temp_dir.clone());
        let storage = AttachmentStorage::new(storage_paths);

        let perm_res = TrashService::permanently_delete_note(&conn, &storage, &note.id).unwrap();
        assert!(perm_res.success);

        // Verify note row is completely removed from SQLite
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM notes WHERE id = ?1",
                params![note.id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(
            !exists,
            "Permanently deleted encrypted note must be removed from notes table"
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
