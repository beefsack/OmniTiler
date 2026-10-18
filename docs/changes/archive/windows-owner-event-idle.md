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
- Implementation, offline verification, the post-correction eight-case idle CPU matrix and visible timeout attribution/correction delivered. Chevron response and physical-input/game acceptance remain open, as detailed below.

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
  or log-write feedback loop was established by these samples; the follow-up
  below attributes the timeouts and corrects fractional-deadline conversion.
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

## Timeout attribution and correction, 2026-10-18

- Added trace-only timeout-source, staged-collect, early-return and zero-request
  counters to existing `pump-waits` summaries/final flush. Source selection uses
  one deadline scan; staged work still skips it. Clock reads and diagnostic
  accumulation are trace-gated. No diagnostic deadline, wake or reconcile added.
- Release empty-visible-pager trace legs used the same scoped launch as above,
  15 s settling, no helpers, keys or CLI operations during idle sampling. Source
  was `62284df5` plus the recorded Windows diagnostic/conversion diff. The
  initial attribution-only leg observed 123 timeouts over 226.840 s, all pager,
  including 10 zero-ms requests; no staged, notification or other deadline cause
  explained the excess. Refined attribution and the final correction followed:

| Conversion | Idle counter span | Pager timeouts | Timeouts/s | Zero requests, already due / future | CPU ms/min |
| --- | ---: | ---: | ---: | ---: | ---: |
| Floor, refined attribution | 190.8 s | 100 | 0.524 | 5 / 0 | 63.0 |
| Ceil, final artifact | 188.856 s | 94 | 0.498 | 0 / 0 | 64.1 |

- All steady-idle timeout causes were pager maintenance; staged and other
  deadline timeout buckets were zero. Before correction, five summary intervals
  contained multiple timeouts, matching approximately five excess returns over
  the nominal 2 s cadence. The final sample had none. One early requested-duration
  return was counted in each refined leg; it did not create an extra final-leg
  timeout. These counters do not prove a particular OS timer-quantum mechanism.
- Cause and fix: flooring a fractional remaining duration can request a wait
  shorter than the actual deadline. Pager sync rearms only once that deadline
  is due; an early service turn can leave the same deadline for another return.
  The before-leg zero requests were already due at the next selection, not
  directly observed future-submillisecond zeros. Round future remaining
  nanoseconds up to whole milliseconds with `div_ceil(1_000_000)`, preserving
  past-due zero, unarmed `INFINITE`, clamping, minimum/tie order and degraded
  caps. This removes the deterministic under-request, not the approved 2 s
  contract. Conversion adds less than 1 ms versus the exact future duration;
  hide-before-drop still refuses without acknowledgement.
- CPU windows were approximately 193.5/190.0 s, distinct from the counter-line
  spans. The 15.625 ms accounting quantum is about 4.9 ms/min here. Single
  samples support the observed timeout-count reduction, not a demonstrated CPU
  improvement or a precise floor. No physical-input, preview or game claim.
- Owner SHA-256 before: `c6ada4fc3f99e0892705ccd1c2710a75103a831c2d4fba9aeb2e823a8bec0f74`;
  final: `b1dfb125fb49b4de90c0adf18a0b0e51d79cb66c5e229e5652e6ea9cc70c98b8`.
  An intermediate microsecond-based ceiling missed sub-microsecond boundaries;
  repaired to nanoseconds, regression-tested and remeasured on the final artifact.
  No performance claim uses the intermediate sample.
- Independent SPI pre/post reads matched; taskbar state matched its preimage.
  Final graceful stop was 93 ms, standalone restore succeeded, and owner/helper
  processes, pager overlays and ledger/request residue were absent. Raw identity,
  display, baseline and restoration responses stay in external evidence sets
  `idle-matrix-20261018-zero-split-before` and
  `idle-matrix-20261018-ceil-nanos` (`leg.*`, `waits-lines.jsonl`,
  `spi-compare.json`, `restore-verify.json`); initial attribution is in
  `idle-matrix-20261017-timeout-attr`.
- Independent final source/evidence review accepted the bounded attribution and
  correction. Locked four-package build/test/strict all-target clippy passed
  through `mise exec --` for `tiler-core`, `tiler-protocol`,
  `tiler-kwin-effect-ffi`, `tiler-windows`: 2,601 passed, 0 failed, 1 ignored.
  Workspace formatting and whitespace checks passed. Regression coverage
  includes 1 ns, 999 ns, whole-ms-plus-1 ns, exact ms, past-due, clamping,
  degraded caps, cause ties and staged/deadline attribution. CI is separate.
- Chevron response needs a verified safe target or user-run observation.
  Physical workspace/Win+arrow focus/move, drag-preview and game comparison
  remain pending and user-owned. No game-cost or physical-responsiveness claim
  follows from this matrix. External settings live-edit was not exercised by
  this matrix; a later bounded gap-edit follow-up is recorded below.

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

## Native z-order wake research, 2026-10-18

No implementation or recovery-contract change selected. Current pager reasserts
with `SetWindowPos(HWND_TOPMOST, ...NOACTIVATE...)`; being topmost does not
prevent another topmost window from covering it. Existing hooks cover
foreground, create/destroy/show/hide and location changes, not reorder.

| Candidate | Coverage and remaining gap |
| --- | --- |
| [WinEvents](https://learn.microsoft.com/en-us/windows/win32/winauto/event-constants): `EVENT_OBJECT_REORDER`, foreground/show/hide/location | Reorder documents container children and parent-reported child-window z-order changes; promising to test, not a documented guarantee of every top-level/topmost burial. Foreground and lifecycle/geometry events cover their own changes, not a guaranteed pure nonactivating reorder. Do not assume a desktop-parent event survives existing HWND/object filters. |
| Own [`WM_WINDOWPOSCHANGED`](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-windowposchanged) | Reports the recipient's own position/size/z-order changes. Not documented as a notification to the pager whenever some other window covers it; reassert-generated messages also need feedback suppression. |
| [`RegisterShellHookWindow`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registershellhookwindow) | Activation, creation/destruction and other shell events, but no documented complete arbitrary-topmost-reorder notification. Microsoft warns this API is not intended for general use and may change/disappear. |
| AppBar notifications | [`ABM_NEW`](https://learn.microsoft.com/en-us/windows/win32/shell/abm-new) registers an AppBar/callback; `ABN_POSCHANGED` covers taskbar/appbar geometry/visibility, `ABN_STATECHANGE` taskbar state, `ABN_FULLSCREENAPP` fullscreen transitions, `ABN_WINDOWARRANGE` arrangement. None documents all foreign topmost reorders. Registration alone is not a reservation (`ABM_SETPOS` negotiates space), but it changes the deliberately non-AppBar hosting contract; not a free notification subscription. Current `ABM_GETSTATE` is a read, not an event. |
| `TaskbarCreated`, settings/display/DPI broadcasts | Existing shell rediscovery/calibration wakes, not complete foreign z-order coverage. |

- [`SetWinEventHook`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook)
  out-of-context delivery avoids injection, queues asynchronously on the
  installing message-loop thread, and can reenter. Any candidate must route
  bounded filtered/coalesced work through the owner, handle desktop/container
  events correctly, and suppress self-feedback; broad subscriptions can add
  event cost rather than remove it. No callback-heavy recovery or new global poll.
- Observed residual visibility-associated cost: trace-off visible empty/managed
  36.4/54.7 ms/min versus hidden 0.0/0.0; trace-on 61.4/54.8 versus 6.1/6.1.
  Hidden readings are granularity-limited. These deltas include all visible
  pager work, icon refresh and sample variation, not isolated timer cost.
  Corrected empty-visible trace still measured 64.1 ms/min at 0.498 timeouts/s;
  no game-cost acceptance or CPU saving is established.
- Recommendation, parked for a later decision: investigate event-first recovery
  with an owned topmost-cover discriminator (nonactivating z-order-only changes,
  show/hide/close and own-window changes), retaining the timer until coverage,
  filtering, no feedback and recovery latency are evidenced. A silent burial
  disproves completeness; successful repeats alone do not prove universal
  coverage. Then choose either sufficiently evidenced native events under the
  existing recovery contract or an explicitly weaker best-effort/staleness
  contract. No such choice is made here. Removing the 2 s service turns also
  needs an explicit decision on the approved 5/10 s artwork freshness that
  currently rides them; do not silently strand refresh or add replacement polls.

Primary documentation above checked 2026-10-18; AppBar notification details:
[position](https://learn.microsoft.com/en-us/windows/win32/shell/abn-poschanged),
[state](https://learn.microsoft.com/en-us/windows/win32/shell/abn-statechange),
[fullscreen](https://learn.microsoft.com/en-us/windows/win32/shell/abn-fullscreenapp),
[arrangement](https://learn.microsoft.com/en-us/windows/win32/shell/abn-windowarrange).

## Decisions and pending evidence

- No arbitrary replacement chord polling interval: modifier edges already reach the native hook and can signal the owner.
- Existing input/render pumps already block at healthy idle; historical 10 ms polling is not present in this baseline.
- Physical game comparison remains user-owned. No game-performance claim follows from idle results.
- Directory watches rearm before inspection; modifier-only transitions, refused-input capacity recovery and asynchronous completions explicitly signal the owner.
- Missing hide acknowledgement must still trigger the existing 500 ms refusal deadline, never authorize a drop without proof. Completion callbacks retain owned event lifetime through shutdown.
- Post-log-watch-correction eight-case CPU matrix, bounded hidden-pager wait
  evidence and later auto-hide response are measured above. Visible timeout
  attribution and fractional-deadline correction are delivered; chevron
  response remains open, alongside physical
  workspace/Win+arrow focus/move, drag-preview and game comparison. External
  settings live-edit was not exercised by the idle matrix; the later scoped
  follow-up below samples gap adoption, not the full notification/last-good
  matrix.
- Retain the approved visible-pager 2 s reassert for now. Options for a later slice are proving a complete native z-order wake source or choosing a changed recovery/staleness contract; do not silently delete the period or add a replacement global poll.
- Production settings and ledger directories differ. A same-directory seam intentionally retains content notifications for settings fidelity and can still wake on colocated log writes; no performance claim covers that configuration.

## Scoped native follow-up (2026-10-18)

- Source `1ae85779` with documentation-only working changes; normal scoped
  owner, two identity-tagged disposable helpers, one output, ordinary integrity,
  trace off and no keyboard injection. Owner SHA-256
  `366a3619a0b9dcdedfd4960453a03da3e0ea55b2a0d8e75302275fa7bd37086a`;
  helper `3825ce9afe3bbee10db22d9e09e24623a609a98812b81a34a648a40b2e6fdbb9`.
- External settings gap edit 8 -> 12 -> 8 reached `settings-applied` with
  `lanes:["gaps"]` both ways, and the helper frame changed after the first edit.
  Each in-place write was followed by
  `settings-degraded:malformed`, followed 9 ms later by applied. This measures
  the interval between logged observations, not write-to-wake latency. The
  edited-gap reconcile reported two mismatches; restoration reported zero.
  Wake/adoption is observed, not complete geometric-convergence acceptance.
  Diagnosis remains limited: this run used DPI-unaware helpers, while owner
  diagnostics use physical coordinates, and no edited-gap per-window
  planned/effective/readback entry was retained. Do not mix those units or infer
  a new product defect from aggregate mismatches. A matched PMv2-helper gap-edit
  readback is the next discriminator; no geometry policy or source fix selected.
- Chevron response **not run**: taskbar child discovery returned one entry
  without a class, no verified chevron/overflow target and no clicks. This
  neither establishes physical absence nor closes tray-geometry response.
- Workspace select-away/return logged successful two-window hide/reveal and
  focus on return (26/37 ms total actions); this supplements already accepted
  healthy switching, not physical-input or deadline-expiry evidence.
- Non-hooking stop/restore and bounded owned-helper hide/restore succeeded.
  Final `mise exec -- just stop` reported clean; input/render shutdown reported
  clean, helpers exited and ledger/stop/workspace requests were absent.
  Restoration evidence is limited: settings were compared as decoded text,
  SPI values matched but getter success flags were discarded, and taskbar
  comparison covered window metadata rather than `ABM_GETSTATE`. Do not claim
  an independent byte-exact/settings/SPI/taskbar-state restoration proof from
  this run. No taskbar-setting mutation was attempted.
- External evidence `wu-sweep-20261018-164250-30608`: `report.json`,
  `settings.pre.json`, `final-verify.json`, inspected harness and owner log
  `run-01dd5ec4460b8c23.log`. Chevron, physical responsiveness and games remain
  pending; the visible-pager 2 s timer is unchanged.
