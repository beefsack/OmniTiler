# Windows workspace return focus

## Goal and scope

- Diagnose focus loss on an occupied -> empty -> occupied workspace roundtrip.
- Restore the approved workspace-arrival focus behavior using the exact-desktop-only explicit-selection exception approved 2026-10-27.
- Preserve host-caused focus-loss behavior, asynchronous target identity and supersession rules, all other fullscreen/game gates, and one-shot native focus semantics. No waits or timers.
- Investigate rapid-switch remembered-focus loss and latest-world rendering;
  record the rendering intent, but leave contract/architecture changes proposed.
- Compare command-time active-window ownership with the earlier transactional
  arrival proposal; preserve both designs and expose attribution limits.
- Implement the independent approved command-memory and Windows latest-world
  scheduling portions, failing-first. Keep existing foreground attribution,
  view authority and one-shot focus contracts where parked decisions intervene.
- Current direction: command-owned memory, prompt intentional focus updates and
  latest-only rendering; defer/reconcile is superseded. Reduce ambiguity before
  choosing its policy; see the [measurements and staged design](#ambiguity-reduction-measurements-and-staged-design-2026-10-27).

## Acceptance and approach

- Join the reported sequence to core target selection, native dispatch, and foreground observations.
- Reproduce with fixture-only scope at normal and rapid pace if the report cannot identify the refusal.
- If a bounded defect is proven, add a failing regression, apply the smallest correction, run relevant offline gates, and verify the fixture roundtrip and cleanup.
- Escalate unresolved policy or safety implications rather than changing the contract.

## Units and evidence

1. Report analysis: complete. Dirty build at `cab52ffd53b7554c4d66d7adbca50dbd06a55926`; provenance lists the same changed paths as the examined working tree, but does not prove byte equality.
2. Fixture reproduction: normal and rapid roundtrips restored focus; the reported refusal was not reproduced.
3. Trace-only refusal discriminator and regression coverage: delivered offline; no focus behavior changed.
4. Updated reproduction identifies readable fullscreen shell as the veto; approved exact-desktop explicit-selection exception implemented.
5. Behavioral red regression, implementation, adversarial identity/safety coverage, native gates and independent review complete.
6. Fixture normal-return verification passes. Subsequent physical recheck restores
   Terminal on ten reported slow roundtrips; the new trace exercises the exact
   desktop exception successfully. Rapid switching exposes a separate problem below.

The reported trace selects ordinal 2 (opaque identity `ws-4`) at line 1547 and returns to ordinal 1 at line 1574; lines 1593 and 1610 repeat the sequence. Core selects remembered `w6` on both returns (lines 1573 and 1609). Both dispatches report `setter:refused`, `primitive_called:false`, and no foreground proof; occupied-workspace pager observations remain unfocused (lines 1578 and 1617). A later foreground edge for `w6` at line 1635 precedes restored focused state at line 1646. That build's diagnostics cannot distinguish the native pre-primitive refusal branches.

## Material decisions and limits

- Workspace arrival still requires remembered focus or the approved eligible fallback; an empty workspace has no focused client.
- The updated trace proves readable fullscreen shell classification, not asynchronous show ordering, is the refusal cause.
- Existing unrelated changes remain outside this investigation.

## Accepted fixture evidence (2026-10-27)

- Fixture-only Windows scope, two owned windows, production workspace-select CLI, and no synthetic input. Rebuilt current source at the revision above; owner SHA-256 `F62694E8C35B6ECD1ADF4A461E03BE42AE2620A3D355DF9F5C694A0AD5B60D9E`, fixture SHA-256 `79C702D95F9C6BCDD3EEF1DDD40954AFD7FC0E73BF6A98B26D4AD99BB59433C7`.
- Normal roundtrip: `run-01dd65c19c4a86aa.log`, line 121, accepts the remembered target's native setter; subsequent foreground edge and independent native/fixture readbacks establish focus and both windows visible.
- Rapid roundtrip: `run-01dd65c204577453.log`, line 100, selects empty ordinal 2; lines 117-118 accept the remembered target's setter on return. Line 120 drains its foreground edge; later native and fixture readbacks both confirm focus. Request publication gap is 175.8 ms, including dispatch/apply proof and required control transport polling, without fixed sleeps or visibility/focus polling between selections. This is not a physical-shortcut or zero-delay-burst claim.
- An earlier leg labelled rapid retained a fixed inter-selection delay; it is not accepted as rapid evidence. The corrected leg above supplies that evidence.
- Each accepted leg shut down its fixture, stopped/restored its exact owner, verified clean recovery/control state and no owned process residue, and compared settings bytes and taskbar/SPI against preimages. Final cleanup reports contain no failures.
- The initial live investigation changed no production source or regression tests. The diagnostics follow-up below has separate offline verification; those live results predate it.

## Refusal diagnostics delivered (2026-10-27)

- Approved scope: trace-only attribution of the four pre-primitive `Ok(false)` branches; no behavior change, timers, retries, or weakened safety gates.
- `world_native.rs` records `refusal_gate` as `scope`, `classify-focus`, `focus-write-veto`, or `move-size-veto`. `refusal_reason` is the existing classification reason for `classify-focus`, otherwise null.
- `target_visible` and `target_cloaked` copy the already-observed target facts; `target_hidden` means a hide-claim ledger entry, not independent native invisibility. A target can become invisible between observation's entry check and facts sampling.
- `fg_identity` is one post-veto foreground HWND relation: `none`, `target`, or `other`. It is not full native identity proof, a window-class name, or a causal snapshot of an earlier gate read. The single foreground sample occurs only on traced refusals; successful and trace-off paths add no probing.
- `world_diff.rs` projects the detail into existing bounded dispatch rows; `world_tick.rs` emits the six new keys only for those refusals. Successful/skipped rows omit the keys, preserving their prior serialized shape. No new log event or per-tick stream is added; no captions, paths, or application content are recorded.
- Ten regressions cover all four branches, cloaked and raced-invisible targets, trace-off and success behavior, exact added foreground-read budget, and JSON projection/key absence. A failing JSON regression exposed unconditional null fields on non-refusals; the final conditional projection fixes that volume issue. A test-counter borrow panic was corrected without product changes.
- Locked native core/protocol/Windows build and full tests pass: 2942 passed, one existing ignored test. Strict all-target clippy, workspace format and diff checks pass. Stress parser/seam check, definition mock, and fault mocks pass; additive refusal fields do not change existing parser requirements.
- Focus-path review found no blocking issue; visibility-race and nullable-reason documentation were clarified. Final focused rerun passes all ten regressions. No live test was run after this diagnostic-only change, and the underlying refusal remains unexplained.

## Blocker and next evidence

### Updated physical reproduction

- `run-01dd65c5b93d7506.log` provenance identifies dirty `cab52ffd53b7554c4d66d7adbca50dbd06a55926`, owner SHA-256 `3AF38961539C70A87A0B79F8C67A70EB4D03EBD35A87410D079D04AA46CB1A3C`, with the new diagnostic source files present in its changed-file list. This does not prove byte equality with the examined tree.
- Earlier moves of `w6` are right (line 82), left (104), and down (127). Subsequent directional navigation ends with an accepted focus setter targeting `w6` (373), followed by its foreground edge (376). No trace evidence shows those moves making the target ineligible: at refusal it is visible, uncloaked, and has no hide claim.
- Selecting empty workspace 2 produces a desktop `Progman` foreground event (407). On return (426, repeated projection 432), remembered target `w6` refuses through `focus-write-veto`, before prime/native setter, with `target_visible:true`, `target_cloaked:false`, `target_hidden:false`, and post-veto `fg_identity:other`. Occupied workspace 1 is unfocused (431).
- A second roundtrip begins at 481 after roughly 19 seconds; its return again refuses the same gate (502, repeated projection 508), and remains unfocused (507). These are two distinct attempts, not four; paired projections repeat each attempt's evidence.
- No intervening foreground edge is recorded between `Progman` (407) and the later `w6` foreground event (522-523). The later observation and pager report focus (527, 532), with no new focus effect. Its external cause is not established; it does not prove either refused arrival setter landed.

### Remaining discriminator

The proven blocker is foreground safety, not target classification or core target selection. `Progman` is the last-observed competing foreground, but the diagnostic samples only a post-veto HWND relation. It does not bind the gate's own foreground read to that shell window or record whether observation succeeded.

`focus_write_vetoed` refuses a readable captionless full-monitor foreground, or fails closed after an unreadable foreground unless its narrow verified same-pass hidden-departure lane applies. A desktop surface meeting the fullscreen geometry predicate, or an unreadable desktop surface, can explain the evidence; the trace cannot choose between them. Prior fixture tests left ordinary external applications outside scope and do not establish coverage of this desktop-foreground case.

### Gate-time sub-branch diagnostics delivered

- Existing decision reads now populate `fg_veto_kind` (`readable-fullscreen`, `unreadable`, or `hidden-departure`), `fg_gate_identity` (the gate-time HWND relation, distinct from the later `fg_identity`), and visible-lane `fg_readable`. Available `fg_shell`, `fg_fullscreen`, and executable basename `fg_exe` accompany them. `fg_visible` is the readable observation's fact or the existing entry visibility decision on unreadable/hidden lanes. Unavailable facts are omitted, never fabricated.
- These fields appear only on traced foreground-safety refusals, within the existing bounded dispatch rows. No native probes, gate decisions, probe ordering, retry, wait, or timer changes were added. Success/off-trace paths build no payload and allocate no diagnostic basename. The observation seam still erases underlying failure reasons; `unreadable` does not claim to identify a particular failed Win32 query.
- Eleven additional regressions cover readable fullscreen shell/non-shell, visible/invisible unreadable foreground, stale and same-pass hidden departures, gate-time versus later foreground changes, trace/off-trace payload construction, and JSON projection/omission. A red success/off-trace storage regression drove removal of eager capture; lazy-builder coverage also prevents discarded off-trace payload allocation.
- Full locked native core/protocol/Windows build/tests pass: 2953 passed, one existing ignored test. Strict all-target clippy, workspace fmt/diff checks, stress parser/seam and definition/fault mocks pass. Independent review found no blockers and confirmed unchanged gate read order/outcomes.

### Fixture desktop-foreground investigation

- Rebuilt dirty source at the revision above; launched owner SHA-256 `3AE24A136525CEBB8394ED47DBBD23F43C8D473C876D3BC47E0B057B3A6FEC29`. Fixture-only scope used three windows, directional moves right/left/down and focus navigation, ending with independently verified fixture focus before selecting empty workspace 2.
- No-click condition: the empty workspace foreground became an ordinary external application, not the desktop. Return focus was accepted (`run-01dd65ca26b8627f.log`, line 346; paired projection 355), and later native plus fixture observations confirmed remembered-target focus and visibility. This is a negative repro, not coverage of the desktop-foreground condition.
- Desktop condition: exact shell identity was verified before a no-input `SetForegroundWindow` attempt. The API returned false and foreground stayed on the external application. No synthetic desktop click, input fallback, foreign-app hiding, or gate change was attempted. The required desktop-foreground condition is blocked, not passed. Return afterward again accepted focus (404, paired projection 416), but did not test the hypothesis.
- Independent read-only shell evidence: the `GetShellWindow` surface is class `Progman`, owned by the verified Explorer process, top-level root with no parent/owner; taskbar is a distinct `Shell_TrayWnd` in that same process. Shell style `0x96000000` has no caption/child bits; extended style is `0x00200080`. It is visible, not iconic, uncloaked, with PMv2 outer and DWM frames matching the monitor's full bounds. These facts satisfy the fullscreen geometry shape; they are later readbacks, not a recorded live veto or proof of gate-time readability. Unaware monitor enumeration was virtualized and is not used as physical-coordinate proof.
- Both conditions completed exact-owner stop/restore and fixture shutdown, byte-identical settings restoration and taskbar/SPI checks, and no owned process/recovery/control residue. The first driver run stopped on an already-focused navigation no-op's missing apply row; joining that step to its dispatch and native observations repaired the external verifier, then the full run completed in a new evidence directory. No product change followed that harness failure.

### Minimal reproduction and accepted diagnosis

`run-01dd65cae8797264.log` requires no moves: select empty workspace 2 (line 70), then return to workspace 1 (97-98). At line 77 the foreground is `Progman`, identified by the separate pager probe as an exact readable desktop handle. Return refusal 97 (paired projection 103) records `readable-fullscreen`, `fg_readable:true`, `fg_shell:true`, `fg_fullscreen:true`, `fg_visible:true`, and `fg_exe:explorer.exe`. Core target `w6` remains healthy; the native focus primitive never runs. Corrective foreground at 138-139 and subsequent teardown are post-reproduction. The focus trace does not itself bind a foreground HWND to the earlier exact-desktop probe; the new exception must establish that proof live rather than trusting class or basename.

### Approved exception and implementation plan (2026-10-27)

- Authority is only the fresh accepted arrival effect of explicit workspace selection; observation, automatic admission, directional focus and other focus causes receive no desktop exception.
- Prove fresh foreground equality with the nonzero live `GetShellWindow()` handle, verified process identity/lifetime, matching user/session, and the system-shell executable. Recheck stability before actuation; unknown/mismatched proof or foreground supersession refuses. Existing target, hook, elevation, modal, lifetime and one-shot gates remain intact. Unsafe native calls stay within existing minimal unsafe modules.
- Write a failing desktop-return regression before the fix. Cover competing fullscreen game/unknown app, unreadable foreground, spoofed Progman, other Explorer/shell surfaces, recycled HWND/process identity, foreground-to-game change, and observation-only desktop focus loss.
- Run locked native core/protocol/Windows build/tests, strict all-target clippy, workspace fmt/diff and stress parser/mocks. Review safety invariants independently, then run a fixture-only normal-return leg without foreign input or hiding. Do not claim exact-desktop native acceptance if the unmanaged application remains foreground.
- The decision is recorded in [fullscreen/focus safety](../decisions.md#window-state-float-sticky-maximize-fullscreen). No focus sink, departure choreography, broad shell exemption, retry, timer or geometry write is authorized.

### Implementation and accepted evidence

- `SelectionArrivalGrant` binds the exact fresh accepted selection command and sequence to its explicit core focus effect id and target. Admission (`af-*`) and cursor-follow (`ffc-*`) effects can accompany a select reply, so reply-level selection alone is insufficient: only the sequence's explicit `fx-*` pair qualifies. Duplicate/command-consumed deliveries, observation, directional and other commands mint no grant.
- The native focus gate permits only a readable foreground proven equal to the nonzero current shell handle, with agreeing observed HWND, PID/creation, executable, user and session across live reads. Expected Explorer path comes from the Windows known-folder API, not an environment variable or basename. Fullscreen/class flags do not establish authority. Unsafe native calls remain in the existing FFI module.
- A desktop proof is re-established after the existing bounded input prime. The exception path rechecks hook state, fresh target eligibility/scope/identity/tag/lifetime and modal fences; optional trace GUI probing precedes the final desktop reproof, whose foreground/shell/PID bracket follows identity/path reads. Any drift or unreadable proof refuses without setter/retry. Geometry/frame safety is unchanged.
- `desktop_exception:true` marks passing the desktop gate, not native execution or arrival; later rechecks may still refuse. Such refusals honestly retain the actual `prime_inserted` count and use `arrival-recheck` when desktop stability fails. Rows not using the exception omit the marker.
- The first behavioral regression failed before the fix because the return effect dispatched but never reached the setter past a desktop foreground. Thirty-two added tests cover positive arrival, games/unknown fullscreen apps, unreadable surfaces, spoofed/nonexact shell surfaces and other Explorer windows, wrong executable/user/session, recycled process/handle proof, hook/modal/target changes, foreground-to-game races, and post-prime shell/process/path/session drift. Non-selection/observation paths and automatic effects remain ineligible.
- A real core away/back command pipeline produces the arrival effect and proves grant binding through dispatch, delivery, apply and acknowledgment, including duplicate/redundant/observation/non-selection negatives. It is not an end-to-end `run_world_command` test: the synthetic high-level tick helper refused the back command unchanged, so the back leg drives the underlying production functions directly. `command-consumed` is covered at grant-construction level. No acceptance oracle was weakened.
- Independent safety review found effect-id-only authority, automatic-effect mixing, missing observed-HWND equality and pre-actuation recheck/trace gaps in the first implementation; these were corrected and the closure review accepted the final scope. No unresolved safety finding remains.
- Locked core/protocol/Windows build and tests pass: 2985 passed, one existing ignored test; strict all-target clippy, workspace fmt/diff and stress parser/seam plus definition/fault mocks pass. The trusted-directory addition is confined to `ffi/process.rs` and its safe export.

### Normal-return native verification

- Latest semantic implementation rebuilt from dirty `cab52ffd53b7554c4d66d7adbca50dbd06a55926`; launched owner SHA-256 `CB58D750D3223BFF454E96AC5DCDD91D583BB50E09AD7CD85AAC64FC49E19484`.
- Fixture-only two-window WS1 -> empty WS2 -> WS1 passes. Departure native/fixture focus agrees; both windows hide on WS2. The unmanaged external application remains foreground there. Return `run-01dd65d59b951af3.log`, lines 125/135 (paired projections), accepts the remembered target's setter; independent native and fixture observations prove focus and both windows visible. `desktop_exception` is absent, so this proves the existing normal path, not the newly authorized desktop path.
- Fixture shutdown, exact-owner stop/restore, settings bytes and taskbar/SPI restoration, ready-false and no owned process/control/recovery residue all pass. No foreign input or shell activation attempt occurred. This fixture leg does not close exact-desktop acceptance; the subsequent physical recheck below supplies it.

### Physical slow-return acceptance

- User physical recheck reports ten successful slow WS1 -> empty WS2 -> WS1
  roundtrips restoring Terminal. In `run-01dd65d67a297aaf.log`, slow returns at
  lines 76, 126 and 178 use `desktop_exception:true` and accept the Terminal
  setter with immediate foreground proof. Paired projections repeat these
  attempts; the later spam section uses no desktop exception.
- Provenance identifies dirty `cab52ffd53b7554c4d66d7adbca50dbd06a55926`, owner
  SHA-256 `0D8C998752E4DF7964A51468FF097478D1B3CE97DC86FC3B95E1ABF3DF5B3DD6`.
  The changed-file list agrees with the inspected tree; byte equality is not
  established. The slow-return exception is accepted; rapid focus memory and
  rendering remain open, so this record stays active.

## Rapid switching and latest-world rendering (2026-10-27)

### Scope and acceptance

- Preserve the user's last focused Terminal through intermediate unlanded
  arrivals; distinguish this from a genuine user/host focus change. The
  [minimal discriminator](../spec/reference-outcomes.md#rapid-unlanded-workspace-arrival)
  records the ambiguity without inventing reference-WM evidence.
- Establish native actuation versus supersession from deduplicated trace rows;
  distinguish focus attempts, proven landings, view writes and visibility posts.
- The subsequent binding unfinished-arrival constraint is recorded in
  [decisions](../decisions.md#latest-world-rendering-2026-10-27). Its proposed
  design is below; implementation depends on explicit boundary choices.
  Resolve the explicit boundary choices before failing-first implementation,
  native gates and fixture-only zero-delay switching/stress verification.

### Accepted diagnosis

- Trace aliases: `w20` Terminal, `w21` Steam, `w22` Firefox. Spam is lines
  599-783, sixteen selection commands in approximately 1.4 seconds. Join
  `world-apply` projections by request/revision; pager-bound copies are not new
  attempts. Include intervening observations when counting native writes.
- Terminal is still observed focused at lines 739/743, revision 105. Return
  revision 107 (747/757) requests `fx-36 -> w20`, but stops mid-apply with
  zero writes, views, visibility posts or dispatched focus. No Terminal native
  setter ran and no Terminal in-flight identity was created there.
- Observation revision 108 (761/769) completes retained view `op-36`, revealing
  three windows with nonactivating `ShowWindowAsync` posts. It carries no focus
  effect. Steam show/foreground edges follow at 762/763. The original focus
  effect is not replayed on observation, as required by the current contract.
- Next departure revision 109 (771/779) truthfully reports Steam foreground.
  `world_capture.rs:1686-1690` maps the live HWND to its token;
  `session/world.rs:1919-1950,2021-2035` converges observed focus and updates
  focus history and `last_active`. Thus Steam replaces Terminal's memory.
- Final return revision 110 (782/790) selects `fx-38 -> w21`. Core selection
  tries remembered focus before MRU/layout (`world_tiling_views.rs:219-275`;
  `session/world.rs:1038-1073`), so this is consistent with overwritten memory,
  not evidence of an MRU/layout fallback defect. The trace does not label the
  chosen candidate branch directly.
- That Steam setter is accepted but immediately unproven. Its foreground edge
  at 785 and `landed-own` flight resolution at 794/797 establish later landing.
  Earlier spam Terminal setters all prove immediate foreground; no old Terminal
  flight was waiting to be resolved. The exact-identity flight rules are not
  implicated by this sequence.
- Steam's first foreground cause is unknown: the trace cannot distinguish OS
  activation during visibility churn, client activation, or a genuine click.
  Cursor-over-Terminal at 771 is not mouse-input proof. Intermediate rendering
  and split view/focus completion expose a plausible causal path, not proof that
  coalescing alone would prevent the loss.

### Rendering decision search and measured gap

- No general process-every-command/render-only-latest-world decision was found
  in decisions, principles or change records/archives before the new statement.
  Existing narrower precedents: Windows' three latest-wins overlay frame slots
  (`decisions.md`, renderer stage 2; `archive/windows-render-thread.md`), macOS
  per-app desired-frame batching, and core retained view replacement per output
  (`world_tiling_views.rs:108-143`). None establishes general Windows foreign
  window actuation coalescing. Principles already require event-driven work,
  causal freshness and no settle-delay workaround.
- `workspace_navigation.rs:47-156` observes, commands and applies for each
  selection in order. `world_diff.rs` gates and applies each accepted reply's
  frames, membership/views and one-shot effects, rather than publishing a
  general latest-only native target. Freshness may stop a pass; retained views
  may be completed later or replaced. Those are not full-world coalescing.

| Spam actuation, unique request/revision | Result |
| --- | --- |
| Eight WS1 arrival commands | Five intermediate arrivals focus Terminal immediately (94/96/98/100/104); one intermediate arrival reveals only on observation 108 (command 107); one never actuates (102); final arrival 110 focuses Steam asynchronously |
| Eight WS2 departures | Seven native view changes; revision 103 makes no native change because WS2 is already shown |
| All selection and observation writes through revision 110 | Seven occupied-workspace reveals and seven hides: 21 show posts, 21 hide posts; six focus setters; zero geometry writes |
| Supersession versus deferral | Arrival 102's retained target is replaced without native writes; 107 is deferred, not permanently superseded; 103 is a native no-op, not a stale-drop failure |

- Therefore six of seven intermediate occupied-workspace arrivals actuate
  visibility, five also actuate focus; only one intermediate arrival is fully
  skipped. The final occupied arrival actuates visibility and focus. Focus and
  visibility are not latest-only. Geometry is also evaluated per reply in code,
  but settled frames require no geometry writes in this sample, so the trace
  does not measure geometry coalescing or its cost.

### Proposed direction, not approved implementation

- Superseded by the detailed design and explicit decision list below. The
  earlier accept-every-departure-memory alternative is no longer compatible
  with the binding unfinished-arrival constraint.

### Verification and next action

- Independent read-only review confirms the log chain and corrected counts,
  including deferred observation actuation and the native no-op departure.
- Existing characterization tests
  `remembered_focus_unproven_without_landing_loses_final_focus` and
  `remembered_focus_veto_without_retry_loses_final_focus` pass (two tests).
  They document related known limits, not a new regression for this sequence.
- No production code or test changed. Full native gates and new zero-delay
  fixture/stress runs are not claimed for this investigation. Documentation
  whitespace and link checks pass.
- Next: resolve the design choices below before implementation. The new design
  investigation itself runs no product tests or live desktop legs.

## Arrival memory and latest-world design (2026-10-27)

- Earlier proposal retained for comparison, not adopted. The subsequent
  [simpler-model comparison](#command-owned-active-window-model-comparison-2026-10-27)
  reconsiders it; the [defer/reconcile design](#defer-and-reconcile-design-2026-10-27)
  now records the selected semantics and remaining implementation choices.

### Binding invariant and proposed ownership

- **Binding:** a superseded unfinished arrival commits no world-state changes.
  This includes foreground observations throughout it and the observation on
  the departure that ends it, not merely the final departure recorder. Freezing
  only `last_active` while still mutating MRU, active-leaf history or future
  admission anchors would violate the rule.
- **Proposed mechanism:** core retains committed policy state and a bounded
  arrival-owned provisional delta. Intermediate observations remain truthful
  facts, but their arrival-derived policy consequences cannot leak into
  committed state or subsequent remembered-target selection. Completion commits
  the eligible delta once; supersession discards it. Do not mutate and later
  restore a whole-world snapshot: that would erase unrelated newer work.
- The provisional write set must cover every arrival-derived policy mutation,
  not just focus: `focused_domain`, `focused_leaf`, `focus_stack`, `last_active`,
  output/visit history,
  admission/placement anchors and view/geometry observation echoes. Explicit
  navigation still needs an effective desired view for resolving each command;
  it must not be confused with a completed visit. The exact history boundary
  and independent-event treatment are open choice D3, not an implicit exception
  to "no world state."
- Raw inventory/foreground and native attempts must stay observable for safety;
  the proposal does not hide Steam from the report or pretend Terminal holds
  foreground. Fact ingestion is distinct from committing an arrival's policy
  consequences. A genuine independent close, click or command needs its own
  boundary, as listed below; occurrence during an arrival is not automatic
  permission to commit that arrival.
- Placement follows [Layering](../principles.md#layering): core owns arrival
  lifecycle, commit policy, memory/history and target choice. Windows owns
  source observations, native identity proof, input provenance when available,
  render scheduling, safety checks and honest attempt/postcondition receipts.
  A Windows-local focus filter would be misplaced policy.
- **A shared-core change is needed, but not yet approved for implementation.**
  Recommend a generic opt-in capability for asynchronous arrival settlement,
  with platform-neutral epoch/progress evidence; no Windows checks, HWNDs or
  shell exception in core. Other adapters retain their existing contract until
  they supply the capability. Shared wire/configuration compatibility and
  versioning require an explicit implementation proposal, not a silent field.

### Causal lifecycle and departure

1. On an accepted effective selection change, core creates an arrival epoch
   before arrival-derived state can commit. Identity includes core generation,
   adapter incarnation, monotonic selection/arrival sequence, logical output
   scope and destination workspace lifetime. It binds the chosen target's
   lifetime, required view/member operations and explicit focus effect if any.
   Empty/minimized-only destinations also have an epoch despite having no focus
   effect. These are proposed semantics, not existing API fields.
2. Baseline is the destination's last committed policy state. Candidate lookup
   uses that baseline: eligible remembered focus, else eligible MRU, else layout
   order. An earlier unfinished visit cannot seed the new visit's baseline.
   Effective desired navigation may advance for every command without making
   an unfinished visit committed history (D3).
3. While pending, observations update factual/provisional state only where the
   arrival could otherwise affect policy. Guard this **before** observation
   convergence, including the report carrying departure: current
   `world_engine.rs:1385-1388` converges before executing a command, and
   `session/world.rs:1919-2035` currently updates focus history unconditionally.
4. A completion certificate must match the current epoch and satisfy D1's
   postconditions in a fresh identity/view/visibility bracket, with no newer
   superseding command, generation change or safety invalidation. Duplicate,
   foreign, stale or mismatched certificates commit nothing. Completion does
   not follow from elapsed time, a setter return, a command ack or a show post.
5. An accepted command that actually navigates away supersedes the pending
   epoch. **Departure records no replacement active-window memory for that
   visit:** retain the committed baseline; discard its entire provisional
   arrival delta. Do this atomically with handling the departure report, so its
   foreground cannot first enter MRU/history. A rejected command is not a
   departure; redundant same-view selection does not mint/reset an epoch or
   fake completion. Scope migration/removal/reset invalidates the old epoch.
6. A finished visit returns to ordinary verified-departure recording. A newer
   visit to the same workspace starts a distinct epoch; A -> B -> A cannot
   resurrect A's old epoch. Shared-view mode needs a group epoch/dependency set,
   not independent output commits that split one core-selected shared view.
7. A native refusal, lost effect, target invalidation or unreadable proof must
   not be labelled finished. Recommend an unfinished-without-actuation state:
   keep the commit guard until departure/explicit supersession, but schedule no
   retries, timeout, polling or busy loop. Independent command/lifetime handling
   follows D2/D3; target invalidation never authorizes an automatic fallback
   focus effect. The next explicit selection uses current candidate eligibility.

### Completion, in-flight identity and existing rules

- **D1 recommendation:** finish when the current arrival's required logical
  view/member/visibility postconditions are proven, plus intended focus proven
  if a focus target exists. Not merely focus setter accepted or effect spent.
  Unrelated geometry/overlays need not finish first; arrival-required reveal
  geometry/hidden-write fences still apply. An empty/minimized-only arrival
  needs view/visibility proof but invents no focused client.
- An already-satisfied view/foreground may complete via a fresh current-epoch
  postcondition read; no gratuitous setter is required. This read must pass
  the same epoch, lifetime, target eligibility and safety checks as issued-focus
  proof. A different ordinary/fullscreen/game foreground cannot satisfy the
  intended target's focus postcondition, and unknown proof cannot finish it.
  An intended eligible fullscreen member retains existing explicit-selection
  rules, not a new blanket exclusion or exemption. A foreground edge alone
  cannot finish an epoch. Windows WinEvents do not carry our arrival/effect ID;
  tagging at drain time does not prove event origin. Store exact dispatch
  identity and revalidate current epoch/postconditions rather than fabricating
  an event-to-command causal stamp.
- Keep **native flight accounting separate from policy completion**. Current
  `world_diff.rs:2297-2369` resolves one accepted native focus by full identity
  and membership lifetime; retain that landing/supersession discipline. A late
  old flight may resolve diagnostically after its arrival is superseded, but
  cannot commit memory, revive the epoch, grant a new desktop exception or
  trigger corrective focus. It cannot be cancelled once issued.
- In ABA reuse of the same target identity, an old edge is not new-epoch
  completion proof. It may wake a fresh bracket that independently proves the
  latest desired postconditions already satisfied; that is a new state proof,
  not attribution of the old setter to the new epoch. If ordering or identity
  is unknown, keep memory uncommitted. No HWND/WindowId equality heuristic.
- R-WS-09 reads committed remembered focus; the 2026-10-26 MRU/layout fallback
  operates only when that committed candidate is no longer eligible. No
  pointer-based target fallback or automatic retry is introduced.
- No-forced-refocus remains binding: desktop/taskbar/minimize/competing-focus
  observations are truthful and stop unsafe/stale focus writes. Preserving old
  memory does not mean continually focusing it. A later explicit selection may
  request focus under the existing gates; observation alone never does.
- Exact-desktop authority remains native and effect-specific.
  `SelectionArrivalGrant` (`world_diff.rs:339-408`) binds accepted command,
  sequence, effect id and target; current one-shots are invalidated by newer
  input (`world_report.rs:209-219`). Never transplant that grant into a later
  observation/latest reply or extend an old effect's validity. D5 proposes
  new core revalidation for a still-current explicit selection **before its
  first native attempt**, not reuse of that grant. Without D5, a stale final
  focus effect remains dropped; view can converge while focus stays unfulfilled.

### Edge intersections: recommendations, not newly selected policy

| Edge | Recommendation / consequence | Status |
| --- | --- | --- |
| Genuine user click on B while arrival to A is unfinished | Respect actual B foreground and cancel pending focus authority; do not silently call the arrival finished. If navigation then aborts it, discard provisional B memory too. Treat independently proven user input as a separate intent only under D2; pointer position or a bare foreground edge is insufficient | D2 open; no click exception selected |
| Setter already issued, lands after departure | Do not undo/compensate or replay focus. Record actual landing, keep old epoch aborted and its memory delta discarded. Later latest-view convergence still uses current safety gates | Mandatory authority/safety limit; D4 clarifies guarantee |
| Host/client activation during pending arrival | Report actual focus, withhold completion without required proof, and preserve baseline on departure. Do not infer genuine intent from timing or app identity; stop/revoke any unissued focus where current gates require | Within invariant; independent-event classification D2/D3 open |
| Competing fullscreen/game/unknown foreground appears | Run immediate safety veto/invalidation regardless of provisional memory; no priming, focus recovery, frame/overlay takeover or interception expansion. Epoch remains unfinished; its departure cannot record the competing game as remembered focus merely because it was observed. An explicitly intended eligible game member keeps existing selection gates | Safety fixed; real-game acceptance separately scoped |
| Target closes, minimizes, rehomes or scope disappears | Consume safety/lifetime facts; invalidate affected native authority, never resurrect dead windows on delta discard. Recommend independent lifetime changes survive, but arrival-derived history does not; fallback only on next authorized intent | D3 open for world-state boundary; no safety weakening |
| Explicit focus/move/send during pending arrival | Process the command exactly once against core-effective navigation plus factual subject/lifetime checks. Do not let provisional memory choose an unrelated anchor. Recommend separate intent ownership; aborting arrival cannot undo that command | D2/D3 open; no swallowed command |
| Foreground proof arrives after a navigation command is already pending | Completion must not jump ahead of earlier accepted navigation. Process source-ordered intents and invalidations first; no timestamp/queue-age guess at historical completion. Delayed evidence cannot retroactively finish an aborted epoch | Conservative ordering recommendation |
| Multi-output/shared-view, unrelated output command | Invalidate only intersecting scope/dependency epochs; retain unaffected arrivals. Shared-view selection has one group boundary. A fullscreen veto remains immediate and can suspend broader native work as today | Design mechanism; regression coverage required |

### Latest-world actuation: proposed event-driven owner loop

- Recommend retain the existing native owner thread first, not add a foreign
  window render thread. Split ordered core incorporation/publication from
  native application as an explicit architectural change; do not just skip
  `observe_and_command` calls. The current sequencer allows one report flight
  and acknowledgements prune exact carried attempts
  (`world_sequencer.rs:93-158`); incorporation ack must stay distinct from
  native/arrival completion.
- Owner cycle: service safety invalidations and truthful observations; process
  available source-ordered commands through core; publish latest immutable
  desired snapshot and dependent operation/effect envelope; perform a bounded
  native work slice; use existing owner wakes to deliver attempt facts and new
  proposed arrival-progress protocol evidence back to core. No arrival-progress
  field or completion receipt exists today. If
  commands arrive during a slice, stop obsolete work at the next safe boundary,
  incorporate those commands, and replan from latest desired plus actual state.
  Never recursively dispatch inside a Win32 call or partially constructed core
  transaction. An uninterruptible native call is a latency bound, not proof of
  cancellation or permission to render the rest of its obsolete world.
- Bounded slices provide fairness, not a settle deadline: at least one eligible
  latest-world work unit gets service between bounded intake slices; pending
  work self-signals an event, otherwise the owner sleeps. No periodic polling,
  busy spin, unbounded queue drain, debounce or new settle timer. Exact work
  budgets are ordinary implementation choices, not completion heuristics.

| Replaced before native dispatch | Preserved / cannot be elided |
| --- | --- |
| Unissued ordinary frame targets, pure show/hide state and presentation for obsolete snapshots, respecting per-property authority and cross-output dependencies | Every ordered core command, independent lifetime/input facts, safety invalidations and exact incorporation/operation receipts |
| Pending pure workspace-view presentation target, with core-specified supersession | Send/migration membership phases, closes, possibly-executed operations, recovery ledger obligations and their barriers; final frame targets may coalesce only around those dependencies |
| Obsolete unissued arrival focus authority, marked superseded rather than applied | Already-issued native calls, spent effect identity, input priming side effects, asynchronous completions, full native flight resolution and truthful partial results |

- Replace whole compatible snapshots/dependency groups, not independent fields
  from unrelated revisions. Only core labels logical operation supersession;
  platform decides mechanical scheduling within that authority. There is one
  latest pending pure-state target per dependency group, not an unbounded queue
  of rendered worlds. Preserve unresolved operation evidence until exact ack,
  using existing lifecycle bounds rather than dropping it to save memory.
- Before each effect/native write, check latest publication, current input and
  invalidation watermarks, generation, identity, scope and all existing gates.
  Claim one-shot authority before dispatch; if superseded before priming, do
  nothing. Recheck after priming as today; if superseded then, record that
  priming happened but do not issue the stale setter. Priming is not repeatable
  work and creates no memory/completion authority.
- Previously issued asynchronous hides/shows may land late; neither superseding
  their target nor observing their completion makes the old world current.
  Rebase from fresh native state and converge only latest authorized state.
  Do not blindly replay an ambiguous membership/view operation or assume a
  posted hide/show finished. Freshness stops, hidden geometry, at-most-once and
  foreground/game gates remain intact through replanning.
- **D5 recommendation:** retain core-owned latest explicit-selection intent
  separately from its expired reply/effect. If the epoch is still current and
  **no focus priming or native focus setter attempt has begun**, fresh evidence
  may let core
  retire the stale unissued effect and issue a new revision-bound authorization
  for that same intent. Track retirement and at-most-one focus attempt at arrival
  level as well as effect-id level; reminting ids must not bypass at-most-once.
  This changes the present observation-no-effect contract and needs approval.
- Require positive owner-bound proof that the retired effect was never claimed
  for native dispatch. An already-spent/claimed effect cannot receive a successor
  even if preflight called no primitive; unknown after a lost acknowledgement
  or reset is not proof of non-execution. Retirement and the shared arrival-level
  dispatch claim must fence both old and successor ids atomically.
- Revalidation requires current scope/target/command provenance, fresh native
  facts and the original explicit selection still unsuperseded. New conflicting
  input, safety veto, proven external focus loss/competitor, or generation/
  target-lifetime change revokes continuation. A pre-existing departure desktop
  is not a newly proven loss; unknown cause must not be labelled benign product
  activation. When source ordering/provenance cannot distinguish a new competing
  change, withhold continuation, preserve memory and expose the gap.
- An observation can carry proof for existing explicit intent, but cannot
  originate focus authority on its own. Windows would need a newly verified
  selection-origin grant bound to core's current reauthorization, not
  `SelectionArrivalGrant::for_accepted` called on an observation or a copied
  original grant. Re-prove the exact desktop live if relevant; non-selection
  automatic effects remain ineligible. This grant/protocol extension is part
  of D5, not an already-approved widening of the desktop exception.
- Once priming/dispatch has begun, or a genuine host-loss/safety refusal cancels
  intent, do not reauthorize/re-prime/retry it. Already-issued flight can settle
  diagnostically and prove current postconditions only under D1; it cannot
  regain actuation authority. No-forced-refocus and game gates outrank final
  focus convergence. Even with D5, unreadable or vetoed cases remain honestly
  unfulfilled; tests must not mask them with sleeps/refocus/weaker oracles.
- Arrival transaction policy applies across **every** publication and command,
  including worlds never rendered. Coalescing is not a substitute for the core
  memory guard; conversely, a guarded memory alone does not eliminate native
  show/hide churn. Publish completion facts back to core with the epoch, never
  mark a skipped intermediate world finished to clear the guard.

### Proposed protocol boundary and implementation units

- Conceptual extension, not current signatures: core-minted arrival epoch and
  required postconditions in desired output; source-bound native progress and
  current-state proof in reports; core completion/supersession status in replies.
  Native keys remain adapter-local opaque lifetime evidence. Core commits or
  aborts, not the adapter. Missing/unknown evidence never implies completion.
- Bind each proof to generation/incarnation, epoch, scope/workspace/target
  lifetimes, relevant operation/effect identities and a fresh observation
  bracket. Native event metadata cannot retroactively manufacture these links.
  No global revision-only comparison: unrelated output progress must not falsely
  complete/cancel a scope. New relevant intent invalidates old one-shot authority.
- Units after decisions: (1) generic core transaction/proof schema with red
  policy tests; (2) simulated adapter and protocol schedules/compatibility;
  (3) Windows source evidence and bounded latest-state drain with preserved
  fences and explicitly approved D5 grant changes if selected; (4) independent
  safety review and offline gates; (5) fixture
  zero-delay acceptance. Accept each evidence seam before native code depends
  on it. No production/test source is changed in this design investigation.

### Failing-first test plan

| Layer / case | Required discriminating assertion |
| --- | --- |
| Exact spam regression | Commit A on WS1; leave; start return A; invalidate before focus; later observation reveals WS1 without focus; observe B; depart with B foreground; return. Assert final target A and unchanged committed MRU/active leaf/history/anchors through the unfinished visit, including before its departure, not just final focus |
| Baseline versus independent work | Several unfinished visits and another output's completed visit; interleave send/close/focus commands. Aborted arrival delta cannot leak, and discard cannot roll back independently owned state; pin D2/D3 outcomes first |
| Completion cases | Setter accepted/unproven, show posted/unconfirmed, view-only reveal, already-satisfied proof with target eligibility/safety, empty/minimized-only, hidden target, partial membership and delayed unrelated overlay. Only D1-qualified proof commits once |
| Core-only ordered/batched/reordered schedules | Same command order, different observation/proof delivery order, duplicate command-consumed replies, ABA same-target epochs, old incarnation/generation, out-of-order completion after departure, shared-view and output removal. Exact final memory/world oracle, zero old-epoch commits; use causal tokens, no timing |
| Flight and game races | Issued focus lands after departure; old A edge while new A epoch exists; unreadable/recycled lifetime; competing ordinary/fullscreen/game foreground before prime, after prime, before setter and before certificate. No stale setter, memory commit, corrective focus or false causal attribution |
| Coalescing and fairness | Ten commands while native slice is busy: all commands incorporated once; no queued obsolete pure-state write executes after supersession fence; latest state applied, operation barriers/receipts preserved. Old issued writes may finish truthfully; no false bound of one total native call; sustained input cannot starve safety or latest render work |
| Exact-desktop authority | No grant transfer across observations/replies/epochs; af/ffc/directional/send-follow/duplicate/consumed paths ineligible; same live shell and target rechecks retained after prime; incomplete arrivals never resurrect grants |
| D5 unissued continuation | Force final reply stale before dispatch claim/prime/setter; fresh current-epoch evidence can reauthorize only if approved and original unclaimed state is proven. Old id/grant stays dead; duplicates cannot double-attempt. After spent claim, preflight refusal, prime, setter, lost execution proof, host loss, competing game, new conflicting input or epoch change, continuation cannot issue; observation with no explicit intent emits none |
| Native/oracle integration | Match current native frame/visibility/foreground and actual pager payload to exact world proof. A skipped intermediate reply need not have native application, but cannot be reported applied/finished; strict omission/identity/receipt joins remain |

- Run existing platform-neutral core-only cases under ordered, batched and
  reordered schedules; add the causal unfinished-arrival fixture/oracles rather
  than claiming existing schedules already cover this policy. If schema changes
  require count updates, explain the source events; never relax counts/receipts
  merely to make reordering pass.
- After approval and implementation, run locked native core/protocol/Windows
  build/tests, strict all-target clippy, workspace fmt/diff; stress schema/parser,
  definition/fault mocks and live-library checks. Shared changes also need
  applicable non-Windows CI evidence before cross-platform acceptance.
- Windows live plan follows [the live guide](../live-windows-testing.md) and
  [fixture README](../../crates/tiler-live-fixture/README.md), fixture-only:
  run `scripts/windows-world-stress.ps1 -Live -DelayMs 0` for home-away and
  trio-churn, then a new authored spam case with A/B, empty WS2 and final WS1.
  No inter-command convergence waits or foreground repairs; checkpoints prove
  final A, exact native visibility/frames and paired pager/world state. Zero
  delay means no added pacing, not zero CLI transport overhead.
- Reproduce the specific split-arrival ordering deterministically in simulated
  host tests first. Native scheduling need not hit the race on every burst;
  report whether an unfinished epoch actually occurred and was superseded,
  not merely that a stress run passed. Stop/restore exact owner, shut down
  fixture, compare settings/taskbar/SPI preimages and verify no residue.
- Current runner is CLI-only (`windows-world-stress.ps1:1-20`); real injected
  shortcut mode remains future work. Existing input filter/foreground guards
  must not be bypassed to make a fast burst. Add that mode only under its
  separately reviewed contract; physical shortcut and actual game acceptance
  remain user-owned. Use offline game seams, not live real games here.

### User decisions needed (recommendations only)

| ID | Choice and recommendation | Consequence |
| --- | --- | --- |
| D1: finished | Recommend required view/member/visibility proof plus intended focus proven (or no focus target), not setter acceptance and not every unrelated frame/overlay. Relevant proof must be incorporated before superseding navigation; late proof never retroactively completes an aborted arrival | View-only/unknown/refused arrivals remain unfinished without retries; empty arrivals can finish. Choose all-world geometry completion instead only if that extra pending lifetime is wanted |
| D2: genuine click/input | Recommend strict provisional memory until D1 completion: a click respects actual focus and stops stale focus, but aborting that arrival still discards its memory. Independently proven explicit focus/move/send intent may establish separately owned state, not finish the old arrival | A click during an aborted visit will not become its remembered window under the strict rule. Making a click itself a completion/new-memory boundary is a separate exception needing approval and source provenance |
| D3: other world state / core scope | Recommend generic opt-in core transaction guarding all arrival-derived policy, not only focus. Stage visit/history/anchor changes; count only completed visits in committed visit history. Preserve independently owned commands and real lifetime/topology facts, never resurrect/undo them on abort | This is a proposed boundary for the broad no-world-state rule, not yet approved. It changes unfinished-visit history and needs generic core/protocol approval; specify any desired different history or independent-event treatment before coding |
| D4: already-issued native effects | Recommend no compensation/retry: strict no-commit guarantee applies to logical world state, while already-issued native calls may still land and are reported honestly. Fence all unissued obsolete writes; latest-state convergence remains gated | A stronger guarantee of zero physical transient after departure cannot be provided for already-issued asynchronous Win32 calls. Would require a different mechanism, not a relaxed safety gate |
| D5: latest unissued focus | Recommend fresh core revalidation of the still-current explicit selection only before its first prime/setter, with arrival-level at-most-one attempt, new revision-bound authority and fresh selection-origin desktop proof. Never revive the old effect/grant or retry a begun/cancelled attempt | Changes observation-no-effect and current grant minting contracts; needs explicit generic core/protocol and native authority approval. Without it, an otherwise eligible final focus can remain lost solely because its unissued reply went stale |

- Safety is not a vote: games/unknown competing foreground always retain their
  native vetoes, and preserving memory never forces refocus. Host activation
  is not a user-intent exception without proof. No decision above is marked
  adopted by this design note.
- Design review identified the final-unissued-focus gap, resulting in explicit
  D5 rather than an implicit retry/grant extension. Closure review confirms
  all identified authority, proof, write-set and protocol-documentation gaps
  are addressed in the proposal. No runtime evidence is added by that review.
- At this proposal stage the next action was D1-D5/core-protocol review. The
  later comparison below revisits that requirement; D1-D5 are not a request to
  approve the older model by default. No runtime verification is claimed.

## Command-owned active-window model comparison (2026-10-27)

- Historical comparison preserved. Command-owned memory and defer/reconcile
  subsequently replaced these options; defer/reconcile is now also superseded.
  None of these historical attribution recommendations selects today's unknown
  policy; use the [current investigation](#prompt-intent-and-render-side-effects-2026-10-27).

### User model and recommendation

- User discussion proposes two owners of a workspace's active-window memory:
  core decisions when processing commands, and genuine external focus changes
  (click or app self-activation). Core picks and records arrival fallback at
  command time; render progress never supplies policy decisions. Render-caused
  foreground changes must not update world state.
- **Recommend this simpler logical model, with Option 1 truthful attribution.**
  It preserves the earlier binding rule by separating the independent core
  command decision from the unfinished platform rendering of it: rendering or
  its supersession commits no new policy state. It does not undo a correct core
  choice just because Windows has not landed focus. This is a proposed design
  selection, not an implemented change or an adopted attribution policy.
- A genuine click/app activation can update active memory even during render
  if it is actually identified as external; no arrival-finished milestone is
  needed in core. The difficult case is distinguishing it from an OS/client
  reaction to our show/hide. Current evidence cannot always do that.

| Question / concrete case | Earlier arrival-transaction proposal | Command-owned model |
| --- | --- | --- |
| Send W without follow to previously empty workspace; select it; leave before native focus | Arrival choice stays provisional until view/focus proof; abort may discard it | Selecting W commits W as active immediately. Later unfinished rendering cannot undo it. Better matches this user's example |
| Genuine click on B while returning to A, then leave | Earlier D2 recommends discarding aborted-visit memory even for the click, unless an independent-intent exception is added | Identified genuine B activation is independently authoritative immediately; no special completion exception |
| Steam becomes foreground during deferred reveal | Protects baseline while pending, including departure; needs completion/abort/provisional write-set machinery | Core keeps command-time A; withholds active-memory updates for render-origin/unknown transitions under the proposed rule, while actual foreground stays reported |
| Directional/follow/close while native rendering is incomplete | Requires separating independent changes from an arrival delta to avoid rollback | Their core decisions stand immediately, independent of rendering. No rollback journal in core |
| Workspace history and admission anchors | Earlier D3 proposes completed-visit history and quarantine of arrival-derived anchors | Explicit command changes stand; only render-feedback contamination is excluded. Must route attribution to all focus-derived policy consumers, not just `last_active` |
| Late known own focus / async hide after navigation | Epoch proof can reject an old completion; native effects still cannot be cancelled | Windows effect/operation generations fence and account for old work; landing is not a new external focus decision. Core needs no arrival completion token |
| Genuine activation indistinguishable from rendering, or an app reacts long after show | Transactions conservatively protect memory until the chosen completion boundary, but cannot identify the cause; delayed activation after completion remains ambiguous | Same attribution information limit. Generic unknown policy exposes it directly instead of claiming provisional state solved causality |

- **Where the older design is genuinely useful:** if product semantics require
  only physically completed visits/choices to count, or require rolling back
  command-time history/admission effects of a failed arrival, a core transaction
  expresses that atomic commit boundary. Those are different semantics from
  the proposed command-owned model. A transaction may also contain speculative
  multi-property policy that must be committed together; native operation
  receipts alone cannot implement that policy. No such extra requirement has
  been established for this focus-memory report.
- **Where it is not a solution:** neither a core arrival epoch nor a Windows
  render generation proves whether Steam self-activated, a user clicked, or
  Windows chose a fallback. Both models need honest attribution; the older one
  also needs completion semantics. Keep its review findings and render/effect
  fencing requirements, but do not retain its core transaction machinery solely
  to solve this narrower memory problem.

### What core actually does today

- `crates/tiler-core/src/world_tiling_views.rs:186-226` changes logical view,
  chooses the eligible remembered/MRU/layout candidate and emits explicit
  `fx-*`; it does **not** record that selected candidate in session memory.
  `remembered_view_focus` (`:236-277`) is a read-only candidate lookup. There is
  no later selection-effect-publication helper that commits that target.
- Thus the user's send-stay-to-empty then select example needs a real core
  change: commit the selected valid candidate and corresponding MRU/active
  policy when the selection command is accepted, even if native focus never
  runs. Do not confuse emitted `focus_effects` with recorded active memory.
- `crates/tiler-core/src/world_sends.rs:115-168` immediately commits logical
  membership, optionally changes follow-view, and emits the selected follow/
  stay focus effect. That block does not itself write destination `last_active`.
  `world_commands.rs:69-99` explicit focus similarly emits an effect. Command
  paths are not uniformly a single existing memory setter: tiled Engine plans
  can update focused state, while direct world effects can be intent-only.
  `session/world.rs:579-639` explicitly distinguishes `sync_focus_from_window`
  (focus/stack) from `remember_focus` (also `last_active`). Audit command result
  application rather than asserting all directional/follow/close paths already
  implement the model or blindly applying an existing helper with pending gates.
- `session/world.rs:1919-1950,2021-2035` observation convergence adopts observed
  tiled focus and updates `focused_domain`, `focused_leaf`, `focus_stack`,
  `last_active`. Its retained-focus branch keeps an existing valid A when no
  observed focus is supplied; absence does not always overwrite A with MRU.
  Source focus arrives via `world_capture.rs:1686-1690`, then
  `world_observe.rs:333-348`. Current report focus has no render-origin qualifier.
- Recommended generic core delta is therefore more than a boolean at one
  assignment: (1) record core-owned active choices consistently at command/
  core-lifecycle decision time; (2) separate actual observed foreground from
  permission for that observation to update active memory/MRU/implicit anchors;
  (3) propagate the qualifier through convergence and every relevant policy
  consumer. Do not suppress inventory, actual safety facts or independent close
  successor decisions. No need for provisional arrival state or a core
  rendering-completion protocol under this model.

### Attribution evidence: existing versus available mechanisms

| Mechanism | What exists / what can be established | What cannot be claimed |
| --- | --- | --- |
| Keyboard hook | `ffi/llhook.rs:98-119,168-181` copies raw key events; Alt/Tab edges can traverse it. `snapkey/chords.rs:313-318` passes unrelated/extra-modified input untracked; no target-bound Alt-Tab activation record exists | Seeing Alt/Tab does not identify which later foreground event it caused. Current trace is not an all-key input log |
| Mouse hook | `ffi/llhook.rs:16-24,245-264` is conditional FFC motion-only; ignores button messages, passes through and filters injected motion. No always-on click attribution hook exists | Cursor over Terminal at log 771 does not prove a click, nor prove no click. Adding click capture changes hook cost/scope and is not selected here |
| WinEvents | `ffi/hooks.rs:13-21,43-66` carries kind, HWND, object, thread and generated-time; foreground event identifies the resulting foreground window. `observe.rs:403-408` explicitly encodes no causal claim | No source command/effect id or click/app/OS activation reason. Event thread is not proof of who initiated the focus change; callback/drain time is not causality |
| Documented Alt-Tab events | Microsoft documents `EVENT_SYSTEM_SWITCHEND` with the switched-to HWND; it is stronger target-specific evidence than adjacent raw keys | Not currently subscribed: `tiling_sys/run_loop.rs:387-399` and `observe.rs:196-206` omit SWITCHSTART/END. New subscription, identity/source-order handling and safety/cost review would be a separate addition, not existing coverage |
| Recipient activation messages | `WM_ACTIVATE` has `WA_CLICKACTIVE`; ordinary `WA_ACTIVE` also covers keyboard/API methods | These go to activated/deactivated window procedures, not our existing out-of-context WinEvent feed. No foreign-window activation-message capture exists; `ffi/pager.rs:813-826` handles our pager only. Do not propose invasive/injected foreign hooks as an automatic fix |
| Own show/hide | We know exact issued operations, lifetimes and native postconditions; `world_native.rs:433-553` uses nonactivating paths, including async show | Nonactivating intent does not prove later foreground causality. `ShowWindowAsync` returns after posting, not after all application reactions. A visibility readback is not an activation-origin fence |
| Own focus flight | `world_diff.rs:2297-2369,3758-3817` retains exact target HWND/PID/creation/tag lifetime and resolves accepted setters as own landing or readable supersession; no retry/timeout | This is the approved identity-based convention, not OS cause proof. The documented simultaneous click on that same flight target is indistinguishable and already accepted as ours. Another window becoming foreground is not automatically genuine external intent |

- API evidence, read 2026-10-27:
  [WinEvent callback](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nc-winuser-wineventproc),
  [foreground and SWITCHEND constants](https://learn.microsoft.com/en-us/windows/win32/winauto/event-constants),
  [ShowWindowAsync](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindowasync),
  [WM_ACTIVATE](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-activate).
  These are distinct from causal guarantees the current product actually uses.

### Render scope and the irreducible unknown

- Windows can maintain mechanical render publication/operation identities and
  source observation watermarks. Start tracking before own visibility/focus
  writes, not only after an accepted focus setter. Preserve outstanding async
  operations/flight evidence across superseding commands. Deferred observation
  reveals, zero-focus-write arrivals and queued edges all matter. This is
  Windows render/effect accounting, not a core arrival-finished concept.
- Neither returning from apply, posting a show, changing workspace nor proving
  visibility closes activation causality. A `rendering=true` stack-local flag
  cleared at return would miss this report. A readback/drain may prove current
  render state, but does not prove an app cannot subsequently react to an older
  show. Current APIs supply no general finite causal fence for arbitrary client
  activation. A generation label can identify our work; it cannot label an
  otherwise unattributed WinEvent as caused by it.
- Proposed evidence classes are conceptual, not current wire fields:
  known-own-focus landing; independently established external activation;
  unattributed transition with overlapping/unresolved render context; unknown
  context (lost history/unreadable identity). Core applies the opt-in policy;
  Windows supplies actual foreground plus evidence/uncertainty, never a claimed
  origin inferred from elapsed time, cursor position or coincident HWND.
- Preserve the qualifier of an ambiguous transition through later samples,
  including departure. Re-reading the same Steam foreground after apply returns
  is not a new genuine activation and must not launder the suppressed update.
  Transition/context loss stays unknown rather than silently authorizing memory.
- Proposed clear/replacement predicate: a newer source-ordered foreground
  transition replaces the old transition record, but gets its own attribution
  rather than inheriting "external"; independently proven click/app intent
  targeting the same live window may replace its policy qualifier without a
  new HWND change. Identity lifetime end removes the old record. Generation
  reset invalidates old evidence but does not prove that a bootstrap sample is
  external. New explicit core commands update active memory by their own
  authority; they never retroactively relabel an old ambiguous Steam transition.
  Own-flight landing resolves native accounting, not external intent.
- Thus uncertainty for one unchanged transition may last indefinitely when no
  new independent evidence exists. Store bounded current transition/effect
  evidence, not an unbounded event history or a global memory freeze; explicit
  core decisions continue. This is an explicit cost of the conservative option,
  not a causal timeout invented to make suppression disappear. Pin replacement,
  generation/identity invalidation and no-repromotion cases in tests.
- **Remaining information limit:** a real click or app self-activation during
  render may have exactly the same observed data as fallback caused by our
  rendering. Suppressing unknown protects command-time A but can miss genuine B
  memory changes. Accepting unknown preserves those real changes but permits
  this Steam failure. No transaction, event timestamp, recent-input check or
  visibility acknowledgement solves that tradeoff. A finite operational
  "outside render" boundary is not proof of external origin; choosing it would
  be an explicitly approved approximation, not a causal implementation.

### Option 1 versus Option 2 and the reported sequence

| Approach | Effect on the Steam sequence | Cost / layering |
| --- | --- | --- |
| 1: actual foreground plus generic uncertainty; core declines active-memory updates for own-render/unattributed render transitions | If Steam's transition is flagged and that status persists through report 109, memory stays A and return 110 selects A. No flight is needed to flag deferred show work | Recommended: truthful facts and policy remain separated; generic opt-in shared-core/wire change and command-time active recording are required |
| 2: omit the focus-policy change on Windows | With existing A memory, making focus absent can retain A in this exact sequence. Only works if filtering also covers departure and later re-samples | Core-untouched filtering conflates no/unknown foreground with a withheld policy event. Filtering just memory while keeping actual foreground as a distinct core input is effectively Option 1's split, not the existing unchanged schema |

- Option 2 need not blind native safety if Windows keeps checking actual
  foreground separately. It is nevertheless wrong to claim zero cost: current
  `focused_global` also informs core fact/policy consumers, and it would not
  supply the missing command-time candidate recording. Retaining raw foreground
  locally does not make an empty core focus report truthful. Recommend no
  adapter-local implicit decision to hide observed focus from core.
- Concrete trace: return 107 stops with no native writes (747/757); observation
  108 reveals via three async show posts (761/769). That reply is `partial:true`
  and has queued foreground/visibility invalidations. Steam's foreground edge
  at 762/763 is thus a concrete render-overlap unknown, **not proven render
  cause**. Departure 109 (771/779) must preserve that qualifier on its factual
  Steam focus instead of committing it. Merely flagging `in_flight` would fail:
  there was no Terminal setter/flight. Return 110's target changes to A under
  either adequately scoped suppression approach; native landing still depends
  on current gates, not on this design's prediction.
- This is conditional evidence, not a live verification. A flag scoped only to
  time inside the apply function would let both approaches fail. Exact cause
  of the user's Steam transition remains unknown.

### Existing safety/authority and rendering interactions

- [Layering](../principles.md#layering) favors Option 1: core decides whether
  reported attribution permits a memory update; platform reports limitations
  neutrally. Do not leak a Windows `rendering` policy switch into shared core;
  use a generic opt-in focus-provenance/authority contract with existing defaults
  unchanged until adapters supply it. Not a promise of a one-line guard.
- The 2026-10-26 no-forced-refocus rule still reads actual host focus/loss, even
  when active memory is A. No observation-triggered setter to force A back.
  A genuine recognized external B updates memory without any "finish arrival"
  wait. Unknown B can veto native focus without becoming remembered B.
- Competing fullscreen/game/unknown foreground always reaches live safety gates
  and invalidates unsafe pending actuation, independent of memory permission.
  Command-time active state is not authorization to focus, resize or overlay a
  game. Explicit intended game members retain existing selection eligibility;
  no new hook, interception or game exception is selected.
- Exact-desktop grants remain bound to the fresh accepted explicit selection
  effect and live shell proof (`world_diff.rs:339-408`). Neither attribution,
  recorded active intent nor own-flight recognition grants extra desktop
  authority. No old grant/effect revival, timeout, replay or corrective focus.
- Keep event-driven latest-world draining, dependency fences, honest operation
  receipts and late-call accounting from the earlier renderer design. Drop
  the requirement to wait for rendering before committing core active choices.
  A final stale unissued effect still presents the earlier D5 **native actuation
  authority** question; changing logical memory ownership does not silently
  approve continuation. Resolve it separately if implementation needs it, not
  by retaining the whole core transaction design.

### Current decisions needed and test-plan delta

- Two semantic choices remain, neither authorizing a schema/implementation:
  (a) **command-owned active memory versus completion-conditioned transaction**:
  recommend command-owned, recording core-selected candidates at decision time
  after the command-path audit; (b) **Option 1 conservative unknown policy versus
  accepting/omitting ambiguous focus**: recommend actual foreground plus a
  generic qualifier, with no active-memory update from unattributed render-
  overlap transitions. Accept the consequence that indistinguishable genuine
  clicks/app activation may not update memory, potentially for the unchanged
  transition's whole lifetime. Replacement rules are above; no timing expiry
  or automatic re-promotion when render returns.
- A model choice is not approval of a particular core/wire scope. Follow with
  the command-path/consumer audit and smallest generic opt-in contract proposal;
  no new input hook or desktop/focus continuation authority is selected here.
- Perfect genuine-versus-render-origin detection is not available from today's
  observation set. If losing those genuine changes is unacceptable, source
  evidence must improve or the unknown rule must change; do not implement the
  model claiming both guarantees. Targeted SWITCHEND is a possible later
  evidence improvement, not a solution for app self-activation or all clicks.
- Replace provisional-commit/completed-visit test oracles with command-time
  intent oracles: send W without follow to empty/trailing destination, select,
  assert W active immediately before **any** native write/proof; leave early
  and verify W remains active. Pin no-memory MRU/layout choice and no eligible
  target cases. Commands that independently choose active state must be audited
  and tested consistently (directional, explicit focus, follow, close successor).
- Exact red regression remains 107 -> 108 reveal -> ambiguous B -> departure
  109 -> return: assert A retained throughout and final target A; actual B must
  remain reportable throughout. Add re-sample-after-render regression so the
  same ambiguous transition cannot update memory later. Include no-flight,
  async old-generation completion and reordered source observations.
- Add paired origin cases with identical foreground/visibility: proven external
  click/app activation updates B immediately; known-own or unattributed overlap
  does not. Assert unknown stays unknown, no forged input/operation provenance,
  queue-history loss explicit, and no safety/subject-input fact is hidden.
  Stock Windows input cannot fabricate the "proven external" positive; test
  the generic rule with explicit simulated evidence and state native gaps.
- Qualifier lifecycle oracles: same-transition samples and a new render
  generation cannot promote unknown; a newer proven external event, including
  a source-bound same-target click if available, can update memory; successor
  unknown stays unknown; dead identities/reset evidence cannot be reused.
  Explicit core focus commands still update memory without waiting for any of
  these host proofs. Do not use the reported log as a substitute for a red
  simulated-evidence regression or claim native attribution coverage from it.
- Preserve ABA/full-identity flight, effect/grant fencing, no-forced-refocus,
  game-veto, pure-state coalescing/fairness and command-order/barrier tests.
  Ordered/batched/reordered core schedules vary evidence delivery, not command
  execution order. Update schema/strict fixtures deliberately if selected.
- Native plan remains fixture-only home-away, trio-churn and authored A/B spam
  at `-DelayMs 0`, final native/pager/world proof, no per-command convergence
  wait or foreground repair, exact cleanup/settings restoration. Require trace
  evidence that an ambiguous overlap actually occurred before calling it race
  coverage. Zero added pacing is not physical-keyboard-rate evidence; real
  game and physical shortcut acceptance stay separate. No live runs occurred.
- At this comparison stage the next action was choosing the model and unknown
  policy. Defer/reconcile subsequently superseded those choices and has itself
  been superseded by the prompt-intent investigation. The comparison is retained,
  not an outstanding request to select its historical options.

## Defer and reconcile design (2026-10-27)

- **Superseded by the [prompt-intent investigation](#prompt-intent-and-render-side-effects-2026-10-27).**
  This section records the former design, not current semantics. Its click
  override, applied-only ingress and R1 release proposal are withdrawn. R2
  survives only as an independent never-issued-focus authority question.

### Former semantics and smallest scope (superseded)

- Core records its active-window choices when it makes them, without waiting
  for native focus or view completion. The arrival fallback remains eligible
  remembered focus, else MRU, else layout order. A command is not undone because
  its rendering was superseded; rendering itself supplies no new policy state.
  Independent core lifecycle decisions such as a close successor still apply.
- The former rule deferred foreground-change publication during latest-world
  rendering, then reported actual foreground differing from desired after proven
  application, even with unknown click/app/OS origin. This persistence rule is
  withdrawn, not an input-attribution guarantee or the current target design.
- An earlier click may be superseded by the later authorized focus request; a
  later surviving click is reconciled and remembered. No timing comparison is
  needed to infer click cause: final state is what matters. Existing live safety
  and no-forced-refocus can refuse/cancel a native focus request; "latest action
  wins" does not authorize bypassing those gates or issuing a corrective retry.
- Scope: generic command-memory consistency and deferred focus ingress; Windows
  latest-state scheduling/proof/reconciliation. No core arrival transaction,
  completion-conditioned history, rollback or render policy. No production/test
  source is changed here. Other disturbance candidates are recommendations only.

### Core audit and minimal change

| Decision path | Current behavior, source evidence | Smallest proposed correction |
| --- | --- | --- |
| `Select*` / view return | `world_tiling_views.rs:186-226` changes logical view and emits candidate focus; `:236-277` only reads memory | Commit the selected eligible candidate to that workspace's active/MRU memory as part of accepted command, before native work; empty/no eligible remains valid no-focus |
| Direct `Focus`, float directional, no-focus crossing | `world_commands.rs:69-99`, `world_operations.rs:436-474,512-609` can emit a focus target without committing it to session memory | Route successful core-selected targets through the same logical-decision recording boundary; do not equate an effect with a commit or expand float geometry/eligibility policy |
| Engine-routed tiled directional focus | `engine.rs:4455-4500` synchronously synthesizes internal ack/post evidence and calls `verify_focus`; `session/ops/focus.rs:231-252` commits focus, stack and last-active | Keep existing command-time commit; ensure later deferred observations cannot overwrite it. Do not add a second revision/commit |
| Follow/stay send | Ordinary Engine transfer commits synchronously (`engine.rs:2900-2908`); `world_sends.rs:115-168` commits membership/follow-view and emits selected focus; some boundary/exception paths lack linked target-memory recording | Preserve already-committed source/target state; fill only missing selected-target recording after logical ownership is established. Follow chooses mover; stay chooses source survivor or none, not a new destination focus policy |
| Close successor/admission/minimize lifecycle | `session/world.rs:1919-1950,2021-2035` resolves valid observed focus, retained focus, MRU or none while converging inventory; lifecycle plans also commit focus | Process real lifetime events promptly and retain core-selected successor even during deferred focus. Defer only host-focus feedback, not structural removal or the resulting core decision |

- Use existing per-workspace/domain memory, not a global desired-active slot.
  `session/world.rs:579-640` distinguishes focus/stack synchronization from
  remembering last-active; `remember_focus` rejects pending/drag/divergence.
  Add one generic logical-decision recording seam at the accepted world-command
  boundary, reusing existing session validation/commit machinery. Do not call a
  pending-gated helper after returning a successful effect and silently ignore
  its failure, or mutate state before a command can still reject.
- For linked tiled targets, reuse `focused_domain`, `focused_leaf`,
  `focus_stack`, `last_active`. No existing generic non-leaf active-memory slot
  has been established by this audit. If the accepted command-owned contract
  must cover an already-supported selected exception target lacking a leaf,
  propose the smallest per-workspace opaque active-identity storage alongside
  existing logical ownership, with a single canonical update/projection path;
  do not invent a tree link or claim such storage already exists. This is an
  explicit potential core data-model addition, not a helper-only implementation
  claim. Exact integration needs per-path regressions; no new exception
  eligibility, automatic fallback or global focus authority is proposed.
- Core must distinguish actual request/subject context from a foreground-change
  update. Guard host-focus-derived memory/history/anchor updates **before**
  observation convergence, including the departure report. Raw B must not first
  update MRU and then be "deferred" downstream. Explicit command subjects still
  use actual live context, never stale remembered A posing as actual focus.
- Accepted commands commit once; rejected/consumed/duplicate delivery creates
  no new memory decision. Later lifecycle invalidation removes/replaces invalid
  remembered targets normally. Deferred focus never blocks lifetime/display/
  settings processing or protects a dead remembered window.

### Minimal protocol contract, not final field names

- A change is required: `WorldReport.focused_global` currently has only a token
  or empty string (`world_report.rs:1090-1093`); empty cannot distinguish
  observed no managed focus from a deferred focus update. Reusing the last
  reported A or sending empty for both would misrepresent context and policy.
- Propose a generic focus-update discriminant: **deferred**, **observed known
  target**, **observed no managed target**, **unreadable**. Separate live
  foreground/action context, where needed for command subjects and safety,
  from the update channel. During render there is no foreground-change event
  delivered to policy; any raw snapshot context carried to core is explicitly
  non-converging and cannot update memory, MRU, anchors or visit history.
  The live native gates always read actual state. No render-origin uncertainty
  decision or core render-finished token is needed.
- Reply needs durable current desired-active identity for each relevant workspace
  and the currently selected global focus intent (target/no target/held, logical
  scope and source intent identity), so Windows can compare against latest desire
  without caching obsolete effects or re-choosing policy. `WorldReply`
  (`world_report.rs:1271-1329`) has ephemeral effects/transfers and no canonical
  per-workspace active row; presentation foreground is observed, not desire.
  Reuse existing workspace/output rows with a small bounded projection rather
  than a separate unbounded history. Desired active identity is **not** native
  actuation authority; one-shot ids/revisions/grants remain separate.
- Bind reconciled update to the actual capture, adapter incarnation/core
  generation, source observation sequence and render publication being compared.
  Before incorporating it, core verifies relevant desired focus/view intent has
  not been superseded; older feedback cannot overwrite a newer command choice.
  This is normal source-intent fencing, not an arrival-completion transaction.
  Missing proof is not `observed none`; unreadable stays unknown.
- Versioned coordinated core/protocol/Windows change: strict decoder key sets
  and version gate (`crates/tiler-protocol/src/world_protocol.rs:1555-1615`)
  reject unknown fields. An optional field with legacy default helps a **new**
  decoder read older messages; it does not make older decoders accept new fields.
  Specify explicit compatible-version/capability routing before adding fields,
  retain old default semantics for non-opted adapters, update canonical encoders,
  size bounds, strict mocks and simulated producers. No silent v2 shape change
  or invented negotiation API is approved by this design.

### Windows latest-only owner and proof boundary

- Keep one native owner initially. Incorporate each command in order and ack
  exact carried facts through the single-flight sequencer; separate that core
  incorporation ack from native application. Publish immutable latest desired
  state with dependency/operation/effect envelopes. Service safety and new
  commands between bounded native slices; replan before the next obsolete
  unissued write. No unbounded intake drain, sleep, debounce, polling loop or
  settle timer. Existing events or a self-signalled pending-work event drive
  progress, and an idle/unprovable state sleeps without spinning.
- Replace unissued pure frame/view/visibility/presentation targets with compatible
  latest state. Preserve every command, ordered send/migration/close phase,
  already-issued call, possibly-executed operation, recovery obligation and exact
  attempt/ack. Never merge conflicting dependency groups into a Frankenstein
  snapshot. Unrelated output work cannot finish the single global foreground
  phase; a new focus/view intent replaces its prior pending render publication.
- Fence effects before claiming/priming/setter and after any existing prime
  recheck. Superseded unissued effects do nothing; spent/claimed effects are
  never reminted as retries. Preserve exact native flight identity/lifetime,
  landing/supersession diagnostics and no cancellation of issued Win32 calls.
  Exact-desktop grant is not copied to a new reply/publication or conferred by
  durable desired-active memory. The earlier unissued-final-focus authority
  gap remains R2 below, not an implicit approved exception change.
- Windows tracks a render publication and per-required-property status:
  **pending**, **postcondition proven**, **issued but unproven**, **terminal
  refused/unavailable**, **superseded**. These are host-accounting states, not
  new core policy. A successful API return or empty message queue is not proof.

| Required property / event | Proof or action | Limit / truthful degradation |
| --- | --- | --- |
| Frame, membership, view and hidden visibility | Fresh lifetime-bound readback/receipts matching the latest authorized target, with existing scope/hidden-write gates; show/hide events trigger rechecks, not verdicts | Wrong/unknown/held stays unresolved; preserve raw attempt evidence. No quiet-log-as-success |
| Async show/hide | Post all permitted actions, then observe through existing events and fresh readbacks; prove latest visibility of exact members, including hidden departures | `ShowWindowAsync` return only means posted. A readback equal to target does not prove contradictory old queued writes finished (ABA); retain outstanding operation evidence and revalidate after old effects rather than declaring a fabricated queue fence |
| Focus already holds target | Fresh full-identity/scope/view/visibility/safety proof with no conflicting current work | No gratuitous focus call; an unreadable target/foreground is not success |
| Focus request lands | Immediate target foreground proof, or existing exact-identity own-flight landing plus fresh live bracket | Accepted setter/GUI-thread snapshot is not landing. An edge has no effect id; old/superseded identities cannot settle a newer publication by bare HWND equality |
| Focus proven, later B differs at final reconciliation | Do not require final actual foreground to equal desired A to admit B: preserve prior current-intent focus-application proof, compare fresh B after remaining required application, and report the persistent deviation once | Otherwise the predicate "A must be foreground before reporting B" would circularly suppress every surviving click |
| Focus silently ignored, no event, first read still B | Record native call and mismatch as unproven; no completion event/barrier exists in the current mechanism. No wait, timer, retry, made-up landing or false applied flag | Strict applied proof cannot be obtained; R1 defines terminal-incomplete reconciliation instead of indefinite deferral. A finite return from `SetForegroundWindow` does not prove it cannot land later |
| New command or lifetime/display/settings event | Incorporate promptly, advance relevant desired publication/authority, retain old issued-effect evidence and keep foreground changes deferred across replacement | A superseded publication never flushes its B sample into memory on departure. Real lifetime/display/settings facts do not wait for render proof |

- **Proven applied** means every required native property of the current
  publication has sufficient current/source-bound postcondition evidence, not
  every unrelated overlay paint or the entire world's quiescence forever. A
  later persistent external foreground is reconciled after that applied proof;
  distinguish desired-focus application proof from current foreground equality.
  Subsequent invalidation revokes affected proof; new commands revoke stale
  comparisons. Existing presentation safety/ack obligations remain independent.
- There is no general host guarantee that all effects of arbitrary async client
  queues have ended. Do not label old opposing posts settled just because a
  sampled state matches latest desire, and do not block a thread trying to
  manufacture ordering. Rebase on actual state; if proof remains impossible,
  expose unresolved obligations under R1. This is the Ordering/Races gap the
  host actually offers, not a reason to insert a settle delay.

### Deferred foreground publication and reconcile

1. Before a latest publication begins host writes, mark focus updates deferred;
   keep a bounded latest actual foreground sample and native flight evidence.
   Do not accumulate a replay queue of transient focus changes. Live safety,
   request subjects and input guards keep using actual foreground immediately.
2. During render, reports carry the deferred update marker, not an empty/old
   active-window claim. Structural facts and attempts continue through normal
   reports. Overlay safety may hide immediately using actual state; core does
   not learn a fake active window for presentation convenience.
3. Superseding commands discard old pending focus-feedback samples as policy
   updates and compare only against the new latest desire. Old async completions
   remain factual native obligations, never authority to update remembered focus.
4. At an event-driven proof boundary, drain available invalidations/commands,
   prove current publication applicability, sample actual foreground with full
   managed lifetime binding (or explicit no managed target/unreadable), and
   verify relevant publication/source watermarks again. If stable, compare
   actual with durable desired focus. Equal means no new external focus update;
   different means publish the surviving result through generic observed focus.
   Core adopts it and remembers it without another focus effect.
5. Mark the exact comparison consumed once. A new command racing capture/report
   must invalidate that comparison via source-intent binding before convergence;
   an equal/stale duplicate cannot reassert B over a later command. Outside
   rendering, foreground events publish observations immediately as today.

- A click before an eligible latest focus call is overridden if that native
  call actually lands; no click classification required. A click after that
  call that persists until reconciliation yields B and is remembered. The case
  where B appears before any target landing is observed falls in R1's proof
  gap: neither `landed-own` nor physical click timing can be fabricated.
- Immediate native safety can veto on a competing game/unknown/fullscreen or
  genuine host-loss condition. Preserving A memory is not a request to refocus
  it after B. Terminal failure must not cause periodic re-prime/refocus or
  silent adoption of a known transient from an abandoned intermediate world.

### Former no-proof exit and authority question

| ID | Recommendation | Consequence / approval needed |
| --- | --- | --- |
| R1: terminal-incomplete reconciliation (withdrawn) | Former proposal: release foreground deferral at a truthful incomplete native boundary instead of waiting indefinitely for applied proof | **Superseded:** prompt intentional ingress does not depend on render completion; there is no R1 release decision in the current design. Unknown provenance is a separate question, not rescued by a completion proxy. [Current investigation](#prompt-intent-and-render-side-effects-2026-10-27) |
| R2: current unissued focus authorization | For latest-only scheduling, permit a still-current explicit focus intent to receive fresh revision-bound core authorization only with positive proof its prior effect was never claimed/primed/dispatched; retire old id atomically with an intent-level at-most-one dispatch fence | **Not yet approved:** a narrow replacement of stale unissued authority, not retry/replay or grant copying. Existing selection-origin/exact-desktop proof must be reminted under the approved command source and rechecked live. Otherwise final stale effects stay dropped/unfulfilled; terminal result follows R1 |

- R1 preserves outstanding async posts/flight identities as obligations;
  "no authorized unissued work" is not quiescence or proof of no future effects.
  Only a current **non-superseded** publication can reconcile under that proposed
  exit. Superseded samples are discarded, including on departure, never flushed
  as residual foreground. A late known own effect invalidates/reopens current
  host reconciliation; it is not automatically promoted to a new external
  focus decision just because an earlier slice ended incomplete.
- In this withdrawn design R1 was the central no-proof choice. It did not promise a host proof
  that does not exist. No foreground-cause attribution or new global input hook
  is required by either outcome. A target click/game supersession always keeps
  native safety authority regardless of whether R1 releases memory feedback.
- "No event" is not a detectable terminal event. R1's finite boundary is the
  returned attempt and immediate readback with no further authorized action,
  not an invented observation that no event will ever arrive. Ordinary
  event-backed pending show/flight work may still finish later. Releasing at
  that incomplete boundary can report B before a delayed own A lands, so it
  does **not** preserve the exact applied-only transient guarantee; accepting
  that degradation is the R1 decision. Under strict semantics keep it pending
  instead, promptly expose incomplete/actual diagnostic context and wait only
  on real events/new commands, potentially indefinitely. No silent substitute.
- R2 is independent of command-owned memory: remembering A does not authorize
  a setter from every newer observation. If no change to existing one-shot and
  desktop-grant contracts is wanted, retain them and document the unfulfilled
  final-focus case instead of bypassing freshness in Windows.
- At this historical stage the applied-only principle still held and R1 would
  have needed an exception. The later principle revision removes that
  completion-conditioned ingress rule rather than selecting an R1 exception.

### Other render disturbances: recommendations only

| Host state | Recommendation, outside this focus implementation scope |
| --- | --- |
| Geometry/membership/view echoes | Audit existing attempt/receipt reconciliation so delayed own writes do not masquerade as client moves or transfers. Preserve actual lifetime/identity facts and known operation outcomes; never defer real destruction |
| Z-order/activation/presentation | Own raises/show can change observed stacking; consider similar final-state coalescing for policy-sensitive stacking only. Live overlay/fullscreen safety must remain immediate |
| Cursor warp / FFC | Existing self-warp/input-origin filters must not trigger a new user-motion decision. Audit surviving final cursor differences separately; do not defer genuine input handling |
| Minimize/maximize/restyle | Own authorized state changes can generate host echoes. Preserve operation-bound proof, report independent user/app changes promptly; do not broadly defer minimized/closed lifetime events |
| Taskbar/SPI/settings effects | Keep separate owned-change/recovery provenance. Displays/settings always report promptly under approved semantics; no generic defer of these categories |

### Failing-first verification plan and retained design

- **Core:** send W without follow to empty/trailing destination, select it and
  assert active W before any native write; final superseded rendering leaves W
  recorded. Pin explicit/direct/float/tiled/no-focus directional, follow/stay and
  close-successor decisions. Verify no duplicate commit/revision; refused/consumed
  input unchanged; lifetime invalidation removes stale active identities.
- **Steam red regression:** commit A; leave/return; drop unissued focus as in107;
  deferred observation reveals members as in108; transient B foreground; depart
  with B actual as in109; final return selects A. Assert no foreground policy
  update at any intermediate/departure report, but actual B remains usable for
  native safety/subjects. Do not require this trace to prove Steam's cause.
- **Click ordering:** simulate A target landing then B after last focus request,
  finish remaining rendering and reconcile B exactly once with no forced focus.
  Simulate B before last eligible request, A lands, final A emits no external B
  update. Also after-request B with no A proof: assert R1 outcome, never invent
  completion. Native known before/after fixtures test resulting foreground, not
  a new global click provenance mechanism; input guard failures abort.
- **Ignored focus/no event:** setter accepted or refused, immediate B mismatch,
  no future events. Assert no applied claim, no timer/spin/retry, active A remains
  command-owned until approved R1 reconciliation; if R1 selected report B plus
  incomplete status and handle a late A effect honestly. Test no permanently
  hidden result and no stale departure feedback.
- **Core/protocol reordered:** preserve command execution order; permute deferred
  observations, frame/view/member receipts, stable comparisons, duplicates and
  old generation/incarnation/intent replies. Stale reconciliation never overwrites
  newer command focus. Test known-none versus deferred versus unreadable, prompt
  close/hotplug/settings, shared-view and output scope/global focus interactions.
- **Windows offline:** async show late/ABA/outstanding contradictory posts, new
  commands mid-slice and during compare, pending flight identity reuse, safety
  before/after prime, latest-state fairness, preserved membership/operation ack,
  strict desktop-grant and at-most-once effects. R2 needs red unissued-final-focus
  test plus old/successor double-claim/lost-ack negatives if approved.
- **Gates after implementation:** locked core/protocol/Windows build/tests,
  strict all-target clippy, fmt/diff, stress definition/parser/fault mocks and
  live-library checks; shared-core changes need applicable non-Windows CI.
- **Native after gates:** fixture-only home-away, trio-churn and authored A/B
  spam at `-DelayMs 0`, no inter-command convergence wait/foreground repairs.
  Check final active memory, actual native foreground/visibility/frames and
  paired pager/world proof; record actual publication/compare/failure status.
  Include bounded fixture-controlled before/after request cases when feasible;
  silent-ignore/no-event guarantee stays deterministic offline if native host
  cannot force it. Zero added CLI pacing is not a physical-key-rate claim.
  Follow live guide, exact-owner stop/restore, fixture shutdown, preimage and
  residue verification. No live desktop action is performed in this design.
- **Keep:** command-owned logical state, latest-only event-driven owner slices,
  dependency/effect fences, flight identity, live game/desktop/no-forced-refocus,
  source-bound receipts and test evidence separation. **Replace:** core arrival
  transactions, completion-conditioned history, origin attribution flags and
  indefinite same-transition suppression. Defer/reconcile reports final surviving
  state instead of pretending to know why it changed.
- **Smallest proposed implementation:** core command-decision recording plus
  non-converging deferred-focus ingress; protocol update discriminant/live-context
  split, durable desired-active projection and stale-reconcile binding; Windows
  bounded latest drain, per-property proof/failure accounting and final compare.
  R1/R2 remain explicit choices, not changes smuggled into those three units.
- **Former next action, superseded:** resolve R1 (no-proof release) and R2 (unissued-current
  focus authority), then finalize the versioned generic contract and implement
  failing-first. Approved principle/semantics are recorded; runtime design is
  still proposed and no new runtime evidence is claimed.

## Prompt intent and render side effects (2026-10-27)

### Current outcome and boundary

- Retain command-time core choices and latest-only native convergence. Replace
  defer/reconcile: an intentional B activation after workspace selection wins
  even before arrival-A's focus request is issued. Core adopts B promptly,
  invalidates unissued A, and describes B's border in the next reply; no wait
  for final view/focus proof. "Prompt" is event/core-reply/paint latency, not a
  synchronous input callback or a guarantee about already-issued Win32 calls.
- The Layering principle and latest-world decision now express that intent.
  R1 is unnecessary: unknown rendering completion no longer blocks intentional
  ingress. No production/test code or wire shape changes in this investigation.
- **Unresolved:** available passive Windows observations cannot distinguish all
  genuine clicks/app self-activation from hide-triggered fallback. Source
  avoidance reduces that problem but does not yet solve empty arrivals or
  vetoed/ignored setters. The full click and Steam guarantees are not both
  demonstrated. Do not implement an invented classifier or call the design
  complete; this note stays active for the outstanding product decisions.

### 1. Why focus waits and what earlier focus would require

- `tiling_sys/world_diff.rs:3057,3451-3475` applies views before focus effects.
  Hidden/iconic/frameless preimages require a fresh visible framed target
  (`:3516-3553`); live homing must name a controlled scope and exact proven view
  (`:3579-3599`). This is our gating, not a Win32 requirement to wait for every
  departure hide before foregrounding a visible arrival.
- `tiling_sys/world_native.rs:1467-1499` hides departures then reveals arrivals.
  `read_view` (`:1714-1820`) proves **all** active members visible and inactive
  members invisible, not just target eligibility. Thus reveal -> focus -> hide
  cannot pass today's view gate while departures remain visible. A separate
  phased arrival authorization/proof would change that contract and needs review;
  do not relabel a partly shown view as fully applied.
- Preserve live target/scope/lifetime/modal/hook/move-size checks before and after
  the prime. `focus_write_vetoed` (`world_native.rs:796-974`) permits a readable
  ordinary non-fullscreen competing foreground; it does **not** veto every other
  app. Competing fullscreen and unreadable foregrounds still veto except existing
  narrowly proven lanes. The NA changed-foreground guard is separate
  (`world_diff.rs:3600-3727`), including fallback changes. Earlier focus over an
  unchanged ordinary departure does not itself require broadening the native
  foreground veto; bypassing the full-view gate is still a contract change.
- Exact-desktop authority remains fresh-selection/effect/target bound
  (`world_diff.rs:354-409`, `world_native.rs:2050-2069`) with post-prime desktop
  reproof (`:2241-2257`). Neither activating show nor durable desired-active
  memory may bypass these checks. Hidden/cloaked targets remain ineligible
  (`crates/tiler-core/src/window_policy.rs:127-165`).

### 2. Avoiding activation at the source

- Microsoft's [ShowWindow contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow)
  explicitly defines `SW_HIDE` as hiding the window **and activating another
  window**. Its target is unspecified. Async posting is not an activation-origin
  fence; current hide/reveal claims select pumped/nonactivating lanes
  (`world_native.rs:1249-1379`), not a guarantee that the OS never changes focus.
- An exploratory two-window fixture sequence observed A foreground -> hide A ->
  B foreground; activate B -> hide A -> B unchanged; hide both -> foreground
  outside the fixture. Native foreground/PID and fixture focus/visibility agreed.
  **Excluded from accepted live evidence:** required source/artifact provenance
  and recovery preflight were not captured and raw results were not retained in
  an evidence directory. Fixture shutdown/process absence were checked. These
  observations are illustrative only, not a production async-hide verification
  or proof of the user's Steam cause. No further live leg followed that gap.
- Candidate: nonactivating reveal -> prove target ready -> perform the existing
  eligible one-shot focus -> verify landing -> hide departure. It can avoid
  hiding the foreground when landing succeeds, but requires phased view authority
  above and revalidation before every remaining write. Failure/unsafe foreground
  must not trigger a fallback setter or leave an invented completed view.
  Empty/no-eligible arrivals have no authorized arrival target, so this ordering
  cannot eliminate every fallback. Activating `SW_SHOW`/`SW_RESTORE` would move
  focus outside the explicit gated setter and is not a safe shortcut.

### 3. Input evidence and game-safety costs

- Existing keyboard input sees key edges, not an Alt-Tab target; the conditional
  mouse hook only counts genuine motion (`ffi/llhook.rs:16-24,245-264`). WinEvent
  foreground edges expressly carry no cause (`tiling_sys/observe.rs:403-420`).
  Current subscriptions omit SWITCHEND (`tiling_sys/run_loop.rs:387-400`). Paths
  in these sections are relative to `crates/tiler-windows/src/` unless qualified.

| Source | Useful evidence / causal limit | Game-safety cost and recommendation, not selection |
| --- | --- | --- |
| `EVENT_SYSTEM_SWITCHEND` | [Documented](https://learn.microsoft.com/en-us/windows/win32/winauto/event-constants) released Alt-Tab and switched-to HWND, including single-app case without SWITCHSTART. Bind delivery identity/lifetime and validate source order/current applicability; a stale event cannot override a newer command. Does not identify arbitrary clicks, Task View or app activation | Existing out-of-context event mechanism, no new synchronous input-chain hop. Additional callback/drain work still occurs during fullscreen Alt-Tab; cost and native coverage unmeasured. Recommend this narrow first evidence improvement, with fixture validation, not a universal intent detector |
| Passive raw mouse `RIDEV_INPUTSINK` | [Queued background WM_INPUT](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input) proves button edges, not recipient HWND. [RAWMOUSE](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawmouse) commonly supplies relative motion, not an event-time screen click point. A later cursor/WindowFromPoint sample cannot reconstruct delivery or prove activation: capture, child/owner routing, click-through, movement and concurrent app activation remain | No synchronous LL input callback, but receives mouse collection traffic including high-rate motion even if only buttons are consumed. [Last registration wins per process/class](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerrawinputdevices), not across other processes. INPUTSINK continues in background/fullscreen unless removed; no NOLEGACY/capture/exclusive flags proposed. Latency/CPU/anti-cheat compatibility unmeasured; do not select always-on use to claim causality |
| Low-level mouse hook | [Pre-queue callback](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc), coordinates and injected flags improve observation, still no recipient/activation-cause token. It runs in our process, not an injected foreign DLL | Synchronous global input-path hop, hook-thread scheduling, timeout/silent removal. Passing through does not remove latency; fullscreen input traverses it until removal, including suspension-detection gaps. Reject as the default click solution: stronger cost without proof. No anti-cheat compatibility claim |
| App self-activation | [Foreground event](https://learn.microsoft.com/en-us/windows/win32/winauto/event-constants) reports resulting HWND; [SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow) documents eligibility, not a passive caller/reason feed. No supported general passive discriminator established for app request versus OS fallback. `WM_ACTIVATE/WA_CLICKACTIVE` reaches recipient WndProc, not our out-of-context feed | Existing WinEvents add no new input hook, but cannot supply missing provenance. Foreign injection/message interception is not proposed. Unknown risk must not be described as anti-cheat safe |

- [Out-of-context WinEvents](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook)
  are queued asynchronously. Neither that guarantee nor queued WM_INPUT supplies
  a cross-channel click->foreground causal order. Time adjacency, point/target
  equality, a drain boundary or persistence after apply is not proof.

### 4. Remaining ambiguity and recommended default

- Empty arrivals, refused/ignored focus, delayed client reactions, source loss,
  capture/click-through, and simultaneous genuine input versus native effects
  remain ambiguous. The original Steam activation remains **unattributed**, not
  a proven hide consequence. Own-focus flight identity is existing accounting,
  not proof every other HWND is external intent.
- Recommend factual actual foreground + explicit unknown; no remembered-focus
  update from unknown, no forced refocus, and retain current competing-foreground
  safety vetoes. Never promote the same unknown transition merely because a new
  sample or render generation arrived. Confirmed independent intent replaces it
  promptly. This is a **product decision, not adopted behavior**: it protects A
  memory but may miss a genuine B click/app activation and its core-owned border.
  Accepting unknown promptly instead preserves those actions but can reproduce
  Steam contamination. There is no timing-based third option satisfying both.

### 5. Smallest revised design and failing-first plan

1. **Core:** consistently record accepted command/lifecycle active choices using
   the earlier command-path audit, before native rendering; no transaction or
   completion-conditioned history. Separate actual action/subject context from
   authorization for host-focus observations to mutate focus/MRU/anchors. Prompt
   established intentional B replaces A and invalidates unissued arrival focus;
   no setter is emitted merely to echo already-actual B.
2. **Protocol proposal:** generic opt-in actual-context versus policy-update
   evidence (`intentional`, known-own accounting, `unattributed`, explicit
   none/unreadable distinctions), source identity/order/incarnation fencing,
   and bounded durable desired-active/source-intent projection. Exact fields
   remain unselected. Strict version/key checks
   (`crates/tiler-protocol/src/world_protocol.rs:1555-1615`) require coordinated
   version/capability routing; optional keys alone do not support old decoders.
   Preserve defaults for non-opted adapters. Unknown policy belongs to core.
3. **Windows:** latest-only bounded event-driven slices; process established
   intent before the next unissued write, fence obsolete effects/prime/setter,
   retain already-issued flights/async obligations, never infer origin from
   overlap. Border follows B's next core reply without waiting for view completion;
   safety presentation still uses actual host state. SWITCHEND is a candidate
   proven input source; general click/app evidence remains a blocker. Do not
   claim a simulated intentional event proves native click recognition.
4. **Separate proposals:** phased reveal/focus/hide authorization needs review
   with unchanged game/fullscreen/exact-desktop vetoes. Former R2 still asks
   whether a provably never-claimed current intent may obtain fresh one-shot
   authority; retain existing refusal if not selected. No grant copying/retry.

- Red core test: send W without follow to empty, select, assert W active before
  native work, leave early, retain W. Audit direct/directional/follow/stay/close,
  rejected/duplicate commands, exception targets and lifetime invalidation.
- Red click test: select WS1 with A pending; establish intentional B **before**
  any A prime/setter; assert B active immediately, no unissued A dispatch, B's
  border in next core reply, no later snap-back. Repeat with source-bound Alt-Tab;
  test stale/duplicate/reordered input relative to newer commands and same-target
  intentional input. Already-issued late A must be accounted honestly, not retried.
- Red Steam test: reproduce 107 -> deferred reveal 108 -> unattributed B ->
  departure 109 -> return. Actual B stays visible to safety; expected memory A
  is conditional on choosing conservative unknown policy, not proof of cause.
  Pair identical facts with established intentional B and require immediate B;
  repeat unknown after apply to forbid provenance laundering.
- Native evidence work: first establish compliant fixture provenance, recovery
  preflight and retained responses. Compare production nonactivating hide lanes,
  reveal-first, and focus-before-departure with native lifetime/foreground proof.
  Cover empty targets and veto/ignore without relaxing any gate. Synthetic
  Alt-Tab cannot establish physical shortcut/game acceptance; input/game evidence
  remains separate. No always-on input mechanism selected here.
- Offline interleavings: target visibility/cloak, full versus phased view proof,
  old async posts/ABA, new commands between slices, focus-flight identity reuse,
  before/after-prime safety, exact-desktop effect binding, one-shot double claims,
  command ordering/fairness and strict wire fixtures. No sleeps, settle clocks,
  polling, replay, fabricated completion or hidden raw safety facts.
- The subsequent user choice was ambiguity reduction first, not selection of
  either unknown policy; the measurements and staged design below continue this
  investigation. No implementation may claim the click gap is solved.

## Ambiguity reduction measurements and staged design (2026-10-27)

### Outcome and evidence boundary

- User selected reducing ambiguity before deciding whether unattributed
  foreground should update memory. Additional design input: post all arriving
  shows quickly, then prioritize proving them shown over unrelated rendering.
  Continue servicing safety, input and newer commands; do not block the owner
  or add a settle delay. Shown proof is not proof of activation intent.
- Production ordering, safety proofs and protocol are unchanged. Accepted live
  measurements below cover current ordering only. No staged production path,
  staged native timing trial or SWITCHEND implementation was delivered.
- Key result: in the rapid occupied <-> empty sequence, the 52 return windows
  contained only their accepted arrival target's foreground event. All 53
  unexplained-by-focus switch events occurred on **empty departures**. Putting
  arrival focus before departure hides cannot eliminate that empty case because
  no arrival target exists. This fixture did not reproduce Steam activation
  during an unfulfilled return, and does not establish app/click attribution.

### Accepted current-order measurements

- Source `030951a8f1f531e57f2b9218ecd77d61720b84c4`, clean tree at execution;
  owner SHA-256 `AFA1DD84D38BEF37475F888480C6BEAF6BDF5145AE16861224E9C805B114061D`,
  fixture SHA-256 `0C7DFC7AD5F28EC63F06A25C528E72D750893ADEB8D4B10C42D0005A39AE2563`.
  Native Windows, ordinary integrity, fixture-only tiler scope. Source/diff,
  launch identities, display preimage, settings bytes, taskbar/SPI, native/fixture
  observations and raw owner/observer/runner responses are retained outside the
  repository. Concurrent platform changes incorporated later are not this tested
  source revision.
- Recovery preflight proved exact-owner graceful exit and reveal of hidden
  fixture members, then independent idempotent restore. Accepted legs verified
  byte-identical settings, matching taskbar/SPI, fixture shutdown, owner exit and
  no owned process/ledger/pending-control residue. Passive observer release was
  verified by completed pump join plus process absence; its unhook counter is
  attempts, not successful-release proof. One normal observer exit code was not
  captured, but completion summary and process absence were.
- Locked fixture/Windows tests, live-library check, stress definition/parser
  check and fault mocks passed before live work. Current-order home-away and
  trio-churn stress runs passed (102 commands each); authored 104-select
  WS1 <-> empty WS2 spam passed. All used `-DelayMs 0`, no per-command native
  convergence waits, only explicit checkpoint/final proof. Transport still
  serializes publication: observed command rates were about 2.9/s for the mixed
  home-away case and 4.2/s for pure spam, not physical-shortcut-rate evidence.
- For timing, a separately pumped, bounded out-of-context observer retained the
  **source** HWND/thread/event time, not a later foreground sample. Its native
  imports and queue/lifecycle seams were checked, followed by a short native
  smoke. The final wrapped mixed home-away and pure-spam runs passed. Received
  versus drained was 385/385 and 338/338, respectively, with zero cap drops,
  residual queue, pump errors or accounting loss. Identity joins use the proven
  fixture roster/lifetime; cleanup/bootstrap events are excluded explicitly.

| Current-order case | Eligible arrivals with matched foreground | First visibility API post -> target foreground callback, owner clock | First cohort SHOW -> target foreground event, source clock |
| --- | --- | --- | --- |
| Checkpointed mixed home-away, 102 commands | 23/23 | Mean 79.4 ms, median 80 ms, range 74-84 ms | Mean 52.3 ms, median 47 ms, range 31-79 ms |
| Pure rapid WS1 <-> empty WS2, 104 selects | 52/52 home returns | Mean 80.8 ms, median 80 ms, range 74-90 ms | Mean 63.0 ms, median 63 ms, range 31-79 ms |

- The first column of durations is an **upper bound including hook delivery**,
  not the exact foreground-generation instant: visibility-post `call_mono_ms`
  and matched edge `queued_mono_ms` share the owner clock. The edge joins uniquely
  to the independent source event by event time, thread and lifetime-bound HWND.
  Drain adds roughly another 3 ms. Source SHOW/foreground deltas use their own
  event-time domain and have host tick granularity. No cross-process Instant/
  QPC subtraction, setter-return-as-landing, or timing-derived cause is used.
- Empty-away has no intended focus landing, not a failed arrival. The mixed
  trace separately contains 23 empty departures and six early switches with
  skipped focus; these do not enter the 23-arrival timing denominator. Pure
  spam has 52 empty departures and no unmatched eligible home arrival. Already
  visible directional focus is excluded from arrival timings.
- Async visibility API calls themselves took 0-1 ms in the earlier baseline;
  two-window post groups still had intervening per-member preparation/readback
  work (one retained show group had posts 33 ms apart). API time is not proof
  time. No attribution of the whole gap to one function or host behavior is
  established. Prepare-and-post batching must not remove claim persistence,
  per-window live gates or fresh pre-setter checks.

### Foreground census, not a cause classifier

| Observed switch window | Total foreground events | Exact accepted-focus-target compatible | No matching accepted focus flight |
| --- | --- | --- | --- |
| Mixed case, after first switch through last action | 108 | 23 arrivals + 46 directional changes | 39 |
| Pure 104-select spam, after bootstrap and before cleanup | 105 | 52 arrival targets | 53: 52 foreign windows, one other fixture member, all on empty departures |

- These partitions are **operational observations**, not causal verdicts.
  Accepted-flight compatibility uses unique source/identity joins and the issued
  target within the bounded effect window; it cannot rule out simultaneous
  genuine activation of that same target. Native foreground events contain no
  reason. The 39/53 unmatched events are an ambiguity census, not proven render
  effects or intentional app changes. All source facts remain available.
- Whole captures, including bootstrap/cleanup, contain 118 and 111 foreground
  events; filtering only non-fixture identities would incorrectly report 27/54
  as the unknown-cause totals. A managed identity is not an activation cause.
  No unrelated foreground event was observed between first arrival show and
  arrival-target foreground in the 23 + 52 matched return windows.
- Earlier analysis paired every shown member with its own later activation and
  yielded a spurious mixed-case median 109 ms / maximum 1813 ms. Those are
  withdrawn as arrival measurements. Initial fixture readback field-path errors
  and a missing terminal checkpoint in the authored case were corrected without
  product changes; failing evidence is retained. An external observer import
  lookup failed before installing hooks; explicit native bindings, export checks
  and native smoke repaired that plumbing. The rejected staged trial draft was
  never executed and supplies no evidence.

### Staged switch proposal: decision required, no gate change implemented

1. **Prepare/reveal:** retain the accepted select's desired view, member
   lifetimes, operation/effect and source-intent identity. Prepare required
   recovery records and eligibility checks; post all authorized entering-window
   shows nonactivating as a bounded batch, without unrelated geometry/overlay
   work between them. Recheck identity/safety at each native boundary. "Together"
   means prompt posts, not an atomic cross-process Win32 guarantee.
2. **Prove arrival-ready:** event-driven fresh readback proves every required
   arriving member visible under the existing identity/lifetime/scope rules,
   and the selected target visible, framed, uncloaked, non-iconic and homed to
   that desired output/workspace. Departing members may still be visible. This
   is a **new partial view-authority proof**, not `read_view == complete`.
3. **Focus:** only that accepted explicit-selection effect may use arrival-ready
   proof. Preserve all other target/foreground/modal/input/game checks and
   exact-desktop binding/reproof; spend the focus effect only at its existing
   one-shot dispatch boundary. Established newer intentional B cancels unissued
   A immediately, even while readiness is pending. Unknown B is not classified
   by phase or timing; existing competing-foreground vetoes remain live.
4. **Hide departures/final proof:** after exact target foreground is proven,
   revalidate latest desire, safety and foreground before departure writes.
   Hide only current departures; retain full final view proof requiring inactive
   members hidden. If the foreground changes again, that invalidates the claimed
   no-foreground-hide premise; process the new evidence rather than hiding an
   obsolete foreground or restoring A. Inventory/operation receipts never wait
   for this phase to fabricate completion.

| Existing boundary | Exact proposed change / preservation | Consequence and recommendation |
| --- | --- | --- |
| `world_diff.rs:3579-3599` requires `read_view == target`; `world_native.rs:1714-1820` requires all inactive members hidden | Add an explicit-selection-only arrival-ready authorization before departure hiding; do **not** weaken or rename final `read_view` proof. All arriving members and target still need fresh eligibility and exact homing/scope | Principal proof change requiring a decision. Recommend a narrowly scoped phased-proof prototype for ordinary eligible occupied arrivals, not a universal bypass |
| `world_diff.rs:3102-3148` spends the view operation around one hide/reveal batch | Represent unissued reveal/focus/departure phases as native operation accounting bound to the latest intent. A spent/possibly executed native phase is never replayed; partial visibility is never a completed view receipt | Requires deliberate phased renderer accounting, not moving a setter above an unchanged check. Hide -> reveal -> full proof -> focus leaves the original fallback and is not this proposal |
| `world_diff.rs:3506-3577,3729` freshness and one-shot authority | Keep current gates unless separately changed. Waiting for readiness may cross observations/revisions and stale the unissued focus effect | Former R2 is independently relevant: obtaining fresh authority for a provably never-issued current intent needs an explicit contract decision. Do not carry an old desktop grant or silently rearm the effect; without that decision, stale focus stays unfulfilled |
| NA changed-foreground guard, `world_diff.rs:3600-3727` | Retain original foreground baseline/source intent across bounded phases and revalidate before prime/setter; no exemption for fallback or an arbitrary matching HWND | Source-bound phase accounting must not erase a genuine intervening activation. Carrying proof across native-owner lifetimes needs review, not implicit widening of the same-pass hidden-departure lane |
| Native focus/game/desktop gates, `world_native.rs:796-974,2050-2257` | Keep readable fullscreen/unreadable foreground vetoes, scope/identity/modal/move-size/hook checks and post-prime exact-desktop reproof. Only the existing exact fresh selection/effect/target grant permits that desktop exception | Ordinary unchanged non-fullscreen departure can pass today. A departing fullscreen game may not: do not focus over it to satisfy staging. Hold or use existing authorized switch behavior, truthfully retaining possible fallback |

- **Empty/no-eligible arrival:** there is no target to foreground. Hiding its
  departing foreground still allows Windows to choose another window, as the
  `SW_HIDE` contract states. No-target does not mean OS foreground zero. Explicit
  desktop focus, a focus sink, or retaining a hidden foreground would be new
  policy/authority, not part of this proposal. Do not invent one to claim that
  Windows never chooses a fallback.
- **Partial/refused/ignored/superseded:** do not declare arrival-ready/final view
  or focus landing without proof; no timer, replay or corrective focus. Continue
  safety/input and event-driven latest-state convergence, preserving issued
  async obligations. A no-fallback preference cannot indefinitely block the
  latest switch: falling back to current authorized hiding may still cause OS
  activation and must be reported as such, not a successful staged handoff.
- **Estimate:** approximately 80 ms to callback is a measured two-window
  reveal/focus planning proxy, **not a demonstrated staged result or upper bound**.
  The measured empty -> occupied returns already have no outgoing foreground
  to hide, so reordering alone provides no demonstrated reduction in those
  spans. For occupied -> occupied, batching may remove departure work from the
  early critical path, but no numeric saving is established. Empty-departure
  ambiguity remains: the 53 unmatched events cannot be assumed eliminated.

### SWITCHEND, unknown policy and next evidence

- The live guide reserves OS shortcut journeys for physical user input. No
  synthetic Alt-Tab or fabricated SWITCHEND was used; passive capture ran with
  SWITCHEND off. Thus destination/ordering/cost validation is **blocked on a
  bounded physical Alt-Tab leg**, not passed from documentation or fake events.
  No conditional production/protocol implementation was started without it.
- Candidate validation: bind fixture windows and capture SWITCHSTART/END plus
  foreground source HWND/thread/event time on one pumped event feed. Check the
  documented destination, START-less END case, foreground-before/after END,
  cancellation and intervening newer workspace commands. Observed order is not
  a general host guarantee. No new synchronous input-path hook is needed, but
  callback cost and fullscreen/game compatibility are not measured here.
- A minimal future generic intentional-focus input still needs identity,
  source ordering and stale-command fencing, not merely `intentional:true` on
  the latest snapshot. A delayed END must not overwrite a newer core selection;
  sampling the current core revision at callback delivery does not prove its
  generation-time order. Strict protocol version/compatibility routing and
  legacy-adapter defaults remain part of that contract decision.
- **Policy recommendation, not adoption:** do not blindly accept unmatched
  foreground into memory: every rapid roundtrip exposed empty-departure
  ambiguity without authored focus intent. Conservatively retain core memory
  while exposing actual facts and respecting live focus loss; intentional
  evidence must still apply promptly. However these fixture-only, no-click
  successes do not measure missed genuine clicks/app self-activation or the
  reported Steam case. Honor the selected ambiguity-first direction: complete
  phased-proof review and physical SWITCHEND evidence before choosing either
  unknown policy as the product default.
- **Next action:** decide whether to prototype the narrowly scoped arrival-ready
  proof and, separately, any never-issued-focus continuation authority. Arrange
  a bounded physical Alt-Tab capture before its conditional implementation.
  Keep this record active; no policy, safety-proof or protocol change is adopted.

## Independent implementation scope (2026-10-28)

- Approved outcomes: record core-selected active windows when commands execute;
  process commands in order while Windows skips superseded unissued rendering.
- Acceptance: per-path failing-first core regressions; ordered queued-command
  coalescing with current safety/effect/hidden-write/flight fences; full native
  core and Windows suites, formatting and strict clippy; fixture zero-delay
  stress and independent recovery proof with measured actuation counts.
- Units: core command-path audit and linked-focus recording; Windows intake and
  latest-render scheduling; independent safety review; offline and native gates.
- Parked: arrival-ready phased proof, unknown-foreground policy, fresh authority
  for never-issued focus, and physical SWITCHEND attribution. Current host-focus
  convergence remains unchanged; this scope cannot claim to eliminate the
  unattributed Steam contamination or recognize every genuine click.
- Audit confirms `world_tiling_views.rs` emits selection focus without recording
  the chosen linked window. Direct and floating/no-focus directional paths also
  need recording coverage. Existing Engine commits already cover ordinary tiled
  directional focus and transfers. Exception/non-leaf storage may require a
  separate core-model decision and is not silently added.

## Command-owned memory outcome (2026-10-28)

- Delivered subset: direct linked focus, view-return fallback and global-workspace
  swap fallback record their chosen window before emitting the native focus
  effect. Existing synchronous Engine focus/move/lifecycle commits already own
  the other tiled choices; redundant move and vacuous floating-lane recorder
  calls were removed. No protocol or host-observation ingestion changed.
- **User-accepted 2026-10-28:** preserve `Session::remember_focus`'s
  existing fail-closed contract. The helper returns its actual boolean result;
  callers deliberately preserve the accepted effect on false, rather than
  weakening a gate, inventing a rejection, or retrying. False is not success or
  a promise of later repair. Tests pin false plus unchanged memory/focus for
  pending focus, active drag, divergence, unknown binding and held scope.
- Precisely parked: command effects accepted over held or not-yet-linked homes;
  pending/drag/divergence or invalid binding/topology that refuses session focus;
  exception and floating-workspace/non-leaf storage. Pending plans own their
  staged focus; rebinding underneath them needs a core-contract decision.
  Exception targets have no leaf in the existing `last_active` model. No new
  storage, staging transaction or public diagnostic was added. Normal command
  reachability of pending/drag/divergence residue was not established; helper
  refusal tests exercise those backstops directly.

| Path | Ownership and discriminating evidence |
| --- | --- |
| Direct focus | New recorder; disabling it leaves remembered A while the explicit effect names B; restored green |
| Tiled directional focus | Existing `verify_focus` commit; disabling its `last_active` apply leaves observed B while the effect names A; restored green |
| Floating/sticky directional focus and float moves | Non-leaf/exception subset parked; characterization pins emitted floating target without overwriting tiled memory |
| No-focus cross-output focus | Chooses the destination's already-remembered leaf; characterization pins effect/memory agreement, not a new memory choice |
| Local move and neighbor swap | Keep the mover active; admission requires mover equals observed focus. Direct command-seam tests skip the report sweep; Engine pre-command convergence and move commit redundantly record B. Disabling either alone stays green; disabling both leaves stale C while each effect names B. Restored green for both paths |
| Cross-output move | Existing `verify_move` commit; disabling it with wrapper recording absent leaves destination's stale C rather than mover A; restored green |
| Send/move with follow | Existing lifecycle commit; disabling its `last_active` apply leaves destination memory empty rather than mover A for same-output sends, or stale D rather than mover A for direction-only cross-output sends; both restored green |
| Send without follow | Existing lifecycle successor commit; disabling its memory apply leaves source memory empty rather than the chosen survivor for both same-output and direction-only cross-output sends; both restored green. Direct/relative/numbered and operation aliases share `execute_send`; direction-only sends use `execute_output_send`, with the same lifecycle owner |
| Workspace select/arrival/`retain_view` | Shared by direct, numbered, relative, previous and trailing selects. Direct fallback test avoids the report sweep: remembered A becomes ineligible, effect chooses B; recorder disabled leaves A, restored green. Redundant select chooses nothing new |
| Global-workspace swap fallback | New recorder is load-bearing: domain relocation alone retains ineligible B1; explicit fallback effect names B2. Disabling recorder leaves B1, restored green |
| Close/remove fallback | Convergence-owned, no close `WorldCommand` or command focus effect. With closed B absent and empty observed focus, existing MRU chooses A. Disabling convergence memory update leaves no resolvable remembered window; restored green |

- Twenty-four focused regressions pass. The original purported view test only
  exercised direct focus; a later full send/select sweep pre-recorded selection
  memory. Neither is counted as fallback red evidence; the direct `retain_view`
  discriminator replaces that oracle. Existing Engine ownership probes were
  temporary mutations and are fully restored.
- Independent core review found the missing close fallback discriminator; that
  finding is resolved once with the convergence-owned red/green test. No other
  correctness blocker was found. Core source/tests are independent of Windows
  coalescing; no shared platform scheduling change is required for this subset.
- Departure and observation ingestion of `report.focused_global` is unchanged.
  An unexplained Steam foreground change can still overwrite the core's active
  window before departure. The Steam spam bug is only partly addressed until
  conservative A versus permissive B unknown-foreground policy is decided.
  Staged arrival authority, R2 and SWITCHEND attribution also remain parked.
- Offline verification: locked core/protocol/Windows tests pass (core library
  972; Windows library 1160, one existing ignored; integration/doc tests pass),
  strict locked all-target clippy for those crates, workspace format check and
  diff check pass. Other buildable suites: settings exporter 14, live fixture
  119 and macOS integration 150 pass. macOS library has 1336 passes and 19
  foreign-host failures; an unchanged-HEAD baseline has the identical failure
  list. The KDE service crate does not build on Windows (`rustix` and Unix OS
  imports); its tests were not represented as passing. No live action was run.
- Next core action: decide only the parked memory/model and foreground-policy
  subsets above; the linked, safely bindable command-choice work is complete.

## Windows batch-local coalescing outcome (2026-10-28)

- **User-accepted 2026-10-28:** coalesce pure numbered/trailing selection
  batches and previous/relative history batches within their existing drains.
  Dispatch every eligible command in order, acknowledge exact carried receipts,
  and run the native diff only for the surviving selection. Numbered sends
  flush the pending selection first and retain their full membership pass.
  No cross-queue ordering or cross-tick pending slot is invented.
- **User-accepted 2026-10-28:** superseded commands service an owed
  observation-only fence before their single dispatch, using core-only capture,
  delivery and acknowledgment. Rejection catch-up is core-only too. No retry,
  wait, timer, effect remint, grant transfer or observation-ingestion change.
  The full command route retains its existing refusal/recovery behavior.
- Superseded legs perform no rendering, including observation catch-up. They
  retain fail-closed admission: installing the owned lifetime-tag property on
  a newly proven member is a native identity-binding write, not rendering.
  Removing that proof would weaken later write authorization. Offline admission
  coverage counts this install seam separately and confirms an unproven stranger
  stays unbound; it does not claim a live new-member tag install never occurs.
- Final flush rechecks suspension, elevation and topology existence; all existing
  native game/fullscreen/unreadable-foreground/exact-desktop checks, effect
  fencing, hidden-write fences, in-flight focus rules and bounded pager tracing
  remain intact. Superseded incorporation does not advance native diff/proof
  watermarks, clear a gesture terminal, or create successful native receipts.

| Intake family | Delivered scope / mechanical barrier |
| --- | --- |
| Numbered/trailing selection | Latest-only within one digit-intent batch |
| Previous/relative workspace history | Latest-only within one history-intent batch; every toggle/step still executes in core |
| Directional focus, including no-focus crossing | Per-command: chord-origin advancement and the next live foreground/output context depend on the prior native focus |
| Directional move | Per-command: fresh subject/frame/focus and clear/arrival proof depend on the preceding application |
| Numbered/relative/directional sends | Full membership barriers: distinct windows' transfers are not overwritten state; chained subjects depend on homing receipts and origin/follow focus |
| Workspace migration | Per-command: next source/view resolution depends on applied view proof |
| Toggles, resize, launch, close, gesture, settings/lifetime | Unchanged per-command/external boundaries; no elision of effects or distinct subject obligations |
| CLI and mixed queues | Unchanged: one pending request per control file and separately drained families, not one source-ordered buffered command stream |

- Eighteen focused tests pass. Command and observation apply counters cover the
  real native-diff entry points, so catch-up cannot hide an intermediate render.
  Three alternating commands advance three revisions but only one native pass;
  forced catch-up adds one incorporated observation and no render. Disabling
  pre-dispatch fence service makes that test fail; restoring it passes. A linked
  send produces the real window transfer after the full selection barrier.
  Trailing selection, history toggle/relative ordering and two-output final views
  match sequential reference executions. Suspension/elevation flips at flush
  produce no native pass. Gesture coverage pins the single-slot stash only, not
  mid-burst terminal acknowledgment; moving-cursor burst coverage remains open.
- Independent final Windows safety review found no blocker. Its low coverage
  findings for trailing selection, history toggle and two-output state received
  tests; the narrower gesture/moving-cursor limits above remain explicit.
- Locked core/protocol/Windows tests pass (3069 passed, one existing ignored;
  Windows library 1171). Strict locked all-target clippy with `-D warnings`,
  workspace format check and `git diff --check` pass. Live-library parser and
  45 fault mocks, stress parser/seam and definition mocks pass.

### Accepted native verification

- Fixture-only, ordinary-integrity native Windows; source dirty `83f22aa7`.
  Stress owner SHA-256
  `0390A4B985032CAEE406DA1A02ED502185E85AB246A2FE8C1D9DA80F00274C97`;
  fixture SHA-256
  `650C87CB9AEB1340D3C4E1671A2757F23C56F0510D8D118474017689886DB592`.
  Source/change lists, exact launch identities, settings bytes, taskbar/SPI,
  display baseline and raw replies/logs are retained outside the repository.
  A changed-file list alone does not establish source byte equality.
- Required bounded recovery proof precedes the accepted stress legs: two
  identity-verified fixture windows hide; the exact owned watcher and primary
  are terminated; independent readback proves both still hidden after primary
  exit. Standalone restore reports two restored windows and a cleaned ledger;
  independent visibility plus settings/taskbar/SPI readbacks match baseline.
  The proof artifact is separately hashed in retained evidence. Graceful and
  exact-identity forced exit of a non-hooking fixture were also proven.
- Earlier six runs at 0/10/50 ms passed but lacked the independent pre-run
  hide/restore proof. They are excluded from acceptance, not retroactively
  validated by their cleanup. The two runs below followed the corrected proof;
  each completed exact-owner stop/restore, fixture shutdown, settings restoration
  and clean run-owned process/ledger/control state. An unrelated unproven fixture
  process with no test windows was preserved, never claimed as run-owned cleanup.

| Accepted case, added delay | Commands | Show posts | Hide posts | Native focus setters |
| --- | --- | --- | --- | --- |
| home-away, 0 ms | 102 | 50 | 50 | 73 |
| trio-churn, 0 ms | 102 | 75 | 75 | 74 |

- Counts include command and observation applies, deduplicated by fresh revision
  with paired pager projections excluded. Stress windows contain 291 and 288
  fresh revisions respectively. Excluded bootstrap/setup contributes zero
  visibility posts and zero/one focus setter respectively; no cleanup-tail apply
  is included. Count posted visibility APIs and `primitive_called:true`, not
  desired operations or attempted effects.
- No comparable HEAD-only baseline was built, so no native count reduction is
  claimed. CLI publication uses single-pending request files (61 transport gaps
  per case); these successful zero-added-delay legs do not demonstrate keyboard
  bursts or a measured coalescing speedup. Injected-key filtering is unchanged.
- Next: review the tentative bounded scheduler choices; general cross-family or
  subject/proof-independent intake needs a separately justified extension.
  Staged arrival-ready authority, unknown-foreground A/B, R2, SWITCHEND and
  exception/non-leaf memory remain parked. This combined investigation stays
  active for those decisions; the bounded Windows implementation is complete.
