# Move (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

## 2. Move

<a id="scrolling-backfill-additive-existing-wide-tables-above-unchanged"></a>
<a id="r-mov-01-backfill-perpendicular-move-of-a-flat-triple-scrolling"></a>
### R-MOV-01: perpendicular move of a flat triple

- Given (tree profiles): `H[A,C,B*]`

- Given (column profiles): `COL[C1[A],C2[C],C3[B*]]`, three single-window columns
  in strip order; shipped defaults apply. This is a native model leg,
  not an equal-third rectangle projection; there is no shared column.

- When: Move B down.

- When (column leg): move B down. Native verbs: niri `MoveWindowDown`, karousel
  `windowMoveDown`, paneru `Swap(South)`; PaperWM down-move inventory
  unresolved.

- Observe: Restructure vs no-op

- Observe (column leg): perpendicular restructure vs in-column reorder vs stay.

- Then COSMIC: `V[H[A,C],B]` via R1; `UT(2026-08-20)` ver-unknown + [S1-07](../../cosmic-move-conformance.md#sequence-s1---three-terminals)
- Then Hyprland/Dwindle: Semantic remove+reinsert move, not swap (swap is a separate action erroring with no target); flat 3-child start has no ordinary binary form (default ratio 1 yields halves, not thirds); exact outcome TBD (ratios, focal anchor, geometry recalc); `S(S-hyp-move)` + `S(S-hyp-moveswap)`
- Then bspwm: Configured `-s south --follow` is node swap, not R1; no south target in a single row, so no swap occurs and tree/focus stay unchanged; `S(S-bsp-swap)` + `S(S-bsp-flt-focus)`
- Then i3: `V[H[A,C],B*]`: no same-orientation parent, so the workspace force-wraps to V, then B inserts after the H at workspace level; focus stays B; `S(S-i3-move)`
- Then xmonad/Tall+Navigation2D: fixture-inapplicable for the exact foreign Given (F: flat 3-child H has no ordinary Tall binary form - Tall `frac=1/2` master/stack, no thirds - so the native Tall projection of the fixture is unrecorded and the exact outcome stays TBD). Conditional analogue only: in native Tall order `[A,C,B]` (A master left half, C/B right half split equally) `windowSwap` D `False` from bottom-stack B has no below candidate under tiled hybrid line/side with wrap False, so the analogue miss is no-op with tree/focus unchanged; core stack verbs are not directional; `S(S-xmo-layout)` + `S(S-xmo-nav)` + `S(S-xmo-core-nav)`
- Then sway: `V[H[A,C],B*]`: no parallel V parent, so the workspace force-wraps to V, then B inserts after the H at workspace level; focus stays B; `S(S-sway-move)`
- Then qtile/Columns: Flat 3-child start has no ordinary Columns form (default num_columns=2, third window stacks in-column); exact outcome TBD. Policy: shuffle_down reorders within the column only (edge is no-op), no R1 restructure; `S(S-qti-shuffle)` + `S(S-qti-add)`
- Then awesome/tile: Flat 3-child start has no ordinary tile form (master plus one vertical stack column, not flat thirds); exact outcome TBD. Policy: semantic move is geometric swap.bydirection with no R1 restructure (miss is no-op); `S(S-awe-swap)` + `S(S-awe-tile)`
- Then niri: B is the sole tile, so `move_down` returns false and B stays.
  `S(S-nir-move)`.
- Then PaperWM: registered `move-down` binds same-space `swap`; B is the sole row of C3, so the down step is out-of-range and returns with B staying (no model or selection change). `S(S-pap-moveverbs)` + `S(S-pap-swap)`.
- Then karousel/Lazy: B is the sole window, so `windowMoveDown` is a
  no-op and B stays. `S(S-kar-move)`.
- Then paneru: B stays (Single-column B has no South peer; single output,
  so no display fall-through); tree and focus unchanged.
  `S(S-pan-swap-peer)` + `S(S-pan-swap)`.
- Then Ours KDE: `V[H[A,C],B]` R1 via shared Engine; selected `D(D-dec-cos)`
- Then Ours Windows: `V[H[A,C],B]` R1 via shared Engine; selected `D(D-dec-cos)`
- Variant hook: V-MOVE-PERP.

<a id="r-mov-02-backfill-in-group-vertical-swap-scrolling"></a>
### R-MOV-02: in-group vertical swap

- Given (tree profiles): `H[A,V[C,B*]]`

- Given (column profiles): `COL[C1[A],C2[C,B*]]` with C above B, both visible
  where supported; shipped defaults apply.

- When: Move B up.

- When (column leg): move B up. Native verbs: niri `MoveWindowUp`, karousel
  `windowMoveUp`, paneru `Swap(North)`; PaperWM up-move inventory
  unresolved.

- Observe: Swap vs wrap

- Observe (column leg): B swaps above C in place.

- Then COSMIC: `H[A,V[B,C]]` via R2a leaf swap; `UT(2026-08-20)` + [S1-11](../../cosmic-move-conformance.md#sequence-s1---three-terminals)
- Then Hyprland/Dwindle: Binary-compatible start; yields `H[A,V[B,C]]`: focal 1px above B sits inside C's expanded full-V box (distance 0; ideal-BB reserved expansion at work-area edges stays within C's span, strictly closest either way), direct-partner override orders B first with top/bottom split at default ratio 1 (halves); focus stays B (non-silent); V persists through recalc under the same portrait condition the starting `V[C,B]` exhibits; `S(S-hyp-move)`
- Then bspwm: Configured `-s north --follow` swaps B/C (same-desktop swap retains focus on B; `--follow` inert here); `S(S-bsp-swap)` + `S(S-bsp-flt-focus)`
- Then i3: `H[A,V[B*,C]]`: in-parent leaf swap with C; focus stays B; `S(S-i3-move)`
- Then xmonad/Tall+Navigation2D: `H[A,V[C,B]]` is Tall's projected geometry for StackSet `[A,C,B*]` (`nmaster=1`, `frac=1/2`: master A takes the left half via `splitHorizontallyBy`, stack C/B takes the right half split equally via `splitVertically`), not a structural tree; `windowSwap` U `False` from B selects C by tiled line/side geometry (C sits above sharing the x-range while A spans full height and fails the above test), and `swap` exchanges stack positions retaining mover focus (StackSet `[A,B*,C]`, projected `H[A,V[B*,C]]`). `S(S-xmo-layout)` + `S(S-xmo-nav)` + `S(S-xmo-out)`
- Then sway: `H[A,V[B*,C]]`: in-parent leaf swap with C; focus stays B; `S(S-sway-move)`
- Then qtile/Columns: Column-embedding analogue: B below C in one column, shuffle_up swaps B above C with focus retained (no focus write on the shuffle_up path); `S(S-qti-shuffle)`
- Then awesome/tile: Tile-projected start for order [A,C,B] (master A, stack C/B); geometric swap up selects C by strict-origin halfplanes with nearest geometric rect (C above sharing the x-range at distance 0 beats full-height A), yielding order [A,B,C] projected `H[A,V[B*,C]]` with mover focus retained (swap exchanges list positions with no focus write). `S(S-awe-swap)` + `S(S-awe-tile)` + `S(S-awe-geodir)`
- Then niri: B swaps with C and the active index follows B, so B stays
  focused. `S(S-nir-move)`.
- Then PaperWM: registered `move-up` binds same-space `swap`; B swaps above C in place within C2 with the selection staying on B (no focus write on the swap path), followed by layout and forced viewport. `S(S-pap-moveverbs)` + `S(S-pap-swap)`.
- Then karousel/Lazy: B swaps above C with no focus write, so focus stays
  B. `S(S-kar-move)`.
- Then paneru: B swaps with C above in the same strip. `S(S-pan-move)`.
- Then Ours KDE: `H[A,V[B,C]]`; `D(D-dec-cos)`
- Then Ours Windows: `H[A,V[B,C]]`; `D(D-dec-cos)`
- Variant hook: V-MOVE-NARY.

<a id="r-mov-03-backfill-same-row-carry-to-the-right-scrolling"></a>
### R-MOV-03: same-row carry to the right

- Given (tree profiles): `H[A,B*,C,D]`

- Given (column profiles): `COL[C1[A],C2[B*],C3[C],C4[D]]`, four single-window
  columns in strip order; shipped defaults apply. This is a native model
  leg, not an equal-quarter rectangle projection.

- When: Move B right.

- When (column leg): move B right. Native verbs: niri `MoveColumnRight`, karousel
  `windowMoveRight`, paneru `Swap(East)`; PaperWM right-move inventory
  unresolved.

- Observe: Wrap pair vs flat insert

- Observe (column leg): pair wrap vs flat reorder, column join, or stay.

- Then COSMIC: `H[A,H[B,C],D]` R2c; [S18-01](../../cosmic-move-conformance.md#sequence-s18---r2c-container-neighbour) authored observation, widths unrecorded
- Then Hyprland/Dwindle: Flat 4-child start has no ordinary binary form (default ratio 1 yields halves, not quarters); wrap-vs-insert anchor TBD (focal, ratios, geometry recalc); `S(S-hyp-move)`
- Then bspwm: East-neighbor leaf swap with C, projected `H[A,C,B*,D]` with the native binary grouping retained (leaf exchange in place, not N-ary flattening and not a nested wrap; in-order leaves lay out left-to-right under any binary grouping, so adjacent C beats D by boundary distance with history only breaking ties); same-desktop swap retains focus on B with `--follow` inert here; the unrecorded embedding is immaterial to this swap/focus Observe; `S(S-bsp-swap)` + `S(S-bsp-move-target)`
- Then i3: `H[A,C,B*,D]`: flat sibling swap with C, not a nested wrap; focus stays B; `S(S-i3-move)`
- Then xmonad/Tall+Navigation2D: fixture-inapplicable for the exact foreign Given (F: flat 4-child H has no ordinary Tall binary form - flat master/stack only, no wrap-group primitive - so the native Tall projection of the fixture is unrecorded and the exact outcome stays TBD). Conditional analogue only: in native Tall order `[A,B,C,D]` (A master left half, B/C/D right column) `windowSwap` R `False` from top-stack B has no right candidate under tiled hybrid line/side with wrap False, so the analogue miss is no-op with tree/focus unchanged; `S(S-xmo-layout)` + `S(S-xmo-nav)`
- Then sway: `H[A,C,B*,D]`: flat sibling swap with C, not a nested wrap (separate `swap` verb unused); focus stays B; `S(S-sway-move)`
- Then qtile/Columns: Flat 4-child start has no ordinary Columns form; exact outcome TBD. Policy: shuffle_right carries B into the adjacent column (new column at a shared-column edge, no-op only for a sole-column sole window); focus stays B; `S(S-qti-shuffle)`
- Then awesome/tile: Flat 4-child start has no ordinary tile form (1 master plus 3 in one column); exact outcome TBD. Policy: geometric swap-or-miss with no nested wrap/insert; focus retained; `S(S-awe-swap)` + `S(S-awe-tile)`
- Then niri: C2 moves after C3, B stays focused. `S(S-nir-move)`.
- Then PaperWM: registered `move-right` binds same-space `swap`;
  columns C2/C3 exchange (order A,C,B,D) with the selection staying on B
  (no focus write on the swap path), followed by layout and forced viewport.
  `S(S-pap-moveverbs)` + `S(S-pap-swap)`.
- Then karousel/Lazy: single-window B joins C3 at the bottom via the
  single-window path. `S(S-kar-move)`.
- Then paneru: B swaps east with C in the same strip. `S(S-pan-move)`.
- Then Ours KDE: `group-with-neighbor` default gives `H[A,H[B,C],D]`; global
  `sameAxisMove=swap-with-neighbor` gives `H[A,C,B*,D]`, shares travel with windows.
  Shared Engine/protocol and KDE KCM/live reread delivered offline;
  functional IDs [delivered offline](../../changes/archive/admission-and-move-settings.md). Native journey pending;
  `D(D-dec-cos)` + USER 2026-10-07 item 3.
- Then Ours Windows: `group-with-neighbor` default wraps `H[A,H[B,C],D]`;
  live `core.same_axis_move=swap-with-neighbor` gives `H[A,C,B*,D]`, shares/focus
  travel. Windows adapter delivered 2026-10-11, base `fafcd31` + delivery commit
  ([record](../../changes/archive/windows-same-axis-move.md)): retained Engine H4
  right/left tests and native gates passed; agent-observed Settings Apply/Revert
  and Apply-alone no-layout-change passed. Physical move journey pending;
  `D(D-dec-cos)`.
- Variant hook: V-MOVE-NARY.

<a id="r-mov-04-backfill-same-axis-ancestor-escape-scrolling"></a>
### R-MOV-04: same-axis ancestor escape

- Given (tree profiles): `H[H[A,B*],C]`

- Given (column profiles): `H[H[A,B*],C]`; recursive same-axis ancestry has no faithful
  column fixture. Column consume/expel mechanics belong to R-COL-03.

- When: Move B right.

- When (column leg): move B right.

- Observe: Escape vs stay nested

- Observe (column leg): escape vs stay nested, as in the original row.

- Then COSMIC: `H[A,B,C]` via R3 ascend + same-axis flatten; `UT(2026-08-20)` ver-unknown + [S1-03](../../cosmic-move-conformance.md#sequence-s1---three-terminals)
- Then Hyprland/Dwindle: No flat escape: removes B then splits C, retaining binary nesting and focus B. Equal halves make C the same shape as the original inner H: if wider than tall, yields `H[A,H[B,C]]`; exact order TBD at the square tie (admission orders by the focal y half, but recalc uses H because only height greater than width selects V). No live-cursor dependence; `S(S-hyp-move)` + `S(S-hyp-ins)`
- Then bspwm: Swaps B east with C to `H[H[A,C],B*]` (C is the immediate east neighbor sharing B's vertical range; boundary distance then history rank); same-desktop swap retains focus on B with `--follow` inert here; `S(S-bsp-swap)` + `S(S-bsp-move-target)`
- Then i3: `H[H[A],B*,C]`: B extracted after the inner H at the outer level; single-child `H[A]` wrapper persists (empty-only close, narrow flatten); focus stays B; `S(S-i3-move)`
- Then xmonad/Tall+Navigation2D: fixture-inapplicable for the exact foreign Given (F: nested `H[H[A,B],C]` has no Tall counterpart - no nesting levels, flat master/stack only, no escape primitive - so the native Tall projection of the fixture is unrecorded and the exact outcome stays TBD). Conditional analogue only: in native Tall order `[A,B,C]` (A master left half, B/C right column) `windowSwap` R `False` from B has no right candidate under tiled hybrid line/side with wrap False, so the analogue miss is no-op with tree/focus unchanged; `S(S-xmo-layout)` + `S(S-xmo-nav)` + `S(S-xmo-out)`
- Then sway: `H[H[A],B*,C]`: B promoted out of the inner H into C's outer sibling list immediately before C; single-child `H[A]` wrapper persists (empty-only reap, redundant-pair squash only); focus stays B; `S(S-sway-move)` + `S(S-sway-cleanup)`
- Then qtile/Columns: No nesting in Columns: shuffle_right carries B into C's column (a shared-column edge creates a new column instead); focus stays B; exact order/frames TBD; `S(S-qti-shuffle)`
- Then awesome/tile: fixture-inapplicable; nested `H[H[A,B],C]` has no tile counterpart (flat tiled-client list, no nesting levels and no escape primitive), so this Given has no applicable tile journey. `S(S-awe-swap)` + `S(S-awe-tile)`
- Then niri: fixture-inapplicable; ordered columns have no nested H
  ancestor to escape. `S(S-nir-move)`.
- Then PaperWM: fixture-inapplicable; column/row membership has no nested
  H ancestor. `S(S-pap-move)`.
- Then karousel/Lazy: fixture-inapplicable; Grid/Column membership has no
  nested H ancestor. `S(S-kar-move)`.
- Then paneru: fixture-inapplicable; the strip/column model has no nested
  H ancestor. `S(S-pan-model)`.
- Then Ours KDE: R3 ascend; selected Extract to same axis, no setting (User 2026-10-12 D12, [decision](../../decisions.md#move-layout-and-output-commands)); native fixture TBD. `D(D-dec-cos)`.
- Then Ours Windows: same selected Extract to same axis, no setting; shared-default verification-only evidence 2026-10-13 ([record](../../changes/archive/windows-item22-verification.md)); native fixture TBD ([D12 decision](../../decisions.md#move-layout-and-output-commands)). `D(D-dec-cos)`.
- Variant hook: V-MOVE-NARY.

<a id="r-mov-05-backfill-edge-move-with-no-left-neighbor-scrolling"></a>
### R-MOV-05: edge move with no left neighbor

- Given (tree profiles): `H[A*,B]` single output, no neighbor

- Given (column profiles): `COL[C1[A*],C2[B]]`, each one window; shipped
  defaults apply; single output.

- When: Move left past edge.

- When (column leg): move A left. Native verbs: niri `MoveColumnLeft`, karousel
  `windowMoveLeft`, paneru `Swap(West)`; PaperWM left-move inventory
  unresolved.

- Observe: No-op vs cross-ws/output

- Observe (column leg): stay vs cross-output/workspace.

- Then COSMIC: Single-output no-op; [S5-01](../../cosmic-move-conformance.md#sequence-s5---output-edge-no-op) + [S17](../../cosmic-move-conformance.md#sequence-s17---single-output-directional-no-ops) authored observations; R4 never reached in UT
- Then Hyprland/Dwindle: No observable change: the off-edge focal still resolves to the same single monitor (nearest fallback), so A is removed (B fills the workspace) then reinserted ahead of B at the beyond-left focal half, not the live cursor, rebuilding equal `H[A,B]` with focus on A (nodes rebuilt, tree and focus identical); `S(S-hyp-move)`
- Then bspwm: No-op: no west target, so the swap refuses with tree and focus unchanged; `S(S-bsp-swap)` + `S(S-bsp-flt-focus)`
- Then i3: No-op: workspace-level H with A first (no left swap) and the parent is the workspace, so the output-directed attempt finds no output on a single output; tree and focus unchanged; `S(S-i3-move)`
- Then xmonad/Tall+Navigation2D: Analogous flat 2-window Tall embedding (A master, B stack side-by-side): west `windowGo`/`windowSwap` from A has no directional target with wrap False, so miss is no-op with tree/focus unchanged; core stack verbs are not directional; `S(S-xmo-layout)` + `S(S-xmo-nav)` + `S(S-xmo-core-nav)`
- Then sway: No-op: workspace-level H with A first (no left swap), and the next-output lookup finds no adjacent output on a single output; tree and focus unchanged; `S(S-sway-move)` + `S(S-sway-outmove)`
- Then qtile/Columns: No-op: leftmost sole-column A has no adjacent column and no shared column to split, so shuffle_left returns with tree and focus unchanged; `S(S-qti-shuffle)`
- Then awesome/tile: No-op: bydirection miss leaves tree and focus unchanged, and global_bydirection finds no next screen on a single output; `S(S-awe-focus)`
- Then niri: index 0 `move_left` returns false, so A stays.
  `S(S-nir-move)`.
- Then PaperWM: registered `move-left` binds same-space `swap`; A is the first column, so the left step is out-of-range and returns with A staying. `S(S-pap-moveverbs)` + `S(S-pap-swap)`.
- Then karousel/Lazy: no left column on the single-window path, so the
  move returns without acting and A stays. `S(S-kar-move)`.
- Then paneru: A stays (first-column A has no West peer; West/East never
  cross displays); tree and focus unchanged. `S(S-pan-swap-peer)` +
  `S(S-pan-swap)`.
- Then Ours KDE: Local R1/R2/R3 first; no adjacent output in this fixture means
  no-op. Exhausted R4 otherwise crosses in all four directions, never cycles
  workspaces; [item-5 offline record](../../changes/archive/four-direction-output-transfer.md).
- Then Ours Windows: local-only (corrected 2026-10-08 at `db31234`):
  Local R1/R2/R3 first; no adjacent output in this fixture means no-op.
  No Windows crossing wired: single-domain event `cross_output_transfer:false`
  (`crates/tiler-windows/src/tiling_sys.rs:6099/6105`); handoff item 5 plus
  the parked parity-queue multi-output foundation pending. `D(D-dec-cos)` is
  the selected target, not current capability.
- Variant hook: V-R4-DIR.


## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours cells cite the Engine move rules at `9241c94`
(`S(S-ours-move)`); selected intent and doc assertions are never evidence.

### R-MOV-06: move into a nested perpendicular neighbor with remembered child

- Given (tree profiles): `H[A*,V[B,C]]`, V prior focused child C.
  Ordinary windows, no rules, scale 1, zero gaps for reference geometry.
- Given (column profiles): `COL[C1[A*],C2[B,C]]` with B above C, both
  vertically visible where supported; shipped defaults apply. Same
  identities; move-right is the native column right-move per profile.
- When: move A right. Native verbs per profile: COSMIC directional move;
  Hyprland `movewindow r`; bspwm `node -s east --follow`; i3 `move right`;
  xmonad Navigation2D `windowSwap R`; sway `move right`; qtile
  `shuffle_right()`; awesome `swap.bydirection("right")`; niri
  `MoveColumnRight`; karousel `windowMoveRight`; PaperWM inventory TBD;
  paneru `Swap(East)`; Ours Engine `plan_move` Right.
- Observe: enter nested neighbor at remembered child vs swap whole
  neighbor, wrap beside it, or geometric leaf swap. Keep group history
  explicit.
- Then COSMIC: A enters the V group at the geometric middle index 1
  regardless of the remembered child C, yielding `V[B,A*,C]` (sole-child
  root collapse via the single-child flatten); focus stays A (`Done`).
  `S(S-cos-move)`.
- Then Hyprland/Dwindle: A enters the V group at the focal-hit leaf, not the remembered child: the 1px-right focal lands inside V with no single-window partner override (neighbor is a group node), and the override focal resolves via closest-node distance with no MRU anchor, so A splits the focal-hit leaf; no whole-group swap or beside-wrap; focus stays A (non-silent); exact B-vs-C leaf TBD (F: A's mid-height vs the B/C divider plus admission order unrecorded, so remembered-C vs geometric leaf is not established). `S(S-hyp-move)` + `S(S-hyp-ins)`; leaf queued.
- Then bspwm: A swaps with leaf C, not the whole V group; C wins the
  equal-distance tie by history, and focus stays A. `S(S-bsp-move-target)`.
- Then i3: A enters the V group after the remembered C as `V[B,C,A*]` (bordering-branch descend picks the focus-head child C for the RIGHT step into VERT, `AFTER` insert on orientation mismatch via `insert_con_into`); not a whole-group swap or beside-wrap; focus stays A. `S(S-i3-move)`.
- Then xmonad/Tall+Navigation2D: fixture-inapplicable (flat Tall has no
  nested V group). `S(S-xmo-layout)`.
- Then sway: A enters the V group at index 1 before the remembered C as `V[B,A*,C]` (perpendicular reparent to the MRU active child C; not a whole-group swap or beside-wrap); focus stays A. `S(S-sway-move)`.
- Then qtile/Columns: under `COL[C1[A*],C2[B,C]]` sole A carries into C2 via `shuffle_right` (join at C2's current, no whole-neighbor swap or beside-wrap; no focus write, so focus stays A); exact row inside C2 TBD (F: C2's stored current is unrecorded and `insert_position=0` inserts at current). `S(S-qti-shuffle)` + `S(S-qti-add)`.
- Then awesome/tile: tile projection (master A, stack B/C) swaps A with B
  geometrically, focus retained. `S(S-awe-swap)` + `S(S-awe-tile)`.
- Then niri: C1 moves after C2, A stays focused. `S(S-nir-move)`.
- Then PaperWM: registered move-right binds same-space `swap`
  (`move_to` is viewport-only, not membership); A swaps columns with C2
  (order [B,C],[A]) with the selection staying on A, followed by layout
  and forced viewport - no join into C2 (that is the separate slurp verb).
  `S(S-pap-moveverbs)` + `S(S-pap-swap)` + `S(S-pap-move)`.
- Then karousel/Lazy: A joins C2 at the bottom via the single-window path.
  `S(S-kar-move)`.
- Then paneru: C1 moves after C2 (East resolves to the right neighbour B,
  top of the B/C stack via at-or-last); columns exchange with focus staying
  on A (no focus write in the swap path). `S(S-pan-swap-peer)` +
  `S(S-pan-swap)`.
- Then Ours KDE: A leaves the root and enters V at index 1; tree
  `V[B,A*,C]`, focus A. `S(S-ours-move)`. Selected Geometric middle default;
  two-option setting post-0.1 ([User 2026-10-12 D09](../../decisions.md#move-layout-and-output-commands)); native fixture TBD.
- Then Ours Windows: same `V[B,A*,C]` insert as Ours KDE via the shared
  Engine. `S(S-ours-move)`; same D09 selection, verification-only offline
  2026-10-13 ([record](../../changes/archive/windows-item22-verification.md));
  native fixture TBD.
- Variant hook: V-MOVE-NARY.

### R-MOV-07: orthogonal escape across a perpendicular parent

- Given (tree profiles): `V[H[A,B*],C]`. Ordinary windows, no rules,
  scale 1, zero gaps for reference geometry.
- Given (column profiles): none exists. Side-by-side A,B with full-width
  C below has no column counterpart, so there is no faithful column
  Given; scrolling Thens below are fixture-inapplicable qualifications.
- When: move B down. Native verbs per profile: COSMIC directional move;
  Hyprland `movewindow d`; bspwm `node -s south --follow`; i3 `move down`;
  xmonad Navigation2D `windowSwap D`; sway `move down`; qtile Columns
  model inapplicable below; awesome `swap.bydirection("down")` in the
  tile projection; Ours Engine `plan_move` Down.
- Observe: escape across an orthogonal parent vs move into C, swap, or
  carry group. R-MOV-04 already covers same-axis escape.
- Then COSMIC: B wraps with the perpendicular parent via the R1
  orientation-mismatch fork (new Horizontal group beside the inner H),
  yielding `V[H[A,B*],C]` after single-child flatten; focus stays B
  (`Done`). `S(S-cos-move)`.
- Then Hyprland/Dwindle: B escapes the inner H and splits C: the 1px-down focal lands inside C (no single-window partner override on the side-by-side parent), so removal promotes A and B re-admits at C's node; no swap or group carry; focus stays B (non-silent). `S(S-hyp-move)` + `S(S-hyp-ins)`.
- Then bspwm: B swaps with the south leaf C, yielding `V[H[A,C],B*]`;
  focus stays B. `S(S-bsp-move-target)`.
- Then i3: B escapes H to the outer V with R1 continuation, focus stays B.
  `S(S-i3-move)`.
- Then xmonad/Tall+Navigation2D: fixture-inapplicable (flat Tall has no
  nesting levels). `S(S-xmo-layout)`.
- Then sway: B escapes H to the outer V with R1 continuation, focus stays
  B. `S(S-sway-move)`.
- Then qtile/Columns: fixture-inapplicable (side-by-side H has no Columns
  counterpart; down verbs step in-column only). `S(S-qti-shuffle)` +
  `S(S-qti-add)`.
- Then awesome/tile: tile projection (master A, stack B/C) swaps B down
  with C geometrically, focus retained. `S(S-awe-swap)` + `S(S-awe-tile)`.
- Then niri: fixture-inapplicable (no column counterpart for side-by-side
  A,B under spanning C). `S(S-nir-move)`.
- Then PaperWM: fixture-inapplicable (same missing counterpart).
  `S(S-pap-move)`.
- Then karousel/Lazy: fixture-inapplicable (same missing counterpart).
  `S(S-kar-move)`.
- Then paneru: fixture-inapplicable (same missing counterpart).
  `S(S-pan-model)`.
- Then Ours KDE: B leaves the inner H with A as the sole remainder, and
  the inner H is replaced by V[A,B] mover-last; tree `V[V[A,B*],C]`,
  focus B. `S(S-ours-move)`. Selected Wrap default; two-option setting
  post-0.1 ([User 2026-10-12 D10](../../decisions.md#move-layout-and-output-commands)); native fixture TBD.
- Then Ours Windows: same `V[V[A,B*],C]` wrap as Ours KDE via the shared
  Engine. `S(S-ours-move)`; same D10 selection, verification-only offline
  2026-10-13 ([record](../../changes/archive/windows-item22-verification.md));
  native fixture TBD.
- Variant hook: V-MOVE-NARY.

### R-MOV-08: exhausted vertical move across stacked outputs

- Given (all profiles): `U=H[X]`, `L=V[A*,B]`; U directly above L.
  Ordinary windows, no rules, single workspace per output. Fresh mirrored
  down-edge leg only if asymmetric.
- Given (column profiles): same output split with `U=COL[CX[X]]` and
  `L=COL[C1[A*,B]]` at shipped defaults; viewport recorded per output.
- Given (layout-driven native legs): xmonad/Tall and awesome/tile use
  A as left master and B as right stack on L, with X alone on U.
  This is not a V projection; A is at the upper edge in either layout.
  qtile prepares one visible column A above B. `S(S-xmo-layout)` +
  `S(S-awe-tile)` + `S(S-qti-shuffle)`.
- When: move A up. Native verbs per profile: COSMIC directional move;
  Hyprland `movewindow u`; bspwm `node -s north --follow`; i3 `move up`; xmonad
  Navigation2D `windowSwap U`; sway `move up`; qtile `shuffle_up()`;
  awesome `swap.global_bydirection("up")`; niri `MoveWindowUp`; PaperWM
  up-move inventory below; karousel inapplicable below; paneru
  `Swap(North)` inventory below; Ours Engine `plan_move` Up.
  Explicit output-transfer verbs are not this leg (R-OUT-04 covers those).
- Observe: exhausted vertical move crosses output vs stays/restructures
  locally. Ours item 5 extends R4 to Up/Down after local exhaustion.
- Then COSMIC: A has no in-tree move (V edge), so `move_current_node`
  returns `MoveFurther`; at default `Vertical` Up first attempts
  `MoveToPreviousWorkspace` and only from the first workspace falls to
  `MoveToOutput(Up)`, which selects U on full-output overlap plus
  nearest origin distance and transfers with follow. Cross-vs-local TBD
  (F: source workspace index unspecified) and exact admission TBD (F:
  destination contents unrecorded). `S(S-cos-move)` + `S(S-cos-move-out)`.
- Then Hyprland/Dwindle: crosses to U via the monitor fallback.
  `S(S-hyp-move)`.
- Then bspwm: A swaps north with X across outputs, yielding `U=A*` and
  `L=V[X,B]`; `--follow` focuses A on U. `S(S-bsp-move-target)`.
- Then i3: crosses to U via the output-directed fallback with mover
  follow. `S(S-i3-move)` + `S(S-i3-outmove)`.
- Then xmonad/Tall+Navigation2D: profile move is `windowSwap` U `False`
  (same tiled layer across all visible screens via `navigableWindows`/`sortedScreens`;
  `windowToScreen` is the separate carry verb, not exercised). Tiled selection is
  hybrid line/side with stack-order tie preference and no refusal branch: hit swaps
  stack positions across screens with mover focus retained, miss is a no-op (wrap False);
  empty-workspace branch is no-op (no screen fallback, unlike `windowGo`). Exact hit-vs-miss
  TBD (F: output geometry unrecorded - U/L rects and alignment selecting whether sole-X
  full-screen U contains A's center-x - so the U candidate has no basis).
  `S(S-xmo-nav)` + `S(S-xmo-out)` + `S(S-xmo-scope)`; queued.
- Then sway: crosses to the output above via the directional attach path.
  `S(S-sway-move)` + `S(S-sway-outmove)`.
- Then qtile/Columns: A is first in C1, so `shuffle_up` is an edge no-op
  and A stays. `S(S-qti-shuffle)`.
- Then awesome/tile: crosses to U via `swap.global_bydirection`.
  `S(S-awe-swap)`.
- Then niri: A is first in C1, so `move_up` returns false and A stays.
  `S(S-nir-move)`.
- Then PaperWM: verb-distinguished. Same-space `move-up` is `swap` edge no-op (A is the first row of C1, out-of-range returns) so A stays in L; explicit `move-monitor-above` carries via `switchMonitor` neighbor index (-1 stays) to U with focus. `S(S-pap-moveverbs)` + `S(S-pap-mon)`.
- Then karousel/Lazy: fixture-inapplicable for cross-output (single-screen
  profile). `S(S-kar-base)`.
- Then paneru: A has no North peer (stack-top, nothing above), so the
  no-peer fall-through carries A to U via `ToNextDisplay` (Follow): A appends
  after X on U's selected strip with width-ratio preserved and the mouse
  warped; focus stays A; source collapses to B. `S(S-pan-swap-peer)` +
  `S(S-pan-swap)` + `S(S-pan-display)`.
- Then Ours KDE: A crosses up after local exhaustion; source becomes B,
  destination `V[X,A*]` places A nearest the source. Full-rectangle adjacency,
  work-area placement and arrival/follow fences delivered offline;
  [record](../../changes/archive/four-direction-output-transfer.md). Native journey pending.
- Then Ours Windows: four-direction R4 crossing delivered offline in the
  [foundation](../../changes/windows-multi-output-foundation.md); shared
  selector audit/regressions `fda618f`, 2026-10-14
  ([record](../../changes/archive/windows-position-based-output-selection.md)).
  Nearest-source landing/work-area placement retained; physical journey TBD.
- Variant hook: V-R4-DIR.

## Explicit-swap fresh legs (additive reuse; no duplicated start/action row)

Each leg replays the explicit swap verb from the same fixture as the
named move row, from a fresh fixture. Move-vs-swap reuse per the
expansion record; outcomes are qualified legs, not second scenarios.

### R-MOV-01 explicit-swap leg: swap B down from `H[A,C,B*]`

- When (fresh): the profile's explicit swap verb with its native target
  resolution. Hyprland `swapInDirection down` (directional focal target);
  bspwm `node -s south` (directional node target); i3/sway
  `swap container with mark|con_id` naming C (targeted only, no
  directional form); xmonad `windowSwap D` in the Tall projection
  (master A, stack C/B); qtile standalone swap inventory below;
  awesome `swap.bydirection("down")` in the tile projection; paneru
  `Swap(South)`; COSMIC/PaperWM swap inventory unresolved; niri,
  karousel, and Ours have no standalone swap verb (see Thens).
- Observe: swap exchanges vs no-op. Same fixture as R-MOV-01, fresh run.
- Then COSMIC: no-counterpart (`SwapWindow` is an overview grab with no
  directional target form; directional chords are `Move`). `S(S-cos-swap)`.
- Then Hyprland/Dwindle: TBD; flat triple has no ordinary binary swap
  target. `S(S-hyp-moveswap)`.
- Then bspwm: no south target in a single row, so no swap; tree and focus
  unchanged. `S(S-bsp-swap)`.
- Then i3: C named by mark/con_id; B and C exchange.
  `S(S-i3-swap)`.
- Then xmonad/Tall+Navigation2D: nothing geometrically below B in the
  Tall projection, so the miss is a no-op. `S(S-xmo-nav)` +
  `S(S-xmo-layout)`.
- Then sway: C named by mark/con_id; B and C exchange.
  `S(S-sway-move)`.
- Then qtile/Columns: no-counterpart; `swap(c1,c2)` is an internal drag
  helper, not an exposed standalone command. `S(S-qti-swap-inventory)`.
- Then awesome/tile: nothing geometrically below B in the tile
  projection, so the miss is a no-op. `S(S-awe-swap)` + `S(S-awe-tile)`.
- Then niri: no-counterpart (move/consume verbs in `S(S-nir-move)` plus
  the focus-verb inventory in `S(S-nir-actions)` list no swap verb).
- Then PaperWM: the registered `move-down` IS the swap verb (same-space `swap`); sole-row B has no down neighbor, so the swap is an edge no-op and B stays. `S(S-pap-moveverbs)` + `S(S-pap-swap)`.
- Then karousel/Lazy: no-counterpart (window/column moves in
  `S(S-kar-move)` plus the focus verbs in `S(S-kar-focus)` list no swap
  verb).
- Then paneru: B stays (Single-column B has no South peer; single output,
  so no display fall-through); tree and focus unchanged.
  `S(S-pan-swap-peer)` + `S(S-pan-swap)`.
- Then Ours KDE: no-counterpart (every `MoveOperation` is directional;
  exchange occurs only as R2a inside a directional move).
  `S(S-ours-move)`.
- Then Ours Windows: no-counterpart; no standalone swap verb in the shared
  directional operation inventory.
  `S(S-ours-move)`.

### R-MOV-03 explicit-swap leg: swap B right from `H[A,B*,C,D]`

- When (fresh): same per-profile swap inventory as the R-MOV-01 swap leg,
  right direction; i3/sway name C, xmonad and
  awesome use the flat-four projections below.
- Observe: swap exchanges vs no-op. Same fixture as R-MOV-03, fresh run.
- Then COSMIC: no-counterpart (same overview-only `SwapWindow`; no
  directional form). `S(S-cos-swap)`.
- Then Hyprland/Dwindle: TBD; flat four-child start has no ordinary
  binary swap target. `S(S-hyp-moveswap)`.
- Then bspwm: B swaps east with C, focus stays B. `S(S-bsp-swap)` +
  `S(S-bsp-flt-focus)`.
- Then i3: C named by mark/con_id; B and C exchange.
  `S(S-i3-swap)`.
- Then xmonad/Tall+Navigation2D: nothing geometrically right of B in the
  Tall projection (B tops the stack column), so the miss is a no-op.
  `S(S-xmo-nav)` + `S(S-xmo-layout)`.
- Then sway: C named by mark/con_id; B and C exchange.
  `S(S-sway-move)`.
- Then qtile/Columns: no-counterpart; only the internal drag helper
  exchanges clients, not a standalone swap command. `S(S-qti-swap-inventory)`.
- Then awesome/tile: nothing geometrically right of B in the tile
  projection, so the miss is a no-op. `S(S-awe-swap)` + `S(S-awe-tile)`.
- Then niri: no-counterpart (same verb inventories as the R-MOV-01 swap
  leg list no swap verb). `S(S-nir-move)` + `S(S-nir-actions)`.
- Then PaperWM: the registered `move-right` IS the swap verb (same-space `swap`); columns C2/C3 exchange (order A,C,B,D) with the selection staying on B, followed by layout and forced viewport. `S(S-pap-moveverbs)` + `S(S-pap-swap)`.
- Then karousel/Lazy: no-counterpart (same verb inventories list no swap
  verb). `S(S-kar-move)` + `S(S-kar-focus)`.
- Then paneru: B swaps east with C (columns C2/C3 exchange, order A,C,B,D);
  focus stays B (no focus write in the swap path). `S(S-pan-swap-peer)` +
  `S(S-pan-swap)`.
- Then Ours KDE: no-counterpart (same directional-only operation
  inventory). `S(S-ours-move)`.
- Then Ours Windows: no-counterpart; no standalone swap verb in the shared
  directional operation inventory.
  `S(S-ours-move)`.

## Selected additions (USER 2026-10-07; delivery status per profile)

[Items 3/5](../../decisions.md#move-layout-and-output-commands) select R-MOV-03's
global `sameAxisMove` / `core.same_axis_move` setting (`group-with-neighbor`
default, label `Group with neighbor`, tooltip COSMIC; `swap-with-neighbor`,
label `Swap with neighbor`, tooltip i3, sway, only for adjacent direct leaf
siblings in R2c, shares travel with windows). Exact IDs selected 2026-10-08;
functional rename delivered offline, no migration or aliases; retired IDs
follow existing invalid handling. Windows schema-v1/UI/live move setting
delivered 2026-10-11 ([record](../../changes/archive/windows-same-axis-move.md));
physical move journey remains pending.
Leaf/group rules unchanged. R-MOV-08 selects local restructure/
swap/escape first, then all-four-direction crossing including sole root
leaf. Adjacency uses reciprocal edge-touch + positive overlap on FULL
output rectangles, horizontal too; window-based selection (User decision
2026-10-09): shared edge containing the moving window centre projection,
else larger window-span overlap along the edge, final left/top tie-break;
unreadable topology refuses, no candidate no-op, no wrap. Shared core/KDE
selection delivered offline 2026-10-09
([record](../../changes/archive/position-based-output-selection.md)). Windows
foundation already uses this selector; audit/regressions and explicit-send
integration delivered offline `fda618f`, 2026-10-14
([Windows record](../../changes/archive/windows-position-based-output-selection.md)).
Native journeys remain pending. Evidence below is offline, not a live
observation or new reference vote.

<a id="r-mov-09-flat-swap-right-with-unequal-sibling-shares"></a>
### R-MOV-09: swap-with-neighbor right with unequal sibling shares

- Given (tree leg): `H[A,B*,C,D]`, shares 1/10, 2/10, 3/10, 4/10;
  `swap-with-neighbor` selected. Ordinary tiles, one output, no minimum constraints.
- Given (other models): exact N-ary unequal-share fixture/swap variant
  applicability TBD; do not silently replace it with equal shares or columns.
- When: move B right once.
- Observe: flat identity order, shares travelling with windows vs slots,
  focus, and absence of new wrap group.
- Then COSMIC: `H[A,H[B,C],D]` via the len>2 fork (next leaf C; new group with B first, no `add_window` in the fork path so the new group keeps equal halves); focus stays B (Done, no ShiftFocus). Shares from the Given 1/10-4/10: root `remove_window` redistributes B's 2/10 proportionally over A/group/D (1:3:4), so root `[1.25,3.75,5]/10` with the new group split equally (B=C=1.875/10); exact pixels TBD (output width unrecorded; i32 `round` plus overflow-to-last). `S(S-cos-move)` + `S(S-cos-newgroup)` + `S(S-cos-sizes)`.
- Then Hyprland/Dwindle: exact N-ary start has no ordinary default-ratio binary form (ratio 1 yields halves); a binary rectangle embedding holding the same rectangles is conceivable per index conventions but unrecorded in this row, so exact outcome TBD (focal anchor, ratios, geometry recalc). Policy: remove+reinsert at the 1px focal with silent source refocus and monitor fallback. `S(S-hyp-move)`.
- Then bspwm: B swaps east with C (immediate east wins by boundary distance); same-desktop swap retains focus on B; exact frames TBD without the binary embedding. `S(S-bsp-move-target)`.
- Then i3: `H[A,C,B*,D]`: flat sibling swap with C; shares travel with windows (list-position exchange, no percent rewrite: B keeps 2/10, C keeps 3/10); focus stays B. `S(S-i3-move)`.
- Then xmonad/Tall+Navigation2D: fixture-inapplicable for the exact foreign Given (F: flat unequal-share 4-child H has no ordinary Tall binary form - nmaster=1, frac=1/2 master/stack, no quarters let alone 1/10-4/10 - so the native Tall projection of the fixture is unrecorded and the exact outcome stays TBD). Conditional analogue only: Tall stores no per-slot shares (shares derive from `frac`/`nmaster` only, so no slot-vs-window share question arises) and has no wrap-group primitive; in native Tall order `[A,B,C,D]` (A master, B top of stack) `windowSwap` R `False` from B has no right candidate under tiled hybrid line/side with wrap False, so the analogue miss is no-op: order unchanged, no new group, focus retained. `S(S-xmo-layout)` + `S(S-xmo-nav)`.
- Then sway: `H[A,C,B*,D]`: flat sibling swap with C via `list_swap` (no fraction reset in the swap branch, shares travel); focus stays B. `S(S-sway-move)`.
- Then qtile/Columns: exact N-ary unequal-share start has no ordinary Columns form (default num_columns=2, so the third window stacks in-column rather than opening a third column; widths unset in this row); exact outcome TBD. Policy: `shuffle_right` carries B into the adjacent column (new column at a shared-column edge; sole-column sole-window no-op only); focus stays B. `S(S-qti-shuffle)`.
- Then awesome/tile: exact flat unequal-share start has no ordinary tile form (nmaster=1 master plus one stack column, not flat quarters); exact outcome TBD. Policy: geometric swap-or-miss with no nested wrap (tile-projection miss is no-op); focus retained (no focus write). `S(S-awe-swap)` + `S(S-awe-tile)`.
- Then niri: TBD; no column fixture is Given in this row (do not manufacture columns). Widths are arbitrarily settable (`set_column_width` proportion/fixed/adjust), so 1/10 shares are not categorically impossible; the TBD is the missing column fixture and unset widths, not the presets. `S(S-nir-move)` + `S(S-nir-base)` + `S(S-nir-resize)`.
- Then PaperWM: in the strip projection (columns in order) the registered `move-right` is same-space `swap`, exchanging B/C with edge return; no column fixture is Given in this row so the projection is qualified, not a native leg. `move_to` scrolls the viewport, not membership. Exact fixture outcome TBD. `S(S-pap-moveverbs)` + `S(S-pap-mon)` + `S(S-pap-move)`.
- Then karousel/Lazy: TBD; no column fixture is Given in this row. Manual widths are arbitrary (host resize feeds width delta, not restricted to presets), so 1/10 shares are not categorically impossible; the TBD is the missing fixture and unset widths. `S(S-kar-move)` + `S(S-kar-base)` + `S(S-kar-manual-width)`.
- Then paneru: in the strip projection of the tree leg (four `Single` columns in order) B swaps east with C (East resolves to the right neighbour deterministically); no column fixture is Given in this row so the projection is qualified, not a native leg. Shares TBD (WidthRatio mapping unrecorded); focus stays B (no focus write in the swap path). `S(S-pan-swap-peer)` + `S(S-pan-swap)`.
- Then Ours KDE: `H[A,C,B*,D]` with shares 1/10, 3/10, 2/10,
  4/10; focus B, no wrap. Delivered offline, item 3.2; core strict-apply
  and session tests exercise unequal-share right and left swaps;
  [record](../../changes/archive/same-axis-move-setting.md). Native journey pending.
- Then Ours Windows: same selected `swap-with-neighbor`/share target;
  adapter delivered 2026-10-11, base `fafcd31` + delivery commit
  ([record](../../changes/archive/windows-same-axis-move.md)). Retained Engine H4
  right/left unequal-share swaps preserve identity widths/shares/focus (projection
  rounding tolerated); native settings adoption passed. Exact Given 1/10-4/10
  live frames and physical move journey pending; item 3.2.
- Variant hook: V-MOVE-NARY.

<a id="r-mov-10-flat-swap-right-beside-a-group-neighbor"></a>
### R-MOV-10: swap-with-neighbor right beside a group neighbor

- Given (tree leg): `H[A,B*,V[C,D],E]`, `swap-with-neighbor` selected; one output,
  ordinary tiles, no minimum constraints. B's adjacent direct sibling is V,
  not a leaf. Record shares and V's remembered child before the move.
- Given (other models): exact nested fixture/variant applicability TBD;
  never manufacture H/V ancestry for columns.
- When: move B right once.
- Observe: unchanged leaf/group rule vs broadening swapping to whole groups;
  target child/index, topology, shares and focus.
- Then COSMIC: `H[A,H[B,V[C,D]],E]` via the len>2 fork (next is the V group but len!=2, so wrap not enter; B first, V order preserved); focus stays B (Done); shares TBD. `S(S-cos-move)` + `S(S-cos-newgroup)`.
- Then Hyprland/Dwindle: nested `H[A,B,V,E]` has no ordinary default-ratio binary form (ratio 1 halves only); a binary rectangle embedding holding the same rectangles is conceivable per index conventions but unrecorded in this row, so exact outcome TBD (1px-east focal at V's west edge/C-D boundary; ratios, geometry recalc). Policy: remove+reinsert at the focal. `S(S-hyp-move)`.
- Then bspwm: B swaps east with the east leaf (C or D by boundary distance then history rank); same-desktop swap retains focus on B; exact child/frames TBD (shares/history unrecorded). `S(S-bsp-move-target)`.
- Then i3: B enters the V group via the bordering-branch descend (`con_descend_direction` picks V's last-focused child); exact index TBD (V's remembered child unrecorded); focus stays B. `S(S-i3-move)`.
- Then xmonad/Tall+Navigation2D: fixture-inapplicable (flat Tall has no nested V group). `S(S-xmo-layout)`.
- Then sway: B enters V via perpendicular reparent to the focus-inactive child; exact index TBD (V focus history unrecorded); focus stays B. `S(S-sway-move)`.
- Then qtile/Columns: TBD; exact nested fixture has no established Columns counterpart in this row (no column Given; never manufacture H/V ancestry). Policy: `shuffle_right` carries into the adjacent column. `S(S-qti-shuffle)`.
- Then awesome/tile: fixture-inapplicable; nested `H[A,B,V,E]` has no tile counterpart (flat tiled-client list, no nesting levels and no group-enter primitive), so this Given has no applicable tile journey. `S(S-awe-swap)` + `S(S-awe-tile)`.
- Then niri: fixture-inapplicable; ordered columns have no nested H/V group to enter. `S(S-nir-move)`.
- Then PaperWM: fixture-inapplicable; column/row membership has no nested H ancestor. `S(S-pap-move)`.
- Then karousel/Lazy: fixture-inapplicable; Grid/Column membership has no nested H ancestor. `S(S-kar-move)`.
- Then paneru: fixture-inapplicable; the strip/column model has no nested V group (`Single`/`Stack`/`Tabs` only). `S(S-pan-model)`.
- Then Ours KDE: existing R2c group-neighbor wrap unchanged under both modes:
  `H[A,H[B,V[C,D]],E]`, focus B. With root shares `[1,2,3,4]` and V shares
  `[1,1]`, root becomes `[1,5,4]`, new H gets `[1,1]`, V stays `[1,1]`.
  No whole-group flat swap. Planner parity and strict apply verified offline;
  [record](../../changes/archive/same-axis-move-setting.md). Native journey pending.
- Then Ours Windows: restricted setting scope delivered 2026-10-11, base
  `fafcd31` + delivery commit ([record](../../changes/archive/windows-same-axis-move.md)).
  Retained Engine group-neighbor and nested-boundary parity passes across both
  modes; no whole-group swap. Exact Given live topology/index/shares TBD;
  physical move journey pending.
- Variant hook: V-MOVE-NARY.

### R-MOV-11: sole root leaf moves up to the adjacent output

- Given: lower output L full rectangle `(0,1080,1920,1080)` holds sole A*;
  upper U `(0,0,1920,1080)` has a current workspace; topology readable,
  unique reciprocal edge-touch with positive horizontal overlap. Record
  destination contents/focus before acting. Fresh mirrored left/right/down
  legs use the same sole-leaf and adjacency predicate.
- When: move A up; repeat mirrored directions from fresh fixtures.
- Observe: sole-root crossing vs no-cross gate; target current workspace,
  source membership and target admission (exact unspecified target TBD).
- Then COSMIC: sole A has no parent, so `move_current_node` returns `MoveFurther`; at default `Vertical`, Up first attempts `MoveToPreviousWorkspace`. From the first workspace this fails and propagates to `MoveToOutput(Up)`, which selects U on full-output overlap plus nearest origin distance and transfers with follow. From a later workspace it moves to the same output's previous workspace instead. Source workspace index is unspecified, so cross-vs-local remains TBD. Mirrored Left/Right map directly to `MoveToOutput` and cross given adjacency; Down first attempts `MoveToNextWorkspace` and may land same-output. Exact admission TBD (destination contents unrecorded). `S(S-cos-move)` + `S(S-cos-move-out)`.
- Then Hyprland/Dwindle: crosses to U via the monitor fallback (1px-up focal lands in U; containing-else-nearest query, fallback default true, `assignToSpace` to U's active workspace); exact admission TBD (U contents unrecorded). `S(S-hyp-move)`.
- Then bspwm: TBD without destination contents (north swap needs a leaf target; empty U has none, occupied swaps); policy is boundary distance then history rank, same-desktop retain vs cross-monitor follow. `S(S-bsp-move-target)`.
- Then i3: crosses to U via the output-directed fallback with mover follow (sole-workspace and workspace-level no-swap paths both fall back; `workspace_show` follows). `S(S-i3-move)` + `S(S-i3-outmove)`.
- Then xmonad/Tall+Navigation2D: profile move is `windowSwap` U (same-layer; `windowToScreen` is the separate carry verb, not exercised). Tiled candidates span all visible screens: with U occupied it swaps stack positions across screens with mover focus retained; with U empty there is no candidate so the miss is a no-op (wrap False). U contents unrecorded, so swap-vs-noop TBD plus target geometry TBD. `S(S-xmo-nav)` + `S(S-xmo-out)` + `S(S-xmo-scope)`.
- Then sway: crosses to the output above via the directional attach path (workspace-level no-swap falls to next-output attach to the active workspace). `S(S-sway-move)` + `S(S-sway-outmove)`.
- Then qtile/Columns: sole A stays (single-window column: `shuffle_up` in-column edge no-op; mirrored `shuffle_left` sole-column sole-window no-op and `shuffle_down` edge no-op; no cross-screen carry in Columns). `S(S-qti-shuffle)`.
- Then awesome/tile: crosses to U via `swap.global_bydirection` (local miss then screen cross); empty-vs-exchange TBD (U contents unrecorded); focus retained on the mover (no focus write; global re-activates mover). `S(S-awe-swap)` + `S(S-awe-focus)`.
- Then niri: sole/first A, so `move_up` returns false and A stays. `S(S-nir-move)`.
- Then PaperWM: verb-distinguished. Same-space `move-up` is `swap` edge no-op (sole row/col, out-of-range returns) so A stays; explicit `move-monitor-above` carries via `switchMonitor` (neighbor index; -1 stays) to U with focus. Mirrored same-space legs stay all dirs; monitor-carry crosses given a neighbor in any direction. `S(S-pap-moveverbs)` + `S(S-pap-mon)`.
- Then karousel/Lazy: fixture-inapplicable for cross-output (single-screen profile). `S(S-kar-base)`.
- Then paneru: up (North) crosses via the no-peer fall-through to `ToNextDisplay` (`Single` has no stack neighbour); mirrored South same by the min.y gate; East/West stay (no display fall-through for E/W). `S(S-pan-swap-peer)` + `S(S-pan-swap)` + `S(S-pan-display)`.
- Then Ours KDE: A crosses to U's current workspace; sole root is eligible
  in all four directions, source becomes empty. Empty target becomes sole A;
  occupied target uses selected 2026-10-14 whole-target R4 wrap nearest the
  source per REQ-MOV-08 ([discriminator](../reference-outcomes.md#kde-occupied-r4-target-shape-discriminator));
  no remembered-leaf insertion, no flat append; empty takes the mover alone.
  Core compass tests and production-entry/Engine fixture delivered offline;
  [record](../../changes/archive/four-direction-output-transfer.md). Native journey TBD.
  Single-candidate fixture: crossing stands under the User decision 2026-10-09
  window-based selection; multi-candidate selection now delivered offline
  ([selection record](../../changes/archive/position-based-output-selection.md)).
- Then Ours Windows: sole-root four-direction crossing to empty/occupied
  current targets delivered offline in the foundation; occupied landing follows
  the selected whole-target R4 wrap nearest source shared with core; shared selector audited
  `fda618f`, 2026-10-14 ([record](../../changes/archive/windows-position-based-output-selection.md)).
  Native gates 1,892 passed, zero failed, one ignored; physical journey TBD.
- Variant hook: V-R4-DIR.

### R-MOV-12: exhausted up move with two candidate outputs above

- Given: L `(0,1080,1920,1080)` holds sole A*; U1 `(0,0,960,1080)` and
  U2 `(960,0,960,1080)` both touch L's upper edge with positive overlap.
  Full topology readable; each upper output has a current workspace.
- When: move A up once.
- Observe: ambiguity refusal vs selecting a candidate by focus/geometry;
  membership/layout writes.
- Then COSMIC: Up first attempts the previous workspace at default `Vertical`, as in R-MOV-11. Only from the first workspace does failure propagate to `MoveToOutput`; that branch selects one upper output without an ambiguity refusal (`next_output` keeps minimum origin distance, ties keep the first enumerated) and follows. Cross-vs-local TBD (source index unspecified); on crossing, exact U1/U2 and admission TBD (enumeration and contents unrecorded). `S(S-cos-move)` + `S(S-cos-move-out)`.
- Then Hyprland/Dwindle: selects U2 via containing-else-nearest (no refusal branch in the move path): the 1px-up focal is (960,1079) from the sole-A box (0,1080,1920,1080), contained only by U2 under pinned half-open containment (U1 [0,960) excludes the seam x=960), so monitor order is moot. Exact admission TBD (F: U2 contents unrecorded). `S(S-hyp-move)`; queued.
- Then bspwm: selects by boundary distance then history with no ambiguity gate; exact target/no-op TBD (upper workspace contents unrecorded; empty has no leaf). `S(S-bsp-move-target)`.
- Then i3: selects the closest output (no refusal; NULL only when none); exact U1/U2 TBD (both y=0 tie, output list order unrecorded). `S(S-i3-move)` + `S(S-i3-outmove)`.
- Then xmonad/Tall+Navigation2D: profile move is `windowSwap` U (same-layer; `windowToScreen` is the separate carry verb, not exercised). Selection is tiled line/side plus center distance with stack-order tie preference, no refusal branch: hit swaps stack positions across screens with mover focus retained, miss is a no-op. Exact candidate TBD (two-candidate geometry plus both workspaces' contents unrecorded). `S(S-xmo-nav)` + `S(S-xmo-out)` + `S(S-xmo-scope)`.
- Then sway: selects the adjacent output (no refusal; NULL only when none); exact U1/U2 TBD (center x=960 on the U1/U2 boundary; wlroots tie unestablished). `S(S-sway-move)` + `S(S-sway-outmove)`.
- Then qtile/Columns: sole A stays (`shuffle_up` edge no-op; no cross-screen carry). `S(S-qti-shuffle)`.
- Then awesome/tile: selects via `swap.global_bydirection` (no refusal); exact U1/U2 TBD (nearest by client geometries; destination contents unrecorded). `S(S-awe-swap)` + `S(S-awe-focus)`.
- Then niri: sole/first A, so `move_up` returns false and A stays. `S(S-nir-move)`.
- Then PaperWM: verb-distinguished. Same-space `move-up` is `swap` edge no-op (sole, out-of-range returns) so A stays; explicit `move-monitor-above` carries via `switchMonitor` neighbor index (only -1 stays, no other refusal); exact U1/U2 TBD (neighbor-index order unpinned). `S(S-pap-moveverbs)` + `S(S-pap-mon)`.
- Then karousel/Lazy: fixture-inapplicable for cross-output (single-screen profile). `S(S-kar-base)`.
- Then paneru: crosses via the fall-through `ToNextDisplay` (no refusal/ambiguity gate); exact U1/U2 TBD (display order unrecorded). `S(S-pan-swap-peer)` + `S(S-pan-swap)` + `S(S-pan-display)`.
- Then Ours KDE: selected (User decision 2026-10-09): shared edge containing
  the moving window centre projection, else larger window-span overlap along
  the edge, final left/top tie-break; unreadable topology refuses, no
  candidate no-op, no wrap. Implemented offline 2026-10-09: four-direction
  two-candidate observer tests, mirrored shared selector tests and real-Engine
  non-left/top R4 crossing. Pinned dispatch target survives mover relocation;
  delayed arrival follows once; removal refuses and reconciles.
  World route aligned offline 2026-10-18: core-owned FULL topology applies the
  same ranker; retained R4 transfers pin native arrival and fence newer moves.
  [World-route evidence](../../changes/archive/world-split-edge-selection.md).
  Landing into an occupied target follows the selected 2026-10-14 whole-target
  R4 wrap nearest source; empty takes the mover alone.
  [Record](../../changes/archive/position-based-output-selection.md).
  Native journey TBD.
- Then Ours Windows: shared window-based selection wired by the foundation,
  audited/regressed `fda618f`, 2026-10-14: centre precedence, exact half-open
  odd-extent seam, span-overlap fallback and left/top/identity ties. Non-left/top
  target remains pinned after arrival ([record](../../changes/archive/windows-position-based-output-selection.md)).
  Occupied landing follows the selected whole-target R4 wrap nearest source
  shared with core. Physical multi-candidate journey TBD; item 5.2.
- Variant hook: V-R4-DIR.

#### R-MOV-12 technical selection discriminators (fresh legs)

All rows move sole A up after local exhaustion; full topology is readable.
Intervals below are shared FULL edges on the x axis. These are offline Ours
KDE/core sub-legs, not reference votes; reference/native outcomes remain TBD.

| Given | When | Ours KDE/core offline outcome (2026-10-09; world route 2026-10-18) |
| --- | --- | --- |
| Original U1/U2 edges [0,960), [960,1920); A x=860, width=200, centre=960 | Move up once | U2: half-open containment puts the seam in the right edge |
| Source x-span [0,1600); upper edges [0,400), [1200,1600); A x=300, width=1100, centre=850 | Move up once | U2: centre in gap; window overlap 200 beats 100 |
| Source x-span [0,1600); overlapping upper edges [0,1200), [400,1600); A x=900, width=400, centre=1100 | Move up once | U1: both contain centre; left/top wins directly despite U2's larger span overlap |

Evidence: `position_based_output_selection.rs`, `plan-directional.test.ts`;
odd extents retain exact half-pixel centres, Left/Right mirror onto y.
World-route expected results: `world_directional_intents.rs` R-MOV-12 tests;
production entry and retained replay: `world-split-edge-engine-fixture.test.ts`.

### R-MOV-13: panel work-area gap with touching full output rectangles

- Given: U full `(0,0,1920,1080)`, work area `(0,0,1920,1040)`;
  L full/work area `(0,1080,1920,1080)` holds sole A*. A panel on U
  leaves a 40px work-area gap; full rectangles are unique reciprocal
  edge-touch neighbors with positive overlap. Fresh horizontal leg:
  left full `(0,0,1920,1080)`, work area `(0,0,1880,1080)`;
  right full/work area `(1920,0,1920,1080)` holds sole A*.
- When: move A up; fresh horizontal leg move A left.
- Observe: full-output adjacency vs work-area-gap rejection, both axes.
- Then COSMIC: Left crosses directly with follow; Up first attempts the same output's previous workspace at default `Vertical` and crosses only when that fails from the first workspace. Up cross-vs-local remains TBD (source index unspecified). When reached, `next_output` selects on full output `geometry()`, so the 40px work-area gap does not block either output-transfer branch. Exact admission TBD (destination contents unrecorded). `S(S-cos-move)` + `S(S-cos-move-out)`.
- Then Hyprland/Dwindle: crosses in both legs via containing-else-nearest on full monitor boxes (focal still in the target; the query ignores reserved); the 40px work-area gap does not block. Exact admission TBD. `S(S-hyp-move)`.
- Then bspwm: No work-area-gap rejection in either leg (the directional selector matches on window rectangles with no work-area adjacency gate, so the 40px panel gap only shifts boundary distance); occupied-vs-empty destination decides swap-vs-no-op, but U contents/focus are unrecorded, so the exact per-leg outcome stays TBD (F: empty U offers no leaf candidate while occupied U swaps by boundary distance then history rank). Cross-monitor follow focuses the mover. `S(S-bsp-move-target)`; queued.
- Then i3: crosses in both legs via closest-output on full output rects (overlap check on output rects; panel/work-area does not change rects). `S(S-i3-move)` + `S(S-i3-outmove)`.
- Then xmonad/Tall+Navigation2D: profile move is `windowSwap` in both legs (same-layer; `windowToScreen` is the separate carry verb, not exercised). No work-area-gap rejection branch in the pinned nav source: occupied targets swap stack positions across screens with mover focus retained, empty targets miss as no-op. Contents unrecorded, so swap-vs-noop TBD per leg plus target geometry TBD. `S(S-xmo-nav)` + `S(S-xmo-out)` + `S(S-xmo-scope)`.
- Then sway: crosses in both legs via the adjacent output (layout boxes are full outputs; the work-area gap does not remove adjacency). `S(S-sway-move)` + `S(S-sway-outmove)`.
- Then qtile/Columns: stays in both legs (sole edge no-ops; local Columns only). `S(S-qti-shuffle)`.
- Then awesome/tile: crosses in both legs via `swap.global_bydirection` (screen geometries are full; the work-area gap does not remove the next screen). `S(S-awe-swap)` + `S(S-awe-focus)`.
- Then niri: stays in both legs (first/sole edge returns false). `S(S-nir-move)`.
- Then PaperWM: verb-distinguished. Same-space `move-up`/`move-left` are `swap` edge no-ops (sole, out-of-range returns) so A stays both legs; explicit `move-monitor-above`/`move-monitor-left` carry via `switchMonitor` neighbor index (display topology, not work-area; -1 stays) so both legs cross. `S(S-pap-moveverbs)` + `S(S-pap-mon)`.
- Then karousel/Lazy: fixture-inapplicable for cross-output (single-screen profile). `S(S-kar-base)`.
- Then paneru: up crosses via the North fall-through (bounds min.y gate; the work-area gap does not change min.y); left stays (West has no display fall-through). `S(S-pan-swap-peer)` + `S(S-pan-swap)` + `S(S-pan-display)`.
- Then Ours KDE: crosses in both legs using FULL rectangles for selection;
  panel gap does not block. Placement still uses each desktop's work area.
  Both-axis observer tests and stacked-output production-entry/Engine fixture
  delivered offline; [record](../../changes/archive/four-direction-output-transfer.md).
  Native journey TBD. Single-neighbor fixture: crossing stands under the User
  decision 2026-10-09 window-based selection; multi-candidate selection
  delivered offline ([selection record](../../changes/archive/position-based-output-selection.md)).
- Then Ours Windows: foundation selects on FULL rectangles and places in
  work areas; audit/regressions `fda618f`, 2026-10-14 cover panel gaps,
  negative/stacked geometry and differing physical work rectangles
  ([record](../../changes/archive/windows-position-based-output-selection.md)).
  Physical two-output and mixed-DPI settlement TBD; item 5.2.
- Variant hook: V-R4-DIR.
