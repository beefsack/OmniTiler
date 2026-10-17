# Best-effort admission when space runs out

- Status: source investigation/proposal delivered, 2026-10-17; product choices OPEN.
- Goal: replace geometric-capacity holds with deterministic best effort without
  degenerate native rectangles, implicit workspace changes or repeated effects.
- Direction: user 2026-10-16 requests anchor fallback and float/overlap settings,
  prefers float, and rejects automatically opening on another workspace.
- Scope: proposal and discriminating specification rows only. No implementation,
  settings, dependency changes or live testing. Existing selections remain in force
  until the choices below are accepted.

## Plan and acceptance

1. Audit shared projection/admission and KDE, Windows and macOS consumption.
2. Specify candidate semantics, defaults, edge cases and selection consequences.
3. Add OPEN requirements/action sequences, preserving established reference evidence.
4. Check source locators, links, requirement counts and documentation diff.

- Acceptance: every requested interaction has an explicit recommendation or TBD;
  choices are distinguishable by short action sequences; no proposed behavior is
  mislabeled selected, implemented or native-verified.

## Current behavior and evidence

- Locators below were checked against `b244646b`; source evidence, not native
  acceptance. Tier 1 evidence remains
  [R-MIN-04](../spec/reference-outcomes/minimum-size.md#r-min-04-pure-geometry-exhaustion)
  and [R-INS-09](../spec/reference-outcomes/insertion.md#r-ins-09-burst-of-maps-versus-roster-delivered-together):
  splitting at the anchor, not alternate-target search or capacity floating.

| Surface | Current behavior / locator |
| --- | --- |
| Strict geometry | `crates/tiler-core/src/geometry.rs:143-164,220-239`: positive bounds and every group must satisfy `extent - gap*(children-1) >= children`; reserve one coordinate unit per child. This is topology-dependent, not a window-count limit. |
| Declared minima (B6) | `crates/tiler-core/src/size_hints.rs:524-565,615-623`: strict geometric validation runs first; feasible minimums borrow sibling slack; infeasible minimums flag proportional tiles rather than refuse. KDE `kwin/src/plan-adapter.ts:1279-1294,4424-4446` (`overconstrainedEffective` / `effectiveTargetFor`) and Windows `crates/tiler-windows/src/tiling/geometry.rs:227-248` raise native extents at the tile origin, permitting overlap/overflow. |
| Ordinary admission | `crates/tiler-core/src/session/ops/lifecycle.rs:363-425,464-484`: wrap eligible focus, project, reject without staging if impossible; successful tiled admission targets newcomer focus. `session.rs:2758-2794`: old/new equal binary wrapper; no eligible focus wraps the whole root. `cosmic_v1.rs:63-85`: width > height means horizontal, otherwise vertical; shares `[1,1]`. |
| Full-world hold | `crates/tiler-core/src/world_publish.rs:26-55,232-260,339-359`: the narrow sweep classifier maps geometric `malformed-topology`/`seed-failed` to recoverable `constraint-clamp`, retains topology and emits complete held lifetimes. Other failures stay unreadable. Do not globally reinterpret every `MalformedTopology` as capacity. |
| KDE | `kwin/src/world-apply.ts:311-332,348-350,409-411`: only controlled, unheld frames can write; overlays/exclusions/floats are guarded. Capacity hold is not a float classification. Existing B6 legacy actuation is not evidence that the new world path has native B6 acceptance. |
| Windows | `crates/tiler-windows/src/tiling_sys/admission.rs:594-604` dispatches shared Engine; `:654-677` retains rejected close facts. `tiling_sys/geometry.rs:121-148` logs rejection/invalid snapshot, stages no plan and returns without geometry writes. A newcomer left at its native frame can look floating, but this path does not admit a capacity float. Automatic fixed-size floats are a different, real feature. |
| Fixed automatic floats | `crates/tiler-core/src/session/ops/lifecycle.rs:268-354`: fixed-hint admission stages an automatic membership-only float, no frame/focus writes; later hint changes do not reclassify. `session/ops/float.rs:81-99,128-133`: explicit tile override stages transactionally after successful admission proposal, never on refusal; intentional float withdraws fixed provenance transactionally. Do not reuse the fixed-size marker for capacity. |
| macOS | `crates/tiler-macos/src/runtime_core.rs:33-59,72-124`: retained Session distinguishes placed intentional floats and automatic fixed floats; AX has no native float flag, so normalized observations must echo retained classification. A new placement-less capacity float needs an explicit echo or it can accidentally re-tile. Windows/macOS still use transitional domain APIs, not the KDE full-world boundary (separate adoption items). |
| Startup/enable | `crates/tiler-core/src/engine.rs:1471-1655`: clean recursive-cut fit before sequential seed. `seed.rs:360-409,562-570`: spatial `(y,x,h,w)` order, focused member last, stable adapter order on ties; each step uses ordinary admission. Failed whole seeds currently produce a hold, not a useful partial roster. |
| Gap/bounds changes | `crates/tiler-core/src/session/world.rs:223-257` preserves topology, exceptions and focus, atomically rejects invalid updates. `engine.rs:2256-2274` refuses failed retained projection without commit. No capacity-eviction/recovery policy exists. |

- Axis-input subtlety: fresh/seed/unfloat use a **plain-projected** anchor via
  `crates/tiler-core/src/session.rs:2743-2755` and `seed.rs:411-431`; workspace
  sends use a **hinted-projected** anchor via `session/ops/workspace.rs:216-269`.
  Preserve ordinary successful routes in this item; do not silently normalize
  their possibly different long edges.
- Selected policies to preserve unless explicitly superseded: B6, clean-fit
  startup, D03 after-focused insertion, D16 float carry, D23 minimize slots,
  R4 whole-target wrap and native safety. See
  [placement/minimum/startup](../decisions.md#placement-minimums-and-startup-adoption).

## Decisions needed

- Every recommendation below is **unselected** unless listed here. D1 is
  especially material: default capacity floating alone does **not**
  eliminate B6 minimum overlap.
- User selections 2026-10-17:
  - D1: geometric exhaustion only; B6 minimum overlap stays, with a UX
    follow-up on B6 overlap as part of dogfooding.
  - D2: default anchor fallback is largest splittable window.
  - D3: default no-space result is float; overlap remains the alternative
    setting value.
  - D4: a capacity float keeps the frame it opened with (eviction keeps the
    last live frame). The `capacity` origin is provenance for diagnostics
    only; functionally the window is treated exactly like a normal float, with
    no capacity-specific behavior. The new window is focused whether it tiles
    or capacity-floats.

| ID | Options and consequences | Recommendation |
| --- | --- | --- |
| D1: What counts as no space? | **Geometric only:** preserve B6 and resolve the reported hold; minimum-infeasible but positive tiles can still overlap. **Include declared minima:** float/search before B6 overlap too, changing REQ-MIN-01..03 and startup minimum behavior; sole oversized clients still cannot be made to fit by floating. | Geometric only for this change; explicitly confirm the remaining B6 overlap. If the goal is to remove that too, select minimum-aware admission as a separate expansion before implementation, not an inferred preference. |
| D2: Anchor fallback and its default | **Largest splittable window:** most usable remaining area, but placement may jump to another branch. **Closest splittable relative:** local ancestry first, potentially much smaller tiles. **Do nothing:** no search, go directly to float/overlap even if another tile could split. Relative can mean leaf search or wrapping an ancestor group; the latter changes an entire subtree. | Default **largest splittable window**. Define relative as the leaf-ring search below, not parent wrapping; no alternate axis, rebalancing, output search or workspace spill. |
| D3: No-split result and its default | **Float it:** preserve established tiling, create a separately navigable float; no guarantee it will not visually cover tiles. **Allow overlap:** keep tiled membership/topology but accept overflowing geometry and less predictable directional navigation. Dropping/refusing the newcomer leaves the original UX failure. | Default **float it**; keep overlap as the requested explicit alternative, with D9's bounded meaning. "Do nothing" in D2 feeds this setting, not an admission rejection. |
| D4: Float frame, provenance and focus | **Keep native frame:** least interference and no geometry retry. **Restore/center:** more consistent float size, but new writes/activation hazards. **User float marker:** fits existing plumbing but loses cause. **Dedicated capacity origin:** truthful diagnostics and future policy, requires all adapters to retain/echo it. | Preserve fresh observed frame; on eviction preserve last live frame. Dedicated `capacity` automatic origin, distinct from `fixed-size` and user intent. Never focus the alternate anchor; tiled success retains newcomer-focus semantics, floating leaves native birth activation alone and keeps prior tiled focus. |
| D5: Recovery and lifetime | **Retry automatically** on close/grow/gap decrease/visit: fills space, but can churn layout/focus and race live user changes. **Explicit retry only:** stable floats can remain after capacity returns. **Persist reason** across restarts: extra identity-safe storage; **session-local reason:** restart is fresh adoption and can classify differently. | No automatic re-tile. Explicit tile/unfloat retries current admission once; failure stays floated, with no successful-tile override. No retries on replies, capacity return, workspace visits or ordinary disable/enable. Keep reason for the live lifetime/domain moves; no new cross-restart persistence in this item. Existing user-float persistence is not relabeled capacity memory. |
| D6: Which operations? | **New maps only:** narrow but sends/startup can still hold. **All capacity-producing routes:** consistent availability, but explicit moves must preserve their promised target/shape rather than search elsewhere. | Include serial startup/enable seed, fresh/rejoin fallback admission, explicit tile, sends, R4 and drag, with route-specific rules below. Existing successful routes unchanged. No automatic workspace/output relocation. |
| D7: Existing tiles become geometrically infeasible | **Keep constraint hold:** safest but leaves an entire readable domain inert after shrink/gap changes. **Float deterministic victims:** useful partial layout, but existing windows leave tiling. **Reseed/rebalance:** changes established topology and placement. | Float the shortest newest-first victim prefix that makes the retained tree project, contracting removed branches with existing survivor-share rules. Protect fullscreen/maximized reserved slots, fixed explicit-tile overrides and unsafe/excluded members; defer during gestures. If the protected remainder or usable bounds cannot project, retain a truthful narrow hold, not fabricated rectangles. |
| D8: Startup and coalesced admission order | **Preserve clean fit then serial seed:** selected adoption survives; deterministic partial success, possible early capacity floats. **Rebalance roster:** might fit more windows but reverses selected startup policy. **Arrival-order batch policy:** platform-dependent and not established by the references. | Preserve clean/tolerance-valid minimum-feasible recursive-cut adoption first, including enable's existing fit rules. If declined, reuse spatial/focus-last seed and stable adapter-order ties, committing all tile/float classifications atomically as one accepted roster. Do not claim burst/native-order parity. |
| D9: Meaning of geometric overlap | **Positive overflowing projection:** keep hierarchy/order/shares, expand an infeasible group's effective extent to its gap-plus-one-pixel floor without expanding the parent's assigned slot; descendants can overlap other branches/overflow the domain. **Zero/negative tiles:** invalid host requests. **Stack a newcomer over an unchanged tile:** requires a new capacity-stack allocation/navigation model. | First option, explicit overlap-only projector; keep strict projection unchanged. This is a host-limitation deviation, not native-tiler parity. It needs golden geometry/navigation cases before coding; never pretend B6 alone solves zero-hint exhaustion. |
| D10: Settings boundary, placement and timing | **Shared global settings:** consistent portable policy, native storage/controls. **Per-workspace overrides:** more state and precedence not requested. **Immediate reclassification on Apply:** surprising reflows; **next decision only:** existing settled floats/tiles stay stable. | Two portable global enum fields in shared catalog, Tiling & Workspaces > Admission, with capacity help. Apply to subsequent admission or actual capacity-repair decisions only, no tree rebuild/retry just because settings changed. Native capability gating and saved/requested/confirmed state remain separate. |

## Candidate semantics (if recommendations are selected)

### Splittable, ordering and split

- Scope is one readable tiling domain with valid positive usable bounds and gap.
  Repair an already geometrically infeasible retained tree under D7 before
  admitting newcomers. Invalid topology, incomplete observations, feedback
  ambiguity and unsafe native state remain holds/refusals, not capacity evidence.
- Try the ordinary anchor and ordinary topology first. A candidate window is
  splittable only when inserting the equal old/new wrapper on its long edge
  passes **whole-tree strict and normal hinted projection**, with a positive
  rectangle for every tiled lifetime and complete coverage. `overconstrained`
  minimum flags alone do not disqualify it under D1. For an isolated binary
  split this requires axis extent `>= gap+2` and cross extent `>=1`; that local
  test alone is not the general oracle.
- Rank fallback candidates by their current plain-projected allocation, not
  native minimum-expanded/fullscreen frames. Eligible targets are tiled leaves
  in that domain, including reserved overlay slots where the existing safety
  contract permits splitting behind them; never floats, minimized slots or
  fit-excluded/unreadable clients. Native overlay writes remain forbidden.
- Largest: descending `width*height` using a wide integer; ties by ascending
  depth-first child-order leaf index, then stable window-lifetime identity.
- Closest relative: ascend from anchor one ancestor at a time; at each ancestor
  search leaves of sibling subtrees, excluding the child subtree just visited.
  Rank sibling subtrees by absolute child-index distance, then ascending child
  index; visit leaves within each subtree depth-first in child order. First
  splittable leaf wins. "Closest" means topology, not screen distance/MRU;
  ancestor groups themselves are not targets. Missing ordinary anchor uses
  the existing root fallback first, then the same depth-first leaf order.
- Candidate fallback axis: horizontal if its plain allocation is wider than
  tall, vertical otherwise (square tie vertical). Insert old then new with
  equal shares. Never try its short edge, borrow ancestor space, wrap a
  relative group or rebalance as a hidden second attempt. Successful ordinary
  send/admit axis inputs remain as audited above.
- No candidate, or D2 do-nothing, means **policy exhaustion**, not a proof that
  no possible tiling exists. D3 then applies on the same output/workspace.

### Float, recovery and existing-domain repair

- Fresh capacity float: membership recorded, no tile slot, no allocation/frame
  authority, unchanged observed frame/position, ordinary float decoration and
  navigation semantics; no forced always-on-top, raise, centering or resize.
  Automatic origin is core-owned state carried/echoed by every adapter; merely
  adding a world hold classifier is insufficient for transitional consumers.
- Evicted tile: retain last trustworthy native frame, remove only its tiled
  link, record capacity origin. Surviving branches contract without reseeding.
  Track first **successful tiled-admission sequence** for victim order (new
  core-owned ordering state); newest first, stable lifetime identity descending
  for ties. A clean-fit roster shares one initial sequence, so identity breaks
  its ties. Preserve first sequence on moves/re-tile for the same lifetime.
  Re-observation, focus,
  repeated replies and failed admissions never refresh that order. Evict the
  first projecting prefix, not a global maximum-retention optimizer.
- No native focus setter for shrink repair. Preserve observed active window
  even if floated; if it had tiled focus, retain a surviving prior tiled MRU
  internally (or none). Never move focus to a hidden workspace to repair layout.
- Explicit tile retries once using ordinary anchor/fallback policy. If D3
  floats again, retain capacity status and frame, report no-space, no toggle
  loop or false tile override. Successful tile clears capacity origin. User
  intentional-float action takes ownership under existing user-float semantics;
  ordinary native movement/resizing alone does not trigger admission.
- Close/identity replacement clears origin/order. Explicit float send carries
  origin without re-tiling; return/growth/gap reduction does not restore an old
  tile slot. Minimize/restore preserves float status without creating a D23
  tiled slot. Ordinary disable/enable does not clear live capacity exceptions;
  a fresh restart can lose the session-local reason and run fresh adoption.
- D7 protects overlays and explicit fixed-tile safety from capacity eviction;
  never classify a game, stubborn resize or missing hint as exhaustion. A
  protected-only impossible tree, nonpositive usable area or unsafe gesture
  remains held. Geometric repair is not permission to clear fullscreen,
  maximize, fixed overrides, minimize, styles or native safety guards.

### Operation boundaries

| Trigger | Recommended capacity result |
| --- | --- |
| Fresh map / serial seed / explicit tile | Ordinary anchor first, D2 fallback, then D3. Preserve ineligible fixed/transient/game lanes before capacity policy. Failed explicit tile stays float rather than toggling twice. |
| Startup/enable | Preserve clean fit before serial seed. Finish with a complete mixed tiled/float roster instead of discarding the whole seed at the first exhausted member; preserve existing intentional/fixed/capacity exceptions. No balanced optimizer. |
| Explicit workspace send of a tiled member | Keep named destination, remembered/MRU anchor chain and follow/stay. If it cannot split, D2 may find another leaf **within that destination**; D3 may float mover there. Commit source removal only with existing transfer/arrival proof; failure retains source. Sending an already floated member retains float. This does not authorize automatic spill or macOS native-Space transfer. |
| R4 cross-output move | Preserve whole-target directional wrap (not alternate leaf admission). If that exact target shape fails, apply D3 to mover at the explicitly chosen output; no D2 search that would violate R4. Preserve source until transaction/arrival succeeds. |
| Drag/drop | Preserve resolved target/side, no D2 redirection. Preview must explicitly show D3 float/overlap and finish must use the same fresh-validated result. Float keeps release frame, not a surprise tile snap. Until an honest fallback preview is available, refuse/snap back under existing contract; no hidden finish-only float. |
| Cross-output float fallback frame | Use the route's established native float/move-only frame remap to the named output, once with arrival proof. If no safe remap exists, refuse/hold that transfer, not a guessed center or off-screen success. Same-output send preserves exact frame. |
| Gaps/output shrink, whole-workspace migration or rejoin restoring an old slot | First try the promised retained geometry/topology. On proven geometric failure, D7 under float mode; under overlap use D9. Do not silently choose a new admission anchor instead of a promised old slot. Explicit fresh-admission fallback may use D2/D3. |

### Overlap and full-state invariants

- D9 overlap mode uses positive effective targets: at each group, allocate with
  `effective_extent=max(assigned_extent, gap*(children-1)+children)` and existing
  reserve-one/share rounding from the assigned origin; cross extent unchanged.
  The parent slot is not enlarged. Apply recursively, then declared minimum
  origin expansion under B6. Flag capacity overflow separately from minimum
  overconstraint. No negative rectangles, silent gap reduction or global scale.
- Checked arithmetic and existing geometry/transport bounds still apply; if
  effective targets cannot be represented, keep a truthful hold. This is not
  authorization for unbounded off-screen writes. Overlap mode can keep even
  unusably tiny tiled allocations; the float default avoids that policy.
- Admit/evict/float classification, order, tree contraction and any destination
  membership are transactional core state, not an adapter reacting to a hold.
  Full replies retain every live lifetime with explicit tile, capacity-float or
  held status; floats have no phantom tile allocation. Required provenance must
  not be encoded as the existing `fixed_auto` flag.
- Identical accepted facts/settings replay the same classification, targets and
  operation identities. No new focus/raise, frame reset, transfer, retry or
  re-tile on repeated replies; preserve revision/watermark/lifetime fencing and
  refused/ambiguous-property settlement. Real new capacity input can cause one
  repair, not an idle audit or periodic retry.

## Settings proposal

- Proposed functional labels/tokens (not existing APIs):
  - `When the anchor cannot split`: `largest-splittable-window` (default),
    `closest-splittable-relative`, `no-search`.
  - `When no split is available`: `float` (default), `allow-overlap`.
- Help must say: positive extent after gaps, declared-minimum B6 overlap remains
  under D1, search never leaves the domain, no-search also invokes the second
  setting, capacity floats stay floating until explicitly tiled. No WM-branded
  labels: this is an explicit host-limitation deviation from the tier rule.
- Extend the existing core `tiling.admission` catalog section
  (`crates/tiler-core/src/settings_catalog.rs:61-68,101-105`). KDE native controls
  follow the approved checked-in descriptor/exporter direction; Windows consumes
  shared definitions, preserving additive schema-v1 native mappings and strict
  invalid/last-good behavior. KDE retains tolerant fallback. No generic setter
  invokes native remediation. Do not invent macOS persistence/UI before its
  consumer exists; core policy and truthful lifetime echo still need parity.
- Missing values use selected defaults only after approval. Exact stable field
  IDs/native storage keys remain implementation naming, not renamed old keys.

## Verification and risks

- Proposed implementation evidence: table-driven geometric/axis/ordering tests
  (including square/equal-area ties), B6-only fixtures, 800x600 gap-8 seed, mixed
  roster atomicity, shrink protected survivors and victim order, stable replay,
  close/identity cleanup, explicit retry, refused transfers, drag preview/finish,
  and catalog decode/default/timing parity. No universal 13/14-window cap.
- Native acceptance remains TBD for all fallback frames, focus, float navigation,
  resize/shrink, explicit transfers and preview. No live testing performed here.
- Main risks: retaining B6 overlap may undershoot user intent; overlap projection
  deliberately violates containment; newest-first eviction can remove more tiles
  than an optimizer; session-local reason can change on restart; full-world-only
  delivery would strand Windows/macOS; protected extreme geometries still hold.
- Documentation evidence/outcome: source audit and independent contract review
  accepted; cited Windows rejection handling, transactional fixed override and
  KDE minimum helper inspected directly. Eleven discriminating variants and
  eight new OPEN requirements capture the proposal; no decisions promoted.
  Requirement recount: 138 NORMATIVE / 38 OPEN / 176 total; all 24 added local
  Markdown targets resolve, and the new section anchor resolves. Whitespace and
  ASCII checks pass. A broader link scan found a pre-existing stale Windows
  `settings.rs` link in the spec; left unchanged, outside this proposal.
- Exact next action: user selects D1-D10 (including the specified semantics), then
  promote accepted decisions and implement shared classification/projection plus
  all consumers, with offline evidence before separately authorized native checks.
