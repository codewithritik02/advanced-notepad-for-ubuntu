# 09 — Windows Test Matrix

Legend: **M** = must pass for release · **S** = should pass · **N** = nice to have
Status column: `[ ]` pending · `[x]` pass · `[!]` fail (bug ID likho) · `[-]` N/A

## A. Automated (CI + local)

| # | Test | Platform | Pri | Status |
|---|---|---|---|---|
| A1 | `cargo check --all-targets --locked` | win | M | [ ] |
| A2 | `cargo clippy --all-targets --locked` (no warnings) | win+linux | M | [ ] |
| A3 | `cargo test --locked` full | win | M | [ ] |
| A4 | `cargo test` full | linux (no regression) | M | [ ] |
| A5 | `sqlite_schema_migrations_integrity_test` | win | M | [ ] |
| A6 | `e2e_critical_user_journeys_test` | win | M | [ ] |
| A7 | `regression_defect_tracking_test` | win | M | [ ] |
| A8 | `startup_shutdown_recovery_test` (lock semantics) | win | M | [ ] |
| A9 | `backup_creation_verification_restore_test` | win | M | [ ] |
| A10 | `attachments_filesystem_consistency_test` | win | M | [ ] |
| A11 | `unicode_input_validation_adversarial_test` + Windows names | win | M | [ ] |
| A12 | `encryption_locking_security_boundaries_test` | win | M | [ ] |
| A13 | `npm run typecheck / lint / test:safety / test` | win | M | [ ] |
| A14 | `npm run build` | win | M | [ ] |
| A15 | `test:editor:browser` (Chrome) | win | N | [ ] |
| A16 | `npm audit --omit=dev`, `cargo audit` | linux job | M | [ ] |
| A17 | Naye unit tests: instance lock, `file_uri_to_path`, `sanitize_for_windows`, retry helper, path key | both | M | [ ] |

## B. Installer lifecycle

| # | Scenario | Expected | Pri | Status |
|---|---|---|---|---|
| B1 | Fresh install (standard user) | No UAC, app in Start menu | M | [ ] |
| B2 | Launch from Start menu | Window opens, empty state | M | [ ] |
| B3 | Upgrade over older build | Notes intact, version updated | M | [ ] |
| B4 | Reinstall same version | Works, notes intact | M | [ ] |
| B5 | Uninstall | App removed, **data folder remains** | M | [ ] |
| B6 | Reinstall after uninstall | Old notes visible | M | [ ] |
| B7 | Silent install `/S` + uninstall `/S` | Data remains | M | [ ] |
| B8 | Install path with spaces/Unicode user | Works | S | [ ] |
| B9 | WebView2 missing (Win10) | Installer installs/prompt clearly | S | [ ] |
| B10 | Downgrade attempt | Blocked with message | S | [ ] |
| B11 | Downloaded SHA-256 matches `SHA256SUMS.txt` | Match | M | [ ] |
| B12 | SmartScreen behaviour documented (unsigned) | Docs match reality | M | [ ] |

## C. Instance and file-open

| # | Scenario | Expected | Pri | Status |
|---|---|---|---|---|
| C1 | Launch app twice | Second brings first to front, no 2nd window | M | [ ] |
| C2 | Kill app (Task Manager), relaunch | Starts normally, no stale lock error | M | [ ] |
| C3 | Two accounts / same data? | N/A, per-user | - | [-] |
| C4 | Double-click `.md` while app closed | App starts, file opens | M | [ ] |
| C5 | Double-click `.md` while app open | Same window, file opens | M | [ ] |
| C6 | Open with, 10 files | All open, respects limit | S | [ ] |
| C7 | File while app minimized | Restored and focused | M | [ ] |
| C8 | File while app password-locked | Queued, opens after unlock | M | [ ] |
| C9 | Path: spaces, Unicode, long (>260), UNC | Opens or clear error | S | [ ] |
| C10 | Same file via different case | One record | S | [ ] |
| C11 | Crafted args (`--x`, 1000 args, non-text file) | Ignored safely, no crash | M | [ ] |

## D. Data safety and filesystem

| # | Scenario | Expected | Pri | Status |
|---|---|---|---|---|
| D1 | Force-kill during autosave, relaunch | Last state consistent (WAL recovery) | M | [ ] |
| D2 | Corrupt DB file → start | Isolated to `.corrupt.<ts>`, fresh DB, no crash | M | [ ] |
| D3 | Backup create/verify/restore | Works while app running | M | [ ] |
| D4 | Restore with Defender scanning | Retry success or clean error | M | [ ] |
| D5 | Large attachment (500 MB+) add/open/remove | OK | S | [ ] |
| D6 | Attachment names `con.txt`, `a:b`, trailing dot | Stored/exported safely | M | [ ] |
| D7 | Export zip with reserved names → extract on Windows | Extracts with safe names | M | [ ] |
| D8 | Linux backup → Windows restore (and reverse) | Compatible | M | [ ] |
| D9 | Encrypted notes: enable, lock, restart, unlock | OK, no plaintext temp left | M | [ ] |
| D10 | Encryption migration with antivirus active | Completes or rolls back safely | M | [ ] |
| D11 | Data folder in OneDrive-redirected profile | Documented/handled | S | [ ] |
| D12 | Disk full / read-only dir | Clean error, no corruption | S | [ ] |
| D13 | Log rotation with log file open | No crash | S | [ ] |

## E. UI / platform feel

| # | Scenario | Pri | Status |
|---|---|---|---|
| E1 | DPI 100/125/150/200% layout | M | [ ] |
| E2 | Dark/light theme, system theme switch | S | [ ] |
| E3 | Fonts (Segoe UI fallback), emoji rendering | S | [ ] |
| E4 | Shortcuts: Ctrl+Z/Y, Ctrl+S?, zoom Ctrl+/-/0, wheel zoom | M | [ ] |
| E5 | IME (Hindi/Japanese) editor input | S | [ ] |
| E6 | Window resize/min size (900×600), multi-monitor | S | [ ] |
| E7 | High Contrast mode usability | N | [ ] |
| E8 | Taskbar icon, alt-tab icon, window title | S | [ ] |
| E9 | Toasts top-right (0.1.5 feature) | S | [ ] |
| E10 | Drag-drop files into app | S | [ ] |
| E11 | Clipboard paste images/text | S | [ ] |

## F. Security / offline

| # | Scenario | Pri | Status |
|---|---|---|---|
| F1 | No outbound network (firewall block, app fully works) | M | [ ] |
| F2 | Data dir ACL: only current user | M | [ ] |
| F3 | CSP holds (no remote scripts), `ipc.localhost` only | M | [ ] |
| F4 | Blocked executable attachment extensions still enforced (`.exe/.bat/.ps1`) | M | [ ] |
| F5 | Path traversal attempts (`..\..\`, `C:\`, UNC, ADS `file.txt:stream`) rejected | M | [ ] |

> F5 note: **NTFS Alternate Data Streams** (`name:stream`) aur `\\?\` paths ko traversal checks mein explicitly test karo, ye Linux par exist hi nahi karte, isliye existing tests inhe cover nahi karte.

## Release gate
Release tab hi jab: sab **M** rows `[x]`, koi S1/S2 open nahi, S rows ke failures known-issues mein documented.
