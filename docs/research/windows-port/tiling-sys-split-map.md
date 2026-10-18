# Windows tiling-system split map

Behavior-preserving extraction plan for [Module size refactoring](../../backlog.md).
Target the [file-size soft caps](../../principles.md#file-size): about 1,500
production lines or 3,000 test lines per file, split by responsibility.

## Baseline and convention

- Baseline: `tiling_sys.rs` has **61,917 lines**, counted with `rg -c '^'`
  (includes blank lines), before the first extraction.
- Its 28 inline test modules occupy **26,470 lines**. The remaining
  **35,447 lines** include production, imports, declarations and separators,
  364 lines of top-level test-only helpers, and smaller test-only branches
  inside production functions. Production is therefore approximately 35k lines.
- Reuse existing `tiling_sys/scope_*.rs` child modules. Preserve
  each test module's name and declaration position; unwrap its body into the
  new file. Existing `super::` access then needs no visibility changes.
- Table sizes are baseline estimates including inline wrappers; extracted
  file sizes can differ through wrapper removal and `cargo fmt`.
- Grouped rows mean multiple files, preserving each child module's name and
  level, except D23's explicit `hidden` child described below. Do not add
  umbrella test modules.
- Production boundaries below are responsibility outlines, not cut lines.
  Confirm whole-item boundaries and dependencies before each move. A large
  single function must not be divided by changing its logic.

## Ordered extraction map

All target stems below are `.rs` files under
`crates/tiler-windows/src/tiling_sys/`; the module root is `mod.rs`.
Tests consistently use `tests_*.rs`. Remaining targets are responsibility
outlines; confirm whole-item boundaries before each extraction.

| Order | Target stem(s) | Contents | Approx lines | Dependencies / risks | Status |
| --- | --- | --- | ---: | --- | --- |
| 1 | `tests_directional_cross` | `directional_cross_adapter_tests` | 2,360 | Two production seams and shared fixture | Extracted; 2,333 lines |
| 2 | `tests_pending_drop` | `pending_drop_tests` | 2,865 | Render/gesture types, queue/service and injectors | Extracted; 2,852 lines |
| 3 | `tests_fixed_predicate` | `fixed_predicate_tests` | 2,659 | Hint context, predicate and fixture | Extracted; 2,652 lines |
| 4 | `tests_d23_minimize`, `tests_d23_minimize_hidden` | `d23_minimize_adapter_tests`, nested `hidden` tests | 3,599 | Child imports parent helpers; all 27 test bodies retained | Done; 2,062 / 1,537 lines, no `include!` |
| 5 | `tests_float_unfloat` | `float_unfloat_clear_tests` | 2,216 | Clear-effects trait and fixture | Extracted; 2,207 lines |
| 6 | `tests_workspace_migrate` | `workspace_migrate_adapter_tests` | 2,054 | Engine event stamps, migration and injectors | Extracted; 2,047 lines |
| 7 | `tests_output_send`, `tests_cross_drag` | Output-send and cross-drag adapter tests | 1,614 / 704 | Shared fixture; keep separate module names | Extracted; 1,606 / 699 lines |
| 8 | `tests_restart`, `tests_orientation`, `tests_float_nav` | Workspace/navigation tests | 848 / 562 / 531 | Engine fixture, retained-state seams | Extracted; 843 / 559 / 527 lines |
| 9 | `tests_deferred_send`, `tests_render_frame`, `tests_preview` | Deferred send, renderer and preview tests | 779 / 791 / 607 | Render seams and test injection | Extracted; 775 / 788 / 595 lines |
| 10 | `tests_born_maximized`, `tests_owned_exit`, `tests_send_preflight`, `tests_float_send_gate` | Admission and exit tests | 924 / 285 / 69 / 188 | Fullscreen/float ownership seams | Extracted; 913 / 282 / 66 / 184 lines |
| 11 | `tests_windrag`, `tests_intent_wake`, `tests_launch`, `tests_stage3` | Gesture and dispatch tests | 483 / 193 / 352 / 598 | Preserve cfg attributes and preceding module comments | Extracted; 476 / 190 / 342 / 588 lines |
| 12 | `tests_esc`, `tests_input_snapshot`, `tests_same_axis`, `tests_border_shell`, `tests_owner_work` | Remaining small test modules | 19 / 203 / 166 / 39 / 163 | Narrow parent seams | Extracted; 16 / 200 / 163 / 36 / 160 lines |
| 13 | `tests_rmax03` | `rmax03_adapter_tests`, including shared fixture | 599 | 58 fixture references plus existing scope tests; same module name and parent level retained | Extracted last; 594 lines |
| 14 | `shell`, `monitors`, `observe` | Shell constants/filters, monitor areas, hook queue/wake, observation/identity | 200 / 250 / 250 | Shared types; monitor/input coupling | Extracted; 69 / 106 / 511 lines |
| 15 | `hints`, `pending_seams`, `placement`, `minmax` | Hint types, test injectors/live seams, placement translation, hint acquisition | 1,100 total | cfg branches travel with their items; avoid overlapping ownership | Extracted; 138 / 495 / 177 / 314 lines |
| 16 | `trace`, `owner_work`, `preview`, `input` | JSON/border trace glue, work guard, preview bounds, input bridge/snapshot | 1,200 total | Shared `TileLoop` fields and crate-visible snapshot planner stay in parent | Extracted; 93 / 250 / 139 / 248 lines |
| 17 | `render_bridge`, `render_ack`, `retained` | Render bridge/carriers/acks/health, hidden/closed/retained records | 1,800 total | Native/render coupling and shared fields | Done; 469 / 966 / 41 lines |
| 18 | `focus`, `group_underlay`, `drag_preview`, `probes` | Managed publication, focus origins, borders, movesize/preview/hover, write/zoom/restore probes | 2,600 total | Observation `TileLoop` impl stays intact in parent for later extraction | Done; 609 / 705 / 1,002 / 251 lines |
| 19 | `fullscreen`, `admission` | Fullscreen metadata/holds/exit, admission/float gates, event stamps | 600 / 900 | Ownership and engine seams | Extracted; 645 / 848 lines |
| 20 | `rows`, `reconcile`, `geometry`, `focus_actuation` | Marks/hydration/pruning, row assembly, reconcile tick, geometry/audit, focus and predicates | 2,100 total | Reconcile/apply boundary and test seams | Extracted; 906 / 289 / 529 / 391 lines |
| 21 | `keyboard_tick`, `keyboard_directional`, `keyboard_overlays`, `keyboard_resize`, `chord_resolution`, `float_toggle`, `dispatch` | Keyboard tick, directional/maximize/fullscreen dispatch, resize/chords/float/raise/unfloat, launch/orientation/tiling/sticky | 4,600 total | Three match-arm helpers replace the oversized function; loop continuation and effect fences preserved | Done; 217 / 1,288 / 641 / 296 / 213 / 1,132 / 1,377 lines |
| 22 | `directional_resolution`, `directional_cross`, `output_send`, `cross_drag` | Output routing/fences, directional crossing, send/drop/settle/reflow | 4,300 total | Retained Engine and native verification; keep operations intact | Done; 368 / 626 / 1,175 / 1,455 lines |
| 23 | `restart`, `assignments` | Born/slotless/restart marks, assignments, hide/reveal | 1,300 total | Shared retained-state transitions | Extracted; 658 / 492 lines |
| 24 | `workspace_mode`, `selection`, `sends`, `send_preflight`, `send_native`, `send_stay`, `send_follow` | Workspace mode/release/retry, select/send, trailing-spare/follow behavior | 2,300+ total | Existing scope children call parent seams; later extraction needs narrow re-exports | Done; 489 / 663 / 652 / 106 / 403 / 411 / 137 lines |
| 25 | `cli_resize`, `cli_workspace`, `workspace_navigation`, `workspace_send_tick`, `migration`, `migration_run` | CLI command polls, workspace/send/migrate ticks and migration execution | 4,100 total | Shared loop state; preserve command ordering | Done; 174 / 817 / 334 / 543 / 390 / 1,393 lines |
| 26 | `history`, `action_context`, `output_sync` | Maximized remapping, history/effects, monitor/output sync, maintenance | 1,200 total | Mode-aware scope children and retained output identity | Done; 623 / 93 / 480 lines |
| 27 | `native_drag`, `pending_drop`, `gesture_tick` | Native START/Esc helpers, pending-drop servicing, gesture tick | 2,000 total | Shared pending state and render bridge | Done; 356 / 776 / 920 lines at extraction as `windrag`; `windrag.rs` renamed to `native_drag.rs` after Win+drag removal |
| 28 | `settings` | Foreground/suspend reads, run configuration, live settings adoption/poll | 500 | Shared loop configuration | Done; 549 lines |
| 29 | `run_loop`, `run_loop_wait`, `run_loop_intake`, `run_loop_tick` | Main tile loop plus wait/intake/tick helpers (second approved refactor exception) | 2,600 total | Entrypoint/state orchestration; nested continues and per-turn guard preserved | Done; 1,060 / 141 / 766 / 764 lines; `mod.rs` 1,082 |
| 30 | `commands`, `first_run`, `proof_commands`, `inspection` | Public CLI entrypoints, first-run configuration, proof loops, inventory/inspection and shutdown glue | 1,500+ total | Preserve external entrypoint paths via parent re-exports | Done; 472 / 261 / 336 / 452 lines |
| Follow-up | `observation` | Whole observation `TileLoop` impl and enumeration/cursor helpers | 400 total | Keep shared `TileLoop` state and crate-visible snapshot planner in `mod.rs` initially | Done; 359 lines |
| Follow-up | `keyboard_tick`, `keyboard_directional`, `keyboard_overlays` | Three large keyboard match arms extracted into responsibility-based helpers | 2,136 before split | Explicit behavior-preserving refactor exception, not a pure move | Done; 217 / 1,288 / 641 lines |
| Follow-up | `tests_scope`, `tests_scope_send`, `tests_scope_hotplug` | Scope test filenames shortened via `#[path]`, module names unchanged | - | Preserve all qualified test paths | Done; 1,746 / 778 / 948 lines; all 46 qualified paths unchanged |

## Verification and handover

- Whole-item extractions are pure moves. Keyboard match-arm and run_tile_loop
  helper extractions are the two explicit refactor exceptions: outer continues
  become equivalent helper returns, preserving nested targets, order and RAII.
  Compare removed/added bodies, permitting necessary declarations/visibility,
  helper calls, arm-level continue-to-return changes, log-path borrowing and rustfmt.
- Required gates: locked four-package build/test/strict all-target clippy,
  workspace format check and `git diff --check`. Test baseline:
  **2,095 passed / 0 failed / 1 ignored** before upstream shared-policy tests.
  After `7da6d1e`, the same gates pass with **2,106 / 0 / 1**: all original
  suite counts are unchanged, with exactly 11 additional shared-core tests.
  This is the current baseline. Moves must preserve counts; an increase is
  accepted only when verified as new tests in pulled upstream commits.
  No live tests for these moves.
- Phase-1 parent: **35,537 lines**, down **26,380** (42.6%) after 14 extraction
  commits. All 28 inline test modules now occupy 28 child files totaling
  **26,288 lines**. No inline test modules remain. The production prefix before
  the original inline test section is byte-identical to baseline.
- Completed phase-1 evidence: [record](../../changes/archive/windows-tiling-sys-split-phase1.md).
- Phase 2 initially subdivided D23 at a whole-test boundary with same-namespace `include!`:
  2,061 / 1,535 lines, byte-identical moved bodies and all 27 names preserved.
  All required gates pass with 2,106 / 0 / 1.
- Row 14 extracts shell/monitor/observe leaves; parent becomes **34,884 lines**.
  Whole bodies preserved apart from necessary visibility and one formatted
  signature. Public identity/observation and PMv2 paths remain re-exported.
- Row 15 extracts hints/seams/placement/minmax; parent becomes **33,818 lines**.
  Bodies and cfg branches preserved with necessary visibility/import changes
  and two formatted signatures. Required gates pass with 2,106 / 0 / 1.
- Row 16 extracts trace/owner-work/preview/input; parent becomes **33,138 lines**.
  `SnapshotOffer` and `plan_snapshot_offer` stay in the parent, preserving their
  crate-visible paths in all builds. Required gates pass with 2,106 / 0 / 1.
- Row 17 extracts render bridge/posting/acks/health and retained records;
  parent becomes **31,690 lines**. Bodies preserve exact text after removing
  visibility/import changes. Required gates pass with 2,106 / 0 / 1.
- Row 18 extracts focus/underlay/probes; parent becomes **29,158 lines**.
  `live_overlay_flags` retains its public parent path. Comparison permits only
  visibility/import changes and one formatted signature. Gates: 2,106 / 0 / 1.
- Row 19 extracts fullscreen/admission; parent becomes **27,706 lines**.
  Exact bodies preserved after necessary visibility/import changes and four
  formatted admission signatures. Required gates pass with 2,106 / 0 / 1.
- Row 20 extracts rows/reconcile/geometry/focus-actuation; parent becomes
  **25,630 lines**. Bodies preserve exact text except necessary visibility/imports
  and one formatted signature. Required gates pass with 2,106 / 0 / 1.
- Row 21 extracts keyboard/chords/dispatch; parent becomes **20,511 lines**.
  Bodies compare exactly after removing visibility/import changes. `keyboard_1`
  exceeds the soft cap solely to preserve `keyboard_tick` intact. Public launch
  API paths are preserved; required gates pass with 2,106 / 0 / 1.
- Row 22 extracts directional/output-send/cross-drag; parent becomes **16,917 lines**.
  Bodies compare exactly after removing visibility/import changes. All four
  children meet the soft cap; required gates pass with 2,106 / 0 / 1.
- Row 23 extracts restart/assignments; parent becomes **15,782 lines**. Bodies
  compare exactly after removing visibility/import changes, with no formatter
  deviations. Required gates pass with 2,106 / 0 / 1.
- Directory checkpoint: all 72 files moved under `tiling_sys/`; `mod.rs`
  had **15,731 lines** after module/import spelling changes. Production
  children use role names; the then-oversized `keyboard_tick.rs` stayed intact.
  The two proof scripts' source-hash paths follow the root move.
- D23 now uses a real `hidden` test submodule: **2,062 / 1,537 lines**.
  All 27 test bodies are unchanged; 13 qualified test paths gain `hidden::`.
  Descendant access retains private helpers without visibility changes.
  Required gates pass with **2,106 / 0 / 1** after both structural steps.
- Keyboard follow-up: extract directional, maximize and fullscreen arms into
  three helpers, grouped in `keyboard_directional.rs` (**1,288 lines**) and
  `keyboard_overlays.rs` (**641 lines**); `keyboard_tick.rs` is **217 lines**.
  The 58 arm-level continues become helper returns. Original bodies compare
  exactly after those changes, log-path borrowing and rustfmt; the event loop
  compares exactly apart from helper imports/calls. All required gates preserve
  **2,106 / 0 / 1**. The family now has **74 files**, all **41 production
  children** below the soft cap; only `mod.rs` (**15,735 lines**) remains oversized.
- Windows tiling_sys phase 2 complete. Rows 24-30 plus observation are
  extracted with the sizes listed above; scope test files are renamed with all
  46 qualified paths unchanged. `run_loop` helpers are the second approved
  refactor exception after the keyboard arms: wait/intake/tick extraction
  preserves nested `continue` targets and per-turn `OwnerWorkGuard` RAII, using
  narrow parent imports and `cfg(test)`-gated test imports. Other moves are
  pure whole-item moves. Independent review found no behavior, control-flow,
  RAII or public-API regressions. Final family: 103 files including root
  (71 production / 32 test), 68,882 total lines (production 39,118 / test
  29,764); max production `cross_drag.rs` 1,455, max test
  `tests_pending_drop.rs` 2,852; no production file over 1,500, no test file
  over 3,000; `mod.rs` 1,082. Required gates pass with 2,106 / 0 / 1.
- Production risk: moving a function into a child changes unqualified name
  resolution and sibling access. Confirm imports and minimal visibility rather
  than assuming private child items remain callable from the parent.
- D23's 20 helper functions remain in `tests_d23_minimize.rs`; its child
  `tests_d23_minimize_hidden.rs` imports them through `use super::*;`.

## workspace_owner / snapkey split (rows 18-20)

Follow-on to the tiling_sys family above, same soft caps (about 1,500
production lines or 3,000 test lines per file). Sizes below use one stable
convention: physical lines counted with blank lines included, nonblank lines
with only whitespace-free content (re-measured 2026-10-15; the earlier row-18
report counted every test physical size 1 high).

Row 18 moves every inline test module into role files; no production moves.
Owner tests keep one shared-helper umbrella plus six role files; snapkey
keeps three role files plus the nested sys tests. Qualified paths gain one
role segment under the owner tests umbrella only; snapkey paths unchanged.

Owner tests (physical / nonblank / test count): toggle 1,335 / 1,318 / 13,
send 1,909 / 1,880 / 27, hints-markers 1,322 / 1,305 / 17, send-follow
1,215 / 1,196 / 15, float-send 1,279 / 1,268 / 7, layout 718 / 689 / 7,
fixed-admission 2,810 / 2,746 / 39, umbrella 152 / 141 / 0; 125 tests total.
Snapkey tests: shift-sync 80 / 76 / 3, windrag-mask 80 / 74 / 5, history
304 / 294 / 6, sys 219 / 212 / 6; 20 tests total.

Row 19 moves owner production into seven role leaves behind a 73-line mod.rs
(physical): sends 467, migration 309, directional 397, markers 135,
foreground 144, rows 178, identity 142. Row 20 moves snapkey production into
eight role leaves behind an 81-line mod.rs (physical): chords 827, intents
496, classifier 784, push_select 685, push_command 1,002, queue 603, diag
426, sys 1,262. All leaf bodies are whole-item moves; public crate paths are
preserved through parent re-exports. Max production file 1,262, max test
file 2,810: every file within caps. Required gates pass with 2,115 / 0 / 1.
Full layout and evidence: [owner/snapkey record](../../changes/archive/windows-owner-snapkey-split.md).

## remaining top-level split (rows 22-26)

Follow-on to the families above, same soft caps and the same
physical/nonblank size convention.

Row 22 moves every inline test module of the four remaining oversized
Windows top-level files into file-backed directory modules; no
production moves. Settings tests 1,330 / 1,297 / 33, tiling tests
294 / 286 / 6, input_pump tests 728 / 709 / 19 plus native core_tests
1,568 / 1,525 / 35, render_pump tests 822 / 785 / 28 plus native
native_tests 88 / 83 / 3; 124 tests total, all qualified paths
unchanged (one necessary `include_str!` fixture depth fix in
`settings/tests.rs`).

Rows 23-26 move production into role leaves behind thin `mod.rs` roots
(physical): settings mod 86 (schema 604, chords 166, catalog 1,374,
effective 226, validation 248, presets 130, persistence 188,
tile_bridge 66); tiling mod 79 (geometry 248, classification 310,
float 706, reconcile 592, cli 1,429, workspace 629); input_pump mod 73
(backoff 60, inbox_sender 278, channels 123, snapshot 64, fake 74,
native 1,143); render_pump mod 64 (present 136, render_core 652, fake
309, native 1,118). All leaf bodies are whole-item moves; public crate
paths are preserved through parent re-exports, with only two
compile-required `pub(super)` widenings (`InboxSender::invalidates`,
`hide_backoff`, each for the sibling `tests` module). Max production
file 1,429, max test file 1,568: every file within caps. Required gates
pass with 2,115 / 0 / 1. Full layout and evidence:
[remaining-split record](../../changes/archive/windows-remaining-module-split.md).

## Windows module-size tail

Tests-first extraction preserves all 70 qualified test paths: workspace
1,156 lines, active-border 238, render-handoff 458. Render handoff production
is then 1,057 lines and needs no further split. Settings UI was already
split (largest production file 1,184) and stays unchanged.

Four production rows preserve whole bodies, cfg gates and API paths:
workspace parent 1,144 / outputs 624; active-border parent 1,107 / carrier
654; product-hide parent 135 / native sys 1,429 / watcher 254; WinArrow
parent 731 / native sys 1,026. Only the carrier's parent/test-facing private
items gain `pub(super)`; existing crate-visible paths stay re-exported.
No helper extraction or size-cap exception. Locked four-package build,
tests and strict all-target clippy, workspace fmt and diff checks pass:
2,313 passed / 0 failed / 1 ignored. Full before/after sizes and evidence:
[tail-split record](../../changes/archive/windows-module-size-tail.md).
