# Windows dedicated input thread

## Goal and scope

- Deliver stage 1 (complete A) of the selected target C: both low-level hooks
  and their synchronous consume/pass classifiers share one dedicated input
  pump. Selection is for latency, not Xbox prompt containment.
- Owner retains observation, Engine, foreign/foreground actuation, overlays,
  tray, settings, trace writing, Mouse Snap SPI and the single recovery ledger.
- Preserve current input behavior, pairing, game-safety and recovery policy.
  Render-thread migration is stage 2.
- Binding contracts: [design](../../research/windows-port/owner-threading.md#contracts-that-must-survive-the-split),
  [principles](../../principles.md), and the native
  [common definition of done](../../backlog.md#common-definition-of-done-and-live-check-protocol).

## Acceptance and approach

- Immutable owner-built snapshots and bounded ordered controls replace owner
  access to input-local TLS. Classifiers never wait for owner replies or locks.
- Keep both hooks together, including synchronous mouse-Down Win-up mask
  reservation, injected filtering and mask reentry protection.
- Preserve ordered config/invalidation, consumed held pairs, terminal delivery,
  saturated new-Down pass-through and monotonic Esc START/END snapshots.
  Mouse moves remain pass-through, without per-motion intents.
- Include readiness, narrow install failure/retry, graceful unhook acknowledgement
  and truthful bounded stop failure handling. No overlapping input threads.
- Input publishes bounded diagnostics to the owner writer. Trace queue age and
  installer dispatch gaps with explicit exclusions from actual input latency.

## Verification

- Existing classifier/queue fixtures plus deterministic snapshot/config races,
  consumed-pair/saturation/terminal edges, Esc ordering, mask reentry and
  install/retry/stop failure seams.
- Root mise-selected MSVC locked build/test for `tiler-core`, `tiler-protocol`,
  `tiler-kwin-effect-ffi`, `tiler-windows`; all-workspace fmt; allowlisted strict
  all-target Clippy; `git diff --check`; applicable CI after publication.
- Physical input/display acceptance remains pending user checks: scoped native
  edge drags (dev/release, trace on/off), chord holds/releases and preheld
  modifiers, Win+drag/Esc, focus/workspace actions, live settings and verified
  graceful stop/reveal/SPI restore. Compare input smoothness separately from
  owner settlement using the
  [comparison commands](windows-resize-hook-latency.md#exact-user-comparison-plan).

## Delivered implementation, 2026-10-12

- `input_handoff.rs` and `input_pump.rs` provide immutable whole snapshots,
  ordered bounded controls, policy-tagged single-outstanding intent batches,
  process-global monotonic Esc sequences and bounded diagnostic exchanges.
  Pump exchanges use try-locks outside callbacks; callbacks read local state
  and atomic freshness only. Owner publication/drain is in `tiling_sys.rs`.
- Policy publication arms an atomic freshness fence before transport. Keyboard
  and mouse callbacks refuse fresh consumption until applied, while existing
  classifier verdicts, swallowed pairs and Win-up masks remain input-local.
  Refused policy snapshots retain one retry generation, including a revert to
  unchanged content; every snapshot application acknowledges that generation.
  New owner runs allocate above the process-global generation maximum.
- Safety transport has an eight-item inbox, bounded pending FIFO, eight distinct
  invalidation slots and two clear flags. Overflow subsumes invalidations with
  an all-origin windrag clear, retaining Cancel/paired-Up obligations. The owner
  forces a full live snapshot behind the clear, even when desired content is
  unchanged. Duplicate collapse cannot cross a snapshot/control boundary.
- Every successful hook installation reapplies the retained snapshot. Controls
  also remove origins from retention so partial-failure recovery or gate
  refresh cannot resurrect an invalidated origin.
- Readiness, independent hook backoff/retry, creator-thread unhook retries and
  bounded stop reporting are included. A process-level pump reservation survives
  missing/unclean stop acknowledgements and dropped handles; only successful
  release/join frees it. Owner restore/reveal and the existing watcher/ledger
  remain authoritative.
- Trace-only native timestamp-to-callback gaps, pump dispatch spans, queue age
  and transport timing go to the owner writer with explicit coverage exclusions.
  These are diagnostics, not end-to-end input or mouse-to-photon measurements.

## Corrections and regression evidence

- Earlier latest-wins configuration, unbounded transport and a one-carried-item
  overflow path were rejected. The first safety correction still accumulated
  distinct HWNDs without a fixed bound; fixed slots plus clear/resend replace it.
- Freshness: real callback regressions
  `publication_fence_refuses_fresh_chord_through_real_callback` and
  `mouse_fence_refuses_fresh_down_but_swallows_consumed_up` cover stale fresh
  refusal and held-pair/mask retention. Owner publication fixtures cover refused
  policy/revert reuse, forced unchanged republish and applied-generation recovery.
- Safety controls: `sender_retains_distinct_safety_controls_under_stall`,
  `sender_escalates_to_clear_beyond_fixed_cap` and
  `stalled_overflow_escalates_and_resumes_through_pump_seam` cover distinct
  invalidations, fixed bounds and suspend Esc clears across repeated stalls.
  `clear_forgets_retained_then_forced_same_snapshot_restores` covers unchanged
  snapshot restoration; snapshot-boundary duplicate regressions retain newer
  safety obligations.
- Installation: production `PumpCore::step` regressions
  `retained_snapshot_reapplies_after_initial_install`,
  `retained_snapshot_reapplies_after_partial_install_failure` and
  `keyboard_recovers_without_rearming_invalidated_origin` cover initial and both
  partial-failure directions. Scripted backends create the same fresh TLS state
  as installation without native hooks or input effects.
- Whole-diff contract review is clean after correcting retry/revert liveness,
  clear-driven republishing, retained-origin resurrection, duplicate ordering,
  process-lifetime fencing/no-overlap and the remaining owner-TLS diagnostic
  read. Global fence/Esc fixtures serialize their shared state.

## Final offline gates and outcome

- Rust 1.99.0, `x86_64-pc-windows-msvc`; root mise-selected locked four-package
  build/test PASS: 1,552 passed, zero failed, one pre-existing ignored benchmark.
  Windows lib: 430 passed; Windows integration: 358 passed (tiling: 115).
  All-workspace fmt, locked four-package strict all-target Clippy with warnings
  denied and `git diff --check` PASS.
- Stage 1 is complete offline. Durable implementation choices are in
  [decisions](../../decisions.md#windows-owner-threading). No physical latency,
  hook-delivery or display acceptance is inferred from these fixtures.
- Pending user checks: scoped edge drags, four profile/trace combinations, held
  chords/repeats/releases, preheld modifiers, live bindings, Win+drag/Esc,
  focus/workspace actions and graceful stop with verified reveal/Mouse Snap
  restore. Input smoothness and owner settlement remain separate measurements.
- Render-thread migration is the next architectural stage. Gaming-cost evidence
  and Win+G/F11 containment remain separate; this delivery claims neither.
