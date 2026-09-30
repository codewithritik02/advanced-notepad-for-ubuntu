pub mod notebooks;
pub mod notes;
pub mod search;
pub mod settings;
pub mod tags;

pub use notebooks::NotebookRepository;
pub use notes::NoteRepository;
pub use search::SearchRepository;
pub use settings::SettingsRepository;
pub use tags::TagRepository;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::{CreateNotebookDto, CreateNoteDto, UpdateNoteDto};

    #[test]
    fn test_repositories_crud_and_persistence() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_repo_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init connection failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Notebooks
        let parent_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Work Projects".to_string(),
                parent_id: None,
            },
        ).expect("Create parent notebook failed");

        let child_nb = NotebookRepository::create(
            &conn,
            CreateNotebookDto {
                name: "Personal Notepad".to_string(),
                parent_id: Some(parent_nb.id.clone()),
            },
        ).expect("Create child notebook failed");

        let nbs = NotebookRepository::list(&conn).expect("List notebooks failed");
        assert_eq!(nbs.len(), 2);
        assert_eq!(child_nb.parent_id, Some(parent_nb.id.clone()));

        // 2. Notes
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Architecture Design".to_string()),
                content: Some("Clean 3-tier desktop architecture".to_string()),
                format: Some("md".to_string()),
                notebook_id: Some(child_nb.id.clone()),
                is_favorite: Some(true),
                is_pinned: Some(true),
            },
        ).expect("Create note failed");

        assert_eq!(note.title, "Architecture Design");
        assert_eq!(note.format, "md");
        assert!(note.is_favorite);
        assert!(note.is_pinned);

        // Get by ID
        let fetched = NoteRepository::get_by_id(&conn, &note.id)
            .expect("Get note failed")
            .expect("Note must exist");
        assert_eq!(fetched.id, note.id);

        // Update note
        let updated = NoteRepository::update(
            &conn,
            &note.id,
            UpdateNoteDto {
                title: Some("Updated Title 🚀".to_string()),
                content: Some("New content with emojis".to_string()),
                format: None,
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: None,
                is_deleted: None,
            },
        ).expect("Update note failed");
        assert_eq!(updated.title, "Updated Title 🚀");
        assert_eq!(updated.content, "New content with emojis");
        assert!(!updated.is_favorite);

        // 3. Tags & Note-Tags
        let tag = TagRepository::create(&conn, "urgent").expect("Create tag failed");
        let tag2 = TagRepository::create(&conn, "offline").expect("Create tag 2 failed");

        TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).expect("Link tag 1 failed");
        TagRepository::add_tag_to_note(&conn, &note.id, &tag2.id).expect("Link tag 2 failed");

        let note_tags = TagRepository::get_tags_for_note(&conn, &note.id).expect("Get note tags failed");
        assert_eq!(note_tags.len(), 2);

        // 4. Settings
        SettingsRepository::set(&conn, "theme", "dark").expect("Set setting failed");
        let theme = SettingsRepository::get(&conn, "theme").expect("Get setting failed");
        assert_eq!(theme, Some("dark".to_string()));

        // Update existing setting
        SettingsRepository::set(&conn, "theme", "light").expect("Update setting failed");
        let theme2 = SettingsRepository::get(&conn, "theme").expect("Get setting failed");
        assert_eq!(theme2, Some("light".to_string()));

        // 5. Soft Delete Note
        let deleted = NoteRepository::delete(&conn, &note.id, true).expect("Soft delete failed");
        assert!(deleted);

        let active_notes = NoteRepository::list(&conn, false).expect("List active notes failed");
        assert_eq!(active_notes.len(), 0);

        let all_notes = NoteRepository::list(&conn, true).expect("List all notes failed");
        assert_eq!(all_notes.len(), 1);
        assert!(all_notes[0].is_deleted);

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
