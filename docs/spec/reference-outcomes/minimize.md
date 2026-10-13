# Minimize (native icon-minimize) (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there.

R-MIN-01..03 are size constraints, not icon-minimize coverage. A hide
verb alone never proves minimize: only a sourced client/host iconify
request path (or its evidenced absence) votes here.

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Givens are separate fixtures, never H/V ancestry
claims. Ours KDE and Ours Windows cite separate adapter keys.

### R-MNZ-01: native minimize of a middle tile

- Given (tree profiles): `H[A,B,C]`, history A,C,B (B focused).
  Ordinary windows, no rules.
- Given (column profiles): `COL[C1[A],C2[B*],C3[C]]`, shipped defaults.
  Same history.
- Given (layout-driven projections): xmonad `[A,B*,C]` master/stack;
  qtile three single-window columns; awesome `[A,B*,C]` master/stack.
- When: native minimize B (client/host iconify request; per-profile
  path in each Then).
- Observe: tiling allocation (leaves vs stays) plus restore path; refocus.
- Then COSMIC: B unmaps from the tiling tree (siblings reflow) into
  `MinimizedWindow::Tiling` restore state; focus falls to MRU
  unminimized (C). `S(S-cos-minimize)` + `S(S-cos-focusfix)`.
- Then Hyprland/Dwindle: no-counterpart (native minimize requests are
  consumed and dropped with no tiling effect: the sole `stateRequest`
  listener `onUpdateState` handles fullscreen/maximize only and no layout
  path consults minimized, so B stays tiled; the X11 flag echo is
  client-only; no minimize verb in the action/dispatcher inventory).
  `S(S-hyp-mininv)`.
- Then bspwm: no-counterpart (no `WM_CHANGE_STATE`/iconic request
  branch and no minimize verb; `hidden` is WM-owned hide, never a
  minimize vote). `S(S-bsp-mininv)`.
- Then i3: no-counterpart (iconic request rejected and reverted to
  normal; B stays tiled). Scratchpad is separate. `S(S-i3-mininv)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (no minimize verb in the
  core key inventory and no iconify request branch anywhere in the profile
  event chain: core `ClientMessage` handling only restarts or broadcasts,
  EWMH handles close/desktop/active only, fullscreen handling is
  fullscreen-atom-only, and Navigation2D carries no event hook; the
  internal hide/reveal primitives never vote per the header, contrib
  minimize actions/layout are out-of-profile, and the startup iconified
  scan is admission-only). `S(S-xmo-mininv)`.
- Then sway: no-counterpart (the focused minimize request is cleared to
  unminimized and the container never detaches: the sole minimize listener
  only echoes the xwayland flag when unfocused, activation clears it, and
  neither visibility nor arrange consults the flag; B stays tiled; no
  minimize verb in the command table, scratchpad separate).
  `S(S-sway-mininv)`.
- Then qtile/Columns: B leaves the Columns allocation into the floating layout via mark_floating and hides as MINIMIZED/IconicState under shipped auto_minimize (X11 iconic request) and the Wayland minimize request path; survivors refill through the column drop with no group refocus (mark_floating focuses the floating layout when current), so group current stays B; `S(S-qti-minimize)` + `S(S-qti-minfocus)`
- Then awesome/tile: B is banned (excluded) from the arrangement
  (`ICONIC`) keeping client order; HIDDEN request maps to the same
  setter. Refocus follows history (C). `S(S-awe-minimize)` +
  `S(S-awe-hist)`.
- Then niri: no-counterpart (`SetMinimized` is an explicit no-op; no
  Action verb). `S(S-nir-mininv)`.
- Then PaperWM: B leaves the space tiling when `stick()` emits workspace
  `window-removed` into `remove_handler` and `space.removeWindow` (column
  spliced with empty-column drop and space relayout plus topmost-neighbor
  selection); the tiled mark is kept (`_tiled_on_minimize`); sibling
  columns keep their widths (per-column layout, no rescale). GNOME focus
  pick TBD (host). `S(S-pap-minimize)` + `S(S-pap-layout)`; focus queued
  (host).
- Then karousel/Lazy: B enters `TiledMinimized` (B is the sole window of
  C2, so its removal empties and destroys C2 via `onColumnRemoved`); focus
  passes Immediate (B is the last-focused client) to the left column A
  (left-else-right `columnToFocus` via `getWindowToFocus`); survivors keep
  widths (reposition only, so no reflow). `S(S-kar-minimize)`.
- Then paneru: B is marked `Unmanaged::Minimized` (off screen per the
  on-screen check), leaving the strip with its index remembered as
  `PreviousManagedStrip` while survivors keep their widths (no
  cross-column rescale); refocus runs nearest-center `give_away_focus`
  on the active strip, exact target TBD (F: missing frame/display
  geometry). `S(S-pan-minimize)` + `S(S-pan-stripwidth)`.
- Then Ours KDE: delivered offline: B releases allocation, siblings reflow, MRU C focus, session-only restore context; explicit `closed_windows` clears closed slots through core/protocol/KDE, including hidden domains ([rework](../../changes/archive/d23-rework.md)). Native release/frames/focus/close ordering TBD. `S(S-ours-minkde)`.
- Then Ours Windows: adapter delivered offline 2026-10-13: known minimized rows omit, allocation releases/reflows with MRU and session-only MinimizedSlot. Verified `closed_windows` clears closed slots including automatic hidden-domain fanout; unreadable/rejected observations retain reports ([record](../../changes/archive/windows-d23-restore-close.md)). Former retained-allocation divergence removed. Native frames/focus/close ordering TBD. `S(S-ours-minwin)`.
- Variant hook: provisional/TBD (minimize-slot hook, to discuss).

### R-MNZ-02: native restore of the minimized tile

- Given: the actual minimized state from R-MNZ-01 (same identities).
- When: native restore (unminimize) B.
- Observe: slot restoration; focus.
- Then COSMIC: B remaps from the stored tiling state (old slot via
  `remap` with the saved state; `was_maximized` re-overlays when set);
  restore performs no focus write (tiling branch returns no focus target
  and the caller sets none), so the pre-restore MRU focus C stays valid
  and B returns unfocused. `S(S-cos-minimize)` + `S(S-cos-focusfix)`.
- Then Hyprland/Dwindle: no-counterpart (nothing minimized to restore;
  backend `setMinimized` is echo-only/no-op and no unminimize path exists
  in the pinned inventory). `S(S-hyp-mininv)`.
- Then bspwm: no-counterpart (same missing request path and verb).
  `S(S-bsp-mininv)`.
- Then i3: no-counterpart (nothing minimized to restore).
  `S(S-i3-mininv)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (nothing minimized to
  restore; same missing verb and request path as R-MNZ-01).
  `S(S-xmo-mininv)`.
- Then sway: no-counterpart (nothing minimized to restore; no unminimize
  verb, scratchpad separate). `S(S-sway-mininv)`.
- Then qtile/Columns: restore via minimized=false into floating=false re-admits B at the focused cc position per add_client (shipped insert_position 0), not the old slot; the path focuses B through the layout when B is current; `S(S-qti-minimize)` + `S(S-qti-minfocus)`
- Then awesome/tile: B remaps `NORMAL` at its retained client order
   with tile recalc via arrange; the delayed history check no-ops while
   the MNZ-01 refocus C stays visible-focused, so focus stays C.
   `S(S-awe-minimize)` + `S(S-awe-hist)`.
- Then niri: no-counterpart. `S(S-nir-mininv)`.
- Then PaperWM: marked B leaves the scratch layer via `unmakeScratch`
  (scratch frame saved, float cleared, `unstick`) into workspace
  `window-added` handling, which re-inserts after the then-selected window
  per `getOpenWindowPositionIndex` (shipped RIGHT default), not the old
  slot; focus is requested for B on the active space via the
  existing-window branch. The live selection follows the host focus pick
  from R-MNZ-01, so the exact slot rides host state: slot TBD (host);
  native focus delivery TBD (host). `S(S-pap-minimize)` + `S(S-pap-ins)`;
  slot queued (host).
- Then karousel/Lazy: unminimize constructs a new `Tiled` in a fresh column
  after the last-focused column (C1 once MNZ-01 refocused A, so between A
  and C), not the old slot, with width from B's preferredWidth clamped
  into [min,max]; the re-tile passes no focus and host unminimize issues
  no activation (`setMinimized` emits only), so KWin focus stays A. `S(S-kar-minimize)` +
  `S(S-kar-ins)` + `S(S-kar-min)` + `S(S-kwin-min)`.
- Then paneru: the `Minimized` mark is removed and
  `window_managed_trigger` reinserts at the remembered
  `PreviousManagedStrip` index (old slot) when that workspace/virtual-strip
  still exists and no `index` insertion rule overrides (none under shipped
  defaults); otherwise the active-strip overlap/end fallback applies. The
  restore issues no focus write, so focus stays at the R-MNZ-01 target,
  exact identity TBD with it (F: same missing geometry).
  `S(S-pan-minimize)`.
- Then Ours KDE: delivered offline under User 2026-10-13 option (b): best-effort rejoin beside former neighbor, same side/group/index where possible, comparable old share; exact equality is only a fast path. Resize/new windows/other-sibling closes/swaps do not invalidate alone; session-only, no restore focus write. COSMIC `3d55cba0` source gate passed ([rework](../../changes/archive/d23-rework.md)). Native slot/share/focus TBD. `S(S-ours-minkde)`.
- Then Ours Windows: adapter delivered offline 2026-10-13: tiled restore uses shared best-effort neighborhood rejoin; actual resize/swap, admission, sibling-close, bounds/gaps and interleaved-minimize fixtures pass. Observed focus naming restored B keeps prior Engine MRU; reconcile writes no focus. Session-only, native slot/share/focus TBD ([record](../../changes/archive/windows-d23-restore-close.md)). `S(S-ours-minwin)`.
- Variant hook: provisional/TBD (restore-slot hook, to discuss).

### R-MNZ-03: native minimize of a sole workspace window

- Given: `WS1=H[A*]` shown, `WS2` occupied. Columns: `WS1` holds one
  column; paneru uses virtual rows (A on VW1, C on VW2).
- When: native minimize A on WS1 (same paths as R-MNZ-01).
- Observe: workspace occupancy/cleanup; focus fallback.
- Then COSMIC: A is stored in `minimized_windows` as tiling restore
  state with the tiling layer emptied; the workspace is retained, not
  cleaned up (`is_empty` is false while any minimized entry exists, and
  only trailing-empty handling applies). Keyboard focus resolves to none:
  the minimized filter leaves no unminimized MRU target and no mapped or
  fullscreen fallback exists. `S(S-cos-minimize)` + `S(S-cos-focusfix)`.
- Then Hyprland/Dwindle: no-counterpart (request dropped as in R-MNZ-01;
  WS1 never empties this way). `S(S-hyp-mininv)`.
- Then bspwm: no-counterpart (same missing request path and verb).
  `S(S-bsp-mininv)`.
- Then i3: no-counterpart (request rejected; WS1 never empties this
  way). `S(S-i3-mininv)`.
- Then xmonad/Tall+Navigation2D: no-counterpart (same missing request
  path as R-MNZ-01; the sole workspace never empties this way).
  `S(S-xmo-mininv)`.
- Then sway: no-counterpart (focused request cleared as in R-MNZ-01; WS1
  never empties this way). `S(S-sway-mininv)`.
- Then qtile/Columns: A hides as MINIMIZED with group membership retained (hide only, no workspace cleanup verb in the cited path); no group refocus runs (layout focus retained when current), so current stays A; `S(S-qti-minimize)` + `S(S-qti-minfocus)`
- Then awesome/tile: A unmaps (`ICONIC`) with client order, tags and
   screen kept (the setter touches none); the workspace/tag is retained
   with no cleanup path; the delayed history check finds no visible
   candidate and writes no focus, so focus is none.
   `S(S-awe-minimize)` + `S(S-awe-hist)`.
- Then niri: no-counterpart. `S(S-nir-mininv)`.
- Then PaperWM: same stick-to-scratch path as R-MNZ-01; sole-space
  empty-column splice plus GNOME cleanup TBD. `S(S-pap-minimize)`; queued.
- Then karousel/Lazy: same `TiledMinimized` path as R-MNZ-01 (A is the
  sole window of the sole column C1, so C1 is destroyed; being the last
  column, the focus target is null, so the script writes no focus and the
  grid sits empty with the desktop object retained: no desktop-removal
  path in the traced source, and desktops are explicit-only with no empty
  auto-removal). Host minimize issues no refocus, so KWin active stays the
  minimized A with no fallback. `S(S-kar-minimize)` + `S(S-kar-ws)` + `S(S-kwin-min)` + `S(S-kwin-desktops)`.
- Then paneru: same Minimized-mark plus strip-remove path as R-MNZ-01;
  the emptied sole row is retained (virtual reaping spares index 0, and
  no activation runs a reap here). `give_away_focus` finds no neighbour
  so paneru issues no refocus; which window the host focuses instead is
  TBD (host). `S(S-pan-minimize)` + `S(S-pan-ws)`; focus queued (H:
  opaque host minimize-focus journey).
- Then Ours KDE: delivered offline: workspace retained, focus none, stored restore slot via shared core plus KDE sole-minimize null-focus wiring; shared-core-first target met ([decision](../../decisions.md#window-state-float-sticky-maximize-fullscreen)). Exact native occupancy/focus TBD user-owned. `S(S-ours-minkde)`.
- Then Ours Windows: adapter delivered offline 2026-10-13: sole-minimize observation empty, slot-aware Engine session retained and focus none; verified sole-close report forgets slot before session retirement. Native workspace/focus journey TBD ([record](../../changes/archive/windows-d23-restore-close.md)). `S(S-ours-minwin)`.
- Variant hook: provisional/TBD (minimized-occupancy hook, to discuss).

#### D23 restore and close-cleanup discriminators (2026-10-13)

Each restore leg starts fresh from `H[A,B*,C]` with unequal shares; minimize B,
perform one intervening action, then restore B. These are variants of existing
R-MNZ-01..03, not additional numbered scenarios. COSMIC `3d55cba0` source gate
passed: stored parent-or-sibling/orientation/index/sizes, surviving-anchor
rejoin, otherwise fresh admission (`src/shell/layout/tiling/mod.rs:343-349,
458-545,1329-1346`). The exact per-action COSMIC frames and native/reference
outcomes below remain TBD; general source behavior is not native evidence.
Shared-core fixtures: `crates/tiler-core/tests/minimize_restore.rs`;
production-entry fixtures: `kwin/tests/minimize-close-cleanup-entry.test.ts`;
[offline evidence](../../changes/archive/d23-rework.md).

Windows adapter boundary evidence 2026-10-13: actual resize/swap, focus naming
the restored member, newcomer/sibling close/interleaving, bounds/gaps,
restart/fallback, hidden iconic refresh and automatic hidden-domain close drain
pass ([record](../../changes/archive/windows-d23-restore-close.md)). Source-slot
lifetime after send elsewhere is separately TBD in the
[minimal discriminator](../reference-outcomes.md#d23-best-effort-restore-and-close-discriminators).

| Intervening action | Selected Ours target | Delivered offline regression |
| --- | --- | --- |
| Focus A instead of C | Rejoin old neighborhood; no OmniTiler restore focus write | Shared focus-change fixture rejoins exactly; focus stays A |
| Resize A/C shares | Best-effort rejoin, comparable old B share | Shared resize fixture retains current survivor proportions and stored regroup hints |
| Open D | Rejoin beside former neighbor, preserve side/group/index best-effort | Shared and real-entry fixtures place B beside former neighbor rather than fresh beside D |
| Close A, retain C | Rejoin by surviving former neighbor C | Shared close fixture places B before C on stored side |
| Swap A/C | Best-effort neighborhood rejoin, no invalidation solely from swap | Shared swap fixture regroups B with former neighbor C |
| Minimize/restore another sibling | Best-effort rejoin when former neighbor survives | Shared interleaved roundtrip fixture restores neighborhood with no focus write |
| Change gaps/resolution | Rejoin using current projection with comparable old share | Shared bounds/gap fixture retains stored neighborhood and current domain geometry |
| Restart owner | Session-only slot expires; fresh ordinary admission | Fresh generation has no stored slot |
| Close all former neighbors, or remove domain/send B elsewhere | Fresh ordinary admission | Separate shared fixtures cover all three fallback cases |
| Close B while minimized | Clear stored slot, distinguish close from minimize via small protocol addition | Core/protocol/real-entry cleanup fixtures; sole slot-only session retires; rejected report stays pending |
| Resize survivors to small integer weights with a skewed old B share | Comparable old fraction, survivor ratios retained | Share fixture: old B 1/10, current survivors `[1,1]` -> inserted `[2,9,9]`, not `[1,1,1]` |
| Admit D nested under a surviving former group, then restore B | Same former group/index where possible | Shared surviving-group fixture inserts at stored index without flattening D's group |
