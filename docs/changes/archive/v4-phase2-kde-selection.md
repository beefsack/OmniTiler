# V4 phase 2: KDE selection

## Goal and status

- Port workspace selection and its acknowledgement/correlation to the native
  KDE adapter while retaining K1 ordinary tiling. Accepted: direct-v4
  resize/orientation, native selector/readback, exact view-attempt correlation,
  focusless completion and bounded non-wedging populated-selection retention.
- Start from the [K1 closeout](v4-phase2-kde-port.md#k1-final-outcome).
  The [phase-1 note](../v4-phase1-core-protocol.md) remains active and unchanged.
- First repair resize and toggle-orientation emitters to use the existing direct
  v4 commands. The codec already accepts both; no codec change is required.

## Scope and non-goals

- Product decision: this slice covers native selector/readback, exact original
  view-attempt correlation and focusless terminal proof. Focus-bearing terminal
  completion stays in the arrival/focus-feedback slices, preserving their order.
  Core completion is unchanged; no acknowledgements or feedback are fabricated.
- Retain the authenticated v4 connection, native observation fences, opaque
  intent storage, core-owned policy and ordinary tile/reflow path.
- Arrival, focus feedback, state targets, scaled metrics, reconnect/rehome,
  visible active border and legacy removal remain separate later slices.
- Immutable wire and legacy fixtures remain unchanged. No C++ presentation DTO
  or script-side absolute border geometry change is planned.

## Approach and bounded units

1. Repair both resize/orientation emitter paths and add direct-v4 regressions.
2. Audit selection against the [contract](../../porting/world-contract.md), then
   implement only missing native selection and acknowledgement mechanics.
3. Run the full adapter and scoped Rust gates, then prove selection and continued
   ordinary tiling using fixture-scoped native state and correlated full wire.
4. Resolve one independent integrated review, record exact capabilities and
   remaining degradation, and archive only after acceptance.

## Acceptance and verification

- Direct resize/orientation payloads reach the genuine v4 core without a retired
  operation wrapper; preserve resize press accounting and existing safety gates.
- Selection reaches the intended native desktop with authoritative native
  observations and correlated view attempts/terminal events, not setter success.
- Ordinary fixture tiling and reflow continue across selection. No new skips.
- An open populated-selection binding must not block later selections, commands,
  capture/reply flow or tiling. Retention must remain bounded under repeated
  populated switches; missing/pending requester outcomes are reported exactly.
- Sequential declared-environment adapter typecheck/build/full tests and widened
  scoped Rust tests/build/clippy/formatting remain green; unsafe workspace tests,
  ignored forcing and legacy fixture generation are excluded.
- Native work follows the [live guide](../../live-kwin-testing.md) and
  [fixture reference](../../../crates/tiler-live-fixture/README.md): fresh occupied
  workspace inventory, fixture-only filter, evaluated readiness, independent
  fixture/native joins and complete correlated request/reply capture.

## Evidence and open limits

- Both entry and shortcut emitters now send direct `resize` and
  `toggle-orientation`, already supported by `decode_command_v4`. Strict
  TypeScript and the full adapter build/test gate pass: 984 tests, no failures
  or skips. The genuine-core carrier observes 12px then 14px boundary changes
  and an orientation flip through production callbacks, queue and real engine.
  These are native-shaped offline readbacks, not live geometry proof.
- K1 remains the baseline except where the native proof and final capability
  list below supersede its selection and resize/orientation limitations.

## Selection dependency

- Source audit finds the native selector path and native-only view attempts
  already connected: `world-apply.ts` spends retained view operations before
  `switchView`, records native pre/post/base, and the v4 producer publishes them.
  Core latches the exact parent/output/workspace tuple in
  `settle_selection_feedback` and checks authoritative native echo or raw attempt
  proof in `selection_native_proof`. Native evidence is recorded below.
- Focusless native selections can terminate through the existing shared
  predicate with `focus:unproven`. Focus-bearing selections cannot yet terminate:
  `selection_terminal` requires correlated focus feedback when an effect exists,
  while the KDE v4 producer currently emits an empty `focusFeedback` lane.
  Proven focus completion also requires arrival readiness. Both ports belong to
  later slices; no synthetic success, changed core predicate or local settlement
  is permitted here.
- The accepted scope retains populated-workspace focus settlement as deferred;
  native switching and bounded non-wedging retention are verified separately.

## Open-binding evidence

- Core source replaces the prior view flight per output in `retain_view_inner`,
  prunes overlapping selection bindings on changed selection, and stores at
  most one selection binding per focus output. Native target/raw maps are
  bounded by the affected outputs; supersession silently drops the old parent.
- A new direct-v4 core regression passes four alternating populated switches,
  each with a fresh prior native echo and native-tagged attempt. It verifies one
  retained binding/scope/view flight, empty feedback and terminal lanes, fresh
  command correlation, subsequent observation acceptance and a real orientation
  frame change while the final focus-bearing binding remains open.
- Initial harness failures used a stale native current (making a selection
  redundant), then a non-focused orientation subject (legitimate refusal).
  Coherent native echoes and the actual focused subject correct those inputs;
  no product behavior or completion rule changed.
- Widened scoped Rust tests pass with the new regression and both stage5 proofs
  enabled. Actual private-bus ownership, opaque storage, queue/take and wait-wake
  outcomes execute without skip diagnostics. Scoped build, strict all-target
  clippy and formatting pass. The final adapter typecheck and build/full test
  gate also pass: 984 tests, no skips. Both stage5 proofs remain enabled.
- Degraded protocol outcome: accepted select replies can carry `result:null`
  and `switchSettled:[]` indefinitely while their focus-bearing binding is open.
  The empty terminal lane is not a failed terminal or a selection-success
  receipt. A superseded open parent emits no terminal. Queue/take readiness is
  report-result delivery, not switch completion.

## Native evidence

- The source-built service and freshly built exact-class fixture-scoped script
  ran without loading the optional effect. A fresh native inventory counted
  all-desktop occupancy; the accepted leg used three appended desktops beyond
  the highest occupied ordinal. Five fixture windows joined one-to-one by
  sentinel title, process identity and independent native window UUID.
- Complete service traffic was filtered by exact destination/serving sender and
  joined by caller identity and reply cookie: 76 call/reply pairs, with one parked
  wait at teardown. An explicit positive-marker call and its real reply are
  present. Intent seed reads, discovery, accepted revisioned settings and all 18
  ready capture/reply pairs correlate with native-PID readiness, dispatch and
  applied journal evidence. Adapter/generation, source/request echo and native
  invalidation are checked independently; source sequence is not a native clock.
- Populated A-to-B, B-to-A and A-to-B selections each reached the authoritative
  native current desktop. Their exact original parent/output/logical workspace
  view attempts carried native pre/post/baseline references and unchanged attempt
  stamps. Each populated grant carried its own command sequence and
  `awaitShown:true`; all focus-feedback and terminal lanes stayed empty.
- After the first open populated selection, adding a third fixture to the
  selected workspace produced a three-window layout; closing it restored the
  pair. A later direct resize moved the shared native boundary by 12 logical
  pixels; direct orientation changed the pair to a vertical stack. Capture/reply
  flow and the later populated selections continued without a wedge.
- Selecting the managed-empty desktop then produced exactly one correlated
  terminal: `op-6`, `outcome:complete`, `focus:unproven`, empty failed set and
  `fallback:false`. Native current and the following accepted inventory agree;
  no managed member or focus effect existed on that target. Native all-desktop
  surfaces still exist, so this is not a claim of zero native occupants.
- All six queued commands returned exact matching `globalCmd` values and later
  `TakeCommandResult` outcomes `ready` with `result:null`. Only the focusless
  switch emitted a terminal. Queued commands prove native actuation, not physical
  shortcut delivery; production callbacks are covered by the offline carrier.
- Independent native frame readbacks agree with accepted core targets under the
  producer's existing logical-pixel rounding; client geometry was also retained.
  Fractional native sizes
  differ by 0.4 logical units in three-window/oriented layouts. Final fixture
  socket size readback was not collected; no exact arrival or scaled-metric
  acceptance is claimed. Retained cardinalities are proved by the offline
  regression, not private-state introspection in the live service.
- Native-PID journal inspection finds no runtime exception. It does retain Qt
  used-before-declaration warnings, presentation stale-coverage diagnostics,
  degraded corrupt intent seeds and absent optional effect/gesture/tray D-Bus
  endpoint errors. These did not interrupt tiling; this is not noise-free or new
  nonempty restart-hydration proof.
- Exact controller/helpers were unloaded, the fixture and development service
  exited, its socket disappeared, the original native desktop was restored and
  development status was down. The three appended desktops and persisted project
  shortcut records remain ordinary lifecycle residue. The generated bundle was
  replaced by the normal unscoped, non-trace build after testing.

## Final outcome

- Independent integrated source/evidence review found no blocking code defect.
  Its stale-status and discriminator-row findings are corrected in this closeout.
- Final changed-file whitespace, added ASCII, full-content NUL and relative
  Markdown target checks pass. Immutable v4 wire and legacy fixtures are
  unchanged. No ignored-test forcing, legacy generator or whole-workspace pass
  is claimed. Phase 1 remains active; this accepts only the selection slice.

### Can

- Switch the tested native selector between populated managed desktops, continue
  ordinary admission/reflow and commands, correlate original view attempts and
  emit the existing focusless completion on a managed-empty target.
- Use direct-v4 resize/orientation through production callbacks and the genuine
  core; observe native resize/orientation through the queue. Repeated populated
  supersession retains bounded state in the direct-v4 core regression.
- Retain the separately accepted K1 connection, ordinary tiling, recovery,
  opaque persistence, own-process exclusion and hidden-presentation capabilities.

### Cannot claim

- Focus-bearing terminal completion, arrival readiness or causal focus proof;
  native state-target, scaled-metric, physical reconnect/rehome, multi-output or
  physical-key/input acceptance. Those remain later slices or separate checks.
- A visible active border, native group-underlay/drag-preview rendering or pager
  surface. The border remains off until focus feedback; no geometry workaround
  is introduced. K1's other unproved native cases remain unproved.

### Degraded and simplification boundary

- A populated selection may visibly switch while its terminal remains absent
  indefinitely. Core snapshots carry `result:null` and `switchSettled:[]`;
  queue/take becomes `ready/result:null`, not indefinitely pending. Superseding
  the open parent silently drops it without a terminal. The KDE entry does not
  consume `switchSettled` as a user-facing completion receipt.
- Fractional native quantization, optional-endpoint noise, presentation coverage
  diagnostics and other K1 unproved internal-window cases remain unresolved.
- Reuse the existing native selector/attempt path and shared core completion
  predicate. No alternative settlement layer was needed. The unused retired
  operation-wrapper type stays for the final legacy-removal slice.
