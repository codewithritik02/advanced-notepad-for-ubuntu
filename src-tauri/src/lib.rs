pub mod commands;
pub mod storage;

use storage::{init_connection, run_migrations, Database, StoragePaths};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let storage_paths = StoragePaths::from_root(app_data_dir);
            storage_paths
                .ensure_directories()
                .map_err(|e| e.to_string())?;

            let mut conn =
                init_connection(&storage_paths.database_file).map_err(|e| e.to_string())?;

            // Execute all pending migrations in order
            run_migrations(&mut conn).map_err(|e| e.to_string())?;

            app.manage(storage_paths);
            app.manage(Database::new(conn));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_storage_info,
            commands::list_notes,
            commands::get_note,
            commands::create_note,
            commands::update_note,
            commands::delete_note,
            commands::restore_note,
            commands::move_note_to_notebook,
            commands::set_note_favorite,
            commands::toggle_note_favorite,
            commands::list_favorite_notes,
            commands::list_notebooks,
            commands::get_notebook,
            commands::create_notebook,
            commands::update_notebook,
            commands::delete_notebook,
            commands::list_tags,
            commands::get_tag,
            commands::create_tag,
            commands::update_tag,
            commands::delete_tag,
            commands::add_note_tag,
            commands::remove_note_tag,
            commands::get_note_tags,
            commands::set_note_tags,
            commands::get_notes_for_tag,
            commands::get_tag_note_counts,
            commands::get_all_notes_tags,
            commands::get_note_metadata,
            commands::get_note_notebook_path,
            commands::get_setting,
            commands::set_setting,
            commands::search_notes,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
