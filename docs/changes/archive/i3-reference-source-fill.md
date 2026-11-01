# i3 reference source-fill

## Goal and scope

- Baseline `431039f`: attempt all i3 N13/U22 using read-only
  `upstream i3/i3 repository` at
  `903bcd518df32b0e055b17f5da3f988a0187fd3d`, matching the matrix pin.
- Documentation only; preserve scenarios, profiles, approved rules, other WM
  cells, source checkouts and three stashes. No code or live testing.
- Existing F13/L18 excluded. Blockers retain TBD with H/F/L reason.

## Acceptance and approach

- Reuse the shared sway/i3 orientation; distinguish source divergences.
- Trace all N/U candidates with independent source verification per slice.
- Record real approved-rule comparisons in the preceding table format.
- Reconcile occurrence-aware scope, citations, triage counts/ledgers and links;
  verify unchanged scenarios/profiles/rules and documentation scope/whitespace.
- Archive this note.

## Bounded units

1. Shared orientation and exact inventory (complete).
2. Tree/admission/states/special (7 N + 8 U, corrected and independently verified).
3. Workspaces/outputs (5 N + 3 U, corrected and independently verified).
4. Sticky restart/session/control/startup (1 N + 11 U, corrected and independently verified).
5. Final residual/comparison audits, triage reconciliation and archive (complete).

## Accepted evidence

- Shared orientation and original inventory:
  `sway-i3-orientation.md`,
  `sway-i3-baseline-inventory.json`. i3 text is unchanged by sway.
- Sway discoveries passed forward: criteria can reach hidden workspaces;
  hotplug focus listeners must be traced; per-output Given history can omit
  a global history projection; explicit `0` is a different target, not a no-op.
- Stash identities: `81c2864883e8dfea439f37a61e6ce5ef8c77c3dc`,
  `e9f14c588daafac4fa2e58b27b6f1f2c59bd12ee`,
  `7fe6c8d19da46d162ee8b5a941fa7b352a5084e3`.
- Tree/state/special: 5 N + 8 U closed; MAX-02/SPC-01 reclassified F.
  Independent review corrected the false non-parent-relative placement claim:
  zero-origin floats can center on the leader. Directional focus fences while
  criteria focus unblocks fullscreen, so an unstated verb is a real input gap.
  Generic newcomer-focus runtime TBDs proposed during review were rejected:
  source policy is evidenced and Observe controls required precision.
- SPC-13 native missing/unreadable/corrupt/ID mismatch paths are sourced,
  not unsupported inventory absences. Fresh start recomputes; orderly re-exec
  preserves the file. Foreign v1/partial-membership formats have no counterpart.
  A stale queued token was removed after verification found no unknown leg.
- Workspaces/output: 3 N + 2 U closed; OUT-06/WS-17/WS-27 reclassified F.
  Parser words resolve directionally at runtime, disproving WS-27's earlier
  unsupported claim. Hidden criteria migration preserves focus; empty-source
  send focuses the workspace node. Unasked numeric frames were removed.
  Global history projection remains F rather than inventing first/second targets.
- Sticky/session/control: 1 N + 10 U closed; RST-02 reclassified H.
  Sticky flag and frame survive orderly restart and same-output push. The
  initial origin F was rejected as overprecision; a stored-frame relation
  answers Observe without pixels. Dump and coordinate-guard citations added.
- All **35 candidates attempted: 9 N + 20 U closed (29)**; **6 reclassified
  H1/F5**. Original F13/L18 unchanged. i3 residual **37 (H1/F18/L18)**;
  global **520 reference TBD: N0/H78/F302/L140/U0**. No N/U remains in any WM.
- Final scope/source audit verified all latest corrections. Its rejection of
  policy-only comparisons solely for in-fixture convergence was not accepted:
  prior per-WM tables include such differences. A separate check sourced five
  additional comparisons; final audit independently rechecked all five.
  The first OUT-01 comparison used a neighboring focus/wrap path; correction
  traced `move_to_output_directed` and preserved its no-wrap convergence.

## i3 versus approved rules

All i3 locations below use `903bcd518df32b0e055b17f5da3f988a0187fd3d`.
Keys resolve in the [matrix index](../../spec/reference-outcomes.md).
Comparisons do not authorize changes to Ours. Policy-only rows leave excluded
cells unedited and do not claim missing fixture outcomes.

### Comparisons requiring user review

| Case | Approved rule location | Pinned i3 source evidence | Explicit deliberate-deviation coverage |
|---|---|---|---|
| SPC-13/D7: whole-tree file preserves orderly re-exec, but fresh start loses explicit tile; store/version/partial semantics differ from persistent intentional-float overrides | `docs/spec/functional-spec.md:355` REQ-SPC-04g; `docs/../../decisions/layout-and-admission.md#fixed-size-admission-predicate` D7 | `src/util.c:289-316`; `src/tree.c:66-123`; `src/main.c:923-937`; `src/manage.c:44-68,282-283,357-376,461-474`; `src/load_layout.c:518-534,574-575,594-595,763-764`; `S(S-i3-restart)` + `S(S-i3-min)` | No explicit i3 coverage. Fault recompute converges behaviorally; fresh-start persistence and foreign membership formats differ. |
| OUT-06: evacuation to primary/first survivor and fresh reconnect assignment instead of nearest-survivor selection and original-output return affinity | `docs/spec/functional-spec.md:282` REQ-OUT-06; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` displacement policy | `src/randr.c:80-99,456-525,860-918`; `S(S-i3-ws)` | No explicit i3 coverage. Two-output destination is undiscriminating; evacuated workspaces remain on L rather than affinity-returning. |
| WS-12: criteria-targeted hidden workspace moves remain hidden and focus-neutral instead of active-only follow migration | `docs/spec/functional-spec.md:159` REQ-WS-12c; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` D3 | `src/commands.c:143-220,1068-1115`; `src/match.c:145-189`; `src/workspace.c:1059-1160`; `S(S-i3-ws)` | No explicit i3 coverage. Ours hidden baseline has no counterpart; i3 has sourced extra reach and stay-hidden behavior. |
| WS-17: global previous name retained/recreated instead of scoped histories with removal/out-of-scope clearing, no recreation and disconnected-history discard | `docs/spec/functional-spec.md:152,165` REQ-WS-08/12i; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` items 1.1-1.5, D9 | `src/workspace.c:19,459-462,478-480,573-575,892-913`; `src/randr.c:456-525,860-918`; `S(S-i3-ws)` | No explicit i3 coverage. Reconnect never consulting history converges; exact toggle targets remain F. |
| WS-20: relative sends always stay with no next-spare lifecycle instead of default-follow with stay alternative and lifecycle-supplied spare | `docs/spec/functional-spec.md:167` REQ-WS-14; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` item 2/2.2 | `src/commands.c:263-290`; `src/con.c:1415-1464`; `src/workspace.c:131-160`; `S(S-i3-movews)` | No explicit i3 coverage. Wrap and source retention are shared mechanics, not spare creation. |
| WS-27: closest origin-coordinate with first-enumerated ties and farthest-opposite wrap instead of FULL-rect edge-touch/largest-overlap, left/top ties and no wrap | `docs/spec/functional-spec.md:159` REQ-WS-12c; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` 2026-10-09 targeting | `src/output.c:33-50`; `src/randr.c:225-246,259-319`; `S(S-i3-wsdir)` | No explicit i3 coverage. Never using window position converges; metric/wrap differ, exact U1/U2/views remain F. |
| Fixed predicate D1: both-axis equality requires both maxima positive, excluding equal partial-zero that counts in both Ours modes | `docs/spec/functional-spec.md:349` REQ-SPC-04a; `docs/../../decisions/layout-and-admission.md#fixed-size-admission-predicate` D1, lines 425-435 | `src/manage.c:461-474`; `S(S-i3-min)` | No explicit i3 coverage. Equal-partial-zero tiles in i3 but floats under D1; full-zero converges, sentinel outcomes TBD. Policy-only. |
| FLT-01: repeated float uses stored requested geometry with zero-origin centering instead of 60% first-time fallback and last-float placement retention | `docs/spec/functional-spec.md:180` REQ-FLT-01; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` retained placement, lines 1336-1350 | `src/manage.c:516-522`; `src/floating.c:329-330,364-381,419-451`; `S(S-i3-flt-toggle)` | No explicit i3 coverage. Unfloat fresh admission converges; requested geometry is not a retained last-float frame. Excluded L cell unedited. |
| SPC-11/D5: fullscreen exit clears mode without born-fullscreen first-exit current-hint admission classification | `docs/spec/functional-spec.md:353` REQ-SPC-04e; `docs/../../decisions/layout-and-admission.md#fixed-size-admission-predicate` D5, lines 450-454 | `src/manage.c:402-418`; `src/con.c:1188-1200,1209-1240,1244-1287,1295-1310`; `S(S-i3-fs)` | D5 explicitly names COSMIC deliberate deviation, with no explicit i3 coverage. Previously-floating membership retention converges; first-exit classification differs. Excluded L cell unedited. |
| WS-12/D7: stickies travel as whole-workspace members instead of staying on source and never being members | `docs/spec/functional-spec.md:163` REQ-WS-12g; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` D7, lines 310-314 | `src/workspace.c:1059-1135,565-566`; `src/con.c:491-505`; `src/output.c:87-123`; `S(S-i3-ws)` + `S(S-i3-stickyshow)` | No explicit i3 coverage. Move-operation membership differs; subsequent show/push can change final resting place. Float carry converges. |
| OUT-01: closest output-rect overlap/origin-coordinate selection instead of FULL-rect edge-touch and window-centre/overlap/left-top selection | `docs/spec/functional-spec.md:277` REQ-OUT-01; `docs/spec/functional-spec.md:110` REQ-MOV-08; `docs/../../decisions/layout-and-admission.md#directional-moveoutput-transfer-and-selection` item 5/5.2 | `src/move.c:206-253`; `src/randr.c:259-319`; `S(S-i3-outmove)` | No explicit i3 coverage. Container move uses closest-only with NULL no-op, not the neighboring focus/workspace wrap path. No-wrap and single-candidate crossing/focus converge; candidate metric differs. |

### Comparisons already covered or OPEN

- MOV-06 tails after remembered C versus sway's before-C and Ours midpoint;
  the exact Ours index is OPEN. GRP-02 directional joins, insertion sequence/
  preselection and special-window eligibility remain OPEN comparisons.
- Fullscreen keeps the tree; actual focus verb remains F. Fixed normal admission
  floats; fresh-admission unfloat converges. Sticky restart frame is relationally
  retained, without an original-workspace return slot.
- Unsupported maximize/workspace-mode/control/enable journeys do not counter-vote
  Ours-only selections. Explicit `0` is not trailing-empty reuse.
- Approved rules, scenarios, profiles and Ours/excluded cells were preserved.

## Verification and next action

- Three independent slice reviews and corrected-source rechecks completed;
  final audit verified latest cleanup/relational closure and comparison scope.
- Integrated checker `i3-final-checker.py`, evidence
  `i3-final-rigorous-evidence.log`: **730 assertions passed,
  zero failures/pending checks**. It covers all 35 occurrence scope,
  full multiline preservation, source ranges/pins, all WM partitions/counts,
  zero N-area rows/columns, exact fixture membership/ledgers, eleven comparisons,
  links, scenarios/profiles/rules, checkout cleanliness and stash object identities.
- Exact next action for these per-WM source-fill passes: none. No N/U remains
  for any reference WM; comparisons and residual H/F/L follow-ups are user-owned.
