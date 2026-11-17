# Remove the stacked inner engines (C1)

## Goal, scope and status

- Status: approved (User 2026-11-17); implementation in progress.
- Make `WorldEngine` the sole engine. Fold retained layout state into its
  workspace model and replace the inner event/acknowledge/verify protocol with
  typed, synchronous layout operations. Keep the algorithms and behaviour.
- Authority: [C1 decision and finding](core-v4-review.md#c1-core-is-three-engines-stacked),
  [approved sequence](core-v4-review.md#user-decisions-2026-11-12),
  [Simplicity](../principles.md#simplicity), and
  [world ownership](../decisions/engine.md#core-owned-world-and-platform-boundary).
- C1 introduces the first owned layout sub-state for C2 to build on, not the
  rest of C2: no general decomposition of connection, focus, settlement,
  intent or presentation, and no replay-cache redesign.
  Not C5: no wire-format extraction or attempt wire-shape unification.
  No product, layout-policy, retry or native-settlement changes.

## Ordering check

- Verified: the review's suggested prerequisite list names C3, C4, C10 and
  the C6 flags (`core-v4-review.md:332-365`). The
  [backlog](../backlog.md) records the flags removed and retains C6 settings
  exposure plus shell classification; shell conversion is explicitly with
  the Windows port. The later backlog goal puts all core/contract work before
  full dogfooding, not all C6 before C1.
- Verified: remaining `HostCaps` in `settings_catalog.rs:171-185,223` describes
  which hosts expose a settings UI, including macOS's absent UI. It is not a
  world handshake capability or a field in the
  [settings message](../porting/world-contract.md#revisioned-settings).
  Moving that exposure metadata to adapters need not change contract shape.
- No evidenced outstanding prerequisite blocks this proposal. Cursor warp is
  separately deferred to Windows; C4 is
  [complete offline](archive/unified-property-attempts.md), retaining the
  existing wire locations for C5. Platform adoption/native acceptance remains
  separate, not claimed complete here.

## Current inventory

- Locations below are in `crates/tiler-core/src/` unless qualified, inspected on
  `96ad5635` plus the current working changes; historical review lines differ.

| Layer or seam | Source | What C1 removes or preserves |
| --- | --- | --- |
| World wrapping an engine | `world_engine.rs:1341-1368`; `engine.rs:54-164` | Remove `WorldEngine.engine` and `Engine`, including its separate identity, settings propagation and request orchestration. Preserve real world admission, sequencing, replay and native convergence. |
| Synthetic event boundary | `boundary.rs:41-203,673-701`; `engine.rs:1428-1556` | Remove crate-private `CoreEvent`, `CoreReply` and `Engine::handle` dispatch, which repeat owner/generation/correlation/revision/fingerprint. Keep public `CoreCommand` as data vocabulary; its removal is not needed to remove the inner engine. |
| Twelve dispatch sites | `world_commands.rs:1385`; `world_operations.rs:1519,1920`; `world_sends.rs:698,940`; `world_gesture.rs:641,826,955,1362,1420`; `world_model.rs:1736,1836` | Replace event construction and reply interpretation with typed calls/results. Include observation sweeps, deferred gesture repair, previews and pair drops, not just commands. |
| Fabricated completion | `engine.rs:3989-4010,4374-4402,4607-4635,4970-5019,5572-5593,5810-5818`; `seed.rs:611-635` | Remove locally fabricated accepted acknowledgements and post-observations copied from plans. No native adapter participates in these handshakes. |
| Inner retained machine | `session.rs:753-843,1846-1958`; `reconcile.rs:236-246` | Remove `Session` as an independent identity/pending/divergence owner, `Reconciler` and staged ack/verify storage. Preserve atomic application of the useful layout data currently installed by verification. |
| World and layout membership | `world_engine.rs:1495-1579`; `world_members.rs:1-28`; `session.rs:760-774`; `directional.rs:154-166` | World members already own homing; sessions are layout projections, not equivalent complete inventories. Preserve trees/leaf bindings without creating a second homing authority. |
| Useful layout machinery | `policy.rs:23-28`; `session.rs:765-835`; `session/ops/`; `session/world.rs`; `session/group_alloc.rs:1-73` | Retain algorithms, local topology, focus recency, restore slots, exception/provenance memory and generation-wide group identity semantics. |
| Types still consumed | `contract.rs`; `boundary.rs:16-28`; `crates/tiler-protocol/src/world_protocol.rs:64-68,4325-4356` | Prune obsolete envelope/ack types, not public semantic vocabulary. Keep `CoreCommand`, `DragSide`, `ResizeMode`, `DomainKey` and `DragHoverPrior` import paths/shapes; no codec or adapter constructor migration is required by this design. |

## Proposed endpoint

- Replace `WorldEngine.engine` with one owned `WorldLayout` sub-state in a new
  `world_layout/` module. Its private fields hold the per-`DomainKey` layout
  collection, policy, group allocator and admission high-watermark; do not
  spread these fields across `WorldEngine`. Each domain holds its tree, leaf
  bindings, retained geometry/gaps and necessary layout history.
- Expose a narrow crate-private typed API for observation convergence,
  projection, local operations, atomic two-domain operations and previews.
  Callers supply validated context and receive outcomes, not mutable maps.
  This is the first owned sub-state C2 will build on, not another engine with
  a report protocol, connection identity or pending native-operation machine.
- Split `world_layout/` by responsibility: state/API, convergence/admission,
  local operations, transfers/pair planning and gesture geometry, with focused
  test modules. Replace the oversized `engine.rs`/`session.rs`, not move their
  monoliths intact. Follow the [File Size](../principles.md#file-size) soft caps:
  about 1,500 production and 3,000 test lines per file; split for responsibility,
  not arbitrary fragments or a new hard line-count gate.
- Keep `members` as the single lifetime/homing authority, and existing
  `mapping`/`views` as workspace-ring/current-view owners. Layout bindings store
  leaf association, not an independently mutable home. Derive snapshot links
  from the workspace key and bindings. Preserve members without layout leaves
  when floating, minimized or held; do not force one-to-one inventory equality.
- Retain focus recency, last-active leaf, minimized restore slots, float
  geometry, automatic/capacity float provenance, tile overrides and admission
  order with their current lifetimes. Do not merge logical command focus with
  observed foreground, or remove useful state merely because `Session` goes.
- One layout-sub-state policy, group allocator and admission high-watermark
  replace engine-wide copies/propagation. Preserve burned group serials across
  failed trials and discarded clones, and D7 ordering across domains.
- Retain the existing layout-revision semantics where observable:
  `world_presentation_v4.rs:376-390` publishes `accepted_revision()` as
  `baseRev`; the protocol encodes it at `world_protocol.rs:6963-6964`.
  World publication revision is not a substitute. Remove fingerprints and
  inner counters only after tracing their consumers; do not add replacement
  correlation identities simply to preserve an obsolete handshake.
- Internal layout operations take typed window/domain/direction/resize inputs
  and return typed outcomes. Translate the stable public command vocabulary
  once at the existing core operation boundary, preserving validation and
  refusal precedence. Do not replace `CoreEvent` with another all-purpose
  command envelope or retain an inner dispatcher.
- Validate a local candidate, then install the affected layout data and world
  membership/view consequences synchronously. Read-only previews install no
  layout topology; preserve existing group-serial burns from discarded preview
  trials (`engine.rs:5040-5074`). Two-domain operations commit both sides or
  neither; temporary pair snapshots are allowed, a retained pair/session
  engine is not.
- Remove replaced orchestration in each migrated path. At completion there
  is no `Engine`, independent `Session` machine, `Reconciler`, synthetic
  ack/post-observation, old event dispatcher or compatibility route.
  Keep meaningful diagnostics, with current privacy and correlation semantics.

## Platform-facing vocabulary and verification limits

- Keep `boundary::CoreCommand` variants, fields and `op()` tokens unchanged,
  including current string-valued fields. Keep `WorldCommand` (including
  `Operation`/`Implicit`), report/reply/settlement shapes, public `WorldEngine`
  constructors and methods, and shared semantic data import paths unchanged.
  These are data/API consumers, not callers of the crate-private inner engine.
  Adapter-facing shape changes belong to C5/the ports; no uncompiled adapter
  edits or duplicate migration are required for C1.
- Verified caller inventory (paths relative to `crates/`):

| Consumer | Relevant dependency |
| --- | --- |
| macOS | `tiler-macos/src/world_command_map.rs:244-295` constructs resize/float/orientation commands; `world_builder.rs:239` carries `WorldCommand`; `runtime_types.rs:825-843` references commands and `ResizeMode`. |
| Windows | `tiler-windows/src/tiling_sys/dispatch.rs:528-530`, `float_toggle.rs:240-243`, `keyboard_resize.rs:214-219` construct commands; `world_sequencer.rs:884-900` matches send/move variants and also uses the public world engine API. |
| Linux service and codec | `omnitiler/tests/rapid_input_burst.rs:942-954` constructs explicit/implicit float commands; `tiler-protocol/src/world_protocol.rs:2230-2396,4325-4356` decodes the vocabulary. Existing sizing/encoding must continue to compile without changing command shapes or wire bytes. |

- Linux evidence is partial, not a native compile claim. Source inspection:
  macOS `lib.rs:161-168` includes the pure command-map/builder modules without a
  host cfg, so host `cargo check -p tiler-macos --lib` can check their production
  constructors; test constructors require checking tests too. Native FFI and
  target-gated dependencies remain excluded. Windows `lib.rs:39-40` excludes
  `tiling_sys` on Linux: a host check can cover portable modules, not the listed
  Windows command callers.
- Cross-target checks require the matching target standard libraries; inspection
  found neither Windows nor macOS target libraries available. No cross-target
  check or successful macOS host check is claimed. No dependency installation
  is part of C1. Native-target evidence still requires an appropriately
  provisioned toolchain or host; keep that evidence separate from Linux gates.

## Behaviour-preservation constraints and risks

- Execute every command in order against preceding core state; native apply
  never gates the next keypress. Platforms still apply latest world.
  [Ordered-command rule](../decisions/commands.md#ordered-core-commands-and-latest-world-application).
- Synthetic commit removal is not native success. Preserve C4 attempt phases,
  retained focus, transfers and predecessors, arrival dependencies, visibility,
  birth/state clears and all held/lifetime/fullscreen/capability fences.
- Preserve refused-operation atomicity, relocation rollback and earlier batch
  results. Existing anchors: `engine.rs:2503-2541`,
  `session.rs:1908-1933`, `world_run_v4.rs:534-545`. Moving a check earlier may
  change refusal precedence or consume state incorrectly.
- Preserve observation-convergence ordering as the dispatcher disappears:
  `engine.rs:1429-1436,1489-1554` converges ordinary operations before planning
  and pair operations once over both domains. Removing the fake handshake
  must not remove, duplicate or reorder this real convergence work.
- Preserve canonical pair identity/duplicate-state guards and admission/gap
  inheritance (`engine.rs:1302-1422`). Current temporary pair identity rewriting
  must not alter published group identities or post-split focus/history.
- Preserve the conservative unchanged-domain proof, not just its fast return:
  `engine.rs:595-633`, `session/world.rs:1586-1664`. Keep focus bookkeeping,
  admission high-water updates and all applicable invalidation conditions;
  removing an obsolete pending flag is not permission to bypass a real hold.
- Gesture preview/drop and seed admissions also use the inner stack. Trace
  every remaining transient drag consumer before deleting it; retain the
  world gesture lifecycle and genuine preview/drop safety rules.

## Migration units and verification

- User 2026-11-17: rebuild directly to the approved endpoint in the most efficient
  order. Intermediate states need not compile or pass gates. No temporary
  access paths, shims or staged coexistence for unmigrated families. Layout
  data always has exactly one storage location; never synchronize copies.
- String-valued `CoreCommand` fields remain stable in C1. Their replacement
  belongs to C5; translate them once at the core operation boundary.

1. Use the existing suite as the behavioural check, porting useful semantics
   rather than the deleted acknowledgement machine. No new before-change
   oracles, sequence-comparison harness, internal snapshots or normalization
   layer. Add a short test only for an important, confirmed coverage gap.
2. Replace layout storage and unchanged-domain proof with `WorldLayout`;
   eliminate duplicate identity/homing ownership. Preserve rollback, layout
   revisions, group allocation and admission ordering.
3. Replace inner dispatch with typed synchronous local, transfer/pair and gesture
   operations; remove `Engine`, independent `Session`, `Reconciler`, synthetic
   acknowledgements and obsolete protocol scaffolding directly. Split by
   responsibility, retaining only the public semantic vocabulary at its current
   import paths. Keep macOS/Windows source, v4 bytes and frozen fixtures unchanged.
4. Preserve useful allocation-burn and skip-equivalence tests, including
   non-fixed-point focus and retired-domain admission high-water. Reuse world
   gesture/restored-routing/logical-contract/selection/state and rapid-input
   tests. Checks between implementation units are optional review aids.
5. Confirm complete replies/publications, settlements, refusal atomicity and
   gesture convergence through existing tests; unsupported behavioural
   ambiguities remain TBD for a user decision.
6. At completion run format, scoped strict Clippy/build/core/protocol/service
   tests, KDE typecheck/tests and frozen-wire/stage5 proofs. Use macOS host
   checks for the portable subset where available; report any failures
   honestly. Obtain native-target caller compilation when available; Linux
   Windows portable checks do not substitute for it. No native caller source
   edits are planned, and native evidence gaps must remain explicit.
   No live desktop mutation is needed to approve this design.
7. Run `bash scripts/check-report-cost.sh` at completion and earlier only where
   useful. Keep its 24M/100M instructions-per-report ceilings and
   6.5 growth bound; do not loosen them to accommodate an unexplained regression.
   Check manual release latency with `report_scale all 31 3 all` at completion
   against the [approved budget](../decisions/engine.md#core-per-report-latency).
   Preserve the [cost optimizations](archive/core-per-report-cost.md), not
   merely a passing aggregate threshold.

## Decisions and evidence limits

- This endpoint and migration plan are approved (User 2026-11-17). No additional
  product choice is identified: session folding belongs to C1 already;
  keeping useful types and observable revision/refusal semantics is preservation,
  not a request to reopen C1-C10.
- Verified by source inspection: inventory, UI-only settings exposure, existing
  ownership, synthetic completion, public-vocabulary coupling, host cfg
  boundaries, observable layout revision and the report-cost gate.
  Target-library availability was inspected; no cargo checks, tests or
  performance measurements were run for this documentation-only proposal.
- Unverified: complete per-operation equivalence, exact replacement signatures,
  all transient-drag consumers and native caller compilation. These are
  implementation proof obligations, not asserted outcomes.
- If implementation uncovers a behavioural ambiguity, stop that change and
  add the minimal discriminating sequence to
  [reference outcomes](../spec/reference-outcomes.md); unsupported outcomes
  remain TBD for a user decision rather than being changed by the refactor.

## Implementation evidence

- `WorldEngine` now owns `WorldLayout` instead of `Engine`. Private per-domain
  storage retains leaf associations and layout history without stored homing
  rows; policy, allocator, admission high-water and configuration are owned once.
  Exception and restore-slot snapshots derive their home from the domain key.
- Storage ownership is installed; algorithm and caller migration is pending.
  Old engine/session modules remain to supply algorithms during replacement,
  not as a working compatibility route. Compilation and behavioural acceptance
  are not yet established; only scoped formatting and source review are current.
- World-owned membership/provenance queries and per-domain focus bookkeeping
  now replace `WorldEngine`'s session-query assumptions. Useful tests are ported
  for leaf-only focus matching, stale recency pruning, native-focus versus
  accepted-history separation, convergence MRU repair and domain isolation.
  These tests are not yet executed while caller migration is incomplete.
- Source review corrected the initial focus-tail extraction to preserve its
  unconditional MRU append, confined indexes/validation to affected domains,
  and restored distinct minimized-slot retention for fixed-size versus capacity
  provenance. Scoped formatting passes; a partial library check reports only
  unmigrated engine-field callers, not a successful crate build.
- Hints-aware projection and typed synchronous orientation now operate on
  per-domain storage and local candidates. Source review preserves separate
  advisory/affected-geometry/admission-fit gates, affected-geometry ordering,
  per-domain refusal scope, revision bounds and the shared focus-settle tail
  even when observed focus does not resolve. Initial extraction conflated two
  geometry gates and omitted that missed-focus tail; both are corrected.
  Callers, convergence/seeding and the remaining operation families are still
  unmigrated. Formatting is current; full build and test acceptance is pending.
- Ordinary-first admission, fallback search, allocator-burning tree edits and
  newest-first capacity repair now work on typed inputs and one disposable
  domain candidate. Repair shares the local focus resolver and fit projector;
  it retains protected/override exclusions and native-then-retained geometry.
  Source review and scoped formatting are current; convergence and caller
  integration are still pending, with no new build or test acceptance.
- Minimized-neighborhood rejoin now uses domain-local restore slots and leaf
  bindings, retaining old-side/same-parent placement, live-survivor identity
  guards, exact share fractions and overflow fallback. Four existing share
  tests are ported unchanged in meaning. Source review and scoped formatting
  pass; these tests are not yet executed during caller migration.
- Retained single-domain observation convergence now validates and installs one
  domain candidate synchronously, preserving ordered admission, restore,
  classification, capacity repair, hidden-lifetime pruning and focus settling.
  Three direct tests cover focus-only revision stability, revision-bound
  rollback with allocation burns, and explicit-hidden marker retention.
  Review removes redundant copied membership indexes. Scoped formatting passes;
  a library check still fails on 129 unmigrated engine-field callers, with no
  convergence-module diagnostic. Tests and end-to-end integration remain pending.
- Typed single-window admission now installs deferred, automatic-fixed, tiled
  or capacity-float candidates without acknowledgement scaffolding, retaining
  sequence/provenance lifetimes and focus-settle rules. Observation gates and
  semantic/revision-bound failures are shared with orientation and convergence.
  Review restores command-shape validation, including placement rectangles,
  before scope/duplicate/completeness checks. Scoped formatting passes; caller
  integration and executed behavioural acceptance remain pending.
- Fresh-domain convergence now constructs an empty local candidate and installs
  only after one successful convergence. Retained convergence shares the same
  candidate builder. Legacy tiled seed construction directly composes the
  admission algorithms, keeping one revision per leg, no-hint seed semantics,
  focus recency, capacity exceptions and discarded-trial allocation burns
  without synthesized observations. Source review and scoped formatting pass;
  two direct tests are added but not executed. Fit/adoption routing, relocation
   and world caller integration remain pending.
- Retained gap updates now validate one local candidate, project with client
  clamp assessment and perform revision-neutral capacity repair when projection
  fails. Empty retirement, exception-only adoption, focus settling and capacity
  provenance pruning retain their source order. The retained projector reuses
  hinted geometry construction rather than duplicating it. Source review and
  scoped formatting pass; three direct tests are present but unexecuted.
  Retained reconcile, fresh routing and world caller integration remain pending.
- Retained reconciliation now builds a local candidate, including unique-source
  relocation when eligible, and installs only after all gates pass. It preserves
  gap-mismatch precedence, relocated exact-set checks, focus-none allowances,
  bounds-change repair and revision-neutral projection. Gap updates share the
  local repair tail. Three relocation/reconcile tests are ported but unexecuted;
  source review and scoped formatting pass. Fresh routing and caller integration
  remain pending.
- Fresh fitted admission now uses the shared recursive-cut fitter and validates
  one local candidate before installation. Fixed-member, centre-split and
  minimum-infeasible declines preserve their diagnostics and allocation burns;
  successful rosters share one admission sequence and one accepted revision.
  Two direct tests cover the focus/sequence tail and discarded-fit burns.
  Scoped formatting passes; a library-and-tests check still fails on unmigrated
  engine consumers, with no fitted-admission diagnostic. Tests remain unexecuted.
- The conservative unchanged-domain proof now checks retained geometry,
  membership, provenance, unhinted observation and resolving focus directly,
  then runs the shared focus-settle tail without cloning a domain candidate.
  The sole admission high-water needs no per-domain counter synchronization.
  Direct tests retain non-fixed-point focus normalization, domain isolation and
  conservative misses. Source review corrects the observation-shape check to
  inspect carried entries directly. Scoped formatting passes; library/tests
  still fail on unmigrated consumers, with no unchanged-proof diagnostic.
- World member restore-slot queries and explicit move relocation now use the
  owned layout directly. Hostability and commit share candidate validation,
  preserve key-derived exception/slot homes and install only after success;
  empty-source and absent-source handling remain at the world boundary.
  One direct relocation test is added but unexecuted. Source review and scoped
  formatting pass; reported diagnostic checks now leave 120 library errors in
  other unmigrated consumers, not a successful build.
- Positive-close focus, view-return focus, off-route state diagnostics and v4
  group presentation now read per-domain layout bindings, focus, geometry and
  revisions directly. Presentation retains world holds and transfer fences;
  obsolete inner pending/divergence checks are removed without changing public
  vocabulary. Trees are borrowed instead of snapshot-cloned. Source review and
  scoped formatting pass; reported library diagnostics leave 112 errors in
  other unmigrated consumers. Existing behavioural tests remain unexecuted.
- Observation classification, admission-size accounting and maximized-float
  restore queries now use layout membership/provenance directly, preserving
  the exception-only floating-geometry lookup and fixed-predicate setting.
  Source review and scoped formatting pass; reported library diagnostics leave
  101 errors in other unmigrated consumers. No test acceptance is claimed.
- Admission now shares a pure local candidate builder with explicit working
  admission-counter input, so fresh seeds and transfers need not install a
  temporary domain to plan admission. The retained method installs its result
  only after success. Two direct tests cover uninstalled candidates and
  revision-bound allocation burns; source review and scoped formatting pass.
  Reported diagnostic checks find no admission-module errors, but global
  compilation and executed behavioural acceptance remain pending.
- Fresh reconcile routing now composes fitted admission, carried-order mixed
  convergence, legacy seed/admission and spatial serial convergence directly.
  Legacy seeded final admission retains its default fixed-admission policy;
  relocated/converged candidates use current configuration. Source review
  restores distinct mixed/serial focus gates, legacy admission geometry,
  relocation gap validation and pre-retirement revision/counter handling.
  Two direct tests cover focused fit versus serial seeding and disjoint-source
  relocation fallback; they remain unexecuted. Scoped formatting passes; a
  library-and-tests check still fails on 101 library and 293 test-build errors
  in unmigrated consumers, with no adoption-module error. Caller integration
  is pending.
- World observation sweeps now compose typed convergence, retained/fresh
  reconciliation and gap updates directly. Gesture deferral uses retained
  geometry; inset exhaustion, unchanged-sweep skipping, disjoint fresh
  adoption and safety-exclusion focus holds retain their source ordering.
  Explicit closes forget restore slots only after successful convergence;
  hidden lifetimes retain their provenance. Two direct tests cover this
  cleanup and revision-bound refusal. Source review corrected a test's
  revision expectation because provenance pruning itself advances convergence.
  Scoped formatting passes; reported diagnostics find no sweep/convergence
  errors, but global compilation and executed behavioural acceptance remain
  pending.
- Keyboard, pointer-edge and atomic pointer-corner resize now validate local
  layout candidates and reuse the existing pixel/share derivation algorithms.
  Pointer observation binding remains distinct from keyboard binding; corners
  normalize horizontal-first and derive the second axis from intermediate
  geometry. The resize commit tail preserves last-active and capacity state
  rather than adopting another operation's pruning rules. Eight direct tests
  port repeat/minimum, focus/overlay, atomicity and revision-limit coverage.
  Source review corrects a test to expect focus mismatch before directional
  exhaustion. Scoped formatting passes; reported diagnostics find no resize
  errors. Caller integration and executed behavioural acceptance remain pending.
- Intentional float/unfloat now operates on one retained candidate, reusing
  admission for explicit unfloat and preserving durable placement, fullscreen
  tolerance, maximized refusal, capacity no-space origin and lifecycle focus/
  provenance cleanup. No observed-focus sync is introduced. Nine direct tests
  cover these transitions, validation priority and revision-bound allocation
  burns. Source review removes domain-key checks absent from float command
  shape validation. Scoped formatting passes; a library-and-tests diagnostic
  still fails on 83 library and 268 test-build errors in unmigrated consumers,
  with no layout-module error. Integration and executed tests remain pending.
- Local directional move and focus now use domain candidates and the existing
  pure planners without inner dispatch. Move preserves R1-R3 tree/share edits
  and no adjacent-output fallback; focus retains accepted-history descent and
  the maximized/fullscreen sibling-skip rule. Separate commit tails preserve
  accepted focus history versus movement pruning. Four direct tests cover
  rule families, same-axis modes, focus/provenance differences, refusal order
  and revision-bound allocation burns. Source review restores the geometry
  leak guard and replaces an ineffective allocator assertion with a direct
  counter check. Scoped formatting passes; reported diagnostics find no new
  directional-module errors. Caller integration and executed tests remain
  pending.
- Local orientation, float, directional and resize operations now separate
  candidate validation from retained installation. Builders take explicit
  admission configuration and counters so relocated and legacy-seeded trials
  can remain uninstalled until successful completion. Existing retained entry
  points preserve their validation order and commit tails. Source review and
  scoped formatting pass; global compilation and executed tests remain pending.
- Typed local entry points now compose retained, unique-source relocation and
  legacy-seed candidates without temporary map installation. Converged refusal
  fences, fresh-floating convergence persistence, legacy default admission
  policy and discarded-trial allocation burns remain distinct. Six direct tests
  cover routing and typed delegation. Review restores unconditional storage of
  scaled outer gaps and makes seed-policy and ambiguous-order tests
  discriminating. Scoped formatting passes; global compilation and runtime
  acceptance remain pending.
- Local world commands now call typed layout operations directly after one
  retained-observation convergence, before ordinary command validation.
  Direction-only move/focus paths carry their already-parsed direction;
  batch focus and release also use layout queries directly. Float placement
  retention, composed-maximize rollback, command sequencing and focus effects
  retain their prior ordering. Two direct world tests cover convergence
  persistence after invalid input and post-inset refusal handling; initial test
  compilation errors are corrected. Scoped formatting passes. A library/tests
  diagnostic still fails on 79 library and 264 test-build errors in unmigrated
  consumers, with no new local-path/test error; runtime acceptance is pending.
- Retained publication now reads per-domain layout bindings, exceptions and
  restore slots directly. Geometry-only projection reuses the existing hinted
  client-clamp projector; key-derived homes, per-domain capacity provenance and
  tile/exception/slot ordering preserve the held and controlled row paths.
  Source review and scoped formatting pass. A reported library diagnostic still
  fails on 75 unmigrated consumers, with no publication-module error; runtime
  acceptance remains pending.
- Sticky transitions now use layout membership, durable float placement and
  revision-neutral slot/exception mutators directly. Explicit sticky unfloat
  shares typed float settlement with ordinary commands while retaining raw
  staged focus and filtered closes from its former boundary. Existing staged
  world adoption, source reflow, exact tiled-outcome checks and frame-token
  retention remain unchanged. One direct mutator test is added but unexecuted.
  Source review and scoped formatting pass; a library/tests diagnostic still
  fails on 59 library and 244 test-build errors in unmigrated consumers, with
  no new helper or test error. Runtime acceptance remains pending.
- Report ingress now updates the layout cursor and derives known-before and
  tiled-before lifetime sets directly from per-key bindings, exceptions and
  restore slots. Post-sweep slot cleanup and three existing test accessors
  use the same owned layout. Source review and scoped formatting pass; no
  additional build or runtime acceptance is claimed.
- Private canonical pair candidates now own only the two affected domain
  candidates and ephemeral revision/focus/id-restoration metadata. Assembly and
  non-consuming atomic installation preserve exact duplicate guards, target
  identity restoration, source-priority orphan/provenance partitioning and
  inherited gaps. Review restores pair-global focus validation and linked-leaf
  uniqueness without over-pruning inactive domain focus or repeated group ids.
  Ten direct tests cover these boundaries but remain unexecuted. Scoped
  formatting passes; a library/tests diagnostic still fails on 54 library and
  235 test-build errors in unmigrated consumers, with no pair/topology error.
  Combined pair convergence and operation callers remain pending.
- Whole-workspace migration now partitions and converges local candidates,
  relocates and validates hinted client-clamp geometry, then installs both
  affected domains atomically. Both partition trials retain the same original
  admission-counter baseline, with their maximum committed only on success;
  source-carried outer gaps and other-side restore slots remain unchanged.
  World migration uses typed results without an event/acknowledgement path.
  Five direct tests are added but unexecuted; four existing test accessors now
  read layout domain counts or tiled/exception membership directly. Source
  review and scoped formatting pass. A library/tests diagnostic still fails
  on 48 library and 224 test-build errors in unmigrated consumers, with no
  new migration-module/test error. Runtime acceptance remains pending.
- Global workspace swaps now prepare both relocated candidates and physical
  sticky stays before an atomic affected-domain installation. Extraction carries
  all per-window provenance; rootless sticky-only hosts, stale empty sources,
  remembered focus and world rekey consequences retain their prior behavior.
  Seven direct tests are added but unexecuted. Source review and scoped
  formatting pass; a library/tests diagnostic still fails on 36 library and
  212 test-build errors in unmigrated consumers, with no swap-module/test error.
  Runtime acceptance remains pending.
- Single-domain and combined pair convergence now share classification, restore,
  admission and retention phases. Pair convergence preserves global ordering,
  evolving cross-domain anchors, remapped remembered focus, shared admission
  counters and global-union revision accounting before atomic installation.
  Review restores the single-domain retained-world anchor and prevents stale
  capacity provenance from returning after a successful retile. Eight direct
  tests are added but unexecuted. Scoped formatting passes; a library/tests
  diagnostic still fails on 36 library and 212 test-build errors in unmigrated
  consumers, with no pair-convergence or shared-phase error. Pair callers and
  runtime acceptance remain pending.
- Directional pair focus and R1-R4 movement now compose typed layout operations,
  reusing local tree edits and installing combined convergence before later
  command refusals. Cross-output capacity fallback preserves untouched frames,
  source-priority provenance and first admission order. Review restores global
  pair pruning and world refusal precedence, removes direction string roundtrips,
  and uses closed assembly failures. Nine direct tests cover these paths;
  initial minimum-hint fixtures are corrected to actual geometric exhaustion.
  Scoped formatting passes; a library/tests diagnostic still fails on 35
  library and 211 test-build errors in unmigrated consumers, with no new pair
  operation/test error. All nine tests and end-to-end acceptance remain pending.
- Explicit workspace/output sends now compose typed source seeding, combined
  convergence and tiled, deferred or capacity-float transfer candidates directly.
  Shared transfer tails retain focus, revision and global provenance semantics.
  Source review preserves explicit-send differences: empty hidden sets, retained
  close slots, ignored empty-target residue, legacy seed configuration and
  distinct focus refusal messages. Same-output capacity fallback replaces prior
  durable geometry; cross-output fallback keeps its untouched-frame semantics.
  Nine direct tests are added but unexecuted. Scoped formatting passes; a direct
  library/tests diagnostic still fails on 24 library and 200 test-build errors
  in remaining consumers, with no new send-module/test error. Runtime acceptance
  remains pending.
- Gesture ingress now calls typed local and occupied-pair preview/drop operations
  and pointer resize directly, retaining lifecycle, capture shields, hover
  identity, transfer settlement and release-frame placement. Pure pointer
  resolution shares the generation allocator; discarded previews install no
  topology. Composed-float moves validate both domains before revision-neutral
  placement installation. Review restores fresh-drag legacy configuration and
  occupied-pair source/target checks before convergence. Nine direct tests cover
  these boundaries but remain unexecuted. Scoped formatting passes; a reported
  library/tests diagnostic still fails on remaining consumers (183 test-build
  errors), with no gesture-module/test error. Public drag-command consumers and
  executed acceptance remain pending.
- Remaining production operation consumers now use typed layout queries,
  reconcile/gap updates and local or occupied-pair drag operations. Obsolete
  event completion and reply interpretation are removed. Public cross-source
  singleton drag fallback retains mixed exception rosters and fresh installation
  without an extra revision. Review corrects unchanged-resolution fallback and
  preserves live exception geometry separately from durable float memory.
  Four focused tests are added but unexecuted. A direct core library check
  passes with 71 warnings from retained obsolete code and unused helpers; a
  reported library/tests check still fails on 174 old test accessors. Machine
  removal, test migration and full runtime acceptance remain pending.
- World tests now inspect layout bindings, focus, revisions, restore slots and
  float provenance directly. Three tests solely planting removed inner pending,
  drag-capture or divergence states are removed; surviving world refusal tests
  retain their product invariants. Raw multi-domain swap planting is replaced
  by real world refusal/recovery and one direct malformed-candidate atomicity
  test. Fresh fixed-window override coverage uses pending intent rather than a
  no-op committed-tile marker. This unit removes three tests and adds one.
  Library/tests compilation passes. Reported core execution has 2,418 passes
  and nine failures (seven in new layout tests, two in world command tests);
  these require source-based triage, not assumed baseline failures. A direct
  swap-test spot check passes all nine tests. Runtime acceptance, old-machine
  removal and final gates remain pending.
- Initial executable coverage now passes all 2,427 core library tests. Triage
  corrects pair revision normalization on unchanged convergence, malformed new
  resize/capacity/overlay fixtures and one obsolete duplicate-inner-owner
  assertion while retaining real ownerless-command coverage. Source inspection
  also corrects the empty-target send expectation: intermediate convergence
  retires the target gap, so final installation inherits the source gap.
  No test count changes in this correction. Old machines and their independent
  test rigs still remain; endpoint removal and full acceptance are pending.
- Old production `Engine`, independent `Session`, `Reconciler`, event/reply
  dispatch and synthetic acknowledgement/post-observation scaffolding are now
  removed. Semantic command/domain/resize/drag vocabulary remains at its public
  import paths. The shared burning allocator and its three tests move into
  layout storage; entry-edge ranking and centered float placement remain pure
  helpers. A direct core library check passes with 21 outstanding unused-code
  warnings; reported core/protocol/service library builds pass.
- Test migration is still required: 204 embedded tests were removed with the
  old machine files, three allocator tests moved and 20 pure tests retained.
  Meaningful layout coverage among the removed tests must be ported from
  `37fb0ade`; obsolete inner-handshake/identity coverage must be distinguished,
  not silently discarded. Surviving old-machine test rigs currently fail
  compilation (140 reported library-test errors), so no post-removal runtime
  acceptance or final count reconciliation is claimed.
- Reported portable Windows library checking passes. Portable macOS host
  checking fails with seven errors in `drag_fixture.rs` and `world_runtime.rs`
  involving command-result and frame-setter vocabulary. Native sources are
  unchanged; baseline failure status has not been verified. These host checks
  do not establish native-target caller compilation.
- Retained convergence, admission, reconciliation, minimize/restore and gap
  repair tests now use typed layout operations. Across 16 test/helper files,
  155 tests become 122: 30 obsolete reconciler-handshake tests and three
  pending cancellation/divergence tests retire with the deleted machines.
  Remaining assertions retain admission order, provenance, refusal atomicity,
  restore-slot cleanup and independent-domain behavior. Typed-ID bounds and
  portable-import checks remain; a direct integration spot check passes both.
  Scoped formatting passes. Internal migrated tests are not yet executed:
  other standalone and integration rigs still prevent full test compilation.
  Removed embedded coverage also still requires semantic reconciliation.
- Remaining internal standalone rigs now compile against typed focus, resize,
  send and gesture operations; their test counts are unchanged. Deleted
  inner-focus capability-handshake assertions retire, while ranking, fallback,
  topology, geometry and atomicity checks remain. A direct keyboard-resize
  spot check passes all seven tests. Reported full core library execution has
  2,196 passes and 24 failures after test migration. These failures require
  source-based triage of migrated setup and operation boundaries; they are not
  established baseline failures. External old-machine rigs, meaningful removed
  embedded coverage and final acceptance remain pending.
- Source-based triage resolves all 24 failures as migrated test setup or
  boundary errors, with no production changes or test-count reduction.
  Old engine tests now include preceding observation convergence; hinted
  geometry tests retain equal-share rather than fitted-share setup. Unknown
  target refusals exercise the world-owned catalog. Stale inactive-domain
  focus history is checked through combined pair convergence, matching the
  old multi-domain context rather than isolated target convergence.
  Direct core library execution passes all 2,220 tests. This count reflects
  removal of 204 embedded tests still awaiting coverage reconciliation and
  three retired pending-state tests; it is not full-suite acceptance.
