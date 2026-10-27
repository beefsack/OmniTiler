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

### Offline continuation (2026-10-28)

- Audit confirmed more defects in the corrected driver: millisecond sampler
  durations reached `AddSeconds`, dispatch had no sampler-ready handshake,
  and fixture/job cleanup did not cover every failure path. Neither old driver
  is suitable for another live leg.
- A replacement external native-event helper was rejected before live use.
  Its reported 47 passing seam checks did not validate the real native backend or
  production trace schema. Direct execution of `review-regressions.ps1` with
  `pwsh -NoProfile -File` produced six expected-red findings (exit 1):
  - Production-shaped select rows cannot join: the helper requires `serial`,
    absent from `select_primary_line` in `tiling_sys/cli_workspace.rs` and
    `workspace_log` in `tiling_sys/history.rs`.
  - A thrown cleanup stage still yields aggregate `ok=true`.
  - The cloak P/Invoke names a nonexistent export instead of binding the
    managed overload to `DwmGetWindowAttribute`.
  - The event range covers destroy/show/hide/reorder, not cloak transitions.
  - The live capture branch sleeps without a Windows message pump.
  - Taskbar-relative z-order is reversed versus `order_vs_taskbar` in
    `ffi/pager.rs`: encountering the taskbar while walking `PREV` means the
    pager is below it, not above it.
- The replacement driver also lacks termination-proof, restoration-proof,
  main-experiment and final-cleanup bodies. Do not launch it. Its passing
  parser/seam suite is not a validated live harness. Review findings and
  failing checks remain with the external evidence, not production code.
- `scripts/windows-dev.ps1` is safe to dot-source: its dot-source guard returns
  before live action blocks. An initial claim to the contrary was refuted by
  source inspection; AST statement counts alone do not establish execution.
- No new live effects, appearance evidence, production changes or product
  red/green regression resulted. A read-only final process inventory found no
  owner, live fixture or legacy test-window processes. This is not a new
  restoration proof; the earlier full recovery evidence remains above.

## Blocker and next action

- Diagnosis remains incomplete after two failed live evidence drivers and an
  offline-rejected replacement. Stop this bounded investigation here. No
  production fix, product red/green regression or live fix verification is
  claimed; retain the active record and backlog item.
- Exact next action: pin the join to actual production select/admission rows,
  preserving tickless gate events, then validate a real pumping native-event
  backend and its low-level failure paths offline. Require correct cloak
  entrypoint/event coverage, z-order direction, millisecond bounds, readiness
  before dispatch, exact executable/PID/lifetime binding, and cleanup that
  fails whenever any required stage fails. Do not substitute all-in-one fake
  sampler/pump results for exercising the backend. Do not launch any of the
  three rejected drivers.
- Then prove non-hooking graceful/forced termination and one-window independent
  hide/restore recovery before a bounded fixture-only native-event experiment.
  Native hide/cloak/z-order events can establish a mechanism if observed;
  stable sampled facts cannot exclude brief disappearance or isolate repaint.
- A user physical high-frame-rate capture of the pager before/through/after a
  switch would establish the reported appearance and timing, not its cause.
  If the native probe finds no transient, correlate that capture with native
  events and paint/erase instrumentation before selecting a fix. Repaint,
  coverage and short native transients remain hypotheses. Latest-world
  coalescing is neither diagnosed nor implemented here.

## Decisions

- Tentative (pending user review): prefer a native-event-first causal probe
  before another pixel-sampling harness. This is an investigation method, not
  a rendering-policy change; it supplies no appearance proof on its own.
