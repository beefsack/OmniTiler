# Backlog

Only meaningful pending or active work is listed.
Completed and superseded status histories are in the
[backlog archive](backlog-archive.md).

Live work follows the applicable live-testing guide
([KDE](live-kwin-testing.md), [Windows](live-windows-testing.md),
[macOS](live-macos-testing.md)): agent legs use the live fixture scoped
fixture-only, and physical acceptance is user-owned. Architecture review
claims must be verified
before acting ([review](research/architecture-review/review.md)); user
decisions of 2026-09-24 are under
[Architecture Direction](decisions.md#engine-architecture-and-convergence).

## Open work

- Core/KDE order (user 2026-10-17): (1) KDE unfloat defect below; (2) live
  KDE acceptance of full-state replies (now owned by the KDE v2 port item
  below); (3) KDE Settings core-catalog
  migration (delivered offline); (4) best-effort admission core+KDE (delivered
  offline; live re-check below); (5) KDE dual-output live
  testing and fixes; (6) KDE shared window-policy adoption; (7) 0.1 triage live
  checks; (8) 0.1 packaging/NixOS/Ubuntu/licensing gates; (9) shared core and
  KDE module-size refactoring. Live passes recorded before the full-state
  route (2026-10-17) need repeating where that route changed the path.
- P0 | Windows dual-output live testing and fixes | User 2026-10-19 first
  two-monitor run. Delivered: (2)/(3) fixed (secondary pager bar-right anchoring
  user-accepted 2026-10-20); (1) DPI rebasing and exact-owner directional
  control accepted, Paint/Firefox transfers pass both ways with exact arrival
  and follow focus; Win+digit/send-follow/explicit-send, Explorer-restart
  survival with correct pager positions, and cross-output title-bar drag with
  drop overlay pass (Task Manager untiled: expected unmanageable class).
  (4) cursor-first fresh placement delivered in core and Windows; final native
  close-out accepted both migration directions, underlay show/release and
  directional maximize-clear continuation
  ([closing outcome](changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24)).
  Remaining, manual user checks: user dogfood re-check of (4); cross-output
  drag empty singleton and reverse-DPI start; W2M-10 partial; W2M-04/07/08,
  remaining topology, display changes and sleep/wake; primary anchor churn
  single observation (game cost unmeasured). The synthetic-input drag driver
  stays disabled fail-closed ([live invariant](live-windows-testing.md)).
  Full run history archived 2026-10-24. [Record](changes/windows-dual-output-live.md),
  [two-monitor checklist](changes/windows-physical-checklist.md#two-monitor-pc).
- P0 | Cross-platform cursor-first output targeting (User 2026-10-20) |
  Decided on [Tier 1 evidence](changes/archive/cursor-first-output-evidence.md):
  all-platform switch target cursor (default, COSMIC) or focused output (i3/sway);
  three-way new-window placement COSMIC (default: cursor output/last-focused
  window), Hyprland (cursor output/cursor-closest), i3/sway (focused
  output/last-focused). Sends use the focused window's output; relative
  next/previous targets count there too, no setting (Decision 1c). Focus-follows-
  cursor on/off and cursor-warp are settings, both default off (Decision 1d);
  i3/sway preset enables cross-output warp. Replaces R-OUT-05's old default.
  Shared-core placement where approved; platforms supply cursor/settings. Core
  switch/placement/send targeting and Windows delivered 2026-10-22
  (fixture-verified both outputs; rearchitecture closed 2026-10-24); active
  focus-follows-cursor and warp are stage 4 (default-off core/Windows delivered;
  native hover-follow unverified). KDE/macOS by their ports
  ([guide](porting/world-adapter-guide.md), [KDE](porting/kde-port-notes.md),
  [macOS](porting/macos-port-notes.md)).
  Tentative (user review pending): three-way placement subsumes launch targeting.
- P1 | Empty-workspace focus across outputs | User physical 2026-10-20:
  Win+digit to an empty workspace on one output focused a window on another,
  so a following workspace send moved that window. Desired behavior is
  undecided: stay focused on the empty output with no focused window, or move
  to another output's window? Capture Tier 1 outcomes in the
  [minimal action sequence](spec/reference-outcomes.md#empty-workspace-focus-across-outputs),
  then decide with the user. Related to cursor-first targeting above.
- P1 | Settings window behavior presets | User 2026-10-20: cross-platform
  convenience control (toggle, dropdown or buttons) sets ALL settings to match
  a reference tiler: COSMIC (default), i3/sway, Hyprland. Next: inventory every
  setting's per-preset value from Tier 1 evidence; leave missing evidence TBD.
  Hyprland warp stays off pending its own verb-dependent evidence row.
- P1 | Cross-output directional focus picks the window nearest the crossed edge |
  User dogfood trace 2026-10-20: `O1=W1`, `O2=H[W2 W3]`; Win+Right from W1
  selected W3, expected W2. User direction: last-focused only for switching
  workspaces on one output; output crossing chooses edge-nearest. Conflicts with
  current Engine adjacent-output last-focused tiled leaf ([decision](decisions.md#engine-operations-and-policy),
  [R-OUT-03](spec/reference-outcomes/multi-output.md#r-out-03-focus-left-across-outputs)).
  Next: confirm [minimal sequence](spec/reference-outcomes.md#cross-output-focus-edge-discriminator)
  and Tier 1 cells (destination >=2 tiles, MRU not edge-nearest), then inspect
  the trace to confirm W3 was O2's MRU versus a bug. Likely shared core behavior;
  any core change needs user approval.
- P1 | Dogfood permanent maximize-clear refusals | User 2026-10-20 accepted
  Windows pre-clear diagnostic priority with fullscreen veto, and B9
  `float-maximize-clear` diagnostics with fullscreen veto. Core-owned
  maximize-clear continuation is now delivered and fixture-verified on both
  outputs; watch for surprising remaining native safety refusals while
  dogfooding and revisit
  ([decisions](decisions.md#window-state-float-sticky-maximize-fullscreen)).
- P0 | Windows single-output dogfood 2026-10-19 | Delivered and pushed
  (`a372078e`, `6911b8d2`, CI green). Delivered: (1) fullscreen admin-game
  suspension behavior, (2) maximized return focus, (3) pager z-order/styling,
  halo and shared WinRT accent accepted; (5) black accent/startup crash fixed
  and accepted over 10 starts; (6) app-hidden-as-close adopted (release/reflow,
  fresh admission on reshow). Remaining user-run acceptance: app hide/reshow,
  desktop/Start pager, accent change while running
  ([record](changes/windows-single-output-dogfood.md)); (4) pager hidden by
  desktop-click/Start menu (misclassification fixed, native diagnostics added;
  user recheck pending). Hidden windows' Alt+Tab/taskbar presence is the
  separate post-0.1 item below. Full dogfood history archived 2026-10-24.
- P1 | Core/platform boundaries and shared acceptance baselines | User
  2026-10-18 baseline goal; the boundary is set by
  [Layering](principles.md#layering) and the user-approved 2026-10-20
  [core/platform decisions](decisions.md#core-platform-boundary), extending the
  [core-owned world-model direction](decisions.md#core-owned-world-model).
  Shared-core/Windows rearchitecture is complete with final native close-out
  ([outcome and limits](changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24));
  the core-only ordered baseline passes 6/6 checkpoints. Remaining: one shared
  core/platform case set with truthful host-state capture and documented platform
  gaps, native connectors and suitable fixture controls, with explicit deadlines
  only where justified by [Simplicity](principles.md#simplicity). The native
  connector/separation review and cleanup review are separate rows below;
  KDE/macOS adoption belongs to their active v2 port items.
- P1 | Native shared acceptance connector and separation review (inherits boundaries P1) |
  Deferred to the later separation review per the [closing outcome](changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24):
  the native side must reject missing readback, pending operations, omissions/
  fences, coverage defects and skipped intermediate hide/reveal before any live
  driver is accepted. The accepted core-only ordered baseline
  (`test-fixtures/world-actions.json`, 6/6 checkpoints via `world_acceptance`)
  stays; its logical receipts do not prove native hide/reveal. Three connector
  drafts were tried and withdrawn; they are not evidence.
- P1 | Code cleanup/abstraction review (inherits boundaries P1) | Deferred
  separate work per the [closing outcome](changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24):
  dead-API disposition delivered (`presentation_bytes`, batch hide machine,
  verdict ingress removed); remaining cleanup/abstraction review stays open, no
  known correctness defect waived. Coordinate with the P2 fixture follow-ups
  row and the repository cleanup pass.
- P1 | KDE v2 port (inherits boundaries P1) | KDE JS/Rust v1 consumers and the
  TypeScript born-maximized copy remain intentionally stale; the port replaces
  them together with no dual route, per the finalized
  [guide](porting/world-adapter-guide.md) and
  [KDE notes](porting/kde-port-notes.md). Approved port work: host-owned
  bootstrap/bindings and outside native changes; remote raw classification facts
  via broker transport to shared policy entry; both separate-view modes (linked
  vs separate views, unchanged tokens) subject to the KWin setting and required
  warning, not global-unique removal; equal workspace count as host limitation;
  native startup/outside-change adoption; independent host highlight colors.
  Deferred investigations: Activities/multi-desktop, async configure. Four
  minimal port discriminator rows keep untested native outcomes TBD; Activities
  defaults tentative pending KDE evidence. v1 KDE rows below keep their
  historical evidence; replacement belongs to this port. Status: staged plan
  recorded; host-owned native adoption and its revision (native evidence
  only, hybrid creation correlation, optional native current) delivered
  core-only with KDE consumption (native acceptance pending); Stage 2 v2
  broker cutover partial with five migration/reuse/lifecycle oracles failing.
  [Record](changes/kde-world-v2-port.md).
- P1 | Live fixture first use on macOS (2026-10-20) |
  `crates/tiler-live-fixture` is the standard agent live-test window
  (KDE live-verified, KDE `--scope-class`). Windows first use and
  fixture-scoped tiler smoke passed 2026-10-21 on two outputs
  ([evidence](changes/archive/windows-live-fixture-first-use.md)).
  macOS: standalone first use, the two `deadline_reader_*` test races and
  the bundle-scoping gap (signed regular-policy bundle
  `io.github.omnitiler.live-fixture`) resolved 2026-10-20
  ([evidence](changes/archive/macos-live-fixture-smoke.md)); fixture-scoped
  integrated tiling with the tap also passed one-to-three admission,
  socket/native geometry, focus, close/reflow and teardown
  ([evidence](changes/archive/macos-fixture-tiler-integration.md)).
  macOS remaining: product focus/move chords and managed select/send/trailing
  need physical fixture checks or a fixture-aware guarded diagnostic driver;
  existing owned controls/stay/trailing injections remain legacy-helper/TextEdit
  only ([checklist](live-macos-testing.md#fixture-first-live-tests)).
  [Reference](../crates/tiler-live-fixture/README.md).
- P2 | Live fixture follow-ups | Structured native outer-frame readback,
  clearable size constraints, fixture/native identity correlation, Windows
  elevated instance for UAC/unmanageable cases (physical approval), safe
  exclusive-mode selection, z-order query where hosts allow, then a bounded
  request-correlated change history. Add pragmatically as tests need them.
- P1 | KDE client self-resize while tiled (F1, 2026-10-20) | A tiled
  client resizing itself leaves geometry inconsistent with its tile until an
  unrelated subscribed event reconciles; reproduced on Wayland and X11 with
  the live fixture. First: capture Tier 1 reference outcomes and check
  existing spec rows, then user decides the desired behavior before repair.
  [Record](changes/archive/kde-dev-scope-fixture-integration.md).
  [Research](research/live-test-window.md),
  [record](changes/archive/controllable-live-test-window.md).
- P0 | KDE Meta+G unfloat does not re-tile until focus changes | Fixed offline
  2026-10-17, pending user re-check. The apply observation retained stale
  floating/fit-excluded flags after accepted unfloat; settle float departures
  and tile overrides before capture while preserving float placement timing.
  Producer-shaped production-entry regressions fail before the correction and
  pass after it; Rust workspace **2888 passed**, KWin **1772 passed**.
  [Evidence and script-restart re-check](changes/archive/kde-unfloat-immediate-retile.md).
- P0 | KDE dual-output live testing and fixes | Comprehensive native
  two-output inventory on physical outputs, not the researched VM route
  (User 2026-10-18). [Current evidence and parked questions](changes/kde-dual-output-live.md),
  [checks](#multi-output-setup), [coverage](research/two-output-test-environment.md).
  Aligned migration/send settlement, source-MRU stays, independent workspace
  switching and panel-gap vertical sends pass sampled native checks. New:
  both-output empty-end forward wrap, R4 authoritative nested-return shape and
  direct-API preview screenshots; deferred trailing-send focus fixed offline.
  Empty show creates no spare; each ordinary fill does. Policy is Tentative
  (user review pending). Pending: preset application/keyboard delivery,
  exact named-leaf R4 order, reviewed drag lifecycle, visible underlay, mapped
  effect identity and remaining inventory matrix. Hotplug stays parked.
  Rust 3353 / KWin 1920 pass; protected state/count/names/order restored,
  development stopped, project effect unloaded. Byte-exact config and full
  effects-baseline restoration remain unproven; no full inventory gate completed.
  Additional non-sticky multi-desktop membership retention remains TBD; under
  v2 it belongs to the deferred Activities/multi-desktop investigation in the
  KDE v2 port item, not the superseded conversion oracle.
- P0 | KDE floating dialog pushed behind other windows | Implemented offline
  2026-10-19 on the shipped world route: option A keep-above, focused automatic
  float preservation and correlated diagnostics; no legacy edits, Windows
  unchanged. D8 / REQ-SPC-04h / R-FLT-12 amended. KWin **1951**, Rust **3353**
  pass; strict gates clean. Native stand-ins exposed an admission-focus write,
  now fixed offline; corrected live repeat remains. **BLOCKED**: actual unload
  leaves owned above without recoverable ownership across restart; matching
  existing world behavior is unsafe. External lifecycle ownership question,
  incomplete baseline restoration (pointer/views, two retired empty desktops)
  and capture limits are in the [record](changes/kde-automatic-float-stacking.md).
  Concurrent fullscreen/game, final modal/terminal checks and user's Steam
  retest remain pending; no live acceptance claimed.
- P0 | KDE rapid input drops and stalled geometry | Delivered 2026-10-19:
  production tiled directional FIFO with drain-time subject binding, exact
  convergence-based clamp budgets, and vacated-source R4 return reconciliation.
  KWin 1903/1903; Rust workspace 3289 passed; native callback-driven 8 same-output
  moves, 16 occupied-target crossings and rapid move/focus pairs pass. Input
  ordering is **Tentative (user review pending)**. Physical-key/perceived-latency
  acceptance remains; original short-height cause lacked frames, while current
  full-height arrivals pass. [Outcome](changes/archive/kde-rapid-input-geometry.md).
- P0 (after floating dialog defect) | KDE aligned per-output workspaces |
  Implemented 2026-10-19: full aligned grid, per-output trailing, global
  hole/removal guards, minimum two including trailing, startup adoption,
  KDE `global-unique` removal/fallback and trailing-landing logical migration.
  Rust **3335**, KWin **1919**, strict gates pass. Native hidden-member
  reachability, independent trailing, holes/removal, migration out/back with
  settlement and native output-move adoption pass at callback/state level.
  D5 logical source-refill interpretation and old-mode fallback are
  **Tentative (user review pending)**. Disjoint-model T05 is superseded.
  Historical note: the `global-unique` removal/fallback and old-mode fallback
  above are v1 aligned-grid acceptance (2026-10-19). Under the closed v2
  contract the KDE v2 port delivers both separate-view modes subject to the
  KWin setting and required warning, not global-unique removal
  ([closing outcome](changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24)).
  - NEXT hotplug: user 2026-10-19 selected the physical route: move affected
    Plasma surfaces and windows off the output before disabling it, keep the
    protected terminal on the surviving output, and restore the exact output
    configuration and surface placement afterwards (no virtual outputs). Probe
    native disconnect event order first, then implement retained origin
    logicals before evacuation/pruning, trailing displacement and return on
    reconnect. Aligned no-merge/return is not delivered yet.
  - Remaining: physical drag/shortcut evidence. The earlier own-send anomaly
    did not reproduce under complete wire/native capture
    ([dual-output record](changes/kde-dual-output-live.md)).
  - Follow-ups: Windows is complete under v2; macOS follows its v2 port item;
    independent-set defaults remain unchanged.
  [Record](changes/kde-aligned-output-workspaces.md).
- P1 (after floating dialog defect) | KDE open-animation tile jump | New
  windows play the open effect at initial placement, then jump into the tile.
  Source investigation 2026-10-16: no animation wait; 120 ms admission
  debounce plus planner IPC, and Wayland resize needs a client round trip
  after map. Tiling itself must not animate; goal is final geometry before
  opening. Timing-probe draft removed 2026-10-18: it instrumented only legacy
  `DescribePlan`, not the production `DescribeWorld` route; no native timing
  evidence was collected. Next: implement a trace-only world-route probe
  (admission, planner send/reply, requested and signal-observed geometry),
  then user-run single-window timing capture with a screen recording.
- P1 | Shared settings schema and tabbed Settings | Delivered: core-owned
  settings catalog (tabs, fields, types, defaults, validation) that each
  platform renders natively and persists, so KDE, Windows and macOS do not
  maintain separate dialogs.
  [Options and recommendation](research/shared-settings-schema.md) delivered
  2026-10-15. [KDE-only tabs delivered](changes/archive/kde-settings-tabs.md):
  Tiling & Workspaces, Appearance, Shortcuts, Desktop Integration; settings
  build/CTest and staged native rendering/switching pass. Windows grouping
  and pre-0.1 route/timing are user-approved 2026-10-16. The Windows
  Settings dialog has grown too tall to use; priority raised. Approved route
  D (Rust core catalog with checked-in generated native descriptors) pre-0.1:
  first slice builds the core catalog with tabs/sections and platform-specific
  fields, messages and native panel slots, and Windows renders a tabbed dialog
  from it using the KDE tab groups; KDE migration to the catalog follows as a
  KDE-stream item. [Windows first slice](changes/shared-settings-catalog-windows-tabs.md)
  implemented: 17 portable fields, four Windows-only fields, four native tabs,
  persistent feedback and scrollable long notes; physical size/keyboard/
  accessibility and save/reload checks remain pending.
  User-approved placement: Tiling & Workspaces (gaps, workspace defaults/scope,
  move/admission/refill); Appearance (border/underlay); Shortcuts (takeover,
  Win+L, presets, bindings, Xbox/Game Bar); Desktop Integration (mouse Snap,
  taskbar pager). CI 38876735949 passed all eight jobs.
  Agent-live 2026-10-17: four-tab draft/save/reload, Ctrl+Tab/reverse,
  Revert and Escape no-save passed; original settings bytes restored.
  Full scale/accessibility/traversal and physical feel remain unverified;
  [dated evidence](changes/shared-settings-catalog-windows-tabs.md#agent-live-follow-up-2026-10-17).
- P1 | KDE Settings native re-check | Core-catalog migration delivered offline
  2026-10-17: exporter, checked-in descriptors and CI freshness gate; existing
  labels/placement, KConfig keys/fallback, both identities, settings-only
  packaging and native operation isolation retained. Rust/KWin suites and
  hermetic native settings/effect tests pass. Native targeted-accessibility
  checks passed both identities 2026-10-18: all tabs/Status, gap draft/save/reopen,
  Cancel, Defaults without save, both post-switch draft read-backs and unchanged
  ordinary-Apply endpoints
  ([partial evidence](changes/kde-live-acceptance.md#settings-targeted-native-interaction-2026-10-18)).
  Physical traversal/buddies/visual UX and transient-operation non-invocation
  remain pending;
  [original steps](changes/archive/kde-settings-core-catalog.md).
- P0 | Core update replies carry workspace state | Core and KDE delivered
  offline 2026-10-17 under selected D1-D5: complete world reports/replies,
  generation/revision/input-watermark fences, Planner-owned workspace policy,
  retained transfer/view/property targets, explicit holds and a bounded 4 MiB
  transport. [Contract, measurements and live checks](changes/archive/core-full-state-replies.md).
  User-owned live KDE acceptance remains pending (raised to P0 2026-10-17:
  two live regressions, rejected actions and lost outer gap, were fixed in
  `3c94a819`, `9eaea71a`, `9fe0e79b`; the outer gap fix was confirmed live).
  The v1 KDE evidence above is historical; KDE-side acceptance now belongs to
  the KDE v2 port item.
  Bounded native callback/state passes 2026-10-18 cover sampled ordinary/safety/
  restart paths, request-size transport limits, selection/history, floating-boundary
  numbered/relative follow/stay and one tiled numbered send-follow. No complete
  live inventory gate: D7 precondition and tiled return remain blocked; physical
  input, drag/visual UX and remaining permutations are pending
  ([coverage matrix](changes/kde-live-acceptance.md#remaining-coverage-and-next-step)).
  Windows/macOS adoption is
  tracked separately below. The temporary geometric-capacity hold is cleared
  by best-effort admission below; protected/invalid geometry still holds.
- P1 | Best-effort admission live re-check | Core/KDE delivered offline
  2026-10-18 under selected D1-D10; the full-state geometric-capacity hold is
  cleared. Largest-splittable/float defaults, atomic mixed seed, ordinary
  persisted capacity floats, newest-first protected repair and positive-overflow
  alternative are wired through shared catalog settings and KDE. Rust workspace
  **3092 passed**, KWin **1824 passed**, native effect/settings **34/27 passed**;
  exporter/API/shell checks and all eight CI jobs pass (`b7da8fb9`, run
  39076930351). No live acceptance claimed.
  [Delivery and exact script-restart re-check](changes/archive/best-effort-admission.md#residual-risks-and-exact-live-re-check).
  Windows/macOS consumption remains separate below. B6 declared-minimum overlap
  is unchanged; review its UX during dogfooding.
- P1 | macOS: adopt best-effort admission | Consume the shared D1-D10 policy
  with full-world adoption below; retain/echo ordinary float classification
  through AX observations and restart persistence, no capacity-specific behavior.
  Add native settings only through its eventual catalog consumer. Verify mixed
  startup rosters, focus-on-admission subject to native safety, explicit retry,
  protected newest-first shrink repair and overlap navigation. Preserve AX/thread
  ownership and native-Space capability refusals; no automatic Space relocation
  or unsupported transfer/remap inferred from shared core. Native acceptance
  remains separately required.
- P1 | KDE maximize/fullscreen asynchronous-settlement false refusals | Native
  callbacks logged refusal/shortcut failure before the requested state settled
  successfully. Replace same-stack final-outcome inference: confirm once at a
  fresh accepted observation boundary (recommended), or distinguish issuance
  from final outcome. Preserve lifetime/scope/focus/generation/supersession
  fences, one setter attempt, no retry/replay and truthful genuine refusal.
  Add producer-shaped delayed-settlement, silent-refusal and intervening-command/
  restart regressions with renewed wrappers, QML array-likes and native properties.
  No correction delivered; [coverage matrix](changes/kde-live-acceptance.md#remaining-coverage-and-next-step).
- P1 | KDE D7 controller-restart precondition: exact-site reproduction |
  A stable fixed-client explicit-tile request refused `partial-observation`
  before controller replacement. Fail-closed behavior is expected; product vs
  fixture trigger is unresolved because operation payload/refusal site is absent.
  Reproduce offline immediately after workspace enable, with/without intervening
  observes; retain complete reports/replies, known-set bindings and exact refusal
  site. No delay-only workaround or controller-restart pass inferred.
  [Coverage matrix](changes/kde-live-acceptance.md#remaining-coverage-and-next-step).
- P0 | KDE restart float-intent loss on held first observation | Delivered:
  held-first preservation and fresh script-stream handover now pass the complete-
  baseline two-output restart; both intent sets survive and explicit retiling
  retains the fixed override. KWin 1962/Rust 3353; native-ID/default-tiling churn
  only. Historical silence traced to a helper-ID collision; no lifecycle policy
  selected. [Accepted outcome and limits](changes/archive/kde-restart-float-intent.md).
- P1 | Windows: adopt full world-state core replies | Superseded as an
  implementation route by the [core world-model rearchitecture](changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24):
  stages 2a/2b accepted 2026-10-22 (logical workspace authority, sole Windows
  world owner), closed 2026-10-24; W2a gated diagnostics were evidence only and are
  superseded by the delivered mapping authority and T2 refusal below. Windows is dogfood-ready at
  the bounded acceptance level. Remaining native gaps per the close-out:
  MRU occupied-source-refill staging, global-unique/shared mode checks,
  tile-override restart staging and user-owned physical shortcut/drag/game
  acceptance. The legacy domain API stays compiling, not behaviorally migrated.
  Constructor prerequisite delivered `086f9d8c`. [Slices and evidence](changes/archive/windows-full-world-state-replies.md).
- P1 | Core + Windows: world migration authority and T2 crossing refusal |
  Assessed 2026-10-20 ([record](changes/archive/windows-full-world-state-replies.md)):
  (1) the proposed default-off migration-only authority capability was NOT
  approved in that form: whole-workspace migration is a [core-owned mapping
  change](decisions.md#core-owned-world-model) actuated by the adapter. The
  rearchitecture (closed 2026-10-24) delivered that mapping
  authority with observed member/view/refill settlement; KDE's `world-apply.ts`
  bypass must be aligned with the contract by the KDE port
  ([guide](porting/world-adapter-guide.md)). The parked KDE
  global-native-inventory migration-confirmation mismatch stays parked.
  (2) T2 crossing refusal: delivered in the v2 world route - staged-clone refusal
  preserves unchanged source Engine and membership on unsupported directional
  output-send/R4 crossings, fixture-accepted with the core close-out
  ([closeout](changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24)).
  Explicit-target sends keep the parked `cross-domain-mismatch` contract gap.
  Physical two-output refusal validation remains unrun. Each fix lands as one
  change across shared core and the Windows adapter.
  **Caveat (user 2026-10-19): be extremely careful to avoid platform-specific
  logic in core.** For each blocker, first assess whether a small, simple,
  sensible alternative without core changes exists (adapter-side or
  platform-neutral). Only if none exists, design the core change to add
  minimal conditional logic or burden, expressed as a platform-neutral
  capability rather than a host check; precedent is the KDE opt-in for
  output-shared workspaces. Any core change needs explicit user approval
  before implementation; bring the options, trade-offs and recommendation to
  the user.
- Windows physical/user-run acceptance is consolidated by setup in the
  [Windows checklist](changes/windows-physical-checklist.md), including the
  two-monitor migration/T2 checks paired with the preceding item. Each row
  carries its approved gate IDs, pass criterion and source record.
- P1 | Shared: tolerate host-settled geometry | User 2026-10-19: hosts often
  settle a window at a frame slightly different from the requested one (DPI
  virtualization rounding, character-cell size increments in terminals,
  minimum/maximum hints, self-resizing clients). Find one general,
  platform-neutral solution so the tiler accepts small discrepancies without a
  back-and-forth fight with the host, not a per-platform exception.
  Research/design first: (1) inventory current core/KDE/Windows/macOS behavior
  when the observed frame differs from the request (re-issued writes, existing
  tolerances, readback mismatch logging); (2) check how Tier 1 input WMs
  (sway, i3, Hyprland, COSMIC; local pinned sources) treat increments, hints
  and host rounding for tiled windows; (3) propose a rule, draft: the
  host-settled frame is accepted for that allocation and the same window is
  not re-written for the same allocation; small discrepancies leave
  neighbors alone; larger ones (increments, minimums) reflow neighbors at most
  once; logs report host-settled outcomes truthfully. Weigh against the parked
  [learned size limits](changes/learned-size-limits.md) candidate (rejected
  for complexity). Any core change follows
  [Layering](principles.md#layering) and needs user approval before
  implementation. Evidence cases: Windows DPI-unaware one-pixel offsets at
  125% scale (`9fa32420`,
  [record](changes/windows-best-effort-admission-live.md)), terminal
  increments, decision on interactive-resize noise (rounding, size
  increments, self-resizing clients).
- P1 | Shared core: own output topology | Delivered in core and Windows under
  the closed v2 contract ([closing outcome](changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24)):
  FULL world topology retained in all modes, per-use targets core-derived.
  KDE/macOS must report FULL bounds with truthful native receipts per the v2
  port guides (both unported; their v1 directional-intent/FULL-bounds work is
  historical evidence). Remaining native acceptance: split-edge and ordinary
  two-output legs.
  User 2026-10-18 order: topology first, then D23 rehome/13f maximize base.
  User direction 2026-10-18: retain FULL monitor rectangles from world
  reports and derive adjacency/targets in core, not adapters. Strict-parity
  extraction remains tentative (user accepted the tentative choices 2026-10-18).
  Split-edge alignment is already decided (User 2026-10-09,
  [selection record](changes/archive/position-based-output-selection.md),
  Tier 1 basis R-MOV-12/R-WS-27 in
  [consensus](research/reference-wm-consensus.md)): exhausted moves and
  explicit sends use window-centre projection, then window-span overlap, then
  left/top; whole-workspace migration uses largest shared edge, then left/top;
  focus keeps unique-reciprocal horizontal. World move/send/migration alignment
  delivered offline 2026-10-18 with R-MOV-12/R-WS-27 expected results and real
  Engine KDE world-entry fixtures; retained R4 arrival is identity-pinned.
  [Alignment evidence](changes/archive/world-split-edge-selection.md).
  NEXT: remaining native split-edge and ordinary two-output acceptance.
  Migration settlement is core-owned under v2 (delivered, natively accepted
  both directions); disjoint-model confirmation work is superseded.
  [Delivery and live procedure](changes/archive/shared-core-owned-output-topology.md), alongside the
  [D23/13f proposal](changes/shared-core-d23-rehome-and-13f-maximize-base.md).
- P1 | macOS: report FULL display bounds for core-owned topology | Delivered
  2026-10-18: world topology uses retained FULL `NativeDomain.bounds`; domain
  bounds remain visible/menu-Dock work areas and adjacency stays empty. Display
  identity/UUID homing, negative/above-primary origins and native Space refusals
  are preserved. Producer-shaped FULL/work-area, shared-edge rounding, shuffled
  Space order and missing-primary fixtures pass; macOS tests, strict clippy/fmt,
  signed app build and verification pass. Scoped single-display TextEdit smoke
  completed with clean shutdown and stable frames consistent with visible-area
  tiling. Native
  two-display arrangement validation remains pending: attach a second display,
  capture FULL/visible/report geometry, tile disposable windows with differing
  work-area insets, repeat left/above-primary arrangements and supported edge
  actions, then restore the baseline. Rect-bearing diagnostic capture is still
  needed for the live FULL/report comparison; do not infer arrangement from
  Space order. [Evidence and exact steps](changes/archive/macos-full-display-topology.md).
  Note: v1 delivery; the macOS v2 port below must supply FULL bounds with
  truthful native receipts and transport per the v2 guide.
- P1 | Windows Xbox shortcut conflicts (Win+G, Win+F11) | User 2026-10-16
  approved choices implemented offline: Compatible float `Ctrl+Win+G`,
  fullscreen `Ctrl+Win+F11`, exact Ctrl-arm routing and hold/release coverage;
  Authentic keeps Win+G/F11 with cached read-only per-user `GamingHomeApp`
  configuration and supported current-user Game Bar package-query evidence.
  User decision 2026-10-17 (supersedes the 2026-10-16 conservative-warning
  decisions for BOTH Game Bar and GamingHomeApp): warnings only on provably
  present conflict. Game Bar query positive installed only;
  failure/unknown same as absence, no notes/conflict rows/badges.
  GamingHomeApp per-user config positively present/configured only;
  unknown/absent/unreadable no amber/tray/Settings warning. Honest
  normal-level probe outcome logs retained. Part B implemented offline
  2026-10-17: all warning surfaces (tray, badge, selected detail/actions,
  static list/OS text, preview) obey the provably-present rule; unknown,
  absent, and failed probes show ordinary no-known-conflict text. Locked
  native gates: 2,500 passed, 0 failed, 1 ignored; fmt, strict clippy,
  diff-check pass. Historical note: before Part B the shipped behavior still
  warned conservatively on unknown; that interim is superseded.
  Correction B delivered offline:
  detected Game Bar installation warns; successful package
  absence clears all Win+G warnings, notes and conflict rows, including static
  binding-list and selected OS text (not just probe-derived details/badges).
  Presence/absence/failure, disabled-row and effective-rebound-chord regressions
  pass; locked native gates: 2,286 passed, 0 failed, 1 ignored.
  Resolve buttons
  open Settings only, explaining capture/managed-app-data loss and Store
  reinstall limits; no tiler uninstall or registry/policy writes.
  [Active record and physical plan](changes/windows-gaming-coexistence.md),
  [research limits](research/windows-port/xbox-shortcut-conflicts.md).
  Pending physical acceptance: [keyboard/shortcuts checklist](changes/windows-physical-checklist.md#single-monitor-keyboardshortcuts)
  and [games checklist](changes/windows-physical-checklist.md#games).
- P1 | Windows launcher geometry fighting | Hint/timeout flap correlates
  with repeated four-window writes. User-approved lifetime-bound declared-hint
  reuse on timeout (no expiry), rate-limited per-window diagnostics and
  log-derived synthetic regression delivered offline. Four-launcher plus
  Notepad/Paint physical acceptance pending
  ([record](changes/windows-launcher-geometry-fighting.md)).
  Agent-live 2026-10-17: exact launcher journey skipped (complete approved app
  set unavailable); ordinary Paint timeout reused declared hints with unchanged
  targets, supplementary only. Full launcher acceptance remains pending.
  Review 2026-10-18: full ordinary-app journey remains user-run under the live
  guide; disposable helpers cannot substitute for launcher convergence or genuine
  Paint hint changes. No new launcher or physical-resize acceptance.
- P3 | Windows visibility diagnostics | Delivered offline: bounded normal-level
  per-window hide/reveal outcomes and reasons, native visibility/cloak samples,
  exception lanes/refusal causes and monotonic timing; no behavior change or
  Xbox recovery claim. Locked native gates 2,313 passed; agent-live 2026-10-17
  normal-level switches pass (no malformed rows; overflow/exception stress not
  claimed). ([record](changes/archive/windows-visibility-diagnostics.md),
  [incident diagnosis](changes/windows-gaming-coexistence.md)).
- PRE-0.1 | KDE multi-output test environment | Narrowed slice promoted
  2026-10-13: provide a user-operated two-output environment for the must-pass
  gate. [Research and coverage](research/two-output-test-environment.md)
  delivered. User 2026-10-13 declined the VM route: multi-output development
  and testing run natively on a user-owned PC with a physical second monitor
  ([decision](decisions.md#scope-and-platform-goals)). No VM setup or new
  dependency. Check 2 additionally needs a third output (user to arrange).
  General live-test reference environments remain parked.

- P0 | 0.1 release (KDE Plasma, Windows, macOS) | User 2026-10-17: 0.1
  covers all three platforms, aiming for feature parity with KDE as far as
  is sensible per host ([scope](decisions.md#scope-and-platform-goals)).
  User 2026-10-18: the KDE categories below and approved 57-check triage
  classification are the shared 0.1 gate template. Windows and macOS each propose
  applicable checks, documented host-difference exceptions and platform-specific
  extras for user approval ([decision](decisions.md#scope-and-platform-goals)).
  Originally the KDE-only MVP (user 2026-10-10). Must-pass gate: VISION
  reliability (sleep/wake, output plug/unplug, resolution/scaling changes,
  fullscreen applications, underlying configuration changes), gaming/fullscreen
  non-interference, core journeys (tiling, focus/move, workspaces, multi-output,
  border, shortcuts, settings, restart), and missing/failed/removed companion
  leaves tiling working (and KWin stable on KDE). Other live checks may ship as
  listed known issues provided none crashes or silently stops tiling; no separate
  dogfood period. Includes these pending pieces:
  - Windows gate D1-D4 approved exactly as written (User 2026-10-19):
    [approved gate and pending evidence](changes/windows-0.1-gate-proposal.md).
    Base 45 must-pass / 7 known-issue-allowed / 5 excluded plus WX1-WX8
    must-pass; only the Authentic Win+G/Win+F11 exact-chord containment gap
    is a listed limitation. Native/physical acceptance against the shipping
    runtime route and delivery dependencies remain pending; gate definition
    approval is not release acceptance.
    Pending journeys are grouped in the [Windows physical/user-run checklist](changes/windows-physical-checklist.md).
  - [OPEN-requirement triage and live-check gate](research/release-0.1-triage.md):
    User 2026-10-10 approved recommendation (a) for 15 units: D02, D04, D05,
    D06, D07, D08, D11, D14, D15, D17, D18 (no setting), D19, D20 (no lower
    verb for now), D21, D24; 16 rows approved. User 2026-10-12 also approved
    D01, D03, D09, D10, D12, D13, D16, D22, D23, D25, D26, D27, D28 (seventeen rows).
    All 28 units and Proposal B / N1-N4 classification approved; triage review complete.
    Spec now 138 NORMATIVE / 0 PROVISIONAL / 28 OPEN; verification and
    implementation gaps are below.
  - Proposal B classification approved exactly User 2026-10-12: original 53
    checks = 39 must-pass / 12 known-issue-allowed / 2 Windows-release; N1-N4
    add four must-pass checks, total 57 = 43 / 12 / 2. K checks: 5, 24, 26,
    29, 35, 36, 37, 38, 40, 41, 52, 53; W checks: 3, 4; all others M.
    D28 KDE fullscreen-send acceptance including a real game is explicitly
    must-pass (User 2026-10-12), a sub-leg of the reference-comparison check;
    counted once within original check 1. Multi-output must-pass checks require
    a two-output setup (VM or hardware) and must pass before 0.1 can ship.
    Implementation and native/live acceptance remain pending.
  - Original 60 OPEN rows triaged: 32 relevant / 28 post-0.1. P2 settings now
    contain approved D09/D10/D27; D03/D12/D13/D16/D22/D23/D25/D28
    have no setting. [User decisions](research/release-0.1-triage.md#user-decisions-2026-10-12).
  - Release gates owned by their own items: 0.1 packaging (mainstream install
    path), External NixOS/Home Manager validation, Ubuntu 26.04/26.10
    acceptance and Licensing (all below); OBS handoff scripting is part of packaging.
  - Workspace tiling toggle Meta+Y (KDE) delivered offline 2026-10-11
    ([record](changes/archive/kde-workspace-tiling-shortcut.md)); Win+Y (Windows
    item 21) delivered offline 2026-10-13
    ([record](changes/archive/windows-workspace-tiling-shortcut.md)).
    Native/physical and preset acceptance remains in Pending live checks.
  KDE current approach retained; no new
  non-native-research items. Lessons persist linked
  ([candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [cross-cutting findings](research/non-native-tiler-host-interaction.md#cross-cutting-findings),
  [evidence](changes/archive/non-native-tiler-host-interaction.md)).
- P0 | macOS: propose 0.1 gate exceptions against the shared template |
  Apply the [shared gate decision](decisions.md#scope-and-platform-goals) for user approval.
- P1 | Ubuntu 26.04/26.10 acceptance | KWin 6.6 floor selected by user
  2026-10-11 and delivered offline; release 6.6.4, updates 6.6.6 and 26.10
  6.7.5 two-package builds and full apt install/upgrade/removal solver acceptance
  passed ([solver evidence](changes/archive/ubuntu-apt-solver-acceptance.md)).
  User 2026-10-13 selected (b): exact effect pin retained; installation-guide
  note and Debian effect-package description explain the rebuild wait
  ([delivery](changes/archive/ubuntu-kwin-upgrade-note.md)). Remaining:
  user-owned shared-workspace/Settings/native-visual journeys on 6.6
  and per-output regression on 6.7+, OBS target provisioning/source handoff.
  Plasma 5 and KWin <= 6.5 remain unsupported.
  [Decision](decisions.md#kde-support-floor),
  [evidence and live checks](changes/archive/kwin-6.6-support.md).
- P1 | Licensing | Decision and declarations applied 2026-10-11: MIT default,
  GPL-2.0-or-later native effect, MIT Rust staticlib and Settings KCMs.
  [Research and dependency inventory](research/licensing.md). Third-party
  notice implementation delivered 2026-10-13: committed generated union,
  manual `just` regeneration, CI freshness lint, source archive and
  Debian/RPM/Arch/Nix installation. Pending: actual distro package
  builds/extraction; hosted CI execution;
  notice delivery in future GitHub binary archives (producer outside this
  change). [Evidence](changes/archive/third-party-notices.md). Audit gate delivered
  2026-10-12: keep `licenses`, `bans`, `sources`; per-distro closure and vendored
  inventory recorded; [evidence](changes/archive/licensing-audit-gate.md).
- P2 | OBS release handoff redesign | Route selected by user 2026-10-18;
  no longer an open release decision or separate blocker. Route 3 scripting
  belongs to 0.1 packaging below, deferred until close to 0.1. Automation
  (Route 1 bot account or SCM bridge) is post-0.1.
  [Decision and route](research/distribution-package-feasibility/obs-release-handoff.md#selected-route-3-consequences).
- P2 | [Push code down](principles.md#layering) | Recurring post-0.1 passes: move logic from host-coupled
  `kwin/src` TypeScript and `kwin/native-effect` C++ (including settings and
  shortcut reconciliation) into system-specific Rust, then into the agnostic
  shared core where appropriate. First pass after 0.1, aligned with or ahead
  of deferred K2/K3. Born-maximized core policy is delivered offline;
  user 2026-10-13 option C defers KDE rewiring to this pass. KDE retains its
  conformance-pinned TypeScript copy because clear/keep runs synchronously
  before dispatch and the planner protocol lacks the decision. Runtime
  protocol changes and build-time generation were declined
  ([record](changes/archive/born-maximized-shared-policy.md)).
  User 2026-10-14 also defers D06 focused-output newcomer selection from the
  KDE adapter into Rust core/Planner to this post-0.1 pass, alongside
  born-maximized; adapter routing remains selected for 0.1
  ([record](changes/kde-dual-output-live.md#focused-output-admission-correction-and-breadth-repeat-2026-10-14)).
  Link with KDE interaction performance/threading and
  [Live sibling reflow while dragging](#open-work).
  User 2026-10-17 accepted the staged KWin-to-Rust path as this item's
  direction ([decision](decisions.md#engine-architecture-and-convergence),
  [research](research/kwin-rust-migration.md)); refresh its inventory first.
- P2 | KDE interaction performance and threading | New post-0.1 research,
  not a current priority (user 2026-10-13; no significant KDE jank). Windows
  input/render threading drastically improved UX. Explore dynamic resizing
  during drag and drag/move/resize work in multithreaded Rust, C++/TypeScript
  thin shims, relying on Rust thread safety; include later platforms. First
  research KWin effect/script threading constraints; interaction with KWin may
  change. Linked with Push code down and Live sibling reflow while dragging.
- P2 | Smooth KWin upgrades | Future research (user 2026-10-13): patch-release
  effect loading compatibility (IID/version binding), OBS rebuild latency and
  pin options. Aim for the smoothest possible user upgrades; exact pin remains
  selected. [Decision](decisions.md#kde-support-floor).
- P2 | Post-0.1 research: hidden-workspace presentation configurability |
  User 2026-10-17 (`SW_HIDE` kept for 0.1): investigate trivial
  configurability (e.g. off-screen parking for shell inclusion);
  switch-versus-pull remains open in post-0.1 research.
  User 2026-10-19: wants hidden windows kept in taskbar/Alt+Tab, with
  selection switching to the window's workspace and focusing it, and is
  exploring configurable `SW_HIDE`/cloak. Next after the 2026-10-19 dogfood
  item: bounded private ApplicationView cloak spike on one disposable owned
  helper, user-run per the live guide (cloak/uncloak, taskbar and Alt+Tab
  listing, selection, stop/crash restoration on the current build). Report
  feasibility and a scoped design before any product change or 0.1 change.
  Tooling and exact user-run steps: [active spike](changes/windows-cloak-spike.md).
  [Research](research/windows-port/alt-tab-hidden-workspaces.md),
  [decision](decisions.md#workspaces).
- P1 | 0.1 triage decisions - batch 1 verification (KDE/shared) | User
  2026-10-10 approved the 15 units above, exactly recommendation (a) in
  [triage](research/release-0.1-triage.md#user-decisions-2026-10-10).
  - D05 / REQ-INS-05: verification-only (2026-10-11). The existing
    shared Engine convergence route retains prior tiled focus B while an
    ordinary float is focused; newcomer admission splits B's long edge rather
    than root-wrapping. Shared geometry/focus and KDE PlanAdapter wire fixtures
    prove this offline under unchanged admission policy; native observation/
    activation acceptance remains user-owned in Pending live checks.
    [Evidence](changes/archive/triage-batch1-kde-shared.md).
  - Observe-first, contingent implementation: D06 / REQ-OUT-05 currently
    admits on the newcomer's observed native output, with no focused-output
    override; verify focused output + newcomer activation. D17 / REQ-WS-09
    retains shared `last_active`, but KDE return focus is shell-driven; verify
    remembered B on return. Implement only if these native outcomes mismatch.
  - Verification-only: D02, D04, D07, D08, D11, D14 (both rows), D15, D18,
    D19, D20, D21, D24. Existing mechanisms/defaults match; exact native
    journeys remain pending in Pending live checks. D07's shared close/refocus/
    fresh-reopen fixture now proves fresh admission offline with no old-slot
    restoration; exact native ratio/order/focus evidence remains pending.
    D18 has no setting; D20 needs raise-order proof, not a lower action.
  - 2026-10-18 single-output attempt: blocked before any D-unit journey; no
    acceptance advanced. Owned-desktop selection refused, later selection
    summaries lacked canonical wire evidence, and an empty backing desktop
    retired. Original desktop identity/name was not restored; runtime float-intent
    preimage was missed by the helper. Preserve both pending disposition, repair
    capture, then discriminate fixture mapping from adoption/view application
    before any product fix. Options and producer-shaped outline:
    [native triage record](changes/kde-triage-live.md#blockers-and-next-discriminator).
  - Windows: D05 shares the proven Engine route; D14's keyboard trigger gap is already
    handoff item 6 (include R-RSZ-02/03 acceptance there). D02's owned-dialog
    exclusion is the approved divergence; D17 remembered focus and D19 hide/
    reveal are already wired. No new Windows-specific handoff item warranted.
- P1 | 0.1 triage decisions - implementation and verification (KDE/shared) |
  D01, D03, D09, D10, D12, D13, D16, D22, D23, D25, D26, D27, D28 approved by
  User 2026-10-12. Historical gap assessment at `fafcd31`, D01 refined by
  [research](research/d01-fullscreen-admission.md) at `01be284`. D01 and D28
  are delivered offline ([record](changes/archive/kde-fullscreen-activation-and-send.md));
  native acceptance remains pending. [Selections and caveats](research/release-0.1-triage.md#user-decisions-2026-10-12).
  Verification-only means a matching mechanism exists, not native acceptance.
  Approved units include contingent fixes if live outcomes differ, subject to
  their recorded return-to-user conditions.
  Paths below are repository-relative; line locators are revision-bound.

  | Unit | Gap assessment | One-line reason / source |
  | --- | --- | --- |
  | D01 | delivered offline; native verification pending | KDE automatic convergence re-observes before focus and fences overlay/native-focus races; admission retains slots/state and explicit switching/shortcuts remain effective. Shared fixtures verify host-approved/denied admission and exact CLOSE-05 removal/refill/focus. Joint native admission/overlay-close cleanup pending ([record](changes/archive/kde-fullscreen-activation-and-send.md)). |
  | D03 | verification-only | After-focused `[old,new]` and per-admission focus anchor exist (`crates/tiler-core/src/session.rs:2218-2231`, `crates/tiler-core/src/session/ops/lifecycle.rs:374-384`); chained legs 2-3 need live pointer/focus proof. |
  | D09 | verification-only | R-MOV-06 two-child neighbor uses middle index `len/2` (`crates/tiler-core/src/directional.rs:907-918`, `crates/tiler-core/src/session/ops/move.rs:897-905`); exact native fixture pending. |
  | D10 | verification-only | Orthogonal escape continues through R1 wrap (`crates/tiler-core/src/directional.rs:1003-1007`, `crates/tiler-core/src/session/ops/move.rs:1089-1140`). |
  | D12 | verification-only | Same-axis escape inserts into the ancestor axis (`crates/tiler-core/src/directional.rs:993-1002`, `crates/tiler-core/src/session/ops/move.rs:1080-1087`). |
  | D13 | verification-only | Local move wins before output crossing (`crates/tiler-core/src/directional.rs:1046-1088`); OUT-02 local-wrap multi-output fixture remains live-pending. |
  | D16 | delivered offline on KDE and Windows; native verification pending | Ordinary and floating+fullscreen/maximized sends use slotless MoveDeferred; float frame/intent retained, no geometry/overlay writes or source/destination tile reflow. Windows adapter delivered 2026-10-13 with drifted-survivor application suppression and real entry/Engine regressions; native combo carry/exit remains pending. Sticky/automatic-fixed refusals unchanged; no setting. [Shared/KDE record](changes/archive/d16-floating-transfer.md), [Windows record](changes/archive/windows-d16-floating-transfer.md), [decision](decisions.md#workspaces). |
  | D22 | approved; verification-only | Float-out preserves survivor proportions (50/25/25, float middle 25 -> approximately 67/33); no setting. Removal retains survivor share ratios (`crates/tiler-core/src/cosmic_v1.rs:175-198`, `crates/tiler-core/src/session.rs:2398-2399` at `fafcd31`, revision-bound). COSMIC observed, not source-proven, plus i3/sway Tier 1; thin i3/sway one-family basis disclosed; no evidenced alternative, Hyprland/Tier 2 untraced. Native reflow live check pending. [Decision](decisions.md#window-state-float-sticky-maximize-fullscreen). |
  | D23 | delivered offline on KDE and Windows; native verification pending | COSMIC `3d55cba0` source gate passed. Shared best-effort neighbor rejoin and additive `closed_windows` serve both adapters. Windows delivered 2026-10-13: retained-allocation divergence replaced with release/reflow/MRU, no-focus restore, sole null focus and hidden-domain close cleanup; session-only. Native gates 1,845 passed, 0 failed, 1 ignored; native journeys pending. [Shared/KDE evidence](changes/archive/d23-rework.md), [Windows evidence](changes/archive/windows-d23-restore-close.md), [decision](decisions.md#window-state-float-sticky-maximize-fullscreen). |
  | D25 | approved (a); verification-only | Keep host-owned set; re-observe host membership, no moves; focus only where provable; rebuild layout by re-adoption, no saved-layout restore; intentional floats stay floating via existing restart store; no setting. Generic fresh snapshot/rebuild/converge supports membership/provable focus (`kwin/src/plan-adapter-entry.ts:2186-2300`, `crates/tiler-core/src/session/world.rs:746-799` at `fafcd31`, revision-bound). Native dogfood/live checks will assess orderly restart; outcomes pending. [Decision](decisions.md#restart-persistence). |
  | D26 | approved (b); verification-only, no code | Restore already-persisted owned gaps/settings; re-observe host set/membership/maximize, no invented layout restore. Gaps/settings reload exists (`kwin/src/domain-gap.ts:90` at `fafcd31`); intent cannot cross login/bus/KWin namespace (`crates/omnitiler/src/float_intent_store.rs:11,30-34`). Listed 0.1 known limitation: intentional-float memory lost on logout/login, reopened apps new identities; unchanged REQ-RST-01d lifetime. Revisit after 0.1 with shared restart store. Native dogfood/live checks will assess; outcomes pending. [Decision](decisions.md#restart-persistence). |
  | D27 | KDE verification-only; no longer BLOCKED. Windows partial verification; approved 5b implementation pending via P2 Activation request setting | User 2026-10-13 accepts native KWin host exception: valid-token/FromTool/allowed-X11 switches + focuses, missing/stale-token demand-attention; converge without intercept/revert/suppress. No KDE gaming guard; rely on native input freshness. Real-game active and idle/paused check must pass; reopen only on actual active-play switch-away. Windows 2026-10-14 `f4b9f41`: fresh background current/hidden calls denied with no switch/focus change; shell flash observed, visible urgency unproven. Previously attached caller converges; no product classifier/marker. User-approved 2026-10-16 Windows 5b: OS-permitted Switch and focus over fullscreen/games, no urgency-only downgrade; default Mark urgent only/non-permitted unchanged. Not yet implemented; User 2026-10-17 folds the standalone 5b implementation into the planned post-0.1 `Activation request` setting item (P2 0.1 triage settings follow-up). Clean permitted route, mark/clear and game evidence TBD ([Windows evidence/options](changes/archive/windows-d27-activation-verification.md)). Setting KDE applicability deferred to design ([research](research/d27-kwin-activation-trace.md); [decision](decisions.md#activation-requests)). |
  | D28 | delivered offline, including born-fullscreen option (a); native/game verification pending | Explicit numbered/relative follow/stay carries retained fullscreen tiles and never-tiled slotless members. Shared MoveDeferred rehomes the initial hold without tree mutation, source admission or geometry setters; D5 first exit admits on the destination. Fullscreen flag and existing send fences retained. Native KDE journey including a real game is must-pass; stop if KWin drops/corrupts fullscreen. No setting; native outcome TBD ([record](changes/archive/kde-fullscreen-activation-and-send.md); [decision](decisions.md#window-state-float-sticky-maximize-fullscreen)). |

  - Native verification queue (all units): no live acceptance claimed. Pending
    native journeys: R-WS-05 roundtrip, numbered/relative follow/stay,
    sole-window/empty-target, float/overlay flags and same-output frame (D16);
    float-out reflow shares (D22); best-effort rejoin journeys, internal slot
    retirement and sole-iconic retention (D23; two agent-live sweeps passed
    minimize/reflow/restore and close variants); permitted/mark/clear and game
    outcomes (D27; 5b implementation pending via the P2 `Activation request`
    setting item); orderly owner restart and end-session restore (D25/D26);
    joint native admission/overlay-close cleanup and fullscreen-send game journey
    incl. born-fullscreen sole-held-source/empty-target legs (D01/D28).
    Second agent-live D16 sweep: numbered ordinary-float roundtrip and
    fullscreen stay-carry passed; a small-frame destination-exit defect was
    fixed ([record](changes/archive/windows-floating-fullscreen-frame-restore.md)).
    Shared-core fixes serve Windows via the handoff below; Linux work owns
    KDE/core. D23 source slot: User decision 2026-10-17 option B (forget on
    confirmed rehome; shared-core implementation approved, not implemented;
    [proposal](changes/shared-core-d23-rehome-and-13f-maximize-base.md)).
- P2 | 0.1 triage settings follow-up | User 2026-10-12 approved post-0.1
  D09/D10 with two options each; functional names/options/WM tooltips are in
  [Move decisions](decisions.md#move-layout-and-output-commands).
  Approved D27 `Activation request`: `Mark urgent only` (default; tooltip
  Hyprland, i3, sway) / `Switch and focus` (tooltip COSMIC, xmonad). User-approved
  2026-10-16 Windows amendment: Switch and focus honors OS-permitted activation
  even over fullscreen/games, no urgency-only downgrade; default/non-permitted
  unchanged. Implement that behavior and help text; no activation setting is
  currently shipped. User 2026-10-17 folds D27 5b into this setting item;
  the standalone pending Windows implementation entry is removed. KDE applicability is reviewed when the setting
  is designed; no decision now. Functional names/options; WM names only in tooltips.
  Qualifies via COSMIC Tier 1 plus xmonad Tier 2 under the
  [tier rule](decisions.md#reference-tiers); [activation decision](decisions.md#activation-requests).
  Shared core policy settings serve Windows; Windows schema/UI/live wiring
  belongs to handoff item 22, coordinated after shared delivery.
- P0 | Windows dogfood 2026-10-11 | User live run `tile --user-start --trace`
  at `7f407ca`. All sub-items delivered offline (records in the archive);
  remaining are live/physical acceptance plus two approved-but-unimplemented
  shared policies: (a) pending-prompt stop cancels with no preset written;
  (b) verified departure focus feeds remembered return focus incl. fullscreen;
  (c) group projection honors minimum hints; (d) `core.born_maximized` default
  tile-on-launch + `keep-maximized-overlay` (both born values Paint-launch
  user-accepted physically 2026-10-12, scoped single-output); (e) border radius
  default 6 (user-accepted 2026-10-12); Win+B/Win+T launchers delivered;
  underlay candidate C `#662a82da` selected for both platforms (black/white/grey
  at 100%/150% visual check pending); items 13+17 (bounded min/max, predicate,
  current-hint exit/classification), 10+18 (float directional focus, half-snaps,
  maximized focus fence) and 11+15 (R-MAX-03 retile, ordinary maximized unfloat)
  delivered with scoped 2026-10-12 physical acceptances. Remaining user-run
  acceptance: broader rebinding, real Explorer restart, tray toggle focus
  observation, hidden retile/session restore/fixed marker/restart/failure/race/
  game/DPI/multi-output legs. Follow-ups: 5b folded into the P2 triage settings
  item; D23 source-slot option B (User 2026-10-17) shared-core implementation
  approved, not implemented; 13f fixed-override born-maximized option A (User
  2026-10-17) approved, not implemented
  ([proposal](changes/shared-core-d23-rehome-and-13f-maximize-base.md)); 13g
  safe-FFI route chosen (option A), implementation queued (see unsafe-FFI
  item); taskbar indicator pulled forward as dogfood blocker (see presentation
  research). Full sub-item history archived 2026-10-24.
- P1 | Unified foreground dev entry point | Delivered `8d379f1` (+ shell CI
  harness fixes): plain `just dev [verbose|trace]` dispatches per platform.
  User dogfooded Windows `just dev` 2026-10-13 (feels excellent). Remaining:
  tray Stop UI, whole-desktop visual/physical shortcut specifics, Linux KDE
  streaming/Ctrl+C teardown in devenv. [Record](changes/dev-entry-point.md)
  stays active until the KDE check passes.
- P0 | Windows owner threading | User direction 2026-10-12: rendering moves
  off the input hook thread (target C staged: dedicated input thread, then
  dedicated render thread). All three stages delivered offline 2026-10-12 with
  agent-live evidence; user physical 2026-10-12: edge drag-resize much faster,
  Win+arrow slowness fixed (`21a9e58`) and user-confirmed. Remaining: user
  physical single-output pixel/feel check, multi-output/mixed-DPI and
  gaming-cost legs. [Design](research/windows-port/owner-threading.md),
  [evidence](changes/archive/windows-resize-hook-latency.md). Full stage history
  archived 2026-10-24.
- P1 | Rust `unsafe` policy | User rule 2026-10-13: only tiny ABI/FFI
  boundaries in one dedicated FFI module per crate; every other module/crate
  uses `#![forbid(unsafe_code)]`. Guardrail applied 2026-10-12/13: `tiler-core`,
  `tiler-protocol` and unsafe-free Windows modules forbid unsafe; Linux planner
  env call sites replaced by explicit trace inputs (all three planner roots
  forbid unsafe); 29 test/example root gaps closed (1,644 workspace tests pass).
  KWin effect FFI stays a minimal boundary module.
  ([record](changes/archive/linux-planner-unsafe-guardrail.md)).
- P1 | KDE follow-ups from 2026-10-11 Windows dogfood | Delivered offline:
  R-MAX-06 born-maximized setting (default tile), active border radius 6,
  born-maximized shared-core policy (KDE keeps its conformance-pinned copy until
  post-0.1 Push code down, User option C 2026-10-13), Windows adapter-copy
  replacement, underlay candidate C `#662a82da` on both platforms (KDE
  kcfg/KCM default delivered). Remaining live checks: Settings UI, both-values
  launch, radius appearance, black/white/grey underlay visual check before 0.1.
  ([records](changes/archive/born-maximized-shared-policy.md)).
- P1 | Windows unsafe FFI consolidation | Required under user rule 2026-10-13.
  Delivered offline 2026-10-17: one minimal safe FFI module on `windows-sys`
  (User option A 2026-10-17, responsibility-split `ffi/` with one unsafe
  opt-in root); zero unsafe outside it; workspace lint inheritance and CI
  allowlist gate delivered (32/32 policy tests). Gates 2,596 passed;
  owned-window lifecycle, settings transactions, tray/pager picks and renderer
  smoke pass. Remaining: physical keyboard and DPI/display/Explorer acceptance.
  [Rule](decisions.md#rust-unsafe-guardrail),
  [research](research/windows-port/safe-win32-wrappers.md),
  [evidence](changes/archive/windows-unsafe-ffi-consolidation.md).
- P2 | Windows: review `windows-sys` unsafe for safe `windows` crate calls |
  User 2026-10-19, after the `windows` crate was added for the WinRT accent:
  inventory our `ffi/` unsafe and identify calls the `windows` crate exposes
  as genuinely safe APIs (WinRT/COM wrappers, owned handles), so correct
  unsafe usage defers to a high-quality upstream library where possible.
  Most Win32 functions stay `unsafe fn` in that crate too; expect a small
  result. Research first, findings and recommendation to the user before any
  migration. Revisits the 2026-10-17 option A choice in
  [research](research/windows-port/safe-win32-wrappers.md).
- P1 | Remove legacy routes and code | User 2026-10-18: pre-0.1, prune legacy
  aggressively. KDE removal delivered 2026-10-19: plan/workspace-send adapters,
  DescribePlan service/protocol, exclusive tests/fixtures and obsolete custom-tile
  harness removed; world formatter, native rendering/settings and intent store
  retained. Remaining: portable per-domain core API after Windows/macOS world
  adoption, plus reviewed native-only callers, fixtures and dead settings.
  [Evidence and outstanding visual/group smoke gap](changes/kde-legacy-route-removal.md).
- P1 | Repository cleanup pass | User 2026-10-18: the repository has grown
  large and cluttered; schedule a cleanup pass soon. Inventory and remove
  obsolete harnesses/scripts, superseded routes and fixtures, stale records and
  dead code, with a reviewed removal list before deleting. KDE's obsolete
  custom-tile acceptance harness/test and legacy-route fixture are removed
  ([evidence](changes/kde-legacy-route-removal.md)). Windows inventory complete: one
  completed note archived unchanged; no source/scripts proven safe to delete.
  Active acceptance records and parked implementation/test bases retained;
  three cleanup groups await user review. [Windows inventory](changes/archive/windows-cleanup-inventory.md).
- P1 | Module size refactoring | User 2026-10-14: split oversized sources toward
  the [file-size](principles.md#file-size) soft caps; behavior-preserving moves
  between features. Windows complete: `tiling_sys` 61,917 lines into a 103-file
  family (max production 1,455), owner/snapkey/settings/tiling/input_pump/
  render_pump split, tail split (`workspace` 2,918 -> 1,144, `product_hide`
  1,836 -> 135, `winarrow` 1,774 -> 731); no Windows test file over 3,000.
  Remaining: core `engine.rs`/`session.rs` large inline test modules as
  extraction candidates; shared crates and KDE files split with their work
  streams. ([split map](research/windows-port/tiling-sys-split-map.md),
  [phase 2](changes/archive/windows-tiling-sys-split-phase2.md)).
- P2 | Windows shortcut-dialog presentation review | Delivered and scripted-live
  verified 2026-10-17 (conflict preview, Apply/Revert dialogs, discard
  protection, preview-time stale refusal; 2,458/0/1 gates). User 2026-10-17 keeps
  the shipped dialogs as shipped (no extra Apply confirmation). Physical chord
  delivery is not claimed. [Delivery](changes/archive/windows-shortcut-settings-dialogs.md).
- P1 | Windows fullscreen-exit acceptance | Startup/born exits, retained managed
  slot return, borderless exit and owned-exit metadata retirement pinned
  offline; owned Notepad return and fullscreen stop/relaunch/exit tiling
  physically user-accepted 2026-10-12 (scoped single-output). User-selected
  automatic stale retirement delivered offline for positively restored nonzero
  chrome masks; originally captionless zero-bit metadata stays fail-closed
  without a restoration witness (Win+F11 owned toggle is the release path).
  Remaining: app-native/born-after-start/borderless, retirement physical,
  fixed/game/DPI/multi-output. [Record](changes/archive/windows-fullscreen-exit-acceptance.md).
- P0 | Windows port | KDE-first core extraction finished at K1 (user
  2026-09-30; K2/K3 revisit when macOS starts). User decisions (2026-09-30):
  Windows 11 x64 only; managed per-monitor workspaces required for
  feature-complete; Meta/Win default shortcuts matching KDE (2026-10-01),
  Windows Snap takeover with visible off, Win+L explicit opt-in; custom-drawing
  underlay experiment with outline fallback; Store plus signed installer plus
  winget under evaluation; develop natively on Windows 11; multi-output live
  work on a multi-output setup; elevated windows unmanaged by default; logical
  workspace model refined in spikes; shared settings/action intent (deferred K2)
  and difference classification (deferred K3) revisited when macOS starts.
  Status: Phase 1 lifecycle/recovery + input, Phase 2 Engine-driven tiling
  preview, shortcut slice, foreground/hook handling, settings UI + tray +
  first-run, managed workspaces, latency fixes and minimum-size hints all
  delivered and accepted on single-display setups; full history archived
  2026-10-24. Open: final package/update channel (see 0.1 release); day-one
  setup and pending governance ([environment](windows-dev-environment.md)).
  [Plan](research/windows-port/plan.md), [decision](decisions.md#scope-and-platform-goals).
- P0 | Windows feature parity queue | User order (2026-10-02), each matching
  KDE behavior/bindings. All slices delivered offline; physical/live acceptance
  pending per slice: (1) active border; (2) group underlay A/B (C parked, see
  PARKED); (3) maximise; (4) fullscreen; (5) float; (6) sticky float; (7) mouse
  drag move + drop preview same-output (Win+drag removed 2026-10-18, native
  title-bar drag + shared preview/drop retained;
  [removal](changes/archive/windows-win-drag-review.md)); (8) multi-output
  foundation (`2c05ee5` + `f2169d6`; explicit output send `fda618f`;
  [remaining checks](changes/windows-multi-output-foundation.md#remaining-physical-check));
  (9) taskbar workspace indicator pulled forward (see presentation research);
  (10) settings core slice + tray + first-run + workspace tiling toggle;
  (11) settings grouping route approved (see shared settings item). Agent runs
  were disturbed by Xbox-mode paths (see gaming coexistence). Full slice
  records archived 2026-10-24.
- P0 | Windows gaming coexistence | Windows release gate after multi-output,
  before any Windows release (user 2026-10-10); not now despite P0. (a) Authentic
  ownership with disclosed limitation selected 2026-10-08 (Compatible/disable/
  rebind provide Game Bar access); exact-chord containment (Win+G/Win+F11 leak)
  stays parked for later test + user review; no host-setting/registry/policy/
  hook-thread alternative selected or authorized. (b) user chose
  documented-signal-only Xbox-mode detection (2026-10-03); blocked until one
  exists. (c) researched; alternate Game Bar access deferred (see Future).
  2026-10-16 incident: invisibility/exception origin unresolved in normal logs;
  no runtime fix selected - user-chosen fullscreen/float rebinds recommended for
  conflict avoidance; bounded visibility/reveal/exception diagnostics needed
  before another recovery repro. M16 gaming-cost: 100 ms poll removed; idle
  matrix + pager-wait evidence measured 2026-10-17/18; visible-pager 2 s
  topmost-recovery timer kept for 0.1 (User option A 2026-10-19, amending
  decision 5); remaining coverage deferred to the P2 pager event-coverage item.
  Game cost unmeasured; physical game measurements are user-owned
  ([principle](principles.md#gaming-compatibility)). Full incident/measure
  history archived 2026-10-24. [Record](changes/windows-gaming-coexistence.md),
  [decision](decisions.md#gaming-safety).
- P0 | Windows release gate | Now part of 0.1 (user 2026-10-17; was its own
  later release, user 2026-10-10); exact Windows 0.1 gate pending definition
  under the 0.1 release item. Current contents: managed
  per-monitor workspaces (multi-output), gaming coexistence (Win+G/F11
  containment review, Xbox mode detection, M16 event-driven owner measurement),
  combined workspace-presentation research (hidden-workspace Alt+Tab/taskbar
  semantics plus taskbar indicator parity 10), restart persistence (item 8,
  on-window markers selected 2026-10-10) and package/update channel decision (short research refresh
  during Windows release planning; most obvious and unsurprising for users).
  Gaming and Windows presentation research run after multi-output,
  before release; presentation research has produced options plus a
  recommendation for the user. The parked selection decision remains open
  and is distinct from release completion. Non-native lessons and evidence stay linked:
  [event/pump boundary](research/non-native-tiler-host-interaction.md#layer-layout-authority-events-and-reconciliation-dim-1),
  [workspaces/Alt+Tab](research/non-native-tiler-host-interaction.md#workspaces-monitors-overview-and-alttab-dim-4),
  [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [evidence](changes/archive/non-native-tiler-host-interaction.md).
- P0 | Windows workspace presentation research | After multi-output, before
  Windows release (user 2026-10-10): hidden-workspace Alt+Tab/taskbar semantics
  + taskbar workspace indicator (parity 10) with options + recommendation.
  Research delivered 2026-10-17 (keep `SW_HIDE`, User decision 2026-10-17 for
  0.1; per-option comparison, owned-helper live visibility baseline,
  reference-outcome discriminators). Taskbar indicator slice pulled forward as
  dogfood blocker (user 2026-10-14): KDE pager-style per-output indicator
  anchored bar-left of the tray, default-on with opt-out; MVP delivered
  2026-10-16 on the primary bottom taskbar, user-confirmed 2026-10-17 (auto-hide
  hides the pager, tray number/menu remain). Remaining: secondary tray anchors,
  physical multi-output, mouse/game/anti-cheat, DPI reconfiguration, non-bottom
  taskbars, accessibility/theme/cost; whole-strip overcapacity (no pager drawn,
  reason logged) needs physical proof. D27 5a/5b pending via the P2
  `Activation request` setting item. Mechanism + switch/pull selection moved to
  post-0.1 research (see P2 hidden-workspace item). Bounded user-assisted cloak
  spike active: [spike](changes/windows-cloak-spike.md).
  [Indicator options](research/windows-port/taskbar-workspace-indicator.md),
  [Alt+Tab note](research/windows-port/alt-tab-hidden-workspaces.md),
  [evidence](changes/archive/windows-workspace-presentation-research.md).
- P1 | KDE follow-ups from the Windows port | Audit 2026-10-05; user order
  B1/B2, B3-B5, B7, B8. Delivered offline: B1/B2 stale maximize/sticky
  attempted-state refusals repaired; B3-B5 shared-core fixtures, no defect;
  B6 infeasible minimums origin+minimum; Q3 born-maximized admission (tile with
  reserved slot + maximize overlay); Q3-scope R-MAX-03 floating retile overlay;
  B9 overlaid unfloat (unmaximize then fresh admission, User 2026-10-08); B7
  movement-only underlay A/B (C parked). Shared restart store in core selected
  (user 2026-10-08). Still pending, user-owned live: native-change repeats and
  held-key autorepeat ([discriminator](spec/reference-outcomes.md#r-max-04--r-flt-02-kde-held-key-discriminator));
  physical delivery/suppression; multi-output remap checks; B8 after user
  acceptance of the KDE shortcut controls. Full audit history archived 2026-10-24.
- P1 | Windows parity with the 2026-10-05 KDE session | Next Windows session:
  (a) float-origin directional focus + half-snaps (delivered offline 2026-10-12,
  ordinary single-output user-accepted); (b) Q3 born-max reserved slot + overlay
  and R-MAX-03 (delivered offline 2026-10-12, scoped legs user-accepted);
  (c) B9 overlaid unfloat (delivered offline; scoped native legs agent-observed);
  (d) keyboard resize (delivered 2026-10-11; physical interception user-owned).
  Non-local workspace modes incl. mode-aware hotplug delivered offline
  2026-10-15; physical two-output journeys user-owned.
  [Decisions](decisions.md#window-state-float-sticky-maximize-fullscreen).
- P1 | Adopt reference-consensus additions | User 2026-10-07 accepted Table A
  recommendations; items 1-5 shared core/KDE delivered (item 1 single-output
  user-confirmed 2026-10-07; items 2-5 offline, native journeys pending),
  Windows adapter wiring delivered per the handoff below (archived 2026-10-24).
  Q2 R-SPC-04 shared core + Linux planner + KDE delivered offline (User
  2026-10-08); D1 predicate setting + functional naming delivered; D5/D6
  delivered; D7 tile-override store delivered. Q3 R-RST-01 Linux planner/KDE
  intentional-float restart delivered offline. Q4 R-WS-12 core + Linux planner +
  KDE whole-active-workspace migration delivered offline (D8
  fullscreen/maximized carry delivered 2026-10-09). R-LAY-04 PARKED/OPEN until a
  genuine second layout exists. Bindings for other accepted additions remain to
  be chosen. ([consensus](research/reference-wm-consensus.md),
  [decisions](decisions.md#move-layout-and-output-commands))
- P1 | Windows handoff: reference-consensus additions | D1, user 2026-10-07:
  all 24 Windows adapter items delivered offline (full checklists, seams and
  user journeys archived 2026-10-24; retained as evidence, superseded as
  planning detail). Native acceptance pending in [Pending
  live checks](#pending-live-checks) and the
  [physical checklist](changes/windows-physical-checklist.md). Shared-core
  delivery serves Windows; Windows adapter/native work follows the shared
  contract.
  ### Common definition of done and live-check protocol
  Archived with the handoff above. Current native/live acceptance runs through
  the [Windows physical/user-run checklist](changes/windows-physical-checklist.md)
  and [Pending live checks](#pending-live-checks); live work is user-owned per
  [live Windows testing](live-windows-testing.md).

  ### Source discrepancies to preserve and report

  | Existing assertion | Current source / implementation gap | Handoff treatment |
  | --- | --- | --- |
  | [R-OUT-01](spec/reference-outcomes/multi-output.md#r-out-01-scrolling-assessment-move-left-onto-an-occupied-output) Ours Windows claimed exhausted-horizontal R4; [R-MOV-05](spec/reference-outcomes/move.md#r-mov-05-backfill-edge-move-with-no-left-neighbor-scrolling) claimed horizontal crossing with Up/Down excluded. | `src/tiling_sys.rs:6099/6105` sets `cross_output_transfer:false` in a single-domain event; no Windows cross actuation. Shared core supports four-direction R4. | Corrected both cells 2026-10-08 at `db31234` to local-only current-code facts. Handoff item 5 is sequenced after the single-output queue (18 before 10/11); no native acceptance claimed. |
  | [R-WS-10](spec/reference-outcomes/workspaces.md#r-ws-10-send-b-away-empty-middle-retained-vs-removed) and [R-WS-16](spec/reference-outcomes/workspaces.md#r-ws-16-previous-after-the-visited-empty-workspace-is-removed) Ours Windows said no removal path; spec REQ-WS-10 repeated it. | `src/workspace.rs:508` removes stable IDs; `src/tiling_sys.rs:9228-9234` retirement cleanup and :10074-10080 selection invoke it. `core/workspace.rs:35` plans eligible invisible-empty removal, protecting the trailing spare and minimum count. | Corrected both cells and REQ-WS-10 2026-10-08 at `db31234`. Exact native scenarios remain TBD; handoff item 1 invalidates actually removed IDs and tests surviving empties separately. |
  | Existing higher-level item-2 summary says Compatible disables arrows, while decision 2.1 identifies stock holders only on KDE and says Windows ownership unknown. | `src/settings.rs:954-957` currently returns None for all Ctrl/Alt chords; no Windows holder evidence for new send/output arms. Verified 2026-10-08 at `db31234`; status outstanding, no new policy selected. | KDE arrows are disabled by KDE Compatible. Windows unknown new arms keep defaults with unknown-ownership text until evidenced policy is recorded; do not manufacture Windows holder claims. |
  | Current settings/rebind helpers assume every implemented action has a canonical default chord. | `src/settings.rs:1044` canonical helper returns None for empty defaults; `build_remap` :1182 skips that action; `validate_bindings` :1405 rejects it. Keep-empty is currently active/effective (:1082); UI `refresh_info` (`src/settings_ui.rs:248`) labels it disabled. Verified 2026-10-08 at `db31234`; status outstanding, plus `settings_ui.rs:286` resize note. | Items 2/5 must wire unbound ACTION targets explicitly, make Keep-empty non-intercepting and display unbound distinctly from Disabled. These are adapter limitations, not a decision reversal. |
- P1 | Shortcut conflict model on KDE and macOS | Per-binding conflict list
  plus compatible/authentic presets (user 2026-10-03); KDE builds on its
  existing shortcut override Apply/Force/Revert; macOS when it starts.
  KDE catalog/presets and Keep-preserving Apply delivered offline (M13,
  2026-10-09); live acceptance is in Pending live checks below.
  Integrated rebind and KDE first-run prompt deferred. KDE live
  acceptance pending: [checks](live-shortcut-override-verification.md),
  [record](changes/kde-shortcut-conflicts.md).
- P1 | Cross-platform functional specification | After the Windows tiling
  dogfood slice, define window/workspace behavior and keyboard shortcuts as
  the selected behavior contract for Linux, Windows and macOS, except recorded
  exceptions. COSMIC is the reference-default yardstick; macOS modifier
  mapping is decided when macOS starts.
  [Reference outcomes](spec/reference-outcomes.md) use per-area Given/When/Then
  scenarios, twelve pinned reference profiles and separate Ours KDE/Windows
  outcomes. Source evidence is not a product decision; AGENTS.md requires a
  discriminating row per ambiguity. All twelve remaining source-fill passes
  completed (`f04511c`..`bfa486b`): zero not-attempted/unsupported-pending
  candidates remain; residual host, fixture and live limits are in the
  [archived ledger](changes/archive/reference-source-fill-remaining.md).
  [Functional spec](spec/functional-spec.md) remains single-file with a
  [grouped open-decisions index](spec/functional-spec.md#open-index).
  Current review 2026-10-12: all nine prior provisional requirements ratified;
  B9 no-refusal sub-leg selected; R-LAY-04 parked OPEN; G-06 makes
  REQ-MAX-08 NORMATIVE. Batch 1 approved 16 more rows; thirteen units / seventeen rows
  approved 2026-10-12, all 28 triage units decided. Totals: 138 NORMATIVE,
  28 OPEN, 0 PROVISIONAL requirement rows; 156 scenarios including R-WS-27 with TBD
  reference outcomes. Platform implementation/native gaps stay
  distinct from selected intent; Windows control cells now cite Windows evidence.
  [Cross-WM consensus analysis](research/reference-wm-consensus.md) Table A
  now lists 24 Ours-vs-strong-consensus differences with recommendations
  (selected decisions in [decisions](decisions.md)), plus strong-consensus predicates where Ours is
  TBD pending native observation. Splits decided 2026-10-07:
  R-DRAG-07 host-native presentation (KDE frame follows pointer, Windows
  Win-drag stationary preview) and R-DRAG-08 focus at press on both
  platforms (Windows implementation pending, KDE timing needs a live check).
  Historical live-test queue snapshot: 508 cells grouped by environment
  (2026-10-07, not the current TBD total); candidate per-WM environments
  are proposed in the
  [live-test environment proposal](research/live-test-vms/proposal.md).
  Table A selections and 2026-10-10 comparison decisions are recorded in
  [decisions](decisions.md#reference-matrix-and-spec-authority);
  priority native TBD rows R-WS-02, R-WS-04, R-WS-05, R-START-03, R-MAX-01 remain.
- P3 | Prior-art catalogue upkeep | Catalogue delivered 2026-10-03; maintain
  [the index](research/prior-art.md) as projects are studied. Deferred general
  source analysis: FancyWM core submodules and niri/sway/river/awesome/dwm
  (distinct from the completed bounded matrix source-fill passes).
  [Evidence](changes/archive/prior-art-catalogue.md).
- P1 | macOS port | Started 2026-10-14 (user), superseding the after-KDE-0.1
  sequencing. Port contract finalized 2026-10-24:
  [world adapter guide](porting/world-adapter-guide.md) plus
  [macOS port notes](porting/macos-port-notes.md); macOS remains unported to
  v2: legacy routes must be replaced together (no dual route), supplying
  truthful native receipts/transport and host-owned bootstrap/outside-change
  adoption (see boundaries item).
  [Staged v2 plan and abstraction-fit review](changes/macos-world-v2-port.md)
  record the current build gaps, KDE adoption dependency and acceptance gates.
  **Current 2026-10-16:** basic tiling dogfood preview
  ([guide](changes/macos-dogfood-preview.md)) user-accepted after
  [hardening](changes/archive/macos-dogfood-hardening.md): directional focus
  (including same-app), move, 8-point outer/inner gaps and quiet idle logging pass
  physically. [Physical results](changes/archive/macos-dogfood-hardening.md#user-physical-results-2026-10-16):
  OS disabled the tap under a stall without a timeout notification; health-check
  re-enable recovered (accepted tentatively). Carbon physical delivery failed
  (diagnostic-only). All three manual native Space-transfer routes moved an
  ordinary window ([results](research/macos-port/native-spaces-routes.md#user-manual-transfer-check--2026-10-16)).
  **Next, in order (user-reviewed 2026-10-17):**
  1. **Done 2026-10-17:** user-assisted synthetic-drag discriminator
     ([results](changes/macos-managed-workspaces.md#user-assisted-discriminator-results-2026-10-17)).
     A synthetic drag plus a Desktop-N chord moved TextEdit to another Space.
     Synthetic Ctrl+Right failed even alone, because Mission Control's arrow
     holders require Control plus Fn and the executor sets only Control.
     Transfer is unparked.
   2. **Done 2026-10-18:** full world-state core replies, including native runtime
      Ctrl+2/Ctrl+1, per-window retained-layout correction across blind hidden AX
      snapshots, inspected ring/indicator screenshots and clean overlay teardown.
      903 macOS tests, strict clippy, format and signed build/verify pass; unsafe
      remains 114. [Outcome](changes/archive/macos-full-state-replies.md#final-outcome-2026-10-18).
    3. Finish managed native workspaces. **2026-10-19:** numbered select,
       send-follow and default-unbound send-stay pass live; same-current send is
       an unchanged no-op. Trailing select/send-follow/indicator are armed after
       offline integration review; native procedure 3 remains pending.
       Existing-contract bindings settle creation; stale clicks refuse; send
       preparation runs off-main. Trailing sends above Desktop 9 refuse before
       creation. Next: diagnostic-harness review and disposable TextEdit
       procedure 3, then per-display attribution. Outward pixels, relative
       actions and physical/two-display checks pending. Four-crate tests 2385
       passed; unsafe 114, unchanged.
       [Checkpoint and checklist](changes/macos-managed-workspaces.md#trailing-workspace-armed-checkpoint-2026-10-19).
       Dogfood 2026-10-19: MC select animation and post-send tiling feel slow.
       Offline investigation delivered: MC AX selector plus 300 ms
       stabilization, 450 ms drag grab/hold, and no reconcile on Space change
       (1 s backstop when AX is silent) are implementation costs; SIP-on
       Dock-swipe selection is the leading candidate
       ([findings](research/macos-port/native-spaces-routes.md)). **Top macOS
       priority (user 2026-10-19):** fast workspace switching and moving.
       Delivered for dogfooding and user-approved 2026-10-20: yabai-style
       Dock-swipe selection (43-62 ms native-current), Dock exit gating after
       creation, bridged native sends with drag fallback and Dock-swipe follow
       (arrival 88-98 ms), event-driven arrival rereads (first correct tile
       1,000 -> 270 ms), trailing reuse (122 ms) and post-create select
       (900-944 ms), and exact-shape AccessibilityUIServer/ControlCenter
       removal exceptions. Next, in order:
       0. Fixture-authenticated guarded input harness: extend the guarded
          diagnostic controls (currently legacy-helper/TextEdit only) to
          authenticate live-fixture windows, so product focus/move chords and
          managed select/send/trailing checks run against the fixture
          ([fixture integration](changes/archive/macos-fixture-tiler-integration.md)).
          Prerequisite for automated 3a.
       a. Live-verify the trailing send-follow focus fix (yabai move ->
          front-PSN -> selector): sole-window follow to reused and new Spaces,
          inverse follow, stay/source focus, two create/auto-remove cycles,
          with timings.
       b. Restore MC AX selection as the automatic fallback when Dock-swipe is
          unavailable (e.g. refused macOS >= 27 payload), per
          [Host Integration](principles.md#host-integration).
       c. User-configurable route settings for selection (Dock-swipe / MC AX)
          and sends (bridged / drag), per Host Integration.
       d. Unproven: stay visit-to-tile latency, physical smoothness/input
          readiness, multi-display, macOS 15 floor, user-run opened/pinned
          ControlCenter contrast.
       [Send checkpoint and checklist](changes/macos-managed-workspaces.md#send-latency-work-2026-10-20).
  4. Drag/drop overlays, re-scoped (user 2026-10-17): no modifier+drag; the
     producer is a passively observed native title-bar drag of a managed
     window, matching Windows' title-bar route, with physical user acceptance.
     Offline Engine preview/drop plumbing and the above-target carrier exist;
     the Meta+left diagnostic stays hard-gated until replaced.
     [Earlier evidence](changes/macos-drag-drop-overlays.md).
  5. SkyLight-created window experiment for the group underlay and a
     below-target active border
     ([research](research/macos-port/underlay-rendering-routes.md),
     [decision](decisions.md)): Experiments 1 and 3,
     then remove the failed AppKit-relative and SkyLight-on-AppKit ordering
     code. If it passes, ship the product underlay and move the border below
     the target. [Underlay evidence](changes/macos-group-underlay.md).
  6. `ffi.rs` unsafe surface review straight after item 5 (unsafe blocks went
     93 -> 114 on 2026-10-17, 114 -> 132 on 2026-10-20): minimise and isolate every unsafe use, prefer
     maintained bindings, and remove diagnostics that no longer serve a live
     route.
  7. macOS 0.1 gate: keep the current definition (P0 0.1 item); decide after
     dogfooding.
  8. Reliability and game-safety work required by the gate (sleep/wake, display
     plug/unplug, resolution changes, games): none exercised on macOS yet.
  9. Smaller items: verify that Authentic can override each conflicting macOS
     system shortcut for the default bindings (test-only slice of the shortcut
     conflict model, user 2026-10-17); indicator icons (deferred) and the
     indicator on multiple displays; `just launch` exits nonzero on successful
     probes; `just dev` (`scripts/macos-dev.sh`) leaks its `tail -F` log
     followers after the run ends (orphans found 2026-10-19), so reap them on
     every exit path; split oversized macOS production files toward the
     [file size](principles.md#file-size) cap (several hotspots, including
     `runtime.rs`); Mission Control Ctrl+Up/Down chords; macOS 15 floor; P2
     classification discriminators.
  10. Distribution: DMG and Homebrew, signing and notarisation (paid Apple
      account), update channel, App Store.
  **Pending physical acceptance (dogfooding):** workspace selects, send-follow,
  send-stay and trailing Space, plus the user-approved 2026-10-19 decisions
  ([dogfood checklist](changes/macos-managed-workspaces.md#trailing-workspace-armed-checkpoint-2026-10-19));
  Mission Control transition feel;
  consume-first ([§5](changes/archive/macos-dogfood-hardening.md#5-consume-first-parity-2026-10-17));
  active border appearance ([§6](changes/archive/macos-dogfood-hardening.md#6-active-border-appearance-check-pending-2026-10-17));
  workspace indicator click and Space-switch updates
  ([record](changes/macos-workspace-indicator.md)). Delivered 2026-10-17:
  consume-first input parity with Windows
  ([evidence](changes/archive/macos-consume-first-input.md)); active border
  ([evidence](changes/macos-active-border.md)); menu-bar workspace indicator
  (user quick check passed).
  Native-Space capability comparison ([verdict](research/macos-port/native-spaces-routes.md#live-verdict--2026-10-16),
  [record](changes/archive/macos-native-workspaces.md)): MC AX create/select/remove
   and gesture select work on owned fixtures; direct SLS window move dispatched
   without effect; the old unmangled bridged lookup missed the dispatcher
   (corrected by the send work above).
   Earlier milestones (summarized; full evidence in linked records): kickoff and
   Phase 0 decisions/probes (TCC/signing, owned-window AX legs, Carbon/tap
   lifecycle, tier-2 yabai-style route), Phase 1 threading/lifecycle skeleton,
   window-policy/classification slice (User option A 2026-10-15), scoped-tile
   dev mode (406 macOS tests, signed bundle), dogfood hardening, consume-first
   input parity, active border and menu-bar indicator (user quick-check passed).
   Full history: [kickoff](changes/archive/macos-port-kickoff.md),
   [Phase 0](changes/macos-phase-0.md),
   [Phase 1](changes/macos-phase-1-skeleton.md),
   [classification](changes/macos-window-classification.md),
   [scoped tile](changes/macos-scoped-tile.md),
   [hardening](changes/archive/macos-dogfood-hardening.md).
  [setup](macos-dev-environment.md)
  [survey](research/macos-port/prior-art.md)
  [plan](research/macos-port/plan.md)
- P2 | Shared minimize/fullscreen reference comparison |
   User-approved option A (2026-10-15): keep truthful adapter reporting for
   tile A/B/C -> minimize B -> restore B fullscreen -> exit; existing core
   forgets the saved D23 slot and fresh-admits. Keep this behavior; compare
   exact Tier 1/native outcomes before proposing any shared slot change
  ([discriminator](spec/reference-outcomes.md#macos-classification-discriminators),
   [record](changes/macos-window-classification.md)).
- P1 | Windows shared window-policy adoption | Adopt the approved
   `tiler_core::window_policy` predicates in the Windows work stream,
   preserving adapter acquisition/actuation and current conformance vectors.
- P1 | KDE shared window-policy adoption | Adopt the approved
   `tiler_core::window_policy` predicates in the KDE work stream,
   preserving script-host acquisition and documented host-specific exceptions.
- P2 | macOS classification dogfooding discriminators | Investigate AX-size-
   settability-only admission, actionable accessory documents, elevated-level
   standard windows, app-specific nonstandard documents and sheet identity.
   Keep conservative current candidates tentative; refine against Tier 1 intent
   and the [minimal sequences](spec/reference-outcomes.md#macos-classification-discriminators).
- P1 | Rust toolchain tracking (recurring) | User (2026-09-30): track latest
  stable Rust pre-1.0 and fix breakage. Windows uses rustup `stable`; Linux
  and CI get Rust from the nixpkgs pins in `devenv.yaml` and `flake.lock`, which must be bumped
  regularly to stay close to stable. Revisit the upgrade process at 1.0.
  Checked 2026-10-04 (`4212456`, `929adf5`, `c486c59`; CI green): hosted
  Windows/macOS CI now refreshes stable (1.99.0); Linux devenv stays at
  1.98.1 and native CI flake at 1.97.1 until 1.99.0 reaches nixos-unstable
  (read-only recheck 2026-10-09: devenv `e554fab7` 1.98.1; native flake
  `54ba4bce` 1.97.1, lagging devenv; latest stable 1.99.0 dated 2026-10-01;
   unstable HEAD `e7439b6b` dated 2026-10-08 still 1.98.1, no `1_99.nix`;
2945:   read-only recheck 2026-10-12: latest stable still 1.99.0 dated 2026-10-01;
2946:   unstable HEAD `6510408d` dated 2026-10-10 still defaults to 1.98.1, no
2947:   `1_99.nix`, KWin/Qt/KF unchanged at 6.7.5 / 6.11.2 / 6.30.0; 1.99.0 has
2948:   reached `staging-next` but per procedure that route needs a separate decision).
  No bump. Next: recheck, then bump devenv and native flake
  pins; user re-enters devenv after the bump.
  [record](changes/archive/rust-toolchain-tracking-2026-10.md)
- PARKED | Group underlay stage C (unfocused dragged window) | A/B delivered
  and user-confirmed 2026-10-05; multi-output remap checks remain under K1 visual
  policy below. C is parked
  for reassessment with Windows parity item 7; stop/report if it grows complex.
  [change](changes/group-underlay-move-trigger.md)
- P1 | Ghostty/local native alignment | The ~56 px shortfall is unexplained
  (source-only baseline fix is in; needs a fresh `just dev trace` local-move
  plus follow-on command trace), and one requested `2032x1092` became
  `1920x1036` while another primary Ghostty accepted full size (per-window
  native cap or stale output-derived cap unproven). Hypothesis
  (2026-09-28): 1920x1036 equals another output's work area, so the
  window may be constrained to the other output. Next: `just dev trace`
  with one tall tiled window on the source output; check output, bounds and constraints.
  Gates reconciliation phase 2.
  [bounds fix](changes/archive/multi-output-domain-bounds.md)
  [phase 2 parked](changes/learned-size-limits.md)
  [drag investigation](changes/window-alignment-drag-investigation.md)
- P1 | External NixOS/Home Manager delivery validation | Clean external
  install, update, rollback, and host-matching KWin ABI still unproven
  on an independent host. Remains P1 in the 0.1 gate (user 2026-10-10).
  [change](changes/archive/nix-current-host-delivery.md)
- P1 | 0.1 packaging: mainstream install path | Nix flake/Home Manager,
  GitHub Release and OBS distro packages (user 2026-10-10; KDE Store excluded).
  OBS/AUR names are `omnitiler` and `omnitiler-native-effect` (user
  2026-10-11); core Settings ownership delivered
  ([evidence](changes/archive/release-0.1-core-settings.md)), effect companion
  is effect-only.
  OBS builds release tags only into one stable repository: openSUSE Tumbleweed,
  Fedora 43/44 with updates repos; Arch via AUR PKGBUILD, not OBS pacman.
  Ubuntu 26.04/26.10 support delivered offline under the KWin 6.6 floor;
  [evidence](changes/archive/kwin-6.6-support.md). KDE neon is out of 0.1.
  User 2026-10-18: maintainer OBS and AUR accounts now exist; project creation
  and remaining account-dependent steps are deferred until close to 0.1.
  Spec, PKGBUILD and OBS service prep may proceed offline.
  Acceptance: core script/planner/tray independent of optional ABI-bound effect;
  KWin updates remain possible without the companion (Debian exactly pins the
  compiled-against KWin build); mismatched/absent effect degrades borders/drag preview
  until rebuild. Must-pass missing/failed/removed effect check keeps tiling working
  and KWin stable. Install/uninstall docs, package descriptions and a post-removal
  package note say "press Revert in Settings before removing".
  [research](research/distribution-package-feasibility/feasibility.md)
  [OBS](research/distribution-package-feasibility/obs.md)
  Offline prep delivered 2026-10-11: tagged-SHA prebuilt JS + vendored Rust
  source archive, checksums, release/credential-gated trigger workflows,
  RPM and AUR split recipes, Ubuntu core Debian recipe, user activation units
  and Revert guidance. Exact scratch-tag archive built without networking in
  Tumbleweed/Fedora 44 RPM, Arch split and Ubuntu 26.04 Debian containers;
  Fedora 43 passed an earlier source-contract probe. External Nix validation
  remains separate. Settings ownership is delivered: both KCMs remain in core
  when the effect is absent. Packaging item remains P1: implement the selected
  [Route 3 handoff script](research/distribution-package-feasibility/obs-release-handoff.md#selected-route-3-consequences)
  close to 0.1, provision targets, complete third-party notice delivery and prove
  remaining solver/live-session behavior (Ubuntu apt solver passed).
  [Preparation](changes/archive/release-0.1-offline-packaging.md)
- P2 | Simplify drag preview + cross-output drop code | Post-live cleanup only:
  cut advisory `hover_prior` validation, lag-only fences, verbose comments;
  net deletion, no behavior change.
  [change](changes/archive/cross-output-drag-preview.md)
- P2 | Live sibling reflow while dragging | Deferred, not light (est. several
  hundred to ~1,000 lines, write-fighting risk); research done.
  [research](research/drag-and-drop-reorganisation.md). Link with post-0.1
  KDE interaction performance/threading and Push code down: research KWin
  effect/script threading constraints before considering multithreaded Rust
  interaction work with thin host shims (User 2026-10-13).
- P2 | Robust difference reconciliation | Scope decided (user, 2026-09-28):
  (1) event/next-command detection only, no polling (revisit after testing);
  (2) learn size limits only from settled repeatable evidence; (3) replan
  neighbours around learned limits only, native maximum behavior unchanged;
  (4) KWin script and Rust tiling only, native effect unchanged. Added
  complexity must deliver more value than it costs. Phase 1 (shared
  foreground/hidden classifier, refresh classification logs) passed the
  user's live test at `062d707` (2026-09-28, "felt good and minimally
  janky"; trace `omnitiler-dev.uE1S5n.log`). Quiet
  refresh outcomes were ~60% of refresh log lines. Phase 2 (learned limits)
  parked by the user (2026-09-28) after a candidate exceeded the complexity
  rule; preserved on branch `wip/learned-size-limits`; resumes only if the
  P1 Ghostty/output check leaves a genuine client-held limit.
  [record](changes/archive/robust-difference-reconciliation.md)
  [parked](changes/learned-size-limits.md)
- P2 | PID 3568836 SIGABRT (`QKeySequence` D-Bus abort) | Formally open;
  needs sender, method, and fault-stack evidence before attributing it to
  the effect or script.
  [record](changes/archive/kwin-qkeysequence-dbus-abort.md)
- PARKED | JavaScript workload evidence | User decision 2026-10-09;
  relevant only if a native replacement of KWin-script window add/remove
  handling is proposed, requiring sustained-workload evidence then.
  [change](changes/js-workload.md)
- DEFERRED 2026-10-10 | Reference matrix follow-ups | Both host-source pinning
  (GNOME Shell/Mutter/wlroots; former estimate 19 cells, now 40 host-source
  candidates after reclassification: 38 PaperWM, 2 sway) and fixture/scenario
  wording tightening (302 cells across 92 rows) are deferred. Revisit wording
  only when a specific open decision depends on a row; neither is P1 work.
  [Residual ledger](changes/archive/reference-source-fill-remaining.md),
  [decision](decisions.md#reference-matrix-and-spec-authority).
- P2 | Windows pager event coverage | Post-0.1 (user 2026-10-19 option A,
  amending 2026-10-18 decision 5): retain the visible-pager 2 s
  topmost-recovery timer for 0.1. Exercise own-window instrumentation,
  arbitrary visibility loss, Explorer restart, display/DPI changes and
  broader reorder coverage, including the observed desktop-parent
  `EVENT_OBJECT_REORDER` (`OBJID_CLIENT`) outside current subscriptions.
  Prove complete event coverage and self-feedback-safe recovery before
  replacing the timer; no best-effort substitute.
  [Probe and limits](changes/archive/windows-pager-event-coverage.md).
- P2 | Keyboard-layout support | After initial release; US keyboards only
  for MVP. Read-only investigation queued: behavior of `Meta+Shift+1..9` and
  the US shifted-symbol aliases on German, AZERTY, UK and Nordic layouts.
  [change](changes/shortcuts.md)
- PARKED | Live-test reference environments | Deferred until after 0.1;
  revisit when a specific decision needs live reference evidence or tabbed
  stacks are designed (user 2026-10-10). Exception 2026-10-13: the narrowed
  KDE multi-output must-pass environment slice is pre-0.1, listed above;
  the general reference-environments proposal remains parked research.
  [proposal](research/live-test-vms/proposal.md)
- P3 | Gap-drag anchor (post-0.1) | No anchor at gap 0; gap-drag exists only
  with gaps above 0, edge-drag covers gap 0 (user 2026-10-10). Research complete
  (layer-shell gap surfaces first, KWin input filter fallback; needs a Rust
  split-boundary request).
  [native boundary](decisions.md#engine-architecture-and-convergence)
- P2 | Commit metadata leak | Commit `c4b181f` carries a machine host name in
  its author email ([Local-System Information](../AGENTS.md#local-system-information)).
  Fixing it rewrites published history and needs a force-push: explicit user
  approval, at a time when no other agent is pushing.
- P3 | Stale branches | Twelve stale branches need explicit user
  authorization before deletion.
  [branches](https://github.com/beefsack/OmniTiler/branches)
- P3 | Other compositor validation | bspwm, Hyprland, COSMIC runtime
  validation only; pinned-semantics research is not a substitute.
  [comparison](reference-wm-comparison.md)
  [profile research](research/reference-wm-profile-support.md)
- P3 | Artifact publication | GitHub Release plus OBS distro packages once
  0.1 delivery prerequisites are complete, for the 0.1 release; KDE Store
  excluded (user 2026-10-10).
  [foundations](changes/archive/delivered-foundations.md)
- P3 | Live workspace-mode switch (post-MVP) | Quiesce, rebuild mapping,
  fresh-adopt without native moves; needs a safe Planner generation
  transition.
  [research](research/live-settings-after-ar15.md)
- P3 | Post-MVP tiling profiles | Re-expose `shortcutProfile` with distinct
  catalogs and live switching when adding new tiling types.
  [change](changes/shortcuts.md)
- P3 | Tabbed stacks (next after the 0.1 release) | User 2026-10-07: tabs are
  a feature of the most important reference tilers (COSMIC, Hyprland, i3,
  sway), so V-GROUP-STACK leaves deferral and is scheduled as the first
  item after 0.1. User decision 2026-10-09: built in the Rust engine, with
  their own evidence when designed; no Custom Tile stability-proof precondition.
  User sketch: size stacked windows slightly shorter and
  draw the tab strip with machinery like the group underlay. Matrix
  inputs: R-INS-02, R-GRP-01..03 (closing the active tab keeps the group
  and activates the next tab, 4/4 references), R-DRAG-01, R-COL-06. Risk:
  mouse interaction with tabs across Linux, Windows and macOS may be
  complex; scope keyboard-first if so.

## Future

Unprioritised ideas; not scheduled.

<a id="pinned-persistent-workspaces"></a>
- Pinned/persistent workspaces (COSMIC pin, Hyprland `persistent` rule, niri
  named workspaces): an empty-surviving workspace with monitor affinity;
  pinned workspaces keep their tiling override across restart; group with
  the shared restart store (G-07, User 2026-10-10). Ordinary unpinned overrides
  still reset; feature design/implementation remains Future scope.

- Windows app-owned fullscreen UX spike: options for exiting/toggling
  fullscreen an application entered itself (user 2026-10-03, later).
  User 2026-10-07 (R-MAX-05, references 8/8 exit without refusal): keep
  the refusal until the spike; explore capturing/retaining window style
  and frame state early (at admission or before fullscreen) so an exit
  can restore without guessing at fullscreen time.
- Windows alternate Game Bar shortcut experiment: keyboard and mouse players
  need a shortcut that opens Game Bar over a fullscreen game (for example
  Xbox voice chat); Start menu and controller access are not enough (user
  2026-10-03, after parity and correctness work).

- Automatic uninstall revert of host settings: parked research after 0.1
  (user 2026-10-10), aiming for "uninstall leaves no changed KDE settings".
  Possibly via package manager hooks; no verified per-user hook exists for
  Nix/Home Manager, KDE Store or distro packages. For 0.1, document pressing
  Revert in Settings before removal in install/uninstall docs, package
  descriptions and a post-removal package note (P1 packaging acceptance).
  [research](changes/archive/host-settings-conflicts.md)

## Pending live checks

Items below retain their stated pending scope; dated user confirmations are
recorded separately from unexercised legs. Reference-WM checks test other compositors.

- Proposal B classification approved exactly User 2026-10-12: original 53
  checks = 39 must-pass / 12 known-issue-allowed / 2 Windows-release. K checks:
  5, 24, 26, 29, 35, 36, 37, 38, 40, 41, 52, 53; W checks: 3, 4; all others M.
  Per-check classifications are in
  [Proposal B](research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2).
  Known-issue-allowed still requires a listed issue and no crash/silent tiling
  stop. Four approved new must-pass checks below give 57 total = 43/12/2;
  D28's native fullscreen-send/game journey stays a must-pass sub-leg of check 1,
  not a separately counted check. D27's real-game active/idle activation check
  is also a must-pass sub-leg of reference-comparison check 1 (User 2026-10-13),
  counted once. Multi-output must-pass checks require a
  two-output setup (VM or hardware) and must pass before 0.1 can ship.
  Batch-1 extras remain outside this inventory; native/live evidence is pending.

  | Check | Approved classification | Action | Expected result |
  | --- | --- | --- | --- |
  | N1 effect-absent | must-pass | Start with effect missing/disabled; disable mid-session; remove optional effect package; exercise failed endpoint with existing resilience check 28. | Tiling keeps working throughout; KWin stable. |
  | N2 external config | must-pass | Change host configuration outside Settings, then reconfigure. | Validated reconcile without unintended writes; Fix/Revert ownership respected; no crash or silent stop. |
  | N3 sleep + gaming | must-pass | Sleep/wake with fullscreen game and tiled session. | Game unimpeded; tiling recovers; no stuck holds. |
  | N4 scaling + overlays | must-pass | Change scale with maximized/fullscreen/fixed-float windows. | Overlays retained; no fullscreen geometry writes; quiet settlement. |

- D27 KDE real-game activation (must-pass, check 1 sub-leg): compare tiler-off/on
  unsolicited hidden-desktop requests during active play and idle/paused play.
  Native input-freshness gating is expected to mark only during active play;
  idle/pager/taskbar behavior matches without-tiler native behavior. No
  OmniTiler guard/interception/reversion/suppression. Verify valid-token switch
  convergence and missing/stale-token attention without interference. Reopen
  only on real active-play switch-away; native result TBD.
- Underlay candidate C visual check before 0.1: after default implementation,
  check `#662a82da` over black, white and grey wallpapers on KDE; Windows
  matching visual and nested-placement checks are in the
  [display/shell checklist](changes/windows-physical-checklist.md#displayshell).
  No visual evidence yet.

- 0.1 triage batch 1 (User 2026-10-10; native KDE checks outside the original
  Proposal B inventory). These are
  verification-only, with D06/D17
  observe-first; record actual outcomes in existing
  [reference scenarios](spec/reference-outcomes.md), leaving unsupported legs TBD.
  - D02 / R-SPC-01: open transient/modeless and modal dialog legs; dialog stays
    floating, tiles untouched. Record native type mapping and parent-focus/
    modal fence results separately; those request outcomes remain unselected.
  - D04 / R-INS-03: open the first ordinary window on an empty shown workspace;
    full work-area tile and real native newcomer focus, not just desired focus.
  - D05 / R-INS-05 (offline-proven, native live pending): focus an ordinary float
    after B was the tiled focus; open a newcomer. Anchor at tiling neighbor B
    using its long edge, not root-wrap; verify real native newcomer activation.
  - D06 / R-OUT-05 (two outputs): occupy both, focus one with pointer on the
    other; open a newcomer. Focused-output arrival and newcomer activation
    required. Adapter routing corrected after native inverse failure;
    opposite-pointer admissions pass both ways. Remaining birth/activation
    timing and real-game variants stay pending in the
    [partial native pass](changes/kde-dual-output-live.md).
  - D07 / R-CLOSE-02: close, refocus a different tile, fresh-reopen; new admission
    at current focus, no old-slot restoration. Exact ratio/order/focus evidence TBD.
  - D08 / R-CLOSE-03: close the sole shown-workspace window; same workspace stays
    shown empty, with no focused client.
  - D11 / R-MOV-01: flat-triple perpendicular move produces V[H[A,C],B], changing
    root orientation rather than refusing; shared R1 is already proven offline.
  - D14 / R-RSZ-02/03: outward edge resize no-ops; nested resize changes adjacent
    shares at nearest matching-edge-axis split, not ancestor shares. KDE pixel
    path exists; Windows keyboard legs await existing handoff item 6.
  - D15 / R-MOU-01: hover between clients retains focus; plain click focuses at
    press. Host-native default path; no focus-policy mutation selected.
  - D17 / R-WS-09 (observe-first): focus B, select WS2 then WS1 with no sends;
    remembered B must return. KDE shell-driven outcome/viewport remains unverified.
  - D18 / R-WS-02: use existing send-back/return journey; verify B inserts after
    remembered A in tall and wide legs. Anchor synthetic proof exists; exact
    after-order/feel pending, no setting.
  - D19 / R-FLT-13: switch away from ordinary float F then return; F hides while
    away, frame retained, remembered focus restored under D17. Desktop membership
    already scopes KDE floats; Windows hide/reveal is already wired.
  - D20 / R-FLT-12: focus each overlapping float in turn; native stacking raises
    the focused float. Existing activation is sourced; relative order TBD.
    No lower-verb check or implementation is required for now.
  - D21 / R-FLT-14: native pointer move/resize preserves free float geometry;
    project resize still refuses. Native journey is verification-only.
  - D24 / R-CLOSE-04: close focused ordinary float; tiles keep their geometry
    and MRU survivor B receives focus. Shared exception-drop already sourced.

- Reference comparison decisions 2026-10-10 (shared/core KDE delivered offline,
     Windows G-05, G-06 and G-37 delivered offline; user-owned checks):
  [Delivery](changes/archive/reference-comparison-implementation.md).
  - G-05: freshly admit (640,0) and (0,480) equal partial-zero hints under
    both predicates: tile for both-axes, untouched automatic float for
    either-axis. Full-zero/unbounded sentinel still tile; fully positive
    fixed still floats; changing settings never reclassifies existing clients.
  - G-06: maximize focused B among tiles, press each directional focus chord:
    no-op until unmaximize/Alt+Tab. In fresh maximized-B legs, directional
    move clears maximize first and then performs the ordinary move; record
    native clear/move timing for horizontal/vertical/partial maximize, synchronous
    and delayed native notifications. Unconfirmed clear must log refusal,
    without a delayed structural move; a later press retries. Repeat with an
    eligible maximized float (clear then half-snap). Existing pointer/fullscreen
    controls unchanged. Windows offline delivery:
    [item 18](changes/archive/windows-maximized-directional-navigation.md);
    Windows ordinary single-output tiled focus fence/clear-then-move and float
    four-half-snaps user-accepted physically 2026-10-12, scoped normal owner,
    Notepad/Paint, base `e506350` plus combined directional/retile/B9 deliveries;
    partial-maximize/exact timing/failure/race/preset/fixed/game/DPI/multi-output
    and KDE native checks remain pending.
  - G-37 (multi-output): visit surviving WS1 then active WS2 on L, with a
    distinct trailing E; migrate WS2 to R. Default shows last remaining E;
    MRU shows eligible WS1. Clear/invalidate history and repeat: MRU falls
    back to last remaining. Check each output's independent history, live
    settings pickup (KCM labels/tooltips/Save/Defaults/Revert), empty migrations
    and unchanged destination insertion. Surviving empty previous qualifies;
    removed/migrated/out-of-scope previous does not, with no recreation.
  - G-D2: send maximized B to another occupied workspace with follow and
    stay; it arrives maximized, unmaximizing there restores its target slot.
    Test numbered/relative and same-/cross-output paths. Windows existing
    maximize policy is verification; KDE native membership/output setters
    leave maximize untouched offline, real compositor carry remains unobserved.
    Fullscreen Windows carry remains item 9/Table A. D28 (b), User 2026-10-12:
    KDE explicit fullscreen send (follow/stay, numbered/relative), including
    a real game, is must-pass for 0.1. Arrival must retain fullscreen; no tiling
    geometry writes to the fullscreen mover beyond native workspace movement,
    no unsolicited fullscreen writes. Record flag, source/target membership,
    view/focus, visible/input state and native geometry/write behavior. If
    KWin cannot carry without dropping/corrupting fullscreen, stop and return
    to the user. Implementation is delivered offline; native/game acceptance
    remains pending. Repeat with a never-tiled born-fullscreen B: no source
    admission or source/target reflow, slotless target hold retained, first
    exit admits at destination. Include a sole held source and empty target.

- KDE two-candidate output move/send/migration (offline delivered 2026-10-09):
  use one wide source and two outputs touching the requested edge. Exhaust local
  moves, then cross with Meta+Shift+arrow; explicit Meta+Ctrl+Alt+arrow sends
  before exhaustion. Place the mover so its centre projects onto each candidate
  in fresh legs, then in a gap to check overlap and left/top ties; rebind stay
  and verify source view/focus. With unequal shared edges, bind whole-workspace
  migration and verify the largest edge wins even when the focused window
  projects onto the smaller candidate. Check work-area placement, chosen-target
  native arrival and no replay, float carry/sticky stay on migration. User-owned;
  native evidence pending; Windows cross-output legs await the item 5 foundation.
  [Record](changes/archive/position-based-output-selection.md),
  [R-MOV-12](spec/reference-outcomes/move.md#r-mov-12-exhausted-up-move-with-two-candidate-outputs-above),
  [R-WS-27](spec/reference-outcomes/workspaces.md#r-ws-27-two-candidate-migration-selection).

### Windows session

- All pending Windows physical/user-run checks, setup requirements and blocked
  legs: [consolidated checklist](changes/windows-physical-checklist.md).
  The dated notes below retain bounded evidence, not duplicate run instructions.
- Scoped native follow-up 2026-10-18: optional border/underlay-off tiling and
  workspace hide/reveal passed a bounded helper leg; this upgrades N1 to partial,
  not complete visual-failure acceptance
  ([evidence](changes/archive/windows-owner-threading-acceptance.md#optional-visuals-disabled-native-follow-up-2026-10-18)).
  External gap-file wake/adoption is partial; chevron response not run without
  a verified target. Tray workspace toggle and same-axis movement lacked a
  verified native trigger; two side-by-side helpers are not nested visual proof.
  [Observations and restoration limits](changes/archive/windows-owner-event-idle.md#scoped-native-follow-up-2026-10-18).
  No physical shortcut, full launcher, game or multi-output pass is inferred.
- Second agent-live sweep 2026-10-17: pager trailing-empty admission/send/close
  rendering passed; selected D16/D23 legs passed after the Windows-local
  [floating fullscreen frame fix](changes/archive/windows-floating-fullscreen-frame-restore.md).
  Calculator supplied a constrained negative control and Paint passed both born
  settings; positive partial-zero/either-axis admission remains untested.
  Ordinary/sticky restart and the protected owned fixed-anchor case passed;
  Paint app-native modal fullscreen passed. Scripted keyboard legs remain
  blocked by the injected-input fence; the complete launcher journey was
  unavailable. No physical/game/multi-output acceptance inferred. Settings/SPI
  restored, ledger clean, all proven owned disposable fixtures closed. Fix
  gates passed 2,469/0/1; CI 38983566504 passed seven streams, with only the
  known unrelated shell API guard failure.
- User physical acceptance 2026-10-12: radius 6/live Apply, ordinary float
  focus/half-snaps, G-06 maximum focus fence/clear then move, R-MAX-03
  retile/restore and both born-setting Paint launches accepted under the
  [dogfood conditions](#open-work). Remaining exact fixtures, fixed-size,
  game/DPI/multi-output legs stay pending; no physical B9 claim added.
- Fullscreen exit: owned Notepad managed-slot return and stop/relaunch with
  fullscreen then exit tiling physically user-accepted 2026-10-12, scoped
  single-output Notepad/Paint, base `e506350` plus pre-retirement changes.
  Born-after-start/app-native/borderless, automatic-retirement/explicit-clear
  recovery and fixed/game/DPI/multi-output remain pending. Each Win+F11 also
  raised the known parked Xbox prompt; eventual fix stays in gaming coexistence.
  [Lifecycle record](changes/archive/windows-fullscreen-exit-acceptance.md).
  Agent-live 2026-10-17: Paint native View-menu fullscreen/Escape exit passed
  overlay hold/release with stable tiled main/sibling frames. No F11 sent; this
  modal-surface leg does not prove exclusive/borderless games or main-window
  fullscreen. Settings/SPI restored and all proven owned fixtures closed.
- Placement and parity/settings/tray remaining acceptance is in the
  [checklist](changes/windows-physical-checklist.md); the original Windows
  dogfood startup anchor remains unknown. Settings Apply/Revert and tray proof
  are delivered, not open implementation; physical containment limitations
  remain disclosed/parked. Native title-bar drag remains; Win+drag is removed.

### Reference WMs (user)

- B9 / R-FLT-06 on COSMIC: tiled workspace, intentionally float B, natively
  maximize it, then toggle float off once. Confirm the sourced
  unmaximize-then-fresh-admit behavior selected 2026-10-08. Record the
  actual result; this is confirmation, not a product or implementation gate.
  [row](spec/reference-outcomes/floating.md)

### Single-output setup

- B1/B2 native-change repeats and held-key autorepeat (Q8 offline evidence
  delivered): repeat Meta+M/native-restore/repress and the inverse ordering;
  repeat sticky-on/native-unstick/repress and sticky-off/native-restick/repress,
  both tiled and floating origins. Hold/release Meta+M and Meta+Shift+G, record
  KGlobalAccel callback count/visible cycling and release/repress behavior.
  Native callback/state signal 2026-10-18: sticky-on/native-unstick/sticky-on
  succeeded after explicit maximize restore/read-back; rapid focus callbacks
  showed busy suppression. The run continued after a maximize false refusal,
  so neither is clean whole-journey acceptance. B1 diagnostic correction is
  parked in Open work; remaining origins/orderings are unverified
  ([coverage matrix](changes/kde-live-acceptance.md#remaining-coverage-and-next-step)).
  Physical repeat behavior remains TBD; these live checks stay user-owned.
  [Original steps](changes/archive/kde-post-windows-followups.md#user-owned-test-system-checks),
  [offline record](changes/archive/kde-repeat-offline-fixtures.md),
  [discriminator](spec/reference-outcomes.md#r-max-04--r-flt-02-kde-held-key-discriminator).
- B9 unfloat while maximized on KDE (offline delivered): intentionally float
  B, natively maximize it, then toggle float off once. Verify maximize clears
  before fresh tiled placement, focus/settlement logs, ordinary nonmax unfloat,
  fixed-client explicit tile override, refused/raced clear and later-press
  recovery. Sampled native callback/state pass 2026-10-18: one maximized-unfloat
  activation cleared maximize and tiled at the first accepted report boundary;
  ordinary nonmax unfloat also passed. Physical delivery, fixed-client/raced-clear
  and later-press permutations remain pending. Fullscreen/sticky gates unchanged.
  [Coverage matrix](changes/kde-live-acceptance.md#remaining-coverage-and-next-step).
  [Record](changes/archive/maximized-intentional-unfloat.md).
- D7 fixed-window tile override across script reload/planner restart (KDE
  offline delivered): Planner-only replacement with the same genuine fixed
  client passed natively 2026-10-18 after explicit workspace re-enable; override
  persistence and reversible clear were verified by store and native read-back
  ([partial record](changes/kde-live-acceptance.md)). A separate churned
  controller-replacement run is not accepted; clean script/controller restart
  remains pending. A subsequent stable stopped-fixture setup also refused its
  first explicit fixed-tile request as `partial-observation` and stopped without
  retry or controller replacement. Exact-site reproduction is parked in Open work;
  the trigger remains unresolved. Successfully tile a fixed client, reload the
  script and restart the planner with the same live client; it remains tiled. Check hint
  loss/predicate changes then workspace enable, re-float/close/complete-inventory
  prune, hidden clients, degraded/old-format/unmatched fallback to untouched
  fixed float, and separate hydrate/persist/clear ACK counts. Ordinary non-fixed
  commands gain no tile persistence; float controls receive zero extra writes.
  [Record](changes/archive/fixed-window-tile-override-restart.md). Remaining native legs pending.
- D5 born-fullscreen first exit incl. games (KDE offline delivered): both
  predicates, current hints/predicate after fullscreen churn, non-fixed exit
  tiles, prior fixed float restores, later toggles retain identity. Verify no
  fullscreen writes and no geometry/focus/stacking/keep-above/maximize-clear
  writes to fixed first exits; correlated classification logs.
  Genuine fixed-client born-first-exit and prior-fixed-float restore sampled
  native callback/state passes 2026-10-18, with zero geometry writes and ordinary
  tiled control unchanged ([partial record](changes/kde-live-acceptance.md)).
  Non-fixed born exit, predicate/current-hint permutations, games and physical
  delivery remain pending. [Record](changes/archive/fixed-size-exit-and-enable.md).
- D6 tiling enable with fixed windows (KDE offline delivered): fixed/non-fixed
  arrivals while floating, formerly ordinary tiles gaining fixed hints,
  predicate changes, explicit user tile overrides, intentional/sticky floats
  and fixed/maximized controls. Fixed automatic floats stay untouched; others
  tile, overrides retain Q3 slots. Verify classification logs and no writes.
  Sampled native callback/state pass 2026-10-18: genuine fixed automatic float
  unchanged with zero writes; ordinary editor control tiled
  ([partial record](changes/kde-live-acceptance.md)). Other permutations above
  remain pending. [Record](changes/archive/fixed-size-exit-and-enable.md).
- KDE full-state churn follow-ups (unclassified, 2026-10-18): three
  `partial-observation` refusals and one float-command log followed by an
  observe-only dispatch occurred during live membership/restart churn. A fixed
  explicit-tile leg displaced an ordinary control; re-float geometry differed
  from the pre-tile frame. D7's stable precondition refusal is tracked in Open
  work; no admission/capacity diagnosis is established. Discriminate the other
  observations separately; the tiled send-back hold has its own parked item.
  Two disposable helpers also exited after their last proven live observation;
  cause unknown. A controller-free supervised lifetime probe did not reproduce
  the exits; retain owned process identity and wait status on subsequent legs.
  No clean controller-restart pass is inferred from this churned run.
- KDE Q3 R-RST-01/03/04 (offline delivered, user decisions 2026-10-08): intentional
  ordinary float plus automatic fixed float and a tile; script stop/start
  preserves ordinary intent/current frame, automatic fixed identity recomputes.
  Move/resize the intentional float while stopped; restart must not write its
  geometry, focus or stacking. Check hidden/minimized and excluded live clients,
  native hidden-to-hidden relocation, verified close during startup and settled
  unfloat. Restart the planner alone; current float observation still recovers
  without rehydration. Corrupt the private store only in a user-owned isolated
  journey: normal degraded reason/count logs, empty adoption, then next settled
  full write replaces state. Store write failure keeps local/native state.
  User logout/login/new KWin must not adopt older intent; no settings/layout
  persistence claim. Physical startup-time sends and separate storage ACK
  tokens also need verification. [Record](changes/archive/kde-intentional-float-restart.md),
  [live guide](live-kwin-testing.md). Sampled native Planner-only restart preserved
  an ordinary float's current frame; script restart, stopped move/resize and the
  remaining storage/lifetime permutations are pending
  ([coverage matrix](changes/kde-live-acceptance.md#remaining-coverage-and-next-step)).
- KDE D1 settings UI (offline delivered): verify predicate labels/tooltips,
  Save/Defaults/Revert and live pickup; one-axis-fixed new clients tile under
  `both-axes-fixed`, float without writes under `either-axis-fixed`. Switch
  values with existing tiles/floats: no reclassification, live tile overrides
  survive; subsequent admissions use the new value. Verify renamed Same-axis
  move labels/tooltips and effective reload logs.
  [Record](changes/archive/admission-and-move-settings.md). User-owned, native TBD.
- KDE Q2 R-SPC-04/06..13 (offline delivered, user decisions 2026-10-08): open a
  fixed-size ordinary client beside a tile; record unchanged incoming frame,
  focus and stacking, including a fixed borderless game. Check single-axis
  clients tile by default and float under `either-axis-fixed`; equal partial-zero
  clients tile by default and float via their nonzero axis under `either-axis-fixed`
  after G-05 delivery (offline complete; exact partial-zero legs are above);
  unset/full-zero/sentinel
  hints do not auto-float. Gain/lose fixed hints after admission without
  changing float identity. Meta+G tiles an automatic float and stays tiled
  through minimize/restore, workspace/output observation and domain re-adoption;
  new client/ref recomputes; owner restart restores matched fixed-window tile
  overrides with recompute fallback (D7 delivered offline). Sticky origin commands retain
  their prior semantics. Fixed born-maximized stays floating beneath native
  maximize; workspace enable keeps fixed automatic floats slotless while
  maximized, native unmaximize stays floating (explicit tile overrides use Q3).
  Born-fullscreen first exit classifies
  as newly admitted (fixed floats with no writes,
  otherwise tiles; prior fixed-floating fullscreen restores floating; no
  writes while fullscreen). Workspace enable checks every window being tiled
  (fixed stays/becomes untouched float including arrivals
  while floating, others tile; user tile overrides stay tiled;
  intentional/sticky keep existing rules).
  Foreground/hidden startup and correlated classification/admission/terminal
  logs need native verification. [Record](changes/archive/fixed-size-admission.md),
  [live guide](live-kwin-testing.md). Only the D5/D6 and Planner-only D7 samples
  above have native callback/state passes; no complete Q2 or game/physical result
  ([coverage matrix](changes/kde-live-acceptance.md#remaining-coverage-and-next-step)).
- KDE R-WS-08/11 item 1: user reported "worked perfectly" on a SINGLE output
  (2026-10-07); single-output native journey confirmed. The report did not
  specify individual edge/>9 cases or which presets were exercised.
  Full-state callback/state repeat 2026-10-18 passed numbered selection and
  previous-workspace return with remembered owned focus and correlated
  command/dispatch/apply evidence; no physical/ring-wrap/>9 claim
  ([coverage matrix](changes/kde-live-acceptance.md#remaining-coverage-and-next-step)).
  Remaining: multi-output per-output/shared scope, hotplug/return (R-WS-15..17),
  and preset-specific checks. Verify Authentic's confirmed Apply/Force clears stock KWin "Switch One
  Desktop to the Left", "Switch One Desktop to the Right", "Switch One
  Desktop Up", and "Switch One Desktop Down" holders; Compatible disables our
  four arrows while letters/Tab work, stock holders remain unchanged on a
  fresh baseline, and Revert restores defaults after earlier clearing.
  [Offline record](changes/archive/kde-workspace-history-ring.md),
  [live guide](live-kwin-testing.md).
- KDE R-WS-01/14 item 2 (offline delivered): physical relative send-follow
  Meta+Ctrl+Shift+H/K/Left/Up previous and J/L/Down/Right next. Check both-end
  wrap, >9 and trailing-empty fill/new spare, including a sole mover emptying
  the source. Rebind numbered/append and relative send-and-stay from their
  empty defaults: source remains selected, source tiled MRU gets focus,
  destination not selected; check empty-source native focus separately.
  Floating-boundary default send now follows; explicit stay keeps source,
  floating frames unchanged and only tiled sides reflow. Verify Authentic
  Apply/Force clears stock "Window One Desktop to the Left/to the Right/Up/Down"
  holders; Compatible disables our four arrows, keeps letters/stay rebindings;
  Revert restores cleared defaults. Multi-output relative-ring scope remains
  user-owned. [Record](changes/archive/kde-workspace-send-follow-stay.md),
  [live guide](live-kwin-testing.md). Bounded callback/state passes 2026-10-18:
  ordinary-float numbered/relative follow/stay across a floating boundary retained
  the float frame and correct view/focus; one two-tiled-domain numbered follow
  moved/focused the subject and reflowed both sides. Stay actions were invoked by
  registered name without rebinding, not physically pressed. Tiled return/stay/
  relative is blocked by the parked hold; append/sole-mover/spare, both-end wrap,
  >9, preset/rebinding and physical delivery remain pending
  ([coverage matrix](changes/kde-live-acceptance.md#remaining-coverage-and-next-step)).
- KDE R-MOV-03 item 3 (offline delivered): toggle Same-axis move in KCM
  between Group with neighbor and Swap with neighbor. In an N-ary group, move an interior
  leaf toward a direct leaf sibling: grouping creates the nested pair, swapping
  exchanges windows with unequal shares traveling and focus on the mover.
  Verify Save live reread without controller/session restart, including
  switching back on an existing tree; no tree rebuild on setting change.
  Check a group neighbor and group-end move retain existing rules.
  [Record](changes/archive/same-axis-move-setting.md),
  [live guide](live-kwin-testing.md). User-owned; no item-3 live result claimed.
- R-DRAG-08 on KDE: with A focused, Meta+left press on unfocused tiled B,
  move, release at A's edge; record whether B is focused at press, during
  the hold, or only after drop (decision: focus at press).
- KDE Q3 born-maximized overlay (`9b612be`) and B6 origin+minimum
  (`cf6ab31`): steps in
  [Q3 record](changes/archive/kde-born-maximized-overlay.md) and
  [B6 record](changes/archive/kde-minimum-origin-placement.md).
- KDE R-MAX-03 Q3 scope: on a floating workspace, first observe maximized A
  without a tile slot beside ordinary B. Toggle the workspace tiled: A stays
  maximized over a reserved slot, B takes its allocated share. Natively
  unmaximize A: it lands in that slot; repeat maximize/unmaximize and confirm
  quiet settlement with no unsolicited toggles or geometry fighting. Repeat
  with a session-restored maximized A at a user-owned session boundary; no loop.
  [Scope record](changes/archive/kde-maximized-floating-retile-overlay.md),
  [live guide](live-kwin-testing.md).

- Tray workspace toggle: floating/tiled toggling confirmed live by the user
  (2026-09-29). Remaining: default change logs `stage=persist
  ... outcome=written`, `plan:config-reloaded stage=default-tiled`, applies
  to new workspaces and to all workspaces after a session restart;
  cross-boundary send is a native move with tiled-side reflow. Red flags:
  geometry changes while floating, menu check disagreeing with KWin, re-tile
  without confirmed release or `native-failed` sends. The same action now
  defaults to Meta+Y (user 2026-10-10); physical delivery is pending below.
  [change](changes/archive/tray-workspace-toggle.md)
- KDE workspace tiling Meta+Y (offline delivered 2026-10-11): physical chord
  and tray checkbox toggle the same current-workspace state, with matching
  snapshot/menu checks and no effect on another workspace or saved default.
  Verify the native/KCM row, Keep/custom/empty preservation, Authentic reset,
  Compatible holder discovery, Apply/Force/Revert isolation and restart
  persistence, including upgrading an existing empty assignment. No automatic
  correction is expected: explicitly stage Authentic and confirm Apply/Force
  to assign the canonical chord to an existing empty/custom row. Other 36
  unbound rows remain unbound. No stock Plasma 6.7.5 holder found offline;
  distro/user assignments can differ. User-owned, no live result claimed.
  [Record](changes/archive/kde-workspace-tiling-shortcut.md),
  [steps](live-shortcut-override-verification.md#pending-workspace-tiling-default-metay).
- Windows workspace tiling Win+Y (offline delivered 2026-10-13): native runtime
  and physical results TBD; [keyboard/shortcuts checklist](changes/windows-physical-checklist.md#single-monitor-keyboardshortcuts),
  with the [source record](changes/archive/windows-workspace-tiling-shortcut.md).
- Active border skips Plasma applet popups (user option A, 2026-09-29;
  KRunner out of scope for now): with an ordinary bordered window active,
  opening the Application Launcher or a tray popup hides the border and logs
  `active-border:visible vis=0 reason=applet-popup appletPopup=1`; closing
  it and refocusing restores `vis=1 reason=eligible appletPopup=0`; ordinary
  dialogs and tool windows keep the border. Red flags: border on the
  launcher, `appletPopup=0` while the launcher is active. If more shell
  surfaces still get the border, tighten further.
  [decision](decisions.md#visuals-border-underlay-and-grouping)
- Host settings Revert and tray conflict indicator: Fix confirmed live by the
  user (2026-09-29). Remaining: Revert removes the local key and native edge
  previews return (one `op=revert setting=<key> outcome=ok reason=ok` line
  per click). Tray (fresh session): with a conflict, a `dialog-warning`
  overlay on the icon and a top "Conflicting KDE settings..." menu row that
  opens Settings; Fix clears both without a tray restart and logs one
  `outcome=conflict-updated conflict=false`; Revert brings them back with one
  `conflict=true`. Red flags: stale overlay/row, repeated conflict lines on
  heartbeats, left-click no longer opening the menu, snapshot-loss
  `NeedsAttention` changed.
  [change](changes/archive/host-settings-conflicts.md)

- Confirmed live by the user (2026-09-29): tiling gap Save re-spaces tiles
  without restart and the unified settings page shows everything (closes the
  P0 settings live-application launch blocker); group underlay survives
  workspace switches and `just dev` restarts, its settings apply live and it
  slides with workspace transitions; a plain drag released off-screen
  retiles correctly with no `snapshot-invalid` (trace `H28tD1`); mid-drag
  workspace send now tiles the mover on its destination (after `8be1a32`).
  Maximise and fullscreen hide the underlay; tray Settings and Desktop
  Effects Configure open the unified page. KWin Scripts Configure is not
  listed under `just dev` (script loaded from the worktree, not installed);
  verify with an installed package when available.
  [settings](changes/archive/unified-settings-page.md)
  [underlay](changes/archive/group-underlay-and-preview-colors.md)
  [mid-drag](changes/archive/mid-drag-destination-recovery.md)

- Quiet refresh logs: compare `stage=refresh terminal=quiet` under normal
  and trace logging; non-quiet terminals stay visible in both. (Unfloat
  placement and the duplicate-implementation unification were confirmed live
  by the user on 2026-09-28.)
  [change](changes/archive/unfloat-admission-axis.md)
- Same-output drag drops + preview overlay: user confirmed live at
  `826b233` (2026-09-28) that a paused drag stays under the pointer, the
  preview renders well and drops place correctly. Remaining: overlay
  independent of the Meta outline, Esc/cancel/no-Finished clearing.
  [change](changes/archive/cross-output-drag-preview.md)
  [policy](changes/archive/drag-drop-reorganisation.md)
- Drop-intent edge drag leftovers: tiled Meta+left snap-back, Esc/zero-move,
  size-increment client, and the `p13` 36 px bottom-edge shortfall.
  [follow-up](changes/archive/passive-press-move-snapback.md)
- Convergence step-1 leftovers: close/reopen focus, transient unreadable
  frames, minimize/fullscreen/maximize restoration, rapid commands (sticky /
  Meta+G cross-workspace already confirmed live).
  [change](changes/archive/observation-convergence.md)
- Resilience batches: empty-login open, resize completion, Meta+M; group
  bridge demarshalling, modifier/render delivery and performance, including
  outline after config change and close/reopen; endpoint
  `stage=failed`/`available=1` transitions; tray status current.
  [change 3](changes/archive/resilience-change-3.md)
  [change 4](changes/archive/resilience-change-4.md)
  [audit](changes/archive/resilience-audit.md)
  [sweep](changes/archive/fail-closed-sweep.md)
  [bridge](changes/archive/active-group-highlight-design.md)
- Tray Active status under `just dev` (worktree tray): healthy shows Active,
  snapshot loss shows NeedsAttention; confirm panel presence and lifecycle
  (no tray launcher existed in the earlier report). Systemd checks need a packaged
  Home Manager install: login starts one tray; killing it restarts it; a
  second invocation exits cleanly; logout stops it without a loop; `just dev`
  preserves the packaged owner; tray diagnostics appear once each in
  `journalctl --user` (native journald submission removed).
  [change](changes/archive/tray-and-restart-followups.md)
  [audit](changes/archive/resilience-audit.md)
- Process-loss and sleep recovery cases on a single-output setup.
  [live plan](changes/archive/recovery-process-sleep-audit.md#live-test-plan)
- Native border delivery and suppression: fresh-session plugin discovery,
  border rendering, and oracle endpoint; fullscreen/any-maximise suppresses
  both outlines; active-border disappearance gets a fresh combined trace if
  it recurs.
  [diagnostics](changes/archive/active-border-visibility-diagnostics.md)
  [decision](decisions.md#visuals-border-underlay-and-grouping)
- Sticky adoption restart/re-enable: intentional sticky identity stays sticky;
  unknown-origin native sticky adoption retains the selected sticky-off rules.
  Verify separately from Windows' pending restart-preservation implementation.
  [decision](decisions.md#window-state-float-sticky-maximize-fullscreen)
- Non-visible workspace tiling: hidden startup/open/move with unchanged
  native focus/desktop and no rendered-state claim.
  [record](changes/background-tiling.md)
- Reliability gates: fullscreen residual-cost gate plus gaming cost baseline,
  work-area projection gate (resolution/scaling/
  work-area change incl. fullscreen isolation and restoration).
  Maximize-admission/session-restore checks are covered by Q3/R-MAX-03 above.
  [investigation](changes/reliability-condition-investigation.md)
- Custom Tile manual runtime check: separately authorized run with exact
  restoration.
  [change](changes/custom-tile-runtime.md)
- Nested placement visual smoke: occupied-leaf whole-rectangle preview and
  nested split placement.
  [change](changes/archive/nested-placement-affordance.md)
- Tray live/release acceptance: KWin-owner snapshots, watcher ordering,
  update/rollback; login/autostart uses the packaged systemd checks above.
  [change](changes/archive/architecture-review-ar13-tray.md)
  [carrier](changes/archive/tray-carrier.md)
- Drag-oracle post-fix proof: rebuild/new session with committed resizes,
  8 px gaps, one planned-applied result, no immediate reconcile.
  [decision](decisions.md#pointer-drag-and-drop)
- Startup adoption: current hybrid is clean/tolerance-valid recursive-cut fit,
  otherwise sequential long-edge seeding (`2c918d3`, 2026-10-03), with no
  centre-split fallback. Restart over previously tiled windows was confirmed
  live 2026-09-29. Remaining: a fresh session after confirmed Planner loss;
  check clean tiles preserve their fit and overlapping windows seed a
  long-edge chain (KDE fixtures added in B5).
  [record](changes/placement-aware-startup-adoption.md)
- KWin controller silent unload (diagnostic only): attribute only with
  before/after `isScriptLoaded`, exact `Script<ID>`, and KWin PID/start
  evidence if it recurs.
  [protocol](live-kwin-testing.md)
- Correlated observability live capture: whole-system lifecycle coverage
  and live capture unproven; per-route offline diagnostics shipped.
  [coverage](changes/archive/observability-coverage-assessment.md)

### Multi-output setup

- Windows item 12 (offline-complete 2026-10-15): remaining scope/history/ring,
  shared startup-anchor, hotplug/R-WS-17 and mixed-DPI physical journeys are in
  the [two-monitor checklist](changes/windows-physical-checklist.md#two-monitor-pc).
  Nearest-survivor discrimination needs three outputs; physical keyboard
  delivery and exact restoration remain pending. [Evidence and tentative boundaries](changes/archive/windows-workspace-scope-hotplug.md),
  [R-WS-17](spec/reference-outcomes/workspaces.md#r-ws-17-previous-around-disconnect-displacement-and-reconnect-return).

- [Checklist and partial native results](changes/kde-dual-output-live.md):
  aligned migration and explicit follow/stay settlement, independent numbered/
  relative/history switching, empty-cell numbered departure, border rendering,
  panel-gap vertical sends and direct-API preview rendering pass sampled legs.
  Authoritative R4 nested-return shape passes; exact named-leaf order does not.
  Deferred trailing-send focus fixed offline (3 regressions), native repeat TBD.
  Forward relative-end wrap needs its spare-maintenance oracle; preset Apply and
  keyboard delivery remain pending. Absolute drag wrappers rejected twice;
  no live drag acceptance. Visible underlay/mapped identity remain pending.
  Protected placement/settings restored, development stopped and transient
  effect unloaded; normal desktop lifecycle churn retained. No full inventory
  gate is claimed. Hotplug/disconnect remains parked.

- Occupied R4 whole-target wrap (User 2026-10-14, offline delivered): start
  `L=H[W1,W2,W3*]`, `R=W4`; move right -> `L=H[W1,W2]`, `R=H[W3,W4]`;
  move left -> `L=H[H[W1,W2],W3]`, `R=W4`. Verify the exact tree, including
  the nested same-axis group, with focus following W3. Authoritative production
  trace proves nested same-axis return and native mover focus. Exact named-leaf
  proof remains pending: trace redacts identities, live peer pair was reversed.
  [Record](changes/kde-dual-output-live.md#whole-target-r4-wrap-decision-2026-10-14).
- KDE Q4 R-WS-12/R-WS-21..26 (offline delivered, user decisions 2026-10-08): rebind
  the four empty-default migration actions; local/global-unique with native
  `perOutputVirtualDesktops` on migrates, off/unreadable/shared refuses before
  writes with exact reason. Select active WS2 with nested unequal-share tree,
  occupied target WS3 and hidden WS4; verify stable ID/order/tiling mode,
  insertion after WS3, WS3 hidden, source last remaining and trailing lifecycle.
  Observe delayed member arrival and both views before focus; empty uses native
  output switch without client focus. Check intentional/automatic float remap
  across work areas/scales, sticky source, minimized/transient clients, member
  removal and partial-write reconciliation. D8 migration with fullscreen/maximized
  members (delivered offline): native move only with no extra
  fullscreen size/position/focus writes; an explicit user move is not unwanted
  interference. Check native fullscreen focus retention (no tiler focus write),
  native re-fit across work areas/scales, preserved maximize slot after unmaximize
  on the target, affected-view overlays and mid-flight mode changes. Previous
  invalidation and reconnect forget only moved ID.
  [Record](changes/archive/kde-whole-workspace-output-migration.md),
  [live guide](live-kwin-testing.md). User-owned; native outcomes remain TBD.
- KDE item 5 R-MOV-08/R-OUT-04 (offline delivered, needs two outputs): stack
  outputs vertically; Meta+Shift+Up/Down crosses after local swap/restructure/
  escape is exhausted, including a sole window. Add a panel work-area gap:
  full rectangles still select the neighbor, tiles stay inside work areas.
  Horizontal Meta+Shift+Left/Right still works, including sole windows;
  no candidate stays put, unreadable topology refuses. Multi-candidate
  position-based selection delivered offline in `a8e32ce`; its dedicated
  two-candidate live check is above. Meta+Ctrl+Alt+arrows/
  HJKL sends before local exhaustion to the target's CURRENT workspace and
  follows the mover; rebind directional output-stay from empty defaults and
  verify source selection/MRU focus and ordinary remembered-leaf admission.
  Check membership-only floating boundaries with only tiled-side reflow.
  [Record](changes/archive/four-direction-output-transfer.md),
   [live guide](live-kwin-testing.md). [Partial native pass](changes/kde-dual-output-live.md):
   output-left/right follow and stay (invokeShortcut) have arrival/focus evidence;
   diagnostic routes reverified live. Sole-source R4 arrived/followed both ways
   after the own-mover observation-gap fix. Exact D13 local-wrap, occupied R4
    named-leaf order, floating-workspace/fixed boundaries and physical
   stay bindings remain pending.
- Core extraction K1 visual policy: confirmed by the user
  (2026-09-30: active border, fullscreen/maximise suppression, group
  underlay, drag preview). Remaining: border, underlay and preview remap
  correctly across outputs with differing scales/origins.
   Destination frame/gap, active border and direct-API preview rendering pass
   sampled native checks in the [record](changes/kde-dual-output-live.md).
   Visible underlay, production drag preview and mapped-binary identity remain
   pending.
  [change](changes/archive/portable-core-k1-visual-policy.md)
- Born-fullscreen dogfood: game fullscreen beside tiles (no gap,
  `initial-fullscreen-held`), exit (fresh tile, `initial-fullscreen-released`),
  re-enter/exit (slot retained); optionally close before first exit.
  [change](changes/archive/born-fullscreen-admission.md)
- Directional + shortcut + convergence re-test: shortcut Apply/Force/Revert per
  [shortcut plan](live-shortcut-override-verification.md) (Lock Session
  relocation, Revert, Phase-2 resize chords insofar as not covered there),
  then the observation-convergence steps 2-3 list (startup fit; open/close
  foreground and hidden; float/sticky/maximize/fullscreen restore; gap
  reload; displaced return; hidden admission; sends incl. rapid;
  refusal/close; R4 occupied/empty/failed; follow-once; phantom revisit;
  correlated logs).
  [contract](changes/shortcut-override.md)
  [failures](changes/archive/multi-output-failures.md)
  [snapshot](changes/archive/multi-output-directional-snapshot.md)
  [convergence](changes/archive/observation-convergence.md)
- Native dual-output mode subcases: maximize/fullscreen enter, five-second
  fullscreen hold and tile-frame restoration passed using native setters and
  independent `maximizeMode`/`fullScreen` readbacks on both scales.
  Overlay-held carry, reserved-slot proof and born-state/game legs remain
  pending. Early JavaScript-expando/synchronous probes are excluded.
  [record](changes/kde-dual-output-live.md)
- Cross-output drops at the pointer: occupied and empty destinations (incl.
  native-output lag), source membership removal, preview/drop agreement at
  group edges, center snap-back with hidden preview, wrong-output and refusal
  recovery.
  [change](changes/archive/cross-output-drag-preview.md)
- KWin restart with surviving Planner: all-new window IDs fresh-adopt
  spatially per output (no ID-order rebuild), tray status recovers; script
  reload with shared IDs keeps groups/splits.
  [change](changes/archive/tray-and-restart-followups.md)
- Reconciliation phase 1 (multi-output setup): hidden and sticky domains stay quiet without
  ping-pong, unreadable outputs imply no departure, stale replies ignored,
  immediate/delayed send/R4 follow and forced refresh.
  [record](changes/archive/robust-difference-reconciliation.md)
  Sampled output sends and corrected sole-source R4 settle/follow both ways;
  dedicated quiet/delayed/rapid/stale-replay cases remain pending.
  [native results](changes/kde-dual-output-live.md)
- Process-loss and sleep recovery cases on a multi-output setup.
  [live plan](changes/archive/recovery-process-sleep-audit.md#live-test-plan)
- Output hotplug displacement/return: disconnect preserves layouts per
  surviving monitor; reconnect returns workspaces with current contents;
  explicit moves stay at their destination.
  Sampled software disable/return preserves client/backing identity and restores
  its output/frame; priority exchange and exact topology restore pass. Inverse
  disable, changed contents/explicit moves and physical unplug remain pending.
  [Native results](changes/kde-dual-output-live.md)
  [investigation](changes/reliability-condition-investigation.md)
- Multi-output workspace anti-oscillation: trailing-empty behavior on a
  multi-output setup. First-output fill/append and emptied-source retirement
  observed with independent second-output view; dedicated close/empty/switch,
  stable-spare repetitions and inverse-output cases remain pending.
  [runbook](live-oscillation-verification.md)
  [native results](changes/kde-dual-output-live.md)
- Native dev lifecycle removal/dogfood coexistence: removal and
  dogfood-coexistence refusal unverified (startup/discovery already
  accepted).
  [record](changes/archive/native-dev-setup-lifecycle.md)
  [host builder](changes/archive/host-matched-native-development-builds.md)

## Open user decisions

- Windows Win+F11/Win+G containment (release gate after multi-output):
  Authentic ownership and disclosed limitation are selected 2026-10-08;
  Compatible/disable/rebind provide Game Bar access. Later test and user review
  owns containment and any host-setting/thread/registry/policy alternative;
  none of those mechanisms is selected or authorized by the ratification.
  [record](changes/windows-gaming-coexistence.md)
- macOS Phase 0 (started 2026-10-14): decide host/floor, Intel support, stable
  signer and Meta mapping. [plan](research/macos-port/plan.md)
- Windows package/update channel: deferred to Windows release planning;
  short research refresh then user decision, most obvious and unsurprising
  for users. Blocks release packaging, not parity. [plan](research/windows-port/plan.md)
- Windows workspace presentation: after multi-output, Windows options
  and recommendation for hidden-workspace Alt+Tab/taskbar semantics plus
  taskbar workspace indicator (parity 10), as a Windows release gate.
  Research delivered with recommendation; selection parked, not selected.
  [note](research/windows-port/alt-tab-hidden-workspaces.md)
- Other unselected behavior: the functional spec's
  [open-decisions index](spec/functional-spec.md#open-index) lists 28 OPEN
  requirement rows; no PROVISIONAL requirement rows remain. KDE fullscreen
  carry is delivered offline under NORMATIVE REQ-MAX-09b (D28 (b), with
  born-fullscreen option (a)); native acceptance remains pending.
  Design questions remain distinct from delivered implementation/live gaps.
- KDE 2026-10-19 decisions awaiting review: entries marked "Tentative (user
  review pending), 2026-10-19" in [decisions](decisions.md) and
  [reference outcomes](spec/reference-outcomes.md) (rapid directional queueing,
  ring-end retention/sticky occupancy, `global-unique` fallback, aligned D5
  refill, automatic-float keep-above rules, placement tree traces).
- KDE 2026-10-19 open questions:
  - Automatic-float keep-above: track project-owned keep-above outside the
    script so it is restored on unload/restart (KWin offers no script unload
    hook)? Blocks acceptance. [record](changes/kde-automatic-float-stacking.md)
  - World-route active-group highlight has no query producer: drive it from
    focus/domain/tree lifecycle events or a new native visibility trigger?
    [record](changes/kde-legacy-route-removal.md)
  - Remaining physical keyboard/drag/visible-underlay checks: trusted manual
    input, or a separately scoped absolute-input fixture? Which T03 action
    should refuse in shared mode? Can mapped-binary identity be proved in a
    fresh compositor session? Which realistic geometry reaches R4
    capacity-float fallback, and which focus result should it publish?
    [record](changes/kde-dual-output-live.md)

## Known issues and risks

- Linux workspace gates fail in `crates/tiler-macos` since upstream macOS
  commits of 2026-10-19: non-macOS `display_cgid_of_pinned` is missing for
  `workspace_send_live_tests.rs`, and `take_trailing_proof` in
  `workspace_select.rs` fails strict dead-code. KDE/core gates pass otherwise.
  Since `0f76c595` CI `rust` and the Windows portable build also fail to
  compile `tiler-macos` (`AX_ATTRIBUTE_UNSUPPORTED` missing from
  `crate::ffi`; no `take_trailing_proof` on `SelectionState`), so `main` CI is
  red independent of later changes.

- Approved 0.1 known limitation D26(b) (User 2026-10-12): intentional-float
  memory does not survive logout/login under unchanged session-scoped
  REQ-RST-01d lifetime; reopened apps have new identities. Restore persisted
  gaps/settings and re-observe host set/membership/maximize; no invented layout
  restore. Verification-only; native dogfood/live checks will assess. Cross-login
  memory is revisited after 0.1 with the shared restart store.
- Borderless-windowed fullscreen heuristic (born-fullscreen option 3):
  watched, conditional (user 2026-10-10). The user watches for borderless
  games during dogfooding; pursue only if games arrive non-fullscreen.
  [change](changes/archive/born-fullscreen-admission.md)
- PID 3568836 SIGABRT (`QKeySequence` D-Bus abort) unattributed; needs
  sender, method, and fault-stack evidence.
  [record](changes/archive/kwin-qkeysequence-dbus-abort.md)
- Whole-snapshot `window-out-of-bounds` rejection removed offline
  (2026-09-29); observed out-of-work-area frames are now tolerated drift.
  Watch for any regression where a genuinely foreign window is tiled.
  [change](changes/archive/mid-drag-workspace-recovery.md)
- Ghostty-class shortfalls (~56 px) have no proven native cause; interim
  acceptance can leave a visible gap or reassert the accepted rect when a
  neighbour drifts.
  [bounds fix](changes/archive/multi-output-domain-bounds.md)
  [drag investigation](changes/window-alignment-drag-investigation.md)
  [record](changes/archive/robust-difference-reconciliation.md)
- Sticky pager appearance and Ghostty fullscreen-to-maximise on workspace
  return remain unconfirmed; no native cause or workaround established.
  [behavior](decisions.md#window-state-float-sticky-maximize-fullscreen)
