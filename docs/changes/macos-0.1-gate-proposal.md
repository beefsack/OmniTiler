# macOS 0.1 gate proposal

- Status: APPROVED 2026-10-29 with amendments (D1-D3 as written, D4 amended,
  D5/D6 rejected; see Proposed decisions and the [durable approval](../decisions.md#macos-01-gate-approval-and-01-scope-principle-2026-10-29)).
  Native/physical acceptance remains pending; this is not release acceptance.
- Basis: [shared scope and platform goals](../decisions.md#scope-and-platform-goals)
  (2026-10-17/18) and the approved
  [57-check inventory](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2)
  (original 53 = 39 must-pass / 12 known-issue-allowed / 2
  Windows-release; N1-N4 add four must-pass checks, total 57 = 43 / 12 / 2).
- This record remains active pending gate acceptance; acceptance work
  tracks against it until the gate passes.

## Goal, scope and acceptance

- Map all 53 original checks and N1-N4 to macOS exactly once, preserving
  their original labels (53 = 39 must-pass / 12 known-issue-allowed / 2
  Windows-release; with N1-N4, 57 = 43 / 12 / 2). The approved macOS
  treatment is 43 must-pass / 7 known-issue-allowed / 7 excluded, plus
  eight macOS-specific extras (MX1-MX8, all must-pass). The D5/D6 sub-leg
  exceptions are rejected: the native Settings window and the product
  underlay attempt are 0.1 scope.
- Record approved-to-propose host equivalents, exclusions and grounded
  macOS additions; distinguish existing evidence from outstanding
  acceptance. No runtime, functional specification or principles changes;
  no new live testing was run for this proposal.
- Approach: inventory the shared template, inspect current and archived
  macOS records, reconcile platform differences, then verify IDs, counts
  and links. All 57 mappings and extras are reviewed together; physical
  and native acceptance, the v2 port completion and the managed-workspace
  redogfood remain separate dependencies.
- Logical applicability versus host mechanism replacement: the shared
  behavioral assertion always applies unless excluded with a reason.
  A different native mechanism (Mission Control AX or Dock-swipe instead
  of a compositor desktop switch, CGEventTap instead of KGlobalAccel,
  menu-bar status item instead of a Plasma pager or taskbar overlay) is a
  host-difference adaptation (H), never a waiver. Only genuinely
  non-applicable product surface (reference-host confirmation, KDE-only
  components, Windows-only journeys) is excluded (X).

## Mapping and evidence

- Mapping: **A** = applies as-is (same behavioral assertion; use macOS
  input names); **H** = host-difference adaptation (macOS equivalent
  stated); **X** = not applicable (reason stated). Different
  implementation alone does not justify an exception.
- Original class: **M** = must-pass, **K** = known-issue-allowed,
  **W** = the original Windows-release class, not a macOS waiver. Both W
  rows are Windows-product journeys; they are proposed excluded (X) for
  macOS under D1, with macOS placement/settings/indicator coverage
  carried by the base M rows and MX extras instead.
- Evidence: **P** = partial macOS native/physical evidence, with remaining
  acceptance pending; **O** = offline implementation/fixtures only for
  the mapped journey, native acceptance pending; **U** = pending, no
  accepted macOS journey located for this check; **X** = excluded, no
  macOS acceptance needed under D1. No row is claimed fully passed.
  A related pass does not complete a larger check or establish evidence
  for a changed runtime route. P rows cite historical pre-v2 evidence
  (scoped-preview and owned-fixture legs); those historical passes are
  preserved as partial history, not current-route proof, and are
  rechecked after the v2 cutover rather than downgraded by newer legs
  run under different conditions.
- Source baseline: repository records reviewed 2026-10-28. In particular,
  the world-contract-v2 port is partial (stage 1 complete offline, stage
  2 and offline stage-3 work partial, targeting/receipt cutover and
  direct event-driven wake accepted offline only) and presentation plus
  native acceptance remain open ([port record](macos-world-v2-port.md)).
  Recheck affected journeys after v2 completion and after the targeted
  redogfood; do not transfer legacy scoped-tile evidence automatically
  where the route changed. Latest source records take precedence over
  historical backlog wording.

| Inventory | Applies as-is | Host-adapted | N/A | Extras |
| --- | --- | --- | --- | --- |
| Original 57 checks | 20 | 30 | 7 | Not counted here |
| macOS additions | Not classified A/H/X | Not classified A/H/X | 0 | 8 |

| Evidence status | Partial native/physical (P) | Offline only (O) | Pending/unverified (U) | N/A (X) | Complete gate passes |
| --- | --- | --- | --- | --- | --- |
| Original 57 checks | 8 | 24 | 18 | 7 | 0 |
| Eight extras | 4 | 1 | 3 | 0 | 0 |

- All 50 applicable base checks and all eight extras retain pending
  acceptance; P/O are not alternative spellings of a full pass.
- Multi-output must-pass checks require a two-display setup and pass
  before 0.1 can ship. macOS "Displays have separate Spaces" constrains
  feasible views; equal workspace counts are set only for a proven host
  requirement. Native two-display arrangement validation is pending
  ([topology evidence](archive/macos-full-display-topology.md)).

### Original checks 1-53

| ID | Class | Shared check | Map | macOS assertion / host equivalent | Evidence and exact remaining acceptance |
| --- | --- | --- | --- | --- | --- |
| 1 | M | Reference G-05/G-06/G-37/G-D2; D28 fullscreen carry | H | Shared newcomer predicate, focus/move mutex, source refill and maximized carry. macOS explicit fullscreen send must carry once the logical-membership design lands; never force-exit; current refusal is a limit, not a pass. Requires REQUIRED USER-RUN REAL GAME leg for the D28 carry plus incremental game CPU/frame-time/input cost against tiler-off (idle cost is not a substitute). | **U**: no accepted macOS reference-comparison or fullscreen-carry journey; current evidence is send-refused plus home-desktop-not-returning ([direction](../decisions.md#macos-dogfood-review-decisions-2026-10-27)). Fullscreen logical-membership design plus one bounded user-run probe is ROI item 4. |
| 2 | M | Two-candidate output move/send/migration | A | Same target-selection, arrival and refusal assertions; window-centre/span ranking versus whole-workspace largest-edge ranking. | **O**: shared fixtures only; native two-display journeys pending, including topology discrimination and Space-order (not arrangement) grounding. |
| 3 | W | Placement physical acceptance | X | Windows session-dogfood journey, not a macOS product check. macOS placement acceptance rides checks 13/18/44 and MX7. | **X**: [shared inventory](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2). |
| 4 | W | Windows parity/settings/tray physical | X | Windows Snap/Settings/tray/Explorer/DPI journey, not a macOS product check. macOS settings/indicator acceptance rides checks 12/20/22/29/37 and MX3/MX6/MX8. | **X**: [shared inventory](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2). |
| 5 | K | B9/R-FLT-06 on COSMIC | X | Reference-WM confirmation on COSMIC, not a macOS product check. macOS B9 remains check 7. | **X**: [shared inventory](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2). |
| 6 | M | B1/B2 repeats; held-key autorepeat | A | Repeat maximize/sticky native-change cycles and physical hold/release; one toggle per intended activation, no stuck consumption or OS leakage. | **O**: offline repeat/refusal rules only; physical delivery and native-change repetitions pending. |
| 7 | M | B9 unfloat while maximized | A | Clear/observe maximize, then fresh tiling admission; preserve focus and fixed-origin overrides. | **U**: no accepted macOS B9 journey; `maximized: false` assumptions are not capability proof ([risks](macos-world-v2-port.md#risks-and-open-evidence)). |
| 8 | M | D7 tile-override restart | A | Same tile-override marker semantics on restart; markers confer no write/recovery authority. | **U**: no accepted macOS D7 restart journey located. |
| 9 | M | D5 born-fullscreen exit, including games | H | On first native fullscreen exit, reclassify using current hints; no fullscreen geometry interference. Fullscreen windows stay untouched by ordinary tiling. Requires REQUIRED USER-RUN REAL GAME leg plus incremental game CPU/frame-time/input cost against tiler-off (idle cost is not a substitute). | **U**: restored-fullscreen outcomes remain TBD discriminators ([classification](macos-window-classification.md)); option A reporting selected, native user-run acceptance pending. |
| 10 | M | D6 tiling enable with fixed windows | A | Classify all candidates under the current fixed predicate; preserve intentional/explicit-tile intent and no-touch automatic floats. | **O**: [classification delivered offline](macos-window-classification.md); native enable matrix and maximum-hint availability pending. |
| 11 | M | Q3 R-RST-01/03/04 | H | Restart intent/frame/store through the macOS owner and native Space adoption; no saved-layout restore or new cross-login identity. | **U**: D25/D26 native dogfood assessment pending; no accepted macOS restart matrix located. |
| 12 | M | D1 settings UI | H | Native Settings dialog is 0.1 scope (D5 rejected 2026-10-29): check 12 requires native Settings-window acceptance. Predicate changes affect later admission, not mass reclassification. Validated save/read/reload/default/predicate semantics stay must-pass; reuse shared settings-window elements and validate them across platforms with the Windows/KDE Settings windows. | **O**: file/CLI route exists; native Settings dialog plus validated save/read/reload/default/predicate semantics, external-edit reconcile and live pickup pending. |
| 13 | M | Q2 fixed admission, including borderless game | A | Keep the selected predicate and fixed-origin no geometry/focus/stacking writes on automatic admission. Requires REQUIRED USER-RUN REAL GAME no-touch proof including a game-shaped window plus incremental game CPU/frame-time/input cost against tiler-off (idle cost is not a substitute). | **O**: [classification offline](macos-window-classification.md); no-touch user-run proof pending. |
| 14 | M | R-WS-08/11 history/ring remainder | H | Project workspace histories/rings over native Spaces; retain scope, hotplug and preset assertions. Native Spaces first; no virtual/parking substitute. | **P**: numbered select plus relative/previous selection pass live on owned windows ([checkpoint](macos-managed-workspaces.md)); trailing lifecycle, presets and two-display legs pending. |
| 15 | M | R-WS-01/14 follow/stay sends | A | Numbered/relative follow/stay, wrap and trailing fill; preserve native focus gates. Bridged native move is default with synthetic title-bar drag fallback; sends are user-selectable later (shelved 3c). | **P**: numbered send-follow and default-unbound send-stay pass live; inverse/relative/two-display and trailing send-follow focus fix pending ([record](macos-managed-workspaces.md)). |
| 16 | M | R-MOV-03 same-axis move | A | Both selected move policies, live settings reread without tree rebuild. | **P**: directional move passes physically in the scoped preview ([hardening](archive/macos-dogfood-hardening.md)); native move topology on the v2 route pending. |
| 17 | M | R-DRAG-08 press-focus | H | No modifier-drag producer on macOS; plain click focuses, hover off. The producer is a passively observed native title-bar drag, matching the Windows title-bar route. | **P**: decided click-focus holds in preview; passive-producer classification pending, Meta+left stays diagnostic-only ([foundation](macos-drag-drop-overlays.md)). |
| 18 | M | Q3 born-maximized; B6 minimums | A | Both born-max policies plus minimum-origin placement, overlay/slot restoration and quiet settlement. | **O**: classification/hint plumbing offline; maximum-hint availability unresolved, native born legs pending. |
| 19 | M | R-MAX-03 Q3 scope | A | Retiling a floating workspace reserves a tiled slot while retaining maximize; restore returns to that slot. | **O**: offline only; native retile/restore legs pending. |
| 20 | M | Tray workspace toggle remainder | H | Actual workspace TILING TOGGLE remains must-pass: current-mode toggle, future-default seeding, and shortcut/menu equivalent. Menu-bar indicator numbers select workspaces but do not substitute for the toggle. Core trailing logical slot is surfaced by the pager without eagerly creating a native Space. | **U**: no accepted macOS toggle journey located; source review found no macOS toggle wiring. Plain-numbers quick check passed historically; toggle, click handling, Space-switch updates, multi-display and icons pending ([record](macos-workspace-indicator.md)). |
| 21 | M | Active border over applet popups | H | AppKit nonactivating click-through carrier above its verified target replaces the KWin effect; suppress stale border and restore the eligible foreground without stealing focus. | **O**: [implementation plus owned-fixture evidence](macos-active-border.md); appearance, fullscreen suppression and game/context acceptance pending (TBD discriminators). |
| 22 | M | Host-settings Revert; tray indicator | H | macOS Authentic may take over conflicting shortcuts with explicit Revert and owned preimages; scoped preview changes no native settings. Truthful menu-bar conflict/status indication separately. | **O**: Authentic ownership and Revert rule selected, not natively accepted; per-chord override verification is a smaller item pending ([scope](../decisions.md#scope-and-platform-goals)). |
| 23 | M | Confirmed-2026-09-29 remainder | H | Carry forward behavior, not KDE build/install steps: validated settings, directional focus/move, gaps and quiet idle; installed launch rides MX8. Product group underlay is 0.1 scope (D6 rejected 2026-10-29): attempt before 0.1, reference-first from sibling JankyBorders SkyLight-drawn borders; on failure report evidence to the user. Border/preview/drag safety is unaffected. | **P**: gaps, directional focus/move and quiet idle pass physically ([hardening](archive/macos-dogfood-hardening.md)); underlay/product-visual, drag and installed-route proof pending. |
| 24 | K | Quiet refresh logs | A | Bounded run-log/console summaries; preserve immediate safety/decision/failure evidence. | **P**: quiet idle logging passes physically; long trace-off/release comparison pending. Only listed cosmetic issues may ship. |
| 25 | M | Same-output drag/preview remainder | H | Native title-bar drag preview/drop agreement, no premature sibling writes, Esc cancellation; no modifier-drag producer required. | **U**: passive producer parked; no accepted native drag journey ([foundation](macos-drag-drop-overlays.md)). |
| 26 | K | Drop-intent edge leftovers | H | Native title-bar snap-back, Esc timing, size constraints and edge/drop assertions; no KDE-only oracle IDs. | **O**: offline Engine preview/drop plumbing only; exact remaining edge matrix pending. Check 25 safety assertions stay M. |
| 27 | M | Convergence step-1 remainder | A | Close/reopen focus, state restoration, unreadable observations and rapid commands converge without stale writes. | **O**: offline fixtures only; focus-walk first-dispatch flake diagnosed and unrepaired, native remainder pending. |
| 28 | M | Resilience batches | H | Owner/input/render failure seams with ledger-backed recovery; keep visual failure distinct from owner loss. | **O**: offline failure/teardown seams only; full failure matrix and N1 remainder pending. |
| 29 | K | Tray Active status (`just dev`) | H | Menu-bar indicator liveness/Settings/Stop in development; no systemd equivalent. | **P**: indicator quick check passed; wider stale/dead-owner/status permutations pending. |
| 30 | M | Process-loss and sleep, single output | H | Exact-owner loss/watchdog/independent restore plus native suspend/resume; reobserve rather than replay setters. | **U**: sleep/wake never exercised on macOS; no accepted recovery journey. Stop/restart is not sleep evidence. |
| 31 | M | Native border delivery/suppression | H | Owned AppKit carrier replaces the KWin effect; click-through, foreground tracking, fullscreen suppression and recovery. | **O**: owned-fixture delivery only; fullscreen suppression, games and mixed-scale checks pending. |
| 32 | M | Sticky adoption restart | H | Sticky only with positive all-Spaces proof; restart preserves proven sticky identity and visibility. | **U**: sticky scope needs a per-display verification plan; no accepted journey ([port record](macos-world-v2-port.md)). |
| 33 | M | Non-visible workspace tiling | H | Off-Space members retain background membership/planning without native geometry writes; project the latest state safely on reveal, without a workspace switch or focus steal for hidden admission. | **O**: offline membership/report facts only; native hide/reveal checkpoints (visible home, hidden intermediate, reveal with retained layout) pending. |
| 34 | M | Gaming and work-area reliability | A | Preserve fullscreen/game non-interference, correct work areas after changes. Requires REQUIRED USER-RUN REAL GAME leg plus incremental game CPU/frame-time/input cost against tiler-off (idle cost is not a substitute). | **U**: reliability/game-safety work never exercised on macOS (ROI item 6); user-run acceptance pending. |
| 35 | K | Custom Tile manual runtime | X | KDE-specific development component; no macOS counterpart. Core tiling remains covered elsewhere. | **X**: [shared inventory](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2). |
| 36 | K | Nested placement visual smoke | A | Inspect shared nested placement and rendered geometry on macOS, without inferring visual quality from tests. | **U**: appearance pending; no accepted macOS visual-smoke record. |
| 37 | K | Tray live/release acceptance | H | Signed DMG plus Homebrew installed lifecycle/login/autostart replaces KDE tray/service packaging. | **U**: development-owner evidence only; install path, signing/notarization and update channel open under [scope](../decisions.md#scope-and-platform-goals). K does not waive a required delivery decision. |
| 38 | K | Drag-oracle post-fix diagnostic proof | X | KWin-specific oracle diagnostic; no macOS counterpart. macOS behavioral proof stays in checks 17/25/26/47. | **X**: [shared inventory](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2). |
| 39 | M | Startup adoption | H | Host-owned native-Space adoption in native order with membership preserved and no launch moves replaces Planner loss; fresh owner re-adopts eligible frames with fit/fallback. | **O**: core adoption delivered, macOS consumption partial; four-Spaces-to-four-workspaces native proof pending ([adoption](../decisions.md#native-adoption-2026-10-24)). |
| 40 | K | KWin controller silent-unload diagnostic | X | No KWin controller on macOS; do not equate this diagnostic with permission for silent owner failure. Owner loss remains checks 28/30/50 and MX2/MX3. | **X**: [shared inventory](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2). |
| 41 | K | Correlated observability capture | H | Bounded action correlation, opaque lifecycle tokens and monotonic timings replace KWin multi-endpoint capture; omitted/coalesced history stays explicit. | **O**: offline diagnostics only; overflow/exception/incident and whole-system correlation pending. |
| 42 | M | Q4 whole-workspace migration | H | Project logical membership/views over host-owned Spaces; retain layout, source refill and fullscreen/maximized carry. No automatic Space relocation beyond the selected send routes. | **O**: executor/desire plumbing offline; native arrival/partial-failure and all-mode integration pending. |
| 43 | M | R-MOV-03/R-OUT-04 output move/send | A | All four directions, local move first, current destination, proper selection/arrival and follow/stay fences. | **O**: offline fixtures; native two-display/mixed-scale, float-boundary and suppression journeys pending. |
| 44 | M | K1 visual remap across scales | H | FULL plus visible work-area rectangles with logical-point rounding replace compositor-local mapping; border/preview/pager carriers reanchor, no fullscreen geometry writes. Underlay reanchor is 0.1 scope (D6 rejected 2026-10-29): attempt before 0.1 as in check 23. | **O**: FULL/work-area report fixtures pass ([topology](archive/macos-full-display-topology.md)); second-display arrangement validation and mixed-scale transitions pending. |
| 45 | M | Born-fullscreen dogfood | A | Real app born fullscreen beside tiles, exit/re-enter and untouched frames; not a helper-only pass. Requires REQUIRED USER-RUN REAL GAME leg plus incremental game CPU/frame-time/input cost against tiler-off (idle cost is not a substitute). | **U**: user-run born-fullscreen/game journey pending. |
| 46 | M | Directional/shortcut/convergence re-test | H | CGEventTap as sole hotkey route plus saved-settings Apply/Revert replace KGlobalAccel Apply/Force/host Revert; keep actual directional/convergence outcomes. | **P**: directional focus/move pass physically; Carbon retired after 0/3 physical delivery; physical chord delivery/readiness/smoothness pending. |
| 47 | M | Cross-output drops | A | Native title-bar drag into occupied/empty destination layouts; preview/drop agree, source reflows, failed arrivals never commit membership. | **U**: producer parked; no accepted cross-output drop journey. |
| 48 | M | KWin restart with surviving Planner | H | No independent surviving Planner on macOS. Restart the owner with apps alive, preserve intent/recovery obligations, rebuild from current observations; no saved-tree promise. | **O**: orderly-restart plumbing only; full D25 set/membership/provable-focus and session matrix pending. |
| 49 | M | Reconciliation phase 1 | A | Quiet settlement after delayed/clamped/stale observations and rapid multi-Space operations; no departure/ping-pong or replay. | **O**: offline fences only; native multi-Space matrix pending. |
| 50 | M | Process-loss and sleep, multi-output | H | Owner/watchdog recovery and suspend/resume with per-display Spaces; refresh identities/topology and restore hidden claims. | **U**: never exercised on macOS; no accepted combined journey. |
| 51 | M | Output hotplug displacement/return | H | Stable display mappings and project workspace displacement/return replace host desktop bindings. | **U**: plug/unplug never exercised on macOS; simulated topology only; physical removal/reconnect pending. |
| 52 | K | Multi-output anti-oscillation | A | Repeat trailing-empty/close/send/switch lifecycle without ping-pong or tiling stop; retain K only for listed nonfatal edge issues. | **O**: single-display trailing lifecycle only; exact two-display repetitions pending. |
| 53 | K | Native development lifecycle removal/dogfood | X | KDE script/effect development-setup removal path, not a macOS artifact. Product visual unavailability stays N1; macOS stop/removal/recovery stays MX2/MX3 and the separate delivery gate. | **X**: [shared inventory](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2). |

### Shared additions N1-N4

| ID | Class | Shared check | Map | macOS assertion / host equivalent | Evidence and exact remaining acceptance |
| --- | --- | --- | --- | --- | --- |
| N1 | M | Missing/disabled/removed effect | H | macOS has in-process AppKit carriers (border, indicator, preview), not a separately installed KWin effect package; there is no independently removable effect package on macOS. The carrier-creation/refusal/unavailability equivalent is accepted under D3: disable visuals and exercise carrier-creation/refusal/unavailability while keeping tiling operational; no stale overlays or silent stop. Package-remove actions are N/A, not the failure-tolerance assertion. Owner loss stays checks 28/30/50 and MX2/MX3. | **U**: visual-unavailable native journey pending; creation-refusal/hide faults offline only. Never claim a killed integrated owner keeps tiling. |
| N2 | M | External host configuration | H | External display/Space/menu-bar settings and tiler-file edits replace KWin configuration edits. Reconfigure truthfully; distinguish saved-file reload from owned session restoration; no unintended writes. | **O**: file/CLI route exists offline; outside-change reconcile/ownership roundtrip pending. |
| N3 | M | Sleep with fullscreen game | A | Sleep/wake with a fullscreen session and tiled windows; game unimpeded, no stuck holds, correct recovery. Requires REQUIRED USER-RUN REAL GAME leg plus incremental game CPU/frame-time/input cost against tiler-off (idle cost is not a substitute). | **U**: never exercised on macOS; user-run acceptance pending. |
| N4 | M | Scaling with overlays | H | Change display resolution/scale/work area with maximized/fullscreen/fixed-float windows; fresh logical-point conversion and carrier/pager reanchor, no fullscreen geometry writes. Underlay reanchor is 0.1 scope (D6 rejected 2026-10-29): attempt before 0.1 as in check 23. | **O**: report/geometry fixtures only, not a physical reconfiguration; native matrix pending. |

## Proposed macOS-specific extras

- These eight extras extend or sharpen existing gate categories; they are
  not added to the historical 57 tally. Shared journeys need not run
  twice when one retained evidence set establishes both assertions. All
  are proposed **must-pass**.
- No new capability, host mechanism, urgency design or shell-selection
  policy is selected. Unsupported native outcomes remain TBD in the
  existing linked discriminators; this gate sets acceptance questions,
  not invented answers.

| ID | Extra / related checks | Proposed macOS acceptance and grounding | Current evidence |
| --- | --- | --- | --- |
| MX1 | Managed native-Space lifecycle (14/15/42/43/49/52) | Numbered/relative/previous/trailing select, send-follow/stay, inverse follow, stay/source focus, two create/auto-remove cycles and unknown-occupancy removal refusal on run-owned Spaces through the selected Dock-swipe route. MC AX fallback arms only where the macOS release refuses Dock-swipe, per the already-decided deferral (backlog 3b/3c); the fallback is conditional, not an immediate must-pass. Lazy backing only: a native Space is created on first switch/send, never by eager append; core trailing workspace surfaced by the pager. Stale indicator clicks refuse. | **P**: numbered select, send-follow and unbound send-stay pass live; trailing armed after offline review; Dock-swipe follow 88-98 ms and trailing reuse 122 ms measured ([record](macos-managed-workspaces.md)). Trailing send-follow focus fix, stay/source-focus, inverse follow, two create/auto-remove cycles, two-display and opened/pinned contrast pending. |
| MX2 | Tap health and input recovery (6/28/30/46) | Consume-first parity with Windows; health-check re-enable on stall is the accepted product route ([live guide](../live-macos-testing.md#user-run-physical-checks), [recovery](../decisions.md#scope-and-platform-goals)); OS timeout notification is an opportunistic faster path, not mandatory. Lock, Secure Input and unknown-session release; tap release on disable/exit verified separately from physical delivery. | **P**: consume-first delivered; health-check recovery live-proven after a real stall without timeout notification ([record](archive/macos-consume-first-input.md)). OS timeout-notification recovery opportunistic, not mandatory; release-on-exit proof pending. |
| MX3 | Identity, signing, TCC and scope (28/30/46/48) | Stable bundle ID/install path/one persistent self-signed development identity; LaunchServices attribution (`ax_trusted:true`) before AX effects; fixture-only scope `io.github.omnitiler.live-fixture`; guarded input ownership and independent restoration before dependent effects. | **P**: fixture first-use and fixture-scoped integrated tiling pass ([evidence](archive/macos-fixture-tiler-integration.md)); persistent-identity recipe stands. Release signing/notarization separate. |
| MX4 | App-hidden lifecycle (D23 family; 11/33/48) | Cmd+H reports explicit minimized facts with retained lifetime/close plumbing and distinct hidden-vs-minimize diagnostics; never restore or focus the hidden lifetime; hide-then-terminate emits a close. Reversible adapter mapping; shared `minimized` documentation broadening deferred pending KDE agreement. | **O**: option B implemented and regression-covered offline ([port record](macos-world-v2-port.md)); native hide/unhide/terminate lifecycle pending. |
| MX5 | Fullscreen membership and dedicated Spaces (1/9/31/45/N3) | Native fullscreen windows stay logical members; no force-exit; ordinary tiling never touches fullscreen frames; dedicated fullscreen Spaces omitted from `native_order` with empty logical view while current; fullscreen-Space scope and sticky subsets stay fenced TBD. | **U**: direction selected, design plus one bounded user-run probe pending (ROI item 4); current send-refused limit stands ([dogfood direction](../decisions.md#macos-dogfood-review-decisions-2026-10-27)). |
| MX6 | Menu-bar indicator productization (20/22/29/N4) | Single-display numbers/click/current-highlight/occupancy-unknown handling; ambiguous multi-display scope hides the item rather than guessing; numbers beyond armed 1-9 visible but disabled; application icons excluded from this proposal per the existing deferral. Indicator numbers select workspaces; the check-20 tiling toggle is separate and not substituted. | **P**: plain-numbers quick check passed historically; click handling, Space-switch updates, current-highlight/unknown handling, appearance and game/fullscreen acceptance pending ([record](macos-workspace-indicator.md)). |
| MX7 | Sleep, hotplug and scaling on native Spaces (30/44/50/51/N3/N4) | Native suspend/resume, display plug/unplug with displacement/return, and resolution/scale/work-area changes with then-current Space contents; reobserve topology/membership, preserve hidden claims, no replayed writes or stuck holds. Two-display setup required. | **U**: none exercised on macOS yet (backlog item 8). |
| MX8 | Distribution and installed lifecycle (22/29/37/48) | Signed DMG plus Homebrew; release signing/notarization, update channel and App Store decision closed before 0.1 ships; installed login/autostart/stop/removal acceptance. | **U**: DMG plus Homebrew selected; paid-account signing/notarization, updates and Store remain open ([scope](../decisions.md#scope-and-platform-goals)). No accepted installed journey. |

## Proposed decisions

| Decision | Recommendation | Consequence / alternative |
| --- | --- | --- |
| D1: macOS host equivalents and exclusions | Accept the A/H/X matrix, with only seven full-check exclusions: 3, 4 (both originally W, Windows-product journeys) and 5, 35, 38, 40, 53 (all originally K, reference-host or KDE-only surface). Retain all 43 original M checks and the macOS equivalents of their safety assertions. | H is substitution, not reduced rigor. Passive-drag legs stay pending, not excepted; N1 AppKit-carrier steps are applicable only as stated. Reject/adjust individual equivalents explicitly rather than infer blanket host parity. |
| D2: Original K rows and W treatment | Keep the seven applicable K checks (24, 26, 29, 36, 37, 41, 52) known-issue-allowed only when listed and with no crash or silent tiling stop. Exclude both W checks as Windows-product journeys under D1. | Proposed effective base: 43 must-pass, 7 K, 7 excluded. No current M failure becomes a K issue; package/update-channel selection and installed delivery remain separate dependencies even though indicator polish is K. |
| D3: macOS companion-failure boundary (N1) | Accept optional visual disable/unavailability and carrier-creation failure as the macOS effect-absent equivalent, with tiling continuing; retain integrated owner/thread-loss recovery separately in checks 28/30/50 and MX2/MX3. | There is no removable KWin-effect package on macOS. If this is insufficient, specify a stronger failure-isolation requirement before claiming N1 equivalence; no architecture change is authorized by this proposal. |
| D4: macOS extras and limitations carried (approved as amended 2026-10-29) | Add MX1-MX8 as must-pass, with only already-decided limits carried, not newly approved: dynamic radius shelved with version-derived default staying, route settings and native stress/fixture-harness work shelved, MC fallback deferred until Dock-swipe refusal, floor verification deferred with the macOS 15 support promise retained as explicit unverified risk, indicator icons excluded from this proposal per the existing deferral. SkyLight underlay evaluation is NOT deferred post-0.1: it is part of the 0.1 underlay attempt (D6 rejected). No game waiver is proposed. | Shelter/deferral is sequencing, not a safety waiver. No game-safety, recovery or fullscreen-exit failure is a known issue. First-float placement is decided for macOS (option A, no exception); the implementation follows. The carried radius/icon limits are subject to the P1 product-scope audit vs Windows 0.1. |
| D5: file/CLI settings exception for 0.1 (REJECTED 2026-10-29) | Rejected: the native macOS Settings window IS 0.1 scope, so check 12 requires native Settings-window acceptance before 0.1 ships. Validated save/read/reload/default/predicate semantics still must-pass. User rationale: Windows/KDE Settings windows and validating shared settings-window elements across platforms. | The file/CLI route stays, but it does not substitute for the native Settings dialog. |
| D6: post-0.1 underlay-feature sub-leg exception (REJECTED 2026-10-29) | Rejected: the product group-underlay visual legs in checks 23/44/N4 remain 0.1 scope. Attempt before 0.1 (Windows/KDE already have it), reference-first starting from sibling JankyBorders SkyLight-drawn borders (see [group underlay](macos-group-underlay.md) and [rendering routes](../research/macos-port/underlay-rendering-routes.md)). Existing border/preview/drag safety is unaffected. | On failure report evidence to the user; no automatic deferral. No other visual omission is implied or approved. |

## Known gaps, unresolved choices and blockers

- Known gaps stay pending, never passed; no live substitution: v2
  port completion (native executors, presentation, native acceptance);
  redogfood items 3a/3d (trailing send-follow focus fix, stay latency,
  smoothness/readiness, multi-display, opened/pinned contrast); two-display
  arrangement validation; best-effort admission adoption (ROI item 5);
  FFI unsafe review (decoupled, ROI item 7); drag/drop overlays (ROI item 8);
  reliability/game-safety work (ROI item 6); small fixes first (focus-walk
  fence, pager core-trailing check, `just dev` log-follower leak,
  `just launch` exit code).
- Resolved 2026-10-29: D5 rejected (native Settings window is 0.1 scope),
  D6 rejected (product underlay attempt is 0.1 scope), first-float adopts
  option A for macOS with no exception. Still open: fullscreen
  logical-membership design; distribution channel plus paid
  signing/notarization/update decisions; App Store stance. Already-decided
  MC fallback, floor and route-settings timing is not reopened; the carried
  icon/radius limits are subject to the P1 product-scope audit vs Windows 0.1.
- macOS 15 floor: support promise retained; floor-release verification
  deferred as explicit unverified risk.
- Source ambiguity: macOS classification discriminators (AX-size-only
  admission, accessory documents, elevated-level standard windows,
  app-specific nonstandard documents, sheet identity, restored-fullscreen
  reference outcomes); group-underlay chord and rendering route;
  title-bar drag classification and cross-output integration; active-border
  appearance pixels; sticky per-display scope; notch-straddling geometry;
  separate-Spaces-off spanning domains. Each resolves through its
  existing TBD row, not through this gate.
- Packaging/distribution dependencies accurately linked: DMG plus
  Homebrew selected with release signing/notarization, update channel
  and App Store open ([scope](../decisions.md#scope-and-platform-goals));
  backlog distribution item; MX8 acceptance pending. No installed pass
  is claimed.
- Reliability and game safety are absolute: sleep/wake, output hotplug,
  resolution/scaling/config changes and fullscreen handling must pass on
  two outputs; game legs are REQUIRED USER-RUN REAL GAME acceptance with
  incremental game CPU/frame-time/input cost against tiler-off (idle cost
  is not a substitute); the tiler must never crash or silently stop
  tiling. K rows ship only as listed issues under D2 with those
  invariants intact.

## Verification

- Documentation-only inventory: 57 original IDs mapped exactly once
  (1-53 plus N1-N4); classes preserved from the approved inventory;
  approved treatment 2026-10-29: 43 M / 7 K / 7 X plus eight MX extras
  (MX1-MX8, all must-pass).
- Counts: A=20, H=30, X=7 across the base; evidence P=8, O=24, U=18,
  X=7; extras evidence P=4, O=1, U=3; complete gate passes 0.
- Checks to run after any edit: `git diff --check` and
  `git status --short`. No code or desktop tests performed or claimed.
- Approved 2026-10-29 (D1-D3 as written, D4 amended, D5/D6 rejected); this
  record stays active for pending acceptance. The approval is also recorded
  in durable decisions.
