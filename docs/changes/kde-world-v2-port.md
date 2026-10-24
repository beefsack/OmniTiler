# KDE world-contract v2 port

## Goal and current outcome

- Make KDE capture/observe/actuate/render over the sole shared `WorldEngine`;
  Windows is the migrated reference. No v1 compatibility, policy copies or
  second owner. [Principles](../principles.md#layering) and the
  [adapter guide](../porting/world-adapter-guide.md) are authoritative.
- Stage 0 complete against `a0f54105`; stage 1 implements capability-gated
  host-owned adoption in core/protocol. KDE remains unported and native
  acceptance remains TBD. This record stays active for the whole port;
  stages 2 onward have not started.
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
| 2. Broker and strict v2 cutover | Port Rust service and JS wire together; sole identity/sequencer, stream, request/observation namespace and monotonic clock; replace v1 discovery, stale config, wire vectors and legacy routes, never shim them | Protocol/service/JS paired tests; invalidation during flight, duplicates, lost replies, reset, reconnect; authorization separated for query/presentation/commands versus report/actuator; native work actuator-only, terminal results issuing-reply-only, no subscriber effect replay |
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
- Next: stage 2 atomic Rust/JS v2 broker/wire cutover; no stage 2 work delivered
  here. Native adoption and hide/reveal acceptance remain fixture-first work.
