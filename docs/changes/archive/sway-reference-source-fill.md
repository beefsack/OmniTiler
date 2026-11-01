# sway reference source-fill

## Goal and scope

- Baseline `03d39ca`: attempt all sway N12/U24 cells using read-only
  `upstream swaywm/sway repository` at
  `1652c54b73f67df17b7b4ab0b0f7048204aa8104`, matching the matrix pin.
- Documentation only. Preserve scenarios, profiles, approved rules, other WM
  cells, source checkouts and three stashes. No code or live testing.
- Existing H2/F16/L17 excluded. New blockers retain TBD with reason and H/F/L
  classification. No wlroots source pass or new source pin.

## Acceptance and approach

- Shared sway/i3 orientation learned once and passed to each fill unit.
- Trace every N/U candidate; separate independent source verification per slice.
- Record approved-rule comparisons using prior per-WM table conventions.
- Reconcile exact occurrence scope, source citations, triage ledgers and totals;
  verify unchanged scenarios/profiles/rules, documentation scope and whitespace.
- Archive this note per WM.

## Bounded units

1. Shared orientation, full pins and exact inventory (complete).
2. Tree/admission/window states/special windows (7 N + 9 U, independently verified).
3. Workspaces/outputs/mouse (5 N + 3 U, corrected and independently verified).
4. Session/control/startup (12 U, independently verified).
5. Final scope/count/comparison audits, triage reconciliation and archive (complete).

## Accepted evidence

- Main initially clean and matched `origin/main` at `03d39ca`.
- Both source checkouts clean; i3 pin also confirmed as
  `903bcd518df32b0e055b17f5da3f988a0187fd3d`.
- Orientation and exact baseline inventory:
  `sway-i3-orientation.md`,
  `sway-i3-baseline-inventory.json`.
- Stash identities: `81c2864883e8dfea439f37a61e6ce5ef8c77c3dc`,
  `e9f14c588daafac4fa2e58b27b6f1f2c59bd12ee`,
  `7fe6c8d19da46d162ee8b5a941fa7b352a5084e3`.
- Tree/state slice: 7 N + 9 U closed, no reclassifications. Independent
  source review confirmed all 16 cells and legend ranges. Join appends last;
  perpendicular move inserts before the remembered child; fullscreen blocks
  ordinary newcomer focus. Evidence: `sway-slice1-review.md`.
- Rejected comparison classification: the fill proposed 14 rows, all
  convergence, OPEN or absent journeys. Independent review found no genuine
  approved-rule difference in that set; these belong in covered/OPEN notes.
- Workspace/output/mouse: 1 N + 2 U closed; 4 N + 1 U reclassified F.
  The initial trace missed criteria-targeted hidden WS2 and hotplug destroy
  listeners. Correction used those source branches. The review's proposed
  WS-17 first-WS1/second-D closure was also rejected: the per-L Given does not
  fix global seat history. Corrected F qualification was independently accepted.
  OUT-01 retains only missing output aspect, not a configurable-default gap.
  WS-03 explicit `0` creates/moves to a different workspace, not a no-op.
- Session/control: 11 U closed; RST-02 reclassified H for external app relaunch
  with secondary flags/order F. Reload is not owner restart or session recovery.
- All **36 candidates attempted: 8 N + 22 U closed (30)**; **6 reclassified
  H1/F5**. Sway residual **41 (H3/F21/L17)**; global **549 reference TBD:
  N13/H77/F297/L140/U22**. No sway N/U remain; only i3 N13/U22 remain.
- Final source/scope audit confirmed exact changes and proposed five additional
  policy comparisons. A separate check verified all five, corrected the unfloat
  citation, scoped sticky membership to the move operation, limited D1 to the
  equal-partial-zero discriminator and left wlroots internals unclaimed.

## sway versus approved rules

All source locations below use `1652c54b73f67df17b7b4ab0b0f7048204aa8104`.
Keys resolve in the [matrix index](../../spec/reference-outcomes.md). These
comparisons do not authorize changes to Ours; excluded cells remain unedited.

### Comparisons requiring user review

| Case | Approved rule location | Pinned sway source evidence | Explicit deliberate-deviation coverage |
|---|---|---|---|
| OUT-06: priority/output-order evacuation instead of nearest surviving output | `docs/spec/functional-spec.md:282` REQ-OUT-06; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` displacement policy, lines 183-207 | `sway/tree/output.c:205-257,31-57` (priority evacuation/return); `sway/input/seat.c:234-330` (destroy listeners); `S(S-sway-evac)` | No explicit sway coverage. Sole-survivor L is undiscriminating; return-to-original converges. |
| WS-17: per-seat name history with stale-name recreation and no out-of-scope clear instead of scoped stable-ID histories, removal clearing and disconnected-history discard | `docs/spec/functional-spec.md:152,165` REQ-WS-08/12i; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` items 1.2/1.3/1.5, D9 | `sway/input/seat.c:1098-1110`; `sway/commands/workspace.c:215-222`; `sway/tree/workspace.c:220-221,709-743`; `sway/tree/output.c:31-57,205-257`; `S(S-sway-ws)` + `S(S-sway-evac)` | No explicit sway coverage. Change-only recording/history-independent return converge; exact toggle targets remain F, with no invented hotplug focus/record. |
| WS-20: relative sends always stay and create no next spare instead of default-follow with stay alternative and spare lifecycle | `docs/spec/functional-spec.md:167` REQ-WS-14; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` item 2/2.2 | `sway/commands/move.c:598-608`; `sway/tree/workspace.c:314-331`; `S(S-sway-movews)` + `S(S-sway-wsretain)` | No explicit sway coverage. Wrap/attachment are shared mechanics; keeping an active empty source is not next-spare creation. |
| WS-12: criteria reaches hidden workspaces; directional migration delegates adjacency with farthest-opposite fallback instead of active-only, FULL-rect edge-touch/largest-overlap selection and no wrap | `docs/spec/functional-spec.md:159` REQ-WS-12c; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` D3 and 2026-10-09 selection | `sway/commands.c:182-199,240-300`; `sway/criteria.c:453-471,514-524`; `sway/commands/move.c:27-80,630-670`; `S(S-sway-ws)` + `S(S-sway-wsdir)` | No explicit sway coverage. R is given, not a selection proof; follow-only/same-object carry converge. wlroots internal tie policy is unpinned. |
| Fixed predicate D1: either-axis equality requires both minima nonzero instead of guarded both/either-axis with equal partial-zero counting | `docs/spec/functional-spec.md:349` REQ-SPC-04a; `docs/../../decisions/layout-and-admission.md#fixed-size-admission-predicate` D1, lines 425-435 | `sway/desktop/xdg_shell.c:228-235`; `sway/desktop/xwayland.c:310-340` (presence/equality with parent/type scope); `S(S-sway-spc)` | No explicit sway coverage. Equal-partial-zero tiles in sway but floats under D1; full-zero converges, sentinel outcomes TBD. Policy-only; excluded cells unedited. |
| FLT-01: constraint-clamped 50%/75% centered placement recomputed on every float instead of 60% first-time fallback and retained prior placement | `docs/spec/functional-spec.md:180` REQ-FLT-01; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` retained placement, lines 1336-1350 | `sway/tree/container.c:864-931,955-1024` (size/center, enable recompute, disable fresh admission); `S(S-sway-float)` | No explicit sway coverage. Fresh-admission unfloat converges. Policy-only; excluded L cell unedited. |
| SPC-11/D5: fullscreen exit clears mode without first-exit current-hint reclassification | `docs/spec/functional-spec.md:353` REQ-SPC-04e; `docs/../../decisions/layout-and-admission.md#fixed-size-admission-predicate` D5, lines 450-454 | `sway/tree/view.c:908-916`; `sway/desktop/xdg_shell.c:228-235`; `sway/desktop/xwayland.c:310-340,756-774`; `sway/tree/container.c:1260-1310`; `S(S-sway-spc)` + `S(S-sway-full)` | D5 explicitly names COSMIC deliberate deviation, with no explicit sway coverage. Previously-floating saved-geometry restore converges; born-fullscreen classification differs. Excluded L cell unedited. |
| WS-12/D7: stickies transfer as whole-workspace members instead of staying on source and never being members | `docs/spec/functional-spec.md:163` REQ-WS-12g; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` D7, lines 310-314 | `sway/tree/workspace.c:1131-1161,752-764`; `sway/tree/container.c:1705-1707`; `sway/commands/sticky.c:15-49`; `sway/input/seat.c:1209-1221`; `S(S-sway-ws)` + `S(S-sway-sticky)` | No explicit sway coverage. Move-operation membership differs; subsequent seat-switch pulls/evacuation can change final resting place. Float carry converges. |
| OUT-01: output-centre input to delegated adjacency instead of FULL-rect edge-touch and window-centre/overlap/left-top selection | `docs/spec/functional-spec.md:277` REQ-OUT-01; `docs/spec/functional-spec.md:110` REQ-MOV-08; `docs/../../decisions/layout-and-admission.md#directional-moveoutput-transfer-and-selection` item 5/5.2 | `sway/tree/output.c:316-331`; `sway/commands/move.c:168-196,277-298`; `S(S-sway-outmove)` | No explicit sway coverage. Only pinned delegation input and NULL no-edge result are asserted; wlroots internals/ties unpinned. Two-output crossing/focus/order converge, selection undiscriminated. |

### Comparisons already covered or OPEN

- GRP-02 directional joining, INS-04/06/08, MOV-06 exact index, SPC-01
  eligibility, MAX-02 focus and RST-02 session recovery are OPEN comparisons.
  Retained fullscreen tree, fixed-floating base and fresh-admission unfloat converge.
- MOU-03 tiled transfer converges at policy level; exact drop pixel remains F.
- Unsupported maximize/workspace-mode/owner-control/startup/restart journeys
  do not counter-vote Ours-only selections. Explicit `0` is not trailing-empty reuse.
- Approved rules, scenario wording, profiles and Ours cells were preserved.

## Verification and next action

- Three independent slice reviews, corrected workspace recheck, final residual/
  comparison audit and separate added-comparison source review passed.
- Integrated checker: `sway-final-checker.py`; evidence:
  `sway-final-rigorous-evidence.log`: **618 assertions passed,
  zero failures/pending checks**. Checks cover exact 36-candidate
  occurrence scope/full multiline preservation, all WM partitions/counts,
  N-area/fixture ledgers, source pins/ranges, archive links/nine comparison rows,
  scenarios/profiles/rules, checkout cleanliness and stash object identities.
- Verification-plumbing repair: the checker initially demanded all 24 U
  closures; it now verifies the actual 22 closures plus H1/F1 reclassifications.
- Exact next action for sway source fill: none. i3 N13/U22 is the last WM pass.
