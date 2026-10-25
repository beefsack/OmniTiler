# KDE world-contract v2 port

## Goal and current outcome

- Make KDE capture/observe/actuate/render over the sole shared `WorldEngine`;
  Windows is the migrated reference. No v1 compatibility, policy copies or
  second owner. [Principles](../principles.md#layering) and the
  [adapter guide](../porting/world-adapter-guide.md) are authoritative.
- Stages 0/1/1b/2 are complete offline. The strict paired broker/v2 cutover,
  B1 command wake and B2 native-attempt/source-release settlement are delivered;
  all 720 KDE JS tests pass. Native acceptance remains TBD and this record stays
  active for the whole port. Stages 3 onward are not started except for the
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
| 1b. Native-only evidence and backing correlation | Core/protocol complete; optional native current, native-only host receipts, hybrid creation desires/attempts, native order/removal and spanning regressions | 32 creation/adoption discriminators and 16 unknown/minimized pins pass; full core/protocol/Windows gates pass; host consumption and native acceptance remain separate |
| 2. Broker and strict v2 cutover | Complete offline: strict broker, creation consumption, raw attempts, migration/lifecycle oracles, B1 wake and B2 source release/native settlement | Rust 216/216; JS 720/720; scoped build/clippy, typecheck/build/fmt and core acceptance green; native acceptance separate |
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
  offline gates and documentation reconciliation. B2 now includes the separately
  approved tentative native-attempt core extension; other core policy is unchanged.
- Core command rejections are explicit terminal failures for their requester,
  not fabricated null successes or arbitrary retries; the same fresh facts then
  enter the observation-only lane. Malformed capture never retires a command.
- Minimum stage 3 work pulled into the draft: projected v2 facts and native
  membership evidence, desire parsing, clear/frame tokens and output-specific
  logical-to-native target resolution, plus explicit native creation desires and
   raw creation attempts. Dispatched frame/member/view attempts now ride reports;
   local legacy attempt bookkeeping is not settlement evidence. Clear actuation
   remains a stage 3 hold, so its raw attempt lane is truthfully empty.
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
- Migration/lifecycle oracle reconciliation (tentative, 2026-10-25): retain the
  migrating core logical id and its output-specific proven backing. Request
  fresh backing when none exists; another logical workspace's empty native
  desktop is not eligible reuse. The earlier dormant-return rule is superseded
  in intent by released-source option 1 below; its implementation is unaccepted.
  Native-bound empty workspaces retain identity; occupancy
  adds an ordinary unbound trailing spare without creating a native desktop.
  These preserve existing core behavior, not new reuse or deletion policy.
- Released-source backing (tentative, user-selected 2026-10-25): release the
  migrating workspace's source binding, keeping no dormant backing there. An
  unbound desktop observed current or otherwise present in that output's complete
  native order adopts as a fresh empty logical row at native-order position, by
  the outside-creation rule. Migration stays at the destination; the user's
  switch wins and the paired refill retires forever. Ordinary trailing/equal-
  count lifecycle is unchanged. No reverse provenance; eligible-spare reuse
  remains parked. Applies equally to macOS Spaces. Implementation and oracle
  alignment remain outstanding, not a completed B2 claim.

## Stage 1b creation correlation

- Delivered core types: `NativeCreationDesire { operation_id,
  logical_workspace, target_output }` on replies (`nativeCreations`), and
  `NativeCreationAttempt { operation_id, logical_workspace, target_output,
  setter, order_before, order_after, created_native: Option<String>, attempt_ms }` on reports
  (`creationAttempts`). `NativeObservation.current_native` becomes
  `Option<String>` (`current: null`, missing/empty refused). Attempt wire fields
  are `op/ws/out/setter/before/after/created/attemptMs`; optional `created` carries
  host-correlated identity, not a required synchronous handle. Missing/null/empty
  means uncorrelated. Host consumers must implement the same contract.
- Desires use stable `op-native-<serial>` identities, not command ids;
  attempts match operation/logical/output, consume once, and withdraw execution
  work after an attempt. Refusal, changed replay, stale/future stamps and
  multiple candidates hold with `AmbiguousCreation`. The ordinary empty spare
  never requests creation. Native order reorders bound logical rows.
- Counterexample motivating the tentative hybrid rule: accept creation for logical L with before=after
  `[a,b]`; the requested desktop T is asynchronous. A user creates X before T
  appears. Fresh order `[a,b,X]` is identical to the trace where X is the requested
  desktop. The draft binds X to L and retires the operation; it has no causal
  proof. This requires no false adapter evidence: setter acceptance is not
  completion. A later `[a,b,X,T]` cannot repair that already-consumed attribution.
- Tentative user choice 2026-10-25, option C (review after KDE/macOS land):
  correlated `created` binds exactly that fresh id once, even with raced orders.
  Otherwise exactly one outstanding creation on that output may bind the next
  singleton addition. Multiple additions/outstanding creations hold
  `AmbiguousCreation`, never blindly re-create. Accepted outside-first race:
  X binds to L, the window lands on a real desktop, and later T adopts as outside
  creation. This relaxation deliberately is not causal attribution.
- `accepted_no_delta_late_singleton_binds_uncorrelated_risk` replaces the former
  no-guess discriminator to pin that explicit trade-off. Causal racing,
  raced-before, sequential causal operations, late visibility, changed replay,
  reused id and multi-outstanding tests distinguish the correlated path and its
  conservative holds. Review caught a stale mint-baseline refusal of valid causal
  receipts; limiting that check to uncorrelated inference fixes both regressions.
- Existing-native reuse is separately parked: explicit eligible-unbound
  selection versus fresh creation. Recommendation: fresh creation when no
  proven eligible backing exists, never cannibalize another logical workspace
  or reinterpret vanished-id/new-id counts as a rename. A draft count-only reuse
  branch was removed and a deletion-plus-outside-creation regression pins it.
- Neutrality: Windows needs no native creation evidence; KDE can prove a same-turn
  delta, macOS can use single-outstanding asynchronous inference, and GNOME/ChromeOS may offer different native creation
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

- Current disposition: fresh creation/correlation and KDE consumption are
  implemented offline. The five former red oracles do not establish an eligible
  unbound native candidate: their existing desktops already back other logical
  rows. Preserve those bindings and create fresh backing. Only a future explicit
  eligible-unbound reuse rule remains parked; no such rule is needed for these
  journeys. The earlier cutover history follows.
- Stage 1b resolves explicit fresh creation correlation through desires/attempts;
  only existing-native reuse eligibility remains parked. The following records
  the earlier cutover gap; KDE consumption is the next integration check.
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

- Current disposition: correlated fresh destination backing, stable logical-id
  migration, member homing, both views and inverse migration pass offline.
  Returning to the original output can revalidate that logical id's dormant
  native binding; this is not arbitrary empty-desktop reuse. No adapter renaming
  or binding shim is retained. Native acceptance remains TBD; B2 below separately
  blocks the source-view-rewind journey. Earlier investigation history follows.
- Stage 1b now authorizes correlated destination creation; native integration
  remains unaccepted until the KDE consumer and the retained oracles pass.
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

### B1. External command wake delivery (offline complete)

- `WaitWorldWake` holds one asynchronous `callDBus` request. Queue insertion
  and actual head drain advance a broker wake cursor; only pending work with an
  advanced cursor completes a wake. The subsequent fresh capture drains one
  command. Rejected/no-drain captures and exact replay cannot cause a hot loop.
- Wait request: `{v:2,gen,adapter,nonce,cursor}`; replies are `wake` with the
  advanced cursor, `lease` with the unchanged cursor, `superseded`, or bounded
  rejection. Wait requires the same UID and live KWin owner as capture, plus
  the bound producer and generation. It neither executes core nor takes the
  capture lock. One server slot, owned-nonce cleanup, supersession and generation
  displacement bound stale/dead waiters.
- Pinned KWin `v6.7.4`, `src/scripting/scripting.cpp:301-376`: `callDBus`
  uses nonblocking `sessionBus().asyncCall(message)` with no timeout argument;
  its pending-call watcher belongs to the script. Only successful replies call
  JS; errors/timeouts merely log and return. Script teardown destroys the
  watcher, so callback-only timeout recovery is not available. The signature
  (`src/scripting/scripting.h:115-125`) exposes no timeout option.
- Timeout chain: Qt `v6.11.1`, `src/dbus/qdbusconnection.cpp:439-446` and
  `qdbusintegrator.cpp:2190-2220`, passes default `-1` to libdbus. At
  `dbus-1.16.2`, `dbus/dbus-connection-internal.h:48` and
  `dbus/dbus-pending-call.c:130` resolve it to 25,000 ms. Qt exposes no
  additional maximum here; plain KWin JS exposes no override.
- Service lease is 20,000 ms, a heuristic below that host limit, not a proven
  scheduling margin. Every wait has one cancelable 25,000 ms host-deadline
  watchdog. Silent errors/timeouts and absent-service discovery chain from that
  limit, even with no native event. Successful idle lease replies re-arm the
  held call without capturing; there is no idle capture or arbitrary retry
  interval. Persistent refusal/supersession stands down to the host deadline.
  Script stop cancels client timers; stale callbacks cannot clear newer waits.
- A timerless recovery proposal was rejected because silent errors still waited
  for a native event. Initial watchdog recovery then reproduced a bound-but-
  waitless stall and immediate rejection/supersession loops; regression-backed
  corrections retain the discovery deadline chain and bound re-arm. A private-
  bus test additionally caught swapped generation/lifetime arguments at the
  endpoint; correcting the tuple restores the real wake/queue/capture route.
- This is host transport, not a core policy hook. Other remote hosts must be
  able to use their own event delivery without a KWin-shaped core abstraction.

<a id="b1-external-command-wake-delivery-parked"></a>

<a id="b2-source-view-rewind-during-migration-parked"></a>

### B2. Source-view rewind during migration

- Core/protocol and paired KDE complete offline with 19 native-attempt settlement
  pins. Source release is arrival-gated, also covers explicit closes/newer off-output proof,
  and adopts listed backing even if never selected. The first proven-rewind
  reply retains the migration parent; fresh adoption supplies its echo on the
  next report, then the refill retires and both switches win. Stage 2 is closed
  offline. Earlier investigation below is historical
  and superseded by the core outcome at the end of this record.

- Minimal discriminator: migrate a workspace; let members and both native views
  land; before the proof capture, restore both old native currents; observe
  again. Core retains the migration and refill child. The adapter's last-moment
  source-view veto recognizes its proven refill was abandoned and holds it;
  the core cannot settle that supersession from the available attempt fields.
  The full JS suite keeps this as a failing final-convergence assertion, not a
  skip. Target-only lag still converges.
- Removed the legacy first-remaining-row refill fallback: only a core-published
  paired child authorizes a source view. The discriminator also checks that no
  unchosen refill is actuated. Local feedback is not core settlement evidence.
- Existing `MemberAttempt`/`ViewAttempt` pre/post/baseline fields name logical
  workspaces. Host-owned KDE therefore reports null there, real setter
  disposition and exact desired-target correlation, then fresh native facts.
  Translating native readbacks through an adapter table would restore a second
  binding owner. Windows can supply logical preimages because its domains are
  tiler-owned; that route does not prove host-owned supersession.
- Options: classify a proven-then-abandoned view as external and teach core to
  settle from native evidence; distinguish lag and external input with neutral
  native attempt preimages/baseline; or reapply the retained refill (risks
  fighting an actual user switch). Recommendation: preserve the safe hold and
  red discriminator, then review native evidence/preimage consumption in core;
  do not invent adapter verdicts or waive this as impossible native timing.
- Neutrality concern: Windows needs its existing logical receipt route; KDE,
  macOS, GNOME and ChromeOS need host-owned native-domain evidence without
  synchronous-switch, ordinal, logical-id-equality or Windows-local assumptions.
  Any shared representation/settlement change needs a separate product decision.
- User-selected option 1 (tentative, 2026-10-25; review after KDE/macOS land)
  authorizes capability-gated native attempt pre/post/baseline evidence, translated
  only by core. Matching outstanding work settles; other changes are outside
  input. Windows logical evidence remains unchanged. The late-send/intermediate-
  readings discriminator must settle, not adopt the tiler's move as outside input.
- The uncommitted draft uses `MemberAttempt.native_pre/native_post` and
  `ViewAttempt.native_pre/native_post/native_baseline`, optional `String`s;
  wire `nativePre/nativePost/nativeBase`. Non-null native evidence without
  `hostOwned` refuses; host-owned ignores logical attempt evidence. KDE captures
  native reads around actual setters without an adapter binding table.
- Blocking discriminator: the exact old source native remains in the native
  order, but `(source, native)` still maps to the logical workspace now owned by
  the destination. `native_echo_for` cannot resolve it in the source ring.
  Native preimages resolve a switch to another surviving bound desktop, not this
  dormant desktop. The original KDE test remains unmodified and red.
- Options: retain the dormant binding and hold (current behavior); adopt the
  selected vacated desktop as a new source logical row and separately preserve
  reverse-migration provenance; or settle without adopting the current (violates
  switch-wins, not recommended). Recommendation: decide the second option's
  identity/provenance contract before implementation; retain hold meanwhile.
  Do not silently sacrifice reverse-migration backing or weaken the test.
- Neutrality: Windows keeps its logical domain; KDE/macOS and future GNOME/
  ChromeOS must represent a selected vacated host desktop without assuming a
  native id is the migrating logical id or stealing its return binding.
- Superseding ownership decision, 2026-10-25: released-source option 1 above
  removes dormant return backing; reverse-migration provenance is not required.
  The unchanged KDE oracles conflict with that decision: the settled migration
  test explicitly forbids a fresh row for the released source, while the B2
  rewind test requires retaining parent and refill despite a proven abandoned
  refill. Neither expectation was altered. Recommended resolution: align those
  assertions with fresh adoption and switch-wins, retaining stable destination
  identity, native order, no unchosen writes and eventual empty transfers.
  Oracle changes require a decision before another semantic implementation.

### Existing deferred product boundaries

- Host-hidden decision (2026-10-25, tentative until native Activities testing):
  KDE inactive-Activity and macOS Cmd+H windows report existing
  `WindowFact.minimized=true`; no new core state. Inactive-Activity windows are
  retained and never tiled, focused or moved by core except with their workspace
  (migration/swap), when the host removes their display, or to complete a transfer
  pending before the hide. Activities support is later work, not implemented here.
- All-desktop maps to existing sticky; subsets float without rehome. Unknown
  float-sticky membership can lose its reply row; tiled-sticky retains a minimized
  slot. Neutral all-desktop home/slot retention remains parked; preserve current
  behavior and investigate fixture-first, never fabricate current membership.
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
- Stage 1b accepted offline evidence (2026-10-25): locked all-target core/protocol/
  Windows build and strict clippy initially pass; 2,032 tests across 52 suites (30
  Stage 1b, 16 unknown/minimized, 28 codec). Formatting and whitespace pass.
  The exact world-acceptance runner passes 6/6 checkpoints; final gates use
  isolated build artifacts after identifying cross-revision cache reuse.
  macOS test targets compile; shared 11 and trailing-join 7 tests pass. After
  integration of the app-hidden amendment, world 52 tests pass.
  Full macOS library: 1,120 pass; four dock-swipe tests require a readable macOS
  version and fail identically on the unchanged baseline on a non-macOS host.
  The trailing-join fixture now supplies exact creation attempts instead of
  relying on the former uncorrelated outside-adoption path; production macOS
  creation-attempt consumption remains follow-up work, not delivered here.
- Native-only view closure: final audit reproduced logical view echoes falsely
  settling a host-owned flight under unknown native current, and stale logical
  echoes blocking actual native proof. Logical echoes are now ignored throughout
  host-owned staging, settlement and supersession tracking; tiler-owned logical
  receipt behavior is unchanged. Two regressions fail before and pass after;
  independent review removed the superseded conflict branch. Final fresh-artifact
  gates: 2,034 core/protocol/Windows tests across 52 suites, strict clippy/build,
  fmt/whitespace and acceptance 6/6 pass; Stage 1b suite is 32/32. macOS world
  52/52 and trailing-join 7/7 pass, all targets compile; full library is 1,120 pass
  plus the same four non-macOS version-gate failures. KDE remains 690/695 and
  `omnitiler` 192/192 after this closure, with typecheck/build/strict clippy green.
- KDE creation integration (2026-10-25): execute only `nativeCreations`, spend
  before the synchronous native call, and report full before/after orders plus
  unique same-turn `created`. Core-published bindings alone release dependents.
  Removed superseded pending logical bindings/name-based recovery. Unreadable
  post-call order retains incomplete execution evidence locally; the first
  successful later full read completes an uncorrelated receipt with original
  write stamp, never an invented empty after-image or causal id. Generation
  reset clears evidence and spent identities. Point-of-use tests pin these paths.
- KDE gates: locked `omnitiler` build, 192 tests and strict all-target clippy pass;
  workspace formatting/whitespace, JS typecheck/build pass. Canonical fixtures
  regenerated through their producers. JS suite: 695 total, 690 pass, five fail,
  zero skipped/cancelled. The native-homing oracle from the former six now passes.
  Creation contract tests changed with the explicit authorization/receipt cutover;
  the five red migration/spare expectations remain intact.
- Remaining five, with no acceptance claim: aligned migration engine target-id
  (`ws-1` versus expected `ws-3`) and target-view (`ws-2` versus `ws-3`); migration
  producer member list (`win-b` versus `win-a,win-c,win-m`); departed spare
  retirement (`ws-4` retained native-bound); split-edge target view (`native-5`
  versus existing `ws-a`). Creation now supplies backing for the migrated logical
  id; the tests expect destination trailing/existing-desktop reuse. These
  differences do not authorize a core policy change or native success claim.
- Parked integration choice: align migration assertions with core's retained
  migrated logical id and fresh backing, or separately review existing-native
  reuse/lifecycle behavior. Recommendation: preserve core and the red oracles
  until the intended native sequence is discriminated; do not rename identities
  or remove bound empty desktops just to make the suite green. B1 remains the
  separately scoped event-delivery investigation, not implemented here.
- The earlier five-oracle handover above is superseded by the Stage 2 outcome
  below. The whole port remains active, not archived or complete.

## Stage 2 oracle and attempt outcome (2026-10-25)

| Original oracle | Disposition | Current offline outcome |
| --- | --- | --- |
| Aligned migration target id | (a) v1 destination-cell identity expectation | Stable source logical id rides to the target on fresh correlated backing; existing destination cell is not stolen |
| Aligned migration target view | (a) old native reuse expectation, plus (c) source rewind ambiguity | Target-only lag converges on the new binding; separate both-view rewind remains red under B2 |
| Migration producer members | (a) fixture poked a native echo instead of selecting the command-authoritative source; (b) memberless actuation depended on requester results | Production numbered selection supplies the intended source; migration members come only from full core desires under actuatable scope, including an empty set with no requester result |
| Departed-spare retirement | (a) old native-bound empty retirement expectation | Native-bound empty stays; occupancy appends one unbound logical spare with zero native creations |
| Split-edge migration view | (a) old existing-desktop reuse expectation | Largest-edge target unchanged; stable logical id, fresh target/refill backing, member homing and both views settle |

- Scope consumption mirrors the Windows v2 reference (`world_diff.rs`,
  `scope_actuatable` and migration loop): Controlled proceeds; Untouched floating
  scopes proceed only with an unheld output and no gesture. Missing/held scope
  cannot prove empty membership. Native work never requires a different client's
  terminal result. No core/protocol behavior changed.
- Native-bound retention is already an input to `world_model.rs` lifecycle;
  `world_commands.rs::commit_migration` already moves the stable id. Tier 1
  source checks remain corroboration, not host-backing policy: COSMIC
  `3d55cba0:src/shell/mod.rs` `ensure_last_empty` extends occupied/pinned ends;
  Hyprland `19fb395d:src/config/shared/actions/ConfigActions.cpp`
  `findOrCreateWorkspace` creates only missing targets. Neither establishes that
  KDE may steal another logical workspace's native desktop or delete its bound
  empty row.
- Raw frame/member/view arrays now carry actual dispatches only, immediate
  readable frame/prewrite evidence, nullable DPI and original broker-clock
  stamps. Throw/refusal and post-output-send early exits retain evidence;
  no-setter paths emit nothing. Host-owned member/view pre/post/base remain null,
  never native ids disguised as logical receipts. Clear stays unattempted while
  its native setter remains outside this stage.
- Pending attempts survive transport loss and redeliver unchanged. Only a
  validated, current, bound accepted reply retires its exactly matching snapshot;
  newer rows survive an older acknowledgement, and generation reset clears them.
  Populated producer bytes and canonical report fixtures cover the new arrays.
- Boundary corrections remove fabricated accepted/no-write member evidence,
  premature acknowledgement, requester-dependent empty migration and the legacy
  unchosen-refill fallback. Focused tests pin these corrections. The first
  target-only lag rewrite alone was insufficient evidence; the restored both-view
  discriminator remains a nonzero semantic failure under B2, not accepted work.
- Final offline gates through devenv with isolated intermediate and target build
  artifacts: locked all-target core/protocol/Windows/omnitiler builds, 2,226 tests
  across 57 suites, strict all-target clippy, workspace fmt and whitespace pass;
  this includes unchanged core/protocol/Windows 2,034 and KDE Rust 192 tests.
  World acceptance passes 6/6 checkpoints. macOS all targets compile; its full
  library reports 1,122 pass and the same four dock-swipe version-read failures
  on a non-macOS host; world 53/53 and trailing-join 7/7 pass separately.
  JS typecheck/build pass; full suite is 708 total, 707 pass,
  one B2 failure, zero skipped/cancelled. The populated raw-attempt file passes
  12/12, including acknowledgement, throw-after-move, early-veto and no-setter
  discriminators. No core/protocol source, dependencies or live state changed.
- Shared-cache gate attempts reproduced stale pre-closure native-view behavior;
  fresh intermediate artifacts eliminate both false core failures. Initial
  verification also produced transport-test timeouts; fresh gates above replace
  those results, not the retained B2 semantic failure.
- Exact next action: review B2 native-view supersession evidence and choose its
  core-owned resolution. B1 remains the separately scoped wake package; stages
  3+ and native testing are not started by this delivery.

## B1 delivery and B2 draft evidence (2026-10-25)

- B1 locked `omnitiler` build and tests: 216 passed (167 library, 38 flow,
  11 tray); strict all-target clippy, formatting and whitespace pass. Private-bus
  `private_bus_wait_wake_end_to_end_does_not_block_queue` proves owner-only wait,
  independent queue progress while parked, wake before drain, and issuer-only
  terminal collection. JS typecheck/build pass; 719/720 tests pass, only original
  B2 fails, zero skipped/cancelled. Wake suite 12/12; timeout/reload/stale-waiter,
  supersession, burst and no-hotloop regressions pass. No native acceptance.
- B2 draft remains uncommitted, not accepted delivery: core/protocol/Windows
  locked all-target build/tests and strict clippy pass, 2,043 tests across 53
  suites; native-attempt suite 8/8 and acceptance 6/6. macOS all targets compile;
  library 1,122 pass plus the same four non-macOS version-read failures. Native-
  only precedence and cross-output refusal corrections have regressions. These
  gates do not resolve dormant-source identity or make the original B2 green.
  Final formatting/whitespace pass; KDE service build, 192 tests and strict
  all-target clippy pass with the draft. JS remains 707/708, only original B2 red.
- Shared Cargo intermediates reproduced stale-schema build failures; rebuilding
  package artifacts and isolating worktree intermediates restored correct inputs.
  Earlier parallel JS runs showed timing failures in two engine tests; both pass
  in isolation and the final full run has only the original B2 failure.
- After integrating subsequent core/macOS changes, locked core/protocol/Windows/
  service builds, strict clippy and all-target tests pass: 2,251 tests across 57
  suites (2,035 core/protocol/Windows plus 216 service). Formatting/whitespace,
  acceptance 6/6 and JS typecheck/build pass. Final full JS run is 719/720, only
  original B2 red. An initial run additionally saw a helper exit 101 (stderr
  discarded by its harness, cause unproven) and a discovery timeout; both cases
  pass individually and in the subsequent full run without source changes.
  macOS all-target check and 58 world-filtered tests pass; full native acceptance
  remains separate.
- Stages 0/1/1b remain complete offline, Stage 2 partial, Stages 3+ and native
  acceptance untouched. Next: decide dormant-source identity/return provenance,
  then complete B2, re-run all gates and close Stage 2 only with a green original
  discriminator. The port record remains active, not archived.

## B2 closure attempt and helper investigation (2026-10-25)

- Reconciled the full native-attempt draft onto the delivered command-wake and
  intervening macOS/core changes without losing files. The delivered B1 worktree
  was clean, fully integrated and removed. B2 remains uncommitted and unaccepted;
  Stage 2 is not closed and Stage 3+ has not started.
- Two semantic approaches failed acceptance. Proactive source release/adoption
  regressed the migration producer's later backing correlation; an A/B run
  isolated the change. Reactive selection/proven-refill adoption restored that
  producer, but only releases on a selected, apparently vacated source with a
  proven refill write, not on migration or a non-current ordered desktop. It
  therefore does not implement the selected option 1 contract. Its B2 result
  moves the failure from final convergence to the earlier retention assertion:
  both transfers are already retired. Neither approach is an accepted fix.
- Independent boundary review confirmed the oracle conflict in
  `world-aligned-migration-engine.test.ts`: the ordinary settled journey requires
  no fresh source logical, and the rewind requires both transfers retained.
  Released-source/fresh adoption plus proven switch-wins require the opposite.
  Preserve the tests and draft pending explicit oracle alignment. Review also
  flagged the reactive helper's negative vacancy inference without a separate
  roster-freshness proof and delayed native attempt translation against mutable
  bindings; establish discriminators before treating either as a safe general
  abstraction. Windows logical receipts remain unchanged.
- Draft verification through devenv: core/protocol/Windows all-target tests
  2,045 passed across 53 suites, including native-attempt 9/9; strict all-target
  clippy, workspace fmt/whitespace and world acceptance 6/6 passed. KDE Rust
  216/216 and strict clippy passed; JS typecheck/build passed. macOS all targets
  checked; world-filtered 58/58 and trailing-join 7/7 passed; full library 1,127
  passed plus the same four non-macOS version-read failures. Existing macOS
  lib-test dead-code warnings also block strict clippy there. These results do
  not override the B2 semantic failure or claim a completed core delivery.
- Helper investigation corrected the earlier incomplete failure description:
  the retained failing run contains three bare helper exits 101 (aligned
  migration, FIFO and split-edge journeys) plus two file-level V8 heap failures
  (cross-scope and float-nav). Original helper stderr was discarded, so cargo
  failure versus helper panic cannot be recovered. The original causes remain
  unproven; no race fix is claimed. Repeated focused run: 16/17, cold parallel
  engine run: 21/22, and three full runs: 719/720 each, only B2 red. No 101 or
  heap failure recurred; the focused and cold runs observed 85 helper exits 0.
- Seven engine harnesses now retain bounded stderr tails and report code/signal
  on `close`, after stdio flush rather than `exit`. A temporary helper compile
  error reproduced discarded/truncated diagnostics and then verified complete
  compiler diagnostics in the failure; the helper source was restored exactly.
  Independent review accepted this diagnostic-only repair. No assertion or
  product behavior was weakened; final full JS run remains 719/720, B2 only.
- Next decision: align the two contradictory migration assertions with the
  selected released-source contract, or retain the old contract. Recommendation:
  align assertions while preserving stronger native adoption/order, stable
  destination, no-refill-reapply and zero-unchosen-write checks. Then implement
  unconditional source release plus ordinary ordered adoption and rerun all
  core/KDE gates. Do not begin Stage 3 while Stage 2 remains blocked.

## B2 completion constraints (2026-10-25)

- Released-source/fresh adoption supersedes assertions encoding dormant backing
  or forbidding a fresh source logical row. Preserve the migration scenarios and
  all compatible assertions, including retention of the migration transfer in
  the first proven-rewind reply. Do not equate moving the logical workspace with
  retiring its transfer: the destination logical identity and membership stay.
- Strengthen the rewind journey with fresh source adoption at native-order
  position, destination identity/membership, both observed user-selected views,
  paired refill retirement with no re-application, and no unchosen source write.
  List every changed assertion and its contract reason in the delivery evidence.
- Replace the unsuccessful reactive-only source release with release plus
  ordinary complete-order adoption, including non-current source backing, while
  preserving later backing correlation. Add negative-vacancy and delayed-attempt
  rebinding discriminators before accepting settlement changes.
- Deliver core/protocol with scoped core/protocol/Windows gates first, then KDE,
  then Stage 2 closure. The existing macOS dead-code strict-clippy failure is
  outside this work; report it without changing macOS policy. Stage 3+ stays out
  of scope. Genuine additional product choices remain parked, not guessed.

## B2 core/protocol outcome (2026-10-25)

- Delivered `MemberAttempt.native_pre/native_post: Option<String>` (wire
  `nativePre/nativePost`) and
  `ViewAttempt.native_pre/native_post/native_baseline: Option<String>` (wire
  `nativePre/nativePost/nativeBase`). Missing/null/empty is unknown. Non-null
  without host-owned capability refuses; host-owned consumes native evidence
  only, tiler-owned keeps logical evidence. Validation, digest and conservative
  wire accounting include the fields. Windows logical behavior is unchanged;
  macOS carries raw Space ids without an adapter binding owner.
- Positive arrival releases source backing; no dormant return binding or reverse
  provenance. Listed released backing adopts fresh at authoritative native-order
  position, current or never selected. Explicit-close and newer off-output exits
  also release; refusal rollback retains backing. Partial native lists or source
  creations defer cleanup in deduplicated retained work, retried during native
  intake even after the migration retires. A source-returned logical cancels
  stale cleanup. Unlisted released ids adopt ordinarily if observed later.
- Retained transfer targets now survive trailing lifecycle while awaiting
  correlated backing. This fixes the earlier producer's empty-leg-3 binding:
  lifecycle had removed its unbound/invisible target and creation authorization.
  Ordinary trailing/equal-count policy remains unchanged. No spare reuse.
- Stage-order freshness preserves the migration transfer in the first proven-
  rewind reply: newly adopted source backing is not yet a staged echo. The next
  observation resolves it, retires the paired refill and settles without fighting
  either user switch. No artificial timeout or reverse-migration metadata.
- Required pins remain green:
  `ask5_late_send_with_intermediate_readings_settles_transfer`,
  `host_owned_unknown_native_holds_despite_conflicting_logical`,
  `cross_output_refused_native_proves_only_with_source_homing`. New pins cover
  never-selected adoption/order, stale occupant facts, incomplete inventories,
  delayed attempts across actual binding replacement while flights remain
  pending, retired replay, empty target lifecycle, all-closed/off-output exits
  and deferred release after another source creation resolves. The old
  negative-vacancy helper is removed, not accepted as a safe abstraction.
- Independent contract review and focused cleanup review accepted the corrected
  design. Fresh isolated-artifact gates after integrating current core/macOS
  changes: locked core/protocol/Windows all-target build/tests and strict clippy
  pass, 2,057 tests across 53 suites (1,277 core, 55 protocol, 725 Windows);
  native-attempt 19/19. Workspace fmt/whitespace and world acceptance 6/6 pass.
  macOS all targets compile, world-filtered 57/57, trailing 52/52 and trailing-join
  9/9 pass; full library 1,149 pass plus the four unchanged non-macOS version-read
  failures. Existing macOS strict-clippy dead code is outside this work.
- KDE changes are prepared separately: only dormant-source/fresh-adoption and
  refill-retention expectations are superseded. Migration-transfer retention,
  scenario faults and final convergence stay. The paired delivery records the
  exact assertion changes and final KDE gates; Stage 2 is not closed by this
  core-only outcome. Native acceptance and Stage 3+ remain outstanding.

## Stage 2 closure and exact assertion alignment (2026-10-25)

- Core delivery `c76ba5f1` precedes the paired KDE delivery. KDE now captures
  member native pre/post and view native pre/post/baseline from fresh reads around
  actual setters and observation-time baselines. Logical fields remain null;
  no adapter binding table. Pending attempt identity/dedup includes native keys.
  A proven abandoned refill plus abandoned target suppresses both view rewrites;
  target-only lag still retries. Core owns settlement and adoption.
- Superseded assertions, and only their contract reasons:
  - `world-aligned-migration-engine.test.ts`, ordinary settled migration:
    `!finalRows.some(source ws-1 binding)` becomes positive fresh source logical
    presence, distinct from the migrated logical. The old absence encoded
    dormant retention; the approved contract adopts released listed backing.
  - Same file, both-view rewind: the old `held.some(kind === "view")` requirement
    becomes refill retirement on the next observation and zero re-application.
    Retaining an abandoned child conflicts with permanent refill retirement.
    The original migration-transfer retention assertion, bound refill assertion,
    scenario and final empty-transfer convergence all remain and pass.
  - Same file's post-rewind write allow-list becomes the strictly stronger empty
    write list. Counting begins before either current is rewound, covering the
    immediate lagged apply as well as subsequent rounds. Added source native-
    order position, distinct fresh identity, retained destination identity and
    native member homing, both user currents, and final binding-survival checks.
  - `world-aligned-migration-producer.test.ts`, return leg:
    `leg2backing === "ws-1"` becomes nonempty freshly created backing absent from
    the pre-return native order, with the original `ws-1` still owned by its
    distinct adopted logical. Dormant return reuse was superseded; no generated
    replacement id is assumed. All unreadable/stale/close/echo faults, stable
    logical identity, membership, both views, exactly-once and leg-3 checks stay.
- Final paired gates through devenv: locked `omnitiler` all-target build/tests
  216/216 (167 library, 38 flow, 11 tray), strict all-target clippy and workspace
  formatting pass. JS typecheck/build and full suite pass 720/720, zero failures,
  skipped or cancelled, including the rewind with the earlier write boundary.
  Core/protocol/Windows gates remain 2,057/2,057 and acceptance 6/6 as above.
- Additional verification disclosed, not repaired here: four unchanged macOS
  dock-swipe version-read failures on a non-macOS host and existing macOS lib-test
  dead-code strict-clippy failure. An optional broader KDE settings-exporter test
  also exposes the previously noted catalog-count drift (19 versus 23); that
  crate is unchanged and outside the service/JS Stage 2 delivery gates. No
  dependency or live-state change. The earlier helper exits/heap failures did
  not recur; original causes remain unproven, diagnostics are now retained.
- Stage 2 is complete offline, not native-accepted. Source-release and other
  host-owned adoption decisions remain tentative for the comprehensive
  cross-platform review in the backlog. Existing parked native-order merging,
  eligible-spare reuse, unknown sticky retention, Activities/configure and
  deferred directional policy questions are unchanged. Deferred release work
  on an absent output remains deduplicated until that output is observed or the
  incarnation resets; no per-report growth is introduced.
- Exact next action: Stage 3 observe/actuate - finish fresh v2 facts and attempt
  receipts, presentation ingress and frame/member/view/clear operation-token
  handling; preserve native identity, recovery, watermark/generation fencing and
  last-moment vetoes; remove legacy feedback/verdict and native-derived homing
  routes. Do not claim native arrival from a setter or logical receipt alone.
  Stage 3 and native testing were not started by this delivery.
