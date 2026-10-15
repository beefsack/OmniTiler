# Windows remaining module split

ACTIVE (row 22 done; production rows pending). Tests-first row of the
remaining Windows top-level module-size split: extract every inline test
module out of `settings.rs` (4,368 lines), `tiling.rs` (4,224),
`input_pump.rs` (4,115) and `render_pump.rs` (3,203) into directory
modules. No production code moves in this row.

## Goal

Bring the four remaining oversized Windows files toward the file-size
soft caps (about 1,500 production lines or 3,000 test lines per file)
without changing behavior: every inline test module moves out to a
file-backed module, keeping module paths identical.

## Scope / non-goals

- In scope: `settings::tests` (33 tests) into `settings/tests.rs`;
  `tiling::tests` (6 tests) into `tiling/tests.rs`;
  `input_pump::tests` (19 tests) into `input_pump/tests.rs` plus the
  nested `input_pump::native::core_tests` (35 tests) into
  `input_pump/native/core_tests.rs`; `render_pump::tests` (28 tests)
  into `render_pump/tests.rs` plus the nested
  `render_pump::native::native_tests` (3 tests) into
  `render_pump/native/native_tests.rs`. Parents become `mod.rs` with
  `#[cfg(test)]` `mod` declarations; all qualified test paths
  unchanged; no `include!`, no positional names.
- Non-goals: no production split (later rows), no backlog/split-map
  updates (later row), no behavior, import-semantics, or
  qualified-path changes.
- Shared `tiler-core` `engine.rs` / `session.rs` test modules are out
  of scope (listed only; see backlog).

## Acceptance

- Every test file is at most 3,000 physical lines (largest is
  `input_pump/native/core_tests.rs` at 1,568).
- Test count preserved: 124 `#[test]` total (33 / 6 / 19+35 / 28+3).
- Required gates pass with the suite baseline 2,115 passed / 0 failed /
  1 ignored.

## Approach / units

- Pure moves: file-backed modules keep the original nesting depth, so
  `use super::*` and `super::super::` paths are unchanged. Top-level
  test bodies dedent one level, nested native test bodies two levels
  (verified programmatically per file).
- Deltas beyond dedent: one necessary `include_str!` depth fix
  (`settings/tests.rs` fixture path gains one `../`; the added length
  reflows that one statement under rustfmt) plus rustfmt joins where
  dedented lines now fit. No `include!`, no visibility changes.
- Parents remain oversized by design (settings 3,032 / tiling 3,929 /
  input_pump 1,816 / render_pump 2,286 physical lines); production
  leaves follow in later rows per the map below.

## Verification and evidence

- Extraction proofs: 6/6 test files byte-identical to (base slice,
  mechanical dedent, edition-2024 rustfmt) modulo the single
  `include_str!` fix; 4/4 parents byte-identical to the base minus the
  test regions plus the `mod` declarations.
- File sizes (physical / tests): settings mod 3,032 / tests 1,330 / 33;
  tiling mod 3,929 / tests 294 / 6; input_pump mod 1,816 / tests
  728 / 19 / core_tests 1,568 / 35; render_pump mod 2,286 / tests
  822 / 28 / native_tests 88 / 3. Zero `#[test]` remain in parents.
- Gates: locked four-package build, same-package tests, workspace fmt
  check, strict all-target clippy, `git diff --check`; suite
  2,115 / 0 / 1.
- Oversized-function scan: no function in the four families exceeds
  1,500 lines (largest production fn `binding_catalog` ~260 lines,
  largest test ~208). No helper-extraction row is needed; later rows
  are whole-item moves only.

## Production map (next rows)

Whole-item responsibility moves into directory modules with
descriptive role names; visibility changes only as needed to compile.

- `settings/mod.rs` (3,032): schema structs plus defaults;
  chord grammar (`Chord`, parse/render/vk lookup, lock predicate);
  binding vocabulary plus catalog builders; effective
  bindings/remap/disabled builders; validation; presets; file
  persistence plus live poll; tile-options bridge.
- `tiling/mod.rs` (3,929): geometry/bounds normalization plus
  size-hint conversion; classification (`classify`,
  `classify_focus`, allowlist/scope); float focus, half-snap,
  fullscreen, gesture/drag, reconcile builders; CLI
  parsers/verifiers per proof command plus workspace/resize
  request render/parse.
- `input_pump/mod.rs` (1,816): backoff plus wait classification;
  inbox sender; input channels; snapshot helpers plus fake backend;
  `native` (backend trait, real hooks, wait/spawn, `PumpCore`,
  `run_pump`, re-exports).
- `render_pump/mod.rs` (2,286): present types plus `Present` trait
  plus `RenderCore` plus fake present; `native` (channels, signals,
  lifecycle, handle, spawn, `run_renderer`, wait classification).
