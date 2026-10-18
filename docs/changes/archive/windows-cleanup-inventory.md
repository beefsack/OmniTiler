# Windows cleanup inventory

- Goal: reduce obsolete Windows-owned repository material without changing behavior or losing parked work and live-test evidence.
- Scope: `crates/tiler-windows`, Windows scripts/recipes, Windows change notes and `docs/research/windows-port`.
- Non-goals: shared code, KDE/macOS, live testing, branch removal, or resolving pending product choices.
- Inspection baseline: `8bde0db6` (2026-10-18).
- Acceptance: reviewed inventory before removal, only unambiguous cleanup applied, applied diff reviewed, locked four-package Windows build/tests/strict all-target clippy, workspace formatting and whitespace checks.

## Candidate list

Each numbered row is one candidate group. "Pending user review" means no removal or move in this pass. No source deletion is justified by the inspected references.

| # | Path or group | Evidence and risk | Proposed action |
| --- | --- | --- | --- |
| 1 | `docs/changes/windows-admission-ci-compatibility.md` | Completed test-only correction; Verification records all gates and hosted CI passed at `c75fe675`, with no live smoke required. No inbound basename references before this inventory. Low risk; preserve all historical evidence. | Archive unchanged to `docs/changes/archive/`. |
| 2 | `docs/changes/windows-launcher-geometry-fighting.md` | Offline implementation delivered, but Verification explicitly requires keeping the record active until physical launcher acceptance; final follow-up leaves that journey pending. Archiving could obscure unfinished acceptance. | Keep active; pending user review of any later archive/consolidation. |
| 3 | `docs/changes/shared-settings-catalog-windows-tabs.md` | Evidence and outcome explicitly keeps the record active until physical acceptance; scale/layout/accessibility matrix remains incomplete. Also covers shared catalog work, outside Windows-only cleanup. | Keep active; pending user review of any later archive/consolidation. |
| 4 | `crates/tiler-windows/src/tiling_sys/world_capture.rs`, `world_diff.rs`, `world_diff_tests.rs`, `world_sequencer.rs`; `docs/changes/windows-full-world-state-replies.md` | Test-only module wiring is intentional in `tiling_sys/mod.rs`; parked world-state adoption retains its basis on `wip/windows-world-diff`. Removing apparent nonproduction code would destroy useful coverage. | Keep. |
| 5 | `crates/tiler-windows/src/win_mouse.rs`, `tiling_sys/windrag.rs`, `tiling_sys/tests_windrag.rs`, `snapkey/tests_windrag_mask.rs`; `docs/changes/windows-win-drag-review.md` | Baseline review proposed dropping Win+drag; subsequent backlog decision `b8b7c411` queues that removal. Existing paths and shared native-drag helpers require dedicated behavioral/specification work, not dead-code cleanup. | Keep pending the separately queued removal work. |
| 6 | `crates/tiler-windows/src/product_hide/sys/mod.rs` reserved tile-override helpers | Four `allow(dead_code)` annotations reserve item 13 D7 marker helpers; marker values have test coverage. A reservation is not proof of obsolescence. | Keep; pending user review of reservation retirement. |
| 7 | `crates/tiler-windows/src/settings_ui/ctl.rs::wide`; `settings_ui/dialogs.rs::{dialog_open,defer_close,take_close_deferred}` | Annotated helpers have test callers (`settings_ui/tests.rs` and dialog unit tests). Removing them also removes test seams; no compiler-proven unused route. | Keep. |
| 8 | `crates/tiler-windows/src/tiling_sys/cross_drag.rs::prepare_cross_drag_event` | Annotated helper has a test-only import in `mod.rs` and a caller in `tests_pending_drop.rs`; removing the seam would break retained coverage. Production uses the candidate-engine twin. | Keep. |
| 9 | `crates/tiler-windows/Cargo.toml` features and platform gates | No crate feature table exists; dependency feature lists expose serde/Win32 APIs. `cfg(windows)` gates are load-bearing platform boundaries, not stale optional features. | Keep. |
| 10 | `scripts/windows-*.ps1` (13 files), `just/windows.just` | Live proof/recovery routes, not obsolete offline scripts. Mock recipes preserve mock-first checks. Scripts without recipes remain directly callable; float/fullscreen/maximise/underlay reuse active-border/shortcuts helpers. No superseding route proven. | Keep. |
| 11 | `crates/tiler-windows/src/bin/tiler-test-window.rs`, `tiler-winarrow-spike.rs` | Owned fixture and input proof binaries support retained live harnesses. Future/live-test value prevents removal. | Keep. |
| 12 | `docs/changes/windows-best-effort-admission-live.md`, `windows-0.1-gate-proposal.md`, `windows-phase1-implementation.md` | Admission live follow-up and gate proposal remain pending; environment guide still links the phase-one record. Not completed stale notes. | Keep. |
| 13 | `docs/changes/windows-all-member-workspace-sends.md`, `windows-multi-output-foundation.md`, `windows-taskbar-pager-icons.md`, `windows-gaming-coexistence.md`, `windows-tiling-sys-split-phase2.md` | Records explicitly remain active for acceptance or ongoing split work. Offline completion alone does not establish full acceptance. | Keep. |
| 14 | `docs/research/windows-port/` (9 files) | Historical design/research snapshots, with delivered work linked to archived evidence (for example `safe-win32-wrappers.md`). No superseding consolidated document established. | Keep. |

## Review and application

- Proposed totals: 14 candidate groups; delete 0, archive 1, consolidate 0, keep 13. Three keep groups (2, 3, 6) are pending user review before any later cleanup; none is selected for removal by this inventory. Group 5 belongs to separately queued implementation work.
- Tentative cleanup choice: archive only the fully completed admission CI compatibility note; preserve active acceptance records even when implementation is delivered.
- No dead source, fixtures, tests, scripts, feature flags or branches are selected for deletion.
- Inventory review accepted the single archive move after correcting the script count, the test-only helper caller and the subsequently queued Win+drag decision. No source removal was approved by the evidence.
- Applied: moved the 46-line admission CI compatibility note unchanged to [the archive](windows-admission-ci-compatibility.md); zero source/script/test deletions, zero changed test counts. The inventory is archived as the completed scope record; retained candidates remain at their current paths.
- Verification: `mise exec -- cargo build --locked`, `cargo test --locked` and `cargo clippy --locked --all-targets -- -D warnings`, each with `-p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows`, passed. Tests: 2,883 passed / 0 failed / 1 ignored, unchanged from baseline. `mise exec -- cargo fmt --all -- --check` and `git diff --check` passed. No live testing performed.
- Applied-diff review passed: unchanged archive content, resolving links, corrected inventory counts/callers, no protected-work removal and no behavior changes.
