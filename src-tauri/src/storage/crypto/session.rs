use rusqlite::Connection;
use std::sync::RwLock;

use crate::storage::crypto::kdf::{derive_kek, generate_dek, generate_salt, unwrap_dek, wrap_dek};
use crate::storage::crypto::metadata::{
    EncryptionMetadata, KdfConfig, MigrationStatus, ENCRYPTION_METADATA_KEY,
};
use crate::storage::crypto::migration::{DisableEncryptionReport, MigrationService};
use crate::storage::crypto::secret::{
    validate_password, ChangePasswordDto, SecretBytes, SetupEncryptionDto, UnlockDto,
};
use crate::storage::errors::CryptoError;
use crate::storage::paths::StoragePaths;
use crate::storage::repositories::settings::SettingsRepository;

/// Runtime memory state of application encryption and session keys (Task 7, 8, 12, 13, 17).
pub enum SessionState {
    /// Encryption has not been enabled on this installation. All storage is unencrypted.
    Disabled,
    /// Encryption is active, but the application is locked. Protected data is inaccessible.
    Locked(EncryptionMetadata),
    /// Encryption is active and valid key material is loaded in memory.
    Unlocked {
        metadata: EncryptionMetadata,
        dek: SecretBytes,
    },
    /// An interrupted or failed migration was detected at startup. Normal operations are locked/suspended.
    InterruptedMigration(EncryptionMetadata),
}

/// Thread-safe central manager of encryption state, credentials, and session keys.
pub struct CryptoSession {
    state: RwLock<SessionState>,
}

impl CryptoSession {
    /// Initializes session state by inspecting persistent SQLite settings (Task 8, 17).
    pub fn init_from_database(conn: &Connection) -> Result<Self, CryptoError> {
        let meta_opt = SettingsRepository::get(conn, ENCRYPTION_METADATA_KEY)
            .map_err(|e| CryptoError::InvalidMetadata(e.to_string()))?;

        let initial_state = match meta_opt {
            Some(raw) => {
                let meta = EncryptionMetadata::from_json(&raw)?;
                if !meta.enabled {
                    SessionState::Disabled
                } else if meta.migration_status.is_interrupted()
                    || meta.migration_status.is_failed()
                {
                    SessionState::InterruptedMigration(meta)
                } else {
                    SessionState::Locked(meta)
                }
            }
            None => SessionState::Disabled,
        };

        Ok(Self {
            state: RwLock::new(initial_state),
        })
    }

    /// Initializes session state and runs crash-recovery detection at application startup (Phase 12, Task 18, 22).
    pub fn init_and_recover(conn: &Connection, paths: &StoragePaths) -> Result<Self, CryptoError> {
        use crate::storage::crypto::migration::StartupRecoveryDecision;
        use crate::storage::crypto::{
            configure_security_log_file, record_security_event, SecurityEventCode, SecurityLogLevel,
        };

        configure_security_log_file(paths.security_log_file());

        let decision = MigrationService::detect_and_recover_at_startup(conn, paths)?;
        match decision {
            StartupRecoveryDecision::AutoFinalizedVerified => {
                record_security_event(
                    SecurityEventCode::MigrationRecoveryTriggered,
                    SecurityLogLevel::Info,
                    "Auto-finalized verified migration at startup",
                    None,
                );
                Self::init_from_database(conn)
            }
            StartupRecoveryDecision::CleanedUpUnstarted => {
                Ok(Self::new_with_state(SessionState::Disabled))
            }
            StartupRecoveryDecision::RestrictedRecoveryRequired { .. } => {
                record_security_event(
                    SecurityEventCode::MigrationRecoveryTriggered,
                    SecurityLogLevel::Warning,
                    "Interrupted migration detected at startup; restricted recovery mode activated",
                    None,
                );
                let meta_opt = SettingsRepository::get(conn, ENCRYPTION_METADATA_KEY)
                    .map_err(|e| CryptoError::InvalidMetadata(e.to_string()))?;
                let meta = match meta_opt {
                    Some(raw) => EncryptionMetadata::from_json(&raw)?,
                    None => {
                        let salt = [0u8; crate::storage::crypto::REQUIRED_SALT_LEN_BYTES];
                        let kdf = KdfConfig::default_profile(salt);
                        let wrapped = crate::storage::crypto::metadata::WrappedKey::new(
                            &[0u8; crate::storage::crypto::NONCE_LEN_BYTES],
                            &[0u8; crate::storage::crypto::DEK_LEN_BYTES],
                            &[0u8; crate::storage::crypto::metadata::AEAD_TAG_LEN_BYTES],
                        );
                        let mut m = EncryptionMetadata::new(kdf, wrapped);
                        m.migration_status = MigrationStatus::MigrationInProgress;
                        m
                    }
                };
                Ok(Self::new_with_state(SessionState::InterruptedMigration(
                    meta,
                )))
            }
            StartupRecoveryDecision::None => Self::init_from_database(conn),
        }
    }

    /// Constructs a session with an explicit initial state (primarily for isolated unit tests).
    pub fn new_with_state(state: SessionState) -> Self {
        Self {
            state: RwLock::new(state),
        }
    }

    /// Returns whether encryption is active on this installation.
    pub fn is_encryption_enabled(&self) -> bool {
        let guard = self.state.read().unwrap();
        match &*guard {
            SessionState::Disabled => false,
            SessionState::Locked(m) => m.enabled,
            SessionState::Unlocked { metadata, .. } => metadata.enabled,
            SessionState::InterruptedMigration(m) => m.enabled,
        }
    }

    /// Returns whether the application is currently locked or in an interrupted state.
    pub fn is_locked(&self) -> bool {
        let guard = self.state.read().unwrap();
        matches!(
            &*guard,
            SessionState::Locked(_) | SessionState::InterruptedMigration(_)
        )
    }

    /// Returns whether the application is in an interrupted migration state (Task 17).
    pub fn is_interrupted(&self) -> bool {
        let guard = self.state.read().unwrap();
        matches!(&*guard, SessionState::InterruptedMigration(_))
    }

    /// Retrieves current migration status if encryption metadata exists.
    pub fn get_migration_status(&self) -> Option<MigrationStatus> {
        let guard = self.state.read().unwrap();
        match &*guard {
            SessionState::Disabled => None,
            SessionState::Locked(m) => Some(m.migration_status.clone()),
            SessionState::Unlocked { metadata, .. } => Some(metadata.migration_status.clone()),
            SessionState::InterruptedMigration(m) => Some(m.migration_status.clone()),
        }
    }

    /// Returns the recovery checkpoint path if the session is currently interrupted (Task 18).
    pub fn get_recovery_checkpoint_path(&self, paths: &StoragePaths) -> Option<String> {
        let guard = self.state.read().unwrap();
        if let SessionState::InterruptedMigration(_) = *guard {
            MigrationService::read_transition_marker(paths)
                .ok()
                .flatten()
                .and_then(|m| {
                    if m.checkpoint_path.is_empty() {
                        None
                    } else {
                        Some(m.checkpoint_path)
                    }
                })
        } else {
            None
        }
    }

    /// Retrieves a clone of the active Data Encryption Key.
    /// Fails with `CryptoError::AppLocked` if locked, `CryptoError::MigrationError` if interrupted,
    /// or `CryptoError::NotEnabled` if encryption is disabled.
    pub fn get_active_dek(&self) -> Result<SecretBytes, CryptoError> {
        let guard = self.state.read().unwrap();
        match &*guard {
            SessionState::Unlocked { dek, .. } => Ok(dek.clone()),
            SessionState::Locked(_) => Err(CryptoError::AppLocked),
            SessionState::InterruptedMigration(_) => Err(CryptoError::MigrationError(
                "Application storage is in an interrupted migration state. Restore from a pre-migration backup checkpoint.".to_string(),
            )),
            SessionState::Disabled => Err(CryptoError::NotEnabled),
        }
    }

    /// Locks the application immediately, sanitizing and purging the Data Encryption Key from memory (Task 13, 22).
    pub fn lock(&self) {
        let mut guard = self.state.write().unwrap();
        let new_state = match &*guard {
            SessionState::Unlocked { metadata, .. } => Some(SessionState::Locked(metadata.clone())),
            _ => None,
        };
        if let Some(state) = new_state {
            *guard = state;
            crate::storage::crypto::record_security_event(
                crate::storage::crypto::SecurityEventCode::ManualLockTriggered,
                crate::storage::crypto::SecurityLogLevel::Info,
                "Application session locked manually; DEK purged from memory",
                None,
            );
        }
    }

    /// Automatically locks the application due to inactivity or OS sleep event (Task 14, 22).
    pub fn auto_lock(&self, idle_seconds: u64) {
        let mut guard = self.state.write().unwrap();
        let new_state = match &*guard {
            SessionState::Unlocked { metadata, .. } => Some(SessionState::Locked(metadata.clone())),
            _ => None,
        };
        if let Some(state) = new_state {
            *guard = state;
            crate::storage::crypto::record_security_event(
                crate::storage::crypto::SecurityEventCode::AutoLockTriggered,
                crate::storage::crypto::SecurityLogLevel::Info,
                &format!(
                    "Application session locked automatically after {idle_seconds}s inactivity"
                ),
                None,
            );
        }
    }

    /// Unlocks the application using the supplied password (Task 12, 22).
    /// Recovers the DEK and switches state to `Unlocked`.
    pub fn unlock(&self, dto: UnlockDto) -> Result<(), CryptoError> {
        use crate::storage::crypto::{
            classify_crypto_failure, record_security_event, SecurityEventCode, SecurityLogLevel,
        };

        record_security_event(
            SecurityEventCode::UnlockAttempted,
            SecurityLogLevel::Info,
            "Application unlock attempt initiated",
            None,
        );

        let mut guard = self.state.write().unwrap();
        let metadata = match &*guard {
            SessionState::Locked(m) => m.clone(),
            SessionState::Unlocked { metadata, .. } => metadata.clone(),
            SessionState::Disabled => return Err(CryptoError::NotEnabled),
            SessionState::InterruptedMigration(_) => {
                let err = CryptoError::MigrationError(
                    "Cannot unlock: storage is in an interrupted migration state. Restore from a pre-migration backup checkpoint.".to_string(),
                );
                record_security_event(
                    SecurityEventCode::UnlockFailed,
                    SecurityLogLevel::Warning,
                    "Unlock rejected: storage is in interrupted migration state",
                    Some("migration_interrupted"),
                );
                return Err(err);
            }
        };

        let kek = match derive_kek(&dto.password, &metadata.kdf) {
            Ok(k) => k,
            Err(e) => {
                let (class, is_corrupt) = classify_crypto_failure(&e);
                record_security_event(
                    SecurityEventCode::UnlockFailed,
                    if is_corrupt {
                        SecurityLogLevel::Error
                    } else {
                        SecurityLogLevel::Warning
                    },
                    "Key derivation failed during unlock attempt",
                    Some(class),
                );
                return Err(e);
            }
        };

        let dek = match unwrap_dek(&metadata.wrapped_dek, &kek) {
            Ok(d) => d,
            Err(e) => {
                let (class, is_corrupt) = classify_crypto_failure(&e);
                record_security_event(
                    SecurityEventCode::UnlockFailed,
                    if is_corrupt {
                        SecurityLogLevel::Error
                    } else {
                        SecurityLogLevel::Warning
                    },
                    "Unlock failed: credential or metadata validation error",
                    Some(class),
                );
                return Err(e);
            }
        };

        *guard = SessionState::Unlocked { metadata, dek };
        record_security_event(
            SecurityEventCode::UnlockSucceeded,
            SecurityLogLevel::Info,
            "Application unlocked successfully",
            None,
        );
        Ok(())
    }

    /// Executes the initial encryption setup and safe migration workflow (Task 8, 17):
    /// 1. Validates password constraints and confirmation match.
    /// 2. Ensures encryption is not already enabled or in progress.
    /// 3. Validates pre-migration database health and attachment storage.
    /// 4. Creates a verified pre-migration recovery checkpoint in `checkpoints/`.
    /// 5. Generates random salt, derives KEK, generates random DEK, and wraps DEK.
    /// 6. Validates that the wrapped DEK can be successfully unwrapped with the derived KEK.
    /// 7. Persists the versioned metadata to SQLite settings in `MigrationInProgress` state.
    /// 8. Executes migration of existing notes and attachments with rollback tracking.
    /// 9. Verifies post-migration storage consistency (100% encrypted and test-decryptable).
    /// 10. Finalizes metadata to `Active` state and sets runtime session to `Unlocked`.
    pub fn setup_encryption(
        &self,
        conn: &Connection,
        paths: &StoragePaths,
        dto: SetupEncryptionDto,
    ) -> Result<EncryptionMetadata, CryptoError> {
        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::EncryptionSetupStarted,
            crate::storage::crypto::SecurityLogLevel::Info,
            "Initial encryption setup and migration process started",
            None,
        );

        // 1. Password validation
        validate_password(&dto.password)?;
        if dto.password != dto.confirm_password {
            return Err(CryptoError::InvalidMetadata(
                "Password confirmation does not match the chosen password.".to_string(),
            ));
        }

        // 2. Prevent duplicate encryption setup
        {
            let guard = self.state.read().unwrap();
            match &*guard {
                SessionState::Locked(_)
                | SessionState::Unlocked { .. }
                | SessionState::InterruptedMigration(_) => {
                    return Err(CryptoError::InvalidMetadata(
                        "Encryption is already enabled or migration is in progress on this application.".to_string(),
                    ));
                }
                SessionState::Disabled => {}
            }
        }

        // 3. Pre-migration integrity validation
        MigrationService::validate_pre_migration(conn, paths)?;

        // 4. Create verified pre-migration recovery checkpoint (Phase 11, Task 16, Task 8, Task 17)
        let checkpoint_path = MigrationService::create_recovery_checkpoint(conn, paths)?;

        // Durable operation marker: CheckpointCreated (Phase 12, Task 18)
        let op_id = uuid::Uuid::new_v4().to_string();
        let mut marker = crate::storage::crypto::migration::EncryptionTransitionMarker {
            operation_id: op_id.clone(),
            operation_type: "setup_migration".to_string(),
            stage: MigrationStatus::CheckpointCreated,
            checkpoint_path: checkpoint_path.to_string_lossy().to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        MigrationService::save_transition_marker(paths, &marker)?;

        // 5. Cryptographic key generation & wrapping
        let salt = generate_salt();
        let kdf_config = KdfConfig::default_profile(salt);
        let kek = derive_kek(&dto.password, &kdf_config)?;
        let dek = generate_dek();
        let wrapped_dek = wrap_dek(&dek, &kek)?;

        // 6. Verification: test unwrapping immediately
        let test_dek = unwrap_dek(&wrapped_dek, &kek)?;
        if test_dek.as_slice() != dek.as_slice() {
            MigrationService::remove_transition_marker(paths);
            return Err(CryptoError::KeyManagement(
                "Immediate pre-persistence verification of wrapped key failed".to_string(),
            ));
        }

        // 7. Persist initial metadata with MigrationInProgress status to prevent mixed state on unexpected exit
        let mut metadata = EncryptionMetadata::new(kdf_config, wrapped_dek);
        metadata.migration_status = MigrationStatus::MigrationInProgress;
        let meta_json = metadata.to_json()?;

        SettingsRepository::set(conn, ENCRYPTION_METADATA_KEY, &meta_json).map_err(|e| {
            CryptoError::InvalidMetadata(format!("Failed to persist encryption metadata: {e}"))
        })?;

        marker.stage = MigrationStatus::MigrationInProgress;
        marker.updated_at = chrono::Utc::now().to_rfc3339();
        MigrationService::save_transition_marker(paths, &marker)?;

        // 8. Execute safe migration workflow
        let migration_res = (|| -> Result<(), CryptoError> {
            // A. Migrate existing plaintext notes into ENC$v1$ authenticated ciphertext
            MigrationService::migrate_notes(conn, &dek)?;

            // B. Update marker to StagingPrepared
            marker.stage = MigrationStatus::StagingPrepared;
            marker.updated_at = chrono::Utc::now().to_rfc3339();
            let _ = MigrationService::save_transition_marker(paths, &marker);

            // C. Migrate existing plaintext attachment records and files on disk
            MigrationService::migrate_attachments(conn, paths, &dek)?;

            // D. Purge unencrypted plaintext FTS search index and synchronization triggers (Phase 12, Task 11)
            if let Err(e) =
                crate::storage::repositories::SearchRepository::purge_plaintext_index(conn)
            {
                eprintln!(
                    "[Crypto] Warning: Failed to purge unencrypted FTS search index on setup: {e}"
                );
            }

            // E. Post-migration verification: 100% ciphertext envelopes and test-decrypt
            MigrationService::verify_post_migration(conn, paths, &dek)?;

            // F. Update marker to Verified
            marker.stage = MigrationStatus::Verified;
            marker.updated_at = chrono::Utc::now().to_rfc3339();
            let _ = MigrationService::save_transition_marker(paths, &marker);

            Ok(())
        })();

        if let Err(err) = migration_res {
            crate::storage::crypto::record_security_event(
                crate::storage::crypto::SecurityEventCode::EncryptionMigrationFailed,
                crate::storage::crypto::SecurityLogLevel::Error,
                "Encryption migration step failed",
                Some(&err.to_string()),
            );

            // Record failed state in persistent metadata and marker
            metadata.migration_status = MigrationStatus::Failed(err.to_string());
            marker.stage = MigrationStatus::Failed(err.to_string());
            marker.updated_at = chrono::Utc::now().to_rfc3339();
            let _ = MigrationService::save_transition_marker(paths, &marker);

            if let Ok(failed_json) = metadata.to_json() {
                let _ = SettingsRepository::set(conn, ENCRYPTION_METADATA_KEY, &failed_json);
            }
            return Err(err);
        }

        // 9. Mark migration Active and persist final metadata
        metadata.migration_status = MigrationStatus::Active;
        let final_json = metadata.to_json()?;
        SettingsRepository::set(conn, ENCRYPTION_METADATA_KEY, &final_json).map_err(|e| {
            CryptoError::InvalidMetadata(format!("Failed to finalize encryption metadata: {e}"))
        })?;

        MigrationService::remove_transition_marker(paths);

        // 10. Transition runtime session memory state to Unlocked
        {
            let mut guard = self.state.write().unwrap();
            *guard = SessionState::Unlocked {
                metadata: metadata.clone(),
                dek,
            };
        }

        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::EncryptionSetupCompleted,
            crate::storage::crypto::SecurityLogLevel::Info,
            "Encryption successfully enabled and verified",
            None,
        );

        Ok(metadata)
    }

    /// Safely rolls back an interrupted or corrupt migration using the pre-migration recovery checkpoint (Phase 12, Task 18).
    pub fn rollback_interrupted_migration(
        &self,
        conn: &mut Connection,
        paths: &StoragePaths,
    ) -> Result<String, CryptoError> {
        let msg = MigrationService::rollback_interrupted_migration(conn, paths)?;
        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::MigrationRecoveryTriggered,
            crate::storage::crypto::SecurityLogLevel::Warning,
            "Rollback of interrupted migration executed successfully",
            None,
        );
        let mut guard = self.state.write().unwrap();
        *guard = SessionState::Disabled;
        Ok(msg)
    }

    /// Resumes an interrupted migration from its current stage using the user's master password (Phase 12, Task 18).
    pub fn resume_interrupted_migration(
        &self,
        conn: &Connection,
        paths: &StoragePaths,
        password: &str,
    ) -> Result<EncryptionMetadata, CryptoError> {
        let (metadata, dek) =
            MigrationService::resume_interrupted_migration(conn, paths, password)?;
        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::MigrationRecoveryTriggered,
            crate::storage::crypto::SecurityLogLevel::Info,
            "Resume of interrupted migration executed successfully",
            None,
        );
        let mut guard = self.state.write().unwrap();
        *guard = SessionState::Unlocked {
            metadata: metadata.clone(),
            dek,
        };
        Ok(metadata)
    }

    /// Changes the user encryption password without re-encrypting notes or attachments (Task 15).
    pub fn change_password(
        &self,
        conn: &Connection,
        dto: ChangePasswordDto,
    ) -> Result<(), CryptoError> {
        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::PasswordChangeStarted,
            crate::storage::crypto::SecurityLogLevel::Info,
            "Password change operation initiated",
            None,
        );

        validate_password(&dto.new_password)?;
        if dto.new_password != dto.confirm_new_password {
            return Err(CryptoError::InvalidMetadata(
                "New password confirmation does not match.".to_string(),
            ));
        }

        if dto.new_password == dto.current_password {
            return Err(CryptoError::InvalidMetadata(
                "New password must be different from current password.".to_string(),
            ));
        }

        let mut guard = self.state.write().unwrap();
        let (mut metadata, dek) = match &*guard {
            SessionState::Unlocked { metadata, dek } => (metadata.clone(), dek.clone()),
            SessionState::Locked(_) => return Err(CryptoError::AppLocked),
            SessionState::Disabled => return Err(CryptoError::NotEnabled),
            SessionState::InterruptedMigration(_) => {
                return Err(CryptoError::MigrationError(
                    "Cannot change password during an interrupted migration state.".to_string(),
                ));
            }
        };

        // 1. Verify current password
        let old_kek = match derive_kek(&dto.current_password, &metadata.kdf) {
            Ok(k) => k,
            Err(e) => {
                crate::storage::crypto::record_security_event(
                    crate::storage::crypto::SecurityEventCode::PasswordChangeFailed,
                    crate::storage::crypto::SecurityLogLevel::Warning,
                    "Password change failed: KDF derivation error on current password",
                    Some("kdf_error"),
                );
                return Err(e);
            }
        };

        let verified_dek = match unwrap_dek(&metadata.wrapped_dek, &old_kek) {
            Ok(d) => d,
            Err(e) => {
                crate::storage::crypto::record_security_event(
                    crate::storage::crypto::SecurityEventCode::PasswordChangeFailed,
                    crate::storage::crypto::SecurityLogLevel::Warning,
                    "Password change failed: current password verification failed",
                    Some("credential_mismatch"),
                );
                return Err(e);
            }
        };

        if verified_dek.as_slice() != dek.as_slice() {
            crate::storage::crypto::record_security_event(
                crate::storage::crypto::SecurityEventCode::PasswordChangeFailed,
                crate::storage::crypto::SecurityLogLevel::Warning,
                "Password change failed: unwrapped key mismatch",
                Some("credential_mismatch"),
            );
            return Err(CryptoError::AuthenticationFailed);
        }

        // 2. Generate fresh salt and new KEK
        let new_salt = generate_salt();
        let new_kdf = KdfConfig::default_profile(new_salt);
        let new_kek = derive_kek(&dto.new_password, &new_kdf)?;

        // 3. Re-wrap existing DEK
        let new_wrapped_dek = wrap_dek(&dek, &new_kek)?;

        // Self-verification check before committing to SQLite (Task 15):
        // Guarantee the new credential can unlock the same data key before retiring old metadata
        let test_unwrapped = unwrap_dek(&new_wrapped_dek, &new_kek)?;
        if test_unwrapped.as_slice() != dek.as_slice() {
            crate::storage::crypto::record_security_event(
                crate::storage::crypto::SecurityEventCode::PasswordChangeFailed,
                crate::storage::crypto::SecurityLogLevel::Error,
                "Password change failed: self-verification unwrap mismatch",
                Some("self_verification_failed"),
            );
            return Err(CryptoError::KeyManagement(
                "Self-verification failed: new credential could not unwrap the data encryption key.".to_string(),
            ));
        }

        // 4. Update metadata atomically
        metadata.kdf = new_kdf;
        metadata.wrapped_dek = new_wrapped_dek;
        metadata.updated_at = chrono::Utc::now().to_rfc3339();

        let meta_json = metadata.to_json()?;
        SettingsRepository::set(conn, ENCRYPTION_METADATA_KEY, &meta_json).map_err(|e| {
            CryptoError::InvalidMetadata(format!(
                "Failed to persist updated encryption metadata: {e}"
            ))
        })?;

        *guard = SessionState::Unlocked { metadata, dek };
        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::PasswordChangeCompleted,
            crate::storage::crypto::SecurityLogLevel::Info,
            "Password changed successfully; DEK re-wrapped with fresh salt",
            None,
        );
        Ok(())
    }

    /// Disables encryption completely, decrypting all database notes and attachments on disk
    /// after creating a verified recovery checkpoint (Phase 12, Task 21).
    pub fn disable_encryption(
        &self,
        conn: &Connection,
        paths: &StoragePaths,
        password: &str,
    ) -> Result<DisableEncryptionReport, CryptoError> {
        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::DisableEncryptionStarted,
            crate::storage::crypto::SecurityLogLevel::Warning,
            "Disable encryption workflow initiated",
            None,
        );

        let report = match MigrationService::disable_encryption(conn, paths, password) {
            Ok(r) => r,
            Err(e) => {
                let (class, _) = crate::storage::crypto::classify_crypto_failure(&e);
                crate::storage::crypto::record_security_event(
                    crate::storage::crypto::SecurityEventCode::DisableEncryptionFailed,
                    crate::storage::crypto::SecurityLogLevel::Error,
                    "Disable encryption workflow failed",
                    Some(class),
                );
                return Err(e);
            }
        };

        let mut guard = self.state.write().unwrap();
        *guard = SessionState::Disabled;

        crate::storage::crypto::record_security_event(
            crate::storage::crypto::SecurityEventCode::DisableEncryptionCompleted,
            crate::storage::crypto::SecurityLogLevel::Info,
            "Disable encryption completed successfully; all notes and attachments decrypted",
            Some(&format!(
                "notes_decrypted={}, attachments_decrypted={}",
                report.notes_decrypted, report.attachments_decrypted
            )),
        );

        Ok(report)
    }

    /// Resynchronizes runtime in-memory encryption session state with the persistent database and disk (Phase 12, Task 19).
    /// Used immediately following database restore operations to guarantee in-memory keys and state match
    /// the newly activated storage contents.
    pub fn refresh_from_database(
        &self,
        conn: &Connection,
        paths: &StoragePaths,
    ) -> Result<bool, CryptoError> {
        let meta_opt = SettingsRepository::get(conn, ENCRYPTION_METADATA_KEY)
            .map_err(|e| CryptoError::InvalidMetadata(e.to_string()))?;

        let marker_opt = MigrationService::read_transition_marker(paths)
            .ok()
            .flatten();

        let new_state = match meta_opt {
            Some(raw) => {
                let meta = EncryptionMetadata::from_json(&raw)?;
                if !meta.enabled {
                    SessionState::Disabled
                } else if meta.migration_status.is_interrupted()
                    || meta.migration_status.is_failed()
                    || marker_opt
                        .as_ref()
                        .map(|m| m.stage.is_interrupted())
                        .unwrap_or(false)
                {
                    SessionState::InterruptedMigration(meta)
                } else {
                    SessionState::Locked(meta)
                }
            }
            None => {
                if let Some(marker) = marker_opt {
                    if marker.stage.is_interrupted() {
                        let salt = [0u8; crate::storage::crypto::REQUIRED_SALT_LEN_BYTES];
                        let kdf = KdfConfig::default_profile(salt);
                        let wrapped = crate::storage::crypto::metadata::WrappedKey::new(
                            &[0u8; crate::storage::crypto::NONCE_LEN_BYTES],
                            &[0u8; crate::storage::crypto::DEK_LEN_BYTES],
                            &[0u8; crate::storage::crypto::metadata::AEAD_TAG_LEN_BYTES],
                        );
                        let mut m = EncryptionMetadata::new(kdf, wrapped);
                        m.migration_status = MigrationStatus::MigrationInProgress;
                        SessionState::InterruptedMigration(m)
                    } else {
                        SessionState::Disabled
                    }
                } else {
                    SessionState::Disabled
                }
            }
        };

        let is_encrypted = !matches!(&new_state, SessionState::Disabled);

        let mut guard = self.state.write().unwrap();
        *guard = new_state;

        Ok(is_encrypted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::init_connection;
    use crate::storage::migrations::run_migrations;
    use crate::storage::models::CreateNoteDto;
    use crate::storage::repositories::NoteRepository;
    use crate::storage::services::RestoreService;

    #[test]
    fn test_encryption_setup_lock_unlock_and_change_password_workflow() {
        let temp_dir =
            std::env::temp_dir().join(format!("pn_crypto_session_test_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        // 1. Create a note in unencrypted database
        let unencrypted_note = NoteRepository::create(
            &conn,
            CreateNoteDto {
                title: Some("Pre-Encryption Note".to_string()),
                content: Some("Sensitive User Draft 12345".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(unencrypted_note.title, "Pre-Encryption Note");

        let session = CryptoSession::init_from_database(&conn).unwrap();
        assert!(!session.is_encryption_enabled());
        assert!(!session.is_locked());
        assert!(!session.is_interrupted());

        // 2. Setup encryption with password
        let password = "StrongPassword2026!";
        let setup_dto = SetupEncryptionDto {
            password: password.to_string(),
            confirm_password: password.to_string(),
        };

        let meta = session
            .setup_encryption(&conn, &paths, setup_dto)
            .expect("Setup should succeed");
        assert!(meta.enabled);
        assert_eq!(meta.migration_status, MigrationStatus::Active);
        assert!(session.is_encryption_enabled());
        assert!(!session.is_locked());
        assert!(!session.is_interrupted());

        // Verify pre-migration checkpoint was created
        let checkpoints = RestoreService::list_recovery_checkpoints(&paths).unwrap();
        assert_eq!(
            checkpoints.len(),
            1,
            "Verified pre-migration checkpoint must exist"
        );

        // Verify DEK is accessible when unlocked
        let active_dek = session.get_active_dek().unwrap();
        assert_eq!(active_dek.len(), 32);

        // Verify pre-existing note was safely migrated to ciphertext
        let raw_title: String = conn
            .query_row(
                "SELECT title FROM notes WHERE id = ?1",
                [&unencrypted_note.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            raw_title.starts_with("ENC$v1$"),
            "Pre-existing note title must be migrated to ciphertext envelope"
        );

        let decrypted_note =
            NoteRepository::get_by_id_with_key(&conn, &unencrypted_note.id, Some(&active_dek))
                .unwrap()
                .expect("Note must exist");
        assert_eq!(decrypted_note.title, "Pre-Encryption Note");
        assert_eq!(decrypted_note.content, "Sensitive User Draft 12345");

        // 3. Lock the application
        session.lock();
        assert!(session.is_locked());
        assert_eq!(session.get_active_dek(), Err(CryptoError::AppLocked));

        // 4. Try unlocking with incorrect password
        let wrong_unlock = session.unlock(UnlockDto {
            password: "WrongPassword!".to_string(),
        });
        assert_eq!(wrong_unlock, Err(CryptoError::AuthenticationFailed));
        assert!(
            session.is_locked(),
            "Application must remain locked after wrong password"
        );

        // 5. Unlock with correct password
        session
            .unlock(UnlockDto {
                password: password.to_string(),
            })
            .expect("Unlocking with correct password must succeed");
        assert!(!session.is_locked());
        let unlocked_dek = session.get_active_dek().unwrap();
        assert_eq!(unlocked_dek.as_slice(), active_dek.as_slice());

        // 6. Change password
        let new_password = "BrandNewSuperSecurePassword!";
        session
            .change_password(
                &conn,
                ChangePasswordDto {
                    current_password: password.to_string(),
                    new_password: new_password.to_string(),
                    confirm_new_password: new_password.to_string(),
                },
            )
            .expect("Password change should succeed");

        // 7. Lock again and verify old password fails and new password succeeds
        session.lock();
        assert!(session.is_locked());

        let old_pwd_res = session.unlock(UnlockDto {
            password: password.to_string(),
        });
        assert_eq!(old_pwd_res, Err(CryptoError::AuthenticationFailed));

        session
            .unlock(UnlockDto {
                password: new_password.to_string(),
            })
            .expect("New password must unlock");
        assert!(!session.is_locked());

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_interrupted_migration_detection_and_lockout() {
        let temp_dir = std::env::temp_dir().join(format!(
            "pn_crypto_interrupted_test_{}",
            uuid::Uuid::new_v4()
        ));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();

        let mut conn = init_connection(&paths.database_file).unwrap();
        run_migrations(&mut conn).unwrap();

        // Simulate an interrupted migration by saving metadata with MigrationInProgress
        let salt = generate_salt();
        let kdf_config = KdfConfig::default_profile(salt);
        let kek = derive_kek("TestPass123!", &kdf_config).unwrap();
        let dek = generate_dek();
        let wrapped_dek = wrap_dek(&dek, &kek).unwrap();

        let mut meta = EncryptionMetadata::new(kdf_config, wrapped_dek);
        meta.migration_status = MigrationStatus::MigrationInProgress;

        SettingsRepository::set(&conn, ENCRYPTION_METADATA_KEY, &meta.to_json().unwrap()).unwrap();

        // Boot session from database
        let session = CryptoSession::init_from_database(&conn).unwrap();
        assert!(session.is_encryption_enabled());
        assert!(session.is_locked());
        assert!(session.is_interrupted());
        assert_eq!(
            session.get_migration_status(),
            Some(MigrationStatus::MigrationInProgress)
        );

        // Attempting to unlock must fail with MigrationError
        let unlock_res = session.unlock(UnlockDto {
            password: "TestPass123!".to_string(),
        });
        assert!(matches!(unlock_res, Err(CryptoError::MigrationError(_))));

        // Attempting to get DEK must fail with MigrationError
        let dek_res = session.get_active_dek();
        assert!(matches!(dek_res, Err(CryptoError::MigrationError(_))));

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
