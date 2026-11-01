# Window state

### Fullscreen and maximize overlays
- Decision: maximize/fullscreen are native overlay state, not topology. A maximized member retains identity, leaf/tree position, share and exact allocation; receives no geometry write; unmaximize restores retained allocation. Native maximize modes collapse to boolean; H/V maximize is not modeled. Fullscreen retains member/tree/share, pauses geometry writes and restores current Engine allocation on exit. Fullscreen initially observed at startup/birth stays out of tree until first non-fullscreen observation and then receives fresh admission; later fullscreen retains tile. Borderless output-sized windows are not inferred fullscreen on KDE. Fullscreen takes precedence over maximize for refusal/safety except explicit R-MAX-11 toggle replacement below.
- Windows app-owned fullscreen exit remains refused without a restoration preimage (REQ-MAX-05/05b). An adapter-local adoption-baseline design and per-case user-owned acceptance gate are recorded in [Windows fullscreen exit baseline](../changes/windows-fullscreen-exit-baseline.md); this does not change current refusal or authorize live checks.
- Why: not recorded.
- Assumes / revisit if: unknown maximize transition remains unknown until subsequent observation; missing native change is not inferred. Fullscreen/born-fullscreen and native session-restore acceptance remain separate.
- Status: active, 2026-10-12
- Evidence: [window-state reference outcomes](../spec/reference-outcomes/maximize-fullscreen.md).

### Born-maximized policy and tile overlays
- Decision: first-seen maximized admission is controlled by `core.born_maximized`: default `tile-on-launch` unmaximizes once then ordinary admission; alternative `keep-maximized-overlay` reserves tile slot under maximize. Functional labels are "Tile on launch"/"Keep maximized overlay"; reference names only in tooltips. Fullscreen untouched; later maximize and floating-to-tiled admission remain overlay; fixed-size exceptions remain governed by layout admission. Core owns two-value policy/default; Windows uses shared policy. KDE retains TypeScript copy pinned by one conformance case table until post-0.1 Push code down; existing KDE behavior, labels/tooltips/key and subsequent-admissions-only adoption unchanged.
- Why: born-maximized launch (e.g. browser session restore) felt janky in Windows dogfood.
- Assumes / revisit if: game risk of maximized-window launch is checked by user. Native launch/restore/session-restore no-loop remains pending; no policy change to reserved-slot alternative.
- Status: active, 2026-10-13
- Evidence: [born-maximized shared policy](../changes/archive/born-maximized-shared-policy.md), [Windows delivery](../changes/archive/windows-born-maximized-shared-policy.md).

### Maximize command and fullscreen replacement (R-MAX-11)
- Decision: ordinary/fullscreen maximize toggle uses `WorldCommand::ToggleMaximize`, Session decision, command-only `CommandResult::Maximize`; KDE no longer chooses toggle locally. Explicit Meta+M on fullscreen may perform only native state writes needed to replace fullscreen with maximize. No automatic/unsolicited fullscreen writes. Tiled base keeps slot/share without sibling reflow; floating base stays floating with durable placement; next Meta+M unmaximizes to base. Both flags set becomes maximized, not fullscreen. KDE clears `fullScreen` before `setMaximize`; applied flags may lag client acknowledgment; no intermediate readback, delay, retry or inferred success. Final readback logs confirmed/unconfirmed; later observations authoritative.
- Why: behavior/refusal was split among KDE and Windows; boolean fullscreen fact could not route neutral exit.
- Assumes / revisit if: additive command/result requires coordinated protocol bump review with unreadable-desktop ownership and Windows staged switches; no bump is implied here. See [Window-state commands](contract.md#window-state-commands-r14) for cross-platform command/refusal/fullscreen ownership contract. Windows/macOS runtime adoption separate; macOS sticky unsupported until all-Spaces route proven.
- Status: active, 2026-10-29
- Evidence: [fullscreen/maximize toggle](../changes/archive/fullscreen-maximize-toggle.md), [R14/R-MAX-11](../changes/core-interface-review.md).

### Directional maximize and fullscreen operations
- Decision: for maximized or fullscreen focused windows, directional focus skips same-output siblings and continues to the adjacent output in the requested direction; if there is no adjacent output, focus stays. Cross-output focus may target fullscreen or maximized members, preserving their overlay state. Existing retained-allocation ranking and eligibility/safety gates are unchanged. A maximized directional move still attempts one unmaximize and moves only after observed clear in that invocation; existing pointer-operation refusals remain. Fullscreen directional move remains fenced. Explicit click/focus-by-window/workspace switch is unaffected.
- Why: directional focus should leave an obstructing overlay when an adjacent output provides a target, and cross-output focus should not disturb the target overlay.
- Rejected: making maximized focus a no-op on every output; making fullscreen/maximized destination state ineligible by default.
- Assumes / revisit if: single-output maximized focus remains a no-op as there is no adjacent output; eligibility, retained-allocation ranking, and safety fences do not change. Native focus/state acceptance and Windows failure/race/preset/fixed/game/DPI/multi-output variants remain pending.
- Status: active, 2026-11-01
- Evidence: [maximized directional](../changes/archive/windows-maximized-directional-navigation.md), Tier 1 [R-MAX-08](../spec/reference-outcomes/maximize-fullscreen.md#r-max-08-focus-and-move-while-maximized) COSMIC single-output no-op and move, [R-MAX-02](../spec/reference-outcomes/maximize-fullscreen.md#r-max-02-fullscreen-focus-and-exit) i3/sway/Hyprland fullscreen focus fences, [R-OUT-11](../spec/reference-outcomes/multi-output.md#r-out-11-directional-focus-between-fullscreen-or-maximized-outputs) sway fullscreen target and selected Ours KDE/Windows outcome.

### Workspace sends of fullscreen/maximized members
- Decision: maximized workspace send preserves maximize on arrival cross-platform, deliberately unlike COSMIC/niri unmaximize, consistent with whole-window state traveling; KDE flag-stable send uses native membership/output transfer without unmaximize/remaximize/overlay geometry writes. Explicit follow/stay numbered or relative sends carry tiled fullscreen members and never-tiled born-fullscreen members while preserving fullscreen. A born-fullscreen member remains slotless on destination with no source/target reflow or source admission; first fullscreen exit uses fresh destination admission. Explicit numbered/relative FOLLOW and output FOLLOW focus fullscreen mover after verified arrival; STAY unchanged. No unsolicited focus changes, no restore or mover geometry write before send, and no tiling geometry writes to fullscreen subject. Existing arrival/current-view/lifetime/follow/stay fences apply.
- Why: a workspace send is an explicit user action and should be honored, not refused; unlike an in-layout move, it relocates the whole window so maximize/fullscreen state travels.
- Assumes / revisit if: KDE native fullscreen carry including a real game is must-pass; stop for user review if KWin cannot carry without dropping/corrupting fullscreen. Windows physical shortcuts/native flag races/app games/cross-output remain pending. Whole-workspace output migration overlay rule is separate.
- Status: active, 2026-10-14
- Evidence: [KDE fullscreen send](../changes/archive/kde-fullscreen-activation-and-send.md), [Windows fullscreen carry](../changes/archive/windows-fullscreen-workspace-carry.md), [reference comparison](../changes/archive/reference-comparison-implementation.md), [R-MAX-09](../spec/reference-outcomes/maximize-fullscreen.md#r-max-09-send-a-maximizedfullscreen-window-to-another-workspace).

### Floating membership and placement
- Decision: intentional float is per-window state outside tile tree. Eligible active normal float toggle removes leaf and lets siblings reflow; first-time geometry is centered 60% work-area fallback, later uses retained float placement. Unfloat is fresh ordinary admission, never old-slot restoration, and carries live frame; moving/resizing float alone never tiles it. Explicit float toggle changes base membership beneath retained fullscreen: float releases slot/reflows siblings; toggle again fresh-admits invisibly; exit reveals last base float placement or current tile allocation. No writes to fullscreen subject (geometry/focus/stacking/native state), though sibling reflow is allowed; never capture observed fullscreen frame as placement. Ordinary native float route retains separate legacy safety fence until adapter replacement.
- Why: not recorded.
- Assumes / revisit if: shared Session/KDE delivery is offline; native acceptance and Windows/macOS adapter integration remain pending. Fullscreen float behavior follows Tier 1 majority; legacy native route does not authorize unsafe writes.
- Status: active, 2026-10-28
- Evidence: [fullscreen float](../changes/archive/fullscreen-float-toggle.md), [window-state reference outcomes](../spec/reference-outcomes/maximize-fullscreen.md#r-max-10-float-toggle-on-fullscreen).

### Sticky floating
- Decision: sticky float remains visible across every managed workspace of its output. Tiled-origin sticky toggle floats first; float-origin preserves frame. Sticky-off for prior tiled members fresh-admits to current workspace; prior float remains float. `Meta+G` on sticky clears sticky and tiles on workspace where pressed regardless of prior tiled/ordinary/adopted origin; `Meta+Shift+G` sticky-off honors prior floating origin. Native-sticky window with empty desktop list and no same-runtime origin is adopted as sticky float with prior-float semantics; sticky-off leaves normal float on then-current desktop with geometry/focus preserved; later ordinary toggle tiles. KWin `keepAbove=true` for intentional/sticky float; restore project-cleared keepBelow/prior keepAbove=false before unfloat, fresh tiled admission or controller disable; preserve pre-existing keepAbove. Missing/refused/unverifiable write fails transition without retry.
- Why: not recorded.
- Assumes / revisit if: exact-reference desktop echo fence has no retry/timeout/fallback/polling. Cross-display native sticky behavior is not claimed; restart persistence and native restore evidence remain separately scoped.
- Status: active, 2026-09-25
- Evidence: [sticky policy](workspaces.md#workspace-sends-and-floating-state), [sticky reference](../spec/reference-outcomes/maximize-fullscreen.md).

### Maximized intentional unfloat (B9)
- Decision: explicit unfloat of intentionally floating natively maximized window unmaximizes then fresh-admits; no retained-maximize unfloat. One clear attempt, observe clear, then admission. Unconfirmed clear retains float intent for later press; no marker/intent commit. Owned fullscreen metadata vetoes before and after clear, including non-covering transition frames.
- Why: not recorded.
- Assumes / revisit if: physical B9/fixed marker/restart/failure/game/DPI/multi-output and KDE native acceptance remain pending; COSMIC reference observation confirmation is not implementation gate.
- Status: active, 2026-10-08
- Evidence: [maximized intentional unfloat](../changes/archive/maximized-intentional-unfloat.md), [Windows delivery](../changes/archive/windows-maximized-intentional-unfloat.md).

### Background tiling and permissive admission
- Decision: background tiling is supported at startup and on open/move for non-visible workspaces without switching visibility or stealing focus. Existing floating/sticky/fullscreen/maximize/gap rules remain authoritative. Incoming normal window frame never decides whether it may join a tiled domain; assign new complete geometry for every member and permit reflow, so old out-of-work-area window cannot prevent later tile or create restart-persistent deadlock. Valid observed frames outside work area are host drift, not rejection, for retained/directional/send target observations; canonical projection and bounded host drift handling converge them. Malformed rectangles/domain homing/reply geometry remain validated.
- Why: not recorded.
- Assumes / revisit if: offline verified; live acceptance pending.
- Status: active, 2026-09-29
- Evidence: [mid-drag workspace recovery](../changes/archive/mid-drag-workspace-recovery.md), [background tiling](../changes/archive/observation-convergence.md).

### Minimize, restore and rehome
- Decision: minimizing releases the tile allocation and reflows survivors; restore attempts best-effort rejoin beside a surviving former neighbor, preserving side, group/index and comparable share. Fresh ordinary admission is used only when no former neighbor remains, domain is gone, or window was sent elsewhere. Resize, new windows, sibling closes and swaps do not invalidate the slot; stored slots are session-only. Explicit close clears the minimized slot. Confirmed rehome forgets source slot; return is fresh admission.
- Why: not recorded.
- Assumes / revisit if: COSMIC evidence supports best-effort restore; native close/minimize ordering remains pending. Windows/COSMIC delivery evidence is not native acceptance.
- Status: active, 2026-10-17
- Evidence: [D23 restore and close](../changes/archive/d23-rework.md), [Windows D23](../changes/archive/windows-d23-restore-close.md).

### Transient dialogs and float visibility
- Decision: transient dialogs float rather than tile; Windows owned-dialog exclusion is an intentional divergence. Ordinary floats hide when their workspace is not shown and retain frame across switch. Focusing overlapping float raises it; no project lower verb. Closing focused ordinary float leaves tiles untouched and refocuses MRU survivor.
- Why: not recorded.
- Assumes / revisit if: parent-focus/modal legs remain TBD; KDE automatic fixed floats use above-tiles layer without project focus writes, qualified by game safety. Relative float order remains host-owned; unload restoration and final native acceptance remain open.
- Status: active, 2026-10-17
- Evidence: [D23 discriminator](../spec/reference-outcomes.md#d23-best-effort-restore-and-close-discriminators).

### KDE floating directional focus and half-snap
- Decision: float-origin focus searches ordinary/sticky floats on current output/workspace by top-left along requested axis, ignoring perpendicular coordinate. Up/Left include equal positions and choose first nearest tie; Down/Right require positive movement and choose last nearest tie. Sticky candidates precede ordinary in native encounter order. Windows candidates use eligible observations followed by retained maximized rows, with sticky-first layering over the combined list; distance wins across classes/layers and only equal-axis ties use this order. Visible sticky candidates participate across home workspaces on the same output; automatic fixed floats retain the no-touch rule. Local miss never cycles workspaces; Up/Down retain focus. The historical Left/Right adjacent-output remembered tiled-target behavior is superseded for World float-origin focus by D4 local-only policy; the separate internal float-cross proposal is unchanged. Tile-origin navigation skips floats. KDE direction-only commands route to core, which owns float-focus lattice and half-snap geometry; this supersedes the earlier KWin-local selection/half-snap route. Rust retains remembered cross-output focus authority using internal `focus.float_subject`, leafless `from_leaf: null` and `focused-floating-window` precondition; tiled wire replies do not change. Meta+Shift+arrow requests that half again, retaining float/sticky state and focus; use COSMIC integer relative-geometry formula, inner gap at outer edge and between halves, ignore outer gap.
- Why: not recorded.
- Assumes / revisit if: quarter/maximize/outward transfer transitions deferred; incompatible size constraints, interactive resize, fullscreen/maximized refuse geometry writes. Physical acceptance remains user-owned.
- Status: active, 2026-10-28
- Evidence: [Windows float navigation](../changes/archive/windows-floating-directional-navigation.md), [KDE float navigation](../changes/archive/kde-floating-directional-navigation.md).

### Windows fullscreen safety and foreground policy
- Decision: managed fullscreen retains tree/share, pauses geometry, restores current Engine allocation on exit, precedes maximize and suppresses border/underlay. Unmanageable fullscreen foreground permits only non-activating reconciliation of other verified managed windows; foreground remains excluded/untouched, automatic focus blocked, overlays suppressed, fresh admission/unmaximize/marker/visibility deferred. Invalid/unreadable foreground remains fully blocked. Never synthesize app F11 or guess restore; project toggle uses official style/frame APIs and inert preimage, one attempt per discrete key-down, held repeats consumed.
- Why: preserve unrelated geometry convergence without game focus/overlay disturbance or writes to game.
- Assumes / revisit if: medium-integrity owner cannot manage elevated game; wider game/performance variants and physical input/display remain unproven. Captionless full-monitor windows classify fullscreen on Windows; KDE remains native-flag based.
- Status: active, 2026-10-19
- Evidence: [Windows fullscreen](../changes/archive/windows-fullscreen.md), [single-output dogfood](../changes/windows-single-output-dogfood.md).

### Windows fullscreen metadata retirement
- Decision: automatically clear owned fullscreen metadata only when fresh observation proves exit: original cleared nonzero chrome mask reads restored and fullscreen reads false, with exact-lifetime/preimage revalidation and a second proof before removal. Reconcile and maximize-clear routes share metadata-only sequence; no geometry write. Transition, unreadable, changed-preimage, identity-race, or genuine re-fullscreen retain veto. Originally captionless zero-bit preimages fail closed; Win+F11 owned exit remains release path.
- Why: not recorded.
- Assumes / revisit if: user watches for failures to tile during dogfood; time or non-coverage alone never proves exit.
- Status: active, 2026-10-12
- Evidence: [Windows fullscreen exit](../changes/archive/windows-fullscreen-exit-acceptance.md).

### Windows float and sticky persistence
- Decision: Windows float geometry uses centered 60% fallback then retained geometry; unfloat uses live frame and ordinary placement. Native move/resize never implicitly unfloat; tiled pointer ops refuse float; sends refuse focused float while selection hide/reveal preserves geometry. Sticky floats remain visible across managed workspaces on output; same-runtime sticky-off fresh-admits tiled origin or keeps float origin. Win+G on either sticky origin clears sticky and tiles. Intentional/sticky floats persist across owner restart; stop/crash reveals hidden windows without geometry recovery; terminal remains ordinary.
- Why: not recorded.
- Assumes / revisit if: Windows implementation still has runtime-local reset pending persistence handoff; physical/crash/watcher/display checks remain pending. Native markers are lifetime properties and survive owner stop/crash for next-owner normal-float adoption.
- Status: active, 2026-10-08
- Evidence: [Windows float](../changes/archive/windows-float.md), [Windows sticky](../changes/archive/windows-sticky-float.md).

### Windows app-hide close
- Decision: treat managed app-initiated hide as close only after positively verifying alive, invisible, uncloaked, lifetime-fenced, and no tiler hide claim or durable ledger entry. Release allocation and apply existing close/MRU rules; reshow is fresh admission without remembered slot. Unknown/unreadable facts and tiler-owned workspace hides retain safe hold.
- Why: avoid invisible member indefinitely blocking survivor reflow without confusing native minimize with close.
- Assumes / revisit if: implementation remains blocked on durable-claim safety finding; reference evidence supports removal, not identical native hide detection or focus defaults.
- Status: active, 2026-10-19
- Evidence: [D23 discriminator](../spec/reference-outcomes.md#d23-best-effort-restore-and-close-discriminators), [safety blocker](../changes/windows-single-output-dogfood.md#external-hide-close-implementation-and-safety-blocker).
