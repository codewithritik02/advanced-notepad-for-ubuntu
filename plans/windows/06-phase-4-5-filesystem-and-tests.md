# Phase 4 & 5 — Filesystem aur Tests

---

## PHASE 4 — Filesystem behaviour (0.5–1 din)
**Bugs:** W-05, W-06, W-08, W-20 · **Decision:** D-09

**Goal:** File rename/delete/replace, naam aur path issues Windows par data-safe hon.

### Sub-phase 4.1 — `fs_util` module
Naya: `src-tauri/src/storage/fs_util.rs`
- [ ] `rename_with_retry(from, to)`
- [ ] `remove_file_with_retry(path)` (read-only attribute clear karke)
- [ ] `remove_dir_all_with_retry(path)`
- [ ] `canonicalize_clean(path)` (`\\?\` strip jahan safe)
- [ ] Retry policy: sirf Windows par, errors `ERROR_SHARING_VIOLATION (32)`, `ERROR_LOCK_VIOLATION (33)`, `ERROR_ACCESS_DENIED (5)`; backoff 25, 50, 100, 200, 400 ms (max ~1 s); Linux par seedha pass-through
- [ ] Unit tests: Windows par ek handle open rakh kar rename try (retry exhaust → clean error), handle drop → success
- [ ] Error mapping: user-friendly message ("File is in use by another program (antivirus, backup, indexer). Try again.")

### Sub-phase 4.2 — Call sites migrate karo (priority order)
1. [ ] `storage/database.rs:159-183` — corrupt DB isolation. **Pehle ensure karo ki connection drop hui hai.** Test: corrupt DB ke saath startup Windows par recover.
2. [ ] `services/restore*` — restore flow: connection close → replace → reopen. Order audit.
3. [ ] `crypto/migration.rs:245,420` — encryption migration atomic renames.
4. [ ] `attachment_storage.rs:377` (temp → final), `:474` (`remove_file`), cleanup loops (`:652,685,705`)
5. [ ] `crypto/diagnostics.rs:276` — log rotation (agar log file open hai to rename fail; handle close karke rotate ya copy+truncate)
6. [ ] `storage/paths.rs::ensure_directories` — `decrypted_attachments_dir` remove_dir_all (agar koi file open ho)
7. [ ] `services/backup_retention.rs` — old backups delete
8. [ ] `import_export.rs::write_file_atomically` — atomic write (temp + rename; Windows par existing file open ho to retry)

### Sub-phase 4.3 — Filename safety (W-08)
- [ ] Pehle verify: attachments disk par **original naam** se store hote hain ya UUID se? (Audit karo; agar UUID to risk sirf export/restore mein.)
- [ ] `sanitize_for_windows(name)`:
  - invalid chars `< > : " / \ | ? *` aur control chars → `_`
  - reserved names (`CON, PRN, AUX, NUL, COM1–9, LPT1–9`, extension ke saath bhi) → prefix `_`
  - trailing dot/space strip
  - length limit (component 255, total path budget)
- [ ] Apply: export (zip entry, single file), attachments extract/restore, backup restore, import
- [ ] **Linux par bhi** export mein sanitize karo? Decision: haan, kyunki cross-OS portability (Linux se banaya zip Windows par khulna chahiye).
- [ ] Long paths: `longPathAware` manifest (Phase 6) + path length guard error
- [ ] Tests: adversarial names (`con.txt`, `a:b.md`, `x.`, `COM1`, 300-char names, emoji, Hindi/Unicode)
- [ ] Existing `unicode_input_validation_adversarial_test.rs` extend

### Sub-phase 4.4 — Path identity (W-20)
- [ ] External documents registry: `path_key(path)` (Windows: lowercase + normalized separators)
- [ ] DB schema change **nahi** chahiye agar sirf comparison key lookup mein badle (migration avoid). Agar chahiye to proper migration + test.

### Sub-phase 4.5 — Data directory (W-10, D-08)
- [ ] Final decision ke hisaab se `resolve_app_data_root`
- [ ] Windows par pehle se created dir/ACL check
- [ ] Docs/README table update

### Exit criteria
- Windows par corrupt-DB recovery, restore, migration, log rotation, attachment add/remove sab pass.
- Linux par retry-helper pass-through, koi regression nahi.

---

## PHASE 5 — Tests cross-platform (0.5–1 din)
**Bugs:** W-07, W-14, W-15

**Goal:** `cargo test` aur `npm test` Windows par green.

### Sub-phase 5.1 — Rust test helpers
- [ ] `tests/common/mod.rs` mein `abs_path("tmp/a.md")` helper: Windows par `C:\tmp\a.md`, Unix par `/tmp/a.md`
- [ ] Tests jo `/tmp/...` strings ko `is_absolute()` ke liye use karte hain:
  - [ ] `open_files.rs` tests (lines ~691–762, 883–891, 948–990)
  - [ ] `paths.rs:415`
- [ ] `/bin/sh` wali injection-test string (`open_files.rs:761`) — sirf data hai, theek, lekin assertion `is_absolute()` par depend hai, check
- [ ] Unix-only tests ko `#[cfg(unix)]` (already kai jagah), aur **Windows-only equivalents** add (symlink Windows par admin/dev mode maangta hai → skip with message)
- [ ] `socket_path_in` tests Unix-only

### Sub-phase 5.2 — Windows-specific naye tests
- [ ] Instance lock (Phase 2)
- [ ] `file_uri_to_path` Windows cases (Phase 3, shared logic)
- [ ] `sanitize_for_windows` (Phase 4)
- [ ] rename/remove retry (Phase 4)
- [ ] Path identity case-insensitive (Phase 4)

### Sub-phase 5.3 — Frontend + scripts
- [ ] `node scripts/check-test-safety.mjs` Windows par (path separators)
- [ ] `scripts/lint-frontend.mjs` path handling (`path.join`, `\\` splitting?)
- [ ] `scripts/test-editor-browser.mjs`: Chrome path `EDITOR_TEST_BROWSER` env se (`C:\Program Files\Google\Chrome\Application\chrome.exe`); Windows CI par non-blocking pehle
- [ ] `scripts/generate-icons.py` — Windows par Python path, sirf dev tool, docs mein note
- [ ] Frontend tests mein exact-string/line-ending sensitive asserts check

### Sub-phase 5.4 — Stability
- [ ] Windows par `cargo test` 3 baar chalao (flaky tests pakadne ke liye, specially file-locking waale)
- [ ] Test duration measure (Windows Defender real-time scanning tests ko slow karta hai; `%TEMP%` ko exclusion CI mein nahi lagta, to timeouts check)
- [ ] Parallelism: `--test-threads` Windows CI par tune

### Exit criteria
- Windows par: `npm test`, `npm run lint`, `cargo clippy`, `cargo test` sab green (ya known-skips documented).
- Skipped tests ki list reasons ke saath document.
