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

    /// Rebuilds search index when needed via SearchRepository.
    pub fn rebuild_index(conn: &Connection) -> Result<(), StorageError> {
        SearchRepository::rebuild_search_index(conn)
    }

    /// Recovers search index if corrupted or missing.
    pub fn recover_index(conn: &Connection) -> Result<(), StorageError> {
        SearchRepository::recover_index(conn)
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
}
