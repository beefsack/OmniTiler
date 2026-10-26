# Directional focus without a focused window

## Goal and scope

Implement the User 2026-10-26 workspace decision in the shared core: directional
focus without a focused window starts at the current workspace/output, crosses
to the adjacent output using existing selection, or does nothing. Never select
a current-workspace window. Preserve host-caused focus loss without automatic
refocus. Windows routing remains a thin host-fact/input adapter.

Non-goals: change cross-output target selection, decide empty-workspace arrival
across outputs, change floating/fullscreen/game eligibility, or add platform
checks to core.

## Acceptance and bounded units

1. Shared-core implementation and regression/acceptance tests for occupied and
   empty no-focus origins, adjacent output selection and no-neighbor no-op;
   prove observation-only host focus loss emits no forced-focus effect.
2. Inspect Windows routing and repair only if needed; fixture-only live proof
   when practical under the Windows testing protocol.
3. Record Tier 1 per-WM matrix cells from the existing discriminator and pinned
   sibling sources. COSMIC gets only a surface check; inconclusive stays TBD.
4. Run native Windows package build/test/fmt/strict clippy per the development
   runbooks; report other-platform gates separately. Update durable decisions,
   specification and backlog, then archive this record with accepted evidence.

## Verification

- Targeted shared-core tests and platform-neutral world acceptance sequences.
- Native Windows allowlist build/test/clippy and workspace formatting.
- Independent review of changed focus/effect contract and safety gates.
- Fixture live result, or an explicit practical blocker/limit.

## Decisions and outcome

- Product behavior is already user-approved. No new policy selected.
- Implemented in `world_operations.rs`; an absent subject never runs local
  focus. It uses the existing neutral action-output resolver, strict topology,
  pair resolution and remembered tiled destination selection. All four
  directions apply the approved no-focus rule; the focused path is unchanged.
- Tentative (pending user review): explicit empty `win` on `focus-directional`
  encodes no focused window for in-process and JSON callers. Missing/null and
  invalid nonempty ids still refuse; move/send still require a subject.
- Floating-workspace navigation, stale named-subject, held/unreadable scope,
  suspension/elevation and at-most-once gates are retained. No nearest-edge or
  destination fallback policy was added; unreadable destination handling reuses
  the existing pair logic. No focus-recovery production code was needed.
- The Windows hook already queues origin-less physical focus intents. Its thin
  dispatch now reports the command to core after fresh observation and native
  safety checks. The separate CLI retains its unmanaged-origin refusal; no
  injected-key exception or broader live-control route was added.
- Tier 1 pinned source cells updated. i3/sway workspace-node output jumps and
  Hyprland's default monitor fallback support the selected behavior. COSMIC's
  surface check is inconclusive for the exact sequence; outcome stays TBD for
  user testing. No new COSMIC policy selected.

## Accepted evidence (2026-10-26)

- Red/green: the no-focus crossing regression initially rejected with
  `snapshot-invalid` / `command-invalid`; it passes after implementation.
- `world_no_focus_tests`: 16 tests cover occupied/empty origins, source context,
  existing remembered destination selection, no neighbor/target/source,
  contradictory focus, stale/invalid commands, vertical topology, floating
  source/destination, held source and unreadable domains.
- `world_no_focus_directional`: two public-engine acceptance journeys prove
  loss -> crossing -> quiet arrival and loss -> no-op -> quiet observation.
  Host loss mints no forced-focus effect, including an occupied shown workspace.
- Protocol: explicit-empty decode/strict-invalid tests and decode-to-engine
  crossing with canonical reply equality pass. Windows: five new direct-world
  and production keyboard-route tests cover occupied/empty origins, absent
  target and suspension/elevation fences.
- Native Windows gates pass with Rust/Cargo 1.99.0 (MSVC):
  `mise exec -- cargo build --locked -p tiler-core -p tiler-protocol -p tiler-windows`,
  the corresponding `cargo test --locked`, `cargo fmt --all -- --check`, and
  `cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-windows --all-targets -- -D warnings`.
  Core library: 925 passed; Windows library: 1067 passed, one existing ignored.
- Independent source/diff review found no blockers. Physical-hook classifier,
  queue and dispatch inspection confirms the origin-less route is reachable
  without changing input consumption or synthetic-input filtering.
- Fixture-only native Windows run: owned minimize removed managed focus;
  observation-only follow-up did not force native foreground back. Existing
  focused boundary retention and the CLI unmanaged-origin refusal were observed,
  not misclassified as new-path proof. No synthetic input was used. Exact owner
  stop/independent restore and fixture shutdown completed; settings bytes and
  taskbar/Mouse Snap preimages matched, with no process/ledger/request residue.
  Raw provenance, artifact hashes and native readbacks remain outside the repo.

## Remaining verification and limits

- Implementation pushed as `00221036`; integration verification remains open
  because the whole-workspace CI gates are not green. Keep this record active
  until the failures below are resolved and the required gates rerun.
- Native macOS/KDE integration, physical no-focus shortcuts, native no-focus
  output crossing and game variants remain unproved.
- A live new-path leg was not practical: normal owners ignore injected keys,
  and the approved CLI requires a managed origin. Neither gate was weakened.
- Review notes: the vertical regression exercises Down (Up shares the same
  branch); fit-excluded/game destination exclusion relies on existing session
  admission. No new game-specific behavior or acceptance is claimed.

## Cross-platform CI evidence and blocker

[CI run 40315667855](https://github.com/beefsack/OmniTiler/actions/runs/40315667855)
at `00221036` completed with failure. Comparison:
[parent run 40313044307](https://github.com/beefsack/OmniTiler/actions/runs/40313044307)
at `68ce0b75`.

- macOS CI passed its fixture and portable build/tests, unsafe gate, formatting
  and strict clippy. Shell, licenses/bans/sources and notices jobs also passed.
- Linux shared-core no-focus unit and acceptance journeys passed. The workspace
  test step then failed on eight untouched `tiler-macos` dock-swipe tests with
  `macOS major unreadable`, identical to the parent run. Later Linux formatting,
  strict clippy, unsafe and KDE exporter steps were skipped, not passed.
- KWin tests failed fetching `async-broadcast v0.7.2` in offline mode, also in
  the parent run; typecheck was skipped. Native effect tests failed the same two
  facts-transport tests as the parent; native settings tests were skipped.
- Windows portable tests hit the same eight dock-swipe failures plus
  `wrong_sid_arrival_is_unproven_without_replay` (`scripted full row`). The
  additional failure is absent in the parent and passes on Linux/macOS at the
  same feature revision; its cause is unproven, not established as pre-existing
  or flaky. The test and scripted native-row loop are untouched by this change.
  Later Windows CI formatting/clippy gates were skipped; the local Windows
  three-package gates above passed independently.
- No unrelated test, dependency, native transport or safety-gate changes were
  made to obtain a green result. Next: resolve the owning macOS/KWin/native CI
  failures, investigate the additional Windows test failure, rerun required
  gates, then archive this record. Physical new-path proof remains separate.
