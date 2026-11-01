# Remaining reference source-fill triage

Archived 2026-10-10: all twelve source-fill passes are complete. This is a
residual classification ledger, not an active source-fill plan. Counts below
are the source-pass completion snapshot, not a fresh recount after later
implementation or matrix maintenance.

## Follow-up decision 2026-10-10

- User deferred both GNOME Shell/Mutter/wlroots host-source pinning and
  fixture/scenario wording tightening (302 cells across 92 rows).
- Revisit wording only when a specific open decision depends on such a row.
- The former host-source estimate of 19 cells (17 PaperWM, 2 sway) preceded
  21 PaperWM N-to-H reclassifications. The snapshot now has 40 candidates
  (38 PaperWM Shell/Mutter, 2 sway wlroots), not promised whole-cell closures.
  Other residual host limits are unchanged; no source was pinned.
- Product comparisons were reviewed and selected separately on 2026-10-10;
  older per-pass "user review" next-action text below is historical provenance.
  [Durable decision](../../decisions/reference-and-shortcuts.md#reference-outcome-authority).
- Exact next action for this completed source-fill plan: none. Deferred
  follow-ups and user-owned runtime checks are separate.

## Goal and scope

Triage all remaining reference TBD cells by WM to select high-yield,
context-bounded source passes. The initial triage classified existing evidence;
the counts and ledgers now include the completed PaperWM, karousel, paneru,
COSMIC, xmonad, awesome, bspwm, Hyprland, qtile, niri, sway and i3 source passes recorded in
[PaperWM's outcome note](paperwm-reference-source-fill.md)
and [karousel's outcome note](karousel-reference-source-fill.md), plus
[paneru's outcome note](paneru-reference-source-fill.md) and
[COSMIC's outcome note](cosmic-reference-source-fill.md) and
[xmonad's outcome note](xmonad-reference-source-fill.md) and
[awesome's outcome note](awesome-reference-source-fill.md) and
[bspwm's outcome note](bspwm-reference-source-fill.md) and
[Hyprland's outcome note](hyprland-reference-source-fill.md) and
[qtile's outcome note](qtile-reference-source-fill.md) and
[niri's outcome note](niri-reference-source-fill.md) and
[sway's outcome note](sway-reference-source-fill.md) and
[i3's outcome note](i3-reference-source-fill.md). No scenarios
were tightened. Documentation only; no live
testing, cloning, new source pins, source-checkout changes, or stash changes.
Ours is counted separately and is out of scope for reference source tracing.

## Initial triage acceptance and approach

- Reconcile every reference TBD cell to one primary blocker class, using
  existing cell/row/profile text and light pinned-source inspection for ambiguity.
- Report per-WM totals/classes, not-attempted area splits, missing-host-source
  and fixture-gap aggregates, and ranked expected source-pass yields.
- Independently spot-check a classification sample.
- Verify coverage, counts, unchanged matrix/scenarios, diff scope and whitespace;
  publish this report only.

## Initial triage bounded units

1. Inventory plus one bounded per-WM classification unit at a time.
2. Reconcile aggregates and rank per-WM passes split by area.
3. Independent spot-check, report integration, checks and publication.

## Current evidence and classification method

- Initial triage baseline: `f4ad288`, 2026-10-09, committed as `abf7f26`.
  All 1,006 TBD reference cells out of 1,896 classified; triage alone closed
  zero cells. Current: **520 reference TBD**, after 42 PaperWM, 37 karousel,
  51 paneru, 29 COSMIC, 48 xmonad, 36 awesome, 53 bspwm, 44 Hyprland,
  43 qtile, 44 niri, 30 sway and 29 i3 closures.
  **Ours: 169 TBD**, excluded.
- Twelve reference profiles, as pinned in the [matrix index](../../spec/reference-outcomes.md#wm-profiles-and-config-assumptions).
  KWin is not a separate column. Karousel's host KWin is already pinned at
  `8438567a` (`S-kwin-resizeable`, `S-kwin-tabbox`, `S-kwin-act`); unfinished
  tracing at that revision is not missing-host-source.
- Each cell counts once if it contains TBD, regardless of the number of TBD
  tokens. Separate explicit-swap cells under MOV-01/MOV-03 count separately.
  Five repeated WM/area/row triples give 1,006 cells but 1,001 distinct triples:
  Hyprland MOV-01/03, PaperWM MOV-01/03, paneru MOV-01.
- Each remaining cell read with its Given/When/Observe, global profile and
  existing notes; ambiguous cases received light source inspection, not full
  tracing. Existing source citations support triage but do not prove closure.
  A primary class identifies the selected remaining blocker; compound cells
  retain secondary limitations in the yield qualification below.
- **N, not-attempted:** the unresolved leg has not received adequate tracing
  and is likely answerable from pinned source. A partially sourced cell can
  still be N; the label concerns its remaining leg, not absence of source tags.
- **H, missing-host-source:** decisive policy resides in an unpinned external
  codebase, named below. Already-pinned dependencies do not qualify.
- **F, fixture-gap:** scenario/profile/action wording omits an input the code
  depends on, named below. Observe controls required precision.
- **L, live-only:** remaining outcome depends on runtime timing, client
  response or observation, rather than an unfinished deterministic policy trace.
- **U, unsupported-pending:** cited inventories establish no in-profile
  counterpart; explicit unsupported-outcome wording is the cheap closure.
- Reusable prior evidence: six preceding source groups reduced TBDs
  1,050 -> 1,006 (44 net); their detailed changes remain in git history.
  Qtile minimize retains group current with floating-layout bookkeeping;
  restore fresh-admits and focuses through the tiled layout when current.
  PaperWM selected-window
  reinsertion is now traced: selected+1 RIGHT; host-selected anchors remain
  qualified where needed. PaperWM has native live-alt-tab; do not repeat
  the old absence claim.
  Preserve unknown user-tested versions and the distinction between policy
  evidence and runtime acceptance. Approved deliberate deviations remain intact.
- Source access: some COSMIC/Hyprland/bspwm temporary exports were
  empty or mismatched and were not accepted as pinned evidence. Existing
  pinned citations used, sibling checkouts verified where available, and
  pinned raw files used for light checks. No sources were cloned or newly pinned.

## Per-WM summary

| WM/profile | TBD | N | H | F | L | U |
|---|---:|---:|---:|---:|---:|---:|
| COSMIC | 53 | 0 | 1 | 33 | 19 | 0 |
| Hyprland/Dwindle | 66 | 0 | 1 | 53 | 12 | 0 |
| bspwm | 37 | 0 | 1 | 27 | 9 | 0 |
| i3 | 37 | 0 | 1 | 18 | 18 | 0 |
| xmonad/Tall+Navigation2D | 27 | 0 | 1 | 18 | 8 | 0 |
| sway | 41 | 0 | 3 | 21 | 17 | 0 |
| qtile/Columns | 48 | 0 | 1 | 34 | 13 | 0 |
| awesome/tile | 53 | 0 | 1 | 36 | 16 | 0 |
| niri | 20 | 0 | 1 | 12 | 7 | 0 |
| PaperWM | 61 | 0 | 38 | 14 | 9 | 0 |
| karousel/Lazy | 36 | 0 | 1 | 27 | 8 | 0 |
| paneru | 41 | 0 | 28 | 9 | 4 | 0 |
| **Total references** | **520** | **0** | **78** | **302** | **140** | **0** |

## Not-attempted by area

All files are under `docs/spec/reference-outcomes/`. Abbreviations used here
and in the fixture ledger: COS=COSMIC, HYP=Hyprland/Dwindle, BSP=bspwm,
I3=i3, XMO=xmonad/Tall+Navigation2D, SWY=sway, QTI=qtile/Columns,
AWE=awesome/tile, NIR=niri, PAP=PaperWM, KAR=karousel/Lazy, PAN=paneru.

| Area file | COS | HYP | BSP | I3 | XMO | SWY | QTI | AWE | NIR | PAP | KAR | PAN | Total |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| activation.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| close.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| column-mechanics.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| floating.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| focus.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| groups-stacks.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| insertion.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| layout-commands.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| maximize-fullscreen.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| minimize.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| minimum-size.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| mouse.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| move.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| multi-output.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| resize.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| restart-persistence.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| special-windows.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| workspaces.md | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| **Total** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** |

No duplicate remaining N cells. PaperWM MOV-01/03 and paneru MOV-01
main and explicit-swap cells are now closed. Both Hyprland MOV-01/03 legs
are F, not N.

## Missing-host-source aggregate

Counts are unique **primary H cells per external source group**, indicating
source legs that could be unlocked, not guaranteed whole-cell completion.
GNOME Shell includes its `org.gnome.shell.app-switcher` schema; Shell/Mutter
are grouped because several PaperWM cells span both. Their union is now 38
(17 existing plus 21 reclassified after extension tracing). The other primary
H cells are sway 3, paneru 28, karousel 1, COSMIC 1, xmonad 1, awesome 1, bspwm 1, Hyprland 1, qtile 1, niri 1 and i3 1:
total 78.

| External codebase/platform | Cells | WMs | Missing policy | Suggested revision approach, no pin performed |
|---|---:|---|---|---|
| GNOME Shell/Mutter, including app-switcher schema | 38 | PaperWM | Workspace lifecycle/session wiring, activation/urgency/refocus, native move/resize, sticky/float/overlay visibility and stacking, startup stacking/tab-list policy, type/fence behavior, hotplug and output-neighbor tie order | Select a PaperWM-compatible GNOME release; record exact paired Shell/Mutter distro revisions/patches and deployed build; freeze `current-workspace-only` value/schema. |
| wlroots | 2 | sway | Adjacent-output equal-distance tie selection | Read sway `1652c54b` dependency/build declarations; pin the exact compatible deployed wlroots revision, not unrelated latest. |
| macOS proprietary host platform | 28 | paneru | WindowServer/native window policy (26), menu bar/NSStatusItem (1), native switcher (1) | No public implementation source to pin. Record macOS build, SDK and documented AX/CoreGraphics/AppKit contracts; opaque host policy still needs later observation. A new source pin cannot promise these closures. |
| Plasma session manager (ksmserver), plus app session participation | 1 | karousel | Post-session restored app set/order, rematching and layout recovery; karousel's fresh adoption is traced | Select the deployed Plasma-compatible ksmserver revision and app/session restore inputs; KWin `8438567a` is already pinned and is not this missing dependency. |
| COSMIC session manager (cosmic-session), plus app session participation | 1 | COSMIC | Session relaunch set/order and per-app restored flags; compositor recreates pinned workspace shells and freshly admits clients | Pin the deployed cosmic-session revision and session/app restore inputs; `COSMIC_SESSION_SOCK` delegates outside the pinned compositor. |
| External X session manager (identity not recorded), plus app session participation | 1 | xmonad | Post-login app relaunch set/order and restored flags; xmonad's resume-only StateFile is not cross-session app recovery | Identify and pin the deployed X session manager and session/app restore inputs; xmonad scans and freshly admits live clients, with fullscreen requiring a new ClientMessage. |
| External X session manager (identity not recorded), plus app session participation | 1 | awesome | Post-login app relaunch set/order and restored flags; atexit client order and floating X properties cover in-place re-exec, not app relaunch | Identify and pin the deployed X session manager and session/app restore inputs; awesome freshly admits clients and reads their new EWMH flags. |
| External X session manager (identity not recorded), plus app session participation | 1 | bspwm | Post-login app relaunch set/order and restored flags; dump/load retains live X window IDs and has no cross-session identity rematching | Identify and pin the deployed X session manager and session/app restore inputs; bspwm freshly admits new clients, with fullscreen depending on relaunch flags. |
| External session manager (identity not recorded), plus app session participation | 1 | Hyprland | Post-login app relaunch set/order and restored flags; config reload has no window/layout store or session identity rematching | Identify and pin the deployed session manager and session/app restore inputs; Hyprland freshly admits clients, with maximum/fullscreen requiring fresh client requests. |
| External session manager (identity not recorded), plus app session participation | 1 | qtile | Post-login app relaunch set/order and restored flags; QtileState covers in-place X11 re-exec, not cross-session app recovery | Identify and pin the deployed session manager and session/app restore inputs; qtile freshly admits clients from new flags, with no per-window layout/intent store. |
| External session manager (identity not recorded), plus app session participation | 1 | niri | Post-login app relaunch set/order and restored flags; niri-session launches the compositor and startup commands fresh-spawn, with no app/layout recovery store | Identify and pin the deployed session manager and session/app restore inputs; niri freshly admits clients, with native maximum/fullscreen requiring new client flags. |
| External session manager (identity not recorded), plus app session participation | 1 | sway | Post-login app relaunch set/order and flags; reload is config-only with no layout recovery store | Identify and pin the deployed session manager and session/app restore inputs; sway freshly admits clients, with fullscreen requiring fresh flags. |
| External X session manager (identity not recorded), plus app session participation | 1 | i3 | Post-login app relaunch set/order and flags; layout file covers in-place re-exec, not fresh-login app recovery | Identify and pin the deployed session manager and session/app restore inputs; i3 freshly manages clients, with fullscreen requiring fresh flags. |

Primary H cell ledger (IDs below retain their `R-` prefix):

- GNOME Shell/PaperWM: R-CLOSE-04; R-FLT-14; R-MNZ-03; R-MOU-01;
  R-CTL-03; R-RST-02; R-WS-07/09/16/17/18/20/23.
- Mutter/PaperWM: R-FLT-14; R-DRAG-08; R-MOU-01; R-MOV-12;
  R-SPC-01; R-WS-17/27.
- Newly primary H/PaperWM (21, Shell/Mutter union): R-ACT-01/02;
  R-CLOSE-05; R-FLT-02/05/12/13; R-GRP-03; R-MAX-09;
  R-MNZ-01/02; R-DRAG-03; R-MOU-02; R-OUT-06; R-START-01/02/03;
  R-SPC-02; R-WS-05/24/25. These extension legs are traced; the cells
  name their remaining host policy. Startup cells retain secondary viewport
  fixture limits; R-START-03 also retains client-settlement limits.
- wlroots/sway: R-MOV-12; R-WS-27 (`wlr_output_layout_adjacent_output`
  after pinned sway output-direction delegation).
- macOS WindowServer/native policy/paneru: R-CLOSE-05; R-FLT-02/05/06/14;
  R-MAX-01/04/06/08/09; R-DRAG-08; R-MOU-01; R-MOV-12; R-OUT-06;
  R-RST-02; R-SPC-11. Menu bar: R-CTL-03. Switcher: R-WS-07.
- Newly primary H/paneru (10): R-ACT-01/02; R-CLOSE-02/03;
  R-COL-10; R-FLT-13; R-INS-06; R-MNZ-03; R-RST-01; R-SPC-13.
  WM policy legs are traced; remaining macOS activation/urgency, native
  tabs/Space, stacking, close/minimize and restart focus policy is opaque.
  R-RST-01/SPC-13 retain secondary live-frame fixture and settlement limits.
- Plasma session manager/karousel: R-RST-02 (startup re-adoption traced;
  session-restored app participation/order and rematching remain unpinned).
- COSMIC session manager/COSMIC: R-RST-02 (pinned workspace shells and fresh
  admission traced; session relaunch participation/order/flags remain unpinned).
- External X session manager/xmonad: R-RST-02 (runtime-resume StateFile and
  fresh startup scan/admission traced; session relaunch remains outside the
  pinned WM, with secondary app-participation/order/flags fixture limits).
- External X session manager/awesome: R-RST-02 (in-place order/floating
  persistence and fresh startup scan/admission traced; session app relaunch
  remains outside the pinned WM, with secondary order/flags fixture limits).
- External X session manager/bspwm: R-RST-02 (same-session dump/re-exec,
  retained client X IDs, fresh admission and manual-only orphan adoption
  traced; session app relaunch remains outside the pin, with secondary
  per-app flags/order fixture limits).
- External session manager/Hyprland: R-RST-02 (config-only reload, fresh
  map-time admission/routing and fresh client maximum/fullscreen traced;
  session relaunch outside the pin, with secondary app flags/order limits).
- External session manager/qtile: R-RST-02 (in-place-only X11 QtileState,
  fresh admission and native state paths traced; session app relaunch outside
  the pin, with secondary per-app flags/order limits).
- External session manager/niri: R-RST-02 (config-only reload and fresh
  startup commands/admission traced; session app relaunch outside the pin,
  with secondary per-app flags/order limits).
- External session manager/sway: R-RST-02 (config-only reload and fresh
  admission traced; session app relaunch outside the pin, with secondary
  per-app flags/order limits).
- External session manager/i3: R-RST-02 (in-place-only layout file and
  fresh startup manage traced; session app relaunch outside the pin,
  with secondary per-app flags/order limits).
- Secondary host limits are excluded from primary unlock counts. PaperWM's
  newly F cells R-WS-12/22, R-RST-01 and R-SPC-03/13 retain host
  qualifications; R-DRAG-04 retains host Esc-interception uncertainty.
  COSMIC's pinned `cosmic-comp` references are not external missing hosts.
  Its lockfile-pinned smithay/x11rb hint mapping is now traced; R-SPC-07 is
  F for the per-axis absent-height X11 fixture encoding, not missing source.

## Fixture-gap aggregate

**302 cells across 92 scenario rows.** This ledger identifies wording to
tighten in Given, When or the profile; it does not edit scenarios or choose
the missing values. Counts include two HYP explicit-swap extras. The existing
profile defaults remain fixed; projection entries ask how a foreign fixture
maps to that profile, not permission to change defaults.

| Area / row (cells) | WMs | Missing input(s) |
|---|---|---|
| activation.md / R-ACT-01 (1) | KAR | Activation request timestamp (stale/zero vs unknown) selecting the allowWindowActivation fork. |
| close.md / R-CLOSE-01 (4) | HYP, QTI, AWE, PAN | Flat H[A,B,C] projection: HYP binary embedding + output geometry for collapse widths; QTI Columns embedding (num_columns=2, third stacks in-column); AWE master/stack projection + list order + work-area shares. PAN: viewport origin and column centers selecting nearest-center A vs C refocus. |
| close.md / R-CLOSE-02 (6) | HYP, BSP, QTI, AWE, KAR, XMO | 50/30/20 start: HYP binary + output width + pointer half for reopened B; BSP binary holding shares; QTI Columns + manual width mapping; AWE master/stack/windowfact mapping. KAR: newcomer protocol selecting X11-manage vs Wayland-add activation. XMO: native Tall projection/work-area geometry for exact survivor/reopened widths. |
| close.md / R-CLOSE-04 (2) | PAN, XMO | PAN: column centers and viewport origin selecting nearest-center A vs B after float close. XMO: F's StackSet slot/admission order selecting positional down-else-up refocus. |
| close.md / R-CLOSE-05 (1) | COS | Output/work-area geometry for exact post-close pixel frames. |
| column-mechanics.md / R-COL-01 (1) | KAR | Newcomer activation protocol/timestamp/token inputs and preferred frame/min/max for exact widths. |
| column-mechanics.md / R-COL-07 (1) | PAN | MoveFocus Follow/Stay input selecting follow focus for virtual-row transfer. |
| column-mechanics.md / R-COL-09 (1) | PAN | MoveFocus Follow/Stay input, including empty-source Stay-becomes-Follow branch. |
| floating.md / R-FLT-01 (2) | COS, HYP | COS: B prior float geometry or output/work-area geometry for cascade/center fallback. HYP: binary embedding + B float size/output geometry + pointer for unfloat anchor. |
| floating.md / R-FLT-03 (5) | HYP, BSP, QTI, AWE, XMO | Flat 50/30/20 projection: HYP/BSP binary for ratio-vs-equalize; HYP output geometry. QTI Columns 960/576/384 mapping; AWE 1920px tile mapping under nmaster=1, mwfact=0.5 and equal stack shares. XMO: native Tall projection, not an assumed H-child-to-StackSet order. |
| floating.md / R-FLT-04 (1) | COS | Move-A parameters (pointer path/semantic target) + workspace/output geometry. |
| floating.md / R-FLT-06 (3) | COS, HYP, QTI | COS: workspace/output/tree geometry for settled post-admit frame. HYP: work-area geometry for retained-maximize settled frame; secondary client ack/visuals, focus stays B. QTI: column/work-area geometry for exact fresh-tiled pixels after maximize-dropping unfloat. |
| floating.md / R-FLT-08 (1) | PAP | Remembered tiled selectedWindow at F-focus time (switchRight target). |
| floating.md / R-FLT-09 (2) | PAP, QTI | PAP: remembered tiled selectedWindow at F-focus time. QTI: stored tiled column current selecting the exact tile target; G is never a candidate. |
| floating.md / R-FLT-10 (1) | BSP | Output/work-area geometry for recomputed B tile frame after F/B swap. |
| floating.md / R-FLT-11 (1) | BSP | Focus/visit-history rank breaking equal-distance A/B north tie for second-leg swap. |
| floating.md / R-FLT-12 (1) | KAR | Raise/lower step producer selecting the native host path (no script verb). |
| floating.md / R-FLT-13 (1) | HYP | Pointer at WS1 return (focus under follow_mouse=1). |
| focus.md / R-FOC-03 (1) | BSP | Binary embedding selecting B-origin next/previous internal-node endpoints; bare selectors include containers. |
| focus.md / R-FOC-04 (1) | BSP | Child PATH selector (`first` vs `second`) selecting A vs B; no per-container remembered-child policy. |
| groups-stacks.md / R-GRP-01 (2) | I3, SWY | Which stack-toggle verb/gesture: tabbed, stacked, toggle split/all. |
| insertion.md / R-INS-01 (2) | HYP, KAR | HYP: pointer half of B determining C before/after B. KAR: newcomer activation protocol/timestamp/token inputs and preferred frame/min/max for exact widths. |
| insertion.md / R-INS-02 (3) | QTI, AWE, KAR | A/B membership projection of S[A*,B]: QTI Columns; AWE tile list (no tab-stack counterpart). KAR: newcomer activation protocol/timestamp/token inputs. |
| insertion.md / R-INS-03 (1) | KAR | Newcomer protocol, input/tabfocus eligibility and activation policy inputs. |
| insertion.md / R-INS-04 (6) | COS, HYP, KAR, XMO, AWE, BSP | COS/BSP: realized anchor frames/output dimensions for successive axes. HYP: anchor frames for successive axes and fixed pointer position for sides (active-tile anchor chain traced). KAR: activation inputs for successive newcomers selecting the remembered insertion-anchor chain. XMO/AWE: landscape work-area dimensions for exact frames; secondary pointer/focus-update settlement is live-only. COS retains exact-frame limits. |
| insertion.md / R-INS-05 (2) | HYP, KAR | HYP: pointer position, pointer-hit vs active-tile anchor. KAR: newcomer activation inputs with F already active. |
| insertion.md / R-INS-06 (6) | COS, HYP, KAR, AWE, BSP, QTI | COS/AWE: anchor/output/work-area geometry for exact max/full admission frames. HYP: pointer half for before/after B in both legs (covering overlay exit and anchor B traced). BSP: B's realized anchor rectangle selecting the fullscreen-leg split axis. KAR: newcomer activation inputs selecting later-focus overlay clearing and viewport movement. QTI: X11 vs Wayland backend for final overlay cover; secondary render/pixel settlement, newcomer focus sourced. |
| insertion.md / R-INS-07 (4) | COS, HYP, QTI, KAR | COS: WS2 B/C rectangles + output dimensions. HYP: same + mouse point for anchor and D/C order. QTI: WS2 column membership + setup order. KAR: newcomer activation inputs selecting focus/no-steal/desktop switch. |
| layout-commands.md / R-LAY-05 (1) | HYP | Node/parent rectangles for axis recalculation after each toggle. |
| layout-commands.md / R-LAY-06 (2) | HYP, KAR | HYP: pointer half for newcomer A/B order. KAR: newcomer activation inputs and preferred frame/min/max for exact widths. |
| maximize-fullscreen.md / R-MAX-01 (2) | HYP, QTI | HYP: binary embedding of H[A,B*,C,D] + output geometry. QTI: work-area/output geometry and gap/column projection for exact maximize/restore frames; secondary convergence/client-ack timing. |
| maximize-fullscreen.md / R-MAX-02 (2) | XMO, I3 | XMO: focus verb selecting core stack focusUp/focusDown vs same-layer Navigation2D windowGo while fullscreen. I3: directional focus fences, whereas criteria focus unblocks fullscreen; focus verb unstated. |
| maximize-fullscreen.md / R-MAX-06 (2) | HYP, KAR | HYP: target-workspace anchor inputs/output geometry for siblings (newcomer focus and restore traced; secondary native ack/visuals). KAR: newcomer protocol selecting X11-manage vs Wayland-add activation. |
| maximize-fullscreen.md / R-MAX-07 (6) | COS, HYP, QTI, AWE, I3, SWY | COS/HYP/QTI/AWE: monitor/work-area geometry. I3: captionless type + fullscreen atom/override-redirect. SWY: protocol path + xdg-fullscreen vs Xwayland type/override state. |
| maximize-fullscreen.md / R-MAX-09 (1) | COS | WS2 tile geometry/focus history for target slot; secondary client settlement. |
| minimize.md / R-MNZ-01 (1) | PAN | Frame/display geometry selecting nearest-center refocus target. |
| minimize.md / R-MNZ-02 (1) | PAN | Same geometry as MNZ-01 selecting the retained focus identity after old-slot restore. |
| minimum-size.md / R-MIN-01 (3) | COS, HYP, KAR | COS: realized anchor frames/output dimensions; secondary client settlement. HYP: pointer for Dwindle admission anchor. KAR: newcomer protocol selecting X11-manage vs Wayland-add activation. |
| minimum-size.md / R-MIN-03 (2) | COS, KAR | COS: output/work-area dimensions for sole-leaf frame; secondary client settlement. KAR: newcomer protocol selecting X11-manage vs Wayland-add activation. |
| mouse.md / R-DRAG-01 (6) | HYP, I3, SWY, QTI, NIR, PAP | HYP: release geometry. I3: center pixel/band + Shift swap state. SWY: center band (swap/tabify/30% split). QTI: hover pixel + drop-side geometry. NIR: center pixel and tile/work-area geometry selecting NewColumn vs InColumn. PAP: pointer y selecting the within-column row. |
| mouse.md / R-DRAG-02 (7) | HYP, BSP, I3, SWY, QTI, AWE, NIR | N start state + between-child bar/drop pixel. BSP: initial monitor/position and same/cross-monitor branch. AWE: N initial frame. NIR: tiled vs floating N and bar pixel selecting column insertion vs retained float/mover share. HYP/I3/SWY/QTI: bar-to-drop mapping. |
| mouse.md / R-DRAG-03 (5) | COS, HYP, QTI, I3, SWY | COS: workspace/output geometry. HYP/QTI: release geometry. I3: top-edge pixel + DT_PARENT/DT_SIBLING band. SWY: top-edge pixel + layout-border parent band. |
| mouse.md / R-DRAG-04 (9) | COS, HYP, BSP, XMO, SWY, QTI, AWE, NIR, PAP | Pointer hover/drop point at Esc. BSP/XMO: moved-then-Esc topology. SWY: Esc/release position for NULL-abort vs finalize. AWE/NIR: Esc and release points. PAP: hover/drop selecting end() branch; secondary host Esc interception. |
| mouse.md / R-DRAG-05 (2) | HYP, KAR | HYP: press/drop point for no-motion rebuilt topology. KAR: title-bar press-hold duration relative to startDragTime. |
| mouse.md / R-DRAG-06 (8) | COS, HYP, BSP, I3, SWY, QTI, AWE, PAP | Panel/taskbar and pointer containment: COS work area + cursor-output contents; HYP panel/monitor; BSP/I3/SWY on-output vs off-all-outputs; QTI producer branch + panel; AWE panel/taskbar geometry. PAP: pointer path, including whether a last DnD zone was acquired. |
| mouse.md / R-DRAG-07 (5) | HYP, I3, SWY, QTI, NIR | HYP: drop point. I3/SWY: edge of A + pixel/branch. QTI: drop + pause-point pixels. NIR: pause rest position/duration selecting edge auto-scroll and final insert mapping. |
| mouse.md / R-DRAG-08 (4) | HYP, I3, SWY, QTI | HYP: drop edge. I3: release pixel + DT branch. SWY: release pixel + drop branch. QTI: edge/side for drop-side focus. |
| mouse.md / R-MOU-02 (3) | COS, HYP, NIR | Work-area/output pixel geometry for the 100px fork-resize share delta. HYP: enabled resize parent-box dimensions; shipped bare-edge resize is off. NIR: work-area width fixing the 0.5W pixel base. |
| mouse.md / R-MOU-03 (6) | COS, HYP, QTI, AWE, NIR, SWY | COS: C-edge hover pixel and WS3 switcher-target/drop geometry. HYP: hover/drop pixel for fresh tile admission; WS3 switcher drop has no counterpart. QTI: hovered window/edge target + cross-output drop geometry. AWE: hovered client/index for tiled hover-swap; WS3 switcher drop target has no counterpart. NIR: overview-toggle step and WS3 membership/geometry selecting a rendered workspace target. SWY: C-edge hover/drop pixel selecting split/insert vs swap; switcher drop has no counterpart. |
| move.md / R-MOV-01 (5) | HYP x2, QTI, AWE, XMO | HYP main: binary H[A,C,B*] + output geometry for focal anchor/ratios; swap: embedding for target existence. QTI/AWE: Columns/tile projection of COL[C1[A],C2[C],C3[B*]]. XMO main: native Tall projection of the foreign flat triple (conditional analogue is not the exact Given). |
| move.md / R-MOV-03 (5) | HYP x2, QTI, AWE, XMO | HYP main: binary H[A,B*,C,D] + geometry; swap: embedding for target existence. QTI/AWE: four single-window-column projection. XMO main: native Tall projection, not assumed H identity order. |
| move.md / R-MOV-04 (3) | HYP, QTI, XMO | HYP: geometry for focal y-half at square tie. QTI: Columns counterpart of H[H[A,B*],C]. XMO: native Tall projection of the nested-H fixture. |
| move.md / R-MOV-06 (2) | HYP, QTI | HYP: A mid-height vs B/C divider plus node admission order selecting exact geometric B-vs-C leaf; distance-only focal admission has no remembered-child branch. QTI: C2's stored current selecting carry-at-current row. |
| move.md / R-MOV-08 (2) | COS, XMO | COS: source workspace index for workspace-first vs output fallback, plus destination contents. XMO: U/L output rectangles and alignment selecting Navigation2D hit vs miss. |
| move.md / R-MOV-09 (10) | COS, HYP, BSP, QTI, AWE, NIR, PAP, KAR, PAN, XMO | COS: output width. HYP/BSP: binary embedding + width/shares. QTI/AWE: Columns/tile share mapping. NIR/KAR: column fixture with 1/10-4/10 widths. PAP/PAN: strip order + widths; PAN WidthRatio mapping. XMO: native Tall projection of the unequal-share flat fixture. |
| move.md / R-MOV-10 (6) | COS, HYP, BSP, I3, SWY, QTI | COS: shares + V remembered child. HYP: binary H[A,B,V,E] + shares/geometry. BSP: V shares/child + boundary geometry/history. I3/SWY: V remembered child C/D. QTI: Columns counterpart. |
| move.md / R-MOV-11 (5) | COS, HYP, BSP, XMO, AWE | COS: source workspace index + destination contents. HYP: U contents. BSP/XMO/AWE: U contents/focus for empty vs swap; XMO target geometry. |
| move.md / R-MOV-12 (6) | COS, HYP, BSP, I3, XMO, AWE | COS: source index + output order + destination contents. HYP: U2 contents for admission (half-open containment deterministically selects U2). BSP: U1/U2 contents. I3: output order for y=0 tie. XMO: U1/U2 contents + geometry. AWE: contents + enumeration order. |
| move.md / R-MOV-13 (4) | COS, HYP, XMO, BSP | COS: source index + destination contents. HYP: target contents. XMO: per-leg contents + target geometry. BSP: U contents/focus selecting occupied-swap vs empty-no-op. |
| multi-output.md / R-OUT-01 (5) | HYP, BSP, XMO, AWE, SWY | HYP: L work area/X geometry + alignment + drop half. BSP: L/X geometry. XMO: L/R Tall window and output geometry. AWE: initial global client order and work areas for post-exchange order/frames. SWY: L output aspect selecting geometry-following default layout and target split shape (profile config fixed). |
| multi-output.md / R-OUT-02 (4) | HYP, BSP, XMO, AWE | HYP: arrangement/adjacency + alignment. BSP: L/X geometry + alignment. XMO: output geometry for cross/local selection. AWE: tile projection of V[A,B]. |
| multi-output.md / R-OUT-07 (3) | COS, HYP, AWE | COS: work area + decoration sizes. HYP: cursor + L work area. AWE: initial global client order and work area for retained-order admission frames. |
| multi-output.md / R-OUT-06 (7) | COS, HYP, AWE, QTI, NIR, SWY, I3 | COS: L's workspace contents/focus history selecting exact post-evacuation node; secondary frame/client settlement. HYP: R/L contents/focus history selecting evacuated focus (return affinity traced). AWE: remaining-screen focus history selecting evacuated focus. QTI: R/L contents/focus history and group numbering selecting exact focus/fresh reconnect assignment; no evacuation merge or return-affinity store. NIR: R/L contents/focus history selecting exact focused node; distinct-workspace evacuation and preferred-output return affinity traced. SWY: R/L contents and seat focus stack/history selecting evacuation/reconnect focus, including destroyed-empty listeners; priority return affinity traced. I3: R/L contents/focus history and workspace names/numbers selecting exact evacuation focus and fresh reconnect identity; bindings fixed by profile. |
| resize.md / R-RSZ-01 (1) | KAR | Recorded but unstated viewport width selecting contextual grow/shrink step. |
| restart-persistence.md / R-RST-01 (4) | PAP, KAR, AWE, QTI | PAP: F float mechanism, above-flag, minimized-scratch or list-only; secondary host tab-list selection. KAR: creation/manage order selecting fresh column adoption. AWE: pre-restart stacking/admission order and whether B focus raised it, selecting last-visible rule-focus in startup scan. QTI: pre-restart stacking/admission order selecting last stealable startup focus in X11 QueryTree scan; Wayland has no restart journey. |
| restart-persistence.md / R-RST-03 (2) | PAP, KAR | PAP: F float mechanism, above-flag, minimized-scratch or list-only. KAR: creation/manage order, moved F frame width and pre-adoption focus state. |
| restart-persistence.md / R-START-01 (2) | AWE, KAR | AWE: global-client manage/swap list order for 2x2 field (fixture gives focus order only). KAR: creation/manage order (focus order does not reorder Workspace.windows). |
| restart-persistence.md / R-START-02 (2) | AWE, KAR | AWE: global-client manage/swap list order for A-D cascade. KAR: creation/manage order selecting fresh columns, not cascade position. |
| restart-persistence.md / R-START-03 (2) | AWE, KAR | AWE: global-client manage/swap list order for A-E infeasible-minima cascade. KAR: creation/manage order and minimum-dependent origins. |
| restart-persistence.md / R-CTL-04 (1) | COS | Existing override workspace pinned vs unpinned at restart. |
| special-windows.md / R-SPC-01 (2) | KAR, I3 | KAR: native newcomer activation timestamp/token inputs for the eligible dialog. I3: D request geometry, leader rect and output containment selecting stored placement vs parent/workspace centering. |
| special-windows.md / R-SPC-02 (3) | I3, SWY, KAR | I3/SWY: external task-switcher identity for listing/presence. KAR: utility X11 activation inputs selecting denied-activation attention. |
| special-windows.md / R-SPC-03 (12) | COS, HYP, BSP, I3, XMO, SWY, QTI, AWE, NIR, PAP, KAR, PAN | Media/PiP app, toolkit/version, actual flags/rules. PAN: title/subrole and trigger flags for windows.pip matching. PAP: Normal/non-transient antecedent; secondary host/app above-flag behavior. |
| special-windows.md / R-SPC-07 (3) | COS, XMO, BSP | X11 encoding for width fixed with height absent; WM_NORMAL_HINTS flags govern whole tuples, not independent axes. XMO/BSP's other whole-pair equality legs are traced. |
| special-windows.md / R-SPC-13 (2) | PAP, KAR | PAP: A's live frame for exact widths; secondary host tab-list selection. KAR: creation/manage order, frames/min/max and pre-startup focus state. |
| workspaces.md / R-WS-01 (3) | HYP, BSP, AWE | HYP: C long-edge geometry + cursor half. BSP: C/output geometry. AWE: work areas for recomputed retained-order tile frames. |
| workspaces.md / R-WS-02 (3) | HYP, PAN, AWE | HYP: return cursor for anchor/order. PAN: MoveFocus Follow/Stay policy. AWE: work areas for send/return frames. |
| workspaces.md / R-WS-04 (4) | HYP, QTI, AWE, BSP | HYP: D geometry + cursor half. QTI/AWE: D geometry + WS2 focus after floated-C removal. BSP: C's saved rectangle/output geometry for the split axis; vacant-collapse already determines B's slot and leaves D unchanged. |
| workspaces.md / R-WS-05 (2) | HYP, AWE | Window/output/work-area geometry for forward/return frames. |
| workspaces.md / R-WS-06 (3) | COS, HYP, AWE | COS: WS1 rectangles/output for float frame/clamp. HYP: WS2 contents + A geometry + cursor half. AWE: work areas for retained-frame floating send and tile-return frames. |
| workspaces.md / R-WS-09 (1) | HYP | Return pointer position for focus. |
| workspaces.md / R-WS-12 (5) | COS, PAP, AWE, QTI, SWY | COS: WS2 contents/focus history selecting moved-active focus. PAP: live stack/tab order selecting displaced views/focus; secondary host neighbor/index policy. AWE: whether C is the previously focused client on R, selecting final focus while the moved tag stays hidden. QTI: current screen/current-window history for layout_all focus after toscreen view-ownership reassignment. SWY: WS2/WS3 contents and seat focus history selecting moved focus and displaced views; criteria reaches hidden WS2. |
| workspaces.md / R-WS-15 (3) | HYP, BSP, PAP | HYP: arrival pointer. BSP: visit/focus history. PAP: MRU visit sequence. |
| workspaces.md / R-WS-17 (5) | BSP, AWE, QTI, SWY, I3 | BSP: visit history for last-walk targets in removal variant. AWE: remaining-screen client focus history after disconnect/previous. QTI: group numbering/assignment selecting fresh reconnect mapping and second previous toggle. SWY: pre-disconnect global seat focus/previous chain; the per-L WS1 record does not fix sway's single global history. I3: global focus/previous-name chain at disconnect; the per-L WS1 record does not fix i3's single global history. |
| workspaces.md / R-WS-18 (2) | COS, HYP | COS: geometry. HYP: geometry + E numeric ID/position. |
| workspaces.md / R-WS-19 (1) | COS | Post-transfer window/output geometry. |
| workspaces.md / R-WS-20 (2) | COS, HYP | COS: invoked relative-send chord direction selecting the shipped Vertical layout branch. HYP: E's numeric ID selecting E-vs-created numeric+1 target; ordinal trailing position does not fix that ID. |
| workspaces.md / R-WS-21 (3) | SWY, QTI, AWE | SWY: L remaining workspace order/history. QTI: WS3 remembered current_window. AWE: per-screen tag/client histories selecting source refill and focus. |
| workspaces.md / R-WS-22 (4) | SWY, QTI, PAP, AWE | SWY: WS1 contents/focus-memory node. QTI: WS3 current_window and frames. PAP: live stack/tab order selecting shown views/moved-active focus; secondary host neighbor/index policy. AWE: per-screen tag/client histories selecting refill/focus. |
| workspaces.md / R-WS-23 (6) | HYP, BSP, SWY, QTI, NIR, AWE | HYP: global workspace creation order for first-enumerated WS1/E refill, plus E's number/full numeric inventory for the created-number leg. BSP: WS1/spare E visit history. SWY: WS1 contents/focus-memory. QTI: WS3 current_window. NIR: E spare index/order. AWE: source history for empty-tag migration refill. |
| workspaces.md / R-WS-26 (7) | BSP, I3, SWY, QTI, NIR, PAP, AWE | BSP: history older than WS2. I3: refill identity + W_prev. SWY: W_prev. QTI/NIR: W_prev + return scope. PAP: MRU visit order. AWE: L visit history and R client focus history for cross-screen restore/next-previous. |
| workspaces.md / R-WS-27 (4) | COS, BSP, NIR, I3 | COS: output order. BSP: monitor order + follow/stay. NIR: output order for 1080 center-y tie + focused windows/views. I3: output enumeration order for equal-y U1/U2 tie and destination contents for displaced views; direction resolves at runtime. |

## Unsupported-pending and live-only ledger

This ledger, the H/F lists and the area counts provide the durable
classification audit. N cells are the remaining TBDs after these exclusions.
Ranges denote contiguous IDs; slash-separated numbers denote only those IDs.
All IDs have prefix `R-`; none of these U/L entries are duplicated swap cells.

| WM | U rows | L rows |
|---|---|---|
| COSMIC | None (all 8 closed) | FLT-05; GRP-01; MAX-01..06; MIN-02; DRAG-01/05/07..08; RST-03..04; START-03; SPC-08/11; WS-07 |
| Hyprland | None (all 17 closed) | FLT-11; GRP-01; MAX-04..05; MIN-02..03; RST-04; SPC-08..11; WS-26 |
| bspwm | None (all 20 closed) | MAX-04; MIN-02; DRAG-08; RST-03..04; SPC-08..09/11; WS-25 |
| i3 | None (20 U closed; 1 H and 1 F reclassified) | FLT-01..03/06/11; MAX-05; MIN-01..03; DRAG-05; OUT-07; RST-03..04; SPC-08..09/11; WS-18..19 |
| xmonad | None (all 23 closed) | MIN-01..03; MAX-05; RST-03..04; SPC-08..09 |
| sway | None (22 U closed; 1 H and 1 F reclassified) | FLT-01..03/06/11; MAX-05; MIN-01..03; DRAG-05; OUT-07; RST-04; SPC-08..09/11; WS-18..19 |
| qtile | None (all 21 closed) | MAX-04..06; MIN-01..03; RST-03..04; SPC-08..11; WS-25 |
| awesome | None (all 12 closed) | FLT-02/05..06; MAX-04; MIN-01..03; RST-03..04; SPC-08..13; WS-25 |
| niri | None (all 12 closed) | FLT-06; MAX-01; MIN-02; RST-03..04; SPC-08..09 |
| PaperWM | None (all 8 closed) | MAX-01; MIN-01..03; SPC-05/08..11 |
| karousel | None (all 8 closed) | FLT-06; MAX-01; RST-04; SPC-05/08..11 |
| paneru | None (all 13 closed) | RST-03; MIN-01..03 |

## Ranked per-WM passes and expected yield

Rank by unfinished source-candidate count N, then N+U for ties. These are
triage candidate pools, not measured completion forecasts. N+U is the
upper candidate yield for a pass also closing unsupported statements.
All twelve per-WM passes are now complete; no N/U candidate remains.

| Rank | WM | Source candidates N | Cheap U | N+U ceiling | N with F/H/L secondary |
|---|---|---:|---:|---:|---:|
| Complete | i3 | 0 | 0 | 0 | 0 |
| Complete | sway | 0 | 0 | 0 | 0 |
| Complete | niri | 0 | 0 | 0 | 0 |
| Complete | qtile | 0 | 0 | 0 | 0 |
| Complete | Hyprland | 0 | 0 | 0 | 0 |
| Complete | bspwm | 0 | 0 | 0 | 0 |
| Complete | awesome | 0 | 0 | 0 | 0 |
| Complete | xmonad | 0 | 0 | 0 | 0 |
| Complete | COSMIC | 0 | 0 | 0 | 0 |
| Complete | PaperWM | 0 | 0 | 0 | 0 |
| Complete | karousel | 0 | 0 | 0 | 0 |
| Complete | paneru | 0 | 0 | 0 | 0 |

Context-sized area splits are given in the N table. The completed PaperWM
pass used navigation/admission, window states, workspaces/output, mouse/groups,
and persistence/special-window slices; U cells were evaluated with their area.

## PaperWM source-pass outcome

- All **70 N + 8 U attempted**, with independent source verification of
  every slice after corrections. **34 N + 8 U closed (42 cells)**;
  **36 N reclassified: 21 H, 8 F, 7 L**. No N/U candidates remain.
- New H cells are listed in the host ledger. New F: R-DRAG-01/04/06,
  R-RST-01, R-SPC-03/13, R-WS-12/22. New L: R-MAX-01,
  R-MIN-01/02/03, R-SPC-05/10/11. Secondary limits remain explicit in cells.
- Existing 17 H, 6 F and 2 L cells were left unchanged. Current PaperWM
  residual is **61: H38/F14/L9**. Other profiles, scenario wording and
  approved rules were preserved; no material approved-rule contradictions.
- Evidence is pinned extension policy, not live runtime acceptance. No live
  testing, source-checkout changes or stash changes were performed.

## karousel source-pass outcome

- All **58 N + 8 U attempted**, with independent source verification of
  every slice after corrections. **29 N + 8 U closed (37 cells)**;
  **29 N reclassified: 1 H, 24 F, 4 L**. No N/U candidates remain.
- New H: R-RST-02 (unpinned Plasma session manager ksmserver and app
  participation). New F: R-COL-01; R-FLT-12; R-INS-01..07; R-LAY-06;
  R-CLOSE-02; R-MAX-06; R-MIN-01/03; R-RSZ-01; R-DRAG-05;
  R-START-01..03; R-RST-01/03; R-SPC-01/02/13. New L: R-MAX-01;
  R-SPC-05/10/11. Secondary limits remain explicit in cells.
- Existing 3 F and 4 L cells were unchanged. Current karousel residual is
  **36: H1/F27/L8**. Other profiles, scenarios and approved rules were
  preserved; no material approved-rule contradictions.
- Pinned KWin native paths were traced using raw `KDE/kwin@8438567a`
  citations, including script focus, native switching, fixed desktops,
  sticky, close/minimize, move/resize, new-window activation and window order.
  No live testing, source-checkout changes or stash changes were performed.

## paneru source-pass outcome

- All **57 N + 13 U attempted**, with independent source verification of
  every slice after corrections. **38 N + 13 U closed (51 cells)**;
  **19 N reclassified: 10 H, 6 F, 3 L**. No N/U candidates remain.
- New H: R-ACT-01/02; R-CLOSE-02/03; R-COL-10; R-FLT-13;
  R-INS-06; R-MNZ-03; R-RST-01; R-SPC-13. New F: R-CLOSE-01/04;
  R-COL-07/09; R-MNZ-01/02. New L: R-MIN-01/02/03.
- Existing H18/F3/L1 cells were unchanged. Current paneru residual is
  **41: H28/F9/L4**. macOS host implementation is not public; no new source
  pin can resolve its opaque policy. Secondary limits remain named in cells.
- Other profiles, scenarios and approved rules were preserved; no material
  approved-rule contradictions. No code changes, live testing, source-checkout
  changes or stash changes. The archived paneru note records corrections
  and accepted independent evidence.

## COSMIC source-pass outcome

- All **41 N + 8 U attempted**, with separate independent source verification
  of every area slice and a strict final occurrence-aware check. **21 N + 8 U
  closed (29 cells)**; **20 N reclassified: 1 H, 14 F, 5 L**. No N/U remain.
- New H: R-RST-02 (`cosmic-session`, session/app relaunch participation).
  New F: R-CLOSE-05; R-INS-04/06; R-MOV-08; R-MIN-01/03;
  R-MAX-09; R-MOU-02/03; R-OUT-06; R-WS-12/20; R-CTL-04; R-SPC-07.
  New L: R-FLT-05; R-MIN-02; R-MAX-01/05; R-START-03.
- Existing F19/L14 cells unchanged. Current COSMIC residual is **53:
  H1/F33/L19**. Lockfile-pinned smithay/x11rb mapping was traced without a
  new source revision; only X11 per-axis absent-height fixture encoding remains.
- Approved rules unchanged. Source comparisons requiring user review and
  explicit existing deviation coverage are listed with rule/source locations
  in [COSMIC's archived note](cosmic-reference-source-fill.md).
  REQ-MAX-08/09 retain OPEN/unresolved wording despite earlier Windows policy
  text; this pass does not reconcile that product decision.
- No scenario/code/source-checkout/stash changes or live testing.
- Final executable reconciliation: 95 assertions passed after independent
  review and the archived-path checker repair, including
  every per-WM class partition/actual TBD count, N-area row/column totals,
  occurrence/scope/citation checks, fixture membership and exact COSMIC H/F/L
  ledger reconciliation. Three stash object identities and source pin preserved.

## xmonad source-pass outcome

- All **41 N + 23 U attempted**, with independent source verification of
  every area slice. **25 N + 23 U closed (48 cells)**; **16 N reclassified:
  1 H, 11 F, 4 L**. No N/U candidates remain.
- New H: R-RST-02 (external X session manager, plus app session participation).
  New F: R-CLOSE-02/04; R-FLT-03; R-INS-04; R-MAX-02;
  R-MOV-01/03/04/08/09 (main legs only); R-SPC-07.
  New L: R-MIN-01..03; R-MAX-05. INS-04 retains secondary live settlement;
  RST-02 retains secondary app set/order/flags fixture limits.
- Existing F7/L4 unchanged. Current xmonad residual is **27: H1/F18/L8**.
  Exact foreign move fixtures retain TBD rather than guessing a Tall
  projection. SPC-11 closes sourced exit classification without requiring
  beyond-Observe frames/focus.
- Approved rules unchanged. Source differences, explicit existing deviation
  coverage, corrections and final verification are recorded in
  [xmonad's archived note](xmonad-reference-source-fill.md).
- No scenario/code/source-checkout/stash changes or live testing.

## awesome source-pass outcome

- All **50 N + 12 U attempted**, with separate independent source verification
  of every slice and a fresh audit of corrected admission, delayed focus and
  restart chains. **24 N + 12 U closed (36 cells)**; **26 N reclassified:
  1 H, 17 F, 8 L**. No N/U candidates remain.
- New H: R-RST-02 (external X session manager and app relaunch participation).
  New F: R-INS-04/06; R-MOU-03; R-OUT-01/06/07; R-RST-01;
  R-WS-01/02/05/06/12/17/21/22/23/26. New L: R-MIN-01..03;
  R-SPC-08/09/12/13; R-WS-25. Secondary limits remain explicit in cells.
- Existing F19/L8 unchanged. Current awesome residual is **53: H1/F36/L16**.
  Corrected source orientation: `client_array_push` prepends, so ordinary
  newcomers become the first/master tile, not the last stack member.
  Already-resolved/excluded cells remain outside this pass; the archived note
  records older append wording discovered there for separate correction.
- Approved rules unchanged. Source comparisons, correction evidence and final
  verification are recorded in [awesome's archived note](awesome-reference-source-fill.md).
- No scenario/code/source-checkout/stash changes or live testing.

## bspwm source-pass outcome

- All **42 N + 20 U attempted**, with independent source verification of
  every slice and a separate corrected-policy/fixture/comparison audit.
  **33 N + 20 U closed (53 cells)**; **9 N reclassified: 1 H, 7 F, 1 L**.
  No N/U candidates remain.
- New H: R-RST-02 (external X session manager, per-app relaunch participation).
  New F: R-FOC-03/04; R-INS-04/06; R-MOV-13; R-SPC-07; R-WS-04.
  New L: R-MIN-02 (native shrink/grow settlement). Secondary limits remain
  explicit; session relaunch retains per-app flags/order fixture limits.
- Existing F20/L8 unchanged. Current bspwm residual is **37: H1/F27/L9**.
  Vacant-leaf insertion wraps at live destination focus; occupied geometry
  collapses the vacant child, with the saved-rectangle axis still fixture-bound.
  Dump/restore keeps live client X IDs; it is not cross-session app rematching.
- Approved rules unchanged. Eight source comparisons and correction evidence
  are recorded in [bspwm's archived note](bspwm-reference-source-fill.md).
- No scenario/code/source-checkout/stash changes or live testing.

## Hyprland source-pass outcome

- All **44 N + 17 U attempted**, with separate independent source verification
  of each of five area slices and a corrected-policy/comparison audit.
  **27 N + 17 U closed (44 cells)**; **17 N reclassified: 1 H, 12 F, 4 L**.
  No N/U candidates remain.
- New H: R-RST-02 (external session manager and app relaunch participation).
  New F: R-FLT-03/06; R-INS-04/06; R-MAX-06; R-MOV-06/12;
  R-OUT-06; R-MOU-02/03; R-WS-20/23. New L: R-MAX-05;
  R-MIN-02/03; R-WS-26. Secondary limits remain explicit, including WS-26's
  remembered-origin activation and native settlement on FLT-06/MAX-06.
- Existing F41/L8 unchanged. Current Hyprland residual is **66:
  H1/F53/L12**. Lockfile-pinned hyprutils containment is traced, not a missing
  host: the MOV-12 seam selects U2. MOV-06's exact leaf remains fixture-bound.
- Overprecision resize and MOV-07 TBDs were removed only where Observe did
  not require numeric shares/steps/axes. WS-20 was reopened for E's unspecified
  numeric ID; ordinal order does not prove the numeric+1 target is E.
- Approved rules unchanged. Eight source comparisons, source corrections and
  final verification are recorded in [Hyprland's archived note](hyprland-reference-source-fill.md).
- No scenario/code/source-checkout/stash changes or live testing.

## qtile source-pass outcome

- All **37 N + 21 U attempted**, with independent source verification of
  all four slices and a fresh residual/approved-rule audit. **22 N + 21 U
  closed (43 cells)**; **15 N reclassified: 1 H, 9 F, 5 L**. No N/U remain.
- New H: R-RST-02 (external session manager and per-app relaunch).
  New F: R-FLT-06/09; R-INS-06; R-MAX-01; R-MOV-06; R-OUT-06;
  R-RST-01; R-WS-12/17. New L: R-MAX-05/06; R-MIN-01..03.
  Secondary live/fixture limits remain explicit in cells.
- Existing F25/L8 unchanged. Current qtile residual is **48:
  H1/F34/L13**. Profile backend remains unspecified: fullscreen retains its
  tiled slot, maximize removes it; X11 restart freshly re-admits while
  Wayland has no restart journey. QueryTree focus is F, not missing-host H.
- Approved rules unchanged. Source comparisons, corrected policy and final
  verification are recorded in [qtile's archived note](qtile-reference-source-fill.md).
- No scenario/code/source-checkout/stash changes or live testing.

## niri source-pass outcome

- All **41 N + 12 U attempted**, with independent source verification of
  all five slices and fresh verification of the corrected mouse branches.
  **32 N + 12 U closed (44 cells)**; **9 N reclassified: 1 H, 6 F, 2 L**.
  No N/U candidates remain.
- New H: R-RST-02 (external session manager and app relaunch participation).
  New F: R-DRAG-01/02/07; R-MOU-02/03; R-OUT-06.
  New L: R-MAX-01; R-MIN-02. Secondary limits remain explicit in cells.
- Existing F6/L5 unchanged. Current niri residual is **20: H1/F12/L7**.
  Pointer drops have NewColumn/InColumn/Floating branches and overview
  workspace targeting; a pause can change insertion through edge auto-scroll.
  These selectors remain fixture-bound, rather than guessed closed.
- Approved rules unchanged. Source comparisons, rejected approaches and
  corrected verification are recorded in [niri's archived note](niri-reference-source-fill.md).
- No scenario/code/source-checkout/stash changes or live testing.

## sway source-pass outcome

- All **12 N + 24 U attempted**, with independent source verification of
  three slices and corrected-policy/comparison audits. **8 N + 22 U closed
  (30 cells)**; **6 reclassified: 1 H, 5 F**. No sway N/U remain.
- New H: R-RST-02. New F: R-OUT-01/06; R-MOU-03; R-WS-12/17.
  Existing H2/F16/L17 unchanged. Residual **41: H3/F21/L17**.
- Criteria-targeted whole-workspace moves reach hidden WS2; active-only
  unsupported wording was corrected. Hotplug focus includes destroy listeners;
  per-L history does not fix sway's global seat history. Empty-source relative
  send focus resolves to the workspace node without an extra fixture gap.
- Approved rules unchanged. Nine independently verified comparisons and
  rejected traces/corrections are recorded in
  [sway's archived note](sway-reference-source-fill.md).
- No scenario/code/source-checkout/stash changes or live testing.

## i3 source-pass outcome

- All **13 N + 22 U attempted**, with independent source verification of
  three slices, corrected-policy rechecks and final comparison audits.
  **9 N + 20 U closed (29 cells)**; **6 reclassified: 1 H, 5 F**.
- New H: R-RST-02. New F: R-MAX-02; R-SPC-01; R-OUT-06;
  R-WS-17/27. Original F13/L18 unchanged; residual **37: H1/F18/L18**.
- Directional workspace output selection exists: parser words resolve through
  `get_output_from_string`; the prior unsupported WS-27 inventory was wrong.
  Criteria focus can clear fullscreen while directional focus fences. Hidden
  workspace migration is focus-neutral; sticky restart geometry closes
  relationally without needing pixels. Fresh start is not orderly re-exec.
- Eleven source-policy comparisons, corrections and verification are recorded
  in [i3's archived note](i3-reference-source-fill.md). Approved rules,
  scenarios, profiles and excluded cells preserved; no live/code/source/stash changes.
- No N/U remains for any reference WM. Residual H/F/L needs host-source,
  fixture inputs or user-owned runtime observations outside these passes.

## Verification and next action

- Initial triage independent occurrence-aware recount: 1,896 reference cells, 1,006 TBD;
  no KWin column; Ours TBD 169. Classification multiset exactly matches
  all TBD occurrences, including five duplicated row/profile keys.
- Initial triage aggregates reconciled: WM/area N totals 506; H 37; F 178 across 63 rows;
  L 97; U 188. The fixture table's cell counts sum to 178.
- Initial triage independent sample of 48 cells, at least three per WM, spanning all
  classes (N17/H9/F15/L4/U3). Forty-seven rationales confirmed; paneru
  MOV-03 was corrected to refer to the actual TBD explicit-swap leg rather
  than the resolved main leg. Its N class is unchanged. Three secondary
  notes were corrected: Hyprland LAY-06 client timing, sway MOU-03's
  input-less fixture qualification, awesome SPC-03's unpinned app source.
- Initial triage temporary evidence: `reference-triage-inventory.json`,
  twelve `reference-triage-<WM>.json` files, `reference-triage-aggregate.json`
  and `reference-triage-spot-check.json`. These are session artifacts, not
  repository dependencies; the durable classification ledgers are above.
- Current aggregates: 1,896 reference cells, 520 TBD; N0/H78/F302/L140/U0.
  Area N totals sum to zero; fixture ledger sums to 302 across 92 rows.
  The PaperWM, karousel, paneru, COSMIC, xmonad, awesome, bspwm, Hyprland, qtile and niri outcomes and independent verification are recorded
  above and in their archived notes, plus sway and i3; final occurrence-aware and diff-scope
  checks accompany each pass.
- Exact next action for PaperWM: none. Host-source pinning, fixture inputs
  and live-only observations remain user-owned; another WM pass is separate.
- Exact next action for karousel: none. Its remaining session-manager source,
  fixture inputs and live-only observations remain user-owned.
- Exact next action for paneru: none. Its remaining opaque macOS host policy,
  fixture inputs and live-only observations remain user-owned.
- Exact next action for COSMIC source fill: none; no N/U areas remain. User
  review of the listed rule differences, session-source pinning, fixture inputs
  and live-only observations remain separate follow-up work.
- Exact next action for xmonad source fill: none; no N/U areas remain. User
  review of listed rule differences, session-manager identification/pinning,
  fixture inputs and live-only observations remain separate follow-up work.
- xmonad final integrated verification: 77 executable assertions passed,
  independently rerun after archive. Exact changed-candidate/exclusion scope,
  all partitions and actual counts, N-area sums, fixture membership and
  xmonad H/F/L ledgers reconcile. Checkouts/pins and three stash object
  identities preserved; scenario/profile/rule/other-WM text unchanged.
- Exact next action for awesome source fill: none; no N/U areas remain. User
  review of listed rule differences, session-manager identification/pinning,
  fixture inputs and live-only observations remain separate follow-up work.
- awesome final integrated verification: 111 executable assertions passed.
  Exact changed-candidate/exclusion scope, all partitions and actual counts,
  N-area sums, fixture membership and awesome H/F/L ledgers reconcile.
  Source pin/checkout and three stash object identities preserved; scenario,
  profile, approved-rule and other-WM text unchanged.
- Exact next action for bspwm source fill: none; no N/U areas remain. User
  review of listed rule differences, session-manager identification/pinning,
  fixture inputs and live-only observations remain separate follow-up work.
- bspwm final integrated verification: 77 executable assertions passed,
  independently rerun after archive and integration.
  Exact 62-candidate/exclusion scope, all partitions and actual counts,
  N-area sums, fixture membership and bspwm H/F/L ledgers reconcile.
  Source pin/checkout and three stash object identities preserved; scenario,
  profile, approved-rule and other-WM text unchanged.
- Exact next action for Hyprland source fill: none; no N/U areas remain.
  User review of listed rule differences, session-manager identification/pinning,
  fixture inputs and live-only observations remain separate follow-up work.
- Hyprland final integrated verification: 218 executable assertions passed
  after archive and verification-plumbing repair. Exact 61-candidate scope,
  full multiline preservation, all actual per-WM counts/partitions, N-area
  sums, exact fixture memberships and Hyprland H/F/L ledgers reconcile.
  Fixture ledger totals 277 across 92 rows. Source pin/clean checkout,
  lockfile dependency narHash and three stash identities preserved; scenario,
  profile, approved-rule and other-WM text unchanged.
- Exact next action for qtile source fill: none; no N/U areas remain.
  User review of listed comparisons, external session-manager identification,
  fixture inputs and live-only observations remain separate follow-up work.
- qtile final integrated verification: 818 executable assertions passed after
  archive and verification-plumbing repair. Exact 58-candidate scope, full
  multiline preservation, all actual per-WM counts/partitions, N-area row and
  column sums, fixture memberships (286 cells across 92 rows), qtile H/F/L
  ledgers, source ranges and local links reconcile. Manual source review
  independently supports the ten comparison rows. Source pin/clean checkout,
  three stash identities, scenarios, profiles and approved rules preserved.
- Exact next action for niri source fill: none; no N/U areas remain.
  User review of listed comparisons, external session-manager identification,
  fixture inputs and live-only observations remain separate follow-up work.
- niri final integrated verification: 615 executable assertions passed after
  archive, mouse branch correction and verification-plumbing repair. Exact
  53-candidate/full multiline preservation, all actual per-WM counts/classes,
  N-area row/column sums, fixture membership (292 cells across 92 rows),
  niri H/F/L ledgers, eleven comparison rows, pinned citation ranges and local
  links reconcile. Full index text outside niri legend blocks is preserved.
  Source pin/clean checkout, three stash object identities, scenarios,
  profiles and approved rules preserved.
- Exact next action for sway source fill: none; no N/U areas remain.
  Integrated verification covers exact candidate/exclusion scope, multiline
  preservation, all WM counts/partitions, N-area sums, fixture membership,
  sway ledgers, nine comparison rows, pinned ranges, local links, clean pins
  and three stash object identities. At sway completion, only i3 N13/U22 remained.
- Exact next action for i3 source fill: none; all twelve WM passes complete,
  with no N/U candidates in any area. Final integrated reconciliation passed
  **730 executable assertions, zero failures/pending checks**, verifying exact
  35-candidate scope, full multiline preservation, all WM counts/partitions,
  zero N-area sums, fixture memberships, H/F/L ledgers, eleven comparison rows,
  citation ranges/local links, clean source pins and three stash object identities.

## Backlog detail moved 2026-11-01

### Original backlog lines 1679-1713

````markdown
- P1 | Cross-platform functional specification | After the Windows tiling
  dogfood slice, define window/workspace behavior and keyboard shortcuts as
  the selected behavior contract for Linux, Windows and macOS, except recorded
  exceptions. COSMIC is the reference-default yardstick; macOS modifier
  mapping is decided when macOS starts.
  [Reference outcomes](spec/reference-outcomes.md) use per-area Given/When/Then
  scenarios, twelve pinned reference profiles and separate Ours KDE/Windows
  outcomes. Source evidence is not a product decision; AGENTS.md requires a
  discriminating row per ambiguity. All twelve remaining source-fill passes
  completed (`f04511c`..`bfa486b`): zero not-attempted/unsupported-pending
  candidates remain; residual host, fixture and live limits are in the
  [archived ledger](changes/archive/reference-source-fill-remaining.md).
  [Functional spec](spec/functional-spec.md) remains single-file with a
  [grouped open-decisions index](spec/functional-spec.md#open-index).
  Current review 2026-10-12: all nine prior provisional requirements ratified;
  B9 no-refusal sub-leg selected; R-LAY-04 parked OPEN; G-06 makes
  REQ-MAX-08 NORMATIVE. Batch 1 approved 16 more rows; thirteen units / seventeen rows
  approved 2026-10-12, all 28 triage units decided. Totals: 138 NORMATIVE,
  28 OPEN, 0 PROVISIONAL requirement rows; 156 scenarios including R-WS-27 with TBD
  reference outcomes. Platform implementation/native gaps stay
  distinct from selected intent; Windows control cells now cite Windows evidence.
  [Cross-WM consensus analysis](research/reference-wm-consensus.md) Table A
  now lists 24 Ours-vs-strong-consensus differences with recommendations
  (selected decisions in [decisions](decisions.md)), plus strong-consensus predicates where Ours is
  TBD pending native observation. Splits decided 2026-10-07:
  R-DRAG-07 host-native presentation (KDE frame follows pointer, Windows
  Win-drag stationary preview) and R-DRAG-08 focus at press on both
  platforms (Windows implementation pending, KDE timing needs a live check).
  Historical live-test queue snapshot: 508 cells grouped by environment
  (2026-10-07, not the current TBD total); candidate per-WM environments
  are proposed in the
  [live-test environment proposal](research/live-test-vms/proposal.md).
  Table A selections and 2026-10-10 comparison decisions are recorded in
  [decisions](./decisions/reference-and-shortcuts.md#reference-outcome-authority);
  priority native TBD rows R-WS-02, R-WS-04, R-WS-05, R-START-03, R-MAX-01 remain.
````

### Original backlog lines 2076-2082

- DEFERRED 2026-10-10 | Reference matrix follow-ups | Both host-source pinning
  (GNOME Shell/Mutter/wlroots; former estimate 19 cells, now 40 host-source
  candidates after reclassification: 38 PaperWM, 2 sway) and fixture/scenario
  wording tightening (302 cells across 92 rows) are deferred. Revisit wording
  only when a specific open decision depends on a row; neither is P1 work.
  [Residual ledger](reference-source-fill-remaining.md),
  [decision](../../decisions/reference-and-shortcuts.md#reference-outcome-authority).
