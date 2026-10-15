import { invoke } from "@tauri-apps/api/core";

export interface EncryptionStatusDto {
  enabled: boolean;
  is_locked: boolean;
  algorithm?: string | null;
  kdf_algorithm?: string | null;
  format_version?: number | null;
  created_at?: string | null;
  migration_status?: string | null;
  is_interrupted?: boolean;
  recovery_checkpoint_path?: string | null;
}

export interface EncryptionSetupResult {
  success: boolean;
  format_version: number;
  algorithm: string;
  message: string;
}

export interface SetupEncryptionDto {
  password: string;
  confirm_password: string;
}

export interface UnlockDto {
  password: string;
}

export interface ChangePasswordDto {
  current_password: string;
  new_password: string;
  confirm_new_password: string;
}

export interface SecurityLogEntry {
  timestamp: string;
  code: string;
  level: "Info" | "Warning" | "Error" | "Critical";
  message: string;
  details?: string | null;
}

export const cryptoStorage = {
  /**
   * Retrieves current encryption status and lock state.
   */
  async getEncryptionStatus(): Promise<EncryptionStatusDto> {
    return await invoke<EncryptionStatusDto>("get_encryption_status");
  },

  /**
   * Enables encryption on the current installation with pre-migration recovery checkpoint.
   */
  async setupEncryption(dto: SetupEncryptionDto): Promise<EncryptionSetupResult> {
    return await invoke<EncryptionSetupResult>("setup_encryption", { dto });
  },

  /**
   * Unlocks the application using the supplied encryption password.
   */
  async unlockApplication(dto: UnlockDto): Promise<boolean> {
    return await invoke<boolean>("unlock_application", { dto });
  },

  /**
   * Manually locks the application and purges the DEK from memory.
   */
  async lockApplication(): Promise<boolean> {
    return await invoke<boolean>("lock_application");
  },

  /**
   * Changes encryption password by re-wrapping the DEK.
   */
  async changePassword(dto: ChangePasswordDto): Promise<boolean> {
    return await invoke<boolean>("change_encryption_password", { dto });
  },

  /**
   * Safely rolls back an interrupted migration by restoring the pre-migration backup checkpoint.
   */
  async rollbackInterruptedMigration(): Promise<string> {
    return await invoke<string>("rollback_interrupted_migration");
  },

  /**
   * Resumes an interrupted migration by supplying the master encryption password.
   */
  async resumeInterruptedMigration(dto: UnlockDto): Promise<boolean> {
    return await invoke<boolean>("resume_interrupted_migration", { dto });
  },

  /**
   * Disables encryption, decrypting all database notes and attachments on disk after creating a verified recovery checkpoint (Phase 12, Task 21).
   */
  async disableEncryption(dto: UnlockDto): Promise<boolean> {
    return await invoke<boolean>("disable_encryption", { dto });
  },

  /**
   * Retrieves security audit log entries for diagnostics without exposing sensitive secrets (Phase 12, Task 22).
   */
  async getSecurityDiagnostics(limit?: number): Promise<SecurityLogEntry[]> {
    return await invoke<SecurityLogEntry[]>("get_security_diagnostics", { limit });
  },
};
