# Windows launcher geometry fighting

Status: diagnosis complete, behavior fix blocked on evidence and timeout-policy
review; launcher physical acceptance pending. Reported 2026-10-16.

## Goal and scope

- Converge without geometry fighting when Steam, Battle.net, Epic Games
  Launcher and Xbox share a tiled workspace. Preserve ordinary-app tiling.
- Investigate minimum queries, native geometry and existing convergence before
  adding learning, retries or app-specific rules. No such rule is selected here.
- Acceptance: a regression reproduces the established request/observation
  sequence; the four-launcher physical journey settles without continued
  writes, and Notepad/Paint still redistribute feasible minimums correctly.
- This change records diagnosis only. No product code, shared-core behavior,
  admission predicate, geometry tolerance or selected overflow behavior changes.

## Evidence and timeline

- Source: the supplied non-trace incident log, 870 JSON lines. Its 118
  `min-hints` records include 112 with exactly `changed=[w27]`; 82 `readback`
  records and 217 `tick` records exist. There are no `plan`, `write`,
  `readback-detail` or `min-hints-detail` records.
- The log has no executable-basename or window-class identity strings.
  `pager-icons.class` and `.exe` are counters, not identities. Neither `w27`
  nor the other affected tokens can be assigned to a launcher. The application
  list above is the user's report, not log attribution. No titles were used.

| Log lines / ticks | Observed normal-level evidence |
| --- | --- |
| 29-33 / 3-4 | Seven queried members: `w27` is the only changed hint; usable hints fall to six with one timeout, one write and one mismatch, then return to seven with zero writes/mismatches. |
| 35-211 / subsequent early ticks | The same hint-present/hint-unknown transitions recur, interspersed with quiet ticks. This is not a transition on every poll. |
| 290-432 / 113-177 | Four-member workspace; the same hint flap now correlates with full-domain rewrites. Tick 127 writes four with one mismatch; tick 128 writes none but still mismatches; tick 129 writes four again without another logged hint transition; tick 130 restores the hint, writes four and matches. Ticks 133/134, 136/137 and 139/140 repeat four writes on each side of the transition. |
| 449-721 / 186-309 | Three queried members; repeated `w27` hint flap, commonly one write/one mismatch on timeout followed by zero writes/mismatches on hint recovery. |
| 737-746 / 315-319 | The visible membership/workspace context changes; later summary reports zero writes/mismatches. This does not prove convergence in the original four-launcher arrangement. |

- High confidence: a usable hint for `w27` repeatedly becomes unknown while
  aggregate timeout count rises by one, then returns when the timeout count
  drops. `changed` compares hint values, not query outcomes; attribution joins
  that sole changed token to same-operation counts, not a direct per-token
  timeout record. The repeated four-member examples have zero failed, invalid
  and budget-skipped queries. A shortened per-query remaining deadline is
  still possible; zero budget-skips does not prove a full 10ms wait was offered.
- High confidence: the hint flap and repeated writes are real. Strong causal
  hypothesis: removing/reinstating the hint alternates minimum-aware projection
  and/or native effective targets, creating a feedback loop with client-held
  geometry. Exact target changes and the tick-128/129 re-arm are not provable
  without rectangles. An asynchronous client resize or frame change could also
  re-arm the tracker between polls.
- Unknown: requested versus reported dimensions/positions, actual min/max,
  setter outcomes per member, outer/visible insets and query latency per member.
  Minimum refusal, self-resize, self-move, DPI/frame rounding and delayed
  `WM_GETMINMAXINFO` completion cannot be separated. Chromium/CEF/Electron,
  WinUI or custom-chrome attribution is unsupported, not a diagnosis.

## Why current convergence can miss this sequence

- `tiling_sys/minmax.rs::query_outer_tracks` requests `WM_GETMINMAXINFO` waits
  of at most 10ms and the remaining aggregate deadline; scheduling/native work
  is not a hard-real-time bound. Timeout returns unknown;
  `min_hint_for` and `rows.rs::assemble_domain_rows` pass no hint to the Engine.
- `rows.rs::log_min_hint_summary` also removes the last logged hint on unknown.
  Its cache is not a last-success fallback: it is synchronized before native
  application and supplies the same current hint to overconstrained targets.
- `tiler-core/src/boundary.rs::project_retained_tiled_geometry` projects from
  current hints. A hint transition can change several siblings' rectangles
  without changing retained shares. Clamp acceptance requires a meaningful
  hint on the differing axis; unhinted drift is not accepted. The 2px
  size-hint tolerance does not accept position drift or stabilize changing
  targets. No measured incident delta establishes a rounding-only cause.
- `tiling/reconcile.rs::RefusedTracker` suppresses only an identical
  `(desired, observed)` pair. Either changing re-arms writes. A matching
  readback clears the suppression. Failed setters alone use 500ms-to-5s
  backoff; a successful setter with a different readback uses the clamp lane,
  not backoff. Aggregate mismatch does not identify which lane occurred.
- `tiling_sys/geometry.rs::apply_geometry` compares exact effective targets,
  using origin+minimum for overconstrained writable members. A stable refused
  pair can settle; a moving target or delayed native observation defeats that
  narrow suppression. The original log cannot prove which changed each time.
- KDE's bounded stable-drift acceptance and own-mover tolerances in
  `kwin/src/plan-adapter.ts` do not govern Windows. Own-mover tolerance concerns
  post-transfer scope observations, not general client resize drift. Render
  overlays do not supply Windows native geometry convergence policy.
- [Oscillation verification](../live-oscillation-verification.md) concerns
  KWin trailing-empty desktop creation/removal, not this geometry loop.
  [Reliability investigation](reliability-condition-investigation.md) describes
  KDE projection/reassert isolation, not a Windows loop-breaking guarantee.
  [Learned limits](learned-size-limits.md) remain parked, not implemented.

## Existing decisions and unselected options

- [Placement](../decisions.md#placement-minimums-and-startup-adoption) already
  selects B6: keep slots, use tile origin plus declared minimum, permit last-
  resort overlap/overflow, no automatic float or alternative-axis optimizer.
  The incident does not establish whether these launchers were infeasible.
- The same decision explicitly says failed/timed-out/invalid Windows queries
  supply no hint. Last-known hints are selected only for retained tiled
  maximized/fullscreen overlays. Extending that exception to normal visible
  timeout queries is a behavior choice, not a silent cache repair.
- [Fixed-size admission D2](../decisions.md#fixed-size-admission) is
  admission-only classification, not a ban on reacting to self-resizes or
  ordinary minimum changes. No min=max evidence implicates fixed admission.

| Option | Consequence / recommendation |
| --- | --- |
| Bounded last-good declared hint on a visible member's timeout | Recommended for review after geometry confirmation: preserve allocation through transient missing evidence, without inferring a limit from refused geometry. Bound by the exact live member, replace on fresh usable evidence, clear on membership loss, define expiry and unknown-result handling explicitly. Changes the current timeout rule; not selected. |
| Defer affected-domain geometry while a required query is unknown | Avoids projecting with a suddenly missing hint, but delays sibling layout/actions behind one slow app. No indefinite freeze or retry policy is selected. |
| Broader geometry oscillation suppression or longer query waits | Additional state may suppress legitimate new intents; longer waits trade owner responsiveness for query success. Neither is justified by this log alone. |

- No automatic floating, learned client limits, persistent/app-wide floor,
  KDE/macOS convergence change or new overflow policy is implemented.
- [Minimal discriminator](../spec/reference-outcomes.md#windows-minimum-query-timeout-discriminator)
  leaves unsupported outcomes TBD. A replay with invented rectangles would
  demonstrate a hypothesis, not reproduce this incident; no such regression
  is claimed.

## Observability gap and next evidence

- Normal diagnostics violate end-to-end reconstructability for this case:
  hint values/reasons, requested/effective/readback rectangles and per-window
  skip/setter outcomes are trace-only or absent. There is no token-to-class/
  process join. Aggregate `query_ms` cannot distinguish a slow response from
  a shortened remaining deadline or changing frame conversion.
- Before a definitive replay, capture correlated token, class and executable
  basename (never titles/full paths), per-query outcome/offered timeout/elapsed
  time, raw outer min/max, DPI and measured insets, converted hint, planned
  rectangle, effective visible and requested outer rectangles, setter status,
  post-write and next-poll visible rectangles, and clamp/backoff/skip reason.
- Existing trace supplies `min-hints-detail` (minimum and reason), `plan`,
  `write` (effective desired) and `readback-detail`. It does not supply all
  identity, maximum, native conversion, timing or skip details above. A trace
  alone cannot close those remaining gaps. Existing read-only `inventory`
  supplies class/basename, but has no opaque-token join. Do not enable the
  owned-helper-only proof/audit path on launchers to obtain that join.
- Small diagnostic follow-up: enrich changed-hint and mismatched-readback
  summaries with their per-token facts, plus a lifetime-bound class/basename
  join and per-query timing/conversion facts. Reuse change/mismatch gates;
  steady state stays quiet. No user content or behavior changes.

## Physical check plan (pending)

1. Follow [the Windows guide](../live-windows-testing.md); use an identified
   build, recoverable baseline and executable scope for only the test apps.
   Keep games, elevated/protected apps and unrelated windows outside the run.
   Keep baselines outside repository records; never collect titles/content.
2. With disposable documents, tile Notepad and Paint alone. Let them settle
   for 10 seconds. Physically resize shares through a feasible Paint minimum
   and back; verify minimum redistribution, unchanged focus and quiet idle.
3. In a separate tiled workspace open Steam, Battle.net, Epic Games Launcher
   and Xbox in that order, waiting 10 seconds after each. Run the user-owned
   scoped `mise exec -- just dev trace` journey with the additional diagnostics
   above. Do not start games, downloads or change launcher settings.
4. With all four present, physically resize one split smaller and restore it;
   idle 10 seconds; switch away and back once. Capture each correlated query,
   plan, effective write and subsequent frame. Identify apps by class/basename
   only. If bouncing begins, capture at most 10 polling ticks before stopping
   through the established recovery route, rather than leaving it fighting.
5. Remove one launcher from that workspace and revisit the remaining three;
   then repeat Notepad/Paint control. A future fix must stop unsolicited
   repeated writes, preserve supported minimum placement and still react to
   genuine native resize/valid hint changes. Existing B6 overlap is not itself
   a failure; silent floating or unexpected focus/workspace changes are.
6. Stop using `mise exec -- just stop`; verify release, revealed windows and
   recovery state, then restore the scoped baseline. Record native outcomes
   separately from offline tests.

## Verification and remaining work

- Native four-package `cargo build --locked`, `cargo test --locked` and strict
  `cargo clippy --locked --all-targets -- -D warnings` pass through
  `mise exec --`; all-package fmt check passes. Toolchain: Rust/Cargo 1.99.0.
  Tests: 2,210 passed / 0 failed / 1 ignored. No new tests or behavior fix.
- The standard test suite includes invisible own-process Win32 fixture-window
  creation/destruction in `tests/product_hide.rs::native_topmost_hide`.
  These gate results are not launcher or tiler physical evidence.
- Next: review timeout fallback choice, add the missing bounded diagnostics,
  obtain the physical trace, then accept a sanitized geometry fixture before
  implementing a behavior fix. Keep this record active until physical
  acceptance; no decision is promoted by this diagnosis.
