# Mouse (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

## 10. Mouse drag

<a id="scrolling-backfill-additive-existing-wide-tables-above-unchanged"></a>
<a id="r-drag-01-backfill-drag-onto-a-column-centre-scrolling"></a>
### R-DRAG-01: drag onto centre (stack join)

- Given (tree profiles): `H[A,B*,C]`, B tiled

- Given (columns): `COL[C1[A],C2[B*],C3[C]]`, each 0.5W; VP recorded.
  Same identities and action as the original row: drag B onto C's
  centre; release.

- When: Drag B onto C's centre (not an edge); release

- Observe: Stack join vs refusal; source restoration

- Observe (column leg): column join vs swap/move/float-out; source restoration.

- Then COSMIC: Centre WindowStack drop converts the target to a stack and appends the mover's surfaces; tiling drags run in overview mode with drop-zone placeholders. The centre region is the rounded central thirds of the target frame, outside it the nearest edge picks the `WindowSplit` direction; `S(S-cos-drop)` + `S(S-cos-zone)` + `S(S-cos-dragedge)`; native preview delivery TBD
- Then Hyprland/Dwindle: No centre-stack mapping: ungrouped C cannot accept a group join (join needs a grouped hover + `drag_into_group` + grouping gates); B floats at threshold (siblings refill) then re-tiles via fresh Dwindle admission, no old-slot restore; exact re-admission position TBD (release geometry unrecorded); `S(S-hyp-drag)` + `S(S-hyp-dragend)` + `S(S-hyp-ins)`
- Then bspwm: No stack join: pointer ACTION_MOVE hover over tiled C swaps B/C in place (`H[A,C,B*]`); same-desktop swap writes borders only so focus stays B; centre-vs-edge not distinguished (no zone check in the tiled path; the Given centre lands inside C); the tiled mover never leaves the tree mid-hold, so no source-slot restore step; `S(S-bsp-drag)` + `S(S-bsp-flt-swap)`
- Then i3: No COSMIC stack join: centre DT_CENTER (cursor outside 30% edge bands) runs `con_move_to_target` without swap (Shift swap-modifier `con_swap` instead); tiled producer needs an enabled `tiling_drag` path (shipped modifier+titlebar, code default modifier-only) with >1 drop target, else no drag starts; self-centre/Esc/NULL-target aborts with indicator destroy and no mutation; `S(S-i3-tdrag)` + `S(S-i3-tdrop)`; exact centre pixel/swap state and native preview TBD
- Then xmonad/Tall+Navigation2D: No centre-stack mapping in source (Tall/Mirror Tall/Full only, no stack/tab join verb): `mod-button1` focuses B then installs the move grab with press-time `shiftMaster` (focused B to master, focus stays B); per-motion `moveWindow` writes the raw frame plus `float` on each motion and button-release `done` `float`s again; survivors refill via Tall recalc over the float-excluded set with no peer mutation and no slot restore; no zone/centre check, preview, placeholder, or threshold in the path; `S(S-xmo-mouse)` + `S(S-xmo-layout)` + `S(S-xmo-arrange)` + `S(S-xmo-master)`.
- Then sway: No COSMIC stack join: content-centre (outside 30% edge bands, not a titlebar) runs centre `container_swap` with the hovered target (titlebar hover instead tabifies via `container_split` L_TABBED + indexed insert); needs enabled `tiling_drag` (shipped enabled, threshold 9) with >1 tiling view, else NULL abort; self/descendant-centre NULL aborts with indicator destroy and no mutation; swap preserves mover focus on the same workspace; `S(S-sway-tdrag)` + `S(S-sway-tdrop)`; exact centre pixel and native preview TBD
- Then qtile/Columns: No stack join: pointer over C runs the explicit set_position swap (B/C exchange places with heights, no centre/edge distinction); shipped Mod+Button1 instead runs set_position_floating (floating tweak, not the tiled swap); exact hover pixel and drop-side focus TBD; `S(S-qti-drag)` + `S(S-qti-tweak)`
- Then awesome/tile: No centre-stack mapping in source (no stack/tab join verb; shipped layouts floating plus tile variants/fair/spiral/max/magnifier/corner per `S(S-awe-default)`, layout per-tag per `S(S-awe-layout)`; tiled set excludes float/max/full per `S(S-awe-tile)`): tiled mouse.move over C swaps B/C global-client positions via move_handler with tile recalc settling frames (centre-vs-edge not distinguished, no zone check in the path; swap writes no focus); a floating B would move its frame via the floating geometry path instead; shipped Mod4+Button1 client move and titlebar Button1 move share the same activate-with-mouse_move path; no source-slot placeholder/restore in the tiled path (snap placeholder is floating aerosnap only); column leg runs the same swap with no column-join verb. `S(S-awe-drag)` + `S(S-awe-swap)` + `S(S-awe-tile)` + `S(S-awe-layout)` + `S(S-awe-default)`
- Then niri: the drop re-inserts B at the pointer insert position,
  which resolves to a new-column split or an in-column member-add by
  pointer geometry (`scrolling_insert_position`: `NewColumn` when the
  column gap is nearest, else `InColumn` committed via
  `add_tile_to_column`); no swap and no float-out (scrolling B stays
  scrolling), and the removed tile is not restored to its source
  slot. The pointer path carries no consume/expel verb (separate
  keyboard Action). Exact branch stays TBD (F: centre pixel and
  tile/work-area geometry unrecorded, so the gap-vs-tile distance
  comparison is unspecified). `S(S-nir-drag)` +
  `S(S-nir-consume)`; queued.
- Then PaperWM: centre x lands in the column body, so
  `selectDndZone` yields only a within-column `[j,i]` or column
  `[j]` insert committed via `addWindow` with activation; the grab
  path invokes no slurp/barf/join verb. The emptied source column
  splices away at removal, so no source-slot restore. Exact `[j,i]`
  row TBD (pointer y unrecorded). `S(S-pap-grab)` +
  `S(S-pap-grabzone)` + `S(S-pap-slurp)` + `S(S-pap-close)` +
  `S(S-pap-layout)`; queued.
- Then karousel/Lazy: pointer drag untiles B to float under shipped
  `untileOnDrag=true`, so no column join or source-slot restoration
  occurs. `S(S-kar-ptr)`.
- Then paneru: pointer drag is the host macOS journey (no engine
  drag model; MouseDragged forwards to Lua only). `S(S-pan-mouse)`;
  queued.
- Then Ours KDE: Centre stack request refused (snap-back); `D(D-dec-cos)` + `D(D-dec-nest)`; physical check pending
- Then Ours Windows: Centre stack request refused (snap-back); `D(D-dec-cos)` + `D(D-dec-nest)`; physical check pending
- Variant hook: V-DRAG-ZONE.

<a id="r-drag-02-backfill-drag-to-a-between-column-bar-scrolling"></a>
### R-DRAG-02: drag to a between-child bar

- Given (tree profiles): `H[A,B,C]` equal (640 each at 1920); existing N outside that group

- Given (columns): `H[A,B,C]` equal (640 each at 1920) projected to
  `COL[C1[A],C2[B],C3[C]]` plus existing N outside that group; VP
  recorded. Same action: drag N to the bar between A and B.

- When: Drag N to the between-child bar between A and B; release

- Observe: Flat vs nested; mover share (n=4 after insertion)

- Observe (column leg): flat insert at index vs nested/no-op; mover share (n=4
  after insertion in the original predicate).

- Then COSMIC: Flat `H[A,N,B,C]`, all 480 (mover 1/n, peers scaled); `UT(2026-08-22)` + [Test B](../../cosmic-move-conformance.md#follow-up-manual-observations-tests-a-c); GroupInterior drop inserts at the hovered index; `S(S-cos-drop)`
- Then Hyprland/Dwindle: No between-child bar/index mapping in source; if N starts tiled it floats at threshold then re-tiles via the Dwindle admission anchor at the drop point, while ordinary floating N stays floating absent a successful group join (ungrouped A/B/C offer none); exact outcome TBD (N initial state, binary embedding/bar equivalent, and drop geometry unspecified); `S(S-hyp-drag)` + `S(S-hyp-dragend)` + `S(S-hyp-ins)`
- Then bspwm: No between-child bar/index mapping: a bar hover is unmanaged (root/gap), so no swap/insert occurs and N stays put on the same monitor (off all monitors likewise no-ops); a cross-monitor point instead transfers to that monitor's focus; exact bar pixel/N start state TBD; `S(S-bsp-drag)`
- Then i3: No between-child bar/index mapping: the bar x resolves via nearest-edge direction into DT_SIBLING (inside the 30% edge band, split if parent orientation differs then `insert_con_into`) or DT_CENTER (`con_move_to_target`, or `con_swap` with Shift); orientation comes from the edge direction, not splith default; focus preserved; `S(S-i3-tdrag)` + `S(S-i3-tdrop)`; exact bar pixel, N start state (tiled vs floating) and mover share/frames TBD (drop geometry unspecified)
- Then xmonad/Tall+Navigation2D: No between-child bar/index mapping in source (no index-insert verb; the pointer path is raw frame writes plus `float`, never admission): N stays floating at its dragged frame with press-time `shiftMaster` (focus stays N), whether N starts tiled (first motion floats it) or floating (stays floating); no flat/nested insert, no 1/n share; bar-vs-window distinction absent (no zone check in the path); `S(S-xmo-mouse)` + `S(S-xmo-master)`.
- Then sway: No between-child bar mapping as such: a titlebar hover inserts flat at the computed bar index via `split_titlebar`/`split_border` (`H[A,N,B,C]` shape when bars are the hover); content hover resolves via nearest-edge into edge split+insert vs centre swap, orientation from the edge direction; the mover adopts a sibling share (finalize copies sibling fractions); `S(S-sway-tdrag)` + `S(S-sway-tdrop)`; exact bar pixel, N start state and frames TBD (drop geometry unspecified)
- Then qtile/Columns: No between-child bar/index mapping in source: a bar hover hits no window, so set_position swaps nothing and N stays; shipped Mod+Button1 instead tweaks N's frame on the floating path; exact outcome TBD (producer, N float state, and bar pixel unspecified); `S(S-qti-drag)` + `S(S-qti-tweak)`
- Then awesome/tile: No between-child bar/index mapping in source: bar/gap hover hits no tiled client, so move_handler swaps nothing and N stays (no flat insert, no 1/n share; shares are master/stack plus windowfact); exact bar pixel/N start/focus TBD; `S(S-awe-drag)` + `S(S-awe-tile)`
- Then niri: for a scrolling N, the drop re-inserts at the pointer
  insert position (flat new column at the bar gap when the gap is
  nearest; no nesting); N keeps its dragged width and peers keep
  their widths (no 1/n rescale). A floating N instead stays floating
  (`InsertPosition::Floating`, layer preserved). N's start state is
  unrecorded, so the flat-vs-float outcome and mover share stay TBD
  (F: N tiled vs floating unstated; bar pixel unrecorded).
  `S(S-nir-drag)`; queued.
- Then PaperWM: the bar maps to the `[j]` column zone, so the
  drop inserts a new tiled-N column at the zone index via
  `addWindow`; the mover column takes the mover frame width while
  peers keep widths (no 1/n rescale), then the drop activates. A
  floating N never enters the PaperWM move grab. `S(S-pap-grab)` +
  `S(S-pap-grabzone)` + `S(S-pap-layout)`.
- Then karousel/Lazy: a tiled N untiles at session start under shipped
  `untileOnDrag=true` via `floatClient` (height-capped float), so no
  bar-index insert occurs; A/B/C keep widths with reposition only (no
  1/n rescale); the script writes no focus on the float path.
  `S(S-kar-ptr)` + `S(S-kar-float)` + `S(S-kar-close)`.
- Then paneru: pointer drag is the host macOS journey (no engine
  drag model). `S(S-pan-mouse)`; queued.
- Then Ours KDE: TBD (between-child drop not checked here)
- Then Ours Windows: TBD (between-child drop not checked here)
- Variant hook: V-DRAG-ZONE.

<a id="r-drag-03-backfill-two-drag-producers-same-drop-scrolling"></a>
### R-DRAG-03: two drag producers, same drop

- Given (tree profiles): `H[A,B*]`, both tiled

- Given (columns): `COL[C1[A],C2[B*]]`; VP recorded. Same action:
  title-bar drag B to A's top edge; repeat with Mod+Left client drag.

- When: Title-bar drag B to A's top edge; repeat from the same start with Meta/Win+left client drag

- Observe: Same drop topology and mover; no client click or sibling reflow before drop

- Observe (column leg): same drop topology across producers; no client click or
  reflow before drop.

- Then COSMIC: No client click on either path: Super-held presses are suppressed in the compositor, title-bar drags start server-side; the grabbed source unmaps to a `GrabbedWindow` placeholder holding its slot (siblings do not reflow into it). A's top edge resolves to `WindowSplit` Up (nearest-edge pick; centre-thirds would stack instead); `S(S-cos-dragstart)` + `S(S-cos-dragedge)`; native reflow frames TBD
- Then Hyprland/Dwindle: Dispatcher support only, not a default-binding claim: `mouse:movewindow` begins a drag on hit, decoration `DRAG_START` hit skips it; threshold default 0 floats B at grab so siblings refill mid-hold (no slot placeholder); drop re-tiles via fresh admission; same-topology across producers and client-click behavior TBD (release geometry/client ack unrecorded); `S(S-hyp-keyend)` + `S(S-hyp-drag)` + `S(S-hyp-dragend)`
- Then bspwm: Producers differ: only the modifier+button1 ACTION_MOVE producer exists (per-motion hover swap; top-edge hover over A swaps A/B in place with borders-only and no focus write; the press is consumed with no client click and siblings reflow only on swap); a bare title-bar drag without the pointer modifier starts no grab in source, so the title-bar leg never runs and cross-producer parity never arises; `S(S-bsp-drag)`
- Then i3: Both producers start the same tiled drag under shipped `tiling_drag modifier titlebar` (modifier+left anywhere incl client; titlebar left without modifier; code default modifier-only refuses the titlebar-only path): modifier path starts immediately before focus, titlebar path focuses first then drags thresholded (~15px) with no client click in either case; grabbed source stays mapped with indicator-only preview (siblings do not reflow mid-hold); A's top edge is nearest-edge DT_SIBLING Up (outer thin band would be DT_PARENT instead); `S(S-i3-tdrag)` + `S(S-i3-tdrop)`; exact edge pixel/parent-band and native frames TBD
- Then xmonad/Tall+Navigation2D: Producers share no drop topology: only `mod-button1` (move) and `mod-button3` (resize) drags exist, both float on motion/release plus `shiftMaster`; a bare title-bar drag without the modifier has no counterpart in source, so cross-producer same-topology never runs; no edge/centre topology, no placeholder; `S(S-xmo-mouse)`
- Then sway: Both producers start the same tiled drag under settled-enabled `tiling_drag` (modifier+left anywhere incl client; titlebar left without modifier; code defaults enabled/threshold 9, unlike the i3 modifier-only default): press focuses first via the generic click-focus path with no client button forwarded before the grab, then modifier begins immediately while titlebar waits for the output-scaled 9px threshold; the grabbed source stays attached with indicator-only preview (detach only at finalize; siblings do not reflow mid-hold); A's top edge is a 30px/30% edge split Up (the outer layout-border walk may take a layout parent instead); `S(S-sway-tdrag)` + `S(S-sway-tdrop)`; exact edge pixel/parent-band and native frames TBD
- Then qtile/Columns: Producers differ: shipped Mod+Button1/Button3 are floating tweaks (frame write plus closest-screen transfer, never the tiled set_position swap); a bare title-bar drag has no counterpart in the shipped bindings; cross-producer same-topology TBD (title-bar leg unevidenced); `S(S-qti-tweak)` + `S(S-qti-drag)`
- Then awesome/tile: Both producers share the same tiled move path (shipped Mod4+Button1 client move and titlebar Button1 move both activate with the mouse_move action); press activates B (focus plus raise via permissions.activate; ordinary focusable B passes while the move guard excludes fullscreen/maximized/desktop/splash/dock); source stays mapped mid-hold with siblings reflowing only on hover-swap via recalc (no placeholder/scale/preview in the tiled path; snap placeholder is floating aerosnap only); A's top edge hovers A, so the drop swaps B/A with the mover retained; no client click delivery on either producer path and no reflow before the hover-swap; column leg runs the same shared path. `S(S-awe-drag)` + `S(S-awe-swap)`
- Then niri: both producers use MoveGrab ending in the same
  pointer-insert drop (same topology and mover); the titlebar leg's
  initiating press is client-delivered (the move request is
  serial-qualified on a same-client press), while the Mod+Left press
  is consumed compositor-side (grab with `Focus::Clear`, motion with
  no client focus); neither path delivers a completed client click
  during the move, and the source stays mapped until the move starts
  with survivors refilling after removal. The client path also
  permits horizontal viewport scrolling, unlike Mod+Left.
  `S(S-nir-drag)` + `S(S-nir-clientgrab)`.
- Then PaperWM: title-bar and Mod+Left presses both arrive as
  `MOVING` and construct the same `MoveGrab` with the same DnD
  zones; `begin` connects button-release/touch/motion/monitor
  signals only, so any client-click delivery rides the host grab.
  Scroll-phase moves clones only; source removal and reflow start
  at `beginDnD`. Click delivery TBD (host Mutter grab semantics).
  `S(S-pap-grab)` + `S(S-pap-grabzone)`; queued.
- Then karousel/Lazy: the move/resize session hooks carry no producer
  branch, so both producers funnel identically to untile-or-snap-back
  under shipped `untileOnDrag=true` (float via `floatClient`, else the
  `moving` retile-back on finish); the script writes no click and holds
  no drop topology; host session start/finish rides the KWin interactive
  move/resize signals. `S(S-kar-ptr)` + `S(S-kwin-moveresize)`.
- Then paneru: pointer drag is the host macOS journey (no engine
  drag model). `S(S-pan-mouse)`; queued.
- Then Ours KDE: same resolver selected `D(D-dec-drag)`; foreground and
  hidden-output reconcile dispatch/replies are held during an active drag,
  with fresh recovery after release, proven offline
  ([record](../../changes/kde-dual-output-live.md#cross-output-drag-lifecycle-correction-2026-10-14));
  physical producer parity remains TBD.
- Then Ours Windows: both producers delivered with three-window synthetic preview/drop agreement and mid-hold sibling stability; exact row TBD `D(D-win-drag)`
- Variant hook: V-DRAG-ZONE.

<a id="r-drag-04-backfill-esc-during-a-column-drag-scrolling"></a>
### R-DRAG-04: Esc during a drag

- Given (tree profiles): `H[A,B*]`, both tiled

- Given (columns): `COL[C1[A],C2[B*]]`; VP recorded. Same action:
  start moving B; press Esc; release.

- When: Start moving B; press Esc; release

- Observe: Source topology/geometry retained; no drop plan; preview cleared

- Observe (column leg): source restoration vs drop/persist; no drop plan.

- Then COSMIC: Bare Escape intercepts to `PrivateAction::Escape`, which unsets the pointer grab; the move-grab `unset` is a no-op so dropping the grab runs the normal `Drop` `drop_window` at the current hover: no cancel path exists, and a zero-move drop restores via the `InitialPlaceholder`; `S(S-cos-dragesc)` + `S(S-cos-drop)`; moved-then-Escaped exact topology TBD (fixture states no hover/drop point, so which `drop_window` branch runs is unspecified; the normal-drop mechanism itself is proven)
- Then Hyprland/Dwindle: No Esc cancel path: any key press (Esc included) runs the normal `endDragTarget` drop, never a restore; moved-then-Esc exact topology TBD (hover/drop point unrecorded); `S(S-hyp-keyend)` + `S(S-hyp-dragend)`
- Then bspwm: No Esc/key cancel path: pointer grabs end only on button release; key presses run the generic event switch with no grab-cancel branch, so performed hover swaps persist and release ends normally; moved-then-Esc exact topology TBD (hover/drop point unspecified); `S(S-bsp-drag)`
- Then i3: Any key press (Esc included) during the drag reverts: `tiling_drag` aborts with indicator destroy and no tree/focus mutation, never a drop-at-hover; `S(S-i3-tdrag)`; no drop plan and no preview residue by source
- Then xmonad/Tall+Navigation2D: No Esc/key cancel path in source: the pointer grab ends only on button release, which runs release `done` (`float`, never a restore); performed motion persists as B floating at its written frame plus `shiftMaster`; moved-then-Esc exact topology TBD (hover/drop point unspecified; the normal-release mechanism itself is sourced); `S(S-xmo-mouse)`
- Then sway: No key-press cancellation hook: bare Esc does not revert or end the tiled drag; release runs the normal `finalize_move` at the current hover (NULL hover aborts with no mutation, otherwise drops); diverges from the i3 any-key revert; `S(S-sway-tdrag)` + `S(S-sway-tdrop)`; moved-then-Esc exact topology TBD (hover/drop point unspecified; the normal-drop mechanism itself is sourced)
- Then qtile/Columns: No key-cancel branch in the inspected drag inventory (set_position swap, floating tweaks, shipped Drag/Click bindings only): release runs the normal swap-or-tweak at the current pointer, so performed motion persists; moved-then-Esc exact topology TBD (hover/drop point unspecified); `S(S-qti-drag)` + `S(S-qti-tweak)`
- Then awesome/tile: No key-cancel branch in the inspected drag inventory (grab ends on button release only; leave callbacks run the normal geometry emit); release drops at the current hover (swap-or-move), so performed motion persists; moved-then-Esc exact topology TBD (hover/drop point unspecified); `S(S-awe-drag)`
- Then niri: the pointer-grab impl carries no key path
  (motion/button/axis/frame only), so Esc cannot cancel; release
  commits via interactive_move_end. Exact topology TBD (hover/drop
  point unspecified). `S(S-nir-drag)`; queued.
- Then PaperWM: `MoveGrab` connects button-release/touch-end/
  motion/monitor signals only, with no extension key path (the
  navigator Esc destroys the keyboard dispatcher, never the drag),
  so the extension offers no cancel; whether the host intercepts
  Esc before release is unpinned. Absent host interception,
  release runs the normal `end()` (zone drop, scratch-temp, or
  in-space restore) with zone actors destroyed. Which `end()`
  branch runs TBD (hover/drop point unrecorded).
  `S(S-pap-grab)` + `S(S-pap-grabzone)`; queued.
- Then karousel/Lazy: host KWin move-session cancel untraced in the
  profile source (karousel finish only retiles). Queued.
- Then paneru: pointer drag is the host macOS journey. `S(S-pan-mouse)`;
  queued.
- Then Ours KDE: cancelled verdict makes no plan and clears preview `D(D-dec-drag)`
- Then Ours Windows: synthetic title/Win Esc restores all frames without mutation, Win preview hidden before Up; physical edge/exact row TBD `D(D-win-drag)`
- Variant hook: V-DRAG-ZONE.

<a id="r-drag-05-backfill-zero-move-pressrelease-scrolling"></a>
### R-DRAG-05: zero-move press/release

- Given (tree profiles): `H[A,B*]`, both tiled

- Given (columns): `COL[C1[A],C2[B*]]`; VP recorded. Same action:
  press/release the move gesture on B without moving.

- When: Press/release the move gesture on B without moving

- Observe: No topology/share change or preview residue

- Observe (column leg): no mutation or preview residue.

- Then COSMIC: Client-initiated moves stay `Delayed` below 1px motion; a no-move tiling drop restores the source slot via `InitialPlaceholder`; `S(S-cos-dragthresh)`; native residue TBD
- Then Hyprland/Dwindle: Threshold default 0 picks B up immediately (float + sibling refill), so release re-tiles via fresh admission rather than slot restore; exact rebuilt topology TBD (no motion/drop point recorded); no preview/placeholder residue path established in source; `S(S-hyp-drag)` + `S(S-hyp-dragend)`
- Then bspwm: No mutation: zero-move press/release runs no MOTION path so `move_client` is never called; any sub-threshold motion over B's own window returns false (pointer still over self), so no swap/transfer occurs; no threshold pickup, placeholder, or indicator residue path in source; `S(S-bsp-drag)`
- Then i3: No mutation: titlebar-path press/release stays under the ~15px threshold so the callback never runs (NULL target abort); modifier-path immediate pick-up released over its own centre hits the self-centre no-draw abort; both destroy the indicator with no preview residue; `S(S-i3-tdrag)` + `S(S-i3-tdrop)`; native residue TBD only outside this source path
- Then xmonad/Tall+Navigation2D: Not no-mutation here: the grab (`buttonReleaseMask` + `pointerMotionMask`) carries no threshold branch, so zero-move press/release still runs release `done` (`float w`) with press-time `shiftMaster` - B floats at its unchanged frame and becomes master with focus retained, survivors refill around the floater via Tall recalc over the float-excluded set; no preview/placeholder residue path in source; `S(S-xmo-mouse)` + `S(S-xmo-arrange)` + `S(S-xmo-master)`.
- Then sway: No mutation: titlebar press/release under the output-scaled 9px threshold never reaches post-threshold targeting (`finalize_move` with NULL target returns to default with indicator destroy); modifier immediate pick-up released over its own centre/titlebar hits the self/descendant NULL abort (source-titlebar cancel + self-centre guard); both destroy the indicator with no preview residue; `S(S-sway-tdrag)` + `S(S-sway-tdrop)`; native residue TBD only outside this source path
- Then qtile/Columns: No mutation: with the pointer still over B's own frame set_position skips self and finds no other hit, so no swap occurs; a zero-delta floating tweak rewrites the same frame; no threshold pickup, placeholder, or indicator residue path in source; `S(S-qti-drag)` + `S(S-qti-tweak)`
- Then awesome/tile: No mutation on zero-move: pointer still over B's own frame gives no other hovered tiled client, so move_handler swaps nothing (self guarded); floating path rewrites the same frame; grab ends on release with no threshold pickup/placeholder/preview residue in source; `S(S-awe-drag)`
- Then niri: the 8px gesture threshold is never reached, so the tile
  is never removed and release only activates B; no topology change
  and no preview residue (the insert hint exists only while moving).
  `S(S-nir-drag)`.
- Then PaperWM: DnD never begins, so B stays in place; end always
  activates B. `S(S-pap-grab)`.
- Then karousel/Lazy: a started move session untiles immediately even
  without geometry motion (no script threshold), so a started session
  floats B with no slot restore; Mod+Left starts that session immediately
  via the host Move path so zero-move floats B, while title-bar press only
  arms a delayed start after `startDragTime` so a quick zero-move
  press/release starts no session and B stays tiled. Title-bar hold
  duration vs the delay is unrecorded, so that branch stays TBD.
  `S(S-kar-ptr)` + `S(S-kwin-moveresize)`; title-bar hold TBD (F:
  press-hold duration vs `startDragTime` unrecorded).
- Then paneru: press marks held with no membership/order/share write
  and release runs only the click reshuffle around B; no layout drag
  model exists (no preview to clear); no membership/order/share change
  and no preview residue on this zero-move press/release (strip-offset
  pixels outside this Observe). `S(S-pan-mouse)`.
- Then Ours KDE: no-change verdict makes no plan `D(D-dec-drag)`
- Then Ours Windows: synthetic title/Win zero-move preserves all frames without mutation or preview residue on a three-window fixture; exact row TBD `D(D-win-drag)`
- Variant hook: V-DRAG-ZONE.

<a id="r-drag-06-backfill-release-outside-the-work-area-scrolling"></a>
### R-DRAG-06: release outside the work area

- Given (tree profiles): `H[A,B*]`, one output with a panel/taskbar outside the work area

- Given (columns): `COL[C1[A],C2[B*]]` with a panel/taskbar outside
  the work area; VP recorded. Same action: move B; release over the
  panel.

- When: Move B; release over the panel/taskbar outside the work area

- Observe: Source restoration vs off-area placement; preview cleared

- Observe (column leg): source restoration vs off-area placement; preview cleared.

- Then COSMIC: Drop geometries come from the work-area `non_exclusive_zone`, so a pointer over the panel matches no tile geometry and yields no zone: the hover placeholders clear and the drop falls back to a fresh `map_to_tree` admission inside the work area (neither off-area placement nor source-slot restore); `S(S-cos-dropzone)`; realized native frames TBD
- Then Hyprland/Dwindle: No off-area placement and no slot restore: release re-tiles inside the work area via fresh admission (floating-middle monitor check only moves workspaces, never parks on panels); exact frames TBD (panel/monitor geometry unrecorded); `S(S-hyp-drag)` + `S(S-hyp-dragend)`
- Then bspwm: No off-area placement and no source-restore step: panel hover is unmanaged on the same monitor (no swap), off all monitors likewise no-ops; tiled B never leaves the tree mid-hold; which branch runs TBD (panel geometry/containment unrecorded); no preview residue path; `S(S-bsp-drag)`
- Then i3: No off-area placement: pointer off all outputs yields NULL target and aborts with no mutation; pointer inside the output but outside tiles falls back to the visible-workspace admission inside the work area; `S(S-i3-tdrop)`; which branch runs is TBD (panel/taskbar geometry and output containment unrecorded); indicator destroyed either way
- Then xmonad/Tall+Navigation2D: Off-workarea frame retained with no source-restore step: per-motion `moveWindow` writes the raw frame with no zone/work-area check and release `float`s there with press-time `shiftMaster`; no admission fallback parks it back inside; no preview residue path in source; single-output fixture stays on its screen via `pointScreen`; `S(S-xmo-mouse)` + `S(S-xmo-master)`.
- Then sway: No off-area placement: pointer over a layer surface (panel/taskbar) yields a NULL node and NULL target, and release aborts with no mutation (indicator destroyed at seatop end); only workspace/edge targets inside the output admit, never parking on the panel; `S(S-sway-tdrop)`; which branch runs is TBD (panel/taskbar geometry and output containment unrecorded); indicator destroyed either way
- Then qtile/Columns: No off-area placement via set_position (panel hover hits no tiled window, so no swap; tiled B never leaves the layout mid-hold); floating tweak_float transfers across screens by closest-screen only, never parks on panels; which branch runs TBD (producer and panel geometry unrecorded); no preview residue path; `S(S-qti-drag)` + `S(S-qti-tweak)`
- Then awesome/tile: No off-area placement via the tiled path: panel/taskbar hover hits no tiled client so no swap occurs and tiled B never leaves the layout mid-hold (screen follow only on actual screen change); which branch runs TBD (panel geometry/containment unrecorded); no preview residue path; `S(S-awe-drag)`
- Then niri: the grab survives off-output pointer positions and the
  end re-inserts at the last tracked output (no restore path and no
  off-area parking: a panel hover still resolves to a work-area insert
  position); the insert hint clears when the grab ends.
  `S(S-nir-drag)`.
- Then PaperWM: with no zone acquired, the no-target `end()`
  branch moves the frame out and scratch-temps B then unmakes
  scratch on animation completion (float/above/sticky cleared),
  re-entering via window-added as an existing insert at the
  open-position index; zone actors destroyed, never a source-slot
  restore or off-area parking. Which branch runs TBD (pointer path
  unrecorded; leaving all zones keeps the last acquired zone).
  `S(S-pap-grab)` + `S(S-pap-grabzone)` +
  `S(S-pap-ins)`; queued.
- Then karousel/Lazy: untile at grab start leaves no restoration path,
  so the drop stays a host-positioned float (no zone/admission fallback
  in script); there is no script drop preview to clear.
  `S(S-kar-ptr)` + `S(S-kar-float)` +
  `S(S-kwin-moveresize)`.
- Then paneru: pointer drag is the host macOS journey. `S(S-pan-mouse)`;
  queued.
- Then Ours KDE: unresolved target snaps back `D(D-dec-drag)`
- Then Ours Windows: title/Win taskbar-outside refusal restores all frames without mutation or preview residue on a three-window fixture; exact row TBD `D(D-win-drag)`
- Variant hook: V-DRAG-ZONE.

<a id="r-drag-07-backfill-dragged-frame-vs-retained-allocation-scrolling"></a>
### R-DRAG-07: dragged frame vs retained allocation

- Given (tree profiles): `H[A,B*]`, both tiled

- Given (columns): `COL[C1[A],C2[B*]]`; VP recorded. Same action:
  Mod+Left client drag B to A's edge; pause before release.

- When: Meta/Win+left client drag B to A's edge; pause before release

- Observe: Native frame follows pointer vs retained source allocation with target-slot preview; final placement unchanged

- Observe (column leg): native frame follows pointer vs retained allocation with
  preview; final placement unchanged.

- Then COSMIC: Tiling mover image follows the pointer at retained client size (pointer motion writes the grab location, render translates retained geometry by location+offset), scaled 0.6->1.0 over 150ms (0.4 alpha on other outputs), with a `StackHover` indicator and overview drop-zone placeholders; the source unmaps to a `GrabbedWindow` placeholder at grab start; `S(S-cos-dragframe)`; native pixels/timing TBD
- Then Hyprland/Dwindle: Dragged frame follows the pointer (floating position writes + warp) while the tiled source already refilled at pick-up; no target-slot preview/placeholder established in source; final placement is the drop re-tile, exact position TBD; `S(S-hyp-drag)` + `S(S-hyp-dragend)`; visuals/timing TBD
- Then bspwm: Retained allocation mid-hold: the tiled mover stays in tree until a hover swap/transfer (no pointer-following frame write, no scale/alpha/indicator or target-slot preview in source); siblings reflow only on swap/transfer; swaps run on motion and release only ends the grab, so the pause itself writes nothing and the post-release tree equals the no-pause run; `S(S-bsp-drag)`
- Then i3: Retained allocation mid-hold: tiled `tiling_drag` never writes the source frame, it only draws the `i3-drag` drop indicator at the target slot (siblings do not reflow until drop); this is the floating-modifier client path, distinct from `floating_drag_window` which moves the floating frame; final placement follows the DT_SIBLING/CENTER/PARENT branch for A's edge; `S(S-i3-tdrag)` + `S(S-i3-tdrop)`; native pixels/timing TBD
- Then xmonad/Tall+Navigation2D: Frame follows the pointer: per-motion `moveWindow` at retained client size with `float` on each motion (survivors refill mid-hold via Tall recalc over the float-excluded set, no slot placeholder), release `float`s the final frame with press-time `shiftMaster`; the traced path contains no target-slot preview, scale, indicator, or drop branch, so none is produced; `S(S-xmo-mouse)` + `S(S-xmo-arrange)` + `S(S-xmo-master)`.
- Then sway: Retained allocation mid-hold: the mover stays attached until finalize (`container_detach` only on the non-swap drop; the swap path re-links in place), so siblings do not reflow; only the indicator rect moves (drop-box positioned/sized per target) with pointer focus cleared at begin; final placement follows the titlebar-tabbed/edge-split/centre-swap/empty-workspace branch for A's edge; `S(S-sway-tdrag)` + `S(S-sway-tdrop)`; native pixels/timing TBD
- Then qtile/Columns: Shipped producer follows the pointer by floating tweak (frame writes on motion, then closest-screen transfer only; no slot preview, scale, or indicator in source); explicit set_position swaps only at invocation with no mid-hold preview either; exact pixels and final placement TBD; `S(S-qti-tweak)` + `S(S-qti-drag)`
- Then awesome/tile: Retained allocation mid-hold: tiled mouse.move never writes the source frame, it only swaps on hover via move_handler with siblings reflowing only on swap via recalc; the traced tiled path carries no target-slot preview, scale, indicator, or placeholder (snap placeholder is floating aerosnap only), so none is produced; the pause changes nothing and the release commits the same hover-swap over A; column leg retains allocation the same way. `S(S-awe-drag)` + `S(S-awe-tile)`
- Then niri: the tile is removed and pinned to the cursor during the
  move (absolute delta keeps it under the pointer); the insert hint
  marks the target slot while moving, and the drop re-inserts at the
  pointer insert position. Whether a pause leaves that placement
  unchanged stays TBD (F: pause rest position and duration
  unrecorded; per-frame edge view-scroll runs during the move and can
  shift the viewport and insert mapping). `S(S-nir-drag)`; queued.
- Then PaperWM: mid-hold the clone tracks the pointer
  (pointer-minus-offset) while `tile-preview` zone actors mark the
  target slot; the source leaves the strip only at `beginDnD`.
  `S(S-pap-grab)` + `S(S-pap-grabzone)`.
- Then karousel/Lazy: the host frame follows mid-hold while karousel
  untiles at session start (shipped default); release is outside the
  pause fixture. `S(S-kar-ptr)`; queued.
- Then paneru: pointer drag is the host macOS journey. `S(S-pan-mouse)`;
  queued.
- Then Ours KDE: native frame moves and target-slot preview is selected
  `D(D-dec-drag)`; offline previews survive native output reassignment in both
  directions, while hidden and foreground reconcile replies write no geometry
  during the hold. Preview lookup/refusal regression evidence:
  [record](../../changes/kde-dual-output-live.md#cross-output-drag-lifecycle-correction-2026-10-14).
  Physical preview rendering and no-jump pause remain TBD.
- Then Ours Windows: stationary source with visible target-slot preview;
  fresh three-Notepad synthetic hold/preview/drop agreement agent-observed
  2026-10-11, base `9d12c7f` plus [press-focus delivery](../../changes/archive/windows-drag-press-focus.md).
  Press-focus changes timing only; native title-bar path stays intact.
  Physical feel/custom-frame checks remain user-owned. `D(D-win-drag)`
- Variant hook: V-DRAG-ZONE.

<a id="r-drag-08-backfill-press-focus-on-an-unfocused-column-scrolling"></a>
### R-DRAG-08: press focus on an unfocused tile

- Given (tree profiles): `H[A*,B]`, B unfocused tiled

- Given (columns): `COL[C1[A*],C2[B]]`, B unfocused; VP recorded.
  Same action: Mod+Left client press on B; move; release at A's edge.

- When: Meta/Win+left client press on B; move; release at A's edge

- Observe: Native focus changes at press vs drop; dragged group visual without changing retained focus

- Observe (column leg): focus changes at press vs drop.

- Then COSMIC: Pointer press changes keyboard focus unless the pointer is grabbed; the Super+Left move path focuses the target at press and the drop focuses the dropped mapped; `S(S-cos-dragpress)` + `S(S-cos-dragframe)`; dragged-group visuals TBD
- Then Hyprland/Dwindle: Press focuses B (`rawWindowFocus` + raise at `dragBegin`), drop focuses the dragged; dispatcher/begin-drag policy only, not a default-binding claim; exact edge position and group visuals TBD; `S(S-hyp-drag)` + `S(S-hyp-dragend)` + `S(S-hyp-keyend)`
- Then bspwm: Press writes no focus (the ACTION_MOVE grab path has no focus write; focus only via the separate ACTION_FOCUS/click path); same-monitor hover swap writes borders only, so B stays unfocused mid-hold and after; exact visuals TBD; `S(S-bsp-drag)`
- Then i3: Modifier-client press does not focus before the drag (tiling drag masks enter-window; end restores focus/fullscreen, with old-focus restore on the DT_PARENT `tree_move` path), so B stays unfocused mid-hold; drop-side focus follows the branch taken, not a promised mover-focus; `S(S-i3-tdrag)` + `S(S-i3-tdrop)`; exact press-vs-drop focus and dragged-group visuals TBD (edge pixel and branch unspecified; titlebar producer would focus first instead)
- Then xmonad/Tall+Navigation2D: Press focuses B (`focus w` runs before `mouseMoveWindow`; `peek`-driven `setTopFocus` actuates it on refresh), mid-hold retains B (no focus write in the per-motion path), press-time `shiftMaster` keeps focus on B (focus stays with the moved item); no dragged-group visual path in source; settled `mod-button1` producer per the fixture gesture; `S(S-xmo-mouse)` + `S(S-xmo-topfocus)` + `S(S-xmo-master)`.
- Then sway: Press focuses B first via the generic click-focus path (titlebar press selects the inactive view; client press focuses the container), then the drag begins with pointer focus cleared; mid-hold retains B's seat focus (no focus write in the motion path); drop-side focus follows the branch taken - non-swap inserts keep seat focus, centre `container_swap` preserves the mover focus on the same workspace via `swap_focus`; diverges from the i3 modifier-no-focus path; `S(S-sway-tdrag)` + `S(S-sway-tdrop)`; exact press-vs-drop focus and dragged-group visuals TBD (edge pixel and branch unspecified)
- Then qtile/Columns: Press focuses B first via follow_mouse_focus=true (shipped; bring_front_click=false, only Mod+Button2 bring_to_front bound); mid-hold retains B; drop-side focus TBD (Columns swap writes no focus; exact branch/edge unspecified); `S(S-qti-tweak)` + `S(S-qti-drag)`
- Then awesome/tile: Press activates B (focus plus raise via permissions.activate; ordinary focusable B passes while the move guard excludes fullscreen/maximized/desktop/splash/dock, both Mod4+client and titlebar producers); mid-hold retains B (no focus write in move_handler or the local swap path); drop-side focus stays B after the swap (swap exchanges positions with no focus write; global re-activation runs only on cross-screen per the global path, and this single-screen fixture stays local); no tag write on the swap path, so no screen-consistency strip/retag or delayed tagged/untagged refocus engages; dragged-group visuals have no counterpart in source; column leg keeps press-vs-drop focus the same way. `S(S-awe-drag)` + `S(S-awe-swap)`
- Then niri: Mod+Left press activates B before the grab, and the drop
  re-inserts with activation so the mover is active after the drop.
  The moving tile renders with an alpha dip while moving; no
  dragged-group visual in the path.
  `S(S-nir-drag)` + `S(S-nir-ptr)`.
- Then PaperWM: grab begin does not focus; the drop end activates;
  press-focus journey TBD (GNOME default untraced at pin).
  `S(S-pap-grab)`; queued.
- Then karousel/Lazy: press focus is the host KWin journey (no
  focus policy in the profile source). Queued.
- Then paneru: FFM focuses B on pointer entry before the press;
  press delivery and drop-side focus TBD. `S(S-pan-mouse)`; queued.
- Then Ours KDE: exact focus timing TBD
- Then Ours Windows: press-focus delivered `9bb2b6f` and agent-observed 2026-10-11
  ([delivery record](../../changes/archive/windows-drag-press-focus.md)); offline
  revalidated 2026-10-17 at `5d953e98`, native gates passed (2,448 passed,
  0 failed, 1 ignored), no new live run:
  unfocused B becomes exact foreground before movement/Up; no-move and Esc
  retain focus with stable frames and no drop plan. Moving drop keeps source
  and siblings stationary during preview, lands on the preview allocation,
  and keeps B foreground. Actual press setter accepted, 6-10ms observed.
  Drop still verifies fresh focus and refuses failures; already-foreground
  skips redundant setter. Native title-bar and inactive resize paths unchanged,
  underlay C parked. Physical Start-mask/input/resize feel remain user-owned;
  positive refusal-race native probes not run. `D(D-win-drag)`
- Variant hook: V-DRAG-ZONE.

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours cells cite Engine + adapter source at `9241c94`
plus established `S(S-ours-*)` keys and the new `S(S-ours-mou)` pointer
paths; selected intent and doc assertions are never evidence.

### R-MOU-01: pointer hover vs click focus

- Given (tree profiles): `H[A*,B]`, pointer on A. Hover leg names its
  exact variant per profile (shipped ffm default, explicit
  enabled/disabled override, or no-counterpart); click leg runs from a
  fresh click-focus fixture. Ordinary windows, no rules.
- Given (column profiles): `COL[C1[A*],C2[B]]`, pointer on C1; same
  two legs; shipped defaults apply; viewport recorded.
- When: move the pointer into B (hover leg); plain left-click B
  without modifiers (click leg, never a mod-drag).
- Observe: hover vs click focus, delayed/sloppy policy, and click
  delivery vs focus-only consumption. Modifier-drag press focus stays
  R-DRAG-08.
- Then COSMIC: click presses focus B and deliver to the client unless
  Super-held (compositor consumes) or on the resize fork (focus kept);
  hover at the shipped default (`focus_follows_cursor=false`) retains A
  with no scheduled focus; the explicit enabled variant schedules B after
  the configured delay (default 250ms) via the pointer-focus state.
  `S(S-cos-dragpress)` + `S(S-cos-hoverfocus)`.
- Then Hyprland/Dwindle: hover focuses B under shipped
  `input:follow_mouse=1` (FFM reason); plain press refocuses B with
  raise (CLICK reason, unless `follow_mouse=3`). `S(S-hyp-follow)` +
  `S(S-hyp-drag)`.
- Then bspwm: plain button1 click focuses B via ACTION_FOCUS and replays to the client (shipped `click_to_focus=button1` with `swallow_first_click=false`); hover at the shipped default retains A (no enter/motion focus subscription while `focus_follows_pointer=false`); the explicit enabled variant focuses on pointer motion via the motion path with no scheduled delay. `S(S-bsp-ptrfocus)`.
- Then i3: hover focuses B under shipped enabled
  focus-follows-mouse; plain click focuses via `con_activate` in the
  click path. `S(S-i3-ffm)` + `S(S-i3-click)`.
- Then xmonad/Tall+Navigation2D: hover focuses B on entry under
  shipped `focusFollowsMouse=True`; plain click focuses B under
  shipped `clickJustFocuses=True`. `S(S-xmo-ffm)`.
- Then sway: hover focuses B under shipped `focus_follows_mouse=yes`;
  plain click focuses the clicked container. `S(S-sway-ffm)`.
- Then qtile/Columns: hover focuses B via EnterNotify under shipped
  `follow_mouse_focus=True`; plain click focuses via `focus_by_click`
  without raising under shipped `bring_front_click=False`.
  `S(S-qti-click)`.
- Then awesome/tile: hover focuses B without raising via shipped
  `mouse::enter` sloppy activate; plain button1 click activates
  focus plus raise. `S(S-awe-sloppy)`.
- Then niri: shipped hover-focus is off; the named
  `focus-follows-mouse max-scroll-amount="0%"` variant focuses fully
  visible B without raising. Plain click activates B and forwards the
  button event. `S(S-nir-ffm)`.
- Then PaperWM: CENTER/EDGE are viewport modes, not pointer-focus
  policy; host hover/click producers and delivery remain TBD.
  `S(S-pap-focusmode)`; queued.
- Then karousel/Lazy: hover/click focus is the host KWin journey
  (no focus policy in the profile source). Queued.
- Then paneru: hover focuses B under shipped enabled
  `focus_follows_mouse`; click marks held and
  reshuffles on release while host macOS click focuses, journey TBD.
  `S(S-pan-base)` + `S(S-pan-mouse)`; click-focus queued.
- Then Ours KDE: native host click/hover focus journey untraced
  (adapter verbs are inventory only, not the host producer path).
  `S(S-ours-mou)`; queued.
- Then Ours Windows: same host-producer journey TBD as Ours KDE.
  `S(S-ours-mou)`; queued.
- Variant hook: provisional/TBD (pointer-focus hook, to discuss).

### R-MOU-02: drag the shared edge to resize

- Given (tree profiles): `H[A*,B]` 50/50, fixed work area. Ordinary
  windows, no rules, scale 1.
- Given (column profiles): two 0.5W single-window columns at shipped
  defaults; viewport recorded. Bare edge drag first; profile-required
  modifier grabs named per profile, never silently substituted.
- When: drag the shared edge right 100px with the profile's
  applicable producer (named per Then; bare edge first, modifier
  grabs only where the profile requires one); release.
- Observe: ratio/share change vs frame-only/no resize, sibling clamp
  and ratio retention. Float drag/resize stays R-FLT-14.
- Then COSMIC: bare-edge press reaches the tiling resize fork (gap
  handle keeps keyboard focus) and motion adjusts the nearest
  matching-edge-axis ancestor pair by the rounded pointer delta with the
  360px vertical (240px horizontal) pair minima; the exact 100px share
  outcome stays TBD (F: fixed work-area/output geometry and shared-edge
  press pixel unrecorded, so the 50/50 pixel base is unspecified).
  `S(S-cos-resize)`; share queued.
- Then Hyprland/Dwindle: bare-edge drag starts no resize under
  shipped `resize_on_border=false` (falls to the click-focus path);
  enabled variant begins an MBIND_RESIZE drag whose tiled motion
  dispatches pixel deltas to the shared split (float-only min/max
  clamp); tiled share outcome TBD (F: work-area/parent-box dimensions
  unrecorded, so the 100px-to-ratio scale is unspecified).
  `S(S-hyp-edgeresize)` + `S(S-hyp-resize)`; queued.
- Then bspwm: `pointer_modifier=Mod4` resize_side/corner grab adjusts the fence
  split_ratio by dx/fence-width clamped to [0,1] with reflow; 100px
  moves the shared fence by 100/fence-width. `S(S-bsp-ptrresize)`.
- Then i3: border drag moves percent shares between the pair with a
  1px minimum clamp; 100px grows A and shrinks B by that amount.
  `S(S-i3-border)`.
- Then xmonad/Tall+Navigation2D: bare edge drag starts nothing
  (inspected bindings list only mod-button1 move and mod-button3
  resize); mod-button3 leg floats B instead of resizing shares, so
  tile shares never change on either producer. `S(S-xmo-mouse)`.
- Then sway: border BTN_LEFT press begins the tiling edge resize;
  motion resizes the pair's width fractions with a sane-minimum
  clamp. `S(S-sway-rszedge)`.
- Then qtile/Columns: no-counterpart (shipped Drag binds are
  floating-only; the tiled mod-drag swaps instead of resizing, so no
  pointer verb changes tile shares). `S(S-qti-drag)`.
- Then awesome/tile: edge drag reaches the tile layout's
  mouse_resize_handler, which moves master_width_factor to the
  pointer x; master and stack reflow. `S(S-awe-tresize)`.
- Then niri: Mod+Right and valid client edge-resize requests open an
  interactive resize that writes the dragged column width (`SetFixed`
  from the pointer delta; neighbors keep their widths); the exact
  100px share outcome stays TBD (F: fixed work-area width unrecorded,
  so the 0.5W pixel base is unspecified).
  `S(S-nir-ptr)` + `S(S-nir-clientgrab)`; queued.
- Then PaperWM: `RESIZING_*` builds a marker `ResizeGrab` whose
  `end` is a no-op; `resizeHandler` ignores the grabbed window
  mid-grab and re-tiles from the new frame after, with per-column
  widths from live frames. Whether the edge press starts a native
  resize at all, and its amounts, ride the host. Share outcome TBD
  (host Mutter resize journey). `S(S-pap-grab)` +
  `S(S-pap-grabzone)` + `S(S-pap-layout)`; queued.
- Then karousel/Lazy: edge drag writes the dragged column width via
  onUserResizeWidth while the neighbor keeps its width under shipped
  `resizeNeighborColumn=false`. `S(S-kar-ptr)`.
- Then paneru: modifier-hold move resizes the grabbed window width by
  5x the pointer delta with a frame-only write and no sibling-share
  step, so the sibling keeps its width; conditional on the grab (if A
  grabbed then A gains 500px for the 100px drag with B unchanged, and
  vice versa); no clamp step in the traced path. `S(S-pan-mouse)`.
- Then Ours KDE: pointer-resize verb exists in the adapter inventory
  but proves no share outcome; host journey TBD. `S(S-ours-mou)`;
  queued.
- Then Ours Windows: same inventory-only verb, host journey TBD.
  `S(S-ours-mou)`; queued.
- Variant hook: provisional/TBD (edge-resize hook, to discuss).

### R-MOU-03: drag across outputs and onto a workspace target

- Given (tree profiles): `L:WS1=H[A,B*]`, `R:WS2=H[C]`, hidden WS3
  exists. Ordinary windows, no rules.
- Given (column profiles): per-output strips at shipped defaults;
  karousel two-output fixture inapplicable (single-screen scope).
- When: drag B to C's edge on R; release. Fresh fixture: drag B onto
  the WS3 switcher/overview target.
- Observe: cross-output insertion/follow vs float/cancel;
  hidden-workspace hover-switch/drop vs unavailable target. Two
  explicit targets, not a generic off-area drop (R-DRAG-06).
- Then COSMIC: drop lands in the cursor output's active space and focuses the
  dropped window (tiling grabs open overview mode); the exact C-edge
  insert position and the WS3 switcher/overview-target journey stay TBD
  (F: C-edge hover pixel and switcher-target geometry/drop point
  unrecorded, so which `drop_window` zone branch runs is unspecified).
  `S(S-cos-dragframe)` + `S(S-cos-drop)`; switcher queued.
- Then Hyprland/Dwindle: tiled-origin drag floats at pick-up, a
  middle crossing reassigns to that monitor's active workspace, and
  the drop re-tiles via fresh admission with mover focus; exact
  C-edge insert position TBD (F: hover pixel and drop point unrecorded).
  The WS3 switcher leg has no counterpart in the traced drop path
  (decoration/group/re-tile only). `S(S-hyp-drag)` +
  `S(S-hyp-dragend)`; queued.
- Then bspwm: cross-output leg transfers with no float/cancel branch in the tiled path (same-monitor hover swaps; any R hover falls to the monitor-point branch), so B transfers to R's shown desk at its focus with follow focusing B (sole C anchors the insert); the hidden-WS3 switcher/overview leg has no counterpart (no overview/switcher drop target in source; acquisition is managed-window hover or monitor-point only). `S(S-bsp-drag)` + `S(S-bsp-switcher)`.
- Then i3: cross-output leg inserts B beside C on R through the same
  rect-hit DT mechanics (the walk spans all outputs) and, B focused,
  shows the new workspace with mover focus; hidden-WS3 switcher leg
  has no counterpart (drop targets are visible-workspace tiles only).
  `S(S-i3-tdrop)`.
- Then xmonad/Tall+Navigation2D: cross-output drop journey untraced
  (pointer path writes raw frames plus shiftMaster; no drop-branch
  model in this profile). `S(S-xmo-mouse)`; queued.
- Then sway: tiled B uses the tiled producer only (modifier+left anywhere
  incl client, titlebar left without modifier; floating uses the separate
  floating-move producer): press focuses first, modifier begins immediately
  while titlebar waits the output-scaled 9px threshold; the source stays
  attached with indicator-only preview. At any found target the same
  finalize branches run (titlebar tabbed split+indexed insert, 30px/30%
  edge split+insert, centre `container_swap`, empty-workspace add;
  layer-surface NULL aborts; no float/cancel branch, no key-press revert).
  Cross-output targets resolve through the same cursor-coords lookup
  spanning all outputs with no output clamp in the traced targeting path;
  the exact C-edge branch stays TBD (F: C-edge hover pixel and drop point
  unrecorded). The WS3 switcher/overview leg has no counterpart (no
  switcher drop target in source; the seatop handles button/motion only).
  `S(S-sway-tdrop)` + `S(S-sway-switcher)`; edge queued.
- Then qtile/Columns: tiled cross-output outcome TBD (tiled mod-drag
  swaps with the hovered window while only the float branch carries
  across screens). `S(S-qti-drag)`; queued.
- Then awesome/tile: tiled move follows the screen under the pointer (`move_handler` sets screen on screen change, then swaps with the hovered tiled `current_client`; no focus write in the swap path, press-activated B retained); insertion is hover-swap, not index insert; exact hovered index TBD (F: pointer unrecorded). WS3 switcher/overview leg has no counterpart (no overview/switcher drop target in the shipped key/mouse inventory; mod+Tab is history-previous only).
  `S(S-awe-drag)` + `S(S-awe-keys)`; hover queued (F: pointer unrecorded).
- Then niri: the moving tile tracks output changes with output focus
  and drops at the pointer insert position on R with activation
  (insertion with follow, no float/cancel). The hidden-WS3
  switcher/overview drop stays TBD (F: no hover-switch in the move
  path - the target resolves only at drop via the output's rendered
  workspaces or a newly created slot; reaching hidden WS3 needs an
  overview toggle mid-grab whose step, WS3 monitor membership and
  target geometry are unrecorded). `S(S-nir-drag)`; queued.
- Then PaperWM: DnD zones span all spaces and entering R begins DnD;
  the drop inserts at the R zone and activates; WS3 switcher leg has
  no counterpart (minimaps hide during DnD). `S(S-pap-grab)`.
- Then karousel/Lazy: cross-output leg fixture-inapplicable
  (single-screen scope, `S(S-kar-single)`); a switcher-target drop holds
  no grid-insertion branch in script (B already floated at session start
  under shipped `untileOnDrag=true`; TabBox accept only activates per the
  host path), so B stays floating with no column insert; no pointer-drop
  commit path exists in the host TabBox (outside press closes/aborts,
  accept only activates via the keyboard paths), so no switcher gesture
  commits a column drop (no counterpart for that target).
  `S(S-kar-single)` + `S(S-kar-ptr)` + `S(S-kwin-tabbox)`.
- Then paneru: pointer drag is the host macOS journey (no engine
  drag model; MouseDragged forwards to Lua only); display verbs are
  keyboard-only. `S(S-pan-mouse)`; queued.
- Then Ours KDE: destination preview and drop routing with native mover
  reassignment are proven offline, retaining the Started source binding;
  source/destination reconcile actuation is held until release, with post-hold
  recovery. Ambiguous/absent mover evidence refuses, repeated identical preview
  refusal/backoff logs are state-change-only
  ([record](../../changes/kde-dual-output-live.md#cross-output-drag-lifecycle-correction-2026-10-14)).
  Physical occupied/empty insertion, preview/drop agreement and hidden workspace
  target remain TBD. `S(S-ours-mou)` inventory alone is not outcome evidence.
- Then Ours Windows: same inventory-only verb, host journey TBD.
  `S(S-ours-mou)`; queued.
- Variant hook: provisional/TBD (cross-output-drop hook, to discuss).
