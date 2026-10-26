# KDE world-contract v2 port

## Goal and current outcome

- Make KDE capture/observe/actuate/render over the sole shared `WorldEngine`;
  Windows is the migrated reference. No v1 compatibility, policy copies or
  second owner. [Principles](../principles.md#layering) and the
  [adapter guide](../porting/world-adapter-guide.md) are authoritative.
- Stages 0/1/1b/2/3 are complete offline. Stage 4 is partial: strict raw capture,
  Rust fixed-size wire classification and obsolete selector deletion are
  delivered offline; directional shortcuts and implicit workspace selection/send
  now submit existing core commands only. N6 adds the neutral tiling toggle;
  N6 birth-clear is accepted offline with neutral opt-in: KDE enables it,
  Windows preserves its existing birth route. Deleted raw windows close and
  unmanaged host windows are absent before core admission.
  Role and remaining local-fixed policy remain; gesture transport is accepted
  offline with physical input still pending. Stage 5 remains unaccepted: its
  causal-witness draft has disconnected native fact publication and incomplete
  cross-surface hide routing. Current service 312, JS 720 and native CTest 34
  pass, but do not prove that production boundary. Native acceptance remains
  TBD and this record stays active for the whole port. Host integration and the
  documented unknown host facts remain later work.
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
| 3. Observe and actuate | Complete offline: raw clear/frame/member/view attempts, native presentation ingress, operation spending; local verdict, clamp-budget and native-derived migration-source routes deleted | Rust 216/216, exporter 14/14, JS 764/764; host-matched effect build and final native CTest 34/34; native arrival/physical acceptance TBD |
| 4. Shared policy only | Partial offline: raw candidate capture and shared Rust fixed-size wire classification; obsolete selectors and native float-navigation removed; shortcuts command-only; N6 toggle/birth-clear accepted offline. Role, local-fixed and gesture policy remain | N6 core 1,339, protocol 57, Windows Linux subset 725, service 278 and JS 737 pass; acceptance 6/6. Full stage acceptance not claimed |
| 5. Pure presentation and coordinates | Unaccepted causal-witness draft over the earlier renderer cutover; native counter publication and cross-surface hide routing remain blocking. No KDE pager renderer | Latest service 312, JS 720, native CTest 34 and acceptance 6 pass; component/entry regressions do not establish the complete native event/facts/draw route. See the 2026-10-27 outcome |
| 6. Host integration | Cursor from workspace.cursorPos plus workspace.windowAt, no polling; warp unsupported, never reported applied; active-border color with default system accent; KDE uses the native Plasma pager, so core pager-focused-window color does not apply | Declaration/source checks and adapter tests; border override; native styling remains host-owned |
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

## Stage 3 outcome (2026-10-26)

- Clear now calls KWin's nonactivating `setMaximize(false, false)` at most once
  per `clear_op`, with fresh maximized pre/post reads and actual
  accepted/refused/threw disposition. Fullscreen/minimized/sticky, gestures,
  unknown safety, changed homing/lifetime and stale watermark/generation veto
  before spending; no-setter paths emit no attempt. Already-clear is observation,
  not fabricated execution. Raw attempts retain the original broker clock and
  survive lost acknowledgements; only exact bound incorporation retires them.
- Float spending follows the final identity/safety/freshness gate immediately
  before dispatch. Frame/clear, transfer phases and effects remain spent across
  repeated replies and clear on generation rotation. Fresh captures, not setter
  booleans or signals, provide settlement evidence. Existing migration, both-view
  rewind, target-only lag, FIFO and no-unchosen-write scenarios remain intact.
- Deleted `AttemptOutcome`, `AttemptFeedback`, `PendingAttempt`, `noteFeedback`,
  `settleAttempt`, local possibly-executed verdict transitions and the entry's
  terminal-settlement mirror. Dispatch records and proven-target safety vetoes
  are mechanical only. Deleted the local three-attempt clamp budget and its
  key/reset state: the real-engine regression proves core `constraint-clamp`
  suppression, no writes under hold and resumed writes after external change.
  Deleted migration's native-member-homing source fallback; core result or
  paired refill provenance alone supplies that source. Host-owned reports now
  omit logical views; native current/membership alone supplies core receipts.
- Presentation captures proof-bound foreground/frame, native interactive move,
  actual reversed bottom-first KWin stacking and bounded live pager frames.
  Unknown stacking is empty; unavailable retained float/tile snapshots are null,
  never a live frame relabelled as retained. Unmanaged foreground and game
  identity are not invented. Classification and renderer derivation remain
  Stages 4/5, not hidden alternate routes accepted by this stage.
- Native `WaitPresentationFacts` supplies the actual effect style triple and
  observed modifier chord with instance/revision, availability and supersession.
  Style/chord are seeded before registration and on reconnect. Only the current
  KWin bus owner may wait; foreign calls cannot replace the single pending slot.
  Changes wake captures, idle leases do not. Malformed/unavailable/throw/timeout
  paths hide unknown facts and recover on the verified host deadline; superseded
  calls stand down, avoiding a competing-waiter loop. Late callbacks fence before
  touching the newer watchdog. Teardown publishes unavailable.
- Transport proof: Qt rejects delayed loopback replies on the shared connection
  (`local-loop message cannot have delayed replies`). The sole existing
  ActiveBorder endpoint therefore uses an effect-owned dedicated connection to
  the same bus; service/path/interface and other methods stay unchanged. The
  private-bus test uses production source and topology, dynamic asynchronous
  calls, foreign denial, lease/change/supersession and exact teardown. A separate
  negative leg pins the loopback limitation, not successful acceptance.
- Coordinate proof: FULL/work/frame/client/cursor capture uses adapter-local
  `Math.round` in KWin-logical pixels. Meaningful client hints become
  `clientHint + round(frameExtent) - round(clientExtent)` per axis; missing
  client geometry yields unknown converted hints, zero/unbounded sentinels keep
  their meaning. `clientGeometry` is the window rectangle, not workspace
  `clientArea`. Native style doubles use the same logical units: resolve negative
  extension to current border width, then round to the protocol style range.
  No device-scale multiplication or Windows invisible-border compensation.
  Fractional and decorated-frame regressions pin the conversion.
- A required native-build repair adapts KWin 6.8's `paintScreen` bool return
  while retaining the 6.7 void override, selected at compile time by package
  version. The 6.8 branch propagates painting failure; no runtime policy route.
  Pinned API evidence uses local KWin `v6.7.4` sources (`window.h` clientGeometry,
  setMaximize; workspace wrapper stacking/order; scripting asyncCall), with the
  specific ABI comparison against `v6.8.0:src/effect/effect.h`.
- KDE exporter ownership confirmed: shared catalog grew from 19 to 23 fields;
  the four output/cursor fields currently lack KDE schema keys/controls. The
  explicit partition is now 16 exposed, six unsupported and one schema-only.
  No new control/default or shared catalog change; generated artifacts unchanged.

### Stage 3 accepted evidence and corrections

- Through the declared development environment: locked service all-target build
  and tests 216/216; exporter 14/14 and exact `--check`; strict all-target clippy
  for both crates; workspace formatting and whitespace; core world acceptance
  6/6. Shared core/protocol/Windows source is unchanged; the prior 2,057 baseline
  is not claimed as a freshly rerun full suite.
- JS typecheck/build and full official suite 764/764, zero failures/skips/cancels.
  Host-matched native effect build passes. Final test-enabled native CTest
  passes 34/34 after seed/availability and watchdog corrections. No live KWin
  mutation or native acceptance claim.
- Final integration includes the subsequently landed core remembered-focus
  selection and Windows focus/pager changes. Locked all-target builds and
  strict clippy pass for core/protocol/Windows/service/exporter; final tests
  pass 2,293/2,293 across 59 suites, JS remains 764/764, and acceptance is 6/6.
  The incoming selection change exposed a stale acceptance-fixture expectation:
  `select-previous` command 12 now expects exactly `fx-12` focus on `win-a`,
  the bootstrap focus retained through admission when observed focus is empty.
  Only that expected effect row and its companion explanation changed; checker,
  core semantics, geometry, view flights and all 24 acceptance tests remain
  intact. The initial five example-test failures and exact-run rejection are
  replaced by these fresh passing results, not waived.
- Review corrected undispatched float spending, a fabricated clear-veto row,
  the missing final clear gate, local clamp ownership and pure-hold rescheduling.
  Blanket removal of follow-up scheduling initially stalled the retained
  both-view rewind; the narrow abandoned-refill suppression follow-up restores
  that unchanged oracle and extinguishes on core retirement. The initial
  loopback held-call approach failed on real Qt transport and was replaced by
  the proven dedicated-connection mechanism, not accepted via a weaker oracle.
  Test introspection initially blocked its own event loop; dynamic async calls
  now match KWin's actual producer and complete the transport leg promptly.

### Remaining host facts and neutrality review (parked)

- **Decorated hints above the shared bound:** a client minimum of 16380 plus
  20 logical pixels of decoration reports exactly 16400. Current core sanitizes
  this to absent; KDE neither saturates nor substitutes another constraint.
  Options: retain that bounded sanitization, widen neutral hint representation,
  or express an explicit out-of-range constraint. Recommendation: preserve core
  now, decide a neutral representation with native evidence later. Windows
  physical units and KDE logical units can both reach the bound; macOS, GNOME
  and ChromeOS must not inherit Windows-specific border or scale assumptions.
  The [minimal discriminator](../spec/reference-outcomes.md#2026-10-24-port-discriminator-stubs)
  keeps native/reference outcomes TBD. No core change is requested by Stage 3.
- **Game pause ingress:** no proven focused-fullscreen-game identity source is
  available in the current KDE capture path, so `paused` remains false rather
  than labelling every fullscreen application a game. Existing fullscreen hide
  vetoes remain; pager/game-cost acceptance is not established. Options: ground
  game facts through the shared classification path, conservatively pause every
  fullscreen app, or retain the unsupported lane. Recommendation: investigate
  the shared classification/native facts first, without selecting new product
  behavior. Windows' existing game lane, KDE/macOS host facts and future GNOME/
  ChromeOS sources must feed neutral policy, not adapter app-name heuristics.
- Chord truth is the existing effect's observed modifier level on mouse events,
  not a new keyboard hook. Unknown chord disables that arm; native interactive
  move remains separately observed. Retained pager frame sources and unmanaged
  foreground proof remain unknown where no native source exists; renderer and
  game-pause acceptance belong to the remaining stages.
- Earlier parked native-order merge, eligible-spare reuse, unknown sticky
  retention, Activities/configure and directional product questions remain
  unchanged. Tentative host-owned adoption decisions retain their later
  cross-platform review. Exact next action: Stage 4 broker raw-capture envelope
  into existing shared Rust classification/policy entry points, then delete its
  superseded TypeScript policy copies.

## Stage 4 bounded plan (2026-10-26)

- Cut over the paired capture route atomically to required KWin-shaped raw
  classification facts. The Rust broker normalizes native facts and calls
  existing shared predicates before producing strict v2 `WindowFact`s; no
  projected-fact fallback or new core ingress. Preserve capture replay,
  authentication, freshness and malformed-capture queue retention.
- Delete fixed-size/admission derivation from the JS wire producer with
  captured-shape Rust coverage. Follow with independent boundary review and
  KDE offline gates. Subsequent units remove remaining command/gesture/scope
  copies only where the existing world command path can represent them.
- Born-maximized investigation found that the shared stateless predicates exist,
  but world-engine admission does not call them or issue birth-clear tokens.
  Moving the JS lifetime sets and clear authorization into the broker would
  create a second policy owner. Park that deletion pending a minimal neutral
  world-engine integration proposal; keep existing behavior meanwhile. No core
  change or new broker authorization lane is selected.
- Native property evidence includes KWin `v6.7.4:src/window.h` type flags,
  `transient`, `transientFor`, `modal`, `wantsInput`, `noBorder`, control flags,
  app identity and client hints. Missing local declarations do not imply a
  missing native API. Unknown facts must not become invented eligible facts.

### Stage 4 partial outcome and deletion boundary

- The sole capture lane now requires raw `windows` rows with 19 exact keys:
  `win/out`, `frameGeometry/clientGeometry`, `minSize/maxSize`, `desktops`,
  `normalWindow/managed/deleted/transient/transientFor/dialog/modal/wantsInput`,
  `fullScreen/maximizeMode/onAllDesktops/minimized`. Rectangles carry native
  fractional `x/y/width/height`; hints carry client `width/height`; opaque owner
  and desktop ids are mechanical native identity reads. Optional float/tile
  intent arrays retain the existing proven intent inputs, not native role
  verdicts. Projected verdict keys refuse; no old projected capture fallback.
- `omnitiler::kde_raw` validates and calls shared `size_hints::is_fixed_size_with`
  on client hints, preserving the previous client-size fixed predicate when
  client geometry is unknown. It converts frame-space hints separately using
  the established frame/client extent difference and TS `Math.round` semantics,
  including negative halves. Zero/unbounded sentinels and above-bound decorated
  constraints keep their existing core sanitization, never saturation.
- Missing required booleans/owner reads, malformed membership or invalid native
  maximize enums hold the producer inventory instead of inventing eligibility
  or a single home. Optional unusable client geometry yields unknown hint
  deltas, not whole-capture failure. Raw validation precedes counter/floor/drain
  changes; rejected captures replay exactly and do not retire queued commands.
- Role/control/ownership fields are captured but deliberately not normalized to
  `SafetyFacts` yet. The existing normal-window candidate filter remains on
  both sides while that mapping is unresolved. Non-normal popups with no usable
  frame/output/membership cannot hold ordinary candidates. Full native type,
  app/rule and borderless capture remain unfinished, not implied by this slice.
- Deleted `born-maximized.ts`, `fixed-size-predicate.ts`,
  `migration-source-refill.ts` and `same-axis-move.ts`; their configuration-only
  exports now live in `world-settings-live.ts`, unchanged defaults/read timing.
  Deleted unused `selectMigrationSourceRefill`, `ensureTrailingEmptyDesktop`
  and `chooseDisplacedDestination` copies and their TS policy-only cases;
  `workspace-output-relocation.test.ts` is removed. Existing Rust refill,
  trailing, displacement and same-axis tests cover those policies; config
  carriage tests remain. This is not deletion of the still-active born policy.
- Explicitly retained: `world-born-maximized.ts` lifetime/clear policy;
  `world-observe.ts` local fixed-size derivation feeding that deferred path;
  `scope-class.ts` fixture-only safety filter; oracle/gesture verdict routing;
  shortcut float/native-toggle and tiling-toggle preselection; remaining
  workspace/command-context/state policy audit. Native identity/recovery seams
  are unchanged. No core/protocol/Windows/effect source or dependency-toolchain
  change; the service adds only an existing workspace core-crate dependency.
- Integration tests use `raw_world_eval`, which includes the actual Rust
  normalizer before strict decode/engine evaluation. The initially added TS
  classifier mirror was deleted, not accepted as another oracle. Rust's
  captured-shape corpus has 28 rows; actual broker flow tests own rejection,
  command retention and replay. No live/native acceptance is claimed.

### Stage 4 evidence and rejected approaches

- Final scoped locked all-target tests pass 2,329/2,329 across 60 suites:
  core/protocol/Windows 2,063; service 252 (183 library, 42 flow, 11 tray,
  16 repeated production-normalizer tests in the example target); exporter 14.
  Locked scoped all-target builds/strict clippy, workspace fmt/whitespace,
  JS typecheck/build and exact core acceptance 6/6 pass. Final official JS suite
  passes 758/758, zero failed/skipped/cancelled. The lower JS count reflects
  removed dead selector cases plus three new capture regressions.
- An initial parallel Rust/JS verification run had two discovery timeouts.
  After building all targets,
  the full official JS run passes unchanged; no timeout or assertion was
  weakened. The timeout cause is not established.
- Review rejected invented eligible defaults, ignored `LeaveUnmanaged` verdicts
  and a new transient/modal float behavior. Those membership changes were
  removed entirely, not worked around in another adapter owner. A decorated-
  only fixed classifier also lost existing client fixed pins under unknown
  client geometry; the shared predicate now receives the true client hints.
  Added absent-property/owner/membership and non-normal-unreadable regressions.
- Regenerated reply-golden pager content no longer includes minimized
  `win-min`, matching the current core producer; its projected window facts are
  unchanged. The exact fixture check remains, not an exception for stale data.

### N5. Native role/control normalization (parked)

- Shared `window_policy::classify_membership` leaves held safety exclusions
  unmanaged, otherwise transient windows float. Windows maps owned/dialog
  facts into exclusions; macOS already consumes neutral transient float
  membership. KWin exposes actual dialog/transient/owner/modal and wants-input
  properties, but ownership is not automatically a Windows-owned-dialog
  exclusion, and KWin focusability is not an HWND no-activate style bit.
- Options: document truthful native-to-existing-neutral-fact mappings; or
  propose the smallest shared representation change if the existing predicates
  cannot express a required distinction. Recommendation: try the existing
  membership entry point first with explicit cross-host examples, never invent
  false cloaked/privileged/tool facts or collapse `LeaveUnmanaged` into Tile.
  Preserve current KDE candidate semantics until resolved. Windows must retain
  its safety exclusions; KDE/macOS and future GNOME/ChromeOS must express native
  transient/control facts without assuming Windows ownership, styles or
  visibility semantics. The minimal discriminator remains native/reference TBD.

### N6. Missing world intents (approved 2026-10-26; partial outcome below)

- **Birth clear:** `window_policy` has stateless born/fullscreen/fixed gates,
  but world admission does not call them or issue birth-clear tokens. Options:
  integrate mode and lifetime transitions into `WorldEngine` and reuse existing
  `clear_operation`/raw `ClearAttempt`; or retain the current adapter policy
  temporarily. Recommendation: the neutral core integration. Never relocate
  the JS lifetime sets into the broker or add a broker clear-authorization lane.
  Windows' existing born behavior must stay; KDE/macOS and future GNOME/ChromeOS
  need commandless birth/exit reconciliation with truthful asynchronous reads,
  not synchronous setter completion or platform-specific logical state.
- **Tiling toggle:** `WorldCommand` has `SetWorkspaceTiling`, not a toggle intent.
  KDE currently negates a cached reply mode before submitting set, including the
  tray route. Options: add a neutral toggle resolved against retained mode at
  execution; or preserve cached preselection. Recommendation: neutral toggle,
  retaining explicit set for callers that genuinely choose a mode. Windows,
  KDE, macOS, GNOME and ChromeOS must all resolve racing toggles identically
  without adapter snapshot policy. The user approved both core changes on
  2026-10-26; [current outcome](#n6-core-intents-outcome-and-blocker-2026-10-26)
  supersedes these earlier parked requests.
- Gesture transport is not blocked on a new core API: `GestureIngress` already
  carries stable id/window/kind, start frame/cursor, live cursor, phase, Esc and
  modal-move facts; `GestureSettled` supplies the terminal outcome. Next bounded
  implementation: replace KDE oracle verdict/resize-target commands with this
  existing lifecycle ingress, then remove its TS resolution code. Retain native
  capture/freshness and truthful unavailable cancel facts.

### Game-identity investigation (parked)

- Windows production `native_guards::classify_foreground` consumes validity,
  desktop identity, visibility, caption style, DWM/cloak readability and full-
  monitor containment. `tiling/workspace.rs::is_borderless_fullscreen` is a
  captionless covering-surface proxy, not positive game identification.
  `tiling_sys/settings.rs::suspend_read_inner` exempts verified managed overlays;
  `run_loop_tick.rs` hides border/underlay/preview on suspension and can retain
  the approved geometry-only lane for other verified windows. A paused flag
  alone does not describe this complete production behavior.
- KWin `v6.7.4:src/window.h` exposes BOTH `fullScreen` and `noBorder` as
  Q_PROPERTYs, plus frame/client geometry, managed/deleted, role and app identity.
  `noBorder` means compositor decoration absence, not absence of client-side
  titlebars or proof of a game. App ids/classes and fullscreen are not positive
  game identity either. No native/shared positive game source was established;
  KDE keeps `paused=false` and existing fullscreen hide vetoes.
- Options: retain unsupported game pause; approve a neutral focused fullscreen-
  surface suspension policy using truthful native control/cover facts (without
  calling it game identity); or require explicit trusted game classification
  before pause. Recommendation: review the neutral safety-proxy option against
  ordinary fullscreen apps and Windows managed-overlay/limited-lane semantics,
  without adding KDE app-name/size heuristics. Windows style/DWM facts must not
  become required core inputs for KDE, macOS, GNOME or ChromeOS; each host must
  provide its own truthful surface/control facts or unknown. No shared change
  or new suspension behavior is selected.
- Stage 4 remains partial and the whole-port record remains active. Stage 5 is
  not started. Existing Stage 3 hint-bound and earlier native-order/sticky/
  Activities/configure questions remain parked. Next action: implement the
  existing gesture lifecycle transport; separately review N5/N6 and fullscreen
  safety before claiming the remaining policy deletions or Stage 4 closure.

## Stage 4 command-only outcome and remaining blockers (2026-10-26)

- Delivered only the independently verified command slice. Directional focus/move
  for tiled, floating and sticky subjects now submit the same existing core
  commands; removed the drain-time float/sticky rejection. Core already owns
  float focus selection, half-snap geometry and clear/placement operations
  (`world_operations.rs`). Deleted `kwin/src/world-float-nav.ts` and its native
  focus/geometry route, not the core's existing behavior.
- Numbered/trailing/previous/relative selections now carry `out:null`; core
  resolves the configured cursor/focused output. Keyboard numbered/relative sends
  carry `HomeThenCachedLive`, preserving retained-home-first resolution without
  adapter output preselection. Genuine explicit-output commands remain valid.
  Native sticky/maximize/fullscreen toggles lack equivalent core commands and
  remain unchanged. N6 tiling-toggle, birth-clear and game/fullscreen suspension
  paths are untouched; no core, protocol, dependency or native-effect change.
- Accepted gates through the declared development environment: locked all-target
  service build, tests 252/252 (183 library, 42 flow, 11 tray, 16 example), strict
  all-target clippy, workspace fmt/whitespace; JS typecheck/build and full suite
  762/762, zero failures/skips/cancels; exact world acceptance 6/6. Independent
  review reproduced the 11 command-routing tests. Synthetic service refusals
  prove adapter surfacing, not native arrival or engine policy. Core-only full
  gates were not rerun because shared source is unchanged. No live acceptance.

### N5 existing-entry experiment: parked, no production change

- A direct `classify_membership` experiment was rejected and removed. Passing
  native minimized into `SafetyFacts` produces `LeaveUnmanaged`; projecting that
  as `floating=false` still makes it a member and collapses the verdict into the
  tile lane. Omitting it instead loses KDE's existing retained-minimized facts.
  Neither outcome is accepted. No false safety defaults or membership code from
  that experiment remains.
- Cross-host examples clarify the missing distinction: a minimized ordinary KDE
  window is retained with `WindowFact.minimized`; macOS filters minimized/hidden
  windows before its all-clear membership call; Windows owned/dialog safety
  exclusions remain unmanaged. A KWin normal transient/parent link is not by
  itself a Windows owned-dialog exclusion, nor is `wantsInput=false` an HWND
  no-activate style. A real KWin dialog has `normalWindow=false` and is currently
  filtered before capture; `normalWindow=true,dialog=true` is not a real type
  combination (`v6.7.4:src/window.h`, `isNormalWindow`/`isDialog`).
- The raw lane also lacks visibility evidence: readable geometry is not proof
  of visibility. KWin's script-visible `hidden` exists at the pin but is not
  carried; gathering it is not itself a solution to the retention distinction.
  Cloaked/privileged/tool facts must never be manufactured from unrelated flags.
- Options: preserve the existing candidate path; explicitly separate retained
  membership from immediate safety eligibility in shared classification; or omit
  minimized members. Recommendation: retain current behavior and review the
  smallest neutral membership/retention contract before another mapping attempt.
  Windows safety gates must stay; KDE, macOS, GNOME and ChromeOS must not inherit
  an HWND-style ownership/visibility model or lose host-hidden membership by
  projection. Native/reference outcomes remain TBD in the existing role row.

### Gesture transport: unaccepted draft, correlation decision required

- Two transport approaches failed semantic review: window-only effect witness
  matching, then script adoption of effect serials using same-window/open and
  start-frame similarity. An unconsumed old open witness can still bind a new
  script START; stateless broker equality checks cannot establish causality.
  Fast START/Esc/finish before the witness arrives also emits an unsupported
  release before real cancel evidence. Stop this approach; no gesture deletion
  or native change is included in the delivered command slice.
- Other unaccepted draft defects: one unavailable effect interval disables later
  witness binding until reset, despite script ids already being monotonic; null
  preview while open can leave an old shape shown. Passing draft tests encode
  these paths and do not override the blockers. No final native gate or native
  acceptance is claimed for that draft.
- Options: make one native lifecycle source authoritative for gesture identity
  and frozen start/finish/cancel facts, folding script cursor/hit facts in the
  broker; or establish an explicit causally acknowledged cross-source binding.
  Recommendation: review the single-native-source boundary first, avoiding
  frame/window/age heuristics and retaining truthful unsupported facts when the
  source is unavailable. This is a transport ownership decision, not permission
  to change core gesture policy. A first-seen revision alone cannot prove that
  a delayed witness belongs to a newer script START.
- Core-produced drop preview must be forwarded separately from group underlay
  bounds and requester command results. Capture-only forwarding is feasible in
  the draft, but is not a substitute for correct lifecycle correlation. Native
  warp remains unsupported and never applied.
- Stage 4 remains partial; Stage 5 and native acceptance remain outstanding.
  Exact next action: choose the gesture capture identity boundary above, add the
  stale-open and fast-Esc discriminators, then replace the unaccepted transport.
  N5/N6 and the existing fullscreen/game, hint-bound, native-order, sticky,
  Activities/configure and directional product questions remain parked.

### Single-source gesture investigation (2026-10-26)

- Tentative transport boundary: one native source owns start, phases, cancellation
  and finish under one immutable gesture identity. The broker folds script
  `workspace.cursorPos`/`workspace.windowAt` facts into that identity; script
  signals never mint a second lifecycle or bind a delayed native witness.
  Native effect capture is the candidate because scripting cannot observe Esc.
  The complete cancellation source is not yet proven; no replacement is landed.
- At KWin `v6.7.4`, `src/window.h:1512-1514` exposes no cancellation parameter;
  `src/effect/effectwindow.cpp:83-91` only re-emits those signals. Geometry
  equality and missing steps do not prove Esc. The native keyboard spy runs
  before filters (`src/keyboard_input.cpp:328-329`); the move/resize filter
  forwards Esc synchronously to `finishInteractiveMoveResize(true)`
  (`src/input.cpp:698-711`, `src/window.cpp:2484-2516,1069-1110`). This establishes
  observation-before-finish, even for a fast gesture, not proof that every
  observed Esc reaches that filter: earlier shortcut/effect filters can consume
  it. A sticky key flag alone therefore needs a causal-delivery fence.
- Non-keyboard cancellation is a separate blocker: Xwayland
  `_NET_WM_MOVERESIZE` cancel calls the same indistinguishable finish
  (`src/events.cpp:614-621`). RootInfo's X11 filter is registered before effects
  and filters run in registration order (`src/netinfo.cpp:131-137`,
  `src/x11eventfilter.cpp`, `src/main.cpp:340-460`). An effect's late filter sees
  the message only after finish. Deferring finish with a zero-delay callback
  does not supply dispatch identity: Xwayland drains multiple messages before
  returning to the event loop (`src/xwayland/xwayland.cpp:382-405`). An earlier
  normal finish followed by a no-op cancel for the same window can bind falsely.
  Qt native-event filters are not this socket-notifier dispatch path.
- Independent source review confirmed these boundaries. No geometry-inferred
  cancellation, deferred same-window correlation, production gesture deletion,
  core change or live smoke is accepted by this investigation. Offline/native
  implementation gates have not been rerun; Stage 4 remains partial.
- **Cancellation contract parked:** options are a proven native per-dispatch
  capture mechanism, a KWin signal carrying explicit cancellation, or explicitly
  reduced host coverage with unsupported completions held. Recommendation:
  retain the existing route until causal cancellation evidence is available;
  do not silently narrow Xwayland coverage. `GestureIngress` has no unknown
  terminal: omission retains a core session, whereas `Cancelled` asserts a fact.
  Neutrality concern: definite terminal facts required by the existing ingress
  must not become an assumed Windows-equivalent host capability for KDE, macOS,
  GNOME or ChromeOS. Any shared unknown-terminal semantics require a separate
  decision, not a broker-side outcome owner.
- Exact next action: resolve the native cancellation boundary before implementing
  transport or deleting the oracle route. N5/N6, game/fullscreen suspension and
  the other parked product questions remain unchanged; cursor warp remains
  unsupported and is never reported applied.

### Gesture transport decision B and replacement plan (2026-10-26)

- Selected cancellation contract: no KWin patch. An Esc observed during an
  active native move/resize reports `Cancelled` only with causal delivery to
  that dispatch's synchronous finish; otherwise finish reports `Released`.
  Earlier shortcut/effect filters may consume Esc. Xwayland
  `_NET_WM_MOVERESIZE` cancel has no distinguishable effect signal and reports
  `Released`. COSMIC-style Esc-as-drop is a possible later fallback, not the
  selected behavior. This supersedes the investigation's recommendation to
  retain the old route pending complete cancellation coverage.
- One effect-owned lifecycle mints immutable identity and freezes start facts;
  the broker folds script cursor/hit-test observations into it. Script signals
  neither mint a second lifecycle nor bind delayed witnesses. No window/frame/
  age correlation. Unavailable native capture omits unsupported ingress; later
  availability must recover without reset. Existing `GestureIngress` and
  `GestureSettled` remain unchanged; omission is not a terminal.
- Replacement units: prove the pinned same-dispatch Esc fence; reproduce the
  stale witness, fast Esc, consumed Esc, availability recovery and null-preview
  failures; implement native lifecycle capture and broker folding; retire the
  superseded oracle only after integration proof; run scoped offline gates and
  separately assess fixture-only native evidence. Shared core is out of scope.
- Gesture transport precedes Stage 5 freshness. Expose its immutable identity
  for the preview path now, but defer preview identity enforcement and the
  native invalidation counter/covered-capture witness to that next package.
  The three Stage 5 blockers below remain unaccepted. Cursor warp remains
  unsupported and is never reported applied. N5, game/fullscreen suspension,
  race-audit items and later stages remain parked.

### Gesture transport cutover: unaccepted recovery outcome (2026-10-26)

- Historical status: the recovery blockers in this subsection are superseded by
  [the accepted offline correction](#gesture-transport-accepted-offline-recovery-2026-10-26).
- **Incomplete, not accepted:** the native source and broker cutover build,
  but terminal-capture recovery has two reproduced failures. The old JS
  verdict/drop/resize/preview route and native oracle endpoint/press capture
  are removed; do not deploy the replacement as accepted gesture transport.
  No live fixture leg or native gesture acceptance is claimed.
- Native `GestureLifecycleStore` mints `{instance, serial}`: effect-instance
  UUID plus nonzero decimal-u64 serial, immutable across phases. Frozen
  window/kind/start frame/start cursor/modal truth accompany every phase.
  Separate decimal `revision` fences waiter updates, not identity.
  Service `com.omnitiler.Gesture`, object `/com/omnitiler/Gesture`, interface
  `com.omnitiler.Gesture1`: `WaitGesture`, `LastGesture`, `AckGesture`.
  Eight head-first terminal records await exact identity ack; overflow discards
  the oldest evidence, never rebinds or fabricates it.
- Delivered-Esc fence: passive `InputEventFilter` at `InteractiveMoveResize`
  weight installs after the builtin filter, so `lower_bound` inserts it before
  that consumer and after earlier shortcuts/effects. It returns false, never
  calls finish, and arms only for the exact live interactive window. Builtin
  Esc synchronously emits finish, consuming the arm as `Cancelled`; otherwise
  finish is `Released`. No sticky spy flag or queued cancellation inference.
  Primary proof: KWin `v6.7.4`, `src/input.h` ordered dispatch/filter weights,
  `src/input.cpp` installation and `MoveResizeFilter::keyboardKey`,
  `src/keyboard_input.cpp` spies-before-filters, `src/window.cpp`
  `keyPressEvent`/`finishInteractiveMoveResize`, and
  `src/effect/effectwindow.cpp` direct finish forwarding. Host-version headers
  were additionally compared. Earlier-filter-consumed Esc and Xwayland cancel
  remain decision-B gaps; COSMIC fallback is not implemented.
- Raw `nativeGesture`, script `cursor` and advisory `cursorWindow` feed
  `kde_gesture.rs` before strict core decode. Broker mapping alone allocates
  existing core-u64 ids. Open phases prefer same-capture script cursor;
  terminals retain native finish cursor. `windowAt` cannot override the mover
  or invent a core hit-target field. Frozen-fact contradictions reject;
  stale revisions/serials omit. Identity retention is bounded to 256 entries
  with preserved serial floors; a seventeenth source instance rotates the
  broker rather than evicting anti-replay evidence. Staged maps install only
  on accepted captures. Non-null script-minted `gestureIngress` refuses.
- Bound actuator-only `gestureAssoc:{instance,serial,coreId}|null` names actual
  normalized ingress, survives byte-identical replay, and stays out of the
  presentation cache. Exact `gestureSettled.id`, association and captured native
  identity must match before ack. Ack retains settled identity through
  host-deadline recovery. `getGestureIdentity` exposes the matching open
  native/core association for Stage 5; preview correlation and the native
  invalidation counter remain unimplemented.
- Focused coverage: fast Esc retains `Cancelled` before first pull; consumed
  Esc finishes `Released`; stale witness/script START cannot mint another
  lifecycle; unavailable source recovers without reset; accepted null preview
  clears an open gesture's old shape. Null clearing already passed before this
  package, not a claimed new red-to-green repair.

| Blocking boundary | Reproduced sequence and result | Required correction |
| --- | --- | --- |
| Terminal fast-retry bound | Drop four consecutive terminal `DescribeWorld` replies without other native events. Retry callback clears its record before resend; attempt count resets to one, so two-second retries exceed the intended bound | Preserve exact envelope and accounting across sends; after bounded fast attempts recover at the host deadline without new terminal identity or duplicate commit |
| Malformed terminal response | Return a non-string terminal response. `lastResolvedCaptureSource` is set before parsing, blocking retry; pending terminal has no recovery | Resolve replay obligation only on a valid definitive response; malformed/oversized transport data must preserve recoverable evidence |

- Fresh declared-environment evidence before the two new red regressions:
  service build/strict clippy and **307/307** tests; core/protocol strict clippy
  and **1,396/1,396**; FFI strict clippy and **20/20**; JS typecheck/build and
  **703/703**; host-matched effect build and **35/35** CTests; exact world
  acceptance **6/6**. Final JS suite with discriminators: **703 passed, two
  failed**, neither skipped. Workspace formatting and whitespace pass after
  mechanical cleanup. No shared-core/protocol source, dependency or other-platform change
  belongs to this package.
- Superseded Rust `drag_oracle.rs`, its crate/header/link and Rust CTest are now
  removed together with their mechanical unsafe-guardrail inventory entries,
  CI/package/notice wiring and active development commands. Shared-core behavior
  and protocol are unchanged. Host-matched native CTest is **34/34** after
  deleting the obsolete Rust-oracle test; unsafe guardrail remains **32/32**.
- **Exact next action:** correct the two retained red recovery discriminators,
  then rerun scoped gates and boundary review. Stage 5 remains unaccepted; N5,
  game/fullscreen suspension, race audit and later stages stay parked. Fixture drag/release/Esc testing
  follows successful offline acceptance only.

### Gesture integration verification (2026-10-26)

- Integrated the newer native macOS selection/pager and Windows/core arrival-
  focus work without changing their semantics. The append-only Windows test
  conflict retains both focus and birth-clear suites. New macOS pager/tile
  report fixtures explicitly retain `birth_clear:false` and default born mode.
- Declared-environment service build, **307/307** tests and strict clippy pass;
  core/protocol **1,402/1,402** and strict clippy pass; Windows Linux-available
  **725/725** and strict clippy pass, not native Windows-target evidence.
  JS typecheck/build pass; **703 pass, two recovery regressions fail**, neither
  skipped. Host-matched native build/CTest **34/34**, exact world acceptance
  **6/6**, workspace fmt/whitespace, portable source checks, source-archive
  contract and third-party-notice freshness pass. Obsolete FFI gate is removed
  with its crate, not represented as a new pass.
- Additional workspace build passes. Workspace tests report **3,928 pass,
  eight fail**, all macOS dock-swipe tests requiring an unavailable native OS
  version on Linux. Workspace strict clippy reports existing macOS dead code:
  `trailing_cleanup_decision`, `observe_creation_receipts`,
  `release_selection_focus`, `SelectionFocusRelease`,
  `release_selection_focus_after_arrival`. No unrelated correction or native
  macOS acceptance is claimed.
- KWin API inventory initially rejected `clientGeometry`, Workspace
  `stackingOrder` and `windowAt`. Local source audit proves all three at
  `v6.6.0` (`e53bb3be7ecf1735c7739b797f94d66de422ab4d`), unchanged at
  `v6.6.4`: `src/window.h:123` and
  `src/scripting/workspace_wrapper.h:66,309`, implementation `.cpp:310,341`.
  Defensive producer reads conform to these signatures/order; inventory and
  **10/10** guard regressions pass without changing capture behavior.

### Gesture transport accepted offline recovery (2026-10-26)

- The two original recovery regressions pass and independent boundary review
  accepts the correction. One implementation initially passed an inaccurate
  fresh-source terminal mock; review rejected it. Corrected tests model the
  real broker's consume-once terminal and byte-identical cached response: a
  fresh source cannot re-emit a consumed `GestureSettled`.
- Retain the oldest exact terminal envelope, original source/watermark and
  attempt accounting through timeout, malformed/wrong-lane response, busy wire,
  same-generation rediscovery and genuine native invalidation. Two bounded
  fast retries transition to existing host-deadline recovery, without permanent
  abandonment or a new terminal identity. Rotation and stop clear the old
  recovery obligation; a newer fresh capture cannot replace the owed envelope.
- `beginReplayCapture` stamps the original envelope watermark.
  `tryDischargeTerminalSettlement` is ack-only: current generation/incarnation,
  exact reply source/watermark, settled-id/core-association equality and exact
  pending native identity must all prove. It applies no setter, desire, effect
  or paint. A stale/superseded reply may therefore discharge only its terminal
  while its snapshot remains unapplied. Mismatched or malformed data cannot
  resolve that obligation.
- Timer justification lives at the definition: the terminal is already frozen;
  two-second cadence bounds lost transport callbacks and rapid retry load,
  while the existing 25-second host deadline bounds silent-call recovery.
  Elapsed time proves no finish, cancellation, freshness or settlement. At most
  one recovery timer is live; neither cadence is a native settle wait.
- Final serial declared-environment gates: workspace build; service **307/307**,
  core/protocol **1,402/1,402**, Windows Linux-available **725/725**, and strict
  scoped clippy pass. JS typecheck/build and **711/711** tests pass, with zero
  failures/skips/cancellations. Host-matched native build/CTest **34/34**, exact
  world acceptance **6/6**, workspace fmt/whitespace, KWin API guard **10/10**
  and third-party-notice freshness pass. Existing full-workspace macOS Linux
  limitations remain **3,928 passed/eight dock-swipe failures** and five strict-
  clippy dead-code errors listed above; no unrelated fixes or native claims.
- Bound remains the broker's existing remembered-source cache. If the exact
  response is evicted before recovery, the adapter cannot manufacture a
  settlement or a replacement terminal; cache-eviction recovery remains a
  separate limitation. No shared-core behavior/protocol change is introduced.
- Native fixture drag/release/Esc evidence is next, separately from this offline
  acceptance. Stage 5 producing-gesture/native-invalidation freshness remains
  unaccepted and is the next renderer package; N5, game/fullscreen suspension,
  race-audit items and later stages remain parked.
- Read-only live preflight created no windows and changed no controller, effect,
  shortcut or configuration. The fixture protocol has no synthetic keyboard or
  pointer-input operation; socket focus/geometry requests do not prove native
  interactive move, release or delivered Esc. Physical fixture input remains
  required for that leg. Controller evaluated-readiness, gesture execution and
  complete post-mutation restoration have not been exercised by this preflight.

## User decisions (2026-10-26)

- [Approved decisions](../decisions.md#kde-port-and-race-decisions-2026-10-26)
  supersede earlier parked recommendations only as specified here; approval is
  not implementation or native acceptance.
- Stage 5: accepted offline gesture transport precedes renderer freshness.
  Every preview names its producing native `{instance, serial}`. Effect-owned
  native invalidation advances on geometry, focus/activation and gesture
  boundaries; the broker records capture coverage, and rendering requires that
  coverage to include the latest invalidation. Full production-routing red
  regressions for the three interleavings below precede implementation. No
  timing, geometry, age or settle-wait inference; any shared-core need returns
  for a separate decision. Pending cancellation semantics remain unchanged.
- Fullscreen suspension option B is approved, not implemented: neutral core
  presentation suppression on the focused-fullscreen output, with truthful
  host fullscreen facts, no game/app/size heuristics and ordinary window
  commands/membership retained. Windows keeps its adapter path until review;
  a neutral fullscreen-exit command is a separate future question.
- N5 option B is approved, not implemented: shared membership is distinct from
  immediate layout/action eligibility; minimized/host-hidden windows stay
  members but ineligible. Windows unmanaged safety exclusions stay unchanged;
  KWin dialog/transient facts use the existing float path. Layout, focus
  navigation, sends and migration all need member-but-ineligible regressions.
- Race-audit option A choices are approved, not implemented: intent-only
  `ToggleFloat` resolves remembered/current floating geometry at execution;
  explicit rectangles mean genuine user placement. Implicit output commands
  leave output unset for execution-time configured cursor/focus resolution;
  explicit output means a real user choice. Prove Windows float parity and
  per-platform output behavior, reporting any visible differences.
- No OmniTiler KDE pager: native Plasma pager only. Its core focused-window
  color is inapplicable on KDE; Stage 6 above is corrected accordingly. The
  backlog entry is already recorded. Earlier proposed KDE pager renderer/color
  work below is historical, not future scope.
- Only Stage 5 causal freshness is implementation scope here. Fullscreen, N5,
  float/output command changes and Stages 6-8 remain separate work.

## Stage 5 bounded plan (2026-10-26)

- Replace native border/underlay geometry and eligibility derivation with the
  accepted broker presentation. Retain native item anchoring, host styling and
  immediate hide-only freshness/liveness vetoes; keep the pending gesture oracle
  and N5/N6/game paths unchanged. Drop preview renders only published core bounds.
- Establish an event-driven read-only presentation consumer with reconnect seeding,
  generation/request fencing and no one-shot lanes. Extend the broker projection
  only as needed to carry the core's separate drop-preview shape. Presentation
  reads retain their existing separate authority from capture and actuation.
- Delete superseded native policy FFI consumers and legacy active-group result
  forwarding, with token/freshness/render-shape regressions and a host-matched
  native build/CTest. Verify the service, full JS suite and exact world acceptance
  through the declared development environment. Shared core remains unchanged.
- Prove KWin-logical geometry end-to-end at the conversion/render use sites and
  in the port notes. KDE uses the native Plasma pager; no OmniTiler pager
  renderer or core pager-focused-window styling applies here.
- Native acceptance remains distinct from offline gates; a live check requires
  fixture-only scope and independent frame/visibility/foreground readback.
- The initial broker draft projected a `drag-preview` result into a separate
  read-only `preview` field before stripping requester results. This draft
  passes the service build, tests and strict clippy but has not had contract
  review and is not accepted delivery. The native
  renderer, legacy forwarding and gesture ingress remain unchanged; Stage 5 is
  incomplete. Next: review preview freshness/retention semantics, implement the
  native subscriber/render cutover, then run the required gates. The outcome
  below supersedes that draft status, not the Stage 5 acceptance conditions.

### Stage 5 unaccepted renderer outcome (2026-10-26)

- **Incomplete, not accepted:** the read-only renderer cutover builds and its
  offline suites pass, but causal freshness remains unproven in three concrete
  source-traced interleavings below. Initial liveness-only and subsequent
  gesture-epoch/held-order-floor fences both leave stale-render paths. No live
  fixture leg or native render acceptance was attempted; Stage 6 was not started.
- Broker projection now carries only preview `rect`, mover-home
  `sourceOut`/`sourceWs` and insertion-neighbor `win`, or null. Requester-only
  `op`, hover chain, session revision and result-domain metadata are excluded;
  take remains sender-bound. Rejections retain the last accepted projection;
  an accepted preview-less capture, including failed-command fallback, clears
  preview. Rotation drops the cache. Three service-flow regressions cover those
  rules and cross-sender terminal isolation. These snapshot rules alone do not
  correlate a preview to a still-live native gesture.
- `world-presentation.ts` reads `ReadWorldPresentation` through its separately
  authorized read lane and forwards accepted shapes to native `SetPresentation`.
  Discovery seeds, accepted-capture notifications refresh, effect-facts instance
  changes reseed, and request/generation checks reject stale callbacks. One
  25-second host-deadline watchdog recovers silent KWin D-Bus errors without
  idle polling. Replies invalidated by an accepted capture during flight are
  discarded before painting. Capture/actuation and terminal collection are not
  renderer authority.
- Native rendering consumes complete border inner/outer, underlay outer and
  preview rectangles; native item anchoring, color/radius and hide-only live
  vetoes remain. Border stroke is recovered only from equal positive insets of
  the published pair, never current configured width. Pinned KWin `v6.7.4`
  `src/scene/borderoutline.h` and `outlinedborderitem.cpp` establish that native
  inflation by that stroke reproduces the published outer rectangle. The
  helper test plus source inspection is not an independent live draw readback.
- Deleted legacy JS active-group and preview-result rendering writers,
  `active-group-highlight.ts`, native group/preview setters, native geometry and
  eligibility policy FFI consumers, `group_highlight_ffi.h`,
  `visual_policy_ffi.h`, Rust `group_highlight.rs`/`visual.rs`, their unused crate
  dependencies and superseded policy-only tests. The native drag oracle and
  presentation-facts ingress remain. Shared-core behavior is unchanged; only
  the unsafe-root test allowlist/fixture was synchronized with deleted FFI files.

| Blocking boundary | Minimal counterexample and current defect | Required proof before acceptance |
| --- | --- | --- |
| Geometry dirt versus in-flight newer snapshot | Hold paint P5; read already-cached P6 captured before a frame move; move the target/member; then deliver P6 before post-move capture. Native dirt floor snapshots P5, so P6 incorrectly clears it despite stale geometry | A capture/native invalidation witness must fence both the read and final native dispatch; numeric advancement beyond the last displayed paint is not causal proof |
| Preview publication versus gesture boundary | Queue preview for gesture A; finish A and start B; drain A's queued preview and read it entirely within B's epoch. Separately deliver an already-sent A paint after B starts | Correlate the producing preview with its live native gesture at publication and dispatch, not the neighbor window or merely any current move. Do not change pending cancellation semantics |
| Focus clear versus in-transit paint | Show A; activate B, clearing native fence; deliver an old A paint; return to A | Production activation and error-hide paths must preserve invalidation/order evidence; reset only an explicitly replaced stream. Helper-only fence tests do not prove production routing |

- [Discriminator rows](../spec/reference-outcomes.md#2026-10-24-port-discriminator-stubs)
  retain native/reference outcomes TBD. Existing green helper tests do not
  execute these complete transport/native-event interleavings. No weakening of
  hide-until-fresh or cross-gesture isolation is selected.
- Coordinates: raw `kde_raw.rs::math_round` and JS rectangle capture use
  `Math.round` semantics, including `-10.5 -> -10`; raw fractions are not
  rounded with Rust's half-away-from-zero rule. Presentation ingress style
  numbers already share KWin-logical frame units. Rendering maps scene to item
  coordinates with `mapFromScene` only: no device-scale conversion, Windows
  invisible-border compensation or second outset. See the use-site proof in
  `presentation_consumer.h`/`updateBorder` and the port notes.
- Native border color keeps its own override and live system-accent default,
  independent of underlay/drop fills. Superseding user decision: KDE uses the
  native Plasma pager; no OmniTiler pager renderer or core pager-focused-window
  color applies on KDE.
- PARKED neutrality concerns: integer rectangles and the shared geometry bound
  quantize fractional host coordinates; `*_px` style names must not imply
  Windows physical pixels on other hosts; `foreground_cloaked` and the documented
  Win+Shift chord are Windows-shaped ingress vocabulary, not portable native
  capability promises. The uniform-ring inset assumption is checked, not
  approximated. No shared-core representation change is selected.
- Final scoped declared-environment gates: locked all-target service build,
  **281/281** tests and strict clippy pass; core/protocol build, **1,396/1,396**
  tests and strict clippy pass; FFI **20/20** and strict clippy pass; JS
  typecheck/build and **750/750** tests pass; host-matched effect build and
  **33/33** CTests pass; exact world acceptance **6/6**, workspace formatting
  and whitespace pass. An additional workspace-wide test run stops at four
  existing macOS dock-swipe version-read failures on a non-macOS host;
  workspace-wide strict clippy stops at existing macOS
  `trailing_cleanup_decision` dead code. Neither overrides the scoped results
  nor the renderer's semantic blockers. No dependency changes.
- **Exact next action:** agree the end-to-end native invalidation and producing-
  gesture correlation contract, then add failing full-interleaving regressions
  for all three rows before further renderer changes. Any shared-core extension
  needs a separate decision. N5, cancellation evidence, game/fullscreen
  suspension, race-audit items and native acceptance remain parked.

### Stage 5 causal-witness draft: unaccepted (2026-10-27)

- **Not accepted.** This draft does not supersede `92010762` as accepted Stage 5
  delivery. The approved causal contract needs no shared-core/protocol change,
  but native routing still violates it. No live load or fixture mutation was
  performed. Fullscreen, N5, float/output command changes and later stages are
  not implemented.
- Draft vocabulary, not an accepted final contract:
  - Effect facts carry decimal-u64 `invalidation` in their own `instance`
    namespace. `PresentationRouter` retains the effect-owned counter separately
    from paint order; missing, foreign or nonmatching coverage refuses.
  - Raw KDE-only `nativeInvalidWitness:{instance,counter}|null` strips before
    strict core decoding. Accepted broker captures publish
    `coveredInstance`/`coveredInvalid` on the read-only projection. This is
    separate from the existing JS-owned envelope `nativeInvalid` watermark.
  - Preview adds `producerInstance`/`producerSerial`, matched against the live
    native gesture at subscriber and endpoint. Unproven command-produced
    previews have null producers and cannot draw; issuer result collection is
    unchanged. `HidePresentation` retains evidence; `ClearPresentation` resets
    paint order only for explicit stream replacement, retaining source truth.
- Failing-first component/entry cases reproduced all three original
  interleavings. An initial correction was insufficient: coverage at apply time
  did not protect a held preview, geometry depended on visible membership, and
  incoming paint could establish the native latest value. The subsequent
  source-owned router/equality-gate correction passes those component cases but
  remains semantically unsuccessful at the production wiring boundary below.

| Remaining blocker | Minimal discriminating sequence and source evidence | Required proof |
| --- | --- | --- |
| Native counter is not published after startup | Seed facts/counter at 0; move a native member; router advances to 1. `noteFactsInvalidation` is called only when the facts object is created, not by the native bump routes. Later captures still cover 0 and the native endpoint refuses indefinitely, even after unrelated chord/style wakes | Execute native event -> actual facts publication -> broker capture -> endpoint, proving immediate stale hide and eventual covering repaint without reset |
| Native visibility predicates are not re-evaluated together | Show border/underlay/preview; move tracked window/member or cross a gesture boundary. Router latest advances, but frame handlers refresh only border or underlay and gesture routes refresh only preview. Other native items can remain visible despite failing covered-current | Exercise actual invalidation-to-item visibility routing for all three surfaces, not only the coverage predicate or source-name checks |

- Coverage limitation: `stage5-causal-freshness.rs` exercises `WorldService`;
  `world-entry-freshness.test.ts` exercises entry/facts callbacks and renderer
  calls; `world-presentation-freshness.test.ts` exercises subscriber routing.
  Native consumer/router tests and metadata/static checks do not execute the
  complete effect-to-facts publication and item-visibility side effects. Their
  green results cannot substitute for the missing full-interleaving regressions.
- Latest declared-environment gates: locked service build/all-target tests
  **312/312** and strict clippy; core/protocol build/tests **1,422/1,422** and
  strict clippy; Windows Linux-available tests **725/725** and strict clippy;
  JS typecheck/build/tests **720/720**; host-matched effect build and native
  CTest **34/34**; exact world acceptance **6/6**; workspace fmt, whitespace and
  KWin API guard pass. Obsolete effect FFI crate was removed earlier; its gate
  is inapplicable, not newly passing. Known macOS-on-Linux limitations were not
  changed or reclassified. No dependency or other-platform source changes.
- Integration with the concurrent no-focus directional work preserves this
  KDE-only diff. The counts above include its additional core/protocol tests;
  fresh integrated build, tests, strict clippy, JS and host-matched native gates
  pass without changing the unaccepted renderer verdict.
- Tentative technical choices: witness/producer field names, null unproven
  coverage, and private Qt-only event routing preserve the selected causal
  contract but are not accepted implementation evidence.
- **Exact next action:** reproduce both remaining blockers through a faithful
  native event/facts/item test route, reconnect counter publication and refresh
  every drawn surface on invalidation, then rerun the complete three-row
  interleavings and all gates. Do not deploy or claim Stage 5 accepted meanwhile.
  No new shared-core decision is currently identified. Physical fixture drag,
  release, delivered Esc, rapid consecutive gestures, focus return and native
  stale-hide/fresh-repaint checks follow offline correction, with exact teardown.

<a id="n6-core-intents-outcome-and-blocker-2026-10-26"></a>
## N6 core-intents outcome (2026-10-26)

- Final continuation decision (tentative technical, 2026-10-26): native KWin
  `deleted` means close, never candidate; `managed=false` means application-
  handled/non-manageable host window, absent from `WindowFact`. Preserve raw
  booleans in capture and normalize in Rust, not a JS policy mirror. Verify
  scripting/property semantics against local KWin `v6.7.4` sources. No core
  controllability/liveness fact and no N5 membership/retention decision.
- Windows keeps its existing birth route. A minimal neutral automatic-core-
  birth opt-in is authorized to avoid preamble refusal bypass and asynchronous
  duplicate restore. Existing unavailable `cfg(windows)` tests stay unverified.
  Remaining work: close/absence producer-to-core pins, neutral capability plus
  Windows non-opt-in pins, boundary review, fresh gates and accepted outcome.
- PARKED neutrality note for later cross-platform review: visible-but-
  uncontrollable host windows (for example integrity-protected Windows windows
  or AX-denied macOS windows) may need a neutral controllability fact. None is
  selected now; KWin override-redirect host mechanics do not decide that model.

- Continuation decision (tentative, 2026-10-26): independently proven
  born-maximized clear authority under `tile-on-launch` may proceed despite a
  geometry-convergence hold. Maximize clear is a birth-authorized mode
  transition, not geometry; host-controlled maximized frames cannot first prove
  converged geometry. Fullscreen, protected/refused/unmanaged and all safety
  holds still veto; game safety wins. Geometry remains held until fresh readable
  facts arrive, and raw `ClearAttempt` plus fresh native evidence alone settle
  clear. The cohort discriminator must pass without weakening its assertions.
- Remaining bounded work: narrow core property authority and negative safety
  pins; preserve Windows' existing born route or prove deferred-corner parity;
  captured-shape raw-producer-to-core KDE birth/exit coverage; independent
  boundary review and fresh full gates. Local-fixed classification stays for
  admission-focus/keep-above consumers. Race-audit parked items, N5, gesture
  cancellation and fullscreen/game suspension remain unchanged. The earlier
  blocked evidence below is historical until superseded by final results.

### Approved scope and accepted toggle

- User-approved core changes: neutral execution-time workspace tiling toggle,
  and world-owned born-maximized/fullscreen/fixed-size admission and lifetime
  reconciliation through existing clear tokens/raw attempts. No broker birth
  owner, synchronous completion assumption or platform-specific logical state.
  [Durable decisions](../decisions.md#world-command-intent-and-birth-ownership-2026-10-26).
- Accepted offline: `WorldCommand::ToggleWorkspaceTiling { output, workspace }`
  resolves retained mode at execution and uses the existing explicit-set body.
  Wire `toggle-workspace-tiling` accepts `op/out/ws`; a `tiled` field refuses.
  Explicit `SetWorkspaceTiling` is unchanged. KDE shortcut/tray and Windows
  callers no longer negate cached mode; display-only snapshot consumers remain.
- Core pins in `world_toggle_tests.rs`:
  `toggle_flips_tiled_to_floating_without_preselected_mode`,
  `toggle_flips_floating_back_to_tiled`, `queued_racing_toggles_compose`,
  `explicit_set_keeps_genuine_set_semantics`, and
  `toggle_unknown_workspace_refuses_like_set`. Codec vocabulary,
  typed-id and extra-key cases plus KDE stale-catchup routing pin the boundary.

### Earlier birth draft, corrections and unresolved authority (historical)

- **Unaccepted draft, not Stage 4 completion:** new isolated `world_born.rs`
  owns birth retention and existing `window_policy` gates. New ingress
  `WorldReport.born_maximized: BornMaximized`, wire `bornMaximized`, backfills
  missing/invalid values to `tile-on-launch`; digest and size accounting include
  it. `op-born-<serial>` tokens use `WindowDesire.clear_operation` and existing
  `ClearAttempt` with no command or new attempt schema. Serials do not reuse ids
  on incarnation rebind. macOS constructor changes only default the new field;
  its live setting carriage is not delivered here.
- Commandless accepted dispatch with unchanged/unknown facts remains pending,
  never re-actuated; later readable unmaximized facts settle. Proven refusal
  retires once; throw/unreadable evidence holds. Fullscreen birth exits, fixed
  gates, per-lifetime keep-overlay and floating-first decisions remain core-side.
  These are draft rules, not proof of native completion.
- Review rejected inline copies of the stateless gates and unbounded
  close-only lifetime sets. Corrections call `should_hold_born_fullscreen`,
  `should_clear_maximize_at_admission_with_born_mode`, `born_keep_latch` and
  `classify_born_fullscreen_exit`, using pre-sweep retained slots; authoritative
  complete departures prune every set. Incomplete absence is not a close;
  minimized retained slots still restore without becoming new births. No
  manufactured `SafetyFacts` or N5 normalization is added.
- Current focused birth tests use normalized synthetic facts, not a captured
  raw KWin producer-to-core birth journey. Realistic captured-shape birth/exit
  integration evidence remains required before acceptance.
- **Blocking discriminator:** first complete report has two maximized,
  `fit_excluded` ordinary births on one tiled workspace. Existing engine
  overlap inference holds the sweep `Unreadable`; post-sweep birth minting then
  refuses that same held scope. Both windows remain maximized and the hold
  cannot resolve. `same_domain_cohort_clear_admission_parked_b1` expects two
  bindings and fails with zero. Sequential arrival passes; using separate
  outputs in the earlier dedup test concealed rather than solved this case.
- PARKED options: (1) separate birth-clear admission authority from frame
  convergence under independently proven window/home/safety facts; (2) teach
  engine seeding to support simultaneous overlaid births; (3) accept the
  indefinite cohort hold. Recommendation: review option 1's narrow property
  authority, keeping geometry/membership holds intact. No blanket hold bypass,
  seed change, adapter fallback or changed expected outcome is authorized.
  [Minimal row](../spec/reference-outcomes.md#2026-10-24-port-discriminator-stubs)
  leaves native/reference outcomes TBD. One unaccepted implementation plus its
  predicate/lifetime corrections remains blocked; green surrounding gates do
  not override the red discriminator.
- KDE draft deletes `world-born-maximized.ts`, its four lifetime sets, the
  entry's native birth-clear decision/recapture path and 34 policy-only tests.
  Native `world-apply.ts` clear actuation/freshness/attempt carriage stays.
  Local `world-observe.ts::isFixedSizeWindow` is **not deleted**: its decoration
  still feeds admission-focus gates (`world-apply.ts:2288,2316`) and automatic
  keep-above (`world-native.ts:858,896`). Options: consume equivalent core
  desires at those use sites or retain the copy temporarily. Recommendation:
  delete only with core-owned focus/stack authorization and pinning tests;
  this remaining policy boundary is not a successful N6 deletion claim.
- Windows retains its existing admission/restart/reconcile preamble. It runs
  before capture, usually supplying post-clear facts; keeping it avoids changing
  its native identity/lifetime behavior without native evidence. Mode ingress
  now reaches the world sequencer; deferred corners may use the existing raw
  clear lane, so unchanged tests alone do not establish identical corner
  outcomes. Windows born parity is **unaccepted**, not proven by 725 passing
  reference tests. No preamble deletion or native Windows change is claimed.

### Race-safety audit (all world commands; other changes parked)

- **Tentative guidance:** prefer commands expressing intent resolved against
  core-retained state at execution. Stable user-chosen ids and live input
  coordinates are not automatically policy preselection. Changing any command
  beyond the approved toggle needs a separate decision. File lines below refer
  to this draft; `Operation` expands all inner `CoreCommand` variants.

| Command | Visible caller / implementation evidence | Risk and recommendation |
| --- | --- | --- |
| `Focus` | `crates/tiler-macos/src/world_builder.rs:2037` test construction; `world_dispatch.rs:1383` match | No production producer found; explicit subject/direction, no cached-mode choice |
| `Send` | `crates/tiler-windows/src/tiling_sys/dispatch.rs:811` | Target workspace sampled as output current before execution. PARKED: preserve genuine explicit target; consider neutral output-current send only if that is the user's intent |
| `SwitchView`, `SelectWorkspace` | KDE command builders; Windows `crates/tiler-windows/src/tiling_sys/selection.rs:73` | Genuine stable workspace selection; retain explicit identity semantics |
| `SelectNumbered`, `SelectRelative`, `SelectPrevious`, `SelectTrailing` | KDE `kwin/src/world-entry.ts:3454,3467,3472`; Windows `cli_workspace.rs:683-707`, `history.rs:155-161`, `workspace_navigation.rs:123-125`; macOS `crates/tiler-macos/src/world_command_map.rs:95-109` | Core resolves positions/history; KDE and Windows implicit routes pass `None`. Windows navigation/macOS pass `Some(output)`. PARKED: distinguish genuinely pinned output from implicit action targeting before standardizing on core resolution |
| `SendNumbered`, `SendRelative` | KDE `kwin/src/world-entry.ts:934,3481`; Windows `sends.rs:81,87`, `workspace_send_tick.rs:126`; macOS `crates/tiler-macos/src/world_command_map.rs:158,168,291,304,313` | Core resolves ordinal/delta. KDE/Windows implicit `SendSource` safe; macOS explicit admitted source is pinned. PARKED: keep explicit claim semantics unless implicit source intent is approved |
| `SetWorkspaceTiling` | `kwin/src/world-report.ts:466` definition, no production caller found | Invites boolean preselection if misused as toggle; keep only genuine explicit sets |
| `ToggleWorkspaceTiling` | KDE `kwin/src/world-shortcuts.ts:321`, `world-entry.ts:3504`; Windows `crates/tiler-windows/src/tiling_sys/workspace_mode.rs:167` | Fixed: mode resolves core-side at execution; target remains explicit logical identity |
| `MigrateWorkspace` | Enum/codec route; no adapter producer found in audit | Genuine explicit source/target; keep |
| `FocusDirectional`, `MoveDirectional` | KDE `kwin/src/world-entry.ts:1310-1311,3344,3361`; Windows `keyboard_directional.rs:281,285` | Subject/direction only; topology/float outcome core-resolved; keep |
| `SendDirectional`, `MigrateDirectional` | KDE `kwin/src/world-entry.ts:892,913`; Windows `workspace_send_tick.rs:280,371`, `migration.rs:157` | Core resolves target/source as documented; keep |
| `Operation(ToggleFloat)` | KDE `kwin/src/world-native.ts:384,498`; Windows `float_toggle.rs:240,604`; macOS `crates/tiler-macos/src/world_command_map.rs:229-231` | KDE/Windows capture `float_rect` at intent time; macOS passes `None`. Highest parked snapshot risk: determine whether geometry is explicit placement intent or should resolve from execution facts; recommend `None` for genuinely implicit geometry, subject to parity review |
| `Operation(Resize)` | KDE `kwin/src/world-shortcuts.ts:195`, `world-entry.ts:3372`; Windows `keyboard_resize.rs:214`; macOS `world_command_map.rs:190-240` | Local repeat index is input sequence, not cached layout choice; keep |
| `Operation(PointerResize, DragDrop, DragPreview)` | KDE `kwin/src/world-drag.ts:261,309,313,363`; macOS `runtime_thread.rs:1315`; core `world_operations.rs:108-124` | Live cursor/boundary/start binding; core validates outcome. Keep; gesture-cancellation contract separately parked |
| `Operation(ToggleOrientation, Move, Focus)` | KDE `kwin/src/world-entry.ts:3378`; Windows `dispatch.rs:528`; macOS `world_command_map.rs:190-240` | Orientation/subject/direction resolved core-side. macOS local Move/Focus pass `cross_output_transfer:false`; PARKED explicit-vs-implicit output policy audit before changing that route |
| `Operation(SendToWorkspace, SendToOutput)` | `crates/tiler-core/src/world_operations.rs:51-68`; no direct adapter producers found | Explicit workspace target, same sampled-current risk as `Send`; retain genuine explicit semantics |
| `Operation(MigrateWorkspace, ReleaseDomain)` | `crates/tiler-core/src/world_operations.rs:72-75,1115,1162`; no direct adapter producers found | World route intercepts/refuses unsupported forms; no new adapter preselection found |
| `Operation(Reconcile, UpdateGaps)` | `crates/tiler-core/src/world_operations.rs:113-124,1124,1191`; no direct adapter producers found | Observation/config reconciliation, no cached action target; keep |

- Bare Windows caller basenames in the table are under
  `crates/tiler-windows/src/tiling_sys/`; bare macOS basenames are under
  `crates/tiler-macos/src/`. KDE native sticky/maximize/fullscreen toggles remain
  outside `WorldCommand`; their live native reads are not cached reply mode,
  but no new neutral toggle API or race-semantics change is selected here.
- PARKED neutrality concern: the retained Windows synchronous preamble can hide
  admission/hold gaps that host-owned asynchronous adapters expose. Options:
  keep separate proven native preambles, or converge on core-owned admission
  with host-independent property authority and parity tests. Recommendation:
  resolve the birth-clear authority contract without imposing synchronous
  setter completion, Windows ownership/styles or logical backing on other hosts.
  Existing N5, gesture cancellation, fullscreen/game suspension, native order,
  sticky, Activities/configure and directional product questions remain parked.

### Earlier blocked evidence (superseded by continuation below)

- Latest declared-environment locked all-target builds and strict all-target
  clippy pass for core/protocol/Windows/service. All-target tests with continued
  execution after failure: **2,347 passed, one failed across 59 suites**:
  core 1,313 pass plus the cohort failure; protocol 56/56; Windows 725/725;
  service 253/253. Focused birth suite is 25 pass / one failure; toggle is 5/5.
- Workspace fmt/whitespace and exact world acceptance 6/6 pass. Latest JS
  typecheck/build and official suite pass 728/728, zero skips/cancels. macOS
  all-target check and world-filtered tests pass 58/58. Earlier full macOS
  library result remains 1,149 pass plus four known non-macOS version-read
  failures; not a newly rerun full-library success. No dependencies changed,
  no live tests or native acceptance; effect gates are unchanged/out of N6.
- **Exact next action:** decide whether birth-clear admission gets narrow,
  independently proven property authority when geometry convergence is held
  (recommended), versus broader cohort seeding or retaining the hold. Then
  repair the unchanged red discriminator, prove Windows born parity and delete
  remaining local-fixed policy consumers before rerunning all gates. Keep this
  whole-port record active; N6 birth and Stage 4 are not complete.

### N6 continuation: cohort resolved, safety evidence blocked (historical)

- The tentative independent-property-authority decision is implemented:
  `born_clear_authorized` distinguishes geometry convergence from per-window
  mode authority, and held reply rows can publish `clear_operation` while
  frame/membership authority stays held with no allocation. The unchanged
  `same_domain_cohort_clear_admission_parked_b1` assertions now pass. A full
  cohort journey proves distinct tokens, raw-attempt incorporation, fresh
  unmaximized facts, subsequent geometry convergence and no re-mint. No engine
  overlap-seeding change or broad frame/member/focus hold bypass.
- Known safety facts re-prove every desire: paused/gesture/unreadable/minimized,
  fullscreen/sticky/floating/fixed, unknown host-owned membership/order conflict,
  floating workspace and migration-gap fences. Review caught retained tokens
  surviving sticky/floating changes; those now abandon without retry. Explicit
  retained float intent also supersedes birth authority without confusing a
  legitimate held-fullscreen-exit exception. Existing ToggleFloat refusal
  semantics remain unchanged; global mode flips do not reclassify a lifetime.
- Added captured-shaped raw-carrier journeys through production
  `WorldService.evaluate_capture` -> `kde_raw::normalize_raw_windows` -> core:
  same-desktop cohort, fullscreen first exit under both modes, fixed birth/exit,
  keep-overlay lifetime latch, asynchronous accepted/unchanged then later clear,
  refusal and non-normal/minimized cases. Data uses UUID-shaped identities,
  fractional frame/client rectangles, maximize enum 3, bounded/unbounded hints
  and raw attempt stamps. These are shaped fixtures, not a claimed live capture.
  The separate JS actuator pin proves a held-row clear writes no geometry.
- **Blocking safety discriminator:**
  `born_raw_unmanaged_deleted_candidates_authorize_no_clear` expects zero clear
  authority for isolated `managed=false,deleted=false` and
  `managed=true,deleted=true` rows. Both instead mint `op-born-1`.
  `kde_raw.rs:295-296` validates but discards these native facts; `WindowFact`
  cannot carry them, and the current native clear gate does not recheck them.
  The original green test asserted that unsafe mint and was rejected; its
  replacement aggregates both no-authorization expectations and stays red.
  The required independent safety proof is absent, so birth remains unaccepted.
- PARKED representation choice: exclude unmanaged/deleted candidates through
  N5 membership normalization, or carry neutral host controllability/liveness
  facts into core and veto birth authority without changing membership.
  Recommendation: the latter, preserving the parked membership policy and
  core-owned birth decision; no fake unreadable rectangles, float flags or
  broker clear-authorization lane. This needs a shared-contract decision.
- **Windows route remains unresolved, not unchanged in outcome:** preamble
  native implementations are untouched, but the new automatic core route can
  bypass their allowlist/hosted-child/medium-integrity refusal gates and can
  double an idempotent restore while a preamble clear lands asynchronously.
  The current core coexistence therefore does not satisfy deferred-corner parity.
  Options: a neutral core ownership/capability switch leaving admission entirely
  on the existing route, or separately reviewed shared safety/evidence and
  deduplication covering the native preamble. Recommendation: retain the current
  Windows route with explicit neutral ownership rather than waive its gates.
  No opt-out, token-prefix filter or new schema has been selected.
- Four new Windows actuator tests cover token spending, already-clear preamble
  wins, gesture/hidden deferral and uncontrolled-scope refusal. They live under
  `cfg(windows)`: Linux builds/tests do **not** compile or execute them. A Windows
  host gate is required before relying on those pins; Linux's 725 passes cannot
  establish that parity. No Windows-native execution is claimed.
- Local-fixed classification stays for admission-focus/keep-above as requested;
  its consumers and future core-authorization deletion boundary above are
  unchanged. ToggleFloat geometry, selection output preselection and explicit-
  send current sampling remain parked, with no command semantic changes. N5,
  gesture cancellation, fullscreen/game suspension and other parked boundaries
  are not silently resolved by this work.
- Latest full declared-environment locked builds and strict scoped all-target
  clippy pass. Tests with `--no-fail-fast`: **2,375 pass / one failure across 59
  suites** (core 1,334, protocol 56, Windows Linux subset 725, service 260 pass
  / one raw safety failure). Core birth 46/46, including unchanged cohort and
  migration-gap journeys. JS typecheck/build and 729/729 tests pass; acceptance
  6/6, workspace fmt and whitespace pass. macOS all-target check and 58 world-
  filtered tests pass, with existing dead-code warnings; full native library
  not rerun. No dependencies or live state changed.
- **Exact next action:** decide neutral controllability/liveness evidence versus
  N5 candidate exclusion, then make the unchanged raw safety expectation pass.
  Resolve explicit Windows birth ownership without changing its existing born
  outcomes, verify its native-target pins, and rerun full gates. The whole-port
  record stays active; N6 and Stage 4 are not complete.

### N6 accepted offline outcome

- N6 is complete offline. The two earlier semantic failures are resolved, not
  waived: the original cohort assertions pass, and the raw unsafe-mint oracle
  is replaced by the selected exact host-lifecycle contract. Independent
  boundary review accepted the corrected source. The entire KDE port and
  Stage 4 remain incomplete; this record stays active.
- Final contract additions for other adapters:
  - `ToggleWorkspaceTiling { output, workspace }`, wire
    `toggle-workspace-tiling` with `out/ws`: flip retained mode at execution;
    no `tiled` key. Genuine `SetWorkspaceTiling` remains unchanged.
  - `WorldReport.birth_clear: bool`, wire `birthClear`: neutral opt-in for all
    automatic birth-clear behavior, missing defaults false; present non-bool
    refuses. KDE sends true; Windows and macOS send false. Off drops automatic
    bindings and lifetime marks and publishes no automatic token; explicit
    directional/unfloat clears remain unchanged. Digest and size budget cover it.
  - `WorldReport.born_maximized: BornMaximized`, wire `bornMaximized`: live
    subsequent-admission setting, missing/invalid defaults `tile-on-launch`.
    Retained lifetime decisions do not reclassify on a settings change.
  - `op-born-<serial>` uses existing `clear_operation` and `ClearAttempt`, no
    new attempt fields. Serial remains monotonic through incarnation rebind;
    generation reset is a fresh namespace. Attempts spend once; accepted plus
    unchanged/unknown facts stay pending. Fresh readable unmaximized facts and
    exact-correlated raw evidence settle; setter acceptance alone never does.
- Independent mode authority allows a proven birth clear under geometry-only
  convergence holds. Frame/membership authority stays held and allocations stay
  absent until fresh facts permit geometry. Paused-game, fullscreen, minimized,
  unreadable, fixed/sticky/floating, explicit float intent, unknown native home,
  native-order conflict and migration-gap fences still veto narrowly. No new
  engine seed policy, safety fact, N5 mapping or broker policy owner.
- KWin host lifecycle normalization is factual, not `classify_membership`:
  `deleted=true` joins explicit closes (deleted takes precedence), never a
  candidate; `managed=false` is absent without a close, never a candidate.
  Both native booleans remain in the raw capture vocabulary; Rust normalizes
  raw rows and deduplicates native/explicit closes. Explicit close still wins
  during incomplete inventory; unknown lifecycle booleans refuse with queue
  retention. Actual scripting inventory excludes deleted objects, so normal
  live close delivery remains the existing event-gated close lane; the observer
  also handles enumerated-deleted objects using identity/lifecycle without
  requiring usable geometry. No speculative tombstone transport or fabricated
  readable/safe defaults.
- Pinned local KWin `v6.7.4` evidence: `src/window.h:281-289` exposes `managed`
  as `isClient` (KWin controls placement versus override-redirect entirely
  handled by the application), and `deleted` as `isDeleted` (already deleted,
  retained only for compositor animation), both constant Qt properties.
  `src/scripting/workspace_wrapper.cpp:505-509` returns `workspace()->windows()`;
  `src/workspace.h:205-213,354` separates live and deleted collections;
  `src/window.cpp:507-518` implements deletion state. These facts do not decide
  N5 retained-membership semantics. Native outcomes are not inferred from source.
- Windows keeps admission/restart/reconcile native preamble behavior, including
  its refusal, proof, attempted-lifetime and asynchronous corners. Production
  `world_tick.rs:1189-1203` dispatches through the sequencer, whose report sets
  `birth_clear=false`; there is no second automatic born lane to bypass those
  gates or duplicate its clear. Explicit directional behavior is unchanged.
  Five added native-target tests (four actuator pins plus sequencer opt-out)
  are **unverified** because `tiling_sys` is `cfg(windows)` and unavailable to
  Linux builds/tests; they are not part of the passing 725 count. Core disabled-
  capability pins establish no automatic mint for maximized birth, fullscreen
  birth/exit, deferred conditions or opt-out after mint, while explicit clear
  still works. This preserves the existing route rather than claiming migration
  or native Windows parity validation.
- KDE deletes `world-born-maximized.ts`, its four lifetime sets, entry-side
  native clear/recapture decision and 34 policy-only tests. Native actuation,
  freshness/spending and raw-attempt transport stay. Raw captured-shaped service
  journeys cover cohort/exit/fixed/async/refusal/keep-overlay plus eight exact
  lifecycle cases; two added corpus rows bring the raw corpus to 30. Eight
  producer lifecycle tests plus the held-clear actuator test cover both sides
  of the boundary. Local `isFixedSizeWindow` stays solely for the already noted
  admission-focus/keep-above consumers; no completion claim for their deletion.
- Merge pins: `queued_racing_toggles_compose`,
  `explicit_set_keeps_genuine_set_semantics`,
  `same_domain_cohort_clear_admission_parked_b1`,
  `cohort_clear_publishes_held_property_tokens`,
  `cohort_clear_settles_and_converges_frames`,
  `async_success_completes_on_later_unmaximized_facts`,
  `migration_gap_defers_born_mint_until_complete_inventory`,
  `disabled_birth_never_mints_or_publishes`,
  `disabled_fullscreen_birth_and_exit_never_bind`,
  `disabled_flag_off_after_mint_produces_no_desire`,
  `disabled_explicit_directional_clear_unaffected`, and codec
  `birth_clear_capability_decodes_tolerantly`. macOS constructor updates only
  set false/default born mode; opting into this new behavior is separate work.
- Final declared-environment evidence: locked all-target builds, all-target
  tests and strict all-target clippy for core/protocol/Windows/service pass.
  **2,399/2,399 tests across 59 suites**: core 1,339; protocol 57 (including 24
  acceptance-checker example tests); Windows Linux subset 725; service 278.
  Focused core born suite 51/51; original cohort assertions unchanged. JS
  typecheck/build and full suite 737/737, zero failures/skips/cancels. Workspace
  fmt/whitespace and exact world acceptance 6/6 pass. macOS all-target check
  and `--lib world` 58/58 pass with existing dead-code warnings; full native
  library not rerun. No dependency changes, live tests or native acceptance.
- Tentative decisions retained: independent property authority, factual KWin
  lifecycle boundary, and intent-first race guidance. PARKED cross-platform
  neutrality concern: visible-but-uncontrollable windows (Windows elevated or
  integrity-protected, macOS AX-denied) may need a neutral controllability fact
  later; none selected now. N5, gesture cancellation, fullscreen/game suspension,
  local-fixed consumer deletion and all race-audit recommendations remain parked.
- **Exact next action for N6: none.** Continue separately scoped port work; do
  not mark Stage 4 or native acceptance complete from N6's offline result.
