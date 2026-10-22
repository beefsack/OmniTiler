# Reference-WM Outcome Matrix

Purpose: canonical scenario/outcome evidence feeding a future
functional spec. It records observed or source-evidenced outcomes per
scenario for reference WMs and our KDE/Windows behavior. Variant hooks
are provisional, to discuss. Recorded decisions in
[decisions.md](../decisions.md) select supported variants and pending
configuration; a foreign outcome here does not automatically become
supported, and variants are spec hooks only, not implemented settings.
Defaults/settings selection uses the user's 2026-10-12
[reference tiers](../decisions.md#reference-tiers); evidence tallies are retained.

Row addition rule: add the shortest action sequence for an uncovered
behavior or ambiguity (reference WMs disagree, or our behavior is undecided).
Reuse existing coverage rather than duplicating a scenario with a
trivially different start state.

### Fixture-driven native state discriminators

| Variant | Minimal action sequence | Observed KDE outcome | Unsupported outcome / decision |
| --- | --- | --- | --- |
| Client-driven tiled geometry | Tile A/B; have A request a different size without activation or output/desktop change; observe; admit C | Wayland and X11/XWayland: A keeps client-requested geometry, no world dispatch in bounded capture; admission triggers reconciliation and reclaims A's tile. Current hooks observe homing/activation/inventory, not ordinary frame changes | Desired immediate correction versus retained client geometry and reference-WM outcomes TBD; no new policy selected. [Evidence](../changes/archive/kde-dev-scope-fixture-integration.md#tiler-findings-and-host-limits) |
| X11 urgency hint versus attention state | With B active, request critical attention on A; clear A's attention; activate A | WM_HINTS urgency clears, but KWin DEMANDS_ATTENTION persists until activation. Accepted clear is a client-hint acknowledgement, not compositor-state withdrawal | Other hosts/reference WMs and visual urgency presentation TBD; no tiler urgency policy changed. [Evidence](../changes/archive/kde-dev-scope-fixture-integration.md#tiler-findings-and-host-limits) |

<a id="cursor-first-output-targeting-discriminators"></a>
### Cursor-first output-targeting discriminators

User 2026-10-20 [accepted configurable targeting](../decisions.md#cursor-first-output-targeting)
on the Tier 1 evidence, replacing R-OUT-05 / REQ-OUT-05's focused-output default
with cursor output and a focused-output option. Windows switch/fresh-placement
adoption is [accepted](../changes/core-world-model-rearchitecture.md#stage-2c-accepted-outcome-2026-10-22);
KDE/macOS adoption and focus/warp behavior remain pending.
Workspace-switch and three-way new-window settings are recorded in that decision.
The user's COSMIC recollection is uncertain; existing
[R-OUT-05](reference-outcomes/multi-output.md#r-out-05-open-a-window-with-two-occupied-outputs)
is a historical map-only tally (static-pointer fixture; keyboard focus vs
cursor/active-output conflated there), not pointer routing and not undisputed
focused-output evidence. Tier 1 motion-first evidence now lives in
[R-OUT-05a](reference-outcomes/multi-output.md#r-out-05a-motion-then-ordinary-open),
[R-OUT-08](reference-outcomes/multi-output.md#r-out-08-numbered-switch-versus-send-with-opposite-cursor),
[R-OUT-09](reference-outcomes/multi-output.md#r-out-09-click-on-empty-area-of-the-other-output) and
[R-OUT-10](reference-outcomes/multi-output.md#r-out-10-focuscursor-coupling-defaults).
R-OUT-05a separates same-output insertion, opposite-output no-token map, and
built-in shortcut launch with propagated startup context.

| Variant | Minimal action sequence | Direction / decision | Unsupported outcome / evidence |
| --- | --- | --- | --- |
| Workspace switch with opposite cursor | Focus A on occupied L; put cursor on occupied R without changing focus; select another workspace | User 2026-10-20 accepted: cursor output default (switch R), focused-output option (switch L) | Tier 1 discriminating fixtures in [R-OUT-08](reference-outcomes/multi-output.md#r-out-08-numbered-switch-versus-send-with-opposite-cursor): fresh number lands R in COSMIC (trailing-empty index on the active output) and Hyprland (created on the focus monitor), L in i3/sway (created on the focused output); per-output relative-next advances R (COSMIC plain NextWorkspace, Hyprland `m+1` to ws 6) vs L (i3/sway `next_on_output` to ws 2). Existing global numbers stay owned (caveat in row). No consensus; Tier2/3 TBD. Keys `S(S-cos-cursor)`, `S(S-hyp-cursor)`, `S(S-i3-ffmoff)`, `S(S-sway-ffmoff)`. Ours: core covers all four selection verbs; Windows native trailing selection passes both configured targets |
| Workspace send with opposite cursor | Reset the same fixture; send focused A to the next/previous workspace | User 2026-10-20 Decision 1c: act on A's output L regardless of cursor; relative targets also count on L, no setting | Tier 1 send-source consensus 4/4 (all act on focused A on L); selected relative-target rule has 3 to 1 support. Target-scope split in [R-OUT-08](reference-outcomes/multi-output.md#r-out-08-numbered-switch-versus-send-with-opposite-cursor): relative +1 targets ws 6 (focus-monitor-relative, R) in Hyprland while the subject stays A from L, versus ws 2 (focused-output-relative, L) in COSMIC/i3/sway. Ours: core relative/numbered cases and Windows native numbered-stay pass; native relative-send discriminator remains TBD |
| Launch same versus other output | Focus A beside B on L; retain a distinct layout on R; launch C with cursor on L; reset and launch C with cursor on R, retaining A's focus before launch | User 2026-10-20 accepted three-way placement: COSMIC default cursor-output/MRU; Hyprland cursor-output/cursor-closest; i3/sway focused-output/MRU | Tier 1 motion-first evidence supporting the accepted R-OUT-05 override: COSMIC lands R splitting MRU Y; Hyprland lands R splitting the cursor-closest node; i3/sway with FFM off stay on L. Same-output selected-focus anchor holds in all four; split geometry differs; other-output insertion has no universal consensus. Evidence: [R-OUT-05a](reference-outcomes/multi-output.md#r-out-05a-motion-then-ordinary-open) |
| Foreground handoff at map | Focus A on L, cursor on R; admit C while the foreground sample is absent, unknown, or names newly mapped C; repeat with a genuine existing B on R focused before admission | Core retains A's output for the focused-placement leg across the handoff; genuine current managed focus B supersedes it. Cursor-first placements still choose R. Startup/restart adoption is separate | Core and Windows native-seam handoff cases pass; Windows native foreground-stealing admission passes both directions under all three placements. Exact reference-WM handoff outcomes and native absent/unknown samples remain TBD; no reference result inferred |
| Optional cursor/focus coupling | With both options off, move cursor L -> R; explicitly focus A on L; repeat each leg with only the corresponding option on | User 2026-10-20 Decision 1d: focus-follows-cursor on/off, default off; cursor-warp setting default off (COSMIC). i3/sway preset enables warp when focus crosses outputs; Hyprland preset leaves warp off pending its own verb-dependent evidence row | Tier 1: FFM configurable in all four, default-off only in COSMIC; all three other Tier 1 defaults are on. Warp configurable in all four, but general cursor-following and cross-output warp are distinct. Ours retains the selected settings; active behavior/preset warp remains pending. Evidence: [R-OUT-10](reference-outcomes/multi-output.md#r-out-10-focuscursor-coupling-defaults); empty-click split in [R-OUT-09](reference-outcomes/multi-output.md#r-out-09-click-on-empty-area-of-the-other-output) |
| Hyprland preset warp by verb | Focus A on L with cursor on L; explicitly focus B on R; reset; switch to R's workspace; reset; send A to R with follow | Record each verb's cursor position separately before choosing Hyprland preset warp behavior; leave warp off meanwhile | Hyprland per-verb outcomes TBD; no uniform warp behavior inferred from the other presets or general coupling evidence. Ours not implemented |

<a id="empty-workspace-focus-across-outputs"></a>
### Empty-workspace focus across outputs

Related to [cursor-first targeting](#cursor-first-output-targeting-discriminators).
The user is unsure of the desired behavior; gather Tier 1 outcomes before deciding.

| Variant | Minimal action sequence | Discriminator | Unsupported outcome / evidence |
| --- | --- | --- | --- |
| Switch to empty, then send | Keep B visible on R; focus A on L with cursor on L; press Win+digit to select an empty workspace on L; record focused output/window; immediately issue a workspace send | Does focus stay on empty L with no focused window (send has no subject), or move to B on R (send moves B)? | User physical 2026-10-20: focus moved to another output's window and the following send moved it. Desired outcome undecided; COSMIC, Hyprland, i3 and sway outcomes TBD. No fix or focus policy selected |

<a id="cross-output-focus-edge-discriminator"></a>
### Cross-output focus: edge-nearest versus destination MRU

User direction 2026-10-20: last-focused selection applies only to switching
workspaces on one output; crossing an output boundary should select the window
nearest the crossed edge. This conflicts with the current Engine last-focused
tiled-leaf proposal in [R-OUT-03](reference-outcomes/multi-output.md#r-out-03-focus-left-across-outputs),
whose single destination tile does not distinguish these rules.

| Variant | Minimal action sequence | User direction / discriminator | Unsupported outcome / next evidence |
| --- | --- | --- | --- |
| Cross right with destination MRU away from edge | Equal aligned outputs: `O1=W1`, `O2=H[W2 W3]`, W2 nearest O2's left edge. Focus W3, then W1 without changing O2's MRU; press Win+Right once | Edge-nearest chooses W2; destination MRU chooses W3. Keep layouts, membership and current workspaces unchanged | User dogfood trace 2026-10-20 selected W3, expected W2; W3's MRU identity remains unverified. Next: confirm this row and Tier 1 functional-matrix cells (COSMIC, Hyprland, i3, sway outcomes all TBD), then inspect the trace to distinguish MRU behavior from a bug. Likely shared core behavior; any core change needs user approval |

### Admission and geometric capacity (source audit 2026-10-16)

- Ordinary focused anchors: reuse [R-INS-01/04](reference-outcomes/insertion.md).
- Exhausted anchor, room elsewhere, no client minima:
  [R-MIN-04](reference-outcomes/minimum-size.md#r-min-04-pure-geometry-exhaustion).
- Serial maps versus a whole roster:
  [R-INS-09](reference-outcomes/insertion.md#r-ins-09-burst-of-maps-versus-roster-delivered-together).
- Enable/adoption ordering and geometry preservation: reuse
  [R-START-01..03](reference-outcomes/restart-persistence.md#r-start-01-enable-over-a-2x2-float-field);
  the neighboring toggle/layout-switch/restart/reload discriminators are in
  that section. B6/startup decisions stand, with selected geometric fallback
  and complete mixed-roster seeding below.

<a id="best-effort-capacity-discriminators-ours-to-be-open"></a>
### Best-effort capacity discriminators

- User selected D1-D10 2026-10-17: largest-splittable/float defaults, no automatic
  workspace spill; explicit sends remain supported.
- [Selected semantics](../decisions.md#best-effort-admission) and
  [delivery/evidence](../changes/archive/best-effort-admission.md).
  These are variants of R-MIN-04, R-INS-09, R-START-01..03 and the named
  send/move/drag rows, not new reference evidence. Core/KDE delivered offline;
  exact reference/native outcomes remain TBD. Windows/macOS adoption is pending.

| Variant | Minimal action sequence | Selected discriminator / offline contract | Unsupported outcome / native evidence |
| --- | --- | --- | --- |
| Minimum versus geometric exhaustion | Run R-MIN-01 with all logical extents positive but infeasible declared minima; independently run zero-hint R-MIN-04 | D1 geometric-only policy preserves B6 in the first leg. Float default does not remove declared-minimum overlap | Native settlement TBD; no new minimum threshold or native squeeze inferred |
| Largest versus relative versus no-search | In R-MIN-04 make B unsplittable, sibling C splittable, and outside A strictly larger than C; open N while B focused | D2: largest chooses A (default); relative chooses C; no-search invokes D3 on original B. Plain long-edge equal old/new wrapper, focus newcomer whether tiled/floated, never anchor | Native activation TBD; no short-axis/rebalance/output/workspace search |
| Relative rings and equal-area ties | Make B's immediate sibling subtree contain two splittable leaves; put a larger splittable leaf at the next ancestor; open N. Repeat with equal-area candidates on opposite sides of the anchor | D2 sibling-subtree leaves before higher rings, index-distance then child-order DFS; largest ties use DFS then lifetime ID; never wrap parent | Exact native geometry TBD; no screen-distance/MRU interpretation |
| No split anywhere | In a positive 1x1 usable domain with gap 8 and ordinary zero-hint A, open N; repeat one accepted report | D3/D4 default float N at opening frame, diagnostic capacity origin, keep A tiled; D9 alternative positive overflow. Focus N once subject to existing safety/freshness fences | Native frame/focus TBD; no universal window-count cap |
| Capacity return and explicit retry | Capacity-float N; close another tiled leaf or enlarge output; revisit workspace and repeat report; explicitly tile N; repeat with still-insufficient space | D5 no automatic retry; explicit retry tiles once if feasible, otherwise stays float without false override. Membership persists like ordinary floats across restart | Native restart/retry TBD; no retained old tile slot |
| Unfloat target versus aggregate settlement | Explicitly tile N; read back N exactly at its target while a rewritten neighbor differs by one pixel and another unwritten entry remains pending | Windows `unfloat-mismatch` summarizes the complete geometry pass, not N alone. Exact inset arithmetic and a target-tiled aggregate-settlement seam are covered offline; do not interpret this diagnostic as failed target admission | Tick 275 target matched; neighbor native one-pixel cause remains TBD without delayed fresh outer/visible readback ([evidence](../changes/windows-best-effort-admission-live.md#offline-unfloat-discriminator)). No tolerance selected |
| Windows awareness versus host placement | Create identical hidden helpers in unaware/system-aware/PMv2 arms; show, adopt gaps 8/8 -> 12/12, stop owner, replay each outer request with direct PMv2 `SetWindowPos`; compare outer/DWM/DPI readbacks | At 125% scale, two unaware-window outer offsets reproduce without the adapter; system-aware and PMv2 arms match. Same minimum policy, above-minimum targets; no adapter correction or tolerance selected | Numeric hints differ; direct first read follows 700 ms, then five seconds. Synchronous readback, representability, other scales and compensation remain TBD ([evidence](../changes/windows-best-effort-admission-live.md#controlled-awarenessdirect-write-discriminator-2026-10-18)); not an unfloat-neighbor verdict |
| Enable/startup clean fit versus exhausted seed | Enable over a clean 2x2 field; separately enable over overlapping many-window geometry whose sequential focused anchor exhausts; repeat after restart | D8 clean fit first, otherwise existing spatial/focus-last serial seed with D2/D3, complete atomic mixed roster. D5 persisted floats remain floated; diagnostic cause may normalize to ordinary | Native batch order and original reference enable discriminators remain TBD |
| Named send versus R4 whole-wrap | Send tiled N to an exhausted destination with room at another leaf; independently move N across an output edge where the mandated whole-target R4 wrap cannot project | D6: named send may use D2 within destination, then D3; R4 preserves whole-target shape, bypasses D2, then D3. No spill to a third workspace/output; source retained until verified transfer | Capacity transfer/follow/stay/frame remap TBD; macOS native-Space sends remain capability-gated |
| Drag fallback preview agreement | Drag N to a resolved exhausted edge; observe preview; release there; repeat after a capacity change between preview and finish | D6 same preview/finish, no alternate-target search, fresh validation. Float fallback currently refuses/snaps back without an honest float preview; overlap uses the same gate | Native preview/release/remap TBD |
| Existing geometry becomes impossible | Settle unequal-age A/B/C; increase gaps or shrink bounds until strict projection fails; repeat report, restore capacity | D7 float shortest newest-successful-admission-first prefix, preserve survivor topology/shares, native focus and live frame, persist floats, no automatic re-tile. D9 alternative retains positive overflowing tiles | Native shrink settlement TBD; minimum-only shrink stays B6 |
| Hidden versus visible gap repair | From the same retained tiled topology, hide the domain, increase supported gaps until strict projection fails, then reveal; compare with the same gap change while visible, including direct 8/8 -> 64/64 versus staged 8/8 -> 32/32 -> 64/64 | D7 repair and ordinary-float registration on committed victims; no hidden geometry writes. Match retained tree/shares, admission history and fresh hints. Offline matched-tree replay repairs both gap sequences; stale pre-adoption all-tiled rows in the Windows reveal sequence undo that repair by re-admission | Windows 2026-10-18: hidden frames preserved, but reveal kept 40 tiles where visible shrink produced 30 floats. Windows stale-row defect reproduced offline; original native tree/session preimage unavailable and corrected native outcome TBD ([evidence](../changes/windows-best-effort-admission-live.md#offline-hidden-repair-discriminator)) |
| Gap adoption before another intent | Retain a tiled domain, grow supported gaps to require repair, then immediately focus, move, resize, send or finish a gesture before an ordinary reconcile tick | D5/D7 committed floats remain floats unless the action explicitly retries that subject; stale pre-adoption observation flags must not undo repair | Windows non-select callsites are audit candidates only; per-route causal reproductions and native outcomes TBD. No blanket correction or shared semantic change selected |
| Protected remainder and game safety | Put a retained fullscreen/maximized reserved slot beside normal A; shrink until projection fails; repeat with only protected slots or nonpositive usable area | D7 only safe eligible victims, preserve overlays/fixed overrides, defer gestures, never classify refusal as capacity; impossible protected remainder stays narrowly held | Native safety TBD; no live game probe performed |
| Apply settings versus repeated full replies | Capacity-float N; change anchor/no-space settings; resend identical observations, including after manual focus/frame changes; separately start a new admission | D10 next-decision only, no reclassification/retry; stable targets/operation identities, no repeated focus/raise/transfer/frame reset. Actual new geometry may repair once | Windows/macOS consumer adoption and native settings pickup TBD |

### KDE automatic fixed-float stacking discriminators

Selected D8 amendment and option A (User 2026-10-17/18); Windows unchanged.
[Current evidence and blockers](../changes/kde-automatic-float-stacking.md).

| Variant | Minimal action sequence | Selected / offline outcome | Native or unsupported outcome |
| --- | --- | --- | --- |
| Automatic float versus tiled focus | Open fixed-size F over tile A; focus disposable terminal T; refocus F; reconcile | F above tiles, no F frame/focus/below writes; automatic survivor-focus intent cannot activate A over focused F; explicit command focus unchanged | Initial native F had one admission-focus write, corrected offline; corrected native repeat/physical Alt+Tab and Steam TBD |
| Existing and later native flags | Admit F with keep-below, then separately pre-existing keep-above; separately set below after project admission | Existing below: zero stacking writes through tile/stop; existing above: untouched. Tentative (user review pending): later below wins without reassertion | Native flag matrix TBD |
| Fullscreen conflict and exit | Admit F above tiles; focus fullscreen G in the same scope; exit fullscreen | Tentative (user review pending): release owned above once, reacquire on exit; G receives no writes | Only isolated focused-fullscreen stand-in sampled; concurrent F/G coverage and actual game TBD |
| Unload with surviving float | Admit F; unload/reload script without closing F; focus fullscreen G | Actual unload has no JavaScript stop callback; former project above becomes indistinguishable from pre-existing above and is not owned for suspension | Unresolved safety blocker, no selected recovery ownership; native journey TBD |
| Modal sibling | Open fixed-size window-modal F; focus T on same workspace; refocus F; reconcile | Keep modal relationship and native frame/focus; project adds only eligible above | Modal flag/above and API focus cycle sampled, zero switch-time controller actuations; physical interaction and corrected-build repeat TBD |

### Full-state reply settlement discriminators

Selected core/KDE contract, delivered offline; native/reference outcomes
remain TBD unless separately established.
[Combined core/workspace contract](../changes/archive/core-full-state-replies.md).

| Variant | Minimal action sequence | Established evidence / candidate distinction | Unsupported outcome / decision |
| --- | --- | --- | --- |
| New input before reply | Dispatch a layout report for A/B; start dragging B before its reply returns; deliver that reply before the next report is dispatched | Selected watermark checks the latest synchronous invalidation; offline tests prove zero stale setters and observation-only catch-up without replay | Exact native/reference ordering and visual drag outcome TBD |
| Lost transfer reply versus refusal | Send B to another workspace; withhold the reply before native assignment; deliver a fresh full observation. Separately deliver the reply but refuse native assignment | D3 retains an unattempted assignment under a stable identity; confirmed refusal settles; possibly-executed ambiguity holds without blind repeat | Native follow/arrival/refusal and reference outcomes TBD; no repeat without fresh unambiguous proof |
| Workspace selection without Planner | With two existing workspaces, make Planner unavailable; select the other, then request trailing creation | D2 selects Planner-only workspace policy and narrow refusal while unavailable, superseding [AR6](../changes/architecture-review-ar6-workspaces.md#integration-route-and-blocker); native observation/safety remain independent | Native responsiveness and reference outcome TBD |
| Repeated state after manual focus | Directionally focus overlapping B once; manually focus A; deliver another otherwise identical desired-state snapshot | D3 selects at-most-once focus/raise/stack effects; repeated snapshots do not restore active/stacking state | Native focus/raise/stack settlement and reference outcomes TBD |
| Silent final notification loss | Settle A/B; suppress the final native change notification for B; generate no further input | D5 selects event-driven full reports and dirty/ack recovery, no idle audit; a missed final event needs a later trigger | Exact native/reference notification-loss outcome TBD |

### Windows minimum-query timeout discriminator

| Variant | Minimal action sequence | Established evidence | Unsupported outcome / decision |
| --- | --- | --- | --- |
| Temporary missing minimum during settled tiling | Tile ordinary A/B with A's usable minimum; time out A's next `WM_GETMINMAXINFO` query; return the same hint on the next query; repeat without user input; then return a successful empty query | User 2026-10-16 correction selects last-good declared-hint reuse on timeout with no time bound, replaced only by a successful query (including no hint), dropped on close/identity change. Windows four-member synthetic regression holds targets through repeated hint/timeout transitions, reacts to fresh bounds and clears on successful empty; normal diagnostics correlate identity, query, requested/readback geometry and suppression. [Diagnosis, delivery and physical plan](../changes/windows-launcher-geometry-fighting.md) | Exact launcher/reference geometry and native physical convergence TBD; original log lacked rectangles and class/process attribution. No learned limit, auto-float or KDE/macOS policy change; existing B6 minimum-infeasible origin placement remains unchanged. |

### Windows single-output dogfood discriminators

Variants of [R-WS-09](reference-outcomes/workspaces.md#r-ws-09-select-ws2-select-ws1-return-focus-and-viewport).
[Diagnosis and delivery](../changes/windows-single-output-dogfood.md).

| Variant | Minimal action sequence | Established evidence / selected contract | Unsupported outcome / pending decision |
| --- | --- | --- | --- |
| Native-focused overlay departure | On WS1 focus ordinary A, then natively focus managed maximized B without a current snap origin; select WS2 and return to WS1. Repeat with managed fullscreen B and stale origin | Remember the independently verified dispatch-time foreground member B, including retained max/fullscreen identity/token; restore it through existing eligible-focus targeting. Missing/stale snap origin does not override native focus. Offline departure/return-target and geometry-exemption regressions pass. User physical acceptance 2026-10-19: maximized windows regain focus on workspace return | Exact reported failing switch is not identifiable in the incident log. Fullscreen and every keyboard/pager/history variant remain TBD; existing fullscreen/elevation activation fences stay in force, with no delayed retry |
| Pager native versus HWND order | Overlap managed A/B with native topmost having the smaller HWND; inspect pager, reverse native stacking, then select away and inspect hidden workspace; close/reopen A | Use existing EnumWindows top-to-bottom observation, including retained overlays; paint miniatures in reverse. Hidden members keep last observed relative order. Full identity pruning and same-turn first-admission order covered offline; no extra native query. User physical acceptance 2026-10-19: pager respects native z-order | Exact native update latency and wider hidden/close/reuse physical matrix TBD; never-observed members use deterministic key order until observed |
| Elevated fullscreen blast radius | With ordinary managed A/B present, foreground an elevated fullscreen game; close B or change A's frame, then return to the ordinary desktop | User-approved 2026-10-19: non-activating geometry reconciliation for other existing verified managed windows only, foreground excluded/untouched, automatic focus blocked and overlays suppressed. Invalid/unreadable foreground remains fully blocked; managed/holdable fullscreen unchanged. Fresh admission/native unmaximize/marker/visibility effects defer to resume. Real Engine/writable-scoping regressions pass; native setters stubbed. User physical acceptance 2026-10-19: fix effective | Wider game/performance variants TBD; no quantified imperceptibility claim |

### Windows Xbox return discriminator

| Variant | Minimal action sequence | Observed evidence | Unsupported outcome / next discriminator |
| --- | --- | --- | --- |
| Owned fullscreen across Xbox entry/return | Tile A/B; fullscreen A once; if Xbox mode opens, return through Task View; inspect A before selecting another workspace; then separately round-trip workspaces and exit project fullscreen | User incident: fullscreen consumed; zero eligible observations; generic fullscreen suspension/resume flaps; later workspace hide of origin; return `ok` with no focus; later project-fullscreen exit; gestures refused as floating/exception. [Diagnosis](../changes/windows-gaming-coexistence.md#incident-diagnosis-and-prevention-options-2026-10-16) distinguishes log facts from OS attribution | Exact hide/cloak/reveal and exception/mode transitions TBD; inspect owned claims, visible/cloaked/fullscreen facts and exception lane at each boundary. No heuristic Xbox detector, automatic exception clear or new shortcut policy selected |

### Windows Xbox shortcut configuration discriminators

| Variant | Minimal action sequence | Established evidence | Unsupported outcome / next discriminator |
| --- | --- | --- | --- |
| Ctrl-first Compatible alternates | With Xbox mode enabled and Game Bar retained, apply Compatible; physically press Ctrl+Win+G and Ctrl+Win+F11 on an ordinary tiled target; hold, change modifier release order, then tap plain Win | User verified Ctrl+Win+G does not open Game Bar; approved exact alternates implemented with offline preset/validation/classifier hold tests. [Delivery](../changes/windows-gaming-coexistence.md) | Actual float/fullscreen dispatch without Game Bar or any Windows function/Xbox activation, physical holds/releases and plain Win TBD; offline tests do not establish OS nonactivation |
| Authentic configuration remedies | Outside a game/capture, open each Authentic conflict's Settings button; user disables Xbox mode in Settings; separately, only with additional opt-in after publisher/reinstall/app-data checks, removes Game Bar through Windows; recheck and inspect each original chord | User decision 2026-10-17 (supersedes 2026-10-16 conservative warnings): warnings only on provably present conflict. GamingHomeApp positively present/configured only; unknown/absent/unreadable produces no amber/tray/Settings warning. Part B implemented offline 2026-10-17 (locked gates 2,500/0/1): every surface obeys the rule with ordinary no-known-conflict text. Historical note: before Part B the shipped behavior still warned conservatively on unknown. Settings-only actions, no tiler mutation or acknowledgement | Actual Settings page availability, Win+F11/Win+G nonactivation or activation prompts after user remedy, capture/reinstall recovery TBD; navigation never resolves a conflict |
| Game Bar absence versus query failure | Bind Authentic Win+G; inject registered, successful empty and failed current-user package-query results; inspect tray/badge, binding-list OS text and selected OS/detail; rebind to Ctrl+Win+G, then repeat | User decision 2026-10-17 (supersedes 2026-10-16): warn only on a positive installed query; failure/unknown counts the same as absence, with no notes, conflict rows or badges. Part B implemented offline 2026-10-17: registered warns; absence, unknown, and failure all remove every Game Bar warning/note/action and use ordinary no-known-conflict OS text, including disabled rows. Historical note: before Part B query failure retained its warning. Rebound Ctrl chord stays silent; Xbox unknown likewise warns nowhere | No new approved absence exception beyond the 2026-10-17 rule. Physical Windows dispatch remains TBD |
| Unknown configuration versus confirmed recipe-off | Bind Authentic F11, compare present empty REG_SZ fixture against absent/unreadable/unrecognised fixtures, then rebind away or turn takeover off | Part B offline policy implemented 2026-10-17 (User 2026-10-17): only the recognised enabled AUMID warns; unknown/absent/unreadable warns nowhere and recognised recipe-off records an honest off status (never a warning); no active conflicting chord clears a proven warning. Historical note: before Part B unknown retained warning/Settings action. Probe/spawn failure replaces stale proven state with unknown (warns nowhere) with honest logs. Xbox mapping is undocumented and logged categorically | Native mapping stability across Windows versions and actual Settings state TBD; no runtime Xbox-mode detector or suppression guarantee inferred |

### macOS classification discriminators

Variants of R-SPC-01/04 and R-MNZ-01; typed-fact evidence plus explicitly
identified native observations below.
[Implementation and pinned reference comparison](../changes/macos-window-classification.md).
These rows select no new native behavior.

| Variant | Minimal action sequence | Offline candidate / source evidence | Unsupported outcome / decision |
| --- | --- | --- | --- |
| AX size-settable only | Open a standard window with `AXSize` not settable and no usable min/max bounds; open an ordinary resizable sibling | Both reach shared hint-based admission; settability is carried without inventing fixed bounds. glide floats non-resizable; yabai additionally requires a small current frame. Native 2026-10-15: Calculator standard window had no usable hints and size-settable=false; its position moved to a tile slot but its size remained clamped, with stable quiet readbacks | macOS capability-only fixed admission versus ordinary tile TBD. Existing shared both/either-axis hint rule remains selected; no synthetic equal bounds or app-specific exception introduced |
| Restored directly fullscreen | Tile A/B/C; minimize middle B; restore B directly fullscreen; leave fullscreen; record B's side/index/share and A/C allocations | User-approved option A, 2026-10-15: report fullscreen truthfully, matching existing adapter/core behavior; remove the macOS wire-omission workaround. Existing core forgets B's D23 slot and fresh-admits on exit; asymmetric three-window offline regression distinguishes this from saved-middle restore and proves no fullscreen writes | Keep current core behavior; overall goal is Tier 1 input-WM consistency. Exact reference/native outcomes remain TBD; no slot-retention change selected |
| Accessory app with a document window | Open a standard document window in a menu-bar/accessory app, then open its utility panel | Candidate excludes accessory/prohibited owners; rift filters non-Regular owners while AeroSpace has narrower per-window heuristics | Whether an actionable accessory document/panel should participate, and unreadable activation-policy handling, TBD; no bundle-specific exception selected |
| Standard elevated-level window | Tile ordinary A; open standard B at a nonzero CG level, then open an AXFloatingWindow panel at that level | Candidate excludes B but keeps the recognized panel as untouched float; rift rejects non-normal levels except an app-specific dialog carve-out, yabai can float non-normal levels | Standard elevated-level exclusion versus float TBD; recognized panel float is the tentative mapping of the selected transient rule. Exact native level/role combination TBD |
| Nonstandard application document | Open a large application document reporting AXDialog (e.g. undecorated qutebrowser or IBKR), then open its ordinary dialog | Candidate floats both; AeroSpace and rift have different application-specific manage exceptions | Application-specific tiling exception versus generic dialog float TBD; do not infer manageability from current size |
| Sheet identity | Open parent A; open an attached sheet; close the sheet | Candidate floats a separately supplied sheet identity without writes; Silica resolves sheets to the parent. Native 2026-10-15: attached sheet had a separate CG surface but no separately joined AXWindows target; it stayed unchanged while main windows tiled and closed through the helper's endSheet path | AXSheets child traversal, distinct managed identity versus parent alias, and exact modal-focus/settled-geometry behavior remain TBD; native-child non-actuation does not prove a separate float member |
| Transient fullscreen | Open a dialog/sheet; enter native fullscreen if supported; exit | Candidate preserves independent transient-float and fullscreen facts, with no geometry writes | Native feasibility, exact origin/hold outcome and focus TBD; no app-specific fullscreen behavior selected |

### World-state focus-fallback discriminators

| Variant | Minimal action sequence | Observed evidence | Unsupported outcome |
| --- | --- | --- | --- |
| Focused first admission disappears | Admit focused C, then unfocused A/B in separate reports; close C while its app remains alive and reports no focused window; re-observe A/B separately | Shared focus-stack append repair preserves MRU fallback and keeps the scope Controlled; deterministic core/macOS tests and owned three-app native reflow pass. Retained MRU order and observed-focus authority are unchanged ([record](../changes/archive/macos-full-state-replies.md)) | Exact reference/native automatic focus-arrival ordering remains TBD; no automatic admission-focus policy selected |
| Surviving tiles without any focus proof | Start a fresh domain with A/B and no observed focus; repeat after losing the last known focus without a resolving history entry | Existing strict focus binding is unchanged by the MRU repair; no permissive projection-without-focus rule is introduced | Reference outcome and whether to relax a genuine focus-None/empty mismatch remain TBD; test separately from focused-removal fallback |

### macOS native input discriminators

| Variant | Minimal action sequence | Observed evidence | Unsupported outcome |
| --- | --- | --- | --- |
| Carbon physical versus synthetic (retired 2026-10-16; do not run) | Historical sequence: focus an owned helper; register Ctrl+h on the dedicated input owner; press physical Ctrl+h; compare owner delivery and helper suppression; stop and repeat delivery | Registration of all 17 chords returned success. To-PID, Session and HID synthetic producers produced no Carbon hit. Tap pipeline did observe/suppress Session posts. Later diagnostic physical delivery failed 0/3 presses ([results](../changes/archive/macos-dogfood-hardening.md#user-physical-results-2026-10-16)); Carbon hotkey code is now removed | Historical physical Carbon delivery/suppression TBD for this owner sequence; no further acceptance required for the removed route. Synthetic no-delivery alone is not evidence of physical failure |
| OS timeout versus explicit re-arm | With an owned target, apply bounded physical HID pressure during a probe-only callback stall; require an actual OS timeout-disable notification, verified recovery and the next exact chord suppressed; then stop | Session- and HID-posted synthetic stalls did not establish a timeout notification. A disabled enabled-state readback without notification now has separate health recovery. Explicit old-tap disable/fresh-guard recovery is separate evidence | Actual OS timeout-notification/recovery TBD; neither an unnotified readback nor explicit fresh-guard recovery substitutes for it |
| User-input notification versus consent loss | Focus an owned target; explicitly disable the probe tap once; record the actual native notification, enabled readback and next exact/unmatched pairs | Native 2026-10-16: real UserInput notification, verified same-tap re-enable; exact pair suppressed in 49 ms, unmatched extra/plain pairs delivered. Enum name alone is not evidence of consent revocation; privacy/lock/eligibility gates still apply | Revoked-consent, actual Secure Input onset/return and lock-transition native outcomes TBD; this explicit fault does not establish those transitions |
| Compatible exact arrow arms | With an owned target, compare physical Ctrl+arrow, Ctrl+Shift+arrow and Ctrl+Alt+arrow under both presets; record holder action and tiler action | Preview Compatible disables only established unshifted Ctrl+arrow conflicts; synthetic pairs reach the helper without Engine input, while shifted moves still work. Alt resize is a distinct full chord | Physical holder behavior for shifted/Alt arms TBD; absence of an established conflict is not a conflict-free claim |
| Consume before eligibility refusal | Scope Calculator; focus a disposable TextEdit marker with cursor at end; press owned Ctrl+H once; stop the bounded tap and repeat. Separately repeat with TextEdit in scope but native fullscreen | Current user rule (2026-10-16): swallow first, then refuse unsafe actions, including fullscreen/game foregrounds, matching Windows. Offline callback-match and owner-drain regressions prove consume/refuse separation for basic and select-only categories; synthetic live suppression/release and real user-disable recovery pass. [Evidence](../changes/archive/macos-consume-first-input.md) | Physical marker non-deletion while armed, native backward-delete after stop, isolated fullscreen non-effects and actual game behavior TBD. No game exemption/pass-through selected; lock/Secure Input still release |
| Same-app directional focus arrival | Tile two ordinary resizable windows A/B from one app; focus A; press Ctrl+Right toward B; type a distinct marker; reverse with Ctrl+Left; repeat at the exhausted edge | First dogfood logs prove input receipt and native dispatch but not arrival. Native tile focus now follows SLPS dispatch with exact-window AXRaise, matching the pinned yabai focus-with-raise route; sequencing/failure regressions pass | Same-PID native arrival and physical arrow behavior TBD; marker must land in the newly focused document, layout unchanged, exhausted edge retains focus. [Procedure](../changes/archive/macos-dogfood-hardening.md#user-run-physical-script) |

### macOS native workspace discriminator

| Variant | Minimal action sequence | Observed evidence | Unsupported outcome |
| --- | --- | --- | --- |
| Native transfer and per-Space layout return | Tile A/B on native Space O and C on T; send B to T with follow; return to O; repeat send-and-stay; revisit T | Owned-helper direct SLS dispatch left membership unchanged; public AppKit self-join proves fixture recovery only. Native selection and create/remove have separate current/membership readbacks; retained independent layouts and zero hidden writes have offline regressions. Existing-Space select-only preview bindings are now armed ([fallback](../changes/macos-managed-workspaces.md#select-only-fallback-handover-2026-10-17)) | Ordinary-window transfer/inverse, physical retained-layout return and follow/stay focus TBD; send bindings remain unarmed |
| Ctrl-as-Meta relative workspace collision | With default macOS Meta=Ctrl, tile A/B on O and C on T; press Ctrl+H, then Ctrl+Cmd+H; repeat Ctrl+Shift+H versus Ctrl+Cmd+Shift+H | User 2026-10-16 selects Ctrl+Cmd for the auxiliary workspace arm; non-Ctrl Meta uses shared Meta+Ctrl. Offline catalogue tests distinguish all focus/move and workspace arms for every supported Meta and preset | Modifier choice resolved; physical relative select/send, previous Ctrl+Cmd+Tab, output Ctrl+Cmd+Alt actions and customized/common-app conflicts remain pending native integration and acceptance |
| Physical drag transfer routes | With disposable TextEdit A on O and an adjacent empty T, independently drag A while holding its title bar and press Ctrl+Right; repeat with enabled Switch to Desktop 2 / Ctrl+2; repeat by dragging A to T's Mission Control thumbnail; read A's membership before/after each and return it to O | User physically observed all three moves with native follow and no unusual jank; final single-Space cleanup readback passed ([results](../research/macos-port/native-spaces-routes.md#user-manual-transfer-check-2026-10-16)). Per-route after-membership readbacks were not captured. Later clear-helper synthetic outward attempts posted but reported NoEffect | Synthetic carry/inverse and exact membership, numbered/helper/TextEdit synthetic transfer, send-stay switch-back jank, multi-display and minimum-release outcomes TBD; physical success does not prove synthetic delivery |
| Desktop-N ownership across processes | Read status; Apply for two holders; read status in a fresh process; Revert; read fresh status and compare preferences | Both disabled holders enabled with unchanged chords; fresh process corroborated both. Revert restored exact effective preimages and removed recovery JSON; preferences unchanged. Later tagged Desktop-1/2 helper roundtrip proves native holder delivery with exact Apply/Revert restoration ([delivery](../changes/macos-managed-workspaces.md#tagged-native-holder-delivery-checkpoint-2026-10-18)) | Automatic Authentic startup and logout/reboot persistence TBD; evidence establishes same-login-session operation only |
| Product send subject changes before drag | Focus disposable A on O; request send-follow to T; replace focus or subject identity before point proof; retry unchanged, then with a stale Desktop-N holder | Offline pure proof and injectable controller refuse pure focus swap, unknown evidence, lifetime/revision drift and unusable holders; admission pins subject generation/source and terminal feedback never replays uncertain effects. Event-driven effect-owner tests cover busy/cancel/release. All product sends remain unarmed ([checkpoint](../changes/macos-managed-workspaces.md#identity-and-asynchronous-controller-prerequisite-checkpoint-2026-10-18)) | Actual native proof/backend and AppKit handoff, exact point/chord-bound permit consumption, exceptional teardown ownership, runtime transfer, automatic Authentic enable and exact native restoration TBD; fixture-owned proof is not product authority |
| Input-reaching titlebar after recovery | On owned helper A, read unfiltered CG rectangles plus AX and WindowServer point hits; open/close MC; repeat; then public self-join O→T→O and repeat at the same titlebar | Both hits resolve A despite display-spanning layer-20 Dock backing before/after MC. After recovery, focus names A but AX point identity names the protected terminal; bounded settle and pre-dispatch AXRaise did not change it; zero synthetic posts ([checkpoint](../changes/macos-managed-workspaces.md#current-discriminator-and-continuation-2026-10-17)) | Why focus and positional identity diverge, changed clear-helper setup, and synthetic transfer/inverse remain TBD; no focus/identity bypass selected |
| Clear-helper synthetic carry | Spawn A outside the protected rectangle; verify CG/AX frames; self-join O→T→O; verify AX and WindowServer titlebar hits; hold synthetic drag while posting Ctrl+Right; read membership/current; independently restore/remove T | Both hits joined A. Two outward legs reported original membership/current after one down/up pair each for mouse and key; second corroborated rightward order/keycode. No inverse ran. Cleanup excerpts report exact restoration; raw latency arrays were not retained ([handover](../changes/macos-managed-workspaces.md#select-only-fallback-handover-2026-10-17)) | Chord delivery versus held-drag interaction TBD; no timing or physical jank claim; transfer parked, guards unchanged |
| Real/synthetic drag × real/synthetic chord | On TextEdit A at Desktop 1 with empty Desktop 2 and tiler stopped, reset between four trials: real drag + real Ctrl+Right; real drag + `drag-diag-chord`; `drag-diag-hold` + real Ctrl+Right; `drag-diag-full`. Separately compare `drag-diag-full --amethyst --desktop 2` with an already-enabled Desktop-2 shortcut | User-assisted results: real/real passed; tagged synthetic Control-only arrow failed even alone; untagged synthetic drag plus Ctrl+2 moved TextEdit and view together. Later owned-helper tagged Control+Fn arrow and Control-only Desktop-N roundtrips pass with exact membership/current readbacks and raw protected-window before/after records ([delivery](../changes/macos-managed-workspaces.md#tagged-native-holder-delivery-checkpoint-2026-10-18)) | Tap coexistence, product send-follow/stay, physical switch-back feel and multi-display remain TBD; product sends remain disabled |
| Tagged native-holder roundtrip from clear Space | Select existing T with clear helper placement; spawn A; prove independent self-join T->O->T; hold synthetic titlebar drag and post tagged Control+Fn Left/Right; read membership/current after each release; repeat with explicit enabled Desktop-N holders and `spaces-drag-transfer --numbered`; Revert and restore original current | Both routes pass T=277->O=1->T with balanced posts, exact helper frame, raw protected CG bounds unchanged and final onscreen state restored; helper absent, settings/current/topology restored. NumericPad not needed. No ownership or hit-test gate weakened ([checkpoint](../changes/macos-managed-workspaces.md#tagged-native-holder-delivery-checkpoint-2026-10-18)) | Physical visual-jank acceptance, live tap coexistence, ordinary-window product execution, send-stay focus, trailing lifecycle and two-display attribution TBD |
| Empty-Space product select eligibility | With A on O and empty T, run Authentic bounded preview; press Ctrl+2, Ctrl+Cmd+J, Ctrl+Cmd+Tab, Ctrl+1; verify arrival/history and basic-chord consumption/refusal on T | Offline category-gate, separate-cache wiring, consume-first drain, no-focus claim, single-dispatch/history and retained hidden-layout tests pass. Untagged synthetic Ctrl+2 and Ctrl+1 prove tap→engine→drain arrival/return with current indicator readback and inspected legible screenshots. Product refuses selection with zero or multiple native displays | Runtime relative/previous history, physical navigation and MC jank TBD; only 18 existing-Space select bindings armed; index 0 and sends remain disabled |
| Successful empty AX snapshot on hidden Space | Tile three identified windows on O; select T; receive successful empty AX snapshots for the hidden app; select O and re-enumerate in reverse order | Native roundtrip exposed slot reassignment; same-lifetime known-hidden retention correction preserves exact per-ID AX/CG frames live. Causal regressions forbid hidden writes, current-loss over-preservation and retired-domain resurrection | A close while hidden converges on the next covering observation; no immediate hidden-close oracle or broader automatic focus policy inferred |
| Repeated trailing selection on native empty | From one native user Space, select/create trailing T; return to O; toggle back to T; request trailing again | Historical MC AX batch read back all selections but appended a second SID; occupancy cause was not captured at the decision boundary | Reuse/chrome uncertainty at that checkpoint is superseded by the two exact-shape rows below; no broader exemption inferred |
| Partial native accessibility-chrome attachment | With user Spaces O/P/T, inspect apparently empty runtime-created T; select trailing twice; return to O; stop and inspect removal | Exact system AccessibilityUIServer, offscreen layer-3 0x0, attaches to O/T but not P; user-approved 2026-10-20 exact-owner exception corrects false unknown. Product create/select confirms in 900-944 ms, gated reuse in 122 ms without append; inherited 434 removal and runtime-created 445/455 shutdown removal pass. Ordered native rows, not sorted report IDs, gate reuse | Other classes never exempt by analogy; separate exact ControlCenter discriminator below. Physical/floor/multi-display acceptance and trailing-send subject focus remain TBD ([outcome](../changes/macos-managed-workspaces.md#trailing-lifecycle-investigation-2026-10-20)) |
| ControlCenter status surface versus opened panel | Inspect inactive run-created T containing system ControlCenter status surface; attempt empty removal; contrast an owner AX popup or a differently shaped CG panel on T | Captured exact-system offscreen layer-25 120x30 surface with fresh readable empty owner AX inventory is user-approved 2026-10-20 for the narrow exemption; verified 467 removal restores `[1,277]`/current 1. Real-shaped RED/GREEN, normal-layer/unknown-owner/AX-failure/panel regressions pass | Other widths/onscreen shapes, layer-2005 352x149 panel and any nonempty/unknown AX inventory remain blocking. Opened/pinned module live contrast and two fresh automatic create/remove cycles TBD; no bundle-wide exception |
| Sole visible subject follow to empty Space | Send the source's sole visible managed window to reused empty T with follow; repeat with newly created T; compare with a companion left on source | Pre-fix sole TextEdit subject reaches exact membership/current in 65 ms from native dispatch but Finder remains frontmost; `PossiblyExecuted`, no replay. Missing reference destination-front-PSN step is repaired and offline sequencing/failure regressions pass | Post-fix reused/new-target exact subject focus, inverse/stay regression and timings TBD; companion-window causal contrast and physical/input-ready acceptance not inferred ([latest outcome](../changes/macos-managed-workspaces.md#controlcenter-cleanup-and-follow-focus-repair-2026-10-20)) |
| Menu-extra membership on apparently empty Space | Create temporary T, select T through the runtime, return to O, inspect all T memberships, request exact empty-T removal | Removal refused on foreign ControlCenter layer-25 window 3635; T retained, no foreign window touched | Whether this menu-extra is safely classifiable as chrome, and exact original-topology restoration, TBD; preserve unknown-occupancy refusal without a new exemption |
| Menu-bar all-workspace readout | Keep owned A on O; create empty T; show all numbers; select T; verify O remains occupied; freshly hit-test and click O's number; stop and restore the original one-Space layout | Offline projection/config, retained membership, generation/revision-fenced click admission and native-drain refusal tests pass; live proof pending ([record](../changes/macos-workspace-indicator.md)) | Physical appearance, menu-bar crowding, optional icon recognizability, multiple displays, fullscreen and games TBD; native empty Spaces are shown but no trailing lifecycle is implied |
| Same-current numbered send | Tile A on native Space O (current); press the numbered send-follow chord whose Desktop-N resolves to O itself | Core/macOS regressions return `RefusalKind::Unchanged`, retaining no transfer or replay; tiled/floating and unknown-subject fences covered. KDE uses its distinct adapter-side `same-workspace` token before native writes. Synthetic TextEdit product send-1 on current 1 reports `unchanged`, zero native effects and no current/focus drift ([checkpoint](../changes/macos-managed-workspaces.md#same-current-and-send-stay-checkpoint-2026-10-19)) | Physical same-current press and reference-WM same-workspace outcomes TBD |
| Numbered send-stay and source focus | Tile A/B on native O and C on T; send B to T with stay; select T and send B back with stay | Stay 1-9 implemented, default-unbound. Offline committed-core MRU/proven-empty pins, single-flight/release gates, return-failure hold/no replay and no previous-history advance pass. Synthetic bounded TextEdit product stays pass 1->277 with final current 1/focus A, then 277->1 with final current 277/focus C; exact membership/current and `Applied` settlements retained, focus joins transcript-only ([checkpoint](../changes/macos-managed-workspaces.md#same-current-and-send-stay-checkpoint-2026-10-19)) | Physical switch-back feel, live previous-history probe, reference-WM outcomes and multi-display stay TBD; unknown focus/occupancy never guessed |
| Bridged follow versus fast stay geometry | Tile disposable A/B half-width on O with empty T; send B with follow, return B, then send B with stay and revisit T | Synthetic bridged follow confirms exact membership/current/subject focus in 88-98 ms after submission; exact subject geometry in 270/218 ms. Latest stay confirms forward arrival in 92 ms plus 25 ms return selector, final current O/focus A. B retains half-width while T is inactive and is full-width on revisit; an earlier raced-return setter refused despite an exact readback, never counted as applied. Current-only frame fences remain ([checkpoint](../changes/macos-managed-workspaces.md#send-latency-work-2026-10-20)) | Immediate inactive-target tiling, visit-to-first-tile latency, physical/visual/input readiness, minimum-release and multi-display outcomes TBD; no hidden setter or added stay delay selected |

### macOS active-border discriminators

| Variant | Minimal action sequence | Observed evidence | Unsupported outcome |
| --- | --- | --- | --- |
| Active-target appearance and input | Focus owned A; sample a painted border pixel above owned receipt B; guarded click; native fullscreen A and exit | Foreign-helper global ordering and unchanged target focus; border-region synthetic mouse-down receipt delta +1; native fullscreen entry hide, held-identity exit readback false and carrier/focus return. Production snapshot-to-application maxima 22–32 ms versus a 253 ms pre-fix sample; not display latency | Physical appearance/perceived tracking, receipt up-edge readback and wider stacking variants TBD; [appearance procedure](../changes/archive/macos-dogfood-hardening.md#6-active-border-appearance-check-pending-2026-10-17) |
| Native Space switch without transfer | Focus disposable A on O; create/select empty T without sending A; select O and focus A; remove T | Scoped input-off production runs hide on empty T and re-show for A on O; native command selection, not tap input. Fresh final readback confirms one original Space/current and Mission Control absent | Physical tap-to-select, multi-display attribution and wider stacking variants TBD; window transfer remains separate |
| Floating zoom suppression | Successfully float A; verify float state; zoom to work area; restore; retile | Pure plan-membership/zoom tests pass. Synthetic toggle inputs were posted and merged, but neither an actual float nor a maximize transition was established | Live float/zoom suppression TBD; rejected float must not be treated as actual floating or override retained plan truth |

### macOS group-underlay discriminators

| Variant | Minimal action sequence | Observed evidence | Unsupported outcome / next discriminator |
| --- | --- | --- | --- |
| Below-member foreign ordering | Open owned members A/B and nonmember C; show the fill relative-Below the lowest member; read one global CG order; repeat from a front-ordered initial state | Both public routes fail: carrier/member indices 12/[45,2], then 0/[40,38]. The second proves click receipt, focus isolation, fullscreen hide/return and teardown, not acceptable ordering. [Diagnostic and limitations](../changes/macos-group-underlay.md) | Successful below-all-members placement with C above, interleaved and below TBD; scoped private ordering evaluation versus deferral remains open. No product effect enabled |
| Modifier hold versus classified native move | In a focused tiled group, hold Cmd+Shift in either order with an extra modifier; release; separately titlebar-move, resize and programmatically move a member | Shared KDE/Windows policy requires both chord modifiers or a matching focused interactive move; resize alone must not trigger | macOS chord selection, observation/delivery and native-move classification TBD; AX move notification alone is not classification |

### KDE rapid directional input discriminator

| Variant | Minimal action sequence | Reference source / selected KDE outcome | Unsupported outcome |
| --- | --- | --- | --- |
| Opposite focus and mixed commands before settlement | Focus B with left neighbor A; press focus-left then focus-right before the first reply. Separately press focus-left then move-right before its reply | i3 `903bcd51` `src/key_press.c:24-38`, sway `1652c54b` `sway/input/keyboard.c:480-530`, Hyprland `19fb395d` `src/keybinds/Manager.cpp:190-280`, COSMIC `3d55cba0` `src/input/mod.rs` `filter_keyboard_input` / `handle_action` dispatch discrete matched presses sequentially, without adapter-busy refusal. **Tentative (user review pending):** KDE queues unsent tiled directions in order and binds each to current native focus at dispatch. Real-Engine entry regressions prove A-then-B focus and mixed focus/move; native in-flight focus pair accepts both and returns to origin ([record](../changes/archive/kde-rapid-input-geometry.md)) | Physical key delivery, perceived latency and native separately forced debounce remain TBD. Native float/sticky and non-directional queue semantics are not selected; sent-head replay remains forbidden |

### KDE directional transfer settlement discriminator

| Variant | Minimal action sequence | Observed KDE outcome | Unsupported outcome / next discriminator |
| --- | --- | --- | --- |
| Own mover absent after native transfer setters | Place sole owned A on an otherwise empty shown workspace; move right into an empty adjacent output, then left back; record transfer stages, scopes, membership and focus | Reproduced post-setter `geometry-scope` refusal with `fresh_len=0 flight_len=1`, then native arrival by reconcile. Corrected exact-minus-mover tolerance is post-setter only; gated sole-source repeats both ways and occupied left repeat reach native-written/followed/arrived ([record](../changes/kde-dual-output-live.md)) | Unrelated/pre-transfer drift remains refused offline; occupied right live repeat, arbitrary partial-write recovery and exact reference outcomes TBD. No new transfer policy selected |

### KDE occupied R4 target-shape discriminator

Variant of [R-OUT-01](reference-outcomes/multi-output.md#r-out-01-move-left-onto-an-occupied-output).

| Variant | Minimal action sequence | Ours offline outcome | Reference / native |
| --- | --- | --- | --- |
| Near-side remembered leaf on parallel multi-window target | Set `L=H[W1,W2]` remembering W2 (near-side), `R=H[W3*,W4]`; move W3 left. Mirror with the target remembering its near-left leaf | Selected 2026-10-14: `L=H[H[W1,W2],W3]`, mirrored right `H[W4,H[W1,W2]]`; every occupied R4 landing wraps the whole target tree per REQ-MOV-08, no remembered-leaf insertion, no flat append; empty takes the mover alone. Far-side history: before change `L=H[H[W1,W3],W2]`; far-side correction kept the same whole-wrap pair. Core implementation covers all four directions with focused tests passing | COSMIC S20/S23 observations wrap the whole target, including a parallel root. Authoritative KDE world trace 2026-10-19 proves nested return `H[1,1](H[1,1](L,L),L)` with native arrival/mover focus; trace redacts leaf identities and live peer order was W2,W1, so exact named fixture remains TBD. i3 flat tail insertion is rejected for the selected layout-preserving policy; sway/Hyprland exact wrapper TBD. No overriding Tier 1 majority established |

### Core-owned topology split-edge discriminator

| Variant | Minimal action sequence | Ours offline outcome | Reference / native |
| --- | --- | --- | --- |
| Migration versus window-position versus unique focus | Use FULL rects S=(0,0,1000,1000), B=(1000,0,1000,700), C=(1000,700,1000,300). Put sole tiled A on S with centre y=850. From fresh identical state, migrate workspace right; separately exhaust move-right, send-to-output-right and focus-right | World route applies the existing 2026-10-09 decision: migration selects B (longer edge), exhausted move/send select C (centre projection), focus keeps strict unique-reciprocal horizontal policy and does not cross. Expected-result Rust regressions and real-Engine KDE world-entry move/send/migration/focus fixtures pass. [Outcome](../changes/archive/world-split-edge-selection.md). Windows legacy focus remains distinct | Exact reference/native three-output results TBD; ordinary two-output native acceptance remains separate |

### Windows world-adoption authority discriminators

| Variant | Minimal action sequence | Verified offline distinction / proposal | Reference / native |
| --- | --- | --- | --- |
| Migration into a not-yet-observed scope | Two edge-touching FULL outputs, W containing A only on source, different target view; report complete inventories and empty adjacency; issue migrate-right; re-observe unchanged native state | Core emits Migration plus source-refill View, but target W scope and member authority hold as incomplete inventory. Windows facade writes nothing and retains the obligation; KDE's migration path uses result provenance without this scope gate. Shared authority precedence/publication needs explicit resolution before positive Windows migration acceptance; no adapter override | Positive native member/view/refill settlement TBD; [W2b proposal](../changes/windows-full-world-state-replies.md#w2b-cross-stream-proposals) |
| Capacity-float crossing without a Windows remap | Tile 800px A on source, 9px B on a 25x25 edge-touching target; send A right-stay from a complete world report with empty adjacency | Legacy Windows refuses `float-no-remap`, preserving source Engine/membership (T2). Raw world route accepts a capacity float and offline facade moves it; this remains a parity-gap diagnostic, not accepted behavior. A separate test-only staged entry now refuses before adoption; diagnostic float origin never drives adapter policy | Runtime world refusal and physical native outcomes TBD; offline evidence does not establish runtime acceptance |
| No-core staged send versus R4 refusal | From the preceding two-output state, separately evaluate send-right and exhausted move-right on a fresh clone of the booted world; discard unsupported candidate; release exact flight and report unchanged observations | Test-only sequencer entry refuses the staged floating cross-output Window transfer as local `float-no-remap`, preserving retained Engine and real managed membership. Operation cross-output Move agrees. Real close/feedback obligations survive until accepted catch-up; sequence gaps do not replay. [Expanded offline proof](../changes/windows-full-world-state-replies.md#t2-offline-staged-transaction-2026-10-20) | Native world refusal TBD; live owner remains legacy |
| Existing float versus tiled-to-float crossing | Start A already floating on source and send right into a roomy target; repeat with automatic-fixed A; separately send tiled A to a roomy target and move A locally | Ordinary and automatic-fixed float facade arrivals retain frame and stay views. Fitting tiled crossings/local moves adopt and continue. Whole-workspace migration keeps its distinct D7/D8 contract and adds no staging clone; no diagnostic-origin branch | Physical remap/shortcut acceptance TBD; no runtime wiring |
| Source repair concurrent with crossing | Boot A/B tiled on source; shrink source work to 24x24 so D7 selects A as victim; request send-right or exhausted move-right into a roomy target in that report; catch up observation-only, then repeat with native tiled flag still stale | Fixture outcomes: sends reject `focus-mismatch`, R4 rejects `partial-observation`, including Operation forms; source Engine stays unchanged by rejection. Catch-up accepts floating A at source with no transfer. Later stale tiled-flag commands give the same verdicts. No extra converged-baseline clone needed for these tested shapes | Physical and other untested repair shapes TBD; not a universal classification theorem |
| Direction-only versus explicit-target send | From identical source/target reports, send A right by SendDirectional; separately name target output/native workspace via Send or Operation SendToOutput | Direction-only uses `start_output_send`, stages supported transfer or T2 capacity fallback. Explicit variants use `start_send` and currently reject `cross-domain-mismatch` in these fixtures; adapter staging preserves that core refusal/source state, not API equivalence | Shared-contract resolution and physical explicit-target outcomes TBD; no core correction selected |

### KDE dual-output admission and workspace-scope discriminators

Variants of [R-OUT-05](reference-outcomes/multi-output.md#r-out-05-open-a-window-with-two-occupied-outputs) and [R-WS-11](reference-outcomes/workspaces.md#r-ws-11-next-workspace-previous-workspace).

| Variant | Minimal action sequence | Observed KDE outcome | Unsupported outcome / next discriminator |
| --- | --- | --- | --- |
| Inverse opposite-pointer admission | Occupy both outputs; focus B on the second while pointer stays on the first; launch ordinary C; record native output, membership and active window | Pre-fix C joined the pointer output. Corrected exact-birth routing reaches the focused output with C active; final-source managed-anchor inverse repeat has routed/native membership evidence ([record](../changes/kde-dual-output-live.md#focused-output-admission-correction-and-breadth-repeat-2026-10-14)) | D06 remains selected. Intra-round birth placement and remaining native activation-order/reference timing outcomes TBD |
| Managed focus shift and birth within one debounce | Settle A and B on different outputs; focus A, then activate known managed B and open C before the pending debounce; pointer stays on A's output | Offline pre-fix C followed stale A; synchronous managed-activation capture now routes C to B. Both transfer and same-output burst regressions pass | Native tightly coalesced focus/birth ordering and reference timing outcomes TBD; sampled settled-anchor admission does not establish this leg |
| Empty ring-end departure versus spare maintenance | On each output show/select an existing empty ring end with a sticky anchor, then relative-next; separately show a genuinely empty end and depart by ordinal 1; finally occupy each output's trailing end | **Tentative (user review pending), 2026-10-19:** showing/selecting creates nothing; relative-next wraps to cell 1 with the opposite view unchanged. Root cause was counting the sticky anchor as occupancy, not selection itself. Core RED/GREEN and real world_eval + production apply/fresh replies cover both wraps and fills, including native empty-list sticky homing. Native callbacks prove both forward wraps and one new spare per ordinary fill; genuinely empty show creates none. A departed globally empty, unshown, non-trailing cell may retire after sticky rehome | Tier 1 source: COSMIC `3d55cba0:src/shell/mod.rs:652-672` appends only for occupied/pinned last; `src/shell/workspace.rs:1538-1543` excludes its separate sticky layer from emptiness. Hyprland `19fb395d:src/config/shared/actions/ConfigActions.cpp:150-198` creates only a missing target, not another spare when selecting an existing empty. i3/sway have no trailing-spare equivalent; no contradiction found. Exact reference runtime fixture and keyboard delivery remain TBD. [Native evidence/restoration limits](../changes/kde-dual-output-live.md#empty-ring-end-correction-2026-10-19) |
| Shared selection versus migration refusal | In shared mode select ordinal 2, step next and Previous; separately request whole-workspace output migration | Production core supports the shared selection ring; migration has distinct refusal rules. No shared-mode live refusal leg was dispatched | Which T03 selection action is expected to refuse, and with which reason? Native/reference outcomes TBD until the action is specified |
| Shown secondary current outside its managed ring | Enable with distinct populated native currents; select ordinal 2 and return, then step next/previous on each output | User-selected 2026-10-18: each distinct shown current belongs to its output, one ring per native. Core claims complete startup currents before generic binding echoes; secondary no longer inherits a foreign-homed current. Native KWin 6.7.5 distinct-current enable and eight independent numbered/relative transitions pass ([outcome](../changes/archive/kde-startup-desktop-ownership.md)) | Full inverse relative-send ring-end/follow/stay matrix remains separate; switching proof does not claim send coverage |
| Full-state startup rings with shared or distinct visible currents | Enable `per-output-local` with the same populated native current on both outputs; repeat with distinct populated currents; inspect complete wire/native bindings and scoped switching | User-selected 2026-10-18: shared current stays primary-owned; each other showing output receives fresh backing and same-output membership moves of its non-sticky windows. Distinct secondary current belongs to secondary. Native shared/distinct legs pass with direct mode evidence, one ring/native, friendly spare names, independent numbered/relative switching and output-send follow in both directions. No duplicate visible-current ownership ([outcome](../changes/archive/kde-startup-desktop-ownership.md)) | Native physical shortcut delivery, whole-workspace migration and full inventory acceptance remain separate |
| Shared-current startup with exceptional window states | Before enable put ordinary, intentional-floating, fullscreen, maximized, minimized, sticky and minimized-sticky clients on the non-primary output's shared current; record native state and enable once | Native KWin 6.7.5: every non-sticky client changes only backing membership/output-local view; output and float/overlay/minimized state preserved; both sticky clients unchanged. Minimized wire entries now require native `sticky` evidence; source-slot restore policy is unchanged | Multi-desktop non-sticky memberships and exact post-unminimize restore shape not tested here |
| Startup primary differs from focused output | Put the focused ordinary client on a non-primary physical output; enable with a shared current; compare ownership with reversed focus | Current carrier reports KDE `activeScreen` as primary, with output-order fallback; first complete startup latches conversion. Existing carrier semantics retained | Physical-primary versus focused-output selection remains TBD; the completed legs explicitly established the reported primary |
| Multi-desktop non-sticky startup member | Put a non-sticky client on native desktops A and B, with A shared at enable; convert its output and read the complete membership list | Existing core observation has one home (first native member), and KDE membership setter replaces that home with one desktop | Retaining additional native memberships versus single-homing, and native/reference outcomes, remain TBD; no multi-desktop acceptance claimed |
| Whole-workspace migration with native output-send echo | Two owned ordinary members on source workspace A; target output shows occupied B; migrate A toward target once | INT_MAX accepted. Immediate native evidence shows `(source output, target current B)` before output propagation; exact same-flight lag echo is now tolerated with unchanged-view/safety/freshness proof and both members move. Unshipped core confirmation trials exposed lost target follow, then an ambiguous source refill and retained no-op feedback storm | Approved whole-workspace preservation remains the oracle; complete migration/refill and inverse pending. Replay arrival/view/feedback sequence before another fix; arbitrary partial-write/reference recovery remains TBD |
| Output follow return after retained source hold | Two owned peers on each uniquely bound shown workspace; send A right with follow; after observation, send A left with follow | Original return refused retained `scope-held` after unreadable source evidence. Fresh same-fixture right-follow/left-return and both stay directions pass with complete wire/native evidence on the prototype-integrated tree; hold did not recur. Final KDE-only repeat stopped at protected-active fixture gate before dispatch | Original hold trigger/reference recovery remains TBD; no guard or retry-policy change. Final-tree native repeat pending |
| Migration arrival before native view settlement | Migrate two members A to an occupied output; re-observe after member homing but before target follow/source refill; then observe native view results and member closure | Homing-only core confirmation drops the parent before target follow. View-gated prototype retains it and target follow lands, but source refill remains ambiguous and repeated removal feedback drives no-op dispatches. Prototypes are not shipped | Exact source-refill native outcome and safe terminal settlement remain TBD. Replay complete producer-shaped sequence, including failed view proof and closure, without inventing retries |
| Explicit move during output absence | Disable A's output; on survivor send A to another workspace; re-enable original output; record A and donor contents separately | A keeps its explicitly selected workspace, which returns to the re-enabled output; donor contents remain on the survivor ([record](../changes/kde-dual-output-live.md#further-native-outcomes-2026-10-14)) | Exact changed-donor return affinity and new-content variant TBD; retained A membership alone does not prove either |

Shared-mode hotplug has a separate [minimal action-sequence discriminator](reference-outcomes/workspaces.md#shared-mode-hotplug-discriminator-2026-10-15):
one shared history/ring without project displacement; unsupported reference and
native outcomes remain TBD.

### Windows settings placement review

| Variant | Minimal action sequence | Ours outcome / evidence | Unsupported outcome |
| --- | --- | --- | --- |
| Four-tab placement | Open Settings and visit all four tabs | User-approved 2026-10-16: Tiling & Workspaces contains gaps, workspace defaults/scope, move/admission/refill; Appearance border/underlay; Shortcuts takeover, Win+L, presets, bindings, Xbox/Game Bar; Desktop Integration mouse Snap/taskbar pager. Delivered catalog/tab CI 38876735949 passed | Physical layout, keyboard, accessibility and save/reload acceptance remain pending; placement approval is not live evidence |

### Windows taskbar pager discriminators

| Variant | Minimal action sequence | Ours outcome / evidence | Unsupported outcome |
| --- | --- | --- | --- |
| Rendered-output click versus focus | Focus output A; click workspace N on output B's pager | Tentative 2026-10-16: select the exact rendered output/workspace identity, not A or a recomputed ordinal; portable fence coverage. Native one-output message-driven selection passed ([record](../changes/archive/windows-taskbar-pager.md)); 2026-10-20 two-output carrier geometry and alternating secondary/primary/secondary message selections passed on empty workspaces ([live fixes](../changes/windows-dual-output-live.md)) | Physical two-output click/focus, occupied-workspace pager switching and reference outcomes TBD |
| Secondary without a notification-area HWND | Enumerate a secondary taskbar with readable descendants, visible left app host, exact full-bar composition bridge and zero-area input site; compare an unreadable fact, unknown visible child and right-edge content | Tentative (user review pending), 2026-10-20: recognized hierarchy serves at bar-right minus the existing gap; unsupported/unknown facts refuse. Exact valid tray/clock children instead select their leftmost exclusion edge regardless of enumeration order; degenerate exact anchors refuse. Pure production-resolver fixtures and native two-carrier smoke pass ([record](../changes/windows-dual-output-live.md)) | HWND-less composited content, changed shell hierarchies, Explorer restart, display changes and physical selection TBD; no universal visual exclusion guarantee |
| Queued click across suppression | Queue a pager click; disable the pager or enter managed fullscreen before dispatch; resume | Drop the queued click, keep the current workspace; shared visibility/dispatch gates and regression tests. Managed-fullscreen hide/return passed natively, without a queued-click race | Physical race timing and real-game outcomes TBD; no replay is selected |
| Strip wider than entire tray-left span | Keep adding workspaces until fixed DPI-scaled cells exceed the entire tray-left taskbar span | User-approved 2026-10-16: pager is not drawn, reason logged, tray number/menu remain. Current geometry already refuses; no code change. Ordinary app-button crowding does not affect placement | Physical over-wide-strip outcome TBD; no alternate overcapacity treatment selected or needed |
| Latest pending pager click | Before dispatch, click workspace 2 then workspace 3 on the pager | User-approved 2026-10-16: retain only workspace 3's click; never replay workspace 2. Offline overwrite/dispatch regression passes | Physical burst timing and reference outcomes TBD |
| Miniature icon sizing | Resize a miniature across the DPI-scaled 8 logical px icon minimum, with obtainable then unavailable artwork | User-approved 2026-10-16: cap at 16 logical px, minimum 8, one physical border pixel plus one padding pixel clear per side; omit missing/too-small icons. Offline sizing coverage | Physical Calculator plain-gray miniature accepted-for-now User 2026-10-17 (user has seen Calculator); packaged-artwork lookup deferred to dogfooding review |
| Miniature icon and number styling | Inspect obtainable opaque/transparent-edge icons over overlapping miniatures and workspace numbers; repeat at separately approved DPI scales | User-requested and physically accepted 2026-10-19: 128/255 icon opacity after source-alpha composition; 10pt Segoe UI Bold white numbers with offset black shadow, substantial visual improvement. Base label centering, cell/strip geometry and tray gap unchanged; blending/font regressions pass | Subsequent centered-halo request is separate; wider alpha-edge/DPI variants TBD |
| Centered number halo | Inspect white workspace numbers over busy miniatures; compare both sides of each glyph, then separately approved DPI scales | User-requested and physically accepted 2026-10-19: replace offset shadow with eight symmetric black text copies around unchanged centered white glyphs. Radius one DPI-scaled logical pixel (1px at 96 DPI, 2px at 144/192 DPI), zero net offset; font/cell/strip/tray geometry unchanged. Pure offset/sizing regressions pass | Wider DPI acceptance TBD |
| Shared Windows accent | Enable border theme mode; compare border and pager with Personalization accent across starts; change accent while idle, observe both, then restore it; separately repeat pager-disabled, hidden/reshown and border-theme-off variants | User-selected 2026-10-19: documented UISettings.GetColorValue(Accent), positive-alpha guard, configured border fallback and pager blue fallback. Diagnostic startup exits are native access violations localized to activation-factory vtable dispatch. Delivered correction: one owner-scope WinRT apartment and version cache; direct RoActivateInstance bypasses the process-static factory cache; both surfaces consume one plain snapshot, renderer never queries. Shared-query/stage diagnostics and lifecycle/cache/fallback regressions pass. User physical acceptance 2026-10-19 PASSED: 10/10 starts succeeded with correct accent on both active border and pager. No polling, new dependency, settings or core change | Idle-refresh and wider native/fault variants TBD. Prior successful-black values lack raw query evidence; their exact cause and cached-pointer origin of the activation fault remain unproven |
| Maximized admission interrupted by accent crash | Start with an ordinary member and an initially maximized eligible member; allow admission/restore and the next healthy reconcile | Incident evidence: initial restore dispatched, ordinary member written, both read back in one slot despite opposite planned slots, then native accent activation crash before tick two. Suspension false; not the limited-fullscreen lane. Existing admission code defers restored-member convergence until the next observation | Corrected-build next-tick separation and absence of persistent overlap TBD; no independent geometry defect or geometry remedy inferred |
| Desktop and Start pager visibility | With pager visible, click desktop background; return to an ordinary app; open/dismiss Start; focus a second ordinary app as control; correlate each transition with foreground surface identity, integrity/read result and pager gates | User-required 2026-10-19: pager stays visible on desktop and while Start is open. Follow-up log identifies successful low-integrity CoreWindow reads incorrectly taking the non-medium elevated gate, without a fullscreen veto. Corrected pager classification suppresses only at/above high (`0x3000`); low/AppContainer and medium do not suppress. Failed reads and fullscreen/game gates remain unchanged; no shell exemption. Normal transition-only `pager-native` after actual shows/existing topmost recovery reports visibility, checked topmost and bounded taskbar-relative order; unknown stays unknown. Offline policy/privacy/real-log regressions and independent review pass ([record](../changes/windows-single-output-dogfood.md)) | Corrected physical desktop/Start visibility and cause of persistent disappearance after allowed recovery TBD; no new timer/polling or claim of presentation from allowed gates |
| Pager integrity versus fullscreen | With no fullscreen cover, focus low/AppContainer, medium and high-integrity surfaces; compare unreadable process/integrity controls; separately repeat a genuine fullscreen cover | Offline production-gate fixtures: low/untrusted/medium/medium-plus allow, at/above high suppress, open/read failure suppress; fullscreen still suppresses even at low integrity. Tiling's medium-only manageability is unchanged | Native protected/elevated/game experiments are not part of the ordinary desktop/Start acceptance; exact reference outcomes TBD |
| Primary taskbar auto-hide | Enable auto-hide; reveal the taskbar temporarily; disable auto-hide | User-confirmed 2026-10-17, as shipped: hide the pager throughout auto-hide, retaining tray number/menu; restore from fresh geometry when disabled. Read-only production ABM_GETSTATE, offline transition coverage and native toggle/exact-state restoration passed ([record](../changes/archive/windows-taskbar-pager-lifecycle.md)) | Slide-in/out tracking deferred; reference outcome TBD |
| Nonactivating topmost cover | With an unchanged visible pager and already-shown disposable helper overlapping it, promote the helper to topmost with `NOMOVE/NOSIZE/NOACTIVATE`; observe raw events and independent z-order | Native 2026-10-18: desktop-parent `REORDER`, object `-4`, accompanied observed burial; current subscription omits it. Pager remained visible and recovered between sampled readbacks. Broader complete coverage unproven; retain existing 2 s recovery ([probe](../changes/archive/windows-pager-event-coverage.md)) | Universal visibility/topmost coverage, self-feedback safety, own-window/Explorer/display/DPI legs TBD; no OS-silence, timer-only cause or weaker contract claimed |

### Windows focused-output workspace discriminator

| Variant | Minimal action sequence | Ours outcome / evidence | Unsupported outcome |
| --- | --- | --- | --- |
| Native focus differs from last selected output | Select a workspace on A; focus an existing ordinary window on B; select ordinal 2 without an explicit output; reverse A/B | Corrected 2026-10-20: genuine foreground monitor beats the stale last-switch cache. Offline both-direction/cache/unknown-output regressions and agent-driven exact-owner CLI selections on both outputs pass. Tentative (user review pending) fallback without readable foreground: pointer, cache, stable output order, matching the existing tray resolver. User physical 2026-10-20: Win+digit on each output passes except the [undecided empty-workspace focus case](#empty-workspace-focus-across-outputs). [Record](../changes/windows-dual-output-live.md) | Exact reference fallback outcomes and a physical stale-cache discrimination fixture remain TBD; the user's passing routing check does not establish every fallback leg |

### Windows dual-output transfer discriminators

Variants of R-OUT-04/07 and R-MOU-03. These are bounded native observations,
not new reference votes or selected transfer policies.
[Evidence and limits](../changes/windows-dual-output-live.md#additional-two-output-checks-2026-10-20).

| Variant | Minimal action sequence | Ours observed outcome | Unsupported outcome / next discriminator |
| --- | --- | --- | --- |
| Floating membership versus physical source | Float A on L; explicitly send A to R with follow; retain its unchanged physical frame on L; immediately request send-left with follow, separately repeat with stay | Windows 2026-10-20: original return refused `unmanaged` by comparing membership home R with foreground physical L. Narrow admitted-float proof now permits the return; native Paint follow/return-follow both commit, unchanged frame and current views; return-stay passes offline | Native return-stay, reference/KDE and three-output directional ranking remain TBD. Existing membership-source routing is unchanged; exact subject/lifetime/hidden fences remain; no relocation or new source-selection policy is inferred |
| Empty singleton versus occupied pair drop | Native-title-bar drag tiled A from L onto an occupied R edge; reset, empty R and release A inside R; compare exact release point and replies | Windows 2026-10-20: occupied pair-route preview/plan/arrival agree; empty route reports `cross_output: true, empty_destination: true`, then `unchanged` and source snap-back. One real-Engine producer-shape singleton replay projects and admits, so those facts alone do not reproduce it | Exact adapter event/retained session and reference/corrected native outcome TBD. New refusal-only event diagnostics enable exact replay; no drop policy or core behavior change selected |
| Cross-DPI native move start | Drag Paint from lower-DPI R to higher-DPI L; correlate native start hit-test, size transition and settle classification; compare reverse direction | Windows 2026-10-20: reverse of the passing occupied leg settled as `PointerResize` after size growth; start `move-kind` was `unknown`, then `malformed-input` refusal | Start hit-test cause, reference behavior and corrected native outcome TBD; no size-change tolerance or forced move classification selected |

### Windows shared startup discriminator

| Variant | Minimal action sequence | Ours offline outcome | Reference / native |
| --- | --- | --- | --- |
| Different stable-first and primary/focused outputs | With two outputs whose stable-key order differs from primary order, restart the owner in shared mode; select Previous before any completed switch; repeat with foreground on the other output | Tentative 2026-10-15: stable-first output supplies the initial logical view, all outputs converge to it, Previous has no prior transition. Synthetic owner coverage passes ([record](../changes/archive/windows-workspace-scope-owner.md)) | Exact KDE/reference startup anchor, primary/focused alternatives and physical two-output Windows outcome TBD; single-output startup cannot distinguish them |

### KDE cross-output drag native-scope discriminator

Variant of [R-MOU-03](reference-outcomes/mouse.md#r-mou-03-drag-across-outputs-and-onto-a-workspace-target).

| Variant | Minimal action sequence | Ours offline outcome | Reference / native |
| --- | --- | --- | --- |
| Native mover leaves foreground before Finish | Begin dragging A from L to R; keep foreground on L while native output becomes R; hold over a valid edge, deliver an outstanding R reconcile reply, then release; repeat reversed | Before repair preview refuses `absent` and hidden reconcile can actuate during hold. Corrected unique hidden-holder lookup routes preview/drop to R with Started source L; neither foreground nor hidden reconcile actuates mid-hold; release reobserves and converges. Zero/multiple hidden holders still refuse; repeated identical refusal/backoff logs once until state changes. Follow-up also accepts an observed destination becoming foreground before mover arrival ([record](../changes/kde-dual-output-live.md#cross-output-drag-lifecycle-correction-2026-10-14)) | Physical mixed-scale occupied/empty preview/drop and pause rendering TBD; exact native/reference timing TBD |
| Consecutive gesture and same-gesture re-cross | Drag A L to R; without releasing return to L's edge; release; begin another drag while foreground still belongs to another output | Same-correlation logical bounds/source coverage passes. Before correction foreground-only Started/verdict lookup misses the mover on the other observed output; corrected fresh cross-domain lookup arms/routes it. Idle abandoned holds and missing preview replies recover, late replies remain fenced | Reported visual cursor/overlay misalignment and exact preview/drop agreement TBD; native crosses with an unavailable oracle prove fallback recovery only, not drop-intent acceptance |
| Preview failure stage | Hold A over an occupied destination edge, move between edge and gap, then return without release; correlate ambiguous preview refusals and later recovery | `cross-domain-mismatch` and `malformed-topology` remain unrooted in the reported run. Closed, bounded stage diagnostics distinguish convergence, capture, resolver and projection without changing outcomes | Which stage causes the reported bands and whether its refusal is appropriate TBD; no new drop or cancellation policy selected |

### D16 floating-overlay carry discriminator

Variant of [R-WS-05](reference-outcomes/workspaces.md#d16-combo-and-arrival-frame-discriminators-2026-10-13), selected 2026-10-13.

| Variant | Minimal action sequence | Ours offline outcome | Reference / native |
| --- | --- | --- | --- |
| Floating overlay versus born-fullscreen hold | Float B on WS1; fullscreen B; explicitly send to WS2; leave fullscreen. Repeat with maximize and numbered/relative follow/stay | B retains float plus overlay on arrival with no geometry/overlay writes or tile reflow. Exit restores the floating frame without admission, unlike D28 born-fullscreen first exit. Shared/KDE fixtures pass ([record](../changes/archive/d16-floating-transfer.md)); Windows adapter/Engine evidence 2026-10-13 ([record](../changes/archive/windows-d16-floating-transfer.md)); sticky/automatic-fixed refusals unchanged | Exact qualified reference and native KDE/Windows carry/exit outcomes TBD |

### D23 best-effort restore and close discriminators

Variants of [R-MNZ-01..03](reference-outcomes/minimize.md#d23-restore-and-close-cleanup-discriminators-2026-10-13), selected 2026-10-13.

| Variant | Minimal action sequence | Ours offline outcome | Reference / native |
| --- | --- | --- | --- |
| Former group versus new focus | Tile A/B/C; minimize B; admit D nested under a surviving former group; restore B | Rejoin the surviving former group at stored index with comparable stored share; do not fresh-admit beside D. Shared and KDE entry fixtures verify rejoin/no focus write ([record](../changes/archive/d23-rework.md)) | COSMIC `3d55cba0`, `src/shell/layout/tiling/mod.rs:458-502,505-545`: surviving parent/index or sibling regroup before fresh admission. Exact per-action reference frames and native KDE/Windows outcomes TBD |
| Minimized close versus omission | Tile B; minimize B; close B; repeat without the close | Explicit `closed_windows` clears stored slot and retires the sole slot-only session; bare omission retains it for restore. Rejected/undelivered close reports remain pending; unreadable membership does not invent an empty snapshot | Shared/protocol/KDE fixtures passed; Windows adapter/hidden-domain fanout evidence 2026-10-13 ([record](../changes/archive/windows-d23-restore-close.md)). Exact native close ordering and other reference outcomes TBD |
| Close hides a live window instead of destroying it | Start `H[W1 V[W2 W3]]`; close W2 through an application that hides its window; compare actual destroy and native minimize controls; reshow W2 | User 2026-10-19 selects close/allocation retirement and existing close/MRU rules after positive alive/invisible/uncloaked/lifetime/no-tiler-claim-or-ledger verification; W3 refills the right column, and W2 reshows as fresh admission with no old slot. Unknown facts and tiler hides hold. Delivered adapter-only; uncertain hide ownership persists through visible recovery and subsequent hide until authoritative ledger resolution. Offline regressions and independent safety review pass ([record](../changes/windows-single-output-dogfood.md#completion-and-remaining-acceptance)) | Tier 1 allocation-removal basis: `S-cos-rem` (COSMIC unmap/group collapse), `S-sway-close` (unmap/detach/rearrange), `S-hyp-close` (unmap removes layout target). Native Windows hide detection, iconic state in the incident, exact reference action outcomes and corrected physical behavior remain TBD; no minimize/rejoin policy change |
| Uncertain tiler hide versus later app hide | Commit a tiler hide, fail post/readback and rescue read; observe visible recovery then another hide while the ledger entry remains; compare an ordinary workspace hide/reveal and authoritative ledger absence | Offline adapter fixture: retained uncertainty defers through visibility and later hide, including a non-uncertain post failure; tiler hide/reveal never becomes an app close. Only authoritative ledger absence clears uncertainty without dropping membership, after which a positively verified app hide closes. Fresh-token reshow has no remembered slot | Native fault injection and exact reference failure outcomes TBD; no live failure was induced |
| Source slot after membership rehoming while minimized | Tile A on WS1 with B/C nested grouped; minimize B; confirmed membership rehome of B to WS2 while minimized (explicit-send route; native minimized-target shortcut reachability TBD); restore B there; focus A to set the fresh-admission anchor; return B to WS1 with A and C surviving throughout. Observe source-slot state plus whether B rejoins C's group (keep) or fresh-admits at A (forget) | Destination fresh-admission fallback is selected. Windows boundary fixture simulates membership rehoming and retains the shared source slot; it does not prove an explicit minimized-send route. User decision 2026-10-17, option B: forget the source slot on confirmed rehome; a later return is admitted fresh. Shared-core implementation (affects KDE) approved, not implemented | COSMIC `3d55cba0`: minimized send removes the source entry and clears the anchor (`workspace.rs:659-669`, `mod.rs:3333-3360,3383-3418`); destination entry is anchorless and destination restore fresh-admits (`tiling/mod.rs:544-545` via `mod.rs:4439-4453`) ([findings](../research/d23-minimized-rehome.md)). Ordinary fresh-return is inference from the shared move path; exact return shape TBD. Hyprland/i3/sway have no native minimize counterpart: scenario N/A, non-voting (`S(S-hyp-mininv)`, `S(S-i3-mininv)`, `S(S-sway-mininv)`). Whole-workspace migration keeps entries with the workspace and sticky-layer entries are a separate route, neither votes on per-window rehoming. Exact native KDE/Windows outcomes and explicit minimized-send reachability TBD |
| Confirmed rehome before any restore | Tile A on WS1 with B/C grouped; minimize B; confirm B's per-window membership moves to WS2, then back to WS1 while still minimized; focus A and restore B, with A/C surviving | Approved D23 option B requires the first confirmed departure to forget B's source slot; restore admits fresh at A rather than rejoining C, even without a destination restore. Implementation pending; [cross-platform proposal](../changes/shared-core-d23-rehome-and-13f-maximize-base.md) | Exact reference return-without-restore outcome, native membership evidence and rendered KDE/Windows/macOS managed-workspace outcomes TBD. Own minimized-send shortcut reachability remains TBD; no new targeting behavior is selected |

### Windows asynchronous preview-finish discriminator

| Variant | Minimal action sequence | Windows offline outcome / tentative decision | Unsupported / physical outcomes |
| --- | --- | --- | --- |
| Missing hide proof | Show a drag preview for A; end the gesture; withhold the renderer's verified hidden acknowledgement past 500 ms; later try a fresh gesture | User 2026-10-14 approved as delivered ([decision](../decisions.md#windows-owner-threading)): refuse that finish without Engine drop, focus, geometry or snap-back writes; retain the hide safety obligation. A verified hide instead permits fresh owner revalidation before actuation; cancel/Esc/suspend/new gesture/stop invalidate pending work ([record](../changes/archive/windows-render-thread.md)) | Deterministic pending-owner fixtures prove ordering/refusal. Agent live verified hide-before-own-slot-drop, not a physical structural drop. Native missing-ack and exact reference-WM outcomes TBD; no crash/game experiment inferred |

### Windows cross-output transfer failure discriminator

| Variant | Minimal action sequence | Windows offline outcome / tentative decision | Unsupported / physical outcomes |
| --- | --- | --- | --- |
| Failed transfer tail | Exhaust a tiled move into another output, or release a native title-bar drag there; refuse final topology, geometry readback or arrival proof (also source reflow for an empty drag destination) | User 2026-10-14 approved as delivered ([decision](../decisions.md#windows-multi-output-foundation)): discard candidate Engine changes, retain original logical membership and skip follow-focus; any partial native geometry converges through subsequent observation without replay. Occupied/empty release and keyboard refusal fixtures pass ([record](../changes/windows-multi-output-foundation.md)) | Physical refusal timing, partial-write recovery and exact reference-WM outcomes TBD |
| Focus fails after verified transfer | Complete the same transfer with verified arrival; refuse its subsequent foreground setter | User 2026-10-14 approved as delivered ([decision](../decisions.md#windows-multi-output-foundation)): verified membership/Engine transfer stands; report focus failure without claiming follow-focus | Physical setter refusal and exact reference-WM outcomes TBD |

### Windows tray menu pager discriminator (2026-10-15)

| Variant | Minimal action sequence | Ours outcome | Unsupported / physical outcomes |
| --- | --- | --- | --- |
| Already-active pager row | With two workspaces on the focused output, open the tray menu and click the already-active workspace number | Observed native 2026-10-15: `already-active` visibility no-op, visibility unchanged ([record](../changes/archive/windows-tray-menu-pager.md)) | Native focus/geometry settlement and exact reference-WM outcomes TBD; no geometry/history invariants measured; no in-taskbar visual pager behavior selected |

### Windows hidden-workspace presentation discriminators (2026-10-17)

Variant hooks: V-WS-SHELL-ACTIVATE (R-WS-07 Ours Windows cell). User decision
2026-10-17: keep `SW_HIDE` for 0.1. Post-0.1 research investigates trivial
configurability (e.g. off-screen parking); switch-versus-pull remains open in
post-0.1 research. Full comparison and recommendation (not a selection):
[hidden-workspace presentation research](../research/windows-port/alt-tab-hidden-workspaces.md).

| Variant | Minimal action sequence | Established evidence | Unsupported outcome |
| --- | --- | --- | --- |
| Hidden shell selection under SW_HIDE | Tile A on WS1 and B on WS2; product-hide WS2; tour Alt+Tab, Win+Tab/Task View, taskbar buttons and hover thumbnails; attempt to select B | Owned-helper live 2026-10-17: `SW_HIDE` reads `IsWindowVisible=false`, `DWMWA_CLOAKED=0`, geometry retained; `SW_SHOWNA` restores identical geometry. Omission from every shell surface follows the documented visible-window rule (inference, not a visual observation) | Actual Alt+Tab/Win+Tab/taskbar/thumbnail/Peek visuals; explicit-selection activation (nothing to select today); switch-versus-pull if a future backend lists B; reference outcomes TBD |
| Minimized presentation instead of hide | Same setup with B minimized instead of hidden; same shell tour; restore B | Owned-helper live 2026-10-17: minimized snapshot stays `visible=true` at the iconic -32000 rect with nonactivating minimize/restore; listing inferred retained (not visually observed) | Shell visuals; D23 collision (minimize releases allocation and stores a restore slot, so minimization is recommended against as the hiding backend; that recommendation is PARKED with the presentation decision, not a final selection); reference outcomes TBD |
| Parked presentation instead of hide | Same setup with B parked at an off-screen corner instead of hidden; same shell tour plus owner crash; restore B | No local evidence; Glaze `PlaceInCorner` source shape only. Crash residue, sliver clickability and geometry-restore behavior all TBD | Listing/selection visuals, parked-window detection, durable geometry ledger, monitor/DPI/maximize handling, crash-strand guarantee and reference outcomes TBD |
| Hidden-workspace urgency marking (D27 5a) | With B product-hidden on WS2, deliver an unsolicited activation request for B; observe the workspace indicator, taskbar flash surface and focus/workspace | `FlashWindowEx` is documented to flash without changing activation ([MS docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-flashwindowex)). 2026-10-14: shell flash observed for hidden targets but the visible surface was not proven; planned workspace-urgent indicator unimplemented | Visible mark/clear behavior per backend (SW_HIDE has no visible caption/button to flash), screen-reader announcement and reference outcomes TBD |

### R-FLT-06 readings

Minimal variants of [R-FLT-06](reference-outcomes/floating.md#r-flt-06-float-toggle-over-a-maximized-window), recorded with the [B9 delivery](../changes/archive/maximized-intentional-unfloat.md). These qualify the ordinary-float row without changing fullscreen or sticky policy.

Windows delivered the same fixed-override, unconfirmed-clear and sticky-boundary
readings offline 2026-10-12 ([record](../changes/archive/windows-maximized-intentional-unfloat.md));
scripted native effects plus real Engine admission prove ordering and later-press
recovery, not real native settlement or marker readback.

| Windows discriminator | Minimal action sequence | Windows offline outcome | Reference / native |
| --- | --- | --- | --- |
| B9 fullscreen transition | Float and maximize B; toggle float off; owned fullscreen metadata appears after clear with a non-covering fresh frame | Tentative 2026-10-12, pending user review: re-read metadata after clear and refuse fresh admission; retain float intent. Fresh captionless fullscreen also vetoes admission | Exact reference outcomes and native transition timing TBD; no game experiment inferred |
| R-MAX-03 setting independence | Under each born-maximized setting, first admit maximized A on a floating workspace beside B; enable tiling; restore A | Reserved slot without clear under both settings; B keeps its share and restored A projects to the slot. Born R-MAX-06 setting affects only its separate admission lane | Ordinary Windows retile/restore physically user-accepted 2026-10-12 under conditions below; exact setting-qualified retile repetitions/reference outcomes TBD |

| Variant | Action sequence | Ours KDE offline outcome / reading | References / native |
| --- | --- | --- | --- |
| Fixed explicit override | Admit fixed B as an automatic float; maximize B; toggle ordinary float off once; observe again | Clear maximize, observe clear, fresh-admit; successful explicit tile commits D3/D7 override, so B does not re-auto-float | Fixed admission floats per profile: COSMIC min-max `S(S-cos-min)`; Hyprland `suggestsFloat` min-max `S(S-hyp-float)`; bspwm fixed admission `S(S-bsp-admit)`; i3 fixed floats `S(S-i3-min)`; xmonad fixed/transient `S(S-xmo-float)`; sway `wants_floating` fixed `S(S-sway-max)`; qtile fixed-size rules `S(S-qti-float)`; awesome fixed implicit `S(S-awe-fixed-dynamic)`; niri `compute_open_floating` `S(S-nir-spc)`; karousel shapeability gate `S(S-kar-spc)`; PaperWM `add_filter` admits Normal non-transient only with rejects floating `S(S-pap-spc)` (fixed-size mapping TBD). Maximize legs per R-MAX-01 (bspwm/i3/xmonad/sway no maximize counterpart; PaperWM width-only `S(S-pap-widthmax)`; paneru host-only `S(S-pan-axfs)`). Toggle legs per R-FLT-06: COSMIC unmaximize-first `S(S-cos-flttoggle)`; Hyprland clear/re-apply retaining `S(S-hyp-float)`; qtile unfloat with maximize dropped `S(S-qti-float)` + `S(S-qti-fs)`; awesome explicit-only flip `S(S-awe-float)` + `S(S-awe-fs)`; i3/sway standard disable `S(S-i3-flt-toggle)` / `S(S-sway-float)`; niri plain move `S(S-nir-flttoggle)`; karousel state flip `S(S-kar-acts)` + `S(S-kar-float)`. Explicit-override commit has no reference counterpart (Ours-only); settled frames/focus TBD |
| Clear not confirmed | Intentionally float B; maximize B; toggle float off while clear is refused or remains unobserved; press again after normal state is observed | No admission on unconfirmed clear, float intent retained, logged narrow refusal with no stuck flight; later explicit press fresh-admits | No reference profile has an observation gate: each toggle path runs synchronously in source with no pending or refused-clear branch (COSMIC unmaximize-first `S(S-cos-flttoggle)`; Hyprland clear/re-apply `S(S-hyp-float)`; qtile unfloat `S(S-qti-float)`; i3 `floating_disable` `S(S-i3-flt-toggle)`; sway `container_set_floating` `S(S-sway-float)`; awesome `set_floating` `S(S-awe-float)`; niri plain move `S(S-nir-flttoggle)`; karousel state flip `S(S-kar-acts)`). Refused/unobserved-clear hold with later-press recovery is Ours-only; native clear ack and focus visuals TBD in every row (true runtime, live-only) |
| Sticky boundary | Make B sticky; maximize B; toggle ordinary float once | Existing sticky-maximize refusal unchanged; ordinary R-FLT-06 does not select a sticky policy change | Sticky legs per R-FLT-02: COSMIC output-set layer `S(S-cos-sticky)`; Hyprland float-only pin guard `S(S-hyp-pin)` (tiled pin refused); bspwm no float guard `S(S-bsp-sticky)`; i3 sets on any con but pushes floats only `S(S-i3-sticky)`; sway sets unconditionally but effective only when floating `S(S-sway-sticky)`; awesome orthogonal `S(S-awe-sticky)`; xmonad/qtile/niri/karousel/paneru no sticky verb (`S(S-xmo-float)`, `S(S-qti-float)`, `S(S-nir-acts)`, `S(S-kar-acts)`, `S(S-pan-cmds)`); PaperWM scratch stuck plus above plus float `S(S-pap-float)`. Maximize and toggle legs per R-MAX-01/R-FLT-06 above; the sticky plus maximize plus toggle combination itself is untraced in every profile, settled TBD |

### R-MAX-04 / R-FLT-02 KDE held-key discriminator

Minimal action sequence for the pending B1/B2 physical-repeat leg. The
[toggle activation decision](../decisions.md#window-state-float-sticky-maximize-fullscreen)
selects at most one native attempt per explicit activation; it does not
establish KGlobalAccel's held-key delivery or select a repeat-suppression policy.

| Variant | Minimal action sequence | Offline evidence | Physical outcome / remaining decision |
| --- | --- | --- | --- |
| Held-key delivery | Focus ordinary eligible A; hold Meta+M past the repeat delay; release; press once again. From a fresh ordinary A, repeat with Meta+Shift+G | Historical removed-legacy fixtures pinned one native attempt per delivered maximize callback, repeated native restore/restick cycles and duplicate sticky notification convergence, plus per-activation sticky attempts; world-route re-verification TBD | Ours KDE: callback count while held, visible cycling, release/repress delivery and any required suppression policy TBD, user-owned. Ours Windows: held repeats consumed without dispatch per the recorded decision; physical journey TBD. Reference WMs: this exact held-key sequence TBD; no source/native claim added |

### KDE world-route group-highlight discriminator

| Variant | Minimal action sequence | Current evidence | Unsupported outcome / decision |
| --- | --- | --- | --- |
| World group payload production | Tile A/B in one group; focus A; hold Meta+Shift; release; start and finish an interactive move of A | The world entry can forward active-group/no-group results, but no production caller submits the core active-group operation. Ordinary focus/observation live samples emitted no group setter. This gap predates legacy-route deletion | Chord/move rendering TBD. Decide whether to query on focus/domain/tree lifecycle changes or add a native visibility-triggered query; neither trigger migration is selected here |

### R-LAY-01 Windows held-key discriminator

| Variant | Minimal action sequence | Windows offline outcome / tentative decision | Unsupported / physical outcomes |
| --- | --- | --- | --- |
| Held orientation toggle | Focus B in `H[A,B*]`; hold Win+O past repeat delay; release; press once again | User 2026-10-14 approved as delivered ([decision](../decisions.md#move-layout-and-output-commands)): one toggle on discrete down, consumed repeats do not retoggle; release/repress returns H. Classifier/queue tests 2026-10-11, base `755aab8` + delivery commit ([record](../changes/archive/windows-parent-orientation-toggle.md)) | Windows physical delivery/OS suppression TBD, user-owned. KDE held callback behavior and exact reference-WM held sequence TBD; no unsupported outcome inferred |

### R-CTL-05 M13 readings

Minimal variants of [R-CTL-05/06](reference-outcomes/restart-persistence.md#r-ctl-05-shortcut-staging-and-apply), User 2026-10-08 M13 = B. Native store stays authoritative; these are offline KDE outcomes, not live acceptance or selected Windows runtime changes.

| Variant | Minimal action sequence | Ours KDE offline outcome / reading | References / native |
| --- | --- | --- | --- |
| Keep custom / canonical | Assign a custom chord in KDE Shortcuts (repeat with canonical); keep row checked; confirm project Apply | Assignment unchanged; post-write verification compares the preserved image | No counterpart: pinned shortcut inventories define bindings only with no Keep/staging verbs, so a kept custom chord has no native leg; outcome TBD (owner-specific, project-only Keep). COSMIC `S(S-cos-shortcut)`; Hyprland `S(S-hyp-shortcut)`; bspwm `S(S-bsp-ctl)`; i3 `S(S-i3-bind)`; xmonad `S(S-xmo-ctl)`; sway `S(S-sway-bind)`; qtile `S(S-qti-keys)`; awesome `S(S-awe-keys)`; niri `S(S-nir-acts)`; PaperWM `S(S-pap-acts)`; karousel `S(S-kar-acts)`; paneru `S(S-pan-cmds)` |
| Keep empty | Clear a project's assignment (or observe unresolved empty registration); check Keep; confirm Apply | Stays empty; Keep never repairs or enables it | No counterpart: same binding-only inventories list no Keep/empty-repair verbs; outcome TBD (owner-specific, project-only Keep). `S(S-cos-shortcut)` + `S(S-hyp-shortcut)` + `S(S-bsp-ctl)` + `S(S-i3-bind)` + `S(S-xmo-ctl)` + `S(S-sway-bind)` + `S(S-qti-keys)` + `S(S-awe-keys)` + `S(S-nir-acts)` + `S(S-pap-acts)` + `S(S-kar-acts)` + `S(S-pan-cmds)` |
| Authentic intent | Customize or clear a binding; stage Authentic; decline Apply; then confirm fresh Apply | Staging/decline writes nothing; confirmed Apply writes canonical | No counterpart: no staged-Authentic/decline model in any pinned shortcut inventory; outcome TBD (owner-specific, project-only Authentic staging). `S(S-cos-shortcut)` + `S(S-hyp-shortcut)` + `S(S-bsp-ctl)` + `S(S-i3-bind)` + `S(S-xmo-ctl)` + `S(S-sway-bind)` + `S(S-qti-keys)` + `S(S-awe-keys)` + `S(S-nir-acts)` + `S(S-pap-acts)` + `S(S-kar-acts)` + `S(S-pan-cmds)` |
| Consumed Authentic | Stage Authentic; confirm Apply/Force successfully; customize in KDE Shortcuts with project Settings still open; confirm Apply again | Successful commit consumes reset intent; later Apply preserves the new custom chord. Failure/decline retains staged intent | No counterpart: no consumable reset-intent/commit model in any pinned shortcut inventory; outcome TBD (owner-specific, project-only Authentic). `S(S-cos-shortcut)` + `S(S-hyp-shortcut)` + `S(S-bsp-ctl)` + `S(S-i3-bind)` + `S(S-xmo-ctl)` + `S(S-sway-bind)` + `S(S-qti-keys)` + `S(S-awe-keys)` + `S(S-nir-acts)` + `S(S-pap-acts)` + `S(S-kar-acts)` + `S(S-pan-cmds)` |
| Compatible | Customize a nonconflicting row; stage Compatible; confirm Apply | Known/discovered conflicts disabled; nonconflicting custom assignments preserved, no replacements | No counterpart: no Compatible staging model in any pinned shortcut inventory (conflict lookup/unbind/mask verbs only, no staging); outcome TBD (owner-specific, project-only Compatible). `S(S-cos-shortcut)` + `S(S-hyp-shortcut)` + `S(S-bsp-ctl)` + `S(S-i3-bind)` + `S(S-xmo-ctl)` + `S(S-sway-bind)` + `S(S-qti-keys)` + `S(S-awe-keys)` + `S(S-nir-acts)` + `S(S-pap-acts)` + `S(S-kar-acts)` + `S(S-pan-cmds)` |
| Keep custom conflict | Give a project and foreign action the same custom chord; Keep; Apply; preview Force; confirm | Actual custom chord shown; Apply refuses; confirmed/revalidated Force removes only that chord from the foreign holder and preserves project assignment | No counterpart: no Keep/preview/Force foreign-clearing model in any pinned shortcut inventory (mask/unbind verbs only, no draft preview); outcome TBD (owner-specific, project-only Keep/Force). `S(S-cos-shortcut)` + `S(S-hyp-shortcut)` + `S(S-bsp-ctl)` + `S(S-i3-bind)` + `S(S-xmo-ctl)` + `S(S-sway-bind)` + `S(S-qti-keys)` + `S(S-awe-keys)` + `S(S-nir-acts)` + `S(S-pap-acts)` + `S(S-kar-acts)` + `S(S-pan-cmds)` |
| Stale Force / write drift | Preview custom conflict; change project/foreign assignment or staged intent; confirm Force (separately: change Keep during a selected write) | Stale confirmation refuses before writes; post-write drift fails verification without canonical reset | No counterpart: no stale-preview/write-drift revalidation model in any pinned shortcut inventory; outcome TBD (owner-specific, project-only Force; live timing-dependent). `S(S-cos-shortcut)` + `S(S-hyp-shortcut)` + `S(S-bsp-ctl)` + `S(S-i3-bind)` + `S(S-xmo-ctl)` + `S(S-sway-bind)` + `S(S-qti-keys)` + `S(S-awe-keys)` + `S(S-nir-acts)` + `S(S-pap-acts)` + `S(S-kar-acts)` + `S(S-pan-cmds)` |

### R-CTL-05 KDE modal-flow discriminators (2026-10-12)

The preceding M13 readings describe the earlier confirmed-Apply presentation;
Keep/Authentic semantics remain unchanged. The [current dialog flow](../decisions.md#shortcuts-conflicts-and-presets)
supersedes that presentation. No-stacked-confirmations is kept as shipped User 2026-10-17 (accepted-for-now; later UX review nonblocking). [Offline evidence](../changes/archive/kde-shortcut-result-dialogs.md).

| Variant | Minimal action sequence | Ours KDE offline outcome | Unsupported / physical outcomes |
| --- | --- | --- | --- |
| Direct Apply | Stage a conflict-free choice; click Apply Shortcuts; use its chord before overall Settings Apply/OK | No preliminary question; applies immediately, result shown in a short dialog: "Shortcuts applied."; ordinary Save remains isolated | Dialogs work well per user live review 2026-10-12; exact physical delivery and new KCM loading TBD. Windows adapter dialogs verified below; exact reference-WM sequence TBD |
| Complete modal / Cancel / Force | Stage conflicts across several chords with more than 26 holders and one default-only blocker retaining an unrelated chord; Apply; Cancel; remove blocker; Apply; Force | Complete scrollable blocker-first list, stable holder ordering, full indexed diagnostics; blocked Force disabled, retained keys shown; Cancel writes nothing and opens no second dialog; fresh Force is the sole confirmation and revalidates | Exact native many-holder list, physical delivery and timing-dependent stale refusal TBD, user-owned. Windows/reference outcomes TBD |
| Revert result | After Force, Revert; accept custom-binding-loss warning; repeat with empty cleared list | One loss confirmation followed by success/error result; foreign defaults restored, custom preimages not restored; empty list succeeds | Native default restoration and physical delivery TBD, user-owned; Windows settings Revert is a separate implementation |
| Full catalog / same-window repeat | Empty 93 project assignments; give 43 foreign holder records their required chords; Authentic; Apply; Cancel; Apply; Force; repeat conflict-free full Apply in the same Settings window | Cancel writes nothing; Force clears exact listed keys then assigns the catalog; per-operation budgets permit repeated use beyond 64 total writes. User 2026-10-14 removes previous-identity labels/Revert skip; old-name records are ordinary foreign holders | Initial KDE live fixture used old-name records: 136-write Force about 250 ms, later 93-write Apply about 187 ms; complete snapshots/restoration verified ([record](../changes/archive/kde-shortcut-apply-failure.md)). Updated ordinary-holder Revert is covered offline; physical delivery, other platforms and reference outcomes TBD |
| Unchanged Lock Session exemption | With Lock Session already on its post keys, stage another holder conflict; Apply; compare with a fresh preview that relocates Lock Session | User 2026-10-14: omit the entire Lock Session exempt line for equal actual/post keys; show actual move and preserved alternates only for different keys. Native dialog regression passes | Updated live dialog and exact reference outcomes TBD |

### R-CTL-05 Windows modal-flow discriminators (2026-10-17)

Windows Apply saves the full draft; Revert reloads saved settings, not KDE
shortcut defaults. Presentation is kept as shipped User 2026-10-17 (accepted-for-now; later UX review nonblocking).
[Offline and scripted live evidence](../changes/archive/windows-shortcut-settings-dialogs.md).

| Variant | Minimal action sequence | Windows outcome | Unsupported / physical outcomes |
| --- | --- | --- | --- |
| Preset / direct Apply | Open Settings from the scoped tiler's tray; select Shortcuts; stage Compatible; Apply | Staging writes nothing; conflict-free Apply shows only "Settings saved."; owner observes the keyboard revision | Scripted desktop verified; physical chord delivery and exact reference-WM journey TBD |
| Advisory preview / Cancel / Apply | Stage Authentic with an unresolved gaming warning; Apply; Cancel; Apply; accept preview | Complete scrollable warning set, Apply/Cancel with no Force; Cancel preserves draft/file and opens no result; accepted Apply saves and shows the short result | Scripted desktop verified; physical chord delivery and other platforms TBD |
| Revert / discard safety | Edit a core field; Revert; Cancel; Revert; Discard | Cancel retains the edit; discard reloads saved values and shows "Reverted to saved settings."; neither path writes or undoes an earlier Apply | Scripted desktop verified; exact reference-WM journey TBD |
| Error / preview drift | Apply an invalid field; separately change the saved file while the preview is open, then accept | Short error modal; validation writes nothing; preview-time external change is reloaded/refused, never overwritten | Scripted desktop verified; atomic filesystem races outside the existing stale-check contract are not claimed solved |

### 2026-10-10 reference comparison discriminators

Minimal variants of existing R-MAX-08 and R-WS-23. Selected intent is recorded
below; shared-core/KDE delivery is offline. Windows G-06 is delivered offline
2026-10-12 ([record](../changes/archive/windows-maximized-directional-navigation.md));
G-37 Windows schema/UI/runtime delivered offline 2026-10-14
([Windows record](../changes/archive/windows-workspace-output-migration.md));
native outcomes remain pending
([shared record](../changes/archive/reference-comparison-implementation.md)).

| Variant | Minimal action sequence | Selected Ours KDE / Windows target | Reference / native evidence |
| --- | --- | --- | --- |
| Isolated maximized mover (G-06, R-MAX-08) | Prepare `H[A,B*,C]`, maximize B; move B right once without a preceding focus command | One unmaximize attempt, ordinary move only after observed clear in that invocation; otherwise narrow refusal/no structural move. KDE production-entry real-Engine fixture and Windows guarded routing/pre-post-clear policy delivered offline | Existing R-MAX-08 traces COSMIC unmax-move and Hyprland refusal on still-focused maximized B; Windows ordinary clear-then-tiled-move physically user-accepted 2026-10-12 under conditions below; exact tree fixture/other references and KDE native TBD |
| Source MRU differs from order (G-37, R-WS-23) | L has occupied WS1, occupied WS2, trailing E; visit WS1 then WS2, migrate active WS2 to R; repeat fresh under each source-refill value | Default `last-remaining-workspace` shows E; `most-recently-used-workspace` shows eligible WS1 from L history. KDE production-entry real-Engine fixtures delivered offline; Windows scoped selector/config/runtime delivered offline 2026-10-14 | Existing R-WS-23 gives reference policies, but exact history-qualified outcomes for this added variant are TBD; Ours native journeys TBD |
| Source MRU fallback (G-37, R-WS-23) | Same source order; no eligible previous entry on L; migrate active WS2 to R with MRU selected; repeat with the remembered entry removed or migrated out of L's scope | Fall back to last remaining; do not recreate or follow an out-of-scope ID. Eligibility: pre-mutation previous stable ID must remain live in the source scoped ring excluding the migrated ID; surviving empty eligible, invalidated IDs excluded. Offline shared/KDE and Windows (2026-10-14) fixtures delivered | All exact reference outcomes and Ours native journeys TBD; global-history models are not substituted for the selected per-output model |
| Unconfirmed maximize clear (G-06, R-MAX-08) | Maximize B; move right while native clear refuses or remains unobserved; later press after restore | One attempt per invocation; log refusal, keep structural state, no delayed move. Later press can move. KDE refusal fixtures and Windows pre/post-clear policy tests delivered offline | Exact reference and native failure-timing outcomes TBD |
| Surviving-empty MRU (G-37, R-WS-23) | Visit empty WS1 then occupied WS2 on L; keep WS1 live, migrate WS2 right with MRU | WS1 qualifies while still in the remaining source ring, even empty. If lifecycle removes it, fall back to last remaining; never recreate. Shared/KDE and Windows (2026-10-14) selector fixtures delivered offline | Exact reference outcomes and native lifecycle timing TBD |

### Windows floating candidate discriminator (2026-10-12)

| Variant | Minimal action sequence | Windows offline outcome / decision | Reference / native |
| --- | --- | --- | --- |
| Observed/retained equal-axis tie (R-FLT-09) | Float F and G; maximize another float M; arrange G's top-left x equal to M's visible top-left x; focus F to their right and press Left, then repeat from their left with Right; reverse G/M stacking and repeat | User 2026-10-14 approved, current order kept ([decision](../decisions.md#window-state-float-sticky-maximize-fullscreen)): observed G precedes retained M within its ordinary layer, Left keeps G and Right keeps M. One shared axis-distance race across classes/layers, so nearer M still beats farther G; encounter order affects ties only ([record](../changes/archive/windows-floating-directional-navigation.md)) | Exact combined-order/stacking outcomes and Windows physical tie order TBD. Source tally: COSMIC 3d55cba0 and Hyprland ae50c4d6 use unified live-list races, no comparable observed/retained split; 7 other available profiles lack the exact maximize/float-origin leg. No reference majority for observed-first; native maximum geometry need not equal restore geometry |
| Owned fullscreen transition (G-06, R-MAX-08) | Maximize B; enter project fullscreen; during a transitional non-covering frame press move Right while owned fullscreen metadata is present | Tentative 2026-10-12, pending user review: metadata vetoes maximize clear even if the fresh geometry is not a full cover; no clear or move. Pre-clear fullscreen precedence is pinned offline; actual transition routing is native TBD ([record](../changes/archive/windows-maximized-directional-navigation.md)) | Exact reference outcomes and Windows native transition timing TBD; no game experiment inferred |

### Windows fullscreen-exit discriminators (2026-10-12)

User rule: a window leaving fullscreen, including borderless, uses ordinary
admission/tiling; D5 fixed-size and intentional-float identities remain applicable.
No target writes while fullscreen. [Offline record](../changes/archive/windows-fullscreen-exit-acceptance.md).

| Variant | Minimal action sequence | Windows offline outcome | Reference / native |
| --- | --- | --- | --- |
| First fullscreen exit | Start owner with ordinary A already fullscreen; repeat fresh by opening fullscreen A after owner start; exit A through its app. Repeat with captionless full-monitor cover becoming a smaller borderless frame | Same first-exit current-hint admission route; non-fixed/unknown tiles, fixed automatically floats; commit consumes the exit mark | Owned Notepad startup variant physically user-accepted 2026-10-12 under conditions below: stop/relaunch while fullscreen then owned exit tiles. App-native/born/borderless/fixed/game/DPI/multi-output and exact reference outcomes TBD |
| Retained fullscreen return | Tile A beside B; fullscreen A; observe again; exit A | Retains A's allocation; retained-only fullscreen excluded from writable subset; exit returns to same slot | Win+F11 owned tiled Notepad/exit slot return physically user-accepted 2026-10-12 under conditions below; focus-away/back/app-native and other exact reference/native variants TBD |
| Owned veto recovery | Enter owned fullscreen on maximized A; observe non-covering transition with metadata; attempt move/unfloat; exit through owned toggle; attempt again | Pre/post-clear veto until metadata absent; owned exit verifies restore before removal, failed exit preserves retry; later clear/admission succeeds | Native metadata removal/transition timing and exact reference outcomes TBD |
| Stale owned preimage | Enter owned fullscreen; owner restarts with properties still present; frame becomes normal without owned exit; maximize then move/unfloat | User 2026-10-12: automatic retirement on fresh proven exit. Nonzero owned chrome mask restored plus fullscreen false, exact lifetime/preimage revalidation and second live proof retire metadata only. Reconcile sweep requires no explicit command; move/unfloat then proceed. Mere non-coverage never retires | Retirement/native transition timing and exact references TBD; captionless zero-bit policy is decided in the next row |
| Captionless owned preimage | Enter owned fullscreen on originally captionless A; return to a non-covering frame without owned exit; attempt maximized move/unfloat; exit with Win+F11 owned toggle; retry | User follow-up 2026-10-12: without a restoration witness, zero-bit metadata stays fail-closed and vetoes maximize clear; no automatic retirement. Owned toggle exit verifies restoration before metadata removal and remains the release path | Exact reference outcomes and captionless native restore/retry timing TBD; user watches for windows failing to tile during dogfood |
| Re-fullscreen after retirement | Leave owned fullscreen by other means with restored chrome; reconcile retires stale marker; enter genuine owned fullscreen again; move/unfloat | Fresh owned preimage/fullscreen keeps the veto; no removal during re-fullscreen. Changed preimage or re-fullscreen between retirement probes also preserves metadata | Exact reference and native race outcomes TBD |

- Owned fullscreen physical conditions, 2026-10-12: scoped single-output
  Notepad/Paint, physical keyboard/mouse; base `e506350` plus directional,
  retile/B9 and fullscreen-exit regression changes before automatic retirement.
  Owned managed-slot and startup-exit tiling passed. Every Win+F11 also raised
  the known Xbox full screen experience prompt; containment remains parked.
  These results do not establish the later automatic-retirement behavior.

### Windows user-accepted physical evidence (2026-10-12)

- Conditions: scoped normal owner, single output, Notepad/Paint, physical
  keyboard/mouse; base `e506350` plus combined directional, retile-overlay and
  B9 Windows deliveries. These results qualify the linked Windows cells; exact
  unreported fixtures/variants stay TBD.
- Float-origin Win+arrow focus and Win+Shift+arrow half-snaps accepted; maximized
  tiled Win+arrow keeps focus; maximized float clears then all four half-snaps;
  maximized tiled clears then moves (user finds it sensible).
- Floating workspace -> maximize -> retile retains maximize; unmaximize lands
  in reserved slot. Both born-maximized values accepted with maximized Paint
  launch. Radius 6 correct, live Settings Apply accepted (styling evidence only).
- Observation only: tray workspace tiling toggles do not restore previous window
  focus; may suit tray keyboard accessibility, no behavior change requested.
- Game/DPI/multi-output/fixed-size and exact tie/sticky/failure/race legs pending.
  Existing ordinary B9 native evidence remains distinct from physical item-11
  acceptance.

### Triage discriminators 2026-10-11 (D01, D26, D27 selected 2026-10-12)

User 2026-10-12 selected D01, D26 (b) and D27; D26 lists cross-login
intentional-float memory loss as a 0.1 known limitation.
Ours and reference/native outcome cells below are TBD; no native
evidence is fabricated. These rows discriminate focus/visibility and store
lifetime, linking existing fixtures:
[R-INS-06](reference-outcomes/insertion.md#r-ins-06-open-over-an-overlay-maximized-plus-fresh-fullscreen-variant),
[R-CLOSE-05](reference-outcomes/close.md#r-close-05-close-a-maximized-or-fullscreen-window),
[R-ACT-01](reference-outcomes/activation.md#r-act-01-hidden-window-sends-an-unsolicited-activation-request),
[R-RST-02](reference-outcomes/restart-persistence.md#r-rst-02-end-session-restore-session-and-apps).

| Variant | Minimal action sequence | Selected / tentative target | Ours / ref outcomes |
| --- | --- | --- | --- |
| Maximize admission plus overlay close (D01) | Maximize focused B over survivor A; open C; close B | Selected: structural admission behind retained maximize state/slot, host-owned activation/focus/stacking; closing B removes its slot and refills/focuses survivors ([decision](../decisions.md#placement-minimums-and-startup-adoption)) | Ours offline delivered: native automatic activation retains slots/state with automatic-focus fences; removal/refill/focus with exact full-domain survivor geometry plus focus ([record](../changes/archive/kde-fullscreen-activation-and-send.md)); native outcomes and cleanup TBD, user-owned; reference sources in R-INS-06/R-CLOSE-05 |
| Fullscreen admission plus overlay close (D01) | Fullscreen focused B over survivor A; open C; close B | Selected: native activation ownership, no B fullscreen geometry writes or tiler focus actuation while fullscreen is involved; closing B removes its slot and refills/focuses survivors ([decision](../decisions.md#placement-minimums-and-startup-adoption)) | Ours offline delivered: automaticFocusGuard reobserves after geometry and fences automatic focus on overlay/midapply/domain/nativefocus races; explicit shortcuts and sends stay effective ([record](../changes/archive/kde-fullscreen-activation-and-send.md)); native outcomes TBD, user-owned; reference sources in R-INS-06/R-CLOSE-05 |
| Ordinary activation vs active/idle game (D27) | From hidden B, send valid-token then missing/stale-token activation; repeat while a real game has active input, then while idle/paused; compare tiler-off/on, plus pager/taskbar selection | User 2026-10-13: KDE native host exception, no tiler guard/intercept/revert/suppress. KWin token serial/X11 user-time gating expected mark-only in active play; idle/pager behavior exactly without tiler. Real-game active/idle check must pass; reopen only on actual active-play switch-away. Windows user-approved 2026-10-16: Switch and focus honors OS-permitted requests even over fullscreen/games, implementation pending; default Mark urgent only and denied requests unchanged. Setting KDE applicability reviewed at design ([decision](../decisions.md#activation-requests)) | KDE source trace complete: valid-token/FromTool/allowed-X11 switches + focuses, missing/stale-token demand-attention; convergence exists, verification-only ([trace](../research/d27-kwin-activation-trace.md)). Exact native/game and unsupported reference outcomes TBD |
| Windows caller rights / hidden urgency (D27, 2026-10-14) | A foreground, B current then product-hidden on workspace 2: plain `SetForegroundWindow` from a fresh never-attached background process; separately `FlashWindowEx(FLASHW_TRAY)`; compare a previously attached controller and tiler off/on; observe foreground, visibility, selected workspace and owned shell flash | User-approved 2026-10-16: OS-permitted Switch and focus has no fullscreen/game urgency-only downgrade, not yet implemented. Default Mark urgent only/non-permitted unchanged; explicit selection separate. Visible urgency/clear design unselected | At `f4b9f41`, fresh current/hidden calls denied, no focus/workspace switch; previously attached caller granted hidden activation with workspace convergence. Shell flash heard for hidden targets, not visible-marker proof. Helper self-request/native-permitted clean route, visible mark/clear, fullscreen/game and physical selection TBD ([record](../changes/archive/windows-d27-activation-verification.md)); reference outcomes TBD |
| Windows OS-permitted activation over fullscreen/game (D27 5b) | With a game/fullscreen window foreground on WS1 and B hidden on WS2, request B activation with established OS permission; repeat in Switch and focus and Mark urgent only, then with a denied request | User-approved 2026-10-16: Switch and focus plus OS permission switches to WS2 and focuses B, even though WS1/game becomes hidden; default Mark urgent only and denied requests unchanged. No extra tiler game-protection override. Approved target, implementation/tests pending | Clean permission discriminator and real-game active/idle outcomes TBD; no new live evidence or KDE behavior change |
| New-login intentional-float memory (D26) | End session; start a new login and observe owned intent | Selected (b) listed 0.1 known limitation: intentional-float memory does not survive logout/login under unchanged session-scoped REQ-RST-01d; reopened apps have new identities. Revisit after 0.1 with shared restart store ([decision](../decisions.md#restart-persistence), User 2026-10-12) | Ours TBD; ref/native TBD beyond linked R-RST-02; native dogfood/live check pending, verification-only |

### KDE script restart with a retained Planner

| Variant | Minimal action sequence | Established target / offline evidence | Unsupported outcome |
| --- | --- | --- | --- |
| Fresh script stream versus workspace-mode reset | Saved default Floating; explicitly tile two occupied output scopes; settle normal float A and fixed-window tile override B; restart only the script, retaining Planner and clients; observe before commands, then explicitly tile both scopes again | REQ-CTL-04 resets scope modes to the saved default; REQ-RST-01b preserves float/tile identities. Real-entry/retained-engine replay accepts fresh request/command counters under a new adapter incarnation, retains both persisted sets and native frames/memberships, then proves controlled fixed-window geometry after explicit retiling. Fixed incarnation reproduces `request-superseded`. Complete-baseline native repeat confirms both phases ([outcome](../changes/archive/kde-restart-float-intent.md)) | Reference counterparts TBD. No saved-layout or scope-mode preservation selected |
| Loaded object versus evaluated controller | Load a helper after the controller; leave the helper running while removing the controller; load its replacement and inspect/run the returned ID | Historical receipts plus pinned KWin source identify numeric-path reuse: the replacement's ID addresses the surviving helper, whose already-running `run` is a no-op despite plugin-name loaded state. Teardown/verify helpers before removing an older controller; require evaluated readiness ([procedure](../live-kwin-testing.md#kwin-script-lifecycle)) | Historical helper source/plugin and direct collided-path dispatch were not captured; reference counterparts TBD. Do not reproduce the collision on a live desktop |

### Windows restart hydration discriminator

| Variant | Minimal action sequence | Observed evidence | Unsupported outcome / next discriminator |
| --- | --- | --- | --- |
| First-observation rejection of marked floats | Settle a tiled-origin sticky, float-origin sticky and ordinary intentional float at centered frames; include an unsuppressed fixed-size first-tiled Settings anchor and an ordinary tiled remainder; stop, independently restore and relaunch with apps alive; inspect first and second observations | Native 2026-10-17: bounded facts capture the fixed anchor's equal min/max `[1157,789]`, fresh session, convergence, then duplicate admission. Mixed baseline tiles all three floats on tick 1 and adopts on tick 2 without repair. Windows-only observation holds preserve starting frames with zero writes in six protected mixed trials, including four centered starts; next observation adopts. [Repair and A/B limits](../changes/archive/windows-restart-hydration-rejection.md); [historical receipts](../changes/archive/windows-restart-intent.md#agent-live-follow-up-2026-10-17). User decision 2026-10-17: shared-core duplicate-window rejection unchanged unless KDE shows the same harm; user tests KDE dogfooding | Shared-core no-focus reply/projection repair requires a separate valid-input KDE decision; unchanged here. Exact historical candidate unavailable. D25 full native acceptance, post-restart un-stick, saved-layout restoration and reference outcomes remain TBD |

### D01 native-activation discriminators (selected 2026-10-12)

Source-only [D01 research](../research/d01-fullscreen-admission.md) informed the
user's 2026-10-12 [native activation ownership decision](../decisions.md#placement-minimums-and-startup-adoption).
These fresh sequences
distinguish native activation ownership from blanket retention or forced
newcomer activation. Selected Ours: honor host approval/denial and explicit
switching, with no focus actuation while fullscreen is involved; runtime TBD.

| Variant | Minimal action sequence | Source evidence | Native / Ours runtime |
| --- | --- | --- | --- |
| Wayland admission, host policy | With tiler disabled, focus fullscreen B; open ordinary C on that output at Low FSP; repeat fresh at Medium without a valid activation token; compare with tiler enabled | KWin v6.6.3 Low ordinary-map path activates C; Medium requires native activation authorization. B's fullscreen flag is not cleared by activation. [Baseline](../research/d01-fullscreen-admission.md#native-kwin-baseline) | Untested; native pixels/input and tiler parity TBD |
| Explicit target versus navigation | Focus fullscreen B; explicitly select existing unrelated A by Alt+Tab/taskbar/target command; repeat fresh with directional focus | KWin Alt+Tab activates A without clearing B fullscreen; i3/sway target focus exits B while directional navigation can be fenced. [R-INS-06 discriminator](reference-outcomes/insertion.md#r-ins-06-explicit-selection-discriminator-fresh-fullscreen-leg) | Untested; native settlement/Ours result TBD |
| Xwayland launch provenance | Focus fullscreen B; open C with fresh user-launch timestamp; repeat with stale/zero user-time; compare tiler-off/on | KWin v6.6.3 X11 map uses timestamps/FSP; fresh authorization can activate, zero/stale can deny/restack underneath. [Baseline](../research/d01-fullscreen-admission.md#native-kwin-baseline) | Untested; exact launch provenance and runtime outcomes TBD |

### D28 born-fullscreen send discriminator (selected 2026-10-12)

| Variant | Minimal action sequence | Selected target | Ours / reference outcomes |
| --- | --- | --- | --- |
| Slotless fullscreen carry and destination exit | First observe B already fullscreen on WS1 with tiled A; keep B under the real initial hold; explicitly send B to occupied WS2; repeat fresh for numbered/relative follow/stay; exit fullscreen on WS2 | B remains fullscreen and slotless on WS2; no source admission or source/target reflow; native desktop movement only, with established follow/stay focus semantics; first exit admits B on WS2 under D5 ([decision](../decisions.md#window-state-float-sticky-maximize-fullscreen)) | Ours offline delivered: shared MoveDeferred slotless hold with no tree mutation or geometry writes, still-fullscreen slotless target until D5 first exit ([record](../changes/archive/kde-fullscreen-activation-and-send.md)); production-path cross-output integration not covered, wire plus mock-output covered; native and game outcome TBD, user-owned; unsupported reference outcomes TBD |

### 0.1 package split discriminator (Settings decision pending)

| Variant | Minimal action sequence | Offline evidence | Desired / native outcome |
| --- | --- | --- | --- |
| Native companion removed | Install core and companion; use Settings Fix once; close Settings; remove only the companion; open another ordinary window; choose tray Settings or KWin Scripts Configure | Core owns script/planner/tray; the script's `X-KDE-ConfigModule` and tray Settings depend on the companion KCM. Removal does not restore prior host keys. [Packaging record](../changes/archive/release-0.1-offline-packaging.md) | Tiling must continue and KWin remain stable per approved 0.1 gate; physical outcome TBD. Settings/Revert availability is a product decision, TBD; recommendation is a KWin-independent KCM in core or a non-effect settings companion. Reference-WM equivalents TBD, not inferred |

### KWin 6.6 shared-workspace fallback

User decision 2026-10-11; [support floor](../decisions.md#kde-support-floor).

| Variant | Minimal action sequence | Ours KDE offline outcome | Unsupported / physical outcomes |
| --- | --- | --- | --- |
| Saved per-output mode on 6.6 | On two outputs with WS1 current, start with a saved per-output mode; switch to WS2 from one output | Effective mode is shared with a degradation diagnostic; WS2 is current on both outputs. Migration refuses `per-output-unsupported`. [Legacy fixtures](../../kwin/tests/kwin-66-compat.test.ts) also cover cross-output send/follow and stay | KWin 6.7+ keeps per-output behavior; native focus/visibility and Settings journey TBD, user-owned. Plasma 5 / KWin <= 6.5 unsupported; exact reference-WM sequence TBD |

## Notation

- `H[a,b,c]` horizontal split, children left to right.
- `V[a,b,c]` vertical split, children top to bottom.
- Splits may hold 3+ children (N-ary). `H[H[a,b],c]` is a nested group.
- Focus marked with `*`, e.g. `H[A,B*]`. Order is identity order.
- Ratios shown only where load-bearing (e.g. `1/n` mover share).
- Actions are semantic (move/focus/send/float/maximize), not WM
  bindings; bindings differ per WM and are out of scope here.
- "no-op" means tree and focus unchanged. "TBD" means not established
  by the cited evidence; never read as a negative claim.
- Focus history means the per-domain MRU stack. Tests establish it by
  focusing windows in a stated order with focus moves before the action.
- Unspecified splits are equal, with no manual preselection, rules or
  minimum-size constraints. `S[A*,B]` means one tabbed tile, A active.
- Binary WMs cannot construct flat N-ary trees. For those rows use the
  same rectangles with a binary embedding and record that embedding;
  outcomes for the exact N-ary start remain TBD unless qualified.
- Startup fixtures below are proposed repeatable inputs, not recovered
  historical rectangles. Existing proof cells establish the policy,
  not a live execution of every new fixture. Rectangles are `(x,y,w,h)`
  in work-area-relative physical pixels, excluding decorations.

## WM profiles and config assumptions

| WM | Version / source | Config assumption |
|---|---|---|
| COSMIC (cosmic-comp) | User-tested version/config unknown; source `3d55cba0` (commit date 2026-10-01) | Prospective tests: tiled mode, ordinary admission without explicit direction; record orientation/gaps |
| Hyprland | Docs baseline `v0.56.2`; separate source `19fb395d` (commit date 2026-10-04) | Prospective tests: Dwindle (`general:layout`), preserve_split=false, force_split=0 follow_mouse, smart_split=false, split_width_multiplier=1, use_active_for_splits=true, default_split_ratio=1, split_bias=0 directional, permanent_direction_override=false, precise_mouse_move=false, no preselect; semantic directional move (not swap), send follows (not silent), window_direction_monitor_fallback=true; group auto_group=true (join-only), group_on_movetoworkspace=false; size_limits_tiled=false; ordinary config, no custom rules (defaults evidenced `S(S-hyp-defaults)`); Master needs a separate profile |
| bspwm | Docs baseline `0.9.12`; separate source `e11eff4` (commit date 2026-01-08) | Prospective tests: tiled, automatic_scheme=longest_side, initial_polarity=second_child, split_ratio=0.5, honor_size_hints=false; directional swap via `node -s DIR --follow`, send via shipped `node -d N` (stays; `--follow` alternate follows) |
| i3 | Local checkout `903bcd51` (2026-09-21) | Prospective tests: splith unless fixture-directed, no custom workspace/window rules; semantic `move <direction>` vs separate `swap`, native `move to workspace` is no-follow (`S(S-i3-move)` + `S(S-i3-movews)`); no implicit maximize/workspace float mode (`S(S-i3-max)` + `S(S-i3-wsmode)`); tiled-drag producer conditional/TBD where stated (shipped `etc/config` `tiling_drag modifier titlebar` vs code default modifier-only); semantic gestures, not literal keys |
| xmonad | Local checkout `284dd52c9c957cab6b6e5cc7580f2a63dafa00a7` (2026-10-03); contrib `5097a457e7a409bc9a7584dc5aa82b34c69d6dda` (2026-10-03) | Prospective tests: core `Tall nmaster=1 ratio=1/2 delta=3/100`, layout choice (Tall, Mirror Tall, Full) with Tall active; core keys stack focus (`focusUp`/`focusDown`/`focusMaster`) and stack swap (`swapUp`/`swapDown`/`swapMaster`) only, no directional core verb; workspace send is `StackSet.shift`/`shiftWin` via `insertUp`/`delete'` with source view unchanged (no view/follow); manage is `Operations.manage` `insertUp` plus fixed-size/transient float only, core `manageHook` MPlayer-only, core `handleEventHook` default-true; directional focus/move is contrib `Navigation2D` `windowGo`/`windowSwap` with `withNavigation2DConfig def` (tiled hybrid line/side, float center, screen line, no custom layout, wrap False; tiled/float separate layers, miss is no-op); EWMH is `ewmh` + `ewmhFullscreen` with `fullscreenEventHook` and default `fullscreenHooks` (`doFullFloat`/`doSink`), no fullscreen manage hook (admission itself tiles; fullscreen is post-map `ClientMessage` only); no tab stacks, no sticky, no maximize state, no workspace tiling toggle in this profile; tree fixtures may be inapplicable (Tall is flat master/stack, not N-ary H/V) |
| sway | Local checkout `1652c54b` (2026-09-21; describe `1.11-rc2-165-g1652c54b`) | Prospective tests: shipped `config.in`, no custom rules (`default_orientation` unset `L_NONE`, `workspace_layout` default); manual `splith`/`splitv` (`$mod+b`/`$mod+v`), layout toggle styles; fixture-directed splits, new-workspace layout follows output geometry (H unless portrait output); semantic `move <direction>` vs separate `swap container with ...`; native `move ... to workspace` is no-follow (source-inactive refocus), independent `workspace` command switches; `S(S-sway-default)` + `S(S-sway-wsdefault)` + `S(S-sway-move)` + `S(S-sway-movews)` + `S(S-sway-switch)` |
| qtile | Local checkout `83c697a5` = tag `v0.37.1` (2026-09-20) | Prospective tests: shipped `default_config.py`, initial active layout Columns (`layouts[0]`), Max available; `S(S-qti-default)` |
| awesome | Local checkout `0a5e50cf` (2026-08-28; describe `v4.3-1751-g0a5e50cf`) | Prospective tests: shipped `awesomerc.lua` otherwise unmodified; initial tag layout floating (`layouts[1]`); tiling scenarios select `suit.tile` (first tiling choice, `layouts[2]`); shipped defaults kept (nmaster=1, mwfact 0.5, ncol=1, fill expand, gap 0, no_overlap+no_offscreen placement, rules/focus filter); semantic focus/move use `focus.bydirection` / `swap.bydirection` locally and their `global_bydirection` equivalents for multi-output rows, not literal keys (shipped keys bind byidx focus/swap only); workspace-floating reads as the shipped floating layout, workspace enable/disable as per-tag `layout.set(tile)` / `layout.set(floating)`; `S(S-awe-default)` + `S(S-awe-tile)` + `S(S-awe-keys)` + `S(S-awe-focus)` + `S(S-awe-swap)` + `S(S-awe-layout)` |
| niri | Source `ed22699d99462f61ab171472d3ea67e844ea580d` | Shipped `resources/default-config.kdl`: default column width 1/2, presets 1/3-1/2-2/3, centering never (`S(S-nir-base)`); record display mode/rules before fill |
| PaperWM | Source `8bf6dd264f60d6c0c402b63df7b424b888959a48` | Shipped schema: `open-window-position` 0 (RIGHT of current window), `default-focus-mode` 0 (DEFAULT) (`S(S-pap-base)` plus README corroboration); verify per-case option overrides |
| karousel | Source `8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b` | Shipped `src/lib/config/definition.ts`: widths 50%/100%, Lazy scrolling on, Centered/Grouped off, no default stack (`S(S-kar-base)`); single-screen profile |
| paneru | Source `b1b6abbd3f1a4be138152b6f0389c9ff1b27a269` | Shipped config defaults (`S(S-pan-base)`); no custom rules; freeze virtual-workspace/native-tab options per case; macOS live evidence unavailable on this host |
| Ours KDE | Current `kwin/` adapter + shared Engine (`cosmic_v1`) | Requested `per-output-local` default; effective shared on KWin 6.6, per-output on 6.7+; gaps 8/8 |
| Ours Windows | Current `tiler-windows` + shared Engine | One 2560x1440 output, DPI 120/125%, gaps 8 |

Existing user tests often have unknown tested versions/config. Where
the tested version is unknown this file says so; today's source pins
are never applied retroactively to user-test cells.

## Evidence tags

Per-cell tags, kept terse via citation keys (legend below):

- `UT(date)` user-tested on that date (repo explicitly records the
  user ran that test). `UT(date-unrecorded)` where the repo records a
  user test but no date. Tested WM versions/config are unknown unless
  the source says otherwise. The 2026-08-20 date is repo-evidenced in
  `D-ref` (`[C-OBS-1]` screenshots, `[C-OBS-3]` transcript); the
  2026-08-22 date comes from screenshot filenames quoted in `D-move`
  Tests A-C.
- `S(key)` source read at a pinned commit, `key` maps to
  repo:path:line@commit in the legend.
- `D(key)` docs link/anchor, `key` maps to a repo doc path.
  This preserves the cited document's confidence level; an unverified
  community claim is not upgraded by copying it here. Linked corpus
  steps without UT are documentation evidence, not user tests.
- `TBD` unknown source behavior; stays TBD until evidenced.
- Mixed cells count once per evidence class present (e.g. a cell with
  `UT+S` counts 1 user-tested and 1 source).

Legend:

- `S-cos-fixed-hints` cosmic-comp:src/shell/layout/mod.rs:17-55 +
  src/shell/element/surface.rs:565-595 (whole-size equality; Wayland drops
  only full-zero, X11 delegates optional hints) + smithay:src/xwayland/xwm/surface.rs:1141-1175
  (existing pin e3d461a from cosmic-comp `Cargo.lock`: `min`/`max_size`
  return the `normal_hints` option fields; update at :1648-1656) +
  x11rb:src/properties.rs:257-259,348-349,667-678 (existing pin 0.13.2
  from cosmic-comp `Cargo.lock`, checksum `9993aa5b`: WM_NORMAL_HINTS
  `min_size`/`max_size` are flag-gated options; pins reused, no new
  revision selected) @3d55cba0 for cosmic-comp
  (smithay/x11rb pins reused from cosmic-comp `Cargo.lock`)
- `S-cos-fixed-admission` cosmic-comp:src/shell/mod.rs:2957-3041
  (fullscreen first; float/tile before sticky/maximize; native focus target)
  @3d55cba0
- `S-cos-fixed-toggle` cosmic-comp:src/shell/workspace.rs:1491-1519
  (explicit floating toggle maps directly without hint reclassification)
  @3d55cba0
- `S-cos-fixed-workspace` cosmic-comp:src/shell/workspace.rs:1440-1454
  (workspace enable retiles all ordinary floats without a hint check)
  @3d55cba0
- `S-cos-fixed-maximize` cosmic-comp:src/shell/mod.rs:4470-4544 +
  src/shell/workspace.rs:1002-1036 (maximize retains original layer and
  unmaximize restores it) @3d55cba0
- `S-cos-fixed-fullscreen` cosmic-comp:src/shell/mod.rs:2754-2807
  (no restore state exits to workspace-mode default; floating restore
  retains layer/geometry) @3d55cba0
- `S-hyp-fixed-hints` Hyprland:src/desktop/view/window/Window.cpp:1026-1038 +
  src/desktop/view/window/X11Backend.cpp:83-95 (Wayland either-axis with
  minima >1; X11 both positive axes) @19fb395d
- `S-sway-fixed-hints` sway:sway/desktop/xdg_shell.c:229-235
  (xdg either-axis equality with both minima nonzero) @1652c54b
- `S-awe-fixed-dynamic` awesome:lib/awful/client.lua:895-906,973-1024
  (both positive axes; hint signals recompute implicit floating unless
  an explicit floating state overrides it) @0a5e50cf
- `S-cos-add` cosmic-comp:src/shell/layout/tiling/mod.rs:219-244
  (`add_window`) @3d55cba0
- `S-cos-rem` cosmic-comp:src/shell/layout/tiling/mod.rs:255-282
  (`remove_window` proportional redistribution) and :1446-1490
  (`unmap_internal`: len>2 `remove_window`, len==2 group dissolve with the
  survivor orphaned) and src/shell/workspace.rs:648-688 (`unmap_element`:
  maximized overlay cleared first, focus sets dropped, then tiling unmap)
  and :1002-1030 (`unmaximize_request` floating unmap plus tiling recalc)
  @3d55cba0
- `S-cos-last` cosmic-comp:src/shell/layout/tiling/mod.rs:417-433
  (`last_active` resolved at admission, then `map_to_tree`) and :2826-2845
  (`last_active_window` matches the MRU-first focus entry present in the
  tree) @3d55cba0
- `S-cos-axis` cosmic-comp:src/shell/layout/tiling/mod.rs:548-616
  (ordinary admission splits the selected leaf's long edge; no-focus
  fallback splits root using output dimensions) @3d55cba0
- `S-cos-seq` cosmic-comp:src/shell/workspace.rs:1441-1452
  (`set_tiling` maps floating windows sequentially) @3d55cba0
- `S-cos-zone` cosmic-comp:src/shell/layout/tiling/mod.rs:96-103
  (`TargetZone` zone names only) @3d55cba0
- `S-cos-stack` cosmic-comp:data/keybindings.ron:84 (Super+S
  `ToggleStacking`) + src/shell/layout/tiling/mod.rs:2132-2160
  (focused window converts to a single-tab stack, focus appended) and
  :2161-2250 (stack splits back to tiles) and :2260-2330
  (group-focused branch converts the whole group into one stack, first
  pre-order surface initially active) +
  src/shell/element/stack.rs:156-180 (`CosmicStack::new` sets active tab 0)
  and :325-375 (`handle_focus` steps the active tab on Focus Left/Right)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-drop` cosmic-comp:src/shell/layout/tiling/mod.rs:1383-1400
  (`unmap_as_placeholder` stores `InitialPlaceholder` so a no-move drop
  restores the source slot) and :2677-2800 (`drop_window` zone branches
  GroupEdge/GroupInterior/WindowSplit/WindowStack plus fresh-map fallback)
  and src/shell/layout/floating/mod.rs:717-740 (floating drop with
  hovered-stack join) @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-dragstart` cosmic-comp:src/shell/mod.rs:3779-3814
  (`move_request`; client-initiated sub-pixel moves defer to a Delayed
  grab) and :4017-4030 (tiling grabs open overview mode, `Focus::Clear`)
  + src/shell/element/window.rs:743-770 (title-bar `DragStart` enters the
  same `NoMouseButtons` path) + src/input/mod.rs:928-960,1036
  (Super+Left enters move, press focuses the target)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-dragesc` cosmic-comp:src/input/mod.rs:2356-2364 (pointer-grab
  detection includes `MoveGrab`) and :2525-2543 (bare Escape intercepts to
  `PrivateAction::Escape`) + src/input/actions.rs:80-98 (Escape unsets
  pointer/keyboard grabs and clears overview/resize) +
  src/shell/grabs/moving.rs:656 (`PointerGrab::unset` is a no-op) and
  :901-960 (`impl Drop for MoveGrab` runs the normal `drop_window`
  resolution when the grab is dropped, so Escape drops rather than cancels)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-dragthresh` cosmic-comp:src/shell/grabs/delay.rs:82-84
  (Delayed pointer grab activates at 1px motion distance) +
  `S(S-cos-dragstart)` client-defer branch and `S(S-cos-drop)`
  no-move-restore branch @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-dragframe` cosmic-comp:src/shell/grabs/moving.rs:100-145
  (tiling mover rescales 0.6->1.0 over 150ms, 0.4 alpha on other outputs;
  render translates the retained window geometry by location+offset)
  and :474-492 (`StackHover` indicator) and :386-421 (pointer motion
  writes `grab_state.location`; per-output tracking, outside all outputs
  leaves cursor output/location unchanged)
  and :902-1080 (`Drop` lands in the cursor output's space and focuses the
  dropped mapped) @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-dragpress` cosmic-comp:src/input/mod.rs:881-887 (pointer press
  changes keyboard focus unless the pointer is grabbed) + `S(S-cos-dragstart)`
  (Super+Left press focuses the move target)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-hoverfocus` cosmic-comp:src/input/mod.rs:494-586 (hover focus
  only when `focus_follows_cursor` is set, with scheduled delayed focus
  via the pointer-focus state) + cosmic-comp-config/src/lib.rs:97-101,144-146
  (`focus_follows_cursor` defaults false, delay default 250ms)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af for compositor paths,
  @3d55cba0 for config
  (shipped hover retains focus; enabled variant focuses after the delay)
- `S-cos-cursor` cosmic-comp:src/shell/seats.rs:310-332
  (`active_output` is the cursor output, `focused_output` the keyboard-focus
  output; the two may differ) + src/input/mod.rs:365-376 (relative motion
  resolves the containing output) and :450-457 (containing-output motion
  target) and :715-728 (unconditional `set_active_output` after motion) and
  :802-812 (absolute motion likewise, `Keep the seat's active output
  following the pointer`) and :881-900 (press resolves its target via
  `seat.active_output()`) and :1036-1048 (`set_focus` only inside
  `if Some(target)`, else overview only: empty click writes no focus) and
  :494-587 (FFM gates only keyboard focus, delayed) +
  cosmic-comp-config/src/lib.rs:97-101,144-146
  (`focus_follows_cursor`/`cursor_follows_focus` both false, delay 250) +
  src/shell/focus/mod.rs:400-442 (cursor warps to the focused window only
  when enabled) + src/input/actions.rs:186-210 (numbered/last switches use
  `active_output`) and :1142-1180 (next/previous use `active_output`) and
  :292-381 (numbered/last/relative sends use `focused_output`) +
  cosmic-comp-config/src/workspace.rs:34-42 (shipped `OutputBound` workspace mode) +
  src/shell/mod.rs:2907-2931 (no-token ordinary map takes
  `seat.active_output()`, token `Workspace(handle)` selects the workspace)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  ([seats.rs](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/seats.rs),
  [input/mod.rs](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/input/mod.rs),
  [actions.rs](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/input/actions.rs),
  [shell/mod.rs](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/mod.rs))
  (cursor output vs keyboard-focus output split; motion-first routing,
  switch/send divergence, empty-click and coupling evidence)
  Launch-context extension at the same pin: src/input/actions.rs:1041,1062-1090
  (`Spawn` captures the active output's workspace in an activation token,
  exported as `XDG_ACTIVATION_TOKEN` / `DESKTOP_STARTUP_ID`) +
  src/wayland/handlers/xdg_activation.rs:119-162,200-219,256-258
  (workspace-token activation and focus) + cosmic-comp-config/src/lib.rs:325-330
  (default activation policy `Focus`). This captures launch-time R, distinct
  from the map-time no-token fallback.
- `S-cos-ctl-tile` cosmic-comp:cosmic-comp-config/src/lib.rs:88-95
  (`autotile` + `TileBehavior` Global/PerWorkspace) +
  src/shell/mod.rs:1468-1512 (`update_autotile[_behavior]` sets
  `tiling_enabled`; Global retoggles existing workspaces) and :632-648
  (new workspaces inherit the set's `tiling_enabled`)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-syscmd` cosmic-settings-daemon:config/src/shortcuts/mod.rs:53-76
  (`system_actions` merges system then user config) +
  data/system_actions.ron:52-55 (`WindowSwitcher: "cosmic-launcher
  alt-tab"`, `WindowSwitcherPrevious: "cosmic-launcher shift-alt-tab"`,
  `WorkspaceOverview: "cosmic-workspaces"`)
  @e37160f14d1e7ee428f973cd2848b4e95f83dfe1
  (pin from cosmic-comp `Cargo.lock`)
- `S-lch-altab` cosmic-launcher:src/app.rs:74-77 (`AltTab`/`ShiftAltTab`
  tasks) and :783-816 (Tab stepping, Alt-release activates the focused
  item) and :474-477 + src/subscriptions/launcher.rs:114-116 (activation
  forwards `Activate(item)` to the pop-launcher service)
  @49d11203116c43419d2b64844ac5c457124a8571
- `S-pop-toplevel` launcher:plugins/src/cosmic_toplevel/mod.rs:186-218
  (empty-query search appends every tracked toplevel, no
  workspace/visibility filter) and :145-160 (`Activate(id)` forwards the
  foreign handle) + plugins/src/cosmic_toplevel/toplevel_handler.rs:171-181
  (`Activate` calls `manager.activate` on the cosmic toplevel for each seat)
  @6390080a98a4a59b4e8196d28de97d3cb4d138ec
  (pin from cosmic-launcher `Cargo.lock`)
- `S-cos-topact` cosmic-comp:src/wayland/handlers/toplevel_management.rs:31-99
  (`activate` unminimizes, locates the window across all outputs/spaces,
  switches to its workspace via `shell.activate`, focuses it; sticky branch
  focuses in place) @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-dropzone` cosmic-comp:src/shell/layout/tiling/mod.rs:3436-3462
  (drop geometries from the work-area `non_exclusive_zone`, contains-walk)
  and :3416-3423 (hover cleared when overview is inactive or the pointer
  reports no location) and :3632-3640 (no containing geometry yields no
  zone) and :2789-2800 (`drop_window` with no hover fresh-maps via
  `map_to_tree`)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-dragedge` cosmic-comp:src/shell/layout/tiling/mod.rs:3660-3707
  (centre-thirds stack region, else nearest-edge `WindowSplit` direction)
  + src/shell/mod.rs:3915-3922 (grabbed tiled source unmaps to a
  `GrabbedWindow` placeholder holding its slot) + src/input/mod.rs:908-913
  (Super-held press is suppressed, never passed to the client)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-shortcut` cosmic-settings-daemon:config/src/shortcuts/action.rs:15-16
  (`Disable` masks a default binding) + config/src/shortcuts/mod.rs:91-99
  (user `custom` overlays system `defaults`) + cosmic-comp:src/config/mod.rs:262-285
  (compositor loads and hot-reloads `shortcuts`/`system_actions` on config change)
  @e37160f14d1e7ee428f973cd2848b4e95f83dfe1 for the daemon paths
  (pin from cosmic-comp `Cargo.lock`),
  @3d55cba06c9cf6f27609cdefb520f7857dba20af for cosmic-comp
- `S-hyp-moveswap`
  Hyprland:src/config/shared/actions/ConfigActions.cpp:549-563
  (`moveInDirection` refuses when fullscreen, else delegates to layout) and :565-583
  (`swapInDirection` errors with no target) @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-movews`
  Hyprland:src/config/shared/actions/ConfigActions.cpp:380-437
  (`moveToWorkspace` silent source-refocus vs follow workspace-switch+mover-focus;
  :150-166 explicit find-or-create workspaces, no trailing-empty semantic) +
  src/state/workspace/Resolver.cpp:324-331 (numeric `0` is an invalid
  workspace ID) +
  src/desktop/state/GlobalWindowController.cpp:38-80 (transfer: float
  monitor-relative retain, `group_on_movetoworkspace=false` gate, fullscreen
  internal-mode save/clear plus re-apply after `newTarget` re-admission) + src/layout/target/Target.cpp:20-35
  (`assignToSpace` had-space move path) + src/layout/space/Space.cpp:50-59 +
  src/layout/algorithm/Algorithm.cpp:62-78 (move vs add dispatch) +
  src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:67-107,262-266
  (re-admission anchor: active window on that workspace else mouse
  closest-node; a sole tiled window is the only candidate regardless of
  cursor) +
  src/layout/algorithm/floating/default/DefaultFloatingAlgorithm.cpp:127-182
  (float workspace move retains monitor-relative position)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-pin`
  Hyprland:src/config/shared/actions/ConfigActions.cpp:286-294
  (`pinWindow` float-only guard) @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-ins`
  Hyprland:src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:28-61
  (`preserve_split=false` recomputes split axis from parent geometry) and
  :63-107 (anchor: mouse-hit window on the active workspace, else active tiled
  window via `use_active_for_splits`, else first/closest node) and :146-160
  (parent-geometry long-edge axis via `split_width_multiplier`) and :213-229
  (`force_split=0` follow_mouse orders newcomer by pointer half) and :244-260
  (initial half-boxes plus geometry recalc)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-move`
  Hyprland:src/layout/LayoutManager.cpp:136-147 (delegates to space) +
  src/layout/algorithm/ModeAlgorithm.cpp:26-54 (1px-beyond-edge focal point) +
  src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:558-610
  (remove + reinsert at focal point; direct single-window split-partner
  override; silent refocuses old position; off-monitor focal with
  `binds:window_direction_monitor_fallback` crosses monitors) and :85
  (the ordering cursor is the override focal while a move sets it) and
  :262-266 (`movedTarget` carries the focal as that override) and
  :213-229 (`force_split=0` orders the mover by that focal half) and
  :650-665 (`getClosestNode` is distance-only over mapped targets with
  strict-less replacement, no history/MRU read) +
  src/state/MonitorQueryCore.cpp:64-66,99-131 (vec-only query returns the
  containing else nearest monitor, so a single-output off-edge focal resolves
  to the same monitor; containment is half-open per pinned hyprutils
  `src/math/Box.cpp:6,43-45` existing pin `95983ee` from Hyprland
  `flake.lock` at `19fb395d`, `>=0.14.0` satisfied at pin) +
  src/config/values/ConfigValues.cpp:626-627 (fallback defaults true)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-newfocus`
  Hyprland:src/desktop/view/window/Window.cpp:1156-1167 (initial map takes
  the focus monitor with no cursor branch; static rule-monitor override
  follows at :1240-1268) and :1481-1520 (ordinary newcomer
  takes focus unless no-focus rule/state, layer grab, or workspace/monitor
  silent) and :1553-1559 (silent restores the previous focus)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-cursor`
  Hyprland:src/config/values/ConfigValues.cpp:380-385
  (`input:follow_mouse=1` default) and :581
  (`misc:mouse_move_focuses_monitor=true` default) and :690-695
  (`cursor:no_warps=false`, `persistent_warps=false`,
  `warp_on_change_workspace=0`, `warp_on_monitor_change=-1` inherits
  workspace) and :763-774 (Dwindle shipped defaults incl.
  `use_active_for_splits=true`, `force_split=0` follow_mouse) +
  src/managers/input/InputManager.cpp:283 (motion resolves `PMONITOR`
  from coords) and :354-355 (`rawMonitorFocus` when the pointer monitor
  differs and the option or refocus applies) and :607-619 (empty motion
  clears keyboard focus only on refocus or with no focused window) and
  :671-691 (`FOLLOWMOUSE!=1` without refocus returns early: window focus
  stays) and :902-913 (button press requires a window for refocus) and
  :934-935 (button press updates the focus monitor) +
  src/desktop/state/FocusState.cpp:271-290 (`rawMonitorFocus` swaps the
  focus monitor, not window focus) +
  src/config/shared/actions/ConfigActions.cpp:1016-1080
  (`changeWorkspace` acts against the focus monitor) and :150-198
  (new numbered targets are created on the focus monitor) and :380-434
  (`moveToWorkspace` operates on the focused/passed window, silent
  source-refocus vs follow) and :70-138,1056-1077 (explicit
  directional/monitor focus warps) +
  src/state/workspace/Resolver.cpp:126-127
  (`getWorkspaceTargetFromString` defaults its base to the focus monitor)
  and :181-201,268-330 (monitor-relative `m+1` and relative `+`/`-` targets
  resolve from the base monitor's active workspace) +
  src/pointer/PointerController.cpp:15-28 (`no_warps` gate) +
  src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:85-107
  (same-workspace active-window anchor else cursor `getClosestNode`) and
  :213-229 (pointer-half newcomer order) and :146-160 (geometry axes)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  ([ConfigValues](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/config/values/ConfigValues.cpp),
  [InputManager](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/managers/input/InputManager.cpp),
  [FocusState](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/desktop/state/FocusState.cpp),
  [ConfigActions](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/config/shared/actions/ConfigActions.cpp),
  [Dwindle](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp))
  (focus-monitor vs window-focus split; motion-first routing, switch/send
  divergence, empty-click and coupling evidence; relative-send target
  identity is fixture-dependent, resolved in R-OUT-08)
  Launch-context extension at the same pin:
  src/config/lua/bindings/LuaBindingsDispatchers.cpp:156-176 (shortcut exec) +
  src/config/supplementary/executor/Executor.cpp:145-171 (capture focus
  monitor's active workspace in `HL_INITIAL_WORKSPACE_TOKEN`) +
  src/config/values/ConfigValues.cpp:591 (`initial_workspace_tracking=1`) +
  src/desktop/view/window/Window.cpp:1173-1199,1360-1399,1507-1512,1553-1559
  (token-target difference sets silent; unchanged launch/map workspace does
  not set silent, so ordinary newcomer focus still applies).
- `S-hyp-group`
  Hyprland:src/desktop/view/window/Window.cpp:1436-1447 (`group:auto_group`
  joins the focused window's existing group on ordinary open, never creates) +
  src/desktop/view/Group.cpp:97-173 (`add` inserts after current by default
  and makes the newcomer current) +
  src/config/shared/actions/ConfigActions.cpp:1783-1819
  (`moveIntoOrCreateGroup` needs a directional neighbor; creates a group on it
  first) and :1338-1354 (join helper focuses the mover) +
  src/config/values/ConfigValues.cpp:502,510,514 (`insert_after_current=true`,
  `auto_group=true`, `group_on_movetoworkspace=false`)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-defaults`
  Hyprland:src/config/values/ConfigValues.cpp:179 (`general:layout=dwindle`)
  and :763-774 (`dwindle:force_split=0` follow_mouse, `preserve_split=false`,
  `smart_split=false`, `permanent_direction_override=false`,
  `split_width_multiplier=1`, `use_active_for_splits=true`,
  `default_split_ratio=1`, `split_bias=0` directional,
  `precise_mouse_move=false`) and :626-627
  (`binds:window_direction_monitor_fallback=true`) and :502-504,510-511
  (`insert_after_current=true`, `focus_removed_window=true`,
  `merge_groups_on_drag=true`, `auto_group=true`, `drag_into_group=1`)
  and :514 (`group_on_movetoworkspace=false`) and :629
  (`binds:drag_threshold=0`) and :604 (`misc:size_limits_tiled=false`)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-bsp-float` bspwm:doc/bspwm.1.asciidoc:355-356
  (floating uses no tiling space, stays in tree) @e11eff4
- `S-bsp-sticky` bspwm:doc/bspwm.1.asciidoc:368-369
  (sticky is monitor-desktop scoped) @e11eff4
- `S-bsp-bal` bspwm:doc/bspwm.1.asciidoc:454-458
  (`-E` equalize / `-B` balance) @e11eff4
- `S-bsp-ins` bspwm:doc/bspwm.1.asciidoc:706-719
  (`split_ratio`/`automatic_scheme`/`initial_polarity`) @e11eff4
- `S-bsp-swap` bspwm:doc/bspwm.1.asciidoc:424-428
  (`-n` send to node / `-s` swap nodes) @e11eff4
- `S-bsp-move` bspwm:doc/bspwm.1.asciidoc:436-437
  (`-v` moves by pixels) @e11eff4
- `S-bsp-send` bspwm:doc/bspwm.1.asciidoc:418-422
  (`-d` send to desktop / `-m` send to monitor) @e11eff4
- `S-out07-bsp-mon` bspwm:doc/bspwm.1.asciidoc:287-300
  (`MONITOR_SEL` includes `DIR`; east selects the monitor in that
  direction relative to the reference monitor)
  @e11eff4
  (send target selector; transfer and follow per `S(S-bsp-send)` +
  `S(S-bsp-xfer)`)
- `S-bsp-hint` bspwm:doc/bspwm.1.asciidoc:819-820
  (`honor_size_hints` defaults false) + src/events.c:98-218
  (`configure_request`: tiled requests get a synthetic notify, allocation
  retained) and :261-297 (hint refresh plus arrange ignored under the
  default) @e11eff4 for the doc path,
  @e11eff4cb3333216ad03c815609a4ed79e08929c for src
  (tiled app-resize ignored under the shipped default)
- `S-bsp-insert` bspwm:src/tree.c:291-380 (`insert_node` automatic
  split at the anchor: longest-side axis from the anchor rectangle,
  newcomer second child under `second_child`) and src/window.c:74-82,166
  (ordinary admission anchors at the desktop focus) and :210-222
  (ordinary newcomer takes focus) @e11eff4cb3333216ad03c815609a4ed79e08929c
- `S-bsp-fltanchor` bspwm:src/tree.c:1945-1961 (`set_floating` keeps the tree
  slot vacant in place with no focus write) + src/window.c:162-166 (ordinary
  newcomers skip the floating/fullscreen vacant mark and insert at the desktop
  focus) @e11eff4cb3333216ad03c815609a4ed79e08929c
  (float-focus anchor and newcomer-layer legs; split and focus stay
  `S(S-bsp-insert)`)
- `S-bsp-xfer` bspwm:src/tree.c:1629-1652 (`transfer_node` unlinks then
  inserts at `dd->focus` with follow-gated source/destination focus) and
  :1337-1405 (`unlink_node` sibling promotion) and :291-310
  (`insert_node` empty focus/root takes root sole; otherwise automatic
  longest-side second-child split) and
  src/messages.c:171-261 (`-d`/`-m`/`-n`/`-s` with `--follow` dispatch;
  `-d`/`-m` insert at `dd->focus`)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
- `S-bsp-state` bspwm:src/tree.c:1945-1982 (`set_floating`/`set_fullscreen`
  toggle vacant in place: tree slot kept, no focus write) and :2151-2184
  (`set_sticky` has no float-only guard; off-desktop sticky transfers to
  the focused desktop) and :520-536 (`transfer_sticky_nodes` moves sticky
  subtrees to the target desktop) and :588-598 (desktop-switch
  focus-path sticky follow) @e11eff4cb3333216ad03c815609a4ed79e08929c
- `S-bsp-layout` bspwm:doc/bspwm.1.asciidoc:350-360 (floating/fullscreen
  are per-window states) and :505 (desktop layout is tiled/monocle only)
  @e11eff4
- `S-xmo-ins` xmonad:src/XMonad/StackSet.hs:472-486 (`insertUp`
  above the focused element with newcomer focus) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
- `S-xmo-shift` xmonad:src/XMonad/StackSet.hs:572-585 (`shiftWin`
  via `insertUp`/`delete'`) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
- `S-sway-default` sway:config.in:142-151 (shipped default
  `splith`/`splitv` on `$mod+b`/`$mod+v`, `layout stacking`/`tabbed`/`toggle split`)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-ins` sway:sway/tree/view.c:848-901 (admission anchors on the
  seat focus-inactive node: `container_add_sibling` after the focused
  tiling sibling, `workspace_add_tiling` fallback) and :696-730,944-955
  (`should_focus` gate: active-workspace, no `no_focus` match; ordinary
  newcomer takes focus) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (ordinary admission inserts after focus; no geometry-driven axis)
- `S-sway-split` sway:sway/commands/split.c:12-24 (`do_split` via
  `container_split`/`workspace_split`) and sway/tree/container.c:1565-1621
  (`container_split`: singleton H/V containers re-layout instead of
  nesting) and sway/tree/workspace.c:1058-1079 (`workspace_split`)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-layout` sway:sway/commands/layout.c:11-22 (`layout tabbed`/
  `stacked` parse) and :29-44 (`toggle_split_layout` flips HORIZ/VERT) and
  :117-199 (operates on the parent split like i3; single-child flatten,
  workspace wrap for new containers)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (tabbed/stacked are parent split layouts holding tabs; the path writes
  no focus, so focus stays on the previously focused child)
- `S-sway-move` sway:sway/commands/move.c:112-166
  (`container_move_to_container_from_direction`: same-parent same-workspace
  sibling swap, cousin promotion, parallel/perpendicular reparent) and
  :168-197 (`container_move_to_workspace_from_direction`: parallel flat
  insert else perpendicular focus-inactive recursion) and
  :301-415 (`container_move_in_direction`: lone-workspace force-wrap,
  singleton-child workspace-level fallback mirroring i3, promotion insert;
  off-edge falls through to next-output) and :672-711
  (`cmd_move_in_direction`: floating movers shift the frame by 10px default,
  retaining floating) and sway/commands/swap.c:37-63
  (`swap container with id|con_id|mark` is a separate explicit-target verb)
  and sway/input/seat.c:1412-1433 (`seat_get_active_tiling_child`: MRU
  child whose parent matches, selecting the perpendicular-reparent target)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (semantic `move <direction>` keeps focus on the mover)
- `S-sway-cleanup` sway:sway/tree/container.c:525-555
  (`container_reap_empty` destroys empty-only cons; single-child wrappers
  persist; explicit `container_flatten` only) and :1727-1773
  (`container_squash` merges only redundant H/V pairs) and
  sway/commands/move.c:410-412,612-614,722-724 (reap/consider-destroy after
  moves) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-movews` sway:sway/commands/move.c:198-275
  (`container_move_to_workspace`: floating movers stay floating with
  coordinate fix, tiled movers append; `container_move_to_container`:
  attach after the destination) and :470-480 (`move to workspace number`
  targets explicit workspaces, no trailing-empty/`0` shortcut) and :516-517
  (workspace destination resolves via focus-inactive tiling only) and
  :599-608 (mover focus restored to source inactive: no-follow, no switch)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (`move container to workspace` is no-follow; order is after the target focus)
- `S-sway-wskeys` sway:config.in:127-136 ($mod+Shift+n
  `move container to workspace number`) + independent `workspace`
  switch per `S(S-sway-switch)` @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-outmove` sway:sway/commands/move.c:277-298
  (`container_move_to_next_output` to the active workspace via directional
  attach) and sway/tree/output.c:316-331 (`output_get_in_direction` uses
  adjacent output only, NULL when none)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-out07-sway-out` sway:sway/commands/move.c:519-525
  (`move container to output <dir>` resolves the adjacent output and takes
  the seat focus-inactive node on it as destination) and :598-608 (mover
  focus restored to the source inactive: stay, no follow variant)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (attach-after destination per `S(S-sway-movews)`; distinct from the
  directional-exhaustion path `S(S-sway-outmove)`)
- `S-sway-wsdefault` sway:sway/tree/output.c:441-450
  (`output_get_default_layout`: configured `default_orientation`, else
  V iff output taller than wide, else H) and sway/config.c:252-253
  (code defaults `default_layout`/`default_orientation` are `L_NONE`) and
  sway/tree/workspace.c:217-219 (new workspaces take the output default)
  and :939-995 (`workspace_add/insert_tiling` splits only under a configured
  `default_layout`) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (shipped baseline: geometry-following workspace layout, no auto-split)
- `S-sway-switch` sway:sway/tree/workspace.c:731-743 (`workspace_switch`
  is the independent switch verb, focusing the target's focus-inactive node;
  never called by the move-to-workspace path)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-ffmoff` sway:sway/config.c:272-273 (defaults
  `focus_follows_mouse` yes, `mouse_warping` output) +
  sway/commands/focus_follows_mouse.c:6-18 (no/yes/always) and
  sway/commands/mouse_warping.c:5-17 (output/container/none) +
  sway/input/seatop_default.c:602-625,640 (FFM gates at :611-612: disabled
  motion leaves output focus on L) and :538-600 (default hover-focus
  checker: window focus :584-599, empty/gap cross focuses R's workspace
  :543-582) and :372-380 (empty-workspace click sets R workspace focus) +
  sway/tree/view.c:628-680 (assignments/token first, else
  seat-focus-inactive fallback to L) and :848-901 (anchor after the seat
  focus-inactive tiling sibling, else workspace append: flat `H[A,C,B]`)
  + sway/tree/workspace.c:153-176 (fresh numbers take the focused output
  absent assignment) and :512-536 (relative next/prev scan from the
  seat-focused workspace globally) and :731-743 (existing visible targets
  stay owned) + sway/commands/workspace.c:201-205 (number lookup, create only
  if missing) + sway/commands/move.c:440-520 (relative/numbered sends
  resolve from the focused subject/current workspace) and :599-608
  (source refocus, no follow) + sway/input/seat.c:1531-1549 (warp gate:
  none returns; output warp skips when the cursor is already on the focus
  output)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  ([seatop_default](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/input/seatop_default.c),
  [view](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/view.c),
  [workspace](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/workspace.c),
  [seat](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/input/seat.c))
  (FFM-off motion isolation, focus-inactive admission anchor, send source,
  empty-click focus and warp defaults)
  Launch-context extension at the same pin:
  sway/commands/exec_always.c:28-81 (internal launch context and token env) +
  sway/desktop/launcher.c:104-140,242-263 (capture/resolve the seat-focused
  workspace) + sway/tree/view.c:628-670,696-731,822-827,944
  (launch workspace before map fallback, focus on unchanged active workspace).
- `S-sway-wsmode` sway:sway/commands/workspace_layout.c:5-18
  (`workspace_layout` values default/stacking/tabbed only, no tiling on/off)
  and sway/tree/container.c:955-965 (`container_set_floating` is per-window)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-float` sway:sway/tree/container.c:910-931
  (`container_floating_set_default_size`: half-width/three-quarter-height
  clamped to config floating min/max) and :864-908
  (`container_floating_resize_and_center`: center on workspace/output) and
  :955-1030 (`container_set_floating`: float detach+reap, unfloat after
  focus-inactive tiling with fractions reset, no old-slot restore) and
  sway/commands/floating.c:14-59 (toggle dispatch, scratchpad guard) and
  sway/config.c:266-269 (floating max auto, min 75x50)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-sticky` sway:sway/commands/sticky.c:15-49 (`sticky` sets
  `is_sticky` unconditionally, moves to the active workspace floating list
  only when sticky-or-child) and sway/tree/container.c:1705-1711
  (`container_is_sticky` requires floating)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-focus` sway:sway/commands/focus.c:138-224
  (`node_get_in_direction_tiling`: sibling walk only, fullscreen drops to
  outputs) and :226-271 (`node_get_in_direction_floating`: center-delta
  search among workspace floats only, wrap to furthest opposite) and
  :473-479 (floating subjects use float search, tiled use tile search)
  and sway/commands/focus_wrapping.c:6-19 + sway/config.c:274
  (default `WRAP_YES`) and sway/tree/output.c:316-331
  (`output_get_in_direction` adjacent-only, NULL when none, so the
  fullscreen drop is fenced on a single output)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-full` sway:sway/tree/container.c:1200-1232
  (workspace-fullscreen sets `ws->fullscreen` and focuses, tree retained)
  and :1260-1341 (disable clears mode; `container_set_fullscreen` swaps
  existing workspace/global fullscreen) and sway/commands/fullscreen.c:12-58
  (no refusal branch) and sway/desktop/xdg_shell.c:395-424 +
  sway/desktop/xwayland.c:608-620 (client fullscreen converges on the same
  mode path) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-max` sway:sway/commands.c:118-129 (command inventory: `exit`/
  `reload`, no maximize verb) and sway/desktop/xdg_shell.c:385-393
  (maximize request only schedules a configure, no mode change) and
  :228-235 + sway/desktop/xwayland.c:310-340 (`wants_floating` is fixed-size
  min==max or dialog/parent/type only)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-min` sway:sway/tree/arrange.c:15-98,100-183 (fraction normalize +
  10px zeroing bound at :91-95,177-180; `MIN_SANE` 100x60 gap reservation
  only per include/sway/tree/node.h:8-9; no client-hint consult) and
  sway/tree/view.c:260-271 + sway/desktop/xdg_shell.c:149-157 +
  sway/desktop/xwayland.c:382-399 (per-protocol min/max getters) and
  sway/tree/view.c:908 (float admission evaluated at map) +
  sway/desktop/xwayland.c:756-775 (runtime hint path handles urgency only,
  no re-admission) and sway/tree/container.c:793-830
  (float clamp is config `floating_min/maximum_size`, not client hints) and
  sway/input/seatop_resize_floating.c:77-96 (client hints enforced on
  floating resize only) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-close` sway:sway/input/seat.c:234-325 (destroyed-node focus falls
  to the focus-inactive view of the parent, else workspace/last workspace)
  and :219-232 + :1378-1393 (focus-inactive is MRU order) and
  sway/tree/view.c:991-1006 (unmap detaches, reaps empty parent, rearranges)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-reload` sway:sway/commands.c:118-129 (`reload` + `exit`, no
  `restart` verb) and sway/commands/reload.c:15-34,54-71 (in-place config
  reload, not a process restart)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-geometry` sway:sway/tree/view.c:848-901 (anchor is the seat
  focus-inactive tiling node, spliced after via `container_add_sibling`) and
  sway/tree/arrange.c:15-52,100-137 (newcomer fraction 0 takes the mean,
  renormalized in the same parent; no parent growth, no alternative search)
  and :70-97,155-180 (`total_gap=min(inner*(n-1),max(0,parent-MIN_SANE*n))`
  is a gap budget only) plus include/sway/tree/node.h:8-9 (`MIN_SANE` W100/H60
  definition only) and arrange.c:91-95,177-180 (`w<10||h<10` zeroes both) and
  sway/tree/container.c:990-1025 (unfloat after focus-inactive tiling,
  fractions reset) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  ([view.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/view.c),
  [arrange.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/arrange.c))
  (no explicit numeric capacity bound in the traced Tier 1 admission/allocation
  paths; path-scoped only, no global no-limits claim)
- `S-sway-bind` sway:sway/commands.c:47,50,94,97
  (`bindsym`/`bindcode`/`unbindsym`/`unbindcode` registration) and
  sway/commands/bind.c:579-594 (bind/unbind definition dispatch)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (binding definition/removal only; no staging/Compatible/Apply/Force/
  Disable/Revert or preimage model in the inspected command inventory)
- `S-sway-switcher` sway:config.in:19 (`$menu wmenu-run`) and :74
  (`$mod+d exec $menu` external launcher; no Tab switcher bind) +
  sway/commands/focus.c:380-470 (`focus` inventory: directional/next/prev/
  output/tiling/floating/mode_toggle/parent/child single-targets; no
  listing verb) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (no native Alt+Tab/listing verb; the shipped menu is an external launcher)
- `S-sway-tray` sway:sway/commands/bar.c:31-34
  (`tray_bindcode`/`tray_bindsym`/`tray_output`/`tray_padding` registration) +
  sway/commands/bar/tray_output.c:8-27 + sway/commands/bar/tray_padding.c:8-33
  + sway/commands/bar/tray_bind.c:8-76 (`HAVE_TRAY` bar-hosted tray)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (bar-hosted tray; no owner notification-icon lifecycle in the inspected
  inventory)
- `S-sway-tdrag` sway:sway/config.c:283-284 (code defaults
  `tiling_drag` true, threshold 9; shipped `config.in` sets neither, so
  enablement is settled enabled) + sway/input/seatop_default.c:359-364
  (modifier vs titlebar producer predicates) and :439-456 (press focuses
  before drag begin) and :491-501 (tiling begin: titlebar thresholded vs
  modifier immediate; tiled non-fullscreen guard) +
  sway/input/seatop_move_tiling.c:60-82 (titlebar threshold scaled by
  output; indicator/cursor only after exceed) and :504-512 (modifier
  immediate begin) and :467-502 (begin keeps the source attached, clears
  pointer focus, no detach)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-tdrop` sway:sway/input/seatop_move_tiling.c:182-216
  (layer-surface NULL and sole-child guards; empty-workspace box) and
  :218-241 (titlebar split vs source-titlebar cancel) and :243-290 (30px
  `DROP_LAYOUT_BORDER` layout-edge walk) and :292-330 (30% closest-edge
  else centre) and :349-431 (`finalize_move`: NULL abort, empty-workspace
  add, titlebar tabbed split+indexed insert, centre `container_swap` else
  edge split+insert, sibling-share adopt, reap/arrange; no seat focus
  write except via swap) + sway/tree/container.c:1857-1921
  (`container_swap` preserves mover focus on the same workspace) and
  sway/input/seat.c:1563-1570 (move-tiling seatop handles button/motion
  only; no automatic Esc/key-press revert path) and
  include/sway/input/seat.h:17-50 (`sway_seatop_impl` has no keyboard
  callback) + sway/input/keyboard.c:433-591 (key dispatch runs
  bindings/compositor/client paths, no seatop path) and :267-285
  (compositor helper is VT-switch only) and :287-315 (pointer-keysym
  helper maps mouse-keys keysyms/motion only) +
  sway/input/cursor.c:39-123 (`node_at_coords`: scene hit then
  output-layout fallback, spanning all outputs) +
  sway/input/cursor.c:206-230 (key press only drives hide-when-typing) +
  sway/input/seatop_move_tiling.c:459-465 (impl is button/pointer-motion/
  tablet-tip/unref/end only)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-ffm` sway:sway/config.c:272 (`focus_follows_mouse` defaults
  `FOLLOWS_YES`) and sway/commands/focus_follows_mouse.c (policy verb) +
  sway/input/seatop_default.c:438-454 (plain click focuses the clicked
  container via `seat_set_focus`)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (shipped hover-focus policy plus plain-click focus)
- `S-sway-rszedge` sway:sway/input/seatop_default.c:396-409 (border
  BTN_LEFT press begins the tiling edge resize) +
  sway/input/seatop_resize_tiling.c:22-37 (`wlr_edges` edge state plus
  offset direction) + sway/commands/resize.c:66-110 (pair width
  fractions with sane-minimum clamp)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (border drag moves the pair shares with clamp)
- `S-qti-default` qtile:libqtile/resources/default_config.py:101-103
  (shipped `layouts = [Columns(...), Max()]`, initial active Columns)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-add` qtile:libqtile/layout/columns.py:132-152
  (split/num_columns/insert_position/wrap/align/initial_ratio defaults) and :266-276
  (`add_client` new-column/focused-position admission) + libqtile/layout/base.py:275-303
  (`_ClientList.add_client` insert at current, newcomer focused) + libqtile/group.py:226-244
  (`add` float-rule match, tiled layouts admit, newcomer focused when stealable)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-fltanchor` qtile:libqtile/group.py:199-206 (float focus blurs the
  tiled layouts and focuses the floating layout instead of moving any tiled
  current) + libqtile/layout/columns.py:216-221 (`focus` matches tiled column
  members only, so a floating focus leaves `current` at the last tiled focus)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (float-focus anchor leg; admission and newcomer focus stay `S(S-qti-add)`)
- `S-qti-focus` qtile:libqtile/layout/columns.py:142-144 (wrap defaults) and :385-450
  (`left`/`right` column step, `up`/`down` in-column step, `next`/`previous`; tiled columns only)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-shuffle` qtile:libqtile/layout/columns.py:452-506
  (`shuffle_left`/`shuffle_right` carry across columns or split a shared edge column,
  sole-column sole window no-op; `shuffle_up`/`shuffle_down` reorder in-column only)
  + :173-191 (`swap` exchanges two clients with heights)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-current` qtile:libqtile/group.py:186-208 (`focus` sets the group
  current and routes tiled focus into each layout) + libqtile/layout/columns.py:216-225
  (`focus` records the column current and the in-column current) + libqtile/layout/base.py:238-243
  (`focus` marks the collection current)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (tiled focus-tracking leg; `left()` then reads the stored column current)
- `S-qti-colwidth` qtile:libqtile/layout/columns.py:227-235 (`get_ratio_widths`
  equal widths via `initial_ratio`) + :237-264 (`add_column`
  equal widths via `initial_ratio`; `remove_column` width redistribution)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-colmode` qtile:libqtile/layout/columns.py:34-57 (`toggle_split`
  flag flip; `_Column.add_client`/`remove` equal height-share handling) and :290-336
  (`configure` split shows all members by height shares, stacked shows the current
  window only; every column shares the group width, no viewport)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-maxfloat` qtile:libqtile/backend/x11/window.py:1917-1926
  (`_reconfigure_floating` moves maximize/fullscreen to the floating layer via
  `mark_floating`) + libqtile/backend/wayland/window.py:770-780 (same floating-layer move)
  + libqtile/backend/x11/window.py:1290-1328 (`focus` restacks via `check_stacking`)
  + libqtile/backend/x11/core.py:944-952 (`check_stacking` re-layers a previously
  focused fullscreen window via `change_layer`) + libqtile/backend/x11/window.py:973-1007
  (`change_layer` layer/stack reorder) + libqtile/backend/wayland/window.py:634-641
  (`get_new_layer` maps maximized/fullscreen to `LAYER_MAX`/`LAYER_FULLSCREEN`)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (maximize leaves the tiled layout; fullscreen keeps its tiled slot per `S(S-qti-fsslot)`;
  final X11 cover is focus-driven restack with no shared order, Wayland cover is per-state reparent layers)
- `S-qti-remove` qtile:libqtile/layout/columns.py:278-288
  (`remove` drops emptied columns, returns current) + libqtile/layout/base.py:317-330
  (`_ClientList.remove` positional current adjust) + libqtile/group.py:246-302
  (`remove` floating vs tiled next-focus, close refocus when the closed window had focus)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-group` qtile:libqtile/backend/x11/window.py:1946-1987
  (`togroup` hide, source `remove`, target `add`, `switch_group` follows via `toscreen`)
  + libqtile/backend/wayland/window.py:526-571 (same transfer path)
  + libqtile/resources/default_config.py:75-98 (groups 1-9, `togroup` with `switch_group=True`)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-float` qtile:libqtile/backend/x11/window.py:1712-1769
  (floating setter keeps tile frame, `toggle_floating` flips) + libqtile/group.py:304-332
  (`mark_floating` removes from layouts on float, re-adds via `add_client` on unfloat)
  + libqtile/layout/floating.py:14-30,82-84 (default float rules incl fixed-size) and :169-204
  (unplaced floats center)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-fs` qtile:libqtile/backend/x11/window.py:1789-1800
  (`fullscreen` float state) and :2221-2236 (`toggle_maximize`/`toggle_fullscreen`)
  + libqtile/backend/base/window.py:262-285 (`maximized` float state at work-area size)
  + libqtile/group.py:231-232 (`auto_fullscreen` admission)
  + libqtile/backend/x11/window.py:1758-1766
  (`wants_to_fullscreen`) + libqtile/backend/wayland/window.py:432-433
  (native `handle_request_maximize` drives the maximized state)
  + libqtile/backend/x11/window.py:636-654 (`update_state` syncs
  urgent/fullscreen only, not maximized) and :2083-2101 (client
  `_NET_WM_STATE` messages echoed to the property only, no maximized drive)
  + libqtile/resources/default_config.py:47-55
  (`Mod+f` fullscreen, `Mod+t` floating)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-fs-restore` qtile:libqtile/backend/base/window.py:262-285
  (maximize saves the current float state and restores it on exit) and :305-323
  (`_set_fullscreen` saves the prior float-state on entry and restores it
  on exit) + :340-361 (`save_float_state`/`restore_float_state`: geometry
  plus saved state re-applied; restore never re-runs the float-rule match)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-drag` qtile:libqtile/backend/x11/window.py:2238-2250
  (unbound tiled `set_position`: floating tweaks frame, tiled swaps with the
  window under the pointer) and :2203-2205 (shipped `set_position_floating`
  floating tweak) + libqtile/backend/wayland/window.py:841-856 (same swap
  policy) and :837-839 (same floating tweak)
  + libqtile/resources/default_config.py:172-176 (shipped `Mod+Button1` move
  binds `set_position_floating`, `Mod+Button3` resize)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-click` qtile:libqtile/backend/x11/window.py:2003-2011
  (`handle_EnterNotify` focuses via group when shipped
  `follow_mouse_focus=True`) and :1532-1535 (`handle_ButtonPress`
  focuses via `focus_by_click`) +
  libqtile/backend/x11/core.py:892-915 (`focus_by_click` focuses the
  group window, raises only when `bring_front_click` allows) +
  libqtile/resources/default_config.py:180-182 (shipped
  `follow_mouse_focus=True`, `bring_front_click=False`)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (hover focuses; click focuses without raising)
- `S-qti-layout` qtile:libqtile/layout/max.py:31-51 (only the focused window shown)
  + libqtile/core/manager.py:1274-1303 (`next_layout`/`prev_layout` rotation)
  + libqtile/resources/default_config.py:47,101-103 (`Mod+Tab` rotates; `Max` available)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-reload` qtile:libqtile/core/manager.py:326-345 (`reload_config`
  rebuilds groups/screens from config) and :1553 (`shutdown`)
  + libqtile/core/state.py:23-40 (dump carries groups/layouts/screens/
  scratchpads only, no per-window float state)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-close` qtile:libqtile/group.py:246-302 (`remove` MRU-gated
  `previous_win`, floating vs tiled next-focus, close refocus when the closed
  window had focus) + libqtile/resources/default_config.py:198 (shipped
  `focus_previous_on_window_remove=false`)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-split` qtile:libqtile/layout/columns.py:71-80 (split/stacked column
  modes) and :380-383 (`toggle_split` flips the current column) +
  libqtile/resources/default_config.py:101-115 (profile layouts Columns plus
  Max only) @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-tweak` qtile:libqtile/backend/x11/window.py:1850-1880 (`tweak_float`
  frame write plus closest-screen transfer) and :1890-1926
  (`_reconfigure_floating` FLOATING plus `mark_floating`) +
  libqtile/resources/default_config.py:172-176 (shipped Drag binds
  `set_position_floating`/size) and :180-182 (`follow_mouse_focus` true,
  `bring_front_click` false)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-state` qtile:libqtile/core/manager.py:293-313 (`restart` dumps
  `QtileState`, backend-gated) and :326-352 (`reload_config` rebuilds
  groups/screens from config) and :1553 (`shutdown`) +
  libqtile/core/state.py:23-40 (state carries groups/layouts/screens/
  scratchpads only) + libqtile/backend/base/core.py:23 (restart supported by
  default) and libqtile/backend/wayland/core.py:242 (Wayland opts out)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-rstadmit` qtile:libqtile/backend/x11/core.py:251-289 (startup scan
  in `query_tree` order with `manage`) and :476-486 (stacking order from
  the X server) + libqtile/backend/x11/window.py:1681-1686 (`Window` init
  calls `set_group`) and :1928-1943 (`set_group` reads `_NET_WM_DESKTOP`
  with transient fallback, hides off-current) + libqtile/core/manager.py:796-819
  (`manage` re-admits to the bound/current group) and :162-173,189-193
  (state-file `apply` then layout show) + libqtile/core/state.py:42-59
  (`apply` restores group/layout/screen assignment) +
  libqtile/group.py:226-244 (per-window float-rule match with stealable
  focus) + libqtile/layout/floating.py:14-30 (shipped type/fixed-size rules)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (X11-only restart admission; Wayland has no restart journey per `S(S-qti-state)`)
- `S-qti-min` qtile:libqtile/backend/x11/window.py:553-564 (fixed-size is
  min==max only) and :805-815 (`place` `respect_hints` defaults false) and
  :860-889 (hint clamp only when requested) +
  libqtile/layout/columns.py:312-323 (tiled place without hints) +
  libqtile/layout/floating.py:240-249 (float place `respect_hints=true`) +
  libqtile/backend/wayland/window.py:189-192 (`respect_hints` TODO) +
  libqtile/backend/x11/window.py:2017-2041 (own geometry with
  `respect_hints` false) and :598-630 (only floating increments change
  layout) and :2127-2136 (hints update with no tiled promotion)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (tiled app-resize and hint-change ignored; float-only hint path)
- `S-qti-keys` qtile:libqtile/config.py:25 (`Key` static definition) +
  libqtile/core/manager.py:569-599 (grab/ungrab/regrab keys) +
  libqtile/resources/default_config.py:13-59 (static key list; reload/shutdown
  bound, no restart binding, no first-run/preset/staging verbs)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-switcher` qtile:libqtile/resources/default_config.py:18-59
  (shipped keys: mod+Tab `next_layout`, mod+space layout-local `next`;
  no Alt+Tab listing) and :126-132 (shipped bar shows `WindowName`
  current-focus display only) + libqtile/group.py:363-382 (`toscreen`
  pulls a group with no listing) and :443-483 (`next/prev_window`
  cycle the current group only) + libqtile/widget/tasklist.py:29-36,
  windowtabs.py:6-12, windowname.py:7-10 (TaskList/WindowTabs/WindowName
  display the current group only)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (no native Alt+Tab/cross-group listing verb or binding; widgets are
  current-group display, not a switcher; `toscreen` is a switch primitive
  with no listing)
- `S-qti-tray` qtile:libqtile/widget/systray.py:68-104 (bar-hosted Systray,
  `supported_backends` x11 only; hosts client icons, no owner icon lifecycle)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-wsdef` qtile:libqtile/resources/default_config.py:75-99 (static
  groups 1-9 plus togroup bindings) and :184-196 (global `floating_layout`
  rules, `auto_fullscreen`; no per-group tiling default)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-awe-default` awesome:awesomerc.lua:83-98 (`request::default_layouts`
  with `suit.floating` first) + :134 (tags use `awful.layout.layouts[1]`,
  so initial tag layout is floating)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
- `S-awe-tile` awesome:lib/awful/layout/suit/tile.lua:232-310
  (do_tile partitions tiled-client order: nmaster master at mwfact, rest into
  ncol stack columns with windowfact shares) + :164-169 (size-hint shaping
  when arranging) + awesome:lib/awful/tag.lua:49-64 (shipped defaults gap 0,
  gap_single_client, fill expand, mwfact 0.5, nmaster 1, ncol 1) +
  awesome:lib/awful/screen.lua:529-560 (tiled_clients ordered top-to-bottom;
  float/fullscreen/maximized excluded) + awesome:lib/awful/client.lua:219-251
  (client.tiled/visible over the unstacked get: newest-first global order, float/fullscreen/max excluded) + awesome:objects/client.c:2202 (manage
  prepends the newcomer at the front via client_array_push) +
  awesome:common/array.h:110-122 (array_push splices at index 0, i.e. prepend;
  array_append is the separate end-insert) + awesome:objects/client.c:3084-3116 (unstacked get walks globalconf.clients in
  order) + :3269-3304 (swap exchanges positions)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (tile order is global-client order with the newcomer first, i.e. reverse admission; stacking order differs)
- `S-awe-focus` awesome:lib/awful/client/focus.lua:171-190
  (bydirection over visible plus filter geometries via get_in_direction; miss
  changes nothing) + :76-90 (focus.filter excludes desktop/dock/splash plus
  unfocusable) + :202-229 (global_bydirection crosses screens on miss) +
  awesome:lib/gears/geometry.lua:149-169 (nearest in-direction rect, nil when
  none) + awesome:lib/awful/screen.lua:164-171 (no next screen is no-op) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (no float exclusion in the walk; shipped keys bind index focus only, see S-awe-keys)
- `S-awe-geodir` awesome:lib/gears/geometry.lua:95-106 (a direction means
  strictly greater/lesser x/y origin) and :149-168 (nearest in-direction
  rect wins, nil when none qualifies)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (occlusion geometry is load-bearing here)
- `S-awe-swap` awesome:lib/awful/client.lua:308-323
  (swap.bydirection same-screen geometric swap; miss no-op) + :342-369
  (global cross-screen move/swap) + :385-391 (swap.byidx index primitive) + :653-674
  (`move_to_screen` sets screen plus screen focus with activate, no tag write) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (local swap exchanges positions with no focus write; the global path invokes
  global focus and screen-transfer activation; settled frames follow tile recalc)
- `S-awe-tag` awesome:lib/awful/client.lua:577-587
  (move_to_tag sets screen plus tags with no view switch; focused mover emits
  activate raise) + :609-629 (toggle_tag same-screen only) +
  awesome:lib/awful/tag.lua:1637-1651 (view_only explicit tag switch) +
  awesome:lib/awful/client.lua:186-210 (jump_to switches to first tag and
  focuses; sticky covered) + awesome:lib/awful/client/urgent.lua:53-59
  (urgent.jumpto) + awesome:lib/awful/permissions/init.lua:167-219
  (activate sets focus only when visible, else urgent with raise) +
  awesome:lib/awful/permissions/init.lua:310-330 (`request::tag`
  default handler retags to the screen selected tags) +
  awesome:lib/awful/tag.lua:1813-1836 (screen-consistency strip plus
  `request::tag`) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
- `S-awe-layout` awesome:lib/awful/layout/init.lua:115-123
  (get returns the tag layout, floating fallback) + :177-180 (set is per-tag)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f (no workspace tiling flag; layout is always set per tag)
- `S-awe-manage` awesome:awesomerc.lua:467-502
  (global rule focus-filter plus raise with no_overlap plus no_offscreen
  placement; floating rule_any incl fixed/dialog roles) +
  awesome:lib/awful/permissions/init.lua:311-331 (tag handler:
  transient/sticky/selected-tags admission) +
  awesome:lib/awful/client.lua:1887-1901 (startup no_offscreen plus
  focus-history add) + awesome:lib/awful/layout/init.lua:354-375 (arrange on
  tagged/untagged/layout signals) +
  awesome:lib/ruled/client.lua:662-664 (global-rule `focus` truthy emits
  `request::activate "rules"` with raise outside startup, so the ordinary
  newcomer takes focus) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
- `S-awe-float` awesome:lib/awful/client.lua:973-1003
  (explicit over implicit float incl type/fullscreen/max/fixed-size) +
  :837-853 (set_floating restores floating_geometry; no focus write) +
  :1031-1041 (toggle/delete) + :1957 (floating is a persistent property) +
  awesome:lib/awful/layout/suit/floating.lua:112-119 (floating arrange no-op)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
- `S-awe-hint` awesome:lib/awful/permissions/init.lua:365-385 (tiled
  geometry early-return refuses the client resize) +
  awesome:property.c:166-177 (hints update emits
  `property::size_hints`) + awesome:lib/awful/client.lua:1023 (only
  implicit float updater) + awesome:lib/awful/layout/init.lua:343-360
  (listens `size_hints_honor`, not `size_hints`, so no immediate reflow
  while B stays resizable)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (tiled app-resize ignored; later arrange hint-shaping is the qualifier)
- `S-awe-fs` awesome:lib/awful/permissions/init.lua:365-434
  (geometry handler refuses tiled geometry unless floating/floating-layout/
  fullscreen/maximized context; placement maximize/restore) +
  awesome:awesomerc.lua:418-458 (Mod4+f fullscreen and Mod4+m/Ctrl+m/Shift+m
  maximize toggles with raise) + awesome:ewmh.c:325-375 (client-message
  REMOVE/ADD/TOGGLE state atoms drive the same setters, REMOVE clears incl
  native unmaximize) + :588-650 (manage-time hint read applies maximized/
  fullscreen) + awesome:objects/client.c:2699-2790 (plain boolean sets, no
  attempted-state fence) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f (plain boolean properties; no
  owner/preimage tracking or attempted-state fence)
- `S-awe-hist` awesome:lib/awful/client/focus.lua:96-144
  (MRU history add plus visible-with-fallback get) +
  awesome:lib/awful/permissions/init.lua:101-146 (check_focus prefers
  non-sticky history then sticky fallback) +
  awesome:lib/awful/client.lua:1902 (unmanage deletes history) +
  awesome:lib/awful/permissions/init.lua:809-814 (refocus hooks on
  unmanage/tag/hide/minimize/sticky) +
  awesome:objects/tag.c:376,394 (tag/untag reban synchronously) +
  awesome:banning.c:30-43 (reban unfocuses the hidden client) +
  awesome:objects/client.c:1778-1785 (ban unfocus clears focus) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
- `S-awe-sticky` awesome:lib/awful/placement.lua:949-979
  (sticky reads on every selected tag) +
  awesome:lib/awful/permissions/init.lua:94-116 (focus prefers non-sticky,
  sticky fallback) + awesome:objects/client.c:1684-1694 (C sticky reads on
  selected tags) + :2644-2657 (set_sticky plain set) + awesome:ewmh.c:56-73,248
  (sticky echoed to _NET_WM_STATE on property::sticky) + :330-337,615-625
  (state re-read at manage) + awesome:awesome.c:257 with
  objects/client.c:2241 (startup re-manage re-reads hints)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f (orthogonal property; no float-only guard in source)
- `S-awe-keys` awesome:awesomerc.lua:266-310
  (shipped focus/swap by index only plus urgent.jumpto; no directional
  binding, no switcher listing) + :323-326 (layout inc rotation) + :331-385
  (numrow view/move_to_tag/toggle_tag with no view switch on move) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
- `S-awe-switcher` awesome:lib/awful/menu.lua:524-583 (`menu.clients`
  lists every client via unfiltered `client_iterate`, activation runs
  `tags.viewmore` plus `activate raise`) +
  lib/awful/client.lua:1497-1512 (`client.iterate` over all screens with
  no tag filter) + awesomerc.lua:175-184 (shipped tasklist shows
  `currenttags` only; `client_list` exposed on right-click, no
  shipped-key switcher) and :266-285 (shipped mod+Tab is
  `history.previous`, not a listing; byidx focus/swap cycle visible
  clients) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (native all-client list plus tag-view activation exist as a programmatic/
  mouse menu, not a shipped-key Alt+Tab switcher; shipped keys list and
  focus visible clients only)
- `S-awe-drag` awesome:lib/awful/layout/init.lua:400-420
  (move_handler tiled mouse.move swap with hovered tiled current_client plus
  screen follow) + lib/awful/mouse/client.lua:21-34 (move guard
  fullscreen/maximized/desktop/dock/splash) + lib/awful/mouse/resize.lua:151-223
  (grab ends on button release only, no key-cancel branch) + :227-244 (floating
  frame write vs tiled layout-resize dispatch) +
  lib/awful/mouse/snap.lua:108-155,268-286 (aerosnap placeholder/apply
  floating-only) + awesome:awesomerc.lua:401-413 (modkey/Mod4+Button1 move,
  modkey/Mod4+Button3 resize) + :525-534 (titlebar move/resize) +
  awesome:lib/awful/client.lua:1703-1736 (activate with mouse_move action) +
  awesome:lib/awful/permissions/init.lua:167-219 (activate focus plus raise)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (tiled drag swaps on hover with no focus write; no stack/bar/preview/cancel model)
- `S-awe-sloppy` awesome:awesomerc.lua:581-585 (shipped sloppy focus:
  `mouse::enter` activates with `raise = false`) and :400-405
  (shipped button1 binds plain `mouse_click` activate)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (hover focuses without raising; click activates focus plus raise)
- `S-awe-tresize` awesome:lib/awful/mouse/resize.lua:213-225 (tiled
  clients delegate to the layout `resize_handler`) +
  lib/awful/layout/suit/tile.lua:49-69 (`mouse_resize_handler` moves
  `master_width_factor` to the pointer x)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (edge drag moves mwfact with master/stack reflow)
- `S-awe-ctl` awesome:awesome.c:111-141 (atexit saves client order to the root
  property) + :539-544 (awesome_restart re-execs) + awesome:awesomerc.lua:64,234
  (restart menu plus key) + :206 (wibox.widget.systray hosts client icons) +
  awesome:lib/wibox/widget/systray.lua:57-113 (bar-hosted tray, no owner icon
  lifecycle) + awesome:lib/awful/client.lua:1467-1477,1957 (floating persists
  across restarts; only floating registered)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (no first-run/preset/staging/Force/Disable/Revert/preimage model in the inspected
  inventory; tag layouts recreated from rc on restart)
- `S-awe-rst` awesome:awesome.c:117-141 (atexit reparents in stack order
  keeping stacking intact, then saves client order to the root property) +
  :167-199 (restore_client_order permutes the scanned set into the saved
  order) + :200-265 (startup scan manages mapped X windows in query-tree
  order, then restores the saved order) + :880-885 (scan runs before the
  startup signal with no post-scan refocus) + awesome:ewmh.c:434-456
  (desktop index retags through `request::tag`) + :524-544 (desktop
  written from live tag state) + :599-613 (desktop re-read at manage) +
  awesome:objects/tag.c:375 (tag writes desktop) + :395 (untag writes
  desktop) + awesome:objects/client.c:2268-2280 (manage emits
  `request::manage` with the startup context) +
  awesome:lib/ruled/client.lua:677-683 (rules apply on manage) +
  awesome:lib/awful/tag.lua:315-334 (tag.new selects the first tag) +
  awesome:objects/client.h:259-292 (client_raise appends on top) +
  awesome:objects/client.c:3395-3412 (raise no-op at stack top) +
  :3422-3436 (lower no-op at stack bottom) + awesome:stack.c:44-62
  (stack push/append ends) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (scan-last visible manage takes rule-focus; query-tree listing direction
  is server behavior outside the pinned source)
- `S-cos-min` cosmic-comp:src/shell/layout/tiling/mod.rs:2998-3128
  (position/allocation pass with no minimum consult; maximized/fullscreen
  skipped, otherwise unconditional `set_geometry` plus configure)
  and src/shell/layout/mod.rs:17-55 @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (tile allocation/cropping without minimum enforcement; fixed-size admission floats)
- `S-cos-geometry` cosmic-comp:src/shell/layout/tiling/mod.rs:407-436
  (one `map_to_tree` plus one `update_positions` plus one push per
  `map_internal` call) and :2898-2937 (`new_group` wraps only the anchor,
  preserving outside siblings) and :2998-3131 (`update_positions` subtracts
  gaps with no floor; degenerate geometry possible, client/native settlement
  TBD) and src/shell/element/window.rs:250-258 (ordinary-window configure
  height floored at zero after decoration subtraction; width unfloored) and
  src/shell/element/surface.rs:293-316 (pending surface size/configure) and
  src/shell/workspace.rs:1440-1452 (`set_tiling` maps floaters
  sequentially holding the frozen focus stack; ordinary-map focus still
  advances per `S(S-cos-mapfocus)`) and :1491-1519 (per-window toggle is fresh
  admission) @3d55cba06c9cf6f27609cdefb520f7857dba20af
  ([mod.rs](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/layout/tiling/mod.rs),
  [workspace.rs](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/workspace.rs))
  (serial admission geometry; no admission capacity/refusal/alternative search)
- `S-cos-bornmax` cosmic-comp:src/shell/mod.rs:3001-3022,4461-4500
  and src/shell/workspace.rs:1002-1043 @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (newcomer tiles then requested maximum overlays retained slot;
  preceding unmaximize loop targets other existing maxima)
- `S-hyp-min` Hyprland:src/layout/target/WindowTarget.cpp:236-247
  and src/config/values/ConfigValues.cpp:604 @19fb395d45314960e6f79f17994a84094f1cd4f6
  (tiled size limits default off; opt-in size clamp/recenter, not auto-float)
- `S-hyp-geometry` Hyprland:src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:63-107
  (ordinary active-tiled same-workspace anchor, else closest; NOT feasibility)
  and :109-116 (MAXSIZE-predicted float diversion is max-hint only, distinct
  from exhaustion) and :236-251 (new parent wraps the anchor only, no outside
  share steal) and :48-54 (`recalcSizePosRecursive` `.noNegativeSize` zero
  floor on pre-gap node boxes) and src/layout/target/WindowTarget.cpp:148-256
  (final tiled size subtracts gaps/reserved extents, no floor by default;
  opt-in `size_limits_tiled` clamp only) and
  src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:251-259 (recalc per
  map, serial) @19fb395d45314960e6f79f17994a84094f1cd4f6
  ([DwindleAlgorithm.cpp](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp))
  (no minimum/geometric admission check; no workspace tiling toggle per
  `S(S-hyp-wsrule)`; dwindle/master switch moves tiled targets in list order
  via remove/newTarget with unhide safeguard and conditional refocus per
  `src/layout/algorithm/Algorithm.cpp:196-221`, float membership untouched: algorithm switch, not
  a floating-to-tiling toggle)
- `S-hyp-bornmax` Hyprland:src/desktop/view/window/Window.cpp:883-897,1230-1233,1527-1562
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (pending client maximum consumed/applied at map)
- `S-hyp-float`
  Hyprland:src/config/shared/actions/ConfigActions.cpp:236-267
  (`floatWindow` toggle via `changeFloatingMode`, no refusal branch) +
  src/layout/LayoutManager.cpp:32-53 (`changeFloatingMode` clears then
  re-applies fullscreen around the toggle) +
  src/layout/space/Space.cpp:117-125 (`toggleTargetFloating` flips
  `wasTiling`) + src/layout/algorithm/Algorithm.cpp:17-39,62-94
  (per-window float dispatch for add/move/set; no workspace tiling flag) +
  src/layout/algorithm/floating/default/DefaultFloatingAlgorithm.cpp:158-182,217-220
  (`wasTiling` center retain with last size; remembers size on remove) +
  src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:262-310
  (unfloat re-admits via `addTarget` anchor; float removal promotes the
  sibling and recalculates) + src/layout/target/WindowTarget.cpp:285-295
  (`setFloating` clears pinned) + src/desktop/view/window/Window.cpp:1026-1038,1220-1222
  (`suggestsFloat`: traits or min==max hint; applied at initial map)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-fs`
  Hyprland:src/config/shared/actions/ConfigActions.cpp:338-378
  (`fullscreenWindow` toggle/set with internal/client modes) +
  src/managers/fullscreen/FullscreenController.cpp:292-421
  (`setFullscreenMode` internal/client/pinned handling) +
  src/managers/fullscreen/handler/FullscreenHandler.cpp:93-127,129-148,191-217
  (`FULLSCREEN` covers the monitor box, `MAXIMIZED` covers the work area;
  float size remembered on entry; exit leaves placement to recalc) +
  src/desktop/view/window/Window.cpp:848-897 (client fullscreen/maximize
  routes to `setFullscreenMode`, pending when unmapped, echo swallow) and
  :1498-1545 (map applies requested FS and replaces existing workspace FS)
  + src/desktop/view/window/WindowFullscreenPolicy.cpp:29-47,58-64
  (pending request store/consume; maximize-echo guard) +
  src/config/values/ConfigValues.cpp:587-588
  (`misc:on_focus_under_fullscreen` defaults 2 `exit_fullscreen`: a
  newcomer map clears the covering fullscreen/maximized window)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-bsp-min` bspwm:src/tree.c:101-134,150-170 and src/window.c:699-701
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (opt-in leaf `apply_size_hints` minimum clamp on every reflow; internal fence clamp is 32-based constraint minima when the sum fits, not hint-driven)
- `S-bsp-admit` bspwm:src/rule.c:256-293 and src/tree.c:787-795
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (fullscreen state/fixed-size floating admission; maximum flags not admission state)
- `S-bsp-close` bspwm:src/tree.c:1337-1405 (`unlink_node` sibling promotion) and :1407-1421 (`close_node` delete/kill) and :1441-1474 (`remove_node` + focus guess) and :538-578 (`focus_node` history fallback) and :645-663 (shows the resolved desktop, sets its focus and records history) and src/history.c:171-180 (`history_last_node` MRU) @e11eff4cb3333216ad03c815609a4ed79e08929c
  (close removal + MRU refocus, no spatial rule)
- `S-bsp-wsretain` bspwm:src/messages.c:793-803 (desktop removal only via
  explicit `desktop -r`, refused on the sole desktop) and
  src/desktop.c:336 (`remove_desktop`)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (empty desktops retained absent an explicit removal verb)
- `S-bsp-drag` bspwm:src/window.c:487-545 (`move_client` tiled hover-swap vs float move, cross-monitor transfer) and src/pointer.c:58-68 (buttons grabbed with the modifier) and :248-307 (ACTION_MOVE grab/track, button-release end only) and src/events.c:40-89 (`handle_event` switch has no key-press cancel branch) @e11eff4cb3333216ad03c815609a4ed79e08929c
  (pointer drag swaps on hover; no zones/cancel/preview)
- `S-bsp-ptrfocus` bspwm:src/settings.h:54 (`FOCUS_FOLLOWS_POINTER`
  defaults false) and :57-58 (`CLICK_TO_FOCUS` defaults button1,
  `SWALLOW_FIRST_CLICK` defaults false) and
  doc/bspwm.1.asciidoc:759-772 (`click_to_focus`, `focus_follows_pointer`,
  `pointer_follows_focus` settings) and src/window.c:200 (client enter
  mask subscribed only while `focus_follows_pointer`) and
  src/events.c:375-398 (`button_press` ACTION_FOCUS plus replay) and
  :425-472 (`motion_notify` motion focus with the unintentional-motion
  filter) and src/messages.c:1753-1771 (setting toggle rewires enter
  masks and the motion recorder)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (shipped hover stays unfocused while button1 click focuses and
  replays; enabled hover focuses on motion with no scheduled delay)
- `S-bsp-ptrresize` bspwm:src/pointer.c:259-307 (`track_pointer`
  motion drives `resize_client` with pointer deltas) and
  src/settings.h:30 (`pointer_modifier` defaults Mod4) and
  src/window.c:547-590 (tiled resize adjusts the fence split_ratio
  by dx/fence-width clamped to [0,1] with reflow)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (modifier resize_side/corner grab moves the shared fence share)
- `S-bsp-restore` bspwm:src/query.c:38-67 (`query_state` dump incl history/stack) and :116-183 (node/client dump incl sticky/state) and src/restore.c:111-162 (restart replaces monitors, restores history/stack) and :345-409 (node sticky restore) and :436-474 (client state restore) and :188-192 (regenerate call site) and src/tree.c:2285-2293 (`regenerate_ids_in` skips client leaves, so dumped X window IDs persist) and src/window.c:44-82 (`schedule_window`/`manage_window` fresh admit via rules) and :428-447 (`adopt_orphans` manual scan only) and src/bspwm.c:154-156 (startup `-s` restore) and :275-326 (restart dump + re-exec) and src/messages.c:1250-1263,1317-1320 (`-d`/`-l`/`-r` verbs) @e11eff4cb3333216ad03c815609a4ed79e08929c
  (restart persists sticky/state/focus; dump fields round-trip:
  query.c:107 `focusedNodeId`, :122 node `id` (live X window for client leaves), :124 `splitRatio`, :179 `floatingRectangle`,
  :57 history; restore.c:320-322, :359 node `id`, :361, :364, :464-466, :188-192)
- `S-bsp-fs` bspwm:src/messages.c:287-318 (`node -t --state` incl `~` alternate) and src/tree.c:1889-1943 (`set_state` last_state memory, vacant in place) and :1963-1987 (`set_fullscreen`) and :1989-2004 (`neutralize_occluding_windows` clears a covered fullscreen to last_state on focus) and src/stack.c:123-133 (fullscreen outranks tiled in `stack_level`) and src/events.c:474-490 (EWMH fullscreen ADD/REMOVE/TOGGLE with ignore gates) and src/settings.h:60 (default 0, honored both ways) @e11eff4cb3333216ad03c815609a4ed79e08929c
  (project toggle and EWMH converge; newcomer focus clears covered fullscreen; no refusal branch)
- `S-bsp-ctl` bspwm:src/messages.c:287-358 (node `-t` state incl `~` alternate, `-g` flags hidden/sticky/private/locked/marked only) and :1250-1327 (wm `-d` dump/`-l` load/`-a` add-monitor/`-O` reorder/`-o` adopt-orphans/`-g` status/`-h` history/`-r` restart only) @e11eff4cb3333216ad03c815609a4ed79e08929c
  (no preset/tray/binding-staging model in the inspected inventory)
- `S-bsp-switcher` bspwm:examples/sxhkdrc:11 (`dmenu_run` external program
  launcher) and :66-82 (shipped focus verbs: directional, path-jump,
  `{next,prev}.local` current-desktop cycle, `{prev,next}.local` desktop
  step, `last` node/desktop toggle, `older/newer` history; no Alt+Tab
  listing) + doc/bspwm.1.asciidoc:82-107 (`NODE_SEL` incl
  `CYCLE_DIR`/local/last/newest/older/newer, no cross-desktop listing
  selector) @e11eff4cb3333216ad03c815609a4ed79e08929c
  (no native switcher/listing verb; last/history verbs are single-targets,
  not a B listing; `bspc query` lists have no traced switcher activation)
- `S-i3-min` i3:src/render.c:43-124 (tiled `render_con` allocation
  with size-hint ignore note, no minimum clamp) and
  src/manage.c:461-474,528-533 (fixed-size min==max admission floats) and
  src/floating.c:76-130,187-229 (`floating_check_size` float-only min/max
  clamp) @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (tiled allocation unclamped; float clamp and fixed-size float admission separate)
- `S-i3-fixed-runtime` i3:src/handlers.c:1007-1020 (`handle_normal_hints`
  routes hint updates to `floating_check_size` plus render for floating
  containers only; tiled containers get no re-admission)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (post-admission hint changes never re-float tiles)
- `S-i3-geometry` i3:src/tree.c:149-181 (`tree_open_con` inserts after the
  focused tiling sibling within the existing parent, not a forced binary
  split) and src/con.c:165-205 (after-focus sibling attachment) and
  :1140-1180 (`con_fix_percent` assigns each zero-percent
  newcomer the mean of the existing percents, then renormalizes the same
  parent only; no parent growth) and
  src/render.c:91-124,176 (tiled render ignores hints) plus src/util.c:53-57
  (`rect_sanitize_dimensions` floors client/frame dimensions at >=1) and
  src/manage.c:44-68 (startup iterates X query-tree children serially via
  `manage_window`: fresh adoption, not geometry preservation) and
  src/commands.c:1668-1687 (`reload` is config-only, no adoption)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  ([tree.c](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/tree.c),
  [con.c](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/con.c),
  [manage.c](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/manage.c))
  (no exhausted-size check; no ancestor steal; no alternative search)
- `S-i3-admit` i3:src/manage.c:139-143,402-421 and src/con.c:428-474,
  src/x.c:834-864 @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (fullscreen atom admission; maximize flags derived from layout)
- `S-i3-ffmoff` i3:src/handlers.c:91-125 (`check_crossing_screen_boundary`
  early-returns when FFM is disabled) and :127-201 (enter-window returns
  disabled at :175-177) and :203-260 (root motion returns disabled at
  :219-221: cursor R changes neither focus nor output) +
  src/config_directives.c:447-448 (`focus_follows_mouse` sets the disable
  flag; zero-init default is enabled) and :451-455 (`mouse_warping`
  none/output) + include/data.h:142-143 (`POINTER_WARPING_OUTPUT=0`
  default) + src/x.c:1549-1555 (warp gate) + src/workspace.c:555-559
  (cross-output warp on workspace switch) and :131-165 (fresh hidden
  numbers assigned to the focused output absent a rule) and :752-890
  (`next_on_output` scans the focused output) and :581-750 (plain
  next/prev scan global workspaces) + src/commands.c:855-880 (numbered
  switch shows the existing workspace on its owner) and :227-369 (sends
  move the focused subject; relative source is the focused workspace) +
  src/con.c:1569-1585 (`con_move_to_workspace` focused subject, source
  focus retained) + src/manage.c:344-365 (ordinary admission at focus;
  token/startup legs :333-343 and assignments :289-332 are distinct) and
  :661-664 (newcomer activation) + src/tree.c:149-181 and src/con.c:165-205
  (after-focus sibling attach: flat `H[A,C,B]`, not a leaf wrap) +
  src/click.c:205-277 (any workspace click focuses it: `workspace_show`
  plus activate, so empty R selects R)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  ([handlers.c](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/handlers.c),
  [workspace.c](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/workspace.c),
  [manage.c](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/manage.c),
  [click.c](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/click.c))
  (FFM-off motion/change isolation, focused-output admission, send
  source, flat-sibling anchor, empty-click focus and warp defaults)
  Launch-context extension at the same pin: src/commands.c:1243-1268
  (`exec` calls `start_application`) + src/startup.c:136-176,195-196
  (capture `con_get_workspace(focused)` in startup context) +
  src/manage.c:226,328-340,419-441 (startup workspace before ordinary map
  fallback; newcomer focus when visible on the focused output).
- `S-xmo-admit` xmonad:src/XMonad/Operations.hs:90-124,328-335
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (core-only ordinary manage/tile path; configured hooks/contrib remain TBD)
- `D-move` [cosmic-move-conformance.md](../cosmic-move-conformance.md)
  (S1-S3 UT 2026-08-20, version unknown; S4-S23 authored observations;
  S1-17/S3-05 unconfirmed corrections)
- `D-ref` [reference-wm-comparison.md](../reference-wm-comparison.md)
  sections 1-10
- `D-ref11` [reference-wm-comparison.md section
  11](../reference-wm-comparison.md#11-directional-window-movement-with-no-candidate-window-cosmic)
  (COSMIC move model; bspwm/Hyprland move notes unverified tier)
- `D-prof`
  [reference-wm-profile-support.md](../research/reference-wm-profile-support.md)
  (pins v0.56.2 / 0.9.12 / v50.0.1, 2026-09-17)
- `D-prior` [prior-art.md](../research/prior-art.md) (2026-10-03
  inventory)
- `D-dec-ww` [decisions.md](../decisions.md)
  (Workspaces and Window State entries; formerly one combined section)
- `D-dec-cos`
  [decisions.md](../decisions.md) (Move, Engine Operations and Visuals entries;
  formerly the combined COSMIC Movement And Groups section)
- `D-dec-x`
  [decisions.md](../decisions.md)
  (cross-platform behavior entries; formerly Cross-Platform Behavior)
- `D-dec-win` [decisions.md](../decisions.md) (Windows platform sub-bullets:
  managed workspaces, minimums)
- `D-dec-nest`
   [decisions.md](../decisions.md#pointer-drag-and-drop) ("Pointer,
   Drag And Drop")
- `D-dec-drag`
  [decisions.md](../decisions.md#pointer-drag-and-drop)
  ("Pointer, Drag And Drop")
- `D-win-drag`
  [windows-mouse-drag.md](../changes/archive/windows-mouse-drag.md)
  (accepted same-output title/Win producers and preview; synthetic proof,
  physical checks and exact unexecuted fixtures remain explicit)
- `D-dec-max` [decisions.md](../decisions.md#window-state-float-sticky-maximize-fullscreen)
  ("Window State: Float, Sticky, Maximize, Fullscreen")
- `D-place`
  [placement-correctness.md](../changes/archive/windows-placement-correctness.md#evidence-and-current-state)
  (synthetic/API proof 2026-10-03, physical feel user-owned;
  [candidate rows](../changes/archive/windows-placement-correctness.md#candidate-matrix-rows))
- `D-max`
  [windows-maximise.md](../changes/archive/windows-maximise.md#accepted-evidence)
  (synthetic proof)
- `D-fs` [windows-fullscreen.md](../changes/archive/windows-fullscreen.md)
  (scoped proof)
- `D-float` [windows-float.md](../changes/archive/windows-float.md)
  (gates pass, behavior rows user-owned)
- `D-tray-task`
  [windows-tray-first-run.md](../changes/archive/windows-tray-first-run.md#accepted-verification-and-outcome)
  (posted TaskbarCreated re-adds the GUID icon; actual menu Stop
  cleans up; owner-side proof, not reference-WM behavior)
- `D-sticky`
  [windows-sticky-float.md](../changes/archive/windows-sticky-float.md)
  (scoped helper proof, remainder user-owned)
- `D-kde-follow` [KDE post-Windows follow-ups](../changes/archive/kde-post-windows-followups.md)
  (2026-10-05 fixture-first explicit toggle repair and KDE/Engine coverage;
  offline evidence, physical delivery remains TBD)
- `D-min-games` [minimums and game admission](../research/cross-platform-core/post-windows-audit.md#2026-10-05-follow-up-q2-minimum-infeasibility--q3-games)
  (2026-10-05 current project source and pinned upstream comparison;
  unsupported exact native outcomes remain TBD, not inferred from source policy)
- `S-ours-toggle` omnitiler:crates/tiler-windows/src/tiling_sys.rs:6482-6491 @ad6d69c
  (historical KDE attempted-state fence, removed with the legacy route, vs
  discrete Windows dispatch; source paths, not physical repeat-delivery proof)
- `S-ours-fs-exit` omnitiler:crates/tiler-windows/src/tiling.rs:635-677 @ad6d69c
  (historical KDE public fullscreen setter, removed with the legacy route, vs
  Windows project-preimage exit gate)
- `S-ours-ovref` overlay isolation parity: omnitiler:
  crates/tiler-windows/src/tiling.rs:377-394 (`overlay_refusal` for
  directional/pointer routes; focus carries no write and stays allowed) +
  crates/tiler-windows/src/tiling_sys.rs:5983-5998 (maximized focused
  mover refuses before Engine mutation) + kwin/src/world-apply.ts
  (overlay guards: fit-excluded, fullscreen, and sticky rows take no
  geometry writes)
  @9241c94
- `S-ours-winsend` omnitiler:crates/tiler-windows/src/
  tiling_sys.rs:10532-10548 (tiled-to-tiled Engine send with source reflow
  and follow) and :10720-10731 (tiled maximized member sends; fullscreen
  mover refuses with no writes) and :10883-10910 (retained maximized
  mover proceeds with flag recheck; target allocation kept, overlay
  geometry never writes)
  @9241c94
- `S-ours-send-boundary` historical: the retired `isSendActive`
  coordination hook (removed with the legacy route) plus the standalone
  same-output prototype (dev-only, never a production route);
  the host-native desktop-send journey remains untraced and world-route
  re-verification is TBD
  @9241c94
- `S-ours-sticky-restart` omnitiler:crates/tiler-windows/src/tiling_sys.rs:8867-8898,8981-8999 @ad6d69c
  (historical KDE native-sticky unknown-float adoption, removed with the
  legacy route, vs Windows marker consumption into normal float)
- `S-ours-overlay-unfloat` omnitiler:crates/tiler-windows/src/tiling.rs:395-410,
  crates/tiler-windows/src/tiling_sys.rs:7788-7813 @ad6d69c
  (historical KDE floating-target overlay dispatch bypass, removed with the
  legacy route; Windows refuses; settled KDE native outcome remains TBD)
- `D-alt-tab`
  [hidden-workspace Alt+Tab research](../research/windows-port/alt-tab-hidden-workspaces.md)
  (official docs, pinned KWin/reference source and upstream reports; no live probe)
- `D-cosmic-kb` COSMIC keybindings.ron / support articles via `D-ref`
  (Super+O/S/G/M/F11 bindings)
- `S-cos-flt-focus` cosmic-comp:src/shell/mod.rs:4136-4210 and
  src/shell/layout/tiling/mod.rs:1835-1852,1899-2087
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (ordinary tiled subjects search the tile tree; floating subjects search
  ordinary/sticky floats by top-left coordinate delta on the requested axis;
  Up/Left include equal positions, first minimum tie; Down/Right strictly
  positive movement, last nearest tie; sticky Space precedes ordinary Space)
- `S-cos-focus-fallback` cosmic-comp:src/input/actions.rs:535-541,745-810
  and src/shell/mod.rs:2273-2302
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (no local focus target falls through to workspace/output navigation;
  no next output means no output switch)
- `S-cos-flt-move` cosmic-comp:src/shell/mod.rs:4225-4253,
  src/shell/layout/floating/mod.rs:184-189,252-265,1184-1288 and
  src/input/actions.rs:812-881 @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (free float snaps to a half; quarter/maximize transitions and repeated
  outward movement use floating snap state, not tiling-tree admission)
- `D-float-nav`
  [KDE floating directional navigation](../changes/archive/kde-floating-directional-navigation.md)
  (2026-10-05 user decision; offline focus/half-snap and reconcile regressions,
  live acceptance pending; Windows and stateful snap transitions pending)
- `S-cos-sticky-layer` cosmic-comp:src/shell/mod.rs:4769-4800,4834-4849
  and src/shell/workspace.rs:418-471
  and src/shell/focus/order.rs:419-422 (sticky stage renders independent
  of the active workspace) and src/input/mod.rs:2883-2915 (sticky
  hit-tested before workspace windows) and src/shell/focus/mod.rs:700-716
  (sticky keyboard focus stays valid across workspace switches) and
  src/shell/mod.rs:525-553 (workspace switch flips only the set active
  index, sticky layer untouched)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (sticky windows use a separate output-set floating layer, visible across
  switches on the same output; pinned denotes workspaces)
- `S-hyp-flt-focus` Hyprland:src/desktop/state/WindowQuery.cpp:23-46,67-99,130-207,209-256
  and src/config/shared/actions/ConfigActions.cpp:476-526
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (tiled search excludes ordinary floats even on retry; float search uses
  angle/distance among floats, with monitor/edge fallback)
- `S-hyp-flt-move` Hyprland:src/layout/algorithm/Algorithm.cpp:163-168
  and src/layout/algorithm/floating/default/DefaultFloatingAlgorithm.cpp:255-272
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (floating directional move snaps position to work-area edge, retains size)
- `S-hyp-flt-pin` Hyprland:src/config/shared/actions/ConfigActions.cpp:286-294
  @19fb395d45314960e6f79f17994a84094f1cd4f6 (pin is float-only)
- `S-hyp-close`
  Hyprland:src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:268-310
  (live-tree removal: sibling promotion + recalc; last-node erase) +
  src/desktop/view/window/Window.cpp:1668-1713 (unmap removes group
  membership and layout target, then refocuses only if the closed window
  was focused: grouped next else `focus_on_close` cursor/next/MRU branch;
  no candidate on an emptied workspace runs the pointer refocus) +
  src/desktop/view/window/Window.cpp:485-527 (empty-close runs only for
  special workspaces via `misc:close_special_on_empty`; no ordinary
  empty-destroy path) +
  src/layout/algorithm/Algorithm.cpp:231-261 (`getNextCandidate`: tiled
  closest-node else tiled-back/float-back; floating/MRU mode uses reverse
  window history with no closing-window exclusion) +
  src/desktop/state/FocusState.cpp:130-155 (focusing the unmapped
  self-candidate clears to none) +
  src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:481-491
  (tiled next is closest node by old middle, else first) +
  src/config/values/ConfigValues.cpp:383-384 (`input:focus_on_close`
  default `next`=0, cursor=1, mru=2)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-groupop`
  Hyprland:src/config/shared/actions/ConfigActions.cpp:963-977
  (`toggleGroup` creates a one-window group else destroys) and :979-993
  (`changeGroupActive` steps current, errors on single-member group) +
  src/desktop/view/Group.cpp:27-69 (create replaces head target) and
  :300-315 (`moveCurrent` wraps) and :317-342 (`setCurrent` swaps visible
  tab, refocuses only if the group was focused)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-drag`
  Hyprland:src/layout/supplementary/DragController.cpp:135-157
  (threshold-reached tiled pick-up floats via `changeFloatingMode`,
  remembers 0.8489 tile-size float size) and :159-251 (`dragBegin`
  threshold init, `rawWindowFocus` + raise at press) and :336-357
  (threshold gate on motion) and :401-412 (move writes floating
  position + warp, snap when enabled) and :486-491 (floating middle
  crossing a monitor reassigns to that monitor's active workspace) +
  src/config/values/ConfigValues.cpp:629-631 (`binds:drag_threshold`
  default 0 immediate, `drag_center_window` default true)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-dragend`
  Hyprland:src/layout/supplementary/DragController.cpp:252-334
  (drop: decoration `DRAG_END` hit consumes; grouped hover +
  `drag_into_group` + `canBeGroupedInto` joins, else still-floating
  tiled-origin re-tiles via `changeFloatingMode` with original float
  size remembered; `setTargetGeom` is floating-only; drop focuses the
  dragged) +
  src/desktop/view/window/WindowGroupMembership.cpp:75-90
  (`canBeGroupedInto` lock/deny/merge gates) +
  src/render/decorations/CHyprGroupBarDecoration.cpp:425-449
  (groupbar drop inserts at bar index and focuses the dragged) +
  src/config/values/ConfigValues.cpp:502-514 (`insert_after_current`,
  `focus_removed_window`, `merge_groups_on_drag`, `drag_into_group`
  0/1/2, `group_on_movetoworkspace=false`)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-keyend`
  Hyprland:src/keybinds/Manager.cpp:226 (any key press runs
  `endDragTarget` before dispatch) and :356 (mouse press ends a prior
  drag; no Esc-specific branch) +
  src/config/shared/actions/ConfigActions.cpp:1673-1715
  (`mouse:movewindow` dispatcher support: release ends drag, press
  hit-tests with reserved/input/floating extents and begins a drag;
  decoration `DRAG_START` hit skips compositor drag)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-follow`
  Hyprland:src/config/values/ConfigValues.cpp:380-385
  (`input:follow_mouse` defaults 1, threshold 0, `mouse_refocus` true) +
  src/managers/input/InputManager.cpp:237-273 (`mouseMoveUnified`
  FFM vs CLICK focus reasons) and :900-918 (press refocuses with
  raise unless `follow_mouse=3`)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (shipped hover-focus policy plus plain-click focus; drag press is
  `S(S-hyp-drag)`)
- `S-hyp-edgeresize`
  Hyprland:src/config/values/ConfigValues.cpp:181-184
  (`general:resize_on_border` defaults false, grab extend 15) +
  src/managers/input/InputManager.cpp:880-895 (border click begins
  an MBIND_RESIZE drag only when enabled) +
  src/layout/supplementary/DragController.cpp:27-29,432-476
  (resize modes; float min/max clamp, tiled pixel deltas to resizeTarget)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (bare edge starts no resize at shipped default; enabled-variant
  share outcome TBD)
- `S-hyp-monlife`
  Hyprland:src/state/workspace/LifecyclePolicy.cpp:95-120 (disconnect moves
  workspaces to the first remaining monitor with return-address record plus
  remembered active) and :39-93 (reconnect returns same-address workspaces
  with remembered-active activation, else default/recovery) +
  src/state/workspace/LifecyclePolicyAdapter.cpp:143-158 (move via
  placementController whole-workspace reassignment, activate via
  changeWorkspace) +
  src/state/workspace/PlacementController.cpp:301-329 (whole-ws reassignment:
  float reposition, FS setBox, pin stays) +
  src/output/Monitor.cpp:378-456,471-473 (disconnect path: migration call,
  cursor warp to backup, monitor refocus)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (evacuation with return affinity; focused node needs fixture history)
- `S-hyp-wsrule`
  Hyprland:src/config/shared/workspace/WorkspaceRule.hpp:11-45
  (workspace rule fields: monitor/persistent/gaps/border/layout, no
  tiling/floating default) + `S(S-hyp-float)` per-window float
  dispatch (no workspace tiling flag)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-shortcut`
  Hyprland:src/keybinds/Registry.cpp:57 +
  src/keybinds/Manager.cpp:800-801 (`findShortcutConflict` lookup) +
  src/config/lua/bindings/LuaBindingsToplevel.cpp:399-407 (`unbind`) +
  src/config/shared/actions/ConfigActions.cpp:200-1824 (dispatcher
  inventory at pin lists no first-run/preset/tray/staging/Force
  actions); tray/first-run search hits only xwayland tray atoms and
  unrelated `compatible` strings, not owner controls
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-switcher`
  Hyprland:src/config/lua/bindings/LuaBindingsDispatchers.cpp:930-951
  (`hl.dsp.window.cycle_next`) + src/desktop/state/WindowQuery.hpp:21-27
  (`SWindowCycleOptions` defaults `visible=false`) +
  src/desktop/state/WindowQuery.cpp:260-317 (same-workspace match when
  `visible=false`) + src/config/shared/actions/ConfigActions.cpp:70-100
  (`switchToWindow` focus path, no listing UI) and :606-630
  (`focusCurrentOrLast`/`focusUrgentOrLast` history single-targets) +
  src/config/lua/bindings/LuaBindingsDispatchers.cpp:1091-1145
  (`hl.dsp.focus` inventory: direction/monitor/workspace/window-selector/
  urgent_or_last/last) + example/hyprland.lua:31-33,263
  (`menu = "hyprlauncher"` external launcher, no Tab switcher bind)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (no native Alt+Tab/listing verb; cycle is same-workspace by default;
  last/urgent are history single-targets, not a B listing; the shipped
  example launcher is external)
- `S-bsp-flt-focus` bspwm:src/query.c:583-584,
  src/tree.c:1124-1149,2250-2261, src/geometry.c:49-154 and
  src/settings.c:108 @e11eff4cb3333216ad03c815609a4ed79e08929c
  (unqualified directional selector includes tiles/floats; boundary distance
  first, history rank only breaks ties; default tightness HIGH)
- `S-bsp-flt-swap` bspwm:src/tree.c:101-134,1489-1623
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (same-desktop node swap retains client state/floating rectangle and focus;
  tiled arrangement is recomputed)
- `S-i3-flt-focus` i3:src/tree.c:503-577 and src/commands.c:1292-1324
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (directional `focus` dispatches via `tree_next`; tiled walk excludes
  floating list; floating left/right cycles that list with wrapping; up/down
  returns no target; sticky does not change this path)
- `S-i3-flt-move` i3:src/commands.c:1554-1588 and
  parser-specs/commands.spec:407-411
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (bare directional move shifts floating frame by 10px, retains floating)
- `S-i3-ins` i3:src/tree.c:149-181 (`tree_open_con` attaches to the
  focused parent) and src/con.c:165-205 (`_con_attach` inserts after the
  first tiling container in the parent focus stack) and src/manage.c:423-459,
  661-664 (visible-workspace newcomer takes focus via `con_activate`)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (ordinary admission inserts after focus; no geometry-driven axis)
- `S-i3-layout` i3:src/commands.c:1599-1624 (`layout tabbed`/`stacked`
  sets the parent split layout) and src/con.c:1620-1630 (tabbed behaves
  HORIZ, stacked VERT) and src/con.c:1997-2013 (`con_set_layout` retargets
  the focused window's parent, same children) and src/con.c:2109-2142
  (`con_toggle_layout` retargets the parent; `split` flips
  L_SPLITH/L_SPLITV) @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (tabbed/stacked are parent split layouts holding tabs; neither path
  writes focus, so focus stays on the previously focused child)
- `S-i3-solesave` i3:src/con.c:2019-2056 (workspace parent with children
  wraps them into a new split container carrying the requested split
  layout; focus order preserved) + src/workspace.c:55-70 (shipped auto
  orientation: wide output SPLITH, portrait SPLITV) and src/config.c:215-216
  (code default `NO_ORIENTATION` auto)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (sole-leaf toggle saves the flipped layout into a wrapper, so the next
  admission inherits it)
- `S-i3-move` i3:src/move.c:259-353 (`tree_move` same-orientation swap,
  force-orientation wrap, lone-workspace output-directed fallback) and
  :355-404 (move into the container above via `insert_con_into`) and
  src/move.c:65-171 (`insert_con_into` detach/reinsert) and
  src/workspace.c:953-984 (`ws_force_orientation` wraps the workspace) and
  src/tree.c:657-685 + src/con.c:2241-2245 (only empty cons close; narrow
  flatten needs a redundant orientation pair, so single-child wrappers persist)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (semantic `move <direction>` keeps focus on the mover)
- `S-i3-movews` i3:src/con.c:1569-1580 (`con_move_to_workspace` targets the
  focused descendant) and :1321-1324,1367-1386 (floating movers move as
  wrappers; floating targets fall back to the workspace) and :1415-1464
  (after-focused attach; focus restored to the source: no-follow) and
  src/commands.c:223-229,316-369 (`move to workspace` name/number via the
  same path) @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (`move container to workspace` is no-follow; order is after the target focus)
- `S-i3-wskeys` i3:etc/config:152-162 (Mod1+Shift+n
  `move container to workspace number`) + independent `workspace`
  switch per `S(S-i3-ws)` @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-i3-flt-toggle` i3:src/floating.c:277-281,328-342,367 (`floating_enable`
  detaches to a workspace floating wrapper framed from stored geometry with
  size clamp) and :372-383 (leader-centered else workspace-centered fallback
  when geometry is (0,0)) and :387-403 (output containment fix with
  workspace re-center) and :419-447 (`floating_disable` inserts after the
  tiling-focused descendant with percent reset, no old-slot restore) and
  src/commands.c:1142-1168 (float toggle dispatch)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-i3-sticky` i3:src/commands.c:1515-1548 (sticky sets on any window con,
  no float-only guard, then pushes) and src/output.c:87-123 (only floating
  stickies move to the visible workspace) and src/ewmh.c:146-155 (sticky
  effective only when floating) and src/manage.c:476-487 (admission sticky
  hints) and src/load_layout.c:578-580 + src/ipc.c:645-646
  (serialized `sticky` round-trip on restart) and src/con.c:1388-1394 +
  src/floating.c:815-835 (same-output sticky push retains the frame)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-i3-fs` i3:src/con.c:1188-1309 (fullscreen toggle/enable/disable is a
  mode flag, tree retained; enable focuses the target) and
  src/commands.c:1488-1509 (command path) and src/handlers.c:681-690
  (client FULLSCREEN message uses the same toggle) and
  src/render.c:126-138,253-268 (overlay render; floating blocked except popup
  modes) and src/tree.c:515-520 (directional focus from fullscreen drops to
  workspace level) and src/commands.c:1431-1481 (criteria `focus` via
  `con_activate_unblock`) and src/con.c:302-330 (unblock disables covering
  fullscreen, then activates) @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-i3-max` i3:parser-specs/commands.spec:226-262 (fullscreen/sticky/
  floating verbs, no maximize verb) and src/con.c:428-485 (maximized derived
  from layout) and src/x.c:831-866 (hints written to the client only)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-i3-wsmode` i3:src/commands.c:1142-1168 (float is per-window) and
  src/workspace.c:996-1020 (`workspace_attach_to`: layout only
  default/stacked/tabbed) and parser-specs/config.spec:157-160
  (`workspace_layout` values, no tiling on/off)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-i3-close` i3:src/tree.c:263-311 (close detaches with percent fix; focus
  next only if the closed con was focused) and src/con.c:1651-1685
  (`con_next_focused`: non-head keeps head, head takes next sibling else
  parent; floating maps to its wrapper)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-i3-wsretain` i3:src/workspace.c:530-533 (old workspace closes only
  when empty and invisible; shown workspaces are retained)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (visible-empty retention; hidden-empty cleanup is the disclosed
  counterpart)
- `S-i3-grp` i3:src/con.c:1997-2006 (`con_set_layout` retargets non-workspace
  cons to the parent split) and :2109-2196 (`con_toggle_layout`
  stacked/tabbed/split/all) and :1620-1630 (tabbed HORIZ, stacked VERT) and
  src/tree.c:503-577,593-628 (`tree_next` walks matching-orientation parents;
  tabbed left/right and stacked up/down step tabs, wrap per `focus_wrapping`)
  and src/click.c:232-247,263-273 (stacked/tabbed decoration scroll steps tabs;
  decoration tab click switches to that tab)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (no single toggle-stack verb; `layout tabbed`/`stacked` converts the parent)
- `S-i3-outmove` i3:src/move.c:206-253 (`move_to_output_directed`: closest output
  in direction, visible workspace, `attach_to_workspace`, mover-focused follow via
  `workspace_show`) and :179-199 (`attach_to_workspace`: RIGHT/DOWN to HEAD,
  else TAIL; focus TAIL; direct workspace insert with no `workspace_attach_to`
  wrapper on this path) and :259-282,342-347
  (lone/single-child workspace falls back to output-directed; workspace-level
  no-swap falls back to output-directed)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-i3-tdrag` i3:src/click.c:250-259 (floating-modifier+left starts `tiling_drag`
  before focus) and :332-340 (titlebar/modifier-or-titlebar `tiling_drag` after
  focus) and :284-285,322-324 (floating-modifier/titlebar producers for
  `floating_drag_window`, not tiled drag) and src/config.c:231-232 (code default
  `tiling_drag` modifier-only, swap Shift) and etc/config:50,54 (shipped
  `floating_modifier Mod1`, `tiling_drag modifier titlebar`) and
  src/config_directives.c:366-372,711-723 (both directives) and
  src/drag.c:43-48,156-167 (15px threshold gate; titlebar path thresholded,
  modifier path immediate) and :88-91 (key press reverts) and
  src/tiling_drag.c:304-311,320-331,402-425 (no focus change mid-drag; REVERT /
  NULL target / self-centre aborts with indicator destroy; focus/fullscreen
  restore at end)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-i3-tdrop` i3:src/tiling_drag.c:13-35,42-67 (tiling-only drop targets exclude
  floating/hidden/internal/fullscreen-covered; drag needs >1 target) and :73-94
  (`find_drop_target` rect hit else visible-workspace fallback, NULL off-output)
  and :158-244 (nearest-edge direction; outer thin band DT_PARENT, 30% edge band
  DT_SIBLING, remainder DT_CENTER; self-centre draws nothing) and :333-396
  (CENTER: swap-modifier `con_swap` else `con_move_to_target`; SIBLING: split if
  parent orientation differs then `insert_con_into`; PARENT: edge-of-tabbed/stack
  retarget then `tree_move` with old-focus restore; orientation from direction)
  and src/con.c:1498-1532 (`con_move_to_target` split-target descends to focus)
  and src/con.c:2580-2659 (`con_swap` leaf swap)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-i3-ffm` i3:src/config_directives.c:447-448 (`focus_follows_mouse`
  sets `disable_focus_follows_mouse`) and src/handlers.c:95,175,219
  (enter-notify focus gated on that flag)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (enter-notify focus gated on that flag, enabled at shipped
  zero-init default; plain click is `S(S-i3-click)`)
- `S-i3-click` i3:src/click.c:205-277 (any workspace click resolves
  the workspace plus floating con, then `con_activate` focuses the
  clicked con or its focused descendant)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (plain click focuses B)
- `S-i3-border` i3:src/click.c:24-71 (`tiling_resize_for_border` pair
  search plus directional dispatch) and :85-105 (border button paths) +
  src/resize.c:127-169 (`percent_for_1px` minimum plus pair percent
  share moves)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (border drag moves percent shares with a 1px clamp)
- `S-i3-wiz` i3:i3-config-wizard/main.c:827-832 (existing-config exits,
  no overwrite) @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (first-run writer guard; no Compatible preset/stale-choice model in the
  inspected inventory)
- `S-i3-tray` i3:parser-specs/config.spec:553-554,640-652
  (`tray_output`/`tray_padding` bar options) + i3bar/src/xcb.c:46-48
  (selection window for tray support) + i3bar/include/trayclients.h (client
  list) @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (bar-hosted XEMBED tray; no owner notification-icon lifecycle in the
  inspected inventory)
- `S-i3-bind` i3:parser-specs/config.spec:437-464 (`bindsym`/`bindcode` to
  `cfg_binding`) + src/bindings.c:59-111 (`configure_binding`)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (binding definition only; no staging/Compatible/Apply/Force/Disable/Revert
  or preimage model in the inspected command/config inventory)
- `S-i3-switcher` i3:etc/config:62-68 (`dmenu_run` launcher plus a
  commented-out `rofi` alternate, both external) +
  parser-specs/commands.spec:189-213 (`FOCUS` inventory: directional,
  output, tiling/floating/mode_toggle, parent/child, workspace; no listing
  verb) + workspace number/move binds per `S(S-i3-wskeys)`
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (no native Alt+Tab/listing verb; launchers are external; focus verbs are
  single-targets and workspace number/next/prev switch views without listing)
- `S-xmo-core-nav` xmonad:src/XMonad/Config.hs:185-215
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (default core navigation is stack focus/swap with no directional core verb; directional scenarios in this profile use contrib Navigation2D, see S-xmo-nav)
- `S-xmo-layout` xmonad:src/XMonad/Config.hs:137-149 (layout
  `Tall ||| Mirror Tall ||| Full`, `nmaster=1`, `ratio=1/2`, `delta=3/100`) and
  src/XMonad/Layout.hs:50-70,95-98 (`Full` focused-fullscreen, `Tall` master/stack
  `tile`/`splitVertically`) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (flat two-pane tiling; no N-ary H/V tree, no tab stacks, no maximize state)
- `S-xmo-float` xmonad:src/XMonad/StackSet.hs:527-532 (`float`/`sink`
  floating map only, stack retained) and src/XMonad/Operations.hs:93-99
  (`isFixedSizeOrTransient`: `isJust` whole-pair `sh_min_size==sh_max_size`
  equality, no zero/sentinel guard) and :107-119
  (`manage` fixed-size/transient float via `insertUp`+`float`, else `insertUp`)
  and :719-753 (`floatLocation`: managed native geometry plus size hints;
  error fallback is full-screen `RationalRect 0 0 1 1`; unmanaged centering
  is admission-only) and :774-781 (`float` recomputes from current geometry,
  focus retained)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
- `S-xmo-restart` xmonad:src/XMonad/Operations.hs:646-712 (`StateFile` carries
  the whole `StackSet` including the floating map; `writeStateToFile`/
  `readStateFile` resume-only: file removed after reading; `restart prog True` resumes with the current window state)
  + src/XMonad/Main.hs:220-248 (startup: serialized state resumed only from an
  existing file, else `initialWinset`; live top-level scan drops gone windows
  via `W.delete` and freshly admits new ones via `manage`)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (owner restart preserves floats as ordinary floats; no sticky concept;
  layout ratio round-trips: src/XMonad/Layout.hs:56-63 `Tall`
  `tallNMaster`/`tallRatioIncrement`/`tallRatio` deriving `Show, Read`,
  serialized with the windowset)
- `S-xmo-nav` xmonad-contrib:XMonad/Actions/Navigation2D.hs:462-473
  (`withNavigation2DConfig` + `def`: tiled hybrid line/side, float center,
  screen line, no custom layout) and :493-512 (`windowGo` focus-target,
  `windowSwap` same-layer swap retaining mover focus via `swap` :856-898; miss is no-op)
  and :467-473 (`def` strategy defaults: `defaultTiledNavigation`
  `hybridOf lineNavigation sideNavigation`, `floatNavigation`
  `centerNavigation`, `screenNavigation` `lineNavigation`,
  `layoutNavigation`/`unmappedWindowRect` empty) and :570-600 (`actOnLayer`
  `thisLayer` same-layer operation across all visible screens; `navigableWindows`
  covers every visible screen via `sortedScreens` :940-954, partitions
  floating/tiled by the `floating` map, unmapped windows skipped) and
  :663-711 (`doTiledNavigation`/`doFloatNavigation`/`doScreenNavigation`
  via `runNav`; miss returns the input unchanged, i.e. no-op) and :713-751,
  :752-816, :818-855 (tiled hybrid line/side plus float-center/screen-line
  algorithms: directional edge overlap plus distance, stack-order tie preference) and :523-534
  (`windowToScreen` moves via `W.shift`, `screenGo` focuses via `W.view`;
  separate verbs from `windowSwap`)
  @5097a457e7a409bc9a7584dc5aa82b34c69d6dda
  (tiled/float separate layers across all visible screens via `thisLayer`; profile wrap False;
  `windowSwap` empty-workspace branch is no-op with no screen fallback, unlike `windowGo`)
- `S-xmo-close` xmonad:src/XMonad/StackSet.hs:336-339 (`filter`
  focus moves down else up, order preserved) and :511-532 (`delete` is
  `sink` plus `delete'`; `float`/`sink` are floating-map writes only)
  and src/XMonad/Operations.hs:129-152 (`unmanage` via `W.delete`;
  `killWindow`/`kill`) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (close removes from the stack with positional down-else-up refocus, no
  MRU rule; Tall reflows unconditionally via recalc)
- `S-xmo-topfocus` xmonad:src/XMonad/Operations.hs:217 (`windows` refresh ends in `setTopFocus`) and :391-392 (`setTopFocus`: X focus to `peek`, else root) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (every refresh actuates StackSet focus; an emptied stack focuses root, i.e. focus none)
- `S-xmo-arrange` xmonad:src/XMonad/Operations.hs:183-221 (Tall arrange input filters out floating-map members; floats restacked first-on-top via `restackWindows`; allocations applied via `tileWindow`; visibility via `reveal`/`hide`; refresh ends in `setTopFocus`) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (floats never consume tile shares; floats render above tiles; allocation writes are unconditional `moveResizeWindow`, native ack untraced)
- `S-xmo-ws` xmonad:src/XMonad/StackSet.hs:134-164 (workspace zipper:
  current/visible/hidden lists; `Workspace` is tag/layout/`Maybe` stack,
  so closing the last window empties the stack without removing the
  workspace) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (static-workspace retention; empty-stack X focus per `S(S-xmo-topfocus)`)
- `S-sway-wsretain` sway:sway/tree/workspace.c:314-331
  (`workspace_consider_destroy` spares output-active and seat-focused
  workspaces; other empties are destroyed)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-nir-close` niri:src/layout/scrolling.rs:1062-1074 (`remove_tile`
  drops a sole-tile column whole) and :1074-1160
  (`remove_tile_by_idx` active-index fixup to next else previous) and
  :1192-1276 (`remove_column_by_idx` activates the clamped next column)
  and :826-831 (activating a different column resets
  `activate_prev_column_on_removal`) and :1052-1058 (`add_column`
  records the pre-add view offset for previous-column restore) and src/layout/workspace.rs:783-797 (floating vs scrolling dispatch
  plus focus-flag update) and src/layout/floating.rs:515-552 (float
  removal, active falls to topmost) and src/layout/monitor.rs:650-670
  (cleanup spares the active workspace)
  @ed22699d99462f61ab171472d3ea67e844ea580d
- `S-pap-close` PaperWM:tiling.js:981-1010 (`removeWindow` neighbor
  selection plus empty-column splice) and :1046-1056
  (`removeFloating` splice) and :3950-3966 (`remove_handler`
  shell-focus note plus space removal) and :714-751
  (layout reads each column's live or saved tiled width independently)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (selection and removal legs; settled widths and shell focus fallback
  stay TBD where stated)
- `S-kar-close` karousel:src/lib/layout/Column.ts:297-325
  (`onWindowRemoved` above-else-below focus plus last-window destroy)
  and src/lib/layout/Grid.ts:161-185 (`onColumnRemoved` left-else-right
  focus, null on the last column)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
- `S-kar-manual-width` karousel:src/lib/world/clientState/Tiled.ts:88-101,144-152
  (host interactive resize feeds width delta to the column) and :162-171
  (non-interactive external geometry re-asserts via rate-limited
  `onFrameGeometryChanged`) and
  src/lib/layout/Column.ts:101-114,143-162 (arbitrary width clamped to
  size hints, stored as preferred width; optional neighbor redistribution)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (manual widths are not restricted to keyboard presets)
- `S-pan-close` paneru:src/ecs/triggers.rs:912-975
  (`window_destroyed_trigger` focus give-away plus despawn) and
  :1018-1062 (`give_away_focus` nearest-center plus tabbed branches) and
  :1400-1418 (`window_removal_trigger` strip removal) and
  src/ecs/workspace.rs:505-525 (row-0 orphan spare)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (removal and nearest-center policy; exact focus target stays TBD
  where stated)
- `S-ours-close` omnitiler:crates/tiler-core/src/session.rs:2163-2230
  (`remove_leaf_from_tree`/`remove_node` collapse with proportional
  shares) and crates/tiler-core/src/session/ops/lifecycle.rs:450-485
  (`propose_remove` focus-stack fallback, unfocused removal preserves
  focus) and crates/tiler-core/src/session/world.rs:586-605
  (`focus_stack_fallback` MRU) and crates/tiler-core/src/session/world.rs:750-766
  (`converge_observation` drops host-closed windows via the same
  collapse) and
  crates/tiler-core/src/cosmic_v1.rs:175-190
  (`proportional_removal_shares`) @9241c94
  (Engine desired topology/focus plus KDE world-route applied-scope delivery;
  world-route scope-retire proof and native focus confirmation stay TBD
  sub-legs)
- `S-close-verbs` shared close-verb inventory for R-CLOSE-03/04/05:
  COSMIC data/keybindings.ron:6-7 (`Close` Super+q/Alt+F4) +
  src/input/actions.rs:180 @3d55cba06c9cf6f27609cdefb520f7857dba20af;
  Hyprland src/config/shared/actions/ConfigActions.hpp:40 +
  ConfigActions.cpp:213 (`killWindow`)
  @19fb395d45314960e6f79f17994a84094f1cd4f6;
  bspwm doc/bspwm.1.asciidoc:475-476 (`node -c|--close`)
  @e11eff4cb3333216ad03c815609a4ed79e08929c;
  i3 src/commands.c:1213 + parser-specs/commands.spec:219-224 (`kill`)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d;
  xmonad src/XMonad/Config.hs:191 (`kill`)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7;
  sway sway/commands/kill.c:15 (`kill`)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104;
  qtile libqtile/resources/default_config.py:48 (`lazy.window.kill()`)
  @83c697a5621306c3586efca31867efcfa0482e2d;
  awesome awesomerc.lua:424 (`c:kill()`)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f;
  niri niri-ipc/src/lib.rs:290 (`CloseWindow`)
  @ed22699d99462f61ab171472d3ea67e844ea580d;
  PaperWM schemas/org.gnome.shell.extensions.paperwm.gschema.xml:448
  (`close-window`) @8bf6dd264f60d6c0c402b63df7b424b888959a48;
  karousel src/lib/keyBindings/Actions.ts (no close verb in the
  inspected inventory; host KWin close drives `onWindowRemoved`)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b;
  paneru src/types/commands.rs (no close `Operation` in the inspected
  inventory; host macOS close observed via AX destroy)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269;
  Ours crates/tiler-core/src/session/ops/lifecycle.rs:65
  (`SessionCommand::Remove` dispatch) and :570 (shape gate)
  @9241c94 (native close is the host action, KWin user close on KDE and
  WM_CLOSE/app close on Windows, converged by the Engine via observation
  per `S(S-ours-close)`; `Remove` is the observation-driven proposal,
  never the native verb)
- `S-xmo-mouse` xmonad:src/XMonad/Operations.hs:787-841
  (`mouseDragCursor` grab with release `done`; `mouseMoveWindow` writes the
  raw frame plus `float` on motion and `float` on release with no clamp/zone
  check; `mouseResizeWindow` resizes via `applySizeHintsContents` plus `float`
  on motion/release; no key-cancel branch) and src/XMonad/Config.hs:246-256
  (`mod-button1` focus + move + `shiftMaster`; `mod-button3` focus +
  resize + `shiftMaster`) and src/XMonad/Main.hs:330-344 (button-release
  ends dragging via `done`; motion routes to the drag closure; no key path)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (any release floats, even zero-move; raw frames retained off-workarea;
  hints can shape extents; no zones, preview, or restore)
- `S-xmo-ffm` xmonad:src/XMonad/Config.hs:173-174
  (`focusFollowsMouse = True` default) and :177-178
  (`clickJustFocuses = True` default) and src/XMonad/Core.hs:133
  (entry events may change focus)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (shipped hover-focus plus click-focuses policies)
- `S-xmo-out` xmonad-contrib:XMonad/Actions/Navigation2D.hs:511-534
  (`windowSwap` same-layer stack-position swap via `swap` :856-898
  retaining mover focus, cross-screen when the directional candidate is on
  another visible screen; `windowToScreen` moves via `W.shift`; `screenGo`
  focuses via `W.view`; `windowSwap` empty-workspace branch is no-op with no
  screen fallback)
  @5097a457e7a409bc9a7584dc5aa82b34c69d6dda
  (profile directional move is `windowSwap` within the same layer across all
  visible screens; `windowToScreen` is the separate carry verb, not exercised here)
- `S-xmo-ctl` xmonad:src/XMonad/Config.hs:188-227 (key inventory:
  spawn/kill/NextLayout/refresh/focus/swap/shrink/expand/sink/IncMasterN/
  quit/restart only; no first-run/preset/prompt/tray/staging/Force/Disable/
  Revert/preimage verbs) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (owner-specific control outcomes have no counterpart here)
- `S-xmo-switcher` xmonad:src/XMonad/Config.hs:189 (`dmenu_run`/`gmrun`
  external launchers) and :199-205 (mod+Tab `focusDown` / mod+Shift+Tab
  `focusUp`: same-stack cycle, not a listing) and :230-235 (mod-[1..9]
  `greedyView`, mod-shift-[1..9] `shift`; view/switch verbs without
  listing) + xmonad-contrib per `S(S-xmo-nav)` (Navigation2D
  `windowGo`/`windowSwap` stay on the same layer)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (no native Alt+Tab/cross-workspace listing verb in this profile)
- `S-xmo-ewmh` xmonad-contrib:XMonad/Hooks/EwmhDesktops.hs:107-112,664-680
  (`ewmh`, `ewmhFullscreen`, `fullscreenEventHook` ClientMessage add/remove/toggle)
  and :682-709 (add/remove/toggle dispatch via `fullscreenHooks` with `_NET_WM_STATE`
  property converged via `chWstate`)
  and :143 (`fullscreenHooks` defaults) + XMonad/Hooks/ManageHelpers.hs:289-290,329-330
  (`doFullFloat` fullscreen float `RationalRect 0 0 1 1`, `doSink`)
  @5097a457e7a409bc9a7584dc5aa82b34c69d6dda
  (event-path fullscreen only; `ewmh` alone excludes fullscreen handling)
- `S-ours-flt-target` omnitiler:crates/tiler-core/src/session/world.rs:769-835
  and crates/tiler-core/src/directional.rs:1108-1135
  @2bdd944536fa2608f60b68686f8ec57d61663726
  (floating windows hold exceptions, not tile leaves; directional focus
  selects only tree siblings/descendants)
- `S-ours-flt-subject` omnitiler:kwin/src/world-entry.ts (directional
  commands for float/sticky subjects drop with `command-dropped ...-float`;
  float subjects take the float-subject path instead) and
  crates/tiler-windows/src/tiling_sys.rs:5904-5954
  @2bdd944536fa2608f60b68686f8ec57d61663726
  (KDE drops floating/sticky subjects from tile dispatch; Windows refuses each explicitly)
- `S-cos-newgroup` cosmic-comp:src/shell/layout/tiling/mod.rs:2898-2937
  @3d55cba0
  (new_group moves the split leaf first, then appends the newcomer, and
  keeps the old position; child order is insertion order)
- `S-cos-sizes` cosmic-comp:src/shell/layout/tiling/mod.rs:177-190
  (`Data::new_group` halves the placeholder geo) and :219-244
  (`add_window` proportional rescale plus leftover insert) and :246-252
  (`swap_windows` exchanges sizes with windows) and :255-290
  (`remove_window` proportional redistribution plus overflow-to-last)
  @3d55cba0
  (group share arithmetic; pixels need the output width)
- `S-cos-mapfocus` cosmic-comp:src/shell/mod.rs:2970-2984
  (new window joins the focused stack), :3001-3014 (fresh tiling map at
  target MRU, no restore state), :3028-3041 (newcomer is the focus target
  on the active workspace) and src/shell/element/stack.rs:200-234
  (join with no index appends the tab and makes it active) @3d55cba0
- `S-cos-send` cosmic-comp:src/shell/mod.rs:3541-3622
  (follow activates the target and focuses the mover; no-follow returns no
  target; floating workspaces map arrivals to the floating layer while
  tiled workspaces fresh-map them) and src/input/actions.rs:292-345
  (`SendToWorkspace` is no-follow, `MoveToWorkspace` follows;
  `SendToLastWorkspace`/`MoveToLastWorkspace` target `len-1`) and
  src/shell/mod.rs:652-704 (trailing empty workspace ensured; non-active
  non-last empties removed) and
  src/shell/layout/floating/mod.rs:474-476 (arrival with no position
  reuses last geometry loc, else cascade) @3d55cba0
- `S-cos-sendoverlay` cosmic-comp:src/shell/mod.rs:3302-3310
  (`move_current`: Element focus routes to `move_element`, Fullscreen focus
  to `move_window`) and :3337-3360 (`move_window` takes the fullscreen
  surface with its restore and drops tiling slot state) and :3495-3502
  (fullscreen carried to the target via `map_fullscreen`) and
  src/shell/workspace.rs:648-653 (`unmap_element` unmaximizes first, so a
  maximized mover travels with `was_maximized` in restore data) and
  src/shell/mod.rs:3587-3603 (tiled target unmaximizes other maxima then
  fresh-maps the mover with no mover re-maximize; tiling-disabled target
  maps floating and re-maximizes when `was_maximized`)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (workspace-send overlay carry: fullscreen travels, tiling-target
  maximize does not)
- `S-out07-cos-dirwrap` cosmic-comp:src/shell/layout/tiling/mod.rs:563-585
  (output-send direction branch: wraps the target root with the newcomer
  in a Vertical group for Left/Right, newcomer at index 0 for Right/Down
  else 1; the MRU node is ignored on this path)
  @3d55cba0
  (directional output admission ignores the remembered leaf; ordinary
  MRU split is the direction-less branch per `S(S-cos-axis)`)
- `S-cos-sysact` cosmic-comp:src/input/actions.rs:1034-1039
  @3d55cba0
  (`System` shortcut actions, which include the Alt+Tab window switcher,
  spawn a configured external command; listing/activation policy is not in
  the pinned compositor source)
- `S-cos-focusfix` cosmic-comp:src/shell/focus/mod.rs:126-147
  (focus-stack iteration is MRU-first), :553-616 (dead/missing focus runs
  fixup), :684-790 (validity plus MRU-last target, else first mapped) and
  src/shell/workspace.rs:655-688 (send/close drops the window from focus
  sets), :1491-1504 (float toggle leaves the tiling tree but keeps the
  focus entry) @3d55cba0
- `S-cos-flttoggle` cosmic-comp:src/shell/workspace.rs:1491-1506
  (toggle unmaximizes first, then swaps tiling<->floating; unfloat
  fresh-maps at focus MRU, no old-slot restore) and :1508-1519 (focused
  variant refuses while fullscreen is focused) and
  src/shell/layout/floating/mod.rs:341-350,474-476 (float frame reuses
  last geometry, else cascade/center) @3d55cba0
- `S-cos-wstile` cosmic-comp:src/shell/workspace.rs:1433-1489
  (`set_tiling`: disable moves every tiled window to floating, enable
  fresh-maps every floater sequentially at focus MRU with the pre-enable
  focus stack held across the loop; maximized windows unmaximized first,
  then re-overlaid with retargeted layer) and
  src/shell/layout/floating/mod.rs:1325-1327 (`mapped` iterates
  `space.elements().rev()`, z-order front-to-back) and `S(S-cos-raise)`
  (fixture focus order raises D last, so D is topmost regardless of map
  order) @3d55cba0
- `S-cos-raise` cosmic-comp:src/shell/focus/mod.rs:198-224 (`set_focus`
  appends the target to the focus stack) and :288-345 (`update_active`
  collects the focused windows, then raises focused sticky and ordinary
  floaters) and :479-481 (`raise_with_children` calls
  `space.raise_element(focused, true)`) and src/input/mod.rs:2883-2889
  (sticky hit-tested before workspace windows on the focus path) and
  src/input/actions.rs:978-987 (`ToggleStacking` calls
  `toggle_stacking_focused`, a
  tile/stack convert per `S(S-cos-stack)`, not a z-order lower) and cosmic-comp:data/keybindings.ron:83-92
  (stacking/float/maximize/fullscreen/orientation bindings only, no lower
  binding) + cosmic-settings-daemon:config/src/shortcuts/action.rs:8-151
  (pinned `Action` inventory: Move/SwapWindow/Fullscreen/Maximize plus
  `ToggleStacking` stack-convert only; no raise/lower verb) and
  src/wayland/handlers/xdg_shell/mod.rs:265-304 (client unmaximize/fullscreen
  requests only, no lower request path)
  @3d55cba0 for cosmic-comp,
  @e37160f14d1e7ee428f973cd2848b4e95f83dfe1 for the daemon path
  (focus raises; explicit native lower has no producer)
- `S-cos-restore` cosmic-comp:src/shell/layout/tiling/mod.rs:1309-1340
  (`unmap` saves `RestoreTilingState`: parent/sibling/orientation/idx/sizes)
  and :438-540 (`remap` restores the old slot from that state, sibling
  path included) @3d55cba0
- `S-cos-maxtoggle` cosmic-comp:src/shell/mod.rs:4353-4367
  (`maximize_toggle`: maximized unmaximizes, fullscreen is a no-op,
  otherwise a new `maximize_request`) @3d55cba0
- `S-cos-maxmove` cosmic-comp:src/shell/mod.rs:4140-4143 (tiled maximized
  focus returns `FocusResult::None`: no traversal, overlay fences) and
  :4239-4253 (move unmaximizes tiling-origin maxima first, then dispatches
  the mover through floating-then-tiling `move_current_element`)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (focus/move while maximized: fence plus unmaximize-first dispatch)
- `S-cos-fsreq` cosmic-comp:src/shell/workspace.rs:1255-1282
  (`map_fullscreen` at output geometry with focus append) and
  src/shell/mod.rs:4891-5021 (`fullscreen_request` sticky/tiling/floating
  branches capture restore state; focus target is Fullscreen) @3d55cba0
- `S-cos-fsrestore` cosmic-comp:src/shell/mod.rs:5023-5048
  (`unfullscreen_request` remaps from saved state, returns the restored
  window as focus) and :2755-2863 (restore branches: floating geometry
  restore, tiling old-slot remap, maximized re-overlay) @3d55cba0
- `S-cos-sticky` cosmic-comp:src/shell/mod.rs:4769-4800 (to-sticky:
  focus-stack cleanup, tiled subjects float first, map to the output-set
  sticky layer) and :4827-4871 (un-sticky restores the remembered
  Tiling/Floating layer at the active workspace, appends focus) @3d55cba0
- `S-cos-persist` cosmic-comp:src/shell/mod.rs:853-864 (only
  pinned-workspace config carried into a new session) and :1511-1524
  (`persist` writes pinned workspaces only) and :867-935 (`add_output`
  restore: backup-set restore else fresh set, pinned shells recreated via
  `create_workspace_from_pinned`, `prefers_output` reclaim) and
  src/shell/workspace.rs:455-471 (`to_pinned` carries output
  match/tiling flag/id/name only, no window/sticky/float state) and
  src/input/actions.rs:162-166 (`Action::Terminate` stops to the login
  manager, no re-exec verb) + cosmic-settings-daemon:config/src/shortcuts/action.rs:8-151
  (`Action` inventory carries `Terminate` with no restart/re-exec verb)
  @3d55cba0 for cosmic-comp,
  @e37160f14d1e7ee428f973cd2848b4e95f83dfe1 for the daemon path
- `S-cos-native-unmax` cosmic-comp:src/wayland/handlers/xdg_shell/mod.rs:265-276
  (Wayland client unmaximize routes to `shell.unmaximize_request`, or clears
  the pending flag) and src/xwayland.rs:1135-1146 (X11 same) and
  src/wayland/handlers/toplevel_management.rs:218-223 (external client same)
  and src/shell/mod.rs:4502-4544 (sticky/workspace dispatch; state taken,
  original geometry/layer restored) @3d55cba0
- `S-cos-fsact` cosmic-comp:src/input/actions.rs:927-953 (project Fullscreen
  toggle dispatches on focus kind: Element enters, Fullscreen exits, focus
  set to result) and src/wayland/handlers/xdg_shell/mod.rs:278-304 (client
  fullscreen routes to the same shell request plus focus, pending fallback)
  and src/xwayland.rs:1159-1181 (X11 same) @3d55cba0
- `S-cos-admit` cosmic-comp:src/shell/mod.rs:2886-2913 (admission consumes
  pending protocol flags; fullscreen iff flag present) and :2960-2999
  (flagged fullscreen maps fullscreen; dialog/exception/tiling-disabled maps
  floating; otherwise tiling) and src/xwayland.rs:812-823 (X11 flags from
  protocol state, including `is_fullscreen`) and
  src/shell/layout/mod.rs:17-44 (dialog is parent/window-type checks) @3d55cba0
- `S-cos-floatpos` cosmic-comp:src/shell/layout/floating/mod.rs:474-476
  (arrival with no position reuses last geometry loc, else cascade) and
  :486-604 (cascade from spawn_order with down/side offsets, new-column
  fallback, centered fallback; no parent-relative branch) @3d55cba0
- `S-cos-modal` cosmic-comp:src/shell/layout/mod.rs:17-44 (modal only feeds
  the X11 is_dialog branch; Wayland is parent-only with no modal branch) and
  src/shell/focus/mod.rs:684-737 (focus validity covers sticky/focus-stack/
  mapped/fullscreen only, no modal fence branch) @3d55cba0
- `S-cos-maxpolicy` cosmic-comp:src/shell/mod.rs:4461-4500 (`maximize_request`
  records original geometry+layer and overlays the work area; no-op if already
  maximized) and :4502-4544 (`unmaximize_request` dispatches sticky/workspace,
  restores original geometry/layer, re-applies snap) @3d55cba0
- `S-nir-base` niri:resources/default-config.kdl:123
  (`center-focused-column "never"`) and :129-132 (default presets 1/3, 1/2,
  2/3 of output) and :142 (default column width proportion 0.5)
  @ed22699d99462f61ab171472d3ea67e844ea580d
- `S-pap-base` PaperWM:schemas/org.gnome.shell.extensions.paperwm.gschema.xml:663-665
  (`default-focus-mode` 0 DEFAULT) and :668-670 (`open-window-position` 0
  RIGHT of current window) @8bf6dd264f60d6c0c402b63df7b424b888959a48
- `S-kar-base` karousel:src/lib/config/definition.ts:108-111 (`presetWidths`
  "50%, 100%") and :133-136 (`stackColumnsByDefault` false) and :153-166
  (`scrollingLazy` true, `scrollingCentered`/`scrollingGrouped` false)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
- `S-pan-base` paneru:src/config.rs:1282-1284 (default width presets
  0.25-2.0) and :799-808 (focus-follows-mouse and mouse-follows-focus
  enabled) and :822-828 (native tabs enabled) and :856-862 (one workspace,
  append admission) @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
- `S-ours-ins` omnitiler:crates/tiler-core/src/session.rs:2124-2161
  (`insert_tiled`: empty tree returns the single new leaf; focused leaf
  wraps old/new in an ordered binary group, no same-axis N-ary append) and
  crates/tiler-core/src/cosmic_v1.rs:63-69 (`admission_axis`: wide selects
  Horizontal, else Vertical) and
  crates/tiler-core/src/session/ops/lifecycle.rs:237-261 (normal tiled
  admission dispatches through `admission_axis_for_rect` plus eligible
  focus into `insert_tiled`) and
  crates/tiler-windows/src/product_hide.rs:825
  (`admit_managed_claim` managed-claim gate) @9241c94
  (shared Engine plus adapter integration; newcomer focus, order, and exact
  frames are separate TBD sub-legs, never read from this tag)
- `S-ours-admit` omnitiler:crates/tiler-core/src/session/world.rs:529-545
  (`eligible_focus_in`: anchor is the focused leaf only when the focused
  domain is the target domain and the leaf is still a linked tile leaf) and
  crates/tiler-core/src/session/ops/lifecycle.rs:249 (normal admission
  resolves that eligible focus) and :331-349 (admitted newcomer becomes
  the desired focus leaf with `last_active` updated, then dispatches) and
  crates/tiler-windows/src/tiling_sys.rs:4,21 (Windows retains the shared
  `tiler_core::engine::Engine` as layout) @9241c94
  (Engine desired-focus plus adapters' admit application; adapter-side
  physical focus confirmation and exact frames stay TBD sub-legs).
  D05 float-focus leg: the retained Engine convergence route anchors at the
  prior tiled focus (a focused ordinary float holds an exception, never a
  tile leaf; same B via the evolving tiled focus), proven offline by
  `crates/tiler-core/tests/session_float_focus_admission.rs` (convergence
  fixture); the legacy KWin wire characterization was removed with the legacy
  route, native observation/activation journey pending (live TBD, user-owned).
- `S-ours-admission-capacity` omnitiler:crates/tiler-core/src/seed.rs:360-409
  (`order_spatial_with_focus_last`: deterministic spatial `(y,x,h,w)` stable
  sort with the focused window last; ties preserve adapter observation order;
  tied non-focused rects fail closed on non-admission rebuilds) and :91-100,103-358
  (`try_recursive_cut_fit[_with_centre_count]`: geometry-ordered
  recursive-cut fit first, declining to the sequential seed) and :531-603
  (`seed_admit_step`: one admit per revision through the retained
  `SessionCommand::Admit` lifecycle) and :614-657 (`seed_session`: spatial
  order in with focus last, one revision per member) and
  crates/tiler-core/src/engine.rs:1449-1510 (fit-before-seed: centre-split or
  minimum-infeasible fit declines to the sequential long-edge seed; the fitted
  path commits via `propose_fitted_admit`) and :1768-1809 (ordinary admit
  rebuilds in seed order, then one `Admit` propose) and
  crates/tiler-core/src/session/fit.rs:30-179 (`propose_fitted_admit` stages
  the complete tree through the normal lifecycle; projection failure or empty
  geometry refuses `MalformedTopology` with no mutation) and
  crates/tiler-core/src/session/ops/lifecycle.rs:415-425 (normal `Admit`:
  projection error or any leaf rect `w<=0||h<=0` refuses
  `MalformedTopology`; `PendingDesired` never staged) and
  crates/tiler-core/src/session.rs:3022-3068 (`project_output_geometry`
  validation) and crates/tiler-core/src/geometry.rs:154-249 (`project` gates:
  positive bounds/shares, `avail >= shares.len()`, gap budget; negative
  segment errors) @15faf1e46ba82ac462fa7b8deefa33e650825fca
  (insert proposal structure versus projection refusal; the 13/14-window
  report is user-supplied, not verified legacy behavior; no universal cap)
- `S-nir-ins` niri:src/layout/scrolling.rs:903-923 (`add_tile` always wraps
  the tile in a new column) and :999-1017 (`add_column`: index defaults to
  active+1, 0 on an empty strip; the new column activates when told to) and
  src/layout/workspace.rs:636-676 (Auto target: no focus steal from an
  active pending fullscreen; pending maximized/fullscreen tiles open in
  the scrolling layout; plain floats go to the floating layer) and
  :820-831 (`resolve_default_width`: no rule falls back to the configured
  default column width) and
  src/handlers/xdg_shell.rs:1107-1116 (`open_on_workspace` rule routes the
  target monitor)   @ed22699d99462f61ab171472d3ea67e844ea580d
  (position, default-width, routing, Smart-activation and focus-scroll
  policy legs)
- `S-nir-fltanchor` niri:src/layout/workspace.rs:1868-1878 (`activate_window`
  float-vs-scrolling dispatch: float focus sets the floating-active flag
  without touching the scrolling active column) + src/handlers/compositor.rs:150-176
  (map computes the newcomer's own floating state and defaults to `Smart`
  activation) + src/layout/mod.rs:509-519 (`ActivateWindow`, `Smart` default)
  + src/niri.rs:1292-1300 (keyboard focus derives from the layout active
  window) @ed22699d99462f61ab171472d3ea67e844ea580d
  (float-focus anchor and newcomer-focus legs; column position stays
  `S(S-nir-ins)`)
- `S-nir-wsopen` niri:src/handlers/xdg_shell.rs:1105-1123 (rule resolves
  the target monitor) and :1125-1157 (`open_on_output`/fullscreen/
  parent/active-monitor precedence; no-rule default is the active
  monitor) and :1167-1174 (rule resolves the named workspace,
  else the active one) and src/handlers/compositor.rs:152-221 (activation
  decision plus `AddWindowTarget::Workspace` map path) and
  src/layout/monitor.rs:494-520,577-620 (workspace-target resolve; `Smart`
  never activates the workspace) and src/layout/mod.rs:1002-1062
  (workspace-target dispatch; `Smart` never switches monitors) and
  src/tests/window_opening.rs:164-215 (open-on-workspace snapshot powerset)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (default plus inactive-target routing plus no-switch legs;
  column-position leg is `S(S-nir-ins)`)
- `S-pap-ins` PaperWM:tiling.js:3994-4008 (fresh windows redirect to the
  selected space) and :4048-4055 + :4105-4120 (winprop `spaceIndex` moves
  the window to that space and re-inserts it there) and :4155 (`addWindow`
  at `getOpenWindowPositionIndex`) and :4262-4280 (index: selected+1 under
  the shipped RIGHT default) and :4071-4086 (fullscreen newcomers insert
  normally, then re-fullscreen after a timeout) and :4157-4161 (maximized
  newcomers unmaximize, then maximize horizontally) and :4204-4224 (fresh
  windows activate on actor show) and :4241-4247 (inserts landing on an
  inactive space only ensure the viewport, never steal focus)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (position, routing, newcomer-state, and focus legs; settled widths
  per `S(S-pap-layout)`, viewport per `S(S-pap-view)`)
- `S-pap-fltanchor` PaperWM:tiling.js:879-881 (`getWindows` reduces the column
  strips only) + :1032-1044 (`isFloating`/`addFloating` hold floats in a
  separate `_floating` list) + :4676-4679 (`focus_handler` returns before any
  selection/viewport write for windows outside the column strip) + :3927-3945
  (`add_filter` admits Normal non-transient windows only)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (float-focus anchor and newcomer-layer legs; position and activation stay
  `S(S-pap-ins)`)
- `S-kar-ins` karousel:src/lib/world/clientState/Tiled.ts:8-17 (ordinary
  admission opens a new column after the last-focused column, else the
  last column, appending the window at the bottom) and
  src/lib/layout/Grid.ts:150-158 (new column inserts after its left
  neighbor) and src/lib/layout/Column.ts:275-295 (`onWindowAdded`
  end-inserts and focuses only a window that is already focused) and
  src/lib/layout/Window.ts:8-24 (maximized/fullscreen newcomers skip
  arrange instead of fighting the user) and
  src/lib/layout/Column.ts:14,267-272 (stacked display exists behind
  `toggleStacked`, off unless `stackColumnsByDefault`) and
  src/lib/layout/Grid.ts:97-107 (`columnsSetX` repositions from the new
  column onward without writing widths) and
  src/lib/world/ClientWrapper.ts:24 (`preferredWidth` from the client
  frame width at wrap)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (new-column position leg; KWin-side focus, viewport, and settled widths
  stay TBD)
- `S-kar-fltanchor` karousel:src/lib/layout/Grid.ts:16 (null init) + :165-166
  (removal fixup) + :195-202 (`onColumnFocused` is the only other writer of
  `lastFocusedColumn`) + src/lib/world/ClientManager.ts:181-188
  (`onClientFocused` returns unless `findTiledWindow` resolves a tiled window)
  + src/lib/layout/Window.ts:75-85 (`onFocused` forwards to
  `column.onWindowFocused`) + src/lib/layout/Column.ts:327-329
  (column focus bridge)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (float-focus anchor leg; new-column position and focus remainder stay
  `S(S-kar-ins)`)
- `S-pan-ins` paneru:src/ecs/triggers.rs:1064-1145 (`spawn_window_trigger`
  spawns the managed entity and emits the spawn event) and :771-879
  (`window_managed_trigger`: re-inserts at the remembered previous strip
  index when it still exists, else into the active strip at the config
  `insertion()` index, else at the visually overlapped column, else at the
  end, then reshuffles) @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (insertion-position policy; focus outcome stays TBD)
- `S-pan-fresh` paneru:src/ecs/triggers.rs:1220-1228 (`apply_window_positions`
  fresh-`Added<Window>` path) + :1258-1270 (rule-floating windows leave the
  strip as `Unmanaged::Floating`) + :1282-1304 (rule `insertion()` index, else
  after-focus, else append) + :1309-1324 (`dont_focus` keeps focus, else
  synthesize `WindowFocused` for the newcomer) + src/ecs.rs:857-866
  (`insertion()`/`dont_focus()` default to unset/false absent a matching rule)
  + src/ecs/params.rs:310-312 (`focused()` returns the focused entity
  regardless of managed state) + src/ecs/layout.rs:419-431 (`index_of`
  misses for entities outside the strip)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (fresh-admission anchor and focus legs under shipped defaults; widths and
  reshuffle are out of scope for the anchor/focus Observe)
- `S-pan-model` paneru:src/ecs/layout.rs:196-203 (`StackItem` distinguishes
  single windows from app-native tabs) and :256-265 (`Column::Stack` is
  ordered top-to-bottom, `Column::Tabs` holds native tabs, `Fullscren` is
  a separate kind) @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
- `S-hyp-pre` Hyprland:src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:715-748
  (`layoutmsg preselect <direction>` writes `m_overrideDirection`) and
  :153-182 (the override forces the admission axis and newcomer side, then
  resets after one opening unless `permanent_direction_override` is set) and
  src/config/values/ConfigValues.cpp:767 (`permanent_direction_override`
  default false, also covered by `S(S-hyp-defaults)`)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (verb, override, and one-shot-vs-persistent legs; exact fixture frames
  stay TBD)
- `S-bsp-pre` bspwm:doc/bspwm.1.asciidoc:431-434 (`node -p DIR` preselects
  the splitting area, `-o` its ratio: manual insertion mode) and
  src/messages.c:359-382 (verb parsing plus `~` cancel) and
  src/tree.c:215-223 (`presel_dir` stores the split direction) and
  :410-447 (`insert_node` preselect branch: stored direction overrides the
  automatic axis, WEST/EAST vertical plus NORTH/SOUTH horizontal with the
  stored ratio, then `cancel_presel`) and :237-251 (`cancel_presel`
  one-shot cancel)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (verb, manual-mode, override and one-shot-consumption legs; exact
  fixture geometry stays TBD)
- `S-i3-split` i3:parser-specs/commands.spec:254-257 (`split
  v|h|t|vertical|horizontal|toggle` into `cmd_split`) and
  src/commands.c:1174-1200 (`cmd_split` via `tree_split` VERT/HORIZ, `t`
  toggles the current orientation)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (verb and orientation-set legs; `tree_split` wraps immediately with
  percent carry and the wrapper persists for later admissions)
- `S-cos-tilefocus` cosmic-comp:src/shell/layout/tiling/mod.rs:1835-2087
  (`next_focus`: `In` descends to the remembered else first child, `Out`
  returns the parent group, directional orientation walk with geometric
  descent, exhausted edges None)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (tiled directional/group focus; exact tie rectangles stay TBD)
- `S-cos-focuskeys` cosmic-comp:data/keybindings.ron:9-18 (shipped
  `Focus` verbs Left/Right/Up/Down/Out/In; no next/previous cycle verb;
  the switcher is an external `System` command)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (focus-verb inventory; switcher listing policy is external)
- `S-hyp-focus`
  Hyprland:src/config/shared/actions/ConfigActions.cpp:440-530
  (`moveFocus`: directional query, group-cycle, monitor fallback,
  full-size stay) and :1736-1785 (`cycleNext` verb plus workspace
  cycle; previous runs the same verb with the direction bool flipped;
  no float filter unless an only-flag is passed) +
  src/config/shared/actions/ConfigActions.hpp:50-61 and :84 (focus inventory:
  directional, window, current/last, urgent/last, monitor, cycle; no
  parent/child container verb) +
  src/desktop/state/WindowQuery.cpp:146-207 (tiled intersect walk with
  history vs shared-length tie select) and :100-125 (directional search skips
  non-allowed candidates while a non-layout-managed covering fullscreen exists)
  and :277-313 (cycle availability
  gate: same-workspace unless visible, mapped, float-filtered) +
  src/desktop/state/WindowQuery.hpp:21-27 (`SWindowCycleOptions`
  defaults `visible=false`) +
  src/desktop/state/WindowState.cpp:14-20 (window list is view-create
  order) + src/config/values/ConfigValues.cpp:621
  (`binds:focus_preferred_method` defaults 0 history) and :623
  (`binds:movefocus_cycles_fullscreen` defaults false) +
  src/config/shared/actions/ConfigActions.cpp:104-138
  (`tryMoveFocusToMonitor`: active-workspace focus candidate with cursor
  warp, else monitor-middle warp) +
  src/state/MonitorQueryCore.cpp:133-194 (`directionLookup`: STICKS
  edge-touch plus longest-overlap selection) +
  src/workspace/HLWorkspace.cpp:134-144 (`getFocusCandidate`:
  last-focused else top-left else first)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (focus verbs plus tie/cycle/order/fullscreen-gate legs plus
  cross-output focus-target legs)
- `S-bsp-cycle` bspwm:doc/bspwm.1.asciidoc:52 (`CYCLE_DIR` next|prev)
  and :82-116 (NODE_SEL incl `first_ancestor` and PATH `parent`/`first`/
  `second` jumps) and :412-414 (`node -f` focus verb) and
  src/tree.c:891-930 (in-order `next_node`/`prev_node` walk over all
  nodes incl internals) and :859-880 (`first/second_extrema` resolve to
  leaves) and :1108-1122 (`find_first_ancestor` climbs to the first
  matching parent) and :811-820 (`is_focusable` admits containers with a
  shown client leaf) and :1729-1780 (`find_closest_node` desktop-wrap
  loop) and src/query.c:1070-1223 (empty selector matches any non-NULL
  node, internals included) and src/window.c:925-941 (focusing a
  client-less container clears X input to root)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (bare next/prev walks every node with match-all; container focus
  stores the internal and clears X input; wrap lands on extrema leaves)
- `S-i3-focusnext` i3:parser-specs/commands.spec:185-201 (`focus`
  direction/next|prev/sibling/parent|child grammar) and
  src/commands.c:1292-1340 (`cmd_focus_direction` auto-direction via
  parent orientation, `cmd_focus_sibling`)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (next/prev/sibling verbs; wrap config is `S(S-i3-flt-focus)`)
- `S-i3-focuslvl` i3:src/commands.c:1403-1430 (`cmd_focus_level`
  parent|child) and src/tree.c:386-409 (`level_up`/`level_down`)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (container focus scope)
- `S-sway-focusnext` sway:sway/commands/focus.c:17-60
  (`get_direction_from_next_prev` parent-layout mapping) and :440-450
  (next/prev/sibling dispatch)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (next/prev verbs; walk and wrap are `S(S-sway-focus)`)
- `S-sway-focuslvl` sway:sway/commands/focus.c:355-380
  (`focus_parent`/`focus_child` via parent node and active tiling
  child) and :432-438 (parent|child dispatch)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (container focus scope)
- `S-awe-cycle` awesome:lib/awful/client.lua:256-290 (`client.next`
  index cycle via `gmath.cycle` over visible clients with the focus
  filter) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (cycle verb and wrap; order via S-awe-tile+S-awe-manage)
- `S-nir-focus` niri:src/layout/scrolling.rs:1581-1600
  (`focus_left`/`focus_right` edge booleans) and :1465-1476
  (`activate_window` sets the column member then activates the column) and
  :4411-4426 (`activate_idx` stored member plus column `activate_window`) and
  src/layout/workspace.rs:938-990 (tiling/floating dispatch,
  first/last, `LeftOrLast`/`RightOrFirst` wrap variants)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (column focus verbs and edge policy; left/right steps preserve the
  target column's stored member)
- `S-nir-actions` niri:niri-ipc/src/lib.rs:322-390 (`FocusWindow`,
  `FocusWindowInColumn`, `FocusWindowPrevious`, `FocusColumnLeft/Right/
  First/Last/LeftOrLast/RightOrFirst`, `FocusWindowUp/Down` variants;
  no plain spatial next/previous cycle pair)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (focus-verb inventory)
- `S-pap-focus` PaperWM:tiling.js:1129-1200 (`switch` with `loop`,
  left/right column step, `sortWindows` topmost pick, up/down rows,
  `ensureViewport`) and :5562-5570 (`sortWindows` stacking order)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (directional switch incl stack-topmost member pick; linear cycle verbs
  are `S(S-pap-cycle)`)
- `S-pap-focusmode` PaperWM:tiling.js:36-37 (`FocusModes` DEFAULT 0,
  CENTER 1, EDGE 2) and :266 (`focusMode` DEFAULT) and :4577-4594
  (`getDefaultFocusMode` falls back to DEFAULT)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (viewport-mode inventory, not evidence of host hover/click producers)
- `S-pap-grab` PaperWM:grab.js:55-145 (`MoveGrab.begin` plus 300px
  vertical / Ctrl / monitor-change DnD trigger with minimaps hidden)
  and :437-556 (`end` inserts at the DnD zone with activation,
  temporarily makes scratch off-zone then unmakes on animation
  completion, or stays with activation when DnD never began) +
  scratch.js:137-143 (unmake clears float/above/sticky) +
  tiling.js:4520-4535 (MOVING grab begins the PaperWM move) and
  :4536-4560 (RESIZING_* builds a no-op marker `ResizeGrab`)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (pointer DnD zone model with minimaps hidden; resize grabs are
  native-Mutter journeys)
- `S-pap-grabzone` PaperWM:grab.js:213-338 (`selectDndZone` yields
  `[j]` column or `[j,i]` row targets only, never a join; a null
  hover clears nothing, so the last acquired zone stands) and
  :360-435 (`motion` tracks the clone to the pointer in DnD, clone-only
  scroll-phase otherwise) and :590-640 (`tile-preview` zone actors,
  null early-returns) and :115-127 + :184 (button-release/touch-end/
  motion/monitor end signals only, no extension key path) and
  :658-665 (`ResizeGrab.end` no-op marker) +
  tiling.js:2218-2219 (display grab-op begin/end signals) and
  :3692-3693 (`resizeHandler` ignores the grabbed window mid-grab)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (DnD zone shapes, clone tracking, preview actors, and the release-only
  signal inventory behind `S(S-pap-grab)`)
- `S-kar-focus` karousel:src/lib/keyBindings/Actions.ts:6-60
  (`focusLeft/Right/Up/Down/Next/Previous/Start/End`, tiled-only
  dispatch via `doIfTiledFocused` in definition.ts:10-53) and
  src/lib/layout/Column.ts:198-211 (`getFocusTaker` retained per column,
  `getWindowToFocus` focus-taker else first)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (focus-verb inventory; left-column member follows the per-column
  focus-taker)
- `S-pan-cmds` paneru:src/types/commands.rs:220-275 (`Operation`:
  directional `Focus`, `FocusOrVirtual`, `FocusManaged/Unmanaged`,
  `RaiseFloating`; no next/previous cycle pair, no parent verb)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (focus-verb inventory; traversal stays TBD)
- `S-pan-switcher` paneru:src/types/commands.rs:277-281
  (`ToggleFloatingLayer`: Alt-tab flips only the active workspace's
  floating/tiled tiers and focuses the tier's last-focused window; no
  cross-strip listing) @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (no native cross-virtual-strip switcher/listing op; macOS owns
  listing and activation)
- `S-pan-axfs` paneru:src/util.rs:193-197 (AX `AXFullScreen`
  observation; no zoom/maximize AX attribute read in the inspected
  surface)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (native fullscreen is observable; host zoom is not)
- `S-pan-fsfocus` paneru:src/commands.rs:292-318 (West focus on a native-
  fullscreen space raises the last column top instead of traversing) +
  src/ecs/workspace.rs:267-287 (native fullscreen pins a `Fullscren`
  strip with a restore marker) and :400-453 (`SpaceDestroyed` reinserts
  the fullscreen window at the marker index with reshuffle and despawns
  the strip; no focus write)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (fullscreen focus branch plus exit restore)
- `S-pan-mouse` paneru:src/ecs/mouse.rs:100-180 (`mouse_moved_trigger`
  focuses the window under the cursor when FFM is enabled, 50ms
  throttle; interaction-tested) and :207-253 (`mouse_down_trigger`
  marks held plus `mouse_up_trigger` reshuffles around the clicked
  window) and :294-360 (`mouse_resize_trigger` resizes width by 5x
  pointer delta while the resize modifier holds) +
  src/lua/convert.rs:174-175 (`MouseDragged` forwards to Lua only, no
  layout drag model)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (hover focus plus click reshuffle plus modifier resize; host
  click-focus and pointer-drag journeys stay TBD)
- `S-pan-focusobs` paneru:src/ecs/triggers.rs:230-312
  (`window_focused_trigger` host-follow path: frontmost guard,
  app-reported window wins, Hidden unhide re-trigger, restore-guard
  absorb, already-focused short-circuit) + src/events.rs:52-113
  (`Event` window inventory: `WindowFocused` host-follow only, no
  activation-request/urgency-mark event)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (host-observed focus follow with guards; activation/urgency marker
  policy stays host-owned)
- `S-ours-focus` omnitiler:crates/tiler-core/src/directional.rs:50-64
  (axis/step for direction) and :1068-1135 (`descend_focus_target`
  plus `plan_focus`: matching-axis climb, same-axis edge child else
  perpendicular first child, exhausted edges `Edge`; targets are
  leaves only, never containers) and
  crates/tiler-core/src/session/ops/focus.rs:26-115 (`propose_focus`:
  opaque match, `plan_focus` wrap, `Edge` refuses `Unchanged` with no
  plan and no pending; single-output cross-output attempts refuse the
  same way) and kwin/src/world-apply.ts (focus arrives only via one-shot
  command effects with arrival/view proof) and
  crates/tiler-windows/src/tiling_sys.rs:5402 (`actuate_focus`) and
  :6111-6120 (`FocusDirectional` reply actuated, `focus-ok` outcome)
  @9241c94
  (leaf-only directional focus model plus adapters' delivery;
  selected intent is never evidence)
- `S-cos-move` cosmic-comp:src/shell/layout/tiling/mod.rs:1507-1560
  (`move_current_node` entry, stack-internal move, R1 orientation
  mapping) and :1598-1830 (R1/R2/R3 branches plus output fallback)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (directional move implementation; unresolved predicates are identified in cells)
- `S-cos-move-out` cosmic-comp:src/input/actions.rs:812-880
  (`Action::Move` maps `MoveFurther` to previous/next-workspace at default
  `Vertical` else `MoveToOutput`, propagate true) and :613-660
  (`MoveToOutput` via `next_output` plus `move_current` with follow) and
  :436-510 (`MoveToPreviousWorkspace` attempt plus Err-propagate fallback
  to `MoveToOutput`) and src/shell/mod.rs:2273-2300 (`next_output` full-geometry
  overlap plus nearest origin distance, no refusal) and :3164-3200
  (`move_current` to the target output active workspace) and
  cosmic-comp-config/src/workspace.rs:40-45 (default `Vertical`)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af for compositor paths,
  @3d55cba0 for config
  (exhausted-move output callchain; enumeration order unrecorded)
- `S-cos-swap` cosmic-comp:src/input/actions.rs:883-902
  (`Action::SwapWindow` opens an overview `SwapWindowGrab` for the focused
  node descriptor, no directional target form) +
  data/keybindings.ron:87 (`SwapWindow` bound Super+x; directional chords
  bind `Move`, :19-26) @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (overview-grab swap inventory only; directional swap has no counterpart)
- `S-cos-pre` cosmic-comp:src/input/actions.rs:962-976
  (`ToggleOrientation`/`Orientation` dispatch to `update_orientation` on the
  focused parent group) + src/shell/layout/tiling/mod.rs:2089-2130
  (`update_orientation` flips the existing parent axis with proportional
  rescale, no focus write) + data/keybindings.ron:83 (Super+o
  `ToggleOrientation`; no preselect binding in :83-92) +
  src/config/key_bindings.rs:6-25 (`Action`/`PrivateAction` carry internal
  `Resizing` only) @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (existing-axis toggle inventory only; one-shot admission preselect has no
  counterpart)
- `S-bsp-move-target` bspwm:src/tree.c:1124-1149 (directional candidates
  are leaves on all monitors' shown desktops; distance then history rank)
  and :1489-1620 (leaf exchange, same-desktop focus retention and
  cross-monitor follow) and src/geometry.c:49-154 (directional range and
  boundary distance) and src/history.c:311-323 (MRU rank) and
  src/settings.c:108 (default HIGH directional tightness)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
- `S-qti-swap-inventory` qtile:libqtile/layout/columns.py:173-191
  (internal `swap` helper, not exposed) and :204-508 (exposed command
  inventory: directional shuffles, no standalone swap) and
  libqtile/backend/x11/window.py:2249 (interactive drag calls the helper)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-nir-move` niri:src/layout/scrolling.rs:1702-1800 (`move_left`/
  `move_right` column reorder, `move_down`/`move_up` in-column step)
  and :1795-2060 (`consume_or_expel`/`consume_into`/`expel_from`) and
  src/layout/workspace.rs:1072-1125 (tiling/floating move dispatch) and
  niri-ipc/src/lib.rs:389-442 (column/window move/consume verbs)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (column move inventory; tree-nesting legs have no counterpart)
- `S-pap-move` PaperWM:tiling.js:4440-4500 (`move_to` viewport placement,
  not membership reorder) and :5228-5260 (`slurp` join) and :3490 (`barf`
  expel path) and :1129-1200 (`switch` directional focus, not a move)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (column/row model and viewport/join paths; registered directional verbs
  bind same-space swap per `S(S-pap-moveverbs)`)
- `S-kar-move` karousel:src/lib/keyBindings/Actions.ts:86-160
  (`windowMoveLeft/Right` shared-vs-single column paths,
  `windowMoveUp/Down` in-column step, `windowMoveNext/Previous`)
  and :184-200 (`columnMoveLeft/Right/Start/End`) and
  src/lib/layout/Column.ts:41-60 (`moveWindowUp/Down`) and
  src/lib/layout/Grid.ts:27-51 (`moveColumn`)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (column/window move inventory; tree-nesting legs have no counterpart)
- `S-pan-move` paneru:src/types/commands.rs:220-270 (`Swap`,
  `VirtualMove`, `ToNextDisplay` verbs; directional `Focus` separate)
  and src/ecs/layout_ops.rs:89-130 (`Swap` same-strip exchange,
  `MoveToWorkspace` virtual-row marker)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (swap/virtual-move inventory; split-tree reparent legs TBD)
- `S-i3-swap` i3:src/commands.c:1961-2011 (`cmd_swap`: target resolved by
  explicit `id`/`con_id`/`mark` only, no directional form) and
  src/con.c:2580-2659 (`con_swap` leaf exchange)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (targeted swap only; directional swap has no counterpart)
- `S-ours-move` omnitiler:crates/tiler-core/src/directional.rs:786-929
  (`plan_local`: perpendicular R1 wrap, 2-child R2a leaf swap, R2b
  group insert/split, N-ary R2c wrap, R3 escape with same-axis insert
  vs R1 continuation) and :945-1021 (`plan_move_with_capabilities`:
  only Left/Right cross outputs, Up/Down never cross)
  and crates/tiler-core/src/session/ops/move.rs:529-601 (applied R1
  perpendicular wrap with mover at the directional end) and :737-828
  (applied R2b insertion and retained mover focus target)
  @9241c94
  (Engine move rules: every `MoveOperation` is directional; neighbor
  exchange occurs only as R2a inside a directional move, never as a
  standalone swap verb)
- `S-cos-resize` cosmic-comp:src/shell/layout/tiling/mod.rs:2443-2475
  (`possible_resizes` edge walk) and :2477-2512 (`resize_request` nearest
  matching-edge-axis ancestor) and :2514-2600 (pixel `resize` with
  pair/leaf minima) + data/keybindings.ron:91-92 (`Resizing`
  Outwards/Inwards) + src/input/mod.rs:895-900 (tiling resize-fork
  handle between tiles keeps keyboard focus on grab) +
  cosmic-settings-daemon:config/src/shortcuts/action.rs:8-151 (Action
  inventory at pin: only `Resizing` for resize plus magnification
  `ZoomIn`/`ZoomOut`; no equalize/balance) +
  src/config/key_bindings.rs:6-25 (`Action`/`PrivateAction`: internal
  `Resizing` only) + src/shell/layout/tiling/mod.rs:219-244
  (`add_window` admission `equal_sizing`, not a user verb) +
  data/keybindings.ron:83-92 (tiling-adjacent bindings:
  orientation/stacking/tiling/floating/swap/maximize/fullscreen/Resizing
  only) + src/shell/layout/tiling/grabs/resize.rs:385-437 (fork-drag
  rounded axis delta accumulates into the ancestor pair with the same
  360/240 minima)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af for cosmic-comp,
  @e37160f14d1e7ee428f973cd2848b4e95f83dfe1 for the daemon path
  (pin from cosmic-comp `Cargo.lock`)
  (keyboard pixel resize plus fork handle and fork-drag delta; dragged
  share needs geometry; no local or workspace-wide equalize verb)
- `S-hyp-resize`
  Hyprland:src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:312-399
  (`resizeTarget` pixel delta plus edge/smart-resizing path) +
  src/config/shared/actions/ConfigActions.cpp:670-683 (pixel `resize`
  dispatcher) + src/config/shared/actions/ConfigActions.hpp:39-111
  (window/workspace action declarations at pin: close/kill/signal/
  float/pseudo/pin/fullscreen/move/swap/focus/center/cycle/tag/pass/
  set_prop/group/workspace/monitor/special/exec/submap/dpms verbs plus
  pixel `resize`; no equalize/balance) +
  src/config/lua/bindings/LuaBindingsDispatchers.cpp:1298-1360
  (dispatched names at pin; window table carries `resize` only, no
  equalize name) +
  src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:681-769
  (`layoutmsg` inventory: togglesplit/swapsplit/rotatesplit/movetoroot/
  preselect/splitratio only) and :749-767 (`splitratio` adjusts the
  single `CURRENT_NODE` parent split by delta or exact value clamped
  0.1-1.9, not the whole workspace) +
  src/layout/LayoutManager.hpp:80 (keyboard `resize` lands with default
  corner `CORNER_NONE`) + src/config/values/ConfigValues.cpp:766
  (`dwindle:smart_resizing` defaults true)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (keyboard pixel-delta step via the CORNER_NONE smart path:
  side-by-side/inner-parent scope, pair-only, reversible absent the
  0.1-1.9 clamp; exact shares need parent-box geometry; no local or
  workspace-wide equalize verb)
- `S-bsp-resize` bspwm:doc/bspwm.1.asciidoc:439-442 (`-z` pixel handle)
  and :454-458 (`-E`/`-B`) + src/messages.c:432-447 (`-z` dispatch) and
  :557-569 (`-E`/`-B` dispatch to `equalize_tree`/`balance_tree`) +
  src/tree.c:1258-1283 (equalize/balance) + src/window.c:547-590
  (`resize_client` fence share move with reflow) + src/tree.c:1003-1025
  (`find_fence` climb to the first matching parent) + src/settings.h:44
  (shipped `SPLIT_RATIO` 0.5)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (pixel-handle fence-share move with reflow; root equalize/balance evidenced)
- `S-i3-resize` i3:src/commands.c:451-467 (tiling-direction participants)
  and :544-581 (`resize grow|shrink`, shrink negates) and
  src/resize.c:72-144 (climb to the first matching orientation) and
  parser-specs/commands.spec:280-311 (grammar, default 10px/ppt) and
  parser-specs/commands.spec:12-48 (`INITIAL` command inventory at pin:
  move/exec/layout/focus/split/`resize`/swap and others; no
  equalize/balance/normalize command) and src/commands.c:623-670
  (`cmd_resize_set` writes an exact width/height on the single focused
  container via `resize_set_tiling`, not all siblings) and :590-621
  (`resize_set_tiling` per-target delta)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (pair-step grow/shrink plus single-container exact set; no local or
  tree-wide equalize verb)
- `S-sway-resize` sway:sway/commands/resize.c:45-64 (resize-parent climb)
  and :237-280 (tiled adjust, default 10/ppt, unchanged error) and
  :554-576 (grow/shrink dispatch) and sway/commands.c:114-143 (runtime
  command table at pin: layout/move/`resize`/split/swap and others; no
  equalize/balance verb) and sway/commands/resize.c:412-470
  (`cmd_resize_set` writes an exact size on the single focused container,
  not all siblings) and sway/tree/arrange.c:48-52,133-137 (width/height
  fraction `normalize` re-sums to 1.0 internally, not a user verb)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (pair-step grow/shrink plus single-container exact set; no local or
  workspace-wide equalize verb)
- `S-xmo-resize` xmonad:src/XMonad/Layout.hs:77-78 (`Shrink`/`Expand`
  move `frac` by `delta` 3/100) and src/XMonad/Config.hs:211-212
  (`mod-h`/`mod-l`) and src/XMonad/Layout.hs:41-78 (`Resize`
  `Shrink`/`Expand` plus `IncMasterN` messages only) and
  src/XMonad/Config.hs:188-227 (key inventory at pin:
  spawn/kill/NextLayout/refresh/focus/swap/shrink/expand/sink/IncMasterN/
  quit/restart only; no equalize verb) and
  xmonad-contrib:XMonad/Layout/BinarySpacePartition.hs:107-122
  (`Balance` retiles, `Equalize` tunes ratios; out of this
  Tall+Navigation2D profile) and :465-476 (`equalize`/`balancedTree`
  implementations) and :774-775 (`Equalize`/`Balance` dispatch)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (frac-only; no edge-targeted verb; no in-profile equalize verb)
  (contrib path @5097a457e7a409bc9a7584dc5aa82b34c69d6dda)
- `S-qti-resize` qtile:libqtile/layout/columns.py:134 (`grow_amount` 10)
  and :309-310 (width weights project proportionally to work-area pixels)
  and :509-561 (directional grows move width/height from the neighbor) and
  :563-570 (`normalize` equal widths)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-awe-resize` awesome:lib/awful/tag.lua:760-770 (`incmwfact`
  master-factor step) + awesomerc.lua:311-313 (`mod-l` +0.05 / `mod-h`
  -0.05) + lib/awful/layout/suit/tile.lua:232-310 (mwfact partition) +
  lib/awful/tag.lua:753-756 (`setmwfact` sets the single master factor)
  and :1395-1413 (`incnmaster` changes the master count) and :1515-1530
  (`incncol` changes the column count) + awesomerc.lua:315-321
  (`mod-Shift-h/l` master count, `mod-Ctrl-h/l` column count) +
  lib/awful/client.lua:1155-1171 (`normalize` helper re-sums ratios) and
  :1257-1300 (`setwfact` writes one client's window factor) and
  :1322-1345 (`incwfact` steps one client's factor with renormalization)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (no edge-targeted verb; master-factor/count/column and per-client
  factor verbs adjust one share or count, never equal shares to all
  clients)
- `S-nir-resize` niri:src/layout/scrolling.rs:4927-4973 (preset-cycle
  index plus preset apply) and :4990-5022 (`set_column_width`
  proportion/fixed/adjust) + src/layout/mod.rs:3020 (`toggle_width`) +
  src/input/mod.rs:1620-1623 (`SwitchPresetColumnWidth(Back)` dispatch) +
  niri-ipc/src/lib.rs:715-761 (width-action inventory: preset, maximize,
  set/adjust; no edge-targeted verb) +
  resources/default-config.kdl:556-558,589-590 (`Mod+R`/`Mod+Shift+R`
  binds) + niri-ipc/src/lib.rs:194-770 (Action inventory at pin: focus/
  move/consume/expel/swap/center/preset/set/maximize/expand verbs; no
  equalize/balance) and :675-767 (per-column/per-window width verbs:
  `SetWindowWidth`/`SetWindowHeight` single target, `ResetWindowHeight`
  automatic height only, `SetColumnWidth` single column,
  `ExpandColumnToAvailableWidth` focused column only) +
  src/layout/scrolling.rs:2630-2670 (`toggle_width`/`set_window_width`
  per-column) and :2700-2720 (`reset_window_height` single window) and
  :2265-2290 (`center_visible_columns` viewport centering, no width
  equalization) and :2772-2810 (`expand_column_to_available_width`
  focused-column growth only)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (columns independent; no edge-targeted counterpart; no local or
  workspace-wide equalize verb)
- `S-pap-resize` PaperWM:tiling.js:4873-4912 (`resizeWInc`/`resizeWDec`
  10% step) and :4937-4960 (width cycle direction) + lib.js:11-40
  (`findNext`/`findPrev` wrap at the preset ends) +
  keybindings.js:270-291 (registered action inventory: w/h inc/dec plus
  width/height cycling; no edge-targeted verb) +
  keybindings.js:270-313 (registered action inventory at pin: resize-h/w
  inc/dec plus width/height cycling plus center/slurp/barf/maximize; no
  equalize/balance) and tiling.js:4832-4871 (`resizeHInc`/`resizeHDec`
  per-window step) and :4929-4970 (per-window width cycling through
  presets)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (grid-snapped step; neighbor columns keep widths per `S(S-pap-layout)`;
  no edge counterpart; no local or space-wide equalize verb)
- `S-kar-resize` karousel:src/lib/keyBindings/Actions.ts:160-175
  (height actions) and :203-247 (column width increase/decrease/cycle plus
  `columnsWidthEqualize` via `fillSpace`; no edge-targeted verb) +
  src/lib/world/World.ts:29 (shipped `scrollingCentered=false` selects
  `ContextualResizer`) +
  src/lib/behavior/columnResizer/ContextualResizer.ts:6-41
  (increase: smallest strictly greater width, recenters viewport) and
  :43-88 (decrease: separate contextual path) +
  src/lib/behavior/columnResizer/RawResizer.ts:6-30 (preset-step
  increase/decrease, centered-mode only)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (contextual step; reversal not established; no edge counterpart)
- `S-kar-ptr` karousel:src/lib/world/clientState/Tiled.ts:71-115
  (interactive move/resize session hooks: move untiles under
  `untileOnDrag` else marks moving with retile-back on finish; resize
  records start width plus neighbor) and :144-153 (width-change
  handler calls `onUserResizeWidth`) +
  src/lib/config/definition.ts:122-126 (`untileOnDrag` defaults true)
  and :137-141 (`resizeNeighborColumn` defaults false)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (pointer move untiles at shipped default; edge resize writes the
  dragged column width with a stable neighbor)
- `S-pan-resize` paneru:src/types/commands.rs:152-180 (`ResizeDirection`
  Grow/Shrink) and :226-242
  (`Resize`/`SetWidth`/`Equalize`/`Balance`; no edge-targeted verb) +
  src/commands.rs:744-829 (`resize_window` preset cycle) and :1291-1330
  (`equalize_column`: `Stack`-only height evening) and :1331-1372
  (`balance_strip`: every column to the focused width) +
  src/ecs/layout.rs:258-266 (`Single` vs `Stack` columns)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (neighbor mapping TBD; no edge counterpart)
- `S-pan-setwidth` paneru:src/types/commands.rs:231-232
  (`Operation::SetWidth(f64)` exact display-width ratio) and
  src/ecs/layout_ops.rs:162-200 (`LayoutOp::SetWidth` stores `WidthRatio`
  per window, shared with stacked siblings, routed through interactive
  resize for the focused window)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (exact-ratio preparation; per-window ratios survive sibling removal)
- `S-hyp-spc` Hyprland:src/desktop/view/window/X11Backend.cpp:53-96
  (DIALOG/SPLASH/TOOLBAR/UTILITY float atoms; non-DIALOG floats suggest
  no initial focus) and :84-96 (`suggestsFloat`: modal, transient,
  role, override-redirect, parent, or min==max fixed size) +
  src/desktop/view/window/WaylandBackend.cpp:25-42 (parent or either-dim
  fixed size suggests float; modal flag; no splash/utility type branch) +
  src/layout/algorithm/floating/default/DefaultFloatingAlgorithm.cpp:20-45,91-100
  (new floats center in the work area with no parent-relative branch; X11
  requested geometry or rule position excepted) +
  src/desktop/state/FocusState.cpp:89-93 (Wayland modal-child parent-focus
  refusal) + src/config/values/ConfigValues.cpp:193
  (`general:modal_parent_blocking` defaults true)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (transient/modal/type/fixed-size float legs plus centered placement and the
  Wayland modal fence; newcomer focus is `S(S-hyp-newfocus)`)
- `S-bsp-spc` bspwm:src/rule.c:230-253 (DIALOG floats centered;
  TOOLBAR/UTILITY set no-focus; DOCK/DESKTOP/NOTIFICATION unmanaged) and
  :276-289 (transient floats) and :291-299 (min==max fixed size floats)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (type/transient/fixed-size admission legs; placement stays TBD; switcher is `S-bsp-switcher`)
- `S-sway-spc` sway:sway/desktop/xdg_shell.c:229-235 (`wants_floating`:
  either-dimension min==max or parent, no modal branch) and sway/desktop/xwayland.c:308-340
  (modal, DIALOG/UTILITY/TOOLBAR/SPLASH, or fixed size floats)
  and sway/tree/container.c:864-908,910-931 (admission floats center
  on the workspace/output at default half-width/three-quarter-height,
  not parent-relative) and sway/tree/view.c:696-730,944-955
  (`should_focus`: ordinary newcomer takes focus on the active workspace)
  and sway/tree/view.c:1168-1194 (focus validity lists only
  sticky/tab/fullscreen/transient fences, no modal branch)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (backend-split float legs with centered placement, newcomer focus,
  and no modal fence)
- `S-qti-spc` qtile:libqtile/layout/floating.py:14-30 (shipped
  `default_float_rules`: utility/notification/toolbar/splash/dialog
  plus fixed-size/ratio; transient match is doc-only, not default) and
  :169-204 (unplaced floats center; transients center on the parent at
  :180-184) + libqtile/group.py:226-244 (`add` floats on rule match,
  focuses when stealable) + libqtile/backend/x11/window.py:1232-1233
  (`can_steal_focus`, notification excluded; no modal branch anywhere) +
  libqtile/backend/wayland/window.py:627-628 (`get_wm_type` delegates to
  the view) + libqtile/backend/wayland/qw/xdg-view.c:303-310 (xdg returns
  normal/dialog only) and libqtile/backend/wayland/qw/xwayland-view.c:396-430
  (XWayland preserves utility/splash/dialog types)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (type/fixed-size float legs with parent centering and stealable focus;
  modal flag inert; switcher separate per `S(S-qti-switcher)`)
- `S-nir-spc` niri:src/window/mod.rs:377-396 (`compute_open_floating`:
  explicit rule, parent, or fixed positive height min==max floats) +
  src/handlers/compositor.rs:150-175,203-228 (passes the boolean to
  `add_window`) + src/handlers/xdg_shell.rs:1134-1155 (parent dialog
  placed next to the parent, following it across outputs) +
  src/utils/xwayland/satellite.rs:34-77 (optional X11 bridge setup;
  requires a working xwayland-satellite executable) +
  niri-config/src/window_rule.rs:125-144 (`Match` has no window-type
  field) + resources/default-config.kdl:322-328 (shipped Firefox PiP
  app-id/title rule opens floating; the only PiP branch in defaults)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (float admission plus parent-placement and app-rule PiP legs; native
  xdg splash/utility types do not exist; other PiP apps and focus stay TBD)
- `S-nir-fixed-open` niri:src/window/mod.rs:377-396
  (`compute_open_floating` definition: explicit rule, parent, or fixed
  positive height) + src/handlers/compositor.rs:150 (layer-shell open
  path) and src/handlers/xdg_shell.rs:635,845,1164 (map/configure open
  paths; the only callsites)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (fixed classification runs at open only; no hint-change caller)
- `S-pap-spc` PaperWM:tiling.js:3341-3374 (`isTransient`/`hasTransient`;
  transients take focus, blocking the parent on Wayland) and :3927-3945
  (`add_filter` admits Normal non-transient windows only) and :4125-4135
  (rejected windows float with `make_above`)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (transient/type float legs; fence enforcement and focus stay TBD)
- `S-kar-spc` karousel:src/lib/world/Clients.ts:7-16 (`canTileEver`:
  moveable and resizeable, or fullscreen; popups and prohibited classes
  excluded) and src/lib/rules/WindowRuleEnforcer.ts:13-24 (`shouldTile`
  requires normalWindow plus non-transient, non-modal, managed, and no
  prefer-floating rule) and src/lib/world/ClientManager.ts:72-82
  (`findTransientFor` tracks the transient link without changing state)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (transient/modal exclusion plus shapeability gate; type/kind
  resolution via pinned KWin `normalWindow`/`resizeable` per
  `S(S-kwin-resizeable)`)
- `S-kwin-resizeable` kwin:src/window.h:546 (`resizeable` scripting
  property reads `isResizable`) + src/xdgshellwindow.cpp:628-644
  (`minSize` expands to an enforced minimum, absent `maxSize` maps to
  `INT_MAX`) and :682-696 (`isResizable` is either-axis strict
  inequality `min.w<max.w || min.h<max.h`, modulo fullscreen/special/
  forced-size gates) + src/x11window.cpp:3050-3058 (`minSize`/`maxSize`
  pass ICCCM hints through with rules only) and :3446-3472 (same
  either-axis inequality plus unmanaged/NET/motif gates) +
  src/utils/xcbutils.h:932-972 (`hasMinSize`/`hasMaxSize` gates; absent
  max maps to `INT_MAX` clamped to >=1; absent min falls back to base
  size, absent base to 0,0) @8438567a
  (pinned KWin source at 8438567a via upstream export, raw
  KDE/kwin@8438567a provenance; karousel `resizeable` resolves here)
  Raw sources: [window.h](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/window.h),
  [xdgshellwindow.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/xdgshellwindow.cpp),
  [x11window.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/x11window.cpp),
  [xcbutils.h](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/utils/xcbutils.h).
- `S-kwin-tabbox` kwin:src/tabbox/tabbox.cpp:87-97 (`checkDesktop`:
  `AllDesktopsClients` lists every desktop, default
  `OnlyCurrentDesktopClients` lists the current desktop only) and
  :157-169 (`clientToAddToList` requires the desktop/activity/
  application/minimized/screen checks plus `wantsTabFocus` and
  `!skipSwitcher()`) and :256-267 (ctor: default config
  current-desktop-only, alternative config all-desktops) and :592-606
  (outside pointer press closes/aborts, no drop commit) and :631-644
  (`Enter`/`Space` accept via the keyboard path) and :992-1005
  (`accept` runs `Workspace::activateWindow`) and :1006-1014
  (`modifiersReleased` accepts via the keyboard release path) +
  src/tabbox/tabboxconfig.h:47-51 (desktop-mode enum) and :260-263
  (default mode current-desktop-only) + src/activation.cpp:294-324
  (`activateWindow`: raise, then off-desktop windows follow
  `activationDesktopPolicy`: `SwitchToOtherDesktop` switches to the
  window's desktop with membership unchanged, `BringToCurrentDesktop`
  pulls into the current desktop, `DoNothing`) +
  src/options.h:321-326 (policy enum) and :821-823 (default
  `SwitchToOtherDesktop`) @8438567a
  (pinned KWin source at 8438567a via upstream export, raw
  KDE/kwin@8438567a provenance; TabBox listing/activation policy
  resolves here)
  Raw sources: [tabbox.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/tabbox/tabbox.cpp),
  [tabboxconfig.h](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/tabbox/tabboxconfig.h),
  [activation.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/activation.cpp),
  [options.h](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/options.h).
- `S-kwin-act` kwin:src/netinfo.cpp:149-177
  (`RootInfo::changeActiveWindow`: tool-source requests
  force-activate; app-source requests activate only under
  `allowWindowActivation`, else `demandAttention`) +
  src/x11window.cpp:4257-4330 (`allowWindowActivation`: shipped FSP
  Low compares the request timestamp against the active window's
  user time; stale/zero fail to `demandAttention`, unknown
  activates) and :4249-4254 (`updateUrgency`: hint urgency marks via
  `demandAttention`) + src/window.cpp:679-691 (`demandAttention`
  refuses while active and otherwise only sets the flag) +
  src/activation.cpp:250-253 (`setActiveWindow` clears with
  `demandAttention(false)`) and :294-321 (`activateWindow` follows
  `activationDesktopPolicy` off-desktop) + src/events.cpp:313-315
  (`WM2Urgency` notify drives `updateUrgency`) +
  src/kcms/options/kwinoptions_settings.kcfg:128-132 (shipped FSP
  default Low=1) and :43-49 (shipped `SwitchToOtherDesktop`)
  @8438567a
  (pinned KWin source at 8438567a via upstream export, raw
  KDE/kwin@8438567a provenance; app-request outcome forks on the
  message timestamp; hint marks without focusing; activation clears)
  Raw sources: [netinfo.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/netinfo.cpp),
  [x11window.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/x11window.cpp),
  [window.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/window.cpp),
  [activation.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/activation.cpp),
  [events.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/events.cpp),
  [kwinoptions_settings.kcfg](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/kcms/options/kwinoptions_settings.kcfg).
- `S-kwin-manage` kwin:src/x11window.cpp:813 (`readUserTimeMapTimestamp`
  user-time/startup/session inputs) and :824-830 (manage allow fork incl
  session) and :836-837 (off-desktop switch on allow) and :851-866
  (restack-under-active on deny; `requestFocus` on allow+current vs
  `demandAttention`) and :4116-4130 (timestamp source: user time with
  startup-id override) and :4257-4330 (`allowWindowActivation` FSP/
  timestamp fork) @8438567a
  (pinned KWin source at 8438567a via upstream export, raw
  KDE/kwin@8438567a provenance; X11 newcomer activation leg; karousel
  focus-taker follows only on KWin focus)
  Raw sources: [x11window.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/x11window.cpp).
- `S-kwin-add` kwin:src/workspace.cpp:930-951 (`addWaylandWindow`
  `shouldActivate` incl `mayActivate`-token, FSP-Low and no-active
  branches, plus activate vs `demandAttention`/restack) +
  src/activation.cpp:578-614 (`mayActivate` token/app-id/
  transient-serial/rules legs) + src/xdgactivationv1.cpp:104-125
  (token activate path) @8438567a
  (pinned KWin source at 8438567a via upstream export, raw
  KDE/kwin@8438567a provenance; Wayland newcomer activation leg)
  Raw sources: [workspace.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/workspace.cpp),
  [activation.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/activation.cpp),
  [xdgactivationv1.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/xdgactivationv1.cpp).
- `S-kwin-winorder` kwin:src/workspace.h:210 (`windows()` returns
  `m_windows`) + src/scripting/workspace_wrapper.cpp:505-522 (scripting
  `Workspace.windows` reads `workspace()->windows()` with count/at
  index) + src/workspace.cpp:856-857 (managed X11 append at manage) and
  :868-869 (unmanaged append) and :926-927 (Wayland append at add)
  @8438567a
  (pinned KWin source at 8438567a via upstream export, raw
  KDE/kwin@8438567a provenance; script-visible window order is KWin
  manage/creation order, not focus order)
  Raw sources: [workspace.h](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/workspace.h),
  [workspace_wrapper.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/scripting/workspace_wrapper.cpp),
  [workspace.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/workspace.cpp).
- `S-kwin-scriptact` kwin:src/scripting/workspace_wrapper.cpp:260-262
  (`setActiveWindow` calls `activateWindow(window)` with the default
  `force=false`) + src/workspace.h:170 (default `force=false`) +
  src/activation.cpp:294-341 (`activateWindow`: raise, off-desktop policy
  switch, `focusPolicyIsReasonable` gate) + :366-409 (`requestFocus`:
  modal redirect, splash/shown/`wantsInput` gates, `takeFocus` plus
  `setActiveWindow`) + src/options.h:316-318 (`focusPolicyIsReasonable`
  is ClickToFocus/FocusFollowsMouse) + src/options.cpp:35-36
  (`ClickToFocus` default, `nextFocusPrefersMouse` false) @8438567a
  (pinned KWin source at 8438567a via upstream export, raw
  KDE/kwin@8438567a provenance; script focus path, not newcomer admission)
  Raw sources: [workspace_wrapper.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/scripting/workspace_wrapper.cpp),
  [workspace.h](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/workspace.h),
  [activation.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/activation.cpp),
  [options.h](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/options.h),
  [options.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/options.cpp).
- `S-kwin-switch` kwin:src/virtualdesktops.cpp:567-600 (`setCurrent`
  writes current plus emits) + src/workspace.cpp:1039-1042
  (`slotCurrentDesktopChanged` runs visibility plus activation) +
  :1092-1116 (`activateWindowOnDesktop`/`findWindowToActivateOnDesktop`:
  reasonable-policy MRU focus-chain, `NextFocusPrefersMouse` mouse branch
  else chain) + :2416-2420 (`scheduleRearrange` is a separate strut timer,
  not focus delivery) + src/focuschain.cpp:37-60 (`getForActivation`
  MRU shown window on desktop/output) +
  src/kcms/options/kwinoptions_settings.kcfg:92-104 (shipped
  `ClickToFocus` plus `NextFocusPrefersMouse` false) @8438567a
  (pinned KWin source at 8438567a via upstream export, raw
  KDE/kwin@8438567a provenance; native desktop-switch activation leg)
  Raw sources: [virtualdesktops.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/virtualdesktops.cpp),
  [workspace.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/workspace.cpp),
  [focuschain.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/focuschain.cpp),
  [kwinoptions_settings.kcfg](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/kcms/options/kwinoptions_settings.kcfg).
- `S-kwin-desktops` kwin:src/virtualdesktops.cpp:449-530
  (`createVirtualDesktop`/`removeVirtualDesktop` explicit only,
  last-desktop protected) + :603-651 (`setCount` explicit resize only) +
  karousel:src/lib/world/DesktopManager.ts:79-112 (`updateDesktops`/
  `removeKwinDesktop`/`destroyDesktop` destroy only KWin-removed
  desktops) @8438567a for kwin (pinned via upstream KWin source at 8438567a export,
  raw KDE/kwin@8438567a provenance),
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b for karousel
  (no auto-spare, no empty auto-removal; emptied desktops retained)
  Raw sources: [virtualdesktops.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/virtualdesktops.cpp).
- `S-kwin-sticky` kwin:src/window.cpp:697-809 (`setDesktops` writes the list with rules check plus transient/modal propagation and `desktopsChanged`; `setOnAllDesktops(true)` writes empty, false writes the current desktop; `isOnDesktop` is true for empty) @8438567a
  (pinned KWin source at 8438567a via upstream export, raw KDE/kwin@8438567a provenance; sticky assignment/visibility leg)
  Raw sources: [window.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/window.cpp).
- `S-kwin-close` kwin:src/activation.cpp:433-480 (`activateNextWindow`: no-op unless the closed window was active, else MRU focus-chain `nextForDesktop` plus `requestFocus`, desktop fallback) + src/focuschain.cpp:247-270 (`isUsableFocusCandidate` shown/on-current checks; `nextForDesktop` MRU usable pick) + src/workspace.cpp:955-961 (`removeWaylandWindow` activates-next before remove) + src/x11window.cpp:218/`destroyWindow` (X11 release/destroy activates-next before remove) @8438567a
  (pinned KWin source at 8438567a via upstream export, raw KDE/kwin@8438567a provenance; native close refocus leg)
  Raw sources: [activation.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/activation.cpp), [focuschain.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/focuschain.cpp), [workspace.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/workspace.cpp), [x11window.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/x11window.cpp).
- `S-kwin-min` kwin:src/window.cpp:848-866 (`setMinimized` writes the flag with rules/minimizable gates and emits only; no focus call) + src/activation.cpp:294-341 (`activateWindow` unminimizes as one step of activation; unminimize alone issues no activation) @8438567a
  (pinned KWin source at 8438567a via upstream export, raw KDE/kwin@8438567a provenance; native minimize/unminimize focus leg)
  Raw sources: [window.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/window.cpp), [activation.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/activation.cpp).
- `S-kwin-moveresize` kwin:src/window.cpp:775 (`moveResize` writes the geometry via `setMoveResizeGeometry`/`moveResizeInternal`) + :1039-1086 (interactive move/resize session start/finish signals) + :1116-1139 (`startDelayed`/`stopDelayed`: title-bar press arms a `startDragTime` timer, release stops it) + :2066-2092 (`MouseMove` press path starts the session immediately) + :2566-2575 (`endInteractiveMoveResize` finishes on release) + :2764-2797 (decoration press arms the delay, release finishes only if started) + src/input.cpp:680-692 (`MoveResizeInputFilter` release ends the session) + :1295-1324 (`Meta+Left` resolves to the Move path via `commandAll`) + src/kcms/options/kwinoptions_settings.kcfg:152-164,203-218,291-315 (shipped title-bar Raise/ActivateAndRaise plus `Meta`+`Move` defaults) @8438567a
  (pinned KWin source at 8438567a via upstream export, raw KDE/kwin@8438567a provenance; native float pointer translation plus title-bar-delayed vs modifier-immediate gesture legs)
  Raw sources: [window.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/window.cpp), [input.cpp](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/input.cpp), [kwinoptions_settings.kcfg](https://raw.githubusercontent.com/KDE/kwin/8438567a/src/kcms/options/kwinoptions_settings.kcfg).
- `S-kar-admit` karousel:src/lib/world/ClientManager.ts:30-56
  (`addClient` evaluates the shapeability gates once at add) and
  :158-176 (`toggleFloatingClient`: float-to-tile requires `canTileEver`,
  so an unshapeable fixed float keeps floating; tile-to-float always
  applies) + src/lib/rules/WindowRuleEnforcer.ts:30-44 (only
  `captionChanged`   re-evaluates tiling, for caption-follow rules; no
  size-hint watcher) @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (admission-once plus explicit-toggle gate; hint-change recompute absent)
- `S-kar-readmit` karousel:src/lib/world/World.ts:92-96
  (`addExistingClients` iterates live `Workspace.windows` into `addClient`
  each) + src/lib/world/ClientManager.ts:30-56 (`addClient` re-evaluates
  `canTileEver` shapeability plus rules/desktop fresh at add; rejects
  construct Floating anew) + src/lib/keyBindings/Actions.ts:1-60 (no
  restart/reload/persist verb in the Actions inventory)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (re-admission reclassifies every client from current host flags with no
  durable float-intent or tile-override store; prior float origin leaves
  no trace)
- `S-hyp-cfg` Hyprland:src/desktop/view/window/Window.cpp:950-970
  (`onConfigureRequest`: tiled X11 requests are refused via an
  authoritative `sendWindowSize` resend; only floats take the request)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (tile-authoritative app-resize outcome)
- `S-pan-spc` paneru:src/manager/windows.rs:230-262 (AXUnknown and
  non-real role/subrole windows ignored; forced-manage rule override)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (role-gated management; non-standard dialog/transient/splash subroles
  skipped unless forced; no transient/parent/modal branch)
- `S-pan-admit` paneru:src/manager/windows.rs:291-301 (`is_real` is
  standard-subrole or window-role plus floating-subrole; no size
  predicate) + src/ecs.rs:838-856 (`WindowProperties::floating` is
  rule-configured, never hint-derived) + src/ecs/triggers.rs:1472
  (non-resizable/minimum-width surfaces only as a runtime resize-failure
  observation, never an admission classifier) + src/ecs/systems.rs:786-895
  (`window_resized_update_frame`/`window_moved_update_frame`: live frame
  re-read into Bounds/Position with strip nudge, floating leave-alone,
  own-echo skip; no re-float or reclassification)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (no fixed-size admission counterpart: AX exposes no min/max hint
  equality, so fixed-hint fixtures cannot classify; rule-assigned float
  only; app-owned geometry observed with no re-float)
- `S-ours-spc-kde` KDE observer gate on the world route:
  omnitiler:kwin/src/world-observe.ts:434 (non-`normalWindow`
  windows skipped before observation) + kwin/src/kwin-globals.d.ts
  (`normalWindow` Q_PROPERTY)
  @f0969090a810ce85937728630f9b69ed2601dbe9
  (kind gate only; dialog/splash/utility type-eligibility mapping is
  untraced with no pinned KWin source in-repo, so typed fixtures stay TBD)
- `S-ours-spc-win` Windows candidate/admission gates:
  omnitiler:crates/tiler-windows/src/tiling_sys.rs `admission_clear_eligible`,
  `query_outer_tracks`, `min_hint_for`, `hidden_hint_for`, `assemble_domain_rows`,
  `stamp_engine_event`, `hydrate_tile_override_marker`, `writable_tokens` +
  crates/tiler-windows/src/tiling.rs `hints_from_outer` + workspace_owner.rs
  `fixed_admission_tests`, tiling_sys.rs `fixed_predicate_tests` /
  `restart_adoption_tests`, settings.rs `CoreSettings.fixed_size_predicate`
  @a41fcb5 plus runtime delivery 2026-10-11
  ([record](../changes/archive/windows-fixed-size-admission.md)). Existing
  owned/dialog/tool/no-activate exclusions stand; shared fixed admission is
  enabled after lifetime/origin and no-touch wiring. Native query/game journeys,
  PiP and standalone-splash eligibility stay TBD.
- `S-ours-resize`
  omnitiler:crates/tiler-core/src/session/ops/resize.rs:9-48
  (nearest matching-edge-axis ancestor, adjacent shares only, Unchanged
  refusal) and :49 (`propose_resize`) and :160-183 (step derivation via
  `derive_keyboard_pixel_shares` into `apply_resize_shares`; Unchanged and
  PairBelowMinimum refusals) and :1397-1533 (`derive_keyboard_pixel_shares`) +
  crates/tiler-core/src/cosmic_v1.rs:233 (`keyboard_step_px` 12 then +2)
  and :273 (`pair_admits_resize`) and :313
  (`clamp_keyboard_shrink_pair`) +
  crates/tiler-core/src/directional.rs:1297 (`apply_resize_shares`) +
  crates/tiler-core/src/session/ops/mod.rs:8-15 (operation families:
  drag/float/focus/lifecycle/move/resize/workspace, no equalize family) +
  kwin/src/world-report.ts (directional focus/move plus generic `operation`
  commands; no equalize op)
  @9241c94
  (shared Engine plus world-route operation dispatch; no equalize verb in
  either inventory)
- `S-ours-winbind` omnitiler:crates/tiler-windows/src/settings.rs:773-775
  (resize rows `implemented: false`) and :1892-1898 (resize rows are the
  only unimplemented ones) and :1167,1216 (unimplemented guards) +
  crates/tiler-windows/src/settings_ui.rs:284-287 (keyboard
  resize not intercepted; pointer resizing exists) +
  crates/tiler-windows/src/tiling_sys.rs:13026-13048 (pointer-resize
  gesture maps to `CoreCommand::PointerResize`, the Windows resize path)
  @9241c94
  (historical missing Windows keyboard-resize trigger). Superseded 2026-10-11,
  base `f794cf9` + delivery commit: `settings.rs` `RESIZE_ROWS` /
  `binding_catalog`, `snapkey.rs` `ResizeIntent` / `QueuedSnapEvent::Resize`,
  `tiling_sys.rs` `dispatch_resize_intent`, and `tiling.rs`
  `resize_repeat_next`; retained Engine resize fixtures and exact modifier /
  held-routing / queue-mask tests in `tests/tiling.rs`, `tests/snapkey.rs`,
  `tests/settings.rs`. Agent-observed downstream CLI geometry and native
  Settings Apply/Revert; physical interception remains user-owned.
  [Delivery/evidence](../changes/archive/windows-keyboard-resize.md).

  Minimal Windows preset discriminator, tentative pending user review:

  | Given | When | Ours Windows | Unsupported outcome |
  | --- | --- | --- | --- |
  | Default resize rows; Windows chord ownership unknown | Apply Compatible, then Authentic | Both keep Win+Alt and Win+Shift+Alt resize defaults with unknown-ownership / unproven-containment notes; Compatible disables known conflicts only (2026-10-11, base `f794cf9` + delivery commit, preset fixtures) | Actual foreign owner and physical OS containment TBD |
- `S-ours-mou` omnitiler:crates/tiler-windows/src/tiling_sys.rs:13017-13045
  (pointer gestures map to `CoreCommand::DragDrop`/`PointerResize`) +
  kwin/src/world-report.ts (pointer/drag intents ride generic `operation`
  commands) and kwin/src/world-drag.ts (tiled move-drop route)
  @9241c94
  (verb inventory only, never behavior: gesture verbs exist on both
  platforms while every host click/hover/share/drop outcome stays TBD)
- `S-nir-min` niri:src/layout/scrolling.rs:4572-4620 (column width
  resolves with min/max clamp) and :4621-4645 (tile-height bounds from
  the working height and multi-window minimum sizes) and :1278-1354
  (`update_window` re-runs the clamp on hint or resize updates and
  shifts neighbors with widths kept; no focus write) and :4100-4111
  (`update_config` re-resolves sizes on viewport config updates)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (width/height clamp plus reactive and resize re-resolve legs)
- `S-kar-min` karousel:src/lib/layout/Column.ts:79-102 (`getMinWidth`/
  `getMaxWidth` clamp in `setWidth`)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (admission frames TBD)
- `S-cos-orient` cosmic-comp:data/keybindings.ron:83 (Super+o
  `ToggleOrientation`) + :83-92 (layout-geometry bindings: orientation/
  stacking/tiling/float/swap/maximize/fullscreen/resize; no rotate/mirror/
  master verb) + src/input/actions.rs:962-976 (`ToggleOrientation`/
  `Orientation` dispatch to `update_orientation`) +
  src/shell/layout/tiling/mod.rs:2089-2130 (`update_orientation` flips the
  focused parent group's axis with proportional size rescale, no focus write)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (parent-axis toggle only)
- `S-cos-model` cosmic-comp:src/shell/layout/tiling/mod.rs:150-162
  (`Data` holds Group/Mapped nodes only)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (no master concept in the tiling model)
- `S-cos-wslay` cosmic-comp:cosmic-comp-config/src/workspace.rs:8-45
  (`WorkspaceConfig.workspace_layout` is a single global Vertical/Horizontal
  value) + src/input/mod.rs:1230-1231 and src/input/actions.rs:215-219
  (the value drives workspace-navigation gesture/direction mapping, not a
  tiled algorithm) @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (no runtime per-workspace layout-select verb; tiling-enable scope lives
  in `S(S-cos-ctl-tile)`)
- `S-hyp-lay`
  Hyprland:src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:676-702
  (`layoutmsg` dispatch: togglesplit/swapsplit/rotatesplit/movetoroot) +
  :774-868 (`toggleSplit` flips the parent `splitTop`; `swapSplit` exchanges
  the parent's children; `rotateSplit(angle)` flips the axis with the
  angle-conditional swap; `moveToRoot` swaps the node toward the root and
  returns false at the root; none writes focus) + :28-61 (immediate
  recalculation derives the axis from parent geometry at shipped
  preserve_split/smart_split/precise_mouse_move=false defaults, overriding
  the explicit toggle/rotation bit)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-layout` Hyprland:src/config/values/ConfigValues.cpp:179
  (`general:layout` is the global default layout value) +
  src/layout/supplementary/WorkspaceAlgoMatcher.cpp:30-35 (registered tiled
  algorithms: dwindle/master/scrolling/monocle) and :106-142
  (`tiledAlgoForWorkspace` prefers a workspace rule's layout override;
  `updateWorkspaceLayouts` switches a workspace's tiled algorithm on mismatch) +
  src/layout/algorithm/Algorithm.cpp:196-221 (`updateTiledAlgo` re-admits
  existing tiled targets in order with focus restore) +
  src/layout/algorithm/tiled/master/MasterAlgorithm.cpp:46-82 (admission
  placement) and src/config/values/ConfigValues.cpp:784-786 (shipped
  master defaults: `new_status=slave`, `new_on_top=false`,
  `new_on_active=none`, so re-admits append in order)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (per-workspace ownership plus order-preserving switch legs)
- `S-bsp-type` bspwm:doc/bspwm.1.asciidoc:442-443 (`node -y/--type` sets or
  cycles the splitting type of the selected node) + src/tree.c:193-203
  (`set_type` flips `split_type` with constraint rebuild, no focus write)
  @e11eff4 for the doc path,
  @e11eff4cb3333216ad03c815609a4ed79e08929c for src
- `S-bsp-rot` bspwm:doc/bspwm.1.asciidoc:448-452 (`node -R/--rotate`
  90|270|180, `-F/--flip` horizontal|vertical) + src/tree.c:1202-1256
  (`rotate_tree_rec` flips the split type at every level with the
  degree-conditional child swap; `flip_tree` swaps children where the split
  matches; neither writes focus)
  @e11eff4 for the doc path,
  @e11eff4cb3333216ad03c815609a4ed79e08929c for src
- `S-bsp-desklay` bspwm:doc/bspwm.1.asciidoc:505 (`desktop -l/--layout`
  CYCLE_DIR|monocle|tiled) + src/messages.c:765-780 (`set_layout` applies
  per desktop) @e11eff4 for the doc path,
  @e11eff4cb3333216ad03c815609a4ed79e08929c for src
- `S-i3-cmds` i3:parser-specs/commands.spec:23-34 (command dispatch inventory:
  layout/split/focus/move and others; no rotate/mirror/master/promote verb)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
- `S-sway-cmds` sway:sway/commands.c:114-143 (alphabetized runtime command
  table: layout/split/move/swap and others; no rotate/mirror/master verb)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-xmo-master` xmonad:src/XMonad/StackSet.hs:540-556 (`swapMaster`/
  `shiftMaster` make the focused window the master; focus stays with the
  moved item) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
- `S-xmo-wslay` xmonad:src/XMonad/StackSet.hs:157 (each `Workspace` carries
  its own layout) + src/XMonad/Config.hs:137 (`Tall ||| Mirror Tall ||| Full`)
  and :193 (mod-space sends `NextLayout` on the current workspace only)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
- `S-awe-master` awesome:lib/awful/client.lua:460-477 (`getmaster` reads the
  first visible client; `setmaster` moves the client to the primary section
  via repeated `swap`, which writes no focus)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
- `S-awe-tileleft` awesome:lib/awful/layout/suit/tile.lua:328-337
  (`tile.left` runs the tile algorithm mirrored via `do_tile "left"`) and
  :370-372 (plain `tile` aliases the right variant) +
  awesomerc.lua:83-97 (shipped layout list includes floating, tile,
  tile.left, and others) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
- `S-qti-wslay` qtile:libqtile/group.py:15-45 (each group keeps its own
  `layouts` list plus `current_layout` index) and :77-90 (`layout`
  property/setter over that index) +
  libqtile/core/manager.py:1274-1302 (`next_layout`/`prev_layout` with an
  optional group name target the named group's index)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-nir-float` niri:niri-ipc/src/lib.rs:811-819 (`ToggleWindowFloating`
  moves one window; no workspace toggle verb) +
  src/layout/workspace.rs:55,660-677 (`floating_is_active` derives from
  admission/focus, not a command)
  @ed22699d99462f61ab171472d3ea67e844ea580d
- `S-nir-acts` niri:niri-ipc/src/lib.rs:194-946 (full `Action` enum:
  column/window focus, moves, consume/expel, width presets, tabbed display,
  float/floating-focus, workspace moves; no orientation/rotate/mirror/
  master/layout-select/scroll-step/split-preselect verb)
  @ed22699d99462f61ab171472d3ea67e844ea580d
- `S-nir-maxfs` niri:src/layout/workspace.rs:1288-1351
  (`set_fullscreen`/`toggle_fullscreen` with floating-restore memory) and
  :1353-1416 (`set_maximized`/`toggle_maximized` from the column pending
  flag, idempotent clear, unmaximize-into-floating) and :649-669
  (pending-maximized/fullscreen tiles open in the scrolling layout; new
  focus is fenced only against active fullscreen) and :644-648
  (`add_tile` seeds `restore_to_floating` from the computed floating
  boolean at admission) and :896-905
  (configure maps Fullscreen to view size, Maximized to working-area
  size) + src/layout/scrolling.rs:2869-2929 (`set_fullscreen`/
  `set_maximized` set the column pending flags, extracting a multi-tile
  column first; entry writes only the target column) + src/handlers/xdg_shell.rs:466-477,550-559,696,770 and
  src/handlers/mod.rs:551-591 (client maximize/fullscreen requests route
  into the same setters, mapped and unmapped) + src/input/mod.rs:1690-1700
  (`MaximizeColumn` is full-width, `MaximizeWindowToEdges` drives the
  native toggle) + src/layout/mod.rs:625-632 (Smart activation evaluates
  the supplied fullscreen fence rather than always declining focus)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (native maximized/fullscreen state; width actions are distinct)
- `S-nir-flttoggle` niri:src/layout/workspace.rs:1418-1468
  (`toggle_window_floating` moves the tile scrolling<->floating via
  `remove_tile`/`add_tile` with no maximize, clear, or refusal branch;
  floating arrival activates only the requested target) and :1403-1416
  (`toggle_maximized` reads the column pending flag; floating windows
  cannot be maximized) + src/layout/floating.rs:425-440 (floating
  admission resolves a non-normal size via the stored floating size else
  `(0,0)`) + src/layout/scrolling.rs:1062-1090 (`remove_tile` drops the
  tile with its column; sole-tile removal drops the whole column with
  its pending flag)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (plain tile-move toggle path; settled frame is client-driven)
- `S-nir-wscarry` niri:src/layout/tests.rs:3708-3725
  (`MoveColumnToWorkspace` keeps the column Maximized after transfer and
  unfullscreen) and :3728-3750 (`MoveWindowToWorkspace` drops the
  column-held flags so the window arrives Normal; FIXME documents the loss) and
  src/layout/monitor.rs:920-972 (`move_column_to_workspace` moves the
  whole active column object, landing via target `add_column`) and
  src/layout/scrolling.rs:999-1015 (`add_column`: index defaults to
  active+1, 0 on an empty strip) and src/layout/workspace.rs:753-768
  (`add_column` inserts at the target default index, active+1 or 0 when
  empty)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (column-send retains overlay and lands after the target active;
  window-send strips to Normal)
- `S-hyp-wsmove-fs` Hyprland:src/state/workspace/PlacementController.cpp:301-329
  (whole-workspace monitor reassignment; floating reposition plus fullscreen
  setBox to the new monitor box) and :316-317 (fullscreen branch)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (carries fullscreen; maximized members move as members with no separate
  gate traced; no refusal)
- `S-hyp-pinstay` Hyprland:src/state/workspace/PlacementController.cpp:301-306
  (pinned members are reassigned to the next workspace on the old monitor,
  never carried with the moved workspace)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-sway-wsmove-fs` sway:sway/tree/workspace.c:1131-1161
  (workspace_move_to_output detach/attach with source refill, no overlay
  gate) + sway/tree/arrange.c:310-316 (fullscreen container set to output
  geometry on arrange)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (fullscreen carry; no maximize state claimed)
- `S-i3-wsmove-fs` i3:src/workspace.c:1115-1136
  (workspace_move_to_output detach/attach with floating coordinate fix, no
  overlay gate) and :446-457 (workspace_show CF_OUTPUT fullscreen handling)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (fullscreen carry; no maximize state claimed)
- `S-nir-wsmove` niri:src/layout/mod.rs:3452-3525
  (move_workspace_to_output_by_id whole-workspace remove/insert, activation
  only when moved-active, no overlay gate) +
  src/layout/workspace.rs:508-535 (set_output re-enters all windows on the
  new output)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (members carried with the workspace; no overlay-specific refusal traced)
- `S-cos-wsmove-fs` cosmic-comp:src/shell/workspace.rs:589-635
  (Workspace::set_output moves tiling plus floating layers, all mapped,
  minimized, and active fullscreen surfaces to the new output)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (carries mapped plus fullscreen; maximized moves as a mapped member, no
  separate gate claimed)
- `S-pap-acts` PaperWM:keybindings.js:191-238 (switch/move-as-swap verbs) +
  :240-344 (scratch/slurp/barf/maximize-width/fullscreen/focus-mode/
  open-position; no orientation/rotate/mirror/master/layout-select/
  workspace-float verb) + prefs.js:18-69 (settings UI pages
  general/workspaces/keybindings/winprops/advanced/about: binding editing
  only, no first-run/preset/prompt, settings-race, Keep/staging/Force/
  draft, or preimage writers) @8bf6dd264f60d6c0c402b63df7b424b888959a48
- `S-pap-switcher` PaperWM:keybindings.js:93-99 (`live-alt-tab` /
  `live-alt-tab-backward` / scratch variants registration) +
  liveAltTab.js:43-61 (`_getWindowList`: `NORMAL_ALL` tab list minus
  scratch, narrowed to the active workspace only when the external GNOME
  `current-workspace-only` setting asks) and :146-190 (accept runs the
  shell popup `_finish` then `focus_handler` with no take/move; the
  workspace-switch effect rides the unpinned shell activation path) +
  schemas/org.gnome.shell.extensions.paperwm.gschema.xml:12-28
  (shipped Alt+Tab/Super+Tab defaults) + tiling.js:4597-4680
  (`focus_handler` is space-local: viewport/reorder within the window's
  own space, no cross-space move)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (native live-alt-tab list verb exists; workspace scope follows the
  external GNOME setting with no pinned default; membership unchanged,
  workspace-switch effect on the unpinned shell path stays TBD)
- `S-pap-unmov` PaperWM:tiling.js:1391-1398 (layout skips placement while
  easing and for fullscreen/maximized windows: `unMovable` returns early,
  leaving the frame alone)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (overlay-frame stability leg; focus/switch verbs stay TBD)
- `S-pap-widthmax` PaperWM:tiling.js:3514-3526 (maximize events convert
  to width-maximize when `maximize-within-tiling` holds: unmaximize,
  restore last layout frame, `toggleMaximizeHorizontally`) and :4155-4162
  (admission converts native-maximized newcomers the same way) and
  :4794-4830 (width toggle with `unmaximizedRect` memory; full work-area
  width at the shipped 100 percent) + schemas/org.gnome.shell.extensions.
  paperwm.gschema.xml:630-633 (width percent default 1.00) and :635-638
  (`maximize-within-tiling` default true)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (native maximize never overlays at shipped defaults)
- `S-pap-fsframe` PaperWM:tiling.js:3678-3685 (position updates skipped
  while fullscreen or fullscreen-locked) and :3793-3825 (fullscreen exit
  restores the saved frame and clears it; `saveFullscreenFrame` records
  frame plus tiled width)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (native fullscreen frame memory; entry itself is the Meta API flag)
- `S-pap-appresize` PaperWM:tiling.js:3665-3675 (`addResizeHandler` wires
  `size-changed` through a `RESIZE`-phase `later_add` into `resizeHandler`)
  and :3686-3783 (`resizeHandler`: nulls `_targetWidth/_targetHeight`,
  tiled windows queue a column relayout while non-tiled windows take
  `nonTiledSizeHandler`; no minima/hint branch) and :616-624 (post-request
  frame re-read: X11 sync converges, Wayland async may not, actuals feed
  layout) @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (app-owned resize/hint reaction is tile-authoritative relayout recomputed
  from preferredWidth/client frame; exact native settlement is client-timing
  per `S(S-pap-layout)`)
- `S-pap-swap` PaperWM:tiling.js:1063-1094 (`swap` exchanges model
  positions with the directional neighbor, then layouts with no
  unmaximize branch; selection unchanged)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (move-left/right verb path; registration is `space.swap`)
- `S-pap-take` PaperWM:tiling.js:5407-5420 (`takeWindow` removes the
  window from its space for navigator cross-space moves) and :5395-5400
  (`moveDown/UpSpace` via `selectSequenceSpace(..., true)`) and
  :5528-5555 (destroy finalization: insert into the selected space,
  make selectedWindow, then `Main.activateWindow`: shipped completion
  follows) + keybindings.js:189 (`take-window` registration) and
  :172-173 (move-down/up-workspace) + schemas/org.gnome.shell.extensions.paperwm.gschema.xml:68-75
  (Super+Ctrl+Page_Down/Up move defaults) and :173-175 (Super+t take
  default)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (single-window cross-space transfer verb; no whole-column verb;
  extension overlay carry follows the existing-window admission leg;
  native overlay survival stays TBD where stated;
  shipped completion follows; legacy inactive-space no-steal per `S(S-pap-ins)`
  is a different journey, not the shipped-send outcome)
- `S-kar-acts` karousel:src/lib/keyBindings/Actions.ts:6-60 (focus verbs) +
  :86-175 (window/column move verbs) + :176-260 (`windowToggleFloating`
  per-window only, column move/stacked/width/preset verbs) + :252-544
  (scroll/screen-switch/tail/desktop-move verbs) +
  src/lib/keyBindings/definition.ts:12-313 (shipped binds are
  focus/move/toggle-floating/column/width/scroll/screen/tail/num-column
  binds only) + src/main/main.ts:1-11 (entry constructs the World;
  enable/disable is the script lifecycle)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (full definition/Actions inventory carries no close/restart/reload/
  persist/session/first-run/preset/prompt/settings-race/tray/
  workspace-default/staging/Save/Apply/Force/preview/draft/
  preimage/restore/switcher verb; no rotate/mirror/master/orientation/
  layout-select/workspace-toggle verb)
- `S-kar-switcher` karousel:src/lib/keyBindings/definition.ts:12-60
  (shipped focus binds move within the grid; no switcher/listing bind) +
  src/lib/world/clientState/Tiled.ts:222-246 (`skipSwitcher` KWin
  switcher-exclusion flag, written and restored around tiling) +
  src/lib/config/definition.ts:145-150 (`skipSwitcher` default false)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (no switcher verb in the Actions inventory; KWin owns listing and
  activation; karousel only opts windows out via `skipSwitcher`,
  default included)
- `S-kar-maxfs` karousel:src/lib/world/clientState/Tiled.ts:66-70
  (`maximizedAboutToChange` observed) and :174-184 (`fullScreenChanged`
  observed; untileable-after-exit floats) and :222-242 (tiling admission
  force-unmaximizes; fullscreen kept with keepAbove) + src/lib/layout/
  Window.ts:91-126 (`restoreToTiled` clears both when unfocused;
  maximize/fullscreen handlers set `skipArrange` with layering) +
  src/lib/layout/Grid.ts:195-201 and Column.ts:335-341 (focusing another
  window restores the old one to tiled) + src/lib/config/definition.ts:
  141-147 (`reMaximize` default false) and :175-195 (`tiledKeepBelow`
  default true)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (overlay membership kept with arrange skipped; focus change restores)
- `S-kar-tile` karousel:src/lib/world/Clients.ts:7-16 (`canTileEver`:
  moveable and resizeable, or fullscreen; popups and prohibited classes
  excluded) and :48-53 (`isFullScreenGeometry` coverage test)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (shapeability decides tileability, not monitor coverage)
- `S-ours-planops` shared Engine operation inventory:
  omnitiler:crates/tiler-core/src/session/ops/mod.rs:8-15 (families:
  drag/float/focus/lifecycle/move/resize/workspace only) +
  crates/tiler-core/src/directional.rs:236 (`MoveOperation` is
  directional-only) and :517 (`FocusPlan` is directional leaf-or-Edge only);
  KDE adapter dispatch on the world route: omnitiler:kwin/src/world-report.ts
  (directional focus/move, workspace select/send/migrate, tiling commands,
  plus generic `operation` commands);
  Windows dispatch: omnitiler:crates/tiler-windows/src/snapkey.rs:104-107
  (`SnapOp`: Focus/Move only) and :427-430 (`WorkspaceOp`: Select/Send only)
  @a77dd341f311da080ba94347c82a34d1d1c57893
  (no layout/orient/rotate/mirror/master/layout-select verb in any of the three
  layers)
- `S-cos-ws` cosmic-comp:src/input/actions.rs:186-211
  (`Workspace(key)` index activate, `LastWorkspace` targets `len-1`) and
  :212-290 (`NextWorkspace`/`PreviousWorkspace` with `workspace_wraparound`
  plus output fallback) and :292-346 (`MoveTo`/`SendToWorkspace` index
  mapping with follow vs stay, plus Last variants) and :348-530
  (`MoveTo`/`SendToNextWorkspace` active+1 and `MoveTo`/
  `SendToPreviousWorkspace` active-1, wraparound cycle else output
  fallback) and :35-42 (`propagate_by_default`: shortcut-path propagate
  true only for `Focus`/`Move`, so relative sends carry propagate=false)
  and :684-740 (`MigrateWorkspaceToOutput` migrates the active
  workspace, activates it there, then switches output; Next/Previous
  migrate actions are deprecated no-ops) and :1142-1200
  (`to_next_workspace`/`to_previous_workspace` wrap-or-stay) +
  src/shell/mod.rs:525-583 (`set.activate` refuses idx past the end,
  `activate_previous` gesture-only) + src/shell/mod.rs:652-704
  (`ensure_last_empty` adds only when the last is occupied/pinned) +
  cosmic-comp-config/src/workspace.rs:14-20 (`workspace_wraparound`
  defaults true) @3d55cba06c9cf6f27609cdefb520f7857dba20af for the
   compositor paths (config path per `S(S-cos-wslay)` repo split)
   (no history-toggle verb in the workspace action inventory)
- `S-cos-wsmig` cosmic-comp:src/shell/mod.rs:1032-1062
  (`migrate_workspace`: Global-mode / same-output / unknown-target-set
  refusals; otherwise the same workspace object is removed, set to the
  new output, and inserted after the target active; no emptiness gate)
  and :713-728 (`post_remove_workspace`: an emptied set gains one fresh
  workspace, else the active falls to the last entry with Active state)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-outremove` cosmic-comp:src/shell/mod.rs:937-1029
  (`remove_output`: layer close, first-remaining output takes all kept
  workspaces via `set_output` plus `refresh`, empty/token/pin-gated
  `can_auto_remove` drops, sticky/minimized layers merge; backup set
  only when no outputs remain) and :867-935 (`add_output`: backup-set
  restore else fresh set, `prefers_output` workspaces reclaimed) and
  src/shell/workspace.rs:498-500 (`can_auto_remove`: empty plus no
  token/pin) and :589-645 (`set_output` carries tiling/floating layers
  plus `output_stack` return affinity)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (evacuation destination plus return affinity; exact focus node needs
  fixture history, settled visuals live)
- `S-cos-wskeys` cosmic-comp:data/keybindings.ron:38-47
  (Super+Shift+1..9 `MoveToWorkspace`, Super+Shift+0
  `MoveToLastWorkspace`; no `SendToWorkspace` binding) and :57-64
  (Super+Shift+Ctrl arrows/hjkl `MoveToPrevious/NextWorkspace`) +
  justfile:17-18 (keybindings.ron installs as the
  CosmicSettings.Shortcuts defaults)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
- `S-cos-wssingle` cosmic-comp:src/shell/mod.rs:3181-3194
  (`move_current` refuses with `InvalidWorkspaceIndex` when the send stays
  on the same output, targets the last workspace, starts from the adjacent
  workspace, the source holds a single window, and the target is empty;
  both follow and stay refuse before any transfer)
  @3d55cba0
- `S-hyp-ws`
  Hyprland:src/config/shared/actions/ConfigActions.cpp:170-198
  (`back_and_forth` resolve: re-invoking switch-to-current goes to the
  timeline previous, `=2` per-monitor) and :380-436 (`moveToWorkspace`
  follow focuses the mover, silent refocuses the source) and :1016-1083
  (`changeWorkspace`, cross-monitor focuses the focus candidate) and
  :1107-1117 (`moveToMonitor` whole-workspace verb) +
  src/state/workspace/PlacementController.cpp:259-295 (`moveWorkspaceToMonitor`
  gap plug: non-active moves change neither view; an active-leg move plugs the
  source with the first enumerated remaining non-special workspace, else creates
  the first free number, via `changeWorkspace`) and :302-361 (carryFocus
  destination activation swaps the destination active directly with no
  window-focus write) +
  src/output/Monitor.cpp:1398-1423 (workspace switch: remembered feeds
  focus only when floating, else the fullscreen cover; `follow_mouse=1`
  pointer-hit wins before the focus candidate; pointer fixture
  unspecified) + src/state/workspace/Resolver.cpp:181-205 (`prev` is
  MRU-history previous, `next` is numeric+1) +
  src/desktop/history/WorkspaceHistoryTracker.cpp:40-110 (MRU timeline
  track plus previous lookup; `gc` runs inside the previous lookups and keeps
  the second-position entry while pruning older dead ones) +
  src/workspace/HLWorkspace.cpp:122-135
  (`getLastFocusedWindow`/`rememberFocusedWindow`) +
  src/workspace/HLWorkspace.cpp:134-143 (`getFocusCandidate` prefers
  last-focused, else top-left, else first; `follow_mouse=0` variant) +
  src/workspace/RegularWorkspace.cpp:36-53 (persistent self-hold, rule-set) +
  src/state/workspace/State.cpp:60 (destroy erases the weak state entry) +
  src/desktop/DesktopTypes.hpp:25-31 (state/history refs are weak; windows
  hold strong workspace refs; monitors hold the active workspace) +
  src/config/values/ConfigValues.cpp:380 (`input:follow_mouse`
  default 1) + src/config/values/ConfigValues.cpp:615 (`workspace_back_and_forth`
  default 0 off) @19fb395d45314960e6f79f17994a84094f1cd4f6
  (numbered IDs stable, duplicates refused, no renumber path; empty lifetime is
  refcount with those holds plus the persistent self-hold only, no ordinary
  empty-destroy verb per `S(S-hyp-close)`)
- `S-hyp-wskeys` Hyprland:example/hyprland.lua:277-278 (mainMod+n
  workspace focus, mainMod+SHIFT+n `window.move({workspace=i})` with
  follow absent) + src/config/lua/bindings/LuaBindingsDispatchers.cpp:813-818
  (workspace block: follow absent so silent is false, shipped move
  follows; `follow=false` stays; the :826-829 analog is the monitor
  block) @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-hyp-mondir` Hyprland directional monitor resolution for workspace moves:
  Hyprland:src/config/lua/bindings/LuaBindingsDispatchers.cpp:1193-1213
  (`moveworkspacetomonitor`/`movecurrentworkspacetomonitor` resolve the
  monitor string through `configString`) +
  src/state/MonitorQueryCore.cpp:133-195 (`directionLookup`: Up needs
  edge-stick of the reference top against the candidate bottom, then keeps
  the longest shared x-intersection) and :196-205 (`fromConfigString` maps
  direction words through that lookup) + src/macros.hpp:37 (`STICKS`
  within 2px) + src/helpers/MiscFunctions.cpp:109 (`isDirection` words)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (directional whole-workspace target resolution; displaced views and exact
  focus are separate TBD legs per `S(S-hyp-ws)`)
- `S-bsp-ws` bspwm:doc/bspwm.1.asciidoc:52 (`CYCLE_DIR` next|prev) and
  :215 (DESKTOP_SEL grammar) and :233-234 (`last` is the previously
  focused desktop) and :418-422 (`node -d/-m` desktop/monitor send with
  `--follow`) and :486-512 (`desktop -f/-a/-m/-s/-l`, whole-desktop move
  via `-m`) and :514 (explicit `desktop -r` removal) +
  src/desktop.c:39-74 (`activate_desktop` show/hide) and :75-110
  (`find_closest_desktop` monitor-crossing wrap) and :167-181
  (`transfer_desktop` sticky-count/unlink/insert head) and :270-274
  (tail-append desktop insert) and :182-235 (desktop
  transfer with follow) and :336-360 (explicit removal only) +
  src/messages.c:656-803 (`desktop -f/-a/-m/-s/-b/-l/-n/-r` verbs plus
  absent-selector failure and explicit-only removal) and
  src/monitor.c:459-558 (RandR wired tracking with
  `remove_disabled`/`remove_unplugged` gates) and src/settings.h:67-69
  (`REMOVE_DISABLED`/`REMOVE_UNPLUGGED`/`MERGE_OVERLAPPING` default
  false) and src/types.h:283-293 (`desktop_t.focus` memory) +
  `S(S-bsp-close)` (focus_node history fallback)
  @e11eff4 for the doc path,
  @e11eff4cb3333216ad03c815609a4ed79e08929c for src
- `S-bsp-wskeys` bspwm:examples/sxhkdrc:83-85 (super+shift+n
  `node -d '^{1-9,10}'` with no `--follow`: shipped send stays;
  `--follow` alternate per `S(S-bsp-send)`) @e11eff4
- `S-bsp-wsstay` bspwm:src/desktop.c:173-178 (`transfer_desktop` counts
  sticky nodes only when the moved desktop was active, then unlinks) and
  :186-188 (insert on the destination, destination sticky-count takes them)
  and :207-213 (stickies moved off the transferred desktop back to the
  source's shown remainder, else to the destination's shown desk)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (sticky stays on the source output's shown desktop; only with no source
  desktop shown does it land on the destination's shown desk)
- `S-bsp-wshist` bspwm:src/history.c:114-121 (`history_remove` with a NULL
  node drops every entry locating the desktop) + src/desktop.c:188
  (transfer drops the moved desktop's entries and adds none for it)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (the migrated desktop leaves the `last` history walk)
- `S-bsp-mondir` bspwm directional monitor resolution for desktop moves:
  bspwm:src/monitor.c:408-427 (`nearest_monitor`: keeps the minimum
  `boundary_distance` among direction-side monitors matching the
  selectors, no ambiguity gate) + src/geometry.c:49-71
  (`boundary_distance` per-direction edge gap) and :72-155 (`on_dir_side`:
  HIGH tightness plus a shared x/y range) + src/settings.c:108 (default
  HIGH tightness) @e11eff4cb3333216ad03c815609a4ed79e08929c
  (directional whole-desktop target resolution; list order untraced, so a
  distance tie keeps the first enumerated)
- `S-i3-ws` i3:src/workspace.c:131-160 (`workspace_get` creates on
  demand) and :438-505 (`workspace_show` records the previous name,
  focuses the descended remembered focus, closes the empty old
  workspace) and :581-660 + :666-880 (`workspace_next`/`prev` wrap via
  first/last fallback) and :892-913 (single previous-name
  back-and-forth) and :1059-1150 (`workspace_move_to_output` whole-
  workspace detach/attach with source refill and displaced cleanup) +
  src/commands.c:1068-1115 (`move workspace to output` dispatch) and
  parser-specs/commands.spec:165-183 (next/prev/back_and_forth grammar)
  and :293-296 (relative move-to-workspace) and :375-433 (relative and
  whole-workspace move grammar) and src/randr.c:860-918 (`move_content`:
  disconnect evacuates to the first output, empty-unfocused destroyed,
  floating fix, focus follows the moved focused con, docks moved) and
  :456-525 (`init_ws_for_output`: reconnect assigns by assignment else
  first-assigned else first-free, no identity/affinity store, no previous
  consult) and :80-99 (`get_first_output`: primary else first active)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (disconnect evacuates to the first remaining output; reconnect is fresh
  assignment, never return affinity)
- `S-i3-wsdir` i3:parser-specs/commands.spec:445-453 (`MOVE_WORKSPACE_TO_OUTPUT_WORD`
  takes `output = word`, admitting directional words) + src/output.c:33-50
  (`get_output_from_string` maps left/right/up/down via
  `get_output_next_wrap`) + src/randr.c:225-246 (`get_output_next_wrap`:
  closest in direction else farthest-opposite wrap) and :259-319
  (`get_output_next`: closest by coordinate with x/y-overlap gate,
  first-enumerated ties) + src/commands.c:1023-1115
  (`user_output_names_find_next` matches explicit names cyclically) +
  src/move.c:266-269 (`tree_move` refuses `CT_WORKSPACE` on the window
  directional path only)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (the `move workspace to output` word resolves directionally at runtime;
  window moves never substitute)
- `S-i3-stickyshow` i3:src/workspace.c:562-567 (`workspace_show` tail
  pushes floating sticky windows to the now-visible workspace after
  focusing) + `S(S-i3-sticky)` push filter (tiling cons skipped)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (show-time sticky re-home targets the shown workspace only)
- `S-xmo-ws` xmonad:src/XMonad/StackSet.hs:182 (`Stack.focus` per
  workspace) and :231-260 (`view` keeps each workspace's focus,
  unknown tags return unchanged) and :262-275 (`greedyView` display
  swap across screens) and :572-600 (`shift`/`shiftWin` explicit-tag
  transfer only) + src/XMonad/Config.hs:50-57 (workspace list is static
  configuration) + `S(S-xmo-ctl)` (no back-and-forth, relative-switch,
  or relative-send verb in the profiled inventory)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
- `S-xmo-wskeys` xmonad:src/XMonad/Config.hs:230-234 (mod-[1..9]
  `W.greedyView` switch, mod-shift-[1..9] `W.shift` send: shipped send
  stays; `W.greedyView . W.shift` composition views after the shift)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
- `S-sway-ws` sway:sway/tree/workspace.c:177-200 (`workspace_create`)
  and :299-334 (`workspace_consider_destroy` drops empty non-active)
  and :548-660 (`workspace_prev`/`next` wrap via last/first fallback)
  and :700-745 (`workspace_auto_back_and_forth` plus `workspace_switch`
  focusing the seat focus-inactive node) and :1131-1161
  (`workspace_move_to_output` detach/attach with source refill and
  displaced consider-destroy) + sway/commands/workspace.c:180-230
  (switch incl create plus back_and_forth) and sway/commands/move.c:419-480
  (`move to workspace` next/prev/number/back_and_forth) and :630-665
  (`move workspace to output` acts on the handler-context active
  workspace) and :664-666 (focus to the moved workspace's focus-inactive
  node) + sway/commands.c:182-199 (handler workspace from the matched
  container) and :240-300 (criteria loop sets handler context per match)
  + sway/criteria.c:453-471,514-524 (workspace-regex match over all
  outputs incl hidden) + sway/input/seat.c:1098-1110 (per-seat
  `prev_workspace_name` recorded on workspace change only)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
- `S-sway-wsdir` sway directional resolution for workspace moves:
  sway:sway/commands/move.c:27-80 (`output_in_direction`: up/down/left/
  right resolve through the wlroots adjacent output, else the
  farthest-opposite fallback, else a name/id lookup; NULL only when
  none) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (wlroots `wlr_output_layout_adjacent_output` is outside the pinned
  source, so a two-candidate tie-break stays TBD)
- `S-sway-stickypull` sway:sway/input/seat.c:1209-1221 (seat
  workspace-focus change moves sticky floaters to the newly focused
  workspace; the move path writes raw focus only, never this path)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (sticky re-home runs on switches, never inside workspace moves)
- `S-sway-wsactive` sway:sway/desktop/output.c:76-85 (output active is
  queried: seat-stack child else list items[0]) +
  sway/input/seat.c:1412-1432 (stack walk matches on direct parent) +
  sway/tree/node.c:103-121 (a workspace node's parent follows its output
  ownership) + sway/input/seat.c:1177-1188 (focus pushes the workspace
  node onto the stack) + sway/tree/output.c:333-345 (attach re-points
  the workspace's output ownership)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (the shown workspace follows ownership queries without any focus
  write; the moved workspace node qualifies on the new output)
- `S-qti-ws` qtile:libqtile/config.py:578-625 (`set_group` assigns or
  cross-screen swaps, saving `previous_group`) and :626 (`_toggle_group`
  falls back to the previous group) and :715-735 (`next_group`/
  `prev_group` modulo wrap, `toggle_group` previous-or-named) +
  libqtile/group.py:110-145 (`layout_all` focuses the remembered
  `current_window` on the current screen) and :146-160 (`set_screen`
  show/hide) and :363-395 (`toscreen` pull with toggle) and :429-434
  (`get_next_group`/`get_previous_group` modulo) +
  libqtile/backend/x11/window.py:1946-1960 (`togroup` takes explicit
  group names only) + `S(S-qti-wsdef)` (static groups 1-9)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-wskeys` qtile:libqtile/resources/default_config.py:77-98
  (mod+shift+n `togroup(i.name, switch_group=True)`: shipped send
  follows; commented `togroup(i.name)` stays alternate)
  @83c697a5621306c3586efca31867efcfa0482e2d
- `S-qti-screen` qtile:libqtile/core/manager.py:389-400
  (`get_available_group` config-order scan, screen-affinity gated, no
  identity store) and :448-513 (`_process_screens` re-keys outputs to
  screens) and :516-533 (`reconfigure_screens` re-runs the assignment
  and `hide`s groups whose screen left, contents retained) and :177-178
  (shipped `reconfigure_screens` subscribes the hook) +
  libqtile/config.py:578-623 (`set_group` same-group early return,
  cross-screen swap, else assign-plus-hide) +
  libqtile/resources/default_config.py:199 (shipped default on) +
  libqtile/backend/x11/core.py:831-832 (`ScreenChangeNotify` fires
  `screen_change`) and libqtile/backend/wayland/core.py:379-380
  (output change fires `screen_change`)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (host output removal never merges groups; reconnect is fresh
  config-order assignment, not stored return affinity)
- `S-awe-ws` awesome:lib/awful/tag.lua:489-532 (`tag.history.update`
  per-screen MRU) and :534-566 (`history.restore` defaults to the
  previous-set toggle) and :1569-1586 (`viewidx` cycles, so viewnext/
  viewprev wrap) and :1637-1660 (`view_only` selects plus history
  update) and :607-645 (`set_screen` moves the tag plus all member
  clients, restoring old-screen history) and :409-485 (explicit
  `tag.delete` only) and :1913-1948 (screen `removed` default: `request::screen`
  salvage chance, `removal-pending` plus `request::tag`, fallback delete into
  the first tag of a remaining screen, history cleared; no affinity store) + static tags 1-9 per `S(S-awe-default)` and
  tag-switch refocus per `S(S-awe-hist)`
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
- `S-awe-wsdir` awesome directional screen lookup is view-only:
  awesome:lib/awful/screen.lua:134-155 (`get_next_in_direction`) and
  :156-171 (`focus_bydirection` moves the pointer plus screen focus,
  never a tag) + lib/gears/geometry.lua:95-168 (`is_in_direction` plus
  edge distance plus `get_in_direction`)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (no directional whole-tag migration verb; `tag.screen` per `S(S-awe-ws)`
  takes an explicit screen)
- `S-nir-ws` niri:src/layout/monitor.rs:442-495 (activate stores the
  previous id; remove/insert/append/cleanup write no previous id, only
  activate writes at :455 plus gesture :2074-2076 with save/restore at
  :1261-1264/:1287-1290) and :547-621 (`add_column`/`add_tile` insert the
  next empty bottom spare plus top spare under the option, activate only
  when asked) and :650-679 (`clean_up_workspaces` drops empty
  non-active non-trailing workspaces) and :721-745 (`insert_workspace`
  clamps past the trailing empty, activates only when asked) and
  :750-785 (`append_workspaces` inserts before the trailing empty,
  keeps empty focus, clears the switch) and :800-900 (`move_to_workspace`
  up/down clamp plus follow activation; cleanup only when no switch
  animation) and :920-972 (`move_column_to_workspace` moves the whole
  active column object, landing via target `add_column`) and :960-1010
  (switch up/down clamp at the ends) and :1002-1030
  (`previous_workspace_idx`, `switch_workspace_previous`,
  `switch_workspace_auto_back_and_forth`, out-of-range switch clamps
  to last) and :293-351 (`Monitor::new` trailing empty plus
  `ws_id_to_activate` select) + src/layout/mod.rs:353
  (`last_active_workspace_id` stored on monitor removal) and :760-876
  (`add_output` moves preferred workspaces back plus stored last-active
  restore) and :878-944 (`remove_output` stores last-active, preferred
  fallback to primary, `append_workspaces` distribute) and :2145-2169
  (relative-move dispatch, `focus=true` Smart else No) and :2324-2344
  (keyboard focus resolves to the active workspace's active window) and
  :3452-3520 (`move_workspace_to_output_by_id` whole-workspace
  remove/insert, activation only when moved-active) + src/layout/workspace.rs:49-55
  (each workspace owns its scrolling state, floating space, and active
  flag) and :479-486 (`find_preferred_output` via `original_outputs`)
  and :508-535 (`set_output` preserves `original_outputs`) + src/input/mod.rs:1329-1366
  (`MoveWindowToWorkspace` reference plus `focus` Smart/No) and
  :1437-1460 (`MoveColumnToWorkspace` reference plus `focus`) and
  :2134-2155 (`MoveWorkspaceToMonitorByRef` resolves hidden workspaces
  by reference) + niri-config/src/binds.rs:227-243 (`focus` defaults
  true) + src/input/mod.rs:1536 (`FocusWorkspacePrevious` binding) +
  src/ui/mru.rs:584-592 (MRU UI lists every workspace's windows)
  @ed22699d99462f61ab171472d3ea67e844ea580d
- `S-nir-wskeys` niri:resources/default-config.kdl:528-536
  (Mod+Ctrl+1..9 `move-column-to-workspace`: shipped send moves the
  column and follows under the `focus=true` default) and :471/:539
  (window-only variants are commented alternates, not shipped binds) +
  `focus=false` stays per `S(S-nir-ws)`
  @ed22699d99462f61ab171472d3ea67e844ea580d
- `S-nir-mru` niri:src/ui/mru.rs:577-607 (MRU collect walks every
  workspace's windows, stamps output/workspace flags, sorts by focus
  timestamp, default scope All) and :826-830 (scope filter: All passes
  everything) + src/layout/workspace.rs:451-466 (`windows()` iterates
  scrolling-plus-floating tiles) + niri-config/src/recent_windows.rs:47-57,221-252
  (`recent_windows` defaults on with Alt+Tab/Mod+Tab and Alt+grave
  binds) + src/niri.rs:1090-1118 (`focus_window` via `activate_window`;
  `confirm_mru` focuses the confirmed selection) +
  src/layout/mod.rs:1553-1587 (`activate_window` switches to the
  window's workspace; no window move) and
  src/layout/scrolling.rs:1465-1476 (column/position activation)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (native MRU switcher: default All scope lists every workspace's
  windows; confirm switches to the selection's workspace with
  membership unchanged)
- `S-nir-wsremove` niri:src/layout/monitor.rs:696-719
  (`remove_workspace_by_idx`: removing the last spawns a bottom spare
  first; the active steps to the previous entry; switch cleared, cleanup
  runs) @ed22699d99462f61ab171472d3ea67e844ea580d
  (source refill after a workspace leaves is the previous entry)
- `S-nir-outdir` niri directional output resolution for workspace moves:
  niri:src/niri.rs:3658-3672 (`output_up_of`: full-width vertical-strip
  overlap plus minimum centre-y distance) and :3721-3724 (`output_up`
  from the active output) + src/input/mod.rs:2101-2108
  (`MoveWorkspaceToMonitorUp` dispatch) + niri-ipc/src/lib.rs:781-783
  (directional workspace-move verbs)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (output order untraced, so an equal-distance tie keeps the first
  enumerated)
- `S-pap-space` PaperWM:tiling.js:1096-1127 (`switchLinear` column loop)
  and :2865-2925 (`selectSequenceSpace`: adjacent steps stop at the
  ends, `move` takes the window first) and :3034-3070
   (`selectStackSpace`: MRU-stack steps with wrap) and :3222-3230
  (`removeSpace`) and :2477-2510 (`workspacesChanged` mirrors GNOME
  add/remove) and :462-487 (`activate`/`activateWithFocus` call native
  activate with or without a focus target) and :900-912
  (`selectedWindow` retention) and :2576-2620 (`moveToMonitor` whole-
  space choreography with swap fallback) and :3281-3308 (`mru()`
  live-computed: active plus `NORMAL_ALL` tab-list plus index order;
  `stack` fronted on every workspace switch) and :2427-2438
  (`setMonitors` monitor-map write plus `_updateMonitor`) and :1986-2012
  (`Space.setMonitor` monitor reassignment with geometry) and :2042-2043
  (layout plus `monitor-changed`, no column rewrite) and :2761-2780
  (`_getOrderedSpaces` workspace-index order) and :5361-5366
  (`previous-workspace` / move-previous exports) +
  keybindings.js:153-154 (`previous-workspace` registrations) and
  :153-173 (workspace switch/move actions)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (workspace add/remove is GNOME-owned; loop variants are column-level;
  monitor reassignment re-keys no model so columns/selection carry; stack
  walk inputs are live tab/switch order with host neighbor/index remainder)
- `S-kar-ws` karousel:src/lib/keyBindings/Actions.ts:453-467
  (`columnMoveToDesktop` moves the whole column via `moveToGrid` after
  the target's last column, no desktop switch) + :469-504
  (`columnMoveToNextDesktop`/`columnMoveToPreviousDesktop` stop at the
  desktop ends) + src/lib/keyBindings/definition.ts:299-311
  (`column-move-to-desktop-{}`/`tail-move-to-desktop-{}` bindings,
  tiled-only via `doIfTiledFocused`) + src/lib/layout/Column.ts:20-31
  (`moveToGrid` cross-desktop transfer: Immediate pass when focused else
  None, then client desktops reassigned) + src/lib/layout/Grid.ts:150-170
  (`onColumnAdded` appends, `onColumnRemoved` refreshes
  `lastFocusedColumn`) + :161-185 (left-else-right focus column,
  Immediate/OnUnfocus/None pass, `autoAdjustScroll`) + :195-202
  (`onColumnFocused` tracks `lastFocusedColumn` and scrolls to the
  column) + :82-95 (`getLastFocusedColumn`/`getLastFocusedWindow`
  null-guards) + src/lib/layout/Column.ts:297-329 (`onWindowRemoved`
  above-else-below candidate with Immediate/OnUnfocus pass, focus-taker
  bridge) + src/lib/layout/Window.ts:62-69 (`focus` requests the passer
  when KWin has not focused) + src/lib/world/FocusPassing.ts:1-51
  (Immediate/OnUnfocus/None, 200ms expiry) +
  src/lib/world/World.ts:124-131 (`doIfTiledFocused` gates floats out) +
  src/lib/world/DesktopManager.ts:40-49 (`getDesktopInCurrentActivity`
  plus exactly-1 desktop/activity gate, else float) + :65-67 (grids keyed
  activity|desktop) + src/lib/world/clientState/Tiled.ts:35-45
  (`desktopsChanged`/`activitiesChanged` grid mover) + :211-220
  (`moveWindowToGrid` fresh column at last-focused else last, OnUnfocus
  when focused else None) + src/lib/world/clientState/Floating.ts:40-62
  (tileChanged/frameGeometryChanged only, no desktop mover) +
  src/lib/world/ClientManager.ts:181-188 (`onClientFocused` leaves
  `lastFocusedColumn` when no tiled window resolves) +
  src/lib/layout/Desktop.ts:90-103 (`autoAdjustScroll`/`scrollToColumn`
  to the last-focused column) + src/lib/workspace.ts:27-29 (desktop
  switch only re-arranges) + `S(S-kar-acts)` (no desktop-switch, history,
  or select verb in the inventory)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (desktops and their switching are KWin-native; KWin-side activation,
  switch focus, and desktop inventory/removal stay host-owned per
  `S(S-kwin-scriptact)` + `S(S-kwin-switch)` + `S(S-kwin-desktops)`)
- `S-pan-ws` paneru:src/ecs/workspace.rs:882-1000 (switch: North stops,
  South steps or auto-creates, `First`/`Last` jump, `VirtualNumber`
  spawns the absent strip) and :1040-1120 (relative move with
  `MoveFocus` Follow/Stay; South needs len>1, North stops at 0) and
  :660-835 (`handle_virtual_window_moves`: tab-group carry, mid-strip
  gate else strip-end append, missing-row creation, empty-source Follow,
  Follow/Stay focus) and :125-141 (`PreviousStripPosition` plus
  remembered-window restore guard) and :1120-1145 (center-column fallback
  for never-focused strips) and :1144-1300 (`show_active_workspace` parks
  other strips and restores the remembered strip focus with a raise) and
  :1351-1383 (empty-row reaping, never
  index 0) + src/ecs/layout.rs:970-981 (`tab_group` single-window
  passthrough) + src/config.rs:815-819 (`reap_empty_workspaces` defaults
  off) + src/config.rs:860-866 (`insert_windows_mid_strip` defaults off) +
  src/config.rs:868-872 (`create_virtual_workspace_automatically`
  defaults off) + `S(S-pan-cmds)` (no history verb; `Virtual` is directional)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
- `S-ours-ws` workspace mechanics at this HEAD:
  omnitiler:crates/tiler-core/src/session/ops/workspace.rs:37-47
  (explicit same-output send proposal only) and :92-101 (`Unchanged` /
  `CrossDomainMismatch` / `UnknownDomain` refusals) and :121-192
  (remembered-leaf / focus-MRU / root anchor plus follow-on-commit)
  + crates/tiler-core/src/session/world.rs:574-640 (`remembered_leaf`,
  `focus_stack_fallback`, `updated_last_active` per-domain memory) +
  Windows: crates/tiler-windows/src/tiling_sys.rs:9815 (`workspace_do_select`
  hide/reveal with `already-active` short-circuit) and
  crates/tiler-windows/src/workspace.rs:194-232 (mode pruning leaves
  workspace order intact; output seeding and workspace count) and
  :297-315 (index resolvers incl
  trailing) and :400-415 (`focus_target` prefers `last_focus`, else first
  visible) and crates/tiler-windows/src/snapkey.rs:427-447 (`WorkspaceOp`
  Select/Send, index only) + KDE world route:
  omnitiler:kwin/src/world-native.ts (`resolveDesktop` desktop handle
  resolution) and kwin/src/world-apply.ts (membership/view actuation;
  desktops themselves are Plasma-owned) @60771bd
  (Engine/KDE have no select verb; Windows has index-only Select;
  no history/relative/whole-workspace verb in these inventories)
- `S-cos-minimize` cosmic-comp:src/shell/mod.rs:4369-4410
  (`minimize_request`: sticky vs workspace dispatch into
  `minimized_windows`) and src/shell/workspace.rs:1045-1145 (`minimize`:
  fullscreen branch plus tiling/floating unmap storing
  `MinimizedWindow::Tiling/Floating` restore data) and :1148-1260
  (`unminimize`: fullscreen refocus plus floating remap and tiling old-slot
  `remap` with the stored state) and
  src/shell/layout/tiling/mod.rs:1414-1445 (`unmap_window_internal` with the
  minimizing flag removes the node and reflows siblings) and
  src/shell/focus/mod.rs:108-137 (`is_minimized` filter plus last
  unminimized MRU pick) and src/shell/workspace.rs:502-540
  (`refresh_focus_stack` retains minimized members) and :1538-1543
  (`is_empty` false while any minimized entry exists) and
  src/shell/mod.rs:4411-4458 (`unminimize_request`: workspace tiling/floating
  branches set no focus) and src/shell/focus/mod.rs:771-790 (no unminimized
  MRU/mapped/fullscreen target resolves to none) @3d55cba0
  (tiling unmap/reflow plus stored restore slot; focus leg via the MRU
  skip-minimized filter; sole-minimize retention with focus none)
- `S-hyp-mininv` Hyprland:src/config/shared/actions/ConfigActions.cpp:200-1824
  (dispatcher inventory at pin lists no minimize action) and
  src/config/shared/actions/ConfigActions.hpp:39-111 (window/workspace
  action declarations at pin: close/kill/signal/float/pseudo/pin/fullscreen/
  move/swap/focus/center/cycle/tag/pass/send_key/swap_next/alter_zorder/
  set_prop/group/workspace/monitor/special/exec/exit/reload/submap/dpms verbs;
  no minimize) and
  src/config/lua/bindings/LuaBindingsDispatchers.cpp:1298-1360
  (dispatched names at pin; no minimize name) and
  src/desktop/view/window/WindowBackend.hpp:74-90 (`SBackendStateRequest`
  carries optional minimized) and :131 (`setMinimized` backend virtual) and
  :144 (single `stateRequest` signal) and
  src/desktop/view/window/X11Backend.cpp:190-204 (commit builds the request
  from `requestsMinimize`, then resets; sole emission site with
  WaylandBackend.cpp:183-190) and src/desktop/view/window/Window.cpp:140
  (sole `stateRequest` listener) and :839-841 (`onUpdateState` handles
  fullscreen/maximize only, minimized dropped; no layout path consults
  minimized) and src/desktop/view/window/X11Backend.cpp:381-383
  (`setMinimized` echoes the flag to the X11 surface only) and
  src/desktop/view/window/WaylandBackend.cpp:366 (`setMinimized` no-op) and
  src/xwayland/XSurface.cpp:238-241 (surface flag plus state echo, no layout
  call) and src/protocols/ForeignToplevelWlr.cpp:85-108 (set/unset minimized
  emit event/ipc only, no layout call) @19fb395d45314960e6f79f17994a84094f1cd4f6
  (native minimize requests consumed and dropped with no tiling effect;
  window stays tiled; no minimize verb)
- `S-bsp-mininv` bspwm:src/events.c:40-80 (`handle_event` dispatch) and
  :301-330 (`client_message` handles `_NET_WM_STATE`/`_NET_ACTIVE_WINDOW`/
  `_NET_CURRENT_DESKTOP` only; no `WM_CHANGE_STATE`/iconic branch) and
  src/window.c:899-909 (the WM itself sets `ICONIC` when it hides; no
  client-iconify verb) and src/messages.c:344-345 (`hidden` is a
  scriptable hide flag) @e11eff4cb3333216ad03c815609a4ed79e08929c
  (no native minimize request path or verb; `_NET_WM_STATE_HIDDEN`
  handling is WM-owned hide and never votes here)
- `S-i3-mininv` i3:src/handlers.c:847-857 (`WM_CHANGE_STATE` iconic
  request rejected and reverted to normal; other states unhandled) and
  src/commands.c:1921-1951 (`move scratchpad` plus `scratchpad show`
  dispatch, a separate mechanism)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (native minimize request refused; window stays tiled)
- `S-xmo-mininv` xmonad:src/XMonad/Config.hs:188-227 (key inventory:
  spawn/kill/NextLayout/refresh/focus/swap/shrink/expand/sink/IncMasterN/
  quit/restart only; no minimize verb) and :119-120 (`handleEventHook`
  default-true) and src/XMonad/Main.hs:267-271 plus :405-419 (event
  dispatch runs the config hook then the default handler; `PropertyEvent`
  handles `WM_NAME` only; `ClientMessage` handles `XMONAD_RESTART` only,
  else broadcasts; no iconify branch) and :432-438
  (startup scan notes `WM_STATE` iconified only) and
  src/XMonad/Operations.hs:107-119 (`manage`: fixed-size/transient float
  via `insertUp` plus `float`, else `insertUp`; no minimize branch) and
  xmonad-contrib:XMonad/Hooks/EwmhDesktops.hs:595-640
  (`ewmhDesktopsEventHook'`: close/current-desktop/wm-desktop/active-window
  only, else `mempty`) and :354-388 (only `_NET_WM_STATE_ABOVE/BELOW`
  `ClientMessage` handling) and :765 (advertises `_NET_WM_STATE_HIDDEN`,
  no minimize handling) and :679-709 (`fullscreenEventHook'`: fullscreen
  atom only) and XMonad/Actions/Navigation2D.hs (focus/swap actions only,
  no event hook in the profile) and
  src/XMonad/Operations.hs:278-290 (`hide`/`reveal` internal unmap/map
  primitives, never a minimize vote per the matrix header) and
  xmonad-contrib:XMonad/Actions/Minimize.hs:1-60 plus
  XMonad/Layout/Minimize.hs:1-60 (minimize/maximize actions plus layout
  modifier exist in contrib but are out of this Tall+Navigation2D profile)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (no iconify request handling in profile; no minimize counterpart)
  (contrib path @5097a457e7a409bc9a7584dc5aa82b34c69d6dda)
- `S-sway-mininv` sway:sway/commands.c:114-143 (alphabetized runtime command
  table: layout/split/move/swap/scratchpad and others; no minimize verb)
  and sway/commands/scratchpad.c (separate scratchpad path) and
  sway/desktop/xwayland.c:622-634 (sole minimize listener:
  `handle_request_minimize` echoes the protocol flag via
  `wlr_xwayland_surface_set_minimized` when unfocused only, clearing it when
  focused) and :287-288 (activate clears minimized) and
  sway/tree/view.c:1149-1180 (`view_is_visible`: destroying/workspace/
  sticky/tab checks only, no minimized consult; container never detaches on
  the minimize path) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (focused minimize request cleared to unminimized with no container detach;
  window stays tiled; no minimize verb)
- `S-qti-minimize` qtile:libqtile/backend/x11/window.py:1802-1816
  (`minimized` setter via `toggle_minimize` into `MINIMIZED`) and
  :1890-1926 (`_reconfigure_floating`: `MINIMIZED` sets `IconicState` plus
  `hide()`, else-branch clears via `floating=false`) and :2110-2116
  (`WM_CHANGE_STATE` iconic honored under `auto_minimize`) and
  libqtile/resources/default_config.py:207 (`auto_minimize=true` shipped)
  and libqtile/backend/base/float_states.py (`MINIMIZED` enum member) and
  libqtile/group.py:304-332 (`mark_floating`: True removes from
  `tiled_windows`/layouts into the floating layout, False re-adds via
  `add_client`) and libqtile/layout/columns.py:224-225 (`cc` is the current
  column) and :132-152 (shipped-relevant defaults: `insert_position` 0 at
  current, `fair` off, `align` right) and :266-288 (`add_client` admits at
  the focused `cc` position, `remove` drops the emptied column) and
  libqtile/resources/default_config.py:102 (`Columns` with shipped defaults)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (native request path plus hide and allocation paths; restore re-admits at
  the focused position, not the old slot; restore focuses through the layout when current)
- `S-qti-nosticky` qtile:libqtile/layout/columns.py:204-508 (exposed
  Columns command inventory: directional focus, shuffles, grows, normalize,
  swap-column, toggle_split; no sticky verb) + libqtile/backend/x11/window.py:2213-2234
  (window toggles: enable/disable floating, toggle_maximize, toggle_fullscreen;
  no sticky verb) + libqtile/group.py:186-332 (group focus/add/
  remove/mark_floating carry no sticky flag) + libqtile/backend/x11/xcbq.py:98
  (`_NET_WM_STATE_STICKY` atom string only, wired to no window verb)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (no sticky verb or all-workspace flag in source; float stays per-window
  per `S(S-qti-float)`)
- `S-qti-wstoggle` qtile:libqtile/group.py:226-244 (`add` admits tiled
  layouts plus a floating layer, no workspace mode branch) and :304-332
  (`mark_floating` per-window layer move) + libqtile/layout/columns.py:204-508
  (exposed inventory carries no workspace tiling flag or toggle) +
  libqtile/resources/default_config.py:75-103 (static groups 1-9, shipped
  `layouts = [Columns(...), Max()]`) and :184-196 (global `floating_layout`
  rules plus `auto_fullscreen`, no per-group tiling default)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (Columns always tiles plus a floating layer; float is per-window)
- `S-qti-snap` qtile:libqtile/layout/columns.py:452-506 (`shuffle_left`/
  `shuffle_right` carry the tiled `cc.cw` across columns or split a shared
  edge column with sole-column sole-window no-op; `shuffle_up`/`shuffle_down`
  reorder in-column only) + libqtile/backend/x11/window.py:2240-2250
  (unbound tiled `set_position`: floating explicit tweak else swap with the
  window under the pointer) and :2203-2205 (shipped `set_position_floating`
  explicit-coordinate tweak) + libqtile/backend/wayland/window.py:841-857
  (same swap policy) and :837-839 (same explicit tweak)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (no directional float-move or half/quarter snap verb or state in the cited paths)
- `S-qti-fsslot` qtile:libqtile/group.py:304-332 (`mark_floating` removes
  maximized windows from `tiled_windows`/layouts but skips removal while
  fullscreen, so fullscreen keeps its tiled slot) + libqtile/backend/base/window.py:262-285
  (`maximized` float state at work-area size with save) and :305-323
  (`_set_fullscreen` save on entry, restore on exit) and :340-361
  (`save_float_state`/`restore_float_state` geometry plus saved state with no
  rule re-match) + libqtile/group.py:231-232 (`auto_fullscreen` admission) +
  libqtile/backend/x11/window.py:636-654 (`update_state` syncs urgent/fullscreen
  only) and :2083-2101 (client `_NET_WM_STATE` echoed to the property only) +
  libqtile/backend/wayland/window.py:432-433 (native `handle_request_maximize`
  drives the maximized state) + libqtile/backend/x11/window.py:2221-2236
  (`toggle_maximize`/`toggle_fullscreen`) + libqtile/resources/default_config.py:47-55
  (`Mod+f` fullscreen, `Mod+t` floating)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (maximize leaves the tiling, fullscreen keeps its slot; X11 native maximize
  is echo-only while the Wayland request drives the state)
- `S-qti-minfocus` qtile:libqtile/backend/wayland/window.py:746-756
  (`minimized` setter into `MINIMIZED`, restore via `floating=false`) and
  :762-786 (`_reconfigure_floating`: `MINIMIZED` hides, else places) and
  :376-437 (native minimize/maximize/fullscreen request callbacks drive the
  state with no `auto_minimize` gate) + libqtile/backend/x11/window.py:1802-1816
  (same setter shape) and :1890-1926 (same hide/restore shape) and :2110-2116
  (`WM_CHANGE_STATE` iconic honored under `auto_minimize`) +
  libqtile/resources/default_config.py:207 (`auto_minimize=true` shipped) +
  libqtile/group.py:304-332 (True removes into the floating layout, False
  re-adds at the focused `cc`; current retained, restore focuses through the
  layout when current; hide retains membership with no workspace cleanup verb)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (minimize/restore carry no refocus write beyond the layout focus)
- `S-awe-minimize` awesome:objects/client.c:2554-2614
  (`client_set_minimized`: `ICONIC` unmap plus `NORMAL` remap, `banning`
  update, `property::minimized` signal) and ewmh.c:402-409
  (`_NET_WM_STATE_HIDDEN` ADD/REMOVE/TOGGLE drives the same setter) and
   lib/awful/permissions/init.lua:809-814 (refocus hooks on
  unmanage/tag/hide/minimize/sticky) and lib/awful/layout/init.lua:343-352
  (property::minimized/fullscreen/maximized/floating arrange hooks)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (native request path plus unmap/ban with retained client order)
- `S-nir-mininv` niri:src/protocols/foreign_toplevel.rs:574-575
  (`SetMinimized`/`UnsetMinimized` explicit no-ops) and
  niri-ipc/src/lib.rs:194-946 (full `Action` enum at pin lists no minimize
  verb) @ed22699d99462f61ab171472d3ea67e844ea580d
- `S-pap-minimize` PaperWM:tiling.js:4720-4738 (`minimizeHandler`: tiled
  mark plus move to the scratch layer; unminimize via `unmakeScratch`) and
  :3487-3511 (`notify::minimized` wiring) and :411-412 (workspace
  `window-removed`/`window-added` wired to `remove_handler`/`add_handler`)
  and :3950-3966 (`remove_handler`: `space.removeWindow` on workspace exit,
  so `stick()` removes from the space synchronously) and :3972-3984
  (`add_handler`: `unstick` re-enters via `insertWindow` existing path) and
  :981-1030 (`removeWindow`: column splice with empty-column drop, neighbor
  selection, relayout) and :4155 (`insertWindow` re-adds via `addWindow` at
  `getOpenWindowPositionIndex`) and :4262-4280
  (`getOpenWindowPositionIndex`: shipped RIGHT default inserts after the
  selected window, not the old slot) and scratch.js:62-110 (`makeScratch`:
  float plus above plus `stick()`) and :137-145 (`unmakeScratch`: float
  cleared with unstick) @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (scratch-layer path with synchronous removal and indexed reinsertion;
  settled widths per `S(S-pap-layout)`, exact slot follows the live
  selection, focus stays TBD where stated)
- `S-kar-minimize` karousel:src/lib/world/ClientManager.ts:84-96
  (`minimizeClient`: `Tiled` to `TiledMinimized`, Immediate focus pass when
  the client is the last-focused one) and :182
  (`onClientFocused` records `lastFocusedClient`) and
  src/lib/world/Clients.ts:16-28 (`canTileNow` excludes minimized;
  `makeTileable` unminimizes) and
  src/lib/world/clientState/TiledMinimized.ts (minimizedChanged retile) and
  src/lib/layout/Window.ts:134-136 (`destroy` forwards to
  `column.onWindowRemoved`) and src/lib/layout/Column.ts:297-325
  (`onWindowRemoved`: above-else-below focus candidate, last-window destroy)
  and :205-211 (`getWindowToFocus`: focus-taker else first window) and
  src/lib/layout/Grid.ts:161-185 (`onColumnRemoved`: left-else-right focus
  column, null on the last column) and
  src/lib/world/clientState/Tiled.ts:8-16 (re-tile builds a fresh column
  after the last-focused, else last, column: not the old slot)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (minimized state path with directed focus pass and fresh-column restore;
  reflow frames stay TBD)
- `S-pan-minimize` paneru:src/ecs/triggers.rs:544-559 (`WindowMinimized`
  inserts `Unmanaged::Minimized`; `WindowDeminimized` removes it) and
  :727-772 (`remember_managed_strip` plus `window_minimized_trigger`:
  strip removal with remembered index and active-strip `give_away_focus`)
  and :773-860 (`window_managed_trigger`: reinsert at the remembered
  `PreviousManagedStrip` index, else active-strip overlap/end) and :990-1040
  (`give_away_focus`: nearest-center neighbor, else any other column) and
  src/ecs.rs:857-859 (`WindowProperties.insertion`: rule `index` override,
  None under shipped defaults) and src/ecs/state.rs:587-589
  (Minimized/Hidden never on screen) and src/types/commands.rs:220-275 (`Operation`
  inventory at pin lists no minimize verb) @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (minimize-mark path; old-slot restore only same-workspace without an
  insertion override; strip reflow and focus target stay TBD)
- `S-ours-minkde` KDE production observer on the world route:
  omnitiler:kwin/src/world-observe.ts (`captureWorldObservation`:
  non-`normalWindow` windows skipped; output/desktop/frame gates hold the
  domain instead of converging partial membership; minimized rows ride
  separate lifetimes; active-window value decides global focus) and
  kwin/src/kwin-globals.d.ts (`minimized` Q_PROPERTY with
  `minimizedChanged` NOTIFY) @13dcb76
  (minimized frame readability and native active-window value decide
  between stale-frame observation and fail-closed holds; Engine effect
  and focus stay TBD)
- `S-ours-minwin` Windows adapter minimize path at this HEAD:
  crates/tiler-windows/src/tiling_sys.rs:523-525 (doc: `IsIconic` read
  before frames; iconic returns known identity as `Minimized`) and
  :553-556 (code: iconic short-circuit with no frame read) plus
  :1569-1588 (no-frame retained row: Engine membership survives
  minimization) and :4424-4429 (retained row rides the last-known tile
  rect, hintless, no writes) and :9236-9245 (close cleanup drops only
  truly absent HWNDs) and crates/tiler-windows/src/tiling.rs:341-347
  (`minimized` classifies as a state skip) @13dcb76
  (slot retained without reflow; focus stays TBD)
- `S-cos-grpmove` cosmic-comp:src/shell/element/stack.rs:431-483
  (`handle_move`: in-stack Left/Right reorder when a neighbor exists,
  else `MoveOut` with the active index clamped to the survivor) and
  src/shell/layout/tiling/mod.rs:1507-1560 (`move_current_node`
  stack-internal branch plus `MoveOut` reinsert as a new tile beside
  the group) @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (leave/reorder legs; outside-into-stack join per `S(S-cos-move)`)
- `S-cos-grpclose` cosmic-comp:src/shell/element/stack.rs:241-271
  (`remove_window`: active index clamped with `fetch_min`, single-member
  dissolve path) and :273-305 (`remove_idx` same index fixup)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (membership plus active-tab leg; semantic tab-bar membership suffices)
- `S-hyp-grpmove` Hyprland:src/config/shared/actions/ConfigActions.cpp:1338-1375
  (`moveWindowIntoGroupHelper` add plus `setCurrent` mover plus mover
  focus, `moveWindowOutOfGroupHelper` remove with direction focal plus
  `focus_removed_window` mover/group-current branch) and :1377-1416
  (`moveIntoGroup` needs a directional neighbor already in a group,
  `moveOutOfGroup` needs group membership) +
  src/desktop/view/Group.cpp:97-173 (`add` inserts after current by
  default and makes the newcomer current) and :233-298 (`remove`
  index fixup with single-member dissolve via target switch)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
- `S-nir-consume` niri:src/layout/scrolling.rs:925-951
  (`add_tile_to_column`: `None` appends last with `activate_idx` plus
  column activation) and :1795-1901
  (`consume_or_expel_window_left`: single-tile joins the left column,
  multi-tile expels to a new left column) and :1903-1993 (right-side
  mirror) and :1995-2060 (`consume_into_column`/`expel_from_column`
  explicit verbs) and :2189-2226 (`toggle_column_tabbed_display`
  Normal/Tabbed flip) and :1626-1640 (`focus_down`/`focus_up` column
  member step) and :4865-4871 (`activate_idx` saturating step)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (membership/order/active legs; semantic tab-bar membership suffices)
- `S-pap-slurp` PaperWM:tiling.js:5228-5310 (`slurp` on the focused column
  consumes the directional neighbor per `open_window_position` at the
  ABOVE/BELOW/TOP/BOTTOM position, emptied columns removed; shipped
  RIGHT `slurp(B)` from the right column has no right neighbor) and
  :5317-5359 (`barf` expels the named or bottom window to a new column
  at the directional open position) @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (membership legs; selection retained on both steps - neither path
  writes selectedWindow)
- `S-kar-grpmove` karousel:src/lib/keyBindings/Actions.ts:22-36
  (`focusUp`/`focusDown` member step via above/below window) and :90-120
  (`windowMoveLeft` single-window joins the left column, shared-column
  expels to a new left column; `windowMoveRight` mirror) and
  src/lib/layout/Window.ts:26-33 (`moveToColumn` remove plus add) and
  src/lib/layout/Column.ts:275-295 (`onWindowAdded` appends last or first
  with `isFocused` focus-taker update) and :297-325 (`onWindowRemoved`
  above/below focus-taker fixup plus column destroy) and :267-273
  (`toggleStacked` needs 2+ windows) and :225-250 (stacked overlapping
  arrange vs visible heights)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (membership/order/focus-taker legs)
- `S-ours-grp` omnitiler:crates/tiler-core/src/directional.rs:107-123
  (`Node` is Leaf or split-axis Group only, no tab/stack variant) +
  crates/tiler-core/src/session/ops/mod.rs:8-15 (families:
  drag/float/focus/lifecycle/move/resize/workspace only) +
  crates/tiler-core/src/session.rs:1810-1817 (Center never plans,
  unsupported stack behavior) and :3674-3699 (center preview/release
  fail closed as unsupported stack with no plan)
  @9241c94
  (no tab carrier and no semantic join/leave verb in any Engine layer)
- `S-nir-view` niri viewport, focus-scroll, center and manual scroll:
  niri:src/layout/scrolling.rs:575-579 (center-focused policy: Always,
  or single-column) and :655-715 (focus scroll: centered vs minimal fit,
  OnOverflow neighbor rule) and :779-829 (every column activation
  animates the view; same-column DnD exception) and :2228-2270
  (`center_column`/`center_window` one-shot, active-column only) +
  src/layout/workspace.rs:1182-1196 (center dispatch incl floating) +
  src/input/mod.rs:1666-1680 (`CenterColumn` dispatch) and :3386-3406
  (touchpad gesture scrolls the view) +
  src/layout/scrolling.rs:3101-3127 (gesture update writes only the view
  offset, no focus write) and :3203-3528 (gesture end snaps to the closest
  column boundary, extends furthest toward the gesture direction, writes
  the active column and clamps to the first/last column) +
  niri-ipc/src/lib.rs:448-472
  (`ToggleColumnTabbedDisplay`/`SetColumnDisplay`/`CenterColumn`/
  `CenterWindow`/`CenterVisibleColumns` actions)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (policy plus one-shot centers plus touchpad gesture scroll: view-only
  during, snap-activate with strip clamp at end)
- `S-pap-view` PaperWM viewport, center and gesture scroll:
  PaperWM:tiling.js:4291-4355 (`ensuredX`: neighbor/minimal,
  CENTER/EDGE/wide/edge-margin branches) and :5055-5081
  (`centerWindow` one-shot work-area centering via `move_to`) and
  :1956-1980 (background scroll only during grab/navigation switches
  focus) + gestures.js:338-368 (swipe moves the view and reselects the
  swipe target) and :369-420 (glide snaps via `ensuredX` with selection)
  + keybindings.js:302-311 (center-horizontally/vertically/center
  registrations)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (one-shot center plus minimal/mode scroll plus reselecting swipe;
  keyboard step inventory is `S(S-pap-scroll)`)
- `S-pap-layout` PaperWM per-column width policy:
  PaperWM:tiling.js:724-740 (selected column from the selected window's
  tiledWidth/frame, other columns max of members, work-area clamp;
  widths independent per column) and :576-599 (`layoutColumnSimple`
  preferredWidth px/% override plus resizable targets) and :801-804
  (single-column one-shot center)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (existing columns keep widths, newcomer from frame/preferredWidth;
  no cross-column rescale)
- `S-pap-cycle` PaperWM linear next/previous cycle verbs:
  PaperWM:tiling.js:1096-1123 (`switchLinear` column/row walk with loop
  wrap, `getWindow` plus `ensureViewport`) and :883-891 (`getWindow`
  false out of range) + keybindings.js:191-194 (`switch-next`/
  `switch-previous` registration; :204-207 loop variants) +
  schemas/org.gnome.shell.extensions.paperwm.gschema.xml:212-219
  (shipped Super+period/comma) and :247-254 (loop variants unbound)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (model-order cycle with stay-at-edge under the bound verbs; floats
  excluded per `S(S-pap-float)`)
- `S-pap-scroll` PaperWM keyboard scroll-step inventory:
  PaperWM:tiling.js:1264-1290 (`drift` moves the view but reselects via
  `findTargetWindow` plus `ensureViewport`) + keybindings.js:201-202
  (`drift-left`/`drift-right` registration) +
  schemas/org.gnome.shell.extensions.paperwm.gschema.xml:229-236
  (shipped Super+bracketleft/bracketright)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (no focus-preserving keyboard scroll-step verb; drift reselects)
- `S-pap-wssel` PaperWM workspace-switch float handling and return focus:
  PaperWM:tiling.js:2723-2729 (`switchWorkspace` moves the tiled
  `getWindows()` only; floats stay on their workspace) and :1036-1044
  (`addFloating` holds floats in the per-space `_floating` list with the
  clone parented to the space actor) + navigator.js:455-456 (switch accept
  falls back to the retained tiled `selectedWindow`; floats never hold
  selection) and :463-472 (selected window with focus runs
  `focus_handler`, else `Main.activateWindow`)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (switch moves tiles only; accept activates the retained tile; float
  frames retained per `S(S-pap-layout)`; workspace visibility and float
  stacking stay host-side)
- `S-kar-scroll` karousel viewport, focus-scroll and manual scroll:
  karousel:src/lib/layout/Desktop.ts:61-80 (`scrollIntoView` minimal)
  and :83-104 (`scrollCenterRange`/`scrollCenterVisible` Centered/Grouped
  variants, `autoAdjustScroll`, `scrollToColumn`) +
  src/lib/layout/Grid.ts:195-201 (column focus scrolls via
  `scrollToColumn`) + src/lib/keyBindings/Actions.ts:327-400
  (`gridScrollLeft/Right` by step, `gridScrollFocused` one-shot,
  edge-column verbs) + src/lib/config/definition.ts:103-106
  (`manualScrollStep` default 200) + src/lib/behavior/scroller/
  LazyScroller.ts:1-5 (minimal), CenteredScroller.ts:1-3 and
  GroupedScroller.ts:1-5 (centering variants)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (Lazy minimal focus-scroll plus focus-preserving manual step plus
  one-shot recenter; Centered/Grouped are named variants only)
- `S-kar-cycle` karousel preset-width cycling:
  karousel:src/lib/behavior/PresetWidths.ts:8-18 (`next`/`prev` wrap to
  the first width on exhaust) + src/lib/keyBindings/Actions.ts:220-229
  (`cyclePresetWidths`/`cyclePresetWidthsReverse` via the resizer)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (forward and reverse wrap through the shipped 50%/100% presets)
- `S-pan-colops` paneru column/viewport operations:
  paneru:src/types/commands.rs:222-280 (`Center`/`Resize`/`SetWidth`/
  `Stack`/`ToggleTabbedDisplay`/`Snap`/`VirtualMove` ops) +
  src/commands.rs:690-742 (`command_center_window` one-shot strip
  reposition with manual-offset record) and :744-829 (Grow/Shrink preset
  stepping with cycle) and :1460-1511 (`toggle_tabbed_display_handler`:
  Stack split/tabbed flip, no-op otherwise, tabs cycle with Focus
  North/South) + src/ecs/layout.rs:702-800 (`stack` merges into the left
  neighbor, `unstack` splits to an adjacent own column) +
  src/ecs/focus.rs:310-345 (`autocenter_window_on_focus` plus
  `reshuffle_around`) + src/config.rs:785-790 (`window_resize_cycle`
  defaults true, `auto_center` defaults off)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (one-shot center, wrapping width cycle, left-merge/right-split,
  tabbed flip and expose-on-focus; settled frames stay TBD)
- `S-pan-tabs` paneru app-native tab nesting:
  paneru:src/ecs/systems.rs:1447-1535 (`detect_tabbed_windows`: same-app
  same-frame hidden-leader grouping via `convert_to_tabs` plus newcomer
  focus) and :1351-1440 (`regroup_stray_native_tabs` folds stray
  background tabs into the showing leader) +
  src/ecs/layout.rs:515-547 (`convert_to_tabs` grouping) and :196-203
  (`StackItem` single vs app-native tabs per `S(S-pan-model)`)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (detection/grouping plus newcomer focus; width stability and tab
  selection remainders stay TBD)
- `S-pan-stripwidth` paneru strip width policy:
  paneru:src/ecs/layout.rs:293-301 (`Column::width` is the widest member
  frame) and :923-939 (`column_positions` accumulates member widths with
  no cross-column rescale) + src/ecs/triggers.rs:1115-1116 (newcomer
  `WidthRatio` from its OS frame width) and :1194-1215 (no-rule windows
  keep that ratio; rule widths apply only with a matching rule)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (newcomer takes its own frame width; existing columns keep widths;
  exact pixels need the newcomer frame)
- `S-pan-focus` paneru directional focus traversal:
  paneru:src/commands.rs:279-398 (`command_move_focus`: East/West strip
  neighbours via `get_window_in_direction`, North/South inside a `Stack`
  column only, no wrap; off-strip focus enters from the directional edge;
  N/S display fall-through only; `reshuffle_around` the target) and
  :135-175 (peer resolution per `S(S-pan-swap-peer)`) +
  src/ecs/focus.rs:310-345 (focus exposes via `reshuffle_around`;
  `auto_center` off by default per `S(S-pan-colops)`)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (strip-order traversal with no history consult; settled offsets stay TBD)
- `S-pan-stack` paneru stack join/leave focus:
  paneru:src/commands.rs:1422-1459 (`stack_windows_handler`: the focused
  window merges left/splits right with no focus write, then reshuffles
  around it) + src/ecs/layout.rs:702-746 (`stack` appends the mover last;
  leftmost no-op) and :758-806 (`unstack` restores the mover to an
  adjacent own column right of the survivors)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (membership/order plus focus-retention legs)

- `S-bsp-stack` bspwm:src/stack.c:135-187 (`limit_above`/`limit_below`
  plus `stack`: focused nodes take the above branch with `window_above`,
  unfocused the below branch; floats participate unless `auto_raise` is
  held false) and src/events.c:455,471 (pointer-motion hold-false, restore
  true) and src/rule.c:256-265 (`_apply_window_state` maps
  `_NET_WM_STATE_BELOW` to `LAYER_BELOW`) and src/messages.c:271-279
  (`node -l/--layer` sets the stacking layer) and
  doc/bspwm.1.asciidoc:469 (`-l` sets the stacking layer) @e11eff4
  for the doc path, @e11eff4cb3333216ad03c815609a4ed79e08929c for src
  (raise-on-focus stacking; no project same-layer order-lower verb:
  BELOW is a persistent stacking layer, not this leg's order-lower)
- `S-bsp-fltptr` bspwm:src/window.c:487-545 (`move_client` float branch
  writes `floating_rectangle` x/y) and :547-630 (`resize_client` float
  branch grows w/h with hints applied and writes the rectangle) and
  src/pointer.c:58-68,248-307 (modifier+button pointer grab with
  `ACTION_MOVE`/`ACTION_RESIZE_CORNER`, bottom-right default handle)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (free float pointer move/resize; tiled branch swaps instead)
- `S-i3-raise` i3:src/con.c:281-294 (`con_raise` moves the float to the
  tail of the workspace floating list; `con_activate` focuses plus raises)
  and src/floating.c:478-484 (`floating_raise_con` tail insert) and
  src/click.c:279-284 (raise on click) @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (raise only; no lower verb in `con.h`/`floating.h`)
- `S-i3-fltdrag` i3:src/click.c:284-290 (floating-modifier+left and
  titlebar-left drag producers) and :305-330 (floating-modifier+right and
  border/decoration-right resize producers) and src/floating.c:597,701
  (raise before drag/resize) @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (free float drag/resize with raise)
- `S-sway-fltraise` sway:sway/tree/container.c:1682-1693
  (`container_raise_floating`: scene top plus floating-list end) and
  sway/tree/root.c:203 (raise on focus path) and
  sway/input/seatop_down.c:231 (raise on press) and
  sway/input/seatop_move_floating.c:75 +
  sway/input/seatop_resize_floating.c:188 (raise on float move/resize begin)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (raise paths; no lower verb in the inspected inventory)
- `S-sway-fltptr` sway:sway/input/seatop_default.c:458-488 (float move via
  mod+left/titlebar-left; float resize via border-left or mod+resize with
  quadrant-resolved edges) and sway/input/seatop_move_floating.c:39-45
  (free pending-x/y write) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (free float pointer move/resize; hint clamp is `S(S-sway-min)`)
- `S-hyp-raise`
  Hyprland:src/config/shared/actions/ConfigActions.cpp:755-769
  (`alterZOrder` top/bottom) + src/desktop/state/WindowState.cpp:43-70
  (`raise`/`lower` reorder via `moveToZ` with no focus write) +
  src/desktop/view/window/Window.cpp:833,
  1001,1462,1908 (raise on float-toggle/activate) +
  src/managers/input/InputManager.cpp:924 (raise on float click) +
  src/config/lua/bindings/LuaBindingsDispatchers.cpp:608-613 (Lua-only
  `bringToTop`/`alter_zorder`; no keybind dispatcher)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (raise on focus/press; lower is Lua-only with no focus write)
- `S-hyp-fltdrag`
  Hyprland:src/layout/supplementary/DragController.cpp:135-157 (tiled
  pick-up branch skipped for floats) and :401-430 (float position/size
  writes) + src/config/shared/actions/ConfigActions.cpp:1687-1715
  (`movewindow` mouse producer) + src/config/values/ConfigValues.cpp:188
  (`general:snap:enabled` defaults false)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (free float pointer move/resize, snap off default)
- `S-nir-fltact` niri:src/layout/floating.rs:583-593 (`activate_window`
  raises to index 0) and :865-930 (directional float focus runs nearest
  center-distance search, miss returns false) + src/layout/mod.rs:1553
  (`activate_window` layout entry) + src/layout/workspace.rs:1868-1872
  (tiling/floating activation dispatch)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (focus raises floats; no lower verb in `S(S-nir-acts)`)
- `S-nir-fltfocus` niri:src/layout/floating.rs:855-930
  (`focus_directional` nearest-center search plus `focus_left/right/up/
  down`; miss returns false with no focus change)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (float-to-float search and miss behavior)
- `S-nir-ffm` niri:src/niri.rs:6790-6840 (opt-in pointer-entry
  activation without raising, optional scroll threshold) and
  resources/default-config.kdl:68-70 (shipped option commented out) +
  src/input/mod.rs:2626,2727 (pointer-motion focus dispatch) and
  :3031-3033,3097-3106 (plain press activates and forwards the event)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (shipped hover off; named enabled variant and ordinary click focus)
- `S-nir-clientgrab` niri:src/handlers/xdg_shell.rs:71-182
  (valid same-client move request starts MoveGrab with viewport
  scrolling enabled) and :184-309 (client edge-resize request)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (client titlebar/edge producers exist, separate from modifier grabs;
  move requests are serial-qualified on a same-client press)
- `S-nir-drag` niri:src/input/move_grab.rs:82-117 (release runs
  `activate_window` when still recognizing else `interactive_move_end`;
  no key path in the pointer-grab impl) and :183-219 (8px gesture
  threshold before the move begins) and :221-260 (moving tile tracks
  the output with focus; off-output positions keep the grab alive;
  absolute delta pins the tile to the cursor) + src/layout/mod.rs:3824-3884
  (`interactive_move_begin`) and :3885-4060 (update removes the tile
  and tracks its moving state) and :4078-4095
  (output change focuses the new output) and :4112-4330
  (end resolves Existing/NewAt workspace and commits
  NewColumn/InColumn/Floating with `ActivateWindow::Yes`,
  or re-activates) and :2869-2930 (insert hint shown only while
  Moving, cleared otherwise) and :2621-2665 (per-frame edge
  view-scroll runs during the move) and :4020-4030 (alpha dip while
  moving) + src/layout/scrolling.rs:836-901 (insert branch by pointer
  geometry) + src/layout/monitor.rs:1595-1641 (drop workspace resolve
  over rendered geos, else NewAt)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (pointer move/remove/reinsert path with insert-hint preview;
  insert branch and workspace resolve by pointer geometry;
  no keyboard consume/expel call; InColumn commits a member-add)
- `S-nir-ptr` niri:src/input/mod.rs:2929 (Mod+Left activates plus move
  grab) and :2964-3020 (Mod+Right edge resize grab; floats skip the
  double-click gesture) + src/input/move_grab.rs:173-260 (motion delta;
  floating skips tiled viewport adjustment) + src/layout/mod.rs:3824-3900
  (interactive move update) and :2384-2391 (`resize_edges_under`) +
  src/layout/scrolling.rs:3559-3655 (scrolling interactive resize
  begin/update: dragged column width `SetFixed` from the delta,
  neighbors untouched) + src/layout/floating.rs:1106-1168
  (interactive resize writes fixed sizes from deltas) and :948-962
  (directional 50px steps) @ed22699d99462f61ab171472d3ea67e844ea580d
  (pointer move/resize with activation raise; scrolling resize writes
  the dragged column width)
- `S-xmo-restack` xmonad:src/XMonad/Operations.hs:197-204 (`restackWindows`
  with floats-first `flt ++ rs` order) and :212-218 (`W.peek` border plus
  `setTopFocus`) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (floats always above tiles; F-vs-G order effect of focus untraced)
- `S-xmo-switch` xmonad:src/XMonad/StackSet.hs:231-243 (`view` swaps the
  current workspace, hidden moves) + `S(S-xmo-restack)` (refresh draws
  only member floats; `W.peek` plus `setTopFocus` restores focus)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (workspace switch hides, keeps frames, restores peek focus)
- `S-awe-raise` awesome:objects/client.c:3395-3430 (`c:raise()` top of
  layer, `c:lower()` bottom of layer) and :4507-4508 (method registration)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
- `S-qti-raise` qtile:libqtile/backend/base/window.py:600-610 (normal
  activation focuses the group and calls `bring_to_front`) +
  libqtile/backend/x11/window.py:1487-1492 (`StackMode.Above` plus
  raise-children), :1435-1440 (`move_to_bottom`, masked stacking write)
  and :973-1081 (same-layer bottom placement) +
  libqtile/backend/wayland/window.py:115-122
  (`bring_to_front` plus `move_to_bottom`) +
  libqtile/backend/x11/core.py:905-915 (bring-front-click incl
  floating-only) @83c697a5621306c3586efca31867efcfa0482e2d
  (raise on activation; explicit lower exists on both backends)
- `S-qti-flt13` qtile:libqtile/group.py:110-145 (`layout_all` lays out
  the floating layer then focuses `current_window`) and :146-165
  (`set_screen` shows with float offset plus `layout_all`, hides all on
  `None`) and :168-197 (`focus` records `current_window`) +
  libqtile/layout/floating.py:90-115 (`to_screen`) and :206-252
  (`configure` keeps placed geometry plus `unhide`)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (switch hides, keeps frames, restores current-window focus)
- `S-cos-fltptr` cosmic-comp:src/shell/mod.rs:4325,4589
  (`floating_layer.resize_request` edge grabs) +
  src/shell/grabs/moving.rs:967-1012 (floating `drop_window` retains in
  the floating layer; `move_element`) @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (free float pointer move/resize; producers are `S(S-cos-dragstart)`)
- `S-pap-float` PaperWM:tiling.js:1031-1055 (per-space `_floating` list
  with `addFloating`/`removeFloating`) and :1129-1160 (`switch` walks
  tiled columns only) and :2136-2142 (`selectedIndex` -1 for floats) and
  :4400 (select raises) and :4125-4135 (non-tileable admission floats plus
  `make_above`) and :4355-4373 (ensureViewport rejects floats before
  changing selectedWindow) and :4597-4610 (scratch/transient focus returns
  without changing tiled selection) +
  scratch.js:62-83 (`makeScratch`: above plus stick plus
  float flag) and :137-145 (`unmakeScratch` restores)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (dialog-float and scratch-stuck paths; ordinary windows tile)
- `S-kar-float` karousel:src/lib/world/clientState/Floating.ts:1-20
  (keepAbove only when configured) +
  src/lib/config/definition.ts:188-191 (`floatingKeepAbove` defaults
  false) + src/lib/world/ClientManager.ts:105-110,158-175
  (float/toggle transitions) + src/lib/layout/Grid.ts:161-176 (removal
  focus fixup) + src/lib/world/World.ts:124-131 (`doIfTiledFocused` gate)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (float state, tiled-only verbs, no raise/lower/sticky verb)
- `S-pan-flt` paneru:src/ecs.rs:350 (`Unmanaged::Floating`, not part of
  tiling) + src/commands.rs:203-230 (visible floats filtered by workspace
  membership) and :458-500 (`RaiseFloating` focuses last-floating and
  raises others in-tier; AX raise needs app-frontmost) +
  src/types/commands.rs:220-280 (`Manage` toggle plus `FocusUnmanaged`/
  `FocusManaged`/`RaiseFloating`/`FloatingLayer`) + src/commands.rs:1002-1051
  (`Manage` toggles `Unmanaged::Floating` on the focused window with no
  focus write; unfloat placement rides `window_managed_trigger` per
  `S(S-pan-ins)`) + src/ecs/triggers.rs:359-360
  (per-workspace focus-history record) and :600-670
  (`window_unmanaged_trigger`: floating drops strip membership, closing
  the column gap)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (float model, toggle and raise semantics; arbitrary-F raise and lower
  have no verb path)
- `S-ours-fltrefuse` shared Engine float refusal:
  omnitiler:crates/tiler-core/src/session/ops/resize.rs:102-104
  (keyboard resize refuses exceptions as `NotTiled`) and :454-456
  (pointer resize refuses exceptions as `NotTiled`) @9241c94
  (project float resize has no shared path; host journeys stay TBD)
- `S-ours-fltsel` Windows float workspace select:
  omnitiler:crates/tiler-windows/src/tiling_sys.rs:9840-9900
  (leaving members hide through identity or recovery read, reveal on
  return) + crates/tiler-windows/src/workspace.rs:400-412
  (`focus_target` prefers `last_focus` when still a member and visible)
  @9241c94
  (float hide/reveal with last-focus return; frames TBD)
- `S-ours-fltstack` float stacking on both Ours platforms:
  omnitiler:kwin/src/world-native.ts (automatic fixed-float keep-above
  application with below-wins and external-clear release; relative F/G
  order stays host-owned)
  + crates/tiler-windows/src/tiling_sys.rs:7263-7280 (admission places
  non-topmost floats with `HWND_TOPMOST`) and :3993-4014
  (`set_topmost_band` without move/size/activate) and :7450-7521
  (unfloat restores the preimage) @9241c94
  (stacking bands exist; relative F/G order and lower have no path)

- `S-cos-out` COSMIC output verbs and admission output:
  cosmic-comp:src/input/actions.rs:535-541 (`SwitchOutput` directional
  output switch) and :613-664 (`MoveToOutput`/`SendToOutput`
  window-level directional transfer via `move_current` to the target
  output's active workspace; Move follows with mover focus, Send
  retains source focus) and :684-740 (`MigrateWorkspaceToOutput`
  whole-workspace migration, not a window verb) +
  src/shell/mod.rs:2716 (pending admission falls back to the active
  output) and :2914 (output defaults to the seat active output) and
  :3164-3175 (`move_current` defaults an absent index to the target
  output's active workspace)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (window vs whole-workspace verbs distinguished; target send anchor
  is `S(S-cos-send)`)
- `S-hyp-mon` Hyprland explicit monitor verbs:
  Hyprland:src/config/shared/actions/ConfigActions.cpp:1107-1117
  (`moveToMonitor` whole-workspace verb) and :1189-1196 (`focusMonitor`
  via `tryMoveFocusToMonitor`) @19fb395d45314960e6f79f17994a84094f1cd4f6
  (directional `moveFocus` monitor fallback itself is `S(S-hyp-focus)`;
  window-to-monitor carry vs whole-workspace move stays TBD here)
- `S-nir-mon` niri monitor verbs and transfer implementation:
  niri:niri-ipc/src/lib.rs:607-623 (`FocusMonitor*` directional/previous/
  next/named verbs) and :624-652 (`MoveWindowToMonitor*` directional/
  previous/next/named verbs) and :653-681 (`MoveColumnToMonitor*`
  directional/previous/next/named verbs) + src/input/mod.rs:1773-1785
  (`MoveWindowToMonitorLeft` carries the focused window via
  `move_to_output` plus `focus_output`) and :972-990
  (`MoveColumnLeftOrToMonitorLeft` edge-or-cross variant) +
  src/layout/mod.rs:3298-3313 (`focus_output`) and :3314-3369
  (`move_to_output` remove/insert across monitors into the target
  active workspace with Smart activate)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (per-workspace `FocusColumnLeft`/`move_left` edge-false itself is
  `S(S-nir-focus)`/`S(S-nir-move)`; monitor transfer carries with follow)
- `S-pap-mon` PaperWM monitor and directional-move verbs:
  PaperWM:tiling.js:2535-2575 (`switchMonitor` focus choreography with
  optional window carry: removes from the source space, changes to the
  target space, activates with focus) and :2576-2620 (`moveToMonitor`
  whole-space choreography with swap fallback, not a window verb) and
  :1063-1090 (`swap` same-space column reorder with edge return) and
  :1125 (`switchLeft` per-space column step) + keybindings.js:230-231
  (`move-left` binds same-space `swap`, not cross-monitor transfer)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (window carry vs whole-space move vs same-space swap distinguished;
  carry target position per `S(S-pap-ins)` open-position index)
- `S-pap-moveverbs` PaperWM:keybindings.js:101-112
  (`move-monitor-*` bind `switchMonitor` with carry true) and :114-124
  (`switch-monitor-*` carry false) and :230-237 (`move-left/right/up/down`
  bind same-space `swap`)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (registered directional verb inventory; implementation is `S(S-pap-mon)`)
- `S-kar-single` karousel single-screen scope:
  karousel:README.md:20-23 (Limitations: no multiple screens) +
  `S(S-kar-base)` shipped single-screen profile
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b for the profile,
  README at the same checkout
  (two-output fixtures have no counterpart; never a no-op vote)
- `S-pan-display` paneru display verbs and transfer implementation:
  paneru:src/types/commands.rs:235-236 (`ToNextDisplay(MoveFocus)`
  moves the focused window to the next display) and :288-293
  (pointer `ToNextDisplay`) and src/types/argv.rs:97-98
  (`nextdisplay` is Follow, `nextdisplaysend` is Stay) +
  src/commands.rs:1112-1230 (`to_next_display`: removes from the
  source strip, appends to the target display's selected strip with
  width-ratio preserved; Follow warps the mouse to the moved window,
  Stay refocuses the source neighbour) and :647-665 (display
  fall-through only when no swap peer; directional `Focus` itself is
  `S(S-pan-cmds)`)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (Follow/Stay forms distinguished; each display owns its strip)
- `S-ours-out` Ours cross-output production paths:
  omnitiler:crates/tiler-core/src/session/ops/focus.rs:233-245
  (`propose_cross_output_focus`: exhausted horizontal directional focus
  crosses to the adjacent output's selected domain workspace, then to
  that domain's valid last-focused tiled leaf; a sole occupant is that
  leaf, so the target is determined) and :425-458 (shared
  adjacency/reciprocity/remembered-target implementation) and
  crates/tiler-core/src/session/ops/move.rs:12-53 (cross-output snapshot
  carries adjacent-output domains; ambiguous ids fail closed) and
  crates/tiler-core/src/session/ops/workspace.rs:28-30 (send is
  same-output only; cross-output targets refuse as
  `CrossDomainMismatch`) @9241c94
  (directional cross-output exists with a determined sole-occupant
  target; workspace send has no cross-output counterpart)
- `S-pan-swap` paneru directional swap scope:
  paneru:src/commands.rs:592-646 (`command_swap_focus` resolves a
  same-strip peer via `get_window_in_direction` and swaps slots) and
  :647-665 (display fall-through only when no peer was swapped, and
  only for North/South; West/East never cross displays)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (directional move-left stays within the strip/display)
- `S-pan-swap-peer` paneru:src/commands.rs:135-175
  (`get_window_in_direction`: East/West resolve to strip
  neighbours, North/South resolve inside a `Stack` column only and
  return None for `Single`/`Tabs`/`Fullscren`)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (peer-resolution leg for directional swap; carry/focus via `S(S-pan-swap)`)
- `S-xmo-scope` xmonad cross-screen scope and shift focus:
  xmonad-contrib:XMonad/Actions/Navigation2D.hs:587-612
  (`navigableWindows` covers all visible screens via `sortedScreens`,
  so `windowGo` directional candidates include other screens; `windowSwap`
  candidates share the same cross-screen scope within the same layer) +
  xmonad:src/XMonad/StackSet.hs:566-584 (`shift`/`shiftWin` leave the
  moved window as the focused element on the target stack with no view
  change; source refocus after `delete'` per `S(S-xmo-close)`)
  @5097a457e7a409bc9a7584dc5aa82b34c69d6dda for Navigation2D,
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7 for StackSet
  (cross-screen focus and carry established; source refocus per close policy)
- `S-xmo-rescreen` xmonad:src/XMonad/Operations.hs:349-357 (`getCleanedScreenInfo`
  via `nubScreens`/`getScreenInfo`: screen-rect list construction, duplicates plus
  contained rects removed) and :361-371 (`rescreen`:
  positional workspace-to-screen reassignment retaining stacks, no
  window migration; current workspace kept) and src/XMonad/Main.hs:250-254,406
  (event-loop `xrrUpdateConfiguration` plus root `ConfigureEvent` to
  `rescreen`) @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (hotplug path plus screen-list construction source-traced; per-cell screen
  enumeration/geometry is F and live delivery timing is L, never generic H)
- `S-i3-outfocus` i3 directional focus output fallback:
  i3:src/tree.c:469-502 (`get_tree_next_workspace` returns the visible
  workspace on the directional output) and :593-634 (`tree_next` shows
  that workspace and focuses the descended container)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (leftmost-child `focus left` climbs to the workspace level and
  crosses; in-workspace walk itself is `S(S-i3-flt-focus)`)
- `S-bsp-monrm` bspwm RandR disconnect/reconnect policy:
  bspwm:src/settings.h:67-69 (remove-unplugged/disabled and
  merge-overlapping defaults false) + src/monitor.c:459-493 (marks
  wiring and reuses a monitor with the same RandR id), :527-538
  (remove-unplugged=true merges before removing) and :286-298
  (`merge_monitors` transfers all desktops to the target)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (default retains disconnected monitor/desktops; same-id return
  reuses them; named removal variant migrates, not destroys)
- `S-sway-evac` sway output-removal evacuation and return:
  sway:sway/tree/output.c:205-257 (`output_evacuate` migrates each
  workspace to the highest-available else fallback output, destroying
  empties; sticky-only empties evacuate stickies first) and :258-280
  (`output_destroy` guards) and :31-57 (`restore_workspaces` moves back
  workspaces whose highest-available is the new output, plus
  fallback-output workspaces; history never consulted)
  + sway/input/seat.c:234-330 (`handle_seat_node_destroy`: destroyed-empty
  refocus via sibling/workspace/last-known fallback, visible-only guard)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (evacuation destination plus priority return affinity; evacuate/restore
  issue no `seat_set_focus` themselves, destroyed-empty focus via the
  seat destroy-listener; history never consulted)
- `S-cos-act` cosmic-comp:src/state.rs:167 (`not_sandboxed` is
  true with no security context, panel excepted) +
  src/wayland/handlers/xdg_activation.rs:33-70 (such clients get
  Workspace tokens without/against serials) and :72-78
  (serial-less tokens from other clients get `UrgentOnly`) and
  :95-111 (stale serials are denied) and :119-172 (`UrgentOnly`
  only adds workspace-level `WState::Urgent`; workspace tokens
  follow the `ActivationPolicy`) + cosmic-comp-config/src/lib.rs:154,324-329
  (shipped default `Focus`) +
  src/wayland/handlers/xdg_activation.rs:186-217
  (`activate_surface` switches to the element workspace) and
  :255-262 (focuses the element) + src/shell/mod.rs:542-546
  (workspace activation removes `Urgent` from both sides;
  same-workspace focus clear per `S(S-cos-actclear)`)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (ordinary requests switch and focus; sandboxed serial-less marks
  only with stale-serial denial)
- `S-cos-actclear` cosmic-comp:src/shell/mod.rs:541-546
  (workspace switch removes `Urgent` from both sides only when
  `self.active != idx`) +
  src/wayland/handlers/xdg_activation.rs:124-130 (`UrgentOnly`
  adds workspace-level `WState::Urgent`) +
  src/shell/focus/mod.rs:198-220 (`set_focus` appends the focus
  stack and updates active with no `Urgent` removal) and :288-329
  (`update_active` writes activation flags only) + exhaustive
  `WState::Urgent` removal inventory (removals only at
  src/shell/mod.rs:544-545)
  @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (same-workspace focus retains the workspace-level marker; only a
  workspace switch clears it)
- `S-hyp-act`
  Hyprland:src/desktop/view/window/Window.cpp:813-825 (`activate`
  always sets the urgent hint but focuses only under
  `misc:focus_on_activate` or force) + src/config/values/ConfigValues.cpp:580
  (shipped default false) + src/desktop/state/FocusState.cpp:214-215
  (taking focus strips the urgent bit) +
  src/desktop/view/window/Window.cpp:1828-1850 (X11
  `onActivationRequest` funnels through `activate`) +
  src/protocols/XDGActivation.cpp:87-102 (Wayland xdg-activation
  dispatch calls the same `activate`)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (both request routes mark without focusing at default; focus clears)
- `S-bsp-act` bspwm:src/events.c:327-332 (`_NET_ACTIVE_WINDOW`
  focuses the located node) + src/settings.h:59 + src/settings.c:128
  (shipped `ignore_ewmh_focus=false`) + src/tree.c:645-651 (focus on
  another desktop shows it and sets `m->desk`) + src/tree.c:2230-2246
  (`set_urgent` flag write) and :604-606 (focus path clears urgency)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (hidden-desktop requests switch and focus; focus clears urgency)
- `S-i3-act` i3:docs/userguide:1369-1382 (`smart` is the default:
  visible requesters focus, hidden ones mark urgent) +
  src/handlers.c:430-442 (configure-request branch) and :800-809
  (`_NET_ACTIVE_WINDOW` branch) + src/handlers.c:696-704
  (demand-attention add/remove/toggle) + src/con.c:264-273 (focus
  resets leaf urgency with parent/workspace propagation)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (hidden requesters mark only at default; focus clears)
- `S-sway-act` sway:sway/xdg_activation_v1.c:40-52 (internal-seat
  requests activate; tokens from a focus-less client only mark
  urgent) +
  sway/tree/view.c:476-506 (`FOWA_SMART`/`URGENT`/`FOCUS`/`NONE`
  dispatch) + sway/config.c:257 (shipped default `FOWA_URGENT`) and
  :256 (`urgent_timeout` 500) + sway/input/seat.c:1093 (focus clears
  urgency) and :1225-1239 (workspace-switch focus arms the clear
  timer instead)
  @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (shipped default marks without focusing; focus clears)
- `S-qti-act` qtile:libqtile/backend/x11/window.py:615-621 (hint
  urgency sets the flag off-focus) and :658-666 (`urgent` property
  plus demands-attention setter) and :1305-1312 (focus path resets
  the flag and strips the state atom) and :2102-2108
  (`_NET_ACTIVE_WINDOW`: pager source activates, app source goes to
  `activate_by_config`) + libqtile/backend/base/window.py:594-610
  (`activate` pulls the group via `set_group` and focuses) and
  :611-632 (`activate_by_config`: shipped `smart` activates only
  same-screen requesters, else marks urgent) +
  libqtile/resources/default_config.py:197 (shipped
  `focus_on_window_activation = "smart"`) + libqtile/group.py:158-165
  (hidden groups carry `screen = None` via `hide`)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (app-source hidden-group requests mark without switching; pager
  source activates; marker without steal; focus clears)
- `S-awe-act` awesome:lib/awful/permissions/init.lua:167-178
  (`request::activate` filter gate) + :197-212 (with empty `ewmh`/
  generic filters invisible clients skip focus and `raise` marks
  `c.urgent` without switching tags) + :333-340 (`request::urgent`
  handler sets `c.urgent` off-focus) + :576-583 (sole shipped
  `add_activate_filter` is `mouse_enter`-scoped) +
  awesome:ewmh.c:497-513 (`_NET_ACTIVE_WINDOW` emits
  `request::activate` context `ewmh` with `raise=true`) +
  awesome:objects/client.c:1841-1873 (`client_focus_update` clears
  the urgent flag on focus) + objects/client.h:316-321
  (`isvisible` needs selected tags, unhidden, unminimized) +
  awesome:lib/awful/client/urgent.lua:103-107 (`focus` drops the
  urgent-stack entry)
  @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (hidden-tag requests mark without switching; marker without steal;
  focus clears)
- `S-xmo-act` xmonad-contrib:XMonad/Hooks/EwmhDesktops.hs:226-259
  (default `doFocus` activate hook focuses immediately, switching
  workspace if necessary; `doAskUrgent` marking is opt-in via
  `setEwmhActivateHook`) + :138-143 (`ewmh` default
  `activateHook = doFocus`) +
  xmonad-contrib:XMonad/Hooks/UrgencyHook.hs:574-579 (`doAskUrgent`
  opt-in marker via `askUrgent`) + xmonad:src/XMonad/Operations.hs:423-434
  (core focus path reads `WMHints` for input-focus only) with zero
  urgency handling in core (`rg -i urgent src/` hits nothing)
  @5097a457e7a409bc9a7584dc5aa82b34c69d6dda for contrib paths,
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7 for core
  (default request switches and focuses; no native urgency
  mark/clear in this profile beyond the opt-in hook)
- `S-nir-act` niri:src/handlers/mod.rs:766-804 (`token_created`:
  serial-less tokens get the `UrgentOnlyMarker`; invalid serials
  are denied unless the debug flag is set) and :806-835
  (`request_activation`: `Ignore` drops, `SetUrgent`/urgent-only
  marks, `Focus`/valid tokens call `activate_window`; the shipped
  default-config carries no `on-xdg-activate` rule so the
  urgent-only branch applies) + src/window/mapped.rs:601-609
  (`set_urgent` refuses while focused) and :390-398 (taking focus
  resets urgency)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (serial-less requests mark without focusing, invalid ones are
  denied; focus clears)
- `S-ours-act` Ours activation/urgency position at this HEAD:
  omnitiler:crates/tiler-core/src/session/world.rs:380-389
  (`sync_focus_from_window` only resyncs focus for ordinary
  activation of a known tiled window in an existing domain, failing
  closed otherwise) + kwin/src/world-apply.ts (focus arrives only via
  one-shot command effects with arrival/view proof; no
  attention/urgency signal) +
  crates/tiler-windows/src/workspace.rs:388-397 (`note_foreground`
  records foreground observation as last-focus; no flash or marker
  path exists in the adapter)
  @29bc4d9
  (ordinary-activation sync and foreground observation exist;
  unsolicited-request routing plus native mark/clear are TBD)
- `S-hyp-reload` Hyprland:src/config/shared/actions/ConfigActions.cpp:1223-1240
  (`exit` stops the compositor; `reloadConfig` re-applies config on the
  live tree, no re-exec) + src/ipc/s1/Commands.cpp:1237-1256 (`reload`
  incl `full-reset`, config only; no layout dump or re-exec verb)
  @19fb395d45314960e6f79f17994a84094f1cd4f6
  (in-place reload vs exit only; orderly re-exec recovery untraced)
- `S-i3-restart` i3:src/commands.c:1695-1725 (`restart` carries the IPC
  fd and calls `i3_restart`) + src/util.c:289-316 (`i3_restart` stores
  the layout file and re-execs with `--restart`) + src/main.c:418-440
  (`--restart` consumes the file on re-exec only) +
  src/load_layout.c:594-595 (percent readback), :574-575,763-764
  (focused flag and activation), :518-534 (floating geometry readback) +
  src/ipc.c:413,431,508-519,628-630 (dumped percent/focus/rect fields) +
  src/manage.c:44-68 (`manage_existing_windows` re-runs ordinary manage
  over mapped windows on every start) + src/main.c:1121 (startup call) +
  src/manage.c:282-283 (layout matching only with a restart file) +
  src/tree.c:66-123 (restore with missing/slurp/append fallback to init,
  placeholder open at :116) + src/main.c:923-937 (layout-path consume else
  `tree_init`) + src/load_layout.c:583-584 (`restart_mode` swallow flag)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (in-place layout-file restart; fresh start without the file recomputes
  admission, losing pre-stop explicit tiles; whole-tree dump has no
  versioned membership/partial semantics)
- `S-nir-rst` niri:niri-ipc/src/lib.rs:196-204 (`Quit` exits) and
  :936-947 (`LoadConfigFile` reloads the current/new config file only;
  no layout dump or re-exec verb in the full `Action` enum) +
  src/cli.rs:22-31 (`--session` imports environment to systemd/D-Bus and runs
  D-Bus services, main-instance only) + src/main.rs:73-96 (`--session` TTY env
  handling with `XDG_CURRENT_DESKTOP`/`XDG_SESSION_TYPE` set) and :224-243
  (session environment import plus D-Bus/a11y start) and :257 (config-file
  watcher setup, reload vs restart) and :168-169 + :260-267
  (`spawn_at_startup`/`spawn_sh_at_startup` taken from config and spawned fresh
  at startup, plus the CLI command) + resources/niri-session (session launcher
  starts niri.service/niri.target with env import and single-instance guard
  only) + resources/niri.service:14 (`ExecStart=niri --session`) +
  niri-config/src/lib.rs:73 (`spawn_at_startup` config field, fresh-spawn only,
  no layout store)
  @ed22699d99462f61ab171472d3ea67e844ea580d
  (quit plus config reload only; startup fresh-spawns with no layout restore;
  session-manager app restore untraced, H)
- `S-pap-rst` PaperWM:tiling.js:3829-3900 (`SaveState` update/prepare
  for controlled restarts: monitors, spaces, targetX plus stacking) and
  :2050-2131 (`addAll`: prevSpace columns restored verbatim where present
  with dead pruned, else workspace windows appended in `xz_comparator`
  order - x buckets plus stacking tiebreak; newcomers at `length`;
  selection is the host `NORMAL` tab-list head among indexed windows) and
  :3979-4021 (`insertWindow` re-adds with `existing: true`) +
  extension.js:57-80 (disable/enable lifecycle)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (controlled-restart staging plus existing adoption with comparator order
  and tab-list selection; settled widths per `S(S-pap-layout)`, viewport
  per `S(S-pap-view)`; enable path reuses one module-level `SaveState`
  (:93-102), disable saves via `prepare` (:207-228), and enable re-adds
  through `spaces.init` plus `addAll(prevSpace)` (:154-199, :389-395))
- `S-pap-readopt` PaperWM:tiling.js:3831-3835 (`SaveState` holds
  prevMonitors/prevSpaces/prevTargetX only, no float member) and
  :2115-2123 (`addAll`: above-or-minimized windows re-float via
  `Scratch.makeScratch`, everything else re-tiles through `add_filter`)
  + scratch.js:62-83 (`makeScratch`: float flag plus above)
  @8bf6dd264f60d6c0c402b63df7b424b888959a48
  (float state re-derives from live host above flags at re-adoption;
  list-only floats have no staged counterpart; fixed-size windows pass
  `add_filter`, so no fixed origin exists)
- `S-kar-start` karousel:src/lib/world/World.ts:75 (construction
  calls `addExistingClients`) and :92-96 (iterates `Workspace.windows`
  into `addClient` each) + src/lib/world/ClientManager.ts:30-45
  (live re-admission into the Grid, no persisted layout)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (script enable adopts existing clients in workspace order; resulting
  column order/widths untraced)
- `S-kar-rst` karousel:src/lib/keyBindings/Actions.ts:1-60 (Actions
  inventory carries focus/move/width/scroll verbs; no restart/reload/
  persist verb) + src/lib/world/ClientManager.ts:30-45 (`addClient`
  re-admits live clients into the Grid, no persisted layout)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (live-only Grid state; disable+enable recovery untraced)
- `S-pan-rst` paneru:src/ecs/restore.rs:28-60 (`SessionRestore` state
  plus grace timer) and :371-400 (`matches_startup_restore_state`
  gated on `restore_enabled`) + :246-287 (planner hard match plus
  unique title/bundle/identifier/role/subrole fallback with hard-collision
  guard plus ambiguous skip) + :482-561 (consumed-entity `Unmanaged`
  clear plus saved-strip rebuild with display remap) + src/config.rs:690-712
  (`restore_enabled` defaults true, grace default 2000ms) +
  src/ecs/triggers.rs:1064-1152 (`spawn_window_trigger` startup
  matching against the restore resource) + src/ecs/state.rs:26,301-323
  (`state.json` atomic save, version-gated load and XDG state path) and
  :83-96 (`SavedWindow` identity only: window_id/pid/psn/bundle/title/
  identifier/role/subrole; no frame/focus/float member) and
  :197-248 (extract saves strip columns only) and
  :799-826 (periodic and AppExit saves) + src/ecs.rs:175,792
  (periodic save registration and startup load)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (grace-windowed startup matching; exact strips/widths/focus untraced)
- `S-ours-kde-rst` Ours KDE restart position on the world route:
  omnitiler:crates/tiler-core/src/session/world.rs:677-700
  (`converge_observation` converges each session from a fresh complete
  observation; the session object holds no layout store) +
  kwin/src/world-entry.ts (fresh complete observation per dispatch; no
  TS layout store; float/tile-override identity seeds from the
  Rust-owned runtime store via `ReadFloatIntent`)
  @6da3d86
  (fresh admission with classified exceptions on restart; cross-login
  intent memory remains a known limitation; native focus and host-max
  restore untraced)
- `S-ours-win-rst` Ours Windows restart position at this HEAD:
  omnitiler:crates/tiler-windows/src/tiling_sys.rs:3 (product
  loop runs a full `EnumWindows` observation) and :1202-1204 (saved
  settings supply the startup base for gaps/preset only, never a
  layout reseed) + crates/tiler-windows/src/main.rs:24 (`tile` stop
  leaves geometry in place with the ledger left for explicit
  standalone `restore`; first run persists the preset choice only) +
  crates/tiler-windows/src/tiling_sys.rs:1444-1452 (intentional-float
  `floated` lifetimes are session-local, cleared on unfloat/close/stop;
  preimages session-local with stop restoring only raised bands) +
  crates/tiler-core/src/session/world.rs:677-700 (shared stateless
  converge; no layout store)
  @6da3d86
  (settings-only durability with fresh observation on restart;
  intentional floats reset because the float store is session-local;
  native focus and host-max restore untraced)
- `S-cos-wsroute` cosmic-comp:src/shell/mod.rs:2907-2945 (pending
  activation `Workspace(handle)` selects the target space, else the
  seat-active space) and src/wayland/handlers/xdg_activation.rs:62-68,95-105
  (workspace-handle token creation) @3d55cba06c9cf6f27609cdefb520f7857dba20af
  (launch routing leg; anchor/focus/switch legs are `S(S-cos-last)` +
  `S(S-cos-axis)` + `S(S-cos-mapfocus)`)
- `S-hyp-winws` Hyprland:src/desktop/view/window/Window.cpp:1270-1287
  (static `workspace` rule resolves the target workspace) and :1358-1404
  (`silent` keeps the current workspace: no `changeWorkspace`, special-workspace
  forces silent) and :1507-1513 (silent skips newcomer focus) and :1184-1192
  (`HL_INITIAL_WORKSPACE_TOKEN` env routes the newcomer to the token workspace) +
  src/config/supplementary/executor/Executor.cpp:166 (`HL_INITIAL_WORKSPACE_TOKEN`
  env issued for spawned processes) @19fb395d45314960e6f79f17994a84094f1cd4f6
  (routing plus no-switch/no-focus legs; executor env-token routing covered;
  Dwindle anchor on the target stays TBD)
- `S-bsp-wsroute` bspwm:src/rule.c:120-129 (`make_rule_consequence`
  defaults; `follow` off via calloc) and :405-406 (`desktop` consequence)
  and src/window.c:105-112 (desktop target resolves monitor/desktop/focus)
  and :205-219 (inactive target hidden, activated-not-focused without `follow`)
  @e11eff4cb3333216ad03c815609a4ed79e08929c
  (rule routing plus no-switch/no-steal legs; position leg is `S(S-bsp-insert)`)
- `S-i3-assign` i3:src/manage.c:289-316 (workspace/number assignment opens
  at `con_descend_tiling_focused` on the assigned workspace, urgency when
  invisible) and :428-442 (invisible or cross-output target takes no focus)
  @903bcd518df32b0e055b17f5da3f988a0187fd3d
  (routing plus urgency/no-steal legs; order leg is `S(S-i3-ins)`)
- `S-xmo-doshift` xmonad:src/XMonad/ManageHook.hs:124-125 (`doShift`
  moves the window via `W.shiftWin`) and src/XMonad/Config.hs:93-96
  (shipped core `manageHook` is MPlayer-only, so the fixture rule is custom)
  @284dd52c9c957cab6b6e5cc7580f2a63dafa00a7
  (rule combinator plus shipped-hook qualifier; shift/anchor legs are
  `S(S-xmo-shift)` + `S(S-xmo-admit)`)
- `S-sway-assign` sway:sway/commands/assign.c:9-62 (`assign` criteria to
  workspace/number/output) and sway/tree/view.c:628-665 (`select_workspace`
  resolves the assign target, creating it if needed) and :696-714
  (`should_focus` false across workspaces) @1652c54b73f67df17b7b4ab0b0f7048204aa8104
  (routing plus no-focus leg; sibling-anchor leg is `S(S-sway-ins)`)
- `S-qti-dgroup` qtile:libqtile/config.py:754-756 (`Group.matches`
  assigns matched windows) + libqtile/dgroups.py:148-162 (matching rule
  moves the client via `togroup`) + libqtile/backend/x11/window.py:1967-1987
  (`togroup` hides/removes/adds, switches only when `switch_group=true`)
  @83c697a5621306c3586efca31867efcfa0482e2d
  (routing plus no-switch legs; position leg is `S(S-qti-add)`)
- `S-awe-wsroute` awesome:awesomerc.lua:467-479 (shipped global rule: focus
  filter, no `switch_to_tags`) + :514-516 (tag routing is opt-in per rule) +
  lib/awful/permissions/init.lua:167-218 (`activate` focuses only when
  visible, marks urgent plus optional tag switch otherwise) and :311-329
  (`tag` handler assigns rule tags) @0a5e50cf7ee214fae47159e0e976ab4a78d2ed4f
  (routing plus no-focus/no-switch legs; order leg is `S(S-awe-tile)`)
- `S-kar-wsroute` karousel:src/lib/rules/WindowRuleEnforcer.ts:10-45
  (plugin window rules cover tile/float/caption matching only, no
  desktop target) and src/lib/world/DesktopManager.ts:44-49
  (`getDesktopForClient` tiles native single-desktop clients on their
  desktop's grid) and src/lib/world/Clients.ts:21-30 (`makeTileable`
  pins multi-desktop newcomers to the current desktop)
  @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b
  (no plugin launch-rule route; native target-grid admission is
  `S(S-kar-ins)`; target focus/no-steal/switch stay TBD; cross-output
  leg is `S(S-kar-single)`)
- `S-pan-spawn` paneru:src/ecs/triggers.rs:1277-1305 (fresh spawn inserts
  into the active strip at the config `insertion()` index or after focus,
  then focuses unless `dont_focus`) and src/ecs.rs:857-867 (`insertion()`
  is a strip index, not a workspace selector)
  @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269
  (no workspace-targeted launch rule; remembered-strip path in
  `window_managed_trigger` is re-manage only)

## Variant hooks (selected status where decided; otherwise provisional)

| Hook | Meaning | Status |
|---|---|---|
| V-INS-AXIS | New-window split axis: long-edge vs orientation-toggle vs alternate | Selected as user statement `D-dec-x` |
| V-MOVE-PERP | Perpendicular move: COSMIC restructure vs no-op/swap | COSMIC R1 selected; foreign swap/no-op unselected (`D-dec-cos`) |
| V-MOVE-NARY | 3+-child wrap vs flat swap; same-orientation nesting allowed | Ordered N-ary + R2b/R2c/R3 selected (`D-dec-cos`); global `sameAxisMove` / `core.same_axis_move`, `group-with-neighbor` default (label `Group with neighbor`, tooltip COSMIC) or `swap-with-neighbor` (label `Swap with neighbor`, tooltip i3, sway) for R2c adjacent direct leaf siblings only, shares travel with windows; leaf/group rules unchanged. Shared core/KDE behavior and functional IDs [delivered offline](../changes/archive/admission-and-move-settings.md); Windows settings wiring/native journey pending (R-MOV-03/09/10). Exact IDs selected User 2026-10-08; breaking pre-release configs acceptable, no migration or aliases |
| V-WS-FOLLOW | Send follows focus vs leaves focus in source | `D-dec-cos` selects verified follow; USER 2026-10-07 item 2: numbered/relative follow defaults, bindable unbound stay, target resolved once in existing-order ring; shared core/KDE [delivered offline](../changes/archive/kde-workspace-send-follow-stay.md), floating-boundary follow gap repaired, item-2 native journey/Windows wiring pending (R-WS-18..20). Item 5 explicit output follow/stay [KDE delivered offline](../changes/archive/four-direction-output-transfer.md); [Windows delivered offline](../changes/archive/windows-explicit-output-send.md) `fda618f`, 2026-10-14, ordinary current-target admission/commit-last follow/source-MRU-null stay; physical two-output journey pending (R-OUT-04/07) |
| V-WS-SHELL-ACTIVATE | Shell selection of another workspace's window: switch workspace vs pull window | KDE native configured policy (default switch); Windows option unselected (`D-alt-tab`) |
| V-WS-ANCHOR | Target anchor: remembered-leaf vs focus-history vs root; axis by long edge | Selected rule (`D-dec-x` + `D-place` synthetic proof) |
| V-FLOAT-GEO | First-float geometry: centered 60% vs app frame vs tile share | `D-dec-ww` selects centered-60% first, retained after |
| V-FLOAT-FOCUS | Separate tile/float directional layers vs cross-layer targets vs refusal | User 2026-10-05 selects COSMIC float/sticky top-left-axis search, existing project edge behavior; KDE offline delivered, Windows pending `D-float-nav` |
| V-FLOAT-SNAP | Float directional move: half/quarter/maximize/transfer vs pixel move vs refusal | User 2026-10-05 selects COSMIC; KDE first half-snap delivered, later stateful transitions and Windows pending `D-float-nav` |
| V-FLOAT-REFLOW | Float-removal survivor reflow: equalize vs ratio-preserve | Provisional, to discuss |
| V-STICKY-SCOPE | Sticky scope: all-workspaces floating-only vs monitor-desktop | `D-ref` recommends Hyprland/COSMIC; ours selects all-ws float-only; user 2026-10-07: Windows sticky survives restart with R-RST-01 (implementation pending; KDE already retains sticky) |
| V-MAX-MODEL | Maximize: retained-slot overlay vs layout reflow vs no state | Selected: retained-slot overlay (`D-dec-ww` KDE + `D-dec-max`); [Q3](../decisions.md#window-state-float-sticky-maximize-fullscreen) includes R-MAX-03 (stays maximized over reserved slot), KDE [delivered offline](../changes/archive/kde-maximized-floating-retile-overlay.md) and Windows [delivered offline 2026-10-12](../changes/archive/windows-maximized-floating-retile-overlay.md). R-MAX-06 changed 2026-10-11 to default tile-on-launch with overlay alternative ([Windows setting](../changes/archive/windows-born-maximized-setting.md)); R-MAX-03 independent. Native journeys TBD |
| V-FS-SLOT | In-place fullscreen: retain slot vs remove/reflow | Retained slot selected (`D-dec-ww`); born-fullscreen is a separate future row |
| V-START-SEED | Startup non-fitting topology: centre-cut inference vs long-edge seed | Selected: sequential long-edge seed, no centre inference (startup hybrid User 2026-10-08) |
| V-START-MIN | Minimum-infeasible writes: origin+minimum vs skip vs float | B6 selected on both platforms, no setting (user 2026-10-07); KDE [delivered offline](../changes/archive/kde-minimum-origin-placement.md), native journey TBD (`D-place`; [shared minimum projection](../../crates/tiler-core/src/size_hints.rs)) |
| V-CLOSE-FOCUS | Removal focus: source-MRU top vs spatial neighbor vs target history | `D-dec-cos` selects source-MRU top |
| V-GROUP-STACK | Tabbed stacks: supported vs fail-closed refuse | User 2026-10-07: tabs first after 0.1; close active tab keeps group, activates next (COSMIC/Hyprland/i3/sway); until then refuse closed (`D-dec-cos`) |
| V-R4-DIR | Exhausted directional move: cross-output vs no-op vs workspace cycle | USER 2026-10-07 item 5 as replaced by User decision 2026-10-09: local restructure/swap/escape first, then all-four-direction crossing including sole root leaf; reciprocal edge-touch + positive overlap on FULL output rectangles, horizontal too; window-based selection (shared edge containing moving window centre projection, else larger window-span overlap, final left/top tie-break); whole-workspace migration largest shared edge then left/top; unreadable topology refuses, no candidate no-op, no wrap. Shared core/KDE selection [delivered offline](../changes/archive/position-based-output-selection.md); work-area placement retained; occupied R4 landing is whole-target wrap nearest source, no flat append or remembered-leaf insertion, empty takes mover alone (User decision 2026-10-14; [wrap decision](../changes/kde-dual-output-live.md#whole-target-r4-wrap-decision-2026-10-14)); Windows directional wiring [delivered offline 2026-10-13](../changes/windows-multi-output-foundation.md); [selector audit and explicit-send integration](../changes/archive/windows-position-based-output-selection.md) `fda618f`, 2026-10-14. Physical journeys and item-14 migration integration remain pending (R-MOV-08/11..13) |
| V-DRAG-ZONE | Drop zones: edge/interior/stack mapping; centre-stack refused | `D-dec-cos` + `D-dec-nest` select split-only |

## Coverage accounting

- 158 scenarios: 58 historical plus 67 expansion additions (125), 14
  discriminators for USER selections 2026-10-07 (items 1-5, including 1.5;
  R-WS-15..20, R-MOV-09..13, R-LAY-05/06, R-OUT-07), 8 Q2 discriminators
  R-SPC-06..13, 2 Q3 restart research discriminators, 6 Q4 migration
  discriminators R-WS-21..26, and R-WS-27 two-candidate migration selection
  (User decision 2026-10-09, reference outcomes TBD), plus 2
  admission-geometry evidence rows (R-MIN-04, R-INS-09; D1-D10 selected
  2026-10-17, core/KDE delivered offline, native outcomes TBD):
  58+67+14+8+2+6+1+2 = 158.
  Their 196 Then bullets distinguish selected targets (with delivery evidence where available)
  from reference outcomes that were TBD at addition. Established source-fill
  legs now vote under the ordinary evidence rule; current counts are in the
  [source-fill consensus supplement](../research/reference-wm-consensus.md#pinned-source-fill-supplement-2026-10-09).
  Eight Q2 discriminators R-SPC-06..13 added 2026-10-08 contribute 112 Then
  bullets; D1-D8 are user-selected NORMATIVE
  (User 2026-10-08; D1/D5/D6 delivered offline, D7 implementation pending),
  KDE is implemented offline; R-SPC-11/12 include minimal changed-hint/predicate,
  repeated-exit and maximized-enable discriminators with reference/native TBD,
  unsupported reference/native outcomes stay TBD; the source-fill supplement
  counts only established discriminating legs, without changing selections.
  Six Q4 migration discriminators R-WS-21..26 added 2026-10-08 contribute 84
  Then bullets; D1-D9 are user-selected NORMATIVE
  (User 2026-10-08; D8 carry implementation pending
  plus live check), KDE is implemented
  offline. Pinned-source transfer and carry legs are filled; unsupported
  native/runtime and fixture-dependent outcomes remain TBD.
- Baseline expansion accounting: 1198 coverage cells: 67x14 new, 58x4 scrolling assessments,
  and 2x14 explicit-swap legs. Mutually exclusive semantic status totals:
  evidenced 372, partial 237, TBD-only 224, qualified-only 328, mixed 37.
  Mixed includes separate qualified/applicable legs; it does not mean a no-op.
- 522 historical wide-table cells at baseline `e160894` (eight references
  plus combined Ours per row) are migrated to GWT without retrospectively
  assigning the new status classes. Present form: 58 historical scenarios
  x 14 profiles = 812 Then bullets; 158 scenarios x 14 = 2212 Then bullets
  (+28 explicit-swap-leg bullets). Expansion record's total coverage count
  remains 1720 as baseline provenance; baseline assessed cells were not a
  uniform 125x14 grid. Baseline semantic-status totals above exclude the 22
  decision discriminators and the later Q3/Q4/R-WS-27 additions.
- [Archived expansion record](../changes/archive/reference-matrix-expansion.md)
  holds final accounting, source/inventory/native-test queue and residual work.

## Area files

Scenario rows live in area files under `reference-outcomes/` (58 historical
scenarios, migrated to GWT; plus 6 insertion scenarios from piece B1, 4 focus
scenarios from piece B2, 3 move scenarios from piece B3, 4 resize
scenarios from piece B4, and 4 layout-command scenarios from piece B5,
plus 7 workspace scenarios, 3 minimize scenarios, 2 maximize scenarios,
2 groups scenarios, 3 floating scenarios, 3 close scenarios,
4 multi-output scenarios, 3 mouse scenarios, 5 special-windows scenarios,
2 activation scenarios, 2 restart scenarios and 10 column scenarios, GWT
only: 125 expansion-baseline scenarios, plus 14 decision discriminators
2026-10-07, 8 Q2 fixed-size discriminators, 2 Q3 restart research
discriminators and 6 Q4 workspace migration discriminators 2026-10-08:
156 scenarios total (R-WS-27 two-candidate migration selection added User
decision 2026-10-09 with reference outcomes TBD), plus R-MIN-04 and
R-INS-09 (158 total; D1-D10 selected 2026-10-17, native outcomes TBD).
This index retains purpose, row-addition rule, notation,
profiles, evidence tags/legend, variant hooks, and deferred. All 158
scenarios use the GWT form below; no wide-table rows remain.
Areas follow the approved priority order; column mechanics follows, and
minimum-size stays a supplemental file (not nested in resize).

| Area | File | Scenarios | Candidates |
|---|---|---|---|
| Insertion | [insertion.md](reference-outcomes/insertion.md) | R-INS-01..09 (9) | R-INS-09 fit-first/spatial-focus-last atomic mixed seed selected D8, core/KDE delivered offline; exact native burst order TBD; R-INS-03..08 landed in piece B1 |
| Focus | [focus.md](reference-outcomes/focus.md) | R-FOC-01..04 (4) | none (landed in piece B2) |
| Move | [move.md](reference-outcomes/move.md) | R-MOV-01..13 (13) | R-MOV-09..13 added 2026-10-07; reference source legs filled, unsupported legs TBD; KDE items 3/5 delivered offline; Windows item 3 delivered 2026-10-11 (`755aab8`), native Settings/no-rebuild observed, physical moves pending; Windows item 5 pending |
| Resize | [resize.md](reference-outcomes/resize.md) | R-RSZ-01..04 (4) | none (landed in piece B4) |
| Layout commands | [layout-commands.md](reference-outcomes/layout-commands.md) | R-LAY-01..06 (6) | R-LAY-01/05/06 KDE implemented offline; Windows item 4 delivered 2026-10-11 (base `755aab8` + delivery commit), native Settings/owner adoption observed, physical toggle/OS suppression pending; reference source legs filled, unsupported legs TBD |
| Workspaces | [workspaces.md](reference-outcomes/workspaces.md) | R-WS-01..27 (27) | KDE items 1/2 delivered, item 1 single-output user-confirmed, item 2 offline only; Q4 R-WS-12/21..26 implemented offline under user-selected NORMATIVE D1-D9; R-WS-27 selection delivered offline in shared/KDE and Windows. Windows local R-WS-12/21..27 including D8/G-37 delivered offline 2026-10-14 ([record](../changes/archive/windows-workspace-output-migration.md)); non-local item 12 including mode-aware hotplug offline-complete 2026-10-15, single-output native CLI regression passed in all modes ([record](../changes/archive/windows-workspace-scope-hotplug.md)); physical two-output journeys and unsupported outcomes TBD |
| Minimize | [minimize.md](reference-outcomes/minimize.md) | R-MNZ-01..03 (3) | none (landed) |
| Maximise / fullscreen | [maximize-fullscreen.md](reference-outcomes/maximize-fullscreen.md) | R-MAX-01..09 (9) | none (landed with scrolling backfill) |
| Groups / stacks | [groups-stacks.md](reference-outcomes/groups-stacks.md) | R-GRP-01..03 (3) | none (R-GRP-02..03 landed with scrolling backfill) |
| Floating | [floating.md](reference-outcomes/floating.md) | R-FLT-01..14 (14) | none (R-FLT-12..14 landed with scrolling backfill) |
| Close / reflow | [close.md](reference-outcomes/close.md) | R-CLOSE-01..05 (5) | none (R-CLOSE-03..05 landed with scrolling backfill) |
| Multi-output | [multi-output.md](reference-outcomes/multi-output.md) | R-OUT-01..07 (7) | R-OUT-07 added 2026-10-07; reference source legs filled, unsupported legs TBD; KDE item 5 R-OUT-01/04/07 delivered offline; native journey/Windows wiring pending |
| Mouse | [mouse.md](reference-outcomes/mouse.md) | R-DRAG-01..08 + R-MOU-01..03 (11) | none (R-MOU-01..03 landed with scrolling backfill) |
| Special windows | [special-windows.md](reference-outcomes/special-windows.md) | R-SPC-01..13 (13) | R-SPC-04/06..13 KDE and Windows implemented offline under selected D1-D8/G-05; Windows prerequisite a41fcb5 plus runtime delivery 2026-10-11. R-SPC-11/12 lifecycle/enable and R-SPC-13 fixed-origin maximized restart discriminate readings; Windows hydrated-override overlay route is approved shared policy User 2026-10-17, option A (not implemented). Native and unsupported reference outcomes TBD |
| Activation | [activation.md](reference-outcomes/activation.md) | R-ACT-01..02 (2) | none (landed; no backfill: no prior rows) |
| Restart / persistence | [restart-persistence.md](reference-outcomes/restart-persistence.md) | R-START-01..03 + R-CTL-01..07 + R-RST-01..04 (14) | Q3 KDE intentional membership implemented offline under user-selected NORMATIVE D1-D4 (User 2026-10-08; fixed-window tile-override persistence pending); R-RST-03/04 cover frame drift and automatic-vs-intent origin; startup-vs-restart-vs-reload discriminator table added (no new scenario); native, Windows and unsupported reference legs TBD |
| Column mechanics | [column-mechanics.md](reference-outcomes/column-mechanics.md) | R-COL-01..10 (10) | none (landed) |
| Minimum-size (supplemental) | [minimum-size.md](reference-outcomes/minimum-size.md) | R-MIN-01..04 (4) | R-MIN-04 geometric best effort selected D1-D10, core/KDE delivered offline, B6 unchanged; native outcomes TBD; R-MIN-01..03 piece B4; R-MNZ icon-minimize is separate |

## macOS drag/drop discriminators

These qualify the existing R-DRAG rows, not a new reference behavior selection.
Native/physical outcomes remain TBD; offline controller tests are not host proof.

| Given | Minimal action sequence | Discriminator | macOS outcome |
|---|---|---|---|
| Two owned tiled windows A/B; frames re-read after tiling | Meta+left press on a freshly joined A point; move toward two different B edges; release | Existing Meta mapping, changing insertion-slot preview, committed Engine topology and applied frames agree | TBD; product arm is hard parked, no successful live drag/drop leg |
| The same A/B baseline, reset independently | Native titlebar press on A; move to the same B edge; release | Native move source has the same core result without confusing resize/programmatic movement | TBD; affirmative native-move classifier not selected |
| The same baseline and an active proven drag | Escape; release mouse; click the owned receipt window | No layout mutation, overlay gone, balanced release and subsequent input receipt | TBD; synthetic drag/paired-click routes refuse before posting pending a balanced exact-owned release contract |
| Two owned windows on different outputs/workspaces | Drag A toward B, then independently repeat over native fullscreen | Cross-domain targeting versus refusal; no fullscreen overlay/input interference | TBD; cross-output integration and product fullscreen proof absent |
| An owned helper moved by tiling; old pointer coordinates no longer hit it | Attempt a guarded synthetic edge at the old point | Refuse without posting to the unrelated point owner | Pure guard regression passes; live correction not repeated; earlier stale-coordinate attempt may have reached the protected terminal |

See [the incomplete change](../changes/macos-drag-drop-overlays.md) for the live
failures, restoration limits and disabled diagnostic surfaces.

## Scrolling column notation

Additive; the existing H/V/S notation is unchanged.

- `COL[C1[A],C2[B*,C],C3[D]]`: ordered columns, with C2's B/C vertically
  visible. Column identifiers persist through reordering.
- `COL[C1[A],C2[S[B*,C]]]`: tabbed-display column, distinct from both
  vertical visible stacking and a tree tab group. Accordion/overlapping
  display is an explicit profile-qualified rendering, never silently
  called tabs.
- Widths: `w(C1)=0.5W`, where W is viewport work-area width; absolute
  pixels only when load-bearing. Internal heights use ratios when needed.
- Viewport: `VP(x=0,W=2400)` is the strip-coordinate interval `[0,2400)`.
  State column positions/widths and gaps when clipping/ties matter; focus
  and viewport are separate state variables. No ambiguous inline boundary
  marker.
- Workspaces/outputs reuse `L:WS1=COL[...]`; paneru adds
  `Space1:{VW1=COL[...],VW2=COL[...]}`. Native Space and virtual row are
  separate domains.

## Cross-model projection rules

- H/V fixtures remain exact split-tree fixtures. A flat H has a matching
  projection to columns only if widths, visible windows, and viewport
  match; that proves rectangle-level behavior, not a split-tree. V can map
  to a visible multi-window column under the same qualification. Recursive
  H/V ancestry usually has no column counterpart.
- S maps only to a supported tabbed-display fixture with stated membership
  and active tab; PaperWM accordion and visible vertical columns are not
  exact S equivalents. Never flatten nesting just to obtain a result.
- Backfill assesses every existing row for each scrolling profile with a
  separately stated column Given; projections are marked explicitly and
  link any distinct column candidate.

## Outcome qualifiers

- `fixture-inapplicable`: no faithful start exists in that profile.
- `no-counterpart`: evidenced missing action or state (needs pinned
  inventory evidence).
- `owner-specific`: external journey outside the WM profile.
- None of these counts as an agreeing no-op. Applicable unknown outcomes
  stay TBD. Existing cells keep their current vocabulary.

## New-scenario format (GWT)

- Scenarios use Given/When/Then with one Then bullet per profile (14):
  COSMIC, Hyprland/Dwindle, bspwm, i3, xmonad/Tall+Navigation2D, sway,
  qtile/Columns, awesome/tile, niri, PaperWM, karousel/Lazy, paneru,
  Ours KDE, Ours Windows. Ours KDE and Ours Windows always have separate
  Then entries, never a combined verdict.
- Model-specific Given bullets and independently reset variant legs; never
  pretend every WM can instantiate one H/V fixture.
- This convention applies to every scenario, including single-step
  predicates. All 58 historical scenarios are migrated to this form;
  no wide-table rows remain.
- Attach `S(real-key)`, `D(real-key)`, `UT(actual-date)` only to
  established predicates. Partial evidence qualifies its leg; TBD stays on
  the unsupported part. Never invent citation keys.

## Scrolling baselines (VERIFIED at pinned sources)

Pins are source-read anchors, never retroactive user-test versions. The
eight existing WM pins are unchanged. Checkouts verified at exactly these
commits; PaperWM.spoon stays corroboration only, never a separate profile.

- niri @ed22699d99462f61ab171472d3ea67e844ea580d `S(S-nir-base)`:
  resources/default-config.kdl:123 (`center-focused-column "never"`),
  :129-132 (default presets 1/3, 1/2, 2/3 of output), :142
  (`default-column-width { proportion 0.5; }`).
- PaperWM @8bf6dd264f60d6c0c402b63df7b424b888959a48 `S(S-pap-base)`:
  shipped schema `open-window-position` default 0 (RIGHT of current
  window), `default-focus-mode` default 0 (DEFAULT). README right-of-active
  and `tiling.js` FocusModes corroborate; the schema is authoritative.
- karousel @8b9f0b62b2922703d7c25a79d5d49ae93cd3f93b `S(S-kar-base)`:
  src/lib/config/definition.ts:108-111 (`presetWidths` "50%, 100%"),
  :133-136 (`stackColumnsByDefault` false), :153-156 (`scrollingLazy`
  true), :158-166 (`scrollingCentered`/`scrollingGrouped` false).
- paneru @b1b6abbd3f1a4be138152b6f0389c9ff1b27a269 `S(S-pan-base)`:
  src/config.rs:1282-1284 (default width presets 0.25, 0.33333, 0.50,
  0.66667, 0.75, 1.0, 1.5, 2.0), :799-808 (focus-follows-mouse and
  mouse-follows-focus enabled), :822-828 (native tabs enabled), :856-862
  (one workspace, append admission).

## Windows output-send held-key discriminator

- User 2026-10-14 approved as delivered ([decision](../decisions.md#move-layout-and-output-commands)); offline evidence `fda618f`, 2026-10-14.
  Fresh action-sequence sub-leg of R-OUT-04/07, not a reference vote.

| Given | When | Windows offline outcome | Physical/reference outcome |
| --- | --- | --- | --- |
| Managed tiled A; readable adjacent output; default follow row | Hold Win+Ctrl+Alt+Right through down, repeat, release; fresh reset with Shift also held | Live repeats dispatch; release is consumed without dispatch; default shifted follow refuses. Modifier drift ends repeat dispatch while preserving paired release verdict | Windows physical suppression/repeat and KDE/reference held-key outcomes TBD |
| Stay row rebound to Win+Ctrl+Alt+Shift+F6 | Down, repeat, release with exact modifiers; fresh reset without Shift | Only exact rebound arm dispatches stay, using the directional letter hold slot; source MRU/null focus and visible target membership retained | Physical rebound adoption/containment and KDE/reference modifier outcomes TBD |

## Deferred areas

- All approved behavioral areas are assessed, including ratio balance
  (R-RSZ-04), workspace lifecycle (R-WS-03/10/13), and column mechanics
  (R-COL-01..10). Scrolling uses its own model, never assumed H/V equivalence;
  PaperWM.spoon remains corroboration, not a separate profile.
- Applicable unknown outcomes remain TBD. Source/inventory checks and native
  journeys are grouped by environment in the archived expansion record;
  qualified absent fixtures/verbs are not live no-op cases.
- Product choices remain pending batch review of the consolidated
  [consensus Table A](../research/reference-wm-consensus.md#table-a-strong-cross-family-consensus-where-ours-differs-24-consolidated).
- Historical-cell status recensus remains separate work; no retrospective
  status classes are assigned to migrated historical cells. Wide-table
  migration itself is complete.
- Gaps/borders/corners/active indication: metrics exist (`D-ref`
  section 9) but are styling, not behavior variants; out of scope.
- Fullscreen games bypass: all three agree cover-and-restore
  (`D-ref` section 10); no discriminating row needed now.
