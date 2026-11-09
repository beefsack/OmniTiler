# Backlog detail moved 2026-11-01

Historical detail moved from `docs/backlog.md`; current KDE run instructions and v4 coverage are consolidated in the [KDE v4 acceptance checklist](kde-v4-acceptance-checklist.md). Windows and reference-WM scope below is unchanged; historical KDE results are not new v4 passes.

#### Original backlog lines 2183-2203

- Proposal B classification approved exactly User 2026-10-12: original 53
  checks = 39 must-pass / 12 known-issue-allowed / 2 Windows-release. K checks:
  5, 24, 26, 29, 35, 36, 37, 38, 40, 41, 52, 53; W checks: 3, 4; all others M.
  Per-check classifications are in
  [Proposal B](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2).
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

#### Original backlog lines 2204-2210

- D27 KDE real-game activation (must-pass, check 1 sub-leg): compare tiler-off/on
  unsolicited hidden-desktop requests during active play and idle/paused play.
  Native input-freshness gating is expected to mark only during active play;
  idle/pager/taskbar behavior matches without-tiler native behavior. No
  OmniTiler guard/interception/reversion/suppression. Verify valid-token switch
  convergence and missing/stale-token attention without interference. Reopen
  only on real active-play switch-away; native result TBD.
#### Original backlog lines 2211-2216

- Underlay candidate C visual check before 0.1: after default implementation,
  check `#662a82da` over black, white and grey wallpapers on KDE; Windows
  matching visual and nested-placement checks are in the
  [display/shell checklist](windows-physical-checklist.md#displayshell).
  No visual evidence yet.

#### Original backlog lines 2217-2262

- 0.1 triage batch 1 (User 2026-10-10; native KDE checks outside the original
  Proposal B inventory). These are
  verification-only, with D06/D17
  observe-first; record actual outcomes in existing
  [reference scenarios](../spec/reference-outcomes.md), leaving unsupported legs TBD.
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
    [partial native pass](kde-dual-output-live.md).
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

#### Original backlog lines 2263-2307

- Reference comparison decisions 2026-10-10 (shared/core KDE delivered offline,
     Windows G-05, G-06 and G-37 delivered offline; user-owned checks):
  [Delivery](archive/reference-comparison-implementation.md).
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
    [item 18](archive/windows-maximized-directional-navigation.md);
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

#### Original backlog lines 2308-2321

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
  [Record](archive/position-based-output-selection.md),
  [R-MOV-12](../spec/reference-outcomes/move.md#r-mov-12-exhausted-up-move-with-two-candidate-outputs-above),
  [R-WS-27](../spec/reference-outcomes/workspaces.md#r-ws-27-two-candidate-migration-selection).

### Windows session

#### Original backlog lines 2324-2326

- All pending Windows physical/user-run checks, setup requirements and blocked
  legs: [consolidated checklist](windows-physical-checklist.md).
  The dated notes below retain bounded evidence, not duplicate run instructions.
#### Original backlog lines 2327-2335

- Scoped native follow-up 2026-10-18: optional border/underlay-off tiling and
  workspace hide/reveal passed a bounded helper leg; this upgrades N1 to partial,
  not complete visual-failure acceptance
  ([evidence](archive/windows-owner-threading-acceptance.md#optional-visuals-disabled-native-follow-up-2026-10-18)).
  External gap-file wake/adoption is partial; chevron response not run without
  a verified target. Tray workspace toggle and same-axis movement lacked a
  verified native trigger; two side-by-side helpers are not nested visual proof.
  [Observations and restoration limits](archive/windows-owner-event-idle.md#scoped-native-follow-up-2026-10-18).
  No physical shortcut, full launcher, game or multi-output pass is inferred.
#### Original backlog lines 2336-2347

- Second agent-live sweep 2026-10-17: pager trailing-empty admission/send/close
  rendering passed; selected D16/D23 legs passed after the Windows-local
  [floating fullscreen frame fix](archive/windows-floating-fullscreen-frame-restore.md).
  Calculator supplied a constrained negative control and Paint passed both born
  settings; positive partial-zero/either-axis admission remains untested.
  Ordinary/sticky restart and the protected owned fixed-anchor case passed;
  Paint app-native modal fullscreen passed. Scripted keyboard legs remain
  blocked by the injected-input fence; the complete launcher journey was
  unavailable. No physical/game/multi-output acceptance inferred. Settings/SPI
  restored, ledger clean, all proven owned disposable fixtures closed. Fix
  gates passed 2,469/0/1; CI 38983566504 passed seven streams, with only the
  known unrelated shell API guard failure.
#### Original backlog lines 2348-2352

- User physical acceptance 2026-10-12: radius 6/live Apply, ordinary float
  focus/half-snaps, G-06 maximum focus fence/clear then move, R-MAX-03
  retile/restore and both born-setting Paint launches accepted under the
  [dogfood conditions](../backlog.md#open-work). Remaining exact fixtures, fixed-size,
  game/DPI/multi-output legs stay pending; no physical B9 claim added.
#### Original backlog lines 2353-2363

- Fullscreen exit: owned Notepad managed-slot return and stop/relaunch with
  fullscreen then exit tiling physically user-accepted 2026-10-12, scoped
  single-output Notepad/Paint, base `e506350` plus pre-retirement changes.
  Born-after-start/app-native/borderless, automatic-retirement/explicit-clear
  recovery and fixed/game/DPI/multi-output remain pending. Each Win+F11 also
  raised the known parked Xbox prompt; eventual fix stays in gaming coexistence.
  [Lifecycle record](archive/windows-fullscreen-exit-acceptance.md).
  Agent-live 2026-10-17: Paint native View-menu fullscreen/Escape exit passed
  overlay hold/release with stable tiled main/sibling frames. No F11 sent; this
  modal-surface leg does not prove exclusive/borderless games or main-window
  fullscreen. Settings/SPI restored and all proven owned fixtures closed.
#### Original backlog lines 2364-2369

- Placement and parity/settings/tray remaining acceptance is in the
  [checklist](windows-physical-checklist.md); the original Windows
  dogfood startup anchor remains unknown. Settings Apply/Revert and tray proof
  are delivered, not open implementation; physical containment limitations
  remain disclosed/parked. Native title-bar drag remains; Win+drag is removed.

### Reference WMs (user)

#### Original backlog lines 2372-2377

- B9 / R-FLT-06 on COSMIC: tiled workspace, intentionally float B, natively
  maximize it, then toggle float off once. Confirm the sourced
  unmaximize-then-fresh-admit behavior selected 2026-10-08. Record the
  actual result; this is confirmation, not a product or implementation gate.
  [row](../spec/reference-outcomes/floating.md)

### Single-output setup

#### Original backlog lines 2380-2394

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
  ([coverage matrix](kde-live-acceptance.md#remaining-coverage-and-next-step)).
  Physical repeat behavior remains TBD; these live checks stay user-owned.
  [Original steps](archive/kde-post-windows-followups.md#user-owned-test-system-checks),
  [offline record](archive/kde-repeat-offline-fixtures.md),
  [discriminator](../spec/reference-outcomes.md#r-max-04--r-flt-02-kde-held-key-discriminator).
#### Original backlog lines 2395-2404

- B9 unfloat while maximized on KDE (offline delivered): intentionally float
  B, natively maximize it, then toggle float off once. Verify maximize clears
  before fresh tiled placement, focus/settlement logs, ordinary nonmax unfloat,
  fixed-client explicit tile override, refused/raced clear and later-press
  recovery. Sampled native callback/state pass 2026-10-18: one maximized-unfloat
  activation cleared maximize and tiled at the first accepted report boundary;
  ordinary nonmax unfloat also passed. Physical delivery, fixed-client/raced-clear
  and later-press permutations remain pending. Fullscreen/sticky gates unchanged.
  [Coverage matrix](kde-live-acceptance.md#remaining-coverage-and-next-step).
  [Record](archive/maximized-intentional-unfloat.md).
#### Original backlog lines 2405-2420

- D7 fixed-window tile override across script reload/planner restart (KDE
  offline delivered): Planner-only replacement with the same genuine fixed
  client passed natively 2026-10-18 after explicit workspace re-enable; override
  persistence and reversible clear were verified by store and native read-back
  ([partial record](kde-live-acceptance.md)). A separate churned
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
  [Record](archive/fixed-window-tile-override-restart.md). Remaining native legs pending.
#### Original backlog lines 2421-2430

- D5 born-fullscreen first exit incl. games (KDE offline delivered): both
  predicates, current hints/predicate after fullscreen churn, non-fixed exit
  tiles, prior fixed float restores, later toggles retain identity. Verify no
  fullscreen writes and no geometry/focus/stacking/keep-above/maximize-clear
  writes to fixed first exits; correlated classification logs.
  Genuine fixed-client born-first-exit and prior-fixed-float restore sampled
  native callback/state passes 2026-10-18, with zero geometry writes and ordinary
  tiled control unchanged ([partial record](kde-live-acceptance.md)).
  Non-fixed born exit, predicate/current-hint permutations, games and physical
  delivery remain pending. [Record](archive/fixed-size-exit-and-enable.md).
#### Original backlog lines 2431-2439

- D6 tiling enable with fixed windows (KDE offline delivered): fixed/non-fixed
  arrivals while floating, formerly ordinary tiles gaining fixed hints,
  predicate changes, explicit user tile overrides, intentional/sticky floats
  and fixed/maximized controls. Fixed automatic floats stay untouched; others
  tile, overrides retain Q3 slots. Verify classification logs and no writes.
  Sampled native callback/state pass 2026-10-18: genuine fixed automatic float
  unchanged with zero writes; ordinary editor control tiled
  ([partial record](kde-live-acceptance.md)). Other permutations above
  remain pending. [Record](archive/fixed-size-exit-and-enable.md).
#### Original backlog lines 2440-2450

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
#### Original backlog lines 2451-2467

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
  tokens also need verification. [Record](archive/kde-intentional-float-restart.md),
  [live guide](../live-kwin-testing.md). Sampled native Planner-only restart preserved
  an ordinary float's current frame; script restart, stopped move/resize and the
  remaining storage/lifetime permutations are pending
  ([coverage matrix](kde-live-acceptance.md#remaining-coverage-and-next-step)).
#### Original backlog lines 2468-2474

- KDE D1 settings UI (offline delivered): verify predicate labels/tooltips,
  Save/Defaults/Revert and live pickup; one-axis-fixed new clients tile under
  `both-axes-fixed`, float without writes under `either-axis-fixed`. Switch
  values with existing tiles/floats: no reclassification, live tile overrides
  survive; subsequent admissions use the new value. Verify renamed Same-axis
  move labels/tooltips and effective reload logs.
  [Record](archive/admission-and-move-settings.md). User-owned, native TBD.
#### Original backlog lines 2475-2501

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
  logs need native verification. [Record](archive/fixed-size-admission.md),
  [live guide](../live-kwin-testing.md). Only the D5/D6 and Planner-only D7 samples
  above have native callback/state passes; no complete Q2 or game/physical result
  ([coverage matrix](kde-live-acceptance.md#remaining-coverage-and-next-step)).
#### Original backlog lines 2502-2516

- KDE R-WS-08/11 item 1: user reported "worked perfectly" on a SINGLE output
  (2026-10-07); single-output native journey confirmed. The report did not
  specify individual edge/>9 cases or which presets were exercised.
  Full-state callback/state repeat 2026-10-18 passed numbered selection and
  previous-workspace return with remembered owned focus and correlated
  command/dispatch/apply evidence; no physical/ring-wrap/>9 claim
  ([coverage matrix](kde-live-acceptance.md#remaining-coverage-and-next-step)).
  Remaining: multi-output per-output/shared scope, hotplug/return (R-WS-15..17),
  and preset-specific checks. Verify Authentic's confirmed Apply/Force clears stock KWin "Switch One
  Desktop to the Left", "Switch One Desktop to the Right", "Switch One
  Desktop Up", and "Switch One Desktop Down" holders; Compatible disables our
  four arrows while letters/Tab work, stock holders remain unchanged on a
  fresh baseline, and Revert restores defaults after earlier clearing.
  [Offline record](archive/kde-workspace-history-ring.md),
  [live guide](../live-kwin-testing.md).
#### Original backlog lines 2517-2536

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
  user-owned. [Record](archive/kde-workspace-send-follow-stay.md),
  [live guide](../live-kwin-testing.md). Bounded callback/state passes 2026-10-18:
  ordinary-float numbered/relative follow/stay across a floating boundary retained
  the float frame and correct view/focus; one two-tiled-domain numbered follow
  moved/focused the subject and reflowed both sides. Stay actions were invoked by
  registered name without rebinding, not physically pressed. Tiled return/stay/
  relative is blocked by the parked hold; append/sole-mover/spare, both-end wrap,
  >9, preset/rebinding and physical delivery remain pending
  ([coverage matrix](kde-live-acceptance.md#remaining-coverage-and-next-step)).
#### Original backlog lines 2537-2545

- KDE R-MOV-03 item 3 (offline delivered): toggle Same-axis move in KCM
  between Group with neighbor and Swap with neighbor. In an N-ary group, move an interior
  leaf toward a direct leaf sibling: grouping creates the nested pair, swapping
  exchanges windows with unequal shares traveling and focus on the mover.
  Verify Save live reread without controller/session restart, including
  switching back on an existing tree; no tree rebuild on setting change.
  Check a group neighbor and group-end move retain existing rules.
  [Record](archive/same-axis-move-setting.md),
  [live guide](../live-kwin-testing.md). User-owned; no item-3 live result claimed.
#### Original backlog lines 2546-2548

- R-DRAG-08 on KDE: with A focused, Meta+left press on unfocused tiled B,
  move, release at A's edge; record whether B is focused at press, during
  the hold, or only after drop (decision: focus at press).
#### Original backlog lines 2549-2552

- KDE Q3 born-maximized overlay (`9b612be`) and B6 origin+minimum
  (`cf6ab31`): steps in
  [Q3 record](archive/kde-born-maximized-overlay.md) and
  [B6 record](archive/kde-minimum-origin-placement.md).
#### Original backlog lines 2553-2561

- KDE R-MAX-03 Q3 scope: on a floating workspace, first observe maximized A
  without a tile slot beside ordinary B. Toggle the workspace tiled: A stays
  maximized over a reserved slot, B takes its allocated share. Natively
  unmaximize A: it lands in that slot; repeat maximize/unmaximize and confirm
  quiet settlement with no unsolicited toggles or geometry fighting. Repeat
  with a session-restored maximized A at a user-owned session boundary; no loop.
  [Scope record](archive/kde-maximized-floating-retile-overlay.md),
  [live guide](../live-kwin-testing.md).

#### Original backlog lines 2562-2570

- Tray workspace toggle: floating/tiled toggling confirmed live by the user
  (2026-09-29). Remaining: default change logs `stage=persist
  ... outcome=written`, `plan:config-reloaded stage=default-tiled`, applies
  to new workspaces and to all workspaces after a session restart;
  cross-boundary send is a native move with tiled-side reflow. Red flags:
  geometry changes while floating, menu check disagreeing with KWin, re-tile
  without confirmed release or `native-failed` sends. The same action now
  defaults to Meta+Y (user 2026-10-10); physical delivery is pending below.
  [change](archive/tray-workspace-toggle.md)
#### Original backlog lines 2571-2582

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
  [Record](archive/kde-workspace-tiling-shortcut.md),
  [steps](../live-shortcut-override-verification.md#pending-workspace-tiling-default-metay).
#### Original backlog lines 2583-2585

- Windows workspace tiling Win+Y (offline delivered 2026-10-13): native runtime
  and physical results TBD; [keyboard/shortcuts checklist](windows-physical-checklist.md#single-monitor-keyboardshortcuts),
  with the [source record](archive/windows-workspace-tiling-shortcut.md).
#### Original backlog lines 2586-2594

- Active border skips Plasma applet popups (user option A, 2026-09-29;
  KRunner out of scope for now): with an ordinary bordered window active,
  opening the Application Launcher or a tray popup hides the border and logs
  `active-border:visible vis=0 reason=applet-popup appletPopup=1`; closing
  it and refocusing restores `vis=1 reason=eligible appletPopup=0`; ordinary
  dialogs and tool windows keep the border. Red flags: border on the
  launcher, `appletPopup=0` while the launcher is active. If more shell
  surfaces still get the border, tighten further.
  [decision](../decisions/visuals.md#shared-visual-safety)
#### Original backlog lines 2595-2606

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
  [change](archive/host-settings-conflicts.md)

#### Original backlog lines 2607-2621

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
  [settings](archive/unified-settings-page.md)
  [underlay](archive/group-underlay-and-preview-colors.md)
  [mid-drag](archive/mid-drag-destination-recovery.md)

#### Original backlog lines 2622-2626

- Quiet refresh logs: compare `stage=refresh terminal=quiet` under normal
  and trace logging; non-quiet terminals stay visible in both. (Unfloat
  placement and the duplicate-implementation unification were confirmed live
  by the user on 2026-09-28.)
  [change](archive/unfloat-admission-axis.md)
#### Original backlog lines 2627-2632

- Same-output drag drops + preview overlay: user confirmed live at
  `826b233` (2026-09-28) that a paused drag stays under the pointer, the
  preview renders well and drops place correctly. Remaining: overlay
  independent of the Meta outline, Esc/cancel/no-Finished clearing.
  [change](archive/cross-output-drag-preview.md)
  [policy](archive/drag-drop-reorganisation.md)
#### Original backlog lines 2633-2635

- Drop-intent edge drag leftovers: tiled Meta+left snap-back, Esc/zero-move,
  size-increment client, and the `p13` 36 px bottom-edge shortfall.
  [follow-up](archive/passive-press-move-snapback.md)
#### Original backlog lines 2636-2639

- Convergence step-1 leftovers: close/reopen focus, transient unreadable
  frames, minimize/fullscreen/maximize restoration, rapid commands (sticky /
  Meta+G cross-workspace already confirmed live).
  [change](archive/observation-convergence.md)
#### Original backlog lines 2640-2648

- Resilience batches: empty-login open, resize completion, Meta+M; group
  bridge demarshalling, modifier/render delivery and performance, including
  outline after config change and close/reopen; endpoint
  `stage=failed`/`available=1` transitions; tray status current.
  [change 3](archive/resilience-change-3.md)
  [change 4](archive/resilience-change-4.md)
  [audit](archive/resilience-audit.md)
  [sweep](archive/fail-closed-sweep.md)
  [bridge](archive/active-group-highlight-design.md)
#### Original backlog lines 2649-2657

- Tray Active status under `just dev` (worktree tray): healthy shows Active,
  snapshot loss shows NeedsAttention; confirm panel presence and lifecycle
  (no tray launcher existed in the earlier report). Systemd checks need a packaged
  Home Manager install: login starts one tray; killing it restarts it; a
  second invocation exits cleanly; logout stops it without a loop; `just dev`
  preserves the packaged owner; tray diagnostics appear once each in
  `journalctl --user` (native journald submission removed).
  [change](archive/tray-and-restart-followups.md)
  [audit](archive/resilience-audit.md)
#### Original backlog lines 2658-2659

- Process-loss and sleep recovery cases on a single-output setup.
  [live plan](archive/recovery-process-sleep-audit.md#live-test-plan)
#### Original backlog lines 2660-2665

- Native border delivery and suppression: fresh-session plugin discovery,
  border rendering, and oracle endpoint; fullscreen/any-maximise suppresses
  both outlines; active-border disappearance gets a fresh combined trace if
  it recurs.
  [diagnostics](archive/active-border-visibility-diagnostics.md)
  [decision](../decisions/visuals.md#shared-visual-safety)
#### Original backlog lines 2666-2669

- Sticky adoption restart/re-enable: intentional sticky identity stays sticky;
  unknown-origin native sticky adoption retains the selected sticky-off rules.
  Verify separately from Windows' pending restart-preservation implementation.
  [decision](../decisions/window-state.md#fullscreen-and-maximize-overlays)
#### Original backlog lines 2670-2672

- Non-visible workspace tiling: hidden startup/open/move with unchanged
  native focus/desktop and no rendered-state claim.
  [record](background-tiling.md)
#### Original backlog lines 2673-2677

- Reliability gates: fullscreen residual-cost gate plus gaming cost baseline,
  work-area projection gate (resolution/scaling/
  work-area change incl. fullscreen isolation and restoration).
  Maximize-admission/session-restore checks are covered by Q3/R-MAX-03 above.
  [investigation](reliability-condition-investigation.md)
#### Original backlog lines 2678-2680

- Custom Tile manual runtime check: separately authorized run with exact
  restoration.
  [change](custom-tile-runtime.md)
#### Original backlog lines 2681-2683

- Nested placement visual smoke: occupied-leaf whole-rectangle preview and
  nested split placement.
  [change](archive/nested-placement-affordance.md)
#### Original backlog lines 2684-2687

- Tray live/release acceptance: KWin-owner snapshots, watcher ordering,
  update/rollback; login/autostart uses the packaged systemd checks above.
  [change](archive/architecture-review-ar13-tray.md)
  [carrier](archive/tray-carrier.md)
#### Original backlog lines 2688-2690

- Drag-oracle post-fix proof: rebuild/new session with committed resizes,
  8 px gaps, one planned-applied result, no immediate reconcile.
  [decision](../decisions/visuals.md#pointer-and-drag-behavior)
#### Original backlog lines 2691-2697

- Startup adoption: current hybrid is clean/tolerance-valid recursive-cut fit,
  otherwise sequential long-edge seeding (`2c918d3`, 2026-10-03), with no
  centre-split fallback. Restart over previously tiled windows was confirmed
  live 2026-09-29. Remaining: a fresh session after confirmed Planner loss;
  check clean tiles preserve their fit and overlapping windows seed a
  long-edge chain (KDE fixtures added in B5).
  [record](placement-aware-startup-adoption.md)
#### Original backlog lines 2698-2701

- KWin controller silent unload (diagnostic only): attribute only with
  before/after `isScriptLoaded`, exact `Script<ID>`, and KWin PID/start
  evidence if it recurs.
  [protocol](../live-kwin-testing.md)
#### Original backlog lines 2702-2705

- Correlated observability live capture: whole-system lifecycle coverage
  and live capture unproven; per-route offline diagnostics shipped.
  [coverage](archive/observability-coverage-assessment.md)

### Multi-output setup

#### Original backlog lines 2708-2714

- Windows item 12 (offline-complete 2026-10-15): remaining scope/history/ring,
  shared startup-anchor, hotplug/R-WS-17 and mixed-DPI physical journeys are in
  the [two-monitor checklist](windows-physical-checklist.md#two-monitor-pc).
  Nearest-survivor discrimination needs three outputs; physical keyboard
  delivery and exact restoration remain pending. [Evidence and tentative boundaries](archive/windows-workspace-scope-hotplug.md),
  [R-WS-17](../spec/reference-outcomes/workspaces.md#r-ws-17-previous-around-disconnect-displacement-and-reconnect-return).

#### Original backlog lines 2715-2727

- [Checklist and partial native results](kde-dual-output-live.md):
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

#### Original backlog lines 2728-2734

- Occupied R4 whole-target wrap (User 2026-10-14, offline delivered): start
  `L=H[W1,W2,W3*]`, `R=W4`; move right -> `L=H[W1,W2]`, `R=H[W3,W4]`;
  move left -> `L=H[H[W1,W2],W3]`, `R=W4`. Verify the exact tree, including
  the nested same-axis group, with focus following W3. Authoritative production
  trace proves nested same-axis return and native mover focus. Exact named-leaf
  proof remains pending: trace redacts identities, live peer pair was reversed.
  [Record](kde-dual-output-live.md#whole-target-r4-wrap-decision-2026-10-14).
#### Original backlog lines 2735-2752

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
  [Record](archive/kde-whole-workspace-output-migration.md),
  [live guide](../live-kwin-testing.md). User-owned; native outcomes remain TBD.
#### Original backlog lines 2753-2771

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
  [Record](archive/four-direction-output-transfer.md),
   [live guide](../live-kwin-testing.md). [Partial native pass](kde-dual-output-live.md):
   output-left/right follow and stay (invokeShortcut) have arrival/focus evidence;
   diagnostic routes reverified live. Sole-source R4 arrived/followed both ways
   after the own-mover observation-gap fix. Exact D13 local-wrap, occupied R4
    named-leaf order, floating-workspace/fixed boundaries and physical
   stay bindings remain pending.
#### Original backlog lines 2772-2780

- Core extraction K1 visual policy: confirmed by the user
  (2026-09-30: active border, fullscreen/maximise suppression, group
  underlay, drag preview). Remaining: border, underlay and preview remap
  correctly across outputs with differing scales/origins.
   Destination frame/gap, active border and direct-API preview rendering pass
   sampled native checks in the [record](kde-dual-output-live.md).
   Visible underlay, production drag preview and mapped-binary identity remain
   pending.
  [change](archive/portable-core-k1-visual-policy.md)
#### Original backlog lines 2781-2784

- Born-fullscreen dogfood: game fullscreen beside tiles (no gap,
  `initial-fullscreen-held`), exit (fresh tile, `initial-fullscreen-released`),
  re-enter/exit (slot retained); optionally close before first exit.
  [change](archive/born-fullscreen-admission.md)
#### Original backlog lines 2785-2796

- Directional + shortcut + convergence re-test: shortcut Apply/Force/Revert per
  [shortcut plan](../live-shortcut-override-verification.md) (Lock Session
  relocation, Revert, Phase-2 resize chords insofar as not covered there),
  then the observation-convergence steps 2-3 list (startup fit; open/close
  foreground and hidden; float/sticky/maximize/fullscreen restore; gap
  reload; displaced return; hidden admission; sends incl. rapid;
  refusal/close; R4 occupied/empty/failed; follow-once; phantom revisit;
  correlated logs).
  [contract](shortcut-override.md)
  [failures](archive/multi-output-failures.md)
  [snapshot](archive/multi-output-directional-snapshot.md)
  [convergence](archive/observation-convergence.md)
#### Original backlog lines 2797-2802

- Native dual-output mode subcases: maximize/fullscreen enter, five-second
  fullscreen hold and tile-frame restoration passed using native setters and
  independent `maximizeMode`/`fullScreen` readbacks on both scales.
  Overlay-held carry, reserved-slot proof and born-state/game legs remain
  pending. Early JavaScript-expando/synchronous probes are excluded.
  [record](kde-dual-output-live.md)
#### Original backlog lines 2803-2807

- Cross-output drops at the pointer: occupied and empty destinations (incl.
  native-output lag), source membership removal, preview/drop agreement at
  group edges, center snap-back with hidden preview, wrong-output and refusal
  recovery.
  [change](archive/cross-output-drag-preview.md)
#### Original backlog lines 2808-2811

- KWin restart with surviving Planner: all-new window IDs fresh-adopt
  spatially per output (no ID-order rebuild), tray status recovers; script
  reload with shared IDs keeps groups/splits.
  [change](archive/tray-and-restart-followups.md)
#### Original backlog lines 2812-2818

- Reconciliation phase 1 (multi-output setup): hidden and sticky domains stay quiet without
  ping-pong, unreadable outputs imply no departure, stale replies ignored,
  immediate/delayed send/R4 follow and forced refresh.
  [record](archive/robust-difference-reconciliation.md)
  Sampled output sends and corrected sole-source R4 settle/follow both ways;
  dedicated quiet/delayed/rapid/stale-replay cases remain pending.
  [native results](kde-dual-output-live.md)
#### Original backlog lines 2819-2820

- Process-loss and sleep recovery cases on a multi-output setup.
  [live plan](archive/recovery-process-sleep-audit.md#live-test-plan)
#### Original backlog lines 2821-2828

- Output hotplug displacement/return: disconnect preserves layouts per
  surviving monitor; reconnect returns workspaces with current contents;
  explicit moves stay at their destination.
  Sampled software disable/return preserves client/backing identity and restores
  its output/frame; priority exchange and exact topology restore pass. Inverse
  disable, changed contents/explicit moves and physical unplug remain pending.
  [Native results](kde-dual-output-live.md)
  [investigation](reliability-condition-investigation.md)
#### Original backlog lines 2829-2834

- Multi-output workspace anti-oscillation: trailing-empty behavior on a
  multi-output setup. First-output fill/append and emptied-source retirement
  observed with independent second-output view; dedicated close/empty/switch,
  stable-spare repetitions and inverse-output cases remain pending.
  [runbook](../live-oscillation-verification.md)
  [native results](kde-dual-output-live.md)
#### Original backlog lines 2835-2840

- Native dev lifecycle removal/dogfood coexistence: removal and
  dogfood-coexistence refusal unverified (startup/discovery already
  accepted).
  [record](archive/native-dev-setup-lifecycle.md)
  [host builder](archive/host-matched-native-development-builds.md)

Active pending work: [backlog](../backlog.md#pending-live-checks).

## Current v4 acceptance supersession

The dated inventory above is historical: 57 = 43 M / 12 K / 2 W. The current
[KDE v4 checklist](kde-v4-acceptance-checklist.md) applies these approved changes:

- User 2026-11-09, A4/M18: retain all native minimum/origin/convergence journeys
  and must-pass classification; use proportional-allocation/clamp core settlement
  instead of B6 raised-minimum writes and adapter reassertions.
- User 2026-11-09, A1/K35: retire standalone Custom Tile component check; product
  tiling/drag/native safety remains under 25/36/47.
- User 2026-11-09, A2/K24: retain quiet-versus-actionable diagnostics via v4
  normal/trace comparison with named correlated outcomes; retire old log strings.
- User 2026-11-09, A3/K38: retain committed resize/8px-gap/no-extra-reconcile
  physical proof; use v4 command/reply/native convergence as oracle.

Current inventory: 56 = 43 M / 11 K / 2 W; KDE has 53 active checks (43 M / 10 K)
plus retired K35, with Windows 3-4 and COSMIC reference confirmation 5 separate.
No new native pass is established by these acceptance-oracle approvals.
