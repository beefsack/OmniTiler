# Groups / stacks (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

## 8. Groups / stacks

<a id="scrolling-backfill-additive-existing-wide-tables-above-unchanged"></a>
<a id="r-grp-01-backfill-toggle-stack-and-switch-tabs-scrolling"></a>
### R-GRP-01: toggle stack and switch tabs

- Given (tree profiles): `H[A,B*]`

- Given (column profiles): `COL[C1[A,B*]]` with A above B in one column at shipped
  defaults; viewport recorded. Toggle the profile tabbed/stacked display
  where present, then step tabs/members. This is a display-mode
  projection, not an exact H-tree conversion. Paneru exact `S` fixture is
  inapplicable per its Stack/Tabs model; its native-tab journey belongs
  to R-COL-10.

- When: Toggle stack on the group, switch tabs.

- Observe: Split-to-stack conversion; tab switch

- Observe (column leg): display conversion; member step.

- Then COSMIC: Super+S toggles the focused node: fixture B is window-focused, so B alone becomes a single-tab stack (`H[A,S[B]]`, B active); group-focused converts the whole group to `S[A,B]` (A initially active). Stack splits back to tiles; tabs step with Focus Left/Right (Up/Down enters/leaves the group); `S(S-cos-stack)` + `D(D-ref)`; tab-bar visuals/native timing TBD
- Then Hyprland/Dwindle: `toggleGroup` on focused B creates a one-window group at B's slot (`H[A,G[B]]`), no whole-group conversion counterpart; tab step on the single-member group errors/leaves current unchanged; multi-tab stepping wraps current and refocuses only if the group was focused; `S(S-hyp-groupop)`; bar visuals/native timing TBD
- Then bspwm: No-counterpart (no stack/tab group in source; monocle is a desktop layout, not tabs, so toggle/tab-step never run); `S(S-bsp-layout)`
- Then i3: `layout tabbed` (or `stacked`) retargets the H parent, so `H[A,B*]` becomes a 2-tab tabbed (or stacked) parent with B active; `toggle` cycles stacked/tabbed/split; tabs step via directional focus (tabbed HORIZ left/right, stacked VERT up/down) or decoration tab click/scroll; `S(S-i3-layout)` + `S(S-i3-grp)`; exact toggle verb (tabbed vs stacked vs toggle split/all) and bar visuals/native timing TBD (gesture unspecified)
- Then xmonad/Tall+Navigation2D: Unsupported action parameter here: no stack/tab group in source (core layouts Tall, Mirror Tall, Full only; `NextLayout` rotates layouts, no tab-toggle/tab-step verb in this profile, so toggle/tab-step never run); `S(S-xmo-layout)` + `S(S-xmo-core-nav)`
- Then sway: `layout tabbed` (or `stacked`) retargets the H parent, so `H[A,B*]` becomes a 2-tab tabbed (or stacked) parent with B active (single-child flatten/workspace wrap like i3); tabs step via directional focus (tabbed left/right, stacked up/down, wrap per config); `S(S-sway-layout)` + `S(S-sway-focus)`; exact toggle verb (tabbed vs stacked vs toggle split/all) and bar visuals/native timing TBD (gesture unspecified)
- Then qtile/Columns: Unsupported action parameter here: no tab-stack group in this profile (layouts are Columns plus Max only); the counterpart `toggle_split` flips the current column split/unsplit (unsplit shows one window, not tabs) with no tab-step verb, so the split-to-stack conversion and tab switch never run; `S(S-qti-split)` + `S(S-qti-default)`
- Then awesome/tile: Unsupported action parameter here: no stack/tab group primitive in source (shipped layouts floating plus tile variants/fair/spiral/max/magnifier/corner; per-tag layout via set/inc, no tab-toggle/tab-step verb), so toggle/tab-step never runs in either the tree or the column leg; `S(S-awe-default)` + `S(S-awe-layout)` + `S(S-awe-keys)`
- Then niri: toggles Normal/Tabbed via `toggle_column_tabbed_display` and
  steps members via `focus_down`/`focus_up` (`activate_idx` saturating
  step). `S(S-nir-consume)`.
- Then PaperWM: no-counterpart (no tabbed/stacked display toggle in the
  registered action inventory; slurp/barf are visible-height consume,
  not tabs). `S(S-pap-acts)`.
- Then karousel/Lazy: toggles stacked via `column-toggle-stacked` (needs
  2+ windows; overlapping arrange, not tabs) and steps members via
  `focusDown`/`focusUp` (above/below window). `S(S-kar-grpmove)`.
- Then paneru: fixture-inapplicable (`Stack` is visible stacking, `Tabs`
  holds app-native tabs per `S(S-pan-model)`; exact `S` toggle has no
  counterpart, native-tab variant under R-COL-10). `S(S-pan-model)`.
- Then Ours KDE: Deferred: centre-stack drops refused fail-closed; no tab carrier/bindings; `D(D-dec-cos)`
- Then Ours Windows: Deferred: centre-stack drops refused fail-closed; no tab carrier/bindings; `D(D-dec-cos)`
- Variant hook: V-GROUP-STACK.


## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours cells cite Engine source at `9241c94` plus the new
`S(S-ours-grp)` model key; selected intent and doc assertions are never
evidence.

### R-GRP-02: join a tile into a stack; move a tab out

- Given (tree profiles): `H[S[A,C],B*]`, left stack holds A then C with A
  the active tab. Ordinary windows, no rules, shipped defaults. Sequential
  primary journey: join B left into the stack, then move B (now active in
  the joined stack) out right once with no reset between the steps.
  Record actual post-join order before the leave step.
- Given (column profiles): `COL[C1[A,C],C2[B*]]`, C1 holds A above C both
  vertically visible where the model supports it (tabbed-display columns
  use an explicit tabbed Given instead); shipped defaults apply.
  PaperWM uses an A-focused left-column variant for the slurp leg
  (explicitly recorded). Same sequential journey with the profile native
  consume/slurp/join then expel/barf/leave verbs. Paneru uses one strip
  with two columns.
- When: step 1 semantic join B left into the stack/column (native verbs
  named per profile below; pointer centre-join stays R-DRAG-01 and never
  substitutes); step 2 move the joined B out right once (native verb per
  profile). A missing verb is not a no-op.
- Observe: join vs swap, actual post-join membership/order/active tab;
  one right operation as a new tile vs reorder vs dissolve.
- Then COSMIC: joins via directional `Move` left: the root len-2
  stack-neighbor branch appends B last into the stack as `S[A,C,B*]`
  with B activated, mover focus follows the stack (`ShiftFocus`); then
  one Right `handle_move` from the joined B removes the last tab via
  `MoveOut` (survivor active clamps to C) and reinserts B as a new tile
  right of the group. `S(S-cos-move)` + `S(S-cos-grpmove)`.
- Then Hyprland/Dwindle: joins via `moveIntoGroup left` to `G[A,B*,C]`
  (B after current A per `insert_after_current`, B made current and
  focused); then leaves via `moveOutOfGroup right` to a new tile right of
  the group with the mover focused under shipped
  `focus_removed_window=true`, group retains `[A,C]`.
  `S(S-hyp-grpmove)` + `S(S-hyp-defaults)`.
- Then bspwm: no-counterpart (no stack/tab group in source; monocle is a
  desktop layout, not tabs, so neither leg has a faithful start).
  `S(S-bsp-layout)`.
- Then i3: joins via `move left` into the tabbed parent appended last as `S[A,C,B*]` with B active (bordering-branch descend picks C for the LEFT step into HORIZ tabbed,   `AFTER` insert via `insert_con_into` with global focused preserved despite stack surgery); then leaves via `move right` promoting B after the group to a new tile right as `H[S[A,C],B*]` with focus staying B (no same-orientation swap at the tail: lone-last-child escapes via the above-container `AFTER` insert; group retained as `[A,C]` with survivor active A, no dissolve; single-child wrappers persist). `S(S-i3-grp)` + `S(S-i3-move)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (core layouts Tall,
  Mirror Tall, Full only; no tab-toggle/join/leave verb in this profile).
  `S(S-xmo-layout)` + `S(S-xmo-core-nav)`.
- Then sway: joins via `move left` into the tabbed parent appended last as `S[A,C,B*]` with B active (parallel reparent to length, not a swap; no focus write, so focus stays B); then leaves via `move right` promoting B after the group to a new tile right as `H[S[A,C],B*]` with focus staying B (group retained as `[A,C]`, no dissolve; single-child wrappers persist, empty-only reap). `S(S-sway-layout)` + `S(S-sway-move)` + `S(S-sway-cleanup)`.
- Then qtile/Columns: no-counterpart (layouts are Columns plus Max only;
  `toggle_split` flips split/unsplit with no tab-step/join verb, so
  neither leg has a faithful start). `S(S-qti-split)` + `S(S-qti-default)`.
- Then awesome/tile: no-counterpart (shipped layouts floating plus tile
  variants with per-tag select, no tab-toggle/join/leave verb).
  `S(S-awe-default)` + `S(S-awe-layout)` + `S(S-awe-keys)`.
- Then niri: joins B left into C1 appended last as `C1[A,C,B*]` with B
  activated (`None` appends plus `activate_idx` and column activation);
  then expels B right to a new sole column with B active. `S(S-nir-consume)`.
- Then PaperWM: from the A-focused variant, `slurp(A)` joins B into C1
  appended last (directional RIGHT consumes `space[1][0]`, emptied column
  removed); then `barf` expels B to a new column at the open position.
  Shipped RIGHT `slurp(B)` from the right column is a no-op (no right
  neighbor) and never substitutes. Selection retained on both steps
  (neither path writes `selectedWindow`). `S(S-pap-slurp)`.
- Then karousel/Lazy: joins via `windowMoveLeft` (single-window C2 into
  existing C1 appended last with B staying focused via `onWindowAdded`
  focus-taker update); then leaves via `windowMoveRight` (shared-column
  window to a new own column, source retains `[A,C]`). Visible heights
  apply at shipped defaults (stacked display off). `S(S-kar-grpmove)`.
- Then paneru: joins via `Stack(true)` on focused B into the left column
  appended last as `Stack[A,C,B]` with B staying focused (no focus write);
  then leaves via `Stack(false)` to a new own column right of the retained
  `Stack[A,C]` with B staying focused; each step reshuffles around B.
  `S(S-pan-stack)` + `S(S-pan-colops)`.
- Then Ours KDE: fixture-inapplicable (no tab/stack carrier in the Engine
  `Node` model and no semantic join/leave verb in any Engine layer, so
  neither leg has a faithful start). `S(S-ours-grp)`.
- Then Ours Windows: same fixture-inapplicable leg as Ours KDE via the
  shared Engine (no tab carrier, no join/leave verb). `S(S-ours-grp)`.
- Variant hook: V-GROUP-STACK ([user decision 2026-10-07](../../decisions/visuals.md#shared-visual-safety): tabs first after 0.1; until then centre-stack refuse closed).

### R-GRP-03: close the active tab

- Given (tree profiles): `S[A,B*,C]`, order A then B then C with B the
  active tab; history A,C,B (focus A then C then B). Ordinary windows,
  no rules, shipped defaults. Record actual membership and active tab.
- Given (column profiles): tabbed-display `COL[C1[S[A,B*,C]]]` where the
  model supports tabs (niri tabbed display, karousel stacked display as
  an explicit stacked variant); otherwise `COL[C1[A,B*,C]]` with all
  three vertically visible. Shipped defaults apply; viewport recorded.
  Paneru Stack/Tabs distinction is per `S(S-pan-model)`.
- When: close B via the profile native close (window close, not ungroup).
- Observe: focused tab after close (neighbor vs MRU), group retained vs
  flattening, tab-bar update.
- Then COSMIC: group retained as a 2-tab stack with C the active tab
  (`fetch_min` keeps index 1, now C). `S(S-cos-grpclose)`.
- Then Hyprland/Dwindle: group retained as `[A,C]` with C focused (grouped
  next for the focused close under shipped `focus_on_close=next`; MRU C
  coincides here). `S(S-hyp-close)` + `S(S-hyp-grpmove)`.
- Then bspwm: fixture-inapplicable (no stack/tab group in source, so the
  `S[A,B*,C]` start has no counterpart). `S(S-bsp-layout)`.
- Then i3: group retained as 2-tab tabbed (3-to-2 keeps the parent);
  focus falls to C via `con_next_focused` (B is the focus-stack head, so
  the next focus-stack entry C wins, then descends). `S(S-i3-grp)` +
  `S(S-i3-close)`.
- Then xmonad/Tall+Navigation2D: fixture-inapplicable (no stack/tab group
  in source). `S(S-xmo-layout)`.
- Then sway: group retained as 2-tab tabbed; destroyed-node focus falls
  to the parent focus-inactive view, which is MRU C here. `S(S-sway-close)` +
  `S(S-sway-layout)`.
- Then qtile/Columns: fixture-inapplicable (no tab-stack group; Columns
  plus Max only). `S(S-qti-split)` + `S(S-qti-default)`.
- Then awesome/tile: fixture-inapplicable (no stack/tab group primitive).
  `S(S-awe-default)` + `S(S-awe-layout)`.
- Then niri: the column is retained as `[A,C]` (multi-member removal never
  dissolves a column); the active index stays to take the next member, so
  C becomes active with focus to C (positional next, MRU C coinciding
  here); Tabbed display retained. `S(S-nir-consume)` + `S(S-nir-close)`.
- Then PaperWM: the column is retained as `[A,C]` (member splice;
  no tab bar to update, no tabbed display); extension selection
  falls to the stack-topmost surviving neighbour (not MRU
  guaranteed), while exact native A/C focus is TBD (host shell
  focus fallback). `S(S-pap-close)`; queued.
- Then karousel/Lazy: the column is retained as `[A,C]` (member splice,
  no tab bar to update); focus falls to the above neighbour A via the
  focus-taker fixup (not MRU C), with a script `Immediate` focus write
  through the host activation path; visible heights redistribute while
  the stacked variant keeps overlapping arrange. `S(S-kar-grpmove)` +
  `S(S-kwin-scriptact)`.
- Then paneru: the all-visible Stack column is retained as `[A,C]`
  (member splice, no tab bar to update); focus falls to A (the sole
  remaining column top under the nearest-center give-away with a single
  candidate, not MRU C). `S(S-pan-model)` + `S(S-pan-close)`.
- Then Ours KDE: fixture-inapplicable (no tab/stack carrier in the Engine
  `Node` model, so the `S` start has no counterpart; ordinary close stays
  R-CLOSE). `S(S-ours-grp)`.
- Then Ours Windows: same fixture-inapplicable leg as Ours KDE via the
  shared Engine. `S(S-ours-grp)`.
- Variant hook: V-GROUP-STACK ([user decision 2026-10-07](../../decisions/visuals.md#shared-visual-safety): tabs first after 0.1; close keeps group and activates next tab; until then centre-stack refuse closed).
