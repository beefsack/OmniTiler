# Windows tray-menu graceful stop

Status: delivered with scoped native stop verification; archived 2026-10-14.

## Goal and acceptance

- Graceful stop cancels an open owned tray menu and reaches normal teardown
  within the existing stop timeout, including nested menus.
- Preserve the native outside-click dismissal pattern: foreground the owner
  before tracking, then post `WM_NULL` after tracking. Physical outside-click
  acceptance remains separate.
- Preserve settings, unrelated windows, scoped membership and the injected
  keyboard filter. Do not infer physical shortcut acceptance from native tests.

## Scope and approach

- Reproduce the stop starvation already observed with the production tray
  callback; inspect the stop-aware first-run dialog as a related native pattern.
- Add the smallest menu-thread cancellation mechanism and targeted regression
  coverage. Review native ownership and teardown boundaries; run native gates.
- Live-verify an open menu followed by graceful stop, then continue bounded
  underlay, tray-state, float/combo and minimize/close verification as feasible.
- Tentative, pending user review: use standard menu-thread cancellation and
  the documented foreground/`WM_NULL` popup lifecycle. Use an agent-owned
  disposable test helper for scoped rendering and workspace evidence.

## Evidence and current outcome

- A production tray popup prevented the owner from draining a graceful stop
  request. Posted and synchronous popup `WM_CANCELMODE` attempts did not
  dismiss it. Posting Esc down/up to the verified owned popup did dismiss it;
  the pending stop then exited the owner. No forced termination was used.
- `just stop` subsequently verified independent restoration: no owner, popup,
  tray, ledger/request or payload-pointer residue; Mouse Snap matched its
  preimage and saved settings were byte-identical. All helper fixtures were
  closed. Raw identity, message and restoration evidence stays outside the repo.
- The owner supplies its existing exact-creation stop-file predicate to the
  tray. A 50 ms window timer polls it inside modal tracking and calls `EndMenu`
  on the menu's thread. Stop wins racing picks, suppressing settings launches,
  workspace toggles and persisted default changes.
- An active-track guard refuses callback reentry and prevents queued stale
  timers from cancelling a later menu. Timer-install failure destroys the owned
  menu without tracking and emits a bounded diagnostic. Foreground-before-track
  and post-track `WM_NULL` were already present and are preserved.
- Review corrected silent timer-install failure and the missing active-track
  guard before acceptance. Five new regressions exercise production entry,
  timer-install, cancellation and post-pick decision fences. The related eight
  first-run stop tests also pass; no first-run publication behavior changed.
- Initial no-menu fixture runs sent mouse-down rather than mouse-up callback
  constants. Those pre-effect results are withdrawn. Corrected production
  callbacks opened verified owned visible popups before timed stop.
- Fixed-build native acceptance: `STOP_MS=411` and `STOP_MS=86`, both below the
  normal five-second stop timeout. Owners exited and popup/tray windows were
  destroyed without manual dismissal. A repeated callback during tracking
  retained exactly the same single popup. Independent review accepted these
  action/readback gates and fixed-build provenance.
- Each acceptance ended with `just stop`, clean ledger/request/pointer readbacks,
  restored Mouse Snap and byte-identical saved settings. Final closeout after
  subsequent journeys also found no project owner, tray, popup or test helper.
- Final native gates: Rust 1.99.0, MSVC target; locked four-package build/test,
  workspace format, strict all-target clippy and whitespace checks pass:
  **1,852 passed, zero failed, one ignored** (+5 over the verification baseline).
- Physical keyboard/outside-click acceptance is not inferred. Separate native
  underlay/toggle/combo/slot-cleanup gaps remain in their feature records.
