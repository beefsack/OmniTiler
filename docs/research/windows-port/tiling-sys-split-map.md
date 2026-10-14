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
- Reuse existing `tiling_sys_scope_*.rs` path-included child modules. Preserve
  each test module's name and declaration position; unwrap its body into the
  new file. Existing `super::` access then needs no visibility changes.
- Table sizes are baseline estimates including inline wrappers; extracted
  file sizes can differ through wrapper removal and `cargo fmt`.
- Grouped rows mean multiple files, preserving each child module's name and
  level. Do not nest existing tests under a new umbrella module.
- Production boundaries below are responsibility outlines, not cut lines.
  Confirm whole-item boundaries and dependencies before each move. A large
  single function must not be divided by changing its logic.

## Ordered extraction map

All target stems below are sibling `.rs` files prefixed `tiling_sys_`.
Numbered production stems denote further whole-item subdivisions as needed.

| Order | Target stem(s) | Contents | Approx lines | Dependencies / risks | Status |
| --- | --- | --- | ---: | --- | --- |
| 1 | `tests_directional_cross` | `directional_cross_adapter_tests` | 2,360 | Two production seams and shared fixture | Extracted; 2,333 lines |
| 2 | `tests_pending_drop` | `pending_drop_tests` | 2,865 | Render/gesture types, queue/service and injectors | Extracted; 2,852 lines |
| 3 | `tests_fixed_predicate` | `fixed_predicate_tests` | 2,659 | Hint context, predicate and fixture | Extracted; 2,652 lines |
| 4 | `tests_d23_minimize`, `tests_d23_minimize_2` | `d23_minimize_adapter_tests` | 3,599 | Same-namespace `include!` preserves helpers and all 27 test names | Extracted and subdivided; 2,061 / 1,535 lines |
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
| 17 | `render_1`, `_2`, `retained` | Render bridge/carriers/acks/health, hidden/closed/retained records | 1,800 total | Native/render coupling and shared fields | Pending |
| 18 | `focus`, `underlay_1`, `_2`, `probes` | Managed publication, focus origins, borders, movesize/preview/hover, write/zoom/restore probes | 2,600 total | Several `TileLoop` methods; move whole impl blocks/items | Pending |
| 19 | `fullscreen`, `admission` | Fullscreen metadata/holds/exit, admission/float gates, event stamps | 600 / 900 | Ownership and engine seams | Pending |
| 20 | `rows`, `reconcile`, `geometry`, `focus_actuation` | Marks/hydration/pruning, row assembly, reconcile tick, geometry/audit, focus and predicates | 2,100 total | Reconcile/apply boundary and test seams | Pending |
| 21 | `keyboard_1`, `_2`, `chords_1`, `_2`, `dispatch` | Keyboard tick, resize/chords/float/raise/unfloat, launch/orientation/tiling/sticky | 4,600 total | Large individual functions may exceed cap intact | Pending |
| 22 | `directional_1`, `_2`, `output_send`, `cross_drag` | Output routing/fences, directional crossing, send/drop/settle/reflow | 4,300 total | Retained Engine and native verification; keep operations intact | Pending |
| 23 | `restart`, `assignments` | Born/slotless/restart marks, assignments, hide/reveal | 1,300 total | Shared retained-state transitions | Pending |
| 24 | `selection`, `sends` | Workspace select/send, trailing-spare/follow behavior | 2,300 total | Existing scope children call parent seams; later extraction needs narrow re-exports | Pending; late |
| 25 | `cli_poll_1`, `_2`, `workspace_tick_1`, `_2`, `migration` | CLI command polls, workspace/send/migrate ticks and migration execution | 4,100 total | Shared loop state; preserve command ordering | Pending |
| 26 | `history`, `action_context`, `output_sync` | Maximized remapping, history/effects, monitor/output sync, maintenance | 1,200 total | Mode-aware scope children and retained output identity | Pending |
| 27 | `windrag`, `pending_drop`, `gesture_tick` | Windrag transitions, pending-drop servicing, gesture tick | 2,000 total | Shared pending state and render bridge | Pending |
| 28 | `settings` | Foreground/suspend reads, run configuration, live settings adoption/poll | 500 | Shared loop configuration | Pending |
| 29 | `run_1`, `_2` | Main tile loop | 2,600 total | Entrypoint/state orchestration; one large function may remain oversized | Pending; late |
| 30 | `commands_1`, `_2` | Public CLI entrypoints, inventory/inspection and shutdown glue | 1,500 total | Preserve external entrypoint paths via parent re-exports | Pending; last |

## Verification and handover

- Every extraction is one pure-move commit. Compare removed/added bodies,
  permitting only necessary module declarations/visibility and rustfmt changes.
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
- Phase 2 subdivides D23 at a whole-test boundary with same-namespace `include!`:
  2,061 / 1,535 lines, byte-identical moved bodies and all 27 names preserved.
  All required gates pass with 2,106 / 0 / 1.
- Row 14 extracts shell/monitor/observe leaves; parent becomes **34,884 lines**.
  Whole bodies preserved apart from necessary visibility and one formatted
  signature. Public identity/observation and PMv2 paths remain re-exported.
- Row 15 extracts hints/seams/placement/minmax; parent becomes **33,818 lines**.
  Bodies and cfg branches preserved with necessary visibility/import changes
  and two formatted signatures. Required gates pass with 2,106 / 0 / 1.
- Row 16 extracts trace/owner-work/preview/input; parent now **33,138 lines**.
  `SnapshotOffer` and `plan_snapshot_offer` stay in the parent, preserving their
  crate-visible paths in all builds. Required gates pass with 2,106 / 0 / 1.
- Next: production orders 17-30, starting render/retained.
  Keep `TileLoop` state in the parent
  initially and use narrow imports/re-exports for moved leaves.
- Production risk: moving a function into a child changes unqualified name
  resolution and sibling access. Confirm imports and minimal visibility rather
  than assuming private child items remain callable from the parent.
- D23's 20 helper functions remain in the first file; the second file contributes
  13 unchanged tests through `include!`, retaining the existing module namespace.
