# Best-effort admission when space runs out

- Status: core/KDE delivered offline 2026-10-18; live re-check pending.
- Goal: replace geometric-capacity holds with deterministic best effort without
  degenerate native rectangles, implicit workspace changes or repeated effects.
- Direction: user 2026-10-16 requests anchor fallback and float/overlap settings,
  prefers float, and rejects automatically opening on another workspace.
- Scope: shared core and KDE implementation, settings and offline evidence;
  Windows/macOS receive mechanical compatibility fixes only. No live testing.
- The User selections below override conflicting historical recommendations,
  including fresh-float focus and cross-restart persistence. Capacity origin is
  diagnostic only; capacity floats otherwise behave and persist as ordinary floats.

## Plan and acceptance

1. Lock numeric overlap geometry/navigation goldens before production projection.
2. Implement shared policy/catalog, transactional admission, overlap projection,
   newest-first repair and route-specific behavior with regression tests.
3. Wire the shared settings and complete mixed roster through protocol and KDE,
   retaining ordinary-float persistence and next-decision settings timing.
4. Verify portable API guard, exporter freshness, Rust, KWin, shell and native
   checks; review contracts independently; promote decisions and archive.

- Acceptance: selected D1-D10 and operation boundaries hold offline, including
  stable replay, positive geometry, B6 preservation, atomic tile/float rosters,
  explicit retries, safety protections, persisted floats and no settings reflow.
  Native behavior remains unverified until the separate live re-check.
- Accepted pre-implementation fixtures: `d9_overlap_golden` pins flat 1px overflow,
  nested branch/sibling overlap, square/nonzero-origin geometry, checked offset
  failure, B6-after ordering and tree-relative directional navigation. Six tests
  pass with strict projection refusing exhausted inputs; no production overlap
  implementation existed when these numeric expectations were accepted.
- Shared primitives delivered: explicit plain/hinted positive-overflow projectors
  with separate capacity-overflow flags and checked endpoints; strict projection
  remains unchanged. Goldens now exercise production, including endpoint failure.
- Shared catalog and KDE Settings expose `admission.anchor-fallback` and
  `admission.no-space` (`anchorFallback` / `noSpaceResult` native keys).
  Windows explicitly partitions these as unsupported until runtime adoption.
  Workspace tests, exporter freshness, KWin tests and 27 offline native tests pass
  for this primitive/settings increment. Runtime delivery and final evidence follow.

## Historical source audit

- Locators below were checked against `b244646b`; source evidence, not native
  acceptance. Tier 1 evidence remains
  [R-MIN-04](../../spec/reference-outcomes/minimum-size.md#r-min-04-pure-geometry-exhaustion)
  and [R-INS-09](../../spec/reference-outcomes/insertion.md#r-ins-09-burst-of-maps-versus-roster-delivered-together):
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
  [placement/minimum/startup](../../decisions.md#placement-minimums-and-startup-adoption).

## User selections and historical options

- The selected D1-D10 are promoted to
  [durable decisions](../../decisions.md#best-effort-admission). The original
  options table is historical: the User selections override conflicting
  recommendations, especially float focus and persistence. Default capacity
  floating alone does **not** eliminate B6 minimum overlap.
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
  - D5: no automatic re-tile when capacity returns; explicit tile/unfloat
    behaves as for any float. Capacity floats persist across restart like
    user floats, so restart adoption cannot re-tile one and capacity-float a
    different window, which would change the layout surprisingly.
  - D6: all capacity-producing routes, with the route rules under Operation
    boundaries as proposed.
  - D7: float the newest tiles (first successful tiled-admission order) until
    the retained tree projects, with the proposed protections. Revisit later
    if needed: alternatives such as float smallest or float biggest may suit
    better, but this case may prove rare enough. Evicted windows persist as
    floats across restart, as in D5.
  - D8: preserve clean-fit adoption first, then the existing spatial
    focus-last serial seed with D2/D3 fallback, committing all tile/float
    classifications atomically as one roster; persisted floats stay floating.
  - D9: overlap mode uses the positive-overflow projector as proposed, with
    golden geometry/navigation cases agreed before coding.
  - D10: two global fields in the shared catalog under Tiling & Workspaces >
    Admission, applied to subsequent decisions only (no reflow on Apply).
  - All D1-D10 are selected; implementation may proceed.

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

## Selected semantics (subject to User selections above)

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
  a fresh restart retains ordinary float membership through the existing user
  float store. Diagnostic reason and tiled-admission history are not persisted.
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

## Settings delivery

- Selected functional labels/tokens:
  - `When the anchor cannot split`: `largest-splittable-window` (default),
    `closest-splittable-relative`, `no-search`.
  - `When no split is available`: `float` (default), `allow-overlap`.
- Help must say: positive extent after gaps, declared-minimum B6 overlap remains
  under D1, search never leaves the domain, no-search also invokes the second
  setting, capacity floats stay floating until explicitly tiled. No WM-branded
  labels: this is an explicit host-limitation deviation from the tier rule.
- Extend the existing core `tiling.admission` catalog section
  (`crates/tiler-core/src/settings_catalog.rs:61-68,101-105`). KDE native controls
  follow the approved checked-in descriptor/exporter direction. The fields are
  `admission.anchor-fallback` / `admission.no-space`, native keys
  `anchorFallback` / `noSpaceResult`. KDE retains tolerant fallback. Windows
  explicitly partitions these catalog fields as unsupported until adoption;
  no ignored schema keys or macOS UI were added. No generic setter invokes
  native remediation. Missing/invalid KDE values use selected defaults.

## Delivered implementation and accepted evidence

- Commits: `91f1da86` numeric geometry/navigation goldens before production;
  `1caadf15` shared settings, explicit plain/hinted overlap projectors and KDE
  controls; `cf422d2f` transactional core policy and KDE runtime integration.
  `26c88a8d` defers native-gesture capacity repair and supplies defaults in
  Windows-only full-world fixture constructors; `c75fe675` aligns stale native
  Windows preview/restart fixtures without runtime adoption; `b7da8fb9` keeps
  first-successful-admission age monotonic across independent Engine domains.
- Core: ordinary-anchor/fallback admission, positive-overflow projection,
  first-successful-admission ordering, protected newest-first capacity repair,
  fit-first atomic mixed startup roster and overlap-aware operations. Default
  800x600 gap-8 fourteen-window seed now tiles rather than holds; genuinely tiny
  domains exercise mixed tile/float rosters. No universal window-count limit.
- KDE: complete explicit float rows, diagnostic-only capacity origin, ordinary
  float echo and the existing private restart-intent store; no phantom slot or
  capacity frame setter. Explicit failed retry reports no-space without a tile
  override. One-shot newcomer focus preserves ordinary float focus bookkeeping
  and D01 overlay/game activation ownership; fresh maps into previously empty
  controlled domains focus, startup/enable adoption does not steal focus.
- Sends/R4: distinct `move-to-float` transaction, exact R4 wrap first/no D2,
  survivor-only geometry, source/arrival/follow proof and native cross-output
  remap once. Cross-output capacity floats claim no source-coordinate target
  frame. Same-output sends carry the actual observed live frame, not canonical
  allocation. Drag float fallback honestly refuses/snaps back until a matching
  preview exists; overlap preview/finish share fresh validation.
- Independent review corrected batch-order eviction, current-frame transfer
  handling and exact size accounting. Regression fixes also pin real geometry
  changes versus settings-only overlap retention, protected activation and
  mid-gesture at-most-once focus disposal. Final evidence follows all fixes.
- Final Rust workspace: **3092 passed, 0 failed, 86 suites**; format and strict
  workspace clippy pass. KWin: **1824 passed, 0 failed, 274 suites**, typecheck
  and bundle build pass. Shared exporter freshness, portable/shell API guards,
  15 headless shell suites and unsafe guardrail pass. Hermetic native effect
  **34/34** and native settings **27/27** tests pass. Concurrent native-platform
  changes were preserved. [CI 39076930351](https://github.com/beefsack/OmniTiler/actions/runs/39076930351)
  passes all eight jobs at `b7da8fb9`, including Windows/macOS supported-target
  build/tests/clippy, Rust, KWin, shell, native, licenses and notices.
- Native-gesture regression: shrinking/changing gaps during a title-bar gesture
  retains topology, admission order and float status under a truthful gesture
  hold; the next accepted clear-gesture report repairs once using live frames.
- Cross-domain age regression: chronological admissions in independently seeded
  domains use one Engine-wide counter; sending them together preserves lifetime
  ages, and shrink evicts the actual newest, not a colliding local sequence.
- D1-D10 promoted, capacity requirement rows selected: **148 NORMATIVE /
  28 OPEN / 176 total**. Windows/macOS consumption is a separate follow-up, not
  claimed by compiling their transitional domain callers.

## Residual risks and exact live re-check

- No live test performed. B6 minimum overlap remains; overlap mode deliberately
  permits overflow and tiny allocations. Newest-first is not an optimizer.
  Protected/invalid geometry still holds. Ordinary float persistence retains
  membership, not topology, diagnostic cause or admission history. Existing
  lost/stale/gesture focus effects are dropped, never replayed later.
- Use disposable ordinary windows on an isolated test workspace; keep unrelated
  clients and fullscreen/game sessions outside the test. Save current settings.
  Script changes require **`just dev-off` then `just dev`** (or `just dev-on`
  after the build is current). **`just reload` reloads only the Planner** and
  does not pick up script changes.
1. Open Settings > Tiling & Workspaces > Admission. Confirm defaults Largest
   splittable window / Float, all three anchor choices and both no-space choices.
   Save/reopen, Cancel and Defaults; confirm status reports saved/requested,
   never falsely confirmed. Applying these policies alone must not move tiles.
2. Temporarily select No search / Float and a larger inner gap (up to 64).
   Repeatedly open disposable ordinary clients at the focused newest tile until
   that anchor cannot split. The newcomer must keep its opening frame, focus
   once and behave as an ordinary float; existing tiles keep tiling. A client
   minimum alone may still cause B6 overlap and is not this exhaustion test.
3. Leave a large tile elsewhere. Apply Largest splittable window without any
   movement; explicitly tile the floated newcomer with Meta+G. If a strict
   alternate split fits, it must tile there and keep newcomer focus. Repeat
   using Closest splittable relative to distinguish the sibling-ring choice.
4. With a capacity float present, close/grow/reduce gaps and switch workspaces:
   it stays float. Explicit Meta+G retries once; still-full remains float and
   reports no-space, while feasible tiles. Minimize/restore, then restart with
   `just dev-off` followed by `just dev`: float membership remains, with no
   arbitrary replacement victim. Repeat ordinary disable/enable.
5. Settle known older/newer tiles, increase gaps until retained geometry fails:
   newest eligible tiles float first at last live frames, survivor tree contracts,
   active native focus stays unchanged. Repeated updates do not refocus/reset
   frames. A retained maximized/fullscreen slot must not be cleared or evicted;
   no game probe is required.
6. Select Allow overlap (no immediate reflow), then make a new exhausted split:
   every target remains positive, navigation remains deterministic, and repeated
   reports do not rewrite floats or reapply focus. Switch back to Float: existing
   overlap remains until an actual later capacity decision.
7. With two outputs, send a tiled client into an exhausted destination (follow
   and stay), then try directional R4. Destination remains explicit, R4 never
   redirects to another leaf/output, source reflows only after the transfer
   proof, native remap keeps the mover visible, no duplicate transfer/focus.
   Drag to an exhausted edge: preview/drop agree or refuse/snap back, never a
   hidden finish-only float. Restore saved settings and close test clients.
- Exact next action: user-owned live re-check above; Windows/macOS adoption is
  tracked separately. No open product decision.
