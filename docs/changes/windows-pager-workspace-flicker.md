# Windows pager flicker on workspace switch

## Goal and scope

- Diagnose the pager disappearing briefly during workspace switches with causal
  evidence and fix the smallest uncontentious rendering defect.
- Exclude Windows latest-world coalescing and the parked workspace-return focus
  design. Preserve fullscreen/game safety and native authority gates. No timers,
  settle waits or retries as fixes.

## Acceptance and plan

1. Trace workspace switching through pager publication and native presentation;
   distinguish actual hiding from repaint or replacement artifacts.
2. Add a failing-first offline regression where practical, then implement a
   bounded fix. Stop at diagnosis if the cause requires coalescing or policy.
3. Verify fixture-only live behavior under the complete Windows testing guide,
   including preflight, provenance and recovery.
4. Review evidence, record material decisions, and archive this record on
   completion.

## Current evidence

- User dogfood 2026-10-27: pager disappears and reappears momentarily during
  workspace changes. Cause not yet established.
- Offline tracing found no unconditional hide/recreate on a plain switch.
  Pager frame changes call `move_show` and invalidate the native surface;
  suppression, output removal and DPI recreation remain conditional paths.
  Source: `crates/tiler-windows/src/pager_sys.rs` (`sync`, `hide_all`),
  `crates/tiler-windows/src/ffi/pager.rs` (`move_show`, `paint_frame`), and
  `crates/tiler-windows/src/tiling_sys/pager_host.rs` (`sync_taskbar_pager`).
- Initial fixture-only diagnosis joined successful select dispatches to pager
  and world traces. Sampled pager HWND/visibility persisted, but no causal
  conclusion is accepted: all pixel hashes failed, coordinates were mixed,
  and sampling cannot exclude short transients. The driver failed at shutdown
  serialization; cleanup was reported separately. Review also found the
  independent hide/restore proof ran too late. Correct the evidence harness
  and preflight/recovery order before another leg; do not change production
  behavior to compensate for failed evidence.
- Direct report inspection confirms 18-23 samples for each of seven probed
  switches, not the initially reported single sample. All sampled visibility
  flags were true and the pager HWND persisted. The rapid pair had command
  outcomes only, not appearance sampling. No valid pixels were captured.
- Native trace silence is not a continuous visibility oracle: `emit_native_state`
  probes after show effects and periodic z-order reassertion, and `emit_facts`
  suppresses equal signatures. Gate events without tick fields must not be
  dropped by tick-only joins. The full log contains allowed gate changes; no
  suppression is observed. These findings do not prove repaint is the cause.
- The corrected harness failed at its first fixture identity assertion before
  launching the main experiment. Its `Assert-OsIdentity` parameter `$P` collides
  case-insensitively with local `$p`; the fixture remained alive after failure.
  It also contains unvalidated sampler duration/cleanup paths. Do not rerun it
  as-is. No corrected physical-pixel or causal result is accepted.
- Recovery was independently verified after exact executable/hash/lifetime-bound
  termination of that disposable fixture: no owner or fixture processes, owner
  not ready, clean ledger, no pending workspace/directional/send/migration/stop
  markers, settings bytes and all captured taskbar/SPI fields unchanged.

## Blocker and next action

- Diagnosis remains incomplete after two failed evidence drivers. No production
  changes, offline red/green regression or live fix verification are claimed.
- Before any further live leg, validate a bounded evidence helper offline,
  including identity assertions, millisecond/second units, sampler readiness
  before command dispatch, physical coordinates, hash/PNG correctness, exact
  fixture joins, tickless trace events, and cleanup after each injected failure.
- Then prove non-hooking graceful/forced termination and one-window independent
  hide/restore recovery before the main fixture-only appearance experiment.
  Require actual before/through/after appearance evidence before selecting a fix.
- Leave this record active and retain the backlog item. Repaint, short native
  transients and coverage remain hypotheses; latest-world coalescing is neither
  diagnosed nor implemented here.

## Decisions

- None yet.
