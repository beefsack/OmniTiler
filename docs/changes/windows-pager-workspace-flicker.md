# Windows pager flicker on workspace switch

## Goal and scope

- Diagnose the pager disappearing briefly during workspace switches with causal
  evidence and propose the smallest supported remedy. Presentation behavior
  remains unchanged during this investigation.
- Exclude Windows latest-world coalescing and the parked workspace-return focus
  design. Preserve fullscreen/game safety and native authority gates. No timers,
  settle waits or retries as fixes.

## Acceptance and plan

1. Trace workspace switching through pager publication and native presentation;
   distinguish actual hiding from repaint or replacement artifacts.
2. Compare workspace switching with focus-only repaint, including native
   visibility sets, taskbar interaction and surface geometry.
3. Separate source guarantees and accepted native evidence from unproven
   compositor-visible explanations; identify the discriminating capture.
4. Report a proposed remedy and switch-latency implications. Keep the record
   active while the visible cause and remedy remain unresolved.

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

## Switch-versus-focus audit (2026-10-28)

### Observation boundary

- Updated dogfood report: one or two frames of apparent disappearance on
  workspace switches, never on window-focus changes. Disabling the NVIDIA
  overlay removed the separate notification-bell/tray shift; the remaining
  micro-flicker is the subject here. Earlier observations may mix the symptoms.
- This continuation is source analysis, not a new live or physical reproduction.
  Earlier accepted fixture traces exclude switch-time hide, suppression and
  recreation in that leg, but contain no accepted pixels of this micro-flicker.

### 1. Erase, paint and presentation

- `ffi/pager.rs:1012-1017` zero-initializes `WNDCLASSW` and never assigns
  `hbrBackground`: the class brush is NULL. Class flags are
  `CS_HREDRAW | CS_VREDRAW`, not application double buffering.
- `ffi/pager.rs:880-884,936` traces `WM_ERASEBKGND`, then calls
  `DefWindowProcW`; it neither fills nor hides the pager there.
  `ffi/pager.rs:349-360` calls `SetWindowPos` then
  `InvalidateRect(hwnd, NULL, TRUE)`. TRUE requests erasure when the update
  region is processed, not an immediate clear at invalidation. With no class
  brush, default handling supplies no class-brush background fill. Therefore
  erase-message counts do not establish a cleared surface or a clear-to-paint
  time gap. See the Microsoft
  [erase contract](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-erasebkgnd)
  and [invalidation contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-invalidaterect).
- `ffi/pager.rs:899-912` handles `WM_PAINT` by cloning the current frame.
  `:584-614,618-757` paints directly into the `BeginPaint` DC: each cell's
  background `FillRect` overwrites its old miniatures/text, then that cell's
  miniatures, borders and text are drawn. It does not clear the whole strip
  first in a separate pass. Focus-only repaint runs this same full-cell loop.
- `ffi/pager.rs:299-312,549-582` creates an opaque popup and blits opaque
  artwork, with no `WS_EX_LAYERED` or `UpdateLayeredWindow`. No pager-specific
  DWM attribute, region, reparenting or buffered-paint call was found in the
  native carrier or Windows source search. DWM's normal redirection still
  applies: [composition](https://learn.microsoft.com/en-us/windows/win32/dwm/dwm-overview)
  uses off-screen window surfaces, not literal drawing to the physical screen.
- Conclusion: the classic separate OS background-erase explanation is not
  supported. There is no application-owned complete-frame/single-blit boundary
  around the GDI drawing, so an intermediate drawing presentation cannot be
  excluded. Native paint entry/exit is not a compositor-present receipt. Paint
  delay alone should leave the old valid pixels, not prove disappearance.

### 2. What actually differs on a workspace switch

- Both active-index and miniature-focus changes fail `frames_equal`
  (`pager_sys.rs:234-247`). Both replace the stored frame and call the same
  `move_show` (`:765-782`): topmost `SetWindowPos` with
  `SWP_NOACTIVATE | SWP_SHOWWINDOW`, followed by full-client invalidation.
  Thus neither this call nor full-strip GDI repaint is switch-specific.
- Width is workspace count times DPI-scaled cell width, with the right edge
  at `tray_left - gap` (`pager.rs:444-467,610-616`). Active selection alone
  does not enter that formula. Trailing cleanup can remove invisible empty
  nonfinal workspaces or append a needed trailing workspace, with a two-slot
  floor (`tiler-core/src/workspace.rs:3,35-65`); count changes are conditional,
  not inherent to selecting an existing workspace. Compare x/y/w/h and ids
  on the actual flickering switches before attributing a resize.
- The pager is reused by output; removal and DPI change are the recreation
  conditions (`pager_sys.rs:675-714`). Hide/ensure-show and periodic topmost
  recovery are separate paths (`ffi/pager.rs:368-405`); the latter is on an
  existing two-second cadence (`pager_sys.rs:70,818-835`), not a switch phase.
- A further switch-specific shell call was found: the owner computes the
  workspace ordinal and updates its tray icon before taskbar-pager sync
  (`tiling_sys/run_loop_intake.rs:198-204,277`). Ordinal changes rebuild both
  normal/warning icons and replace the old pair (`ffi/tray.rs:730-765`), then
  call `Shell_NotifyIconW(NIM_MODIFY)` (`:107-125,169-175`). Same-output focus
  changes normally leave ordinal/warning/status unchanged and skip this work.
  Icons stay 32x32 (`:187-211`); ordinary update is MODIFY, not DELETE/ADD.
  This is independent of the resolved NVIDIA bell, and does not itself prove
  tray movement or pager occlusion. Ordinary MODIFY is not currently traced.

### 3. Departure-hide membership and cloaking

- `tiling_sys/world_native.rs:1434-1459` builds departures from exact
  `core_homing` members on this output but outside the destination, excludes
  sticky keys, and resolves each key's stored identity. Arrivals must have a
  matching hidden claim. There is no process-wide enumeration/hide in this
  construction; individual claims address individual HWNDs.
- Owner-process windows never become observed tiling candidates
  (`tiling_sys/observe.rs:861-871`). Hide admission independently refuses the
  owner PID, parented/owned windows, tool windows and no-activate windows
  (`product_hide/sys/mod.rs:1357-1359,1388-1402`), and binds exact process and
  member lifetime (`:1475-1511`). Pager creation has NULL parent/owner and
  TOOLWINDOW/TOPMOST/NOACTIVATE styles (`ffi/pager.rs:299-312`), so it also
  cannot disappear through an ownership relationship with a departing client.
- Production hide/reveal chooses claim-based visibility primitives
  (`tiling_sys/world_native.rs:1277-1292,1351-1363`), not cloak or minimize
  operations on the pager. Cloaking/ITaskbarList mutation belongs to the
  experimental test-helper path (`cloak.rs:2-8`; implementation and helper
  bindings in `ffi/cloak.rs` and `test_window.rs`). Ordinary observation reads
  the cloak fact (`tiling_sys/observe.rs:1061-1073`).
- This structural exclusion agrees with the earlier accepted fixture trace;
  it is not proof of uninterrupted DWM visibility in dogfood.

### 4. Indirect shell and foreground effects

- Current view order is departure hides, then arrival reveals
  (`tiling_sys/world_native.rs:1467-1499`); the view pass precedes one-shot
  focus (`tiling_sys/world_diff.rs:3118-3123,3461,3579-3599`). Matching arrivals
  use the nonactivating lane (`:3110-3122`), which posts per-HWND
  `SetWindowPos` with ASYNCWINDOWPOS/NOACTIVATE/NOZORDER plus HIDE/SHOW
  (`product_hide/sys/mod.rs:979-1017`). Other view changes use the pumped
  `ShowWindowAsync` lane (`:666-703`). Same-view focus does not need this
  departure/reveal batch; the explicit focus setter itself is common.
- Hiding/showing application windows can change Explorer's taskbar button
  presentation. There is no production `ITaskbarList::DeleteTab/AddTab` in
  this path. The pager is not an Explorer child: it is a separate topmost
  popup positioned over the taskbar rectangle, immediately left of the tray
  (`ffi/pager.rs:299-312`; `pager.rs:456-477`). A taskbar repaint underneath
  an unchanged, above-taskbar opaque pager is not enough to blank it: coverage
  would require relative z-order/occlusion changes, surface invalidation, or
  changed anchor geometry. Normal button reflow is not evidence of tray-edge
  movement. Fresh taskbar facts are re-read each pump
  (`tiling_sys/pager_host.rs:647-673`; `ffi/pager.rs:1111-1136`).
- Hiding the foreground permits fallback activation; the
  [SW_HIDE contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow)
  explicitly activates another window. Previously accepted source-event
  measurements found unmatched foreground events on empty departures, not
  causal attribution of those events. The roughly 80ms figure is first show
  post to matched target foreground callback including delivery, not measured
  pager blank duration (`windows-workspace-return-focus.md:1295-1300,1334-1355`).

### Ranked explanations and proposed remedy

1. **Shell/taskbar interaction during switching, cause still unproven.**
   Best switch-only match: client visibility/button churn, foreground fallback
   and the additional numbered-tray MODIFY. Against: fixed-size tray icon,
   separate opaque pager, and earlier post-effect samples above the taskbar;
   no transient occlusion or taskbar-order reversal was captured.
2. **Intermediate GDI drawing exposed by switch-time scheduling/composition.**
   Direct per-cell fills exist without an application backbuffer. Against:
   focus changes do the same full-strip repaint without reported flicker;
   there is no separate class-brush clear. Needs a switch-specific coupling.
3. **Conditional strip move/resize.** Count or tray-edge changes can move it.
   Against: not inherent to a switch, and the earlier larger tray shift is
   separately resolved. Native move requests alone do not prove changed bounds.

- Proposed rendering remedy, **only if capture shows incomplete cell drawing**:
  draw a complete opaque frame off-screen and copy it once in `WM_PAINT`;
  avoid requesting separate background erase. Retain the existing safe
  topmost/show behavior until occlusion is excluded. Changing only erase=FALSE
  is not a demonstrated fix with a NULL brush. Removing content-only
  `SetWindowPos` could reduce native work but also remove immediate topmost
  protection and worsen the first hypothesis.
- If capture instead shows the whole strip replaced by taskbar pixels, pursue
  taskbar-order evidence and a bounded, safety-gated event-driven z-order
  correction, not a paint-buffer fix. A tray-ordinal freeze A/B would isolate
  its shell call, but is a separate temporary behavior experiment, not tracing
  and not performed here. Do not remove the numbered icon on inference alone.
- Latency: no remedy should add a wait, retry or switch-settle deadline. A
  reused off-screen buffer adds a strip-sized copy to painting, not a deliberate
  arrival delay; its actual cost must be measured. No numeric speedup is
  established. Preserve and compare select-to-visible/select-to-foreground
  timing and command counts as well as blank-frame count.
- The staged show -> visible proof -> focus -> foreground proof -> departure
  hide design changes the foreground/visibility coupling and may change this
  artifact independently. It does not remove tray MODIFY, guarantee taskbar
  order, or solve no-target empty departures. Verify it separately rather than
  folding a pager remedy into the switch-order experiment.
- Exact next evidence: user-owned high-frame-rate recording (prefer 120/240fps
  where supported, otherwise a physical high-speed camera) with the whole
  taskbar, tray, pager and arriving/departing content visible, and the existing
  in-product trace enabled. Record identifiable slow fixed-count roundtrips
  and same-output focus changes as the control. Inspect every missing frame:
  progressive background-only cells support drawing; underlying taskbar with unchanged strip
  suggests coverage/surface loss; changed x/width plus logged geometry suggests
  reanchoring. Join native events and world rows by request/revision, retaining
  tickless gate rows. Existing pager events have order, not per-event clocks
  (`ffi/pager_trace.rs:46-74`); batch context cannot measure erase/paint latency
  or uniquely identify an occluder. A recording can narrow these branches, not
  by itself prove tray MODIFY caused them. No additional diagnostic or product
  code was added in this continuation.

- Source paths above are relative to `crates/tiler-windows/src/` unless another
  crate or document is named.

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

- User-accepted 2026-10-28: retain at most 128 ordered native pager
  events per drain with explicit overflow counts and opaque window-lifecycle
  tokens. Trace-disabled runs capture nothing. Existing pager-cell caps stay
  unchanged; unchanged presentations still report native activity.
- User-accepted 2026-10-28: tick/request/observation/revision identify
  drain context for correlation, not causal attribution of queued OS messages.
  Sequence records observed order; pre-registration lifecycle is unknown.
