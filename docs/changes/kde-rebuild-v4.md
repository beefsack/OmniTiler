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
| Visibility (user decision 2026-11-20) | KDE uses host-owned native desktops and declares `windowSuppression:false`. The applicable set of visibility attempts and no-calls is empty by contract. | Prove the native arrival path with member and view attempts, the native-arm arrival bracket, focus feedback and terminal switch completion. Keep unknown reads unknown; selector success, membership exclusion and `hidden=false` do not prove visibility. |
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
| 6. Native arrival and creation (user decision 2026-11-20) | On host-owned desktops, the applicable set of visibility attempts and no-calls is empty by contract. Check actual member/view attempts, canonical `propertyAttempts` encoding and exact ACK retirement; prove same-watermark native arrival vs invalidation, wrong home/subject, show-before-focus and no premature switch completion. Guard creation spending against skipped replies, delayed ACK, unreadable post-read and unrelated roster changes; only a same-call correlated ID can bind. | Native fixture source/destination in appended workspaces: correlate member and view attempts, the native-arm arrival bracket, focus feedback and terminal completion; include supersession and unreadable witness controls where supported. Correlate the creation operation, before/after roster, same-turn native ID and follow-up core binding; lose an ACK without creating twice. |
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

## User decision (2026-11-20)

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

## Unit 4: strict checked endpoint ingress

Queue, Take, Wait and presentation-read now use the same strict JSON
decoder as capture and discovery. Duplicate keys, including escaped spellings
of the same key and nested objects inside arrays, refuse as `malformed`.
Size and authorization still win before decoding; envelope keys, version and
values precede serving generation and command semantics where applicable.
Capture and discovery were not changed.

Private-bus and broker checks cover mixed refusals, valid controls, no queued
work or wake progress from refusals, and intact result/presentation delivery.
The existing v3 and frozen-v4 capture refusals still pass. Scoped Rust format,
strict all-target Clippy and all-target tests passed: 3,550 tests, none failed
or skipped. No fixture bytes changed; native valid-route evidence belongs to
the integrated recovery unit.

## Unit 5: exact close delivery

Two native runs provided partial evidence. The first held a bound reply until
the second fixture closed, skipped its native apply, then reflowed the
remaining fixture. It recorded retirement counts but not the serialized
close IDs or a requester result. The second recovered an identical retained
Take result from the same requester connection, but the queued command
triggered a capture before the first close; the next capture retired both
closes together. Neither run establishes the full close-delivery sequence.

The close ledger now freezes only tracked IDs in the successfully encoded
`report.closed`. Ready incorporation for the bound generation and source
retires that token before requester result collection or native apply.
Rejected, stale and lost replies do not retire it, and same-envelope
redelivery keeps the original token. There are no wire or fixture-byte changes.

**User-accepted live proof (2026-11-20):**

In the combined fixture run, `src-9-1f5k7sa` (seq 9) serialized only
`4c2ce73b-6460-48a0-8478-c5234e30476a`. While its ACK was held, native
readback proved the second fixture absent. Releasing rev 9 retired one
close and skipped all setters. `src-a-pcjmw2` (seq 10) serialized only
`86bf98f8-3a14-436d-8e75-ddf07bf536d9`, retired it, and reflowed the
remaining fixture from 1012×542 to 2032×1092. It was already active;
the retained focus operation did not write. The external probe's two
checked Takes returned the same ready result for global command 2, joined
to `cmdSeq=2` in the held capture without re-execution. The independent
seq 14 capture retired one close and consumed the one-shot apply skip.

## Unit 6: native arrival and creation

**User decision (2026-11-20):** KDE visibility is native-owned. The
[v4 contract](../porting/world-contract.md#visibility-and-focus-ordering)
uses native desktop selection and membership with `windowSuppression:false`,
null visibility targets, and no visibility writes. KDE emits no visibility
attempts or no-calls by design.

Unit 6 proves member and view attempts, then the native-arm arrival
bracket: same watermark, same window lifetime, native home equal to the
frozen target, and a readable frame. Correlate that bracket with focus
feedback and a `SwitchEvent` that cannot complete before arrival, and
prove native creation correlation. An exact listed `windowAt` hit is
an extra positive arrival witness only; a miss is unknown. Selector
success, membership exclusion, and `hidden=false` never prove visibility.

The first fixture run was perturbed and verified only a native creation
and shown arrival, not receipt redelivery or causal focus completion.
The checked reply `src-f-t2tm5u` (rev 15) requested `op-native-4` for
`ws-17`; apply logged
`mk=1`. The next complete roster had one additional desktop,
`75ec61c7-4333-4e5f-a259-ba8d8b3b1e68`, which the rev 16 reply bound to
`ws-17`. The current desktop did not change in that interval. Two later
fixture resizes left the roster unchanged; their captures were fresh,
not replay. The same-call ID rule has offline coverage; the live trace
did not expose that receipt's ID at the call boundary.

The fixture-only trace records `creation-readback` with the operation,
logical workspace and native ID when synchronous post-call enumeration
finds exactly one new ID. The floor uses `op=- ws=-`. KWin's create call
returns no ID; the trace does not participate in binding.

For `src-o-1cv3jga` (rev 24), `member-attempt op=op-5 setter=accepted`
moved fixture `29f41e83-bdd6-46d4-9ea9-3dc9f5b6c2e4` into the new
desktop. Native readback found that exact UUID at its frame center on
the destination. The same reply's `focus-24 afterShownOp=op-5` still
named the source workspace and logged `no-call reason=identity-stale`.
The next reply reported `completed-with-failures focus=unproven`.
This is not evidence of an accepted arrival bracket, a causal focus
setter, or successful switch completion.

The clean fixture leg bound `op-native-3` to the unique same-turn native
desktop at rev 18. At rev 22, both `op-5` and `focus-22` named the bound
destination `ws-21`. The accepted member and view writes read back that
same native desktop. Both samples of the shown-arrival bracket carried
watermark 22 and matched the same fixture lifetime; the native home and
frame read agreed with the independent frame-center `windowAt` hit.
KWin was already focused on that fixture and performed no focus write.
Capture `src-n-nmdunb` carried the bracket and proven foreground
feedback; rev 23 was the first terminal for `op-5`, `complete` with
`focus=proven`. The timed-out creation ACK and superseded publication
are separate offline checks, not claims about this native run.

## Unit 7: checked-service and native latency

Thirty fresh release samples per workload used the checked service in
process, not D-Bus transport. Cold timed the first admitted capture of
200 windows; warm timed first Queue through last Take for 16 ordered
commands on 200 stable windows. All commands settled, exact replays
returned the same bytes, and the final state matched the ordered oracle.
Handshake and settings were measured separately.

| Checked span | First (ms) | p50 (ms) | p95 (ms) | Max (ms) |
| --- | ---: | ---: | ---: | ---: |
| Cold capture | 51.367 | 49.543 | 50.322 | 51.367 |
| Warm burst | 19.947 | 19.183 | 19.361 | 19.947 |
| Capture within burst | 19.882 | 19.056 | 19.232 | 19.882 |

Handshake and settings p95 were 0.025 and 0.028 ms. The cold request
was 142,445 bytes; the reply was at most 68,372 bytes. Warm capture was
142,446 bytes in and 69,158 out; the 16 Queue and 16 Take calls totalled
2,167/1,902 and 1,335/2,271 request/reply bytes per sample. These
measurements are below the investigation budgets, not release criteria.
Steady structural p95 at 10/50/200 windows was 0.1661/1.5152/2.9247 ms;
Callgrind measured 7.822M/34.595M instructions at 50/200. Comparable
small-broker medians were 206.6 µs fresh and 212.4 µs warm.

On the real host, three cold fixture admissions shared a capture.
Callback-to-send was 83–92 ms; the shared service round trip was 4 ms.
Eligible reply-to-frame-setter was 5 ms per window, and each exact
native frame event followed its setter by 35–58 ms. A separate
six-command burst filed every command before taking results: 2.41 ms
to file, 213.06 ms to the last observed Take including polling
(one run). A two-output send was confirmed by independent output, home
and frame changes for the same native window. These runs do not
establish a 200-window native distribution or isolate membership
setter timing.

The nested drag leg remains unverified. Installed KWin exposes a
fake-input protocol and a screenshot service, but preparation did not
establish an input client that verifies the owned compositor on the
same connection it uses to send events. No 640×480 capture from the
private nested session was observed. No nested compositor was
launched: there is no X–Y–X timing, release-placement result, or
visible-paint evidence. The presentation trace records submission,
not paint, and isolated stills could not exclude a fleeting invalid
preview. Scheduling was left unchanged.

**User decision (2026-11-20):** accept this gap. Drag X–Y–X, release
placement and visible preview paint timing move to in-person physical
testing, as do other subjective jank or performance checks that scripts
cannot measure reliably.

Scoped Rust format, strict all-target Clippy and all-target tests passed:
3,553 tests, none failed or skipped. No fixture bytes changed.

## Unit 8: recovery and integrated native proof

With the fixture controller still loaded, a Planner reload invalidated an
occupied ready-reply continuation. The held reply contained a focus operation;
generation reset cleared it before apply, the new generation reached ready,
and releasing the hold returned `release-empty` in the new generation. There
was no setter from the held reply. This demonstrates continuation fencing, not
cancellation of an outstanding raw D-Bus callback.
Replacement discovery accepted settings revision 1 before the first
ready Describe in both the Planner-only and full replacements.

On an orderly controller and Planner restart, the same fixture lifetime
retained byte-identical stored intent. The replacement Planner refused the
old pending result request with `generation-mismatch`. Native sticky and
keep-above state went true→false→true: startup repaired the original native
state as required by the [preimage contract](../porting/world-contract.md#freshness-and-recovery),
then a fresh commandless plan reconciled the retained intent. Exactly one
accepted setter per property read `pre=0 post=1`. The next accepted capture
and an independent native readback confirmed the final state, with no stale
or duplicate setter.

Ordinary native admission and close reflow matched published allocations.
Float and unfloat changed native keep-above and restored the two tile
slots. The checked producer queued all 60 directional commands before
taking results; all completed in order, with the expected final native
slots and foreground. This was not physical shortcut input.

The explicit and positive-close requests retained the same ID through
skipped issuing and intermediate publications. Explicit focus called the
setter once at rev 11. Positive close serialized and retired the exact
closed fixture; native fallback was C, while the retained successor A
was focused by one setter at rev 14. The feedback and independent native
foreground agreed in both cases.

The same fixture lifetime also moved to the named right-hand output under
a checked send command. The terminal receipt, native membership readback
and tiled frame all agreed with that output's active workspace.

The admission two-skip witness is native-inapplicable on this fixture.
The issuing reply carried `focus-5` and was skipped; on the next capture
KWin already reported the newcomer as foreground. Core logged
`equality reason=already-target`, and the intermediate reply had
`focusReq=0`. This was retirement from an observation, not an adapter
setter or evaluated no-call on a retained intermediate request.

Offline, `world-focus-request-producer.test.ts` covers this lane with
“skips the issuing and intermediate automatic-admission publications;
latest applies once with the issuing rev”. Readiness cancellation is
covered separately by `world-focus-feedback.test.ts`, “null envelope
cancels unstarted readiness; a newer id then adopts once”, using a
deferred explicit selection. By the
[v4 focus contract](../porting/world-contract.md#one-retained-focus-request),
only an explicit command may carry non-null `afterShown`; admission
never waits on readiness.

The corrected send-follow run retained its transfer after the issuing
reply was skipped, but Core observed the mover already in the foreground
and retired the focus request by equality before the next reply. This is
a Core no-call, not evidence of native arrival or a focus setter.

The separate selection run retained `focus-10` through both skipped
replies. Applying the next reply switched the destination view and
deferred focus as `not-ready`. KWin selected the staged fallback C, not
the requested target B. On the accepted view readback, Core retired the
request with `no-call reason=baseline-changed`. This proves retention
and the view change, not a native focus setter or a `complete/proven`
`SwitchEvent` for the selection.

`retire_pending_focus` in `world_close_focus.rs` retires the request
when `focused_global` differs from `baseline_focused`. In this run the
accepted view switch itself caused KWin to activate C, so the next
capture tripped that fence before the focus setter could run. Unit 6's
native arrival proof covered the already-target equality branch only.
User decision (2026-11-20): a foreground change caused by the request's
own view switch should not cancel it. The contract change that expresses
this is still open for design and is not part of this unit.

In isolated nested KWin, killing the exact Planner process removed its
D-Bus owner. The controller and both fixture lifetimes stayed in place.
The replacement accepted settings before the first ready Describe,
published tiled allocations matching native geometry, and accepted a
new focus command whose setter and foreground readback agreed. A
separate connection's old-generation Take was refused with
`generation-mismatch`; this does not prove requester continuity or a
pending result at the point of forced loss.

The final offline gates passed: 3,553 Rust and 1,404 KWin tests with no
failures or skips, both Qt probes, the Stage 5 checked routes, and the
launcher, native-effect, and helper suites. No wire fixture bytes changed.

The isolated Callgrind run measured 7.81M and 34.59M instructions per
report at 50 and 200 windows, with 4.43× growth, under all three limits.
Worst steady p95 was 0.226, 1.791, and 4.616 ms at 10, 50, and 200
windows. The small broker comparison gave median series p50/p95 of
203.2/210.2 µs fresh and 209.9/218.0 µs warm, with comparable reply
sizes. These are not host or checked-service burst latency measurements.
