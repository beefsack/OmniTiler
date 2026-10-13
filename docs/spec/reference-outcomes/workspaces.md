# Workspace send / follow / return (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

## 3. Workspace send / follow / return

<a id="scrolling-backfill-additive-existing-wide-tables-above-unchanged"></a>
<a id="r-ws-01-backfill-send-to-another-workspace-scrolling"></a>
### R-WS-01: send to another workspace

- Given (tree profiles): WS1 `H[A,B*]`, WS2 `H[C]`

- Given (column profiles): `WS1=COL[C1[A],C2[B*]]`, `WS2=COL[C1[C]]`, each
  0.5W at shipped defaults. Paneru uses one Space with two virtual
  rows (A/B-active row, C row).

- When: Move/send B to WS2 with the profile's shipped-default binding. From fresh fixtures, replay the alternate stay/follow verb, flag or composition where established; do not assume the default follows.

- When (column leg): move/send B to WS2 with the profile's shipped-default workspace-transfer binding; replay the established alternate from a fresh fixture. Record column vs window granularity and finish any navigator/drop journey before observing focus.

- Observe: Source collapse, target position, focus

- Observe (column leg): source collapse, target column position, focus
  (follow vs stay).

- Then COSMIC: Shipped `MoveToWorkspace` (Super+Shift+1..9) follows with B: source collapses to A; target splits C's long edge (C geometry unrecorded), B after C; alternate `SendToWorkspace` (unbound at shipped defaults) leaves focus (falls back to A); `S(S-cos-wskeys)` + `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)` + `S(S-cos-send)` + `S(S-cos-focusfix)`
- Then Hyprland/Dwindle: Shipped `hl.dsp.window.move({workspace=i})` (mainMod+Shift+n, follow absent so silent is false) follows: switches workspace and focuses mover; alternate `follow=false` stays (silent refocuses the source); source collapses via sibling promotion; target anchor is sole C regardless of cursor (only tiled candidate on WS2); splits C's long edge (C geometry unrecorded, so axis TBD), B before/after C TBD (cursor half); `S(S-hyp-wskeys)` + `S(S-hyp-movews)`
- Then bspwm: Shipped `node -d '^{1-9,10}'` (super+shift+n, no `--follow`) stays on the source; alternate `--follow` keeps focus on B; source unlinks with sibling promotion; target inserts at WS2 focus C, splitting C's long edge with B second child after C; exact axis (C geometry unrecorded)/frames TBD; `S(S-bsp-wskeys)` + `S(S-bsp-send)` + `S(S-bsp-xfer)`
- Then i3: Shipped `move container to workspace number N` (Mod1+Shift+n, no-follow, stays on WS1): source collapses to sole A; target C,B with B after focused C; focus stays A; alternate independent `workspace` command switches instead; `S(S-i3-wskeys)` + `S(S-i3-movews)`
- Then xmonad/Tall+Navigation2D: Shipped mod-shift-[1..9] `W.shift` stays (source view unchanged): shiftWin inserts above target focus via insertUp; alternate composition `W.greedyView . W.shift` views after the shift; `S(S-xmo-wskeys)` + `S(S-xmo-shift)`
- Then sway: Shipped `move container to workspace number N` ($mod+Shift+n, no-follow, stays on WS1): source collapses to sole A; target C,B with B after focus-inactive C; mover focus restored to source inactive (A); alternate independent `workspace` command switches instead; `S(S-sway-wskeys)` + `S(S-sway-movews)` + `S(S-sway-switch)`
- Then qtile/Columns: Shipped `togroup(i.name, switch_group=True)` (mod+shift+n) follows with mover focus: `togroup` hides B, `remove`s it from the source (emptied C2 dropped, source collapses to sole A with positional refocus to A) and `group.add` admits B into a new second column on the target (single-column target under shipped `num_columns=2` right-align, so `COL[C1[C],C2[B*]]`) with newcomer focus; `switch_group=True` then pulls the target via `toscreen`. Alternate `switch_group=False` (commented shipped form) stays on the source with A refocused. `S(S-qti-wskeys)` + `S(S-qti-group)` + `S(S-qti-add)` + `S(S-qti-remove)`
- Then awesome/tile: Shipped mod+Shift+numrow `move_to_tag` stays (sets screen plus tags with no view switch): source reflows via tile recalc with history refocus to A (non-sticky-first MRU via the delayed tagged/untagged refocus; the pre-move activate emits while B is still visible); target keeps B's retained global-client position (move_to_tag reinserts nothing, tile is stateless recalc); alternate `tag:view_only()` after the move switches; exact pixel frames TBD (F: work area unrecorded); `S(S-awe-keys)` + `S(S-awe-tag)` + `S(S-awe-hist)` + `S(S-awe-tile)`
- Then niri: Shipped `move-column-to-workspace N` (Mod+Ctrl+N,
  column granularity) with `focus=true` (default) follows with B via
  Smart activation; alternate `focus=false` stays. B transfers to the
  target workspace as a new column after C (the sole target column is
  active). `S(S-nir-wskeys)` + `S(S-nir-ws)`.
- Then PaperWM: Shipped `move-down/up-workspace` (Super+Ctrl+PageDown/Up)
  and `take-window` (Super+t) follow: take removes B via
  `space.removeWindow` (column splice, source collapses to A) and steps
  to the adjacent space with an end stop; the drop inserts at the open
  position (selected+1 under the shipped RIGHT default) as selectedWindow
  plus `Main.activateWindow`, so B follows; fresh-insertion no-steal on
  inactive spaces is a different journey, not the shipped-send outcome;
  no send-and-stay counterpart exists in the registered inventory.
  `S(S-pap-take)` + `S(S-pap-ins)` + `S(S-pap-space)`.
- Then karousel/Lazy: shipped `column-move-to-desktop-{}` stays on the
  source (no desktop-switch verb exists; desktops/switching are
  KWin-native): the whole column moves grids via `moveToGrid` and
  appends after the target's last column (after sole C); source drops
  the column with left-else-right `lastFocusedColumn` fixup and an
  Immediate request that focuses source survivor A through the script
  path (raise, on-current-desktop so no switch, reasonable-policy
  `requestFocus` takes focus). `S(S-kar-ws)` + `S(S-kwin-scriptact)`.
- Then paneru: `VirtualMoveNumber` carries the focused window to the
  indexed row under the `MoveFocus` Follow/Stay policy, appending B after
  sole C (strip-end append with `insert_windows_mid_strip` off, not
  after-focus). Follow switches the view and focuses B; Stay keeps the
  source view and refocuses A. `S(S-pan-ws)`.
- Then Ours KDE: source collapses; target admits at remembered-leaf/focus-history/root
  identically for follow/stay. Numbered follow defaults unchanged; explicit
  stay is registered unbound, preserves source selection and applies source
  focused-removal MRU (A here). Floating boundaries transfer membership only,
  default follows and explicit stay preserves source view/native boundary focus.
  Item 2 implemented offline, native journey pending.
  [Core/protocol](../../../crates/tiler-core/tests/session_send_to_workspace.rs),
  [adapter fixtures](../../../kwin/tests/workspace-send-follow-stay.test.ts),
  [record](../../changes/archive/kde-workspace-send-follow-stay.md);
  `D(D-dec-cos)` + step-3 `D(D-dec-ww)`.
- Then Ours Windows: explicit numbered/relative follow/stay delivered,
  same retained Engine admission/geometry. Agent-observed Notepad follow
  switches/focuses mover; stay keeps source with MRU survivor (null plan
  issues no setter), target hidden/unwritten. Native gates passed 2026-10-11,
  base `553c65b` plus [delivery record](../../changes/archive/windows-workspace-send-follow-stay.md).
  Physical chords and this fixture's exact ordinary target order remain TBD;
  `D(D-dec-cos)` + step-3 `D(D-dec-ww)`.
- Variant hook: V-WS-FOLLOW.

<a id="r-ws-02-backfill-send-back-and-return-anchor-scrolling"></a>
### R-WS-02: send back and return anchor

- Given (tree profiles): WS1 tall case `H[C,V[A,B*]]` or wide case `V[C,H[A,B*]]`; WS2 empty; inner area 2544x1364, gap 8: A becomes 1268x1364 (tall) or 2544x678 (wide) after B leaves

- Given (column profiles): the original `H[C,V[A,B]]` ancestry has no exact
  column counterpart, so that ancestry is fixture-inapplicable; the
  model-qualified rerun below reproduces the predicate with the same
  explicit preparation. `WS1=COL[C1[C],C2[A],C3[B*]]`, WS2 empty.
  Focus A then B (WS1 history A,B); send B to WS2; select WS1 and
  focus A; select WS2 and focus B; send B back to WS1. No step is
  omitted and C is never dropped from the fixture.

- When: Focus A then B; send B to WS2; select WS1/focus A; select WS2/focus B; send B back to WS1

- When (column leg): both selections and both sends run through the profile's
  native verbs.

- Observe: Return anchor + side/order + axis, rather than old-slot restoration

- Observe (column leg): return anchor and order, focus, viewport.

- Then COSMIC: Returns at A (target MRU): tall `V[A,B]` stacked, wide `H[A,B]` side-by-side, B after A, no old-slot restore; `SendToWorkspace` stays on WS2 (kept because active, not as trailing empty) with focus none, `MoveToWorkspace` follows to WS1 with B; `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)` + `S(S-cos-send)` + `S(S-cos-focusfix)`
- Then Hyprland/Dwindle: No remembered-leaf anchor in source (not MRU/slot restore): the target holds C and A (B's departure leaves both), so the return anchor is C vs A TBD (cursor closest-node; sends carry no focal, and the mover itself is excluded from anchor candidacy); only if A is selected does the given box resolve the axis (tall 1268x1364 portrait so `V`, wide 2544x678 landscape so `H`); order TBD (cursor half); follow switches workspace and focuses mover, silent refocuses the source; `S(S-hyp-movews)`
- Then bspwm: Return inserts at WS1 focus A (explicit selection) via `transfer_node` at `dd->focus`: source unlinks with sibling promotion, then `insert_node` splits A's long edge with B second child after A: tall `V[A,B]`, wide `H[A,B]`; no old-slot restore (fresh split); shipped `node -d` without `--follow` stays on the source, `--follow` focuses B; `S(S-bsp-xfer)` + `S(S-bsp-insert)`
- Then i3: `move container to workspace` (no-follow both legs, stays on source): B returns into the surviving single-child parent after A (tall `V[A,B]`, wide `H[A,B]`), old-slot coincidence via parent persistence, not MRU fresh-map; return leaves focus on the now-empty WS2; `S(S-i3-movews)`
- Then xmonad/Tall+Navigation2D: Return inserts above live WS1 focus A via `insertUp` (target-stack order `[B,A]`, B focused there; no follow - `shiftWin` never changes the current view; no old-slot store anywhere in `StackSet`); Tall is fixed master/stack (no long-edge axis, no MRU/history anchor), so the tall `V`/wide `H` fixture distinction is inapplicable and no axis applies. `S(S-xmo-shift)` + `S(S-xmo-layout)`
- Then sway: `move container to workspace` (no-follow both legs, stays on source): B returns into the surviving single-child parent after focus-inactive A (tall `V[A,B]`, wide `H[A,B]`), old-slot coincidence via parent persistence, not MRU fresh-map; axis from the surviving parent layout, not geometry; return leaves focus on the now-empty WS2; `S(S-sway-movews)` + `S(S-sway-cleanup)`
- Then qtile/Columns: Return is fresh admission at live target focus (`insert_position=0` inserts B above current A in A's column, so C2 holds `[B*,A]` with newcomer focus), no old-slot restore; Columns has no long-edge axis (in-column vertical stack, width-shared columns), so the tall/wide axis distinction is inapplicable and there is no viewport to restore. `S(S-qti-group)` + `S(S-qti-add)` + `S(S-qti-colmode)`
- Then awesome/tile: Both legs no-follow (move_to_tag sets screen plus tags with no view switch); return keeps B's retained global-client position via move_to_tag (no reinsertion, no old-slot store; tile is stateless recalc); tall/wide long-edge distinction inapplicable (fixed master/stack partition); source view unchanged either leg; exact pixel frames TBD (F: work area unrecorded); `S(S-awe-tag)` + `S(S-awe-tile)`
- Then niri: B re-admits after the explicitly focused A with Smart
  follow (mover-active follows with focus B; `focus=false` stays); every
  column activation animates the view to B's column with the minimal fit
  under shipped `Never` (centered only under Always/OnOverflow per the
  focus-scroll policy). `S(S-nir-ins)` + `S(S-nir-ws)` + `S(S-nir-view)` +
  `S(S-nir-base)` + `S(S-nir-wskeys)`.
- Then PaperWM: B re-inserts after the explicitly focused A at the open
  position (selected+1 RIGHT at the shipped default); via the shipped
  take path the drop finalizes with selectedWindow plus
  `Main.activateWindow`, so the return follows with B, while the
  fresh-insertion no-steal on inactive spaces is a different journey.
  Viewport keeps B visible via minimal `ensuredX`/`ensureViewport` scroll
  under DEFAULT. `S(S-pap-ins)` + `S(S-pap-take)` + `S(S-pap-view)`.
- Then karousel/Lazy: B's column re-admits after the target's last
  column (WS1 retains C then A, so after A: `[C,A,B]`); source WS2 is emptied with
  `lastFocusedColumn` nulled and no focus request (no survivor column,
  so the Immediate pass has no recipient). Focus stays B (no karousel
  focus write on this leg; the desktops reassignment itself activates
  nothing). Viewport scrolls to A's column on WS1 via
  `autoAdjustScroll`, using synchronous minimal-scroll to keep that
  column visible. `S(S-kar-ins)` + `S(S-kar-ws)` +
  `S(S-kar-scroll)` + `S(S-kwin-scriptact)`.
- Then paneru: B re-inserts at the remembered strip index for A, else
  the config insertion index, overlap, or end; arrival focus follows the
  `VirtualMoveNumber` verb's `MoveFocus` policy (Follow carries focus to
  B, Stay refocuses the source neighbour), which this fixture leaves
  unstated, so the exact focus stays TBD. `S(S-pan-ins)` + `S(S-pan-ws)`;
  focus queued.
- Then Ours KDE: Remembered A: tall stacked, wide side-by-side; `D(D-place)` synthetic proof, physical feel pending; exact order TBD
- Then Ours Windows: Remembered A: tall stacked, wide side-by-side; `D(D-place)` synthetic proof, physical feel pending; exact order TBD
- Variant hook: V-WS-ANCHOR.

<a id="r-ws-03-backfill-trailing-empty-shortcut-scrolling"></a>
### R-WS-03: trailing-empty shortcut

- Given (tree profiles): WS1 `H[A,B*]`, trailing empty WS exists

- Given (column profiles): `COL[C1[A],C2[B*]]` with a trailing empty
  workspace/strip present; the action names the trailing-empty
  shortcut (`0` target).

- When: Send B via the trailing-empty shortcut (`0` target)

- When (column leg): send B via the trailing-empty parameter.

- Observe: Reuse existing empty vs create another; focus

- Observe (column leg): reuse of the existing empty vs another creation; focus.

- Then COSMIC: Reuses the existing trailing empty (B lands sole; refresh then ensures a fresh trailing empty); `SendToLastWorkspace` leaves focus (falls back to A), `MoveToLastWorkspace` follows with B; numeric `0` is a separate binding (index 9), not the trailing-empty action; `S(S-cos-send)` + `S(S-cos-focusfix)`
- Then Hyprland/Dwindle: Unsupported action parameter here: no trailing-empty shortcut exists in source (workspaces are explicit find-or-create; numeric `0` is an invalid workspace ID, so the `0` send resolves invalid and the move errors with no transfer: tree and focus unchanged). The only empty-related resolver is first-empty `empty`, never trailing-empty. `S(S-hyp-movews)` + `S(S-hyp-ws)`
- Then bspwm: Unsupported action parameter here: no trailing-empty shortcut in source (desktops are explicit-only with explicit `desktop -r` removal), so the trailing-empty/`0` send has no built-in equivalent and never runs (no applicable journey); `S(S-bsp-send)` + `S(S-bsp-ws)`
- Then i3: Unsupported action parameter here: no trailing-empty shortcut in source (`move to workspace number` targets explicit workspaces via `workspace_get` create-on-demand; explicit `0` names workspace `0`, a different target out of scope for reuse, never the trailing empty); the trailing-empty send has no applicable journey and never runs; `S(S-i3-movews)` + `S(S-i3-ws)`
- Then xmonad/Tall+Navigation2D: Unsupported action parameter here: no trailing-empty shortcut in source (workspaces explicit; `shiftWin` to a non-member tag returns the input unchanged, so the trailing-empty/`0` send never runs); `S(S-xmo-shift)`
- Then sway: Unsupported action parameter here: no trailing-empty shortcut in source (`move to workspace number` targets explicit workspaces with find-or-create; `0` names explicit workspace `0`, never the trailing empty), so the trailing-empty send has no applicable journey and never votes here; the explicit-`0` input instead creates workspace `0` and moves B there sole via the ordinary number path (different target, out of scope for reuse). `S(S-sway-movews)`
- Then qtile/Columns: Unsupported action parameter here: no trailing-empty shortcut in source (groups are explicit 1-9; `togroup` takes explicit names only and an unknown group raises before any hide/remove/add, so the `0`/trailing-empty send never runs and tree and focus stay unchanged); `S(S-qti-group)`
- Then awesome/tile: no-applicable-journey: tags are explicit per-screen (1-9, recreated per screen) with explicit `view_only`, and the shipped numrow inventory binds view/move/toggle only with no trailing-empty shortcut or `0` target in source; the `0` send has no applicable tile journey, so tree and focus stay unchanged; `S(S-awe-tag)` + `S(S-awe-keys)` + `S(S-awe-ws)`
- Then niri: no-counterpart (no trailing-empty shortcut exists;
  indices address existing workspaces only and cleanup keeps the
  last). `S(S-nir-acts)`.
- Then PaperWM: no-counterpart (no trailing-empty shortcut in the
  registered action inventory). `S(S-pap-acts)`.
- Then karousel/Lazy: owner-specific (desktops are KWin-native and
  no shortcut verb exists). `S(S-kar-acts)`.
- Then paneru: no-counterpart (no trailing concept exists; explicit
  `VirtualAdd` creates instead). `S(S-pan-cmds)`.
- Then Ours KDE: KDE mapping TBD
- Then Ours Windows: Reuse trailing empty; per-output-local mapping; `D(D-dec-win)` (2026-10-02)
- Variant hook: V-WS-FOLLOW.

<a id="r-ws-04-backfill-memory-invalidation-scrolling"></a>
### R-WS-04: memory invalidation

- Given (tree profiles): WS1 `H[A,B*]`; WS2 `H[C,D]`

- Given (column profiles): `WS1=COL[C1[A],C2[B*]]`, `WS2=COL[C1[C],C2[D]]`.
  On WS2 focus D then C (history D,C); float C to remove the
  remembered leaf; select WS1 and focus B; then send B to WS2.
  No step is reordered: C is floated only after holding focus.

- When: On WS2 focus D then C; float C to remove the remembered leaf; select WS1/focus B; send B to WS2

- When (column leg): the float removal runs first, then the native send.

- Observe: Memory invalidation; surviving D from history vs root; axis/order/follow

- Observe (column leg): surviving anchor for B (history vs sole candidate), float
  exit, target position.

- Then COSMIC: Floated C leaves the tiling tree (D sole); MRU search skips C (no tiling node) and matches D, so B admits at surviving D from history (not root), splits D's long edge (D geometry unrecorded), B after D; `SendToWorkspace` leaves focus (falls back to A), `MoveToWorkspace` follows with B; `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)` + `S(S-cos-focusfix)` + `S(S-cos-send)`
- Then Hyprland/Dwindle: No remembered-leaf or history anchor in source (floated C simply leaves the tiled set); target anchor is sole D regardless of cursor (only tiled candidate on WS2); splits D's long edge (D geometry unrecorded, so axis TBD), B before/after D TBD (cursor half); `S(S-hyp-movews)`
- Then bspwm: no remembered-leaf/history anchor exists in source; floated C
  keeps its tree slot vacant in place with no focus write (so the live WS2
  focus stays C, now floating) and leaves the tiling space geometrically.
  Arrival inserts at that live focus via `transfer_node` (source unlink
  with sibling promotion, insert at `dd->focus`); `insert_node` wraps the
  vacant C first and B second (shipped `SECOND_CHILD` polarity) with the
  longest-side axis read from C's saved rectangle, then
  `propagate_flags_upward` leaves the new parent occupied (vacant needs
  both children vacant); `apply_layout` gives both children of a
  vacant split the full rectangle, so B takes C's full prior slot with D
  unchanged and the recorded axis is geometrically inert while C stays
  floating. Exact axis value stays TBD (F: C's saved rectangle/output
  geometry unrecorded). Shipped `node -d` without `--follow` stays on the
  source, `--follow` follows; `S(S-bsp-xfer)` + `S(S-bsp-state)` +
  `S(S-bsp-float)` + `S(S-bsp-insert)`
- Then i3: `move container to workspace` (no-follow): floated C sits in the WS2 floating list (floating-target fallback), so the anchor is sole D with B after D; source collapses; focus stays A; `S(S-i3-movews)`
- Then xmonad/Tall+Navigation2D: Floated C stays in the stack with focus retained (`float` is a floating-map write only); B arrives via `shiftWin` as `insertUp` above live WS2 focus C (order `[B,C,D]`, B focused there; no floating-leaf split, Tall has no splits, and no history/remembered-leaf anchor exists in source); source collapses to A with source view unchanged (no follow); C remains floating. `S(S-xmo-shift)` + `S(S-xmo-float)`
- Then sway: `move container to workspace` (no-follow): the workspace destination resolves via focus-inactive tiling only, so floated C never anchors; B lands after sole D; source collapses; focus stays A; `S(S-sway-movews)`
- Then qtile/Columns: No remembered-leaf/history anchor in source (floated C leaves the layouts for the floating list); B admits at live WS2 focus via the ordinary anchor; exact order TBD (D geometry and live focus unrecorded); `S(S-qti-group)` + `S(S-qti-add)` + `S(S-qti-float)`
- Then awesome/tile: No remembered-leaf/history admission anchor in source (tile recalc over live tiled order); floated C leaves the tiled set; B admits via the ordinary path with no view switch; exact order/frames TBD; `S(S-awe-tag)` + `S(S-awe-float)` + `S(S-awe-tile)`
- Then niri: floated C leaves the strip into WS2's own floating space
  (each workspace owns its scrolling state, floating space, and active
  flag); floating C via the plain tile-move toggle drops its sole-tile
  column whole with survivor D activating (clamped next), so B admits
  through the ordinary column path at active+1 after sole D with no leaf
  memory to invalidate. Shipped column send follows with Smart
  (`focus=false` stays). `S(S-nir-ws)` + `S(S-nir-ins)` +
  `S(S-nir-close)` + `S(S-nir-flttoggle)` + `S(S-nir-wskeys)`.
- Then PaperWM: anchoring is open-position index only, so no
  remembered-leaf memory exists to invalidate; floated C leaves tiling via
  the scratch path (`toggle-scratch` -> `stick()` fires workspace
  window-removed -> `remove_handler` runs `space.removeWindow`
  synchronously) with neighbor selection (`sortWindows` topmost pick),
  leaving sole D as the live selectedWindow, so B admits after D at
  selected+1 and follows via the shipped take completion
  (`Main.activateWindow`, ordinary convention). `S(S-pap-ins)` +
  `S(S-pap-float)` + `S(S-pap-minimize)` + `S(S-pap-space)` +
  `S(S-pap-take)`.
- Then karousel/Lazy: floating C keeps host focus unchanged (the
  state flip issues no focus write; active stays C until the
  scenario's explicit native select, which resolves B via the MRU
  chain). The explicit verb appends B's column after the target's last
  column (sole D, so after D); the last-focused variant belongs to the
  separate Tiled `desktopsChanged` mover, not this verb. The send
  focuses source survivor A through the script path. The exactly-1
  desktop/activity gate holds for B, else float. `S(S-kar-ins)` +
  `S(S-kar-ws)` + `S(S-kar-float)` + `S(S-kwin-scriptact)` +
  `S(S-kwin-switch)`.
- Then paneru: floated C leaves the WS2 strip via the Floating path
  (strip membership dropped), leaving sole D; B appends after D with no
  remembered-leaf anchor (strip-end append with `insert_windows_mid_strip`
  off). Follow switches the view and focuses B; Stay keeps the source view
  and refocuses A. `S(S-pan-ws)` + `S(S-pan-flt)`.
- Then Ours KDE: Valid focus-history after invalid remembered leaf, then genuine no-focus root; selected `D(D-dec-x)`; scenario result TBD
- Then Ours Windows: Valid focus-history after invalid remembered leaf, then genuine no-focus root; selected `D(D-dec-x)`; scenario result TBD
- Variant hook: V-WS-ANCHOR.

<a id="r-ws-05-backfill-floating-transfer-scrolling"></a>
### R-WS-05: floating transfer

- Given (tree profiles): WS1 `H[A,B*]`, WS2 empty

- Given (column profiles): `WS1=COL[C1[A],C2[B*]]`, WS2 empty; send B to WS2,
  float B there, then request B back on WS1.

- When: Send B to WS2; select WS2/focus B; float B; request send B back to WS1; select WS1

- When (column leg): both transfers run through the profile's native send verb.

- Observe: Whether floating B can transfer; retained float vs fresh tiled admission; focus

- Observe (column leg): whether the floating B transfers; retained float vs fresh
  admission; focus.

- Then COSMIC: Floating B transfers; fresh tiled admission at A (splits A's long edge, B after A), float not retained; `SendToWorkspace` + select focuses A (WS1 MRU; B admitted unfocused), `MoveToWorkspace` focuses B; `S(S-cos-send)` + `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)` + `S(S-cos-focusfix)`
- Then Hyprland/Dwindle: Forward B takes a full-work-area tile box on empty WS2 (sole tiled tile, not maximized mode); floated B transfers retaining float state at monitor-relative position (never fresh-tiled on arrival); the floated return makes no new tiling admission, so no anchor/axis/order applies: sole A stays the unchanged tiled tile; follow/silent focus per `S(S-hyp-movews)`; exact frames TBD; `S(S-hyp-movews)`
- Then bspwm: Forward B sole on WS2 (empty destination focus NULL takes root); floated B transfers retaining float (node moves with client state via `transfer_node`, no fresh tiling; sole A unchanged); shipped send without `--follow` stays on the source, `--follow` keeps focus on B; `S(S-bsp-xfer)` + `S(S-bsp-float)`
- Then i3: `move container to workspace` (no-follow both legs): forward B sole on WS2; floated B transfers as a floating wrapper to WS1 (float retained, no fresh tiling; sole A unchanged); return stays on the now-empty WS2; `S(S-i3-movews)`
- Then xmonad/Tall+Navigation2D: Forward B sole on WS2 (tiled full); floated B transfers retaining float (`shiftWin` runs `delete'`, which preserves the floating-map entry verbatim, so no fresh tiling occurs and B keeps its float frame while sole A keeps its full tile allocation); each arrival is `insertUp` above target focus with B current on that stack and source view unchanged (no follow); X focus follows `setTopFocus` on the current peek (forward leg X focus is source survivor A; the return leg empties WS2 so refresh focuses root until the final select views WS1 with B, focusing B); B renders above the tile (arrange restacks floats first-on-top). `S(S-xmo-shift)` + `S(S-xmo-float)` + `S(S-xmo-arrange)` + `S(S-xmo-topfocus)`
- Then sway: `move container to workspace` (no-follow both legs): forward B sole on WS2; floated B transfers as floating to WS1 (float retained, no fresh tiling; coordinate fix only on output change, same-output leg performs no rewrite; sole A unchanged); return stays on the now-empty WS2; `S(S-sway-movews)`
- Then qtile/Columns: Floated B transfers retaining float (`togroup` `remove` takes the floating path and target `add` re-admits into the floating list from the retained float state, never fresh-tiled; sole A unchanged); the shipped binding follows via `switch_group=True` with mover focus through `toscreen`. The move path writes no resize, only a screen-x offset. `S(S-qti-group)` + `S(S-qti-float)`
- Then awesome/tile: Forward B sole on WS2 (single tile takes the work area; exact pixels TBD (F: work area unrecorded)); floated B transfers retaining float (persistent client property, no fresh tiling; sole A unchanged); no view switch either leg (move_to_tag never switches); the forward send-leg banning clears the hidden mover and the delayed check refocuses the shown source to A, while after the explicit WS2 refocus the floated B keeps focus (no write on the float path); the return send's banning clears focus again with the emptied source offering no visible candidate, so the check writes nothing; the final WS1 select refocuses the now-visible B via history (newest visible); `S(S-awe-tag)` + `S(S-awe-float)` + `S(S-awe-hist)`
- Then niri: B transfers with its floating state carried through the
  remove/add path (never fresh-tiled on arrival) and sole A is
  unchanged; `focus=true` (default) follows with B, `focus=false`
  stays. `S(S-nir-ws)`.
- Then PaperWM: forward B admits sole on WS2; ordinary float maps to
  toggle-scratch (`stick()` plus above plus float flag), so B is stuck on
  all workspaces and already present on WS1: in this journey no take of B
  runs (scratch focus returns without any selection write, so the tiled
  selection is untouched and the shipped selection-verbs take that
  selection, not B) and no fresh-tiled admission occurs (retained scratch;
  dialog `_floating` floats are a different scope). Return-focus on the
  WS1 select has no extension write, so focus stays TBD (host).
  `S(S-pap-ins)` + `S(S-pap-float)` + `S(S-pap-space)` + `S(S-pap-wssel)`;
  focus queued (host).
- Then karousel/Lazy: forward tiled B opens a new column sole on the
  target desktop (appended after the last column) with source fixup and
  source-survivor focus as in R-WS-01. The floating return is a
  tiled-only no-op: `doIfTiledFocused` gates floats out, so no verb
  runs, no `desktops` write is issued, and B remains on WS2 floating
  with focus unchanged. Floating owns no desktop mover. `S(S-kar-ins)` +
  `S(S-kar-float)` + `S(S-kar-ws)`.
- Then paneru: forward B moves via `VirtualMoveNumber` to the indexed row
  (B sole); floating B there leaves the strip via the Floating path, so WS2
  is empty. The return re-inserts B into WS1 after A (strip-end append) but
  retains `Unmanaged::Floating` (the virtual-move path never clears
  `Unmanaged`; only `Manage`/re-manage does), so B is strip-resident but
  still floating with its float frame kept (no resize runs in the move
  path), focusing B. The emptied source forces the Follow path, so
  the Stay run also switches the view and focuses B. `S(S-pan-ws)` +
  `S(S-pan-flt)`.
- Then Ours KDE: ordinary/combo float send delivered offline via slotless deferred transfer, float/size and live overlay retained, destination/source tiles untouched; follow/stay, real numbered/relative routing, sticky refuses, no setting. Same-output exact frame/no geometry write confirmed User 2026-10-13; destination overlay exit remains floating in real-Planner fixtures ([record](../../changes/archive/d16-floating-transfer.md), [decision](../../decisions.md#workspaces)). Native roundtrip/combo exit pending, frames/focus TBD
- Then Ours Windows: adapter delivered offline 2026-10-13 through slotless MoveDeferred, exact float frame/intent retained, deferred follow/stay skip both geometry applications including drifted survivors. Real entry numbered/relative routing, refusals and shared Engine combo/exit regressions pass; combo native readback defers in fake-window fixtures, not successful native carry evidence ([record](../../changes/archive/windows-d16-floating-transfer.md)). Native roundtrip/combo exit pending, frames/focus TBD
- Variant hook: V-FLOAT-GEO.

#### D16 combo and arrival-frame discriminators (2026-10-13)

| Variant | Minimal action sequence | Selected Ours target / offline evidence | Unsupported outcomes |
| --- | --- | --- | --- |
| Same-output frame | Float B on WS1; record frame; send to WS2 and back on the same output; repeat numbered/relative follow/stay | Exact frame retained, no geometry write; ordinary and combo delivery offline | Exact native frame/focus and reference-WM qualified sequence TBD |
| Floating plus overlay | Float B; fullscreen B; explicitly send to WS2; leave fullscreen. Repeat fresh with maximized B and numbered/relative follow/stay | Arrives still floating and fullscreen/maximized, exactly as-is; later exit returns to floating frame. Shared/KDE slotless delivery offline; real shortcut matrix and real-Planner destination refresh prove retention/no admission | Exact reference and native combo/exit outcomes TBD |
| Sticky refusal | Make floating B sticky; fullscreen or maximize B; explicitly send to WS2 | Still refuses with no dispatch/setters; ordinary sticky refusal unchanged | Exact qualified reference/native outcome TBD |

<a id="r-ws-06-backfill-send-to-a-floating-workspace-scrolling"></a>
### R-WS-06: send to a floating workspace

- Given (tree profiles): WS1 tiled `H[A,B*]`; WS2 floating

- Given (column profiles): attempted mapping is WS1 with `COL[C1[A],C2[B*]]` at
  shipped defaults plus a `WS2 floating` target. No scrolling profile has
  a workspace-wide floating mode, so the target parameter has no faithful
  start; the Thens below are applicability qualifications, never an
  ordinary transfer with a substituted target. No send/return journey runs
  on an impossible target.

- When: Send B to WS2; send B back to WS1

- Observe: Native membership/follow, source reflow and floating frame preservation vs two-domain plan

- Observe (column leg): whether the `WS2 floating` target exists natively.

- Then COSMIC: Forward B arrives floating reusing its last tiled origin with clamped size (exact frame TBD); source reflows; return is fresh tiled admission at A (B after A); `SendToWorkspace` leaves focus (source-MRU fallback each leg), `MoveToWorkspace` follows with B; `S(S-cos-send)` + `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)` + `S(S-cos-focusfix)`
- Then Hyprland/Dwindle: Forward TBD: no workspace floating mode exists in source to map `WS2 floating` onto (arrival dispatch is per-window by the mover's own float state, which this fixture never changes), so no floating arrival is recorded; return anchor is sole A (A geometry unrecorded, so axis TBD), B before/after A TBD (cursor half); exact frames TBD; `S(S-hyp-movews)`
- Then bspwm: no-counterpart (no workspace floating mode exists in source: float is per-window and desktop layout is tiled/monocle only, so the forward send to a floating workspace never runs and the return leg is conditional on that unestablished forward); `S(S-bsp-float)` + `S(S-bsp-layout)`
- Then i3: No workspace-mode counterpart in source (no workspace floating mode exists: float is per-window; `workspace_layout` default/stacked/tabbed only), so the forward send to a floating workspace never runs and the return leg, conditional on that unestablished forward, never runs either; no applicable journey; `S(S-i3-wsmode)` + `S(S-i3-movews)`
- Then xmonad/Tall+Navigation2D: no workspace-mode counterpart (no workspace floating mode in source: float is per-window with layout Tall/Mirror/Full plus floating layer only, so the forward/return send never runs); `S(S-xmo-float)` + `S(S-xmo-layout)`
- Then sway: No workspace-mode counterpart in source (no workspace floating mode exists: float is per-window; `workspace_layout` default/stacked/tabbed only), so the forward send to a floating workspace never runs and the return leg, conditional on that unestablished forward, never runs either; no applicable journey. `S(S-sway-wsmode)`
- Then qtile/Columns: no-counterpart (no workspace floating mode exists in source: float is per-window and Columns always tiles plus a floating layer, so the `WS2 floating` target has no faithful start and no floating-workspace transfer journey ever runs; an ordinary `togroup` would admit B tiled via the ordinary anchor, which substitutes a different target and never votes here); `S(S-qti-wstoggle)` + `S(S-qti-group)`
- Then awesome/tile: WS2 floating reads as the shipped floating layout on that tag (layout is per-tag via `layout.set`); forward B arrives unarranged (floating arrange no-op, incoming geometry kept); return re-admits via tile partition over retained global order; no view switch either leg; exact pixel frames TBD (F: work area unrecorded); `S(S-awe-tag)` + `S(S-awe-layout)` + `S(S-awe-float)`
- Then niri: fixture-inapplicable (no workspace floating mode exists to
  construct WS2 with; `ToggleWindowFloating` is per-window only and
  `floating_is_active` derives from admission/focus, not a mode).
  `S(S-nir-float)`.
- Then PaperWM: fixture-inapplicable (no floating workspace mode and no
  workspace toggle in the registered action inventory). `S(S-pap-acts)`.
- Then karousel/Lazy: fixture-inapplicable (float is per-window only; no
  floating desktop mode). `S(S-kar-acts)`.
- Then paneru: fixture-inapplicable (no floating workspace mode; `Manage`
  is per-window and the tier flip is focus-only). `S(S-pan-cmds)`.
- Then Ours KDE: membership-only boundary send, only tiled side reflows,
  floating frames untouched. Default follows with verified arrival/switch/focus
  readback; explicit stay preserves source view/native boundary focus. Item 2
  implemented offline; physical feel/native journey pending.
  [Fixtures](../../../kwin/tests/workspace-send-follow-stay.test.ts),
  [record](../../changes/archive/kde-workspace-send-follow-stay.md); `D(D-dec-ww)`.
- Then Ours Windows: agent-observed Notepad numbered follow/stay across
  both floating boundaries preserves floating frames, reflows only tiled
  sides and keeps source/native focus for stay. Native gates passed
  2026-10-11, base `553c65b` plus
  [delivery record](../../changes/archive/windows-workspace-send-follow-stay.md).
  Prior [Paint proof](../../changes/archive/windows-workspace-tiling.md)
  retained; physical feel/chords TBD.
- Variant hook: V-WS-FOLLOW.

<a id="r-ws-07-backfill-shell-switcher-listing-scrolling"></a>
### R-WS-07: shell switcher listing

- Given (tree profiles): WS1 `H[A,B*]`, WS2 `H[C*]` currently shown; KDE switcher includes all desktops

- Given (column profiles): `WS1=COL[C1[A],C2[B*]]`, `WS2=COL[C1[C*]]` shown.
  Use niri's default All MRU scope; Workspace-only is a separate variant.

- When: Select B in Alt+Tab

- When (column leg): select B in the profile's window switcher.

- Observe: B listed vs omitted; switch to WS1 with B membership unchanged vs pull B into WS2

- Observe (column leg): B listed vs omitted; switch to WS1 with membership
  unchanged vs pull into WS2.

- Then COSMIC: B listed: the Alt+Tab empty-query search appends every compositor-published toplevel with no workspace/visibility filter; selecting B calls `manager.activate`, and the compositor unminimizes B, switches to WS1 via `shell.activate`, and focuses B with membership unchanged (never pulled into WS2; sticky windows focus in place); `S(S-cos-sysact)` + `S(S-cos-syscmd)` + `S(S-lch-altab)` + `S(S-pop-toplevel)` + `S(S-cos-topact)`; exact switcher visuals/key-repeat timing TBD
- Then Hyprland/Dwindle: no-counterpart (no Alt+Tab/listing verb in the dispatcher inventory: `cycle_next` cycles the same workspace by default, `focus last`/`urgent_or_last` are history single-targets, and the shipped example binds an external `hyprlauncher` menu). `S(S-hyp-switcher)`
- Then bspwm: no-counterpart (no Alt+Tab/listing verb in source or the shipped sxhkdrc: `super+Tab` toggles the last desktop, `grave` the last node, `o/i` walk history single-targets, and `{next,prev}.local` cycles the current desktop only; `dmenu_run` is an external program launcher and `bspc query` lists have no traced switcher activation). `S(S-bsp-switcher)`
- Then i3: no-counterpart (no Alt+Tab/listing verb in the FOCUS command inventory or shipped config: `dmenu_run` plus a commented `rofi` alternate are external launchers, and focus verbs are directional/output/mode/parent-child single-targets while workspace number/next/prev switch views without listing). `S(S-i3-switcher)`
- Then xmonad/Tall+Navigation2D: no-counterpart (no Alt+Tab/cross-workspace listing verb in this profile: core mod+Tab is same-stack `focusDown`, `dmenu_run`/`gmrun` are external launchers, and Navigation2D `windowGo`/`windowSwap` stay on the same layer). `S(S-xmo-switcher)`
- Then sway: no-counterpart (no Alt+Tab/listing verb in the focus command inventory or shipped config: `$menu wmenu-run` is an external launcher, and focus verbs are directional/output/mode/parent-child single-targets with no listing). `S(S-sway-switcher)`
- Then qtile/Columns: no-counterpart (no Alt+Tab/listing verb in the shipped keys: mod+Tab switches layouts, mod+space and `next/prev_window` cycle the current group only, and the bar `WindowName`/`TaskList`/`WindowTabs` widgets display the current group only; `toscreen` pulls a group with no listing). `S(S-qti-switcher)`
- Then awesome/tile: B is listed by the native `awful.menu.clients` inventory (unfiltered `client.iterate` over all screens with no tag filter, exposed as `client_list` on the shipped tasklist right-click; the shipped mod+Tab key is history-previous, not a listing, and the tasklist shows current tags only); activation views B's tags via `viewmore` with membership unchanged (no tag write to B) plus `activate raise`. `S(S-awe-switcher)`
- Then niri: B is listed in the default All scope (the MRU UI collects
  every workspace's windows sorted by focus timestamp; Alt+Tab/Mod+Tab
  binds ship by default with `recent_windows` on); confirming B runs
  `activate_window`, which switches to WS1 and focuses B with membership
  unchanged (no window move in the path). `S(S-nir-mru)`
- Then PaperWM: B is listed by the native `live-alt-tab` verb (shipped
  Alt+Tab/Super+Tab defaults; `_getWindowList` uses `NORMAL_ALL` minus
  scratch) when the external GNOME `current-workspace-only` setting is
  off, and omitted when it is on (no pinned GNOME default traced); the
  accept path runs the shell popup `_finish` then `focus_handler` with
  no take/move, so membership is unchanged, while the workspace-switch
  effect rides the unpinned shell activation path and stays TBD.
  `S(S-pap-switcher)`; switch queued.
- Then karousel/Lazy: owner-specific (the KWin switcher owns listing
  and activation; no switcher verb in the Actions inventory, whose focus
  verbs stay on the current grid). Under the row's all-desktops KDE
  switcher (the alternative TabBox mode `AllDesktopsClients` lists every
  desktop; the shipped default lists the current desktop only, so B is
  omitted there; karousel `skipSwitcher` defaults false so B is not
  excluded), B is listed; accept runs `Workspace::activateWindow`,
  default `SwitchToOtherDesktop` switches to WS1 with membership
  unchanged, alternative `BringToCurrentDesktop` pulls B into the current
  desktop. `S(S-kwin-tabbox)` + `S(S-kar-acts)` +
  `S(S-kar-switcher)`.
- Then paneru: owner-specific (the macOS switcher owns listing and
  activation; no cross-strip listing op exists: `ToggleFloatingLayer`
  flips only the active workspace's floating/tiled tiers); B
  listing/activation TBD (no host switcher source traced).
  `S(S-pan-cmds)` + `S(S-pan-switcher)`.
- Then Ours KDE: Per the KDE source, native filter permits B; TabBox activation follows configured policy, default switch to WS1, alternative bring-to-current; exact user-version live outcome TBD; `D(D-alt-tab)`
- Then Ours Windows: Current `SW_HIDE`: B omitted; future inclusion/activation policy TBD; `D(D-alt-tab)`
- Variant hook: V-WS-SHELL-ACTIVATE.

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours cells cite Engine + adapter code at `60771bd`
(`S(S-ours-ws)` for workspace mechanics, `S(S-ours-planops)` for the
verb inventory); selected intent and doc assertions are never evidence.

### R-WS-08: back-and-forth workspace twice

- Given (tree profiles): occupied WS1, WS2, WS3; visit WS1, then WS2,
  then WS3 (WS3 shown). Ordinary windows, no rules, shipped defaults
  unless the back-and-forth verb itself needs enabling (named per
  profile; Hyprland `binds:workspace_back_and_forth` defaults off).
- Given (column profiles): same three-workspace visit with
  `COL[C1[A]]`-style single columns per workspace at shipped defaults;
  paneru uses one native Space with virtual rows VW1..VW3. The
  Hyprland Then names its enabling variant explicitly; that variant
  never votes as a shipped-baseline outcome.
- When: invoke the profile's back-and-forth / previous-workspace verb
  twice from fresh switch state (native verbs named per profile below).
  A missing verb is not a no-op.
- Observe: which workspace is shown after each press; last-view toggle
  between two vs MRU-list traversal vs absent verb.
- Then COSMIC: no-counterpart (no history-toggle verb exists; switching
  to the current workspace is a plain re-activate, and `LastWorkspace`
  targets the last index `len-1`, not the last-viewed workspace).
  `S(S-cos-ws)`.
- Then Hyprland/Dwindle: single previous-view toggle once enabled
  (`binds:workspace_back_and_forth=1`): re-invoking switch-to-current
  gives WS2 then WS3. Shipped default 0 only re-activates WS3;
  `=2` restricts the previous lookup to this monitor.
  `S(S-hyp-ws)`.
- Then bspwm: last-view toggle via the `last` desktop selector
  (`desktop -f last`): first press shows WS2, second returns to WS3.
  `S(S-bsp-ws)`.
- Then i3: single previous-name toggle (`workspace back_and_forth`
  shows `previous_workspace_name`, refreshed on every switch): first
  press shows WS2, second returns to WS3. `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no back-and-forth or
  previous-workspace verb in the core key inventory; directional verbs
  are Navigation2D only). `S(S-xmo-ctl)`.
- Then sway: single previous-name toggle (`workspace back_and_forth`
  via `prev_workspace_name`, plus the auto variant): first press shows
  WS2, second returns to WS3. `S(S-sway-ws)`.
- Then qtile/Columns: previous-group toggle (`toggle_group()` with no
  name falls back to `previous_group`, saved on every `set_group`):
  first press shows WS2, second returns to WS3. `S(S-qti-ws)`.
- Then awesome/tile: previous-set toggle (`tag.history.restore()`
  defaults to the special `"previous"` index swapping the last two
  selected sets): first press shows WS2, second returns to WS3.
  `S(S-awe-ws)`.
- Then niri: single previous-id toggle (`FocusWorkspacePrevious` /
  `switch_workspace_previous` goes to `previous_workspace_id`, stored
  on every activate): first press shows WS2, second returns to WS3;
  `switch_workspace_auto_back_and_forth` is the same single-toggle
  variant. `S(S-nir-ws)`.
- Then PaperWM: MRU-list traversal with wrap, not a two-state toggle
  (`previous-workspace` steps DOWN an MRU stack and wraps around, so a
  double press keeps walking down the list). `S(S-pap-space)`.
- Then karousel/Lazy: no-counterpart (no desktop-switch or history
  verb in the Actions inventory; desktop switching is KWin-native).
  `S(S-kar-acts)`.
- Then paneru: no-counterpart (no history verb in the `Operation`
  inventory; `Virtual` is directional North/South only).
  `S(S-pan-cmds)`.
- Then Ours KDE: WS2 then WS3, stable-ID two-view toggle via Meta+Ctrl+Tab;
  implemented; single-output native journey user-confirmed 2026-10-07
  ("worked perfectly"). Individual cases/presets were not specified;
  multi-output/hotplug/preset checks remain pending. Scoped observed-change history in
  [adapter](../../../kwin/src/workspace-native.ts),
  [fixtures](../../../kwin/tests/workspace-previous-relative.test.ts),
  [record](../../changes/archive/kde-workspace-history-ring.md).
- Then Ours Windows: WS2 then WS3 via Win+Ctrl+Tab in portable local
  observed-history tests; delivered offline 2026-10-10, base `91db9d2` plus
  [delivery record](../../changes/archive/windows-workspace-history-ring.md).
  Native exact-owner CLI toggle WS2 then WS3 agent-observed 2026-10-11
  at base `553c65b` plus [item-2 record](../../changes/archive/windows-workspace-send-follow-stay.md).
  Physical chord result TBD; non-local/multi-output pending.
- Variant hook: provisional/TBD (history-toggle hook, to discuss).

### R-WS-09: select WS2; select WS1 (return focus and viewport)

- Given (tree profiles): `WS1=H[A,B*]`, `WS2=H[C]`; WS1 history A,B
  (focus A then B before leaving). Ordinary windows, no rules,
  shipped defaults.
- Given (column profiles): `WS1=COL[C1[A],C2[B*]]` each 0.5W with the
  viewport showing both columns; `WS2=COL[C1[C]]`. Paneru:
  `Space1:{VW1=COL[C1[A*]],VW2=COL[C2[B]]}`-style rows are not this
  fixture; use two virtual rows holding A-then-B history on row 1 and C
  on row 2.
- When: select WS2; select WS1 (the profile's native workspace
  switch, named per profile below). No sends and no refocus between
  the switches, unlike R-WS-02.
- Observe: focused window on return (remembered B vs first/master/
  root); column profiles additionally observe the saved viewport.
- Then COSMIC: focuses B (per-workspace MRU focus stack; the valid
  MRU-last target wins, else the first mapped). `S(S-cos-focusfix)`.
- Then Hyprland/Dwindle: pointer-dependent (tiled remembered B is not
  consulted: `getLastFocusedWindow` feeds focus only when floating, else
  the fullscreen cover; `input:follow_mouse=1` pointer-hit wins before
  the focus candidate, and the pointer fixture is unspecified, so B vs
  pointer-hit window stays TBD). Named `input:follow_mouse=0` variant
  focuses B via `getFocusCandidate` (last-focused, else top-left, else
  first). `S(S-hyp-ws)`; pointer-position queued.
- Then bspwm: focuses WS1's remembered `d->focus` (B) via the
  focus/history fallback. `S(S-bsp-ws)`.
- Then i3: focuses the descended remembered focus of WS1 (B).
  `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: each workspace keeps its own Stack
  focus, so `view` back restores WS1 with B focused. `S(S-xmo-ws)`.
- Then sway: focuses the seat focus-inactive node of WS1 (B); empty
  workspaces fall back to the workspace node. `S(S-sway-switch)`.
- Then qtile/Columns: `layout_all` focuses the group's remembered
  `current_window` (B) on the current screen. `S(S-qti-ws)`.
- Then awesome/tile: tag-switch refocus prefers history (B) with a
  sticky fallback. `S(S-awe-hist)`.
- Then niri: focus resolves to the newly active workspace's active
  window (B); each workspace owns its ScrollingSpace, so the saved
  viewport returns too. `S(S-nir-ws)`.
- Then PaperWM: the space retains `selectedWindow` (B), but native
  `workspace.activate` carries no focus target, so the GNOME-side
  restore on return stays TBD. `S(S-pap-space)`; return-focus queued.
- Then karousel/Lazy: mixed (the switch itself is KWin-native;
  karousel only re-arranges and tracks activation per-grid). Native
  select runs `setCurrent` into visibility plus
  `activateWindowOnDesktop`: shipped ClickToFocus is reasonable and
  `NextFocusPrefersMouse` is false, so the MRU focus-chain decides
  with no mouse contest on the single output. WS1 MRU is B (A-then-B
  history), so the return focuses B with membership unchanged;
  karousel records it in `lastFocusedColumn` and scrolls to B's column,
  already contained in the both-visible view, so the minimal scroll is
  a no-op and the viewport is unchanged. `S(S-kar-ws)` +
  `S(S-kwin-switch)`.
- Then paneru: refocuses the restored strip's remembered window with
  a restore guard (a never-focused strip falls back to the column
  closest to the display centre); the strip keeps its saved origin.
  `S(S-pan-ws)`.
- Then Ours KDE: mixed (no select verb exists, so the switch journey
  is shell-driven; the Engine keeps per-domain `last_active`, but
  native focus on a shell-driven return stays TBD).
  `S(S-ours-planops)` + `S(S-ours-ws)`; return-focus queued.
- Then Ours Windows: focuses the remembered `last_focus` member (B),
  else the first visible member, via `focus_target` on select.
  `S(S-ours-ws)`. Windows adapter regression 2026-10-11 (base `94fa4ef`
  plus delivery commit) records verified departure foreground, including
  retained fullscreen members, before numbered/history/CLI selects; offline
  return-target and no-fullscreen-geometry coverage passed. Native game/fixture
  outcome remains pending ([record](../../changes/archive/windows-workspace-fullscreen-focus.md)).
- Variant hook: provisional/TBD (return-focus hook, to discuss).

### R-WS-10: send B away; empty middle retained vs removed

- Given (tree profiles): occupied WS1, WS2, WS3; shown `WS2=H[B*]`.
  Ordinary windows, no rules, shipped defaults.
- Given (column profiles): `WS2=COL[C1[B*]]` shown; WS1 and WS3
  occupied single-column workspaces (paneru: virtual rows VW1..VW3
  with B alone on VW2).
- When: send B to WS1 (the profile's native send, follow variant
  noted per profile); select WS3.
- Observe: emptied middle workspace retained vs removed/renumbered;
  active-empty protection vs immediate cleanup.
- Then COSMIC: removed (non-active non-last empties are removed while
  a trailing empty is ensured, so the middle collapses out).
  `S(S-cos-send)`.
- Then Hyprland/Dwindle: removed. Numbered IDs never renumber (duplicates
  refused, no renumber path); no ordinary empty-destroy verb exists (only
  special close-on-empty), and empty lifetime is refcount: the state list and
  history entries hold weak refs, member windows hold strong refs, the monitor
  holds the active workspace, and only a persistent rule self-holds. The
  fixture carries no persistent rule, so once B leaves and the switch away
  drops the monitor hold, the emptied middle is destroyed (retained while
  active-empty via the monitor hold); the freed number is reusable via
  find-or-create. `S(S-hyp-ws)` + `S(S-hyp-close)`
- Then bspwm: retained (desktops persist until the explicit
  `desktop -r`; emptiness never auto-removes). `S(S-bsp-ws)`.
- Then i3: removed (the empty non-visible old workspace is closed on
  the switch away). `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: retained (the configured workspace
  list is static; StackSet never removes entries). `S(S-xmo-ws)`.
- Then sway: removed (`workspace_consider_destroy` drops empty
  non-active workspaces). `S(S-sway-ws)`.
- Then qtile/Columns: retained (static groups 1-9; emptiness never
  removes). `S(S-qti-wsdef)`.
- Then awesome/tile: retained (static per-screen tags; only the
  explicit `tag.delete` removes). `S(S-awe-ws)`.
- Then niri: removed (cleanup drops empty non-active workspaces except
  the trailing one, so later indices shift; the
  `empty_workspace_above_first` option changes the kept end).
  `S(S-nir-ws)`.
- Then PaperWM: owner-specific (GNOME owns workspace add/remove;
  PaperWM only mirrors via `workspacesChanged`). `S(S-pap-space)`.
- Then karousel/Lazy: owner-specific (KWin owns desktops; karousel
  only tracks them). `S(S-kar-ws)`.
- Then paneru: retained at shipped defaults (`reap_empty_workspaces`
  defaults off and index 0 is never reaped; enabling it despawns empty
  non-active rows). `S(S-pan-ws)`.
- Then Ours KDE: the adapter adopts backing desktops as managed and removes
  eligible non-trailing invisible empties, protecting occupied, visible,
  flight-pinned, displaced and unmapped desktops and the minimum count;
  it ensures a structural trailing spare. This is the selected
  [workspace lifecycle](../../decisions.md#workspaces),
  implemented by `kwin/src/workspace-native.ts` and covered by its lifecycle
  fixtures. The exact send-then-select native outcome remains TBD; the
  [partial two-output pass](../../changes/kde-dual-output-live.md) observed
  startup retirement, not this action sequence. `S(S-ours-ws)`.
- Then Ours Windows: emptied-middle removal route exists (corrected
  2026-10-08 at `db31234`): `crates/tiler-windows/src/workspace.rs:508`
  `apply_cleanup` removes stable IDs, invoked from retirement cleanup at
  `crates/tiler-windows/src/tiling_sys.rs:9228-9234` and selection at :10074-10080;
  `crates/tiler-core/src/workspace.rs:35` removes eligible invisible
  empties, preserving the trailing spare and minimum count. Eligible invisible
  empties can be removed; visible, occupied, or retained-policy-excluded ids
  stay. Whether this exact given (emptied
  middle WS2 after a send, then select WS3) retains or removes on Windows
  is TBD; no live acceptance recorded here. `S(S-ours-ws)`.
- Variant hook: provisional/TBD (empty-workspace lifecycle hook).

### R-WS-11: next workspace; previous workspace

- Given (all profiles): occupied WS1, WS2, WS3 with WS3 shown; other
  outputs absent. Trailing inventory is profile-native and part of the
  fixture: COSMIC and niri keep a trailing empty workspace, qtile and
  awesome carry static 1-9 inventories (4-9 empty); bspwm/i3/sway hold
  exactly WS1..WS3. Shipped defaults (COSMIC `workspace_wraparound`
  defaults true; qtile skip options default off).
- Given (column profiles): same three workspaces as single-column
  strips; paneru uses virtual rows VW1..VW3 in one Space.
- Given (edge leg, fresh reset per profile): the faithful native last
  inventory (COSMIC/niri trailing-last workspace; qtile group 9;
  awesome tag 9; bspwm/i3/sway last list entry) shown, then next;
  plus the first entry shown, then previous. bspwm/i3/sway have no
  trailing inventory, so their primary leg already is the edge.
- When: primary leg next from WS3, then previous back (the profile's
  native relative switch, named per profile); edge leg per the Given
  above. Hyprland's `e+1`/`e-1` ordered-index variant is independent
  and never substituted for plain next/previous.
- Observe: primary landing (existing empty vs wrap vs create) and the
  return; edge wrap vs stop/clamp/create. Wrap consensus is scored on
  the edge leg only, never on the mid-inventory primary.
- Then COSMIC: primary next lands on the existing trailing empty WS4,
  previous returns to WS3; edge wraps at the shipped default (last next
  goes to WS1, first previous to the last), else stays with output
  fallback. `S(S-cos-ws)`.
- Then Hyprland/Dwindle: plain next goes to numeric+1 (WS4 via
  find-or-create) and plain previous goes to the MRU-history previous
  (back to WS3 here); no wrap on either verb at any index.
  `S(S-hyp-ws)`.
- Then bspwm: wraps with no trailing stop (`CYCLE_DIR` next/prev walk
  the circular desktop list, so WS3 next goes to WS1 and previous
  returns to WS3; primary and edge coincide). `S(S-bsp-ws)`.
- Then i3: wraps with no trailing stop (`workspace next`/`prev` fall
  back to the first/last workspace, so WS3 next goes to WS1 and
  previous returns to WS3; primary and edge coincide). `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no relative-
  workspace verb in the core key inventory). `S(S-xmo-ctl)`.
- Then sway: wraps with no trailing stop (same first/last fallback,
  WS3 next goes to WS1 and previous returns to WS3; primary and edge
  coincide). `S(S-sway-ws)`.
- Then qtile/Columns: primary next lands on existing group 4 and
  previous returns to group 3; edge wraps modulo the static list
  (group 9 next goes to group 1). `S(S-qti-ws)`.
- Then awesome/tile: primary next lands on existing tag 4 and
  previous returns to tag 3; edge wraps with cycling (tag 9 next goes
  to tag 1). `S(S-awe-ws)`.
- Then niri: primary next lands on the existing trailing empty WS4
  and previous returns to WS3; edge stops at both ends (up saturates
  at 0, down clamps at the last index; gesture DnD clamps the same
  way). `S(S-nir-ws)`.
- Then PaperWM: primary next lands on the adjacent existing space when
  present, else stays at the actual last; edge stops at the actual
  first/last (out-of-range sequence steps return; the looping variants
  are column-level, not workspace). `S(S-pap-space)`.
- Then karousel/Lazy: no-counterpart (no desktop-switch verb in the
  Actions inventory). `S(S-kar-acts)`.
- Then paneru: South steps or auto-creates at the last row (VW3 South
  creates VW4 when the strip is non-empty and `create_workspace_automatically`
  is on, shipped default off, else stays; North from a created VW4 returns
  to VW3) while North saturates at the first row; never a cyclic wrap.
  `S(S-pan-ws)`.
- Then Ours KDE: next from WS3 selects existing trailing empty, previous
  returns to WS3; last next wraps to first, first previous to last. Scoped
  existing order includes ordinals beyond 9; selection creates nothing.
  H/K/Left/Up previous, J/L/Down/Right next, all Meta+Ctrl. Authentic clears
  stock KWin desktop-switch arrows through confirmed shortcut application;
  Compatible disables our arrows, letters remain. Single-output native journey
  user-confirmed 2026-10-07 ("worked perfectly"); individual edge/>9 cases and
  presets unspecified, multi-output/presets pending. [Adapter](../../../kwin/src/workspace-native.ts),
  [fixtures](../../../kwin/tests/workspace-previous-relative.test.ts),
  [reconciler](../../../kwin/native-effect/shortcutreconciler.cpp),
  [record](../../changes/archive/kde-workspace-history-ring.md).
- Then Ours Windows: local Win+Ctrl+H/K/Left/Up previous,
  J/L/Down/Right next; full existing ring wraps including trailing empty and
  >9 without creation. Compatible disables only Left/Right. Portable tests
  and native offline gates passed 2026-10-10, base `91db9d2` plus
  [delivery record](../../changes/archive/windows-workspace-history-ring.md).
  Native CLI wrap/trailing/>9 in an 11-ID ring and native Settings outcomes
  agent-observed 2026-10-11, base `553c65b` plus
  [item-2 record](../../changes/archive/windows-workspace-send-follow-stay.md).
  Physical chord/preset OS outcomes TBD; non-local/multi-output pending.
- Variant hook: provisional/TBD (relative-switch hook, to discuss).

### R-WS-12: move whole WS2 to R

- Given (all profiles): L shows WS1; occupied WS2 belongs to L but is
  hidden; R shows WS3. Ordinary windows, no rules, shipped defaults.
  The hidden baseline is kept everywhere: no profile may pre-select
  WS2 to satisfy an active-only verb. Active-only verbs therefore get
  an evidenced no-counterpart on the fixture plus one independent
  explicitly-selected active leg each.
- Given (column profiles): same output/workspace assignment with
  single-column strips; karousel has no second output in its
  single-screen profile, so its Then is a fixture qualification;
  paneru's native Space and virtual rows are separate domains.
- When: move the whole WS2 to R (the profile's native
  workspace-to-output verb, named per profile below). Window-only
  sends never substitute.
- Observe: whole-workspace reassignment vs view switch or window-only
  transfer; displaced destination view; focus.
- Then COSMIC: mixed (the fixture verb `MigrateWorkspaceToOutput`
  moves the active workspace only, so the hidden WS2 has
  no-counterpart on this fixture; an independent leg with WS2
  explicitly selected migrates after R's active, activates and switches
  output there (L keeps showing WS1, R shows the migrated WS2), while
  the moved-active focus node stays TBD (F: WS2 contents/focus history
  unstated, so which retained focus-stack entry the SwitchOutput path
  focuses is unspecified). `S(S-cos-ws)` + `S(S-cos-wsmig)`;
  focus queued.
- Then Hyprland/Dwindle: whole-workspace reassignment via
  `moveToMonitor`: WS2 was never active on L, so the source gap-plug is
  skipped (L keeps showing WS1) and the carryFocus activation is skipped too
  (R keeps showing WS3 with WS2 present hidden); members carry with floating
  reposition and fullscreen setBox (no pinned members in the fixture; the
  pinned branch self-assigns on this non-active leg since the plug workspace
  is the mover itself). Focus
  unchanged (no focus write anywhere in the non-active path).
  `S(S-hyp-ws)` + `S(S-hyp-wsmove-fs)`
- Then bspwm: whole-desktop reassignment via `desktop -m MONITOR` (`transfer_desktop` unlink/append-insert; the hidden WS2 move leaves L showing WS1 and R showing WS3 with WS2 present hidden, no focus write on either follow leg since the moved desktop was not active); `S(S-bsp-ws)`
- Then i3: whole-workspace detach/attach via the matched-window form `[workspace="^WS2$"] move workspace to output R` (criteria targeting walks all cons including hidden, so hidden WS2 resolves; the bare current-workspace invocation moves the focused workspace only and never substitutes); L keeps showing WS1 and R keeps showing WS3 with WS2 present hidden (the hidden move path issues no `workspace_show`, so destination and source focus are unchanged at workspace level). `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: no-counterpart on the ownership
  fixture (no workspace-ownership move verb exists); independently,
  `greedyView` reassigns display/view with a hidden swap, never an
  ownership vote. `S(S-xmo-ws)`.
- Then sway: whole-workspace move via the criteria-targeted form
  `[workspace="WS2"] move workspace to output R` (matched-window
  targeting iterates WS2's containers over all outputs incl hidden, so
  handler workspace resolves to hidden WS2; the bare current-workspace
  invocation never substitutes): detaches/attaches the same workspace
  object via `workspace_move_to_output` (source refilled with a fresh
  workspace when last, displaced consider-destroyed only if
  empty/non-active/unreferenced) and focuses the moved workspace's
  focus-inactive node. Exact focus node and displaced views stay TBD
  (F: WS2/WS3 contents and focus history unstated). `S(S-sway-ws)`;
  focus/views queued.
- Then qtile/Columns: shared group/view-ownership reassignment via
  `toscreen` on R (a swap runs only when the group already had a
  screen; WS3 is unscreened and hidden, so R takes WS2 while WS3 hides
  and L keeps showing WS1; this is view ownership, not a
  container-tree move, and is labeled as such); exact focus stays TBD
  (F: current-screen/current-window history unstated, and the
  `set_group`/`toscreen` path writes no focus beyond `layout_all`).
  `S(S-qti-ws)`; focus queued.
- Then awesome/tile: shared tag/view-ownership reassignment via
  `tag.screen` (moves the tag plus all member clients with screen plus tags rewritten; the moved tag was hidden, so no old-screen history restore runs and L keeps showing WS1; labeled as view ownership, not a tree move); destination selection untouched so R keeps showing WS3 with the unselected WS2 present; exact focus stays TBD (F: client inventory and initial focus unstated, and the move path writes no focus). `S(S-awe-ws)`.
- Then niri: whole-workspace reassignment via
  `MoveWorkspaceToMonitorByRef` (explicit output-plus-reference
  resolves the hidden WS2; hidden move inserts after R's active entry
  with no activation, so displaced WS3 stays shown; the active-workspace
  variant activates the target). `S(S-nir-ws)`.
- Then PaperWM: hidden WS2 has no-counterpart on this fixture (whole-space
  verbs take no space argument; they walk the live stack from the
  active/selected space only). Independent active leg (WS2 explicitly
  selected): `move-space-monitor` runs the stack dance (`selectStackSpace`
  DOWN over `[activeSpace, ...stack-minus-shown]` with wrap, navigator
  finish activating the selected space with its retained selectedWindow,
  `switchMonitor` to the neighbor; `lteSpaces` notify-and-stay; last-on
  monitor swap fallback; `-1` neighbor stays) and reassigns the same space
  object via `setMonitors`/`setMonitor` (no column rewrite, so
  columns/order/shares/selection carry). Exact displaced views and focus
  hinge on the unstated live stack/tab order (stack seeded from `mru()`:
  active plus `NORMAL_ALL` tab-list plus index order) and the unpinned host
  neighbor/index order, so they stay TBD. `S(S-pap-space)` + `S(S-pap-mon)`
  + `S(S-pap-wssel)`; views/focus queued (fixture + host).
- Then karousel/Lazy: fixture-inapplicable (single-screen profile has
  no second output to receive WS2). `S(S-kar-base)`.
- Then paneru: no-counterpart (`ToNextDisplay` moves the focused
  window only, never a whole strip or Space). `S(S-pan-cmds)`.
- Then Ours KDE: mixed (the hidden WS2 has no-counterpart on this
  fixture: the four directional follow-only verbs act on the active
  workspace only; an independent leg with WS2 explicitly selected
  migrates with its stable backing id retained, implemented offline,
  native journey TBD; NORMATIVE D1-D9, User 2026-10-08, D8 carry
  delivered offline 2026-10-09). [Engine](../../../crates/tiler-core/src/engine.rs),
  [adapter](../../../kwin/src/workspace-send-adapter.ts),
  [native map](../../../kwin/src/workspace-native.ts),
  [fixtures](../../../kwin/tests/workspace-migrate.test.ts),
  [record](../../changes/archive/kde-whole-workspace-output-migration.md).
- Then Ours Windows: no-counterpart (index `Select`/`Send` only; no
  whole-workspace verb). `S(S-ours-planops)`.
- Variant hook: NORMATIVE workspace-output migration (User 2026-10-08;
  D8 carry delivered offline).

### R-WS-13: select absent WS9

- Given (all profiles): WS1 occupied; the target name/ordinal WS9 is
  absent (qtile/awesome carry a fixture difference noted below:
  their shipped static 1-9 inventories cannot construct an absent
  WS9). Shipped defaults.
- Given (column profiles): same single-workspace start as strips;
  paneru holds one Space with a single virtual row.
- When: select WS9 (the profile's native select by index/name).
- Observe: create/select vs refusal/no-op; static inventories vs
  dynamic creation. Never infer from a send.
- Then COSMIC: refuses (activate with idx past the end returns
  `InvalidWorkspaceIndex`; no creation path). `S(S-cos-ws)`.
- Then Hyprland/Dwindle: creates and selects (the switch path runs
  find-or-create). `S(S-hyp-ws)`.
- Then bspwm: refuses (an absent selector fails instead of creating).
  `S(S-bsp-ws)`.
- Then i3: creates and selects (`workspace_get` creates on demand).
  `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: no-op (`view` of a non-member tag
  returns the stackset unchanged). `S(S-xmo-ws)`.
- Then sway: creates and selects (the switch path creates on demand).
  `S(S-sway-ws)`.
- Then qtile/Columns: fixture-inapplicable (shipped static groups 1-9
  already contain 9, so an absent WS9 cannot be constructed).
  `S(S-qti-wsdef)`.
- Then awesome/tile: fixture-inapplicable (shipped static tags 1-9
  already contain tag 9, so an absent WS9 cannot be constructed; no
  creation path exists). `S(S-awe-ws)`.
- Then niri: clamps to the last workspace (no creation path).
  `S(S-nir-ws)`.
- Then PaperWM: owner-specific (GNOME dynamic-workspace policy owns
  creation; PaperWM only mirrors). `S(S-pap-space)`.
- Then karousel/Lazy: owner-specific (KWin owns desktops; no select
  verb exists). `S(S-kar-acts)`.
- Then paneru: creates and selects (`VirtualNumber` spawns the absent
  strip and switches to it). `S(S-pan-ws)`.
- Then Ours KDE: no-counterpart (no select verb exists in the Engine
  + adapter op inventory, so the select journey is shell-driven).
  `S(S-ours-planops)`.
- Then Ours Windows: refuses (`unknown-target`: the index resolves
  against the existing order only). `S(S-ours-ws)`.
- Variant hook: provisional/TBD (absent-select hook, to discuss).

### R-WS-14: send B to next; fresh run send B to previous

- Given (tree profiles): occupied WS1..WS3 with `WS2=H[A,B*]` shown.
  Ordinary windows, no rules, shipped defaults. Two legs from fresh
  fixtures with a reset between: leg 1 send B to the next workspace,
  leg 2 send B to the previous workspace. Edge wrap is a later
  qualified leg of R-WS-11, never combined here.
- Given (column profiles): `WS2=COL[C1[A],C2[B*]]` shown with the
  viewport on both columns; paneru uses an active strip holding
  columns A and focused B.
- When: the profile's native relative-send verb per leg (named per
  profile below); explicit-only inventories never substitute an
  absolute send.
- Observe: relative target resolution and follow policy.
- Then COSMIC: Shipped `MoveToNextWorkspace`/`MoveToPreviousWorkspace`
  resolve to active plus/minus one (next cycles to index 0 and previous
  to the last with wraparound, else output fallback) and follow with B;
  `SendToNextWorkspace`/`SendToPreviousWorkspace` leave focus.
  `S(S-cos-ws)` + `S(S-cos-wskeys)`.
- Then Hyprland/Dwindle: plain `next` targets WS3; plain `previous`
  targets the MRU previous workspace, not necessarily WS1. Follow
  focuses B; silent refocuses the source. `S(S-hyp-movews)` +
  `S(S-hyp-ws)`.
- Then bspwm: unflagged relative form `node -d next`/`prev` resolves the relative desktop (without `--follow` stays on the source; `--follow` keeps focus on B). `S(S-bsp-ws)`.
- Then i3: `move to workspace next`/`prev` resolves through the same
  no-follow path (focus restored to the source). `S(S-i3-movews)` +
  `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (relative shift
  needs CycleWS, outside the core+Navigation2D profile).
  `S(S-xmo-ctl)`.
- Then sway: `move to workspace next`/`prev` resolves through the
  same no-follow path (mover focus restored to the source inactive).
  `S(S-sway-movews)` + `S(S-sway-ws)`.
- Then qtile/Columns: no-counterpart (`togroup` takes explicit group
  names only; next/previous are view verbs that transfer nothing).
  `S(S-qti-ws)`.
- Then awesome/tile: no-applicable-journey (no relative-send verb exists: `move_to_tag` takes an explicit tag only, shipped numrow binds view/move/toggle by index only, and directional `swap.bydirection` is same-screen only; the relative next/previous send has no applicable tile journey, so tree and focus stay unchanged). `S(S-awe-tag)` + `S(S-awe-keys)` + `S(S-awe-swap)`.
- Then niri: resolves to the adjacent index with clamping at both
  ends (same-index is a no-op, never a wrap); Smart follow activates
  the target when the mover was active (`focus=false` stays).
  `S(S-nir-ws)`.
- Then PaperWM: resolves to the adjacent space via `selectSequenceSpace`
  with take-first and stops at the ends (out-of-range return, no wrap);
  the drop inserts at the open position (selected+1 under the shipped
  RIGHT default) as selectedWindow plus `Main.activateWindow`, so both
  legs follow; no stay variant exists in the registered inventory.
  `S(S-pap-space)` + `S(S-pap-take)` + `S(S-pap-ins)`.
- Then karousel/Lazy: each leg resolves to the adjacent desktop with
  an edge stop (past either end returns, never wraps) and moves the
  whole column C2, appended after the target's last column. `moveToGrid`
  passes Immediate to the source survivor (A on WS2), which takes focus
  through the script path; no target focus and no desktop switch are
  issued, so the follow and stay runs are identical (stay on the source
  with A focused; single verb, no follow flag). `S(S-kar-ws)` +
  `S(S-kwin-scriptact)`.
- Then paneru: South moves with the len-greater-than-one gate and
  North stops at index 0, each carrying the `MoveFocus`
  Follow/Stay policy. `S(S-pan-ws)`.
- Then Ours KDE: next resolves WS3, previous WS1 from the existing scoped
  ordinal ring (not MRU), once before transfer. Default relative follow
  Meta+Ctrl+Shift+H/K/Left/Up previous, J/L/Down/Right next; stay registered
  unbound, keeps WS2 selected and source MRU focus. Admission unchanged.
  Authentic clears stock KWin Window One Desktop arrows; Compatible disables
  our four arrows, keeps letters. Implemented offline, native journey pending.
  [Fixtures](../../../kwin/tests/workspace-send-follow-stay.test.ts),
  [reconciler](../../../kwin/native-effect/shortcutreconciler.cpp),
  [record](../../changes/archive/kde-workspace-send-follow-stay.md).
- Then Ours Windows: relative follow/stay uses the scoped ordinal ring,
  frozen once; Ctrl+Shift arrows/H/J/K/L follow, stay unbound bindable.
  Native CLI send-next/stay-next/stay-prev and source MRU/null agent-observed
  2026-10-11, base `553c65b` plus
  [delivery record](../../changes/archive/windows-workspace-send-follow-stay.md).
  Exact fresh WS2 previous-follow fixture and physical chords remain TBD.
- Variant hook: V-WS-FOLLOW (follow policy for relative sends).

## Selected additions (USER 2026-10-07; KDE items 1/2 delivered)

Targets follow [items 1/2 and decision 1.5](../../decisions.md#workspaces).
KDE R-WS-08/11 and R-WS-15..17 carry offline adapter/fixture evidence;
the user confirmed item 1's single-output native journey on 2026-10-07
("worked perfectly"), without specifying individual cases or presets.
Multi-output/hotplug and preset checks remain pending. Item 2 R-WS-01/14 and
R-WS-18..20 carry offline evidence only; native journeys pending.
Other source cells retain their pinned-current meaning.
R-WS-08 selects two-view previous-ID toggle, per-output local/global-unique
history or one shared history; record all successful observed changes, not
same-workspace activation/output focus alone. Removed/unassigned/out-of-scope
IDs clear; disconnected output history is discarded. Hotplug records without
history-driven reconnect selection. R-WS-11 selects the scoped existing-order
ring, wrapping including trailing empty and ordinals beyond 9, selection
creates nothing. R-WS-01/14 select numbered/relative follow defaults plus
bindable unbound stay, same ring resolved once before transfer, normal spare
maintenance; item 2 repairs KDE's source-view-preserving floating-boundary
path to the already-decided follow default. Shared core/KDE item 2 is
implemented offline. Windows local items 1/2 delivered with native gates and
scoped agent-observed CLI/Settings outcomes 2026-10-11, base `553c65b` plus
[record](../../changes/archive/windows-workspace-send-follow-stay.md).
Physical shortcut containment and non-local/multi-output runtime remain
pending in the [handoff](../../backlog.md).

### R-WS-15: previous on L after a workspace change on R

- Given (local/global-unique leg): occupied WS1/WS2 on L and two occupied
  workspaces on R; stable IDs, separate scoped orders. Start on L WS1.
- Given (shared leg): one shared workspace set shown across L/R; the
  corresponding observed changes are to WS1, WS2, then WS3, not independent
  per-output views. Reference native scope/model applicability stays TBD.
- When: visit L WS1 -> WS2; change R's workspace; focus L without changing
  its workspace; invoke previous twice. Shared leg: visit WS1 -> WS2 ->
  WS3, then invoke previous twice.
- Observe: per-output isolation vs shared two-view history; output-focus
  alone vs workspace change as a history producer.
- Then COSMIC: no-counterpart (no history-toggle verb exists; `LastWorkspace`
  targets the last index `len-1`, not the last-viewed workspace).
  `S(S-cos-ws)`.
- Then Hyprland/Dwindle: single global MRU timeline (entries carry
  workspace+monitor); `=1` previous is timeline-next (global), `=2`
  scans timeline-next for the same monitor; workspace.active and
  monitor.focused (deferred) both record, so the R change and the
  focus-L both re-front their entries; shipped `=0` only re-activates.
  Local leg (enabled `=1` variant): first previous shows R's current
  workspace, second toggles back to WS2; `=2` variant: first shows WS1,
  second WS2. Arrival focus via the focus candidate with pointer-hit
  precedence stays TBD (pointer fixture unspecified). Shared leg: no
  shared set exists (per-monitor ownership); local-leg rule applies per
  monitor. `S(S-hyp-ws)`; focus queued.
- Then bspwm: global history walk (`last` = HISTORY_OLDER match,
  monitor scope unfiltered without the `local` option); every desktop
  activation records, same-desktop focus records nothing
  (`activate_desktop` returns false). Local leg: first `last` shows R's
  current workspace, second returns to WS2; shown desktop per press
  established, exact focused node via the history fallback stays TBD.
  Shared leg: desktops are per-monitor; no shared set exists.
  `S(S-bsp-ws)` + `S(S-bsp-close)`; focused-node queued.
- Then i3: single global `previous_workspace_name`; every `workspace_show`
  records except the same-workspace early return (before the record) and
  internal cons. Local leg: the R change overwrites the previous, so first
  `back_and_forth` shows R's old workspace (on its output, descended
  remembered focus), second returns to WS2; output-focus alone records
  nothing. Shared leg: same single-toggle on the one output (WS2 then
  WS3); cross-output sharing N/A. `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no back-and-forth or
  previous-workspace verb in the profiled key inventory). `S(S-xmo-ctl)`.
- Then sway: per-seat single `prev_workspace_name`; `set_workspace`
  records on change only (same-workspace early return records nothing).
  Local leg: first `back_and_forth` shows R's old workspace (seat
  focus-inactive focus), second returns to WS2; output-focus alone
  records nothing. Shared leg: same single-toggle on the one output
  (WS2 then WS3). `S(S-sway-ws)` + `S(S-sway-switch)`.
- Then qtile/Columns: per-screen `previous_group` (saved on `set_group`
  only when the group changes; same-group re-select and bare screen
  focus record nothing); bare `toggle_group()` falls back to it.
  Local leg: WS1 then WS2 with the group's remembered `current_window`
  refocus. Shared leg model-inapplicable (a group shows on one screen
  at a time; cross-screen `set_group` swaps). `S(S-qti-ws)`.
- Then awesome/tile: per-screen tag history (identical selected-set
  re-select records nothing); `history.restore()` defaults to the
  `"previous"` toggle. Local leg: WS1 then WS2 with history-preferred
  refocus. Shared leg model-inapplicable (tags are per-screen).
  `S(S-awe-ws)` + `S(S-awe-hist)`.
- Then niri: per-monitor `previous_workspace_id` (recorded on every
  activation; same-index activation records nothing); `FocusWorkspacePrevious`
  / `switch_workspace_previous` goes to it, unresolvable stays.
  Local leg: WS1 then WS2 with the active workspace's active window
  focused. Shared leg fixture-inapplicable (each output owns its
  workspaces; no shared set). `S(S-nir-ws)`.
- Then PaperWM: MRU-list traversal with wrap, not a two-state toggle
  (`selectSequenceSpace` steps DOWN to the next-older live-MRU entry
  and wraps; the MRU is computed live, not recorded). Local leg: each
  press keeps walking down the list (never auto-returns); exact
  per-press spaces depend on the live stack order, TBD. Shared leg:
  spaces are per-monitor. `S(S-pap-space)`; per-press targets queued.
- Then karousel/Lazy: no-counterpart (no desktop-switch or history
  verb in the Actions inventory; desktop switching is KWin-native).
  `S(S-kar-acts)`.
- Then paneru: no-counterpart (no history verb in the `Operation`
  inventory; `Virtual` is directional North/South only).
  `S(S-pan-ws)`.
- Then Ours KDE: local/global-unique L WS1 then WS2, R unchanged;
  shared WS2 then WS3. Implemented offline, native journey TBD; item 1.2.
  [Fixtures](../../../kwin/tests/workspace-previous-relative.test.ts).
- Then Ours Windows: per-output pure isolation tested offline 2026-10-10
  (base `91db9d2` plus [record](../../changes/archive/windows-workspace-history-ring.md));
  local single-display runtime wired. This two-output journey and non-local
  modes remain pending; native outcome TBD (item 1.2).
- Variant hook: provisional/TBD (R-WS-08 history scope).

### R-WS-16: previous after the visited empty workspace is removed

- Given: a scoped trailing empty E and occupied W with stable IDs.
- When: visit E -> W; let E be removed by the native/managed lifecycle;
  invoke previous. Do not substitute another ordinal for E. If the profile
  cannot remove E, removal applicability/outcome stays TBD.
- Observe: removed-ID invalidation vs ordinal reinterpretation/recreation;
  separately record whether the empty ID actually survives.
- Then COSMIC: no-counterpart (no history-toggle verb exists; `LastWorkspace`
  targets the last index `len-1`, not the last-viewed workspace).
  `S(S-cos-ws)`.
- Then Hyprland/Dwindle: recreates E. Previous is timeline-next after W (both
  the `=1` global and the `=2` same-monitor scans agree here); `gc` runs on
  the lookup and keeps the second-position entry, so E's dead entry still
  resolves via its stored target (E is destroyed once inactive: weak
  state/history refs, no windows left, monitor hold dropped, and no persistent
  rule in the fixture), and find-or-create recreates the freed number and
  switches to it: recreation, not invalidation. `S(S-hyp-ws)`
- Then bspwm: E survives (desktops persist until the explicit `desktop -r`;
  emptiness never auto-removes), and `last` shows the surviving E.
  `S(S-bsp-ws)` + `S(S-bsp-wsretain)`.
- Then i3: E closes when empty and invisible, but previous is name-based
  and `workspace_get` creates on demand, so invoking previous recreates E
  by name and switches to it (recreation, not invalidation).
  `S(S-i3-ws)` + `S(S-i3-wsretain)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no back-and-forth or
  previous-workspace verb in the profiled key inventory). `S(S-xmo-ctl)`.
- Then sway: empty non-active workspaces are destroyed, but `back_and_forth`
  falls back to creating the previous name, so invoking previous recreates
  E and switches to it (recreation, not invalidation). `S(S-sway-ws)` +
  `S(S-sway-wsretain)`.
- Then qtile/Columns: removal inapplicable (static groups 1-9; emptiness
  never removes), so E survives and the previous-group toggle shows E.
  `S(S-qti-ws)` + `S(S-qti-wsdef)`.
- Then awesome/tile: removal inapplicable (static per-screen tags; only the
  explicit `tag.delete` removes), so E survives and `history.restore()`
  shows E. `S(S-awe-ws)`.
- Then niri: the trailing empty is spared by cleanup (active and trailing
  workspaces are never dropped), so E survives and `switch_workspace_previous`
  resolves the stored id to E and shows it; a removed id would resolve to
  no-op instead. `S(S-nir-ws)`.
- Then PaperWM: workspace add/remove is GNOME-owned (PaperWM only mirrors);
  `removeSpace` splices a removed space out of the MRU walk, so a removed E
  drops from the traversal with wrap to the next-older entry. Whether E
  itself survives depends on GNOME policy, TBD. `S(S-pap-space)`;
  E-survival queued.
- Then karousel/Lazy: no-counterpart (no desktop-switch or history
  verb in the Actions inventory; desktop switching is KWin-native).
  `S(S-kar-acts)`.
- Then paneru: no-counterpart (no history verb in the `Operation`
  inventory; `Virtual` is directional North/South only).
  `S(S-pan-ws)`.
- Then Ours KDE: removal clears previous E; no-op until next
  recorded change, no recreation/reinterpretation. Surviving empty E stays
  valid. Implemented offline, native journey TBD; item 1.3.
  [Fixtures](../../../kwin/tests/workspace-previous-relative.test.ts).
- Then Ours Windows: surviving empty previous stays valid; actual cleanup
  of a previously visited intermediate empty clears the stable ID, no
  recreation/reinterpretation. Portable lifecycle regression passed
  2026-10-10, base `91db9d2` plus
  [record](../../changes/archive/windows-workspace-history-ring.md).
  Exact given trailing-empty removal applicability/native result stays TBD.
- Variant hook: provisional/TBD (R-WS-08 previous-ID validity).

### R-WS-17: previous around disconnect displacement and reconnect return

- Given: local/global-unique L shows occupied WS2, previous WS1; R has an
  active window on workspace D whose disconnect displacement will show D
  on L. Record session-local output IDs and workspace scope.
- When: disconnect R; observe D shown on L; invoke previous on L;
  reconnect R; observe D's return scope; invoke previous on L again.
- Observe: hotplug history recording, reconnect selection independent of
  history, and clearing a previous ID when it returns to another output.
- Then COSMIC: no-counterpart (no history-toggle verb exists; `LastWorkspace`
  targets the last index `len-1`, not the last-viewed workspace).
  `S(S-cos-ws)`.
- Then Hyprland/Dwindle: D shown on L (premise input) fronts the single
  global timeline via the workspace-active record (monitor-focus re-tracks the
  same entry); first previous from D goes timeline-next, WS2 (the `=1` global
  and `=2` same-monitor scans agree here). Disconnect evacuates R's workspaces
  to the first remaining monitor with return-address records plus remembered
  active; reconnect moves returning D back to R and activates it via the
  remembered entry, never consulting the History tracker, while L keeps WS2.
  No previous-ID invalidation exists (entries persist, so nothing clears when
  D returns to R). Second previous on L targets D on R (both scans agree on
  the stale-tagged entry) and switches monitor focus back to R/D.
  `S(S-hyp-ws)` + `S(S-hyp-monlife)`
- Then bspwm: shipped defaults retain the disconnected monitor and its
  desktops (`remove-unplugged`/`remove-disabled` default false; same-id
  return reuses them), so D never shows on L and the displacement has no
  counterpart at defaults; the named removal variant migrates all desktops
  to the target before removing. Under that variant the `last` history
  walk applies; exact first/second targets stay TBD. `S(S-bsp-monrm)` +
  `S(S-bsp-ws)`; variant targets queued.
- Then i3: the displaced workspace migrates to L with its name preserved via
  `move_content` (first-remaining-output attach with floating-coordinate fix);
  the displacement `workspace_show` early-returns on the moved workspace
  (same-workspace re-show), so it records no previous, and reconnect assigns
  fresh via `init_ws_for_output` without consulting previous-workspace history
  (no return affinity under the profile with no assignments; no previous-ID
  clearing exists, stale names recreate via `workspace_get`). First/second
  previous targets stay TBD (F: the global focus chain and previous name at
  disconnect are unstated; the Given per-L previous WS1 does not fix i3's
  single global history). `S(S-i3-ws)`; targets queued.
- Then xmonad/Tall+Navigation2D: no-counterpart (no back-and-forth or
  previous-workspace verb in the profiled key inventory). `S(S-xmo-ctl)`.
- Then sway: output removal evacuates each workspace to the priority
  highest-available else fallback output (destroying empties); evacuate
  and restore issue no seat focus/previous write themselves
  (destroyed-empty focus refocuses via the seat destroy-listener;
  inventoried occupied workspaces survive), so the displacement itself
  records nothing and reconnect returns D to R via the same priority
  affinity without consulting previous-workspace history. Sway keeps a
  single per-seat previous (change-only) with global name lookup and no
  out-of-scope clear (stale names recreate). First/second previous
  targets stay TBD (F: pre-disconnect global seat focus and previous
  chain unstated; the Given per-L previous WS1 does not fix sway's
  single global history). `S(S-sway-evac)` + `S(S-sway-ws)`;
  targets queued.
- Then qtile/Columns: previous is per-screen, so R-side state lives on the
  removed screen object while L's own record (WS1) survives intact; first
  previous on L selects WS1 with the group's remembered `current_window`.
  Disconnect hides R-shown groups with contents retained (no merge into L)
  via the `screen_change`/`reconfigure_screens` path, and reconnect assigns
  the first config-order available group with no identity/affinity store;
  exact return mapping and the second toggle stay TBD (F: group
  numbering/assignment unstated). `S(S-qti-ws)` + `S(S-qti-screen)`;
  return/second queued.
- Then awesome/tile: disconnect evacuates via the shipped-default removed fallback, not `tag.screen`: D is deleted with its members into L's first tag and R-side history is cleared (L history untouched); the first previous on L restores the Given WS1 set via `history.restore`; reconnect mints fresh per-screen tags with no return of D (no affinity store); the second previous toggles back to WS2. Exact refocus targets stay TBD (F: WS1/WS2 focus histories unstated, read by the selection-change path). `S(S-awe-ws)` + `S(S-awe-hist)`.
- Then niri: displaced workspaces insert before the trailing empty on L
  (`append_workspaces`); showing D on L records WS2 in L's per-monitor
  previous id, so the first previous on L selects WS2. Reconnect returns
  D to R by preferred-output affinity (`original_outputs`) with the
  stored last-active workspace reactivated, never consulting history;
  remove/insert/append/cleanup write no previous id (only activate
  writes), so L's previous still names the departed D, which resolves to
  no entry on L and the second previous on L is a no-op staying on WS2.
  `S(S-nir-ws)`.
- Then PaperWM: owner-specific (GNOME owns outputs and workspace add/remove;
  PaperWM only mirrors via `workspacesChanged`); the MRU is live-computed,
  so there is no recorded previous ID to invalidate. Native displacement
  selection, return scope, and toggles stay TBD. `S(S-pap-space)`.
- Then karousel/Lazy: fixture-inapplicable (single-screen profile has
  no second output to receive WS2). `S(S-kar-single)`.
- Then paneru: no-counterpart (no history verb in the `Operation`
  inventory; `Virtual` is directional North/South only).
  `S(S-pan-ws)`.
- Then Ours KDE: disconnect WS2 -> D records previous WS2;
  first toggle selects WS2, previous D. D's return to R clears L's previous
  D; if reconnect preserves L WS2 with no further recorded change, second
  toggle is a no-op. Reconnect never consults/restores history; any observed
  workspace change records normally; R's disconnected history is discarded.
  Implemented offline, native journey TBD (1.5).
  [Fixtures](../../../kwin/tests/workspace-previous-relative.test.ts).
- Then Ours Windows: same selected 1.5 history/scope rule and conditional
  second toggle; pure disconnect/reconnect invalidation tested offline
  2026-10-10, base `91db9d2` plus
  [record](../../changes/archive/windows-workspace-history-ring.md).
  Multi-output runtime parked; native journey TBD.
- Variant hook: provisional/TBD (R-WS-08 hotplug history).

### R-WS-18: relative next send fills the trailing empty

- Given (tree leg): scoped order occupied WS1..WS10, shown last occupied
  WS10 `H[A,B*]`, trailing empty E. WS10 establishes an ordinal beyond 9.
- Given (column leg): same order with WS10 `COL[C1[A],C2[B*]]`;
  trailing-empty/model applicability TBD per reference.
- When: send B next with follow; inspect E and the spare; fresh reset,
  repeat with stay. Resolve the pre-transfer order once each run.
- Observe: E reused/filled vs new target; next trailing spare; follow/stay.
- Then COSMIC: next resolves to active+1, which is the existing trailing
  empty E; B fills E (sole; no split), source collapses to A, and
  `ensure_last_empty` supplies the next spare while removing non-active
  non-last empties. `MoveToNextWorkspace` follows with B, `SendToNextWorkspace`
  leaves focus. Exact frames TBD. `S(S-cos-ws)` + `S(S-cos-send)` +
  `S(S-cos-newgroup)`; frames queued.
- Then Hyprland/Dwindle: plain next targets numeric+1 via find-or-create;
  follow focuses the mover, silent refocuses the source. E is filled when
  it is the numeric+1 workspace, else a new numeric workspace is created
  (no trailing-empty semantic exists); B lands sole (empty target has no
  anchor contest). Exact E-vs-created identity and frames stay TBD.
  `S(S-hyp-ws)` + `S(S-hyp-movews)`; identity/frames queued.
- Then bspwm: E is an explicit desktop; unflagged `node -d next` resolves
  the relative desktop, transfers with sibling promotion at the source
  via `transfer_node`, and B fills the empty E sole (empty focus/root takes root, no split; source collapses to A); `--follow` keeps focus on B,
  otherwise the send stays on the source. Desktops are retained (explicit
  `desktop -r` only, no spare lifecycle). `S(S-bsp-ws)` + `S(S-bsp-xfer)` +
  `S(S-bsp-insert)`
- Then i3: `move to workspace next` resolves via the same no-follow path
  (both the follow and stay runs stay: mover focus restored to the source);
  B attaches after the E target focus. No spare is created (creation is
  on-demand via select); source A survives. Exact frames TBD.
  `S(S-i3-movews)` + `S(S-i3-ws)`; frames queued.
- Then xmonad/Tall+Navigation2D: no-counterpart (no relative-send verb in
  the profiled core+Navigation2D inventory; relative shift needs CycleWS).
  `S(S-xmo-ctl)`.
- Then sway: `move to workspace next` resolves via the same no-follow path
  (both runs stay: mover focus restored to the source inactive); B attaches
  after the E target focus. No spare is created; source A survives.
  Exact frames TBD. `S(S-sway-movews)` + `S(S-sway-ws)`; frames queued.
- Then qtile/Columns: no-counterpart (`togroup` takes explicit group
  names only; next/previous are view verbs that transfer nothing).
  `S(S-qti-ws)`.
- Then awesome/tile: no-applicable-journey (no relative-send verb exists: `move_to_tag` takes an explicit tag only, shipped numrow binds view/move/toggle by index only, and directional `swap.bydirection` is same-screen only; the relative send has no applicable tile journey, so tree and focus stay unchanged). `S(S-awe-tag)` + `S(S-awe-keys)` + `S(S-awe-swap)`.
- Then niri: down resolves to `min(active+1, len-1)`, which is E; B fills E
  (sole) and filling the last workspace inserts the next empty bottom spare;
  source A survives. Smart follow activates the target when the mover was
  active (`focus=false` stays). `S(S-nir-ws)` + `S(S-nir-ins)` +
  `S(S-nir-wscarry)`.
- Then PaperWM: `moveDownSpace` takes the selected window first and steps to
  the adjacent space with an end stop; the drop completes with insert plus
  `Main.activateWindow` (follow per take finalization). Target insert is
  the open-position index (exact position TBD per the R-WS-14 precedent);
  whether E exists as a GNOME space stays TBD. `S(S-pap-space)` +
  `S(S-pap-take)` + `S(S-pap-ins)`; position/E-applicability queued.
- Then karousel/Lazy: `columnMoveToNextDesktop` steps to the adjacent
  desktop with an edge stop (no wrap); desktops are explicit-only with
  no auto-spare and no empty auto-removal, so the fixture-count E
  persists and B fills it sole, appended after E's last (empty target).
  Source A survives with the removal fixup and takes focus through the
  script path; no switch and no target focus are issued, so follow and
  stay runs are identical (stay with A). `S(S-kar-ws)` +
  `S(S-kwin-scriptact)` + `S(S-kwin-desktops)`.
- Then paneru: South relative move passes the len-greater-than-one gate
  (source holds A and B) and targets the next virtual index under the
  `MoveFocus` Follow/Stay policy; a missing target row is created by the
  move path (the auto-create gate sits on the switch path only), so B lands
  sole on the next row whether or not a row pre-exists there, and the source
  collapses to A. Follow switches the view and focuses B; Stay keeps the
  source view and refocuses A. E has no virtual-row counterpart at shipped
  defaults (no trailing concept; auto-create and reaping default off), and
  no spare lifecycle runs. `S(S-pan-ws)`.
- Then Ours KDE: selected B fills existing E; normal lifecycle supplies
  next empty; source A survives. Follow goes with B, stay preserves source
  view with focused-removal MRU. Implemented offline (item 2.2), native journey
  pending. Ring/spare and send-flight [fixtures](../../../kwin/tests/workspace-send-follow-stay.test.ts).
- Then Ours Windows: ring/spare/follow/stay implemented; native CLI relative
  next from ordinal 10 fills the existing spare, supplies a next spare and
  follows mover (sole-source variant), agent-observed 2026-10-11, base
  `553c65b` plus [record](../../changes/archive/windows-workspace-send-follow-stay.md).
  Two-window WS10 stay variant offline-tested; exact native fixture/physical
  chords TBD (items 1.4/2.2).
- Variant hook: V-WS-FOLLOW.

### R-WS-19: relative previous send wraps from the first workspace

- Given (tree leg): scoped occupied WS1..WS3, shown first WS1 `H[A,B*]`,
  trailing empty E last; history does not redefine the ordinal order.
- Given (column leg): WS1 `COL[C1[A],C2[B*]]` with the same order;
  trailing-empty/model applicability TBD per reference.
- When: send B previous with follow; fresh reset, repeat with stay.
- Observe: ordinal wrap to E vs MRU target; target resolved before transfer;
  spare maintenance and follow/stay.
- Then COSMIC: previous resolves to active-1; from the first workspace the
  decrement fails and wraparound (shipped default on) cycles to
  `MoveToLastWorkspace`/`SendToLastWorkspace`, which is the pre-transfer E;
  B fills E (sole), source collapses to A, next spare ensured. Follow goes
  with B, stay leaves focus. Exact frames TBD. `S(S-cos-ws)` +
  `S(S-cos-send)` + `S(S-cos-newgroup)`; frames queued.
- Then Hyprland/Dwindle: no wrap occurs: plain previous resolves to the
  MRU-history previous, never an ordinal target, and the fresh fixture carries
  no history past WS1, so the target resolves invalid and the send errors with
  no transfer (tree and focus unchanged). Follow/silent focus never applies
  (no move runs). `S(S-hyp-ws)` + `S(S-hyp-movews)`
- Then bspwm: the desktop list is circular, so previous from the first wraps
  to the last desktop E; `transfer_node` fills the empty E sole with
  source sibling promotion (empty focus/root takes root, no split; source collapses to A); `--follow` keeps focus on B, otherwise stays.
  Desktops are retained (explicit `desktop -r` only). `S(S-bsp-ws)` +
  `S(S-bsp-xfer)` + `S(S-bsp-insert)`
- Then i3: `move to workspace prev` wraps via the first/last fallback to the
  pre-transfer E through the same no-follow path (both runs stay); B attaches
  after the E target focus; no spare is created; source A survives. Exact
  frames TBD. `S(S-i3-movews)` + `S(S-i3-ws)`; frames queued.
- Then xmonad/Tall+Navigation2D: no-counterpart (no relative-send verb in
  the profiled core+Navigation2D inventory; relative shift needs CycleWS).
  `S(S-xmo-ctl)`.
- Then sway: `move to workspace prev` wraps via the last/first fallback to
  the pre-transfer E through the same no-follow path (both runs stay);
  B attaches after the E target focus; no spare is created; source A
  survives. Exact frames TBD. `S(S-sway-movews)` + `S(S-sway-ws)`;
  frames queued.
- Then qtile/Columns: no-counterpart (`togroup` takes explicit group
  names only; next/previous are view verbs that transfer nothing).
  `S(S-qti-ws)`.
- Then awesome/tile: no-applicable-journey (no relative-send verb exists: `move_to_tag` takes an explicit tag only, shipped numrow binds view/move/toggle by index only, and directional `swap.bydirection` is same-screen only; the relative send has no applicable tile journey, so tree and focus stay unchanged). `S(S-awe-tag)` + `S(S-awe-keys)` + `S(S-awe-swap)`.
- Then niri: up resolves to `saturating_sub(1)` = same index from the first
  workspace, which is a no-op that never wraps, so no transfer to E occurs
  in either run. `S(S-nir-ws)`.
- Then PaperWM: `moveUpSpace` steps to the adjacent space with an end stop,
  so from the first space no move occurs in either run. `S(S-pap-space)`.
- Then karousel/Lazy: `columnMoveToPreviousDesktop` stops at the first
  desktop edge, so from the first desktop no move occurs in either run.
  `S(S-kar-ws)`.
- Then paneru: North relative move stops at index 0, so from the first row
  no move occurs in either run. `S(S-pan-ws)`.
- Then Ours KDE: selected previous wraps to pre-transfer E, fills it;
  normal lifecycle supplies next empty. Follow with B, stay on WS1 with A.
  Implemented offline; item 2.2, native journey pending.
  [Fixtures](../../../kwin/tests/workspace-send-follow-stay.test.ts).
- Then Ours Windows: first relative previous stay fills pre-transfer trailing
  E, retains source MRU survivor and supplies next spare, agent-observed
  via CLI 2026-10-11, base `553c65b` plus
  [record](../../changes/archive/windows-workspace-send-follow-stay.md).
  Follow counterpart tested offline; exact native follow/physical chords TBD.
- Variant hook: V-WS-FOLLOW.

### R-WS-20: relative edge sends when the source becomes empty

- Given: fresh fixtures from R-WS-18 and R-WS-19, but source holds sole B*
  (column leg `COL[C1[B*]]`); all other scope/order preparation unchanged.
- When: send B next from last occupied; reset, send B previous from first;
  repeat each from fresh fixtures with stay instead of follow.
- Observe: source emptiness vs target/spare resolution and follow/stay;
  record source retention/removal and exact empty-source focus separately.
- Then COSMIC: next leg never transfers on a Left/Right-inferred chord
  (early return under the shipped Vertical layout before any move) and
  refuses with `InvalidWorkspaceIndex` on an Up/Down-inferred chord
  (single-window source to the adjacent trailing empty, before any
  transfer, both follow and stay runs; source unchanged, focus stays B);
  the shipped shortcut path carries propagate=false for these verbs, so
  no output-move fallback runs, and with the shipped default
  wraparound=true the refused Up/Down chord falls back to an index-1
  send (follow with B to WS1, stay keeps the emptied source active).
  Previous leg proceeds on an Up/Down-inferred chord (non-adjacent wrap to the
  pre-transfer E, fills it sole; follow activates E with focus B, stay keeps
  the active emptied source with eventual keyboard focus none via the fixup
  path).
  Post-transfer cleanup converges via refresh `ensure_last_empty`: the
  emptied source is retained while active and dropped once non-active
  non-last empty, with the next spare ensured.
  `S(S-cos-wssingle)` + `S(S-cos-ws)` + `S(S-cos-send)` +
  `S(S-cos-focusfix)`; chord queued (F: invoked chord direction
  unrecorded; exact next-leg outcome TBD; primary F).
- Then Hyprland/Dwindle: next targets numeric+1 via find-or-create (no trailing-empty semantic exists); whether that slot is the fixture E or a newly created workspace stays TBD (F: E's own number unstated, so E-vs-created identity is not established). B lands sole with no anchor contest when the target is empty.
  Empty-source focus is established: follow focuses the mover, silent refocuses
  the emptied source to none via the first-window fallback. Numbered IDs never
  renumber; emptied-source survival is refcount (weak state/history refs, no
  persistent rule in the fixture): follow switches away so the source is
  destroyed, silent keeps it active-empty retained. Previous from the first
  follows the MRU-history rule with no history here, so no transfer occurs
  (never an ordinal wrap). `S(S-hyp-ws)` + `S(S-hyp-movews)`
- Then bspwm: emptied source retained either leg (desktops persist until the
  explicit `desktop -r`); `transfer_node` fills the empty E sole
  with source sibling promotion (empty focus/root takes root, no split); `--follow` keeps focus on B, otherwise the send stays on the (emptied)
  source with empty-source focus none. `S(S-bsp-ws)` +
  `S(S-bsp-xfer)` + `S(S-bsp-wsretain)`
- Then i3: next/previous resolve via the wrap fallback through the same
  no-follow path (both runs stay; mover focus restored to the source);
  attachment is after the target focus. The emptied source is retained
  while visible and closes once empty and invisible; the restored focus is
  the emptied source workspace node. `S(S-i3-movews)` + `S(S-i3-ws)` +
  `S(S-i3-wsretain)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no relative-send verb in
  the profiled core+Navigation2D inventory; relative shift needs CycleWS).
  `S(S-xmo-ctl)`.
- Then sway: next/previous resolve via the wrap fallback through the same
  no-follow path (both runs stay: mover focus restored to the source
  inactive, no switch); attachment is after the target focus (empty E
  takes B sole). The emptied source is spared while active (or
  seat/focus-inactive referenced) and destroyed once empty and
  non-active, leaving focus on the emptied source workspace node.
  `S(S-sway-movews)` + `S(S-sway-ws)` + `S(S-sway-wsretain)`.
- Then qtile/Columns: no-counterpart (`togroup` takes explicit group
  names only; next/previous are view verbs that transfer nothing).
  `S(S-qti-ws)`.
- Then awesome/tile: no-applicable-journey (no relative-send verb exists: `move_to_tag` takes an explicit tag only, shipped numrow binds view/move/toggle by index only, and directional `swap.bydirection` is same-screen only; the relative send has no applicable tile journey, so tree and focus stay unchanged). `S(S-awe-tag)` + `S(S-awe-keys)` + `S(S-awe-swap)`.
- Then niri: next fills E (sole) with the next empty bottom spare; with
  Smart follow the target activates and the emptied source (non-active,
  non-trailing) is removed at eventual cleanup (cleanup gated on no
  switch animation), while `focus=false` stay keeps the active emptied
  source spared. Follow focuses B via the active workspace's active
  window; stay leaves the emptied active source with focus none (no
  active window). Previous from the first is a same-index no-op (never
  wraps). `S(S-nir-ws)` + `S(S-nir-ins)`.
- Then PaperWM: next fills E when present (take-first, open-position
  insert, `Main.activateWindow` follow); the emptied source's column
  splices with space removal GNOME-owned, TBD. Previous from the first
  stops at the end (no move). Exact insert position and E-applicability
  stay TBD. `S(S-pap-space)` + `S(S-pap-take)` + `S(S-pap-ins)`;
  position/removal queued.
- Then karousel/Lazy: next moves the whole sole-window column to the
  adjacent desktop, appended after the target's last (E sole when E is
  the target); the emptied source drops its last column with
  `lastFocusedColumn` nulled and no focus request, and the emptied
  desktop is retained (removal is explicit-only; karousel destroys
  Desktop objects only on KWin removal). Focus stays B with no karousel
  write. Previous from the first stops at the edge (no move, focus
  unchanged). E persists as in R-WS-18. `S(S-kar-ws)` +
  `S(S-kwin-scriptact)` + `S(S-kwin-desktops)`.
- Then paneru: both legs are no-moves (South relative move needs
  len-greater-than-one, but the sole-B strip has length one; North stops
  at index 0). `S(S-pan-ws)`.
- Then Ours KDE: selected target is pre-transfer E in both legs; fills E,
  normal lifecycle supplies next empty. Follow with B; stay preserves source
  view. Core emptied-source desired focus is null (adapter issues no focus
  setter); native focus and exact source retention/removal remain TBD.
  Implemented offline (item 2.2); native journey pending.
  [Core](../../../crates/tiler-core/tests/session_send_to_workspace.rs),
  [ring/flight fixtures](../../../kwin/tests/workspace-send-follow-stay.test.ts).
- Then Ours Windows: target/spare/follow/stay implemented. Agent-observed
  sole-source next follow fills spare; sole-source stay keeps source view
  with null focus/no setter (native focus may fall to existing terminal,
  never explicitly targeted). Offline coverage includes both edge directions.
  Exact full given lifecycle/focus variants and physical chords remain TBD;
  2026-10-11, base `553c65b` plus
  [record](../../changes/archive/windows-workspace-send-follow-stay.md).
- Variant hook: V-WS-FOLLOW.

## Selected addition (user-selected NORMATIVE 2026-10-08; delivery status per profile)

Targets follow [workspace migration D1-D9](../../decisions.md#workspaces)
(User 2026-10-08 selections, REQ-WS-12a..i). KDE
R-WS-21..26 carry offline Engine/adapter/fixture evidence; D1-D9 native
runtime outcomes are TBD (no live acceptance recorded here); D8 carry is
delivered offline 2026-10-09 with live check pending. Ours Windows
has no whole-workspace verb. Reference overlay-carry evidence below is
source-read at the stated pins; all other unsupported reference outcomes
stay TBD:
no new reference source was read for those fixtures. The hidden R-WS-12
baseline above is unchanged. Record:
[Q4](../../changes/archive/kde-whole-workspace-output-migration.md).

<a id="r-ws-21-migrate-mode-capability"></a>
### R-WS-21: migrate mode/capability (true/false/unreadable/shared)

- Given: L shows active WS2 `H[A,B*]`; R shows WS3. Project mode
  local, global-unique, or shared; native per-output desktop option
  true, false, or unreadable. Fresh reset per leg.
- When: migrate WS2 right.
- Observe: migration vs refusal reason; views unchanged on refusal;
  no setting writes.
- Then COSMIC: no mode/shared/per-output gate exists in the profiled
  inventory; the active WS2 migrates via `MigrateWorkspaceToOutput`
  (activates there, switches output); refusal legs have no counterpart.
  `S(S-cos-ws)`.
- Then Hyprland/Dwindle: no mode gate exists (the move path takes workspace
  plus monitor directly, with no mode or per-output check); whole-workspace
  reassignment via `moveToMonitor`. With L focused (B* focused there) the
  destination activates: R shows the moved WS2 with WS3 hidden, and the source
  gap-plugs with no window-focus write, so the focused window object stays B.
  The fixture names no other L workspace, so the plug finds nothing remaining
  and creates the first free number (taken: moved WS2 plus WS3; created 1),
  which L shows. Refusal legs have no counterpart (nothing to refuse through).
  `S(S-hyp-ws)` + `S(S-hyp-wsmove-fs)` + `S(S-hyp-pinstay)`
- Then bspwm: no mode gate exists; whole-desktop reassignment via
  `desktop -m MONITOR`; both branches resolve the source through the
  focus fallback (NULL desk is transient: history-last-else-head is
  shown with its focus-memory node), so follow and stay differ only in
  destination focus (follow focuses the moved desktop, stay keeps
  source focus). Which history entry shows is fixture-unstated.
  `S(S-bsp-ws)` + `S(S-bsp-wsstay)` + `S(S-bsp-close)`; history-entry
  queued.
- Then i3: no mode gate exists; WS2 is active so the bare
  `move workspace to output R` detaches/attaches the whole workspace
  with floating fix and shows it on R with the descended remembered
  focus; the emptied source shows its next focus-stack entry (created
  if last). `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no workspace-ownership
  move verb exists; the mode gate is likewise absent). `S(S-xmo-ws)`.
- Then sway: no mode gate exists; WS2 is active so
  `move workspace to output` (handler-context active) detaches/attaches
  with source refill; R shows the migrated WS2 with B focused (the
  workspace node's parent follows output ownership, and the stacked
  WS2 node qualifies on R with no focus write); L's shown entry stays
  TBD (remaining order and prior stack unstated). `S(S-sway-ws)` +
  `S(S-sway-wsactive)`; L-shown queued.
- Then qtile/Columns: no mode gate exists; view-ownership reassignment
  via `toscreen` on R (both workspaces screened, so the groups swap);
  R refocuses the mover (WS2's remembered `current_window`, B) while
  L's refocus target stays TBD (WS3's remembered window is
  fixture-unstated). `S(S-qti-ws)`; L-focus queued.
- Then awesome/tile: no mode gate exists; view-ownership reassignment
  via `tag.screen` (moves the tag plus all member clients with screen plus tags rewritten, layout retained as the same tag; R keeps showing WS3 since
  selection is untouched; the old screen restores from history, whose
  content is fixture-unstated); exact restored set and focus stay TBD (F: history content fixture-unstated, genuinely read by the restore and selection-change paths).
  `S(S-awe-ws)`; restored-set/focus queued.
- Then niri: no mode gate exists; whole-workspace reassignment via
  `MoveWorkspaceToMonitorByRef` (explicit output-plus-reference
  resolution; the moved-active variant activates the target).
  `S(S-nir-ws)`.
- Then PaperWM: no mode/shared/per-output gate exists in the registered
  inventory; the active WS2 migrates via `move-space-monitor` (swap
  fallback when it is the monitor's last space; `-1` neighbor stays with
  no move; fewer-or-equal spaces than monitors notifies and stays);
  refusal legs have no counterpart and write no setting. `S(S-pap-space)` +
  `S(S-pap-mon)`.
- Then karousel/Lazy: fixture-inapplicable (single-screen profile has
  no second output to receive WS2). `S(S-kar-single)`.
- Then paneru: no-counterpart (`ToNextDisplay` moves the focused
  window only, never a whole strip or Space). `S(S-pan-cmds)`.
- Then Ours KDE: local/global-unique with strict-true flag migrate;
  shared refuses `mode-shared`, false refuses `per-output-disabled`,
  unreadable refuses `per-output-unreadable`; never writes the
  setting. Implemented offline (NORMATIVE D2, User 2026-10-08), native journey TBD.
  [Observer](../../../kwin/src/plan-adapter-entry.ts),
  [fixtures](../../../kwin/tests/workspace-migrate.test.ts).
- Then Ours Windows: no-counterpart (no whole-workspace verb).
  `S(S-ours-planops)`.
- Variant hook: NORMATIVE workspace-output migration (User 2026-10-08).

<a id="r-ws-22-active-migrate-layout-retained"></a>
### R-WS-22: explicitly selected active leg migrates with layout retained

- Given: L shows WS1; occupied WS2 `H[A,V[B*,C]]` hidden on L; R
  shows WS3 plus hidden WS4. Strict local/true. Fresh reset; select
  WS2 on L first (active leg only, never the hidden baseline).
- When: migrate WS2 right.
- Observe: backing id retained; tree/order/shares/remembered
  focus/tiling mode; target order and shown view; moved active focus.
- Then COSMIC: the same workspace object moves between sets (same
  handle/backing id retained; removed from the source, inserted after
  R's active, so R order becomes WS3, WS2, WS4) with `set_output`
  carrying tiling plus floating layers and `tiling_enabled` untouched;
  it activates there and switches output (L keeps showing WS1, R shows
  the migrated WS2), so tree/order/shares and tiling mode carry as the
  same object and the moved-active focus resolves to retained B
  (fixture-focused; the SwitchOutput leg focuses the target
  focus-stack last). `S(S-cos-ws)` + `S(S-cos-wsmig)` +
  `S(S-cos-wsmove-fs)`.
- Then Hyprland/Dwindle: the same workspace object is reassigned to R
  (`m_monitor`): backing id retained; members keep the workspace with floating
  reposition and fullscreen setBox, so the tiling tree, split shares (no ratio
  writes, geometry refits only), remembered focus (the tracker travels with the
  object; B remembered), and tiling mode carry. The fixture names no other L
  workspace beyond WS1, so the source plug selects WS1 and L shows it. No
  reorder occurs (in-place reassignment; the retained creation order is the
  target order); R shows the moved WS2 with WS3/WS4 hidden, and the move
  writes no window focus (B retains object focus). `S(S-hyp-ws)` +
  `S(S-hyp-wsmove-fs)` + `S(S-hyp-pinstay)`
- Then bspwm: the desktop object is reassigned via transfer
  (unlink/insert, tree retained); the source always resolves through
  the focus fallback (NULL desk transient: history-last-else-head shown
  with focus memory); follow and stay differ only in destination focus
  (follow focuses the moved desktop's remembered focus (B) on a focused
  monitor, stay keeps source focus); the flag is fixture-unstated.
  `S(S-bsp-ws)` + `S(S-bsp-wsstay)` + `S(S-bsp-close)`; flag queued.
- Then i3: the same con detaches/attaches (layout retained) with
  floating coordinate fix; shown on R with the descended remembered
  focus (B); the emptied source shows its next focus-stack entry
  (created if last). `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no workspace-ownership
  move verb exists). `S(S-xmo-ws)`.
- Then sway: detach/attach with source refill (replacement with raw
  focus when last, else no show write); R shows the migrated WS2 with
  B focused (ownership query, no focus write); L shows WS1 (stacked
  WS1 node, else the sole-remainder items[0] fallback); the exact L
  focus node stays TBD (no refocus write; WS1 content unstated).
  `S(S-sway-ws)` + `S(S-sway-wsactive)`; L-focus queued.
- Then qtile/Columns: cross-screen `set_group` swaps WS2 with WS3 (both
  screened); R refocuses B via `layout_all` while L's refocus target
  stays TBD (WS3's remembered `current_window` is fixture-unstated);
  exact frames TBD. `S(S-qti-ws)`; L-focus/frames queued.
- Then awesome/tile: `tag.screen` moves the tag plus all member clients
  with screen plus tags rewritten (layout retained as the same tag, backing id retained); R keeps showing WS3 (selection
  untouched) while the old screen restores from history (content
  fixture-unstated); exact restored set and focus stay TBD (F: history content fixture-unstated, genuinely read by the restore and selection-change paths).
  `S(S-awe-ws)`; restored-set/focus queued.
- Then niri: remove/insert by reference with moved-active activation;
  inserts after R's active entry; members retained via `set_output`
  re-entry; focus resolves to the moved workspace's active window (B).
  `S(S-nir-wsmove)` + `S(S-nir-ws)`.
- Then PaperWM: the same space object migrates (spaces keyed by workspace;
  `setMonitors`/`setMonitor` reassign the monitor with geometry/layout but
  no column rewrite), so backing id, columns/order/shares, retained
  `selectedWindow` and focus mode carry; the dance is `selectStackSpace`
  DOWN with wrap plus navigator finish (retained-tile activation) plus
  `switchMonitor` (`lteSpaces` notify-and-stay; last-on-monitor swap
  fallback; `-1` stays). Target order follows GNOME index ownership
  (`_getOrderedSpaces` workspace-index order) and shown views/moved-active
  focus follow the live stack (WS2 fronted by the explicit select; older
  entries from the unstated tab order), so target order, views and focus
  stay TBD. `S(S-pap-space)` + `S(S-pap-mon)` + `S(S-pap-wssel)`;
  order/views/focus queued (fixture + host).
- Then karousel/Lazy: fixture-inapplicable (single-screen profile has
  no second output). `S(S-kar-single)`.
- Then paneru: no-counterpart (`ToNextDisplay` moves the focused
  window only). `S(S-pan-cmds)`.
- Then Ours KDE: same id, tree/order/shares/remembered
  focus/tiling mode retained via `relocate_domain`; inserts after the
  target current and shows the migrated workspace; the prior target
  stays hidden; moved active client refocused after verified arrival
  and views. Implemented offline (NORMATIVE D3/D4/D6, User 2026-10-08),
  native journey TBD.
  [Engine](../../../crates/tiler-core/src/engine.rs),
  [native map](../../../kwin/src/workspace-native.ts),
  [engine fixtures](../../../kwin/tests/workspace-migrate-engine-fixture.test.ts).
- Then Ours Windows: no-counterpart (no whole-workspace verb).
  `S(S-ours-planops)`.
- Variant hook: NORMATIVE workspace-output migration (User 2026-10-08).

<a id="r-ws-23-source-refill-empty-migrate"></a>
### R-WS-23: source refill and empty migration

- Given: L order WS1, active WS2 `H[A,B*]`, spare E; R shows WS3.
  Strict local/true. Second leg from a fresh fixture: trailing empty
  E selected active on L.
- When: migrate right; inspect the source view and both
  minimum-two/trailing-spare inventories.
- Observe: source refill vs removal; empty migration vs refusal.
- Then COSMIC: the source shows the last remaining entry (post-remove
  falls back to last with Active state; a fresh empty is added only if
  the set emptied); the empty E migrates identically (no emptiness gate
  among the traced migrate refusals). `S(S-cos-wsmig)`.
- Then Hyprland/Dwindle: source refills, never removes: the active-leg move
  gap-plugs L via `changeWorkspace`; numbered IDs never renumber and no
  ordinary empty-destroy verb exists (refcount only: weak state/history refs,
  no persistent rule in the fixture). The empty E migrates identically (the
  move path has no window-count gate). Leg 1 leaves two remaining candidates
  (WS1, E), so which one the first-enumerated plug selects stays TBD (F:
  creation order genuinely unstated, proven by the two-candidate state-vector
  enumeration). Leg 2's plug creates the first free number, whose value stays
  TBD (F: E's own number and the full number inventory unstated). `S(S-hyp-ws)`
  + `S(S-hyp-wsmove-fs)`; plug-pick queued.
- Then bspwm: retained (desktops persist until the explicit
  `desktop -r`); both legs resolve the source through the focus
  fallback: history-last-else-head is shown with focus memory (the
  fixture's visit history is unstated, so WS1 vs E stays TBD; E as a
  never-visited spare constrains but never establishes the pick); the
  empty E transfers identically (no emptiness gate). `S(S-bsp-ws)` +
  `S(S-bsp-wsretain)` + `S(S-bsp-wsstay)` + `S(S-bsp-close)`;
  history-entry queued.
- Then i3: the emptied source shows the most-recent focus-head entry
  (a replacement is created when the moved workspace was last); the
  empty E detaches/attaches the same way (no emptiness gate); leg 1
  shows WS2 on R with the descended focus (B) via the same show chain
  as R-WS-21, the E leg focuses the empty node. `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no workspace-ownership
  move verb exists). `S(S-xmo-ws)`.
- Then sway: detach/attach with source refill (replacement created
  with raw focus when last); the empty E migrates identically (no
  emptiness gate); both legs leave L showing WS1 (leg 1: stacked WS1
  node, else items[0] of the Given order; leg 2: WS1 sole remainder):
  the query resolves through the stacked workspace nodes with no focus
  write; exact focus nodes stay TBD (WS1 content unstated). `S(S-sway-ws)`
  + `S(S-sway-wsactive)`; focus queued.
- Then qtile/Columns: retained (static groups 1-9); both legs swap
  with WS3 (moved and target groups are all screened: L shows WS3, R
  shows the mover); each screen refocuses its new group's remembered
  `current_window` (WS3's remembered window is fixture-unstated, so the
  exact L focus stays TBD). `S(S-qti-ws)` + `S(S-qti-wsdef)`; L-focus
  queued.
- Then awesome/tile: retained (static per-screen tags; only the
  explicit `tag.delete` removes); the empty E reassigns via
  `tag.screen` the same way (screen plus tags rewritten, no emptiness gate); the old screen restores from history
  (the moved tag was selected, so restore runs; history content is
  fixture-unstated, so the exact restored set stays TBD (F: history content fixture-unstated, genuinely read by the restore)). `S(S-awe-ws)`;
  restored-set queued.
- Then niri: cleanup drops empty non-active non-trailing workspaces
  (the trailing empty is spared); source refill is the previous entry,
  so leg 1 shows WS1; the E leg migrates by index the same way
  (activation only when moved-active; E's index is fixture-unstated,
  so its exact predecessor stays TBD). `S(S-nir-ws)` +
  `S(S-nir-wsremove)`; E-predecessor queued.
- Then PaperWM: owner-specific (GNOME owns workspace add/remove;
  PaperWM only mirrors via `workspacesChanged`); the move choreography
  applies; source refill/removal and empty-E migration stay TBD (GNOME
  policy). `S(S-pap-space)`; refill/removal queued.
- Then karousel/Lazy: fixture-inapplicable (single-screen profile has
  no second output). `S(S-kar-single)`.
- Then paneru: no-counterpart (window-only move verb; no whole-strip
  or Space move). `S(S-pan-cmds)`.
- Then Ours KDE: source shows the last remaining scoped entry;
  existing minimum-two/trailing-spare lifecycle converges on topology
  signals; empty migrates under the same id with no fabricated focus.
  Implemented offline (NORMATIVE D5, User 2026-10-08), native lifecycle/focus TBD.
  G-37 User 2026-10-10 keeps this as the default and selects an MRU setting
  using the pre-mutation item-1.2 per-output previous stable ID, eligible
  only while live in the remaining source-output scoped ring (surviving
  empties valid; migrated/removed/out-of-scope excluded). Without an eligible
  entry use last remaining; a still-scoped live current view is kept in both
  modes. Shared selector/KDE KCM/live setting/MRU delivered offline 2026-10-10;
  destination insertion and history invalidation unchanged.
  [Production-entry real-Engine default/MRU fixtures](../../../kwin/tests/g06-g37-entry-discrimination.test.ts),
  [eligibility/config fixtures](../../../kwin/tests/migration-source-refill.test.ts).
  [Discriminating variants](../reference-outcomes.md#2026-10-10-reference-comparison-discriminators).
  [Native map](../../../kwin/src/workspace-native.ts),
  [fixtures](../../../kwin/tests/workspace-migrate.test.ts).
- Then Ours Windows: no-counterpart (no whole-workspace verb).
  `S(S-ours-planops)`.
- Variant hook: NORMATIVE workspace-output migration (User 2026-10-08).

<a id="r-ws-24-float-carry-sticky-stay"></a>
### R-WS-24: float carry and sticky stay

- Given: active WS2 holds tile A plus intentional float F; the
  source output also shows sticky all-desktops S. Strict local/true.
  Fresh leg replaces F with an automatic fixed-size float.
- When: migrate WS2 right; inspect F class/origin/output and S
  output/all-desktops flag.
- Observe: float carry vs fresh admission; sticky move vs stay.
- Then COSMIC: F carries both legs (set_output moves the tiling plus
  floating layers, fixed-size floats included); S stays (per-output-set
  sticky layer, never a workspace member). `S(S-cos-wsmove-fs)` +
  `S(S-cos-sticky)`.
- Then Hyprland/Dwindle: F carries both legs (floating reposition on
  the move); S stays (pinned members are reassigned to the next
  workspace on the old monitor, never carried). `S(S-hyp-wsmove-fs)` +
  `S(S-hyp-pinstay)`.
- Then bspwm: F carries (whole-desktop transfer moves the tree;
  floating uses no tiling space and stays in-tree); S stays on the
  source (stickies move off the transferred desktop back to the
  source's shown remainder, else to the destination's shown desk).
  `S(S-bsp-ws)` + `S(S-bsp-float)` + `S(S-bsp-wsstay)`.
- Then i3: F carries (detach/attach with floating coordinate fix); S
  carries as a member in both cases: tiled stickies never match the
  show-time push filter, and a floating S travels inside the moved
  workspace's floating list, so the source-refill show (which pushes
  floating stickies to the shown workspace) never re-homes it.
  `S(S-i3-wsmove-fs)` + `S(S-i3-sticky)` + `S(S-i3-stickyshow)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no workspace-ownership
  move verb exists). `S(S-xmo-ws)`.
- Then sway: F carries (detach/attach carries the workspace floating
  list); S carries as a member (the sticky pull to the focused
  workspace runs on switches only; the move path writes raw focus, so
  no re-home runs before observation; a later switch would pull a
  floating S). `S(S-sway-wsmove-fs)` + `S(S-sway-sticky)` +
  `S(S-sway-stickypull)`.
- Then qtile/Columns: F carries (`set_screen` pushes floats via
  `to_screen` and shows them); no-counterpart for the sticky leg (no
  sticky state or verb exists in the profiled group/window inventory,
  so sticky stay-vs-carry never runs).
  `S(S-qti-ws)` + `S(S-qti-nosticky)`.
- Then awesome/tile: F carries (`tag.screen` moves all member clients;
  float is a persistent property); S carries as a member too
  (`set_screen` rewrites screen plus tags for every tagged client with
  no sticky guard; sticky only grants visibility on all selected tags,
  never source ownership). `S(S-awe-ws)` + `S(S-awe-float)` +
  `S(S-awe-sticky)`.
- Then niri: F carries (each workspace owns its floating space;
  `set_output` re-enters all windows); no-counterpart for the sticky
  leg (no sticky state or verb in the profiled `Action` inventory).
  `S(S-nir-wsmove)` + `S(S-nir-ws)` + `S(S-nir-acts)`.
- Then PaperWM: the automatic fixed-size premise is unsupported here
  (`add_filter` admits Normal non-transient windows only with no
  fixed-size branch, so a fixed-size Normal window tiles as an ordinary
  member, never `_floating`); the intentional leg maps to toggle-scratch
  (stuck on all workspaces, never a space member: the move dance writes
  no stuck state and `insertWindow` early-returns for on-all-workspaces
  windows, so S stays and is not a migrated member with its flag
  untouched). Tiled members (A, plus fixed-size-as-tiled) carry as
  same-space members via `setMonitors`/`setMonitor`. Native float-frame
  carry across monitors has no extension frame write in the path (layout
  skips `unMovable`, floats are only shown, scratch geometry moves only
  on toggle), so float output/frames plus stuck visibility stay TBD
  (host). `S(S-pap-space)` + `S(S-pap-float)` + `S(S-pap-unmov)`;
  frames/visibility queued (host).
- Then karousel/Lazy: fixture-inapplicable (single-screen profile has
  no second output). `S(S-kar-single)`.
- Then paneru: no-counterpart (window-only move verb; unmanaged floats
  never migrate as members). `S(S-pan-cmds)` + `S(S-pan-flt)`.
- Then Ours KDE: F carries with class/origin preserved via native
  output remap (never fresh-tiled); S stays on the source and is
  never a member. Implemented offline (NORMATIVE D7, User 2026-10-08),
  native geometry TBD.
  [Adapter](../../../kwin/src/workspace-send-adapter.ts),
  [engine fixtures](../../../kwin/tests/workspace-migrate-engine-fixture.test.ts).
- Then Ours Windows: no-counterpart (no whole-workspace verb).
  `S(S-ours-planops)`.
- Variant hook: NORMATIVE workspace-output migration (User 2026-10-08).

<a id="r-ws-25-overlay-refusal"></a>
### R-WS-25: overlay carry (fullscreen/maximized)

- Given: fresh legs with (a) a fullscreen migrating member, (b) a
  maximized migrating member, (c) a fullscreen/maximized client in
  the affected target current view. Strict local/true.
- When: migrate right; inspect all native writes and focus.
- Discriminating legs: make the fullscreen member active; use different output
  work areas and delayed native re-fit. Separately enter fullscreen during a
  member transfer, while awaiting arrival, and immediately before follow.
- Observe: whole carry vs whole no-write refusal; extra
  size/position/focus writes while fullscreen; retained maximize slot after
  native unmaximize on the target; native fullscreen focus retention (TBD).
- Then COSMIC: carries: `Workspace::set_output` moves the tiling plus
  floating layers, all mapped, minimized, and active fullscreen surfaces
  to the new output with no refusal gate. `S(S-cos-wsmove-fs)`;
  maximized moves as a mapped member (no separate gate claimed).
- Then Hyprland/Dwindle: carries: the workspace move reassigns every
  member window to the new monitor, repositions floating windows and
  resizes fullscreen windows to the new monitor box with no refusal gate.
  `S(S-hyp-wsmove-fs)`; maximized members move as members (no separate
  gate traced).
- Then bspwm: carries fullscreen as members (whole-desktop transfer;
  fullscreen toggles vacant in place with the tree slot kept; no
  overlay gate in the transfer path); no maximize state exists so no
  maximize claim; delayed re-fit and mid-flight legs stay TBD (native
  runtime). `S(S-bsp-ws)` + `S(S-bsp-state)` + `S(S-bsp-fs)`; re-fit
  queued.
- Then i3: carries fullscreen: `workspace_move_to_output` detaches and
  attaches the whole workspace with floating coordinate fix and no overlay
  gate; `workspace_show` manages CF_OUTPUT fullscreen state.
  `S(S-i3-wsmove-fs)`; no maximize state exists so no maximize claim.
- Then xmonad/Tall+Navigation2D: no-counterpart (no workspace-ownership
  move verb exists; no maximize state in this profile). `S(S-xmo-ws)` +
  `S(S-xmo-layout)`.
- Then sway: carries fullscreen: `workspace_move_to_output` detaches and
  attaches the whole workspace with source refill and no overlay gate;
  arrange sets the fullscreen container to the output geometry.
  `S(S-sway-wsmove-fs)`; no maximize state claimed.
- Then qtile/Columns: carries as members (view-ownership reassignment
  via `toscreen`/`set_screen` incl floating show; fullscreen/maximized
  are window float states with no separate move gate traced); delayed
  re-fit and mid-flight legs stay TBD (client/native runtime).
  `S(S-qti-ws)` + `S(S-qti-fs)`; re-fit queued.
- Then awesome/tile: carries as members (`tag.screen` moves all member
  clients with screen plus tags rewritten; fullscreen/maximized are plain boolean properties with no
  move gate traced); delayed re-fit and mid-flight legs stay TBD
  (L: client/native runtime). `S(S-awe-ws)` + `S(S-awe-fs)`; re-fit queued.
- Then niri: carries as workspace members: `move_workspace_to_output_by_id`
  removes/inserts the whole workspace with activation only when
  moved-active and no overlay gate; `set_output` re-enters all windows on
  the new output. `S(S-nir-wsmove)`; fullscreen/maximized move as members
  (no overlay-specific branch traced).
- Then PaperWM: fullscreen/maximized members migrate as space members (no
  overlay gate in `move-space-monitor`/`swapMonitor`; layout skips
  placement via `unMovable`, leaving frames alone); no extra
  size/position writes run in the path, but native fullscreen focus
  retention rides host activation and stays TBD (host). `S(S-pap-space)` +
  `S(S-pap-unmov)`; focus queued (host).
- Then karousel/Lazy: fixture-inapplicable (single-screen profile has
  no second output; overlay membership is otherwise kept with arrange
  skipped). `S(S-kar-single)` + `S(S-kar-maxfs)`.
- Then paneru: no-counterpart (no whole-strip or Space move verb;
  `ToNextDisplay` is window-only). `S(S-pan-cmds)`.
- Then Ours KDE: NORMATIVE D8 CHANGED (User 2026-10-08): fullscreen plus
  maximized members are carried with no refusal in moved members or
  affected views; the tiler issues only the native move with no extra
  size/position/focus writes while fullscreen. An explicit user move is not
  unwanted interference. Delivered offline 2026-10-09: maximized overlay and
  reserved slot retained without unmaximize, affected-view overlays allowed;
  native re-fit geometry drift does not invalidate arrival. Immediate live
  guards suppress overlay geometry and fullscreen focus writes. Fullscreen
  follow logs `native-only`, not confirmed native client focus. Frozen-state
  changes still settle/reconcile normally. Exact native focus/re-fit and
  mid-flight mode-change outcomes remain TBD for user testing.
  [Adapter](../../../kwin/src/workspace-send-adapter.ts),
  [fixtures](../../../kwin/tests/workspace-migrate.test.ts),
  [Engine fixtures](../../../kwin/tests/workspace-migrate-engine-fixture.test.ts),
  [record](../../changes/archive/migration-overlay-carry.md).
- Then Ours Windows: no-counterpart (no whole-workspace verb).
  `S(S-ours-planops)`.
- Variant hook: NORMATIVE overlay carry (User 2026-10-08; D8 CHANGED,
  delivered offline, live check pending).

<a id="r-ws-26-history-hotplug-invalidation"></a>
### R-WS-26: history invalidation and hotplug-return removal

- Given: WS2 has been visited on L; a displaced-origin mapping names
  WS2 and a sibling. Strict local/true. Fresh fixture for reconnect.
- When: explicitly select WS2 on L; migrate right; focus L and invoke
  previous. Separately reconnect the displaced origin after migration.
- Observe: previous clearing vs ordinal reuse; return-association
  removal scope; planned vs completed reporting.
- Then COSMIC: no-counterpart (no history-toggle verb exists; `LastWorkspace`
  targets the last index `len-1`, not the last-viewed workspace).
  `S(S-cos-ws)`.
- Then Hyprland/Dwindle: single global MRU timeline (entries persist;
  dead ones pruned by `gc`); the monitor-move path references no
  history tracker and the lifecycle adapter performs no activation outside
  `changeWorkspace`. With L focused through the select, the migrate carries
  focus to R (hence the refocus-L step, which only schedules a deferred
  re-track): the first previous is a proven event-loop race (L) between that
  deferred track and the invocation -- deferred-first switches cross-monitor
  to WS2 on R, invocation-first finds the untracked plug and errors to a
  no-op. Reconnect (premise mapping input) returns WS2 to the origin and
  erases only WS2's return entry (sibling entries return-or-persist
  independently); remembered-active/default selection never consults the
  History tracker. No previous-ID clearing exists (stale monitor tags persist)
  and no planned/completed reporting exists in the move/lifecycle paths. The
  exact post-return activation stays TBD (F: the origin's remembered value
  unstated). `S(S-hyp-ws)` + `S(S-hyp-monlife)`; first-previous race and
  activation queued.
- Then bspwm: transfer drops the moved desktop's history entries and
  adds none for it, so the first `last` after migration skips WS2 to
  the next-older entry (fixture history unstated, so the exact target
  stays TBD); return-association scope and planned/completed reporting
  stay TBD (no such concepts in source). `S(S-bsp-ws)` +
  `S(S-bsp-wshist)`; next-older target queued.
- Then i3: single global `previous_workspace_name`; the move's two
  shows record refill-then-mover names, so invoking previous selects
  the recorded refill entry (exact refill TBD on fixture grounds);
  reconnect return placement and the exact second toggle stay TBD.
  `S(S-i3-ws)`; refill-return/second queued.
- Then xmonad/Tall+Navigation2D: no-counterpart (no back-and-forth or
  previous-workspace verb in the profiled key inventory). `S(S-xmo-ctl)`.
- Then sway: per-seat single `prev_workspace_name`; the move itself
  records nothing (raw focus only, never the recording switch path),
  so previous still names the pre-select workspace (exact target TBD,
  W_prev unstated); reconnect affinity and the exact second toggle
  stay TBD. `S(S-sway-ws)` + `S(S-sway-evac)`; target/second queued.
- Then qtile/Columns: per-screen `previous_group`; the cross-screen
  swap writes previous only on the requesting screen (R records WS3;
  L's record still names the pre-select group), so invoking previous
  on L shows the pre-select group (exact TBD, W_prev unstated); return
  scope stays TBD. `S(S-qti-ws)`; pre-select/return queued.
- Then awesome/tile: per-screen tag history; the migration-time `tag.screen` move restores L via `history.restore(L,1)`, so the refill view stays TBD (F: L visit history before the WS2 select is unstated, genuinely read by the restore); the previous-toggle restores the stored pre-move {WS2} set, whose entries select under an activated-plus-any-screen check with no same-screen gate, so the R-homed WS2 is selected and joins R's shown set while L is left empty; the R-side refocus target stays TBD (F: R focus history unstated, read by the selection-change path). Return scope needs no removal: no hotplug-return association store exists in the traced path (the removed handler carries no affinity write; added screens mint fresh tags). `S(S-awe-ws)` + `S(S-awe-hist)`.
- Then niri: per-monitor `previous_workspace_id`; neither the remove
  nor the insert path writes it, so previous still names the pre-select
  entry and invoking it shows that entry (exact TBD, W_prev unstated;
  an off-monitor id resolves to no-op); return scope and the exact
  second toggle stay TBD. `S(S-nir-ws)` + `S(S-nir-wsremove)`;
  pre-select/return queued.
- Then PaperWM: live-computed MRU walk (no recorded previous ID to
  invalidate); invoking previous steps DOWN to the next-older live
  entry (exact TBD, live order unstated); GNOME owns add/remove, so
  return scope and further toggles stay TBD. `S(S-pap-space)`;
  entry/return queued.
- Then karousel/Lazy: no-counterpart (no desktop-switch or history
  verb in the Actions inventory; desktop switching is KWin-native).
  `S(S-kar-acts)`.
- Then paneru: no-counterpart (no history verb in the `Operation`
  inventory; `Virtual` is directional North/South only).
  `S(S-pan-ws)`.
- Then Ours KDE: previous IDs that leave the recording output scope
  clear, with no ordinal reinterpretation or recreation; only the moved id drops from the
  hotplug-return associations (siblings kept); the Engine `planned`
  reply is a retained rekey, and completion reports only verified
  arrival/views/focus with correlated partial/uncertain/recovery
  terminals. Implemented offline (NORMATIVE D9, User 2026-10-08), native/hotplug TBD.
  [Native map](../../../kwin/src/workspace-native.ts),
  [adapter](../../../kwin/src/workspace-send-adapter.ts),
  [fixtures](../../../kwin/tests/workspace-migrate.test.ts).
- Then Ours Windows: no-counterpart (no whole-workspace verb).
  `S(S-ours-planops)`.
- Variant hook: NORMATIVE workspace-output migration (User 2026-10-08).

<a id="r-ws-27-two-candidate-migration-selection"></a>
### R-WS-27: two-candidate whole-workspace migration selection

- Given: L `(0,1080,1920,1080)` shows active WS2 `H[A,B*]`, B's centre
  at x=1600; U1 `(0,0,1200,1080)` and U2 `(1200,0,720,1080)` both touch
  L's upper edge with positive overlap. U1 has the larger shared edge;
  B's centre projects onto U2, distinguishing workspace from window targeting.
  Full topology readable; strict local/true. Fresh reset per leg.
- When: migrate WS2 up once.
- Observe: selection of one candidate vs refusal; views/focus on move.
- Then COSMIC: selects one upper output without ambiguity refusal via
  `MigrateWorkspaceToOutput(Up)` (`next_output` keeps minimum origin
  distance, ties keep the first enumerated); exact U1/U2 TBD
  (enumeration unrecorded; both overlap with equal 1080 distance). It
  activates there and switches output; exact focused window and target
  views stay TBD. `S(S-cos-ws)` + `S(S-cos-move-out)`;
  identity/focus queued.
- Then Hyprland/Dwindle: selects U1 via `movecurrentworkspacetomonitor
  up` (directional monitor query: edge-stick within 2px plus longest
  shared x-intersection, U1 1200 vs U2 720; longest wins with no ambiguity
  refusal, a no-candidate direction errors instead). This
  matches the project's largest-edge leg here; source equal-edge ties
  retain the first enumerated, not an established left/top rule. With L
  focused (B* focused there) the destination activates (U1 shows the moved
  WS2) and the source gap-plugs a freshly created first-free-numbered
  workspace (no other L workspace in the fixture); the move writes no window
  focus, so B retains object focus. `S(S-hyp-ws)` + `S(S-hyp-mondir)`
- Then bspwm: selects one upper output via `desktop -m north`
  (`MONITOR_SEL` DIR; `nearest_monitor` keeps minimum boundary
  distance, no ambiguity gate); exact U1/U2 TBD (both qualify under
  HIGH tightness with distance 1; monitor list order unrecorded).
  Follow/stay flag fixture-unstated, so destination focus stays TBD;
  transfer drops the moved desktop's history entries. `S(S-bsp-ws)` +
  `S(S-out07-bsp-mon)` + `S(S-bsp-mondir)` + `S(S-bsp-wsstay)` +
  `S(S-bsp-wshist)`; identity/focus queued.
- Then i3: `move workspace to output up` resolves directionally (the output word `up` maps via `get_output_next_wrap` to the closest overlapping output above, wrapping to the farthest opposite when none; U1/U2 both overlap with equal y, so first-enumerated wins). The up-migration runs via `workspace_move_to_output` with source refill when last and destination `workspace_show` focusing the moved B. Exact U1/U2 identity and displaced views stay TBD (F: output list order and U1/U2 contents unstated). `S(S-i3-ws)` + `S(S-i3-wsdir)`; identity/views queued.
- Then xmonad/Tall+Navigation2D: no-counterpart (no workspace-ownership
  move verb exists; `shift`/`shiftWin` take explicit tags only, and no
  directional workspace verb exists in the profiled inventory, so migration selection never runs).
  `S(S-xmo-ws)` + `S(S-xmo-ctl)`.
- Then sway: selects the adjacent output via `move workspace to output
  up` (`output_in_direction` tries the wlroots adjacent output, else
  the farthest-opposite fallback; NULL only when none); exact U1/U2
  TBD (wlroots tie-break untraced; the ref point is the workspace
  centre x=960, not B's centre). Moved focus and L-shown entry stay
  TBD. `S(S-sway-ws)` + `S(S-sway-wsdir)`; identity/views queued.
- Then qtile/Columns: no-counterpart (no output-directional
  whole-group verb exists: `toscreen` takes an explicit screen while
  `next/prev_group` resolve relatively among groups, so the up-migration
  selection never runs and views and focus stay unchanged).
  `S(S-qti-ws)` + `S(S-qti-wsdef)`.
- Then awesome/tile: no-applicable-journey (no directional whole-tag
  migration verb exists: `tag.screen` takes an explicit screen, and
  `get_next_in_direction` drives view-only `screen.focus_bydirection`, never
  a tag move; the up-migration has no applicable tile journey, so views and focus stay unchanged). `S(S-awe-ws)` +
  `S(S-awe-wsdir)`.
- Then niri: selects one upper output via `MoveWorkspaceToMonitorUp`
  (`output_up_of`: full-width vertical-strip overlap plus minimum
  centre-y distance, no refusal); exact U1/U2 TBD (both overlap with
  equal 1080 centre-y distance; output order unrecorded). Moved-active
  activation switches the active monitor; exact focused window and
  views stay TBD. `S(S-nir-ws)` + `S(S-nir-outdir)`;
  identity/focus queued.
- Then PaperWM: directional verb via `move-space-monitor-above`
  (`moveToMonitor` UP with swap fallback when it is the monitor's
  last space); exact U1/U2 TBD (GNOME `get_monitor_neighbor_index`
  untraced; -1 stays). Exact views and focus stay TBD.
  `S(S-pap-space)`; identity/views queued.
- Then karousel/Lazy: fixture-inapplicable (single-screen profile has
  no second output to receive WS2). `S(S-kar-single)`.
- Then paneru: no-counterpart (`ToNextDisplay` moves the focused
  window only, never a whole strip or Space; `VirtualMove` is
  strip-relative). Unsupported outcome established: no whole-strip/Space
  migration verb exists. `S(S-pan-cmds)` +
  `S(S-pan-display)`.
- Then Ours KDE: selected (User decision 2026-10-09): largest shared edge,
  then left/top, so select U1 despite B projecting onto U2; unreadable topology
  refuses, no candidate no-op, no wrap.
  Implemented offline 2026-10-09: shared selector and KDE largest-edge/equal-edge
  regressions, real-Engine multi-candidate migration. Native journey TBD.
  [Record](../../changes/archive/position-based-output-selection.md).
- Then Ours Windows: no-counterpart (no whole-workspace verb); same
  selected largest-shared-edge then left/top target, implementation pending.
  `S(S-ours-planops)`.
- Variant hook: NORMATIVE workspace-output migration (User decision 2026-10-09).
