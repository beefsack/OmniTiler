# D23 minimize + restore (MNZ-01/02/03)

Goal: deliver user-approved D23 option (a), no setting: MNZ-01 minimize releases
allocation with sibling reflow, MRU focus and stored old slot; MNZ-02 native
restore returns to stored slot with no OmniTiler focus write; MNZ-03 sole-window
minimize retains workspace, focus none, stored slot.

Scope:
- Shared core first (serves Windows): stored restore slots, minimize
  release/reflow/MRU, no-focus restore, sole null focus, invalidation fallback.
- KDE adapter: observation/restore wiring including sole-minimize null focus.
- Windows: compile-only changes only; exact adapter work goes to backlog
  handoff item 22 as a new entry.
- Records: spec/matrix Ours cells, backlog D23 row, this note (archive on
  completion).

Non-goals: settings, Windows behavior change, macOS, live testing
(user-owned, list pending).

Acceptance:
- Red/green regressions (shared core + KDE production path) for all three legs
  plus the invalidation fallback.
- Full gates: cargo test --workspace, clippy --workspace --all-targets -- -D
  warnings, cargo fmt --all -- --check, KWin JS tests, typecheck and bundle
  build; CTest only if native code changed.
- One independent read-only diff review.
- Live checks listed pending, user-owned.

Approach: shared core first, then KDE, then Windows compile-only + handoff
entry, then records/gates/review.

Tentative decisions (pending user review, recorded here):
- Old slot representation: session-runtime stored slot bound to window identity
  plus domain; invalid when sibling closed, tree restructured, window sent
  elsewhere, or workspace removed; fallback is fresh ordinary admission per
  existing rules.
- Restart: stored slot is session-runtime only, not persisted.

Units:
1. Shared core minimize/restore + regressions.
2. KDE adapter wiring + regressions.
3. Windows compile-only + handoff item 22 entry.
4. Records (spec/matrix/backlog) + full gates.
5. Independent read-only review.

Verification: unit 1-2 red/green regressions; unit 4 full gates; unit 5 review.
Live checks pending (user-owned): native MNZ-01 release/frames/focus, MNZ-02
slot/focus, MNZ-03 occupancy/focus, WS1->WS2->WS1 style native journeys.

Outcome (delivered offline 2026-10-12):
- Shared core: session-runtime MinimizedSlot storage bound to window identity
  plus domain, minimize release/reflow/MRU, no-focus restore, sole null focus
  and invalidation fallback (fresh ordinary admission per existing rules).
- Engine: retire fix so sole-minimize sessions with live slots are retained,
  not retired, through reconcile.
- KDE adapter: omission/restore observation wiring including sole-minimize
  null focus.
- Windows: compile-only; exact adapter work stays in backlog handoff item 22.

Evidence:
- Red/green regressions: core minimize_restore 5 tests
  (`crates/tiler-core/tests/minimize_restore.rs`: MNZ-01 release/MRU/slot,
  MNZ-02 old-slot restore with no focus write, MNZ-03 sole retain/focus-none/
  slot plus engine-reconcile leg, sibling-close invalidation fallback) and
  kwin minimize-restore-entry 3 tests
  (`kwin/tests/minimize-restore-entry.test.ts`: production entry plus real
  Planner MNZ-01/MNZ-02 leg, MNZ-03 sole leg, invalidation fallback leg).
- Full gates per unit 4 (exact commands/results recorded by the unit runner).

Tentative decisions (pending user review, recorded here):
- Old slot representation: session-runtime stored slot bound to window identity
  plus domain; invalid when sibling closed, tree restructured, window sent
  elsewhere, or workspace removed; fallback is fresh ordinary admission per
  existing rules.
- Restart: stored slot is session-runtime only, not persisted.
- Known risks: close-all retains slot-only sessions until restart; sole-restore
  operation reports Rejected focus-mismatch while converged state persists.

Pending live checks (user-owned): native MNZ-01 release/frames/focus, MNZ-02
slot/focus, MNZ-03 occupancy/focus, WS1->WS2->WS1 style native journeys.

Independent review: accept with non-blocking followups (read-only diff
review, no re-execution; gates evidence consistent):
- Prune lingering close-slots so close-all does not retain slot-only
  sessions until restart.
- Reconcile focus-binding ergonomics for focus-none sole restores that
  report focus-mismatch on correct state.
- Optional corner test: unfloat arriving with a stale minimize slot.
