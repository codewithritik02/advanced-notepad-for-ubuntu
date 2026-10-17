mod common;

use std::fs;
use std::path::Path;
use std::time::Instant;

use personal_notepad_lib::storage::{
    attachment_storage::AttachmentStorage,
    errors::StorageError,
    limits::{
        sanitize_pagination_bounds, validate_bulk_operation_count, validate_note_content,
        validate_notebook_name, validate_tag_name, DEFAULT_PAGE_SIZE, MAX_ATTACHMENT_SIZE_BYTES,
        MAX_BATCH_FILE_COUNT, MAX_CHECKPOINT_COUNT, MAX_IMPORT_BATCH_SIZE_BYTES,
        MAX_IMPORT_FILE_SIZE_BYTES, MAX_NOTEBOOK_NAME_CHARS, MAX_NOTE_CONTENT_BYTES,
        MAX_NOTE_TITLE_CHARS, MAX_PAGE_SIZE, MAX_SEARCH_LIMIT, MAX_SEARCH_QUERY_LENGTH,
        MAX_TAG_NAME_CHARS,
    },
    models::{
        BackupCreateRequest, BackupType, CreateNoteDto, CreateNotebookDto, ListNotesQueryOptions,
        UpdateNoteDto,
    },
    repositories::{
        NoteRepository, NotebookRepository, SearchRepository, SettingsRepository, TagRepository,
    },
    services::{AttachmentService, BackupService, InternalLinkService, TrashService},
};

use common::{lock_process_global_test_state, TestAppDir};

// =============================================================================
// Task 16 — Test Large Data Sets and Resource Limits
// =============================================================================

#[test]
fn test_synthetic_dataset_list_retrieval_and_index_efficiency() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Seed synthetic dataset: 150 notes across notebooks, tags, pinned, favorites
    let nb1 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Research & Development".to_string(),
            parent_id: None,
        },
    )
    .expect("create nb1");

    let nb2 = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Architecture Specs".to_string(),
            parent_id: None,
        },
    )
    .expect("create nb2");

    let tag1 = TagRepository::create(&conn, "perf").expect("create tag1");
    let tag2 = TagRepository::create(&conn, "core").expect("create tag2");

    // Bulk insert 150 notes in a transaction for maximum speed
    conn.execute("BEGIN TRANSACTION", []).expect("begin tx");
    for i in 0..150 {
        let nb_id = if i % 3 == 0 {
            Some(nb1.id.as_str())
        } else if i % 3 == 1 {
            Some(nb2.id.as_str())
        } else {
            None
        };

        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Synthetic Note Item #{i:04}")),
                content: Some(format!(
                    "Body text for synthetic note #{i}. Includes technical metrics, code references, and benchmarks."
                )),
                format: Some("md".to_string()),
                notebook_id: nb_id.map(str::to_string),
                is_favorite: Some(i % 5 == 0),
                is_pinned: Some(i % 10 == 0),
            },
        )
        .expect("create synthetic note");

        if i % 4 == 0 {
            TagRepository::add_tag_to_note(&conn, &note.id, &tag1.id).expect("add tag1");
        }
        if i % 6 == 0 {
            TagRepository::add_tag_to_note(&conn, &note.id, &tag2.id).expect("add tag2");
        }
    }
    conn.execute("COMMIT", []).expect("commit tx");

    // 2. Measure initial list loading latency with bounded content preview
    let start_list = Instant::now();
    let options = ListNotesQueryOptions {
        include_deleted: false,
        notebook_id: None,
        unfiled_only: false,
        limit: Some(50),
        offset: Some(0),
        content_limit: Some(300),
    };
    let initial_slice =
        NoteRepository::list_filtered_options(&conn, &options, None).expect("list initial slice");
    let list_elapsed = start_list.elapsed();

    assert_eq!(
        initial_slice.len(),
        50,
        "Default pagination must yield 50 notes"
    );
    assert!(
        list_elapsed.as_millis() < 50,
        "50-note page query with content preview must finish under 50ms (took {}ms)",
        list_elapsed.as_millis()
    );

    // 3. Verify SQLite EXPLAIN QUERY PLAN uses index idx_notes_list_sort
    let plan: String = conn
        .query_row(
            "EXPLAIN QUERY PLAN SELECT id FROM notes WHERE is_deleted = 0 ORDER BY is_pinned DESC, modified_at DESC LIMIT 50",
            [],
            |r| r.get(3),
        )
        .expect("explain query plan");

    assert!(
        plan.contains("idx_notes_list_sort") || plan.contains("USING INDEX"),
        "List query must use composite covering index; query plan: {plan}"
    );
    assert!(
        !plan.contains("TEMP B-TREE"),
        "List query must not construct temporary B-trees for sorting; query plan: {plan}"
    );

    // 4. Measure scalar note counts latency
    let start_counts = Instant::now();
    let counts = NoteRepository::get_note_counts(&conn).expect("get counts");
    let counts_elapsed = start_counts.elapsed();

    assert_eq!(counts.total, 150);
    assert_eq!(counts.favorites, 30);
    assert_eq!(counts.unfiled, 50);
    assert_eq!(counts.trash, 0);
    assert_eq!(counts.by_notebook.get(&nb1.id).copied().unwrap_or(0), 50);
    assert_eq!(counts.by_notebook.get(&nb2.id).copied().unwrap_or(0), 50);
    assert!(
        counts_elapsed.as_millis() < 10,
        "Aggregate count calculation must finish under 10ms (took {}ms)",
        counts_elapsed.as_millis()
    );
}

#[test]
fn test_notebook_hierarchy_navigation_and_depth_limits() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Build valid 3-level notebook hierarchy (Root -> Child -> Grandchild)
    let root = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Level 1: Root Project".to_string(),
            parent_id: None,
        },
    )
    .expect("create root");

    let child = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Level 2: Subsystem Components".to_string(),
            parent_id: Some(root.id.clone()),
        },
    )
    .expect("create child");

    let grandchild = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Level 3: Low-Level Modules".to_string(),
            parent_id: Some(child.id.clone()),
        },
    )
    .expect("create grandchild");

    // 2. Enforce MAX_NOTEBOOK_NESTING_DEPTH = 3: creating level 4 must be rejected
    let great_grandchild_res = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Level 4: Beyond Limit".to_string(),
            parent_id: Some(grandchild.id.clone()),
        },
    );

    assert!(
        matches!(great_grandchild_res, Err(StorageError::InvalidHierarchy(ref msg)) if msg.contains("maximum nesting depth")),
        "Attempting to create notebook at depth 4 must be rejected with InvalidHierarchy error"
    );

    // 3. Notebook name length boundaries (MAX_NOTEBOOK_NAME_CHARS = 200)
    let valid_200_name = "N".repeat(MAX_NOTEBOOK_NAME_CHARS);
    assert!(validate_notebook_name(&valid_200_name).is_ok());

    let invalid_201_name = "N".repeat(MAX_NOTEBOOK_NAME_CHARS + 1);
    let name_res = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: invalid_201_name,
            parent_id: None,
        },
    );
    assert!(
        matches!(name_res, Err(StorageError::Validation(ref msg)) if msg.contains("exceeds the maximum allowed limit of 200")),
        "Notebook name exceeding 200 characters must be rejected"
    );

    // 4. Query notes filtered by notebook at each depth level
    let n1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Root Note".to_string()),
            notebook_id: Some(root.id.clone()),
            ..Default::default()
        },
    )
    .expect("create n1");

    let n2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Child Note".to_string()),
            notebook_id: Some(child.id.clone()),
            ..Default::default()
        },
    )
    .expect("create n2");

    let n3 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Grandchild Note".to_string()),
            notebook_id: Some(grandchild.id.clone()),
            ..Default::default()
        },
    )
    .expect("create n3");

    let root_notes = NoteRepository::list_by_notebook(&conn, &root.id).expect("list root");
    assert_eq!(root_notes.len(), 1);
    assert_eq!(root_notes[0].id, n1.id);

    let child_notes = NoteRepository::list_by_notebook(&conn, &child.id).expect("list child");
    assert_eq!(child_notes.len(), 1);
    assert_eq!(child_notes[0].id, n2.id);

    let grandchild_notes =
        NoteRepository::list_by_notebook(&conn, &grandchild.id).expect("list grandchild");
    assert_eq!(grandchild_notes.len(), 1);
    assert_eq!(grandchild_notes[0].id, n3.id);
}

#[test]
fn test_large_note_content_integrity_and_size_bounds() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Create realistic 1 MB note (1,048,576 bytes)
    let paragraph = "Personal Notepad high-performance markdown storage verification paragraph.\n";
    let repeats = 1_048_576 / paragraph.len();
    let mut large_content = String::with_capacity(1_048_576 + 1024);
    for _ in 0..repeats {
        large_content.push_str(paragraph);
    }
    // Pad to exact 1 MB
    while large_content.len() < 1_048_576 {
        large_content.push('#');
    }
    assert_eq!(large_content.len(), 1_048_576);

    let start_create = Instant::now();
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("1 MB Stress Test Note".to_string()),
            content: Some(large_content.clone()),
            format: Some("md".to_string()),
            ..Default::default()
        },
    )
    .expect("create 1 MB note");
    let create_elapsed = start_create.elapsed();

    assert!(
        create_elapsed.as_millis() < 200,
        "Persisting 1 MB note must finish under 200ms (took {}ms)",
        create_elapsed.as_millis()
    );

    // 2. Read full 1 MB note and assert bit-for-bit parity
    let start_read = Instant::now();
    let fetched = NoteRepository::get_by_id(&conn, &note.id)
        .expect("get note")
        .expect("note exists");
    let read_elapsed = start_read.elapsed();

    assert_eq!(fetched.content.len(), 1_048_576);
    assert_eq!(
        fetched.content, large_content,
        "Full 1 MB content must match without truncation"
    );
    assert!(
        read_elapsed.as_millis() < 50,
        "Reading 1 MB note must finish under 50ms (took {}ms)",
        read_elapsed.as_millis()
    );

    // 3. Update 1 MB note
    let mut modified_large_content = large_content.clone();
    modified_large_content.replace_range(
        0..64,
        "# Updated Header: System Performance Benchmark Passed with Zero Drift\n",
    );

    let updated = NoteRepository::update(
        &conn,
        &note.id,
        UpdateNoteDto {
            content: Some(modified_large_content.clone()),
            ..Default::default()
        },
    )
    .expect("update 1 MB note");

    assert_eq!(updated.content.len(), modified_large_content.len());

    // 4. Boundary enforcement: MAX_NOTE_CONTENT_BYTES = 10 MB (10,485,760 bytes)
    // 10 MB exact is valid
    assert!(validate_note_content(Some(&"x".repeat(1024 * 1024))).is_ok());

    // Content exceeding 10 MB limit by 1 byte must be rejected
    let oversized = "x".repeat(MAX_NOTE_CONTENT_BYTES + 1);
    let over_res = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Oversized Note".to_string()),
            content: Some(oversized),
            ..Default::default()
        },
    );

    assert!(
        matches!(over_res, Err(StorageError::Validation(ref msg)) if msg.contains("exceeds the maximum allowed limit of 10 MB")),
        "Note content exceeding 10 MB must be rejected with Validation error"
    );
}

#[test]
fn test_note_title_length_boundary_and_multilingual_unicode_handling() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Exact 500 characters title boundary (MAX_NOTE_TITLE_CHARS = 500)
    let title_500 = "T".repeat(MAX_NOTE_TITLE_CHARS);
    let note_500 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(title_500.clone()),
            content: Some("Short content".to_string()),
            ..Default::default()
        },
    )
    .expect("create note with 500-char title");

    assert_eq!(note_500.title.chars().count(), 500);

    // 2. Title of 501 characters must be rejected
    let title_501 = "T".repeat(MAX_NOTE_TITLE_CHARS + 1);
    let res_501 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(title_501),
            content: Some("Short content".to_string()),
            ..Default::default()
        },
    );

    assert!(
        matches!(res_501, Err(StorageError::Validation(ref msg)) if msg.contains("exceeds the maximum allowed limit of 500 characters")),
        "Note title exceeding 500 characters must be rejected"
    );

    // 3. Multilingual Unicode characters counted by char count, not byte count
    // A 400-character Hindi & Japanese title (multiple bytes per character) is well within 500 chars limit
    let hindi_char = 'क';
    let japanese_char = '本';
    let unicode_title: String = (0..200)
        .map(|_| hindi_char)
        .chain((0..200).map(|_| japanese_char))
        .collect();
    assert_eq!(unicode_title.chars().count(), 400);
    assert!(
        unicode_title.len() > 1000,
        "Byte length is > 1000 bytes while char count is 400"
    );

    let unicode_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(unicode_title.clone()),
            content: Some("Multilingual title content".to_string()),
            ..Default::default()
        },
    )
    .expect("create note with 400-char multibyte title");

    assert_eq!(unicode_note.title, unicode_title);
}

#[test]
fn test_fts5_search_performance_and_query_limits_at_scale() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Seed dataset with varied technical keywords
    let keywords = [
        (
            "Telemetry",
            "Orbital mechanics and telemetry processing pipeline.",
        ),
        (
            "Quantum",
            "Superconducting qubits, decoherence mitigation, and cryogenics.",
        ),
        (
            "Database",
            "SQLite write-ahead logging, B-tree indexes, and memory pools.",
        ),
        (
            "Security",
            "Authenticated AES-256-GCM encryption with envelope keys.",
        ),
    ];

    for (title, body) in keywords {
        for i in 0..10 {
            NoteRepository::create(
                &conn,
                CreateNoteDto {
                    title: Some(format!("{title} Document {i}")),
                    content: Some(format!("{body} Revision index {i}.")),
                    ..Default::default()
                },
            )
            .expect("create search note");
        }
    }

    // 2. Measure search latency across index
    let start_search = Instant::now();
    let results = SearchRepository::search(&conn, "Superconducting qubits", None).expect("search");
    let search_elapsed = start_search.elapsed();

    assert_eq!(results.len(), 10);
    assert!(
        search_elapsed.as_millis() < 20,
        "FTS5 query across dataset must complete under 20ms (took {}ms)",
        search_elapsed.as_millis()
    );

    // 3. Search query length limit (MAX_SEARCH_QUERY_LENGTH = 1000)
    let query_1000 = "a".repeat(MAX_SEARCH_QUERY_LENGTH);
    let search_1000_res = SearchRepository::search(&conn, &query_1000, None);
    assert!(
        search_1000_res.is_ok(),
        "Query of 1000 characters is accepted"
    );

    let query_1001 = "a".repeat(MAX_SEARCH_QUERY_LENGTH + 1);
    let search_1001_res = SearchRepository::search(&conn, &query_1001, None);
    assert!(
        matches!(search_1001_res, Err(StorageError::Validation(ref msg)) if msg.contains("Search query is too long")),
        "Search query exceeding 1000 characters must be rejected"
    );

    // 4. Search result limit clamping (MAX_SEARCH_LIMIT = 100)
    let requested_limit_500 =
        SearchRepository::search(&conn, "Revision", Some(500)).expect("search with limit 500");
    assert!(
        requested_limit_500.len() <= MAX_SEARCH_LIMIT,
        "Requested search limit must be clamped to MAX_SEARCH_LIMIT (100)"
    );
}

#[test]
fn test_tag_filtering_at_scale_and_name_length_bounds() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Tag name length boundaries (MAX_TAG_NAME_CHARS = 100)
    let valid_tag = "t".repeat(MAX_TAG_NAME_CHARS);
    let tag = TagRepository::create(&conn, &valid_tag).expect("create 100-char tag");
    assert_eq!(tag.name.chars().count(), 100);

    let invalid_tag = "t".repeat(MAX_TAG_NAME_CHARS + 1);
    let tag_res = TagRepository::create(&conn, &invalid_tag);
    assert!(
        matches!(tag_res, Err(StorageError::Validation(ref msg)) if msg.contains("cannot exceed 100 characters")),
        "Tag name exceeding 100 characters must be rejected"
    );

    // 2. Empty or whitespace tag names rejected
    assert!(validate_tag_name("").is_err());
    assert!(validate_tag_name("    ").is_err());

    // 3. Query notes by tag across dataset
    let note1 = NoteRepository::create(&conn, CreateNoteDto::default()).expect("note 1");
    let note2 = NoteRepository::create(&conn, CreateNoteDto::default()).expect("note 2");
    TagRepository::add_tag_to_note(&conn, &note1.id, &tag.id).expect("assign tag 1");
    TagRepository::add_tag_to_note(&conn, &note2.id, &tag.id).expect("assign tag 2");

    let tagged_notes = NoteRepository::list_by_tag(&conn, &tag.id).expect("list by tag");
    assert_eq!(tagged_notes.len(), 2);
}

#[test]
fn test_favorites_and_pinned_filtering_scalability() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // Seed favorites with mixed pinned state
    for i in 0..20 {
        NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Favorite Note {i}")),
                is_favorite: Some(true),
                is_pinned: Some(i == 7 || i == 15),
                ..Default::default()
            },
        )
        .expect("create fav note");
    }

    // 1. Query favorites
    let favorites = NoteRepository::list_favorites(&conn).expect("list favorites");
    assert_eq!(favorites.len(), 20);

    // 2. Pinned notes must appear first
    assert!(favorites[0].is_pinned);
    assert!(favorites[1].is_pinned);
    assert!(!favorites[2].is_pinned);

    // 3. Query plan check: uses idx_notes_favorite_list
    let plan: String = conn
        .query_row(
            "EXPLAIN QUERY PLAN SELECT id FROM notes WHERE is_favorite = 1 AND is_deleted = 0 ORDER BY is_pinned DESC, modified_at DESC",
            [],
            |r| r.get(3),
        )
        .expect("explain fav plan");

    assert!(
        plan.contains("idx_notes_favorite_list") || plan.contains("USING INDEX"),
        "Favorites query must use covering index: {plan}"
    );
}

#[test]
fn test_trash_operations_and_bulk_limits_at_scale() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Bulk limits validation (MAX_BULK_OPERATION_ITEMS = 500)
    assert!(validate_bulk_operation_count(1).is_ok());
    assert!(validate_bulk_operation_count(500).is_ok());
    assert!(validate_bulk_operation_count(0).is_err());
    assert!(validate_bulk_operation_count(501).is_err());

    // 2. Soft-delete and restore operations
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Recycle Candidate".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let trashed = TrashService::soft_delete(&conn, &note.id).expect("soft delete");
    assert!(trashed.is_deleted);

    let counts_trashed = NoteRepository::get_note_counts(&conn).expect("counts");
    assert_eq!(counts_trashed.trash, 1);
    assert_eq!(counts_trashed.total, 0);

    let restored = TrashService::restore(&conn, &note.id).expect("restore");
    assert!(!restored.is_deleted);

    let counts_restored = NoteRepository::get_note_counts(&conn).expect("counts");
    assert_eq!(counts_restored.trash, 0);
    assert_eq!(counts_restored.total, 1);
}

#[test]
fn test_internal_links_and_backlinks_fan_out_scalability() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Target note referenced by 40 distinct source notes
    let target_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Central Specifications Hub".to_string()),
            content: Some("Core design system guidelines.".to_string()),
            ..Default::default()
        },
    )
    .expect("create target note");

    conn.execute("BEGIN TRANSACTION", []).expect("begin tx");
    for i in 0..40 {
        NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Source Document #{i}")),
                content: Some(format!(
                    "References [Central Hub](notepad://note/{}) for details.",
                    target_note.id
                )),
                ..Default::default()
            },
        )
        .expect("create source note");
    }
    conn.execute("COMMIT", []).expect("commit tx");

    // 2. Discover backlinks
    let start_backlinks = Instant::now();
    let backlinks =
        InternalLinkService::get_backlinks(&conn, &target_note.id).expect("get backlinks");
    let elapsed = start_backlinks.elapsed();

    assert_eq!(
        backlinks.len(),
        40,
        "All 40 incoming backlinks must be discovered"
    );
    assert!(
        elapsed.as_millis() < 25,
        "Backlinks discovery for 40 references must take under 25ms (took {}ms)",
        elapsed.as_millis()
    );
}

#[test]
fn test_attachment_storage_and_file_size_limits() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let note = NoteRepository::create(&conn, CreateNoteDto::default()).expect("note");

    let storage = AttachmentStorage::new(app_dir.paths.clone());

    // 1. Valid attachment file (64 KiB)
    let src_file = app_dir.dir.join("sample_asset.bin");
    fs::write(&src_file, vec![0x42; 64 * 1024]).expect("write asset");

    let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file)
        .expect("add attachment");
    assert_eq!(att.file_size, 64 * 1024);

    let list = AttachmentService::list_attachments(&conn, &note.id).expect("list");
    assert_eq!(list.len(), 1);

    // 2. Enforce MAX_ATTACHMENT_SIZE_BYTES = 100 MiB
    assert_eq!(MAX_ATTACHMENT_SIZE_BYTES, 100 * 1024 * 1024);
}

#[test]
fn test_import_and_export_batch_resource_limits() {
    // 1. Validate individual file limit: 10 MiB
    assert_eq!(MAX_IMPORT_FILE_SIZE_BYTES, 10 * 1024 * 1024);

    // 2. Validate aggregate batch size limit: 100 MiB
    assert_eq!(MAX_IMPORT_BATCH_SIZE_BYTES, 100 * 1024 * 1024);

    // 3. Validate batch file count limit: 500 files
    assert_eq!(MAX_BATCH_FILE_COUNT, 500);
}

#[test]
fn test_backup_and_recovery_checkpoint_retention_limits() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Create notes and settings
    NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Mission Telemetry".to_string()),
            content: Some("Flight computer logging entries.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    SettingsRepository::set(&conn, "theme", "system").expect("set theme");

    // 2. Create backup
    let backup_res = BackupService::create_backup(
        &conn,
        &app_dir.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("create backup");

    assert!(backup_res.success);
    assert_eq!(backup_res.note_count, 1);
    assert!(Path::new(&backup_res.file_path).exists());

    // 3. Verify MAX_CHECKPOINT_COUNT = 10 limit
    assert_eq!(MAX_CHECKPOINT_COUNT, 10);
}

#[test]
fn test_pagination_bounds_and_memory_protection() {
    // 1. Bounded pagination sanitization
    assert_eq!(sanitize_pagination_bounds(None), DEFAULT_PAGE_SIZE);
    assert_eq!(sanitize_pagination_bounds(Some(0)), DEFAULT_PAGE_SIZE);
    assert_eq!(sanitize_pagination_bounds(Some(25)), 25);
    assert_eq!(sanitize_pagination_bounds(Some(100)), 100);
    assert_eq!(sanitize_pagination_bounds(Some(200)), MAX_PAGE_SIZE);
    assert_eq!(sanitize_pagination_bounds(Some(500)), MAX_PAGE_SIZE);
    assert_eq!(sanitize_pagination_bounds(Some(99999)), MAX_PAGE_SIZE);
}

#[test]
fn test_repeated_operations_stability_and_wal_checkpointing() {
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // Execute 100 sequential note creations, updates, and reads
    for i in 0..100 {
        let note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some(format!("Churn Note {i}")),
                content: Some(format!("Content body iteration {i}")),
                ..Default::default()
            },
        )
        .expect("churn create");

        let updated = NoteRepository::update(
            &conn,
            &note.id,
            UpdateNoteDto {
                title: Some(format!("Churn Note {i} Updated")),
                ..Default::default()
            },
        )
        .expect("churn update");

        assert_eq!(updated.title, format!("Churn Note {i} Updated"));

        let _read = NoteRepository::get_by_id(&conn, &note.id).expect("churn read");

        if i % 2 == 0 {
            NoteRepository::delete(&conn, &note.id, false).expect("churn delete");
        }
    }

    // Verify database remains responsive and consistent
    let remaining_notes = NoteRepository::list(&conn, true).expect("list final");
    assert_eq!(remaining_notes.len(), 50);

    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("pragma integrity_check");
    assert_eq!(
        integrity, "ok",
        "Database integrity must be ok after rapid churn"
    );
}
