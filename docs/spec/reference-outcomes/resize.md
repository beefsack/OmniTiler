# Resize (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

Reuse: R-CLOSE-02 ratio persistence through close/open, R-FLT-03 removal ratios, R-MIN-01..03 hint limits (minimum-size supplement).

## New scenarios (GWT, piece B4; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours cells cite Engine + adapter source at `9241c94`
(`S(S-ours-resize)`); selected intent and doc assertions are never evidence.

### R-RSZ-01: keyboard grow/shrink of a tiled pair

- Given (tree profiles): `H[A*,B]` 50/50, no limiting hints. Ordinary
  windows, no rules, scale 1, zero gaps for reference geometry.
- Given (column profiles): `COL[C1[A*],C2[B]]`, each `0.5W`, shipped
  defaults apply; viewport recorded.
- When: keyboard grow A toward B one declared step; shrink one step.
  Native verbs per profile: COSMIC `Resizing(Outwards)` / `Resizing(Inwards)`
  (`Super+r` / `Super+Shift+r`, then Right); Hyprland relative `resize`
  delta (+10,0) / (-10,0); bspwm `node A -z right 10 0` / `right -10 0`;
  i3 `resize grow right 10 px` / `resize shrink right 10 px`;
  xmonad `Expand` / `Shrink` (`mod-l` / `mod-h`);
  sway `resize grow right 10 px` / `resize shrink right 10 px`; qtile Columns
  `grow_right()` / `grow_left()` (`grow_amount` 10); awesome
  `tag.incmwfact(+0.05)` / `(-0.05)` (`mod-l` / `mod-h`); niri
  `switch-preset-column-width` / `switch-preset-column-width-back`
  (`Mod+R` / `Mod+Shift+R`); PaperWM `resizeWInc` / `resizeWDec`; karousel
  `columnWidthIncrease` / `columnWidthDecrease`; paneru `Resize(Grow)` /
  `Resize(Shrink)`; Ours KDE world-route `resize` operation commands (outwards/inwards toward right,
   press counter from 0); Windows
   Win+Alt+Right / Win+Alt+Shift+Right use the shared resize path. Explicit px legs are named command parameters,
  not changes to shipped config; bare i3/sway defaults use ppt.
- Observe: ratio vs pixel increment, neighbor allocation and reversibility.
- Then COSMIC: pixel step moves the shared boundary (Outwards grows A,
  Inwards shrinks A, symmetric); only the two adjacent shares change;
  pair/leaf minima gate and clamp one-sided. `S(S-cos-resize)`.
- Then Hyprland/Dwindle: the declared +10px delta grows A through the
  shared H split (corner-NONE smart path selects the side-by-side
  parent; only the pair changes) and -10px negates it, so reversible
  absent clamping.
  `S(S-hyp-resize)`.
- Then bspwm: Relative `-z` handle moves the fence share: `right 10 0` from A finds the east fence via `find_fence` and shifts `split_ratio` by `dx/fence-width` clamped to [0,1] with reflow (only the fenced split changes); `right -10 0` negates it, so reversible absent clamping; `S(S-bsp-resize)` + `S(S-bsp-ptrresize)`.
- Then i3: grows/shrinks by the explicit 10px against the
  tiling participant found by climbing to the matching orientation;
  shrink is the negated grow, so reversible. `S(S-i3-resize)`.
- Then xmonad/Tall+Navigation2D: `Expand`/`Shrink` move master `frac` by
  `delta` 3/100 (master grows, stack share shrinks); reversible pair.
  `S(S-xmo-resize)`.
- Then sway: grows/shrinks by the explicit 10px against the resize parent
  found by climbing to the matching
  layout; shrink is the negated grow. `S(S-sway-resize)`.
- Then qtile/Columns: transfers 10 width-weight units from B to A:
  100/100 becomes 110/90 (55/45 shares, not a pixel step); `grow_left()`
  reverses this pair. `S(S-qti-resize)`.
- Then awesome/tile: `incmwfact(+0.05)` grows the master share (A) against
  the stack share (B); `-0.05` reverses it. `S(S-awe-resize)`.
- Then niri: preset-cycle step changes the focused column width to the next
  preset (from 0.5 toward 2/3 forward, back returns); columns are
  independent, no neighbor share is taken. `S(S-nir-resize)`.
- Then PaperWM: width snaps to a 10%-of-available-width grid and steps;
  Inc/Dec reverse only on that grid (0.5W can be off-grid after margins).
  Neighbor columns keep their widths (per-column targetWidth, no rescale);
  layout re-ensures placement after the frame write. `S(S-pap-resize)` +
  `S(S-pap-layout)`.
- Then karousel/Lazy: contextual increase picks the smallest strictly greater width among visible-space slack plus presets and writes it as preferredWidth with recenter; decrease runs the separate offscreen-terms path over presets (so shrink is not the inverse of grow); neighbor widths are untouched (independent columns, reposition only); focus unwritten. Exact step TBD (fixture records but does not state viewport width). `S(S-kar-resize)` + `S(S-kar-min)` + `S(S-kar-scroll)` + `S(S-kar-base)`; step TBD (F: missing viewport width).
- Then paneru: `Resize(Grow)` steps the focused width to the next preset
  above (0.5W to 0.66667 at shipped defaults) and `Resize(Shrink)`
  returns it; neighbours are untouched (only the focused window plus
  stacked siblings resize, then reshuffle repositions). `S(S-pan-resize)`
  + `S(S-pan-base)`.
- Then Ours KDE: Outwards moves the shared boundary by 12px (press 0, then
  14/16/18/20 on repeat) from B's share into A's; only adjacent shares
  change; Inwards reverses it; minima refuse or clamp. The KDE adapter
  dispatches `op: "resize"` with direction/mode/press_index.
  `S(S-ours-resize)`.
- Then Ours Windows: delivered 2026-10-11, base `f794cf9` + delivery commit:
  dedicated Alt intent dispatches `CoreCommand::Resize`; retained-Engine
  fixtures prove 12/14/16/18/20px cap, adjacent shares only, inward reversal
  and minima clamp. Agent-observed exact-owner CLI native grow/repeat/reversal
  and vertical grow passed; Settings Keep/Disable/Rebind Apply/Revert passed.
  Physical chord interception/rebound adoption and native minima-floor
  exhaustion remain user-owned. `S(S-ours-winbind)`;
  [record](../../changes/archive/windows-keyboard-resize.md).
- Variant hook: provisional/TBD (no suitable existing hook; V-FLOAT-REFLOW
  covers float-removal reflow, not keyboard resize).

### R-RSZ-02: outward resize at the work-area edge

- Given (tree/layout-driven profiles): `H[A*,B]`, A touches the left work-area edge.
  Ordinary windows, no rules, scale 1, zero gaps for reference geometry.
- Given (column profiles): `COL[C1[A*],C2[B]]` at shipped defaults; A
  touches the left viewport edge; viewport recorded.
- When: request outward resize at A's left edge. Native verbs per profile:
  COSMIC resize with LEFT edge Outwards; Hyprland edge/corner resize delta
  (smart-resizing path); bspwm `node -z left ...`; i3 `resize grow left`;
  sway `resize grow left`; qtile `grow_left()`; Ours Engine
   `propose_resize` Left Outwards (both adapters). No generic width verb is substituted for the scrolling
  profiles: each profile's edge-targeted inventory is established in its
  Then (no-counterpart where the pinned inventory lists no edge verb).
  Fixture must distinguish edge resize from generic width grow (R-RSZ-01).
- Observe: clamp/no-op, opposite-edge redistribution or overflow.
  Edge-targeted verb absence is not a no-op.
- Then COSMIC: no-op (no matching-edge ancestor at the outer edge; tree and
  focus unchanged). `S(S-cos-resize)`.
- Then Hyprland/Dwindle: no edge no-op: the outward delta moves the
  shared H split through the same corner-NONE smart path (no separate
  edge verb; edge position feeds only the full-span DISPLAY gates),
  clamped 0.1-1.9.
  `S(S-hyp-resize)`.
- Then bspwm: No-op: the outer-edge `left` handle finds no west fence via `find_fence`, so `resize_client` returns false with tree and focus unchanged and no redistribution or overflow; `S(S-bsp-resize)` + `S(S-bsp-ptrresize)`.
- Then i3: no-op (no second container in that direction; command errors,
  tree and focus unchanged). `S(S-i3-resize)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no edge-targeted verb;
  `Shrink`/`Expand` are frac-only). `S(S-xmo-resize)` + `S(S-xmo-core-nav)`.
- Then sway: no-op (resize parent search excludes the outer edge; unchanged
  boundaries report `Cannot resize any further`). `S(S-sway-resize)`.
- Then qtile/Columns: redistribution (`grow_left()` at the leftmost column
  transfers `grow_amount` from A's share to B: A shrinks, B grows).
  `S(S-qti-resize)`.
- Then awesome/tile: no-counterpart (no edge-targeted verb; `incmwfact`
  only). `S(S-awe-keys)` + `S(S-awe-resize)`.
- Then niri: no-counterpart (the pinned Action inventory lists preset,
  maximize, set/adjust and expand width verbs, none edge-targeted).
  `S(S-nir-resize)`.
- Then PaperWM: no-counterpart (the registered action inventory lists
  w/h inc/dec plus width/height cycling only, none edge-targeted).
  `S(S-pap-resize)`.
- Then karousel/Lazy: no-counterpart (the action inventory lists width
  increase/decrease/maximize/minimize/cycle/equalize/squeeze, none
  edge-targeted). `S(S-kar-resize)`.
- Then paneru: no-counterpart (the `Operation` inventory lists
  preset-cycling `Resize`/`SetWidth`, none edge-targeted).
  `S(S-pan-resize)`.
- Then Ours KDE: no-op (exhausted/clamped-unchanged boundary refuses
  `Unchanged` with no plan and no pending). `S(S-ours-resize)`.
- Then Ours Windows: delivered 2026-10-11, base `f794cf9` + delivery commit:
  retained-Engine edge/cross-axis no-mutation fixtures passed; agent-observed
  CLI left/top outer-edge `unchanged` with bit-identical native rects.
  Physical chord journey pending. `S(S-ours-winbind)`;
  [record](../../changes/archive/windows-keyboard-resize.md).
- Variant hook: provisional/TBD.

### R-RSZ-03: nested resize scope (nearest split vs ancestor)

- Given (tree profiles): `H[H[A*,B],C]`, inner/outer halves. Ordinary
  windows, no rules, scale 1, zero gaps for reference geometry.
- Given (column profiles): none exists. Recursive same-axis ancestry has no
  faithful column fixture, so there is no column Given; scrolling Thens
  below are fixture-inapplicable qualifications. Consume/expel mechanics
  belong to R-COL-03.
- When: grow A right one step. Native verbs per profile: same keyboard-grow
  verbs as R-RSZ-01 per profile.
- Observe: nearest split vs ancestor redistribution; which ratio changes.
- Then COSMIC: inner H split moves (nearest matching-edge-axis ancestor
  wins); outer shares unchanged. `S(S-cos-resize)`.
- Then Hyprland/Dwindle: inner H split moves (corner-NONE smart walk
  selects the side-by-side inner parent first; outer shares unchanged).
  `S(S-hyp-resize)`.
- Then bspwm: Inner split moves: `right` from A finds the first east fence (the inner H parent) via `find_fence`, shifting only its `split_ratio` with reflow; the outer share stays unchanged; `S(S-bsp-resize)` + `S(S-bsp-ptrresize)`.
- Then i3: inner H participant pair moves (find climbs to the first
  matching orientation); outer percent unchanged. `S(S-i3-resize)`.
- Then xmonad/Tall+Navigation2D: fixture-inapplicable (flat Tall has no
  nesting levels). `S(S-xmo-layout)`.
- Then sway: inner H resize parent moves (climb stops at the first L_HORIZ
  parent); outer fractions unchanged. `S(S-sway-resize)`.
- Then qtile/Columns: fixture-inapplicable (flat Columns has no nested H
  ancestor). `S(S-qti-shuffle)` + `S(S-qti-add)`.
- Then awesome/tile: fixture-inapplicable (flat tiled-client list has no
  nesting levels). `S(S-awe-swap)` + `S(S-awe-tile)`.
- Then niri: fixture-inapplicable (ordered columns have no nested H
  ancestor). `S(S-nir-move)`.
- Then PaperWM: fixture-inapplicable (column/row membership has no nested
  H ancestor). `S(S-pap-move)`.
- Then karousel/Lazy: fixture-inapplicable (Grid/Column membership has no
  nested H ancestor). `S(S-kar-move)`.
- Then paneru: fixture-inapplicable (strip/column model has no nested H
  ancestor). `S(S-pan-model)`.
- Then Ours KDE: inner group shares move (nearest matching-edge-axis
  ancestor wins); only the two adjacent shares change. `S(S-ours-resize)`.
- Then Ours Windows: shared nearest matching-edge-axis path wired 2026-10-11,
  base `f794cf9` + delivery commit; Windows retained-Engine adjacent-share
  fixtures passed. Exact nested native fixture and physical chord journey
  remain TBD. `S(S-ours-resize)` + `S(S-ours-winbind)`;
  [record](../../changes/archive/windows-keyboard-resize.md).
- Variant hook: provisional/TBD.

### R-RSZ-04: equalize/balance once

- Given (N-ary tree profiles): `H[A*,B,C]` 50/30/20. Ordinary windows, no
  rules, scale 1, zero gaps for reference geometry.
- Given (binary profiles): `H[A*,H[B,C]]`, outer ratio 0.5 and inner
  B/C ratio 0.6/0.4, yielding the same 50/30/20 rectangles; this is an
  explicit embedding, not a flat N-ary tree. Layout-driven profiles use
  the same rectangle projection; qtile uses the column Given below.
- Given (scrolling profiles and qtile/Columns): `COL[C1[A*],C2[B],C3[C]]` with widths
  0.5W/0.3W/0.2W; viewport shows all three columns; shipped defaults
  apply; viewport recorded.
- When: equalize/balance once, as two independent legs from fresh fixtures
  with a reset between: leg `-E` (equalize) and leg `-B` (balance) where
  the profile names both; otherwise the profile's single verb. Native
  verbs per profile: bspwm `bspc node @/ -E` / `bspc node @/ -B`
  (workspace root, not focused leaf A); qtile `normalize()`; karousel
  `columnsWidthEqualize`; paneru `Equalize` / `Balance`. Other profiles
  per inventory below. Binary embedding must distinguish equalize from
  equal leaf area.
- Observe: equal sibling shares vs recursive tree balance, preserved ratios
  or missing command.
- Then COSMIC: no-counterpart (no equalize/balance verb in the action,
  binding, or tiling inventories: the shortcut Action inventory carries
  only `Resizing` for resize plus magnification `ZoomIn`/`ZoomOut`, the
  shipped keybindings bind only `Resizing(Outwards/Inwards)`, and the
  tiling layout exposes only edge-walk `possible_resizes`,
  `resize_request`, and pixel `resize`; admission `equal_sizing` is an
  automatic new-window split, not a user verb; neither a local nor a
  workspace-wide equalize exists). `S(S-cos-resize)`.
- Then Hyprland/Dwindle: no-counterpart (no equalize/balance verb in the
  action, dispatcher, or Dwindle inventories: the window action
  declarations and dispatched names carry only pixel `resize`, the
  Dwindle path is per-split pixel `resizeTarget`, and the Dwindle
  `layoutmsg` inventory is togglesplit/swapsplit/rotatesplit/movetoroot/
  preselect/splitratio only; `splitratio` adjusts the single
  `CURRENT_NODE` parent split by delta or exact value, not the whole
  workspace, so it is not an equalize counterpart).
  `S(S-hyp-resize)`.
- Then bspwm: leg `-E` resets every split ratio to the configured 0.5, so
  A stays 0.5 and the inner pair splits evenly: 0.5/0.25/0.25. Fresh leg
  `-B` rebalances by leaf count (outer 1/3, inner 1/2): exact thirds.
  `S(S-bsp-resize)` + `S(S-bsp-bal)` + `S(S-bsp-ins)` (default ratio).
- Then i3: no-counterpart (no equalize/balance verb in the command
  inventory: `INITIAL` lists move/exec/layout/focus/split/`resize`/swap
  and others with no equalize form, and the `RESIZE` grammar is
  grow/shrink/set only; grow/shrink moves one tiling pair by px/ppt and
  `resize set` writes an exact width/height on the single focused
  container, neither equalizes all siblings nor balances the tree).
  `S(S-i3-resize)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no equalize verb in the
  profile: core `Tall` handles only `Shrink`/`Expand`/`IncMasterN` with
  keys for focus/swap/shrink/expand/master-count/sink only; contrib
  `BinarySpacePartition` `Balance`/`Equalize` messages exist but are
  out of this Tall+Navigation2D profile, so the binary-embedded Given
  has no applicable equalize leg). `S(S-xmo-resize)`.
- Then sway: no-counterpart (no equalize/balance verb in the command
  inventory: the runtime table carries layout/move/`resize`/split/swap
  and others with no equalize form, and `resize` dispatches only
  set/grow/shrink; grow/shrink moves one resize-parent pair and `resize
  set` writes an exact size on the single focused container; arrange
  `normalize` only re-sums fractions to 1.0 internally, it is not a user
  verb). `S(S-sway-resize)`.
- Then qtile/Columns: `normalize()` sets every column width (and in-column
  heights) to 100: exact thirds on the column Given. `S(S-qti-resize)`.
- Then awesome/tile: no-counterpart (no equalize verb in the tag, key,
  layout, or client inventories: `incmwfact`/`setmwfact` step or set the
  single master factor, `incnmaster`/`incncol` change counts, and
  `setwfact`/`incwfact` write one client's window factor with the rest
  rescaled; none writes equal shares to all clients).
  `S(S-awe-resize)`.
- Then niri: no-counterpart (no equalize/balance verb in the Action or
  scrolling inventories: the width actions are per-column/per-window
  preset-cycle, set/adjust proportion/fixed, single-window height reset,
  single-column maximize/expand, and viewport centering; `SetColumnWidth`
  /`SetWindowWidth` target one column or window, `ResetWindowHeight`
  restores automatic height only, `ExpandColumnToAvailableWidth` grows
  only the focused column, and `CenterVisibleColumns` recenters without
  equalizing widths). `S(S-nir-resize)`.
- Then PaperWM: no-counterpart (no equalize/balance verb in the
  registered action or tiling inventories: the registered actions are
  w/h inc/dec plus per-window width/height cycling plus
  center/slurp/barf/maximize only; `resizeWInc`/`resizeWDec` step one
  window by 10% and width cycling moves one window through presets via
  `findNext`/`findPrev`; none equalizes the whole space).
  `S(S-pap-resize)`.
- Then karousel/Lazy: `columnsWidthEqualize` gives the visible columns
  equal shares via `fillSpace` (min/max-clamped; no limiting hints here).
  All three columns are visible in this Given, so the visible scope is the
  full strip: exact thirds. `S(S-kar-resize)`.
- Then paneru: `Equalize` is a no-op on these widths (it evens heights
  inside `Stack` columns only; single-window `Single` columns are
  skipped). `Balance` sets every column to the focused A width, so all
  three columns become 0.5W. `S(S-pan-resize)`.
- Then Ours KDE: no-counterpart (no equalize verb in the op inventory:
  directional resize only). `S(S-ours-resize)`.
- Then Ours Windows: no-counterpart (no equalize verb in the operation
  inventory; directional keyboard resize is a separate operation).
  `S(S-ours-resize)` + `S(S-ours-winbind)`.
- Variant hook: provisional/TBD.
