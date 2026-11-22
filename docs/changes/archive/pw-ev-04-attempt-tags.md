# PW-EV-04: Structural attempt evidence

The [approved protocol review option](../protocol-correctness-review.md#pw-ev-04--make-attempt-purpose-and-native-call-status-structural) is an in-place active v4 cutover. It separates call outcome from no-call and gives frame evidence an explicit purpose. Accepted rows keep their meaning, including first correlation, replay, slot/visibility holds, and the revision-bound automatic maximize-clear state lane. Native creation and focus feedback remain separate; settlement `kind` and property-section decode order are unchanged. Windows/macOS typed callers are deferred.

The [wire contract](../../porting/world-contract.md#raw-attempts-and-settlement) records the full old-to-new mapping. The capture broker still checks the same stamps; its production clock validator did not need to change.

The Rust codec hard-refuses the fully retired frame form and quarantines an identifiable frame with valid disposition but malformed purpose. The baseline reader rejected a valid tagged frame and accepted the retired frame; the cutover reverses both results. The existing 13 canonical-attempt and 41 positive-close focus tests pass before and after. A KDE retention probe accepts the new called frame and focus no-call forms only after cutover, continues to reject the forbidden frame arm and the visibility token on the focus no-call arm, and no longer accepts the retired flat forms.

KDE producers, the retained raw evidence store, and strict report encoding now use the same tagged grammar. The established strict Rust report encoder refreshed active report witnesses, with counterfactual negatives derived at their manifest pointers. Five ordered scenarios changed only their attempt inputs. The KDE byte recorder updated three reports; the migration recorder produced no changes. The Stage5 recorder refreshed both chains, preserving the genuine source and clock correlations and using the existing generation normalization. Narrow report, ordered scenario, broker replay and KDE tests passed.

The integrated KDE run caught stale consumers of the flat frame and state fields in diagnostic traces and tests. These now read the tagged rows without changing trace semantics. Focus diagnostics retain their existing tokens independently of the new wire vocabulary. The targeted KDE rerun, including the same-lane controls and trace cases, passed 238 tests.

The integrated Rust run caught test-helper lint errors and a staging witness that carried `sticky:false` instead of `sticky:null`. The witness now isolates the unsupported native attempt from an unrelated sticky fact conflict. The two malformed float-purpose witnesses also preserve their single-window envelope and single-attempt scope.

The retained-intent journey uses a JSON-encoded input string rather than a nested report. Its genuine engine recorder updated the refusal attempt in the `clear` input. The other inputs and all six reply byte strings are unchanged; the unflagged replay passed.

Final offline gates passed: Rust format, scoped all-target Clippy, 3,578 scoped all-target Rust tests, the standalone raw evaluator test, the fresh offline evaluator build, and KDE typecheck and tests (1,487 passed; the optional native coordinate harness was skipped). No live KDE or deferred-platform run was performed.
