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
- Implementation, offline verification and the post-correction eight-case idle CPU matrix delivered. Visible timeout attribution, chevron response and physical-input/game acceptance remain open, as detailed below.

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

- These after samples precede the final name-only ledger-watch correction. The trace runs still included unnecessary log-content notification wakes, fixed subsequently and verified offline; post-correction samples are recorded separately below.
- After CPU sample windows were 117-121 s after 15 s settling. CPU accounting quantum was 15.625 ms, approximately 7.8 ms/min uncertainty; the empty trace-off result was only one quantum and is not a precise performance floor.
- After counters were mapped to thread IDs on each sample. Context switches were bursty 0-44/s, mostly owner-thread activity; two counter sampling errors were retained as errors. Most CPU was on owner, with at most two quanta on another thread in managed cases. Do not infer thread roles for unnamed threads.
- Trace owner waits fell from approximately 9.5/s to 1.1-1.6/s over whole runs. Approximately 0.5/s were visible-pager maintenance deadlines; remaining notifications included the subsequently corrected log-write feedback. Input/render idle wait deltas remained zero. No repeated idle full reconciliation remained after removing diagnostic-only deadlines.
- Scripted workspace select: 153/180/217 ms versus baseline 217/245/281 ms, all successful. Process startup and 50 ms observation granularity prevent a precise product-latency claim; ranges overlap.
- Clean stop: 75-210 ms versus baseline 80-288 ms; all scoped owners exited, recovery/restore succeeded, request/ledger files cleared and changed state restored. Non-target terminal windows were excluded and left unchanged.

## Post-correction live matrix, 2026-10-17

- Release source `c785d98f1324244071376c294afb92ef7bdf658b`, clean tree;
  includes owner-idle `1a4f82ad`, its log-watch correction and Windows FFI
  consolidation/correction `1fe27640`/`f42691af`. Rebuilt with
  `mise exec -- cargo build --locked --release -p tiler-windows`.
- SHA-256: owner `071ad55c77c0e58a80b8b966a60837c66b3318448d8abe0550e1a379df1cf7d0`;
  disposable helper `dd842b5ea5683f4ac2712e0d82ad5276f9c558cd00ceb2406ffbe22121d6e722`.
  Windows 11 physical host, medium integrity, single output. Exact OS/display,
  executable/PID/start identities and raw responses remain in external evidence,
  not repository machine records.
- Launch: `tile --user-start --scope-exe tiler-test-window.exe`, adding
  `--trace` for trace legs. Managed cases used two identity-tagged helpers;
  other applications were excluded. No keyboard injection. Workspaces were
  idle during sampling, then switched through the exact-owner CLI.
- Each leg settled for 15 s, then sampled process CPU over 152.7-154.5 s wall
  time, including sparse thread-counter collection. Hidden cases enabled
  taskbar auto-hide; visible cases disabled it. No game was run.

| Workspace contents | Pager | Trace | CPU ms/sample | CPU ms/min |
| --- | --- | --- | ---: | ---: |
| Empty | Visible | Off | 93.8 | 36.4 |
| Empty | Visible | On | 156.2 | 61.4 |
| Two helpers | Visible | Off | 140.6 | 54.7 |
| Two helpers | Visible | On | 140.6 | 54.8 |
| Empty | Hidden, auto-hide | Off | 0.0 | 0.0 |
| Empty | Hidden, auto-hide | On | 15.6 | 6.1 |
| Two helpers | Hidden, auto-hide | Off | 0.0 | 0.0 |
| Two helpers | Hidden, auto-hide | On | 15.6 | 6.1 |

- Accounting quantum was 15.625 ms, about 6.1 ms/min for these windows.
  Hidden results are granularity-limited near-zero observations, not proven
  zero CPU. These are single samples, not confidence intervals. In particular,
  the earlier 7.9 ms/min empty-visible sample and current 36.4 are different
  revisions/windows; quantization alone does not explain their difference,
  and neither regression nor improvement is established by that comparison.
- Surviving-thread snapshots concentrated CPU in one thread, except two in
  managed trace-off. Thread churn prevents complete lifetime attribution;
  rounded agreement with process totals does not prove exited-thread zero CPU.
  Context-switch reads were 7-9 sparse instantaneous samples per leg, not
  totals/averages: visible samples ranged 0-28.7/s; hidden samples read 0/s.
  Counter errors were retained, not converted to zero, including two in the
  visible trace-empty leg.
- Hidden trace snapshots: empty owner timeout delta 0 across 146.4 s;
  managed delta 0 across 178.5 s. No periodic pager timeout was observed;
  CLI/completion notifications still woke the owner. This is bounded live
  evidence supporting the blocking-wait design, not proof of forever-idle:
  the retained 300 s gaming refresh lies outside these windows.
- Visible trace snapshots, including startup and CLI operations: empty
  +107 waits/+98 timeouts over 174.7 s; managed +175/+102 over 182.7 s.
  Timeout counts (~0.56/s) exceed nominal 2 s pager maintenance (~0.50/s).
  Aggregate counters also include staged-work zero-timeout collects and
  cannot identify the cause of every excess return. No new periodic timer
  or log-write feedback loop was established; attribution remains open.
- Managed visible trace contained 60 icon-result batches, all misses,
  approximately paired every 6 s, correlated with completion notifications
  (+64 across the run). This is the already-approved 5 s negative icon-cache
  retry serviced on visible 2 s turns, not startup-only work or a new
  independently armed timer. Hidden managed trace had no icon-result batches.
  Positive 10 s refresh was not exercised by the iconless helpers.
- Later taskbar response passed: visible pager -> auto-hide enabled, no pager
  -> auto-hide disabled, visible pager recreated. This proves the exercised
  later change, not just startup appearance; no precise latency was measured.
  Chevron response is unmeasured: UI Automation/Win32 discovery did not expose
  a verified click target. That is not proof the chevron is physically absent
  or that tray-geometry response has no contract; no speculative click was made.
- All eight legs' workspace selects completed `ok` in owner logs. Later
  idle-matrix harness legs measured 43-161 ms dispatch-to-observation, including CLI
  startup; the first leg's incomplete parsed result was checked in its log.
  This is not physical-input or isolated product-latency evidence.
- Graceful stop took 76-86 ms; standalone restore succeeded and ledger/request
  state cleared. A separate exact-owner emergency-stop/restore also succeeded.
  Final taskbar state matched its preimage, no owner/watch-owner/helper or
  pager overlay remained, and no hidden-window claims remained. Managed hidden
  helper visibility/styles/rectangles matched their post-tiling baseline before
  exact close; this does not claim restoration to original pre-tiling geometry.
  Owner logs report Mouse Snap disabled on resume and restored at teardown;
  that restore path verifies its SPI write. A separate final read-only
  `SPI_GETWINARRANGING` returned true. No independent pre-run SPI readback was
  captured, so there is no independent pre/post comparison; standalone
  `mouse_snap_restored:false` is only a no-op report, not restoration proof.
  One failed harness setup was stopped/restored; its partial sample is excluded.
- Evidence set `idle-matrix-20261017-postcorr`: case-prefixed
  `measure/provenance/workspace/waits/stop` JSON, helper readbacks,
  `G-pager-response.json`, `preimage.json` and `restore-verify.json`.
  Independent evidence/source review accepted the measurements with the
  attribution, granularity, restoration and chevron limits above.

### Open finding and next action

- Visible timeout excess is not causally resolved. Options: (a) add trace-only
  per-timeout attribution (staged flag, selected deadline and wait outcome),
  then repeat one bounded visible leg; (b) retain the aggregate uncertainty
  without a stronger wake-frequency claim. Recommendation: (a), before changing
  any timeout calculation or declaring every residual wake accounted for.
  No runtime fix was made: changing retry cadence would override the existing
  icon-freshness decision, and the counters do not establish a small causal fix.
- Chevron response needs a verified safe target or user-run observation.
  Physical workspace/Win+arrow focus/move, drag-preview and game comparison
  remain pending and user-owned. No game-cost or physical-responsiveness claim
  follows from this matrix. External settings live-edit remains unexercised.

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
- Artwork cache age thresholds are also retained: negative entries retry after
  5 s because applications set icons late; positive entries refresh after 10 s
  because Windows has no icon-change event. These approved thresholds are
  checked on existing visible-pager turns, not independently armed deadlines;
  worker completions add event wakes. Hidden/non-drawable artwork is not queued.
- Healthy hidden-pager idle has no periodic pager-maintenance deadline, with trace enabled or disabled. The separate 300 s gaming refresh remains. Visible pager maintenance was armed in the visible live cases.

## Decisions and pending evidence

- No arbitrary replacement chord polling interval: modifier edges already reach the native hook and can signal the owner.
- Existing input/render pumps already block at healthy idle; historical 10 ms polling is not present in this baseline.
- Physical game comparison remains user-owned. No game-performance claim follows from idle results.
- Directory watches rearm before inspection; modifier-only transitions, refused-input capacity recovery and asynchronous completions explicitly signal the owner.
- Missing hide acknowledgement must still trigger the existing 500 ms refusal deadline, never authorize a drop without proof. Completion callbacks retain owned event lifetime through shutdown.
- Post-log-watch-correction eight-case CPU matrix, bounded hidden-pager wait
  evidence and later auto-hide response are measured above. Visible timeout
  attribution and chevron response remain open, alongside physical
  workspace/Win+arrow focus/move, drag-preview and game comparison. External
  settings live-edit behavior was not exercised; notification/last-good
  contracts have offline coverage.
- Retain the approved visible-pager 2 s reassert for now. Options for a later slice are proving a complete native z-order wake source or choosing a changed recovery/staleness contract; do not silently delete the period or add a replacement global poll.
- Production settings and ledger directories differ. A same-directory seam intentionally retains content notifications for settings fidelity and can still wake on colocated log writes; no performance claim covers that configuration.
