# KDE restart float-intent preservation

## Final outcome

- Complete: held-first intent preservation and the retained-Planner sequence
  correction pass offline and the complete-baseline two-output native restart.
  Intent identities and native state survive; explicit post-restart retiling
  proves the fixed override remains controlled. Workspace modes still reset
  under the existing REQ-CTL-04 policy; layout restoration is not asserted.
- The historical zero-traffic instance was a procedure artifact: a surviving
  query helper occupied the reused numeric script path, so `run` reached the
  already-running helper instead of evaluating the replacement controller.
  Helper accounting/teardown and evaluated-readiness gates correct the procedure;
  the separate production sequence fix is not credited with fixing this collision.

## Goal and boundaries

- Preserve settled persisted float intent when the first production-world
  observation holds its scope as unreadable; no implicit toggle is authorized.
- Replay the real persisted-store shape and real first-observation hold before
  changing production code. Preserve tile overrides and distinguish capacity
  admission from explicit intent; previous tree topology is not a requirement.
- Scope: `entry.ts` -> `world-*.ts` and core `world_*` modules. Legacy adapters
  are unshipped and excluded. Automatic keep-above lifecycle ownership remains
  parked in its existing change record.

## Acceptance and approach

1. Accept a producer-shaped RED replay: persisted float plus tile override,
   unreadable first observation, then complete observation of the same identities.
2. Correct uncommanded loss without changing explicit toggle/close semantics;
   add relevant held/missing-row and scope-isolation controls.
3. Run targeted RED/GREEN, full KWin and Rust suites, typecheck/build, formatting
   and strict Clippy; independently review persistence and native safety.
4. Capture a complete fresh live baseline and proven complete request/reply
   traffic, then repeat script restart with one Meta+G float and one tile override
   on two outputs. Stop on any baseline gap or unexpected refusal and restore.
5. Record exact native limits and restoration; archive only if acceptance is met.

## Current outcome

- Desktop prerequisite completed: captured four-desktop names/order restored with
  replacement native IDs for two retired empties; protected terminal unchanged,
  controller stopped. Detailed outcome remains in the automatic-float record.
- Real-engine replay reproduced the loss: the first native unreadable scope
  produced no float row, and the next durable snapshot lost its seeded member.
  Whole-inventory unreadability also dropped the tile override. Both fail before
  the correction and pass afterward; topology preservation is not asserted.
- Convergence now removes float membership only with positive, controlled tile
  proof in the same live native scope or explicit successful unfloat. Held,
  missing, untouched, overlay and minimized rows retain intent. Incomplete fresh
  inventory prevents absence-based pruning of either persisted set; complete
  departure still prunes. Unrelated holds conservatively defer departure cleanup.
- Independent review exposed an overlay/cross-scope false-positive prune in the
  first correction. Both controls reproduced loss before tightening authority,
  live-safety and scope checks; corrected controls pass. No lifecycle ownership
  policy or new product decision selected.
- Integrated gates: KWin 1959 passed, Rust workspace 3353 passed, zero failures
  or ignored; typecheck, bundle build, Rust formatting and strict all-target
  workspace Clippy pass.

## Native attempt and next gate

- Complete fresh baseline and marker-proven canonical production traffic capture
  were established. Native float and fixed tile-override identities survived a
  script restart and later writes. These are bounded membership observations,
  not accepted clean-journey evidence: startup ignored the staged floating default
  until KWin configuration was reloaded, and the run continued after that mismatch.
- Independent review rejected the clean-pass claim. All sampled test scopes were
  floating, so tiled-scope tile-frame persistence was not demonstrated either.
- Protected placement/focus, pointer, per-output views, store bytes/mode,
  shortcuts and effects were restored. Retired baseline desktops were recreated
  with captured names/order under replacement IDs; configuration differs only by
  those IDs and default tiling groups. A restoration-only probe found the removed
  temporary key still cached; reconfigure plus a fresh probe restored its effective
  default. Controller, Planner and helpers are stopped.

## Clean repeat: stopped on silent startup

- A fresh baseline and continuous, marker-proven request/return/error capture
  preceded startup. The effective floating default was verified before enabling
  the controller; the prior configuration mismatch did not recur.
- Focus-verified registered callbacks enabled only the two isolated test scopes.
  Both accepted tiled modes while the protected scope remained floating. A normal
  client became an explicit float; a genuine fixed-size client became an explicit
  tile override with a controller-assigned native frame and released owned
  keep-above. This is callback evidence, not physical-key delivery or KWin Tile
  object association.
- The settled store contained exactly one float and one tile override. Script
  restart retained the same Planner and live clients, but the replacement script
  emitted no seed request, world call or post-restart journal output despite
  successful load/introspection/run. One bounded owned-client focus event also
  produced no output. This was not the previously observed explicit stale-sequence
  refusal chain. The leg stopped without another restart; no clean live pass or
  post-restart persistence claim is made.
- Independent evidence reconciliation confirms the failure and restoration:
  protected state/focus, pointer, corresponding per-output views, shortcuts and
  effects match; desktop names/count/order match with replacement IDs for retired
  empties. Configuration differs only by replacement IDs and default tiling groups;
  the temporary key/group is absent and a fresh probe verifies the effective
  original default after reconfigure. Store bytes/mode match the fresh baseline
  by explicit byte restoration, not controller close convergence. All owned
  clients, controller, Planner, monitors and helpers are stopped.

## Startup diagnosis and readiness correction

- Offline evidence confirms zero replacement-instance calls, not a Planner
  generation or ownership refusal. Completed production entry evaluation would
  also emit the unconditional tray-start diagnostic. Retained helper/load IDs
  and pinned KWin lifecycle source identify the actual collision: a query helper
  at ID 3 survived removal of controller ID 2; the replacement received ID 3,
  whose D-Bus path still addressed the already-running helper. Its no-op `run`
  produced no controller evaluation, while plugin-name lookup still reported
  the new controller loaded. A later helper at ID 4 evaluated normally.
- KWin at `1f543de908`, `Script::run`/`slotScriptLoadedFromFile`, delays a fresh
  D-Bus reply until after evaluation. The earlier interpretation that it replies
  before evaluation was incorrect. `Scripting::loadScript` uses current list
  size, `scriptDestroyed` removes without renumbering, and `AbstractScript`
  ignores duplicate-path registration failure; an already-running `run` exits
  before the delayed-reply path. This explains why lifecycle receipts were
  collision-blind. The exact helper plugin/source receipt and the raw D-Bus
  dispatch to the collided path were not retained; the classification rests on
  the load-ID chronology and source mechanism, not direct dispatch capture.
- The prior filtered journal also omitted script-engine diagnostics. The live
  guide now requires helper receipt accounting and teardown before removing an
  older controller, evaluated readiness, correlated seed/world traffic and
  unfiltered current-process script-engine diagnostics. No production startup
  guard, float-intent or Planner behavior caused that zero-evaluation instance.
- A fresh readiness-only leg passed those gates with no script-engine errors,
  then stopped before the journey when empty spare desktops retired to two.
  Restoration was verified, including exact configuration/store bytes and
  desktop names/order with replacement IDs. This is expected empty-cell
  lifecycle behavior under the aligned-workspace decision, not restart evidence.
  The next setup must occupy the isolated test cells before controller startup.

## Retained-Planner sequence defect

- With occupied test cells, the replacement evaluated and sent seed/discovery
  traffic. Every request then hit `request-superseded`: the production adapter
  reused its fixed incarnation while resetting request/command counters to one
  against the retained Planner's higher watermarks. The leg stopped and restored;
  no accepted post-restart write or native acceptance was claimed.
- Production now mints a fresh adapter incarnation per evaluation, keeping the
  owner stable and using the existing new-incarnation protocol path. A permanent
  real-entry/retained-engine two-start replay reproduces the refusal with the
  fixed token and passes with the correction. New commands execute rather than
  inheriting the old command-consumed watermark; old commands are not replayed.
- Scope-mode reset is already selected by REQ-CTL-04, not float-intent loss:
  restart reseeds workspace modes from the saved default. Acceptance therefore
  observes native frames/memberships and both persisted identities before any
  post-restart command, then explicitly re-enables the two test scopes to prove
  the fixed override still receives a controlled tiled frame. Neither scope-mode
  nor previous layout preservation is introduced here.
- Independent review identified missing permanent two-start coverage and the
  distinction between scope-mode and tile-override identity. Inspection then
  corrected the new replay's fixed-size and second-scope fixtures. Both scopes
  are now genuinely tiled before restart and explicitly re-tiled afterward;
  accepted replies prove controlled fixed-window geometry and float retention.
- Latest gates: KWin 1962 passed, Rust workspace 3353 passed, zero failures;
  typecheck/build, Rust formatting and strict all-target workspace Clippy pass.

## Corrected-build native observations

- The replacement used a distinct incarnation and accepted reset requests at
  revisions 11 and 12 after the old instance reached revision 10. Both seeded
  intent sets and all six subsequent write bodies retained the same identities;
  sampled native frames, memberships, outputs and keep-above state were unchanged.
  Explicit retiling of both scopes then accepted fresh commands and retained the
  fixed override's controller-assigned frame. This corroborates the sequence fix.
- These are bounded native observations, not complete acceptance: independent
  evidence review found shortcut counts without exact tuples and no initial
  loaded-effects inventory. Store/configuration bytes, sampled protected state,
  pointer and desktop names/order were restored, but full baseline equality is
  unprovable for those missing captures. Empty per-operation reply extracts also
  cannot serve as reply evidence; the continuous raw monitor must be decoded.
- A redundant second start was refused before loading another controller; only
  the intended initial and replacement instances evaluated. Do not infer an
  extra controller or a product refusal from that pre-effect procedural error.
  A fresh complete-baseline leg is required, with exact capture commands retained.

## Complete-baseline native acceptance

- Fresh captures include configuration/store bytes and modes, four desktop
  identities/names/order, output/current views, pointer, complete 25-field
  protected-window state, exact shortcut tuples across 20 components (457 rows,
  32 project actions), and loaded/active effects (29 loaded). Capture commands,
  a paired positive marker, continuous unfiltered current-process journal and
  36,600 lines of complete request/return/error traffic were retained. Actual
  correlated reply bodies, not empty convenience extracts, support acceptance.
- Exactly one initial and one replacement controller evaluated against the same
  Planner and live clients. Different adapter incarnations accepted reset
  requests at revision 11 and subsequent revisions through 16, with no rejection
  or consumed command. Every post-restart write retained the explicit float and
  fixed-window tile identities. Native frames, membership, outputs, stacking,
  views and pointer matched before any new command; store bytes also matched.
- Both scopes reset to Floating as specified. Explicitly re-enabling each scope
  accepted fresh commands at revisions 14 and 16. The fixed override's controlled
  native frame matched the accepted allocation with owned keep-above released;
  the normal explicit float remained floating. These are registered-callback
  observations, not physical-key delivery or KWin Tile object association.
- Independent reconciliation verified the full shortcut component tuples and
  loaded/active effects byte-for-byte, protected state 25/25, store bytes/modes
  after Planner shutdown, original effective configuration via a fresh probe,
  original desktop count/names/order and corresponding views. One retired empty
  desktop was recreated under a replacement ID. Configuration bytes differ only
  by native desktop ID/default-tiling lifecycle changes: the initial disk IDs
  were already stale against the captured live inventory, and KWin flushed the
  live IDs. No byte-exact kwinrc restoration claim is made for that documented
  lifecycle churn. All owned clients/controller/Planner/helpers/monitors stopped.
- Final gates: KWin 1962/1962, Rust workspace 3353/3353, zero failed or ignored;
  typecheck/build, Rust formatting, strict all-target workspace Clippy and
  whitespace checks pass. Native acceptance is complete; no next action here.

## Remaining boundaries

- Automatic keep-above unload/restart ownership remains parked unchanged. The
  successful pre-restart explicit tile released automatic ownership; no survivor
  policy is inferred from these persistence tests.
- No new tentative product decision or unresolved membership-policy question was
  introduced. The historical collided helper's exact source/plugin and direct
  dispatch receipt remain evidence limits, not new startup or ownership policy.
