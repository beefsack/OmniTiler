# Windows owner event-driven idle

## Goal and scope

- Reduce non-game Windows idle CPU cost toward zero without changing shared-core or KDE behavior.
- Replace the owner's unconditional 100 ms maintenance poll with blocking event waits and specifically justified, armed-only deadlines.
- Preserve non-blocking low-level hooks, input wake delivery, hide-before-drop acknowledgements, recovery and clean stop.
- Physical game measurements and physical-input latency remain separate acceptance work.

## Acceptance and scope limits

- Release idle evidence with trace disabled/enabled, both empty and with managed test windows: CPU time per minute, thread CPU and context-switch/wait evidence.
- Complete periodic-wake inventory with each retained period's specific reason.
- Regression coverage for event delivery, deadlines, request/settings changes and missing wakes.
- Scripted workspace switching and clean-stop comparison; supported preview/input checks without substituting synthetic input for physical evidence.
- Locked Windows package build/tests, formatting, strict all-target clippy and whitespace checks; CI results recorded separately from local Windows evidence.
- Implementation and offline verification delivered; post-correction live remeasurement and physical-input acceptance remain pending, as detailed below.

## Approach

- Owner waits on existing input/render/message signals plus directory-change notifications for request/stop files and settings.
- Modifier-only hook transitions and asynchronous completion results explicitly wake the owner.
- Replace timer riders with native events or computed deadlines. Do not remove active-gesture timing without evidence that pointer-only preview updates remain correct.
- Supersede the owner-maintenance exception in the Windows threading decision with explicit event delivery and armed deadlines.
- Keep directory notification rearming race-safe: changes concurrent with inspection must remain observable before the next blocking wait.

## Outcome

- Windows-crate-only change; shared core, protocol and KDE behavior unchanged.
- Removed unconditional owner 100 ms polling, healthy 2 s monitor polling and redundant 5 s pager-secondary revalidation. Input/render idle waits were already event-driven in the measured baseline.
- Owner blocks on messages, existing input/render events, request/settings directory changes, and asynchronous icon/gaming completion. Modifier and Esc transitions explicitly wake it without making low-level callbacks wait for owner work.
- Ledger notifications watch names rather than log-content writes. Workspace/resize already publish complete requests atomically; stop now also publishes atomically, preserving overwrite and identity fencing. Settings retain content notifications for external in-place edits.
- Diagnostics neither arm a deadline nor force reconcile; throttled summaries ride natural turns, with a final read-only counter flush.
- Independent review corrected missing pending-finish deadlines, completion-handle lifetime, saturated-input retry handshakes and pager-backoff zero-timeout spinning. Regression coverage includes adversarial drain/refusal interleaving and notification rearming.
- Test isolation corrected: new stalled-input tests take the existing global-fence guard before clearing process-wide Esc/policy state. Earlier parallel-suite failures are not classified as proven pre-existing.
- Final local four-package build, tests, formatting, strict all-target clippy and whitespace checks passed after the test-isolation correction: 2,494 tests passed, 0 failed, 1 ignored, including Windows library 1,046 passed, 0 failed, 1 ignored. CI is reported separately.

## Baseline evidence

- Release, scoped runs, 15 s settling and approximately 79 s CPU sample windows; non-game evidence only.
- Empty/trace off: 500.0 ms CPU per 79.4 s (0.630% of one core).
- Empty/trace on: 562.5 ms per 79.4 s (0.708%).
- Managed windows/trace off: 453.1 ms per 79.5 s (0.570%).
- Managed windows/trace on: 671.9 ms per 79.6 s (0.844%).
- Owner wait rate approximately 9.5/s; input and render idle wait deltas zero. Trace-off thread counter samples approximately 34-52 context switches/s, concentrated on the owner; instance churn limits exact thread attribution.
- Scripted workspace CLI round-trip: 217-281 ms, including process startup and 50 ms observation granularity. Clean stop: 80-288 ms.
- Physical Win+arrow and drag-preview latency unmeasured: injected input is intentionally ignored.

## Measured comparison

CPU milliseconds per minute, release scoped runs, non-game idle only:

| Case | Before | Measured after | Reduction |
| --- | ---: | ---: | ---: |
| Empty, trace off | 377.8 | 7.9 | 97.9% |
| Empty, trace on | 425.0 | 104.0 | 75.5% |
| Managed windows, trace off | 341.9 | 85.2 | 75.1% |
| Managed windows, trace on | 506.7 | 116.4 | 77.0% |

- These after samples precede the final name-only ledger-watch correction. The trace runs still included unnecessary log-content notification wakes, fixed subsequently and verified offline; final-release live remeasurement remains pending.
- After CPU sample windows were 117-121 s after 15 s settling. CPU accounting quantum was 15.625 ms, approximately 7.8 ms/min uncertainty; the empty trace-off result was only one quantum and is not a precise performance floor.
- After counters were mapped to thread IDs on each sample. Context switches were bursty 0-44/s, mostly owner-thread activity; two counter sampling errors were retained as errors. Most CPU was on owner, with at most two quanta on another thread in managed cases. Do not infer thread roles for unnamed threads.
- Trace owner waits fell from approximately 9.5/s to 1.1-1.6/s over whole runs. Approximately 0.5/s were visible-pager maintenance deadlines; remaining notifications included the subsequently corrected log-write feedback. Input/render idle wait deltas remained zero. No repeated idle full reconciliation remained after removing diagnostic-only deadlines.
- Scripted workspace select: 153/180/217 ms versus baseline 217/245/281 ms, all successful. Process startup and 50 ms observation granularity prevent a precise product-latency claim; ranges overlap.
- Clean stop: 75-210 ms versus baseline 80-288 ms; all scoped owners exited, recovery/restore succeeded, request/ledger files cleared and changed state restored. Non-target terminal windows were excluded and left unchanged.

## Retained deadlines and reasons

| Deadline | Armed condition and specific reason |
| --- | --- |
| 100 ms gesture sampling | Active gesture only; pointer preview can change without a target-window location event. |
| 500 ms pending finish | Existing hide-proof refusal bound; expiry refuses actuation, never bypasses acknowledgement. |
| Restore observation deadline | Existing bounded asynchronous restore observation, not steady maintenance. |
| 5 s doubling to 60 s hook retry | Failed legacy hook setup only; bounded degraded recovery. |
| 5 s pager/icon creation retry | Failed setup only; retry unavailable resources without hot spinning. |
| 2 s visible-pager reassert | Existing topmost recovery contract; silent z-order changes have no established reliable wake. Combined with creation backoff using the later deadline to avoid zero-timeout spinning. |
| 300 s gaming refresh | Recheck out-of-file facts that have no established change event; no overlapping probe. |
| 2 s tray-less monitor fallback | Missing normal owner broadcast window; retain display rediscovery in degraded runs. |
| 100 ms watcher-failure fallback | Failed notification setup/rearm only; preserve requests/settings/stop while retrying the watcher. |
| Bounded run end | Explicit proof/run duration, not steady maintenance. |

- Focus settlement (10 x 50 ms), hide confirmation (25 ms within a 2 s budget), render finish/teardown waits, modal-tray stop checking and input/render contention or setup retries remain operation-scoped, not healthy idle polling.
- Healthy hidden-pager idle has no periodic maintenance deadline, with trace enabled or disabled. Visible pager maintenance was armed in every measured live run.

## Decisions and pending evidence

- No arbitrary replacement chord polling interval: modifier edges already reach the native hook and can signal the owner.
- Existing input/render pumps already block at healthy idle; historical 10 ms polling is not present in this baseline.
- Physical game comparison remains user-owned. No game-performance claim follows from idle results.
- Directory watches rearm before inspection; modifier-only transitions, refused-input capacity recovery and asynchronous completions explicitly signal the owner.
- Missing hide acknowledgement must still trigger the existing 500 ms refusal deadline, never authorize a drop without proof. Completion callbacks retain owned event lifetime through shutdown.
- Pending: final post-log-watch-correction four-case live matrix; hidden-pager indefinite-wait live evidence; physical workspace/Win+arrow focus/move and drag-preview responsiveness; pager auto-hide/chevron change-response checks; physical game comparison. Startup pager appearance is not proof of later taskbar-change response. External settings live-edit behavior was not exercised; notification/last-good contracts have offline coverage.
- Retain the approved visible-pager 2 s reassert for now. Options for a later slice are proving a complete native z-order wake source or choosing a changed recovery/staleness contract; do not silently delete the period or add a replacement global poll.
- Production settings and ledger directories differ. A same-directory seam intentionally retains content notifications for settings fidelity and can still wake on colocated log writes; no performance claim covers that configuration.
