/**
 * Strongly-typed domain models and DTOs corresponding to SQLite schema entities.
 */

export type NoteFormat = "txt" | "md";

/**
 * Strongly-typed lifecycle state of a note (Phase 8, Task 2)
 */
export type NoteLifecycle = "active" | "trashed";

/**
 * Derives the authoritative NoteLifecycle state from the is_deleted flag.
 */
export function getNoteLifecycle(note: { is_deleted: boolean }): NoteLifecycle {
  return note.is_deleted ? "trashed" : "active";
}

export interface Note {
  id: string;
  title: string;
  content: string;
  format: NoteFormat | string;
  notebook_id: string | null;
  created_at: string;
  modified_at: string;
  is_favorite: boolean;
  is_pinned: boolean;
  is_deleted: boolean;
  deleted_at: string | null;
}

/**
 * Lightweight DTO representation for trashed notes listing (Phase 8, Task 10)
 */
export interface TrashedNoteItem {
  id: string;
  title: string;
  preview: string;
  notebookId: string | null;
  notebookName?: string | null;
  createdAt: string;
  modifiedAt: string;
  deletedAt: string | null;
  isFavorite: boolean;
  attachmentCount: number;
}

/**
 * Result report of a single permanent deletion operation (Phase 8, Task 7)
 */
export interface PermanentDeleteResult {
  noteId: string;
  success: boolean;
  attachmentCleanupFailed: boolean;
  errorMessage?: string | null;
}

/**
 * Summary report of an empty-trash batch operation (Phase 8, Task 8)
 */
export interface EmptyTrashResult {
  deletedCount: number;
  failedCount: number;
  attachmentCleanupFailures: string[];
  errors: string[];
}

export interface NoteMetadata {
  note_id: string;
  created_at: string;
  modified_at: string;
  format: string;
  notebook_id: string | null;
  notebook_name?: string | null;
  notebook_path?: string | null;
  is_favorite: boolean;
  is_pinned: boolean;
  tags: Tag[];
  word_count: number;
  character_count: number;
  byte_size: number;
  attachment_count?: number;
}

export interface ListNotesOptions {
  includeDeleted?: boolean;
  notebookId?: string | null;
  unfiledOnly?: boolean;
  favoritesOnly?: boolean;
}

export interface CreateNoteInput {
  title?: string;
  content?: string;
  format?: "txt" | "md";
  notebook_id?: string | null;
  is_favorite?: boolean;
  is_pinned?: boolean;
}

export interface UpdateNoteInput {
  title?: string;
  content?: string;
  format?: "txt" | "md";
  notebook_id?: string | null;
  is_favorite?: boolean;
  is_pinned?: boolean;
  is_deleted?: boolean;
}

export interface Notebook {
  id: string;
  name: string;
  parent_id: string | null;
  created_at: string;
  modified_at: string;
}

export interface CreateNotebookInput {
  name: string;
  parent_id?: string | null;
}

export interface UpdateNotebookInput {
  name: string;
}

export interface Tag {
  id: string;
  name: string;
  created_at: string;
}

export interface CreateTagInput {
  name: string;
}

export interface UpdateTagInput {
  name: string;
}

export interface NoteTagAssignment {
  note_id: string;
  tag_id: string;
}

export interface SetNoteTagsInput {
  note_id: string;
  tag_ids: string[];
}

export interface SearchQuery {
  query: string;
  limit?: number;
}

export interface SearchResult {
  noteId: string;
  title: string;
  snippet?: string;
  modifiedAt: string;
  notebookId: string | null;
  favorite: boolean;
}

export interface Attachment {
  id: string;
  note_id: string;
  file_name: string;
  relative_path?: string;
  mime_type: string | null;
  file_size: number;
  created_at: string;
  modified_at?: string;
  // Convenience aliases for specification compatibility
  original_name?: string;
  size_bytes?: number;
}

export interface AddAttachmentInput {
  note_id: string;
  source_path: string;
}

export interface CreateAttachmentInput {
  note_id: string;
  file_name: string;
  relative_path: string;
  mime_type: string | null;
  file_size: number;
}

export interface MissingManagedFileIssue {
  attachment_id: string;
  note_id: string;
  file_name: string;
  relative_path: string;
}

export interface SizeMismatchIssue {
  attachment_id: string;
  note_id: string;
  file_name: string;
  relative_path: string;
  recorded_size: number;
  actual_size: number;
}

export interface AttachmentConsistencyReport {
  missing_managed_files: MissingManagedFileIssue[];
  unreferenced_managed_files: string[];
  abandoned_temp_files: string[];
  size_mismatches: SizeMismatchIssue[];
  total_attachments_checked: number;
  total_disk_files_checked: number;
  is_consistent: boolean;
}


export interface Setting {
  key: string;
  value: string;
}

export interface StorageInfo {
  data_dir: string;
  database_file: string;
  is_initialized: boolean;
}

/**
 * Strongly-typed lifecycle status of an internal link target (Phase 9, Task 3)
 */
export type LinkTargetLifecycle = "active" | "trashed" | "missing";

/**
 * Representation of a parsed internal link token in note content
 */
export interface ParsedInternalLink {
  targetId: string;
  label: string;
  raw: string;
  startIndex: number;
  endIndex: number;
}

/**
 * Resolved target metadata for following or inspecting an internal link
 */
export interface ResolvedLinkTarget {
  targetId: string;
  title: string | null;
  lifecycle: LinkTargetLifecycle;
  notebookId: string | null;
  notebookName?: string | null;
}

/**
 * Lightweight search candidate for selecting a note to link to (Note Picker)
 */
export interface LinkableNoteItem {
  id: string;
  title: string;
  preview: string;
  notebookId: string | null;
  notebookName?: string | null;
  modifiedAt: string;
}

/**
 * Backlink entry indicating a source note that links to the target note
 */
export interface BacklinkItem {
  sourceNoteId: string;
  sourceNoteTitle: string;
  sourceNotebookId: string | null;
  sourceNotebookName?: string | null;
  labelSnippet?: string | null;
  modifiedAt: string;
}

/**
 * Input parameters for inserting an internal link into editor content
 */
export interface InsertInternalLinkInput {
  targetNoteId: string;
  targetTitle: string;
  customLabel?: string;
}

// ==========================================
// Phase 10: Import & Export Contracts
// ==========================================

export type ImportFormat = "txt" | "md" | "json";
export type ExportFormat = "txt" | "md" | "json";
export type DuplicatePolicy = "import_as_new" | "skip_exact" | "ask";
export type DuplicateMatch =
  | { type: "exactMatch"; existingId: string; existingTitle: string }
  | { type: "contentMatch"; existingId: string; existingTitle: string }
  | { type: "titleCollisionOnly"; existingId: string; existingTitle: string }
  | { type: "none" };
export type OperationOutcome =
  | "full_success"
  | "success_with_warnings"
  | "partial_success"
  | "cancelled"
  | "total_failure";

export type ValidationStatus = "valid" | "warning" | "invalid";
export type ImportCandidateStatus = ValidationStatus;
export type ImportItemStatus = "imported" | "skipped" | "rejected" | "cancelled" | "failed";
export type ExportItemStatus = "exported" | "skipped" | "cancelled" | "failed";

export interface ImportProgress {
  operationId?: string | null;
  total: number;
  currentIndex: number;
  currentFile: string;
  importedCount: number;
  skippedCount: number;
  failedCount: number;
  cancelledCount: number;
}

export interface ExportProgress {
  operationId?: string | null;
  total: number;
  currentIndex: number;
  currentTitle: string;
  exportedCount: number;
  skippedCount: number;
  failedCount: number;
  cancelledCount: number;
}

export interface ImportCandidate {
  id: string;
  filePath: string;
  fileName: string;
  fileSize: number;
  detectedFormat: ImportFormat;
  proposedTitle: string;
  validationStatus: ValidationStatus;
  warnings: string[];
  errorMessage?: string | null;
  isDuplicate: boolean;
  duplicateNoteId?: string | null;
  duplicateNoteTitle?: string | null;
  selected: boolean;
}

export interface ImportPreview {
  candidates: ImportCandidate[];
  totalFiles: number;
  validFiles: number;
  invalidFiles: number;
  duplicateFiles: number;
  totalSizeBytes: number;
}

export interface ImportOptions {
  targetNotebookId?: string | null;
  duplicatePolicy: DuplicatePolicy;
}

export interface ImportCandidateOverride {
  id: string;
  filePath: string;
  selected: boolean;
  customTitle?: string | null;
}

export interface ImportRequest {
  operationId?: string;
  candidates: ImportCandidateOverride[];
  options: ImportOptions;
}

export interface ImportFileResult {
  candidateId: string;
  fileName: string;
  status: ImportItemStatus;
  createdNoteId?: string | null;
  message?: string | null;
}

export interface ImportSummary {
  totalProcessed: number;
  importedCount: number;
  skippedCount: number;
  failedCount: number;
  cancelledCount: number;
  outcome?: OperationOutcome;
  results: ImportFileResult[];
  durationMs: number;
}

export interface ExportSingleNoteRequest {
  noteId: string;
  format: ExportFormat;
  destinationPath: string;
  overwrite?: boolean;
}

export type ExportScope =
  | { type: "single_note"; note_id: string }
  | { type: "selected_notes"; note_ids: string[] }
  | { type: "notebook"; notebook_id: string; include_nested: boolean }
  | { type: "all_notes" };

export interface ExportNoteMetadataItem {
  id: string;
  title: string;
  format: string;
  notebookId?: string | null;
  notebookPath?: string | null;
  isFavorite: boolean;
  isPinned: boolean;
  tagCount: number;
  contentLengthBytes: number;
  modifiedAt: string;
}

export interface ExportSelectionPreview {
  totalRequested: number;
  totalIncluded: number;
  skippedTrashedCount: number;
  skippedMissingCount: number;
  notes: ExportNoteMetadataItem[];
  includedNotebookCount: number;
}

export interface ExportBatchRequest {
  operationId?: string;
  scope?: ExportScope;
  noteIds?: string[];
  format: ExportFormat;
  destinationDir?: string;
  destinationFile?: string | null;
  includeMetadata?: boolean;
  overwrite?: boolean;
}

export interface ExportFileResult {
  noteId: string;
  noteTitle: string;
  filePath: string;
  status: ExportItemStatus;
  message?: string | null;
}

export interface ExportSummary {
  totalNotes: number;
  exportedCount: number;
  skippedCount: number;
  failedCount: number;
  cancelledCount: number;
  outcome?: OperationOutcome;
  results: ExportFileResult[];
  durationMs: number;
}

export interface InterchangeInternalLink {
  label: string;
  target_export_id: string;
  is_package_internal: boolean;
}

export interface InterchangeNote {
  export_id: string;
  title: string;
  content: string;
  format: string;
  notebook_path?: string | null;
  tags?: string[];
  is_favorite?: boolean;
  is_pinned?: boolean;
  created_at?: string | null;
  modified_at?: string | null;
  attachments_omitted_count?: number;
  internal_links?: InterchangeInternalLink[];
}

export interface InterchangeDocument {
  $schema?: string;
  app: string;
  format?: string;
  version: string;
  exported_at: string;
  note_count: number;
  internal_link_policy?: string;
  attachment_policy?: string;
  notes: InterchangeNote[];
}

/**
 * =========================================================================
 * Phase 11 — Backup & Data Safety Types and DTOs (Task 4)
 * =========================================================================
 */

export type BackupType = "manual" | "automatic" | "checkpoint";

export interface BackupDatabaseInfo {
  entry_path: string;
  file_size: number;
  sha256: string;
  schema_version: number;
  note_count: number;
  notebook_count: number;
  tag_count: number;
  trash_count: number;
}

export interface BackupAttachmentsSummary {
  total_files: number;
  total_bytes: number;
}

export interface BackupAttachmentItem {
  id: string;
  note_id: string;
  file_name: string;
  relative_path: string;
  file_size: number;
  mime_type: string | null;
  sha256: string;
}

export interface BackupScopeDeclaration {
  includes_settings: boolean;
  includes_trash: boolean;
  includes_attachments: boolean;
}

export interface BackupManifest {
  format_identifier: string;
  format_version: number;
  app_version: string;
  created_at: string;
  backup_type: BackupType;
  database: BackupDatabaseInfo;
  attachments_summary: BackupAttachmentsSummary;
  attachments: BackupAttachmentItem[];
  scope: BackupScopeDeclaration;
  checksums: Record<string, string>;
}

export interface BackupCreateRequest {
  destinationPath?: string | null;
  backupType?: BackupType | null;
  allowOverwrite?: boolean | null;
}

export interface BackupCreateResult {
  success: boolean;
  filePath: string;
  fileName: string;
  fileSize: number;
  createdAt: string;
  backupType: BackupType;
  verified: boolean;
  noteCount: number;
  attachmentCount: number;
  warningMessage?: string | null;
}

export interface BackupListItem {
  filePath: string;
  fileName: string;
  fileSize: number;
  createdAt: string;
  backupType: BackupType;
  formatVersion?: number;
  schemaVersion?: number;
  noteCount?: number;
  notebookCount?: number;
  attachmentCount?: number;
  isVerified: boolean;
  isValid: boolean;
  issueSummary?: string | null;
}

export type BackupValidationIssueSeverity = "warning" | "error";

export interface BackupValidationIssue {
  severity: BackupValidationIssueSeverity;
  code: string;
  message: string;
}

export type BackupCompatibilityStatus =
  | "compatible"
  | "requiresMigration"
  | "incompatibleVersion"
  | "corrupted";

export interface BackupValidationResult {
  isValid: boolean;
  compatibility: BackupCompatibilityStatus;
  manifest?: BackupManifest | null;
  issues: BackupValidationIssue[];
}

export interface RestorePreviewResult {
  filePath: string;
  fileName: string;
  fileSize: number;
  createdAt: string;
  backupType: BackupType;
  formatVersion: number;
  schemaVersion: number;
  noteCount: number;
  notebookCount: number;
  tagCount: number;
  trashCount: number;
  attachmentCount: number;
  attachmentTotalBytes: number;
  includesSettings: boolean;
  compatibility: BackupCompatibilityStatus;
  warnings: string[];
  canRestore: boolean;
}

export interface BackupRestoreRequest {
  filePath: string;
  operationId?: string;
}

export interface PostRestoreValidationReport {
  isValid: boolean;
  integrityCheckPassed: boolean;
  foreignKeyCheckPassed: boolean;
  schemaVersion: number;
  noteCount: number;
  notebookCount: number;
  tagCount: number;
  trashCount: number;
  attachmentCount: number;
  attachmentsVerifiedOnDisk: boolean;
  ftsRebuiltAndQueryable: boolean;
  internalLinksVerified: boolean;
  settingsVerified: boolean;
}

export interface BackupRestoreResult {
  success: boolean;
  restoredAt: string;
  noteCount: number;
  notebookCount: number;
  tagCount: number;
  trashCount: number;
  attachmentCount: number;
  checkpointPath: string;
  warningMessage?: string | null;
  validationReport?: PostRestoreValidationReport | null;
}

export type BackupProgressStage =
  | "starting"
  | "snapshottingDatabase"
  | "collectingAttachments"
  | "generatingManifest"
  | "writingArchive"
  | "verifyingArchive"
  | "applyingRetention"
  | "extractingStaging"
  | "preparingStagingData"
  | "checkingDatabaseIntegrity"
  | "checkingAttachments"
  | "validatingBackup"
  | "validatingStagedData"
  | "creatingCheckpoint"
  | "preparingActivation"
  | "activatingRestoredData"
  | "verifyingRestoredState"
  | "rebuildingSearchIndex"
  | "completed"
  | "failed"
  | "failedWithRecoveryAvailable"
  | "failedAndRecoveryRequired"
  | "cancelled";

export interface BackupProgressEvent {
  stage: BackupProgressStage;
  message: string;
  currentItem?: number | null;
  totalItems?: number | null;
  percentage?: number | null;
  cancellable?: boolean;
  operationId?: string | null;
}

export interface BackupSettings {
  autoBackupEnabled: boolean;
  intervalMinutes: number;
  backupDestinationDir?: string | null;
  retentionCount: number;
  backupOnStartup: boolean;
}

export type BackupHealthLevel = "healthy" | "warning" | "critical" | "disabled";

export interface BackupHealthWarning {
  code: string;
  level: "warning" | "critical" | "info";
  message: string;
  detail?: string | null;
}

export interface BackupHealthStatus {
  healthLevel: BackupHealthLevel;
  autoBackupEnabled: boolean;
  destinationAvailable: boolean;
  destinationPath: string;
  isSameDisk: boolean;
  lastAttemptAt?: string | null;
  lastSuccessfulAt?: string | null;
  lastFailureCategory?: string | null;
  lastErrorMessage?: string | null;
  lastRetentionWarning?: string | null;
  failureCount: number;
  managedBackupCount: number;
  hasVerifiedBackup: boolean;
  latestBackupName?: string | null;
  latestBackupVerified: boolean;
  warnings: BackupHealthWarning[];
}

export interface BackupSchedulerState {
  lastAttemptAt?: string | null;
  lastSuccessfulAt?: string | null;
  lastErrorMessage?: string | null;
  lastFailureCategory?: string | null;
  lastRetentionWarning?: string | null;
  failureCount: number;
}

/**
 * Detailed metadata for an authoritative pre-restore recovery checkpoint (Phase 11, Task 16).
 */
export interface RecoveryCheckpoint {
  checkpointId: string;
  filePath: string;
  fileName: string;
  fileSize: number;
  createdAt: string;
  noteCount: number;
  trashCount: number;
  notebookCount: number;
  tagCount: number;
  attachmentCount: number;
  attachmentTotalBytes: number;
  schemaVersion: number;
  isVerified: boolean;
}

/**
 * Authoritative summary of an isolated staged archive extraction (Phase 11, Task 17).
 */
export interface StagedRestoreSummary {
  stagingId: string;
  stagingDir: string;
  databasePath: string;
  manifestPresent: boolean;
  totalExtractedFiles: number;
  totalExtractedBytes: number;
  noteCount: number;
  notebookCount: number;
  tagCount: number;
  trashCount: number;
  attachmentCount: number;
  initialSchemaVersion: number;
  isValid: boolean;
}

/**
 * Result of running database schema migrations on a staged database before activation (Phase 11, Task 18).
 */
export interface StagedMigrationResult {
  stagingId: string;
  stagingDir: string;
  previousSchemaVersion: number;
  migratedSchemaVersion: number;
  migrationsApplied: number;
  noteCount: number;
  trashCount: number;
  notebookCount: number;
  tagCount: number;
  attachmentCount: number;
  isVerified: boolean;
}


