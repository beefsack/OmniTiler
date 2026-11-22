# PW-BR-02: broker capture facts

[Protocol review](../protocol-correctness-review.md#pw-br-02--the-capture-embeds-two-protocol-versions-and-documents-fields-the-broker-reject), option 2. Amend pre-release v4 in place: the bound capture envelope alone carries `v`. Its `report` member holds unversioned capture facts, without broker-owned stamp keys. The broker builds the v4 report and stamps it before the existing standalone report decoder. The standalone versioned report, raw evaluator route and their active fixture bytes stay unchanged; keep frozen fixture bytes and defer native typed port changes.

The cutover covers the broker, KDE producer and harnesses, checked route tests, existing active capture recordings, and the contract example. Preserve the strict envelope shape. For that shape an unsupported outer version fails before inspecting `report`; on a valid outer binding, any nested `v` or broker-owned stamp is a bound `unknown-field`, even when both kinds of key are present. There is no compatibility path.

Acceptance: observe legacy rejection and the facts-only transition with broker stamps; pin refusal precedence and correlation; exercise the real KDE-to-broker route; record active capture vectors with existing recorders only; prove standalone and frozen fixture preservation. Verify Rust format, lint, selected workspace tests and raw evaluator, then KDE typecheck and tests with freshly built evaluator examples.

The broker regression failed before the cutover on legacy nested `v` acceptance and passed afterward. The real KDE Stage 5 bridge matches the new facts-only envelope fixture, through the production broker. Existing Stage 5, KDE migration, and KDE byte recorders regenerated 19 Stage 5 envelopes and their bound reply siblings plus seven KDE capture requests. Stage 5 re-recording changed source correlations, their reply echoes and some clock samples, but not projected contents. Frozen fixture comparison against `ff6eaddc` and standalone active report comparison against the pre-cutover baseline both passed.

Offline verification passed: Rust format and Clippy, 3,587 selected Rust tests, the raw evaluator example test, offline builds of both evaluator examples, KDE typecheck, 1,488 KDE tests (one skipped), and the whitespace check. Native platform acceptance remains separate.

Independent review found no production issue. The service-route test covers nested `v` and both command stamps individually, and the KDE harness vectors use the facts-only shape.
