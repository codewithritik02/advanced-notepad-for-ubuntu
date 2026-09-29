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
            commands::list_notebooks,
            commands::create_notebook,
            commands::list_tags,
            commands::create_tag,
            commands::add_note_tag,
            commands::get_note_tags,
            commands::get_setting,
            commands::set_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
