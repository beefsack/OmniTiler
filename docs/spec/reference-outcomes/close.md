# Close / reflow (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

## 7. Close / reflow

<a id="scrolling-backfill-additive-existing-wide-tables-above-unchanged"></a>
<a id="r-close-01-backfill-close-the-middle-tile-scrolling"></a>
### R-CLOSE-01: close the middle tile

- Given (tree profiles): `H[A,B,C]`; focus A,C,B so B is active and C is next MRU

- Given (column profiles): `COL[C1[A],C2[B*],C3[C]]`, each 0.5W; VP recorded;
  history A,C,B. Same identities and action as the original row.

- When: Close B.

- When (column leg): close B with the profile's verb from the inventory below.

- Observe: Collapse + focus selection (MRU vs spatial)

- Observe (column leg): survivor order and focus selection (the original discriminator).

- Then COSMIC: Survivors `[A,C]` keep order, proportional rescale; focus C (MRU top via fixup); `S(S-cos-rem)` + `S(S-cos-focusfix)`
- Then Hyprland/Dwindle: Flat 3-child start has no ordinary binary form (default ratio 1 yields halves, not thirds); exact N-ary collapse TBD. Policy: live-tree removal promotes the sibling and recalcs; closed-focused refocus defaults to spatial `next` (closest node by old middle, else first/back fallback), not MRU C; cursor/MRU modes only via explicit `focus_on_close=1/2`; `S(S-hyp-close)`
- Then bspwm: Survivors `[A,C]` keep order with sibling promotion + arrange (either binary embedding of the flat triple converges here: the middle leaf's brother promotes into its place); focus C (history MRU via the focus guess, coinciding with MRU here, not a spatial rule); close asks the client (delete/kill), removal unlinks + drops history; the unrecorded embedding is immaterial to this collapse/focus Observe. `S(S-bsp-close)`
- Then i3: Survivors `[A,C]` keep nodes order with percent rescale; focus C (second in the focus stack via `con_next_focused`, coinciding with MRU here, not a spatial rule); `S(S-i3-close)`
- Then xmonad/Tall+Navigation2D: Exact flat 3-child H has no Tall counterpart (flat N-ary H vs master/stack two-pane); analogous policy only: close removes B via `delete` (`sink` + `delete'`/`filter`, order preserved, focus down else up, so C under the projected `[A,B*,C]` order; positional, not MRU); survivors `[A,C]` keep stack order and refill via unconditional Tall recalc; `S(S-xmo-close)` + `S(S-xmo-layout)`
- Then sway: Survivors `[A,C]` keep order with fraction renormalize; focus C (focus-inactive view of the parent in MRU order, coinciding with MRU here, not a spatial rule); unmap detaches + reaps + rearranges; `S(S-sway-close)`
- Then qtile/Columns: Flat 3-child start has no ordinary Columns form (default num_columns=2, third window stacks in-column); exact collapse/focus TBD. Policy: tiled close unlinks with sibling promotion, drops emptied columns with width-share redistribute, and refocuses positionally via the layout return (shipped focus_previous_on_window_remove=false, so no MRU previous_win); `S(S-qti-close)`
- Then awesome/tile: Flat 3-child start has no ordinary tile form (master plus one vertical stack column, not flat thirds); exact frames TBD. Policy: survivors `[A,C]` keep order and refill via stateless tile recalc (no old-slot store); focus C via history MRU (unmanage deletes B, delayed refocus takes top visible non-sticky history, else sticky fallback, else first visible); `S(S-awe-hist)` + `S(S-awe-tile)`
- Then niri: columns A,C keep order and widths; B's column removed;
  C's column activates (clamped next), focus C. `S(S-nir-close)`.
- Then PaperWM: columns A,C keep order; selection C (topmost
  neighbor: last-activated of A,C). `S(S-pap-close)`.
- Then karousel/Lazy: B's column destroyed; A,C keep order and widths;
  focus A (left neighbor). `S(S-kar-close)`.
- Then paneru: entity despawned and stripped with order preserved;
  focus via nearest-center (exact target TBD without geometry: F -
  fixture gives 0.5W columns with VP recorded but no viewport
  origin/column centers selecting the A-vs-C pick).
  `S(S-pan-close)`; queued.
- Then Ours KDE: Leaf removed, C selected as source-MRU top; `D(D-dec-cos)`
- Then Ours Windows: Leaf removed, C selected as source-MRU top; `D(D-dec-cos)`
- Variant hook: V-CLOSE-FOCUS.

<a id="r-close-02-backfill-close-refocus-fresh-reopen-scrolling"></a>
### R-CLOSE-02: close, refocus, fresh reopen

- Given (tree profiles): `H[A,B,C]` manual 50/30/20; focus A,C,B

- Given (column profiles): three single-window columns with manual widths
  0.5/0.3/0.2W, prepared per model (niri `SetColumnWidth`, PaperWM
  `resizeW` 10% grid at zero gaps/margins, karousel host interactive
  resize, paneru `SetWidth` exact ratios); history A,C,B. Column models have
  no tree-ratio rescale: survivor columns keep independent widths.
  Same identities and action as the original row: close B; focus C;
  open a new B with same app/rules.

- When: Close B; focus C; open a new B with same app/rules (verbs per the inventory below).

- When (column leg): close B with the profile's verb; focus C; open a new B.

- Observe: Survivor rescale; fresh admission vs old ratio/slot; reopened focus

- Observe (column leg): survivor widths; fresh admission vs old slot; reopened
  focus.

- Then COSMIC: Survivors rescale proportionally (ratio preserved); reopened B is fresh admission at C (after C, old slot not restored); focus newcomer; `S(S-cos-rem)` + `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)` + `S(S-cos-mapfocus)`
- Then Hyprland/Dwindle: Flat 3-child start has no ordinary binary form; exact survivor widths and reopened frames TBD. Policy: close promotes sibling + recalc (ratios live on ancestors, no old-slot store); reopened B fresh-admits via the Dwindle anchor with newcomer focus, before/after by pointer half; `S(S-hyp-close)` + `S(S-hyp-ins)` + `S(S-hyp-newfocus)`
- Then bspwm: Survivors refill via sibling promotion + arrange (ratios live on ancestors, no old-slot store; exact 50/30/20 widths TBD without the binary embedding); reopened B fresh-inserts at the desktop focus after C with newcomer focus; `S(S-bsp-close)` + `S(S-bsp-insert)`
- Then i3: Survivors rescale proportionally via percent fix; reopened B fresh-admits after focused C with newcomer focus, no old-slot store; `S(S-i3-close)` + `S(S-i3-ins)`
- Then xmonad/Tall+Navigation2D: Exact 50/30/20 flat-H fixture has no Tall counterpart (Tall splits master/stack at `frac`, stack splits equally; per-slot ratios live nowhere - shares derive from `frac`/`nmaster` only); analogous policy only: survivors refill via removal + Tall recalc to `frac` shares with no old-slot store; reopened B fresh-admits via `insertUp` above focused C with newcomer focus; exact survivor widths/reopened pixel frames TBD (F: work-area geometry unrecorded, so Tall shares have no pixel basis); `S(S-xmo-close)` + `S(S-xmo-ins)` + `S(S-xmo-layout)`; queued.
- Then sway: Survivors rescale proportionally via fraction renormalize; reopened B fresh-admits after focused C via the focus-inactive anchor with newcomer focus, no old-slot store; `S(S-sway-close)` + `S(S-sway-ins)`
- Then qtile/Columns: Same non-ordinary start; exact survivor widths and reopened frames TBD. Policy: close redistributes the removed height/width share across survivors (no old-slot store); reopened B fresh-admits at the focused position with newcomer focus; `S(S-qti-close)` + `S(S-qti-add)`
- Then awesome/tile: Same non-ordinary start; survivors refill via tile recalc with no old-slot/ratio store (shares live in master/stack plus windowfact, not per-slot); reopened B is fresh manage admission appending last with newcomer focus; exact survivor widths/reopened frames TBD; `S(S-awe-tile)` + `S(S-awe-manage)` + `S(S-awe-hist)`
- Then niri: survivors keep 0.5/0.2W with no rescale; reopened B is a
  fresh column after the active C at the default width (no old-slot store)
  and takes focus under Smart (no pending fullscreen to fence it).
  `S(S-nir-close)` + `S(S-nir-ins)`.
- Then PaperWM: 50/30/20 prepared via the `resizeW` grid; survivors
  keep frame-widths (layout reads live frames); reopened B is fresh
  at selected+1 RIGHT with activate-on-show (no old-slot store).
  `S(S-pap-close)` + `S(S-pap-ins)` + `S(S-pap-resize)`.
- Then karousel/Lazy: close runs the host KWin path (no close verb in the Actions inventory); B's removal destroys C2 with the last-focused fixup falling to C1, survivors keep their manually resized widths (reposition only, no old-slot store). Reopened B is a new column after the last-focused column at open time (C3 once C is focused, so at the end) with width from its preferredWidth clamped into [min,max]; reopened focus TBD (fixture states no protocol selecting the X11-manage vs Wayland-add fork). `S(S-kar-manual-width)` + `S(S-kar-close)` + `S(S-kar-ins)` + `S(S-kar-min)` + `S(S-kar-fltanchor)` + `S(S-kwin-manage)` + `S(S-kwin-add)`; focus TBD (F: missing protocol selecting the newcomer activation fork).
- Then paneru: exact ratios prepared via `SetWidth`; survivors keep
  per-window ratios; reopened B is fresh at rule-index/after-focus/end
  (no custom rules under shipped defaults, so no rule index; C focused
  last so after-focus appends); policy synthesizes `WindowFocused` for
  the newcomer, but reopened focus TBD (host remainder: physical
  acceptance rides the guarded host `WindowFocused` follow path with
  frontmost/app-reported guards). `S(S-pan-close)` + `S(S-pan-ins)` +
  `S(S-pan-fresh)` + `S(S-pan-focusobs)` + `S(S-pan-setwidth)`; queued.
- Then Ours KDE: reopened B is fresh admission at focused C (after C, old slot not restored) with newcomer focus; survivor rescale TBD (manual-ratio leg unproven here). `S(S-ours-close)` + `S(S-ours-ins)` + `S(S-ours-admit)`; proven offline for fresh admission/order/desired focus only (`session_float_focus_admission` shared fixture); native journey pending (live TBD).
- Then Ours Windows: same fresh-admission leg as Ours KDE via the shared Engine; survivor rescale likewise TBD. `S(S-ours-close)` + `S(S-ours-ins)` + `S(S-ours-admit)`; proven offline for fresh admission/order/desired focus only (shared fixture); native journey pending (live TBD).
- Variant hook: V-CLOSE-FOCUS.


## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours cells cite Engine + adapter source at `9241c94`
plus the new `S(S-ours-close)` model key; selected intent and doc
assertions are never evidence.

Close verbs (shared inventory for R-CLOSE-03/04/05; each When below
closes the responsive client via its native graceful close route, no
forced kill): COSMIC `Close`; Hyprland `killWindow`; bspwm
`node -c|--close` (client delete request with kill fallback);
i3 `kill [window|client]`; xmonad `kill`; sway `kill`; qtile
`lazy.window.kill()`; awesome `c:kill()`; niri `CloseWindow`; PaperWM
`close-window`; karousel host KWin close (no karousel verb in
`Actions.ts`); paneru host macOS close (no close `Operation`); Ours
native close is the host action (KWin user close; Windows WM_CLOSE/app
close), converged by the Engine via observation - `SessionCommand::Remove`
is the observation-driven proposal, never the native verb.
`S(S-close-verbs)`.

### R-CLOSE-03: close the sole window on the shown workspace

- Given (tree profiles): `WS1=H[A*]` shown, WS2 occupied. Ordinary
  window, no rules, scale 1, zero gaps for reference geometry.
- Given (column profiles): `WS1=COL[C1[A*]]` sole column, WS2
  occupied; shipped defaults apply; viewport recorded.
- Given (paneru): `Space1:{VW1(row0)=COL[C1[A*]]}` sole row-0 virtual
  row, second native Space occupied; row 0 is never reaped, so the
  retention condition is exact.
- When: close A with the profile's verb from the inventory above.
- Observe: empty shown workspace retained vs view changed/removed;
  focus none vs another domain.
- Then COSMIC: WS1 retained as shown empty (only non-active non-last
  empties are removed; trailing empty ensured); focus none (fixup finds
  no mapped target). `S(S-cos-rem)` + `S(S-cos-send)` +
  `S(S-cos-focusfix)`.
- Then Hyprland/Dwindle: A removed (sole node erased; no recalc on
  the last-node path, recalc only with sibling promotion);
  WS1 retained as shown empty (empty-close runs only for special
  workspaces; no ordinary empty-destroy path); focus none (no tiled
  closest/first candidate and empty tiled/floating fallbacks, so the
  empty-workspace refocus finds no visible target).
  `S(S-hyp-close)`.
- Then bspwm: desktop retained as shown empty (desktops removed only by
  explicit `desktop -r`); removal unlinks with sibling promotion and
  drops history; focus none (sole removal clears the desktop focus, and
  the shown-desktop guess finds no surviving focusable leaf, clearing X
  input to root). `S(S-bsp-close)` + `S(S-bsp-wsretain)`.
- Then i3: WS1 retained (only invisible empty workspaces auto-close);
  A detached; focus falls to the empty workspace (no window focus).
  `S(S-i3-close)` + `S(S-i3-wsretain)`.
- Then xmonad/Tall+Navigation2D: workspace retained with the stack emptied to `Nothing` (static workspace zipper; `filter` on the sole window yields `Nothing` while the workspace entry persists); A removed via `delete` through `unmanage`; refresh ends in `setTopFocus`, and `peek` on the empty stack is `Nothing`, so X focus falls to root (focus none). `S(S-xmo-close)` + `S(S-xmo-ws)` + `S(S-xmo-topfocus)`.
- Then sway: WS1 retained (`consider_destroy` spares the output-active
  workspace); focus falls to the empty workspace. `S(S-sway-close)` +
  `S(S-sway-wsretain)`.
- Then qtile/Columns: group retained as shown empty (static groups);
  focus none (no next focus clears). `S(S-qti-close)` +
  `S(S-qti-wsdef)`.
- Then awesome/tile: tag retained as shown empty (static tags);
  unmanage deletes history; refocus finds no visible client, focus
  none. `S(S-awe-hist)` + `S(S-awe-tile)`.
- Then niri: WS1 retained (cleanup spares the active workspace); sole
  column removed; focus none. `S(S-nir-close)`.
- Then PaperWM: space retained as shown empty (column spliced, space
  object persists); selection none (no neighbors). `S(S-pap-close)`.
- Then karousel/Lazy: last column destroyed, grid empty, desktop
  retained; focus none (no column to focus). `S(S-kar-close)`.
- Then paneru: row 0 retained (orphan reaping spares it); entity
  despawned and stripped; WM writes no focus (no neighbor for
  `give_away_focus`); observed focus TBD (host remainder: host decides
  focus after close - whether another app/Space or none - and paneru
  follows only if the host reports `WindowFocused` via the guarded
  follow path; second Space occupied as context, not a proven handoff).
  `S(S-pan-close)` + `S(S-pan-focusobs)`; queued.
- Then Ours KDE: Engine collapses the domain to empty with desired
  focus none; adapter retires applied scope at the remove-empty
  boundary; native focus journey TBD. `S(S-ours-close)`; queued.
- Then Ours Windows: same Engine collapse and desired focus none via
  the shared Engine; native journey TBD. `S(S-ours-close)`; queued.
- Variant hook: provisional/TBD (sole-close retention hook, to discuss).

### R-CLOSE-04: close a focused ordinary float over tiles

- Given (tree profiles): `H[A,B]` plus ordinary `F* (500,300,400,300)`;
  history A,B,F. Ordinary windows, no rules, scale 1.
- Given (column profiles): two single-window columns A,B plus ordinary
  `F*` at the same frame; same history; shipped defaults apply.
- When: close F with the profile's verb from the inventory above.
- Observe: float removal leaves tiles untouched vs reflow; last tiled
  MRU vs other focus fallback.
- Then COSMIC: F removed from the floating layer (floats unmap outside
  the tiling tree, so tiles keep allocations); focus B via MRU fixup.
  `S(S-cos-flttoggle)` + `S(S-cos-focusfix)`.
- Then Hyprland/Dwindle: F removed from the floating set only (float
  data erased, tiled nodes untouched with no tiled recalc); focus none
  (the floating/history-reverse branch has no closing-window exclusion
  so it returns closing F itself, and focusing that unmapped candidate
  clears to none). `S(S-hyp-close)` + `S(S-hyp-float)`.
- Then bspwm: F unlinked with no tiling-space effect (floats use none);
  focus B via history MRU guess. `S(S-bsp-float)` + `S(S-bsp-close)`.
- Then i3: floating wrapper detached with tiling percents untouched;
  focus B (focus-stack next). `S(S-i3-close)`.
- Then xmonad/Tall+Navigation2D: `delete` sinks the float and drops it
  from the stack; the Tall arrange input excludes floating-map members, so A/B tile allocations are unaffected (recalc runs over the unchanged tiled set); stack order preserved; exact focus TBD (F: positional down-else-up needs F's stack slot, and the fixture states no admission order placing F in the stack). `S(S-xmo-close)` + `S(S-xmo-arrange)`; queued.
- Then sway: F detached from the floating list with tiling fractions
  untouched; focus B (focus-inactive tiling view). `S(S-sway-close)`.
- Then qtile/Columns: F removed from the group outside Columns;
  survivor widths stable; refocus B via the floating branch.
  `S(S-qti-close)`.
- Then awesome/tile: unmanage deletes history; top visible B
  refocused; tiles recalc without F (floats excluded). `S(S-awe-hist)`
  + `S(S-awe-tile)`.
- Then niri: floating tile removed, floating deactivates with scrolling
  untouched; focus returns to B's column. `S(S-nir-close)`.
- Then PaperWM: `removeFloating` splices `_floating` with tiled columns
  untouched; shell focus fallback TBD. `S(S-pap-close)`; queued.
- Then karousel/Lazy: floats live outside grid columns so tiles are untouched; `Floating.destroy` ignores the close passFocus, so the script writes no focus on the float-close path (the tiled anchor stays at B's column); the host close runs `activateNextWindow` into the MRU focus-chain usable pick (history A,B,F minus removed F, so B) through the reasonable-policy `requestFocus` gates. `S(S-kar-float)` + `S(S-kar-close)` + `S(S-kar-fltanchor)` + `S(S-kwin-close)` + `S(S-kwin-scriptact)`.
- Then paneru: entity despawned and stripped with A/B columns
  untouched; exact focus TBD (nearest-center, geometry-dependent: F -
  fixture gives no column centers/viewport origin selecting the A-vs-B
  pick). `S(S-pan-close)`; queued.
- Then Ours KDE: Engine drops the float exception with the tree
  untouched and preserves B focus; adapter native journey TBD.
  `S(S-ours-close)` + `S(S-ours-flt-target)`; queued.
- Then Ours Windows: same Engine exception-drop and preserved focus
  via the shared Engine; native journey TBD. `S(S-ours-close)` +
  `S(S-ours-flt-target)`; queued.
- Variant hook: provisional/TBD (float-close hook, to discuss).

### R-CLOSE-05: close a maximized or fullscreen window

- Given (tree profiles): `H[A,B*]`; prepare `B:max`; fresh variant
  `B:full`. Record actual pre-action topology per profile.
- Given (column profiles): `COL[C1[A],C2[B*]]`; same two preparations;
  record applicability.
- When: close B with the profile's verb from the inventory above. No
  native restore action is available after destruction.
- Observe: overlay cleanup, remaining tile allocation and focus.
- Then COSMIC: max leg first clears the overlay via `unmaximize_request`
  (floating unmap plus tiling recalc), then removes B with single-child
  group flatten (A orphaned to root; `remove_window` proportional
  redistribution applies only to len>2) and A refills full width.
  Full leg already
  unmapped B into a `Fullscreen` target during preparation (A already
  full), so close drops the fullscreen surface with its saved tiling
  restore discarded and the tree keeps A full. Both legs focus A via MRU
  fixup (B dropped from focus sets). Exact pixel frames TBD (F: output
  and work-area geometry unrecorded). `S(S-cos-rem)` + `S(S-cos-maxpolicy)` +
  `S(S-cos-fsreq)` + `S(S-cos-fsrestore)` + `S(S-cos-focusfix)`.
- Then Hyprland/Dwindle: unmap removes the layout target and group
  membership, refocusing since B was focused; max/full mode dies with
  B; A refills; focus A. `S(S-hyp-close)` + `S(S-hyp-fs)`.
- Then bspwm: max leg no-counterpart (no maximize state); full leg:
  node removed with sibling promotion, A refills, focus A via history
  guess. `S(S-bsp-fs)` + `S(S-bsp-close)`.
- Then i3: max leg no-counterpart (no maximize verb); full leg: mode
  flag dies with the con, tree detaches with percent fix, A refills,
  focus A. `S(S-i3-max)` + `S(S-i3-close)`.
- Then xmonad/Tall+Navigation2D: max leg no-counterpart (flat Tall/Mirror/Full only, no maximize state); full leg applicable (float-based EWMH fullscreen, `RationalRect 0 0 1 1` via the ClientMessage event path; admission itself tiles): `kill` destroys B and `unmanage`/`delete` removes it from stack and float map with no restore state stored; A refills full via single-window Tall and focus is A (sole survivor, direction-independent). `S(S-xmo-layout)` + `S(S-xmo-close)` + `S(S-xmo-ewmh)`.
- Then sway: max leg no-counterpart (no maximize verb); full leg
  applicable but `ws->fullscreen` clearing on destroy untraced.
  `S(S-sway-max)` + `S(S-sway-close)` + `S(S-sway-full)`; queued.
- Then qtile/Columns: maximized/fullscreen are float-layer states
  dying with B; group removal refocuses; A refills; focus A.
  `S(S-qti-fs)` + `S(S-qti-close)`.
- Then awesome/tile: boolean max/full die with the client; unmanage
  deletes history; A refocused and refills. `S(S-awe-fs)` +
  `S(S-awe-hist)` + `S(S-awe-tile)`.
- Then niri: `remove_tile` path regardless of flags; the
  maximized/fullscreen flag dies with B so no restore exists; surviving
  column A keeps its independent width (no refill rescale); focus A.
  `S(S-nir-close)` + `S(S-nir-maxfs)`.
- Then PaperWM: `removeWindow` path regardless of state (no
  maximized/fullscreen branch; width conversion moot after destruction):
  the max leg closes the width-maximized tile like an ordinary tile and
  the full leg closes with the flag dying with B; column spliced with
  empty-column drop and relayout, surviving column A keeping its width,
  selection to the topmost neighbor. Shell focus fallback TBD (host).
  `S(S-pap-close)` + `S(S-pap-layout)` + `S(S-pap-widthmax)`; focus queued
  (host).
- Then karousel/Lazy: column removal path with left-focus; the window
  state dies with B so no restore exists; surviving column A keeps its
  width; A focused as sole survivor. `S(S-kar-close)`.
- Then paneru: max leg owner-specific (host zoom journey TBD); full
  leg applicable (fullscreen strip marker) but close-from-fullscreen
  journey untraced. `S(S-pan-axfs)` + `S(S-pan-fsfocus)`; queued.
- Then Ours KDE: Engine removes the tile with desired focus A;
  portables keep overlays tiled so no restore exists; native overlay
  cleanup TBD. `S(S-ours-close)`; selected removal plus survivor refill/focus
  (User 2026-10-12 D01 / CLOSE-05 (a), [decision](../../decisions.md#placement-minimums-and-startup-adoption)); queued.
- Then Ours Windows: same Engine removal and desired focus A; native
  cleanup TBD. `S(S-ours-close)`; same CLOSE-05 selection, queued.
- Variant hook: provisional/TBD (overlay-close hook, to discuss).
