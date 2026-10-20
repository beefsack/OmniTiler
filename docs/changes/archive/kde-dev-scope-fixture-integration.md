# KDE development scope and fixture integration

- Status: delivered; scoped controller journeys completed on KDE Wayland and
  X11/XWayland. Native-effect visuals remain host-limited. Client-driven geometry
  reconciliation is an evidence-backed follow-up, not changed here.

## Goal and scope

- Add a repeatable development-only KDE class/app-id admission scope, retaining
  unchanged unscoped behavior and keeping all plumbing in the KDE platform layer.
- Exercise native Wayland and X11/XWayland fixture journeys with OmniTiler active;
  distinguish host limitations, fixture defects and tiler findings.
- No core behavior changes, product fixes, persistent configuration changes beyond
  the normal development lifecycle, or exclusive display-mode tests.

## Acceptance and approach

- Exact identity matching, multiple allowed identities, unchanged empty scope;
  validate inputs before live mutation and test platform admission/plumbing.
- Document fixture-scoped development usage and promote the durable scope choice.
- Retain authoritative fixture/native observations correlated with controller and
  Planner diagnostics, evaluated readiness and complete cleanup evidence.
- Preserve unrelated windows and configuration; report rather than repair tiler
  defects. Fix only small reproduced fixture defects with regression coverage.
- Run relevant TS/Rust tests, lint, formatting and repository entry points.

## Bounded units

1. Implement platform-only scope plumbing, targeted tests and usage documentation.
2. Independently review admission safety and launcher/lifecycle contracts.
3. Run scoped fixture journeys on both backends; assess ergonomics and history.
4. Verify cleanup, consolidate evidence and archive the completed record.

## Baseline

- Fixture isolation and API coverage are recorded in
  [the original helper record](controllable-live-test-window.md) and
  [the expansion record](live-fixture-api-expansion.md).
- Existing KDE admission covers normal windows across the entire host world.
  A desktop/output choice is not a window-management isolation boundary.
- Existing fixture and dependency changes are retained unchanged unless a small
  fixture correction is established by this integration work.

## Scope implementation and offline evidence

- Repeatable `--scope-class NAME` on `just dev`, `dev-kde`, `dev-on` and
  `scripts/start-test.sh start`; exact case-sensitive class/app-id allowlist.
  The dev bundle bakes a validated comma-separated list. Installed bundles
  always bake an empty scope, even with a contaminated build environment.
- Platform observation omits foreign windows and focus; window-bearing command
  submissions refuse foreign targets. Malformed scope and throwing scope readers
  refuse startup/observation rather than broadening management. Explicit scope
  on an already-up controller refuses instead of implying a changed scope.
- Independent review found and closed fail-open reader/observation paths and
  installed-build contamination; regression coverage includes each correction.
- Offline verification: TypeScript typecheck and 672 tests passed; mocked dev-loop
  harness 457 checks passed; source-archive harness, shell syntax and just format
  passed. API guard passed during implementation verification.
- A malformed-list CLI probe exposed a trailing-empty validation hole before its
  correction and loaded an inert controller. The retained receipt and journal
  established `scope-invalid` with no ready/dispatch; exact receipt-bound unload
  completed, followed by verified controller/Planner DOWN. Regression tests now
  reject trailing/leading/double commas before build/load.
- First scoped bring-up failed evaluated readiness: shell quoting baked a stray
  apostrophe into the scope, so startup correctly refused. No fixture journey
  proceeded. Exact controller/Planner teardown and protected-window/configuration
  comparisons passed. The quote expansion and duplicate env/argv forwarding were
  corrected; executable esbuild/VM tests now prove empty, single and multiple
  scope values. Updated offline suite: 676 tests passed, plus the 457-check mocked
  lifecycle harness and archive contamination checks.
- Native-effect preflight reported an incompatible staged effect ABI. Controller
  journeys use the documented detached lifecycle; effect visuals are not claimed
  and no native installation or session change is part of this work.

## Live outcomes

- Captured fixture JSON replies, correlated complete D-Bus request/reply traffic
  with positive markers, journal cursors under the recorded KWin identity, native
  owned-window snapshots and before/after inventories outside the repository.
- Successful bring-ups established `world:ready`, `world:discovered`, matched
  dispatch/applied pairs and `ReadFloatIntent`/`DiscoverWorld` traffic before
  fixture mutations. Scoped bundle and journal identities matched the fixture
  sentinel; foreign windows were absent from management.
- Normal detached `dev-on`/`dev-off` controller runs were used, not a full native
  effect/tray visual acceptance run. No exclusive fullscreen or output-mode
  changes were attempted.

| Scenario | Wayland | X11/XWayland |
| --- | --- | --- |
| Spawn three normal windows, tile | Pass: three geometry writes and observed negotiated sizes | Pass: three tiled windows, native positions verified |
| Minimum-size constraint | Host-limited: hint accepted, programmatic resize not clamped | Pass: below-minimum request clamped natively |
| New fixed-size windows | Pass: non-resizable and min=max admissions floated above tiles, sizes retained, tiled siblings untouched | Pass: both fixed predicates produced above floats without disturbing tiles |
| Maximize/restore and borderless fullscreen/exit | Native mode/geometry readbacks passed; standalone changes need not dispatch (finding below) | Maximize/restore native flags and retained tile restored; fullscreen geometry verified, mode getter remains unknown by design |
| Minimize/restore | Host-limited: minimize accepted, state unknown, restore unsupported; owned window then closed | Pass: native HIDDEN/minimized then focused/windowed restoration |
| Decorations off/on | Host-limited: accepted cache-only hints, no trustworthy readback | Pass: native frame extents removed/restored |
| Dialog/utility types | Host-limited: X11-only roles | Pass: native roles verified, excluded from tiled trio |
| Levels, attention and focus | Host-limited: non-normal level/focus and attention clear unsupported; attention request forwarded | Pass for top/bottom/normal flags and bounded focus readback; attention clear caveat below |
| Batch close and reflow | Pass: survivors expanded after removal, no stale gap | Pass: closing tiled/role windows expanded survivor; close-all settled |
| Self-resize while tiled | Tiler finding: client geometry remained until next subscribed event | Same finding; exact resize predicate matched without world dispatch |
| Compact output, subscriptions, event-driven waits | Pass: subscribed mutation stream and focus predicate matched | Pass: compact snapshots and focus predicate matched |
| Foreign-window exclusion | Pass: protected non-fixture frame/state unchanged | Pass: protected non-fixture frame/state unchanged |

## Tiler findings and host limits

- F1: with two tiled windows, a client resize without activation or homing leaves
  client geometry different from the retained tile and emits no world dispatch
  during the bounded capture. Both backends reproduced it. A later admission
  triggers reconciliation and restores the old tile allocation (Wayland
  supplemental request 9 applied three geometry writes). X11 maximize/restore
  similarly changed native state without dispatch; native restoration still
  returned to the original allocation.
- Source explains the gap: `kwin/src/world-entry.ts` subscribes per-window
  `outputChanged`/`desktopsChanged`, workspace inventory/activation/view changes
  and configuration changes, but no ordinary frame/maximize/fullscreen change
  signals. This establishes current mechanics, not an intentional product policy
  or desired correction behavior. Preserve as a tiler observation/invalidation
  finding; exact desired reaction and reference outcomes remain TBD in the
  [minimal discriminator](../../spec/reference-outcomes.md#fixture-driven-native-state-discriminators).
  No tiler product repair was made.
- B1 was not a fixture bug: X11 attention clear removes WM_HINTS urgency, while
  KWin's DEMANDS_ATTENTION persists on an inactive window until activation.
  Isolated reproduction separated the two native properties and verified
  activation clears the remaining state. The fixture uses winit's hint-clear
  operation correctly. README/protocol wording and a reporting regression test
  now prevent interpreting `accepted` as compositor withdrawal. No fixture
  behavior fix was necessary.
- Wayland global position, restore-from-minimize, focus driving and several
  stacking/readback controls remain explicitly unsupported. Native-effect ABI
  mismatch prevented visual effect acceptance; no visual blur/chrome/rendering
  claims follow from these programmatic results.

## Usefulness and history verdict

- Useful for repeatable real tiler tests: stable caller IDs, multiple windows,
  ordered batches, honest requested/configured/observed fields and compact output
  made admission, modes, constraints and reflow practical on both backends.
- Bounded `--wait-ms`/`--expect` was the strongest settlement primitive: focus
  predicates matched real readbacks in the completed runs. A match proves a
  point-in-time native predicate, not future stability or tiler agreement.
- Fixture request IDs plus journal cursors and Planner/D-Bus slices correlate
  a step, but do not automatically map each fixture ID to a native/internal ID
  or each resulting geometry write to that step. `applied member` is membership
  writes, not a managed-window count; this is clarified at point of use.
- A small bounded request-correlated change history would have helped identify
  which survivor received the later reclaim write, distinguish a dialog/utility
  exclusion from a quiet normal window, and retain before/after evidence across
  the silent self-resize interval. The urgency experiment also needed two native
  state sources beyond the accepted reply. A history cannot prove exclusive
  causation, recover an unobserved host signal or replace unsupported readbacks.
- Priorities: (1) resolve F1's desired event/invalidation contract, (2) expose a
  stable fixture/native identity join and changed-field observation in test
  evidence, (3) consider bounded timestamped last-N history with request overlap
  correlation. History remains deferred, not implemented or newly approved.

## Final verification and cleanup

- Latest TypeScript typecheck and 676 tests passed; KWin API guard clean. The
  repository has no separate TS lint entry point. Mocked dev lifecycle: 478
  checks passed, including literal shell-metacharacter arguments and inherited
  scope-environment isolation. Just recipes use positional arguments rather
  than shell-interpolated input; explicit flags/default alone select the scope.
- Source-archive contract tests passed, including installed-scope contamination;
  shell syntax, just formatting, Rust workspace formatting and diff whitespace
  checks passed.
- `cargo test --locked -p tiler-live-fixture`: 117 passed.
  `cargo test --locked --workspace --exclude tiler-macos`: 2,327 passed.
  Strict all-target clippy with the same exclusion passed. The existing macOS
  crate Linux failures were excluded, not repaired or reclassified.
- Unsafe guardrail: 32 tests passed; third-party-notice freshness passed.
  Dependency policy changes predate this work; no dependency was added here.
- Every successful run stopped through `just dev-off`; exact controller/helper
  objects and plugins disappeared, Planner processes exited and status returned
  DOWN. Owned fixture servers shut down, windows closed, sockets and empty run
  directories were removed. Owned capture processes and the earlier empty
  controller receipt directory were removed; no owned runtime residue remains.
- Protected-window frame/state and pointer comparisons matched. Full main-leg
  shortcut/component tuples, project configuration, desktop and effect inventories
  matched; pre-existing stale shortcut records were preserved. Native effects
  were not loaded. Runtime float-intent store absence was restored after retaining
  the run-created empty store as evidence.
- KWin configuration bytes/hash matched the baseline. The normal disable/enable
  route rewrote its inode/mtime; timestamp equality is not claimed. No unrelated
  configuration, global shortcut, real window or preserved residue was changed.
