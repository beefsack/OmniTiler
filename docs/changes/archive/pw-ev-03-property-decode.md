# PW-EV-03: Decode property attempts as one section

The [approved protocol review option](../protocol-correctness-review.md#pw-ev-03--decode-the-property-section-in-one-place) consolidates Rust v4 report decoding without changing any wire fields. Previously the codec validated frame, member and view rows, then creation, then state and visibility. It now validates all five property lanes before creation. Within each lane the first hard error still wins in producer order. The [wire contract](../../porting/world-contract.md#worldreport) documents the precedence. Wire replay order (PW-IN-05) is unchanged.

The paired tests returned `creation-id-invalid` on the original decoder and now return `duplicate-state-attempt` and `duplicate-visibility-attempt`, respectively. The standalone controls retain the original tokens. Review caught a change in the diagnostic sequence when both creation and visibility rows are held; the decoder preserves the original sequence and leaves the residual fields in lane/ordinal order for binary search. A mixed-held regression checks both residual lookups and the original property wire order through replay.

The four frozen families contain no `propertyAttempts` section. Independent review also checked current-shape tests that load or transplant frozen input and found no affected refusal expectations. All frozen paths match `ff6eaddc`; no active fixtures needed re-recording.

The scoped all-target Rust Clippy and test gates passed (3,581 tests), along with the standalone raw evaluator test, fresh offline raw and broker evaluator build, KWin typecheck, and KWin tests (1,487 passed, one optional test skipped). The format check passed after three test-only line wraps. The whitespace diff check passed. No live or deferred-platform verification was performed.

The intentional compatibility difference is limited to the first hard report-codec refusal when both creation and state or visibility are defective. The mixed-held case also keeps creation-before-visibility diagnostic order while consistently replaying both retained field sets.
