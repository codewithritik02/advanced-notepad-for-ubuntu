# Phase 2 & 3 — Instance Lock aur Single-instance / Open-with

---

## PHASE 2 — Instance Lock (data-safety fix) — 0.5 din
**Bugs:** W-01, W-11 · **Decision:** D-01

**Goal:** Dono OS par ek hi data dir ko ek hi time par ek hi instance khole, crash ke baad stale lock na ho.

### 2.1 Design
- Nayi lock file: `<root>/.app_instance.lock` — process iski exclusive lock leta hai (`File::try_lock`), `AppInstanceLock` struct file handle hold karta hai (handle drop = lock release).
- `<root>/.app_instance.pid` abhi bhi likhi jaati hai (Linux `forward_launch` isko padhta hai) — lock jeetne ke **baad** likho.
- Agar lock nahi mila → `StorageError::Conflict("Another instance ... already running")` (same message, UI/tests pe asar nahi).
- `/proc` check hata do (ya sirf Linux fallback diagnostics ke liye PID padhne tak limited).
- `Drop`: PID file remove, lock file chhod do (delete mat karo jab tak zaroorat na ho, race se bachne ke liye).

### 2.2 Tasks
- [ ] `paths.rs`: `app_instance_lock_file()` ke saath nayi `app_instance_guard_file()`
- [ ] `AppInstanceLock::acquire` rewrite (file lock)
- [ ] `lifecycle.rs` mein koi change nahi (API same)
- [ ] `lib.rs` `forward_launch` check: PID file se padhna theek rakho (Linux), lekin ab lock source of truth hai
- [ ] Unit tests (Linux + Windows dono par chalne wale):
  - [ ] pehla acquire OK, doosra (same process, doosra handle) → Conflict
  - [ ] drop ke baad dobara acquire OK
  - [ ] stale `.pid` file (garbage PID) ke saath acquire OK (kyunki lock free hai)
  - [ ] lock hold karte hue PID file mein koi bhi number ho, behaviour same
- [ ] Multi-process test: child process spawn kar ke lock hold karao, parent acquire fail ho (`std::process::Command` se test binary dobara)
- [ ] Existing tests: `startup_shutdown_recovery_test.rs`, `test_infrastructure_test.rs` update (agar PID-file semantics assert karte hain)
- [ ] Docs: `docs/development/startup-shutdown-and-application-lifecycle.md` update (local)

### 2.3 Careful points
- Rust MSRV check (`File::try_lock` ≥ 1.89). Fallback: `fs4` crate.
- Lock network filesystem par unreliable, documented limitation.
- Test fixtures ke `TestAppDir` har test ka alag dir dete hain, to parallel tests mein conflict nahi.

### Exit criteria
- Linux: saare tests green, behaviour same.
- Windows: do instances ek data dir par → doosra clean error ke saath rukta hai.
- Task Manager se kill → next launch bina error ke chalu (stale lock nahi).

---

## PHASE 3 — Single-instance forwarding aur "Open with" — 1–1.5 din
**Bugs:** W-02, W-03, W-04, W-05, W-09, W-17 · **Decision:** D-02

**Goal:** Windows par `.md/.txt` double-click / "Open with" / dobara launch → running window aage aaye aur file khule.

### Sub-phase 3.1 — Plugin integration (Windows only)
- [ ] `Cargo.toml`:
  ```toml
  [target.'cfg(windows)'.dependencies]
  tauri-plugin-single-instance = "2"
  ```
- [ ] `lib.rs`: `#[cfg(windows)]` builder mein plugin sabse **pehle** register karo:
  ```rust
  #[cfg(windows)]
  let builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
      // parse paths -> PendingOpenFiles.push_all -> focus window -> emit OPEN_FILES_QUEUED_EVENT
  }));
  ```
- [ ] Focus logic (`unminimize, show, set_focus`) ko ek shared helper `bring_to_front(&AppHandle)` mein nikalo, jo Linux listener aur Windows callback dono use karein (duplicate code hatega).
- [ ] Windows par `set_always_on_top` pulse ki zaroorat hai ya nahi test karo (Windows foreground-lock rules; kabhi taskbar flash karta hai).
- [ ] Windows par `#[cfg(unix)]` forward_launch skip (plugin pehle hi handle karta hai).

### Sub-phase 3.2 — Path parsing (W-04)
- [ ] `argument_to_path` ko pure function mein todo: `file_uri_to_path(rest: &str, windows: bool)` taaki **Linux par bhi Windows logic test ho** (D-11).
- [ ] Windows rules:
  - `file:///C:/Users/me/a%20b.md` → `C:\Users\me\a b.md`
  - `file://localhost/C:/x` → `C:\x`
  - `file://server/share/x.md` (UNC) → `\\server\share\x.md` ya reject (decide + document)
  - Plain `C:\...`, `C:/...`, relative paths cwd se join
- [ ] `is_absolute()` Windows par drive-relative (`C:foo`) / root-relative (`\foo`) ko handle (reject ya normalize)
- [ ] Unit tests (shared logic, dono platform)

### Sub-phase 3.3 — Canonical paths (W-05)
- [ ] `dunce` crate add (ya khud ka helper) → `fs_util::canonicalize_clean`
- [ ] `open_files.rs:471` aur `attachment_storage.rs` canonical call sites update
- [ ] External docs registry: path identity key (W-20) — Windows par case-insensitive compare, display path original case rakho
- [ ] Test: same file ko `c:\a\X.md` aur `C:\A\x.md` se kholna → ek hi record

### Sub-phase 3.4 — `default_data_dir` aur dead code (W-03, W-09)
- [ ] `default_data_dir`/`forward_launch`/`ipc` → clearly `#[cfg(unix)]` module mein, Windows par compile na ho
- [ ] `remove_socket_file`, `remember_socket_for_cleanup` ke Windows no-op ya cfg
- [ ] `cargo clippy --all-targets` dono platforms par warning-free

### Sub-phase 3.5 — Security notes (W-17)
- [ ] Plugin callback ke argv input ko **untrusted** maano: existing `parse_launch_paths` limits (`MAX_LAUNCH_FILES`, validation, `decode_message` jaisi checks) reuse
- [ ] Sirf `.txt/.md` text files kholne ka existing check (`read_external_text`) Windows par bhi hit ho
- [ ] Docs: security boundary = user profile

### Sub-phase 3.6 — Manual verification
- [ ] App chalu hai, Explorer se `.md` double-click → wahi window mein file khule, window aage aaye
- [ ] App minimized hai → restore hoti hai
- [ ] App locked (password) hai → file queue hoti hai, unlock ke baad khulti hai (Linux ke jaisa behaviour)
- [ ] 10 files ek saath select + Enter
- [ ] Unicode/space/long path wali file
- [ ] Network share (UNC) file
- [ ] App band hai → double-click → app start + file khule (cold start)

### Exit criteria
- Windows par single-instance + Open with kaam karta hai.
- Linux ka socket flow bina change, tests green.
- Windows aur Linux par path-parsing tests green.
