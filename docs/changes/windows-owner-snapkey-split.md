# Windows owner/snapkey test split

ACTIVE. Tests-first row of the `workspace_owner`/`snapkey` module-size split:
extract every inline test module into responsibility-named files. No production
code moves in this row.

## Goal

Bring the two remaining oversized Windows files toward the file-size soft caps
(about 1,500 production lines or 3,000 test lines per file) without changing
behavior: `workspace_owner.rs` 12,551 lines and `snapkey.rs` 6,765 lines move
all inline tests out, split by role so every resulting test file stays under
3,000 physical lines.

## Scope / non-goals

- In scope: move `workspace_owner` `tests` (7,917 lines) into six role files
  under `workspace_owner/`, move `fixed_admission_tests` (2,836 lines) into one
  file, move snapkey `shift_sync_tests`, `windrag_mask_tests`, `history_tests`
  (473 lines combined) plus nested `sys::sys_tests` (219 lines) under
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
  `tests_fixed_admission.rs` at 2,836).
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
- File sizes (physical / nonblank / tests): toggle 1,336/1,318/13, send
  1,910/1,880/27, hints-markers 1,323/1,305/17, send-follow 1,216/1,196/15,
  float-send 1,280/1,268/7, layout 719/689/7, fixed-admission 2,811/2,746/39;
  umbrella `tests.rs` 153/141/0; shift-sync 81/76/3, windrag-mask 81/74/5,
  history 305/294/6, sys 220/212/6. Parents: `workspace_owner.rs`
  1,802/1,726/0, `snapkey.rs` 6,082/5,844/0.
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
