# KDE shared native roster slots

## Goal and scope

Implement the [approved slot policy](../decisions/workspaces.md#dynamic-workspace-lifecycle-and-kde-pager-backing)
using the [roster-plan contract](../porting/world-contract.md#shared-native-roster-slot-plans).
Only separate workspace modes on a host-owned shared roster with independently
writable per-output selectors take this path. Keep the other routes unchanged.

## Units and dependencies

1. Core plan and tests: project lifecycle before publication, retain settled
   and desired epochs separately, and replace shared-route joint retirement,
   spare fan-out and local markers. Cover slot assignment, padding navigation
   and native selection, count changes and fences, migration, partial proof
   and replay. Preserve independent-roster and linked-mode regressions.
2. KWin roster-plan apply: after the core plan, wire exact occupancy,
   operation/epoch authority and readbacks through the contract. Test the real
   capture/core/apply pipeline on a fake host: setter arguments and order,
   refusal and external invalidation, arrival-before-follow, sibling isolation
   and zero mutations on settled replay.
3. Run the acceptance gate after the offline path is proved.

## Acceptance

- Run the same send-and-follow on independent rosters, on KDE's shared roster
  with per-output switching enabled, and on KDE with the API present but the
  setting off. Assert sibling current workspace, members and history, not just
  desktop count. The independent route leaves sibling bindings and state
  alone; the setting-off route takes only existing effective shared behavior,
  reports `unsupported-mode` for the separate preference, and neither issues
  a supposedly local switch nor rewrites the saved preference. Check shared
  behavior as linked, not against the separate-route isolation oracle.
- Verify both [proposed outcomes](../spec/reference-outcomes/workspaces.md#shared-native-roster-slot-discriminators),
  exact pager IDs/order and final visibility/focus. Separate KWin setters can
  briefly show B or hide A; live flicker and focus decide acceptability. One
  publication and one apply is the clean-path target, not a promise on async
  arrival, refusal or external input.
- Inject a partial apply with B moved and A refused. Record the actual prefix;
  no dependent view switch or destructive step, and neither window is
  reinterpreted under the unproved mapping.

Core planning, typed wire proof and KWin apply are implemented together.
The active KDE recordings exercise the new plan; frozen wire fixtures remain
unchanged.

## Native KDE evidence — `90391310` plus sibling-fence fix

Real Plasma, shared roster, per-output desktop selection enabled. The
send-and-follow callback was invoked through KGlobalAccel, not a physical key.

| Leg | Result and native readback |
| --- | --- |
| Sole mover, occupied prefix | **Pass.** Staged A alone on source D2, B on D3 and a real spare on D4. One plan moved B to D2, then A to D3, then switched the source selector to D3. Counts: 2 membership, 1 view, 0 creates/removes/reorders; exact roster `[D1,D2,D3,D4]` unchanged. A ended visible and focused; B ended off-view. |
| Stayer starting on D1 | **Pass after the fix.** With A and the stayer on source D1, one plan appended D4, moved only A to D3 and selected D3 on the source. Counts: 1 membership, 1 view, 1 create, 0 removes/reorders; exact prefix `[D1,D2,D3]` unchanged. A ended visible and focused; B stayed on D2 and the stayer on D1, both off-view. |

Both final reruns left the sibling selector, logical membership and previous
workspace unchanged; C stayed visible on sibling D1. `TakeCommandResult`
acknowledged command 1 in both runs; the roster feedback proved all three
planned steps, and the following publication settled the switch with focus
proved. The native signal timeline showed B briefly on-view (~7 ms) and A
briefly off-view (~4 ms) during the sole move. In the stayer move B never came
on-view and A was briefly off-view (~4 ms). Neither leg recorded a loss of
focus from A; these are state intervals, not rendered flicker measurements.

Before the fix, the D1 stayer command reached core (`cmdSeq=1`) and retained
A's follow transfer, but an unchanged managed maximized sibling occupant
fenced the shared native slot globally. Core emitted an `unreadable` hold
and no roster plan; KWin had no roster writes to apply. This was neither the
adjacent-spare no-op nor an adapter refusal. The aggregate apply counter
`held=1` preceded the command and does not identify the cause. The fix
scopes that occupant's fence to its own output while preserving complete
occupancy proof and global deletion checks. The focused regression failed
before the fix; afterward the 64 native retirement tests, 2,470 core library
tests, strict core test-target Clippy, and the workspace Rust formatting
check passed. Frozen fixtures remained unchanged.

For every attempted leg, the fixture and helpers were stopped, dev mode was
confirmed DOWN, both outputs were reset to workspace 1, and operator-terminal
focus was confirmed. The independent-roster and per-output-setting-off
controls remain pending user testing; partial apply is covered offline.
