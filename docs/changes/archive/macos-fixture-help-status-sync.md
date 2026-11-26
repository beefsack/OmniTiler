# macOS fixture-leg help and v4 status sync (P6-7)

- **Status:** Complete (2026-11-25), offline verification only.
- **Source:** [macOS v4 open-decisions review](../macos-v4-open-decisions-review.md#part-6-follow-up-review-1c90b677e97ef34a-and-next-macos-work-queue-2026-11-25) finding **P6-7**; follows the [fixture-leg diagnostic split](macos-fixture-leg-diagnostic-split.md).
- **Boundary:** Documentation and CLI help only; no new flags, behavior changes, fixture split, or native testing.

## Goal

Describe the two diagnostic paths shipped into `fixture-leg` and replace the
stale R3/D4/D5/M4 status sentences left after the v4 work, without claiming
native facts beyond recorded evidence.

## Changes

- `crates/tiler-macos/src/cli.rs` `help_text()`: the `fixture-leg` usage line
  adds `[--corrupt-intent-store]`; its description now states that
  `--corrupt-intent-store` pre-seeds the isolated product store with malformed
  JSON and requires one bounded startup diagnostic while the product still
  tiles and shuts down cleanly, and that the leg-only `--chord maximize-toggle`
  sends two real `maximize-m` presses through the product tap and requires
  native apply plus exact base restore evidence, not slice-5 acceptance.
  Existing flags, the 90 s default, one-chord-per-owned-window ordering, the
  `--hover-probe` three-chord minimum, and preset semantics are unchanged.
- `docs/decisions/macos.md` (macOS v4 open decisions): the stale "C5
  typed-boundary dispatch (R3) remains open" sentence now records that macOS
  R3 typed ingress landed while C5 Windows typed-boundary adoption remains
  open. The D4 `Assumes / revisit if` sentence now records store adoption and
  orderly-restart persistence proved on the signed fixture (2026-11-24), with
  write-failure recovery, numeric launch-date stability, and re-login namespace
  rejection still open. The D5 bullet records the second bounded diagnostic:
  first window-hide and close were state-identical at delivery (AX-absent,
  CG-present, CG-onscreen), so no close-only predicate exists; D5 closed on
  fallback Option A with the CG-alive oracle authoritative, no notification
  transport, and the diagnostic probe retired.
- `docs/changes/v4-phase3-macos-port.md` open decisions: M3 now records D4
  store adoption and orderly-restart persistence proved on the signed fixture
  (2026-11-24) and malformed-store startup degradation (2026-11-25), leaving
  numeric launch-date stability, write-failure recovery, and re-login
  namespace rejection open. M4 records native apply/restore proof
  (2026-11-24) and leaves physical Ctrl+M delivery, paint, native Zoom/drift
  interaction, external move/resize clear, fullscreen replacement,
  send/output/work-area changes, and restart-to-base open.
- `docs/changes/macos-v4-open-decisions-review.md` Part 6 closure: P6-3 is
  closed by the archived split note and P6-7 by this note; P6-4 remains open.
  P6-1/P6-2/P6-5/P6-6 closures are unchanged.

## AXZoomed decision

`fixture_sample::read_joined_frame` keeps `.ok()` on the `AXZoomed` read. Its
only caller is the bounded `fixture-leg` maximize diagnostic; distinguishing a
read error from an absent attribute would require either a new
`FrameSnapshot` field (changing the JSON evidence shape) or a verdict change
that refuses unreadable statuses (changing what the existing maximize evidence
accepts). Both exceed this help/status sync's no-behavior-change boundary, and
`scope_inspect` already reports the exact `AXZoomed` read status where a
diagnostic needs it. The collapse is recorded as residual rather than widened
into a new diagnostic here.

## Verification

Offline verification (2026-11-25):

- `cargo fmt --all -- --check` — clean.
- `cargo test --locked -p tiler-macos --lib` — **1,693 passed / 0 failed / 3
  ignored**.
- The patch passed the whitespace check. Clippy was not run; the only Rust
  change was the `help_text()` string.

## Residual

- `AXZoomed` read error and absent attribute still both surface as `None`;
  distinguishing them requires a later diagnostic change (see above).
- No native/live legs were run; all native claims above quote existing
  recorded evidence only.
