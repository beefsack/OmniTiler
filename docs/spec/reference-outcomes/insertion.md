# Insertion (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

## 1. Insertion / splits

Windows D01/D03 offline disposition 2026-10-13: shared overlay and after-focused
contracts unchanged; native approval/denial, explicit switching and chained
pointer/focus outcomes remain TBD. No KWin stacking/focus-fence inference or new
Windows behavior ([verification and exact journeys](../../changes/archive/windows-item22-verification.md)).

<a id="scrolling-backfill-additive-existing-wide-tables-above-unchanged"></a>
<a id="r-ins-01-backfill-ordinary-third-window-admission-scrolling-profiles"></a>
### R-INS-01: ordinary third-window admission

- Given (tree profiles): `H[A,B*]`, B projected 1200x600

- Given (column profiles): `COL[C1[A],C2[B*]]`, each 0.5W, viewport showing both;
  shipped defaults per profile. The tree fixture `H[A,B*]` has no stated
  widths/viewport, so there is no exact projection - this is a separate
  column Given reusing the same A/B/C identities and open-C action.

- When: Open C with B still focused.

- When (column leg): open C (same ordinary-open verbs as R-INS-03 per profile).

- Observe: Split axis + position of C

- Observe (column leg): same-column vs new-column admission, position relative to
  focus, widths stable vs rescaled, newcomer focus and viewport.

- Then COSMIC: Splits B's long edge (side-by-side), C appended after B, focus C; `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)` + `S(S-cos-mapfocus)`
- Then Hyprland/Dwindle: Parent-geometry side-by-side split of B's long edge; C before/after B TBD (pointer half under follow_mouse); ordinary newcomer focused; `S(S-hyp-ins)` + `S(S-hyp-newfocus)`
- Then bspwm: Splits B's long edge side-by-side (1200x600 landscape selects vertical under longest-side; equal `H[A,B]` halves give a 2400x600 work area by index notation, so no absolute origin is needed for this Observe), C second child after B, newcomer focused; `S(S-bsp-ins)` + `S(S-bsp-insert)`
- Then i3: `H[A,B,C*]`: retains H (no geometry-driven axis under the splith embedding); C inserted after focused B, newcomer focused; `S(S-i3-ins)`
- Then xmonad/Tall+Navigation2D: Start `H[A,B*]` is exactly two-window Tall (master A left, B stack right side-by-side); `insertUp` C above focused B yields StackSet order `[A,C*,B]` with newcomer StackSet focus, projected `H[A,V[C*,B]]` (master left half, C/B stacked right) via `tile`/`splitHorizontallyBy`+`splitVertically`, not a long-edge H split; B's fixture frame 1200x600 fills the right half, so the work area is 2400x600 and post-admission tile allocations are A `(0,0,1200,600)`, C `(1200,0,1200,300)`, B `(1200,300,1200,300)` (work-area-relative allocations before border subtraction in `tileWindow`); shares rescale deterministically (master keeps `frac` 1/2, B goes 1/2 to 1/4, C takes 1/4). `S(S-xmo-ins)` + `S(S-xmo-layout)` + `S(S-xmo-admit)`
- Then sway: `H[A,B,C*]`: retains H (no geometry-driven axis; shipped landscape default H); C inserted after focused B via the focus-inactive anchor, ordinary newcomer focused; `S(S-sway-ins)` + `S(S-sway-wsdefault)`
- Then qtile/Columns: Columns admits C into the focused column at the current position (insert_position=0 inserts at current, pushing B after) with newcomer focus; under the two-column default (num_columns=2, align right, split) C stacks with B vertically: projected `H[A,V[C*,B]]` with equal height shares; widths stable (at the column limit, no new column); no scrolling viewport (all columns share the group width); `S(S-qti-default)` + `S(S-qti-add)` + `S(S-qti-colmode)`
- Then awesome/tile: Displayed start `H[A,B*]` projects as two-window tile with tiled-client order `[A,B]` (A master left, B stack right side-by-side; display-implied order, not an admission-sequence claim); manage prepends C first via client_array_push yielding tiled-client order `[C*,A,B]` with newcomer focus via the shipped global rule, projected `H[C*,V[A,B]]` (C master left half, A/B stacked right) via `do_tile`/`tile_group`, not a long-edge H split; B's fixture frame 1200x600 fills the right half, so the work area is 2400x600 and post-admission tile allocations are C `(0,0,1200,600)`, A `(1200,0,1200,300)`, B `(1200,300,1200,300)` (work-area-relative allocations before border subtraction and hint shaping in `tile_group`/`apply_size_hints`); shares rescale deterministically (master keeps `mwfact` 1/2 with newcomer C taking it, prior master A joins the stack so both A and B go from 1/2 to 1/4). `S(S-awe-tile)` + `S(S-awe-manage)`
- Then niri: C opens as a new column right after the active column at the
  default width with existing widths kept (no rescale); under Smart with
  no pending fullscreen C activates and takes focus, and the view animates
  to C with the minimal fit under shipped `Never`.
  `S(S-nir-ins)` + `S(S-nir-base)` + `S(S-nir-view)`.
- Then PaperWM: C inserts as a new column at selected+1 (after B) under the shipped RIGHT default, activated on show; existing columns keep widths (per-column targetWidth, no rescale), C takes its own frame/preferred width; viewport keeps C visible via minimal ensuredX scroll under DEFAULT. `S(S-pap-ins)` + `S(S-pap-layout)` + `S(S-pap-view)`.
- Then karousel/Lazy: C opens a new column after the last-focused column (B's column under the B* tiled focus), appended at the bottom; existing columns keep widths (new-column width from the newcomer preferred width clamped into [min,max]; `columnsSetX` repositions only); newcomer focus TBD (fixture states no protocol, X11 user-time/startup/session, Wayland token/app-id/transient-serial, or FSP/rules inputs selecting the host activation fork); viewport is a deterministic conditional (Lazy minimal scroll toward the last-focused column: moves only if the newcomer is accepted and not contained, else stays on B); exact settled widths TBD (fixture states no newcomer frame/min/max inputs for the clamp). `S(S-kar-ins)` + `S(S-kar-fltanchor)` + `S(S-kar-scroll)` + `S(S-kar-min)` + `S(S-kwin-manage)` + `S(S-kwin-add)`.
- Then paneru: ordinary C takes the fresh path as a new Single column appended at the end after B (no rule index under shipped defaults; focused B holds the last index so the after-focus slot misses and it appends); newcomer focus is synthesized to C (`dont_focus` false); existing columns keep widths with no rescale; the strip reshuffles minimally to bring C into view with no centering (`auto_center` off). `S(S-pan-fresh)` + `S(S-pan-base)` + `S(S-pan-stripwidth)` + `S(S-pan-colops)`.
- Then Ours KDE: Long-edge split at focused leaf; `D(D-dec-x)`. Selected After focused, no setting (User 2026-10-12 D03, [decision](../../decisions.md#placement-minimums-and-startup-adoption)); native order/journey TBD.
- Then Ours Windows: same selected long-edge / After focused policy, no setting; native order/journey TBD ([D03 decision](../../decisions.md#placement-minimums-and-startup-adoption)).
- Variant hook: V-INS-AXIS.

<a id="r-ins-02-backfill-stack-admission-scrolling-profiles"></a>
### R-INS-02: stack admission

- Given (tree profiles): Stack `S[A*,B]` (COSMIC)

- Given (column profiles): the same A/B identities and open-C action as the tree
  fixture `S[A*,B]`, column-qualified to `COL[C1[S[A*,B]]]` only where the
  profile supports a tabbed-display column with stated membership and
  active tab. PaperWM accordion and visible vertical stacking are not
  exact S equivalents; paneru native tabs are a distinct nesting
  predicate (see R-COL-10).

- When: Open C.

- When (column leg): open C (ordinary open per profile).

- Observe: Does C join the active stack

- Observe (column leg): C joins the tabbed display as a tab vs ordinary column
  admission; membership, order, active tab, and focus.

- Then COSMIC: Joins active stack as appended tab, newcomer active, focus stays stack; `D(D-ref)` + `S(S-cos-mapfocus)`
- Then Hyprland/Dwindle: No auto-created tab stack here: read as a Hyprland group analogue, C auto-joins the focused group as the tab after current (`insert_after_current`), newcomer current and focused; fresh groups still need a directional create/join; `S(S-hyp-group)` + `S(S-hyp-newfocus)`
- Then bspwm: No groups: one window per leaf, so C ordinary-tiles at the focused leaf instead of joining; `D(D-ref)` + `S(S-bsp-insert)`
- Then i3: `S[A,C*,B]` (tabbed embedding of the fixture stack): C joins the focused tabbed parent as the tab after focused A, newcomer active; `S(S-i3-ins)` + `S(S-i3-layout)`
- Then xmonad/Tall+Navigation2D: no-counterpart (no tabbed-stack group in the Tall/core/contrib profile; no join primitive here, so C has no stack to join); `S(S-xmo-layout)`
- Then sway: `S[A,C*,B]` (tabbed embedding of the fixture stack): C joins the focused tabbed parent as the tab after focused A, newcomer active; stacked embedding analogous; `S(S-sway-ins)` + `S(S-sway-layout)`
- Then qtile/Columns: No tab-stack join in this profile (split/unsplit columns only): C ordinary-admits at the focused position with newcomer focus, not as a tab; exact order/frames TBD; `S(S-qti-add)`
- Then awesome/tile: No tab-stack join in this profile: C ordinary-admits appending last in tiled order with newcomer focus, not as a tab; exact frames TBD; `S(S-awe-tile)` + `S(S-awe-manage)`
- Then niri: C does not join as a tab - ordinary open always wraps a new
  column after the active column, never into the existing one (its
  membership and active tab untouched); under Smart with no pending
  fullscreen the new column activates with focus to C. `S(S-nir-ins)`.
- Then PaperWM: fixture-inapplicable - no tabbed-display column in this profile (columns always show all windows vertically, no tabbed/stacked display verb); ordinary open creates a new column per the insertion path. `S(S-pap-acts)` + `S(S-pap-ins)`.
- Then karousel/Lazy: C does not join as a tab - ordinary open creates a new single-window column (stacked display exists behind `toggleStacked`, off by default, so no tabbed display or active tab to report); newcomer focus TBD (fixture states no protocol, X11 user-time/startup/session, Wayland token/app-id/transient-serial, or FSP/rules inputs selecting the host activation fork). `S(S-kar-ins)` + `S(S-kar-fltanchor)` + `S(S-kwin-manage)` + `S(S-kwin-add)`.
- Then paneru: fixture-inapplicable - `Stack` is ordered visible stacking while `Tabs` holds app-native tabs, so the S fixture has no counterpart here; native-tab nesting stays under R-COL-10. `S(S-pan-model)`.
- Then Ours KDE: TBD (stacks unselected)
- Then Ours Windows: TBD (stacks unselected)
- Variant hook: V-GROUP-STACK.


## New scenarios (GWT, piece B1; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours cells cite Engine + adapter source at `9241c94`
(`S(S-ours-ins)`); selected intent and doc assertions are never evidence.

### R-INS-03: first admission on an empty workspace

- Given (tree profiles): `WS1=H[]`, no focused window. Ordinary resizable
  windows, no rules, scale 1, zero gaps for reference geometry.
- Given (column profiles): empty column strip / empty Space row per
  profile; shipped defaults apply (`S(S-nir-base)`, `S(S-pap-base)`,
  `S(S-kar-base)`, `S(S-pan-base)`); record actual viewport before fill.
- When: open A. Native verbs/config per profile: COSMIC ordinary admission
  without explicit direction; Hyprland ordinary map under the Dwindle
  profile; bspwm ordinary manage; i3/sway ordinary manage; qtile Columns
  admit; awesome manage + tag arrange; niri/PaperWM/karousel/paneru
  ordinary open at shipped defaults; Ours Engine `insert_tiled` via the
  adapter `admit` op (`S(S-ours-ins)`).
- Observe: first tile size/anchor and focus vs leaving the app
  unmanaged/floating.
- Then COSMIC: A tiles full work area, focused. `S(S-cos-axis)` + `S(S-cos-mapfocus)`.
- Then Hyprland/Dwindle: A tiles full work area, focused. `S(S-hyp-ins)` + `S(S-hyp-newfocus)`.
- Then bspwm: A tiles full desktop, focused. `S(S-bsp-insert)` + `S(S-bsp-ins)`.
- Then i3: A tiles workspace, focused. `S(S-i3-ins)`.
- Then xmonad/Tall+Navigation2D: A tiles as sole stack entry, focused. `S(S-xmo-admit)` + `S(S-xmo-ins)`.
- Then sway: A tiles workspace, focused. `S(S-sway-ins)` + `S(S-sway-wsdefault)`.
- Then qtile/Columns: A tiles, focused. `S(S-qti-add)` under `S(S-qti-default)`.
- Then awesome/tile: A tiles full tag, focused. `S(S-awe-tile)` + `S(S-awe-manage)`.
- Then niri: A opens a new column at index 0 at the default column width;
  under Smart with no pending fullscreen it activates with focus to A.
  `S(S-nir-base)` + `S(S-nir-ins)`.
- Then PaperWM: A inserts as a new column at index 0 (empty space, no selection, RIGHT default), activated on show; width from its own frame/preferredWidth per the column layout, single column centered one-shot. `S(S-pap-base)` + `S(S-pap-ins)` + `S(S-pap-layout)`.
- Then karousel/Lazy: A opens a new column at the start of the empty grid, width from the client preferred width clamped into [min,max]; newcomer focus TBD (fixture states no X11 user-time/startup/session, Wayland token/app-id, or tabfocus/policy inputs selecting the host activation fork; the no-active-window branches still require the ordinary wantsInput/non-popup/non-special gates). `S(S-kar-base)` + `S(S-kar-ins)` + `S(S-kar-min)` + `S(S-kwin-manage)` + `S(S-kwin-add)`.
- Then paneru: A appends as a Single column at index 0 of the empty active strip (no rule index and no focused window to follow under shipped defaults); newcomer focus is synthesized to A (`dont_focus` false); A takes its OS frame width with the lone strip pinned to the viewport edge. `S(S-pan-fresh)` + `S(S-pan-base)` + `S(S-pan-stripwidth)`.
- Then Ours KDE: A is the single new leaf with full-work-area geometry applied via the admit geometry plan; newcomer is the desired focus leaf. `S(S-ours-ins)` + `S(S-ours-admit)`; native activation TBD.
- Then Ours Windows: same single-leaf leg as Ours KDE via the shared Engine plus the managed-claim gate. `S(S-ours-ins)` + `S(S-ours-admit)`; native activation TBD.
- Variant hook: V-INS-AXIS.

### R-INS-04: chained admission with a fixed pointer

- Given (tree profiles): `H[A,B*]`, landscape work area, pointer fixed
  outside eligible windows. Ordinary windows, no refocus between legs.
- Given (column profiles): `COL[C1[A],C2[B*]]` at shipped defaults;
  viewport recorded; pointer fixed outside eligible windows.
- When: open C, then D, then E; do not refocus. Native verbs/config: same
  ordinary-open verbs as R-INS-03 per profile; pointer/focus update policy
  per profile TBD (live-test leg).
- Observe: topology after each admission - spiral/orientation alternation,
  current-leaf long edge, flat insertion or master/stack; column sizes
  stable vs reflow. First leg reuses R-INS-01; legs 2-3 are live-test
  candidates (admission plus pointer/focus updates and repeated geometry
  recalculation).
- Fixture-equivalence qualifier: the landscape-work-area fixture does NOT
  establish B at 1200x600, so first-leg reuse below is policy-branch only;
  pixel dimensions stay TBD and are never re-voted here.
- Then COSMIC: C splits B's long edge after B, focused (axis follows B's actual frame); D then splits C's long edge after C with newcomer focus, and E splits D's long edge after D the same way (anchor is focus MRU each leg, so the fixed pointer is irrelevant). Exact axes follow each anchor's realized frame and pixel frames TBD (F: anchor frames plus output dimensions unrecorded). `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)` + `S(S-cos-mapfocus)`.
- Then Hyprland/Dwindle: C splits B's long-edge axis with newcomer focus (anchor is the active tile B under `use_active_for_splits`, pointer irrelevant to the anchor); newcomer side follows the pointer half under follow_mouse, side TBD (F: pointer fixed outside eligible windows but unrecorded). Legs 2-3 run the same path with no refocus between legs: D anchors at focused C splitting C's long edge with newcomer focus, then E anchors at D the same way; per-leg axes follow each anchor's realized frame, exact axes TBD (F: anchor frames unrecorded), and per-leg sides TBD (F: same unrecorded pointer). `S(S-hyp-ins)` + `S(S-hyp-newfocus)` + `S(S-hyp-defaults)`.
- Then bspwm: C splits B's long edge as second child after B, focused (axis follows B's actual frame); legs 2-3 follow the same admission chain at the current focus (D after C, E after D, each newcomer focused; the fixed pointer has no admission branch) with each axis following its anchor's realized frame, exact axes TBD (F: anchor frames plus output dimensions unrecorded). `S(S-bsp-ins)` + `S(S-bsp-insert)`.
- Then i3: flat `H[A,B,C*]`, C after B, focused; D then admits after C as `H[A,B,C,D*]` focused, E after D as `H[A,B,C,D,E*]` focused (each leg attaches after the focus-stack tiling head with ordinary newcomer focus; the fixed outside pointer selects no anchor). No geometry-driven axis; pixel dimensions never re-voted per the row qualifier. `S(S-i3-ins)`.
- Then xmonad/Tall+Navigation2D: StackSet `[A,C*,B]` projected `H[A,V[C*,B]]`, newcomer focused; legs 2-3 follow the same admission path with no refocus between legs: D inserts via `insertUp` above C with newcomer focus (order `[A,D*,C,B]`), then E above D the same way (order `[A,E*,D,C,B]`; the fixed pointer has no admission branch in `manage`/`insertUp`); Tall shares recompute each leg (master `frac`, stack split equally, no axis alternation); exact pixel frames TBD (F: landscape work area has no dimensions, and the row qualifier bars re-voting R-INS-01 pixels here) plus pointer/focus-update settlement TBD (L: live-test leg per the row Observe). `S(S-xmo-ins)` + `S(S-xmo-layout)` + `S(S-xmo-admit)`; queued.
- Then sway: flat `H[A,B,C*]`, C after B, focused; D then admits after C as `H[A,B,C,D*]` focused, E after D as `H[A,B,C,D,E*]` focused (each leg anchors after the focus-inactive newcomer with ordinary newcomer focus; the fixed outside pointer selects no anchor). No geometry-driven axis; pixel dimensions never re-voted per the row qualifier. `S(S-sway-ins)` + `S(S-sway-wsdefault)`.
- Then qtile/Columns: C joins the focused column at the current position pushing B after, focused; legs 2-3 repeat the same path with no refocus between legs (D inserts at current above C with newcomer focus, then E above D the same way; the fixed pointer selects no anchor, admission follows column current); widths stable (no new column at the limit), in-column heights re-split equally each leg. `S(S-qti-default)` + `S(S-qti-add)` + `S(S-qti-colmode)`.
- Then awesome/tile: tiled-client order `[C*,A,B]` projected `H[C*,V[A,B]]`, newcomer focused; legs 2-3 follow the same admission path with no refocus between legs: D prepends first via manage push with newcomer focus (order `[D*,C,A,B]`), then E prepends first the same way (order `[E*,D,C,A,B]`; the fixed pointer has no admission branch in manage push); tile shares recompute each leg (master `mwfact`, stack split equally, no axis alternation); exact pixel frames TBD (F: landscape work area has no dimensions, and the row qualifier bars re-voting R-INS-01 pixels here) plus pointer/focus-update settlement TBD (L: live-test leg per the row Observe). `S(S-awe-tile)` + `S(S-awe-manage)`; queued.
- Then niri: each leg opens a new column after the active column at the
  default width with existing widths kept (no rescale); C anchors at B,
  then D anchors at the Smart-activated C and E at D the same way (the
  fixed pointer selects no anchor - admission follows the active column),
  each newcomer activating with focus under Smart (no pending fullscreen)
  with the view animating minimal-fit under `Never`. Links R-COL-01.
  `S(S-nir-ins)` + `S(S-nir-base)` + `S(S-nir-view)`.
- Then PaperWM: each leg inserts at selected+1 under the shipped RIGHT default with the newcomer activated on show (selection follows the newcomer, so D follows C and E follows D); existing columns keep widths per the column layout and each newcomer is kept visible; the anchor is the selected window, so the fixed pointer is irrelevant. `S(S-pap-ins)` + `S(S-pap-layout)` + `S(S-pap-view)`.
- Then karousel/Lazy: leg 1 per R-INS-01 (new column after B's column, appended at the bottom; existing widths stable; focus and exact widths TBD for that row's missing activation and frame/min/max inputs); legs 2-3 TBD (each anchor follows the last-focused column, which turns on whether the prior newcomer was accepted under those same missing inputs). `S(S-kar-ins)` + `S(S-kar-fltanchor)` + `S(S-kar-scroll)` + `S(S-kwin-manage)` + `S(S-kwin-add)`; links R-COL-01.
- Then paneru: leg 1 per R-INS-01 (C appended after B with synthesized focus); D then appends after C and E after D the same way, each anchor following the last-synthesized focus with newcomer focus synthesized; existing widths stay stable with no rescale and each leg reshuffles minimally with no centering. `S(S-pan-fresh)` + `S(S-pan-base)` + `S(S-pan-stripwidth)` + `S(S-pan-colops)`; links R-COL-01.
- Then Ours KDE: leg-1 topology per `S(S-ours-ins)` (focused-leaf wrap old/new, admission-axis split, equal shares), newcomer desired focus; selected re-application per leg (User 2026-10-12 D03 (a), [decision](../../decisions.md#placement-minimums-and-startup-adoption)); legs 2-3 native outcomes TBD.
- Then Ours Windows: leg-1 topology per `S(S-ours-ins)` (same old/new wrap), newcomer desired focus; same selected per-leg policy, legs 2-3 native outcomes TBD ([D03 decision](../../decisions.md#placement-minimums-and-startup-adoption)).
- Variant hook: V-INS-AXIS.

### R-INS-05: admission while an ordinary float has focus

- Given (tree profiles): `H[A,B]` plus ordinary `F*` (float focused);
  prior tiled focus B. Record the actual pre-action anchor per profile.
- Given (column profiles): column fixture plus ordinary focused float;
  prior tiled focus recorded; shipped defaults apply.
- When: open C. Native verbs/config: same ordinary-open verbs as R-INS-03;
  eligible-anchor filtering per profile is the discriminating leg
  (source-clear route: do not assume floats split).
- Observe: float focus as anchor, fallback to last tile, root or pointer
  target; newcomer layer and focus.
- Then COSMIC: C tiles at the last-active tile (float focus is not in the tiling tree, so B anchors), newcomer focused. `S(S-cos-last)` + `S(S-cos-mapfocus)`.
- Then Hyprland/Dwindle: float focus excluded from the active-tiled candidate, but the anchor is the pointer-hit window under shipped follow_mouse else the active tile - pointer unstated here, so B vs pointer-hit TBD; newcomer tiles focused. `S(S-hyp-ins)` + `S(S-hyp-newfocus)` + `S(S-hyp-defaults)`.
- Then bspwm: C splits the focused float F at its own tree slot - floats stay vacant in place with the slot kept, `manage_window` anchors at the desktop focus with no float exclusion, and `insert_node` wraps F plus the newcomer with C as second child under the shipped polarity; C tiles (ordinary, no float/fullscreen/hidden state) and takes focus. `S(S-bsp-insert)` + `S(S-bsp-ins)` + `S(S-bsp-fltanchor)`.
- Then i3: C tiles at the tiling-focused descendant (float focus excluded), newcomer focused. `S(S-i3-ins)`.
- Then xmonad/Tall+Navigation2D: C inserts above the focused float F via `insertUp` with newcomer focus - `manage` has no float-focus exclusion and F stays in both the stack and the floating map; C tiles (ordinary resizable, neither fixed-size nor transient). `S(S-xmo-admit)` + `S(S-xmo-ins)` + `S(S-xmo-float)`.
- Then sway: C tiles at the focus-inactive tiling anchor (float focus excluded), newcomer focused. `S(S-sway-ins)`.
- Then qtile/Columns: ordinary C admits to the tiled path (no float-rule match) at the current tiled position - a new column opens only while the focused column is non-empty below `num_columns`, otherwise C inserts at current pushing later clients after; focusing float F only blurs the tiled layouts without moving `Columns.current`, so the eligible anchor stays the last tiled current (B); C tiles with newcomer focus. `S(S-qti-add)` + `S(S-qti-fltanchor)`.
- Then awesome/tile: C appends last in global client order regardless of F's focus - `manage` pushes the newcomer at the end (anchor-independent) while the tile partition excludes the float F; C tiles (ordinary) with newcomer focus via the shipped global rule. `S(S-awe-tile)` + `S(S-awe-manage)`.
- Then niri: ordinary C (no floating rule, normal sizing) takes the workspace Auto scrolling branch into a new column after the scrolling-active column - focusing float F only sets the floating-active flag and never moves the scrolling active column, so the anchor stays B; under the fixture's Smart activation (no pending fullscreen) the new column activates, the floating layer deactivates, and keyboard focus follows the layout active window to C; C tiles in the scrolling layer. `S(S-nir-ins)` + `S(S-nir-fltanchor)`.
- Then PaperWM: C inserts tiled at selected+1 under the shipped RIGHT default - ordinary C passes the Normal-only admit filter, and the focused float lives in the separate floating list with focusing it returning before any selection write, so the selected anchor stays B; newcomer activated on show. `S(S-pap-ins)` + `S(S-pap-fltanchor)`.
- Then karousel/Lazy: C tiles in a new column after the last-focused column - `lastFocusedColumn` is written only by the tiled column-focus path, so focusing float F leaves it at B's column, appended at the bottom; newcomer focus TBD (float F holds the active window so the no-active branches are unavailable; fixture states no X11 user-time/startup/session, Wayland token/app-id/transient-serial, or FSP/rules inputs selecting the fork). `S(S-kar-ins)` + `S(S-kar-fltanchor)` + `S(S-kwin-manage)` + `S(S-kwin-add)`.
- Then paneru: fresh C stays managed in the active strip and appends at the end - no rule index or `dont_focus` applies under the shipped defaults, and the focused ordinary float is unmanaged outside the strip so the after-focus lookup misses; newcomer focus is synthesized. `S(S-pan-fresh)` + `S(S-pan-base)`.
- Then Ours KDE: C tiles at the prior tiled focus B (float F holds an exception, not a tile leaf) via the retained Engine convergence route: the evolving tiled focus stays B, and in the 120x80 test domain C splits B's long edge after B with ordinary long-edge admission (`H[A,V[B,C]]`, A keeps its 60x80 half, B/C take 60x40 each); retained focus B until native observation follows C, native newcomer activation pending. `S(S-ours-ins)` + `S(S-ours-admit)` + `S(S-ours-flt-target)`; retained route proven offline under the unchanged admission policy (`session_float_focus_admission` convergence fixture + `float-focus-admission` KWin wire characterization); native activation/journey pending (live TBD, user-owned).
- Then Ours Windows: same retained-route anchor-at-B leg as Ours KDE via the shared Engine (same 120x80 test-domain geometry). `S(S-ours-admit)` + `S(S-ours-flt-target)` + `S(S-ours-ins)`; retained route proven offline (shared convergence fixture); native activation/journey pending (live TBD, user-owned).
- Variant hook: provisional/TBD (no suitable existing hook; do not reuse
  V-FLOAT-FOCUS, which covers directional search, not admission anchor).

### R-INS-06: open over an overlay (maximized, plus fresh fullscreen variant)

- Given (tree profiles): `H[A,B*]`; prepare `B:max`; record the actual
  pre-action tree per profile. Fresh variant leg: `B:full`. Maximize and
  fullscreen are separate fresh legs, not synonyms.
- Given (column profiles): `COL[C1[A],C2[B*]]`; prepare `B:max` where the
  profile has a native maximize; else applicability TBD; fresh `B:full`
  variant recorded separately.
- When: open C. Native verbs/config: ordinary open per profile; overlay
  verbs per profile TBD at pin (live-test leg: overlay admission, render,
  and focus paths).
- Observe: overlay retained, cleared, or covering a newly admitted window;
  newcomer focus/visibility and underlying layout.
- Source audit 2026-10-12: [D01 evidence and native baseline](../../research/d01-fullscreen-admission.md)
  supplies pinned defaults and line-level citations for the non-scrolling
  profiles and niri. Focus, visibility and fullscreen state are independent;
  retaining fullscreen need not retain its cover. Source analysis only,
  runtime/native settlement unverified. Transients are separate from ordinary C.
- Then COSMIC: `B:max` leg - opening C first unmaximizes B back to its
  tiling slot, then C fresh-maps at the MRU anchor B splitting B's long
  edge after B with newcomer focus. Fresh `B:full` leg - B stays a
  retained fullscreen target (no exit path in admission); C admits at
  the last-active tile A (fullscreen B is not in the tiling tree, so
  the anchor skips it), splitting A's long edge after A with newcomer
  focus; B's fullscreen state is retained but rendered below C (normal
  layers precede unfocused fullscreen in front-to-back render order).
  Exact axes/frames TBD
  (F: anchor frames plus output and work-area geometry unrecorded).
  `S(S-cos-maxpolicy)` + `S(S-cos-admit)` + `S(S-cos-last)` +
  `S(S-cos-axis)` + `S(S-cos-newgroup)` + `S(S-cos-mapfocus)` +
  `S(S-cos-fsreq)`; [render-order audit](../../research/d01-fullscreen-admission.md#cosmic).
- Then Hyprland/Dwindle: `B:max` leg - C ordinary-admits at the active-tile anchor B splitting B's long edge with newcomer focus, while the existing maximize is cleared first (shipped `on_focus_under_fullscreen=2` exits the covering window to NONE on the newcomer map); B retained tiled. Fresh `B:full` leg - same path: B's fullscreen is exited on C's admission and C admits at B with newcomer focus. Newcomer before/after B TBD in both legs (F: pointer unstated, follow_mouse half decides). `S(S-hyp-ins)` + `S(S-hyp-newfocus)` + `S(S-hyp-fs)`.
- Then bspwm: `B:max` leg no-counterpart (no maximize in the state inventory: maximize flags are client-hint bits, not tree state, so the maximized qualifier is void on admission; `S(S-bsp-fs)`); fresh `B:full` leg: opening C focus-clears B's fullscreen to its last state via `neutralize_occluding_windows` (fullscreen outranks tiled in `stack_level`, so the newcomer focus sees B covered and restores it), B is retained tiled, and C splits B as second child after B with newcomer focus; the split axis follows B's realized frame, exact axis TBD (F: anchor frames plus output and work-area geometry unrecorded). `S(S-bsp-fs)` + `S(S-bsp-insert)` + `S(S-bsp-ins)`.
- Then i3: `B:max` leg no-counterpart (no maximize verb; `S(S-i3-max)`); fresh `B:full` leg: C tiles after B with no newcomer focus (cover inserts after the focused head with `set_focus` false), B's fullscreen cover retained with no exit path in admission (transient-only `PDF_LEAVE` exit inapplicable to ordinary C), focus stays B. `S(S-i3-max)` + `S(S-i3-fs)` + `S(S-i3-ins)`.
- Then xmonad/Tall+Navigation2D: `B:max` leg no-counterpart (no maximize state; `S(S-xmo-layout)`); fresh `B:full` leg applicable via `S(S-xmo-ewmh)` (float-based EWMH fullscreen; admission itself tiles, fullscreen arrives only via the post-map ClientMessage path): opening C has no overlay-clear branch in `manage`, so B's fullscreen float is retained; C fresh-admits tiled via `insertUp` above B with newcomer StackSet and X focus (`setTopFocus` on the newcomer peek); C tiles underneath B's covering full-screen float (arrange restacks floats first-on-top). `S(S-xmo-layout)` + `S(S-xmo-ewmh)` + `S(S-xmo-ins)` + `S(S-xmo-admit)` + `S(S-xmo-arrange)` + `S(S-xmo-topfocus)`.
- Then sway: `B:max` leg no-counterpart (no maximize verb; `S(S-sway-max)`); fresh `B:full` leg: C admits after B as a sibling with no newcomer focus (active-workspace fullscreen blocks `should_focus`), B's fullscreen cover retained with no exit path in admission, focus stays B. `S(S-sway-max)` + `S(S-sway-full)` + `S(S-sway-ins)`.
- Then qtile/Columns: `B:max` leg - maximizing B floats it out of the tiled Columns (`mark_floating` removal, overlay retained at work-area size), collapsing the group to single `C1[A]`; C then fresh-admits as a new column after A with newcomer group-focus while B's maximized overlay is retained (no clear path in admission). Fresh `B:full` leg - fullscreen B keeps its tiled slot (`mark_floating` skips removal for fullscreen), so C inserts at current in C2 above B with newcomer group-focus while B's fullscreen state is retained. Final cover in both legs (C underneath vs raised over B) TBD (F: backend unspecified - X11 restacks on focus via `check_stacking`/`change_layer` while Wayland reparents to `LAYER_MAX`/`LAYER_FULLSCREEN` with no shared order; L: overlay render/pixel settlement is the row's live-test leg; newcomer focus itself is code-sourced via stealable-focus admission). `S(S-qti-fs)` + `S(S-qti-maxfloat)` + `S(S-qti-float)` + `S(S-qti-add)` + `S(S-qti-remove)`.
- Then awesome/tile: `B:max` and fresh `B:full` retain B's respective boolean/geometry and exclude B from tiled clients (`S(S-awe-fs)` + `S(S-awe-float)` + `S(S-awe-tile)`). C tiles with newcomer focus and is raised by shipped rule activation; C is above B, not underneath. B's fullscreen stacking layer depends on being most recently raised, so raising C demotes B without clearing fullscreen; maximize already uses normal stacking. Tiled allocation `[C,A]` (C master, A stack); exact frames/runtime settlement TBD. `S(S-awe-manage)`; [raise/layer source correction](../../research/d01-fullscreen-admission.md#awesome).
- Then niri: `B:max` leg - C opens as a new column after the active B at
  the default width with existing widths kept; B's maximized flag is
  retained (no clear path in admission) and under Smart C activates and
  takes focus (the fence is fullscreen-only), the view animating
  minimal-fit to C. Fresh `B:full` leg - same admission with B's
  fullscreen retained, but Smart withholds activation against the active
  pending fullscreen so focus stays B and the view stays on B's fullscreen
  cover. `S(S-nir-ins)` + `S(S-nir-maxfs)` + `S(S-nir-base)` +
  `S(S-nir-view)`.
- Then PaperWM: `B:max` leg - at shipped defaults native maximize converts to width-maximize, so B stays tiled and wide; C inserts at selected+1 and activates on show with B retaining its width. Fresh `B:full` leg - C inserts tiled at selected+1 and is selected/activated while B's fullscreen state is retained (no exit path in admission). `S(S-pap-ins)` + `S(S-pap-widthmax)` + `S(S-pap-layout)` + `S(S-pap-fsframe)`.
- Then karousel/Lazy: C opens a new column after B's column, appended at the bottom; existing widths stable; B's max/fullscreen overlay is retained on admission with no clear path (existing-member `skipArrange`), and a later focus change restores the old focus-taker to tiled; whether C is accepted (and thus whether B is cleared and the viewport moves under the Lazy conditional) stays TBD (fixture states no protocol, X11 user-time/startup/session, Wayland token/app-id/transient-serial, or FSP/rules inputs selecting the host activation fork). `S(S-kar-ins)` + `S(S-kar-maxfs)` + `S(S-kar-fltanchor)` + `S(S-kar-scroll)` + `S(S-kwin-manage)` + `S(S-kwin-add)`.
- Then paneru: TBD (both legs; `Fullscren` is a column kind per `S(S-pan-model)`, native preparation requires confirmation). `B:max` preparation is host zoom with no paneru counterpart (AX observes `AXFullScreen` only per `S(S-pan-axfs)`; no public macOS source). Fresh `B:full` pins a host `Fullscren` strip per `S(S-pan-fsfocus)`; ordinary admission over that pinned strip inserts as a Single column per the fresh path (`S(S-pan-fresh)`), with native-Space visibility/focus TBD.
- Then Ours KDE: C structurally admits with B slot and fullscreen/maximized state retained and geometry writes skipped. Convergence follows valid observed tiled focus (B if native kept B, C if native activated C), otherwise retained focus/MRU; it does not unconditionally focus C. Delivered offline: native automatic activation retains slots/state; automaticFocusGuard reobserves after geometry and fences automatic focus on overlay/midapply/domain/nativefocus races; explicit directional shortcuts and sends stay effective. Final native focus/stacking/input and overlay-close cleanup TBD, user-owned ([record](../../changes/archive/kde-fullscreen-activation-and-send.md); [current-source chain](../../research/d01-fullscreen-admission.md#current-omnitiler-kde-behavior)). Overlay-unfloat `S(S-ours-overlay-unfloat)` is not admission evidence.
- Then Ours Windows: TBD, same split as Ours KDE; native foreground rules remain host-specific.
- Selected Ours (User 2026-10-12 D01 (a)): [native activation ownership](../../decisions.md#placement-minimums-and-startup-adoption) for both overlay legs, delivered offline with automatic-focus fences ([record](../../changes/archive/kde-fullscreen-activation-and-send.md)). This selection does not establish runtime focus/visibility.
- Variant hook: V-MAX-MODEL for the `B:max` leg; V-FS-SLOT for the fresh
  `B:full` leg.

#### R-INS-06 explicit-selection discriminator (fresh fullscreen leg)

- Given: the same A/B membership, B fullscreen and focused; do not open C.
- When: explicitly select existing A by target/cycle/taskbar-equivalent.
  Directional navigation is recorded separately; it is not the same action.
- Observe: A focus and visibility, B fullscreen flag versus stacking/viewport.
- Evidence: source-only [per-WM citations](../../research/d01-fullscreen-admission.md#reference-source-evidence);
  game/runtime settlement unverified. Selected Ours policy is linked below.

| Profile | Sourced explicit-selection outcome / uncertainty |
|---|---|
| COSMIC | A focused/rendered above B; B fullscreen retained below |
| Hyprland | Default tiled target selection exits B and focuses A; modes ignore/take-over differ. Directional movefocus fenced by default |
| bspwm | A focus neutralizes occluding B to last state |
| i3 | Criteria focus / pager activation calls unblock, exits B, focuses A; directional/relative navigation can be fenced |
| sway | Explicit target focus disables covering fullscreen, focuses A; directional focus instead goes to outputs |
| xmonad profile | A focused below retained B fullscreen float |
| awesome | Shipped focus.byidx raises A above retained B; focus-only without raise differs |
| qtile | A group-focused, B fullscreen retained; final cover backend-dependent/TBD. X11 layer demotion can keep B atop shared layer |
| niri (non-voting) | View moves to A; B remains fullscreen offscreen |
| Ours KDE | Selected: honor explicit native switching, no tiler focus actuation while fullscreen is involved ([D01 decision](../../decisions.md#placement-minimums-and-startup-adoption)); native settled result TBD |
| Ours Windows | Same native-ownership consideration under Windows foreground rules; runtime TBD, no KWin-layering claim |

### R-INS-07: open with a destination rule into an inactive workspace/output

- Given: active `L:WS1=H[A*]`; inactive `R:WS2=H[B,C]`, WS2 prior focus C;
  pointer on L. Fixture-only destination rule (one case): the newly opened
  D belongs to WS2/R. The rule must be recorded with the run.
- Given (column profiles): same workspace/output split with column
  fixtures per workspace; shipped defaults apply; rule recorded.
- When: open D under that rule. Native verbs/config: launch routing plus
  inactive-workspace admission per profile TBD at pin (live-test leg).
- Observe: target-local anchor vs global focus; target admission without
  stealing source focus or switching output.
- Then COSMIC: D routes to WS2 via the fixture-declared workspace-target activation token and admits at the target MRU anchor (C), D after C on C's long-edge split; source focus stays A with an urgent mark on WS2 and no workspace/output switch; exact nested embedding and frames TBD (genuinely require the unknown target geometry). `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-mapfocus)` + `S(S-cos-wsroute)`.
- Then Hyprland/Dwindle: a fixture-declared silent `workspace` windowrule routes D to WS2 with no workspace switch and no newcomer focus (source focus stays A); the target anchor falls to `getClosestNode` for the mouse-on-L point (PWORKSPACE is inactive, so the mouse-hit and same-workspace active branches do not apply), leaving D before/after C and exact geometry TBD. `S(S-hyp-ins)` + `S(S-hyp-newfocus)` + `S(S-hyp-winws)`.
- Then bspwm: a fixture-declared `desktop=WS2` rule routes D to WS2 at the desktop focus C, inserted as second child at the anchor (D after C); the inactive target is hidden (no switch) and activated-not-focused under the shipped follow-off default, so source focus stays A. `S(S-bsp-insert)` + `S(S-bsp-ins)` + `S(S-bsp-wsroute)`.
- Then i3: a fixture-declared `assign` (workspace or number) routes D to WS2 at the tiling-focused descendant C, opened after focus (D after C); the invisible target marks urgency with no focus steal (source focus stays A) and no workspace switch in the manage path. `S(S-i3-ins)` + `S(S-i3-assign)`.
- Then xmonad/Tall+Navigation2D: under a fixture-declared custom `doShift WS2` (shipped core `manageHook` is MPlayer-only), D shifts to WS2 and inserts above the target focus C via `insertUp` (D above C, newcomer current on that stack); `onWorkspace` restores the current view, so no workspace switch and visible focus stays A. `S(S-xmo-admit)` + `S(S-xmo-shift)` + `S(S-xmo-doshift)`.
- Then sway: a fixture-declared `assign` routes D to WS2 (created if needed); admission anchors after the target focus-inactive node C with the ordinary sibling policy, but the newcomer takes no focus on the inactive workspace and no workspace switch occurs in the map path, so source focus stays A. `S(S-sway-ins)` + `S(S-sway-assign)`.
- Then qtile/Columns: a fixture-declared Group `matches` rule assigns D to the WS2 group via `togroup` without `switch_group` (stays, no screen switch); Columns opens a new column if below `num_columns=2`, otherwise inserts in the current column at `insert_position=0`, making D current in that group while visible focus stays A; exact embedding TBD because this row does not declare column membership or setup order. `S(S-qti-add)` + `S(S-qti-dgroup)`.
- Then awesome/tile: a fixture-declared ruled tag routes D to the WS2 tag where it appends last (D after C); activation only focuses when visible, so the inactive D takes no focus (urgent instead) and nothing switches tags under the shipped rule (`switch_to_tags` opt-in, unset); visible focus stays A. `S(S-awe-tile)` + `S(S-awe-manage)` + `S(S-awe-wsroute)`.
- Then niri: a fixture-declared `open_on_workspace` rule resolves the target monitor and named workspace at initial configure and maps D there as a new column after the fixture-declared active C; under the shipped `Smart` activation the inactive workspace is not activated (no switch) and visible focus stays A while D activates within the target workspace. `S(S-nir-ins)` + `S(S-nir-wsopen)`.
- Then PaperWM: a fixture-declared winprop `spaceIndex` moves D to WS2 and re-inserts it there via the existing-window path, landing after the fixture-declared selected C under the shipped RIGHT default; inactive-space inserts only ensure the viewport without stealing focus or switching, so source focus stays A. `S(S-pap-ins)`.
- Then karousel/Lazy: cross-output leg fixture-inapplicable under the single-screen scope (`S(S-kar-single)`); workspace leg has no plugin launch-rule route (window rules cover tile/float/caption only), while a native single-desktop D tiles on the WS2 grid after the last-focused column (C's column under the WS2 prior focus) per the ordinary admission path; target focus, no-steal, and switch TBD (fixture states no protocol, X11 user-time/startup/session, Wayland token/app-id, or FSP/rules inputs selecting the host activation fork; on acceptance the pinned `activationDesktopPolicy` `SwitchToOtherDesktop` default switches to the target desktop per `S(S-kwin-act)`). `S(S-kar-single)` + `S(S-kar-ins)` + `S(S-kar-wsroute)` + `S(S-kwin-manage)` + `S(S-kwin-add)` + `S(S-kwin-act)`.
- Then paneru: no-counterpart - fresh spawn admits only into the active strip at the config `insertion()` index or after focus, with no workspace-targeted launch rule in the inspected spawn/config inventory (remembered-strip re-insert is a re-manage path, not fresh routing); anchor/focus/switch have no fresh-routing leg. `S(S-pan-ins)` + `S(S-pan-spawn)`.
- Then Ours KDE: TBD.
- Then Ours Windows: TBD.
- Variant hook: provisional/TBD (contrast rule-targeted routing with
  R-OUT-05 pointer/focused-output routing when that row lands).

### R-INS-08: admission under an explicit preselected split direction

- Given (tree profiles): `H[A,B*]`, B landscape, no existing
  preselection. Record actual admission focus/anchor.
- Given (column profiles): applicability TBD per profile (explicit
  direction vs column insertion-index models); shipped defaults apply.
- When: preselect a vertical split at B; open C, then D without refocus.
  Native verbs per profile: Hyprland `layoutmsg preselect <direction>`
  (`S(S-hyp-pre)`); bspwm `node -p DIR` manual insertion mode
  (`S(S-bsp-pre)`); i3 `split vertical` (`S(S-i3-split)`);
  sway `splitv`/`split v` (`S(S-sway-default)` + `S(S-sway-split)`);
  COSMIC/xmonad/qtile/awesome/scrolling profiles/Ours: preselect-verb
  inventory TBD at pin (a missing verb is not a no-op; a layout
  approximation is not an exact fixture). Keep this distinct from an
  automatic chain.
- Observe: explicit direction overrides auto axis vs unsupported;
  consumed one-shot vs persistent direction on the next admission.
- Then COSMIC: no-counterpart (no split-direction preselect verb in the
  inspected inventory; `ToggleOrientation`/`Orientation` flip the
  existing parent axis only, and the pointer/drop-zone path is not this
  leg). `S(S-cos-pre)`.
- Then Hyprland/Dwindle: preselect forces the next admission axis and newcomer side, then resets under the shipped default. `S(S-hyp-pre)` + `S(S-hyp-defaults)`.
- Then bspwm: `-p DIR` preselects the splitting area with `~` cancel (manual insertion mode); the preselect overrides the automatic axis at B (WEST/EAST vertical, NORTH/SOUTH horizontal, newcomer on the preselected side at the stored ratio) and is consumed one-shot via cancel after use, so D admits on the automatic branch. `S(S-bsp-pre)`.
- Then i3: `split vertical` wraps B in a persistent V parent via `tree_split` (`H[A,V[B]]`, B's percent carried); C admits after B in that V (override of the shipped splith default) with newcomer focus, D admits after C in the same V with newcomer focus (persistent, not one-shot). `S(S-i3-split)` + `S(S-i3-ins)`.
- Then xmonad/Tall+Navigation2D: no-counterpart - fixed master/stack algorithm has no split-direction verb. `S(S-xmo-layout)` + `S(S-xmo-core-nav)`.
- Then sway: `splitv` dispatches through `do_split` wrapping B in a persistent V parent; C admits after B in that V (override) with newcomer focus, D admits after C in the same V with newcomer focus (persistent, not one-shot). `S(S-sway-default)` + `S(S-sway-split)` + `S(S-sway-ins)`.
- Then qtile/Columns: no-counterpart (insertion-index model per `S(S-qti-add)`; the Columns command inventory - directional focus per `S(S-qti-focus)`, shuffle carry per `S(S-qti-shuffle)`, `toggle_split` display flip per `S(S-qti-split)` - lists no split-direction preselect verb, so neither leg has a preselect path; ordinary admission would follow `S(S-qti-add)` only). `S(S-qti-add)` + `S(S-qti-focus)` + `S(S-qti-shuffle)` + `S(S-qti-split)`.
- Then awesome/tile: no-counterpart - fixed master/stack tile algorithm (`tile`/`tile.left`/`tile.top`/`tile.bottom` via `do_tile`, per-tag `layout.get`/`set` only) has no split-direction preselect verb in the shipped layout/tag/key inventories (shipped keys bind index focus/swap only); neither leg has a preselect path and no override/consumption leg runs; ordinary admission would follow `S(S-awe-tile)` only. `S(S-awe-tile)` + `S(S-awe-tileleft)` + `S(S-awe-layout)` + `S(S-awe-keys)`.
- Then niri: no-counterpart for the split-direction preselect
  (insertion-index model: ordinary admission always opens a new column
  after the active column per `S(S-nir-ins)`; no split-direction preselect
  verb in the full `Action` inventory, so neither leg has a preselect
  path). `S(S-nir-ins)` + `S(S-nir-acts)`.
- Then PaperWM: no-counterpart for the split-direction preselect (open-position index model, no direction-preselect verb in the registered inventory); C then D insert at selected+1 each under the shipped RIGHT default with ordinary activation. `S(S-pap-ins)` + `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (column insertion-index model; the Actions/definition inventory lists no split-direction preselect verb, so neither leg has a preselect path; ordinary admission would follow `S(S-kar-ins)` only). `S(S-kar-acts)`.
- Then paneru: no-counterpart - the `Operation` inventory lists no split-direction preselect verb, so neither leg has a preselect path; ordinary admission would follow the fresh strip index policy. `S(S-pan-cmds)` + `S(S-pan-ins)` + `S(S-pan-fresh)`.
- Then Ours KDE: TBD (no preselection concept evidenced at `9241c94`; not claimed absent).
- Then Ours Windows: TBD, same as Ours KDE.
- Variant hook: V-INS-AXIS.

<a id="r-ins-09-burst-of-maps-versus-roster-delivered-together"></a>
### R-INS-09: burst of maps versus roster delivered together

- Given (tree profiles): empty workspace plus roster C,D,E (ordinary
  resizable, no rules, scale 1). Two independently reset legs: (a) burst leg:
  three separate map events in order C,D,E with no refocus between; (b)
  together leg: the same roster delivered in one observation/batch where the
  host permits (where no batch verb is evidenced, only the burst leg runs).
- Given (column profiles): empty strip plus roster C,D,E at shipped defaults;
  viewport recorded. Same two legs with the same batch-verb qualifier.
- When: deliver leg (a) as three maps; deliver leg (b) as one batch.
  Native verbs/config: same ordinary-open verbs as R-INS-03 per profile.
- Observe: admission order/anchors (event order versus collective
  optimization); per-map recalculation versus single layout; native
  event-order settlement. First-leg policy reuses R-INS-01/R-INS-04; the
  enable/adoption path (frozen MRU) is R-START-01..03 only, never ordinary maps.
- Then COSMIC: each ordinary map runs one `map_to_tree` plus one
  `update_positions` plus one push per `map_internal` call, serially; each
  leg anchors at the focus MRU with newcomer focus; the `set_tiling` serial
  loop over `floating_layer.mapped()` holding the frozen focus stack is the
  separate enable path (reuse/qualify R-START-01..03, not this row); no batch
  admission verb in the traced path; exact frames TBD.
  `S(S-cos-geometry)` + `S(S-cos-seq)` + `S(S-cos-last)` + `S(S-cos-axis)` +
  `S(S-cos-newgroup)` + `S(S-cos-mapfocus)`
- Then Hyprland/Dwindle: each map runs `addTarget` plus recalculation
  serially; each leg anchors per the ordinary active-tiled/closest rule with
  newcomer focus; no batch admission verb in the traced path; per-leg sides
  TBD (pointer half). `S(S-hyp-ins)` + `S(S-hyp-newfocus)` + `S(S-hyp-defaults)`
- Then bspwm: TBD (burst ancestry unresearched; no claim)
- Then i3: each manage admits serially after the focus-stack tiling head with
  newcomer focus; cold-start `manage_existing_windows` iterates X query-tree
  children serially, so that order follows the server listing, distinct from
  a user burst order; exact frames TBD. `S(S-i3-ins)` + `S(S-i3-geometry)`;
  queued
- Then xmonad/Tall+Navigation2D: TBD (burst ancestry unresearched; no claim).
  `S(S-xmo-layout)`
- Then sway: each map admits serially after the focus-inactive newcomer with
  ordinary newcomer focus; no batch verb in the cited path; exact frames TBD.
  `S(S-sway-ins)`; queued
- Then qtile/Columns: TBD (burst ancestry unresearched; no claim)
- Then awesome/tile: TBD (burst ancestry unresearched; no claim)
- Then niri: TBD (burst ancestry unresearched; no claim)
- Then PaperWM: TBD (burst ancestry unresearched; no claim)
- Then karousel/Lazy: TBD (burst ancestry unresearched; no claim)
- Then paneru: TBD (burst ancestry unresearched; no claim)
- Then Ours KDE: each newcomer admits via one `Admit` propose through the
  retained lifecycle with the projection gate (`MalformedTopology` refusal on
  invalid/degenerate geometry, no mutation); rebuild/seed order is the
  deterministic spatial `(y,x,h,w)` sort with focus last (stable on ties),
  not raw native event order - adapter observations can coalesce, so
  arrival-order claims stay TBD; `seed_admit_step` advances one revision per
  member. Fresh whole-roster admission first attempts the geometry-derived
  recursive-cut fit; clean/tolerance-valid minimum-feasible fits preserve
  placement, otherwise the deterministic long-edge seed runs (R-START-01..03).
  This is not a balanced batch optimizer.
  `S(S-ours-admission-capacity)` + `S(S-ours-ins)` @15faf1e4; native
  event-order/timing TBD, no universal cap
- Then Ours Windows: same one-Admit-per-newcomer/projection-gate/deterministic
  seed-order leg as Ours KDE via the shared Engine.
  `S(S-ours-admission-capacity)` + `S(S-ours-ins)` @15faf1e4; native
  event-order/timing TBD, no universal cap
- Variant hook: none (OPEN; user decision required for any burst-ordering
  policy, nothing selected).
