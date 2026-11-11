# Core v4 review (post-rebuild)

Goal: review the rebuilt core and the core/platform contract against the
rebuild goals, and propose improvements. Pre-0.1: breaking changes are
acceptable.

Status: findings C1-C10 recorded 2026-11-11 against `cd5e9e82`; proposals are
candidates, not approved. Nothing here is product policy until recorded in
[decisions](../decisions.md). Finding ids use a `C` prefix to stay distinct
from the F/R ids in the [interface review](core-interface-review.md).

Scope: core-weighted. KDE is the most complete port, macOS is partial and
Windows has not been ported to v4, so adapters were read only where they
expose a core or contract issue. No test suite was run for this review. The
existing `world_eval` example was built in release for timing only.

## Review criteria

The rebuild goals this review checks:

1. Core holds all policy for the tiler, including workspace effects and
   interactions.
2. The platform handles input and updates window positions, workspaces and
   effects.
3. Core has no platform-specific functionality; host differences are generic
   flags or options.
4. Core reports world states and the platform only needs to apply the latest
   world state to be consistent.
5. The core/platform channel is high speed and low latency.
6. The implementation is the most obvious and straightforward one.
7. Core policy aligns with the Tier 1 input WMs (i3/sway, Hyprland, COSMIC)
   with no diverging functional decisions of our own.

## Verdict

The contract direction is right, but core has not landed in a good spot yet.
Three goals are only partly met (latest-state-only apply, no platform
specifics in core, simplest implementation), and core still stacks three
generations of architecture.

| Goal | Status | Main evidence |
| --- | --- | --- |
| 1. Core holds all policy | Mostly met | Classification, settlement, focus and presentation are in core; residue is C6 |
| 2. Platform only does input and actuation | Not assessed in depth | Contract obligations are heavy (C3, C4) |
| 3. No platform specifics in core | Partly met | No `cfg(target_os)` in core, but C6 |
| 4. Apply latest world state only | Partly met | True for desired rows, false for seven reply lanes (C3) |
| 5. High speed, low latency | Unproven | No budget or gate; measured numbers in C7 |
| 6. Most obvious implementation | Not met | C1, C2, C5, C8 |
| 7. Tier 1 alignment | Mostly met | Strong process; gaps in C9 |

## Findings and proposals

### C1. Core is three engines stacked

- `WorldEngine` rebuilds an old planner-protocol `CoreEvent` (owner,
  generation, correlation, revision, fingerprint) and calls `engine.handle`
  at 12 sites: `world_commands.rs:1374`, `world_operations.rs:1506` and
  `:1892`, `world_sends.rs:697` and `:937`, `world_gesture.rs:597`, `:798`,
  `:947`, `:1287` and `:1345`, `world_model.rs:1611` and `:1711`. A
  representative construction is `world_operations.rs:1486-1506`.
- The inner `Engine` then fabricates its own `AdapterAck Accepted` and
  post-observation and verifies it (`engine.rs:4027-4048`, `4423-4440`,
  `4649-4666`). No adapter takes part in that handshake any more.
- `CoreCommand` carries window, direction and mode as `String`
  (`boundary.rs`), re-parsed inside, although v4 commands are typed.
- Membership lives twice: `WorldEngine.members/mapping/views` and
  `Engine.sessions`.
- Production size of the inner stack: `engine.rs` 5884 lines, `session.rs`
  3402, `reconcile.rs` 2038, `contract.rs` 1440. `reconcile` is referenced by
  no other crate.
- The [send and R4 decision](../decisions/engine.md#workspace-send-and-cross-output-r4-convergence)
  already records "no surviving pair, ack, verify or pending state" for that
  path.
- Proposal:
  - Make session operations commit directly and delete the
    propose/ack/verify machine (`reconcile.rs`, most of `contract.rs`).
  - Replace `CoreEvent`/`CoreReply` with typed method calls.
  - Fold the per-domain sessions into the world workspace model so there is
    one state model.

### C2. `WorldEngine` is a roughly 105-field struct and transactions clone all of it

- Struct at `world_engine.rs:1085-1665`, every field `pub(crate)`, mutated
  from about 30 files of `impl WorldEngine`.
- Whole-struct clones: per report (`world_admit_v4.rs:530`), per command
  (`world_run_v4.rs:542`), per stage (`world_stage_v4.rs:154`), four in
  `world_sticky.rs` (`:744`, `:1017`, `:1389`, `:1654`), plus engine clones
  (`engine.rs:2547`, `:3902`) and session clones (`session/world.rs:265`,
  `:334`, `:491`, `session/ops/float.rs:81`).
- Each `WorldEngine` clone copies `last_report_v4`, `last_reply_v4` and the
  staged report.
- Proposal:
  - Split into owned sub-states with private fields: connection and
    sequencing, workspace model, settlement lanes, focus, intent,
    presentation.
  - Keep the replay cache outside the transactional state.
  - Prefer validate-then-commit over mutate-then-rollback.

### C3. The reply mixes state with one-time events

- "Apply the latest publication" is true only for the state half of the
  reply.
- `world_publish_v4.rs:146-160` takes or drains `gesture_settled`,
  `operation_settled`, `switch_settled`, `intent_writes` and `cursor_warp`;
  `effects` and `result` are also issuing-reply-only.
- `closeFocus` was converted to a retained request for exactly this reason
  ([contract](../porting/world-contract.md#retained-positive-close-focus):
  "an issuing reply may be skipped and its request acted on from a later
  retained snapshot").
- `intentWrites` is durable user state (float and sticky restart intent)
  delivered once.
- Publication itself mutates core state
  (`assemble_publication_rows(&mut self)`, `world_publish.rs:329`).
- Proposal:
  - Split `WorldReply` into `state` (idempotent, latest wins) and `events`
    (sequence-numbered, acknowledged by a watermark in the next report).
  - Make intent a level-triggered desired record per window.
  - Move focus effects to the `closeFocus` retained-request pattern, so one
    mechanism covers both.

### C4. Evidence lanes have proliferated

- There are six attempt lanes (frame, member, view, clear, creation, state)
  plus `LastWrite`, `ArrivalBracket` and `FocusFeedback`.
- Each has its own codec, size estimator and settlement module.
- This pulls against the [Simplicity](../principles.md#simplicity) rule
  "prefer converging to desired state over tracking individual operations".
- The contract already points at unification: `LastWrite.property` is
  "reserved for later separate lane migrations"
  ([raw visibility evidence](../porting/world-contract.md#raw-visibility-evidence))
  and `StateAttempt` is property-generic.
- Proposal: one
  `PropertyAttempt {subject, property, target, rev, setter, pre, post, attemptMs}`
  for frame, member, view, clear, state and visibility, with one settlement
  engine. Keep creation separate. This is cheapest before the Windows port
  starts.

### C5. The JSON wire format lives inside core

- `world_sizing.rs` (1851 production lines) and `world_sizing_v4.rs` (1183)
  are a byte-count twin of the encoder plus canonical encoders.
- Core report types carry raw field maps, and `world_admit_v4.rs:241-256`
  matches on the strings `"nativeWs"`, `"pre"`, `"post"` and `"kind"`.
- Reject kinds are `&'static str` (`world_engine.rs:67-90`) although the
  contract defines closed enums.
- The 4 MiB byte budget is enforced in core and again in the broker
  (`crates/omnitiler/src/world_broker.rs:600`, `:1840`).
- macOS has no wire, yet runs encode, decode, core, encode, decode in
  process per report (`crates/tiler-macos/src/world_dispatch.rs:115-170`).
- Proposal:
  - Core bounds by row counts only (the `MAX_*` constants exist); byte
    budgeting moves to the transport.
  - Delete the sizing twins from core.
  - In-process adapters call the typed API; codec conformance moves to
    tests.
  - Typed enums for reject kinds.

### C6. Platform specifics in core

- `WorldEngineConfig {fixed_send_admission, maximized_focus_fence}` is
  documented as "Platform opt-ins" (`world_engine.rs:1659-1682`).
  - Both are live: `session/world.rs:711-730`, `session/ops/focus.rs:100`
    and `:492`.
  - Windows sets them opposite to the default
    (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:83-86`).
  - The contract says "Automatic fixed-size float sends are allowed
    everywhere; host limits must not reintroduce platform policy opt-ins"
    ([ownership](../porting/world-contract.md#ownership-and-g4-exceptions)).
  - They are outside the handshake, so no wire fixture sees them.
  - Not traced: whether the world layer short-circuits these paths before
    the session predicates run.
- `focused_fullscreen_presentation_suspension` is never read in production
  (`world_engine.rs:1656`, `:2271`), though the KDE service still sets it
  (`crates/omnitiler/src/world_broker.rs:447`). The
  [decision](../decisions/world-rendering.md#focused-fullscreen-presentation-suspension)
  records "Rejected: construction opt-in". Suspension itself is
  unconditional through `focused_fullscreen_suspended_output_v4`.
- `window_classify.rs:152-199` holds Win32 shell class and executable tables
  (`Progman`, `Shell_TrayWnd`, `#32770`, `startmenuexperiencehost.exe`),
  matched against every host's class strings. The contract approved this,
  but it is platform data in core.
- `settings_catalog.rs:175-186` has `HostCaps {windows, kde, macos}`.
- Clean: no `cfg(target_os)`, `cfg(windows)` or `cfg(unix)` in core
  production code; the only platform-named public items are the `cosmic_v1`
  family.
- Proposal:
  - Delete the three flags; make the behaviour unconditional or a catalog
    setting.
  - Have the adapter normalise shell surfaces to neutral `WindowType` values
    (`desktop`, `dock`) or declare a rule table at handshake.
  - Move UI exposure flags to the adapters.

### C7. Latency is unbudgeted and unguarded

- The [Efficiency](../principles.md#efficiency) principle requires a stated
  per-report budget and a regression check. Neither exists: no benchmark and
  no timing in core.
- Per-report work today: grade, estimate, validate, digest, full equality
  against the last report, whole-engine clone, run, reply estimate plus exact
  byte count, then store the report and reply.
- Measurement method: the `world_eval` example (release build) fed
  line-delimited messages: the handshake and settings from the
  `bootstrap-settings` scenario fixture, then synthetic reports with N
  windows on one output, all tiled, frames already at the desired values,
  only the cursor changing between reports. Per-report time is the wall-clock
  difference between a 1-report and a 401-report run, divided by 400. The
  duplicate path repeats one identical report 400 times, which exercises
  decode, admission checks and cached-reply encode without a core run. One
  development machine, two runs each; indicative only.

| Windows | Full path per report | Duplicate path (codec and admission only) | Report bytes | Reply bytes |
| --- | --- | --- | --- | --- |
| 4 | 0.47-0.50 ms | not measured | 4841 | 2850 |
| 8 | 0.63-0.66 ms | not measured | 8545 | 4360 |
| 16 | 1.2-1.3 ms | 0.50 ms | 15973 | 7384 |
| 32 | 2.2-2.4 ms | not measured | 30873 | 13486 |
| 64 | 4.8-5.0 ms | 1.8 ms | 60531 | 25495 |
| 128 | 11.2-12.8 ms | 3.3 ms | 120037 | 49520 |

- Cost is linear at about 85 us per window for a report where nothing
  changed. Roughly 60-70% is the core run at 16, 64 and 128 windows, where
  both paths were timed.
- The larger latency item is in the KDE adapter: `WORLD_DEBOUNCE_MS = 120`
  (`kwin/src/world-entry.ts:195`) also gates commands
  (`world-entry.ts:2340`), so each key press waits 120 ms before the capture
  is sent. No reason is stated at the constant. Worth confirming with the
  existing debounce trace probes.
- Proposal:
  - State a budget and add a gate using the method above.
  - Skip unchanged domains.
  - Dispatch commands at zero delay and keep the debounce for native event
    storms only.

### C8. Readability

- 25 core production files exceed the 1500-line
  [soft cap](../principles.md#file-size); 14 core test files exceed 3000.
- Longest functions: `assemble_publication_rows` 1205 lines
  (`world_publish.rs:329`), `execute_command` 901 (`world_commands.rs:47`),
  `converge_observation_with_hidden` 828 (`session/world.rs:1418`),
  `settle_migration` 792 (`world_settle.rs:1026`).
- 41 functions take 8 or more parameters (maximum 14,
  `session/ops/workspace.rs:595`).
- Core production has 35 `allow(dead_code)` (22 in `engine.rs`) and 49
  `allow(clippy::too_many_arguments)`. At least one suppression is stale
  (`world_engine.rs:948-951`, whose function is now called from
  `world_run_v4.rs`).
- Comments record history rather than behaviour: 131 "legacy" mentions in
  core production code, including references to `run_accepted` and
  `WorldEngine::publish`, which no longer exist; "No behavior change" and
  stage or sub-increment labels in module headers.
- `_v4` suffixes remain on the only implementation.
- Workspace maps are keyed by raw `String` despite typed ids (`mapping`,
  `native_map`, `views`).
- `lib.rs` exposes 49 public modules; adapters import internals such as
  `session::DomainKey`, `boundary::CoreCommand` and `contract::ResizeMode`.
- Test constructors are public production API (`world_engine.rs:1687-1815`).
- Proposal: one facade module for the contract surface, a test-support
  feature, typed ids in maps, then a rename and comment pass after C1 and
  C2.

### C9. Tier 1 alignment

- The process is strong: 183 requirement rows (155 normative), a
  reference-outcome matrix and dated user decisions.
- Settings whose options cite no reference WM: `admission.anchor-fallback`
  and `admission.no-space` (`settings_catalog.rs:375-398`).
- Policies with no Tier 1 counterpart recorded: frame anti-fighting (500 ms
  backoff, 5 s cap, two-attempt burst in `world_frames.rs`), learned minima
  and the best-effort admission search. They look host-driven; the
  [Consistency](../principles.md#consistency) principle asks that each name
  its host limit.
- 28 requirement rows are OPEN with a "current" behaviour already
  implemented, for example REQ-FOC-01, REQ-MOV-02 and REQ-MOV-05.
- `LayoutPolicy` has one implementation (`CosmicV1Policy`, `policy.rs`);
  i3/sway and Hyprland alternatives are per-behaviour settings.
- 201 of 284 decision entries say "Why: not recorded".
- `world-contract.md:3` still says "not implemented authority" although v3
  is removed from core and KDE.
- Proposal: label host-compensation policy explicitly in the spec, and
  decide whether its two settings should be user-facing.

### C10. Two report ingress shapes

- KDE uses `handle_report_batched`, macOS `handle_report`
  (`world_engine.rs:2543`, `:2569`).
- The contract section is titled
  [Queued KDE ingress](../porting/world-contract.md#queued-kde-ingress).
- Proposal: one ingress taking a report plus an ordered command list on
  every platform.

## Time-based behaviour inventory

Found by search. Only the `world_frames.rs` constants and
`WORLD_DEBOUNCE_MS` were individually verified.

- Core: the only time-based policy is frame anti-fighting in
  `world_frames.rs`. Each constant has its reason stated at the definition,
  which satisfies the Simplicity rule:
  - `FRAME_TRANSIENT_BASE_MS = 500` (`:55`)
  - `FRAME_TRANSIENT_MAX_MS = 5000` (`:57`)
  - `FRAME_BURST_MAX_ATTEMPTS = 2` (`:109`)
  - `FRAME_BURST_QUIET_MS = 500` (`:111`)
- Protocol: no timers or delays. It only carries `attemptMs` stamps.
- KDE script (`kwin/src`): nine duration constants.

| Constant | Value | Location |
| --- | --- | --- |
| `WORLD_DEBOUNCE_MS` | 120 ms | `world-entry.ts:195` |
| `WORLD_TIMEOUT_MS` | 2 s | `world-entry.ts:194` |
| `INTENT_STORE_TIMEOUT_MS` | 2 s | `world-intent-store.ts:37` |
| `NATIVE_PREIMAGE_TIMEOUT_MS` | 2 s | `world-native-preimage.ts:42` |
| `TRAY_HEARTBEAT_MS` | 1 s | `tray-publisher.ts:2` |
| `GESTURE_LEASE_MS` | 20 s | `world-gesture.ts:17` |
| `EFFECT_FACTS_LEASE_MS` | 20 s | `world-entry.ts:214` |
| `WAIT_HOST_DEADLINE_MS` | 25 s | `world-entry.ts:223` |
| `PRESENTATION_READ_DEADLINE_MS` | 25 s | `world-presentation.ts:69` |

- KDE retry caps: `GESTURE_ACK_MAX_ATTEMPTS = 3` (`world-entry.ts:1581`) and
  `TERMINAL_CAPTURE_MAX_RETRIES = 2` (`world-entry.ts:5559`).
- Missing reasons: no stated reason was found at `WORLD_DEBOUNCE_MS`,
  `INTENT_STORE_TIMEOUT_MS`, `NATIVE_PREIMAGE_TIMEOUT_MS`,
  `GESTURE_LEASE_MS` or `TRAY_HEARTBEAT_MS`. The Simplicity rule asks for
  one where each limit is defined. This strengthens the debounce point in
  C7.
- KDE service (`crates/omnitiler/src`): `WAIT_LEASE_MS = 20_000`
  (`world_service.rs:73`) and `HOST_CALL_DEADLINE_MS = 25_000`
  (`world_service.rs:81`) carry reasons. The tray endpoint has a 100 ms
  registration retry with 3 attempts and a 30 s freshness window
  (`tray_endpoint.rs:20-22`) without adjacent comments.

## Suggested order

1. Contract shape before more porting: C3, C4, C10 and the C6 flags.
2. Core structure: C1, then C2, then C5.
3. C7 budget and gate, then the KDE debounce.
4. C8 and C9 clean-up.

## Limits of this review

- macOS and Windows adapters were not reviewed in depth.
- How the KDE script treats event lanes on superseded replies was not
  traced.
- Counts of suppressions, legacy mentions, file sizes and function sizes
  come from text search and an indentation-based scan; they are approximate.
- The timing numbers are synthetic and come from one machine; they include
  process pipe I/O and cover a single all-tiled workspace.
