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
| KDE | `kwin/src/born-maximized.ts`, `world-born-maximized.ts`, `fixed-size-predicate.ts`, `migration-source-refill.ts`, `world-drag.ts`, `world-observe.ts`, `world-apply.ts` | Port to world contract v2: screen/window facts and core logical ids replace native-derived domains/bindings. Enable `host_owned_workspace_domains` and `equal_workspace_count` for desktops spanning outputs; mechanically translate independently observed native membership/view into logical receipts. Missing native destination backing never holds logical mapping authority. Remove TypeScript admission/gesture/refill policy copies. Native pager maps the core model; held scope/member fences still apply. |
| macOS | `crates/tiler-macos/src/classifier.rs`, `workspace_select.rs`, `workspace_send*.rs`, `drag_drop.rs`, `runtime.rs` coexist with `world_runtime.rs` | Port all consumers to world contract v2 and `host_owned_workspace_domains`; opt into equal counts only if the host requires them. AX/CG/Space facts, native-to-logical binding and coordinate/lifetime mechanics stay native. Translate observed Space membership/view into logical receipts, not desired-state echoes. Move classification, selection/send/trailing and drag policy to core. The legacy drag consumers cannot access the now-internal Engine/CoreEvent/CoreReply boundary; use world commands and presentation instead. Report inability to create/transfer through truthful feedback, never fabricate backing, arrival or capability. |

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

### Stage 2a logical-authority replacement

- The implementation direction is logical-first, not a narrower migration
  exception: retained core workspace identities, per-screen rings and views
  authorize decisions. Native backing identities are optional observations and
  actuation bindings, never prerequisites for that authority.
- The default world contract requires screen geometry/work areas and truthful
  window facts, but no native workspace domains. Core creates and retains its
  logical workspaces, including trailing empties. Host-owned workspace domains
  and equal workspace counts remain explicit capability-named opt-ins for
  later platform ports; replace the superseded native-derived mapping and
  aligned-grid machinery rather than retaining a compatibility route.
- Replace the report/protocol projection as necessary to make this distinction
  real. Preserve report fencing, command consumption, explicit closes,
  hidden/minimized/unreadable facts and observed transfer settlement. Review
  prototype pieces individually; neither rejected held-authority approach is a
  starting contract.
- Core-only acceptance must cover native-free bootstrap/lifecycle and a
  source-only-native-backing migration with actionable target scope and members
  before arrival, then unchanged reports, arrival/view/refill, incomplete and
  unreadable facts, closes, refusal/supersession and tree/share/focus retention.
  Windows runtime ownership and cursor-first policy remain later slices.

## Stage 2a replacement validation and remaining blocker (2026-10-21)

- Historical validation before the accepted correction below: the logical-first
  replacement was unaccepted. Passing broad gates and
  earlier source review did not establish recovery after actuation feedback is
  acknowledged. No implementation from this replacement is accepted for the
  Windows owner cutover; the blocker below must be resolved first.

- World contract v2 replaces the native-domain-derived route, without a v1
  compatibility path. Core mints stable logical workspace ids, owns per-screen
  rings/views/history/trailing lifecycle and retains exact-lifetime membership
  independently of Engine layout sessions. Releasing a floating workspace's
  layout no longer loses membership or intentional-float truth.
- Reports carry screens (FULL rectangles and work areas), normalized window
  facts, explicit closes and independently observed logical membership/view
  receipts. The default needs no native workspace vocabulary. Optional
  `host_owned_workspace_domains` requires translated logical member receipts
  for host-owned arrival; `equal_workspace_count` pads core rings neutrally.
  Native inventory is observation only, never logical destination authority.
- The source-only-backing discriminator asserts Controlled target scope,
  workspace and membership before staging arrival. Unchanged reports retain
  logical ownership; raw member arrival, target view and source refill prove
  settlement independently. Missing/contradictory receipts, unreadable facts,
  incomplete topology/inventory and gesture barriers do not fabricate proof.
  Explicit closes still settle across gaps. Ambiguous and partially refused
  attempts retain no-retry holds after feedback disappears.
- Proven wholly-unarrived refusal restores the logical unit's ring position,
  views, tiling, tree/shares and remembered focus only from readable source
  evidence and a validated session relocation. Partial arrivals or unhostable
  source facts hold narrowly instead of destructively rebuilding layout.
- Local/global-unique/shared scope behavior remains explicit. Nonempty global
  swaps move complete logical units atomically; sticky floats stay on their
  physical outputs, including sticky-only and empty-side swaps. Shared trailing
  lifecycle uses union occupancy, with one shared history and no displacement
  mapping. Output disconnect/return preserves logical units and layout state.
- Removed native-derived aligned-grid identity, native binding/startup
  conversion, pending native creation and displaced-native scaffolding. Replaced
  obsolete domain-shaped world tests with focused v2 suites. The sole
  WorldEngine is split by envelope, logical lifecycle, observation, membership,
  commands/operations, settlement and publishing; layout remains internal
  Engine/Session machinery.
- Regression coverage includes T2 unchanged-source refusal, boundary floats,
  exact source focus/MRU, default tiling seeds, hidden/minimized/fullscreen facts,
  admission focus lifetime fencing, float-placement settlement, diagnostic-only
  capacity origin, transactional report rejection and stale/consumed commands.
  Exception-only sessions publish truthful rows without inventing a tile tree.
- Locked core/protocol/Windows tests, build and strict all-target clippy pass;
  workspace formatting and whitespace checks pass. A separate discriminating
  recovery probe fails despite those passing gates.
  No dependency, unsafe-code, input-hook or cursor-policy changes. Core-only
  evidence is not native migration acceptance or Windows runtime adoption.

### Unresolved migration refusal recovery

- Invariant: an acknowledged terminal actuation outcome must remain retained
  under its operation identity until readable evidence permits reconciliation.
  A no-retry hold is not enough to remember whether the attempt was possibly
  executed, proven refused or externally superseded.
- The replacement initially restored migration mapping from missing facts when
  refusal arrived inside incomplete inventory. It also swept the retained tree
  empty across the gap. The correction preserves scope/session state and marks
  the flight held until readable source/target evidence returns, but retains
  only `TransferTarget.held`, not the terminal outcome.
- Minimal failing sequence: migrate three source members; report incomplete
  empty inventory with `RefusedProven` for the migration; acknowledge that
  feedback; report complete readable original-source members with no feedback.
  The replacement still desires the workspace at the target with the migration
  held. Wholly-proven-source rollback is unreachable because its branch depends
  on the now-absent feedback row. A possibly-executed hold is indistinguishable.
- `world_migration_refusal.rs` recovery tests repeat `RefusedProven` on the
  readable recovery report, masking this consumption boundary. The separate
  failing probe omitted the repeated feedback and confirmed the stuck target
  mapping and retained held flight. Neither feedback replay nor fabricated
  arrival is an acceptable repair.
- Recommended next change: retain the terminal migration outcome separately
  from no-retry authority, and reconcile it from later readable evidence after
  feedback acknowledgement. First turn the recovery probe into a permanent
  regression, including partial arrivals, explicit closes and supersession;
  inspect the retained-operation state transition before another semantic
  repair. Re-run core/protocol/Windows gates and review this exact boundary.
  This is an implementation blocker, not a new product question.

### Remaining stage 2 boundary

- 2b remains: replace Windows ManagedWorkspaces/direct-Engine ownership
  atomically, including lifecycle/history/tiling/all send routes, startup and
  restart restoration, hidden/fullscreen truth and T2 transaction routing.
  Current Windows world capture/sequencer/diff remain test-only. They report
  unknown member/view receipts until genuine application/readback supplies
  evidence; acknowledgement never copies desires into observations.
- 2b must remove remaining per-domain adapter interfaces and test-only route
  scaffolding. Audit native scope/member gates for floating membership-only
  sends and refill publication. The source-only sticky migration test currently
  guarantees no target sticky row/frame write and the correct source-refill row
  on the unchanged report; immediate refill sticky publication is not established
  by this slice and needs the actuation audit.
- 2c remains: retained cursor/settings/focus, approved cursor-first switch and
  three-way fresh placement, distinguishing startup restoration. Shared
  world-action/native acceptance and fixture checks on both outputs remain.
  No fix for the dual-output fresh-admission defect is claimed here.
- No new product question blocks logical authority. Existing parked focus/send
  questions remain unchanged. Three-way placement subsuming launch targeting
  stays tentative and unimplemented; no additional tentative product decision
  was introduced in 2a.
- Next: resolve the retained-outcome blocker and accept 2a before starting 2b.
  Then implement truthful applied member/view receipts and bootstrap/restoration
  projection, all Windows mode/send routes and both-output native verification.

## Stage 2a accepted outcome (2026-10-21)

- Logical-native-free world contract v2 is accepted for the core boundary,
  not as Windows runtime adoption. The previous unresolved-outcome section
  records the failing predecessor, not a remaining blocker. The
  [logical-authority decision](../decisions.md#logical-workspace-authority)
  is user-approved, not tentative. Inherited machinery can be simplified;
  preserving its implementation is not an acceptance requirement.
- Terminal migration refusal/supersession survives acknowledgement under
  its operation identity, separately from no-retry authority. Later readable
  evidence reconciles without feedback replay. Partial arrival holds, explicit
  closes settle across gaps, and possibly-executed attempts never masquerade
  as proven refusal. Float-placement ambiguity also survives acknowledgement:
  a third rectangle cannot silently become external-input proof.
- Settlement restages logical domains before sweeping and publishing. Proven
  rollback returns source scope, membership, ring position, views, tree/shares
  and focus in the same reply, not one observation later. Sessionless floating
  workspaces migrate through core-owned membership and the same mapping/refill
  commit, preserving floating mode without inventing a tree or focus effect.
- Hotplug collects stranded members before rekeying and retains their unique
  policy transfers until observed arrival. Outsiders and sticky windows are
  excluded. Restored routing tests pin seam/rank selectors, exhausted-edge and
  capacity-float crossings, command results, predicate ingress and no replay.
- Independent deletion review found admission, directional, floating-migration
  and adapter safety coverage gaps. These were restored, not waived by passing
  broad suites. Floating-source refusal was a regression and was removed; the
  absence of an Engine tree is not a reason to refuse logical migration.

### Deleted-test behavior mapping

Core test paths below are relative to `crates/tiler-core/tests/`; suite names
refer to retained v2 files. Native-derived bindings, aligned-cell identities,
native creation/removal and binding-echo startup conversion are superseded by
the approved logical boundary, not treated as retained product requirements.

| Replaced test file | Accepted behaviors and retained coverage |
| --- | --- |
| `../src/world_engine_tests.rs` | Envelope/identity/generation/watermark/duplicates/high-watermarks/size/transactional rejection: `world_envelope_budget`. Window/view/migration/refill/close/refusal/ambiguity/supersession: `world_logical_contract`, `world_truthful_settlement`, `world_migration_refusal`. Operation results, drag preview/drop, active group, move/resize, send permissions and deterministic replay: `world_restored_routing`. Float tokens: `world_capacity_diagnostic`; born state: retained `fixed_size_admission`. |
| `world_admission_focus.rs` | Focus-once, burst last eligible, retry failure, unexplained exclusion, normalized floats, quiet bootstrap/restart and fullscreen/maximized siblings: `world_coverage_gaps::{burst_batch_pins_last_eligible_newcomer_once, explicit_tile_retry_failure_mints_no_focus_and_never_loops, unexplained_excluded_newcomer_takes_no_activation, normalized_ordinary_float_newcomer_focuses_once, maximized_sibling_suppresses_admission_without_retry, restart_adoption_stays_quiet_without_origin}`, plus `world_admission_close_stay` and capacity-origin parity. |
| `world_aligned_migration.rs` | Tree/shares/focus/tiling, arrival/view/refill phases, delayed/partial/unreadable/empty/closed members, refusal/supersession/no replay and sticky source refill: logical/refusal/truthful suites. Directional target ranking: `world_restored_routing::world_migration_selects_largest_edge_despite_window_centre`. Second flight and sessionless floating carry/rollback: `world_coverage_gaps`. Same-native identity, target placeholder/rebinding and aligned-cell occupancy are superseded. |
| `world_aligned_workspaces.rs` | Local/global-unique/shared rings, views/history, numbered/trailing selection, equal counts, occupancy and sticky handling: `world_modes_contract`, logical bootstrap, `world_float_boundary` and `world_global_swap`. Native aligned identities, holes and startup binding conversion are superseded. |
| `world_born_fullscreen.rs` | Slotless born-fullscreen, later overlay slot, exit/replacement and reused-lifetime handling: retained `fixed_size_admission` born-fullscreen/removal tests, logical minimized/fullscreen/hidden test and `world_admission_close_stay` close/reopen lifetime test. |
| `world_boundary_send.rs` | Tiled/float boundaries, membership-only float writes, sticky/excluded refusal and overlay flags: `world_float_boundary`, `world_float_membership`, `world_restored_routing::float_send_authorizes_membership_and_guarded_sends_refuse`, plus empty/occupied targets in `world_coverage_gaps`. |
| `world_capacity_floats.rs` | Mixed seed, ordinary/capacity parity, float echoes, retry, minimized restore, held/exception-only scopes and disable/enable: `world_capacity_diagnostic` and `world_coverage_gaps` retry/restart tests. Origin is diagnostic only. |
| `world_contract_repairs.rs` | Incarnation, topology/focus-pair, release/toggle, held rows, numbered/trailing sends, capacity/minimized/seed, late refusal and reply bounds: envelope/logical/modes/truthful/capacity suites. Same-domain preview and fixed-width predicate ingress: `world_restored_routing`; floating toggles/defaults: `world_float_membership` and `world_float_boundary`. |
| `world_default_tiling.rs` | Startup floating default, later default only on new workspaces and manual override preservation: `world_float_boundary::default_seed_changes_only_new_workspaces`, plus float toggle/release tests. |
| `world_directional_intents.rs` | Focus/move/send/migrate routing, focus-up fence, partial topology, seam/overlap/center ranking, exhausted-edge identity and capacity fallback: logical directional tests and `world_restored_routing::{world_seam_prefers_upper_interval, world_overlap_fallback_selects_larger_span, world_both_centred_skips_overlap_to_left_top, world_send_matches_move_targets, flight_fences_second_move_pre_engine_without_rerank, exhausted_move_retains_identity_pinned_transfer_and_settles_on_arrival, capacity_float_fallback_retains_identity_pinned_window_transfer}`. |
| `world_displacement.rs` | Disconnect/return/nearest survivor, whole-unit tiling/layout, shared/global modes, geometry gaps, unique policy intents and outsiders: logical hotplug, `world_float_boundary::three_output_displacement_preserves_unit_and_returns`, modes/global-swap suites and restored-routing hotplug tests. Destination choice uses retained shared workspace-scope policy, not native-domain fallback identity. |
| `world_float_recovery.rs` | Exact token, lost result, stale/consumed command, ambiguity, confirmation, external supersession, refusal, close, unfloat/replacement and reset: capacity suite, especially `float_ambiguous_third_frame_holds_across_ack`, `float_explicit_supersession_settles_ambiguous`, `float_lost_result_recovers_stale_never_replays`, `float_close_and_reset_clear_token`; envelope and truthful settlement fence replay. |
| `world_focused_removal_fallback.rs` | Focused-close hold and each tree-position survivor reflow: `world_admission_close_stay::close_focused_holds_until_survivor_focus_recovers` and `world_coverage_gaps::close_each_position_reflows_controlled_survivors`. |
| `world_output_send_stay_focus.rs` | Exact MRU survivor versus follow mover, both horizontal directions, empty source, held/refused silence and no settlement replay: logical/close-stay tests and `world_coverage_gaps::{directional_sends_both_directions_name_exact_focus_without_replay, refused_and_held_sends_emit_no_focus_and_retain_nothing}`. Aligned native binding variants are superseded, not the focus policy. |
| `world_pending_send_focus.rs` | Pending stay/follow and emptied source: same-output observed-membership settlement in modes suite, directional focus/no-replay in coverage suite and contradictory-receipt bars in truthful suite. Native destination creation is no longer a prerequisite for logical commit. |
| `world_startup_conversion.rs` | Native shared-current/distinct-current/binding/echo conversion is superseded by native-free bootstrap and optional host-owned receipts in logical/modes suites. Hidden/minimized/fullscreen facts remain covered. Actual platform restart restoration is explicitly pending 2b, not claimed by reobservation or desired snapshots. |
| Windows `tiling_sys/world_diff_tests.rs` | Restored as 36 source-first offline tests: real/iconic/hidden/unreadable/overlay capture, effective-not-allocation geometry, clamp/refusal rearming, all per-flag/lifetime fences, generation/revision/watermark/gesture checks, invalidation before and after first write, bounded tables, ambiguous transfers/views/floats/effects, exact follow-view/focus proof, partial migration/refill, held scopes, consumed catch-up and exact acknowledgement receipts. Only native binding/creation/removal scaffolding is removed; T2's source-state outcome remains pinned in the core logical suite. |

The deleted production `world_displacement.rs` responsibilities are retained
in logical lifecycle plus `world_members::{rekey_members,
relocate_session_for_move}`, exercised by the hotplug, return and rollback
tests above. No accepted product behavior is removed by that source split.

### Final verification and native smoke

- Final locked core/protocol/Windows build and tests pass: 2,648 passed,
  zero failed, one ignored across 69 suites. Strict locked all-target clippy,
  workspace fmt and whitespace checks pass. KDE/macOS and their FFI consumer
  ports are not compiled or repaired in this slice. No dependency, unsafe,
  input-hook or cursor-policy change was introduced.
- The final Windows artifact passed fixture-only smoke on both outputs:
  four joined native lifetimes tiled on their staged outputs; directional
  focus/move on each; float/unfloat; fullscreen coverage/exit on each;
  cross-output send/follow both ways; workspace send/select hide/reveal and
  close/reflow on each. Native PMv2 DWM frames, visibility and foreground
  independently corroborated outcomes and managed-token joins. No synthetic
  input was used. Windows still ran its existing sole legacy owner.
- Exact-owner stop and independent restore were proven before fixture effects
  and again at teardown. Fixture shutdown exited cleanly; owned processes,
  recovery ledger and control requests were absent. Settings bytes, relevant
  SPI/taskbar state and protected-window geometry matched baseline.
- Initial smoke assertions confused DWM frames with outer frames, explicit
  float fallback with frame preservation, and plain hide with cloaking. Source
  and native readbacks corrected those oracles; no product workaround was
  made. Raw source/artifact identities, gate logs and native evidence remain
  outside the repository. Physical shortcuts, drag and games are not claimed.

### Next slice boundary

- 2a has no outstanding implementation blocker. 2b must replace the Windows
  ManagedWorkspaces/direct-Engine owner atomically, with genuine applied
  membership/view receipts, startup/restart restoration, floating membership
  and refill actuation, all scope/send routes, and deletion of superseded
  per-domain adapter interfaces. The offline world route remains test-only.
- KDE/macOS ports must use contract v2, capability opt-ins and truthful receipts;
  their old native-domain/binding APIs intentionally no longer compile against
  this boundary. Their native safety/integration mechanics remain their own.
- 2c remains cursor/settings/focus retention, approved fresh-placement/switch
  policy and dual-output admission correction. Do not confuse startup or
  restoration with fresh placement. Three-way placement subsuming launch
  targeting remains tentative and unimplemented. No other tentative product
  choice was introduced; all four parked questions above remain unchanged.

## Stage 2b partial implementation checkpoint (2026-10-21)

### Ownership and routing

- Implementation is partial and not accepted for Windows dogfooding. Stage 2c
  has not started. The stage 2a evidence above remains evidence for its earlier
  artifact, not native acceptance of this runtime cutover.
- `TileLoop` now retains `WorldOwner`/`WorldSequencer` as its sole world owner.
  The direct Engine, ManagedWorkspaces and WorkspaceScopeOwner fields are gone.
  The legacy `workspace.rs`, `workspace/`, `workspace_scope_owner.rs`,
  `workspace_owner/` and settings `tile_bridge.rs` implementations are deleted.
  Required native identity, marker and lifetime primitives moved to
  `native_guards.rs`; they do not retain workspace rings or history.
- Observation/reconcile, close/reflow, output changes, numbered/relative/
  previous/trailing selection, numbered/relative/output follow/stay sends and
  workspace migration now dispatch through the world route. Resize,
  orientation, float/tiling/sticky actions, directional focus/move and
  drag/drop/preview/group queries have been adapted to world commands or
  world operations. These are source-routing claims, not native acceptance.
- `world_reads.rs` reads core logical mapping and reply presentation.
  `world_capture.rs` projects normalized host facts; `world_native.rs` actuates
  through existing identity-checked geometry, hide/reveal and focus mechanisms.
  Native receipt caches are distinct from core targets. Incorporation
  acknowledgement must never establish native arrival.
- Superseded Windows helper bodies and imports remain in `tiling_sys`.
  Portable per-domain CoreEvent/CoreReply application and session-query APIs
  have not been removed. Their removal, dead-code cleanup and an independent
  final ownership/deletion audit remain required. KDE/macOS ports are unchanged.

### Accepted local observations, not slice acceptance

- Final checkpoint locked build and tests for core, protocol and Windows pass:
  **2,156 passed, zero failed, one ignored across 69 suites**. The Windows
  library contributes 788 passing tests after legacy test removal. Passing
  totals do not close the coverage gaps listed below.
- The production native seam has 32 passing scripted tests. Positive cases
  exercise first bootstrap receipts, same-output numbered follow/stay,
  directional tiled crossings both ways, membership-only float crossings,
  lying setters and migration/source-refill settlement. The scripts update
  native observation maps through setters; desired snapshots alone are not
  their arrival oracle. The 36 source-first offline diff tests remain.
- T2 production sequencing evaluates one candidate and adopts it only for
  supported outcomes. The real no-fit directional regression checks unchanged
  retained source state and observation-only catch-up; float origin remains
  diagnostic. Explicit versus directional send semantics remain parked.
- Strict all-target clippy fails on remaining dead/unused adapter code and
  style findings. Workspace formatting and whitespace checks also fail.
  There has been **no stage 2b live run**, both-output fixture acceptance,
  native restart check or final artifact validation. Windows builds, but its
  runtime operation is not established by these offline results.

### Corrections and remaining verification

- Earlier cutover drafts incorrectly rehomed departing members during view
  switching, re-evaluated a staged candidate, copied desires into applied
  caches, and required an existing receipt before first arrival could prove.
  Corrections have executed scripted regressions; final independent review and
  native verification of their composition remain outstanding. FreshHost now
  seeds its view baseline from captured observed views.
- The hint producer is connected to the world preamble using the existing
  bounded hint/last-good mechanisms. Recycled-HWND retirement and token-scoped
  cleanup were repaired. Born/fullscreen/minimized/hidden, marker restore and
  first-exit capture cases have additional tests, but complete positive native
  admission/restart coverage remains pending.
- No durable workspace-ring store existed in the replaced Windows path.
  Windows currently mints logical rings from live screens and adopts native
  float/sticky/tile-override/recovery facts. The new neutral core RestoredWorld
  bootstrap ingress is tested but is not populated by Windows; unused ingress
  and persistence claims need reconciliation before completion.
- Deleted Windows policy tests have only a provisional coverage map. Core
  logical suites cover scope/history/migration/capacity policy and 12
  `native_guards` tests retain selected identity/marker/fence tables. This is
  not a complete replacement audit: positive adapter scope/hotplug/follow/stay,
  capacity retry, float/unfloat, born-maximized, visibility rollback, hint
  fallback/diagnostics and drag/pending-drop behaviors need named retained
  coverage or restored regressions. In particular, drag/pending-drop arrival
  checks still need review against effective frames and proven membership,
  rather than treating desired allocation as native proof.
- Action outcome logs and callers need a final audit: an accepted command or
  a returned reply is not a verified send/select/migration. Hidden fullscreen
  truth, partial batches, focused games, invalidation before each native effect
  and recovery after failed/ambiguous effects remain mandatory review targets.
- No new product decision is selected here. All four parked questions remain
  unchanged; three-way placement subsuming launch targeting remains tentative.
  No additional input hook or game-performance claim is introduced.
- Next: inspect the current positive native-seam tests and receipt paths,
  finish the deletion/coverage and outcome audits, remove superseded APIs and
  obtain strict local gates before fixture-only native checks on both outputs.
   Do not treat this partial checkpoint as a completed stage 2b slice.

## Stage 2b continuation checkpoint (2026-10-22)

This supersedes the implementation/gate gaps in the preceding checkpoint,
not its historical evidence. Stage 2b is still **not accepted**: whole-workspace
migration/refill has not passed the required native fixture leg. Stage 2c
and the dual-output fresh-placement correction have not started.

### Completed ownership and correctness work

- Removed obsolete Windows send, migration, placement, trailing and visibility
  helper routes and their dead imports/state. The sole production owner is
  `WorldOwner`; all retained layout sessions remain internal to core.
- Made `Engine`, `CoreEvent`, `CoreReply`, their plan carriers and the retained
  Engine query/mutation interface crate-internal. Public world command/result
  vocabulary remains. Removed obsolete Windows event builders and portable
  per-domain diagnostic helpers. Pure layout regressions moved into core's
  internal test modules; there is no public compatibility/test feature.
- Removed unused `RestoredWorld` ingress and its protocol field. Windows never
  populated it, and the replaced owner had no durable ring store. Startup and
  restart mint logical rings and adopt native marker/recovery facts; they do
  not promise persistent workspace order or hidden-view restoration.
- Committed floats enter the same native marker/snapshot lane independently
  of diagnostic `FloatOrigin`. Unsupported staged tiled-to-float output
  crossings still discard the candidate without changing committed source
  mapping/layout/focus.
- Busy/catch-up commands return unverified rather than replaying an older
  reply as success. Drop/pending-drop completion uses independently read
  `effective_frame` or proven membership, never canonical `allocation`.
- Fresh trailing sends stage the newly minted logical destination before
  planning. Numbered workspace sends reject a foreign output before minting;
  explicit versus directional cross-output policy remains unchanged.
- Successful same-output follow sends publish the target logical view and a
  separate retained follow-view operation. Stay sends leave the source view
  unchanged. Member and view arrival still require independent receipts.
  Merely updating the logical view without retaining its actuation flight
  was a failed intermediate approach: frameless redirect rows deadlocked.
  The retained-flight composition replaces that approach.
- Native observations now distinguish the managed member lifetime stamp from
  the legacy owned-helper proof tag. Generic applications have no helper tag;
  that absence must not erase valid member/view receipts. Unknown membership
  preimages hold rather than falsely proving external supersession. Helper
  proof/allowlist identity checks remain separate and unchanged.
- A stay-created hidden workspace can reconcile with no retained/staged focus
  when no close is pending. Previously that valid focusless state held the
  scope unreadable forever, blocking the reveal needed to make it focusable.
  Focus is not invented; the explicit-close survivor hold remains covered.
- Trace-only fresh-admission identity joins and bounded `world-apply` summaries
  expose operation feedback, held scopes and clearly labeled desired transfers.
  Selection diagnostics name the resolved output/core workspace ids rather
  than a stale output cache. These are owner-side event-driven diagnostics,
  not new input hooks, and do not turn accepted replies into native completion.

### Deletion and regression coverage

Core suite names in the earlier deletion table are historical integration
filenames. White-box suites now live in `crates/tiler-core/src/*_tests.rs`;
their internal Engine assertions were retained, not dropped to hide API
breakage. Portable public-world suites remain under `tests/`. Removed restore
ingress tests are superseded only where they exercised that unused input;
quiet bootstrap/newcomer/trailing behavior remains covered.

The following Windows paths are relative to `crates/tiler-windows/src/` unless
an integration path is shown. Each deleted test file's retained behavior is
mapped below; native-derived owner mechanics are superseded, not a second
accepted route. Production coverage comes from the world/native seam, not the
now-test-only `product_hide::batch` state machine.

| Deleted test file | Retained behavior coverage / justified supersession |
| --- | --- |
| `tiling_sys/tests_admission_settings.rs` | Windows settings/admission projection and fixed-hint producer tests; core ordered window-policy fixtures. |
| `tiling_sys/tests_born_maximized.rs` | Windows born-maximized/first-exit/restart tests and native-seam born-fullscreen/quiet-first-exit tests; core fixed-size/born-state suite. |
| `tiling_sys/tests_capacity_floats.rs` | Core `world_capacity_diagnostic_tests`, capacity-admission suites and origin-parity regression; old adapter capacity policy is deleted. |
| `tiling_sys/tests_capacity_recipe.rs` | Same core capacity/admission policy plus restored-routing capacity fallback; no adapter eviction policy retained. |
| `tiling_sys/tests_cross_drag.rs` | Native route `drop_arrival_*`, core drag-preview/drop routing and Windows gesture-state tests. |
| `tiling_sys/tests_d23_minimize.rs` | Core `minimize_restore_tests`, native capture minimized-omission and diff hidden/unreadable tests. |
| `tiling_sys/tests_d23_minimize_hidden.rs` | Same minimized/hidden suites, native receipt and hidden-return composition regressions. |
| `tiling_sys/tests_deferred_send.rs` | Core floating/deferred/slotless send suites and Windows membership-only float crossing tests. |
| `tiling_sys/tests_directional_cross.rs` | Core session cross-output/restored-routing suites and native tiled-crossing-both-ways regression. |
| `tiling_sys/tests_fixed_predicate.rs` | Core fixed-size admission/predicate suites and Windows normalized hint producer regressions. |
| `tiling_sys/tests_float_nav.rs` | Windows float-focus projection and core float-focus/cross-output suites. |
| `tiling_sys/tests_float_unfloat.rs` | Windows toggle/restart marker tests, core float membership/boundary suites and native marker-intent/forget-member regressions. Old marker sweep owner is superseded. |
| `tiling_sys/tests_orientation.rs` | Core session toggle-orientation suite and world operation routing. |
| `tiling_sys/tests_output_send.rs` | Core workspace/output send suites, native follow/stay/crossing tests and production-send route tests. |
| `tiling_sys/tests_pending_drop.rs` | Restored production pending slot-machine suite (25 tests). Its module header maps all 29 old tests individually, including frame/readback/refusal coverage in native/diff suites and remaining physical-drag gaps. |
| `tiling_sys/tests_scope.rs` | Core workspace-scope/modes/global-swap suites; adapter rings/history are superseded. |
| `tiling_sys/tests_scope_hotplug.rs` | Core logical hotplug/displacement/return and modes suites; adapter displacement owner deleted. |
| `tiling_sys/tests_scope_send.rs` | Core scope/send suites and native/production send routes. |
| `tiling_sys/tests_select_repair.rs` | Core relocated gap-repair replay tests, Windows border-shell and reconcile projection coverage. |
| `tiling_sys/tests_timeout_fallback.rs` | Restored `tests_hint_fallback.rs` (8 cases): fresh/timeout/empty/invalid/failed/budget/lifetime/token fallback table over the live hint consumer. |
| `tiling_sys/tests_visibility_batch.rs` | Native-seam partial hide/reveal, lying setter, identity drift and truthful receipt tests; diff partial/invalidation tests. Old batch orchestration is superseded. |
| `tiling_sys/tests_window_diag.rs` | Deleted batching has no production consumer; retained identity-break/rebind/drop-sample diagnostics are asserted in pending-drop tests. Visibility/exception telemetry suites remain. |
| `tiling_sys/tests_workspace_migrate.rs` | Core logical/refusal/migration suites and native `source_refill_migration_*` plus diff migration tests. Whole-workspace native acceptance remains outstanding below. |
| `workspace/tests.rs` | Core workspace-scope/modes/global-swap/logical lifecycle suites; native-desktop ring-floor and old preactivation machinery are superseded by logical ownership. |
| `workspace_scope_owner.rs` (embedded tests) | Core workspace-scope/modes/logical history and trailing lifecycle; the adapter scope owner is deleted. |
| `workspace_owner/fixed_admission_tests.rs` | Core fixed-size/capacity-admission suites and normalized Windows hint tests. |
| `workspace_owner/tests.rs` | Core session lifecycle/send/movement, Windows `native_guards` identity/marker/fence tests and native world receipt tests. |
| `workspace_owner/tests_float_send.rs` | Core floating-deferred send/float-boundary suites and native membership-only crossing cases. |
| `workspace_owner/tests_hints_markers.rs` | Core size-hint/fixed-admission tests, retained Windows restart marker and hint fallback tests. |
| `workspace_owner/tests_layout.rs` | Core session movement/lifecycle/gap-repair suites and native frame/write-readback composition. |
| `workspace_owner/tests_send.rs` | Core send suites and native follow/stay/hidden-return composition. |
| `workspace_owner/tests_send_follow.rs` | Core send focus/history tests and native `same_output_numbered_send_follow_selects_proves_and_settles` (no external select rescue), partial/lying-setter negatives. |
| `workspace_owner/tests_toggle.rs` | Core orientation/float suites and Windows native marker/restart tests. |
| `../tests/workspace_migration.rs` | Core migration mapping/refill/MRU/refusal tests plus native source-refill migration seam. |

Additional final regressions pin generic-app member/helper-tag separation,
unknown-preimage versus genuine external supersession, full
follow/select-back/stay/hidden-return composition, and real claim
hide/reveal/re-hide. The native test harness now snapshots observed frames as
production does. A duplicated per-test ledger directory caused one parallel
`LockContended` gate failure; the colliding test identity was separated rather
than serializing or suppressing the test.

- Final locked core/protocol/Windows build, tests and strict all-target clippy
  pass: **2,202 passed, zero failed, one ignored across 48 suites**. Workspace
  fmt and whitespace checks pass. The full parallel run includes the repaired
  claim-cycle test. No dependencies, unsafe code or input hooks were added.
- Independent source review checked the public-boundary removal, receipt
  composition, helper/member-tag separation, unknown-preimage handling and
  focusless/no-close convergence against the retained close-survivor fence.
  KDE/macOS ports remain outside these gates; portable FFI checks through the
  Windows tests are not a native KDE build.
- Split native receipt capture from actuation and primitive seam tests from
  world-composition tests at the file-size review boundary. Receipt functions
  and assertions moved without semantic changes; full tests, strict clippy,
  formatting and whitespace checks pass after the split.

### Native evidence and limits

- Fixture-only native Windows checks on both outputs passed admission,
  numbered/relative/previous/trailing selection, sticky-anchor visibility,
  same-output fresh trailing follow/stay sends, and positional return into
  the stay-created hidden workspace. Member/HWND/process-lifetime joins,
  PMv2 frames, visibility and foreground independently corroborated effects.
- Both-direction explicit output follow/stay sends passed native frame/output
  and focus checks, with the source survivor expanding. This is **single-window
  send/source reflow**, not whole-workspace migration/refill acceptance.
- Both outputs passed maximize/work-area coverage, borderless-fullscreen/full
  monitor coverage and tiled return. Native coverage, not the fixture's unknown
  fullscreen-mode field, was the oracle.
- Restart preserved intentional float and sticky markers and held a running
  fullscreen window without tiling it. Hidden members were revealed by recovery
  and logical rings/views restarted; persistent hidden views were not an old
  supported guarantee and are not introduced here. Tile-override restart remains
  offline-covered but was not staged live.
- Closing an intentional float preserved the neighboring tile; closing a tile
  expanded its survivor. Exact stop/independent restore and fixture shutdown
  left no owned process/window, ledger or pending controls; settings bytes,
  relevant SPI/taskbar state and protected geometry matched their preimages.
- Directional smoke passed local horizontal focus/move on each output,
  cross-edge move and return, and cross-edge focus. All 11 dispatches were joined
  to bounded world-apply evidence and independent PMv2 frame/foreground reads;
  the final artifact's own hash was checked against the launched payload.
- Three live product defects were reproduced, regression-fixed and rechecked:
  fresh trailing target staging/follow view ownership; generic member-stamp
  receipt capture; and focusless hidden stay-workspace return. Earlier first
  selection attribution and mid-run Mouse Snap equality failures were oracle
  errors, not product repairs. Mouse Snap intentionally changes during the
  active session and must match baseline after teardown.
- Whole-workspace migration/refill still needs a native fixture leg. Windows
  currently exposes it only through a bindable, default-unbound physical chord;
  the exact-owner CLI has no migration control. Do not substitute single-window
  output sends or injected-key delivery for this check. Shared/global live mode
  coverage and physical shortcut/drag acceptance are
  separate outstanding evidence. The disabled drag driver remains disabled.

### Next boundary

- Finish the missing whole-workspace migration/refill
  native control/acceptance path before accepting 2b or combining 2a+2b on the
  Windows dogfood branch. Keep the existing parked focus/send questions unchanged.
- After 2b acceptance, implement 2c retained cursor/settings/focus and approved
  fresh-placement/switch policy, then the remaining convergence/presentation
  stages. Startup/restart is not fresh placement. Three-way placement subsuming
  launch targeting remains tentative, not selected by this checkpoint.

## Stage 2b accepted outcome (2026-10-22)

This supersedes the missing whole-workspace migration acceptance above.
Stages 2a and 2b are accepted together for the logical-world/Windows-owner
boundary. Stage 2c and stages 3-5 remain incomplete; this is not acceptance
of cursor-first fresh placement or the entire rearchitecture.

### Final migration and receipt corrections

- Added the tentative dev-only exact-owner `migrate-workspace --direction`
  control, using the existing normal-owner control boundary and production
  migration handler. Proof owners refuse; lifecycle cleanup covers its request
  file. No binding, default chord, input hook or injected-input exception was
  added. Both physical and CLI migration use the existing live output-context
  selector instead of an initialization-only output cache; the later
  cursor-first policy is not selected early.
- Reproduced the initial native rejection as incomplete inventory despite
  readable managed members. Capture now ignores foreign never-member rows and
  orphan non-member bookkeeping; bound-member drift, unreadable facts,
  incomplete enumeration, hidden/minimized truth and explicit closes retain
  their safety fences. Core and owner diagnostics expose bounded fixed refusal
  reasons rather than hiding command rejection behind `migrate-unverified`.
- Reproduced the occupied-target migration deadlock: frameless redirect-member
  arrival required the target view, while the target view awaited all members.
  Native migration member proof is now independent of that deferred view,
  using the effective actuation frame or mechanical output landing, preserved
  float frames, and existing lifetime/identity/scope/visibility gates. Setter
  success and canonical allocation are not arrival proof. Retained ambiguity
  and spent view phases still prohibit replay.
- Reproduced a second composition defect: empty source-refill views never
  receipted when other hidden workspaces retained homing on the output. The
  empty receipt now requires valid claim identity/tag association, a nonempty
  member stamp, no observed reuse/reveal of that HWND, and no visible managed
  presence. Foreign never-members do not fence scoped completion. Invalid or
  stale claims, visible-capable or unclaimed homing and genuinely unreadable
  members do not fabricate proof. This lets the parent settle before inverse
  migration, selection and close/reflow.
- Producer-shaped regressions cover occupied-target arrival, both-direction
  migration with hidden source members, target/refill settlement, selection
  into the retained occupied workspace, return, and close/reflow. Nine direct
  empty-receipt cases pin claim drift, identity/tag/stamp failures, minimized
  homing and foreign presence. Migration composition tests moved into a focused
  module at the test-file review boundary.
- Independent reviews accepted control authentication/cleanup, managed-scope
  completeness, migration arrival proof and the hardened empty-refill receipt.
  Review found and corrected the first receipt draft's insufficient claim
  association and oversized test module before final native acceptance.

### Final native acceptance

- The final artifact passed fixture-only native Windows checks on both outputs.
  Six caller-identified windows were joined to exact native lifetimes and
  managed tokens; physical frames, visibility and foreground independently
  established outcomes. Three members migrated as one workspace into an
  occupied target and back. Order, layout shares, identities and follow focus
  survived both directions; each membership phase proved all three arrivals.
- Both default source refills showed the last remaining trailing empty, not the
  occupied hidden workspace. The other source/target workspaces remained hidden
  with their members intact. Target and refill views settled, and no Migration
  or View flight remained before subsequent actions. Later observations showed
  no membership flap or effect replay.
- Post-roundtrip numbered selection revealed the retained occupied workspace;
  previous restored the migrated unit. Trailing selection and previous return
  preserved its order and shares. Previous without a distinct post-migration
  history entry correctly remained a self-no-op.
- Closing the focused bottom tile of a stacked pair expanded its sibling to
  their full column while preserving the full-height singleton. Native frames,
  stable survivor lifetimes, victim absence and controlled/no-flight output
  established reflow. Closing a hidden member on the other output preserved
  its hidden sibling. Earlier timeout assertions requiring both survivors to
  grow were an incorrect harness oracle, not a product reflow defect.
- Exact-owner stop, independent restore and fixture shutdown left no owned
  processes/windows, ledger or pending controls, including the migration
  request. Settings bytes, relevant SPI/taskbar state and protected-window
  geometry matched baseline. Raw source/artifact provenance, scripts, native
  snapshots and owner trace remain outside the repository.
- Other corrected live expectations were ordinal rebinding after migration and
  the default last-remaining versus opt-in MRU refill distinction. They did not
  change product policy. Physical shortcuts/drag and games are not established
  by these exact-owner fixture controls.

### Gates, limits and next boundary

- Final locked core/protocol/Windows build, all-target tests and strict
  all-target clippy pass: **2,223 passed, zero failed, one ignored across 46
  suites**. Workspace formatting and whitespace checks pass. No dependency,
  unsafe code or input hook was added. The fixture checks use the final
  production source; the last added close regression is test-only.
- The public per-domain API deletion and sole Windows owner remain as recorded
  above. KDE/macOS intentionally still require their v2 port; they are not
  repaired or claimed to compile by these Windows gates.
- Separate native gaps remain: MRU occupied-source-refill staging,
  global-unique/shared modes, tile-override restart staging and user-owned
  physical shortcut/drag/game acceptance. The default local migration and
  empty-refill acceptance gap is closed, not substituted with output sends.
- Next: stage 2c retained cursor/settings/focus, approved cursor-first switch
  and fresh-placement policy, shared world-action/native acceptance and both
  outputs' cursor/focus-disagreement checks. Startup/restart is not fresh
  placement. Three-way placement subsuming launch targeting remains tentative;
  all four parked focus/send questions remain unchanged. Do not start stages
  3-5 as though 2c or the dual-output fresh-placement defect were complete.

## Stage 2c implementation and native correction checkpoint (2026-10-22)

- Core now retains cursor and targeting settings, resolves implicit numbered,
  relative, previous and trailing selection, and routes fresh tiled lifetimes
  through the approved COSMIC, Hyprland and i3/sway placements. Explicit
  screen selections remain explicit; sends retain focused-member origin.
- The first complete inventory establishes the startup/restart baseline,
  including an empty roster. Later fresh lifetimes route after empty-start,
  close-all and incomplete gaps. Native marker restoration remains adoption,
  not fresh placement. Settings changes never rehome retained members.
- A shared nine-case world-action fixture now drives core-only and production
  Windows native-seam assertions for target output, anchor, unchanged peers,
  frame/member/view proof and no replay. This does not substitute for live
  acceptance.
- Native testing reproduced cross-output size scaling during a DPI transition.
  The native frame actuator now reobserves a real DPI change and reasserts the
  same intended physical frame once through fresh insets, with renewed
  eligibility, identity, lifetime, input and fullscreen vetoes. Stable clamp
  policy is unchanged. Subsequent native checks passed both directions of
  COSMIC and Hyprland placement and one i3/sway direction.
- The inverse i3/sway native case exposed a missing retained-focus fallback.
  Its exact admission-time foreground was not recorded, so foreground loss is
  a tested causal hypothesis, not a recovered native fact. Core now retains
  genuine managed output focus for fresh admission; unknown or stealing-new
  foregrounds cannot erase that context. Regression cases cover missing,
  unknown and new-lifetime foregrounds. Bounded trace output exposes future
  fresh-routing evidence without titles or raw input.
- This checkpoint preceded the final acceptance below. Focus-follows-cursor and cursor-warp
  are retained-only settings, default off; their behavior and i3/sway preset
  warp belong to stage 4. No input hook or continuous cursor poll was added.

## Stage 2c accepted outcome (2026-10-22)

- The final Windows artifact passed all nine shared
  `test-fixtures/world-actions-cursor.json` cases and an additional empty-start
  first-newcomer discriminator. Both outputs passed all three placements under
  cursor/focus disagreement, including native birth on the opposite policy
  output and foreground-stealing newcomers. Native physical frames prove the
  correct target and split anchor: COSMIC target MRU, Hyprland cursor-closest,
  i3/sway focused-output MRU. Stable peers and source tiles stayed unchanged;
  newcomer plus anchor covered the anchor's prior tile across DPI transitions.
- Both switch-target settings selected their expected output while leaving the
  other output's members visible and unchanged. A focused-origin numbered stay
  send ignored the opposite cursor, hid the mover, expanded its source survivor
  and proved the target member transfer. The empty-start newcomer followed the
  cursor rather than native birth affinity. Exact native lifetime joins,
  visibility, foreground and physical frames corroborated the case assertions;
  late reads found no flap or replay.
- Core-only and Windows production-native-seam runners consume the same case
  data. Additional regressions cover all four selection verbs, sends,
  same-output Hyprland focus anchoring, startup/restart, close-all, incomplete
  gaps, incarnation resets, focus handoffs and settings changes without rehome.
  Native DPI tests cover both directions and refusal of the corrective write
  after identity drift, new ineligibility or focused-fullscreen veto.
- Independent integration/safety review accepted the final source and native
  evidence. Locked core/protocol/Windows build, all-target tests and strict
  all-target clippy pass: **2,296 passed, zero failed, one ignored across 48
  suites**. Workspace formatting and whitespace checks pass. No dependencies,
  unsafe code or input hooks were added. Snapshot cursor sampling adds one
  bounded native cursor read per normal capture, not continuous mouse work;
  focused fullscreen games skip that sample. No zero-cost claim is made.
- Exact-owner stop, independent restore and fixture shutdown after every leg
  left no owned processes/windows, ledger or pending controls. Settings bytes,
  relevant SPI/taskbar state and protected-window geometry matched baseline.
  Raw source/artifact provenance, scripts, native readbacks and traces remain
  outside the repository.
- Two product defects were reproduced and corrected: cross-DPI framework
  resizing and loss of pre-admission focused-output context. Earlier script
  property/arithmetic/shell issues were harness failures. The extra expectation
  that previous must restore an output after focus moved elsewhere, and that
  a stay-send source survivor must not reflow, were incorrect harness oracles.
  Neither caused a product-policy change. The parked empty-workspace focus
  behavior remains unchanged; current focus can legitimately retarget a later
  focused-output previous command.

### Remaining boundary and exact next stage

- Stage 2c switch/fresh-placement adoption and the dual-output fresh-admission
  defect are complete. Focus-follows-cursor and cursor-warp are persisted and
  retained, default off, but deliberately have no active dialog controls or
  behavior yet. Stage 4 must implement their policy, including the i3/sway
  preset's cross-output warp; no all-settings reference preset is introduced.
- KDE/macOS must add the required world report fields `cursor`, `switchTarget`,
  `freshPlacement`, `focusFollowsCursor`, `cursorWarp`; implicit selection uses
  nullable `out`, while explicit screen selections remain explicit. The four
  new core catalog fields also need native settings/exporter coverage; the KDE
  exporter's old catalog-partition assertion is not a supported port. These
  consumers are intentionally outside the Windows gates.
- Stages 3-5 remain. Next: move observed convergence, stable clamp/refusal/
  adoption, anti-fighting and completion from Windows into the retained core.
  Keep mechanical DPI/inset conversion and freshness/lifetime vetoes native.
  Then extract gesture, focus/warp and desired presentation, followed by the
  inventory/common-connector audit. Do not treat this slice's native geometry
  checks as stage 3 completion or synthetic input as physical acceptance.
- Existing native gaps remain MRU occupied refill, global-unique/shared modes,
  tile-override restart and physical shortcut/drag/game acceptance. The disabled
  synthetic drag driver stays disabled. The four parked focus/send questions
  remain unchanged; three-way placement subsuming launch targeting remains
  tentative pending review.

## Stage 3a accepted outcome (2026-10-22)

### Frame convergence ownership

- Core now owns the existing identical-pair clamp suppression and transient
  retry policy. `WorldReport.frames` carries raw `FrameAttempt` facts: exact
  lifetime, requested frame, setter result, immediate readback, DPI basis and
  attempt time. Windows reports no clamp/suppress verdict. Core compares
  desired and observed frames and publishes a narrow frame-authority hold or
  withholds an explicit float-placement token. Membership authority remains
  independent; ordinary floats never acquire automatic frame authority.
- An accepted write with a stable readable mismatch suppresses only that
  desired/observed pair. A changed desire or observation re-arms; exact arrival
  clears. Refused/throwing setters and accepted writes without readable proof
  use the existing 500ms doubling backoff, capped at 5s. Retry still rides the
  next eligible event, not a newly introduced timer or polling loop.
- Consumed-attempt watermarks survive exact-match clears: lost acknowledgements
  cannot resurrect an old clamp or replace newer-pair authority. Equal-time
  changed evidence remains actionable; time is not a unique attempt id. Both
  suppression and watermark maps prune against the same retained lifetime
  predicate, explicit closes and incarnation resets. An unreported clock
  (`0`) provides neither transient suppression nor replay deduplication.
- Changed DPI basis invalidates accepted placement evidence, never establishing
  a stable clamp. Native conversion still owns invisible-border insets and one
  freshly gated DPI reassertion. Identity, lifetime, scope, input, fullscreen
  and last-moment freshness vetoes remain native. Removed Windows
  `RefusedTracker`, its backoff constants, readback verdict helpers and obsolete
  exports/tests; core frame-policy and Windows roundtrip tests replace them.
- Review reproduced and corrected three evidence defects before acceptance:
  unreadable accepted-write proof incorrectly becoming a permanent clamp;
  stale feedback resurrecting suppression after an exact clear; and consumed
  watermarks surviving complete-inventory departure without an explicit close.
  Each has a failed-before/passing-after core regression.

### Actionable cross-output placement

- Native verification reproduced an off-screen return send between asymmetric
  outputs: a membership-only pre-arrival row left Windows to translate the
  source rectangle by the output-origin delta. That translation preserved the
  wrong slot size and could land beyond the target's exclusive edge. A
  producer-shaped negative-origin migration case reproduced the same failure
  for tiled workspace members.
- Core now publishes the destination Engine slot before arrival for tiled
  cross-output window transfers and migration members. Native sweep writes
  defer until the membership phase, which consumes that exact target; readback,
  affinity, membership and view/refill receipts still establish completion.
  A desired slot or successful setter never proves arrival. Same-output
  ordering, floating-boundary transactions and held/unprojectable lanes remain
  unchanged.
- One rejected intermediate approach also exposed slots for same-output
  movers, breaking five follow/view-ordering regressions. The accepted rule is
  cross-output-only; do not broaden it to same-output transfers without
  resolving that sequencing contract.

### Verification and native evidence

- Locked core/protocol/Windows build, all-target tests and strict all-target
  clippy pass: **2,317 passed, zero failed, one ignored across 48 suites**.
  Workspace formatting and whitespace checks pass. The core frame suite has
  23 cases, including suppression/rearm, bounded retries, unreadable/DPI facts,
  replacement, close, replay-after-clear and watermark pruning. Production
  Windows diff/seam tests exercise the raw-attempt/report/desire/obey loop,
  asymmetric slot landing, partial settlement and lying setters.
- Fixture-only native checks on both outputs proved three-member whole-workspace
  migration into an occupied target and back, destination coverage, preserved
  order/shares/lifetimes, hidden peers, follow focus and target/source-refill
  completion. Fresh non-geometric focus-command replies showed no retained
  transfers in either direction. Quiet observation ticks omit trace rows, so
  absence of a terminal trace line is not proof of a retained flight.
- After the final replay/pruning corrections, the final artifact separately
  passed four frame perturb/reconvergence cases, two per output, with one
  geometry-write row per perturbation, unchanged peers and stable later reads.
  Both outputs passed close/reflow and fresh replacement/lifetime joins;
  visibility select-away/back, float/unfloat and both-direction cross-DPI sends
  also passed. A fresh focus-command snapshot showed no retained transfers.
  Whole-workspace migration was verified before those core-only corrections,
  not repeated on the final binary; its migration implementation was unchanged.
- Earlier harness failures were native-stderr handling and incorrect oracles:
  no-op selection diagnostics do not identify workspace tokens, quiet logs do
  not expose every settlement, and an ordinary send return need not restore
  its old slot/order/shares. Native occupancy, fresh reply snapshots and actual
  frame coverage replaced those assumptions; no product policy was changed.
- Exact-owner stop, independent restore and graceful fixture shutdown after
  each leg left no owned processes/windows, ledger or pending controls.
  Settings bytes, relevant SPI/taskbar state and protected geometry matched
  baseline. Raw provenance, scripts, artifact hashes and readbacks remain
  outside the repository.
- Fixture size hints are not programmatic clamps. Clamp/refusal/backoff and
  lost-ack predicates are offline evidence only; sub-sample transient frames
  were not observed and no visible-duration claim is made. No dependency,
  unsafe code, input hook or polling loop was added. Actual frame attempts
  add two DPI reads and bounded feedback bookkeeping; cost was not benchmarked.

### Remaining stage 3 and next boundary

- This is a working Windows slice, not completion of stage 3. Next: core-owned
  learned-constraint adoption and remaining transfer/view/float completion and
  anti-fighting decisions. `world_diff.rs` still interprets operation outcomes,
  while core settlement retains their results. Finish that extraction without
  weakening ambiguous-effect, hidden/incomplete or partial-transfer fences.
- Frameless floating-source/unprojectable migrations still use the native
  remap fallback. The tiled asymmetric-output hazard is fixed; the frameless
  boundary needs its own placement contract and regression before claiming
  all cross-output geometry is core-owned. Do not substitute a native clamp.
- Shared convergence case data/core runner/native connector remain stage 3
  acceptance work; the existing cursor-action connector is not that coverage.
  KDE/macOS ports must supply raw `frameAttempts` and a coherent `monotonicMs`
  clock to use this policy; zero/missing timing cannot promise retry backoff or
  deduplication. Their old policy copies remain unported and outside the gates.
- Stages 4 and 5 remain: gestures, focus/warp, desired presentation and pager
  content, then the ownership/dead-API/common-connector audit. Existing MRU
  refill, shared/global modes, tile-override restart and physical acceptance
  gaps remain. All four parked focus/send questions and tentative placement
  subsuming launch targeting remain unchanged.

## Stage 3b implementation plan

- First bounded slice: adopt conservative learned minimum sizes from raw frame
  attempts in core and feed them into the existing size-hint/layout path. Keep
  exact-lifetime pruning, replay fencing and current clamp/backoff suppression.
  Only stable readable accepted growth at the requested origin, without a DPI
  transition or a shrinking axis, can establish a learned minimum. Ordinary
  floats and protected/held evidence must not acquire tile policy.
- Acceptance: core-only learning/replay/lifetime/gap/constraint cases and shared
  convergence data exercised through the Windows native seam; unchanged setter,
  freshness and conversion mechanics; full core/protocol/Windows local gates,
  independent boundary review and fixture-only both-output reconvergence smoke.
  The live fixture's track hints do not establish programmatic clamp learning;
  keep that predicate explicitly offline rather than weaken its assertion.
- Later slices retain transfer/view/float completion and anti-fighting
  extraction, frameless/floating-source migration placement and connector
  breadth. Do not broaden cross-output-only pre-arrival tile publication or
  change the parked focus/send policies.

## Stage 3b1 accepted outcome (2026-10-22)

- Core adopts conservative learned minimum sizes from raw accepted frame
  attempts into the existing hint/layout path. Every world projection uses
  the merged hints; actual desired geometry changes, not only suppression.
  Evidence is exact-lifetime scoped, immediately readable and corroborated by
  current facts, time-stamped, same-origin and nonshrinking on both axes.
  Incomplete/unreadable/gesture scopes, floats, minimized/overlay/sticky/fixed
  rows, known DPI changes and clockless attempts cannot establish a minimum.
- Learned bounds are independent of pair suppression. Exact arrival clears
  suppression, not a bound; later eligible smaller observations disprove the
  corresponding axis. Invalidation uses the prior consumed-attempt watermark
  before learning, so replayed or clockless failures/undercutting targets cannot
  block fresh smaller evidence. Fresh refused/ambiguous/DPI-invalidated attempts
  and undercutting requests do not establish that counter-proof. Declared caps
  suppress conflicting learned axes without silently widening the cap, while
  stronger declared minima and existing infeasible-layout behavior are retained.
- Closing, complete-inventory departure and incarnation reset prune all three
  frame-policy maps together. A float excursion preserves the lifetime's bound
  but grants no automatic float geometry authority. The acquisition heuristic is
  [tentative pending dogfooding](../decisions.md#core-platform-boundary): an
  external enlargement racing a setter cannot be causally distinguished from
  an application clamp. Smaller stable evidence prevents permanent fighting.
- Shared `test-fixtures/frame-convergence.json` supplies acquisition and
  declared-maximum-conflict cases to core and the Windows production seam.
  Windows generates the growth attempt through real diff actuation/readback,
  then feeds its emitted raw attempt through the production sequencer. No
  setter-success echo or hand-built attempt substitutes for that connector.
  This begins the shared convergence connector, not its full acceptance table.
- Review corrected an initial synthetic-attempt connector, missing counter-
  evidence invalidation, zero-clock acquisition and stale replay vetoes.
  Regressions pin those boundaries, both axes, all veto flags, declared-hint
  dynamics, float excursions, infeasible minima and lifetime retirement.
  Independent final review accepted the corrected ordering and native seam.

### Verification and native limits

- Locked core/protocol/Windows build, all-target tests and strict all-target
  clippy pass: **2,337 passed, zero failed, one ignored across 48 suites**.
  Workspace formatting and whitespace checks pass. The core frame-policy suite
  now has 42 cases. No dependency, unsafe code, input hook or polling loop was
  added. Eligible reports add bounded constraint bookkeeping; its cost is not
  benchmarked and no zero-cost claim is made.
- Final-artifact fixture-only native smoke passed both outputs: four joined
  lifetimes tiled, one size/position perturbation per tile reconverged with one
  geometry-write trace row each, unchanged peers and stable later native PMv2
  frames. Sub-sample transient frames were not captured; no visible-duration
  claim is made. Both outputs passed close/reflow and fresh replacement/token
  joins; selection hide/reveal, float/unfloat and both-direction cross-DPI sends
  passed. A fresh nongeometric focus reply showed no retained transfers.
- A declared minimum hint was accepted and read back only as a hint. The
  fixture does not enforce programmatic clamps: learned-minimum acquisition,
  ambiguous writes and replay predicates remain offline evidence. Ordinary
  reconvergence smoke is not live proof of constraint acquisition.
- Exact-owner stop, independent restore and fixture shutdown left no owned
  processes/windows, ledger or controls. Settings bytes, relevant SPI/taskbar
  state and protected-window geometry matched baseline. Raw scripts, provenance,
  hashes and native observations remain outside the repository. No live product
  defect or unresolved restoration issue was encountered.

### Remaining boundary and next action

- Windows remains a working dogfood slice, not completion of stage 3 or the
  rearchitecture. Next: extract transfer/view/float completion and anti-fighting
  verdicts from `tiling_sys/world_diff.rs` into retained core settlement, keeping
  mechanical lifetime/freshness checks and truthful native observations local.
  Inspect `settle_view_attempt`, `settle_transfer_applied`, float outcomes and
  `EffectTracker` against `world_settle.rs`; preserve ambiguous-effect and
  hidden/incomplete/partial-transfer fences rather than merely moving helpers.
- Frameless/floating-source migration still uses native origin-delta remapping
  and needs a separate core placement contract/regression. Cross-output tiled
  pre-arrival slots remain unchanged; same-output broadening remains rejected.
- Stage 4 gesture/focus-warp/presentation/pager extraction and stage 5 ownership,
  dead-API/common-connector audit remain. KDE/macOS ports are intentionally
  untouched; they need the already-documented raw-attempt/timing contract, not
  a new learned-minimum field. Shared convergence case breadth, MRU occupied
  refill, shared/global modes, tile-override restart and user-owned physical
  acceptance remain gaps. The disabled synthetic drag driver stays disabled.
  The four parked focus/send questions and tentative placement/control choices
  are unchanged; no new product question blocks the next slice.

## Stage 3b2 implementation plan

- Next bounded slice: move explicit float-placement completion and its
  refusal/ambiguity decisions from Windows into core using operation-correlated
  raw frame-attempt facts, including the immediate pre-write frame. Remove
  Windows float outcome classification and applied-float policy memory; retain
  only mechanical dispatch fencing, identity/freshness gates and readback.
- Preserve exact-target observation confirmation, ambiguity across feedback
  acknowledgement, no replay, explicit-close and replacement fences, and the
  existing frame clamp/backoff owner. Ordinary floats gain no automatic writes.
- Acceptance: core-only setter/readback truth-table and consumption regressions,
  producer-shaped Windows raw-attempt/sequencer cases, full locked
  core/protocol/Windows gates, independent review and fixture-only native
  float-placement checks on both outputs. Clamps and ambiguous setters remain
  offline predicates because the live fixture cannot enforce them.
- Transfer/view completion and frameless migration placement follow separately;
  do not change same-output transfer ordering or the parked focus/send policies.

## Stage 3b2 accepted outcome (2026-10-22)

### Explicit float-placement completion

- Core derives float-placement confirmation, refusal and ambiguity from raw
  operation-correlated frame attempts plus observed frames. `FrameAttempt`
  carries `operation_id` and the immediate `prewrite` frame; wire attempt rows
  require `op` (empty for ordinary writes) and `pre` (null when absent). Exact
  operation, lifetime and target must match. Digest, validation and request
  sizing cover both fields. Ordinary floats gain no automatic frame authority.
- Windows emits no float `Property` outcome verdict and retains no
  applied-float policy map. It only checks mechanical authority/freshness and
  lifetime, fences dispatch at most once, writes, reads back and reports facts.
  Transfer/view outcome classification remains native and is not accepted as
  core-owned by this slice. Their ordering and T2 are unchanged.
- Confirmation requires readable observed arrival, never setter success.
  Proven refusal requires an explicitly refused setter, unchanged readable
  prewrite/readback and a later corroborating observation. Accepted mismatch,
  throw, missing proof or a DPI-invalidated write retains ambiguity without
  re-actuation. A fresh DPI transition bars same-report confirmation; later
  stable target observation can confirm. New float intent replaces the old
  operation; explicit close settles even across observation gaps.
- Correlated attempt evidence is retained under its float operation before
  inventory/gesture/minimized/unreadable holds, independently of the shared
  frame-consumption watermark. This preserves confirmation/refusal evidence
  after acknowledgement and prevents a clamped third frame from being mistaken
  for external input on recovery. Without ambiguous attempt evidence, the
  existing source/target external-input rule is unchanged.
- Independent review reproduced an initial gap-consumption defect for incomplete
  inventory, gesture barriers and DPI-changing third frames. The retained-raw-
  evidence correction and permanent recovery/ack/refusal/close regressions pass;
  final review accepted the corrected ordering and lifetime-scoped state.

### Verification and native limits

- Final locked core/protocol/Windows build, all-target tests and strict all-target
  clippy pass: **2,348 passed, zero failed, one ignored across 48 suites**.
  Workspace formatting and whitespace checks pass. Core-only cases cover the
  setter/readback matrix, exact correlation, forged ids, replay, observation
  gaps, acknowledgement, replacement and DPI transitions. Windows production
  diff/write/readback/sequencer cases assert raw correlation, no float verdict
  feedback and no repeated dispatch. The shared tiled convergence table is
  unchanged; float connector coverage is currently a production seam test,
  not an additional shared action-case table.
- The final artifact passed fixture-only native checks on both outputs: normal
  float toggles reached the exact core-selected centered frame, peers reflowed,
  one geometry-write trace row accompanied each placement, and later physical
  frames stayed stable. Unfloat restored tiled coverage; later quiet reads
  found no repeated actuation. Selection hide/reveal and both-direction
  cross-DPI stay sends also passed with independent frame/visibility/focus
  observations and exact caller-id/native-lifetime/managed-token joins.
- Production trace does not expose float-operation/raw-attempt correlation or
  retained-float state. Native frames establish placement, not internal flight
  retirement; that correlation and retirement remain offline assertions.
  Clamp/refused/throw predicates remain offline because fixture size hints do
  not enforce programmatic clamps. No physical drag/shortcut/game acceptance
  or transient visible-duration claim is made.
- Exact-owner stop, independent restore and fixture shutdown left no owned
  processes/windows, ledger or pending controls. Settings bytes, relevant
  SPI/taskbar state and protected-window geometry matched baseline. Raw scripts,
  source/artifact identities and native readbacks remain outside the repository.
  Initial PowerShell harness arithmetic and launch-gate issues were corrected;
  cleanup verified their bounded effects, with no product workaround or
  unresolved restoration issue. No dependency, unsafe code, input hook or
  polling loop was added. Explicit float attempts add one native frame read
  and bounded retained-evidence storage; cost was not benchmarked.

### Remaining boundary and exact next action

- Windows remains a working dogfood slice. Stage 3b is incomplete: next extract
  transfer/view completion and anti-fighting from `world_diff.rs` into core,
  including standalone/follow/target/refill phases and `EffectTracker` policy
  memory. Use raw correlated attempt/proof facts rather than adapter verdicts;
  retain mechanical dispatch fencing, lifetime/freshness vetoes and truthful
  native membership/view receipts. Stage 3b2 provides the gap-safe evidence
  retention pattern, not a completed transfer/view contract.
- Frameless/floating-source migration placement remains a separate slice;
  native origin-delta remapping is still active. Preserve cross-output-only
  pre-arrival tile publication and same-output follow ordering.
- Stages 4 and 5 remain untouched. The core interface is not yet settled for
  KDE/macOS ports: their float placements must emit raw attempts with `op`/`pre`
  instead of `Property` verdicts, and ordinary frame attempts must supply empty
  `op` plus null `pre`. Further transfer/view and gesture/presentation changes
  remain ahead. Existing tentative choices and all four parked focus/send
  questions are unchanged; no new product question blocks the next slice.

## Stage 3b3 accepted outcome (2026-10-23)

### Transfer and view completion

- Core now derives retained window, standalone/follow and migration target/
  refill completion from raw `MemberAttempt` and `ViewAttempt` facts. Exact
  operation/target/member correlation excludes unrelated evidence. Pending
  evidence stages before holds and survives acknowledgement; readable arrival
  wins over stale refusal. Explicit refusal needs an unchanged readable
  preimage corroborated by current facts. Ambiguous effects never re-actuate.
- Unknown view baselines and unchanged pre/post views cannot prove external
  supersession. Cross-output follow retains its view evidence and waits for
  exact observed view or genuine external proof across acknowledgement/gaps.
  Same-output membership still precedes its separate `{op}-follow` view flight;
  cross-output-only pre-arrival tile publication is unchanged.
- Migration keeps member, target-view and source-refill phases ordered and
  preserves partial arrivals. Refused refill evidence remains held with its
  child until late exact arrival or a known third source view proves newer
  input; child disappearance alone cannot manufacture that proof. Explicit
  closes, terminal migration reconciliation and replacement fences remain.
- Windows emits raw attempts, not transfer/view verdicts, and no longer keeps
  applied-window/applied-view or ambiguous policy maps. Its remaining tracker
  fences dispatch mechanically, including across missing baseline inventories.
  Last-moment identity, freshness, scope and visibility vetoes remain native.
  Raw attempts are retained even when the pass stops after a write.
- No-write pre-proven arrivals legitimately emit no attempt. Existing independent
  native receipt capture already handles them; a new singleton-follow seam
  regression proves zero setters/attempts and subsequent retirement of all
  flights. An initially suspected missing-receipt defect was disproven, not
  worked around by fabricated attempts or setter echoes.

### Verification and native evidence

- Final locked core/protocol/Windows build, all-target tests and strict
  all-target clippy pass: **2,360 passed, zero failed, one ignored across 48
  suites**. Workspace formatting and whitespace checks pass. Independent final
  boundary and evidence review accepted the corrected source. No dependency,
  unsafe code, input hook or polling loop was added; bounded attempt/evidence
  bookkeeping replaces native policy memory. Cost was not benchmarked.
- Review reproduced and corrected false supersession from unknown baselines,
  one-report-only follow holds, and refusal evidence losing its meaning after
  acknowledgement. Permanent cases cover correlation, gap/ack recovery, arrival
  precedence, current incomplete screens, refused refill retention and later
  external source-view input. Producer-shaped Windows tests exercise actual
  diff/readback/sequencer attempts and no-repeat dispatch.
- Final-source fixture-only native checks passed standalone selection hide/
  reveal on both outputs; trailing stay with selection return; and same-output
  trailing follow, including a pre-proven no-write arrival and a fresh empty-
  flight snapshot. The later numbered reunion sequence was not completed live;
  it remains covered offline, not claimed from these native checks.
- Both outputs passed cross-output follow in both directions, followed by a
  three-member whole-workspace migration into an occupied target and back.
  Exact lifetimes, native physical frames, visibility and foreground establish
  arrival, preserved order/shares, follow focus and intact hidden peers. Fresh
  accepted nongeometric replies show all member/view/refill flights retired
  after each crossing. Default source refills remained trailing empties; the
  occupied peers stayed hidden on inverse migration. Close/reflow proved victim
  absence and correct survivor expansion without changing hidden peers.
- Harness corrections concerned cursor targeting, JSON escaping, cached send
  diagnostics, no-write proof, workspace pruning/ordinals and rejected probes
  without desired snapshots. They changed no product policy. Raw correlation
  was checked in actual trace rows rather than a summary's mistaken refill-id
  attribution. Exact stop/independent restore and graceful fixture shutdown
  after every leg left no owned processes, ledger or controls; settings bytes,
  relevant SPI/taskbar state and protected geometry matched baseline. Raw
  scripts, source/artifact identities and readbacks remain outside the repo.
- Refusal/throw/clamp and observation-gap predicates remain offline evidence;
  the fixture cannot enforce programmatic clamps. No physical shortcut/drag/
  game or sub-sample visible-duration acceptance is claimed.

### Port contract and remaining boundary

- World contract v2 now accepts optional `memberAttempts` rows
  `{op,win,out,ws,setter,pre:{out,ws}|null,post:{out,ws}|null,attemptMs}` and
  `viewAttempts` rows `{op,out,ws,setter,pre,post,base,attemptMs}`. View proof
  fields are logical ids or null (empty wire strings normalize to unknown);
  counters are decimal strings. Each row records an actual setter, not a
  completion verdict. Standalone/refill views use their own id; cross-output
  follow and migration-target views use their parent id. Digest, validation,
  byte budgets and acknowledgement cover both sections.
- Transfer/view feedback verdict rows now reject the report; ports must remove
  them rather than keep a dual route. Independent member/view receipts remain
  required, including no-write proofs. KDE/macOS are intentionally unported;
  the interface is not settled enough to begin their full ports yet.
- Stage 3b remains incomplete. Exact next unit: core-owned frameless/floating-
  source migration placement replacing native `remap_to_output` and the
  `move_membership` remap fallback, with asymmetric-output regressions and
  both-output fixture verification. Preserve same-output follow ordering and
  cross-output-only tile slots. Shared convergence-case breadth still needs
  completion; this slice adds production seam coverage, not a new shared table.
- Stages 4 and 5 remain: gestures, active focus/warp, desired presentation/pager,
  then ownership/dead-API/common-connector audit and finalized port notes.
  Windows is a working dogfood slice, not the completed rearchitecture. MRU
  refill, shared/global native checks, tile-override restart and user-owned
  physical acceptance remain gaps. Existing tentative decisions and the four
  parked focus/send questions are unchanged; no new product question blocks
  the next unit.

## Stage 3b4 implementation plan

- Move frameless/floating-source cross-output migration placement into core;
  remove native `remap_to_output` and membership remap fallback. Core publishes
  an explicit destination frame from normalized source geometry and target
  work area, including asymmetric and negative-origin outputs. Preserve
  same-output follow ordering, cross-output-only pre-arrival tile slots,
  ordinary float membership-only sends and T2 source-state transactions.
- Cover placement and arrival through core-only tests and production Windows
  diff/readback/sequencer tests, then broaden shared convergence-table cases.
  Verify locked core/protocol/Windows build, tests, strict clippy, formatting
  and whitespace; independently review ownership and safety before fixture-only
  both-output verification. Native readbacks, not setter success, prove arrival.
- Stages 4 and 5 follow only from a verified working Windows slice. Existing
  parked questions, tentative decisions and user-owned physical acceptance
  remain unchanged.

## Stage 3b4 accepted outcome (2026-10-23)

### Frameless migration placement and shared convergence

- Core publishes an explicit `effective_frame` for readable ordinary members
  migrating from sessionless/floating workspaces across outputs. It translates
  the observed frame by the FULL-output delta, then constrains its origin to
  the destination work area without resizing. An oversized axis pins to that
  work area's origin. Raw observed output, not retained logical homing, grounds
  the translation; an already-target frame never translates twice.
- These remain membership-actuation targets, not tile allocations or ordinary
  float frame authority. Same-output follow ordering and cross-output-only
  pre-arrival tile slots are unchanged. Intentional floats preserve physical
  frames; sticky, overlay, minimized, unreadable and incomplete lanes acquire
  no guessed placement. Unsupported tiled-to-float transactions are unchanged.
- Deleted Windows `remap_to_output`, its membership fallback and the `remap`
  switch. The adapter writes only core-published frames under its existing
  freshness/lifetime/scope/input gates, then independently observes arrival.
- Shared `test-fixtures/frame-convergence.json` now covers five frame cases
  (learned growth, declared-cap conflict, refused/throwing retry, exact
  reconvergence), retired-lifetime replacement, incomplete/hidden/unreadable
  observations with recovery, and partial migration with late arrival. Core
  and Windows use the same oracle. Windows attempts come from production
  diff/write/readback sequencing, never hand-built verdicts or setter echoes.
  Private learned/pair maps are asserted in core; native reply geometry and
  authority expose their portable effects in Windows. Float-operation and
  full view/refill completion retain their separate production seam suites.
- Independent review caught and corrected differing normalized observation
  ingress hidden behind platform-specific expectations, literal-only table
  assertions, and logical-home-based double translation. A failed-before
  already-target regression proves the latter fix. The unified oracle has no
  platform-specific expected outcome overrides. Focused fixture test modules
  replace growth of the existing large policy/seam modules.

### Verification and limits

- Final locked core/protocol/Windows build, all-target tests and strict
  all-target clippy pass: **2,377 passed, zero failed, one ignored across 48
  suites**. Workspace formatting and whitespace checks pass. Final independent
  boundary/evidence review accepted the corrected source. No dependency,
  unsafe code, input hook or polling loop was added; placement adds bounded
  report-time geometry calculations, with no benchmark or zero-cost claim.
- Final-source fixture-only native checks passed sessionless whole-workspace
  migration into an occupied destination and back across both outputs. A
  near-edge member discriminated constrained placement from blind translation;
  an oversized member proved size preservation and axis-origin pinning. Exact
  native physical frames, lifetime/token joins and foreground corroborated
  both directions; hidden destination peers stayed intact. Later accepted
  snapshots independently omitted each migration and its source-refill id.
- Tiled admission, selection hide/reveal on both outputs and both-direction
  tiled send-follow also passed with independent native readbacks and fresh
  empty-flight snapshots. The sessionless leg's expected placements derive
  from native source frames and topology: trace does not expose its effective
  frame field. An attempted float toggle on a floating workspace was a no-op;
  that sibling stayed ordinary, so intentional-float preservation is offline
  evidence only. A later floating-scope selection retained its separate view
  flight and was not treated as proof that every flight had retired.
- Clamp/refusal/throw, lost observations and partial-arrival predicates remain
  offline assertions; fixture track hints do not enforce programmatic clamps.
  Physical shortcut/drag/game acceptance and sub-sample visible-duration
  claims remain excluded. Harness corrections concerned shell encoding,
  native-versus-client geometry, arithmetic grouping and unsupported probe
  assumptions; none changed product policy or required a production workaround.
- Exact-owner stop, independent restore and fixture shutdown left no owned
  process/window, ledger or pending control. Settings bytes, relevant
  SPI/taskbar state and protected-window geometry matched baseline. Raw
  provenance, scripts, artifact identities and native readbacks remain outside
  the repository.

### Remaining boundary and exact next action

- Stage 3's convergence/completion/frameless placement extraction is complete.
  Windows remains a working dogfood slice, not the completed rearchitecture.
  Stage 4 is next: report gesture lifecycle into core, retain drag session and
  edge-share/release policy there, then publish desired shapes, active focus/
  warp policy and per-screen pager content. Preserve the disabled synthetic
  drag driver and focused-fullscreen-game pause; physical drag stays user-owned.
- Stage 5 still needs the inventory/dead-API/common-connector audit and final
  KDE/macOS port notes. The existing frame/member/view fields suffice for this
  placement slice, but gesture/presentation changes remain ahead, so the core
  interface is not yet settled enough for full ports. Audit floating-scope
  standalone view flight behavior without treating migration retirement as
  its acceptance. MRU occupied refill, shared/global native checks and
  tile-override restart staging remain separate native gaps.
- All four parked focus/send questions and the existing tentative learned-
  minimum, placement and dev-control choices remain unchanged. No new product
  decision or implementation blocker prevents stage 4.

## Stage 4a implementation plan

- Add normalized gesture start (exact window lifetime and move/resize kind),
  cursor, release and cancel ingress to the world route. Retain the gesture,
  resolve its target/preview, and commit release or edge-share changes in core.
  Reuse the existing internal drag and pointer-resize machinery rather than
  introduce a second policy implementation.
- Replace Windows drag targeting, pending-drop and geometry decisions with
  fact reporting and desired-overlay rendering. Preserve native input ownership,
  cancellation detection, lifetime/freshness vetoes, coordinate conversion and
  truthful actuation readback. Preview hide acknowledgement is a native rendering
  fence, not release-outcome policy.
- Acceptance: core lifecycle/target/release/resize/gap/game-pause regressions,
  Windows report-mapping and render-consumption tests, locked core/protocol/
  Windows build/tests/strict clippy, workspace fmt and whitespace, independent
  boundary review, and fixture-only both-output smoke without synthetic drag.
  Physical drag acceptance remains separate; the drag driver stays disabled.
- Focus/warp and border/group/pager presentation follow as separate slices.
  Preserve existing release and edge-share behavior; unsupported new behavior
  stays TBD rather than being inferred from the extraction.

## Stage 4a accepted outcome (2026-10-23)

### Gesture ownership and contract

- `gesture.rs` defines normalized lifecycle facts; `world_gesture.rs` retains
  the sole gesture session, source homing, immutable start truth and hover
  prior. Binding before the sweep shields the native-moving window's logical
  home. Core computes drop targets/rectangles, same-output drag release,
  cross-output send/follow and pointer-resize shares through existing internal
  Engine policy. No adapter-selected target or geometry enters that policy.
- `gestureIngress` carries `id` (nonzero monotonic decimal-u64 string within
  the adapter incarnation), `win`, `kind`, `phase`, `cursor`, `startCursor`,
  `start`, `modal` and `esc`. Ports must retain the same id and frozen
  start facts through coalesced start/end and delayed rendering. Missing cursor
  stays unknown. The new optional `gestureSettled` reply field carries
  `{id, committed}`; it acknowledges incorporation, not native completion.
  Duplicate requests repeat their reply; later unrelated reports carry null.
  Digest, validation, conservative bounds and both encoders cover the fields.
- The consumed-id watermark is bounded scalar state, not a frame-keyed or
  unbounded lifetime table. Refused/unreadable terminal attempts consume their
  id without committing; stale cancel cannot erase a newer session. Missing
  ingress holds, explicit close clears, and fresh ids supersede abandoned
  sessions. Observation gaps never fabricate a valid release or arrival.
- Windows reports native start/update/release/cancel, keeps identity/tag/input
  freshness and render-hide acknowledgement, then consumes the ordinary world
  reply. All finish refusal, Esc, timeout and suspension paths retain a terminal
  cancellation until incorporation. There is no mutating result-query API.
  Diagnostics distinguish core commit from refusal/unverified native outcome.
- Deleted Windows cross-drag destination/settlement, same/cross preview policy,
  pending-drop destination and arrival/follow-focus branches, drag snapshots,
  fingerprint mirrors and obsolete public gesture reexports. Pending rendering
  remains a mechanical hide-before-effects fence, not a second drag owner.
  Removed unused moved preview-gate APIs instead of retaining test-only policy.

### Coverage and corrections

| Superseded coverage | Retained production coverage |
| --- | --- |
| Windows preview gate table and source-bounds helper test | Core world lifecycle zero/outside/cancel/float/sticky/replacement/gap cases and production destination/derivation tests; Windows native identity/tag/input gates remain tested. |
| Windows pending-drop policy assertions | Producer/capture/sequencer/reply tests retain hide-ack, invalidation, timeout, missing observation, lifetime drift and cancellation-delivery cases. Core owns commit assertions. |
| Windows public pure-gesture test imports | Tests import the actual core owner; no Windows compatibility aliases. |
| Cross-drop arrival/focus tail | Existing core retained send/transfer settlement and native raw-attempt/receipt suites remain the arrival oracle; gesture replies alone prove no native arrival. |

- Review reproduced and corrected post-sweep binding duplicating the mover,
  absent-ingress close leakage, overwritten down-point truth, terminal replay
  under frame-based identity, abandoned native cancellations, stale-reply
  success diagnostics and inline down-point removal before capture. Failed-
  before/passing-after regressions cover the concrete lifecycle defects.
  A first read-only review missed remaining adapter policy; the final ownership
  review inspected its deletion and accepted the corrected end-to-end route.
- Core lifecycle coverage includes resize shares, modal move across rescale,
  no-change/cancel/Esc, zero/outside, close/replacement, incomplete recovery,
  consumed commands, terminal replay/supersession, game pause, reply sizing and
  transactional rejection. Windows tests exercise actual report production,
  terminal retention/incorporation and renderer consumption, not setter echoes.

### Verification and native limits

- Locked core/protocol/Windows build, all-target tests and strict all-target
  clippy pass: **2,421 passed, zero failed, one ignored across 48 suites**.
  Workspace formatting and whitespace checks pass. Independent final boundary
  review accepted the reply-based terminal contract; subsequent cleanup only
  removed dead preview APIs, compatibility reexports and unused allocation.
  KDE/macOS remain intentionally unported; required report/reply literal
  additions are not a platform implementation or compilation claim.
- Fixture-only both-output smoke passed admission/coverage, float/unfloat with
  exact core-selected frames and stable peers, selection hide/reveal and
  both-direction send-follow. Exact caller-id/native-lifetime/token joins,
  physical frames, visibility and foreground corroborate the outcomes. Six
  fresh accepted snapshots showed no retained transfer flights. These are
  nondrag runtime checks, not gesture or physical shortcut acceptance.
- Exact-owner stop, independent restore and fixture shutdown left no owned
  processes/windows, ledger or controls. Settings bytes, relevant SPI/taskbar
  state and protected-window geometry matched baseline. Raw scripts, artifact
  provenance and native readbacks remain outside the repository.
- No dependency, unsafe code, input hook or timer was added. Existing coalesced
  native gesture wakes now run core report/preview work; its cost is unmeasured.
  Native suspension and core fullscreen/cursor gates pause overlay/release work.
  Physical drag/game acceptance remains user-owned; the synthetic drag driver
  is unchanged and disabled. No zero-cost or transient-duration claim is made.

### Remaining boundary and exact next action

- Stage 4a is complete at the offline gesture/native nondrag acceptance bar.
  Windows remains a working dogfood slice, not the completed rearchitecture.
  Next: stage 4b active focus-follows-cursor and cursor warp, including i3/sway
  cross-output preset behavior, both default off. Then stage 4c core border/
  group/pager presentation and stage 5 ownership/dead-API/common-connector audit.
- Cross-output pointer-precise preview versus send-based release placement is
  inherited, not repaired or physically accepted here. A new minimal
  [discriminator](../spec/reference-outcomes.md#windows-dual-output-transfer-discriminators)
  records the alternatives and recommends physical discrimination before a
  policy change. Keep current placement; do not claim preview/release parity.
- Core gesture ingress/terminal acknowledgement is ready for port adaptation,
  but the overall interface still awaits focus/warp and desired presentation.
  Audit floating-scope standalone view flight behavior in stage 5. MRU refill,
  shared/global native checks, tile-override restart and physical acceptance
  remain gaps. Existing tentative decisions and all four parked focus/send
  questions are unchanged.

## Stage 4b implementation checkpoint (2026-10-23)

- Stage 4b is implemented but **not accepted**. Stage 4a's acceptance above
  remains its earlier artifact's evidence. Stages 4c and 5 have not started;
  this checkpoint is not the completed rearchitecture or dogfood readiness.
- Core now derives default-off focus-follows-cursor from an optional
  `cursorHit` exact managed lifetime, reported alongside cursor position.
  Native hit testing supplies observation, not focus policy. Core checks
  current logical view, eligibility and holds; it does not guess stacking
  from rectangles. Admission and command focus take precedence, and an
  unchanged hover does not repeatedly steal keyboard focus.
- Core publishes default-off cross-output `warpCursor` coordinates from the
  final focus effect and its effective frame, constrained to the destination
  work area. Windows requires fresh exact-target foreground, lifetime,
  controlled shown view and input/fullscreen checks before applying the point.
  Same-output focus does not warp. The approved i3/sway cross-output behavior
  is expressible through these settings; no all-settings preset is introduced.
- Windows reports genuine motion through a non-consuming `WH_MOUSE_LL` tap,
  installed only for enabled focus-follows-cursor after the suspension check.
  Motion coalesces into normal observation turns without a new timer.
  Suspension removes the tap; failed release retains its handle and retries
  with bounded diagnostics and backoff. Exit has a best-effort release guard.
  Cost is one additional hook-chain link per mouse message while installed
  and one owner turn per motion burst. Game-start detection latency and these
  costs are unmeasured; input before suspension can still traverse the tap.
  Injected-flagged motion is ignored; a native cursor warp can cause a bounded
  self-wake. No zero-cost or physical-input acceptance is claimed.
- Core-only, protocol and production Windows tests cover hit identity and
  absence, current-view filtering, overlap hit truth, focus precedence,
  stationary hover, game/gesture pauses, warp correlation/refusal and hook
  install/release failures. Independent source review accepted these offline
  boundaries after corrections to hidden-window guessing, decoupled warps,
  absent idle-motion wakes and lost failed-unhook handles.
- Locked core/protocol/Windows build, all-target tests and strict all-target
  clippy pass: **2,467 passed, zero failed, one ignored**. Workspace formatting
  and whitespace checks pass. KDE/macOS ports remain intentionally outside
  these gates. No dependency was added.

### Partial native evidence and unresolved attribution

- Fixture-only native checks passed both-output admission, default-off FFC
  negative controls, live setting adoption and tap installation. Cross-output
  warp-off preserved the cursor; warp-on reached the exact constrained tile
  centre in both directions, while same-output focus left it unchanged.
  Selection hide/reveal on both outputs and a borderless-fullscreen coverage/
  return probe passed. Native frames, cursor and foreground plus exact
  caller-id/lifetime/token joins established those bounded results.
- Actual FFC hover-follow remains **unverified**. Positioned-cursor trials
  observed native focus reach the hovered fixture without an attributable
  owner focus-effect row. Later trials captured an accepted observation with
  cursor over the peer and source still focused, but no successful focus
  effect; a later focus change again lacked attribution. Existing trace data
  omits the reported hit, minted effect ids and individual hold reasons, so
  it cannot distinguish absent hit, eligibility suppression, held dispatch
  or an external focus change. In particular, a focus setter can execute
  without immediate foreground proof and therefore without a successful
  effect count; quiet trace alone cannot exclude delayed owner activation.
  The source of those native changes is unknown.
  Do not classify them as product success or operator activity.
- Rejected-focus-probe, dwell and prompt-perturbation approaches did not close
  this attribution gap. Some live scripts also initially wrote a setting at
  the wrong JSON level or incorrectly required foreground to stay unchanged
  after an FFC-enabled move; those harness errors were corrected, not product
  repairs. A suspected rejected-command hover-consumption defect was not
  reproduced: rejected core candidates are not adopted.
- Every live leg stopped the exact owner, independently restored and shut
  down the fixture. Settings bytes, cursor, relevant SPI/taskbar state and
  protected-window geometry matched baseline; owned processes, ledger and
  controls were absent. Raw scripts, provenance and observations remain
  outside the repository. Physical move delivery, shortcuts, drag and games
  remain unclaimed; the synthetic drag driver stays disabled.

### Next boundary

- Resolve attribution before another FFC live attempt: add bounded trace-only
  cursor-hit, minted focus-effect identity and dispatch-hold diagnostics, with
  offline tests proving the diagnostic oracle. Then use a fixture-only
  positioned-cursor observation that cannot introduce a competing focus
  command. No further identical live approach is accepted as new evidence.
- Do not change static-hover retry policy on the present evidence: a first
  held hover was suspected, not established. Preserve current behavior until
  the exact sequence is reproduced; record a discriminator if it reveals a
  genuine product choice rather than an implementation defect.
- No stage 4b durable acceptance decision is promoted. Desired border/group/
  pager presentation, the common acceptance audit, floating-scope standalone
  view-flight investigation and finalized KDE/macOS port contract remain.
  The core interface is not settled for full ports. Existing tentative
  decisions, four parked focus/send questions and native gaps are unchanged.

## Stage 4b accepted outcome (2026-10-23)

- Core-owned exact-hit hover focus and cross-output cursor warp are accepted
  with both settings default off. The implementation boundary described in
  the checkpoint is unchanged; no static-hover retry policy was altered.
  Native hover-follow remains explicitly unverified, not a product success.
- Bounded `world-apply` diagnostics now join reported `cursor_hit` and FFC
  setting, minted focus effect identity/target, native setter disposition,
  immediate foreground proof and individual hold reason. A setter with no
  immediate foreground proof logs as dispatched and unproven, not as silence.
  Held minted effects and changed hit/focus/setting observations also log;
  unchanged idle observations stay quiet. Offline production-seam tests prove
  no-setter, refused/throwing setter, proven focus and truncation distinctions.
- A changed fixture-only diagnostic leg established both-output admission,
  exact native-lifetime/token joins and an FFC-off negative control. With FFC
  armed, positioned synthetic motion produced no changed-hover observation or
  attributed effect during the bounded window. A complete tap-wake -> hover
  -> effect -> native-arrival chain was neither proven nor disproven. Injected
  motion is filtered by design; synthetic positioning is not established as an
  owner wake. No retry loop or production workaround was introduced.
- Read-only SPI queries with verified constants report OS active-window
  tracking disabled. The fixture ignores mouse-move events rather than
  focusing itself. These checks exclude those proposed confounds at the
  observed settings, but do not establish the missing hover wake or attribute
  earlier unexplained focus changes. No OS tracking setting was changed.
- Final-artifact cross-output warp reached the exact requested tile centre;
  same-output focus left the pointer unchanged. Immediate focus proof was
  initially absent on one row, but the independent foreground gate passed
  before warp and a later report corroborated focus. The earlier artifact's
  both-direction warp, warp-off and both-output selection checks remain
  bounded prior evidence; the diagnostic-only final artifact reran one crossing,
  same-output no-warp and selection smoke, not native FFC success.
- Final locked core/protocol/Windows build, all-target tests and strict
  all-target clippy pass: **2,478 passed, zero failed, one ignored**. Workspace
  formatting and whitespace checks pass. Independent boundary/safety/evidence
  review found no blocking issue and accepted the default-off residual.
  KDE/macOS remain intentionally unported. No dependency was added.
- Exact-owner stop, independent restore and fixture shutdown left no owned
  processes/windows, ledger or controls. Settings bytes, cursor, relevant
  SPI/taskbar state and protected-window geometry matched baseline. Raw source,
  artifact, scripts, gate logs and native observations remain outside the
  repository. Physical motion/shortcuts/drag/games remain user-owned; the
  synthetic drag driver stays disabled. Mouse-tap cost and suspension latency
  remain unmeasured, including input traversing the tap before suspension.
- Next: stage 4c desired border/group/drop presentation and per-screen pager
  content, then stage 5 ownership/dead-API/common-connector audit and finalized
  port notes. The overall core interface is not yet settled for full ports.
  Floating-scope standalone view flight and inherited cross-output preview/
  send-release asymmetry remain audit targets. Existing native gaps, tentative
  choices and four parked focus/send questions are unchanged.

## Stage 4c implementation plan (2026-10-23)

- Move border eligibility and complete border/group expansion into shared
  presentation types. Fresh scoped foreground facts can describe ordinary
  floats and unmanaged foregrounds without admitting them. Host-scaled style
  dimensions and frame coordinates use the same adapter units; Windows keeps
  DPI conversion, styling, lifetime/scope vetoes and z-order anchoring.
- Use the sole retained world's read-only group projection for fast per-wake
  presentation, without a second Engine or a presentation-triggered world
  command. Preserve genuine foreground matching, pending/hold fences and the
  current movement-or-chord trigger. Drop geometry already comes from the
  stage-4a core gesture result; retain its lifecycle dispatch and audit its
  remaining consumer rather than replacing it with a second preview resolver.
- Core owns pager workspace content, labels, current/occupied/focused facts,
  window lists, native-observation stacking retention and frame-source
  precedence. Native enumeration and saved-frame caches supply observations,
  not an adapter content-order policy. Windows retains strip/cell placement,
  miniature coordinate scaling, artwork acquisition and click-to-command.
- Implement border/group extraction, then pager extraction. Accept only after
  core-only regressions, core/protocol/Windows local gates, independent review
  and both-output fixture border/underlay/pager readback. Physical drag/input
  and the existing default-off FFC residual remain distinct acceptance gaps.

## Stage 4c accepted outcome (2026-10-23)

- Core `visual.rs` owns border eligibility/reasons and checked complete
  border/group expansion. Native-scoped ordinary floats and unmanaged
  foregrounds remain border-eligible without acquiring membership. Dimensions
  are host-scaled before the core call; coordinates stay in adapter units.
- `WorldEngine::group_highlight` reads retained topology, hints, membership,
  tiling mode, focus and holds without adoption, revision/history changes or
  actuation. Windows no longer dispatches an `ActiveGroup` command per wake.
  The movement-or-chord trigger and projected source group during an open
  native move are preserved. Native scope/proof/lifetime and renderer-side
  freshness/fullscreen vetoes remain; drop shapes still use stage 4a's single
  core gesture lifecycle, not a second preview resolver.
- Core owns pager rings, labels, effective current, occupied and
  contains-focused facts, per-workspace paint lists and source rectangles.
  Native top-to-bottom observations update core-retained stacking; hidden and
  minimized members keep prior relative order. Frame precedence remains live,
  then float snapshot, then tile/hide snapshot. Fresh candidate frames survive
  same-report admission but remain invisible until membership exists. Windows
  performs only binding, strip/cell placement, miniature scaling, artwork and
  drawing. Core ordinals reach native paint and click attribution verbatim;
  ordinal-only changes invalidate the paint cache.
- This is an in-process presentation observation/query contract on the sole
  retained world, not a new `WorldReport`/`WorldReply` wire section. KDE/macOS
  port transport and remaining dead query APIs are stage-5 audit work; no port
  compilation or final interface-freeze claim is made here.

### Verification and native evidence

- Core-only coverage includes visibility/overflow matrices, unmanaged border
  facts, group source geometry during open moves, mismatched focus and
  transfers, read-only queries, pager order/fallback/hidden lifetime retention,
  same-admission frames and no-admission observations. Windows regressions
  distinguish raw display identities from core wire ids and verify the
  complete ordinal-to-native-frame/click path.
- Review corrections removed an unintended blanket retained-gesture hide,
  fixed core-wire to native-output binding, retained first-admission frames
  and removed native positional ordinal reconstruction. The gesture defect
  was reproduced before correction. Independent boundary review inspected
  the broad extraction; subsequent corrections have targeted regressions and
  final native readback. One unrelated parallel taskbar-queue test failed
  once during development, then passed isolated and repeated full runs;
  interference is suspected, not established or repaired in this slice.
- Final locked core/protocol/Windows build, all-target tests and strict
  all-target clippy pass: **2,491 passed, zero failed, one ignored across 48
  suites**. Workspace formatting and whitespace checks pass. No dependency,
  hook, timer or unsafe block was added by stage 4c.
- Final-artifact fixture-only native checks passed both-output admission and
  exact border bounds against fresh physical visible frames plus scaled style;
  ordinary-float border retention, maximize/fullscreen hiding and return also
  passed. Bounded synthetic chord observations showed exact group-union plus
  extension underlays on both outputs, independently enumerated and measured;
  release hid each underlay. This is not physical chord or drag acceptance.
- Native pager overlays were visible on both outputs with ordered numeric
  cells, current highlighting and light-gray miniature regions. Per-output
  select-away/previous restored native visibility/layout without disturbing
  the peer output. Hidden-view screenshots retained the occupied workspace's
  miniature regions while highlighting the empty view. These screenshots do
  not independently prove icon artwork or every window's miniature/order;
  those content assertions have deterministic core/native-seam coverage.
- Exact-owner stop, independent restore and fixture shutdown left no owned
  processes/windows, ledger or controls. Settings bytes, cursor, relevant
  SPI/taskbar state and protected-window geometry matched preimages. Raw
  provenance, scripts, screenshots, logs and observations remain outside the
  repository. Default-off FFC/warp were not changed. No measured-cost or
  physical game/input claim is added.

### Remaining completion boundary

- Stage 4c is complete; stage 5 has not started. The current Windows slice is
  working, but the whole rearchitecture is not yet declared dogfood-ready.
- Next: audit every inventory row and dead API, common acceptance connector,
  floating-scope standalone view-flight completion and cross-output
  preview/send-release asymmetry; finalize KDE/macOS port notes, then run the
  broad both-output fixture regression and publish the dogfood-readiness
  summary. Keep current placement while the physical preview discriminator
  remains unresolved, rather than deciding a parked product question.
- Native MRU occupied refill, shared/global modes, tile-override restart and
  physical shortcut/drag/game checks remain gaps. Native FFC hover-follow is
  still unverified and default off; mouse-tap cost/suspension latency remain
  unmeasured. Existing tentative learned-minimum acquisition, three-way
  placement/launch-targeting and development controls, plus the four parked
  focus/send questions, are unchanged.

## Stage 5 completion-audit checkpoint (2026-10-23)

### Ownership audit and remaining units

- The inventory above is the pre-extraction map, not a list of current
  Windows owners. Source inspection confirms one retained `WorldEngine`;
  Windows observations, native bindings/recovery claims and renderer caches
  are not additional workspace owners. The following is an interim audit,
  not interface-freeze or whole-stage acceptance.

| Inventory row | Current policy owner | Remaining completion check |
| --- | --- | --- |
| Workspace model | Core logical mapping, membership, rings and views | Dead adapter APIs and all-mode native regression |
| Scope and lifecycle | Core mode, history, trailing and hotplug lifecycle | Shared/global native regression |
| Workspace commands | Core selection, send, migration and refill | Floating-flight correction below; remaining adapter context/fence audit |
| Fresh placement | Core startup/fresh distinction, output and neighbor | Retained adapter output-context consumers need disposition |
| Output resolution | Core topology and action target selection | Audit adapter rectangle grounding and directional fences for policy duplication |
| Classification | Core normalized eligibility and retained admission | Final disposition of remaining float/admission helpers |
| Verification and fighting | Core raw-attempt settlement, suppression, bounds and completion | Shared acceptance connector breadth |
| Gesture | Core retained lifecycle, preview, release and resize | Physical cross-output preview/release discriminator remains unresolved |
| Visual shapes | Core border/group/drop eligibility and geometry | Out-of-process presentation transport contract |
| Pager | Core content, retained order and source-frame precedence | Out-of-process observation/query transport contract |
| Cursor | Core targeting, hover and warp policy | Native hover wake and cost gaps unchanged |
| Behavior settings | Core behavior values; platform persistence/normalization | Final port exporter notes |

- Remaining bounded units: finish portable-policy/dead-API disposition;
  complete the shared world-action case/Windows connector and case-authoring
  instructions; settle the transport-facing presentation interface and port
  notes; finish taskbar-queue isolation investigation; run the full both-output
  regression matrix and publish dogfood readiness. Do not substitute the
  floating-flight leg below for that broad acceptance pass.
- Confirmed cleanup candidates still need consumer inspection and removal:
  `WorldReply::presentation_bytes` is a zero-valued reserve, not a serialized
  presentation section; `product_hide::batch` appears superseded by the
  retained world/native seam. Remaining command decoders and test-only
  helpers likewise need actual consumer checks before deletion.
- The in-process stage-4c presentation API is sufficient for Windows, not a
  settled KDE D-Bus contract. The remote owner must receive presentation
  observations and expose the retained pager/group projection without a
  second Engine or adapter retention/content policy. Current wire replies
  lack that projection. Finalize that interface before full KDE/macOS ports;
  do not infer transport sufficiency from Windows direct queries or preserve
  the old `ActiveGroup` command route as a compatibility shim.
- Taskbar-queue interference remains a hypothesis: several pager tests inject
  and drain one global queue. Reproduce the interference and isolate the test
  seam rather than weakening assertions or treating repeated green runs as a
  root-cause fix. Also inspect `directional_resolution::output_for_rect`:
  its overlap fallback currently subtracts from the rectangle's left/top
  instead of its right/bottom, producing zero intersections when the centre
  is outside all outputs. Keep native observed affinity distinct from core
  action targeting while resolving this helper's owner and coverage.

### Floating view and migration completion correction

- Reproduced a connector defect: a standalone selection into a floating
  workspace published an `Untouched` scope, but Windows required `Controlled`
  to switch its view. No setter, raw view attempt or observed echo could
  arrive, so the core correctly retained the flight indefinitely. The same
  gate defect affected floating migration targets and deferred source refills.
- Windows now permits transfer-coupled membership/view actuation into an
  `Untouched` target only with an unheld output and no gesture barrier.
  Unknown/held scopes and outputs remain closed. Native freshness, input,
  lifetime, scope, at-most-once and target-before-refill proof gates remain;
  raw member/view attempts still go to core for every verdict. Sweep frame
  authority is unchanged. Core-owned sessionless migration placements remain
  distinct from intentional-float frames, which are preserved.
- Core migration now rekeys its floating-scope marker with the workspace on
  the commit reply, matching its existing tiling-mode/id move and swap path.
  This removes a transient false `Controlled` target and keeps inverse
  migration sessionless. Selection, migration/refill policy and the parked
  focus/send choices are unchanged.
- Independent review also found a fallible sticky-stay derivation after swap
  commit. Both stays are now prepared before mapping/history/session mutation.
  The refusal regression deliberately injects a multi-domain internal session
  to reach the failure; ordinary public world reports maintain single-domain
  sessions, so this is defensive atomicity coverage, not a reproduced native
  user failure. Successful sticky/sessionless swaps remain covered.
- Core regressions cover numbered/previous/trailing floating selection,
  truthful refusal settlement, floating migration/inverse and floating swap.
  Windows regressions cover floating target/refill dispatch, holds and
  non-replay, plus a nonempty production-native-seam hide/reveal, receipt and
  echo-settlement roundtrip. No core completion shortcut or desired-state
  echo was added.

### Floating-flight accepted evidence

- Final locked core/protocol/Windows build, all-target tests and strict
  all-target clippy pass: **2,503 passed, zero failed, one ignored across 48
  suites**, plus **119 fixture tests** (2,622 total across 49 suites).
  Workspace formatting and whitespace checks pass. Independent source review
  accepted the floating authority, target/refill ordering, refusal rollback
  and corrected defensive swap atomicity. No dependency, hook, timer or unsafe
  block was added by this unit.
- Final-artifact fixture-only native verification passed nonempty floating
  trailing/previous hide/reveal on both outputs, stable frames/peer visibility
  and explicit operation retirement from fresh whole snapshots. Floating
  workspace migration and inverse passed core FULL-delta/constrained placement,
  intentional-float frame preservation, target/refill visibility and chained
  flight retirement. Both-output tiled selection and both-direction send-follow
  also passed as bounded regression checks, not the full stage-5 matrix.
- An earlier external harness initially mistook a pre-dispatch empty transfer
  snapshot for command acceptance; the raw trace showed the later accepted
  migration/refill pair. The corrected harness joins the accepted operations
  before checking their retirement. No production workaround was added.
  A new native-seam test also initially reused another test's storage identity;
  giving it a unique identity removed that reproduced test-isolation failure.
  Neither issue establishes the separate taskbar-queue race's cause.
- Exact-owner stop, independent restore and fixture shutdown left no owned
  process, ledger or control residue. Settings bytes, relevant SPI/taskbar state
  and protected-window geometry matched preimages. Cursor restoration was
  deliberately not attempted when the ownership guard lacked a fixture-owned
  foreground; the pointer was left parked, not falsely reported as restored.
  Raw scripts, identities, artifacts, logs and observations remain outside the
  repository. Numbered floating selection is covered offline, not by this live
  leg. No physical input, game or native FFC acceptance is claimed.
- This floating-flight unit is accepted. Stage 5 overall, interface freeze and
  final dogfood readiness remain incomplete. Next audit unit: disposition of
  adapter output context/rectangle grounding and directional fences, then dead
  APIs and taskbar test isolation; continue the connector/transport/full-matrix
  units above. Preserve the preview/send-release asymmetry pending the existing
  physical discriminator. Native MRU occupied refill, shared/global modes,
  tile-override restart and physical input/game gaps remain; default-off native
  FFC and unmeasured hook cost/suspension latency are unchanged.

## Stage 5 output-grounding and dead-fence audit (2026-10-23)

- Reproduced the off-screen rectangle defect: with two adjacent outputs, a
  frame whose centre is beyond every output but whose tail overlaps the
  non-first output incorrectly grounded to the first. The retired formula
  intersected left/top rather than right/bottom and always scored zero area.
- Core `output_selection::output_for_rect` now owns observed rectangle
  grounding: integer-rounded centre with half-open containment first, then
  largest intersection area, then first topology entry on ties (including
  all-zero intersections). Arithmetic uses widened coordinates. Windows only
  maps native device identities and supplies its unknown-device sentinel when
  topology is empty. Point grounding uses existing core `output_at_cursor`.
  Grounding reports observed affinity; it does not override logical membership,
  command targets or native lifetime/freshness/scope vetoes.
- Consumer inspection covered admission/startup/restart affinity, observation,
  native member/view/arrival proof, fullscreen anchoring and foreground context.
  Deleted unused directional target/point wrappers, parallel directional/send
  fence snapshots and predicates, and the test-only `SendPreflight` route and
  tests. Live `SendTarget`, fresh native pre-effect guards and core settlement
  remain. No parked targeting, float-home or preview/release choice changed.

### Verification and limits

- Independent boundary review accepted geometry, numeric safety, native binding
  and zero-production-caller deletions. Core/native-shell regressions cover the
  original off-screen sliver, negative origins, odd extents/half-open edges,
  centre priority over larger overlap, zero-area ties, empty topology and extreme
  coordinates. The overlap discriminator is deterministic only; no native
  off-screen frame was staged for this unit.
- Locked core/protocol/Windows/fixture build, all-target tests and strict
  all-target clippy pass: **2,503 passed, zero failed, one ignored across 48
  suites**, plus **119 fixture tests**. Workspace format and whitespace pass.
  No dependency, unsafe block, hook or timer was added.
- Final-artifact fixture-only native regression passed admission/coverage on
  both outputs, local directional focus/move on one, cross-output send-follow
  and inverse send-stay, and trailing/previous hide/reveal on the other. Fresh
  caller-id/lifetime/token joins, PMv2 physical frames, visibility, foreground,
  accepted command traces and fresh operation-drain snapshots established these
  bounded results. This is not the full stage-5 both-output matrix.
- Exact-owner stop, independent restore and fixture shutdown left no owned
  processes, ledger or controls. Settings bytes, cursor, relevant SPI/taskbar
  state and protected-window geometry matched preimages. A shell-encoding
  mismatch failed before live effects and was corrected in the external driver
  environment, not production. Raw evidence remains outside the repository.
  FFC/warp stayed off; no physical input/game or measured-cost claim is added.

### Remaining live adapter policy

- The earlier ownership table is still interim. Moving stateless geometry and
  removing dead fences does not finish the action-policy cutover:
  `chord_output`/`output_context_keys` still select foreground, then pointer,
  cache and output order for migration/control context. This differs from the
  configured core workspace-switch selector; preserve current semantics while
  moving the remaining action context into the world contract.
- `keyboard_directional.rs` still chooses automatic-float and floating-workspace
  refusal, fullscreen-move refusal, maximized-focus fencing and the
  unmaximize-before-move continuation. Some predicates are core helpers, but the
  adapter still chooses action sequencing. These need core-owned decisions and
  normalized observed continuation, retaining native exact-subject, fresh
  foreground/view, input and fullscreen vetoes. Do not call them merely
  mechanical or delete them without replacement and regression coverage.
- Remaining units: that live policy cutover; dead API/product-hide disposition;
  taskbar-queue root-cause/isolation; shared world-action core/native/live
  connector; wire presentation consumed uniformly by Windows; finalized
  KDE/macOS port notes; full both-output matrix and dogfood-readiness summary.
  Stage 5, the portable interface and dogfood readiness remain incomplete.
  Existing tentative decisions, parked questions and native acceptance gaps
  remain unchanged.

## Stage 5 taskbar-test isolation outcome (2026-10-23)

- Deterministically reproduced shared-queue test interference: eight concurrent
  producers staged five taskbar events each before a barrier; one drain took all
  forty while other drains took none. The existing parallel pager tests injected
  and drained that same process-global queue. This proves a test-isolation defect,
  not retrospective attribution of the earlier single transient failure.
- Test builds now give each thread its own queue storage through the existing
  accessor. Real taskbar trampoline, bounded queue push/take, hook pending probe,
  drain/filter and pager assertions stay on the production path. Ordinary builds
  retain the original process-global queues. Main-hook test storage is isolated
  the same way; no serialization, sleeps or reduced assertions were introduced.
- Parallel verification separately reproduced a pager-fixture ledger directory
  collision (`LockContended`): process id plus wall-clock timestamp was not unique
  within a clock tick. An atomic suffix makes each rig distinct, matching the
  existing gate-log fixture convention. The two reproduced mechanisms are fixed;
  which caused the earlier transient remains unknown.
- Barrier regressions prove independent taskbar and main-hook test storage. The
  taskbar callback-route regression also checks all event fields, order, cap,
  overflow coalescing and clean second take through the real trampoline. Review
  replaced an initial separate-injection-stash draft with storage-only isolation
  to retain callback, cap and pending-probe parity. No alternate oracle remains.
- Independent review accepted the final source and production/test boundary.
  Locked core/protocol/Windows/fixture build, all-target tests and strict
  all-target clippy pass: **2,506 passed, zero failed, one ignored across 48
  suites**, plus **119 fixture tests** (**2,625 total across 49 suites**).
  Concurrent taskbar tests pass **14/14** and tiling-system tests **402/402**;
  workspace formatting and whitespace checks pass. No dependency, new unsafe
  block, native hook or timer was added.
- Final-artifact fixture-only no-behavior-regression checks again passed the
  bounded preceding unit's bar: admission/coverage on both outputs, directional
  focus/move on one, send-follow/inverse send-stay across outputs, and trailing/
  previous on the other, with fresh native frames/visibility/foreground and
  operation retirement. This does not prove native taskbar callback delivery or
  replace the full stage-5 matrix. Exact stop/independent restore and fixture
  shutdown left no owned process/ledger/control residue; settings bytes, cursor,
  relevant SPI/taskbar state and protected geometry matched preimages. FFC/warp
  remained off; no physical/game/input or measured-cost claim is added.

### Next action-policy boundary

- Source audit extends the preceding directional-policy finding: Windows also
  reroutes sticky subjects to the displayed workspace, builds floating-focus
  candidates and selects the target, computes constrained float half-snaps, and
  directly writes/focuses floats without dispatching a world command.
  See `keyboard_directional.rs` (sticky routing and refusal lattice, maximize
  clear/continue, floating focus/half-snap, tiled world dispatch),
  `tiling/float.rs` (selection/geometry/clear predicates), and `admission.rs`
  (`float_focus_candidates`). These are live portable policies, not dead APIs.
- Next: design and implement one world-owned directional action route covering
  these branches and observed maximize-clear continuation; remove the superseded
  Windows decisions. Preserve refusal atomicity, current outcome vocabulary,
  sticky/current-view behavior and all parked targeting choices. Keep native
  exact-subject/input/freshness vetoes, coordinate conversion, writes and truthful
  arrival proof. Merely adding four core guard helpers while the float branch
  still bypasses world dispatch does not complete this boundary.
- Then finish migration/control output-context ownership, remaining dead APIs,
  common case/connector acceptance, uniform wire presentation, KDE/macOS port
  notes and full both-output regression/dogfood readiness. No interface freeze
  or whole-stage acceptance is claimed; prior tentative decisions and native
  gaps are unchanged.

## Stage 5 directional draft - not accepted (2026-10-23)

- The current draft routes float and tiled directional intents through world
  commands and moves sticky-view routing, ordered refusals, float candidate
  selection and constrained half-snap geometry into `world_directional.rs` and
  `world_operations.rs`. Float half-snaps now publish retained `frame_operation`
  placements through the common native lane. Duplicate directional focus
  actuation was removed; the common spent effect is the sole focus writer.
  These are implementation progress, not acceptance of this draft.
- The initial stateless clear-result/bespoke float-write design failed the
  ownership review. Its correction added retained float placement and
  `PendingDirectionalClear`, but still does not provide an observation-driven
  clear operation. Both designs remain unaccepted; passing tests do not settle
  the ownership or behavior defects below.

### Blocking defects and missing discriminators

- `world_operations.rs::move_directional` consumes a pending clear only when
  another same-window directional command arrives. Ordinary reports do not
  advance or cancel it, and the held-scope return precedes pending consumption.
  Native veto/observation-failure returns in `keyboard_directional.rs` leave the
  binding pending. The adapter then issues a second command after its native
  clear rather than reporting a correlated clear attempt for the original
  operation. The stored originating command sequence is not used to identify
  continuation. A later genuine intent can therefore consume an abandoned
  binding; this source-grounded defect lacks an executable regression yet.
- The correction introduced `MoveFloatRefusedSticky` instead of preserving
  existing sticky half-snaps because the common float setter gate rejects
  sticky windows. This is an unintended behavior change, not an unresolved
  product choice. Restore explicit directional sticky placement through the
  common operation contract while retaining protection against ordinary
  unsolicited sticky frame writes. Do not accept the new refusal or its tests
  as the behavioral baseline.
- The Windows `MoveFloatApplied` branch logs `move-float-applied` from the
  presence of a requested rectangle, without proving native frame arrival or
  inspecting refusal. A retained desire is not completion. Cover held/refused
  writes and report truthful pending/refused/applied outcomes.
- Add real continuation sequences: commandless reports, native veto/refused or
  ambiguous clear, unchanged maximized observation, held scope, changed focus,
  view/domain/lifetime, replay and a later independent intent. Existing tests
  exercise a second command, not the required retained observation lifecycle.
- Add production-projection coverage for retained maximized float candidates
  and preserve original observed-then-retained encounter/tie order. The new
  helper accepts staged maximized carriers, but that alone does not prove the
  production capture route or candidate ordering. No confirmed production
  candidate drop is claimed from the helper test.
- Source comparison confirms the old half-snap output was membership-home
  (with sticky displayed-view routing), not physical-frame output. Preserve
  that parked choice. New screen work-area/gap inputs must be proven equivalent
  to the former native area/inner-gap inputs.

### Verification boundary and next action

- Current locked core/protocol/Windows/fixture all-target build, tests and strict
  clippy pass: **2,644 passed, zero failed, one ignored across 49 suites**.
  Workspace formatting and whitespace checks pass. Full raw gate outputs and
  draft snapshots remain outside the repository. These gates do not accept the
  known defects or replace live verification. No live test was run for this
  draft, and no final-artifact directional acceptance is claimed.
- Next: replace the second-command clear bridge with a correlated common
  clear desire/raw-attempt contract and retained original-intent continuation
  driven by normalized observation. Cancel/retire on narrow refusal, lost
  authority or supersession; never attach an unrelated command to old clear
  state. Preserve native exact-subject/input/freshness/fullscreen vetoes. Then
  restore explicit sticky half-snaps, correct completion diagnostics, add the
  missing regressions, rerun gates and both-output fixture acceptance, and
  review the finished boundary before accepting this unit.
- Migration/control context, dead APIs, the common acceptance connector,
  uniform wire presentation, KDE/macOS port notes, full both-output matrix and
  dogfood-readiness summary remain after this unit. The portable interface is
  not settled and the working draft is not ready for dogfooding. No new durable
  product decision or change to the existing parked questions is accepted.

## Stage 5 directional lifecycle checkpoint - native acceptance pending (2026-10-23)

- The replacement removes the second-command clear bridge. Core publishes
  `WindowDesire.clear_operation` under the original operation identity;
  Windows' common diff lane spends it before one nonactivating native clear
  and returns raw `ClearAttempt` evidence. Acknowledgement never proves arrival.
- `RetainedClear` stages correlated evidence before observation gaps. Ordinary
  commandless reports resume the original tiled move or retained float placement
  only after an accepted clear attempt and readable normalized unmaximized
  facts. External unmaximize without correlated evidence does not continue.
  Close, home/view/focus changes and superseding intents abandon old bindings;
  ambiguous attempts retain no-retry state. No new command identity is minted.
- Explicit sticky half-snaps again use the common retained frame-operation
  lane; unsolicited sticky frame writes remain blocked. The Windows handler
  logs float dispatch rather than claiming frame arrival from a requested
  rectangle. Production capture/order and lifecycle regressions were added.
- Design and independent source reviews inspected original-identity correlation,
  ordinary-observation continuation, tiled/float routing, supersession, sticky
  authorization and diagnostic honesty. Final locked all-target build/tests and
  strict clippy for core/protocol/Windows/fixture pass: **2,644 passed, zero
  failed, one ignored across 49 suites**. Workspace fmt and whitespace pass.
- This is a checkpoint, not directional or whole-stage acceptance. No native
  run covers this artifact yet. Remaining risks include a shared native
  suspension/foreground race between dispatch and effects, ambiguous clear
  evidence retaining a bounded no-retry hold, and continuation refusal after a
  proven clear leaving the window unmaximized without a terminal result.
  Held-scope continuation and same-report supersession ordering need final
  acceptance scrutiny, not an assumption that broad passing gates prove them.
- Next: fixture-only both-output directional tile/float focus and move,
  maximize-clear continuation and sticky half-snaps with independent frame,
  foreground and operation-retirement proof. Correct any concrete lifecycle or
  diagnostic findings before accepting this unit. Then finish the remaining
  stage-5 ownership, connector, presentation transport, port and readiness work
  listed above. The core interface is not frozen; dogfooding is not ready.
