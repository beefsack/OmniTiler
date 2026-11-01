# niri reference source-fill

## Goal and scope

- Baseline `7ed5a19`: attempt niri's 41 N and 12 U cells using the read-only
  checkout `upstream YaLTeR/niri repository` at
  `ed22699d99462f61ab171472d3ea67e844ea580d`, confirmed from the matrix index.
- Documentation only; preserve scenario wording, profiles, approved rules,
  other WM cells, source checkouts and the user's three stashes. No live testing.
- Existing F6/L5 excluded. Newly discovered blockers retain literal TBD with
  reasons and are reclassified H/F/L.

## Acceptance and approach

- Learn the codebase once; pass its compact orientation to each fill unit.
- Trace every candidate without guessing; independently source-verify each slice.
- Record approved-rule comparisons in the preceding per-WM table format.
- Reconcile occurrence-aware scope, citations, triage ledgers and counts;
  verify unchanged scenarios and documentation-only diff scope/whitespace.
- Archive this note and publish only intended documentation.

## Bounded units

1. Orientation, pin verification and exact inventory (complete).
2. Admission/focus/columns/close/groups/layout (13 N, verified).
3. Floating/maximize/minimum-size/special (10 N + 2 U, verified).
4. Workspaces/output (8 N, verified).
5. Mouse (9 N, corrected branches independently verified).
6. Session/control/startup (1 N + 10 U, verified).
7. Independent residual/comparison audit, reconciliation, triage and archive (complete).

## Accepted evidence

- Initial workspace clean; `main` matches `origin/main` at `7ed5a19`.
- Stash identities: `81c2864883e8dfea439f37a61e6ce5ef8c77c3dc`,
  `e9f14c588daafac4fa2e58b27b6f1f2c59bd12ee`,
  `7fe6c8d19da46d162ee8b5a941fa7b352a5084e3`.
- Orientation confirmed the clean source pin and exact N41/U12/F6/L5
  inventory: `niri-baseline-inventory.json`.
- Shared map: scrolling/column, workspace/monitor/layout, floating, input
  grabs, window open classification, default KDL bindings, IPC Action inventory.
- Admission slice: 13 N closed. Independent review corrected the rejected
  update-only gesture trace: gesture completion snaps and re-anchors focus,
  rather than preserving offscreen focus. Corrected source review passed.
- State/special slice: 8 N + 2 U closed; MAX-01/MIN-02 reclassified L for
  required native frames/convergence. Born-fixed overlay exits use the
  admission-seeded `restore_to_floating`, not a from-floating-only journey.
  Independent corrections identified the removal anchor flag precisely and
  added the correct MRU source key; corrected review passed.
- Workspace/output slice: 7 N closed; OUT-06 reclassified F for unspecified
  focused node. Numeric offsets/widths and exact cleanup timing were removed
  only where Observe does not require them. Hotplug appends distinct workspace
  objects, never merges their contents; stale previous IDs remain lookup misses.
  Independent corrected-policy/count verification passed.
- Mouse slice: 4 N closed; DRAG-01/02/07 and MOU-02/03 reclassified F.
  The original single-path approach and initial review missed competing
  InColumn/Floating commits, overview targeting, initiating client click
  delivery and per-frame edge auto-scroll. That evidence rejected;
  one bounded correction round used those new causal source branches, followed
  by a fresh independent source review of every mouse cell. Corrected review
  passed. The earlier 8-closure mouse report is not accepted evidence.
- Session/control slice: 10 U closed; RST-02 reclassified H. The launcher
  starts the compositor and configured startup commands fresh-spawn; app
  relaunch participation/order/flags remain outside pinned niri source.
- All **53 candidates attempted: 32 N + 12 U closed (44)**;
  **9 N reclassified H1/F6/L2**. Original F6/L5 unchanged. niri residual
  **20 (H1/F12/L7)**; global references **579 TBD:
  N25/H76/F292/L140/U46**, down from 623. No N/U remain.
- New H: R-RST-02. New F: R-DRAG-01/02/07; R-MOU-02/03; R-OUT-06.
  New L: R-MAX-01; R-MIN-02.

## niri versus approved rules

Source keys resolve in the [matrix index](../../spec/reference-outcomes.md).
All niri locations below use `ed22699d99462f61ab171472d3ea67e844ea580d`.
These comparisons do not authorize changes to Ours. Policy differences may
exist despite in-fixture convergence; fixture-bound outcomes stay TBD.

### Comparisons requiring user review

| Case | Approved rule location | Pinned niri source evidence | Explicit deliberate-deviation coverage |
|---|---|---|---|
| FLT-01: first float uses tile+(50,50) clamped instead of centered 60% fallback | `docs/spec/functional-spec.md:180` REQ-FLT-01; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` retained placement | `src/layout/workspace.rs:1445-1468` (stored-or-default size, tile-relative offset and clamp); `S(S-nir-flttoggle)` | No explicit niri coverage; fresh-admission unfloat itself converges |
| Fixed predicate: hardcoded positive fixed height instead of configurable both/either-axis equality | `docs/spec/functional-spec.md:349` REQ-SPC-04a; `docs/../../decisions/layout-and-admission.md#fixed-size-admission-predicate` D1 | `src/window/mod.rs:377-396` (`min.h > 0 && min.h == max.h`); `S(S-nir-spc)` + `S(S-nir-fixed-open)` | No explicit niri coverage; predicate switch has no niri counterpart |
| SPC-10: fixed born-maximized opens in scrolling maximized instead of floating base beneath native overlay | `docs/spec/functional-spec.md:352` REQ-SPC-04d; `docs/../../decisions/layout-and-admission.md#fixed-size-admission-predicate` D4 | `src/layout/workspace.rs:644-669,1353-1401` (seed computed floating, pending maximum forces scrolling, restore floats); `S(S-nir-maxfs)` | No explicit niri coverage; fixed-floating exit converges, admission mechanism differs |
| SPC-11: born-fullscreen exit replays admission-seeded floating memory instead of first-exit current-hint classification | `docs/spec/functional-spec.md:353` REQ-SPC-04e; `docs/../../decisions/layout-and-admission.md#fixed-size-admission-predicate` D5 | `src/layout/workspace.rs:644-648,1288-1342` (seed and replay); `S(S-nir-fixed-open)` + `S(S-nir-maxfs)` | D5 explicitly names COSMIC deviation for game safety; no explicit niri coverage |
| WS-02: return uses live active+1 instead of separately validated remembered leaf | `docs/spec/functional-spec.md:145` REQ-WS-02; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` approved 2026-09-20 | `src/layout/scrolling.rs:999-1015`; `src/layout/monitor.rs:800-900` (mover-active Smart follow); `S(S-nir-ins)` + `S(S-nir-ws)` | No explicit niri coverage; fixture converges after explicitly focused A |
| WS-04: floated remembered C leaves active-relative sole-D admission instead of leaf/history/root fallback chain | `docs/spec/functional-spec.md:148` REQ-WS-04; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` fallback chain | `src/layout/workspace.rs:1418-1468`; `src/layout/scrolling.rs:1062-1276,999-1015` (remove and active+1); `S(S-nir-flttoggle)` + `S(S-nir-close)` + `S(S-nir-ins)` | No explicit niri coverage; no remembered-leaf store to invalidate |
| WS-17: stale departed previous ID persists as lookup-miss no-op instead of explicit scope invalidation | `docs/spec/functional-spec.md:152,165` REQ-WS-08/12i; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` 1.3/1.5, D9 | `src/layout/monitor.rs:442-495,750-785,1002-1030`; `src/layout/mod.rs:760-944` (affinity return); `S(S-nir-ws)` | No explicit niri coverage; second previous no-op and history-independent return converge |
| WS-18/20: relative send clamps at strip ends instead of wrapping scoped ring including trailing empty | `docs/spec/functional-spec.md:155,167` REQ-WS-11/14; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` 1.4/2.2 | `src/layout/monitor.rs:811-819` (`saturating_sub`, `min`); `S(S-nir-ws)` | No explicit niri coverage; target/spare and follow/stay policy otherwise converges |
| OUT-07: always-follow active-relative output admission instead of follow/stay with remembered-target fallback | `docs/spec/functional-spec.md:280` REQ-OUT-04; `docs/../../decisions/layout-and-admission.md#directional-moveoutput-transfer-and-selection` item 5 | `src/layout/mod.rs:3298-3413` (remove/insert plus `focus_output`); `src/layout/scrolling.rs:999-1015`; `S(S-nir-mon)` + `S(S-nir-ins)` | No explicit niri coverage; no native stay variant |
| DRAG-01: geometry-dependent NewColumn/InColumn commit instead of unconditional center snap-back | `docs/spec/functional-spec.md:291` REQ-DRAG-01; `docs/../../decisions/visuals.md#pointer-and-drag-behavior` closed-center resolver | `src/layout/scrolling.rs:836-901`; `src/layout/mod.rs:4262-4286` (actual column member-add); `S(S-nir-drag)` | Explicit Ours center-stack refusal selected; niri-specific branch selector remains F, not a guessed center result |
| MOU-02: dragged independent column width changes instead of shared-split neighbor reflow | `docs/spec/functional-spec.md:301` REQ-MOU-02; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` pointer resize | `src/layout/scrolling.rs:3559-3655` (`SetFixed`, no peer-width write); `S(S-nir-ptr)` + `S(S-nir-clientgrab)` | No explicit niri coverage; model-qualified difference, exact 100px share remains F |

### Comparisons already covered or OPEN

- Minimum enforcement and no automatic floating fallback align in the traced
  niri legs; Ours B6 origin+minimum/no-setting decision remains explicit
  (`functional-spec.md:238-242`). Native MIN-02 frames remain L.
- MAX-01 retains the tiled allocation in this single-tile-column fixture.
  Configure offers are compositor policy, not project-adapter geometry writes;
  native frames/convergence remain L rather than a claimed contradiction.
- D2 admission-only classification aligns; reactive SPC-05 reflow detail is
  OPEN under REQ-SPC-05b. SPC-02 eligibility remains OPEN.
- Esc cancellation is explicitly deliberate under REQ-DRAG-04 despite the
  reference drop/persist majority. Excluded DRAG-04 remains unchanged.
- GRP-03 retains its group and activates next; INS-02 ordinary admission and
  LAY-06 no saved orientation hint converge with selected rules.
- Other COL/FOC/INS details, FLT-13, OUT-05 and RST-02 are OPEN comparisons.
  WS-02 after-order remains OPEN independently of its approved anchor.
- Unsupported workspace toggles/control/owner-restart inventories have no
  applicable journeys; their closure does not contradict Ours-only selections.
- No approved rule, profile, scenario or Ours cell was edited.

## Verification and next action

- Five independent slice reviews, corrected admission/state/workspace rechecks,
  a residual/comparison audit, and fresh corrected-mouse source verification.
  All material findings corrected before acceptance.
- Exact changed-candidate set: all 53 baseline N/U occurrences. Original
  F6/L5, resolved niri cells, scenarios and all other profiles preserved.
- Post-archive integrated reconciliation: **615 executable assertions passed,
  zero failures**. Evidence: `niri-final-rigorous-checker.py`
  and `niri-final-rigorous-evidence.log`. Exact full multiline
  candidate/exclusion scope, actual twelve-WM counts and class partitions,
  N-area row/column sums, fixture memberships (292 cells across 92 rows),
  niri H/F/L ledgers, local links, pinned citation ranges and eleven comparison
  rows reconcile. Full index text outside niri legend blocks is preserved.
  Source pin/clean checkout, three stash object identities, scenarios,
  profiles and approved rules preserved; documentation-only/whitespace pass.
- Verification-plumbing repair: the initial checker skipped actual integration
  proof and checked stash names rather than object identities. Its 244-assertion
  pre-mouse-correction result is not final evidence. The repaired checker adds
  exact closure/blocker sets, all per-WM/area/fixture reconciliation, real
  triage/archive checks, object identities and corrected source-operation
  checks. Restored full non-niri index preservation
  also required, with every cited/changed niri source-key pin/path/range check after noticing
  their omission in the repaired checker. Independent manual reviews
  establish policy/comparison semantics.
- Citation cleanup names update removal versus end reinsertion precisely
  and distinguishes absent keyboard consume/expel calls from actual InColumn
  member-add. This does not change any outcome or count.
- Exact next action for this source-fill pass: none.
  No N/U areas remain; comparison review, session-source identification,
  fixture inputs and live observations remain separate user-owned follow-ups.
