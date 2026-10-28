# Windows 0.1 approved gate

- Status: approved gate. User approved D1, D2, D3 and D4 exactly as written
  on 2026-10-19. Native/physical acceptance remains pending; this is not
  release acceptance.
- Basis: [shared scope and platform goals](../decisions.md#scope-and-platform-goals)
  (2026-10-18) and the approved
  [57-check inventory](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2).

## Goal, scope and acceptance

- Map all 53 original checks and N1-N4 to Windows exactly once, preserving
  their original 43 must-pass / 12 known-issue-allowed / 2 Windows-release labels.
- Record approved host equivalents, exclusions and grounded Windows additions;
  distinguish existing evidence from outstanding acceptance.
- No runtime, functional specification or principles changes; the approved gate
  is promoted to [durable decisions][scope]. No new live testing.
- Approach: inventory the shared template, inspect current and archived Windows
  records, reconcile platform differences, then verify IDs, counts and links.
  All 57 mappings and extras are reviewed together; physical/native acceptance
  and the in-progress world-route migration remain separate dependencies.

## Mapping and evidence

- Mapping: **A** = applies as-is (same behavioral assertion; use Windows input
  names); **H** = host-difference adaptation (Windows equivalent stated);
  **X** = not applicable (reason stated). Different implementation alone does
  not justify an exception.
- Original class: **M** = must-pass, **K** = known-issue-allowed, **W** = the
  original Windows-release class, not a Windows waiver. Both W rows are approved
  Windows 0.1 must-pass under D2; original table labels remain unchanged.
- Evidence: **P** = partial Windows native/physical evidence, with remaining
  acceptance pending; **O** = offline implementation/fixtures only for the
  mapped journey, native acceptance pending; **U** = pending, no accepted Windows
  journey located for this check; **X** = excluded, no Windows acceptance needed
  under D1. No row is claimed fully passed. A related pass does not complete
  a larger check or establish evidence for a changed runtime route.
- Source baseline: repository records reviewed 2026-10-18. In particular,
  [world adoption][world] is still test-only W2a, not runtime cutover or positive
  migration/refill acceptance. Recheck affected journeys after cutover; do not
  transfer legacy-owner evidence automatically. Latest source records take
  precedence over historical backlog wording.
- The checked-out main contained no macOS equivalent proposal at review time;
  no macOS content is changed. IDs/classes below follow the shared inventory,
  not a new Windows numbering or a claim of cross-platform test-harness parity.

| Inventory | Applies as-is | Host-adapted | N/A | Extras |
| --- | --- | --- | --- | --- |
| Original 57 checks | 22 | 30 | 5 | Not counted here |
| Windows additions | Not classified A/H/X | Not classified A/H/X | 0 | 8 |

| Evidence status | Partial native/physical (P) | Offline only (O) | Pending/unverified (U) | N/A (X) | Complete gate passes |
| --- | --- | --- | --- | --- | --- |
| Original 57 checks | 31 | 16 | 5 | 5 | 0 |
| Eight extras | 4 | 1 | 3 | 0 | 0 |

- All 52 applicable base checks and all eight extras retain pending acceptance;
  P/O are not alternative spellings of a full pass.
- The [consolidated physical/user-run checklist](windows-physical-checklist.md)
  groups pending Windows journeys by setup and maps each row back to these
  gate IDs, including WX1-WX8. Blocked implementation and fixture legs remain
  explicit; the checklist does not replace this gate or claim acceptance.

### Original checks 1-53

| ID | Class | Shared check | Map | Windows assertion / host equivalent | Evidence and exact remaining acceptance |
| --- | --- | --- | --- | --- | --- |
| 1 | M | Reference G-05/G-06/G-37/G-D2; D28 fullscreen carry | H | Windows min/max-track predicate; approved native maximize focus/move fences; project workspace source refill and max/fullscreen carry. Keep explicit real-game send, follow/stay and no unsolicited fullscreen writes. | **P**: [backlog handoff][backlog] items 13/17/18/19/20 record ordinary single-output physical G-06 and bounded G-D2; [item 22][item22] and [float-fullscreen fix][floatfull] add scoped combo evidence, not a real-game D28 pass. Predicate variants, migration, timing/failure and real-game legs pending. |
| 2 | M | Two-candidate output move/send/migration | A | Same target-selection, arrival and refusal assertions; distinguish window-centre/span ranking from whole-workspace largest-edge ranking. | **O**: [foundation][multi], [output sends][outsend], [migration][migration] deliver fixtures. Multi-candidate native journeys pending; topology discrimination can require three outputs, not merely two ([scope/hotplug][hotplug]). |
| 3 | W | Placement physical acceptance | A | Retain the original Windows startup-adoption/maximize physical check; no host substitution. | **P**: [placement correctness][placement] records native startup/maximize/send-return verification; [backlog][backlog] retains the physical matrix and ordinary Win+M samples. Original dogfood anchor is unproved. Shared [fit record][fit] alone is not Windows native proof. |
| 4 | W | Windows parity/settings/tray physical | A | Retain Windows Snap, Settings, tray, Explorer and DPI physical acceptance. | **P**: [tabs][tabs], [dialogs][dialogs] and [actual Explorer restart][pagerlife] cover bounded native paths; [2026-10-18 resize follow-up](archive/windows-keyboard-resize.md#scoped-native-refusal-follow-up-2026-10-18) adds native CLI float/sticky refusals and a physical-coordinate portable 360 px floor/one unchanged request, not physical shortcut delivery or larger app-hint/vertical/nested minimum acceptance. [backlog][backlog] lists physical Snap/shortcuts, scale/accessibility, games and multi-output gaps. |
| 5 | K | B9/R-FLT-06 on COSMIC | X | Reference-WM confirmation on COSMIC, not a Windows product check. Windows B9 remains check 7. | **X**: [shared inventory][triage]; excluding reference-host execution does not waive B9. |
| 6 | M | B1/B2 repeats; held-key autorepeat | A | Repeat maximize/sticky native-change cycles and physical hold/release; one toggle per intended activation, no stuck consumption or OS leakage. | **O**: [gaming classifier evidence][gaming] covers repeat/release rules offline; physical delivery and native-change repetitions remain pending in the [backlog][backlog]. |
| 7 | M | B9 unfloat while maximized | A | Clear/observe maximize, then fresh tiling admission; preserve focus and fixed-origin overrides. | **P**: [Windows B9][unfloat] records scoped clear/admit/focus evidence, not full physical B9. Hidden/restart/fixed/failure/game/DPI permutations pending. |
| 8 | M | D7 tile-override restart | H | Hydrate exact-lifetime on-window fixed-origin tile-override markers before admission, instead of KDE's restart store. Markers confer no write/recovery authority. | **O**: [fixed admission][fixed] proves native property/fixture plumbing, explicitly not ordinary-app D7 restart acceptance. Full override/hint-loss/maximize/restart/enable sequence pending. |
| 9 | M | D5 born-fullscreen exit, including games | A | On first app-native/born fullscreen exit, reclassify using current hints; no fullscreen geometry interference. | **P**: [fullscreen exit][fullexit] records ordinary managed-slot return and stop/relaunch physical acceptance plus a bounded Paint modal-overlay leg. Born-after-start/main-window, borderless, retirement, fixed and real-game variants pending. |
| 10 | M | D6 tiling enable with fixed windows | A | Classify all candidates under current fixed predicate; preserve intentional/sticky/explicit-tile intent and no-touch automatic floats. | **O**: [fixed admission][fixed] delivers enable/classification fixtures. Full native disable/enable matrix pending; a stable fixed restart anchor is not this journey. |
| 11 | M | Q3 R-RST-01/03/04 | H | Windows float/sticky/tile markers, current-frame adoption and identity gates replace the KDE store; no saved-layout restore or new cross-login identity. | **P**: [restart intent][restart], [rejection protection][hydrate] and [capacity restart][admissionlive] show bounded frame/intent preservation. Full restart/session-expiry/store-failure matrix pending; historical failures remain qualified. |
| 12 | M | D1 settings UI | H | Shared-catalog Win32 tabs, Windows storage/timing and Apply/Revert; predicate changes affect later admission, not mass reclassification. | **P**: [tabs][tabs], [dialogs][dialogs] and [policy save/reopen][admissionlive] show bounded native pickup/persistence. Full predicate/label/layout/accessibility and physical checks pending. |
| 13 | M | Q2 fixed admission, including borderless game | A | Keep the selected predicate and fixed-origin no geometry/focus/stacking/keep-above writes on automatic admission. | **P**: [fixed admission][fixed] includes a positive equal-track Settings restart anchor and negative Calculator control. User 2026-10-29 approves focus-half batch invariance: select the last non-withheld newcomer, skip/spend automatic-fixed arrivals, all withheld means no admission focus. Real Windows-profile WorldEngine regression omits automatic-fixed admission focus, with ordinary positive control and no Windows adapter change; Windows adapter-route/native confirmation remains open ([gap and limits](kde-world-v2-port.md#fixed-size-admission-consumer-inventory-and-proposed-core-shape-2026-10-29)). Partial-zero apps, fixed/max intersections, hung hints and borderless-game no-touch proof pending. |
| 14 | M | R-WS-08/11 history/ring remainder | H | Project local/global-unique/shared workspace histories and rings replace KWin desktops; retain scope, hotplug and preset assertions. | **P**: [scope/hotplug][hotplug] records single-output native selection/history/ring checks in all modes. Both-output independence/swap/shared-history and physical hotplug/presets pending. |
| 15 | M | R-WS-01/14 follow/stay sends | A | Numbered/relative follow/stay, wrap and trailing fill; preserve native boundary focus. | **P**: [scope/hotplug][hotplug] records single-output native sends in all modes; [item 22][item22] samples float combos. Full physical, relative/boundary and multi-output permutations pending. |
| 16 | M | R-MOV-03 same-axis move | A | Both selected move policies, live settings reread without tree rebuild. | **O**: [same-axis move][sameaxis] proves movement offline; scoped native Settings Apply/Revert/no-rebuild is supplementary. Actual native move topology and physical trigger journey pending. |
| 17 | M | R-DRAG-08 press-focus | H | No Windows modifier-drag producer or press-focus required; native title-bar drag stays native and inactive resize must not gain project focus. | **P**: [press-focus][pressfocus] is historical modifier-press evidence plus offline revalidation; physical feel/native edge coverage for the native drag pending. Per user decision 2026-10-18 modifier+drag is a Linux idiom not replicated on Windows. |
| 18 | M | Q3 born-maximized; B6 minimums | A | Both born-max policies plus minimum-origin placement, overlay/slot restoration and quiet settlement. | **P**: [born-maximized][bornmax] records ordinary Paint both-values physical/native samples; [placement][placement] proves bounded origin/minimum geometry. Browser/session/fixed/game/DPI and broader B6 interactions pending. |
| 19 | M | R-MAX-03 Q3 scope | A | Retiling a floating workspace reserves a tiled slot while retaining maximize; restore returns to that slot. | **P**: [retile overlay][retile] records single-output physical retile/restore acceptance. Hidden/session/fixed/failure/game/DPI variants pending. |
| 20 | M | Tray workspace toggle remainder | H | Windows tray tiling checkbox plus Win+Y, current-workspace mode and future-workspace default seeding replace Plasma tray actions. | **O**: [workspace shortcut][toggle] records delivered routing but no positive native toggle/sync acceptance. Physical both-way toggle, presets/rebind and Apply/Revert remainder pending in [backlog][backlog]. |
| 21 | M | Active border over applet popups | H | Replace Plasma applets with Start, taskbar flyouts, Task View and Alt+Tab; suppress stale border and restore the actual eligible foreground without stealing focus. | **P**: [active border][border] proves scoped Start/Task View suppression and Alt+Tab hold-cancel. Physical flyouts/topmost/occlusion matrix pending, not waived or assumed equivalent. |
| 22 | M | Host-settings Revert; tray indicator | H | Windows saved-draft Revert is not KDE host-override rollback. Check saved-settings reload plus session Mouse Snap takeover/restoration and truthful tray conflict state separately. | **P**: [dialogs][dialogs] proves native saved-file Revert/preview/stale refusal; [admission follow-up][admissionlive] proves bounded SPI restoration. [Gaming][gaming] warning rule is offline; full external-Snap/config roundtrip and physical indicator acceptance pending. |
| 23 | M | Confirmed-2026-09-29 remainder | H | Carry forward behavior, not KDE build/install steps: Windows Settings, underlay/drag visuals and eventual installed launch. Packaging acceptance remains a separate dependency. | **P**: [tabs][tabs], [drag][drag] and [threading][threads] contain scoped native evidence. Underlay black/white/grey physical check, full drag visual matrix and installed-route proof remain in [backlog][backlog]. |
| 24 | K | Quiet refresh logs | H | Windows bounded run-log/console summaries replace KWin refresh logs; preserve immediate safety/decision/failure evidence. | **P**: [trace bounds][trace] plus [backlog][backlog] 2026-10-14 readable-console dogfood; long trace-off/release comparison remains pending. Only listed cosmetic issues may ship. |
| 25 | M | Same-output drag/preview remainder | A | Native title-bar preview/drop agreement, no premature sibling writes, Esc/cancel/stop and no stale show; no modifier-drag producer required. | **P**: [drag][drag] has bounded native title-bar evidence plus historical Win-drag/preview evidence; [render-thread acceptance][threads] leaves physical pixels, cancellation/races and feel pending. |
| 26 | K | Drop-intent edge leftovers | H | Test native title-bar snap-back, Esc timing, size constraints and edge/drop-policy assertions; no modifier-drag leg or KDE-only callback/oracle IDs such as p13. | **O**: [drag][drag] covers cancellation/unsupported drop fixtures; exact remaining edge matrix pending. The core safety assertions in check 25 remain M. |
| 27 | M | Convergence step-1 remainder | A | Close/reopen focus, state restoration, unreadable observations and rapid commands must converge without stale writes. | **O**: [world fixtures][world] and [item 22][item22] give bounded related coverage, not the complete Windows sequence. Exact native remainder pending. |
| 28 | M | Resilience batches | H | Replace KWin/D-Bus endpoint faults with Windows owner/input/render failure seams and ledger-backed recovery; keep visual failure distinct from owner loss. | **P**: [threading][threads] covers offline failure/teardown seams and native bursts; [managed-workspace recovery][managed] proves bounded watcher recovery. Full failure matrix/N1 remainder pending. |
| 29 | K | Tray Active status in development | H | Windows development-owner tray liveness/Settings/Stop and shell re-add. systemd service-status sub-legs are N/A; no Windows systemd equivalent is proposed. | **P**: [pager lifecycle][pagerlife] proves tray re-add after actual Explorer restart; wider stale/dead-owner/status display permutations pending. |
| 30 | M | Process-loss and sleep, single output | H | Windows exact-owner loss/watchdog/independent restore plus native suspend/resume replace Planner/controller loss; reobserve rather than replay setters. | **P**: [managed workspaces][managed] proves scoped forced-loss recovery. Windows sleep/wake and combined recovery journey remain pending; stop/restart is not sleep evidence. |
| 31 | M | Native border delivery/suppression | H | Owned nonactivating layered border carrier replaces the KWin effect; retain click-through, foreground tracking, fullscreen suppression and recovery. | **P**: [active border][border] proves bounded composed/shell/fullscreen-helper evidence; [threading][threads] leaves physical renderer, games and mixed-DPI checks pending. |
| 32 | M | Sticky adoption restart | H | Exact-window-lifetime Windows sticky/float markers replace host/store observation; preserve both sticky origins and visibility. | **P**: [restart][restart] and [hydration protection][hydrate] prove repeated bounded adoption/frame/visibility preservation, including rejected first observations. Full physical/session-expiry matrix pending; no new un-stick policy. |
| 33 | M | Non-visible workspace tiling | H | Windows product-hidden SW_HIDE members retain background membership/planning without native geometry writes; project the latest state safely on reveal, without a workspace switch/focus steal for hidden admission. This replaces compositor-managed hidden-client presentation, not the background-planning obligation. | **P**: [managed workspaces][managed] and [admission follow-up][admissionlive] show bounded hide/reveal evidence. Corrected 40-helper hidden/visible gap repair at `79442633` retained identical victim sets and survivor plans with zero hidden geometry writes; full tree/focus parity and complete background-admission/geometry acceptance pending. |
| 34 | M | Gaming and work-area reliability | A | Preserve fullscreen/game non-interference, correct work areas after changes and M16 measured incremental game cost. | **O**: [foundation][multi] has work-area fixtures; [backlog M16][backlog] explicitly says game cost remains unmeasured. Idle CPU/wake results and [pager-event investigation][pagerevents] are not gaming acceptance. Native work-area/game checks pending. |
| 35 | K | Custom Tile manual runtime | X | KDE-specific development component; no Windows counterpart in the mapped product. Core tiling remains covered elsewhere. | **X**: [shared inventory][triage]. |
| 36 | K | Nested placement visual smoke | A | Inspect shared nested placement and rendered geometry on Windows, without inferring visual quality from tests. | **U**: [backlog][backlog] still queues Windows underlay/nested visual acceptance; no exact completed Windows visual-smoke record located. |
| 37 | K | Tray live/release acceptance | H | Windows installed tray/Settings/Stop/login/startup lifecycle replaces KDE tray/service packaging. | **U**: development-owner evidence in [backlog][backlog] does not establish installed/login acceptance; Windows package/update channel remains undecided under [scope][scope]. K does not waive a separately required delivery decision. |
| 38 | K | Drag-oracle post-fix diagnostic proof | X | KWin-specific oracle diagnostic; no Windows oracle endpoint counterpart. Windows behavioral proof stays in checks 18/25/26/47. | **X**: [shared inventory][triage]; exclusion is diagnostic-only, not a drag waiver. |
| 39 | M | Startup adoption | H | Windows in-process Engine/current observation replaces Planner loss; fresh owner re-adopts eligible frames with fit/fallback and exception preservation. | **P**: [managed workspaces][managed], [restart][restart] and [hydration protection][hydrate] establish bounded native startup/restart behavior; [shared fit][fit] is policy evidence only. Exact fit/fallback and fresh-owner loss matrix pending. |
| 40 | K | KWin controller silent-unload diagnostic | X | No KWin controller in Windows; do not equate this diagnostic with permission for silent owner failure. Owner loss remains checks 28/30/50 and WX7. | **X**: [shared inventory][triage]. |
| 41 | K | Correlated observability capture | H | Windows opaque tokens, action correlations and monotonic run-log timings replace KWin multi-endpoint capture; omitted/coalesced history must be explicit. | **P**: [visibility diagnostics][visibility] has scoped normal-level emission evidence; full overflow/exception/incident and whole-system correlation remain pending. |
| 42 | M | Q4 whole-workspace migration | H | Project-owned per-output membership/views replace native desktop moves; retain layout, source refill, sticky split and fullscreen/maximized carry. | **O**: [Windows migration][migration] delivers native-adapter fixtures; real two-output arrivals/partial failures and all-mode integration pending. [W2a][world] is not positive world-route acceptance. |
| 43 | M | R-MOV-08/R-OUT-04 output move/send | A | All four directions, local move first, current destination, proper selection/arrival and follow/stay fences. | **O**: [foundation][multi] and [explicit output send][outsend] deliver fixtures. Native multi-output/mixed-DPI, float-boundary, suppression and refusal journeys pending. |
| 44 | M | K1 visual remap across scales | H | Windows physical/full/work rectangles, PMv2 conversion and separate border/underlay/preview/pager carriers replace compositor-local mapping. | **O**: [foundation][multi], [migration][migration] and [pager lifecycle][pagerlife] have coordinate/reconfiguration fixtures. Real negative/offset origins, mixed-DPI transitions, cursor/preview agreement and visual proof pending. |
| 45 | M | Born-fullscreen dogfood | A | Real app/game born fullscreen beside tiles, exit/re-enter and untouched game input/frame; not a helper-only fullscreen pass. | **U**: [fullscreen exit][fullexit] and [backlog][backlog] explicitly leave this born/main-window/game journey pending; the modal-overlay sample is not this pass. |
| 46 | M | Directional/shortcut/convergence re-test | H | Windows owned-chord takeover and saved-settings Apply/Revert/advisory conflict preview replace KGlobalAccel Apply/Force/host Revert; keep actual directional/convergence outcomes. | **P**: [dialogs][dialogs] proves UI/persistence, [backlog][backlog] samples physical directions; no Windows foreign-holder Force operation. Full physical chord/OS delivery and convergence re-test pending. |
| 47 | M | Cross-output drops | A | Native title-bar drag into occupied/empty destination layouts; no modifier-drag producer required. Preview/drop agree, source reflows and failed arrivals do not commit membership. | **O**: [foundation][multi] delivers adapter/real-Engine fixtures. Physical two-output/mixed-DPI, cancellation and lag/refusal recovery pending. |
| 48 | M | KWin restart with surviving Planner | H | No independent surviving Planner on Windows. Restart the owner with apps alive, preserve exact-lifetime intent/recovery obligations, then rebuild from current observations; no saved tree promise. | **P**: [restart][restart], [hydration protection][hydrate] and [capacity restart][admissionlive] prove scoped orderly restart. Full D25 set/membership/provable-focus and session matrix pending. |
| 49 | M | Reconciliation phase 1 | A | Quiet settlement after delayed/clamped/stale observations and rapid multi-output operations; no departure/ping-pong or replay. | **O**: [foundation][multi], [output sends][outsend] and [world fixtures][world] cover related fences. Native phase-1 multi-output matrix pending. |
| 50 | M | Process-loss and sleep, multi-output | H | Windows owner/watchdog recovery and suspend/resume with both output-local workspaces; refresh identities/topology and restore hidden claims. | **U**: [threading][threads] explicitly defers multi-output/process-loss legs; no accepted combined Windows journey located. |
| 51 | M | Output hotplug displacement/return | H | Windows stable output mappings, project workspace displacement/return and mode-specific histories replace host desktop/output bindings. | **O**: [scope/hotplug][hotplug] has simulated topology coverage, not physical removal. Unplug/reconnect, changed contents/explicit moves while absent and mixed-DPI native proof pending. |
| 52 | K | Multi-output anti-oscillation | A | Repeat trailing-empty/close/send/switch lifecycle without ping-pong or tiling stop; retain K only for listed nonfatal edge issues. | **P**: [scope/hotplug][hotplug] and [managed workspaces][managed] sample single-output trailing lifecycle. Exact multi-output anti-oscillation repetitions pending. |
| 53 | K | Native development lifecycle removal/dogfood | X | KDE script/effect development-setup removal path is not a Windows artifact. Product visual unavailability stays N1; Windows installed stop/removal/recovery stays WX7 and the separate delivery gate. | **X**: [shared inventory][triage]. |

### Shared additions N1-N4

| ID | Class | Shared check | Map | Windows assertion / host equivalent | Evidence and exact remaining acceptance |
| --- | --- | --- | --- | --- | --- |
| N1 | M | Missing/disabled/removed effect | H | Windows has in-process render-owned carriers, not a separately installed KWin effect package. Disable visuals and exercise carrier creation/refusal/unavailability while keeping tiling operational; no stale overlays or silent stop. Package-remove/disable-KWin-effect actions are N/A, not the failure-tolerance assertion. Owner/input/render process failure is separately checks 28/30/50 and WX7. | **P**: [threading][threads] adds 2026-10-18 native optional-border/underlay-off tiling and hide/reveal; pager remained enabled, preview producer unexercised. Renderer creation refusal and hide/teardown faults remain offline-only. Full visual-unavailable native journey and the accepted failure boundary pending; do not claim a killed integrated owner continues tiling. |
| N2 | M | External host configuration | H | External Windows Snap/work-area settings and tiler-file edits replace KWin configuration edits. Reconfigure truthfully; distinguish saved-file Revert from owned session Snap restoration; no unintended writes. | **P**: [dialogs][dialogs] proves a preview-time external file change is preserved/refused; [admission follow-up][admissionlive] proves bounded restore. [2026-10-18 gap-file follow-up](archive/windows-owner-event-idle.md#scoped-native-follow-up-2026-10-18) observes wake/adoption, not exact convergence. Matched two-helper 8/12/8 at `b274797f` reproduced two physical default-helper position deltas while PMv2 helpers matched; exact restoration passed. Host-vs-adapter cause and full outside-Settings host-change/reconcile/ownership roundtrip remain open. |
| N3 | M | Sleep with fullscreen game | A | Sleep/wake with a real fullscreen game and tiled session; game unimpeded, no stuck holds, correct recovery. | **U**: [backlog][backlog] and [Windows lifecycle plan][winplan] supply pending scope/design, not native evidence. |
| N4 | M | Scaling with overlays | H | Change Windows DPI/resolution/work area with maximized/fullscreen/fixed-float windows; fresh physical-coordinate conversion and overlay/pager reanchor, no fullscreen geometry writes. | **O**: [pager lifecycle][pagerlife] covers stale-hide/fresh-geometry/DPI fixtures, expressly not physical reconfiguration; [foundation][multi] is not a simulated native DPI transition. Native matrix pending. |

## Approved Windows-specific extras

- These eight extras extend or sharpen existing gate categories; they are not
  added to the historical 57 tally. Shared journeys need not be run twice when
  one retained evidence set establishes both assertions. All are approved
  **must-pass**, with only the explicitly bounded Authentic Win+G/Win+F11
  exact-chord containment gap in WX1 carried as a listed limitation under D4.
- No new capability, host mechanism, urgency design or shell-selection policy
  is selected. Unsupported native outcomes remain TBD in the existing linked
  discriminators; this gate sets acceptance questions, not invented answers.
- Existing D27 defaults and Windows OS-permitted activation amendment remain
  unchanged ([gaming][gaming]); neither denied-request samples nor default
  policy establish complete urgency mark/clear or real-game activation evidence.
  This gate does not move the post-0.1 Activation request setting into 0.1.

| ID | Extra / related checks | Proposed Windows acceptance and grounding | Current evidence |
| --- | --- | --- | --- |
| WX1 | Gaming/Xbox coexistence and M16 (1/6/9/34/45/N3) | Physical Compatible Ctrl+Win+G / Ctrl+Win+F11 press/hold/release/plain-Win and takeover-off routing; no accidental Game Bar/Xbox action. Authentic warning surfaces obey positive-conflict proof and Settings navigation never claims remediation. Measure active/suspended incremental game CPU/frame-time/input cost against tiler-off, not idle cost; no automatic Xbox-mode heuristic or anti-cheat guarantee ([gaming][gaming], [M16 backlog][backlog]). Propose disclosing the already parked Authentic G/F11 containment gap, not waiving real-game safety or invisible-window recovery. | **P**: consumed F11 plus visible Xbox prompt and later incident are evidence of a limitation, not containment success. Rebind/warning changes are offline; physical alternate chords, game cost and clean Xbox-return recovery pending. [Pager investigation][pagerevents] retains the 2 s timer because event completeness is unproven. |
| WX2 | Explorer/TaskbarCreated and pager (4/21/29/31/37) | Actual Explorer restart and synthetic broadcast recover tray/pager from fresh geometry; auto-hide and opt-out remain respected, no foreign shell injection. Distinguish observed tray re-add from unlogged pager broadcast receipt; retain fullscreen/elevation suppression ([pager lifecycle][pagerlife]). | **P**: actual restart, auto-hide and live opt-out passed in bounded primary bottom-taskbar evidence. Secondary/non-bottom support is safely refused; physical multi-output/pager/input/appearance and accessibility remain pending, not approved exceptions here. |
| WX3 | DPI/display/taskbar coordinate changes (2/44/47/51/N4) | Verify PMv2/full-vs-work/visible-vs-outer conversions, offset/negative outputs, mixed-DPI crossing, taskbar insets, fresh tray anchors and no stale carriers after resolution/scaling. Unsupported anchors must fail safely with tray fallback, not guessed placement ([foundation][multi], [migration][migration], [pager lifecycle][pagerlife]). | **O**: coordinate/reconfiguration fixtures exist; physical display/DPI transition and multi-output acceptance pending. No fixed-support expansion is proposed. |
| WX4 | Sleep/wake and session return (30/50/N3) | Separate native sleep/resume from orderly restart; reobserve topology/lifetimes, preserve hidden-window recovery obligations and avoid replayed writes/stuck holds. Include lock/unlock/session return as the Windows-specific lifecycle discriminator in the [Windows plan][winplan]; do not infer an implemented WTS/power policy from design. | **U**: lifecycle design exists; no accepted Windows sleep/session-return matrix located. Single- and multi-output/game legs pending. |
| WX5 | Elevated windows, UAC and secure desktop (13/21/28/31) | Verify the selected medium-integrity default: elevated/protected windows stay unmanaged, unharmed and not product-hidden; no automatic elevation/UIAccess. Separately verify UAC secure-desktop/return does not receive tiler geometry/focus/overlay control or stale queued effects. Owned-chord consumption and action eligibility are separate; no new pass-through policy is inferred ([Windows plan][winplan], [gaming][gaming]). | **U**: documented default and offline integrity/action fences are not UAC-consent/secure-desktop acceptance. Exact native interception/return outcomes remain pending/TBD. |
| WX6 | Native virtual desktops and shell coexistence (14/21/33/39) | Keep project workspaces and the approved SW_HIDE mechanism; native desktops are not adopted as a per-monitor workspace backend. Observe native desktop switch/return, Task View/Alt+Tab/taskbar and overlay pollution without relocating native desktop membership or promising hidden-window shell listing. Existing switch-versus-pull remains deferred ([presentation research][presentation], [backlog][backlog]). | **U**: presentation research distinguishes live visibility facts from unobserved shell visuals; no accepted native-desktop coexistence journey located. Task View recoverability for the *alternative native-desktop model* is not recovery proof for hidden SW_HIDE windows. |
| WX7 | Hidden-window owner crash/recovery (11/28/30/48/50) | Verify write-before-hide ledger, exact-lifetime watcher reveal, independent restore, minimize/maximize preimages and truthful uncertain-identity residue; no foreign writes or false-clean shutdown. Verify current recovery after owner/input/render loss, not just normal Stop. The existing design does not promise unconditional no-strand under unprovable identity ([managed workspaces][managed], [threading][threads], [presentation][presentation]). | **P**: bounded older watcher/forced-loss and independent restore journeys pass; later orderly restart/hydration passes do not replace the current combined failure matrix. Multi-output, uncertain/partial recovery and session combinations pending. |
| WX8 | Best-effort admission/capacity parity (10/13/15/27/33/42/43) | Native true fresh exhaustion/largest-splittable fallback, shortest-prefix/protected shrink, failed-unfloat intent retention/retry, same-output sends, actual overlap/navigation and hidden-repair parity. Preserve accepted T2 cross-output capacity-float refusal; no FloatOrigin-driven policy or unsupported remap ([admission follow-up][admissionlive], [world prerequisite][world]). | **P**: bounded Settings/restart pass, newest-suffix shrink and corrected hidden/visible victim-set/plan parity at `79442633`; fresh/send/retry no-space preconditions and one-pixel unfloat cause remain unproved. Physical default-helper gap deltas confirmed at `b274797f`; matched PMv2 readbacks exact, host-vs-adapter cause parked. Repaired unfloat fixture parked safely before intent. World T2 diagnostic currently reproduces a gap, not refusal parity. User 2026-10-19 closed the separate live re-check and carried its remaining legs here: true fresh exhaustion/fallback, native minimal-prefix and protected-member shrink, failed-unfloat retention then retry, capacity sends follow/stay, overlap overflow, and two-output T2 refusal. A default-policy 500-arrival replay never exhausted a domain; the next attempt uses a few helpers with large declared minimum sizes (adding minimum-size support to the owned test window if it lacks it). The one-pixel cause moves to the shared host-settled geometry backlog item. |

## User decisions

- User approval 2026-10-19: D1, D2, D3 and D4 are each accepted exactly as
  written below, including their stated boundaries and consequences. The
  recommendation text is retained verbatim as the approved decision wording.
- Effective gate: 45 base must-pass / 7 known-issue-allowed / 5 excluded,
  plus eight must-pass extras WX1-WX8. N1 owner/thread-loss recovery remains
  separate in checks 28/30/50 and WX7. Approval does not change mappings,
  evidence cells or pending native acceptance.

| Decision | Recommendation | Consequence / alternative |
| --- | --- | --- |
| D1: Windows host equivalents and exclusions | Accept the A/H/X matrix, with only five full-check exclusions: 5, 35, 38, 40, 53 (all originally K). Retain all 43 M checks and the Windows equivalents of their safety assertions. | H is substitution, not reduced rigor. Check 21 physical shell-popups stays pending, not an exception; check 29 systemd and N1 KWin-package sub-steps are inapplicable only as stated. Reject/adjust individual equivalents explicitly rather than infer blanket host parity. |
| D2: Original W-class checks 3/4 and inherited K rows | Make original checks 3 and 4 Windows 0.1 must-pass. Keep the seven applicable K checks (24, 26, 29, 36, 37, 41, 52) known-issue-allowed only when listed and with no crash or silent tiling stop. | Proposed effective base: 45 must-pass, 7 K, 5 excluded. No current M failure becomes a K issue; package/update-channel selection and installed delivery remain separate dependencies even though tray polish is K. |
| D3: Windows companion-failure boundary (N1) | Accept optional visual disable/unavailability and carrier-creation failure as the Windows effect-absent equivalent, with tiling continuing; retain integrated owner/thread-loss recovery separately. | There is no removable KWin-effect package or surviving external Planner on Windows. If this is insufficient, specify a stronger failure-isolation requirement before claiming N1 equivalence; no architecture change is authorized by this proposal. |
| D4: Windows extras and Authentic collision limit | Add WX1-WX8. Require physical Compatible routing, real-game safety/cost and Xbox-return recovery; carry only the already disclosed Authentic G/F11 exact-chord containment gap as a listed limit. | Requiring reliable containment of original G/F11 instead would reopen the parked blocker. Conflict avoidance is not suppression; no game-safety, recovery, elevation or secure-desktop failure is proposed as a known issue. |

## Dependencies, outcome and verification

- Delivery dependencies are unchanged: [world adoption][world] needs positive
  migration/refill authority, source-preserving T2 refusal, atomic cutover and
  hidden/fullscreen/bootstrap parity; [best-effort native follow-up][admissionlive]
  needs its retained-input discriminators. No release pass follows from their
  offline test totals. Windows package/update-channel selection and installed
  acceptance remain in [scope][scope] / [backlog][backlog].
- Presentation choices: reuse original IDs/classes; count whole
  checks rather than sub-legs; group overlapping extras to avoid duplicate runs;
  use conservative P/O/U/X evidence labels, never a blanket pass. A/H/X choices,
  W-to-must-pass treatment, N1 boundary and extras are adopted under D1-D4.
- Record acceptance: all 57 IDs/classes mapped once, explicit Windows
  equivalents/exclusion reasons, evidence pointers and pending gaps on every
  row, eight grounded extras, and four decisions approved exactly as written.
- Verification: documentation-only inventory/count/link and whitespace checks passed;
  no code or desktop tests performed. Approval update verified links, ASCII
  additions and `git diff --check`; mappings and evidence cells are unchanged.
  Durable decisions and backlog now reflect the approved gate; no principles
  or macOS records changed. This record remains active for pending acceptance.
- Next action: complete the remaining Windows native/physical checks against
  the runtime route that will ship, tracked under the P0 0.1 release item.
  No Windows 0.1 gate completion is claimed.

## Evidence sources

[scope]: ../decisions.md#scope-and-platform-goals
[triage]: ../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2
[backlog]: ../backlog.md
[world]: archive/windows-full-world-state-replies.md
[admissionlive]: windows-best-effort-admission-live.md
[gaming]: windows-gaming-coexistence.md
[pagerevents]: archive/windows-pager-event-coverage.md
[pagerlife]: archive/windows-taskbar-pager-lifecycle.md
[floatfull]: archive/windows-floating-fullscreen-frame-restore.md
[item22]: archive/windows-item22-verification.md
[restart]: archive/windows-restart-intent.md
[hydrate]: archive/windows-restart-hydration-rejection.md
[hotplug]: archive/windows-workspace-scope-hotplug.md
[multi]: windows-multi-output-foundation.md
[outsend]: archive/windows-explicit-output-send.md
[migration]: archive/windows-workspace-output-migration.md
[managed]: archive/windows-managed-workspaces.md
[threads]: archive/windows-owner-threading-acceptance.md
[tabs]: shared-settings-catalog-windows-tabs.md
[dialogs]: archive/windows-shortcut-settings-dialogs.md
[border]: archive/windows-active-border.md
[visibility]: archive/windows-visibility-diagnostics.md
[trace]: archive/windows-trace-volume.md
[fixed]: archive/windows-fixed-size-admission.md
[drag]: archive/windows-mouse-drag.md
[fit]: placement-aware-startup-adoption.md
[winplan]: ../research/windows-port/plan.md
[presentation]: ../research/windows-port/alt-tab-hidden-workspaces.md
[placement]: archive/windows-placement-correctness.md
[unfloat]: archive/windows-maximized-intentional-unfloat.md
[fullexit]: archive/windows-fullscreen-exit-acceptance.md
[sameaxis]: archive/windows-same-axis-move.md
[pressfocus]: archive/windows-drag-press-focus.md
[bornmax]: archive/windows-born-maximized-setting.md
[retile]: archive/windows-maximized-floating-retile-overlay.md
[toggle]: archive/windows-workspace-tiling-shortcut.md
