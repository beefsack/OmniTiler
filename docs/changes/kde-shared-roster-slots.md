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

Design approved; implementation and native acceptance are pending.
