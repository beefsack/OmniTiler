# KDE world-contract v2 port

## Goal and current outcome

- Make KDE capture/observe/actuate/render over the sole shared `WorldEngine`;
  Windows is the migrated reference. No v1 compatibility, policy copies or
  second owner. [Principles](../principles.md#layering) and the
  [adapter guide](../porting/world-adapter-guide.md) are authoritative.
- Stage 0 complete against `a0f54105`; stage 1 implements capability-gated
  host-owned adoption in core/protocol. KDE remains unported and native
  acceptance remains TBD. This record stays active for the whole port;
  stage 2 has a committed partial paired implementation, not accepted as a
  completed stage; stages 3 onward remain outstanding except for the
  observation/actuation vocabulary required by the atomic wire cutover.
- Initial inventory confirmed the `01a73798` KDE gaps: JS pins version 1
  (`world-seq.ts`), `world-report.ts` emits the old report vocabulary,
  `world_service.rs::parse_discovery_request` requires v1, and
  `WorldBackend::fresh` references the removed
  `aligned_per_output_workspaces` config field. The effect's `updateBorder`
  still derives eligibility and bounds. Stage 1 replaces the previously inert
  core native observations with adoption and binding publication.
- Native API checks use the sibling KWin clone at the notes' `v6.7.4` pin,
  never fetched source. The declaration header separately cites 6.7.5;
  pinned comparison confirms identical relied-on scripting APIs between the
  tags. Keep the documented 6.7.4 pin; declarations are not the API ceiling.

## Stages, outcomes and evidence

| Stage | Outcome and deletion boundary | Required evidence |
| --- | --- | --- |
| 0. Inventory and plan | This plan, current-tree discrepancies, explicit parked contracts | Source inspection and unchanged core baseline; complete |
| 1. Host-owned adoption | Capability-gated core native order/bindings and native member intake; preserve startup inference and adopt later outside moves/creations; no KDE policy workaround | Core-only delivery: four bound workspaces plus ordinary spare, no launch moves/effects, stable bindings, exact startup geometry, outside move/create and uncertainty regressions; native result TBD |
| 1b. Native-only evidence and backing correlation | Uncommitted core/protocol draft; optional native current, native-only host receipts, creation desires/attempts, native order/removal and spanning regressions | Blocked by truthful asynchronous creation/outside-creation indistinguishability; 21 draft tests pass and one new blocking regression fails; not a delivered contract |
| 2. Broker and strict v2 cutover | Parked: paired Rust/JS implementation and canonical v2 fixtures exist, but command wake delivery and native backing/settlement remain unresolved | Partial offline gates below; six KDE integration failures remain, no native or completion claim |
| 3. Observe and actuate | v2 facts/receipts, presentation ingress and operation tokens; preserve native identity/recovery and last-moment vetoes; delete old feedback/verdict and native-derived homing routes | Fresh post-dispatch facts, truthful attempts, token spending, generation/watermark refusal, transfer/clear/float settlement and no fabricated arrival |
| 4. Shared policy only | Broker capture envelope carries raw native role/control/ownership/app/hint facts to existing Rust core entry points; delete TS admission, born-maximized, fixed-size, refill, scope, drag/gesture, same-axis move and tiling-toggle policy | Core-policy normalization and real capture-envelope tests; shortcut/gesture inputs never preselect verdicts; no shared-ingress redesign implied |
| 5. Pure presentation and coordinates | Effect renders core border/underlay/drop/pager bounds; delete effect geometry/eligibility derivation and obsolete policy FFI routes; keep native anchoring and immediate hide-only safety vetoes | Rendering/token/freshness/game-pause tests and host-matched native build; one KWin-logical coordinate space with fractional rounding and style-unit conversion proof; add window clientGeometry reads for hint conversion (not workspace clientArea), never Windows border compensation |
| 6. Host integration | Cursor from workspace.cursorPos plus workspace.windowAt, no polling; warp unsupported, never reported applied; independent active-border and pager-focused-window colors, default system accent, full-strength border/dimmer fill | Declaration/source checks and adapter tests; independent overrides; native styling remains host-owned |
| 7. Workspace modes and investigations | Keep per-output-local default, global-unique separate, shared linked; equal_workspace_count is a host limit; required separate-mode warning when KWin per-output desktops are off, settings action if feasible | All-mode offline/native cases; labels "Displays have separate workspaces" / "Workspaces span displays"; Activities/multi-desktop and configure investigations below, no speculative core changes |
| 8. Native acceptance and ownership audit | Fixture-first integrated journeys and deletion audit; no logical-receipt-only success claims | Independent native hide/reveal checkpoints, frame/visibility/foreground joins, both-output cases, exact restoration, no stale v1 or policy route |

## Stage 1 implementation boundary

- `WindowFact.native_workspace` (optional wire `nativeWs`) carries observed
  opaque native membership, distinct from logical `observed_workspace`.
  Non-null evidence without the capability refuses. Validation, digest and
  conservative wire accounting cover it; no version-1 route is added.
- `world_native.rs` maps `(output, native id)` to core logical ids and derives
  observed logical member/view receipts. Only actually observed per-output
  pairs publish `native_binding`; migration or shared mirroring cannot invent
  native backing. Generation/incarnation reset clears the map.
- Complete native lists use existing screen/inventory completeness flags.
  Missing lists never prove deletion; native-bound empties retain identity.
  Ordinary trailing and equal-count policy is unchanged. Outside creations
  bind at native-order positions; outside moves use existing homing/settlement.
- Native uncertainty retains known homing with existing `Unreadable` write
  fences; unhomed newcomers await evidence without a membership write. Fresh
  minimized members use native membership, never the current desktop. Native-free
  Windows uses `native_workspace: None` with no behavior change.
- Tests use UUID/numeric native ids, asymmetric desktop ordering and reversed
  window input order. Exact 300/500 startup allocations prove existing position
  and size inference, alongside minimized/floating/fullscreen membership,
  reset/repeat, separate/linked lists, outside move/create, uncertainty,
  pending transfer, truthful migrated bindings and equal-count cases.

## Stage 2 implementation boundary

- Replace the service's report passthrough with one broker-owned engine,
  identity, ordering namespace and monotonic clock. Capture transports projected
  v2 facts for now; raw classification capture remains stage 4.
- Separate the exclusive KWin capture/actuator route from same-user command and
  presentation routes. Bind command deduplication and terminal collection to the
  authenticated sender, never a caller-supplied client id alone. Native work
  travels only on capture replies; terminal outcomes travel only on the issuing
  sender's collection route. Presentation reconnect never replays either lane.
- Capture envelopes bind the discovered generation/incarnation, a fresh source
  correlation and the native invalidation watermark. Exact redelivery replays
  without draining newly queued work; changed source reuse and stale watermarks
  refuse. The JS callback also fences invalidation during flight.
- Bounded units: Rust broker and authorization; paired JS wire/entry and minimum
  v2 observation/actuation carriers; independent boundary review; complete KDE
  offline gates and documentation reconciliation. No core changes are planned.
- Core command rejections are explicit terminal failures for their requester,
  not fabricated null successes or arbitrary retries; the same fresh facts then
  enter the observation-only lane. Malformed capture never retires a command.
- Minimum stage 3 work pulled into the draft: projected v2 facts and native
  membership evidence, desire parsing, clear/frame tokens and output-specific
  logical-to-native target resolution. Raw frame/member/view/clear attempts are
  still absent; local legacy attempt bookkeeping is not settlement evidence.
- Canonical reply fixtures are generated by
  `crates/omnitiler/examples/world_wire_fixtures.rs` through `WorldService`.
  Request byte gates exercise the JS producer. The TS real-engine transport
  harness is entry-only, not a second broker oracle; Rust tests own broker
  sequencing, authorization, deduplication and replay evidence.

## Tentative technical decisions

- Sequence the atomic service/JS v2 wire cutover before deleting capture-policy
  consumers, then renderer/host integration and native acceptance. Each stage
  removes its superseded route before acceptance; this is not dual-route support.
- Use a KDE broker capture envelope feeding existing shared Rust predicates,
  not a new shared classification ingress. This implements the guide's remote
  transport boundary without changing core policy or deciding parked questions.
- P1 count clarification (tentative, later review): four occupied native
  desktops map to four native-bound logical workspaces in native order with
  membership preserved and no workspace moves; the existing ordinary trailing
  spare follows unchanged. This preserves core behavior rather than adding a
  host-owned lifecycle exception.
- Broker transport (tentative): one discovered producer lifetime; a replacement
  lifetime rotates generation/incarnation and all retained broker state.
  Capture carries a local invalidation watermark plus a monotonic transport
  nonce, not core request/command counters. Exact remembered redelivery is
  byte-identical; forgotten old nonces refuse without draining new commands.
- Command transport (tentative): authenticated sender plus client id and
  command id define deduplication/collection; both queue and take bind the
  serving generation. Terminal collection is redeliverable without replaying
  native effects. A new bus connection cannot collect an old sender's results;
  stable reconnect claims would need a separately reviewed ownership mechanism.
- Host-owned adoption revision (tentative, 2026-10-25): adapters report only
  `nativeWs` and per-display native desktop order/current; core derives logical
  member/view receipts and owns binding. Tiler-owned Windows keeps logical
  receipts. Adapters keep live native-id/handle lookups, not a binding table.
- Optional native current (tentative): `None` means a non-desktop current surface;
  retain logical view while using order. Adopt only ordered desktops, hold
  unknown membership, and preserve fullscreen home/no-write safety.
- Native order is authoritative (tentative), with no insertion-position
  assumption. Identical spanning projections form one logical set; shared is
  fully supported, separate-view modes degraded with adapter warning. Complete
  vanished ids are removals, never renames; fresh generations re-adopt.

## Stage 1b creation-correlation blocker

- Proposed draft types: `NativeCreationDesire { operation_id,
  logical_workspace, target_output }` on replies (`nativeCreations`), and
  `NativeCreationAttempt { operation_id, logical_workspace, target_output,
  setter, order_before, order_after, attempt_ms }` on reports
  (`creationAttempts`). `NativeObservation.current_native` becomes
  `Option<String>` (`current: null`, missing/empty refused). These are not a
  delivered API; macOS stage 3 must not consume the draft as settled.
- Draft desires use stable `op-native-<serial>` identities, not command ids;
  attempts match operation/logical/output, consume once, and withdraw execution
  work after an attempt. Refusal, changed replay, stale/future stamps and
  multiple candidates hold with `AmbiguousCreation`. The ordinary empty spare
  never requests creation. Native order reorders bound logical rows.
- Blocking counterexample: accept creation for logical L with before=after
  `[a,b]`; the requested desktop T is asynchronous. A user creates X before T
  appears. Fresh order `[a,b,X]` is identical to the trace where X is the requested
  desktop. The draft binds X to L and retires the operation; it has no causal
  proof. This requires no false adapter evidence: setter acceptance is not
  completion. A later `[a,b,X,T]` cannot repair that already-consumed attribution.
- Regression `accepted_no_delta_late_singleton_must_not_guess_outside_race` in
  `crates/tiler-core/tests/world_host_owned_stage1b.rs` fails: three bindings
  instead of the required two, with X bound to L. The same file's
  `delayed_no_delta_attempt_settles_on_late_readback` passes but encodes the
  incompatible unique-delta inference. Neither oracle was weakened.
- Options: add host-correlated created-id/completion evidence; hold every
  uncorrelated late addition; or explicitly relax to unique-delta inference.
  Recommendation: causal evidence where a host can prove it, otherwise safe
  `AmbiguousCreation` with no re-create. Establish how KDE and macOS provide
  that evidence before choosing a field or claiming delayed settlement. The
  draft stays uncommitted; N3/N4 remain unresolved.
- Existing-native reuse is separately parked: explicit eligible-unbound
  selection versus fresh creation. Recommendation: fresh creation when no
  proven eligible backing exists, never cannibalize another logical workspace
  or reinterpret vanished-id/new-id counts as a rename. A draft count-only reuse
  branch was removed and a deletion-plus-outside-creation regression pins it.
- Neutrality: Windows needs no native creation evidence; KDE/macOS need causal
  asynchronous attribution; GNOME/ChromeOS may offer different native creation
  receipts. Do not force synchronous creation, a returned native handle, an
  insertion ordinal, or a Windows-local backing model on all hosts.
- Sticky semantics are unchanged: float-only over all managed workspaces of the
  owning output; migration leaves sticky members on the source. Native
  all-desktop scope still requires positive host proof.

## Decisions and core-neutrality concerns

### P1. Four-total versus existing trailing lifecycle

- Resolved tentatively as four native-bound workspaces plus ordinary trailing
  spare, as above. `workspace.rs::plan_trailing` remains unchanged; no Windows,
  KDE, macOS, GNOME or ChromeOS-specific lifecycle exception. The
  [discriminator](../spec/reference-outcomes.md#2026-10-24-port-discriminator-stubs)
  now distinguishes native mapping from total rows; native outcome remains TBD.

### N1. Native inventory scope and completeness

- Existing flags suffice for adoption: a present list is complete only with
  both screen `complete` and report `inventory_complete`; otherwise no new
  binding is adopted. Absent rows never remove mappings. Duplicate output rows
  and within-output ids refuse; repeated native ids across outputs remain valid.
- No extra completeness field added. Per-window native membership is the only
  new evidence field. Windows' native-free path is unchanged; KDE global ids
  can repeat across outputs, and macOS-style differing lists remain independent
  in separate-view modes. GNOME/ChromeOS need only report truthful projections
  and lower existing completeness flags when a list cannot be captured.

### N2. Linked views with incompatible native-order projections (parked)

- Shared core has one logical ring. Native lists such as output A `[a,b]` and
  B `[c,a]` cannot use the simple stable-output first-appearance union without
  reversing B's order. Stage 1 holds fresh ambiguous projections using existing
  fences, adopts nothing, and recovers on consistent observations; no merge
  policy or invented backing is selected. Compatible linked projections retain
  one ring and only genuine per-output native bindings.
- Options: require host projections compatible with linked order, select a
  neutral order-preserving merge, or provide richer native-domain evidence.
  Recommendation: establish real host need first, then propose the smallest
  neutral solution; keep the hold meanwhile. Windows native-free linked views
  are unaffected; KDE global lists normally agree; macOS independent Spaces,
  GNOME global workspaces and ChromeOS desks must not be forced into a
  Windows-shaped backing assumption. Native result TBD.

### N3. Empty native creation versus an unbound logical spare (parked)

- Current observations do not correlate an occupant-less native creation with
  an already-published unbound logical spare. Stage 1 truthfully adopts a new
  outside native id as a workspace; it does not pretend to prove a requested
  creation or bind by guessed ordinal.
- Options: retain independent adoption, reuse an eligible empty spare, or add
  an explicit neutral binding observation. Recommendation: exercise the actual
  creation route after the wire cutover before proposing a new shared field.
  Windows creates logical backing locally; KDE/macOS native creation and future
  GNOME/ChromeOS adapters must distinguish observed backing from desired ids.
  Native deletion/reordering policy is not selected by this adoption change.
- The paired adapter draft exposes the correlation gap offline: creating
  backing for an unbound desired logical row is observed as a new native id,
  which core adopts under another logical id. Treating that adoption as proof
  for the requested logical row is false; forgetting the pending creation can
  lead to repeated creation. No live desktop creation was attempted.
- Recommendation remains an explicit neutral correlation proposal before any
  core change; do not guess by ordinal, rename a native id into a logical id,
  or weaken the pending-creation oracle. Add native proof after that contract
  is agreed.

### N4. Migrated logical workspace without destination backing (parked)

- `world_commands.rs::commit_migration` moves the logical id between rings;
  it does not invent a destination native binding. The draft correctly resolves
  transfers through `(output, logical id) -> native_binding` and holds an
  unbound target. Migration/member/view settlement is therefore not established
  by the current producer sequence. A correct logical migration result alone
  does not establish native output movement.
- Options: explicitly correlate creation/reuse of destination backing with the
  retained logical operation; require pre-existing observed backing; or defer
  native migration until that contract exists. Recommendation: investigate the
  same smallest neutral binding receipt as N3, retaining hold meanwhile. No
  destination rebinding or core change is selected.
- Windows' local backing, KDE's output-projected global desktops, macOS Spaces,
  and future GNOME/ChromeOS native domains all need truthful operation/backing
  correlation without assumed id equality or fabricated cross-output binding.

### B1. External command wake delivery (parked)

- The draft exposes same-user queue/query routes and emits `WorldWake`, but
  KWin JS `callDBus` has no signal-subscription primitive. An external queued
  command waits for a subsequent native-event capture. This is not accepted
  command responsiveness or a completed broker cutover.
- Tentative direction (2026-10-25): one long-held asynchronous `callDBus` wait
  request completed by the service when work is available, re-armed on reply.
  Any timeout re-arm must name a verified host-limit deadline, not idle polling.
  Implementation belongs to the next package.
- Pinned KWin `v6.7.4`, `src/scripting/scripting.cpp:301-376`: `callDBus`
  uses nonblocking `sessionBus().asyncCall(message)` with no timeout argument;
  its pending-call watcher belongs to the script. Only successful replies call
  JS; errors/timeouts merely log and return. Script teardown destroys the
  watcher, so callback-only timeout recovery is not available. The signature
  (`src/scripting/scripting.h:115-125`) exposes no timeout option.
- Event-driven waiting is mechanically plausible, but not proven end-to-end.
  Qt's effective default timeout value remains unverified; do not invent a
  deadline. Next-package feasibility must establish that bound, normal service
  deadline replies, no-callback error recovery and reload/dead-waiter cleanup
  without periodic polling. No core hook or dependency change is implied.
- This is host transport, not a core policy hook. Other remote hosts must be
  able to use their own event delivery without a KWin-shaped core abstraction.

### Existing deferred product boundaries

- Activities/multi-desktop: existing tentative default retains inactive-activity
  windows unmanaged with slots held; all-desktop maps to sticky; subsets float
  without rehome. Options: retain this default or propose a native-evidence-backed
  alternative; recommendation: investigate fixture-first and preserve core.
- Async configure versus application clamp: options are existing convergence or
  a new neutral pending/completion representation. Recommendation: reproduce
  natively first, inspect pinned KWin completion evidence, then propose only if
  needed. Windows staged readbacks do not establish native completion; macOS,
  GNOME and ChromeOS would need equally truthful host evidence. No new signal.
- Preserve the rearchitecture's parked empty-workspace cross-output focus,
  edge-nearest versus MRU focus, float physical-output versus membership-home
  direction, explicit versus directional send, and cross-output drag preview
  versus release placement. Options remain current behavior or later separately
  approved policy; recommendation: no port-side change.

## Verification and live acceptance

- Linux commands run through `devenv shell --impure --`; dependencies stay in
  `devenv.nix`. No dependency changes for stages 0/1.
- Every core-changing commit: locked core/protocol build and all-target tests,
  strict all-target clippy (`-- -D warnings`), `cargo fmt --all -- --check`, and
  `cargo run --locked -p tiler-protocol --example world_acceptance --
  test-fixtures/world-actions.json`. Include Windows reference regressions
  when shared report construction/semantics change. Inspect `git diff --check`.
- KDE cutover: locked `omnitiler` build/tests/strict all-target clippy;
  `npm --prefix kwin run typecheck`, `build`, `test`; replace v1 vectors rather
  than keeping them green through shims. Effect stages use
  `just build-native-effect` (host-matched static build, no plugin load) plus
  applicable CMake tests. These are future gates, not current passing claims.
- Live work follows [live-kwin-testing](../live-kwin-testing.md) and the
  [fixture contract](../../crates/tiler-live-fixture/README.md): fixture-only
  `OmniTilerTestWindow` scope, fresh baselines, exact controller readiness and
  helper accounting, correlated reports/replies and exact teardown/restoration.
  The current readiness guide names v1 discovery; update it with the cutover.
- Join caller id, native lifetime, output, operation and provenance freshly.
  Test select-away AND intermediate native hide before select-back AND reveal;
  independently read visibility, frame and foreground. Reject missing reads,
  pending operations, holds/omissions, coverage defects or skipped checkpoints.
  Logical receipts, setter success and signal delivery alone never prove this.
- Cover native startup memberships, outside desktop moves/creation, all three
  workspace modes, both-output sends/migration/refill, geometry/constraints,
  presentation freshness and game pause. Physical shortcut/drag/game claims
  remain separate from guarded synthetic fixture checks. No shared connector
  redesign is part of this port.

## Accepted evidence (2026-10-24)

- Unchanged-source targeted tests through devenv: 6 lifecycle tests plus 7
  individual bootstrap/admission/capability/settlement/validation/digest tests,
  13 passed, zero failed. Core-only world acceptance: 6/6 checkpoints.
- These establish the existing append rule and inert native capability, not
  delivered adoption or native hide/reveal. No source or live state changed.
- Independent source review accepted the plan and parked count ambiguity;
  fresh lifecycle/report/bootstrap tests passed 12/12 and world acceptance
  passed 6/6 checkpoints. Pinned KWin inspection corrected a prior notes
  conflation: current `readClientArea` reads workspace work area, not window
  `clientGeometry`; that read-only property exists and must be added for the
  port's hint conversion. `windowAt` and `cursorPos` exist at the pin.
- Full unchanged core/protocol baseline through devenv: locked build and
  all-target tests passed (1,244 tests, zero failed/ignored, 28 suites), strict
  all-target clippy passed, workspace formatting and whitespace passed, and
  world acceptance passed 6/6 checkpoints. KDE and native gates were not run.
- Stage 1 review corrected fabricated cross-output native bindings, missed
  linked late bindings and unknown-membership write authority. Minimized native
  startup homing and an empty-target minimized fallback received regressions;
  the fallback regression failed before its narrow repair and passed after.
  No failed approach remains as an accepted behavior or weakened oracle.
- Final Stage 1 core/protocol/Windows gates through devenv: locked build and
  all-target tests passed (1,984 passed, zero failed/ignored, 50 suites), strict
  all-target clippy passed, workspace formatting/whitespace passed, and world
  acceptance passed 6/6 checkpoints. Adoption coverage is 14/14 tests; codec
  coverage includes native membership parsing, capability refusal and size
  bounds. Independent boundary review accepted the corrected implementation.
  No KDE build, native-host or physical acceptance is claimed.
- Stage 2 partial evidence (2026-10-25): locked `omnitiler` build and tests pass
  (192 passed: 157 library, 24 world-service flow, 11 tray); strict all-target
  clippy and package-scoped formatting checks pass. JS typecheck and build pass; `npm test`
  reports 694 tests, 688 passed, six failed, zero skipped/cancelled.
- Fresh full-workspace formatting and whitespace gates pass. Locked
  `omnitiler` build, all-target tests (192 passed), strict all-target clippy,
  JS typecheck/build and world acceptance (6/6) pass. The full KDE suite still
  reports 688 passed and the same six failures, zero skipped/cancelled.
  Dependencies and shared core/protocol are unchanged.
- The six retained failures are two aligned migration engine cases plus
  migration producer, spare occupancy/retirement, native homing migration and
  split-edge native migration. They are not waived, skipped or claimed as
  positive acceptance. The split-edge logical result selects the expected
  output; the unresolved native move does not follow from it. No new target
  selection policy is needed on that evidence. Native-bound empty retention
  also differs from the older tentative departed-spare retirement oracle;
  preserve current core behavior pending N3/N4 investigation.
- Two harness-only migration settlement variations did not establish the missing
  backing proof. They are not accepted fixes; the failing integration oracles
  remain intact. Staged-binding apply tests prove only behavior after a binding
  is supplied, never that the current host/core creation path can supply it.
- Boundary review found replay of older non-last captures, lost queue
  acknowledgements, subscriber-parser mismatch, logical/native target confusion
  and late-generation queue/take hazards. Corrections have focused regressions;
  the final queue/take stale-callback regressions fail without their gates and
  pass with them. No shared core/protocol or native effect code was changed.
- Stage 2 remains parked, not complete. No native run, full-core gate rerun or
  live hide/reveal acceptance is claimed. Next: resolve event-driven command
  delivery and N3/N4 backing correlation, finish truthful raw attempts, then
  rerun the complete KDE gate without weakening the six integration oracles.
- Stage 2 partial delivery `0f3c92b5` includes the strict paired cutover and
  preserved six red oracles. Rebased over the four macOS updates; fresh offline
  `omnitiler` 192 tests, JS typecheck/build, workspace fmt/whitespace and world
  acceptance 6/6 pass; KDE remains 688/694 passed. No native acceptance.
- Stage 1b draft gates before the final counterexample: locked core/protocol/
  Windows build, 2,006 all-target tests across 51 suites, strict all-target
  clippy, workspace fmt/whitespace and world acceptance 6/6 pass. macOS test
  targets compile after mechanical constructor updates; related world tests pass.
  The new blocking counterexample then reports 21 passed / 1 failed in the
  focused Stage 1b suite. These green gates do not override its semantic failure;
  no Stage 1b core commit or KDE creation-consumer change was made.
- Broader incoming-tree checks also exposed unrelated settings-export catalog
  count drift and macOS dead-code strict-clippy warnings. Some non-native macOS
  tests require a readable macOS version and fail on a non-macOS host. No
  unrelated repair, dependency change or live mutation was attempted.
- Exact next action: settle the causal creation-evidence/safe-hold contract
  against the indistinguishable-prefix regression, then finish Stage 1b gates
  and KDE desire/attempt consumption before rerunning the six migration/lifecycle
  oracles. Stage 1b and the whole port remain active, not archived or complete.
