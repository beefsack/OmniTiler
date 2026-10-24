# Windows tiling-system split, phase 2

- Status: complete, archived 2026-10-24. All map orders 14-30, both approved
  helper extractions and the scope/test renames are done; see Completion below.
  Broader cross-platform module-size work remains in
  [backlog](../../backlog.md).

## Goal and scope

Continue the [split map](../../research/windows-port/tiling-sys-split-map.md)
after phase 1: subdivide the D23 tests and extract production responsibilities
in orders 14-30. Preserve behavior, test names and public entrypoint paths.
Use whole-item moves, necessary visibility/imports/module declarations and
rustfmt changes. Two explicit helper-extraction exceptions preserve behavior:
keyboard-tick match arms and run_tile_loop wait/intake/tick responsibility
helpers. Other oversized individual functions remain intact.
Organize the existing family under `tiling_sys/mod.rs`, use role-based child
names and replace the D23 textual include with a real test submodule.

## Approach and acceptance

- One responsibility extraction per commit, following the map order; main
  loop and public commands last. Keep shared loop state in the parent initially.
- Compare moved bodies and inspect the minimal color-moved diff.
- Each extraction requires locked build, test and strict all-target clippy for
  `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`,
  workspace format checking and `git diff --check`.
- Baseline: 35,537 parent lines; 2,106 tests passed, zero failed, one ignored.
  Test-count increases require evidence of additions in upstream commits.
- No live testing. Record file sizes and soft-cap exceptions after each move.
- Keyboard follow-up: keep the batch loop and shared log-path snapshot in
  `keyboard_tick.rs`; move directional dispatch to `keyboard_directional.rs`
  and maximize/fullscreen dispatch to `keyboard_overlays.rs`. Convert only
  arm-level `continue` statements to helper returns; preserve statement order,
  ownership, native-effect fences and event-loop continuation. Compare helper
  bodies against the pre-extraction arms and run the existing required gates.

## Accepted evidence

- D23: 3,595 -> 2,061 / 1,535 test lines. The last 13 whole tests move
  byte-for-byte into a same-namespace `include!`; 20 helpers stay in the first
  file and all 27 test names are preserved. Parent remains 35,537 lines.
- Required build/test/fmt/clippy/diff gates pass: 2,106 / 0 / 1.
- Shell/monitor/observe: parent 35,537 -> 34,884 lines; children 69 / 106 /
  511 lines. Bodies compare identically after removing necessary visibility
  changes and one rustfmt signature reflow. Original public paths are preserved
  through parent re-exports; shared state stays in the parent. Required gates
  pass with the same 2,106 / 0 / 1 baseline.
- Hints/seams/placement/minmax: parent 34,884 -> 33,818 lines; children
  138 / 495 / 177 / 314 lines. All cfg branches travel with their items;
  comparison permits only necessary visibility/imports and two formatted
  signatures. Required gates pass on final formatted source: 2,106 / 0 / 1.
- Trace/owner-work/preview/input: parent 33,818 -> 33,138 lines; children
  93 / 250 / 139 / 248 lines. Bodies differ only by necessary visibility,
  imports and four formatted signatures. Shared fields remain in the parent;
  crate-visible snapshot planner and enum also remain there to preserve paths
  in all builds. Required gates pass: 2,106 / 0 / 1.
- Render/retained: parent 33,138 -> 31,690 lines; children 469 / 966 / 41
  lines. Moved bodies are byte-identical after removing visibility/import
  changes; only blank seams collapse in the parent. Pending drop/cross state,
  restore wake and `TileLoop` impls remain for their later responsibilities.
  Required gates pass: 2,106 / 0 / 1.

- Focus/underlay/probes: parent 31,690 -> 29,158 lines; children 609 / 705 /
  1,002 / 251 lines. Bodies differ only by necessary visibility/imports and
  one formatted signature. Public overlay-flags path remains re-exported in
  all builds. The observation `TileLoop` impl remains intact in the parent.
  Required gates pass: 2,106 / 0 / 1.

- Fullscreen/admission: parent 29,158 -> 27,706 lines; children 645 / 848
  lines. Fullscreen bodies compare exactly after removing visibility/imports;
  admission also has four necessary formatted signatures. Required gates pass:
  2,106 / 0 / 1.

- Rows/reconcile/geometry/focus-actuation: parent 27,706 -> 25,630 lines;
  children 906 / 289 / 529 / 391 lines. Moved bodies preserve exact text apart
  from necessary visibility/imports and one formatted signature; child-local
  helpers stay private. Crate-visible `owner_snap_dropped` path is preserved.
  Required gates pass: 2,106 / 0 / 1.

- Keyboard/chords/dispatch: parent 25,630 -> 20,511 lines; children 2,138 /
  296 / 213 / 1,132 / 1,377 lines. Moved bodies compare exactly after removing
  necessary visibility/import changes. Public launch API paths are preserved.
  `keyboard_1` is a deliberate soft-cap exception: the single `keyboard_tick`
  function stays intact. Required gates pass: 2,106 / 0 / 1.

- Directional/output-send/cross-drag: parent 20,511 -> 16,917 lines; children
  368 / 626 / 1,175 / 1,455 lines. All moved bodies compare exactly after
  removing necessary visibility/imports; no signature reflows. Required gates
  pass: 2,106 / 0 / 1.

- Restart/assignments: parent 16,917 -> 15,782 lines; children 658 / 492
  lines. Bodies compare exactly after removing necessary visibility/imports,
  with no formatter deviations. Required gates pass: 2,106 / 0 / 1.

- Layout checkpoint: all 72 files moved under `tiling_sys/`; `mod.rs` had 15,731
  lines. Positional production filenames have role names; tests use `tests_*`.
  Only module paths/imports and two proof-script source paths change.
  At that checkpoint `keyboard_tick.rs` remained byte-identical (2,138 lines,
  one 2,136-line function).
- D23: real `hidden` child replaces `include!`, using parent-private helpers
  without visibility changes. Files have 2,062 / 1,537 lines; all 27 test bodies
  remain unchanged, with 13 qualified test paths gaining `hidden::`.
  Locked four-package build/test/clippy, workspace fmt and diff checks pass
  after both structural steps: 2,106 passed / 0 failed / 1 ignored.
- Keyboard follow-up: directional, maximize and fullscreen arms now have one
  helper each, grouped by responsibility. `keyboard_tick.rs` is 217 lines;
  `keyboard_directional.rs` is 1,288 and `keyboard_overlays.rs` is 641. All 41
  production children meet the soft cap; `mod.rs` remains 15,735 lines.
  All 58 arm-level continues become helper returns; no nested loops or original
  returns occur in the moved arms. Formatted helper bodies compare exactly to
  the originals after only that adaptation and borrowed batch log-path use.
  The event loop compares exactly apart from helper imports/calls. Locked
  four-package build/test/strict all-target clippy, workspace fmt and diff
  checks pass: 2,106 passed / 0 failed / 1 ignored.

- Workspace selection/sends: `workspace_mode` 489 / `selection` 663 / `sends`
  652 / `send_preflight` 106 / `send_native` 403 / `send_stay` 411 /
  `send_follow` 137 lines; parent 12,922. Polls/ticks/migration:
  `cli_resize` 174 / `cli_workspace` 817 / `workspace_navigation` 334 /
  `workspace_send_tick` 543 / `migration` 390 / `migration_run` 1,393 lines
  (`migration_run` holds the cap); parent 9,299. History/context/output-sync:
  623 / 93 / 480 lines; parent 8,122. Windrag/pending/gesture: 356 / 776 /
  920 lines; parent 6,091. Settings: 549 lines; parent 5,552. Run loop pure
  move: 2,628 lines; parent 2,928. Public commands: 472 / 261 / 336 / 452
  lines; parent 1,431. Observation: 359 lines; parent 1,076. Scope filenames
  shortened via `#[path]` (`tests_scope` 1,746 / `tests_scope_send` 778 /
  `tests_scope_hotplug` 948); all 46 qualified test paths unchanged.
  All bodies compare exactly apart from necessary visibility/imports, narrow
  parent re-exports, `cfg(test)`-gated test imports and rustfmt signature
  wrapping (row-24 pair, `capture_gesture_start`, `suspend_read`); the
  keyboard-arm and run-loop helper refactors are separately approved.
- Run-loop helpers (second approved refactor exception after the keyboard
  arms): edge drain, snap/windrag intake and active tick move into
  `run_loop_wait.rs` 141 / `run_loop_intake.rs` 766 / `run_loop_tick.rs` 764;
  `run_loop.rs` is 1,060 lines and `mod.rs` is 1,082. Skeleton keeps
  stop/deadline/wait/clock/guard/ack-only/teardown byte-exact; stop/deadline
  and ack-only exits stay inline, idle/suspended/paused returns lead to the
  same outer `continue`, and `Err` propagates through the closure into
  unchanged teardown; all nested `continue`s
  retain original targets and per-turn `OwnerWorkGuard` RAII stays in the
  skeleton. Locked four-package build/test/strict all-target clippy, workspace
  fmt and diff checks pass: 2,106 passed / 0 failed / 1 ignored. Independent
  review found no behavior, control-flow, RAII or public-API regressions.

## Completion

Windows `tiling_sys` phase 2 is complete. All map orders 14-30, the
observation impl, the D23 `hidden` submodule, the scope filename cleanup and
both approved helper extractions (keyboard arms, run-loop wait/intake/tick)
are done. Final family: 103 files including root (71 production / 32 test),
68,882 lines (production 39,118 / test 29,764); max production
`cross_drag.rs` 1,455, max test `tests_pending_drop.rs` 2,852; no production
file over 1,500, no test file over 3,000. The broader cross-platform
module-size backlog (`workspace_owner.rs`, `snapkey.rs`, shared crates, KDE
files) remains open.

## Commit records

| Commit | Extraction | Parent lines afterward | Child lines |
| --- | --- | ---: | ---: |
| `114d8ae` | D23 subdivision | 35,537 | 2,061 / 1,535 |
| `06b8e28` | Shell/monitor/observe | 34,884 | 69 / 106 / 511 |
| `40540a0` | Hints/seams/placement/minmax | 33,818 | 138 / 495 / 177 / 314 |
| `9fb3247` | Trace/owner-work/preview/input | 33,138 | 93 / 250 / 139 / 248 |
| `5f3c764` | Render/retained | 31,690 | 469 / 966 / 41 |
| `d8bd237` | Focus/underlay/probes | 29,158 | 609 / 705 / 1,002 / 251 |
| `9654902` | Fullscreen/admission | 27,706 | 645 / 848 |
| `2be88c0` | Rows/reconcile/geometry/focus-actuation | 25,630 | 906 / 289 / 529 / 391 |
| `f13aeba` | Keyboard/chords/dispatch | 20,511 | 2,138 / 296 / 213 / 1,132 / 1,377 |
| `4c0e069` | Directional/output-send/cross-drag | 16,917 | 368 / 626 / 1,175 / 1,455 |
