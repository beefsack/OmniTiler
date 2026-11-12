# macOS responsiveness investigation

## Outcome and evidence floor

- Latest: [rerun4 unanalysed observations](#rerun4-unanalysed-observations-2026-11-12)
  record 72 native focus dispatches, close/open command loss/recovery and a
  final send refusal followed by inventory holds. No new diagnosis or fix.
  [Rerun3 fault isolation](#rerun3-fault-isolation-2026-11-12)
  at `632ee7d9` plus working changes. Fixed the adapter's 16-call focus-outbox
  exhaustion. Close-related CG holds clear, but a separate core execution-subject
  recovery defect keeps implicit focus/move commands refusing afterward.
  The dated sections below retain earlier findings and verification as history.
- Kept one small macOS-only lookup optimization in
  `crates/tiler-macos/src/native_facts.rs:514`. Its benefit is measured
  headless, not a demonstrated improvement in interactive latency.
- Timing is off by default. `TILER_MACOS_STAGE_TIMING=1` adds bounded async
  stage records without changing planning, native writes, waits or cadence.
  The current instrumented binaries build; the signed bundle must be rebuilt
  before capture. Signing, launch and native acceptance are not newly proved.
- Goal: locate input, observation, report/codec, apply, rendering and
  settlement costs for focus, move/swap, select, send, admission/close and
  drag/resize. Preserve behavior, safety readbacks and latest-only apply.
- Headless diagnostic gates and the analysis recipe pass. Next native evidence
  requires one new signed, user-run capture; this investigation stays open.
  Coalesced notification receipt is not the exact close event; neither owner
  completion nor readback establishes painted-result latency.

Evidence labels below:

- **Observed live prerequisites**: native preflight and failed controller
  join only; no live latency measurement or action min/median/max.
- **Measured headless**: optimized pure-data lookup comparison below.
- **Historical headless**: earlier `world_eval` measurements in
  [core review C7](core-v4-review.md#c7-latency-is-unbudgeted-and-unguarded),
  not newly measured macOS stages.
- **Static**: code paths, constants and call counts; not elapsed time.
- **Unknown**: no defensible measured duration. Static waits are not an
  estimate of total action latency.

## Earlier fixture capture result (before instrumentation)

- Current-source development binaries were assembled into the documented
  probe/fixture bundle layout, with the persistent development signer and
  macOS 15 deployment floor. Both bundles passed deep/strict signature
  verification (exit 0); embedded binaries matched fresh build artifacts.
  Older signed artifacts differed and were not used as source evidence.
- Bundle-attributed LaunchServices preflight reported AX trusted, unlocked
  on-console session, Secure Input inactive and event-post access; wrapper
  exit 0. Its payload was observed but not retained as a separate file.
- One bounded, post-only `focus-h` controller leg refused with
  `caller w0 has no live native window join; refusing`; wrapper exit 1,
  empty action-report output. This is not an application latency failure.
- `fixture_leg.rs:801-819` registers the fixture, spawns callers, then joins
  them. `join_fixture_windows` (`958-998`) takes one CG snapshot before
  socket reads, requiring exact PID/caller/run-tag agreement. It has no
  eventual-visibility wait. Missing CG visibility, missing native title or
  a differing tag cannot be discriminated by the retained failure output;
  a creation/CG race is plausible, not established.
- Product launch (`fixture_leg.rs:885-893`), admission/bind (`901-909`) and
  posts (`911-922`) were not reached. This post-only leg did not exercise
  the independent away/return proof or create native Spaces.
- Teardown reported current Space restored, no owned residue, awake helper
  reaped and a retained receipt due to a newly appearing protected-window
  mismatch. Exact-process absence and owned run-directory absence were
  also observed. `product_reaped:true` is vacuous here: no product launched.
  Fixture registration clears pending launch accounting
  (`live_leg.rs:855-871`); the registered fixture shutdown path ran.
- Frontmost-focus restoration is unverified: no immediate pre/post-leg
  frontmost PID was captured. `current_restored` covers Space SIDs only;
  fixture exit does not prove return of focus. The regular-policy fixture
  launch may have activated it even though no chord posted. No inferred
  focus restoration or broader corrective actuation was attempted.
- Ownership failure independently stops progression. Additionally, the
  controller's mismatch-based clean gate (`live_leg.rs:1308-1339`) retains
  receipts on baseline drift, whereas the guide's durable default says
  baseline drift alone does not stop a leg. This contract tension is not
  resolved here. The pending receipt was preserved, not removed to retry.
- Procedure deviations: bundle assembly used a new worktree-relative install
  location rather than the previous install path. Preflight succeeded, but
  this does not prove stable-path grant survival. The fixture build's raw
  exit was not captured correctly; successful build output/artifacts are
  supporting evidence only. No raw exit is inferred for that invocation.
  Frontmost-focus restoration lacks the required readback proof.

## Historical static action-path inventory

All source anchors in this section are under `crates/tiler-macos/src/` and
refer to the pre-instrumentation inventory. Use the timing anchors below for
current locations. The admission/close `ax_session.rs` anchor below is test
coverage, not the production callback or a measured notification timestamp.

### Shared keyboard, observation and apply stages

1. **Input**: native tap callback (`ffi.rs:8643`, `native_input.rs:687`)
   performs bounded matching/forwarding, not AX or core work. Input owner
   pump (`native_input.rs:1107-1191`) maps actions through
   `runtime_input.rs:63-163` into the engine control inbox
   (`runtime_thread.rs:355-403`). Control is FIFO, wake-driven; queue and
   lock contention durations are unknown.
2. **Observation/report**: app owners publish scoped snapshots
   (`runtime_worker.rs:95-131,1097-1111,1147-1195`). Input can use retained
   truth; it does not necessarily wait for a new AX inventory first.
   `world_dispatch.rs:733-873` builds a report, transacts, accepts the
   sequenced reply and applies only coherent current results. Stale replies
   do not actuate (`875-911`). Whole-world report construction and pager
   geometry are pure work (`world_builder.rs:130,378`), duration unknown.
3. **In-process boundary/core**: `world_dispatch.rs:115-197` clones the
   report, encodes/decodes it, calls core at `150`, then encodes/decodes the
   reply. Four codec calls plus core are serialized per transaction. Stage
   costs are unknown here; the codec also validates the boundary.
4. **Latest-only apply**: `consume_world_reply` (`world_dispatch.rs:1713-1740`)
   publishes per-app frames (`1747-1873`) and retained focus. Owner commands
   fence lifetime, topology and latest slot; already-observed matching
   geometry skips writes (`runtime_worker.rs:1480-1588`). Each attempted
   frame runs size-position-size with three safety fences and readbacks
   (`native_tile_backend.rs:224-303,313-390`), then one full post-batch
   scoped observation (`runtime_worker.rs:1612-1627`). No frame retry loop.
5. **Rendering/settlement**: border and pager consume presentation, not AX
   planning. Border's dirty fast join is throttled to 33 ms
   (`tile.rs:1760-1772,2544-2570`); 250 ms is the main idle backstop, not a
   mandatory key delay. Border tick reads frontmost/session/accent and
   published facts (`1901-1980`); its frame is publisher-provided, not a new
   main-thread AX/CG inventory. A successful setter is dispatch truth;
   native arrival requires later observations. Neither proves painted UI.

| Action class | Ordered branch after input/observation | Where time can go; evidence |
| --- | --- | --- |
| Focus navigation | `runtime_thread.rs:1694-1739` -> `world_command_map.rs:248` -> shared report/core -> retained focus `world_dispatch.rs:3070,3265-3349,3444-3476` -> owner `runtime_worker.rs:727-999,1258-1300` -> pre/post selected-focus oracle and next observation -> border | Native identity/selection fences, focus dispatch, oracle and owner queue; durations unknown. Quiet owner pump is at most a 20 ms slice in the absence of a wake, not a universal 20 ms charge. Zero live samples. |
| Move/swap | `world_command_map.rs:255` -> shared report/core -> latest per-app frames -> serial fenced size-position-size -> post-batch snapshot -> presentation | Full AX inventories, per-leg rejoins/readbacks and serial per-app work; durations unknown. No fixed debounce/sleep in frame application. Zero live samples. |
| Workspace select | core selection -> serialized off-main `select_owner.rs:160-207` -> `workspace_select.rs:700-929` -> fresh row proofs -> `dock_swipe_select.rs:588-825` -> swipe edges -> event-latched exact SID arrival -> observation/feedback | Repeated SLS row/type reads plus unknown OS transition time. Static arrival deadline 3000 ms, not fixed sleep; no inter-edge sleep. Trailing creation separately waits for actual Mission Control exit, at most 2000 ms with 50 ms runloop slices (`dock_swipe_select.rs:51-66`). Zero live samples. |
| Send | `workspace_send_managed.rs:656-909` -> one route decision -> bridged native move/follow (`940-1189`, `workspace_send_native.rs:345-500`) or drag fallback (`space_drag_transfer.rs:510-567`) -> membership/current/focus evidence -> feedback | Full native proof sets (`workspace_send_native.rs:596-729`), unknown blocking cost. Drag fallback deliberately waits 50+400=450 ms (static) before release; settlement reads immediately then at 200 ms intervals up to 3000 ms. Bridged route has no such 450 ms hold. No replay/fallback after possibly dispatched native work. Zero live samples. |
| Admission/close | launch/AX create or destroy notification (`ax_session.rs:1531-1559`) -> owner scoped read -> `runtime_thread.rs:1675-1693` -> `runtime_native.rs:469-573,694-841` roster/close truth -> reconcile `885-901` -> shared report/core -> reflow + retained admission/close focus -> native observation | Read/notification delivery, inventory and core/apply costs unknown. `pending_closes` comes from actual roster evidence, not a synthetic disappearance. A 1 s scoped backstop repairs missed notifications; it is not normal per-close waiting (`native_tile_backend.rs:885`). Zero live samples. |
| Cheap drag/resize | Ordinary product arm is parked: `drag_session.rs:64-95`, `tile.rs:2329-2352`. Offline scaffold: tap gate -> latest mouse batch -> Started/Updated/Released ingress (`drag_session.rs:341-454,524-636,745-764`) -> `runtime_thread.rs:1151-1237,1875-1895` -> `world_dispatch.rs:404-536` -> preview carrier (`drag_session.rs:1274-1304`) / release frames | No active product mouse path to measure. Ingress inspected is Move only, not resize. Scaffold preview query timeout 100 ms (`drag_session.rs:62`); press-proof age limit 500 ms is a freshness bound, not a delay. No activation or product-behavior change made. |

Numbered follow sends are the currently armed managed route; do not infer
that diagnostic stay/trailing/relative mechanics imply product support.
The diagnostic frame CLI's 75 s caller budget is not product action latency.
Eligibility refresh cadence (250 ms) and per-call AX timeout (2 s) are
freshness/failure bounds, not additive action delays or a whole-action bound.

## Ranked cost candidates

Ranking is investigation priority, not a measured attribution of total latency.

| Rank / cause | Evidence and cost | Confidence | Proposed fix; size / risk / owner |
| --- | --- | --- | --- |
| 1. Repeated native inventory and per-leg joins | `native_facts.rs:412-561`: full title-free CG list, app AX enumeration and many per-window getters. `native_tile_backend.rs:1473-1566`: another focused-window CG join/global focus read. Every attempted frame leg fences and reads back. Time unknown; slow AX calls can head-of-line block one app owner. | High structural, unknown magnitude | Time each native leg first; only then reuse same-snapshot data where existing proof semantics permit. Medium / medium-high / macOS. Do not remove safety fences or proof readbacks. |
| 2. Conditional drag-send holds | `space_drag_transfer.rs:55-64`: 450 ms fixed hold plus immediate/200 ms settlement reads, 3 s deadline. Static, not measured live; only fallback route. | High for configured waits, unknown occurrence | Measure bridged/fallback route separately; change waits only after native correctness discrimination. Medium / high / macOS. No wait changed. |
| 3. Full report/core work and in-process codecs | `world_dispatch.rs:115-197`; historical C7 full path 0.47-0.50 ms at 4 windows, 4.8-5.0 ms at 64, 11.2-12.8 ms at 128. Includes evaluator codecs/pipe IO, not isolated core or macOS timing. | High path, low transferability of costs | Typed in-process entry preserving validation, plus core C1/C2/C5 work. Large / medium-high / core owner coordination; no core or contract edit here. |
| 4. Serialized queues and quiet owner pumping | `runtime_thread.rs:355-403`, `runtime_worker.rs:40-45,1147,1436`; FIFO engine plus per-app AX owner. Quiet pump slice 20 ms; actual queue time unknown. | High architecture, unknown magnitude | Measure enqueue/take/native-call times; consider a wake-integrated native owner only if quiet time dominates. Medium / medium / macOS. Preserve latest-only slots and per-app ownership. |
| 5. Synchronous native diagnostic stderr | Unconditional frame `eprintln!` at `native_tile_backend.rs:1321`; normal structured sink is bounded/nonblocking (`runtime_log.rs:5-12,159-188`). Slow stderr can stall the native owner; ordinary cost unknown. | High possibility, unknown frequency/cost | Compare normal/slow sink then route bounded structured timing without losing evidence. Small-medium / low-medium / macOS. Not changed without measurement. |
| 6. Global CG linear lookup inside AX loop | Former `native_facts.rs:514` scanned global rows for every AX wid despite existing per-PID sorted evidence. Headless improvement below, at most microseconds in sampled synthetic reads. | High local, unknown interactive benefit | Reuse existing index; tiny / low / macOS. Implemented and kept. |

## Kept change and headless measurements

- Replaced only the per-window global linear search with binary search in
  existing `cg_entries`. No extra allocation, native call, caching lifetime,
  safety omission or publication change. Setup already filters PID, stable
  sorts and deduplicates by wid (`native_facts.rs:467-478`); first duplicate
  wins just as in the old search. Consumers use only layer and on-screen.
- Standalone optimized Rust harness mirrors that setup and both lookups.
  Interleaved PIDs, absent keys, different-payload duplicate CG rows and a
  repeated query were checked for equal results. These are synthetic data,
  not real CG/AX latency. No permanent benchmark or test helper was added.
- Two runs, each 21 samples/configuration, alternating old/new order;
  1500-20000 whole-read repetitions/sample, normalized by repetitions.
  `black_box` protects inputs/results. The table is run 2's **min/median/max
  of batch-average ns per synthetic snapshot**, including existing index
  setup plus all queries, not individual-operation latency percentiles.

| Base global rows / actual queries | Old ns min/median/max | New ns min/median/max |
| --- | --- | --- |
| 8 / 3 | 40.3 / 50.4 / 96.5 | 40.1 / 51.1 / 81.2 |
| 128 / 2 | 220.3 / 224.0 / 247.6 | 224.3 / 227.8 / 251.8 |
| 128 / 5 | 335.0 / 350.5 / 372.2 | 229.9 / 240.0 / 258.0 |
| 128 / 17 | 770.4 / 800.1 / 846.2 | 256.2 / 261.8 / 277.9 |
| 128 / 65 | 2512.4 / 2594.1 / 2794.9 | 390.4 / 398.3 / 419.8 |
| 512 / 2 | 460.7 / 467.7 / 501.0 | 468.4 / 474.7 / 485.7 |
| 512 / 5 | 779.6 / 801.8 / 822.1 | 467.9 / 486.6 / 526.4 |
| 512 / 17 | 2364.1 / 2435.6 / 2541.9 | 512.5 / 519.0 / 584.6 |
| 512 / 65 | 8597.8 / 8765.8 / 9252.7 | 672.6 / 688.2 / 747.1 |

- Global lists additionally include duplicate rows (2, 4, 8 respectively);
  unique owner inventories are 3, 16, 32. Large query batches reuse some
  keys, so these are lookup stress cases, not 65 distinct owned windows.
- Kept: 5-65-query medians improve 0.11-8.08 us in run 2, consistently with
  run 1. Two-query cases regress 3.8-7.0 ns (about 1-2% of this synthetic
  setup+lookup operation); tiny case is neutral within spread. This small
  tradeoff does not establish an AX or user-visible speedup.
- No other optimization was attempted or reverted. No tests were edited.

## Earlier lookup verification and limitations

- `tiler-macos --lib`: 1570 passed, 0 failed, raw exit 0 after the source
  edit. Full log inspected. Sixteen existing unused-variable warnings remain.
- An initial invocation accidentally included a `.` test filter: 0 passed,
  1570 filtered out, exit 0. It is compilation evidence only; the subsequent
  unfiltered run supplies the test result above.
- Headless harness compile and both corrected runs: raw exit 0. Initial
  timing totals were mislabeled per-operation; only corrected, normalized
  setup-plus-lookup results above are accepted.
- Per-file `rustfmt --check --edition 2024` passes, raw exit 0. An initial
  check without the crate's explicit edition reported unrelated import-order
  differences; no broad formatting change was made.
- Current-source probe binary build and bundle verification: raw exit 0;
  previous baseline library build: raw exit 0. Signed bundles precede the
  lookup edit and are not optimized-live evidence. No live result is claimed
  from the headless lib suite or from build/signature success.

## Opt-in stage timing (2026-11-12)

- Scope: macOS adapter only; core/protocol/contract unchanged. No optimization,
  retry, native read/write removal, wait adjustment or gesture activation.
- Approach: one process-monotonic microsecond clock and trace-id counter;
  receipt/enqueue stamps cross existing jobs; bounded shared maps join frame,
  focus and select/send operation identities. No synchronous tap logging or
  per-window scan logging. Disabled helpers return before clocks, maps, TLS
  and formatting (apart from first-use environment-switch initialization).
- Units: `count` is microseconds except `*-bind` identities, `timing-intake`
  (zero marker), and `*-n` instrumented-call counts. `t=...ms` belongs to a
  different runtime origin; never subtract it from timing stamps.
- Join: `pid=0 ref=trace` for engine/feedback/lane records. Frame/focus bind
  records map `(pid,gen,ref=command/request)` to `count=trace`. Op bind reverses
  this: `ref=trace count=op-sequence`. Maps cap at 128; missing/evicted joins
  and latest-only skipped commands are incomplete, not zero-latency actions.
- Fixed action/outcome tokens and numeric IDs/counts only: no titles, typed
  text, raw keys, bundle names, document content or workspace names in timing
  lines. Other stdout/stderr diagnostics have a different privacy contract;
  review them separately before sharing.

Current anchors below are relative to `crates/tiler-macos/src/`:

| Stage | Instrumented anchor | Meaning |
| --- | --- | --- |
| Intake/input map/engine queue | `ffi.rs:9040,9093`; `runtime_input.rs:121-205`; `runtime_thread.rs:1721-1744,1768-1790` | Tap receipt to enqueue, then pure engine inbox wait; programmatic submits start at submit. No timestamp of physical key depression. |
| Host change | `runtime_worker.rs:95-118`; scoped-observation arm in `runtime_thread.rs` | Starts at delivered poll-result receipt, class `host-change`; includes engine queue/reconcile/reflow, excludes native notification delivery and snapshot acquisition. Admission and close are coalesced, not separate input actions. |
| Engine map/report/codecs/core/accept/publish/dispatch | `world_dispatch.rs:148-203,760-1005`; input arm in `runtime_thread.rs` | Four codec legs plus core boundary, sequencer gate, consume/publication and whole dispatch. `timing-done` is engine dispatch return only. |
| Frame/focus apply and observed readback | `runtime_worker.rs:984-1002,1443-1461,1644-1755` | Native frame/focus attempt duration; post-batch snapshot duration; `timing-readback-total` is origin-to-snapshot-ready per distinct parent trace, not target equality. Shared batch cost is attributed through the first command, not exclusive action cost. |
| Native calls | `native_facts.rs:400-434`; `native_tile_backend.rs:364-413,1868-1875` | Actual CG list, whole AX inventory, frame setter/readback legs, focus setter. `ax-read/ax-set/ax-focus/cg` totals and `-n` companions. An inventory is one inclusive high-level call, not a count of every internal AX getter. |
| Select/send lane and waits | `select_owner.rs`; `dock_swipe_select.rs`; `workspace_send_managed.rs`; `space_drag_transfer.rs`; `workspace_send_effect.rs`; `select_feedback.rs:220` | Whole lane class `select-exec/send-bridged/send-drag/send-stay/send-trailing`; actual elapsed sleeps/polls `grab-settle/animation-hold/arrival-poll/select-latch` and count companions. Lane execution is not slept time. |
| Feedback totals | `runtime_thread.rs` View/MemberAttempt, AxResult, FocusResult and CloseFocusResult arms (`1986-2176`) | Origin to feedback receipt/staging: `timing-frame-settle/focus-settle/op-settle`. Typed focus covers navigation, admission and close as well as deferred focus. Setter accepted is feedback truth, not convergence or paint. |
| Presentation submit | `tile.rs:1984,2895`; `select_feedback.rs:254-283` | Changed-only border/indicator work under the latest publication trace. Coalesced attribution can skip intermediate actions; not exact causal per-action render and not compositor timing. |

- Unsplittable here without more plumbing: native create/destroy callback to
  first scoped acquisition; complete AX/SLS/selection-oracle query inventory;
  Mission Control exit runloop waits; shutdown-lane reports; actual compositor
  paint. Parked mouse drag/resize remains unavailable. Native owner scheduling
  and fences remain inside apply/feedback totals rather than separate queue
  spans. Follow-up attribution requires evidence, not another optimization.
- The async sink is bounded at 256 events. Its in-memory `logger_dropped`
  counter is not exposed by the current tile terminal JSON. Check gaps in the
  full structured-event sequence before filtering; gaps prove missing records,
  but no gaps cannot prove absence of a truncated tail. Missing intakes,
  bindings and endpoints are censored in analysis. No completeness or enabled
  overhead bound is claimed.

## User-run bounded captures

- Ordinary-app acceptance is physical and user-run, not a fixture retry.
  The earlier fixture join/recovery-contract blockers remain relevant to
  future controller work; this procedure does not bypass their proof gates.
- Save work; stop an existing tiler normally before starting another.
  Prepare disposable ordinary windows across two already-available managed
  workspaces on one display. Avoid fullscreen, games, protected/elevated apps,
  lock and Secure Input. Do not create Spaces or change system shortcuts for
  this capture. The unscoped route manages ordinary apps, including terminals;
  use the TextEdit-only alternative below if that is unwanted.
- Prerequisites: the existing macOS development toolchain, one persistent
  `OmniTiler Mac Dev` identity/private key and bundle Accessibility/input
  consent. See [signing instructions](../macos-dev-environment.md#persistent-self-signed-development-identity).
  No new installs, certificate trust changes or TCC resets are required by
  timing. If the identity is missing or a guard/build/signature check fails,
  stop and report its exact error; never substitute an old/ad-hoc bundle.
- From the checkout containing these source changes, build/sign and verify:

```sh
mise exec -- just app 'OmniTiler Mac Dev'
mise exec -- just verify
shasum -a 256 target/macos/OmniTilerProbe.app/Contents/MacOS/tiler-macos
mkdir target/macos/responsiveness-manual
open -n --stdout target/macos/responsiveness-manual/preflight.json --stderr target/macos/responsiveness-manual/preflight.err target/macos/OmniTilerProbe.app --args preflight
```

- Run one command at a time and stop on nonzero status. `mkdir` must create
  a fresh directory; if it exists, choose a fresh run name throughout rather
  than overwriting it. The build script builds only `tiler-macos --bins`,
  enforces macOS 15.0, checks owned-bundle markers and signs nested-first at
  its fixed paths. Instrumented per-crate binary build passed; assembly,
  signing and launch are still unrun for this timing revision. Do not launch
  an older signed artifact. The workspace-wide launcher is not needed.
- After preflight finishes, inspect both files:

```sh
cat target/macos/responsiveness-manual/preflight.json
cat target/macos/responsiveness-manual/preflight.err
```

- Require AX trusted, event-post access, unlocked session and inactive Secure
  Input. Lost consent is a stop, not a reason to reset TCC automatically.
  Then launch the instrumented signed bundle with an explicit LaunchServices
  environment value (a shell environment prefix alone is insufficient):

```sh
open -n --env TILER_MACOS_STAGE_TIMING=1 --stdout target/macos/responsiveness-manual/tile-a.jsonl --stderr target/macos/responsiveness-manual/tile-a.err target/macos/OmniTilerProbe.app --args tile --user-start --unscoped --input tap --seconds 120 --log verbose
```

- Narrow alternative: replace `--unscoped` with `--scope com.apple.TextEdit`;
  the scope covers every TextEdit document. Do not run both commands. Confirm
  `tilestarted` before physical actions and note its PID. Use `--seconds`
  1..=120 for these finite captures; 300 refuses at startup and its diagnostic
  lands in stderr, not necessarily stdout. Inspect both files if startup is
  missing. Timing is cached for the process; omit `--env` on a later fresh
  launch to return to default off.
- Run A below uses `tile-a`. After clean shutdown, repeat the launch with both
  output names changed to `tile-b`, then `tile-c`, each with `--seconds 120`.
  Never overlap runs or overwrite an earlier capture. Inspect each terminal
  outcome before starting the next run.

| Run | Elapsed | Physical action script (default Ctrl modifier; use configured equivalent if changed) |
| --- | --- | --- |
| Each | 0:00-0:15 | Wait for initial tiling; note window count, app classes and starting workspace. No sensitive document content is needed. |
| A | 0:15-1:00 | Ten focus left/right pairs with Ctrl+H/L, pausing about one second after each key release. Try J/K only where a vertical neighbor exists. Record edge/no-op presses separately. |
| A | 1:00-1:50 | Ten move/swap back-and-forth pairs with Ctrl+Shift+H/L (or J/K where geometry permits), pausing between releases. Record refusal or unchanged geometry, not a successful move. |
| B | 0:15-1:00 | Ten workspace 1/2 round trips with Ctrl+2 then Ctrl+1. Let each transition settle; stop pressing if it is still moving. Skip if two existing destinations are unavailable. |
| B | 1:00-1:50 | Five send-follow round trips of one disposable window with Ctrl+Shift+2 then Ctrl+Shift+1. Wait after each release. The log's lane class separates bridged from drag fallback; do not force fallback with new flags. |
| C | 0:15-1:30 | Create and close five disposable ordinary windows, one at a time, waiting for admission/reflow/focus after each. Note perceived delay or flicker and approximate interval; log class is coalesced `host-change`, not exact notification latency. |
| Each | Remainder to 2:00 | Stay idle and let the 120-second run expire. Stop the action block early if needed; do not rush transitions, arm the parked mouse path or invent a resize leg. |

- Stop early on a crash, wrong repeated actuation, lost consent, lock or
  Secure Input. Use Activity Monitor only for the exact product PID from this
  run if urgent/hung. `open` returning or Ctrl+C in its terminal does not stop
  the launched app. A forced/unclean teardown is not acceptance.
- Inspect that run's terminal `tile-a.jsonl` (or `tile-b`/`tile-c`) outcome and require `shutdown_clean:true`;
  confirm interception, active border and this product instance are gone.
  Close only disposable documents afterward. Geometry/Space moves are not
  automatically restored. Keep logs if a transition or cleanup fails.
- Output lands in `target/macos/responsiveness-manual/`. After reviewing full
  logs locally, extract the content-free timing lines; empty output means
  timing is not established, not zero latency:

```sh
RUN=tile-a
grep '^tiler-log .*kind=timing-' "target/macos/responsiveness-manual/$RUN.err" > "target/macos/responsiveness-manual/$RUN-timing.log"
awk '/^tiler-log / {split($2,s,"="); if (last && s[2]!=last+1) print "sequence gap",last,s[2]; last=s[2]}' "target/macos/responsiveness-manual/$RUN.err"
```

- Repeat extraction/analysis separately with `RUN=tile-b` and `RUN=tile-c`.
  Never concatenate process logs: trace identities restart on each launch.
- Attach each run's `*-timing.log`, analysis JSON below and reviewed terminal outcome;
  include source revision/uncommitted timing changes, executable hash, app
  classes/window count, skipped blocks, visible route, subjective slow/janky
  actions and clean/unclean teardown. Do not attach unreviewed full stderr,
  preflight diagnostics or documents. No live action times are claimed here.

## Analysis recipe

- Run against one process/run only; trace IDs restart per process. Requires
  existing `jq` and shell utilities. Two-pass bind lookup tolerates file order;
  conflicting/ambiguous identities remain unjoined. Missing intake traces
  are excluded from distributions. No endpoint is `incomplete`, never zero.
- Output milliseconds by intake class and raw outcome: per-kind endpoint MAX
  per trace; representative MAX across endpoints is only an observed
  feedback/readback approximation. A failed snapshot/timeout/refused/not-called
  stays separate; `applied` means setter feedback, not pixels or equality.
- Stage statistics use per-action sums within each kind/outcome. Cumulative
  settle/readback/done records and bind/count companions are not stage costs.
  Do not add nested dispatch/report/codecs/native spans together. Batch-native
  aggregates and coalesced UI submits are attribution hints, not exclusive
  action accounting. Median averages the two middle values; p95 is nearest rank.
- Paste this whole block from the capture checkout; it writes only analysis
  files in the run directory, never runs the product:

```sh
RUN=tile-a bash <<'SH'
set -euo pipefail
cat > target/macos/responsiveness-manual/analyze.jq <<'JQ'
# Two-pass joins; all durations below are milliseconds, not paint latency.
def med: if length==0 then null elif length%2==1 then .[length/2|floor]
  else (.[length/2-1]+.[length/2])/2 end;
def p95: if length==0 then null else .[(0.95*length|ceil)-1] end;
def stats: sort as $s|{n:($s|length),min:$s[0],med:($s|med),p95:($s|p95),max:$s[-1]};
[inputs|select(test("tiler-log"))
 |capture("tiler-log seq=(?<seq>[0-9]+) t=(?<t>[0-9]+)ms pid=(?<pid>[0-9]+) gen=(?<gen>[0-9]+) ref=(?<ref>[0-9]+) kind=(?<kind>[^ ]+) outcome=(?<out>[^ ]+) count=(?<count>[0-9]+)")
 |select(.kind|startswith("timing-"))
 |{seq:(.seq|tonumber),pid:(.pid|tonumber),gen:(.gen|tonumber),ref:(.ref|tonumber),kind,out,count:(.count|tonumber)}] as $ev
|(["timing-frame-bind","timing-focus-bind","timing-op-bind"]) as $bk
|(["timing-frame-settle","timing-focus-settle","timing-op-settle","timing-readback-total"]) as $epk
|([$ev[]|select(.kind=="timing-frame-bind")|{k:"\(.pid):\(.gen):\(.ref)",t:.count}]
 |group_by(.k)|map({key:.[0].k,ts:([.[].t]|unique)})) as $fbg
|([$ev[]|select(.kind=="timing-focus-bind")|{k:"\(.pid):\(.gen):\(.ref)",t:.count}]
 |group_by(.k)|map({key:.[0].k,ts:([.[].t]|unique)})) as $fog
|([$ev[]|select(.kind=="timing-op-bind")|{k:(.count|tostring),t:.ref}]
 |group_by(.k)|map({key:.[0].k,ts:([.[].t]|unique)})) as $opg
|([$fbg[]|select(.ts|length==1)]|map({key,value:.ts[0]})|from_entries) as $fb
|([$fog[]|select(.ts|length==1)]|map({key,value:.ts[0]})|from_entries) as $fo
|([$fbg[]|select(.ts|length>1)]) as $fconf
|([$fog[]|select(.ts|length>1)]) as $foconf
|([$opg[]|select(.ts|length>1)]) as $opconf
|([$fconf[].key]) as $fck
|([$foconf[].key]) as $fock
|($ev|map(. as $e|"\($e.pid):\($e.gen):\($e.ref)" as $k|($fb[$k]) as $f|($fo[$k]) as $o
 |.trace=(if (.kind|IN($bk[])) then null
  elif $e.pid==0 then (if $e.ref==0 then null else $e.ref end)
  elif $e.kind=="timing-frame-apply" or $e.kind=="timing-observe" then $f
  elif $e.kind=="timing-focus-apply" then $o
  elif ($e.kind|startswith("timing-native")) or $e.kind=="timing-lane-exec" then
   (if $f!=null and $o!=null then (if $f==$o then $f else null end)
    elif $f!=null then $f elif $o!=null then $o else null end)
  else null end)
 |.jr=(if (.kind|IN($bk[])) then "binder"
  elif $e.pid==0 then (if $e.ref==0 then "untraced-ref0" else null end)
  elif ($e.kind=="timing-frame-apply" or $e.kind=="timing-observe") and ($fck|index($k)) then "conflict-bind"
  elif $e.kind=="timing-focus-apply" and ($fock|index($k)) then "conflict-bind"
  elif ((($e.kind|startswith("timing-native")) or $e.kind=="timing-lane-exec") and (($fck+$fock)|index($k))) then "conflict-bind"
  elif ((($e.kind|startswith("timing-native")) or $e.kind=="timing-lane-exec") and $f!=null and $o!=null and $f!=$o) then "ambiguous"
  elif .trace==null then "missing-bind" else null end)
 |if .jr!=null then .trace=null else . end)) as $j
|([$j[]|select(.kind=="timing-intake" and .trace!=null)]
 |group_by(.trace)|map({key:(.[0].trace|tostring),value:.[0].out})|from_entries) as $cls
|(([$j[]|select(.trace!=null)|.trace]+[$fbg[].ts[]]+[$fog[].ts[]]+[$opg[].ts[]])
 |map(select(.!=0))|unique) as $uni
|([$cls|keys[]|tonumber]) as $itr
|(($uni-$itr)) as $noint
|([$j[]|select(.trace!=null and $cls["\(.trace)"]!=null and (.kind|IN($epk[])))]
 |group_by([.trace,.kind,.out])|map(max_by(.count)
  |{trace,action:$cls["\(.trace)"],kind,out,ms:(.count/1000)})) as $eper
|(($eper|group_by([.action,.kind,.out])|map({action:.[0].action,kind:.[0].kind,
 out:.[0].out,q:([.[].ms]|stats)}))) as $epd
|(($eper|group_by(.trace)|map({trace:.[0].trace,action:.[0].action,
 rep:(max_by(.ms)|{ms,kind,out})})
 |map(.+{approx:"per-trace MAX across endpoint kinds; observed feedback/readback arrival approx, never completion/paint/equality"}))) as $epr
|(([$eper[].trace]|unique)) as $ept
|(($itr-$ept)) as $incomp
|([$j[]|select(.trace!=null and $cls["\(.trace)"]!=null and .kind!="timing-intake"
 and .kind!="timing-done" and (.kind|IN($bk[])|not) and (.kind|endswith("-n")|not) and (.kind|IN($epk[])|not))]
 |group_by([.trace,.kind,.out])
 |map({a:$cls["\(.[0].trace)"],kind:.[0].kind,out:.[0].out,ms:((map(.count)|add)/1000)})
 |group_by([.a,.kind,.out])
 |map({action:.[0].a,kind:.[0].kind,out:.[0].out,q:([.[].ms]|stats)})) as $std
|([$j[]|select(.trace!=null and $cls["\(.trace)"]!=null and (.kind|endswith("-n")))]
 |group_by([.trace,.kind,.out])
 |map({a:$cls["\(.[0].trace)"],kind:.[0].kind,out:.[0].out,c:(map(.count)|add)})
 |group_by([.a,.kind,.out])
 |map({action:.[0].a,kind:.[0].kind,out:.[0].out,calls:([.[].c]|add),traces:length})) as $cal
|([$j[]|select(.jr!=null and .jr!="binder")]
 |group_by([.jr,.kind,.out])
 |map({reason:.[0].jr,kind:.[0].kind,out:.[0].out,n:length})) as $unj
|{binders:{frame:($fb|length),focus:($fo|length),
  conflicts:{frame:$fconf,focus:$foconf,op:$opconf}},
 intake:{records:([$j[]|select(.kind=="timing-intake")]|length),traces:($itr|length)},
 censored:{no_intake:$noint,incomplete:$incomp},
 endpoints:{note:"grouped by intake action class + endpoint kind/outcome; values are per-trace per-kind MAX ms; representative is an approximation of observed feedback/readback arrival only (applied means feedback arrived, never native observed equality)",
  distribution:$epd,representative:$epr},
 engine_returns:([$j[]|select(.kind=="timing-done" and .trace!=null and $cls["\(.trace)"]!=null)]
  |group_by([$cls["\(.trace)"],.out])|map({action:$cls["\(.[0].trace)"],out:.[0].out,n:length})),
 stages:$std,calls:$cal,unjoined:$unj}
JQ
jq -Rn -f target/macos/responsiveness-manual/analyze.jq < "target/macos/responsiveness-manual/$RUN-timing.log" > "target/macos/responsiveness-manual/$RUN-analysis.json"
jq '{intake,censored,unjoined,engine_returns,endpoints:.endpoints.distribution,stages,calls}' "target/macos/responsiveness-manual/$RUN-analysis.json"
SH
```

### Verified schema sample (not live performance)

- Real async-sink smoke output supplies 18 schema lines with synthetic,
  deliberately nonphysical stage values. Recipe output: one `focus-step`
  intake, no missing/incomplete/unjoined traces, one frame/focus/op bind;
  representative trace 9001 endpoint is `readback-total/snapshot` at 0.310 ms.

| Sample group | n | min / median / p95 / max ms |
| --- | --- | --- |
| focus-step / readback-total / snapshot | 1 | 0.310 / 0.310 / 0.310 / 0.310 |
| focus-step / focus-settle / applied | 1 | 0.121 / 0.121 / 0.121 / 0.121 |
| focus-step / frame-apply / applied | 1 | 0.180 / 0.180 / 0.180 / 0.180 |
| focus-step / native-wait / select-latch | 1 | 0.400 / 0.400 / 0.400 / 0.400 |

- Call companions: `ax-read=2`, `select-latch=1`. The sample's wait exceeds
  its synthetic endpoint; these are schema checks, not physical latencies.
- Independent synthetic quantile input: queue values 1,2,3,4 ms yield
  `n=4 min=1 med=2.5 p95=4 max=4`. A no-intake 9.999 ms value is excluded;
  one missing bind, one conflicting bind and one ambiguous frame/focus join
  are reported, not attributed. Two incomplete intake traces remain listed;
  failed endpoint remains its own outcome. Recipe executions exit 0.

## One user-run capture: functional findings

- Evidence: `tile-a.err` and `tile-a.jsonl`, a signed-bundle, TextEdit-scoped
  120-second physical run with timing enabled. Clean shutdown is reported;
  physical behavior is user-reported, not an automated native acceptance leg.
- At the first observed workspace-2 selection, an in-flight observation was
  rejected for stale topology revision (`tile-a.err:657`). The next accepted
  snapshot reports two live AX windows, two domains and four homed member
  rows (`666-678`), then the first `held-incomplete-inventory`. That hold
  persists. Domain/member counts are not distinct-window identity dumps.
- Rejection marks both coverage and the app roster incomplete
  (`runtime_native.rs:559-572`). A fresh covering observation clears coverage
  (`877-883`); covering roster incorporation can clear its own hold
  (`runtime_core.rs:1036-1042`). Therefore a permanent stale-revision latch
  alone is NOT established. Inventory also depends on roster completeness,
  readable domains and skipped outputs (`world_builder.rs:636-640`). The log
  lacks the discriminant needed to name the persistent input to this AND.
- Directional focus/move: 54/16 `focus-unknown` refusals mean no execution
  subject, not missing report foreground. Mapping intentionally requests
  implicit focus (`world_command_map.rs:248-262`); core refuses when its
  execution cursor and canonical slot are both empty
  (`tiler-core/src/world_commands.rs:68-74`, `world_close_focus.rs:93-97`).
  Core refuses to record native focus in held domains
  (`world_sends.rs:120-132`), then clears mismatching slots on a native focus
  change (`world_run_v4.rs:364-404`). This explains the observed combination
  of nonempty logged foreground and empty command subject under a hold;
  it does not establish a core defect or justify bypassing that fence.
- Send: all ten core command results are `scope-held`. The native target is
  intentionally initially unbound (`runtime_thread.rs:2803`) and is filled
  from an accepted core transfer (`2840-2845`). The refused-operation path
  can leave the claim unbound (`2865-2879`); `workspace_send_effect.rs:220-222`
  then correctly refuses without a target SID. This is downstream of the
  core refusal, not evidence that the adapter should guess a destination.
- Second-workspace tiling: no frame attempt follows the first hold; the only
  frame-set diagnostics name the two initial-workspace windows. The log does
  not identify the other member rows or prove their exact native homes.
  The user-reported untiled windows are consistent with the hold, not
  contradicted by the four homed member rows. An admission/binding defect
  versus an incomplete native observation remains unresolved.
- Input: tap matching counts edges, while keyups deliberately increment
  ignored (`runtime_input.rs:109-119`). Ignored also includes unmatched downs
  and orientation autorepeat (`152-167`); aggregates do not distinguish them.
  The timing log contains 124 keyboard-action intakes (68 focus, 16 move,
  four orientation, 26 select, ten send), plus two indicator-click intakes.
  Thus 88 forwarded plus 38 selection-lane enqueues includes clicks and
  sends, not 126 proven keyboard commands. `input_ignored:126` is consistent
  with normal edge filtering, not evidence that half the commands were lost;
  the exact ignored-event breakdown is not logged.
- Eight early `unchanged` directional results are resolved-subject no-ops,
  not malformed input: the diagnostic prints the resolved bookkeeping
  command (`tiler-core/src/world_engine.rs:3191-3203`). Exact edge geometry
  is not logged, so intended edge behavior cannot be confirmed. Four
  orientation refusals occur during the hold; local operations require an
  available readable, nonheld domain (`world_operations.rs:2163-2179`).
- History: stale-revision fencing is present at `5fafd966`, off-current
  recovery at `6a1a6dd8`, and clear-on-native-focus mismatch at `0046d905`.
  These mechanisms predate retained focus and the single-ingress migration.
  The retained-focus change adds the parallel logical cursor; this capture
  does not isolate a new regression in `632ee7d9` or the ingress migration.
- No runtime fix or new headless reproduction was made: the available
  evidence does not establish a safe few-line correction. Needed next
  discriminant is the incomplete-report inputs (roster cause, unreadable
  domains, skipped outputs, coverage) and member lifetime/home identities
  at the first hold. Retain safety gates; do not turn completeness true or
  substitute native foreground as a command bypass.

## One user-run capture: workspace-select timing

- Values are milliseconds, min / median / nearest-rank p95 / max, from the
  documented analyzer applied separately to `tile-a.err`. There are 26
  joined keyboard `select` intake traces here; the 38 selection-lane enqueues
  also include ten sends and two indicator clicks, excluded from this table.
  Nested stages are not additive. Feedback staging, snapshot readiness and
  indicator submission do not establish convergence, paint or input-to-pixel
  latency. No timing from refused actions is evidence of successful action
  responsiveness. This is one capture, not a performance acceptance result.

| Stage / outcome | n | Min | Median | p95 | Max |
| --- | --- | --- | --- | --- | --- |
| Input map | 26 | 0.165 | 13.483 | 21.059 | 22.858 |
| Engine queue | 26 | 0.019 | 0.037 | 0.226 | 1.348 |
| Mapping | 26 | 0.002 | 0.005 | 0.014 | 0.018 |
| Report build | 26 | 0.177 | 0.531 | 0.707 | 0.709 |
| Report encode | 26 | 1.567 | 4.723 | 6.555 | 6.732 |
| Report decode | 26 | 0.965 | 3.019 | 4.107 | 6.785 |
| Core | 26 | 1.144 | 3.851 | 6.470 | 8.035 |
| Reply encode | 26 | 0.550 | 1.688 | 2.370 | 2.444 |
| Reply decode | 26 | 0.440 | 1.099 | 1.592 | 1.754 |
| Accept | 26 | 0.018 | 0.050 | 0.070 | 0.072 |
| Core publication submit | 26 | 0.050 | 0.195 | 0.250 | 0.299 |
| Whole engine dispatch / applied | 26 | 4.958 | 15.833 | 20.620 | 23.219 |
| Select lane execution | 26 | 34.924 | 42.780 | 52.177 | 71.507 |
| Select latch wait | 26 | 27.659 | 34.821 | 44.850 | 45.832 |
| Indicator submit | 26 | 9.332 | 15.583 | 31.729 | 45.802 |
| Native AX read aggregate | 1 | 2.589 | 2.589 | 2.589 | 2.589 |
| Native CG aggregate | 1 | 2.877 | 2.877 | 2.877 | 2.877 |
| Observation snapshot | 1 | 28.302 | 28.302 | 28.302 | 28.302 |
| Origin to view-feedback staging | 26 | 122.557 | 158.097 | 359.701 | 369.568 |
| Origin to snapshot readiness | 1 | 68.163 | 68.163 | 68.163 | 68.163 |

## Timing verification and exact next action

- Latest headless library: 1581 passed, 0 failed, 3 ignored, raw exit 0.
  Explicit timing-enabled ignored tests: 3 passed, 0 failed, exit 0 (shared
  cross-thread async-sink joins, deferred focus origin, typed focus result).
- `tiler-macos --bins` with macOS 15.0 deployment floor: exit 0, six existing
  dead-code warnings. No required-crate compilation blocker. Signed-bundle
  assembly/signature/consent/launch checks are user prerequisites, not proved
  by binary compilation or old signed artifacts.
- All-target clippy: exit 0, unchanged 24 library / 68 library-test warnings
  (16 duplicates); no new finding. Final readback let-chain lint fixed without
  suppression. Edition-2024 changed-file formatting and diff checks pass.
- Added pure timing/schema/bounded-map tests and explicit enabled-only tests.
  Existing event/job literals received zero stamps only; no prior assertions
  were changed. The earlier lookup optimization is retained.
- Earlier select-owner cancellation test failed once under load, then passed
  isolated and on filtered/full reruns; baseline flakiness is not established.
  One enabled verification timed out during compilation before harness output;
  a longer completed run supplied the 3/0 result above. No native latency or
  enabled live-overhead measurement resulted from these checks.
- Follow-up changed documentation only. The extracted analysis wrapper passes
  shell syntax validation and its extracted jq program parses this capture.
  Earlier library/build/lint results above are unchanged historical evidence,
  not rerun gates for a new runtime fix.
- Exact next action: obtain the missing inventory-hold discriminant before
  routing a runtime correction or repeating successful-action latency checks.
  Keep this investigation open; no new runtime/native acceptance is claimed.

## Same-workspace rerun fixes (2026-11-12)

- A separate TextEdit-only, two-window capture without workspace switches or
  inventory holds has 49 bind-time focus no-calls. At `tile-a.err:98-105`, core
  accepts focus, the consumer emits `no-call`, and frame work is redundant.
  Neither that line nor its neighbors logs the focus reason. The headless
  reproduction exposed `IdentityStale`: core requests name logical `ws-1`,
  retained homes name native `space-1`. This was an adapter mapping defect,
  not a failed native focus call or an arrival-proof requirement.
- Fixed `world_dispatch.rs:3446-3507` to resolve the current reply's
  `native_binding` before comparing native homes/context. No binding cache or
  safety bypass. All unsatisfied retained causes share this consumer: explicit
  directional focus, automatic admission, positive close and focus-follows-cursor.
  Selection-backed explicit focus uses a separate, already-resolved arrival path
  (`3311`).
- `runtime_thread.rs:3660` now drives a deterministic Left from window 7 to 8
  through real input mapping, codec and core, then the mocked native owner.
  It asserts differing logical/native IDs, one queued command, exactly one native
  dispatch and Accepted feedback. The pre-fix IdentityStale failure was reported
  by the reproduction; the final suite was independently rerun. This proves
  headless actuation routing, not native arrival.
- Frame scheduling: `tile.rs:2304` starts the runtime; `runtime.rs:5-11` owns one
  AX thread per app. `world_dispatch.rs:1921-1990` groups windows into whole-app
  slots; `runtime_worker.rs:1517-1645` completes each frame before the next.
  Both TextEdit windows therefore serialize. Each old frame performs three
  setters, three frame readbacks and three full safety fences; native setters
  rejoin AX elements per call (`native_tile_backend.rs:2304-2338`). The observed
  roughly 30 ms is the whole transaction, not a fixed wait or isolated AX setter
  cost. Finicky AX size handling motivates size-position-size (`49-58`).
- Direct capture counts: 267 applied/exact, 115 failed/exact, 182 failed/inexact;
  all 564 have readable readback. Given `apply_scoped_frame`'s return branches,
  failed-with-readback means an inter-leg safety refusal, not a logging bug or
  proof that geometry failed. The fallback readback may already match when a
  later leg refuses. `ax-result` retains only `failed-backend`; the exact safety
  predicate is not recoverable. Supersession is possible, not established.
  No failure-driven retry loop exists. New publications can reissue geometry
  until the post-batch snapshot matches; matching observed frames are skipped
  (`runtime_worker.rs:1581-1628,1680-1694`). Partial work can thus recur, but the
  failed label itself does not request more work.
- Kept fresh-readback setter skips (`native_tile_backend.rs:382-484`): ordinary
  move setters 3 -> 2, already-matching later legs 3 -> 1; readbacks remain 3,
  fences remain 3. Six tests pin exact counts, unreadable fallback, safety
  precedence, partial truth and size repair after position perturbs size. No
  initial-size skip: it lacks a fresh pre-write read. Expected live benefit is
  one avoided AX write/rejoin on normal swaps, not a measured millisecond gain.
- Deferred setters-first/readbacks-later batching: it would change per-leg
  evidence and the fresh-read basis for size repair, requiring transaction/report
  restructuring. Cross-app parallelism already exists. Parallelizing windows
  inside one app is medium-to-large work involving owner-affine AX handles,
  per-window lanes and snapshot/fence ordering; not implemented.
- Direct final gates: library 1588 passed, 0 failed, 3 timing-only ignored;
  binary build and all-target clippy exit 0. Clippy remains 24 library / 68 test
  warnings, with none in new hunks. First library run timed out (exit 143); the
  completed rerun passed. A synthetic-context regression during development was
  repaired by limiting native binding resolution to native contexts. Tightening
  the test initially chose an edge direction; deterministic Left fixes the
  fixture without changing core behavior. No new live acceptance is claimed.
- Next: rebuild the signed app and repeat the bounded same-workspace capture.
  Check actual Ctrl+H/L focus changes, owner `focus dispatched`/Accepted results,
  fewer native setter calls on swaps and the second window's visible lag. Retain
  failures and clean-shutdown evidence. The earlier inventory-hold investigation
  remains open and separate.

## Same-workspace diagnostic findings (2026-11-12)

- Evidence: read-only `target/macos/responsiveness-rerun2/tile-a.err` and
  `tile-a.jsonl`, one 120-second TextEdit tap session, no Space switches.
  Log sequence is contiguous through 13041; missing tail data is not disproved.
  Physical impressions are user-reported, not isolated benchmarks.

| Fault | What this capture proves | Cause / owner / confidence |
| --- | --- | --- |
| Inventory hold | 12 held replies in three four-reply clusters: refs 124-127, 141-144, 163-166. No immediately preceding stale observation; topology revision stays constant. Covering-close refs 128, 145, 167 clear the holds. | Exact incomplete-report input missing. macOS roster/oracle candidate, not a proved root cause. `runtime_core.rs:902-1042` retains AX-absent windows when CG says alive or is unreadable; `audit_roster_hold:796` has further causes. High confidence in mechanism, low in per-run attribution. |
| Focus stops | Last native dispatch at 19.043 s, result applied at 19.106 s; next accepted focus/no-call at 19.239 s. First hold is 61.465 s, so it cannot cause the initial focus stop. 88 `focus-accepted` envelopes, 16 native dispatches; close-focus counts: absent 263, satisfied 117, no-call 31, dispatched 16. | Exact no-call predicate was discarded. `world_dispatch.rs:3798-3843`: satisfied means retained observed target already matches, not a setter; absent means no request. Cause and owner undetermined; high confidence in dispatch cessation, no proved logical/native divergence. |
| Close gap | After each coalesced notification, the first successful close prune/reflow follows the 1-second backstop reread. Notification-to-last-setter: 1122/1170/1149 ms on the native clock. Reduced-inventory publication-to-last-frame: 1069/1100/1086 ms on the ring clock; clear-to-last-frame: 85/115/80 ms. | macOS delayed roster truth/backstop path implicated, exact CG/AX cause unknown. Backstop constant: `native_tile_backend.rs:981`. High confidence in ordering, not in CG-linger hypothesis. No positive-close focus request in these three reflows. |
| Move speed | User reports no improvement. Reported frame median/p95/max 36/73/96 ms versus earlier 30/72 ms median/p95 is not a controlled comparison. Actual native aggregates: 171 setter records / 481 calls, 385 read records / 1040 calls, 16 focus records / 16 calls. | macOS serial per-app owner and fenced native work are known structure, not isolated dominant cost. Aggregates are batch-wide, not per-window getters. Existing normal-move setter skip can save one write; these numbers neither prove nor disprove its benefit. |
| Frame refusals | 181 applied/exact, 63 failed/inexact, 28 failed/exact; all 91 failures collapse to `failed-backend`. Existing return paths identify failed-with-readable-readback as inter-leg safety refusal, not proof of AX setter failure. | macOS frame fence path, high confidence; exact predicate undetermined. `native_tile_backend.rs:330` and new per-leg diagnostics discriminate supersession, revision, scope and native proof failures. Exact geometry can coexist with a later safety refusal. |

- Source anchors above are under `crates/tiler-macos/src/`. No behavior fix
  accompanies these diagnostics; prior native-binding mapping and fresh-readback
  setter skips remain unchanged in intent. Safety fences, readbacks and cadence
  were not weakened. No core/protocol change or new live result.
- Core forwarding status: **investigation only, not a confirmed defect**.
  `crates/tiler-core/src/world_sends.rs:106-132` refuses canonical registration
  in held scopes; `world_run_v4.rs:364-404` can clear mismatching implicit/logical
  cursors after a changed native focus or empty native focus. Neither explains
  the first 19.239 s no-call from this capture. A core report needs the new
  report foreground/completeness, hold scope, request target, observed focus,
  exact route outcome and feedback from the same transaction. No core fix ready.

### Added diagnostic vocabulary

- Opt-in: `TILER_MACOS_STAGE_TIMING=1`; common-clock `stamp_us` on explicit
  `tiler-diag` lines. Existing async ring adds `diag-focus-explicit`,
  `diag-focus-admission`, `diag-focus-follows`, `diag-focus-close`, `diag-report`,
  `diag-report-view`, `diag-report-roster`, `diag-reply`, `diag-close`.
- Explicit stages: `report`, `hold`, `hold-cover`, `focus`, `focus-feedback`,
  `focus-done`, `frame`, `close-trigger`, `snapshot-start`, `snapshot-end`,
  `close-absence`, `close-pruned`, `observe-reject`, `close-report`, `close-reply`,
  `batch-queued`, `batch-done`.
- Focus preserves request/cause/target, observed identity/known flag and route
  tokens such as `already-satisfied`, `no-call-home-identity-stale`,
  `no-call-native-unreadable`, `slot-stopped-identity-stale`, plus exact deferred
  refusal reason. Owner feedback preserves setter, reason, pre/post and proof.
- Hold inputs include gap, unreadable domains, skipped-output reasons,
  roster generation/cause and unreadable members/homes (24 entries then `+N-more`).
  Roster tokens include `cg-alive`, `cg-unreadable`, `no-context`,
  `stale-revision`, `blank-hidden`, `unknown-no-frame`,
  `unknown-output-ungrounded` and classifier-omitted minimized/hidden-app.
- Frames include pid/wid/revision, entry/completion, actual attempted/applied
  setters, attempted/unreadable readbacks and all three leg outcomes. Predicate
  tokens include `frame-superseded`, `revision-drift`, `domain-drift`,
  `membership-off-current-space`, `cg-owner-mismatch`, `identity-drift`,
  `scope-out-of-scope`, `secure-input-active`; unknown errors stay sanitized.
- Close absence includes CG `owned|absent|unknown` and fate
  `retained-cg-alive|retained-unknown-oracle|pruned|preserved-hidden`.
  `batch-queued` is publication, not completion. `batch-done` is terminal owner
  accounting after frame work and the post-batch snapshot, not successful paint.

### One-run capture procedure

- From this checkout's root in the current graphical-login terminal, with no
  other probe running. Use three or four disposable blank TextEdit windows in
  one native workspace. Do not switch Spaces, send windows, type document text,
  or add another run. Keep physical recovery available. A consent/preflight
  failure stops setup; do not reset TCC or substitute direct binary execution.
- Build/verify at the existing signed-bundle path, retain its identity/hash and
  fresh LaunchServices preflight. This block refuses an existing capture folder:

```sh
bash <<'SH'
set -euo pipefail
run=target/macos/responsiveness-rerun3
mkdir "$run"
mise exec -- just app 'OmniTiler Mac Dev' >"$run/build.out" 2>"$run/build.err"
mise exec -- just verify >"$run/verify.out" 2>"$run/verify.err"
shasum -a 256 target/macos/OmniTilerProbe.app/Contents/MacOS/tiler-macos >"$run/probe.sha256"
codesign -d --verbose=4 -r- target/macos/OmniTilerProbe.app >"$run/codesign.txt" 2>&1
git rev-parse HEAD >"$run/revision.txt"
git diff --stat >"$run/source-changes.txt"
mise exec -- just preflight-app >"$run/preflight.json" 2>"$run/preflight.err"
jq -e '.arch_supported == true and .ax_trusted == true and
  .session.lock == "Unlocked" and .session.on_console == true and
  .session.login_done == true and .secure_input == "Inactive" and
  .post_event_access == true' "$run/preflight.json"
SH
```

- After those checks succeed, start exactly one bounded run. `open` returning
  zero proves launch submission only, not the probe's exit status. Start the
  action clock when `tilestarted:true` appears in `tile-a.jsonl`:

```sh
open -n --env TILER_MACOS_STAGE_TIMING=1 \
  --stdout target/macos/responsiveness-rerun3/tile-a.jsonl \
  --stderr target/macos/responsiveness-rerun3/tile-a.err \
  target/macos/OmniTilerProbe.app --args tile --user-start \
  --scope com.apple.TextEdit --input tap --seconds 120 --log verbose
```

| Elapsed seconds | Physical actions |
| --- | --- |
| 0-10 | Let admission settle. Choose a real neighboring pair, not an edge. |
| 10-35 | Ten alternating focus pairs, Ctrl+H then Ctrl+L, about 1 second per press; idle for the remaining time. Use Ctrl+J/K instead if the pair is vertical. |
| 35-60 | Ten corresponding move pairs, Ctrl+Shift+H then Ctrl+Shift+L (or J/K), about 1 second per press; idle for the remainder. |
| 60-76 | Close the focused disposable window at 60 (Cmd+W), wait 3 s, focus pair at 63/64, move pair at 65/66; open a blank window at 68 (Cmd+N), wait 3 s, focus pair at 71/72, move pair at 73/74. |
| 76-92 | Repeat the same cycle, adding 16 s to those times. |
| 92-108 | Repeat, adding 32 s to the first cycle's times. |
| 108-120 | Idle until the bounded run finishes. Note visible focus failure, close gap and second-window lag, including approximately when they change. |

- Do not force cleanup to obtain a clean result. After expiry, require the
  terminal outcome below; retain both raw files unchanged even on failure:

```sh
jq -s -e 'map(select(.outcome == "completed")) | length == 1 and
  .[0].shutdown_clean == true' target/macos/responsiveness-rerun3/tile-a.jsonl
grep '^tiler-diag ' target/macos/responsiveness-rerun3/tile-a.err | head -n 5
```

### Mechanical cause analysis

- Save the following fenced block outside the checkout as `analyze-diag.sh`;
  run `sh analyze-diag.sh target/macos/responsiveness-rerun3/tile-a.err > analysis.json`.
  Analyze one complete process log, never concatenated runs. Output preserves
  raw lines and roster tails; numeric-looking ordinary fields remain strings.
- `close_stats` is milliseconds: `ap` absence-to-prune, `pr` prune-to-report,
  `rr` report-to-reply, `rb` reply-to-owner-batch completion. Report source
  `(pid,gen,dispatch)` joins prune's app-local `obs`; report/reply global `obs`
  and batch `worker_obs` are different namespaces. Reply joins unique `req`;
  batch joins the exact nonzero report trace in its CSV. Missing/ambiguous
  joins are censored, never zero. Focus request-to-owner joins require unique
  request identity; repeated requests are intentionally censored.
- Notification is coalesced receipt, without destroyed wid. Snapshot pairs
  join exactly by `(pid,snap)` but are not causally joined to engine reports;
  temporal proximity alone is approximate. Post-batch direct snapshots are
  represented by `batch-done`, not necessarily a `snapshot-start/end` pair.
  Never subtract old native `now_ms`, ring `t` and new `stamp_us` origins.

```sh
#!/bin/sh
# Usage: sh analyze-diag.sh <one-process-stderr>
# Even median is the mean of the two middle samples.
jq -Rn '
def stats(a): (a|sort) as $s|($s|length) as $n
  | if $n==0 then {n:0,min:null,median:null,max:null}
    elif $n%2==1 then {n:$n,min:$s[0],median:$s[($n/2|floor)],max:$s[-1]}
    else {n:$n,min:$s[0],median:(($s[$n/2-1]+$s[$n/2])/2),max:$s[-1]} end;
def dur(a;b): if a==null or b==null or (b<a) then null else (b-a)/1000 end;
[inputs] as $L
| [$L[]|select(startswith("tiler-diag "))] as $D
| [$L[]|select(test("^tiler-log .*kind=diag-"))] as $R
| (($D|length)==0 and ($R|length)==0) as $none
| [$D[]|(.|split(" roster=")) as $halves|($halves[0]|split(" ")) as $toks
  | ($toks|map(select(test("^[^=]+=")))|map(split("=")|{(.[0]):(.[1:]|join("="))})|add//{}) as $kv
  | {raw:.,stage:($kv.stage//null),stamp_us:(try ($kv.stamp_us|tonumber) catch null),
     fields:($kv|del(.stage,.stamp_us)),
     roster:(if ($halves|length)>1 then ($halves[1:]|join(" roster=")) else null end)}] as $REC
| [$D[]|select(test("stage=hold-cover"))
  | capture("obs=(?<o>[0-9]+).*reply=(?<r>[0-9]+).*req=(?<q>[0-9]+).*gap=(?<g>[0-9]+).*fg=(?<f>[^ ]+).*members_readable=(?<mr>[0-9]+).*members_unreadable=(?<mu>[0-9]+).*unreadable=(?<u>[^ ]*).*skipped=(?<sk>[^ ]*).*roster=(?<ro>.*)$")] as $HC
| ($HC|map(.ro|split(","))|add//[]) as $RE
| ([$RE[]|select(test("^[0-9]+:[0-9]+:[A-Za-z-]+$"))|split(":")[2]]|group_by(.)|map({(.[0]):length})|add//{}) as $appcause
| ([$RE[]|select(test("^\\+[0-9]+-more$"))]|length) as $trunc
| ([$RE[]|select(.!="complete" and .!="")|select(test("^[0-9]+:[0-9]+:[A-Za-z-]+$")|not)|select(test("^\\+[0-9]+-more$")|not)|split(" ")[0]|split(":")|last]|group_by(.)|map({(.[0]):length})|add//{}) as $memreason
| [$D[]|select(test("stage=hold "))
  | capture("authority=(?<a>[^ ]+)")|.a] as $HS
| [$D[]|select(test("stage=focus "))
  | capture("stamp_us=(?<u>[0-9]+).*id=(?<id>[^ ]+).*cause=(?<c>[^ ]+).*report=(?<rp>[0-9]+).*outcome=(?<o>[^ ]+)")
  | {id:.id,c:.c,rp:(.rp|tonumber),o:.o,u:(.u|tonumber)}] as $F
| [$D[]|select(test("stage=focus-feedback"))
  | capture("stamp_us=(?<u>[0-9]+).*close=(?<c>[^ ]+).*setter=(?<s>[^ ]+).*reason=(?<r>[^ ]+)")
  | {c:.c,s:.s,r:.r,u:(.u|tonumber)}] as $FB
| [$D[]|select(test("stage=focus-done"))
  | capture("outcome=(?<o>[^ ]+).*cause=(?<c>[^ ]+)")|"\(.o):\(.c)"] as $FD
| [$D[]|select(test("stage=frame "))
  | capture("attempted=(?<a>[0-9]+).*applied=(?<p>[0-9]+).*readbacks=(?<rb>[0-9]+).*unreadable=(?<u>[0-9]+).*leg0=(?<l0>[^ ]+).*leg1=(?<l1>[^ ]+).*leg2=(?<l2>[^ ]+)")
  | {a:(.a|tonumber),p:(.p|tonumber),rb:(.rb|tonumber),u:(.u|tonumber),legs:[.l0,.l1,.l2]}] as $FR
| [$D[]|select(test("stage=snapshot-start"))
  | capture("stamp_us=(?<u>[0-9]+).*snap=(?<s>[0-9]+).*pid=(?<p>[0-9]+)")
  | {snap:(.s|tonumber),pid:(.p|tonumber),u:(.u|tonumber)}] as $SS
| [$D[]|select(test("stage=snapshot-end"))
  | capture("stamp_us=(?<u>[0-9]+).*snap=(?<s>[0-9]+).*pid=(?<p>[0-9]+)")
  | {snap:(.s|tonumber),pid:(.p|tonumber),u:(.u|tonumber)}] as $SE
| [$D[]|select(test("stage=close-absence") and contains(" wid="))
  | capture("stamp_us=(?<u>[0-9]+).*pid=(?<p>[0-9]+).*gen=(?<g>[0-9]+).*obs=(?<o>[0-9]+).*wid=(?<w>[0-9]+)")
  | {pid:(.p|tonumber),gen:(.g|tonumber),wid:(.w|tonumber),obs:(.o|tonumber),u:(.u|tonumber)}] as $AB
| [$D[]|select(test("stage=close-pruned"))
  | capture("stamp_us=(?<u>[0-9]+).*pid=(?<p>[0-9]+).*gen=(?<g>[0-9]+).*obs=(?<o>[0-9]+).*id=(?<id>[^ ]+)")
  | {pid:(.p|tonumber),gen:(.g|tonumber),obs:(.o|tonumber),id:.id,u:(.u|tonumber)}] as $PR
| [$D[]|select(test("stage=close-report"))
  | capture("stamp_us=(?<u>[0-9]+).*dispatch=(?<d>[0-9]+).*obs=(?<o>[0-9]+).*req=(?<q>[0-9]+).*trace=(?<t>[0-9]+).*pid=(?<p>[0-9]+).*gen=(?<g>[0-9]+)")
  | {dispatch:(.d|tonumber),obs:(.o|tonumber),req:(.q|tonumber),trace:(.t|tonumber),pid:(.p|tonumber),gen:(.g|tonumber),u:(.u|tonumber)}] as $RP
| [$D[]|select(test("stage=close-reply"))
  | capture("stamp_us=(?<u>[0-9]+).*dispatch=(?<d>[0-9]+).*obs=(?<o>[0-9]+).*req=(?<q>[0-9]+)")
  | {dispatch:(.d|tonumber),obs:(.o|tonumber),req:(.q|tonumber),u:(.u|tonumber)}] as $RL
| [$D[]|select(test("stage=batch-done"))
  | capture("stamp_us=(?<u>[0-9]+).*pid=(?<p>[0-9]+).*worker_obs=(?<w>[0-9]+).*traces=(?<t>.*)$")
  | {pid:(.p|tonumber),wo:(.w|tonumber),traces:(.t|split(",")),u:(.u|tonumber)}] as $BD
| [$D[]|capture("stage=(?<s>[A-Za-z-]+)")|.s]|group_by(.)|map({(.[0]):length})|add//{} as $stc
| [$R[]|capture("kind=(?<k>[^ ]+)")|.k]|group_by(.)|map({(.[0]):length})|add//{} as $ring
| [$AB|group_by([.pid,.gen,.wid])[]|sort_by(.u)|.[0] as $a
  | ([$PR[]|select(.pid==$a.pid and .gen==$a.gen and .id=="\($a.pid)_\($a.wid)" and .u>=$a.u)]|sort_by(.u)|.[0]) as $p
  | (if $p==null then [] else ([$RP[]|select(.pid==$p.pid and .gen==$p.gen and .dispatch==$p.obs and .u>=$p.u)]|sort_by(.u)) end) as $rpc
  | (if ($rpc|length)==1 then $rpc[0] else null end) as $rp
  | (if $rp==null then [] else ([$RL[]|select(.dispatch==$rp.dispatch and .req==$rp.req and .u>=$rp.u)]|sort_by(.u)) end) as $rlc
  | (if ($rlc|length)==1 then $rlc[0] else null end) as $rl
  | (if $rp==null or $rl==null or $rp.trace==0 then [] else ([$BD[]|select(.pid==$rp.pid and (.traces|index(($rp.trace|tostring))) and .u>=$rl.u)]|sort_by(.u)) end) as $bdc
  | (if ($bdc|length)>=1 then $bdc[0] else null end) as $bd
  | {pid:$a.pid,gen:$a.gen,wid:$a.wid,abs:$a.u,
     prune:(if $p==null then null else $p.u end),
     report:(if $rp==null then null else $rp.u end),
     reply:(if $rl==null then null else $rl.u end),
     batch:(if $bd==null then null else $bd.u end),
     censored:(if $p==null then "prune-missing" elif ($rpc|length)>1 then "report-collided"
       elif ($rpc|length)==0 then "report-missing" elif ($rlc|length)==0 then "reply-missing"
       elif ($rlc|length)>1 then "reply-collided" elif $rp.trace==0 then "batch-trace-unjoinable"
       elif ($bdc|length)==0 then "batch-missing" else null end),
     d_ap:dur($a.u;if $p==null then null else $p.u end),
     d_pr:dur(if $p==null then null else $p.u end;if $rp==null then null else $rp.u end),
     d_rr:dur(if $rp==null then null else $rp.u end;if $rl==null then null else $rl.u end),
     d_rb:dur(if $rl==null then null else $rl.u end;if $bd==null then null else $bd.u end)}] as $C
| {no_diagnostics:$none,
 stage_counts:$stc, ring_diag_counts:$ring, diagnostic_records:$REC,
 hold_roster_app_causes:$appcause, hold_roster_member_reasons:$memreason,
 hold_roster_truncated:$trunc, hold_raw_roster:($HC|map(.ro)|unique),
 held_scope_counts:($HS|group_by(.)|map({(.[0]):length})|add//{}),
 focus_outcome_counts:($F|map(.o)|group_by(.)|map({(.[0]):length})|add//{}),
 focus_cause_counts:($F|map(.c)|group_by(.)|map({(.[0]):length})|add//{}),
 feedback_reason_counts:([$FB[]|.r]|group_by(.)|map({(.[0]):length})|add//{}),
 focus_done_counts:($FD|group_by(.)|map({(.[0]):length})|add//{}),
 frame_leg_counts:([$FR[].legs[]]|group_by(.)|map({(.[0]):length})|add//{}),
 frame_attempted:[$FR[].a], frame_applied:[$FR[].p],
 frame_readbacks:[$FR[].rb], frame_unreadable:[$FR[].u],
 snapshots:[$SS[] as $s|{snap:$s.snap,
   dur:(([$SE[]|select(.snap==$s.snap and .pid==$s.pid)]|if length==1 then dur($s.u;.[0].u) else null end))}],
 fb_joins:([$F|group_by(.id)[] as $g|($g[0]) as $f|($g|length) as $ng
   | ([$FB[]|select(.c==$f.id)]|length) as $nfb
   | ([$FB[]|select(.c==$f.id and .u>=$f.u)]|sort_by(.u)|.[0]) as $m
   | {id:$f.id,unique:($ng==1),
      joined:(if $ng==1 and $nfb==1 and $m!=null then $m.u else null end)}]),
 collided_report_keys:([$RP|group_by([.pid,.gen,.dispatch])[]|select(length>1)|.[0]|[.pid,.gen,.dispatch]]),
 unjoinable_trace_reports:([$RP[]|select(.trace==0)|.obs]),
 closes:$C,
 close_stats:{ap:stats([$C[].d_ap|select(.!=null)]),
   pr:stats([$C[].d_pr|select(.!=null)]),
   rr:stats([$C[].d_rr|select(.!=null)]),
   rb:stats([$C[].d_rb|select(.!=null)])}}
' "$1"
```

- Inspect `diagnostic_records` around each hold transition, the first
  non-dispatched focus, and close identities. Distinguish already-satisfied,
  absent request and refusal; compare target/observed/known. Frame arrays count
  per-window transactions, not aggregate `ax-*-n` batches. For hold clear use
  ordered ring `kind=diag-reply` controlled/held outcomes plus exact request
  `hold-cover`/report inputs; do not infer clear from a missing held line.
- Verification: latest final-source library 1600 passed, 0 failed, 3 ignored;
  library, binary build and all-target clippy raw exits 0, unchanged 24 library / 68 test
  warnings (16 duplicates). Formatting/diff checks pass. These precede this
  docs-only update. The extracted analyzer passes 42 headless assertions,
  including three 1/2/3 ms synthetic closes, missing endpoints, namespace
  separation, collisions, roster tail retention and no diagnostics in rerun2.
  Synthetic durations do not measure native latency. No new signed launch.
- Exact next action: rebuild the signed bundle and perform the single capture
  above, then classify its first divergent route/hold/frame predicate before
  choosing any correction. One capture may still leave censored evidence;
  unknown stays unknown rather than becoming a core defect claim.

## Rerun3 fault isolation (2026-11-12)

- Input: the unchanged `responsiveness-rerun3/tile-a` diagnostic capture;
  8007 stderr lines, one completed clean shutdown. The exact mechanical
  recipe above parses successfully. Its three close joins are uncensored.
  All source anchors below are relative to `crates/`.
- Summary: 163 reports/replies, 149 controlled and 14 incomplete-inventory
  holds (4/4/6 by close), all roster `cg-alive`. Explicit focus: 16 native
  dispatches, 29 already satisfied, 5 outbox-full no-calls. The 109
  `positive-close:envelope-absent` diagnostics include ordinary reports with
  no request; they are not 109 failed close attempts. Their `observed=- known=0`
  fields are placeholders, not retained foreground evidence.

### Focus outbox: fixed, adapter-owned

- `tiler-macos/src/runtime_types.rs:1296` defaults the bound to 16. Native
  focus actuates through the per-app owner slot, not through
  `Runtime::drain_focus_requests` (`runtime.rs:1681`), which has no production
  caller. Terminal `CloseFocusResult` previously staged raw feedback without
  removing the matching accounting row. Core feedback delivery/settlement
  is separate (`world_runtime.rs:541-552`), and does drain on later reports.
- Each completed attempt leaked one row. Capture line 1986 is the first
  `focus-40 no-call-outbox-full-superseded`, after exactly 16 native calls.
  Admission, positive-close and focus-follows-cursor share this dispatcher,
  outbox and fault; selection-release uses the same outbox too.
- Terminal results now retire exact `(request_id, window, close_id)` rows,
  including stale results; admitted native replacements compact superseded
  same-app rows after the unchanged capacity fences. Feedback stays in its
  separate staged slot. The extra close id prevents selection-command/local
  focus-counter numeric collisions from retiring newer work. See
  `world_dispatch.rs:3074-3113`, `runtime_thread.rs:2206`, and the two native
  enqueue sites. No capacity increase, retry or safety-gate bypass.
- `directional_focus_repeated_roundtrips_never_exhaust_outbox` uses real
  codec/core and mocked native focus: 18 alternating directional commands,
  one call and reported Accepted feedback per round, target observation
  settling each request. Neutralizing the retirement/replacement hunks
  reproduces 16 calls then the 17th refusal (exit 101); restored code passes.
  `close_same_pid_purge_keeps_replacement_and_stale_retire_is_noop` covers
  replacement, colliding numeric ids, stale retirement and exact unwind.

### Close roster: confirmed, no small safe correction

- `tiler-macos/src/runtime_core.rs:894-1042` retains AX-absent windows when
  the fresh CG oracle still lists them, or when the oracle is unreadable.
  This protects hidden-Space blindness and hidden/minimized lifetimes;
  AX absence alone must not become destruction. The existing
  [retain/hold decision](../decisions/macos.md#v2-implementation-cutover)
  remains unchanged. The 1 s backstop is provisional, not a close deadline.
- The callback discards notification name/element (`ffi.rs:3189-3205`), and
  `native_tile_backend.rs:1845` consumes only a coalesced count. The capture
  therefore proves receipt of a wake, not an exact-window destroyed event.
  Exact destroyed-window acceptance needs identity captured from existing
  subscriptions, owner/lifetime/generation fencing, snapshot transport and
  roster tests: medium-sized work, not a safe few-line exception. An earlier
  bounded reread is smaller but adds AX/CG load and still cannot prove close
  until CG changes. Neither option was implemented; retain the guard and
  scope exact destroyed evidence separately rather than guessing absence.

Times below are seconds from `stamp_us` only, not ring `t` or native `now_ms`.
Receipt-to-snapshot proximity is temporal; report/reply/batch joins use the
recipe's exact identities. Held reports queue zero frames and no refocus.

| Closed wid | Coalesced receipt; first snapshot | First incomplete report; held reply | CG absent; complete report; controlled reply | Last setter; batch done | Clear observation |
| --- | --- | --- | --- | --- | --- |
| 3732 | 60.189867; snap75 60.190057-60.244598 | 60.246194; 60.257740 | 61.235816; 61.236541; 61.245285 | 61.292196; 61.304041 | app obs53, backstop snap76 |
| 3737 | 77.967939; snap95 77.968115-78.016105 | 78.017072; 78.027380 | 79.025502; 79.026384; 79.035607 | 79.071700; 79.089154 | app obs62, backstop snap96 |
| 3742 | 93.155373; snap113 93.155445-93.187002 | 93.187629; 93.193909 | 93.547466; 93.548209; 93.555802 | 93.587519; 93.605224 | app obs73, owner post-batch readback; no poll-snapshot join |

- Raw line ranges: first close 4442-4625, second 5275-5465, third 6057-6314.
  Each clear is `cg=absent fate=pruned`, not a backstop-trigger exemption.
  Complete replies queue two frames: one actual frame transaction with one
  setter and three readbacks, one redundant skip. No refocus request is
  emitted, including after completion. Third-clear observation precedes
  notification snapshot114 (93.626249), which cannot have caused that clear;
  the preceding empty-batch readback ends at 93.526095. Its direct snapshot
  is not separately numbered (`runtime_worker.rs:1694-1708`).
- Absence-to-prune min/median/max: 360.551/990.964/1009.402 ms.
  Prune-to-report: 0.509/0.522/0.577 ms; report-to-reply:
  7.593/8.744/9.223 ms; reply-to-batch: 49.422/53.547/58.756 ms.
  Receipt-to-last-setter: 1102.329/1103.761/432.146 ms by close.

### Forward to core: implicit subject does not recover after hold

- Retiling recovers; implicit commands do not. Capture lines 4591-4607 show
  complete, known foreground `86169_3294` and controlled reply after prune.
  Lines 4689-4698 then show the same complete known foreground followed by
  `req-95 cmdseq=41 focus-directional ... focus-unknown`. There are 21 such
  post-clear refusals, including move-directional (4841/4888), through 6510.
  Thus neither continuing CG hold nor the separate focus outbox explains
  these command refusals. The adapter retains and reports the survivor.
- Source-traced cause at the capture revision (`632ee7d9` plus adapter changes):
  native focus changes from the closing window to the survivor while inventory
  is held. Canonical recording runs only on `prev != current`, fails closed in
  held homes and clears stale execution slots. Subsequent complete same-focus
  reports skip recording; no pending focus request restores the slots. Missing
  execution subject then refuses at `tiler-core/src/world_commands.rs:68-74`.
  This is a historical trace, not a reproduction on the rebased core.
- Rechecked source anchors after rebase onto `023bc7ec`: the changed-focus
  record/clear is now `tiler-core/src/world_run_v4.rs:363-377`; held-home
  recording fences are `world_sends.rs:109-122,163-164`; the missing-subject
  refusal remains `world_commands.rs:68-74`. Upstream has consolidated to one
  cursor and added an observed-live-bound-foreground fallback when no cursor
  or pending request exists (`world_close_focus.rs:103-130`). That changes the
  historical missing-slot mechanism. The core owner must check this exact
  hold/clear sequence and pending-intent regression against that fallback
  before choosing any further fix; resolution is not established here.
- Minimal core reproduction to add: complete A-focused two-window report;
  incomplete inventory with known B focus; complete inventory with the same
  B focus; implicit directional focus and move. Expected: B resolves as the
  execution subject without another native focus call. Actual capture:
  `focus-unknown` persists. Internal slot state is inferred from source and
  refusal, not directly logged; no isolated core reproduction was run here.
- Proposed core-owned small correction: re-establish an absent execution
  subject on an eligible complete known observation even if foreground did
  not change since the held report. Preserve pending intent, unknown focus,
  home/readability/exclusion/hold fences; do not overwrite a live pending
  cursor with unchanged stale observation. Add hold-clear and pending-intent
  regressions before implementation. Core/protocol behavior is unchanged here.
- Core defect handoff: platform-neutral, not a macOS-specific policy. Sequence:
  complete inventory with A focused -> incomplete inventory/domain held with
  known B focus -> complete inventory, focus still B -> implicit-subject
  focus-directional or move-directional. On macOS the held interval can last
  about 1 s after a close while the destroyed window remains in the system
  window list. Expected execution subject: B. Actual: every later implicit
  command in the capture refuses `focus-unknown` despite `foregroundKnown=true`,
  B bound and a complete roster. The mechanism above is a source trace, not an isolated
  core-test reproduction. Restore an absent subject from an eligible complete
  known foreground without overwriting a live pending focus intent; regress
  both unchanged-focus hold/clear and pending-intent cases.
- Rerun4 user observation, unanalysed: opening a window restored commands;
  sending a window to another workspace appeared to kill them permanently.
  The separate raw-log observations below do not establish that permanence
  or a successful native send.

### Verification and remaining acceptance

- Direct final-source library: 1602 passed, 0 failed, 3 ignored, exit 0.
  Binary build at deployment floor 15.0 and all-target clippy exit 0.
  Clippy remains 24 library / 68 library-test warnings (16 duplicates), no
  new findings in the changed hunks. The 18-round pre-fix exit 101 and focused
  104-pass sweep are execution-reported; the failure log was directly read.
- Additional integration execution reported six `runtime_phase1` failures
  and four `scoped_tile` failures. The six were reproduced with these fix
  hunks absent; the other four have the same absent-envelope/timeout signature
  but were not independently baseline-tested. No integration pass is claimed.
- Bonus: the 16 explicit `timing-focus-settle` input-origin-to-typed-feedback
  totals give min/median/p95/max 46.924/102.403/140.674/140.674 ms (nearest-rank
  p95). Move input-origin-to-last-setter lacks an unambiguous exact join and
  is not reported. Frame-leg counts: applied 143, redundant 81,
  `safety:frame-superseded` 7, no other logged refusal predicate.
- Next: forward the core recovery sequence above; after its regression/fix,
  rebuild the signed bundle and repeat the same 120 s action table with a
  fresh `responsiveness-rerun4` directory. Require focus to continue beyond
  16 actual changes and implicit moves/focus to work after each complete
  close recovery. Residual close delay remains expected until separately
  implementing exact positive-destroyed evidence; native acceptance is pending.

## Rerun4 unanalysed observations (2026-11-12)

- Scope: quick observations only, no troubleshooting, new diagnosis or fix.
  User reports same-workspace TextEdit focus/move spam remained usable until
  a close; a new window restored both. Near the end, sending a window to
  workspace 2 appeared to disable both permanently. Physical behavior is
  user-reported; the bounded capture cannot establish permanence.
- Capture boundary: rerun4 stderr was appended to the rerun3 stderr file,
  rather than written beside rerun4 stdout. Only appended-file lines
  8008-33367 belong to rerun4, beginning at the second runtime `seq=1 t=0ms`
  start. Lines 1-8007 remain rerun3 and were excluded. All line anchors below
  are absolute lines in that appended stderr file. Capture files unchanged.
  Rerun4 stdout reports one 120 s completion with `shutdown_clean=true`,
  four joined workers and zero retained workers.
- Native focus: 72 `stage=focus outcome=dispatched-native` records (not 144:
  the async diagnostic duplicates that token). First at line 8326, last at
  32267; zero `outbox-full` routes. This exceeds the previous 16-call limit
  and supports the outbox fix holding in this run, not general acceptance.
- Command refusals: 62 `unchanged`, 15 `planner-noop`, 69 `focus-unknown`.
  The latter appear in three blocks: 32 between the first close and open,
  24 between the second close and open, 13 after the final workspace phase.
  Approximate ring-time blocks are 28.25-34.87 s, 48.81-52.58 s and
  85.06-92.45 s; timestamps precede nearby synchronous refusal lines, not
  exact command dispatch stamps. Earlier/later healthy intervals still
  contain directional no-ops, not missing-subject refusals.

| Close/open | Raw diagnostic observations (ring times approximate) |
| --- | --- |
| First close | Lines 15767-15863: window 3750 AX-absent but `cg=owned fate=retained-cg-alive`; four `held-incomplete-inventory` replies around 26.57-26.70 s. At 15902-15928, about 27.58 s, CG absence prunes it, inventory is complete with known survivor focus and reply is controlled. Implicit commands then refuse `focus-unknown`. At 17422-17435, about 39.77 s, roster grows 2 -> 3 with a new known foreground; next native focus dispatch is 17559, about 40.54 s. |
| Second close | Lines 18712-18749: window 3797 retained `cg-alive`, two incomplete-inventory holds around 47.15-47.21 s. At 18788-18812, about 48.25 s, CG absence prunes it; complete known-survivor report and controlled reply return. Implicit commands still refuse. At 19960-19978, about 59.49 s, roster grows 2 -> 3 with a new known foreground; native dispatch resumes at 20093, about 60.15 s. |

- Final workspace phase: lines 32367-32380, about 81.82 s, show
  `workspace-send-2` admitted but the native effect refused at `holder-chord`
  with `holder-disabled`, then settled `Refused`; focus route is
  `deferred-refused-focus-unproven`. This does not log a successful native send.
  A later focus route is `deferred-selection-pending`. Workspace-2 select
  and return-to-workspace-1 both report confirmed native arrival.
- After that phase: 19 `held-incomplete-inventory` diagnostics (one for ws-2,
  then 18 for ws-1), roster cause `cg-alive`. Lines 32523-32548 retain three
  first-workspace windows on the other workspace; lines 32612-32637 retain
  two other-workspace windows on return. Thirteen implicit commands refuse
  `focus-unknown` at 32674-33165. The last published report/reply at
  33259-33283, about 100.51 s, remains incomplete/held, now with unknown
  foreground, and queues zero frames. No later native focus dispatch or
  logged hold-clear was observed before clean shutdown at about 120.16 s.
- Counts and key close/open/send/tail lines were directly checked; the
  initial broader log summary was separately reported. These observations
  remain unanalysed and do not resolve the final workspace failure.

### Post-rebase sanity (2026-11-12)

- Rebase onto `023bc7ec` completed without conflicts. Direct offline locked
  macOS crate build: raw exit 0. Direct macOS library tests: raw exit 0,
  1602 passed, 0 failed, 3 timing-only ignored. These gates use the rebased
  source, not the earlier capture binary, and do not reproduce the core
  hold/clear sequence or prove native acceptance. Integration gaps recorded
  above remain; no integration rerun or new live test.
