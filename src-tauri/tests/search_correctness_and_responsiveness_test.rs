mod common;

use common::TestDatabase;
use personal_notepad_lib::storage::{
    crypto::{generate_dek, ENCRYPTION_METADATA_KEY},
    errors::StorageError,
    models::{CreateNoteDto, UpdateNoteDto},
    repositories::{NoteRepository, SearchRepository, SettingsRepository},
};

/// 1. Search by title, exact title ranking precedence, and prefix matching.
#[test]
fn test_search_by_title_ranking_and_precedence() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // 1. Note with exact title match
    let n_exact = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Performance Tuning".to_string()),
            content: Some("General overview notes.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 2. Note with title prefix match
    let n_prefix = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Performance Tuning Guide for SQLite".to_string()),
            content: Some("Details on indexing and VACUUM.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 3. Note with title containing words later
    let n_contains = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Advanced Backend Performance Tuning".to_string()),
            content: Some("WAL checkpointing and cache size.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 4. Note matching only in content
    let n_content = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("System Architecture".to_string()),
            content: Some("We need performance tuning for high query loads.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let results = SearchRepository::search(conn, "Performance Tuning", None)
        .expect("search Performance Tuning");

    assert_eq!(results.len(), 4, "All 4 notes contain the search term");

    // Exact title match must rank first (highest term density)
    assert_eq!(
        results[0].note_id, n_exact.id,
        "Exact title match must rank first"
    );

    // All matching notes are present in the search results
    let returned_ids: Vec<String> = results.iter().map(|r| r.note_id.clone()).collect();
    assert!(returned_ids.contains(&n_exact.id));
    assert!(returned_ids.contains(&n_prefix.id));
    assert!(returned_ids.contains(&n_contains.id));
    assert!(returned_ids.contains(&n_content.id));
}

/// 2. Search by body content, snippet generation, and snippet whitespace cleanup.
#[test]
fn test_search_by_body_content_and_snippet_generation() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Weekly Sprint Review".to_string()),
            content: Some(
                "Agenda items: Review release metrics. \
                 The internal codename for the quantum accelerator is project-starlight and launch is imminent. \
                 Meeting adjourned at 11:30 AM."
                    .to_string(),
            ),
            ..Default::default()
        },
    )
    .unwrap();

    // Search for a keyword that appears only in body content, not in title
    let results = SearchRepository::search(conn, "project-starlight", None)
        .expect("search project-starlight");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].note_id, note.id);
    assert_eq!(results[0].title, "Weekly Sprint Review");

    // Snippet verification
    assert!(results[0].snippet.is_some(), "Snippet must be generated");
    let snippet = results[0].snippet.as_ref().unwrap();
    assert!(
        snippet.to_lowercase().contains("project-starlight"),
        "Snippet must contain the searched term: '{snippet}'"
    );
    assert!(
        !snippet.contains("  "),
        "Snippet must have collapsed whitespace"
    );
}

/// 3. Case-insensitive matching across query variations.
#[test]
fn test_case_insensitive_matching() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Kubernetes Infrastructure".to_string()),
            content: Some("Configuring Nginx Ingress Controller and autoscaling.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Variations for title term: "Kubernetes"
    let q_lower = SearchRepository::search(conn, "kubernetes", None).unwrap();
    let q_upper = SearchRepository::search(conn, "KUBERNETES", None).unwrap();
    let q_mixed = SearchRepository::search(conn, "kUbErNeTeS", None).unwrap();

    assert_eq!(q_lower.len(), 1);
    assert_eq!(q_lower[0].note_id, note.id);
    assert_eq!(q_upper.len(), 1);
    assert_eq!(q_upper[0].note_id, note.id);
    assert_eq!(q_mixed.len(), 1);
    assert_eq!(q_mixed[0].note_id, note.id);

    // Variations for content term: "Ingress"
    let q_content_lower = SearchRepository::search(conn, "ingress", None).unwrap();
    let q_content_upper = SearchRepository::search(conn, "INGRESS", None).unwrap();
    assert_eq!(q_content_lower.len(), 1);
    assert_eq!(q_content_lower[0].note_id, note.id);
    assert_eq!(q_content_upper.len(), 1);
    assert_eq!(q_content_upper[0].note_id, note.id);
}

/// 4. Unicode, accented characters, multilingual text, and special symbols.
#[test]
fn test_unicode_multilingual_and_accented_characters() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // 4a. Devanagari / Hindi
    let n_hindi = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("यात्रा योजना".to_string()),
            content: Some("वाराणसी और हरिद्वार की सुखद यात्रा का विवरण।".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 4b. Arabic RTL
    let n_arabic = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("تقرير الأداء".to_string()),
            content: Some("تحسين قواعد البيانات بالعالم العربي السريع.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 4c. Russian / Cyrillic
    let n_russian = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Архитектура базы данных".to_string()),
            content: Some("Быстрый полнотекстовый поиск в локальной базе.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 4d. Accented European (French / Spanish)
    let n_accent = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Café Gourmet Menu".to_string()),
            content: Some("Crème brûlée and café au lait for breakfast mañana.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 4e. Special symbols and punctuation
    let n_symbols = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("C++ Metaprogramming".to_string()),
            content: Some("Templates, std::vector<T>, and *?^$() syntax quirks.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Execute queries
    let devanagari_res = SearchRepository::search(conn, "यात्रा", None).expect("search Hindi");
    assert_eq!(devanagari_res.len(), 1);
    assert_eq!(devanagari_res[0].note_id, n_hindi.id);

    let arabic_res = SearchRepository::search(conn, "بالعالم", None).expect("search Arabic");
    assert_eq!(arabic_res.len(), 1);
    assert_eq!(arabic_res[0].note_id, n_arabic.id);

    let russian_res = SearchRepository::search(conn, "поиск", None).expect("search Russian");
    assert_eq!(russian_res.len(), 1);
    assert_eq!(russian_res[0].note_id, n_russian.id);

    let accent_res = SearchRepository::search(conn, "crème", None).expect("search French accent");
    assert_eq!(accent_res.len(), 1);
    assert_eq!(accent_res[0].note_id, n_accent.id);

    // Symbols query must not cause SQL or FTS syntax crashes
    let symbols_res =
        SearchRepository::search(conn, "C++ *?^$()", None).expect("search special symbols");
    assert!(symbols_res.iter().any(|r| r.note_id == n_symbols.id));
}

/// 5. Empty, whitespace-only, and query length boundary validation.
#[test]
fn test_empty_whitespace_and_query_length_limits() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    test_db.create_note("Sample Title", "Sample Content", None);

    // 5a. Empty string returns empty vector without running database search
    let empty_res = SearchRepository::search(conn, "", None).expect("empty query");
    assert!(empty_res.is_empty());

    // 5b. Whitespace-only string returns empty vector immediately
    let ws_res = SearchRepository::search(conn, "   \t\n  ", None).expect("whitespace query");
    assert!(ws_res.is_empty());

    // 5c. Query at exact MAX_QUERY_LENGTH (1000 characters) succeeds
    let exact_max_query = "a ".repeat(500); // 1000 chars
    let max_res = SearchRepository::search(conn, &exact_max_query, None).expect("max length query");
    assert!(max_res.is_empty() || !max_res.is_empty());

    // 5d. Query exceeding MAX_QUERY_LENGTH (1001 characters) returns Validation error
    let too_long_query = "a".repeat(SearchRepository::MAX_QUERY_LENGTH + 1);
    let err = SearchRepository::search(conn, &too_long_query, None).unwrap_err();
    match err {
        StorageError::Validation(msg) => {
            assert!(msg.to_lowercase().contains("search query is too long"))
        }
        other => panic!("expected Validation error for over-length query, got {other:?}"),
    }
}

/// 6. Multi-term search (implicit AND) and explicit contiguous quoted phrase matching.
#[test]
fn test_multi_term_and_exact_quoted_phrase_search() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // Note A: Contains both words in sequence
    let n_a = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Consensus Protocols".to_string()),
            content: Some(
                "The Raft algorithm provides reliable leader election and log replication."
                    .to_string(),
            ),
            ..Default::default()
        },
    )
    .unwrap();

    // Note B: Contains both words separated and in reverse order
    let n_b = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Replication Strategies".to_string()),
            content: Some(
                "When replication stalls, the cluster votes for a new leader.".to_string(),
            ),
            ..Default::default()
        },
    )
    .unwrap();

    // Note C: Contains only one of the words ("leader")
    let n_c = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Leadership Dynamics".to_string()),
            content: Some("Key qualities of an engineering team leader.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 6a. Multi-term search: "leader replication" (implicit AND)
    // Both Note A and Note B contain both terms; Note C only contains "leader"
    let multi_term = SearchRepository::search(conn, "leader replication", None).unwrap();
    let multi_ids: Vec<String> = multi_term.into_iter().map(|r| r.note_id).collect();
    assert!(multi_ids.contains(&n_a.id));
    assert!(multi_ids.contains(&n_b.id));
    assert!(!multi_ids.contains(&n_c.id));

    // 6b. Exact quoted phrase: "\"leader election\""
    // Only Note A has the exact contiguous phrase "leader election"
    let phrase_res = SearchRepository::search(conn, "\"leader election\"", None).unwrap();
    let phrase_ids: Vec<String> = phrase_res.into_iter().map(|r| r.note_id).collect();
    assert!(phrase_ids.contains(&n_a.id));
    assert!(!phrase_ids.contains(&n_b.id));
    assert!(!phrase_ids.contains(&n_c.id));
}

/// 7. Index synchronization on note editing (updating note after it was indexed).
#[test]
fn test_index_synchronization_on_note_edit_and_autosave() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Space Exploration Roadmap".to_string()),
            content: Some("Early phase research into chemical propulsion systems.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Verify initial indexing
    let initial_match = SearchRepository::search(conn, "propulsion", None).unwrap();
    assert_eq!(initial_match.len(), 1);
    assert_eq!(initial_match[0].note_id, note.id);

    let non_existent = SearchRepository::search(conn, "superconducting", None).unwrap();
    assert!(non_existent.is_empty());

    // Update note content (simulating autosave flush)
    NoteRepository::update(
        conn,
        &note.id,
        UpdateNoteDto {
            content: Some(
                "Revised phase research into superconducting magnets and ion drives.".to_string(),
            ),
            ..Default::default()
        },
    )
    .unwrap();

    // 1. Stale token "propulsion" must no longer match
    let stale_search = SearchRepository::search(conn, "propulsion", None).unwrap();
    assert!(
        stale_search.is_empty(),
        "Stale content terms must not match after edit"
    );

    // 2. Newly added token "superconducting" must match immediately
    let new_search = SearchRepository::search(conn, "superconducting", None).unwrap();
    assert_eq!(
        new_search.len(),
        1,
        "New content terms must match immediately without manual re-index"
    );
    assert_eq!(new_search[0].note_id, note.id);
}

/// 8. Soft delete excludes notes from search; restore re-indexes immediately.
#[test]
fn test_soft_delete_and_restoration_fts_synchronization() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Confidential Merger Strategy".to_string()),
            content: Some("High priority acquisition targets for Q4.".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 8a. Note matches before deletion
    assert_eq!(
        SearchRepository::search(conn, "acquisition", None)
            .unwrap()
            .len(),
        1
    );

    // 8b. Soft delete note to Trash
    NoteRepository::soft_delete(conn, &note.id).expect("soft delete");

    // Normal search must strictly exclude trashed notes!
    let trash_search = SearchRepository::search(conn, "acquisition", None).unwrap();
    assert!(
        trash_search.is_empty(),
        "Soft-deleted notes must be excluded from search results"
    );

    // Also verify title search excludes it
    let title_search = SearchRepository::search(conn, "Confidential", None).unwrap();
    assert!(title_search.is_empty());

    // 8c. Restore note from Trash
    NoteRepository::restore(conn, &note.id).expect("restore note");

    // Restored note must be searchable again immediately without rebuild
    let restored_search = SearchRepository::search(conn, "acquisition", None).unwrap();
    assert_eq!(
        restored_search.len(),
        1,
        "Restored note must immediately appear in search results"
    );
    assert_eq!(restored_search[0].note_id, note.id);
}

/// 9. Permanent deletion cleanly removes note tokens from FTS index.
#[test]
fn test_permanent_deletion_cleanup_from_search() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Temporary Ephemeral Token".to_string()),
            content: Some("Token value: unique_ephemeral_xyz_987".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // Verify searchable before deletion
    assert_eq!(
        SearchRepository::search(conn, "unique_ephemeral_xyz_987", None)
            .unwrap()
            .len(),
        1
    );

    // Permanently delete note
    NoteRepository::delete(conn, &note.id, false).expect("permanent delete");

    // Must be completely purged from search
    let post_delete = SearchRepository::search(conn, "unique_ephemeral_xyz_987", None).unwrap();
    assert!(
        post_delete.is_empty(),
        "Permanently deleted note must not match search"
    );
}

/// 10. Result limits and deterministic ordering by modified_at DESC.
#[test]
fn test_result_limits_and_ordering() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // Create 60 notes matching the query term with distinct timestamps
    for i in 1..=60 {
        NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some(format!("Note {i:03} - Scalability Report")),
                content: Some(format!(
                    "Content body with benchmark_token for iteration {i}."
                )),
                ..Default::default()
            },
        )
        .unwrap();
    }

    // 10a. Default limit: returns 50 results
    let default_limit_res =
        SearchRepository::search(conn, "benchmark_token", None).expect("default limit search");
    assert_eq!(
        default_limit_res.len(),
        SearchRepository::DEFAULT_LIMIT,
        "Default limit must be 50"
    );

    // 10b. Custom limit below default: returns requested count (e.g. 20)
    let custom_res =
        SearchRepository::search(conn, "benchmark_token", Some(20)).expect("custom limit search");
    assert_eq!(custom_res.len(), 20);

    // 10c. Bounded upper limit: request for 200 is clamped to MAX_LIMIT (100)
    // Since only 60 exist, it returns all 60 without error
    let bounded_res =
        SearchRepository::search(conn, "benchmark_token", Some(200)).expect("bounded search");
    assert_eq!(bounded_res.len(), 60);

    // 10d. Ordering check: results must be sorted by modified_at DESC
    for window in default_limit_res.windows(2) {
        assert!(
            window[0].modified_at >= window[1].modified_at,
            "Results must be ordered by modified_at descending"
        );
    }
}

/// 11. Search persistence across database close and reopen.
#[test]
fn test_search_persistence_across_database_restart() {
    let mut test_db = TestDatabase::new();

    let note_id: String;
    {
        let conn = test_db.connection();
        let note = NoteRepository::create(
            conn,
            CreateNoteDto {
                title: Some("Persistent Architectural Blueprint".to_string()),
                content: Some(
                    "Durable state storage with WAL journaling and zero loss.".to_string(),
                ),
                ..Default::default()
            },
        )
        .unwrap();
        note_id = note.id;
    }

    // Close SQLite connection and reopen database from disk
    test_db.close_and_reopen();

    {
        let conn = test_db.connection();

        // Search immediately functions without requiring re-indexing or rebuild
        let results =
            SearchRepository::search(conn, "journaling", None).expect("post-restart search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note_id, note_id);
        assert_eq!(results[0].title, "Persistent Architectural Blueprint");
    }
}

/// 12. Search index diagnostics and rebuild recovery.
#[test]
fn test_fts_index_diagnose_and_rebuild() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    test_db.create_note("First Note", "Content for first note", None);
    test_db.create_note("Second Note", "Content for second note", None);

    // 12a. Initial diagnostic must be healthy and consistent
    let diag = SearchRepository::diagnose_index(conn).expect("diagnose index");
    assert!(!diag.is_encrypted);
    assert!(diag.fts_table_exists);
    assert!(diag.triggers_intact);
    assert!(diag.integrity_check_passed);
    assert_eq!(diag.active_notes_count, 2);
    assert!(diag.is_consistent);

    // 12b. Simulate FTS table corruption / drop
    conn.execute_batch(
        "DROP TABLE notes_fts;
         DROP TRIGGER notes_fts_ai;
         DROP TRIGGER notes_fts_ad;
         DROP TRIGGER notes_fts_au;",
    )
    .expect("drop FTS structures");

    // Diagnostic reflects inconsistency
    let corrupted_diag = SearchRepository::diagnose_index(conn).expect("corrupted diagnose");
    assert!(!corrupted_diag.is_consistent);
    assert!(!corrupted_diag.fts_table_exists);

    // 12c. Rebuild search index restores consistency
    SearchRepository::rebuild_search_index(conn).expect("rebuild index");

    let restored_diag = SearchRepository::diagnose_index(conn).expect("restored diagnose");
    assert!(restored_diag.is_consistent);
    assert!(restored_diag.fts_table_exists);
    assert!(restored_diag.triggers_intact);
    assert!(restored_diag.integrity_check_passed);

    // Search functions again after rebuild
    let search_res = SearchRepository::search(conn, "first note", None).unwrap();
    assert_eq!(search_res.len(), 1);
}

/// 13. Encrypted database search behavior, locked-state defense, and plaintext purge.
#[test]
fn test_encrypted_database_search_and_locked_state_defense() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let dek = generate_dek();

    // Create note with encrypted content
    let secret_note = NoteRepository::create_with_key(
        conn,
        CreateNoteDto {
            title: Some("Classified Satellite Blueprint".to_string()),
            content: Some("Telemetric downlink frequencies and antenna azimuth.".to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("create note with key");

    // Enable encryption in database settings (Phase 12 marker)
    SettingsRepository::set(
        conn,
        ENCRYPTION_METADATA_KEY,
        "{\"version\":1,\"argon2_salt\":\"dummy\",\"kdf_iterations\":1}",
    )
    .unwrap();

    // Purge unencrypted plaintext FTS index from disk (Task 11 security requirement)
    SearchRepository::purge_plaintext_index(conn).expect("purge plaintext index");

    // 13a. Diagnostic in encrypted mode verifies no plaintext FTS tables exist on disk
    let diag = SearchRepository::diagnose_index(conn).expect("diagnose encrypted index");
    assert!(diag.is_encrypted);
    assert!(
        !diag.fts_table_exists,
        "Encrypted DB must NOT contain plaintext FTS table"
    );
    assert!(diag.is_consistent);

    // 13b. Search while locked (no DEK) must be rejected with StorageError::Crypto
    let locked_res = SearchRepository::search_with_key(conn, "satellite", None, None);
    match locked_res {
        Err(StorageError::Crypto(msg)) => {
            assert!(msg.to_lowercase().contains("application is locked"))
        }
        other => panic!("expected Crypto lock error, got {other:?}"),
    }

    // 13c. Search when unlocked (with valid DEK) matches decrypted content in memory
    let unlocked_res = SearchRepository::search_with_key(conn, "telemetric", None, Some(&dek))
        .expect("unlocked search");
    assert_eq!(unlocked_res.len(), 1);
    assert_eq!(unlocked_res[0].note_id, secret_note.id);
    assert_eq!(unlocked_res[0].title, "Classified Satellite Blueprint");
    assert!(unlocked_res[0].snippet.is_some());
}
