# 02 — Windows Bug & Risk Register

Severity: **S1** = data loss / data corruption risk · **S2** = feature kaam nahi karega · **S3** = edge case / quality · **S4** = infra/process
Confidence: **V** = code padh kar verify kiya · **L** = likely, Windows par confirm karna hai

---

## W-01 · Instance lock `/proc` par depend karta hai — S1 · V

- **Jagah:** `src-tauri/src/storage/paths.rs:310-330` (`AppInstanceLock::acquire`)
- **Problem:** Existing PID zinda hai ya nahi, ye `Path::new("/proc/{pid}").exists()` se check hota hai. Windows par `/proc` hota hi nahi, to `is_running` hamesha `false`.
- **Asar:** Lock kabhi "already running" nahi batata. Do instance ek saath DB, WAL aur attachments kholenge. SQLite WAL file-locking kuch bachayegi, lekin encryption migration, backup/restore, decrypted-attachments dir cleanup (`ensure_directories` `decrypted_dir` delete karta hai!) ek doosre ke saath takra sakte hain.
- **Extra weakness (Linux par bhi):** PID reuse. Crash ke baad stale PID kisi doosre process ka ho sakta hai to false "already running".
- **Fix:** PID file ki jagah **OS-level exclusive file lock** (`File::try_lock` stable in Rust ≥1.89, ya `fs4`/`fd-lock` crate). Process crash par OS khud lock chhod deta hai, stale-PID problem khatam. PID file sirf informational rakhni hai.
- **Phase:** 2

## W-02 · Single-instance forwarding (Unix socket) Windows par nahi — S2 · V

- **Jagah:** `lib.rs:15-20` (`forward_launch`), `lib.rs:45-72` (`start_listener`), `open_files.rs:585+` (`mod ipc`, sab `#[cfg(unix)]`)
- **Problem:** Windows par dusri baar app kholne par ya `.md` double-click karne par:
  - Nayi process start hoti hai, instance lock (W-01 fix ke baad) fail hota hai aur user ko error milta hai.
  - Ya (W-01 fix ke bina) do window khulti hain.
  - "Open with" se file running window mein nahi pahunchti.
- **Fix:** Windows par official `tauri-plugin-single-instance` (cfg(windows) mein), callback mein argv + cwd milta hai → existing `parse_launch_paths` + `PendingOpenFiles::push_all` + window focus + `OPEN_FILES_QUEUED_EVENT` reuse. Linux ka socket code **jaisa hai waisa rahega**.
- **Phase:** 3

## W-03 · `default_data_dir()` Windows par `None` — S2 · V

- **Jagah:** `open_files.rs:506-514`
- **Problem:** Sirf `XDG_DATA_HOME`/`HOME` padhta hai. Windows par `HOME` set nahi hota.
- **Asar:** Agar koi code is function par depend karta raha to Windows par silently None. Abhi sirf `cfg(unix)` caller hai, isliye latent bug.
- **Fix:** Function ko `cfg(unix)` mein rakho ya Windows ke liye `%APPDATA%\<identifier>` fallback do. Plugin approach mein iski zaroorat nahi, to unused-code warning se bacho (clippy `-D warnings` CI mein).
- **Phase:** 3

## W-04 · `file://` URI parsing Unix-style — S3 · V

- **Jagah:** `open_files.rs:83-100` (`argument_to_path`)
- **Problem:** `file:///C:/Users/me/a.md` → `rest = "/C:/Users/me/a.md"` → `PathBuf::from("/C:/Users/...")` galat path. UNC (`file://server/share/...`) bhi handle nahi.
- **Fix:** `#[cfg(windows)]` branch: leading `/` strip kar ke drive letter detect karo (`/C:/` → `C:\`), forward slashes ko backslash, UNC support ya clearly reject. Plain argv paths (Windows Explorer actual mein plain path deta hai) already chalte hain.
- **Phase:** 3

## W-05 · `canonicalize()` Windows par `\\?\` prefix deta hai — S3 · L

- **Jagah:** `open_files.rs:471` (`open_one`), `open_files.rs:522` (`socket_path`), `attachment_storage.rs:158,513,524,559,584`, `paths.rs:24-27`
- **Problem:** Windows par `std::fs::canonicalize` verbatim path (`\\?\C:\...`) return karta hai.
  - External documents ka stored `path` DB mein `\\?\` prefix ke saath aayega. UI mein ugly dikhega, aur same file ko alag tareeke se open karne par duplicate record ban sakta hai.
  - `starts_with` comparisons (agar mixed canonical/non-canonical ho) fail ho sakte hain.
- **Fix:** `dunce::canonicalize` (ya helper jo `\\?\` strip kare jab safe ho) use karo, ek jagah wrapper `fs_util::canonicalize`. DB mein path store karne se pehle normalize.
- **Phase:** 3, 4

## W-06 · Open file ko rename/delete nahi kar sakte (file-sharing semantics) — S1/S3 · L

- **Jagah:**
  - `database.rs:150-190` (`isolate_corrupt_database`, DB/WAL/SHM rename)
  - `crypto/migration.rs:245,420` (staging → final rename)
  - `attachment_storage.rs:377` (temp → final), `:474` (`remove_file`)
  - `storage/paths.rs` `ensure_directories` (`remove_dir_all(decrypted_dir)`)
  - `crypto/diagnostics.rs:276` (log rotation rename)
  - `services/backup_retention.rs`, restore flow
- **Problem:** Linux par open file ko rename/unlink kar sakte ho. Windows par default mein `ERROR_SHARING_VIOLATION` / `ACCESS_DENIED`. Antivirus (Defender), Search Indexer aur backup tools bhi file ko short time ke liye pakad lete hain.
- **Asar:** Corrupt-DB recovery fail ho sakti hai exactly us waqt jab zaroorat hai. Backup restore (jo DB replace karta hai) fail ho sakta hai agar connection abhi band nahi hui. Log rotation fail ho sakti hai.
- **Fix:**
  1. Rename/replace se pehle sab handles explicitly drop/close karo (connection close order audit).
  2. Ek `fs_util::rename_with_retry` / `remove_with_retry` helper: Windows par `ERROR_SHARING_VIOLATION`/`ACCESS_DENIED` par 5–8 baar, 25→400 ms backoff.
  3. Read-only attribute wali file delete: attribute clear karke retry.
  4. Restore + corrupt-isolation flows ke liye Windows-specific tests.
- **Phase:** 4

## W-07 · Tests mein Unix hardcoded paths aur APIs — S4 · V

- **Jagah (examples):**
  - `open_files.rs:691-762, 883-891, 948-990` (`/tmp/...`, `/home/me/...`, `/bin/sh`)
  - `paths.rs:415` (`/tmp/app_data/attachments`)
  - `attachment_storage.rs:1219-1232` (`PermissionsExt`, already `cfg(unix)`)
  - `backup_retention.rs:383-386` (`std::os::unix::fs::symlink`, already `cfg(unix)`)
  - `services/attachments.rs:1213` (`"/tmp"` as traversal id, theek hai)
  - `storage/models.rs:1752,1772` (sirf string fixtures, harmless)
- **Problem:** `Path::new("/tmp/a.md").is_absolute()` Windows par **false** hai (drive letter nahi). To `decode_message` wale tests fail honge. `cwd = "/"`, `"/home/me/work"` joins bhi alag result denge.
- **Fix:** Test helper `abs(&str)` jo platform ke hisaab se `C:\tmp\a.md` ya `/tmp/a.md` banata hai; `tempfile`/`TestAppDir` se asli temp paths; Unix-only tests ko `#[cfg(unix)]`; Windows-only tests alag.
- **Phase:** 5

## W-08 · Reserved device names aur lambe paths — S3 · L

- **Jagah:** `attachment_storage.rs` (filename sanitization), `services/manifest.rs:184`, import/export (zip entry names), note export file naming
- **Problem:** Windows par `CON, PRN, AUX, NUL, COM1-9, LPT1-9` (extension ke saath bhi, jaise `con.txt`), trailing dot/space wale naam, `< > : " | ? *` characters, aur 260-char MAX_PATH. Linux par ye sab valid filenames hain.
- **Asar:** User ne jo attachment Linux par add kiya (naam `aux.pdf`), backup/zip Windows par restore karte waqt fail. Export notes ke titles (jaise `Meeting: Q4?`) file name banate waqt fail.
- **Fix:** `sanitize_filename_for_windows()` — storage ke andar to UUID-based naam rakho (check karna hai), aur sirf export/extract ke time naam sanitize karo. Long path: `\\?\` ya Windows 10 long-path manifest (Tauri mein `longPathAware` manifest add karna). Zip extract ke liye conflict/rename policy.
- **Phase:** 4

## W-09 · Tauri window-destroy aur socket cleanup unconditional — S3 · V

- **Jagah:** `lib.rs:78` (`open_files::remove_socket_file()` — `cfg` ke bina), `lib.rs:70-72` (`remember_socket_for_cleanup` sirf `cfg(unix)` block mein)
- **Problem:** Agar `remove_socket_file` / `remember_socket_for_cleanup` Windows par define hi nahi hain to **compile error**. Agar define hain to dead-code warning (CI mein `clippy` fail kar sakti hai agar `-D warnings`).
- **Fix:** Windows build mein jo bhi compile ho, uski `cfg` consistency saaf karo. Phase 1 mein compiler batayega.
- **Phase:** 1, 3

## W-10 · Data dir Roaming (`%APPDATA%`) mein, SQLite ke liye risky — S3 · L

- **Jagah:** Tauri `app_data_dir()` → `C:\Users\<u>\AppData\Roaming\com.personalnotepad.app`
- **Problem:** Roaming profile ko enterprise/domain PCs par network share par sync kiya ja sakta hai. SQLite WAL network drive par safe nahi.
- **Decision pending:** Local (`%LOCALAPPDATA%`) vs Roaming. README already `%APPDATA%` bolta hai. Linux data path `~/.local/share/...` ke saath parity ke liye Roaming default hai, lekin **notes DB ke liye Local behtar** hai. Dekho `03-decisions.md` D-08.
- **Phase:** 1 (decision), 6 (docs)

## W-11 · Single-instance + encrypted lock state par window events — S3 · L

- **Jagah:** `lib.rs:76-92` (`WindowEvent::Destroyed` → WAL checkpoint, socket remove)
- **Problem:** Windows par shutdown (logoff/reboot) ke waqt process jaldi kill ho sakta hai, `Destroyed` event na aaye. Checkpoint skip.
- **Asar:** Next launch par WAL recover hota hai (SQLite safe hai), lekin `.app_instance.pid` stale reh sakta hai. W-01 ka naya file-lock approach isse bhi theek karta hai.
- **Fix:** Lock = OS file lock (auto release). WAL recovery test on forced kill.
- **Phase:** 2, 8

## W-12 · WebView2 dependency aur offline claim — S2 · V(config)/L(runtime)

- **Jagah:** `tauri.conf.json` bundle, README ("works fully offline")
- **Problem:** Windows par app **WebView2 Runtime** par chalti hai. Windows 11 mein pehle se hai; Windows 10 mein kuch machines par nahi. Default Tauri installer internet se bootstrapper download karta hai, offline machine par install fail.
- **Fix:** `webviewInstallMode`: offline installer / embedded bootstrapper (installer size +~1.8 MB bootstrapper ya ~120 MB offline). Decision `03-decisions.md` D-05.
- **Phase:** 6

## W-13 · Installer/uninstaller se user data delete hone ka risk — S1 · L

- **Jagah:** Tauri NSIS template ka "Delete the application data" uninstall checkbox
- **Problem:** README ka promise: **"Your data is never deleted when you install, upgrade or uninstall."** NSIS uninstaller mein option hota hai jo app data folder delete karta hai. Default unchecked hota hai, lekin silent uninstall (`/S`) ya custom template mein behaviour verify karna zaroori hai.
- **Fix:** Custom NSIS hook ya config se data delete option disable. Fresh install / upgrade / uninstall / reinstall test (Phase 8).
- **Phase:** 6, 8

## W-14 · Line endings (CRLF/LF) — S3 · L

- **Jagah:** repo (`.gitattributes` nahi hai), `scripts/*.mjs`, snapshot/expected-string tests, `open_files.rs:455-460` (CRLF preserve logic)
- **Problem:** Windows par git `core.autocrlf=true` ho to checkout par files CRLF ban jaati hain. Tests jo exact string compare karte hain (frontend tests, Rust fixtures `include_str!`) fail ho sakte hain. Shell script/`.desktop` template CRLF se tootenge.
- **Fix:** `.gitattributes`: `* text=auto eol=lf`, binary types (`*.png *.ico *.icns`) explicitly binary. Repo mein commit karni hai (ye file **tracked** honi chahiye, plans nahi).
- **Phase:** 1

## W-15 · Frontend tests aur scripts Linux-specific — S4 · L

- **Jagah:** `scripts/test-editor-browser.mjs` (`EDITOR_TEST_BROWSER: google-chrome`), `scripts/generate-icons.py`, `scripts/check-test-safety.mjs`, `scripts/lint-frontend.mjs`
- **Problem:** Chrome ka binary naam/path Windows par alag (`chrome.exe`, registry/Program Files). Scripts mein `/` path separators ya `bash` assumptions ho sakte hain.
- **Fix:** Windows CI par Chrome ka path env se pass karo, scripts mein `path.join`/`fileURLToPath` use verify karo. Browser regression test Windows CI mein pehle optional (non-blocking).
- **Phase:** 5, 7

## W-16 · Native integration: file association, protocol, Start menu — S2 · V

- **Jagah:** `desktop-template.desktop` (Linux ke liye MIME), Windows equivalent nahi
- **Problem:** Linux par `.txt/.md` "Open with" `.desktop` MimeType se milta hai. Windows par `bundle.fileAssociations` (NSIS registry keys) config chahiye warna "Open with Personal Notepad" suggestions mein nahi aata.
- **Fix:** `tauri.conf.json` mein Windows ke liye `fileAssociations` (`txt`, `md`, role Editor, `ext` list, mime) — default handler **nahi** banana (user ki choice), sirf "Open with" list mein aana.
- **Phase:** 6

## W-17 · Windows par app permissions (ACL) aur `0o600` equivalent — S3 · L

- **Jagah:** `open_files.rs:654` (socket permission 0o600), data dir creation
- **Problem:** Unix par socket/lock file ka mode restrict hota hai. Windows par `%APPDATA%` per-user ACL se private hota hai, to alag se kuch nahi chahiye, lekin **named pipe** (agar future mein) ko explicit DACL chahiye, warna dusre users connect kar sakte hain.
- **Fix:** Plugin approach (D-02) is risk ko plugin par daal deta hai. Docs mein likhna: Windows par security boundary = user profile ACL. Backups jo user kahin aur save kare unka koi ACL control nahi.
- **Phase:** 3, 8 (docs)

## W-18 · Antivirus / SmartScreen false positives — S3 · L

- **Problem:** Unsigned `.exe` + custom installer par SmartScreen "Windows protected your PC" dikhata hai. Rust binaries kabhi kabhi Defender heuristics trigger karti hain.
- **Fix:** Code signing (Phase 9), pehle beta ke liye documented workaround ("More info → Run anyway"), SHA-256 checksums publish. Microsoft ko false-positive submission agar zaroorat padi.
- **Phase:** 9

## W-19 · Release build time aur CI cost — S4 · L

- **Problem:** `lto=true` + `codegen-units=1` release build Windows runner par 10–25 min le sakta hai. Debug/test builds mein SQLite + argon2 slow hain (Cargo.toml mein already `opt-level=3` for hot deps).
- **Fix:** `Swatinem/rust-cache`, Windows par `--locked`, PR par sirf `cargo check`+tests, full installer build sirf tag/release par.
- **Phase:** 7

## W-20 · Unicode filename normalization aur case-insensitivity — S3 · L

- **Jagah:** attachments naming, import dedup, `open_files` registry (path compare)
- **Problem:** Windows filesystem (NTFS) default **case-insensitive** hai. `A.md` aur `a.md` ek hi file. Linux par alag. Path-based dedup (external documents registry `external_documents.path`) mein case-sensitive string compare se duplicates.
- **Fix:** Windows par path key ko normalize (lowercase compare ya `dunce` + case-insensitive key) sirf external-document identity ke liye. Notes/attachments UUID-based hain to safe (verify).
- **Phase:** 4

---

## Summary table

| ID | Title | Sev | Conf | Phase |
|---|---|---|---|---|
| W-01 | Instance lock `/proc` | S1 | V | 2 |
| W-02 | Single-instance IPC Unix-only | S2 | V | 3 |
| W-03 | `default_data_dir` | S2 | V | 3 |
| W-04 | `file://` parsing | S3 | V | 3 |
| W-05 | `\\?\` canonicalize | S3 | L | 3,4 |
| W-06 | File-sharing semantics (rename/delete) | S1/S3 | L | 4 |
| W-07 | Unix-hardcoded tests | S4 | V | 5 |
| W-08 | Reserved names / long paths | S3 | L | 4 |
| W-09 | Socket cleanup cfg mismatch | S3 | V | 1,3 |
| W-10 | Roaming vs Local data dir | S3 | L | 1,6 |
| W-11 | Abrupt shutdown / stale state | S3 | L | 2,8 |
| W-12 | WebView2 install / offline | S2 | V/L | 6 |
| W-13 | Uninstall data deletion | S1 | L | 6,8 |
| W-14 | CRLF/LF | S3 | L | 1 |
| W-15 | Scripts / browser test Linux-specific | S4 | L | 5,7 |
| W-16 | File associations | S2 | V | 6 |
| W-17 | ACL / permissions model | S3 | L | 3,8 |
| W-18 | SmartScreen / AV | S3 | L | 9 |
| W-19 | Build time / CI cost | S4 | L | 7 |
| W-20 | Case-insensitive FS / path identity | S3 | L | 4 |

**S1 bugs (release blockers):** W-01, W-06 (recovery paths), W-13.
Naye bugs Phase 1 ke baad is table mein **W-21+** ke taur par add karo.
