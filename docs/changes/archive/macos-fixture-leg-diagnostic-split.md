# macOS fixture-leg diagnostic split (P6-3)

Status: Complete (2026-11-25), offline verification only.

## Goal

Split diagnostic-only helpers out of `crates/tiler-macos/src/fixture_leg.rs`
so the file returns toward the ~1,500-line soft cap and
`run_leg_body_macos` stops carrying every diagnostic inline. Behavior, CLI
flags, JSON evidence, and teardown order are unchanged.

## Scope

- Move the post-only owned-Space planning/setup/evidence cluster (including
  its test) into a focused `fixture_owned_space.rs`: occupancy inventory,
  single-append ordinal, `PostOnlyOccupancy`, `PostOnlyOwnedSpace`,
  membership reads/polling/refusal, and the post-only establishment block
  currently inline in `run_leg_body_macos`.
- Move the `maximize-toggle` native evidence path into
  `fixture_maximize.rs`.
- Move the `corrupt-intent-store` pre-seed and bounded diagnostic capture
  into `fixture_corrupt_store.rs` (the leaf stays owned by the post-shutdown
  `live_leg` teardown; never deleted mid-run).
- Move the remaining `fixture_leg.rs` tests into `fixture_leg_tests.rs`,
  declared from `fixture_leg.rs` with `#[path]` so private items stay
  reachable; the owned-Space helper tests move with their helpers.

Non-goals: no CLI/help/docs edits (P6-7), no history rewrite (P6-4), no
native/live legs, no merge resolution changes, no `docs/backlog.md` edits, no
behavior, timing, permission, path, JSON, or teardown-order changes.

## Approach

Existing module style (`fixture_hover.rs`, `fixture_sample.rs`): private
`mod` registration in `lib.rs`, `#![forbid(unsafe_code)]` plus a focused
module doc, only necessary `pub(crate)` items, no new abstraction layer.

## Decisions

- `plan_owned_destinations`/`static_chord_ordinal` move with the owned-Space
  boundary: they encode the same Desktop-9 holder invariant as
  `post_only_owned_ordinal` and are the only other destination-planning code.
- `MAXIMIZE_TOGGLE_CHORD` stays in `fixture_leg.rs`: other modules
  (`cli.rs`, `live_leg.rs`) name it through that path.
- `FOCUS_SETTLE_MS` stays: it paces every chord post, not just post-only
  legs.
- `run_leg_body_macos` keeps its existing `too_many_lines` allow only if it
  remains over the lint threshold; no new allow is added.

## Units

1. `fixture_owned_space.rs`: helpers + establishment + helper tests.
2. `fixture_maximize.rs` and `fixture_corrupt_store.rs`: diagnostic paths.
3. `fixture_leg_tests.rs`: remaining leg tests.

## Verification

Baseline before the change: `cargo test --locked -p tiler-macos --lib` =
1695 tests, `--test integration` = 152 tests. Gate: fmt check, lib and
integration tests, clippy `-D warnings`, `git diff --cached --check`; all
changed files staged, no commit/push.

## Evidence

Baseline counts captured from the staged merge tree (no source changes yet).

Unit 1 accepted: `fixture_leg.rs` 2347 -> 1843 lines, new
`fixture_owned_space.rs` 591 lines, lib registration +1 line; moved region
diffed byte-identical modulo visibility; `run_leg_body_macos` 499 -> 400
lines with the post-only block now a 5-line call; lib tests green, clippy
clean, no new allow.

Unit 2 accepted: `fixture_maximize.rs` and `fixture_corrupt_store.rs` added;
`fixture_leg.rs` 1843 -> 1770 lines; maximize JSON body and corrupt-store
error strings copied verbatim; corrupt-store leaf ownership left with the
post-shutdown `live_leg` teardown; lib tests green, clippy clean.

## Outcome

Complete. `fixture_leg.rs` 2347 -> 1329 lines; new production modules
`fixture_owned_space.rs` (591), `fixture_maximize.rs` (68),
`fixture_corrupt_store.rs` (60); `fixture_leg_tests.rs` (439) holds the leg
tests (13 remain under `fixture_leg::tests`, 4 helper tests moved with
`fixture_owned_space`). `run_leg_body_macos` 499 -> 331 lines; its existing
`too_many_lines` allow is retained (still over the 100-line threshold). The
stale allow on the 20-line `run_leg_body` delegator was removed.

Behavior parity: moved statements, error strings, JSON bodies, settles, and
ordering copied verbatim (moved regions and test bodies diffed against the
staged pre-split file; the two source-scanning tests lost only their dead
production-section split). `note_owned_space` registration order, pending
launch ordering, and post-shutdown corrupt-store leaf ownership are
unchanged.

Evidence: `cargo fmt --all -- --check` clean; `cargo test --locked -p
tiler-macos --lib` 1693 passed, 3 ignored (baseline 1695 total + 1 split-out
test); `--test integration` 152 passed; `cargo clippy --locked -p tiler-core
-p tiler-protocol -p tiler-macos --all-targets -- -D warnings` clean.
