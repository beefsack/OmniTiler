# Linux planner unsafe guardrail

Goal: apply the 2026-10-13 Rust unsafe guardrail to the Linux planner.

Scope:
- Replace all planner test process-environment mutations with explicit inputs.
- Preserve production environment names, defaults, parsing and precedence.
- Add crate-wide `#![forbid(unsafe_code)]` to every planner crate root.
- Audit other workspace roots, excluding the Windows and KWin effect FFI crates;
  add missing forbids only where trivially unsafe-free.

Non-goals: FFI consolidation, production behavior changes and live testing.

Acceptance:
- Planner contains no unsafe code or process-environment mutation.
- Deterministic tests pin existing environment behavior without changing it.
- All applicable crate roots forbid unsafe code; remaining gaps are reported.
- Rust workspace tests, all-target clippy with warnings denied and fmt pass.
- Unsafe-policy backlog and decision status reflect delivery; note archived.

Approach: audit roots and mutation sites, introduce minimal explicit configuration
injection, preserve the environment-reading wrapper, verify and reconcile records.

Units: implementation and crate audit; acceptance review and workspace gates;
completion records.

Verification: crate-wide source audit; `cargo test --workspace`;
`cargo clippy --workspace --all-targets -- -D warnings`;
`cargo fmt --all -- --check`. Native CTest is needed only if the effect FFI
staticlib is affected.

Material decisions:
- Keep the public environment reader and its per-call lookup; a private
  `Option<&str>` helper supplies explicit trace configuration in tests.
- Separate integration-test/example crates do not inherit library forbids;
  add the guardrail at every unsafe-free target root found by Cargo metadata.

Evidence (2026-10-13, offline Linux):
- Planner audit: five unsafe call sites removed (three `set_var`, two
  `remove_var`); no unsafe code or env mutation remains crate-wide.
- Trace tests pin `OMNITILER_TRACE`, absent/default-off, eight disabled values
  and exact `1` opt-in. Unset/non-Unicode reads remain disabled; production
  names, parsing, lookup timing and precedence are unchanged.
- Added 32 forbids: planner lib/bin/test (3), core integration tests (23),
  protocol integration tests (5) and protocol example (1). Core/protocol
  libraries already forbid unsafe. All 34 applicable target roots now forbid;
  no remaining gaps outside the excluded Windows and KWin effect FFI crates.
- `cargo test --workspace`: 1,644 passed, zero failed/ignored across 53
  unit/integration/binary suites; four additional doctest suites contain zero
  tests. Planner subtotal: 142 unit tests and 11 integration tests passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: all five workspace
  packages passed, zero warnings/errors.
- `cargo fmt --all -- --check` and diff whitespace check: passed.
- Native CTest not required: effect FFI staticlib and its implementation
  dependencies are unchanged; core/protocol edits only add target-root lints.

Outcome: complete; unsafe-policy backlog and decision status updated. Windows
FFI consolidation remains queued; KWin effect FFI retains its existing boundary.
No production behavior changes or unresolved blockers.
