use rusqlite::Connection;
use crate::storage::errors::StorageError;
use crate::storage::models::SearchResult;
use crate::storage::repositories::SearchRepository;

/// High-level SearchService that coordinates search requests between
/// Tauri IPC commands and the low-level SearchRepository.
///
/// Follows the recommended architecture from Task 92 (phase-6-specification.md:2530-2552):
/// Tauri Command -> Search Service -> Search Repository -> SQLite
pub struct SearchService;

impl SearchService {
    /// Executes search by validating input and delegating to SearchRepository.
    pub fn search(
        conn: &Connection,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<SearchResult>, StorageError> {
        SearchRepository::search(conn, query, limit)
    }

    /// Executes search with optional active encryption key (Phase 12, Task 11).
    pub fn search_with_key(
        conn: &Connection,
        query: &str,
        limit: Option<usize>,
        dek: Option<&crate::storage::crypto::SecretBytes>,
    ) -> Result<Vec<SearchResult>, StorageError> {
        SearchRepository::search_with_key(conn, query, limit, dek)
    }

    /// Rebuilds search index when needed via SearchRepository.
    pub fn rebuild_index(conn: &Connection) -> Result<(), StorageError> {
        SearchRepository::rebuild_search_index(conn)
    }

    /// Recovers search index if corrupted or missing.
    pub fn recover_index(conn: &Connection) -> Result<(), StorageError> {
        SearchRepository::recover_index(conn)
    }

    /// Purges unencrypted plaintext FTS index and triggers when encryption is enabled (Task 11).
    pub fn purge_plaintext_index(conn: &Connection) -> Result<(), StorageError> {
        SearchRepository::purge_plaintext_index(conn)
    }

    /// Diagnoses search index health, trigger status, and consistency (Phase 13, Task 10).
    pub fn diagnose_index(conn: &Connection) -> Result<crate::storage::models::SearchIndexDiagnostic, StorageError> {
        SearchRepository::diagnose_index(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::CreateNoteDto;
    use crate::storage::repositories::NoteRepository;

    #[test]
    fn test_search_service_pipeline() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_svc_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Search Query Repository".to_string()),
                content: Some("SQL should live in the Rust storage/repository layer, not inside React or Tauri commands.".to_string()),
                ..Default::default()
            },
        ).unwrap();

        // Verification of the pipeline: SearchService -> SearchRepository -> SQLite
        let results = SearchService::search(&conn, "storage/repository", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Search Query Repository");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_search_service_encrypted_pipeline() {
        use crate::storage::crypto::{KdfConfig, EncryptionMetadata, SecretBytes, WrappedKey, generate_salt, ENCRYPTION_METADATA_KEY};
        use crate::storage::repositories::SettingsRepository;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_srch_svc_enc_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).unwrap();
        run_migrations(&mut conn).unwrap();

        let dummy_kdf = KdfConfig::default_profile(generate_salt());
        let dummy_wrapped = WrappedKey::new(&[0u8; 12], &[0u8; 32], &[0u8; 16]);
        let dummy_meta = EncryptionMetadata::new(dummy_kdf, dummy_wrapped);
        SettingsRepository::set(&conn, ENCRYPTION_METADATA_KEY, &dummy_meta.to_json().unwrap()).unwrap();

        SearchService::purge_plaintext_index(&conn).unwrap();

        let dek = SecretBytes::new(vec![0x99; 32]);
        NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Service Encrypted Note".to_string()),
                content: Some("SearchService safely coordinates encrypted in-memory full text search.".to_string()),
                ..Default::default()
            },
            Some(&dek),
        ).unwrap();

        // Locked search rejects
        let locked_res = SearchService::search_with_key(&conn, "SearchService", None, None);
        assert!(locked_res.is_err());

        // Unlocked search returns decrypted match
        let unlocked_res = SearchService::search_with_key(&conn, "coordinates encrypted", None, Some(&dek)).unwrap();
        assert_eq!(unlocked_res.len(), 1);
        assert_eq!(unlocked_res[0].title, "Service Encrypted Note");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
