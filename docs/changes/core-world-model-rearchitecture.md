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
