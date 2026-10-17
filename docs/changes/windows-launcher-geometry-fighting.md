# Windows launcher geometry fighting

Status: user-approved Windows fix and diagnostics delivered offline;
four-launcher physical acceptance pending. Reported and corrected 2026-10-16.

## Goal and scope

- Converge without geometry fighting when Steam, Battle.net, Epic Games
  Launcher and Xbox share a tiled workspace. Preserve ordinary-app tiling.
- Reuse the same live window's last successful declared hints on query timeout,
  with no expiry; never learn limits from geometry or add app-specific rules.
- Acceptance: a regression reproduces the log-derived hint outcome sequence
  with explicitly synthetic geometry; the four-launcher physical journey
  settles without continued writes, and Notepad/Paint still redistribute
  feasible minimums correctly.
- Windows adapter only. Shared-core/KDE/macOS behavior, admission-only policy,
  geometry tolerance and selected overflow behavior remain unchanged.

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

## Why pre-fix convergence could miss this sequence

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

## Approved decision and delivery

- [Placement](../decisions.md#placement-minimums-and-startup-adoption) already
  selects B6: keep slots, use tile origin plus declared minimum, permit last-
  resort overlap/overflow, no automatic float or alternative-axis optimizer.
  The incident does not establish whether these launchers were infeasible.
- User 2026-10-16 correction replaces the earlier timeout-means-no-hint rule:
  retain last-good declared hints with **no time bound**, replace only on the
  next successful query (including a successful no-hint result), and drop on
  close/identity change. No timer or expiry. Failed/unperformed/budget-skipped
  queries remain unknown without erasing the retained successful state.
- [Fixed-size admission D2](../decisions.md#fixed-size-admission) is
  admission-only classification, not a ban on reacting to self-resizes or
  ordinary minimum changes. No min=max evidence implicates fixed admission.

- `tiling_sys/hints.rs` stores last-good state separately from `hint_logged`.
  The full member key, opaque token and verified live lifetime tag fence reuse;
  a recycled HWND or new member cannot inherit. The timestamp measures age
  for diagnostics only. Domain release and float toggles preserve a live
  member's last-good state. Visible and verified-hidden fresh-query rows use
  the same fallback; the existing retained-overlay exception remains intact.
- `minmax.rs` distinguishes successful-empty `NoHint` (clears the old hint)
  from unperformed `Invalid` (preserves it). Usable successful queries replace
  immediately. Query timeouts/aggregate budget and native write gates are
  unchanged; fallback adds no wait, retry or query.
- `tests_timeout_fallback.rs` replays Hint -> Timeout -> Timeout -> Timeout ->
  same Hint -> Timeout -> same Hint through real row assembly and Engine
  projection, with synthetic minimum/frame values. Before the fix a 1000px
  declared minimum projected four widths as 1000/298/299/299, then loss of the
  hint projected 474/474/474/474. After the fix targets stay identical through
  fallback; a fresh changed minimum still reallocates. This is a log-derived
  outcome fixture, not a pixel-exact reconstruction of the incident.
- Regression coverage includes hours-old reuse (no expiry), fresh successful
  empty clearing, unsuccessful query preservation, hidden rows, close,
  same-process HWND reuse, token/identity changes and domain release.
- The existing successful-but-clamped lane already covers successful setters
  with mismatched readback. The real `RefusedTracker` regression asserts that
  the identical pair stays suppressed across fallback, a genuine new target
  re-arms and a match clears it. No broader suppression/backoff change is
  justified; write/readback facts in the diagnostic fixture are scripted,
  not live `apply_geometry` evidence.

- No automatic floating, learned client limits, persistent/app-wide floor,
  KDE/macOS convergence change or new overflow policy is implemented.
- [Minimal discriminator](../spec/reference-outcomes.md#windows-minimum-query-timeout-discriminator)
  records the corrected selection; exact native/reference outcomes remain TBD.

## Normal diagnostics and remaining evidence gaps

- The original normal diagnostics could not reconstruct this case:
  hint values/reasons, requested/effective/readback rectangles and per-window
  skip/setter outcomes are trace-only or absent. There is no token-to-class/
  process join. Aggregate `query_ms` cannot distinguish a slow response from
  a shortened remaining deadline or changing frame conversion.
- New normal `window-diag` entries join opaque token, class/executable
  basename, actual offered query timeout and native round-trip microseconds,
  raw outcome, carried min/max, fallback use/age/reason, planned/effective/
  requested outer rectangles, observed outer/insets, visible pre/post frames,
  write disposition and identical-clamp/transient-backoff/desired-changed/
  observed-changed reasons. Each entry keeps its own operation, correlation,
  tick and opaque domain identifiers, including when emitted later.
- At most one batch per second globally, at most 16 entries per batch;
  unchanged states stay quiet. Latest pending samples rotate fairly, including
  off-current-domain recoveries. `omitted` counts pending entries beyond the
  cap; `coalesced` counts unique tokens changed during throttling. This is
  explicitly sampled evidence, not an exhaustive every-poll trace. The old
  count-only `readback` line is subsumed; existing tick/min-hints summaries stay.
- JSON serialization and lifetime-fenced native class lookup happen only on
  emission; class is cached per member. No extra process opens, logging timers,
  message queries or waits. Timing values do not defeat stable-state dedupe.
  State is pruned with live membership. Idle/fullscreen cost is not physically
  measured by these offline tests; the physical gate remains necessary.
- Independent review corrected native-call timeout timing, native last-error
  capture before elapsed-time measurement, cached-class lifetime invalidation,
  capped-batch fairness and delayed-sample provenance/recovery.
- Existing trace still supplies every-operation `min-hints-detail`, `plan`,
  `write` and `readback-detail` when unsampled geometry is needed. Raw outer
  min/max and query-time DPI are not added; if conversion remains suspect,
  those need a focused diagnostic extension. The original incident's missing
  identity/rectangles cannot be recovered retrospectively.

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
   scoped `mise exec -- just dev trace` journey with `window-diag` enabled
   at normal level as well. Do not start games, downloads or change launcher
   settings.
4. With all four present, physically resize one split smaller and restore it;
   idle 10 seconds; switch away and back once. Capture each correlated query,
   plan, effective write and subsequent frame. Identify apps by class/basename
   only. If bouncing begins, capture at most 10 polling ticks before stopping
   through the established recovery route, rather than leaving it fighting.
5. Remove one launcher from that workspace and revisit the remaining three;
   then repeat Notepad/Paint control. The fix must stop unsolicited
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
  Final tests: 2,235 passed / 0 failed / 1 ignored (25 added tests).
- One full test run failed the unchanged input publication-fence callback
  assertion; four later failures were its poisoned test-lock cascade. The
  isolated test and two subsequent full four-package runs passed without a
  source change. Parallel test-global interference is suspected, not proven;
  no unrelated input behavior or synchronization was changed here.
- The standard test suite includes invisible own-process Win32 fixture-window
  creation/destruction in `tests/product_hide.rs::native_topmost_hide`.
  These gate results are not launcher or tiler physical evidence.
- Next: run the physical launcher/control plan on the updated build and inspect
  correlated sampled diagnostics plus trace. Keep this record active until
  physical acceptance. Any remaining delayed self-resize/move fighting is a
  separate evidenced follow-up, not a reason to silently broaden suppression.

## Second agent-live sweep (2026-10-17)

- Exact four-launcher journey skipped: the full required app set was not
  available within the approved disposable fixture. No installation, launcher
  state mutation or app-specific hint substitution was attempted.
- Supplement only: an ordinary Paint query naturally timed out and reused its
  same-lifetime declared minimum; the subsequent fresh query returned without
  changing planned/effective targets. This supports the reuse path, not full
  launcher convergence, physical resize or genuine changed-hint acceptance.
- Settings/SPI unchanged, scoped owner stopped and disposable fixture closed.
  Keep the four-launcher/control plan pending and this record active.
