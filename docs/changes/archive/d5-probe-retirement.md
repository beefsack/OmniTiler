# D5 Probe Retirement: Closed Destroyed-Identity Diagnostic Removed

- **Status:** Complete (2026-11-25), offline verification only.
- **Source:** [macOS v4 open-decisions review](../macos-v4-open-decisions-review.md#part-6-follow-up-review-1c90b677e97ef34a-and-next-macos-work-queue-2026-11-25) finding **P6-6**; D5 closed on fallback Option A (`61b8d128`, `AXUIElementDestroyed` is state-identical on first window-hide and close, so no production destroy transport will be built).
- **Boundary:** Offline verification only.

## Goal

Retire the closed D5 exact-identity destroyed-window diagnostic so no probe code without a live trigger stays compiled into `tiler-macos` ([Simplicity](../../principles.md#simplicity)) and the hand-justified `unsafe` surface shrinks ([Rust and Memory Safety](../../principles.md#rust-and-memory-safety)). Production observation is unchanged: `ffi::ax_observe_session`, `AX_SESSION_WINDOW_NOTIFICATIONS` (including its `AXUIElementDestroyed` delivery count consumed by `ax_session.rs`), and the Phase 1 observer stay.

## Scope

- Deleted `crates/tiler-macos/src/ax_destroyed_probe.rs` (1,482 lines) and its `lib.rs` module declaration.
- `crates/tiler-macos/src/cli.rs`: removed `Command::AxDestroyedProbe`, its parser arm, and its `help_text()` entry.
- `crates/tiler-macos/src/cli_tests.rs`: removed the dedicated `ax-destroyed-probe` parser test.
- `crates/tiler-macos/src/main.rs`: removed the dispatch arm.
- `crates/tiler-macos/src/ffi.rs`: removed the diagnostic seam (`DestroyedRefcon`, `ax_destroyed_callback`, `ax_observe_destroyed` and its non-macOS stub), the D5-only public types (`DestroyedEvent`, `AxDestroyedRegistration`, `AxDestroyedOutcome`), and the probe-only `app_set_hidden` helper (macOS implementation, stub, and both `pub use` lists). It had no non-probe caller; `app_hidden_for_pid` remains because production classification reads it.
- `crates/tiler-macos/src/fixture_post.rs`: removed the three probe-only harness senders `send_fixture_set_patch`, `send_fixture_get_many`, and `send_fixture_list` plus their private request-id constants, for which the probe module was the only caller. The spawn/close/focus exchanges stay.

Non-goals: no `fixture_leg.rs` split (**P6-3**), no CLI help refresh beyond the removed probe entry (**P6-7**), no docs-wide status sync, no native/live legs, no merge-resolution or Package 1 changes, no production observer or shared AX/CF/private-SPI helper changes.

## Acceptance

- No `ax_destroyed_probe`, `AxDestroyedProbe`, `ax-destroyed-probe`, `ax_observe_destroyed`, `DestroyedEvent`, `AxDestroyedRegistration`, or `AxDestroyedOutcome` reference remains under `crates/`.
- `cargo fmt --all -- --check`, `cargo test --locked -p tiler-macos --lib`, `cargo test --locked -p tiler-macos --test integration`, and strict `cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-macos --all-targets -- -D warnings` are clean.

## Approach

Delete the closed surface at its existing boundaries rather than leaving aliases or compatibility shims: the probe module and its CLI vocabulary, the self-contained `mod macos` observer seam (between `ax_observe_session` and the Phase 1 observer), and the top-level result types. `app_set_hidden` was removed only after a repository-wide search showed its only caller was the probe. Re-export lists were rewrapped to rustfmt's layout after dropping the two removed names.

## Verification

Offline gate at the merged tree (2026-11-25):

- `cargo fmt --all -- --check` — clean.
- `cargo test --locked -p tiler-macos --lib` — **1,692 passed / 0 failed / 3 ignored** (1,699 before: six probe-module tests plus the CLI parser test removed).
- `cargo test --locked -p tiler-macos --test integration` — **152 passed / 0 failed** across three consecutive runs after the change. One earlier run under heavy host load failed `focus_gaps_trace::native_focus_walk_retains_at_edge_per_tier1` with one focus report refused `stale-observation`; the test passes 3/3 in isolation and 3/3 in the full suite afterwards, and the removed probe code never ran in that path (pre-existing load-sensitive harness race, unrelated to this change).
- `cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-macos --all-targets -- -D warnings` — clean.
- Reference sweep: `rg 'ax_destroyed_probe|AxDestroyedProbe|ax-destroyed-probe|ax_observe_destroyed|DestroyedEvent|AxDestroyedRegistration|AxDestroyedOutcome' crates/` — zero hits; `app_set_hidden`, `send_fixture_set_patch`, `send_fixture_get_many`, `send_fixture_list`, and their request-id constants also have zero hits.
- Removed: 2,191 lines deleted (2,162 net), including the 1,482-line probe module and the 305-line `ffi` seam with 14 `unsafe` occurrences (one `unsafe extern "C-unwind" fn` plus 13 `unsafe` blocks).

## Outcome

Complete. The closed D5 diagnostic no longer exists in the crate: no probe module, CLI command, help entry, parser test, dispatch arm, FFI seam, result types, or probe-only fixture senders. Production observation is untouched (`ax_observe_session`, `AX_SESSION_WINDOW_NOTIFICATIONS`, Phase 1 observer, `app_hidden_for_pid`, and the spawn/close/focus fixture exchanges all remain), and the hand-justified `unsafe` surface is 14 occurrences smaller. Offline gates pass at the merged tree; native/live legs remain unrun by design. Tracked as P6-6, closed by this note in the review's Part 6 closure line.
