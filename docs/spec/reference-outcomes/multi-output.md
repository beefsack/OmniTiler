# Multi-output (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

## 9. Multi-output

Column Given bullets are separate fixtures, never H/V ancestry claims.
karousel two-output fixtures are inapplicable per `S(S-kar-single)`;
paneru legs distinguish the native Space from virtual rows.

<a id="scrolling-backfill-additive-wide-rows-above-preserved"></a>
<a id="r-out-01-scrolling-assessment-move-left-onto-an-occupied-output"></a>
### R-OUT-01: move left onto an occupied output

- Given (tree profiles): `L=X`, `R=H[A*,B]`

- Given (column profiles): `L=COL[C0[X]]`, `R=COL[C1[A*],C2[B]]` at shipped
  defaults; viewport recorded. Native verbs (same semantic
  directional move-left as the tree leg above, not explicit
  monitor transfer): niri `MoveColumnLeft`; PaperWM `move-left`;
  paneru `Swap(West)`.

- When: Move A left (occupied target)

- Observe: Cross vs wrap; target split shape

- Then COSMIC: `L=H[X,A]`, `R=B` R4; [S20-01](../../cosmic-move-conformance.md#sequence-s20---horizontal-output-crossing) authored observation
- Then Hyprland/Dwindle: Off-monitor focal transfers via `assignToSpace` to the focal monitor's active workspace; exact L split shape TBD (L work area/X geometry, vertical alignment, and drop half unrecorded). Mechanism: 1px-beyond-edge focal, containing-else-nearest monitor query, fallback default true, then Dwindle re-admission anchor; `S(S-hyp-move)`
- Then bspwm: Crosses to L via configured `-s west --follow`: the west-neighbor search spans all monitors' focused desktops, so X qualifies; cross-monitor node swap puts A sole on L and X in A's R slot (`L=A`, `R=H[X,B]`), `--follow` focuses A on L; exact L split/frames TBD (monitor/X geometry unrecorded); `S(S-bsp-flt-focus)` + `S(S-bsp-flt-swap)`
- Then i3: Crosses to L: workspace-level H has no left swap, so `move_to_output_directed` attaches A to L's visible workspace at TAIL (nodes `[X,A]`, flat `H[X,A]` under the profile splith with default `workspace_layout` and no custom rules; the directed path inserts directly with no `workspace_attach_to` wrapper); R collapses to sole B; mover-focused follow via `workspace_show` focuses A on L; `S(S-i3-move)` + `S(S-i3-outmove)`
- Then xmonad/Tall+Navigation2D: Exact occupied-target crossing TBD (Tall target geometry unrecorded); analogous policy only: profile move is same-layer `windowSwap` (geometric target via `navigableWindows`; `swap` exchanges stack positions retaining mover focus, miss is no-op with wrap False); cross-screen carry via the separate `windowToScreen` (`W.shift`) verb is not exercised here; exact L split/frames TBD; `S(S-xmo-out)` + `S(S-xmo-nav)` + `S(S-xmo-layout)`
- Then sway: Crosses to L: workspace-level H has no left swap, so the next-output attach moves A to L's active workspace at TAIL (nodes `[X,A]`) via `container_move_to_workspace_from_direction` (parallel-LEFT flat insert else perpendicular focus-inactive recursion); R collapses to sole B; focus stays on the mover A on L (no workspace-switch call in this path); `S(S-sway-move)` + `S(S-sway-outmove)` + `S(S-sway-wsdefault)`; exact L wrapper stays TBD (F: L output geometry/aspect unrecorded, so the geometry-following default layout and the workspace layout parallelism branch are unspecified; shipped defaults with no custom rules here)
- Then qtile/Columns: No-op: leftmost sole-column A has no adjacent column and no shared column to split, so shuffle_left returns with tree and focus unchanged; no directional cross-screen carry in Columns (screen placement is togroup/toscreen, not exercised here); `S(S-qti-shuffle)` + `S(S-qti-group)`
- Then awesome/tile: Profile directional move is `swap.global_bydirection` (local `swap.bydirection` miss then screen cross; single-output miss is no-op). From A west local misses (A leftmost under strict-origin geometry), global crosses to L: `focus.global_bydirection` activates the encountered L peer X first, then A/X screen exchange via `move_to_screen` (no tag write, so the mover re-activate emits while mistagged and writes nothing), followed by delayed screen-consistency retag landing A on L's selected tag and X on R's selected tag (`permissions.tag` via `to_selected_tags`); tile recalc on both screens over retained global order (X at its retained position; exact R order/pixel frames TBD (F: global insertion order and work area unrecorded)). Focus ends on X. `S(S-awe-swap)` + `S(S-awe-focus)` + `S(S-awe-tile)`
- Then niri: stays (`move_left` reorders strip columns and returns
  false at index 0; A is already first, so no reorder and no cross;
  crossing needs the separate `MoveColumnToMonitor*` verb, an
  R-OUT-04 leg). `S(S-nir-move)` + `S(S-nir-mon)`.
- Then PaperWM: stays (`move-left` is same-space `swap(LEFT)` and
  returns at the first column; no cross; crossing needs
  `switchMonitor`/`moveToMonitor`, R-OUT-04 legs).
  `S(S-pap-mon)`.
- Then karousel/Lazy: fixture-inapplicable (single-screen profile; no
  counterpart). `S(S-kar-single)`.
- Then paneru: stays (`Swap(West)` resolves a same-strip peer and A
  has none; West/East never fall through to another display, only
  North/South do). `D1`/`D2` displays keep their strips.
  `S(S-pan-swap)`.
- Then Ours KDE: exhausted horizontal R4 into output's current workspace,
  `L=H[X,A*]`, `R=B`, edge insertion nearest source. Reciprocal FULL
  rectangle selection now includes sole root leaves; work-area placement and
  commit/arrival fences retained. [Item-5 offline record](../../changes/archive/four-direction-output-transfer.md);
  native journey pending. User decision 2026-10-09 window-based selection
  delivered offline ([selection record](../../changes/archive/position-based-output-selection.md)); single-target crossing stands.
- Then Ours Windows: exhausted horizontal crossing to the target current
  workspace delivered offline in the [foundation](../../changes/windows-multi-output-foundation.md).
  Shared window-based selector audited/regressed `fda618f`, 2026-10-14
  ([record](../../changes/archive/windows-position-based-output-selection.md));
  nearest-source R4 landing and work-area placement retained. Native gates
  1,892 passed, zero failed, one ignored; physical two-output journey pending.
- Variant hook: V-R4-DIR.

<a id="r-out-02-scrolling-assessment-perpendicular-move-at-an-output-edge"></a>
### R-OUT-02: perpendicular move at an output edge

- Given (tree profiles): `L=X`, `R=V[A*,B]`

- Given (column profiles): `R=COL[C1[A*,B]]` with A above B both visible,
  `L=COL[C0[X]]`; shipped defaults; viewport recorded. Same
  directional verbs as the R-OUT-01 column leg.

- When: Move A left (perpendicular)

- Observe: In-output wrap wins vs cross

- Then COSMIC: No cross; `R=H[A,B]` R1; [S21-01](../../cosmic-move-conformance.md#sequence-s21---perpendicular-wrapno-cross-case) authored observation
- Then Hyprland/Dwindle: Exact cross-vs-local TBD (monitor arrangement/edge adjacency and vertical alignment unrecorded, so the 1px focal may sit on L or R). Policy: focal off-monitor with fallback crosses via `assignToSpace`, else local remove+reinsert ordered by focal half; `S(S-hyp-move)`
- Then bspwm: Crosses rather than local-wrapping under `-s west --follow`: the west search spans monitors, so full-height X qualifies west of A (shared vertical range) while B sits south; A swaps with X (`L=A`, `R=V[X,B]`), `--follow` focuses A on L; no R1-style local wrap in source; exact frames TBD; `S(S-bsp-flt-focus)` + `S(S-bsp-flt-swap)`
- Then i3: No cross: perpendicular LEFT finds no HORIZ parent, so the workspace force-wraps to H and A inserts above its V parent (`H[A,V[B]]`, lone `V[B]` wrapper persists); focus stays A; `S(S-i3-move)` + `S(S-i3-outmove)`
- Then xmonad/Tall+Navigation2D: Exact perpendicular `V` fixture has no active-Tall counterpart (no nested V; Tall is fixed master/stack side-by-side, `Mirror Tall` not assumed in this profile); analogous policy only: no local-wrap primitive exists (selection is purely geometric line/side/center, never tree-orientation), so a qualifying western X still swaps rather than wrapping locally; exact cross-vs-local TBD (output/monitor geometry unrecorded); `S(S-xmo-out)` + `S(S-xmo-nav)`
- Then sway: No cross: perpendicular LEFT finds no HORIZ parent, so the workspace force-wraps to H and A inserts above its V parent (`H[A,V[B]]`, lone `V[B]` wrapper persists); focus stays A; `S(S-sway-move)` + `S(S-sway-outmove)`
- Then qtile/Columns: No cross: `shuffle_left` on the single column carrying A above B prepends a new column holding A (local split into `COL[C0[A*],C1[B]]` at equal `initial_ratio` widths, focus staying A via `layout_all`), never crossing screens. `S(S-qti-shuffle)` + `S(S-qti-colwidth)`
- Then awesome/tile: Exact `V[A,B]` fixture has no ordinary tile counterpart (tile with 2 clients is side-by-side master/stack, not top/bottom V); exact axes/frames TBD. Policy: no local-wrap primitive (move is geometric swap only); west local from A misses, so profile `swap.global_bydirection` crosses to L (A/X screen exchange, focus retained on A) rather than wrapping locally; `S(S-awe-swap)` + `S(S-awe-focus)` + `S(S-awe-tile)`
- Then niri: stays (`move_left` is strip-local column reorder with
  edge-false; single-column fixture has no reorder and no monitor
  leg). `S(S-nir-move)` + `S(S-nir-mon)`.
- Then PaperWM: stays (`swap(LEFT)` swaps columns within the space
  and returns at the first column; no row/column cross).
  `S(S-pap-mon)`.
- Then karousel/Lazy: fixture-inapplicable (single-screen profile; no
  counterpart). `S(S-kar-single)`.
- Then paneru: stays (single-column strip has no western peer for
  `Swap(West)`; no display fall-through on West). `S(S-pan-swap)`.
- Then Ours KDE: Local R1 wins first; selected Wrap locally, no setting (User 2026-10-12 D13, [decision](../../decisions.md#move-layout-and-output-commands)); native fixture TBD. `D(D-dec-cos)`.
- Then Ours Windows: same selected Wrap locally, no setting; shared local-before-cross verification-only evidence 2026-10-13 ([record](../../changes/archive/windows-item22-verification.md)); native fixture TBD ([D13 decision](../../decisions.md#move-layout-and-output-commands)). `D(D-dec-cos)`.
- Variant hook: V-R4-DIR.

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. karousel two-output fixtures are inapplicable per
`S(S-kar-single)` (single-screen profile). paneru legs distinguish the
native macOS Space from virtual rows. Ours cells cite Engine + adapter
source at `9241c94` plus the new `S(S-ours-out)` model key; selected
intent and doc assertions are never evidence.

Output-send verbs (shared inventory for R-OUT-04; each When below sends
the responsive client via its native explicit output-transfer route, not
an edge move): COSMIC `MoveToOutput` (follow) / `SendToOutput` (stay)
(`S(S-cos-out)`); Hyprland `movetoworkspace` to L's workspace in follow
form, silent refocuses the source (`S(S-hyp-movews)`); bspwm
`node -m west` unflagged (without `--follow` stays on the source; `--follow`
follows) (`S(S-bsp-send)` + `S(S-bsp-xfer)`); i3
`move container to output` (`S(S-i3-move)` + `S(S-i3-outmove)`); xmonad
`windowToScreen` (`S(S-xmo-scope)`); sway `move container to output`
(`S(S-sway-move)` + `S(S-sway-outmove)`); qtile `togroup` to L's group
in the shipped follow form (`S(S-qti-group)`); awesome `move_to_tag`
with screen (`S(S-awe-tag)`); niri `MoveWindowToMonitorLeft`
(`S(S-nir-mon)`); PaperWM `switchMonitor` with window carry, not the
whole-space `moveToMonitor` (`S(S-pap-mon)`); karousel has no output
verb (single-screen, `S(S-kar-single)`); paneru `window nextdisplay`
(Follow; `nextdisplaysend` is the Stay variant) (`S(S-pan-display)`);
Ours has distinct `send-to-output` follow/stay commands (item 5, delivered
offline); workspace send still refuses cross-output (`S(S-ours-out)` pinned baseline).

### R-OUT-03: focus left across outputs

- Given (tree profiles): `L=H[X]`, `R=H[A*,B]`, equal aligned outputs.
  Ordinary windows, no rules. Record target workspace MRU before acting.
- Given (column profiles): `L=COL[C0[X]]`, `R=COL[C1[A*],C2[B]]` at
  shipped defaults; viewport recorded.
- Given (paneru): display `D1` owns strip `S1=COL[C0[X]]`, display
  `D2` owns strip `S2=COL[C1[A*],C2[B]]`; each display keeps its own
  strip and native Space. A focused on D2.
- When: focus left from A. Native verbs per profile: COSMIC
  `Focus(Left)`; Hyprland `movefocus l`; bspwm `node -f west`; i3
  `focus left`; xmonad Navigation2D `windowGo L`; sway `focus left`;
  qtile Columns `left()`; awesome `focus.global_bydirection("left")`;
  niri `FocusColumnLeft`; PaperWM `switchLeft(false)`; karousel
  `focus-left`; paneru `Focus(West)`; Ours Engine directional focus
  via the adapter `focus` op (`S(S-ours-focus)` + `S(S-ours-out)`).
- Observe: cross-output target and focus history vs local wrap or stay.
  Compare geometry with target workspace MRU.
- Then COSMIC: crosses to X on L: leftmost A exhausts the tiling walk
  (None), and under the shipped Vertical workspace layout the fallback
  runs `SwitchOutput(Left)`; `next_output` keeps the y-overlapping L by
  nearest origin distance, activates L and focuses its MRU-last target
  (sole X), with no layout or membership write.
  `S(S-cos-tilefocus)` + `S(S-cos-focus-fallback)` + `S(S-cos-move-out)`.
- Then Hyprland/Dwindle: crosses to X on L: the west query admits cross-monitor X under default-true fallback (full-height western overlap, no same-workspace competitor), else the monitor fallback selects L by STICKS longest-intersect with focus candidate X (sole window). No local wrap or stay. `S(S-hyp-focus)`.
- Then bspwm: focuses X on L (west search spans all monitors'
  focused desktops; X is the sole western candidate).
  `S(S-bsp-flt-focus)` + `S(S-bsp-move-target)`.
- Then i3: crosses to L (A is leftmost, so the walk climbs to the
  workspace level and `get_tree_next_workspace` returns L's visible
  workspace; `tree_next` shows it and focuses descended X).
  `S(S-i3-outfocus)` + `S(S-i3-flt-focus)`.
- Then xmonad/Tall+Navigation2D: crosses to X (`navigableWindows`
  covers all visible screens via `sortedScreens`, so the western X
  is a directional candidate of `windowGo L`). `S(S-xmo-scope)`.
- Then sway: crosses to L (output fallback after the tree walk;
  L sole X is the directional node). `S(S-sway-focus)`.
- Then qtile/Columns: A retained (`left()` steps in-group columns
  only; cross-screen needs `toscreen`, not this leg).
  `S(S-qti-focus)` + `S(S-qti-group)`.
- Then awesome/tile: crosses to X via `global_bydirection` (local
  miss then screen cross with mover re-activation). `S(S-awe-focus)`.
- Then niri: A retained (`FocusColumnLeft` returns false at index 0;
  cross-monitor needs the separate `FocusMonitor*` verb, not this
  leg). `S(S-nir-focus)` + `S(S-nir-mon)`.
- Then PaperWM: A retained (`switchLeft(false)` returns false at the
  first column; cross-monitor needs `switchMonitor`, not this leg).
  `S(S-pap-focus)` + `S(S-pap-mon)`.
- Then karousel/Lazy: fixture-inapplicable (single-screen profile).
  `S(S-kar-single)`.
- Then paneru: A retained (first-column A has no western peer for
  `Focus(West)`; West never falls through to another display, only
  North/South do); focus unchanged. `S(S-pan-focus)`.
- Then Ours KDE: crosses to X on L (Engine cross-output proposal
  selects the adjacent output domain's last-focused tiled leaf, and
  sole X is that leaf; actuated via `setActive`).
  `S(S-ours-focus)` + `S(S-ours-out)`.
- Then Ours Windows: same Engine cross to X as Ours KDE, actuated
  via `actuate_focus`. Foundation delivered offline; selector audit `fda618f`,
  2026-10-14 ([record](../../changes/archive/windows-position-based-output-selection.md));
  physical journey pending. `S(S-ours-focus)` + `S(S-ours-out)`.
- Variant hook: provisional/TBD (no suitable existing hook; V-R4-DIR
  covers moves, not focus).

### R-OUT-04: explicitly send a window to the other output

- Given (tree profiles): `L=H[X]`, `R=H[A*,B]` with A focused.
  Ordinary windows, no rules.
- Given (column profiles): `L=COL[C0[X]]`, `R=COL[C1[A*],C2[B]]` at
  shipped defaults; viewport recorded.
- Given (paneru): display `D1` owns strip `S1=COL[C0[X]]`, display
  `D2` owns strip `S2=COL[C1[A*],C2[B]]`; each display keeps its own
  strip and native Space.
- When: explicitly send A to L with the profile verb from the
  inventory above (not an edge move).
- Observe: carry/re-admit vs cross-output swap; follow vs retained
  source focus. Action differs from edge move.
- Then COSMIC: carries A to L's active workspace via `move_current`;
  `MoveToOutput` follows with focus on A, `SendToOutput` retains
  source focus (same send anchor as workspace send). `S(S-cos-out)`
  + `S(S-cos-send)`.
- Then Hyprland/Dwindle: carries A to L's workspace via
  `movetoworkspace`; follow switches monitor and focuses A,
  silent refocuses the source. `S(S-hyp-movews)`.
- Then bspwm: carries via `transfer_node` (unlink plus insert at L's
  focus X, second child, not a swap); unflagged form without `--follow`
  stays on the source, `--follow` focuses A on L.
  `S(S-bsp-send)` + `S(S-bsp-xfer)`.
- Then i3: carries A to L's visible workspace at TAIL (nodes
  `[X,A]`); mover-focused follow via `workspace_show` focuses A on
  L, same default wrapper/follow as the R-OUT-01 crossing.
  `S(S-i3-move)` + `S(S-i3-outmove)`.
- Then xmonad/Tall+Navigation2D: carries via `windowToScreen`
  (`shiftWin`: `insertUp` A above L target focus X, so L order `[A*,X]`
  with A the focused element there; source `delete'` is `filter (/=A)`,
  so source refocuses positionally down to sole B); no view switch
  (stay-like; no follow variant). `S(S-xmo-scope)` + `S(S-xmo-close)`.
- Then sway: carries A to L's active workspace at TAIL; focus stays
  on the mover A on L, same default wrapper/follow as the R-OUT-01
  crossing. `S(S-sway-move)` + `S(S-sway-outmove)`.
- Then qtile/Columns: carries via `togroup` (source `remove`, target
  `add`, not a swap); shipped follow form switches via `toscreen`
  with mover focus. `S(S-qti-group)` + `S(S-qti-add)`.
- Then awesome/tile: carries via `move_to_tag` (sets screen plus
  tags, no view switch); focused mover emits activate raise, so
  focus stays A. `S(S-awe-tag)`.
- Then niri: carries via `move_to_output` to L's active workspace
  plus `focus_output`, following A rather than swapping with X.
  `S(S-nir-mon)`.
- Then PaperWM: carries via `switchMonitor` with window carry
  (removes from the R space, changes to L's space, activates with
  focus on A); not the whole-space `moveToMonitor` swap fallback.
  `S(S-pap-mon)`.
- Then karousel/Lazy: fixture-inapplicable (single-screen profile;
  no output verb). `S(S-kar-single)`.
- Then paneru: carries by appending A to D1's selected strip with
  width-ratio preserved; Follow keeps A focused with the mouse
  warped to it. `S(S-pan-display)`.
- Then Ours KDE: explicit `send-to-output` carries A to L's current workspace
  using ordinary remembered-leaf/focus-history/root admission, not a swap or
  directional edge insertion. Follow Meta+Ctrl+Alt+arrows/HJKL focuses A;
  bindable unbound stay keeps source selection and focuses source MRU (B
  here). Sticky/intentional floats excluded; floating-workspace boundaries
  transfer membership only, tiled sides reflow. Shared core/protocol, KDE
  arrival/visibility fences and native catalog/presets delivered offline;
  [record](../../changes/archive/four-direction-output-transfer.md). Native journey TBD.
- Then Ours Windows: distinct output send delivered offline `fda618f`,
  2026-10-14 ([record](../../changes/archive/windows-explicit-output-send.md)).
  Win+Ctrl+Alt+arrows/HJKL follows; four unbound stay rows keep source MRU/null
  focus. Target current workspace uses ordinary remembered-leaf/history/root
  admission. Sticky/intentional floats excluded; floating boundaries preserve
  mover frames and reflow tiled sides. Candidate Engine/membership publish
  after fresh final fences; failure does not replay native writes. Native gates
  1,892 passed, zero failed, one ignored; physical two-output/DPI/suppression TBD.
- Variant hook: V-WS-FOLLOW (follow vocabulary reused from
  R-WS-01; follow/stay outcomes remain profile-specific).

### R-OUT-05: open a window with two occupied outputs

- Given (tree profiles): L focused `H[A*]`, R `H[B]`; pointer on R,
  no destination rules. Ordinary windows, scale 1.
- Given (column profiles): L focused `COL[C1[A*]]`, R
  `COL[C2[B]]` at shipped defaults; pointer on R; viewport recorded.
- Given (paneru): display `D1` focused with strip `S1=COL[C1[A*]]`,
  display `D2` with strip `S2=COL[C2[B]]`; pointer on D2; each
  display keeps its own strip and native Space.
- When: open C (ordinary admission, no rules).
- Observe: focused output vs pointer output vs app/startup
  assignment; newcomer focus and source view.
- Then COSMIC: lands on the focused output L (admission defaults to
  the seat active output) with newcomer focus. `S(S-cos-out)` +
  `S(S-cos-mapfocus)`.
- Then Hyprland/Dwindle: lands on focused L (initial map takes the focus monitor with no cursor branch absent rules; the pointer on R plays no role) with newcomer focus; source view unchanged. `S(S-hyp-newfocus)`.
- Then bspwm: lands on the focused desktop L with newcomer focus
  (ordinary admission anchors at the desktop focus). `S(S-bsp-insert)`.
- Then i3: lands on the focused workspace with newcomer focus
  (manage admits to focus unless assigned). `S(S-i3-admit)`.
- Then xmonad/Tall+Navigation2D: lands on the current screen with
  newcomer focus (`Operations.manage` on the focused workspace).
  `S(S-xmo-admit)`.
- Then sway: lands on the focused workspace with newcomer focus
  (seat focus-inactive anchor). `S(S-sway-ins)`.
- Then qtile/Columns: lands on the current group (L's screen) with
  newcomer focus (`group.add`). `S(S-qti-add)` + `S(S-qti-group)`.
- Then awesome/tile: lands on the selected tags (focused screen L)
  with newcomer focus (manage rule focus plus raise).
  `S(S-awe-manage)`.
- Then niri: lands on the focused output L (no-rule default is the active
  monitor; `open_on_workspace`, `open_on_output`, fullscreen-request and
  parent legs precede in that order); ordinary C admits as a new column
  after the active column at the default width with existing widths kept,
  and under Smart with no pending fullscreen it activates with focus to
  C with the view animating minimal-fit under shipped `Never`. Pointer
  on R plays no role. `S(S-nir-ins)` + `S(S-nir-base)` +
  `S(S-nir-wsopen)` + `S(S-nir-view)`.
- Then PaperWM: lands on the selected space (focused L; fresh
  windows redirect there) and activates on show. `S(S-pap-ins)`.
- Then karousel/Lazy: fixture-inapplicable (single-screen profile;
  no second output). `S(S-kar-single)`.
- Then paneru: C lands on the focused display D1's active strip after A
  (strip-end append: no rule insertion index under shipped defaults, and
  after-focus at the strip end appends); newcomer C focused (`dont_focus`
  false default); the pointer on D2 plays no role in routing.
  `S(S-pan-spawn)` + `S(S-pan-fresh)`.
- Then Ours KDE: focused-output routing delivered offline, with exact-birth,
  shown-target and eligibility fences plus synchronous known-managed focus
  capture before debounce. Sampled opposite-pointer native admissions land on
  the focused output and focus C in both directions; the inverse correction
  has routed/native membership evidence. Remaining timing/game variants TBD
  ([record](../../changes/kde-dual-output-live.md#focused-output-admission-correction-and-breadth-repeat-2026-10-14)).
  `S(S-ours-admit)` + `S(S-ours-out)`.
- Then Ours Windows: same Engine anchor leg as Ours KDE; output
  routing plus native activation TBD. `S(S-ours-admit)` +
  `S(S-ours-out)`; queued.
- Variant hook: provisional/TBD (contrast rule-targeted R-INS-07,
  which uses an explicit destination rule).

### R-OUT-06: disconnect and reconnect an occupied output

- Given (all profiles): L and R occupied with distinct workspaces;
  R focused. Record output identity and workspace mapping before
  acting. karousel excluded (single-screen, `S(S-kar-single)`).
- When: disconnect R (host output removal); then reconnect the same
  output. No agent hotplug; user host only.
- Observe: window/workspace evacuation, destination and focus; return
  affinity vs fresh reassignment on reconnect.
- Then COSMIC: occupied workspaces evacuate to the first remaining
  output L (occupied sets are kept: `can_auto_remove` needs empty plus no
  token/pin); L keeps showing its current occupied workspace, and the
  seat sync writes active-output L with focused-output cleared while the
  frame fixup retargets keyboard focus to L's shown-workspace MRU-last
  entry (exact node F TBD: L's contents/focus history unstated, so the MRU-last focused node is unspecified; primary F). Reconnect
  restores return affinity via the workspace `output_stack`
  (`prefers_output` reclaims same-edid/connector sets into a fresh set
  showing the first reclaimed entry). Settled client-visible rendering
  stays TBD (L: frame-driven fixup timing plus client ack unobserved).
  `S(S-cos-outremove)` + `S(S-cos-focusfix)`; node queued (F), visuals
  queued (L).
- Then Hyprland/Dwindle: disconnect moves R's workspaces to L (first remaining monitor; return address recorded, active remembered) via whole-workspace reassignment (float reposition, FS setBox, pin stays); reconnect returns them to the same address with the remembered active reactivated (return affinity, not fresh reassignment). Exact focused node TBD (F: R/L contents and focus history unstated). `S(S-hyp-monlife)`; queued.
- Then bspwm: retains R's monitor/desktops in place at shipped
  `remove_unplugged_monitors=false` (no evacuation: L keeps showing its
  current desktop, and the removal path writes no focus so R/L focus
  objects stay); reconnect with the same RandR identity reuses that
  monitor via `update_root` (return affinity, not fresh reassignment).
  Named remove-unplugged=true variant merges desktops into L
  before removing R, not destruction. `S(S-bsp-monrm)`.
- Then i3: disconnect evacuates R's workspaces to the first remaining output L via `move_content` (empty-unfocused workspaces destroyed, the rest detached/attached with floating-coordinate fix; focus follows the moved focused con via `con_focus` plus `workspace_show`, docks moved); L shows the moved focused workspace. Reconnect assigns R fresh via `init_ws_for_output` (assignment else first-assigned else first-free; no identity/affinity store, never consulting previous-workspace history), so the evacuated workspaces stay on L (fresh assignment, not return affinity). Exact evacuated focus node and fresh identity stay TBD (F: R/L contents, focus history and workspace names/numbers unstated; profile fixes shipped defaults with no custom rules). `S(S-i3-ws)`; focus/identity queued.
- Then xmonad/Tall+Navigation2D: no window/workspace migration in
  source: root `ConfigureEvent` runs `rescreen`, which reassigns the
  retained workspaces (stacks intact) across the new screen rects
  positionally via `getCleanedScreenInfo`/`nubScreens` and keeps the
  current workspace; the disconnected screen's workspaces become hidden
  with contents retained, never merged into L; reconnect reassigns
  positionally with no identity/affinity store (fresh reassignment, not
  return affinity). Focus stays with the retained current workspace,
  actuated via `setTopFocus` (`peek`, else root).
  `S(S-xmo-rescreen)` + `S(S-xmo-topfocus)`.
- Then sway: disconnect evacuates each workspace to its output-priority
  highest-available else output-order fallback (`root->outputs`
  items[0] else noop fallback), destroying empties (sticky-only empties
  evacuate stickies first); evacuate/restore issue no `seat_set_focus`
  themselves, but destroyed-empty focus is decided by the seat
  destroy-listener (sibling/workspace/last-known fallback, visible-only
  guard). Reconnect restores via the same priority affinity (workspaces
  whose highest-available is the new output move back, plus
  fallback-output workspaces), never consulting previous-workspace
  history. Exact evacuated and post-reconnect focus stays TBD (F: R/L
  contents and seat focus stack/history unstated).
  `S(S-sway-evac)`; focus queued.
- Then qtile/Columns: no evacuation merge in source: on host output removal the `screen_change` hook runs `reconfigure_screens`, which re-keys the remaining outputs and `hide`s groups whose screen left (contents retained, never merged into L; L keeps showing its group), with no focus retarget in the removal path (retained-`current_window` `layout_all` only). Reconnect re-runs the same assignment (`get_available_group` config-order scan, affinity-gated, no identity/affinity store), so the return mapping is fresh assignment, not stored return affinity. Exact evacuated focus and return identity stay TBD (F: R/L contents, focus history and group numbering unstated). `S(S-qti-screen)` + `S(S-qti-ws)`; focus/identity queued.
- Then awesome/tile: evacuates R clients to L's first tag via the shipped-default screen-removed fallback (tags emit `request::screen` with no rc handler, then `removal-pending` plus `request::tag`, then `delete` into the first tag of a remaining screen; history cleared); exact evacuated focus stays TBD (F: R-side focused client unstated, and the fallback path writes no focus). Reconnect creates fresh per-screen tags with no identity/affinity store (fresh reassignment, not return affinity). `S(S-awe-ws)`; focus queued.
- Then niri: disconnect moves R's workspaces to L before the trailing
  empty (`append_workspaces`) by preferred-output fallback to the primary
  (no remaining preferred when R is gone); L keeps showing its current
  workspace and focus resolves to L's active window. Reconnect returns
  workspaces to R by preferred-output affinity with the stored
  last-active workspace reactivated (return affinity, not fresh
  assignment). Exact focused node stays TBD (F: R/L contents and focus
  history unstated). `S(S-nir-ws)`; node queued (F).
- Then PaperWM: evacuation and return TBD (host GNOME Shell/Mutter output
  removal owns window/workspace migration; `workspacesChanged` only mirrors
  add/remove and establishes no window outcome). TBD; queued (host).
- Then karousel/Lazy: fixture-inapplicable (single-screen profile;
  no hotplug counterpart). `S(S-kar-single)`.
- Then paneru: TBD (macOS display disconnect is host-owned;
  virtual-row vs native-Space fate untraced). TBD; queued.
- Then Ours KDE: TBD (host output-removal domain journey plus
  Engine re-seed untraced). TBD; queued.
- Then Ours Windows: TBD (host topology journey untraced; single
  configured output in this profile). TBD; queued.
- Variant hook: provisional/TBD (host-topology hook, to discuss;
  distinct from R-WS-12 whole-workspace reassignment).

## Selected addition (USER 2026-10-07; delivery status per profile)

[Item 5](../../decisions.md#move-layout-and-output-commands) selects R-OUT-04's
explicit follow/stay output send; follow Meta/Win+Ctrl+Alt+arrows and +H/J/K/L,
stay bindable unbound. Four-direction reciprocal edge-touch and
positive-overlap adjacency uses FULL output rectangles; window-based selection
(User decision 2026-10-09): shared edge containing the moving window centre
projection, else larger window-span overlap, final left/top tie-break;
unreadable topology refuses, no candidate no-op, no wrap. Destination current workspace, ordinary
workspace-send admission and command follow/stay; initially tiled-subject
eligibility, sticky excluded, floating boundaries membership-only with tiled
sides reflowing; ordinary float transfer OPEN. Shared core/KDE selection
delivered offline 2026-10-09 with a real-Engine non-left/top send fixture
([record](../../changes/archive/position-based-output-selection.md)).
Windows wiring delivered offline `fda618f`, 2026-10-14
([record](../../changes/archive/windows-explicit-output-send.md)); physical
journeys pending. Pinned baseline source keys remain historical; dated delivery
evidence is linked separately in the platform cells.

### R-OUT-07: explicit send before local exhaustion with remembered Y

- Given (tree leg): aligned full rectangles L `(0,0,1920,1080)` and
  R `(1920,0,1920,1080)`, unique reciprocal adjacency. Source L
  `H[A*,B]` has a local right neighbor B; destination R's current workspace
  `H[X,Y]` has valid remembered leaf Y. Ordinary tiles, no rules or minimum
  constraints; Y's projected rectangle is tall.
- Given (column leg): source `COL[C1[A*],C2[B]]`, destination
  `COL[CX[X],CY[Y]]` with Y remembered; native model/admission distinction
  recorded, never substituted for recursive long-edge splitting.
- When: explicitly send A right with follow, before any local move;
  fresh reset, repeat with stay.
- Observe: transfer vs local restructure/swap, remembered Y admission vs
  root or X, current destination workspace, and follow/stay.
- Then COSMIC: transfers A to R's active workspace via `move_current`
  (source unmap, not a local restructure; local B never gates); the
  direction branch root-wraps (Vertical for Right, newcomer first, so
  `H[A,H[X,Y]]`), ignoring remembered Y (MRU split is the
  direction-less path only). `MoveToOutput` follows with focus on A,
  `SendToOutput` retains source focus (B sole). Exact pixel frames TBD
  (work area, decorations and native response unrecorded). `S(S-cos-out)` + `S(S-cos-send)` +
  `S(S-out07-cos-dirwrap)`.
- Then Hyprland/Dwindle: carries A to R's workspace via `movetoworkspace`
  (remove plus `assignToSpace` move/add; explicit send, so local B never
  gates); re-admission anchor is the mouse closest-node because the
  active-window candidate must share the target workspace while A lives
  on the source, so remembered Y is not a stored anchor; drop order
  follows the `force_split=0` pointer half. Follow switches monitor and
  focuses A with cursor warp; silent refocuses the source (B sole).
  Exact anchor/order and split frames TBD (cursor unrecorded).
  `S(S-hyp-movews)`.
- Then bspwm: carries via `node -m east` `transfer_node` (unlink with
  sibling promotion on L, so B sole; insert at R's desktop focus, not a
  swap; local B never gates); anchor is destination focus Y with A as
  second child under the longest-side axis (tall Y gives HORIZONTAL
  `(Y,A)`). Unflagged stays on the source; `--follow` focuses A on R.
  `S(S-bsp-send)` + `S(S-bsp-xfer)` + `S(S-bsp-insert)` + `S(S-out07-bsp-mon)`.
- Then i3: carries A to R's visible workspace via `move_to_output_directed`
  (closest east output; explicit send, so local B never gates); RIGHT
  attaches at HEAD (`[A,X,Y]`, focus TAIL) with mover-focused follow via
  `workspace_show` (no stay variant in this verb). Source detaches with
  percent fix (L sole B). Exact pixel frames TBD (work area, decorations
  and native response unrecorded). `S(S-i3-outmove)`.
- Then xmonad/Tall+Navigation2D: carries via `windowToScreen` (`shiftWin`:
  `insertUp` above target focus plus `delete'` source; separate verb from
  `windowSwap`, so local B never gates); A becomes the focused element
  on R's stack with no view switch (stay-like; no follow variant).
  Anchor is above target focus (Y's slot only when Y is target focus;
  no remembered-Y store). Source collapses to sole B. `S(S-xmo-scope)` +
  `S(S-xmo-shift)`.
- Then sway: carries via `move container to output right` to R's active
  workspace (adjacent output only; explicit send, so local B never
  gates); destination is the seat focus-inactive on R (Y when Y is MRU)
  with attach-after (`[X,Y,A]`) or workspace append. Outcome stays on
  the source (mover focus restored to source inactive B; no follow
  variant in this command, distinct from the directional-exhaustion
  next-output path). Exact frames TBD. `S(S-out07-sway-out)` +
  `S(S-sway-movews)`.
- Then qtile/Columns: carries via `togroup` to R's group (hide plus source
  `remove` plus target `add`, not a swap; local B never gates); Columns
  admits into Y's column at Y's slot (insert at current, newcomer
  focused; `insert_position` 0 puts A above Y when Y is current).
  Shipped follow (`switch_group=True`) switches via `toscreen` with mover
  focus; stay keeps the source view with source refocus (B). Source C1
  empties, so the column drops. `S(S-qti-group)` +
  `S(S-qti-add)` + `S(S-qti-remove)`.
- Then awesome/tile: carries via `move_to_tag` (screen plus tags set, no
  view switch; tile recalc on both screens over retained global order with no remembered-Y anchor; local B never gates);
  focused mover emits activate raise and stays visible on the destination tag, so focus stays A (view never
  follows; no stay/follow split). Exact tag order/pixel frames stay TBD (F: global insertion order and work area unrecorded). `S(S-awe-tag)` + `S(S-awe-tile)`.
- Then niri: carries via `MoveWindowToMonitorRight` (`move_to_output`
  remove/insert into R's active workspace plus `focus_output`; always
  follows, no stay variant; local B never gates); target admits as a new
  column after active (`active+1`; after Y when Y is active, so
  active-relative, not a remembered-Y store). Column leg: source C1
  sole-tile column drops whole (B remains, focus B). `S(S-nir-mon)` +
  `S(S-nir-ins)` + `S(S-nir-close)`.
- Then PaperWM: carries via `switchMonitor` with window carry (removes
  from the source space, `change_workspace` to R's space with
  `add_handler` re-inserting the existing window at the open position,
  `activateWithFocus`; always follows, no stay-with-window variant;
  local B never gates, not the whole-space `moveToMonitor` fallback).
  The open position is selected+1 under the shipped RIGHT default, so
  remembered Y is ignored (no memory store). `S(S-pap-mon)` +
  `S(S-pap-moveverbs)` + `S(S-pap-ins)`.
- Then karousel/Lazy: fixture-inapplicable (single-screen profile; no
  output verb, never a transfer vote). `S(S-kar-single)`.
- Then paneru: carries via `window nextdisplay` / `nextdisplaysend`
  (`ToNextDisplay`: removes from the source strip, appends to the target
  display's selected strip with width-ratio preserved; local B never
  gates); anchor is strip-end append (after CY, not at Y's slot; no
  remembered-Y store). Follow keeps A focused with the mouse warped;
  Stay refocuses the source neighbour (B here). `S(S-pan-display)`.
- Then Ours KDE: source collapses to B; A transfers to R's current workspace
  and admits at remembered Y (long-edge vertical split here); follow focuses
  A on R, stay keeps L selected and focuses B. Local neighbor B does not gate
  explicit send. Remembered/focus-history/empty-root admission, follow/stay
  and production-entry/Engine geometry/arrival tested offline;
  [record](../../changes/archive/four-direction-output-transfer.md). Exact native
  child-order journey TBD; item 5.3/5.4.
- Then Ours Windows: explicit transfer before local exhaustion delivered
  offline `fda618f`, 2026-10-14. Real-Engine remembered-leaf geometry differs
  from nearest-source R4 landing; follow/stay/current-view and failure fences
  pass ([record](../../changes/archive/windows-explicit-output-send.md)).
  Exact physical child-order, suppression and mixed-DPI journey TBD.
- Variant hook: V-WS-FOLLOW.
