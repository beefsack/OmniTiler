# KDE v4 acceptance checklist

## Scope, counts and result rules

- Re-baselined 2026-11-09 after KDE phase 2 offline completion. This is the
  single current KDE acceptance queue. Earlier run sheets and dated records
  remain evidence, not parallel checklists or current v3 instructions.
- Historical approved inventory: **57 = 43 M / 12 K / 2 W**. **User 2026-11-09**
  retired standalone K35: current inventory is **56 = 43 M / 11 K / 2 W**.
  KDE has **53 active = 43 M / 10 K**, plus retired K35. Checks 3-4 are
  Windows-only and check 5 is COSMIC reference confirmation, not a product gate.
  [Approval and definitions](../research/release-0.1-triage.md):142-150,545-643;
  [historical inventory](pending-live-checks.md):7-27,220-224.
- **M = 0.1 must-pass**, including N1-N4. **K = known-issue-allowed**, requiring a
  listed issue and no crash or silent tiling stop; K is not a waiver. D27 and
  D28 are must-pass sub-legs of check 1, counted once. Underlay candidate C is a
  separate before-0.1 visual check. Multi-output M legs must pass before 0.1.
- **MAC gate** marks the state-target user check required before starting the
  macOS port ([ordering](../decisions/contract.md)). **Cleared 2026-11-10:** KDE
  phase 2 complete offline and physical ST passed apart from desktop/stacking
  and compositor-boundary recovery, deferred by the user until after macOS start.
  Those sub-legs remain open KDE acceptance. RC instead gates the macOS
  reconnect/rehome slice and 0.1; it is not cleared by ST.
- KDE ST/RC remain prerequisites for macOS slice 5 (state targets)
  and slice 7 (physical reconnect/rehome) acceptance, not port start (amended
  2026-11-10, [ordering](../decisions/contract.md#contract-authority-and-v4-migration)).
  Both checks must pass and resulting core changes be absorbed before either
  slice's acceptance; all existing 0.1 gates remain. Offline completion clears neither.
- Disposition of the 54 historical KDE checks: **53 retained user-run, 0 fully
  closed by scenario/offline coverage, 1 retired (K35)**. Partial coverage is listed
  below; no result from this records work is native acceptance. Six obsolete
  implementation-route groups are retired separately, not six release checks.
- **User 2026-11-09** approved A1-A4 below; **0 acceptance decisions remain open**
  from this re-baseline. Oracle approval is not native execution or acceptance.
- Every unchecked row/sub-leg starts **UNRUN** on v4 unless a bounded result is
  explicitly cited. Record `ID/sub-leg: PASS|FAIL|BLOCKED|NOT RUN; observation;
  evidence filenames`. Never promote a partial sample to a whole-check pass.
  Diagnostic captures use CAPTURED/INCOMPLETE. Missing native fields stay unknown.
- A row's linked source retains the detailed permutations and behavioral oracle.
  Follow the v4 actions here, not removed commands/log names in that source.
  Unsupported outcomes remain TBD in the existing
  [reference discriminators](../spec/reference-outcomes.md), not new policy.

## Setup and evidence for every user-run leg

1. Read the [live KWin guide](../live-kwin-testing.md), especially safety,
   lifecycle, readiness and fixture-first sections. Use latest-stable Plasma.
   Append test workspaces beyond the whole existing roster; alternatively use
   workspaces >n after a fresh occupancy inventory including sticky clients.
   Existing run sheets' no-desktop-churn constraints apply only to those bounded
   sheets, not as an override of the current guide. Normal retirement/appending
   is observed lifecycle churn, not a target to reconstruct.
2. Start from read-only `just dev-status` showing DOWN. Do not displace a
   pre-existing production script without an authoritative restore route. Start
   the [live fixture](../../crates/tiler-live-fixture/README.md), retain its exact
   announcement, use fresh request IDs and join each caller ID/PID/sentinel to
   exactly one native UUID. Use fixture-only scope first; games and other
   application-specific legs are separate user-owned journeys, not a reason to
   broaden fixture management silently.
3. Geometry-only scoped lifecycle: `just dev-on --scope-class OmniTilerTestWindow`,
   then `just dev-off`. Paint/physical dogfood requires the supported native
   effect delivery and a fresh login after rebuild as described in the
   [focus record](v4-phase2-kde-focus-feedback.md):389-403; use
   `just dev trace --scope-class OmniTilerTestWindow`, not detached `dev-on` as
   proof of a loaded effect. Ctrl-C stops the foreground stack. ABI-bound effect
   loading experiments and forced-crash probes use nested KWin only.
4. After every start, retain a pre-start user-journal cursor, verified KWin
   D-Bus-owner PID, then `omnitiler:world:ready`, `world:discovered` and matched
   accepted `dispatch`/`applied`. An inert instance stops the leg; do not restart
   around it. Account for helpers and tear them down in reverse load order.
5. Before/after each action retain native flags, frame, output, membership,
   selected view and actual focus; include named tree/shares where relevant.
   Correlate command/reply and owned native state. Use full `Describe` for
   operation-lane claims; presentation alone cannot prove no pending operations.
   Runner-correlated logs plus native readback suffice for protocol claims; no
   `dbus-monitor`. Keep full combined logs and current-owner journal, including
   the separate unfiltered scripting-error check. No raw key logger.
6. Verify assigned chords first. Physical presses, injected input and callback
   invocation are different evidence. Default state chords: Meta+M maximize,
   Meta+F11 fullscreen, Meta+G float, Meta+Shift+G sticky; Meta+Y workspace tiling.
   Arrows/HJKL focus, Shift moves, Alt grows, Alt+Shift shrinks. Unbound stay and
   migration actions need conflict-free project bindings restored afterwards.
   Observe focus before clicking a terminal. Restore each sub-leg's start state.
7. Stop on crash-class surprises, repeated non-fixture actuation from filter
   failure, an inert/unaccounted lifecycle, or a real product decision. Do not
   retry around a hold or fake an unavailable native operation. No no-argument
   `queryWindowInfo`, broad cleanup or unrelated configuration writes.
8. Teardown with the matching controller stop and exact owned helpers/fixtures;
   verify their absence and undo only recorded test setting/binding changes.
   Restore a display setting changed by a display leg. No routine full baseline
   capture/compare or exact-desktop restoration is required by the current guide.
   Process-loss, store corruption, package removal, sleep/session and physical
   topology legs require their separate user-controlled setup, not the short
   fixture batch. Keep forced-loss/crash-class probes nested.

## Evidence catalog: presence versus recorded execution

- **E0 - phase 2 offline closure**:
  [v3 removal](archive/v4-phase2-kde-v3-removal.md):337-356 reports scoped Rust
  gates, KDE typecheck and 1205 adapter tests passing; both stage5 production
  proofs remain enabled. This records earlier execution, not a new run here.
  It explicitly closes no native/reference acceptance.
- **E1 - live runner**: [implementation](../../scripts/kde-scenarios.mjs):1180
  `scenarioTilingReflow`,1202 `scenarioStateTargets`; five CLI scenarios are
  `tiling-reflow`, `state-targets`, `resize-refusal`, `rapid-burst`, `selection`.
  [Runner record](v4-phase2-kde-scenario-runner.md):174-181,266-278,309-321,
  341-367,415-429 reports respectively 21/16/25/25/29 live checks PASS.
  The input scenarios use injected chords, not physical keys. Selection covers
  one populated appended and one managed-empty original workspace, not arbitrary
  populated-to-populated switches. Rapid-burst is one 60-chord, two-tile burst.
  Resize covers singleton/outer-edge refusal, not nested/min/max exhaustion.
  Current fractional-scale runs did not witness a fractional native frame.
- **E2 - offline producer admission/float/state**:
  [journey tests](../../kwin/tests/world-producer-journeys.test.ts):184
  `admits a newcomer windowAdded onto the tiled row with exact reply frames`,
  219 `reflows survivors on windowRemoved close with exact reply frames`,249
  `floats and unfloats through production callbacks with exact reply frames`,
  334 singleton directional refusal,417 maximize on/off,503 fullscreen on/off,
  571 stacked fullscreen over maximize,641 sibling-close fresh unfloat.
- **E3 - offline producer sticky/selection/scale**: same
  [test file](../../kwin/tests/world-producer-journeys.test.ts):705 sticky on/off,
  807 `restores sticky across an orderly stop/start then sticky-off works`,912
  populated selection with continued tiling,1002 empty selection then admission,
  1060 superseded rapid return,1119 fractional admission,1181 fractional reflow;
  377/1236 are negative oracle controls. E2/E3 code presence is directly locatable;
  [execution record](v4-phase2-kde-scenario-runner.md):431-478 reports 1205/1205
  and typecheck passing. These run the real producer/core with fake native and
  storage/preimage surfaces, not the shared scenario/jq executor. Three-output
  `op-dpv-*`, min/max/nested refusal and 3+ placement oracles remain open
  (:449-461,479-489); remembered-focus arrival half is unobservable (:471-474).
- **E4 - reconnect offline**:
  [producer disconnect tests](../../kwin/tests/world-producer-disconnect.test.ts):83
  `relocates a disconnected output's members onto the survivor with no command
  traffic`,154 renamed-handle return; execution in E2/E3's record. The
  [reconnect record](v4-phase2-kde-physical-reconnect.md):95-213 documents physical
  identity return, minimized-rehome invalidation, overlapping chains and no
  admission focus for rehomed members. [Native triage, 2026-11-09](v4-phase2-kde-physical-reconnect.md#2026-11-09-native-reconnect-triage):
   RC partial, leg 2 failed; summary logs do not prove the live causal divergence.
   The [exact offline leg](v4-phase2-kde-physical-reconnect.md#2026-11-10-offline-leg-2-discriminator)
    reproduced F's non-return after a settled T3 send and now passes with the
    existing separate-layout/current-content rule: F/T2 return, T3 stays, native
    membership converges. Bounded diagnostics and updated traced re-run steps
    are available. Native RC and the user's merge-policy question remain open.
- **E5 - bounded state restart**:
  [state record](v4-phase2-kde-state-targets.md):609-648 reports injected prior-tile
  sticky restart/off PASS with native joins/focus and both readiness gates.
  E10 adds user-reported physical prior-tile AND prior-float orderly restart/off
  PASS, not isolated desktop/stacking preimage recovery. Detailed native-menu
  release, compositor restart recovery and full storage/protocol proof remain
  open. See
  [discriminators](../spec/reference-outcomes.md):104-121.
- **E6 - geometry versus scale change**:
  [scale record](v4-phase2-kde-scaled-metrics.md):87-104 reports current-scale
  native geometry and cross-output convergence PASS. Dynamic scale/mode change
  and border/underlay paint are NOT RUN. KDE wire scale remains 1.
- **E7 - focus/paint limits**:
  [focus record](v4-phase2-kde-focus-feedback.md):209-230,389-454 reports a bounded
  four-switch healthy sample and nested lifetime/frame agreement, not observed
  border paint, dropout rate/duration or mismatch quiet-hide acceptance.
  [Anchored-border discriminator](../spec/reference-outcomes.md):49-53 stays TBD.
- **E8 - historical native samples**:
  [single-output record](kde-live-acceptance.md):114-148,205-267,302-399 and
  [dual-output matrix](kde-dual-output-live.md):360-387 retain callback/native-state
  samples with partial/churn/hold qualifications. These are not v4 physical
  passes. Exact R4 named-leaf order, mapped-effect identity, clean tiled return
  and D7 controller restart cannot be inherited from them. The
  [old physical batch](kde-physical-checklist.md):187-196 remains UNRUN.
- **E9 - retained startup versus superseded B6 oracle**: current
  [world reconciliation](../../crates/tiler-core/src/world_model.rs):1440-1480
  invokes Engine reconcile; [fresh admission](../../crates/tiler-core/src/engine.rs):1800-1931
  retains recursive-cut fit with long-edge seed fallback. That is code presence,
  not an executed v4 Planner-loss journey. In contrast, historical
  [B6](archive/kde-minimum-origin-placement.md):7-11,36-54 raised native extents
  in the deleted plan adapter. The active
  [settlement decision](../decisions/engine.md#core-derived-native-settlement):88
  keeps infeasible minima proportional. Current
  [hint projection](../../crates/tiler-core/src/size_hints.rs):524-543 and
  [world publication](../../crates/tiler-core/src/world_publish.rs):797-805,927-1003
  publish the allocation under core clamp/burst authority, not the old adapter
  raised-minimum target. **User 2026-11-09** approved that replacement as A4 below;
  all native minimum/origin/convergence journeys and M18's must-pass class remain.
- **E10 - physical ST, user report 2026-11-10**:
  [results and limits](v4-phase2-kde-state-targets.md#2026-11-10-physical-state-target-acceptance)
  record physical overlay-stack/base restore, sticky from tile and prior float,
  explicit tile from sticky, and orderly restart/off from both prior modes PASS.
  Native-menu combinations and earlier state-chord spam were reported correct
  in summary, not individual release or counted-delivery proofs. Logs from this
  run were not captured for the record; outcomes are verbal user reports, not
  new E1 runner execution or refreshed E2/E3 offline evidence.
  [Desktop/stacking and KWin-restart recovery](v4-phase2-kde-state-targets.md#deferred-st-recovery)
  remain open, deferred until after macOS start. The
  [sticky-from-tile placement defect](v4-phase2-kde-state-targets.md#sticky-from-tile-placement-defect)
  remains open P1 KDE, not a macOS start gate. Full storage/protocol proof stays open.
- **None** in a coverage cell means no matching v4 acceptance evidence identified
  in this audit, not proof there are no relevant unit tests. Generic admission,
  maximize or disconnect journeys do not prove fixed-client, G-05, G-06, G-37,
  Q4 migration or physical reconnect permutations.

## Release inventory: all 57 checks

Each active KDE row below retains unchecked user-run requirements; bounded
results do not close unreported permutations. K35 is retained as a retired
historical row, not a runnable check. E references are
partial coverage only; source links preserve the original acceptance scope.
Rows 3-5 remain in the 57-count reconciliation but are not KDE work.

| ID/class | v4 coverage / source | User-run action and required observation |
| --- | --- | --- |
| 1 M | E2 adjacent state only; [G set](pending-live-checks.md#original-backlog-lines-2263-2307), [D27](pending-live-checks.md#original-backlog-lines-2204-2210) | [ ] Fresh-admit equal partial-zero hints under both predicates; test G-06 maximize focus fence and clear-then-move, delayed/refused clear and later retry; G-37 migrate with valid/invalid previous history and live setting pickup. Send maximized/fullscreen B follow/stay, numbered/relative, same/cross-output; carry overlay, restore destination slot, no unauthorized fullscreen writes. Repeat never-tiled born-fullscreen, sole held source and empty target. **D28 real game M**; run D27 active-versus-idle leg below. Stop and return a carry failure rather than weaken the gate. |
| 2 M | E4 is disconnect, not position selection; [source](pending-live-checks.md#original-backlog-lines-2308-2321) | [ ] With a wide source touching two candidates, exhaust local moves then cross; explicit-send before exhaustion. Put mover centre over each candidate then in the gap: verify projection/overlap/left-top ties. Migrate across unequal shared edges: largest edge wins. Verify arrival, no replay, float carry, sticky stay and stay-view/focus. Requires three outputs for two candidate destinations. |
| 3 W | [Windows placement](windows-physical-checklist.md) | Windows-only; unchanged, not retired or accepted here. |
| 4 W | [Windows parity/settings/tray](windows-physical-checklist.md) | Windows-only; unchanged, not retired or accepted here. |
| 5 K | [COSMIC confirmation](pending-live-checks.md#original-backlog-lines-2372-2377) | Reference-only: on COSMIC intentionally float B, maximize, unfloat once; record clear-then-fresh-admit. Not a KDE/product gate; remains pending separately. |
| 6 M | E1 state-targets partial; E8 not clean acceptance; E10 physical state transitions/menu/spam user summary PASS, not each native-release permutation or held-key callback counts; [source](pending-live-checks.md#original-backlog-lines-2380-2394) | [ ] Meta+M/native-restore/repress and inverse; sticky native-unstick/restick/repress, tiled and floating origins. Hold/release Meta+M and Meta+Shift+G; record callback count, visible cycling, release/repress. Preserve false-refusal evidence; do not continue around it. |
| 7 M | E2 ordinary unfloat, not raced/fixed clear; [source](pending-live-checks.md#original-backlog-lines-2395-2404) | [ ] Float B, native-maximize, Meta+G once: clear then fresh tile with settled focus/frame. Repeat ordinary nonmax, fixed explicit override, refused/raced clear and later-press recovery. Physical delivery required. |
| 8 M | E5 sticky, not fixed override; [source](pending-live-checks.md#original-backlog-lines-2405-2420) | [ ] Tile genuine fixed client; separately orderly controller and Planner restart with it alive; preserve override. Check hint/predicate changes, hidden clients, re-float/close/prune and degraded/old/unmatched fallback. Correlate hydrate/persist/clear ACKs; ordinary clients gain no tile persistence. Exact-site D7 refusal is diagnostic, not a pass. |
| 9 M | E1 state-targets ordinary exit only; [source](pending-live-checks.md#original-backlog-lines-2421-2430) | [ ] Born-fullscreen first exit: both predicates, changed current hints, non-fixed/fixed/prior-fixed-float and real game. Verify classification, later identity and zero inappropriate geometry/focus/stacking/above/max-clear writes to fixed exits; no writes while fullscreen. |
| 10 M | E2 admission adjacent only; [source](pending-live-checks.md#original-backlog-lines-2431-2439) | [ ] Enable tiling after fixed/nonfixed arrivals, changed hints/predicate, explicit overrides, intentional/sticky floats and maximized controls. Fixed automatic floats stay untouched, other eligible clients tile, overrides retain their base. Verify classification/no-write evidence per permutation. |
| 11 M | E3/E5 bounded sticky restart only; [source](pending-live-checks.md#original-backlog-lines-2451-2467) | [ ] Restart with intentional ordinary float, fixed float and tile; move/resize float while stopped. Preserve current frame without float geometry/focus/stack writes. Check hidden/minimized/excluded, startup close/send, settled unfloat, Planner-only recovery and lifetime non-adoption. Isolated store corruption/write failure only in separate user-controlled setup; capture ACKs, fallback and next-write recovery. |
| 12 M | None; [source](pending-live-checks.md#original-backlog-lines-2468-2474) | [ ] Inspect predicate and Same-axis labels/tooltips; Save/Defaults/Revert/live pickup. New one-axis/partial-zero clients follow predicate, existing clients do not reclassify and overrides survive. Record reload diagnostics and native Settings traversal. |
| 13 M | E2 admission adjacent only; E8 fixed samples partial; [source](pending-live-checks.md#original-backlog-lines-2475-2501) | [ ] Run fixed, one-axis, partial-zero, unset/full-zero/sentinel hints and fixed borderless game admission; preserve fixed incoming frame/focus/stack. Change hints after admission; explicit tile survives minimize/scope/re-adoption but new lifetime recomputes. Include born-max/fullscreen, workspace-enable and foreground/hidden startup permutations. |
| 14 M | E1 selection/E3 partial; [source](pending-live-checks.md#original-backlog-lines-2502-2516) | [ ] Select/previous across each output and shared scope, both ring ends, >9, disconnect/return. Check remembered focus and independent history. Authentic Apply/Force clears named stock arrow holders; Compatible leaves them and disables project arrows; Revert restores cleared defaults. |
| 15 M | E1 selection is not full send matrix; E8 partial; [2026-11-11 command-route recheck](kde-dynamic-workspaces.md#live-recheck-2026-11-11-incomplete) covers sole directional transfer and previous-workspace follow, not physical delivery or full permutations; [source](pending-live-checks.md#original-backlog-lines-2517-2536) | [ ] Physical numbered/relative/append follow and bound stay: tiled/float boundaries, sole source, empty target, spare creation, both-end wrap and >9. Follow mover once; stay keeps source view/MRU or no-client focus. Check preset/rebinding holder isolation and per-output rings. Preserve a tiled-return hold as failure evidence, no retry workaround. |
| 16 M | None; [source](pending-live-checks.md#original-backlog-lines-2537-2545) | [ ] In an unequal-share N-ary tree, switch Same-axis Group/Swap and Save without restart. Move interior leaf: nested pair versus exchange with traveling shares/focus. Change back on existing tree; group-neighbor/end behavior unchanged, no settings-triggered tree rebuild. |
| 17 M | E7 does not prove press focus; [source](pending-live-checks.md#original-backlog-lines-2546-2548) | [ ] Focus A; Meta+left press on unfocused B, hold/move then release at A edge. Record focus at press, hold, drop against selected press-focus behavior. Use release, not Meta+Esc (lock-screen chord collision). |
| 18 M | E2 maximize adjacent only; E9, A4 approved **User 2026-11-09**; [Q3/B6 sources](pending-live-checks.md#original-backlog-lines-2549-2552) | [ ] Fresh born-maximized arrival retains overlay/base slot; unmaximize settles there. Separately admit a minimum-bound newcomer, shrink/grow available area and test oversized sole/secondary-output/hidden clients. Record proportional allocation, native origin/frame, clamping and core quiet settlement, not raised-minimum writes or adapter reassertions. All native minimum/origin/convergence journeys retained; M18 stays must-pass. |
| 19 M | E2 state adjacent only; [source](pending-live-checks.md#original-backlog-lines-2553-2561) | [ ] On floating workspace maximize slotless A beside B, enable tiling: A stays maximized over reserved slot, B gets share. Native unmaximize and repeat quietly; repeat with session-restored maximized A at user-owned session boundary. |
| 20 M | None; [source](pending-live-checks.md#original-backlog-lines-2562-2570) | [ ] Toggle tray workspace state, save default, open a new workspace and orderly restart. Verify effective persisted default, current menu/native agreement, membership-only cross-boundary send and tiled-side reflow; no float geometry writes. Use v4 correlated diagnostics, not old plan log names. Run Meta+Y row below separately. |
| 21 M | E7 integration, not popup paint; [source](pending-live-checks.md#original-backlog-lines-2586-2594) | [ ] With bordered fixture active, open Launcher/tray popup: border hides; close/refocus: returns. Ordinary dialog/tool remains eligible. Record current eligibility diagnostics and paint separately; KRunner is not newly in scope. |
| 22 M | None; [source](pending-live-checks.md#original-backlog-lines-2595-2606) | [ ] Fresh-session conflict: Revert removes owned local key and returns native edge previews; Fix clears tray warning/row live; Revert brings them back once. Left-click/menu and snapshot-loss NeedsAttention unchanged; no heartbeat conflict spam. |
| 23 M | E1 tiling/E6 geometry partial; [source](pending-live-checks.md#original-backlog-lines-2607-2621) | [ ] Recheck live gap/settings/underlay pickup, workspace switch/restart, off-screen and mid-drag send recovery, overlay suppression. Verify Settings launch from tray/effect and KWin Scripts Configure with an installed package, not worktree loading. |
| 24 K | None; A2 approved **User 2026-11-09**; [route removal](kde-legacy-route-removal.md):48-65; [source](pending-live-checks.md#original-backlog-lines-2622-2626) | [ ] Compare v4 normal/trace logging with named correlated quiet versus actionable outcomes. Normal operation stays bounded/quiet; actionable outcomes remain visible and trace adds detail. Old `stage=refresh terminal=quiet` log-string oracle is retired; absence of removed `plan:*` logs alone cannot be a pass. |
| 25 M | No physical preview proof; [source](pending-live-checks.md#original-backlog-lines-2627-2632) | [ ] Same-output title-bar drag, pause, release/drop; check pointer tracking and placement. Preview independent of Meta outline; clear on Esc/cancel/no-Finished where reachable. Use title-bar drag for Esc, not Meta-drag+Esc. |
| 26 K | E1 resize refusal is not drop intent; [source](pending-live-checks.md#original-backlog-lines-2633-2635) | [ ] Check tiled Meta+left snap-back, zero move and title-bar Escape/cancel; size-increment client and p13 bottom-edge shortfall. Measure first divergence; listed issue required if not passing. |
| 27 M | E1 rapid-burst/E2 close partial; [source](pending-live-checks.md#original-backlog-lines-2636-2639) | [ ] Close/refocus/reopen, transient unreadable frames, minimize/fullscreen/maximize restore and rapid commands. FIFO commands/core-reply convergence must settle without stale geometry/focus; record each refusal separately. |
| 28 M | E0 offline, not full resilience; [source](pending-live-checks.md#original-backlog-lines-2640-2648) | [ ] Empty-login open, resize, Meta+M, modifier/render/bridge and outline config/close-reopen. Observe failed/available endpoint transition, uninterrupted tiling and current tray status. Isolate fault injection per guide; N1 handles missing effect. |
| 29 K | None; [source](pending-live-checks.md#original-backlog-lines-2649-2657) | [ ] Check worktree tray panel Active/NeedsAttention. In separate packaged session check one login tray, restart after loss, second invocation exit, logout stop/no loop, dev coexistence and once-only user-journal diagnostics. |
| 30 M | None; [source](pending-live-checks.md#original-backlog-lines-2658-2659) | [ ] On one output follow linked process-loss/sleep plan with fixture tiling. Verify no stuck state/input and recovery/readiness; forced-crash cases nested only. |
| 31 M | E7 partial, not paint; [source](pending-live-checks.md#original-backlog-lines-2660-2665) | [ ] Fresh-session effect discovery/endpoint and border paint; fullscreen/any maximize suppress outlines. Run focus/continuity/mismatch row below; retain combined trace if disappearance recurs. |
| 32 M | E3/E5 injected partial; E10 physical orderly restart/sticky-off from tile AND intentional float user-reported PASS, no retained logs. Unknown-origin native-menu release, full native/storage proof and compositor recovery remain open; [source](pending-live-checks.md#original-backlog-lines-2666-2669) | [x] User-reported sticky on, orderly dev-session restart, sticky off from tile and intentional float (2026-11-10). [ ] Repeat native-menu unknown-origin sticky release with retained home; retain native membership/above and prior-mode evidence. [ ] Compositor-restart recovery separately, deferred until after macOS start. |
| 33 M | E3 selection does not prove hide/reveal; [source](pending-live-checks.md#original-backlog-lines-2670-2672) | [ ] Hidden startup/open/move: native focus/current desktop unchanged. No rendered-state claim from membership exclusion, `hidden=false` or Wayland `visible:null`; unsupported shown observer stays TBD. |
| 34 M | E6 geometry, not cost gate; [source](pending-live-checks.md#original-backlog-lines-2673-2677) | [ ] Run linked fullscreen residual/gaming-cost baseline; change resolution/work area/scale with overlay isolation and restoration. Record native convergence plus cost measurement, not only correctness. |
| 35 K (retired) | A1 approved **User 2026-11-09**; [source](pending-live-checks.md#original-backlog-lines-2678-2680) | RETIRED standalone Custom Tile component check. Product tiling/drag/native safety remains under 25/36/47; do not revive the removed harness or unsafe structural probes. |
| 36 K | None; [source](pending-live-checks.md#original-backlog-lines-2681-2683) | [ ] Stage nested occupied leaf; pointer preview covers whole target rectangle, committed drop creates intended nested split. Retain native tree/frame and visible preview. |
| 37 K | None; [source](pending-live-checks.md#original-backlog-lines-2684-2687) | [ ] Packaged tray: KWin-owner snapshots, watcher ordering, update/rollback, login/autostart; join to check 29's systemd lifecycle without double-counting a result. |
| 38 K | A3 approved **User 2026-11-09**; [route removal](kde-legacy-route-removal.md):48-65; [source](pending-live-checks.md#original-backlog-lines-2688-2690) | [ ] Retain fresh-session committed resize/8px-gap/no-extra-reconcile physical proof, joining v4 command/reply/native convergence. The old one-planned-applied-result oracle is retired, not the physical proof. |
| 39 M | E2 admission, not startup fit; E9 retained algorithm; [source](pending-live-checks.md#original-backlog-lines-2691-2697) | [ ] After confirmed Planner loss in supported isolated journey, fresh-adopt clean tiles and overlapping windows. Clean recursive fit preserved; overlap long-edge seeds, no ID-order/centre-split fallback. Re-prove readiness and native frames. |
| 40 K | Conditional diagnostic; [source](pending-live-checks.md#original-backlog-lines-2698-2701) | [ ] If silent unload recurs, capture before/after isScriptLoaded, exact receipt-bound Script ID and KWin PID/start identity. No guessed attribution and no manufactured unload just to clear this row. |
| 41 K | Offline per-route only; [source](pending-live-checks.md#original-backlog-lines-2702-2705) | [ ] Capture whole-system lifecycle with command/reply/native/renderer joins and timestamps; distinguish missing routes from quiet operation. Never over-capture foreign traffic. |
| 42 M | E4 is reconnect, not Q4 migration; [source](pending-live-checks.md#original-backlog-lines-2735-2752) | [ ] Bind migration actions; migrate nested unequal-share workspace into occupied output. Verify identity/order/mode, insertion, views-before-focus, empty native output switch, delayed arrivals; float/sticky/minimized/transient and fullscreen/maximize carry across work areas/scales. Include partial write/removal, mode change and previous invalidation; no unsolicited fullscreen writes. Use approved common v4 workspace model, not aligned empty-cell oracle. |
| 43 M | E6 sampled transfer/E8 partial; [source](pending-live-checks.md#original-backlog-lines-2753-2771) | [ ] Vertical/horizontal local-first moves then exhausted/sole crossing; panel-gap FULL bounds still choose neighbor, work-area tiles fit. Explicit follow before exhaustion and bound stay retain source MRU/view. Test no candidate, unreadable topology, floating/fixed boundaries; verify D13 local wrap and R4 named tree below. |
| 44 M | E6/E8 destination samples, no full paint/identity; [source](pending-live-checks.md#original-backlog-lines-2772-2780) | [ ] Across differing origins/scales compare frame/gap/border/underlay/production preview remap. Prove mapped effect identity, visible underlay and pointer preview, not only direct-API screenshot or staged binary hash. |
| 45 M | E1 ordinary exit, not real game; [source](pending-live-checks.md#original-backlog-lines-2781-2784) | [ ] Born-fullscreen game beside tiles: slotless hold/no gap; first exit fresh-admits; re-entry/exit retains current slot. Observe optional close-before-exit without inventing admission. |
| 46 M | E1 rapid/selection partial; [source](pending-live-checks.md#original-backlog-lines-2785-2796) | [ ] Shortcut Apply/Force/Revert (including Lock Session relocation and resize chords), then convergence steps 2-3: startup/hidden opens, state restore, gap reload, displaced return, rapid sends/refusal/close, R4 occupied/empty/failed and follow-once/phantom revisit. Preserve correlated logs and physical delivery separately. |
| 47 M | None; [source](pending-live-checks.md#original-backlog-lines-2803-2807) | [ ] Physical title-bar drops into occupied/empty output; held-recross then Esc; verify native-output lag, source removal, group-edge preview/drop agreement, centre snap-back/hidden preview and wrong-output/refusal recovery. Do not retry rejected absolute-drag wrappers. |
| 48 M | E5 orderly restart is not new KWin IDs; [source](pending-live-checks.md#original-backlog-lines-2808-2811) | [ ] At supported user-owned compositor/session boundary with Planner surviving, all-new IDs fresh-adopt spatially per output, tray recovers. Separately script reload with shared IDs retains groups/splits. Forced restart/crash probes nested. |
| 49 M | E8 samples, no v4 whole matrix; [source](pending-live-checks.md#original-backlog-lines-2812-2818) | [ ] Hidden/sticky domains quiet, unreadable output no false departure, stale reply ignored. Compare immediate/delayed/rapid send/R4 follow and forced refresh; capture first unexpected write or view oscillation. |
| 50 M | None; [source](pending-live-checks.md#original-backlog-lines-2819-2820) | [ ] Repeat process-loss/sleep plan with two outputs; independent views/layouts/focus recover with no stranded overlay/input. Forced-crash cases nested; a single-output pass is not this row. |
| 51 M | E4 offline, E8 software disable partial; [source](pending-live-checks.md#original-backlog-lines-2821-2828) | [ ] Run **MAC reconnect gate** below both directions, including changed contents, explicit moves and physical unplug; return then-current contents, not saved snapshot. Three-output nearest-survivor separate, not inferred from two. |
| 52 K | E8 partial; [2026-11-11 recheck](kde-dynamic-workspaces.md#live-recheck-2026-11-11-incomplete): both-output single-create fills and bounded idle PASS; D2 settle FAIL with logical/native view mismatch, full repetition/inverse scope open; [source](pending-live-checks.md#original-backlog-lines-2829-2834) | [ ] Both outputs: close/empty/switch and stable spare repetitions; inverse trailing-empty fill/retire must not oscillate. Use common v4 ring/lifecycle, not aligned-cell behavior. |
| 53 K | E8 startup/discovery only; [source](pending-live-checks.md#original-backlog-lines-2835-2840) | [ ] In dedicated user-controlled delivery journey verify exact native-dev removal and dogfood coexistence refusal. Do not remove/load the active session effect to simulate a routine fixture case. |
| N1 M | No full effect-absence acceptance; [source](pending-live-checks.md#original-backlog-lines-2183-2203) | [ ] Start with missing/disabled effect, disable mid-session, optional package absent and failed endpoint (28): tiling continues, KWin stable. User controls delivery/session; ABI/fault experiments nested. |
| N2 M | None; [source](pending-live-checks.md#original-backlog-lines-2183-2203) | [ ] Change owned host setting outside Settings, reconfigure: validated reconcile without unintended writes, ownership-respecting Fix/Revert, no crash/silent stop. Restore only test-owned change. |
| N3 M | None; [source](pending-live-checks.md#original-backlog-lines-2183-2203) | [ ] Sleep/wake with real fullscreen game and tiles: game unimpeded, tiling resumes, no stuck holds. User-owned session journey, not an injected fixture substitute. |
| N4 M | E6 current scale only; [source](pending-live-checks.md#original-backlog-lines-2183-2203) | [ ] Change display scale with maximized/fullscreen/fixed-float windows: preserve overlays, no fullscreen geometry writes, quiet settlement. Run dynamic-scale row below, restore display setting. |

## Additional KDE acceptance and diagnostic legs

These rows reconcile Pending live checks and Open work beyond the 53 active KDE
entries. Overlap references avoid counting the same evidence twice; they do not
drop the additional permutations. No new release classification is assigned.

| ID / gate / source | Executable user-run steps and expected evidence | Existing v4 coverage / disposition |
| --- | --- | --- |
| ST / **MAC start gate cleared 2026-11-10**, overlaps 6-11,19,32; [current state results](v4-phase2-kde-state-targets.md#2026-11-10-physical-state-target-acceptance) | [x] Physical maximize/fullscreen stack and base restore, sticky on/off from tile and prior float, Meta+G from sticky to current-workspace tile, orderly restart/sticky-off from BOTH prior modes. [x] Offered native maximize/fullscreen/above/all-desktop menu controls handled (user summary only). [ ] Retain detailed native-origin release permutations and full storage/protocol proof. [ ] Desktop/stacking recovery and compositor-boundary recovery remain open, deferred until after macOS start. | **User-run physical ST PASS, 2026-11-10, apart from deferred recovery sub-legs; MAC start gate cleared.** E10 verbal user outcomes, no logs captured for this run; menu releases not reported individually. Supersedes the [2026-11-09 physical failures](v4-phase2-kde-state-targets.md#2026-11-09-physical-state-target-triage) for the reported sequences, not blanket closure of overlapping rows. [Sticky-from-tile placement defect](v4-phase2-kde-state-targets.md#sticky-from-tile-placement-defect) remains open P1 KDE, not a start gate. [Deferred recovery decision and limits](v4-phase2-kde-state-targets.md#deferred-st-recovery) preserve open KDE acceptance. |
| RC / **macOS reconnect/rehome slice and 0.1 gate**, overlaps 51; [native remainder](v4-phase2-kde-physical-reconnect.md#native-only-remainder):214-234 | [ ] Record complete identity rows/bindings for two physical outputs with distinct fixture workspaces; focus removed output, unplug/reconnect, observe displacement/focus/current-content return. Repeat focused survivor (its view/focus retained). During absence explicitly move a fixture out; no pullback. Changed connector only with readable identity, ambiguity holds. Three outputs: nearest survivor by disconnect-time geometry. Supported minimized move: D23 destination restore and return-still-minimized, then restore; no stale source slot. Afterwards ordinary tiling/selection/arrival/focus/paint/state transitions. Restore only test-owned state. | **PARTIAL, 2026-11-09; leg 2 FAIL.** [User-run real-window result](v4-phase2-kde-physical-reconnect.md#2026-11-09-native-reconnect-triage): leg 1 return reported PASS, unplug merge/focus qualified; leg 2 survivor focus retained, F failed to return and explicitly moved T3 was pulled back; leg 3 ordinary tiling reported PASS only. Identity/binding and causal payloads absent from summary logs; remaining variants NOT RUN. RC leg-2 fix and merge decision must land before the macOS reconnect/rehome slice and 0.1, not port start. Unsupported native minimized route remains TBD. |
| SC / N4 overlap; [scale slice](v4-phase2-kde-scaled-metrics.md):80-104 | [ ] Two tiles and loaded-effect observation; change scale once through display UI. Verify actual notification, logical gaps/styles, native frame/allocation convergence and border/underlay repaint. Repeat N4 overlay/fixed-float arms and work-area/mode changes in 34. Restore setting; fractional readback is a separate witness, not guaranteed by a fractional output scale. | E6 current geometry/crossing PASS, dynamic change/paint open. No new resize-minima policy selected. |
| FB / 31,44 overlap; [focus slice](v4-phase2-kde-focus-feedback.md):389-454, [discriminator](../spec/reference-outcomes.md#anchored-border-in-flight-replacement) | [ ] With effect loaded, alternate native focus including formerly non-active fixture; minimize/restore, maximize/unmaximize; physically drag and edge/corner resize, release/cancel. Correlate native frame motion, world traffic, chain/witness joins and paint: follow without broker round trip, hide/re-arm as eligible; record unexpected hidden interval count/duration separately from expected vetoes. Version-mismatch all-surface quiet-hide with tiling continuing requires an isolated supported mismatched-presentation probe; if unavailable, BLOCKED, do not fabricate a command or load ABI experiment in host session. | E7 nested wire/frame agreement only. Full K5/R20, mismatch and real gesture dropout evidence open; old batch's drag-border skip is not a waiver. |
| RZ / 6,27 overlap; [resize diagnosis](kde-resize-refusal-diagnostics.md):69-96, [discriminator](../spec/reference-outcomes.md#keyboard-resize-refusal-discriminator) | [ ] Three tiles, select populated workspace, physically grow right repeatedly, shrink right, shrink/grow left; repeat nested, minimum/maximum and immediately after populated switch. Ordinary first step is 12 logical px; join refusal diagnostic to command/focus/hints/frame. Distinguish legitimate edge/exhaustion from defect; retain full combined log. | E1 singleton/outer-edge injected PASS; offline nested/min/max and real keys remain open. User-facing refusal feedback remains undecided. |
| DG / **0.1 M check 1 sub-leg**; [D27](pending-live-checks.md):30-36 | [ ] Compare tiler off/on unsolicited hidden-desktop activation requests during real active play and idle/paused play. Record valid-token switch convergence and missing/stale-token attention. Native input freshness should gate active play, idle/pager/taskbar follows native baseline. No OmniTiler suppression/reversion/guard. Reopen only for real active-play switch-away. | No real-game native result; TBD. D28 fullscreen-send is row 1, not a new counted check. |
| UC / **before 0.1**; [candidate C](pending-live-checks.md):39-43 | [ ] Display `#662a82da` underlay on black, white and grey KDE wallpapers; compare visible contrast/color and restore wallpaper. Record visual judgment separately from geometry/eligibility. | No visual evidence. Windows visual/nested legs remain with Windows. |
| BF / batch 1 outside 57; [source](pending-live-checks.md):47-91, [delivery](archive/triage-batch1-kde-shared.md) | [ ] D02 dialog/modal floats without disturbing tiles; native type/focus fence undecided. D04 empty first admit full work area/native focus. D05 float-focus then newcomer anchors at former tiled B's long edge. D06 focused output vs opposite pointer, birth/activation timing and game arms; compare against current cursor-first decision, not an obsolete routing oracle. D07 close/refocus/fresh reopen new admission; D08 sole close retains empty shown workspace/no focus. D11 flat triple perpendicular move `V[H[A,C],B]`; D14 outer-edge no-op and nested nearest matching-edge-axis split shares, not ancestor shares. D15 hover retains focus, plain click focuses at press. D17 shell-driven return focus/viewport. D18 send-back/return inserts B after remembered A, tall and wide. D19 switch-away/back intentional float retains frame/remembered focus, native hide observation separate. D20 focus each overlapping float: native raises it; relative order TBD, no lower-verb requirement. D21 native pointer move/resize preserves free-float geometry, project resize refuses. D24 close focused float keeps tiles, MRU survivor focus. | E2/E3 bounded admission/float/selection adjacent; no full native batch closure. D06/D17 observe-first; unsupported outcomes TBD. |
| PX / cursor/empty/no-focus; [Open work](../backlog.md):68-72 | [ ] Cursor-first physical hover-follow across outputs, explicit-send sampling separately; empty-output close/hide/switch/send discriminators; native-minimize sole fixture, prove no managed focus, test no-neighbor and adjacent edge focus then restore without tiler focus write. [Existing O4](kde-physical-checklist.md):165 supplies bounded setup; unreachable routing is BLOCKED, not a policy result. | Core v4 targeting/empty-domain behavior is not native physical proof; COSMIC immediate-send remains TBD. E1 stress does not close no-focus delivery. |
| UF / P0 unfloat; [source](archive/kde-unfloat-immediate-retile.md), [S1](kde-physical-checklist.md):60-65 | [ ] Two tiles, B focused: Meta+G on/off without terminal/focus change, repeat. B fresh-retiles immediately, A reflows; a later focus-triggered snap cannot turn failure into PASS. | E2 offline and E1 injected float partial; user recheck retained separately from maximized-unfloat 7. |
| AS / P0 above ownership; [source](kde-automatic-float-stacking.md) | [ ] First fixture dialog/fixed client, then application-specific Steam/modal/game only after fixture evidence. Compare project-owned versus pre-existing above, orderly unload/restart; project-owned preimage restores, pre-existing above untouched. Capture native state before/after, no acceptance from a raised appearance. | Old local automatic-above mechanism removed, not the R12 ownership requirement. Fresh ownership experiment and physical application legs remain open; blocked preconditions remain BLOCKED. |
| RI / P0 rapid; [source](kde-rapid-input-latest-world.md), [S5/D2](kde-physical-checklist.md):88-93,143-148 | [ ] Physical four left/right move pairs (8 moves), then 16 mixed move/focus presses from known A/B tree; count intended/delivered FIFO commands and final focus/order, latency and effect-loaded paint. Then nested/float/cross-output arms; empty-target crossings only with proven target. Stop first failed burst, no extra spam. | E1 single injected 60-chord PASS. Effect-loaded flicker, physical delivery/feel remain open; tentative ordering policy not approved by a passing run. |
| OT / diagnostic capture; [source](kde-open-animation-timing.md), [procedure](../dev-loop.md#user-run-single-window-capture) | [ ] After dialog/above prerequisite, exactly one ordinary fixture admission with trace journal/video per existing procedure. Record first-visible/open-effect-end/jump offsets, missing events and clock anomalies. | Capture requested; no zero-jump fix established. CAPTURED/INCOMPLETE, not product PASS. |
| CH / churn/D7; [source](pending-live-checks.md):300-310, [exact-site record](archive/kde-d7-tile-precondition.md) | [ ] Reproduce separately on v4 stable setup; retain complete failing operation/report, refusal/observe-only dispatch and first displacement/re-float frame mismatch. Keep helper process identity/wait status. Stop at divergence; do not select a fix or retry through a hold. | Historical trigger unclassified; E8 churn not controller-restart acceptance. |
| SY / standalone Meta+Y, overlaps 20; [source](pending-live-checks.md):453-464 | [ ] Physical Meta+Y and tray checkbox toggle same current workspace, snapshot/menu agree; other workspace/default unchanged. KCM native row: Keep/custom/empty, explicit Authentic Apply/Force canonical reset, Compatible holder discovery and Revert isolation. Restart/upgrade empty assignment persists; other 36 unbound rows remain unbound. | Offline delivered, physical/KCM acceptance open. No automatic correction or invented stock holder. |
| UI / Settings/presets; [sources](../backlog.md):93-94,142, [conflicts](kde-shortcut-conflicts.md) | [ ] Native keyboard traversal/buddies/visual UX and transient-operation non-invocation, live schema/catalog tabs; conflict/preset Apply/Force/Revert with exact project tuples. Run checks 12/14/15/22/46 and standalone SY; unavailable control is BLOCKED, not config-file substitution. | Native recheck pending; retain implementation gaps separately, no assumption that every catalog/UI route is ported. |
| MD / dual-output modes, overlaps 42-45; [source](pending-live-checks.md):741-746 | [ ] On both scales native maximize/fullscreen entry, five-second hold, exit restores tile. Add overlay-held carries, reserved-slot proof, born-state and game arms. For each supported workspace mode, orderly restart to consume saved token, verify independent versus shared views and saved labels/warning (old D4 procedure only where consistent with common v4 model). | E8 setter/readback samples partial, no expando/same-stack proof. Mode/overlay/per-output acceptance retained, aligned-cell assertions retired below. |
| R4 / dual-output umbrella; [source](pending-live-checks.md):643-666, [matrix](kde-dual-output-live.md#must-pass-checklist) | [ ] Exact named tree: `L=H[W1,W2,W3*], R=W4`; right -> `L=H[W1,W2], R=H[W3,W4]`; left -> `L=H[H[W1,W2],W3], R=W4`, focus W3. Also deferred trailing-send native repeat, relative-end spare oracle, preset/keys, physical drag, underlay/mapped identity and current contents after hotplug. | E8 nested shape only, redacted/reversed leaves do not prove exact order. Consolidates backlog P0 dual-output; 42-52 keep individual requirements. |
| F1 / client self-resize; [source](archive/kde-client-frame-drift.md), [best-effort admission](archive/best-effort-admission.md), [discriminator](../spec/reference-outcomes.md#client-driven-tiled-geometry) | [ ] Native Wayland and X11 tiled clients self-resize once and persistently; capture native frame/attempt/reconciliation/visual safety. Separately exact script-restart best-effort admission and native minimum/origin/convergence dogfood using A4's proportional-allocation/clamp core-settlement oracle (**User 2026-11-09**), not old B6 raised extents. | Acceptance open; defer-versus-prompt policy stays parked, host-settled rule requires design approval before code. |
| TP / topology/refusal; [sources](../backlog.md):100,103, [migration](archive/windows-full-world-state-replies.md) | [ ] Compare reported FULL output bounds versus work area, split-edge and ordinary two-output actions; physical migration/T2 crossing refusal retains source/target and exact reason. Explicit-target contract gap remains parked. | E6 sampled geometry insufficient for all topology/refusal legs. Windows follows its port, not closed here. |
| DV / foreground entry; [source](dev-entry-point.md), [Open work](../backlog.md):122 | [ ] Scoped foreground dev/tray Stop UI and Ctrl-C: status DOWN and exact owned tray/controller absent. Verify physical/whole-desktop visual behavior only in separate user-owned dogfood session. | Implementation not whole-desktop acceptance; detached geometry start is not native paint proof. |
| RT / reference additions, overlaps BF; [sources](../backlog.md):118-119,124,140, [D units](archive/kde-fullscreen-activation-and-send.md) | [ ] Complete D03 chained admissions, D09/D10/D12 exact trees, D16 follow/stay/sole/empty float matrix, D22 unequal survivor shares, D23 minimize MRU/rejoin/sole retention, D25 orderly restart/D26 settings; use retained T/S/O recipes in old batch only as bounded procedures. Also KDE post-Windows followups and reference-consensus actions; unselected bindings stay decisions. | E2/E3/E5 cover bounded portions, no blanket D-unit completion. Current R-MAX-10/11 replace old fullscreen float/maximize refusal oracles. |

## Open-work engineering boundaries (not physical passes)

- [Open work](../backlog.md):27,33,43-65,76-80,102,107-112,127-128,
  143,157-165 retains engineering, design, delivery and functional-spec work.
  It is not silently retired by checklist consolidation. In particular:

| Source item | Re-baselined disposition / acceptance link |
| --- | --- |
| Phase 1 corpus/eight OPEN scenario gates, K5/M7, storage/lifetime | Keep shared executor/conformance and witness work; E2/E3 are not shared jq execution. FB/11/32 add KDE native evidence only. |
| Unreadable ownership, visibility-ready/tab-close source work | Keep core/native hide-readiness gap; 33/PX cannot turn membership exclusion into rendered visibility. |
| Reply outcome / handshake-settings-report / decoration input | E0 removes legacy lanes, not every pending validation; keep KWin priority exposure, incarnation cleanup, pager-click/revision and coverage audit until separately verified. |
| Offline producer follow-ups | E2/E3 exclusions remain explicitly scheduled; no physical run substitutes for absent offline three-output/min-max/nested/3+ oracles. |
| R5/R6/F21/R10/R11/R12/R14/R18/F26/F28/F29/R20/R21 | Keep contract/typing/quantization/identity/preimage/state/scripting/diagnostics/conformance/hardening and effect Rust work. SC/RC/AS/FB supply acceptance, not implementation/design completion. |
| Float drag to empty output null geometry; report cost; intent lifetime; interface review | Known defect/performance/ownership work remains open; 47/F1/11/32 are evidence legs, not fixes. |
| KDE v4 completion, Ubuntu, release/package/external delivery | Retain native and release-specific gates; 23/29/37/53 are not independent clean-install or distro acceptance. |
| Settings preset evidence, contingent D fixes, shared host-settled geometry | Keep missing Tier 1 values TBD, contingent fixes behind recorded return conditions; learned-size-limits remains rejected; host-settled rule requires approval. |
| Cleanup/status documentation, group trigger, parked underlay C-stage, functional-spec OPEN rows | Separate work, not proof of obsolescence. Candidate C color UC is NOT parked underlay trigger stage C. No deletion from this inventory pass. |
| Known-issue watches | Preserve D26(b) cross-login intent limitation, borderless-game heuristic, foreign-window bounds regression, Ghostty shortfalls, sticky pager/fullscreen-to-maximize and unverified portable gates; watch is not acceptance. |

## Retired routes and retained product acceptance

| Obsolete route group | Evidence for retirement | What remains |
| --- | --- | --- |
| Aligned per-output lifecycle / D4-D5 empty cells | [Reconnect record](v4-phase2-kde-physical-reconnect.md):52-75: approved common v4 model replaced aligned lifecycle | 14/15/42/51/52/MD/RC retain native history/migration/reconnect/mode behavior; old aligned-cell oracle is not run. |
| `live-test.sh` / inert provenance carrier | [Live guide](../live-kwin-testing.md):107-117; [runner record](v4-phase2-kde-scenario-runner.md):79-128 | E1 or receipt-bound manual lifecycle plus evaluated readiness; old flags prove nothing. |
| Local automatic stacking, result-driven state settlement and obsolete border/report helpers | [State slice](v4-phase2-kde-state-targets.md):294-307; [focus slice](v4-phase2-kde-focus-feedback.md):354-383; [scale slice](v4-phase2-kde-scaled-metrics.md):70-75 | AS/ST/FB retain visible behavior, native preimages and core-target settlement; old local log signatures are not criteria. |
| Plan/workspace-send / DescribePlan route | [Removal outcome](kde-legacy-route-removal.md):48-101 reports reviewed removal and gates | v4 commands/replies plus native joins replace removed route; unresolved group-highlight trigger remains TBD (:103-118). |
| Custom-tile acceptance harness / fixture | [Live guide](../live-kwin-testing.md):156-171; [removal](kde-legacy-route-removal.md):48-65 | **User 2026-11-09**, A1: standalone K35 retired; product safety remains under 25/36/47 and safety bans stand. |
| v3 codec/report/stress lanes / moving-border prototype | [Offline closure](archive/v4-phase2-kde-v3-removal.md):240-356 | E0-E7 supported v4 coverage; immutable fixtures/stage5 proofs retained. No native check is closed by culling dead code. |

## Approved acceptance decisions

All four decisions below are applied. **User 2026-11-09** retired only K35 and
approved the three replacement oracles; the other 53 KDE checks retain their
classes and user-run requirements. The historical 57 rows remain traceable;
the current inventory is 56, including 53 active KDE checks.

| ID / affected check | Approved decision |
| --- | --- |
| A4 / M18 B6 minimum placement | **User 2026-11-09**: use current v4 proportional-allocation/clamp core-settlement oracle (E9), replacing B6 raised-minimum writes and adapter reassertions. All native minimum/origin/convergence journeys retained; M18 stays must-pass. |
| A1 / K35 Custom Tile | **User 2026-11-09**: retire standalone Custom Tile component check. Product tiling/drag/native safety remains under 25/36/47; no removed harness revival. |
| A2 / K24 quiet refresh | **User 2026-11-09**: retain quiet-versus-actionable diagnostic requirement; accept v4 normal/trace comparison with named correlated outcomes; retire old log-string oracle. |
| A3 / K38 drag oracle | **User 2026-11-09**: retain committed resize/8px-gap/no-extra-reconcile physical proof; accept v4 command/reply/native convergence as oracle. |

- No M-check is retired or downgraded; partial producer/injected samples cannot
  replace physical/game/multi-output acceptance. K24 and K38 remain active K checks.
- Missing menu/probe/native capability is BLOCKED/TBD, not acceptance by absence.
  Record ST and RC results per sub-leg: ST cleared macOS start on 2026-11-10
  apart from user-deferred desktop/stacking and compositor-boundary recovery,
  which remain open KDE acceptance; RC still gates its reconnect/rehome slice
  and 0.1.
