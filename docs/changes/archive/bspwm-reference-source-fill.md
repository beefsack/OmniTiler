# bspwm reference source-fill

## Goal and scope

- Baseline `2028348`: attempt bspwm's 42 N and 20 U cells using read-only
  `upstream baskerville/bspwm repository` at
  `e11eff4cb3333216ad03c815609a4ed79e08929c`, confirmed from the matrix legend.
- Documentation only. Preserve scenarios, other profiles, approved rules,
  source checkouts and the user's three stashes. No live testing.
- Existing F20/L8 excluded. Newly discovered blockers retain literal TBD
  with reasons and are reclassified H/F/L.

## Acceptance and approach

- Learn the codebase once; share the compact source map with every fill unit.
- Trace every candidate without guessing; independently source-verify each slice.
- Record material approved-rule comparisons with rule/source locations, using
  the xmonad/awesome handover format. Reference evidence does not approve rules.
- Reconcile occurrence-aware coverage, citations, triage ledgers and counts;
  verify unchanged scenarios and documentation-only diff scope/whitespace.
- Archive this note.

## Bounded units

1. Orientation, pin verification and exact inventory (complete).
2. Admission/focus/close/floating: 12 N + 2 U (verified).
3. States/geometry/groups/special: 13 N + 5 U (verified).
4. Move/output/pointer: 9 N + 1 U (verified).
5. Workspaces/session/control/startup: 8 N + 12 U (verified).
6. Corrected-policy/approved-rule comparison audit (complete).
7. Final reconciliation, triage integration and archive (complete).

## Accepted evidence and outcome

- Workspace clean on `main`, tracking `origin/main`; pinned checkout clean.
  Three stash object identities recorded for final comparison.
- Orientation reconciled 90 bspwm TBDs: N42/U20/F20/L8. No duplicate
  candidate occurrences. Full pin verified against the matrix legend.
- Shared map: `src/tree.c`, `src/window.c`, `src/history.c`, `src/geometry.c`,
  `src/settings.c`, `src/messages.c`, `src/desktop.c`, `src/monitor.c`,
  `src/pointer.c`, `src/events.c`, `src/rule.c`, `src/query.c`, `src/restore.c`,
  `src/bspwm.c`, `examples/sxhkdrc`.
  Longest-side binary insertion, second-child polarity, history refocus,
  geometric directional leaf swap with follow, no-follow desktop send;
  fullscreen vacates the retained tree leaf, sticky has no float guard.
  No maximize state, tab group or workspace tiling-off counterpart.
- Temporary orientation evidence: `bspwm-pass-orientation.json`.
- Slice 1: 8 N + 2 U closed; 4 N reclassified F (FOC-03/04,
  INS-04/06). Review removed unnecessary absolute-frame/live blockers,
  completed cycle/internal-focus and fullscreen-newcomer chains. A fresh
  independent review verified all corrected residuals against the pin.
- Slice 2: 11 N + 5 U closed; SPC-07 reclassified F for the whole-tuple
  X11 hint encoding of height absent. Review corrected the distinction
  between leaf hint clamping and internal 32-based fence constraints.
  Restored literal TBD after the verifier initially counted the F guard closed.
  A separate audit restored MIN-02 as L because Observe requires native
  shrink/grow frames and settlement, beyond the deterministic arrange path.
- Slice 3: 8 N + 1 U closed; MOV-13 reclassified F for unrecorded U
  contents/focus (occupied swap vs empty no-op). Review corrected focus
  markers. Later audit made MOV-03 explicitly an in-order leaf projection
  with native binary grouping retained, and DRAG-07 swaps motion-time rather
  than release-time (release only ends the grab).
- Slice 4: 6 N + 12 U closed; WS-04 reclassified F, RST-02 H.
  Review rejected fabricated pixel/history limits, traced hidden-desktop
  migration and empty-destination/empty-source focus. A correction traced
  WS-04 vacant-collapse: B follows C in the binary tree but takes C's full
  prior tile slot while C floats; D is unchanged. Only the split axis needs
  C's saved rectangle/output geometry. Fresh verification confirmed it.
- RST-02 deterministic policy is fully traced: startup restore is `-s`-gated,
  internal IDs regenerate but client leaves retain live X IDs; orphan adoption
  is manual `wm -o`, not a startup rematch. Cross-session relaunch remains
  primary H with per-app flags/order secondary F. Restore citations extended
  for fresh admission, ID preservation and manual orphan adoption.
- Material rejected approaches: partial policy tracing was repeatedly reported
  as whole-cell closure despite literal TBD; some initial F/L claims hid
  unfinished source paths or asked for precision beyond Observe. Independent
  reviews completed those chains and counted cells only after all required
  outcomes were established. MIN-02's actual native-response requirement was
  restored rather than dropping its TBD to inflate yield.
- All **62 candidates attempted: 33 N + 20 U closed (53)**; **9 N
  reclassified H1/F7/L1**. Original F20/L8 unchanged. bspwm residual
  **37 (H1/F27/L9)**; global references **710 TBD:
  N147/H73/F265/L129/U96**, down from 763.
- New H: R-RST-02. New F: R-FOC-03/04; R-INS-04/06;
  R-MOV-13; R-SPC-07; R-WS-04. New L: R-MIN-02.
  All residuals keep literal TBD with named reasons; no N/U remain.
- Twelve existing `S-bsp-*` legend entries extended at the existing pin;
  no new source revision. Structural check caught an out-of-bounds monitor
  citation (459-560); corrected mechanically to 459-558.

## bspwm versus approved rules

Source keys resolve in the [matrix index](../../spec/reference-outcomes.md).
All source locations below use `e11eff4cb3333216ad03c815609a4ed79e08929c`.
These are reference differences, not authorization to change Ours. Rows naming
excluded scenarios compare policy only; those cells were not changed or attempted.

### Comparisons requiring user review

| Case | Approved rule location | Pinned bspwm source evidence | Explicit deliberate-deviation coverage |
|---|---|---|---|
| FLT-01: unfloat restores the same vacant slot instead of fresh admission | `docs/spec/functional-spec.md:180` REQ-FLT-01; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` option A 2026-09-28 | `src/tree.c:1945-1961` (`set_floating` vacant in place, no focus write); `S(S-bsp-float)` + `S(S-bsp-state)` | No explicit bspwm coverage |
| MAX-02: focusing another tile clears a covered fullscreen to last state instead of retaining the overlay while focus enters/leaves | `docs/spec/functional-spec.md:210` REQ-MAX-02; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` fullscreen rule | `src/tree.c:488-493` (activate focus-change clear) + `:599-602` (`focus_node` clear) + `:1957`,`:1981-2004` (`set_floating`/`set_fullscreen`/`neutralize_occluding_windows`); `S(S-bsp-fs)` + `S(S-bsp-state)` | No explicit bspwm coverage |
| WS-01/02/04 and OUT-07: sends insert at the live destination focus with shipped default-stay plus `--follow` flag, instead of remembered-leaf/focus-history/root admission with default follow and unbound stay | `docs/spec/functional-spec.md:143-145,148,280` REQ-WS-01/01b/02/04, REQ-OUT-04; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` fallback chain + item 2; `docs/../../decisions/layout-and-admission.md#directional-moveoutput-transfer-and-selection` 5.4 | `src/tree.c:1629-1652` (`transfer_node` at `dd->focus`) + `:291-380` (`insert_node` longest-side second-child split) + `:1945-1961` (float vacant, no focus write); `src/messages.c:171-261` (`--follow` dispatch); `S(S-bsp-send)` + `S(S-bsp-xfer)` + `S(S-bsp-insert)` + `S(S-bsp-wskeys)` | No explicit bspwm coverage |
| OUT-06: disconnected monitor/desktops retained in place with no evacuation or focus write (same RandR id reuses them on reconnect), instead of displacing workspaces aside with auto-return; `remove-unplugged` merges into the survivor instead of preserving the displacement | `docs/spec/functional-spec.md:282` REQ-OUT-06; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` displacement policy | `src/settings.h:67-69` (`remove-unplugged`/`merge-overlapping` default false) + `src/monitor.c:459-493` (same-RandR-id reuse), `:527-538` (unplugged merge before remove), `:286-298` (`merge_monitors` transfers all desktops); `S(S-bsp-monrm)` | No explicit bspwm coverage |
| Sticky: sticky sets on tiled members (no float-only guard) with focus-path desktop follow, instead of sticky via all-desktops for intentional floats | `docs/spec/functional-spec.md:181` REQ-FLT-02; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` | `src/tree.c:2151-2184` (`set_sticky`, no float-only guard) + `:520-536` (`transfer_sticky_nodes`) + `:588-598` (desktop-switch focus-path sticky follow); `S(S-bsp-sticky)` + `S(S-bsp-state)` | No explicit bspwm coverage |
| MOV-08/11/12 and OUT-01/04: cross-output moves are leaf swaps by boundary distance then history rank requiring a leaf target (empty output is a no-op), instead of transfer crossing including sole-root leaves to empty targets with centre-projection/overlap/left-top output selection | `docs/spec/functional-spec.md:110-111,277,280` REQ-MOV-08/08b, REQ-OUT-01/04; `docs/../../decisions/layout-and-admission.md#directional-moveoutput-transfer-and-selection` item 5/5.2 | `src/tree.c:1124-1149` (directional leaf candidates, distance then history rank) + `:1489-1620` (leaf exchange, same-desktop retain vs cross-monitor follow) + `src/geometry.c:49-154` (boundary distance) + `src/history.c:311-323` (MRU rank); `S(S-bsp-move-target)` | Item 5.2 selects the window-based output metric; no explicit bspwm leaf-metric coverage |
| DRAG-01/07: per-motion hover swap during motion with release only ending the grab (pause writes nothing, no zones/preview/placeholder), instead of the selected drop resolver | `docs/spec/functional-spec.md:291,298` REQ-DRAG-01/07; `docs/../../decisions/visuals.md#pointer-and-drag-behavior` option A 2026-09-27 | `src/window.c:487-519` (`move_client` tiled hover-swap vs float move) + `src/pointer.c:248-307` (ACTION_MOVE grab/track, button-release end only) + `src/events.c:40-89` (no key-cancel branch); `S(S-bsp-drag)` + `S(S-bsp-flt-swap)` | No explicit bspwm coverage |
| SPC-07: raw whole-size equality floats zero/partial-zero/sentinel hints without validity guards (absent correctly tiles), instead of the selected guarded predicate | `docs/spec/functional-spec.md:349` REQ-SPC-04a; `docs/../../decisions/layout-and-admission.md#fixed-size-admission-predicate` D1 User 2026-10-08 | `src/rule.c:285-299` (`_apply_hints`: `P_MIN_SIZE`/`P_MAX_SIZE` flag gate plus whole-size equality, no zero/sentinel guard); `S(S-bsp-spc)` | D1 selects the guards, but no explicit bspwm-deviation label |

### Comparisons already covered

- MIN-01..03: default-off unclamped tiling with opt-in leaf hint clamping
  versus B6 origin+minimum. Explicit deviation: `docs/decisions.md:699-708`,
  REQ-MIN-01..03.
- MAX-05: no-refusal EWMH/project-toggle convergence (`src/tree.c:1889-1987`,
  `src/events.c:474-490`, `S-bsp-fs`). Windows deliberately retains preimage
  refusal despite exit consensus (`functional-spec.md:214-215`).
- MAX-07: captionless cover tiles absent the fullscreen atom (`S-bsp-admit`).
  Explicit Windows game-compatibility inference difference
  (`functional-spec.md:217-218`).
- MOV-03: leaf swap aligns with the `swap-with-neighbor` setting alternative
  (REQ-MOV-03); default remains `group-with-neighbor`.
- CLOSE-01: source-domain MRU refocus alignment, not a difference
  (`src/tree.c:1337-1474`, `src/history.c:171-180`, `S-bsp-close`).
- RSZ-01: fenced pixel-share change, reversibility and clamping align
  (`src/window.c:547-590`, `src/tree.c:1003-1025`, `S-bsp-resize`).
- OPEN outcomes, owner-specific controls and unsupported verbs do not
  establish approved-rule contradictions. No rule or scenario was edited.

## Verification and next action

- Four independently authored slice source reviews, fresh corrected-policy
  and fixture audit, separate final-correction and rule-comparison verification.
  All material review findings corrected; source policy is not live acceptance.
- Exact changed occurrence set is all 62 baseline N/U candidates. Original
  F20/L8, resolved bspwm cells, other profiles, scenarios and approved rules
  are preserved. Reference cells remain 1,896; Ours remains 169 TBD.
- Temporary executable evidence: `bspwm-pass-final-reconcile.py`
  and `bspwm-pass-final-reconcile.json`. Post-archive integrated
  reconciliation: **77 assertions passed, zero failures**, independently rerun
  after integration. The final reviewer also reconciled the baseline durable
  N/U/F/L inventory, all index profile/notation text, exact eight-row/four-column
  comparison table and ranked secondary-barrier totals (16, leaving 131 N).
- Every per-WM partition/actual count, N-area row/column sum and exact bspwm
  H/F/L ledger reconciles. Fixture ledger totals 265 across 91 rows; other
  WM memberships unchanged. Touched citations and local links resolve;
  source pin/clean checkout and three stash object identities preserved.
  Whitespace and documentation-only diff scope checks pass.
- Exact next action for this source-fill pass: none. No N/U areas remain;
  comparison review, external
  session-manager identification/pinning, fixture inputs and live observations
  remain separate user-owned follow-ups.
