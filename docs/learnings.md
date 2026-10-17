# Learnings

Project-specific lessons from delivered work. Each entry states the lesson,
the evidence and how to apply it.

## Host-shaped test data (2026-10-17)

- Lesson: offline suites can pass while live KDE rejects everything, because
  mocks use idealized values instead of what KWin actually produces.
- Evidence: the full-state replies passed thousands of offline tests, then
  failed live three ways: Qt size hints with zero minimums and unbounded
  (INT_MAX) maximums were rejected as invalid snapshots (`3c94a819`); QML
  array-like lists, braced window IDs and removed-window objects broke
  actuation and close tracking (`9eaea71a`). Mocks used JavaScript arrays,
  stable references, bare IDs and bounded positive hints.
- Apply: fixtures for any host boundary use producer-shaped data (Qt
  sentinels, QML lists, braced IDs, dead objects on removal). Treat a
  boundary rewrite as unproven until a live smoke of ordinary actions
  (open/close, focus, move, resize, float toggle, workspace switch) passes.

## Replacing a route drops side channels (2026-10-17)

- Lesson: when a new boundary replaces per-operation calls, settings and
  state that previously reached the Engine through separate calls are easily
  lost.
- Evidence: after the full-state route, outer gaps were carried but not
  inset, and gap changes used reconcile instead of `UpdateGaps`; an audit
  also found dropped same-axis move selection, resize repeat indexing,
  born-maximized admission, migration source refill, shortcut profile and
  workspace-mode handling (`9fe0e79b`).
- Apply: before switching routes, inventory every input the old route fed
  the core (settings, update calls, startup-only values) and add a
  regression per input through the new route.

## Dev-loop freshness (2026-10-17)

- Lesson: `just reload` rebuilds and restarts the Planner only; KWin script
  changes need `just dev-off` then `just dev` or `just dev-on`.
- Apply: before diagnosing a live log, confirm the running Planner binary and
  loaded script include the fix under test; give users the full restart
  steps when a change touches `kwin/src`.

## Space exhaustion and Tier 1 parity (2026-10-17)

- Lesson: Tier 1 tilers never float, refuse or search for another target
  when the anchor is too small; they keep splitting into degenerate tiles,
  admit serially and (COSMIC) retile from scratch on enable. Our clean-fit
  startup adoption and best-effort fallback are deliberate host-limitation
  deviations, because we cannot squeeze client windows.
- Evidence: [R-MIN-04, R-INS-09](spec/reference-outcomes.md);
  [best-effort admission](changes/best-effort-admission.md).
- Apply: ground admission and layout policy questions in Tier 1 source rows
  before proposing behavior; record deviations as host-limitation choices.
