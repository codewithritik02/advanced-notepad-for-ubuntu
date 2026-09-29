use rusqlite::{params, Connection, Row};
use crate::storage::errors::StorageError;
use crate::storage::models::Tag;

pub struct TagRepository;

impl TagRepository {
    fn map_row(row: &Row) -> rusqlite::Result<Tag> {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
        })
    }

    pub fn create(conn: &Connection, name: &str) -> Result<Tag, StorageError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::Validation("Tag name cannot be empty".to_string()));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, name, now],
        )?;

        Ok(Tag {
            id,
            name: name.to_string(),
            created_at: now,
        })
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

        // 2. Reject duplicate tag name
        let dup = TagRepository::create(&conn, "personal");
        assert!(dup.is_err(), "Duplicate tag name must be rejected by UNIQUE constraint");

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

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
