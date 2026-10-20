# Live fixture readiness

- Status: in progress.

## Goal and scope

- Make `tiler-live-fixture` the standard controllable, observable window source
  for agent live tests on KDE, Windows and macOS, with fixture-only tiler scope.
- Add owner-restricted Windows named-pipe transport with the existing nonce and
  JSON-lines contract; retain Unix sockets on Linux/macOS.
- Complete point-of-use protocol/field documentation and native-extension recipe;
  re-smoke KDE isolation and document first-use native-platform verification.
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
  relevant CI pass; unchanged macOS-crate non-macOS failures remain out of scope.

## Bounded units

1. Commit/push existing fixture, license and KDE scope work; verify baseline CI.
2. Implement Windows transport and dedicated native fixture checks; review security
   and protocol contracts independently, then commit/push and inspect CI.
3. Review ergonomics and perform owned-window KDE isolation re-smoke.
4. Complete reference/platform guidance and durable decision; verify and archive.

## Accepted evidence

- Baseline committed as `695255cc`, `a747ad8a`, `d0f6b06c`, `f38cb7de`.
  Baseline-caused native Nix fileset omission fixed in `e6447a7e`.
- Baseline fixture: 117 tests, strict clippy and format passed. KWin: 676 tests
  and typecheck passed; shell suites, dependency policy, notices and unsafe gate
  passed. CI run `39380028627`: KWin/shell/licenses/notices/macOS/native passed.
- Linux Rust and Windows CI fail in unchanged `tiler-macos` references to
  `AX_ATTRIBUTE_UNSUPPORTED` and `take_trailing_proof`, also present on prior main.
  Native CI currently omits the fixture, so green macOS does not verify it.
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
  waits on Windows too. Native CI validation is pending.
