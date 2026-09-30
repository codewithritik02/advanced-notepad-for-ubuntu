use rusqlite::{params, Connection, Row};
use crate::storage::errors::StorageError;
use crate::storage::models::{Tag, UpdateTagDto};

pub struct TagRepository;

impl TagRepository {
    fn map_row(row: &Row) -> rusqlite::Result<Tag> {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
        })
    }

    pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<Tag>, StorageError> {
        let mut stmt = conn.prepare("SELECT id, name, created_at FROM tags WHERE id = ?1")?;
        let mut iter = stmt.query_map(params![id], Self::map_row)?;
        if let Some(item) = iter.next() {
            Ok(Some(item?))
        } else {
            Ok(None)
        }
    }

    pub fn get_by_name(conn: &Connection, name: &str) -> Result<Option<Tag>, StorageError> {
        let trimmed = name.trim();
        let mut stmt = conn.prepare("SELECT id, name, created_at FROM tags WHERE LOWER(name) = LOWER(?1)")?;
        let mut iter = stmt.query_map(params![trimmed], Self::map_row)?;
        if let Some(item) = iter.next() {
            Ok(Some(item?))
        } else {
            Ok(None)
        }
    }

    pub const MAX_TAG_NAME_LENGTH: usize = 100;

    pub fn validate_tag_name(name: &str) -> Result<String, StorageError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(StorageError::Validation("Tag name cannot be empty".to_string()));
        }
        if trimmed.chars().count() > Self::MAX_TAG_NAME_LENGTH {
            return Err(StorageError::Validation(format!(
                "Tag name cannot exceed {} characters",
                Self::MAX_TAG_NAME_LENGTH
            )));
        }
        Ok(trimmed.to_string())
    }

    pub fn create(conn: &Connection, name: &str) -> Result<Tag, StorageError> {
        let name = Self::validate_tag_name(name)?;

        // If tag already exists (case-insensitive check), return existing tag
        if let Some(existing) = Self::get_by_name(conn, &name)? {
            return Ok(existing);
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, name, now],
        )?;

        Ok(Tag {
            id,
            name,
            created_at: now,
        })
    }

    pub fn update(conn: &Connection, id: &str, dto: UpdateTagDto) -> Result<Tag, StorageError> {
        let name = Self::validate_tag_name(&dto.name)?;

        // Check if another tag already has this name (case-insensitive)
        let mut stmt = conn.prepare(
            "SELECT id, name, created_at FROM tags WHERE LOWER(name) = LOWER(?1) AND id != ?2",
        )?;
        let mut iter = stmt.query_map(params![name, id], Self::map_row)?;
        if iter.next().is_some() {
            return Err(StorageError::Conflict(format!(
                "A tag with the name '{}' already exists",
                name
            )));
        }

        let rows_affected = conn.execute(
            "UPDATE tags SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;

        if rows_affected == 0 {
            return Err(StorageError::NotFound(format!("Tag with id '{}' not found", id)));
        }

        Self::get_by_id(conn, id)?
            .ok_or_else(|| StorageError::NotFound(format!("Tag with id '{}' not found", id)))
    }

    pub fn delete(conn: &Connection, id: &str) -> Result<bool, StorageError> {
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM note_tags WHERE tag_id = ?1", params![id])?;
        let rows_affected = tx.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(rows_affected > 0)
    }

    pub fn list(conn: &Connection) -> Result<Vec<Tag>, StorageError> {
        let mut stmt = conn.prepare("SELECT id, name, created_at FROM tags ORDER BY name ASC")?;
        let iter = stmt.query_map([], Self::map_row)?;

        let mut tags = Vec::new();
        for item in iter {
            tags.push(item?);
        }
        Ok(tags)
    }

    pub fn add_tag_to_note(conn: &Connection, note_id: &str, tag_id: &str) -> Result<(), StorageError> {
        conn.execute(
            "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            params![note_id, tag_id],
        )?;
        Ok(())
    }

    pub fn remove_tag_from_note(conn: &Connection, note_id: &str, tag_id: &str) -> Result<(), StorageError> {
        conn.execute(
            "DELETE FROM note_tags WHERE note_id = ?1 AND tag_id = ?2",
            params![note_id, tag_id],
        )?;
        Ok(())
    }

    pub fn get_tags_for_note(conn: &Connection, note_id: &str) -> Result<Vec<Tag>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.created_at
             FROM tags t
             JOIN note_tags nt ON nt.tag_id = t.id
             WHERE nt.note_id = ?1
             ORDER BY t.name ASC",
        )?;

        let iter = stmt.query_map(params![note_id], Self::map_row)?;
        let mut tags = Vec::new();
        for item in iter {
            tags.push(item?);
        }
        Ok(tags)
    }

    pub fn set_tags_for_note(
        conn: &mut Connection,
        note_id: &str,
        tag_ids: &[String],
    ) -> Result<Vec<Tag>, StorageError> {
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM note_tags WHERE note_id = ?1", params![note_id])?;
        for tag_id in tag_ids {
            tx.execute(
                "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
                params![note_id, tag_id],
            )?;
        }
        tx.commit()?;
        Self::get_tags_for_note(conn, note_id)
    }

    pub fn get_tag_note_counts(
        conn: &Connection,
    ) -> Result<std::collections::HashMap<String, usize>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT nt.tag_id, COUNT(n.id)
             FROM note_tags nt
             JOIN notes n ON n.id = nt.note_id
             WHERE n.is_deleted = 0
             GROUP BY nt.tag_id",
        )?;
        let mut map = std::collections::HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;
        for r in rows {
            let (tag_id, count) = r?;
            map.insert(tag_id, count);
        }
        Ok(map)
    }

    pub fn get_all_notes_tag_names(
        conn: &Connection,
    ) -> Result<std::collections::HashMap<String, Vec<String>>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT nt.note_id, t.name
             FROM note_tags nt
             JOIN tags t ON t.id = nt.tag_id
             JOIN notes n ON n.id = nt.note_id
             WHERE n.is_deleted = 0
             ORDER BY t.name ASC",
        )?;

        let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for r in rows {
            let (note_id, tag_name) = r?;
            map.entry(note_id).or_default().push(tag_name);
        }

        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::CreateNoteDto;
    use crate::storage::repositories::notes::NoteRepository;

    #[test]
    fn test_tags_and_note_tags_relationship() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_tags_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Tag creation with Unicode name
        let tag1 = TagRepository::create(&conn, "महत्वपूर्ण 📌").expect("Create tag failed");
        let tag2 = TagRepository::create(&conn, "personal").expect("Create tag 2 failed");

        assert_eq!(tag1.name, "महत्वपूर्ण 📌");

        // 2. Duplicate tag resolution (case-insensitive) returns existing tag and preserves casing
        let dup = TagRepository::create(&conn, "Personal").expect("Duplicate tag creation should return existing tag");
        assert_eq!(dup.id, tag2.id);
        assert_eq!(dup.name, "personal");

        // 3. Create note and link tags
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Tagged Note".to_string()),
                content: Some("Testing note-tag relationship".to_string()),
                ..Default::default()
            },
        ).expect("Create note failed");

        TagRepository::add_tag_to_note(&conn, &note.id, &tag1.id).expect("Link tag 1 failed");
        TagRepository::add_tag_to_note(&conn, &note.id, &tag2.id).expect("Link tag 2 failed");

        // 4. Retrieve tags for note
        let note_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get tags failed");
        assert_eq!(note_tags.len(), 2);

        // 5. Remove one tag association
        TagRepository::remove_tag_from_note(&conn, &note.id, &tag2.id).expect("Remove tag failed");
        let note_tags_after = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get tags failed");
        assert_eq!(note_tags_after.len(), 1);
        assert_eq!(note_tags_after[0].id, tag1.id);

        // 6. Cascade delete: deleting note automatically deletes note_tags
        NoteRepository::delete(&conn, &note.id, false).expect("Hard delete note failed");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM note_tags WHERE note_id = ?1",
                [&note.id],
                |r| r.get(0),
            )
            .expect("Count query failed");
        assert_eq!(count, 0, "Deleting note must cascade and remove note_tags relations");

        // 7. Get tag by ID
        let fetched = TagRepository::get_by_id(&conn, &tag1.id).expect("Get tag failed");
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().name, "महत्वपूर्ण 📌");

        // 8. Update (rename) tag
        let updated = TagRepository::update(&conn, &tag1.id, UpdateTagDto { name: "महत्वपूर्ण (अपडेटेड)".to_string() })
            .expect("Update tag failed");
        assert_eq!(updated.name, "महत्वपूर्ण (अपडेटेड)");

        // 9. Delete tag and ensure cascade from note_tags
        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note 2".to_string()),
                ..Default::default()
            },
        ).expect("Create note 2 failed");
        TagRepository::add_tag_to_note(&conn, &note2.id, &tag2.id).expect("Link tag 2 failed");

        let deleted = TagRepository::delete(&conn, &tag2.id).expect("Delete tag failed");
        assert!(deleted);
        let tag2_check = TagRepository::get_by_id(&conn, &tag2.id).expect("Get tag 2 failed");
        assert!(tag2_check.is_none());

        // Note 2 still exists
        let note2_check = NoteRepository::get_by_id(&conn, &note2.id).expect("Get note 2 failed");
        assert!(note2_check.is_some(), "Deleting a tag must not delete notes");

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_tag_validation() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_tag_val_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Rejection of empty and whitespace-only tag names
        assert!(TagRepository::create(&conn, "").is_err());
        assert!(TagRepository::create(&conn, "   \t\n  ").is_err());

        // 2. Trimming leading and trailing whitespace
        let tag = TagRepository::create(&conn, "   work   ").expect("Trimmed tag should succeed");
        assert_eq!(tag.name, "work");

        // 3. Length limit validation (MAX_TAG_NAME_LENGTH = 100)
        let exact_100: String = (0..100).map(|_| 'a').collect();
        let tag_100 = TagRepository::create(&conn, &exact_100).expect("100 chars should succeed");
        assert_eq!(tag_100.name.chars().count(), 100);

        let over_100: String = (0..101).map(|_| 'a').collect();
        assert!(TagRepository::create(&conn, &over_100).is_err());

        // 4. Unicode support with emojis and international scripts within length limit
        let unicode_tag = "🚀 प्रोजेक्ट 2026";
        let tag_unicode = TagRepository::create(&conn, unicode_tag).expect("Unicode tag should succeed");
        assert_eq!(tag_unicode.name, unicode_tag);

        // 5. Validation on update
        assert!(TagRepository::update(&conn, &tag.id, UpdateTagDto { name: "".to_string() }).is_err());
        assert!(TagRepository::update(&conn, &tag.id, UpdateTagDto { name: "   ".to_string() }).is_err());
        assert!(TagRepository::update(&conn, &tag.id, UpdateTagDto { name: over_100 }).is_err());

        let updated = TagRepository::update(&conn, &tag.id, UpdateTagDto { name: "  deep-work  ".to_string() })
            .expect("Valid update with trimming should succeed");
        assert_eq!(updated.name, "deep-work");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_tag_case_insensitive_uniqueness_and_rename_conflict() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_tag_case_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Create tag "Work"
        let t1 = TagRepository::create(&conn, "Work").expect("Create Work tag failed");
        assert_eq!(t1.name, "Work");

        // 2. Creating "work" (lowercase) returns existing tag with original display case
        let t2 = TagRepository::create(&conn, "work").expect("Should reuse existing tag");
        assert_eq!(t2.id, t1.id);
        assert_eq!(t2.name, "Work");

        // 3. Creating "  WORK  " returns existing tag
        let t3 = TagRepository::create(&conn, "  WORK  ").expect("Should reuse existing tag");
        assert_eq!(t3.id, t1.id);
        assert_eq!(t3.name, "Work");

        // Total tags count must still be 1
        let all_tags = TagRepository::list(&conn).expect("List tags failed");
        assert_eq!(all_tags.len(), 1);

        // 4. Create another tag "Personal"
        let p1 = TagRepository::create(&conn, "Personal").expect("Create Personal tag failed");
        assert_eq!(p1.name, "Personal");

        // 5. Renaming "Personal" to "work" (conflicts with t1) should be rejected
        let conflict = TagRepository::update(&conn, &p1.id, UpdateTagDto { name: "work".to_string() });
        assert!(conflict.is_err(), "Renaming to existing tag name should return error");

        // 6. Renaming "Work" to "work" (changing its own casing) succeeds
        let updated_case = TagRepository::update(&conn, &t1.id, UpdateTagDto { name: "work".to_string() })
            .expect("Renaming own tag casing should succeed");
        assert_eq!(updated_case.id, t1.id);
        assert_eq!(updated_case.name, "work");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_note_tag_assignment_and_batch_set() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_note_tags_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let tag_a = TagRepository::create(&conn, "urgent").expect("Tag A");
        let tag_b = TagRepository::create(&conn, "finance").expect("Tag B");
        let tag_c = TagRepository::create(&conn, "taxes").expect("Tag C");

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Financial Report".to_string()),
                ..Default::default()
            },
        ).expect("Create note");

        // 1. Add single tags
        TagRepository::add_tag_to_note(&conn, &note.id, &tag_a.id).expect("Add tag A");
        // Idempotent: adding same tag twice does not error or duplicate
        TagRepository::add_tag_to_note(&conn, &note.id, &tag_a.id).expect("Add tag A again");

        let tags1 = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get tags");
        assert_eq!(tags1.len(), 1);
        assert_eq!(tags1[0].id, tag_a.id);

        // 2. Batch set note tags (replace tag A with tag B and tag C)
        let new_tag_ids = vec![tag_b.id.clone(), tag_c.id.clone()];
        let tags2 = TagRepository::set_tags_for_note(&mut conn, &note.id, &new_tag_ids)
            .expect("Batch set tags");
        assert_eq!(tags2.len(), 2);
        assert!(tags2.iter().any(|t| t.id == tag_b.id));
        assert!(tags2.iter().any(|t| t.id == tag_c.id));
        assert!(!tags2.iter().any(|t| t.id == tag_a.id));

        // 3. Batch clear tags (empty list)
        let empty_tags = TagRepository::set_tags_for_note(&mut conn, &note.id, &[])
            .expect("Batch clear tags");
        assert_eq!(empty_tags.len(), 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_59_backend_tag_unit_tests() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_t59_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Create tag
        let created = TagRepository::create(&conn, "development").expect("1. create tag");
        assert_eq!(created.name, "development");
        assert!(!created.id.is_empty());
        assert!(!created.created_at.is_empty());

        // 2. Get tag (by ID and by Name)
        let fetched_by_id = TagRepository::get_by_id(&conn, &created.id)
            .expect("get tag by id")
            .expect("tag must exist");
        assert_eq!(fetched_by_id.id, created.id);
        assert_eq!(fetched_by_id.name, "development");

        let fetched_by_name = TagRepository::get_by_name(&conn, "DEVELOPMENT")
            .expect("get tag by name case-insensitive")
            .expect("tag must exist");
        assert_eq!(fetched_by_name.id, created.id);

        // 3. List tags
        let _tag_b = TagRepository::create(&conn, "analytics").expect("create tag analytics");
        let list = TagRepository::list(&conn).expect("3. list tags");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "analytics"); // Alphabetical order
        assert_eq!(list[1].name, "development");

        // 4. Rename tag
        let renamed = TagRepository::update(
            &conn,
            &created.id,
            UpdateTagDto {
                name: "dev-ops".to_string(),
            },
        ).expect("4. rename tag");
        assert_eq!(renamed.id, created.id);
        assert_eq!(renamed.name, "dev-ops");

        let fetched_after_rename = TagRepository::get_by_id(&conn, &created.id).unwrap().unwrap();
        assert_eq!(fetched_after_rename.name, "dev-ops");

        // 5. Delete tag
        let deleted = TagRepository::delete(&conn, &created.id).expect("5. delete tag");
        assert!(deleted, "First delete must return true");

        let deleted_again = TagRepository::delete(&conn, &created.id).expect("delete already-deleted tag");
        assert!(!deleted_again, "Subsequent delete must return false");

        assert!(TagRepository::get_by_id(&conn, &created.id).unwrap().is_none());

        // 6. Duplicate tag handling (case-insensitive create returns existing, rename conflict errors)
        let dup_initial = TagRepository::create(&conn, "Research").expect("create Research");
        let dup_recreate = TagRepository::create(&conn, "research").expect("6. duplicate create returns existing");
        assert_eq!(dup_recreate.id, dup_initial.id);
        assert_eq!(dup_recreate.name, "Research");

        let other_tag = TagRepository::create(&conn, "Planning").expect("create Planning");
        let conflict_rename = TagRepository::update(
            &conn,
            &other_tag.id,
            UpdateTagDto {
                name: "RESEARCH".to_string(),
            },
        );
        match conflict_rename {
            Err(StorageError::Conflict(msg)) => assert!(msg.contains("already exists")),
            other => panic!("Expected Conflict, got {:?}", other),
        }

        // 7. Unicode tag (multi-byte Devanagari Hindi and emojis)
        let unicode = TagRepository::create(&conn, "यात्रा 🗺️").expect("7. Unicode tag");
        assert_eq!(unicode.name, "यात्रा 🗺️");
        let unicode_fetched = TagRepository::get_by_name(&conn, "यात्रा 🗺️").unwrap().unwrap();
        assert_eq!(unicode_fetched.id, unicode.id);

        // 8. Empty tag validation
        let empty_err = TagRepository::create(&conn, "");
        match empty_err {
            Err(StorageError::Validation(msg)) => assert_eq!(msg, "Tag name cannot be empty"),
            other => panic!("Expected ValidationError, got {:?}", other),
        }

        // 9. Whitespace tag validation
        let whitespace_err = TagRepository::create(&conn, "   \t\r\n   ");
        match whitespace_err {
            Err(StorageError::Validation(msg)) => assert_eq!(msg, "Tag name cannot be empty"),
            other => panic!("Expected ValidationError, got {:?}", other),
        }

        // 10. Long tag validation (boundary: exactly 100 succeeds, 101 fails)
        let valid_100: String = (0..100).map(|_| 'x').collect();
        let tag_100 = TagRepository::create(&conn, &valid_100).expect("100 chars succeeds");
        assert_eq!(tag_100.name.chars().count(), 100);

        let invalid_101: String = (0..101).map(|_| 'x').collect();
        let long_err = TagRepository::create(&conn, &invalid_101);
        match long_err {
            Err(StorageError::Validation(msg)) => assert!(msg.contains("cannot exceed 100 characters")),
            other => panic!("Expected ValidationError, got {:?}", other),
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_60_note_tag_relationship_tests() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_t60_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Task 60 Note".to_string()),
                content: Some("Testing note-tag relationship operations.".to_string()),
                ..Default::default()
            },
        ).expect("create note");

        let tag1 = TagRepository::create(&conn, "urgent").expect("create tag 1");
        let tag2 = TagRepository::create(&conn, "backend").expect("create tag 2");
        let tag3 = TagRepository::create(&conn, "sqlite").expect("create tag 3");
        let tag4 = TagRepository::create(&conn, "performance").expect("create tag 4");

        // 1. Assign tag
        TagRepository::add_tag_to_note(&conn, &note.id, &tag1.id).expect("1. assign tag");
        let initial_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags");
        assert_eq!(initial_tags.len(), 1);
        assert_eq!(initial_tags[0].id, tag1.id);

        // 2. Assign same tag twice (idempotent, no duplicates)
        TagRepository::add_tag_to_note(&conn, &note.id, &tag1.id).expect("2. assign same tag twice");
        let tags_after_duplicate = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags after dup");
        assert_eq!(tags_after_duplicate.len(), 1, "Duplicate assignment must not create duplicate row");

        // 3. Remove tag
        TagRepository::remove_tag_from_note(&conn, &note.id, &tag1.id).expect("3. remove tag");
        let tags_after_remove = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags after remove");
        assert_eq!(tags_after_remove.len(), 0);

        // 4. Remove missing tag (safe no-op, returns Ok)
        let unlinked_tag = TagRepository::create(&conn, "unlinked").expect("create unlinked tag");
        TagRepository::remove_tag_from_note(&conn, &note.id, &unlinked_tag.id).expect("4. remove missing tag");
        let tags_after_missing = TagRepository::get_tags_for_note(&conn, &note.id).expect("get tags after missing");
        assert_eq!(tags_after_missing.len(), 0);

        // 5. Multiple tags (assign 4 tags, verify all 4 returned in alphabetical order)
        TagRepository::add_tag_to_note(&conn, &note.id, &tag1.id).expect("add tag 1");
        TagRepository::add_tag_to_note(&conn, &note.id, &tag2.id).expect("add tag 2");
        TagRepository::add_tag_to_note(&conn, &note.id, &tag3.id).expect("add tag 3");
        TagRepository::add_tag_to_note(&conn, &note.id, &tag4.id).expect("add tag 4");

        let multi_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("5. multiple tags");
        assert_eq!(multi_tags.len(), 4);
        assert_eq!(multi_tags[0].name, "backend");
        assert_eq!(multi_tags[1].name, "performance");
        assert_eq!(multi_tags[2].name, "sqlite");
        assert_eq!(multi_tags[3].name, "urgent");

        // 6. Delete tag (delete tag 3 "sqlite")
        let deleted = TagRepository::delete(&conn, &tag3.id).expect("6. delete tag");
        assert!(deleted);

        let tags_after_tag_delete = TagRepository::get_tags_for_note(&conn, &note.id).expect("tags after delete");
        assert_eq!(tags_after_tag_delete.len(), 3);
        assert!(!tags_after_tag_delete.iter().any(|t| t.id == tag3.id));

        // 7. Note survives tag deletion
        let note_check = NoteRepository::get_by_id(&conn, &note.id).expect("query note").expect("7. note must survive");
        assert_eq!(note_check.id, note.id);
        assert_eq!(note_check.title, "Task 60 Note");
        assert_eq!(note_check.content, "Testing note-tag relationship operations.");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}


