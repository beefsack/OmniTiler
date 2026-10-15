# Windows admission trailing workspace

Windows owner correction delivered offline, 2026-10-16. Native launch and
pager acceptance remains pending.

## Goal and scope

- Filling the final empty workspace by fresh Windows admission immediately
  appends a new empty workspace, matching the existing shared-core rule and
  KDE lifecycle. Both pagers must reflect the resulting owner ring.
- Cover normal, born-fullscreen and slotless-maximized admission, existing
  send/move, restore/unminimize and focus-follow routes.
- Do not change shared-core, KDE or macOS behavior, pager presentation, or
  unrelated window handling.

## Acceptance and approach

- Run the existing Windows scoped trailing convergence after successful
  admission, including quiet maintenance and active reconciliation paths.
- Repeated observations must not grow the ring; non-trailing admission must
  not append unnecessarily. Preserve selection and history.
- Add regression coverage for admission, supported workspace modes and both
  pager projections. Verify adjacent entry routes retain the same invariant.
- Root cause: `ensure_workspace_assignments` establishes membership but does
  not invoke trailing convergence. Selection, send and close paths already do.
  Both pagers render owner workspace IDs and cannot mint the missing spare.

## Bounded units

1. Investigate owner admission, shared policy, KDE and pager projections.
   Complete: the missing spare is an owner lifecycle gap, not a refresh issue.
2. Implement Windows-only convergence and targeted regression tests.
3. Review the diff and run native locked build/test, workspace format, strict
   all-target clippy and diff checks.

## Verification and outcome

- `ensure_workspace_assignments` collects outputs only on successful normal,
  born-fullscreen and slotless-maximized membership assignment. After restart
  marker adoption, it converges each affected output (the shared logical ring
  once), so sticky adoption is reflected before occupancy is evaluated.
- Both active reconciliation and quiet maintenance use this admission path.
  Existing send/move and focus-follow tails already converge; minimized members
  retain their workspace, and restore does not re-admit them elsewhere.
- The reported non-trace sequence corroborates the owner gap: admission filled
  the final workspace, the next numbered selection refused an unknown target,
  and only a later explicit trailing selection created the missing workspace.
- No shared-core or other-platform source changed. Local and scoped modes
  reuse their existing Windows adapters to the shared trailing policy.
- Eleven added tests cover normal and retained admission shapes, all three
  workspace modes, history/active-view retention, no-admission and non-trailing
  no-ops, idempotency, chained fills, sticky exclusion, repeated observation,
  and taskbar/tray projection parity. Native admission fences have no offline
  fixture seam: successful-admission output collection is code-reviewed;
  fixtures exercise post-assignment convergence, not native launch delivery.
- Independent review accepted production wiring and found fixture weaknesses
  (inactive-tail staging, identity mismatch and incomplete born-hold shape).
  Corrected fixtures activate the filled tail, preserve exact member identity,
  and distinguish born-hold snapshot seeds from slotless-maximized members.
- Locked four-package build/test and strict all-target clippy, workspace format
  and diff checks passed: 2,145 passed, zero failed, one pre-existing ignored
  (11 more passed than the baseline). Existing send/move, minimize/restore and
  focus-follow suites pass with the correction.
- Pending native check: in an isolated Notepad-scoped run, keep A on workspace
  1, select empty workspace 2, launch B, and verify workspace 3 appears empty
  in both pagers while B remains active on workspace 2. Repeat from workspace
  3 to verify workspace 4 appears; send/move a window into a final empty,
  minimize/restore it and focus-follow it, checking the spare and both pager
  sets after each step. Close only test windows and restore the initial state.
  Physical launcher-shortcut delivery is a separate user-run leg if required;
  this correction is not specific to browser or launcher shortcuts.
