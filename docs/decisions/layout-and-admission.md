# Layout and admission

### Placement and minimums
- Decision: Default split is long-edge (tall target vertical, wide horizontal), including sends. Minimum hints affect target rectangle/allocation, not alternative-axis/target search. V4 honors feasible minima; infeasible minima retain proportional allocation under core clamp settlement, not adapter-raised extents. No global optimizer or alternative-axis search. User 2026-11-09: KDE M18 uses the v4 proportional-allocation/clamp core-settlement oracle instead of B6 raised-minimum writes and adapter reassertions; all native minimum/origin/convergence journeys remain and M18 stays must-pass. Startup preserves clean/tolerance-valid recursive cuts, otherwise deterministic sequential long-edge seed, no center inference.
- Why: not recorded.
- Assumes / revisit if: clean prior 2x2/nested identities retained; no guaranteed balanced 2x2; deliberate deviation from references that ignore minima.
- Status: active, 2026-11-09
- Evidence: [Windows placement](../changes/archive/windows-placement-correctness.md), [historical KDE B6](../changes/archive/kde-minimum-origin-placement.md), [core settlement](engine.md#core-derived-native-settlement), [approved KDE acceptance](../changes/kde-v4-acceptance-checklist.md#approved-acceptance-decisions).

### Host-settled geometry design timing
- Decision: defer shared host-settled geometry research/design until after macOS slice 1; bring the design for user approval before slice 3. Approval is still required before any core change; this timing decision does not approve a settlement rule.
- Why: restore a usable v4 macOS baseline before designing from host evidence.
- Assumes / revisit if: keep one core-owned, platform-neutral solution; the learned-size-limits candidate remains rejected for complexity.
- Status: active, 2026-11-10; design pending
- Evidence: User 2026-11-10; [pending design and evidence cases](../changes/learned-size-limits.md#original-backlog-lines-1038-1060), [macOS slices](../changes/v4-phase3-macos-port.md#proposed-dogfoodable-slices).

### Inactive destination admission
- Decision: honor a newcomer's observed destination membership without adding an app-to-workspace rule API. Ordinary admission uses the destination's remembered tiled leaf, then existing destination fallback, never the source's global focus. A birth on a workspace not currently displayed on its output consumes automatic admission focus without activating it or switching views, and never replays after return. Existing explicit sends/follow and fresh-placement settings remain unchanged.
- Why: COSMIC, i3 and sway use target-local remembered/inactive tiled focus; Hyprland's inactive Dwindle anchor instead uses cursor proximity. All four inspected silent/inactive routes withhold focus/switch.
- Assumes / revisit if: native routing rules are host input, not a new OmniTiler rule subsystem. Native pickup and exact frames remain TBD; no new behavior assumption or setting.
- Status: active; decided under the standing Tier 1 alignment rule (User 2026-11-13).
- Evidence: [release-qualified inactive-destination discriminator](../spec/reference-outcomes/insertion.md#r-ins-07-open-with-a-destination-rule-into-an-inactive-workspaceoutput), [batch 3](../changes/c9-product-evidence.md#requirement-batch-3-admission-and-fullscreen-2026-11-13).

### Windows declared minimum hints
- Decision: Query fresh application-declared minimum track sizes and provide visible-frame physical-pixel hints to shared minimum-aware projection. Do not learn limits; generic learned limits remain parked. On query timeout, reuse each live window's last-good declared hint without a time bound; replace only on successful query, including success with no usable hint, and drop on close or identity change. Failed, unperformed, and budget-skipped queries remain unknown without erasing last-good state.
- Why: not recorded.
- Assumes / revisit if: refused-attempt suppression remains. No geometry-inferred limits, timer/expiry, learned limits, or app-wide/persistent floors.
- Status: active, 2026-10-16
- Evidence: [placement correctness](../changes/archive/windows-placement-correctness.md), [timeout fallback](../changes/windows-launcher-geometry-fighting.md).

### Windows hint diagnostics and retained overlays
- Decision: Bounded, rate-limited per-window diagnostics join class/executable basename only (never title, full path, or content), hint query outcome/timeout/fallback, requested/readback rectangles, and geometry suppression/re-arm reason. Sample changed/problem states at most once per second, 16 entries per batch, with operation context, coalescing/omission counts, and fair pending delivery. Tiled maximized/fullscreen members retain last-known declared hints by lifetime token and canonical slot until fresh queries resume; floating/born-slotless, minimized, and cloaked rows do not reuse hints. Successful asynchronous Win+M restore dispatch arms one bounded two-second completion wake on the existing 100ms pump; reconcile once restore is observed.
- Why: not recorded.
- Assumes / revisit if: wake is an observation window, not hint expiry or latency guarantee; physical timing remains user-owned. No extra waits or live probes for diagnostics.
- Status: active, 2026-10-16
- Evidence: [placement correctness](../changes/archive/windows-placement-correctness.md), [geometry fallback](../changes/windows-launcher-geometry-fighting.md).

### Fixed-size admission predicate
- Decision: Require both min/max vectors present, usable and nonnegative; retain full-zero and unbounded-sentinel guards, never infer from `resizeable`. KWin unbounded marker means no limit even though references compare raw values and ten including COSMIC float raw-equal sentinels (deliberate host adaptation). Both-axes-fixed default; either-axis-fixed alternative. Zero is unset per axis; fixed axis needs equal nonzero bounds, so min=max=(640,0) is not fixed for both-axes but is for either-axis. Both predicates retain other hint guards; only subsequent admissions change. Functional labels `Width and height both fixed` / `Width or height fixed`; tooltips name COSMIC / Hyprland (Wayland) and sway; no pre-release migration.
- Why: not recorded.
- Assumes / revisit if: KWin sentinel is host adaptation; COSMIC partial-zero raw equality is deliberately not followed. See canonical same-axis setting in [commands](commands.md) only if needed.
- Status: active, 2026-10-10
- Evidence: [predicate implementation](../changes/archive/reference-comparison-implementation.md), [admission](../changes/archive/windows-fixed-size-admission.md).

### Fixed-size admission state transitions
- Decision: Hint changes are admission-only; retain self-resize reaction to avoid gaps/overlaps. Explicit tile/sticky-off-to-tile wins for same live client; new client classifies anew. Born-maximized fixed uses floating base under native maximize; born-fullscreen first exit freshly classifies, fixed floats without writes; fixed-floating restores float. Workspace enable examines every candidate including floating-workspace arrivals; fixed remains untouched float, others tile; explicit tile override wins. Fixed user tile overrides persist across owner restart with recompute fallback.
- Why: not recorded.
- Assumes / revisit if: preserve D2 convergence correction: known automatic float remains floating through fixed-hint loss unless observed suppression, retained fixed tile override or explicit ToggleFloat gives tile win. User-approved durable tile base under native maximize is active shared policy but implementation pending.
- Status: active, 2026-10-25
- Evidence: [fixed-size exit/enable](../changes/archive/fixed-size-exit-and-enable.md), [override restart](../changes/archive/fixed-window-tile-override-restart.md).

### Fixed-size D2 drift and hint-loss correction
- Decision: Hint changes affect admission only in both directions; continue reacting to clients resizing themselves to avoid gaps/overlaps. F1 additionally promptly observes tiled frame drift and reconciles to retained tile geometry under safety, clamp and write-suppression rules, without a write loop against a client repeatedly resizing.
- Why: not recorded.
- Assumes / revisit if: no new hint-signal work. Known automatic float stays floating across fixed-hint loss, including native `floating=false`, unless observed suppress signal, retained fixed tile override or explicit ToggleFloat supplies genuine tile win.
- Status: active, 2026-10-29; F1 implementation pending.
- Evidence: [client drift discriminator](../spec/reference-outcomes.md#client-driven-tiled-geometry), [hint-loss correction](../changes/macos-world-v2-port.md#approved-d2-core-correction-2026-10-25).

### Fixed-size D3-D7 user tile override
- Decision: Explicit tile or sticky-off-to-tile wins for same live client through hide/show, cross-domain observation and workspace re-adoption; new client classifies again. Sticky origin/toggle rules stay unchanged. Startup classifies foreground and hidden windows. Fixed-window user tile override persists across owner restart in same intentional-float store with recompute fallback; only settled tile wins over known automatic fixed origin write `OmniTiler.TileOverride.v1`, generic non-fixed unfloats do not. Hydrate visible and retained/maximized admitted lifetimes before automatic classification; marker needs no hints and grants no write/recovery authority.
- Why: not recorded.
- Assumes / revisit if: D4 fixed+born-maximized uses floating base under native maximize. D5 first exit from born-fullscreen freshly classifies: fixed floats without writes, otherwise tiles; previously fixed-floating restores float. No unsolicited fullscreen writes; explicit maximize replacement only narrow R-MAX-11 exception. D6 enable checks every window including floating-workspace arrivals: fixed remains/becomes untouched float, others tile; explicit tile override remains tile. Intentional/sticky float rules unchanged.
- Status: active, 2026-10-17; native/game checks pending.
- Evidence: [fixed-size transitions](../changes/archive/fixed-size-exit-and-enable.md), [restart override](../changes/archive/fixed-window-tile-override-restart.md).

### Fixed-size D7 durable maximize base
- Decision: Preserve durable tiled base beneath native maximize under either `core.born_maximized` value, including hint loss. This is shared policy affecting all targets, including KDE; Windows interim remains untouched until shared delivery.
- Why: not recorded.
- Assumes / revisit if: Tier 1 research found no exact durable-override/restart/maximize counterpart; neither shared precedence nor setting-led clearing is reference consensus. R-SPC-13 native discriminator stays TBD.
- Status: active, 2026-10-17; implementation not delivered.
- Evidence: [fixed override research](../research/fixed-override-born-maximized.md).

### Automatic fixed-float stacking and gaming safety
- Decision: KDE automatic fixed floats stay above tiles without geometry/focus writes; never write fullscreen clients or cover focused fullscreen game. Preserve pre-existing user/app keep-below (zero writes) and keep-above (untouched). Later keep-below or clearing project-owned above wins without reassertion; suspend owned above while focused fullscreen client scope is active, reacquire after exit, restore on lost automatic classification or explicit tile subject to overlay safety. Stop attempts safe restoration only.
- Why: not recorded.
- Assumes / revisit if: unload/restart ownership is answered by [R12](contract.md#native-writes-difficult-to-undo-r12): the adapter records original state in a host slot outliving the tiler before setting project keep-above, and the next start restores leftovers, so pre-existing above stays untouched; KWin unload without a later start leaves state until next start. Windows automatic floats remain membership-only with no writes.
- Status: active, 2026-10-29
- Evidence: [stacking investigation](../changes/kde-automatic-float-stacking.md), [contract](contract.md#native-writes-difficult-to-undo-r12).

### Core-owned keep-above desire
- Decision: Core owns fixed-size eligibility and suspension, and publishes neutral per-window `keepAboveTiles: null|false|true`: null grants no write authority; false releases only project-owned above; true requests above-tiles with no activation/geometry. Never clear user above, force below or own pre-existing above. Adapters report neutral above/below/ownership facts and apply mechanically; delete KDE-local fixed-size predicate consumer. Preserve below/external-clear no-reacquire; never cover focused fullscreen window. Windows shares codec but automatic-fixed stacking remains opt-out.
- Why: not recorded.
- Assumes / revisit if: coordinated v3 if ready, otherwise exact version bump; implementation pending. Unload/restart ownership is answered by [R12](contract.md#native-writes-difficult-to-undo-r12), not an open blocker.
- Status: active, 2026-10-29; implementation pending.
- Evidence: [approved core shape](../changes/kde-world-v2-port.md#recommended-core-shape-alternatives-and-cutover), [stacking safety](../changes/kde-automatic-float-stacking.md).

### Admission activation and placement
- Decision: Admit newcomer structurally behind retained maximized/fullscreen window and reserved slot. Host decides activation/focus/stacking, including approval, denial and explicit user switches; this ownership applies to maximized and fullscreen windows. Never write fullscreen geometry or actuate focus when fullscreen is involved. First admission fills work area and natively activates newcomer. Ordinary float-focused admission anchors nearest tiling neighbor and uses ordinary long-edge admission, not root-wrap. Apply selected INS-01b After focused per admission leg; no Before focused setting.
- Why: not recorded.
- Assumes / revisit if: native cleanup, additional admission legs and KDE/macOS output targeting remain live-pending.
- Status: active, 2026-10-12
- Evidence: [fullscreen admission](../changes/archive/kde-fullscreen-activation-and-send.md), [triage batch](../changes/archive/triage-batch1-kde-shared.md).

### Reopened-window admission
- Decision: A reopened client is fresh admission at current focus; never restore its old slot.
- Why: not recorded.
- Assumes / revisit if: not recorded.
- Status: active, 2026-10-10
- Evidence: [release triage](../research/release-0.1-triage.md#user-decisions-2026-10-12).

### Admission-focus batch invariance
- Decision: In a batch, core selects the last newcomer not withheld for automatic fixed-size handling, skipping and spending withheld fixed newcomers. If all newcomers are withheld, admission focus is unset. Across causes in one report, precedence is explicit-command, then automatic-admission, then positive-close, then focus-follows-cursor: a close-selected MRU survivor loses to an eligible same-report newcomer. Retention uses the one global slot, not a selection-policy expansion. Approved 2026-11-11 ([design](../changes/retained-focus-requests.md)); not yet implemented.
- Why: not recorded.
- Assumes / revisit if: Windows adapter routing and native confirmation remain open.
- Status: active, 2026-10-29
- Evidence: [fixed-size focus decision](../changes/kde-world-v2-port.md#recommended-core-shape-alternatives-and-cutover).

### Output targeting
- Decision: Cursor output is default for workspace switch/new-window placement; focused-output alternative for switch and three-way new-window setting (cursor output splitting last-focused / closest-to-cursor / focused output splitting last-focused). Window sends, including relative, use focused window's output regardless of cursor. Focus-follows-cursor and cursor-warp settings default off; i3/sway preset enables warp on cross-output focus; Hyprland warp stays off pending verb evidence. Cross-platform presets set all supported settings to COSMIC (default), i3/sway, or Hyprland; missing preset evidence stays TBD. Implement placement in shared core when possible; adapters supply cursor position/settings.
- Why: not recorded.
- Assumes / revisit if: KDE/macOS adoption and native FFC hover remain pending; missing preset evidence stays TBD.
- Status: active, 2026-10-26
- Evidence: [output evidence](../changes/archive/cursor-first-output-evidence.md), [discriminators](../spec/reference-outcomes.md#cursor-first-output-targeting-discriminators).

### Move and resize triage defaults
- Decision: D09/MOV-06 defaults to Geometric middle; post-0.1 Nested move entry offers only Geometric middle (COSMIC) or Remembered child (i3/sway). D10/MOV-07 defaults Wrap; post-0.1 Orthogonal escape offers only Wrap (COSMIC) or Escape outward (i3/sway); drop swap-target-leaf (bspwm/awesome only). D12/MOV-04 defaults Extract to same axis, no setting; Retain nesting held only Hyprland+bspwm. D13/OUT-02 defaults Wrap locally, no setting; Cross output held only bspwm/awesome. Flat-triple perpendicular move follows COSMIC R1, changing root to V[H[A,C],B], not refusal. Outward work-area-edge resize is no-op; nested resize adjusts adjacent shares at nearest matching-edge-axis split.
- Why: not recorded.
- Assumes / revisit if: alternatives failing tier rule do not become settings without new evidence.
- Status: active, 2026-10-12
- Evidence: [triage](../research/release-0.1-triage.md#user-decisions-2026-10-12).

### In-group move and single-output edge
- Decision (user 2026-11-13, REQ-MOV-02 / REQ-MOV-05): confirm the existing in-group leaf swap with mover focus retained (`H[A,V[C,B*]]`, move B up -> `H[A,V[B*,C]]`). After local move exhaustion at a single-output edge with no adjacent output, retain tree and focus unchanged. Existing adjacent-output transfer and other move rules remain unchanged.
- Why: retain the implemented behavior supported by the recorded Tier 1 swap/visually-unchanged outcomes; distinguish Hyprland's remove/reinsert reconstruction from a structural no-op.
- Rejected: changing these moves or adding a setting without an evidenced alternative for these fixtures.
- Status: active, User 2026-11-13; shared-core behavior confirmed, native acceptance pending.
- Evidence / TBD: [R-MOV-02](../spec/reference-outcomes/move.md#r-mov-02-in-group-vertical-swap), [R-MOV-05](../spec/reference-outcomes/move.md#r-mov-05-edge-move-with-no-left-neighbor), [batch 1](../changes/c9-product-evidence.md#requirement-batch-1-decisions-and-implementation-2026-11-13). Exact latest-release Hyprland rebuilt geometry/ratios and native journeys remain TBD.

### Same-axis move setting
- Decision: One global `sameAxisMove` / `core.same_axis_move` setting: `group-with-neighbor` default, label Group with neighbor, tooltip COSMIC; or `swap-with-neighbor`, label Swap with neighbor, tooltip i3/sway. Windows schema-v1 missing defaults to group. Changes affect subsequent moves without rebuilding existing trees; no migration. Swap replaces only R2c when neighbor is adjacent direct leaf sibling in same group, moving shares with windows; leaf/group neighbor rules stay current.
- Why: not recorded.
- Assumes / revisit if: add discriminating rows before broadening leaf/group cases.
- Status: active, 2026-10-08
- Evidence: [setting implementation](../changes/archive/admission-and-move-settings.md), [Windows delivery](../changes/archive/windows-same-axis-move.md).

### Parent orientation toggle
- Decision: Meta+O / Win+O (COSMIC parity) toggles immediate parent group including root, preserving child order, shares and focus. Lone root leaf no-op; no saved orientation hint for future admissions; long-edge rule unchanged. Authentic takes over; Compatible disables Windows binding.
- Why: not recorded.
- Assumes / revisit if: Windows Win+O is OS orientation lock; no stock KDE holder found. Windows repeat is once per discrete down; consume held repeats without dispatch, release/repress toggles.
- Status: active, 2026-10-14
- Evidence: [KDE/core](../changes/archive/parent-orientation-toggle.md), [Windows](../changes/archive/windows-parent-orientation-toggle.md), [held repeat discriminator](../spec/reference-outcomes.md#r-lay-01-windows-held-key-discriminator).

### Workspace-local layout selection
- Decision: Park REQ-LAY-04 until a genuine second layout exists; revisit when tabbed stacks are designed after 0.1. Whether tabs count as L2 is a future decision, not implementation.
- Why: not recorded.
- Assumes / revisit if: not recorded.
- Status: active, 2026-10-09.
- Evidence: [research](../research/workspace-local-layout-selection.md).

### Directional move/output transfer and selection
- Decision: Local restructure/swap/escape wins first; otherwise move crosses adjacent output; sole root leaf crosses all four directions. Adjacency uses reciprocal edge-touch + positive overlap on full output rectangles (not work areas), all directions, no wrap; unreadable topology refuses, no candidate no-op. For exhausted window moves and explicit sends choose edge containing moving-window-center projection; otherwise larger overlap with window span along edge; final left/top. Left/right project y, up/down x; edges half-open, exact half-pixel centers. Multiple center-containing edges tie left/top; span only when none contains center. Ascending x then y; equal positions by stable output ID; reverse multiplicity valid if resolved pair reciprocal. Migration instead largest shared edge then left/top.
- Why: not recorded.
- Assumes / revisit if: deliberate COSMIC origin-distance deviation; Hyprland/i3/sway/awesome/bspwm select by window position; no reference refuses. Full rectangles choose adjacency, work areas still placement.
- Status: active, 2026-10-18
- Evidence: [selection](../changes/archive/position-based-output-selection.md), [world split edge selection](../changes/archive/world-split-edge-selection.md).

### Explicit output-send actions and chords
- Decision: Explicit sends target destination output's current workspace and use ordinary workspace-send admission: remembered leaf, destination focus history, root fallback. Follow binds Meta/Win+Ctrl+Alt+arrows and H/J/K/L; stay bindable/unbound. Target operation preserves explicit workspace, follow/stay and existing fences. Initial R-OUT-04 subject was tiled only, sticky excluded; floating-workspace boundary transfers membership only and reflows tiled sides. Follow/stay also applies at floating-workspace boundary; intentional float transfer is separately governed by D16.
- Why: not recorded.
- Assumes / revisit if: COSMIC Super+Shift+Alt collides with resize-shrink; niri arm collides with workspace send; Meta+Ctrl+Alt absent from KDE stock holders, Windows ownership unknown. Windows follow rejects Shift held; rebound stay temporarily accepts Win+Ctrl+Alt or +Shift and canonical directional letter hold slot.
- Status: active, 2026-10-29
- Evidence: [four-direction transfer](../changes/archive/four-direction-output-transfer.md), [Operation dispatch](../changes/kde-world-v2-port.md#operation-pair-outer-gap-bookkeeping-2026-10-29), [held key](../spec/reference-outcomes.md#windows-output-send-held-key-discriminator).

### Exhausted directional R4 target wrapping
- Decision: Exhausted directional move into occupied target wraps its whole tree in new top-level D-axis split, mover nearest source. Preserve nested same-axis groups: Left on H[A,B] -> H[H[A,B],W]; Right -> H[W,H[A,B]]; Up on V[A,B] -> V[V[A,B],W]; Down -> V[W,V[A,B]]. Single leaf wraps same; empty target receives mover alone. Never insert beside remembered/last-focused leaf or append existing root.
- Why: Predictable placement independent of target focus history, preserving target layout as one unchanged subtree.
- Assumes / revisit if: COSMIC Tier 1 observed R4/S20/S23; i3 flat append rejected. Shared core applies KDE/Windows; R4 best-effort uses D3 only, no D2.
- Status: active, 2026-10-14
- Evidence: [decision and native gate](../changes/kde-dual-output-live.md#whole-target-r4-wrap-decision-2026-10-14).

### Best-effort admission fallback
- Decision: Capacity means geometric exhaustion (whole-tree projection cannot give every tile positive extent after gaps), not minimum infeasibility. Try ordinary anchor first. Fallback always chooses largest-splittable-window by plain area descending, child-order DFS then lifetime ID; no fallback setting, closest-relative or no-search alternative. Require strict and normal hinted projection; minimum-overconstraint does not refuse. Wrap selected leaf old/new equally on plain long edge (square vertical); no short-axis retry, ancestor wrap, rebalance, automatic other workspace/output search or spill.
- Why: geometric exhaustion and minimum-overconstraint are different problems. An ordinary sole newcomer with an oversized minimum still tiles; Tier 1 source accounts do not automatically float it merely for that minimum. Minimum infeasibility alone does not make a capacity float.
- Assumes / revisit if: D1/D2 fallback ordering and geometric splittability above are active; B6 minimum infeasibility remains independent.
- Status: active, User 2026-11-12 supersedes D2's alternatives and configurable-default wording; ordinary-first, splittability, ranking and minimum semantics remain.
- Evidence: [fixed-largest removal](../changes/c9-product-evidence.md#fixed-largest-implementation), [implementation and live re-check](../changes/archive/best-effort-admission.md), [oversized sole minimum](../spec/reference-outcomes/minimum-size.md#r-min-03-backfill-oversized-sole-minimum-scrolling); exact native frames/client settlement TBD.

### Best-effort D3-D4: no-space result and focus
- Decision: Geometric capacity exhaustion always floats; no no-space setting or allow-overlap alternative. Capacity float has no tile slot/frame authority and retains opening frame; eviction retains last live frame. It remains ordinary float for navigation, decoration, movement and explicit tiling; `capacity` origin is diagnostic only, distinct from automatic fixed-size admission. Focus fresh newcomer, whether tiled or floated, never alternate anchor; ordinary activation/game/overlay/gesture/at-most-once fences apply. Repair/eviction never issues focus setter.
- Why: not recorded.
- Assumes / revisit if: minimum-only infeasibility remains B6, not geometric capacity exhaustion.
- Status: active, User 2026-11-13 supersedes D3's configurable default and overlap alternative; D3 float placement/provenance and all D4 focus rules remain.
- Evidence: [best-effort admission](../changes/archive/best-effort-admission.md).

### Best-effort D5: persistence and retry
- Decision: Never automatically re-tile after close, growth, smaller gaps, visits, replies or disable/enable. Explicit tile retries once; if still no space retain float with no false successful-tile override. Persist float membership across restart exactly like user floats. Diagnostic origin and tiled-admission history need not survive core restart; neither affects ordinary float behavior. Close/identity replacement clears live origin/order.
- Why: not recorded.
- Assumes / revisit if: explicit tile remains the only retry trigger.
- Status: active, 2026-10-17
- Evidence: [best-effort admission](../changes/archive/best-effort-admission.md).

### Best-effort D6: command and transfer coverage
- Decision: Apply to fresh/rejoin admission, explicit tile, startup/enable, sends, R4, drag and retained-geometry capacity repair. Named sends search destination only. R4 retains exact whole-target directional wrap and uses D3 but not D2. Transfers preserve source until existing arrival/transaction proof; same-output floats preserve frame, cross-output floats use established native move-only remap once, never guessed frame. Drag retains target/side and fresh preview/finish agreement; without honest float preview refuse/snap back, never float only at finish. Promised old slots/migration first preserve retained topology, then D7/D9, not new anchor. Host capabilities remain gates.
- Why: not recorded.
- Assumes / revisit if: D2 fallback is not applied to R4; ordinary host capability fences stay authoritative.
- Status: active, 2026-10-17
- Evidence: [best-effort admission](../changes/archive/best-effort-admission.md).

### Best-effort D7: capacity eviction
- Decision: Float the shortest newest-first eligible victim prefix until retained projection succeeds, contracting branches by existing survivor-share rules. Ordering is first successful tiled admission and remains for same lifetime across moves/retiles; ties use descending lifetime identity. Clean-fit adoption shares one initial sequence; serial legs retain actual successful order, not ID sort.
- Why: not recorded.
- Assumes / revisit if: protect reserved fullscreen/maximized slots, fixed explicit-tile overrides and unsafe/excluded members; defer during gestures. Impossible protected remainder or invalid usable area remains narrowly held. Evictions persist as ordinary floats.
- Status: active, User 2026-11-13 fixes D7 victim floating rather than selectable overlap retention; ordering, protection, holds and focus rules remain.
- Evidence: [fixed-float removal](../changes/c9-product-evidence.md#fixed-float-implementation), [best-effort admission](../changes/archive/best-effort-admission.md).

### Best-effort D8: startup roster
- Decision: Preserve clean/tolerance-valid minimum-feasible recursive-cut adoption first; otherwise retain spatial `(y,x,h,w)`, focus-last serial seed with stable adapter-order ties and D2/D3 for each leg. Commit complete mixed roster atomically; persisted floats stay floating. Do not claim balanced optimizer or native burst-order parity.
- Why: not recorded.
- Assumes / revisit if: seed order remains per specified spatial tuple and focus-last rule.
- Status: active, 2026-10-17
- Evidence: [best-effort admission](../changes/archive/best-effort-admission.md).

### Best-effort D9: retired capacity-overlap projection
- Decision: Retire positive-overflow capacity projection and accepted-overlap marks with the D3 overlap alternative. No remaining producer or persisted/imported accepted-overlap mark requires retention. Admission, startup, sends and retained operation projections use strict geometry; D7 repairs exhausted retained trees by floating eligible victims. Invalid bounds, unrepresentable targets and impossible protected remainders still hold narrowly; never fabricate native rectangles. B6 minimum settlement and ordinary native/frame overlaps are not this retired policy.
- Why: not recorded.
- Assumes / revisit if: any future imported capacity allocation needs an explicit contract, not implicit overlap authority.
- Status: retired, User 2026-11-13 supersedes D9's capacity-overlap allocation and retention, preserving strict projection, minimum handling and safety holds.
- Evidence: [fixed-float route audit](../changes/c9-product-evidence.md#fixed-float-implementation).

### Best-effort D10: settings
- Decision: Anchor fallback and no-space behavior are fixed by D2/D3, not exposed in storage, controls or settings messages. Removed `anchorFallback` and `noSpaceResult` inputs follow unknown-field handling without adopting values or discarding valid siblings; KDE leaves removed stored keys untouched, Windows ignores them on load and omits them on save. Settings Apply alone causes no capacity reflow, retry or reclassification; subsequent admissions and actual changed-bounds/gaps capacity decisions use the fixed policies. Native validity and saved/requested/confirmed state remain separate.
- Why: not recorded.
- Assumes / revisit if: no additional Apply side effects.
- Status: active, User 2026-11-13 additionally supersedes D10's no-space-setting exposure and overlap retention; next-decision timing, ordinary anchoring, fixed-largest fallback and minimum semantics remain.
- Evidence: [fixed-float removal](../changes/c9-product-evidence.md#fixed-float-implementation), [fixed-largest removal](../changes/c9-product-evidence.md#fixed-largest-implementation), [best-effort admission](../changes/archive/best-effort-admission.md).

### Best-effort Windows capability boundary
- Decision: Windows explicit output-send and R4 capacity-float transfers refuse `float-no-remap`, preserving source Engine/membership until remap rule approved; this is host capability gate, not shared selection change and not change to whole-workspace migration remaps.
- Why: not recorded.
- Assumes / revisit if: native Windows/macOS consumption and KDE live acceptance remain pending.
- Status: active, 2026-10-18
- Evidence: [Windows limits](../changes/archive/windows-best-effort-admission.md#host-differences-and-bounded-limitations).

### Native workspace adoption and R19
- Decision: On fresh core generation, host-owned workspace adoption takes native desktops/Spaces in native order per display, preserving native window membership and issuing no workspace moves; tiling still uses existing size/position startup inference. Later outside moves become new membership; outside-created native desktop becomes workspace. `native[]` bootstraps native-bound rows with capability-gated per-window native membership evidence; completeness flags fence incomplete adoption and native-free behavior stays unchanged. Four occupied desktops map to four native-bound rows in order, members preserved and no launch moves; this is membership mapping, not total reply-row count, and ordinary trailing-spare lifecycle remains unchanged.
- Why: not recorded.
- Assumes / revisit if: R19 option (a) confirmed all five tentative policies active 2026-10-31; native outcomes remain TBD; macOS removal capability gating remains open. Windows tiler-owned domains retain logical receipts.
- Status: active, 2026-10-31
- Evidence: [R19 review](../changes/core-interface-review.md), [four-to-four](../changes/kde-world-v2-port.md#p1-four-total-versus-existing-trailing-lifecycle).

### Native creation and attempt settlement
- Decision: Host-owned adapters report native evidence only: per-window `nativeWs` and per-display desktop order/current. Core alone translates IDs, derives member/view receipts and publishes output-specific `native_binding`; adapters retain live native-id/handle lookups, never native-to-logical table. Windows tiler-owned domains keep logical receipts. Optional `current_native=None` means non-desktop surface, never a workspace; retain logical view while using desktop order. Adopt only ordered desktop IDs; unknown/fullscreen membership retains home without geometry writes.
- Why: not recorded.
- Assumes / revisit if: native order authoritative after create/reorder. Identical spanning projections are one logical set, shared fully supported; separate views degraded and adapter warns. Complete vanished IDs are removals, never renames; fresh generation re-adopts. Native acceptance TBD.
- Status: active, 2026-10-31
- Evidence: [R19 review](../changes/core-interface-review.md).

### Native creation correlation
- Decision: Core issues creation desires and alone binds native backing. An optional host-correlated created ID binds exactly that new ID, causally and at most once, even with outside additions; host need not create synchronously or return a handle. Without correlated ID, exactly one outstanding creation on that output may consume next singleton addition; multiple additions or creations hold `AmbiguousCreation`, never blind re-create.
- Why: not recorded.
- Assumes / revisit if: accepted race: outside X arrives before requested async T, X binds desired logical row; T later is adopted as ordinary outside creation. This inference is not causal proof. KDE may provide same-turn before/after causal IDs; asynchronous hosts use singleton rule.
- Status: active, 2026-10-31; native outcomes TBD.
- Evidence: [creation correlation](../changes/kde-world-v2-port.md#stage-1b-creation-correlation).

### Native attempt settlement
- Decision: Optional capability-gated native before/after/baseline IDs extend `MemberAttempt`/`ViewAttempt`. Non-null evidence without host-owned capability refuses, as with `WindowFact.native_workspace`. Core alone translates IDs through its bindings: matching outstanding work settles; otherwise outside input is adopted. The user's switch back wins, migration stays and paired source refill retires permanently. Windows logical preimages unchanged.
- Why: not recorded.
- Assumes / revisit if: Late tiler send observed with intermediate readings settles as tiler transfer, not outside move. Native outcomes TBD.
- Status: active, 2026-10-31; core implementation and 19 settlement pins delivered.
- Evidence: [settlement](../changes/kde-world-v2-port.md#b2-source-view-rewind-during-migration).

### Native migration backing and removal
- Decision: Migration keeps core logical ID and releases source native binding, retaining no dormant return binding. Missing destination backing requires explicit fresh creation; never reuse empty native desktop bound to another logical workspace. Released desktop observed current or in complete native order without logical workspace is adopted as fresh empty logical workspace at native position, like outside creation. Migrated workspace stays destination; user's switch wins; paired refill child retires permanently. No reverse-migration provenance or eligible-unbound reuse. Ordinary trailing/equal-count lifecycle remains. This contract also applies to macOS Spaces, not only KDE.
- Why: not recorded.
- Assumes / revisit if: Release only after positive member arrival, explicit closes or newer off-output evidence, never negative vacancy inference. Partial native lists or pending creations defer cleanup independently of transfer until safely retried. Refusal rollback keeps source backing. First proven-rewind reply retains migration transfer while fresh source adoption is staged echo; refill retires without reapplication and later observation settles parent. Assertions encoding dormant retention are superseded, migration-transfer retention stays.
- Status: active, 2026-10-31
- Evidence: [native retirement](../changes/kde-world-v2-port.md#native-retirement-implementation-2026-10-29), [source-view decision](../changes/kde-world-v2-port.md#b2-source-view-rewind-during-migration).

### Native empty-workspace removal and padding
- Decision: Logical empty-row pruning is independent of native desktop deletion: remove ordinary unshown empties from their ring, retaining the trailing spare and displaced/held units. Native eligibility uses global occupancy (including unmanaged occupants), any-output visibility, protection and readable facts; KDE applies fresh safety vetoes and owns the native minimum-two floor. Core has no independent minimum floor. Do not relocate occupants to compact gaps; surviving native IDs are stable even if KDE renumbers ordinals. Every trailing logical spare gets immediate native backing; separate mode accepts a shared pager showing the longest output list. Unmapped desktops remain outside management.
- Why: Keep pager backing consistent with the common logical model without reviving the retired aligned-grid/global-hole lifecycle.
- Assumes / revisit if: The 2026-10-29 requirement to retain logical rows until native deletion readback and its core-owned minimum-two/equal-padding removal wording are superseded by [the 2026-11-10 lifecycle decision](workspaces.md#dynamic-workspace-lifecycle-and-kde-pager-backing). Neutral occupancy reporting, conservative native safety and native deletion readback remain required. Internal locally retired-native markers are described in the [implementation note](../changes/kde-dynamic-workspaces.md#implementation-choice-locally-retired-native-markers); post-prune native deletion authority, implementation and native evidence remain incomplete.
- Status: active, User 2026-11-10; supersedes the lifecycle/minimum ownership wording from 2026-10-29, without undoing the 2026-11-08 aligned retirement.
- Evidence: [dynamic lifecycle](../changes/kde-dynamic-workspaces.md); earlier [native retirement](../changes/kde-world-v2-port.md#native-retirement-implementation-2026-10-29).

### Native occupancy and sticky boundaries
- Decision: Sticky semantics remain all managed workspaces of the owning output, float-only and requiring positive all-desktop proof; sticky migration stays on source and does not claim native cross-display behavior. Native `None` means all-desktop, but cannot assign initial single home; float-sticky loses its published exception row when membership is unknown, tiled-sticky retains a minimized slot; never fabricate current membership. Neutral complete/readable occupancy reporting and deletion-readback retention are owned by [Native empty-workspace removal and padding](#native-empty-workspace-removal-and-padding).
- Why: not recorded.
- Assumes / revisit if: macOS native-removal capability gating remains open.
- Status: active, 2026-10-31
- Evidence: [R19 review](../changes/core-interface-review.md).

### Native hidden representation and unknown sticky membership
- Decision: Represent KDE inactive-Activity and macOS Cmd+H windows with existing `WindowFact.minimized=true`, no new core state; qualified workspace/output-loss/pending-transfer relocation is intentional. The later Activities default replaces earlier strict "never move" promise. Positive all-desktop membership for a window with an already retained same-lifetime home preserves that home without fencing its scope, regardless of whether core or the native window manager set sticky. Do not fabricate a current-desktop or logical membership receipt. Genuinely unknown/missing membership and newcomers without a retained home keep existing fences.
- Why: not recorded.
- Assumes / revisit if: Activities native testing pending. Known all-desktop retention does not change sticky-off origin/admission rules or authorize initial home inference.
- Status: active, 2026-11-08
- Evidence: [R19 review](../changes/core-interface-review.md), [sticky-release correction](../changes/v4-phase2-kde-state-targets.md#verified-sticky-release-correction).

### Shared core and adapter delivery coordination
- Decision: Deliver the shared Rust core and KDE adapter on the KDE side first; wire the separate Windows adapter later. Correctness takes priority over preserving Windows behavior: Windows build/behavior breakage is acceptable when the backlog names the specific required Windows changes. Compile-only Windows fixes that preserve current behavior are allowed; correctness-first adapter wiring remains in the backlog handoff.
- Why: not recorded.
- Assumes / revisit if: this sets implementation sequencing and does not alter product behavior.
- Status: active, 2026-10-07
- Evidence: [implementation order and Windows handoff](../backlog.md).
