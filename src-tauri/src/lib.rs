pub mod commands;
pub mod storage;

use storage::{init_connection, run_migrations, Database, StoragePaths};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let storage_paths = StoragePaths::from_root(app_data_dir);
            storage_paths
                .ensure_directories()
                .map_err(|e| e.to_string())?;

            // Recover from any interrupted restore before opening active database (Phase 11, Task 19)
            let _ = storage::services::RestoreService::recover_if_interrupted(&storage_paths);

            let mut conn =
                init_connection(&storage_paths.database_file).map_err(|e| e.to_string())?;

            // Execute all pending migrations in order
            run_migrations(&mut conn).map_err(|e| e.to_string())?;

            let crypto_session = storage::crypto::CryptoSession::init_and_recover(&conn, &storage_paths)
                .map_err(|e| e.to_string())?;

            app.manage(storage_paths);
            app.manage(crypto_session);
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
            commands::soft_delete_note,
            commands::restore_note,
            commands::permanently_delete_note,
            commands::empty_trash,
            commands::list_trashed_notes,
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
            commands::get_tags_for_notes,
            commands::get_note_metadata,
            commands::get_note_notebook_path,
            commands::get_setting,
            commands::set_setting,
            commands::search_notes,
            commands::add_attachment,
            commands::list_attachments,
            commands::remove_attachment,
            commands::open_attachment,
            commands::get_attachment_count,
            commands::check_attachment_consistency,
            commands::cleanup_abandoned_temp_files,
            commands::resolve_link_target,
            commands::get_backlinks,
            commands::scan_import_files,
            commands::execute_import,
            commands::cancel_import_export_operation,
            commands::export_single_note,
            commands::preview_export_selection,
            commands::export_notes_batch,
            commands::create_backup,
            commands::get_backup_settings,
            commands::update_backup_settings,
            commands::check_and_run_automatic_backup,
            commands::get_backup_scheduler_state,
            commands::list_backups,
            commands::delete_managed_backup,
            commands::validate_backup,
            commands::preview_restore,
            commands::create_recovery_checkpoint,
            commands::list_recovery_checkpoints,
            commands::get_latest_recovery_checkpoint,
            commands::stage_restore_archive,
            commands::migrate_staged_database,
            commands::cleanup_staged_restore,
            commands::execute_restore,
            commands::cancel_restore_operation,
            commands::check_and_recover_interrupted_restore,
            commands::validate_post_restore_state,
            commands::get_backup_health_status,
            commands::get_encryption_status,
            commands::setup_encryption,
            commands::unlock_application,
            commands::lock_application,
            commands::change_encryption_password,
            commands::rollback_interrupted_migration,
            commands::resume_interrupted_migration,
            commands::disable_encryption,
            commands::get_security_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
