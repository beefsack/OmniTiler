# Special windows (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there.

Candidates: R-SPC-01 through R-SPC-05 (transients/modals, splash/utility types, PiP, fixed-size admission, app-owned resize/hint changes). No existing special rows to backfill (0 existing rows, 0 cells). PiP is app/flags-specific with no universal native type; no profile invents one.

Q2 discriminators R-SPC-06..13 cover fixed-size edge cases. D1-D8 are
user-selected NORMATIVE (User 2026-10-08; REQ-SPC-04a..h); D1 setting delivered
offline, D5/D6 delivered offline; D7 remains implementation pending in Ours KDE;
[offline delivery and review record](../../changes/archive/fixed-size-admission.md).

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours KDE cells cite the KDE observer gate
(`S(S-ours-spc-kde)`); Ours Windows cells cite the Windows candidate
gates (`S(S-ours-spc-win)`); the shared Engine carries no
type distinction and is never delivery evidence here. Selected intent
and doc assertions are never evidence.

R-SPC-01 fixture roles: D is a typed-dialog transient for A (both
flags set). The fresh modal-D leg keeps transient-for and adds the
modal flag. Profiles with no modal branch report the identical outcome
explicitly; the flag is never silently dropped. A normal-type
transient (no dialog flag) is a different fixture and is not claimed.

### R-SPC-01: open a transient dialog, then request parent focus; fresh modal variant

- Given (tree profiles): `H[A*]`, A parent-eligible; typed-dialog
  transient D for A not yet mapped. Modal-D leg adds the modal flag.
- Given (column profiles): `COL[C1[A*]]` sole column; same D legs.
  Shipped defaults apply; viewport recorded.
- When: open D transient for A; request focus on A.
- Observe: transient float vs tile, parent-relative placement, and
  modal focus fence; dialog and modal flags recorded separately.
- Then COSMIC: D floats (Wayland parent branch; X11 dialog-or-modal
  branch). Placement is cascade, not parent-relative (arrival with no position reuses last geometry else spawn_order cascade); newcomer takes focus on the active workspace, and a later parent focus request succeeds with no modal fence (modal only feeds the X11 dialog branch; no fence branch in focus validity). Dialog vs modal recorded separately: Wayland modal has no branch, X11 modal floats via the same dialog branch. `S(S-cos-admit)` + `S(S-cos-floatpos)` + `S(S-cos-mapfocus)` + `S(S-cos-modal)`.
- Then Hyprland/Dwindle: D floats (X11 dialog/transient/modal/parent/fixed-size suggest float; Wayland parent or either-dim fixed-size suggests float with the modal flag recorded separately but not floating on its own). Placement centers in the work area, not parent-relative (X11 requested geometry or rule position excepted). Newcomer D takes focus (DIALOG keeps initial focus; no-focus rule/silent/grab excepted). A later parent-focus request succeeds with no fence for non-modal D, while a Wayland modal child fences parent focus under shipped `modal_parent_blocking=true` (X11 has no modal fence). `S(S-hyp-spc)` + `S(S-hyp-newfocus)`.
- Then bspwm: D floats centered (transient rule plus dialog-type
  center); no modal mechanism exists so no fence.
  `S(S-bsp-spc)`.
- Then i3: D floats (transient-for plus dialog-type branches; modal adds no branch beyond the same float admission, so the modal leg matches). Frame TBD (F: D request geometry / leader rect unstated, selecting stored-geometry per `floating.c:330` vs parent-relative leader-centered per `:372-378` vs workspace-centered fallback; output containment per `:387-403` unrecorded). A later parent-focus request succeeds with no modal fence. `S(S-i3-min)` + `S(S-i3-flt-toggle)`; queued (placement leg).
- Then xmonad/Tall+Navigation2D: D floats at its managed native
  geometry with newcomer focus; the modal flag is inert (no branch),
  so the modal leg matches. `S(S-xmo-float)`.
- Then sway: xdg D floats (parent branch; modal adds no branch); xwayland D floats
  (modal/dialog branch); modal leg matches. Placement centers on the
  workspace/output (default half-width/three-quarter-height centered frame,
  not parent-relative); newcomer D takes focus on the active workspace; a later
  parent-focus request succeeds with no modal fence.
  `S(S-sway-spc)` + `S(S-sway-float)` + `S(S-sway-ins)`.
- Then qtile/Columns: D floats (dialog type matches shipped rules)
  centered on A (transient placement) with stealable focus; the
  modal flag is inert (no branch). `S(S-qti-spc)`.
- Then awesome/tile: D floats (dialog type is implicitly floating),
  admitted on A's screen and tags with raise; the modal flag is
  inert. `S(S-awe-manage)` + `S(S-awe-float)`.
- Then niri: D floats alongside the parent (`compute_open_floating`:
  explicit rule, parent, or fixed positive height min==max; the
  boolean feeds `add_window`); Smart focus applies and the tiler
  imposes no modal fence. `S(S-nir-spc)` + `S(S-nir-maxfs)`.
- Then PaperWM: D floats above (`add_filter` rejects transients;
  `make_above` so it is not hidden); takes focus. Wayland fence
  enforcement TBD. `S(S-pap-spc)`; queued.
- Then karousel/Lazy: D floats in both legs (transient fails `shouldTile`;
  modal fails it too); the transient link is tracked via `findTransientFor` without changing state. Admission constructs Floating with no height cap or placement write, so the native frame is retained; the script writes no focus and imposes no modal fence (no branch). D is not special (`isSpecialWindow` excludes Dialog) and takes tab focus (`wantsTabFocus` covers Dialog given `wantsInput`), so the generic X11 manage fork applies (non-special branch with tab-focus eligibility); the Wayland leg follows the add fork. Exact focus TBD (fixture: activation timestamp/token selecting the fork unstated).
  `S(S-kar-spc)` + `S(S-kar-float)` + `S(S-kwin-manage)` + `S(S-kwin-add)`.
- Then paneru: role-gated management with no transient/parent/modal branch: a D reporting the standard subrole (or AXWindow plus floating subrole) is managed as ordinary through the fresh path (rule float check, rule insertion index else after-focus else append, `dont_focus` keep else synthesized `WindowFocused`); a D reporting any other dialog/transient subrole is skipped unless a title/bundle rule forces management, spawning no entity and taking no focus write. The modal flag is inert in both legs; no parent-relative placement or modal fence exists. `S(S-pan-spc)` + `S(S-pan-admit)` + `S(S-pan-fresh)`.
- Then Ours KDE: TBD (observer gates on KWin `normalWindow`; the
  dialog-type eligibility mapping is untraced with no pinned KWin
  source in-repo, so no admission outcome is claimed).
  `S(S-ours-spc-kde)`; queued.
- Then Ours Windows: D is excluded from tile targets (owner gate
  plus dialog-class gate). Host placement and modal focus fence TBD;
  exclusion does not establish that native journey. `S(S-ours-spc-win)`;
  queued.
- Variant hook: provisional/TBD (dialog-float hook, to discuss).

### R-SPC-02: open a typed splash window; fresh utility variant

- Given (tree profiles): `H[A*]`, no custom rules. U typed splash;
  fresh leg U typed utility. Backend/protocol legs split only where
  the profile has two backends.
- Given (column profiles): `COL[C1[A*]]` sole column; same U legs.
- Given (niri): X11 U through working xwayland-satellite; native xdg
  splash/utility types have no counterpart.
- Given (Ours Windows): utility U uses `WS_EX_TOOLWINDOW`; record the
  standalone splash's actual class, owner and extended styles.
- When: open U.
- Observe: excluded/unmanaged vs floating/tiled, focus steal, and
  task-switcher presence. All three legs are load-bearing; a cell
  missing any applicable leg stays P.
- Then COSMIC: X11 splash floats while X11 utility tiles (Utility
  excluded from `is_dialog`); Wayland-native typed legs are
  fixture-inapplicable (xdg has no splash/utility type counterpart).
  Newcomer takes focus on the active workspace; the external switcher lists every tracked toplevel with no workspace/visibility filter and activation forwards to the toplevel, so admitted splash/utility are listed when tracked. `S(S-cos-admit)` + `S(S-cos-mapfocus)` + `S(S-lch-altab)` + `S(S-pop-toplevel)` + `S(S-cos-topact)`.
- Then Hyprland/Dwindle: X11 splash and utility both float (SPLASH and UTILITY atoms) with neither taking initial focus (non-DIALOG float atoms suggest no initial focus); Wayland-native typed legs are fixture-inapplicable (no splash/utility type counterpart; float only via parent or fixed size). Switcher leg is a no-counterpart (no native Alt+Tab/listing verb in the inventory; cycle is same-workspace and the shipped launcher is external). `S(S-hyp-spc)` + `S(S-hyp-switcher)`.
- Then bspwm: Splash tiles as ordinary with newcomer focus (no splash branch in the type/transient/fixed legs); utility tiles without focus (`focus=false`); switcher leg no-counterpart (no native switcher/listing verb in this profile, so presence never runs); `S(S-bsp-spc)` + `S(S-bsp-switcher)` + `S(S-bsp-insert)`.
- Then i3: both float (DIALOG/UTILITY/TOOLBAR/NOTIFICATION/SPLASH
  plus MODAL branch) with visible-workspace newcomer focus.
  Switcher presence TBD. `S(S-i3-min)`; queued.
- Then xmonad/Tall+Navigation2D: both tile as ordinary newcomers
  with focus (no type branch; only fixed/transient float); switcher leg no-counterpart (no native Alt+Tab/cross-workspace listing verb in this profile: core mod+Tab is same-stack `focusDown`, `dmenu_run`/`gmrun` are external launchers with no traced switcher activation, and Navigation2D stays on the same layer, so presence never runs). `S(S-xmo-float)` + `S(S-xmo-switcher)`.
- Then sway: xwayland U floats (DIALOG/UTILITY/TOOLBAR/SPLASH and
  modal branches); xdg typed legs are fixture-inapplicable (xdg has
  no window-type counterpart; only parent/fixed-size float). Focus
  and switcher TBD. `S(S-sway-spc)`; queued.
- Then qtile/Columns: X11/XWayland U legs both float (utility/splash/dialog in shipped
  rules; unplaced floats center) with stealable focus on X11 (notification-only
  exclusion, so splash/utility take focus); Wayland-native xdg U legs are
  fixture-inapplicable (xdg returns normal/dialog only, no splash/utility counterpart).
  Switcher leg is a no-counterpart
  (no native Alt+Tab/cross-group listing verb in the inventory; bar widgets
  show the current group only, toscreen pulls with no listing).
  `S(S-qti-spc)` + `S(S-qti-switcher)`.
- Then awesome/tile: both implicitly float (non-normal type) admitted
   on the selected tags with rule placement; splash takes no focus
   (splash fails the global rule's focus filter, so no newcomer
   activate) while utility takes newcomer focus via the shipped global
   rule; both appear in the programmatic `menu.clients` list (unfiltered
   iterate) while the tasklist shows current tags only and no shipped key
   lists clients (`Mod+Tab` is `history.previous`).
   `S(S-awe-float)` + `S(S-awe-manage)` + `S(S-awe-focus)` + `S(S-awe-switcher)`.
- Then niri: parentless resizable splash/utility tiles as ordinary
  columns (the open float predicate is exhaustive over explicit rule,
  parent, and fixed height; `Match` carries no window-type field, so the
  bare type cannot float them); Smart activation applies, so the newcomer
  takes focus with keyboard following the layout active window; admitted
  U is listed in the MRU UI, which iterates every workspace's
  scrolling-plus-floating windows with no type filter. `S(S-nir-spc)` +
  `S(S-nir-ins)` + `S(S-nir-fltanchor)` + `S(S-nir-mru)`.
- Then PaperWM: both float (`add_filter` admits Normal only) with
  `make_above`. The rejected path shows without extension activation, so
  focus follows host activation policy: focus TBD (host: GNOME activation
  policy for shown floats untraced). Switcher presence follows host
  `NORMAL_ALL` type membership for splash/utility: TBD (host: Mutter
  tab-list type filter untraced). `S(S-pap-spc)` + `S(S-pap-fltanchor)` +
  `S(S-pap-switcher)`; focus/switcher queued (host).
- Then karousel/Lazy: X11 splash and utility both float (managed types whose `windowType` is not Normal, so `shouldTile` fails the `normalWindow` gate and `addClient` constructs Floating with the native frame retained and no script focus write); Wayland-native typed legs are fixture-inapplicable (xdg/PlasmaShell roles carry no splash/utility counterpart, mapping only Desktop/Dock/OSD/Notification/Tooltip/Critical/AppletPopup/Normal). Splash is special, so it skips both the X11 manage focus branch (`requestFocus`) and the attention branch (`demandAttention` requires non-special): no steal, no attention. Utility is not special, so the generic fork applies: no `requestFocus` (`wantsTabFocus` requires Normal/Dialog/AppletPopup) with `demandAttention` on the deny branch. Both are excluded from the host switcher list (`wantsTabFocus` false for Splash/Utility). Exact utility attention TBD (fixture: activation timestamp selecting the allow/deny fork unstated). `S(S-kar-spc)` + `S(S-kar-float)` + `S(S-kwin-manage)` + `S(S-kwin-add)` + `S(S-kwin-tabbox)`.
- Then paneru: role-gated management with no splash/utility branch: a U
  reporting the standard subrole (or AXWindow plus floating subrole) is managed as ordinary through the fresh path in both legs; any other typed subrole is skipped unless a title/bundle rule forces management. Skipped windows spawn no entity and take no focus write; paneru offers no switcher entry (no cross-strip listing op; listing and activation are host-owned). `S(S-pan-spc)` + `S(S-pan-admit)` + `S(S-pan-fresh)` + `S(S-pan-switcher)`.
- Then Ours KDE: TBD (same `normalWindow` type-eligibility gap as
  R-SPC-01). `S(S-ours-spc-kde)`; queued.
- Then Ours Windows: utility is toolwindow-excluded from tile
  targets; standalone splash eligibility TBD.
  `S(S-ours-spc-win)`; queued.
- Variant hook: provisional/TBD (window-type hook, to discuss).

### R-SPC-03: enter app picture-in-picture mode

- Given (all profiles): `H[A*]` (tree) or `COL[C1[A*]]` (columns);
  playing media app. Record toolkit, app, version, actual flags and
  rules before acting.
- When: enter the app's PiP mode.
- Observe: separate float/topmost vs ordinary tile; app-rule vs
  native window-type distinction. No universal PiP type is assumed.
- Then COSMIC: TBD (no PiP branch traced; app/flags-specific).
  Queued.
- Then Hyprland/Dwindle: TBD (no PiP branch traced). Queued.
- Then bspwm: TBD (no PiP branch traced). Queued.
- Then i3: TBD (no PiP branch traced). Queued.
- Then xmonad/Tall+Navigation2D: TBD (no PiP branch traced). Queued.
- Then sway: TBD (no PiP branch traced). Queued.
- Then qtile/Columns: TBD (no PiP branch traced; float rules are
  type/class/transient, not PiP). Queued.
- Then awesome/tile: TBD (no PiP branch traced). Queued.
- Then niri: shipped Firefox PiP app-id/title rule floats
  (`open-floating true`); every other app TBD (no native PiP type).
  `S(S-nir-spc)`; queued.
- Then PaperWM: no PiP branch exists in `add_filter`, `insertWindow`, or the registered action inventory, so an ordinary Normal non-transient client would tile as a fresh column at the open-position index with widths per column layout - but the scenario's required pre-act record (toolkit, app, version, actual flags and rules) is omitted, so whether this app's PiP client satisfies that antecedent is unestablished: admission TBD (fixture: record toolkit, app, version, actual flags and rules before acting). App-specific stay-on-top beyond the tiled branch's `unmake_above` TBD (host/live: app above-flag behavior untraced). `S(S-pap-spc)` + `S(S-pap-ins)` + `S(S-pap-layout)` + `S(S-pap-acts)`; admission queued (fixture), stay-on-top queued (host).
- Then karousel/Lazy: TBD (no PiP branch traced). Queued.
- Then paneru: TBD (no PiP branch traced). Queued.
- Then Ours KDE: TBD (no PiP type or rule in the observer or the
  Engine). `S(S-ours-spc-kde)`; queued.
- Then Ours Windows: TBD (owned/tool/dialog gates do not name PiP;
  app-specific eligibility untraced). `S(S-ours-spc-win)`; queued.
- Variant hook: provisional/TBD (PiP hook, to discuss; app-specific).

### R-SPC-04: open a fixed-size window (min=max 640x480)

- Given (tree profiles): `H[A*]`, no custom rules. E fixed at
  min=max 640x480, resizable otherwise ordinary.
- Given (column profiles): `COL[C1[A*]]`; same E.
- When: open E.
- Observe: fixed-size admission exception (float) vs forced tile,
  and any size-only clamp. Exact frames and focus are not
  load-bearing for this predicate.
- Then COSMIC: E floats (min==max is `is_dialog`).
  `S(S-cos-admit)`.
- Then Hyprland/Dwindle: E floats (FIXED_SIZE hints branch).
  `S(S-hyp-spc)`.
- Then bspwm: E floats (min==max hints rule). `S(S-bsp-spc)`.
- Then i3: E floats (min==max branch). `S(S-i3-min)`.
- Then xmonad/Tall+Navigation2D: E floats. `S(S-xmo-float)`.
- Then sway: E floats (xdg either-dimension min==max; xwayland
  fixed branch). `S(S-sway-spc)`.
- Then qtile/Columns: E floats (`has_fixed_size` in shipped rules).
  `S(S-qti-spc)`.
- Then awesome/tile: E implicitly floats (`is_fixed`).
  `S(S-awe-float)`.
- Then niri: E floats (fixed positive height min==max under
  `compute_open_floating`). `S(S-nir-spc)`.
- Then PaperWM: E tiles (Normal type passes `add_filter`; no
  fixed-size exclusion). `S(S-pap-spc)`.
- Then karousel/Lazy: E floats as untileable under the shapeability
  gate (both-axes min==max keeps KWin `isResizable` false under either-axis strict inequality on both backends, given an otherwise moveable ordinary client, so `canTileEver` fails and `addClient` constructs Floating).
  `S(S-kar-spc)` + `S(S-kwin-resizeable)`.
- Then paneru: no fixed-size admission counterpart (role-gated admission with no size predicate; float is rule-assigned); E follows the ordinary role-gated path with no fixed-float leg. `S(S-pan-admit)`.
- Then Ours KDE: E is an automatic float outside the tile tree. No E
  geometry, focus, stacking or keep-above writes from classification;
  exact native/game journey TBD. Implemented offline 2026-10-08 under
  NORMATIVE D1-D8 (User 2026-10-08; D1/D5/D6 delivered offline,
  D7 implementation pending); [real Planner fixtures](../../../kwin/tests/fixed-size-admission.test.ts),
  [record](../../changes/archive/fixed-size-admission.md).
- Then Ours Windows: E floats automatically outside the tile tree with no
  admitted-client geometry/focus/stacking/keep-above writes. Windows adapter
  and retained Engine fixtures delivered offline 2026-10-11, prerequisite
  `a41fcb5` plus runtime delivery; native hints/game journey TBD.
  `S(S-ours-spc-win)`; [record](../../changes/archive/windows-fixed-size-admission.md).
- Variant hook: NORMATIVE D1/D8 (User 2026-10-08; delivered offline).
  KDE `fixedSizePredicate=both-axes-fixed` default (`Width and height both fixed`,
  tooltip COSMIC) or `either-axis-fixed` (`Width or height fixed`, tooltip
  Hyprland (Wayland), sway); both float this fixture. Windows
  `core.fixed_size_predicate`; no migration.

### R-SPC-05: app-owned resize and minimum-hint change on a tile

- Given (tree profiles): `H[A,B*]`, B initially resizable, no
  limiting rules. Fresh run: B raises its minimum hints above its
  allocation instead of requesting a size.
- Given (column profiles): `COL[C1[A],C2[B*]]`; same two legs.
- When: B requests 900x700 (fresh run: B changes minimum hints
  above allocation).
- Observe: app-owned resize vs tile-authoritative allocation, and
  reactive clamp/reflow/float vs ignored request/hint. Client-side
  acknowledgement chatter is not load-bearing beyond the reactive
  outcome.
- Then COSMIC: allocation stays tile-authoritative (tiling without
  minimum enforcement; the compositor drives configure serials).
  `S(S-cos-min)`.
- Then Hyprland/Dwindle: allocation stays authoritative (tiled size
  limits default off; tiled X11 configure requests are refused via
  an authoritative size resend). `S(S-hyp-min)` + `S(S-hyp-cfg)`.
- Then bspwm: request ignored (`honor_size_hints=false` default);
  allocation retained. `S(S-bsp-hint)`.
- Then i3: request ignored (tiled render ignores size hints).
  Allocation retained. `S(S-i3-min)`.
- Then xmonad/Tall+Navigation2D: request has no path (Tall recalc
  owns geometry). Allocation retained. `S(S-xmo-layout)`.
- Then sway: request ignored (arrange ignores hints; the runtime
  hint path handles urgency only). Allocation retained.
  `S(S-sway-min)`.
- Then qtile/Columns: request ignored (tiled place without hints).
  Allocation retained. `S(S-qti-min)`.
- Then awesome/tile: request ignored while B stays resizable
  (tiled geometry early-return refuses the client resize; hints
  update emits `property::size_hints`; only the implicit float
  updater watches it and arrange listens for `size_hints_honor`, so
  no immediate reflow). Later arrange hint-shaping is the qualifier.
  `S(S-awe-hint)` + `S(S-awe-tile)`.
- Then niri: a tiled B's app-owned resize or minimum-hint raise re-runs
  the column width resolve with min/max clamp on the window update path;
  neighbors shift position with widths kept (no rescale); neither leg
  re-floats (open-only classification; the toggle is a plain tile move)
  and no focus is written. `S(S-nir-min)` + `S(S-nir-fixed-open)` +
  `S(S-nir-flttoggle)`.
- Then PaperWM: allocation policy tile-authoritative. A tiled B's app-owned resize or minimum-hint raise has no hint branch (no minima consult); `size-changed` queues a column relayout which recomputes targets from preferredWidth/client frame (not guaranteed to overwrite a preferredWidth the client keeps asserting) with no re-float. Recomputed targets settle natively X11-sync/Wayland-async (post-request re-read, actuals feed layout): exact native frames TBD (live: client settle timing). `S(S-pap-appresize)` + `S(S-pap-layout)`; frames queued (live).
- Then karousel/Lazy: app-owned geometry on tiled B feeds the column width path (`frameGeometryChanged` outside interactive resize re-asserts via rate-limited `onFrameGeometryChanged` into `setWidth` clamped into [min,max] and stored as preferred, with relayout and no re-float); a bare minimum-hint raise with no geometry change has no path (only `captionChanged` re-evaluates tiling for caption-follow rules; no size-hint watcher). Exact native frames TBD (live: client settle timing). `S(S-kar-manual-width)` + `S(S-kar-min)` + `S(S-kar-admit)`.
- Then paneru: app-owned geometry is observed, not refused: the live frame is re-read into Bounds through the moved/resized update path (managed strips nudged, floating windows leave the strip alone, own-resize echoes skipped in flight) with no re-float or admission reclassification; a bare minimum-hint raise with no geometry change has no path (AX exposes no min/max hint equality; float is rule-assigned). `S(S-pan-admit)`.
- Then Ours KDE: Engine allocation stays authoritative (geometry
  applied from the tree). Host hint reaction TBD.
  `S(S-ours-spc-kde)` + `S(S-ours-admit)`; queued.
- Then Ours Windows: Engine allocation stays authoritative with
  declared hints carried per member (min-enforcing rows); exact
  reflow TBD. `S(S-ours-spc-win)`; queued.
- Variant hook: provisional/TBD (client-hint hook, to discuss).

## Q2 fixed-size admission discriminators (2026-10-08)

R-SPC-04's 640x480 fixture is reused for ordinary fixed admission. Its
focus/frame-neutral automatic admission is NORMATIVE D8 (User 2026-10-08);
COSMIC's floating placement/focus is not evidence for our Gaming Compatibility fence.
Native game geometry/focus/stacking must remain untouched by automatic
classification. Existing sticky origin/toggle rules reuse R-FLT-02/05 and
R-RST-01; transient/dialog eligibility reuses R-SPC-01, not new type gates.
All fresh variants below reset the client and WM state independently.

### R-SPC-06: admit a client fixed on only one axis

- Given (tree profiles): `H[A*]`; ordinary non-transient E has min
  640x480, max 640x960 (width fixed). Fresh variant: max 1280x480
  (height fixed). No rules, overlays or explicit float state.
- Given (column profiles): `COL[C1[A*]]`; same hint variants.
- When: open E.
- Observe: width-only/height-only float vs tile; no geometry claim.
- Then COSMIC: tiles in both variants; whole-size equality fails.
  `S(S-cos-fixed-hints)`.
- Then Hyprland/Dwindle: Wayland floats in both; X11 tiles in both.
  `S(S-hyp-fixed-hints)`.
- Then bspwm: tiles in both variants; `_apply_hints` requires
  min_width==max_width and min_height==max_height together, so one-axis
  equality fails. `S(S-bsp-spc)`.
- Then i3: tiles in both variants; the fixed branch requires max_width>0
  and max_height>0 with both axes equal, so one-axis equality fails.
  `S(S-i3-min)`.
- Then xmonad/Tall+Navigation2D: tiles in both variants; `isFixedSize`
  is whole-tuple `sh_min_size==sh_max_size`, so one-axis equality fails.
  `S(S-xmo-float)`.
- Then sway: xdg floats in both; Xwayland floats in both (both minima>0
  with either-axis equality). `S(S-sway-fixed-hints)` + `S(S-sway-spc)`.
- Then qtile/Columns: tiles in both variants; `has_fixed_size` requires
  both flags with 0<min==max on both axes, so one-axis equality fails.
  `S(S-qti-min)`.
- Then awesome/tile: neither variant satisfies implicit fixed-size floating;
  both axes must match. `S(S-awe-fixed-dynamic)`.
- Then niri: width-fixed tiles while height-fixed floats; the predicate is
  height-only (`min.h>0` and `min.h==max.h`), so only the height-fixed
  variant satisfies it. `S(S-nir-spc)`.
- Then PaperWM: tiles in both variants; `add_filter` admits Normal
  non-transient windows with no fixed-size exclusion. `S(S-pap-spc)`.
- Then karousel/Lazy: tiles in both variants; one-axis difference keeps
  KWin `isResizable` true under either-axis strict inequality, so the
  shapeability gate still tiles (given moveable ordinary clients).
  `S(S-kar-spc)` + `S(S-kwin-resizeable)`.
- Then paneru: no fixed-size admission counterpart (role-gated admission
  has no size-hint predicate; float is rule-assigned; AX exposes no
  min/max hint equality). `S(S-pan-admit)`.
- Then Ours KDE: `both-axes-fixed` default tiles in both variants;
  `either-axis-fixed` floats in both without target writes (NORMATIVE D1).
  Switch to either-axis after admitting tiled E: E remains tiled; a new
  otherwise identical F floats. Switching back leaves F floating; explicit
  user tile overrides still win for the live client. This setting-change
  leg has no reference-WM outcome claim. Delivered offline 2026-10-08;
  [predicate/Planner fixtures](../../../kwin/tests/fixed-size-admission.test.ts),
  [D1 record](../../changes/archive/admission-and-move-settings.md). Native TBD.
- Then Ours Windows: both variants tile under `both-axes-fixed` and float
  under `either-axis-fixed`; live changes affect subsequent admissions only,
  without rebuilding retained classifications. Offline 2026-10-11, prerequisite
  `a41fcb5` plus runtime delivery; native UI/hints TBD.
  [Record](../../changes/archive/windows-fixed-size-admission.md).
- Variant hook: NORMATIVE whole-size default or either-axis setting
  (User 2026-10-08); functional IDs delivered offline.

### R-SPC-07: admit with unset, zero or equal partial-zero hints

- Given (tree profiles): `H[A*]`; ordinary non-transient E. Independent
  hint variants: min/max absent; min=max (0,0); min=max (640,0);
  min=max (0,480); min=max (2147483647,2147483647), a KWin unbounded
  sentinel rather than a usable fixed size. Fresh guard variant: width
  min=max=640 but both height bounds absent. No rule/overlay/type exception.
- Given (column profiles): `COL[C1[A*]]`; same independent variants.
- When: open E with one hint variant.
- Observe: absence/zero/sentinel normalization vs raw equality.
- Then COSMIC: Wayland unset/(0,0) do not satisfy the fixed branch;
  both partial-zero variants, the sentinel and the width-640/absent-height
  guard do and float (guard height 0 travels in the (w,h) tuple, kept by the
  full-zero-only filter, equal on both axes). X11 absent does not match
  (flag unset yields None); present-zero, both partial-zero and sentinel
  float (flag set yields Some with whole-tuple equality) via the existing-pin
  chain: cosmic-comp `Cargo.lock` pins smithay e3d461a
  (`src/xwayland/xwm/surface.rs:1141-1175` getters, `:1648-1656` normal-hints
  update) and `x11rb` 0.13.2 (`src/properties.rs:257-259` option fields,
  `:348-349` flag-gated parse, `:667-678` `parse_with_flag`; checksum
  `9993aa5b` in lockfile, pins reused, no new revision). The X11
  absent-height guard encoding stays TBD (F: P_MIN_SIZE/P_MAX_SIZE govern the
  whole (w,h) tuple, so "width 640 plus height absent" has no distinct wire
  value in evidence; whether the harness sends (640,0) or omits the flag is
  unstated). `S(S-cos-fixed-hints)`; X11 guard queued (fixture; primary F).
- Then Hyprland/Dwindle: unset and zero/partial-zero variants do not
  satisfy the fixed branch on either backend; the sentinel floats on
  both (no sentinel branch; Wayland minima>1 with either-axis equality,
  X11 all>0 with both-axis equality); the height-absent guard tiles on
  both (Wayland min<=1 gate, X11 min>0 gate). `S(S-hyp-fixed-hints)`.
- Then bspwm: absent tiles (flag gate fails); (0,0), both partial-zero
  and sentinel float (raw whole-size equality, no zero/sentinel guard);
  the width-640/height-absent guard stays TBD (F: `P_MIN_SIZE`/`P_MAX_SIZE` govern
  the whole (w,h) tuple in the cited `_apply_hints` path, so it has no
  distinct evidenced wire value and the harness sending is unstated (sends
  (640,0) floats per the partial-zero leg, omits flags tiles per the absent
  leg)). `S(S-bsp-spc)`.
- Then i3: absent, (0,0), both partial-zero and the height-absent guard
  tile (max_width>0 and max_height>0 gate fails); the sentinel floats
  (both axes positive and equal). `S(S-i3-min)`.
- Then xmonad/Tall+Navigation2D: absent tiles (`Nothing` fails the whole-pair `sh_min_size==sh_max_size` gate); (0,0), both partial-zero and sentinel float (`Just` whole-pair equality, no zero/sentinel guard). The height-absent guard stays TBD (F: width-640/height-absent has no distinct evidenced X11 wire encoding - `P_MIN_SIZE`/`P_MAX_SIZE` govern the whole (w,h) pair - and the harness sending is unstated; no new pin per scope). `S(S-xmo-float)`; queued.
- Then sway: xdg unset/zero/partial-zero and the height-absent guard fail
  (both minima nonzero gate); xdg sentinel floats. Xwayland unset/zero/
  partial-zero and the guard fail (both minima>0 gate); Xwayland
  sentinel floats (either-axis equality). `S(S-sway-fixed-hints)` +
  `S(S-sway-spc)`.
- Then qtile/Columns: absent, (0,0), both partial-zero and the
  height-absent guard tile (both flags with 0<min==max on both axes);
  the sentinel floats. `S(S-qti-min)`.
- Then awesome/tile: unset, zero/partial-zero and the height-absent guard
  fail the fixed branch (all four bounds >0 required); the sentinel
  floats (both axes positive and equal). `S(S-awe-fixed-dynamic)`.
- Then niri: absent, (0,0), (640,0) and the height-absent guard tile
  (height gate `min.h>0` fails); (0,480) and the sentinel float (height
  equality holds). `S(S-nir-spc)`.
- Then PaperWM: tiles every hint variant (Normal non-transient admission
  only; no fixed-size branch). `S(S-pap-spc)`.
- Then karousel/Lazy: Wayland absent, (0,0), both partial-zero and the
  height-absent guard tile (enforced-minimum vs `INT_MAX` mapping keeps
  strict inequality); Wayland sentinel floats (min==max). X11
  absent tiles (absent max maps to `INT_MAX`, absent min falls back to base size and absent base to 0,0, so strict inequality holds); X11
  present-value (0,0) and both partial-zero legs tile (present max values clamp to >=1, so 0<1 keeps either-axis inequality); X11
  sentinel floats (both axes positive and equal); the X11 height-absent guard tiles (width min==max=640 holds but height min falls back to 0 against `INT_MAX`, so either-axis inequality holds).
  `S(S-kar-spc)` + `S(S-kwin-resizeable)`.
- Then paneru: no fixed-size admission counterpart for any hint variant
  (same rule-gated admission; AX exposes no min/max hint equality).
  `S(S-pan-admit)`.
- Then Ours KDE: missing/full-zero/sentinel do not auto-float; zero is unset
  per axis, so equal (640,0)/(0,480) tiles under `both-axes-fixed` and floats
  under `either-axis-fixed` via its genuinely fixed nonzero axis.
  G-05 delivered offline 2026-10-10; [shared predicate fixtures](../../../crates/tiler-core/tests/fixed_size_admission.rs),
  [KDE fixtures](../../../kwin/tests/fixed-size-admission.test.ts).
  Real-Planner fixtures verify both orientations/modes and no retained-client
  reclassification after a setting change; native representation remains TBD.
  Deliberate COSMIC raw-equality deviation for consistent zero handling.
  G-01 retains full-zero/sentinel guards; sentinel rejection is deliberate
  KDE host adaptation (KWin marker means no limit; ten references including
  COSMIC float raw-equal sentinels). The width-fixed/height-absent
  variant tiles under both (whole vectors must be usable). Reference outcomes
  for that added guard variant and native hint representation remain TBD.
- Then Ours Windows: both partial-zero orientations tile under the default
  and float through the nonzero axis under `either-axis-fixed`; missing,
  full-zero, sentinel and height-absent guards tile under both. Fresh outer
  tracks retain zero per axis through frame conversion and use the shared
  predicate. Offline 2026-10-11, prerequisite `a41fcb5` plus runtime delivery;
  retained classifications survive predicate changes. Native representation
  TBD. [Item 17 record](../../changes/archive/windows-per-axis-zero-predicate.md).
- Variant hook: NORMATIVE whole-size equality vs meaningful-bound
  normalization (User 2026-10-08).

### R-SPC-08: gain or lose fixed hints after admission

- Given (tree profiles): `H[A,B*]`, both ordinary resizable tiles.
  Fresh reverse variant: B is an automatically admitted fixed float at
  min=max 640x480; no explicit user float/tile state.
- Given (column profiles): `COL[C1[A],C2[B*]]`; reverse starts with
  A in a column and B fixed-floating where supported.
- When: tiled B sets min=max 640x480. In the fresh reverse variant,
  floating B removes its maximum hints and becomes resizable.
- Observe: admission-only identity vs reactive float/tile status; frames
  and focus separately TBD. This is not R-SPC-05's minimum-raise fixture.
- Then COSMIC: tiled B stays tiled after becoming fixed; automatic fixed
  float stays floating after becoming resizable (`is_dialog` runs once at
  map; no hint-change reclassification in source). Frames and focus TBD
  (client timing, live-only). `S(S-cos-fixed-admission)`.
- Then Hyprland/Dwindle: tiled B stays tiled; automatic float stays
  floating (`suggestsFloat` applies at initial map with group/predict
  gates; no hint-signal recompute). Frames and focus TBD (client
  timing, live-only). `S(S-hyp-float)`.
- Then bspwm: tiled B stays tiled; automatic float stays floating (hint
  refresh only updates stored hints with arrange; admission state is not
  re-run, and arrange ignores hints under the shipped default). Frames
  and focus TBD (client timing, live-only). `S(S-bsp-hint)`.
- Then i3: tiled B stays tiled; automatic float stays floating (fixed
  check is admission-only; later hint updates only clamp floating via
  `floating_check_size`, never re-admit tiles; tiled render ignores
  hints). Frames and focus TBD (client timing, live-only). `S(S-i3-min)`
  + `S(S-i3-fixed-runtime)`.
- Then xmonad/Tall+Navigation2D: tiled B stays tiled; automatic float
  stays floating (fixed/transient check runs once in `manage`; later
  status changes only via manual float/sink). Frames and focus TBD
  (client timing, live-only). `S(S-xmo-float)`.
- Then sway: tiled B stays tiled; automatic float stays floating (float
  admission evaluated at map; the runtime Xwayland hint path handles
  urgency only, no re-admission). Frames and focus TBD (client timing,
  live-only). `S(S-sway-min)`.
- Then qtile/Columns: tiled B stays tiled; automatic float stays floating
  (hint refresh updates stored hints with no tiled promotion; only
  floating increments relayout). Frames and focus TBD (client timing,
  live-only). `S(S-qti-min)`.
- Then awesome/tile: dynamic classification (not admission-only): tiled B gaining min=max 640x480 turns implicit floating on, and automatic fixed float B losing its maximum hints turns implicit floating off, respectively, unless an explicit `floating` state overrides it (hint signals recompute implicit float; `property::size_hints` watched, explicit wins). Frames and focus TBD (L: client timing, live-only). `S(S-awe-fixed-dynamic)`.
- Then niri: tiled B stays tiled; automatic float stays floating
  (`compute_open_floating` runs at the four open callsites only; later
  status changes only via the plain tile-move toggle). Frames and focus
  TBD (client timing, live-only). `S(S-nir-fixed-open)` +
  `S(S-nir-flttoggle)`.
- Then PaperWM: tiled B stays tiled; automatic float does not arise (no
  fixed-size branch; `add_filter` is admission-only). Where B floats by
  another gate, hint loss alone does not retile it. Frames and focus TBD
  (client timing, live-only). `S(S-pap-spc)`.
- Then karousel/Lazy: tiled B stays tiled; automatic float stays
  floating (gates evaluated once at `addClient`; only a caption-follow
  signal re-evaluates, with no size-hint watcher). Frames and focus TBD
  (client timing, live-only). `S(S-kar-admit)`.
- Then paneru: no fixed-size admission counterpart, so hint gain/loss has
  no reclassification to observe (role-gated admission; rule-assigned
  float only). `S(S-pan-admit)`.
- Then Ours KDE: tiled B stays tiled after becoming fixed; automatic
  fixed float stays floating after becoming resizable. Existing hint
  projection/clamp continues (NORMATIVE D2); react to self-resize to avoid
  gaps/overlaps, no new hint-signal work. Implemented offline
  2026-10-08 with real Planner [fixtures](../../../kwin/tests/fixed-size-admission.test.ts);
  native exact frames/focus TBD.
- Then Ours Windows: retained tiles remain tiled after becoming fixed;
  automatic fixed floats remain floating after losing hints. Current min/max
  hints use shared projection/clamp policy (maximums accept explained clamps,
  never redistribute shares). Offline 2026-10-11;
  [record](../../changes/archive/windows-fixed-size-admission.md). Native frames/focus TBD.
- Variant hook: NORMATIVE admission-only vs dynamic classification
  (User 2026-10-08).

### R-SPC-09: explicitly tile an automatically admitted fixed float

- Given (tree profiles): A tiled, F ordinary fixed-floating with
  min=max 640x480; tiled workspace, no overlay. Fresh sticky variant:
  F is fixed and sticky, with the same hints.
- Given (column profiles): `COL[C1[A]]` plus F floating; sticky
  variant conditional on that profile's sticky model.
- When: explicitly tile F; observe again with identical hints. KDE
  sticky variant uses Meta+G, not origin-preserving Meta+Shift+G.
- Observe: user override survives observation vs immediate re-float.
- Then COSMIC: ordinary fixed F tiles directly without a hint check
  (explicit floating toggle maps directly without hint reclassification; fixed classification runs once at map with no hint-change path, so observation does not re-float it). Sticky occupants have no explicit-tile counterpart: `toggle_floating_window` touches workspace tiling/floating layers only with no sticky branch, and the shipped keybinding inventory carries no `ToggleSticky` binding, so Super+G refuses with sticky preserved. `S(S-cos-fixed-toggle)` + `S(S-cos-sticky)` + `S(S-cos-raise)`.
- Then Hyprland/Dwindle: explicit tile survives observation (toggle via
  `changeFloatingMode` with no refusal branch; `suggestsFloat` applies at
  initial map only). Exact frames/focus TBD (client timing, live-only).
  `S(S-hyp-float)`.
- Then bspwm: explicit tile survives observation (state toggle is vacant
  in place; later hint refresh does not re-run admission). Exact
  frames/focus TBD (client timing, live-only). `S(S-bsp-state)`.
- Then i3: explicit tile survives observation (`floating_disable`
  inserts after the tiling-focused descendant; later hint updates never
  re-admit). Exact frames/focus TBD (client timing, live-only).
  `S(S-i3-flt-toggle)`.
- Then xmonad/Tall+Navigation2D: explicit tile survives observation
  (`sink` clears only the floating map; no automatic re-float on later
  observation). Exact frames/focus TBD (client timing, live-only).
  `S(S-xmo-float)`.
- Then sway: explicit tile survives observation (`container_set_floating`
  is the explicit path; `wants_floating` is evaluated at map only).
  Exact frames/focus TBD (client timing, live-only). `S(S-sway-float)`.
- Then qtile/Columns: explicit tile survives observation (toggle flips
  state with tile-frame retain and layout re-add; float rules are
  evaluated at admission, not re-applied on observation). Exact
  frames/focus TBD (client timing, live-only). `S(S-qti-float)`.
- Then awesome/tile: explicit `floating=false` overrides the implicit fixed status and survives observation (explicit wins; hint signals recompute implicit only); sticky variant uses the same explicit tile path with sticky retained (sticky is orthogonal with no float-only guard, so F stays sticky-tiled). Exact frames/focus TBD (L: client timing, live-only). `S(S-awe-fixed-dynamic)` + `S(S-awe-sticky)` + `S(S-awe-float)`.
- Then niri: explicit tile survives observation (plain tile-move toggle;
  `compute_open_floating` runs at open only). Exact frames/focus TBD
  (client timing, live-only). `S(S-nir-flttoggle)` + `S(S-nir-fixed-open)`.
- Then PaperWM: F is already tiled on admission, so explicit tile retains
  the tile (no fixed-size float to override). Exact frames/focus TBD
  (client timing, live-only). `S(S-pap-spc)`.
- Then karousel/Lazy: explicit tile is refused for the unshapeable fixed
  float (float-to-tile requires `canTileEver`), so F stays floating;
  observation does not retile it. Exact frames/focus TBD (client timing,
  live-only). `S(S-kar-admit)`.
- Then paneru: no fixed-size admission counterpart, so an explicit tile of
  a fixed client has no automatic-float override to test (role-gated
  admission; rule-assigned float only). `S(S-pan-admit)`.
- Then Ours KDE: explicit tile wins for the same live client, including
  sticky Meta+G; ordinary observation does not re-float it. Exact-ref
  suppression survives omitted/scoped/released observations; new refs
  classify again (NORMATIVE D3, no setting). Implemented offline 2026-10-08,
  [real Planner and sticky fixtures](../../../kwin/tests/fixed-size-admission.test.ts).
  D7 now restores matched fixed-window tile overrides across owner restart
  ([restart fixtures](../../../kwin/tests/float-intent.test.ts)); no positions persist.
  Physical hide/show/domain reassignment TBD.
- Then Ours Windows: explicit tile wins for the same live client through
  observations, hide/scope and workspace re-adoption. Only tile wins over known
  automatic fixed origin persist the tile marker; repeated generic non-fixed
  unfloats do not. Offline 2026-10-11;
  [record](../../changes/archive/windows-fixed-size-admission.md). Native journeys TBD.
- Variant hook: NORMATIVE explicit intent vs automatic fixed
  classification (User 2026-10-08).

### R-SPC-10: admit fixed and maximized, then unmaximize

- Given (tree profiles): `H[A*]`; E ordinary min=max 640x480,
  born maximized, not fullscreen/sticky, no custom rule.
- Given (column profiles): `COL[C1[A*]]`; same E.
- When: open E; natively unmaximize E.
- Observe: floating base vs reserved tiled slot beneath maximize;
  no launch unmaximize or focus/geometry claim is inferred.
- Then COSMIC: fixed E admits floating, then maximizes with original
  layer Floating; unmaximize returns to that layer with no reserved tiled slot beneath maximize.
  `S(S-cos-fixed-admission)` + `S(S-cos-fixed-maximize)`.
- Then Hyprland/Dwindle: fixed E admits floating at map (`suggestsFloat`
  min==max applied at initial map); born-maximized applies as
  `FSMODE_MAXIMIZED` at the work area (pending maximum consumed at map);
  native unmaximize exits to `NONE` leaving placement to recalc with
  floating retained. Exact frames/focus TBD (client timing, live-only).
  `S(S-hyp-float)` + `S(S-hyp-fs)` + `S(S-hyp-bornmax)`.
- Then bspwm: E floats (min==max hints rule); no maximize command/state
  exists (desktop layout tiled/monocle only; maximize flags are not tree
  state), so the born-maximized/unmaximize legs have no counterpart.
  `S(S-bsp-spc)` + `S(S-bsp-layout)` + `S(S-bsp-admit)`.
- Then i3: E floats (min==max branch); no maximize verb/state exists
  (maximize is derived-only), so the born-maximized/unmaximize legs have
  no counterpart and never run. `S(S-i3-min)` + `S(S-i3-max)`.
- Then xmonad/Tall+Navigation2D: E floats (fixed/transient manage); no
  maximize command/state exists (`Full` is a workspace layout, not
  per-window maximize), so the born-maximized/unmaximize legs have no
  counterpart. `S(S-xmo-float)` + `S(S-xmo-layout)`.
- Then sway: E floats (xdg either-axis min==max; xwayland fixed branch);
  no maximize verb exists (request only schedules a configure), so the
  born-maximized/unmaximize legs have no counterpart and never run.
  `S(S-sway-spc)` + `S(S-sway-max)`.
- Then qtile/Columns: E admits floating via the fixed-size rule;
  maximize is a floating-layer state at work-area size. Maximize entered
  after fixed-float admission saves `FLOATING`, and unmaximize restores
  that saved state without re-running the fixed rule. X11 client maximize
  messages echo the property without driving this mode; Wayland requests
  drive the setter. Exact born-request ordering, frames and focus TBD
  (client/native timing). `S(S-qti-spc)` + `S(S-qti-fs)` +
  `S(S-qti-fs-restore)`.
- Then awesome/tile: E admits implicitly floating (both positive axes
  equal; maximized also implies implicit float; manage-time maximized
  hint applies), so born-maximized stays implicitly floating; native
  unmaximize clears maximized but fixed keeps implicit float, so it stays
  floating. Exact frames/focus TBD (client timing, live-only).
  `S(S-awe-fixed-dynamic)` + `S(S-awe-float)` + `S(S-awe-fs)`.
- Then niri: fixed height computes floating, but a pending-maximized tile
  opens in the scrolling layout, so born-maximized E admits as a scrolling
  maximized column; native restore clears the flag and floats E (the floating computation seeds restore-to-floating
  at admission, so the seeded memory -- not a from-floating journey --
  drives the exit). `S(S-nir-spc)` + `S(S-nir-fixed-open)` +
  `S(S-nir-maxfs)`.
- Then PaperWM: E tiles (Normal non-transient admission; no fixed-size
  branch). Born-maximized admission hits the `maximized-horizontally`
  handler (:3514-3526), which unmaximizes BOTH first - clearing the native
  flag - then width-toggles with `unmaximizedRect` memory at the shipped
  100 percent; with no native maximized flag left, a native restore has
  nothing to clear: moot. `unmaximizedRect` restores only via a second
  width-toggle (:4794-4830 uses `move_resize_frame` only and never sets a
  native maximize flag; no native toggle-off path is claimed). Exact
  native frames TBD (live: client settle timing). `S(S-pap-spc)` +
  `S(S-pap-widthmax)` + `S(S-pap-layout)`; frames queued (live).
- Then karousel/Lazy: E floats as untileable under the shapeability gate
  (both-axes min==max keeps `isResizable` false); gates evaluate once at
  `addClient` with no size-hint watcher and Floating owns no maximize
  watcher, so born-maximized stays floating
  and native unmaximize leaves it floating. Exact native maximize
  frames/focus TBD (client timing, live-only). `S(S-kar-spc)` + `S(S-kwin-resizeable)` +
  `S(S-kar-admit)` + `S(S-kar-maxfs)`.
- Then paneru: no fixed-size admission counterpart (role-gated admission
  with no size predicate; float is rule-assigned) and no maximize verb
  (AX observes fullscreen only; host zoom is not read), so the combined
  fixed+maximize/unmaximize legs have no built-in equivalent with no
  applicable journey. `S(S-pan-admit)` + `S(S-pan-cmds)` + `S(S-pan-axfs)`.
- Then Ours KDE: fixed E floats beneath native maximize, no reserved
  tile on admission; unmaximize leaves it floating (NORMATIVE D4).
  Non-fixed Q3 is unchanged. D6 workspace enable while maximized keeps a
  fixed automatic float slotless; native unmaximize stays floating.
  Explicit user tile overrides retain Q3 R-MAX-03's reserved-slot route.
  Implemented offline 2026-10-08; [production-entry fixtures](../../../kwin/tests/fixed-size-workspace-entry.test.ts),
  [D5/D6 record](../../changes/archive/fixed-size-exit-and-enable.md). Native TBD.
- Then Ours Windows: fixed birth keeps a slotless floating base beneath
  maximize under both `core.born_maximized` values; unmaximize stays floating.
  Enable rechecks current hints. Fixed-origin explicit overrides instead keep
  a reserved tiled overlay, including hint loss (tentative Windows routing).
  Offline 2026-10-11; [record](../../changes/archive/windows-fixed-size-admission.md).
  Native launch/restore and game no-touch TBD; ordinary non-fixed routes unchanged.
- Variant hook: NORMATIVE fixed-float base vs Q3 reserved tiled base
  (User 2026-10-08).

### R-SPC-11: fixed client exits fullscreen, born or previously floating

- Given (tree profiles): `H[A*]`; E ordinary min=max 640x480,
  born fullscreen. Fresh variant: E first admits fixed-floating, then
  enters fullscreen. Non-fixed variant: N ordinary resizable, born
  fullscreen on a tiled workspace. Workspace remains tiled; no sticky/maximize/rules.
- Given (column profiles): `COL[C1[A*]]`; same independent lifecycles,
  including the non-fixed N variant.
- When: exit E's fullscreen natively (non-fixed variant: exit N's fullscreen natively).
- Discriminating variant: E starts one-axis-fixed under `both-axes-fixed`;
  while fullscreen switch to `either-axis-fixed` or change hints to both-axis
  fixed; exit once; enter fullscreen again, lose fixed hints, exit again.
- Observe: classify fixed hints on first normal admission vs restore
  previously recorded layer. Non-fixed N first exit tiles. During fullscreen, game state is untouched.
  Reference outcomes for the non-fixed N and changed-hints/predicate/repeated-exit
  variants are TBD for every reference profile; native Ours legs remain TBD.
- Then COSMIC: born E and non-fixed N both skip normal mapping (None
  restore) and exit to the tiling default with no hint check; previously
  floating E restores floating with retained geometry; hint changes while
  fullscreen have no effect (float/tile decided at map); re-entering
  fullscreen from the exited tile carries Some restore, so repeated exit
  restores the saved tile/float layer, not current hints. Exact frames/focus
  TBD (client timing, live-only). `S(S-cos-fixed-admission)` +
  `S(S-cos-fixed-fullscreen)` + `S(S-cos-fsrestore)`.
- Then Hyprland/Dwindle: non-fixed N applies fullscreen over ordinary
  tiling and exit to `NONE` recovers tiles; E born/prior retain floating
  as before with float size remembered. Product predicate switch has no
  counterpart (Wayland either-axis minima>1 / X11 both positive axes
  hardcoded, no setting); hint changes while fullscreen have no effect
  (`suggestsFloat` at initial map only); repeated exits follow the same
  recalc path, not current hints. Exact frames/focus TBD (client timing,
  live-only). `S(S-hyp-float)` + `S(S-hyp-fs)` + `S(S-hyp-fixed-hints)`.
- Then bspwm: non-fixed N admits fullscreen state and exit returns to
  last-state tiled (vacant in place); E born/prior end floating as before.
  Predicate switch is product-only (whole-size equality hardcoded in
  `_apply_hints`); hint changes update stored hints only with arrange
  ignoring them under the shipped default, so exits follow last-state, not
  current hints; repeated exits replay last-state. Exact frames/focus TBD
  (client timing, live-only). `S(S-bsp-spc)` + `S(S-bsp-admit)` +
  `S(S-bsp-fs)` + `S(S-bsp-state)` + `S(S-bsp-hint)`.
- Then i3: non-fixed N toggles fullscreen mode on a tiled container and
  exit clears it staying tiled; E born/prior stay floating as before.
  Predicate switch is product-only (both-axes positive equality hardcoded);
  hint updates route to `floating_check_size` for floating containers only
  and never re-admit tiles, so mid-fullscreen hint changes do not alter the
  exit layer; repeated exits clear the mode to the same saved layer. Exact
  frames/focus TBD (client timing, live-only). `S(S-i3-min)` +
  `S(S-i3-admit)` + `S(S-i3-fs)` + `S(S-i3-fixed-runtime)`.
- Then xmonad/Tall+Navigation2D: non-fixed N admits tiled (no manage float cause) then `doFullFloat` fullscreen float via the event hook, exit `doSink` stays tiled; E born/prior land tiled on exit (sink clears the map, no re-float on later observation). Predicate switch is product-only (whole-pair equality, no setting); hint changes have no post-manage path (`manage` once; later only manual float/sink); repeated exits sink again. `S(S-xmo-float)` + `S(S-xmo-ewmh)` + `S(S-xmo-admit)`
- Then sway: non-fixed N maps fullscreen on a tiled container and exit
  clears the mode staying tiled; E born/prior retain floating as before.
  Predicate switch is product-only (xdg/xwayland either-axis hardcoded);
  the runtime hint path handles urgency only with map-time evaluation, so
  hint changes do not alter the exit; repeated exits replay the mode clear.
  Exact frames/focus TBD (client timing, live-only). `S(S-sway-spc)` +
  `S(S-sway-full)` + `S(S-sway-min)` + `S(S-sway-fixed-hints)`.
- Then qtile/Columns: X11-established legs: non-fixed N and born-fixed E
  both admit fullscreen-first (`auto_fullscreen` before the fixed-rule
  `elif`, match skipped) saving `NOT_FLOATING`, so both exit tiled via
  save/restore; prior floating E (admitted `FLOATING` via the fixed rule,
  then fullscreen saving `FLOATING`) restores floating. Exit never re-runs
  the float-rule match; hint updates never promote tiles (floating
  increments only), so mid-fullscreen hint changes do not alter the exit;
  repeated exits replay save/restore. Native ack/focus TBD.
  `S(S-qti-spc)` + `S(S-qti-fs)` + `S(S-qti-fs-restore)` + `S(S-qti-min)`.
- Then awesome/tile: non-fixed N admits implicit float via fullscreen and
  native exit clears it landing tiled; E born/prior stay floating while
  fixed holds as before. One-axis start tiles (both axes must match;
  product predicate switch has no counterpart, hardcoded). Changing hints
  to both-axis fixed while fullscreen recomputes implicit float (hint
  signals unless explicitly overridden), so the first exit floats; losing
  hints while fullscreen recomputes but fullscreen retains, and the second
  exit clears to tiled. Repeated exits track current hints, not saved
  state. Exact frames/focus TBD (client timing, live-only).
  `S(S-awe-fixed-dynamic)` + `S(S-awe-float)` + `S(S-awe-fs)`.
- Then niri: non-fixed N opens scrolling fullscreen (no fixed float) and
  exit lands tiled; fixed E born fullscreen opens scrolling fullscreen
  the same way but exits floating, as does prior floating E: the fixed
  computation seeds restore-to-floating at admission, so exits follow
  that memory, not current hints. Predicate switch is product-only
  (height-only predicate hardcoded); hint changes have no open-path
  caller, so mid-fullscreen changes never alter an exit; repeated exits
  replay the same memory; fullscreen entry writes only the target column.
  `S(S-nir-spc)` + `S(S-nir-fixed-open)` + `S(S-nir-maxfs)`.
- Then PaperWM: non-fixed N is honored fullscreen and exit restores the
  saved tiled frame to tiled, same as born-fixed E (fixed tiles: no
  fixed-size exclusion, then the same fullscreen memory); the prior-floating
  fixed leg still has no faithful start (no fixed float exists to begin
  with: fixture-inapplicable for that leg). Hint changes have no branch
  (no fixed-size exclusion); repeated exits restore the saved frame again.
  Exact native frames TBD (live: client settle timing). `S(S-pap-spc)` +
  `S(S-pap-fsframe)` + `S(S-pap-layout)`; frames queued (live).
- Then karousel/Lazy: non-fixed N born-fullscreen admits Tiled only when the fullscreen state has not materialized at add (flag/geometry unset at add given the Tiled.ts:166 timing race, so `shouldTile` passes); materialized fullscreen fails `shouldTile` and admits Floating despite fullscreen counting as shapeable in `canTileEver`. A Tiled-admitted N stays tiled across native exit (exit-time `canTileEver` passes); a Floating-admitted N stays floating (Floating owns no fullscreen watcher). E born exits floating via
  untileable-after-exit while prior floating stays floating. Tile-vs-float at add TBD (live: fullscreen state timing at admission). Predicate
  switch is product-only (KWin either-axis strict inequality hardcoded, no
  both/either setting); hint changes have no watcher (only caption-follow
  re-evaluates) but the exit check reads current shapeability, so a hint
  change takes effect at exit; repeated exits re-evaluate at each exit.
  Exact frames/focus TBD (client timing, live-only). `S(S-kar-spc)` + `S(S-kwin-resizeable)` +
  `S(S-kar-maxfs)` + `S(S-kar-admit)`.
- Then paneru: non-fixed N follows the same role-gated admission as E (no
  size predicate; float is rule-assigned) with fullscreen as a host AX
  marker, so N/E born/prior exits stay TBD (owner-specific journey);
  predicate/hint variants have no size path to act on. `S(S-pan-admit)` +
  `S(S-pan-cmds)` + `S(S-pan-axfs)`.
- Then Ours KDE: born-fullscreen first exit classifies as newly admitted:
  fixed floats with no writes, otherwise tiles; previously automatic
  floating restores floating. No target writes while fullscreen (NORMATIVE
  D5 CHANGED, User 2026-10-08; deliberate COSMIC deviation for game safety,
  pending live observation). First observed exit for the exact live client
  uses current hints/predicate, so the discriminator floats on first exit;
  later exits retain that float even after hint loss (D2), never re-admit.
  Implemented offline 2026-10-09, [real Planner fixtures](../../../kwin/tests/fixed-size-admission.test.ts),
  [record](../../changes/archive/fixed-size-exit-and-enable.md); native game/restore journey TBD.
- Then Ours Windows: first born-fullscreen exit uses current hints/predicate:
  fixed floats without target writes; unknown/non-fixed tiles. A committed
  first exit consumes its lifetime mark; repeated toggles and previously
  automatic floats preserve admission identity. Offline 2026-10-11, production
  assembly/stamped Engine fixtures; [record](../../changes/archive/windows-fixed-size-admission.md).
   Native/game journey TBD; borderless-game inference unchanged.
   User 2026-10-12 explicitly confirms leaving fullscreen including borderless
   uses ordinary admission/tiling subject to D5 fixed/intentional exceptions.
   Startup/born exits, managed slot return/write exclusion and borderless exit
   pinned offline; owned exit verifies restore before metadata removal, with
    failure/later-press retry coverage. Owned Notepad startup adoption physically
    user-accepted 2026-10-12, scoped single-output Notepad/Paint, base `e506350`
    plus pre-retirement Windows changes: stop/relaunch while fullscreen then
    Win+F11 exit tiles. Every Win+F11 raised the known parked Xbox prompt.
    App-native/born-after-start/borderless, fixed/game/DPI/multi-output and
    later automatic-retirement physical acceptance remain TBD
   ([record](../../changes/archive/windows-fullscreen-exit-acceptance.md)).
- Variant hook: NORMATIVE first normal admission vs fullscreen restore
  identity (User 2026-10-08; first-exit classification delivered offline).

### R-SPC-12: enable workspace tiling with an automatic fixed float

- Given (tree profiles): workspace floating-only with ordinary fixed E
  (min=max 640x480), no explicit per-window override. Separate intentional
  float C is a control; no sticky/fullscreen/maximize/rules. Arrival
  variant: ordinary fixed F arrives while the workspace is floating, then
  tiling is enabled for that workspace.
- Given (column profiles): same free-floating clients on a workspace
  where a floating-to-tiled workspace action exists, including the F
  arrival variant.
- When: enable tiling for that workspace.
- Discriminating variants: disable tiling for an ordinary tile, give it fixed
  hints, enable; or disable for a one-axis automatic float, switch predicate
  to `both-axes-fixed`, enable. Control: explicit user tile override, then
  disable/enable with fixed hints. Maximized control: fixed automatic float,
  disable/enable while maximized, then natively unmaximize.
- Observe: E auto-float survives vs workspace action explicitly tiles it;
  F fixed arrival stays/becomes untouched float while other arrivals tile;
  intentional C is distinguished, not silently reclassified.
  Reference outcomes for the F arrival and changed-hints/predicate/override/
  maximized-control variants are TBD for every reference profile; native Ours
  legs remain TBD.
- Then COSMIC: retiles E, C, and F arrivals without a fixed-hint check
  (enable maps every ordinary floater); disable/give-fixed/enable still
  tiles with fixed ignored; predicate switch is product-only (whole-size
  equality, no setting); explicit override is moot in the cited path
  (tiles anyway, with no intentional/automatic distinction to preserve C); maximized control tiles then re-overlays maximized with
  the layer retargeted, and restore reveals the retained slot.
  `S(S-cos-fixed-workspace)` + `S(S-cos-fixed-maximize)`.
- Then Hyprland/Dwindle: no workspace-mode counterpart per R-FLT-04
  (no workspace tiling toggle; float is per-window, so enable/F-arrival,
  changed-hints/predicate, override, and maximized legs never run with no
  applicable journey).
  `S(S-hyp-float)`.
- Then bspwm: No workspace-mode counterpart per R-FLT-04 (no workspace tiling toggle; float is per-window, so enable/F-arrival, changed-hints/predicate, override, and maximized legs never run); `S(S-bsp-layout)`.
- Then i3: no workspace-mode counterpart per R-FLT-04; F-arrival,
  changed-hints/predicate, override, and maximized legs share the absence
  with no applicable journey. `S(S-i3-wsmode)`.
- Then xmonad/Tall+Navigation2D: no workspace-mode counterpart per
  R-FLT-04 (no workspace tiling toggle in source; float is per-window, so enable/F-arrival, changed-hints/predicate, override, and maximized legs never run). `S(S-xmo-layout)`.
- Then sway: no workspace-mode counterpart per R-FLT-04 (no workspace tiling toggle; float is per-window, so enable/F-arrival, changed-hints/predicate, override, and maximized legs never run with no applicable journey). `S(S-sway-wsmode)`.
- Then qtile/Columns: no workspace-mode counterpart per R-FLT-04;
  F-arrival, changed-hints/predicate, override, and maximized legs share
  the absence (no workspace tiling toggle; static groups with one global
  floating_layout, float is per-window, so enable/F-arrival,
  changed-hints/predicate, override, and maximized legs never run with no
  applicable journey). `S(S-qti-float)` + `S(S-qti-wsdef)`.
- Then awesome/tile: per-tag layout switch (floating<->tile, no global
  flag): F arrival admitted under the floating layout keeps geometry
  (`c.floating` unset), then re-tile recalc floats fixed F (excluded from
  `tiled_clients`) while ordinary arrivals tile; disable/give-fixed/enable
  floats fixed E on recalc and tiles one-axis (both axes must match);
  predicate switch is product-only; explicit `floating=false` survives the
  recalc staying tiled (explicit over implicit); maximized control stays
  maximized-and-implicitly-floating across the switch, and
  restore-then-unmaximize keeps fixed E floating while non-fixed rejoins
  tiled order. Intentional C stays explicitly floating throughout. Exact
  frames/focus TBD (L: client timing and live placement, not in the recalc path). `S(S-awe-layout)` + `S(S-awe-float)` + `S(S-awe-tile)`
  + `S(S-awe-fixed-dynamic)` + `S(S-awe-fs)`.
- Then niri: no workspace-mode counterpart (per-window toggle only);
  F-arrival, changed-hints/predicate, override, and maximized legs share
  the absence: no applicable journey. `S(S-nir-float)`.
- Then PaperWM: no workspace-mode counterpart (no workspace tiling toggle in the registered action inventory); F-arrival,
  changed-hints/predicate, override, and maximized legs share the absence:
  no-counterpart with no applicable journey.
  `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart for the workspace enable (no workspace-mode toggle verb in the full Actions/definition inventory; `windowToggleFloating` is per-window only); F-arrival,
  changed-hints/predicate, override, and maximized legs share the absence;
  no applicable reference journey. `S(S-kar-acts)`.
- Then paneru: no workspace-mode counterpart (no workspace tiling flag; `Manage` is per-window) and no size predicate for the fixed members; F-arrival,
  changed-hints/predicate, override, and maximized legs share the absence
  with no applicable journey.
  `S(S-pan-cmds)` + `S(S-pan-admit)`.
- Then Ours KDE: enabling workspace tiling checks every window being
  tiled including arrivals while floating: fixed stays/becomes untouched
  float, others tile; explicit user tile overrides stay tiled;
  intentional C and sticky floats keep their existing exceptions
  (NORMATIVE D6 CHANGED, User 2026-10-08; COSMIC deviation, pending live
  check). Current hints/predicate recheck ordinary admission pins and automatic
  records; the variants respectively float, tile, retain the explicit tile,
  and retain slotless float under maximize and after unmaximize.
  Implemented offline 2026-10-09;
  [production-entry release/retile fixtures](../../../kwin/tests/fixed-size-workspace-entry.test.ts)
  and [intentional/sticky controls](../../../kwin/tests/fixed-size-admission.test.ts).
  [record](../../changes/archive/fixed-size-exit-and-enable.md). Native release/placement TBD.
- Then Ours Windows: every candidate is freshly classified at enable, including
  floating-workspace arrivals and changed ordinary hints/predicate; fixed stays
  slotless automatic float, non-fixed tiles, explicit tile/intentional/sticky
  intent survives. D4 latches and ordinary pins reset only on confirmed release.
  Offline 2026-10-11; [record](../../changes/archive/windows-fixed-size-admission.md).
  Native release/restore TBD; intentional C also reuses R-FLT-04 evidence.
- Variant hook: NORMATIVE automatic vs intentional float identity at
  workspace enable (User 2026-10-08; check-every-window delivered offline).

### R-SPC-13: adopt an already mapped fixed client on owner startup

- Given (tree profiles): tiler owner stopped, A ordinary and E ordinary
  min=max 640x480 already mapped on a tiled-designated workspace. Fresh
  restart variant: E was explicitly tiled before this owner stop.
  Unavailable-store variant: the intentional-float store is missing or
  unreadable at restart, so the tile override falls back to recompute.
- Given (column profiles): same already mapped A/E with native owner
  startup/restart where supported; no H/V tree is asserted, including the
  explicit-tile and unavailable-store variants.
- When: start the tiler owner with the clients still alive.
- Discriminating variants: successful override then re-float/close; a complete
  startup inventory omits a closed client vs a partial inventory omits a live
  hidden client; old v1 file has no tile field, corrupt file, or stored ID does
  not match E. Each restarts independently. Hint-loss/enable and non-fixed
  controls are in R-RST-04. Reference outcomes for all added legs remain TBD;
  native Ours outcomes remain TBD.
- Windows marker discriminator: explicitly tile fixed E and an intentional
  resizable float N; remove E's maximum hints, natively maximize both, restart
  the owner, then disable/enable tiling. Repeat under both
  `core.born_maximized` values with hint loss; N must keep generic non-fixed
  maximize routing in every leg. Observe override hydration, automatic
  origin, reserved-slot versus maximize-clear routing, and native writes.
  Reference outcomes and Ours Windows observed outcomes remain TBD; Windows
  fixed-origin marker coverage and retained-maximized hydration are implemented
  offline; fixed-override reserved-overlay routing under both born-maximized
  values is approved shared policy User 2026-10-17, option A (not implemented;
  [decision](../../decisions.md#fixed-size-admission)).
  Exact Tier 1 intersection (durable marker plus owner restart with live
  clients plus maximize under both born modes) has no counterpart in these
  four pinned Tier 1 references (COSMIC `3d55cba0`, Hyprland `19fb395d`, i3
  `903bcd51`, sway `1652c54b`):
  COSMIC and Hyprland have no owner-restart-with-live-clients journey, i3
  and sway have no maximize verb. Supported analogues only, not votes:
  COSMIC original-layer retain beneath maximize for ordinary tiled windows;
  Hyprland refusal-free explicit toggle plus pending-maximize-at-map; i3
  whole-tree restart placeholders plus `floating_disable` surviving later
  observation; sway explicit `container_set_floating` surviving observation
  ([findings](../../research/fixed-override-born-maximized.md)).
- Observe: fixed classification at adoption vs open-only classification;
  previous explicit tile override vs recomputed automatic identity.
  Reference outcomes for the unavailable-store variant are TBD.
- Then COSMIC: no-counterpart for this script-owner startup adoption in the inspected compositor inventory; ordinary
  compositor map is not evidence for this owner journey.
  Store-fault/ID/omission variants have no counterpart in the
  traced pinned-workspace persistence inventory (orderly persist of output match/tiling flag/id/name only, no window/store-fault path); no applicable reference journey.
  `S(S-cos-persist)`.
- Then Hyprland/Dwindle: no-counterpart for this owner restart with
  clients alive (no re-exec verb in the dispatcher inventory; reload keeps
  the live tree only and exit stops the compositor; no dump or store
  in inventory, so re-float/close and store-fault/ID/omission variants never run).
  `S(S-hyp-reload)` + `S(S-hyp-shortcut)`.
- Then bspwm: owner `wm -r` dumps full state and re-execs restoring it
  (ratios, WS membership, float frames, sticky/state/lastState, focus
  history/stack round-trip); fixed E stays floating and explicitly tiled E
  stays tiled as dumped state. Re-floated E restores as floating and closed
  E is absent (dump reflects live state). Store-fault (missing/unreadable/
  corrupt/v1/mismatched) and complete-vs-partial omission variants have no
  counterpart in the dump/load inventory (orderly fields only). `S(S-bsp-restore)`.
- Then i3: fixture fresh start (no layout file stated): missing file skips restore (`tree.c:71-74`) into `tree_init` (`main.c:935-937`) plus ordinary manage (`manage.c:44-68`), so fixed E re-floats via min==max and a pre-stop explicit tile is recomputed (lost); unreadable/corrupt fall back the same way (slurp fail `:77-80` / append fail `:95-98` to init); ID mismatch finds no swallow (`manage.c:282-283,357-376`, `restart_mode` flag `load_layout.c:583-584`) into ordinary manage, recomputed; closed E has no window to swallow, absent; re-floated E restores floating via the orderly fields plus placeholder open (`tree.c:116`). Orderly `--restart` with the file preserves tiling via placeholders (out-of-fixture data point). v1 no-tile field and partial inventory omitting a live hidden client have no counterpart in the whole-tree layout dump (no versioned membership/partial semantics). `S(S-i3-restart)` + `S(S-i3-min)`.
- Then xmonad/Tall+Navigation2D: restart resumes the windowset including
  the floating map with Tall recalculated; fixed E carries as an ordinary
  float and explicitly tiled E (sink clears the map) stays tiled. Re-floated
  E carries as floating and closed E is absent. Store-fault/ID/omission
  variants have no counterpart (StateFile carries the whole StackSet only).
  `S(S-xmo-restart)` + `S(S-xmo-float)`.
- Then sway: no-counterpart for this owner restart with clients alive
  (command inventory carries `reload` and `exit` with no restart verb;
  reload is in-place config only), so re-float/close and
  store-fault/ID/omission variants never run with no applicable journey. `S(S-sway-reload)`.
- Then qtile/Columns: X11-only restart: groups/layouts/screens restore while widths reset
  and windows re-admit in X server query-tree/stacking order (state carries no per-window float); fixed E
  re-admits floating via the fixed-size rule, so an explicit tile override
  is lost (re-float moot) and closed E is absent. Wayland has no restart journey.
  Store-fault/ID/omission
  variants have no counterpart (groups/layouts/screens/scratchpads only,
  no tile store). `S(S-qti-state)` + `S(S-qti-rstadmit)` +
  `S(S-qti-reload)` + `S(S-qti-spc)`.
- Then awesome/tile: client order and floating state restore with tags
  recreated (shares recalculate); fixed E recomputes to implicit float on
  re-manage while explicit `floating=false` persists as a property.
  Re-floated E restores as floating and closed E is absent. Store-fault/ID/
  v1 variants have no counterpart (order+floating only, no tile store).
  Exact frames/focus TBD (L: live placement and client settlement, not in the order/floating store). `S(S-awe-ctl)` + `S(S-awe-float)` +
  `S(S-awe-fixed-dynamic)`.
- Then niri: no-counterpart for this owner restart with layout recovery
  (Quit exits and LoadConfigFile reloads config only; no layout dump,
  re-exec verb, or store in inventory); re-float/close and
  store-fault/ID/omission variants share the absence: no applicable
  journey. `S(S-nir-rst)`.
- Then PaperWM: adoption classification determined: controlled disable+enable stages SaveState and re-adds
  existing windows with prevSpace layout restored; fixed E tiles (no
  fixed-size branch, so the pre-stop explicit tile is moot) and stays tiled. Closed E is
  absent (dead pruned). Store-fault/ID/omission variants have no
  counterpart (SaveState covers monitors/spaces/widths only):
  no-counterpart. Exact widths need A's live frame, unstated in the fixture: TBD (fixture: A's live frame).
  Selection is the host tab-list head: TBD (host: tab-list order policy untraced). `S(S-pap-rst)` +
  `S(S-pap-spc)` + `S(S-pap-layout)`; widths queued (fixture), focus queued (host).
- Then karousel/Lazy: script enable adopts existing clients in `Workspace.windows`
  order via `addExistingClients` into `addClient` with live-only Grid state (no persisted layout);
  fixed E re-admits floating via the shapeability gate (both-axes min==max keeps `isResizable` false), so an explicit
  tile override is lost (no tile-override store; re-admission reclassifies from current host flags). Re-floated E is Floating and closed E is absent.
  Store-fault/ID/omission variants have no counterpart (no store at all).
  Each Tiled opens after the last-focused column else the last with `preferredWidth` clamped into [min,max]; no script focus write on this path.
  Exact order TBD (fixture: creation/manage order and A's live frame unstated). Exact widths TBD (fixture: A's live frame value unstated; width wraps the live frame). Exact focus TBD (fixture: pre-startup focus state unstated; admission focuses only an already-focused window). `S(S-kar-start)` + `S(S-kar-rst)` +
  `S(S-kar-spc)` + `S(S-kar-admit)` + `S(S-kar-ins)` + `S(S-kar-min)` + `S(S-kwin-resizeable)` + `S(S-kwin-winorder)`.
- Then paneru: startup windows match SessionRestore within grace (hard window_id/pid/bundle match, else unique title/bundle/identifier/role/subrole fallback with hard-collision guard and ambiguous skip); fixed E
  has no size predicate so a matched E restores into its saved strip position in saved order with Unmanaged cleared while an unmatched E follows role-gated fresh admission (float only if
  rule-assigned). Widths are not persisted and re-derive from the live OS frames, so exact widths TBD (fixture: live-frame values unstated; runtime: AX settle timing, live-only). Closed E is absent from matching (ignored-missing). Missing/corrupt/version-mismatched state files all load as absent, so matching falls back to fresh admission; stored-ID mismatches resolve through the fallback/ambiguous/ignored gates with no further counterpart. Restore writes no focus; focus TBD (host: post-restart active window). `S(S-pan-rst)` + `S(S-pan-admit)` + `S(S-pan-fresh)`; focus queued (host), widths queued (fixture+live).
- Then Ours KDE: startup foreground/hidden adoption floats fixed E;
  fixed-window user tile overrides persist across owner restart in the
  same intentional-float store, recompute fallback if missing/degraded/unmatched
  (NORMATIVE D7, User 2026-10-08; persistence delivered offline 2026-10-09).
  Startup classification implemented offline 2026-10-08;
  [startup entry fixtures](../../../kwin/tests/fixed-size-workspace-entry.test.ts)
  and [restart fixtures](../../../kwin/tests/fixed-size-admission.test.ts).
  [D7 real Planner/entry and lifecycle fixtures](../../../kwin/tests/float-intent.test.ts)
  prove success-only persistence, tile hydration, clear on re-float/close and
  complete-inventory filtering; partial omission retains membership.
  Legacy v1 without tile reads compatibly empty for overrides; degraded reads
  log and proceed empty, both recompute fixed E to untouched float.
  Native owner journey TBD; [D7 record](../../changes/archive/fixed-window-tile-override-restart.md).
- Then Ours Windows: startup recomputes automatic fixed origin; matched
  `OmniTiler.TileOverride.v1` hydrates fixed-origin tile intent before automatic
  classification on visible and retained/maximized lifetimes, including hint
  loss. Generic non-fixed wins write no tile marker; re-float/verified removal
  clears it, absent/corrupt/mismatched identity recomputes without hold/retry.
  Offline 2026-10-11; owned message-only marker readback/retained hydration
  fixtures cover both born modes with no Engine commit or native frame/style
  writes. [Record](../../changes/archive/windows-fixed-size-admission.md).
  Ordinary-app restart/native discriminator outcomes remain TBD.
- Variant hook: NORMATIVE admission/adoption scope (User 2026-10-08);
  explicit identity persistence is separately selected by R-RST-01,
  tile-override persistence delivered offline.
