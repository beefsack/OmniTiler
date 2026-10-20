# Live fixture readiness

- Status: transport and documentation delivered; native macOS test and scoped
  admission gaps remain open below. Windows/macOS live behavior is verify on
  first use, not established by headless tests.

## Goal and scope

- Make `tiler-live-fixture` the standard controllable, observable window source
  for agent live tests on KDE, Windows and macOS, with fixture-only tiler scope.
- Add owner-restricted Windows named-pipe transport with the existing nonce and
  JSON-lines contract; retain Unix sockets on Linux/macOS.
- Complete point-of-use protocol/field documentation and native-extension recipe;
  document first-use native-platform verification and retain prior KDE evidence.
- No product bug fixes, F1 reconciliation work, history ledger, new native window
  features, or changes to existing proof-specific helper contracts.

## Acceptance and approach

- Preserve existing serve/send, ordered replies/events and absolute wait deadlines;
  Windows endpoint access is owner-restricted, never a permissive default DACL.
- Prefer maintained safe transport APIs; any unavoidable unsafe boundary must be
  minimal, explicitly allowed, independently reviewed and safely wrapped.
- Test portable behavior and compile/test the fixture explicitly in native CI;
  Windows/macOS live behavior remains "verify on first use" until exercised there.
- Reference README covers all ops, fields/defaults, discovery, waits, cleanup,
  identity and platform extension boundaries; platform guides and root guidance
  make fixture-first scoped testing discoverable without changing bound proofs.
- Targeted tests, strict clippy, formatting, dependency/notice/unsafe gates and
  relevant CI establish transport coverage; open native failures are reported
  explicitly. Unchanged macOS-crate non-macOS failures remain out of scope.

## Bounded units

1. Commit/push existing fixture, license and KDE scope work; verify baseline CI.
2. Implement Windows transport and dedicated native fixture checks; review security
   and protocol contracts independently, then commit/push and inspect CI.
3. Review ergonomics without further fixture behavior changes; reuse prior KDE
   live evidence rather than repeat desktop mutation for documentation changes.
4. Complete reference/platform guidance and durable decision; verify and archive.

## Accepted evidence

- Baseline committed as `695255cc`, `a747ad8a`, `d0f6b06c`, `f38cb7de`.
  Baseline-caused native Nix fileset omission fixed in `e6447a7e`.
- Baseline fixture: 117 tests, strict clippy and format passed. KWin: 676 tests
  and typecheck passed; shell suites, dependency policy, notices and unsafe gate
  passed. CI run `39380028627`: KWin/shell/licenses/notices/macOS/native passed.
- Linux Rust and Windows CI fail in unchanged `tiler-macos` references to
  `AX_ATTRIBUTE_UNSUPPORTED` and `take_trailing_proof`, also present on prior main.
  Baseline native CI omitted the fixture, so its green macOS job did not verify it.
- Windows implementation uses Tokio named pipes with event-driven IOCP readiness
  and absolute read/write deadlines. `getrandom` supplies OS entropy on all hosts.
  The literal current-user SID and SYSTEM are the only DACL allow entries;
  remote clients are refused and first-instance creation is exclusive.
- A safe `interprocess` implementation was rejected by the unchanged license
  gate because of 0BSD-only transitive dependencies. The replacement's sole
  unsafe boundary, `pipe_win.rs`, builds the descriptor and wraps token/pipe
  helpers behind safe APIs; the unsafe guardrail explicitly limits this opt-in.
- Independent review identified and corrected token-buffer alignment, a reversed
  Win32 status assertion, an untyped ACL buffer, and Windows-only cfg/import/type
  compile defects. Portable client tests now exercise real transport-backed
  waits on Windows too.

## Outcome and open native verification

- Windows transport committed in `99e79e80`; native compile/test corrections in
  `fcdeed57`, `fb6832b7`, `01b069d5`. The changes include backend cfg gates,
  event/deadline test plumbing and native lint corrections; no OmniTiler product
  bug was repaired. Dependency policy stayed unchanged.
- CI run `39393279249`: Windows fixture build, headless tests and strict clippy
  passed. macOS fixture build passed; its tests finished 118 passed, 2 failed.
  KWin, shell, native, licenses and notices jobs passed. Linux Rust and Windows
  portable builds still fail on the pre-existing `tiler-macos` symbols above,
  already failing at `0f76c595`; these are not fixture regressions.
- Open macOS tests:
  `client::tests::deadline_reader_splits_coalesced_and_enforces_cap` and
  `client::tests::deadline_reader_rejects_oversize_coalesced_second_line`.
  In that run both oversized-frame assertions failed to observe `TooLong`;
  the actual alternative result was not printed. Hypothesis, unverified: the
  sender's giant write fails before delivering over 1 MiB on a macOS socket
  pair. Native macOS investigation must capture both write errors and the actual
  reader result before selecting a fix. No test is disabled or declared green.
- Open macOS admission gap: `AppScope` allows explicit bundle IDs only and
  refuses missing bundle identity; the directly launched fixture has no packaged
  bundle identity. There is no working fixture `--scope` value today. Standalone
  first-use verification is separate; integrated fixture tests must wait for a
  proven scoped admission path, never fall back to unscoped tiling.
- README now documents all CLI/protocol controls, requested versus effective
  defaults, events, waits, identity/cleanup, and the native-extension recipe.
  Platform guides make fixture-first scoped tests discoverable, retain explicitly
  bound legacy proofs, and provide Windows/macOS first-use checklists. Root
  guidance points there; the 2026-10-20 standard is recorded in decisions.
- No further fixture code or live desktop changes accompany this documentation
  finish. Prior KDE isolation/integration evidence remains in the linked archive
  records and README; no fresh live smoke is claimed.
- Next native actions: investigate the two macOS assertions and fixture bundle
  admission; verify Windows/macOS windows, identity, readbacks and cleanup on
  first use under the relevant live guide.
