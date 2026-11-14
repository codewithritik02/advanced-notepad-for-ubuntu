use crate::storage::errors::StorageError;
use rusqlite::{params, Connection};

/// Keys the webview may read or write through the generic `get_setting` / `set_setting` commands.
/// Everything else in the settings table (encryption metadata, backup configuration, ...) is owned
/// by the backend and has dedicated, validated commands, so a webview bug or injected script can
/// never overwrite it through the generic path.
pub const FRONTEND_SETTING_KEYS: &[&str] = &["auto_lock_settings"];

/// Largest value the webview may store in a single setting (bytes).
pub const MAX_FRONTEND_SETTING_VALUE_BYTES: usize = 16 * 1024;

pub struct SettingsRepository;

impl SettingsRepository {
    /// Checks a key coming from the webview against the allowlist.
    pub fn validate_frontend_key(key: &str) -> Result<(), StorageError> {
        if FRONTEND_SETTING_KEYS.contains(&key) {
            Ok(())
        } else {
            Err(StorageError::Validation(
                "This setting is not available to the interface.".to_string(),
            ))
        }
    }

    /// Checks a key and value coming from the webview before they are stored.
    pub fn validate_frontend_write(key: &str, value: &str) -> Result<(), StorageError> {
        Self::validate_frontend_key(key)?;
        if value.len() > MAX_FRONTEND_SETTING_VALUE_BYTES {
            return Err(StorageError::Validation(format!(
                "Setting value is too large (limit {MAX_FRONTEND_SETTING_VALUE_BYTES} bytes)."
            )));
        }
        Ok(())
    }
}

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

    pub fn delete(conn: &Connection, key: &str) -> Result<(), StorageError> {
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_cannot_touch_backend_owned_settings() {
        assert!(SettingsRepository::validate_frontend_key("auto_lock_settings").is_ok());
        for key in [
            crate::storage::crypto::metadata::ENCRYPTION_METADATA_KEY,
            crate::storage::services::backup_settings::BACKUP_SETTINGS_KEY,
            "",
            "Auto_Lock_Settings",
            "auto_lock_settings ",
            "theme",
        ] {
            assert!(
                SettingsRepository::validate_frontend_key(key).is_err(),
                "key {key:?} must be refused"
            );
            assert!(SettingsRepository::validate_frontend_write(key, "{}").is_err());
        }
    }

    #[test]
    fn frontend_write_rejects_oversized_values() {
        let ok = "x".repeat(MAX_FRONTEND_SETTING_VALUE_BYTES);
        let too_big = "x".repeat(MAX_FRONTEND_SETTING_VALUE_BYTES + 1);
        assert!(SettingsRepository::validate_frontend_write("auto_lock_settings", &ok).is_ok());
        assert!(
            SettingsRepository::validate_frontend_write("auto_lock_settings", &too_big).is_err()
        );
    }
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;

    #[test]
    fn test_settings_upsert_and_retrieval() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_test_settings_{}", uuid::Uuid::new_v4()));
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
