# Retained per-window intent desire

- Status: approved by the user 2026-11-12; implemented and verified offline in core/protocol/KDE. Native acceptance and other-port adoption remain open.
- Scope: first [C3 conversion](core-v4-review.md#user-decisions-2026-11-12), core/protocol/KDE together; other ports later. No intent-policy change or acknowledged-events lane.
- Model: unified retained `focusRequest`, approved in `18f904fe` ([decision](../decisions/contract.md#state-centric-contract)), without spend-once accounting: storage replacement is repeatable. Core/protocol implement that [contract](../porting/world-contract.md#one-retained-focus-request) shape; coordinated adapter adoption and verification remain incomplete.

## Today (source at `f6ccc9f7`)

- Paths below: `C/` = `crates/tiler-core/src/`, `K/` = `kwin/src/`, `M/` = `crates/tiler-macos/src/`, `W/` = `crates/tiler-windows/src/`.
- `C/world_report.rs:143-164`: `IntentWrite {win,data: Option<String>}` saves opaque bytes or null-clears, capped at 10,000 rows. Inner v1 carries float/tile and sticky prior, not geometry ([contract](../porting/world-contract.md#opaque-intent-storage)). ToggleFloat saves derive from result/committed state (`C/world_publish.rs:172-236`); sticky stages saves, proven native refusal restores prior/clear (`C/world_sticky.rs:632-637,1439-1462,1501-1517`), ambiguous outcomes keep requested intent without retry.
- Publishing drains command/rollback saves; batches merge latest per window (`C/world_publish.rs:54-145`, `C/world_run_v4.rs:683-690`, `C/world_publish_v4.rs:153-154`). Once drained into a wholly lost/skipped reply, saves/rollbacks are not recovered by later consumed/observation replies. Qualification to C3's "delivered once": exact duplicates return cached writes (`C/world_admit_v4.rs:348-358`). Same-value replay is idempotent; obsolete values would regress storage without the existing envelope freshness fences, which remain mandatory.
- KDE persists accepted-but-superseded writes (`K/world-entry.ts:6097-6124`). `K/world-intent-store.ts:362-546` updates an opaque map/persists full snapshots with correlated success. `K/world-entry.ts:4787-4932,5544-5571` serializes, gates native apply on durability/freshness, coalesces continuations and retains owed durability after failure. Delivery queues change; independent storage-call correlation/freshness remains. There is **no intent-write sequence/report ack watermark to delete**; general request/command/revision tracking remains.
- macOS sends empty `intent_records` (`M/world_builder.rs:679`); nonempty writes latch a session-wide hold before native apply, with no persistence/recovery backend (`M/world_dispatch.rs:200-225,540-559`). `M/world_runtime.rs:556-565` retains reply-derived lifetime memory, not restart storage. Windows has no intent-write consumer: its world owner is standby, legacy Engine/native-marker recovery remains active (`W/tiling_sys/world_owner.rs:2-24`). C3 does not establish durable delivery on either platform.

## Approved replacement

- Replace required `intentWrites` with required `intentDesired: IntentDesire[]`, unique by `win`; each row is `{win, data: opaque-string | null}`. Core uses typed values; encoding remains transport work. Keep inner v1, 1024-byte values, 10,000-row cap and 4 MiB reply bound; budget the **entire retained map** including clears, refusing overbound candidates transactionally, never truncating.
- One authoritative current target per window lifetime, bound by `(adapter,gen,win)`, independent of whether a `WindowDesire` is currently visible. String means exact saved bytes; null means explicit desired slot absence; missing row means no storage authority, never delete. Keep null targets after successful clear. Accepted mutations replace that window's target in command order; rollback replaces it with the exact prior. Rejected mutations leave it unchanged. Publication projects, never drains; observation-only and consumed-command replies retain targets.
- Compare latest accepted desire against verified storage/last **successfully persisted** snapshot, not the pending map. Equality means zero writes; unknown/different means atomic replace/clear before dependent native writes. Serialize calls; completion compares captured persisted values with latest desire, never resumes an issuing-reply continuation or proves a newer target. No write id/counter/ack history.
- Completion is adapter-local durable equality, not target removal; no storage report field is added. Failure/unknown retains desire and holds the whole reply's dependent native application. Retry on every later accepted reply while desire differs from durable storage, with no timer or recovery signal. Native sticky refusal uses existing state evidence to replace desire with prior under the [approved rollback rule](../spec/reference-outcomes.md#native-sticky-refusal-and-saved-intent), distinct from storage refusal.
- Reconnect preserves verified storage knowledge; lost knowledge requires binding/storage verification, not assumed success. Restart imports exact lifetime-bound records before admission; accepted valid imports initialize desired intent. Late imports never overwrite runtime intent. Old-generation replies grant nothing. Retain through holds/minimize/incomplete omission; forget on proven lifetime end/complete departure, matching intent-ledger cleanup. Never clear a replacement window via old id; orphan cleanup requires lifetime proof.

## Replace, do not layer; platform boundary

- Delete `IntentWrite`/`intentWrites`, `sticky_command_save`, `sticky_rollback_saves`, `drain_intent_saves`, batch-save accumulation/merge and drain-only sizing reservations. Mutations update retained targets. Consolidate `sticky_saved`'s last-published value into that owner (`C/world_engine.rs:1527-1565`), preserving exact imported/pre-command priors and unknown/conflict bounds, including `sticky_saved_incomplete`, sticky blocks/saturation. Published intent is not observed durability. Keep `sticky_visible_rev` authorization, import/admission/provenance and native attempt tracking; they are not delivery queues.
- KDE replaces `incorporateWrites`, superseded-reply persistence and reply-specific `IntentPersistWaiter` continuations/owed-change flags with one desired-versus-durable reconciler, with recovery per Q2. Keep atomic store v2, storage-call success checks, serialization and freshness checks. Replace consumed-empty/save-once expectations (`C/world_intent_save_tests.rs:422-465`, `C/world_sticky_native_tests.rs:718-751`) with retention/no-redundant-write coverage; preserve refusal/saturation and desired-replacement rollback assertions.
- macOS adoption checklist: update typed/strict replies; implement the lifetime-verified atomic host slot/store (choice open); preload/populate `intent_records` before admission; add desired/durable reconciliation and persist-before-dependent-apply in Accepted and CommandConsumed paths (`M/world_dispatch.rs:540-559,622-643`); replace `intent_writes_hold`/`unpersisted_hold` and latch fields/tests with recoverable gating; fence restart/close/stale callbacks; add conformance/native persistence evidence. Windows adopts during v4 port; native-marker/recovery migration is separate, not decided here.
- [Core-owned intent persistence](../backlog.md) still owns macOS slot selection, lifetime proofs, adapter-echo/retained-frame-readback removal: use core intent/last-known frames, not desire-as-native-fact. Later C3 settlements, warp, `effects` and command `result` need separate retention/completion designs; repeatability is not assumed. C4's unified native attempts/settlement stay later; add no intent attempt lane.

## User decisions (2026-11-12)

| Question | Approved decision | Discriminating reference-outcome row |
| --- | --- | --- |
| Q1: Does core need storage completion/refusal facts? | Durability stays adapter-local. No new report field; compare desired with verified durable storage, writing only on difference. Core cannot claim durable completion. | "Intent desired storage feedback": lose response; observe present/absent/unreadable; restart. |
| Q2: What wakes recovery after storage failure? | Retry on each later accepted reply while desired differs from stored. No timer or storage-recovery signal. | "Intent store recovery": fail, restore storage, deliver unchanged desire. |
| Q3: Which dependent application is held? | Preserve the whole-reply hold. Narrower window/property isolation is separate work. | "Intent failure isolation": A's save fails while B needs reflow; recover A. |
| Q4: Does a verified import initialize desired storage? | Accepted valid imports initialize that lifetime's desire. Late imports never overwrite runtime intent. Keep canonical re-encoding, even when it causes a one-time rewrite. | "Opaque intent import timing": import, admit, lose/change slot, reconnect without a command; retain the existing late-import outcome. |

- Minimal rows are in `docs/spec/reference-outcomes.md`; unsupported native/reference results remain TBD. Existing sticky/current-mode/import decisions stay unchanged.

## Implementation choices and current evidence

- Imported data is not necessarily byte-identical after re-encoding: the strict inner decoder accepts whitespace, field order and escaped keys, while its encoder emits canonical field order without whitespace. A noncanonical accepted record therefore incurs a one-time rewrite per window; canonical imports do not. No raw-byte special case is retained.
- Core retains typed `IntentData` values, including explicit null clears, and projects them in stable window order. Existing conservative reply sizing remains until C5; capacity refusal is transactional across the whole retained map, including imports and queued commands. Resident rows may conservatively consume capacity before lifetime cleanup rather than permitting an accepted mutation to drop its target.
- Removed the one-shot command/rollback queues, drain/merge publication path and separate saved-value owner. Core now publishes the retained typed map. Conservative inline/batch bounds still reserve potential map growth; they are admission bounds, not delivery queues. Exact staged-reply validation remains the transactional commit backstop, including imported rows.
- KDE now owns one desired-versus-durable reconciler, serialized storage calls and one latest dependent-apply slot. Removed `incorporateWrites`, superseded-reply write delivery, persistence waiters and owed-change flags. Storage-call correlation, atomic store v2 and freshness fences remain. Same-generation reconnect preserves verified storage knowledge; seeding is restricted by the entry's existing generation/hydration gates.
- Complete-inventory projection uses the existing lifetime cleanup criterion so a proven-departed target is not published one last time before cleanup. Incomplete omission, held windows and explicit null clears remain retained. Capacity regressions reject new targets transactionally and permit replacement at the cap; overflow never substitutes a dropped target for an accepted mutation.

## Offline outcome and evidence (2026-11-13)

- Direct final verification: `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`, strict `cargo clippy` for the same packages/targets with `-- -D warnings`, and `cargo fmt --all -- --check` pass. The permitted full Rust suite completes with 3,347 passed, zero failed across 68 runner summaries, including doc tests; core library 2,163 passed. KWin `npm test` completes with 1,296 passed, zero failed/cancelled/skipped, and `npm run typecheck` passes. Independent source/fixture review found no remaining blocking findings after provenance and latest-only journey additions.
- Genuine engine replay verifies all ten active visibility replies. `intent_retained_record.rs` records/replays save, two later commandless retained publications, proven-refusal null clear and retained clear with exact encoder bytes. Normal tests require committed bytes; only explicit recorder mode writes. KDE genuine journeys prove persist-before-apply and first delivery of only the final retained projection to a fresh store after withholding the issuing and two intermediate publications; equality then owes zero writes.
- Core tests also retain consumed/duplicate semantics, command-order latest-wins, native-refusal rollback, import initialization/late-import fences, close/departure cleanup and full-map row/byte refusal. KDE entry/store tests cover unchanged-target failure recovery on a later acceptance, racing old success/new desire, obsolete/old-generation replies, retained clears, reconnect/restart and id reuse. Initial discriminator coverage was incomplete during migration; later exact replay and end-to-end regressions close that evidence gap.
- Active reply witnesses were regenerated through the strict codecs; behavioral replies were independently reproduced through real engine transitions. The genuine stage5 recorder produced both full/s2 chains (92 artifacts), and the byte-test generator refreshed the KDE active reply. A durable migration recorder produces the new active migration family; tests now use its current core-selected workspace identities rather than modifying historical recordings.
- Direct frozen-family equality against the pre-change tree: `world-wire/v4` 547/547 files, `stage5` 32/32, `stage5-v4` 52/52, and historical `kde-v4` 20/20 are byte-identical, with no extra files. Both stage5 proofs remain enabled and pass in the final suites.
- macOS edits are shared-type/hold-name mechanics only and unverified on macOS; no persistence backend, import population or recoverable gating was added. Windows needed no shared-type edits and remains unverified. No live desktop testing was performed. Native storage survival/lifetime proof, physical restart/reconnect and other-port adoption are not established by these offline results.

## Native KDE smoke blocked before intent mutation (2026-11-13)

- Baseline `83ab5b91`. Started the Wayland fixture server with no windows,
  appended/selected a native workspace through VirtualDesktopManager D-Bus,
  then ran `just dev trace --scope-class OmniTilerTestWindow`.
  Discovery connected, but all four initial/event-driven captures were rejected
  with `snapshot-invalid / cursor-hit-unknown`; no accepted `applied` row exists.
  Read-only `ReadWorldPresentation` returned
  `outcome:rejected, kind:not-ready, message:"no accepted capture"`.
  See the dated [focus smoke record](retained-focus-requests.md#native-kde-smoke-blocked-at-bring-up-2026-11-13).
- Check 6, floating intent save/equality/restart: **NOT RUN**. No fixture window
  was spawned or toggled, no unrelated accepted reply followed a mutation, and
  no restart was attempted. Startup
  `intent-seed outcome=ok correlation=intent-seed-1 stored=7 returned=7`
  proves only a store read, not a new fixture save, durable equality, lifetime
  restoration or absence of redundant writes after restart.
- Trace/journal and fixture list/shutdown evidence was retained outside the
  repository before teardown. Final dev status is DOWN; fixture process/socket
  and `ydotoold` are absent. No persistence acceptance is claimed. Next: resolve
  startup capture rejection, then perform toggle -> one write -> unrelated
  replies with zero extra writes -> restart -> restored float with zero rewrite.

## Native KDE rerun after scoped-hit repair (2026-11-13)

- Baseline `c388285d`; fixture-only Wayland startup now accepts empty and
  populated captures. See the [focus rerun](retained-focus-requests.md#native-kde-smoke-rerun-after-scoped-hit-repair-2026-11-13)
  for startup/native joins, routes, latency and remaining focus checks.
  Evidence filenames below identify the externally retained bundle.
- Direct storage evidence: B was first floated, then a no-target toggle tiled
  it (`dev-console.log:869-890`). `check6-S0.json` -> `check6-S1.json`
  changes exactly B's `float:true` to `float:false`; the correlated store call
  succeeds. Spawning/closing fixture D causes unrelated accepted publications;
  S1 and S2 have identical bytes and nanosecond mtime, with no extra intent
  store write. This establishes save/equality behavior for that leg, not restart.
- Actual order matters: send-follow and its implicit toggle ran before restart.
  That toggle floated B again (`dev-console.log:1262-1285`), so the restart
  imports S3, with B and C both `float:true`, not S1's tiled B. S3 and S4 have
  equal SHA-256 and nanosecond mtime (`check6-S3/S4-sha.txt` and
  `check6-S3/S4-mtime.txt`); `planner-devon.log:5,9-20` records store seed,
  accepted captures and no intent write. Preimage writes are a separate store,
  not redundant intent writes.
- The new-generation presentation retains both fixture identities as floating
  (`check6-restored.txt:1-3`). Fresh readiness is independently present
  (`check6-readiness2.txt:1-6`); the recovered current-owner journal places the
  exact sentinel/PID/native-ID joins after restart
  (`lead-current-pid.journal:1175-1205,1218-1222`). Native homes and float-sized
  frames agree. Logical workspace labels changed across the new generation;
  the native desktop identities remained joined, not compared by ordinal.
- Result: storage equality, import of the latest floating intent and unchanged
  intent storage across this restart are observed. Full native restart
  acceptance remains incomplete: the journal records native `above` pending
  for both fixtures (`lead-current-pid.journal:1208,1211`) without independent
  above-state readback. The restarted full trace was not captured; its tmpfs
  copy was also attempted only after shutdown, so the pre-teardown preservation
  requirement was not met for that leg. Later journal recovery does not undo
  that evidence gap. No tiled-false restart, storage-failure/reconnect or
  physical-input acceptance is claimed; no product defect was established.
- Next isolated check: toggle a joined fixture, verify one durable change and
  unchanged storage on unrelated accepted replies, restart without intervening
  intent mutations, then join the same native lifetime and verify restored
  mode/above state plus zero intent rewrite. Preserve trace logs before each
  shutdown. The broader change remains open.

## Additional KDE restart evidence (2026-11-13)

- Partial command-route evidence, not complete restart acceptance: toggling a
  joined fixture to float produces one successful intent write. After controller
  and planner restart, fresh readiness passes, the same fixture lifetime retains
  float classification and frame, and independent native `keepAbove` readback is
  true before and after restart. Both incarnations retain full traces; the new
  planner seeds the stored records without another logged intent write.
- Missing evidence: no before/after intent-store bytes or hash comparison, no
  full `Describe`/fixture snapshot pair across restart, and the restarted trace
  still contains a pending above-state operation after an earlier stale drop.
  Native above readback does not prove core settlement or absence of pending
  operations. The broader storage/lifetime and recovery acceptance remains open.
- Two other fixtures lose native maximize across restart. Their intent-policy
  expectation was not established by this leg, so this is an unresolved
  observation, not a confirmed regression. No product fix followed.
- Helpers were absent before controller teardown, traces were copied before
  shutdown, and final read-only status/process checks confirm dev DOWN with no
  fixture or tiler processes. The [shared follow-up](core-platform-switch-evidence.md#kde-follow-up-2026-11-13)
  records helper qualifications and remaining physical-input checks.

## Scoped native maximize restart reproduction (2026-11-13)

- Reproduced the prior maximize-loss observation on two fixture windows, not a
  complete restart acceptance pass. Before restart both exact sentinel/PID/UUID
  joins have native `maximizeMode:3`, `keepAbove:false` and work-area frames;
  fixture mode readbacks agree. After a fresh controller/planner start with
  evaluated readiness, the same UUIDs have `maximizeMode:0`, unchanged false
  keep-above and side-by-side tiled frames. Fixture geometry agrees; its compact
  post-restart snapshot omits unknown mode, which alone is not proof of clear.
- `leg6-store-before.txt` and `leg6-store-after.txt` retain identical lengths and
  SHA-256 values for intent and native-preimage storage. This establishes storage
  equality for this sequence, not float-intent restoration: neither survivor was
  explicitly floated. Native before/after files, fixture snapshots and separate
  readiness/journal captures are retained as `leg6-*` evidence. Full correlated
  Describe replies were unavailable (`not-kwin-owner`); there is no complete
  post-restart presentation pair, so pending-operation settlement is unproved.
- Likely owner: core startup admission, `world_born.rs:464-502` under
  `settings_state.rs:74`'s `TileOnLaunch` default. A fresh generation lacks the
  prior admitted/born sets, so the surviving maximized windows can enter the
  first-seen born-clear lane. Restart logs show two initially held rows, but do
  not directly capture the exact unmaximize setter; this is source attribution,
  not a fully correlated native-write proof.
- Policy verdict: defect candidate / contract ambiguity, not established expected
  behavior. [Born-maximized admission](../decisions/window-state.md#born-maximized-policy-and-tile-overlays)
  clears first-seen maximized admissions by default, but does not explicitly
  distinguish a new launch from a surviving native overlay after tiler restart.
  The [overlay decision](../decisions/window-state.md#fullscreen-and-maximize-overlays)
  explicitly drops only composed maximize on orderly restart; the
  [intent contract](../decisions/contract.md#intent-ownership-and-persistence)
  rejects losing intent on restart. The unchanged `v/float/stickyPrior` storage
  shape alone cannot decide native-overlay survival. A minimal native restart
  discriminator was [initially recorded with expected/reference outcomes TBD](../spec/reference-outcomes.md#native-maximize-across-tiler-restart).
  No product fix or intent-schema change was made in that native leg. The later
  user decision (2026-11-13) preserves startup native maximize; the
  [offline startup-adoption fix](native-maximize-startup-adoption.md) confirms the
  born-clear source path and adds shared core/service regressions. This does not
  supply the missing native setter correlation or post-fix native acceptance.
- Helper accounting is qualified by the
  [scoped follow-up](c9-product-evidence.md#native-recovery-correction-and-scoped-follow-up-2026-11-13).
  Both trace sets were copied before teardown; final output/status receipts and
  later direct process checks establish dev DOWN and no owned live processes.
  Native float/above-true restart restoration, full Describe settlement,
  storage-failure/reconnect and physical acceptance remain open.

## Wire, fixtures and implementation verification

- In-place strict v4 amendment: no fallback, alias, version bump or second delivery path. Matching core/codecs/KDE change together; later ports must adopt before release acceptance. Frozen `test-fixtures/world-wire/v4`, `test-fixtures/stage5` and existing `test-fixtures/stage5-v4` bytes are immutable.
- Refresh affected reply vectors/scenario expectations in `test-fixtures/world-wire/v4-active` through real codecs/engine producers, including active save/clear and skipped-publication cases; redirect changed frozen-vector references, never patch history. Re-record `test-fixtures/stage5-v4-active/{full,s2}` using `kwin/tests/world-stage5-v4-record{,-cli}.ts`, and `test-fixtures/kde-v4-active/bytes` using the genuine `world-bytes.test.ts` generator. Even empty reply lanes change. Preserve both stage5 proofs; no fabricated producer bytes.
- Implement core/protocol/KDE; test skipped/duplicate/obsolete replies, batch latest-wins, retained clears, rollback supersession, failure/recovery, racing success/new desire, reconnect/restart/import timing, close/id reuse and whole-map caps/sizing. Adapt intent-save/native-sticky/codec and KDE store/entry tests; assert unchanged desire means zero disk writes. Run `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`, KDE tests and recorder replay proofs. Native storage/lifetime evidence remains separate for every port.
