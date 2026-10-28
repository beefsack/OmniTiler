# Windows shared window-policy adoption

## Mapping and mismatch risks

| Windows rule | Shared owner or retained native responsibility |
| --- | --- |
| `tiling/classification.rs::classify` | Already normalizes `WindowFacts` into core `classify_geometry`; ordered refusals map back to `SkipReason` (`privileged` becomes `elevated`). |
| `tiling/float.rs::classify_focus` | Already calls core `classify_focus`; overlay-independent admission safety uses this same entry point. |
| Overlay and float/sticky/orientation refusals | `admission.rs` and `chord_resolution.rs` already call core overlay/toggle predicates. |
| Born-fullscreen hold, slotless maximized admission, born-overlay latch | `restart.rs` and `settings.rs` already call `should_hold_born_fullscreen`, `should_admit_slotless_maximized`, and `born_keep_latch`. |
| One-shot admission maximize clear | Already calls `should_clear_maximize_at_admission_with_mode` with retained lifetime latches, not the current setting. |
| Fixed-size classification and first fullscreen exit | Shared size-hint predicate and core world-born lifecycle; adapter retains native query budget/failure handling and consumes committed results. |
| Styles, owner/class, cloak, process/session/integrity, shell, fullscreen coverage | Platform fact acquisition, stays. Identity/scope/proof, UIPI, game coexistence and native write/recovery fences stay native. |
| Report-row authority, native restore sequencing and marker codecs | No equivalent stateless predicate: these bind shared verdicts to native lifetime/ownership and actuation evidence, not a second classification policy. |

- The approved migration is already implemented: see [world-model Stage 1](core-world-model-rearchitecture.md#stage-1-accepted-outcome-2026-10-21) and the [normalized-fact approval](../../decisions.md).
- `classify(...).is_ok()` is already a core policy call, not a local duplicate;
  replacing it with a boolean alias adds no ownership value.
- Windows dialog/owned-dialog exclusions must not become transient floats.
  Retained membership and fresh geometry eligibility are different contracts.
- Live born settings must not replace lifetime-bound born/retile overlay latches.
  Slotless maximized admission must not gain eager slot seeding.
- B9 native unfloat clear sequencing, directional sticky handling and focus
  fences are action routing, not missing classification/admission extraction.
  No new action behavior is selected here.

## Goal, scope and acceptance

- Confirm Windows uses the approved shared classification/admission owner;
  preserve all existing outcomes and conformance vectors.
- Add exhaustive adapter-normalization equivalence coverage against an
  independent pre-extraction ordered oracle and shared eligibility predicates.
- Document the reusable adoption boundary in the adapter guide. No core,
  protocol, KDE/macOS adapter, fixture-vector, native-acquisition or actuation
  changes are needed.
- Preserve fullscreen/game, elevation/UIPI and unmanageable-class safety.

## Bounded work and verification

1. Audit current callers against the approved migration (complete).
2. Add exhaustive Windows equivalence coverage and run locked core/protocol/
   Windows tests, strict all-target clippy, workspace format and diff checks.
3. Independent review, resolve concrete findings, record evidence and archive.

- Commands: `mise exec -- cargo test --locked -p tiler-core -p tiler-protocol -p tiler-windows`;
  `mise exec -- cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-windows --all-targets -- -D warnings`;
  `mise exec -- cargo fmt --all -- --check`; `git diff --check`.
- Live checks are unnecessary for a test/documentation-only confirmation of
  unchanged runtime code; existing native evidence remains historical, not a
  new live acceptance claim.
- No new product decision or behavioral ambiguity identified.

## Closing outcome (2026-10-28)

- The backlog adoption requirement was already satisfied by the prior runtime
  migration. No runtime rewiring or conformance-vector changes were needed.
- Added `windows_classifier_normalization_exhaustive_matches_oracle_and_core`
  in `crates/tiler-windows/tests/tiling.rs`: all 512 safety shapes crossed with
  four overlay combinations pin exact Windows geometry/focus refusal order
  against independent ordered oracles and shared geometry/focus/admission-clear
  boolean predicates. Native-to-neutral field names are exercised directly.
- All four commands above passed on the completed test change; Windows tiling
  integration tests report 107 passed. The full locked suite has no failures
  and one existing ignored test. The fixture-projection test still passes with
  unchanged shared vectors.
- Independent source review found no missing in-scope classification/admission
  extraction or test/documentation findings, and independently passed the new
  regression plus shared-fixture projection, format and whitespace checks.
- [Adapter guide](../../porting/world-adapter-guide.md#classification-ingress-and-remote-transport-rough-edge)
  now explains the reusable predicate/normalization boundary and latch-versus-
  setting risk. Native acquisition/actuation, game/fullscreen, elevation/UIPI
  and class exclusions are untouched. No KDE/macOS adapter changes.
- Live skipped: runtime code is unchanged; no new native acceptance is claimed.
  No new decisions or parked behavior questions. Adoption is complete.
