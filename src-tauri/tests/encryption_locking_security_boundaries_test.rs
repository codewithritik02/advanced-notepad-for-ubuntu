mod common;

use base64::Engine;
use std::fs::{self, File};
use std::io::Read;

use personal_notepad_lib::storage::{
    attachment_storage::AttachmentStorage,
    crypto::{
        classify_crypto_failure, decrypt_file_to_file, decrypt_string, derive_kek,
        encrypt_file_to_file, encrypt_string, is_encrypted_string, sanitize_security_message,
        unwrap_dek, ChangePasswordDto, CryptoSession, EncryptionMetadata,
        EncryptionTransitionMarker, MigrationService, MigrationStatus, SecurityEventCode,
        SecurityLogLevel, SetupEncryptionDto, UnlockDto, CURRENT_ENCRYPTION_FORMAT_VERSION,
        DEFAULT_CIPHER_ALGORITHM, DEFAULT_KDF_ALGORITHM, DEK_LEN_BYTES, ENCRYPTION_METADATA_KEY,
        ENVELOPE_VERSION_V1, MAX_PASSWORD_LENGTH, MIN_PASSWORD_LENGTH, REQUIRED_SALT_LEN_BYTES,
    },
    errors::{AttachmentError, CryptoError, StorageError},
    init_connection,
    models::{
        BackupCreateRequest, BackupRestoreRequest, BackupType, CreateAttachmentDto, CreateNoteDto,
        CreateNotebookDto,
    },
    repositories::{
        AttachmentRepository, NoteRepository, NotebookRepository, SearchRepository,
        SettingsRepository, TagRepository,
    },
    services::{AttachmentService, BackupService, RestoreService},
};

use common::{lock_process_global_test_state, TestAppDir};

// =============================================================================
// Task 15 — Test Encryption, Locking, and Security Boundaries
// =============================================================================

#[test]
fn test_initial_encryption_setup_and_metadata_verification() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. Seed unencrypted notes, notebook, tag, attachment
    let nb = NotebookRepository::create(
        &conn,
        CreateNotebookDto {
            name: "Cryptographic Research".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Quantum Key Distribution".to_string()),
            content: Some("Entangled photon pairs across fiber channels.".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        },
    )
    .expect("create note");

    let tag = TagRepository::create(&conn, "qkd").expect("create tag");
    TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).expect("assign tag");

    let att_dir = app_dir
        .paths
        .get_note_attachments_dir(&note.id)
        .expect("att dir");
    fs::create_dir_all(&att_dir).expect("create att dir");
    let att_name = "photon_spec.txt";
    let att_bytes = b"Polarization states: horizontal, vertical, diagonal, anti-diagonal.";
    fs::write(att_dir.join(att_name), att_bytes).expect("write att file");

    AttachmentRepository::create(
        &conn,
        CreateAttachmentDto {
            note_id: note.id.clone(),
            file_name: att_name.to_string(),
            relative_path: format!("{}/{}", note.id, att_name),
            mime_type: Some("text/plain".to_string()),
            file_size: att_bytes.len() as i64,
        },
    )
    .expect("create att record");

    // 2. Initialize crypto session on unencrypted database
    let session = CryptoSession::init_from_database(&conn).expect("init session");
    assert!(!session.is_encryption_enabled());
    assert!(!session.is_locked());

    // 3. Setup encryption with strong password
    let password = "SuperSecretMasterKey!2026";
    let metadata = session
        .setup_encryption(
            &conn,
            &app_dir.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("setup encryption");

    // 4. Verify returned metadata and session state
    assert!(metadata.enabled);
    assert_eq!(metadata.cipher_algorithm, DEFAULT_CIPHER_ALGORITHM);
    assert_eq!(metadata.kdf.algorithm, DEFAULT_KDF_ALGORITHM);

    let decoded_salt = base64::engine::general_purpose::STANDARD
        .decode(&metadata.kdf.salt_base64)
        .expect("decode salt");
    assert_eq!(decoded_salt.len(), REQUIRED_SALT_LEN_BYTES);
    assert_eq!(metadata.format_version, CURRENT_ENCRYPTION_FORMAT_VERSION);
    assert_eq!(metadata.migration_status, MigrationStatus::Active);

    assert!(session.is_encryption_enabled());
    assert!(
        !session.is_locked(),
        "Immediately after setup, session must be Unlocked"
    );

    // 5. Verify active DEK is loaded
    let active_dek = session.get_active_dek().expect("get active dek");
    assert_eq!(active_dek.len(), DEK_LEN_BYTES);

    // 6. Verify pre-migration recovery checkpoint was created in checkpoints/
    let checkpoints =
        RestoreService::list_recovery_checkpoints(&app_dir.paths).expect("list checkpoints");
    assert!(
        !checkpoints.is_empty(),
        "Pre-migration recovery checkpoint must exist"
    );

    // 7. Inspect SQLite raw rows: note title and content must now be ENC$v1$ ciphertext
    let (raw_title, raw_content): (String, String) = conn
        .query_row(
            "SELECT title, content FROM notes WHERE id = ?1",
            [&note.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("query raw note");

    assert!(
        is_encrypted_string(&raw_title),
        "Stored title must be encrypted envelope"
    );
    assert!(
        is_encrypted_string(&raw_content),
        "Stored content must be encrypted envelope"
    );
    assert!(!raw_title.contains("Quantum Key Distribution"));
    assert!(!raw_content.contains("Entangled photon pairs"));

    // 8. Decrypt note with active DEK through repository
    let decrypted_note = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&active_dek))
        .expect("get decrypted note")
        .expect("note exists");
    assert_eq!(decrypted_note.title, "Quantum Key Distribution");
    assert_eq!(
        decrypted_note.content,
        "Entangled photon pairs across fiber channels."
    );
}

#[test]
fn test_password_validation_length_and_mismatch_boundaries() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let session = CryptoSession::init_from_database(&conn).expect("init session");

    // 1. Password too short (< MIN_PASSWORD_LENGTH)
    let short_pwd = "a".repeat(MIN_PASSWORD_LENGTH - 1);
    let short_res = session.setup_encryption(
        &conn,
        &app_dir.paths,
        SetupEncryptionDto {
            password: short_pwd.clone(),
            confirm_password: short_pwd,
        },
    );
    assert!(
        matches!(short_res, Err(CryptoError::InvalidMetadata(ref msg)) if msg.contains("at least")),
        "Password shorter than {MIN_PASSWORD_LENGTH} must be rejected"
    );

    // 2. Password too long (> MAX_PASSWORD_LENGTH)
    let long_pwd = "a".repeat(MAX_PASSWORD_LENGTH + 1);
    let long_res = session.setup_encryption(
        &conn,
        &app_dir.paths,
        SetupEncryptionDto {
            password: long_pwd.clone(),
            confirm_password: long_pwd,
        },
    );
    assert!(
        matches!(long_res, Err(CryptoError::InvalidMetadata(ref msg)) if msg.contains("exceeds maximum")),
        "Password longer than {MAX_PASSWORD_LENGTH} must be rejected"
    );

    // 3. Password confirmation mismatch
    let mismatch_res = session.setup_encryption(
        &conn,
        &app_dir.paths,
        SetupEncryptionDto {
            password: "ValidPassword123!".to_string(),
            confirm_password: "DifferentPassword123!".to_string(),
        },
    );
    assert!(
        matches!(mismatch_res, Err(CryptoError::InvalidMetadata(ref msg)) if msg.contains("confirmation does not match")),
        "Mismatched password confirmation must be rejected"
    );

    // 4. Exact minimum length boundary (MIN_PASSWORD_LENGTH = 8)
    let min_pwd = "12345678";
    assert!(personal_notepad_lib::storage::crypto::validate_password(min_pwd).is_ok());

    // 5. Exact maximum length boundary (MAX_PASSWORD_LENGTH = 1024)
    let max_pwd = "b".repeat(MAX_PASSWORD_LENGTH);
    assert!(personal_notepad_lib::storage::crypto::validate_password(&max_pwd).is_ok());
}

#[test]
fn test_correct_password_unlock_and_active_dek_retrieval() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let session = CryptoSession::init_from_database(&conn).expect("init session");

    let master_password = "CorrectHorseBatteryStaple#2026";
    let meta = session
        .setup_encryption(
            &conn,
            &app_dir.paths,
            SetupEncryptionDto {
                password: master_password.to_string(),
                confirm_password: master_password.to_string(),
            },
        )
        .expect("setup encryption");

    let original_dek = session.get_active_dek().expect("original dek");

    // Lock session
    session.lock();
    assert!(session.is_locked());
    assert_eq!(session.get_active_dek(), Err(CryptoError::AppLocked));

    // Unlock with correct password
    session
        .unlock(UnlockDto {
            password: master_password.to_string(),
        })
        .expect("unlock with correct password");

    assert!(!session.is_locked());
    let unlocked_dek = session.get_active_dek().expect("unlocked dek");
    assert_eq!(unlocked_dek.as_slice(), original_dek.as_slice());

    // Verify derived KEK matches wrapped DEK unwrap
    let kek = derive_kek(master_password, &meta.kdf).expect("derive kek");
    let unwrapped = unwrap_dek(&meta.wrapped_dek, &kek).expect("unwrap dek");
    assert_eq!(unwrapped.as_slice(), original_dek.as_slice());
}

#[test]
fn test_incorrect_password_rejection_and_locked_session_defense() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let session = CryptoSession::init_from_database(&conn).expect("init session");

    let master_password = "PrimaryPassword123!";
    session
        .setup_encryption(
            &conn,
            &app_dir.paths,
            SetupEncryptionDto {
                password: master_password.to_string(),
                confirm_password: master_password.to_string(),
            },
        )
        .expect("setup encryption");

    session.lock();
    assert!(session.is_locked());

    // Attempt unlock with wrong password
    let bad_unlock_res = session.unlock(UnlockDto {
        password: "WrongPassword456!".to_string(),
    });

    assert!(
        matches!(bad_unlock_res, Err(CryptoError::AuthenticationFailed)),
        "Incorrect password must return AuthenticationFailed"
    );

    // Session remains locked
    assert!(
        session.is_locked(),
        "Session must remain locked after failed attempt"
    );
    assert_eq!(
        session.get_active_dek(),
        Err(CryptoError::AppLocked),
        "Active DEK must not be accessible after failed unlock"
    );

    // Repeated bad attempts do not lock out permanently or corrupt state
    for _ in 0..3 {
        let _ = session.unlock(UnlockDto {
            password: "AnotherBadPassword!".to_string(),
        });
        assert!(session.is_locked());
    }

    // Unlocking with the correct password subsequently succeeds
    session
        .unlock(UnlockDto {
            password: master_password.to_string(),
        })
        .expect("subsequent correct unlock must succeed");

    assert!(!session.is_locked());
    assert!(session.get_active_dek().is_ok());
}

#[test]
fn test_locking_purges_dek_and_cleans_decrypted_attachments_cache() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let session = CryptoSession::init_from_database(&conn).expect("init session");

    let master_pwd = "LockTestPassword2026!";
    session
        .setup_encryption(
            &conn,
            &app_dir.paths,
            SetupEncryptionDto {
                password: master_pwd.to_string(),
                confirm_password: master_pwd.to_string(),
            },
        )
        .expect("setup encryption");

    // 1. Verify active DEK is present
    assert!(session.get_active_dek().is_ok());

    // 2. Simulate temporary decrypted attachment file in decrypted_attachments_dir()
    let dec_dir = app_dir.paths.decrypted_attachments_dir();
    fs::create_dir_all(&dec_dir).expect("create dec dir");
    let temp_decrypted_file = dec_dir.join("temp_decrypted_view.pdf");
    fs::write(&temp_decrypted_file, b"Decrypted PDF contents for preview")
        .expect("write temp dec file");
    assert!(temp_decrypted_file.exists());

    // 3. Manual lock
    session.lock();
    assert!(session.is_locked());
    assert_eq!(session.get_active_dek(), Err(CryptoError::AppLocked));

    // 4. AttachmentStorage cleanup
    let storage = AttachmentStorage::new(app_dir.paths.clone());
    let cleaned = storage
        .cleanup_decrypted_attachments()
        .expect("cleanup dec atts");
    assert!(cleaned >= 1);
    assert!(
        !temp_decrypted_file.exists(),
        "Decrypted temp attachment must be wiped on lock"
    );

    // 5. Auto-lock test
    session
        .unlock(UnlockDto {
            password: master_pwd.to_string(),
        })
        .expect("unlock again");
    assert!(!session.is_locked());

    session.auto_lock(120);
    assert!(session.is_locked(), "auto_lock must lock session");
    assert_eq!(session.get_active_dek(), Err(CryptoError::AppLocked));
}

#[test]
fn test_backend_rejection_of_protected_operations_while_locked() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let session = CryptoSession::init_from_database(&conn).expect("init session");

    let master_pwd = "BackendRejectionPassword!";
    session
        .setup_encryption(
            &conn,
            &app_dir.paths,
            SetupEncryptionDto {
                password: master_pwd.to_string(),
                confirm_password: master_pwd.to_string(),
            },
        )
        .expect("setup encryption");

    let dek = session.get_active_dek().expect("dek");

    // Create note with active DEK
    let note = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Top Secret Blueprint".to_string()),
            content: Some("Classified design schematics.".to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("create note with key");

    // Lock session
    session.lock();
    assert!(session.is_locked());

    // 1. NoteRepository::get_by_id_with_key without DEK (locked state) must return StorageError::Crypto
    let get_res = NoteRepository::get_by_id_with_key(&conn, &note.id, None);
    assert!(
        matches!(get_res, Err(StorageError::Crypto(_))),
        "Reading encrypted note without DEK must return StorageError::Crypto"
    );

    // 2. NoteRepository::list without DEK must fail decryption on encrypted rows
    let list_res = NoteRepository::list(&conn, false);
    assert!(
        list_res.is_err(),
        "Listing notes without DEK when encryption is enabled must fail decryption"
    );

    // 3. SearchRepository::search_with_key without DEK must return StorageError::Crypto
    let search_res = SearchRepository::search_with_key(&conn, "Blueprint", None, None);
    assert!(
        matches!(search_res, Err(StorageError::Crypto(_))),
        "Searching while locked must return StorageError::Crypto"
    );

    // 4. FTS tables must be absent to prevent unencrypted disk leaks
    let fts_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'",
            [],
            |r| r.get(0),
        )
        .expect("query fts table");
    assert_eq!(
        fts_exists, 0,
        "Plaintext FTS virtual table must be dropped when encryption is active"
    );

    // 5. Unlocking restores full capability
    session
        .unlock(UnlockDto {
            password: master_pwd.to_string(),
        })
        .expect("unlock");

    let unlocked_dek = session.get_active_dek().expect("unlocked dek");
    let fetched = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&unlocked_dek))
        .expect("fetch note with key")
        .expect("note exists");
    assert_eq!(fetched.title, "Top Secret Blueprint");

    let search_results =
        SearchRepository::search_with_key(&conn, "Blueprint", None, Some(&unlocked_dek))
            .expect("search with key");
    assert_eq!(search_results.len(), 1);
    assert_eq!(search_results[0].note_id, note.id);
}

#[test]
fn test_password_change_re_wraps_dek_without_data_reencryption() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let session = CryptoSession::init_from_database(&conn).expect("init session");

    let old_password = "OldMasterPassword123!";
    let new_password = "NewMasterPassword456!";

    session
        .setup_encryption(
            &conn,
            &app_dir.paths,
            SetupEncryptionDto {
                password: old_password.to_string(),
                confirm_password: old_password.to_string(),
            },
        )
        .expect("setup encryption");

    let initial_dek = session.get_active_dek().expect("initial dek");

    // Create note under initial password
    let note = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Permanent Ciphertext Note".to_string()),
            content: Some(
                "This ciphertext payload should NOT change during password change.".to_string(),
            ),
            ..Default::default()
        },
        Some(&initial_dek),
    )
    .expect("create note");

    // Record raw ciphertext from database before password change
    let (ciphertext_title_before, ciphertext_content_before): (String, String) = conn
        .query_row(
            "SELECT title, content FROM notes WHERE id = ?1",
            [&note.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("query raw note");

    // Attempt to change password to identical password -> rejected
    let same_pwd_res = session.change_password(
        &conn,
        ChangePasswordDto {
            current_password: old_password.to_string(),
            new_password: old_password.to_string(),
            confirm_new_password: old_password.to_string(),
        },
    );
    assert!(
        same_pwd_res.is_err(),
        "Changing password to the same password must be rejected"
    );

    // Attempt to change password with incorrect current password -> rejected
    let wrong_cur_res = session.change_password(
        &conn,
        ChangePasswordDto {
            current_password: "WrongCurrentPassword!".to_string(),
            new_password: new_password.to_string(),
            confirm_new_password: new_password.to_string(),
        },
    );
    assert!(
        wrong_cur_res.is_err(),
        "Changing password with incorrect current password must be rejected"
    );

    // Successfully change password
    session
        .change_password(
            &conn,
            ChangePasswordDto {
                current_password: old_password.to_string(),
                new_password: new_password.to_string(),
                confirm_new_password: new_password.to_string(),
            },
        )
        .expect("change password");

    // Verify DEK in memory is identical (envelope encryption guarantee)
    let current_dek = session.get_active_dek().expect("current dek");
    assert_eq!(
        current_dek.as_slice(),
        initial_dek.as_slice(),
        "Data Encryption Key must not change during password change"
    );

    // Verify raw ciphertext in database is UNTOUCHED (zero re-encryption of note content)
    let (ciphertext_title_after, ciphertext_content_after): (String, String) = conn
        .query_row(
            "SELECT title, content FROM notes WHERE id = ?1",
            [&note.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("query raw note after");

    assert_eq!(
        ciphertext_title_before, ciphertext_title_after,
        "Note title ciphertext must remain bit-for-bit identical after password change"
    );
    assert_eq!(
        ciphertext_content_before, ciphertext_content_after,
        "Note content ciphertext must remain bit-for-bit identical after password change"
    );

    // Lock and test unlock with old password vs new password
    session.lock();

    let old_unlock_res = session.unlock(UnlockDto {
        password: old_password.to_string(),
    });
    assert!(
        matches!(old_unlock_res, Err(CryptoError::AuthenticationFailed)),
        "Old password must no longer unlock the database"
    );

    session
        .unlock(UnlockDto {
            password: new_password.to_string(),
        })
        .expect("new password must unlock successfully");

    // Verify decrypted note matches original
    let decrypted = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&current_dek))
        .expect("get note")
        .expect("note exists");
    assert_eq!(decrypted.title, "Permanent Ciphertext Note");
    assert_eq!(
        decrypted.content,
        "This ciphertext payload should NOT change during password change."
    );
}

#[test]
fn test_authentication_failure_on_tampered_ciphertext_and_tags() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let session = CryptoSession::init_from_database(&conn).expect("init session");

    let pwd = "TamperDetectionPassword!";
    session
        .setup_encryption(
            &conn,
            &app_dir.paths,
            SetupEncryptionDto {
                password: pwd.to_string(),
                confirm_password: pwd.to_string(),
            },
        )
        .expect("setup encryption");

    let dek = session.get_active_dek().expect("dek");
    let aad = b"notes:aad-tamper-test:content";

    let plaintext = "Sensitive financial transaction records and ledger state.";
    let encrypted_envelope = encrypt_string(plaintext, &dek, aad).expect("encrypt string");

    // 1. Bit flip in ciphertext body
    let raw_b64 = encrypted_envelope.strip_prefix("ENC$v1$").unwrap();
    let mut envelope_bytes = base64::engine::general_purpose::STANDARD
        .decode(raw_b64)
        .expect("decode b64");

    // Byte index 15 is within ciphertext (version = 1 byte, nonce = 12 bytes)
    envelope_bytes[15] ^= 0x55;

    let tampered_b64 = format!(
        "ENC$v1${}",
        base64::engine::general_purpose::STANDARD.encode(&envelope_bytes)
    );
    let tampered_dec_res = decrypt_string(&tampered_b64, &dek, aad);
    assert!(
        matches!(tampered_dec_res, Err(CryptoError::TamperedOrCorrupted(_))),
        "Flipped bit in ciphertext must fail AES-256-GCM authentication"
    );

    // 2. Tampering with the 16-byte authentication tag (last 16 bytes of envelope)
    let mut tag_tampered_bytes = base64::engine::general_purpose::STANDARD
        .decode(raw_b64)
        .expect("decode b64");
    let len = tag_tampered_bytes.len();
    tag_tampered_bytes[len - 1] ^= 0xFF; // Flip byte in tag
    let tag_tampered_b64 = format!(
        "ENC$v1${}",
        base64::engine::general_purpose::STANDARD.encode(&tag_tampered_bytes)
    );
    let tag_tampered_res = decrypt_string(&tag_tampered_b64, &dek, aad);
    assert!(
        matches!(tag_tampered_res, Err(CryptoError::TamperedOrCorrupted(_))),
        "Flipped bit in tag must fail authentication tag verification"
    );

    // 3. Truncated envelope (less than minimum 29 bytes)
    let truncated_bytes = vec![ENVELOPE_VERSION_V1, 0x01, 0x02, 0x03];
    let truncated_b64 = format!(
        "ENC$v1${}",
        base64::engine::general_purpose::STANDARD.encode(&truncated_bytes)
    );
    let trunc_res = decrypt_string(&truncated_b64, &dek, aad);
    assert!(
        matches!(trunc_res, Err(CryptoError::TamperedOrCorrupted(_))),
        "Truncated envelope must be rejected"
    );

    // 4. Tampering with wrapped DEK in _crypto_metadata causes unlock failure
    let meta_raw = SettingsRepository::get(&conn, ENCRYPTION_METADATA_KEY)
        .expect("get meta")
        .expect("meta exists");
    let mut meta = EncryptionMetadata::from_json(&meta_raw).expect("parse meta");

    // Tamper with wrapped DEK ciphertext_base64
    let mut wrapped_bytes = base64::engine::general_purpose::STANDARD
        .decode(&meta.wrapped_dek.ciphertext_base64)
        .expect("decode wrapped dek");
    wrapped_bytes[5] ^= 0xAA;
    meta.wrapped_dek.ciphertext_base64 =
        base64::engine::general_purpose::STANDARD.encode(&wrapped_bytes);
    SettingsRepository::set(&conn, ENCRYPTION_METADATA_KEY, &meta.to_json().unwrap())
        .expect("save tampered meta");

    let tampered_session = CryptoSession::init_from_database(&conn).expect("init tampered session");
    assert!(tampered_session.is_locked());
    let unlock_tampered_res = tampered_session.unlock(UnlockDto {
        password: pwd.to_string(),
    });
    assert!(
        matches!(unlock_tampered_res, Err(CryptoError::AuthenticationFailed)),
        "Tampered wrapped DEK in metadata must fail unlock authentication"
    );
}

#[test]
fn test_nonce_uniqueness_and_associated_data_binding() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let session = CryptoSession::init_from_database(&conn).expect("init session");

    session
        .setup_encryption(
            &conn,
            &app_dir.paths,
            SetupEncryptionDto {
                password: "NonceUniquenessTest1!".to_string(),
                confirm_password: "NonceUniquenessTest1!".to_string(),
            },
        )
        .expect("setup encryption");

    let dek = session.get_active_dek().expect("dek");
    let plaintext = "Constant secret message for nonce variation testing 🚀🔒";
    let aad = b"notes:sample-note-id:title";

    // 1. Nonce uniqueness: encrypting the identical plaintext 10 times must produce 10 distinct envelopes
    let mut envelopes = std::collections::HashSet::new();
    for _ in 0..10 {
        let env = encrypt_string(plaintext, &dek, aad).expect("encrypt");
        assert!(
            envelopes.insert(env),
            "Every encryption must use a unique OsRng nonce"
        );
    }
    assert_eq!(envelopes.len(), 10);

    // 2. All 10 envelopes decrypt back to exact original plaintext
    for env in &envelopes {
        let dec = decrypt_string(env, &dek, aad).expect("decrypt");
        assert_eq!(dec, plaintext);
    }

    // 3. Associated Data (AAD) binding prevents field swapping
    let env_title = encrypt_string("Note Title", &dek, b"notes:n1:title").expect("encrypt title");

    // Attempt to decrypt with content AAD -> fails
    let field_swap_res = decrypt_string(&env_title, &dek, b"notes:n1:content");
    assert!(
        field_swap_res.is_err(),
        "Decrypting title ciphertext with content AAD must fail authentication"
    );

    // 4. Associated Data (AAD) binding prevents note ID splicing
    let note_splice_res = decrypt_string(&env_title, &dek, b"notes:n2:title");
    assert!(
        note_splice_res.is_err(),
        "Decrypting note 1 title with note 2 AAD must fail authentication"
    );
}

#[test]
fn test_attachment_file_encryption_decryption_and_tamper_detection() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();
    let session = CryptoSession::init_from_database(&conn).expect("init session");

    session
        .setup_encryption(
            &conn,
            &app_dir.paths,
            SetupEncryptionDto {
                password: "AttachmentFileCryptoPassword!".to_string(),
                confirm_password: "AttachmentFileCryptoPassword!".to_string(),
            },
        )
        .expect("setup encryption");

    let dek = session.get_active_dek().expect("dek");
    let aad = b"attachments:note-1:blueprint.bin";

    // 1. Create original binary file (64 KiB)
    let src_file = app_dir.dir.join("original_blueprint.bin");
    let mut original_data = vec![0u8; 64 * 1024];
    for (i, byte) in original_data.iter_mut().enumerate() {
        *byte = ((i * 31) ^ 0xA5) as u8;
    }
    fs::write(&src_file, &original_data).expect("write original file");

    // 2. Encrypt file to destination
    let enc_file = app_dir.dir.join("encrypted_blueprint.bin.enc");
    encrypt_file_to_file(&src_file, &enc_file, &dek, aad).expect("encrypt file to file");

    assert!(enc_file.exists());
    let enc_bytes = fs::read(&enc_file).expect("read enc file");
    assert_eq!(
        enc_bytes[0], ENVELOPE_VERSION_V1,
        "File must start with envelope version 0x01"
    );

    // 3. Decrypt file to verified replica
    let dec_file = app_dir.dir.join("decrypted_blueprint.bin");
    decrypt_file_to_file(&enc_file, &dec_file, &dek, aad).expect("decrypt file to file");

    let dec_bytes = fs::read(&dec_file).expect("read dec file");
    assert_eq!(
        original_data, dec_bytes,
        "Decrypted attachment file must match original bytes bit-for-bit"
    );

    // 4. Tamper detection: flip a byte in the encrypted file
    let mut tampered_enc_bytes = enc_bytes.clone();
    tampered_enc_bytes[50] ^= 0x7E;
    let tampered_enc_file = app_dir.dir.join("tampered_blueprint.bin.enc");
    fs::write(&tampered_enc_file, &tampered_enc_bytes).expect("write tampered file");

    let tampered_dec_out = app_dir.dir.join("tampered_decrypted_out.bin");
    let dec_tamper_res = decrypt_file_to_file(&tampered_enc_file, &tampered_dec_out, &dek, aad);
    assert!(
        matches!(dec_tamper_res, Err(CryptoError::TamperedOrCorrupted(_))),
        "Tampered encrypted file must fail decryption authentication"
    );
    assert!(
        !tampered_dec_out.exists(),
        "Failed file decryption must clean up partial output"
    );
}

#[test]
fn test_encrypted_backup_and_restore_boundaries() {
    let _lock = lock_process_global_test_state();
    let src_dir = TestAppDir::new();
    let src_conn = src_dir.open_migrated_database();
    let session = CryptoSession::init_from_database(&src_conn).expect("init session");

    let master_password = "BackupRestoreMasterPassword!";
    session
        .setup_encryption(
            &src_conn,
            &src_dir.paths,
            SetupEncryptionDto {
                password: master_password.to_string(),
                confirm_password: master_password.to_string(),
            },
        )
        .expect("setup encryption");

    let dek = session.get_active_dek().expect("dek");

    // Create encrypted note and attachment in source
    let note = NoteRepository::create_with_key(
        &src_conn,
        CreateNoteDto {
            title: Some("Confidential Operational Telemetry".to_string()),
            content: Some("Mission critical encrypted log stream.".to_string()),
            format: Some("md".to_string()),
            ..Default::default()
        },
        Some(&dek),
    )
    .expect("create note");

    let att_dir = src_dir
        .paths
        .get_note_attachments_dir(&note.id)
        .expect("att dir");
    fs::create_dir_all(&att_dir).expect("create att dir");
    let att_name = "telemetry.enc";
    let att_content = b"Encrypted sensor packets with HMAC signatures.";
    fs::write(att_dir.join(att_name), att_content).expect("write att file");

    AttachmentRepository::create(
        &src_conn,
        CreateAttachmentDto {
            note_id: note.id.clone(),
            file_name: att_name.to_string(),
            relative_path: format!("{}/{}", note.id, att_name),
            mime_type: Some("application/octet-stream".to_string()),
            file_size: att_content.len() as i64,
        },
    )
    .expect("create att");

    // Create backup
    let backup_res = BackupService::create_backup(
        &src_conn,
        &src_dir.paths,
        BackupCreateRequest {
            destination_path: None,
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(false),
        },
        None::<fn(_)>,
    )
    .expect("create backup");

    assert!(backup_res.success);

    // Verify manifest specifies is_encrypted: true
    let backup_file = File::open(&backup_res.file_path).expect("open backup");
    let mut archive = zip::ZipArchive::new(backup_file).expect("open zip");
    let mut manifest_file = archive.by_name("manifest.json").expect("find manifest");
    let mut manifest_content = String::new();
    manifest_file
        .read_to_string(&mut manifest_content)
        .expect("read manifest");

    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_content).expect("parse manifest");
    assert_eq!(
        manifest_json["is_encrypted"], true,
        "Manifest must declare is_encrypted = true"
    );

    // Restore into a fresh target environment
    let target_dir = TestAppDir::new();
    let mut target_conn = target_dir.open_migrated_database();

    let restore_res = RestoreService::execute_restore(
        &mut target_conn,
        &target_dir.paths,
        BackupRestoreRequest {
            file_path: backup_res.file_path.clone(),
            operation_id: Some("restore_crypto_test_1".to_string()),
        },
        None::<fn(_)>,
    )
    .expect("restore backup");

    assert!(restore_res.success);

    // Target session should initially be Locked
    let target_session = CryptoSession::init_from_database(&target_conn).expect("target session");
    assert!(target_session.is_encryption_enabled());
    assert!(
        target_session.is_locked(),
        "Restored encrypted database must start in Locked state"
    );
    assert_eq!(target_session.get_active_dek(), Err(CryptoError::AppLocked));

    // Accessing note while locked fails
    let locked_fetch = NoteRepository::get_by_id_with_key(&target_conn, &note.id, None);
    assert!(
        locked_fetch.is_err(),
        "Accessing note while locked must fail"
    );

    // Unlock restored database with original password
    target_session
        .unlock(UnlockDto {
            password: master_password.to_string(),
        })
        .expect("unlock restored database");

    assert!(!target_session.is_locked());
    let target_dek = target_session.get_active_dek().expect("target dek");

    // Verify note is decrypted accurately
    let restored_note =
        NoteRepository::get_by_id_with_key(&target_conn, &note.id, Some(&target_dek))
            .expect("get restored note")
            .expect("note exists");
    assert_eq!(restored_note.title, "Confidential Operational Telemetry");
    assert_eq!(
        restored_note.content,
        "Mission critical encrypted log stream."
    );

    // Verify attachment file restored on disk
    let restored_att_path = target_dir
        .paths
        .attachments_dir
        .join(&note.id)
        .join(att_name);
    assert!(restored_att_path.exists());
    let restored_att_bytes = fs::read(&restored_att_path).expect("read restored att");
    assert_eq!(att_content.as_slice(), restored_att_bytes.as_slice());
}

#[test]
fn test_interrupted_migration_detection_and_safe_rollback() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let mut conn = app_dir.open_migrated_database();

    // 1. Seed unencrypted notes
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Important Plaintext Note".to_string()),
            content: Some("Original body before interrupted migration.".to_string()),
            ..Default::default()
        },
    )
    .expect("create note");

    // 2. Create pre-migration checkpoint
    let checkpoint = RestoreService::create_recovery_checkpoint(&conn, &app_dir.paths)
        .expect("create checkpoint");

    // 3. Write transition marker simulating interruption at MigrationInProgress
    let marker = EncryptionTransitionMarker {
        operation_id: "op-test-interrupted-mig".to_string(),
        operation_type: "setup_migration".to_string(),
        stage: MigrationStatus::MigrationInProgress,
        checkpoint_path: checkpoint.file_path.clone(),
        started_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    MigrationService::save_transition_marker(&app_dir.paths, &marker).expect("save marker");

    // 4. Initialize session and detect recovery decision
    let session = CryptoSession::init_and_recover(&conn, &app_dir.paths).expect("init and recover");
    assert!(
        session.is_interrupted(),
        "Session must recognize interrupted migration state"
    );

    // 5. Execute safe rollback from recovery checkpoint
    let rollback_msg = session
        .rollback_interrupted_migration(&mut conn, &app_dir.paths)
        .expect("rollback interrupted migration");

    assert!(rollback_msg.to_lowercase().contains("successfully"));
    assert!(!session.is_interrupted());
    assert!(!session.is_encryption_enabled());

    // 6. Verify transition marker was deleted
    let marker_check =
        MigrationService::read_transition_marker(&app_dir.paths).expect("read marker");
    assert!(
        marker_check.is_none(),
        "Transition marker must be removed after rollback"
    );

    // 7. Verify original unencrypted note remains intact and accessible
    let fetched = NoteRepository::get_by_id(&conn, &note.id)
        .expect("get note")
        .expect("note exists");
    assert_eq!(fetched.title, "Important Plaintext Note");
    assert_eq!(
        fetched.content,
        "Original body before interrupted migration."
    );
}

#[test]
fn test_safe_handling_of_sensitive_errors_and_audit_log_sanitization() {
    let _lock = lock_process_global_test_state();

    // 1. Verify classify_crypto_failure differentiates credential mismatches from tampering
    let auth_err = CryptoError::AuthenticationFailed;
    let (code, is_corrupt) = classify_crypto_failure(&auth_err);
    assert_eq!(code, "credential_mismatch");
    assert!(!is_corrupt, "Authentication failure is not corruption");

    let tamper_err = CryptoError::TamperedOrCorrupted("Tag mismatch".to_string());
    let (code2, is_corrupt2) = classify_crypto_failure(&tamper_err);
    assert_eq!(code2, "data_corruption_or_tampering");
    assert!(is_corrupt2, "Tamper failure represents data corruption");

    // 2. Verify sanitize_security_message redacts sensitive parameters
    let raw_msg_password = "Error processing password: 'SuperSecret123!' during authentication";
    let sanitized_pwd = sanitize_security_message(raw_msg_password);
    assert!(
        !sanitized_pwd.contains("SuperSecret123!"),
        "Password must be redacted"
    );
    assert!(sanitized_pwd.contains("[REDACTED]"));

    let raw_msg_cipher =
        "Failed to parse token ENC$v1$SGVsbG8gV29ybGQgQ0lQSEVSVEVYVA== in database";
    let sanitized_cipher = sanitize_security_message(raw_msg_cipher);
    assert!(
        !sanitized_cipher.contains("SGVsbG8gV29ybGQgQ0lQSEVSVEVYVA=="),
        "Ciphertext must be redacted"
    );
    assert!(sanitized_cipher.contains("[REDACTED_CIPHERTEXT]"));

    let raw_multiline = "Line 1: Note Title\nLine 2: Confidential Body paragraph 1\nLine 3: Paragraph 2\nLine 4: Paragraph 3";
    let sanitized_multi = sanitize_security_message(raw_multiline);
    assert!(sanitized_multi.contains("[REDACTED MULTILINE CONTENT]"));
    assert!(!sanitized_multi.contains("Paragraph 3"));

    // 3. Security audit log recording
    personal_notepad_lib::storage::crypto::clear_security_audit_log();
    personal_notepad_lib::storage::crypto::record_security_event(
        SecurityEventCode::UnlockAttempted,
        SecurityLogLevel::Info,
        "User unlock attempted with password: 'SecretPassword!'",
        None,
    );

    let audit_logs = personal_notepad_lib::storage::crypto::get_security_audit_log(10);
    assert_eq!(audit_logs.len(), 1);
    assert_eq!(audit_logs[0].code, SecurityEventCode::UnlockAttempted);
    assert!(
        !audit_logs[0].message.contains("SecretPassword!"),
        "Audit log message must be sanitized"
    );
    assert!(audit_logs[0].message.contains("[REDACTED]"));
}

#[test]
fn test_database_restart_persistence_and_locked_state_enforcement() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let note_id: String;
    let master_password = "ColdBootRestartPassword1!";

    // Setup and seed
    {
        let conn = app_dir.open_migrated_database();
        let session = CryptoSession::init_from_database(&conn).expect("init session");
        session
            .setup_encryption(
                &conn,
                &app_dir.paths,
                SetupEncryptionDto {
                    password: master_password.to_string(),
                    confirm_password: master_password.to_string(),
                },
            )
            .expect("setup encryption");

        let dek = session.get_active_dek().expect("dek");
        let note = NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Cold Boot Persistence Test".to_string()),
                content: Some("Cold boot payload must start locked.".to_string()),
                ..Default::default()
            },
            Some(&dek),
        )
        .expect("create note");
        note_id = note.id;
    }

    // Reopen database from disk in fresh connection (Cold Boot)
    {
        let conn_reopen = init_connection(&app_dir.paths.database_file).expect("reopen connection");
        let session_reopen =
            CryptoSession::init_from_database(&conn_reopen).expect("init reopen session");

        // Verify cold boot starts Locked
        assert!(session_reopen.is_encryption_enabled());
        assert!(
            session_reopen.is_locked(),
            "Cold boot must default to Locked"
        );
        assert_eq!(session_reopen.get_active_dek(), Err(CryptoError::AppLocked));

        // Reading without unlock fails
        let locked_fetch = NoteRepository::get_by_id_with_key(&conn_reopen, &note_id, None);
        assert!(
            locked_fetch.is_err(),
            "Reading note while cold-boot locked must fail"
        );

        // Unlock with master password
        session_reopen
            .unlock(UnlockDto {
                password: master_password.to_string(),
            })
            .expect("unlock cold boot session");

        assert!(!session_reopen.is_locked());
        let dek = session_reopen.get_active_dek().expect("dek");

        // Reading with DEK succeeds
        let fetched = NoteRepository::get_by_id_with_key(&conn_reopen, &note_id, Some(&dek))
            .expect("get note")
            .expect("note exists");
        assert_eq!(fetched.title, "Cold Boot Persistence Test");
        assert_eq!(fetched.content, "Cold boot payload must start locked.");
    }
}

#[test]
fn test_security_boundaries_path_traversal_sql_injection_defense() {
    let _lock = lock_process_global_test_state();
    let app_dir = TestAppDir::new();
    let conn = app_dir.open_migrated_database();

    // 1. SQL Injection defense in note title and content
    let malicious_title = "Safe Note'; DROP TABLE notes; --";
    let malicious_content = "Normal body', (SELECT password FROM users)); --";

    let created_note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(malicious_title.to_string()),
            content: Some(malicious_content.to_string()),
            ..Default::default()
        },
    )
    .expect("create note with SQL characters");

    // Verify notes table still exists and note was stored literally
    let notes_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
        .expect("query notes count");
    assert!(
        notes_count >= 1,
        "Notes table must not be dropped by injection attempts"
    );

    let fetched = NoteRepository::get_by_id(&conn, &created_note.id)
        .expect("get note")
        .expect("note exists");
    assert_eq!(
        fetched.title, malicious_title,
        "Title with SQL characters must be stored literally"
    );
    assert_eq!(
        fetched.content, malicious_content,
        "Content with SQL characters must be stored literally"
    );

    // 2. SQL injection in search query
    let malicious_query = "'; SELECT * FROM sqlite_master; --";
    let search_res = SearchRepository::search(&conn, malicious_query, None);
    assert!(
        search_res.is_ok(),
        "Search query with SQL characters must not crash or execute raw SQL"
    );

    // 3. Path traversal defense in AttachmentStorage
    let att_storage = AttachmentStorage::new(app_dir.paths.clone());

    // A. Rejection of traversal note identifiers in get_note_dir
    let traversal_note_id = "../../etc";
    let note_dir_res = att_storage.get_note_dir(traversal_note_id);
    assert!(
        matches!(note_dir_res, Err(AttachmentError::InvalidRequest(_))),
        "Path traversal in note_id must be rejected with InvalidRequest"
    );

    // B. Rejection of traversal relative path in resolve_safe_path
    let traversal_rel_path = "../../etc/shadow";
    let resolve_res = att_storage.resolve_safe_path(traversal_rel_path);
    assert!(
        resolve_res.is_err(),
        "Path traversal in relative_path must be rejected"
    );

    // C. Legitimate attachment creation is confined within note subdirectory
    let source_dir = app_dir.dir.join("sources");
    fs::create_dir_all(&source_dir).expect("create source dir");
    let safe_source = source_dir.join("safe.txt");
    fs::write(&safe_source, b"harmless content").expect("write safe file");

    let add_res =
        AttachmentService::add_attachment(&conn, &att_storage, &created_note.id, &safe_source);
    assert!(add_res.is_ok(), "Normal attachment succeeds");

    let managed_att = add_res.unwrap();
    assert!(
        managed_att.relative_path.starts_with(&created_note.id),
        "Relative path must stay confined within note subdirectory"
    );
    assert!(!managed_att.relative_path.contains(".."));
}
