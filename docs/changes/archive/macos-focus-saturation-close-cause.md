# macOS focus-saturation: distinct close cause

## Goal and scope

Restore `crates/tiler-macos/tests/integration/runtime_phase1.rs::focus_saturation_drops_counted_and_survives`
as a passing capacity-fence regression without weakening its assertions or
changing `tiler-core`/`tiler-protocol`.

- In scope: the one integration test and its local fixture helpers.
- Out of scope: production adapter behavior, core focus policy, backlog edits.
- Non-negotiable: keep `focus_outbox_depth: 1`; prove one dispatch, one
  `focus_dropped_full`, a drained dispatched/unverified row, and runtime
  survival.

## Why the old shape cannot pass

With one app session, a second explicit focus step while the first request is
pending either resolves from the pending target or re-emits the same request
id; the adapter dedups it (`kind=close-focus outcome=duplicate`). A single
session can hold at most one live outbox row, so the depth-1 fence is
unreachable that way.

## Approach

Use two live apps in one workspace:

1. Admit a surviving app with an unfocused window.
2. Admit the focused app with its focused window and attest its owner-thread
   focus pair.
3. Gate the explicit focus dispatch so its outbox row stays queued.
4. Submit an empty, genuinely windowless scoped observation for the focused
   app (frontmost and selected oracle still name that app): core closes its
   windows and mints a distinct `FocusRequestCause::PositiveClose` survivor
   request (`focus-{issuing_revision}`) targeting the surviving app.
5. The adapter reaches the strict outbox capacity fence and drops the second
   request counted; the gated first dispatch remains the sole queued row.
6. Release the gate, drain the explicit row, and re-observe the focused app to
   prove the runtime still actuates.

Windowless proof needs `focused_wid: None`, `ax_focus: Some((pid, None))`,
`frontmost_pid: Some(pid)`, and no owned windows; the local `obs` helper's
`focused_wid: None` means "unfocused, unknown", which is not enough.

## Outcome and evidence

Resolved 2026-11-24. The stop was a test-shape limit, not a core allowance: a
second live app lets the focused app's proven-windowless close mint a distinct
`PositiveClose` request, which reaches the same depth-1 outbox fence.

- [x] Targeted test passes: `cargo test -p tiler-macos --test integration
      runtime_phase1::focus_saturation_drops_counted_and_survives` (5/5;
      repeated run green).
- [x] `focus_dispatched == 1`, `focus_dropped_full == 1`, one drained
      `dispatched && !verified` request, survival actuation; the explicit step
      is `core_replies["focus-accepted"] == 1`.
- [x] Ring trace: `id=focus-4 cause=explicit-command target=1001_12 ...
      outcome=dispatched-native`; `id=focus-5 cause=positive-close
      target=1002_21 ... outcome=no-call-outbox-full-superseded`; exactly one
      `close-focus no-call` (pid 1001), no `close-focus duplicate`.
- [x] `cargo fmt --all -- --check` exit 0; strict clippy for `tiler-macos`
      `--all-targets` exit 0; full integration target 150 passed / 0 failed.
- [x] Only the integration test changed under `crates/tiler-macos/tests/`;
      no core, protocol, or production adapter change. Worktree left
      uncommitted with journal entry.

## Decisions

- Removed the second explicit focus step: it is race-dependent (dedups or
  resolves a distinct target and spends the fence itself), so the close is the
  only second publication.
- Replaced `focus_replies == 2` with `core_replies["focus-accepted"] == 1` plus
  one `close-focus no-call` and no `close-focus duplicate`. The close publishes
  on the reconcile reply lane, so the old token-family count cannot represent
  it; the replacement proves the same two publications and their distinct
  paths without weakening the capacity assertions.
- The survivor app is observed focused before focus returns to the closing
  app: this spends its admission intent (`already-satisfied`) so no third
  publication competes for the single slot, and leaves its leaf in the same
  domain as the close cohort.
- Both owner backends attest the one established selected-oracle pair
  `(APP_A, Some(11))`, since both workers read the same global oracle.

## Backlog handover

Remove `docs/backlog.md` P1 "Core focus-saturation allowance" and drop the
"1 core-policy stop" clause from the v4 phase 3 status line.

## Stop condition

If the close supersedes the pending explicit request and no concurrent second
request reaches the fence, stop and report the trace rather than forcing the
assertion or changing core.
