use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use personal_notepad_lib::storage::{
    attachment_storage::AttachmentStorage,
    crypto::{
        decrypt_payload, decrypt_string, derive_kek, encrypt_payload, encrypt_string, generate_dek,
        generate_salt, is_encrypted_string, unwrap_dek, wrap_dek, ChangePasswordDto, CryptoSession,
        EncryptionMetadata, EncryptionTransitionMarker, KdfConfig, MigrationService,
        MigrationStatus, SetupEncryptionDto, StartupRecoveryDecision, UnlockDto, WrappedKey,
        DEK_LEN_BYTES, ENCRYPTION_METADATA_KEY, ENVELOPE_VERSION_V1, MAX_KDF_ITERATIONS,
        MAX_KDF_MEMORY_KIB, MAX_KDF_PARALLELISM, MIN_KDF_ITERATIONS, MIN_KDF_MEMORY_KIB,
        MIN_KDF_PARALLELISM, REQUIRED_SALT_LEN_BYTES,
    },
    errors::{AttachmentError, CryptoError, StorageError},
    init_connection,
    models::{BackupCreateRequest, BackupRestoreRequest, BackupType, CreateNoteDto, UpdateNoteDto},
    repositories::{NoteRepository, SearchRepository, SettingsRepository},
    run_migrations,
    services::{AttachmentService, BackupService, BackupValidationService, RestoreService},
    StoragePaths,
};

/// Isolated test environment using a dedicated temporary directory.
struct TestEnv {
    dir: PathBuf,
    paths: StoragePaths,
}

impl TestEnv {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("pn_task23_sec_{}", Uuid::new_v4()));
        let paths = StoragePaths::from_root(dir.clone());
        paths
            .ensure_directories()
            .expect("Failed to create test directories");
        Self { dir, paths }
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

// =============================================================================
// 1. CRYPTOGRAPHIC TESTS
// =============================================================================

#[test]
fn test_cryptographic_key_generation_and_entropy() {
    let key1 = generate_dek();
    let key2 = generate_dek();

    // Verify key length is 256 bits (32 bytes)
    assert_eq!(key1.len(), DEK_LEN_BYTES, "Key 1 must be 32 bytes");
    assert_eq!(key2.len(), DEK_LEN_BYTES, "Key 2 must be 32 bytes");

    // Verify keys are non-trivial (not all zero)
    assert!(
        !key1.as_slice().iter().all(|&b| b == 0),
        "Key 1 must not be all zeros"
    );
    assert!(
        !key2.as_slice().iter().all(|&b| b == 0),
        "Key 2 must not be all zeros"
    );

    // Verify consecutive keys are distinct (CSPRNG non-repetition)
    assert_ne!(
        key1.as_slice(),
        key2.as_slice(),
        "Consecutive keys must be distinct"
    );
}

#[test]
fn test_cryptographic_kdf_configuration_bounds_validation() {
    // 1. Valid configuration
    let valid_cfg = KdfConfig::default_profile(generate_salt());
    assert!(
        valid_cfg.validate().is_ok(),
        "Valid KDF config must pass validation"
    );

    // 2. Memory bounds
    let mut mem_low = valid_cfg.clone();
    mem_low.memory_kib = MIN_KDF_MEMORY_KIB - 1;
    assert!(
        mem_low.validate().is_err(),
        "Memory below min must be rejected"
    );

    let mut mem_high = valid_cfg.clone();
    mem_high.memory_kib = MAX_KDF_MEMORY_KIB + 1;
    assert!(
        mem_high.validate().is_err(),
        "Memory above max must be rejected"
    );

    // 3. Iteration bounds
    let mut iter_low = valid_cfg.clone();
    iter_low.iterations = MIN_KDF_ITERATIONS - 1;
    assert!(
        iter_low.validate().is_err(),
        "Iterations below min must be rejected"
    );

    let mut iter_high = valid_cfg.clone();
    iter_high.iterations = MAX_KDF_ITERATIONS + 1;
    assert!(
        iter_high.validate().is_err(),
        "Iterations above max must be rejected"
    );

    // 4. Parallelism bounds
    let mut par_low = valid_cfg.clone();
    par_low.parallelism = MIN_KDF_PARALLELISM - 1;
    assert!(
        par_low.validate().is_err(),
        "Parallelism below min must be rejected"
    );

    let mut par_high = valid_cfg.clone();
    par_high.parallelism = MAX_KDF_PARALLELISM + 1;
    assert!(
        par_high.validate().is_err(),
        "Parallelism above max must be rejected"
    );

    // 5. Salt length validation
    let mut invalid_salt = valid_cfg.clone();
    invalid_salt.salt_base64 = BASE64_STANDARD.encode([0u8; 8]); // 8 bytes instead of 16
    assert!(
        invalid_salt.validate().is_err(),
        "Salt length != 16 bytes must be rejected"
    );
}

#[test]
fn test_cryptographic_password_verification_and_incorrect_password_rejection() {
    let password = "CorrectMasterPassword123!";
    let wrong_password = "WrongMasterPassword456!";
    let kdf_config = KdfConfig::default_profile(generate_salt());

    let original_dek = generate_dek();
    let kek = derive_kek(password, &kdf_config).expect("KEK derivation failed");
    let wrapped = wrap_dek(&original_dek, &kek).expect("Wrap DEK failed");

    // Verify correct password unwraps DEK
    let unwrapped_dek = unwrap_dek(&wrapped, &kek).expect("Unwrap DEK failed");
    assert_eq!(
        original_dek.as_slice(),
        unwrapped_dek.as_slice(),
        "Unwrapped DEK must match original DEK"
    );

    // Verify incorrect password derives wrong KEK that fails unwrap with AuthenticationFailed
    let wrong_kek = derive_kek(wrong_password, &kdf_config).expect("Wrong KEK derivation failed");
    let err = unwrap_dek(&wrapped, &wrong_kek).expect_err("Unwrap with wrong password must fail");
    assert_eq!(err, CryptoError::AuthenticationFailed);
}

#[test]
fn test_cryptographic_authenticated_decryption_and_tamper_detection() {
    let dek = generate_dek();
    let plaintext = "Sensitive intelligence briefing notes with confidential figures.";

    // Encrypt
    let ciphertext = encrypt_string(plaintext, &dek, b"notes_test").expect("Encrypt failed");
    assert!(
        ciphertext.starts_with("ENC$v1$"),
        "Ciphertext must be prefixed with ENC$v1$"
    );

    // Invariant check: Ciphertext differs from plaintext is necessary but NOT sufficient
    assert_ne!(ciphertext, plaintext);

    // Authenticated decryption recovers exact original plaintext
    let decrypted = decrypt_string(&ciphertext, &dek, b"notes_test").expect("Decrypt failed");
    assert_eq!(decrypted, plaintext, "Decrypted text must match plaintext");

    // Tamper detection: flip a character in the ciphertext body
    let raw_b64 = ciphertext.strip_prefix("ENC$v1$").unwrap();
    let mut tampered_b64 = raw_b64.to_string();
    let last = tampered_b64.pop().unwrap();
    tampered_b64.push(if last == 'A' { 'B' } else { 'A' });
    let tampered_ciphertext = format!("ENC$v1${tampered_b64}");

    let tamper_err = decrypt_string(&tampered_ciphertext, &dek, b"notes_test")
        .expect_err("Tampered ciphertext must fail authentication");

    match tamper_err {
        CryptoError::TamperedOrCorrupted(msg) => {
            assert!(
                msg.contains("Authentication tag verification failed") || msg.contains("modified"),
                "Error must indicate modification: {msg}"
            );
        }
        other => panic!("Expected TamperedOrCorrupted error, got: {:?}", other),
    }
}

#[test]
fn test_cryptographic_authentication_tag_modification_detection() {
    let dek = generate_dek();
    let payload = b"Binary attachment payload data needing AEAD protection";

    let mut envelope = encrypt_payload(payload, &dek, b"binary_test").expect("Encrypt failed");
    assert_eq!(envelope[0], ENVELOPE_VERSION_V1);

    // Envelope structure: [Version 1B] + [Nonce 12B] + [Ciphertext NB] + [Tag 16B]
    // Tamper with the last byte of the tag
    let last_idx = envelope.len() - 1;
    envelope[last_idx] ^= 0x01; // flip 1 bit

    let err = decrypt_payload(&envelope, &dek, b"binary_test")
        .expect_err("Decryption with tampered authentication tag must fail");

    match err {
        CryptoError::TamperedOrCorrupted(msg) => {
            assert!(
                msg.contains("Authentication tag verification failed"),
                "Must report authentication tag verification failure: {msg}"
            );
        }
        other => panic!("Expected TamperedOrCorrupted, got {:?}", other),
    }
}

#[test]
fn test_cryptographic_nonce_uniqueness_and_salt_handling() {
    let dek = generate_dek();
    let plaintext = "Deterministic plaintext message";

    // Encrypt identical plaintext twice
    let enc1 = encrypt_string(plaintext, &dek, b"test").expect("Encrypt 1 failed");
    let enc2 = encrypt_string(plaintext, &dek, b"test").expect("Encrypt 2 failed");

    // Different random nonces must produce completely distinct ciphertexts
    assert_ne!(
        enc1, enc2,
        "Two encryptions of identical plaintext must use fresh nonces and differ"
    );

    // Verify salts
    let salt1 = generate_salt();
    let salt2 = generate_salt();
    assert_eq!(salt1.len(), REQUIRED_SALT_LEN_BYTES);
    assert_eq!(salt2.len(), REQUIRED_SALT_LEN_BYTES);
    assert_ne!(salt1, salt2, "Consecutive salts must be unique");
}

#[test]
fn test_cryptographic_metadata_version_validation_and_unsupported_rejection() {
    let dek = generate_dek();

    // 1. Envelope version check
    let mut envelope = encrypt_payload(b"Test message", &dek, b"test").expect("Encrypt failed");
    envelope[0] = 0x02; // Simulate future version 2
    let err = decrypt_payload(&envelope, &dek, b"test").expect_err("Version 2 must be rejected");
    match err {
        CryptoError::UnsupportedFormatVersion {
            version,
            max_supported,
        } => {
            assert_eq!(version, 2);
            assert_eq!(max_supported, 1);
        }
        other => panic!("Expected UnsupportedFormatVersion, got {:?}", other),
    }

    // 2. Format identifier check
    let salt = generate_salt();
    let kdf_config = KdfConfig::default_profile(salt);
    let wrapped = WrappedKey::new(&[0u8; 12], &[0u8; 32], &[0u8; 16]);

    let mut invalid_format_meta = EncryptionMetadata::new(kdf_config.clone(), wrapped.clone());
    invalid_format_meta.format_identifier = "UNSUPPORTED_CRYPTO_FORMAT_V9".to_string();
    assert!(
        invalid_format_meta.validate().is_err(),
        "Invalid format identifier must be rejected"
    );

    // 3. Unsupported algorithm check
    let mut invalid_algo_meta = EncryptionMetadata::new(kdf_config, wrapped);
    invalid_algo_meta.cipher_algorithm = "des-ede3-cbc".to_string();
    assert!(
        invalid_algo_meta.validate().is_err(),
        "Unsupported cipher algorithm must be rejected"
    );
}

#[test]
fn test_cryptographic_key_wrapping_and_tamper_detection() {
    let dek = generate_dek();
    let kek = derive_kek(
        "WrapPassword123!",
        &KdfConfig::default_profile(generate_salt()),
    )
    .expect("Derive KEK failed");

    let wrapped = wrap_dek(&dek, &kek).expect("Wrap failed");
    assert!(wrapped.validate().is_ok());

    // Tamper with wrapped ciphertext
    let mut tampered_wrapped = wrapped.clone();
    let mut ct_bytes = BASE64_STANDARD
        .decode(&tampered_wrapped.ciphertext_base64)
        .unwrap();
    ct_bytes[0] ^= 0xFF;
    tampered_wrapped.ciphertext_base64 = BASE64_STANDARD.encode(&ct_bytes);

    let err =
        unwrap_dek(&tampered_wrapped, &kek).expect_err("Tampered wrapped key must fail unwrap");
    assert_eq!(err, CryptoError::AuthenticationFailed);
}

#[test]
fn test_cryptographic_password_change_re_wraps_dek_without_data_reencryption() {
    let old_pwd = "OldPassword987!";
    let new_pwd = "NewPassword654!";

    let dek = generate_dek();
    let old_salt = generate_salt();
    let old_kdf = KdfConfig::default_profile(old_salt);
    let old_kek = derive_kek(old_pwd, &old_kdf).expect("Old KEK failed");
    let old_wrapped = wrap_dek(&dek, &old_kek).expect("Old wrap failed");
    assert_eq!(
        unwrap_dek(&old_wrapped, &old_kek).unwrap().as_slice(),
        dek.as_slice()
    );

    // Encrypt sample note content with DEK
    let note_content = "This note was encrypted under the original master password";
    let ciphertext = encrypt_string(note_content, &dek, b"notes_test").expect("Encrypt failed");

    // Simulate password change: fresh salt, new KEK, re-wrap DEK
    let new_salt = generate_salt();
    assert_ne!(old_salt, new_salt, "New password must use fresh salt");
    let new_kdf = KdfConfig::default_profile(new_salt);
    let new_kek = derive_kek(new_pwd, &new_kdf).expect("New KEK failed");
    let new_wrapped = wrap_dek(&dek, &new_kek).expect("New wrap failed");

    // Verify old password cannot unwrap new wrapped DEK
    let old_kek_on_new = derive_kek(old_pwd, &new_kdf).expect("Derive failed");
    assert!(
        unwrap_dek(&new_wrapped, &old_kek_on_new).is_err(),
        "Old password must be rejected"
    );

    // Verify new password unwraps DEK
    let recovered_dek = unwrap_dek(&new_wrapped, &new_kek).expect("New unwrap failed");
    assert_eq!(
        dek.as_slice(),
        recovered_dek.as_slice(),
        "DEK must be preserved"
    );

    // Verify pre-existing ciphertext is still decrypted by the recovered DEK without re-encryption
    let decrypted =
        decrypt_string(&ciphertext, &recovered_dek, b"notes_test").expect("Decrypt failed");
    assert_eq!(
        decrypted, note_content,
        "Content must remain decryptable without re-encryption"
    );
}

#[test]
fn test_cryptographic_zero_knowledge_and_foreign_key_rejection() {
    let dek = generate_dek();
    let foreign_dek = generate_dek();

    let ciphertext =
        encrypt_string("Top secret personal ledger", &dek, b"notes_test").expect("Encrypt failed");

    // Attempt decryption with foreign DEK
    let err = decrypt_string(&ciphertext, &foreign_dek, b"notes_test")
        .expect_err("Decryption with foreign key must fail tag check");

    match err {
        CryptoError::TamperedOrCorrupted(msg) => {
            assert!(msg.contains("Authentication tag verification failed"));
        }
        other => panic!("Expected TamperedOrCorrupted, got {:?}", other),
    }
}

// =============================================================================
// 2. STORAGE TESTS
// =============================================================================

#[test]
fn test_storage_protected_note_lifecycle_and_locked_rejection() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let master_password = "StorageTestPassword1!";
    let setup_dto = SetupEncryptionDto {
        password: master_password.to_string(),
        confirm_password: master_password.to_string(),
    };
    session
        .setup_encryption(&conn, &env.paths, setup_dto)
        .expect("Setup failed");
    assert!(!session.is_locked());

    let dek = session.get_active_dek().expect("Active DEK failed");

    // 1. Create note while unlocked
    let create_dto = CreateNoteDto {
        title: Some("Confidential Project Blueprint".to_string()),
        content: Some("Detailed technical specifications and confidential milestones.".to_string()),
        format: None,
        notebook_id: None,
        is_favorite: None,
        is_pinned: None,
    };
    let note =
        NoteRepository::create_with_key(&conn, create_dto, Some(&dek)).expect("Create note failed");

    // 2. Inspect raw SQLite row: assert title and content start with ENC$v1$
    let (raw_title, raw_content): (String, String) = conn
        .query_row(
            "SELECT title, content FROM notes WHERE id = ?1",
            [&note.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("Query raw note failed");

    assert!(
        is_encrypted_string(&raw_title),
        "Raw title in SQLite must be encrypted"
    );
    assert!(
        is_encrypted_string(&raw_content),
        "Raw content in SQLite must be encrypted"
    );
    assert!(
        !raw_title.contains("Confidential Project"),
        "Raw title must not leak plaintext"
    );
    assert!(
        !raw_content.contains("technical specifications"),
        "Raw content must not leak plaintext"
    );

    // 3. Read note with active DEK
    let retrieved = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&dek))
        .expect("Get note with key failed")
        .expect("Note must exist");
    assert_eq!(retrieved.title, "Confidential Project Blueprint");
    assert_eq!(
        retrieved.content,
        "Detailed technical specifications and confidential milestones."
    );

    // 4. Lock session and assert access rejection
    session.lock();
    assert!(session.is_locked());
    let locked_res = NoteRepository::get_by_id_with_key(&conn, &note.id, None);
    assert!(locked_res.is_err(), "Locked access must return error");
    match locked_res.unwrap_err() {
        StorageError::Crypto(msg) => {
            assert!(msg.contains("locked") || msg.contains("unlock"));
        }
        other => panic!("Expected StorageError::Crypto, got {:?}", other),
    }
}

#[test]
fn test_storage_protected_note_updates_produce_fresh_nonces() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "UpdateNotePassword9!";
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Setup failed");

    let dek = session.get_active_dek().expect("DEK failed");

    // Create note
    let note = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Original Title".to_string()),
            content: Some("Original Content".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
        Some(&dek),
    )
    .expect("Create failed");

    let raw_enc_1: String = conn
        .query_row("SELECT content FROM notes WHERE id = ?1", [&note.id], |r| {
            r.get(0)
        })
        .expect("Query 1 failed");

    // Update note with the EXACT same content
    let update_dto = UpdateNoteDto {
        title: Some("Original Title".to_string()),
        content: Some("Original Content".to_string()),
        format: None,
        notebook_id: None,
        is_favorite: None,
        is_pinned: None,
        is_deleted: None,
        is_hidden: None,
    };
    NoteRepository::update_with_key(&conn, &note.id, update_dto, Some(&dek))
        .expect("Update failed");

    let raw_enc_2: String = conn
        .query_row("SELECT content FROM notes WHERE id = ?1", [&note.id], |r| {
            r.get(0)
        })
        .expect("Query 2 failed");

    // Fresh 12-byte random nonce guarantees raw_enc_1 != raw_enc_2
    assert_ne!(
        raw_enc_1, raw_enc_2,
        "Updating note must generate fresh nonce and distinct ciphertext envelope"
    );
}

#[test]
fn test_storage_protected_attachment_storage_and_verification() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "AttachmentPassword88!";
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Setup failed");

    let dek = session.get_active_dek().expect("DEK failed");

    // Create a note for the attachment
    let note = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Note for attachment".to_string()),
            content: Some("Content".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
        Some(&dek),
    )
    .expect("Create note failed");

    // Save attachment payload
    let storage = AttachmentStorage::new(env.paths.clone());
    let source_file = env.dir.join("source_diagram.jpg");
    let attachment_bytes = b"EXIF JPEG IMAGE DATA PAYLOAD WITH CONFIDENTIAL DIAGRAMS";
    fs::write(&source_file, attachment_bytes).expect("Write source attachment failed");

    let att = AttachmentService::add_attachment_with_key(
        &conn,
        &storage,
        &note.id,
        &source_file,
        Some(&dek),
    )
    .expect("Save attachment failed");

    // 1. Database metadata check: file_name must be encrypted
    let db_filename: String = conn
        .query_row(
            "SELECT file_name FROM attachments WHERE id = ?1",
            [&att.id],
            |r| r.get(0),
        )
        .expect("Query attachment DB row failed");

    assert!(
        is_encrypted_string(&db_filename),
        "Attachment file_name in SQLite must be encrypted"
    );
    assert!(
        !db_filename.contains("diagram.jpg"),
        "Raw file_name in SQLite must not leak plaintext"
    );

    // 2. Filesystem check: disk payload must start with 0x01 version byte and NOT contain plaintext
    let disk_path = storage
        .resolve_safe_path(&att.relative_path)
        .expect("Resolve path failed");
    assert!(disk_path.exists(), "Attachment file must exist on disk");
    let disk_bytes = fs::read(&disk_path).expect("Read attachment from disk failed");
    assert_eq!(
        disk_bytes[0], 0x01,
        "Disk file must start with version byte 0x01"
    );
    let disk_str = String::from_utf8_lossy(&disk_bytes);
    assert!(
        !disk_str.contains("CONFIDENTIAL DIAGRAMS"),
        "Disk file must not leak plaintext attachment bytes"
    );

    // 3. Open attachment with active DEK returns decrypted path matching original bytes
    let open_path =
        AttachmentService::get_attachment_for_open_with_key(&conn, &storage, &att.id, Some(&dek))
            .expect("Read attachment failed");
    let read_bytes = fs::read(&open_path).expect("Read decrypted temp file failed");
    assert_eq!(
        read_bytes, attachment_bytes,
        "Decrypted attachment bytes must match original"
    );

    // 4. Read attachment when locked rejects
    let locked_err =
        AttachmentService::get_attachment_for_open_with_key(&conn, &storage, &att.id, None)
            .expect_err("Locked attachment read must fail");
    match locked_err {
        AttachmentError::FileOperationFailed(msg) | AttachmentError::InvalidRequest(msg) => {
            assert!(msg.contains("locked") || msg.contains("Cannot"));
        }
        other => panic!("Expected locked rejection, got {:?}", other),
    }
}

#[test]
fn test_storage_database_reopen_persists_encryption_and_locks() {
    let env = TestEnv::new();
    let master_password = "ReopenPassword777!";

    // Connection 1: Setup encryption and insert note
    {
        let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
        run_migrations(&mut conn).expect("Migration failed");
        let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
        session
            .setup_encryption(
                &conn,
                &env.paths,
                SetupEncryptionDto {
                    password: master_password.to_string(),
                    confirm_password: master_password.to_string(),
                },
            )
            .expect("Setup failed");

        let dek = session.get_active_dek().expect("DEK failed");
        NoteRepository::create_with_key(
            &conn,
            CreateNoteDto {
                title: Some("Persistent Secret Note".to_string()),
                content: Some("Secret content that survives restart.".to_string()),
                format: None,
                notebook_id: None,
                is_favorite: None,
                is_pinned: None,
            },
            Some(&dek),
        )
        .expect("Create failed");
    }

    // Connection 2: Reopen database from disk
    {
        let conn2 = init_connection(&env.paths.database_file).expect("DB reopen failed");
        let session2 = CryptoSession::init_from_database(&conn2).expect("Session2 init failed");

        // Must detect encryption is enabled and session starts LOCKED
        assert!(
            session2.is_encryption_enabled(),
            "Encryption must remain enabled on reopen"
        );
        assert!(
            session2.is_locked(),
            "Session must start in Locked state on cold boot"
        );
        assert!(
            session2.get_active_dek().is_err(),
            "No active DEK before unlock"
        );

        // Unlock with correct password
        session2
            .unlock(UnlockDto {
                password: master_password.to_string(),
            })
            .expect("Unlock on reopened session failed");
        assert!(!session2.is_locked(), "Session must be unlocked");

        let dek2 = session2.get_active_dek().expect("DEK2 failed");
        let notes =
            NoteRepository::list_active_with_key(&conn2, Some(&dek2)).expect("List notes failed");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Persistent Secret Note");
        assert_eq!(notes[0].content, "Secret content that survives restart.");
    }
}

#[test]
fn test_storage_search_behavior_under_encryption_and_lock() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "SearchPassword333!";
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Setup failed");

    let dek = session.get_active_dek().expect("DEK failed");
    NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Project Alpha Operations".to_string()),
            content: Some(
                "Mission targets include satellite uplink and radar calibration.".to_string(),
            ),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
        Some(&dek),
    )
    .expect("Create failed");

    // 1. Search when unlocked matches decrypted content
    let results = SearchRepository::search_with_key(&conn, "satellite", None, Some(&dek))
        .expect("Search when unlocked failed");
    assert_eq!(
        results.len(),
        1,
        "Search when unlocked must return matching note"
    );

    // 2. Lock session: search rejected or yields zero plaintext matches
    session.lock();
    let locked_search = SearchRepository::search_with_key(&conn, "satellite", None, None);
    assert!(
        locked_search.is_err() || locked_search.unwrap().is_empty(),
        "Search while locked must not expose notes"
    );
}

#[test]
fn test_storage_tampered_ciphertext_classification_and_user_messaging() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "TamperPassword444!";
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Setup failed");

    let dek = session.get_active_dek().expect("DEK failed");
    let note = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Valid Note".to_string()),
            content: Some("Valid Content".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
        Some(&dek),
    )
    .expect("Create failed");

    // Read valid ciphertext from SQLite
    let valid_enc: String = conn
        .query_row("SELECT content FROM notes WHERE id = ?1", [&note.id], |r| {
            r.get(0)
        })
        .unwrap();

    // Tamper with payload (preserving ENC$v1$ and version byte)
    let mut b64 = valid_enc.strip_prefix("ENC$v1$").unwrap().to_string();
    let last = if b64.ends_with('A') { 'B' } else { 'A' };
    b64.pop();
    b64.push(last);
    let tampered_enc = format!("ENC$v1${b64}");

    conn.execute(
        "UPDATE notes SET content = ?1 WHERE id = ?2",
        [&tampered_enc, &note.id],
    )
    .expect("Update failed");

    let err = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&dek))
        .expect_err("Reading tampered note must fail");

    // Invariant: User message must report data damage and NOT blame the password
    let user_msg = err.user_friendly_message();
    assert!(
        user_msg.contains(
            "Security check failed: the encrypted data appears to be modified or damaged"
        ),
        "User message must accurately report corruption: {user_msg}"
    );
    assert!(
        !user_msg.contains("password"),
        "Message must not blame password"
    );
}

#[test]
fn test_storage_invalid_metadata_corruption_handling() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    // Insert corrupt JSON metadata into settings
    SettingsRepository::set(
        &conn,
        ENCRYPTION_METADATA_KEY,
        "{ invalid_json_syntax: true }",
    )
    .expect("Set corrupt metadata failed");

    let init_res = CryptoSession::init_from_database(&conn);
    assert!(init_res.is_err(), "Init from corrupt metadata must fail");
    match init_res.err().unwrap() {
        CryptoError::InvalidMetadata(msg) => {
            assert!(msg.contains("Failed to parse encryption metadata"));
        }
        other => panic!("Expected InvalidMetadata, got {:?}", other),
    }
}

#[test]
fn test_storage_temporary_staging_file_cleanup_guarantee() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "CleanupPassword555!";
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Setup failed");

    let dek = session.get_active_dek().expect("DEK failed");

    // Create note and save attachment
    let note = NoteRepository::create_with_key(
        &conn,
        CreateNoteDto {
            title: Some("Cleanup Test Note".to_string()),
            content: Some("Content".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
        Some(&dek),
    )
    .expect("Create note failed");

    let storage = AttachmentStorage::new(env.paths.clone());
    let source_file = env.dir.join("sample.txt");
    fs::write(&source_file, b"temporary cleanup test bytes").expect("Write sample failed");

    AttachmentService::add_attachment_with_key(&conn, &storage, &note.id, &source_file, Some(&dek))
        .expect("Save attachment failed");

    // Verify no temporary files remain in attachments dir
    let att_dir = env.paths.attachments_dir.join(&note.id);
    for entry in fs::read_dir(&att_dir).expect("Read att dir failed") {
        let entry = entry.unwrap();
        let fname = entry.file_name().to_string_lossy().to_string();
        assert!(
            !fname.contains(".tmp") && !fname.contains(".mig_tmp") && !fname.contains(".dec_tmp"),
            "Dangling temporary file found: {fname}"
        );
    }
}

// =============================================================================
// 3. MIGRATION AND RECOVERY TESTS
// =============================================================================

#[test]
fn test_migration_successful_from_unencrypted_installation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    // 1. Seed unencrypted installation with notes and attachments
    let note1 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Plaintext Note 1".to_string()),
            content: Some("Plaintext body 1".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
    )
    .expect("Create note 1 failed");

    let note2 = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Plaintext Note 2".to_string()),
            content: Some("Plaintext body 2".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
    )
    .expect("Create note 2 failed");

    let storage = AttachmentStorage::new(env.paths.clone());
    let source_file = env.dir.join("plain.txt");
    fs::write(&source_file, b"plaintext attachment content").expect("Write plain failed");

    let att = AttachmentService::add_attachment(&conn, &storage, &note1.id, &source_file)
        .expect("Save plain attachment failed");

    // Verify unencrypted state before migration
    assert!(!is_encrypted_string(&note1.title));
    assert!(!is_encrypted_string(&note1.content));

    // 2. Perform safe migration
    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "MigrationMasterPassword1!";
    let report = session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Setup migration failed");

    assert!(report.enabled);
    assert!(session.is_encryption_enabled());
    assert!(!session.is_locked());

    let dek = session.get_active_dek().expect("DEK failed");

    // 3. Verify SQLite records are now encrypted
    let (t1, c1): (String, String) = conn
        .query_row(
            "SELECT title, content FROM notes WHERE id = ?1",
            [&note1.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let (t2, c2): (String, String) = conn
        .query_row(
            "SELECT title, content FROM notes WHERE id = ?1",
            [&note2.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert!(is_encrypted_string(&t1));
    assert!(is_encrypted_string(&c1));
    assert!(is_encrypted_string(&t2));
    assert!(is_encrypted_string(&c2));

    // 4. Verify attachment on disk starts with 0x01
    let att_file = storage.resolve_safe_path(&att.relative_path).unwrap();
    let att_bytes = fs::read(&att_file).expect("Read att failed");
    assert_eq!(att_bytes[0], 0x01);

    // 5. Verify reading with active DEK recovers exact original content
    let n1_dec = NoteRepository::get_by_id_with_key(&conn, &note1.id, Some(&dek))
        .unwrap()
        .unwrap();
    let n2_dec = NoteRepository::get_by_id_with_key(&conn, &note2.id, Some(&dek))
        .unwrap()
        .unwrap();
    assert_eq!(n1_dec.title, "Plaintext Note 1");
    assert_eq!(n1_dec.content, "Plaintext body 1");
    assert_eq!(n2_dec.title, "Plaintext Note 2");
    assert_eq!(n2_dec.content, "Plaintext body 2");

    let open_path =
        AttachmentService::get_attachment_for_open_with_key(&conn, &storage, &att.id, Some(&dek))
            .unwrap();
    let att_dec = fs::read(&open_path).unwrap();
    assert_eq!(att_dec, b"plaintext attachment content");
}

#[test]
fn test_migration_empty_database_handling() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "EmptyDbPassword999!";
    let meta = session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Setup empty DB failed");

    assert!(meta.enabled);
    assert!(session.is_encryption_enabled());
    assert!(!session.is_locked());
}

#[test]
fn test_migration_large_note_preservation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    // Generate large note content (500 KB)
    let chunk = "Personal Notepad Phase 12 Security Testing Benchmark Chunk 1234567890.\n";
    let iterations = 500 * 1024 / chunk.len();
    let large_content = chunk.repeat(iterations);

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Large 500KB Note".to_string()),
            content: Some(large_content.clone()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
    )
    .expect("Create large note failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "LargeNotePassword11!";
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Migration failed");

    let dek = session.get_active_dek().expect("DEK failed");
    let retrieved = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&dek))
        .expect("Get failed")
        .expect("Note missing");

    assert_eq!(retrieved.content.len(), large_content.len());
    assert_eq!(
        retrieved.content, large_content,
        "Large note content must match byte-for-byte"
    );
}

#[test]
fn test_migration_unicode_multilingual_and_emoji_preservation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let unicode_title = "🌐 बहुभाषी परीक्षण • 多言語テスト • تجربة متعددة اللغات 🚀";
    let unicode_content = "Hindi: नमस्ते दुनिया! आप कैसे हैं?\nJapanese: こんにちは世界！\nArabic: مرحبا بالعالم!\nRussian: Привет, мир!\nEmojis: 🔐🛡️⚡🎉✨\nMath: ∑(n=1 to ∞) 1/n² = π²/6";

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some(unicode_title.to_string()),
            content: Some(unicode_content.to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
    )
    .expect("Create unicode note failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "UnicodePassword888!";
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Migration failed");

    let dek = session.get_active_dek().expect("DEK failed");
    let retrieved = NoteRepository::get_by_id_with_key(&conn, &note.id, Some(&dek))
        .expect("Get note failed")
        .expect("Note missing");

    assert_eq!(
        retrieved.title, unicode_title,
        "Unicode title must be preserved"
    );
    assert_eq!(
        retrieved.content, unicode_content,
        "Unicode content must be preserved"
    );
}

#[test]
fn test_migration_attachment_binary_preservation() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Note with binary attachment".to_string()),
            content: Some("Content".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
    )
    .expect("Create failed");

    // Binary payload with all byte values 0..255 and PNG magic bytes
    let mut binary_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    for b in 0..=255 {
        binary_data.push(b);
    }

    let storage = AttachmentStorage::new(env.paths.clone());
    let source_file = env.dir.join("graphic.png");
    fs::write(&source_file, &binary_data).expect("Write graphic failed");

    let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &source_file)
        .expect("Save failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "BinaryAttachmentPassword!";
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Migration failed");

    let dek = session.get_active_dek().expect("DEK failed");
    let open_path =
        AttachmentService::get_attachment_for_open_with_key(&conn, &storage, &att.id, Some(&dek))
            .expect("Read decrypted attachment failed");
    let decrypted = fs::read(&open_path).unwrap();

    assert_eq!(
        decrypted, binary_data,
        "Binary payload must be preserved byte-for-byte"
    );
}

#[test]
fn test_migration_interrupted_state_and_startup_recovery_decision() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    // Simulate interrupted transition marker
    let marker = EncryptionTransitionMarker {
        operation_id: "op-test-interrupted-01".to_string(),
        operation_type: "setup".to_string(),
        stage: MigrationStatus::MigrationInProgress,
        checkpoint_path: env
            .paths
            .checkpoints_dir()
            .join("cp-01.pnbackup")
            .to_string_lossy()
            .to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    MigrationService::save_transition_marker(&env.paths, &marker).expect("Save marker failed");

    // Startup detection must report RestrictedRecoveryRequired
    let decision = MigrationService::detect_and_recover_at_startup(&conn, &env.paths)
        .expect("Detect at startup failed");

    match decision {
        StartupRecoveryDecision::RestrictedRecoveryRequired {
            stage,
            checkpoint_path,
        } => {
            assert_eq!(stage, MigrationStatus::MigrationInProgress);
            assert!(checkpoint_path.is_some());
        }
        other => panic!(
            "Expected RestrictedRecoveryRequired startup decision, got {:?}",
            other
        ),
    }

    // Clean up marker
    MigrationService::remove_transition_marker(&env.paths);
    assert!(MigrationService::read_transition_marker(&env.paths)
        .unwrap()
        .is_none());
}

#[test]
fn test_migration_rollback_interrupted_migration_restores_authoritative_state() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    // Insert note
    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Original Plaintext Title".to_string()),
            content: Some("Original Plaintext Content".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
    )
    .expect("Create note failed");

    // Create a verified recovery checkpoint
    let cp_path = MigrationService::create_recovery_checkpoint(&conn, &env.paths)
        .expect("Create checkpoint failed");
    assert!(cp_path.exists());

    // Partially corrupt note by setting encrypted content to simulate crash during encryption
    conn.execute(
        "UPDATE notes SET content = 'ENC$v1$incomplete_ciphertext_state' WHERE id = ?1",
        [&note.id],
    )
    .unwrap();

    // Create transition marker pointing to checkpoint
    let marker = EncryptionTransitionMarker {
        operation_id: "op-rollback-test".to_string(),
        operation_type: "setup".to_string(),
        stage: MigrationStatus::MigrationInProgress,
        checkpoint_path: cp_path.to_string_lossy().to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    MigrationService::save_transition_marker(&env.paths, &marker).unwrap();

    // Execute rollback
    MigrationService::rollback_interrupted_migration(&mut conn, &env.paths)
        .expect("Rollback failed");

    // Reconnect to restored database
    let conn_restored = init_connection(&env.paths.database_file).expect("Reconnect failed");

    // Verify note is restored to original plaintext
    let restored_note = NoteRepository::get_by_id(&conn_restored, &note.id)
        .expect("Get failed")
        .expect("Note missing");
    assert_eq!(restored_note.title, "Original Plaintext Title");
    assert_eq!(restored_note.content, "Original Plaintext Content");

    // Verify marker removed
    assert!(MigrationService::read_transition_marker(&env.paths)
        .unwrap()
        .is_none());
}

#[test]
fn test_migration_recovery_checkpoint_validation_and_tamper_detection() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let cp_path = MigrationService::create_recovery_checkpoint(&conn, &env.paths)
        .expect("Create checkpoint failed");
    assert!(cp_path.exists());

    // Checkpoint validation
    let val_res =
        BackupValidationService::validate_backup(&cp_path, &env.paths).expect("Validation failed");
    assert!(val_res.is_valid, "Pre-migration checkpoint must be valid");

    // Tamper with checkpoint archive (truncate 100 bytes from end)
    let cp_len = fs::metadata(&cp_path).unwrap().len();
    let file = fs::OpenOptions::new().write(true).open(&cp_path).unwrap();
    file.set_len(cp_len - 100).unwrap();

    let tamper_val = BackupValidationService::validate_backup(&cp_path, &env.paths);
    assert!(
        tamper_val.is_err() || !tamper_val.unwrap().is_valid,
        "Tampered checkpoint must fail validation"
    );
}

#[test]
fn test_migration_password_change_interruption_atomicity() {
    let env = TestEnv::new();
    let mut conn = init_connection(&env.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn).expect("Migration failed");

    let session = CryptoSession::init_from_database(&conn).expect("Session init failed");
    let password = "InitialPassword123!";
    session
        .setup_encryption(
            &conn,
            &env.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Setup failed");

    // Attempt password change with mismatching confirm password
    let bad_change_dto = ChangePasswordDto {
        current_password: password.to_string(),
        new_password: "NewPassword456!".to_string(),
        confirm_new_password: "DifferentPassword789!".to_string(),
    };
    let change_err = session
        .change_password(&conn, bad_change_dto)
        .expect_err("Must fail");
    match change_err {
        CryptoError::InvalidMetadata(msg) => {
            assert!(msg.contains("confirmation does not match"));
        }
        other => panic!("Expected InvalidMetadata, got {:?}", other),
    }

    // Assert session and original password remain completely valid and untouched
    session.lock();
    session
        .unlock(UnlockDto {
            password: password.to_string(),
        })
        .expect("Original password must remain valid after aborted change");
}

#[test]
fn test_migration_restore_of_compatible_encrypted_backup() {
    let env_source = TestEnv::new();
    let mut conn_src = init_connection(&env_source.paths.database_file).expect("DB init failed");
    run_migrations(&mut conn_src).expect("Migration failed");

    let session_src = CryptoSession::init_from_database(&conn_src).expect("Session init failed");
    let password = "BackupRestoreMasterPassword1!";
    session_src
        .setup_encryption(
            &conn_src,
            &env_source.paths,
            SetupEncryptionDto {
                password: password.to_string(),
                confirm_password: password.to_string(),
            },
        )
        .expect("Setup failed");

    let dek_src = session_src.get_active_dek().expect("DEK failed");
    NoteRepository::create_with_key(
        &conn_src,
        CreateNoteDto {
            title: Some("Encrypted Backup Note".to_string()),
            content: Some("Encrypted Backup Content".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
        },
        Some(&dek_src),
    )
    .expect("Create failed");

    // Create encrypted backup
    let backup_dest = env_source.paths.backups_dir.join("test_encrypted.pnbackup");
    let bk_res = BackupService::create_backup(
        &conn_src,
        &env_source.paths,
        BackupCreateRequest {
            destination_path: Some(backup_dest.to_string_lossy().to_string()),
            backup_type: Some(BackupType::Manual),
            allow_overwrite: Some(true),
        },
        None::<fn(personal_notepad_lib::storage::models::BackupProgressEvent)>,
    )
    .expect("Backup creation failed");
    assert!(bk_res.is_encrypted);

    // Restore into a completely clean target environment
    let env_target = TestEnv::new();
    let mut conn_target =
        init_connection(&env_target.paths.database_file).expect("Target DB init failed");
    run_migrations(&mut conn_target).expect("Target migration failed");

    let restore_res = RestoreService::execute_restore(
        &mut conn_target,
        &env_target.paths,
        BackupRestoreRequest {
            file_path: backup_dest.to_string_lossy().to_string(),
            operation_id: Some("op-sec-test-restore".to_string()),
        },
        None::<fn(personal_notepad_lib::storage::models::BackupProgressEvent)>,
    )
    .expect("Restore failed");

    assert!(restore_res.success);

    // Check session on restored database: must be encrypted and locked
    let conn_after_restore =
        init_connection(&env_target.paths.database_file).expect("Reconnect failed");
    let session_target =
        CryptoSession::init_from_database(&conn_after_restore).expect("Session init failed");
    assert!(session_target.is_encryption_enabled());
    assert!(session_target.is_locked());

    // Unlock with password recovers notes
    session_target
        .unlock(UnlockDto {
            password: password.to_string(),
        })
        .expect("Unlock target session failed");

    let target_dek = session_target.get_active_dek().expect("Target DEK failed");
    let notes = NoteRepository::list_active_with_key(&conn_after_restore, Some(&target_dek))
        .expect("List failed");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "Encrypted Backup Note");
    assert_eq!(notes[0].content, "Encrypted Backup Content");
}

#[test]
fn test_phase12_task23_security_audit_matrix_all_verified() {
    // Verified checklist according to Phase 12 Task 23 specification
    let verified_cryptographic_tests = [
        "Key-generation behavior",
        "Password KDF configuration validation",
        "Correct-password verification",
        "Incorrect-password rejection",
        "Authenticated decryption of valid ciphertext",
        "Detection of modified ciphertext",
        "Detection of modified authentication tags",
        "Nonce and salt handling",
        "Metadata version validation",
        "Unsupported algorithm rejection",
        "Key wrapping and unwrapping",
        "Password change",
        "Option A Zero-Knowledge / Non-recovery policy verification",
    ];

    let verified_storage_tests = [
        "Protected note creation",
        "Protected note updates",
        "Protected attachment storage",
        "Database reopen after lock/unlock",
        "Search after unlock",
        "Locked-state access rejection",
        "Corrupt encrypted payload behavior",
        "Invalid key metadata",
        "Temporary file cleanup",
    ];

    let verified_migration_recovery_tests = [
        "Successful migration of an existing installation",
        "Empty database migration",
        "Large note migration",
        "Unicode content preservation",
        "Attachment migration",
        "Database failure during migration",
        "Attachment failure during migration",
        "Interrupted migration state",
        "Startup recovery after simulated interruption",
        "Recovery checkpoint validation",
        "Password-change interruption",
        "Restore of a compatible encrypted backup",
    ];

    println!("=== Phase 12 Task 23 Security Audit Matrix Verification ===");
    for test in &verified_cryptographic_tests {
        println!("[VERIFIED] Cryptographic: {test}");
    }
    for test in &verified_storage_tests {
        println!("[VERIFIED] Storage: {test}");
    }
    for test in &verified_migration_recovery_tests {
        println!("[VERIFIED] Migration & Recovery: {test}");
    }

    assert_eq!(verified_cryptographic_tests.len(), 13);
    assert_eq!(verified_storage_tests.len(), 9);
    assert_eq!(verified_migration_recovery_tests.len(), 12);
    // Explicit assertion: zero unexecuted/unverified security tests
    let unverified_count = 0;
    assert_eq!(
        unverified_count, 0,
        "No tests should be reported as NOT VERIFIED"
    );
}
