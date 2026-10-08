use chrono::Utc;
use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::storage::errors::{AttachmentError, StorageError};
use crate::storage::models::{Attachment, CreateAttachmentDto};

/// Repository responsible for safe and efficient SQLite operations on the `attachments` table.
/// Adheres to Phase 7, Task 7 and Task 9 requirements:
/// - Parameterized queries for SQL injection prevention
/// - Strict validation and foreign key constraints
/// - Deterministic ordering by created_at ASC, id ASC
pub struct AttachmentRepository;

impl AttachmentRepository {
    /// Maps a database row into an `Attachment` domain model.
    fn map_row(row: &Row) -> rusqlite::Result<Attachment> {
        Ok(Attachment {
            id: row.get(0)?,
            note_id: row.get(1)?,
            file_name: row.get(2)?,
            relative_path: row.get(3)?,
            mime_type: row.get(4)?,
            file_size: row.get(5)?,
            created_at: row.get(6)?,
            modified_at: row.get(7)?,
        })
    }

    /// Verifies that a note exists in the database and is eligible for attachment operations.
    /// Rejects requests for nonexistent notes or soft-deleted notes.
    pub fn verify_note_eligible(conn: &Connection, note_id: &str) -> Result<(), AttachmentError> {
        let trimmed_id = note_id.trim();
        if trimmed_id.is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "Note ID cannot be empty".to_string(),
            ));
        }

        let mut stmt = conn.prepare("SELECT is_deleted FROM notes WHERE id = ?1")?;

        let mut rows = stmt.query(params![trimmed_id])?;
        if let Some(row) = rows.next()? {
            let is_deleted: bool = row.get(0)?;
            if is_deleted {
                return Err(AttachmentError::InvalidRequest(
                    "Cannot attach files to a deleted note".to_string(),
                ));
            }
            Ok(())
        } else {
            Err(AttachmentError::NoteNotFound(format!(
                "Note with id '{trimmed_id}' does not exist"
            )))
        }
    }

    /// Verifies that a note exists in the database (used for reading attachments).
    pub fn verify_note_exists(conn: &Connection, note_id: &str) -> Result<bool, StorageError> {
        let trimmed_id = note_id.trim();
        if trimmed_id.is_empty() {
            return Ok(false);
        }

        let mut stmt = conn.prepare("SELECT 1 FROM notes WHERE id = ?1")?;
        let exists = stmt.exists(params![trimmed_id])?;
        Ok(exists)
    }

    /// Inserts a new attachment record into the database.
    pub fn create(conn: &Connection, dto: CreateAttachmentDto) -> Result<Attachment, StorageError> {
        if dto.note_id.trim().is_empty() {
            return Err(StorageError::Validation("note_id cannot be empty".to_string()));
        }
        if dto.file_name.trim().is_empty() {
            return Err(StorageError::Validation("file_name cannot be empty".to_string()));
        }
        if dto.relative_path.trim().is_empty() {
            return Err(StorageError::Validation(
                "relative_path cannot be empty".to_string(),
            ));
        }
        if dto.file_size < 0 {
            return Err(StorageError::Validation(
                "file_size cannot be negative".to_string(),
            ));
        }

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO attachments (id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                dto.note_id.trim(),
                dto.file_name.trim(),
                dto.relative_path.trim(),
                dto.mime_type,
                dto.file_size,
                now,
                now,
            ],
        )?;

        Ok(Attachment {
            id,
            note_id: dto.note_id.trim().to_string(),
            file_name: dto.file_name.trim().to_string(),
            relative_path: dto.relative_path.trim().to_string(),
            mime_type: dto.mime_type,
            file_size: dto.file_size,
            created_at: now.clone(),
            modified_at: now,
        })
    }

    /// Retrieves an attachment record by its unique identifier.
    pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<Attachment>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at
             FROM attachments
             WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::map_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// Lists all attachments for a specific note, ordered deterministically by creation time and ID.
    pub fn list_by_note_id(
        conn: &Connection,
        note_id: &str,
    ) -> Result<Vec<Attachment>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at
             FROM attachments
             WHERE note_id = ?1
             ORDER BY created_at ASC, id ASC",
        )?;

        let mut rows = stmt.query(params![note_id.trim()])?;
        let mut attachments = Vec::new();
        while let Some(row) = rows.next()? {
            attachments.push(Self::map_row(row)?);
        }

        Ok(attachments)
    }

    /// Lists all attachment records across all notes in the database (Phase 7, Task 21).
    pub fn list_all(conn: &Connection) -> Result<Vec<Attachment>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, note_id, file_name, relative_path, mime_type, file_size, created_at, modified_at
             FROM attachments
             ORDER BY created_at ASC, id ASC",
        )?;

        let mut rows = stmt.query([])?;
        let mut attachments = Vec::new();
        while let Some(row) = rows.next()? {
            attachments.push(Self::map_row(row)?);
        }

        Ok(attachments)
    }

    /// Deletes an attachment record by its unique identifier.
    /// Returns true if a record was deleted, false otherwise.
    pub fn delete(conn: &Connection, id: &str) -> Result<bool, StorageError> {
        let affected = conn.execute("DELETE FROM attachments WHERE id = ?1", params![id])?;

        Ok(affected > 0)
    }

    /// Counts the number of attachments associated with a note.
    pub fn count_by_note_id(conn: &Connection, note_id: &str) -> Result<usize, StorageError> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM attachments WHERE note_id = ?1")?;

        let count: i64 = stmt.query_row(params![note_id.trim()], |row| row.get(0))?;
        Ok(count.max(0) as usize)
    }

    /// Returns a map of note_id -> attachment_count for a batch of note IDs (Phase 7, Task 19).
    /// Uses a single parameterized query with GROUP BY, eliminating N+1 database queries.
    pub fn batch_count_by_note_ids(
        conn: &Connection,
        note_ids: &[String],
    ) -> Result<std::collections::HashMap<String, usize>, StorageError> {
        if note_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let placeholders = vec!["?"; note_ids.len()].join(",");
        let sql = format!(
            "SELECT note_id, COUNT(*) FROM attachments WHERE note_id IN ({}) GROUP BY note_id",
            placeholders
        );

        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = note_ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();

        let mut rows = stmt.query(params.as_slice())?;
        let mut map = std::collections::HashMap::new();
        while let Some(row) = rows.next()? {
            let note_id: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            map.insert(note_id, count.max(0) as usize);
        }

        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::CreateNoteDto;
    use crate::storage::repositories::NoteRepository;

    fn setup_test_db() -> (Connection, PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_att_repo_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("Create tempdir");
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection");
        run_migrations(&mut conn).expect("Run migrations");
        (conn, temp_dir)
    }

    #[test]
    fn test_attachment_repository_crud() {
        let (conn, _td) = setup_test_db();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note for attachment".to_string()),
                ..Default::default()
            },
        )
        .expect("Create note");

        // Verify note eligibility
        assert!(AttachmentRepository::verify_note_eligible(&conn, &note.id).is_ok());

        // Create attachment record
        let att = AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note.id.clone(),
                file_name: "receipt.pdf".to_string(),
                relative_path: format!("{}/unique-receipt.pdf", note.id),
                mime_type: Some("application/pdf".to_string()),
                file_size: 4096,
            },
        )
        .expect("Create attachment");

        assert_eq!(att.note_id, note.id);
        assert_eq!(att.file_name, "receipt.pdf");
        assert_eq!(att.file_size, 4096);
        assert!(att.is_valid());

        // Get by ID
        let fetched = AttachmentRepository::get_by_id(&conn, &att.id)
            .expect("Get attachment")
            .expect("Should exist");
        assert_eq!(fetched.id, att.id);

        // List by note ID
        let list = AttachmentRepository::list_by_note_id(&conn, &note.id).expect("List attachments");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, att.id);

        // Count
        let count = AttachmentRepository::count_by_note_id(&conn, &note.id).expect("Count attachments");
        assert_eq!(count, 1);

        // Delete
        let deleted = AttachmentRepository::delete(&conn, &att.id).expect("Delete attachment");
        assert!(deleted);
        assert_eq!(
            AttachmentRepository::get_by_id(&conn, &att.id).expect("Get attachment"),
            None
        );
        assert_eq!(
            AttachmentRepository::count_by_note_id(&conn, &note.id).expect("Count"),
            0
        );
    }

    #[test]
    fn test_attachment_repository_note_validation() {
        let (conn, _td) = setup_test_db();

        // Nonexistent note
        let err = AttachmentRepository::verify_note_eligible(&conn, "nonexistent-id").unwrap_err();
        match err {
            AttachmentError::NoteNotFound(_) => {}
            other => panic!("Expected NoteNotFound, got: {other:?}"),
        }

        // Soft-deleted note
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Deleted note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        NoteRepository::delete(&conn, &note.id, true).unwrap();

        let err2 = AttachmentRepository::verify_note_eligible(&conn, &note.id).unwrap_err();
        match err2 {
            AttachmentError::InvalidRequest(_) => {}
            other => panic!("Expected InvalidRequest for deleted note, got: {other:?}"),
        }
    }

    #[test]
    fn test_attachment_repository_deterministic_ordering_and_isolation() {
        let (conn, _td) = setup_test_db();

        let note_a = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note A".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let note_b = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note B".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // Insert attachments for Note A
        let att_a1 = AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note_a.id.clone(),
                file_name: "doc_alpha.pdf".to_string(),
                relative_path: "note_a/doc_alpha.pdf".to_string(),
                mime_type: Some("application/pdf".to_string()),
                file_size: 100,
            },
        )
        .unwrap();

        // Small sleep or ensure deterministic time/id ordering
        std::thread::sleep(std::time::Duration::from_millis(10));

        let att_a2 = AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note_a.id.clone(),
                file_name: "doc_beta.pdf".to_string(),
                relative_path: "note_a/doc_beta.pdf".to_string(),
                mime_type: Some("application/pdf".to_string()),
                file_size: 200,
            },
        )
        .unwrap();

        // Insert attachment for Note B
        let _att_b1 = AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note_b.id.clone(),
                file_name: "doc_gamma.pdf".to_string(),
                relative_path: "note_b/doc_gamma.pdf".to_string(),
                mime_type: Some("application/pdf".to_string()),
                file_size: 300,
            },
        )
        .unwrap();

        // 1. Isolation check: Note A query retrieves only Note A attachments
        let list_a = AttachmentRepository::list_by_note_id(&conn, &note_a.id).unwrap();
        assert_eq!(list_a.len(), 2);
        assert_eq!(list_a[0].id, att_a1.id);
        assert_eq!(list_a[1].id, att_a2.id);

        // 2. Note B query retrieves only Note B attachments
        let list_b = AttachmentRepository::list_by_note_id(&conn, &note_b.id).unwrap();
        assert_eq!(list_b.len(), 1);
        assert_eq!(list_b[0].file_name, "doc_gamma.pdf");

        // 3. Count isolation
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note_a.id).unwrap(), 2);
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note_b.id).unwrap(), 1);
    }

    #[test]
    fn test_attachment_repository_database_errors_not_swallowed() {
        let (conn, _td) = setup_test_db();

        // Drop the attachments table to simulate database failure
        conn.execute("DROP TABLE attachments", []).unwrap();

        // list_by_note_id must return Err and NOT silently return an empty Vec
        let res = AttachmentRepository::list_by_note_id(&conn, "any-note");
        assert!(res.is_err(), "Database errors must not be silently converted to empty list");

        // get_by_id must also return Err on DB error, distinguishing failure from missing record
        let res_get = AttachmentRepository::get_by_id(&conn, "any-id");
        assert!(res_get.is_err(), "Database errors must not be silently converted to Ok(None)");
    }

    #[test]
    fn test_attachment_repository_sql_injection_defense() {
        let (conn, _td) = setup_test_db();

        let malicious_payloads = [
            "' OR '1'='1",
            "'; DROP TABLE attachments; --",
            "\" OR \"\"=\"",
            "1 UNION SELECT * FROM notes --",
        ];

        for payload in malicious_payloads {
            // Parameterized query should treat payload as literal note_id and return 0
            let list = AttachmentRepository::list_by_note_id(&conn, payload).unwrap();
            assert_eq!(list.len(), 0);

            let count = AttachmentRepository::count_by_note_id(&conn, payload).unwrap();
            assert_eq!(count, 0);

            let get_res = AttachmentRepository::get_by_id(&conn, payload).unwrap();
            assert_eq!(get_res, None);
        }

        // Table must still exist and be intact
        let table_check: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='attachments'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(table_check, 1);
    }

    #[test]
    fn test_attachment_repository_cascade_on_note_hard_delete() {
        let (conn, _td) = setup_test_db();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Cascade Note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let att = AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note.id.clone(),
                file_name: "cascade.pdf".to_string(),
                relative_path: "note/cascade.pdf".to_string(),
                mime_type: None,
                file_size: 512,
            },
        )
        .unwrap();

        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(), 1);

        // Permanently (hard) delete note
        NoteRepository::delete(&conn, &note.id, false).unwrap();

        // Foreign key cascade should remove attachment record
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(), 0);
        assert_eq!(AttachmentRepository::get_by_id(&conn, &att.id).unwrap(), None);
    }

    #[test]
    fn test_task_19_attachment_count_and_batch_count() {
        let (conn, _td) = setup_test_db();

        let note1 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note 1".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note 2".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let note3 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Note 3".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // Initially zero attachments
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note1.id).unwrap(), 0);
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note2.id).unwrap(), 0);
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note3.id).unwrap(), 0);

        // Add 2 to note1, 1 to note2, 0 to note3
        AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note1.id.clone(),
                file_name: "file1.txt".to_string(),
                relative_path: "n1/file1.txt".to_string(),
                mime_type: None,
                file_size: 100,
            },
        )
        .unwrap();

        AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note1.id.clone(),
                file_name: "file2.txt".to_string(),
                relative_path: "n1/file2.txt".to_string(),
                mime_type: None,
                file_size: 200,
            },
        )
        .unwrap();

        AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: note2.id.clone(),
                file_name: "file3.pdf".to_string(),
                relative_path: "n2/file3.pdf".to_string(),
                mime_type: None,
                file_size: 300,
            },
        )
        .unwrap();

        // Individual counts
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note1.id).unwrap(), 2);
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note2.id).unwrap(), 1);
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note3.id).unwrap(), 0);

        // Batch counts avoiding N+1
        let batch = AttachmentRepository::batch_count_by_note_ids(
            &conn,
            &[note1.id.clone(), note2.id.clone(), note3.id.clone()],
        )
        .unwrap();

        assert_eq!(batch.get(&note1.id), Some(&2));
        assert_eq!(batch.get(&note2.id), Some(&1));
        assert_eq!(batch.get(&note3.id), None); // Not in GROUP BY if 0, maps cleanly to 0
    }

    #[test]
    fn test_task_22_repository_comprehensive_tests() {
        let (conn, _td) = setup_test_db();

        // 1. Valid note with no attachments returns empty list
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Empty Attachments Note".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let empty_list = AttachmentRepository::list_by_note_id(&conn, &note.id).unwrap();
        assert!(empty_list.is_empty());
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(), 0);

        // 2. Non-existent attachment retrieval returns Ok(None)
        let missing_att = AttachmentRepository::get_by_id(&conn, "non-existent-uuid").unwrap();
        assert_eq!(missing_att, None);

        // 3. Deleting non-existent attachment returns Ok(false)
        let deleted = AttachmentRepository::delete(&conn, "non-existent-uuid").unwrap();
        assert!(!deleted);

        // 4. Foreign key integrity: inserting attachment with invalid note_id fails
        let invalid_fk_res = AttachmentRepository::create(
            &conn,
            CreateAttachmentDto {
                note_id: "nonexistent-note-id".to_string(),
                file_name: "test.pdf".to_string(),
                relative_path: "nonexistent-note-id/test.pdf".to_string(),
                mime_type: Some("application/pdf".to_string()),
                file_size: 1024,
            },
        );
        assert!(invalid_fk_res.is_err(), "Foreign key constraint must reject nonexistent note_id");

        // 5. Note eligibility verification
        let missing_note_err = AttachmentRepository::verify_note_eligible(&conn, "missing-note").unwrap_err();
        match missing_note_err {
            AttachmentError::NoteNotFound(_) => {}
            other => panic!("Expected NoteNotFound, got: {other:?}"),
        }

        // Soft-deleted note eligibility
        NoteRepository::delete(&conn, &note.id, true).unwrap();
        let deleted_note_err = AttachmentRepository::verify_note_eligible(&conn, &note.id).unwrap_err();
        match deleted_note_err {
            AttachmentError::InvalidRequest(msg) => {
                assert!(msg.contains("deleted note"));
            }
            other => panic!("Expected InvalidRequest for deleted note, got: {other:?}"),
        }
    }
}
