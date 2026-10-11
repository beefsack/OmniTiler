# Maximise / fullscreen (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

## 5. Maximise / fullscreen

<a id="scrolling-backfill-additive-existing-wide-tables-above-unchanged"></a>
<a id="r-max-01-backfill-maximize-then-restore-scrolling"></a>
### R-MAX-01: maximize then restore

- Given (tree profiles): `H[A,B*,C,D]` equal shares, effective width 2544px, gap 8; min widths 401/864/627/582 constrain actual allocation

- Given (column profiles): `COL[C1[A],C2[B*],C3[C],C4[D]]`, effective
  viewport width 2544px, gap 8, minimum widths 401/864/627/582 as in the
  original row. Column widths follow the shipped presets/rules, not
  equal tree shares; record actual pre-action allocations and viewport.
  Maximize B natively, then restore B. This is a lifecycle projection,
  not an exact H-tree allocation or a width-preset substitute.

- When: Maximize B, then restore B

- Observe: Sibling desired/actual stability, retained hints, exact slot, convergence delay

- Observe (column leg): sibling desired/actual stability, retained hints, exact slot
  and convergence delay; distinguish sourced membership policy from the
  original fixture's load-bearing settled geometry.

- Then COSMIC: Maximize records original geometry+layer and overlays the work area; B stays in the tiling tree so siblings keep allocation with no reflow; restore dispatches by layer and restores original geometry/layer, revealing the retained slot; project focus stays B (no focus write either leg); Super+M distinct from F11; `S(S-cos-maxtoggle)` + `S(S-cos-maxpolicy)` + `S(S-cos-bornmax)` + `D(D-cosmic-kb)`; exact native frames, client-ack visuals and timing TBD (L)
- Then Hyprland/Dwindle: Flat 4-child start has no ordinary binary form; exact N-ary frames TBD. Policy: internal `FSMODE_MAXIMIZED` covers the work area (siblings stay in tree but obscured/blocked); restore to `NONE` lets recalc reveal retained slots; exact frames/focus TBD; `S(S-hyp-fs)`
- Then bspwm: Unsupported action parameter here: no maximize command/state in source (monocle is a desktop layout, maximize flags are not tree state), so maximize/restore never run (no built-in equivalent); `S(S-bsp-layout)` + `S(S-bsp-admit)`
- Then i3: Unsupported action parameter here: no maximize command/state in source (maximize is a derived client hint only), so maximize/restore never run (no built-in equivalent); `S(S-i3-max)`
- Then xmonad/Tall+Navigation2D: Unsupported action parameter here: no maximize command/state in source (`Full` is a workspace focused-fullscreen layout, not per-window maximize, so maximize/restore never run); `S(S-xmo-layout)`
- Then sway: Unsupported action parameter here: no maximize command/state in source (inventory has no maximize verb; client maximize request only schedules a configure), so maximize/restore never run (no built-in equivalent). `S(S-sway-max)`
- Then qtile/Columns: Maximized is a floating-layer state at work-area size: B is removed from the tiling via mark_floating (survivors refill with no retained-slot overlay) and restore re-adds via fresh admission at the focused cc position with saved geometry/state; focus stays B on both legs; exact maximized/restored pixel frames TBD (F: work-area/output geometry plus gap/column projection unrecorded; L: convergence/client-ack timing); `S(S-qti-fs)` + `S(S-qti-fsslot)`
- Then awesome/tile: Maximized leaves the tiling (excluded like floats; survivors refill via stateless recalc, no retained-slot overlay); restore re-includes B at its retained global order position via recalc; the boolean flip is synchronous with raise and no attempted-state fence, so focus stays B and placement maximize/restore covers/restores the work-area frame; `S(S-awe-fs)` + `S(S-awe-tile)`
- Then niri: client maximize routes into `set_maximized(true)`, setting
  the column pending-maximized flag with the strip kept (single-tile
  fixture columns need no extract; siblings keep independent widths;
  configure offers the working-area size); restore routes into
  `set_maximized(false)`, an idempotent clear that floats only a
  previously-floating window. Exact hinted frames and convergence timing
  TBD (live-only). `S(S-nir-maxfs)`; geometry queued (live).
- Then PaperWM: native maximize converts to full-width maximize at the
  shipped default (`maximize-within-tiling` true: unmaximize, restore the
  last layout frame, then width toggle to 1.00 of the work area with
  `unmaximizedRect` memory); B stays tiled and wide while siblings keep
  their widths with viewport scroll. Native restore has no maximized flag
  left to clear; only the width toggle restores the saved width. Exact
  hinted frames follow the width arithmetic; native convergence timing TBD
  (live-only). `S(S-pap-widthmax)` + `S(S-pap-layout)`; timing queued.
- Then karousel/Lazy: native maximize keeps B's column membership with
  `skipArrange` set (siblings keep their slots and widths, B is never arranged;
  layering follows the shipped `tiledKeepBelow` default with no keepAbove write);
  restore clears the flag through the same change handler, revealing the retained
  slot; the maximized frame covers the work area. Convergence timing TBD (client ack at runtime). `S(S-kar-maxfs)`; timing TBD (live-only).
- Then paneru: no-counterpart for a paneru-native `B:max` leg (no verb in
  `Operation`, no zoom/maximize AX read); a host-zoomed window is an
  owner-specific journey with sibling/restore behavior TBD.
  `S(S-pan-cmds)` + `S(S-pan-model)` + `S(S-pan-axfs)`; journey queued.
- Then Ours KDE: KDE: slot/share kept, no writes, exact restore `D(D-dec-ww)`; Windows: same + retained hints + bounded async restore; synthetic proof `D(D-max)` + `D(D-place)`; physical feel pending
- Then Ours Windows: KDE: slot/share kept, no writes, exact restore `D(D-dec-ww)`; Windows: same + retained hints + bounded async restore; synthetic proof `D(D-max)` + `D(D-place)`; physical feel pending
- Variant hook: V-MAX-MODEL.

<a id="r-max-02-backfill-fullscreen-focus-and-exit-scrolling"></a>
### R-MAX-02: fullscreen focus and exit

- Given (tree profiles): `H[A,B*]`

- Given (column profiles): `COL[C1[A],C2[B*]]` at shipped defaults. Fullscreen
  B; focus A; focus B; exit fullscreen.

- When: Fullscreen B; focus A; focus B; exit fullscreen

- Observe: Tree mutation; focus enter/leave; restore

- Observe (column leg): strip mutation; focus enter/leave; restore.

- Then COSMIC: Fullscreen removes B's node with no placeholder (A reflows to full width by proportional rescale) but saves sibling/idx/sizes; focus moves to a separate Fullscreen target. Focus A/B changes focus while the overlay remains until explicit exit. Exit re-inserts B beside A at the saved idx with saved sizes and returns the restored window as focus; separate focus surface; `S(S-cos-fsreq)` + `S(S-cos-fsrestore)` + `S(S-cos-restore)` + `S(S-cos-fsact)` + `S(S-cos-rem)` + `D(D-ref)`; intermediate visible-focus and native sequence TBD
- Then Hyprland/Dwindle: `FSMODE_FULLSCREEN` covers the monitor box (tree kept, siblings blocked, no node removal); focus left is fenced under shipped defaults (`movefocus_cycles_fullscreen=false`: the directional query skips non-allowed tiled candidates while the non-layout-managed covering fullscreen exists, and the full-size stay keeps B since it covers the monitor), so focus stays B; focus B is moot; exit to `NONE` leaves placement to recalc restoring tile boxes with focus staying B (no focus write on the toggle path); `S(S-hyp-fs)` + `S(S-hyp-focus)`
- Then bspwm: Fullscreen covers the monitor with the node kept vacant in place (tree kept, `last_state` remembered, no maximize state); entry writes no focus so focus stays B; focus A clears the covered fullscreen to `last_state` via `neutralize_occluding_windows` with arrange, focus B is ordinary, explicit exit restores `last_state` (moot if focus already cleared); `D(D-ref)` + `S(S-bsp-state)` + `S(S-bsp-fs)`
- Then i3: Fullscreen is a mode flag: tree retained, B overlays via render; enable focuses B; same-workspace directional focus outside B is fenced (drops to workspace level, then seeks a next-output workspace, NULL on a single output, so focus stays B), while criteria `focus` on A unblocks instead (disables the covering fullscreen and focuses A); the `focus A` verb is unstated, so the exact A/B focus sequence stays TBD (F: source-conditional on the focus verb, exit sequence with it); exit on the still-fullscreen path clears the mode and recalc reveals tiles. `S(S-i3-fs)`; queued (focus leg).
- Then xmonad/Tall+Navigation2D: EWMH fullscreen in profile is float-based (post-map `ClientMessage` add/remove/toggle via `fullscreenEventHook` with default `doFullFloat` fullscreen `RationalRect 0 0 1 1` / `doSink`; `_NET_WM_STATE` property added/removed via `chWstate`; stack retained); core `Full` is workspace focused-fullscreen instead. The fullscreen leg floats B out of the `S(S-xmo-arrange)` tiled input so A refills via Tall recalc, floats restack first-on-top, `tileWindow` applies allocations, and `setTopFocus` actuates `peek`; exit `doSink` re-tiles B via the same recalc. Exact A/B focus enter/leave sequence TBD (F: fixture states no focus verb; core stack `focusUp`/`focusDown` vs contrib Navigation2D `windowGo` same-layer path is the discriminating fork, never defaulted). `S(S-xmo-ewmh)` + `S(S-xmo-layout)` + `S(S-xmo-arrange)` + `S(S-xmo-topfocus)`; queued.
- Then sway: Fullscreen is a mode flag: tree retained, `ws->fullscreen` set, enable focuses B on its workspace; directional focus A from workspace-fullscreen drops to outputs (NULL with no adjacent output, so fenced with focus staying B); focus B is moot; exit clears the mode and recalc reveals tiles with focus staying B. `S(S-sway-full)` + `S(S-sway-focus)`
- Then qtile/Columns: Fullscreen is a floating-layer state that keeps its tiled slot (mark_floating skips layout removal while fullscreen, so no refill and no placeholder); focus A/B run the ordinary tiled-column walk and exit restores the saved float state in place; `S(S-qti-fs)` + `S(S-qti-fsslot)`
- Then awesome/tile: Fullscreen leaves the tiling (sole tiled A refills the work area; list order kept, no placeholder); the focus-A step misses (B covers the workarea from the same wa.x origin as A's master allocation, so no candidate is strictly left and B is retained) and focus-B is moot, so focus stays B throughout (no focus write on either toggle and no refocus hook on the fullscreen path); exit clears the boolean and rejoins B at its retained list position via recalc; `S(S-awe-fs)` + `S(S-awe-tile)` + `S(S-awe-focus)` + `S(S-awe-geodir)`
- Then niri: fullscreen sets the column pending-fullscreen flag with the
  strip kept; focus left/right are plain column-index activations either
  way; exit clears the flag, floating only a previously-floating window.
  `S(S-nir-maxfs)` + `S(S-nir-focus)`.
- Then PaperWM: native fullscreen is honored with the tiled frame saved
  (layout and position updates skip it); `switch` focus is model-based
  so A and B are reachable both ways; exit restores the saved frame.
  `S(S-pap-fsframe)` + `S(S-pap-unmov)` + `S(S-pap-focus)`.
- Then karousel/Lazy: fullscreen keeps membership with `skipArrange`
  set; focusing A restores B to tiled via `restoreToTiled`, so the
  focus-B step finds a normal window and the explicit exit is moot.
  `S(S-kar-maxfs)` + `S(S-kar-focus)`.
- Then paneru: native fullscreen removes B from the original strip with
  no reserved column and pins a `Fullscren` strip carrying the restore
  marker (original strip plus index). West focus on that space raises the
  original strip's last column top (A); East focus from off-strip A enters
  the   fullscreen strip's first top (B). The host destroys the native space on
  exit and `SpaceDestroyed` reinserts B at the marker index with a reshuffle, despawning the
  fullscreen strip; the exit issues no focus write, so focus stays B.
  `S(S-pan-model)` + `S(S-pan-fsfocus)`.
- Then Ours KDE: Retained slot overlay; focus may enter/leave; `D(D-dec-ww)` KDE + `D(D-fs)` Windows scoped proof; physical focus sequence pending
- Then Ours Windows: Retained slot overlay; focus may enter/leave; `D(D-dec-ww)` KDE + `D(D-fs)` Windows scoped proof; physical focus sequence pending
- Variant hook: V-FS-SLOT.

<a id="r-max-03-backfill-workspace-floating-toggle-over-a-slotless-maximum-scrolling"></a>
### R-MAX-03: workspace floating toggle over a slotless maximum

- Given (tree profiles): Workspace floating, first-seen maximized A without a prior tile slot

- Given (column profiles): attempted mapping needs a workspace-wide floating
  mode plus a slotless maximized window. No scrolling profile has a
  workspace-wide floating mode, so the target parameter has no faithful
  start; the Thens below are applicability qualifications, never an
  ordinary toggle with a substituted target.

- When: Toggle tiled; restore A if still maximized

- Observe: Maximum across toggle, reserved tile slot and sibling allocation, then native restore and actual tiled write/readback

- Observe (column leg): whether the workspace floating target exists natively.

- Then COSMIC: Enable tiles every floater sequentially at focus-MRU long-edge anchors (A included, no slotless hold); maximized floaters re-overlay with original layer retargeted to Tiling; restore A reveals the retained fresh slot; `S(S-cos-wstile)` + `S(S-cos-last)` + `S(S-cos-axis)`; exact native frames/journey TBD
- Then Hyprland/Dwindle: Unsupported action parameter here: no workspace floating/tiled toggle in source (per-window float dispatch; workspace rules carry no tiling/floating default) and no slotless-maximum hold, so the toggle/restore legs never run (no built-in equivalent); `S(S-hyp-float)` + `S(S-hyp-wsrule)`
- Then bspwm: Unsupported action parameter here: no workspace tiling toggle and no slotless-maximum hold in source, so toggle/restore never run (no built-in equivalent); `S(S-bsp-layout)` + `S(S-bsp-admit)`
- Then i3: Unsupported action parameter here: no workspace tiling toggle and no maximize hold state in source (maximize derived-only), so the toggle/restore legs never run (no built-in equivalent); `S(S-i3-wsmode)` + `S(S-i3-max)`
- Then xmonad/Tall+Navigation2D: Unsupported action parameter here: no workspace tiling toggle and no slotless-maximum hold in source (layout always tiles plus floating layer; maximize is not tree state, so toggle/restore never run); `S(S-xmo-layout)`
- Then sway: Unsupported action parameter here: no workspace tiling toggle and no maximize hold state in source (no maximize verb; maximize request only schedules a configure), so the toggle/restore legs never run (no built-in equivalent). `S(S-sway-wsmode)` + `S(S-sway-max)`
- Then qtile/Columns: Unsupported action parameter here: no workspace floating/tiled toggle and no slotless-maximum hold in source, so the toggle/restore legs never run (no built-in equivalent); ordinary fullscreen-flagged admission applies auto_fullscreen with the slot kept; `S(S-qti-wstoggle)` + `S(S-qti-fsslot)`
- Then awesome/tile: Workspace floating reads as the shipped floating layout (per-tag); slotless maximized A stays maximized-and-implicitly-floating across the switch with geometry kept (floating arrange is a no-op; tile arrange excludes it); restore-then-unmaximize clears the boolean and rejoins A tiled at its retained global order position via recalc (absent explicit floating, which would keep A excluded); neither leg writes focus, so focus is retained; `S(S-awe-layout)` + `S(S-awe-fs)` + `S(S-awe-float)`
- Then niri: fixture-inapplicable (no workspace floating mode exists;
  `ToggleWindowFloating` is per-window only and `floating_is_active`
  derives from admission/focus, not a mode). `S(S-nir-float)`.
- Then PaperWM: fixture-inapplicable (no floating workspace mode and no
  workspace toggle in the registered action inventory).
  `S(S-pap-acts)`.
- Then karousel/Lazy: fixture-inapplicable (float is per-window only; no
  floating desktop mode). `S(S-kar-acts)`.
- Then paneru: fixture-inapplicable (no floating workspace mode; window
  management is per-window). `S(S-pan-cmds)`.
- Then Ours KDE: Selected Q3 scope, delivered offline: floating skips slot seeding and preserves native maximize; toggling tiled reserves non-fixed A's tile slot without clearing or toggling maximize. Siblings receive their allocated shares; later native unmaximize lands A in its reserved slot. D6 exception: fixed automatic clients remain slotless floats on workspace enable, including under maximize; explicit user tile overrides retain Q3 slots ([R-SPC-12](special-windows.md#r-spc-12-enable-workspace-tiling-with-an-automatic-fixed-float)). Existing `fit_excluded` / `skip-maximized` overlay isolation keeps repeated maximize signals and synchronous geometry-write signals from fighting native state. `clearMaximizeAtAdmission` clears only non-fixed held born-fullscreen exits (D5 fixed exits untouched); R-MAX-03 and born-maximized R-MAX-06 share the overlay path ([adapter](../../../kwin/src/plan-adapter.ts), real Engine [fixtures](../../../kwin/tests/plan-adapter.test.ts), [record](../../changes/archive/kde-maximized-floating-retile-overlay.md)); [Q3 scope decision](../../decisions.md#window-state-float-sticky-maximize-fullscreen). Native toggle/restore and session-restore no-loop acceptance remain user-owned, TBD.
- Then Ours Windows: slotless membership preserves floating maximum and hide/reveal, then one clear and fresh tiled plan/native write/matched target readback proven with Notepad/Paint; slotted overlays skip re-clear [accepted correction](../../changes/archive/windows-workspace-tiling.md#r-max-03-accepted-correction); physical feel TBD
- Variant hook: V-WS-TILING plus V-MAX-MODEL - selected Q3 reserved-slot overlay includes floating-to-tiled admission; KDE delivered offline, live TBD; Windows parity (b) pending.

<a id="r-max-04-backfill-shortcut-maximize-native-restore-repress-scrolling"></a>
### R-MAX-04: shortcut maximize, native restore, repress

- Given (tree profiles): `H[A,B*]`; B normal and remains the same native window

- Given (column profiles): `COL[C1[A],C2[B*]]` at shipped defaults; B normal and
  remains the same native window. Shortcut-maximize B; native-restore B;
  press the same shortcut again. Use niri `MaximizeWindowToEdges`, KWin's
  native maximize shortcut for karousel, and a GNOME native maximize
  request for PaperWM (Meta `maximize(BOTH)`, then `unmaximize(BOTH)`).
  PaperWM's width-only shortcut is not substituted for this host action.

- When: Shortcut-maximize B; native-restore B; press the same shortcut again

- Observe: New maximize attempt vs persistent attempted-state refusal

- Observe (column leg): new maximize attempt vs persistent attempted-state refusal.

- Then COSMIC: Native client restore routes to compositor unmaximize_request on all three protocol paths (Wayland, X11, toplevel-management), taking and clearing maximized_state; the repress then sees un-maximized and issues a new maximize_request with re-recorded overlay; `S(S-cos-native-unmax)` + `S(S-cos-maxtoggle)`; native ack/focus visuals TBD
- Then Hyprland/Dwindle: New maximize attempt: client native restore routes to `setFullscreenMode` `NONE` (taking/clearing maximized state), so the repress sees un-maximized and issues a new maximize with re-recorded overlay; native ack/focus visuals TBD; `S(S-hyp-fs)`
- Then bspwm: No state-change path: no maximize command/state for a shortcut press or native restore to act on, so tree state is a no-op; client ack TBD; `S(S-bsp-layout)` + `S(S-bsp-admit)`
- Then i3: No state change path: no maximize command to press and no maximize state for native restore to clear (maximize hints are output-only); outcome is a no-op; `S(S-i3-max)` + `S(S-i3-fs)`
- Then xmonad/Tall+Navigation2D: No state-change path: no maximize command/state for a shortcut press or native restore to act on, so tree state is a no-op; `S(S-xmo-layout)`
- Then sway: No state change path: no maximize command to press and no maximize state for native restore to clear (maximize request only schedules a configure); outcome is a no-op. `S(S-sway-max)`
- Then qtile/Columns: Toggles flip state with no attempted-state fence in source. Backend-qualified: on X11 the native restore is echoed only and never drives maximized, so MAXIMIZED is retained and the repress toggles off (unmaximize); on Wayland the native `handle_request_maximize` drives the state, so native restore clears and the repress re-applies maximize; native ack/focus visuals TBD; `S(S-qti-fs)`
- Then awesome/tile: New maximize attempt: toggles are plain boolean flips with raise and no attempted-state fence, so native restore clears and the repress re-applies; native ack/focus visuals TBD; `S(S-awe-fs)`
- Then niri: `MaximizeWindowToEdges` drives the native toggle (flips the
  column pending flag, no fence); client native restore routes into
  `set_maximized(false)`, an idempotent clear; the repress re-applies:
  a new attempt every press. `S(S-nir-maxfs)`.
- Then PaperWM: each native maximize request converts to a width toggle;
  native restore has no maximized flag to clear. The repress is processed
  again (and can toggle the saved width back), with no attempted-state
  fence. `S(S-pap-widthmax)`.
- Then karousel/Lazy: native KWin maximize on the tiled window is
  observed with `skipArrange` set and no fence in the change handler;
  native restore clears through the same handler; the repress
  re-maximizes: a new attempt every press. `S(S-kar-maxfs)`.
- Then paneru: no-counterpart for a paneru-native maximize leg (no verb in
  `Operation`, no zoom/maximize AX read); a host-zoom journey is
  owner-specific with attempt behavior TBD.
  `S(S-pan-cmds)` + `S(S-pan-axfs)`; journey queued.
- Then Ours KDE: repaired 2026-10-05: same-ref adapter regression issues a new native attempt after restore (and reverse ordering), `D(D-kde-follow)`; earlier refusal remains historical `S(S-ours-toggle)`; physical repeat/delivery outcome TBD
- Then Ours Windows: dispatches one attempt per new discrete down, `D(D-dec-max)`; physical repeat/delivery outcome TBD
- Variant hook: V-MAX-MODEL.

<a id="r-max-05-backfill-app-owned-fullscreen-without-a-preimage-scrolling"></a>
### R-MAX-05: app-owned fullscreen without a preimage

- Given (tree profiles): B entered app-owned fullscreen without a tiler fullscreen preimage

- Given (column profiles): `COL[C1[A],C2[B*]]` at shipped defaults. B entered
  app-owned fullscreen without a manager preimage; focus B; request the
  project fullscreen toggle.

- When: Focus B; request project fullscreen toggle

- Observe: Native exit attempt vs refusal of app-owned fullscreen; slot/geometry after exit

- Observe (column leg): native exit attempt vs refusal of app-owned fullscreen;
  slot/geometry after exit.

- Then COSMIC: Project toggle dispatches on focus kind: Element enters fullscreen_request with restore captured from its layer, Fullscreen exits via unfullscreen_request with old-slot remap; client-initiated fullscreen (Wayland/X11) routes to the same shell request, so a mapped client fullscreen carries a restore entry; no refusal branch in pinned dispatch; `S(S-cos-fsact)` + `S(S-cos-fsreq)` + `S(S-cos-fsrestore)`; app-specific completion and settled slot/geometry TBD (L: client ack timing)
- Then Hyprland/Dwindle: No refusal branch: client fullscreen maps via the same `setFullscreenMode` path (mapped immediate, unmapped pending); project toggle exits via `NONE` when FS else enters, tree retained; app-specific completion and settled slot/geometry TBD (L: client ack timing); `S(S-hyp-fs)`
- Then bspwm: No refusal branch: app EWMH ADD maps via the same set_state with last_state remembered (honored both ways by default); project `node -t ~fullscreen` toggles FULLSCREEN back to last_state (same-state no-op only); EWMH REMOVE/TOGGLE converge on the same restore; slot retained vacant in place, tree kept; `S(S-bsp-fs)` + `S(S-bsp-admit)`
- Then i3: No refusal: client FULLSCREEN messages and the `fullscreen` command converge on the same mode toggle; tree retained, exit via mode clear plus recalc; app-specific completion/slot TBD; `S(S-i3-fs)`
- Then xmonad/Tall+Navigation2D: No refusal branch on the event path: `ClientMessage` fullscreen add/remove/toggle maps via `fullscreenHooks` (`doFullFloat` fullscreen float / `doSink`), `_NET_WM_STATE` property converged via `chWstate`, tree stack retained; refresh excludes the float from the `S(S-xmo-arrange)` tiled input, restacks floats first-on-top, `tileWindow` applies allocations, and `setTopFocus` actuates stack focus; exit `doSink` leaves B tiled in its retained stack slot. App-specific completion/settled rendering TBD (L: client ack timing beyond the hook dispatch). `S(S-xmo-ewmh)` + `S(S-xmo-arrange)` + `S(S-xmo-topfocus)`; queued.
- Then sway: No refusal: client fullscreen requests (xdg/xwayland) and the `fullscreen` command converge on `container_set_fullscreen`; tree retained, exit via mode clear plus recalc; app-specific completion/slot TBD; `S(S-sway-full)`
- Then qtile/Columns: No refusal branch: toggle_fullscreen flips the state regardless of origin and exit clears through the same save/restore path with the retained tiled slot; app-specific completion TBD (L: client ack timing beyond the toggle dispatch); `S(S-qti-fs)` + `S(S-qti-fsslot)`
- Then awesome/tile: No refusal branch: fullscreen is a plain client property with no owner/preimage tracking, so the toggle clears app-owned fullscreen; B rejoins tiled at its retained global order position via recalc with no focus write, so focus is retained; `S(S-awe-fs)` + `S(S-awe-tile)`
- Then niri: the project toggle clears client-origin fullscreen with no
  preimage gate; the tiled column remains and resumes normal sizing.
  `S(S-nir-maxfs)`.
- Then PaperWM: `paper-toggle-fullscreen` flips the native flag
  regardless of origin with no refusal branch; exit restores the saved
  frame. `S(S-pap-acts)` + `S(S-pap-fsframe)`.
- Then karousel/Lazy: no-counterpart (no project fullscreen toggle verb
  in the action inventory; the native KWin exit path is not this toggle).
  `S(S-kar-acts)` + `S(S-kar-maxfs)`.
- Then paneru: no-counterpart (no project fullscreen toggle verb in
  `Operation`; the native exit journey is not this toggle).
  `S(S-pan-cmds)` + `S(S-pan-axfs)`.
- Then Ours KDE: invokes public fullscreen setter toward normal; `S(S-ours-fs-exit)` + `D(D-fs)`; app-specific native completion/slot outcome TBD
- Then Ours Windows: refuses app-owned exit without its restoration preimage, never synthesizes app F11; `S(S-ours-fs-exit)` + `D(D-fs)`; app-specific native completion/slot outcome TBD
- Variant hook: V-FS-SLOT.

<a id="r-max-06-backfill-admit-a-first-seen-maximized-window-scrolling"></a>
### R-MAX-06: admit a first-seen maximized window

- Given (tree profiles): Tiled workspace with B; first-seen eligible maximized A has no retained tile slot and is not fullscreen

- Given (column profiles): `COL[C1[B]]` occupied at shipped defaults.
  First-seen eligible maximized A has no retained slot and is not
  fullscreen; admit A; later natively restore A.

- When: Admit A; later natively restore A

- Observe: One-shot launch restore vs reserved-slot overlay vs slotless hold; B allocation, A admission and focus

- Observe (column leg): overlay/slot admission; B allocation; A focus; restore.

- Then COSMIC: Tiles A then applies requested maximum as overlay with tile slot retained; other existing maxima unmaximized first; A is the focus target on the active workspace; later native restore clears compositor maximized state and reveals the retained slot without touching focus; B allocation follows ordinary admission anchoring (fixture focus unspecified); `S(S-cos-bornmax)` + `S(S-cos-mapfocus)` + `S(S-cos-native-unmax)`; exact native ack/visuals TBD
- Then Hyprland/Dwindle: Pending client maximum consumed/applied at map as `FSMODE_MAXIMIZED` at the work area (replaces existing workspace FS); B allocation follows the ordinary Dwindle anchor; newcomer A takes focus via the ordinary newcomer path; later native restore exits to `NONE` with recalc restoring tile boxes, exact ack/visuals TBD (L: client ack timing); exact settled sibling order TBD (F: target-workspace anchor inputs plus output geometry unrecorded for the exact settled order); `S(S-hyp-bornmax)` + `S(S-hyp-fs)` + `S(S-hyp-ins)` + `S(S-hyp-newfocus)`
- Then bspwm: Ordinary tile admission: maximum flags are not admission state (only fullscreen state and min==max fixed float are read), so A tiles via ordinary insertion at the desktop focus with newcomer focus and B takes the ordinary split share beside A; later native restore is moot with no maximized flag to clear; `S(S-bsp-admit)` + `S(S-bsp-insert)`
- Then i3: Ordinary tiling with A after the sole B as `[B,A*]` with newcomer focus A (after-focus attach; no maximize admission state, maximum flags derive from layout only); later native restore of the never-maximized A is moot. `S(S-i3-admit)` + `S(S-i3-max)`.
- Then xmonad/Tall+Navigation2D: Ordinary manage/tile (fixed/transient float only via `insertUp`+`float`, else `insertUp` with newcomer focus; no size/maximize/fullscreen inference); profile uses the `ewmhFullscreen` event hook (`fullscreenEventHook`) with default `doFullFloat`/`doSink` and no fullscreen manage hook, so admission itself tiles A with newcomer focus via the `S(S-xmo-arrange)` tiled input with Tall recalc, `tileWindow` applying allocations and `setTopFocus` actuating stack focus; B takes the ordinary Tall share beside A. `S(S-xmo-admit)` + `S(S-xmo-ewmh)` + `S(S-xmo-arrange)` + `S(S-xmo-topfocus)`
- Then sway: Ordinary tiling with A after the sole B (`[B,A*]` order): no maximize admission state (maximize request only schedules a configure; `wants_floating` is fixed-size/dialog/parent only), so A inserts after the focus-inactive B with ordinary newcomer focus; fullscreen flag alone would map fullscreen instead; later native restore of the never-maximized A is moot. `S(S-sway-max)` + `S(S-sway-ins)`
- Then qtile/Columns: Fullscreen-flagged admits fullscreen via auto_fullscreen with the tiled slot kept; fixed-size admits floating via the float rules; otherwise ordinary tiling at the focused cc position with newcomer focus when stealable (no maximize-pending admission state; X11 native maximize is echo-only while the Wayland request drives the state post-map); native ack/visuals TBD (L: client ack timing beyond admission); `S(S-qti-float)` + `S(S-qti-fsslot)`
- Then awesome/tile: Maximized-pending A admits implicitly floating (manage-time maximized hint applies; manage prepends A first yielding global `[A,B]`; geometry kept, B keeps sole-tile allocation) and takes newcomer focus via the shipped global rule; later native REMOVE clears the boolean and A rejoins tiled at its retained global order position (prepended first: [A,B]) via recalc with no focus write, so focus stays A; `S(S-awe-float)` + `S(S-awe-fs)` + `S(S-awe-manage)` + `S(S-awe-tile)`
- Then niri: pending-maximized A opens as a new scrolling column and
  takes focus (Smart activation with no active fullscreen to fence it);
  B's column is retained; later native restore clears the flag.
  `S(S-nir-maxfs)`.
- Then PaperWM: admission converts native-maximized A to width-maximize
  (unmaximize plus width toggle) at the open position selected+1 RIGHT;
  later native restore is moot (no flag left). B keeps its width
  (per-column layout, no rescale). A activates on show via the
  fresh-window branch (newcomer focus). `S(S-pap-widthmax)` +
  `S(S-pap-ins)` + `S(S-pap-layout)`.
- Then karousel/Lazy: tiling admission force-unmaximizes A into an
  ordinary column after C1 (no overlay, no slotless hold; B keeps its width);
  a later native restore is moot; admission focus TBD (fixture states no
  protocol selecting the X11-manage vs Wayland-add fork). `S(S-kar-maxfs)` +
  `S(S-kar-ins)` + `S(S-kar-min)` + `S(S-kwin-manage)` + `S(S-kwin-add)`;
  focus TBD (F: missing protocol selecting the newcomer activation fork).
- Then paneru: no-counterpart for a paneru-native maximized-admission
  leg (no verb or model path); a host-zoomed first-seen window is
  owner-specific with admission/restore behavior TBD.
  `S(S-pan-cmds)` + `S(S-pan-axfs)`; journey queued.
- Then Ours KDE: Selected Q3, delivered offline: first-seen maximized A on a tiled domain reserves a tile slot and retains native maximize as an overlay; no launch unmaximize. B receives its ordinary admission share; native restore of A lands in the reserved slot. Repeated maximized observations settle without native clear, toggle or geometry fighting. Existing `fit_excluded` / `skip-maximized` preserve the overlay ([adapter](../../../kwin/src/plan-adapter.ts), real Engine admission/restore [fixtures](../../../kwin/tests/plan-adapter.test.ts), [change](../../changes/archive/kde-born-maximized-overlay.md)); [Q3 decision](../../decisions.md#window-state-float-sticky-maximize-fullscreen). R-MAX-03 now shares this path ([scope delivery](../../changes/archive/kde-maximized-floating-retile-overlay.md)); the first-domain origin gate is retired. Exact native launch/session-restore journey remains user-owned, TBD.
- Then Ours Windows: Offline-delivered 2026-10-11: schema-v1 `core.born_maximized` defaults to `tile-on-launch` (one launch clear, then ordinary admission); `keep-maximized-overlay` retains native maximize over a reserved slot, with B allocated alongside and A restoring exactly into that slot. Repeated overlay observations take no native clear or geometry writes. Choice binds on first-seen tiled admission: live Apply leaves admitted clients/tree unchanged and affects subsequent admissions only. Fullscreen and later maximizes stay untouched; generic R-MAX-03 retile retains its existing one-shot clear under either value (item 11 gap). Item 13 now keeps fixed automatic births slotless under both choices; hydrated fixed-origin overrides use a reserved overlay across hint loss (tentative route, native TBD; [record](../../changes/archive/windows-fixed-size-admission.md)). [Born-maximized record](../../changes/archive/windows-born-maximized-setting.md); [decision](../../decisions.md#window-state-float-sticky-maximize-fullscreen). Native launch/restore, focus, browser session restore and gaming journey TBD.
- Variant hook: V-MAX-MODEL - retained-slot overlay for ordinary maximize and R-MAX-03; R-MAX-06 changed 2026-10-11 to default tile-on-launch with an overlay alternative. Windows setting delivered offline; KDE setting delivery and all native journeys tracked separately. Windows R-MAX-03 parity remains pending.

<a id="r-max-07-backfill-captionless-full-monitor-cover-scrolling"></a>
### R-MAX-07: captionless full-monitor cover

- Given (tree profiles): Captionless window covers the full monitor; KDE native fullscreen and maximize flags are false

- Given (column profiles): a captionless window covers the full monitor; no
  fullscreen or maximize flags set. First observe/admit it at shipped
  defaults.

- When: First observe/admit it

- Observe: Fullscreen exemption vs ordinary tiling despite monitor coverage

- Observe (column leg): fullscreen exemption vs ordinary admission despite monitor
  coverage.

- Then COSMIC: Fullscreen requires the protocol flag (Wayland request or X11 state, consumed at admission); dialog checks (parent/window-type, min==max) admit floating, otherwise ordinary tiling; size plays no role in the cited admission path; `S(S-cos-admit)` + `S(S-cos-min)`; exact admitted frame and game presentation mode TBD
- Then Hyprland/Dwindle: No size inference: fullscreen/maximize require a protocol flag/pending request or rule; captionless cover alone admits ordinary tiling (fixed-size min==max floats instead); exact frame/presentation TBD; `S(S-hyp-float)` + `S(S-hyp-fs)`
- Then bspwm: Ordinary tiling absent the fullscreen atom or a rule: coverage alone is not read (no size inference in the cited admission path), so the captionless cover tiles; `S(S-bsp-admit)`
- Then i3: Ordinary manage absent fullscreen atom/override-redirect; `S(S-i3-admit)`; exact fixture TBD
- Then xmonad/Tall+Navigation2D: Ordinary tiling absent a protocol float cause (fixed/transient only via `manage`; size plays no role in the cited path); profile fullscreen handling is post-map `ClientMessage` only with no fullscreen manage hook. `S(S-xmo-admit)` + `S(S-xmo-ewmh)`
- Then sway: Ordinary tiling absent a protocol fullscreen flag (admission maps fullscreen only from the request flag; `wants_floating` is fixed-size/dialog/parent only; size plays no role); exact fixture TBD; `S(S-sway-max)`
- Then qtile/Columns: No size inference: fullscreen needs the protocol flag, fixed-size floats instead, otherwise ordinary tiling; exact frame/presentation TBD; `S(S-qti-float)` + `S(S-qti-fs)`
- Then awesome/tile: No size inference in the Lua admission path (float iff type/rules/fixed-size/fullscreen/max flags); captionless cover tiles ordinarily (fixed-size floats instead); exact frame/presentation TBD; `S(S-awe-manage)` + `S(S-awe-float)`
- Then niri: ordinary scrolling admission (configure sizes come from the
  protocol Fullscreen/Maximized flags only; size alone follows
  rules/min-max, never fullscreen). `S(S-nir-maxfs)`.
- Then PaperWM: ordinary tiling admission (fullscreen/maximize enter
  through the Meta API flags only; coverage alone changes nothing).
  `S(S-pap-acts)` + `S(S-pap-widthmax)`.
- Then karousel/Lazy: ordinary tiling admission (shapeability, i.e.
  moveable and resizeable, decides tileability; coverage is tested only
  to ignore external geometry while maximized/fullscreen).
  `S(S-kar-tile)` + `S(S-kar-maxfs)`.
- Then paneru: ordinary strip admission (`AXFullScreen` is the only
  overlay entry; size plays no role in the traced admission/model
  paths). `S(S-pan-axfs)` + `S(S-pan-model)`.
- Then Ours KDE: does not infer fullscreen from size, so no maximize-clear but ordinary tiling is possible; Actual game presentation mode is not established by either shape; `D(D-min-games)`
- Then Ours Windows: captionless monitor containment classifies fullscreen; Actual game presentation mode is not established by either shape; `D(D-min-games)`
- Variant hook: V-FS-SLOT.

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Givens are separate fixtures, never H/V ancestry
claims. Ours KDE and Ours Windows cite Engine + adapter source at
`9241c94` unless stated; selected intent is never evidence. A missing
maximize verb is not a no-op: those cells are qualified, never votes.
Native maximize here means the protocol/client maximized state, never a
width preset: width-only actions (`MaximizeColumn`,
`toggle-maximize-width`, `columnWidthMaximize`) are qualified as
width-only legs with true evidence, never cited as native maximize.
Layout-driven profiles carry explicit tile projections with stated
rectangles where geometry is load-bearing.

### R-MAX-08: focus and move while maximized

- Given (tree profiles): `H[A,B*,C]`, ordinary windows, no rules.
  Prepare `B:max` via the profile native maximize path and record the
  actual pre-action tree per profile. Maximize-stateless profiles cannot
  construct this start; see their qualified Thens (no journey runs).
- Given (column profiles): `COL[C1[A],C2[B*],C3[C]]`, each 0.5W at
  shipped defaults; viewport recorded. Same `B:max` preparation; native
  maximize here means the protocol/client state, never a width preset.
- Given (layout-driven projections): awesome `suit.tile` with
  `nmaster=1`: A master at `(0,0,W/2,H)`, B/C stack with B maximized to
  `(0,0,W,H)` via `Mod4+m`, leaving C at `(W/2,0,W/2,H)`; B stays out of
  `tiled_clients` but in `visible`.
- When: focus left once, then move the focused window right once, with no
  refocus between the steps. Native verbs: COSMIC `Focus(Left)` plus
  directional move; Hyprland `movefocus l` plus `movewindow r`; bspwm
  `node -f west` plus `node -s east`; i3/sway `focus left` plus
  `move right`; xmonad Navigation2D `windowGo L` plus `windowSwap R`;
  qtile Columns `left()` plus `shuffle_right()`; awesome
  `focus.bydirection("left")` plus `swap.bydirection("right")`; niri
  `FocusColumnLeft` plus `MoveColumnRight`; PaperWM `switch-left` plus
  `move-right`; karousel `focusLeft` plus `windowMoveRight`; paneru
  `Focus(West)` plus `Swap(East)`; Ours Engine directional focus/move via
  the adapters.
- Observe: whether the overlay fences focus (B retained vs sibling
  access); what the move acts on (hidden-tree move, refusal, overlay
  clearing) and the actual focused mover.
- Then COSMIC: Focus left is fenced: the tiled maximized B early-returns
  `FocusResult::None` with no traversal, and the Left fallback requests an
  output switch that finds no next output on the single output, so focus
  stays B. Move right then unmaximizes B first (tiling-origin maxima) and
  dispatches the now-ordinary mover through the floating-then-tiling move
  path; at `H[A,B*,C]` the same-axis middle move takes the ordinary
  len-3 fork branch (new Vertical group over C plus B, B first), settling
  at `H[A,H[B*,C]]` with focus staying B (`Done`, no focus write).
  `S(S-cos-maxmove)` + `S(S-cos-maxpolicy)` + `S(S-cos-focus-fallback)` +
  `S(S-cos-move)`.
- Then Hyprland/Dwindle: Focus left is fenced under shipped defaults (`movefocus_cycles_fullscreen=false`: the directional query skips non-allowed tiled candidates while the non-layout-managed covering maximized window exists, so no target is found and focus stays B); move right on the still-maximized B refuses (`Can't move fullscreen window`, no state change and no overlay clearing), so the focused mover stays B with the overlay retained. `S(S-hyp-focus)` + `S(S-hyp-moveswap)` + `S(S-hyp-fs)`.
- Then bspwm: no-counterpart (no maximize command/state in source;
  monocle is a desktop layout, maximize flags are not tree state, so
  the preparation is impossible). `S(S-bsp-layout)` + `S(S-bsp-admit)`.
- Then i3: no-counterpart (no maximize verb; maximize is a derived
  client hint only, so the preparation is impossible). `S(S-i3-max)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no maximize
  command/state; `Full` is a workspace focused-fullscreen layout, not
  per-window maximize). `S(S-xmo-layout)`.
- Then sway: no-counterpart (no maximize verb; the maximize request only
  schedules a configure, so the preparation is impossible).
  `S(S-sway-max)`.
- Then qtile/Columns: B's maximize drops its column (survivors refill with positional current fixup to A); left() wraps from A to C and focuses C (access, not a fence); shuffle_right() on sole-window last-column C is a no-op; `S(S-qti-fs)` + `S(S-qti-focus)` + `S(S-qti-shuffle)` + `S(S-qti-remove)`
- Then awesome/tile: focus left misses (B at x=0 is not strictly right
  of any candidate, so no rect qualifies and B is retained: the overlay
  fences by occlusion); swap right targets C (B.x < C.x) and exchanges
  list positions with no focus write, but the tile arrangement is
  unchanged (maximized B stays out of `tiled_clients`) and focus stays
  on B. `S(S-awe-focus)` + `S(S-awe-geodir)` + `S(S-awe-swap)` +
  `S(S-awe-tile)` + `S(S-awe-fs)`.
- Then niri: focus left activates C1 (column-index step with no overlay
  fence in the focus path); move right reorders the focused column past
  C2 while B's column-held maximized flag is untouched, and the moved
  column stays active. `S(S-nir-focus)` + `S(S-nir-move)` +
  `S(S-nir-maxfs)`.
- Then PaperWM: native maximize converts to full-width maximize at the
  shipped default (no overlay is ever produced); `switch-left` steps to
  A's column and `move-right` swaps A/B model positions with selection
  unchanged. `S(S-pap-widthmax)` + `S(S-pap-focus)` + `S(S-pap-swap)`.
- Then karousel/Lazy: focus left lands on A and restores B to tiled via
  `restoreToTiled` (overlay cleared by the focus change itself); the
  subsequent move right merges A into C2 as a shared column `[B,A]` with
  focus staying A (moves pass no focus).
  `S(S-kar-focus)` + `S(S-kar-maxfs)` + `S(S-kar-move)`.
- Then paneru: no-counterpart for a paneru-native maximize leg (no verb in
  `Operation`, no zoom/maximize AX read); a host-zoomed window is an
  owner-specific external journey with focus/swap behavior TBD.
  `S(S-pan-cmds)` + `S(S-pan-model)` + `S(S-pan-axfs)`; host journey queued.
- Then Ours KDE: G-06 delivered offline 2026-10-10: directional focus keeps
  maximized B (no dispatch/activation). Move right clears maximize once,
  then performs the ordinary move only after observed clear with fresh
  direction/reference/focus/domain guards. Unconfirmed clear refuses/logs
  without a structural move or delayed retry. Eligible maximized floats
  continue into their ordinary half-snap after clear; fullscreen precedence
  unchanged. [Production-entry real-Engine discriminator](../../../kwin/tests/g06-g37-entry-discrimination.test.ts),
  [race/refusal fixtures](../../../kwin/tests/plan-maximized-focus-move.test.ts).
  Native timing/visuals remain TBD.
- Then Ours Windows: current-code gap against G-06 (User 2026-10-10): same shared-Engine journey as KDE (focus to A,
  R2c wrap with B retained); the snap overlay refusal only fires for an
  overlaid focused mover, so A moves normally. `S(S-ours-focus)` +
  `S(S-ours-move)` + `S(S-ours-ovref)`.
- Selected target (REQ-MAX-08 NORMATIVE, G-06 User 2026-10-10): on both
  platforms focus left keeps maximized B (no-op); move right then unmaximizes
  B before the ordinary move. Leave the focus fence via unmaximize or Alt+Tab.
  KDE/shared opt-in focus fence delivered offline; Windows implementation
  pending and its current-code Then remains a gap. Native journey TBD.
  Existing maximized-subject pointer refusals remain unchanged.

### R-MAX-09: send a maximized/fullscreen window to another workspace

- Given (tree profiles): `WS1=H[A,B*]`, WS2 occupied by one ordinary
  tile. Prepare `B:max`; run a fresh variant preparing `B:full`
  instead. Same identities in both legs; never chain max into full.
- Given (column profiles): `WS1=COL[C1[A],C2[B*]]`, WS2 with one
  occupied column at shipped defaults. Same two preparation legs; native
  maximize means the protocol/client state, never a width preset.
- Given (layout-driven projections): awesome `suit.tile`, A master and
  B the sole stack window before its overlay;
  xmonad Tall `nmaster=1` `[A,B*]` for the fullscreen leg only (no
  maximize counterpart exists).
- When: send B to WS2 with the profile native workspace send; record the
  follow policy (follow vs stay). Native verbs: COSMIC `MoveToWorkspace`
  (follows) vs `SendToWorkspace` (no-follow); Hyprland `movetoworkspace`;
  bspwm `node -d WS` (without `--follow` stays; `--follow` follows); i3/sway `move to workspace`; xmonad
  `shiftWin`; qtile `togroup` (follows under shipped `switch_group`);
  awesome `move_to_tag`; niri `MoveWindowToWorkspace` (vs
  `MoveColumnToWorkspace`); PaperWM navigator `takeWindow`; karousel
  cross-desktop column move; paneru `VirtualMoveNumber`; Ours Windows
  Shift+Win+digit send (KDE same-output native desktop-send journey TBD;
  R4 cross-output writes `setDesktops`).
- Observe: whether the overlay state is carried to WS2 or restored
  before transfer; source slot/reflow, target overlay, and follow.
- Then COSMIC: Max leg - B's Element focus routes to `move_element`,
  whose `unmap_element` unmaximizes B first (overlay cleared, source
  reflows) carrying only `was_maximized` in restore data; on the tiled WS2
  the mover fresh-maps to tiling after other maxima are unmaximized, with
  no mover re-maximize (overlay not carried). Full leg - B's Fullscreen
  focus routes to `move_window`, which takes the fullscreen surface with
  its restore (tiling slot state dropped) and re-maps it fullscreen on WS2
  (overlay carried). `MoveToWorkspace` follows, `SendToWorkspace` does not.
  Exact target admission slot/geometry TBD (F: WS2 tile geometry plus
  target focus history unrecorded) and native ack/focus visuals TBD (L).
  `S(S-cos-send)` + `S(S-cos-sendoverlay)`; slot
  queued (F), visuals queued (L).
- Then Hyprland/Dwindle: Transfer runs the move-to-workspace path with the mover's internal fullscreen mode saved, cleared for the move, then re-applied after re-admission, so `FSMODE_MAXIMIZED` and fullscreen both travel with the window object to WS2 while the source refills via removal plus recalc; follow switches to WS2 focusing the mover, silent refocuses the source instead (shipped Lua move follows; `follow=false` stays). `S(S-hyp-movews)` +
  `S(S-hyp-fs)`.
- Then bspwm: max leg no-counterpart (no maximize state:
  `S(S-bsp-layout)` + `S(S-bsp-admit)`); full leg transfers the same
  node via unlink with sibling promotion plus destination-focus insert
  while the node stays vacant-fullscreen in place (shipped form without
  `--follow` stays on the source; following only under `--follow`). `S(S-bsp-xfer)` + `S(S-bsp-state)` + `S(S-bsp-fs)`.
- Then i3: max leg no-counterpart (`S(S-i3-max)`); full leg re-attaches
  the same container (mode flag travels with it) at the destination
  focus with source reflow, focused within WS2 but without switching to
  it. `S(S-i3-movews)` + `S(S-i3-fs)`.
- Then xmonad/Tall+Navigation2D: max leg no-counterpart (no maximize command/state; `Full` is a workspace layout, not per-window maximize) (`S(S-xmo-layout)`); full leg shifts B via `shiftWin` (`delete'` keeps the floating map, `insertUp` above target focus; source view unchanged, no follow) while EWMH fullscreen maps through the float hooks (`doFullFloat`/`doSink` with `_NET_WM_STATE` converge), so the fullscreen float entry travels with the window and B floats fullscreen on WS2; source refocus follows the `S(S-xmo-close)` down-else-up policy. Refresh on each workspace runs the `S(S-xmo-arrange)` tiled-only Tall recalc with floats restacked first-on-top, `tileWindow` applying allocations and `setTopFocus` actuating focus. `S(S-xmo-shift)` + `S(S-xmo-ewmh)` + `S(S-xmo-layout)` + `S(S-xmo-arrange)` + `S(S-xmo-topfocus)` + `S(S-xmo-float)`
- Then sway: max leg no-counterpart (`S(S-sway-max)`); full leg
  detaches and re-adds the same container (mode field intact) with
  source reflow, focuses it on the target via `workspace_focus_fullscreen`,
  and refocuses the source (no-follow, no switch). `S(S-sway-movews)` +
  `S(S-sway-full)`.
- Then qtile/Columns: `togroup` hides B, removes it at the source, and
  re-adds it on WS2 following under shipped `switch_group`; the
  maximized/fullscreen window property travels with the window object,
  so the target overlays while the source refills. `S(S-qti-group)` +
  `S(S-qti-fs)`.
- Then awesome/tile: `move_to_tag` carries the client with its plain
  boolean maximized/fullscreen property (no view switch), so WS2 is
  overlaid while the source order refills without B. `S(S-awe-tag)` +
  `S(S-awe-fs)` + `S(S-awe-tile)`.
- Then niri: window-send strips the overlay (the window arrives Normal:
  column-held flags are left behind), so the max and full legs converge
  on a normal admission with no target overlay; follow honors the
  `focus` flag. Column-send would retain instead and is not this leg.
  `S(S-nir-ws)` + `S(S-nir-wscarry)` + `S(S-nir-acts)`.
- Then PaperWM: max leg converts to width-maximize (no overlay), so
  `takeWindow` transfers an ordinary wide window: removed from the source
  space, re-inserted on WS2 at selected+1 RIGHT with its frame width
  carried (column layout reads the frame), then follow-activated per the
  shipped completion. Full leg: the take path writes no fullscreen state
  (removal plus existing+dropping re-insert keep the flag; layout skips
  unMovable placement and position updates skip fullscreen), so the frame
  is untouched by the extension; whether the native fullscreen overlay
  survives the transfer rides host Mutter state, untraced at pin: overlay
  TBD (host). `S(S-pap-widthmax)` + `S(S-pap-take)` + `S(S-pap-ins)` +
  `S(S-pap-layout)` + `S(S-pap-unmov)` + `S(S-pap-fsframe)`; overlay
  queued (host).
- Then karousel/Lazy: the column transfers after the target grid's last column via `Column.moveToGrid` (desktops reassigned; no maximize/fullscreen clear on the path, so `skipArrange` and the overlay travel: carried in both legs); source reflows via `Grid.onColumnRemoved`; no desktop switch is issued (stay); the B-focused Immediate pass refocuses source-grid A through the script path (raise, on-current-desktop, reasonable-policy `requestFocus`). `S(S-kar-ws)` + `S(S-kar-maxfs)` + `S(S-kar-close)` + `S(S-kwin-scriptact)`.
- Then paneru: max leg no-counterpart for a paneru-native preparation
  (same inventory gap as R-MAX-08) with the host-zoom journey
  owner-specific and TBD;
  full leg transfers via `VirtualMoveNumber` under the Follow/Stay policy
  with `Fullscren`-marker carry TBD. `S(S-pan-cmds)` + `S(S-pan-model)` +
  `S(S-pan-axfs)` + `S(S-pan-ws)`; carry queued.
- Then Ours KDE: G-D2 maximized arrival carry delivered offline 2026-10-10:
  maximized tiled movers pass the same-/cross-output send observers and
  flag-stable arrival fences, native membership/output transfer leaves
  maximize untouched, and geometry skips the overlay. Follow/stay keep
  their established view/focus rules. [Real-Planner follow and native-mock stay fixtures](../../../kwin/tests/workspace-send-maximized-carry.test.ts),
  [cross-output carry/race fixtures](../../../kwin/tests/output-send.test.ts).
  Actual native maximize preservation on arrival, source reflow and later
  restore into the target slot remain user-owned checks;
  fullscreen remains observe-first (currently refuses fullscreen sends),
  separate from whole-workspace output migration D8.
- Then Ours Windows: a tiled maximized B sends through the retained
  Engine route (flag recheck, target allocation kept, overlay geometry
  never writes) with explicit follow/stay. Same-output numbered/relative
  native CLI carry agent-observed 2026-10-11: maximize rect unchanged on
  target, stay retains source MRU/visibility, native unmaximize returns to
  retained target allocation. Native gates passed, base `553c65b` plus
  [delivery record](../../changes/archive/windows-workspace-send-follow-stay.md).
  Fullscreen same-output carry delivered and agent-observed 2026-10-11,
  base `6b76589` plus [delivery record](../../changes/archive/windows-fullscreen-workspace-carry.md):
  numbered follow/stay and relative follow keep B captionless at full-output
  bounds without restoring first; source A reflows, target C shares the
  retained Engine allocation, and project fullscreen exit restores B to its
  target slot. Stay keeps A foreground and B hidden until target selection.
  Fullscreen follow suppresses explicit focus actuation (`focus-suppressed`
  observed), selecting/revealing target without forcing B foreground; this
  gaming-first detail is tentative, pending user review. Stable managed
  app-owned fullscreen uses the same carry gates, also tentative; native
  app-owned carry remains TBD. Slotless born-fullscreen has no retained tile
  allocation and stays ineligible. R-MAX-05 toggle refusal is unchanged.
  Physical chords, deterministic native flag-race injection, games and
  cross-output remain pending. `S(S-ours-ws)` + `S(S-ours-winsend)`.
  G-D2 User 2026-10-10 retains maximized arrival carry as the cross-platform
  target (deliberate COSMIC/niri deviation: sending relocates the whole window
  and its state, unlike an in-layout move). The selected fullscreen carry
  from Table A 2026-10-07 is implemented for Windows same-output sends.
- Discriminating Windows variants (2026-10-11): `WS1=H[A,B*:full]`,
  occupied WS2; follow-send B -> target revealed, B still full, zero explicit
  focus actuation (agent-observed); fresh managed app-owned B:full with no
  project preimage -> send uses the same carry fences, toggle still refuses,
  native send outcome TBD. Fresh slotless born-fullscreen B -> send cannot
  obtain a retained tile allocation; native outcome TBD. Unsupported reference
  outcomes for these ownership/focus variants remain TBD.
- Variant hook: NORMATIVE maximized carry (G-D2 User 2026-10-10);
  fullscreen KDE native policy remains observe-first/TBD.
