# Windows remaining module split

ACTIVE (rows 22-25 done; render_pump production pending). Tests-first row of the
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

## Production split (row 23)

`settings/mod.rs` (3,032) becomes `mod.rs` (86) plus eight role
leaves, every production file under the 1,500-line soft cap. No
behavior change: whole-item moves, 8/8 leaf bodies byte-identical to
the staged parent slices modulo one rustfmt blank-line join at the
`effective` excision junction (the shared snapkey import stays in
`mod.rs`) and the header-only `use super::*` per leaf (`chords` is
import-free, matching the owner/snapkey precedent). Zero visibility
changes: no `pub(super)`/`pub(crate)` added, all struct fields were
already `pub`, all cross-file names resolve through the parent
re-exports. All 113 `crate::settings` paths preserved through
explicit parent `pub use`; `settings::tests` (33 tests) untouched.

- `schema.rs` 604: schema consts, `SettingsError`, option
  vocabularies plus accessors, `CoreSettings`/`BorderSettings`/
  `UnderlaySettings`/`KeyboardSettings`/`MouseSettings`/
  `WorkspaceSettings`, `BindingState`/`BindingSetting`/`Settings`
  plus serde defaults.
- `chords.rs` 166: `Chord`, VK constants, `vk_for_key_name`,
  parse/render/lock-chord.
- `catalog.rs` 1,374: binding family/history/op/toggle/launch
  vocabulary, `BindingDef`, row tables plus builders,
  conflict/advisory and arm helpers.
- `effective.rs` 226: `EffectiveBinding`, `effective_bindings`,
  `build_remap`, `build_disabled`.
- `validation.rs` 248: `gap_valid`, `validate_settings` plus
  border/underlay/binding validators.
- `presets.rs` 130: `Preset`, `apply_preset`,
  `compatible_disabled_ids`.
- `persistence.rs` 188: file path, validated load, `LoadOutcome`,
  atomic save, `LiveSettings`/`PollOutcome`/`poll_for_change`.
- `tile_bridge.rs` 66: `tile_options_from_settings`,
  `retained_gaps_match`.
- `mod.rs` 86: module docs, shared imports (incl. the snapkey
  remap/disable import both `effective` and `tests` consume via
  `super::*`), eight `mod` decls, explicit `pub use` re-exports,
  `#[cfg(test)] mod tests`.

Gates: locked four-package build (zero warnings), same-package
tests, workspace fmt check, strict all-target clippy,
`git diff --check`; suite 2,115 / 0 / 1.

## Production split (row 24)

`tiling/mod.rs` (3,929) becomes `mod.rs` (79) plus six role
leaves, every production file under the 1,500-line soft cap. No
behavior change: whole-item moves, 6/6 leaf bodies byte-identical
(ordinal) to the staged parent slices with header-only
`use super::*` per leaf. Zero visibility changes: all private
helpers (`visible_bound`, `bounded_i32`, `pick_edge`, the
border/underlay echo scanners) are used only inside their own
leaf. All 138 `crate::tiling` paths preserved through explicit
parent `pub use`; `tiling::tests` (6 tests) untouched.

- `geometry.rs` 248: shipped gap consts, domain-bounds inset,
  `FrameInsets`, outer-bound/size-hint normalization and
  visible/min/max conversion, `overconstrained_effective`.
- `classification.rs` 310: `TokenMap`, `ObservedTargetRef`,
  `fingerprint`, `WindowFacts`/`SkipReason`, own-settings gate,
  `classify`, overlay/float/sticky/orientation refusal predicates.
- `float.rs` 706: `FloatFocusCandidate` selection, half-snap rect
  plus constraints, maximize-clear/fullscreen/born-maximized
  admission policy, sticky/float markers, retained rects,
  restore-wake, `classify_focus`, `FullscreenToggle`,
  stateless verdicts.
- `reconcile.rs` 592: `ObservedTarget`/`AllowEntry` allowlist
  parse/match/digest, `RefusedTracker`, readback outcomes,
  drop-point/gesture classification, reconcile event builders.
- `cli.rs` 1,429: capture/tile options, scope predicates, CLI
  overrides, plus all four proof verifiers (tile, hide,
  shortcut, workspace) with the shared echo scanners.
- `workspace.rs` 629: children/inspect CLI surface,
  workspace/resize actions/options/requests render/parse,
  `is_borderless_fullscreen`, `tick_summary_signature`.
- `mod.rs` 79: module docs, shared imports (consumed by the
  leaves and `tests` via `super::*`), six `mod` decls,
  explicit `pub use` re-exports, `#[cfg(test)] mod tests`.

Row-22 check (before accepting): the staged parent is
byte-identical to the base outside the single test-region hunk
(the one apparent line-425 diff was a console-recoding artifact
in the comparison pipeline; raw-byte comparison is clean), and
`tests.rs` is the exact one-level dedent (294/294). Line counts
here use raw-byte reads (`ReadAllLines` undercounts by ~158 on
this file through the text pipeline).

Gates: locked four-package build (zero warnings), same-package
tests, workspace fmt check, strict all-target clippy,
`git diff --check`; suite 2,115 / 0 / 1.

## Production map (next rows)

Whole-item responsibility moves into directory modules with
descriptive role names; visibility changes only as needed to compile.

- `settings/mod.rs` DONE row 23 (see above).
- `tiling/mod.rs` DONE row 24 (see above).
- `input_pump/mod.rs` DONE row 25 (see below).
- `render_pump/mod.rs` (2,286): present types plus `Present` trait
  plus `RenderCore` plus fake present; `native` (channels, signals,
  lifecycle, handle, spawn, `run_renderer`, wait classification).

## Production split (row 25)

`input_pump/mod.rs` (1,816) becomes `mod.rs` (73) plus five role
leaves plus a `native` directory module, every production file under
the 1,500-line soft cap. No behavior change: whole-item moves, 5/5
top-level leaf bodies byte-identical to the staged parent slices with
header-only `use super::*`; `native/mod.rs` identical modulo the
one-level dedent plus six rustfmt joins (three `install_keyboard`
signatures, the spawn closure, `wait_ready`, the `None` wait arm;
trailing-comma-only token delta). One necessary visibility widening:
`InboxSender::invalidates` becomes `pub(super)` for the sibling
`tests` module (2 sites); `core_tests` keeps native-private access as
a child module. Zero `pub(super)` elsewhere, no `include!`. All
`crate::input_pump` paths preserved through explicit parent `pub use`
(`classify_input_wait` stays `pub(crate)`); `input_pump::tests`
(19 tests) and `input_pump::native::core_tests` (35 tests) untouched.

- `backoff.rs` 60: install-retry backoff plus ready/stop/contention
  consts, `InputWaitReason`, wait classification.
- `inbox_sender.rs` 278: safety cap plus `InboxSender`.
- `channels.rs` 123: `InputChannels`, wake events plus notify.
- `snapshot.rs` 64: snap-batch age plus shared-snapshot constructor.
- `fake.rs` 74: scripted `FakeBackend`.
- `native/mod.rs` 1,143: backend trait, real hooks, fake
  `NativeHooks` impl, signals, `InputHandle` spawn/wait/stop, wait
  fns, `PumpCore`, `run_pump`; `core_tests` resolves unchanged.
- `mod.rs` 73: module docs, shared imports, seven `mod` decls,
  explicit `pub use` re-exports, `#[cfg(test)] mod tests`.

Gates: locked four-package build (zero warnings), same-package
tests, workspace fmt check, strict all-target clippy,
`git diff --check`; suite 2,115 / 0 / 1.

Row-22 acceptance review: `tests.rs` 728/19, `core_tests.rs`
1,568/35, zero `#[test]` in the parent, all qualified paths
(`input_pump::tests`, `input_pump::native::core_tests`) unchanged in
the suite output. No concrete issue: the inline-`mod native` child
resolved correctly and now becomes an ordinary directory module with
identical paths.

## Production split (row 26)

`render_pump/mod.rs` (2,286) becomes `mod.rs` (64) plus three role
leaves plus a `native` directory module, every production file under
the 1,500-line soft cap. No behavior change: whole-item moves,
`present`/`fake` bodies byte-identical to the staged parent slices
with header-only `use super::*`; `render_core` identical modulo the
one necessary visibility widening below; `native/mod.rs` identical
modulo the one-level dedent plus seven rustfmt joins (four
use/signature joins, the `resolve_anchor` filter closure, the spawn
closure, the `pump_messages` import list; whitespace-only token
delta, 9 insertions / 27 deletions). One necessary visibility
widening: `hide_backoff` becomes `pub(super)` for the sibling
`tests` module (2 sites, via a `#[cfg(test)]`-gated parent import so
production builds stay warning-free); `tag_proven`/`span_us_since`
stay file-local. Zero `pub(crate)`, no `include!`. All
`crate::render_pump` paths preserved through explicit parent `pub
use` (`native` stays a `pub mod` directory module, so
`render_pump::native::{RenderFailEvent, RenderHandle, RenderSignals}`
resolve unchanged); `render_pump::tests` (28 tests) and
`render_pump::native::native_tests` (3 tests) untouched.

- `present.rs` 136: `PresentOutcome` plus impl, `FreshQuery`,
  `ResolvedAnchor` plus impl, the `Present` seam trait.
- `render_core.rs` 652: `RenderCore` plus impl (step/hide/ack,
  border/underlay/preview presents, snapshot sync) plus
  `hide_backoff`/`tag_proven`/`span_us_since`.
- `fake.rs` 309: scripted `FakePresent` plus impl and its `Present`
  impl (real portable painters for checksums).
- `native/mod.rs` 1,118: `NativePresent` backend plus probes,
  `Present` impl, `RenderFailEvent`/`RenderSignals`, wake helpers,
  `RenderHandle` spawn/wait/stop, `RenderWait` classification,
  `pump_messages`, `run_renderer`; `native_tests` resolves unchanged.
- `mod.rs` 64: module docs, shared imports (consumed by the leaves
  and `tests` via `super::*`), readiness/stop/finish consts, five
  `mod` decls, explicit `pub use` re-exports, `#[cfg(test)] mod
  tests`.

Gates: locked four-package build (zero warnings), same-package
tests, workspace fmt check, strict all-target clippy,
`git diff --check`; suite 2,115 / 0 / 1.

Row-22 acceptance review: `tests.rs` 822/28, `native_tests.rs`
88/3, zero `#[test]` in the parent, both qualified paths
(`render_pump::tests`, `render_pump::native::native_tests`)
unchanged in the suite output. Proofs re-ran raw-byte in this row:
parent byte-exact outside the two test regions plus the `mod`
declarations; `tests.rs` exact modulo one rustfmt signature join
(`underlay_frame`); `native_tests.rs` byte-exact. No concrete
issue.
