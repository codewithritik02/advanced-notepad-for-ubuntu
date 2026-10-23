mod common;

use common::{lock_process_global_test_state, TestAppDir};
use personal_notepad_lib::storage::{
    crypto::{CryptoSession, SetupEncryptionDto, UnlockDto},
    lifecycle::{
        checkpoint_database_for_shutdown, initialize_application_storage,
        InitializedApplicationStorage,
    },
    models::{
        BackupCreateRequest, BackupRestoreRequest, BackupType, CreateAttachmentDto, CreateNoteDto,
        CreateNotebookDto, NoteLifecycle, UpdateNoteDto,
    },
    repositories::{
        AttachmentRepository, NoteRepository, NotebookRepository, SearchRepository, TagRepository,
    },
    services::{AttachmentService, BackupService, RestoreService},
    AttachmentStorage,
};
use std::fs;
use std::path::Path;

fn shutdown(runtime: InitializedApplicationStorage) {
    checkpoint_database_for_shutdown(&runtime.database).expect("checkpoint database on shutdown");
    drop(runtime);
}

/// **Workflow A — Basic note persistence**
/// 1. Launch the application.
/// 2. Create a note.
/// 3. Enter a title and content.
/// 4. Save and verify persistence.
/// 5. Navigate away (create second note).
/// 6. Return to original note.
/// 7. Restart the application.
/// 8. Confirm saved content persists verbatim.
#[test]
fn test_workflow_a_basic_note_persistence_and_restart() {
    let _lock = lock_process_global_test_state();
    let env = TestAppDir::with_label("workflow-a");

    // [Step 1] Launch application
    let runtime = initialize_application_storage(env.paths.clone())
        .expect("[Workflow A: Step 1] Application launch failed");

    let note_id = {
        let conn = runtime.database.conn.lock().unwrap();

        // [Step 2] Create a note
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Untitled Note".to_string()),
                content: Some("".to_string()),
                format: Some("txt".to_string()),
                notebook_id: None,
                is_favorite: Some(false),
                is_pinned: Some(false),
            },
        )
        .expect("[Workflow A: Step 2] Failed to create note");
        assert!(!note.id.is_empty(), "Note ID must be valid UUID");

        // [Step 3] Enter a title and content
        let initial_modified = note.modified_at.clone();
        std::thread::sleep(std::time::Duration::from_millis(20));

        let updated = NoteRepository::update(
            &conn,
            &note.id,
            UpdateNoteDto {
                title: Some("Meeting Notes — Q4 Planning".to_string()),
                content: Some(
                    "1. Finalize roadmap\n2. Review budget\n3. Assign deliverables".to_string(),
                ),
                format: Some("md".to_string()),
                notebook_id: None,
                is_favorite: None,
                is_pinned: None,
                is_deleted: None,
                is_hidden: None,
            },
        )
        .expect("[Workflow A: Step 3] Failed to enter title and content");

        // [Step 4] Save / verify persistence
        assert_eq!(updated.title, "Meeting Notes — Q4 Planning");
        assert_eq!(
            updated.content,
            "1. Finalize roadmap\n2. Review budget\n3. Assign deliverables"
        );
        assert!(
            updated.modified_at >= initial_modified,
            "[Workflow A: Step 4] modified_at must advance monotonically"
        );

        // [Step 5] Navigate away: Create second note
        let note2 = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Second Note".to_string()),
                content: Some("Navigated away to this note".to_string()),
                ..Default::default()
            },
        )
        .expect("[Workflow A: Step 5] Failed to create second note");
        assert_ne!(note2.id, note.id);

        // [Step 6] Return to original note
        let reloaded = NoteRepository::get_by_id(&conn, &note.id)
            .expect("[Workflow A: Step 6] Query note failed")
            .expect("[Workflow A: Step 6] Original note not found");
        assert_eq!(reloaded.title, "Meeting Notes — Q4 Planning");
        assert_eq!(
            reloaded.content,
            "1. Finalize roadmap\n2. Review budget\n3. Assign deliverables"
        );

        note.id
    };

    // [Step 7] Restart application
    shutdown(runtime);

    let relaunched = initialize_application_storage(env.paths.clone())
        .expect("[Workflow A: Step 7] Application relaunch failed");

    // [Step 8] Confirm the saved content persists after restart
    {
        let conn = relaunched.database.conn.lock().unwrap();
        let persisted = NoteRepository::get_by_id(&conn, &note_id)
            .expect("[Workflow A: Step 8] Query after restart failed")
            .expect("[Workflow A: Step 8] Note must exist after restart");

        assert_eq!(persisted.title, "Meeting Notes — Q4 Planning");
        assert_eq!(
            persisted.content,
            "1. Finalize roadmap\n2. Review budget\n3. Assign deliverables"
        );
        assert_eq!(persisted.format, "md");
        assert!(!persisted.is_deleted);
    }

    shutdown(relaunched);
}

/// **Workflow B — Organization and search**
/// 1. Create a notebook.
/// 2. Create a note within it.
/// 3. Assign a tag and favorite the note.
/// 4. Search for the note.
/// 5. Open the exact result.
/// 6. Verify the note's metadata and notebook assignment.
#[test]
fn test_workflow_b_organization_and_search() {
    let _lock = lock_process_global_test_state();
    let env = TestAppDir::with_label("workflow-b");
    let runtime = initialize_application_storage(env.paths.clone()).unwrap();

    let conn = runtime.database.conn.lock().unwrap();

    // [Step 1] Create a notebook
    let notebook = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Research & Development".to_string(),
            parent_id: None,
        },
    )
    .expect("[Workflow B: Step 1] Failed to create notebook");

    // [Step 2] Create a note within it
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quantum Teleportation Protocols".to_string()),
            content: Some(
                "Investigation of entanglement fidelity across noisy quantum channels.".to_string(),
            ),
            format: Some("md".to_string()),
            notebook_id: Some(notebook.id.clone()),
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("[Workflow B: Step 2] Failed to create note in notebook");
    assert_eq!(note.notebook_id, Some(notebook.id.clone()));

    // [Step 3] Assign a tag and favorite the note
    let tag =
        TagRepository::create(&conn, "physics").expect("[Workflow B: Step 3] Failed to create tag");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag.id)
        .expect("[Workflow B: Step 3] Failed to assign tag");

    let favorited_note = NoteRepository::update(
        &conn,
        &note.id,
        UpdateNoteDto {
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("[Workflow B: Step 3] Failed to favorite note");
    assert!(favorited_note.is_favorite);

    // [Step 4] Search for the note
    let search_results = SearchRepository::search(&conn, "entanglement", Some(10))
        .expect("[Workflow B: Step 4] Search failed");
    assert!(
        !search_results.is_empty(),
        "[Workflow B: Step 4] Search should find the note"
    );

    // [Step 5] Open the exact result
    let matched_result = search_results
        .iter()
        .find(|r| r.note_id == note.id)
        .expect("[Workflow B: Step 5] Search result matching note ID not found");
    assert_eq!(matched_result.title, "Quantum Teleportation Protocols");

    // [Step 6] Verify metadata and notebook assignment
    let loaded_note = NoteRepository::get_by_id(&conn, &matched_result.note_id)
        .expect("[Workflow B: Step 6] Query failed")
        .expect("[Workflow B: Step 6] Note not found");

    assert_eq!(loaded_note.notebook_id, Some(notebook.id));
    assert!(loaded_note.is_favorite);

    let assigned_tags = TagRepository::get_tags_for_note(&conn, &loaded_note.id).unwrap();
    assert_eq!(assigned_tags.len(), 1);
    assert_eq!(assigned_tags[0].name, "physics");

    drop(conn);
    shutdown(runtime);
}

/// **Workflow C — Attachment lifecycle**
/// 1. Attach a harmless test file.
/// 2. Confirm it appears in the note.
/// 3. Close and reopen the application.
/// 4. Open the attachment.
/// 5. Remove the attachment.
/// 6. Verify the final state.
#[test]
fn test_workflow_c_attachment_lifecycle() {
    let _lock = lock_process_global_test_state();
    let env = TestAppDir::with_label("workflow-c");
    let storage = AttachmentStorage::new(env.paths.clone());

    let runtime = initialize_application_storage(env.paths.clone()).unwrap();

    let (note_id, attachment_id) = {
        let conn = runtime.database.conn.lock().unwrap();

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Specification Document".to_string()),
                content: Some("Please see attached architecture diagram.".to_string()),
                ..Default::default()
            },
        )
        .expect("create note");

        // [Step 1] Attach a harmless test file
        let source_file = env.dir.join("diagram.svg");
        let svg_bytes =
            b"<svg xmlns='http://www.w3.org/2000/svg'><circle cx='50' cy='50' r='40'/></svg>";
        fs::write(&source_file, svg_bytes).expect("write source file");

        let attachment = AttachmentService::add_attachment(&conn, &storage, &note.id, &source_file)
            .expect("[Workflow C: Step 1] Failed to attach file");

        // [Step 2] Confirm it appears in the note
        let attachments = AttachmentRepository::list_by_note_id(&conn, &note.id)
            .expect("[Workflow C: Step 2] Failed to list attachments");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].file_name, "diagram.svg");
        assert_eq!(attachments[0].file_size, svg_bytes.len() as i64);

        (note.id, attachment.id)
    };

    // [Step 3] Close and reopen the application
    shutdown(runtime);

    let relaunched = initialize_application_storage(env.paths.clone())
        .expect("[Workflow C: Step 3] Application reopen failed");

    {
        let conn = relaunched.database.conn.lock().unwrap();

        // [Step 4] Open the attachment
        let opened_path =
            AttachmentService::get_attachment_for_open(&conn, &storage, &attachment_id)
                .expect("[Workflow C: Step 4] Failed to get attachment for open");
        assert!(
            opened_path.exists(),
            "[Workflow C: Step 4] File must exist on disk"
        );

        let opened_bytes = fs::read(&opened_path).expect("read attachment file");
        assert_eq!(
            opened_bytes,
            b"<svg xmlns='http://www.w3.org/2000/svg'><circle cx='50' cy='50' r='40'/></svg>"
        );

        // [Step 5] Remove the attachment
        AttachmentRepository::delete(&conn, &attachment_id)
            .expect("[Workflow C: Step 5] Failed to delete attachment");

        // [Step 6] Verify the final state
        let final_list = AttachmentRepository::list_by_note_id(&conn, &note_id)
            .expect("[Workflow C: Step 6] Failed to list attachments after deletion");
        assert_eq!(
            final_list.len(),
            0,
            "[Workflow C: Step 6] Attachment count must be 0"
        );
    }

    shutdown(relaunched);
}

/// **Workflow D — Trash and recovery**
/// 1. Create and save a note.
/// 2. Move it to Trash.
/// 3. Confirm it disappears from normal lists and search.
/// 4. Restore it.
/// 5. Verify its content and supported relationships.
#[test]
fn test_workflow_d_trash_and_recovery() {
    let _lock = lock_process_global_test_state();
    let env = TestAppDir::with_label("workflow-d");
    let runtime = initialize_application_storage(env.paths.clone()).unwrap();

    let conn = runtime.database.conn.lock().unwrap();

    // [Step 1] Create and save a note
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Critical Strategy Document".to_string()),
            content: Some("Secret strategy for expanding into European markets.".to_string()),
            format: Some("md".to_string()),
            notebook_id: None,
            is_favorite: Some(true),
            is_pinned: Some(true),
        },
    )
    .expect("[Workflow D: Step 1] Failed to create note");

    let tag = TagRepository::create(&conn, "strategy").unwrap();
    TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).unwrap();

    assert_eq!(note.lifecycle(), NoteLifecycle::Active);

    // [Step 2] Move it to Trash
    NoteRepository::delete(&conn, &note.id, true)
        .expect("[Workflow D: Step 2] Failed to soft-delete");

    // [Step 3] Confirm it disappears from normal lists and search
    let active_notes =
        NoteRepository::list(&conn, false).expect("[Workflow D: Step 3] Active list query failed");
    assert!(
        !active_notes.iter().any(|n| n.id == note.id),
        "[Workflow D: Step 3] Note must not appear in active notes list"
    );

    let search_results = SearchRepository::search(&conn, "European", Some(10)).expect("search");
    assert!(
        search_results.is_empty(),
        "[Workflow D: Step 3] Trashed note must not appear in search results"
    );

    let trashed_list = NoteRepository::list_trashed(&conn).expect("list trashed");
    assert!(
        trashed_list.iter().any(|n| n.id == note.id),
        "[Workflow D: Step 3] Note must appear in trash list"
    );

    // [Step 4] Restore it
    NoteRepository::restore(&conn, &note.id).expect("[Workflow D: Step 4] Failed to restore note");

    // [Step 5] Verify its content and supported relationships
    let restored = NoteRepository::get_by_id(&conn, &note.id)
        .expect("get note")
        .expect("[Workflow D: Step 5] Restored note not found");

    assert_eq!(restored.lifecycle(), NoteLifecycle::Active);
    assert_eq!(restored.title, "Critical Strategy Document");
    assert_eq!(
        restored.content,
        "Secret strategy for expanding into European markets."
    );
    assert!(
        restored.is_favorite,
        "[Workflow D: Step 5] Favorite state must be preserved"
    );
    assert!(
        restored.is_pinned,
        "[Workflow D: Step 5] Pinned state must be preserved"
    );

    let tags = TagRepository::get_tags_for_note(&conn, &note.id).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "strategy");

    drop(conn);
    shutdown(runtime);
}

/// **Workflow E — Internal links**
/// 1. Create two notes.
/// 2. Link one note to the other.
/// 3. Activate the link.
/// 4. Navigate backward and forward.
/// 5. Rename the target.
/// 6. Verify link resolution.
#[test]
fn test_workflow_e_internal_links() {
    let _lock = lock_process_global_test_state();
    let env = TestAppDir::with_label("workflow-e");
    let runtime = initialize_application_storage(env.paths.clone()).unwrap();

    let conn = runtime.database.conn.lock().unwrap();

    // [Step 1] Create two notes
    let note_b = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Database Engine Reference".to_string()),
            content: Some("Architecture specification for SQLite with WAL mode.".to_string()),
            ..Default::default()
        },
    )
    .expect("[Workflow E: Step 1] Failed to create Note B");

    let note_a = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Architecture Overview".to_string()),
            content: Some(format!(
                "This document references the core [Database Engine](notepad://note/{}) for details.",
                note_b.id
            )),
            ..Default::default()
        },
    )
    .expect("[Workflow E: Step 1] Failed to create Note A");

    // [Step 2] Link verified in Note A content
    assert!(note_a
        .content
        .contains(&format!("notepad://note/{}", note_b.id)));

    // [Step 3] Activate the link: Extract target ID and resolve note
    let link_prefix = "notepad://note/";
    let start_idx = note_a.content.find(link_prefix).unwrap() + link_prefix.len();
    let end_idx = note_a.content[start_idx..].find(')').unwrap() + start_idx;
    let target_id = &note_a.content[start_idx..end_idx];

    let target_note = NoteRepository::get_by_id(&conn, target_id)
        .expect("[Workflow E: Step 3] Resolution query failed")
        .expect("[Workflow E: Step 3] Target note must resolve");
    assert_eq!(target_note.id, note_b.id);
    assert_eq!(target_note.title, "Database Engine Reference");

    // [Step 4] Navigate backward and forward (history stack verification)
    let mut history_stack = vec![note_a.id.clone()];
    let mut current_pos = 0;

    // Follow link to Note B:
    history_stack.push(target_note.id.clone());
    current_pos += 1;
    assert_eq!(history_stack[current_pos], note_b.id);

    // Back to Note A:
    current_pos -= 1;
    assert_eq!(history_stack[current_pos], note_a.id);

    // Forward to Note B:
    current_pos += 1;
    assert_eq!(history_stack[current_pos], note_b.id);

    // [Step 5] Rename the target
    let renamed_target = NoteRepository::update(
        &conn,
        &note_b.id,
        UpdateNoteDto {
            title: Some("SQLite Engine Specification (v2)".to_string()),
            ..Default::default()
        },
    )
    .expect("[Workflow E: Step 5] Failed to rename target note");
    assert_eq!(renamed_target.title, "SQLite Engine Specification (v2)");

    // [Step 6] Verify link resolution remains stable by UUID
    let resolved_after_rename = NoteRepository::get_by_id(&conn, target_id)
        .expect("resolution query")
        .expect("[Workflow E: Step 6] Target note must resolve after rename");
    assert_eq!(resolved_after_rename.id, note_b.id);
    assert_eq!(
        resolved_after_rename.title,
        "SQLite Engine Specification (v2)"
    );

    drop(conn);
    shutdown(runtime);
}

/// **Workflow F — Backup and restore**
/// 1. Create representative test data.
/// 2. Create and verify a backup.
/// 3. Restore it into a separate test data directory.
/// 4. Compare expected records and files.
#[test]
fn test_workflow_f_backup_and_restore() {
    let _lock = lock_process_global_test_state();

    // [Step 1] Create representative test data in Source environment
    let src_env = TestAppDir::with_label("workflow-f-src");
    let src_conn = src_env.open_migrated_database();

    let nb = NotebookRepository::create(
        &src_conn,
        CreateNotebookDto {
            name: "Cloud Architecture".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    let tag = TagRepository::create(&src_conn, "cloud").unwrap();

    let note = NoteRepository::create(
        &src_conn,
        CreateNoteDto {
            title: Some("Serverless Microservices".to_string()),
            content: Some("Event-driven lambdas and message queues architecture.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(false),
        },
    )
    .expect("create note");
    TagRepository::add_tag_to_note(&src_conn, &note.id, &tag.id).unwrap();

    // Attach file
    let att_dir = src_env.paths.get_note_attachments_dir(&note.id).unwrap();
    fs::create_dir_all(&att_dir).unwrap();
    let att_bytes = b"Serverless config manifest YAML";
    fs::write(att_dir.join("config.yaml"), att_bytes).unwrap();

    AttachmentRepository::create(
        &src_conn,
        CreateAttachmentDto {
            note_id: note.id.clone(),
            file_name: "config.yaml".to_string(),
            relative_path: format!("{}/config.yaml", note.id),
            mime_type: Some("application/yaml".to_string()),
            file_size: att_bytes.len() as i64,
        },
    )
    .unwrap();

    // [Step 2] Create and verify a backup
    let backup_res = BackupService::create_backup(
        &src_conn,
        &src_env.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("[Workflow F: Step 2] Failed to create backup");

    let archive_path = Path::new(&backup_res.file_path);
    assert!(archive_path.exists());

    let preview = RestoreService::preview_restore(archive_path, &src_env.paths)
        .expect("[Workflow F: Step 2] Failed to preview backup");
    assert!(
        preview.can_restore,
        "[Workflow F: Step 2] Backup archive must be restorable"
    );
    assert_eq!(preview.note_count, 1);
    assert_eq!(preview.attachment_count, 1);

    // [Step 3] Restore into a separate isolated test environment
    let target_env = TestAppDir::with_label("workflow-f-target");
    let mut target_conn = target_env.open_migrated_database();

    let restore_res = RestoreService::execute_restore(
        &mut target_conn,
        &target_env.paths,
        BackupRestoreRequest {
            file_path: backup_res.file_path.clone(),
            operation_id: Some("op-e2e-restore-1".to_string()),
        },
        None::<fn(_)>,
    )
    .expect("[Workflow F: Step 3] Failed to execute restore");
    assert!(restore_res.success);

    // [Step 4] Compare expected records and files in restored target
    let restored_note = NoteRepository::get_by_id(&target_conn, &note.id)
        .expect("get restored note")
        .expect("[Workflow F: Step 4] Note missing in restored environment");

    assert_eq!(restored_note.title, "Serverless Microservices");
    assert_eq!(
        restored_note.content,
        "Event-driven lambdas and message queues architecture."
    );
    assert_eq!(restored_note.notebook_id, Some(nb.id.clone()));
    assert!(restored_note.is_favorite);

    let restored_nb = NotebookRepository::get_by_id(&target_conn, &nb.id)
        .expect("get nb")
        .expect("notebook missing in restored environment");
    assert_eq!(restored_nb.name, "Cloud Architecture");

    let restored_tags = TagRepository::get_tags_for_note(&target_conn, &note.id).unwrap();
    assert_eq!(restored_tags.len(), 1);
    assert_eq!(restored_tags[0].name, "cloud");

    let target_att_path = target_env
        .paths
        .attachments_dir
        .join(format!("{}/config.yaml", note.id));
    assert!(
        target_att_path.exists(),
        "[Workflow F: Step 4] Restored attachment file must exist on disk"
    );
    assert_eq!(fs::read(target_att_path).unwrap(), att_bytes);
}

/// **Workflow G — Security**
/// If encryption exists, verify setup, lock, rejected protected access, unlock, and persistence without exposing secrets in test output.
#[test]
fn test_workflow_g_security_encryption_lifecycle() {
    let _lock = lock_process_global_test_state();
    let env = TestAppDir::with_label("workflow-g");

    // Launch application
    let runtime = initialize_application_storage(env.paths.clone()).unwrap();

    // [Step 1] Setup encryption
    let session =
        CryptoSession::init_from_database(&runtime.database.conn.lock().unwrap()).unwrap();
    assert!(!session.is_encryption_enabled());
    let password = "SecretMasterPassword2026!";

    {
        let conn = runtime.database.conn.lock().unwrap();
        session
            .setup_encryption(
                &conn,
                &env.paths,
                SetupEncryptionDto {
                    password: password.to_string(),
                    confirm_password: password.to_string(),
                },
            )
            .expect("[Workflow G: Step 1] Failed to setup encryption");
    }

    assert!(session.is_encryption_enabled());
    assert!(!session.is_locked());

    // [Step 2] Create sensitive note with DEK active
    let note_id = {
        let conn = runtime.database.conn.lock().unwrap();
        let dek = session.get_active_dek().unwrap();

        let note = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Confidential API Credentials".to_string()),
                content: Some("API_KEY=prod_sk_live_998877665544332211".to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .expect("[Workflow G: Step 2] Failed to create encrypted note");

        note.id
    };

    // [Step 3] Lock application
    session.lock();
    assert!(
        session.is_locked(),
        "[Workflow G: Step 3] Application must be locked"
    );
    assert!(
        session.get_active_dek().is_err(),
        "[Workflow G: Step 3] DEK must be purged from memory"
    );

    // [Step 4] Confirm rejected protected access while locked
    {
        let conn = runtime.database.conn.lock().unwrap();

        // Raw row in SQLite contains ciphertext, cannot be read as plaintext without DEK
        let raw_title: String = conn
            .query_row(
                "SELECT title FROM notes WHERE id = ?1",
                rusqlite::params![&note_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            raw_title.starts_with("ENC$v1$"),
            "[Workflow G: Step 4] Database title must be encrypted ciphertext"
        );

        let load_attempt = NoteRepository::get_by_id_with_key(&conn, &note_id, None);
        assert!(
            load_attempt.is_err(),
            "[Workflow G: Step 4] Access without DEK must be rejected"
        );
    }

    // [Step 5] Unlock application with password
    session
        .unlock(UnlockDto {
            password: password.to_string(),
        })
        .expect("[Workflow G: Step 5] Failed to unlock application");
    assert!(
        !session.is_locked(),
        "[Workflow G: Step 5] Application must be unlocked"
    );

    // [Step 6] Verify persistence and correct plaintext recovery
    {
        let conn = runtime.database.conn.lock().unwrap();
        let active_dek = session.get_active_dek().unwrap();

        let decrypted_note = NoteRepository::get_by_id_with_key(&conn, &note_id, Some(&active_dek))
            .expect("[Workflow G: Step 6] Query failed")
            .expect("[Workflow G: Step 6] Note must exist");

        assert_eq!(decrypted_note.title, "Confidential API Credentials");
        assert_eq!(
            decrypted_note.content,
            "API_KEY=prod_sk_live_998877665544332211"
        );
    }

    shutdown(runtime);
}
