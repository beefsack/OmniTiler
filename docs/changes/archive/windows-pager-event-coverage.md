# Windows visible-pager native-event coverage

## Goal and acceptance

- Test whether native events completely cover visible-pager topmost/visibility loss before replacing the existing 2 s recovery timer.
- Use controlled disposable windows, record actual events and independent z-order/visibility observations, and distinguish emitted events from events accepted by current filters.
- A demonstrated gap or unprovable completeness is an acceptable negative outcome: retain the timer and record uncovered cases. No weaker recovery contract or replacement polling.
- Keep Windows unsafe code within `crates/tiler-windows/src/ffi/`; no shared-core or other-platform changes.
- Any timer replacement must also preserve fail-closed pager suppression, creation backoff and the approved icon freshness currently serviced on visible-pager turns. No replacement is accepted solely because an event is emitted.

## Approach and bounded units

1. Inspect pager recovery, native routing and existing helpers; observe raw WinEvents separately from production filtering.
2. Exercise an owned topmost-cover discriminator and lifecycle/taskbar controls; compare independent z-order observations with emitted events.
3. Review source and raw evidence independently; retain recovery if completeness remains unproven, without expanding the experiment to chase a replacement.
4. Run the locked Windows gates and record restoration and evidence limits.

## Live scope and restoration

- Physical Windows desktop, disposable identity-tagged helpers and `tile --user-start --scope-exe tiler-test-window.exe`. No keyboard injection, terminal management, registry/policy writes or physical display mutations.
- Taskbar auto-hide was temporarily enabled and its captured `ABM_GETSTATE` value restored exactly. Raw machine/resource identifiers and helper receipts stay outside repository records.

## Outcome

- Completed 2026-10-18 with a bounded negative outcome: complete native coverage remains **unproven**, not disproven. Retain the visible-pager 2 s recovery timer unchanged. No production change, weaker recovery contract, replacement poll or event-first workaround.
- The tested burial did emit a desktop-parent `EVENT_OBJECT_REORDER`; it was not OS silence. Current subscriptions do not receive that event. Observing it once does not establish completeness, self-feedback safety or coverage of every visibility/topmost-loss mode.
- Independent source/raw-evidence review accepted this limited outcome after correcting initial event-silence, recovery-attribution and cleanup claims.

## Live evidence

- Source `9722cb36c5164c1437a1524e4e8c8846bacfbd8c`, documentation-only working changes; debug payload. Owner SHA-256 `c09d5917e7023ee2ebf294b37b193a877b6b7eceb8f5d52dfe14d1a5abb27054`; helper `42d0440c3f28b0a725ff34b0206f954f9a0dcc5dcafdc6263012b12ae8b89887`. Exact launch/process identity is retained in external provenance.
- External observer source uses an out-of-context hook on its own pumping thread, logging event/object/child/window/thread and timestamps. The reported invocation range was `1..0x800F`; the invocation transcript expired, so that range is recorded execution information, not independently retained call-site proof. Observed event kinds and positive lifecycle controls confirm a live broad observer, not universal coverage.

| Case | Retained observation | Meaning / limit |
| --- | --- | --- |
| Already-shown floated helper promoted over pager | Requested `SetWindowPos` with `HWND_TOPMOST` and `NOMOVE`, `NOSIZE`, `NOACTIVATE`; independent z-order chain showed pager below cover in six samples, then above in the seventh; `IsWindowVisible` stayed true | Z-order burial, not a visibility-loss test. No foreground event occurred in the isolated interval; independent foreground/geometry pre/post responses were not retained |
| Raw events during burial/recovery interval | Exactly two events through the first recovered sample: desktop-parent `REORDER`, object `-4` (`OBJID_CLIENT`), first at the recorded setter timestamp, second 613 ms later; no pager/helper-window event | Raw parent notification exists; current hook ranges omit reorder. Second event is consistent with self-generated recovery output, not proof of its wake source |
| Recovery timing | First below sample +21 ms after recorded setter; still below +612 ms; first above +722 ms | One sampled observation, not a latency distribution. Owner log has nearby icon completion; timeout versus coincident completion wake cannot be separated. The source still gates topmost reassert on the 2 s cadence |
| Lifecycle controls | Helper create/show/foreground, minimize/restore location and minimize events, close hide/destroy observed | Confirms those event kinds in the run. Deliberate foreground-D request had no matching event/preimage and is void as an independent focus control |
| Taskbar auto-hide on/off | Old pager hide/destroy after enabling; recreated pager create/show after disabling; original taskbar state restored | Continuous owner log records auto-hide refusal and recreation. `strip_for_taskbar` refusal yields no served output, so `sync` drops the old overlay; destruction does not imply owner restart. No precise restore latency claimed |

## Coverage interpretation

- `tiling_sys/run_loop.rs` subscribes to foreground, move/size, create/destroy/show/hide and location, not reorder. The observed desktop-parent burial form therefore has no callback-driven owner turn today.
- `tiling_sys/observe.rs` accepts window-object lifecycle/location/foreground edges (`id_object == 0`), not the observed parent/client form. Hooks do not skip the owning process; renderer-carrier filtering does not exclude pager HWNDs. Raw emission, accepted hook edges and a full owner turn are different facts.
- A subscribed callback rejected by the edge filter can still cause a full owner turn through message dispatch (`observe.rs::pump_wait_with_intent`, `run_loop.rs` pre-drain/full-turn path). Merely widening subscription is not evaluated here; do not describe the object filter as independently preventing all message wakes.
- `pager_sys.rs::PagerOverlays::sync` checks visibility on each safe turn, but unchanged frames reassert topmost only when the 2 s cadence is due. An early native wake is not immediate z-order recovery. `ffi/pager.rs` handles shell/settings/display/DPI separately; it has no explicit `WM_WINDOWPOSCHANGED` recovery handler.
- Microsoft [event constants](https://learn.microsoft.com/en-us/windows/win32/winauto/event-constants) describe foreground changes and parent-reported child reordering, not a complete pager-occlusion contract, and recommend verifying event use by UI elements. [Out-of-context delivery](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook) is asynchronous on the installing message-loop thread. [WM_WINDOWPOSCHANGED](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-windowposchanged) describes the recipient's own changes, not every foreign occlusion. Documentation rechecked 2026-10-18.

## Gaps and restoration limits

- Own-pager `WM_WINDOWPOSCHANGED` instrumentation, arbitrary pager visibility loss, Explorer restart/`TaskbarCreated`, physical display/DPI changes and the remaining foreign topmost reorder matrix were not exercised. No event-completeness or feedback-free recovery guarantee follows from this one burial. Physical input/game/elevation/secure-desktop acceptance is separate.
- Final retained read-only snapshot: no owner/helper actors or pager window, no ledger/stop/workspace request residue, taskbar state matched its preimage. Owner log ends with clean input/render shutdown, Mouse Snap restoration and `run-end stopped`; final `mise exec -- just stop` output says `stopped clean`.
- Correct final `SPI_GETWINARRANGING` (`0x0082`) read succeeded and was true. Initial helper mistakenly queried pen visualization (`0x201E`); those stable raw values are not Mouse Snap evidence. No independent correct SPI preimage exists, so exact independent pre/post comparison is unavailable. Restoration rests on the product's disabled/restored log path and correct final read, not the mistaken getter.
- Verbatim helper close/kill/restore preflight console responses expired; helper receipts and final absence remain. Do not promote reported preflight results into independently retained proof. Mono-to-wall anchors are approximate; no sub-100 ms causal timing conclusion is made.
- Evidence set `pager-events-20261018`: `events.jsonl`, `poll-cover.jsonl`, `markers.jsonl`, `preflight.json`, helper receipts, `pev.cs`, `owner-provenance.json`, copied owner log, corrected final snapshot/script, stop output and `corrections-manifest.md`. The manifest distinguishes reconstructed execution inventory from retained responses.

## Verification and disposition

- Locked four-package build/tests through `mise exec --`: 2,750 passed, 0 failed, 1 ignored. Strict all-target clippy, workspace formatting and whitespace checks passed; full gates rerun from PowerShell Core with durable external outputs (`pager-event-gates-20261018/ps7`). Rust/Cargo 1.99.0, MSVC target. No Rust or shared-core changes; hosted CI is separate.
- The conditional pre-0.1 investigation is complete; timer replacement did not meet its acceptance condition. Existing icon refresh, suppression and retry behavior remain unchanged. No further implementation action is selected.
