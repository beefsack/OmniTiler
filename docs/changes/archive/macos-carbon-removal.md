# macOS Carbon hotkey removal

## Goal and scope

Remove the diagnostic-only Carbon hotkey route in favor of the existing event
tap route, following the 2026-10-16 Simplicity decision after physical Carbon
delivery failed 0/3 presses. Remove its FFI, dispatcher, probe, route flags,
tests, recipes, and current documentation. Preserve the event tap implementation,
enabled-state health check, re-enable recovery, and tests unchanged.

Historical evidence stays intact. Mark obsolete Carbon live-procedure steps as
retired rather than deleting their results. No live testing or unrelated shared
core changes are in scope.

## Acceptance and approach

- Remove Carbon-only code and current invocation surfaces.
- Update the input-route decision and remove the completed backlog follow-up.
- Record comparable before/after macOS Rust unsafe block and usage counts.
- Review the removal and historical-documentation boundaries.
- Run macOS tests, workspace strict clippy and formatting checks, signed app
  build, and signature verification using the repository tooling.
- Verify hosted CI for the resulting commit.

## Outcome

Removed the Carbon Event Manager table and dispatcher handlers, registration
guards, native input factory/source, `input-register` command, Carbon route
selection/report fields and Carbon-only tests. No Carbon-specific just recipes
existed. The tap factory/source, enabled-state health check, re-enable recovery
and their regression coverage remain intact; removed Carbon values now refuse
parsing. Existing `--route tap` callers remain valid. Read-only
`CopySymbolicHotKeys` and the SLPS focus APIs are unrelated and remain.

Updated the crate README, route decision, current preview guidance and backlog.
The archived physical script marks the removed diagnostic step retired without
changing its instructions or results; the live guide and discriminator row
likewise fence it from reuse. Historical research and other archived evidence
are unchanged.

### Unsafe surface

Across all 72 Rust files in the macOS crate, raw regex counts before → after:

| Measure | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| Unsafe blocks (`\bunsafe\s*\{`) | 103 | 93 | 10 |
| Unsafe function/signature occurrences (`\bunsafe\s+(?:extern\|fn)\b`) | 52 | 35 | 17 |
| Whole-word `unsafe` occurrences (`\bunsafe\b`) | 185 | 157 | 28 |

Counts compare the pre-removal tracked source with the resulting source,
including tests and stubs. These are textual occurrences, including comments
and strings in the whole-word measure, not expanded/platform-specific compiler
counts; signature counts include function-pointer types and repeated resolution
signatures. All unsafe blocks remain confined to `ffi.rs`; no unsafe was added.

### Verification

- Locked macOS crate tests: **548 passed**, zero failed (426 library tests plus
  122 integration tests).
- macOS CI portable-crate tests (core, protocol, effect FFI, macOS): **1450
  passed**, zero failed; strict all-target clippy with `-D warnings` passes.
- Workspace formatting and whitespace checks pass.
- Full-workspace strict clippy was attempted on macOS and fails in untouched
  `omnitiler/src/float_intent_store.rs`: unavailable `rustix::fs::mkfifoat` and
  mode-type mismatches. This is an existing host-portability limitation outside
  the macOS CI crate selection; no unrelated source changes were made.
- `mise exec -- just app 'OmniTiler Mac Dev'` and `mise exec -- just verify`
  pass for both signed bundles, including the macOS 15 deployment floor.
  Development Gatekeeper rejection remains an expected diagnostic, not a
  security-policy change.
- Review confirms removal completeness and equivalent tap behavior/recovery.
  No live tiler, tap or probe execution was used for this change.
- Initial [hosted CI](https://github.com/beefsack/OmniTiler/actions/runs/38851426008)
  passed macOS and all tests, but Linux/Windows strict clippy found an unused
  `AtomicU32` import after removal of the shared Carbon counter. The import is
  now macOS-only. macOS tests, CI-selected tests/clippy, formatting and signed
  build/verification pass after this import-only correction. Hosted portability
  confirmation pending publication.
