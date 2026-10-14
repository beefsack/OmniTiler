# macOS Phase 1 threading/lifecycle skeleton

## Goal and acceptance

Deliver the adopted main/UI, dedicated input, per-application AX and single
core-owner threading model as an offline-testable macOS adapter skeleton.
The engine uses existing `tiler-core::Engine` entry points; platform policy
extraction, live tiling, native Spaces and settings UI are separate work.

Acceptance: typed observations/actions/actuation/focus handoffs; bounded AX
messaging and thread-stop deadlines; wake-driven idle; app launch/termination
and stale-observation fencing; honest panic/timeout outcomes and structured
diagnostics. Fakes must prove a stalled app cannot stall input, the engine or
another app, termination invalidates in-flight results, out-of-order observations
cannot regress state, and stop during work remains bounded.

## Approach

- Keep the Phase 0 probes intact. Add a portable runtime and narrow adapter
  traits; native AX objects are created and destroyed on their owning threads.
- One engine thread owns the shared Engine and retained observation state.
  All native work is dispatched through per-app workers or a focus handoff.
- Use bounded nonblocking data handoffs and an independent stop/wake path.
  Desired frame state may coalesce; input/lifecycle outcomes must expose
  saturation rather than silently claim delivery. Reject stale app generations
  and observation revisions.
- Input matching reuses the existing configurable-modifier exact-chord matcher;
  the boundary forwards semantic actions without running the core.
- Stop closes admission and invalidates queued work, then joins only threads
  proven exited, under one finite deadline. Timeout or panic is unclean, never
  a false successful teardown.

## Work and verification

1. Confirm existing core-consumer and AX/input seams against source.
2. Implement runtime, core adapter and deterministic failure fixtures.
3. Review lifecycle races and reconcile evidence/documentation.

Required offline gates: macOS crate tests/build, all-target strict clippy,
workspace formatting and diff checks. Preserve non-macOS cfg-gating and the
existing license/notice gates; no additional dependency is expected.

## Delivered

`crates/tiler-macos/src/runtime.rs` supervises readiness, asynchronous app
launch, stop and reap. `runtime_thread.rs` owns the engine/input/app loops;
`runtime_core.rs` builds existing `CoreEvent` requests and consumes shared
plans. Supporting types, lifecycle accounting, observations and logging have
focused modules. Factories construct thread-affine backends inside their owners;
native AX handles never cross threads. Main/UI callers use asynchronous app
launch; bounded startup/launch conveniences are for a lifecycle controller.

The engine owns one `tiler_core::engine::Engine`, using `handle` for reconcile,
focus and move. Typed observations carry app generations and source sequences;
actuation uses whole latest-wins batches with stop/supersession fences. Already
in-flight AX calls may finish, but queued work is invalidated. Focus requests
remain unverified intent and are purged on termination/stop. Observation errors
preserve retained state; only complete observations can replace it. Identical
readbacks and already-observed frame targets do not trigger write loops.

The native per-app backend has a thread-affine AXObserver guard and checked
2 s per-element messaging timeouts before identity joins/reads/writes. It
registers pre-existing windows as well as new ones, pumps at most 20 ms per
slice, and reports subscription/teardown failures honestly. The dedicated input
boundary uses the existing exact raw-flags matcher; Ctrl remains default Meta.
Its native inactive source installs no tap and parks until stop. No live tiling
command or new desktop mutation entry point was added.

Partial startup retains its cleanup report and unfinished handles in
`StartFailure`; dropped handles signal every owner. Shutdown uses one deadline,
joins only proven exits, retains unfinished handles, and caches the terminal
report. Panic, unexpected source exit and refused teardown never become clean
stops, including after earlier reap. Normal structured stderr logs use a bounded
writer thread; timestamps/app-generation/reference IDs correlate events.
Saturation/drop counters and a bounded diagnostic ring expose lost log copies.

## Evidence and review

- Native offline `mise exec -- cargo test --locked -p tiler-macos`: 135 unit,
  9 bounded and 39 runtime integration tests pass. Fakes exercise actual core
  frame/focus/move plans, hung actuation and observation, termination mid-batch,
  out-of-order and old-generation observations, blocked startup, input/AX/engine
  panics, in-flight stop and later reap, teardown refusal, focus saturation,
  slow logging, redundant-write skipping, and concurrent terminate/shutdown.
- Build of the macOS crate and portable core/protocol/effect crates, all-target
  strict clippy (`-D warnings`), formatting and diff checks pass. No new
  dependencies or lockfile changes.
- [CI at `2bfc801`](https://github.com/beefsack/OmniTiler/actions/runs/38593292485)
  passes macOS and Windows portable build/test/strict-clippy, Linux workspace
  test/strict-clippy, formatting, KWin/shell suites, dependency license/bans/
  sources and third-party-notice freshness. The first CI run exposed an
  off-macOS unused timeout import; its cfg guard is corrected and both
  non-macOS gates pass. Native-effect CI remains blocked by a missing macOS
  workspace-member manifest in its build source; the same failure occurs
  [before this change](https://github.com/beefsack/OmniTiler/actions/runs/38592418369).
- Independent lifecycle/FFI review found a taken-batch stop race, startup cleanup
  gaps, incomplete modifier/generation fencing, a shutdown lock-order inversion
  and an AX timeout-before-join omission. Corrections and discriminating fixtures
  landed. Final source inspection also corrected pre-existing-window observer
  subscription, dropped-handle worker stop and stale latest-observation retention.
  Async-sink assertions now rendezvous with sink delivery itself.
- Unsafe additions are confined to the thread-affine observer section of
  `ffi.rs`; no custom unsafe Send/Sync. All new production modules stay below
  the file-size soft cap. `ffi.rs` already exceeded it and remains an exception
  to keep the approved unsafe boundary in one file.

Source shapes were checked in local reference checkouts; canonical pins are
[yabai application observers](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/application.c),
[rift observer](https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/observer.rs)
and [glide run loop](https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/sys/run_loop.rs).
Source shape is not native acceptance evidence. Durable implementation choices
are explicitly [tentative](../../decisions.md#scope-and-platform-goals).

## Limits and next action

This is a synthetic single-domain skeleton (fixed bounds/gaps), not admission
or workspace policy. No additive core API was required. Later extraction needs
the normalized eligibility/overlay/admission/origin policy identified in the
[post-Windows audit](../../research/cross-platform-core/post-windows-audit.md#c-core-extraction-candidates);
topology, native lifetime joins, close-versus-minimize truth, focus agreement and
native readback/recovery remain adapter responsibilities.

Native observer/actuation changes compile but have not been live exercised.
The run-loop slice and whole-app snapshot refusal can delay one app; other apps
and the engine continue. Actual tap delivery/recovery, Space writes, durable
stranded-window recovery and idle/latency measurements remain pending.

Exact next action: resume the Phase 0 owned regular-app focus roundtrip, prove
original -> target -> original agreement, then test chord delivery/suppression and
tap recovery before native Spaces probes. The skeleton can subsequently be
accepted on owned apps under the live-testing protocol.
