# Core world-model rearchitecture

## Goal and scope

- Implement the approved [core/platform boundary](../decisions.md#core-platform-boundary):
  one shared-core model and policy owner; Windows captures input, observes,
  actuates and renders. Windows is the first migrated platform.
- Preserve existing specified behavior except the already-decided
  [cursor-first switch and placement policy](../backlog.md). Preserve current
  empty-workspace cross-output focus and cross-output directional MRU behavior;
  their outstanding product questions are not resolved here.
- No compatibility routes, duplicated policy implementations or second active
  Engine. Delete replaced Windows code and obsolete public core interfaces.
  KDE/macOS porting is outside this implementation; their compilation is not
  a constraint on the new boundary.
- This is the implementation successor to the W2/W3 plan in
  [Windows full-world adoption](windows-full-world-state-replies.md), not
  acceptance of its parked diagnostics or authority shortcuts.

## Inventory and core destinations

Paths below are relative to the named source roots. This inventory describes
current ownership, not already-completed extraction.

### Windows (`crates/tiler-windows/src`)

| Area | Current owner and responsibility | Core destination; adapter remainder |
| --- | --- | --- |
| Workspace model | `workspace.rs::ManagedWorkspaces`, `workspace/outputs.rs`: per-output order/current/members, previous/baseline history, tiled modes, displaced workspaces, trailing creation/cleanup | Retained world workspace-to-screen mapping and membership, using `workspace.rs` and `workspace_scope.rs` policy. Adapter retains native identity/binding and recovery claims only. |
| Scope and lifecycle | `workspace_scope_owner.rs`, `tiling_sys/scope_trailing.rs`, `scope_sends.rs`: local/global-unique/shared scope, relative/previous/trailing selection, lifecycle | World owns scope, order, history, creation/removal and hotplug remap. All modes remain per-screen mappings; shared native counts are an opt-in host limitation. |
| Workspace commands | `workspace_owner/sends.rs`, `migration.rs`, `directional.rs`; `tiling_sys/sends.rs`, `output_send.rs`, `send_follow.rs`, `send_stay.rs`, `send_native.rs`, `send_preflight.rs`, `migration*.rs` | World commands select source/target, change mapping/membership and choose focus/follow/stay. Native layer binds and applies desires and reports actual arrival/view/focus. |
| Fresh placement | `tiling_sys/assignments.rs::ensure_workspace_assignments` chooses the output from the new window's rectangle before assigning membership. `directional_resolution.rs::chord_output` currently prefers foreground, then pointer, then cached output. | Core owns cursor, focused screen, startup-versus-new lifetime distinction, placement neighbor and switch target. Implement approved COSMIC/Hyprland/i3-sway placement and cursor/focused switch settings here. Sends stay focused-window-output based. |
| Output resolution | `tiling_sys/directional_resolution.rs`, `workspace_owner/directional.rs`, `workspace_owner/migration.rs` construct/select output context | Core retains FULL rectangles and work areas, selects targets and derives adjacency (`output_selection.rs`, world topology). Adapter reports geometry. `tiling_sys/placement.rs` is mechanical WINDOWPLACEMENT coordinate conversion and stays native. |
| Classification | `tiling/classification.rs::classify`, dialog membership gate; `tiling/float.rs` overlay, born-fullscreen/maximized, slot and toggle decisions; `tiling_sys/admission.rs`, `assignments.rs`, `fullscreen.rs`, `minmax.rs` | Normalized type/role/ownership/modal/control/overlay/size/app facts enter core. Core decides manage/float/ignore, rules, fixed floats, born state and games. Existing `window_policy.rs`, `admission.rs`, `born_maximized.rs`, `size_hints.rs` are the starting points, not a second policy implementation. |
| Verification and fighting | `tiling/reconcile.rs::readback_outcome`, `RefusedTracker`; `tiling_sys/render_ack.rs`, `render_bridge.rs`, visibility and send verification; `workspace_owner` transfer verification | Core compares desired/observed, adopts constraints or refuses/floats/reapplies and owns completion. Native lifetime/replacement, exact recovery ownership and last-moment freshness checks remain mechanical vetoes. DPI transition rebasing and invisible-border inset conversion stay native; stable app clamps do not. |
| Gesture | `tiling_sys/native_drag.rs`, `cross_drag.rs`, `drag_preview.rs`, `pending_drop.rs`, `gesture_tick.rs`; `group_underlay.rs::classify_move_size_probe` | Core owns drag session/target/release and edge share. Adapter reports gesture kind/start, cursor, release/cancel and applies/renders. Do not revive the disabled synthetic drag driver. |
| Visual shapes | `active_border.rs`, `group_underlay.rs`, `tiling_sys/group_underlay.rs`, drag-preview code; partial primitives already in core `visual.rs` | Core publishes border, group underlay and drop-overlay shapes/bounds. Native layer converts coordinates/DPI and draws with host styling. |
| Pager | `pager.rs::build_output_pager`, `retain_pager_order`, miniature/member projection; `tiling_sys/pager_host.rs`, `pager_sys.rs`, `tray_pager.rs` | World publishes ordered labels/current/occupied/contains-focused and window lists per screen. Placement/anchoring, native styling and click-to-command remain platform-owned. |
| Cursor | `ffi/input.rs::cursor_pos` sampled by `directional_resolution.rs`, `drag_preview.rs`, `tiling_sys/group_underlay.rs`; no retained core cursor today | Core retains reported position and applies targeting/focus-follows-cursor/drag policy. Pause cursor/overlay work under a focused fullscreen game; do not add game-costing input hooks. |
| Behavior settings | `settings/*`, `tiling_sys/settings.rs`, settings selected in admission and runtime routes | Core retains behavior settings; platform persists/normalizes and supplies changes. Host rendering and integration settings stay native. |

### Existing shared core (`crates/tiler-core/src`)

- `Engine`/`Session` own layout, retained slots and per-domain operations.
  `WorldEngine` adds report ordering, mapping, histories, topology, trailing
  lifecycle and retained transfers, but current observation-driven mapping
  still conflicts with authoritative logical migration.
- `window_policy.rs` already contains many Windows-equivalent predicates;
  Windows still executes its own copies. Its boolean eligibility lacks the
  ordered refusal vocabulary used by the Windows classifier.
- `world_report.rs`, `world_publish.rs`, `world_commands.rs`, `world_sends.rs`
  are the existing complete-report/desired-state route. Extend or replace
  them in place rather than adding a competing world API.
- The world route is not a complete owner of admission, cursor, gesture,
  verification policy or presentation. A passing W1/W2 diagnostic is not
  proof that those responsibilities have migrated.

### KDE and macOS porting map

| Platform | Current additional policy | Port requirement |
| --- | --- | --- |
| KDE | `kwin/src/born-maximized.ts`, `world-born-maximized.ts`, `fixed-size-predicate.ts`, `migration-source-refill.ts`, `world-drag.ts`, `world-observe.ts`, `world-apply.ts` | Remove the TypeScript born-maximized/fixed/admission copies, gesture and refill decisions after consuming core state. Report native desktops and external changes truthfully. Enable neutral equal-workspace-count support for desktops spanning outputs, without replacing the per-screen core model. Native pager maps that model. Do not retain migration writes that bypass held scope/member authority. |
| macOS | `crates/tiler-macos/src/classifier.rs`, `workspace_select.rs`, `workspace_send*.rs`, `drag_drop.rs`, `runtime.rs` coexist with `world_runtime.rs` | AX/CG/Space facts and coordinate/lifetime mechanics stay native. Move classification, native-Space selection/send/trailing decisions and drag policy to core. Report host-owned Spaces and inability to create/transfer where appropriate, never fabricate logical/native capability. Reconcile all consumers onto the new sole world route. |

- Remove public portable per-domain `CoreEvent`/`CoreReply` application and
  session-query interfaces when Windows no longer needs them. Internal
  Engine/Session layout machinery is not obsolete merely because its public
  adapter-facing interface is. KDE/macOS legacy consumers must be ported by
  their own changes, not preserved through compatibility aliases here.

## Staged plan and dependencies

Tentative sequencing; each stage must have a single active Windows owner and
remove the code it supersedes before its acceptance.

| Stage | Coherent change | Acceptance |
| --- | --- | --- |
| 0: inventory | This ownership map and implementation plan | Source-grounded Windows map, KDE/macOS port notes and explicit unresolved contracts. Documentation-only. |
| 1: normalized window-policy adoption | Adopt existing core overlay/born/slot predicates; extend core ordered eligibility over neutral facts. Delete superseded Windows predicates; keep native identity and fact collection local. No workspace-owner change. | Core-only ordered/refusal and state-transition baselines plus Windows fact projection regressions; no classification or overlay priority changes. Fixture-scoped admission/overlay/close smoke on both outputs. |
| 2: sole retained world and cursor-first placement | Rework core logical workspace mapping versus native observations; bootstrap modes/history/lifetimes, migration/refill and all send routes; retain cursor/settings/focus and implement approved placement/switch targeting. Atomically replace Windows ManagedWorkspaces/direct-Engine owner with the world route. Remove superseded per-domain interface and test-only scaffolding. | Core-only lifecycle/migration/placement cases; all-mode Windows regressions including hidden/fullscreen truth, T2 and native bootstrap. Fixture both-output cursor/focus disagreement, admission, select/relative/previous/trailing, send follow/stay and positive migration/refill. No active legacy owner. |
| 3: observed convergence and completion | Move clamp/refusal/adoption/anti-fighting and completion into core; native layer writes/reads and reports mechanical conversion failures and freshness/lifetime vetoes. | Shared core cases and Windows connector use the same assertions for reapply, learned bounds, refusal, replacement, incomplete/hidden observation and partial transfer. Fixture verifies meaningful native frame/visibility/focus results, not setter returns. |
| 4: gesture and desired presentation | Core drag sessions/edge shares, cursor focus/warp policy, border/group/drop shapes and pager content; delete Windows target/geometry/content decisions. | Core-only gesture/presentation cases, native rendering and both-output fixture checks. Manual physical drag/shortcut acceptance remains distinct; synthetic driver stays disabled. Gaming pause proven offline; no unmeasured zero-cost claim. |
| 5: completion audit | Verify every inventory row has one owner; finish common acceptance connector, eliminate dead APIs and document platform port gaps. | Core + Windows gates, both-output fixture suite and independent boundary/safety review; no undocumented compatibility routes or accepted-behavior gaps. |

### Shared acceptance cases and fixture contract

- Reuse `test-fixtures/window-policy.json` for stage 1 rather than inventing
  another predicate fixture language. Add ordered classification cases and
  drive them against core alone and Windows normalized-fact projection.
- At stage 2, introduce one minimal world-action case format: initial normalized
  observations/settings, ordered input/host changes, expected desired state and
  observed settlement predicates. Core runner and Windows live connector share
  case data/assertions; only staging, input delivery and native observation
  differ. Avoid claiming shared live acceptance from core expectations alone.
- The accepted [Windows fixture](archive/windows-live-fixture-first-use.md)
  supports caller IDs, many windows, batch mutation and observed waits. Its
  min/max are track hints, fullscreen mode readback is unknown, constraints
  cannot be cleared, and history is not request-correlated. Use fresh windows
  where limits cannot be reset; do not fake clamps or causality.
- Structured native outer-frame capture remains a connector responsibility.
  Join fixture caller IDs to fresh native lifetimes, use physical PMv2 frames,
  distinguish client/outer/DWM bounds and wait for observed settlement.
- Core-only tests must not require native hosts. Document unsupported live
  predicates as platform gaps rather than weakening common assertions.

## Contracts and parked questions

- Retain complete observations, explicit closes, hidden/minimized/unreadable
  distinctions, generation/watermark fencing, command consumption and
  at-most-once ambiguous effect tracking. No command replay on catch-up.
- Replace the migration destination Hold diagnostic with core mapping authority
  and observed member/view/refill settlement. Do not invent an empty native
  destination or bypass held authority.
- Preserve the accepted T2 staged transaction semantics unless a later approved
  boundary change supersedes them: unsupported tiled-to-float output crossings
  must leave committed source state unchanged; never branch on FloatOrigin.
- Neutral equal-workspace-count support serves native desktops shared across
  outputs; host-owned workspaces/Spaces remain observations. No platform-name
  checks in core.
- Tentative: three-way placement subsumes launch targeting, pending review.
  Startup/restart restoration must not be treated as fresh cursor placement.
- Parked: empty-workspace focus across outputs; edge-nearest versus MRU
  cross-output focus; float physical-output versus membership-home direction;
  explicit-output send versus directional-send contract mismatch in W2.
  Preserve current behavior where unspecified. Add minimal discriminating rows
  to `docs/spec/reference-outcomes.md` if implementation reveals another gap.

## Verification and accepted evidence

- Stage 0: source and authoritative decision/Windows adoption records inspected;
  no runtime or live change. Implementation stages remain pending.
- Per implementation stage: locked build/tests and strict all-target clippy
  for core + Windows through `mise exec --`; include protocol/FFI when their
  contracts are touched. Workspace formatting and `git diff --check`.
- Behavioral stages require fixture-only native checks on both outputs under
  the [live Windows guide](../live-windows-testing.md), independent readbacks,
  graceful stop/restore and verified absence of owned processes/control residue.
  Keep raw provenance and machine details outside the repository.
- Promote authorized durable stage decisions to `docs/decisions.md` only after
  stage acceptance; mark tentative choices. Archive this record only after the
  entire boundary and Windows migration are accepted.
