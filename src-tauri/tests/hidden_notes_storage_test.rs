mod common;

use common::TestDatabase;
use personal_notepad_lib::storage::{
    models::{CreateNoteDto, LinkTargetLifecycle, ListNotesQueryOptions, UpdateNoteDto},
    repositories::{NoteRepository, SearchRepository, TagRepository},
    services::InternalLinkService,
};

#[test]
fn test_hidden_notes_set_and_toggle() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Confidential Note".to_string()),
            content: Some("Secret content".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    assert!(
        !note.is_hidden,
        "New note must default to is_hidden = false"
    );

    // 1. Explicitly set hidden = true
    let hidden_note = NoteRepository::set_hidden(conn, &note.id, true).expect("set hidden true");
    assert!(hidden_note.is_hidden);

    let fetched = NoteRepository::get_by_id(conn, &note.id)
        .expect("fetch note")
        .expect("must exist");
    assert!(fetched.is_hidden);

    // 2. Toggle hidden back to false
    let unhidden = NoteRepository::toggle_hidden(conn, &note.id).expect("toggle to false");
    assert!(!unhidden.is_hidden);

    // 3. Toggle hidden back to true
    let rehidden = NoteRepository::toggle_hidden(conn, &note.id).expect("toggle to true");
    assert!(rehidden.is_hidden);
}

#[test]
fn test_hidden_notes_excluded_from_lists_by_default() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let regular = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Public Note".to_string()),
            content: Some("Everyone can see this".to_string()),
            ..Default::default()
        },
    )
    .expect("create public note");

    let secret = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Private Diary".to_string()),
            content: Some("Hidden from sight".to_string()),
            ..Default::default()
        },
    )
    .expect("create private note");

    NoteRepository::set_hidden(conn, &secret.id, true).expect("mark secret note hidden");

    // List with default options (include_hidden = false)
    let default_list = NoteRepository::list_filtered_options(
        conn,
        &ListNotesQueryOptions {
            include_hidden: false,
            ..Default::default()
        },
        None,
    )
    .expect("list notes");

    assert_eq!(default_list.len(), 1);
    assert_eq!(default_list[0].id, regular.id);

    // List with include_hidden = true
    let full_list = NoteRepository::list_filtered_options(
        conn,
        &ListNotesQueryOptions {
            include_hidden: true,
            ..Default::default()
        },
        None,
    )
    .expect("list notes including hidden");

    assert_eq!(full_list.len(), 2);
    let ids: Vec<_> = full_list.iter().map(|n| n.id.as_str()).collect();
    assert!(ids.contains(&regular.id.as_str()));
    assert!(ids.contains(&secret.id.as_str()));
}

#[test]
fn test_hidden_notes_excluded_from_note_counts() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let _note1 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Visible 1".to_string()),
            ..Default::default()
        },
    )
    .expect("create note 1");

    let note2 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Hidden 1".to_string()),
            ..Default::default()
        },
    )
    .expect("create note 2");

    NoteRepository::set_hidden(conn, &note2.id, true).expect("mark note 2 hidden");

    // get_note_counts default (include_hidden = false)
    let counts_default = NoteRepository::get_note_counts(conn).expect("get counts default");
    assert_eq!(counts_default.total, 1);

    // get_note_counts_with_options(true)
    let counts_all = NoteRepository::get_note_counts_with_options(conn, true)
        .expect("get counts including hidden");
    assert_eq!(counts_all.total, 2);

    // Verify after unhiding note 2
    NoteRepository::set_hidden(conn, &note2.id, false).expect("unhide note 2");
    let counts_after = NoteRepository::get_note_counts(conn).expect("get counts after");
    assert_eq!(counts_after.total, 2);
}

#[test]
fn test_hidden_notes_excluded_from_search() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Top Secret Blueprint".to_string()),
            content: Some("Quantum cryptographic keys and passwords".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // Verify search finds it before being hidden
    let before_results = SearchRepository::search(conn, "cryptographic", None).expect("search");
    assert_eq!(before_results.len(), 1);
    assert_eq!(before_results[0].note_id, note.id);

    // Mark note as hidden
    NoteRepository::set_hidden(conn, &note.id, true).expect("hide note");

    // Search query should not reveal hidden note
    let after_results = SearchRepository::search(conn, "cryptographic", None).expect("search");
    assert_eq!(
        after_results.len(),
        0,
        "Hidden notes must not appear in search results"
    );

    let visible_results =
        SearchRepository::search_with_key_and_hidden(conn, "cryptographic", None, None, true)
            .expect("search including hidden");
    assert_eq!(visible_results.len(), 1);
    assert_eq!(visible_results[0].note_id, note.id);
}

#[test]
fn test_hidden_trash_items_and_counts_follow_visibility_setting() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let visible = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Visible trash item".to_string()),
            ..Default::default()
        },
    )
    .expect("create visible note");
    let hidden = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Hidden trash item".to_string()),
            ..Default::default()
        },
    )
    .expect("create hidden note");
    NoteRepository::set_hidden(conn, &hidden.id, true).expect("hide note");
    NoteRepository::soft_delete(conn, &visible.id).expect("trash visible note");
    NoteRepository::soft_delete(conn, &hidden.id).expect("trash hidden note");

    let filtered =
        NoteRepository::list_trashed_items_page_with_options_and_key(conn, None, None, false, None)
            .expect("filtered trash");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, visible.id);
    assert!(!filtered[0].is_hidden);

    let all =
        NoteRepository::list_trashed_items_page_with_options_and_key(conn, None, None, true, None)
            .expect("all trash");
    assert_eq!(all.len(), 2);
    assert!(all
        .iter()
        .any(|item| item.id == hidden.id && item.is_hidden));
    assert_eq!(
        NoteRepository::get_note_counts_with_options(conn, false)
            .expect("filtered counts")
            .trash,
        1
    );
    assert_eq!(
        NoteRepository::get_note_counts_with_options(conn, true)
            .expect("all counts")
            .trash,
        2
    );
}

#[test]
fn test_update_note_dto_supports_is_hidden() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Regular Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    assert!(!note.is_hidden);

    let updated = NoteRepository::update(
        conn,
        &note.id,
        UpdateNoteDto {
            is_hidden: Some(true),
            ..Default::default()
        },
    )
    .expect("update note");

    assert!(updated.is_hidden);
}

#[test]
fn test_hidden_notes_favorite_and_tag_filtering() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    // 1. Test Favorites
    let fav_note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Favorite Note".to_string()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("create favorite note");

    let fav_hidden = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Secret Favorite Note".to_string()),
            is_favorite: Some(true),
            ..Default::default()
        },
    )
    .expect("create secret favorite note");

    NoteRepository::set_hidden(conn, &fav_hidden.id, true).expect("hide secret favorite note");

    let favs_without_hidden = NoteRepository::list_favorites_page_with_options_and_hidden(
        conn, None, None, None, false, None,
    )
    .expect("list favorites excluding hidden");
    assert_eq!(favs_without_hidden.len(), 1);
    assert_eq!(favs_without_hidden[0].id, fav_note.id);

    let favs_with_hidden = NoteRepository::list_favorites_page_with_options_and_hidden(
        conn, None, None, None, true, None,
    )
    .expect("list favorites including hidden");
    assert_eq!(favs_with_hidden.len(), 2);

    // 2. Test Tags
    let tag =
        personal_notepad_lib::storage::repositories::TagRepository::create(conn, "confidential")
            .expect("create tag");

    let tagged_visible = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Tagged Visible".to_string()),
            ..Default::default()
        },
    )
    .expect("create tagged visible");
    personal_notepad_lib::storage::repositories::TagRepository::add_tag_to_note(
        conn,
        &tagged_visible.id,
        &tag.id,
    )
    .expect("add tag to visible note");

    let tagged_hidden = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Tagged Hidden".to_string()),
            ..Default::default()
        },
    )
    .expect("create tagged hidden");
    personal_notepad_lib::storage::repositories::TagRepository::add_tag_to_note(
        conn,
        &tagged_hidden.id,
        &tag.id,
    )
    .expect("add tag to hidden note");
    NoteRepository::set_hidden(conn, &tagged_hidden.id, true).expect("hide tagged note");

    let tag_notes_no_hidden = NoteRepository::list_by_tag_page_with_options_and_hidden(
        conn, &tag.id, None, None, None, false, None,
    )
    .expect("list by tag excluding hidden");
    assert_eq!(tag_notes_no_hidden.len(), 1);
    assert_eq!(tag_notes_no_hidden[0].id, tagged_visible.id);

    let tag_notes_with_hidden = NoteRepository::list_by_tag_page_with_options_and_hidden(
        conn, &tag.id, None, None, None, true, None,
    )
    .expect("list by tag including hidden");
    assert_eq!(tag_notes_with_hidden.len(), 2);

    let filtered_counts = TagRepository::get_tag_note_counts_with_options(conn, false)
        .expect("tag counts excluding hidden");
    assert_eq!(filtered_counts.get(&tag.id), Some(&1));
    let all_counts = TagRepository::get_tag_note_counts_with_options(conn, true)
        .expect("tag counts including hidden");
    assert_eq!(all_counts.get(&tag.id), Some(&2));

    let filtered_names = TagRepository::get_all_notes_tag_names_with_options(conn, false)
        .expect("tag names excluding hidden");
    assert!(filtered_names.contains_key(&tagged_visible.id));
    assert!(!filtered_names.contains_key(&tagged_hidden.id));
    let all_names = TagRepository::get_all_notes_tag_names_with_options(conn, true)
        .expect("tag names including hidden");
    assert!(all_names.contains_key(&tagged_hidden.id));
}

#[test]
fn test_hidden_internal_link_targets_and_backlinks_follow_visibility_setting() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let target = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Hidden target".to_string()),
            ..Default::default()
        },
    )
    .expect("create target");
    NoteRepository::set_hidden(conn, &target.id, true).expect("hide target");
    let source = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Hidden source".to_string()),
            content: Some(format!("See [target](notepad://note/{})", target.id)),
            ..Default::default()
        },
    )
    .expect("create source");
    NoteRepository::set_hidden(conn, &source.id, true).expect("hide source");

    let filtered_target = InternalLinkService::resolve_link_target_with_options_and_key(
        conn, &target.id, false, None,
    )
    .expect("resolve target with hidden notes filtered");
    assert_eq!(filtered_target.lifecycle, LinkTargetLifecycle::Missing);
    let visible_target =
        InternalLinkService::resolve_link_target_with_options_and_key(conn, &target.id, true, None)
            .expect("resolve target including hidden notes");
    assert_eq!(visible_target.lifecycle, LinkTargetLifecycle::Active);

    let filtered_backlinks =
        InternalLinkService::get_backlinks_with_options_and_key(conn, &target.id, false, None)
            .expect("backlinks with hidden notes filtered");
    assert!(filtered_backlinks.is_empty());
    let all_backlinks =
        InternalLinkService::get_backlinks_with_options_and_key(conn, &target.id, true, None)
            .expect("backlinks including hidden notes");
    assert_eq!(all_backlinks.len(), 1);
    assert_eq!(all_backlinks[0].source_note_id, source.id);
}
