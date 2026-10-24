# Reliability Condition Investigation

- Status: historical pre-v2 evidence. The Plan-route file citations below
  refer to removed `kwin/src/plan-adapter*.ts` modules; production uses the
  world route per the [core
  closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24)
  and [world adapter guide](../porting/world-adapter-guide.md). The
  per-slice live gates below remain pending; this is not the current contract.

## Scope

- Static diagnosis of the five reliability conditions in `VISION.md:30-48`.
- No live KWin, Plasma, D-Bus, suspend, output, configuration, or lifecycle
  action occurred. The active border and tray retain only their recorded manual
  confirmation; this record makes no further runtime claim.

## Verdicts

### Output Hotplug - OFFLINE PIECES COVERED, LIVE GATE PENDING

- The Rust planner retains `BTreeMap<DomainKey, Session>` and relocates a
  retained domain by workspace id when the same workspace is observed on a
  different output: `src/session.rs:relocate_domain` is atomic on any
  validation failure and preserves exception classes, gap, revision,
  tree/shares, and focus while updating domain bounds/gap plus window and
  exception homing; `src/planner_protocol.rs:try_relocate_for_target`
  validates workspace-send pending state, request outer gap, target collision,
  and source uniqueness/usability before any source mutation, honors normal
  retained capacity constraints without all-domain clearing, and restores the
  source on any rejected follow-up in the retained and reconcile paths, so
  displaced layouts converge through the normal admit/remove/reconcile flow
  with CURRENT contents. Offline tests cover same-workspace survivor reuse,
  current-contents edits, no-merge into the survivor tree, target
  collision/outer-gap/ambiguous-source/pending refusal atomicity, and
  revision/gap/tree/focus plus float exception class preservation. The
  standalone workspace-send cross-output refusal is unchanged.
- The TypeScript native layer owns session-local displacement identity and
  actuation: `kwin/src/workspace-native.ts:chooseDisplacedDestination` picks
  the nearest survivor only when the native handling source exposes
  removed-output geometry, else the live primary, else existing output
  ordering, with no historical geometry tracking; the current adapter does
  not retain removed geometry so it falls back to primary/ordering and never
  uses post-disconnect window frame geometry as a proxy;
  `reconcileOutputDisplacement` captures a handling-time topology snapshot
  (previous mapping plus last-known window-output membership plus
  last-visible, keyed by stable output) before destructive cleanup, primes
  visible/membership tracking at enable, moves every associated workspace of a
  removed output as a unit (including background occupied workspaces, never
  bulk-assigned to the primary), defers safely with no orphan/rehome/merge
  when no survivor exists, keeps workspace ids unique across logical lists,
  returns by exact stable-key reappearance only (tuple replacements never
  falsely return; unreturnable associations expire with a bounded log and no
  invented return), and returns each workspace by id with current contents on
  reconnect. Visibility follows the active window; otherwise the surviving
  view/focus is preserved. Offline tests cover multiple occupied workspaces on
  one removed output, global-unique and shared behavior, no-duplicate and
  current-membership convergence, and tuple-replacement safety. Hermetic Rust
  and KWin coverage only; no live KWin, Plasma, or D-Bus action occurred.
- The tree retains opaque output IDs only, never native output objects.

#### User-Owned Live Gate - Not Run

1. The user follows `docs/live-kwin-testing.md`, obtains the required session
   authorization, and records a restorable baseline for a tiled scope on two
   outputs, including per-output current desktops, frame geometries, active
   output/workspace, and `omnitiler:plan` plus
   `omnitiler:workspace` journal lines.
2. The user unplugs (or disables) the secondary output, waits for the
   debounced refresh, and records all Plan requests, frame geometries, current
   desktops, active window, and journal lines. The user replugs the output and
   records the same evidence. No project D-Bus method is injected.
3. Pass only if unplug keeps the displaced layout as separate workspace(s) on
   a survivor with no merge into the visible tree, visibility/focus follows
   the active window's location (survivor view preserved otherwise), replug
   returns each displaced workspace with its then-current contents/layout by
   workspace unit, no writes target absent outputs, and no unrelated domain is
   evicted. Record the exact failing observation otherwise.

### Resolution And Scaling Changes - CODE ADDRESSED, LIVE GATE PENDING

- Each observation still obtains current work-area geometry from `clientArea`:
  `kwin/src/plan-adapter-entry.ts:336-388`. When the logical domain, selected
  `(DOMAIN_GAP, OUTER_DOMAIN_GAP) = (8, 8)`, and complete window set match but
  bounds differ, the adapter now sends a retained `reconcile` projection rather
  than replacing `lastGood`: `kwin/src/plan-adapter.ts:216-237,1149-1173`.
- The Rust Planner projects the retained tree into the new bounds, then records
  those bounds without replacing topology, shares, membership, focus, or
  revision: `src/planner_protocol.rs:2060-2256` and
  `src/session.rs:755-785`. Observed client rectangles do not supply shares.
  Changed membership continues through admission/removal before this branch;
  changed domain keys remain outside it.
- A work-area projection is explicitly separate from client geometry drift.
  Its terminal outcomes neither consume the max-three reassert budget nor park
  the scope: `kwin/src/plan-adapter.ts:1276-1283,1685-1697`.
- Fullscreen members stay in the retained tree and reply actuation still skips
  their geometry writes: `kwin/src/plan-adapter.ts:1593-1636`. No generic
  reprojection refusal was added. Existing validation refuses each invalid
  condition before projection with its established exact cause; retained inner
  and outer gap changes refuse as `domain-mismatch` with distinct messages:
  `src/planner_protocol.rs:2085-2129`.
- Hermetic Rust and KWin coverage proves grow, shrink, scale-style bounds,
  selected gaps, reply actuation, fullscreen isolation, combined bounds and
  membership change, and non-consumption of drift retries. Static checks only;
  no live KWin, Plasma, or D-Bus action occurred.

#### User-Owned Live Gate - Not Run

1. The user follows `docs/live-kwin-testing.md`, obtains the required session
   authorization, and records a restorable baseline for a disposable two-window
   normal tiled scope on one existing output, including frame geometries,
   active output/workspace, and `omnitiler:plan` journal lines.
2. The user changes only that output's resolution, waits for the debounced
   refresh, and records all Plan requests, frame geometries, and journal lines.
   The user restores the original resolution and records the same evidence.
3. The user changes only that output's scaling, waits for the refresh, and
   records the same evidence. The user restores the exact baseline. No project
   D-Bus method is injected in either journey.
4. The user repeats one resolution or scaling change with one member fullscreen,
   records the Planner trace request/reply, bounded plan diagnostics, and all
   member frames, then exits fullscreen and restores the exact baseline. The
    trace per-member `omnitiler:plan:write` lines prove the fullscreen member
   carried `disposition=skip-fullscreen` while every sibling carried
   `disposition=written`.
5. Pass only if each bounds change issues one retained projection to the new
   work area with the retained split/share structure and 8px gaps, no second
   `reconcile` plan after settlement, and exact restoration after reversal; while
   fullscreen, that member remains fullscreen through the sibling action and
   retains its position on exit. The no-native-write assertion is proven by the
   fullscreen member's `disposition=skip-fullscreen` write line in the live log.
   Record the exact failing observation otherwise.

### Sleep And Wake - SELECTED RECOVERY, STATIC-COMPLETE, LIVE GATE PENDING

- No committed KWin, Planner, or transport source subscribes to a sleep, resume,
  or power-state event. The production adapter has only window, geometry, scope,
  and shortcut entry points: `kwin/src/plan-adapter.ts:686-731,759-870` and
  `kwin/src/plan-adapter-entry.ts:694-809`.
- The Planner has only in-memory owner, generation, and domain state:
  `src/planner_protocol.rs:1263-1288`. It exits on serving-connection, monitor,
  or Planner-name loss: `src/planner_service.rs:346-415`. Its D-Bus user unit
  specifies `Restart=no`: `home-manager-module.nix:50-61`.
- In current source, if wake drops the Planner name, the process exits and its
  retained topology is lost. The adapter keeps the old flight terminal and
  never replays it; normal Plan transport pins a unique owner via strict
  `NameHasOwner` plus one bounded `StartServiceByName(..., 0)` accepting only
  `PrimaryOwner`/`AlreadyOwner` then `GetNameOwner`, with no Legacy path.
  Ambiguous timeout, malformed, service-fault, missing-callback, and
  correlation-mismatch terminals alone never recover but may run one bounded
  identity probe; only absence or a changed unique owner triggers recovery.
  A later scope/window signal or user shortcut is the remaining route to
  another owner-pinned call. Which KWin signals wake emits remains session
  runtime behavior not established by this repository.
- Selected, static-complete, live gate pending: after CONFIRMED Planner loss,
  one bounded on-demand fresh-session activation fits CURRENT eligible windows
  with the approved simple near-layout heuristic, using normal tiling when it
  cannot produce a supported layout. This is in-memory only: it retains no
  durable layout snapshot or journal, does not infer prior internal history,
  and may change grouping. If the Planner survives sleep, every baseline is
  retained with no rebuild. This adds no polling, retry, or systemd restart
  loop, keeps every old in-flight transaction terminal with old-generation
  replies rejected, replays no commands, recovers no uncertain native
  mutation, applies no stale replies across sessions, settles no parked
  workspace-send partial recovery, and stays unavailable while a workspace
  send blocks Plan. Offline TypeScript and Rust coverage only; no live KWin,
  Plasma, D-Bus, or suspend action occurred.

#### Live Experiment

1. Follow `docs/live-kwin-testing.md` and obtain the required user authorization
   for a session boundary.
2. In a disposable three-window normal tiled scope, record the current Planner
   owner, one successful `DescribePlan` result, the three frame geometries, and
   the current `omnitiler:plan` journal lines.
3. Suspend and wake the host manually. Make no tiling input for 10 seconds.
   Record Planner owner/name state, frame geometries, and new journal lines. If
   loss was confirmed, record whether the selected automatic bounded fresh
   activation occurred; if the Planner survived, record that its layout was
   retained.
4. Issue one existing directional tiling shortcut. If loss was confirmed, record
   whether the request succeeds and whether the resulting layout is a fresh fit
   from CURRENT windows or the normal-tiling fallback; it need not retain the
   prior grouping.
5. Restore the exact scoped baseline. The condition is handled only if confirmed
   loss has the selected automatic bounded fresh-session route, the first
   post-wake operation has the selected fresh-layout semantics, and old-flight
   terminal behavior is preserved. Otherwise record the exact failing step.

### Full-Screen And Gaming - CODE ADDRESSED, LIVE GATE PENDING

- `kwin/src/plan-adapter-entry.ts` now observes the exact fail-closed
  `readProp(ref, "fullScreen") !== false` state and subscribes to each window's
  `fullScreenChanged`, including later additions. A fullscreen window remains in
  the observation set and Planner tree; it is not deferred or removed.
- `kwin/src/plan-adapter.ts` carries the last retained in-bounds planned rect for
  fullscreen members instead of their compositor-owned fullscreen frame. This
  keeps every Planner request valid when the fullscreen frame extends beyond the
  work area, retains the member's tree position and share, and permits sibling
  reflows.
- Reply-time re-observation skips every fullscreen geometry target. Fullscreen
  frame drift is excluded from the reassert-then-park policy, while fullscreen
  exit compares the restored frame against the retained projection and reconciles
  only if restoration is imperfect. Direct move, keyboard resize, and pointer
  resize of a fullscreen target refuse fail-closed; the pointer route's
  adapter records the exact `omnitiler:plan:pointer-refused-fullscreen`
  token rather than a shared derivation failure.
- Hermetic KWin coverage proves admission with a fullscreen member, enter and
  exit restoration, sibling reflow, reconciliation isolation, out-of-bounds
  fullscreen frame containment, and later-window signal attachment. `npm test
  --prefix kwin` passes 476 tests; no live KWin, Plasma, or D-Bus action was run.
- The active-border effect hides its item for fullscreen:
  `kwin/native-effect/activeborderlogic.h:31-37` and
  `kwin/native-effect/activewindowborder.cpp:91-101`.
- Ongoing work while a fullscreen window is foregrounded:
  `kwin/native-effect/activewindowborder.cpp:104-107` has a `paintScreen`
  override that delegates to KWin. Static source cannot establish its frame
  invocation cadence beyond the loaded-effect framework cost.
  `kwin/src/entry.ts:13-49` and `kwin/src/tray-publisher.ts:1-50,90-112` issue a
  `PublishSnapshot` D-Bus call every second. `src/tray_endpoint.rs:556-568` emits
  a tray change signal every second. The plan adapter has event subscriptions
  (`kwin/src/plan-adapter-entry.ts:694-739`), a 120 ms one-shot debounce
  (`kwin/src/plan-adapter.ts:904-920`), and a 2 s one-shot in-flight timeout
  (`kwin/src/plan-adapter.ts:1218-1227`), but no polling loop. Drag-oracle
  handlers are interaction signals only:
  `kwin/src/plan-adapter-entry.ts:975-1027`.
- The reply can include a fullscreen member's retained geometry, but reply
  actuation skips its native geometry write. The per-member
  `omnitiler:plan:write window=<id> disposition=skip-fullscreen
  rect=<...>` line proves that skip in a live log for the fullscreen member,
  while every applied sibling logs `disposition=written`. The recurring tray
  D-Bus work continues during fullscreen. Its measurable gaming cost, and the
  actual cadence of `paintScreen`, are not established statically.

#### User-Owned Live Gate - Not Run

1. The user follows `docs/live-kwin-testing.md` and creates a disposable
   three-window normal tiled scope with bounded journal capture enabled.
2. The user records the member frames, then fullscreens one member using its
   normal fullscreen control. No project D-Bus call is injected.
3. While fullscreen remains active, the user triggers one existing tiling
   shortcut on a non-fullscreen sibling and records the resulting bounded plan
   diagnostic plus all three frames.
4. The user exits fullscreen and restores the exact baseline. Pass only if the
   fullscreen member stays fullscreen during the sibling command, the sibling
   command is applied, and it returns to its retained tree position on exit.
   The fullscreen member's `disposition=skip-fullscreen` write line proves no
   native write reached it, while the sibling's `disposition=written` line
   proves the sibling write. Record the exact failing observation otherwise.

### Underlying Configuration Changes - INTERIM PARTIAL, GAP LIVE GATE PENDING

- The approved interim target remains deliberate tiler reload after saving
  tiling settings. Its implementation is partial: gap reload is statically
  verified, while broader settings reload remains unfinished. The KCM offers
  the implemented gap-only reload and distinguishes restart-required settings
  from settings with no running effect. Existing live border updates remain
  live. Gap runtime verification is pending. Before launch, every user-facing
  setting must apply live; this mandatory blocker is not satisfied by the
  interim work.
- The active-border effect does handle a KWin reconfigure call: it rereads its
  configuration and updates the outline and border at
  `kwin/native-effect/activewindowborder.cpp:45-50`.
- The KCM writes seven script settings to `kwinrc`, including bounded
  `innerGap` and `outerGap` values. Production reads `shortcutProfile` and
  `workspaceMode` at startup; `tilingAlgorithm`, `automaticSplitTarget`, and
  `dropOutlinePreview` are persisted but not consumed by the running
  controller; the gap pair resolves at startup and re-resolves
  only on the deliberate Options `configChanged` reload owned by
  `kwin/src/plan-adapter-entry.ts`. KWin source evidence (6.7.4 tarball):
  `src/scripting/scripting.cpp:224-227` exposes the Options singleton as the
  script global `options`; `src/options.h` declares `void configChanged()`;
  `src/options.cpp:657-662` emits it from `updateSettings()`; `src/workspace.cpp:998-1017`
  invokes that from `slotReconfigure()` after reparsing configuration for the
  `org.kde.KWin /KWin reconfigure` call (`src/dbusinterface.cpp:64-67`,
  `Q_NOREPLY`). The running script is never restarted by that call (the
  Scripting load path returns -1 when already loaded, `Script::run` returns
  early when running), so the entry subscription is the pickup route: it
  re-reads validated gaps, logs one bounded `config-reloaded` line, and
  requests one debounced resync through the existing single-flight guards.
  That resync dispatches one retained `update-gaps` plan for a changed gap
  pair (never a silent baseline adopt, never a work-area reconcile).
  Unchanged signals resync nothing; shortcuts are never re-registered and no
  script/plugin lifecycle runs.
- KCM Apply no longer auto-queues a script reconfigure on save. Saving gaps
  sets reload-required with the exact status `Tiling gaps saved. Reload
  required: the running tiler still uses startup gap values.` Saving only
  startup-consumed `shortcutProfile`/`workspaceMode` sets restart-required with
  `Startup setting saved. Session restart required: the running tiler still uses
  startup values.` and leaves Reload Tiler disabled. Saving only unconsumed
  `tilingAlgorithm`, `automaticSplitTarget`, `dropOutlinePreview` sets
  no-running-effect state with `Setting saved. No running tiler effect: this
  setting is unconsumed; neither reload nor restart applies it.` and leaves
  Reload Tiler disabled with no restart claim. Combined saves distinguish each
  pending category (gap plus startup, gap plus unconsumed, startup plus
  unconsumed, all three): reload applies gaps only, restart applies
  startup-consumed settings only, unconsumed settings stay without running
  effect. An unchanged save sends nothing and leaves the flags untouched. The Reload Tiler button is
  enabled only while gap reload-required; with no pending reload it refuses without
  sending, so an idle click queues no typed D-Bus traffic and leaves the flags
  and status untouched. The deliberate Reload
  Tiler button sends exactly one typed `org.kde.KWin /KWin org.kde.KWin
  reconfigure` request: gap-only success reports `Reload request sent. Application
  unconfirmed; restart the session to guarantee pickup.` and keeps
  reload-required; gap plus startup success reports gap-unconfirmed while retaining
  restart-required for startup settings; gap plus unconsumed success reports
  gap-unconfirmed plus no running effect for unconsumed settings; all-categories
  success reports both residuals; failure reports the gap failure plus any
  pending startup and/or unconsumed residuals and keeps
  reload-required (plus restart-required and/or unconsumed state when present). No status claims applied. KCM-side running confirmation is
  not provable: KWin's reconfigure is Q_NOREPLY, so a queued send alone is
  reported as unconfirmed and session restart remains the guarantee for gap
  pickup only where the retained route below cannot converge (for example a
  refused update). The
  button never touches shortcuts and never unloads scripts or plugins.
- The retained gap-update route is proven in source and offline behavior, not
  payload-only: the entry `options.configChanged` subscription re-reads
  validated gaps and the adapter's debounced resync now dispatches one explicit
  retained `update-gaps` DescribePlan (same domain and complete window set,
  changed inner and/or outer gap; a simultaneous work-area change folds into
  the same projection) instead of silently adopting the new baseline. The Rust
  Planner accepts it on the existing session without reseeding: `Session::
  update_domain_gaps` (`src/session.rs`) adopts the new outer-inset bounds
  plus inner gap atomically while preserving topology, shares, membership,
  focus, exceptions, and accepted revision, and `evaluate_update_gaps_retained`
  (`src/planner_protocol.rs`) reprojects the retained tree with the new inner
  gap and replies the native-apply-relevant geometry plus preserved focus. It
  stages no pending, replays no flight, resets/discards no session, and never
  seeds: unknown domains refuse so the normal admit path seeds them with the
  new gaps. Ordinary drift reconciliation still refuses any gap change as
  `domain-mismatch`; only the deliberate op crosses the boundary. Refusals
  (unknown/diverged/pending/partial-observation/focus-mismatch/malformed plus
  the inherited 0..64 gap-range validation) mutate nothing and keep the old
  baseline, so a later resync retries the same update. The adapter defers the
  update behind an in-flight command through the existing single-flight
  (correlation/epoch/stale-scope fences drop old flights), blocks it while a
  workspace send owns Plan, and never touches drift reconcile accounting.
- Reload/restart-required is dialog-scoped in-memory KCM state, not persisted runtime
  truth: `load()` resets both to `No pending tiler reload in this dialog.` So a
  save-then-load/reopen cycle clears the pending flag even though the running
  tiler is still stale. It cannot truthfully be preserved across dialog reload:
  persisting it would need a new `kwinrc` key outside the known script/effect
  groups and defaults, and KCM-side observation of real pickup is unsupported
  (Q_NOREPLY send; the KCM cannot distinguish restart from reopen). Session restart is the
  guaranteed pickup, but the KCM cannot observe restart versus reopen, so both
  preserving (risking a false pending) and clearing (risking a false clean)
  misstate unobservable runtime state; the implementation keeps the explicit
  dialog-scoped reset and claims no runtime application.
- Shortcut registration is startup-only by safe-capability choice, not by
  categorical assumption: the pinned KWin scripting surface
  (`kwin/src/kwin-globals.d.ts`, sourced to `src/scripting/scripting.h`)
  exposes only `bool registerShortcut(...)` with no unregister or
  re-register operation, so the reload path cannot safely refresh bindings
  (re-calling register would duplicate action registrations, and foreign
  records change only through the explicit KCM Apply/Revert table). The
  deliberate reload therefore never re-registers shortcuts; a profile or mode
  change still needs a session restart, which is the only route that
  re-executes startup registration. Shortcut Apply and Revert stay explicit
  KCM operations (`kwin/native-effect/activeborderconfig_module.cpp:254-323`):
  the KCM can detect recorded-postimage drift when opened
  (`activeborderconfig_module.cpp:448-457`), and no running-script watcher
  reconciles externally changed shortcuts or `kwinrc`.
- Expected residual: hand-edited `kwinrc`, externally changed script settings, or
  a KCM change can leave the already running script using its startup values
  for everything except the gap pair picked up by the deliberate reload,
  including existing shortcut registrations. The deliberate
  reload request is unacknowledged at the KWin transport, so a refused or
  unconverged gap update still needs a session restart for gap pickup; the
  retained `update-gaps` route is the normal pickup and restart is the
  fallback, not the mechanism. Border settings are the only other confirmed live
  configuration path. A command issued inside the ~120ms resync debounce
  window after reload can still carry new gaps before the update flight
  converges and take the pre-existing scope-change reseed path; likewise a
  simultaneous membership change is owned by admit/remove with existing
  scope-change semantics, not by the gap update. The
  all-settings live-application launch blocker is unchanged: overall settings
  liveness is PARTIAL (gaps deliberate-reload plus live borders only).
- Consumed versus unconsumed settings (no false restart promise): session
  restart applies only what startup reads. `shortcutProfile` and
  `workspaceMode` are read at startup (`kwin/src/plan-adapter-entry.ts:
  readShortcutProfile`, `readWorkspaceModeValue`, workspace-native mapping),
  so restart genuinely picks them up. `tilingAlgorithm`,
  `automaticSplitTarget`, and `dropOutlinePreview` are persisted by the KCM
  but read by nothing in the running controller, so neither reload nor
  session restart applies them today; the KCM state machine distinguishes all
  three categories and their combinations with no restart claim for
  unconsumed-only saves, and consuming them
  is launch-blocker work, not a restart.
- Offline verification (2026-09-17, no live KWin, Plasma, D-Bus, or session
  action): `npm run typecheck --prefix kwin` passes; `npm test --prefix kwin`
  passes 773 tests across 110 suites with 0 failures (including the
  `tiler-reload-interim` contract plus `gap-update-reprojection` adapter and
  production-entry tests proving the deliberate signal dispatches retained
  `update-gaps`, the planned reply writes natively with focus preserved and
  converges the baseline, in-flight deferral without replay/reset,
  stale-correlation drop, refusal without baseline motion, and no shortcut
  re-registration; plus the updated work-area test proving a gap change
  routes to `update-gaps`, never to work-area reconcile or silent adopt);
  `npm run build --prefix kwin`
  emits the `kwin/contents/code/main.js` bundle; native `ctest` was not
  re-run (no native source changed); `cargo test`
  passes 531 tests with 0 failures (including 8 retained `update-gaps`
  Planner tests proving inner, outer, and combined acceptance with exact
  reprojected geometries, focus/revision preservation, resized-share
  round-trip restoration, old-flight `domain-mismatch` refusal after update,
  membership/unknown-domain/out-of-range/malformed refusals without
  mutation or seeding) and `cargo build` succeeds. No runtime
  claim is made.

### Maximized Admission (Q3, R-MAX-03/R-MAX-06) - DELIVERED OFFLINE, LIVE GATE PENDING

- This replaces the old maximize-admission clear gate linked from the pending
  live checks. The no-re-maximize-loop invariant remains: a session-restored
  maximized app must not be repeatedly restored or have geometry fought by
  the tiler. Q3 changes the expected first state from restored/tiled to
  maximized over a reserved tile slot; native restore later reveals that slot.
- `kwin/src/plan-adapter.ts`: `clearMaximizeAtAdmission` preserves first-seen
  maxima, including R-MAX-03 floating-to-tiled admission under the 2026-10-07
  Q3 scope decision. The first-domain origin gate is retired. Held-born-fullscreen
  release alone keeps the existing exact-ref one-shot clear/echo fence, with
  no retry even if the app immediately reasserts maximize.
- Anti-loop: Q3 admission issues neither clear nor maximize. Existing retained
  projection, `fit_excluded`, `skip-maximized` writes and overlay drift
  exclusion avoid native geometry fighting. Repeated maximized observations
  settle; ordinary native restore reconciles to the reserved slot.
- Offline evidence: real Engine/adapter admission and restore fixtures prove
  the slot and sibling allocation, actual restored-frame write, foreground
  and hidden overlay isolation, quiet repeats and no native clear/toggle.
  R-MAX-03 retile/restore, non-maximized retile, synchronous write signals and
  born-maximized newcomer interaction are covered; held-fullscreen one-shot
  coverage remains. See [born-maximized record](archive/kde-born-maximized-overlay.md)
  and [scope delivery](archive/kde-maximized-floating-retile-overlay.md).

#### User-Owned Live Gate - Not Run

1. Follow `docs/live-kwin-testing.md` and `docs/dev-loop.md` after user-owned
   delivery of the checkout. On a tiled workspace with B, launch an eligible
   normal resizable A already maximized, not fullscreen. Record native flags,
   frames, focus, and trace `planned-applied` / `skip-maximized` with A's
   reserved target rectangle. A remains maximized, B takes its tile share,
   and no admission clear/toggle is issued for A.
2. Natively restore A. Record that its actual frame lands in the reserved
   target and B keeps its share. Repeat native maximize/restore and observe
   settled idle with no unsolicited toggles, retries or geometry fighting.
3. At a user-owned session boundary, restore a session with maximized A and B.
   A remains maximized over its slot, restores into it when requested, and
   never enters a re-maximize loop. Capture the same flags/frames/trace;
   visual appearance or setter-return diagnostics alone are not acceptance.
4. R-MAX-03: first observe maximized A without a slot beside ordinary B on a
   floating workspace, toggle that workspace tiled, and record flags/frames/trace.
   A stays maximized with a reserved target (`skip-maximized`), B takes its
   allocated share, and no admission clear/toggle occurs. Natively unmaximize
   A: its actual frame lands in that target, B keeps its share, and repeated
   maximize/unmaximize settles without loops or geometry fighting. Repeat
   with a session-restored maximized A at a user-owned session boundary.

## Proposed Slices

- P0 | Fullscreen residual-cost live gate | Code isolation is complete in the
  production Plan observation, request, and geometry-write paths. Run the
  user-owned fullscreen gate above to prove no fullscreen write through entry,
  idle, unrelated activation, sibling tiling input, fullscreen-target tiling
  input, and restore; capture frame-time and project traffic against idle.
- P1 | Output hotplug domain lifecycle | Boundary: implement the selected
  separate-workspace preservation for a disconnected output's displaced layout.
  If the active window was on the disconnected monitor, show its relocated
  workspace and retain focus on that window. If the active window was on a
  surviving monitor, preserve its current visible workspace and focus; displaced
  workspaces remain accessible through normal workspace switching. If there is
  no active window, preserve the surviving monitor view. On reconnection,
  displaced workspaces automatically return to their original monitor with
  their then-current contents and layout, not a saved snapshot: split edits,
  closed and new windows remain reflected. Workspace relocation is the unit: a
  window explicitly moved out stays at its destination and is never individually
  pulled back, while a window moved into a displaced workspace returns with it.
  Destination among multiple surviving outputs uses the nearest surviving
  monitor from geometry already available while handling disconnect, with no
  added historical state. If that would require old output geometry/history or
  an extra tracking mechanism, use the current primary surviving monitor; if
  that is not identifiable, use existing available output ordering as the
  deterministic fallback. Availability and lifetime of removed-output geometry
  are implementation source-check details, not a claim that nearest is always
  feasible. This destination choice is distinct from the selected displacement
  association required for automatic workspace return. On original-output
  reconnect, if the active window is in a returning workspace, show that
  workspace on the reconnected monitor and retain focus on that window. If the
  active window remains on a surviving output, preserve its view and focus with
  no focus stealing. Other workspace selection follows ordinary behavior, with
  no prior-view tracking or new state/history. The initial scope is
  session-local with no restart-persistent mapping or return guarantee. All
  initial hotplug product choices are resolved; implementation and live
  verification remain pending. Gate:
  unplug and replug a tiled secondary output while another output remains
  active; prove selected tree semantics, no writes to absent outputs, and no
  unrelated domain eviction.
- P1 | Work-area change projection live gate | Static implementation now projects
  one retained existing domain through resolution, scaling, and work-area bounds
  changes without client-drift retry/park misuse. Run the exact user-owned gate
  above; output addition, removal, retirement, and all-domain reconciliation
  remain the separate output-hotplug slice.
- P1 | Wake transport recovery | Selected, static-complete, live gate pending:
  after CONFIRMED Planner name or connection loss, one bounded on-demand fresh
  session fits CURRENT eligible windows with simple near-layout fitting or normal
  tiling when no fit applies; surviving Planners retain their layouts with no
  rebuild. Boundary: in-memory fresh recovery only, not durable
  snapshots/journals, old-flight reset or replay, stale cross-session replies,
  uncertain native mutation, or parked workspace-send partial recovery. Gate:
  the sleep/wake experiment above proves the selected automatic recovery
  semantics, bounded failure behavior, and the first post-wake plan result. No
  live result is claimed.
- P1 | All user-facing settings live application (launch blocker) | Boundary:
  script settings, KCM Apply, external `kwinrc`, shortcut drift, and effect
  reconfigure. The approved interim is statically implemented (deliberate
  tiler reload after save, clear reload-required UI, retained live border
  updates, sent-but-unconfirmed reporting, no silent stale state); its live
  verification remains pending. Before launch, change each
  user-facing setting through KCM and its underlying store and prove live
  application, including shortcut registration state.

## Decision Contradiction

- `docs/decisions.md:233-235` claims fullscreen is never tiled, resized, or
  reflowed. The committed production Plan route contradicts that claim as shown
  above. This record does not alter the decision.
