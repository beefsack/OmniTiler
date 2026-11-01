# qtile reference source-fill

## Goal and scope

- Baseline `2287230`: attempt qtile/Columns' 37 N and 21 U cells using the
  read-only checkout `upstream qtile/qtile repository` at
  `83c697a5621306c3586efca31867efcfa0482e2d`, confirmed from the matrix index.
- Documentation only; preserve scenarios, other profiles, approved rules,
  source checkouts and the user's three stashes. No live testing.
- Existing F25/L8 excluded. Newly discovered blockers retain literal TBD
  with reasons and are reclassified H/F/L.

## Acceptance and approach

- Learn the codebase once and pass its compact source map to every fill unit.
- Trace every candidate without guessing; independently source-verify each slice.
- Record approved-rule comparisons in the preceding per-WM table format;
  reference evidence does not approve changes to Ours.
- Reconcile occurrence-aware scope, citations, triage ledgers and counts;
  verify unchanged scenarios and documentation-only diff scope/whitespace.
- Archive this note.

## Bounded units

1. Orientation, pin verification and exact inventory (complete).
2. Admission/focus/column mechanics/move: 9 N (verified).
3. Floating/maximize/fullscreen/minimize/minimum-size: 16 N + 5 U (verified).
4. Workspaces/output/groups: 8 N + 5 U (verified).
5. Session/control/startup/special windows: 4 N + 11 U (verified).
6. Residual/comparison audit, reconciliation, triage and archive (integrated).

## Accepted evidence and outcome

- Initial workspace clean; branch `main` matches `origin/main` at `2287230`.
- Three stash identities recorded: `81c2864883e8dfea439f37a61e6ce5ef8c77c3dc`,
  `e9f14c588daafac4fa2e58b27b6f1f2c59bd12ee`,
  `7fe6c8d19da46d162ee8b5a941fa7b352a5084e3`.
- Orientation reconciled 91 qtile TBDs: N37/U21/F25/L8; exact inventory
  `qtile-baseline-inventory.json`. No candidate duplicates.
- Shared map: Columns/_ClientList, Group, Screen, Qtile manager/state,
  base/X11/Wayland windows and cores, Floating, shipped default config/keys.
  Columns initially active with Max available; two-column limit, insertion
  at current, wrapped tiled-only navigation, shuffles and per-screen groups.
  Backend unspecified in the profile, preserved rather than assumed X11.
- Admission slice: 7 N closed, 2 F (INS-06/MOV-06). Rejected mover-refocus
  substitution for COL-03: explicit no consume/expel counterpart; As-Given
  `shuffle_left` is no-op. INS-06 retains backend cover/render qualifications,
  not an assumed underneath cover; newcomer group-focus is sourced.
- State slice: 8 N + 5 U closed, 3 F (FLT-06/09, MAX-01), 5 L
  (MAX-05/06, MIN-01..03). Fullscreen keeps its tiled slot while maximize
  removes it. Corrected MAX-08 to C focus plus last-column sole-window
  shuffle no-op. Minimize retains group current with bookkeeping-only
  floating focus; restore fresh-admits and focuses through the tiled layout.
- Workspace/output slice: 5 N + 5 U closed, 3 F (WS-12/17, OUT-06).
  Follow/stay, live-current return, local-only shuffle and explicit output
  transfer traced. Hotplug hides unscreened groups without evacuation merge;
  reconnect freshly assigns groups, not an origin-affinity store.
- Session/control/special slice: 2 N + 11 U closed, 1 F (RST-01),
  1 H (RST-02). X11 re-exec restores group/layout names and freshly admits
  live clients; no ordinary-float or fixed-tile override store. Wayland
  refuses restart. Typed splash/utility legs are X11/XWayland; native xdg
  has normal/dialog only. QueryTree stacking is fixture-bound, not new H.
- Material rejected approaches: counting partial policy as closure despite
  literal TBD, fixture substitutions, beyond-Observe pixel/target blockers,
  cross-backend type/restart assumptions, and QueryTree as missing-host H.
  Independent review corrected these before acceptance; no live acceptance
  inferred from synchronous source policy.
- All **58 candidates attempted: 22 N + 21 U closed (43)**;
  **15 N reclassified H1/F9/L5**. Original F25/L8 unchanged. qtile residual
  **48 (H1/F34/L13)**; global references **623 TBD:
  N66/H75/F286/L138/U58**, down from 666. No N/U remain.
- New H: R-RST-02. New F: R-FLT-06/09; R-INS-06; R-MAX-01;
  R-MOV-06; R-OUT-06; R-RST-01; R-WS-12/17.
  New L: R-MAX-05/06; R-MIN-01..03.

## qtile versus approved rules

Source keys resolve in the [matrix index](../../spec/reference-outcomes.md).
All qtile source locations below use
`83c697a5621306c3586efca31867efcfa0482e2d`. These comparisons do not authorize
changes to Ours. General policy differences can exist despite in-fixture
convergence; excluded cells were not attempted or changed.

### Comparisons requiring user review

| Case | Approved rule location | Pinned qtile source evidence | Explicit deliberate-deviation coverage |
|---|---|---|---|
| FLT-08: float-origin miss leaves F for a tile instead of retaining F | `docs/spec/functional-spec.md:188` REQ-FLT-08; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` float-nav miss | `libqtile/layout/columns.py:386-403` (tiled-column walk; floats never targets); `S(S-qti-focus)` | No explicit qtile coverage |
| FLT-09: float-origin focus never selects farther float G instead of float-only G selection | `docs/spec/functional-spec.md:189-190` REQ-FLT-09/09b; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` float-nav selection | Same tiled-column walk; exact tile target fixture-bound; `S(S-qti-focus)` | No explicit qtile coverage |
| MAX-01: maximize removes B and restore fresh-admits instead of retained-slot overlay | `docs/spec/functional-spec.md:209` REQ-MAX-01; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` maximize isolation | `libqtile/group.py:304-332` (only fullscreen skips layout removal); `libqtile/backend/base/window.py:262-285`; `S(S-qti-fsslot)` | No explicit qtile coverage |
| MAX-06: X11 born-maximized tiles ordinarily instead of Q3 reserved-slot overlay | `docs/spec/functional-spec.md:216` REQ-MAX-06; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` Q3 | `libqtile/backend/x11/window.py:636-654`; `libqtile/group.py:226-244` (no maximize admission branch); Wayland request drives state at `libqtile/backend/wayland/window.py:432-433`; `S(S-qti-fsslot)` | No explicit qtile coverage; backend-qualified |
| WS-02: return uses live column current instead of a separately validated remembered leaf | `docs/spec/functional-spec.md:145` REQ-WS-02; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` approved 2026-09-20 | `libqtile/group.py:226-244`; `libqtile/layout/columns.py:266-276,137-141` (`cc`, insert_position=0); `S(S-qti-add)` | No explicit qtile coverage; fixture converges with live A current |
| OUT-06: output loss hides groups and reconnect fresh-assigns instead of nearest-survivor evacuation and origin return | `docs/spec/functional-spec.md:282` REQ-OUT-06; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` displacement policy | `libqtile/core/manager.py:389-400,516-533`; `libqtile/config.py:578-623`; `S(S-qti-screen)` | No explicit qtile coverage; contents never merged in either policy |
| WS-12: explicit-screen view-ownership can pull a hidden group, instead of active-only scoped migration with destination insertion and verified follow | `docs/spec/functional-spec.md:156,158-162` REQ-WS-12/D2-D6; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` migration D2-D6 | `libqtile/group.py:363-393`; `libqtile/config.py:578-623` (screened-group swap or unscreened assign/hide); `S(S-qti-ws)` | No explicit qtile coverage; existing group identity itself retained |
| WS-17: screen-object previous-group references survive scope changes instead of moved/out-of-scope ID invalidation and disconnected-history discard | `docs/spec/functional-spec.md:152,165` REQ-WS-08/12i; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` items 1.3/1.5, D9 | `libqtile/config.py:578-596`; `libqtile/core/manager.py:516-533`; `S(S-qti-ws)` + `S(S-qti-screen)` | No explicit qtile coverage; exact reconnect assignment remains fixture-bound |
| RST-01: X11 restart re-tiles intentional ordinary F instead of preserving its float identity | `docs/spec/functional-spec.md:331-332` REQ-RST-01/01b; `docs/../../decisions/persistence.md#orderly-restart-adoption` | `libqtile/core/state.py:23-59` (no per-window store); `libqtile/group.py:226-244`; `libqtile/layout/floating.py:14-30`; `S(S-qti-state)` + `S(S-qti-rstadmit)` | No explicit qtile coverage; width reset is not a contradiction (no layout restoration selected) |
| SPC-13: X11 restart loses explicit fixed-window tile override instead of D7 persistence | `docs/spec/functional-spec.md:355` REQ-SPC-04g; `docs/../../decisions/layout-and-admission.md#fixed-size-admission-predicate` D7 | `libqtile/core/state.py:23-59`; `libqtile/layout/floating.py:14-30` (rules recomputed at re-admit); `S(S-qti-rstadmit)` + `S(S-qti-spc)` | No explicit qtile coverage; Wayland has no restart journey |

### Comparisons already covered or OPEN

- MIN-01..03 no-enforce versus B6 origin+minimum is explicitly deliberate
  (`docs/decisions.md:699-708`, `functional-spec.md:238-242`); required native
  client settlement remains L. No no-enforce setting selected.
- MAX-05 Windows preimage-gate refusal is explicitly retained despite native
  exit consensus (`functional-spec.md:214-215`); required client completion L.
- MAX-02 allocation retention aligns with REQ-MAX-02. FLT-06 converges on
  B9's unmaximized fresh-tile state; native clear observation is project policy.
- COL/FOC/INS/MOV details, MAX-08, MNZ-01..03, SPC-02 and RST-02 remain
  OPEN product comparisons. WS-02 after-order itself is OPEN (REQ-WS-02b).
- Absent sticky/workspace-toggle/tab-stack/snap/directional-group/control
  inventories have no corresponding journeys, not contradictory outcomes.
- No approved rule, profile, scenario or Ours cell was edited.

## Verification and next action

- Four independent slice source reviews plus a fresh residual/comparison
  audit; all material findings corrected before acceptance.
- Exact changed-candidate set: all 58 baseline N/U occurrences. Original
  F25/L8, resolved qtile cells, scenarios and all other profiles preserved.
- Post-archive integrated reconciliation: **818 executable assertions passed,
  zero failures**. Evidence: `qtile-final-rigorous-checker.py`
  and `qtile-final-rigorous-evidence.log`. Exact candidate and
  full multiline preservation, actual twelve-WM counts/partitions, N-area
  row/column sums, exact fixture memberships, qtile H/F/L, links and pinned
  source ranges reconcile. References 1,896 cells / 623 TBD; Ours 169 TBD;
  fixture ledger 286 cells across 92 rows. Source checkout and stash identities
  preserved; documentation-only scope and whitespace checks pass.
- Verification-plumbing repair: the first checker had tautological/global
  existence checks and incomplete N-area/fixture reconciliation. Removed those
  as semantic proof, added exact row/column/class/membership checks and cited
  source-slice operation checks. INS-06 stacking evidence was extended in
  `S-qti-maxfloat`; only the repaired checker is accepted executable evidence.
  Policy/comparison conclusions also have independent manual source review.
- Exact next action for this source-fill pass: none.
  No N/U areas remain; comparison review, session-source identification,
  fixture inputs and live observations remain separate user-owned follow-ups.
