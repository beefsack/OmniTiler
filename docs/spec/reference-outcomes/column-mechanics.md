# Scrollable-column mechanics (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. No existing column rows to backfill (0 existing rows, 0 cells). Shared lifecycle actions reuse model-qualified existing/new IDs; no second column lifecycle inventory. Approved candidates and final queue are recorded in [reference-matrix-expansion.md](../../changes/archive/reference-matrix-expansion.md).

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours cells cite actual Engine + adapter source via the
established `S(S-ours-*)` model keys; selected intent is never evidence.
A missing strip/column verb is `no-counterpart`, an unbuildable column
start is `fixture-inapplicable`; neither votes as an agreeing no-op.
Shipped scrolling baselines: niri half-width columns, 1/3-1/2-2/3
presets, centering never (`S(S-nir-base)`); PaperWM RIGHT insertion,
DEFAULT focus (`S(S-pap-base)`); karousel 50%/100% widths, Lazy
scrolling, Centered/Grouped off, no default stack (`S(S-kar-base)`);
paneru shipped defaults, width presets, resize-cycle on, auto-center
off, append admission (`S(S-pan-base)`).

- Tier 1 release recheck 2026-11-14: the cells below distinguish the native
  default tiled model from optional algorithms. COSMIC `epoch-1.11.0` uses
  an output-bound Group/Mapped tree (`src/shell/layout/tiling/mod.rs:133-168`);
  i3 `4.25.1` uses container/workspace attachment (`src/tree.c:149-174`);
  sway `1.12` uses workspace tiling siblings (`sway/tree/view.c:898-902`).
  Hyprland `v0.57.2` defaults to Dwindle, but also registers optional master,
  scrolling and monocle (`src/config/values/ConfigValues.cpp:180`,
  `src/layout/supplementary/WorkspaceAlgoMatcher.cpp:22-35`).
- Binding convention for these ten rows: no default binding for the requested
  column/viewport/virtual-row/app-native-tab operation in those four default
  tiled models (`cosmic-comp epoch-1.11.0 data/keybindings.ron`,
  `Hyprland v0.57.2 example/hyprland.lua:265-316`, `i3 4.25.1 etc/config`,
  `sway 1.12 config.in`). Admission/tab creation is an event, not a shortcut.
  Related ordinary tree bindings are named where useful, never counted as
  column counterparts. Optional Hyprland scrolling commands below are also
  unbound in its shipped example. Exact unsupported/native journeys stay TBD.
- REQ-COL-01 through REQ-COL-10 are NORMATIVE only as scope deferrals beyond
  0.1, decided under the standing Tier 1 alignment rule (User 2026-11-13).
  OmniTiler has no strip/viewport model; prior triage places all ten after 0.1.
  No future mechanics or runtime behavior is selected. Tree tabbed stacks
  remain the separate already-selected post-0.1 design, not a column promise.

### R-COL-01: open a window over three half-width columns

- Given (column profiles): `COL[C1[A],C2[B*],C3[C]]`, each 0.5W;
  `VP(x=0,W=2400)` shows C1/C2. Ordinary windows, scale 1, shipped
  defaults apply.
- When: open D.
- Observe: new column before/after focus or append vs same-column
  admission; existing widths stable vs rescaled; new focus viewport.
- Then COSMIC (`epoch-1.11.0`): fixture-inapplicable: no strip/column
  model; ordinary admission splits the focused leaf's long edge
  (`src/shell/layout/tiling/mod.rs:548-616`); automatic, no binding.
- Then Hyprland/Dwindle (`v0.57.2`): fixture-inapplicable under the Dwindle
  profile: admission splits the anchor with pointer-ordered newcomer,
  no strip columns (`src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:63-95`).
  Optional scrolling inserts a new column after the anchor column and fits
  the viewport, rather than rescaling existing columns
  (`src/layout/algorithm/tiled/scrolling/ScrollingAlgorithm.cpp:725-785,330-339`).
  Automatic admission, no binding; exact new focus/native journey TBD.
- Then bspwm: fixture-inapplicable: binary tree, anchor longest-side
  split with newcomer second child. `S(S-bsp-insert)`.
- Then i3 (`4.25.1`): fixture-inapplicable: attach under the focused
  tree parent, no strip (`src/tree.c:149-174`); automatic, no binding.
- Then xmonad/Tall+Navigation2D: fixture-inapplicable: flat
  master/stack, no columns. `S(S-xmo-layout)`.
- Then sway (`1.12`): fixture-inapplicable: sibling attach after the
  focused tiling node (`sway/tree/view.c:898-902`); automatic, no binding.
- Then qtile/Columns: fixture-inapplicable: the exact three-column
  fixture exceeds shipped `num_columns=2`; at the limit admission
  stays in the current column. `S(S-qti-add)`.
- Then awesome/tile: fixture-inapplicable: master/stack partition in
  insertion order, no strip. `S(S-awe-tile)`.
- Then niri: D opens as a new column at active+1 (after B) at the default
  width with existing widths kept (no rescale); pending
  maximized/fullscreen tiles stay in the scrolling layout. Under Smart
  with no pending fullscreen D activates and takes focus, and the view
  animates to D with the minimal fit under shipped `Never`.
  `S(S-nir-ins)` + `S(S-nir-base)` + `S(S-nir-view)`.
- Then PaperWM: D opens as a new column at selected+1 (between B and C) under the shipped RIGHT default, activated on show with inactive-space no-steal; existing columns keep widths (no rescale) and the viewport keeps D visible via minimal ensuredX scroll under DEFAULT. `S(S-pap-ins)` + `S(S-pap-layout)` + `S(S-pap-view)`.
- Then karousel/Lazy: new column after the last-focused column (B's C2 column under the B* tiled focus, so between B and C), window appended at the bottom; existing columns keep widths (newcomer width from its preferred width clamped into [min,max]; `columnsSetX` repositions only); newcomer focus TBD (fixture states no protocol, X11 user-time/startup/session, Wayland token/app-id/transient-serial, or FSP/rules inputs selecting the host activation fork); viewport is a deterministic conditional (Lazy minimal scroll toward the last-focused column: moves only if the newcomer is accepted and not contained); exact settled widths TBD (fixture states no newcomer frame/min/max inputs for the clamp). `S(S-kar-ins)` + `S(S-kar-fltanchor)` + `S(S-kar-scroll)` + `S(S-kar-min)` + `S(S-kwin-manage)` + `S(S-kwin-add)`.
- Then paneru: ordinary D takes the fresh path as a new Single column at index 2 between B and C (no rule index under shipped defaults; focused B at index 1 of 3, so the after-focus slot hits); newcomer focus is synthesized to D; existing columns keep widths with no rescale; the viewport reshuffles minimally toward D with no centering. `S(S-pan-fresh)` + `S(S-pan-base)` + `S(S-pan-stripwidth)` + `S(S-pan-colops)`.
- Then Ours KDE: no-counterpart: Engine has no column/strip admission;
  `Node` is Leaf or split-axis Group only. `S(S-ours-planops)` +
  `S(S-ours-grp)`.
- Then Ours Windows: no-counterpart: same shared Engine; no column
  admission verb in any layer. `S(S-ours-planops)` + `S(S-ours-grp)`.
- Variant hook: provisional/TBD (column-insertion hook, to discuss).

### R-COL-02: cycle column width through presets and back

- Given (column profiles): `COL[C1[A*],C2[B]]`, C1 at the smallest
  declared preset. Shipped presets apply; no manual widths.
- When: cycle column width through one full preset cycle, then reverse
  once, with each profile's native verb.
- Observe: ratio/pixel presets, wrap/stop and window vs column scope;
  oversized columns only where a native preset exists.
- Then COSMIC (`epoch-1.11.0`): no-counterpart, unbound: pixel resize,
  not presets (`src/shell/layout/tiling/mod.rs:2514-2605`). Super+R /
  Super+Shift+R enter ordinary resize (`data/keybindings.ron:91-92`).
- Then Hyprland/Dwindle (`v0.57.2`): no-counterpart, unbound: pixel/ratio
  resize only (`src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:312-470`).
  Optional scrolling `layoutmsg colresize +conf` / `-conf` steps/wraps
  configured widths (`src/layout/algorithm/tiled/scrolling/ScrollingAlgorithm.cpp:1233-1290`);
  defaults 0.333/0.5/0.667/1.0 (`src/config/values/ConfigValues.cpp:828`).
- Then bspwm: no-counterpart: `-z` pixel handle plus root `-E`/`-B`,
  no preset cycle. `S(S-bsp-resize)`.
- Then i3 (`4.25.1`): no-counterpart, unbound: resize grow/shrink/set,
  not preset cycle (`parser-specs/commands.spec:281-306`).
- Then xmonad/Tall+Navigation2D: no-counterpart: `Shrink`/`Expand`
  move `frac` only, no edge or preset verb. `S(S-xmo-resize)`.
- Then sway (`1.12`): no-counterpart, unbound: resize set/grow/shrink,
  not preset cycle (`sway/commands/resize.c:554-583`).
- Then qtile/Columns: no-counterpart for the preset cycle: directional
  grows move width/height from the neighbor plus `normalize`; no
  preset-step verb. `S(S-qti-resize)`.
- Then awesome/tile: no-counterpart: `incmwfact` master-factor step
  only, no preset cycle. `S(S-awe-resize)`.
- Then niri: steps the preset index through 1/3-1/2-2/3 forward and
  with `SwitchPresetColumnWidth(Back)`, wrapping at the ends;
  columns independent. `S(S-nir-resize)` + `S(S-nir-base)`.
- Then PaperWM: 10% grid step plus width-cycle direction with
  registered forward/backward verbs, wrapping on exhaust;
  neighbor columns keep their widths (per-column targetWidth, no rescale).
  `S(S-pap-resize)` + `S(S-pap-layout)`.
- Then karousel/Lazy: forward steps to the next strictly greater
  preset and wraps to the first on exhaust; reverse mirrors. Shipped
  presets 50%/100%. `S(S-kar-cycle)`.
- Then paneru: Grow/Shrink step through the shipped presets and wrap
  by default (`window_resize_cycle` true); `SetWidth` takes an exact
  ratio. `S(S-pan-colops)`.
- Then Ours KDE: no-counterpart: nearest-ancestor share resize only,
  no presets or equalize family. `S(S-ours-resize)`.
- Then Ours Windows: no-counterpart: no keyboard resize trigger at
  all; pointer resize maps to shares, never presets.
  `S(S-ours-resize)` + `S(S-ours-winbind)`.
- Variant hook: provisional/TBD (column-width hook, to discuss).

### R-COL-03: consume a column, then expel it

- Given (column profiles): `COL[C1[A*],C2[B]]`, separate equal-width
  columns. Shipped defaults apply.
- When: consume B into C1 with the native verb; then expel B with the
  native verb.
- Observe: which adjacent window/column is consumed, visible vertical
  allocation vs tabs, new column side and width recovery.
- Then COSMIC (`epoch-1.11.0`): no-counterpart, unbound: tree swap/reparent,
  no column consume/expel (`src/shell/layout/tiling/mod.rs:1507-1560`).
- Then Hyprland/Dwindle (`v0.57.2`): no-counterpart, unbound: directional
  remove/reinsert (`src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:558-595`).
  Optional scrolling `consume` takes the next column's first target;
  `expel` takes the current column's last target into a new column;
  `consume_or_expel prev|next` instead acts on the focused target
  (`src/layout/algorithm/tiled/scrolling/ScrollingAlgorithm.cpp:1554-1616`).
- Then bspwm: no-counterpart: leaf exchange only, no consume/expel
  verb. `S(S-bsp-move-target)`.
- Then i3 (`4.25.1`): no-counterpart, unbound: tree move/reparent, not
  column consume/expel (`src/move.c:259-280`, `parser-specs/commands.spec:12-47`).
- Then xmonad/Tall+Navigation2D: no-counterpart: same-layer stack swap
  only, no consume/expel verb. `S(S-xmo-nav)`.
- Then sway (`1.12`): no-counterpart, unbound: tree swap/reparent, not
  column consume/expel (`sway/commands/move.c:301-330`, `sway/commands.c:114-143`).
- Then qtile/Columns: no-counterpart: no consume/expel verb in the
  Columns command inventory (directional focus, shuffle carry, grow,
  swap-column, `toggle_split` display flip, normalize/reset only);
  As-Given `shuffle_left` from `A*` acts on C1's current A and is a
  leftmost sole-window no-op. `S(S-qti-shuffle)`.
- Then awesome/tile: no-counterpart: geometric position swap only, no
  consume/expel verb. `S(S-awe-swap)`.
- Then niri: left consume joins B into the left column (single-tile
  case) without activating inactive B; expel opens a new adjacent
  column. `S(S-nir-consume)`.
- Then PaperWM: `slurp` consumes the directional neighbor per
  `open-window-position` (RIGHT: B joins C1 at BOTTOM with an equal-height
  pass, emptied C2 removed); `barf` expels to a new column at the
  directional open position. Selection stays on the focused window on
  both steps (neither path writes selectedWindow; slurp skips ensure,
  barf re-ensures the same selection). `S(S-pap-slurp)` + `S(S-pap-view)`.
- Then karousel/Lazy: sole-window B joins the left column at the
  bottom; shared-column B expels to a new adjacent column, with
  focus-taker fixup. `S(S-kar-grpmove)`.
- Then paneru: `Stack(true)` merges B's column into the left neighbor;
  `Stack(false)` splits B back to an adjacent own column. Visible
  allocation is a split display (on this non-tabbed fixture neither
  path enters the `tabbed_stacks` branch; tabbed display needs the
  separate `ToggleTabbedDisplay` verb, off by default); focus is retained on both steps (neither path writes focus).
  `S(S-pan-colops)` + `S(S-pan-stack)`.
- Then Ours KDE: no-counterpart: directional move only, no
  consume/expel or tab carrier. `S(S-ours-move)` + `S(S-ours-grp)`.
- Then Ours Windows: no-counterpart: same Engine; no consume/expel
  verb. `S(S-ours-move)` + `S(S-ours-grp)`.
- Variant hook: provisional/TBD (consume/expel hook, to discuss).

### R-COL-04: focus an off-screen window by identity

- Given (column profiles): `COL[C1[A],C2[B*],C3[C],C4[D]]`, each 0.5W;
  `VP(x=0,W=2400)` shows A/B, D off-screen. Shipped scroll policy;
  fresh named Centered/Grouped (karousel) and CENTER/EDGE (PaperWM)
  variants.
- When: focus D by identity.
- Observe: minimal scroll vs centering/grouped scroll,
  partial-visibility threshold and offscreen input focus.
- Then COSMIC (`epoch-1.11.0`): fixture-inapplicable, unbound: tree focus,
  no strip viewport (`src/shell/layout/tiling/mod.rs:1835-2087`).
- Then Hyprland/Dwindle (`v0.57.2`): fixture-inapplicable, identity focus
  unbound: fixed-work-area tree, not a viewport. Optional scrolling responds
  to `focuswindow` by fitting the target column at `follow_focus=1`,
  `focus_fit_method=1`; center is a named alternative
  (`src/layout/algorithm/tiled/scrolling/ScrollingAlgorithm.cpp:640-723`,
  `src/config/values/ConfigValues.cpp:820-822`). Native endpoint TBD.
- Then bspwm: fixture-inapplicable: leaf selector over shown desktops,
  no strip viewport. `S(S-bsp-flt-focus)`.
- Then i3 (`4.25.1`): fixture-inapplicable, identity focus unbound:
  criteria focus selects a tree container, no viewport (`src/commands.c:1431-1465`).
- Then xmonad/Tall+Navigation2D: fixture-inapplicable: same-layer
  line/side navigation, no strip viewport. `S(S-xmo-nav)`.
- Then sway (`1.12`): fixture-inapplicable, identity focus unbound:
  criteria focus selects a tree node, no viewport (`sway/commands/focus.c:364-389`).
- Then qtile/Columns: fixture-inapplicable for the viewport leg: column
  left/right step with wrap, but all columns share the group width with
  no scrolling viewport. `S(S-qti-focus)` + `S(S-qti-resize)`.
- Then awesome/tile: fixture-inapplicable: nearest in-direction rect,
  no strip viewport. `S(S-awe-focus)`.
- Then niri: activating D's column animates the view with the minimal
  fit under shipped `Never`; Always/OnOverflow center instead.
  `S(S-nir-view)` + `S(S-nir-base)`.
- Then PaperWM: `switch` runs `ensureViewport` for minimal/margin
  scroll under DEFAULT; CENTER centers, EDGE aligns. `S(S-pap-focus)` +
  `S(S-pap-view)` + `S(S-pap-focusmode)`.
- Then karousel/Lazy: focusing D calls `scrollToColumn`, and Lazy
  applies the minimal `scrollIntoView`; Centered/Grouped variants
  center instead. `S(S-kar-scroll)` + `S(S-kar-base)`.
- Then paneru: focusing D reshuffles the strip to bring it into view;
  shipped `auto_center` off means no centering. `S(S-pan-colops)` +
  `S(S-pan-cmds)`.
- Then Ours KDE: fixture-inapplicable: leaf-only directional focus, no
  strip or viewport. `S(S-ours-focus)`.
- Then Ours Windows: fixture-inapplicable: same Engine; no viewport
  concept. `S(S-ours-focus)`.
- Variant hook: provisional/TBD (focus-scroll hook, to discuss).

### R-COL-05: explicitly center the focused column/window

- Given (column profiles): `COL[C1[A],C2[B*],C3[C],C4[D]]`, each 0.5W,
  zero gaps; `VP(x=0,W=2400)`, B fully visible off-center. Shipped
  scroll policy.
- When: explicitly center the focused column/window with the native
  verb.
- Observe: one-shot viewport change vs persistent centering policy,
  whole-column vs single-window target.
- Then COSMIC (`epoch-1.11.0`): fixture-inapplicable, unbound: output-bound
  tree, no strip viewport (`src/shell/layout/tiling/mod.rs:133-168`).
- Then Hyprland/Dwindle (`v0.57.2`): fixture-inapplicable, unbound:
  no strip viewport. Optional scrolling `layoutmsg center` centers the
  active column once without focus or persistent-policy writes
  (`src/layout/algorithm/tiled/scrolling/ScrollingAlgorithm.cpp:1663-1673`).
- Then bspwm: fixture-inapplicable: binary tree, no viewport.
  `S(S-bsp-insert)`.
- Then i3 (`4.25.1`): fixture-inapplicable, unbound: tree attachment,
  no viewport (`src/tree.c:149-174`, `parser-specs/commands.spec:12-47`).
- Then xmonad/Tall+Navigation2D: fixture-inapplicable: flat layout, no
  viewport. `S(S-xmo-layout)`.
- Then sway (`1.12`): fixture-inapplicable, unbound: workspace tiling
  siblings, no viewport (`sway/tree/view.c:898-902`, `sway/commands.c:114-143`).
- Then qtile/Columns: fixture-inapplicable: columns share the group
  width, no viewport to center. `S(S-qti-resize)`.
- Then awesome/tile: fixture-inapplicable: master/stack partition, no
  viewport. `S(S-awe-tile)`.
- Then niri: `CenterColumn` one-shot centers the active column only;
  the `Never` policy is unchanged and focus is untouched.
  `S(S-nir-view)` + `S(S-nir-base)`.
- Then PaperWM: `centerWindow` one-shot centers the frame in the work
  area via `move_to`; distinct from the persistent CENTER focus mode.
  `S(S-pap-view)` + `S(S-pap-focusmode)`.
- Then karousel/Lazy: `gridScrollFocused` one-shot scrolls to the
  focused column without touching focus; distinct from the
  Centered/Grouped scrollers. `S(S-kar-scroll)`.
- Then paneru: `Center` one-shot repositions the strip and records a
  manual offset so later reshuffles do not undo it; focus untouched.
  `S(S-pan-colops)`.
- Then Ours KDE: no-counterpart: no viewport or center verb in any
  Engine/adapter layer. `S(S-ours-planops)`.
- Then Ours Windows: no-counterpart: same Engine; no center verb.
  `S(S-ours-planops)`.
- Variant hook: provisional/TBD (viewport-center hook, to discuss).

### R-COL-06: toggle column tabbed/stacked display, then select

- Given (column profiles): `COL[C1[A],C2[B*,C]]`, B/C both visible.
  Shipped defaults apply (karousel no default stack).
- When: toggle the column tabbed/stacked display with the native verb;
  then select C.
- Observe: display-only membership/height retention vs actual grouping,
  single-visible tabs vs accordion.
- Then COSMIC (`epoch-1.11.0`): fixture-inapplicable: Super+S toggles a
  tree stack, not a scrollable column display (`data/keybindings.ron:84`,
  `src/shell/layout/tiling/mod.rs:2132-2251`); column action unbound.
- Then Hyprland/Dwindle (`v0.57.2`): fixture-inapplicable, unbound:
  compositor groups are not strip-column display. No column tabbed-display
  verb in optional scrolling's inspected `layoutMsg` inventory either
  (`src/layout/algorithm/tiled/scrolling/ScrollingAlgorithm.cpp:1155-1710`);
  grouping remains a separate target (`src/desktop/view/Group.hpp:18`).
- Then bspwm: fixture-inapplicable: no column or tabbed display.
  `S(S-bsp-insert)`.
- Then i3 (`4.25.1`): fixture-inapplicable: Mod1+S/W sets parent tree
  stacking/tabbed layout, not strip-column display (`etc/config:104-105`,
  `src/commands.c:1597-1622`); column action unbound.
- Then xmonad/Tall+Navigation2D: fixture-inapplicable: no tabs or
  columns in this profile. `S(S-xmo-layout)`.
- Then sway (`1.12`): fixture-inapplicable: `$mod+s/w` sets parent tree
  stacking/tabbed layout, not strip-column display (`config.in:149-150`,
  `sway/commands/layout.c:117-146`); column action unbound.
- Then qtile/Columns: `toggle_split` flips C2 between all-visible split
  and single-visible stacked (display-only: membership and height shares
  retained, only the `split` flag flips; stacked shows the current window
  full-height, the rest hidden). Selecting C runs the in-column step
  (`down()` from B under shipped `wrap_focus_stacks=true`); `next()`/
  `previous()` hop columns in stacked mode instead.
  `S(S-qti-split)` + `S(S-qti-focus)` + `S(S-qti-colmode)`.
- Then awesome/tile: fixture-inapplicable: tile partition only, no
  tabbed display. `S(S-awe-tile)`.
- Then niri: `ToggleColumnTabbedDisplay` flips Normal/Tabbed with
  membership/order/active retained; `focus_down`/`focus_up` step
  members. `S(S-nir-consume)`.
- Then PaperWM: no-counterpart: columns always show all windows
  vertically; no tabbed/stacked display verb in the action inventory.
  `S(S-pap-acts)`.
- Then karousel/Lazy: `toggleStacked` needs 2+ windows and switches
  between visible heights and overlapping arrange; `focusUp`/`focusDown`
  step members. `S(S-kar-grpmove)`.
- Then paneru: `ToggleTabbedDisplay` flips a multi-window Stack
  between visible split and single-visible tabs; tabs cycle with Focus
  North/South. `S(S-pan-colops)`.
- Then Ours KDE: no-counterpart: no tab carrier and no display-toggle
  verb. `S(S-ours-grp)`.
- Then Ours Windows: no-counterpart: same Engine; no display toggle.
  `S(S-ours-grp)`.
- Variant hook: provisional/TBD (column-display hook, to discuss).

### R-COL-07: send a whole column to another workspace

- Given (column profiles): WS1 has `COL[C1[A],C2[B*,C]]`, WS2 has one
  occupied column. Shipped defaults; native send/follow policy named.
- When: send whole C2 to WS2 with the native verb.
- Observe: atomic column transfer vs one-window send/no verb,
  membership/width preservation and target column position.
- Then COSMIC (`epoch-1.11.0`): column-atomic leg fixture-inapplicable,
  unbound: Move/Send acts on focused window/stack, no strip-column unit
  (`src/input/actions.rs:293-319`); reuse R-WS-01, not column transfer.
- Then Hyprland/Dwindle (`v0.57.2`): column-atomic leg fixture-inapplicable,
  unbound: ordinary window transfer (`example/hyprland.lua:286-294`).
  Optional scrolling's inspected `layoutMsg` inventory also has no
  whole-column workspace-send verb
  (`src/layout/algorithm/tiled/scrolling/ScrollingAlgorithm.cpp:1155-1710`).
- Then bspwm: column-atomic leg fixture-inapplicable: node send with
  `--follow`; reuse R-WS-01. `S(S-bsp-xfer)`.
- Then i3 (`4.25.1`): column-atomic leg fixture-inapplicable, unbound:
  sends a tree container, not strip column (`src/commands.c:343-364`);
  ordinary Mod1+Shift+digits send is distinct (`etc/config:153-162`).
- Then xmonad/Tall+Navigation2D: column-atomic leg
  fixture-inapplicable: `shiftWin` explicit-tag transfer only; reuse
  R-WS-01. `S(S-xmo-shift)`.
- Then sway (`1.12`): column-atomic leg fixture-inapplicable, unbound:
  sends a tree container, not strip column (`sway/commands/move.c:419-450`);
  ordinary `$mod+Shift+digits` send is distinct (`config.in:127-136`).
- Then qtile/Columns: column-atomic leg fixture-inapplicable:
  per-window `togroup` transfer; reuse R-WS-01. `S(S-qti-group)`.
- Then awesome/tile: column-atomic leg fixture-inapplicable:
  per-client tag move; reuse R-WS-01. `S(S-awe-tag)`.
- Then niri: `MoveColumnToWorkspace` carries the whole column object with
  membership and widths kept, landing after the target workspace's active
  column (index 1 after the sole column here); it keeps Maximized while
  dropping fullscreen. `S(S-nir-wscarry)`.
- Then PaperWM: single-window `takeWindow` only (no whole-column verb
  in the registered inventory): B transfers and reinserts at the target
  open position (selected+1 RIGHT, after the single occupied column)
  with shipped completion follow; C stays. No overlay in this fixture;
  overlay carry follows the ordinary existing-window admission leg.
  `S(S-pap-take)` + `S(S-pap-acts)` + `S(S-pap-ins)`.
- Then karousel/Lazy: `columnMoveToDesktop` moves the whole column
  object to the target grid appended after its last column.
  `S(S-kar-ws)`.
- Then paneru: virtual-row move carries only the window's native tab group
  (`tab_group`, else the single window), appending (or the mid-strip slot
  under the named option, off by default); visible-Stack mates stay behind,
  so whole-column transfer is not atomic (a native Tabs group moves together).
  Follow focus stays TBD (fixture states no `MoveFocus` Follow/Stay input
  selecting the follow fork). `S(S-pan-colops)` + `S(S-pan-ws)`.
- Then Ours KDE: no-counterpart: Engine send is same-output,
  single-window only. `S(S-ours-ws)`.
- Then Ours Windows: no-counterpart: same Engine boundary; index-only
  Select/Send, no column unit. `S(S-ours-ws)`.
- Variant hook: provisional/TBD (column-send hook, to discuss).

### R-COL-08: scroll the viewport without a focus command

- Given (column profiles): four 0.5W columns, focus A, VP at the strip
  start. Shipped scroll policy.
- When: scroll the viewport right by one native step without a focus
  command (keyboard step first, gesture leg only if source is unclear).
- Observe: viewport independent of focus vs automatic refocus; clamp
  and offscreen-focused-window policy.
- Then COSMIC (`epoch-1.11.0`): fixture-inapplicable, unbound: output-bound
  tree, no viewport (`src/shell/layout/tiling/mod.rs:133-168`).
- Then Hyprland/Dwindle (`v0.57.2`): fixture-inapplicable, unbound:
  no viewport. Optional scrolling `layoutmsg move <delta>` changes tape
  offset, then updates focus from the center column; not guaranteed
  focus-preserving (`src/layout/algorithm/tiled/scrolling/ScrollingAlgorithm.cpp:1170-1232`).
  Gesture motion uses `moveTape` without focus during motion; final snap/
  focus is separate (`:1712-1726`, `src/managers/input/trackpad/gestures/ScrollMoveGesture.cpp:66-142`).
- Then bspwm: fixture-inapplicable: no strip viewport.
  `S(S-bsp-insert)`.
- Then i3 (`4.25.1`): fixture-inapplicable, unbound: no viewport or
  scroll command (`src/tree.c:149-174`, `parser-specs/commands.spec:12-47`).
- Then xmonad/Tall+Navigation2D: fixture-inapplicable: no strip
  viewport. `S(S-xmo-layout)`.
- Then sway (`1.12`): fixture-inapplicable, unbound: no viewport or
  scroll command (`sway/tree/view.c:898-902`, `sway/commands.c:114-143`).
- Then qtile/Columns: fixture-inapplicable: columns share the group
  width, no viewport to scroll. `S(S-qti-resize)`.
- Then awesome/tile: fixture-inapplicable: no strip viewport.
  `S(S-awe-tile)`.
- Then niri: the touchpad gesture moves the view with no focus write
  during the motion; at the end it snaps to the closest column boundary,
  extends to the furthest column toward the gesture direction and
  activates that column, clamped to the first/last column so the view
  never rests beyond the strip. The full `Action` inventory carries no
  keyboard scroll-step verb (`CenterColumn`/`CenterWindow`/
  `CenterVisibleColumns` one-shots only).
  `S(S-nir-view)` + `S(S-nir-acts)`.
- Then PaperWM: swipe moves the view but reselects the swipe target,
  and background scroll only switches focus during grab/navigation.
  Keyboard drift-left/right move the view but also reselect - no
  focus-preserving keyboard scroll-step verb in the registered inventory.
  `S(S-pap-view)` + `S(S-pap-scroll)`.
- Then karousel/Lazy: `gridScrollLeft/Right` shift the viewport by the
  200px manual step with no focus call; clamped unless forced.
  `S(S-kar-scroll)`.
- Then paneru: no-counterpart - no manual-scroll verb in the `Operation`
  inventory; strip motion is focus/reshuffle-driven only. `S(S-pan-cmds)` +
  `S(S-pan-colops)`.
- Then Ours KDE: fixture-inapplicable: no strip viewport.
  `S(S-ours-planops)`.
- Then Ours Windows: fixture-inapplicable: no strip viewport.
  `S(S-ours-planops)`.
- Variant hook: provisional/TBD (manual-scroll hook, to discuss).

### R-COL-09: paneru virtual rows, send across rows

- Given (paneru): `Space1:{VW1=COL[C1[A*]],VW2=COL[C2[B]]}`. Shipped
  defaults; no custom rules.
- When: send A to VW2 without changing the native Space, with the
  native virtual-move verb and follow policy named.
- Observe: native Space vs internal row membership, empty-row reaping
  and append vs positional admission.
- Then COSMIC (`epoch-1.11.0`): no-counterpart, unbound: workspace owns
  tiling/floating layers directly, no nested virtual rows
  (`src/shell/workspace.rs:104-121`).
- Then Hyprland/Dwindle (`v0.57.2`): no-counterpart, unbound: numbered,
  named/special workspaces, not sub-rows; optional scrolling does not add
  a virtual workspace layer (`src/workspace/HLWorkspace.hpp:22-63`,
  `src/config/lua/bindings/LuaBindingsDispatchers.cpp:48-147`).
- Then bspwm: no-counterpart: monitor/desktop scope only, no virtual
  rows. `S(S-bsp-ws)`.
- Then i3 (`4.25.1`): no-counterpart, unbound: workspace containers
  attach to output content, no virtual sub-rows (`src/workspace.c:158-167`).
- Then xmonad/Tall+Navigation2D: no-counterpart: static workspace
  list, no virtual rows. `S(S-xmo-ws)`.
- Then sway (`1.12`): no-counterpart, unbound: output-owned workspace
  directly holds tiling/floating lists (`include/sway/tree/workspace.h:26-56`).
- Then qtile/Columns: no-counterpart: static groups 1-9 only, no
  virtual rows. `S(S-qti-ws)`.
- Then awesome/tile: no-counterpart: static tags only, no virtual
  rows. `S(S-awe-ws)`.
- Then niri: no-counterpart: each workspace owns one scrolling strip;
  no sub-rows. `S(S-nir-ws)`.
- Then PaperWM: no-counterpart: spaces are GNOME workspaces; no
  virtual rows. `S(S-pap-space)`.
- Then karousel/Lazy: no-counterpart: desktops are KWin-native; no
  virtual rows. `S(S-kar-ws)`.
- Then paneru: `VirtualMove` carries A's tab group to VW2, appending
  (or the mid-strip slot under the named option, off by default); absent-row spawn
  and reaping per config. Follow focus stays TBD (fixture states no `MoveFocus`
  Follow/Stay input selecting the follow fork, including the empty-source
  Stay-becomes-Follow branch). `S(S-pan-colops)` + `S(S-pan-ws)`.
- Then Ours KDE: no-counterpart: Engine workspaces have no row
  subdivision. `S(S-ours-ws)`.
- Then Ours Windows: no-counterpart: index-only workspaces, no rows.
  `S(S-ours-ws)`.
- Variant hook: provisional/TBD (virtual-row hook, to discuss).

### R-COL-10: app-native tabs nested in a vertical stack

- Given (paneru): `COL[C1[A*,B]]`, B an ordinary visible stack item;
  the same app supports native tabs. Shipped defaults; macOS host only.
- When: the app creates a native tab for A; then select B.
- Observe: native tabs nested inside a vertical stack vs all windows
  as peers, ownership/focus and column width stability.
- Then COSMIC (`epoch-1.11.0`): no-counterpart, unbound: maps individual
  surfaces, not app-native macOS tabs (`src/shell/mod.rs:2990-3030`).
- Then Hyprland/Dwindle (`v0.57.2`): no-counterpart, unbound: per-target
  admission in Dwindle and optional scrolling
  (`src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:63-67`,
  `src/layout/algorithm/tiled/scrolling/ScrollingAlgorithm.cpp:725-785`);
  compositor groups (`src/desktop/view/Group.hpp:18`) are not app-native tabs.
- Then bspwm: no-counterpart: per-node insert/focus; no app-tab
  nesting. `S(S-bsp-insert)`.
- Then i3 (`4.25.1`): no-counterpart, unbound: each managed window has
  its container, not app-native tab nesting (`src/manage.c:106-127`,
  `src/con.c:178-185`); WM tabbed layout is distinct.
- Then xmonad/Tall+Navigation2D: no-counterpart: `insertUp` per window;
  no app-tab nesting. `S(S-xmo-ins)`.
- Then sway (`1.12`): no-counterpart, unbound: each mapped view creates
  a container, not app-native tab nesting (`sway/tree/view.c:812-840`);
  WM tabbed layout is distinct.
- Then qtile/Columns: no-counterpart: per-client admission; no
  app-tab nesting. `S(S-qti-add)`.
- Then awesome/tile: no-counterpart: newcomer appended per client; no
  app-tab nesting. `S(S-awe-tile)`.
- Then niri: no-counterpart: tiler-owned tabbed display only;
  admission is per window. `S(S-nir-consume)` + `S(S-nir-ins)`.
- Then PaperWM: no-counterpart: per-window columns, transients float;
  no app-tab nesting. `S(S-pap-ins)` + `S(S-pap-spc)`.
- Then karousel/Lazy: no-counterpart: per-window tileability gate; no
  app-tab nesting. `S(S-kar-spc)`.
- Then paneru: same-app same-frame newcomer groups with the hidden
  leader via `convert_to_tabs` and takes focus; strays refold.
  Width stability and selecting B stay TBD (both ride the host app's tab
  creation/settlement timing and frames with no public macOS source).
  `S(S-pan-tabs)` + `S(S-pan-colops)`.
- Then Ours KDE: no-counterpart: per-window observation; no tab
  nesting. `S(S-ours-spc-kde)` + `S(S-ours-grp)`.
- Then Ours Windows: no-counterpart: per-window admission gates; no
  tab nesting. `S(S-ours-spc-win)` + `S(S-ours-grp)`.
- Variant hook: provisional/TBD (native-tab hook, to discuss).
