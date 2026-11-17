# Windows Port Plan — Personal Notepad

> Ye poora folder **gitignored** hai (`plans/` `.gitignore` mein hai). Sirf local planning ke liye.
> Banaya gaya: 2026-11-17 · Base version: **0.1.6** · Target: pehla Windows release **0.2.0** (beta label ke saath)

## Goal

Linux (`.deb`) ke saath-saath **Windows 10/11 x64** ke liye ek safe, tested installer (`.exe`, NSIS) release karna.
Shared business logic (Rust storage, React UI) **same** rahega. Platform-specific code alag `cfg` modules mein rahega.

## Final decisions (summary)

Data dir = `%LOCALAPPDATA%` · CI-first Windows testing + VM for QA · unsigned beta · embedBootstrapper (no offline artifact) · version 0.2.0 (`v0.2.0-beta.1`) · Win 11 + Win 10 22H2 best-effort. Detail: `10-risks-timeline.md` section 6.

## Non-goals (abhi ke liye)

- macOS support
- Windows ARM64 (baad mein, Phase 10 optional)
- Microsoft Store / MSIX (baad mein)
- Auto-update

## Files ka index

| File | Kya hai |
|---|---|
| [01-audit-summary.md](01-audit-summary.md) | Project ka current state, kya Linux-only hai, kya portable hai |
| [02-bug-register.md](02-bug-register.md) | Saare Windows bugs/risks (W-01 se W-xx), severity, file:line, fix |
| [03-decisions.md](03-decisions.md) | Technical decisions (lock, single-instance, installer, signing) aur kyun |
| [04-phase-0-1-setup-and-baseline.md](04-phase-0-1-setup-and-baseline.md) | Phase 0 (environment) + Phase 1 (pehla compile) |
| [05-phase-2-3-instance-and-open-files.md](05-phase-2-3-instance-and-open-files.md) | Phase 2 (instance lock) + Phase 3 (single-instance / Open with) |
| [06-phase-4-5-filesystem-and-tests.md](06-phase-4-5-filesystem-and-tests.md) | Phase 4 (filesystem) + Phase 5 (tests portable banana) |
| [07-phase-6-7-packaging-and-ci.md](07-phase-6-7-packaging-and-ci.md) | Phase 6 (installer) + Phase 7 (CI/CD) |
| [08-phase-8-9-qa-and-release.md](08-phase-8-9-qa-and-release.md) | Phase 8 (QA) + Phase 9 (signing, docs, release) |
| [09-test-matrix.md](09-test-matrix.md) | Windows test matrix aur manual QA checklist |
| [10-risks-timeline.md](10-risks-timeline.md) | Risks, time estimate, milestones, definition of done |

## Phase overview

| Phase | Naam | Estimate | Blocker? |
|---|---|---|---|
| 0 | Environment + tooling | 0.5 din | Windows machine/VM/CI chahiye |
| 1 | Pehla Windows compile + baseline | 0.5 din | Phase 0 |
| 2 | Instance lock (data-safety fix) | 0.5 din | Phase 1 |
| 3 | Single-instance + "Open with" | 1–1.5 din | Phase 2 |
| 4 | Filesystem behaviour fixes | 0.5–1 din | Phase 1 |
| 5 | Tests cross-platform | 0.5–1 din | Phase 2–4 |
| 6 | Installer / packaging | 0.5–1 din | Phase 1 |
| 7 | CI/CD Windows | 0.5 din | Phase 5, 6 |
| 8 | Manual QA | 1 din | Phase 2–7 |
| 9 | Signing, docs, release | 0.5 din (+ cert wait) | Phase 8 |
| 10 | Optional: ARM64, MSI, Store | baad mein | — |

**Total: ~5–7 din (best case 3–4)**. Detail `10-risks-timeline.md` mein.

## Status tracker (update karte rehna)

- [~] Phase 0 — Environment (decisions final; Windows env baaki)
- [~] Phase 1 — Baseline compile (1.1 done: .gitattributes + rust-version 1.89; 1.2 Windows compile baaki)
- [ ] Phase 2 — Instance lock
- [ ] Phase 3 — Single-instance / Open with
- [ ] Phase 4 — Filesystem
- [ ] Phase 5 — Tests
- [ ] Phase 6 — Installer
- [ ] Phase 7 — CI/CD
- [ ] Phase 8 — QA
- [ ] Phase 9 — Release

## Golden rules (poore port ke dauran)

1. **Linux behaviour nahi tootna chahiye.** Har phase ke baad Linux ke saare tests pass hone chahiye.
2. **User data kabhi risk mein nahi.** Installer/uninstaller/upgrade ke baad notes safe rahen.
3. **"Platform claims require platform execution."** Windows ka koi bhi feature jab tak asli Windows par run nahi hua, "NOT VERIFIED" hai.
4. **Fake implementation nahi.** Jo cheez Windows par kaam nahi karti use disable karo aur document karo, chhupao mat.
5. **Chhote commits, ek phase = ek ya zyada commits**, aur har commit ke baad CI green.
6. Test hamesha isolated temp data dir mein chalte hain, kabhi asli `%APPDATA%` par nahi.
