# Floor-Aware Rebalancing Feasibility

## Conclusion

A future opt-in can be mathematically feasible without another structural path:
it can move only existing ancestor boundaries, then perform the ordinary split
from a fresh decode. It is feasible only when a precomputed allocation keeps
every existing leaf at KWin's 15% floor and raises the intended leaf to the
30% extent required for an equal split. The current nearest-valid fallback
remains the result when preflight cannot produce that allocation.

This is not host proof for multi-ancestor writes. Current source and live
evidence establish a single shared-boundary ratio write; a multi-write sequence
needs the live spike below before implementation is approved.

## Evidence Boundary

- `kwin/src/controller.ts:67-79` pins `MINIMUM_TILE_FRACTION` to `0.15` and
  obtains the per-output work area.
- `kwin/src/controller.ts:2416-2541` uses `setTileRelativeGeometry` to move a
  shared boundary during resize. `kwin/src/boundary.ts:377-393` is the write
  seam. These support ratio-only updates, not a new split/remove/manage path.
- `kwin/src/controller.ts:2603-2626` and `5529-5545` preflight floor breaches:
  a leaf can be equally split on axis `a` only at extent `>= 2fW_a`, where
  `f = 0.15` and `W_a` is that output work area's extent on `a`.
- `kwin/src/controller.ts:6682-6826` contains current split planning and the
  nearest-valid fallback. It is unchanged by this research.
- `kwin/src/logic.ts:43-49`, `273`, and `312-354` establish scope identity,
  geometry tolerance, and parent-child geometry preconditions.
- The [live-testing guide](../../live-kwin-testing.md) requires a yield after a
  structural `remove()` before any later split, fresh resolution after removal,
  and bounded nested live attempts. It does not prove a yield is needed after a
  ratio-only write.

Authoritative KWin references, limited to two:

1. [KWin scripting API](https://develop.kde.org/docs/plasma/kwin/) documents
   the Workspace, Tile, `relativeGeometry`, and work-area scripting surface.
2. [KWin `tile.h`](https://invent.kde.org/plasma/kwin/-/blob/master/src/tiles/tile.h)
   defines the default tile minimum size as `QSizeF(0.15, 0.15)`.

## Pure Floor Model

Let `F_a = f * W_a` for each axis `a` in `{x, y}`. A leaf requires at least
`F_x` by `F_y`; an equal split of that leaf on axis `a` requires `2F_a` on
that axis before the split.

For a decoded subtree `S`, define `M_a(S)` as the minimum absolute extent its
root must have on axis `a` while all descendant ratios stay fixed:

- For a leaf, `M_x(S) = F_x` and `M_y(S) = F_y`.
- For a node split on `x` with child shares `r` and `1-r`, `M_x(S)` is
  `max(M_x(left)/r, M_x(right)/(1-r))`, while `M_y(S)` is
  `max(M_y(left), M_y(right))`.
- Apply the symmetric recurrence for a `y` split.

For the intended leaf, temporarily replace its requirement on the pending
split axis with `2F_a`. The root is feasible exactly when its available extent
is at least the resulting `M_a(root)` on both axes. This recurrence captures
every existing sibling subtree rather than checking only the directly moved
sibling.

## Candidate Ratio-Only Planner

Ranked by risk:

1. No-op when the intended leaf already meets `2F_a`; use current split
   behavior.
2. Adjust one existing matching-axis ancestor boundary when its feasible
   interval is sufficient. This uses the same write primitive as resize.
3. Adjust a precomputed sequence of matching-axis boundaries on the root to
   intended-leaf path. This is source-derived and requires live proof.
4. If there is no feasible interval allocation, do not write anything and use
   the existing nearest-valid fallback unchanged.

The planner works from the intended leaf to the root to compute each subtree
requirement, then from root to leaf to choose target-child shares. At each
matching-axis ancestor with parent extent `E`, target child requirement `T`,
and off-path sibling requirement `S`, the target share must be in:

```
[T / E, 1 - S / E]
```

An empty interval is impossible. Select current shares when they are in the
interval; otherwise select the nearest interval endpoint. The selected set is
then reduced to the smallest safe write set: prefer a single ancestor that
solves the target requirement, otherwise retain only changed path boundaries.
Recompute all descendant extents from the selected ratios and reject the plan
unless every leaf still meets both floors and the target meets `2F_a`.

This is a pure planner. It must produce all desired ratios before the first
write, so a failure is known before the live tree changes.

## Runtime Boundary And Invariants

- Rebalance performs only `setTileRelativeGeometry` on already decoded
  ancestor boundaries. It must not call `split`, `remove`, `manage`, or
  `unmanage` during this phase.
- Resolve one root in the target `(output object, desktop id)` scope. Floors
  use that root's output work area; no ratio or floor capacity crosses outputs.
- Preflight the whole plan against the recurrence. If impossible, make no
  ratio write and invoke today's fallback selection unchanged.
- After every write, fresh-decode the whole root and check the written boundary
  and all affected leaf extents with `RELATIVE_GEOMETRY_EPSILON`. Never reuse
  stale wrappers for the subsequent write or split.
- If a structural removal occurred before this operation, preserve the proven
  `remove() -> yieldOnce -> fresh decode -> ... -> split()` boundary. A
  ratio-only sequence has no evidence that it independently needs a yield;
  the live spike must test that assumption.
- On a write postcondition failure, stop. Do not attempt reverse writes: no
  rollback primitive is proven. Keep the freshly decoded, still-valid host
  state, do not perform the intended split in that dispatch, and report a
  refusal for a future product policy to handle.

## Multi-Output Scope

The 15% floor is relative to the target output's client work area, so the same
ratios can have different absolute capacity on different outputs. Solve one
root and one work area at a time. A request must neither consume capacity from
another output nor choose an ancestor outside the intended leaf's output and
desktop scope.

## Required Tests Before Implementation

Add pure planner tests that use decoded geometry fixtures, without KWin:

1. Assert the leaf predicate is inclusive at `2fW_a` and rejects just below it
   for both axes.
2. For each split orientation and mixed-axis ancestor path, assert the solver's
   emitted ratios make the intended leaf `>= 2fW_a` and every resulting leaf
   `>= fW_x` by `fW_y`.
3. Assert a sibling-floor conflict and an insufficient root capacity return no
   plan and cause zero planned writes.
4. Assert an already splittable target returns an empty rebalance plan.
5. Assert the planner rejects a candidate before writes when any off-path
   subtree violates its recursive requirement.
6. With the option disabled, assert current nearest-valid fallback target
   selection is unchanged.
7. Assert planning is per output work area: identical relative trees with
   different work extents have independently calculated absolute floors and
   no cross-scope candidate.
8. Assert each application step requires fresh decode and rejects a mismatched
   post-write geometry rather than scheduling the split.

## Future Live Proof Spike

Read `docs/live-kwin-testing.md` before any live work. In a disposable nested
session, use one output and a fixed window count:

1. Build a tree where the intended leaf is below the equal-split threshold but
   a single safe ratio change makes it eligible; prove fresh decode observes
   the changed boundary and the ordinary split produces two non-empty children.
2. Repeat with a multi-ancestor plan; establish whether writes require a yield
   between writes or before the split.
3. Build an impossible case; prove no ratio changed and the existing fallback
   is selected.
4. Force or observe a post-write mismatch path; prove it does not attempt a
   split or speculative reverse write.

Cap nested attempts at two, as required by the live-testing boundary. Test each
supported KWin version and relevant output-work-area arrangement separately.

## Parked Decisions

- Whether rebalancing is an opt-in behavior, its configuration surface, and
  user-visible refusal semantics.
- Whether a post-write host mismatch may retry from fresh decode or must leave
  the request refused for that dispatch.
- Supported KWin versions and whether they require yields around ratio-only
  updates.
- Whether the source-derived multi-ancestor sequence is acceptable after the
  bounded spike, given upstream internals may change.

## Static Runner Handoff (2026-09-01, retired)

- No qualifying nested live proof ran and no policy change was made; the
  retired static-only contract runner and fixture never established native
  feasibility, isolation, dependency suitability, or restoration
  ([archived static runner](https://github.com/beefsack/OmniTiler/blob/480a473e11fd9e0bf66c8d0a0852b6b76def0f0b/scripts/floor-ratio-feasibility.sh),
  [archived fixture](https://github.com/beefsack/OmniTiler/blob/480a473e11fd9e0bf66c8d0a0852b6b76def0f0b/test-fixtures/floor-ratio-evidence.json),
  [archived change note](https://github.com/beefsack/OmniTiler/blob/480a473e11fd9e0bf66c8d0a0852b6b76def0f0b/docs/changes/floor-ratio-feasibility.md)).
