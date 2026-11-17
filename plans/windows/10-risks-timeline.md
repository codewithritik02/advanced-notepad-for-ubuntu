# 10 — Risks, Timeline, Definition of Done

## 1. Timeline

Maan ke chalo: ek developer, ek Windows VM/CI access.

| Phase | Best | Realistic | Worst |
|---|---|---|---|
| 0 Environment | 0.25 | 0.5 | 1 |
| 1 Baseline compile | 0.25 | 0.5 | 1 |
| 2 Instance lock | 0.25 | 0.5 | 1 |
| 3 Single-instance + Open with | 0.75 | 1.25 | 2 |
| 4 Filesystem | 0.5 | 1 | 2 |
| 5 Tests portable | 0.5 | 1 | 1.5 |
| 6 Installer | 0.5 | 0.75 | 1.5 |
| 7 CI/CD | 0.25 | 0.5 | 1 |
| 8 Manual QA + fixes | 0.75 | 1.5 | 3 |
| 9 Signing, docs, release | 0.25 | 0.5 | 2 (cert wait) |
| **Total (din)** | **~4.25** | **~8** | **~16** |

Realistic mein bug-fix buffer shamil hai. Pehle estimate "4–6 din" optimistic tha; Phase 8 ka QA/fix aur Phase 5 ki test porting ko zyada realistic rakha gaya hai.

## 2. Suggested milestones

| Milestone | Phases | Output |
|---|---|---|
| **M1: Compiles** | 0–1 | Windows par build + app launch, bug list verified |
| **M2: Safe** | 2, 4 | Data-safety (lock, file ops) fixed; S1 bugs band |
| **M3: Usable** | 3, 5 | Open with / single-instance, tests green |
| **M4: Shippable beta** | 6–7 | Installer + CI, release workflow |
| **M5: Verified** | 8–9 | QA report, `v0.2.0-beta` draft release |

## 3. Suggested commit/PR breakdown (small, reviewable)

1. `chore: add .gitattributes and rust-version` (1.1)
2. `fix: compile on windows target` (1.3)
3. `fix: cross-platform instance lock using OS file lock` (2)
4. `refactor: extract fs_util (retry rename/remove, clean canonicalize)` (4.1)
5. `fix: use fs_util in db recovery, restore, migration, attachments` (4.2)
6. `fix: windows-safe filenames for export/restore` (4.3)
7. `feat: single-instance and open-with on windows` (3.1, 3.2, 3.3)
8. `test: make rust tests platform independent` (5)
9. `build: windows NSIS packaging config` (6)
10. `ci: windows matrix and release workflow` (7)
11. `docs: windows install and contributing` (9.3)
12. `release: 0.2.0` (9.2)

Har PR: Linux CI green + (jab available) Windows CI green.

## 4. Risk register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Windows machine/VM nahi milti | Med | High | GitHub Actions `windows-latest` + `cargo-xwin` compile check; dost ka PC QA ke liye |
| R2 | File-locking bugs (W-06) tests mein flaky | High | High | Retry helper, handles explicit close, 3x test runs, Windows-only integration tests |
| R3 | `tauri-plugin-single-instance` behaviour (focus rules, argv format) expected se alag | Med | Med | Early spike Phase 3.1; fallback: named pipe via `interprocess` |
| R4 | SmartScreen/Defender block | High (unsigned) | Med | Docs, checksums, signing Phase 9, false-positive submission |
| R5 | WebView2 missing / offline | Low-Med | Med | `embedBootstrapper`, optional offline installer, docs |
| R6 | Linux regression | Low | High | Linux CI har PR par, cfg-isolated changes, no shared-behaviour change without tests |
| R7 | Rust MSRV (`File::try_lock`) | Low | Low | `rust-version` pin, `fs4` fallback |
| R8 | Data dir decision (D-08) release ke baad badalna | Med | High | Release se **pehle** final, migration ke bina badalna nahi |
| R9 | Unknown unknowns (static audit limit) | High | Med | Phase 1 ke baad bug register refresh, Phase 8 buffer |
| R10 | CI Windows slow/costly | Med | Low | Cache, PR par kam jobs, release par full build |
| R11 | Cross-OS backup/zip incompatibility | Med | High | Test D8, filename sanitization, manifest path separators `/` rakho |
| R12 | Single maintainer burnout / scope creep | Med | Med | Non-goals respect karo (ARM64/MSI/Store Phase 10) |

## 5. Definition of Done (v0.2.0-beta Windows)

- [ ] W-01…W-20 sab triaged; S1/S2 fixed; S3 fixed ya known-issue; S4 backlog
- [ ] Windows CI green (Rust + frontend), Linux CI green
- [ ] Release workflow draft release banata hai (`.deb`, `.exe`, `SHA256SUMS.txt`)
- [ ] Test matrix ke saare **M** rows pass (`09-test-matrix.md`)
- [ ] Fresh install / upgrade / uninstall: data safe
- [ ] README, CHANGELOG, CONTRIBUTING, `platform/windows/README.md` updated
- [ ] Versions synchronized (package.json, Cargo.toml, tauri.conf.json, lockfiles)
- [ ] QA report likhi gayi
- [ ] Tag `v0.2.0-beta` + manual publish

## 6. Decisions on the open questions (FINAL, 2026-11-17)

| # | Question | Decision | Why |
|---|---|---|---|
| 1 | Data dir | **`%LOCALAPPDATA%\com.personalnotepad.app\`** (Tauri `app_local_data_dir`). Linux path unchanged. | SQLite WAL Roaming/network sync par unsafe. Koi Windows user abhi nahi, to migration ki zaroorat nahi. README table update hogi. |
| 2 | Windows machine | **GitHub Actions `windows-latest` pehle**, `cargo xwin check` local compile signal ke liye, Windows 11 VM manual QA ke liye (Phase 8). | Free, reproducible, aur koi hardware dependency nahi. |
| 3 | Signing | **Beta unsigned**, SHA-256 checksums + SmartScreen docs. Stable (0.2.x) par signing. | Cost/time; open-source beta ke liye theek. |
| 4 | Offline installer | **Beta mein nahi.** `embedBootstrapper` hi. Offline artifact tab jab user feedback maange. | 120 MB extra artifact ka maintenance abhi justified nahi. |
| 5 | Version | **0.2.0** (Linux aur Windows dono ek saath bump). Pehle pre-release tag `v0.2.0-beta.1`. | Ek hi version number sab platform par, release metadata script simple rehti hai. |
| 6 | Windows 10 | **Windows 11 + Windows 10 22H2 x64 best-effort.** Win 10 par WebView2 missing ho to docs mein steps. | Win 10 mainstream support ho chuka hai, lekin user base abhi bhi bada hai. |

## 7. Next step

Phase 0.3 + 1.1: `.gitattributes`, `rust-version`, aur `cargo xwin check` se pehli compile errors ki list. Ye Linux par ho jata hai aur kisi behaviour ko nahi chhuta.
