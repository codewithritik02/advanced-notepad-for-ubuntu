use rusqlite::{params, Connection, Row};
use crate::storage::errors::StorageError;
use crate::storage::models::{CreateNotebookDto, Notebook, UpdateNotebookDto};

pub struct NotebookRepository;

impl NotebookRepository {
    fn map_row(row: &Row) -> rusqlite::Result<Notebook> {
        Ok(Notebook {
            id: row.get(0)?,
            name: row.get(1)?,
            parent_id: row.get(2)?,
            created_at: row.get(3)?,
            modified_at: row.get(4)?,
        })
    }

    pub fn create(conn: &Connection, dto: CreateNotebookDto) -> Result<Notebook, StorageError> {
        let name = dto.name.trim();
        if name.is_empty() {
            return Err(StorageError::Validation("Notebook name cannot be empty".to_string()));
        }

        // Validate parent_id exists if specified
        if let Some(ref pid) = dto.parent_id {
            if Self::get_by_id(conn, pid)?.is_none() {
                return Err(StorageError::InvalidHierarchy(format!(
                    "Parent notebook with ID '{pid}' does not exist"
                )));
            }
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO notebooks (id, name, parent_id, created_at, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, dto.parent_id, now, now],
        )?;

        Ok(Notebook {
            id,
            name: name.to_string(),
            parent_id: dto.parent_id,
            created_at: now.clone(),
            modified_at: now,
        })
    }

    pub fn list(conn: &Connection) -> Result<Vec<Notebook>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, created_at, modified_at
             FROM notebooks
             ORDER BY parent_id ASC, name ASC",
        )?;

        let iter = stmt.query_map([], Self::map_row)?;
        let mut notebooks = Vec::new();
        for item in iter {
            notebooks.push(item?);
        }
        Ok(notebooks)
    }

    pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<Notebook>, StorageError> {
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, created_at, modified_at
             FROM notebooks
             WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::map_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn update(conn: &Connection, id: &str, dto: UpdateNotebookDto) -> Result<Notebook, StorageError> {
        let existing = Self::get_by_id(conn, id)?
            .ok_or_else(|| StorageError::NotFound(format!("Notebook with id '{id}' not found")))?;

        let name = dto.name.trim();
        if name.is_empty() {
            return Err(StorageError::Validation("Notebook name cannot be empty".to_string()));
        }

        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE notebooks
             SET name = ?1, modified_at = ?2
             WHERE id = ?3",
            params![name, now, id],
        )?;

        Ok(Notebook {
            id: id.to_string(),
            name: name.to_string(),
            parent_id: existing.parent_id,
            created_at: existing.created_at,
            modified_at: now,
        })
    }

    pub fn delete(conn: &mut Connection, id: &str) -> Result<bool, StorageError> {
        let _existing = Self::get_by_id(conn, id)?
            .ok_or_else(|| StorageError::NotFound(format!("Notebook with id '{id}' not found")))?;

        // Verify no child notebooks exist
        let child_count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM notebooks WHERE parent_id = ?1",
            params![id],
            |row| row.get(0),
        )?;

        if child_count > 0 {
            return Err(StorageError::DeleteBlocked(
                "This notebook contains sub-notebooks. Move or delete the sub-notebooks before deleting this notebook.".to_string(),
            ));
        }

        // Transactional delete: unassign notes, then delete notebook
        let tx = conn.transaction()?;

        tx.execute(
            "UPDATE notes SET notebook_id = NULL WHERE notebook_id = ?1",
            params![id],
        )?;

        let rows_affected = tx.execute(
            "DELETE FROM notebooks WHERE id = ?1",
            params![id],
        )?;

        tx.commit()?;

        Ok(rows_affected > 0)
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
    fn test_notebook_hierarchy_and_unicode() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_nb_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Rejection of empty notebook name
        let empty_res = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "   ".to_string(),
                parent_id: None,
            },
        );
        assert!(empty_res.is_err(), "Empty notebook name must be rejected");

        // 2. Unicode root notebook
        let root = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "व्यक्तिगत परियोजनाएं 📁".to_string(),
                parent_id: None,
            },
        ).expect("Create root notebook failed");
        assert_eq!(root.name, "व्यक्तिगत परियोजनाएं 📁");
        assert_eq!(root.parent_id, None);

        // 3. Child nested notebook
        let child = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Personal Notepad".to_string(),
                parent_id: Some(root.id.clone()),
            },
        ).expect("Create child notebook failed");
        assert_eq!(child.parent_id, Some(root.id.clone()));

        // 4. Verification of listing
        let list = NotebookRepository::list(&conn).expect("List failed");
        assert_eq!(list.len(), 2);

        // 5. Verification of get_by_id
        let fetched = NotebookRepository::get_by_id(&conn, &child.id)
            .expect("Get failed")
            .expect("Must exist");
        assert_eq!(fetched.id, child.id);
        assert_eq!(fetched.parent_id, Some(root.id));

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_notebook_crud_rename_and_safe_delete_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_nb_crud_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Invalid parent validation
        let invalid_parent_res = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Orphan Notebook".to_string(),
                parent_id: Some("non-existent-parent-uuid".to_string()),
            },
        );
        assert!(invalid_parent_res.is_err(), "Non-existent parent must be rejected");

        // 2. Create parent and child
        let parent = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Work".to_string(),
                parent_id: None,
            },
        ).expect("Parent creation failed");

        let child = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Projects".to_string(),
                parent_id: Some(parent.id.clone()),
            },
        ).expect("Child creation failed");

        // 3. Create note inside parent
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Parent Note".to_string()),
                content: Some("Important content".to_string()),
                notebook_id: Some(parent.id.clone()),
                ..Default::default()
            },
        ).expect("Note create failed");

        // 4. Attempt to delete parent while child exists -> MUST FAIL
        let del_blocked = NotebookRepository::delete(&mut conn, &parent.id);
        assert!(del_blocked.is_err(), "Deleting parent with children must fail");
        if let Err(StorageError::DeleteBlocked(msg)) = del_blocked {
            assert!(msg.contains("contains sub-notebooks"));
        } else {
            panic!("Expected DeleteBlocked error for sub-notebooks");
        }

        // 5. Rename child
        let renamed_child = NotebookRepository::update(
            &conn,
            &child.id,
            UpdateNotebookDto {
                name: "Active Projects 2026 🚀".to_string(),
            },
        ).expect("Rename child failed");
        assert_eq!(renamed_child.name, "Active Projects 2026 🚀");
        assert_eq!(renamed_child.parent_id, Some(parent.id.clone()));
        assert_eq!(renamed_child.id, child.id);

        // 6. Delete child first
        let child_deleted = NotebookRepository::delete(&mut conn, &child.id).expect("Delete child failed");
        assert!(child_deleted);
        assert!(NotebookRepository::get_by_id(&conn, &child.id).expect("Query failed").is_none());

        // 7. Now delete parent -> notes must be safely unfiled (notebook_id = NULL)
        let parent_deleted = NotebookRepository::delete(&mut conn, &parent.id).expect("Delete parent failed");
        assert!(parent_deleted);
        assert!(NotebookRepository::get_by_id(&conn, &parent.id).expect("Query failed").is_none());

        // Verify note survived intact and is unfiled
        let preserved_note = NoteRepository::get_by_id(&conn, &note.id)
            .expect("Query failed")
            .expect("Note must survive notebook deletion");
        assert_eq!(preserved_note.notebook_id, None);
        assert_eq!(preserved_note.title, "Parent Note");
        assert_eq!(preserved_note.content, "Important content");

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
