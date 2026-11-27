# macOS Frame-Lane Settlement

## Goal and scope

Preserve pending explicit float-frame writes through per-app coalescing and
report ordinary Controlled AX frame attempts to the existing core settlement
policy. Convert the visible work area inward and expose the raw AppKit input
for native diagnosis. No wire, core policy, full-display geometry, or fixture
harness changes are intended.

## Acceptance

- A second accepted reply must not drop an undrained placement write while
  the operation is still published; withdrawal and supersession retire the
  old write. Generation changes fence old work, and spent state is bounded.
- A Controlled one-point clamp should write once, hold under unrelated
  reports, and retry on genuine change. Preserve immediate successful
  readback; isolate Layout correlation and staging from FloatPlacement;
  stage no attempt for a pre-call safety fence. Use one process clock for
  attempts and reports and verify affected time-sensitive policies.
- Keep integer work-area edges within a fractional visible frame, preserving
  the full-display projection; log the raw `NSScreen.visibleFrame` points.
- Reconcile the earlier F2 diagnosis with the verified defects and keep the
  native cause of the historical shortfall distinct from the retry defect.

## Approach

- Carry explicit-write authorization separately from the newest set of frame
  commands. A suppressed duplicate is not withdrawal; a held or absent
  token does not authorize stale native work.
- The report contract permits one frame row per window. Keep the pending
  lanes distinct, with explicit placement taking priority when both have
  evidence for one lifetime. Use a shared clock for frame settlement and
  the native-creation receipt freshness checks already in core.
- Queue completed writes and the owning app's changed observation together.
  Other reports cannot drain a new Layout attempt against the app's
  pre-write frame; unchanged observations still use the normal dedup path.
- Snap visible minima up and maxima down. Continue to use the separate
  full-display rectangle for output topology.

## Offline evidence

- Frame-slot regressions cover a later accepted reply before worker drain,
  withdrawal and held-to-eligible republish, and the bounds and lifetime of
  spent state.
- The real report and reply builders carry a one-point Controlled clamp;
  once the post-write observation is present, subsequent unrelated reports
  hold it and genuine changes make it eligible again. Separate cases cover
  burst re-arm, refusal backoff, placement-vs-layout isolation and a typed
  pre-call fence.
- The mixed-scale fractional producer fixture exercises the raw-to-report
  work area and preserves exact shared-edge full-display adjacency.

## Native evidence boundary

The signed maximize check was not run: two displays attached. The
existing fixture controller chooses native row zero rather than pinning
the AppKit primary. Its normal teardown removes the product log, so the
existing maximize verdict alone cannot prove a clamp hold or preserve both
raw work-area samples. Exact native maximize and directional half-snap
remain scoped dogfooding checks. The historical KDE Ghostty-class
shortfall is separate and its native cause remains unproven.
