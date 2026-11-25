# Package 1 Rectification: Fixture Store Teardown, Applied Close Drain, Window-Id Redaction

- **Status:** Complete (2026-11-25), offline verification only.
- **Source:** [macOS v4 open-decisions review](../macos-v4-open-decisions-review.md#part-6-follow-up-review-1c90b677e97ef34a-and-next-macos-work-queue-2026-11-25) findings **P6-1**, **P6-2**, **P6-5**.
- **Boundary:** No native/live legs. Offline verification only. `docs/backlog.md` is owned by the orchestrator; this note carries the slice's status wording.

## Goal

Close the three correctness/hygiene findings that must land before new native fixture legs:

1. **P6-1:** the fixture-isolated `product/intent-store` is removed only after owned product shutdown, inside `live_leg`'s symlink-guarded owned-run-dir teardown. The mid-run unguarded `remove_dir_all` in `fixture_leg` is gone, so every failure path leaves the store for teardown instead of stranding it as unknown-leaf residue.
2. **P6-2:** on `ReplyVerdict::Applied`, accepted `report.closed` ids are drained from `session.pending_closes` before the `intent_write_hold` and `prove_coherent` early returns, in both the `Accepted` and `CommandConsumed` dispatch paths (and the gesture path that shares the report build). An executed planner-owned select propagates its operation identity on an `intent-write-held` refusal exactly as on a stale-observation refusal.
3. **P6-5:** the normal-level `intent-store: intent binding unverifiable` diagnostic identifies the target by `pid` only, never the raw `<pid>_<wid>` canonical id, per R15.

## Scope

- `crates/tiler-macos/src/live_leg.rs`: owned intent-store teardown (known row/temp leaves, symlink refusal, unknown-leaf residue) plus regressions.
- `crates/tiler-macos/src/fixture_leg.rs`: remove the mid-run corrupt-store `remove_dir_all`; capture the product diagnostic only.
- `crates/tiler-macos/src/world_dispatch.rs`: applied-close drain placement, `intent-write-held` select identity, dispatcher regressions.
- `crates/tiler-macos/src/runtime_thread.rs`: held-select dispatch regression using the real select lane.
- `crates/tiler-macos/src/world_runtime.rs`: redacted unverifiable-binding diagnostic plus unit check.

Non-goals: P6-3 (`fixture_leg` split), P6-4 (history tidying), P6-6 (D5 probe retirement), P6-7 (help/status refresh); no production behavior change beyond the three fixes; no native legs.

## Acceptance

- **P6-1:** targeted `live_leg` teardown tests pass: known store row/temp removed; unknown store leaf retained as residue; store/leaf symlink refused; teardown no longer depends on the corrupt-store happy path.
- **P6-2:** regressions prove an accepted close leaves `pending_closes` even when an intent write holds the reply or emission coherence fails, on both `Accepted` and `CommandConsumed` replies, and an executed select's operation id is returned on `intent-write-held`.
- **P6-5:** a unit check proves the normal diagnostic carries the pid and no raw window id.
- **Integration:** `cargo fmt --all -- --check`; `cargo test --locked -p tiler-macos --lib`; `cargo test --locked -p tiler-macos --test integration`; strict `cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-macos --all-targets -- -D warnings`. Only merge-caused breakage may be repaired.

## Approach

- P6-1 cleanup stays in `live_leg::remove_owned_run_dir` (post-shutdown, symlink-guarded, allowlist-only); `fixture_leg` reads the bounded product diagnostic and leaves the leaf to teardown.
- P6-2 drains `report.closed` once at the top of each `ReplyVerdict::Applied` arm, before any hold/coherence return, and binds `select_operation` on `intent-write-held` when the dispatched command was a select.
- P6-5 derives the log subject from the parsed pid and keeps the one-per-window bound.

## Units

1. P6-1 live-leg teardown and regressions (partial work completed in the worktree; verify and finish).
2. P6-2 applied-close drain and select identity, with dispatcher/runtime regressions.
3. P6-5 diagnostic redaction plus unit check.
4. Merge-tree integration gate (offline builds/tests/clippy).

## Verification

Offline gate at the merged tree (2026-11-25):

- `cargo fmt --all -- --check` — clean (after rustfmt-only repair of merge-caused drift in `world_command_issue_tests.rs`, `world_global_swap_tests.rs`, and the staged Windows merge files `settings.rs`, `world_capture.rs`, `world_diff_tests.rs`, `world_native_roundtrip_tests.rs`, `world_sequencer.rs`).
- `cargo test --locked -p tiler-macos --lib` — **1,699 passed / 0 failed / 3 ignored** (baseline 1,694; five new regressions).
- `cargo test --locked -p tiler-macos --test integration` — **152 passed / 0 failed**.
- `cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-macos --all-targets -- -D warnings` — clean (4m31s; workspace dependency `tiler-windows` also checked).

Focused regressions:

- `live_leg::tests::intent_store_cleanup_takes_only_expected_leaves` — known row + atomic temp removed; unknown leaf retains the whole store.
- `live_leg::tests::intent_store_cleanup_refuses_symlinked_store_or_leaf` — store and leaf symlinks refused.
- `runtime_thread::trailing_join_tests::accepted_intent_hold_drains_closes_and_keeps_select_identity` — real store hydration, gone-process hold, real select lane: close drained, claim carries `op-<seq>`, no refusal counted, `intent-write-held` ring row.
- `world_dispatch::tests::consumed_stale_reply_drains_accepted_closes_before_refusal` — genuine `CommandConsumed` replay refused as `stale-observation`, close already drained.
- `world_runtime::tests::unverifiable_intent_diagnostic_redacts_native_window_id` — pid named, raw wid/canonical id absent.

Falsification (temporary reverts, restored): removing the CommandConsumed drain fails test B; reverting the select identity to `None` fails test A.

## Outcome

Complete. The fixture intent store is now cleaned only post-shutdown by the symlink-guarded owned-run-dir teardown (known row/temp allowlist, unknown leaves retained as residue), and the corrupt-store diagnostic no longer removes the store mid-run. Accepted replies drain `report.closed` before any held/stale early return in both `Accepted` and `CommandConsumed` dispatch (plus the gesture path), and an executed select keeps its operation identity on an `intent-write-held` refusal. The normal-level unverifiable-binding diagnostic names only the pid. Offline gates pass at the merged tree; native/live legs remain unrun by design.
