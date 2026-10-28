# Floating and sticky workspace return focus

## Goal and accepted behavior (2026-10-28)

- Core owns the last active window per workspace, including ordinary floating
  and sticky windows. Restore the eligible remembered target without a
  float-versus-tile preference, then retain the existing tiled fallback order.
- Record accepted command choices before native effects. Exception focus uses
  the same observation-ingestion gates as tiled focus; this change does not
  resolve foreground-origin attribution. Unattributed foreground can still
  replace intended memory, including after an unfinished arrival. The separate
  unknown-foreground decision must eventually address both window classes.
- Remember sticky focus only in the workspace where it was intentionally
  selected or observed focused, not every workspace on which it is visible.
- Preserve valid no-focus states, no forced host-loss recovery, empty-workspace
  selection without a focused client, and lifetime/safety/at-most-once gates.

## Scope and approach

- Keep one canonical lifetime-bound active-window identity per workspace in
  the core, alongside tiled topology memory; no fake tree links or weakened
  pending-plan validation.
- Extend accepted-command recording and observation convergence consistently,
  prune dead identities, and preserve the existing tiled fallback order.
- Windows: narrowly permit a freshly proven sticky focus target whose actual
  home differs from the selected visible workspace. Keep native homing proof
  authoritative and all competing-foreground and scope protections intact.
- Shared-core return behavior changes for every adapter; document the port
  impact without changing KDE or macOS adapters.

## Acceptance and verification

- Failing-first coverage: mouse/observation-focused float, keyboard-focused
  float, floating-only workspace, sticky home different from shown workspace,
  departure before focus lands, close/lifetime/reuse cleanup, and no-focus/empty
  preservation.
- Review core memory and Windows sticky dispatch safety independently.
- Gates: locked tests and strict all-targets clippy for tiler-core,
  tiler-protocol and tiler-windows; workspace formatting and diff whitespace.
- Native foreground confirmation is separate from offline semantic evidence;
  physical acceptance remains pending unless explicitly evidenced here.

## Outcome

- Failing-first core evidence: the linked-focus module passed its 24 original
  tests and failed all four new float/sticky return scenarios before production
  changes.
- Independent core review reproduced newer sticky losing to tiled history
  (at home and away), and unchanged tile observation overwriting an unlanded
  floating choice. Per-session identity maps and ownership arbitration were
  rejected; the replacement has one canonical workspace slot.
- Review also found focus-producing operation, send, admission and cursor routes
  missing from canonical recording. One ordered publication recorder now covers
  every accepted Focus effect before native dispatch, without rewriting Session
  leaf topology. A later admitted tile supersedes earlier leaf memory even when
  native focus has not landed. Re-review accepted the correction.
- Already-known sticky visibility on a different workspace no longer counts as
  a new admission or writes focus memory there. A failing-first multi-report
  regression covers the formerly spurious admission effect; ordinary and genuine
  new-sticky admission behavior remains unchanged. Re-review accepted the fix.
- Core coverage adds 17 linked-focus regressions (41 total in that module):
  observed and keyboard float returns, float-only workspaces, sticky home and
  away, unlanded choices, later tiled focus/operation/send/admission precedence,
  close/prune, and no-focus/empty sticky visibility. Old ineligible-sticky
  fixtures now use unexplained fit-excluded rows, preserving fallback coverage.
- Windows adds 11 sticky dispatch regressions: focus once with distinct home
  and shown scopes; reject ordinary mismatch, uncontrolled/unknown/wrong-output
  scope, recycled key or rotated tag, hidden/iconic/dead target, and competing
  foreground. Failing-first positive and foreground-guard cases were reproduced.
  Independent safety review accepted truthful homing, dual-scope authority,
  fresh lifetime/visibility proof and shown-view foreground-guard routing.
- Native testing was not run: delivery is offline semantic and safety evidence,
  not physical foreground acceptance. Physical acceptance remains pending.
- Unattributed foreground transitions and fail-closed recording skips remain
  accepted residuals; no attribution, retries, forced focus recovery, KDE or
  macOS adapter changes were added.
- Final verification passed:
  - `mise exec -- cargo test --locked -p tiler-core -p tiler-protocol -p tiler-windows --quiet`
    (core library 989 passed; Windows library 1207 passed, one existing ignored;
    protocol, integration and doc-test suites passed).
  - `mise exec -- cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-windows --all-targets -- -D warnings`
  - `mise exec -- cargo fmt --all -- --check`
  - `git diff --check`
