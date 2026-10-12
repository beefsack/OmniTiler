# Float / sticky (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

## 4. Float / sticky

R-FLT-07 through R-FLT-10 use one output, scale 1, a tiled workspace,
zero gaps, and work-area/frame geometry in a 2560x1440 area: A
`(0,0,1280,1440)`, B `(1280,0,1280,1440)`. No fullscreen, maximized,
stacked or input-blocked windows. COSMIC uses Vertical workspace layout,
so a missed horizontal focus target tries another output, not workspace
cycling. Each row starts afresh; run once with F ordinary floating, then
repeat with F sticky floating where supported (Hyprland calls this pinned).
COSMIC pinned *workspaces* are unrelated to sticky windows
`S(S-cos-sticky-layer)`. i3 has only F/G in its floating list in R-FLT-09;
its choice there cannot establish geometric ordering. bspwm uses unqualified
`node -f DIR` for focus and the profile's `node -s DIR --follow` for move.
These are source predictions, not live executions; frame delivery and
unspecified tie/config-dependent outcomes remain TBD.

KDE's selected float-focus variant uses COSMIC's axis metric and asymmetric
ties, with sticky candidates first and KWin native encounter order within
each layer. Its miss behavior deliberately retains the existing project edge
policy: Up/Down retain; Left/Right may focus the adjacent output's remembered
eligible tile, never cycle workspaces. KDE's selected move variant currently
delivers stateless halves only; R-FLT-11 discriminates the deferred snap state.

Column legs below use separately stated column Givens with the same identities and action as the original rows; projections are marked explicitly.

<a id="scrolling-backfill-additive-existing-wide-tables-above-unchanged-1"></a>
<a id="r-flt-01-backfill-toggle-float-then-unfloat-scrolling"></a>
### R-FLT-01: toggle float then unfloat

- Given (tree profiles): `H[A,B*,C]`

- Given (column profiles): three single-window columns `COL[C1[A],C2[B*],C3[C]]`
  at shipped defaults (admit A, then B, then C; focus B; record actual
  order, widths and viewport). Toggle float on B, then unfloat. This is
  a lifecycle projection, not an exact H-tree reflow claim.

- When: Toggle float on B, then unfloat

- Observe: Sibling reflow on float; unfloat placement + focus

- Observe (column leg): sibling reflow on float; unfloat placement plus focus.

- Then COSMIC: Float leaves the tiling tree (survivors rescale proportionally), floating frame reuses last geometry else cascade/center; unfloat fresh-admits at focus MRU with no old-slot restore, focus entry kept; Super+G binding, floats above tiles; `S(S-cos-flttoggle)` + `S(S-cos-rem)` + `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-focusfix)` + `D(D-cosmic-kb)` + `D(D-ref)`; exact float frame/native focus TBD
- Then Hyprland/Dwindle: Flat 3-child start has no ordinary binary form (default ratio 1 yields halves, not thirds); exact N-ary outcome TBD. Policy: toggle floats B via sibling promotion + recalc (survivors refill); float frame centers on the old center with the last size; untoggle fresh-admits via the Dwindle anchor (not old-slot restore) and clears pin; exact frames/focus TBD; `S(S-hyp-float)` + `S(S-hyp-ins)`
- Then bspwm: Float keeps B in tree at its slot as vacant (survivors refill via arrange over the non-vacant leaves under either binary embedding); unfloat restores the same slot, not fresh admission; focus stays B (no focus write in either toggle direction); the unrecorded embedding is immaterial to this reflow/placement/focus Observe. `S(S-bsp-float)` + `S(S-bsp-state)`
- Then i3: Float detaches B into a workspace floating wrapper framed from stored geometry (exact frame TBD); survivors rescale via percent fix; unfloat inserts after the tiling-focused descendant with percent reset, no old-slot restore; focus stays B; `S(S-i3-flt-toggle)`
- Then xmonad/Tall+Navigation2D: Float adds B to the floating map with the stack retained (`float`/`sink` are map-only writes); tiling excludes floats so survivors refill via Tall recalc over the `S(S-xmo-arrange)` tiled-only input. Float frame is the current native geometry plus size hints via `floatLocation` (`applySizeHintsContents`; error fallback is full-screen `RationalRect 0 0 1 1`, not a centering fallback; unmanaged centering and `adjust` are admission-only per `S(S-xmo-float)`). `sink` clears the map with stack order retained (same-slot coincidence, not fresh admission). Refresh restacks floats first-on-top (`flt ++ rs`), `tileWindow` applies each allocation unconditionally, `reveal`/`hide` converge visibility, and `setTopFocus` actuates `peek` (focus stays B). `S(S-xmo-float)` + `S(S-xmo-layout)` + `S(S-xmo-arrange)` + `S(S-xmo-topfocus)`
- Then sway: Float detaches B to a default half-width/three-quarter-height centered frame (not stored geometry); survivors refill via fraction renormalize with empty-only reap; unfloat inserts after focus-inactive tiling with fractions reset, no old-slot restore; focus stays B; exact frame TBD; `S(S-sway-float)` + `S(S-sway-cleanup)`
- Then qtile/Columns: Float removes B from the layouts via mark_floating (survivors refill through the column drop); unfloat re-adds via add_client at the focused cc position (fresh admission, no old-slot restore); unplaced floats center per the generic floating placement rule; focus stays B via layout focus when current (mark_floating focuses when current); `S(S-qti-float)` + `S(S-qti-add)` + `S(S-qti-remove)`
- Then awesome/tile: Float excludes B from tiled_clients (survivors refill via stateless recalc over retained global order); floating frame restores stored floating_geometry; unfloat re-includes B at its retained order position via recalc (no old-slot store in source); focus stays B (set_floating writes no focus); `S(S-awe-float)` + `S(S-awe-tile)`
- Then niri: B leaves its column (survivors A,C keep their independent
  column widths; refocusing B resets `activate_prev_column_on_removal`,
  so removal activates clamped-next C); float frame lands near B's tile
  position plus the (50,50) offset clamped to the work area (no offset
  under `Always` centering). Unfloat re-admits B as a new trailing column
  after the active C (active+1), activates B, and deactivates the floating
  layer, so focus follows B. `S(S-nir-float)` + `S(S-nir-flttoggle)` +
  `S(S-nir-close)` + `S(S-nir-ins)` + `S(S-nir-fltanchor)`.
- Then PaperWM: no-counterpart (ordinary B tiles on admission; only
  non-tileable windows enter the floating list, so the toggle has no
  faithful subject). `S(S-pap-float)`.
- Then karousel/Lazy: B flips to `Floating` (no keepAbove at the shipped default; height capped by the toggle-time limit); C2 is destroyed with the last-focused fixup falling to C1, survivors keep widths (reposition only, so no reflow). Unfloat re-tiles into a new column after the last-focused column (C1, so between A and C) with width from B's preferredWidth clamped into [min,max]; neither leg issues a script focus write, so KWin focus stays B. `S(S-kar-float)` + `S(S-kar-acts)` + `S(S-kar-ins)` + `S(S-kar-close)` + `S(S-kar-min)` + `S(S-kar-fltanchor)`.
- Then paneru: `Manage` toggles B to `Unmanaged::Floating`, dropping
  B's strip membership and closing the column gap while survivors keep
  their widths (no cross-column rescale); unfloat removes the mark and
  re-inserts into the active strip at the visually overlapped column,
  else at the end (floats record no `PreviousManagedStrip`, and no rule
  insertion applies under shipped defaults). Neither leg issues a focus
  write, so focus stays B. `S(S-pan-flt)` + `S(S-pan-ins)` +
  `S(S-pan-stripwidth)`.
- Then Ours KDE: leaves tree, siblings reflow; first float centered 60%, then retained frame; unfloat fresh admission, focus retained; `D(D-dec-ww)`
- Then Ours Windows: same leaves-tree/reflow/frame/admission/focus leg as KDE (`D(D-dec-ww)` shared), plus keep-above preimages; behavior rows user-owned `D(D-float)`
- Variant hook: V-FLOAT-GEO.

<a id="r-flt-02-backfill-sticky-across-a-workspace-switch-scrolling"></a>
### R-FLT-02: sticky across a workspace switch

- Given (tree profiles): `H[A,B*]` + WS2

- Given (column profiles): two-column strip with B focused at shipped defaults.
  Sticky-on B, switch workspace, sticky-off. Same visibility predicate
  as the original row. karousel/paneru have no script/tiler sticky verb;
  their legs assess the host journey, never an inferred absence.

- When: Sticky-on B, switch WS, sticky-off

- Observe: Visibility across WS; off placement

- Observe (column leg): visibility across the switch; off placement.

- Then COSMIC: B stays visible across the switch on the same output:
  sticky lives in the output-set `sticky_layer`, the switch flips only
  the set active index, and the sticky stage renders/hit-tests independent
  of the active workspace (sticky before workspace windows) with sticky
  keyboard focus staying valid across the switch. Tiled subjects float
  first; un-sticky restores the remembered Tiling/Floating layer at the
  active workspace, not the origin, and appends focus; stays on top via
  the sticky-first hit order plus the focus-path raise.
  `S(S-cos-sticky)` + `S(S-cos-sticky-layer)` + `D(D-ref)`
- Then Hyprland/Dwindle: Refused no-op: tiled B fails the float-only pin guard (warning, no state change, no auto-float); B stays tiled on its workspace and is not visible on WS2 after the switch (windows belong to workspace spaces; the switch shows the target space); sticky-off on the never-pinned B hits the same guard with no state change, so the off placement leg never runs; `S(S-hyp-pin)` + `S(S-hyp-ws)`
- Then bspwm: Sticky sets on tiled B with no float-only guard; B follows the monitor's focused desktop across the switch via the focus-path sticky transfer with target-show/source-hide; sticky-off clears in place; `S(S-bsp-sticky)` + `S(S-bsp-state)`
- Then i3: Sticky sets on tiled B with no float-only guard, but the push moves only floating stickies, so tiled B stays on its workspace and is not visible after the switch; sticky-off clears the flag in place; `S(S-i3-sticky)`; exact native journey TBD
- Then xmonad/Tall+Navigation2D: no-counterpart (no sticky verb in the core/contrib profile; float is per-window with no all-workspace floating-only sticky verb, so sticky-on/switch/sticky-off has no applicable journey); `S(S-xmo-float)`
- Then sway: Sticky sets `is_sticky` on tiled B (no float-only guard) but effective-sticky requires floating, so only floating stickies relocate to the active workspace floating list; tiled B stays on its workspace and is not visible after the switch; sticky-off clears the flag in place; exact journey TBD; `S(S-sway-sticky)`
- Then qtile/Columns: no-counterpart (no sticky verb or all-workspace flag in source; float is per-window, so sticky-on/switch/sticky-off has no applicable journey); `S(S-qti-nosticky)`
- Then awesome/tile: Sticky is orthogonal with no float-only guard; sticky B stays visible across the switch (sticky reads on every selected tag); sticky-off clears in place with tile recalc; exact visibility journey/frames TBD (live-only); `S(S-awe-sticky)` + `S(S-awe-float)`
- Then niri: no-counterpart (no sticky verb in the full `Action`
  inventory and no sticky state in the layout model).
  `S(S-nir-acts)`.
- Then PaperWM: scratch makes B stuck plus above plus floating
  (`makeScratch` float flag plus above plus `stick`, which synchronously
  removes B from the space tiling); unmake saves the scratch frame and
  restores unstick/un-above with float cleared. The unstick re-enters
  through the existing-window path and re-tiles B as a new column at
  selected+1 under the shipped RIGHT default; existing columns keep
  widths and activation follows the existing-window branch. Whether the
  stuck window stays visible across the switch rides host `stick()`
  semantics, untraced at pin: visibility TBD (host). `S(S-pap-float)` +
  `S(S-pap-ins)` + `S(S-pap-layout)`; visibility queued (host).
- Then karousel/Lazy: sticky-on floats B via the host `desktopsChanged` path (0 desktops fails the exactly-1 gate, so `floatClient`; no sticky verb exists in the Actions/definition inventory); B stays visible across the KWin-native switch via retained host `onAllDesktops` (empty reads on every desktop; the switch only re-arranges with no float mover); sticky-off writes the current desktop (WS2) via `setOnAllDesktops(false)` and B remains there floating (Floating owns no desktop mover, so no auto re-tile). `S(S-kar-acts)` + `S(S-kar-float)` + `S(S-kar-ws)` + `S(S-kwin-sticky)` + `S(S-kwin-switch)`.
- Then paneru: TBD (no sticky verb in `Operation`; host macOS
  all-desktops assignment is the applicable journey, untraced).
  `S(S-pan-cmds)`; host journey queued.
- Then Ours KDE: All managed workspaces of output, float-only; Win+Shift+G; origin-honoring off (tiled fresh-admits, float stays float); `D(D-dec-ww)` KDE + `D(D-sticky)` Windows scoped proof
- Then Ours Windows: All managed workspaces of output, float-only; Win+Shift+G; origin-honoring off (tiled fresh-admits, float stays float); `D(D-dec-ww)` KDE + `D(D-sticky)` Windows scoped proof
- Variant hook: V-STICKY-SCOPE.

<a id="r-flt-03-backfill-float-out-survivor-widths-scrolling"></a>
### R-FLT-03: float-out survivor widths

- Given (tree profiles): 1920px effective parent width; `H[A,B,C]` 50/30/20 (960/576/384)

- Given (column profiles): three single-window columns with widths 960/576/384
  in a 1920px viewport, set explicitly through native width controls
  (not assumed to be shipped presets). Float A; do not unfloat. Widths
  are independent column state, not an exact H-tree share fixture.

- When: Float A (50% child); do not unfloat

- Observe: Survivor widths: ratio-preserve vs equalize

- Observe (column leg): survivor widths (stable vs rescaled).

- Then COSMIC: B/C become 60/40 at 1152/768, ratio preserved; `UT(2026-08-22)` + [Test C](../../cosmic-move-conformance.md#follow-up-manual-observations-tests-a-c)
- Then Hyprland/Dwindle: Flat 3-child start has no ordinary binary form; exact N-ary widths TBD. Policy: float A out via sibling promotion + recalc (survivors refill the work area); exact B/C widths TBD (F: binary embedding of the flat 50/30/20 start plus output geometry unrecorded, so ratio-preserve vs equalize is not established); `S(S-hyp-float)`
- Then bspwm: TBD (binary embedding of the flat 3-child start unevidenced; float-out refill follows per-node ratios, ratio-vs-equalize for this fixture not established); `S(S-bsp-float)` + `S(S-bsp-bal)`
- Then i3: B/C become 60/40 via percent normalization after A detaches; `S(S-i3-flt-toggle)`; exact client pixels (borders/deco) TBD
- Then xmonad/Tall+Navigation2D: Exact 50/30/20 flat-H fixture has no Tall counterpart (Tall splits master/stack at `frac` 1/2, stack splits equally; per-slot ratios live nowhere). Analogous policy only: float A out via tiling exclusion + Tall recalc over the `S(S-xmo-arrange)` tiled-only input; refresh restacks floats first-on-top and `tileWindow` applies survivor allocations. Exact B/C widths TBD (F: flat shares have no Tall pixel basis; asserting a 960/960 projection would read H identity order as Tall stack order, unevidenced). `S(S-xmo-float)` + `S(S-xmo-layout)` + `S(S-xmo-arrange)`; queued.
- Then sway: Float A out via detach + fraction renormalize: survivors keep 30/20 fractions normalized to 60/40 (1152/768 pre-gap/deco), ratio preserved; tiled arrange applies no client-hint clamp; exact client pixels TBD; `S(S-sway-float)` + `S(S-sway-min)`
- Then qtile/Columns: Flat 3-column start has no ordinary Columns form (default num_columns=2); exact widths TBD. Policy: removal redistributes the removed width/height share across survivors (integer growth, remainder first); `S(S-qti-remove)`
- Then awesome/tile: Exact 50/30/20 flat fixture has no tile counterpart (master mwfact 0.5 plus equal-windowfact stack shares); analogous policy only: float-out plus tile recalc over remaining order; exact survivor widths TBD; `S(S-awe-float)` + `S(S-awe-tile)`
- Then niri: B/C keep their independent widths 576/384 (removal drops
  A's column without rewriting survivor widths). `S(S-nir-float)` +
  `S(S-nir-resize)`.
- Then PaperWM: no-counterpart for ordinary non-sticky float-out;
  scratch removes a tile by making it sticky as well, a different
  state/action variant assessed in R-FLT-02. `S(S-pap-float)`.
- Then karousel/Lazy: B/C keep independent column widths 576/384
  (removal shifts positions, not survivor widths). `S(S-kar-float)`.
- Then paneru: B/C keep their independent widths 576/384 (floating A
  out drops its column with no survivor rescale). `S(S-pan-flt)` +
  `S(S-pan-stripwidth)`.
- Then Ours KDE: TBD (Engine removal reflow not checked here); selected User 2026-10-12 D22 (a) target: floating out a tile leaves survivors in the same relative proportions (this fixture: float A, B/C become 60/40); no setting ([User 2026-10-12 D22](../../decisions.md#window-state-float-sticky-maximize-fullscreen)). Verification-only per gap assessment at `fafcd31` (`cosmic_v1.rs:175-198`, `session.rs:2398-2399`, revision-bound); native reflow live check pending; exact native frames TBD
- Then Ours Windows: TBD (Engine removal reflow not checked here); same selected D22 (a) target; native reflow journey pending; exact native frames TBD
- Variant hook: V-FLOAT-REFLOW.

<a id="scrolling-backfill-additive-existing-wide-tables-above-unchanged"></a>
<a id="r-flt-04-backfill-workspace-floating-toggle-scrolling"></a>
### R-FLT-04: workspace floating toggle

- Given (tree profiles): Workspace tiled with A/B, optionally intentional per-window float C

- Given (column profiles): workspace with `COL[C1[A],C2[B*]]` at shipped
  defaults plus optional intentional per-window float C where the model
  supports one; viewport recorded. Same identities and action as the
  original row: toggle workspace floating; move A; open D; toggle tiled.

- When: Toggle workspace floating; move A; open D; toggle tiled

- When (column leg): toggle workspace floating (no per-window float toggle is
  substituted).

- Observe: Untouched frames/native new window, fresh fit vs retained layout; C exception and effects

- Observe (column leg): workspace-wide float conversion vs no-counterpart; the
  move/open/re-enable legs are conditional on an established toggle.

- Then COSMIC: Disable moves every tiled window to floating (last-geometry else cascade frames; maximized ones unmaximized then re-overlaid as Floating); move A is an unspecified pointer/semantic move while floating-only, outcome TBD; D admits floating while floating-only; re-enable fresh-admits every floater (intentional C included) sequentially at focus-MRU long-edge anchors, re-overlaying maxima as Tiling; `S(S-cos-wstile)` + `S(S-cos-last)` + `S(S-cos-axis)`; exact frames/focus TBD
- Then Hyprland/Dwindle: Unsupported action parameter here: no workspace tiling flag/toggle in source (float is per-window dispatch; workspace rules carry monitor/persistent/gaps/border/layout with no tiling/floating default), so the toggle has no built-in equivalent and the conditional move/open/re-enable legs never run; `S(S-hyp-float)` + `S(S-hyp-wsrule)`
- Then bspwm: Unsupported action parameter here: no workspace tiling flag in source (desktop layout tiled/monocle only; float is per-window), so the toggle has no built-in equivalent and the conditional move/open/re-enable legs never run; `S(S-bsp-layout)` + `S(S-bsp-float)`
- Then i3: Unsupported action parameter here: no workspace tiling flag/toggle in source (float is per-window), so the toggle has no built-in equivalent and the conditional move/open/re-enable legs never run; `S(S-i3-wsmode)`
- Then xmonad/Tall+Navigation2D: Unsupported action parameter here: no workspace tiling flag/toggle in source (float per-window; layout always tiles plus floating layer, so the toggle has no built-in equivalent and the conditional move/open/re-enable legs never run); `S(S-xmo-float)` + `S(S-xmo-layout)`
- Then sway: Unsupported action parameter here: no workspace tiling flag/toggle in source (float is per-window; `workspace_layout` default/stacked/tabbed only), so the toggle has no built-in equivalent and the conditional move/open/re-enable legs never run. `S(S-sway-wsmode)`
- Then qtile/Columns: Unsupported action parameter here: no workspace tiling flag or toggle in source (float is per-window; Columns always tiles plus a floating layer), so the toggle has no built-in equivalent and the conditional move/open/re-enable legs never run; `S(S-qti-wstoggle)`
- Then awesome/tile: Per-tag layout switch (floating<->tile, no global flag): the floating layout arranges nothing (A/B keep frames; intentional C stays explicitly floating); move A swaps list positions with geometry neutral under the floating arrange and no focus write; D admitted under the floating layout keeps incoming geometry with c.floating unset (rule placement may still adjust) and takes newcomer focus via the shipped global rule; re-tile is full stateless recalc over retained order with no focus write, so focus stays D; `S(S-awe-layout)` + `S(S-awe-float)` + `S(S-awe-swap)` + `S(S-awe-tile)` + `S(S-awe-manage)`
- Then niri: no-counterpart (no workspace toggle verb; `ToggleWindowFloating`
  is per-window only and `floating_is_active` derives from admission/focus,
  not a command). `S(S-nir-float)`.
- Then PaperWM: no-counterpart (no floating workspace mode and no workspace
  toggle in the registered action inventory; scratch toggles are overlays
  only). `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (no workspace toggle verb;
  `windowToggleFloating` is per-window only). `S(S-kar-acts)`.
- Then paneru: no-counterpart (no workspace-wide float conversion verb;
  `Manage` is per-window and the tier flip is focus-only).
  `S(S-pan-cmds)`.
- Then Ours KDE: floating/tiled user-confirmed, no-write release/fresh fit selected `D(D-dec-ww)`; C exception preserved by actual Engine regression, physical row TBD [record](../../changes/archive/windows-workspace-tiling.md)
- Then Ours Windows: native move/new-window/frame preservation, release/fresh fit, independent border and floating drag underlay/preview suppression proven synthetically; C exception preserved by actual Engine regression, physical row TBD [record](../../changes/archive/windows-workspace-tiling.md)
- Variant hook: V-WS-TILING.

<a id="r-flt-05-backfill-restart-with-a-sticky-float-scrolling"></a>
### R-FLT-05: restart with a sticky float

- Given (tree profiles): B sticky floating on WS1; WS2 exists

- Given (column profiles): sticky-style float B on WS1 where the model supports
  one (PaperWM scratch-stuck; host journeys for karousel/paneru); WS2
  exists. Owner restart; select WS2. Where sticky has no counterpart
  (niri), the restart leg has no faithful start.

- When: Restart tiler owner; select WS2

- Observe: B remains sticky-visible vs becomes ordinary float; remembered origin

- Observe (column leg): B sticky-visible vs ordinary float; remembered origin.

- Then COSMIC: Pinned source persists only pinned-workspace config (output match, tiling flag, id/name; no window/sticky/float carry-over in the traced path); compositor/owner restart differs from script restart; `S(S-cos-persist)`; exact B visibility/origin journey TBD (live: owner-restart execution unobserved)
- Then Hyprland/Dwindle: No-counterpart for the owner-restart step in the dispatcher inventory (exit stops the compositor; reload re-applies config on the live tree with no re-exec, layout dump or window store), so the restart leg never runs and B's sticky/float visibility and remembered origin never resolve; `S(S-hyp-reload)` + `S(S-hyp-shortcut)`
- Then bspwm: Sticky floating B persists: owner `wm -r` dumps full state (monitors/desktops/nodes incl sticky, client state/lastState, focus history, stacking) and re-execs restoring it; `wm -l` loads the same image; B stays sticky floating with its node kept, visible on the selected WS2 via the focus-path sticky follow (transfer plus target-show/source-hide); `S(S-bsp-restore)` + `S(S-bsp-state)`
- Then i3: In-place restart preserves B sticky-visible on the selected WS2: `sticky` round-trips the layout file with percents/focus/floating geometry, placeholders matched to the live windows via the restart-mode swallow; selecting WS2 pushes the floating sticky via the `workspace_show` tail push (tiling cons skipped). The saved floating rect/geometry is inherited on restore and retained by the same-output push, so B keeps its stored frame without numeric pixels or an original-workspace return slot. Admission sticky hints cover fresh maps only, not this restart. `S(S-i3-sticky)` + `S(S-i3-stickyshow)` + `S(S-i3-restart)`.
- Then xmonad/Tall+Navigation2D: Restart preserves the windowset including the floating map (`StateFile` carries the whole `StackSet`; `restart prog True` resumes; resume-only `readStateFile` removes the file after reading), so B's float carries as an ordinary float with its stored `RationalRect`; sticky cross-workspace visibility has no counterpart here (no sticky verb in this profile). After restart Tall rendering is recalculated over the `S(S-xmo-arrange)` tiled-only input, floats restack first-on-top, `tileWindow` applies allocations, and `setTopFocus` actuates stack focus; selecting WS2 views it with B hidden on WS1. `S(S-xmo-restart)` + `S(S-xmo-float)` + `S(S-xmo-arrange)` + `S(S-xmo-topfocus)`
- Then sway: No-counterpart for the owner-restart step in source (command inventory carries `reload` and `exit` with no restart verb; reload is in-place config only), so the restart leg never runs and sticky/float visibility and remembered origin never resolve. `S(S-sway-reload)` + `S(S-sway-sticky)`
- Then qtile/Columns: no-counterpart (no sticky verb or state in source, so the sticky precondition has no subject and the restart leg never runs; restart dump carries groups/layouts/screens/scratchpads only with no per-window float carry); `S(S-qti-nosticky)` + `S(S-qti-state)`
- Then awesome/tile: Sticky roundtrips via _NET_WM_STATE (set echoes the atom to the window; startup re-manage re-reads it; sticky reads on every selected tag); only floating otherwise registered persistent; visibility journey/remembered origin/WS2-select focus TBD (live-only); `S(S-awe-sticky)` + `S(S-awe-ctl)` + `S(S-awe-float)`
- Then niri: no-counterpart (no sticky verb, so no sticky restart
  subject). `S(S-nir-acts)`.
- Then PaperWM: `SaveState` stages monitors/spaces/targetX only with no
  float or stuck member, so the stuck flag itself is not carried; on
  re-adoption above-or-minimized windows re-float via `makeScratch` while
  everything else re-tiles. Scratch-stuck B (above via `make_above`)
  re-scratches through that branch. Whether B is visible on the selected
  WS2 and which origin it retains ride host above/stick persistence
  across disable/enable, untraced at pin: visibility/origin TBD (host).
  `S(S-pap-float)` + `S(S-pap-rst)` + `S(S-pap-readopt)`; visibility
  queued (host).
- Then karousel/Lazy: sticky preparation has no script verb (host `setOnAllDesktops`); the owner restart re-admits via `addExistingClients` fresh (shapeability plus rules plus exactly-1 desktop/activity gates); script destroy writes no desktops, so the host sticky list is retained and B re-admits as ordinary `Floating` (0 desktops fails the gate, no sticky state in script); B stays visible on the selected WS2 via the retained host assignment with no script-remembered origin. `S(S-kar-acts)` + `S(S-kar-readmit)` + `S(S-kar-float)` + `S(S-kwin-sticky)`.
- Then paneru: TBD (host sticky journey plus restart carry both
  untraced). `S(S-pan-cmds)`; queued.
- Then Ours KDE: source adopts surviving native sticky as unknown-origin sticky float; Q3 leaves this behavior and un-stick semantics unchanged. Confirmed adopted-sticky-off ordinary intent now survives the next owner restart through membership-only persistence (user decisions 2026-10-08, delivered offline; D7 tile-override persistence pending), covered by [offline fixtures](../../../kwin/tests/float-intent.test.ts). `S(S-ours-sticky-restart)` + `D(D-sticky)` baseline; exact native restart/visibility journey TBD
- Then Ours Windows: namespaced window-lifetime markers re-adopt sticky as sticky, preserving either pre-sticky origin without hydration native writes. Agent-observed 2026-10-11, base `e48aed1` plus delivery commit: both origins survive two graceful restarts and remain visible at the same frame on WS2; ordinary intentional float/tile hide. [Revision-bound gates/live evidence](../../changes/archive/windows-restart-intent.md). Post-restart un-stick outcomes are recorded observations only and remain TBD; physical input/app dogfood user-owned. `S(S-ours-sticky-restart)` + `D(D-sticky)` baseline consumption superseded.
- Variant hook: V-STICKY-SCOPE.

<a id="r-flt-06-backfill-float-toggle-over-a-maximized-window-scrolling"></a>
### R-FLT-06: float toggle over a maximized window

- Given (tree profiles): Workspace tiled; B is intentional ordinary float, then natively maximized

- Given (column profiles): `COL[C1[A],C2[B*]]` at shipped defaults; B is an
  intentional ordinary float, then natively maximized via the profile
  path. With B focused, toggle ordinary float once. Width-only maximize
  is a qualified leg, never native maximize.

- When: With B focused, toggle ordinary float once

- Observe: Unmaximize-then-admit vs overlay refusal vs logical unfloat beneath retained maximize; settled slot/frame/focus

- Observe (column leg): overlay refusal vs toggle beneath retained maximize; settled
  slot/frame/focus.

- Then COSMIC: Toggle unmaximizes B first, then the floating occupant fresh-admits to tiling at focus MRU (maximize not retained); `S(S-cos-flttoggle)` + `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-focusfix)`; settled frame/focus and native journey TBD
- Then Hyprland/Dwindle: No refusal: toggle runs `changeFloatingMode`, which temporarily clears FS then re-applies it, flipping float while retaining maximized (tiled maximized at the work area); focus stays B (no focus write on the toggle path); exact settled frame TBD (F: work-area geometry unrecorded for the exact settled frame; L: client ack/visuals); `S(S-hyp-float)` + `S(S-hyp-fs)`
- Then bspwm: no-counterpart (no maximize state in source: monocle is layout, maximize flags are client hints not tree state, so the maximized precondition has no subject and the toggle leg never runs); `S(S-bsp-layout)` + `S(S-bsp-admit)`
- Then i3: No refusal: `floating disable` on already-floating B proceeds (only internal-workspace guard), inserting after the tiling-focused descendant; maximize is derived-only so no maximize interplay; settled frame/focus TBD; `S(S-i3-flt-toggle)` + `S(S-i3-max)`
- Then xmonad/Tall+Navigation2D: no-counterpart for the maximized precondition (no maximize state in source: `Full` is a workspace layout rendering focus fullscreen, maximize flags are not tree state); toggle path is float/`sink` only with stack retained; `S(S-xmo-layout)` + `S(S-xmo-float)`
- Then sway: Maximized precondition has no counterpart here (no maximize state; client request only schedules a configure); toggle path has no maximize/refusal branch, so an ordinary float B toggles via the standard float path; settled slot/frame/focus TBD; `S(S-sway-float)` + `S(S-sway-max)`
- Then qtile/Columns: Maximized is a float state, so toggling float on maximized B runs the unfloat path (fresh tiled admission at the focused cc position, maximize dropped with no restore dispatch); settled frame is the tiled allocation and focus stays B; exact tiled pixels TBD (F: column/work-area geometry unrecorded); `S(S-qti-float)` + `S(S-qti-fs)` + `S(S-qti-fsslot)`
- Then awesome/tile: Maximized implies implicit float, so the toggle (`floating.toggle` via `set_floating`) flips only the explicit `floating` property while the maximize-driven implicit flag is retained orthogonally (explicit reads first); no unmaximize branch and no attempted-state fence, maximize stays a plain boolean. Settled slot/frame/focus TBD (live-only); `S(S-awe-float)` + `S(S-awe-fs)`
- Then niri: toggle runs the plain tile-move path with no maximize or
  refusal branch (`toggle_window_floating`: scrolling `remove_tile`
  then floating `add_tile`); floating admission resolves a non-normal
  size via the stored floating size else `(0,0)`, and the column
  pending-maximized flag stays on the removed scrolling column
  (floating windows cannot be maximized). Settled frame is
  client-driven and focus follows the active target; both TBD
  (live-only), and native configure timing TBD.
  `S(S-nir-maxfs)` + `S(S-nir-flttoggle)`; frame/focus queued.
- Then PaperWM: no-counterpart for an ordinary non-sticky float toggle;
  scratch unmake also unsticks, not this action. Tiled native maximize's
  width conversion does not establish a maximized-dialog toggle.
  `S(S-pap-acts)` + `S(S-pap-float)`.
- Then karousel/Lazy: `windowToggleFloating` flips the Tiled/Floating
  state with no maximize or clear branch, and the Floating state keeps
  the host maximize (height cap only); admission-time force-unmaximize
  is a different journey, not this toggle. The maximized KWin state
  travels host-side with the client; settled frame/focus and native
  clear timing TBD (live-only). `S(S-kar-maxfs)` + `S(S-kar-acts)` +
  `S(S-kar-float)`; frame queued.
- Then paneru: a host-zoomed window is the applicable owner-specific
  journey (still TBD); no paneru-native maximized leg exists (`Operation`
  has no maximize verb and no zoom AX read, so the toggle has no native
  subject). `S(S-pan-cmds)` + `S(S-pan-axfs)`; journey queued.
- Then Ours KDE: B9 delivered offline 2026-10-09: one native maximize clear under the existing echo fence; fresh exact-ref observation must show clear before the ordinary fresh-admission dispatch and geometry writes. Carries the restored frame, not the maximized frame; refused/unobserved/raced clear logs a narrow refusal, preserves float intent and allows a later press. Real Planner ordinary/fixed fixtures and shared-core pre/post-clear regression pass; fixed unfloat commits the existing explicit tile override. [Record](../../changes/archive/maximized-intentional-unfloat.md); native frame/focus journey TBD.
- Then Ours Windows: B9 delivered offline 2026-10-12: ordinary maximized unfloat holds the exact lifetime, issues one nonactivating clear, observes clear and fresh-admits using the restored frame. Agent-observed native 2026-10-12, scoped Notepad/Paint, base `e506350` plus combined Windows changes: exact-owner float CLI on maximized Paint clears, observes clear before geometry, fresh-tiles with focus retained; both tile readbacks match and subsequent reconcile applies zero writes. Refused/unobserved/raced clear retains float intent and permits a later press, covered offline. Scripted seams pin lifetime/focus/domain/fullscreen races and fixed D3/D7 overrides; native failure/race, marker/restart and physical-feel/game/DPI/multi-output remain TBD. [Record](../../changes/archive/windows-maximized-intentional-unfloat.md).
- Discriminating variants: [R-FLT-06 readings](../reference-outcomes.md#r-flt-06-readings); unsupported reference/native outcomes remain TBD.
- Variant hook: V-FLOAT-GEO / V-MAX-MODEL.

<a id="r-flt-07-backfill-tile-origin-focus-over-floats-scrolling"></a>
### R-FLT-07: tile-origin focus over floats

- Given (tree profiles): `H[A,B*]` + F floating `(1000,500,300,200)`

- Given (column profiles): two single-window tiled columns `COL[C1[A],C2[B*]]`
  plus ordinary float F at `(1000,500,300,200)` (dialog/transient where
  the model tiles ordinary windows); matching tile rectangles A
  `(0,0,1280,1440)`, B `(1280,0,1280,1440)` in a 2560x1440 viewport.
  Repeat with PaperWM scratch-stuck F; niri has no sticky counterpart,
  and karousel/paneru host-sticky legs remain TBD (R-FLT-02).

- When: Focus left

- When (column leg): focus left (same native verbs as R-FOC-01 per profile).

- Observe: Can tile-origin focus enter ordinary/sticky F

- Observe (column leg): whether tile-origin focus can enter F.

- Then COSMIC: A; F excluded from tiled search, ordinary/sticky alike; `S(S-cos-flt-focus)`
- Then Hyprland/Dwindle: A; ordinary/pinned F excluded; `S(S-hyp-flt-focus)` + `S(S-hyp-flt-pin)`
- Then bspwm: A; F is eligible but boundary distance A=1 < F=19; sticky same; `S(S-bsp-flt-focus)`
- Then i3: A; floating/sticky F outside tiled walk; `S(S-i3-flt-focus)`
- Then xmonad/Tall+Navigation2D: A; tiled subjects navigate the tiled layer only (floats excluded, ordinary/sticky alike in profile terms); `S(S-xmo-nav)`
- Then sway: A; F excluded from the tiled search (tiled subjects use the tile walk, which never consults the floating list); ordinary/sticky alike; `S(S-sway-focus)`
- Then qtile/Columns: A; tiled-column verbs never consult the floating list, ordinary/sticky alike (no sticky concept); `S(S-qti-focus)` + `S(S-qti-float)`
- Then awesome/tile: A; tiled-origin bydirection includes floats (no float exclusion in the walk): left-of-B candidates are A and F, nearest is A by edge distance; sticky same; `S(S-awe-focus)`
- Then niri: A. `focus_left` steps the column index and activates C1;
  the floating list is never consulted on the tiled path. `S(S-nir-focus)`.
- Then PaperWM: A for both dialog and scratch-stuck F. `switchLeft`
  walks the tiled columns only (the
  `_floating` list is never a switch candidate); single-window C1
  resolves to A. `S(S-pap-focus)`.
- Then karousel/Lazy: A. Focus verbs dispatch tiled-only
  (`doIfTiledFocused`); the left column's single window takes focus.
  `S(S-kar-focus)`.
- Then paneru: A; F is off-strip so never a strip peer for the West
  step. `S(S-pan-cmds)` + `S(S-pan-focus)` + `S(S-pan-flt)`.
- Then Ours KDE: KDE/Windows: A; ordinary/sticky F has no tile leaf, hence never a target; unchanged, KDE regression `D(D-float-nav)` + `S(S-ours-flt-target)`
- Then Ours Windows: A; tile-origin Engine focus still excludes ordinary/sticky floats. Adapter candidate exclusion and existing Engine navigation regressions pass offline 2026-10-12, base `e506350`; item-10 ordinary physical focus/half-snaps user-accepted 2026-10-12, scoped normal owner, single-output Notepad/Paint, combined Windows deliveries. Exact tile-origin fixture native TBD; [record](../../changes/archive/windows-floating-directional-navigation.md).
- Variant hook: V-FLOAT-FOCUS.

<a id="r-flt-08-backfill-float-origin-focus-miss-scrolling"></a>
### R-FLT-08: float-origin focus miss

- Given (tree profiles): `H[A,B]` + F* floating `(500,500,300,200)`; no other floats

- Given (column profiles): two single-window tiled columns `COL[C1[A],C2[B]]`
  plus focused ordinary float `F* (500,500,300,200)` (dialog/transient
  where applicable); no other floats; tile rectangles A/B and viewport
  as in R-FLT-07. Record the remembered tiled selection for PaperWM.

- When: Focus right

- When (column leg): focus right (same native verbs as R-FLT-08 per profile).

- Observe: Float-origin focus enters tiles vs misses/refuses

- Observe (column leg): float-origin focus enters tiles vs miss/refusal.

- Then COSMIC: No local target: tiles excluded; output fallback has no next output, F retained. Sticky same; `S(S-cos-flt-focus)` + `S(S-cos-focus-fallback)`
- Then Hyprland/Dwindle: No-op: float-only search and edge retry find no other float; pinned same; `S(S-hyp-flt-focus)` + `S(S-hyp-flt-pin)`
- Then bspwm: B; unqualified selector crosses layers (distance B=481 < A=799); sticky same; `S(S-bsp-flt-focus)`
- Then i3: F retained: horizontal floating-list wrap selects self; sticky same; `S(S-i3-flt-focus)`
- Then xmonad/Tall+Navigation2D: F retained: float-only search finds no other float (self skipped), miss is no-op so no focus change; tiles excluded; `S(S-xmo-nav)`
- Then sway: F retained: float-only center-delta search finds no other float (self skipped, no furthest for wrap), returns NULL so no focus change; sticky same (no sticky filter in the float search); `S(S-sway-focus)`
- Then qtile/Columns: Float-origin directional verbs still run the tiled-column walk (floats are never targets and no float-only search exists), so focus leaves F for the stepped column cw; `S(S-qti-focus)`
- Then awesome/tile: B; float-origin bydirection still walks visible clients (floats and tiles alike): sole right candidate is B with self excluded; sticky same; `S(S-awe-focus)`
- Then niri: F retained. Floating `focus_right` searches floats by
  center distance and returns false with no other float (no wrap, no
  tile fallback). `S(S-nir-fltfocus)`.
- Then PaperWM: TBD. Dialog/scratch focus leaves the remembered tiled
  `selectedWindow` unchanged; `switchRight` uses that selection, not F.
  It may focus a tile or hit the strip edge depending on the unrecorded
  tiled selection. `S(S-pap-float)`; selection queued.
- Then karousel/Lazy: F retained. Focus verbs dispatch tiled-only, so a
  float-origin step is a no-op. `S(S-kar-focus)`.
- Then paneru: F retained; float-origin focus searches visible floats
  only, and with no other float there is no target and no tile
  fallback. `S(S-pan-cmds)` + `S(S-pan-flt)`.
- Then Ours KDE: F retained, no local float or adjacent output, tiles excluded; ordinary/sticky same; offline `D(D-float-nav)`, live TBD
- Then Ours Windows: F retained on local miss, ordinary/sticky same; no workspace cycling or cross-output actuation (item 5 gap). Offline 2026-10-12, base `e506350`: selection/candidate/Engine fixtures pass. Ordinary float-origin physical focus user-accepted 2026-10-12 under R-FLT-07's conditions; exact miss native fixture remains TBD; [record](../../changes/archive/windows-floating-directional-navigation.md).
- Variant hook: V-FLOAT-FOCUS.

<a id="r-flt-09-backfill-float-origin-focus-toward-a-farther-float-scrolling"></a>
### R-FLT-09: float-origin focus toward a farther float

- Given (tree profiles): `H[A,B]` + F* floating `(500,500,300,200)` + ordinary float G `(1800,500,300,200)`

- Given (column profiles): tiled columns plus focused ordinary float
  `F* (500,500,300,200)` and a second ordinary float G at
  `(1800,500,300,200)` (dialog/transient where applicable); tile
  rectangles A/B and viewport as in R-FLT-07. Repeat with PaperWM
  scratch-stuck F; karousel/paneru host-sticky legs remain TBD.
  Record PaperWM's remembered tiled selection.

- When: Focus right

- When (column leg): focus right (same native verbs as R-FLT-09 per profile).

- Observe: Farther float G vs nearer tile B; sticky-to-ordinary focus

- Observe (column leg): farther float G vs nearer tile; focus target.

- Then COSMIC: G; ordinary/sticky floats share candidates, tiles excluded; x-coordinate delta selects G; `S(S-cos-flt-focus)`
- Then Hyprland/Dwindle: G by floating angle/distance search, not COSMIC's top-left-axis rule; pinned F same; `S(S-hyp-flt-focus)` + `S(S-hyp-flt-pin)`
- Then bspwm: B; all layers eligible, boundary distance B=481 < G=1001; sticky F same; `S(S-bsp-flt-focus)`
- Then i3: G; next floating-list entry (wrap if needed), not geometry; sticky F same; `S(S-i3-flt-focus)`
- Then xmonad/Tall+Navigation2D: G by float-layer center navigation (F center 650 vs G center 1950, positive-delta nearest), not tile geometry nor list order; tiles excluded; `S(S-xmo-nav)`
- Then sway: G by center-delta float search (F center 650 vs G center 1950, positive delta nearest), not COSMIC top-left rule nor list order; tiles excluded; sticky shares candidates (no sticky filter); `S(S-sway-focus)`
- Then qtile/Columns: Tiles only: G is never a candidate (no float-only search exists; directional verbs walk tiled columns), so focus lands on a tile and never G; exact tile target TBD (F: stored tiled current unrecorded, left/right reads cc.cw); `S(S-qti-focus)`
- Then awesome/tile: B by edge-distance geometry (B nearer than G), not list order; tiles and floats share candidacy; sticky same; `S(S-awe-focus)`
- Then niri: G, raised to the floating front. Floating `focus_right`
  picks the nearest positive-delta float center (F 650 vs G 1950) and
  activates it. `S(S-nir-fltfocus)`.
- Then PaperWM: G is not a tiled-switch candidate; final focus TBD.
  Dialog/scratch focus preserves the remembered tiled selection, which
  `switchRight` uses instead of F. `S(S-pap-float)`; selection queued.
- Then karousel/Lazy: F retained. Float-origin focus is a tiled-only
  no-op, so neither the nearer tile nor G is targeted.
  `S(S-kar-focus)`.
- Then paneru: G by the float cone search (F center 650 vs G center
  1950, positive-delta nearest); tiles excluded. `S(S-pan-cmds)` +
  `S(S-pan-flt)`.
- Then Ours KDE: G by top-left x delta; ordinary/sticky share candidates, nearer B excluded; offline `D(D-float-nav)`, live TBD
- Then Ours Windows: G selected by top-left x delta; sticky-first single race, nearer tiles excluded; visible non-home sticky and retained maximized float targets participate. Offline 2026-10-12, base `e506350`, axis/tie/candidate fixtures. Float-origin Win+arrow focus physically user-accepted 2026-10-12 under R-FLT-07's conditions; exact tie/non-home sticky/game/DPI/multi-output TBD. Observed-before-retained tie order remains Tentative 2026-10-12, pending user review; no comparable reference majority; [record](../../changes/archive/windows-floating-directional-navigation.md).
- Variant hook: V-FLOAT-FOCUS.

<a id="r-flt-10-backfill-semantic-float-move-scrolling"></a>
### R-FLT-10: semantic float move

- Given (tree profiles): `H[A,B]` + free, unsnapped F* floating `(1000,500,300,200)`

- Given (column profiles): tiled columns plus free unsnapped ordinary float
  `F* (1000,500,300,200)` (dialog/transient where applicable) at shipped
  defaults; viewport recorded.

- When: Move right once

- When (column leg): move right once via the profile semantic move verb.

- Observe: Move/resize geometry vs tree swap vs refusal; remains floating vs tiles

- Observe (column leg): move/resize geometry vs reorder vs refusal; float state.

- Then COSMIC: Right-half snap `(1280,0,1280,1440)` in floating layer, not tile-tree admission; sticky same. Later snap-state transitions can quarter/maximize or request workspace/output transfer; `S(S-cos-flt-move)`
- Then Hyprland/Dwindle: Snap F to right work-area edge, retain size/y and floating state (reserved extents affect exact x); pinned same; `S(S-hyp-flt-move)` + `S(S-hyp-flt-pin)`
- Then bspwm: Profile swaps F/B tree nodes; F stays floating at its original frame/focus, tile arrangement recomputed (B exact frame TBD). Sticky same; `S(S-bsp-flt-focus)` + `S(S-bsp-flt-swap)`. Separate pixel `-v` moves F, not this profile action; `S(S-bsp-move)`
- Then i3: Bare `move right`: F.x += 10px, stays floating; sticky same; `S(S-i3-flt-move)`
- Then xmonad/Tall+Navigation2D: F stays floating at its original frame: sole float F has no directional float-layer target, so the swap returns `id` with frame/floating retained; no tile-tree admission, no snap; `S(S-xmo-nav)`
- Then sway: Bare `move right`: F.x 1000->1010 (+10px default), stays floating via frame move (single output, no workspace change); sticky same (no guard); `S(S-sway-move)`
- Then qtile/Columns: Unsupported action parameter here: no directional float-move or snap verb in source (shuffle operates on the tiled current; set_position moves floats only by explicit coordinates with tiled pointer-swap, and there is no half/quarter mapping), so F stays floating at its frame; `S(S-qti-snap)`
- Then awesome/tile: Right swaps F/B by strict-origin edge distance (sole right candidate B at distance 20; A at x=0 excluded): the swap exchanges global list positions with no focus write, so focus stays F*; F stays floating at its frame (swap writes no geometry; floating arrange is a no-op) while B's tile allocation recomputes over the new order; no snap/half mapping in source; `S(S-awe-swap)` + `S(S-awe-geodir)` + `S(S-awe-float)` + `S(S-awe-tile)`
- Then niri: F.x 1000->1050, stays floating. Active-float
  `move_right` steps the float position by 50px; no snap state.
  `S(S-nir-ptr)` + `S(S-nir-move)`.
- Then PaperWM: F stays floating at its frame. The move-right verb binds
  same-space `swap` with no window argument, so it exchanges the
  remembered tiled selection's model positions instead of F (float focus
  leaves `selectedWindow` unchanged); selection unchanged with relayout
  plus viewport ensure. The swap path exchanges positions plus relayout
  only, with no snap member in the traced verb path (move verbs bind
  `swap` only). The exact tiled pair follows the live remembered
  selection. `S(S-pap-swap)` + `S(S-pap-float)` +
  `S(S-pap-moveverbs)`.
- Then karousel/Lazy: F retained floating. Semantic moves dispatch
  tiled-only (`doIfTiledFocused`); no snap state exists.
  `S(S-kar-move)` + `S(S-kar-focus)`.
- Then paneru: F stays floating at its frame; the focused float has no
  strip index so the same-strip exchange finds no peer, and East never
  falls through to another display. `S(S-pan-move)` + `S(S-pan-swap)` +
  `S(S-pan-swap-peer)` + `S(S-pan-flt)`.
- Then Ours KDE: right-half `(1280,0,1280,1440)`, remains floating/focused, sticky same; signal/reconcile retention regression `D(D-float-nav)`, live TBD
- Then Ours Windows: right-half `(1280,0,1280,1440)` in the zero-gap fixture; ordinary/sticky remain floating and focused. One guarded geometry write; incompatible declared sizes refuse, no client-clamp reassertion. Offline arithmetic/constraints/reconcile fixtures 2026-10-12, base `e506350`. Win+Shift+arrow half-snaps physically user-accepted 2026-10-12 under R-FLT-07's conditions; exact zero-gap readback/sticky/fixed/clamp/preset/game/DPI/multi-output TBD; [record](../../changes/archive/windows-floating-directional-navigation.md).
- Variant hook: V-FLOAT-SNAP.

<a id="r-flt-11-backfill-second-float-move-and-snap-state-scrolling"></a>
### R-FLT-11: second float move and snap state

- Given (tree profiles): Same free F* and zero-gap fixture as R-FLT-10

- Given (column profiles): same float fixture as R-FLT-10 backfill
  (`F* (1000,500,300,200)`) at shipped defaults. Move right, then move
  up, with no reset between the steps.

- When: Move right, then move up

- Observe: Stateful quarter-snap vs stateless requested half

- Observe (column leg): stateful snap vs stateless second move.

- Then COSMIC: Top-right quarter `(1280,0,1280,720)`; stays floating, sticky same; `S(S-cos-flt-move)`
- Then Hyprland/Dwindle: Top-right corner `(2260,0,300,200)` on the 2560x1440 work area with zero reserved decoration extents (x=2260-right extent, y=top extent; right snaps x retaining y/size, up then snaps y retaining x/size), remains floating; pinned same; non-zero extents/client ack/visuals TBD; `S(S-hyp-flt-move)` + `S(S-hyp-flt-pin)`
- Then bspwm: First leg swaps F/B per R-FLT-10; second leg (up from F's floating frame) sees A and B north at equal boundary distance, tie broken by history rank, so the swap partner is unevidenced here: TBD; `S(S-bsp-flt-focus)` + `S(S-bsp-flt-swap)`
- Then i3: Two bare pixel moves (right x += 10, then up y -= 10), size and floating state retained, no snap state; sticky same; exact frame TBD (output clamp); `S(S-i3-flt-move)`
- Then xmonad/Tall+Navigation2D: Both legs no-op: sole float F has no directional float-layer target either leg, so swaps return `id` with frame/floating retained; no snap state; `S(S-xmo-nav)`
- Then sway: Two bare pixel moves: (1000,500)->(1010,500)->(1010,490), size and floating state retained, no snap state; sticky same; exact clamp TBD; `S(S-sway-move)`
- Then qtile/Columns: Unsupported action parameter here on both legs: same verb gap as R-FLT-10 with no snap state in source, so both steps leave F floating at its frame; `S(S-qti-snap)`
- Then awesome/tile: Both legs geometric swap with no snap state in source. First leg swaps F/B as in R-FLT-10 (sole right candidate B); F keeps its floating frame, so the up leg measures from (1000,500,300,200): A and B both qualify (y=0<500) with B nearer (about 981 vs about 1372 by the edge-distance metric), so the second leg swaps F/B back; both legs keep focus on F* (no focus write); `S(S-awe-swap)` + `S(S-awe-geodir)` + `S(S-awe-float)`
- Then niri: (1050,500) then (1050,450), stays floating. Both legs step
  50px with no snap state. `S(S-nir-ptr)`.
- Then PaperWM: both legs run the same selection-based `swap` as R-FLT-10
  (the up leg swaps the remembered selection's row), so F stays floating
  at its frame through both steps; the swap path exchanges positions plus
  relayout only, with no snap member in the traced verb path. Exact pairs
  follow the live remembered selection. `S(S-pap-swap)` +
  `S(S-pap-float)` + `S(S-pap-moveverbs)`.
- Then karousel/Lazy: both legs no-op, stays floating. Tiled-only move
  verbs never engage; no snap state exists. `S(S-kar-move)`.
- Then paneru: both legs no-op, stays floating; same missing-peer path
  as R-FLT-10 with no snap state in the swap path. `S(S-pan-move)` +
  `S(S-pan-swap)` + `S(S-pan-swap-peer)` + `S(S-pan-flt)`.
- Then Ours KDE: top half `(0,0,2560,720)`, remains floating/focused, sticky same; stateless subset tested `D(D-float-nav)`; stateful transitions deferred, live TBD
- Then Ours Windows: top half `(0,0,2560,720)` after Right then Up in the zero-gap fixture; repeats request the same half. Stateless arithmetic/reconcile covered offline 2026-10-12, base `e506350`. Item-10 half-snaps physically user-accepted 2026-10-12 under R-FLT-07's conditions; exact repetition/zero-gap/sticky fixture TBD. Quarter/maximize/outward-transfer states deferred; [record](../../changes/archive/windows-floating-directional-navigation.md).
- Variant hook: V-FLOAT-SNAP.

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours cells cite Engine + adapter source at `9241c94`
unless stated; selected intent and doc assertions are never evidence.
Raise/lower cells name the native verb or handler per profile; an absent
tiler verb never denies a host pointer journey, so free drag/resize stays
TBD (queued) rather than no-counterpart. Where a raise path is
established but explicit lower is not, the cell is partial or mixed;
applicable unknowns are queued, evidenced absent verbs are not.

### R-FLT-12: raise and lower overlapping floats

- Given (tree profiles): `H[A,B]` plus overlapping ordinary floats F,G
  at shipped defaults; G currently above F. Record actual focus and
  stacking order per profile before acting; no fullscreen, maximized or
  input-blocked windows.
- Given (column profiles): one strip/workspace with tiled columns plus
  two overlapping ordinary floats F,G (G above F) at shipped defaults;
  viewport recorded. Same raise-then-lower journey. PaperWM uses
  dialog/transient floats (the only overlapping-float counterpart; see
  its Then); paneru uses `Unmanaged::Floating` windows.
- When: step 1 raise F (native verb per profile below); step 2 lower F
  (native verb per profile). No reset between the steps. Pointer
  centre-join stays R-DRAG-01 and never substitutes.
- Observe: z-order change vs focus-only; tile/float layer boundary;
  settled stacking order after each step.
- Then COSMIC: focusing F raises it to the top (focused sticky and
  ordinary floaters are raised via the focus path). Explicit lower is a
  qualified no-counterpart leg: the pinned `Action` inventory lists no
  raise/lower verb (stacking-adjacent verbs are the `ToggleStacking`
  tile/stack convert and the `SwapWindow` overview grab only), no lower
  binding exists, and the traced client-request handlers expose no lower
  path; free pointer drag is not this leg (stays R-DRAG-01).
  `S(S-cos-raise)`.
- Then Hyprland/Dwindle: Focusing or pressing F raises it to the top of the window order (float-toggle, activate and click paths raise floating windows); lower runs only through the Lua `alter_zorder bottom` path (no shipped keybind verb), moving F to the bottom with no focus write, so focus stays F and tiling is untouched; settled order is F on top after step 1 and F at the bottom after step 2.
  `S(S-hyp-raise)` + `S(S-hyp-float)`.
- Then bspwm: focusing F restacks it above (focused nodes take the
  above-limit branch; floats participate unless `auto_raise` is held
  false during pointer motion); lower is a qualified no-counterpart leg
  (no project same-layer order-lower verb; `node -l` sets the BELOW
  stacking layer, a persistent layer change rather than this leg's
  order-lower). `S(S-bsp-stack)`.
- Then i3: activating or clicking F raises it to the tail of the
  workspace floating list; no lower verb exists in the float inventory
  (`con.h`/`floating.h` expose raise only). `S(S-i3-raise)`; lower is a
  qualified no-counterpart leg.
- Then xmonad/Tall+Navigation2D: floats always restack above tiles
  (`flt ++ rs` order); the F-vs-G order effect of focusing F is untraced
  (no reorder verb; `focusWindow` rotation vs restack order unresolved).
  `S(S-xmo-restack)`; order queued.
- Then sway: pressing, mapping, moving or resizing F raises it to the
  top of the scene and the end of the floating list; no lower verb
  traced in the container/input inventory. `S(S-sway-fltraise)`; lower
  is a qualified no-counterpart leg; the declared raise uses a press.
- Then qtile/Columns: activating F brings it to front; X11
  `move_to_bottom()` lowers it beneath its same-layer peers without
  changing focus (masked stacking write). Wayland also exposes bottom
  movement. `S(S-qti-raise)`.
- Then awesome/tile: `c:raise()` moves F to the top of its layer and
  `c:lower()` moves it to the bottom (both registered client methods).
  `S(S-awe-raise)`.
- Then niri: focusing F raises it to index 0 of the floating list
  (`FocusWindow` runs the layout activation, which raises); no lower
  verb exists in the full `Action` inventory (the without-raising
  activation is internal focus-follow code, not a verb).
  `S(S-nir-fltact)` + `S(S-nir-acts)`; lower is a qualified
  no-counterpart leg.
- Then PaperWM: ordinary non-sticky app floats have no counterpart;
  scratch is a separate sticky float variant. Dialog-float variant:
  admission marks above, but raising F relative to G and explicit lower
  have no verb in the registered action inventory (tiled selection's
  `raise()` is not a dialog raise path); the F/G stacking journey rides
  host Meta stacking, untraced at pin: stacking TBD (host).
  `S(S-pap-float)` + `S(S-pap-acts)`; stacking queued (host).
- Then karousel/Lazy: TBD (the complete Actions/definition inventory lists no raise/lower verb, so neither step has a script path; host raise rides KWin `activateWindow` raise per `S(S-kwin-scriptact)` but the step producer selecting activation vs another raise path is unstated for this profile, and no lower path is selected). `S(S-kar-acts)` + `S(S-kar-float)` + `S(S-kwin-scriptact)`; producer TBD (F: missing step raise/lower producer selecting the host path).
- Then paneru: no-counterpart - `RaiseFloating` focuses the
  last-floating window while raising the other visible floats, so
  raising an arbitrary F has no verb path (AX raise further couples with
  app-frontmost per the deliberate comment), and the `Operation`
  inventory lists no lower verb. `S(S-pan-flt)` + `S(S-pan-cmds)`.
- Then Ours KDE: activating F dispatches exactly one `setActive` and the
  adapter keeps project floats keep-above; relative F/G order is host
  stacking and TBD. Lower has no path (unfloat only restores the prior
  state). `S(S-ours-focus)` + `S(S-ours-fltstack)`; order queued, lower is
  a qualified no-counterpart leg.
- Then Ours Windows: focusing F actuates focus (`actuate_focus` /
  `SetForegroundWindow` path) while admitted floats sit in the topmost
  band with order by placement/activation; relative order TBD. Lower has
  no path (unfloat only restores the preimage). `S(S-ours-focus)` +
  `S(S-ours-fltstack)`; order queued, lower is a qualified
  no-counterpart leg.
- Variant hook: provisional/TBD (no suitable existing hook; V-FLOAT-SNAP
  covers moves, not stacking).

### R-FLT-13: ordinary float across a workspace switch

- Given (tree profiles): `WS1=H[A,B]` plus ordinary `F* (500,300,400,300)`;
  WS2 occupied by one ordinary tile. Same identities throughout; no
  sticky flag (sticky on/off stays R-FLT-02).
- Given (column profiles): `WS1=COL[C1[A],C2[B]]` plus ordinary
  `F* (500,300,400,300)`; WS2 with one occupied column at shipped
  defaults; viewport recorded. niri uses per-workspace strip columns;
  PaperWM uses a dialog/transient float (the only per-space float
  counterpart); paneru uses an `Unmanaged::Floating` window on one
  virtual strip with a second populated strip as WS2 (native macOS Space
  switches are host-owned, not this leg).
- When: select WS2; select WS1. Native verbs per profile: COSMIC workspace
  switch; Hyprland `workspace`; bspwm `desktop -f`; i3 `workspace`;
  xmonad `view`; sway `workspace`; qtile `Group.toscreen()` (screen
  switch; `togroup` is the window-send verb and never substitutes);
  awesome `view_only`; niri workspace switch; PaperWM space select;
  karousel desktop switch (KWin-native); paneru `VirtualNumber` to the
  populated strip (no auto-create); Ours Windows `WorkspaceOp` Select;
  Ours KDE native desktop select.
- Observe: ordinary float hidden vs sticky-like visibility; retained
  frame/z-order/focus on return.
- Then COSMIC: F hidden while away (ordinary floats live in the
  per-workspace floating layer; only the sticky layer is output-wide);
  frame retained (last-geometry reuse, no switch write); focus returns to
  F (workspace focus-stack top); z-order follows the retained mapped
  order. `S(S-cos-sticky)` + `S(S-cos-flttoggle)` + `S(S-cos-raise)`.
- Then Hyprland/Dwindle: F hidden while away (windows belong to
  workspace spaces; switch shows the target space). Frame and return
  focus TBD: focus depends on the unspecified pointer under shipped
  `follow_mouse=1` (`getFocusCandidate` restores only under
  `follow_mouse=0`). `S(S-hyp-ws)` + `S(S-hyp-float)`; frame/focus queued.
- Then bspwm: F hidden while away (`show_desktop`/`hide_desktop` per
  desktop; floats stay in the desktop tree); frame retained (client
  rectangle kept; vacant in place); focus returns to F (remembered
  desktop focus `d->focus` with history fallback).
  `S(S-bsp-ws)` + `S(S-bsp-float)` + `S(S-bsp-close)`.
- Then i3: F hidden while away (per-workspace floating list; switch shows
  the target workspace); frame retained (stored wrapper geometry, no
  switch write); focus returns to F (`workspace_show` focuses the
  descended remembered focus). `S(S-i3-flt-toggle)` + `S(S-i3-ws)`.
- Then xmonad/Tall+Navigation2D: F hidden while away (refresh draws only
  floats that are members of the visible workspace index); frame retained (global map untouched by
  `view`); focus returns to F (`W.peek` of the restored stack plus
  `setTopFocus`). `S(S-xmo-switch)`.
- Then sway: F hidden while away (per-workspace floating list; switch
  shows the target workspace); frame retained (stored frame, no switch
  write); focus returns to F (switch focuses the seat focus-inactive
  node, i.e. MRU F). `S(S-sway-float)` + `S(S-sway-switch)`.
- Then qtile/Columns: F hidden while away (`set_screen(None)` hides every
  group window); frame retained (placed floats keep x/y/w/h through
  `configure` plus `unhide`); focus returns to F (`layout_all` focuses
  `current_window`, set when F was focused). `S(S-qti-flt13)`.
- Then awesome/tile: F hidden while away (tag membership; only sticky
  reads on every selected tag); frame retained (floating arrange is a
  no-op); focus returns to F (MRU history top with visible fallback on
  tag switch). `S(S-awe-sticky)` + `S(S-awe-float)` + `S(S-awe-hist)`.
- Then niri: F hidden while away (each workspace owns its floating
  space; the switch only flips the active index and never writes tiles,
  positions, order, or per-space active state); frame and z-order retained
  for the same reason; on return focus follows the restored per-space
  active window back to F (floating-active flag plus floating active id
  retained; keyboard focus derives from the layout active window).
  `S(S-nir-ws)` + `S(S-nir-fltanchor)`.
- Then PaperWM: F stays on WS1 (the switch moves the tiled `getWindows()`
  only; floats are never moved or re-parented) with its frame retained
  (floats are never placed by the column layout). Hidden/shown across the
  switch rides host workspace membership, untraced at pin: visibility TBD
  (host). On return the switch accept falls back to the retained tiled
  `selectedWindow` and activates it; focus returns to the tile, not F.
  Float z-order is unwritten by the switch: TBD (host). `S(S-pap-float)`
  + `S(S-pap-wssel)` + `S(S-pap-layout)`; visibility queued (host).
- Then karousel/Lazy: F hidden while away (KWin desktops own windows; Floating subscribes to tile/frameGeometry only, so a float never transfers grids on the switch that only re-arranges); frame retained (no float-frame write on the switch path; keepAbove written only at toggle time, off by default); return focuses F via the native switch MRU chain (shipped ClickToFocus is reasonable with `NextFocusPrefersMouse` false, so no mouse contest; WS1 MRU is F). `S(S-kar-ws)` + `S(S-kar-float)` + `S(S-kar-fltanchor)` + `S(S-kwin-switch)`.
- Then paneru: F stays visible while away (virtual-row switches park
  strips only; the off-strip float keeps its native-space membership, so
  no hide runs); frame retained (no switch-time float write). Z-order TBD
  (host; return raises the restored managed window with its strip). On return focus goes to the strip's restored managed
  window, not F (only strip members can hold the remembered restore
  focus). `S(S-pan-flt)` + `S(S-pan-ws)`.
- Then Ours KDE: visibility, frame/order and return focus TBD (adapter
  writes desktop membership, but the native select journey is untraced).
  `S(S-ours-ws)`; native journey queued.
- Then Ours Windows: F hidden while away (leaving members hide,
  including retained floats; return reveals); focus returns to F
  (`focus_target` prefers `last_focus` when still a member and visible);
  frame TBD (reveal path untraced). `S(S-ours-fltsel)`; frame queued.
- Variant hook: provisional/TBD (R-FLT-02 covers sticky switch, not
  ordinary visibility).

### R-FLT-14: drag and resize a float by pointer

- Given (tree profiles): `H[A,B]` plus ordinary `F* (500,300,400,300)`.
  Ordinary resizable float with no constraining hints, no rules, scale 1,
  zero gaps for reference geometry. Record actual frame and work area
  before acting; both legs run away from work-area edges and sibling
  frames.
- Given (column profiles): one strip/workspace with tiled columns plus
  ordinary `F* (500,300,400,300)` at shipped defaults; viewport recorded.
  niri floats live in the per-workspace floating space; PaperWM uses a
  dialog float; karousel uses a `Floating`-state client; paneru uses an
  `Unmanaged::Floating` window.
- When: leg 1 drag F by (+100,+50) via the profile pointer move producer
  below; release. Leg 2 resize F bottom-right by (+100,+50) via the
  profile pointer resize producer; release. Semantic snap stays
  R-FLT-10/11 and never substitutes; a missing tiler verb never denies
  the host journey.
- Observe: free frame retention vs snap/clamp/tiling; layer, sibling
  stability and focus.
- Then COSMIC: leg 1 Super+Left or titlebar press opens a move grab and
  the floating drop retains F in the floating layer at the translated
  frame; leg 2 edge press opens a floating resize grab growing the frame.
  Focus follows the press. Siblings untouched (floating layer).
  `S(S-cos-dragstart)` + `S(S-cos-fltptr)`.
- Then Hyprland/Dwindle: producer `movewindow` (mouse bind) for leg 1 and
  the resize mouse bind for leg 2. Drag writes the translated position;
  bottom-right resize grows size clamped only by min/max hints (none
  constraining here); shipped snap defaults off, so no snap engages.
  Press raises and focuses F. Tiling untouched.
  `S(S-hyp-fltdrag)`.
- Then bspwm: producer modifier+button pointer grab (`ACTION_MOVE` /
  `ACTION_RESIZE_CORNER`, bottom-right the default handle). Drag writes
  `floating_rectangle` x/y (+100/+50); resize grows w/h (+100/+50) with
  hints applied (non-constraining here) and writes the rectangle. Focus
  retained (no defocus path); tiling untouched (float branch never
  swaps). `S(S-bsp-drag)` + `S(S-bsp-fltptr)`.
- Then i3: producers floating-modifier+left (or titlebar left) for leg 1
  and floating-modifier+right (or border/decoration right, bottom-right
  corner for leg 2) for leg 2. Drag translates the frame; resize grows
  it; both raise F (already top with a single float). Click focuses F.
  Tiling untouched. `S(S-i3-tdrag)` + `S(S-i3-fltdrag)`.
- Then xmonad/Tall+Navigation2D: drag writes the raw frame plus `float`
  on motion and release with no clamp/zone check; resize resizes via
  size hints plus `float` on motion/release. `S(S-xmo-mouse)`.
- Then sway: producers mod+left (or titlebar left) for leg 1 and border
  left (or mod+resize, bottom-right quadrant resolves RIGHT|BOTTOM) for
  leg 2. Move writes pending x/y; resize writes size (client hints
  enforced on floating resize only, non-constraining here); both raise F
  and the press focuses it. Tiling untouched. `S(S-sway-fltptr)`.
- Then qtile/Columns: producers `Mod+Button1` (`set_position_floating`
  frame tweak) and `Mod+Button3` resize. Drag translates the frame;
  resize grows it (ordinary resizable, no constraining hints); placed
  floats keep geometry through configure. Focus retained (shipped
  `bring_front_click` false; no defocus path). Tiling untouched.
  `S(S-qti-drag)` + `S(S-qti-tweak)`.
- Then awesome/tile: producers modkey+Button1 (or titlebar) move and
  modkey+Button3 (or border) resize. Floating frame write path (vs tiled
  layout-resize dispatch); press focuses with raise. Frames retained
  free; tiling untouched. `S(S-awe-drag)`.
- Then niri: producers Mod+Left (activates, raising F, plus move grab)
  and Mod+Right on the edge (activates plus resize grab; floats skip the
  double-click gesture). Drag translates freely; resize grows freely
  (no snap for floats). Focus and front raised via activation. Columns
  untouched. `S(S-nir-ptr)`.
- Then PaperWM: TBD (no float pointer-move/resize path in PaperWM;
  dialog frames are Meta-owned; host GNOME grab journey untraced).
  `S(S-pap-float)`; host journey queued.
- Then karousel/Lazy: no script pointer path either leg (Floating subscribes to tile/frameGeometry only; the Tiled interactive-move/resize handlers never engage a Floating client; no pointer verb in the Actions inventory), so tiles are untouched and the script writes no focus; the host `moveResize` translates F freely with the fixture run away from edges and sibling frames (so no snap zone engages) and the press retains the already-focused F. `S(S-kar-float)` + `S(S-kar-ptr)` + `S(S-kar-acts)` + `S(S-kwin-moveresize)`.
- Then paneru: TBD (no pointer path in `Operation`; macOS host drag is
  the applicable journey, untraced). `S(S-pan-flt)`; host journey queued.
- Then Ours KDE: project route refused (`NotTiled` for float resize
  proposals, keyboard and pointer alike; adapter only gates
  fullscreen/maximize); host KWin interactive move/resize is the
  applicable journey, untraced. `S(S-ours-fltrefuse)`; host journey queued.
- Then Ours Windows: project route refused (shared-Engine `NotTiled`;
  gesture maps to `CoreCommand::PointerResize` but the float proposal
  refuses); host Win32 move/size is the applicable journey, untraced.
  `S(S-ours-winbind)` + `S(S-ours-fltrefuse)`; host journey queued.
- Variant hook: provisional/TBD (semantic snap stays R-FLT-10/11).
