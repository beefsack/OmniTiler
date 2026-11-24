# macOS v4 follow-up fixes (F1–F5)

## Goal

Resolve the five follow-up findings from
`macos-v4-open-decisions-review.md#part-5-follow-up-code-review-and-tentative-decisions-2026-11-24`
in the existing uncommitted D4/R1/R2 worktree, with focused offline regressions,
before the D4 native restart proof or committing the worktree.

Acceptance: F1–F5 semantics below covered by targeted tests; `tiler-core` tests,
`tiler-macos` lib/integration tests, `cargo fmt --all -- --check`, and strict
clippy all clean. No live KWin/macOS/Windows testing in this unit. No commit/push.

## Scope

- **F1 per-window intent hydration.** Replace the session-wide admission latch
  with a per-window admitted set matching core's `intent_admitted.contains(win)`.
  A late report that first reveals App B imports B's stored intent; admitted
  windows never re-import. Mark exactly the intent records shipped in the applied
  report.
- **F2 alive-unverifiable binding.** `None` from the live binding probe no longer
  freezes the whole reply when the process is alive but has no verifiable
  LaunchServices lifetime. Emit one bounded diagnostic per window, skip that
  row's persistence, and continue. Gone processes keep the recoverable hold.
  Keep the store opaque and length-bounded (drop adapter-side core decoding).
- **F3 observer eager work.** The observer loop must not read session/Secure
  Input, pointer position, the ack mutex, or hit-test when focus-follows-cursor
  is off or the game pause is set; a disabled setting must not wake at 4 Hz.
- **F4 Mission Control ownership.** Key the CG presentation check on the surface
  that actually owned the open Mission Control AX tree (Dock on macOS 15, not
  merely WindowManager running). WindowManager AX unavailability falls through
  to Dock; exact bundle/mismatched/protected refusals still refuse.
- **F5 test-store isolation.** Explicit `RuntimeConfig.intent_store` mode
  (`Disabled` default, `Default`, `Isolated(path)`); remove `cfg!(test)` open.
  Production CLI requests `Default`; tests/config literals stay disabled. Reuse
  `desktop_shortcuts::recovery_dir()` for the default directory.

## Non-goals

- R3 typed ingress, D5 production transport, M4, and the D4 native restart proof.
- Any behavior not named above; existing D3=B gates and R1/R2/D4/D5 evidence.
- Growing `crates/tiler-macos/src/fixture_leg.rs` or the real user intent store.

## Bounded units

1. Intent unit: F1, F2, F5 across `world_runtime.rs`, `world_dispatch.rs`,
   `intent_store.rs`, `runtime_types.rs`, `runtime_thread.rs`, `tile.rs`, plus
   the isolated-start integration test.
2. Observer unit: F3 in `cursor_observer.rs` (and its wiring/tests).
3. Mission Control unit: F4 in `mc_lifecycle.rs` / `mc_probe.rs` / tests.

Units 2 and 3 are disjoint from unit 1 and from each other.

## Verification

`cargo test -p tiler-core`; `cargo test -p tiler-macos --lib`;
`cargo test -p tiler-macos --test integration`; `cargo fmt --all -- --check`;
`cargo clippy -p tiler-core -p tiler-macos --all-targets -- -D warnings`.

## Outcome (2026-11-24)

F1–F5 implemented in the existing D4/R1/R2 worktree; no commit or push.

- **F1:** `WorldSession` keeps a per-window `intent_admitted` set; every dispatch
  site marks exactly the applied report's `intent_records`. Regressions cover a
  late report importing only the newly observed app and a shipped window never
  re-importing.
- **F2:** an alive PID with no verifiable LaunchServices lifetime is skipped
  without holding the reply and diagnoses once per window; a gone PID keeps the
  recoverable hold. The store no longer decodes `data`.
- **F3:** the observer reads only the two atomic gates first; disabled and paused
  cycles perform no session, Secure-Input, position, ack, hit-test, or post work.
  Disabled idles at 1 s.
- **F4:** CG presentation is keyed on the last exact-verified owning surface;
  WindowManager non-ownership AX failures fall through to Dock; exact bundle and
  ownership refusals never fall back.
- **F5:** `RuntimeConfig.intent_store` is `Disabled` by default with explicit
  `Default`/`Isolated(path)`; the production CLI selects `Default`; the engine
  opens once and publishes the directory.

Offline evidence: `cargo test -p tiler-core` clean; macOS lib 1687 passed /
0 failed / 3 ignored; integration 151 passed / 0 failed; fmt clean; strict
clippy `-D warnings` clean for `tiler-core` and `tiler-macos` all targets.

Live gates not run (user-paused): real lock/Secure-Input/fullscreen observer
transitions, macOS 15 Mission Control surface, and the D4 native restart/failure
proof.
