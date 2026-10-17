mod common;

use common::TestDatabase;
use personal_notepad_lib::storage::{
    crypto::generate_dek,
    errors::AttachmentError,
    limits::MAX_ATTACHMENT_SIZE_BYTES,
    models::{AttachmentConsistencyReport, CreateNoteDto},
    repositories::{AttachmentRepository, NoteRepository},
    services::{AttachmentService, TrashService},
    AttachmentStorage,
};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;

fn compute_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// 1. Attaching representative supported file types: PNG, JPEG, PDF, TXT, Markdown, JSON.
#[test]
fn test_attachment_storage_supported_file_types() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Note with Diverse Attachments".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let source_dir = test_db.root().join("source_files");
    fs::create_dir_all(&source_dir).expect("create source directory");

    struct TestCase<'a> {
        file_name: &'a str,
        content: &'a [u8],
        expected_mime: &'a str,
    }

    let cases = [
        TestCase {
            file_name: "diagram.png",
            content: b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89",
            expected_mime: "image/png",
        },
        TestCase {
            file_name: "photo.jpg",
            content: b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x01\x00`\x00`\x00\x00\xFF\xDB",
            expected_mime: "image/jpeg",
        },
        TestCase {
            file_name: "spec.pdf",
            content: b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF",
            expected_mime: "application/pdf",
        },
        TestCase {
            file_name: "notes.txt",
            content: b"Plain text documentation and instructions.",
            expected_mime: "text/plain",
        },
        TestCase {
            file_name: "guide.md",
            content: b"# Markdown Guide\n\n- Point 1\n- Point 2\n",
            expected_mime: "text/markdown",
        },
        TestCase {
            file_name: "config.json",
            content: b"{\"version\": 1, \"enabled\": true, \"name\": \"personal-notepad\"}",
            expected_mime: "application/json",
        },
    ];

    for case in &cases {
        let src_path = source_dir.join(case.file_name);
        fs::write(&src_path, case.content).expect("write source fixture");

        let att = AttachmentService::add_attachment(conn, &storage, &note.id, &src_path)
            .unwrap_or_else(|e| panic!("failed to attach {}: {e:?}", case.file_name));

        assert_eq!(att.note_id, note.id);
        assert_eq!(att.file_name, case.file_name);
        assert_eq!(att.file_size, case.content.len() as i64);
        assert_eq!(
            att.mime_type.as_deref(),
            Some(case.expected_mime),
            "mime mismatch for {}",
            case.file_name
        );
        assert!(att.relative_path.starts_with(&format!("{}/", note.id)));
        assert!(storage.file_exists(&att.relative_path));

        // Read stored content via get_attachment_for_open and verify exact byte match
        let open_path = AttachmentService::get_attachment_for_open(conn, &storage, &att.id)
            .expect("resolve attachment path for open");
        let stored_bytes = fs::read(&open_path).expect("read stored attachment");
        assert_eq!(stored_bytes, case.content);
    }

    let all = AttachmentService::list_attachments(conn, &note.id).expect("list attachments");
    assert_eq!(all.len(), cases.len());
}

/// 2. Attaching files with Unicode filenames and special characters.
#[test]
fn test_attachment_unicode_and_special_character_filenames() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Unicode Attachments Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let source_dir = test_db.root().join("source_unicode");
    fs::create_dir_all(&source_dir).expect("create source directory");

    let unicode_names = [
        "यात्रा_दॉक्यूमेंट_📁.pdf",
        "日本語_マニュアル_2026.json",
        "café_crème_résumé.txt",
        "مستند_مهم_2026.md",
        "complex [brackets] & (parentheses) + symbols.txt",
    ];

    for (idx, name) in unicode_names.iter().enumerate() {
        let src_path = source_dir.join(format!("src_{idx}_{name}"));
        let payload = format!("Unicode payload for file index {idx}: {name}").into_bytes();
        fs::write(&src_path, &payload).expect("write unicode source");

        let att = AttachmentService::add_attachment(conn, &storage, &note.id, &src_path)
            .expect("attach unicode filename");

        // The metadata preserves the Unicode original filename verbatim
        assert_eq!(att.file_name, format!("src_{idx}_{name}"));
        assert_eq!(att.file_size, payload.len() as i64);

        // Stored filename on disk uses UUID stem with sanitized ASCII extension
        let open_path = AttachmentService::get_attachment_for_open(conn, &storage, &att.id)
            .expect("open unicode attachment");
        assert!(open_path.exists());
        let read_bytes = fs::read(&open_path).expect("read stored unicode attachment");
        assert_eq!(read_bytes, payload);
    }

    let list = AttachmentService::list_attachments(conn, &note.id).expect("list attachments");
    assert_eq!(list.len(), unicode_names.len());
}

/// 3. Collision resistance: attaching two files with the exact same original filename.
#[test]
fn test_duplicate_original_filenames_collision_free() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Duplicate Filenames Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let source_dir1 = test_db.root().join("folder1");
    let source_dir2 = test_db.root().join("folder2");
    fs::create_dir_all(&source_dir1).expect("create folder 1");
    fs::create_dir_all(&source_dir2).expect("create folder 2");

    let file1_path = source_dir1.join("quarterly_report.pdf");
    let file2_path = source_dir2.join("quarterly_report.pdf");

    let content1 = b"%PDF-1.4 Version 1 content from folder 1";
    let content2 = b"%PDF-1.7 Version 2 distinct content from folder 2";

    fs::write(&file1_path, content1).expect("write file 1");
    fs::write(&file2_path, content2).expect("write file 2");

    let att1 = AttachmentService::add_attachment(conn, &storage, &note.id, &file1_path)
        .expect("attach first file");
    let att2 = AttachmentService::add_attachment(conn, &storage, &note.id, &file2_path)
        .expect("attach second file with same original name");

    // Both retain the same original user-facing name
    assert_eq!(att1.file_name, "quarterly_report.pdf");
    assert_eq!(att2.file_name, "quarterly_report.pdf");

    // But they have distinct primary keys and distinct storage paths
    assert_ne!(att1.id, att2.id);
    assert_ne!(att1.relative_path, att2.relative_path);

    // Both files coexist on disk without overwriting each other
    let path1 = AttachmentService::get_attachment_for_open(conn, &storage, &att1.id)
        .expect("open first attachment");
    let path2 = AttachmentService::get_attachment_for_open(conn, &storage, &att2.id)
        .expect("open second attachment");

    assert_ne!(path1, path2);
    assert_eq!(fs::read(&path1).unwrap(), content1);
    assert_eq!(fs::read(&path2).unwrap(), content2);

    let list = AttachmentService::list_attachments(conn, &note.id).expect("list attachments");
    assert_eq!(list.len(), 2);
}

/// 4. Attaching an empty (0-byte) file.
#[test]
fn test_empty_file_attachment_supported() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Empty File Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let empty_file_path = test_db.root().join("zero_bytes.txt");
    File::create(&empty_file_path).expect("create empty file");

    let att = AttachmentService::add_attachment(conn, &storage, &note.id, &empty_file_path)
        .expect("attach empty file");

    assert_eq!(att.file_size, 0);
    assert_eq!(att.file_name, "zero_bytes.txt");
    assert!(storage.file_exists(&att.relative_path));

    let open_path = AttachmentService::get_attachment_for_open(conn, &storage, &att.id)
        .expect("open empty attachment");
    let content = fs::read(&open_path).expect("read empty attachment");
    assert!(content.is_empty());
}

/// 5. Large file within documented limit & rejection when exceeding size limit.
#[test]
fn test_attachment_size_limit_and_file_too_large_rejection() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Size Bound Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // 5a. Documented default limit is 100 MiB
    let default_storage = AttachmentStorage::new(test_db.paths().clone());
    assert_eq!(default_storage.max_file_size(), MAX_ATTACHMENT_SIZE_BYTES);
    assert_eq!(default_storage.max_file_size(), 100 * 1024 * 1024);

    // 5b. Attach a 256 KiB file with default storage (well within 100 MiB limit)
    let valid_file_path = test_db.root().join("payload_256k.dat");
    let valid_data = vec![0xABu8; 256 * 1024];
    fs::write(&valid_file_path, &valid_data).expect("write 256 KiB file");

    let att = AttachmentService::add_attachment(conn, &default_storage, &note.id, &valid_file_path)
        .expect("attach 256 KiB file");
    assert_eq!(att.file_size, 256 * 1024);
    assert!(default_storage.file_exists(&att.relative_path));

    // 5c. Custom restricted storage limit (e.g. 500 bytes)
    let restricted_storage = AttachmentStorage::with_max_size(test_db.paths().clone(), 500);

    let small_allowed_path = test_db.root().join("small_400b.txt");
    fs::write(&small_allowed_path, vec![b'x'; 400]).expect("write 400 bytes");
    let allowed_att =
        AttachmentService::add_attachment(conn, &restricted_storage, &note.id, &small_allowed_path)
            .expect("attach file within 500 byte limit");
    assert_eq!(allowed_att.file_size, 400);

    let too_large_path = test_db.root().join("oversized_600b.txt");
    fs::write(&too_large_path, vec![b'y'; 600]).expect("write 600 bytes");

    let err =
        AttachmentService::add_attachment(conn, &restricted_storage, &note.id, &too_large_path)
            .unwrap_err();

    match err {
        AttachmentError::FileTooLarge {
            size_bytes,
            max_bytes,
        } => {
            assert_eq!(size_bytes, 600);
            assert_eq!(max_bytes, 500);
        }
        other => panic!("expected FileTooLarge error, got {other:?}"),
    }

    // Verify rejected file created NO database record
    let list = AttachmentService::list_attachments(conn, &note.id).expect("list attachments");
    assert!(!list.iter().any(|a| a.file_name == "oversized_600b.txt"));

    // Verify NO abandoned temporary file was left on disk
    let scan = default_storage.scan_managed_files().expect("scan files");
    assert!(!scan.iter().any(|f| f.is_temp_file));
}

/// 6. Executable blocking policy: never execute or open untrusted executables.
#[test]
fn test_executable_blocking_policy() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    // 6a. Direct extension check verification
    let blocked_extensions = [
        "exe", "bat", "cmd", "sh", "bash", "app", "jar", "run", "ps1", "vbs",
    ];
    for ext in blocked_extensions {
        let filename = format!("payload.{ext}");
        assert!(
            AttachmentStorage::is_executable_extension(&filename),
            "extension {ext} should be blocked"
        );
    }

    let allowed_extensions = [
        "pdf", "png", "jpg", "jpeg", "txt", "md", "json", "docx", "xlsx",
    ];
    for ext in allowed_extensions {
        let filename = format!("document.{ext}");
        assert!(
            !AttachmentStorage::is_executable_extension(&filename),
            "extension {ext} should be allowed"
        );
    }

    // 6b. When opening an attachment, if the filename has an executable extension, it is strictly blocked
    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Executable Blocking Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let sh_script_path = test_db.root().join("deploy.sh");
    fs::write(&sh_script_path, b"#!/bin/sh\necho malicious").expect("write script");

    // Adding the file may succeed (as a stored document file)
    let att = AttachmentService::add_attachment(conn, &storage, &note.id, &sh_script_path)
        .expect("attach script");

    // But attempting to resolve/open it through get_attachment_for_open MUST fail with ExecutableBlocked
    let open_err = AttachmentService::get_attachment_for_open(conn, &storage, &att.id).unwrap_err();
    match open_err {
        AttachmentError::ExecutableBlocked(blocked_name) => {
            assert_eq!(blocked_name, "deploy.sh");
        }
        other => panic!("expected ExecutableBlocked error, got {other:?}"),
    }
}

/// 7. Attachment retrieval and open mechanism: returns verified safe path and content.
#[test]
fn test_attachment_retrieval_and_open_mechanism() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Retrieve Open Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let src_path = test_db.root().join("sample_data.csv");
    let csv_content = b"id,name,role\n1,Alice,Admin\n2,Bob,Editor\n";
    fs::write(&src_path, csv_content).expect("write csv fixture");

    let att =
        AttachmentService::add_attachment(conn, &storage, &note.id, &src_path).expect("attach csv");

    let open_path = AttachmentService::get_attachment_for_open(conn, &storage, &att.id)
        .expect("get attachment for open");

    // Path must be inside attachments_dir
    let canon_root = test_db.paths().attachments_dir.canonicalize().unwrap();
    let canon_file = open_path.canonicalize().unwrap();
    assert!(canon_file.starts_with(&canon_root));
    assert!(open_path.is_file());

    let bytes = fs::read(&open_path).expect("read open path");
    assert_eq!(bytes, csv_content);
}

/// 8. Removing an attachment: removes database metadata and deletes managed file from disk.
#[test]
fn test_attachment_removal_lifecycle() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Removal Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let src_path = test_db.root().join("temp_memo.txt");
    fs::write(&src_path, b"Temporary confidential memo").expect("write memo");

    let att = AttachmentService::add_attachment(conn, &storage, &note.id, &src_path)
        .expect("attach memo");
    assert!(storage.file_exists(&att.relative_path));

    // Remove the attachment
    AttachmentService::remove_attachment(conn, &storage, &att.id).expect("remove attachment");

    // Database record is deleted
    let get_res = AttachmentService::get_attachment(conn, &att.id);
    assert!(matches!(
        get_res,
        Err(AttachmentError::AttachmentNotFound(_))
    ));

    // Managed file is deleted from disk
    assert!(!storage.file_exists(&att.relative_path));

    // Repeated removal returns AttachmentNotFound
    let repeat_res = AttachmentService::remove_attachment(conn, &storage, &att.id);
    assert!(matches!(
        repeat_res,
        Err(AttachmentError::AttachmentNotFound(_))
    ));
}

/// 9. Restart and reopen: attachment metadata and files remain consistent across connection restarts.
#[test]
fn test_restart_and_reopen_attachment_metadata() {
    let mut test_db = TestDatabase::new();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        test_db.connection(),
        CreateNoteDto {
            title: Some("Persistent Attachment Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let src_path = test_db.root().join("persistent_doc.json");
    let doc_json = b"{\"persisted\": true, \"checksum\": \"abc-123\"}";
    fs::write(&src_path, doc_json).expect("write persistent doc");

    let att =
        AttachmentService::add_attachment(test_db.connection(), &storage, &note.id, &src_path)
            .expect("attach persistent doc");

    let original_id = att.id.clone();
    let original_path = att.relative_path.clone();
    let original_size = att.file_size;
    let original_mime = att.mime_type.clone();

    // Simulate application restart
    test_db.close_and_reopen();
    let conn = test_db.connection();

    let fetched = AttachmentService::get_attachment(conn, &original_id)
        .expect("fetch attachment after restart");
    assert_eq!(fetched.id, original_id);
    assert_eq!(fetched.note_id, note.id);
    assert_eq!(fetched.file_name, "persistent_doc.json");
    assert_eq!(fetched.relative_path, original_path);
    assert_eq!(fetched.file_size, original_size);
    assert_eq!(fetched.mime_type, original_mime);

    // Stored disk file remains accessible and readable after restart
    let open_path = AttachmentService::get_attachment_for_open(conn, &storage, &original_id)
        .expect("open attachment after restart");
    let read_bytes = fs::read(open_path).expect("read attachment after restart");
    assert_eq!(read_bytes, doc_json);
}

/// 10. Handling a missing attachment file: produces safe, understandable error without crashing.
#[test]
fn test_missing_managed_file_safe_error_handling() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Missing File Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let src_path = test_db.root().join("ghost_file.pdf");
    fs::write(&src_path, b"%PDF-1.4 ghost data").expect("write ghost file");

    let att = AttachmentService::add_attachment(conn, &storage, &note.id, &src_path)
        .expect("attach ghost file");

    // Manually delete the physical file behind the back of the DB
    let full_path = storage.resolve_safe_path(&att.relative_path).unwrap();
    assert!(full_path.exists());
    fs::remove_file(&full_path).expect("manually remove physical file");
    assert!(!full_path.exists());

    // Calling get_attachment_for_open must return ManagedFileMissing safely
    let open_res = AttachmentService::get_attachment_for_open(conn, &storage, &att.id);
    match open_res {
        Err(AttachmentError::ManagedFileMissing(path)) => {
            assert_eq!(path, att.relative_path);
        }
        other => panic!("expected ManagedFileMissing, got {other:?}"),
    }

    // Consistency check must report this missing file
    let report = AttachmentService::check_consistency(conn, &storage).expect("check consistency");
    assert!(!report.is_consistent);
    assert_eq!(report.missing_managed_files.len(), 1);
    assert_eq!(report.missing_managed_files[0].attachment_id, att.id);
    assert_eq!(
        report.missing_managed_files[0].relative_path,
        att.relative_path
    );
}

/// 11. Handling corrupted attachment and size mismatch detection.
#[test]
fn test_corrupted_attachment_size_mismatch_detection() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Corrupted File Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let src_path = test_db.root().join("intact.txt");
    let original_bytes = b"Original uncorrupted content 100% genuine.";
    fs::write(&src_path, original_bytes).expect("write intact fixture");

    let att = AttachmentService::add_attachment(conn, &storage, &note.id, &src_path)
        .expect("attach intact file");

    // Corrupt the disk file by modifying its size
    let full_path = storage.resolve_safe_path(&att.relative_path).unwrap();
    let mut f = fs::OpenOptions::new()
        .append(true)
        .open(&full_path)
        .unwrap();
    f.write_all(b" Extra corrupted padding bytes!").unwrap();
    drop(f);

    let report = AttachmentService::check_consistency(conn, &storage).expect("check consistency");
    assert!(!report.is_consistent);
    assert_eq!(report.size_mismatches.len(), 1);
    assert_eq!(report.size_mismatches[0].attachment_id, att.id);
    assert_eq!(
        report.size_mismatches[0].recorded_size,
        original_bytes.len() as i64
    );
    assert!(report.size_mismatches[0].actual_size > original_bytes.len() as u64);
}

/// 12. Rejecting invalid or out-of-scope paths (path traversal defense).
#[test]
fn test_path_traversal_rejection_and_isolation() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    // 12a. Out-of-scope relative path traversal
    let traversal_paths = [
        "../outside.txt",
        "../../etc/passwd",
        "..\\windows\\system32\\cmd.exe",
        "valid_note/../../secret.txt",
        "",
        "   ",
    ];

    for bad_path in traversal_paths {
        let res = storage.resolve_safe_path(bad_path);
        assert!(
            res.is_err(),
            "path '{bad_path}' should be rejected by resolve_safe_path"
        );
    }

    // 12b. Invalid note ID for attachment storage
    let invalid_note_ids = [
        "../traversal",
        "note/subfolder",
        "note\\subfolder",
        "",
        "   ",
    ];

    for bad_id in invalid_note_ids {
        let res = storage.get_note_dir(bad_id);
        assert!(
            res.is_err(),
            "note ID '{bad_id}' should be rejected by get_note_dir"
        );
    }

    // 12c. Attachment service rejects empty note or attachment IDs
    let dummy_src = test_db.root().join("dummy.txt");
    fs::write(&dummy_src, b"dummy").unwrap();

    let err_add = AttachmentService::add_attachment(conn, &storage, "  ", &dummy_src).unwrap_err();
    assert!(matches!(err_add, AttachmentError::InvalidRequest(_)));

    let err_get = AttachmentService::get_attachment(conn, "  ").unwrap_err();
    assert!(matches!(err_get, AttachmentError::InvalidRequest(_)));

    let err_open = AttachmentService::get_attachment_for_open(conn, &storage, "  ").unwrap_err();
    assert!(matches!(err_open, AttachmentError::InvalidRequest(_)));

    let err_rm = AttachmentService::remove_attachment(conn, &storage, "  ").unwrap_err();
    assert!(matches!(err_rm, AttachmentError::InvalidRequest(_)));
}

/// 13. Failed copy or disk-write operation rollback safety.
#[test]
fn test_failed_copy_or_insert_rollback_safety() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Rollback Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // 13a. Non-existent source path fails cleanly without creating database records
    let missing_source = test_db.root().join("non_existent_file.pdf");
    let err_missing =
        AttachmentService::add_attachment(conn, &storage, &note.id, &missing_source).unwrap_err();
    assert!(matches!(err_missing, AttachmentError::SourceUnavailable(_)));

    let list_after_missing = AttachmentService::list_attachments(conn, &note.id).unwrap();
    assert!(list_after_missing.is_empty());

    // 13b. Directory as source path fails cleanly
    let dir_source = test_db.root().join("sub_directory");
    fs::create_dir_all(&dir_source).unwrap();
    let err_dir =
        AttachmentService::add_attachment(conn, &storage, &note.id, &dir_source).unwrap_err();
    assert!(matches!(err_dir, AttachmentError::SourceUnavailable(_)));

    // 13c. Attaching to non-existent note fails without leaving disk files
    let valid_src = test_db.root().join("valid_memo.txt");
    fs::write(&valid_src, b"memo bytes").unwrap();
    let err_bad_note =
        AttachmentService::add_attachment(conn, &storage, "non-existent-note-id", &valid_src)
            .unwrap_err();
    assert!(matches!(err_bad_note, AttachmentError::NoteNotFound(_)));

    // 13d. Attaching to soft-deleted note fails
    let doomed = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Doomed Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create doomed note");
    TrashService::soft_delete(conn, &doomed.id).expect("trash doomed note");

    let err_trashed =
        AttachmentService::add_attachment(conn, &storage, &doomed.id, &valid_src).unwrap_err();
    match err_trashed {
        AttachmentError::InvalidRequest(msg) => {
            assert!(msg.contains("Cannot attach files to a deleted note"));
        }
        other => panic!("expected InvalidRequest for trashed note, got {other:?}"),
    }

    // Verify no stray files were created in storage
    let scan = storage.scan_managed_files().unwrap();
    assert!(scan.is_empty());
}

/// 14. Byte-for-byte roundtrip and SHA-256 hash preservation.
#[test]
fn test_byte_for_byte_roundtrip_and_sha256_integrity() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("SHA256 Integrity Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // Generate non-trivial binary payload
    let mut payload = Vec::with_capacity(65536);
    for i in 0..65536 {
        payload.push(((i * 31 + 17) % 256) as u8);
    }
    let expected_hash = compute_sha256(&payload);

    let src_file = test_db.root().join("binary_test.bin.safe.dat");
    fs::write(&src_file, &payload).expect("write binary payload");

    let att = AttachmentService::add_attachment(conn, &storage, &note.id, &src_file)
        .expect("attach binary file");

    let open_path = AttachmentService::get_attachment_for_open(conn, &storage, &att.id)
        .expect("open attachment for integrity verification");

    let stored_bytes = fs::read(open_path).expect("read stored binary");
    assert_eq!(stored_bytes.len(), payload.len());
    assert_eq!(stored_bytes, payload);

    let actual_hash = compute_sha256(&stored_bytes);
    assert_eq!(actual_hash, expected_hash);
}

/// 15. Orphan cleanup behavior: detects unreferenced files and cleans abandoned temp staging files.
#[test]
fn test_orphaned_and_abandoned_temp_files_cleanup() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Orphan Cleanup Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // 15a. Add one legitimate attachment
    let legit_src = test_db.root().join("legit.txt");
    fs::write(&legit_src, b"Legitimate attachment").unwrap();
    let legit_att = AttachmentService::add_attachment(conn, &storage, &note.id, &legit_src)
        .expect("add legitimate attachment");

    let note_dir = storage.get_note_dir(&note.id).unwrap();

    // 15b. Create an unreferenced file in the note directory
    let orphan_path = note_dir.join("orphan_unreferenced.txt");
    fs::write(&orphan_path, b"Ghost data not registered in SQLite").unwrap();

    // 15c. Create an abandoned temporary staging file
    let abandoned_temp_path = note_dir.join("staging.tmp.deadbeef-1234");
    fs::write(&abandoned_temp_path, b"Leftover partial staging copy").unwrap();

    // Check consistency
    let report: AttachmentConsistencyReport =
        AttachmentService::check_consistency(conn, &storage).expect("check consistency");
    assert!(!report.is_consistent);
    assert_eq!(report.total_attachments_checked, 1);
    assert_eq!(report.total_disk_files_checked, 3);
    assert_eq!(report.abandoned_temp_files.len(), 1);
    assert!(report.abandoned_temp_files[0].contains("staging.tmp.deadbeef-1234"));
    assert_eq!(report.unreferenced_managed_files.len(), 1);
    assert!(report.unreferenced_managed_files[0].contains("orphan_unreferenced.txt"));

    // 15d. Run cleanup_abandoned_temp_files with older_than_seconds = 0
    let cleaned_count = storage
        .cleanup_abandoned_temp_files(0)
        .expect("clean temp files");
    assert_eq!(cleaned_count, 1);

    // The abandoned temp file is gone
    assert!(!abandoned_temp_path.exists());

    // The legitimate attachment and orphan remain (orphan is untracked user data; temp file is safe to prune)
    assert!(storage.file_exists(&legit_att.relative_path));
    assert!(orphan_path.exists());

    // Rerunning consistency check shows 0 abandoned temp files
    let report_after = AttachmentService::check_consistency(conn, &storage).unwrap();
    assert_eq!(report_after.abandoned_temp_files.len(), 0);
    assert_eq!(report_after.unreferenced_managed_files.len(), 1);
}

/// 16. Trash and restore behavior: attachments are preserved when note is in Trash and fully restored.
#[test]
fn test_trash_and_restore_preserves_attachments() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Trashed Note With Attachments".to_string()),
            content: Some("Important project notes.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let file1 = test_db.root().join("diagram.png");
    let file2 = test_db.root().join("architecture.pdf");
    fs::write(&file1, b"\x89PNG\r\n\x1a\n fake png").unwrap();
    fs::write(&file2, b"%PDF-1.4 fake pdf").unwrap();

    let att1 = AttachmentService::add_attachment(conn, &storage, &note.id, &file1).unwrap();
    let att2 = AttachmentService::add_attachment(conn, &storage, &note.id, &file2).unwrap();

    // Soft-delete the note
    TrashService::soft_delete(conn, &note.id).expect("trash note");

    // Note is excluded from active notes list
    let active_notes = NoteRepository::list_active(conn).unwrap();
    assert!(!active_notes.iter().any(|n| n.id == note.id));

    // Both attachments still exist on disk
    assert!(storage.file_exists(&att1.relative_path));
    assert!(storage.file_exists(&att2.relative_path));

    // Removing an attachment while note is in Trash is strictly prohibited
    let rm_err = AttachmentService::remove_attachment(conn, &storage, &att1.id).unwrap_err();
    match rm_err {
        AttachmentError::InvalidRequest(msg) => {
            assert!(
                msg.contains("Cannot remove attachments from a deleted note. Restore it first.")
            );
        }
        other => panic!(
            "expected InvalidRequest when removing attachment from trashed note, got {other:?}"
        ),
    }

    // Restore the note
    TrashService::restore(conn, &note.id).expect("restore note");

    // Note is active again
    let active_after = NoteRepository::list_active(conn).unwrap();
    assert!(active_after.iter().any(|n| n.id == note.id));

    // Attachments are listed, retrieved, and readable as normal
    let restored_list = AttachmentService::list_attachments(conn, &note.id).unwrap();
    assert_eq!(restored_list.len(), 2);

    let path1 = AttachmentService::get_attachment_for_open(conn, &storage, &att1.id).unwrap();
    let path2 = AttachmentService::get_attachment_for_open(conn, &storage, &att2.id).unwrap();
    assert!(path1.exists());
    assert!(path2.exists());
}

/// 17. Permanent deletion cleanup: removes all associated attachment files and note directory from disk.
#[test]
fn test_permanent_deletion_cleans_up_note_attachments() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    // Note A: doomed note
    let note_a = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Doomed Note A".to_string()),
            ..Default::default()
        },
    )
    .expect("create note A");

    // Note B: survivor note
    let note_b = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Survivor Note B".to_string()),
            ..Default::default()
        },
    )
    .expect("create note B");

    let src_a1 = test_db.root().join("a1.txt");
    let src_a2 = test_db.root().join("a2.txt");
    let src_b = test_db.root().join("b.txt");

    fs::write(&src_a1, b"Doomed attachment 1").unwrap();
    fs::write(&src_a2, b"Doomed attachment 2").unwrap();
    fs::write(&src_b, b"Survivor attachment").unwrap();

    let att_a1 = AttachmentService::add_attachment(conn, &storage, &note_a.id, &src_a1).unwrap();
    let att_a2 = AttachmentService::add_attachment(conn, &storage, &note_a.id, &src_a2).unwrap();
    let att_b = AttachmentService::add_attachment(conn, &storage, &note_b.id, &src_b).unwrap();

    let note_a_dir = storage.get_note_dir(&note_a.id).unwrap();
    let note_b_dir = storage.get_note_dir(&note_b.id).unwrap();
    assert!(note_a_dir.exists());
    assert!(note_b_dir.exists());

    // Trash note A first (permanent delete requires note to be trashed)
    TrashService::soft_delete(conn, &note_a.id).expect("trash note A");

    // Permanently delete note A
    let perm_res = TrashService::permanently_delete_note(conn, &storage, &note_a.id)
        .expect("permanently delete note A");
    assert!(perm_res.success);
    assert!(!perm_res.attachment_cleanup_failed);
    assert!(perm_res.error_message.is_none());

    // Note A database record is gone
    assert!(NoteRepository::get_by_id(conn, &note_a.id)
        .unwrap()
        .is_none());

    // Note A attachments in SQLite are deleted by cascade
    let att_a_list = AttachmentRepository::list_by_note_id(conn, &note_a.id).unwrap();
    assert!(att_a_list.is_empty());

    // Note A attachment directory and files are removed from disk
    assert!(!storage.file_exists(&att_a1.relative_path));
    assert!(!storage.file_exists(&att_a2.relative_path));
    assert!(!note_a_dir.exists());

    // Survivor Note B is completely unaffected
    assert!(NoteRepository::get_by_id(conn, &note_b.id)
        .unwrap()
        .is_some());
    assert!(storage.file_exists(&att_b.relative_path));
    assert!(note_b_dir.exists());
    let path_b = AttachmentService::get_attachment_for_open(conn, &storage, &att_b.id).unwrap();
    assert_eq!(fs::read(path_b).unwrap(), b"Survivor attachment");
}

/// 18. Encrypted attachment storage and decrypted temporary file cleanup.
#[test]
fn test_encrypted_attachment_storage_and_decrypted_temp_cleanup() {
    let test_db = TestDatabase::new();
    let conn = test_db.connection();
    let storage = AttachmentStorage::new(test_db.paths().clone());

    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Encrypted Note".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    let dek = generate_dek();

    let src_file = test_db.root().join("secret_plan.pdf");
    let secret_payload = b"%PDF-1.7 Top Secret Strategic Architectural Blueprint 2026";
    fs::write(&src_file, secret_payload).expect("write secret payload");

    // 18a. Store attachment with encryption key
    let att =
        AttachmentService::add_attachment_with_key(conn, &storage, &note.id, &src_file, Some(&dek))
            .expect("add encrypted attachment");

    assert_eq!(att.file_name, "secret_plan.pdf");
    assert_eq!(att.file_size, secret_payload.len() as i64);

    // 18b. Verify physical file on disk is encrypted
    let safe_path = storage.resolve_safe_path(&att.relative_path).unwrap();
    assert!(safe_path.exists());
    assert!(AttachmentStorage::is_encrypted_file(&safe_path));

    let raw_stored = fs::read(&safe_path).expect("read raw stored file");
    assert_ne!(raw_stored, secret_payload);
    // Stored file length includes encryption envelope (version + nonce + tag + ciphertext)
    assert!(raw_stored.len() > secret_payload.len());

    // 18c. Attempting to open encrypted attachment without DEK fails
    let err_no_key =
        AttachmentService::get_attachment_for_open(conn, &storage, &att.id).unwrap_err();
    assert!(
        matches!(
            err_no_key,
            AttachmentError::InvalidRequest(_) | AttachmentError::FileOperationFailed(_)
        ),
        "expected error indicating locked application, got {err_no_key:?}"
    );

    // Opening with wrong DEK fails
    let wrong_dek = generate_dek();
    let err_wrong_key = AttachmentService::get_attachment_for_open_with_key(
        conn,
        &storage,
        &att.id,
        Some(&wrong_dek),
    )
    .unwrap_err();
    assert!(matches!(
        err_wrong_key,
        AttachmentError::FileOperationFailed(_)
    ));

    // 18d. Opening with DEK decrypts to temporary decrypted file in decrypted_attachments_dir()
    let decrypted_temp_path =
        AttachmentService::get_attachment_for_open_with_key(conn, &storage, &att.id, Some(&dek))
            .expect("open encrypted attachment with key");

    assert!(decrypted_temp_path.exists());
    assert!(decrypted_temp_path.starts_with(storage.decrypted_attachments_dir()));

    let decrypted_content = fs::read(&decrypted_temp_path).expect("read decrypted temp file");
    assert_eq!(decrypted_content, secret_payload);

    // 18e. Cleanup decrypted temporary files
    let cleaned = storage
        .cleanup_decrypted_attachments()
        .expect("cleanup decrypted files");
    assert!(cleaned >= 1);
    assert!(!decrypted_temp_path.exists());
}
