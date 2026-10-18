# Shared core: core-owned output topology

- Outcome: core and KDE delivered offline, 2026-10-18. Native two-output
  acceptance remains pending; no live testing was performed for this change.
- User direction 2026-10-18: core owns monitor positions in world state and
  derives adjacency; adapters supply FULL rectangles in complete world reports.
- Goal: one topology authority, following [Layering and Simplicity](../../principles.md),
  without a topology RPC, polling, new setting or adapter-owned adjacency cache.
- Non-goals: split-edge policy alignment, new native capabilities, workspace-scope
  redesign, Windows runtime cutover or macOS native-Space transfer support.

## Delivered contract

- Outcome addendum 2026-10-18: [world split-edge alignment](world-split-edge-selection.md)
  now applies the existing 2026-10-09 per-use rules. Move/send use the window
  ranker, migration uses largest shared edge, focus alone keeps strict
  unique-reciprocal horizontal selection. The strict-for-all descriptions and
  parked split-edge status below describe the initial ownership extraction,
  not the final world-route policy. No new product decision was required.

- `OutputTopology.geometry` / wire `rect` means FULL native output geometry in
  one coherent adapter coordinate space. Domain `bounds` is still the separate
  work area; outer-gap handling is unchanged. Never infer FULL geometry from
  work areas, windows or the readable subset of an incomplete report.
- `WorldEngine.live_topology` retains validated live ids and FULL rectangles in
  shared, per-output-local and global-unique modes. Each accepted report replaces
  it before same-report command planning. Empty/incomplete topology or a missing
  rectangle clears availability, not just the failing edge. Null remains
  representable and narrowly fences topology-dependent operations; local work
  and later complete-report recovery remain possible. Unrelated window-inventory
  incompleteness does not clear otherwise complete monitor topology.
- Existing report validation, staged commits, replay, generation reset and
  in-flight transfer fences remain authoritative. Disconnect-time placement
  facts remain separate from live directional topology.
- `output_selection::unique_reciprocal_adjacency` uses existing FULL-rectangle
  candidates: exact edge-touch, positive perpendicular overlap, no wrap. A
  direction resolves only with one forward candidate and a unique reverse
  candidate back to the source. The existing window and migration rankers are
  unchanged and remain distinct policies.
- `start_migration`, `operation_migration`, `directional_pair_for` and directional
  sends resolve in core. Internal reciprocal pair maps are core-derived.
  Contradictory/empty carried world adjacency cannot select a target. Explicit
  named sends retain their existing eligibility, including non-edge targets.
- Direction-only world commands (`focus-directional`, `move-directional`,
  `send-directional`, `migrate-directional`) remove KDE's need to select targets
  or set crossing flags from geometry. Focus retains horizontal-only crossing;
  moves may cross all four directions. No strict target means local focus/move;
  an existing strict target with a held/unreadable pair refuses, without a new
  local fallback. Existing local R1-R3 precedence and safety gates remain.
- Directional sends choose the target output's current native view, preserving
  follow/stay intent. Directional migration carries explicit source output and
  workspace, including when no window is active. Migration still needs complete
  window inventory; geometry-dependent hotplug remap now also fences missing
  FULL rectangles rather than using an invented primary-output placement.
- KDE production no longer calculates or emits resolved world adjacency. It
  emits an empty deprecated `adjacent` object because the existing codec still
  requires the key. Old non-empty values remain accepted but non-authoritative.
  Removing that wire key is a later coordinated cleanup. Legacy per-domain
  adjacency APIs and their existing selector calls remain transitional.

## Decisions and boundaries

| Decision | State |
| --- | --- |
| Core owns FULL topology from complete world reports | User-directed, 2026-10-18; delivered |
| Extract the existing KDE strict unique-reciprocal world policy only | Tentative delivery choice, as requested; ordinary two-output behavior unchanged; not promoted to a new durable selection policy |
| Represent incomplete live topology as unavailable rather than retaining usable stale edges | Tentative implementation choice; missing geometry fences topology-dependent operations, not the entire report |
| Distinguish monitor completeness from unrelated window-inventory completeness | Tentative implementation choice; migration/displacement still retain their own inventory safety gates |
| Add direction-only world intent commands rather than duplicate geometry policy in KDE | Tentative implementation choice; old operation and legacy per-domain contracts remain available |
| Retain an empty wire adjacency key during ignore-then-remove | Tentative compatibility choice; never a competing authority |
| World migration -> largest edge; R4/output send -> window-position rule | Existing 2026-10-09 decision; subsequently delivered offline in the alignment addendum |

- Recommendation for the parked split-edge decision: align migration with
  `select_for_migration`, R4/directional output sends with `select_for_window`,
  and preserve unique-reciprocal KDE focus. Review Windows focus separately:
  its legacy window selector differs from KDE focus. This changes split-edge
  world behavior and must not be hidden in ownership extraction.
- Minimal [split-edge discriminator](../../spec/reference-outcomes.md#core-owned-topology-split-edge-discriminator):
  S=(0,0,1000,1000), B=(1000,0,1000,700), C=(1000,700,1000,300), A on S
  centred at y=850. Current strict world crossing has no right target.
  Proposed expected-result tests, only after a decision:
  `world_migration_split_edge_prefers_largest_shared_edge` -> B;
  `world_r4_split_edge_uses_window_centre` -> C;
  `world_send_split_edge_uses_window_centre` -> C;
  `world_focus_split_edge_preserves_strict_refusal` -> no crossing.
  Reference/native outcomes remain TBD.

## Accepted offline evidence

- Shared `test-fixtures/output-topology-parity.json`: 12 real-shaped cases,
  checked against the original KDE `computeAdjacency` before removal and the
  Rust helper. Frozen original logic remains test-only in
  `kwin/tests/output-topology-parity.test.ts`; no production import or fallback.
  Cases include offsets/negative origins, unequal heights, stacking, normalized
  mixed-scale coordinates, gaps/corner-only contact, forward/reverse ambiguity
  and mirrored overlap. FULL geometry stays separate from panel work areas.
- Rust parity integration: 2 tests; existing ranked-selector tests remain
  separate. Core topology regressions cover all workspace modes, same-report
  updates, ignored wrong adjacency, partial/null fencing and recovery,
  rejected/duplicate reports, generation reset and retained transfers.
- `world_directional_intents`: 10 tests, including ordinary two-output and
  single-output behavior, local fallback without a strict target, vertical
  focus policy, held/unreadable target refusal, follow/stay sends, migration
  without an active window and independent monitor completeness.
- `world_displacement`: 13 tests. Old complete fixtures missing FULL rectangles
  were corrected; the former missing-geometry primary fallback expectation now
  proves narrow remap refusal and recovery from a subsequent FULL report.
- Independent diff review found no remaining authority leak. The cross-refusal
  fallback concern was corrected and regression-tested before acceptance.
- Final offline gates: `cargo test --workspace` **3,170 passed, 0 failed**;
  `cargo clippy --workspace --all-targets -- -D warnings`, workspace formatting,
  KDE settings-exporter freshness and planner-eval build passed. KDE test script
  phases **1,856 passed, 0 failed**; typecheck and `just build-kwin-script` passed.
  World producer/codec byte fixtures and `git diff --check` passed.
- Linux-host portable tests are not native Windows/macOS acceptance. No
  dependencies, unsafe code or Windows/macOS production adapters changed.

## Required follow-ups

- Windows: resume the [W2 capture/diff handover](../windows-full-world-state-replies.md#blocker-preservation-and-exact-next-action).
  Use real W1 FULL reports with empty adjacency to prove migration/refill,
  feedback and re-observation. Do not add Windows adjacency code. Reconcile the
  preserved prototype's native gate failures before W2 acceptance; runtime W3
  remains a separate atomic cutover. Do not silently standardize Windows focus.
- macOS: change world topology from `domain.visible` to already-retained FULL
  `NativeDomain.bounds`, keeping visible work areas/outer gaps unchanged. Verify
  negative/above-primary origins, scale-independent coordinates, rounding at
  shared edges and arrangement independently of Space order/focused display.
  Preserve UUID identity, threading and unsupported Space/transfer refusals.
  [World adoption evidence](macos-full-state-replies.md) is not reopened.
- Wire cleanup: remove the deprecated adjacency key only with coordinated codec
  and fixture updates; legacy per-domain pairs remain until adapter cutover.

## Exact pending KDE two-output validation

Follow [live KWin testing](../../live-kwin-testing.md), with identified disposable
clients and before/after native output, workspace, frame, focus and report/reply
evidence. Preserve and restore the existing desktop/output baseline.

1. Use two FULL edge-touching outputs, A left of B; record FULL rectangles,
   per-desktop work areas, panels and gaps. Settle at least two tiles on A and
   one on B. Verify observed `adjacent` maps are empty and topology rects FULL.
2. Focus/move right from A, then left from B using the directional actions;
   prove local-neighbor precedence first and R4 crossing only after local
   exhaustion. Check focus, membership, frame/whole-target wrapper and gaps.
   Up/down focus must remain local. Repeat from a single-output baseline.
3. Invoke `omnitiler-send-output-right-stay`, then the inverse left-stay;
   repeat with `omnitiler-send-output-right` and left follow. Prove membership
   arrival and exact target current view; stay must preserve views/focus and
   follow must not precede arrival or replay after re-observation.
4. Invoke `omnitiler-migrate-workspace-right` and inverse left with distinct
   populated source/target views. Prove the workspace remains separate, target
   membership arrives, source refill follows the existing policy, and follow-up
   observations settle without repeating effects. Repeat with no active window
   in a readable empty source workspace, subject to existing eligibility.
5. Change panel/work-area geometry without moving FULL outputs; repeat crossing
   and verify the same target and exactly one outer-gap inset. Rearrange outputs
   so the opposite edge touches, then leave a gap: the first fresh report must
   use the new edge, and the gap must refuse crossing without wrap/stale edges.
6. Disconnect/return one output and verify separate displaced-workspace contents,
   survivor view/focus preservation and return behavior. Restore the exact
   output arrangement, panels, clients and workspace baseline. Physical shortcut,
   visual and protected-window/game acceptance remain distinct observations.

- Exact next action: KDE native two-output validation; Windows W2 and macOS FULL
  geometry follow-ups may consume the delivered shared core independently.
