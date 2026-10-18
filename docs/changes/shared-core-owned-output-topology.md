# Shared-core proposal: core-owned output topology

- Status: proposal ready for cross-stream review, 2026-10-18. Documentation
  only; implementation and native acceptance remain pending.
- User direction 2026-10-18: core owns monitor positions in its world state
  and derives output adjacency. Adapters supply positions in complete world
  reports, which already provide initialization and subsequent updates. This
  is a critical part of the new core design, not Windows-specific mapping.
- Goal: one system-agnostic topology authority and one shared selector per
  use, per [Layering and Simplicity](../principles.md). No adapter-owned
  adjacency database, topology initialization RPC, polling or new setting.
- Non-goals: new selection semantics without an explicit decision, native
  capability expansion, float-remap policy, workspace-scope redesign, or
  enabling Windows runtime adoption or expanding macOS runtime capabilities.
  macOS full-world adoption landed separately in `182ba9a6`, offline only;
  [native acceptance remains pending](macos-full-state-replies.md#pending-live-verification).

## 1. Verified current state

| Source | Current contract / limitation |
| --- | --- |
| `crates/tiler-core/src/world_report.rs:383-396` | `OutputTopology` carries output id, native order/current view, completeness, optional `geometry` and primary. Geometry is documented for hotplug nearest-survivor placement, not general directional planning. |
| `crates/tiler-core/src/world_displacement.rs:119-170` | Core already retains `OutputFacts { rect, primary }` on complete non-shared displacement passes. This is not a universal live topology authority: shared mode returns before retention. |
| `crates/tiler-core/src/session/world.rs:21-55`; `session.rs:1914-1947`; `directional.rs:147-152,1075-1107` | `OutputDomain.adjacent` and directional `Output.adjacent` carry adapter-resolved neighbors. Session/pair validation requires known, non-self, reciprocal endpoints; R4 consumes the named pair rather than selecting from full rectangles. |
| `crates/tiler-core/src/world_commands.rs:347-364,996-1035,1414-1457` | World migration and directional focus/move resolve through reported domain adjacency; migration refuses without it even when its explicit command names both outputs. Directional target workspace resolution is separate from output selection. |
| `crates/tiler-core/src/output_selection.rs:1-24,73-163,202-267` | Shared pure FULL-rectangle candidate/ranking rules already exist: exact edge-touch, positive perpendicular overlap, no wrap; window-position and whole-workspace migration rank differently. |
| KDE: `kwin/src/world-observe.ts:272-349,634-686,744-768` | KWin computes unique-reciprocal adjacency from FULL Output `geometry`, copies it into each domain, and also sends the same FULL rectangle as topology `rect`. Domain bounds are separate per-desktop `clientArea` work areas. |
| Windows: `crates/tiler-windows/src/tiling_sys/world_capture.rs:665-681,721-754` | Offline world capture sends FULL `MonitorArea.full` in topology geometry, raw work-area domain bounds plus outer gap, and empty adjacency. This blocks real world migration/refill acceptance; [W2 remains parked](windows-full-world-state-replies.md#w2-design-first-outcome-parked). |
| Windows legacy: `crates/tiler-windows/src/workspace_owner/directional.rs:47-94`; `migration.rs:198-229` | Native topology is read adapter-side; target selection already calls shared core `select_for_window` / `select_for_migration`, then attaches a reciprocal resolved pair. No Windows world adjacency mapping exists or should be added. |
| macOS: `crates/tiler-macos/src/world_builder.rs:172-260`; `runtime_types.rs:154-179`; `topology.rs:124-195` | Full-world adoption is delivered offline in `182ba9a6`, live pending. World domains send raw visible/menu-Dock-excluded work areas with outer gap separately, and empty adjacency. Topology geometry currently also sends `domain.visible`, not FULL bounds. `NativeDomain.bounds` already retains the FULL converted `cg_bounds`; use it for topology without changing work-area placement. |

### Selection rules are not interchangeable today

| Consumer | Verified rule today / proposal boundary |
| --- | --- |
| KDE world directional focus, R4 move and migration | All consume the unique-reciprocal map from `world-observe.ts`. Multiple forward touches or non-unique reverse touches omit the edge. Moving that derivation into core can preserve this exactly. |
| Legacy KDE focus | `kwin/src/plan-adapter-entry.ts:736-800` retains unique-reciprocal selection. Do not silently replace focus with migration or window ranking. |
| Legacy windowed move and directional output send | Core `select_for_window`: moving frame centre projection on shared edge first, otherwise largest window-span overlap, then smallest x/y/id. Half-open centre containment preserves odd half-unit centres. KDE mirrors it in `plan-adapter-entry.ts:802-919`; Windows calls it from `workspace_owner/directional.rs`. Reverse uniqueness is deliberately not required. |
| Legacy whole-workspace migration | `select_for_migration`: largest FULL shared edge, then smallest x/y/id; never window position. KDE mirror: `plan-adapter-entry.ts:920-942`; Windows wrapper: `workspace_owner/migration.rs:204-229`. |
| Windows legacy directional focus | `workspace_owner/directional.rs:4-21,47-79` uses the window selector too. This differs from KDE focus; ownership extraction must not silently standardize it. |
| Explicit world sends | Endpoints are already named; `session.rs:999-1013` skips the reciprocal-adjacency fence for explicit sends. Directional command resolution, if needed, must select in core; do not impose adjacency on arbitrary named sends. |
| Pointer/drag and hotplug placement | Pointer containment over domain bounds (`world_commands.rs:1325-1342`) and nearest-survivor placement (`workspace_scope.rs:184-255`) are distinct policies, not directional adjacency. Keep them distinct. |

- A single `Direction -> OutputId` map cannot represent window-dependent
  choices when an edge touches several outputs. Nor can independently ranked
  forward maps necessarily satisfy legacy reciprocal-map validation. Retain
  candidates in core and construct a validated reciprocal *operation pair*
  internally after selection; do not mistake reverse uniqueness for pair safety.
- The [split-edge discriminator](../spec/reference-outcomes.md#core-owned-topology-split-edge-discriminator)
  makes the world/legacy mismatch explicit. No native result is inferred.

## 2. Proposed minimal core change

1. Make FULL output geometry required for every output in a report claiming
   complete topology. Reuse `OutputTopology.geometry` / wire `rect`; document
   its expanded meaning rather than add a second full-geometry field. Keep
   `None`/`null` representable for explicitly unreadable/partial observations,
   never as permission to infer geometry from a client or work area.
   Domain `bounds` remains the distinct work-area field, with current
   per-domain outer-gap handling unchanged. KDE and Windows already send
   FULL geometry; macOS must replace its world topology's current `visible`
   rectangle with the already-available FULL `NativeDomain.bounds`.
2. Retain validated live output ids, FULL rectangles and completeness in
   `WorldEngine` for every workspace mode, initialized/replaced by accepted
   complete reports before command planning. Reuse existing facts where
   sensible, but do not tie live topology to hotplug displacement or preserve
   stale live edges merely because disconnect-time facts remain for return.
   Accepted report ordering, staged commit, duplicate replay and generation
   reset remain authoritative; no separate adapter topology update protocol.
3. Derive candidates from the retained FULL rectangles with the existing
   `output_selection` geometry predicate. Put unique-reciprocal derivation
   beside those helpers for consumers that retain that policy. Rank per use,
   not once globally. Internal planning may still construct adjacency maps
   for existing Session/R4 contracts; these are core-derived, not host facts.
4. Remove dependence on carried adjacency from `start_migration`,
   `operation_migration`, `directional_pair_for` and any directional
   send-to-output resolution. Explicit named endpoints keep their current
   command eligibility rules; do not turn a named send into an edge-only send.
   Keep local R1-R3 precedence, target-workspace resolution, scope/hold,
   arrival, safety and capability gates unchanged.
5. Extract KDE's existing world map first with parity fixtures. Aligning world
   migration with the already-selected largest-edge rule, and world R4 moves
   with the window rule, is an explicit compatibility decision below: it
   changes current KDE world behavior on split edges, not ordinary two-output
   edge-touch behavior. Do not land that change disguised as a refactor.

### Adapter-field and legacy transition

- Recommend ignore-then-remove at the **world boundary**: initially accept the
  old adjacency field for codec compatibility but never use it as authority;
  core derives even when it is empty or contradictory. Remove KDE production
  calculation/emission in the coordinated core+KDE delivery, then remove the
  deprecated wire field once consumers/fixtures agree. Do not add a platform
  opt-in or use presence of adjacency to select competing authorities.
- The legacy per-domain API also carries adjacency (`OutputDomain` and
  directional snapshots), but lacks a complete world topology. Keep it
  transitional, with existing calls into core's pure geometry selectors; do
  not create a second retained topology model or expand its protocol just for
  Windows/macOS. Geometry-based derivation belongs in the shared helpers and
  the world owner. Remove adapter pair glue when each platform cuts over;
  internal reciprocal pair validation remains useful and need not be removed.

## 3. Platform impact and risks

| Platform / area | Impact and required safeguard |
| --- | --- |
| KDE | Delete script-owned world adjacency after parity proof; retain FULL geometry and work areas. Ordinary two-output focus, R4 move, send, migration/refill and hotplug behavior must remain unchanged. Recheck native two-output behavior: offline map equality alone does not prove settlement, gaps or focus. Split-edge alignment requires the explicit decision below. |
| Windows | Keep the working legacy runtime and W2 prototype parked until the shared change lands. Resume world migration/refill capture and re-observation using existing FULL reports, with **no Windows adjacency implementation**. Configuration acceptance and cross-output capacity-float refusal remain separate decisions. |
| macOS | Update the delivered world builder to report FULL `NativeDomain.bounds`, independently of visible work areas; retain display identity/UUID homing, AX/AppKit threading and native Space capability gates. Display arrangement must be verified, not inferred from Space order or focused display. This corrects the geometry carrier without reopening world adoption or enabling sends. |
| Coordinates / scale | One coherent coordinate space within each adapter report for FULL rects, work areas, window frames and pointer facts. Windows physical pixels, KWin host geometry units and macOS CG/AX top-left points need not share a universal unit. Never independently multiply mixed-scale monitor origins/extents or subtract panels before adjacency. macOS conversion is `primary_h - (y + h)`, with negative origins preserved (`topology.rs:149-195`); integer conversion rounds 1:1 and refuses invalid/unrepresentable geometry (`:64-101`). Check rounding at shared edges. |
| Gaps / non-touching / mirrored outputs | Preserve exact edge-touch, positive overlap and no wrap. Real gaps and corner-only contact produce no directional candidate; do not invent nearest-neighbor or tolerance rules. Overlapping/mirrored arrangements need deterministic fixtures, not accidental order dependence. |
| Hotplug / partial enumeration | Missing/invalid FULL rects, duplicate ids or incomplete topology must refuse topology-dependent crossing, migration and remap narrowly; never derive from the readable subset or treat an omitted output as unplugged. Retain disconnect facts separately, no stale-edge actuation. Unaffected local work and later complete-report recovery remain available. Existing displacement already fences incomplete inventory; distinguish monitor completeness from unrelated window-read failures before broadening holds. |

- Accepted constructor prerequisite (T1, 2026-10-18): `0869f08c` adds
  `WorldEngine::new_with_config(&str, WorldEngineConfig) -> Option<Self>`;
  Windows enables automatic fixed-float sends and disables the core maximized
  focus fence. Existing constructors/defaults and KDE/macOS behavior are
  unchanged. This API is already delivered, not part of the proposed topology
  delta; [configuration evidence](windows-full-world-state-replies.md).

## 4. Acceptance and evidence plan

- Core derivation tests: horizontal offsets/negative origins, unequal heights,
  vertical stacking, normalized mixed-scale arrangements, full vs panel-work
  rectangles, gaps/non-touching/corner-only contact, multi-candidate edges,
  reverse ambiguity, equal-edge and x/y/id ties, odd window centres,
  overflow/invalid rectangles and duplicates. Exercise unique-reciprocal,
  migration and window selectors separately, not one expected universal map.
- World tests: first complete report initializes; moved monitor positions update
  before a same-report command; shared/local/global-unique modes; adapter
  adjacency empty/wrong cannot affect selection; partial/missing rect refuses
  without stale topology use; hotplug/disconnect-return, generation reset,
  rejected/duplicate reports and in-flight transfers preserve existing fences.
- KDE parity: feed the same real-shaped FULL-geometry fixtures into current
  `world-observe.ts::computeAdjacency` and core derivation, including ambiguous
  edges; require map equality for the extraction. Any approved policy-alignment
  delta needs separately named expected-result tests, never a blanket parity
  claim. Keep work-area/outer-gap and native-property fixtures.
- Adapter offline tests: KDE producer/codec/replay without authoritative
  adjacency; Windows actual W1 capture -> configured WorldEngine -> migration/
  refill feedback/re-observation with empty adapter adjacency; macOS builder
  FULL-vs-visible topology fixtures, UUID/full vs visible conversion,
  negative/above-primary origins, scale-independent
  points, missing primary and separate-Spaces ambiguity. No mock adjacency
  injection to manufacture a Windows acceptance result.
- Future live journeys: KDE two-output focus/move and follow/stay send both
  ways, workspace migration/refill, panels, rearrangement and unplug/return;
  Windows equivalent after world cutover, mixed DPI and capacity-float refusal;
  macOS display arrangement/menu-Dock changes and supported focus/Space routes,
  with unsupported transfers still refused. Follow each platform's
  `docs/live-*-testing.md` protocol; physical input/visual/game outcomes remain
  separate from offline proof. No live testing occurred for this proposal.
- Documentation acceptance: source claims reviewed, relative links and
  `git diff --check` pass; no code behavior changed. Implementation/native
  acceptance remains pending, so this proposal stays active rather than archived.

## 5. Open questions and recommendations

| Question | Recommendation |
| --- | --- |
| Remove adjacency immediately or transition? | Ignore-then-remove at the world boundary, with coordinated producer/codec cleanup. Never fall back to adapter adjacency. Keep internal/legacy pair fields until no longer needed. |
| Preserve world strict maps or restore per-use selection on split edges? | First prove strict-map extraction parity. Then explicitly review world migration -> existing largest-edge rule and R4 move/directional output send -> existing window rule, preserving unique-reciprocal KDE focus. This reconciles already-selected legacy behavior but changes current world split-edge behavior. Windows focus parity must remain explicit; do not silently impose KDE focus there. |
| Must a complete report with no FULL rect be rejected wholesale? | Require rects for complete topology; partial/null remains expressible and narrowly holds topology-dependent operations. Decide validation placement with codec/core tests, not a global loss of unrelated tiling. No extra work-area field is needed: domain bounds already supply it. |
| How should mixed-scale rounding or host arrangement gaps behave? | Normalize once at the adapter boundary, preserve exact-touch/no-wrap semantics. If host evidence cannot represent touching edges consistently, record a minimal native discriminator and leave tolerance/gap-jump behavior TBD rather than guessing. |

## 6. Suggested ownership and order

1. KDE stream reviews the semantic split above and delivers core topology
   retention/derivation, core/protocol fixtures and KDE producer removal
   together. Approve any split-edge policy alignment explicitly; ordinary
   two-output parity is mandatory. No Windows topology implementation needed.
2. Windows stream resumes the [parked W2 handover](windows-full-world-state-replies.md#blocker-preservation-and-exact-next-action)
   after shared delivery, proves real migration/refill roundtrips and reconciles
   existing native-gate failures before acceptance; W3 remains an atomic cutover.
3. macOS stream updates its delivered world builder to FULL topology and consumes
   shared derivation, verifies display arrangement, and retains unsupported
   native-Space/transfer refusals. Existing offline adoption is not reopened.

- Exact next action: route this proposal to KDE and macOS streams; KDE reviews
  the per-use selection decision and starts the core+KDE offline unit. Windows
  full-world adoption stays parked pending that shared topology delivery.
