# Hyprland reference source-fill

## Goal and scope

- Baseline `20cf430`: attempt Hyprland/Dwindle's 44 N and 17 U cells using
  read-only `upstream hyprwm/Hyprland repository` at
  `19fb395d45314960e6f79f17994a84094f1cd4f6`, confirmed from the matrix index.
- Documentation only. Preserve scenarios, other profiles, approved rules,
  source checkouts and the user's three stashes. No live testing.
- Existing F41/L8 excluded. Newly discovered blockers retain literal TBD
  with reasons and are reclassified H/F/L.

## Acceptance and approach

- Learn the codebase once; share the compact source map with every fill unit.
- Trace every candidate without guessing; independently source-verify each slice.
- Record approved-rule comparisons in the xmonad/awesome/bspwm table format;
  reference evidence does not approve changes to Ours.
- Reconcile occurrence-aware scope, citations, triage ledgers and counts;
  verify unchanged scenarios and documentation-only diff scope/whitespace.
- Archive this note.

## Bounded units

1. Orientation, pin verification and exact inventory (complete).
2. Admission/focus/close/layout: 8 N (verified).
3. Window states/special: 13 N + 5 U (verified).
4. Move/output/pointer/resize: 11 N (verified, including audit corrections).
5. Workspaces: 11 N + 1 U (verified, including audit corrections).
6. Session/control/startup: 1 N + 11 U (verified).
7. Comparison audit, final reconciliation, triage integration and archive (complete).

## Accepted evidence and outcome

- Workspace and pinned checkout clean; three stash identities recorded.
- Orientation reconciled 110 Hyprland TBDs: N44/U17/F41/L8. No duplicate
  candidate occurrences; MOV-01/03 main and explicit-swap legs are excluded F.
- Shared map: DwindleAlgorithm, ConfigActions, Window/WindowQuery,
  Space/Algorithm/FloatingAlgorithm, FullscreenController/Handler,
  PlacementController/Monitor, Resolver/HistoryTracker/HLWorkspace,
  DragController, Group, ConfigValues, LuaDispatchers and example config.
  Dwindle uses pointer/active anchors, long-edge binary splits and pointer-half
  order; unfloat freshly admits; pin is float-only; default workspace sends
  follow, silent sends stay; workspace previous uses global history.
- Temporary inventory: `hyprland-baseline-inventory.json`.
- Admission slice: 6 N closed, 2 N reclassified F (INS-04/06).
  Independent review traced the floating-close history self-inclusion:
  unmapped closing F is selected, then invalid-mapped focus clears to none.
  Removed the false last-node recalc claim and corrected focus-inventory cites.
- States/special slice: 7 N + 5 U closed, 3 N reclassified F (FLT-03/06,
  MAX-06), 3 L (MAX-05, MIN-02/03). Focus fencing and move refusal are
  pinned defaults; modal parent fencing is Wayland-only. Audit established
  FLT-06 focus-stays-B and MAX-06 newcomer focus plus native-restore policy;
  required geometry/client-settlement limits remain literal TBD.
- Move/output/pointer/resize slice: 6 N closed, 5 N reclassified F
  (MOV-06/12, OUT-06, MOU-02/03). Independent audit removed unasked exact
  share/step/axis TBDs from RSZ-01..03 and MOV-07. Rejected the guessed
  B-leaf tie in MOV-06; exact B-vs-C remains F under Observe.
- MOV-12 containment uses Hyprland's existing `flake.lock` hyprutils pin
  `95983ee836ff205e615bba67d1f67098e1231941`; store source narHash matched
  `sha256-YGRfhw+fJW7oKW3IspoceD34jeokN1KXNxKU6APlFH4=`. Read-only
  `src/math/Box.cpp:6,43-45` is half-open, so focal `(960,1079)` selects U2.
  Withdrawn H/enumeration blockers; only U2 contents remain F. No new pin.
- Workspace slice: 8 N + 1 U closed, 2 N reclassified F (WS-20/23),
  1 L (WS-26). Weak state/history references, active-monitor/window strong
  holds and persistent self-hold explain empty lifetime. Dead second history
  entries can recreate numeric targets. Whole-workspace moves gap-plug and
  activate only on active legs. Non-active pinned self-assignment was corrected.
  WS-20's assumed E=numeric+1 was rejected; WS-26's deferred monitor-focus
  re-track race is sourced, with remembered-origin activation secondary F.
- Session/control/startup slice: 11 U closed; RST-02 reclassified H
  (external session manager, secondary per-app flags/order). Fresh admission,
  routing/token and fresh maximum request paths traced. Rejected an invented
  store-timing L blocker for RST-03: there is no restart/dump/store journey.
- Material rejected approaches: partial policy closure counted despite literal
  TBD, fixture guesses, overprecision beyond Observe, and an external-host
  classification for lockfile-pinned containment. Independent reviews corrected
  these before acceptance. The audit's 29-N closure claim was also rejected by
  an occurrence-aware recount and MOV-06/WS-20 corrections.
- All **61 candidates attempted: 27 N + 17 U closed (44)**;
  **17 N reclassified H1/F12/L4**. Original F41/L8 unchanged. Hyprland
  residual **66 (H1/F53/L12)**; global references **666 TBD:
  N103/H74/F277/L133/U79**, down from 710. No N/U remain.
- New H: R-RST-02. New F: R-FLT-03/06; R-INS-04/06; R-MAX-06;
  R-MOV-06/12; R-OUT-06; R-MOU-02/03; R-WS-20/23.
  New L: R-MAX-05; R-MIN-02/03; R-WS-26.

## Hyprland versus approved rules

Source keys resolve in the [matrix index](../../spec/reference-outcomes.md).
All Hyprland source locations below use
`19fb395d45314960e6f79f17994a84094f1cd4f6`. These comparisons do not authorize
changes to Ours. General policy comparisons can differ even where a scenario's
geometry gives the same result; excluded cells were not changed or attempted.

### Comparisons requiring user review

| Case | Approved rule location | Pinned Hyprland source evidence | Explicit deliberate-deviation coverage |
|---|---|---|---|
| FLT-06: unfloat retains maximize through clear/re-apply instead of unmaximize-then-fresh-admit | `docs/spec/functional-spec.md:185-186` REQ-FLT-06/06b; `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays` B9 User 2026-10-08 | `src/layout/LayoutManager.cpp:32-53` (save internal mode, clear, toggle float, re-apply); `src/layout/space/Space.cpp:117-125`; `src/layout/algorithm/Algorithm.cpp:17-39,62-94`; `S(S-hyp-float)` + `S(S-hyp-fs)` | No explicit Hyprland coverage |
| OUT-06: disconnect destination is first remaining monitor rather than nearest survivor with primary/output-order fallback | `docs/spec/functional-spec.md:282` REQ-OUT-06; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` displacement policy `:193-207` | `src/state/workspace/LifecyclePolicy.cpp:95-120` (first-remaining BACKUP, return address, remembered active) + `:39-93` (same-address return); `S(S-hyp-monlife)` | No explicit Hyprland coverage; two-output fixture converges on the sole survivor and return affinity aligns |
| WS-16: removed empty ID recreated via find-or-create instead of cleared with no recreation | `docs/spec/functional-spec.md:152` REQ-WS-08; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` item 1.3 | `src/desktop/history/WorkspaceHistoryTracker.cpp:60-110` (second-position dead entry retained); `src/state/workspace/Resolver.cpp:42-53,181-191` (numbered target/history); `src/config/shared/actions/ConfigActions.cpp:150-166` (find-or-create); `S(S-hyp-ws)` | No explicit Hyprland coverage |
| WS-19/20: relative sends use MRU previous/numeric+1 next instead of a scoped ordinal ring with wrap and trailing-spare lifecycle | `docs/spec/functional-spec.md:167` REQ-WS-14; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` item 2.2 | `src/state/workspace/Resolver.cpp:181-205`; `src/desktop/history/WorkspaceHistoryTracker.cpp:40-110`; `S(S-hyp-ws)` + `S(S-hyp-movews)` | No explicit Hyprland coverage |
| WS-17/26: history survives output movement/disconnect instead of out-of-recording-scope clear and disconnected-output history discard | `docs/spec/functional-spec.md:152,165` REQ-WS-08/12i; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` item 1.5/D9 | `src/desktop/history/WorkspaceHistoryTracker.cpp:40-110` (persistent timeline, only older dead entries pruned); move/lifecycle paths do not invalidate it; `S(S-hyp-ws)` + `S(S-hyp-monlife)` | No explicit Hyprland coverage; per-ID lifecycle return-entry removal itself aligns with D9 |
| WS-21: whole-workspace move has no scope/mode/capability gate instead of strict local/global-unique gating | `docs/spec/functional-spec.md:158` REQ-WS-12b; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` migration D2 | `src/config/shared/actions/ConfigActions.cpp:1107-1135`; `src/state/workspace/PlacementController.cpp:241-258`; `S(S-hyp-ws)` | No explicit Hyprland coverage; project-specific capability model |
| WS-22/23: destination order stays in-place and source plugs first-enumerated else first-free-number instead of insert-after-current and last-remaining refill | `docs/spec/functional-spec.md:160-161` REQ-WS-12d/12e; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` migration D4/D5 | `src/state/workspace/PlacementController.cpp:259-295,302-361`; `S(S-hyp-ws)` | No explicit Hyprland coverage |
| WS-27: equal-edge ties retain first enumerated instead of left/top | `docs/spec/functional-spec.md:159` REQ-WS-12c; `docs/../../decisions/workspaces.md#workspace-lifecycle-focus-and-history` migration D3, User 2026-10-09 selection | `src/state/MonitorQueryCore.cpp:133-194` (strict-greater longest-intersection ranking); `S(S-hyp-mondir)` | No explicit Hyprland coverage; unequal-edge fixture agrees on U1 |

### Comparisons already covered or OPEN

- MIN-02/03: default-off tiled minimum enforcement versus selected B6
  origin+minimum is an explicit deliberate deviation (`docs/decisions.md:699-708`,
  `docs/spec/functional-spec.md:238-242`). Required native settlement remains L.
- MAX-05: Windows deliberately retains preimage-gate refusal despite exit
  consensus (`functional-spec.md:214-215`). MAX-07 flag-based classification
  versus deliberate Windows captionless-game inference is already recorded
  (`functional-spec.md:217-218`); that excluded cell was not attempted.
- CLOSE-04 focus-none is an OPEN comparison, not a contradiction to the
  focused-tile removal rule (`functional-spec.md:251,254`).
- MAX-02 tree allocation aligns with REQ-MAX-02; focus fencing does not
  establish a normative focus contradiction. MAX-08 focus/move suppression
  remains OPEN (`functional-spec.md:210,219`).
- WS-03's absent trailing-empty parameter and workspace-enable/owner-control/
  restart/store absences have no applicable journeys; unsupported inventories
  do not establish approved-rule contradictions.
- No approved rule, profile, scenario or Ours cell was edited.

## Verification and next action

- Five independent slice source reviews, a fresh residual/approved-rule audit
  and separate verification of audit corrections and comparison evidence.
  All material source findings corrected; evidence is policy, not live acceptance.
- Exact changed candidate set: all 61 baseline N/U occurrences. Original F41/L8,
  resolved Hyprland cells, other profiles, scenarios and approved rules preserved.
  Reference cells remain 1,896; Ours remains 169 TBD.
- Post-archive integrated reconciliation: **218 executable assertions passed,
  zero failures**. Evidence:
  `hyprland-final-rigorous-checker-v2.py` and
  `hyprland-final-rigorous-evidence-v2.log`.
  Full multiline candidate/exclusion/other-WM/scenario preservation, non-Hyprland
  index text, all actual per-WM counts/partitions, N-area sums, exact fixture
  memberships and Hyprland H/F/L ledgers reconcile. Fixture ledger totals
  277 across 92 rows; comparisons are eight rows/four columns. Touched source
  paths/ranges and local links resolve; checkout and three stash identities
  preserved, with lockfile dependency narHash reverified. Whitespace and
  documentation-only scope checks pass.
- Verification-plumbing repair: the initial checker's header/probe checks did
  not prove its full preservation/count/citation claims. Its replacement uses
  full-block parsing, durable-ledger subtraction, actual twelve-WM recount,
  exact membership checks and continuation/shorthand citation resolution;
  only the replacement's 218 assertions are accepted evidence.
- Exact next action for this source-fill pass: none. No N/U areas remain;
  comparison review, external session-manager identification/pinning, fixture
  inputs and live observations remain separate user-owned follow-ups.
