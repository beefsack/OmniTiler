# Minimum-size transitions (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14).

## 12. Minimum-size transitions

<a id="scrolling-assessment-additive-existing-wide-tables-above-unchanged"></a>
<a id="r-min-01-backfill-newcomer-minimum-exceeds-shares-scrolling"></a>
### R-MIN-01: newcomer minimum exceeds shares

- Given (tree profiles): Tiled `H[A*,B]`, inner area 1080x300, gap 8; A/B minima 500x100

- Given (column profiles): `COL[C1[A*],C2[B]]`, viewport 1080x300, gap 8; A/B
  minima 500x100; newcomer C minimum 100x100. Shipped defaults apply;
  viewport recorded. Full-rect prerequisite for a faithful projection: both
  columns fully visible at these widths before admission. An independent
  strip may instead scroll to show C, so tree-style member infeasibility is
  never assumed here.

- When: Open C with minimum 100x100.

- When (column leg): open C (same action as this scenario).

- Observe: Newcomer vs existing member infeasibility; which window floats, skips or overlaps; alternative arrangement considered or not

- Observe (column leg): newcomer vs existing member infeasibility; clamp, skip, overlap
  or reflow; focus.

- Then COSMIC: Traced allocation/cropping without minimum enforcement; fixed-size admission floats separately; admission maps once at the resolved MRU anchor (axis follows the anchor's realized long edge), no alternative search; `S(S-cos-min)` + `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-admit)`; exact axis/frames TBD (F: anchor realized frames plus output dimensions unrecorded; L: client settle timing)
- Then Hyprland/Dwindle: Tiled limits off by default; opt-in clamp/recenter may overlap/overflow, not auto-float; `S(S-hyp-min)`; exact fixture TBD
- Then bspwm: Default off tiles C unconditionally with no float, skip, overlap, or alternative-arrangement search; opt-in clamps every leaf on reflow via `apply_size_hints` minimum grow (internal fence clamp is 32-based constraint minima when the sum fits, else unclamped), including existing members; `S(S-bsp-hint)` + `S(S-bsp-min)`
- Then i3: Traced tiled render without minimum clamping; fixed-size min==max admits floating while ordinary resizable tiles; float min/max clamp is float-only; exact fixture frames/native response TBD; `S(S-i3-min)`
- Then xmonad/Tall+Navigation2D: Tall `tile` allocates unconditionally with no hint consult in the cited tile path (`tileWindow` move/resizes to the allocation unconditionally); ordinary resizable C tiles even when minima exceed shares (fixed-size/transient admission floats separately per `manage`; size hints shape only float frames via `floatLocation` through `applySizeHintsContents`); no alternative-arrangement search exists in the path; with focus A* C inserts above A (order `[C*,A,B]`), so tile allocations on the 1080x300 area are C `(0,0,540,300)`, A/B `(540,0,540,150)` each (xmonad ignores the fixture gap 8 - no gap branch in the cited path; allocations before border subtraction); all shares satisfy the declared minima, and any native client overflow/clamp settlement beyond the allocation stays TBD (L: client-side settlement, not in the tile path). `S(S-xmo-layout)` + `S(S-xmo-admit)`; queued.
- Then sway: Ordinary resizable C tiles unclamped even when minima exceed shares: tiled arrange fraction-normalizes with no client-hint consult (10px zeroing only, `MIN_SANE` 100x60 gap reservation only); fixed-size min==max admits floating instead (xdg parent/fixed-size; xwayland modal/dialog/utility/toolbar/splash/fixed-size at map, runtime hints urgency-only); float clamp is config min/max plus client hints on floating resize only; exact fixture frames/native response TBD; `S(S-sway-min)` + `S(S-sway-max)`
- Then qtile/Columns: Ordinary resizable C tiles unclamped even when minima exceed shares (tiled place runs with respect_hints=false, so no client-hint consult in the Columns path and no float, skip, overlap, or alternative search); fixed-size min==max admits floating instead; any native client overflow/clamp settlement beyond the allocation stays TBD (L: client-side settlement, not in the tile path); `S(S-qti-min)` + `S(S-qti-float)`
- Then awesome/tile: Ordinary resizable C tiles prepending first (order `[C*,A,B]`, C master, A/B stacked; displayed start `H[A*,B]` implies prepared order `[A,B]`; newcomer focus via the shipped global rule) with size-hint shaping only (tile arrange applies hints without minimum-infeasibility float/skip, no alternative-arrangement search); fixed-size min==max floats implicitly instead; tile allocations on the 1080x300 area (shipped gap 0; the fixture gap 8 has no branch in the cited tile path) are C `(0,0,540,300)`, A `(540,0,540,150)`, B `(540,150,540,150)` before border/hint shaping, all satisfying the declared minima; any native client overflow/clamp settlement beyond the allocation stays TBD (L: client-side settlement, not in the tile path). `S(S-awe-tile)` + `S(S-awe-manage)` + `S(S-awe-float)`
- Then niri: C tiles as a new column right after the active A at the
  default width clamped to its min/max; A/B keep their independent widths
  (no rescale, no float, no skip, no overlap, no alternative-arrangement
  search: the open float predicate is rule/parent/fixed-height only);
  Smart activation applies (no pending fullscreen to fence it), so C
  activates and keyboard focus follows it. `S(S-nir-min)` +
  `S(S-nir-ins)` + `S(S-nir-spc)` + `S(S-nir-fltanchor)`.
- Then PaperWM: ordinary resizable C tiles unconditionally. `add_filter`
  has no fixed-size or minimum branch (Normal non-transient passes), so C
  admits as a new column at selected+1 RIGHT; the column layout sizes from
  the frame/preferredWidth with only a work-area clamp and never consults
  client minima, so no member floats, skips, or is refused by policy and
  no alternative arrangement is searched. Exact native frames/focus TBD.
  `S(S-pap-spc)` + `S(S-pap-ins)` + `S(S-pap-layout)`; frames queued.
- Then karousel/Lazy: ordinary resizable C tiles (no minimum-driven float, skip, or alternative-arrangement search); new column after the last-focused column (C1 under the A* focus, so between A and B); column width from C's preferredWidth clamped into [min,max] with the minimum capped at the tiling width; survivors keep widths; newcomer focus TBD (fixture states no protocol selecting the X11-manage vs Wayland-add fork). `S(S-kar-min)` + `S(S-kar-ins)` + `S(S-kar-fltanchor)` + `S(S-kwin-manage)` + `S(S-kwin-add)`; focus TBD (F: missing protocol selecting the newcomer activation fork).
- Then paneru: ordinary resizable C tiles as a new Single column between
  A and B (after-focus under the A* focus; no rule index under shipped
  defaults) with no minimum-driven float, skip or alternative-arrangement
  search; newcomer focus is synthesized to C. Exact native frames TBD
  (L: client-enforced minima). `S(S-pan-fresh)` + `S(S-pan-admit)` +
  `S(S-pan-stripwidth)`; frames queued.
- Then Ours KDE: Shared projection reallocates within the selected tree, without alternative-arrangement search; overconstrained members keep slots and writable members use tile origin with each extent at least its declared minimum (B6). Offline newcomer journey verified; exact native fixture/result TBD. Code: [adapter](../../../kwin/src/plan-adapter.ts) `overconstrainedEffective`, `writeGeometries`; [real-engine fixtures](../../../kwin/tests/workspace-send-engine-fixture.test.ts). Automatic victim/return policy unselected, `D(D-min-games)`.
- Then Ours Windows: Shared projection reallocates within the selected tree, without alternative-arrangement search; overconstrained members keep slots; Windows uses origin+minimum. Exact fixture/result TBD; automatic victim/return policy unselected, `D(D-min-games)`.
- Variant hook: V-START-MIN.

<a id="r-min-02-backfill-shrink-makes-members-infeasible-scrolling"></a>
### R-MIN-02: shrink makes members infeasible

- Given (tree profiles): Tiled `H[A,B]`, inner width 1220, gap 8; both minimum widths 600

- Given (column profiles): `COL[C1[A],C2[B]]`, viewport width 1220, gap
  8; A/B minimum widths 600. Shipped defaults apply.
  Full-rect prerequisite: both columns fully visible
  at 1220 before the shrink. A strip may absorb the shrink by scrolling
  rather than by member infeasibility, so reflow is never assumed here.

- When: Shrink inner width to 1080.

- When (column leg): shrink the viewport width to 1080, then grow back to 1220 in
  the same fixture without a reset (wide-row shrink and recovery).

- Observe: Existing members become infeasible; native frames, focus, float intent and recovery after width grows

- Observe (column leg): infeasible members; clamp, overlap, focus and recovery on grow.

- Then COSMIC: Deterministic re-allocation with no minimum consult: equal-share `H[A,B]` (gap 8) allocates 606/606 at 1220 and 536/536 at 1080, infeasible against the 600 minima but with no clamp, float, skip, or refocus branch in the shrink path; grow-back re-runs the same allocation restoring 606/606 at compositor level. Whether native clients overflow, clamp, or misrender in response is client-side: exact settled frames TBD (L). `S(S-cos-min)` + `S(S-cos-add)` + `S(S-cos-flttoggle)`; settle queued (L)
- Then Hyprland/Dwindle: Default off keeps the tiled shares with no clamp, float intent, or focus write on shrink (tiled size limits default off); grow-back re-runs the same allocation restoring shares at compositor level; opt-in clamps via the same size-clamp/recenter path when honored; exact native shrink/grow client frames and grow-back settlement stay TBD (L: native client response not in the arrange path); `S(S-hyp-min)`; queued.
- Then bspwm: Default off keeps the split ratio with no clamp, float intent, or focus write on shrink; grow-back re-runs the same allocation restoring shares at compositor level; opt-in clamps leaves via the same `apply_size_hints` minimum path when honored; exact native shrink/grow client frames and grow-back settlement stay TBD (L: native response not in the arrange path); `S(S-bsp-min)` + `S(S-bsp-hint)`; queued.
- Then i3: Same unclamped tiled allocation (float clamp float-only); exact shrink/grow frames/focus/float intent TBD (native response not in allocation source); `S(S-i3-min)`
- Then xmonad/Tall+Navigation2D: Same unconditional Tall sizing on shrink (pure `tile` recompute with no reflow clamp and no focus write, so focus is retained; the float map is untouched, so no shrink-driven float intent exists); tile-allocation widths are 610/610 at inner width 1220 and 540/540 at 1080 (`frac` 1/2 via `splitHorizontallyBy`; heights follow the unrecorded work-area height); exact native shrink/grow client frames and grow-back settlement stay TBD (L: native response not in the tile path). `S(S-xmo-layout)`; queued.
- Then sway: Same unclamped tiled allocation on shrink (fraction renormalize, 10px zeroing only; float clamp float-only including client hints on floating resize); exact shrink/grow frames/focus/float intent TBD (native response not in allocation source); `S(S-sway-min)`
- Then qtile/Columns: Same unclamped tiled allocation on shrink and grow-back (no reflow clamp in the Columns path; hint clamp is float-only); no float intent and no focus write run on either leg; exact native shrink/grow client frames and grow-back settlement stay TBD (L: native response not in the tile path); `S(S-qti-min)`
- Then awesome/tile: Same hint-shaped tiled allocation on shrink (no reflow clamp or float intent in the tile path; pure `do_tile` recompute with no focus write, so focus is retained); tile-allocation widths are 610/610 at inner width 1220 and 540/540 at 1080 (`mwfact` 1/2; heights follow the work-area height); grow-back re-runs the same allocation; exact native shrink/grow client frames and grow-back settlement stay TBD (L: native response not in the tile path). `S(S-awe-tile)`
- Then niri: shrink re-resolves the stored column widths with the same
  min/max clamp (widths rise to the 600 minima; heights follow the
  unchanged working height); both columns stay side by side with no
  float, skip, overlap, or refocus branch, and neighbors keep widths;
  grow-back re-runs the same resolve restoring shares. Exact native
  shrink/grow frames and grow-back settlement TBD (live-only).
  `S(S-nir-min)`; settle queued (live).
- Then PaperWM: shrink rewrites no column widths. The layout keeps sizing
  each column from live frames with only the work-area clamp and no client
  minimum consult, so neither member floats, skips, or is refused by
  policy; focus unwritten. Grow-back re-runs the same sizing from the then
  live frames, so restored widths follow whatever the native responses
  settled during the shrink: recovery TBD. Exact native frames/focus TBD
  (live-only). `S(S-pap-layout)`; recovery queued.
- Then karousel/Lazy: shrink rewrites no column widths by policy (`updateWidth` re-snaps to the closest preferredWidth clamped into [min,max]; no reflow clamp and no float intent); the strip absorbs the shrink via scroll; focus unwritten (stays); grow-back re-snaps from the then-live preferredWidths. `S(S-kar-min)` + `S(S-kar-scroll)`.
- Then paneru: the shrink applies no minimum-driven float, skip or
  refusal (no size predicate in the admission/model paths; minima only
  surface as runtime resize-shortfall observations); members keep their
  widths with the strip absorbing the shrink through the viewport, and
  focus is unwritten. Exact shrink/grow frames and grow-back recovery
  follow the live frames, TBD (L: client runtime). `S(S-pan-admit)` +
  `S(S-pan-stripwidth)` + `S(S-pan-colops)`; frames queued.
- Then Ours KDE: Same shared minimum projection; writable shrink-infeasible members use origin+minimum (B6), neither auto-floats. Offline shrink/grow recovery verified; exact native journey TBD. Code: [adapter](../../../kwin/src/plan-adapter.ts) `effectiveTargetFor`, `writeGeometries`; [real-engine fixtures](../../../kwin/tests/workspace-send-engine-fixture.test.ts). A hint-only change on KDE is not an independent dispatch trigger, `D(D-min-games)`.
- Then Ours Windows: Same shared minimum projection; current Windows origin+minimum, neither auto-floats. Exact shrink/grow journey TBD, `D(D-min-games)`.
- Variant hook: V-START-MIN.

<a id="r-min-03-backfill-oversized-sole-minimum-scrolling"></a>
### R-MIN-03: oversized sole minimum

- Given (tree profiles): Empty tiled domain, inner area 1080x600

- Given (column profiles): empty strip, viewport 1080x600; incoming A has declared
  minimum 1200x500. Shipped defaults apply; viewport recorded. A strip may
  show an overflowed sole column with scrolling rather than tiling it
  into the viewport, so sole-leaf tiling is never assumed here.

- When: Open A with declared minimum 1200x500.

- When (column leg): open A (same action as this scenario).

- Observe: Tile/flag vs automatic float; overflow remains even without siblings

- Observe (column leg): tile/flag vs automatic float; overflow with no siblings.

- Then COSMIC: Same tile allocation/cropping (ordinary resizable A tiles; fixed-size min==max exception is not an oversized-min policy); exact sole-leaf native frame TBD (F: output/work-area dimensions unrecorded for this leg; L: client settle timing)
  `S(S-cos-min)` + `S(S-cos-admit)`; frame queued (F+L)
- Then Hyprland/Dwindle: Ordinary resizable A tiles as the sole leaf at the full area (tiled size limits default off, no automatic float); opt-in applies the size-clamp/recenter path when honored; exact sole-leaf native client frame TBD (L: client-side settlement, not in the tile path); `S(S-hyp-min)`; queued.
- Then bspwm: Default off tiles A ordinarily with no automatic float (float only via min==max rule or explicit state); opt-in honored hints grow the leaf at origin via the minimum clamp, so overflow stays with the client; `S(S-bsp-hint)` + `S(S-bsp-min)`
- Then i3: Ordinary resizable A tiles unclamped even when its minimum exceeds the work area (fixed-size exception is min==max, not oversized-min); float clamp float-only; exact sole-leaf native frame TBD; `S(S-i3-min)`
- Then xmonad/Tall+Navigation2D: Ordinary resizable A tiles unconditionally even when its minimum exceeds the work area (sole entry takes the full rect via the `n <= nmaster` tile branch, so the tile allocation is the full `(0,0,1080,600)`; the fixed-size/transient float exception is min==max/transient only, not an oversized-min policy); `tileWindow` applies the allocation unconditionally, so overflow stays with the client; exact sole-leaf native client frame TBD (L: client-side settlement). `S(S-xmo-layout)` + `S(S-xmo-admit)`; queued.
- Then sway: Ordinary resizable A tiles unclamped even when its minimum exceeds the work area (fixed-size exception is min==max, not oversized-min; xwayland type/modal branches likewise admission-only); float clamp float-only; exact sole-leaf native frame TBD; `S(S-sway-min)` + `S(S-sway-max)`
- Then qtile/Columns: Ordinary resizable A tiles unclamped even when its minimum exceeds the work area (the fixed-size float exception is min==max, not oversized-min), so overflow stays with the client with no automatic float; exact sole-leaf native client frame TBD (L: client-side settlement); `S(S-qti-min)` + `S(S-qti-float)`
- Then awesome/tile: Ordinary resizable A tiles unconditionally even when its minimum exceeds the work area (sole entry takes the full `(0,0,1080,600)` rect; the fixed-size float exception is min==max only (transient affects tags/screen per `S(S-awe-manage)`, not implicit float), not an oversized-min policy); hint shaping only, so overflow stays with the client; newcomer focus via the shipped global rule; exact sole-leaf native client frame TBD (L: client-side settlement). `S(S-awe-float)` + `S(S-awe-tile)` + `S(S-awe-manage)`
- Then niri: A tiles as the sole column (ordinary admission; min-width
  alone never floats: the open predicate is rule/parent/fixed-height
  only); its width resolves from the default and clamps up to the 1200
  minimum, so the column overflows the 1080 viewport with the excess kept
  by the client (no automatic float, no skip); Smart activation applies.
  `S(S-nir-min)` + `S(S-nir-ins)` + `S(S-nir-spc)`.
- Then PaperWM: ordinary resizable A tiles even when its minimum exceeds
  the viewport. Admission has no minimum or fixed-size branch and the
  layout applies only the work-area clamp, so A opens as a sole tiled
  column (no automatic float); overflow stays with the client. Exact
  sole-leaf native frame TBD. `S(S-pap-spc)` + `S(S-pap-ins)` +
  `S(S-pap-layout)`; frame queued.
- Then karousel/Lazy: ordinary resizable A tiles as the sole column at the start of the empty grid (no minimum-driven automatic float; the minimum caps at the tiling width, so the column takes 1080); overflow stays with the client; newcomer focus TBD (fixture states no protocol selecting the X11-manage vs Wayland-add fork). `S(S-kar-min)` + `S(S-kar-ins)` + `S(S-kwin-manage)` + `S(S-kwin-add)`; focus TBD (F: missing protocol selecting the newcomer activation fork).
- Then paneru: ordinary resizable A tiles as the sole Single column (no
  minimum-driven automatic float; no size predicate in admission);
  overflow stays with the client and newcomer focus is synthesized to A.
  Exact sole-leaf native frame TBD (L: client runtime). `S(S-pan-fresh)` +
  `S(S-pan-admit)` + `S(S-pan-stripwidth)`; frame queued.
- Then Ours KDE: Core projects the sole leaf and flags its violated minimum; writable KDE raises the violated extent at tile origin (B6), allowing overflow. Offline oversized sole-leaf/offset-domain journey settles quietly. Code: [adapter](../../../kwin/src/plan-adapter.ts) `overconstrainedEffective`, `writeGeometries`; [real-engine fixtures](../../../kwin/tests/workspace-send-engine-fixture.test.ts). Floating cannot make this minimum fit the work area; exact native journey TBD, `D(D-min-games)`.
- Then Ours Windows: Core projects the sole leaf and flags its violated minimum; writable Windows raises width at tile origin. Floating cannot make this minimum fit the work area; exact native journey TBD, `D(D-min-games)`.
- Variant hook: V-START-MIN.

<a id="r-min-04-pure-geometry-exhaustion"></a>
### R-MIN-04: pure geometry exhaustion (no client minima)

- Given (tree profiles): `H[A,V[B*,C]]` with B focused, gap `g>0`,
  both of B's allocated extents smaller than `g+2`, and room elsewhere:
  A is wide enough to split; C has most of the nested parent's height.
  Retain the unequal shares before opening N (a synthetic exhaustion
  fixture, not an observed pixel layout).
  All windows ordinary resizable with zero declared minima (no min/max hints
  anywhere); B6 never triggers here. The 800x600/gap-8/13-14-window report is
  a user-supplied motivating instance, not verified legacy behavior and not a
  universal cap.
- Given (column profiles): `COL[C1[A],C2[B]]` narrow viewport with newcomer C
  at shipped defaults; viewport recorded. A strip may absorb narrowness by
  scrolling, so tree-style infeasibility is never assumed here.
- When: Open newcomer N with B still focused (tree leg) / open C (column
  leg). Native verbs/config: same ordinary-open verbs as R-MIN-03 per profile.
- Observe: the nested parent's extent unchanged versus growth stealing from A;
  degenerate tiny/zero allocation versus floors; alternative-arrangement
  search versus none; focus.
- Then COSMIC: N admits at the MRU anchor B; `new_group` wraps B only (C keeps
  its slot, A untouched, the `V[B,C]` parent extent does not grow);
  `update_positions` subtracts gaps with no floor, so degenerate tiny geometry
  is possible. The ordinary-window path floors configured height at zero
  after subtracting any server decoration, but not width; no capacity
  refusal and no alternative-arrangement search in
  the traced path; exact settled frames and client/native settlement TBD (L).
  `S(S-cos-geometry)` + `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)`
- Then Hyprland/Dwindle: ordinary active-tiled anchor B (same workspace under
  `use_active_for_splits`), else closest node; the new parent wraps B only
  (no outside share steal, A untouched); the MAXSIZE-predicted float diversion
  is max-hint only, distinct from exhaustion; no minimum/geometric admission
  check in the traced path; `recalcSizePosRecursive` applies a `.noNegativeSize`
  zero floor to pre-gap node boxes. Final tiled sizing subtracts gaps and
  reserved decoration space without a floor; zero/negative target sizes can
  result (not a min-20 admission floor; the tiled `MIN_WINDOW_SIZE` clamp is
  opt-in, default off). Exact degenerate
  frames TBD. `S(S-hyp-geometry)` + `S(S-hyp-ins)` + `S(S-hyp-min)`
- Then bspwm: TBD (exact nested-tiny ancestry unresearched; no claim)
- Then i3: N inserts after the focused tiling sibling B within the existing
  parent `V[B,C]` (`tree_open_con` + `_con_attach`); `con_fix_percent`
  assigns each zero-percent newcomer the mean of the existing percents, then
  renormalizes the same parent only (N can take room from C, but B does
  not grow; no parent growth or ancestor steal);
  no exhausted-size check in the traced path; render floors client/frame
  dimensions at >=1 via `rect_sanitize_dimensions`; exact fixture frames and
  native response TBD. `S(S-i3-geometry)` + `S(S-i3-ins)` + `S(S-i3-min)`
- Then xmonad/Tall+Navigation2D: fixture-inapplicable (flat master/stack Tall
  cannot construct this retained nested-share fixture); no geometry asserted here.
  `S(S-xmo-layout)`
- Then sway: N anchors after the focus-inactive tiling sibling B within
  `V[B,C]` with ordinary newcomer focus; arrange fraction-normalizes the same
  parent (N takes a share from B/C, no parent growth or alternative search);
  `total_gap=min(inner*(n-1),max(0,parent-MIN_SANE*n))`
  with `MIN_SANE` W100/H60 is a gap budget only, not tile minima; after
  dimension calculation `w<10||h<10` zeroes both (both bounds live in
  `arrange.c`, `node.h` defines `MIN_SANE` only); no explicit numeric capacity
  bound in the traced Tier 1 admission/allocation paths (path-scoped only, no
  global no-limits claim); exact fixture frames and native response TBD.
  `S(S-sway-geometry)` + `S(S-sway-ins)` + `S(S-sway-min)`
- Then qtile/Columns: TBD (exact nested-tiny ancestry unresearched; no claim)
- Then awesome/tile: TBD (exact nested-tiny ancestry unresearched; no claim)
- Then niri: TBD (exact nested-tiny ancestry unresearched; no claim)
- Then PaperWM: TBD (exact nested-tiny ancestry unresearched; no claim)
- Then karousel/Lazy: TBD (exact nested-tiny ancestry unresearched; no claim)
- Then paneru: TBD (exact nested-tiny ancestry unresearched; no claim)
- Then Ours KDE: ordinary admission proposes structurally (`insert_tiled`
  wrap of B, A/C untouched), but projection reserves one pixel per child
  after gaps: along the split axis, `extent-gap*(n-1)>=n` must hold.
  Projection rejects unrepresentable geometry (projection error or
  any leaf rect `w<=0||h<=0` refuses `MalformedTopology` with no mutation, so
  the outside parent never grows by silent steal); positive tiny shares still
  project. `S(S-ours-admission-capacity)` + `S(S-ours-ins)` + `S(S-ours-admit)`
  @15faf1e4; exact tiny-share geometry and native settlement TBD, no
  universal cap
- Then Ours Windows: same propose-with-projection-gate leg as Ours
  KDE via the shared Engine. `S(S-ours-admission-capacity)` +
  `S(S-ours-ins)` + `S(S-ours-admit)` @15faf1e4; exact geometry and native
  settlement TBD, no universal cap
- Variant hook: none (OPEN; user decision required, nothing selected).
