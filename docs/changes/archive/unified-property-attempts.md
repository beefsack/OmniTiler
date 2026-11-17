# Unified property attempts (C4)

## Goal, scope and status

- Complete: behaviour-preserving core implementation and offline acceptance
  verified 2026-11-17; wire-shape unification remains an explicit C5 target.
- Replace frame, membership, native view, maximize-clear, native state and
  visibility evidence with one typed property attempt record and one settlement
  engine, keeping property policy above it. Creation stays separate.
- Authority: [C4 and neighbouring decisions](../core-v4-review.md#user-decisions-2026-11-12),
  [Simplicity](../../principles.md#simplicity), and
  [current settlement rules](../../decisions/contract.md#state-centric-contract).
- C3 precedes C4; cursor warp is already deferred to the Windows port, not
  redesigned here. C4 precedes Windows and the C1/C2/C5 restructuring. No model,
  command, retry, retained-outcome or persistence redesign is included.

## Pre-migration inventory

- Source locations below are in `crates/tiler-core/src/` unless qualified.
  Verified by source inspection on base `96ad5635` plus existing working changes;
  line numbers describe that tree, not the unchanged base.

| Evidence | Record and correlation | Settlement and policy to preserve |
| --- | --- | --- |
| Frame | `world_report.rs:814` `FrameAttempt`: window/target; optional float operation; immediate readback, DPI pair, optional prewrite and stamp. No publication revision. | `world_frames.rs:664` `settle_frames`, `:177` `consume_attempt`, `:272` `is_stale_replay`: clamp, transient backoff, burst budget and learned minima. Float completion separately consumes the same evidence in `world_settle.rs::settle_rest`. |
| Membership | `world_report.rs:855` `MemberAttempt`: exact operation/window/output/workspace, namespaced pre/post, disposition and stamp. | `world_settle.rs:31` matcher, `:404` target proof, `:693` `settle_rest`: window transfers and migration members, external-input adoption, partial arrival and predecessor protection. |
| Native view | `world_report.rs:889` `ViewAttempt`: operation/output/workspace, native pre/post/baseline and stamp. | `world_settle.rs:504` `settle_views_pre`; retained view/migration paths in `settle_rest`; `world_selection.rs:1629` `selection_native_proof`. Native-free selection needs no invented view attempt. |
| Maximize-clear | `world_report.rs:916` `ClearAttempt`: operation/window, pre/post maximized and stamp; target is implicitly false. | `world_clear.rs:192` `settle_clears` resumes directional/unfloat intent; `world_born.rs:548` `settle_born_clears` retires birth clear. These are distinct policies using one evidence shape, not ordinary maximize-state authorization. |
| Native state | `world_report.rs:946` `StateAttempt`: window/property/bool target/revision, pre/post and stamp. | `world_state.rs:149` `settle_states`, `:219` classifier: capability-gated fullscreen/maximize/sticky/above; binding revision through latest publication is the correlation interval. Metadata at `:25-88` owns property continuations. |
| Visibility | `world_report.rs:1720` `LastWrite`: enclosing window supplies subject; target/authorization revision, route/reason/disposition and identity-bound samples. No operation id or attempt stamp. | `world_selection.rs:1518` `settle_selection_switch`, `:1707` live/held extraction, `:2075` post proof: every frozen show/hide obligation needs its own verdict, latched against issuing-to-receipt observation bounds. |

- Adjacent, not interchangeable evidence: `ArrivalBracket`
  (`world_report.rs:999`, `world_selection.rs:2230` `prove_selection_arrival`)
  is readiness without a setter, not another write attempt. `FocusFeedback`
  (`world_report.rs:1016`) uses exact opaque id/target/issuing revision;
  `world_close_focus.rs:31,103` and `world_focus.rs:37` retain the single focus
  owner and its distinct retirement/classification. Selection consumes feedback
  for its independent completion (`world_selection.rs:1399`). Focus feedback
  remains separate retained-request evidence (User 2026-11-17).
- Creation: `world_report.rs:2072` `NativeCreationAttempt`,
  `world_native.rs:1509,1636` correlation/settlement. Preserve explicit creation
  authorization, causal created identity, before/after order and ambiguous
  changed-replay handling; do not route it through property settlement.
- Intake is currently split into observation vectors (`world_report.rs:1513-1535`)
  and window `last_write` (`:1974`), including raw held-row extraction.
  Codec examples: `crates/tiler-protocol/src/world_protocol.rs:1166,1203,1303,1324`;
  core sizing and protocol encoding must migrate with any wire decision.

## Implemented mechanism

- One canonical typed `PropertyAttempt`, not a bag of optional JSON values:
  subject (window or output), correlation, mechanical disposition, and a tagged
  evidence payload. The payload's tag is the property/attempt kind; do not store
  a second independent property tag or parallel old record.
- Correlation is an explicit operation or revision arm, or ordinary-frame
  target correlation without a minted identity. Preserve optional frame
  operation identity. Only state and visibility currently supply revisions:
  do not infer another from receipt time or add an id to visibility.
- Payloads carry target, readable-or-unknown pre/post and their necessary
  extensions: frame DPI, view baseline, visibility samples/route/reason.
  Preserve original `attemptMs` where supplied; visibility keeps sample
  watermarks, not a fabricated stamp. Clear's false target is structural.
  Closed typed arms prevent bool/frame/workspace confusion and retain namespace
  checks. This is a conceptual shape, not an approved wire schema.
- Share the call dispositions accepted/refused/threw; allow
  `already-at-target` only for membership and `not-called` only where presently
  supported. These are mechanical facts, not completion verdicts.
- One settlement entry path dispatches explicit ordered property phases:
  correlate evidence, retain/update it under that property's rule, evaluate
  proof against staged facts, then return the policy result. Property policy
  owns eligibility, external supersession, continuations and terminal output.
  Use a closed match, not a registry, plugin system or universal equality rule.
- Reuse the existing obligation owners and their required policy memory. A
  shared evidence-slot mechanism replaces duplicated intake/latch plumbing;
  do not add an overarching attempt-history store or merge the C3 terminal maps.
  Frame evidence may be borrowed by float completion and anti-fighting without
  treating either consumer as a new native attempt.
- Keep the current phase boundaries and domain rebuilds
  (`world_run_v4.rs:199-222`): native views first; directional and birth clears
  before member/float continuation; then state, selection feedback, switch
  completion and frame policy. One engine need not mean one commuting pass.
  Remove each replaced lane's intake/settlement plumbing as it migrates.

## Behaviour-preservation constraints

- No new authority, ids, acknowledgements, queues, polling, expiry or retry
  limits. Accepted setters alone never prove completion; unknown remains unknown.
  Existing frame convergence/backoff is not the at-most-once operation rule.
- Preserve actual intake differences, rather than assuming a universal latch:
  member and clear pending evidence can be replaced on later intake
  (`world_settle.rs:719-754`, `world_clear.rs:223-230`); state, selection visibility
  and focus latch first matches. Frames use a consumed watermark; stamp zero
  explicitly does not deduplicate (`world_frames.rs:254-276`). Receipt-bound
  visibility verdicts cannot be re-evaluated at later watermarks.
- Preserve capability, lifetime, generation, home/output, held/excluded,
  minimized, incomplete-inventory, gesture and fullscreen safety rules at their
  existing scope. Evidence captured before a hold must survive recovery.
- Preserve membership no-call proof: equal non-null raw pre/post plus translated
  target proof; consumption is not settlement. Retain predecessor history under
  the accepted [Option A](../../decisions/contract.md#state-centric-contract),
  including readable-source/target-proof clearing, with no cap or pruning.
- Core executes every queued command in order without waiting for native apply;
  platforms apply latest world. Keep retained terminals, focus precedence and
  desktop-foreground semantics unchanged.
- Do not generalize visibility's correlated failure accounting into state or
  transfer completion, or merge clear and state-maximize authorization merely
  because both carry booleans. Arrival/focus cannot replace missing show evidence.

## Material decisions

- User 2026-11-17, Q1=B: preserve existing wire locations in C4 and normalize
  directly into the single core record; per-property mapping stays at the codec
  boundary. Unifying the attempt wire shape is not dropped: it is an explicit
  deferred target of C5, moving the wire format out of core. The target is the
  most correct design, not a shortcut to reduce effort.
- User 2026-11-17, Q2=A: C4 covers the six listed property kinds; focus feedback
  stays separate retained-request evidence. Arrival brackets remain nested
  read-only evidence, and creation stays separate.
- User 2026-11-17 approved the design for behaviour-preserving implementation.
  No open design decision remains. Correlation matchers, phase ordering and
  retention are preservation requirements. Unsupported behavioural outcomes
  stay TBD; any discovered
  ambiguity gets a minimal discriminating row in `docs/spec/reference-outcomes.md`
  before a behavioural decision, not an incidental refactor fix.

## Migration and verification

1. Pin a before/after sequence corpus from existing genuine
   core/codec tests. Cover each correlation rule, malformed/held evidence,
   stale/same-stamp replay, lost feedback, gaps, close/replacement and supersession.
   Preserve existing wire bytes, effective per-kind limits and whole-message
   transactional budgeting.
2. Introduce the canonical record/engine with native-state as the first policy.
   Migrate clear/birth-clear next, then membership/view and their parent
   dependencies, then frames/float and visibility/selection. Focus remains
   separate. Remove old typed records and settlement plumbing in
   each unit; temporary migration adapters have an explicit removal in that unit,
   not a permanent compatibility layer.
3. Compare complete publications and retained state at each report, not only
   final geometry. Preserve same-report clear continuation, native rewind,
   migration partial arrival, frozen show/focus/hide ordering, current-latest
   no-call consumption and published predecessor history. Do not use new verdict
   expectations to make old sequences pass.
4. Run affected core/protocol and KDE tests per unit; at completion run scoped
   Rust build/format/Clippy/tests and KDE typecheck/tests. Existing anchors include
   `world_observe_capture_tests.rs:2155,2357,2671,4201,4564,4874,5440`,
   `world_state_attempt_tests.rs`, `world_selection_await_tests.rs:4977,5039`,
   and `tests/world_native_attempt_settlement.rs`. Update active wire fixtures
   only where approved; preserve frozen `v4` and both stage5 proofs.
5. Preserve the conservative unchanged-domain skip and its equivalence tests.
   Run `bash scripts/check-report-cost.sh` after the final hot-path change:
   existing ceilings are 24M/100M instructions at 50/200 windows and growth 6.5.
   Do not raise them to accommodate an accidental regression. Use the existing
   manual S1/S2/S3 release harness if costs change materially, not another gate.
   Native follow-up is targeted to changed producers; offline equivalence is not
   native acceptance.

## Risks and evidence limits

- Main risks: flattening distinct correlations/latches into false proofs;
  visibility lost from held rows; new global scans/clones on the report path;
  accidentally renewing spent effects or dropping predecessor protection.
  Typed payloads, ordered phases and sequence comparisons address these.
- The accepted membership history can grow without a finite bound; C4 preserves
  that tradeoff rather than introducing compression or retirement policy.
- Source/test locations were inspected, not executed for this design. The
  [cost record](core-per-report-cost.md#outcome-and-gate-verification)
  reports a passing gate and skip-equivalence checks; these are prior evidence,
  not reruns here.
- The latest [D2 native record](../kde-rapid-input-latest-world.md#d2-bounded-native-retry-2026-11-17)
  reports bounded ordered accounting/final convergence PASS. Literal live
  `already-at-target` bytes were not captured; genuine offline serialization
  covers that token. No new native claim or broader rapid-input acceptance.

## Implementation evidence

- Native-state capture and retained pending evidence now use the canonical
  record; correlation/intake moved to the common engine, with state proof
  policy unchanged. Wire-facing typed report rows remain boundary inputs until
  C5; they are not parallel settlement records.
- Pre-change settlement/state corpora pass 18/21 tests. After state migration,
  the same corpora pass, plus all 22 staging and 96 capture tests; core check
  is clean. Initial test compilation needed one causal assertion/accessor
  migration after the retained evidence type changed; no oracle changed.
- Maximize-clear and birth-clear now share canonical evidence, correlated
  replace-pending intake and preimage predicates, with unchanged policy order.
  The clear/birth baselines and migrated suites pass 33/63 tests; state, capture
  and staging checks pass 21/96/22. Four shared-engine tests cover kind isolation,
  retention rules and raw-fact preservation; a direct rerun passes all four.
- Review restored attempts-major state intake after the first extraction added
  a binding-by-attempt scan. State intake again uses one keyed lookup per
  attempt, preserving exact identity/revision/spent guards without a vector
  clone. Post-correction state/capture/clear/birth checks pass; core check is clean.
- Membership/view staged and retained evidence now use canonical arms, exact
  shared correlation and explicit first-match/replace-pending retention.
  Membership extends the shared call disposition with a structural no-call arm.
  Predecessor, migration, namespace and view-rewind policy remains unchanged.
- Membership/view baseline and migrated capture/staging/migration/selection
  suites pass 96/22/21/129 tests, with native-attempt/truthful-settlement
  integration suites passing 19/7. Review found a fresh-view search that stopped
  at the first unrelated view; a regression reproduced the failure before the
  correction and now passes. Final capture count is 97, full core library 2296;
  core check and test compilation pass. Direct regression and common-engine
  reruns pass 1/8 tests.
- Frame/float capture, retained pending and consumed watermarks now use canonical
  records with shared exact correlation and replay mechanics. Zero stamps,
  prior-watermark learning, DPI transitions and float-specific proof remain
  unchanged. Frame policy/fixture/float suites pass 66/3/12 before and after;
  capture/staging and native-attempt/truthful integrations also pass.
- Review removed an unnecessary float-lane clone and completed canonical retained
  slots rather than leaving a typed-payload exception for old test accessors.
  Corrected rejected/accepted and pipeline suites pass 17/30; common-engine
  direct rerun passes 10 tests. Core check, library test compilation and scoped
  formatting pass.
- Visibility now normalizes after capability grading into canonical attempts:
  healthy evidence takes precedence over the first decodable held row.
  Classification restages preserve the frozen collection, including reports
  with zero visibility evidence. Retained switch evidence keeps its receipt
  watermark and one-time verdict; focus and arrival remain separate.
- Review restored keyed visibility intake instead of a new member-by-attempt
  scan, removed normalization clones and integrated the explicit visibility
  phase at its original position. Capture/selection/staging/pipeline suites
  pass 100/129/22/30; native-attempt/truthful integrations pass 19/7.
  Fourteen common-engine tests pass, including a direct final rerun; core
  check, library test compilation and scoped formatting pass.
- All six property kinds use one staged collection and canonical retained
  records. Replaced raw settlement vectors, local intake and conversion-back
  paths are removed; property proof and continuation remain separate.

## Outcome and final acceptance

- Independent source review found no concrete correctness, contract or cost
  findings across all six migrations. Targeted state, frame, selection,
  staging, pipeline and genuine integration checks pass. This is offline
  preservation evidence, not new native-platform acceptance.
- Final Rust gates pass: `cargo fmt --all -- --check`;
  `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`;
  `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`;
  `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`.
  The final direct test rerun passes 3513 tests, with the one prescribed
  unrelated unsafe-posture check filtered.
- Initial strict Clippy found a stranded doc comment and an unnecessary lazy
  test-value construction. Both are corrected; direct format, strict Clippy
  and fourteen common-engine tests pass afterward. No behavior oracle changed.
- KDE `npm run typecheck` and `npm test` pass: 1378 tests, including genuine and
  recorded stage5 proofs. Launcher/native-effect shell suites pass 703/204;
  `node --test scripts/kde-live-helpers.test.mjs scripts/kde-live-session.test.mjs`
  passes 58 tests.
- `bash scripts/check-report-cost.sh` passes at 11,181,342.8 and 47,962,590.5
  instructions/report for 50/200 windows, growth 4.29. Existing 24M/100M
  ceilings and 6.5 growth bound are unchanged. Costs are effectively unchanged
  from the preceding cost record; no additional timing claim is made.
- Frozen `world-wire/v4`, stage5, stage5-v4 and kde-v4 fixtures are unchanged.
  Reconstructed pre-implementation baseline comparison confirms tracked
  changes stay within the C4 source scope. Added-line ASCII/privacy and
  whitespace checks pass, including the new canonical module and this record.
- C4 is complete. C5 retains the deferred unified attempt wire shape.
