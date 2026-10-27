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

## In-product investigation (2026-10-28)

- Direction supersedes the external-probe plan: instrument the owned pager in
  the existing Windows trace, then use the standard fixture-only workspace
  flow. Do not launch, repair or replace the rejected external drivers.
- Native observations retain ordered window-position flags, insert-after,
  show reasons, size/move, erase and paint entry/exit. Tiler-issued pager
  create/destroy, move/show, ensure-show, hide, calibration hide and topmost
  reassertion are separate events. The pager uses opaque GDI, not layered/DWM
  updates; managed-window cloak and departure-hide paths do not target it.
- Review rejected the first counter-only diagnostic diff: last-value flags
  erased transient hide/show ordering and an unconditional ensure counter
  defeated quiet tracing. Replaced with bounded ordered events. A subsequent
  failing-first regression caught repeated trace enable discarding queued
  between-pump messages; fixed without changing pager presentation behavior.
- Offline: 144 targeted pager tests pass; package formatting and strict
  all-target Windows clippy pass. These validate diagnostic plumbing, not the
  reported appearance or a product fix.

### Accepted fixture evidence

- Standard `mise exec -- just world-stress` passed the existing
  `stress-home-away-102` case: 102 commands, including 48 select commands,
  four sends and checkpoint-separated roundtrips plus rapid publication.
  This is command/native/pager-content evidence, not physical appearance
  acceptance. No rejected external driver was used.
- Preflight captured source plus diagnostic diff, artifact hashes, exact
  process/lifetime identities and settings/taskbar/SPI preimages. Existing
  non-hiding graceful/forced-stop and independent one-window hide/restore
  proofs passed before the fixture leg. The stress driver rebuilt its own
  payload; its recorded SHA-256 matches the retained executable, distinct
  from the earlier preflight/proof payloads.
- Direct inspection of the owner log: 242 `pager-wnd` batches, 747 events,
  zero omitted events, all drain contexts safe. No tiler hide, calibration
  hide, native `show=false`, `SWP_HIDEWINDOW` or switch-time destroy/recreate.
  One initial create persisted. All 51 tickless pager-gate rows were allowed;
  suppression was not observed. These exclusions apply to this fixture leg,
  not all environments or unobserved compositor output.
- A plain select at tick 29 joins `workspace` select to accepted `world-apply`,
  `pager-cells` and `pager-wnd` via `req-47`, observation/revision 47. Native
  sequence 86-87 is `tiler-move` then `pos-changing`, flags 80
  (`SWP_NOACTIVATE | SWP_SHOWWINDOW`), same opaque window token. The broader
  log contains 136 move/show calls, 136 paint entry/exit pairs, 141 erase
  messages and 21 topmost reassertions. Event counts alone do not prove
  each call's completion or compositor-visible atomicity.
- Code reading agrees: content changes also invoke `move_show`, which calls
  `SetWindowPos` and `InvalidateRect(..., erase=true)` even when the strip
  rectangle is unchanged. Paint draws cells directly with GDI. This is a
  repaint path, not proof that repaint caused the user's disappearance.
  Only five `pos-changed` messages accompanied 159 `pos-changing` messages;
  position-changing requests must not be treated as completed z-order moves.
  Existing native readback reported the pager visible/topmost/above taskbar,
  but is not a continuous occlusion oracle.
- Recovery readbacks: no owner, fixture or retained-proof helper processes;
  owner not ready; no recovery ledger or pending control markers; settings
  bytes and captured taskbar/SPI state matched their preimages.
- Final checks: `mise exec -- cargo test -p tiler-windows --lib` passed
  (1153 passed, one ignored); targeted pager tests passed (144); package fmt,
  strict all-target Windows clippy and `git diff --check` passed. Diagnostic
  regression was red before the cross-pump retention fix and green after.

## Blocker and next action

- Native hide/suppression/recreation is excluded for the observed fixture
  switches. Pager move/show and repaint activity is present; the trace does
  not establish which visible artifact the user experienced. No product
  flicker fix or visual improvement is claimed. Keep the bounded trace: it
  supplied previously missing native evidence and can correlate a dogfood
  reproduction without another external measurement driver.
- Exact next action: obtain user-owned physical high-frame-rate capture of
  the reported disappearance alongside this in-product trace. Capture is
  needed to distinguish actual surface disappearance, partial repaint and
  occlusion; native message/readback evidence alone cannot settle that
  appearance question. No additional agent live leg is planned here.
- Latest-world coalescing remains undiagnosed and unimplemented; the parked
  focus design is untouched. Retain this active record and backlog item.

## Decisions

- Tentative (pending user review): retain at most 128 ordered native pager
  events per drain with explicit overflow counts and opaque window-lifecycle
  tokens. Trace-disabled runs capture nothing. Existing pager-cell caps stay
  unchanged; unchanged presentations still report native activity.
- Tentative (pending user review): tick/request/observation/revision identify
  drain context for correlation, not causal attribution of queued OS messages.
  Sequence records observed order; pre-registration lifecycle is unknown.
