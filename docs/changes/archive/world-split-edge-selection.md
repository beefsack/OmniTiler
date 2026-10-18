# World-route split-edge output selection

## Goal and scope

- Apply the existing 2026-10-09 output-selection decision to core-owned world
  topology: exhausted R4 moves and directional sends use window centre/span;
  workspace migration uses largest shared edge. Focus stays strict reciprocal.
- Preserve pinned R4 source/target identities across fresh topology checks,
  existing operation safety fences, follow/stay and placement behavior.
- No new policy, wire changes, native adapter cutover or live acceptance.

## Acceptance and approach

- Select from validated FULL rectangles with the existing pure rankers, then
  construct a reciprocal operation pair. Reverse uniqueness is not required.
- Unreadable topology refuses; no candidate is a no-op; no wrap.
- Expected-result regressions cover R-MOV-12 centre/span/tie discriminators,
  R-WS-27 migration, split-edge strict focus, ordinary two-output behavior,
  unreadable topology, no candidate and pinned R4 fresh-topology validation.
- Exercise KDE production-entry/replay fixtures through the world route.
- Verify Rust workspace tests, strict all-target clippy, formatting, KDE tests,
  typecheck, script bundle and fixture/descriptor freshness gates.
- Update outcome/status records without introducing a new product decision.

## Bounded units

1. Implement world selection and Rust/KDE regression coverage.
2. Independently review contracts and safety fences; resolve concrete findings.
3. Complete offline gates, reconcile current main and record accepted outcome.

## Decisions and evidence

- Existing decision: reciprocal edge existence, not reverse uniqueness, governs
  move/send/migration; unique-reciprocal horizontal focus is unchanged.
- Window/migration rankers and reciprocal pair validation are implemented;
  Rust workspace tests and strict clippy passed after selector changes.
- Production world-entry fixtures exposed a pre-existing R4 integration gap:
  successful cross-output moves previously retained no transfer for native
  actuation. The gap is closed with pinned arrival regressions, not merely
  assertions about selected geometry in a reply.
- A migration fixture also exposed target self-refill after native arrival;
  a narrow source-versus-target guard prevents that extra view switch.
- Migration confirmation appears not to settle against KDE's global native
  workspace inventory. Investigate separately from selector policy; no native
  acceptance or settlement claim is made yet.

## Outcome and accepted offline evidence

- World move/send select the existing window ranker; migration selects the
  existing largest-edge ranker. Reciprocal operation pairs are constructed
  after selection; reverse uniqueness is not required. Focus stays strict.
- R4 now retains the existing window-transfer contract for native actuation,
  including capacity-float fallback. Dispatch identities survive delayed
  arrival; newer moves refuse before engine mutation while a flight exists.
  Local R1-R3 produce no transfer. No wire or native capability was added.
- Tentative implementation choices: reuse retained window transfers for R4;
  guard migration refill when observed member homing already names the target;
  add a test-only retained `world_eval` bridge for production-entry fixtures.
- Independent review identified pending-flight fencing and helper-only pinning
  coverage gaps. Production-path regressions now prove pre-engine refusal,
  unchanged revision, pinned arrival and capacity-float retention. The shared
  directional operation path was unified instead of duplicating pair handling.
- Rust workspace: 3,182 passed, zero failures. Strict workspace/all-target
  clippy, formatting, settings-exporter descriptor freshness, protocol examples
  and portable-core gate passed. KDE: 1,870 passed, zero failures; typecheck and
  script packaging passed. World codec/producer byte fixtures pass on both sides.
- Dedicated coverage: 22 world directional-intent regressions and eight KDE
  real-Engine production-entry legs, including native actuation, deferred
  arrival, fresh topology, follow/no replay and unreadable topology.
- Verification invocation correction: an incorrectly quoted trace define made
  the test bundle trace-free; the exact string define restored the intended
  test build and all KDE tests passed. No product change was used to fix it.
- No live tests were performed. Native Windows/macOS acceptance is not claimed.
- Rebased cleanly over the concurrent KDE native output-echo correction and
  Windows world-reply fixes. Repeated all acceptance gates on the integrated
  tree with the results above; both changes are preserved.

## Parked limitation and native acceptance

- KDE migration confirmation compares source native-order absence with a global
  native workspace inventory, leaving successful migrations retained and
  re-observed. Selection and exact target/refill writes are covered; eventual
  migration settlement is not claimed. This pre-existing contract issue is
  tracked in the shared-topology backlog item, not repaired here.
- If three outputs can form a split edge, use FULL source S touching a larger
  B edge and smaller C edge. Put a sole mover with centre projecting onto C.
  From fresh identical states verify exhausted move and directional send reach
  C, workspace migration reaches B regardless of mover position, and horizontal
  focus does not cross the ambiguous edge. Repeat with centre exactly at a seam,
  centre in a gap (span-overlap fallback), and overlapping containing edges
  (left/top tie). Verify native membership, frame/work-area placement, focus,
  follow/stay, delayed arrival and absence of replay; leave unavailable layouts
  and exact reference outcomes TBD.
- With ordinary two edge-touching outputs, verify local-neighbor precedence,
  R4 after exhaustion, both directions of follow/stay sends, and migration
  target/refill. Change work areas without FULL geometry, then introduce a gap;
  verify stable targets in the former and no wrap/crossing in the latter.
  Follow the live-testing procedure and restore the exact baseline.
