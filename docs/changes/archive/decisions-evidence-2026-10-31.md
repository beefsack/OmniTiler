# Decision migration evidence, 2026-10-31

This archive retains implementation and test evidence displaced from current
decision fields. It is not a policy register.

## World rendering and stress

- Latest-world rendering / arrival disturbance reduction: implementation and
  protocol design were not adopted; the unknown-foreground policy and click/app
  activation versus OS fallback attribution remain unresolved. Physical Alt-Tab
  validation and source-fenced protocol changes remain outstanding; empty
  arrivals are included in planned measurements.
- Stress acceptance: the native pager proof joins passed all six matrix legs
  after the Windows directional CLI origin-refresh repair, without waits or
  weakened gates. CLI measured 3.4 actions/s overall and 101 ms between
  contiguous commands.
- KDE renderer freshness: offline actual-facts/broker/JS/native-endpoint route
  passed failing-first regressions; static member-move and focus leave/return
  freshness passed physically. Rapid A/B and in-flight old-paint checks remain.
- Fullscreen presentation: KDE enabled offline; Windows/macOS adoption remains
  pending. Physical mixed-scale dual-output checks passed output-local hiding,
  unaffected output, sends, switch hide/reveal and exit restoration; shortcuts
  acting on fullscreen window remain unconfirmed.
- Float toggle and implicit output targeting: output-targeting implementation
  was offline only with no native acceptance claim; float implementation and
  Windows parity proof remain pending.
- Gesture capture recovery and freshness: physical signal/painting checks remain
  pending. D-Bus delivery proof uses asynchronous `Introspect`, not settle wait.
- Native adoption: accepted four-to-four outcome remains TBD in discriminator.
- Cross-output gesture placement: physical placement confirmed, gesture-local
  gap bookkeeping accepted offline. Ghostty sizing shortfall is external; no
  retries/workarounds selected. Draft upstream report was not filed.

## Interface and focus decisions

- State-centric visibility: v4 direction replaces prior visibility model; frame
  and member attempt lanes migrate separately. Implementation remains pending.
- R10 scale: v4 not implemented; live scale-change check is N4.
- R11 identity: v4 not implemented; KWin/Windows identity availability was
  unchecked.
- R12 crash-safe native writes: not implemented; KWin unload without a later
  start leaves state until next start.
- R14 window state and R18 scripting surface: v4 / command surface not
  implemented. macOS sticky remains unsupported until all-Spaces route proven.
- R20 anchored border: offline comparison recorded 160/129.14 coordinate-unit
  max/mean error for absolute replay and zero error for anchored descriptor
  under the same causal gate. Inference about native frame-event timing is not
  live evidence; physical check remains due per platform.
- Focus guards: not implemented; Windows maximize reporting and current
  Windows/macOS behavior were unchecked.
- Cross-output focus: delivered offline; physical acceptance pending. macOS
  keyboard integration was not implemented in the bounded source state.
- Birth-clear under geometry-convergence hold remains tentative. Only narrow
  independently proven birth-clear authority was selected; native-target tests
  unavailable on verification host remain unverified, not passing.

## Windows evidence

- Owner threading stage 1/2 offline and scoped agent live evidence recorded;
  physical visual/input checks remained pending. Stage 3 combined offline and
  scoped automated desktop acceptance recorded; physical visual/input,
  multi-output/mixed-DPI and gaming cost remained pending. Trace-on idle CPU is
  not game-cost acceptance.
- Stage 2 hide-before-drop timeout is 500 ms. Applied-render acknowledgement
  timings are distinguished from unmeasured same-thread fallback timings.
- Owner idle: post-correction eight-case matrix and taskbar-auto-hide response
  were measured; hidden samples were granularity-limited near zero without
  timeout increments in observed intervals. Pager maintenance explained idle
  timeouts. Deadline rounding changed observed rate 0.524/s to 0.498/s, without
  demonstrated CPU reduction. Retain 2 s visible-pager recovery through 0.1;
  coverage probe did not select a replacement.
- Output-targeting audit closed with regressions and no production route change.
- Multi-output: agent-driven directional transfer/focus evidence exists; user
  physical 2026-10-20 confirms cross-output move chords, remaining topology
  matrix pending. Real-game taskbar cost unmeasured. Legacy driver containment
  does not prove functioning fixture.

## Development and macOS evidence

- Development scope fixture integration evidence is linked from the scope
  decision; live fixture native run passed build/tests/clippy. MacOS fixture
  tests carry two open failures for native macOS investigation; integrated
  fixture run remains blocked by bundle-scope gap.
- macOS 15 floor estimate: Homebrew 30-day OS events sample fetched
  2026-10-14, period beginning 2026-09-14, showed 93.27% events on 15+ and
  71.72% on newer versions. This opt-in developer/event-weighted sample is not
  an all-user or arm64-only share; StatCounter misreports modern releases as
  Catalina. Private APIs still require per-release testing.
- macOS tap: a physical callback stall disabled the tap without timeout
  notification; enabled-state health check detected it and same-tap re-enable
  recovered. User-input notification/recovery and continued suppression were
  live-proven; timeout notification route was not required for recovery.
- Native workspace routes were manually verified for three transfer routes.
  Dock-swipe/MC fallback, exact expose gate and safe send routes retain separate
  physical/dogfood checks. Synthetic follow arrival measured 88-98 ms; first
  exact subject tile 270 ms versus 1000 ms backstop; fast stay may return before
  destination tiling.
- AX/WindowServer drag hit tests joined exact owned targets; spanning Dock
  backing did not invalidate helper hit proof; genuine terminal hits refused
  without posts.
- ControlCenter exact-SID removal passed once; opened/pinned contrast and two
  fresh automatic create/remove cycles remain pending. Subject-follow front pin
  was implemented/regression-covered but not live-verified on reused/new
  trailing targets.

## Licensing and toolchain

- Metadata/first-party notice delivery and dependency audit/CI gate were
  applied. The third-party notice generation policy and remaining GitHub binary
  delivery requirement are recorded in the current licensing decision.
- VS 2026 route passed native offline gates on 2026-10-10; live testing and
  clean-runtime acceptance remain separate.

## Settings, workspaces, layout and admission

- Windows settings catalog, native tabs, dialog flows, tray, pager, first-run,
  workspace scope/history/sends/migration, same-axis move, parent-orientation,
  output-send, fixed-size admission and restart intent have offline/native-agent
  delivery records. Physical shortcut/Snap/Xbox checks, multi-output journeys,
  alternate DPI/output arrangements, and selected pager/game/overlay cases
  remain user-owned or pending as scoped by their linked records.
- KDE settings catalog migration delivered offline with checked-in generated
  descriptors and CI freshness check; native Settings identities and settings-
  only package independence retained. Shortcut result-dialog flow and host
  conflict Fix were live checked; Revert/tray-indicator acceptance remains
  pending. Fixed-size admission, intent persistence, whole-workspace migration,
  aligned workspaces and best-effort admission have offline delivery records;
  native user-selected choices and two-output cases remain pending.
- Core fixed-size D1-D8/G-05 and best-effort admission delivered offline.
  Windows locked native gates recorded 1,950 passed, zero failed, one ignored
  for output migration and 2,500 passed, zero failed, one ignored for gaming
  shortcut Part B; these counts are limited to those named runs.
- Workspace transfer/focus evidence does not establish all native outcomes:
  physical two-output migration and send journeys, fullscreen/game variants,
  floating-boundary roundtrip, and unsupported reference outcomes remain
  pending/TBD. Workspace scope's startup discriminator is still TBD.
- Native adoption R19 core implementation and 19 settlement pins are delivered;
  native outcomes remain TBD. KDE removal implementation is offline evidence,
  not native acceptance. macOS native consumption/removal capability gating
  remains open. Four-to-four acceptance remains TBD in its reference row.
- Best-effort admission core/KDE delivery is offline; Windows/macOS native
  consumption and KDE live acceptance remain pending. Windows float-no-remap
  is a bounded refusal, not a tested remap behavior.

## Detailed source evidence statuses

- Settings/catalog: Windows catalog implementation and physical acceptance are
  tracked in [shared settings](../shared-settings-catalog-windows-tabs.md).
  KDE catalog exporter, generated descriptors and CI freshness check delivered
  offline; both KCM identities and settings-only package independence remain.
  KDE host conflict Fix was live-confirmed; Revert and tray indicator remain
  pending. Windows synthetic/native UI, Apply/Revert, geometry/border/SPI and
  cleanup proof passed; physical shortcut/Snap/Xbox and other DPI/output checks
  remain user-owned ([Windows settings](windows-settings.md)).
- Tray and pager: synthetic/native proof passed for Windows presets, stale-choice
  refusal, menu buttons, warning changes, Stop, TaskbarCreated and crash recovery.
  Real Explorer restart, physical input, alternate DPI/output arrangements,
  multi-output/game/auto-hide and over-wide strips remain unverified. One-output
  overlay product rendering, click-to-switch, live refresh and managed-fullscreen
  hide/return passed; unexplained child-parenting failure did not block selection.
  Pager icon/number, centered halo, accent parity and stacking were physically
  accepted on one output; focused accent was correct on both pager and border
  for 10/10 starts after owner-lifetime correction. Wider variants remain
  pending ([pager](../windows-taskbar-pager-icons.md),
  [dogfood](../windows-single-output-dogfood.md)).
- Workspaces: Windows scope policy, owner selection/sends/history/global swaps/
  shared trailing lifecycle delivered offline; single-output native CLI
  journeys passed in all modes, 17 topology regressions passed, and native
  clean-stop/restore repeated. Physical two-output journeys remain pending.
  Migration's locked native MSVC gates recorded 1,950 passed, zero failed,
  one ignored; non-local modes and two-output acceptance remain pending.
  Ordinary/combo intentional-float transfer delivered offline; WS1->WS2->WS1
  and destination overlay-exit acceptance pending. Windows focus restoration
  dogfood fix delivered offline; fullscreen/game variants remain pending.
  Directional no-focus regressions passed; physical no-focus shortcuts and
  output crossing remain unverified.
- Move/output commands: same-axis move native gates and agent-observed
  Apply/Revert/no-rebuild passed; physical directional journey pending.
  Parent-orientation native gates and agent-observed Settings/owner adoption
  passed; physical toggle/OS suppression pending. Four-direction/output-send
  Windows native gates recorded 1,892 passed, zero failed, one ignored; two-
  output journey and split-edge physical acceptance pending. Core/KDE selection,
  protocol/actions and Windows wiring are documented in the linked delivery
  records; the retained work-area placement remains separate from full-rectangle
  adjacency evidence.
- Fixed-size and restart: D1 setting, per-axis zero, fixed-size D1-D8/G-05,
  D5/D6 exit/enable, D7 restart override and Windows current-hint/enable paths
  delivered offline as recorded by their linked changes. Native hints/UI,
  game, fullscreen, modal-dialog and all user-selected-choice checks remain
  pending. F1 tiled-drift reconciliation is implementation-pending. Durable
  maximize-base policy is approved but not implemented. Windows intentional/
  sticky identity survived two graceful owner restarts on owned native helpers;
  physical/app checks remain pending. D25/D26 are verification-only and native
  acceptance pending; post-restart un-stick remains TBD.
- Reference research: deferred source-fill ledger records 302 residual cells
  across 92 rows; current host-source candidates are 40 (38 PaperWM, 2 sway),
  not guaranteed whole-cell closures. Reference live-test VMs remain parked.
- Shortcut/gaming: KDE Apply/reliability and Windows first-run/resize/Xbox
  implementation records distinguish offline/native-agent evidence from user
  physical acceptance. Windows Xbox Part B locked four-package gates recorded
  2,500 passed, zero failed, one ignored; fmt, strict clippy and diff-check
  passed. Physical shortcut suppression and ownership remain separate from
  these results. `GamingHomeApp`/Game Bar probe behavior passed tests as recorded
  in [coexistence](../windows-gaming-coexistence.md).
- Placement/startup: KDE origin+minimum actuation delivered offline with
  effective-target equality and bounded host-shortfall acceptance; native
  R-MIN-01..03 journeys remain user-owned. Admission D01 focus fences and
  CLOSE-05 removal/refill/focus were verified offline; native cleanup pending.
  INS-04 later legs remain live-pending. R-INS-05 verification retained the
  existing focus behavior without policy change; native observation/activation
  acceptance remains user-owned.
- Capacity admission: core/KDE delivered offline on 2026-10-18; Windows/macOS
  native consumption and KDE live acceptance remain pending. Earlier
  no-geometric-fallback language is superseded; minimum-only B6 remains active.
  See [implementation, evidence and live re-check](best-effort-admission.md).

## Targeted baseline evidence dispositions

This section accounts for evidence and implementation-status facts in the
remaining scoped baseline slices. Product choices remain in their linked
decision topics; these bullets are evidence only.

- Baseline 5616-5742, Windows app-hide close (source line 5628): adapter
  implementation remains blocked on the durable-claim safety finding; reference
  outcomes support allocation removal, not identical native hide detection or
  focus defaults. [Current decision](../../decisions/window-state.md#windows-app-hide-close).
- Baseline 5616-5742, R4 (source lines 5691-5718): the 2026-09-25 selected
  behavior was offline-only, with no live verification claimed. The 2026-10-14
  own-mover tolerance is supported by the linked rightward correction and
  re-verification; only the exact source evidence there applies. Tier 1
  reference agreement is not physical acceptance.
- Baseline 5616-5742, D1-D5 focus (source lines 5719-5729): shared-core selector
  delivery is offline; reference consensus and physical acceptance are not
  inferred. macOS hotkey integration remains deferred. [Current policy](../../decisions/focus.md#cross-output-directional-focus).
- Baseline 5616-5742, portable per-domain API removal (source lines 5730-5735):
  Windows adoption was complete 2026-10-24; internal projections remain inside
  `WorldEngine`. [Current route](../../decisions/engine.md#kde-sole-world-route).
- Baseline 5787-6009, macOS v2 (source lines 5787-5804): implementation and
  production-presentation decisions were accepted offline; native acceptance is
  separate. Three-way classification and `None` membership remain tentative.
  [Current topics](../../decisions/macos.md#v2-implementation-cutover).
- Baseline 5787-6009, targeting/receipt (source lines 5806-5838): these outcomes
  were accepted offline; native acceptance remains separate. No native
  settlement-time change was made. The existing menu-bar indicator gained a
  miniature band; miniature-only repaint preserves click generation. The stale
  topology-drift refusal test was corrected; production behavior was not changed
  for that correction.
  [Current policy](../../decisions/macos.md#targeting-receipt-and-presentation).
- Baseline 5787-6009, T1-T4/lazy backing (source lines 5840-5868): T1/T3/T4
  confirmed; T2's 50 ms pump was interim and the measure-first prerequisite was
  superseded by the direct event-driven wake. No native before/after latency
  improvement numbers were claimed. T4 readback is fixture-only, not a product
  border-pixel claim; dynamic private SLS radius was later shelved. Lazy backing
  and trailing-pager follow-up were to verify core trailing-row presentation,
  not authorize eager append; implementation status is in the linked record
  and native acceptance remains separate.
  [Current decisions](../../decisions/macos.md#trailing-creation-and-presentation-settlement).
- Baseline 5787-6009, fullscreen games (source lines 5869-5877): a specific
  fullscreen Space could not yet be brought forward; send was refused and home
  desktop did not return fullscreen. This is a limitation, not final semantics;
  the chosen direction remains tentative. [Current policy](../../decisions/macos.md#fullscreen-games).
- Baseline 5787-6009, stress and standing limits (source lines 5882-5905):
  native stress runner is dev-only; no hidden-geometry-change claim beyond
  recorded zero-delta stream evidence; zero AX setter-attempt tracing remains
  parked; remembered focus on trailing append is a safe drop. The stated native
  acceptance legs (workspace fixes, pager pixels, input, sticky, multi-display,
  and post-wake performance) and older deferrals remain pending, not passed.
  [Current policy](../../decisions/macos.md#macos-ingress-stress-and-acceptance-limits).
- Baseline 5787-6009, follow-up evidence (source lines 5906-5931): queued
  select/send drain claim delivered at `3d64b5c5`; the exact-generation join
  draft failed review and was not retried; five exit/reap waits stay. Old live
  receipt was retired after two protected-window mismatches from system-service
  window churn; fresh read-only baseline is required before another fixture
  leg. Direct-wake acceptance needs deterministic offline proof and later
  re-dogfood, not native latency measurements. Native latency harness/tracing
  remain shelved.
  [Current review](../../decisions/macos.md#follow-up-review-and-investigation-priority).
- Baseline 5787-6009, virtual-workspace study (source line 5952): workspace
  model review rated prototype effort M and production effort L; gain was
  unquantified and sliver/recovery/fullscreen risks remained. These are study
  estimates, not selected product behavior.
- Baseline 5787-6009, ROI and deferred work (source lines 5932-5951): priority
  ordering, shelving and scope deferrals are current choices in the decision
  topic; implementation-pending and retired-receipt evidence is summarized
  above. The former post-0.1-only SkyLight-underlay deferral is superseded by the
  approved 0.1 gate; do not revive it. [Current gate](../../decisions/macos.md#macos-0-1-scope-and-gate).
- Baseline 5787-6009, trailing pager (source lines 5952-5991): marker and pager
  rules are accepted; implementation evidence is in the linked trailing-pager
  record; implementation was accepted in the source while native acceptance
  remains separate. No eager backing is evidenced.
  [Current policy](../../decisions/macos.md#pager-trailing-spare-and-native-workspace-representation).
- Baseline 5787-6009, 0.1 gate (source lines 5992-6009): decision dispositions
  D1-D6 and scope principle remain policy. The baseline tally (43 must-pass,
  7 known-issue-allowed, 7 excluded, MX1-MX8 must-pass) is a versioned gate
  snapshot, not general evidence or a rule for later scope.
- Baseline 6150-6204, Activities (source lines 6154-6162): tentative pending KDE
  evidence; no Activities implementation or native outcome is claimed.
  [Current decision](../../decisions/kde.md#deferred-activities-support).
- Baseline 6150-6204, configure/settlement (source lines 6163-6172): KDE
  Wayland pending leg remains un-reproduced and KWin completion-signal status
  unknown. Windows `01a73798` staged-frame/minimum-learning behavior is distinct:
  exact immediate target plus eligible later same-origin growth learns minima,
  with producer reporting prior attempts then fresh facts. Later facts are not
  native completion evidence. [Current decision](../../decisions/kde.md#deferred-asynchronous-configure-investigation).
- Baseline 6150-6204, connector/cleanup (source lines 6177-6191): ordered
  baseline `83d77f5c` retained; three native connector drafts withdrawn; logical
  receipts do not prove native hide/reveal. No connector build or claim of
  cleanup completion. [Current decision](../../decisions/kde.md#deferred-connector-and-cleanup-review).
- Baseline 6150-6204, highlight colors (source lines 6192-6204): no distinct
  test/progress evidence in this slice; policy remains in
  [visual decisions](../../decisions/visuals.md#independent-host-highlight-colors).

## Final workspace/layout/admission semantic audit

- The final bounded audit compared every decision, constraint, tentative or
  pending status, rationale, rejection and evidence/status clause in baseline
  lines 1911-3655, 5743-5786 and 6010-6149 against its single topic owner or
  explicit supersession. The baseline hash remained
  `842ec8c8f43c74f47f49546ffb48903804d4daf9`; it was not modified. A
  session-local audit ledger records source counts and clause-to-anchor
  mapping.
- Source top-level bullet counts: Settings 10, Workspaces 43, Move/Layout 12,
  Fixed-Size Admission 4, Restart Persistence 9, Reference/Spec 13, Shortcuts
  17, Gaming Safety 6, Placement/Minimums/Startup 11, Best-Effort Admission
  11; 136 total. The bounded topic owners contain 107 substantive entries.
- The empty-focus directional-command encoding is explicitly retained at
  `commands.md#directional-focus-without-a-focused-window`: only directional
  focus accepts explicit empty `win` for in-process and JSON callers; missing,
  null or invalid ids and empty ids on other commands remain strict.
- The final integration structural scan reported zero field violations and
  zero decision-link failures against the proposed index. The full corpus link
  scan reported 10 unrelated existing failures and zero failures in new topic
  or evidence-archive files. Topic/archive `git diff --check` was clean. Native
  and physical acceptance gaps remain pending as stated in their owned records;
  this audit makes no acceptance claim for those legs.
