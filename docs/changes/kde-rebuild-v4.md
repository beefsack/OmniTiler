# KDE rebuild on the supported v4 interface

## Objective and boundary

Make the supported KDE route correct and recoverable under races, and measure
where latency is spent. This is a cutover of the remaining delivery, bounds
and native-evidence seams, not a rewrite of the compositor integration. The
[amended v4 contract](../porting/world-contract.md) is normative;
[pre-KDE preparation](archive/core-pre-kde-interface.md#deferred-to-kde-rebuild-note)
and [review fixes](archive/pre-kde-external-review-fixes.md) passed offline.
That does not prove the current KDE route on a real desktop.

Keep the Rust broker as sole checked capture/command authority, the prepared
core transition as sole policy owner, KWin JS as native observer and actuator,
and the native effect as a presentation consumer. Keep the existing handshake,
revisioned settings timing, FIFO command admission, issuer-only results,
latest-only native apply, retained focus/drag/intent/settlements and strict
reply checks. Replace the particular paths below; delete their obsolete
implementations in the same cutover. No v3/old-v4 fallback, parallel planner,
new adapter policy, new package/runtime topology, macOS or Windows code, or
change to frozen fixtures. The [KDE checklist](kde-v4-acceptance-checklist.md)
remains the single checklist for release acceptance. Its user-owned physical,
real-app, manual display/topology and subjective feel legs stay TBD where
unverified. Bounded fixture evidence does not replace them.

## Current paths and planned replacements

| Seam | Current supported path and gap | Rebuild target |
| --- | --- | --- |
| B3: settings | `WorldEngine::handle_settings` updates settings, readiness and layout policy before `route_connection_request` encodes the reply and the broker checks length. A valid input with many unknown setting names can expand in diagnostics; request bound alone is not proof of a fitting response. | Prepare the settings transition and its exact bounded acknowledgement before installing settings, readiness or layout policy. Preserve valid-sibling application and stale, duplicate, restart-required and revision-reused semantics. Do not clone unrelated world state. |
| B4: membership | `world_members.rs` owns per-lifetime homing. Individual reports are bounded, but incomplete inventories and held, displaced, floating or in-flight subjects can keep members across reports. Per-report limits do not bound their cumulative union. | Bound retained membership across all insert paths; use existing proved retirement, never evict live/uncertain subjects or infer close. Recover without generation reset once positive evidence frees space. Keep the bound structural and atomic on both production paths. |
| B5: replay and issuers | `WorldBroker::remember_response` stores exact envelope and bound reply bytes, with a 1,024-source count limit but no aggregate byte limit. The 1,024 `(sender,client)` high-water namespaces survive until generation change even when unique D-Bus senders disappear. | Byte-and-entry-bounded FIFO retention, without weakening exact replay or the sequence floor. Retire namespaces by authenticated unique-sender lifetime, not age or LRU; an idle live sender keeps its anti-replay mark. Keep already filed commands ordered and executable. |
| B6: decode | Capture and discovery reject duplicate keys; `QueueWorldCommand`, `TakeCommandResult`, `WaitWorldWake` and `ReadWorldPresentation` parse into a last-wins JSON value before checking fields. | Share strict object decoding, including nested command keys. Retain transport/authentication checks and endpoint-specific checks with explicit mixed-error precedence tests. Refusal must not queue, take, park or advance anything. |
| Close/ACK | `world-close-tracker.ts` records event-gated positive closes. Production `postAcceptCapture` later clears the entire tracker on the apply-side path; accepted skips bypass it. `WorldState` also carries a separate pending-close list, not the production producer. | Freeze the exact serialized close evidence in the capture-flight delivery token, bound to lifetime, generation and source. Retire only matching delivered entries at the incorporation cut, before requester Take, intent persistence or native apply; retain closes proved during flight. Remove blanket clearing and the redundant close owner. |
| Visibility | KDE emits frame, member, view, clear and state property attempts plus separate creation/focus evidence. The native-owned desktop path declares `windowSuppression:false`, and focus readiness already has a native arrival bracket. It has no v4 visibility-attempt producer. | Complete the existing v4 evidence path for applicable native visibility actions or contract-authorized no-calls. Preserve native desktop selection and actual view/member attempts. Keep unknown reads unknown; do not turn a selector success, membership exclusion or `hidden=false` into proof of visibility or independent suppression. |
| Latency and recovery | The structural report gate excludes service transport and native actuation. Prior broker samples use a small genesis reply. Gesture changes and terminals share a 120 ms host debounce. KWin has native-event freshness and fixture-only one-shot skip controls, but the carried races have no current native proof. | Measure cold checked admission and the warm 200-window FIFO workload separately from host capture, native setters/readback and paint. Measure the gesture route before changing scheduling. Prove accepted-but-skipped apply and recovery without reopening old authority or issuing retries on elapsed time alone. |

## Ordered units and acceptance

Every code unit runs scoped Rust format check, strict all-target Clippy and
all-target tests for `tiler-core`, `tiler-protocol`, `omnitiler`, with no
skips. When KWin or the native effect changes, follow Rust with KWin typecheck
and the full suite with both offline Qt probes. Keep both Stage 5 production
proofs and exact byte checks enabled. Frozen `world-wire/v4`, `stage5`,
`stage5-v4` and `kde-v4` must match `ff6eaddc`; any active re-recording must
come from the real producer/codec with exact justification per file and
unchanged unrelated bytes. Table gates are additional discriminators.

| Unit | Offline acceptance | Fixture-scoped live acceptance |
| --- | --- | --- |
| 1. B3 settings preparation | Valid input with deliberate diagnostic expansion fails the bound without changing settings, revision, readiness or layout policy. A fitting request at the same revision succeeds; duplicate/stale/reused requests and restart-only behavior remain correct before and after readiness. Test the checked service, not just the encoder. | Shared final startup/reload leg: initial settings precede first ready capture; an isolated live edit changes the intended next plan, without generation rotation or writes to the user's store. |
| 2. B4 membership capacity | Exercise cumulative churn with incomplete reports, held/floating/displaced/transfer controls, a command-created member path, and exact-bound overflow through typed and checked paths. Prove no evictions or partial adoption, plus FIFO preservation and a fresh capture fitting after an exact close or other existing authoritative retirement. | Shared close/recovery legs exercise native close and later admission. Saturation itself is an offline stress test, not an excuse to flood the desktop. |
| 3. B5 broker lifetimes and memory | Near-limit exact replay returns identical bytes before draining; changed reuse rejects, evicted source is stale, and next fresh source drains only once. Assert byte accounting through eviction and generation reset. Private D-Bus proof: live namespace dedup survives result eviction; disconnect frees space, preserves filed FIFO work without orphaned results, and cannot affect another owner or let stale identity replay. | No real-desktop saturation. Shared recovery leg verifies Planner replacement fences old captures, waits and results. |
| 4. B6 strict endpoint ingress | Check duplicate top-level keys on all four endpoints and nested keys on queue; mixed oversize/auth/duplicate/version/generation/semantic cases assert the chosen precedence and no state effects. Valid requests, strict capture/discovery and property-attempt error precedence remain unchanged. | Shared final matrix uses valid discovery, queue/take, wake and presentation paths; negative tests live on a private bus. |
| 5. Close token and delivery proofs | Real producer plus checked broker: A sent, B proved while A is in flight, accepted but apply skipped; only A retires. B reaches the next accepted capture. Exercise capped serialized subsets, lost/late/mismatched/not-ready/refused ACKs, generation fences and same-source redelivery. No close inferred from an incomplete list. No setter from the skipped reply. | Exact fixture UUIDs for both closes. Delay handling of a real bound ACK via a controlled callback barrier, not a fixed sleep; release it after the second close is natively witnessed. Correlate raw captures, retirement, Take receipts, zero writes on the skipped revision, later native reflow and retained close focus at most once. Use the existing skip control for an independent skipped-apply case. |
| 6. Native visibility and creation | Check target/revision/lifetime/path/setter provenance for each supported visibility attempt/no-call, canonical `propertyAttempts` encoding, exact redelivery and ACK retirement, nullable readbacks and no invented setter. Check same-watermark arrival vs invalidation, wrong home/subject, show-before-focus and no premature switch completion. Guard creation spending against skipped replies, delayed ACK, unreadable post-read and unrelated roster changes; only a same-call correlated ID can bind. | Native fixture source/destination in appended workspaces: correlate selection/member setters, shown-state witnesses, arrival bracket, focus and terminal completion; include supersession and unreadable witness control where supported. On explicit native creation, join the authorized operation, before/after roster, returned native identity and follow-up core binding; lose an ACK without creating twice. Unsupported visibility routes are not synthetic passes. |
| 7. Checked-service and host latency | Reproducible release workload through the real checked service, starting before ingress and ending at returned checked bytes/results. Cold: fresh admitted 200-window inventory, report first sample and distribution across fresh worlds; disclose handshake/settings separately. Warm: 200 stable windows, fixed no-refusal 16-command burst including noncommuting operations; terminal results for every command, exact replay without duplicate execution and final state oracle. Publish p50/p95/max, workload, counts and payload sizes; retain comparable small broker and structural gates. | Separately time callback-to-capture, capture round trip, eligible reply-to-native setter and matching native frame event (not delayed polling); include cold fixture admission, FIFO burst and current two-output sends if present. For drag X–Y–X and release, count motion, dispatch and paint updates and split event-to-dispatch, service and reply-to-paint. Confirm final release uses current target; no stale repaint or fleeting invalid target. Change the scheduler only if evidence isolates it: first changed motion promptly, coalesce replaceable samples while busy, never debounce away a terminal. Re-measure after a change. |
| 8. Recovery and integrated native proof | Preserve coverage for latest-only application, request cancellation/redelivery, exact-lifetime recovery and no busy refusal; exercise readiness and wake/timeout recovery without replaying dead-generation intent. Run final Stage 5, both Qt probes, launcher/native-effect/helper tests and structural cost comparisons on the resulting tree. | Rerun small genuine producer journeys after orderly scoped controller and Planner restart: fresh generation/readiness, withheld stale callbacks, preserved exact-lifetime intent, no repeat setter, pending operations settle or have their actual refusal recorded. Retained focus: explicit, admission, positive close and follow/selection as applicable; skip issuing/intermediate apply, prove one setter or real no-call and cancellation before readiness. Run ordinary admission/reflow, float parity, rapid directional FIFO and two-output named readbacks on already present outputs. No hotplug or display changes. A forced process-loss variant uses nested KWin only. |

Native legs follow the [Testing Posture](../live-kwin-testing.md#testing-posture-user-2026-11-15)
and use [fixture windows](../../crates/tiler-live-fixture/README.md) scoped to
`OmniTilerTestWindow`. Join sentinel, PID and exact native UUID before making
native claims. Keep correlation, decisive log lines and actual native
readbacks; a full correlated Describe, not presentation, proves operation-lane
state. Use existing scoped helpers and read-only native probes; any needed
one-shot delay seam must be trace-only, exact-fixture scoped and absent from a
shipped build. Do not close or kill user windows, change displays or write the
user's settings. Use nested KWin only for an ABI-bound effect or a forced-crash
probe. Copy logs before `just dev-off` and stop exact owned helpers. Record
blocked native preconditions explicitly (including focused-float admission or
held-only target if still unreachable), not as passing or as new policy.

## Decision (2026-11-20), pending user review

Use the eight units in the order above. The decisions below govern their
implementation and evidence.

1. **Settings overflow (B3).** Refuse the entire revision on actual reply
   overflow with the existing bounded transport failure; no truncated
   diagnostics, partial apply or revision consumption. Retrying with a fitting
   snapshot at that revision is valid. Preserve per-field behavior when it
   fits; encode a single exact checked response before installation. Report
   the smallest input within the request bound that overflows the reply and
   whether ordinary settings can reach it. If a realistic input can overflow,
   revisit with bounded-by-construction diagnostics before choosing a fix.
2. **Membership saturation (B4).** Cap at 10,000 retained lifetimes, same as
   the window-row limit. Apply existing proved retirements before measuring
   the final candidate. Refuse a still-over-cap candidate as a typed
   structural capacity failure, preserving the previous world and ordered
   FIFO; use a bounded existing wire error with a distinct diagnostic, not
   `reply-oversize`. Accept a later fitting report in the same generation.
   Do not add partial admission or evict uncertain members.
3. **Replay/issuer lifetime (B5).** Cap combined retained request + reply
   payload bytes at 32 MiB alongside the existing count limit, which also
   bounds metadata. Evict oldest replay pairs first; never lower the sequence
   floor. Scope high-water marks to the authenticated unique sender on the
   serving bus/generation. Retire namespaces on proved name loss; preserve
   already-filed intent and do not publish an orphan result. Keep the
   existing cap/refusal for still-live namespaces, not timeout expiry.
4. **Duplicate/refusal precedence (B6).** Check size and authorization, then
   malformed JSON (including duplicates at any depth), then the existing
   envelope keys/version/value order, then the serving generation before
   command semantics or state access. Capture, discovery and visibility
   section precedence are unchanged.
5. **Close ACK boundary.** No wire amendment: immutable local flight evidence
   retires only on an exact, validated, bound ready incorporation receipt,
   even when the watermark disallows native apply. No receipt on unknown
   source, wrong lifetime/generation or transport loss. A later scoped
   capture can carry still-owed evidence without replaying commands.
6. **Visibility scope.** Keep `windowSuppression:false`: no substitute
   minimize/app-hide actuator. Verify the native witness against the supported
   floor and live host. Emit only causally justified attempts or applicable
   no-calls; keep native-owned selection/view/member/arrival on its existing
   proof path. If no legitimate visibility action or no-call exists, leave
   unit 6 unresolved with the exact missing authorization or observable;
   checked-service and native latency measurement can proceed independently.
   Do not invent a row or silently change the capability or wire shape.
7. **Latency budget and visual availability.** Investigation budgets, not
   release criteria: **100 ms p95** for the first admitted 200-window checked
   capture and **50 ms p95** from first Queue request to last checked Take for
   the 16-command/200-window workload. Exclude native actuation and intentional
   barriers; report warm capture within the burst separately. Retain the
   1/4/10 ms steady structural caps and Callgrind limits. The prior first
   structural report reached 42.9 ms; the small broker sample is not a burst
   baseline. Promotion to release criteria requires measured headroom and a
   separate decision. Claim paint only from an observed supported effect;
   use nested KWin for ABI-bound effect loading and report the route. Missing
   paint/native-witness support is an evidence gap, not a pass from a
   projection or an injected gesture alone.

## Starting evidence

Pre-rebuild offline baseline (not a run of this change): Rust 3,540 and KWin
1,388 tests, zero failures/skips. Stage 5 checked broker 5/5 and KWin
subscriber 34/34; launcher/native-effect/helpers 703/204/58. Callgrind
7.83M/34.63M instructions at 50/200 windows, 4.42x growth; steady p95
0.2316/1.7759/4.5770 ms. Small broker fresh p50/p95 204.8/212.0 us,
warm 211.2/219.5 us. See the [full gates](archive/pre-kde-external-review-fixes.md#completion-gates).
No native rebuild verdict at baseline.

## Unit 1: bounded settings acknowledgement

The checked settings route prepares its canonical acknowledgement inside
the scoped core transition, then installs settings, readiness and layout
knobs. Rejection cannot advance the settings revision or expose an escaped
candidate. No handshake, wire shape or fixture change.

The smallest witnessed overflowing request was 890,878 bytes: 100,193
distinct short unknown keys, including short JSON C0 escapes, produced a
4,194,307-byte candidate reply (limit 4,194,304). Dropping the last key in
that family gave 4,194,265 reply bytes. A conservative lower bound across
key spellings is 887,907 request bytes, not a proof of global minimum.
Each unknown row costs at least five request bytes plus its encoded key,
and contributes 34 reply bytes plus its canonical key. Above a 42:9
reply/request ratio, the finite short-key classes contribute at most
127,636 units of `3*reply - 14*request` in total; allow 8 KiB for the
reply envelope and known settings diagnostics and one request comma.
Ordinary settings have catalog-bounded keys, short values and at most one
diagnostic per key; the KDE snapshot cannot approach this key count or
reply size. The checked regression uses 105,000 simple distinct unknown
keys to preserve the overflow without depending on an optimizer.
Scoped Rust format, strict all-target Clippy and all-target tests passed:
3,543 tests, none failed or skipped. The regression refuses overflow before
and after readiness, accepts a fitting same revision, and verifies duplicate,
stale, reused and restart-required dispositions. Native settings reload is
part of the integrated fixture proof, not established by this test.

## Unit 2: retained lifetime membership bound

Count the retained core membership map on the final staged v4 world,
after the report and command batch have applied existing retirement
rules. More than 10,000 records refuses as `snapshot-invalid` /
`membership-over-cap` before outbound preparation or adoption. Rejected
captures preserve accepted world state and leave checked broker commands
parked in FIFO order. The checked ingress still remembers the bound
refusal for exact redelivery; a later fitting report uses a fresh source
in the same generation. Missing or uncertain lifetimes are not evicted
to make room.

Offline, cumulative incomplete inventories reach exactly 10,000; the
next lifetime is refused. A native sticky rollback adds a member only
in the proof-bearing branch, and that branch fits after an exact close.
The checked service exercises 10,000 real admissions, byte-identical
refusal replay, no result or presentation adoption on refusal, and
ordered recovery of a noncommuting focus/maximize pair. Saturation is
offline; native close and later admission are part of the integrated leg.
Scoped Rust format, strict all-target Clippy and all-target tests passed:
3,546 tests with none failed or skipped. No fixture bytes changed.

## Unit 3: bounded broker replay and issuer lifetime

The checked broker retains exact capture requests and their bound replies
under both the 1,024-source and combined 32 MiB payload limits. Exact replay
bypasses dispatch and drain; changed reuse still refuses, and oldest-first
eviction cannot reopen the sequence floor. Generation rotation clears
retention and byte accounting. The checked regression fills the byte budget
with admitted large reports, verifies replay and eviction independently of
count pressure, and drains pending work only on a fresh source. A malformed
gesture refusal with pending work does not advance the floor or drain.

On the private D-Bus, an idle live issuer still deduplicates after result
eviction and well-known alias loss. Proved unique-name loss frees namespaces
and removes settled and retired results. A focus filed by that issuer still
runs before another issuer's maximize-current-focus; only the latter receives
a result. The post-credential barrier verifies exact-sender invalidation,
no namespace recreation and no reauthorization of an unrelated caller.
There is no wire or fixture-byte change. Planner replacement on the real
desktop remains part of the integrated recovery unit.
Scoped Rust format, strict all-target Clippy and all-target tests passed:
3,548 tests with none failed or skipped. Independent review found no
material issues with the command lifetime or replay boundaries.
