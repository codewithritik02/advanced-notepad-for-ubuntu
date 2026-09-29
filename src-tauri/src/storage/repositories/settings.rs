use rusqlite::{params, Connection};
use crate::storage::errors::StorageError;

pub struct SettingsRepository;

impl SettingsRepository {
    pub fn get(conn: &Connection, key: &str) -> Result<Option<String>, StorageError> {
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set(conn: &Connection, key: &str, value: &str) -> Result<(), StorageError> {
        conn.execute(
            "INSERT INTO settings (key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;

    #[test]
    fn test_settings_upsert_and_retrieval() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_settings_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Non-existent key returns None
        let missing = SettingsRepository::get(&conn, "non_existent").expect("Query failed");
        assert_eq!(missing, None);

        // 2. Initial setting insert
        SettingsRepository::set(&conn, "theme", "dark").expect("Set failed");
        let theme = SettingsRepository::get(&conn, "theme").expect("Get failed");
        assert_eq!(theme, Some("dark".to_string()));

        // 3. Update existing setting (ON CONFLICT DO UPDATE)
        SettingsRepository::set(&conn, "theme", "light").expect("Update failed");
        let updated_theme = SettingsRepository::get(&conn, "theme").expect("Get failed");
        assert_eq!(updated_theme, Some("light".to_string()));

        // 4. Unicode setting value
        SettingsRepository::set(&conn, "author", "ऋतिक सैनी ✍️").expect("Set unicode failed");
        let author = SettingsRepository::get(&conn, "author").expect("Get unicode failed");
        assert_eq!(author, Some("ऋतिक सैनी ✍️".to_string()));

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
