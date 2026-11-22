# Frame attempt replay identity (PW-EV-06)

## Goal and scope

Implemented the approved explicit-identity option in the [protocol review](../protocol-correctness-review.md#pw-ev-06--zero-clock-is-accepted-but-not-replay-identity). Active v4 frame-property evidence carries a required producer-assigned `attemptId` and a required nullable `attemptMs`. Timing remains evidence for timing policy, not an identity. Other attempt lanes and broker stamp-validation policy remain unchanged; PW-BR-04 owns alignment across lanes. Native port adoption is deferred. The frozen fixture families remain byte-identical.

## Approach and verification

Cut over the core, codecs, KDE producer and active fixtures together. Allocate increasing frame IDs once at recording, preserve them on redelivery, and use the existing per-window lifetime watermark to reject equal and older IDs even when no stamp is available. Keep the original timing with the row. Review the whole active route before publishing the contract outcome.

Offline verification passed: Rust format, scoped all-target Clippy, 3,583 scoped all-target Rust tests, the separate raw evaluator test, fresh offline raw and broker evaluator builds, KDE typecheck and 1,488 KDE tests (one optional test skipped), frozen fixture comparison against `ff6eaddc`, and the whitespace check. No live or deferred-platform verification was performed.

The first integrated run found a broker test input missing the required ID,
so it refused as malformed before reaching the future-clock check. The test
now supplies the ID without changing its expected clock refusal. Independent
review led to a compact watermark containing the last consumed ID and an
optional timed float self-echo target, with no second map or retained full
frame row. Two migrated pre-command settlement assertions now expect the
actual ID in their inputs.

On the old core, a baseline probe spent the clockless burst budget twice
instead of once. The old replay predicate also treated an exact same-stamp
row as a replay, and the old codec accepted canonical frame witnesses with
no ID. The replacement checks cover null-stamp A, retry A, equal-evidence B,
and late retry A at frame settlement; the codec refuses missing IDs and KDE
preserves IDs across retries and retires only the exact acknowledged ID.

The strict Rust report producer recorded two active report witnesses and
four ordered-scenario frame inputs; the scenario keeps its authored layout
and assertions. Three negative witnesses were derived by their manifest
mutations. The KDE byte recorder refreshed three active reports. The
Stage5 recorder refreshed both active capture/reply chains from the genuine
entry route, preserving matching nonces and clocks within each cohort.

## PW-BR-04 handoff

The broker capture path checks attempt clocks after strict decode, against the original property/creation JSON rather than only the surviving typed rows. Today it compares canonical stamps on frame, member, view, and creation rows to the authoritative dispatch clock, including identifiable held rows. State retains a stamp but is not checked; visibility has no stamp. Any frame comparison in a typed pass must skip `None` and preserve the original `Some` value (including zero). `attemptId` is not part of the comparison. A typed-only pass must deliberately account for held-row evidence and distinguish its effect from changing which whole captures are refused. Update the broker’s existing state/visibility control and add the bound-state future-stamp and held-row cases described in the review.
