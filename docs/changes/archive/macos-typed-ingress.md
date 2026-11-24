# macOS typed world ingress (R3 / C5)

- **Status:** Complete (2026-11-24); offline gates clean.
- **Goal:** Remove the four-leg in-process JSON codec round-trip from macOS
  world dispatch (`encode_report_v4` → `decode_report_v4` →
  `handle_report_checked` → `encode_reply_v4` →
  `decode_reply_v4_for_host`) so the typed `WorldReportV4` enters core
  admission directly and the typed `WorldReplyV4` reaches the sequencer
  without re-encoding, while every gate the codec enforced stays enforced.
- **Source requirement:** [open-decisions review](../macos-v4-open-decisions-review.md)
  Part 5 (R3) and [core wire boundary](core-wire-boundary.md)
  ("macOS and Windows rebuilds: adopt typed ingress").

## Scope

- `crates/tiler-macos/src/world_dispatch.rs`: replace `codec_transact` with
  a typed transact over `tiler_protocol::world_admission::handle_report_checked`
  (the same production ingress Windows already uses), delete the codec legs,
  and remove the now-dead codec-refusal branch at both dispatch sites.
- `crates/tiler-macos/src/stage_timing.rs`: delete the four codec-leg stage
  constants and the codec-leg buffering (`LegClock`, `stage_leg`,
  `flush_legs`); record the `timing-core` leg directly at the dispatch sites.
- macOS test call sites/comments that named the codec as the production path
  (`runtime_thread.rs`, `world_command_map.rs`).
- `crates/tiler-protocol/src/world_admission.rs` (test module only): a
  retained-gate regression for `roster-host-mismatch` on typed ingress.
- Update the review record's R3 status and archive this note with evidence.

## Non-goals

- No core or protocol production change: the typed validators already exist
  and already run inside core admission (see decision below).
- No change to observer/clock semantics, sequencer acceptance, intent
  reconciliation, Mission Control, or any native actuation.
- No change to the broker/live-fixture wire ingress, which still decodes
  genuine wire bytes through the codec.

## Design (material decision)

`handle_report_checked` is the typed equivalent of the removed codec legs.
For the macOS report it:
1. applies `ProtocolAdmissionGate`: roster mismatch (only when typed residual
   carries roster feedback), exact `actionOutputs` cap, and the canonical
   request byte estimate against `WORLD_MAX_REQUEST_BYTES` (the same gate
   Windows typed ingress uses; `engine_twins_report_v4` asserts the estimate
   covers the canonical encoding of every checked fixture, so it cannot
   admit an over-bound fixture);
2. runs core grading (`graded_effective_v4`) and
   `validate_report_v4_reason`, the typed twin of the decoder's wire-shape,
   row-identity/order, section-cap, held-ordinal, creation/attempt,
   focus-feedback, presentation, and intent-record checks;
3. enforces the native/arrival host gates and generation/replay/staleness
   classification (`superseded`, `reused`, `duplicate`) before any mutation;
4. validates the typed reply through `validate_reply_v4` against the
   handshake-bound capabilities — `macos_capabilities()` is sent in the
   handshake, so `above`/`cursor_warp`/`native_sticky` rows refuse with the
   same fixed `snapshot-invalid` tokens (`above-capability-false`,
   `warp-capability-false`, `sticky-capability-false`) and the exact
   canonical reply byte count is enforced by `count_reply_v4_bytes`.

Wire-only arms that have no typed analogue (required key presence,
`cursorHit`/`gestureIngress` absence spellings, `monotonicMs` presence,
creation-receipt `null`/absence spelling, roster feedback) carry the same
information in the typed fields (`Option`, zero clock, `created_native`)
and the D3=B semantic gates read those typed fields unchanged. The macOS
production report never carries held rows or roster feedback, so there is
no residual evidence to lose.

## Refusal surface

Removed: only the codec-internal failures `codec-report-encode`,
`codec-report-decode`, `codec-reply-encode`, and `codec-reply-decode` (they
became unreachable once the typed path stopped encoding; no test or caller
asserted them). Their former cases now surface through the frozen core
tokens: an over-bound request as `oversized`/`request exceeds size bound`
(the gate's `Refuse` before admission), an over-bound reply as
`reply-oversize`/`reply exceeds size bound`, and a shape-invalid report as
`validate_report_v4_reason`'s existing fixed token (`snapshot-invalid` and
friends) instead of a decoder error.

Unchanged: `snapshot-invalid`, `section-over-cap`, `roster-host-mismatch`,
`above-capability-false`, `warp-capability-false`,
`sticky-capability-false`, `request-reused`, `request-superseded`,
`observation-stale`, and every D3=B semantic refusal. No refusal became an
acceptance.

## Units

1. **Typed ingress cutover** (complex): production and reference changes
   above, plus regression tests proving the typed path is used and the
   retained gates still refuse (typed-invalid/over-cap report, duplicate /
   reused / superseded request sequence, core capability gate binding and
   fixed tokens).
2. **Roster retained-gate test** (simple, protocol test module): typed
   admission still refuses roster feedback on a host without shared-roster
   support.

## Verification

- `cargo test -p tiler-core`
- `cargo test -p tiler-protocol`
- `cargo test -p tiler-macos --lib`
- `cargo test -p tiler-macos --test integration`
- `cargo fmt --all -- --check`
- `cargo clippy -p tiler-core -p tiler-protocol -p tiler-macos --all-targets -- -D warnings`

## Evidence (2026-11-24, offline)

- `cargo test -p tiler-core`: all suites pass (33.5 s).
- `cargo test -p tiler-protocol --offline`: 390 passed, 0 failed across the
  lib, 27 integration targets, and doctests; exit 0.
- `cargo test -p tiler-macos --lib --offline`: 1689 passed, 0 failed,
  3 ignored (was 1687 before the two new regressions).
- `cargo test -p tiler-macos --test integration`: 151 passed, 0 failed.
  Note: one earlier full-suite run executed concurrently with the protocol
  suite failed `native_focus_walk_retains_at_edge_per_tier1` with a stale
  observation count; the test passed 3/3 alone and the full suite passed
  151/0 on an isolated rerun with no code change. The test is a real-time
  focus-walk harness and was not modified.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p tiler-core -p tiler-protocol -p tiler-macos
  --all-targets --offline -- -D warnings`: clean.

Regressions added: `typed_ingress_refuses_invalid_and_over_cap_typed_reports`
(typed shape-invalid report and over-bound request both refuse before any
stage consumption), `typed_ingress_classifies_duplicate_reused_and_superseded`
(replay classification on the typed path), and
`capability_truth_at_typed_reply_gate_refuses_host_unsupported_rows`
(rewritten from the reply-decode test: core `validate_reply_v4` refusals with
the fixed capability tokens, plus equality of the handshake-bound
`macos_capabilities` with the core capability set, so no second policy layer
exists). Protocol test module:
`world_admission::tests::typed_admission_refuses_roster_feedback_on_non_roster_host`.
The typed path is otherwise covered by the unchanged dispatch, D3=B, and
integration suites.
