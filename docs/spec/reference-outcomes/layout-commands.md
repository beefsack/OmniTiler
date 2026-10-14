# Layout commands (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

Tile/float workspace-mode coverage stays in R-FLT-04 (floating area) and R-WS-06 (workspaces area) and is reused here, not duplicated: proposed layout selection is not that toggle. R-FLT-04 and R-WS-06 each carry scrolling assessments within GWT scenarios in their own files.

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Never manufacture nested tree equivalents for columns.
Unchanged Ours cells cite the Engine + adapter op inventory at `a77dd34`
(`S(S-ours-planops)`); R-LAY-01/05/06 KDE cells cite the later offline
implementation record. Selected intent and doc assertions alone are never evidence.

### R-LAY-01: toggle parent split orientation

- Given (tree profiles): `H[A,B*]`, parent selected where necessary.
  Ordinary windows, no rules, scale 1, zero gaps for reference geometry.
- Given (column profiles): none exists. Split-orientation toggle has no
  column fixture; scrolling Thens below are no-counterpart
  qualifications, never H/V ancestry claims.
- Given (layout-driven projections): xmonad/Tall uses order `[A,B*]`
  (A master, B stack); qtile/Columns uses two single-window columns
  `C1[A],C2[B*]`; awesome/tile uses order `[A,B*]` (A master, B stack).
  These establish the start; none supplies an axis-toggle verb.
- When: toggle parent split orientation. Native verbs/config per profile:
  COSMIC `ToggleOrientation`; Hyprland `layoutmsg togglesplit`; bspwm
  `node @parent -y` (cycle or explicit type); i3 `layout toggle split`;
  sway `layout toggle split`; Ours KDE `toggle-orientation` (Meta+O),
  Ours Windows `toggle-orientation` (Win+O). Other profiles
  per inventory below.
- Observe: same children on the new axis vs wrapping a leaf/new group;
  scope of the layout command. Focus retention is recorded where the
  path writes no focus.
- Then COSMIC: same children on the new axis (`V[A,B*]`); sizes rescaled
  proportionally, order kept, focus B. `S(S-cos-orient)`.
- Then Hyprland/Dwindle: no observable change at shipped defaults:
  `togglesplit` flips the bit, but immediate geometry-based recalculation
  restores the H axis; order/focus stay A,B*. `S(S-hyp-lay)` + `S(S-hyp-defaults)`.
- Then bspwm: parent split type flips via `set_type`; same children and
  order, focus unchanged (no focus write on that path). `S(S-bsp-type)`.
- Then i3: same children on the new axis (`V[A,B*]`); `layout toggle split`
  retargets the H parent and flips it to V, order kept, focus B.
  `S(S-i3-layout)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (fixed master/stack
  algorithm has no split-direction verb). `S(S-xmo-layout)`.
- Then sway: same children on the new axis (`V[A,B*]`); `layout toggle
  split` retargets the H parent and flips it to V, order kept, focus B.
  `S(S-sway-layout)`.
- Then qtile/Columns: no-counterpart (`toggle_split` flips split/stacked
  column mode, not an H/V axis). `S(S-qti-split)`.
- Then awesome/tile: no-counterpart (fixed master/stack tile geometry;
  only layout rotation via `inc`, which belongs to R-LAY-04).
  `S(S-awe-tile)` + `S(S-awe-keys)`.
- Then niri: no-counterpart (ordered columns have no split axis; the
  Action inventory lists no orientation verb). `S(S-nir-acts)`.
- Then PaperWM: no-counterpart (column model; the registered action
  inventory lists no orientation verb). `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (Grid/Column model; the Actions
  inventory lists no orientation verb). `S(S-kar-acts)`.
- Then paneru: no-counterpart (the `Operation` inventory lists no
  orientation verb). `S(S-pan-cmds)`.
- Then Ours KDE: `V[A,B*]`, immediate root parent flipped, order/shares/B
  focus preserved; second invocation restores H. Shared Session/Engine/protocol
  and KDE Meta+O/native catalog/presets implemented offline, native journey
  pending; [tests and record](../../changes/archive/parent-orientation-toggle.md).
- Then Ours Windows: Win+O immediate-parent route delivered 2026-10-11,
  base `755aab8` + delivery commit ([record](../../changes/archive/windows-parent-orientation-toggle.md)).
  Real retained Engine H/V root flips preserve order/shares/focus and double
  toggle restores tree; native gates and agent-observed Settings/preset/live
  routing adoption passed. Positive physical toggle/OS suppression pending;
  Authentic owns Win+O, Compatible disables ours. Held-repeat choice approved User 2026-10-14 as delivered ([decision](../../decisions.md#move-layout-and-output-commands)).
- Variant hook: provisional/TBD (orientation-scope hook, to discuss).

### R-LAY-02: rotate 90 degrees; mirror left/right (separate fresh legs)

- Given (tree profiles): `H[A,V[B*,C]]`. Ordinary windows, no rules,
  scale 1, zero gaps for reference geometry. Two legs from fresh
  fixtures with a reset between: leg 1 rotate 90 degrees, leg 2 mirror
  left/right. Rotate and mirror are never combined into one journey.
- Given (column profiles): none exists. A nested H/V tree has no faithful
  column fixture, so there is no column Given; scrolling Thens below are
  fixture-inapplicable qualifications. Consume/expel mechanics belong to
  R-COL-03.
- Given (layout-driven projections): none exists. Flat master/stack or
  tiled-client order has no nested counterpart; qtile/awesome Thens
  below are fixture-inapplicable qualifications, never manufactured
  nesting.
- When: leg 1 the profile's native 90-degree rotate verb; leg 2 the
  profile's native left/right mirror verb, each named below. Node-targeted
  verbs select the fixture root. A missing verb is not a no-op; layout
  cycling is never substituted. Hyprland's verbs dispatch on the focused
  node's parent only, never the whole tree.
- Observe: axis/order/geometry transform, focus preservation or missing
  transform. Rotate and mirror report separately.
- Then COSMIC: no-counterpart on both legs (no rotate/mirror verb in the
  keybinding inventory; `ToggleOrientation` flips only the focused
  parent's axis, which belongs to R-LAY-01). `S(S-cos-orient)`.
- Then Hyprland/Dwindle: no-counterpart for the requested whole-fixture
  transform (`rotatesplit`/`swapsplit` dispatch on the focused node's
  parent only). Nearest native scope: both named commands swap the V
  parent's children to `V[C,B*]` at shipped defaults (recalculation restores
  V after rotation); focus B stays. `S(S-hyp-lay)` + `S(S-hyp-defaults)`.
- Then bspwm: leg 1 `-R 90` on the root flips the split type at every
  level with the 90-degree conditional swap, yielding `V[H[B,C],A]`,
  focus B; leg 2 `-F horizontal` swaps children where the split matches,
  yielding `H[V[B,C],A]`, focus B (no focus write on either path).
  `S(S-bsp-rot)`.
- Then i3: no-counterpart on both legs (the command inventory lists no
  rotate/mirror verb). `S(S-i3-cmds)`.
- Then xmonad/Tall+Navigation2D: fixture-inapplicable (flat Tall has no
  nested H/V levels). `S(S-xmo-layout)`.
- Then sway: no-counterpart on both legs (the runtime command table
  lists no rotate/mirror verb). `S(S-sway-cmds)`.
- Then qtile/Columns: fixture-inapplicable (no nested H counterpart;
  shuffle verbs are column-level or in-column only).
  `S(S-qti-shuffle)` + `S(S-qti-add)`.
- Then awesome/tile: fixture-inapplicable (flat tiled-client order has no
  nesting levels). `S(S-awe-tile)`.
- Then niri: fixture-inapplicable (ordered columns have no nested H
  ancestor). `S(S-nir-move)`.
- Then PaperWM: fixture-inapplicable (column/row membership has no
  nested H ancestor). `S(S-pap-move)`.
- Then karousel/Lazy: fixture-inapplicable (Grid/Column membership has
  no nested H ancestor). `S(S-kar-move)`.
- Then paneru: fixture-inapplicable (strip/column model has no nested H
  ancestor). `S(S-pan-model)`.
- Then Ours KDE: no-counterpart on both legs (no rotate/mirror verb in
  the Engine + adapter op inventory). `S(S-ours-planops)`.
- Then Ours Windows: same missing-verb legs as Ours KDE.
  `S(S-ours-planops)`.
- Variant hook: provisional/TBD (rotate/mirror hook, to discuss).

### R-LAY-03: promote B to master

- Given (model-qualified three-window layout A,B*,C): A is master where
  the profile has a master concept; other profiles use their native
  three-window arrangement (ordinary Dwindle tree, bspwm desktop, i3/sway
  H parent, single COSMIC group, two-column qtile projection, tile order
  for awesome, three single-window columns at shipped defaults for
  scrolling profiles). B focused in every Given.
- When: promote B to master. Native verbs per profile: xmonad
  `swapMaster`; awesome `setmaster`; Hyprland `layoutmsg movetoroot`
  (tree operation, not master identity). Other profiles per inventory
  below; focus-only stepping is never substituted.
- Observe: master identity change/reorder vs focus-only, tree operation
  or no master concept.
- Then COSMIC: no-counterpart (no master concept; the tiling model holds
  Group/Mapped nodes only). `S(S-cos-model)`.
- Then Hyprland/Dwindle: no master concept in Dwindle; `movetoroot` is the
  native tree operation, swapping B toward the root (returns false with no
  mutation when already at the root). `S(S-hyp-lay)`.
- Then bspwm: no-counterpart (desktop layout is tiled/monocle only; no
  master). `S(S-bsp-layout)`.
- Then i3: no-counterpart (split/tabbed/stacked container layouts; no
  master). `S(S-i3-layout)`.
- Then xmonad/Tall+Navigation2D: B becomes master via `swapMaster`
  (old master swapped into tiling order); focus stays B.
  `S(S-xmo-master)`.
- Then sway: no-counterpart (same container layouts as i3; no master).
  `S(S-sway-layout)`.
- Then qtile/Columns: no-counterpart (split/stacked column modes only;
  no master in Columns). `S(S-qti-split)`.
- Then awesome/tile: B moves to the primary section via `setmaster`
  (repeated swaps, no focus write); focus stays B. `S(S-awe-master)` +
  `S(S-awe-swap)`.
- Then niri: no-counterpart (the Action inventory lists no master
  verb). `S(S-nir-acts)`.
- Then PaperWM: no-counterpart (the registered action inventory lists
  no master verb). `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (the Actions inventory lists no
  master verb). `S(S-kar-acts)`.
- Then paneru: no-counterpart (the `Operation` inventory lists no master
  verb). `S(S-pan-cmds)`.
- Then Ours KDE: no-counterpart (no master verb or state in the Engine
  + adapter op inventory). `S(S-ours-planops)`.
- Then Ours Windows: same missing-verb leg as Ours KDE.
  `S(S-ours-planops)`.
- Variant hook: provisional/TBD (master-promote hook, to discuss).

### R-LAY-04: select a native alternative layout on WS2; return to WS1

- Given (all profiles): WS1 and WS2 each hold an A/B-style two-window
  tiled layout under the profile's native layout L1 (named per profile
  below). Ordinary windows, no rules, shipped defaults unless the L1
  choice itself needs naming.
- When: select the profile's native alternative layout L2 on WS2 only;
  then return to WS1. Native verbs/mechanisms: bspwm `desktop -l monocle`;
  i3/sway `layout tabbed`; xmonad `NextLayout`; qtile `next_layout`;
  awesome `layout.set`; Hyprland workspace rule carrying a layout
  override. Other profiles per inventory below. Tile/floating toggles are
  never substituted (they stay R-FLT-04); L1/L2 are real named alternatives
  with native ownership, not tile/float.
- Observe: per-workspace vs global layout selection and preserved window
  order.
- Then COSMIC: no-counterpart (`WorkspaceLayout` Vertical/Horizontal is
  a single global workspace-navigation arrangement, not a tiled algorithm
  choice; `TileBehavior` Global/PerWorkspace scopes tiling enable, not
  algorithm selection; no runtime per-workspace layout-select verb).
  `S(S-cos-wslay)` + `S(S-cos-ctl-tile)`.
- Then Hyprland/Dwindle: L1 dwindle (global default), L2 master (a
  registered tiled algorithm) via a workspace rule carrying the layout
  override; the rule selects WS2's tiled algorithm per workspace while WS1
  keeps dwindle. Order preserved through the switch (existing tiled
  targets re-admit in order and append under shipped master defaults).
  `S(S-hyp-layout)`.
- Then bspwm: L1 tiled, L2 monocle; `desktop -l` is per-desktop, so WS2
  shows monocle while WS1 stays tiled with A/B order preserved.
  `S(S-bsp-desklay)` + `S(S-bsp-layout)`.
- Then i3: L1 split-H, L2 tabbed; `layout tabbed` retargets WS2's H parent
  (same children, order kept, B the active tab) while WS1 keeps split-H.
  `S(S-i3-layout)`.
- Then xmonad/Tall+Navigation2D: L1 Tall, L2 Mirror Tall; each workspace
  carries its own layout, so `NextLayout` on WS2 leaves WS1 on Tall
  with stack order preserved. `S(S-xmo-wslay)`.
- Then sway: L1 H, L2 tabbed; `layout tabbed` retargets WS2's H parent
  (same children, order kept, B the active tab) while WS1 keeps H.
  `S(S-sway-layout)`.
- Then qtile/Columns: L1 Columns, L2 Max; each group keeps its own
  layouts list and current index, so `next_layout` naming WS2's group
  leaves WS1 on Columns. `S(S-qti-wslay)`.
- Then awesome/tile: L1 tile, L2 tile.left; `layout.set` is per-tag, so
  WS2 takes tile.left while WS1 stays tile with order preserved.
  `S(S-awe-tileleft)` + `S(S-awe-layout)`.
- Then niri: no-counterpart (single scrolling model; the Action
  inventory lists no layout-select verb). `S(S-nir-acts)`.
- Then PaperWM: no-counterpart (no layout alternatives or select verb
  in the registered action inventory). `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (desktops share the Grid model; the
  Actions inventory lists no layout-select verb). `S(S-kar-acts)`.
- Then paneru: no-counterpart (the `Operation` inventory lists no layout
  verb; `Virtual*` switches strips, not layouts). `S(S-pan-cmds)`.
- Then Ours KDE: no-counterpart (no layout-select verb in the Engine +
  adapter op inventory; single Engine algorithm). `S(S-ours-planops)`.
- Then Ours Windows: same missing-verb leg as Ours KDE.
  `S(S-ours-planops)`.
- Variant hook: PARKED / OPEN (User decision 2026-10-09): workspace-local
  layout selection waits for a genuine second layout; revisit with tabbed-stack
  design after 0.1. Whether tabs count as L2 remains a future decision.

## Selected addition (USER 2026-10-07; shared core/KDE implemented offline)

[Item 4](../../decisions.md#move-layout-and-output-commands) selects R-LAY-01:
Meta+O / Win+O immediate-parent toggle including root, order/shares/focus
preserved; sole root leaf no-op and no saved admission hint. KDE offline
evidence is linked below; Windows adapter delivered 2026-10-11
([record](../../changes/archive/windows-parent-orientation-toggle.md));
both physical native toggle journeys remain pending.
Reference WM pins and outcomes are unchanged.

### R-LAY-05: immediate parent toggled twice in a nested tree

- Given (tree leg): `H[A,V[B*,C]]`, ordinary tiles, record child shares;
  root H and B's immediate parent V are distinct. No minimum constraints.
- Given (other models): exact nested fixture/axis-toggle applicability TBD;
  no manufactured tree counterpart for column models.
- When: toggle parent split axis twice, observing after each invocation.
- Observe: immediate parent vs root scope; order/shares/focus preserved,
  second toggle returns the original tree.
- Then COSMIC: first toggle flips B's immediate parent V to H, yielding
  `H[A,H[B*,C]]`; same children and order, shares proportionally
  rescaled, focus B (no focus write on that path); second toggle flips H
  back to V, restoring exact `H[A,V[B*,C]]`; root H unchanged.
  `S(S-cos-orient)`.
- Then Hyprland/Dwindle: `togglesplit` dispatches on the focused node and
  flips the immediate parent `splitTop` with no focus write, so scope,
  order and focus are established (immediate parent only, order kept,
  focus B); the following recalculation re-derives the axis from parent
  geometry at shipped defaults, so with no fixture rectangles the axis
  after each invocation stays TBD (underspecified fixture). Second
  invocation flips the bit again under the same recalculation.
  `S(S-hyp-lay)` + `S(S-hyp-defaults)`.
- Then bspwm: first toggle via `node @parent -y` flips the immediate
  parent split type via `set_type` (same children and order, split ratio
  retained, focus unchanged - no focus write on that path); root
  unaffected; second toggle flips the type back, restoring the original
  tree. `S(S-bsp-type)`.
- Then i3: first toggle via `layout toggle split` retargets B's immediate
  parent V and flips it to H (same children, order kept, focus B -
  neither path writes focus); second toggle flips H back to V, restoring
  the original tree. `S(S-i3-layout)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (fixed master/stack
  algorithm has no split-direction verb; flat Tall has no nested H/V
  levels to toggle). `S(S-xmo-layout)`.
- Then sway: first toggle via `layout toggle split` flips B's immediate
  parent V to H (same children, order kept, focus B - no focus write;
  two-child parent so no single-child flatten); second toggle flips back,
  restoring the original tree. `S(S-sway-layout)`.
- Then qtile/Columns: no-counterpart (`toggle_split` flips split/stacked
  column mode, not an H/V axis); no manufactured nested-tree counterpart.
  `S(S-qti-split)`.
- Then awesome/tile: no-counterpart (fixed master/stack tile geometry;
  only layout rotation via `inc`, which belongs to R-LAY-04).
  `S(S-awe-tile)` + `S(S-awe-keys)`.
- Then niri: no-counterpart (ordered columns have no split axis; the
  full Action inventory lists no orientation verb). `S(S-nir-acts)`.
- Then PaperWM: no-counterpart (column model; the registered action
  inventory lists no orientation verb). `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (Grid/Column model; the Actions
  inventory lists no orientation verb). `S(S-kar-acts)`.
- Then paneru: no-counterpart (the `Operation` inventory lists no
  orientation verb). `S(S-pan-cmds)`.
- Then Ours KDE: offline tested `H[A,H[B*,C]]` then exact original
  `H[A,V[B*,C]]`, including unequal child shares, order and B focus;
  root axis unchanged. Meta+O immediate-parent route implemented, native
  journey pending; [record](../../changes/archive/parent-orientation-toggle.md), item 4.2.
- Then Ours Windows: scope/roundtrip delivered 2026-10-11, base `755aab8` +
  delivery commit ([record](../../changes/archive/windows-parent-orientation-toggle.md)).
  Retained Engine nested parent-only and unequal-share exact double roundtrip
  tests pass with focus retained; physical native geometry/focus pending; item 4.2.
- Variant hook: provisional/TBD (R-LAY-01 orientation scope).

### R-LAY-06: sole-leaf toggle then ordinary admission

- Given: sole root A* on a wide 1920x1080 work area, no rules, no minimum
  constraints or manual preselection; record root-leaf state before toggle.
- When: toggle parent axis once; ordinarily admit B with A as target.
- Observe: sole-leaf no-op vs saved orientation affecting future admission;
  long-edge admission axis.
- Then COSMIC: toggle is a no-op (sole Mapped root has no parent group,
  so `update_orientation` returns without pushing a tree; focus
  unchanged); subsequent wide-area admission splits A's long edge per
  the long-edge rule, yielding side-by-side `H[A,B*]` with newcomer B
  focused. `S(S-cos-orient)` + `S(S-cos-axis)` + `S(S-cos-mapfocus)`.
- Then Hyprland/Dwindle: toggle is a no-op (`toggleSplit` returns false
  with no mutation when the node has no parent); subsequent wide-area
  admission splits side-by-side per the long-edge rule with the newcomer
  ordered by pointer half at `force_split=0` (no pointer fixture, so
  exact A/B order stays TBD - true client TBD); newcomer B takes focus.
  `S(S-hyp-lay)` + `S(S-hyp-ins)` + `S(S-hyp-newfocus)`.
- Then bspwm: sole-leaf type flip writes no focus and has no visible
  topology effect (a leaf has no children); it saves no admission hint
  because ordinary insertion builds the new parent axis from the anchor
  rectangle's longest side, ignoring the leaf's stored type; the wide
  1920x1080 anchor (w>h) yields a vertical (side-by-side) parent with A
  first and newcomer B second under `second_child` polarity, focus B.
  `S(S-bsp-type)` + `S(S-bsp-insert)`.
- Then i3: sole-leaf toggle is not a visual no-op structurally - it
  retargets the workspace parent and wraps A into a new split container
  carrying the flipped layout (wide-output default SPLITH flips to
  SPLITV), focus stays A; ordinary admission then attaches B after the
  focused descendant in that SPLITV parent, yielding `V[A,B*]` with B
  focused (saved orientation affects admission, unlike Ours).
  `S(S-i3-layout)` + `S(S-i3-solesave)` + `S(S-i3-ins)`.
- Then xmonad/Tall+Navigation2D: no-counterpart for the toggle (no
  split-direction verb); ordinary admission tiles B via `insertUp` above
  focus with the newcomer brought into focus (master/stack terms, not an
  H/V axis). `S(S-xmo-layout)` + `S(S-xmo-ins)` + `S(S-xmo-admit)`.
- Then sway: sole-leaf toggle operates on the parent like i3 and flips
  the H/V layout (wide-output default H flips to V), focus stays A;
  ordinary admission inserts B after the focused tiling sibling with
  newcomer focus, yielding stacked `V[A,B*]` (saved orientation affects
  admission, unlike Ours). `S(S-sway-layout)` + `S(S-sway-wsdefault)` +
  `S(S-sway-ins)`.
- Then qtile/Columns: no-counterpart for the toggle (`toggle_split` is
  column mode, not an H/V axis); ordinary admission adds B to a
  new/focused-position column with the newcomer focused when stealable
  (ordinary windows). `S(S-qti-split)` + `S(S-qti-add)`.
- Then awesome/tile: no-counterpart for the toggle (fixed tile geometry;
  rotation only via `inc`); ordinary admission appends B at the end of
  tile order (`[A,B]`, B last) under the shipped manage focus-filter.
  `S(S-awe-tile)` + `S(S-awe-keys)` + `S(S-awe-manage)`.
- Then niri: no-counterpart for the toggle (no orientation verb in the
  full Action inventory; nothing saved, later admission unaffected);
  ordinary admission wraps B in a new column after the active A at the
  default width with no rescale, activating under Smart (no pending
  fullscreen) with focus to B and the view animating minimal-fit under
  shipped `Never`. `S(S-nir-acts)` + `S(S-nir-ins)` + `S(S-nir-base)` +
  `S(S-nir-view)`.
- Then PaperWM: no-counterpart for the toggle (no orientation verb in
  the registered inventory); ordinary admission inserts B RIGHT of A
  (selected+1 under the shipped RIGHT default) with the newcomer
  activating on the active space; A keeps its width per the column layout
  and the viewport keeps B visible.
  `S(S-pap-acts)` + `S(S-pap-ins)` + `S(S-pap-layout)` + `S(S-pap-view)`.
- Then karousel/Lazy: no-counterpart for the toggle (no orientation verb
  in the Actions inventory); ordinary admission opens a new column after
  the last-focused column with end-insert, newcomer width from its preferred width clamped into [min,max], existing widths stable; newcomer focus TBD (fixture states no protocol, X11 user-time/startup/session, Wayland token/app-id/transient-serial, or FSP/rules inputs selecting the host activation fork); viewport is a deterministic conditional (Lazy minimal scroll toward the last-focused column); exact settled widths TBD (fixture states no newcomer frame/min/max inputs for the clamp). `S(S-kar-acts)` + `S(S-kar-ins)` + `S(S-kar-fltanchor)` + `S(S-kar-scroll)` + `S(S-kar-min)` + `S(S-kwin-manage)` + `S(S-kwin-add)`.
- Then paneru: no-counterpart for the toggle (no orientation verb in the
  `Operation` inventory; nothing written, no saved hint); ordinary admission
  takes the fresh path with B appended after A as a new Single column and
  focus synthesized to B. `S(S-pan-cmds)` + `S(S-pan-fresh)` + `S(S-pan-base)`.
- Then Ours KDE: offline tested toggle no-op, no pending plan or saved hint;
  subsequent wide-area admission uses unchanged horizontal long-edge rule.
  Core wide fixture gives `H[A,B*]` (newcomer desired focus); exact native
  admission order/focus on the 1920x1080 journey remains TBD.
  [record](../../changes/archive/parent-orientation-toggle.md), item 4.2.
- Then Ours Windows: no-hint/long-edge route delivered 2026-10-11, base
  `755aab8` + delivery commit ([record](../../changes/archive/windows-parent-orientation-toggle.md)).
  Retained Engine lone no-op leaves snapshot unchanged and subsequent wide
  admission uses long edge; exact native newcomer order/focus TBD; item 4.2.
- Variant hook: provisional/TBD (R-LAY-01 sole-leaf admission hint).
