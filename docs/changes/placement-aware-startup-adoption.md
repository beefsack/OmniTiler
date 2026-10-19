# Placement-Aware Startup Adoption

- Historical implementation evidence: legacy KDE adapter/DescribePlan citations
  below describe the removed route. The selected fitting policy remains; current
  production observes and applies through the world route. Legacy test results
  are not current-world acceptance evidence.

## Decision Proposal

- Approved: the initial first-startup policy is a versioned Rust-owned,
  best-effort near-layout fit. It uses one simple, comprehensible, deterministic
  heuristic to reduce unnecessary movement when tiling first applies.
- Exact recognition, historical-topology reconstruction, global optimization,
  exhaustive search, and many special cases are not goals. The prior exact-only
  recommendation is superseded.
- A fit produces the selected project policy, not a claim about the prior
  topology or COSMIC parity. Rust owns fitting and selection; KWin TS continues
  to send only the primitive normalized snapshot.
- At INITIAL adoption, attempt one straightforward deterministic near-layout
  fit. If it cannot produce a valid supported layout, use the existing normal
  deterministic seed/reflow. This keeps the existing managed lifecycle; it
  selects no park/unmanaged fallback or broader activation lifecycle. The same
  fit is selected for a post-CONFIRMED-Planner-loss fresh session only, from
  CURRENT eligible windows; it may change grouping and does not reconstruct the
  prior topology.

## Current Behavior And Boundary

- On first non-empty observation, KWin enables the adapter, observes the active
  output/workspace and eligible normal-window rectangles, then requests a
  resync (`kwin/src/plan-adapter-entry.ts:866-1064,1746-1766`).
- With no committed membership baseline, the adapter admits only the focused
  window (`kwin/src/plan-adapter.ts:1619-1643`). The empty Rust planner rebuilds
  a session by spatially ordering all observed windows with focus last, then
  sequentially performs normal focused-leaf admissions
  (`src/planner_protocol.rs:838-967,1432-1505`). Each split is based on the
  existing projected target, not the incoming rectangle
  (`src/planner_protocol.rs:808-836`). A good pre-existing layout is therefore
  reprojected.
- This proposal applies only when there is no usable retained session for the
  first startup domain, or for the selected post-CONFIRMED-Planner-loss fresh
  session from CURRENT eligible windows. It does not change a pending
  acknowledgement, post-observation verification, divergence, or
  transaction-recovery path.
- Rust owns topology and policy; KWin remains a thin observer/actuator using
  direct geometry (`docs/decisions.md:403-408`). Candidate construction,
  fitting, and selection therefore belong in Rust, with TS sending only the
  primitive normalized snapshot.
- The KCM/restart boundary remains unchanged: validated gaps are resolved once
  at adapter startup and are not hot-reloaded, reseeded, or mutated in flight
  (`kwin/src/plan-adapter-entry.ts:1525-1531`; `kwin/src/domain-gap.ts:1-7`).

## Fitting Direction

- Normalize the existing observation against the current domain work area,
  configured `outerGap` inset, and `innerGap`; these are KCM values with
  defaults `(8, 8)` and range `0..64`, not a hardcoded 8px rule
  (`kwin/src/domain-gap.ts:1-13,89-113`; `docs/decisions.md:446-452`).
- Use one bounded deterministic heuristic within the existing Rust geometry
  boundary to infer an ordered split tree and positive shares, then project the
  fit. It should favor a reasonable low-movement initial result, not perfect
  recognition.
- A deterministic fit is sufficient; it need not be uniquely recoverable from
  the observed rectangles. Rectangles cannot reveal historical same-axis
  grouping, equivalent shares, sibling intent, or domain MRU.
- Keep the heuristic straightforward and its technical mechanics ordinary
  implementation detail under this direction. It must not become exhaustive
  topology enumeration or a broad edge-case policy.

## Eligibility, Ambiguity, And Exceptions

- Preserve current observation eligibility: normal windows on the active
  output and current desktop (including existing all-desktops handling), valid
  quantized frame rectangles, stable IDs, and an active member
  (`kwin/src/plan-adapter-entry.ts:888-1069`). Empty scopes remain disabled.
- Treat an observation that the simple heuristic cannot support as genuine
  inference failure and use the selected fallback. Integer rounding and
  direct-geometry sequencing mean a fit is not proof of historical topology.
- Existing exception semantics remain authoritative: fullscreen and retained
  maximized members skip geometry writes; admission-time maximize clearing is
  an explicit KWin-native deviation; floating/sticky state is exceptional
  (`docs/decisions.md:261-290,416-445`). The current adapter observes these
  flags but omits fullscreen/maximized/sticky from the Rust request, while Rust
  reconstructs them as false (`kwin/src/plan-adapter-entry.ts:1048-1058`,
  `kwin/src/plan-adapter.ts:1973-1979`,
  `src/planner_protocol.rs:675-689`). Implementation must preserve the
  approved exception behavior at the current eligibility or pure input
  boundary. Whether it narrows the fitting input or carries the required
  primitive state is ordinary integration work, not a new product behavior.

## Feasibility And Evidence

- The topology is an ordered, nested N-ary tree with one positive `u64` share
  per child. AR16 retired the former 64-window/16-domain limits; the codec now
  caps request bytes at 1 MiB. Seeding orders windows and uses deterministic
  admission/projection. Exhaustive enumeration of trees, orders, or share
  vectors is not an acceptable adoption strategy; fitting uses a finite
  deterministic grammar.
- Future implementation evidence should establish deterministic near-fitting,
  low unnecessary startup movement, configured gaps/inset handling, genuine
  inference failure, and the selected fallback. A live journey is not requested
  by this design.
- Independent source review found no COSMIC rectangle-to-tree inversion rule.
  COSMIC source parity covers forward focused-cell admission and N-ary share
  evolution, not recovering prior intent from placement
  (`docs/decisions.md:416-445`; `src/cosmic_v1.rs:50-198`). A fitted adoption
  policy is therefore a documented project policy, not claimed COSMIC parity.

## Implementation Outcome

- Static implementation delivers the INITIAL fit only. A fresh focused admit
  with no retained domain attempts one deterministic near-strip fit. After the
  existing valid contained non-overlapping checks, horizontal support needs the
  existing `(x, y, w, h)` order to have strictly non-overlapping sequential x
  intervals (`previous.x + previous.w <= next.x`), regardless of domain edge
  offsets, cross-axis drift, or the observed inter-window gap; vertical mirrors
  with `(y, x, h, w)` order and y intervals. One flat N-ary group uses the
  observed primary spans as shares and projects with the configured gap as the
  canonical valid complete result; exact input reprojection is not required.
  Both axes supporting selects horizontal. Grids, nested, and T arrangements
  with interval overlap on both axes, plus exceptions and invalid geometry,
  use the normal deterministic seed/reflow fallback.
- Rust constructs and validates the fitted N-ary topology, then proposes,
  acknowledges, verifies, and commits it through the existing lifecycle path.
  Foreground and background domains use the same fresh-admit route. Retained,
  pending, divergent, explicit-placement, unfocused, and single-window paths
  retain their prior behavior.
- The native adapter marks any floating, sticky, fullscreen, or maximized
  member as fit-excluded. That declines fitting and preserves the existing
  normal seed/reflow exception behavior; configured gaps remain adapter inputs
  and Rust projection policy. Planner-loss fresh-session integration is now
  static-complete: KWin confirmed-loss recovery clears its lifecycle baseline
  and dispatches one current observation through this same fresh-admit helper
  without replaying the old command; offline TypeScript coverage only, live
  acceptance pending with no live claim.
- Offline Rust and TypeScript coverage establishes deterministic near-strip
  results, canonical configured-gap projection, exceptions and
  unsupported-input fallback, lifecycle base revision, retained follow-up
  behavior, and adapter payload exclusion. Static state is covered by unit
  tests; live KWin/Plasma acceptance is user-owned and pending, and fitted
  geometry is the projected static result, not a live-state claim.

## Backlog Maintenance Finding

- `docs/backlog.md:89-90` is stale: production supports general retained
  per-domain lifecycle and ordered N-ary trees for up to 64 observed windows,
  not exactly three (`src/planner_protocol.rs:1432-1505`;
  `src/directional.rs:107-123`). Narrow follow-up: remove or rewrite only that
  exact-three decision line. Do not change it in this design record.
- `docs/backlog.md:93-94` is also stale as written: current rebuilding uses
  stable spatial `(y, x, h, w)` order with focus last, not lexical opaque-ID
  order or the historical fixed `H[A,V[B,C]]` seed
  (`src/planner_protocol.rs:928-967`). Narrow follow-up: replace it with a
  current-policy question only if ordering remains intentionally unsettled.
- `docs/decisions.md:127-132` also describes Rust as lacking adoption and
  add/remove lifecycle, contrary to the current production path above. This is
  a material documentation contradiction to resolve separately, not an
  authorization to alter lifecycle policy.

## Review 2026-09-29

- Startup requests a debounced complete per-domain observation
  (`kwin/src/plan-adapter-entry.ts:3156-3178`; `kwin/src/plan-adapter.ts:4804-4837`).
  Rust attempts flat-strip fit for a fresh domain, otherwise floating-aware or
  focus-last seed; later arrivals converge into retained topology
  (`crates/tiler-core/src/engine.rs:601-635,792-1002`). Internal seed steps
  never write native geometry: KWin applies one final projection per flight,
  skipping equal rectangles (`crates/tiler-core/src/seed.rs:424-464`;
  `kwin/src/geometry-order.ts:29-52`; `kwin/src/plan-adapter.ts:7478-7597`).
  Confirmed Planner loss reobserves after clearing evidence; KWin restart with
  surviving Planner retains shared-ID domains, fresh-adopts disjoint IDs
  (`kwin/src/plan-adapter.ts:6063-6111`; `docs/decisions.md:301-308`).
- Correction to the initial review: these are not proven successive *startup*
  flights. `uE1S5n.log:4-12` already has `p0/p1` carrying 2/3 members before
  the cited `p2/p3/p4` carry 1/2/3 (`:62-71,124-130,190-199`); the later
  members first appear at `:143-148,210-215`. `H28tD1.log:4-16` already
  settles `p0/p1` before `p2`, and `p3` follows many further refreshes
  (`:47-74,82-610,626-647`); its third member first appears at `:639-641`.
  Neither log records whether these later members were newly mapped or became
  eligible. Native-effect `observe-seed` cannot decide: it runs both on effect
  load and window addition (`kwin/native-effect/activewindowborder.cpp:253-272`).
  `llROHi.log:4-16,151,199-227` starts with earlier 3/2-member plans; later
  Settings open precedes admission of its config window. The other two `p2`
  members were already observed on another desktop (`:17-23`), not created
  during `p2`. `y3jVs3.log:15-21` skips two equal strip members;
  `FeTnf4.log:18-24` begins mid-write. Fit/fallback choice, confirmed-loss
  journey and native visual frames remain unproven by these logs.
- Source explanation: the foreground observer includes every eligible normal
  window on the active output/desktop and quarantines an unreadable member
  rather than sending a partial snapshot (`kwin/src/plan-adapter-entry.ts:1451-1658`).
  Hidden domains are complete individually, processed one flight per domain
  (`kwin/src/plan-adapter.ts:5049-5098,8787-8831`). `admitted=1` counts a
  member newly observed in a retained domain, not necessarily a new KWin window
  (`crates/tiler-core/src/engine.rs:278-340`). Later real opens correctly
  reflow one per observation; `desktop-mismatch` excludes other-desktop members
  until their domain is observed. No cited trace proves that the first fresh
  adoption omitted an already-eligible member.
- Ranked next steps: (1) bounded correlated fit/fallback plus observation-source
  diagnostics (~30-80 production LOC, low risk, inside observability policy);
  (2) include missing pre-existing members in first adoption (already true for
  complete eligible snapshots, 0 LOC absent a proven observer bug; do not merge
  different workspaces); (3) event-driven bounded startup coalescing (~40-100
  LOC plus tests, medium risk, delays tiling, needs a user-visible timing choice
  and evidence of an actual restore burst). Recursive fitting was subsequently
  approved below; keeping noncanonical observed rectangles remains unselected.
- Live acceptance remains: source-pinned user-run `just dev trace`, two normal
  windows at `8,52,756,964` and `772,52,756,964` on `eDP-1` with work area
  `0,44,1536,980` and gaps `8`; first `reconcile` should fit, log two
  `skip-already-equal`, zero `written`, then quiet `applied-evidence-equal`.
  Repeat after confirmed Planner loss (`plan:recovery ... outcome=confirmed-loss`)
  and distinguish KWin restart with surviving Planner. The fit marker is now
  implemented below; observation-source diagnostics remain unimplemented.

## Recursive-Cut Outcome 2026-09-29

- User clarified the concern: restarting over a previously tiled, sometimes
  resized nested layout can swap windows when fallback seeds focus-last.
  The user approved replacing flat-strip fitting with a simple recursive-cut
  heuristic, without rare-case machinery. Defaults, user approved:
  horizontal-before-vertical cuts, geometry order independent of focus, all
  viable cuts per axis forming N-ary siblings, observed-span shares, orthogonal
  recursion and `max(inner gap, floor(3% of domain axis))` per-window cut
  crossing tolerance.
- Rust now fits flat strips and nested/T layouts through the same fresh-adoption
  lifecycle, projects configured gaps, and falls back to unchanged seed/reflow
  on exclusions, invalid geometry, no cut or invalid projection. One bounded
  `omnitiler:adoption-fit outcome=... windows=... reason=... correlation=...`
  line reports each fresh fit attempt; retained reconciles do not repeat it.
  No adapter or native changes. Offline tests cover focus-independent exact
  nested adoption, resized shares, N-ary axes, configured gaps and tolerance
  success/failure. The user confirmed live that restarting over a previously
  tiled workspace works well; the confirmed-loss journey remains pending.

## Centre-Split Outcome 2026-09-29

- User option B extends the recursive fit to eligible overlapping layouts.
  Defaults: if neither axis has a tolerance-valid cut in a piece,
  choose the widest adjacent centre gap (horizontal on axis tie), split that
  piece in two, use each side's maximum observed member span as its share, and
  recurse. Identical centres on both axes decline to the unchanged seed path.
  Clean cuts still take priority; intentional floating and other fit-excluded
  members remain outside this fit.
- Rust carries a `centre_splits` count into the correlated normal-level
  `adoption-fit` line: clean fits and fallbacks report 0. Offline behavior
  tests cover 3-4 overlapping cascades, size-proportional big/small allocation,
  focus and input-order independence, clean nested geometry and identical-centre
  fallback. Live floating-layout adoption has not yet been checked.
