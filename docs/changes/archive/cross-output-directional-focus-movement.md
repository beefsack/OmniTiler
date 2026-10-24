# Cross-Output Directional Focus And Movement

- Status: historical delivery, archived 2026-10-24. The DescribePlan/adapter
  route below is removed; current production uses the world route per the
  [core closeout](core-world-model-rearchitecture.md#closing-outcome-2026-10-24).
- Historical delivery: DescribePlan and adapter transaction details below refer
  to the removed KDE domain route. Current production uses complete world
  reports, direction commands and observation convergence without ack/verify.

## Goal

Restore the documented COSMIC output-edge behavior for `Meta+Arrow` focus and
`Meta+Shift+Arrow` tiled-window movement.

## Scope And Non-Goals

- Route directional focus at the output-axis edge to the adjacent output's
  active workspace and last-focused window.
- Route directional movement that reaches `MoveFurther` to the adjacent
  output's active workspace, retaining the existing transaction, ownership,
  acknowledgement, and verification fences.
- Preserve same-output movement, workspace-axis behavior, exception refusals,
  and fail-closed adjacency observation.
- Do not add shortcuts, live KWin mutation, workspace/output wrapping,
  uncertain-send recovery, or a new topology model.

## Evidence And Decision

- COSMIC Epoch 1.5.0, released 2026-07-29, pins `cosmic-comp`
  `81cd5fdbaa41c3973369ae85bccf829137836e20`. Its documented source evidence
  establishes output-edge `Focus` fallback to `SwitchOutput`, whose target is
  the target output's active workspace and last-focused window, and
  `MoveFurther` fallback to `MoveToOutput`; transfer re-inserts at the target
  tree's focused node or root and Move follows focus. See
  `docs/research/integrated-tiling-workspace-value/cosmic-hyprland-comparison.md`
  C-15 through C-18 and C-41 (lines 655-659 and 682), retrieved 2026-08-10.
- The user identifies S20-S23 as COSMIC observations: they establish crossing
  into an occupied or empty output, R1's no-cross precedence, and repeated
  internal movement before crossing
  (`docs/cosmic-move-conformance.md:61-65,373-405`). The pure replay model is
  reference-only, not those observations. Its occupied target has one leaf, so
  it does not distinguish whole-target-tree wrapping from focused-leaf
  insertion; COSMIC C-41 selects focused-leaf/root insertion for the untested
  multiwindow-target case.
- The user directed recovery of this bounded source-evidenced output-edge
  behavior. The durable decision is reconciled in `docs/decisions.md` with the
  bounded active `DescribePlan` contract selected here.

## Acceptance

- Output-axis exhausted focus selects the adjacent output's active workspace
  last-focused tiled window; unavailable, ambiguous, or empty targets refuse
  without mutation or focus fabrication.
- `MoveFurther` transfers the focused eligible tiled window through KWin 6.7.5
  `workspace.sendClientToScreen` using the exact target output object, then
  writes exact target desktop membership. It commits only after output,
  membership, geometry, and focus readback proves the planned result.
- Same-output directional behavior, workspace-axis fallthrough, full-screen,
  floating, maximized, sticky, topology, owner, and transaction safeguards
  remain unchanged.
- Targeted Rust and KWin adapter tests cover distinct output domains, current
  visible workspaces, focus entry, movement crossing, and existing no-cross
  guards; affected suites, type checks, and build pass.

## Approach

1. Trace the current one-domain focus request and local R4 movement route
   against the documented output-action semantics.
2. Extend the existing multi-domain Rust/adapter contracts to observe the
   adjacent output's current workspace, validate its target focus/root, and
   retain exact source/target identities across the current transaction fences.
3. Use the existing KWin per-output current-desktop surface for focus and
   transfer actuation, then add regression coverage for the documented behavior
   and failure boundaries.
4. Verify, reconcile the active decision, record outcome and publish.

## Clarified Scope

- Current focus is single-domain and movement makes adjacency same-workspace,
  but the Session already owns domains keyed by `(output, workspace)` and the
  existing KWin surface reads and writes a desktop for an exact output. Adding
  target-domain identities, validation, and postconditions is ordinary bounded
  implementation within the selected Rust-authority/adapter architecture, not
  a new architecture decision.
- The correction scope is the source's default Vertical workspace layout:
  `Meta+Left` and `Meta+Right` cross horizontally adjacent outputs only after
  local focus/movement is exhausted. `Meta+Up` and `Meta+Down` retain current
  local behavior; directional workspace cycling is separate unimplemented
  source behavior and is not included. Transfer targets the adjacent output's
  currently active workspace, inserts beside its valid focused leaf or as its
  root, and follows focus. Missing, ambiguous, empty-for-focus, exceptional,
   or changed targets refuse before native action. This preserves S21's local
   perpendicular-wrap precedence and S20/S22/S23 crossing results.

## Incident Correction

- The user-reproduced trace at
  `omnitiler-dev.qQMfPr.log` proves the first rightward
  R4 was planned and natively mutated (`p3`, lines 37-49): output and all three
  changed geometries echoed, but no `r4-desktops-echo` arrived. The timeout sent
  `directional-move-ack` with `adapter-lost` (lines 51-52 and 63), terminally
  diverging the retained pair. The immediate, well-formed reverse `p7` then
  correctly received `diverged/adapter-lost` before planning (lines 59-60,
   then adapter service fault at 73-74). At the time, this was a permanently
   terminal state, not an eligible reverse blocked by a legitimate live flight.
   It is not the selected product policy: the user-approved "log and continue
   rather than hard fail" direction supersedes permanent disable after an
   operation failure. Current uncertain R4 transaction recovery remains
   terminal and unfinished until a separate design can safely preserve later
   valid commands without acknowledging, replaying, or resetting the uncertain
   transaction.
- Both output domains in the outbound request use workspace
  `45e11f71-14f9-44bf-98ec-aca62648c9b8` (line 37). Assigning that unchanged
  desktop membership does not produce a KWin `desktopsChanged` notification,
  while the old adapter required that echo even when exact membership readback
  already proved it.
- The adapter now marks the desktop fence observed only when the retained mover
  snapshot was already on the exact target workspace and a fresh readback has
  exactly that one desktop id. Output, geometry, focus, epoch, full proof,
  acknowledgement, verification, correlation, and canonical-pair fences are
  unchanged. Changed membership still requires `desktopsChanged`; this is not
  a timeout recovery, retry, reseed, or proof relaxation.
- Offline regressions prove the no-notify same-workspace transfer reaches
  accepted ack, verify, and commit with one focus follow; changed membership
  remains pending after output and geometry echoes until its desktop echo; and
  a committed occupied-target transfer plans the immediate reverse R4 from the
  retained target pair.

## Geometry-Fence Correction

- The supplied `omnitiler-dev.kjFKIE.log` records the
  `plan-1-p3` left transfer requesting the correct DP-6 rectangles: the mover
  and target Ghostty are both `1012x1092` at lines 24 and 41-42. Output and
  same-workspace desktop readback both completed (lines 38-39), but the first
  `r4-geometry-echo` occurred before any geometry write (line 37 precedes
  lines 40-42). The adapter previously counted that `sendClientToScreen`
  intermediate mover geometry as the planned mover resize, then considered the
  geometry fence complete once the remaining echoes arrived. Its exact
  post-readback instead terminally reported `post-observation-mismatch` at
  line 47 and sent `adapter-lost` (line 25). Every later command was a planner
  `diverged/adapter-lost` followed by adapter `service-fault`, not a live
  single-flight busy refusal.
- `onR4GeometryEcho` now consumes a per-window geometry fence only after it
  reads that exact window at its planned rectangle. An intermediate or
  different client-committed rectangle leaves the existing bounded deadline
  armed, then terminates as `timeout` if no exact echo arrives; it never
  acknowledges, verifies, commits, retries, or accepts a client-size drift.
  The regression uses a shorter source work area, taller target work area, an
  output-transfer mover echo before geometry writes, and a delayed final mover
  echo. It failed before the correction by sending the accepted ack early, then
  completes ack, verify, and commit after the final exact echo.
- KWin 6.7.5 source confirms the ordering is meaningful: `Window::sendToOutput`
  first calls `moveResize` (`src/window.cpp:3879-3923`), while the subsequent
  script frame write runs through XDG configure/ack/client commit
  (`src/xdgshellwindow.cpp:85-289`). KWin ultimately uses the
  client-committed frame geometry. KWin's public script surface does not expose
  a window's XDG minimum/maximum sizes or configure serials, so this record
  does not attribute the final `1012x1036` Ghostty frame to an app constraint,
  KWin rule, or scaling conversion. The sibling Ghostty reaching `1092` proves
  only that the DP-6 work area itself permits that height.

## Local Geometry Incident

- `omnitiler-dev.nEimw8.log` is a local R1 move, not an
  output crossing: `plan-1-p4` requests two-domain observation but replies with
  three DP-6 rectangles only (lines 49-50), then writes only DP-6 windows
  (lines 55-59). Rust still synchronously commits the local tree before the
  asynchronous frame writes settle.
- The command stall has a separate concrete adapter cause. That local reply
  retained the untouched HDMI-A-2 Firefox in the DP-6 baseline. The next
  foreground refresh synthesized `plan-1-p5` removal of Firefox under DP-6
  bounds (lines 53-54), producing `window-out-of-bounds` at lines 66 and 70-87.
  A local two-domain reply now retains only its source-domain windows and drops
  its directional pair metadata; it cannot synthesize target removal or poison
  the source baseline. Offline coverage proves that exact sequence makes no
  second remove request.
- This correction does not accept the client geometry mismatch. The planned W3
  frame is `1538,52,502,1092` (lines 50 and 57), while later observation is
  `1538,52,502,1036` (lines 69, 76, and 83). Firefox also later reports
  `x=144` while still identifying HDMI-A-2, outside that output's bounds.
  No Firefox setter appears in the local move writes. KWin 6.7.5 `window.h`
  exposes `resizeable`, `minSize`, and `maxSize`; `waylandwindow.cpp`
  `updateGeometry` derives output from the global frame center. The trace does
  not contain those values or the Ghostty executable version, so it cannot
  distinguish client constraints, a rule, or inconsistent native observation.
  The bounded next diagnostic is a trace-only read of these three public Window
   properties for the affected stable IDs before planner dispatch, at write, and
   post-signal time;
  it must not move, resize, focus, or load a production script.
- That diagnostic is implemented behind the existing compile-time trace gate.
  Each `constraint-trace` line carries only the existing opaque window ID,
   output, resource class, `resizeable`, `minSize`, `maxSize`, per-output
   PlacementArea, requested rectangle, and fresh observed rectangle. The
   pre-plan line uses `requested=unknown`, allowing a rejected request to report
   its constraints without a native write. Getter failure is logged as `unknown`
   and cannot change planning or actuation. The entry reads KWin
  6.7.5 public `Window` Q_PROPERTIES (`QSizeF minSize`, `QSizeF maxSize`, and
  `bool resizeable`); it does not inspect captions, raw native identifiers, or
  D-Bus payloads. Post-signal evidence is emitted once only for a mismatched
  non-pointer geometry write, including a contained height mismatch such as
  W3's `1092 -> 1036` frame.
- The selected recoverability rule applies to this local discrepancy: its
   `snapshot-invalid` reply is a failed operation, not a disabled window or
   domain. Existing bounded reconciliation may stop automatic reflow after
   three mismatches, but later explicit commands and fresh valid observations
   remain enabled. This does not relax stale/owner/input fences, claim the
   `1036` frame satisfied the `1092` allocation, replay an uncertain command,
   or alter the separate R4 terminal-transfer policy.
- Rust validates carried rectangles only for containment in their supplied
  domain, not equality with retained allocation rectangles. Therefore a later
  explicit move carrying the contained `1036` W3 frame is not rejected for a
  `1092` canonical mismatch. Adapter parking blocks only automatic
  reconciliation; a successful explicit move clears that parking. The local
  baseline correction remains limited to removing fabricated target membership.

## Outcome

- The active `DescribePlan` route validates complete two-domain evidence and
  applies cross focus without geometry or membership writes.
- R4 stages a transient canonical pair and returns `planned`; it does not
  structurally commit until exact `directional-move-ack` and
  `directional-move-verify` post-observation messages. The terminal boundary
  splits state back to canonical per-domain sessions.
- The KWin adapter uses public KWin 6.7.5 `workspace.sendClientToScreen`, exact
  desktop assignment, `outputChanged`, `desktopsChanged`, and active-window
  readback to follow once mover output plus exact desktop membership are proven.
  Full `frameGeometryChanged` readback still gates acknowledgement, verification,
  and commit, so an unsettled sibling cannot strand confirmed mover follow or
  relax the structural proof. It refuses/terminates on stale scope, timeout,
  wrong output, failed write, or incomplete proof, with no replay.
- Offline verification completed 2026-09-20: `cargo fmt --check`, `cargo test`
  (299 unit tests plus all integration suites), `npm --prefix kwin run
  typecheck`, `npm --prefix kwin run test` (808 passed, 0 failed), and `npm
  --prefix kwin run build` (497.2 kB bundle).
- The user manually confirmed the rightward native transfer rendered as
  `left W1`, `right H[W2 W3]`; the trace also records its native writes. This
  establishes public-slot invocation and compositor placement, but not ack,
  verify, or commit because the unchanged-membership flight timed out.
- The full corrected same-workspace outbound-and-immediate-reverse sequence
  remains the required live acceptance boundary; no broader live correctness
  claim is made.
- The premature intermediate-geometry fence consumption is corrected and
  statically verified. The Ghostty `1012x1036` final frame is a separate,
  unresolved native/client constraint observation: it is neither attributed
  nor accepted, and the next trace must establish exact geometry plus commit
  or the bounded timeout before local or reverse movement is evaluated.
