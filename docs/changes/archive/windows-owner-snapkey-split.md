# Windows owner/snapkey split

COMPLETE (archived 2026-10-15). All three rows done. Tests-first row of the `workspace_owner`/`snapkey` module-size split:
extract every inline test module into responsibility-named files. No production
code moves in this row.

## Goal

Bring the two remaining oversized Windows files toward the file-size soft caps
(about 1,500 production lines or 3,000 test lines per file) without changing
behavior: `workspace_owner.rs` 12,551 lines and `snapkey.rs` 6,765 lines move
all inline tests out, split by role so every resulting test file stays under
3,000 physical lines.

## Scope / non-goals

- In scope: move `workspace_owner` `tests` (7,930 lines) into six role files
  under `workspace_owner/`, move `fixed_admission_tests` (2,810 lines) into one
  file, move snapkey `shift_sync_tests`, `windrag_mask_tests`, `history_tests`
  (464 lines combined) plus nested `sys::sys_tests` (219 lines) under
  `snapkey/`; keep production `.rs` files in place with `#[cfg(test)]`
  `#[path = ...]` declarations; one shared-helper umbrella (`tests` keeps the
  five helpers used across roles).
- Non-goals: no production split (later row), no backlog/split-map updates
  (later row), no behavior, import-semantics, or qualified-path changes beyond
  the added role segment under `workspace_owner::tests`.
- Snapkey qualified paths are unchanged (`snapkey::shift_sync_tests`,
  `snapkey::sys::sys_tests`, etc.); `fixed_admission_tests` keeps its name and
  position; `same_axis_move`, `directional_foundation`, and `migration_tests`
  stay nested where they were.

## Acceptance

- Every test file is at most 3,000 physical lines (largest is
  `tests_fixed_admission.rs` at 2,810).
- Test count preserved: 125 `#[test]` in the owner family, 20 in snapkey.
- Required gates pass with the suite baseline 2,115 passed / 0 failed /
  1 ignored.

## Approach / units

- Pure moves: five files are byte-identical after re-indenting one level
  (verified programmatically); the six role splits additionally rewrite
  `super::` to `super::super::` for the one deeper nesting level (verified by
  reversal plus a lone-`super::` audit). No `include!`, no positional names.
- Shared helpers (`rect`, `key`, `seed_two_tiled`, `float_event`,
  `revision_of`) stay in the `tests` umbrella; section-local helpers
  (orientation, output-send, D16, Rig rigs) moved with their role file. No
  helper visibility widening was needed and no test was duplicated.

## Verification and evidence

- Extraction proofs: 5/5 byte-exact moves (snapkey mods, fixed admission),
  7/7 reversal proofs (six role splits plus the shared-helper umbrella),
  `#[test]` counts 125/125 owner and 20/20 snapkey.
- File sizes (physical / nonblank / tests): toggle 1,335/1,318/13, send
  1,909/1,880/27, hints-markers 1,322/1,305/17, send-follow 1,215/1,196/15,
  float-send 1,279/1,268/7, layout 718/689/7, fixed-admission 2,810/2,746/39;
  umbrella `tests.rs` 152/141/0; shift-sync 80/76/3, windrag-mask 80/74/5,
  history 304/294/6, sys 219/212/6. Parents: `workspace_owner.rs`
  1,801/1,726/0, `snapkey.rs` 6,081/5,844/0.
- Section-local fixtures moved with their sole users after the first cut
  (`seed_send_domains`/`send_through_engine`/`snapshot_window` into
  send-follow, `seed_d16_float_domains`/`d16_eligible` into float-send); the
  `tests` umbrella keeps only the five cross-role helpers. Toolchain
  notes: `#[path]` inside an inline module resolves under that module's
  implied directory for rustc, so role children live behind a file-backed
  `tests.rs` and `sys_tests` lives at `snapkey/sys/sys_tests.rs` via a plain
  `mod` (rustfmt and rustc agree on both).
- Gates: locked four-package build, same-package tests, workspace fmt check,
  strict all-target clippy, `git diff --check`; suite 2,115 / 0 / 1.

## Production split (row 19)

Production follows the tests-first row: `workspace_owner.rs` (1,801 lines)
becomes `workspace_owner/mod.rs` plus seven role leaves, every production
file under the 1,500-line soft cap. No behavior change: whole-item moves,
all leaf bodies byte-identical to the row-18 parent slices, public
`crate::workspace_owner` paths preserved through explicit parent
re-exports (`build_send_event_inner` stays file-local in `sends.rs`: no
test or external use).

- `sends.rs` 467: `OwnerRow`, send builders/stamps, send fences and overlay
  gates, stay-focus, directional refusal, float carry.
- `migration.rs` 309: `MigrateRow`, migrate builder and transfer verify,
  rect translation, migration target and fences.
- `directional.rs` 397: directional/output/drag fence snapshots, selection,
  pair adjacency, transfer verifies, focus target, point lookup.
- `markers.rs` 135: marker slots, restart classification, eligibility gates.
- `foreground.rs` 144: foreground veto classifier.
- `rows.rs` 178: planned writes, writable subset, hidden snapshots, dispatch
  gate, row assembly.
- `identity.rs` 142: output context, fixed-admission stamp, lifetime guards,
  domain values.
- `mod.rs` 73: shared imports, seven `mod` decls, explicit `pub use`
  re-exports, `tests` plus `fixed_admission_tests` decls. The fixed-admission
  test file is renamed `tests_fixed_admission.rs` to
  `fixed_admission_tests.rs` for the ordinary `mod` declaration; module name
  and body are unchanged.

Leaves use `use super::*` per the `tiling_sys` precedent except the
import-free `foreground` and `markers`. Gates: locked four-package build,
same-package tests, workspace fmt check, strict all-target clippy,
`git diff --check`; suite 2,115 / 0 / 1.

## Production split (row 20)

Snapkey production follows the same pattern: `snapkey.rs` (6,081 lines)
becomes `snapkey/mod.rs` plus eight role leaves, every production file
under the 1,500-line soft cap. No behavior change: whole-item moves, all
leaf bodies byte-identical to the staged parent slices except four
rustfmt reflows (three `repeat_live` signatures widened by the added
`pub(super)` prefix, `push_g` split the same way, two `sys` joins from
the one-level dedent). Public `crate::snapkey` paths preserved through
explicit parent re-exports; test module identities unchanged
(`snapkey::shift_sync_tests`, `snapkey::sys::sys_tests`, etc., 20/20
unit tests pass).

- `chords.rs` 827: `KeyboardConfig`, all VK constants, proof marker and
  queue caps, `SnapOp`/`ResizeMode`/`ChordAction` vocabulary, pinned
  routes, catalog/predicate/repeat-live/collision helpers.
- `intents.rs` 496: `SnapCounts`, workspace/launch intent types,
  `Classified` plus impl, `MaskTrigger`, `SnapClassify` state struct,
  `GArm`.
- `classifier.rs` 784: `impl SnapClassify` core (config gates, routing,
  pins, modifiers, `held_shape`, Win/mask tracking) plus the `push`
  dispatcher head.
- `push_select.rs` 685: `push_owned` plus directional/resize/history/
  digit arms.
- `push_command.rs` 1,002: relative-send/output/migrate arms, mode
  toggles (maximize, orientation, tiling, fullscreen, G arms), launch.
- `queue.rs` 603: `QueuedMask`/`SnapOrigin`/verdicts, all queued
  intents/events, `SnapQueue`, `classify_and_queue`, mask reserve/stamp.
- `diag.rs` 426: marked/mod/callback diagnostic buffers and evidence.
- `sys/mod.rs` 1,262: the inline `pub mod sys` dedented one level;
  `mod sys_tests` resolves unchanged.
- `mod.rs` 81: module docs, `Direction` import, nine `mod` decls,
  explicit `pub use` re-exports (`pub(crate)` for `catalog_index`),
  three `#[path]` test decls shortened to the new directory.
- Visibility: 52 `SnapClassify` fields, `PinnedRoute` fields, shared
  helpers, 11 cross-file methods (`push_owned`, nine `push_command`
  arms, `win_up_needs_mask`), and `MarkedDiagBuf::back_mut` widened to
  `pub(super)` only; leaves import sibling-private names through
  explicit `super::chords`/`super::intents` lists. No function exceeds
  1,500 lines (largest is `llproc` at ~531), so no helper extraction
  was needed.

Gates: locked four-package build, same-package tests, workspace fmt
check, strict all-target clippy, `git diff --check`; suite 2,115 / 0 / 1.

## Completion (2026-10-15)

Rows 18-20 complete; this record is archived and the backlog Module size
section points here. Final layout remeasured 2026-10-15 under one convention:
physical means lines counted with blanks included, nonblank means lines with
non-blank content. The earlier row-18 report counted every physical size 1
high; the numbers in this record are the corrected values. Row-18 parents
verified from the row-19/20 patch deletion hunks at 1,801 / 1,726 and
6,081 / 5,844 (physical / nonblank).

Owner family (physical / nonblank / test attributes): production mod 73 / 66,
sends 467 / 448, migration 309 / 300, directional 397 / 382, markers 135 / 129,
foreground 144 / 140, rows 178 / 170, identity 142 / 135; tests umbrella
152 / 141 / 0, toggle 1,335 / 1,318 / 13, send 1,909 / 1,880 / 27,
hints-markers 1,322 / 1,305 / 17, send-follow 1,215 / 1,196 / 15, float-send
1,279 / 1,268 / 7, layout 718 / 689 / 7, fixed-admission 2,810 / 2,746 / 39;
125 tests total. Snapkey family: production mod 81 / 75, chords 827 / 771,
intents 496 / 471, classifier 784 / 761, push_select 685 / 681, push_command
1,002 / 992, queue 603 / 569, diag 426 / 387, sys 1,262 / 1,221; tests
shift-sync 80 / 76 / 3, windrag-mask 80 / 74 / 5, history 304 / 294 / 6, sys
219 / 212 / 6; 20 tests total. Max production file 1,262, max test file
2,810: every file within the soft caps.

Independent read-only review PASSed the moves: owner 79 production spans
matched, tests 125/125 owner and 20/20 snapkey, all code equivalent modulo
allowed visibility/imports/rustfmt; all 93 external references resolved.
Final gates on the finished tree: locked four-package build, same-package
tests (2,115 passed / 0 failed / 1 ignored), workspace fmt check, strict
all-target clippy with -D warnings, and git diff check. Full rows 01-21
replay verified against the staged tree.
