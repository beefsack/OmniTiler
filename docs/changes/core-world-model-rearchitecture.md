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
ownership before stage 1; accepted extractions are recorded below.

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

### Stage 2 implementation slices

- 2a: establish retained logical mapping authority independently of native
  observation. Start with the valid source-only-backing migration discriminator;
  cover unchanged reports, member arrival, target view and source refill,
  including incomplete/unreadable evidence, explicit closes and supersession.
  Native identities remain observations, never fabricated destination domains.
- 2b: atomically replace the Windows workspace/Engine owner for all lifecycle,
  history, tiling and send routes, preserving startup and T2 transactions. Remove
  superseded per-domain adapter interfaces and test-only route scaffolding.
- 2c: retain cursor/settings/focus and implement the approved switch and fresh
  placement policy, with startup/restart restoration kept distinct. Complete
  shared world-action/native acceptance and both-output fixture checks.
- Each slice must have its own passing applicable local gates. A core-only
  slice does not claim runtime adoption, migration live acceptance or a fix for
  dual-output fresh admission.

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
  inventory delivered in `3a4569e1` without runtime or live changes.
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

## Stage 1 accepted outcome (2026-10-21)

- `tiler-core::window_policy` is the sole implementation of ordered geometry
  and focus eligibility. The native adapter projects `WindowFacts` into
  `SafetyFacts` and maps neutral refusal tokens to existing Windows diagnostics;
  `privileged` still renders as `elevated`. Order is unchanged: hidden,
  minimized, cloaked, control privilege, shell, tool, owned dialog, dialog,
  fullscreen, maximized, no-activate. Focus ignores both overlays.
- Windows production uses existing core overlay/toggle/focus-fence,
  born-fullscreen/maximized and slot predicates directly. Removed duplicate
  Windows bodies, public reexports, test-only born-mode wrapper and the private
  admission-clear safety formula. No compatibility aliases or new owner were
  added. All three admission/slot-deferral safety call sites use the same
  normalized core focus classifier, preserving overlay-independent eligibility.
- Shared `test-fixtures/window-policy.json` now includes 22 ordered geometry
  and focus cases, including priority conflicts, consumed by core-only tests
  and Windows production-normalization tests. Exhaustive core coverage crosses
  512 safety shapes with both overlays; a Windows admission seam regression
  crosses the same 2,048 combinations. This is the first shared baseline slice,
  not the later world-action/live-connector case format.
- Independent source review accepted priority, native normalization, production
  argument parity and duplicate removal, including the final admission-safety
  cleanup. Locked core + Windows build/tests and strict all-target clippy pass;
  tests report **2,829 passed, 0 failed, 1 ignored across 69 suites**. Workspace
  format and whitespace checks pass. No dependency, unsafe, input-hook,
  behavior-setting or workspace-owner changes.

### Native fixture evidence

- Final artifact, native Windows 11, fixture-only executable scope: three normal
  windows tiled on their original outputs and an equal-min/max fixture window
  floated without tile writes. Fresh caller-ID/native-lifetime/token joins and
  owner write logs corroborated native physical-frame observations.
- Both outputs independently passed maximize/work-area coverage and observed
  unmaximize/slot return, plus borderless-fullscreen/monitor coverage and normal
  home-layout return. Fullscreen fixture mode remained unknown; native frames,
  not configured mode, established fullscreen coverage.
- A primary-output workspace send-follow/select roundtrip hid and revealed the
  expected members while the other output stayed visible. Closing the tiled
  sibling expanded the survivor; closing the fixed float preserved the other
  tile. These are bounded smoke checks, not full workspace-mode acceptance.
- Final exact-owner stop/independent restore and graceful fixture shutdown left
  no owner, fixture windows, recovery ledger or pending controls. Settings bytes,
  relevant SPI/taskbar state and protected-window geometry matched baseline.
  Final source diff, artifact hashes, gate outputs, process identities and native
  readbacks were inspected and remain outside the repository.
- Live test corrections were expectation errors, not product fixes: resizability
  alone is not the current fixed-size predicate (use equal size hints); numbered
  workspace send follows the mover. Native chrome-adjusted track hints are not
  exact client-size clamps. No failed product approach or unresolved restoration
  issue remains from this stage.

### Remaining scope and next implementation entry

- Stages 2-5 remain pending. Windows still uses its single existing per-domain
  Engine/ManagedWorkspaces owner; world capture/sequencing/diff modules remain
  test-only. Cursor-first placement and the dual-output fresh-admission defect
  are **not** fixed by stage 1.
- Classification rules, manage/float/ignore state, game handling and retained
  admission lifetimes still need the full-world model. This stage moved the
  named stateless predicates only, not those remaining policy owners.
- KDE/macOS are not updated. In particular, macOS's old cross-crate conformance
  test still imports removed Windows predicate APIs and lacks the new fixture
  gate arms; its port must replace these with core/adapter-normalization tests.
  The KDE TypeScript born-maximized copy is still pending its own port.
- Tentative choices: predicate-first sequencing; reuse the existing shared
  window-policy case table; keep native diagnostic vocabulary through mapping
  rather than creating a new Windows policy API. No product behavior was decided
  tentatively in stage 1. Three-way placement subsuming launch targeting remains
  tentative and unimplemented.
- Next: specify the authoritative logical mapping versus native-observation
  contract using `world_engine.rs`, `world_publish.rs`, `world_report.rs` and
  Windows `world_capture.rs`/`world_diff.rs`/`workspace.rs`. Start with a
  core-only discriminating migration/member/view/refill sequence that currently
  holds the absent target, then implement stage 2's single-owner cutover and
  cursor-first policy without an adapter authority bypass. Preserve T2's
  source-state transaction and hidden/fullscreen/bootstrap distinctions.

## Stage 2a investigation and unresolved boundary (2026-10-21)

- The core-only source-only-backing discriminator reproduces a valid accepted
  disjoint migration, not an unknown-workspace rejection: the retained target
  is `out-b/wa1`, the native backing remains observed under `out-a`, and the
  desired target scope is `Hold(IncompleteInventory)`. A fresh unchanged report
  also re-seeds source membership because disjoint flights retain no members.
- Retaining disjoint members prevents source flap-back, but is not the logical
  ownership cutover. Publishing Controlled membership beneath a held target
  contradicts scope authority. Holding scope and membership together preserves
  that fence but leaves the migration unable to progress. Both approaches are
  rejected as stage 2a completion, even with passing tests.
- Windows `tiling_sys/world_diff.rs` checks controlled scope before member,
  target-view and migration actuation. An unheld transfer token does not grant
  authority through those gates. Tests that manually report target arrival
  after an entirely held reply prove settlement only, not achievable migration.
- The unaccepted core prototype also needs correction for logical-identity
  pruning on incomplete topology, refusal/supersession mapping/session
  reconciliation, and disjoint multi-member tree/share/focus assertions.
  Aligned-grid coverage is not evidence for disjoint preservation.
- Focused prototype checks pass: 32 aligned, 13 disjoint and one logical-mapping
  test. Broader core/protocol/Windows tests and strict clippy also pass, but these
  gates do not establish the missing authority contract. No implementation is
  accepted, no Windows owner cutover or cursor-first policy is delivered, and
  no native acceptance is claimed.
- Exact next entry: design the retained logical-domain resolution boundary so
  complete readable source/target-screen facts can authorize the core's logical
  destination independently of whether a native destination domain already
  exists. Keep raw native observations distinct for member/view/refill proof;
  do not weaken genuinely incomplete/unreadable/gesture holds or use retained
  transfer provenance as adapter authority. Require the discriminator to assert
  coherent actionable target scope/member desire before staging arrival, then
  prove all observed phases. Review the unaccepted prototype before reuse;
  stage 2b must not start from it as an accepted foundation.
