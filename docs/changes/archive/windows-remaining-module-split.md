# Windows remaining top-level module split

COMPLETE (archived 2026-10-15). All five rows done: tests-first row 22
plus one production row per family (rows 23-26). The four remaining
oversized Windows top-level files are now directory modules with every
file inside the file-size soft caps (about 1,500 production lines or
3,000 test lines per file). No behavior change, no helper extraction.

## Goal

Bring the four remaining oversized Windows top-level files toward the
soft caps without changing behavior: `settings.rs` (4,368 lines),
`tiling.rs` (4,224), `input_pump.rs` (4,115) and `render_pump.rs`
(3,203) move every inline test module out to file-backed modules (row
22), then split production into role leaves behind thin `mod.rs` roots
(rows 23-26). Module paths and qualified test paths unchanged.

## Scope / non-goals

- In scope: `settings::tests` (33 tests), `tiling::tests` (6),
  `input_pump::tests` (19) plus nested
  `input_pump::native::core_tests` (35), `render_pump::tests` (28) plus
  nested `render_pump::native::native_tests` (3) into file-backed
  modules; then whole-item production moves into role leaves with
  explicit parent `pub use` re-exports preserving all `crate::settings`
  (113 paths), `crate::tiling` (138), `crate::input_pump`, and
  `crate::render_pump` paths.
- Non-goals: no behavior, import-semantics, or qualified-path changes;
  no helper extraction; no other oversized Windows files (listed under
  Completion); shared `tiler-core` `engine.rs` / `session.rs` listed
  only.
- No `include!`, no positional names. Parents become `mod.rs` with
  `#[cfg(test)]` `mod` declarations.

## Acceptance

- Every file in the four families is within caps: max production file
  `tiling/cli.rs` 1,429 physical lines, max test file
  `input_pump/native/core_tests.rs` 1,568.
- Test count preserved: 124 `#[test]` total (33 / 6 / 19+35 / 28+3),
  all qualified paths and inventory unchanged.
- Required gates pass with the suite baseline 2,115 passed / 0 failed /
  1 ignored.

## Approach

- Row 22 (tests-first): pure moves keeping the original nesting depth,
  so `use super::*` and `super::super::` paths are unchanged.
  Top-level test bodies dedent one level, nested native test bodies two
  levels. Deltas beyond dedent: one necessary `include_str!` depth fix
  (`settings/tests.rs` fixture path gains one `../`) plus rustfmt joins
  where dedented lines now fit. Parents stay oversized by design until
  rows 23-26.
- Rows 23-26 (production): whole-item responsibility moves with
  header-only `use super::*` per leaf. Compile-required visibility
  widenings only: `InboxSender::invalidates` to `pub(super)` (row 25,
  sibling `tests`, 2 sites; `core_tests` keeps native-private access
  as a child module) and `hide_backoff` to `pub(super)` (row 26,
  sibling `tests`, 2 sites, via a `#[cfg(test)]`-gated parent import so
  production builds stay warning-free). Native module visibility and
  platform gating are preserved: input_pump `native` stays a private
  `#[cfg(windows)]` module, render_pump `native` stays a `pub mod`
  directory module. Zero NEW `pub(crate)` widenings (the two input_pump
  `pub(crate)` re-exports pre-date the split).
- Oversized-function scan: no function in the four families needs
  helper extraction (largest function upper bound 266 lines).

## Final layout (physical / nonblank / test count)

Sizes verified from LF raw bytes; physical includes blanks.

- settings: mod 86/80, schema 604/550, chords 166/161, catalog
  1374/1343, effective 226/222, validation 248/243, presets 130/127,
  persistence 188/179, tile_bridge 66/64, tests 1330/1297 (33).
- tiling: mod 79/74, geometry 248/233, classification 310/292, float
  706/668, reconcile 592/571, cli 1429/1397, workspace 629/598, tests
  294/286 (6).
- input_pump: mod 73/66, backoff 60/56, inbox_sender 278/266, channels
  123/115, snapshot 64/62, fake 74/71, native/mod 1143/1089, tests
  728/709 (19), native/core_tests 1568/1525 (35).
- render_pump: mod 64/58, present 136/129, render_core 652/630, fake
  309/290, native/mod 1118/1064, tests 822/785 (28),
  native/native_tests 88/83 (3).

## Verification and evidence

- Independent whole-body comparison, pre-row-22 base to final tree
  across all 34 files: ALL production and test bodies covered.
  Differences are ONLY redistributed imports/re-exports/`mod`
  declarations, dedent/rustfmt spacing/trailing commas, the 2
  compile-required `pub(super)` widenings above, and the one settings
  fixture path gaining `../`. No residual behavior or token changes;
  all 124 test paths and inventory unchanged.
- Per-row proofs: 6/6 row-22 test files byte-identical to (base slice,
  mechanical dedent, edition-2024 rustfmt) modulo the single
  `include_str!` fix; 4/4 parents byte-identical to the base minus the
  test regions plus the `mod` declarations. Rows 23-26 leaves
  byte-identical (ordinal) to the staged parent slices modulo the
  documented widenings and rustfmt joins (row 23: one blank-line join
  at the `effective` excision junction; row 25 native: 6 joins,
  trailing-comma-only token delta; row 26 native: 7 joins,
  whitespace-only, 9 insertions / 27 deletions). Zero `#[test]` remain
  in parents.
- Gates on the finished tree: locked four-package build (zero
  warnings), same-package tests (2,115 passed / 0 failed / 1 ignored,
  baseline unchanged), workspace fmt check, strict all-target clippy
  with `-D warnings`, `git diff --check`.
- Full rows 01-26 replay applies cleanly on base `0e74b2b` and matches
  staged tree `cef3fba62bff0987aa38379e27bb6073e34202bc`.

## Completion (2026-10-15)

Rows 22-26 complete; this record is archived and the backlog Module
size section and split map point here. No semantic behavior changes;
no helper-extraction row needed.

Remaining module-size work (out of this request's four-family scope,
not investigated or refactored here): `crates/tiler-windows/src/`
`workspace.rs` 2,918, `active_border_sys.rs` 1,986,
`product_hide.rs` 1,832, `settings_ui.rs` 1,832, `winarrow.rs` 1,774,
`render_handoff.rs` 1,515 physical lines. No test file across Windows
exceeds 3,000 (max `tests_pending_drop.rs` 2,852). Shared core
`engine.rs` (8,491) / `session.rs` (7,239) remain listed only.
