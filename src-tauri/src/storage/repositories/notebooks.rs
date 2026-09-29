use rusqlite::{params, Connection, Row};
use crate::storage::errors::StorageError;
use crate::storage::models::{CreateNotebookDto, Notebook};

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
             ORDER BY name ASC",
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;

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
}
