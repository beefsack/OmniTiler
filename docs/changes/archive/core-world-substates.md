# Own world sub-states (C2)

## Goal and scope

Status: implemented and accepted offline on 2026-11-18. Keep `WorldEngine` as the sole
world owner, with private sub-states, one owner per fact and one atomic report
adoption. Build on C1's private `WorldLayout`; no inner engines or synthetic
acknowledgements.

Authority: [approved sequence](../core-v4-review.md#user-decisions-2026-11-12),
[C2 finding](../core-v4-review.md#c2-worldengine-is-a-roughly-105-field-struct-and-transactions-clone-all-of-it),
[C1 endpoint](core-inner-engines.md#closing-outcome), and the
[engine decision](../../decisions/engine.md#full-state-world-contract-and-convergence).

Keep public types, paths, signatures and traits, the single ordered report
ingress, public command fields and `op()` tokens, frozen fixtures and canonical
bytes. Leave size/byte checks at admission; [C5](../core-v4-review.md#c5-the-json-wire-format-lives-inside-core)
owns codec/byte-budget extraction, C4 attempt wire unification and replacing
platform-facing string-valued `CoreCommand` fields. No adapter, product-policy,
layout algorithm or C8 facade changes. Keep public `WorldEngine: Clone` usable
for `crates/tiler-windows/src/tiling_sys/world_sequencer.rs::deliver_production`:
the [Windows v4 port](../v4-migration.md#4-windows-hostcapabilities) owns replacing
the staged `float-no-remap` veto with `tiledFloatTransfer`. Do not add a public
API for a platform to approve a candidate.

## Ownership and seams

| Question | Owner and answer | What it does not prove |
| --- | --- | --- |
| Where does this lifetime belong? | Workspace model owns rings, current/previous views, topology, displacement, modes and the sole lifetime/home registry. `world_members.rs::core_homing` applies displacement. | Native arrival, a layout leaf, or visibility. |
| Which node, bounds or restore slot? | `WorldLayout` owns trees, bindings, geometry, restore slots, layout recency, policy algorithm and generation allocator. | Logical home or lifetime end. |
| What did the host observe or attempt? | Accepted report facts; native settlement correlates bindings, creation, property attempts, visibility and transfer evidence with retained targets, keeping operation/switch outcomes. | A late native observation alone does not undo a logical action or prove failure. |
| What should be focused or saved? | Focus owns the execution subject, workspace return memory, cursor/hover and retained focus; intent owns import/provenance, admission fences and save/clear desires. | Observed foreground, applied focus or durable storage. |
| What may be painted or clicked? | Presentation projects from the world and owns pager observations and published revision; host maps actual hits/frame events. | Permission to choose a new subject, recompute a group or reinterpret an old click. |

- Diagnostic location is not a home lookup. `resolve_held_window_scopes`
  in `world_observe.rs` uses row output, retained tiled-binding output,
  then prior staged homing; no match means no scoped diagnostic.
  `world_state.rs::state_offroute_scope_for` uses fact output, held-row
  output, logical home (with displacement), then tiled-binding output.
  Preserve both precedences and narrowly named queries. Check an output-less
  held member after send/displacement: neither rehome it from a diagnostic
  nor hold an unrelated scope.
- `SettingsState` owns effective connection policy. Borrow policy values for
  operations and public getters; remove redundant engine mirrors after
  checking defaults. Any layout policy snapshot still needed is derived at
  accepted `handle_settings` (fixed-size predicate, fresh placement), not a
  second owner. Take the current cursor from the staged observation; refresh
  previous/current cursor history at accepted-report staging. If layout still
  needs the current cursor copy, refresh it from that same sample there
  (`world_run_v4.rs`), not on settings updates.
- Window runtime state owns the lifetime-local composed-maximize bit:
  maximize, operations and gestures change it under existing commit/rollback
  gates; lifetime retirement prunes it. Native-state handling reads it as
  policy input, not proof. It is not saved intent. Gesture owns binding/hover,
  terminal watermark/result and retained preview; a terminal commit is not
  native arrival. Focus feedback and creation stay outside C4 settlement.
- Keep connection/configuration and a single optional admitted replay entry
  outside candidates. The entry couples raw report, reply, digest and
  watermarks. Use sequence and digest to shortlist, then full typed equality,
  never digest or graded-fact equivalence. Readiness/revision/command marks and
  `PriorAcceptedContext` are candidate state; the latter is publication proof,
  not replay input.
- Keep prior staged data only for its readers: held-row attribution, bounds
  and pre-capture source freezing via `world_close_focus.rs::execution_subject`
  (availability, foreground and issuing revision). After new observation, the
  first implicit `HomeThenAction` send re-resolves its subject against current
  focus but uses the pre-observation home/action snapshot. Later commands
  freeze against post-prior-command state. Keep the positive-close freeze before
  native fallback and batch focus scratch separate from retained memory.

## Transaction boundaries

Keep gate order (`world_admit_v4.rs`), including conservative size/slot
checks before staging. The candidate borrows connection/configuration; on
success install world and replay together only after outbound validation,
exact canonical sizing and future reserves. Broker encoding still follows
adoption. Preserve commit-only settlement/preview traces.

| Path | What is visible |
| --- | --- |
| Exact ready replay | Cached reply, no convergence, new revision or batch settlements. A changed queued list alone still matches (D2). |
| Fresh, consumed inline command | Reconcile observations; skip command execution and renderer revalidation, with no queued batch executions. An individually consumed queued entry instead skips just that entry in a fresh batch. |
| Valid pre-ready bootstrap | Install bounded intent/import/cleanup state only after outbound checks; no world authority, command consumption, revision or replay advance. |
| Ordinary command refusal | Restore the post-observation/post-prior-command image. Then consume the refused sequence and apply existing focus/result rules. A stale renderer target can refuse at the same read-only gate before checkpointing. |
| Native sticky follow-up | Proven refusal may restore exact prior intent and home; ambiguity cannot. A blocked rollback retains proof. Applied OFF restores the outer save/result before marking completion. |
| Creation exhaustion or late outbound refusal | No candidate state, new replay entry or batch settlements. The generation-shared group allocator alone burns IDs on discarded trials; native creation, birth and command counters do not. |

- **Close, A succeeds, B refuses, then C** (`world_run_v4.rs`): Observation
  can retire membership/layout, settlement, focus and intent for the exact
  closed lifetime. Commands can mutate those same owners and staging. Drop
  B's trial writes, consume B and keep its refused result; install the close
  and A only at report adoption. C sees them and B's consumed sequence,
  not B's partial state. If final sizing refuses the report instead, the
  next valid report starts from the old committed image, with none of
  that close/A/B candidate installed; the broker reparks the unsettled batch.
- **Commandless sticky OFF** (`world_sticky.rs::run_native_sticky_followups`):
  Applied proof plus a fresh readable `sticky == Some(false)` and existing
  scope/safety gates permit the dependent transition. Writable owners include
  membership/layout, intent, native bindings/frame state and staging. On
  `NoEffect`, restore the outer save desire and command result, then mark
  the binding done and install into the report candidate. On failure keep
  the proof, not partial writes. The next report after adoption sees the
  resulting home/layout without re-running the follow-up; after a held or
  discarded trial, it can still process eligible proof. A sticky command's
  `NoEffect` with a typed rejected result is not `CommandApplied::Rejected`;
  preserve the distinct handling in the batch tail.

Use the same borrowed facts for checking and installing. Keep a bounded
broader checkpoint if narrowing would duplicate decision logic (D1); no
general undo framework. Public clones must not leak tentative replay,
settings or world writes through shared mutable handles; preserve the
group-ID exception. Successful rehandshake starts a fresh generation;
failed handshake leaves the old one usable.

## Presentation and tabs

- Preserve [anchored-border following](../../porting/world-contract.md#worldpresentation-and-anchored-border):
  host frame events move the same authorized anchor while its causal proof is
  valid; hide narrowly on loss of proof. Local paint is not zero traffic:
  KDE `PresentationStage5::onMotionFrame` still publishes invalidation.
- Preserve [active-drag continuity](../../decisions/world-rendering.md#active-drag-preview-continuity):
  keep X's valid preview through ordinary motion of the same gesture until
  core supplies Y. Structural invalidation, terminal or suspension hides it;
  release uses current coordinates, not X. Fullscreen suspension is per
  output and does not end the gesture. Preserve underlay layout `baseRev`
  separately from world/presentation revision.
- Keep the current one-visible-window-per-leaf assumption inside layout.
  Replace Presentation's reverse leaf index and `Node` walks
  (`world_presentation_v4.rs`) with layout queries for group description and
  group existence. `world_layout/queries.rs::window_for_leaf` returns the first
  binding; `world_layout/projection.rs` checks geometry count against bindings.
  Keep styles, suspension and freshness in Presentation. No tab representation
   now: [future tabs](../../decisions/visuals.md#tabbed-groups-and-post-01-gate)
   need visibility, focus, input and [protocol work](../../decisions/contract.md#decorations),
  but should not change report adoption, replay or the logical home registry.

## Migration and acceptance

1. Audit prior-stage readers and checkpoint write sets. Move connection,
   effective configuration and replay outside candidates first. Run the
   `world_stage_v4.rs` census on the shared read-only classifier and local
   stage. Bootstrap borrows closes, windows/process facts and intent rows;
   do not copy unrelated lanes via `capture_observation_v4`.
2. Split stable capture inputs from derived state. Build current classification
   and indexes once from the admitted capture; where a mutation requires
   `world_observe.rs::restage_classification`, update derived state without
   recopying raw/effective inputs. Share the effect/no-effect success tail in
   `world_run_v4.rs` only where ordering is identical.
3. Move workspace, settlement, focus/gesture, intent and presentation owners
   with their rules. Keep the engine call sequence visible: a few synchronous
   multi-owner functions for close, rekey or sticky are appropriate; no getter
   facades, dispatch layer or per-property settlement engines. Check the
   initial projection in `world_publish_v4.rs::publish_v4` before removing it:
   project once at `finish_v4` if no live consumer, keeping earlier style,
   focus and selection gates. No cache to hide duplicate work.
4. Apply D1 after mapping write sets. A renamed near-full clone per command
   is not the goal. Change private C1/world queries, move tests or remove
   intermediate types when they encode the wrong owner. Public compatibility
   is evidence, not a veto on private changes. For a contract discrepancy,
   record the shortest sequence in the [reference matrix](../../spec/reference-outcomes.md)
   and handle it explicitly; no shim, regenerated golden or blessing defects.

## Verification strategy

- Start from the existing public report/reply and command-batch tests, protocol
  transcript and frozen fixtures, and service rapid-input tests. Assert
  outcomes, next-report recovery and latest-only native authority, not struct
  placement or checkpoint internals. Keep existing focused layout tests for
  invariants that are not practical to drive through the world boundary.
- Preserve user-visible sequencing: observe once, run inline then queued
  commands in order against prior accepted operations, publish one complete
  latest world. An implicit subject is resolved at execution; withholding or
  retiring a focus request is not a second command or a change of observed
  foreground. Keep retained transfer, intent, focus and gesture authority when
  an intermediate publication is skipped.
- Core anchors: `world_connection_tests` (rehandshake, held rows, group-ID
  burns), `world_renderer_input_v4_tests` (current identity, old/future
  rendered revisions), `world_rapid_input_tests` (FIFO, refused command,
  implicit-send rehome), and `world_sticky_native_tests` (blocked rollback
  proof, OFF follow-up and overlay deferral). Reuse `world_admit_v4_tests` for
  bootstrap, late bounds, creation exhaustion and next-report recovery;
  `world_run_v4_tests` covers identical-batch replay without new settlements.
- Reuse protocol `connection_transcript_v4`, `raw_attempt_settlement_v4`,
  `world_eval_v4`, sizing twins and broker `rapid_input_burst`. Inspect what
  the next valid report sees after refusal, a skipped publication or a new
  generation. Use existing fault injection for unreachable bounds. Add a
  short boundary test only for a confirmed gap; no new comparison harness.
- At completion use existing format, scoped all-target build and strict
  Clippy, core/protocol/service tests and unchanged wire/KDE fixture checks.
  Target-gated caller compilation is separate from Linux portable evidence;
  retain C1's known macOS portable baseline diagnostics as a comparison point,
  not an exemption for new failures. No live desktop mutation for this
  behavioural-preservation check.

## Cost and size

- Run the existing `bash scripts/check-report-cost.sh` at implementation
  completion: steady release S1 at 50/200 windows, ceilings 24M/100M
  instructions per report and growth at most 6.5. Run the existing manual
  release `report_scale all 31 3 all` matrix for steady-state p95 at
  10/50/200 windows: 1/4/10 ms respectively; first reports are separate. No
  new timing assertion or CI gate. C1 recorded 9.26M/40.31M, 4.35 growth,
  and worst steady-state p95 of 0.3134/2.1135/5.0683 ms. Report C2 against the
  same C1 matrix, not only the ceilings. Preserve the conservative
  [unchanged-domain proof and linear hint work](core-per-report-cost.md#outcome-and-gate-verification).
  Profile a representative multi-command burst or sticky follow-up before
  choosing a complicated narrow checkpoint for speed; keep the simpler one
  if the gain is immaterial. The broker's existing 60/100-action cases are
  suitable scenarios, not timing gates.
- These are **core-only** measurements, not capture, IPC, native apply or
  paint latency. In `kwin/src/world-entry.ts`, changed gesture snapshots and
  terminals call `scheduleDispatch` (`WORLD_DEBOUNCE_MS = 120` if unarmed);
  this is neither per-frame latency nor a key delay. Scheduling and native
  evidence belong to the [KDE interactive checks](../kde-v4-acceptance-checklist.md#additional-kde-acceptance-and-diagnostic-legs):
  drag X to Y and back, then release; count motion notifications, captures and
  paints. Separate motion-to-dispatch, core and reply-to-paint; check flicker,
  stale repaint and release target.
- [Retained cursor warp](../retained-cursor-warp.md) belongs to the Windows v4
  port. `world_cursor.rs::warp_for_publish` still reads per-run effects, not
  the retained focus winner; C2 makes no skipped-publication warp guarantee.
- `world_engine.rs` is 4,119 lines on the C1 working tree. Split the entry
  point, owned state, and rule families rather than moving that monolith into
  a different file. Follow the [soft caps](../../principles.md#file-size), about
  1,500 production and 3,000 test lines: split touched files by responsibility
  where it improves review; explain remaining local exceptions. No new
  line-count gate or broad unrelated test/module reshuffle.

## User decisions 2026-11-18

1. **B - Bounded checkpoints.** Keep one report-wide candidate for atomic
   publication. Narrow command and sticky checkpoints only where doing so
   clearly simplifies code or removes material cost. Keep a bounded broader
   checkpoint when proving a narrow write set would add complexity.
2. **A - Report-only replay.** Preserve current matching in C2: an identical
   report with a changed queued list returns the cached reply, no batch
   settlements and no command execution. The
   [reference outcome](../../spec/reference-outcomes.md#typed-report-replay-with-a-changed-queued-list)
   stays TBD as a separate contract decision. A later batch-aware change
   affects `classify_replay_v4` digest/equality, tests and docs. Because the
   entry does not retain queued commands, exact matching also needs a
   bounded batch representation and verification of callers' repark behaviour.
3. **A - Capacity-blocked sticky rollback.** A blocked rollback preserves
   A's refusal proof for a later eligible retry and does not fence B when
   capacity allows. Do not adopt the discarded trial's saved-incomplete
   latch or change the other paths that set it. See the
   [reference outcome](../../spec/reference-outcomes.md#native-sticky-refusal-and-saved-intent).

The source paths and current ordering were inspected on the C1 working tree.
No implementation, tests, native checks or performance measurements were
run for this design. Checkpoint contents and any new coverage are implementation
proof obligations; no equivalence claim is made.

## Closing outcome

- The report transaction now borrows connection policy and prior transfer
  publication evidence; the typed replay entry is outside the candidate.
  Bootstrap imports only the needed report lanes. Census and restaging share a
  read-only classifier; restaging does not recopy stable capture inputs.
- The ordered command loop refreshes cursor history at staging, keeps the
  distinct effect/no-effect result timing, and checks stale renderer input
  before checkpointing. Checkpoints share immutable capture inputs but retain
  broader rollback where narrowing would duplicate policy.
- The retained sub-states are extracted; presentation queries layout and
  projects once on final publication. The engine entry point is separated
  from diagnostics, staging, intent lifecycle and lifetime cleanup.
- Independent final review found no blocking issue. The public surface,
  ordered core execution, report-only replay, latest-world apply and frozen
  world-wire/v4, stage5, stage5-v4 and kde-v4 fixtures are unchanged.
- Workspace format, scoped all-target build and strict Clippy passed. Rust:
  3,447 passed, with `workspace_unsafe_posture` excluded. The sole count
  increase is the sticky capacity boundary regression: A's proof survives,
  B proceeds, and A later completes without a new attempt. KWin typecheck
  and 1,378 tests, launcher 703, native-effect 204 and helper 58 all passed.
- Windows portable library check passed. The macOS portable check failed
  with the same seven diagnostics, verified against the C1 comparison
  revision: stale command-result vocabulary and a frame-setter type
  mismatch. Native Windows/macOS compilation was not run because the
  target libraries were unavailable. No live desktop checks were run.
- Callgrind passed: 8.11M/35.58M instructions per report at 50/200 windows
  (C1: 9.26M/40.31M), growth 4.39 (C1: 4.35). The release matrix covered
  108 case/shape repetitions and 3,348 measured reports. Worst steady-state
  p95 at 10/50/200 windows was 0.3871/1.9654/7.4005 ms (C1:
  0.3134/2.1135/5.0683 ms), all within 1/4/10 ms. The worst 200-window
  command shape's other repetitions were 4.6786 and 4.9984 ms. These are
  core-only costs; first reports and native latency are outside the budget.
- `world_engine.rs` is 1,188 lines. Existing larger rules files, including
  native reconciliation, operations, workspace convergence and sticky,
  retain their domain grouping; broader unrelated splits and large test
  suite reshuffles are outside C2.
