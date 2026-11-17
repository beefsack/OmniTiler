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
