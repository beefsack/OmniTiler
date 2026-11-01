# Core/platform interface review (pre-0.1)

Goal: review the core/platform contract (world report/reply, commands,
presentation, settings, capabilities) for correctness, shared-logic placement,
latency/chattiness, platform neutrality, platform-side workarounds that should
be contract changes, and readiness for post-0.1 tabbed window groups. Pre-0.1:
breaking changes are acceptable.

Status: evidence packages A-G complete; findings F1-F20, candidate
recommendations R1-R9, governance reviews (P, G, M series) recorded; user
decisions pending. Findings are appended as they are verified. Each finding
cites source. "Candidate" means not yet decided; nothing here is approved
product policy until recorded in `docs/decisions.md`.

Review stance (user 2026-10-30): every principle and decision, including the
project's definition of what a principle and a decision are, is open to
review. Existing records are evidence of past intent, not constraints on this
review; a prior decision proven suboptimal is a candidate for change.

Context: a coordinated visibility contract bump (v4) is already designed
([design](windows-workspace-return-focus.md#coordinated-visibility-contract-design-2026-10-30));
current contract is v3 (`crates/tiler-core/src/world_report.rs:41`).

## Findings

### F1. Report mixes six concerns and resends all of them every report

Source: `crates/tiler-core/src/world_report.rs:1010-1219` (`WorldReport`).

One message carries:

- transport/session envelope (owner, incarnation, generation, seqs, request id);
- capabilities (`host_owned_workspace_domains`, `equal_workspace_count`,
  `birth_clear`);
- behavior settings (`workspace_mode`, `fixed_size_predicate`,
  `migration_source_refill`, `anchor_fallback`, `no_space_result`,
  `switch_target`, `fresh_placement`, `focus_follows_cursor`, `cursor_warp`,
  `default_tiled`, `born_maximized`, per-screen `gap`/`outer_gap`, and inside
  presentation `border_enabled`, `underlay_enabled`, style px);
- full world facts (screens, windows up to 10,000, native rows, closes, views);
- one command;
- actuation feedback (frame/member/view/clear/creation attempts);
- presentation/cursor/gesture ingress.

Observations:

- Capabilities are per-report flags although they are stable per adapter
  incarnation (the v4 design already says capability changes need a fresh
  bootstrap). Per-report capabilities invite mid-stream flips that core must
  defend against.
- Settings are resent, digested (`report_digest`, line 1503) and validated on
  every report although they change rarely.
- Candidate: split into a handshake (version + capabilities, fixed for the
  incarnation), a settings message (revisioned, sent on change), and the
  observation/command report. Status: to verify cost and consumers.

Verification (package A, A2/A7/A11): confirmed, with corrections.

- Correction: capabilities are not stable on every adapter today. KDE computes
  `hostOwned = desktopsReadable && screenEntries.every((s) => s.current !== "")`
  per capture (`kwin/src/world-observe.ts:1337`): a transient readability gap
  flips the adapter into the logical-evidence lane for that report. This
  conflates a capability ("this host owns workspaces") with per-report
  completeness, which the contract already has (`inventory_complete`,
  `ScreenReport.complete`). Strengthens the handshake split: capability fixed
  per incarnation, unreadability reported as incompleteness.
- Several settings are constant on some adapters (K `switch_target`,
  `fresh_placement`, `focus_follows_cursor`, `cursor_warp`, `action_cached`;
  M `workspace_mode`, `default_tiled`, `migration_source_refill`,
  `gesture_barrier`, cursor fields). Per-report resend gains nothing for them.
- Semantic drift in a shared field: `ScreenReport.primary` is "first
  work-area device" on W (`world_capture.rs:1032`), "active screen" on K
  (`world-observe.ts:1330`), "explicit input else first output" on M with
  production input `None` (`world_builder.rs:476-480`, `world_runtime.rs:724`).
  One field, three meanings.
- M uses `PHASE1_GAP`/`PHASE1_OUTER_GAP` constants (8) in production
  (`world_runtime.rs:725-726`), i.e. gap settings are not user settings there.

### F2. Stringly-typed and stale command vocabulary

Source: `crates/tiler-core/src/world_report.rs:862-1003` (`WorldCommand`).

- `direction: String` on `Focus`, `FocusDirectional`, `MoveDirectional`,
  `SendDirectional`, `MigrateDirectional` although a typed
  `crate::directional::Direction` exists and is used in
  `CommandResult::Migration` (line 1402). Validation happens later instead of
  by type.
- `Focus { window, direction }` carries a `direction` for a by-id focus; purpose
  unclear (to verify).
- `ToggleMaximize` doc (lines 988-993) describes `clear_fullscreen` and
  `was_maximized` fields that are not on the variant (they are on
  `CommandResult::Maximize`). Stale doc.
- `FocusDirectional` doc (line 948) names "the KDE focus policy": a platform
  name inside core contract docs.
- Overlapping variants: `SwitchView` vs `SelectWorkspace`; `Send` vs
  `Operation(CoreCommand::SendToWorkspace/SendToOutput)`. `Operation(CoreCommand)`
  is a second command route into the legacy per-domain operation vocabulary.
- `SendSource::HomeThenAction` (CLI) vs `HomeThenCachedLive` (keyboard) give the
  same user intent different fallback chains depending on entry route.

Verification (package A, A5/A7): confirmed, with additions.

- `Focus.direction` is validated and then ignored for targeting
  (`world_commands.rs:70-102`): dead payload that still costs validation.
- `SwitchView` bypasses the global-unique swap/history logic that
  `SelectWorkspace` runs (`world_commands.rs:119-188`); W's selection-arrival
  grant excludes it (`tiling_sys/world_diff.rs:354-368`) while M groups it with
  selects (`world_dispatch.rs:1612-1617`). Two platforms already disagree on
  what it is.
- New: the `Operation` doc says migration and domain release are refused
  (`world_report.rs:999-1001`), but the validator accepts `ReleaseDomain` and
  `MigrateWorkspace` (`:2645,2695`) and `world_operations.rs:1289-1355` executes
  them. Stale contract doc on a live path.
- New: macOS keyboard focus/move route through `Operation(CoreCommand::Focus/
  Move)` (`world_command_map.rs:240-248`, `runtime_thread.rs:1693-1702`), not
  `FocusDirectional`/`MoveDirectional`, which W and K use. The same user action
  enters core through two vocabularies depending on platform, so behavior
  parity depends on two code paths agreeing.
- No production constructor located for `SwitchView`, `SetWorkspaceTiling`,
  `MigrateWorkspace` (W/M), `ToggleMaximize` (W/M), `Focus` (W/M). Absence of
  a constructor is not a capability verdict.

### F3. Presentation ingress duplicates window facts and carries adapter-retained layout

Source: `crates/tiler-core/src/world_report.rs:582-652`.

- `PresentationIngress.foreground_{minimized,maximized,fullscreen,cloaked,hidden_workspace,shell}`
  restate facts about one window that `WindowFact`/classification already
  carry; `foreground_cloaked` is Windows vocabulary.
- `PresentationPagerFrame.float_frame/tile_frame` are "the adapter's retained
  slot snapshots": the adapter keeps a copy of core layout state so core can
  read it back. Core already owns allocations and float rects.
- `paused` ("focused-fullscreen-game lane") is decided by the adapter, while
  the neutral focused-fullscreen suspension (`suspended_output`) is decided by
  core. Two mechanisms for overlapping intent; to verify.

Verification (package A, A3/A7/A11): partly confirmed, partly corrected.

- Correction: ingress may name an unmanaged foreground
  (`world_report.rs:606-609`), so `foreground_*` is not pure duplication; it
  is duplication only when the foreground is a reported managed window.
  `foreground_cloaked` and `foreground_shell` are constant false on K and M.
- Correction: float/tile pager frames are not copies of core layout. Only W
  populates them, from native frame snapshots (`tiling_sys/world_tick.rs:
  1390-1419`, hide seeds `tiling_sys/world_native.rs:1613-1633`). K and M send
  `None`. Core pager uses live > float > tile and never falls back to
  `WindowFact.rect` or core allocation (`world_presentation.rs:625-672`), so
  hidden windows on K/M get no miniature. The underlying need is "last known
  frame of a window that is hidden now", which core can retain itself from
  prior facts; it is not a W-specific concept.
- `paused` is true only on W's limited-fullscreen path
  (`tiling_sys/reconcile.rs:203-218`); K and M are constant false.
  `suspended_output` is an engine-config opt-in that only the KDE broker
  enables (`crates/omnitiler/src/world_broker.rs:446-452`). Two
  focused-fullscreen suppression mechanisms, each used by one platform.
- New: pager consumers read different flags for the same concept: W
  miniature focus uses `current` (`tiling_sys/pager_host.rs:425-429`), M uses
  `contains_focused` (`crates/tiler-macos/src/pager.rs:209`). K parses pager but
  no native renderer reads it. Core `label` is unread by W and M mappers.

### F4. Effect identity encodes semantics in id strings

Source: `crates/tiler-core/src/world_report.rs:215-252`.

- Explicit vs automatic focus is distinguished by id prefix (`fx-<seq>` vs
  `af-<window>`) and `is_explicit_focus_for` formats a string to compare.
  Automatic focus "must never actuate focus on adapters keeping observed-focus
  authority": per-adapter behavior keyed off an id convention rather than a
  typed field. Candidate: typed `cause`/`origin` on the effect.
- `FrameSetter` names the setter disposition for member, view, clear and
  creation attempts too. Naming nit: `SetterOutcome`.

Verification (package A, A7): confirmed. Prefix semantics are functional on K:
`af-` decides whether to preserve a focused float over a newcomer, permit an
admission float target, and fence fullscreen/maximized siblings
(`kwin/src/world-apply.ts:2302-2376`). M and W match exact `fx-<seq>` identity;
M also routes selection focus through deferred correlation
(`world_dispatch.rs:1975-1999`). So the effect's cause is part of the contract
in practice but untyped.

### F5. Settlement-by-attempt makes the contract operation-centric, not state-centric

Source: `crates/tiler-core/src/world_report.rs:371-580, 1186-1213, 1221-1284`;
`docs/decisions.md` "core compares desired and observed state and decides
re-apply ... core owns anti-fighting and completion" (2026-10-20).

- Desired state is shipped whole (good), but core also tracks each actuation:
  five raw attempt types today (frame, member, view, clear, native creation),
  retained operation ids, `TransferTarget.held`, a 14-value
  `OperationOutcome`, and the v4 design adds `visibilityWork`,
  `visibilityResults`, `selectionProofs` and `visibilitySettled`.
- This is the main driver of contract size and of the per-actuation
  core roundtrip. It partially contradicts the "ship whole world, platform
  converges" direction. To evaluate: which attempt lanes are needed for core
  policy (learned minima, clamp suppression, ambiguity at-most-once) versus
   which could be plain facts.

Verification (package A, A7/F5): partly refuted.

- Refuted: attempts do not force a core roundtrip per actuation. W and M
  batch latest-wins attempt rows into the next report
  (`tiling_sys/world_sequencer.rs:254-285,338-347`; `world_runtime.rs:601-604`).
- Unverified: no size or latency attribution shows attempts as the main size
  driver. Remove that claim until package C measures it.
- Confirmed consumers: frame bursts and learned minima
  (`world_frames.rs:647-694`), exact member/view correlation
  (`world_settle.rs:28-55`), float settlement and ambiguity hold
  (`world_settle.rs:1737-1795`). These are real core policies, so the question
  is narrower: can each lane be a typed per-window "last write" fact (target,
  readback, time) keyed by desired-state revision instead of by operation id?
- Lane asymmetry: M sends no `clear_attempts` and has no production reader of
  `WindowDesire.clear_operation`; W sends no `creation_attempts`. Not wrong per
  capability, but every lane is a separate contract surface to keep in parity.

### F6. Every accepted report deep-clones the engine and JSON-encodes the reply, on every platform

Source: `crates/tiler-core/src/world_engine.rs:1220-1424`, `:1887`;
`crates/tiler-core/src/world_sizing.rs:1858-1883`.

- `handle_report` computes `report_estimate`, full validation and
  `report_digest`, then `self.clone()` of the whole `WorldEngine` (including
  the inner session `Engine`) to stage the run (`:1391`), then
  `encode_reply_canonical(&reply)` purely to measure length (`:1398-1400`),
  then stores `last_report = Some(report.clone())` (`:1887`) and
  `last_reply = Some(reply.clone())` (`:1407`).
- The clone exists so that a reply-size overflow or a command rejection can
  discard the staged state. The size limit is a D-Bus transport limit, but W
  and M call core in-process with typed values and still pay the clone and the
  JSON encode on every accepted report.
- A conservative pre-mutation bound already exists (`reply_upper_bound`,
  `:1343-1354`), so the exact post-encode check only matters when the bound
  passes but the encoding exceeds it.
- Candidate: move the exact size gate to the transport (KDE broker); make
  command rejection non-fatal to the observation (see F7) so staging is not
  needed for that; then evaluate dropping the per-report engine clone.
  Measure first (package C).

Verification (package C, C5/C6):

- The clone is `WorldEngine::clone`, which also copies `last_report` and
  `last_reply` (previous full typed payloads), every retained/history/frame
  map, staged observation, and each `Session` including minimized-slot trees
  and optional pending desired/drag topology snapshots. Only the
  `Arc<dyn LayoutPolicy>` is shared. Cost per clone grows with windows and with
  whatever optional snapshots are populated.
- The exact canonical encode string is used only for `.len()` and dropped;
  the KDE broker encodes the reply again through a separate protocol writer.
  The core encode is pure waste on every platform for transport purposes.
- The cited estimator test `estimate_upper_bounds_canonical_encoding` was not
  located; no test exercises `reply_upper_bound` directly. So the claim that
  the bound makes the exact gate redundant is plausible but unproven.
  Nitpick: a documented invariant points at a test that does not exist.
- No measured split of clone vs validation vs policy vs encode exists (C2).

### F7. A rejected command rejects the whole report, including its observation

Source: `crates/tiler-core/src/world_engine.rs:1816-1818`;
`crates/omnitiler/src/world_broker.rs:925-985`.

- `CommandApplied::Rejected` returns `WorldOutcome::Rejected` for the whole
  report, discarding the staged observation. The KDE broker then settles the
  command as failed and re-runs the same facts observation-only
  (`dispatch_plain_fallback`): two `handle_report` calls, two engine clones.
  W and M must have equivalent recovery logic (not yet located).
- Candidate: command outcome becomes part of an accepted reply
  (`command_result: Rejected { kind }`), the observation is always
  incorporated. One roundtrip, no adapter fallback lane.

Verification (package B, B4): confirmed; the three platforms recover
differently.

- W: synchronous observation-only catch-up on the same tick products
  (`tiling_sys/world_tick.rs:2352-2410`); command dropped.
- K: settle failed, re-run observation-only in the same broker call
  (`crates/omnitiler/src/world_broker.rs:977-998`).
- M: clear flight, return `Refused`; no same-report observation fallback
  (`crates/tiler-macos/src/world_dispatch.rs:742-783`; gesture `:487-499`;
  selection `workspace_select.rs:1817-1822`). M drains pending attempt maps
  with `mem::take` before dispatch (`world_runtime.rs:327-349,601-604`) and the
  rejection branches do not reinsert them. Whether those attempts are lost is
  unverified, but the contract makes this easy to get wrong.
- Three different recovery behaviors for one contract outcome is itself the
  finding: the contract forces each adapter to invent rejection recovery.

### F8. KDE transport re-parses the full reply JSON repeatedly

Source: `crates/omnitiler/src/world_broker.rs:836-858, 930-975, 1868-1937,
2130, 2161`.

- Request: script builds inner JSON manually plus `JSON.stringify` envelope;
  broker parses the envelope, clones the inner `Value`, stamps it,
  re-serializes (`:939-948`) and parses it again in `decode_request` (`:952`).
- Reply: core encodes once for its size gate (F6); broker encodes again
  (`encode_reply`, `:967`); then `result_of`, `preview_suspended_by_presentation`,
  `drag_preview_of`, `strip_result`, `inject_gesture_assoc`, `bind_response`
  and `project_reply` each parse the full reply string into `serde_json::Value`
  and most re-serialize it. The script then `JSON.parse`s it.
- The recorded synthetic 4 MiB case (A8) took 904 ms to parse the reply.
- Candidate: broker works on the typed `WorldReply` and a typed broker
  envelope, serializes once. Pure IO improvement, no contract change. Measure
  in package C.

Verification (package C, C4): corrected counts for one fresh accepted
capture. Request: 3 full parses (outer envelope, commandless pre-drain decode,
dispatch decode), not 2; plus 1-2 stamped re-serializations. Reply: 5-6 broker
parses commandless, 6-7 with a queued command; 6 serialization calls including
core's discarded encode. The script side builds the envelope by string
templates and parses the reply once more. No helper cost has been measured, and
see F15: at historically measured scales, codec time is small next to core
time, so F8 is a cleanliness/typing issue first and a latency issue only at
the multi-MiB extreme.

### F9. Platforms work around missing core state by echoing core intent back as facts

Source: `crates/tiler-macos/src/runtime_core.rs:70-95,306-322`.

- macOS has no native float state, so the adapter re-asserts
  `floating = true` for "retained intentional floats" from the last reply
  (`normalize_wire_flags`), because otherwise "the next converge would read
  the floated window as tiled and revert the exception". Core's own float
  intent is laundered through the adapter as an observation.
- The same shape appears in F3 (W retains frames so core can read them back).
- Candidate: a fact field that is not natively observable should be omitted
  (`Option`/capability), with core treating its own retained intent as
  authoritative. Platforms with native float state (W, K) keep reporting it
  as an external-change signal.
- macOS also hardcodes `maximized`, `sticky`, `fit_excluded`,
  `fixed_suppress` to false (`runtime_core.rs:312-317`): fine if those are
  capability-absent, but the contract cannot currently express "unknown"
  versus "false".

Verification (package B, B1): confirmed on all three platforms.

- M: `KnownState::of_reply` takes `exception: row.floating` from the last reply
  (`runtime_core.rs:52-62`) and turns it into the next `floating` fact.
- W: `floated` "carries an exception across domain release" and "row assembly
  rides these tokens floating even when the session carries no exception"
  (`tiling_sys/mod.rs:761-768`); also `sticky`, `fixed_tile_overrides`,
  `fixed_origin_overrides`, `fixed_auto_seen`, `born_first_exit`,
  `restart_reject_hold` feed `float_lane` (`tiling_sys/world_capture.rs:676-687`).
- K: `floatingIds` is populated from reply `floating === true &&
  frame_auth === "untouched"` (`kwin/src/world-entry.ts:3286-3289`) and is fed
  back as observation input; persisted by `crates/omnitiler/src/float_intent_store.rs`.
- M also re-publishes prior held desired geometry when fresh rows omit it
  (`world_dispatch.rs:1185-1219`).
- So float/tile intent has three adapter-owned stores plus core's own. Core
  intent that only survives because adapters echo it is the central
  "platform hack instead of core change" pattern in this review. Core already
  persists sessions across incarnations (`world_engine.rs:1329-1332`); intent
  persistence across restarts is the remaining reason adapters hold it.

### F10. Core policy is also consumed as a library, outside the contract

Source: production imports of `tiler_core::window_policy`, `visual`,
`size_hints`, `cursor_targeting`, `output_selection` in
`crates/tiler-windows/src`, `crates/tiler-macos/src`, `crates/omnitiler/src`.

- Shared-library use is acceptable per the review brief. But some calls are
  policy decisions made adapter-side before core sees the report:
  - W decides command refusals itself (`tiling_sys/chord_resolution.rs:153-191`:
    `float_toggle_refusal`, `sticky_toggle_refusal`,
    `orientation_toggle_refusal`).
  - W admission/restart decisions (`tiling_sys/admission.rs:257`
    `should_clear_maximize_at_admission_with_mode`; `tiling_sys/restart.rs:85,
    176,283` born-fullscreen hold, slotless maximized, born keep latch).
  - Membership classification is adapter-side on M and K
    (`classify_membership` in `crates/tiler-macos/src/classifier.rs:47`,
    `classify_retained_membership` in `crates/omnitiler/src/kde_raw.rs:35`),
    with the result reported as `floating`/`fit_excluded` facts.
  - W computes border eligibility locally (`tiling_sys/focus.rs:273`) although
    core publishes border presentation.
- Risk: the same decision exists in two places (adapter library call and
  core world policy), and settings changes cannot reclassify in core.
  Package B will classify each call as "pure helper" versus "decision that
  should move behind the contract".

Verification (package B, B2/B5): confirmed and extended.

- Adapter-side decisions beyond library calls:
  - W vetoes an accepted core reply. `deliver_production` clones the engine,
    runs the report, and if a tiled window's cross-output transfer comes back
    floating (capacity), returns an adapter-made `Rejected "float-no-remap"`
    and discards core's decision (`tiling_sys/world_sequencer.rs:844-947`).
    This is product policy (no-space behavior on cross-output send) living in
    one adapter, it relies on the `op-<seq>` id convention (`:931`, F4), and
    it adds a second engine clone on top of F6.
  - K apply-side focus guards: automatic focus is suppressed when a
    different focused float/sticky window exists, admitted only for ordinary
    admission floats under overlays, and fenced behind fullscreen/maximized
    siblings (`kwin/src/world-apply.ts:2294-2377`). Core emitted the effect;
    the adapter decides whether it happens.
  - K same-operation re-actuation veto after external departure
    (`world-apply.ts:834-859,1120-1153`).
  - M infers "maximized" for presentation from zoom/full-display geometry
    (`world_builder.rs:212-246`) while reporting fact `maximized: false`.
  - W birth preamble admits fullscreen/slotless maximized, adopts markers and
    clears first maximize before the report (`tiling_sys/world_tick.rs:
    1435-1467`); restart adoption (`tiling_sys/restart.rs:366-583`).
- W reads core state outside the reply: `world.engine().adapter_homing(...)`,
  `logical_view`, `logical_ring`, `logical_tiling`, `logical_members`
  (`tiling_sys/world_reads.rs:58-136`); M calls `retained_transfers()`
  (`world_runtime.rs:199-245`). In-process adapters treat `WorldEngine` as a
  queryable library, so the reply is not the full interface on W/M, and the
  KDE path cannot do the same.

### F11. File size and dual engine layers

- `engine.rs` 9,794 lines, `session.rs` 8,734, plus 15 other core files over
  1,500 (A9). `WorldEngine` holds `pub(crate) engine: Engine`
  (`world_engine.rs:425`), which holds `sessions: BTreeMap<DomainKey, Session>`
  (`engine.rs:62`). The session layer has its own adapter-contract
  vocabulary (`contract.rs` "Production transport-independent adapter
  contract", `boundary.rs`, `reconcile.rs`, `cosmic_v1.rs`), all `pub` from
  `crates/tiler-core/src/lib.rs` and still imported by
  `crates/tiler-protocol/src/world_protocol.rs` and macOS. `CoreCommand` comes
  from `boundary.rs`, so `Operation(CoreCommand)` (F2) exposes that inner
  vocabulary through the world contract. Whether the inner contract is
  still a live platform interface is for package B.
- Platform-side: `tiling_sys/world_tick.rs` 6,869, `tiling_sys/pager_host.rs`
  4,560, `tiling_sys/world_diff.rs` 4,503, macOS `ffi.rs` 12,210,
  `runtime_thread.rs` 6,105, `kwin/src/world-entry.ts` 5,161,
  `kwin/native-effect/shortcutreconciler.cpp` 4,761.
- Not a contract defect by itself; relevant because the tab design (package
  D) will land in the session tree layer.
- Package B (B5): no production platform path calls the inner `Session` or
  `reconcile` directly; the legacy planners are retired. But
  `boundary::CoreCommand` is live command vocabulary (M keyboard, W staged
  veto), and the protocol codec still decodes the full inner vocabulary
  including `reconcile`, `update-gaps`, `release-domain`
  (`crates/tiler-protocol/src/world_protocol.rs:1224-1387`) and carries
  `session::DragHoverPrior` (`:1049-1073`). W `world_owner.rs:2-12` still
  says the world path is "standby: unwired" while it is the production path:
  stale module header.

### F12. Engine profiles are per-platform behavior switches named after platforms

Source: `crates/tiler-core/src/world_engine.rs:739-862`; package B3.

| Option | W | K | M | Gated behavior |
|---|---|---|---|---|
| `fixed_send_admission` | true | false | false | automatic fixed-origin float carries on send (`session/world.rs:711-734`) |
| `maximized_focus_fence` | false | true | true | directional focus refused from maximized window (`session/ops/focus.rs:100-108,492-500`) |
| `with_focused_fullscreen_presentation_suspension()` | off | on | off | suppress border/underlay/pager on focused fullscreen output (`world_presentation.rs:779-790`) |
| `paused` ingress (F3) | limited-fullscreen path | false | false | hide border/underlay, empty pager |
| `cross_output_transfer` (command field) | true via dedicated commands | true | false (`world_command_map.rs:240-255`) | directional focus/move crosses outputs |

- Public docs name the platform: "Windows carrier enables it", "Windows
  carrier disables it" (`world_engine.rs:740-744`), "KWin only offers truthful
  fullScreen" (`:853`).
- Each is user-visible behavior that differs by platform with no setting or
  documented product reason in the contract. The brief says to implement and
  name features generically. Candidate: each becomes either a user setting in
  the settings catalog, a declared capability (if it is a host limitation),
  or one shared default. Which one is a product decision per row.

### F13. Platform vocabulary in core public surface

Source: package B6.

- Public declarations: `PresentationIngress.foreground_cloaked`,
  `SafetyFacts.cloaked`, `EligibilityRefusal::Cloaked`,
  `BorderHideReason::Cloaked` (DWM concept). Generic name candidate:
  `hidden_by_host` / `HostHidden`.
- Docs naming platforms are widespread in public items: `world_report.rs:499,
  540,614-615,696,757,761,948`, `window_policy.rs:41-310`, `visual.rs:186-261`,
  `settings_catalog.rs:181-182`, `world_directional.rs` ("KDE
  selectFloatFocusTarget caller parity"), `engine.rs`, `session.rs`,
  `session/world.rs`. Core docs describe behavior as "parity with platform X"
  instead of stating the rule. Nit-level individually; together they show
  core rules defined by reference to one platform's past behavior.

### F14. Two engine clones per W cross-output send, one per report elsewhere

- Combines F6 and the W veto (F10): W staged sends clone `WorldEngine` in the
  adapter (`world_sequencer.rs:848`) and again inside `handle_report`
  (`world_engine.rs:1391`). Quantify in package C before deciding.
- Package C could not quantify it: no existing harness splits clone cost.

### F15. Where the time goes: core policy, not the wire

Source: package C1-C3; `docs/changes/archive/core-full-state-replies.md:186-197`
(historical, pre-world-v3, release, fresh engines, 11 repetitions).

| Workload | Request / reply bytes | Decode / core / encode median |
|---|---|---|
| 800 windows, 64 domains | 226911 / 178690 | 2.45 / 94.35 / 0.33 ms |
| 2000 windows, 64 domains, held | 759891 / 634214 | 6.14 / 93.77 / 0.62 ms |

- Codec is about 3-6% of total at these scales; core is about 94 ms on a
  fresh engine's first report. Core time barely changes from 800 to 2000
  windows, which suggests it is dominated by per-domain or fixed work (64
  domains in both) rather than per-window work. Unverified: not attributed.
- Small scale today (3 windows, 2 outputs, 10 calls): release
  `handle=2214299 ns` total, roughly 0.22 ms per call including test
  scaffolding; debug roughly 1 ms per call.
- Fixture sizes: 7-window/2-screen report 5375 bytes, reply 4971 bytes;
  roughly 0.7-0.8 KB per window per direction.
- Implications for the user's goals:
  - "Ship whole world state each report" is not a bandwidth or codec problem
    at realistic window counts (tens). It stays cheap to well past 200 windows.
    The design question is semantic (state vs operation, F5), not IO.
  - The real latency risk is core compute on each report. The first-report
    800-window cost (94 ms) is above a frame budget. Steady-state cost after
    the first report is unknown. That gap matters more than F8.
  - Prefer improving core cost (clone, fixed per-domain work) and measuring
    steady state before moving any logic out of core for performance.
- Gap: no realistic-scale (10/50/200 windows, 1-4 outputs) steady-state
  matrix and no internal phase split. Closing it needs temporary
  instrumentation at `world_engine.rs` `handle_report` (`:1234`, `:1246`,
  `:1280`, `:1343-1348`, `:1391`, `:1395`, `:1398-1399`) plus a scale harness
  built from existing test report helpers (C2). Not done: it is a code change.
- Nitpick: no automated perf regression gate exists for `handle_report`; the
  only timing outputs are "illustrative, no thresholds".

### F16. Tabbed stacks: design sketch and what it asks of the contract now

Source: package D. Constraints from `docs/decisions.md:3982-3997` (tabs first
after 0.1, Rust engine, later protocol bump accepted, v4 visibility-ready,
close survivor provisionally positional-next) and `docs/backlog.md:1876-1890`
(user sketch: shorter stacked windows, strip drawn with underlay-like
machinery).

Reference models (D5): i3/sway make tabbed a layout of any container (tabs
can hold splits); Hyprland and COSMIC use a separate group/stack of windows
only. All four have the WM/compositor draw the strip. Inactive members are
hidden by the compositor (sway scene disable, Hyprland alpha 0 plus input
block, COSMIC renders only the active member) or merely raised under the
active one (i3, inspected path).

Sketch (reviewer proposal, not decided):

- Core model: add a leaf-only stack node,
  `Node::Stack { id, members: Vec<NodeId>, active: usize }`. COSMIC/Hyprland
  shape, matching the existing COSMIC-analogue tree. Leaf-only avoids
  tabs-of-splits (i3) and keeps projection simple: a stack is one tile for
  geometry, directional planning and active-group purposes.
- Geometry: one tile rect splits into `strip` and `content`; every member gets
  `allocation = content`. This breaks the projector's "non-overlapping leaf
  rects" promise (`geometry.rs:145-164`), so the invariant becomes "one
  allocation per tile; stack members share it".
- Visibility: inactive members get desired `suppressed`, composed in core with
  workspace suppression (already decided for v4). Platforms without
  per-window suppression need a generic fallback chosen by capability, for
  example a stacking-order desire (active raised, i3-style) or "stacks
  unsupported". KDE/macOS have no approved suppression route today (D3).
  Unverified option: a KWin effect can hide inactive members at paint time the
  way sway disables scenes; not investigated.
- Presentation: a `stacks[]` presentation row per visible stack:
  `{ stack, output, strip_rect, active, tabs: [{ window, rect }] }`. The
  renderer gets titles/icons natively by window id; core never handles titles.
  Same renderer machinery as the underlay (W overlay, KDE effect, M carrier).
- Input: renderer hit-tests core-supplied tab rects and sends a generic
  command `StackActivate { window }`. Other commands: `StackToggle` (tile <->
  stack), `StackJoin { direction }`, `StackLeave { direction }`,
  `StackCycle { next|prev }`, all with typed directions (F2).
- Drag: `DragPreview` gains a join-stack target kind (today centre-stack drops
  are refused fail-closed, R-GRP-01).
- Focus: activating a tab focuses its window; the platform must show before
  focusing. In a state contract this is one documented apply order
  (visibility, then frames, then focus), not a per-entry dependency roster.
- Open behavior rows (leave TBD in `docs/spec/reference-outcomes.md` until
  source/user evidence): directional focus inside a stack (COSMIC steps
  members left/right, i3/sway via tree orientation, Hyprland behind a
  setting), new window placement into a focused stack, minimize/float/
  fullscreen of a member, close survivor with a discriminating history
  (`S[A,B*,C]`, history C,A,B, close B: positional-next gives C, MRU gives A).

What the pre-0.1 contract should already get right so tabs are additive:

1. Per-window desired visibility as state on `WindowDesire`, composed in core
   from all reasons; workspace selection becomes one reason, not the
   mechanism. Platforms never learn why a window is hidden.
2. Per-window truthful shown-state fact on `WindowFact`, independent of
   minimized and workspace membership.
3. Multiple windows may share one allocation; remove any adapter assumption
   of disjoint allocations (unverified whether any adapter assumes it).
4. Decorations as a list of typed rows (border, underlay, pager, later stack
   strips) rather than one bespoke top-level field each, with one renderer
   revalidation rule.
5. Renderer input -> generic command path (`StackActivate` is the first
   case of "click on a core-drawn decoration").
6. Typed directions and command names (F2) so stack commands do not add more
   strings.

### F17. The v4 visibility design is the most operation-centric part, and it is next

Source: `docs/changes/windows-workspace-return-focus.md:2436-2484`
(design-only); F5.

- The proposed v4 delta adds `visibilityGroups` (frozen rosters, phases,
  dependencies, fallback), `visibilityWork` (core-minted work ids),
  `visibilityResults` (dispositions, setter outcome, pre/post samples,
  attemptMs), `selectionProofs` and `visibilitySettled`. That is five new
  lanes for one boolean per window, on top of the five attempt lanes in F5.
- It encodes Windows-shaped staging (arrive, foreground proof, then depart)
  as cross-platform contract structure, while KDE and macOS would advertise
  `windowSuppression:false` and not use it initially (D3).
- A state-centric alternative for the same goals: `WindowDesire.visibility`
  plus authority; `WindowFact.visibility { shown, suppression_owned }`; one
  documented apply order; core derives settlement and failed-hide accounting
  from facts plus at most one generic per-window last-write fact (F5). Focus
  grants keep their existing correlation. Whether this meets every
  anti-flicker/ordering requirement in the design table is unverified; the
  staging rules would move from wire structure into documented platform apply
  order plus core policy.
- Tension with a user decision: the 2026-10-29 decision includes
  "per-window suppression authority/correlation". Changing the correlation
  shape is a user decision.
- Package F addenda (reviewer, from F(c)-F(f) evidence):
  - F(c): the suspected "no pager miniature for hidden K/M windows" bug is
    not established. The real difference: only Windows fills the `float`/
    `tile` fallback frame slots, from adapter caches (`world_capture.rs:781-831`);
    K and M send live frames only. Core already holds every member's last
    allocation, so core should be the fallback and the two ingress slots
    can go (extends F3/R2; removes a parity gap).
  - F(d): no production `SwitchView` constructor exists on any platform;
    W and M still disagree on its selection-focus eligibility
    (`world_diff.rs:355-368` vs `world_dispatch.rs:1605-1620`), and core
    keeps a separate path (`world_commands.rs:119-188`). Dead vocabulary
    with divergent handling; supports deleting it under R6.
  - F(e): no adapter dependence on disjoint or unique allocations was
    found (search-limited). No adapter-side blocker for F16's shared stack
    allocation is known.
  - F(f): KDE's separate group-highlight route is gone; group underlay
    renders through uniform presentation (`world-entry.ts:1466-1514`). The
    `decisions.md:3558-3562` entry is stale (G9), and the native class name
    `GroupHighlightObject` is stale naming (R9).
- Nitpick: the design section still cites `WORLD_CONTRACT_VERSION = 2` and a
  tentative v3 (`:2459-2476`); source is v3, so its version anchors are stale.

### F18. macOS loses taken attempts on core rejection and skips the owed catch-up

- Evidence F(a) (D; reviewer re-read `world_runtime.rs:327-349` and
  `world_dispatch.rs:742-784`). macOS drains pending view, member, creation
  and frame attempts with `std::mem::take` into the report. Core rejection
  commits nothing (`world_engine.rs:1391-1407`), and the macOS rejected arm
  clears the flight and returns `Refused` without reinserting the rows or
  sending an observation-only report. The same holds for gesture rejection
  (`world_dispatch.rs:487-499`) and direct workspace selection
  (`workspace_select.rs:1791-1829`).
- Effect: attempts the adapter already actuated never reach core, so core
  cannot attribute the resulting host changes to them; they will look like
  external changes on the next report (runtime effect not reproduced). It
  also breaks the user-approved rule that rejection services
  observation-only catch-up on the same turn (`decisions.md:1935-1937`).
  Windows (`world_tick.rs:2377-2410`) and KDE (`world_broker.rs:977-998`)
  conform; macOS is the outlier. Resolves the open item in G6.
- Root cause is contract-level (F7): a command rejection discards the
  observation, so every adapter must carry its own recovery path and one
  forgot. R3 (command outcome as a reply field, observation always
  incorporated) removes the class; a shared conformance fixture (P8) would
  have caught this instance. Until then it is a macOS bug.

### F19. KDE's per-capture `hostOwned` flips core between two receipt models

- Evidence F(b) (D; reviewer re-read `world-observe.ts:1328-1340,1443-1475`,
  `world_native.rs:85-129`, `world_engine.rs:1601-1634`). KDE computes
  `hostOwned = desktopsReadable && every screen has a current id` per capture.
  A transient readability gap or one screen with an unknown current desktop
  sets it false for that report.
- When true, core derives membership only from native evidence, holds
  unknowns, and fences writes for unreadable scopes. When false, KDE sends
  adapter-derived logical view receipts and no native rows, and core takes
  the tiler-owned route (`effective_logical_receipt` returns the adapter's
  `observed_workspace`), where that fence does not apply. Degraded
  readability therefore switches core to the route with fewer protections at
  the moment protection matters; no transition handling or reset exists.
  Whether this causes visible misplacement was not reproduced.
- This is the F1 defect (a host capability sent as a per-report flag) with a
  concrete failure path. R4 fixes it: the capability is fixed per incarnation
  in the handshake, and readability gaps are reported as incompleteness.

### F20. Steady-state cost is a full reconciliation sweep that grows faster than window and workspace counts

- Evidence package G (offline synthetic reports, release build, relative
  numbers; reviewer read the section's tables). S1 is an unchanged,
  already converged report; median total `handle_report`:

| Windows | 1 output | 2 outputs | 4 outputs |
|---|---:|---:|---:|
| 10 | 0.31 ms | 0.33 ms | 0.42 ms |
| 50 | 2.94 ms | 2.00 ms | 2.09 ms |
| 200 | 27.2 ms | 18.1 ms | 12.7 ms |

- A report in which nothing changed costs the same as one with an external
  move (S2) or a command (S3). There is no unchanged-input fast path.
- `run_accepted` is 82-97% of the total; within it, the readable-domain
  reconciliation sweep is about 48-89%. At 200 windows the parts earlier
  recommendations targeted are small: staging clone about 0.27 ms, discarded
  canonical encode about 0.11 ms, commit plus reply cache about 0.22 ms,
  request decode about 0.75 ms (outside core).
- Growth is superlinear: one output, 10 -> 50 windows is 9.4x and 50 -> 200
  is 9.3x for 4-5x more windows. With 50 windows on 2 outputs, going from 2 to
  20 workspaces per output (2 -> 38 retained sessions) raises S1 from 2.0 ms
  to 10.6 ms. The cause inside the sweep is not attributed (no profile).
- First reports on a fresh engine are larger (200 windows: 138 ms on one
  output, 35 ms on four), consistent with the historical 94 ms (F15).
- Why it matters: every host event that produces a report pays this cost, and
  it adds directly to command latency. Interactive drags and resizes produce
  report streams, so 50 windows across many occupied workspaces already
  exceeds a 60 Hz frame budget (about 16.7 ms) at a few reports per frame and
  exceeds 144 Hz (about 6.9 ms) on its own. This is a jank source the
  contract shape does not cause and moving logic to platforms would not fix.
- Implications:
  - Confirms F15: whole-state shipping is not the cost. The cost is core
    re-deriving the whole world on every report.
  - The fix belongs inside core: find the superlinear term in the sweep, and
    skip or scope work for domains whose inputs did not change. The contract
    stays whole-state; incremental work is a core implementation detail.
  - R7 order changes: profile and fix the reconciliation sweep first, then add
    a per-report cost regression gate; clone, encode and KDE re-parse cuts are
    secondary.
- Caveats: synthetic, very large output rectangles; fixed-order blocks
  without randomization; probe overhead included; S3 measured a workspace
  tiling toggle, not directional focus; the 20-workspace first report was not
  on a fresh engine. Window redistribution across workspaces confounds pure
  per-domain attribution.

### F21. One untyped integer carries three units, and core sizes follow the host's unit

Source: package H.

- The contract promises one coherent adapter space per report but no unit or
  scale. Windows reports physical pixels (Per-Monitor-V2), KDE KWin logical
  pixels, macOS points; all travel as untyped `i32` (`geometry.rs:18-26`,
  `world_report.rs:664-681`).
- Core sizing constants are applied in whatever unit arrives: group drop edge
  depths 32/80, resize pair minima 720x480, child minima 360x240, keyboard
  steps 12..20 (`cosmic_v1.rs:202-245,332-340`). Their doc comments say
  "physical pixels", but the COSMIC reference uses logical coordinates
  (reviewer check at the cited pin: `update_pointer_position` takes
  `Point<f64, Local>`, and `Global`/`Local` are logical-space markers,
  `src/utils/geometry.rs:14-20,51-52`). So KDE and macOS match the reference
  and Windows at 200% scaling gets half-size drop zones, minima and steps.
- Gaps: the contract has per-screen gap fields, but every adapter repeats one
  global value. Windows sends unscaled physical gaps while scaling its border
  style by foreground-window DPI, so on Windows the tile gap and the border
  gap scale differently; KDE and macOS keep logical/point sizes.
- Quantization: KDE ordinary facts use `Math.round`, native gestures
  `std::lround` (differ on negative half-ties); macOS rounds screens but
  truncates AX window frames and drag start frames. Core arrival is exact
  integer equality. No non-converging frame was reproduced.
- Implications: a per-output scale (adapter units per logical pixel) on the
  screen fact would let core own all user-facing sizes in logical units and
  apply them per output, removing Windows border DPI scaling from the adapter
  and the per-platform size divergence. Quantization should be one rule per
  adapter. Exact arrival stays until a fractional-scaling failure is
  reproduced (guide rule).

### F22. Identifiers: one string type for logical and native ids, control data in id text, undefined output identity

Source: package I.

- Logical and native workspace ids are both plain `String` fields
  (`world_report.rs:324-362,683-779`); only field names separate them. macOS
  wraps native `space-<sid>` keys in core `WorkspaceId` and has a fallback
  that puts native text in a logical comparison slot
  (`world_dispatch.rs:1245-1269`). Works by namespace disjointness, not type.
- Core parses control data out of id text: clear continuation strips `op-`
  and parses the command sequence, defaulting to 0 on parse failure
  (`world_clear.rs:547-596`). KDE extracts the output from core's composed
  `output/workspace` scope string (`world-native.ts:555-584`). Same class as
  F4.
- Window ids: core cannot detect a reused id; lifetime discrimination is an
  unwritten adapter obligation. Windows tags lifetime explicitly, KDE uses
  KWin's constructor UUID, macOS `"<pid>_<wid>"` relies on CGWindowID not
  being reused within a live process (unverified).
- Output ids: the product rule says displaced workspaces "return
  automatically to their original monitor" (Workspaces, G-02), but the
  contract does not define what makes an output the same one. Windows keys on
  the GDI device name, KDE on the connector name, macOS on the CG display id.
  A different monitor on the same port or the same monitor on another port
  therefore gets different return behavior per platform. KWin's output object
  also declares `manufacturer`, `model` and `serialNumber` properties
  (reviewer check, `src/core/output.h:246-272` at pin `8438567a`; script
  exposure not checked). Windows display-identity availability not checked.
- Node and operation ids are generation-scoped and restart from zero in a new
  engine; envelope freshness disambiguates them (no defect found).

### F23. Restart identity is minted three ways, and crash safety of native side effects is Windows-only

Source: package J.

- Envelope identity differs per platform: Windows static owner plus
  process-creation generation; KDE broker-minted owner, incarnation and
  generation; macOS fixed owner and generation (`phase-1`) plus pid
  incarnation. Core treats a different incarnation as a fresh namespace with
  no ordering, so a delayed old-incarnation report could reset state (not
  demonstrated; macOS relies on reports never crossing processes).
- Core has an incarnation-change path that keeps Engine sessions, but in
  production every adapter lifetime change gets a fresh Engine: KDE replaces
  the Engine on a new producer even when the broker survives; Windows and
  macOS lose both sides with the process; the Windows reset method has no
  caller. The retained-session path looks unused (production callers not
  exhaustively checked).
- Crash safety: Windows records hides in a write-ahead ledger with an
  independent watcher and startup reclamation. Fullscreen chrome stripping is
  stored in window properties but not restored by the watcher, so a crash
  during fullscreen leaves the window chromeless until a later owner exits
  it. KDE keep-above ownership and pre-sticky stacking live in adapter memory
  and are lost on script reload (parked; restart storage excludes them).
  macOS hides nothing in production and its watchdog exits leaving windows in
  place. No contract rule says what an adapter must do before a native write
  the user cannot easily undo; R1 will add per-window suppression on more
  platforms.
- Restart classification (float/tile/sticky hydration) is adapter logic on
  every platform today; approved core-owned intent (R2) replaces it.

### F24. Settings validation and rejection kinds are per-adapter and stringly typed; one bad row rejects a whole report

Source: package K.

- Settings: core has a strict catalog validator, but adapters apply their
  own rules for the same field. Windows rejects a whole document (startup
  defaults, live keeps last-good); KDE defaults each field silently; macOS
  refuses to start on one invalid field or JSON error (against Resilience:
  invalid settings are not a hard failure). The wire decoder is strict for
  some settings and backfills defaults for three others. KDE's targeting
  defaults differ from the catalog because KDE/macOS adoption of the
  2026-10-20 cursor-first targeting decision is still pending (known gap).
- Rejection: report, command and divergence kinds are `&'static str` tokens
  (about a hundred across validation, engine, commands and protocol);
  lifecycle and directional causes are open `String`s. KDE branches on kind
  strings; macOS matches an adapter `"stale-observation"` string. Typed outer
  enums exist (`HoldReason`, `AttemptOutcome`, `DirectionalOutcome`) but carry
  string causes.
- Narrowest scope: one invalid screen, window, attempt, creation or
  presentation row (including one style value outside 0..1024) returns
  `snapshot-invalid` for the whole report, against the Resilience rule.
  Advisory size hints already sanitize to absent instead.
- User feedback for invalid settings and refused commands differs per
  platform (modal dialog, silent default, exit with stderr).

### F25. Window-state toggles mostly bypass core, and the fullscreen fact cannot say who can exit it

Source: package L.

- Only KDE maximize goes through core (`ToggleMaximize`). Windows maximize,
  fullscreen and sticky, and KDE fullscreen and sticky, are adapter-direct
  native writes reported afterwards; Windows sticky still calls core float
  and send operations underneath. macOS ships none of the four actions. No
  product minimize action exists anywhere.
- Shared policy is duplicated: core `window_policy.rs` already orders sticky
  refusal (fullscreen, then maximize), while KDE and Windows re-check it
  locally; sticky-off restore differs (KDE restores prior float, Windows
  chooses preserve or rehome).
- Fact truthfulness: Windows fullscreen is inferred (captionless plus
  covering) and some rows carry retained values; KDE folds hidden/ineligible
  into `minimized`; macOS never sets `maximized` or `sticky` while its
  presentation infers maximize from geometry. (R1 shown-state and R5
  zoom-inference work already cover the KDE and macOS parts.)
- Fullscreen mechanism differs (Windows tiler-owned borderless simulation;
  KDE/macOS native), and the fact is one boolean: core cannot tell whether a
  fullscreen is tiler-owned or app-owned, so a neutral exit (the Alt+Enter
  question deferred 2026-10-28 to this review) cannot be routed safely.
  Windows refuses to exit app-owned fullscreen today.

### F26. Contract events are logged under per-adapter names, and two adapters log every report at normal level

Source: package M.

- Vocabulary: core already returns the deciding tokens (rejection kind,
  `HoldReason`, settings `Validity`), but each adapter logs the same event
  under its own name and field set. Report rejected: Windows `world-apply`
  `delivery=rejected` with kind and message; macOS `core-reply` with kind only;
  KDE `omnitiler:world:rejected` with kind, message and recovery text. Invalid
  settings: Windows logs `settings-degraded`, macOS exits with a reason, KDE
  logs nothing. The shared `omnitiler:route-diag` schema in the Observability
  decision shows the intended direction but does not cover contract events.
- Volume: the Observability decision says normal operation shows bounded
  summaries and trace carries high-volume detail with no frame/poll noise.
  Windows conforms (accepted-apply lines are trace-gated, outliers and
  one-second summaries otherwise). KDE Rust emits two unthrottled lines per
  `DescribeWorld` call to the journal, and builds them by parsing the request
  and reply JSON again (on top of F8). macOS logs `observation/applied` per
  accepted observation to its default stderr sink at normal level.
- Mechanism: four logging stacks (Windows JSON-line file append, macOS async
  structured logger, KDE Rust `eprintln!`, KWin `console.log` plus Qt
  categories) and four level controls (Windows `--trace` flag, macOS
  `--log normal|verbose|trace`, KWin compile-time trace build, none for KDE
  Rust). The decision's `just dev verbose`/`just dev trace` vocabulary matches
  only macOS.
- Privacy: the principle and redaction rule exist and the inspected helpers
  mostly follow them (no titles found in background logs; package M did not
  audit every site). The rule is ambiguous about application identity and
  process ids: Windows logs window class and executable basename at normal
  level by design, KWin logs configured scope classes at startup, macOS
  normal events carry PIDs, while the rule redacts "native identifiers, raw
  native IDs" without saying whether PIDs or app identity count. Explicit
  inspection commands (Windows `children` with full path and SID, macOS
  `preflight` with bundle id) are user-invoked output, not logs.
- Supportability gaps (unverified): where macOS stderr goes when the tiler
  runs as a bundled login item; whether the Windows per-run log has any size
  bound over a long session (the helper has none); no cross-platform state
  dump exists for bug reports.
- Nits: the macOS dispatch comment says non-SEND refusals stay ring-only, but
  they also reach the stderr sink; the Windows `window-diag` emitter described
  in `diag.rs` was not located; the redaction rule lists "native
  identifiers" and "raw native IDs" as two items.

### F27. GNOME fits the contract with the KDE topology; ChromeOS has no window-management route

Source: package N, plus one reviewer check of the pop-shell clone
(`../shell` is pop-shell, not gnome-shell, HEAD `7898b65c`): its
`metadata.json` declares no `session-modes`, and `src/extension.ts:2723-2729`
`disable()` returns early with `was_locked = true` when
`sessionMode.isLocked`, keeping its state. I: GNOME calls extension
`disable()` on screen lock for extensions without an unlock-dialog mode, and
tiling extensions work around it.

- Topology: a GNOME adapter would be a GJS extension inside the compositor
  plus the Rust core as a session D-Bus service, the same split as KDE. The
  references already use session D-Bus and `Gio.Subprocess` from GJS. The
  contract needs no GNOME-specific shape for transport, input (keybindings,
  grab-op signals), decorations (St/Clutter actors) or native writes
  (`move_resize_frame`, `change_workspace`, `minimize`, `stick`,
  `make_above`, `activate`, native fullscreen).
- Lifetime: screen lock disables extensions, so without the pop-shell
  pattern (or an unlock-dialog session mode) every lock ends a GNOME adapter
  lifetime. Under R12 each lifetime starts fresh core state, so a GNOME
  adapter must keep one lifetime across a locked disable. The R2 host-slot
  assumption holds: Tiling Shell attaches its own fields to `Meta.Window`,
  which outlive an extension disable while the window lives.
- Workspaces: native workspaces are global across outputs (like KDE) and,
  by default, dynamic: the host removes empty workspaces and reindexes the
  rest. No host workspace id appears in the evidence; PaperWM mints its own
  UUIDs per workspace. R11's `NativeWorkspaceId` must therefore allow
  adapter-minted ids that stay stable across host reindexing within a
  lifetime. Host-driven removal maps to the existing native workspace
  retirement path (world v3), unverified for GNOME. Open: how the contract
  represents `workspaces-only-on-primary` (secondary outputs do not switch);
  PaperWM simply forces that setting off.
- Hiding: no cloak; references use actor hide with clones, native minimize,
  or workspace moves. The R1 shown-state fact is mechanism-neutral and fits.
- Settlement: PaperWM corrects positions on `position-changed`, defers via
  `later_add` and retries target-height checks; Mutter settles asynchronously
  like KWin. Fact-based settlement (R1) fits; per-attempt settlement (F5)
  would not add anything here.
- Units: Tiling Shell multiplies gap settings by the monitor or theme scale
  itself. That is the Windows pattern, and it supports R10 option (a), a
  per-output scale on screen facts, over every adapter scaling sizes on its
  own. GNOME frame units under fractional scaling are unverified.
- Output identity: PaperWM restores space-to-monitor assignment by connector
  via DisplayConfig; whether vendor/product/serial are usable was not
  established. Feeds the queued R11 output-identity question.
- Unverified: whether extensions.gnome.org permits an extension that depends
  on a separately installed native service; Mutter window id reuse;
  coordinate units; lock behavior beyond the pop-shell guard.
- ChromeOS (no source, desk reasoning only, unverified): no public API lets a
  third-party program place or hide arbitrary ChromeOS windows; Chrome
  extension window APIs cover browser windows, and Linux apps run in a
  container whose windows the host manages. The blocker is host access, not
  the contract.

### F28. Behavior is tested per adapter, the shared wire corpus can go stale, and the TS contract mirror is hand-written

Source: package O.

- Wire corpus: the 12 `test-fixtures/world-wire/` files are read only by
  seven KWin test files; the six replies are written by an omnitiler example
  that nothing runs automatically. If Rust reply encoding changes and nobody
  reruns the example, KWin tests keep passing against old bytes. The Rust
  codec tests compare protocol and core encodings with each other, not with
  these files. Windows and macOS share the Rust types, so the corpus only
  needs to guard the TypeScript side; that scope is right.
- Type drift: TS contract unions, parser and encoder are hand-written mirrors
  of the Rust types. Protection is fixture bytes, strict TS parsing and the
  `raw_world_eval` bridge, which cover exercised variants only. A Rust variant
  no fixture exercises would first fail at runtime as a rejected reply. v4
  changes most of the wire (R1, R3, R4, R8, R13), so the whole mirror will be
  rewritten anyway.
- Behavior conformance: each adapter has its own fake host (Windows
  `FakeNative`/`FreshHost`, macOS `FakeBackend` plus builder and dispatch
  tests, KDE TS `makeHarness` plus the Rust bridge). No scenario is written
  once and run on every adapter, which is acceptable only if behavior lives in
  core; F9, F10 and F25 show it does not yet. Core has about 160 integration
  tests at the `WorldEngine::handle_report` boundary, some of which also
  inspect engine internals.
- Reference outcomes are not executable: no test index or runner maps
  `docs/spec/reference-outcomes.md` rows to tests; some tests cite rows ad
  hoc, including internal Session tests.
- CI runs Windows and macOS crate tests and Ubuntu KWin/Rust/native suites;
  no live legs (expected).

### F29. External control has three shapes, and only KDE speaks the core command vocabulary

Source: package P.

- Vocabulary: KDE exposes core `WorldCommand` JSON directly through the
  Planner `QueueWorldCommand` D-Bus method (kebab-case wire names) and has no
  action CLI. Windows has its own CLI verbs delivered as command files
  (`directional --focus`, `workspace --select`, `workspace --float`,
  `send-output`), several of which reach adapter-direct paths (F25). macOS
  has only probe and native-operation verbs and no channel to a running tiler.
  A user script cannot carry across platforms, and the Windows verbs would be
  public surface to keep stable after 0.1.
- Trust: the recorded KDE model trusts same-user processes; the Planner
  checks UID and KWin ownership and returns a fixed unauthorized reply. The
  other endpoints stay inside that same-user boundary, but checks are uneven:
  KDE effect `SetPresentation`/`ClearPresentation`/`HidePresentation` and
  `LastGesture` have no sender check while their sibling methods require the
  KWin owner; tray menu events have none; Windows command-file receivers check
  owner fields in the body, not the writer. No endpoint outside the same user
  was found (OS ACLs and bus policy unverified).
- Input bounds: KDE world and effect paths cap sizes and parse strictly.
  Windows command files are read with an unbounded `read_to_string` and the
  workspace request struct accepts unknown fields, while Windows settings
  reads are capped at 64 KiB.
- Read exposure: presentation and gesture reads carry window ids, geometry
  and workspace labels, no titles or app names in the inspected encoders.

### F30. Native workspace backing: the correlation is mostly justified, the field shape is doubled, native acceptance is open

Source: package Q. Five tentative entries (native-only receipts, hybrid
creation correlation option C, native attempt preimages option 1,
released-source backing option 1, trailing count clarification) were
deferred to this review; occupancy and retirement were user-selected
2026-10-29.

- Against the rewritten principle (converge where re-applying is safe;
  correlate only effects that are not): creation is not idempotent (a retry
  makes a duplicate workspace), so its correlation and the
  `AmbiguousCreation` hold are justified. Singleton inference is uniqueness,
  not timing or geometry, and the entry says so; acceptable as written.
  View and member switches are contested by the user (the user's switch back
  must win over a late tiler arrival), so re-applying is not safe and
  pre/post/baseline evidence is justified. Retirement settles from facts
  (eligibility, then absence readback) with no operation ids. Native-only
  receipts match R2 and R11 (core binds; adapters never mint logical ids).
  Released backing is state logic, not correlation.
- Shape: `MemberAttempt` and `ViewAttempt` carry both logical and native
  pre/post(/baseline) field sets; Windows ignores the native set and
  host-owned adapters ignore the logical set. One set typed as a workspace
  reference (logical or native, chosen by the host-owned capability) would
  halve these fields with no semantic change.
- Divergence: KDE consumes retirement desires; no macOS native-removal
  consumer was found, and Windows has no native lane. Whether core gates
  `existence=false` desires on a capability, or a macOS row waits on absence
  proof indefinitely, was not established.
- Acceptance: every native outcome for adoption, creation timing, released
  backing and retirement is still TBD in `reference-outcomes.md`; the logic
  is covered offline by about 120 core tests.
- Trailing "count" is the ordinary lifecycle (a final empty spare, minimum
  two), not a setting; host-owned adapters back the spare only when used.

### F31. Border presentation during motion is decided per adapter; a neutral settling marker has no host signal to carry

Source: package R.

- Border during motion: core replies with absolute `inner`/`outer` bounds
  computed from the last reported frame (`visual.rs:197-207`), and
  `BorderHideReason` has no motion reason (`visual.rs:141-151`). Each adapter
  then decides motion behavior itself: Windows (`focus.rs:300-317`) and macOS
  (`border_runtime.rs`, motion replay inferred) repaint the last absolute reply
  while the window moves, so the border trails by at least one report round
  trip; the KDE effect hides on every frame change until a fresh covering
  reply arrives (`activewindowborder.cpp:1121-1135`). One user situation, three
  outcomes, chosen in adapter code rather than core policy.
- Every host already delivers the live frame without polling: KWin paint-time
  `EffectWindow::frameGeometry()`, the Windows `EVENT_OBJECT_LOCATIONCHANGE`
  hook, macOS `AXMoved`/`AXResized` notifications. The offline comparison
  (`moving_border_comparison.rs`) shows a core-derived relative descriptor
  applied to the live frame tracks with zero error under the same causal
  freshness gate that absolute replies use; the group underlay stays
  group-defined and absolute.
- An anchored descriptor also removes a round trip per motion step: correct
  placement no longer waits on report, core pass and reply; reports still
  carry eligibility changes. The same shape would serve a future tab strip
  attached to a group frame.
- Precondition: decisions.md:287-298 and the backlog wait for macOS drag
  evidence. macOS passive titlebar drag is parked and no native drag journey
  is recorded, so the precondition is not met.
- Settling marker: no pending-configure misread has been reproduced on any
  platform (guide:578-583; reference-outcomes:6430). No host exposes geometry
  completion to the adapter: KWin keeps its configure queue and acknowledged
  serial internal (`xdgshellwindow.cpp:114-198` at the pin) and
  `moveResizeGeometry()` is not scriptable; macOS AX has no completion signal;
  the adapters' `pending*` structures are attempt staging, not completion.
  Core already holds pending and ambiguous attempts and prefers staged facts
  (`world_settle.rs:1770-1889`, `world_frames.rs:127-239`). A neutral marker
  could only be fed by timing, which Ordering and Races forbids
  (principles.md:156-161). The real open problem is accepting small host
  discrepancies, which is the separate backlog item "Shared: tolerate
  host-settled geometry".

### F32. Rust in the KDE native effect is viable; the hurdles are known and mostly already paid

Source: package S. Reviewer assessment 2026-10-31; nothing built or run.

- Precedent: the effect linked a Rust staticlib through a POD C ABI with
  `catch_unwind` on all 11 exports, CMake-driven offline Cargo with vendored
  crates and Nix builds against two KWin versions (AR10). Its removal
  (`92010762`, `21af12a2`) followed the logic it held moving or being retired,
  not a Rust build, packaging or runtime failure.
- Packaging cost is mostly paid: the Arch, RPM and Debian recipes already
  build `omnitiler` with Rust and locked offline vendored crates, and still
  pass the vendor directory to the effect CMake. New cost: Rust inputs for the
  Nix effect derivation (and the settings derivation if settings logic moves),
  effect build time (unmeasured), a notices root for the staticlib. KWin ABI
  rebuilds already apply to the C++ effect; the staticlib is not KWin-bound.
- What can move (package S, A): host-free logic of about 1.7k-2.2k lines
  (upper figure counts the mixed Stage 5 files whole) plus
  about 1.3k lines of tests in the effect (presentation consumer/router,
  gesture lifecycle, active-border logic, Stage 5 records), and the largest
  C++ unit, the shortcut reconciler (about 6k lines plus 6.5k test lines),
  which is mixed logic and KConfig/D-Bus I/O in the settings modules, not
  the effect. The presentation consumer re-parses payloads that
  `tiler-protocol` already types; a Rust consumer can share those types,
  removing one cross-language contract copy (joins R17).
- Must stay C++: the `Effect` subclass, paint hooks and plugin factory
  (KWin's C++ `EffectPluginFactory`, no binary-compatibility promise), Qt
  D-Bus endpoints, KCM widgets. Pure-Rust factories (cxx-qt) are unproven.
- Hurdles, each with a known mitigation:
  1. FFI boundary needs `unsafe` (raw slices, out-pointers): one minimal
     module under the allowlist, or `cxx` (1.0.205) containing it behind safe
     APIs; decide by a spike.
  2. Panics inside KWin: every export catches unwinding and fails closed;
     `panic = "abort"` is not acceptable in a compositor plugin.
  3. Compositor-thread latency: calls are synchronous on KWin's compositor
     thread; keep them bounded and allocation-light, no blocking or locks
     shared with other threads.
  4. Symbols: one combined staticlib per plugin and a linker version script
     exporting only the Qt plugin entry; co-resident Rust plugin collisions
     unverified.
  5. Build glue: reuse the proven custom Cargo glue (Corrosion needs CMake
     3.22 against the 3.19 floor).
  6. Toolchain: no declared MSRV; the lowest recorded distro rustc is 1.93.1
     (Ubuntu 26.04). Declare `rust-version` for the workspace; this already
     binds `omnitiler`.
  7. Licensing: dependencies must avoid Apache-2.0-only terms (GPLv2-only
     combination); MIT or dual MIT/Apache crates are fine; the notices
     script needs the staticlib root.
  8. Migration risk: two implementations during a move; move one module per
     change, delete the C++ in the same change, keep thin C++ boundary tests.
- Sequencing fit: the anchored border (R20) rewrites the effect's border
  consumer for v4; writing that consumer in Rust avoids adding new C++ that
  would later move. Policy found in the effect belongs in core (route A);
  in-effect Rust is for in-process mechanical state only.

## Recommendations (candidates, ordered by leverage; none decided)

| # | Recommendation | Findings | Decision owner |
|---|---|---|---|
| R1 | Visibility as per-window desired state plus truthful shown-state fact; settle from facts; keep correlation only for focus grants. Decide before v4 is built. User 2026-10-30: reopen the v4 visibility shape on this basis; final shape not yet decided. After G1 was surfaced the user confirmed Model A is open to change on merit; user 2026-10-30 approved replacing Model A with R1 (recorded in `docs/decisions.md`; proposal in the visibility design record). | F5, F16, F17 | User (touches 2026-10-29 decision) |
| R2 | Core is the only owner of intent: core persists float/tile and other intent across restarts; facts carry only natively observable state, `Option` for unknown; delete adapter echoes and retained-frame read-back (core keeps last known frame). User 2026-10-30 approved, persistence variant (a2): core owns the intent record, platforms keep it in a host storage slot; recorded in `docs/decisions.md` (Contract, "Intent ownership and persistence"). | F3, F9 | User (data semantics) |
| R3 | Command outcome becomes a reply field; the observation is always incorporated. One recovery behavior instead of three. User 2026-10-30 approved option (a): ships in v4; macOS F18 fixed by its v4 cutover, not separately; recorded in `docs/decisions.md` (Contract, "Command outcome in the reply"). | F7, F18 | User (contract) |
| R4 | Split handshake (version, capabilities fixed per incarnation), revisioned settings, and observation report. KDE `hostOwned` readability gaps become incompleteness. `ScreenReport.primary` gets one meaning. User 2026-10-30 approved 4a option (a): handshake, revisioned settings message and report, in v4; F19 already covered by the 2026-10-28 KDE ownership decision (a). 4b option (a): one optional host-designated primary output id; recorded in `docs/decisions.md` (Contract, "Primary output"). | F1, F19 | User (contract) |
| R5 | Move adapter-side decisions behind the contract: W `float-no-remap` veto, K `af-` focus guards and re-actuation veto, M zoom-as-maximized inference, W/M direct engine queries become reply fields. Engine profiles become named settings or capabilities, product decision per row. 2026-10-30: rows covered by approved Contract entries go to the backlog with no behavior change; user approved unconditional focused-fullscreen presentation suspension and option (a) for KDE automatic-focus guards as core rules ("Automatic focus guards"); option (a) for `fixed_send_admission` (sends allowed everywhere, option removed). `maximized_focus_fence`: no product decision needed (G-06 already normative on all platforms); Windows reports maximize and the core fence replaces its adapter gate. R5 closed. | F10, F12, F14 | User per behavior row |
| R6 | Typed vocabulary: `Direction` enum, typed effect cause, one command per user intent (merge `SwitchView`/`SelectWorkspace`, route M keyboard through directional commands, drop `Operation(CoreCommand)` from the world contract), generic names (`HostHidden`), rules stated instead of "platform X parity". 2026-10-30: naming/typing items and `SwitchView` deletion (no production caller) decided by reviewer, backlog "Typed contract vocabulary"; user approved option (a), macOS keyboard via shared directional commands (supersedes D5 scope note); option (a), one action-context send source fallback ("Send source fallback"). R6 closed. | F2, F4, F11, F13 | reviewer for naming; user for command merges |
| R7 | Performance: steady-state matrix measured (F20). Next: profile the reconciliation sweep and remove its superlinear term; skip or scope work for domains with unchanged inputs; add a per-report core cost regression gate. Secondary: drop the discarded canonical encode (exact size gate to the KDE transport), shrink the staging clone, typed KDE broker path. | F6, F8, F14, F15, F20 | User (performance budget); reviewer for method |
| R8 | Decorations as one typed list with one revalidation rule; generic renderer-input-to-command path (first user: tab activation). 2026-10-30: pager clicks already raise commands, with divergent adapter staleness checks (macOS `verify_core_row_click`, Windows `verify_output_scoped_select_keys`). User approved R8b: decoration-raised commands carry exact target id and rendered presentation revision, core refuses stale ones. User approved R8a option (a): named typed decoration fields, one presentation revision plus foreground token and one staleness rule; no generic list (would be a speculative tab reservation). Recorded as Contract entry "Decorations". R8 closed. | F3, F16 | User (contract) |
| R9 | Hygiene: stale docs (`ToggleMaximize`, `Operation` refusal doc, W `world_owner.rs` header, v4 version anchors, missing estimator test), dead `Focus.direction`, adapter-constant fields. | F2, F6, F11, F17 | reviewer |
| R10 | Sizes in logical units everywhere: screen facts carry a per-output scale (adapter units per logical pixel); settings and core constants are logical; core scales gaps, border/underlay style, drop zones, resize minima and steps per output. reviewer 2026-10-30 (no product behavior change): one fractional-to-integer rule per adapter (KDE native gestures match `Math.round`; macOS AX frames round like topology); per-screen gap fields leave the report with the R4 settings split; exact arrival unchanged until a fractional-scaling failure is reproduced; macOS "exact" readback log wording to R9. User 2026-10-31 approved option (a), on condition that DPI, scale and resolution changes are handled live: scale rides every screen report, core keeps none between reports; all three adapters already re-read screens on those changes (KDE scale-only signal inferred from KWin source, to confirm); recorded as Contract entry "Size units and scaling". R10 closed. | F21 | User (public behavior on Windows) |
| R11 | Identifier contract. reviewer 2026-10-31 (no behavior change, joins R6 typed vocabulary): distinct `NativeWorkspaceId` type, macOS stops wrapping Space keys in `WorkspaceId`; typed command sequence and authority scope fields replace `op-` suffix parsing and `output/workspace` text parsing; the adapter guide states window ids are never reused for another native window within an incarnation (pre-build check: macOS CGWindowID reuse within a live process). User 2026-10-31 approved option (a): physical display identity (manufacturer, model, serial) reported as facts, core derives and matches it, connector decides when unreadable or shared; recorded as Contract entry "Output identity". R11 closed. | F22 | User (output identity); reviewer for typing |
| R12 | Restart and crash safety. reviewer 2026-10-31 (R4 follow-through, no behavior change): the v4 handshake defines one adapter lifetime; core mints the generation there and reports carry connection-scoped sequences, replacing per-adapter owner/generation/incarnation minting; each new adapter lifetime starts fresh core state and adopts from facts plus the core intent record (current production behavior); remove the retained-session incarnation path after confirming no production caller. User 2026-10-31 approved option (a): hard-to-undo native writes record the original state in a host slot first, the next start restores leftovers, the existing Windows watcher also restores stripped chrome, KDE keep-above and sticky preimages move to the runtime file, and per-window suppression is gated on the rule; recorded as Contract entry "Native writes the user cannot easily undo". R12 closed. | F23 | User (crash-safety rule); reviewer for envelope |
| R13 | Settings and rejection. reviewer 2026-10-31, applying the approved Resilience rule and R3/R4: core validates every field of the revisioned settings message with the catalog validator; an invalid field keeps its last-good value (default when none) and core returns a typed per-field settings diagnostic; adapters pass raw values and never clamp, default or refuse to start; the wire decoder is uniformly strict per field. Report, command and cause vocabularies become closed enums on the wire; adapters match enums, never strings; transport/broker kinds stay separate from core kinds. Row-level validation failures hold that row's scope as unreadable (never read as absent or closed); only envelope and identity failures reject a whole report. Each platform keeps its own UI for showing diagnostics. | F24 | reviewer (user may override) |
| R14 | Window-state commands. Candidate (not decided): neutral world commands for fullscreen (on/off/toggle) and sticky, plus `ToggleMaximize` adopted by Windows; core owns shared refusal and sticky-off policy; adapters only perform the native mechanism and report facts; the fullscreen fact gains ownership (tiler-owned vs app-owned, and whether exit is possible) so a neutral exit (Alt+Enter) routes safely. User 2026-10-31 approved option (a): neutral commands in v4, core owns refusal and sticky-off, fullscreen fact gains ownership and exitability; recorded as Contract entry "Window-state commands". User 2026-10-31: macOS 0.1 gets maximize and fullscreen; sticky unsupported until an all-Spaces route is proven. Sticky-off outcome needs no decision: KDE (decisions.md, Window State section, user 2026-09-25 option A) and Windows (`dispatch.rs` `StickyOffMode`) already agree (sticky toggle off keeps a prior float, tiles a prior tiled window; float toggle on sticky always tiles; current workspace via the approved rehome-target intent). User 2026-10-31 approved option (a): no Alt+Enter binding; Alt+Enter stays with apps and the existing `Meta+F11` / `Win+F11` toggle drives the fullscreen command. | F25 | User (contract, public behavior) |
| R15 | Diagnostics. reviewer 2026-10-31, applying the Observability decision and R13 (no product behavior change): core owns the event names and field sets for contract events (report rejected, command refused, row held, settings field invalid, settle timeout) and adapters log the core enums verbatim under those names; per-report and per-observation lines move to verbose/trace on KDE and macOS, normal level keeps change-only or per-second summaries as on Windows; KDE builds summaries from decoded values instead of re-parsing JSON; one level vocabulary `normal`/`verbose`/`trace` on every adapter; stale macOS refusal comment fixed. User 2026-10-31 approved option (a): a window must be identifiable at baseline without titles or content, so application identity (executable basename, window class, bundle id) and process ids may appear at normal level; titles, content, full paths and native window handles/ids stay redacted; recorded in the Observability decision. Open (evidence): macOS bundled stderr destination, Windows log size bound. | F26 | reviewer (user may override); user for privacy scope |
| R16 | Future platforms: no contract change for GNOME now. reviewer 2026-10-31 (joins R11): the adapter guide allows adapter-minted `NativeWorkspaceId` values that stay stable across host reindexing within a lifetime. Carry into a future GNOME port: keep one adapter lifetime across lock-time disable; represent `workspaces-only-on-primary` without forcing the user's setting; check extension distribution rules for a separate native service. ChromeOS: no adapter route without a host window-management API (unverified). | F27 | reviewer |
| R17 | Conformance. reviewer 2026-10-31 (testing only, no product behavior change): CI regenerates the world-wire fixtures and fails on a diff; at v4 the TS contract types are generated from the Rust types (tool chosen at implementation) and the fixture generator emits one sample per report, command, effect and outcome variant that the TS parser must accept; behavior moved into core (R2, R5, R14) is tested at the `WorldEngine::handle_report` boundary with the reference row id in the test name, so one core test covers every platform and adapter tests stay limited to projection and application. | F28 | reviewer (user may override) |
| R18 | External control. Candidate (not decided): one neutral command surface whose verbs are the `WorldCommand` wire names, the same on every platform, delivered over each platform's existing channel (KDE Planner queue, Windows command file, a macOS channel to add); probes and native inspection stay platform-specific developer tools; Windows renames its verbs before 0.1. reviewer 2026-10-31 (hardening inside the recorded same-user trust model, no behavior change for the tiler's own callers): KDE effect writer methods and `LastGesture` accept only the same sender their sibling methods accept; Windows command-file reads get a byte cap and reject unknown fields. User 2026-10-31 approved option (a): 0.1 ships the surface on KDE, Windows and macOS, marked experimental with no stability promise; verbs are the `WorldCommand` wire names; Windows renames its verbs; macOS adds a same-user channel; recorded as Contract entry "Scripting command surface". | F29 | User (public surface); reviewer for hardening |
| R19 | Native workspace backing. Review verdict: the five tentative entries are consistent with the rewritten convergence/correlation principle; recommend the user confirm them as active (native acceptance still TBD). User 2026-10-31 approved option (a): all five confirmed active; native outcomes stay TBD in their reference rows. reviewer 2026-10-31 (no behavior change, joins R6/R11 typing): attempts carry one pre/post/baseline set typed as a logical-or-native workspace reference instead of two parallel sets; before v4, check whether core gates native retirement desires on a capability for adapters without a removal consumer (macOS). | F30 | User (confirm tentative entries); reviewer for typing |
| R20 | Motion. Candidate (not decided): in v4 the border reply becomes an anchored descriptor (subject lifetime, core-computed outsets/width/radius, eligibility, the existing causal freshness witness); the adapter applies it to the live frame from its existing native event route, so motion behavior is one core rule on every platform instead of trail (Windows, macOS) or hide (KDE); underlay stays group-defined absolute. reviewer 2026-10-31 (no behavior change): close the neutral settling-marker investigation; a neutral settling representation may be proposed only with a reproduced failure and a host completion signal the adapter can read; host discrepancies go to "Shared: tolerate host-settled geometry". User 2026-10-31 approved option (a): adopt the anchored border in v4 now; macOS drag evidence no longer gates the shape; recorded as Contract entry "Anchored border". | F31 | User (contract, public behavior); reviewer for settling marker |
| R21 | Rust in the KDE native effect. Candidate plan (not decided): step 0 prerequisites (workspace `rust-version`, staticlib notices root, restored Cargo glue, Nix effect inputs, export version script, bridge spike C ABI vs `cxx`); step 1 pilot with active-border logic and gesture lifecycle, proving every package route, CI and sanitizers; step 2 the presentation consumer/router in Rust sharing `tiler-protocol` types, written together with the R20 anchored border; step 3 shortcut reconciler logic in Rust with KConfig/D-Bus I/O as a C++ shim; D-Bus endpoints, KCM widgets and the effect factory stay C++. User 2026-10-31 approved option (a): steps 0-2 before 0.1, step 3 after (or before 0.1 if rearchitecture and cleanup are finished before the first release); recorded in the Engine Architecture route B note. | F32 | User (timing, scope); reviewer for method |

## Governance review: principles

Source: `docs/principles.md` (read in full). Changes to principles need
explicit user approval; everything below is a candidate.

### P1. Layering states the right model; the contract does not follow it

- `principles.md:79-83`: platforms converge "to the latest desired state ...
  skipping superseded intermediate states". That is the state-centric model.
  F5/F17 (attempt lanes, work rosters, proofs) drift away from it. The
  principle is fine; the practice deviates. R1 realigns.

### P2. Causal-proof rule has no proportionality limit, so it breeds correlation machinery

- `:85-90` (distinguish side effects "by causal evidence, never timing") and
  `:149-151` (freshness "proven causally ... tokens, identities minted at the
  source") are correct, but nothing says when idempotent state convergence
  makes a token unnecessary. Every actuation kind therefore grew its own id,
  attempt row and settlement vocabulary (F4, F5, F17), against Simplicity
  (`:14-20`).
- Candidate addition: "Prefer idempotent desired-vs-observed convergence.
  Mint correlation identities only for non-idempotent or one-shot effects
  (focus grants, one-time geometry writes) and say why where they are
  defined."

### P3. "Platforms classify facts only they can know" is under-specified

- `:91-96` lets platforms classify facts and also forbids passing "raw state
  for the core to guess from". With no line between a native attribute and a
  classification, M and K run membership classification adapter-side and
  report the verdict as `floating`/`fit_excluded` facts (F10), so settings
  changes cannot reclassify in core.
- Candidate clarification: platforms report native attributes (window type,
  size constraints, ownership, observed cause with uncertainty); classifying
  those into tiled/floating/excluded is core policy. "Raw state" in `:95-96`
  should mean "unclassifiable noise", not native attributes.
- Update after the decisions review (G2): this is already a user decision
  (`docs/decisions.md:3359-3363`, 2026-10-20: "Core decides
  manage/float/ignore"). The gap is code drift plus an unmarked older
  delegation. On merit the core-owned rule is the better of the two (settings
  can reclassify, one implementation, cross-platform consistency); the
  principle wording should still say so, because the principle is what
  readers apply when no decision covers a case.

### P4. Platform options conflate host capabilities with behavior choices

- `:97` and `:113-117` say host limitations reach core as neutral options,
  "optional with opt-in from the platform". In practice those opt-ins became
  per-platform behavior profiles named after platforms (F12) and per-report
  flags (F1). The principle allows a platform to opt into behavior, which is
  platform policy under another name.
- Candidate: platforms declare capabilities and limitations (what the host
  can and cannot do) once per incarnation; core derives behavior from them.
  Behavior differences that are product choices are user settings with one
  shared default, never platform opt-ins.

### P5. Resilience's "narrowest scope" is violated by whole-report rejection

- `:132-137` says refuse the narrowest scope (operation, window, domain). A
  rejected command discards the whole observation (F7), and validation of
  one bad field rejects the whole report (historical example: Qt zero size
  hints, `docs/changes/archive/core-full-state-replies.md:200-204`). The
  principle is right; contract validation should be per row or per command
  where possible. Supports R3.

### P6. Ordering rule is violated by adapter vetoes

- `:145-148`: commands resolve against core-retained state at execution. W's
  `float-no-remap` veto and K's `af-` focus guards re-decide core outcomes on
  adapter state after core resolved them (F10). Supports R5.

### P7. Host Integration asks for three routes' worth of complexity by default

- `:34-41` requires, for every undocumented route: a documented automatic
  fallback and a user-configurable route switch. That is two code paths plus a
  setting per case, which is a fixed cost in tests and support. Simplicity
  (`:14-20`) is not referenced.
- Candidate: require the automatic fallback; make the route user-configurable
  only where breakage risk is material and stated.

### P8. No principle covers core compute cost or cross-platform conformance

- Efficiency (`:22-30`) covers polling only; Gaming (`:168-170`) says zero
  cost, but nothing bounds core per-report compute, which F15 identifies as
  the real latency risk.
- Consistency (`:43-49`) is a primary goal, yet Testing (`:205-211`) has no
  rule that each adapter passes one shared contract conformance suite. Parity
  drift found in F2, F3, F5 and B4 is what such a suite catches.
- Candidates: a per-report core latency budget with a regression check
  (R7); shared wire-level conformance fixtures all adapters must pass.

### P9. Nits

- `:58` "Full authenticity on each platform yields consistency" is not true
  when Tier 1 tilers disagree (D5, R-GRP-03, R-OUT-08). The tiering rules in
  `docs/decisions.md:2669` handle this; the principle line overstates.
- File Size (`:189-195`) is widely exceeded (F11: `engine.rs` 9,794 lines,
  `session.rs` 8,734, 15 more over the cap). Either the cap is enforced or it
  is aspirational; today it is neither stated as aspirational nor enforced.

## Governance review: decisions

Source: package E index plus direct reads of `docs/decisions.md:280-459`,
`:745-764`, `:1928-1947`, `:3205-3284`, `:3345-3434`, `:3510-3574`,
`:3626-3710`; `docs/backlog.md:40-64`;
`docs/changes/windows-workspace-return-focus.md:2200-2259,2395-2416`.

### G1. R1 overlaps an alternative the user rejected on 2026-10-28

- `decisions.md:412-420`: keep "core-owned per-window visibility work
  (Model A) ... rather than a platform-reconciles model reporting plain
  facts". The design records "the platform-reconciles alternative is not
  reopened" (`windows-workspace-return-focus.md:2414`); the backlog says the
  user kept Model A "after comparing" (`backlog.md:51-52`).
- The rejected "smaller alternative" in the design (`:2245-2252`) is an
  adapter-defined, correlated batch receipt. R1 differs: core still owns the
  complete per-window target set, policy and completion; only the wire shape
  changes (per-window desired state plus per-window facts and a generic
  last-write fact, with the arrive/focus/depart order as a fixed contract
  rule instead of per-group data). On the wire it is still close to "plain
  facts", and the platform holds departure hides until the focus proof, so the
  ordering logic moves into each adapter as a documented rule.
- The user's 2026-10-30 reopen of R1 was given before this conflict was
  surfaced. Resolution (user 2026-10-30): the reopen stands, and the Model A
  decision is not treated as settled; it changes if it has proven suboptimal.
- Merit assessment: Model A keeps visibility as core-issued per-window work
  with attempt and completion bookkeeping. That is the operation-centric shape
  F5/F17 trace to correlation machinery and adapter scheduling rules, and it
  diverges from the Layering principle that platforms converge to the latest
  desired state (`principles.md:79-83`, P1). The decision's own aims (core owns
  policy and completion meaning; platform responsibilities explicit and never
  competing, `decisions.md:412-420`) are met at least as well by R1: core owns
  the target set and policy, completion is judged from facts, and the
  platform's only ordering duty is one fixed contract rule. Recommendation:
  replace Model A with R1 for v4; the comparison options are Model A, R1, and
  a full platform-reconciles model (R1 differs from the last only in core
  keeping the target set and the arrive/focus/depart rule being normative).
  Approved by the user 2026-10-30; wire field names remain tentative until phase 0.

### G2. Classification ownership: decided for core, delegated in an older unmarked entry, code follows the older one

- `:3359-3363` (2026-10-20): platforms report normalized classification
  facts; "Core decides manage/float/ignore".
- `:752-762` (2026-10-15): "Classification edge cases may use sensible
  tentative adapter decisions"; macOS tentative adapter defaults.
- Code: M `classify_membership`, K `classify_retained_membership` (F10).
  The older entry is not marked superseded. Candidate: mark it superseded and
  schedule the extraction (R5).
- Status 2026-10-31: extraction scheduled in the R5 backlog line (M/K
  membership classification moves into core); marking the 2026-10-15 entry
  superseded waits for the decisions.md topic restructure.

### G3. Anti-fighting: decided for core, older KWin interim policy still recorded

- `:3364-3370` (2026-10-20): core owns anti-fighting and completion.
- `:3653-3675` (2026-09-26/28, interim): KWin's event-driven difference
  classifier, three-strike acceptance of client-held rectangles, retained
  applied evidence and a one-shot replan. Not marked superseded. Live remnant:
  K same-operation re-actuation veto (F10). Candidate: confirm what of this
  still runs and retire or re-scope it.

### G4. The host-synchronous exception is an open loophole

- `:3632-3634` (2026-09-24): "Host-synchronous paths may stay in the adapter
  where moving them would change latency or failure behavior".
- `:3211-3214` (2026-10-20): core applies actions "without platform code
  participating in those decisions".
- The older exception has no list of covered paths. W's `float-no-remap`
  veto, K's `af-` focus guards and M's zoom inference (F10) can all be read
  as covered. Candidate: retire it or bound it to a named list of
  mechanical-safety vetoes (lifetime, freshness, game/fullscreen safety).
- Status 2026-10-31: R5 gives all three cited vetoes a core destination. The
  only other named use found is `decisions.md:4993` (local selection and
  explicit native half-snaps stay host-synchronous). Bounding the exception to
  a named list is a wording change to a user-approved architecture entry;
  left for the decisions.md topic restructure, with the user.
- User 2026-10-31 approved option (a): the exception is bounded to mechanical
  safety vetoes (vanished window or stale handle, stale facts, game/fullscreen
  safety) and same-turn application of a core decision; anything else needs
  its own dated entry and a backlog item moving it into core. The
  `decisions.md` local-selection/half-snap use is stale (KDE routes
  direction-only commands; core owns the float-focus lattice and half-snap
  geometry) and is annotated superseded. Rule added to adapter guide section 6.

### G5. "KDE adopts core feature; Windows/macOS keep adapter path" has become a pattern

- Entries: fullscreen suspension (`:180-195`), birth clear (`:539-545`),
  above-tiles desire (`:2384-2404`), R-MAX-11 command (`:4819-4849`),
  maximized focus (`:4867-4883`), constructor profile T1 (`:3564-3572`:
  Windows `true/false` "to preserve ... adapter-owned maximized focus gates").
- Each is reasonable as a migration step, but none records an end state, so
  migrations freeze into per-platform profiles (F12). T1 (2026-10-18) also
  predates `:3247-3252` (2026-10-21) "maximize portable core authority without
  platform-specific policy" and principle `:113-117`.
- Candidate: each such entry states its end state (converge all platforms by
  milestone X, or a permanent capability with a host-limitation reason).

### G6. Rejection recovery was decided as a workaround, and macOS may not conform

- `:1935-1937` (user-approved 2026-10-26): "command rejection or
  catchup-required refusal services owed observation-only catchup on the same
  turn". This enshrines the two-call recovery that F7 traces to the
  contract. R3 would make it unnecessary.
- B4/F7: M clears the flight and returns `Refused` with no same-report
  observation fallback. If that path is a command rejection under this
  decision, M does not conform. Confirmed by package F: M does not conform
  (F18).

### G7. Intent persistence is split between "not a snapshot" and an adapter store

- `:3517-3518`: desired output "is not a restorable snapshot of trees,
  shares, MRU or minimized history"; `:3557`: KDE production uses
  "DiscoverWorld/DescribeWorld plus float-intent persistence" (adapter-owned
  `float_intent_store.rs`); `:1940-1946`: tiling overrides reset on restart,
  "shared restart store" is Future scope.
- R2 (core-owned intent persistence) needs a user decision on bringing the
  shared restart store forward or accepting float intent loss on restart for
  W/M (and K, if its store is removed).
- Correction 2026-10-30: Windows does not lose intent on tiler restart; it
  keeps float intent, tile overrides and sticky as window properties
  (`crates/tiler-windows/src/product_hide/sys/mod.rs`). KDE's store keys live
  `internalId` within a session-bus plus KWin-owner namespace. macOS: no
  intent persistence found by grep (not exhaustive). Resolved by R2 (a2).

### G8. Adapter authority statement predates the core-owned model

- `:3697-3709`: adapters retain "observation, identity, permissions,
  actuation, event ordering, acknowledgement, recovery, effects, UI, and
  delivery authority"; core "promises no uniform workspace, group, atomicity,
  or geometry semantics where public platform APIs cannot provide them".
  Written for the KWin-only era; "recovery" and "effects" authority overlap
  what `:3233-3246` later gives core. Candidate: rewrite to the current
  boundary or mark historical.

### G9. Stale or unmarked superseded entries

- `:3682-3685` 1 MiB request cap (superseded at `:3550`, entry unmarked;
  current 4 MiB).
- `:3551-3553` "strict v2 wire" (current v3).
- `:289-291` "exact `v:3` cutover" for suppression vs `:5640-5642` same-day
  renumbering to v4; earlier bullet unmarked.
- Index `:20-21`, `:44` show pending/parked items later implemented or
  approved (`:200`, `:220`, `:396-397`).
- `:1889` empty-workspace focus "remains undecided" vs `:379` decided.
- `:5438` links a moved change note (now under `changes/archive/`).
- `:3558-3562` KDE keeps its group-highlight route "until its v2 port";
  package F found the route retired (uniform presentation), so the entry
  is stale.

### G10. decisions.md mixes evidence logs with decisions

- 5,748 lines; package E8 lists 32 passages of test counts, measurements,
  gate status and follow-ups (e.g. `:1647-1715`, `:4973-5028`,
  `:5388-5451`). Principles `:8-9` define decisions as "the how ... including
  why". Combined with G2-G9, the current rule for a topic is hard to find and
  older contradicting entries survive. Candidate: move evidence to change
  records, mark superseded entries in place, and keep one current-contract
  summary per topic.

### G11. Tension between "minimal post-0.1 churn" and "avoid custom harnesses"

- `:323-324` (2026-10-29): tabs should be the only major deferral; keep
  post-0.1 protocol bumps few and deliberate. This supports making R2-R8 contract
  changes now, before 0.1.
- `:3691-3693`: "avoid extensive custom harnesses". P8's shared conformance
  suite would need a scoped exception; worth stating explicitly if adopted.
- Status 2026-10-31: R17 does not adopt a cross-adapter suite; it uses the
  existing fixture generator, a CI diff check and ordinary core-boundary
  tests, so no harness exception is needed.

## Governance review: the record model (what a principle and a decision are)

Source: `docs/principles.md:1-10,51-64`; `docs/decisions.md:1-40` and its
heading structure; `docs/porting/world-contract-v3.md:1-15`; file sizes.
Changes here alter governance and need user approval; all are candidates.

### M1. The four record types are defined by content, with no placement test

- `principles.md:5-10`: VISION is the goal, principles "the what and why",
  the functional spec "the exact behavior contract", decisions "the how:
  selected behavior and implementation choices", and "Overlap is expected".
- "Selected behavior" appears in both the spec and decisions, so behavior is
  recorded in three places (spec rows, `reference-outcomes.md` rows per
  `AGENTS.md`, and decisions such as D1 at `decisions.md:421-424`).
- "Implementation choices" puts code-level detail (KWin three-strike
  classifier, byte caps, G3/G9) at the same approval level as product
  intent, so it is equally expensive to change and goes stale silently.
- Candidate placement test: behavior a user can observe goes in the spec;
  a choice between viable alternatives with a lasting cost goes in decisions;
  everything derivable from code or tests goes in code comments or the change
  record (`AGENTS.md` already asks for point-of-use documentation).

### M2. The core/platform contract has no normative home

- Contract rules are spread over principles (Layering, Ordering), decisions
  (Engine Architecture and Convergence, `:3205-3716`, plus dated platform
  sections), the adapter guide, and design notes.
- The adapter guide calls itself "durable" yet states "strict v2"
  (`world-contract-v3.md:1-6`); the contract is v3. Its normative status
  relative to decisions is not stated.
- Effect: no single place says who owns each decision, so the overlaps in
  G2-G5 and G8 can coexist. Candidate: one normative contract document
  (ownership table, message shapes, ordering rules, conformance fixtures,
  version), with decisions recording only why a contract choice was made.

### M3. decisions.md is half a dated log, half a topic register

- 47 level-2 sections; 18 are dated buckets ("Windows decisions
  (2026-10-28)", "macOS follow-up review decisions 2026-10-28") alongside
  topic sections ("Workspaces", "Restart Persistence"). The header carries
  dated indexes and project status (`:6-40`).
- The current rule for one topic can therefore sit in a topic section and
  several dated buckets. Supersession marking exists (83 mentions of
  "supersed") but is applied inconsistently (G2, G3, G9).
- Candidate: one entry per topic holding the current rule only; dated history
  goes to Git and the archive; status lines move to the backlog or change
  records.

### M4. Decisions have no lifecycle or revisit condition

- The header defines two states: active user-approved, and explicitly
  flagged tentative clauses (`:3-5`). There is no status field, no
  "supersedes"/"superseded by" link, and no stated assumption whose failure
  reopens the decision.
- Example: the Model A entry links its comparison but not the assumption it
  rests on; the design adds "not reopened" (`windows-workspace-return-focus.md:2414`).
  Without a revisit condition, decisions harden into constraints that later
  work defers to; this review initially did so in P3 and G1.
- Candidate entry shape: decision, alternatives considered, key assumption,
  revisit-if, status, supersedes. Pre-0.1, a decision whose revisit condition
  is met is reopened by default.

### M5. Principles mix kinds and mostly lack stated trade-offs

- Sections cover product goals (Consistency, Priorities, Gaming),
  architecture rules (Layering, Ordering, Resilience), engineering practice
  (Unsafe Code, File Size, Testing, Observability) and process (Focus and
  Return on Investment).
- Priorities is explicitly "a judgement guide, not a strict rule"
  (`:55-56`) and game safety is explicitly absolute (`:61`), but other
  sections read as absolute rules ("never timing", "always keep a documented
  route"). P2 and P7 are cases where an unconditional rule produced
  machinery Simplicity argues against.
- Candidate: each principle states its intent, its default rule, and what
  it yields to; only game safety (and any others the user names) is absolute.
  Engineering-practice rules that are enforced by tooling (file size, unsafe)
  state the enforcement or are marked aspirational (P9).

### M6. Proposed record model (candidate)

| Record | Holds | Does not hold | Change authority |
|---|---|---|---|
| `VISION.md` | Why the project exists, end goals | Rules, behavior | User |
| `docs/principles.md` | Few durable trade-off rules; each states intent, default rule and what it yields to. Only game safety (plus any the user names) is absolute | Specific behavior, mechanisms, process detail enforced by tooling | User |
| Spec (`functional-spec.md`, `reference-outcomes.md`) | All user-observable behavior, as rows | Rationale beyond a link | User for product behavior |
| Contract document (evolve `docs/porting/world-contract.md`, not a new file) | Normative core/platform interface: ownership table, messages, apply/ordering rules, capabilities, version, conformance fixtures | History, per-platform status | User for shape; kept current with code |
| `docs/decisions.md` | Register of current choices between viable alternatives with lasting cost (product, architecture, contract). One entry per topic: decision, why, alternatives rejected, key assumption, revisit-if, status (active/tentative), date, evidence link | Superseded text (Git and archive keep it), evidence, test counts, status reports, implementation detail derivable from code | User; pre-0.1, an entry whose revisit-if is met is reopened by default |
| `docs/changes/` | Designs, evidence, measurements, status; archived when done | Current rules (link to decisions) | Working records |
| `docs/backlog.md` | Pending work only: one short line per item with links | History (backlog archive), evidence | Orchestrated, user priorities |
| Code comments | Implementation choices and their reasons at point of use | Product intent | Normal review |

- Placement test: observable behavior -> spec; a lasting choice between
  viable alternatives -> decisions; the core/platform interface -> contract
  document; derivable from code or tests -> code comment or change record.
- User 2026-10-30: approved as the target model; recorded in
  `docs/principles.md` and `docs/decisions.md`. Entry format approved the
  same day (recorded in `docs/decisions.md`). Principle format (Intent,
  Rule, Yields to, optional Example) approved the same day (recorded in
  `docs/principles.md` and `docs/decisions.md`). Cleanup order approved the
  same day: (1) principles migration with P1-P9 folded in; (2) migrate the
  stale/superseded decision entries already found (G2, G3, G8, G9, KDE
  group-highlight); (3) full `decisions.md` topic-register restructure with
  evidence moved out (G10); (4) adapter guide becomes the contract document
  (M2); (5) backlog trim (M7).
- User 2026-10-30: principle set and grouping approved: all 14 sections stay,
  grouped General (Priorities first, Gaming Compatibility `absolute`,
  Simplicity, Focus and ROI), Product (Consistency), Architecture (Layering,
  Ordering and Races, Resilience, Efficiency, Host Integration), Engineering
  (Observability, Testing, Unsafe Code, File Size). No new principles; P1-P9
  fold into the matching sections; each rewrite is approved separately.
- User 2026-10-30: Priorities and Gaming Compatibility rewrites approved and
  applied. Gaming now keys on fullscreen window state rather than game
  identity, prefers leaving uncertain fullscreen windows alone, lets explicit
  user actions apply, and keeps only anti-cheat safety and performance
  `absolute`. Follow-ups for the spec and research steps: re-check rows that
  cite "gaming compatibility must be flawless" (for example REQ-MAX-07b);
  define what each explicit action does to fullscreen state and whether a new
  window may take focus from a fullscreen one (reference-outcome rows); define
  the performance measurement (frame time, input latency while a game has
  focus); research which mechanisms anti-cheat systems flag, and audit
  current code against the anti-cheat rule (not yet checked).
- User 2026-10-30: Simplicity (P2 general form), Focus and ROI, Consistency
  (P4 behavior half, P8 conformance) and Layering rewrites approved. Layering
  was then trimmed to its trade-off (user-approved size rule: mechanisms go
  in the contract document, spec or decisions); its contract bullets (P1
  state-centric contract, P3 facts versus classification, P4 capability
  handshake and settings split, transient side effects) became the first
  new-format entries under `docs/../decisions/contract.md#state-centric-contract`. This sets direction
  for R4 and R5; their message shape stays open.
- User 2026-10-30: trims of already-rewritten principles approved: Gaming's
  anti-cheat mechanism list moved to `docs/../decisions/gaming.md#anti-cheat-mechanism-limits`;
  Focus and ROI goal pointer folded into Intent; Consistency cut to two
  bullets. Priorities and Simplicity unchanged.
- User 2026-10-30: Ordering and Races rewrite approved (P2 correlation only
  for effects unsafe to re-apply; P6 nothing re-decides resolved intent).
  Command payload rule plus a new safety-refusal clause moved to
  `docs/../decisions/contract.md#command-payloads`; the host-ordering citation and
  interleaving-test rule moves into Testing.
- User 2026-10-30: Resilience rewrite approved (P5: narrowest scope includes
  row and command, never a whole report for one bad row; host names made
  generic).
- User 2026-10-30: Efficiency rewrite approved (P8: core per-report cost
  within a stated latency budget with a regression check). Budget value is a
  later user decision after profiling (R7).
- User 2026-10-30: Host Integration rewrite approved (P7: user route setting
  only where breakage risk is real and stated; degrade truthfully where no
  documented route exists).
- User 2026-10-30: Observability rewrite approved (format only; yields to the
  Gaming performance guarantee). Open: whether window titles and app names
  count as user content in logs; current log content not yet checked; belongs
  with the unreviewed diagnostics area.
- User 2026-10-30: Testing rewrite approved (P8 shared conformance fixtures
  for every adapter, a target; the interleaving rule from Ordering and Races).
  Which adapters run `test-fixtures/world-wire/` today is not yet checked.
- User 2026-10-30: Unsafe Code rewrite approved (enforcement stated; option 2
  reworded as "a design that needs no unsafe code", order unchanged).
  Observation (out of brief): no principle covers memory safety in the KDE
  effect's C++ (`kwin/native-effect/`); a rule is a separate user decision.
  User 2026-10-31 approved option (a): the principle becomes "Rust and Memory
  Safety" (Rust wherever a host allows, including host-coupled code; C++ only
  as required shims, RAII, sanitizer CI) and reopened effect-linked Rust for
  re-evaluation against build, packaging and distribution cost.
- User 2026-10-30: File Size rewrite approved (not enforced by tooling;
  overruns tracked in the backlog). Cleanup step 1 (principles migration) is
  complete: all 14 principles use the approved format and P1-P9 are folded
  in. Next: step 2 (stale decision entries).
- User 2026-10-30: cleanup step 2 approved and applied in `docs/decisions.md`.
  G2: macOS tentative classification defaults reworded as core-rule
  candidates fed by facts. G3: KWin robust difference reconciliation,
  three-strike acceptance, applied-evidence and stale-snapshot replan entries
  removed (source check: `kwin/src/world-apply.ts` keeps no local retry
  budget, the core owns clamp assessment in `tiler-core` `size_hints.rs`, no
  `replan` in `kwin/src`); named under Rejected in "State-centric contract".
  G8: KWin-era adapter authority entry reduced to its still-valid facts. G9:
  1 MiB cap removed; "strict v2 wire" now "strict current wire"; `v:3` cutover
  now names v4 after the 2026-10-29 renumbering; empty-workspace focus now
  links to its decision; stress-acceptance link points to `changes/archive/`.
  KDE group highlight: entry now states uniform wire presentation everywhere.
  Code nits logged, not changed: `world-apply.ts:4-5` header still says
  "bounded triple clamp reassertions"; `GroupHighlightObject` in
  `activewindowborder.cpp` now serves general presentation facts. Left for
  step 3: G4 host-synchronous exception, G10/G11 testing-investment entry,
  index lines marked "pending" (implementation status not checked), and
  whether macOS adapter code still classifies (not checked).

### M7. Observation (out of brief): the backlog is not lean

- `docs/backlog.md` is 2,664 lines with sections for open work, future,
  pending live checks, open user decisions and known issues; its header says
  "Only meaningful pending or active work is listed". Contents were not
  reviewed. Same symptom as M3: history accumulating in a current-state file.

## Evidence: package A (contract surface)

### Evidence scope and notation

- Current source contract: `WORLD_CONTRACT_VERSION: u32 = 3` at
  `crates/tiler-core/src/world_report.rs:41`. The v4 design is not current
  adapter implementation evidence.
- Tables below describe observed constructors/consumers, not a support promise.
  `value` means a meaningful runtime input; `constant` means a literal in the
  inspected production constructor; `default` means the constructor calls
  `Default`, not that the type has no other supported values. `Not located`
  does not mean impossible. Conditional roundtrip counts are source-derived
  sequences, not measurements of a typical native action.
- Repeated constructor citations used in the field tables:
  - W: `crates/tiler-windows/src/tiling_sys/world_sequencer.rs:355-403`.
  - K: `kwin/src/world-report.ts:1828-1853`, fed by
    `kwin/src/world-observe.ts:1656-1672`; broker envelope stamping is
    `crates/omnitiler/src/world_broker.rs:897-919`.
  - M: `crates/tiler-macos/src/world_builder.rs:653-705`, with production
    input assembly at `crates/tiler-macos/src/world_runtime.rs:693-728`.
- Core source names in tables are relative to `crates/tiler-core/src/`;
  Windows `tiling_sys/` names are relative to `crates/tiler-windows/src/`;
  macOS source names are relative to `crates/tiler-macos/src/`.

### A1. Transport, event intake, and cadence

| Platform | Report -> core -> reply | Serialization actually on this path |
|---|---|---|
| Windows | `WorldSequencer::bootstrap` creates an owned `WorldEngine` (`tiling_sys/world_sequencer.rs:209-216`); `deliver` calls it directly (`:486-490`). Native diff applies the typed reply. | No adapter report/reply JSON transport. Constructor comment explicitly says `never encodes JSON transport` (`:320-322`). **Observation:** the shared core nevertheless canonical-encodes every newly accepted reply for its size gate, including Windows (`world_engine.rs:1395-1400`). |
| KDE | Script sends a string envelope to `com.omnitiler.Planner`, `/com/omnitiler/Planner`, `com.omnitiler.Planner1`, `DescribeWorld` (`kwin/src/world-entry.ts:149-155,3821-3823`). Broker stamps owner/adapter/generation/observation/request/command/clock, decodes to `WorldReport`, calls engine, encodes reply (`crates/omnitiler/src/world_broker.rs:897-919,948-967,1004-1016`). | Inner report is manually assembled JSON with escaped string values (`kwin/src/world-report.ts:1682-1684,1828-1853`); envelope builder uses `JSON.stringify` (`:1611-1620`); `parseWorldReply` calls `JSON.parse` (`:607-612`). Broker creates JSON strings before strict codec decode and encodes replies after core's own canonical size check. Raw classification is Rust-side before strict decode (`crates/omnitiler/src/kde_raw.rs:1-16`). |
| macOS | `build_dispatch_report` returns a typed report; `session.engine_mut().handle_report(&report)` then `sequencer().accept_reply(&reply)` (`world_dispatch.rs:317-349`); `consume_world_reply` emits per-app latest-wins frames/focus (`:375-390,1222-1227`). | No JSON encode/decode found in `world_dispatch.rs`. The report boundary is typed/in-process. Shared core still canonical-encodes accepted replies (`world_engine.rs:1395-1400`). JSON elsewhere, e.g. `ffi.rs:6398`, is not evidence of JSON report transport. |

Short verbatim boundary excerpts:

```rust
// crates/tiler-windows/src/tiling_sys/world_sequencer.rs:489-490
pub(super) fn deliver(&mut self, report: &WorldReport) -> WorldOutcome {
    self.engine.handle_report(report)
```

```rust
// crates/tiler-core/src/world_engine.rs:1395-1400
let outcome = staged.run_accepted(report, &owner, &correlation, digest, consumed);
match outcome {
    WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
        if reply_estimate(&reply) > WORLD_MAX_REPLY_BYTES
            || encode_reply_canonical(&reply).len() > WORLD_MAX_REPLY_BYTES
```

```rust
// crates/omnitiler/src/world_broker.rs:1008-1016
let report = match decode_request(stamped) {
    Ok(report) => report,
    Err(error) => return Ok(Err((error.kind, error.message))),
};
match self.engine.handle_report(&report) {
    WorldOutcome::Accepted(reply)
    | WorldOutcome::Duplicate(reply)
    | WorldOutcome::CommandConsumed(reply) => {
        let encoded = encode_reply(&reply);
```

| Platform | Invalidating inputs and scheduling facts |
|---|---|
| Windows | WinEvent hook set includes create/show/hide/destroy/location (`tiling_sys/run_loop.rs:401-413`, `tiling_sys/observe.rs:428-470`). Hook drain preserves foreground edges, counts show/hide/create/destroy/location, and excludes own overlays (`tiling_sys/run_loop_wait.rs:4-35`). Owner drains hooks before observation/apply with no message pump between (`tiling_sys/run_loop.rs:1073-1077`), pumps snap/workspace/send/output/migrate/history intake (`:1165-1197`), then `run_active_tick` (`:1202-1232`). Ack-only wakes explicitly skip reconcile/settings/input (`:1124-1137`). Healthy idle has no unconditional poll: all deadlines optional (`tiling_sys/run_loop_wait.rs:73-77`). Gesture-only pointer sampling is 100 ms; windowless topology fallback 2 s; degraded directory watch fallback 100 ms (`:50-71`). Sequencer invalidation synchronously advances current observation and sets dirty (`tiling_sys/world_sequencer.rs:244-252`). No global 120 ms debounce is shown on this Windows path. |
| KDE | Per-window `outputChanged`, `desktopsChanged`, `hiddenChanged`, `frameGeometryChanged` subscriptions at `kwin/src/world-entry.ts:4528`; workspace `windowAdded`, `windowActivated`, `desktopsChanged`, `screensChanged`, `currentDesktopChanged`, `activeScreenChanged` at `:4592`; explicit closes via `windowRemoved` at `:4608-4615`. Invalidations advance current observation before debounce (`:4579-4596`). `WORLD_DEBOUNCE_MS = 120` (`:167`); one armed timer calls `dispatch(false)` (`:975-992`). A separate zero-delay queued-directional drain runs after accepted replies but never preempts an armed native debounce (`:995-1008`). Explicit directional input has a no-flight/no-debounce/no-catch-up fast path (`:1288-1291`). Post-apply dirty work schedules another dispatch (`:3465-3476`). |
| macOS | Registered app notifications: `AXWindowCreated`, `AXFocusedWindowChanged`, `AXMainWindowChanged`, `AXWindowMoved`, `AXWindowResized` (`ffi.rs:931-937`); window notifications: `AXMoved`, `AXResized`, `AXUIElementDestroyed`, `AXWindowMiniaturized`, `AXWindowDeminiaturized` (`:949-955`). Engine inbox blocks on a condvar, then `process_job` (`runtime_thread.rs:283-288,330-344,1199-1214`). Main-thread discovery/topology/evidence polling cadence is 250 ms (`tile.rs:692-693`), but pending display/Space/native-completion events can run cadence work early (`:1778-1794,2542-2565`); not equivalent to a mandatory 250 ms engine delay. Frame output slots coalesce overwritten frames (`world_dispatch.rs:1557-1586`). Successful write-feedback staging invalidates the shared clock (`runtime_thread.rs:1817-1822`). |

#### Roundtrip accounting (source bounds, not native measurements)

- A core roundtrip here means one `handle_report` call plus its reply, not a
  D-Bus method count or native setter count. KDE additionally has command
  queue/take and presentation-read/wake methods (`world-entry.ts:154-168`).
- Core executes at most one carried command per accepted report and publishes
  one full snapshot (`world_engine.rs:1426-1430`). Attempts are vectors, not
  one report per window/setter (`world_report.rs:1186-1213`).

| Action | Windows | KDE | macOS |
|---|---|---|---|
| Numbered workspace select, bound destination | One command report; post-apply view/geometry evidence rides subsequent observation(s). Selection constructors: `tiling_sys/workspace_navigation.rs:96-98,149-151`; attempt staging: `tiling_sys/world_tick.rs:2767-2776`. Superseded command leg may first service one core-only catch-up (`:2117-2125`). | Number maps to `cmdSelectNumbered(null,n)` (`kwin/src/world-entry.ts:5099-5108`); command drains on fresh capture (`world_broker.rs:930-975`), native view write/readback becomes later `viewAttempts`. Dirty follow-up can incur observation debounce. Command rejection also performs an observation-only fallback in the same broker call (`world_broker.rs:977-982`). | `WorldCommand::SelectNumbered` (`world_command_map.rs:98-102`); native selection and its asynchronous arrival evidence are separate from report issuance. View/member/frame/creation feedback is drained at next report (`world_runtime.rs:601-604`). |
| Directional focus | One command report for ordinary tiled success; focus observation can generate another report. `FocusDirectional` constructors at `tiling_sys/keyboard_directional.rs:292,844`. | One carried directional command; no universal extra command for ordinary tiled success. Entry creates `cmdFocusDirectional` (`kwin/src/world-entry.ts:4973-4987`). | Keyboard maps to `Operation(CoreCommand::Focus)` (`world_command_map.rs:240-248`; wrapper `runtime_thread.rs:1693-1702`), not the dedicated `FocusDirectional` constructor. One report issues the effect, later native focus observations report resulting truth. |
| Window open | Observation report admits/replans; multiple frame setters may be returned, with their latest attempt rows combined in a later report (`tiling_sys/world_sequencer.rs:254-268,338-347`). | `windowAdded` invalidates; debounced full capture -> admission snapshot -> native apply -> later combined feedback. | AX window-created -> observation/engine job path; later per-app frame-write feedback can cause another report. Exact full notification-to-job mapping was not exhaustively traced here. |
| Drag | Report phases started/updated/released/cancelled; held pointer sampling 100 ms. Pending drop sends `Released` through `run_observation_tick` and reads `gesture_settled` (`tiling_sys/pending_drop.rs:487-515`). | Native gesture snapshot rides capture; `gestureIngress:null` in script is normalized broker-side (`kwin/src/world-report.ts:1851`). Number of updates depends on native input/coalescing. | Gesture dispatch builds report with `Some(ingress)`, calls engine, maps preview and terminal (`world_dispatch.rs:317-348,373-390`). Legacy `Operation(DragPreview/DragDrop)` is not active Meta+left route (`world_command_map.rs:35-39`). |

- **Unverified:** no fresh per-action native report-count trace was obtained.
  A successful bound view change commonly has an issuance report followed by
  a proof report, but the code does not establish an exact universal "two"
  for Win+2: stale flights, incomplete facts, geometry callbacks, creation,
  external events and native completion can change the count. No measured
  typical-count claim is made for any action/platform.

### A2. WorldReport field inventory

Every top-level field in `world_report.rs:1010-1219` is listed below. Lifetime
classification is about the inspected producer, not a wire-level immutability
guarantee: the struct accepts settings/capabilities on every report.

| Field | Lifetime | Windows (W constructor) | KDE (K constructor) | macOS (M constructor) | Main core consumer(s) |
|---|---|---|---|---|---|
| `contract_version` | Build contract | constant `WORLD_CONTRACT_VERSION` | wire `v` constant | same constant | `world_engine.rs:1240-1246` |
| `owner` | Owner/session | constant `WINDOWS_OWNER` | broker stamps `BROKER_OWNER` | constant `PHASE1_OWNER` input | owner parse/bind `world_engine.rs:1253-1261,1507` |
| `adapter_incarnation` | Incarnation | sequencer identity | broker-owned incarnation, not script capture identity (`world_broker.rs:900-902`) | session incarnation | freshness/reset `world_engine.rs:1271-1280,1391-1394` |
| `core_generation` | Generation | bound engine identity | broker generation | constant `PHASE1_GENERATION` input | first gate `world_engine.rs:1228-1232` |
| `observation_seq` | Invalidating event watermark | `self.current_obs` | broker stamp | ticket clock | `world_engine.rs:1321-1325`; reply correlation |
| `request_seq` | Per request | next sequenced request | broker stamp | ticket request | freshness `world_engine.rs:1291-1318` |
| `request_id` | Per request | `format!("req-{request_seq}")` | capture `source` stamped into `req` | ticket request id | correlation `world_engine.rs:1262-1268`; echo `world_publish.rs:941` |
| `command_seq` | Optional per command | supplied sequenced value | drained global command sequence | ticket command sequence | command highwater `world_engine.rs:1363-1374` |
| `command` | Optional per command | supplied typed command | queue head JSON, strict decoded | supplied typed command | `world_commands.rs:49-70,847-848` |
| `workspace_mode` | Setting | captured setting | effective requested mode | constant `PerOutputLocal` | `world_model.rs:58,305,360`; global swap `world_commands.rs:126` |
| `screens` | Observation, with settings in each row | captured screens | full topology/workarea screen facts | built screens | `world_observe.rs:32-33,164`; topology/ring policies |
| `windows` | Observation | captured facts | Rust-normalized raw KWin facts | built facts | `world_observe.rs:179,308` |
| `views` | Observation/proof | `captured.observed_views`, explicitly not last desire | observed views when host inventory known | constant empty; native evidence supplies proof | `world_observe.rs:168`; `world_engine.rs:1484` |
| `closed` | Explicit event obligations | captured closes | native deleted/removed ids | pending closes plus current closes | `world_observe.rs:185` |
| `host_owned_workspace_domains` | Capability field | constant `false` | **value:** `desktopsReadable && screenEntries.every((s) => s.current !== "")` (`world-observe.ts:1337`) | constant `true` | `world_native.rs:95-121`; native/logical evidence lane `world_settle.rs:58-77` |
| `equal_workspace_count` | Capability/setting | captured setting | observed `equalCount` | constant `false` | `world_model.rs:196,261`; `world_native.rs:554` |
| `birth_clear` | Capability | constant `false` | constant `true` (`world-report.ts:1834`) | constant `false` | `world_born.rs:224,261,481` |
| `native` | Observation, capability-gated | constant empty | native order/current/occupancy rows | native order/current/occupancy rows | `world_native.rs`; `world_observe.rs`; creation/transfer settlement |
| `fixed_size_predicate` | Setting | captured setting | config predicate resolved once per capture (`world-observe.ts:790-801`) | session predicate | `world_engine.rs:1512-1513` |
| `migration_source_refill` | Setting | captured setting | `input.refill` | constant `LastRemaining` in production input (`world_runtime.rs:722`) | `world_model.rs:878` |
| `anchor_fallback` | Setting | `Default::default()` | `input.anchorFallback` | `AnchorFallback::default()` | `world_engine.rs:1514` |
| `no_space_result` | Setting | `Default::default()` | `input.noSpaceResult` | `NoSpaceResult::default()` | `world_engine.rs:1515` |
| `switch_target` | Setting | captured setting | constant `"focused-output"` (`world-observe.ts:1670`) | default | `world_engine.rs:1520`; selection command resolution |
| `fresh_placement` | Setting | captured setting | constant `"focused-output-last-focused"` (`world-observe.ts:1670`) | default | `world_engine.rs:1521,1526` |
| `focus_follows_cursor` | Setting | captured setting | constant `false` | constant `false` | `world_engine.rs:1522`; `world_cursor.rs` |
| `cursor_warp` | Setting | captured setting | constant `false` | constant `false` | `world_engine.rs:1523`; `world_publish.rs:926` |
| `cursor` | Observation | captured point/None | native cursor fact | constant `None` | `world_engine.rs:1524-1527`; cursor/selection policy |
| `cursor_hit` | Observation | exact managed lifetime from native hit testing | raw `cursorWindow` normalized broker-side (script not final `cursorHit`) | constant `None` | `world_cursor.rs:85`; validation requires reported window and cursor (`world_report.rs:1109-1118`) |
| `focused_global` | Observation | captured global foreground lifetime | `activeId` | retained observed focus or empty | `world_observe.rs:378-381,748-757`; `world_presentation.rs:652-655,782-790` |
| `action_foreground` | Observation/context | grounded captured output | `activeName` | constant `None` | action resolution, `world_engine.rs:1467-1471` |
| `action_cached` | Retained observation/context | captured cached output | constant `null` | constant `None` | action resolution; digest `world_report.rs:1557-1564` |
| `action_outputs` | Registry/context | captured ordered known registry | live screen names (`world-observe.ts:1668`) | constant empty | action resolution; digest `world_report.rs:1566-1569` |
| `default_tiled` | Setting/seed | captured setting | `input.defaultTiled` | constant `true` in production (`world_runtime.rs:723`) | `world_model.rs:136,150,784`; trailing creation `world_commands.rs:676` |
| `born_maximized` | Setting | captured setting | `input.bornMaximized` | default | `world_born.rs:425,438` |
| `inventory_complete` | Observation | captured completeness | `observation.complete` | computed from context/roster/coverage | `world_observe.rs:229`; settle gates `world_settle.rs:1803` |
| `gesture_barrier` | Observation/barrier | captured barrier | `observation.gesture` | constant `false` in production input (`world_runtime.rs:720`) | `world_cursor.rs:170,222`; settlement and born gates |
| `gesture` | Optional per phase | captured lifecycle | script native snapshot, broker normalization | supplied gesture phase | `world_engine.rs:1492`; `world_gesture.rs` |
| `monotonic_ms` | Per report clock | supplied monotonic time | broker clock stamp | `input_eligibility::now_ms()` (`world_runtime.rs:727`) | frame retry/burst `world_frames.rs:554,679`; native creation stamp `world_native.rs:1202` |
| `frames` | Batched feedback | pending latest per window | `input.frameAttempts` | drained pending frame rows | `world_frames.rs:647-694`; float settle `world_settle.rs:1779-1795` |
| `member_attempts` | Batched feedback | pending per op/window | `input.memberAttempts` | drained member rows | exact correlation `world_settle.rs:28-40` |
| `view_attempts` | Batched feedback | pending per phase id | `input.viewAttempts` | drained view rows | exact correlation `world_settle.rs:45-55` |
| `clear_attempts` | Batched feedback | pending per op/window | `input.clearAttempts` | **constant empty in production** (`world_runtime.rs:713,798`) | directional/born clear settlement (`world_report.rs:1202-1207`; `world_born.rs`) |
| `creation_attempts` | Batched feedback | constant empty | native creation attempts | drained creation rows | `world_native.rs` creation binding/settlement |
| `presentation` | Per-report observations plus style settings | captured ingress | normalized observed ingress | built ingress | `world_presentation.rs:599-655,800-808` |

#### Embedded row fields and feedback shape

| Type and all fields | Producer/consumer evidence |
|---|---|
| `ScreenReport { output, full_rect, work_area, gap, outer_gap, complete, primary }` (`world_report.rs:673-680`) | All platforms produce screen identity/geometry/completeness; `gap`/`outer_gap` are settings, other fields observations. M uses `PHASE1_GAP`, `PHASE1_OUTER_GAP`, `primary_output:None` at `world_runtime.rs:724-726`. Per-field native assignment details for W/K screen rows not exhaustively collected. |
| `WindowFact { window, output, rect, floating, fit_excluded, fullscreen, maximized, sticky, fixed_auto, fixed_suppress, hints, minimized, observed_workspace, native_workspace }` (`world_report.rs:706-720`) | All platforms supply facts. W uses logical observation receipts and no native workspace; K/M use capability-gated native workspace evidence. K raw classification maps role/control/owner facts Rust-side and explicitly does not invent Windows `owned_dialog/no_activate/cloaked` facts (`kde_raw.rs:8-16`). Every individual flag assignment across all three producers remains a separate, uncompleted submatrix; do not infer equal platform richness from the common Rust type. |
| `ScreenView { output, view }` (`world_report.rs:729-732`) | W independently observed applied views (`world_sequencer.rs:324-350`); K observed views; M empty. Core `world_observe.rs:168`; raw echoes settle views before staging (`world_engine.rs:1481-1484`). |
| `NativeObservation { output, native_order, current_native, occupancy }`; `NativeOccupancy { native, count }` (`world_report.rs:738-740,775-780`) | W unused/empty; K/M meaningful. `current_native:None` can represent fullscreen/non-ordered Space; unknown occupancy is `None`, not zero (`world_report.rs:734-736,760-762`). |
| `FrameAttempt { window, target, setter, readback, dpi_before, dpi_after, attempt_ms, operation_id, prewrite }` (`world_report.rs:456-465`) | W/K/M frame feedback; W has per-window DPI basis, other coordinate spaces may carry None (`:439-442`). Correlates explicit float writes by op/window/target; regular frame attempts have no operation id. Core anti-fighting and float settlement, not adapter verdicts. Exact K/M per-field DPI/prewrite assignment audit remains unverified. |
| `MemberAttempt { operation_id, window, target_output, target_workspace, setter, pre, post, native_pre, native_post, attempt_ms }` (`world_report.rs:503-514`) | W/K/M; W logical evidence, K/M host-native lane. Core ignores logical pre/post for host-owned reports (`world_settle.rs:58-77`). |
| `ViewAttempt { operation_id, target_output, target_workspace, setter, pre, post, baseline, native_pre, native_post, native_baseline, attempt_ms }` (`world_report.rs:543-555`) | W/K/M; same capability-separated evidence lane. Attempts cover standalone/follow/migration-target/refill phases (`:516-541`). |
| `ClearAttempt { operation_id, window, setter, pre_maximized, post_maximized, attempt_ms }` (`world_report.rs:573-580`) | W/K meaningful, M production empty. Core clear completion/continuation. |
| `NativeCreationAttempt { operation_id, logical_workspace, target_output, setter, order_before, order_after, created_native, attempt_ms }` (`world_report.rs:818-828`) | W empty, K/M meaningful. Core owns native binding; correlated optional created id is native causal evidence, not permission. |

### A3. PresentationIngress field inventory and pager frame provenance

Producer sites: W `tiling_sys/world_capture.rs:834-857`; K
`kwin/src/world-observe.ts:1604-1654`; M `world_builder.rs:248-266`.

| Field | Windows | KDE | macOS | Core use / lifetime |
|---|---|---|---|---|
| `foreground` | verified captured token or empty | foreground record or empty | observed focus token | per observation; foreground freshness token in reply (`world_presentation.rs:804`) |
| `foreground_minimized` | captured fact | foreground minimized | minimized set membership | per observation; border/underlay eligibility |
| `foreground_maximized` | captured fact | foreground maximized | zoom/full-display heuristic, engine-placed exemption (`world_builder.rs:212-246`) | per observation; border/underlay eligibility |
| `foreground_fullscreen` | captured fact | foreground fullscreen | retained focused fullscreen | per observation; border/underlay eligibility |
| `foreground_cloaked` | captured DWM fact | constant false, explicitly no DWM equivalent | constant false | per observation on W; border/underlay eligibility |
| `foreground_hidden_workspace` | captured fact | foreground home not in visible native currents | `!matches!(eligibility.home_current, Some(true))` | per observation; hide eligibility |
| `foreground_shell` | captured fact | constant false, normal-window set excludes shell | constant false | per observation on W; hide eligibility |
| `foreground_frame` | live fact frame | non-minimized live frame, otherwise null | grounded retained observed live frame | per observation; border geometry |
| `border_enabled` | setting | effect snapshot available | style setting | setting/liveness; border eligibility |
| `underlay_enabled` | setting | effect snapshot available | constant false | setting/liveness; underlay eligibility |
| `border_gap_px` | scaled style `.0` | rounded/clamped snapshot style, zero if unknown | style gap | setting; geometry, already adapter units |
| `border_width_px` | scaled style `.1` | rounded/clamped snapshot style, zero if unknown | style width | setting; geometry |
| `underlay_extension_px` | resolved style `.2` | sentinel resolved to width then rounded/clamped | style extension, despite underlay disabled | setting; geometry |
| `chord_held` | native movement chord | effect snapshot chord level | constant false | per event; underlay trigger |
| `move_window` | classified native Move subject | foreground KWin `move` level (`world-observe.ts:1531-1542`) | constant empty | per event; underlay trigger |
| `paused` | true on limited fullscreen observation; false ordinary/command paths | constant false | constant false | per event; early hidden border/underlay and empty pager (`world_presentation.rs:163,808`) |
| `pager_order` | captured native top-to-bottom order | reversed native `stackingOrder`, capped 1024 | constant empty | per observation; retained/deterministic ordering fallback (`world_presentation.rs:599-624`) |
| `pager_frames` | live plus retained float/tile sources | live-only, float/tile null | live-only, float/tile None | per observation; wholesale rebuild of three core source maps (`world_presentation.rs:625-647`) |

`PresentationPagerFrame` has exactly `window`, `live`, `float_frame`,
`tile_frame` (`world_report.rs:593-598`). Actual sources:

```rust
// crates/tiler-windows/src/tiling_sys/world_capture.rs:813-821,823-831
for (token, frame) in state.float_rects.iter() {
    // row insertion omitted
        .float_frame = Some(*frame);
}
for (token, frame) in state.member_rects.iter() {
    // row insertion omitted
        .tile_frame = Some(*frame);
}
```

- W caches are not merely serialized core allocations. `snapshot_frames`
  stores `window.visible` into float or tile cache after identity binding
  (`tiling_sys/world_tick.rs:1390-1419`); hide-commit seeds store native frames
  by exact member lifetime (`tiling_sys/world_native.rs:1613-1633`); restart
  also seeds caches (`tiling_sys/restart.rs:137,494,571`).
- K explicitly has no retained frame store (`kwin/src/world-observe.ts:1644-1653`):
  `live: w.minimized ? null : w.rect, float: null, tile: null`.
- M likewise does not retain float/tile snapshots for ingress
  (`world_builder.rs:130-136,187-203`): `float_frame: None, tile_frame: None`.
- Core pager never falls back to `WindowFact.rect` or allocation. It
  rebuilds `presentation_live/float/tile` from ingress and resolves in that
  order only (`world_presentation.rs:591-597,625-672`). Thus a missing live
  source with no adapter-retained source yields no miniature even if core
  retains membership/layout.

### A4. WorldReply and nested WorldPresentation inventory

Core constructs all replies; platform entries below are consumers, not
producers. Accepted snapshot assignment is `world_publish.rs:937-958`;
presentation is initially default there and computed separately from ingress
(`world_presentation.rs:800-808`). Reply data is per accepted report, except
the explicit session/generation identity fields.

| Field | Core producer/meaning | Windows consumer | KDE consumer | macOS consumer |
|---|---|---|---|---|
| `contract_version` | constant version | typed reply boundary | strict JSON parser | typed reply boundary; explicit version check in sequencer not verified |
| `adapter_incarnation` | report echo | ack fence `tiling_sys/world_sequencer.rs:497+` | wire `adapter` | exact match `world_sequencer.rs:293-297` |
| `core_generation` | engine generation | ack exact generation (`:503`) | wire `gen` | exact match `world_sequencer.rs:288-292` |
| `request_id` | report echo | ack flight correlation | wire `req` | exact match `world_sequencer.rs:303-307` |
| `based_on_observation_seq` | report watermark echo | current-observation fence | wire `obs` | current clock AND flight watermark `world_sequencer.rs:318-325` |
| `state_revision` | monotonic accepted revision | apply/freshness fence | wire `rev` | newer-only `world_sequencer.rs:313-317` |
| `ready` | true on accepted publish | readiness/apply | wire readiness | requires true `world_sequencer.rs:308-312` |
| `command_consumed` | command highwater verdict | observation still incorporable | wire `consumed` | documented observation apply, no command re-execution (`world_sequencer.rs:279-285`) |
| `scopes` | per-domain authority | native diff authority | `world-apply.ts` authority gates | hold diagnostics `world_dispatch.rs:1313-1316` |
| `workspaces` | ring/order/bindings/tiling/hold/trailing | logical workspace facade | native desktop lifecycle and binding | native->logical current and pager translation (`world_dispatch.rs:1260-1268,1292-1302`) |
| `outputs` | logical views/tiling/holds | facade view application | native view application | synthetic current views `world_dispatch.rs:1274-1281`; native selection paths |
| `windows` | geometry/membership authority, homing | native frame/member diff | native frame/member diff | current-scope plan truth and writes `world_dispatch.rs:1321-1335,1362-1378` |
| `transfers` | retained operation targets/held | member/view apply | member/view apply; focus waits for follow view (`world-apply.ts:2269-2274`) | native select/send machinery; exhaustive field-read audit not completed |
| `effects` | per-report at-most-once focus/raise/stack | spend before native apply; selection grant below | spend and guard `world-apply.ts:2275-2303` | spend then dispatch focus (`world_dispatch.rs:1945-2037`); Raise/Stack have no native route (`:2098-2099`) |
| `command_result` | optional correlated result | directional, float and preview callers | wire `result`; native maximize route (`world-native.ts:950-1034`) | gesture preview mapper (`world_runtime.rs:844+`); exhaustive other result consumption not located |
| `gesture_settled` | one incorporated terminal, taken on publish (`world_publish.rs:929`) | clear override/pending drop (`tiling_sys/world_tick.rs:123-127`; `pending_drop.rs:515`) | parsed wire terminal; exact consumer not exhaustively traced | returned to drag controller (`world_dispatch.rs:374,396`) |
| `operation_settled` | terminal clear/float events, taken on publish (`world_publish.rs:933`) | trace/use at `tiling_sys/world_tick.rs:415` | wire parsed; exhaustive actuation consumer not located | no production use located in inspected dispatch/sequencer; do not infer absence repository-wide |
| `native_creations` | capability-gated native creation ops | empty under W capabilities | create exactly once per op | matched in `runtime_thread.rs:843,922`; `world_dispatch.rs:2210` |
| `cursor_warp` | optional point from core policy (`world_publish.rs:926`) | enabled by W setting; native warp gated by proof | setting constant off, so ordinary K output None | setting constant off, so ordinary M output None |
| `presentation` | `presentation_for_publish` | renderer consumes core shapes/content | script presentation subscriber -> native effect for border/underlay | shared applied-reply shapes -> main-thread renderer (`world_dispatch.rs:1380-1387`) |

All nested reply-row fields (definition inventory; not every native field
consumer has been individually traced):

| Row | Fields | Construction / usage scope |
|---|---|---|
| `ScopeStatus` | `scope`, `authority` | `world_report.rs:366-369`; all adapters use authority gates |
| `WorkspaceDesire` | `id`, `output`, `native_binding`, `scope`, `order`, `tiling_mode`, `existence`, `hold`, `trailing` | `world_report.rs:333-348`; native binding meaningful K/M, None W; `existence=false` is removal eligibility, not unconditional delete (`:324-331`) |
| `OutputDesire` | `output`, `logical_view`, `tiling_mode`, `hold` | `world_report.rs:356-362`; all adapters |
| `WindowDesire` | `lifetime`, `output`, `workspace`, `allocation`, `effective_frame`, `frame_authority`, `membership_authority`, `floating`, `float_origin`, `frame_operation`, `clear_operation` | `world_report.rs:286-322`; all adapters. `float_origin` explicitly diagnostic only, no functional branches authorized (`:303-306`). M production clear feedback empty even though common reply row has clear token. |
| `TransferTarget` | `operation_id`, `kind`, `target_output`, `target_workspace`, `follow`, `held` | `world_report.rs:270-280`; all adapters with variant-dependent native routes |
| `OneShotEffect` | `id`, `kind`, `target`, `issued_revision` | `world_report.rs:219-226`; all adapters spend id before dispatch; native kinds differ |
| `OperationEvent` | `operation_id`, `window`, `outcome`, `cause` | `world_report.rs:1279-1284`; 14 `OperationOutcome` variants at `:1234-1249` |
| `NativeCreationDesire` | `operation_id`, `logical_workspace`, `target_output` | `world_report.rs:792-796`; K/M; W native capability off |

Nested presentation fields:

| Type / fields | Core computation | Platform consumption facts |
|---|---|---|
| `WorldPresentation.foreground` | copied ingress token (`world_presentation.rs:804`) | W/M renderer freshness token; K effect payload also carries foreground |
| `WorldPresentation.suspended_output` | focused truthful fullscreen output if engine opt-in (`world_presentation.rs:775-790,805`) | KDE broker opts in; W/M initialization inspected does not opt in. K parser accepts optional `suspendedOutput` (`kwin/src/world-report.ts:1239-1252`); script border/underlay payload formatter does not forward that field separately. |
| `WorldPresentation.border` -> `visible`, `reason`, `inner`, `outer` | `visual.rs:202-207`; `world_presentation.rs:158+` | W/M render core shapes; K forwards visible/inner/outer, not reason (`kwin/src/world-presentation.ts:165-170`) |
| `WorldPresentation.underlay::Hidden` -> `base_revision`, `reason` | `world_presentation.rs:103-108` | W logs/classifies hidden; K sends `{status:"hidden"}`; M ingress disables underlay |
| `WorldPresentation.underlay::Found` -> `base_revision`, `home`, `group`, `focused_leaf`, `focused_window`, `members`, `union_rect`, `outer_rect` | `world_presentation.rs:89-98`; member `{ window, leaf, rect }` at `active_group.rs:24-28` | K formatter forwards focused win, outer rect and member window ids, explicitly not member rects (`kwin/src/world-presentation.ts:135-147,171-179`); W renderer consumes reply; exact W per-field usage not fully enumerated |
| `WorldPresentation.pager[]` -> `output`, `workspaces[]` | `world_presentation.rs:552-555,699-762` | W pager and M indicator consume pager; K parses common shape, native pager-render consumer not located in this package |
| `PagerWorkspace` -> `id`, `ordinal`, `label`, `current`, `occupied`, `contains_focused`, `trailing`, `windows[]` | `world_presentation.rs:534-547,736-759` | M additionally translates native bindings (`world_dispatch.rs:1285-1302`); exact per-platform consumption of each label/flag not fully enumerated |
| `PagerWindow` -> `window`, `frame` | `world_presentation.rs:524-527,731-734` | only members with resolved source included; no source means no miniature |

### A5. WorldCommand variant and payload inventory

`world_report.rs:862-1003` is the complete Rust variant list. K has wire
builders for this vocabulary (`kwin/src/world-report.ts:30-49,445-568`);
builder availability is not proof of an active keyboard route. M and W
constructor sites below are active-route evidence where found; `not located`
is explicitly not an unsupported verdict.

| Variant and every payload field | Windows constructor evidence | KDE constructor/route evidence | macOS constructor evidence | Core route |
|---|---|---|---|---|
| `Focus { window, direction }` | direct constructor not located | wire builder `world-report.ts:565`; active shortcut uses directional variant instead | direct constructor not located; keyboard uses Operation Focus | `world_commands.rs:70-103` |
| `Send { window, target_output, target_workspace, follow }` | `tiling_sys/dispatch.rs:811` | common wire vocabulary | direct constructor not located | `world_commands.rs:104-118` -> `start_send` |
| `SwitchView { output, workspace }` | active constructor not located | common wire vocabulary | consumed as select-like ticket `world_dispatch.rs:1612-1617`; constructor not located | `world_commands.rs:119-121` -> `retain_view` |
| `SelectWorkspace { output, workspace }` | `tiling_sys/selection.rs:73` | builder `world-report.ts:456` | indicator selection `world_command_map.rs:92-97` | `world_commands.rs:122+`, including global-unique swap |
| `SelectNumbered { output, index }` | `tiling_sys/workspace_navigation.rs:98,151`; CLI `cli_workspace.rs:689` | `world-entry.ts:5106` | `world_command_map.rs:98-102` | selection resolution in `world_commands.rs` |
| `SelectRelative { output, delta }` | `tiling_sys/history.rs:78,82`; CLI `cli_workspace.rs:707` | `world-entry.ts:5123-5125` | `world_command_map.rs:104-107` | selection resolution |
| `SelectPrevious { output }` | `tiling_sys/history.rs:76`; CLI `cli_workspace.rs:699` | `world-entry.ts:5119` | `world_command_map.rs:108-110` | selection/history resolution |
| `SelectTrailing { output }` | `tiling_sys/workspace_navigation.rs:96,149`; CLI `cli_workspace.rs:683` | `world-entry.ts:5106` index zero | `world_command_map.rs:111-115`, capability gate | trailing reuse/create |
| `SendRelative { window, source, delta, follow }` | `tiling_sys/sends.rs:87`; `workspace_send_tick.rs:126` | `world-entry.ts:5133` | active constructor not located in inspected map | source frozen before staging (`world_engine.rs:1446-1455`) |
| `SendNumbered { window, source, index, follow }` | `tiling_sys/sends.rs:81`; `workspace_navigation.rs:336` | catalog move route `world-entry.ts:5111-5116` | `world_command_map.rs:155-171,208-224` | frozen source; scoped numbered send |
| `SetWorkspaceTiling { output, workspace, tiled }` | active constructor not located | builder `world-report.ts:508` | active constructor not located | retains supplied per-workspace mode |
| `ToggleWorkspaceTiling { output, workspace }` | `tiling_sys/workspace_mode.rs:167` | builder `world-report.ts:511`; entry `:5149+` | active constructor not located | flips current retained mode at execution |
| `MigrateWorkspace { source_output, target_output, workspace }` | active constructor not located | builder `world-report.ts:520` | active constructor not located | explicit migration route |
| `FocusDirectional { window, direction }` | `tiling_sys/keyboard_directional.rs:292,844` | `world-entry.ts:4985` | active constructor not located; Operation Focus instead | world-owned directional/float-focus resolution |
| `MoveDirectional { window, direction, same_axis }` | `tiling_sys/keyboard_directional.rs:296` | `world-entry.ts:5002` | active constructor not located; Operation Move instead | world-owned directional/half-snap resolution |
| `SendDirectional { window, direction, follow }` | `tiling_sys/workspace_send_tick.rs:280,371` | `world-entry.ts:5138-5140` output-send route | active constructor not located | `world_commands.rs:807-819` |
| `MigrateDirectional { direction }` | `tiling_sys/migration.rs:157` | `world-entry.ts:5143-5145` migration route | active constructor not located | frozen action source `world_engine.rs:1467-1471`; `world_commands.rs:820-842` |
| `ToggleMaximize { window }` | active constructor not located | `world-entry.ts:5057-5074` | active constructor not located | `world_commands.rs:844-845` |
| `Operation(CoreCommand)` | Resize, ToggleFloat, ToggleOrientation | Resize, ToggleFloat, ToggleOrientation | Focus, Move, Resize, ToggleFloat, ToggleOrientation | `world_commands.rs:847-848`; full dispatch at `world_operations.rs:50-133` |

Actual Operation producers (not merely enum parsers):

- W: `tiling_sys/keyboard_resize.rs:214` wraps `CoreCommand::Resize`;
  `tiling_sys/float_toggle.rs:240,604` wraps `ToggleFloat`;
  `tiling_sys/dispatch.rs:528` wraps `ToggleOrientation`.
- K: `kwin/src/world-entry.ts:5013,5019` and
  `kwin/src/world-shortcuts.ts:195,256` submit resize/orientation;
  `kwin/src/world-native.ts:421,742` submits toggle-float with `rect`.
- M: `world_command_map.rs:240-290` constructs Focus/Move/Resize/ToggleFloat/
  ToggleOrientation; `runtime_thread.rs:1658,1693-1702` wraps the returned
  command in `WorldCommand::Operation`.
- Core also validates/dispatches legacy operation variants not found in these
  ordinary keyboard producers: `Reconcile`, `UpdateGaps`, `PointerResize`,
  `SendToWorkspace`, `SendToOutput`, `MigrateWorkspace`, `ReleaseDomain`,
  `DragDrop`, `DragPreview` (`world_report.rs:2641-2717`;
  `world_operations.rs:50-133`). This is not a claim of an exposed active
  native shortcut for every one.

### A6. CommandResult variant and payload inventory

All result variants are core-produced per command/gesture, never session
settings. They share snapshot ordering, not a second reply channel.

| Variant / all fields | Core meaning | Windows | KDE | macOS |
|---|---|---|---|---|
| `DragPreview { rect, hover, domain, source, target, revision }` | read-only preview, no revision advance (`world_report.rs:1368-1382`) | `tiling_sys/drag_preview.rs:184` | wire drag-preview; presentation preview subscriber path | `world_runtime.rs:844+`, `drag_drop.rs:1738` |
| `Float { window, rect, no_space }` | explicit float provenance; rect None on unfloat; no-space re-float (`world_report.rs:1383-1394`) | `tiling_sys/float_toggle.rs:612` checks `no_space` | native toggle-float route submits Operation | no production native result matcher located in inspected dispatch; geometry can still ride snapshot |
| `Migration { direction, source, target, members, floats }` | provenance; actual actuation snapshot+transfer (`world_report.rs:1395-1407`) | trace variant name; full native result-field use not enumerated | wire migrate-workspace result | active result consumer not located |
| `Directional { outcome, target, rect, cause }` | refusal/float focus/snap/need-clear decision, tiled success no result (`world_report.rs:1408-1431`) | `tiling_sys/keyboard_directional.rs:333,868` | dedicated directional routes; exact result reader not exhaustively traced | keyboard uses legacy Operation Focus/Move instead of dedicated directional command; matcher not located |
| `Maximize { window, maximized, clear_fullscreen, was_maximized }` | desired maximized state and source preimages (`world_report.rs:1432-1441`) | active command/result route not located | native consumer `kwin/src/world-native.ts:950-1034` | active command/result route not located |

### A7. F1-F5 source checks and observations

#### F1: full report costs and actual capability stability

- `WorldReport` is explicitly a complete observation on **every** update
  (`world_report.rs:1005-1008`); each platform constructor above includes
  settings/capabilities with observations and feedback.
- Core calls `report_estimate`, validates, computes full report digest, and
  uses full typed equality for duplicate identity (`world_engine.rs:1234-1246,
  1280,1298-1307`). Digest includes settings and capabilities
  (`world_report.rs:1541-1545,1614-1624`). On a new accepted report it clones
  the engine before running policy (`world_engine.rs:1391-1395`) and exact
  encodes accepted reply (`:1398-1400`). No attribution measurement here
  separates settings digest cost from the rest.
- **Observation:** K's `hostOwned` is not constant per incarnation in the
  inspected constructor. It is a readability expression and missing native
  evidence clears native rows (`kwin/src/world-observe.ts:1334-1337,1465+`).
  W and M literals are constant false/true respectively. The preliminary
  claim that all capability flags are stable adapter-incarnation facts
  therefore is not an accurate description of every current producer.

#### F2: by-id direction, selection differences, and Operation documentation

```rust
// crates/tiler-core/src/world_commands.rs:70-76
WorldCommand::Focus { window, direction } => {
    if !matches!(direction.as_str(), "left" | "right" | "up" | "down") {
        return CommandApplied::Rejected {
            kind: KIND_DIRECTION,
            message: MSG_DIRECTION,
        };
    }
```

- The rest of that arm checks known `window`, records focus memory, and
  emits `explicit_focus(command_seq, window.clone(), ...)`; `direction` is
  not passed to target selection (`world_commands.rs:77-102`). It also
  participates in input validation (`world_report.rs:2564-2565`) and encoded
  size (`world_sizing.rs:876-879`). This is a concrete non-geometric use,
  not an unexplained unread field.
- `SwitchView` goes directly to `retain_view`
  (`world_commands.rs:119-121`). `SelectWorkspace` first handles global-unique
  visible-elsewhere swapping, including mapping/session relocation and
  history updates (`:122-188`). W's `SelectionArrivalGrant::selects`
  includes the five Select variants and explicitly excludes SwitchView
  (`tiling_sys/world_diff.rs:354-368`). M selection ticket matching groups
  SwitchView with Select variants (`world_dispatch.rs:1612-1617`).
- The two SendSource fallback chains are explicit contract types
  (`world_report.rs:833-843`) and are frozen before stage/sweep
  (`world_engine.rs:1439-1455`). No equivalence claim is established by their
  common broad user intent.
- Stale maximize preimage prose is present on the fieldless
  `ToggleMaximize { window }` variant (`world_report.rs:988-994`); actual
  preimages are result fields (`:1436-1440`). The `FocusDirectional` doc
  literally says `the KDE focus policy` (`:948`).
- **Observation:** `Operation` docs say directional migration/domain release
  are refused (`world_report.rs:999-1001`), but current validator accepts
  `ReleaseDomain` and valid `MigrateWorkspace` (`:2645,2695`), dispatcher routes
  both (`world_operations.rs:81-85`), and helpers execute them:

```rust
// crates/tiler-core/src/world_operations.rs:1322-1329
self.start_migration(
    report,
    command_seq,
    &domain_key.output,
    &target_output,
    &domain_key.workspace.0,
    held_scopes,
)
// :1353-1355
self.engine.remove(&domain_key);
self.note_command(command_seq);
CommandApplied::NoEffect
```

#### F3: duplicated eligibility fields, paused lanes, and retained pager state

- `WindowFact` has minimized/maximized/fullscreen and rectangle
  (`world_report.rs:706-720`); presentation ingress independently has the
  foreground versions plus cloaked/hidden-workspace/shell
  (`:633-651`). Ingress may name an **unmanaged** foreground (`:606-609`),
  so not every such ingress field necessarily duplicates a reported managed
  WindowFact. M maximized ingress is derived from zoom/full-display rules,
  not a blind copy (`world_builder.rs:212-246`).
- W limited fullscreen path passes `None, true` for cursor/presentation pause
  into the observation tick (`tiling_sys/reconcile.rs:203-218`). Normal path
  passes false (`:146-158`); command/catch-up paths also force false
  (`tiling_sys/world_tick.rs:1994-1996,2197`). Full blocking suspension can
  skip reconcile entirely (`tiling_sys/run_loop_tick.rs:441-495`). K and M
  set paused false, with K explicitly documenting no game-identity baseline
  (`kwin/src/world-observe.ts:1636-1642`; M `world_builder.rs:264`).
- `suspended_output` is a different, output-scoped opt-in: no game/app/size
  heuristics, just `focused_global` -> staged fullscreen fact -> fact output
  (`world_presentation.rs:775-790`). KDE broker calls
  `.with_focused_fullscreen_presentation_suspension()`
  (`crates/omnitiler/src/world_broker.rs:446-452`). Core default is false
  (`world_engine.rs:848`); the search found no W/M production opt-in.
- Pager cache origins are enumerated in A3. **Observation:** adapter-retained
  float/tile ingress is W-specific in the inspected constructors and is
  native snapshot state, not uniformly a copy of core layout on all adapters.

#### F4: exact identity matching versus prefix branches

```rust
// crates/tiler-core/src/world_report.rs:235-240,249-250
pub fn explicit_focus(command_seq: u64, target: WindowId, issued_revision: u64) -> Self {
    Self {
        id: format!("fx-{command_seq}"),
        kind: EffectKind::Focus,
        target,
        issued_revision,
// ...
pub fn is_explicit_focus_for(&self, command_seq: u64) -> bool {
    self.kind == EffectKind::Focus && self.id == format!("fx-{command_seq}")
```

```rust
// crates/tiler-core/src/world_model.rs:1084-1089
vec![OneShotEffect {
    id: format!("af-{}", target.0),
    kind: EffectKind::Focus,
    target,
    issued_revision: self.state_revision + 1,
}]
```

- M normal focus dispatch requires explicit Focus/Operation Focus ticket and
  exact `is_explicit_focus_for(seq)` (`world_dispatch.rs:1980-1983,2033-2037`).
  Selection focus binds a deferred correlation, released after native arrival
  (`:1975-1978,1991-1999`). Thus explicit focus includes selection outcomes,
  not only by-id/directional focus commands.
- W selection arrival exception similarly filters exact identity and target
  (`tiling_sys/world_diff.rs:378-407`); diagnostic/route code also uses
  `starts_with("fx-")` (`tiling_sys/keyboard_directional.rs:616,897`). This
  does not by itself establish that all W focus actuation is prefix-gated.
- K has functional `af-` prefix checks: preserve focused float/sticky over
  newcomer (`kwin/src/world-apply.ts:2302-2313`); permit ordinary admission
  float target (`:2330-2342`); fence fullscreen/maximized siblings
  (`:2352-2376`). Verbatim discriminator:

```ts
// kwin/src/world-apply.ts:2330-2334
const replyRow = reply.windows.find((w) => w.win === fx.target);
const admissionFloatTarget =
    fx.id.startsWith("af-") &&
    observed.floating === true &&
    replyRow?.floating === true;
```

- `FrameSetter` is indeed reused in frame/member/view/clear/native-creation
  attempts (`world_report.rs:459,508,547,576,822`). Naming preference is not
  a behavioral finding here.

#### F5: what raw attempts actually feed

- Per-window latest-wins feedback is batched. Windows maps by window or
  operation/window and sorts into one report (`tiling_sys/world_sequencer.rs:
  254-285,338-347`); M drains all pending feedback vectors together
  (`world_runtime.rs:601-604`). This is not a mandatory separate core
  roundtrip for each native setter.
- Core member correlation matches operation/window/target output/workspace
  (`world_settle.rs:28-40`), view correlation matches owning phase/target
  (`:45-55`). Capability changes which raw lane is used, not an OR fallback
  (`:58-77`).
- Frame policy consumes attempt timestamps/readback/basis and maintains burst
  counts and learned minimum evidence (`world_frames.rs:647-694`):

```rust
// crates/tiler-core/src/world_frames.rs:667-670,685-693
if let Some(current) = self.eligible_fact(report, &attempt.window) {
    match self.frame_bursts.get_mut(&attempt.window) {
        Some(entry) if entry.desired == attempt.target => {
            entry.attempts = entry.attempts.saturating_add(1);
// ...
// Learned minima ride the same fresh-evidence gate but persist:
// only stable readable accepted growth at the requested origin
// learns (synchronous shape, or async shape where the readback
// proves dispatch at target and the later fact holds the size).
```

- Float settlement exact-correlates frame operation/window/target and saves
  fresh attempt evidence across later unreadable reports
  (`world_settle.rs:1770-1795`). Its docs explicitly distinguish at-target
  observation, ambiguity hold, and external supersession
  (`:1737-1755`). These are concrete policy consumers of attempts.
- **Unverified:** no byte attribution or latency attribution establishes
  attempts as the main size/cost driver. The synthetic report and test timings
  below are total workloads, not an attempt-lane ablation. Whether individual
  evidence lanes should be replaced with facts is not determined here.

### A8. Existing measurements and bounded offline test

| Evidence | Recorded numbers (verbatim values) | What was measured / limits |
|---|---|---|
| `docs/changes/kde-live-acceptance.md:185-200` | request `4194304` bytes accepted, unpadded `1925198`; reply `1608803` bytes parsed in `904 ms`; construction `273 ms`; identical resend `380 ms`; `4194305` refused before decode | Synthetic 5000-window/64-domain transport test, no native window changes. Delivery proof, not responsiveness/layout usability. Exact-4-MiB reply delivery untested. |
| `docs/changes/kde-world-v2-port.md:140-162` | 40 reports `54,977 ns`; 10 core calls `5,877,546 ns`; 10 publication wraps `5,999 ns`; candidate offers `6,078 ns`; retained descriptor application `7,156 ns` | Historical illustrative debug run, test scaffolding/clock overhead included; virtual tick deltas are not native ms/transport timings/asynchronous core benchmarks; no threshold. |
| `docs/changes/windows-workspace-return-focus.md:1337-1354` | mixed 102 commands, 23/23 arrivals; visibility-post->foreground callback mean `79.4 ms`, median `80 ms`, range `74-84 ms`; SHOW->foreground event mean `52.3 ms`, median `47 ms`, range `31-79 ms`; rapid 104 selects, 52/52 returns, callback mean `80.8 ms`, median `80 ms`, range `74-90 ms` | Historical native visibility/focus evidence, not isolated handle_report latency. Hook delivery upper bound and roughly 3 ms drain. Visibility APIs `0-1 ms` did not prove native landing; per-member prep/readback could separate posts by 33 ms. |
| `docs/changes/macos-world-v2-port.md:1083-1090` | reconcile wait `256 ms`, input->reconcile `423 ms`, native Dock `43 ms` | Historical dogfood observations; raw logs not public in record. The 250->50 ms cadence bound comparison is theoretical, not a measured latency distribution. Current event-driven early cadence evidence is in A1. |
| `docs/research/js-baseline-measurement/wayland-revalidation.md:94-106` | Variant B X11/xterm windowAdded tier 20 median `7.5`, p95 `14`, max `14` ms; tier 50 median `21`, p95 `42`, max `90`; Wayland/konsole tier 20/50 median `0`, p95 `1`, max `1` ms | Legacy KWin JavaScript path, not world-v3 handle_report/JSON transport. |
| `docs/research/js-baseline-measurement/timing-attribution.md:57-58,159,177-179` | CHANGE `n=63`, median `1`, p95 `29`, max `526`, mean `42.9 ms`; SAME `n=37`, median `0`, p95 `1`, max `5`, mean `3.4 ms`; amplified `0.0066 ms mean`, `0.034 ms per write`, `0.006-0.026 ms per full small-N reconcile pass` | Legacy JS measurements and amplification, not current Rust world contract cost. |
| `docs/research/js-baseline-measurement/wayland-revalidation.md:112-115` | RSS deltas X11 `24156` / `24960` KB; Wayland `31432` / `28852` KB at tiers 20/50 | Process RSS deltas, not allocation counts or message sizes. |
| `docs/research/sustained-workload-validation/extension-point-asymmetry.md:137` | `No performance claims are made beyond source evidence.` | Source/extension-point research, not measured current workload. |

- Existing sizing implementation is `world_sizing.rs`; encoded-size checks
  include `world_sizing.rs:1199-1200,1811-1813`. No printed size benchmark was
  run from that module. `world_stress.rs` is an existing stress driver; it was
  not executed for this evidence package.
- Bounded existing offline measurement test executed successfully:

```sh
cargo test -p tiler-core --test moving_border_comparison moving_tracking_lag_with_virtual_schedule -- --exact --nocapture
```

- Result: `1 passed; 0 failed; 7 filtered out; finished in 0.01s`.
  Source for printed counters: `crates/tiler-core/tests/moving_border_comparison.rs:
  1334-1385`. Selected output, preserving numeric values:

```text
motion: ticks=40 completions=10 superseded=29
motion: visible_ticks=35 hidden_ticks=5 err_samples=35
motion: live_pos_max=160 live_pos_mean=129.14 live_dim_max=24 live_dim_mean=12.40
motion: anchored_pos_max=0 anchored_dim_max=0
motion: new_paint_age_min=5 max=5 mean=5.00
motion: retained_visible_age_min=5 max=8 mean=6.46 samples=35
motion: measured_obs_to_start_min=0 max=0 mean=0.00
motion: measured_start_to_complete_min=3 max=3 mean=3.00
motion: measured_complete_to_publish_min=1 max=1 mean=1.00
motion: measured_publish_to_paint_min=1 max=1 mean=1.00
motion: measured_new_reply_total_min=5 max=5 mean=5.00 rows=9
microbench_illustrative_no_thresholds_ns: build=77277 handle=9144295 calls=10 publish_wrap=8743 offer=8551 retained_recompute=9073
```

- This is an offline virtual-schedule moving-border comparison, not a native
  benchmark. The `motion:` schedule quantities are virtual ticks; only the
  explicitly `_ns` microbench line is wall-clock nanoseconds. It has no
  production threshold and no serialization/attempt-lane attribution.

### A9. Production source files longer than 1,500 lines

- Full-file line counts, including embedded test modules. Dedicated test-only
  files (`*_tests.rs`, `tests_*.rs`, `tests.rs`, `tests/`) excluded. A file
  containing an early `#[cfg(test)]` helper is not necessarily test-only.
  Counts obtained from source-file `wc -l`, not generated bundles. macOS
  diagnostic/live tooling is separated from ordinary runtime sources below.

| Directory prefix | File: full-file lines |
|---|---|
| `crates/tiler-core/src/` | `boundary.rs: 1579`; `contract.rs: 1509`; `directional.rs: 1675`; `engine.rs: 9794`; `reconcile.rs: 3777`; `session.rs: 8734`; `session/world.rs: 2277`; `session/ops/drag.rs: 1750`; `session/ops/move.rs: 1999`; `session/ops/resize.rs: 1855`; `world_engine.rs: 1929`; `world_native.rs: 1823`; `world_operations.rs: 1851`; `world_presentation.rs: 1958`; `world_report.rs: 2722`; `world_settle.rs: 1893`; `world_sizing.rs: 1883` |
| `crates/tiler-protocol/src/` | `world_protocol.rs: 2195`; `world_stress.rs: 2606` |
| `crates/tiler-windows/src/` | `ffi/pager.rs: 1502`; `gaming.rs: 1739`; `pager.rs: 2331`; `pager_sys.rs: 1664`; `product_hide/sys/mod.rs: 2616`; `tiling_sys/observe.rs: 1824`; `tiling_sys/pager_host.rs: 4560`; `tiling_sys/run_loop.rs: 1560`; `tiling_sys/world_capture.rs: 1752`; `tiling_sys/world_diff.rs: 4503`; `tiling_sys/world_native.rs: 2655`; `tiling_sys/world_tick.rs: 6869` |
| `crates/tiler-macos/src/` runtime and native support | `ax.rs: 2198`; `ax_session.rs: 1563`; `border_runtime.rs: 1935`; `border_session.rs: 1801`; `drag_drop.rs: 2077`; `drag_session.rs: 2546`; `ffi.rs: 12210`; `group_underlay_session.rs: 2018`; `input_session_legs.rs: 1661`; `mc_lifecycle.rs: 1561`; `native_tile_backend.rs: 2138`; `runtime.rs: 2219`; `runtime_core.rs: 1686`; `runtime_thread.rs: 6105`; `runtime_types.rs: 1541`; `runtime_worker.rs: 1536`; `tile.rs: 5501`; `workspace_indicator.rs: 3482`; `workspace_indicator_proof.rs: 3067`; `workspace_select.rs: 2267`; `workspace_send_effect.rs: 1861`; `workspace_send_managed.rs: 2186`; `workspace_send_native.rs: 1704`; `world_builder.rs: 3526`; `world_dispatch.rs: 3723` |
| `crates/tiler-macos/src/` CLI/fixture/probe/live tooling | `cli.rs: 3036`; `fixture_post.rs: 3932`; `live_leg.rs: 2254`; `probe_target.rs: 2089`; `space_direct_probe.rs: 1850`; `space_drag_live.rs: 1774`; `textedit_post.rs: 2578` |
| `crates/omnitiler/src/` (KDE service) | `float_intent_store.rs: 1522`; `planner_service.rs: 2162`; `tray.rs: 2561`; `tray_endpoint.rs: 3553`; `world_broker.rs: 2865`; `world_service.rs: 1558` |
| `kwin/src/` | `world-apply.ts: 2429`; `world-entry.ts: 5161`; `world-native.ts: 1794`; `world-observe.ts: 1673`; `world-report.ts: 1854` |

### A10. Explicit verification gaps

- No live tests were run. Exact native report counts for workspace selection,
  focus, birth, and drag remain unmeasured; A1 contains source-path bounds.
- Top-level WorldReport/WorldReply fields and complete command/result variant
  payloads are inventoried. Per-field native assignment/consumption of every
  nested fact/desire/attempt/pager row is not exhaustive; gaps are identified
  in A2/A4/A6. Common type/codec presence is not treated as native support.
- The KDE broker's raw cursor/gesture normalization and every native
  CommandResult reader were not exhaustively traced. Command constructor
  absence is only a search result, not a platform capability verdict.
- No measured attribution establishes which field group dominates size,
  cloning, hashing, validation, encoding, or core latency.

### A11. Targeted nested-field evidence and result-reader completion

This supplement supersedes the incomplete native-result cells in A4/A6 and
narrows A10. The nested-field matrix remains deliberately partial: the rows
below identify concrete constant/unused/asymmetric lanes, not an exhaustive
native-consumer audit. Paths use the prefixes declared above; `kde_raw.rs`
here is `crates/omnitiler/src/kde_raw.rs`, not a protocol-crate file.

#### Nested facts with constant or asymmetric values

| Field(s) | Windows evidence | KDE evidence | macOS evidence |
|---|---|---|---|
| `WindowFact.observed_workspace` | Every constructor in `tiling_sys/world_capture.rs` sets `None`: `:974,996,1245,1284,1413,1511`. | Projection sets `"observedWs": null`, `crates/omnitiler/src/kde_raw.rs:523`. | Regular/minimized rows set `None`, `runtime_core.rs:320,343`; unknown-home rows likewise, `world_builder.rs:638`. W's logical proof in A2 is therefore not a populated WindowFact receipt: view/member feedback lanes are separate. |
| `WindowFact.native_workspace` | Every constructor sets `None`, `tiling_sys/world_capture.rs:975,997,1246,1285,1414,1512`. | Exactly-one-desktop and host-owned condition, `kde_raw.rs:498-502,524`. | Regular/minimized rows set native Space, `runtime_core.rs:321,344`; grounded unknown-home rows set `None`, `world_builder.rs:639`. |
| `fit_excluded`, `maximized`, `sticky`, `fixed_suppress` | Runtime-derived on ordinary rows, `tiling_sys/world_capture.rs:1273-1281`; unreadable/minimized placeholder rows zero most flags but retain supplied sticky, `:959-998`. | Derived flags, not constants: `kde_raw.rs:478-497,515-520`; sticky is `on_all_desktops`, maximized is `maximize_mode != 0`, fixed suppression is tile intent. | All four are literal `false` in ordinary/minimized collectors, `runtime_core.rs:312-317,335-340`, and unknown-home rows, `world_builder.rs:630-635`. Maximized presentation ingress is separately derived, not this fact (`world_builder.rs:212-246`). |
| `fixed_auto` | Classified state, suppressed when pinned tiled: `fixed_auto: fixed_auto && !fixed_suppress`, `tiling_sys/world_capture.rs:1280`; placeholder false at `:968,992`. | `let fixed_auto = fresh_fixed`, `kde_raw.rs:475-480`; hints are retained separately, `:504-509,522`. | Always false from both returns of `normalize_wire_flags`, `runtime_core.rs:86-95`, called by regular and unknown-home collectors (`:306`; `world_builder.rs:623-624`); minimized rows false (`runtime_core.rs:339`). Hints still ride, `:318,341`. |
| `ScreenReport.primary` | First enumerated work-area device comparison, `tiling_sys/world_capture.rs:1032`. | Active-screen comparison, not a separate hardware-primary read: `kwin/src/world-observe.ts:1324-1330,1429`. | Explicit input else first output, `world_builder.rs:476-480,494`; native production input uses `primary_output: None`, `world_runtime.rs:724`; skipped screens false, `world_builder.rs:516`. |
| Screen `gap`, `outer_gap`, `complete` | Settings and derived completeness, `tiling_sys/world_capture.rs:1024-1031`. | Captured `gap, outer`, readability-based complete, `kwin/src/world-observe.ts:1425-1428`. | Builder forwards input gaps and derives completeness, `world_builder.rs:491-494`; skipped rows complete false, `:513-515`. Native input uses `PHASE1_GAP`/`PHASE1_OUTER_GAP`, both 8 (`world_runtime.rs:725-726`; `runtime_types.rs:44-46`); synthetic branch instead uses outer gap 0 (`world_runtime.rs:733,812-813`). |

Short verbatim assignments underlying these distinctions:

```rust
// crates/tiler-macos/src/runtime_core.rs:91-94
if known.exception && !fullscreen && !floating_cls && known.placed {
    return (true, false);
}
(floating_cls, false)
```

```rust
// crates/omnitiler/src/kde_raw.rs:498-502
let native_ws = if host_owned && desktops.len() == 1 {
    Some(desktops[0].clone())
} else {
    None
};
```

#### Desired rows and pager: targeted native reads versus carriage

| Field(s) | Windows | KDE | macOS |
|---|---|---|---|
| `WindowDesire.float_origin` | Diagnostic projection reads it (`tiling_sys/world_tick.rs:732`); capacity-float code explicitly excludes it from functional parity (`tiling_sys/capacity_floats.rs:12`). | Parser validates/carries `float_origin` (`kwin/src/world-report.ts:844-855`); search of `kwin/src` found no property read outside that codec. | Search of `crates/tiler-macos/src` found only test initializer occurrences, no production field read. Whole desired rows are nevertheless cloned into lifetime memory (`runtime_core.rs:267-271`). |
| `allocation`, `effective_frame`, `frame_operation`, `clear_operation` | Explicit placement consumes frame token and effective frame (`tiling_sys/world_diff.rs:2970-2983`); clear token has its own loop (`:3120-3125`). The `world_tick.rs:716-735` projection is diagnostics, not itself native actuation. | Native apply consumes `frame_op`/`frame` and `clear_op` (`kwin/src/world-apply.ts:1065-1081,1233-1235`). | Plan truth uses `effective_frame.or(allocation)`, `world_dispatch.rs:1321-1334`; actuation requires effective frame and spends frame token (`:1362-1378`). No production `clear_operation` field reader found in macOS source; clear feedback is empty as recorded in A2. |
| Workspace `native_binding` / pager trailing linkage | W capability excludes native bindings; no claim of a functional binding consumer from the common row alone. | `nativeWsOf(reply, t.out, t.ws)` gates transfer target (`kwin/src/world-apply.ts:1817-1825`). | Applied native->logical lookup (`world_dispatch.rs:1260-1268`); indicator spare needs both pager and desire trailing plus unbound desire (`workspace_indicator.rs:374-381`). |
| Pager `ordinal`, `current`, `contains_focused`, `occupied`, `trailing`, `label`, window `frame` | Production mapper copies id/ordinal/current and window frame; miniature focus uses `workspace.current && pager_window_focused(...)`, not `contains_focused` (`tiling_sys/pager_host.rs:401-438`). This mapper does not read label/occupied/trailing/contains_focused. | Pager shape parsed (`kwin/src/world-report.ts:1181-1232`), but no pager-field renderer read found in `kwin/src`; effect payload contains foreground/border/underlay/preview, not pager (`kwin/src/world-presentation.ts:220-230`). | Indicator uses ordinal/current/trailing/occupied (`workspace_indicator.rs:355-399`); miniature mapper uses window frame and `contains_focused` (`pager.rs:143-165,198-209,244-255`). These mappers do not consume core label. |

- **Observation:** Windows and macOS miniature focus guards read different
  pager flags. W uses `current`; M uses `contains_focused`. This is a source
  distinction, not an assertion that their resulting focus marks differ.
- No desired-row field is declared globally unused solely because one
  renderer or apply loop omits it. The bounded no-read searches above exclude
  tests from the runtime claims and do not treat parsing/copying as actuation.

#### CommandResult delivery: core snapshot versus KDE requester lane

- Correction to A6's introductory sentence: core produces a result in its
  `WorldReply`, but **KDE does have a separate native terminal-result channel**.
  The broker extracts the encoded result, settles the drained queue head,
  and nulls the actuator response result. Thus grepping `reply.result` in
  native apply would miss real result consumers.
- Exact flow: `crates/omnitiler/src/world_broker.rs:967-975` calls
  `result_of(&encoded)` then `pop_settle_ready`; `:1035-1038` stores the
  sender-bound settlement. `:840-858` extracts renderer-only preview and
  strips the result. `:1157-1184` serves settled/retired take results;
  `:1997-2002` serializes `Ready.result`. Service forwarding is
  `crates/omnitiler/src/world_service.rs:253-261` and
  `crates/omnitiler/src/planner_service.rs:547-548,576-585`.
- Script sends `TakeCommandResult` (`kwin/src/world-entry.ts:155,2825-2827`),
  generation-gates ready outcomes, stores and calls back with `take.result`
  (`:2886-2906`), then supplies it to `postAcceptCapture` (`:3994-4009`) and
  native apply (`:3375`). `lastTakeResult` storage (`:2905`; declaration in
  `kwin/src/world-state.ts:189`) has no other read found in `kwin/src`.
  The non-actuating path still takes and discards a result (`:3986-3991`).

```ts
// kwin/src/world-report.ts:1018-1019
if (r["result"] !== null && r["result"] !== undefined) {
    throw new Error("actuator-result");
```

#### Native result readers by variant

Inventory scope: production native adapter source in
`crates/tiler-windows/src`, `crates/tiler-macos/src`, `kwin/src`, plus the
KDE broker/projection and native effect. Tests/fixtures, result definitions,
codec validation and generic transport are not counted as result-field
actuation. No additional variant readers were located by searches for
`CommandResult`, `command_result`, `takeResult`, `WorldReplyResult`, and the
five result op tags in those trees. Generic KDE take carriage above applies
to all variants, including variants without a script field reader.

| Variant | Windows production readers | KDE production readers | macOS production readers |
|---|---|---|---|
| `DragPreview` | `tiling_sys/drag_preview.rs:182-191`: reads `rect`, `hover`, `source`; only accepts matching `source_key`. | Broker `preview_suspended_by_presentation` reads result `domainOut` against presentation `suspendedOutput` (`crates/omnitiler/src/world_broker.rs:2129-2146`); `drag_preview_of` reads `rect`, `sourceOut`, `sourceWs`, `win`/target (`:2160-2183`). These become renderer `preview`, not a take-result branch. Script forwards these derived fields and producer proof (`kwin/src/world-presentation.ts:199-230`), checks live producer (`:518-523`); effect gates and paints derived preview rect (`kwin/native-effect/presentation_stage5.cpp:273-297`; `kwin/native-effect/activewindowborder.cpp:1796-1817`). No direct script `takeResult.op === "drag-preview"` reader found. | `world_runtime.rs:839-856` maps `rect`, `hover`, `target` to `PreviewOutcome::Show`. Production callers `world_dispatch.rs:373,437,483`. Earlier A6 citation to `drag_drop.rs:1738` is test-only, not another production reader. |
| `Float` | `tiling_sys/float_toggle.rs:610-615`: reads only `no_space` to select capacity-refloat behavior. | `kwin/src/world-entry.ts:3215-3226` reads win/rect-null/noSpace to record tile intent and release automatic float; `:3231-3237` handles/logs no-space; `:3326-3330` reads rect-nonnull/win to record float intent. Result rect is provenance here, not a second frame setter (`kwin/src/world-apply.ts:1052-1058`). | No production field reader found; whole reply/rows can still carry geometry. |
| `Migration` | No result-field reader found; variant-name diagnostic below only. | `kwin/src/world-apply.ts:1807-1810`: reads `targetOut`, `targetWs`, `sourceOut` as advisory refill-source provenance when target matches transfer. No read of result direction/member/float lists found in native script. | No production field reader found. |
| `Directional` | `tiling_sys/keyboard_directional.rs:326-335` destructures outcome/target/rect/cause for snap handling; second route reads outcome/cause (`:866-884`). | No direct directional result-field reader found in take consumers; common effects/desires remain separate actuation lanes. | No production field reader found. |
| `Maximize` | No result-field reader found; variant-name diagnostic below only. | `kwin/src/world-entry.ts:3402-3404` calls native handler. `kwin/src/world-native.ts:950-988,1000-1035` reads all four result fields: `win` identity/focus, `maximized` desired flag, `clearFullscreen` and `wasMaximized` source preimages, then native setters/pending target. | No production field reader found. |

- W additionally matches **every variant name only** in
  `tiling_sys/world_tick.rs:298-306`, emitted as diagnostic `command_result`
  at `:995`. This is the only located W reader for Migration/Maximize, but
  it does not read their payloads.
- KDE broker `result_of` (`crates/omnitiler/src/world_broker.rs:1932-1936`),
  settlement storage and take serialization carry every variant generically.
  The preview projector deliberately omits result hover/revision/domain
  fields from renderer preview (`:2149-2158`), while domain output separately
  participates in the suspension guard above. Native payload parsing stores
  derived rect/source/window/producer proof in
  `kwin/native-effect/presentation_consumer.h:666-714,774-780`.

Short verbatim result-consumer excerpts:

```ts
// kwin/src/world-apply.ts:1807-1810
const advisorySourceOut = env.takeResult !== null && env.takeResult.op === "migrate-workspace" &&
    env.takeResult.targetOut === t.out && env.takeResult.targetWs === t.ws
    ? env.takeResult.sourceOut
    : null;
```

```rust
// crates/tiler-macos/src/world_runtime.rs:844-854
match &reply.command_result {
    Some(CommandResult::DragPreview {
        rect,
        hover,
        target,
        ..
    }) => PreviewOutcome::Show {
        rect: *rect,
        hover_prior: hover.clone().map(Box::new),
        target: target.clone(),
    },
```

#### Citation/count clarifications and remaining gaps

- A3's `// row insertion omitted` lines are excerpt elision markers, not
  comments present in the source. The assignments themselves are verbatim;
  actual Windows source ranges remain `world_capture.rs:813-831`.
- A9's KWin list covered TypeScript only. Additional production C++ files
  over 1,500 lines, measured with `wc -l`: `kwin/native-effect/shortcutreconciler.cpp`
  4,761; `kwin/native-effect/unifiedsettings_module.cpp` 2,257;
  `kwin/native-effect/activewindowborder.cpp` 1,855. Dedicated tests and
  generated headers are excluded from this supplemental count.
- Remaining A10 gaps: nested feedback-row assignment/consumption (including
  K/M DPI/prewrite), unlisted desired-row/transfer/effect/presentation
  per-field readers, and KDE raw cursor/gesture normalization are not
  exhaustively traced. Native result readers now have the bounded inventory
  above; constructor absence is still not a capability verdict. No live
  report counts, typical-action counts or cost attribution were measured.
- Binding-value citation for the desired-row table: core publishes
  `native_binding: None` when `host_owned_workspace_domains` is false
  (`crates/tiler-core/src/world_publish.rs:1086-1090`), as in W's constructor
  (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:370`).

## Evidence: package B (platform-side policy and workarounds)

### B0. Evidence boundaries and source notation

- These are source facts and observations, not findings, recommendations, or
  approved behavior changes. F1-F11 and package A are unchanged.
- `D` means the cited source was directly read for this evidence section.
  `I` means a collected source index/excerpt, not independently reread in full.
  An indexed read/write list is not a proof that no indirect access exists.
- Paths in W tables are relative to `crates/tiler-windows/src/`, M tables to
  `crates/tiler-macos/src/`, and core tables to `crates/tiler-core/src/`.
  K paths are repository-relative. Dedicated tests and known inline test
  modules are excluded. No behavior was exercised or timing measured.
- The requested exhaustive inventories are not fully closed: remaining
  read/write, native-carrier, and alias-call coverage gaps are listed in B7.
  Declaration presence, source comments, production calls, and observed native
  behavior are different kinds of evidence; only the first three occur here.

### B1. Retained platform state

#### Windows: persistent fields, not a second layout tree

| Evidence | State and key -> contents | Declaration | Writes / reads collected |
|---|---|---|---|
| D/I | `member_rects`: token -> last visible/hidden snapshot; `float_rects`: token -> live float frame | `tiling_sys/mod.rs:412-414,758-760` | D: `world_tick.rs:1395-1419` refreshes one lane; `world_capture.rs:813-831` emits both pager fallbacks. I: member writes/seeds `admission.rs:87,207`, `restart.rs:137,180`, `world_native.rs:1632`; reads `restart.rs:84`, `world_reads.rs:191`, `world_capture.rs:1210,1368,1467`; float writes `float_toggle.rs:247,685,690`, `restart.rs:494,571`, `world_native.rs:1630`; reads `float_toggle.rs:35`, `world_reads.rs:192`, `world_capture.rs:1209,1466`; pruning `rows.rs:391-392,470`. |
| D/I | `floated`: full window identity -> intentional float lifetime | `tiling_sys/mod.rs:761-769` | I: writes/removes `float_toggle.rs:276,681-689`, `dispatch.rs:747,766`, `restart.rs:490,568`; reads D `world_capture.rs:683`; I `capacity_floats.rs:115,117`, `world_reads.rs:223`; prune `rows.rs:409`. |
| D/I | `sticky`: full identity -> pre-sticky float boolean | `tiling_sys/mod.rs:770-775` | I: inserts `restart.rs:486,565`; toggle accesses `float_toggle.rs:301,637,688`, `dispatch.rs:243,476,700,730,949,991,1059,1093`; reads D `world_capture.rs:676,699`, `world_tick.rs:1408`; I `world_native.rs:1119,1159,1223,1399,1426,1652,1703,1812,1940,2046,2532,2624`; prune `rows.rs:410`. |
| D/I | `fixed_tile_overrides`, `fixed_origin_overrides`: identity -> explicit tile win, durable fixed-origin subset | `tiling_sys/mod.rs:787-809` | I: toggle writes `float_toggle.rs:254-255,703,705`, `dispatch.rs:739-740,775,987-988,1098-1099`; restart `restart.rs:467,470-471`; reads D `world_capture.rs:698`; I `admission.rs:25,72,379,420`; pruning `rows.rs:317,406-407`, `world_tick.rs:1897,2743`. |
| D/I | `fixed_auto_seen`: identity -> adapter automatic-fixed origin; `born_first_exit`: identity -> first fullscreen-exit latch | `tiling_sys/mod.rs:810-824` | I: auto writes/uses `restart.rs:261`, `admission.rs:64,176,420`, `float_toggle.rs:256,702`; exit writes/uses `fullscreen.rs:595,676`, `admission.rs:61,161`; D capture reads `world_capture.rs:682,700`; I retirement `world_tick.rs:1898-1903,2744-2749`, `rows.rs:404-405,485`. |
| I | `born_keep_overlay`, `retile_keep_overlay`, `born_fullscreen`, `seen_nonfullscreen`, `maximize_admission_attempted`: window-keyed birth/clear state | `tiling_sys/mod.rs:525,532,539,545,550` | latch writes `restart.rs:138,287,289`; reads `restart.rs:88`, `admission.rs:92,263-264`; admission attempt guard/insert/removal `admission.rs:261,269,283`; fullscreen retirement `world_tick.rs:1906,2752`; overlay sets prune `rows.rs:402-403`. |
| D/I | `restart_reject_hold`: full identity -> observation-scoped hydration write barrier | `tiling_sys/mod.rs:776-786` | D capture reads `world_capture.rs:684`; I `restart.rs:366-583` clears/rebuilds marker-adoption state. Its declaration says rejected candidates report floating and are excluded from tile writes; full mutation inventory remains open. |
| D/I | `applied_homing`: token -> applied logical `(output, workspace)`; `applied_views`: output -> applied logical view | `tiling_sys/mod.rs:401-407` | I homing write/use `world_tick.rs:109,1912,1936`, `world_native_receipts.rs:54,272`; read `output_sync.rs:398`, `directional_resolution.rs:95`; views write/use `world_tick.rs:1915,2761`. These declarations explicitly source values from reply rows/outputs. |
| D/I | `member_tokens`, `member_identity`, `member_tags`: full native identity -> world token, process identity, lifetime property tag | `tiling_sys/mod.rs:408-429` | I admission writes `restart.rs:124-137,229,436`; tag accesses `assignments.rs:66,137,220,298,329`, `rows.rs:60,107,139,413,531,600,617`, `world_diff.rs:176,252`, `world_native_receipts.rs:97,257`; D `world_reads.rs:113-117` joins token to core homing. Core receives opaque tokens, not these native identity/property structures. |
| D/I | `hidden_claims`: full identity -> hide-ledger record/nonce | `tiling_sys/mod.rs:415-420` | I write/read/remove `world_native.rs:1301,1332,1366,1371`, `world_native_receipts.rs:59,69,177`; reads `assignments.rs:266,366,460`, `world_capture.rs:1117,1164,1317`; D `world_reads.rs:122` adds hidden status to core-backed home. Native hide claims are not core logical homing. |
| D/I | `output_tokens`: device -> opaque output ID; `overlay_now`: token -> `(maximized, fullscreen)`; `float_topmost_prev`: identity -> prior topmost band | `tiling_sys/mod.rs:397-400,754-757,825-831` | D output reads `world_reads.rs:46-55`; I overlay refresh `observation.rs:375`, reads `admission.rs:382`, `world_reads.rs:195`; complete topmost-band use index not collected. Core receives mapped output/overlay facts, not native device keys/band restoration memory. |
| I | `stable`, `managed`, `active`; gesture preimage/end-cursor/start-key/start-tag maps | `tiling_sys/mod.rs:286-315` | HWND-keyed stable frames, managed/gesture state; exact per-field mutation inventory not collected. |
| I | `last_hwnds`, `minimized_omitted`, `pending_closed`; hint last-good/fallback caches | `tiling_sys/mod.rs:559-599` | inventory/omission/close and size-hint retention. Exact read/write closure not collected. |

Verbatim D excerpts establish what the frame and float caches actually do:

```rust
// crates/tiler-windows/src/tiling_sys/world_tick.rs:1408-1419
let float_lane = state.sticky.contains_key(&key)
    || state.fixed_auto_seen.contains(&key)
    || state.core_is_float_token(&bound)
    || state.born_fullscreen.contains(&key);
if float_lane {
    state.float_rects.insert(bound.clone(), window.visible);
    if state.fixed_auto_seen.contains(&key) || state.core_is_automatic_token(&bound) {
        state.member_rects.remove(&bound);
    }
    continue;
}
state.member_rects.insert(bound, window.visible);
```

```rust
// crates/tiler-windows/src/tiling_sys/world_capture.rs:676-687
let sticky = state.sticky.contains_key(key);
let (fixed_auto, fixed_suppress) = capture_fixed(state, key);
crate::native_guards::float_lane(
    sticky,
    fixed_auto,
    fixed_suppress,
    state.born_first_exit.contains(key),
    state.floated.contains(key),
    state.restart_reject_hold.contains(key),
    state.born_fullscreen.contains(key),
    hidden,
)
```

- D: core also retains `frames`, `retained_floats`, learned minima,
  birth-lifetime sets, mapping/views/members and presentation frame maps
  (`world_engine.rs:795-808,816-845`). This establishes overlapping categories,
  not equal values: native last-observed frames are not core allocations.
- D: the Windows `floated` declaration explicitly says the adapter carries an
  exception across domain release and reports it back for re-adoption
  (`tiling_sys/mod.rs:761-768`): `row assembly rides these tokens floating even
  when the session carries no exception`.

Windows operation/focus state is also retained outside the engine:

| Evidence | State | Write/read and retirement evidence |
|---|---|---|
| I | `WorldSequencer.pending_frames` by token, members by `(op,window)`, views by op, clears by `(op,window)` | declarations `tiling_sys/world_sequencer.rs:182-194`; staging/report rows `:254-285,338-347`; exact acknowledgement/pruning of every lane not recollected here. |
| D/I | `EffectTracker.attempted_members`, `spent_effects`, `spent_floats`, `spent_clears`, `spent_views`, `spent_reveals` | D declarations `tiling_sys/world_diff.rs:1060-1087`, contains/inserts `:1108-1157`, reset `:1178-1185`; I reply-operation pruning `:1380-1415`; member consumers `:2837,2895,3542,3598`, float `:3057`, clear `:3159,3274`, view `:2320,2330,3364,3448,3666,3676,3729,3744,4350,4361`, reveal `:3410,3424`. |
| I | `HostDiff.in_flight`: exact focus token/native identity/tag/effect/time; foreground event queue and metadata; in-flight diagnostic stash | fields `tiling_sys/world_diff.rs:1200-1252`; arm `:4208`; clear on reset `:1343`, lifetime end `:2547`, landing `:2598`, foreign supersession `:2618`; foreground enqueue `:1280,1317,1325`, drain `:2580-2582`. |

- D: `EffectTracker` calls itself `At-most-once dispatch fences` and explicitly
  says core owns completion/refusal/ambiguity (`world_diff.rs:1053-1058`).
  Its values are local dispatch history, whereas core's retained operations
  and consumed-frame state are declared at `world_engine.rs:795-806`.
- I: per-apply `LiveNative` also holds joins named `core_homing`, `core_rings`,
  `core_views`, `desired_frames`, `proven_homing`, `proven_views`, `token_key`
  (`tiling_sys/world_native.rs:511-636`). These are constructor-derived joins
  and borrowed caches, not evidence of an independently updated layout tree.

#### macOS: observation roster, reply lookup, applied geometry and operation state

| Evidence | State/key -> value | Write / read evidence | Core counterpart actually established |
|---|---|---|---|
| D/I | `RetainedCore.apps` by PID; `RetainedApp.roster` by WID -> geometry/Space IDs/fullscreen/minimized/hidden/exclusion facts; derived window list/focused WID | D declarations `runtime_core.rs:97-131,149-155`; I roster update `:836-865`, prune `:751-823`, preservation/rehoming reads `:921-1092`; native relaunch clear `runtime_native.rs:255-275` | `WindowFact` receives a projection, not the native roster or exclusion metadata. |
| D/I | `window_domains`: runtime ID -> native domain; `known_domains`: domain -> authoritative bounds | D declarations `runtime_core.rs:156-168`; D known-domain refresh `:1129-1149`; I homing refresh `:889-898,1048-1062`, reads `world_command_map.rs:46`, `world_dispatch.rs:1553`, `runtime_thread.rs:1093,1678` | Core retains logical mapping/members; this cache's workspace is native Space vocabulary. |
| D/I | `minimized`: runtime ID -> window/output/native workspace/sticky record | D `runtime_core.rs:133-145,169-173`; I update/prune `world_runtime.rs:492-516,534-540`, read sorted facts `:394-396` | Explicit minimized fact is sent to core; cached native home is not a logical binding authority. |
| D | `LifetimeMemory.rows`: lifetime -> last applied `WindowDesire` | `runtime_core.rs:251-273` rebuilds from reply; `world_runtime.rs:355-360` sources sequencer's last reply; `runtime_core.rs:295-306` reads for float echo | Exact reply copy, not an independently observed float flag. |
| D/I | `SequencerState.last_reply`, `spent_effects`, `spent_commands` | D declarations `world_sequencer.rs:118-128`; complete per-method read/write index not collected | Reply snapshot plus adapter delivery deduplication, separate from core command highwater/retained effects. |
| D/I | Pending views by op, members by `(op,window)`, creations by op, frames by window; pending writes by `(pid,cmd_id)` -> `(window,rect,placement)`; spent frame operations; pending closes | declarations `world_runtime.rs:92-118`; I inserts `:221,266-267,312-313,908-911,951`, write-result remove `:942,945,980`; D drain `:327-349`, spend `:386-389`, report drain `:601-605` | Core retains transfers/frame operations; these maps carry undelivered attempts and command/write correlation. |
| D/I | `Shared.last_desired`, `last_geometry_key`, `last_presentation`, pager binding and workspace-desire snapshots | D `runtime_shared.rs:394-422`; D retention read/write `world_dispatch.rs:1419-1436`; I desired write `:1583-1589`; consumers `runtime.rs:1659-1660,1813-1814,1835-1836` | Presentation/workspace rows are copied from core; retained desired geometry may also include prior held rows (excerpt below). |
| D/I | `AppliedGeometryKey`: current scope set, binding vector, lifetime -> logical home, scope -> `(workarea,bounds)` | D `runtime_shared.rs:221-233`; key assembly and comparison `world_dispatch.rs:1158-1191` | Key joins current native geometry to reply logical state. |
| D/I | One latest-wins `selection_focus_pending` with op/command/window/route/expected SID/app generation/prior focus/prior frontmost; supersession floor | D `runtime_shared.rs:182-205,333-345`; I bind `world_dispatch.rs:1755-1769`, take on matching completion `:1807-1813`, drop `:1666-1672,1930-1937` | Adapter retains asynchronous native selection-to-focus correlation after core chooses target. |
| I | `Shared.apps` by PID -> per-app latest frame/focus slots | `runtime_shared.rs:32-48,245`; frame slots publish `world_dispatch.rs:1564-1580`; app lifecycle `runtime_thread.rs:1344-1390,1504-1521` | Pending native output commands, not logical layout. Exact focus-slot read/write closure remains open. |
| I | `BorderFactsStore` PID map; `EligibilityFactsStore` PID map and current Space list | `border_runtime.rs:87-119`, publish/remove `:140-155`; `input_eligibility.rs:211-251`, publish `:270-276` | Native focus/safety evidence, not all roster metadata crosses world report. Full readers not collected. |
| I | `SelectionState.spent_creations`, pending creation map by op, last-created/proof options; `TrailingLifecycle.baseline` Space set and owned Space vector | `workspace_select.rs:229-261,492-496`; spent write/read `:305-318`, pending insert/drain `:326-330,367-370`, baseline collect/adopt `:501-520` | Creation operations are core desires; native append proof and spent-side-effect memory are adapter state. |

```rust
// crates/tiler-macos/src/runtime_core.rs:86-95
pub(crate) fn normalize_wire_flags(
    floating_cls: bool,
    fullscreen: bool,
    known: &KnownState,
) -> (bool, bool) {
    if known.exception && !fullscreen && !floating_cls && known.placed {
        return (true, false);
    }
    (floating_cls, false)
}
```

- D observation: `KnownState::of_reply` sets `exception: row.floating` and
  `placed: row.floating && row.effective_frame.is_some()`
  (`runtime_core.rs:52-62`). The comment says macOS has no AX-level float state
  (`:75-82`). Core intent therefore changes the next reported `floating` fact.
- D observation: a same-key held reply can publish prior desired geometry even
  when fresh desired rows omit it. `retain_stable_geometry` excludes minimized
  rows and noncurrent scopes, then executes `merged.push(prior.clone())`
  (`world_dispatch.rs:1185-1219`). Native write emission is separately derived
  only from current `Controlled` rows with an effective frame (`:1356-1379`);
  carried presentation geometry is not evidence of an extra native write.

#### KDE: script state, persistence and native-effect fact caches

| Evidence | State/key -> value | Write/read/retirement evidence | Core counterpart |
|---|---|---|---|
| D/I | `WorldAdapterState.dispatchedOps`, `spentEffects`, `floatSpent`, `clearSpent`, `creationSpent`: op/effect IDs | D `kwin/src/world-state.ts:131-161`; I mark/read `:225-232`; generation reset `:769-806` | Local at-most-once dispatch history; core retains operation/effect identity and settlement. |
| D/I | `appliedWindowTarget`: window -> `{op,out,ws}`; `appliedViewTarget`: output -> `{op,ws}` | D declarations `world-state.ts:149-157`; I writes `:276-282`; I reads `world-apply.ts:843-846,1148-1153` | Proven local native targets used to veto same-op re-actuation after external departure. |
| D/I | Frame/member/view/clear/creation attempts and pending creation readbacks | D `world-state.ts:162-183`; I record/validate `:310-329,413-533`, report rows `:535-567`, exact-flight acknowledgement `:575-635`, reset `:769-806` | Facts returned to core; pending creation is executed-but-unproven native evidence. |
| D/I | `lastReply`, `lastTakeResult`; pending closes and observation-only catch-up | D `world-state.ts:125-130,184-189` | Reply/property-diff snapshot and terminal requester result; exact lastReply assignment index not recollected. |
| D/I | `floatingIds`, `tileIds`: live internal ID sets, disjoint intent/override membership | I declarations `kwin/src/world-entry.ts:748-756`; hydrate `:1362-1399`, persist `:1410-1421`; D accepted-reply additions/prune/refloat `:3286-3330`; source search persistence call `:3445`; I observation inputs `:951-952`, toggle wiring `:4783-4793` | Float/explicit tile intent is reported back to core, whose reply also supplies the classifications used to update sets. |
| D/I | `logicalOfNative`: `(output,native ID)` -> `{logical,out}` | I declaration `world-entry.ts:447-455`; source search set `:3092`, reads `:920,3149-3155,4744`, D prune `:3256-3266` | Retained native/logical binding snapshot. Reply rows are tried before the cached fallback (`:3143-3155`). |
| D/I | Runtime float-intent store: bus/KWin-owner namespace plus float/tile member lists | D scope `crates/omnitiler/src/float_intent_store.rs:1-9,30-34`; I schema `:384-392`, read `:196-231`, write `:271-331`; service reads `planner_service.rs:814-831`, writes `:972-1007` | Persists intent membership only; explicitly no rects/focus/stacking/history/native writes. No persistent core layout image established. |
| I | C++ `m_maximizedWindows`, maximize/group-visibility subscriptions, gesture attachment set and window -> gesture ID hash | `kwin/native-effect/activewindowborder.h:243-246,268-270`; writes/reads `activewindowborder.cpp:649-743,920-964,997-1053` | Native overlay/subscription/gesture facts; not a core logical layout map. |

- D observation: `floatingIds.add(row.win)` is conditional on core reply
  `row.floating === true && row.frame_auth === "untouched"` and a live observed
  ID (`kwin/src/world-entry.ts:3286-3289`). Absence only prunes intent under a
  complete/readable/no-hold inventory (`:3302-3318`).
- I: `windowHomingDetaches` is native object -> detach callbacks
  (`world-entry.ts:4486`); trace source/window maps at `:463-499` are diagnostic
  correlation, not layout. Native-effect fields beyond the cited header scan
  remain a coverage gap, not evidence of absence.

### B2. Adapter decisions and core-library call classification

- Classification below is mechanical: **helper** returns geometry, formatting,
  parsing/validation or direct lookup; **decision** returns membership,
  eligibility, refusal, admission/hold, trigger or behavioral targeting.
  A pure/deterministic function can still be a decision. These labels do not
  decide which component should own it. World-engine method calls are separated
  from free functions below.
- D core definitions distinguish these returns: `window_policy.rs:172-174`
  focus classification, `:194-236` membership, `:270-320` refusal; `visual.rs:
  213-256` border eligibility. Thus membership and border eligibility are not
  geometry-only utilities even when called in process.

#### Windows free-function call inventory (I, except D core definitions above)

| Class | Function(s) | Production call sites |
|---|---|---|
| decision | `window_policy::classify_geometry`, `classify_focus` | `tiling/classification.rs:195`, `tiling/float.rs:268` |
| decision | `window_policy::sticky_toggle_refusal`, `float_toggle_refusal`, `orientation_toggle_refusal` | `tiling_sys/chord_resolution.rs:153,165`; `:158,167`; `:186,191` |
| decision | `window_policy::overlay_refusal`, `should_clear_maximize_at_admission_with_mode` | `tiling_sys/admission.rs:323,257` |
| decision | `window_policy::should_hold_born_fullscreen`, `should_admit_slotless_maximized`, `born_keep_latch` | `tiling_sys/restart.rs:85,176,283`; hold also `tiling_sys/settings.rs:199` |
| decision | `size_hints::is_fixed_size_with` | `tiling_sys/admission.rs:106`, `tiling_sys/minmax.rs:183` |
| decision | `visual::border_eligibility` | `tiling_sys/focus.rs:273` |
| decision | `visual::group_underlay_trigger`, `group_underlay_chord_held` | `tiling_sys/group_underlay.rs:212,194`; chord also `input_pump/native/mod.rs:653`, `tiling_sys/run_loop.rs:277`, `run_loop_tick.rs:385`, `world_capture.rs:532` |
| decision | `output_selection::output_for_rect` alias `core_output_for_rect`, `cursor_targeting::output_at_cursor` | `tiling_sys/directional_resolution.rs:22,33` |
| helper | `geometry::inset_bounds`, `bounds::rect_contained`, `bounds::valid_carried_rect` | `tiling/geometry.rs:34`, `tiling/workspace.rs:977`; valid rect `tiling_sys/minmax.rs:171,312`, `observe.rs:1301` |
| helper | `visual::group_underlay_effective_extension` | `tiling_sys/group_underlay.rs:313`, `world_capture.rs:548` |
| helper | `visual::active_border_use_theme`, `world_presentation::pager_workspace_label` | `active_border.rs:617`, `tray.rs:302` (theme/color rendering choice, not membership/focus/layout) |
| helper | `gesture::preview_prior_desc`, `workspace_scope::mode_token` | preview `tiling_sys/drag_preview.rs:210`, `gesture_tick.rs:500`; mode `settings/schema.rs:374`, `tiling_sys/run_loop.rs:323`, `settings.rs:463-464,704` |
| helper | `settings_catalog::find_field`, `find_section`, `validate_field_value`, `validate_text` | `settings/windows_catalog.rs:386`; `settings_ui/tabs.rs:511`; value validation `windows_catalog.rs:467,470,473`; text `:475` |

- D correction to the collected call index: `settings/windows_catalog.rs:850`
  (`validate_text`) and `:909` (`validate_field_value`) are inside the test
  module beginning at `:553-554`, not production calls.
- D verbatim eligibility normalization (`tiling/classification.rs:194-200`):

```rust
pub fn classify(facts: &WindowFacts) -> Result<(), SkipReason> {
    tiler_core::window_policy::classify_geometry(
        &safety_facts_of(facts),
        facts.captionless_fullscreen,
        facts.maximized,
    )
```

Additional adapter branches, not established merely by a `tiler_core::` grep:

| Evidence | Decision sequence | Source |
|---|---|---|
| D | Birth preamble admits fullscreen, admits slotless maximized, adopts markers, clears first maximize before report | `tiling_sys/world_tick.rs:1435-1439,1464-1467` |
| I | Sticky/restart adoption checks stored marker/identity/scope/lifetime, groups valid members by output/workspace, inserts sticky/floated and float frame; next report carries floating | `tiling_sys/restart.rs:366-583`; inserts `:486-494,565-571`; preamble caller above. No per-domain engine commit at this site according to `:550-553`. |
| D | Unmanageable fullscreen foreground gets narrow observation lane, no cursor sample, presentation paused | `tiling_sys/reconcile.rs:172-219`; literal call tail is `None, true` at `:217-218`. A zero foreground returns without reporting (`:188-189`). |
| D | Windows may decline an already accepted staged world reply and preserve committed core world | `tiling_sys/world_sequencer.rs:844-889,897-943`; exact excerpt below. |

```rust
// crates/tiler-windows/src/tiling_sys/world_sequencer.rs:878-889
let (staged, candidate) = self.deliver_staged_candidate(report);
let declined = matches!(
    &staged,
    WorldOutcome::Accepted(reply) if Self::staged_declines(&mover, report, reply)
);
if declined {
    return WorldOutcome::Rejected {
        kind: "float-no-remap",
        message: "cross-output capacity float without remap",
    };
}
self.adopt_or_deliver((staged, candidate))
```

- D observation: this is adapter-created `WorldOutcome::Rejected`, not a
  rejection returned by core. `deliver_staged_candidate` clones the engine and
  calls `candidate.handle_report(report)` (`:848-850`); `staged_declines` joins
  report-time nonfloating source, command operation, cross-output transfer and
  reply classification (`:921-943`). This adds a second staging clone on those
  Windows command routes, beyond the core clone recorded in F6.

#### macOS and KDE free-function calls and local decisions

| Evidence | Class | Call / branch and concrete input/output | Site |
|---|---|---|---|
| I/D | decision | M `classify_membership` with `visible: true`, default other safety flags, native sheet/dialog/modal reason; returns FloatMember/Tile/LeaveUnmanaged | `classifier.rs:361-389`; core definition D `window_policy.rs:194-202` |
| I | helper | M `active_border_inner_rect`; `group_underlay_effective_extension`, `group_underlay_outer_rect`; theme/color helper `active_border_use_theme` | `active_border.rs:345`; `group_underlay.rs:115-116`; `active_border.rs:250`, `pager_highlight.rs:63` |
| I | decision | M `active_border_state` and `active_border_is_maximized` (overlay OR then border visible state) | `active_border.rs:334-344` |
| D | decision | M local retained-intent float normalization | `runtime_core.rs:86-95,295-306` (B1 excerpt) |
| D | decision | M presentation-only zoom inference from retained frame, visible/full display geometry and `engine_placed` exemption | `world_builder.rs:212-246`; excerpt below |
| D | decision | M preserves prior same-state held desired geometry, separately from controlled native frame emission | `world_dispatch.rs:1149-1219,1356-1379` |
| D | decision | M `FocusStep`/`MoveStep` choose inner Operation and disable cross-output transfer; resize tracks repeat by focused window/direction/mode | `world_command_map.rs:240-275` |
| D | decision | K raw `classify_retained_membership` with hidden/minimized safety and native transient/dialog/modal/owner; `LeaveUnmanaged` drops row | `crates/omnitiler/src/kde_raw.rs:425-441` |
| D | decision | K `is_fixed_size_with` uses raw client hints, not decoration-adjusted frame hints; explicit tile suppresses automatic/transient float but explicit float wins | `kde_raw.rs:451-492` |
| I | helper | K opaque-ID/rect validation, fixed predicate parsing | `kde_raw.rs:95-101,142-146,269-274`; `crates/omnitiler/src/kde_gesture.rs:44`; `world_broker.rs:2054` |
| I | decision | K pre-setter and between-step freshness/lifetime/gesture/membership/native overlay guards | `kwin/src/world-apply.ts:155-249` |
| I | decision | K same-operation departure from applied target, third-location homing, or held/dispatched transfer withholds native write | `world-apply.ts:834-859,1120-1153` |
| D | decision | K automatic focus preserves a different observed focused float/sticky window; allows only ordinary reply-classified admission floats through overlay exception; blocks newcomer focus behind fullscreen/maximized sibling | `world-apply.ts:2294-2377` |
| D | decision | K intent withdrawal/addition/prune depends on reply authority, live overlay reads, complete/readable inventory and terminal toggle result | `world-entry.ts:3157-3220,3286-3330` |

```rust
// crates/tiler-macos/src/world_builder.rs:243-246
let (zoom_h, zoom_v) =
    crate::border_runtime::zoom_axes(visual, &visibles, eligibility.engine_placed);
let full_display = crate::border_runtime::covers_full_display(visual, &fulls);
(zoom_h || full_display) || (zoom_v || full_display)
```

- D observation: M sends that inferred result as presentation
  `foreground_maximized`, while window facts explicitly use `maximized: false`
  (`runtime_core.rs:307-317`). The two fields have different producers, not one
  common observed zoom fact.

```typescript
// kwin/src/world-apply.ts:2301-2304
const focusedId = observation.focusedGlobal;
if (typeof focusedId === "string" && focusedId !== "" && focusedId !== fx.target && fx.id.startsWith("af-")) {
    const focused = byWin.get(focusedId);
    if (focused !== undefined && (focused.floating === true || focused.sticky === true)) {
```

- D: these are actual apply-side conditions in addition to the core reply,
  not just comments documenting shared core policy. No equivalence verdict
  between the adapter predicates and core predicates is claimed.

### B3. Engine profiles and command opt-ins

| Option | Core construction / gated branch (D) | Windows | KDE | macOS |
|---|---|---|---|---|
| `fixed_send_admission` | `WorldEngineConfig:742`, default false `:753`; constructor sets inner Engine `:781`; propagates to sessions `engine.rs:666-669,820-821`. `session/world.rs:711-734` permits automatic fixed-origin float carries when on, still requiring no `fixed_suppress`. | true (`tiling_sys/world_sequencer.rs:83-88`) | default false (`crates/omnitiler/src/world_broker.rs:450`) | default false (`world_runtime.rs:140-141`, I constructor index) |
| `maximized_focus_fence` | config `world_engine.rs:745`, default true `:754`; sets Engine `:780`; session local focus fence `session/ops/focus.rs:100-108` and cross-output fence `:492-500` return `RefusalKind::Unchanged` for maximized, nonfullscreen subject | false (`world_sequencer.rs:86`) | default true | default true |
| `with_focused_fullscreen_presentation_suspension()` | only `WorldEngine` `with_*` builder located; flag starts false `world_engine.rs:848`, sets true `:859-862`; `world_presentation.rs:779-790` returns focused staged fullscreen fact's output, otherwise None; output feeds border/underlay/pager suspension `:800-808` | not enabled | enabled `world_broker.rs:450-452` | not enabled |
| `fixed_size_admission` | not an exposed config option: always `engine.set_fixed_size_admission(true)` at `world_engine.rs:779` | on | on | on |

```rust
// crates/tiler-core/src/session/ops/focus.rs:100-108
if self.maximized_focus_fence
    && let Some(entry) = session_observation
        .windows
        .iter()
        .find(|w| w.window == focused_window)
    && entry.maximized
    && !entry.fullscreen
{
    return Err(ProposeError::Refused(RefusalKind::Unchanged));
```

- D: `cross_output_transfer` is a command/capability field, not a
  `WorldEngineConfig` field. `world_operations.rs:87-116` routes
  `Operation(CoreCommand::Move/Focus)` through `operation_directional` only when
  this flag is true. M's `FocusStep` and `MoveStep` explicitly set it false
  (`world_command_map.rs:240-255`); W/K dedicated directional commands are a
  different entry route (F2 already records their constructors).
- D: inner `engine.rs:4062-4067` falls back to local move when no two-domain
  directional pair exists; for an assembled pair it sets
  `capabilities.cross_output_transfer = *cross_output_transfer` (`:4114-4115`).
- I: no `FocusStep` type was found in core. D: it is M's
  `InputSemanticAction::FocusStep`, translated to `CoreCommand::Focus` as above.
  It is not another engine builder or construction profile.

### B4. Rejected-command observations and recovery

| Platform | Source-observed command/observation disposition |
|---|---|
| W (D) | `tiling_sys/world_tick.rs:2352-2410`: logs rejected outcome, calls `note_rejected_or_lost`, synchronously calls observation-only catch-up with the same tick products. Superseded mode calls core-only observation; full mode calls native-applying observation. Command remains dropped. Observation-only rejection handlers separately return None/false (`:1589-1592,2090-2093`, I). |
| K (D) | `crates/omnitiler/src/world_broker.rs:977-998`: terminally settles command failed, then `dispatch_plain_fallback(plain_stamped)`, whose `run_engine` decodes and calls `handle_report` again (`:1008-1022`). The fallback's own rejection remains a rejection; it does not change the already-settled queue command. |
| M (D) | `world_dispatch.rs:742-783`: clears flight, logs refusal (with send detail for send commands), returns `DispatchVerdict::Refused`; no observation-only call in that branch. Gesture path likewise clears/refuses (`:487-499`). Direct native selection path `workspace_select.rs:1817-1822` clears flight and returns failure, no fallback. Later independent observations are possible; no same-report fallback was found here. |

```rust
// crates/tiler-windows/src/tiling_sys/world_tick.rs:2377-2385
world.note_rejected_or_lost(&report.request_id);
// The rejection just armed observation-only catch-up; service
// it synchronously on this turn's tick products instead of
// stranding the next command behind an unrelated wake. The full
// leg applies it natively; the superseded leg stays native-free
// (core-only). Either way the rejected command stays dropped
// (never queued, never replayed).
if mode == CommandApplyMode::Superseded {
    run_observation_core_only(
```

```rust
// crates/omnitiler/src/world_broker.rs:977-982
WorldOutcome::Rejected { kind, message } => {
    self.pop_settle_failed(queued, kind, message)?;
    // Fallback observation-only dispatch: any preview in its
    // reply is session-produced, never the settled command's.
    self.dispatch_plain_fallback(plain_stamped)
        .map(|outcome| (outcome, false))
```

```rust
// crates/tiler-macos/src/world_dispatch.rs:780-783
DispatchVerdict::Refused {
    token: format!("rejected-{kind}"),
    select_operation: None,
}
```

- D observation (out of brief): M drains attempt maps via `mem::take`
  (`world_runtime.rs:327-349,601-604`) before dispatch. The inspected
  `WorldOutcome::Rejected` branches do not reinsert those attempt rows. The
  no-native-context path deliberately guards before draining (`:592-599`).
  End-to-end survival/reconstruction of rejected-report attempts is unverified;
  this is not a tested data-loss claim.

### B5. Inner contract use and non-world path evidence

| Surface | Production evidence | What this establishes |
|---|---|---|
| `boundary::CoreCommand` | D M `world_command_map.rs:240-289` constructs Focus/Move/Resize/Toggle commands; `runtime_thread.rs:1702` wraps Operation (I). D W `world_sequencer.rs:902-910` matches SendToOutput/cross-output Move. | Live executable command vocabulary, not mere type imports; it travels inside the world report rather than proving a second transport. |
| `contract` | D codec `crates/tiler-protocol/src/world_protocol.rs:1031-1034` decodes DragSide edge values. I M `native_input.rs:138,150` constructs ResizeMode variants; `runtime_types.rs:609` carries that type. | Public enum vocabulary is still used. No direct platform call into inner contract reconciliation was located. |
| `session` | D codec constructs `DragHoverPrior` with source leaf/window/revision/prior edge/domain (`world_protocol.rs:1049-1062`), plus source DomainKey (`:1065-1073`). I W/M drag APIs carry DragHoverPrior. | More than Rect/DomainKey aliases cross the surface: inner drag advisory state remains encoded in the world command codec. No direct production platform `Session::propose/commit` call found. |
| `reconcile` | Indexed searches found no production platform call to `tiler_core::reconcile` | Bounded negative search, not proof that no reexport/alias exists. Adapter modules named reconcile are not this core API. |
| `cosmic_v1` | I W `tiling/workspace.rs:414`, M `runtime_types.rs:601` are documentation references to `keyboard_step_px`, not calls | No production invocation found in W/M/K search. Resize commands carry press index; documentation reference must not be counted as a live invocation. |
| Codec `decode_core_command` | D `world_protocol.rs:1224-1387` accepts reconcile, update-gaps, release-domain, move, focus, resize, pointer-resize, toggles, sends, migration, drag-drop/preview | The inner command vocabulary remains a production wire-decoding surface. Reconcile/update-gaps/release-domain acceptance is actual code, not inferred from imports. |
| Windows `tiling/` | D `tiling/mod.rs:2-10,17-74` is described as `Tiling-only Windows helpers over the world route` and exports normalization, CLI, native marker/restore, geometry, resize-repeat helpers; D classifier call shown in B2; D active tick calls `world.deliver_production` (`world_tick.rs:2352`) | Directory is production, but its name is not evidence of a second legacy layout engine. No second production inner `Engine::handle` call found in the searched Windows paths. |
| KDE entry | I `kwin/src/entry.ts:5-15,27` calls `startWorldAdapterEntry` and documents legacy plan removal; Rust broker engine calls are D in B4 | No production legacy planner call found under searched K script/broker paths. |
| macOS entry | D dispatch and native-selection paths call `WorldEngine::handle_report`; I `world_runtime.rs:3-13` documents retired per-domain Engine.handle aggregation | No production legacy per-domain Engine/Session invocation established; a directory/runtime type named Engine alone is not evidence of core `Engine`. |

- D observation (out of brief): W also uses read-only methods on the retained
  engine outside report/reply: `logical_view`, `logical_ring`, `logical_tiling`,
  `adapter_homing`, `logical_members` (`tiling_sys/world_reads.rs:58-136`).
  Example: `self.world.engine().adapter_homing(...)` resolves a native member's
  home; it is not reconstructed solely from a received `WorldReply`.
- I: M looks up `retained_transfers()` while forming attempt correlation
  (`world_runtime.rs:199-202,241-245`, `world_dispatch.rs:1699,2194`).
  These are live world-engine library queries, distinct from the older inner
  session contract and from pure geometry utilities.
- D observation: W `world_owner.rs:2-12` still says `standby: unwired` and
  `legacy Engine path remains`, whereas the inspected production call at
  `world_tick.rs:2352` dispatches through the world owner. The header cannot
  establish that the legacy path is still active.

### B6. Platform vocabulary in core production API/docs

- D source scan used case-sensitive platform terms and cloak variants across
  `crates/tiler-core/src/*.rs` and submodules. The table keeps public/member
  declarations and module/function/type documentation; implementation comments
  are separated below. Generic plural `Windows` meaning application windows
  (for example `world_model.rs:925,1344`, `boundary.rs:767`, `engine.rs:195`)
  is not counted as a platform name. Known inline-test hits are excluded.

| Core path:line | Short source excerpt / surface |
|---|---|
| `world_report.rs:499` | `Windows keeps the logical route with both fields None` (member attempt docs) |
| `world_report.rs:540` | `Windows keeps the` logical route (view attempt docs) |
| `world_report.rs:614-615` | `physical pixels on Windows`, `KWin logical pixels on KDE` (presentation units) |
| `world_report.rs:638` | `pub foreground_cloaked: bool` |
| `world_report.rs:696` | `KWin desktop id, macOS Space id, opaque platform` (native workspace docs) |
| `world_report.rs:757,761` | `KWin global ids`; `a fullscreen Space is showing` (native observation docs) |
| `world_report.rs:948` | `the KDE focus policy allows crossing` (directional focus command docs) |
| `world_engine.rs:740-744` | `Windows workspace sends`, `Windows carrier enables it`, `Windows carrier disables it` (public config fields) |
| `world_engine.rs:853` | `KWin only offers truthful fullScreen, never game identity` (public opt-in builder docs; source uses backticks around fullScreen) |
| `boundary.rs:51` | `adapters constructing events directly (Windows) pass the default` |
| `window_policy.rs:41,49` | `Windows WindowFacts mapping`; `pub cloaked: bool` (`SafetyFacts`) |
| `window_policy.rs:84,92` | `Windows reports privileged as`; enum variant `Cloaked` (`EligibilityRefusal`) |
| `window_policy.rs:240,251,267,310` | `matches Windows classify`; `Windows classify_focus`; `Same token vocabulary as Windows overlay_refusal`; `KDE gates the same way` (eligibility/refusal docs; names are backticked in source) |
| `visual.rs:147,181` | `Cloaked` (`BorderHideReason`); `pub cloaked: bool` (border eligibility facts) |
| `visual.rs:186,212,261` | `Windows PMv2 physical`; `Windows maps its zoom axis here`; `the KDE BorderOutline ring` (border docs) |
| `size_hints.rs:38,151` | `KWin unbounded sentinel` (module/public hint normalization docs) |
| `settings_catalog.rs:181-182` | `exposed on Windows and KDE`, `not on macOS` (portable settings capability docs) |
| `output_selection.rs:12,315` | `Strict map (KDE world parity)`; `FULL rectangles (KDE world` |
| `workspace_scope.rs:14` | `mirrors the KDE WorkspaceNativeAdapter seams` (module docs) |
| `gesture.rs:12` | `Pure decisions moved here from the Windows adapter` (module docs) |
| `active_group.rs:9` | `no Qt/KWin types` (module docs) |
| `world_presentation.rs:3,6` | `for example a KDE D-Bus`; `Windows consumes only` (module docs) |
| `world_directional.rs:6,169,229,282,343,365` | `The Windows`; `KDE selectFloatFocusTarget caller parity`; `KDE` float focus/snap/target-validity/constraint token parity docs |
| `world_born.rs:112` | `KWin lifecycle normalization` |
| `engine.rs:101,105,108-110,663,1455,2295` | Windows carrier defaults/send opt-in docs; `KWin fresh-admit naming`; `requiring KWin to derive an admit` |
| `session.rs:390,769,779,784-786` | `No KWin execution fields`; Windows vs Linux policy opt-in docs, including `KDE/macOS carriers` |
| `session/world.rs:667,808,823,837-838` | `Windows automatic fixed-size carry`; Windows/Linux fixed-admission/focus opt-ins; `KDE/macOS` send defaults |
| `session/ops/resize.rs:407` | `COSMIC semantics live in ... cosmic_v1. KWin/JS share math is` |

- D: cloak-related public names include both `EligibilityRefusal::Cloaked` and
  `BorderHideReason::Cloaked`; the corresponding formatting token is `cloaked`
  (`window_policy.rs:110`, `visual.rs:163`). No public field/type/variant named
  KDE/KWin/Windows/Win32/DWM/macOS/AX/Plasma was found by this token scan; names
  inside documentation are much more common than platform-named declarations.
- D supplementary implementation/private-doc hits: `world_engine.rs:726-729`,
  `world_operations.rs:31,353,492,495`, `world_native.rs:90`,
  `engine.rs:2423,3315,3554`, `session/world.rs:706`,
  `session/ops/focus.rs:99`, `session/ops/toggle_orientation.rs:105`,
  `size_hints.rs:273`. These use platform names in comments; they are not new
  public platform-tagged enum variants. No non-test `Win32`, `DWM`, standalone
  `AX`, or `Plasma` hit was returned by the scan. `Space` hits above refer to
  native macOS workspace terminology, not generic geometry space.

### B7. Explicit unverified gaps

- B1 is a substantial retained-state inventory, not every field's full
  def/use graph. W stable/managed/gesture/hint/native-band state, M focus-slot
  delivery and border/eligibility stores, K helper-owned state and C++ native
  effect fields need complete reader/writer closure before calling it exhaustive.
- W `LiveNative` maps are per-apply joins/borrows; their full ownership and all
  mutations were not individually traced. Native marker/ledger payloads and
  all visibility helper caches were not inventoried field-by-field.
- M workspace-send state also uses non-map operation structs
  (`workspace_send_effect.rs:616,721-727,808-862,1052,1111,1195-1209`, I);
  no complete lifecycle trace is provided. The correlation-counter maps at
  `runtime_types.rs:1189-1191` were indexed but their functional role/readers
  were not verified and are not asserted as layout/operation authority.
- Free-function tables exclude type/enum constructors, Default/parse methods
  unless explicitly listed, and world-engine methods described separately.
  Alias/reexport and inline-cfg coverage is source-search based, not a compiler
  call graph. This is not proof that every production library decision has
  been found. Shared-return labels do not establish equality with world policy.
- No exhaustive equivalence comparison between birth/focus/float policy in
  adapters and in core was performed; the actual local predicates are recorded
  without deciding whether any duplication is acceptable.
- Rejected-report attempt reconstruction on M, eventual incorporation after M
  refusal, real native anti-fighting behavior, and per-platform parity remain
  untested. No live session, mutation probe, or cost measurement supports them.

## Evidence: package C (performance)

### C1. Existing offline harnesses and profile runs

- D/I notation: D means source inspected or command executed directly; I means
  an indexed source or reported run not reproduced directly. Historical numbers
  remain historical even where the document containing them was read directly.
- I search coverage: Rust sources/Cargo manifests for count loops, window/output
  counts, sizing/stress/large-world names; core/protocol test listings; benchmark
  paths; size references in Rust and Markdown. No existing 10/50/200-window by
  1-4-output `handle_report` performance/size sweep was located. Core/protocol
  test listings reported zero benchmarks. This is a search result, not proof
  that no additional harness exists.

| Evidence | Existing harness and scale | Source / what is measured |
|---|---|---|
| D source, I run | `moving_tracking_lag_with_virtual_schedule`: 3 base windows, 2 outputs; 40 virtual ticks, 10 timed completions | `crates/tiler-core/tests/moving_border_comparison.rs:77-101,1075-1097,1115-1163`; base roster indexed at `:424-435,480-500` (I). Aggregate time around `accept_reply`, not internal phase attribution; illustrative, no thresholds. |
| D source, I run | `accepts_duo_all_schedules_with_locked_counts`: 2 windows, 1 screen | `crates/tiler-protocol/src/world_stress.rs:2194-2208`; fixture count indexed (I). Ordered/batched/reordered report totals 186/149/151, 16 checkpoints each; at least 100 command actions. No handler timing/size stdout. |
| D source, I run | `accepts_trio_all_schedules_with_locked_counts`: 3 windows, 1 screen | `crates/tiler-protocol/src/world_stress.rs:2211-2225`; fixture count indexed (I). Report totals 190/156/158, 19 checkpoints each. No handler timing/size stdout. |
| D source, I run | `oversized_request_refuses_before_any_mutation`: 10,000 long-ID windows | `crates/tiler-core/tests/world_envelope_budget.rs:322-347`. Tests rejection at the request-estimate gate, not accepted large-world policy cost. No byte-size stdout. |

- I exact executed commands (each selected test passed in both profiles):

```sh
cargo test -p tiler-core --test moving_border_comparison moving_tracking_lag_with_virtual_schedule -- --exact --nocapture
cargo test -p tiler-core --release --test moving_border_comparison moving_tracking_lag_with_virtual_schedule -- --exact --nocapture
cargo test -p tiler-protocol world_stress::stress_tests::accepts_duo_all_schedules_with_locked_counts -- --exact --nocapture
cargo test -p tiler-protocol --release world_stress::stress_tests::accepts_duo_all_schedules_with_locked_counts -- --exact --nocapture
cargo test -p tiler-protocol world_stress::stress_tests::accepts_trio_all_schedules_with_locked_counts -- --exact --nocapture
cargo test -p tiler-protocol --release world_stress::stress_tests::accepts_trio_all_schedules_with_locked_counts -- --exact --nocapture
cargo test -p tiler-core --test world_envelope_budget oversized_request_refuses_before_any_mutation -- --exact --nocapture
cargo test -p tiler-core --release --test world_envelope_budget oversized_request_refuses_before_any_mutation -- --exact --nocapture
```

- I verbatim microbench stdout, debug then release. These are aggregate
  nanoseconds over 10 timed completions, not per-call latency or native latency:

```text
microbench_illustrative_no_thresholds_ns: build=82203 handle=10235089 calls=10 publish_wrap=25931 offer=9593 retained_recompute=10322
microbench_illustrative_no_thresholds_ns: build=23433 handle=2214299 calls=10 publish_wrap=4048 offer=3727 retained_recompute=1029
```

- I verbatim Cargo result lines, duo debug/release, trio debug/release,
  oversized-request debug/release respectively. Durations are whole-test elapsed
  times, not `handle_report` timings:

```text
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in 0.28s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in 0.06s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in 0.33s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in 0.08s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.03s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.01s
```

- I command correction: an initial unqualified duo filter selected zero tests;
  the module-qualified commands above selected the actual tests. A size-search
  shell quoting error was corrected before obtaining search results. Neither
  failed command is performance evidence. An additional debug
  `cargo test -p tiler-protocol` run reported 69 passed, 0 failed; it supplied
  correctness coverage, not cost attribution.

### C2. Internal cost split and missing measurement

- I no inspected existing harness split `handle_report` into engine clone,
  validation/digest, policy, and canonical encoding. The moving-border test
  only times the enclosing acceptance call. The historical codec/core/encode
  split in C3 still treats core as one opaque span.
- D source boundaries for minimal temporary instrumentation (not added):
  `crates/tiler-core/src/world_engine.rs::WorldEngine::handle_report`:
  time `report_estimate` (`:1234`), `validate_report_reason` (`:1246`), and
  `report_digest` (`:1280`) separately; time `reply_upper_bound` (`:1343-1348`)
  separately from those; time only `self.clone()` (`:1391`), then
  `staged.run_accepted(...)` (`:1395`), `reply_estimate` (`:1398`), and
  `encode_reply_canonical(...).len()` (`:1399`). Keep commit/cache replacement
  (`:1406-1407`) and incarnation reset separate so the spans do not silently
  absorb drops or resets. `run_accepted` is convergence/settlement/commands/
  publication together, not a pure layout-only span (`:1426-1429`).
- I minimal missing scale harness: extend existing report/fact construction
  helpers in `crates/tiler-protocol/tests/world_codec.rs:12-60` or typed
  `report(...)` in `crates/tiler-core/tests/moving_border_comparison.rs:77-101`
  with 10/50/200-window, 1/2/4-output cases; print actual request-string byte
  length and canonical reply byte length. Use both initial and subsequent
  accepted reports to distinguish empty-engine from populated-engine cloning;
  disclose fitting versus constraint-held outcomes and command/feedback state.
  No such matrix or temporary instrumentation was written or measured here.

### C3. Encoded sizes: concrete fixtures and historical samples

- D commands below measure compact UTF-8 JSON of existing fixture objects,
  excluding their pretty-printing, wrapper, and file newline. They do not
  measure the broker envelope, native traffic, or a typical-user distribution;
  jq reserialization is not a byte-for-byte canonical-writer equality test.

```sh
jq -c '{file:input_filename,bytes:(.report|tojson|utf8bytelength),windows:(.report.windows|length),screens:(.report.screens|length)}' test-fixtures/world-wire/report-observe.json test-fixtures/world-wire/report-send.json test-fixtures/world-wire/report-outer-gap.json
jq -c '{file:input_filename,bytes:(tojson|utf8bytelength),windows:(.windows|length),outputs:(.outputs|length)}' test-fixtures/world-wire/reply-golden.json test-fixtures/world-wire/reply-group.json test-fixtures/world-wire/reply-migrate.json
```

- D verbatim output:

```text
{"file":"test-fixtures/world-wire/report-observe.json","bytes":5375,"windows":7,"screens":2}
{"file":"test-fixtures/world-wire/report-send.json","bytes":5953,"windows":7,"screens":2}
{"file":"test-fixtures/world-wire/report-outer-gap.json","bytes":2731,"windows":2,"screens":1}
{"file":"test-fixtures/world-wire/reply-golden.json","bytes":4971,"windows":7,"outputs":2}
{"file":"test-fixtures/world-wire/reply-group.json","bytes":1642,"windows":2,"outputs":1}
{"file":"test-fixtures/world-wire/reply-migrate.json","bytes":2672,"windows":2,"outputs":2}
```

- I fixture provenance indexed at
  `crates/omnitiler/examples/world_wire_fixtures.rs:1-7,24-29`: observe and
  outer-gap are production-payload inputs; other snapshots reuse the report
  shape with synthetic screens. Rows above are independent samples, not
  necessarily matched request/reply pairs.
- D document, I historical measurements:
  `docs/changes/archive/core-full-state-replies.md:186-197` reports release,
  fresh engines, 11 repetitions, exact compact UTF-8 sizes. Not rerun here:

| Workload | Request / reply bytes (verbatim) | Total median / p95 (verbatim) | Decode/core/encode medians (verbatim) |
|---|---|---|---|
| 800 windows, 64 domains | `226911 / 178690` | `97.36 / 103.07 ms` | `2.45/94.35/0.33 ms` |
| 2000 windows, 128-byte IDs, 64 domains | `759891 / 634214` | `100.07 / 104.21 ms` | `6.14/93.77/0.62 ms` |

- D document: the 800-window sample had usable allocations; the 2000-window
  sample was constraint-held, with zero usable allocations. Domains are not
  asserted to be outputs. I no ordinary 10/50/200-window size table was located;
  these larger historical samples cannot supply it. Prior A8 and KDE synthetic
  transport measurements were not repeated.

### C4. KDE JSON pass accounting

- D source-derived counts below are for one fresh successful capture, not exact
  replay, command-queue/take/read methods, errors, or native timing. One strict
  `decode_request` performs one full JSON parse
  (`crates/tiler-protocol/src/world_protocol.rs:1593-1601`). Broker source below
  is `crates/omnitiler/src/world_broker.rs`.

| Path | Full request parses | Stamped request serializations | Reply parses in broker helpers | Reply/projection serialization calls after core |
|---|---|---|---|---|
| D commandless acceptance | 3: outer envelope + pre-drain decode + dispatch decode | 1 | 5 if preview suspended, otherwise 6 | 5: protocol encode + 4 helper serializations |
| D queued-command acceptance | 3: outer envelope + commandless pre-drain decode + commanded decode | 2 | 6 if preview suspended, otherwise 7 | 5: protocol encode + 4 helper serializations |

- D request call sites: outer parse and inner-map clone at `:1515-1518,1586-1602`;
  commandless stamp/serialize/decode and explicit discard at `:737-764`;
  commandless execution decode at `:794-795,1004-1016`; queued branch
  clone/stamp/serialize/decode at `:939-952`. Thus the request total is three
  full parses, not two. Queue submission parsing is a separate method.
- D reply helper inventory: `preview_suspended_by_presentation` parses once
  (`:2129-2147`); only a false result calls `drag_preview_of`, which parses
  once (`:840-844,2160-2162`). `strip_result` (`:1920-1927`),
  `inject_gesture_assoc` (`:1895-1914`), `bind_response` (`:1868-1886`), and
  `project_reply` (`:2195-2238`) each parse once and serialize once; the last
  serializes a reduced projection, not an identical full reply. Invocation
  order is `:851-858`. Queued success additionally parses once in `result_of`
  (`:967-975,1932-1937`); it does not serialize the full reply there.
- D core separately canonical-encodes newly accepted staged replies for its
  size gate (`crates/tiler-core/src/world_engine.rs:1397-1407`), making six
  reply/projection serialization calls including core on the fresh accepted
  paths above. Protocol encoding is a separate writer, not reuse of that
  string (`crates/tiler-protocol/src/world_protocol.rs:1944-1963`). A command
  rejection can add an observation-only dispatch/decode/core call
  (`world_broker.rs:977-998`); it is not the queued-success row.
- D script envelope construction uses template strings and five individual
  string-value `JSON.stringify` calls, not a whole-envelope stringify
  (`kwin/src/world-report.ts:1611-1620`). I inner report is manually assembled
  (`:1828-1853`); script reply parsing adds one `JSON.parse` (`:607-615`).
- I no existing broker helper cost test was located. Targeted `Instant`,
  `elapsed`, and `bench` searches found broker clock use, not phase timings.
  Minimal temporary cost instrumentation would time each named helper at
  `world_broker.rs:840-858,967-972`, plus `decode_request` and `encode_reply`
  at `:750,952,1008,1016`, using the same existing captured payloads. No such
  instrumentation was added; byte-pass counts alone are not cost percentages.

### C5. What an engine clone contains

- D `WorldEngine`, `Engine`, and `Session` derive `Clone`; the staging call is
  `WorldEngine::clone`, not merely `Engine::clone`. Source below is relative
  to `crates/tiler-core/src/`.

| D type / fields | Growth-bearing owned state and source |
|---|---|
| `WorldEngine` cached messages | `last_report: Option<WorldReport>`, `last_reply: Option<WorldReply>` (`world_engine.rs:422-439`): previous full typed payloads are inside the clone. |
| Retained work and per-window memory | `retained`, `retained_floats`, `retained_clears`, `retained_born`; born-lifetime sets; `frames`, `consumed_frames`, `frame_bursts`, `learned_mins` (`:444-512`). |
| Workspace/output/history | `retained_creations`, `pending_source_releases`, `last_views`, `mapping`, `native_map`, `views`, `history`, `displaced`, `output_facts`, `live_topology`, `tiling` (`:524-585`). `ScopeHistory` has `previous`/`baseline` maps and two optional shared-scope strings, not a shared allocation or unbounded event log (`workspace_scope.rs:458-467`). |
| Membership, staged observation, presentation | `members`, `floating_scopes`, `hold_reasons`, `pending_forget`, `operation_settled`, `admission_focus_spent`, `staged`, three presentation frame maps, `presentation_order`, `last_active_window` (`world_engine.rs:591-722`). `StagedReport` owns screen/domain vectors and fact/output/homing maps (`:401-419`). |
| `Engine` | `sessions: BTreeMap<DomainKey, Session>`, `outer_gaps`, owned owner/generation; `policy: Arc<dyn LayoutPolicy>` (`engine.rs:54-65`). |
| `Session` | `domains`, `trees`, `windows`, `focus_stack`, `last_active`, `exceptions`, `retained_float_geometry`; fixed/capacity sets, `admission_order`, `overlap_accepted`, `minimized_slots`, canonical-pair restore map, reconciler, optional pending desired/drag/trace state (`session.rs:752-860`). |
| Nested snapshots | `PendingDesired` and `DragState` own additional tree/window/exception maps (`session.rs:619-674`); `MinimizedSlot` owns a `Node` tree (`session/world.rs:199-206`); `Node::Group` owns recursive `Vec<Node>` and `Vec<u64>` (`directional.rs:122-133`). `Reconciler` derives `Clone` and owns optional pending state (`reconcile.rs:228-246`). |

- D not all deep: the layout policy in `Engine` and each `Session` is an
  `Arc<dyn LayoutPolicy>` (`engine.rs:61`, `session.rs:755`), so cloning shares
  that policy. The listed mutable maps/sets/vectors/strings and recursive nodes
  are owned and copied by derived cloning, rather than shared by `Arc`/`Rc`.
  I the inspected-source sharing search found no other shared mutable state in
  these definitions; it also indexed the built-in policy as a unit struct
  (`policy.rs:93-97`). No heap-byte or allocation-count measurement was obtained;
  inline struct size would not quantify this heap-owned graph.

### C6. Upper-bound claim versus exact encoding use

- D `reply_upper_bound` documents that any publishable reply fits its bound
  (`world_sizing.rs:1817-1856`); its implementation adds `report_estimate` and
  saturating per-transfer/workspace/window allowances plus slack (`:1858-1883`).
  `handle_report` uses it before mutation (`world_engine.rs:1327-1354`).
- D a separate post-run `reply_estimate` documents structural coverage of every
  encoded section (`world_sizing.rs:157-176`). It is not the same function as
  the pre-mutation bound. I repository search located no direct test invocation
  of `reply_upper_bound`; the test name `estimate_upper_bounds_canonical_encoding`
  cited by the estimate's documentation was found only in that documentation.
  Located codec tests exercise writer equality and selected gesture/warp/
  operation/native-workspace estimate cases, not exhaustive pre-bound coverage
  (`crates/tiler-protocol/tests/world_codec.rs:1255,1286,1310,1392,1659`, I).
- D the exact encoded string is used only for `.len()` and discarded at
  `world_engine.rs:1398-1400`; commit/cache/return retain the typed reply
  (`:1406-1411`). The KDE broker separately encodes it again
  (`world_broker.rs:967,1016`). Thus core's exact encoding is not also the
  transport payload on the inspected path. If the documented pre-bound and
  structural-estimate invariants always hold, the later exact gate cannot
  reject an otherwise passing reply; universal correctness of those invariants
  was not established by the located tests. No removal decision is made here.

### C7. Gaps and observations outside the brief

- I gaps: no realistic-scale accepted-report timing/size matrix; no internal
  clone/validation/policy/encode cost split; no broker helper cost attribution;
  no engine heap-size measurement. C2/C4 identify minimal timing locations;
  none of the measurements establishes attempt rows as the dominant byte or
  latency contributor. Per-section encoded lengths on those same scale reports
  would be needed to attribute message size to feedback versus other fields.
- D observation (out of brief): the sizing documentation cites a test name not
  located by the reported exact-name search (I search); this is a traceability
  gap, not evidence that the bound fails.
- D observation (out of brief): each stored minimized slot contains an owned
  pre-minimize tree, and pending desired/drag state can contain additional
  topology snapshots (`session/world.rs:199-206`, `session.rs:619-674,848-854`).
  Window count alone does not describe which of those optional snapshots is
  populated; no asymptotic or measured clone-cost claim is made for them.

## Evidence: package D (tabbed groups inputs)

### D0. Scope and notation

- D = source/document read directly; I = reported source inspection or indexed
  evidence. Mixed entries identify the distinction. These are inputs, not a
  tab design or a recommendation. No live behavior was exercised.
- D current contract is v3 (`crates/tiler-core/src/world_report.rs:41`). The
  coordinated visibility design is not implemented tab support. Source pins
  below come from `docs/spec/reference-outcomes.md:1136-1141`; all reference
  source citations are at the stated pin, not a moving checkout.
- D reference notation: "`S[A*,B]` means one tabbed tile, A active"
  (`docs/spec/reference-outcomes.md:1123`). "TBD" means unestablished, not a
  negative (`:1118-1119`).

### D1. Existing core tree, geometry and group presentation

| Evidence | Current fact and source |
|---|---|
| D | `Node` has only `Leaf { id }` and `Group { id, axis, children: Vec<Node>, shares: Vec<u64> }`. The group splits its extent along the axis with one positive share per child; no layout-kind or selected-child field exists in this definition (`crates/tiler-core/src/directional.rs:117-133`). |
| D | `Session` retains `trees: BTreeMap<DomainKey, Option<Node>>`, window links, one focused domain/leaf, per-domain focus stacks and last-active leaves (`crates/tiler-core/src/session.rs:752-767`). `Engine` retains per-domain sessions and an `Arc<dyn LayoutPolicy>`; its comment says COSMIC v1 is the only implementation (`crates/tiler-core/src/engine.rs:54-65`). |
| D | `geometry::project` validates and calls `layout_into`; strict projection promises contained, non-overlapping leaf rectangles. `layout_into` divides the group extent, advances a cursor by child size plus gap, and recursively projects every child (`crates/tiler-core/src/geometry.rs:145-164,319-405`). It does not choose one visible child. |
| D | `size_hints::project_with_hints` validates with strict `project` and runs `layout_hinted` for all leaves (`crates/tiler-core/src/size_hints.rs:530-565`). There is also positive-capacity-overflow projection: descendants can overlap other branches or leave the domain while retaining positive extents (`crates/tiler-core/src/geometry.rs:167-205`; hinted counterpart `size_hints.rs:568-589`). This is not a shared-rect, selected-member tab layout. |
| D/I | D `ActiveGroup { group, members, bounds }` and `ActiveGroupMember { window, leaf, rect }` describe the focused leaf's immediate parent, not a tab's active member (`crates/tiler-core/src/active_group.rs:21-55`). I `describe_active_group_with_hints` filters projected descendant leaves and computes their checked union (`:134-189`). |
| D | Existing workspace `TilingMode` is only `Tiled` or `Floating`; floating leaves native geometry untouched (`crates/tiler-core/src/world_report.rs:85-95`). It is not split/tabbed/stacked mode. |
| I | Core keyword search found focus-history/local traversal stacks and an explicitly unsupported center-stack drop fact, not a tab carrier (`crates/tiler-core/src/contract.rs:1150,1209-1259`; `reconcile.rs:1789`). Search covered directional, session, engine, active-group, geometry, presentation and tab/stack/monocle terminology. Bounded conclusion: the inspected topology/projectors have no container whose children share one rect with one selected visible member. |

### D2. Existing documentation and visibility-ready design

| Evidence | Record / quotation |
|---|---|
| D | `docs/decisions.md:3982-3990`: "Grouping here means nested split-tree structure and placement. Tabs, stacked/shared groups, and compositor group behavior are excluded." Tabs are first after 0.1; "No tab or stack carrier, controls, or bindings are selected." |
| D | `docs/backlog.md:1876-1890`: "Tabbed stacks (next after the 0.1 release)", "built in the Rust engine". User sketch: "size stacked windows slightly shorter and draw the tab strip with machinery like the group underlay." Keyboard-first is conditional on mouse complexity, not selected implementation. |
| D | `docs/decisions.md:3991-3997`: coordinated v4 is "visibility-ready", with independent per-window suppression authority/correlation/capability, core-composed workspace versus inactive-tab suppression, truthful hidden observations. Full tabs remain post-0.1; later tab bump accepted, expected around 0.2. Close survivor is provisionally positional-next, clamped at end, pending source confirmation. |
| D | `docs/spec/reference-outcomes/groups-stacks.md:9-47`: R-GRP-01 is split-to-stack then tab switch. Ours KDE/Windows: "Deferred: centre-stack drops refused fail-closed; no tab carrier/bindings". R-GRP-02 is sequential join then leave (`:61-100`); Ours is fixture-inapplicable, not a tested no-op (`:126-131`). |
| D | R-GRP-03 starts `S[A,B*,C]` with history A,C,B, then closes B (`docs/spec/reference-outcomes/groups-stacks.md:133-161`). The recorded C survivor agrees for COSMIC, Hyprland, i3 and sway, but the row itself identifies positional index retention for COSMIC and focus-history selection for i3/sway. This fixture does not distinguish next-position from MRU because both select C. |
| D | `docs/changes/windows-workspace-return-focus.md:2404-2432`: "The new visibility contract remains design only"; phase-1 core gap foundations are complete offline. Includes independent suppression capability, authority/correlation, composition and truthful hidden observations, but "No tab containers, reservations, tab wire tokens, UI or promises of avoiding a later tab bump." Proposed spellings are tentative, not existing APIs. |
| D | Same design `:2438-2448`: suppress when ANY applicable core reason requires it; release of workspace suppression cannot release another reason. Suppression release is not unminimize, activate, app-hide, membership or frame authority. Facts must not project hidden as minimized. |
| D | Proposed delta at `:2473-2484`: `windowSuppression`, per-window `visibilityAuthority`, frozen `visibilityGroups`, keyed `visibilityWork`, `visibilityResults`, `selectionProofs`, `visibilitySettled`, and fact/sample `visibility:{visible:bool|null,suppressionOwned:bool|null}`. `visible` is native shown state, not unobscured pixels or current-workspace membership; a product-owned claim does not prove invisibility. These are design-only names. |
| D | Research index distinguishes COSMIC stacks from splits and Hyprland groups from its Monocle algorithm; i3/sway use tree/container layouts (`docs/research/prior-art.md:230-253`). Historical consensus backfill also distinguishes tabbed display from visible stacking and app-native tabs (`docs/research/reference-wm-consensus.md:1006-1023`); its counts are indexed history, not a fresh source verification. |

### D3. Visibility contract and platform mechanics today

- D `WindowDesire` has lifetime/output/workspace, allocation/effective frame,
  frame/membership authority, float classification and frame/clear operation
  tokens, but no visibility/minimize/offscreen/suppression target
  (`crates/tiler-core/src/world_report.rs:282-322`). Workspace selection is
  expressed through `OutputDesire.logical_view`, plus workspace/native binding
  rows (`:332-362`); absence of an allocation alone is explicitly not a float
  classification (`:295-301`).
- D `WindowFact` reports `minimized`, rect, fullscreen/maximized and logical/
  native membership, but has no per-window shown-state or product-suppression
  ownership field (`world_report.rs:706-721`). `ScreenView` is independently
  verified applied logical view, not a desired-view echo (`:723-728`).
  `PresentationIngress` has foreground-only minimized/cloaked/hidden-workspace
  facts (`:600-650`), not a complete per-window visibility inventory.
- I current feedback has member/view/frame/clear/native-creation attempts;
  `WorldReply` has operation/gesture settlement but no visibility-work/result
  lane (`world_report.rs:371-580,1186-1213,1301-1358`). D the proposed lane in
  D2 must not be read as current feedback.

| Evidence | Platform | Current mechanism and evidence boundary |
|---|---|---|
| I, with D hide-function read | Windows | Workspace path calls managed product-hide claims (`crates/tiler-windows/src/tiling_sys/world_native.rs:391-418`). `hide_managed_claim_with_mode_traced` commits the durable claim, posts `SW_HIDE` using the pumped ShowWindowAsync route or nonactivating SetWindowPos route, then confirms hide (`crates/tiler-windows/src/product_hide/sys/mod.rs:1751-1793`, D). I API definitions use `SWP_HIDEWINDOW`; reveal posts `SW_SHOWNA` or preserves the originally minimized case with `SW_SHOWMINNOACTIVE`; nonactivating SetWindowPos show is also supported (`:791-810,1877-1878,1901-1940`). This inspected route is shown-state hiding, not DWM cloaking or offscreen parking. I world visibility readback/claim-ledger evidence: `world_native.rs:1114-1155,1235-1292`. |
| D/I | KDE | D workspace selection writes `setCurrentDesktopForScreen` when supported, else shared `currentDesktop`; readback uses `currentDesktopForScreen` or shared selection (`kwin/src/workspace-native.ts:470-528`). I no implemented arbitrary tab-member hide route was established. D coordinated design explicitly keeps per-window suppression false until an approved writable route is proven; hidden/minimized declarations are read-only and must not be replaced with minimization or parking desktops (`docs/changes/windows-workspace-return-focus.md:2657-2660`). |
| I/D | macOS | I production `NativeSelection::select` calls `mc_goto_sid(..., false)` after safety/topology checks: native Space selection, not per-window hide (`crates/tiler-macos/src/workspace_select.rs:2127-2178`). D coordinated design says no approved arbitrary per-window hide route; advertise suppression false. AX owned minimization is fixture-only, and app hide is not per-window suppression (`docs/changes/windows-workspace-return-focus.md:2661-2667`). No native suppression live behavior was tested. |

### D4. Decorations, group rendering and drag previews

- D capture/policy/render division is explicit: adapter captures observed
  foreground and host-scaled style, core computes verdicts, adapter renders
  (`crates/tiler-core/src/world_report.rs:600-650`).
- D `WorldReply.presentation: WorldPresentation` carries `foreground`,
  optional `suspended_output`, `border`, `underlay`, `pager`; renderer must
  revalidate the foreground token (`world_report.rs:1353-1358`;
  `world_presentation.rs:111-127`). Underlay `Found` contains revision/home,
  group/focused identities, projected members, union rect and outer fill rect;
  otherwise `Hidden { base_revision, reason }` (`world_presentation.rs:85-109`).
  These definitions have no tab labels/order/selected-tab or tab-hit-region
  fields. Group underlay is a split-group highlight, not stacked content.
- D `command_result` is correlated with the snapshot. `DragPreview` carries
  rect/hover/domain/source/neighbor target/revision; it does not commit or
  advance revision (`world_report.rs:1286-1299,1367-1382`). Other inspected
  variants are Float, Migration and Directional; no current group-rendering
  result variant was located (`:1390-1423`, D read plus I enum inspection).
- D historical wording "Operation results preserve ... group rendering" exists
  at `docs/changes/archive/core-full-state-replies.md:176`. Current source puts
  group rendering in uniform `presentation.underlay`, not a separate current
  `CommandResult` group operation; the historical sentence is not today's
  field inventory.

| Evidence | Renderer consumer |
|---|---|
| I | Windows consumes `reply.presentation.border` (`crates/tiler-windows/src/tiling_sys/focus.rs:302-310`), owns border/underlay/preview overlays (`tiling_sys/mod.rs:634-703`), and applies them in its native run loop (`tiling_sys/run_loop.rs:628-672`). Preview command-result consumer: `tiling_sys/drag_preview.rs:185`. |
| D/I | KDE effect's `updateGroupUnderlayFill` paints an ImageItem; `updateGroupAnchorAndGeometry` takes accepted core member IDs/outer rect and anchors below the lowest renderable member, translating coordinates rather than recomputing shape (`kwin/native-effect/activewindowborder.cpp:513-595`, D). I `updateDragPreview` uses `m_dragPreviewItem` (`:1781-1808`). This is native effect rendering, not Rust drawing. |
| D/I | macOS production builds `BorderReplyTick` from retained presentation and drives the carrier controller (`crates/tiler-macos/src/tile.rs:1945-1974`, D); `border_runtime.rs:622-659` uses core border visibility/outer rect with active-Space/session fences and coordinate flip (D). I `world_builder.rs:128-130` sets `underlay_enabled:false`, commenting "no production underlay loop yet". `group_underlay_session.rs:3-7,934-939` is a fixture carrier, not production underlay. |

### D5. Pinned reference WM semantics

- D documented pins: i3 `903bcd51`, sway `1652c54b`, Hyprland `19fb395d`,
  COSMIC `3d55cba0` (`docs/spec/reference-outcomes.md:1136-1141`). Expanded
  refs used for direct reads: i3 `903bcd518df32b0e055b17f5da3f988a0187fd3d`,
  sway `1652c54b73f67df17b7b4ab0b0f7048204aa8104`, Hyprland
  `19fb395d45314960e6f79f17994a84094f1cd4f6`. Paths in each table refer to
  that WM's source at its pin. Source behavior is not a fresh live test.

#### D5a. i3 at `903bcd51`

| Evidence | Aspect | Source fact |
|---|---|---|
| I | Model | Tabbed/stacked are parent-container `layout` values; `con_set_layout` retargets to the parent (`src/con.c:1997-2006`), not a separate group object. |
| D | Rects / inactive content | `render_con_tabbed` assigns all children the same content slot, reserving one titlebar row; stacked reserves one row per child (`src/render.c:437-480`). It renders every child then raises in focus order, with focused child last and recursively rendered again (`:161-208`). This evidence is overlap/raise behavior, not proof that inactive X clients are unmapped. I `src/x.c:1143-1178` maps client children; no inactive-unmap path was established in this inspection. |
| D/I | Focus / directional entry and exit | D `con_orientation` returns HORIZ for tabbed and VERT for stacked (`src/con.c:1620-1630`). I `tree_next` climbs matching-orientation ancestors, selects siblings, descends and wraps per `focus_wrapping` (`src/tree.c:503-587,593-629`). Thus left/right tabbed and up/down stacked use ordinary tree focus traversal, including exits to ancestor siblings. Decoration clicks/scroll select tabs (`src/click.c:232-277`). |
| I | Move into/out | `src/move.c:310-353` swaps siblings, inserts into a bordering branch, or climbs when no sibling exists. The indexed sequential fixture joins B last into `S[A,C,B*]`, then promotes tail B right to `H[S[A,C],B*]` without dissolving the remaining group (`docs/spec/reference-outcomes/groups-stacks.md:96`, D document read, not a fresh source replay). |
| D/I | Titlebars | D layout supplies per-child decoration rects (`src/render.c:437-480`); I i3's `x_draw_decoration` draws decoration colors/text (`src/x.c:459-520`). The WM, not client applications, draws these tabs. |

#### D5b. sway at `1652c54b`

| Evidence | Aspect | Source fact |
|---|---|---|
| I | Model | `layout tabbed`/`stacking` modifies a parent split layout, including workspace wrapping where needed (`sway/commands/layout.c:11-22,117-199`). |
| D | Wayland content visibility | `arrange_children` keeps child/titlebar scenes enabled but disables inactive border trees and calls `disable_container`; this reparents inactive view scenes under their content tree (`sway/desktop/transaction.c:291-303,313-373`). That content tree is a child of the border tree (`sway/tree/container.c:85-86`), so inactive client content is under a disabled scene while titles remain available. This is compositor scene visibility, not client minimization or a workspace move. I `view_is_visible` also rejects inactive tab/stack ancestry (`sway/tree/view.c:1172-1185`). |
| I | Focus / directional entry and exit | `sway/commands/focus.c:17-58` maps next/prev to left/right in tabbed and up/down in stacked. `:158-223,473-489` searches matching-layout siblings/ancestors, descends to focus-inactive view and handles outputs/wrapping. Layout conversion itself does not write focus (indexed `docs/spec/reference-outcomes.md:1571-1577`). |
| I | Move into/out | `sway/commands/move.c:112-166,301-415` handles sibling movement, reparenting/promotion and ancestor search. Indexed join/leave fixture appends B into the tabbed parent then promotes it right at the edge with focus retained (`docs/spec/reference-outcomes/groups-stacks.md:100`, D document read). |
| D/I | Titlebars | D transaction arranges side-by-side title slots for tabbed, rows for stacked (`sway/desktop/transaction.c:321-373`). I `container_arrange_title_bar` positions title/marks text scene nodes and updates background/border scene rectangles (`sway/tree/container.c:286-370`). The compositor draws them. Exact xdg activation writes were not traced. |

#### D5c. Hyprland at `19fb395d` (Dwindle profile)

| Evidence | Aspect | Source fact |
|---|---|---|
| I | Model | Separate `CGroup` replaces the first member's layout target with a group target; adding members updates group visibility (`src/desktop/view/Group.cpp:42-66,97-162`). This is separate from the Dwindle split tree and from Monocle layout. |
| D | Inactive content | `CGroup::updateWindowVisibility` clears input blocking and sets layout alpha 1 for current; others get `FOCUS_BLOCK_GROUP_INACTIVE` and layout alpha 0, then the group target is recalculated/damaged (`src/desktop/view/Group.cpp:413-432`). |
| I | Focus / directional navigation | `moveCurrent` wraps and changing current refocuses only if the group was focused (`Group.cpp:300-342`). `changeGroupActive` rejects a one-member group (`src/config/shared/actions/ConfigActions.cpp:963-993`). `moveFocus` conditionally cycles group members left/right first with `binds:movefocus_cycles_groupfirst`, otherwise queries a directional target and switches (`:440-474`). These are distinct paths, not an unconditional tab-first rule. |
| I | Membership / lock | Move-into helper adds mover, makes it current and focuses it; move-out follows `group:focus_removed_window` (`ConfigActions.cpp:1338-1375`). `moveIntoGroup`/`moveOutOfGroup` and `moveWindowOrGroup` enforce global/member locks as applicable (`:1377-1475`); lock toggle is `:1319-1335`, and `binds:ignore_group_lock` bypasses checks. The inspected lock checks guard membership movement, not ordinary tab activation. |
| I | Groupbar | `CHyprGroupBarDecoration::draw` renders per-member bars with stacked/side-by-side and locked/active colors (`src/render/decorations/CHyprGroupBarDecoration.cpp:102-180`); click/scroll activation is `:452-520`. The compositor decoration owns drawing/input. |

#### D5d. COSMIC at `3d55cba0`

| Evidence | Aspect | Source fact |
|---|---|---|
| I | Model / geometry | Separate stack element holds windows and active index; all members receive common content geometry below `TAB_HEIGHT` (24) (`src/shell/element/stack.rs:146,194-224,598-609`). |
| D/I | Inactive content | D `push_render_elements` indexes `windows[active]` for geometry and content preparation (`stack.rs:762-828`). I the continuation submits only that active window's render elements plus the stack UI (`:829-870`); popup rendering likewise selects the active member (`:660-693`). This is compositor render selection, not minimize/offscreen state. |
| I | Focus / directional entry and exit | `handle_focus` steps left/right through active members, restores at an exhausted edge, restores previous member for Up/Down, and toggles whole-stack focus for In/Out (`stack.rs:311-419`). Adjacent spatial focus is separate ancestor-group traversal (`src/shell/layout/tiling/mod.rs:1835-1965`); no more specific adjacent-stack entry rule was established here. |
| I | Move into/out | Stack-local `handle_move` reorders in-range left/right, removes active member and returns `MoveOut` at an edge (`stack.rs:431-483`). Indexed root-len-2 neighbor fixture appends mover B and activates it, then a right move at the tail expels it as a new tile (`docs/spec/reference-outcomes/groups-stacks.md:81-86`, D document read). |
| I | Tabbar | `DefaultDecorations::view` constructs `Tabs` from all windows, active index and activation/group-focus state, with press/drag/menu/close callbacks (`stack.rs:1301-1395`). `focus_under` hit-tests stack UI in the tab strip and otherwise delegates to active content (`:534-580`). The compositor's stack UI draws the tabs. |

### D6. Gaps and observations outside the brief

- D/I gaps: no native tab visuals/timing/input tests; no end-to-end proof that
  existing overlays can implement a tab strip; no KDE/macOS approved arbitrary
  per-window suppression actuator established. Reference evidence is source or
  indexed fixtures, not new live outcomes. i3 inactive-client unmapping and
  sway xdg activation writes remain unestablished; COSMIC adjacent-stack entry
  was not traced beyond generic group traversal.
- D the close-tab fixture's A,C,B history makes MRU and positional-next agree.
  The existing provisional positional-next decision is recorded in D2; no
  further survivor policy is selected by this package. A discriminating case
  would keep `S[A,B*,C]` but establish history C,A,B before closing B; outcomes
  remain TBD until source/user evidence establishes them.
- D observation (out of brief): the coordinated design section still lists
  current source version 2 and tentative next v3
  (`docs/changes/windows-workspace-return-focus.md:2459-2476`), while current
  `WORLD_CONTRACT_VERSION` is 3. Its design-only semantics are relevant inputs;
  those numeric source anchors are stale, not evidence of a shipped v4.
- D observation (out of brief): i3 `con_orientation`'s tabbed comment says
  "vertical" while the code returns HORIZ (`src/con.c:1628-1630` at its pin).
  Directional facts above follow the return value, not the comment.

## Evidence: package E (decisions index)

### E0. Scope and notation

- Source: `docs/decisions.md`, 5,748 lines, 47 second-level sections. Section endpoints exclude the next heading; lines 1-81 are the preamble/index.
- D = read directly; I = reported from targeted source extraction or existence checks. Each row identifies its evidence basis. Short quotes are excerpts, not replacements for their surrounding qualifications.
- Date/attribution copies the entry's wording. A section date is labelled as such; "not stated" does not infer an owner. Approval, tentative status, implementation and native acceptance are distinct.
- E2-E4 overlap deliberately. Repeated summaries and links are grouped where they concern the same choice. E5 lists pairs for review, not findings of contradiction or decisions about precedence. E6 distinguishes missing references from historical/removal wording and existing internal symbols.
- Relevance keys: F1 report shape; F2 commands; F3/F9 presentation facts and retained intent; F4/F5 identities and settlement; F7 rejection; F10 adapter policy; F12/F13 platform selectors; F16/F17 groups and visibility. Principles P1-P9 were read; findings were consulted by heading only.

### E1. Section map

| # | Section heading as written | Source range | Topic | Basis |
|---|---|---|---|---|
| 1 | Latest-world rendering (2026-10-27) | docs/decisions.md:82-135 | Command processing versus rendered intermediate states | D |
| 2 | Stress acceptance and dispatch (2026-10-27) | docs/decisions.md:136-161 | Shared stress oracles and dispatch routes | D |
| 3 | KDE port and race decisions (2026-10-26) | docs/decisions.md:162-233 | Freshness, suspension, eligibility and intent | D |
| 4 | Accepted reviews of tentative decisions (2026-10-27) | docs/decisions.md:234-281 | Gesture recovery and native renderer vocabulary | D |
| 5 | Accepted reviews of tentative decisions (2026-10-29) | docs/decisions.md:282-301 | Protocol cutover and completeness | D |
| 6 | Windows decisions (2026-10-29) | docs/decisions.md:302-325 | Staged focus outcome and sticky-off intent | D |
| 7 | Windows decisions (2026-10-28) | docs/decisions.md:326-459 | Coalescing, focus memory, visibility and edge focus | D |
| 8 | Active-drag preview continuity (2026-10-27) | docs/decisions.md:460-476 | Same-gesture preview continuity | D |
| 9 | Cross-output gesture release placement (2026-10-27) | docs/decisions.md:477-496 | Release target versus displayed preview | D |
| 10 | World command intent and birth ownership (2026-10-26) | docs/decisions.md:497-546 | Intent commands, birth clear and capability | D |
| 11 | KDE rapid directional input | docs/decisions.md:547-558 | Tentative FIFO directional dispatch | D |
| 12 | Development Entry Point | docs/decisions.md:559-595 | Development launch and fixture entry points | D |
| 13 | Scope and Platform Goals | docs/decisions.md:596-1110 | Platform scope, macOS routes and release gates | D |
| 14 | Windows Owner Threading | docs/decisions.md:1111-1270 | Input, owner and renderer handoffs | D |
| 15 | Windows Multi-Output Foundation | docs/decisions.md:1271-1324 | Transfer proof, DPI basis and migration | D |
| 16 | Project Name | docs/decisions.md:1325-1342 | Product naming | D |
| 17 | Licensing | docs/decisions.md:1343-1389 | Licences and notices | D |
| 18 | Development Environment | docs/decisions.md:1390-1468 | Tooling and fixture setup | D |
| 19 | Settings, Tray and First-Run | docs/decisions.md:1469-1758 | Catalog, settings delivery and pager surfaces | D |
| 20 | Workspaces | docs/decisions.md:1759-2238 | Authority, rings, selection, sends and migration | D |
| 21 | Move, Layout and Output Commands | docs/decisions.md:2239-2382 | Move/layout vocabulary and output sends | D |
| 22 | Fixed-Size Admission | docs/decisions.md:2383-2514 | Fixed classification, intent and above-tiles | D |
| 23 | Restart Persistence | docs/decisions.md:2515-2606 | Hydration, markers and float intent | D |
| 24 | Reference Matrix and Spec Authority | docs/decisions.md:2607-2684 | Discriminators, evidence authority and tiers | D |
| 25 | Shortcuts, Conflicts and Presets | docs/decisions.md:2685-2965 | Chords, conflicts and owned remediation | D |
| 26 | Gaming Safety | docs/decisions.md:2966-2994 | Suspension and game membership | D |
| 27 | Placement, Minimums and Startup Adoption | docs/decisions.md:2995-3128 | Native activation, placement and hint transport | D |
| 28 | Best-Effort Admission | docs/decisions.md:3129-3204 | Capacity floats, retries and adoption seeds | D |
| 29 | Engine Architecture and Convergence | docs/decisions.md:3205-3716 | World boundary and successive ownership extractions | D |
| 30 | Visuals: Border, Underlay and Grouping | docs/decisions.md:3717-3998 | Visual eligibility, carriers, grouping and tabs | D |
| 31 | Pointer, Drag and Drop | docs/decisions.md:3999-4110 | Drag lifecycle, insertion and restore | D |
| 32 | KDE Support Floor | docs/decisions.md:4111-4143 | Host versions and capability degradation | D |
| 33 | Delivery and Installation | docs/decisions.md:4144-4256 | Packages and installation | D |
| 34 | Linux Planner Activation | docs/decisions.md:4257-4330 | Activation, authentication and recovery | D |
| 35 | Live KWin/Plasma Boundary | docs/decisions.md:4331-4395 | Native-test boundaries and restoration | D |
| 36 | Observability | docs/decisions.md:4396-4453 | Summary/trace policy and diagnostics | D |
| 37 | Activation Requests | docs/decisions.md:4454-4530 | Urgency, focus and activation options | D |
| 38 | Window State: Float, Sticky, Maximize, Fullscreen | docs/decisions.md:4531-5165 | Membership, restore slots and native state transitions | D |
| 39 | Engine Operations and Policy | docs/decisions.md:5166-5294 | Structural authority, policy modes and R4 | D |
| 40 | Workspace vocabulary 2026-10-24 | docs/decisions.md:5295-5338 | Linked/separate views and equal-count limits | D |
| 41 | macOS v2 implementation decisions 2026-10-25 | docs/decisions.md:5339-5350 | Cutover, missing proof and binding lookup | D |
| 42 | macOS v2 targeting, receipt and presentation decisions 2026-10-26 | docs/decisions.md:5351-5385 | Arrival focus, creation ambiguity and miniature | D |
| 43 | macOS dogfood review decisions 2026-10-27 | docs/decisions.md:5386-5452 | Targeting, ordering and stress follow-ups | D |
| 44 | macOS follow-up review decisions 2026-10-28 | docs/decisions.md:5453-5561 | FIFO, receipt retirement, trailing pager and scope | D |
| 45 | Native adoption 2026-10-24 | docs/decisions.md:5562-5693 | Native identity, creation, retirement and settlement | D |
| 46 | Port deferred investigations 2026-10-24 | docs/decisions.md:5694-5731 | Activities, async settling and parked investigations | D |
| 47 | Independent host highlight colors 2026-10-24 | docs/decisions.md:5732-5748 | Independent native border/pager styling | D |

### E2. Decisions shaping the core/platform contract

| ID | Date / attribution as written | Decision excerpt and scope | Source | Basis |
|---|---|---|---|---|
| E2.01 | 2026-10-27 / User | "Core processes every command"; render final state, not each intermediate arrival | docs/decisions.md:84-100 | D |
| E2.02 | 2026-10-27 / User-approved semantics; tentative implementation record | Active-window memory follows command decisions, not render completion | docs/decisions.md:101-123 | D |
| E2.03 | 2026-10-26 section / approved; implemented offline 2026-10-27 | "neutral core rule hides borders, underlays, overlays and drop preview"; output-local fullscreen suspension | docs/decisions.md:180-199 | D |
| E2.04 | 2026-10-28 / N5, option B, implemented offline | "Minimized and host-hidden windows remain members but are ineligible" | docs/decisions.md:200-211 | D |
| E2.05 | 2026-10-26 section / approved; pending implementation; User 2026-10-29 | "ToggleFloat carries no intent-time rectangle"; no macOS exception, current frame on first float | docs/decisions.md:212-219 | D |
| E2.06 | 2026-10-28 / option A, implemented offline | "implicit selection/send/move commands leave output unset"; execution-time resolution | docs/decisions.md:220-228 | D |
| E2.07 | 2026-10-29 section / User-accepted on review | "exact v:3 cutover"; independent suppression capability, core work/group ids, typed lifecycle and known-state flags | docs/decisions.md:284-293 | D |
| E2.08 | 2026-10-29 section / User-accepted on review | "inventory readability gates host-owned domains only"; reuse completeness/unknown-hold paths | docs/decisions.md:294-297 | D |
| E2.09 | 2026-10-29 section / not separately stated | "No departure hide under staged authority without foreground proof"; individual visibility outcomes, fallback or blocked | docs/decisions.md:304-310 | D |
| E2.10 | 2026-10-29 section / option (A) | Core "sticky off, rehome to current" intent; adapters stop constructing explicit Send from sampled current | docs/decisions.md:312-318 | D |
| E2.11 | 2026-10-28 / approved | "core remembers lifetime-bound floating, sticky and tiled identities"; accepted focus records before native effects | docs/decisions.md:396-411 | D |
| E2.12 | 2026-10-28 / Visibility protocol model | "core-owned per-window visibility work (Model A)"; platforms schedule, core owns completion and meaning | docs/decisions.md:412-420 | D |
| E2.13 | D1 selected 2026-10-28; D2 user decision; D3-D5 user-accepted 2026-10-29 | Crossed-edge focus, allocation/history/overlap/DFS ranking; explicit no-focus origins and macOS integration exclusion | docs/decisions.md:421-456 | D |
| E2.14 | 2026-10-27 / Decision | Same-gesture completed preview may persist; borders/underlays and actuation remain exact-current | docs/decisions.md:462-475 | I |
| E2.15 | 2026-10-27 / Decision | Release-time pointer target uses core resolver, not lagging displayed preview | docs/decisions.md:479-486 | I |
| E2.16 | 2026-10-26 section / User-approved | "ToggleWorkspaceTiling" resolves retained mode; explicit setter remains; wire out/ws, never tiled | docs/decisions.md:499-504 | D |
| E2.17 | 2026-10-26 section / User-approved architecture | Birth/exit clear uses "clear_operation and raw ClearAttempt"; setter acceptance is not completion | docs/decisions.md:505-511 | D |
| E2.18 | 2026-10-26 / Tentative user decision | Birth-clear property authority may proceed under geometry hold, with fresh-native settlement | docs/decisions.md:521-531 | D |
| E2.19 | 2026-10-26 / Tentative technical decision | "deleted emits a close"; managed=false emits no WindowFact; no new core liveness fields | docs/decisions.md:532-538 | D |
| E2.20 | Date/attribution not separately stated | "birth_clear / wire birthClear, default false"; Windows keeps preamble route | docs/decisions.md:539-545 | D |
| E2.21 | 2026-10-16 / User decision | Catalog owns ids/types/defaults/bounds/timing; widgets/storage/load policy/operations platform-owned; preserve strict Windows versus tolerant KDE decoding | docs/decisions.md:1471-1493 | D |
| E2.22 | 2026-10-15 / User approved | Settings schema-v1 workspace tokens; missing backfills local, invalid refuses; adoption only on restart | docs/decisions.md:1793-1802 | D |
| E2.23 | 2026-10-16 / User, full-state replies D2/D3 | "KDE Planner is the sole workspace-policy authority"; no silent TypeScript fallback | docs/decisions.md:1761-1767 | D |
| E2.24 | Date/attribution not separately stated | Stable transfer/view targets; possibly-executed feedback before dispatch; ambiguous effects not repeated; focus/raise/stack at-most-once | docs/decisions.md:1768-1776 | D |
| E2.25 | 2026-10-26 / User; User-approved | Workspace focus MRU then deterministic first; no forced recovery from host focus loss | docs/decisions.md:1880-1891; docs/decisions.md:1907-1914 | D |
| E2.26 | 2026-10-26 / User; User-approved 2026-10-27 | No-focus directional command uses explicit empty win; missing/null/invalid and other commands retain strict validation | docs/decisions.md:1892-1906 | D |
| E2.27 | 2026-10-26 / User | Windows retains lifetime-bound in-flight focus identity; exact prior landing is not supersession, unreadable never proves landing | docs/decisions.md:1915-1928 | D |
| E2.28 | 2026-10-26 / User-approved | "command rejection or catchup-required refusal" services observation-only catchup same turn, no replay | docs/decisions.md:1935-1938 | D |
| E2.29 | 2026-10-29 / User | Explicit "Operation(SendToOutput)" uses output-send builder, retaining explicit workspace and follow/stay | docs/decisions.md:2241-2244 | I |
| E2.30 | 2026-10-29 / User | Admission-focus batch invariance; neutral keepAboveTiles tri-state, no authority on null, no user-above ownership | docs/decisions.md:2389-2404 | D |
| E2.31 | 2026-10-08 and 2026-10-10 / User decisions | Fixed classification requires usable min/max vectors; explicit same-lifetime tile intent wins automatic classification | docs/decisions.md:2405-2431; docs/decisions.md:2445-2448 | I |
| E2.32 | 2026-10-17 / User decision | Duplicate-window rejection unchanged; hydration failures skip writes and await revalidation, not core recovery authority | docs/decisions.md:2517-2525 | I |
| E2.33 | 2026-10-08 / User decisions | "persist explicit float/unfloat intent only after successful native application" | docs/decisions.md:2545-2570 | I |
| E2.34 | 2026-10-20 / User accepted; User | Common cursor/focused-output targeting; three-way new-window placement implemented in shared core | docs/decisions.md:3031-3061 | I |
| E2.35 | 2026-10-20 / User direction | Core owns outputs/workspaces/mapping/windows/focus/cursor; platform input, facts and native application only | docs/decisions.md:3207-3232 | D |
| E2.36 | 2026-10-20 / User approved | Core model includes "behavior settings"; output is desired frames, shown workspace and focus; limitations neutral options | docs/decisions.md:3233-3246 | D |
| E2.37 | 2026-10-21 / User approved | Logical mapping authoritative; host_owned_workspace_domains optional, equal_workspace_count independent | docs/decisions.md:3247-3263 | D |
| E2.38 | 2026-10-24 / Action-context ownership | Freeze source/current before incorporating report; registry fallback and explicit targets; send-policy mismatch parked | docs/decisions.md:3324-3338 | D |
| E2.39 | 2026-10-24 / World-contract cleanup | Remove external verdict feedback; "raw frame, clear, membership and view attempts; core derives settlement" | docs/decisions.md:3339-3346 | D |
| E2.40 | 2026-10-24 / Uniform world presentation | Every report carries foreground/style/movement/pause/stacking/frame-source facts; reply carries shapes and ordered pager; core source precedence | docs/decisions.md:3347-3358 | D |
| E2.41 | 2026-10-20 / User approved | Platforms report normalized type/owner/modal/size/control facts; "Core decides manage/float/ignore" | docs/decisions.md:3359-3363 | D |
| E2.42 | 2026-10-20 / User approved | Core decides reapply/adopt/refuse/float and "owns anti-fighting and completion" | docs/decisions.md:3364-3370 | D |
| E2.43 | 2026-10-22 / Frame convergence extraction | Lifetime-scoped write/readback drives clamp suppression/retry; core destination slot precedes arrival | docs/decisions.md:3371-3381 | D |
| E2.44 | 2026-10-22 / Learned-constraint extraction; User-approved 2026-10-26 heuristic | Core retains axis-minimum evidence; corroboration, timestamps and counter-evidence qualify learning | docs/decisions.md:3382-3402 | D |
| E2.45 | 2026-10-22 / Explicit float-placement completion | Frame attempts correlate operation/lifetime/target/preimage; core classifies refusal/ambiguity; no re-actuation after ambiguous write | docs/decisions.md:3403-3414 | D |
| E2.46 | 2026-10-23 / Transfer/view completion extraction | Correlated attempts plus native receipts settle; "an attempt row is not a completion prerequisite" | docs/decisions.md:3415-3426 | D |
| E2.47 | 2026-10-23 / Gesture lifecycle extraction | Stable nonzero gesture sequence, lifetime/start/release facts; consumed watermark; gestureSettled.committed proves core mutation only | docs/decisions.md:3440-3454 | D |
| E2.48 | 2026-10-20 / User approved; 2026-10-23 / Cursor focus/warp extraction | Core pager content, hover eligibility and warp targets; native last-moment vetoes and focus proof separate | docs/decisions.md:3489-3503 | D |
| E2.49 | 2026-10-21 / Boundary extraction | Shared ordered geometry/focus eligibility; adapters also call shared predicates directly | docs/decisions.md:3504-3512 | D |
| E2.50 | 2026-10-16 / User, full-state replies D1 | Retained side-effect-free Engine behind report/reply; no serialized core state; full observations/settings and complete desires/results/effects | docs/decisions.md:3513-3527 | D |
| E2.51 | Date/attribution not separately restated after D1 | Fresh generation/discovery, exact duplicate requests, bounded decimal u64 strings, command high-watermark and current watermark before setters | docs/decisions.md:3528-3541 | D |
| E2.52 | 2026-10-16 / User, D4/D5 | 4 MiB request/reply budget; validate before convergence, commit after bounded encoding; oversize mutates no candidate state | docs/decisions.md:3542-3547 | D |
| E2.53 | 2026-10-18 / User accepted (T1) | Constructor fixed_send_admission/maximized_focus_fence profiles preserve Windows gates; no wire policy fields | docs/decisions.md:3564-3572 | D |
| E2.54 | 2026-10-28 / User-approved | Explicit fullscreen float toggle changes base membership but authorizes no fullscreen subject writes | docs/decisions.md:4672-4688 | I |
| E2.55 | 2026-10-29 / User | ToggleMaximize and command-only Maximize result replace KDE-local choice; native writes explicit, no inferred success | docs/decisions.md:4830-4849 | D |
| E2.56 | 2026-10-29 / User (R-MAX-02) | Fullscreen directional focus skips same-output siblings; adjacent-output policy common to all platforms, implementation pending | docs/decisions.md:4854-4859 | D |
| E2.57 | 2026-10-13 / Amended by user, option C | Shared born-maximized policy/defaults; KDE TypeScript copy pinned by conformance table | docs/decisions.md:5110-5128 | I |
| E2.58 | 2026-09-14 / Authorized | Incoming normal-window frame never decides admission; malformed rectangles/homing/reply geometry still validated | docs/decisions.md:5147-5164 | I |
| E2.59 | 2026-09-09 / authorized | cosmic_v1 policy version 1, portable lifecycle semantics; alternative behavior belongs in separate versioned mode | docs/decisions.md:5200-5225 | D |
| E2.60 | 2026-10-26 / User-confirmed | Post-arm timeout is Threw, topology drift remains unproven; selection focus defers to native arrival; creation never replayed | docs/decisions.md:5353-5372 | D |
| E2.61 | 2026-10-26 / User-confirmed | Miniature reuses indicator gates; retained/native facts ground hidden eligibility; unknown cloak/session retains renderer veto | docs/decisions.md:5373-5384 | D |
| E2.62 | 2026-10-28 / User-confirmed | User FIFO bypasses saturation; bounded observations/feedback; select/send drain claims retained | docs/decisions.md:5455-5467 | D |
| E2.63 | Decision 7 approved 2026-10-28; User-accepted 2026-10-29 | Optional trailing marker, one classifier, missing false; core rows and designation drive lazy-backed macOS pager | docs/decisions.md:5512-5540 | D |
| E2.64 | 2026-10-24 / User-approved; tentative amendment 2026-10-25 | Native order bootstraps host-owned workspaces; adapters supply native-only facts, core translates/binds; Windows logical receipts unchanged | docs/decisions.md:5564-5591 | D |
| E2.65 | 2026-10-25 / Tentative | Optional current_native, authoritative order, unknown/fullscreen retains home without geometry writes | docs/decisions.md:5592-5601 | D |
| E2.66 | 2026-10-25 / Tentative hybrid creation correlation (option C) | Core creates/binds backing; singleton inference permitted without correlated id; AmbiguousCreation otherwise holds | docs/decisions.md:5602-5614 | D |
| E2.67 | 2026-10-29 / User; padding option 1 | Native cleanup uses global occupancy, neutral readability, deletion readback and stable padding identities; v3 now, visibility v4 | docs/decisions.md:5628-5651 | D |
| E2.68 | 2026-10-25 / Tentative native attempt settlement (option 1), user-selected | Native ids extend MemberAttempt/ViewAttempt only with capability; core translates and settles/adopts | docs/decisions.md:5652-5662 | D |
| E2.69 | 2026-10-25 / Tentative released-source backing (option 1), user-selected | Migration releases native backing; positive evidence required, partial lists/pending creations defer cleanup | docs/decisions.md:5663-5680 | D |
| E2.70 | 2026-10-25 / Tentative host-hidden representation, user-selected | Activity-hidden and Cmd+H map to WindowFact.minimized=true; unknown sticky home must not be fabricated | docs/decisions.md:5681-5692 | D |
| E2.71 | 2026-10-28 section / approved for a narrow prototype | Staged switch: show arrivals, prove visible, one-shot focus, prove foreground, hide departures; explicit-selection-only arrival-ready authority | docs/decisions.md:363-370 | D |
| E2.72 | 2026-10-28 section / R2, approved narrowly | Fresh revision-bound one-shot authority only if prior effect never claimed/primed/dispatched; old id retires atomically, no retry/grant copying | docs/decisions.md:371-378 | D |
| E2.73 | 2026-10-28 section / Empty-workspace focus across outputs; Coalescing extension | Empty selection has no focused window; broader intake coalescing deferred pending staged-switch/keyboard measurements | docs/decisions.md:379-395 | D |
| E2.74 | 2026-10-29 / User | Tab core/protocol post-0.1; v4 suppression capability/authority independent of workspace ownership; truthful hidden observations | docs/decisions.md:3991-3997 | I |
| E2.75 | 2026-10-26 / User | Games remain workspace members despite suspension | docs/decisions.md:2990-2993 | I |
| E2.76 | 2026-10-19 / User decision | Windows app-hide closes only after positive live/invisible/uncloaked/lifetime proof and no tiler hide claim or durable ledger; unknown holds | docs/decisions.md:5168-5182 | D |

### E3. Platform selectors, opt-ins and parity directions

| ID | Date / attribution as written | Selector or exception | Source | Basis |
|---|---|---|---|---|
| E3.01 | 2026-10-26 section / approved | KDE opts into core fullscreen suspension; Windows retains adapter path; Windows/macOS production unchanged | docs/decisions.md:180-195 | D |
| E3.02 | 2026-10-28 / N5 implemented offline | Windows safety exclusions unmanaged; KDE host-hidden eligibility uses retained path, legacy Windows/macOS unchanged | docs/decisions.md:200-209 | D |
| E3.03 | 2026-10-29 / User | No macOS float-placement exception; replace centered-60% fallback with current frame | docs/decisions.md:216-219 | D |
| E3.04 | 2026-10-26 section / approved | "no OmniTiler KDE pager; use the native Plasma pager" | docs/decisions.md:229-231 | D |
| E3.05 | 2026-10-28 / D1 selected; D2 user decision | Edge focus sides with Hyprland+sway; candidate selector is "option a, Hyprland default" | docs/decisions.md:421-442 | D |
| E3.06 | 2026-10-29 / D5 user-accepted | macOS hotkeys keep cross_output_transfer:false; new shared selector not enabled there | docs/decisions.md:452-456 | D |
| E3.07 | Date/attribution not separately stated | birthClear default false; KDE opts in; Windows/macOS do not; Windows preamble remains | docs/decisions.md:539-545 | D |
| E3.08 | 2026-10-15 / Tentative adapter defaults | macOS AX/owner/level/dialog classifications choose exclude/float behavior | docs/decisions.md:755-762 | D |
| E3.09 | 2026-10-16 / User direction; User; User-approved 2026-10-17 | macOS defaults consistent with KDE/Windows; consume-first matches Windows; eligibility outside callback | docs/decisions.md:821-833; docs/decisions.md:842-861 | I |
| E3.10 | 2026-10-19 / User-approved | macOS managed native workspaces supersede select-only fallback; route/follow-stay and current-Space no-op rules | docs/decisions.md:973-997 | I |
| E3.11 | 2026-10-16 / User decision | Preserve Windows strict/last-good versus KDE tolerant settings decoding and distinct platform keys | docs/decisions.md:1471-1493 | D |
| E3.12 | 2026-10-15 / User approved | Windows workspace tokens and restart adoption match KDE | docs/decisions.md:1793-1802 | D |
| E3.13 | 2026-10-26 / User | Workspace MRU/first sides with i3/sway and COSMIC; rejects Hyprland cursor fallback | docs/decisions.md:1880-1889 | D |
| E3.14 | 2026-10-26 / User | No-focus crossing follows i3/sway/Hyprland; possible later preset-selected COSMIC variant | docs/decisions.md:1892-1906 | D |
| E3.15 | 2026-10-18 / KDE aligned per-output decisions | Aligned per-output mapping replaces disjoint mapping and startup conversion; other platforms retain alternatives | docs/decisions.md:1981-1993 | I |
| E3.16 | 2026-10-17 / User decision | Windows hidden-workspace presentation keeps SW_HIDE for 0.1 | docs/decisions.md:2084-2088 | I |
| E3.17 | 2026-10-08 / User decisions, containing D2 | KDE local/global-unique requires strict perOutputVirtualDesktops capability; shared false/unreadable refuses | docs/decisions.md:2163-2170 | I |
| E3.18 | 2026-10-28 / User; 2026-10-29 / User | KDE opts into core above-tiles desire; Windows codec shared but automatic-fixed stacking opt-out | docs/decisions.md:2384-2404 | D |
| E3.19 | 2026-10-08 and 2026-10-10 / User decisions | Fixed-size predicate is explicitly "not plain COSMIC parity" | docs/decisions.md:2405-2425 | I |
| E3.20 | 2026-10-20 / User accepted; User | Common placement/targeting configurable across platforms; cursor versus focused presets grounded in Tier 1 | docs/decisions.md:3031-3061 | I |
| E3.21 | 2026-10-18 / User accepted (T1) | Constructor defaults false/true; Windows true/false for fixed sends and adapter maximized focus gates | docs/decisions.md:3564-3572 | D |
| E3.22 | 2026-10-18 / macOS complete-world mapping; User-approved 2026-10-19 | Per-output native Spaces, no invented creation/ordinary transfer capability; 18 select-only bindings retained | docs/decisions.md:3581-3598 | D |
| E3.23 | 2026-10-02 / selected | Windows layered border uses tiler-core::visual policy and KDE defaults/target-parented band parity; DWM colour-only carrier rejected | docs/decisions.md:3788-3800 | D |
| E3.24 | 2026-10-08 / Theme mapping; Windows source amended by user 2026-10-19 | Windows UISettings accent replaces DwmGetColorizationColor; border/pager theme independence | docs/decisions.md:3803-3816 | D |
| E3.25 | 2026-10-17 / User-approved interim route; revised | macOS AppKit border interim; SkyLight underlay/below-target experiment for KDE/Windows parity, product underlay withheld without ordering proof | docs/decisions.md:3843-3902 | I |
| E3.26 | 2026-10-11 / User decision | KDE 6.6+ support; missing per-output scripting capability resolves requested modes to shared | docs/decisions.md:4117-4125 | I |
| E3.27 | 2026-10-14 / User approved | Windows floating candidate encounter order is observed then retained maximized, with sticky-first layering | docs/decisions.md:4774-4783 | I |
| E3.28 | 2026-09-26 / User decision option 2 | KWin first-seen fullscreen stays outside tree; native-only planner floating hold; later fullscreen keeps tile | docs/decisions.md:4794-4805 | D |
| E3.29 | 2026-09-14 / authorized; User decision I 2026-09-26 | KWin maximize axes collapse to boolean; absent maximizedChanged remains tiled until later read | docs/decisions.md:4806-4818 | D |
| E3.30 | 2026-10-28 / User; 2026-10-29 / User | R-MAX-11 selects Hyprland replacement semantics; core command adopted KDE-side, Windows/macOS runtime separate | docs/decisions.md:4819-4849 | D |
| E3.31 | 2026-10-29 / User | Windows legacy float_toggle_refusal keeps fullscreen refusal until Windows integration | docs/decisions.md:4850-4853 | D |
| E3.32 | 2026-10-10 / User, G-06 | Both platforms follow COSMIC maximized focus/move; KDE flag local, Windows native guard and opt-in core fence | docs/decisions.md:4867-4883 | D |
| E3.33 | 2026-10-11 / User, option B, cross-platform; user amendment 2026-10-13 option C | Born-maximized shared setting; KDE keeps conformance-pinned TypeScript policy copy | docs/decisions.md:5086-5128 | I |
| E3.34 | 2026-09-09 / authorized | cosmic_v1 versioned parity and explicit exceptions; future Hyprland behavior separate mode, not adapter fallback | docs/decisions.md:5200-5234 | D |
| E3.35 | Established 2026-09-13, closed 2026-09-16 / COSMIC geometry parity | Gap defaults selected; retain shares without claiming exact COSMIC parity | docs/decisions.md:5235-5242 | D |
| E3.36 | 2026-10-28 / User decision | Native Spaces macOS default; parked/hidden virtual-workspace alternative only possible later opt-in | docs/decisions.md:5504-5507 | D |
| E3.37 | 2026-10-25 / Tentative amendment | Host-owned KDE/macOS native-only receipts versus Windows logical receipts | docs/decisions.md:5586-5591 | D |
| E3.38 | 2026-10-29 / User | Core cleanup of managed KDE native desktops, native minimum-two/equal-count padding; KDE mechanical eligibility/veto boundary | docs/decisions.md:5628-5651 | D |
| E3.39 | 2026-10-25 / Tentative released-source backing, user-selected | Released-source native-backing contract explicitly applies to macOS, not only KDE | docs/decisions.md:5663-5680 | D |
| E3.40 | 2026-10-25 / Tentative host-hidden representation, user-selected | KDE inactive-Activity and macOS Cmd+H share minimized encoding | docs/decisions.md:5681-5685 | D |
| E3.41 | 2026-10-19 / User decision | Windows app-initiated hide counts as close only under positive proof and absence of owned hide claims; MRU allocation release, reshow fresh | docs/decisions.md:5168-5182 | D |

### E4. Choices or retained authority delegated to adapters

| ID | Date / attribution as written | Delegation excerpt or retained choice | Source | Basis |
|---|---|---|---|---|
| E4.01 | 2026-10-28 / Visibility protocol model | Platforms own order/batching/skipping superseded unissued work within core rules | docs/decisions.md:412-419 | D |
| E4.02 | 2026-10-28 / approved | Native homing/foreground vetoes remain authoritative for sticky return-focus dispatch | docs/decisions.md:408-410 | D |
| E4.03 | Date/attribution not separately stated | Windows retains birth admission preamble and async-restore refusal gates; no automatic core birth-clear | docs/decisions.md:539-543 | D |
| E4.04 | Date/attribution not separately stated | "Classification edge cases may use sensible tentative adapter decisions" | docs/decisions.md:752-754 | D |
| E4.05 | 2026-10-15 / Tentative adapter defaults | AX/owner/level facts select admission, exclusion and dialog float defaults | docs/decisions.md:755-762 | D |
| E4.06 | 2026-10-16 / User decision | Widgets, storage, load policy and operations remain platform-owned; generic setter never invokes remediation | docs/decisions.md:1471-1493 | D |
| E4.07 | Date/attribution not separately stated | Possibly-executed feedback armed before dispatch; ambiguity retained; at-most-once focus/raise/stack | docs/decisions.md:1768-1776 | D |
| E4.08 | 2026-10-26 / User | Windows retains asynchronous in-flight focus identity and classifies exact prior landing versus supersession | docs/decisions.md:1915-1928 | D |
| E4.09 | 2026-10-12 / User decision, D01 | "KWin decides activation/focus and stacking"; honor native approval/denial, automatic focus fence | docs/decisions.md:2997-3008 | D |
| E4.10 | 2026-10-23 / Desired presentation extraction | Windows native lifetime/scope/freshness vetoes, DPI/anchoring/artwork/style and click conversion remain | docs/decisions.md:3302-3315 | D |
| E4.11 | 2026-10-22 / Explicit float-placement completion | Mechanical freshness/lifetime gates and at-most-once dispatch retained, not outcome classifier/policy memory | docs/decisions.md:3403-3414 | D |
| E4.12 | 2026-10-23 / Transfer/view completion extraction | Windows retains at-most-once dispatch and last-moment safety, not applied/ambiguous policy maps | docs/decisions.md:3415-3426 | D |
| E4.13 | 2026-10-23 / Cursor focus/warp extraction | "last-moment input, lifetime, shown-view and exact foreground vetoes" | docs/decisions.md:3494-3503 | D |
| E4.14 | 2026-10-18 / User accepted (T1) | Windows uses constructor profile preserving "adapter-owned maximized focus gates" | docs/decisions.md:3564-3572 | D |
| E4.15 | 2026-09-24 / user-approved | Host-synchronous paths may stay local if extraction changes latency/failure behavior | docs/decisions.md:3628-3637 | D |
| E4.16 | 2026-09-28 / user decision; user decision A 2026-09-26, interim | KWin event-driven difference classifier retains overlay/sticky/fullscreen/unreadable fences and three-strike geometry acceptance | docs/decisions.md:3653-3666 | D |
| E4.17 | Date not restated; user decision G 2026-09-26 for retry | KWin retains applied geometry/domain/flag evidence and one fresh pre-write replan, never after setters | docs/decisions.md:3667-3675 | D |
| E4.18 | Date/attribution not separately stated | Adapters retain ordering/ack/recovery/effects/UI/delivery authority; sequential KWin actuation and divergence accounting | docs/decisions.md:3697-3709 | D |
| E4.19 | 2026-10-17 / User-approved interim/revised routes | macOS renderer owner/Space/scope/freshness/lock fences; unknown hides, underlay must prove below-all-members | docs/decisions.md:3843-3902 | I |
| E4.20 | Date/attribution not separately stated | KWin owns above/below transition; adapter stores/restores prior pair and refuses unverifiable write | docs/decisions.md:4788-4793 | D |
| E4.21 | 2026-10-10 / User, G-06 | KDE maximized flag remains local; adapter enforces focus fence; Windows reads fresh retained flags | docs/decisions.md:4867-4883 | D |
| E4.22 | 2026-10-13 / Amended by user, option C | KDE keeps existing TypeScript born-maximized policy copy pinned by conformance cases | docs/decisions.md:5110-5128 | I |
| E4.23 | 2026-10-26 / User-confirmed | Adapter retains deferred selection-focus intent; arrival permits dispatch, refusal/supersession/unknown/outbox drops it | docs/decisions.md:5364-5368 | D |
| E4.24 | 2026-10-26 / User-confirmed | Adapter presentation eligibility combines retained home with fresh current and geometry predicates; renderer unknown-state vetoes | docs/decisions.md:5381-5384 | D |
| E4.25 | 2026-10-28 / User-confirmed | Adapter FIFO retains queued select/send drain claims, not busy-dropped | docs/decisions.md:5455-5467 | D |
| E4.26 | 2026-10-25 / Tentative amendment | Live native-id/handle lookup allowed; no native-to-logical table; translation/receipts core-owned | docs/decisions.md:5586-5591 | D |
| E4.27 | 2026-10-29 / User | Native deletion safety vetoes retained, but "No adapter-owned occupant eligibility" | docs/decisions.md:5636-5642 | D |

### E5. Candidate internal conflicts or unmarked supersessions

| ID | Earlier or first entry: brief quote, date/attribution | Other entry: brief quote, date/attribution | Comparison to resolve, not a verdict | Basis |
|---|---|---|---|---|
| E5.01 | "N5 workspace membership versus immediate eligibility (option B; pending)" (docs/decisions.md:20; User-approved 2026-10-26 index) | "N5, option B, implemented offline (2026-10-28)" (docs/decisions.md:200) | Index versus detailed implementation status | D |
| E5.02 | "Intent-only float toggle and implicit output targeting (option A; pending)" (docs/decisions.md:21; User-approved 2026-10-26 index) | "Output-targeting race audit, option A, implemented offline (2026-10-28)" (docs/decisions.md:220) | Same status appears pending in index | D |
| E5.03 | "Floating/exception/non-leaf command-memory storage stays parked" (docs/decisions.md:44; tentative index) | "Last active window per workspace (approved 2026-10-28)" and floating/sticky/tiled slot (docs/decisions.md:396-397) | Parked index versus approved storage | D |
| E5.04 | "Empty-workspace focus across outputs remains undecided" (docs/decisions.md:1889; User 2026-10-26 entry) | "selecting an empty workspace stays on" (docs/decisions.md:379; 2026-10-28 section) | Unresolved wording versus later selected behavior; continuation says that output with no focused window | D |
| E5.05 | "no ... edge-nearest policy is added" (docs/decisions.md:1900-1901; User 2026-10-26) | "no-focus origins use the same selector" (docs/decisions.md:443-446; D3 user-accepted 2026-10-29) | Remembered-target text versus crossed-edge no-focus selector; old paragraph not marked here | D |
| E5.06 | "Host-synchronous paths may stay in the adapter" (docs/decisions.md:3632-3634; user-approved 2026-09-24) | "without platform code participating in those decisions" (docs/decisions.md:3211-3214; User direction 2026-10-20) | Scope of retained host-synchronous decision exception | D |
| E5.07 | "sensible tentative adapter decisions" (docs/decisions.md:752-754; attribution not restated) | "Core decides manage/float/ignore" (docs/decisions.md:3362-3363; User approved 2026-10-20) | Classification authority versus edge-case delegation | D |
| E5.08 | "three ... reassertions ... accepts ... client-held rectangle" (docs/decisions.md:3659-3662; user decision A 2026-09-26, interim) | "core owns anti-fighting and completion" (docs/decisions.md:3364-3367; User approved 2026-10-20) | Local reconciliation acceptance versus extracted convergence ownership | D |
| E5.09 | "codec rejects requests above 1 MiB" (docs/decisions.md:3683; AR16, shipped offline, no date restated) | "one bounded 4 MiB request/reply budget" (docs/decisions.md:3542; User 2026-10-16 D4/D5) | Later full-state paragraph explicitly supersedes old boundary decisions at 3550; old AR16 text remains unmarked | D |
| E5.10 | "strict v2 wire with no compatibility route" (docs/decisions.md:3552; delivered 2026-10-17, v2 closing 2026-10-24) | "This coordinated exact cutover is v3" (docs/decisions.md:5640; User 2026-10-29) | Current contract claim versus later cutover; distinguish historical rearchitecture name from wire version | D |
| E5.11 | "exact v:3 cutover" includes suppression/work ids (docs/decisions.md:289-291; User-accepted review 2026-10-29) | "planned Windows visibility cutover becomes v4" (docs/decisions.md:5640-5642; User 2026-10-29) | Same-day cutover-number supersession; earlier accepted-review bullet not marked | D |
| E5.12 | "no ... ack/verify/pending" (docs/decisions.md:5243-5245; Approved 2026-09-20, USER step-3 2026-09-25) | "targets ... remain outstanding until observed confirmation" (docs/decisions.md:1768-1769; date not restated); core derives transfer/view settlement (docs/decisions.md:3415-3425; 2026-10-23) | Logical topology commit versus native operation settlement; may describe different layers | D |
| E5.13 | "no visual may show while maximized or fullscreen" (docs/decisions.md:3719-3720; Shared, date not stated) | "remaining-output pager content" (docs/decisions.md:191-192; fullscreen suspension approved, implemented offline 2026-10-27) | Meaning of visual and output scope; earlier next line names only border/group | D |
| E5.14 | "never actuates focus while a fullscreen window is involved" (docs/decisions.md:3001-3003; User decision 2026-10-12 D01) | fullscreen focus "continues to an adjacent output" (docs/decisions.md:4854-4858; User 2026-10-29 R-MAX-02) | Automatic activation boundary versus later explicit directional behavior; explicit-shortcut exception already appears at 3005 | D |
| E5.15 | "core ... without platform-specific policy" (docs/decisions.md:3251-3252; User approved 2026-10-21) | Windows constructor uses true/false "to preserve ... adapter-owned maximized focus gates" (docs/decisions.md:3567-3569; User accepted 2026-10-18 T1) | Neutral option/host mechanism versus behavior profile; neither row resolves classification | D |

### E6. Possibly stale references and mechanical checks

| ID | Decision reference and date/attribution | Check and result | Classification of evidence only | Basis |
|---|---|---|---|---|
| E6.01 | Stress acceptance link, docs/decisions.md:5438; 2026-10-27 dogfood section, decision confirmed | `ls docs/changes/platform-neutral-stress-acceptance.md docs/changes/archive/platform-neutral-stress-acceptance.md`: original missing, archive exists (exit 2 for missing operand). Targeted all-relative-link check reported this as its sole missing file target | Missing file link; anchor targets not comprehensively checked | D for path; I for link sweep |
| E6.02 | "strict v2 wire", docs/decisions.md:3551-3553; v2 closing outcome 2026-10-24 | `git grep -n -E 'WORLD_CONTRACT_VERSION|WORLD_PROTOCOL_VERSION' -- crates/tiler-core/src/world_report.rs crates/tiler-protocol/src/world_protocol.rs`: core constant is 3 at world_report.rs:41; protocol aliases it at world_protocol.rs:187 | Current-version wording candidate, not proof that historical v2 headings are wrong | D |
| E6.03 | "codec rejects requests above 1 MiB", docs/decisions.md:3682-3685; AR16 shipped offline, date not restated | Production request-budget search reports `WORLD_MAX_REQUEST_BYTES = 4 * 1024 * 1024` at core world_report.rs:43 and protocol world_protocol.rs:189 | Old budget wording; full-state supersession already stated at docs/decisions.md:3550 | I for constants; D for decision |
| E6.04 | WorldEngineConfig/fixed_send_admission/maximized_focus_fence, docs/decisions.md:3564-3572; User accepted 2026-10-18 T1 | Per-name `git grep -l -F` over crates/kwin finds all; config fields occur in world_engine.rs and Windows world_sequencer.rs | Existing names, not stale by existence test | I |
| E6.05 | Public CoreEvent/CoreReply/ManagedWorkspaces removal, docs/decisions.md:1785-1791; Windows adoption 2026-10-24 | Per-name grep finds CoreEvent/CoreReply internals in core and ManagedWorkspaces text in Windows removal comments; does not establish surviving public API | Explicit historical/removal wording; do not infer staleness from an internal occurrence | I |
| E6.06 | ActiveGroup removed path, docs/decisions.md:3355-3357; Uniform world presentation 2026-10-24 | Per-name grep finds ActiveGroup types in active_group.rs/world_presentation.rs | Statement concerns command/reply path, not absence of every internal group type | I |
| E6.07 | cosmic_v1, LayoutPolicy, ToggleMaximize/Maximize result, native options and DiscoverWorld/DescribeWorld; docs/decisions.md:5200; docs/decisions.md:3630; docs/decisions.md:4831-4832; docs/decisions.md:3254-3255; docs/decisions.md:3557 | Production symbol searches report occurrences; per-name follow-up confirms cosmic_v1 and command/result variants | No missing symbol established in these checked groups | I |
| E6.08 | Active change-note links at docs/decisions.md:93,345,691,878,1293,1307,1494,1664,1757,3563; dates belong to their containing entries | File checks report all ten targets exist; macos-phase-1-skeleton.md also has archive copy | No missing file established for this targeted group; duplicate basename alone is not a conflict | I |

### E8. Evidence/process detail embedded in decisions

| ID | Source range | Detail block, not a new behavioral choice | Basis |
|---|---|---|---|
| E8.01 | docs/decisions.md:172-179 | Freshness failing-first/offline/physical status following sequencing choice | D |
| E8.02 | docs/decisions.md:196-199 | Dual-output fullscreen proof and unconfirmed shortcuts | D |
| E8.03 | docs/decisions.md:205-211 | N5 regression consumers and implementation limits | D |
| E8.04 | docs/decisions.md:650-663 | Adoption percentages, proxy source and coverage caveats | I |
| E8.05 | docs/decisions.md:893-900 | Native arrival latency measurements | I |
| E8.06 | docs/decisions.md:1087-1109 | Triage tally and test-environment evidence mixed with selected gates | I |
| E8.07 | docs/decisions.md:1173-1198 | Owner-idle measurements, event coverage and unmeasured costs | I |
| E8.08 | docs/decisions.md:1212-1223 | Diagnostic limits and physical acceptance residuals | I |
| E8.09 | docs/decisions.md:1647-1715 | Pager retries/timing, dimensions, observed starts and acceptance mixed with style choices | I |
| E8.10 | docs/decisions.md:1818-1822 | Workspace test counts | I |
| E8.11 | docs/decisions.md:2097-2099; docs/decisions.md:2137-2139 | Workspace delivery/native gate status | I |
| E8.12 | docs/decisions.md:2229-2237 | Migration checks and aggregate test tally | I |
| E8.13 | docs/decisions.md:2357-2369 | Move/output native gates and acceptance status | I |
| E8.14 | docs/decisions.md:2431; docs/decisions.md:2468 | Dense fixed-admission delivery and test-status records | I |
| E8.15 | docs/decisions.md:2816-2848 | Probe/gate outcomes and remediation evidence mixed with product restrictions | I |
| E8.16 | docs/decisions.md:2961-2965 | Shortcut test-count/delivery status | I |
| E8.17 | docs/decisions.md:3113-3127 | Native hint availability and verification detail | I |
| E8.18 | docs/decisions.md:3500-3503 | Cursor-hook evidence and unmeasured native cost | D |
| E8.19 | docs/decisions.md:3778-3787 | Border-radius evidence and native follow-ups | I |
| E8.20 | docs/decisions.md:3817-3842 | Accent reliability, startup proof and display/DPI follow-up | I |
| E8.21 | docs/decisions.md:3858-3874 | macOS renderer cadence and physical follow-ups | I |
| E8.22 | docs/decisions.md:3887-3902 | Underlay fixture ordering evidence and evaluation limits | I |
| E8.23 | docs/decisions.md:3945-3948; docs/decisions.md:3959-3963 | Underlay acceptance and native timing/pixel evidence | I |
| E8.24 | docs/decisions.md:4843-4849 | Native maximize mechanism/version evidence and runtime-adoption residuals | D |
| E8.25 | docs/decisions.md:4884 | Dense maximized-navigation native evidence/remaining cases | D |
| E8.26 | docs/decisions.md:4932-4953 | Send/follow delivery and native proof status | I |
| E8.27 | docs/decisions.md:4973-5028 | Windows native gates, physical evidence and follow-ups mixed with rules | I |
| E8.28 | docs/decisions.md:5074-5108 | Born-maximized evidence and delivery mixed with later setting decision | I |
| E8.29 | docs/decisions.md:5388-5451 | Dogfood/stress measurements and follow-ups mixed with reviewed choices | I |
| E8.30 | docs/decisions.md:5458-5477; docs/decisions.md:5484-5500 | Test-layout/latency/receipt and priority process records | D |
| E8.31 | docs/decisions.md:5544-5560 | Release-gate counts, carried limits and selected scope | I |
| E8.32 | docs/decisions.md:5660-5662 | Settlement-pin count and delivery status | D |

- E8 ranges identify evidence-bearing passages; ranges containing an actual choice are labelled mixed rather than asserting the entire passage lacks decisions. No file/symbol/behavior change is proposed by this index.

## Evidence: package F (bug check)

- D = direct source read; I = indexed source evidence. These are static source findings, not runtime reproductions or severity assessments.

### F(a). macOS attempts, rejection, and observation catch-up

- D: `crates/tiler-macos/src/world_runtime.rs:327-349` drains all four pending attempt maps with `std::mem::take`: views, members, creations, frames. `:592-604` returns before draining for an empty native context, otherwise takes all four. The rows enter native report inputs at `:710-714` and synthetic inputs at `:795-799`.
- D: Ordinary dispatch builds that report, then calls `handle_report` (`crates/tiler-macos/src/world_dispatch.rs:530-558`). Core command rejection returns `WorldOutcome::Rejected` (`crates/tiler-core/src/world_engine.rs:1816-1818`); `handle_report` only commits its staged clone on accepted/command-consumed outcomes (`:1391-1407,1422`). Thus a rejected report does not commit those attempt rows to core.
- D: The macOS rejected arm calls `clear_flight()`, emits/counts the rejection, and returns `Refused { token: format!("rejected-{kind}"), select_operation: None }` (`crates/tiler-macos/src/world_dispatch.rs:742-784`). It neither reinserts the taken rows nor calls observation-only fallback. The local report then leaves scope. The consumed pending rows are not retained for redelivery on this path.
- D: Flight clearing is separate from attempt preservation: `clear_flight` sets `flight = None` and the dirty hint; its comment says "the next native event redispatches observation-only" (`crates/tiler-macos/src/world_sequencer.rs:271-277`). Flight stores request/observation/command identifiers, not attempts (`:255-267`). Command sequences remain spent (`:237-253,271-273`). This is not a same-turn observation submission.
- D: Not every `Refused` means core lost attempts. After an accepted reply, stale-observation/coherence refusal can invoke `maybe_retry` (`crates/tiler-macos/src/world_dispatch.rs:559-658`); core already incorporated that first report. Retry is conditional on current topology and no queued job able to incorporate (`:797-810,876-904`), opens `try_begin_request(None)`, and calls `dispatch_ticket` immediately (`:821-858`). Other accepted-reply refusal reasons do not use this retry (`:637-658`); the command-consumed reply-refusal arm also just clears/returns (`:723-730`). Empty-context refusal precedes the drain and preserves pending rows.
- D: Gesture core rejection likewise clears/returns without fallback (`crates/tiler-macos/src/world_dispatch.rs:487-499`). Direct workspace selection builds the same report and clears/returns on core rejection or reply refusal (`crates/tiler-macos/src/workspace_select.rs:1791-1829`). The ordinary selection caller's `Refused` arm stores a correlation or counts refusal, not a catch-up dispatch (`crates/tiler-macos/src/runtime_thread.rs:2065-2088`).
- D: `docs/decisions.md:1935-1937` says "command rejection or catchup-required refusal" services observation-only catch-up "on the same turn". The inspected macOS command-rejection paths have no such same-turn call. Its sequencer's request gate checks flight/counter availability (`crates/tiler-macos/src/world_sequencer.rs:239-268`); no Windows-style `CatchupRequired` gate was identified. No unconditional later observation after rejection is established by these paths.
- D: Windows keeps delivered attempt snapshots in the flight (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:468-477`); rejection/loss clears the flight while retaining dirty work and arming observation-only catch-up (`:570-593`). `CatchupRequired` blocks commands (`:432-434`). The full command leg services that refusal synchronously (`crates/tiler-windows/src/tiling_sys/world_tick.rs:2296-2346`); the superseded leg records prior pre-dispatch fence service (`:2319-2325`). On core rejection it calls `run_observation_tick` or native-free `run_observation_core_only` before returning (`:2377-2410`).
- D: KDE settles a rejected queued command as failed, then calls `dispatch_plain_fallback(plain_stamped)` in the same broker call (`crates/omnitiler/src/world_broker.rs:977-998`); the function's contract specifies "the same fresh facts observation-only" and no command replay (`:922-929`).

### F(b). KDE per-capture `hostOwned`

- D: `kwin/src/world-observe.ts:1332-1337` computes completeness separately and computes `hostOwned = desktopsReadable && screenEntries.every((s) => s.current !== "")`. This is capture-time readability/current-ID evidence, not a remembered host identity. A readability failure or empty current ID can therefore change the value for the same window/host between captures; no such flip was executed here. Serialization sends that boolean (`kwin/src/world-report.ts:1828-1835`).
- D: True emits empty logical view receipts plus native order/current/occupancy; false emits logical-binding-derived view receipts and `native: []` (`kwin/src/world-observe.ts:1443-1475`). Core rejects nonempty native rows or creation attempts when the capability is false (`crates/tiler-core/src/world_report.rs:2030-2035`).
- D: Core uses the current report's value directly: `native_authoritative` returns it; `effective_logical_receipt` returns `fact.observed_workspace` when false, but derives from complete native evidence when true (`crates/tiler-core/src/world_native.rs:85-129`). `view_echo_for` similarly switches explicit/native provenance (`:172-195`), and native convergence returns immediately when false (`:221-225`).
- D: Host-owned unknown membership retains existing homing and fences writes (`crates/tiler-core/src/world_engine.rs:1601-1634`). Native-free unknown membership can instead adopt the effective view after an output change or for a newcomer (`crates/tiler-core/src/world_members.rs:140-198`). Domain readability also requires complete inventory specifically when host-owned (`crates/tiler-core/src/world_observe.rs:394-404`); other completeness/readability checks still apply.
- D: Accepted reports retain the current report and update `last_views` using native echoes when true, logical receipts when false (`crates/tiler-core/src/world_engine.rs:1881-1922`). I: The core capability-reference search covered `world_report`, `world_observe`, `world_engine`, `world_model`, `world_members`, `world_native`, `world_publish`, `world_born`, and `world_settle`; no explicit previous-value comparison or wholesale retained-state reset on a flip was identified. The demonstrated change is current-report evidence handling, not a demonstrated reset or particular window movement.

### F(c). Hidden/off-workspace pager miniatures

- D: The blanket claim that hidden KDE/macOS windows have no miniature while Windows windows do is not established. Core rebuilds the three frame maps from ingress each report (`crates/tiler-core/src/world_presentation.rs:625-647`), requires an admitted member, and resolves `live > float > tile` (`:659-673`). Logical workspace members get a miniature only if that lookup succeeds (`:716-735`). There is no `WindowFact.rect` or allocation fallback in this lookup, and no off-workspace exclusion there.
- D: KDE sends every bounded `liveById` row with `live: w.minimized ? null : w.rect`, and always `float: null, tile: null` (`kwin/src/world-observe.ts:1644-1654`). The construction has no off-workspace test. A nonminimized off-workspace window present in that map can therefore supply a miniature; a window with no live source has no retained-slot fallback from KDE.
- D: macOS iterates `members.values()` and emits `live: Some(rect)` for positive-width/height windows, with both fallback fields `None` (`crates/tiler-macos/src/world_builder.rs:187-207`). Current-space eligibility sets the separate foreground-hidden flag afterwards (`:208-211`), not a pager-row exclusion. A positive-sized member frame can supply a miniature; no live row means no macOS fallback slot.
- D: Windows supplies observed/retained live rectangles and additionally fills float/tile slots from `state.float_rects`/`state.member_rects` (`crates/tiler-windows/src/tiling_sys/world_capture.rs:781-831`). Hide-commit seeds populate those caches by exact window lifetime (`crates/tiler-windows/src/tiling_sys/world_native.rs:1613-1634`). Thus Windows has fallback sources when fresh live geometry is absent; hidden status alone guarantees neither presence nor absence on any platform. Which off-workspace windows are observable in a particular native capture was not runtime-verified.

### F(d). `SwitchView` versus `SelectWorkspace`

- D: Core `SwitchView` calls `retain_view` directly; `SelectWorkspace` first has a `GlobalUnique` visible-elsewhere swap branch, including view/history updates (`crates/tiler-core/src/world_commands.rs:119-188`). They are distinct command paths, not aliases in this source.
- D: Windows' `SelectionArrivalGrant::selects` includes the five `Select*` variants and explicitly excludes `SwitchView` (`crates/tiler-windows/src/tiling_sys/world_diff.rs:355-368`). A grant further requires a fresh accepted delivery and that command's correlated explicit-focus effect (`:371-401`).
- D: macOS `selection_ticket_seq` includes the same five variants plus `SwitchView` (`crates/tiler-macos/src/world_dispatch.rs:1605-1620`); that predicate leads to `bind_selection_focus` (`:1991-1999`). Deferred focus additionally requires a correlated effect, live native Space binding, and focus baselines (`:1623-1649`), with arrival proof rather than immediate selection focus (`:1975-1979`). This is a difference in eligibility for selection-focus handling, not evidence that a current production caller sends `SwitchView`.
- I: Windows constructs `SelectWorkspace` in `crates/tiler-windows/src/tiling_sys/selection.rs:63-77`; macOS constructs it for indicator selection in `crates/tiler-macos/src/world_command_map.rs:86-97`. Searches of both production source trees found no `SwitchView` constructor.
- I: KDE exports separate `select-workspace` and `switch-view` constructors (`kwin/src/world-report.ts:455-457,558-560`), with no calls to those constructors found in `kwin/src/*.ts`. D: Its context reference collector treats both as carrying workspace references (`kwin/src/world-command-context.ts:48-52`); that collector does not itself implement selection/focus behavior. No KDE equivalent of the differing Windows/macOS selection-focus predicate was established.

### F(e). Allocation overlap/uniqueness search

- D: No concrete two-window disjoint-allocation or rectangle-uniqueness requirement was found in the inspected adapter paths. Windows hide seeds are keyed by member token (`crates/tiler-windows/src/tiling_sys/world_native.rs:1613-1634`); macOS maps frame operations by window and emits each `(window, rect)` as a window-identified command (`crates/tiler-macos/src/world_dispatch.rs:1487-1554`). These sites do not deduplicate equal rectangles.
- D: KDE's underlay chooses the lowest-stacked renderable member by UUID membership and native stack order, not allocation geometry (`kwin/native-effect/activewindowborder.cpp:526-557`). KDE/macOS `Raise`/`Stack` effects have no native actuation in the inspected dispatchers (`kwin/src/world-apply.ts:2266-2288`; `crates/tiler-macos/src/world_dispatch.rs:2098-2101`). KDE automatic-float stacking uses window IDs and live native safety flags (`kwin/src/world-native.ts:1046-1109,1177-1185`), not pairwise allocated-frame overlap in those blocks.
- I: Additional inspected candidates: macOS drag admission requires containment plus AX/WindowServer hits naming the exact focused window (`crates/tiler-macos/src/drag_session.rs:120-186`); Windows output targeting compares a rectangle to monitor bounds (`crates/tiler-windows/src/tiling_sys/directional_resolution.rs:4-24`), while observation snapshots native stack order (`crates/tiler-windows/src/tiling_sys/observation.rs:178-184,307-334`). None of these inspected candidates establishes allocation uniqueness.
- D: Reproducible structural search across `crates/tiler-windows/src`, `crates/tiler-macos/src`, `kwin/src`, `kwin/native-effect`, and `crates/omnitiler/src`: `rg -n '(HashMap|BTreeMap|HashSet|BTreeSet)<[^>]*(Rect|Frame)|(QHash|QMap|map|unordered_map)<[^,>]*(Rect|Frame)|Map<(Rect|WorldRect|Frame)|Set<(Rect|WorldRect|Frame)|allocation.*(unique|duplicate|overlap|intersect)|(unique|duplicate|overlap|intersect).*allocation' ... --glob '!**/*test*'`. Hits were identity/domain-keyed geometry or frame-command collections, not rectangle-keyed window allocations. File-name exclusion does not exclude inline tests.
- D: An untruncated `rg -n -i 'overlap|intersect|hit.?test|z.?order|stacking|raise|(HashMap|BTreeMap|HashSet|BTreeSet)<[^>]*(Rect|Frame)|allocation.*(unique|duplicate)|frame.*(overlap|duplicate)' crates/omnitiler/src` found service-call/set overlap comments, not window-allocation overlap logic. I: Candidate reads used `overlap|intersect|hit.?test|z.?order|stacking|raise|allocation` in Windows `dispatch`, `observation`, `directional_resolution`, `pending_seams`, `gesture_tick`; macOS `drag_session`, `drag_native`, `native_tile_backend`, `select_owner`, `focus`; KDE `world-apply.ts`/`gesturefacts.cpp`. This is search-limited absence, not an exhaustive proof over every native/FFI path.

### F(f). KDE group-highlight route

- D: `docs/decisions.md:3558-3562` describes forwarding active-group results to the existing endpoint "until its v2 port" moves rendering to uniform wire presentation. Current `kwin/src/world-entry.ts:1466-1514` instead wires a `ReadWorldPresentation` subscriber for accepted border/underlay/preview shapes to the renderer. Its methods resolve to `SetPresentation`, `HidePresentation`, `ClearPresentation` (`kwin/src/world-presentation.ts:75-77`).
- D: The native class still bears the name `GroupHighlightObject`, but its exported slots are presentation set/clear/hide and `WaitPresentationFacts`; set/clear/hide delegate to presentation handlers (`kwin/native-effect/activewindowborder.cpp:48-109`). The class name is not evidence of a separate legacy group-result route.
- D: Searching `kwin/src`, `kwin/native-effect`, and `crates` for `SetGroupHighlight|ClearGroupHighlight|GetGroupHighlightStatus|SetActiveGroup|setGroupHighlight|highlightGroup` found only deleted-method validation names at `kwin/native-effect/validate-metadata.cmake:128`, no producer/implementation of those old group methods. The demonstrated current group-underlay rendering route is uniform presentation, despite retained highlight naming.

### Observations and limits

- No additional important observation outside the six questions was identified.
- Native runtime outcomes, capture availability for particular hidden windows, and exhaustive allocation-uniqueness absence remain undetermined. The source paths distinguish core rejection from adapter refusal after core acceptance; those outcomes must not be conflated when describing attempt loss.

## Evidence: package G (steady-state performance)

### Method and measurement boundaries

- Offline synthetic reports, release build, one development machine, relative numbers. Plain `cargo` was available and used; no `mise` fallback. No native capture, actuation, compositor, broker, or transport was exercised.
- Matrix: 10/50/200 windows across 1/2/4 outputs, plus 50 windows/2 outputs with 20 rather than 2 logical workspaces per output. Each scenario/cell used 5 discarded warmups and 50 timed samples, each on an independently prepared engine. Thus S1 measures a fresh-sequence report after convergence, not the lifetime behavior of one engine receiving 50 consecutive reports.
- Output rectangles were 16000x6000, workareas 16000x5960, gaps zero. Nonoverlapping synthetic output origins were (0,0), (-16000,0), (0,6000), (-16000,6000). Initial frames were 300x200; windows were round-robin assigned to outputs, with no size hints, minimized/fullscreen/maximized/sticky flags, gesture, or presentation decorations. All inventories/screens were complete; mode was `per-output-local`, `hostOwned=false`, default tiling enabled.
- Preparation echoed desired output, logical workspace, frame/allocation, floating flag, and view receipts into subsequent facts. A bounded 20-report loop required two successive matching desired geometry/membership/view signatures, matching receipts, and zero held scopes/constraint clamps. Setup, engine construction, request-string construction, equality checks, status checks, printing, and final engine destruction were outside measured `handle_report`.
- S0: first report on a fresh engine, before any convergence. S1: unchanged converged facts with fresh observation/request sequences and request ID. S2: the same converged facts with only the first window's x coordinate increased by 25, representing an external move. S3: the same facts plus one fresh `ToggleWorkspaceTiling` command on the first output's first logical workspace. S3 therefore changes that workspace to floating; it is not a directional-focus benchmark.
- Every timed string was decoded by `tiler_protocol::world_protocol::decode_request`; decoded typed equality with the intended report was asserted. The decoded report was the actual core input. Every measured core outcome was strictly `WorldOutcome::Accepted`, not `Duplicate`, `CommandConsumed`, or `Rejected`: 50 per row, 2000 measured reports in total. Reply encoding used `tiler_protocol::world_protocol::encode_reply` outside core.
- Clock probes used `Instant` and thread-local counters around the named core expressions. `estimate` = `report_estimate`; `validate` = `validate_report_reason`; `digest` = `report_digest`; `upper` = `reply_upper_bound` including `domain_keys().len()`; `clone` = `self.clone()`; `reset` = incarnation reset only; `run` = all of `staged.run_accepted`; `reply_est` = `reply_estimate`; `canonical` = `encode_reply_canonical(...).len()` including temporary string destruction; `commit` = `*self = staged`, including destruction of the old engine; `cache` = `self.last_reply = Some(reply.clone())`, including replacement/destruction. Reset, commit, and cache do not share a span.
- Children inside `run`: `stage_facts` is that call alone; `settle` covers `converge_logical_pre`, view settlement, staged-domain construction, gesture binding, remaining settlement, restaging, and frame settlement; `reconcile` covers hold setup and the initial readable-domain sweep; `command` is `execute_command` alone; `publish` is `publish` alone. Settings/history/topology work, session-snapshot preparation, focus/gesture bookkeeping, command resweeps/restaging, and `finish` remain in `run` but outside those child spans. Children are not a complete partition. Total `handle` was timed externally through returned-outcome extraction. `protocol_total` is each sample's decode + handle + reply encode, not a broker/IPC measurement.
- All times below are microseconds, formatted as p50/p95. Quantiles use nearest rank (samples 25 and 48 of 50); individual phase quantiles need not sum to the total quantile. Probe clock/counter overhead is included in enclosing spans and not subtracted. Request/reply bytes are the first timed sample; sequence/revision digit changes can change other samples' sizes.

### Commands and checks

```sh
cargo test -p tiler-protocol --test package_g_scale --release -- --ignored --nocapture
cargo fmt --all -- --check
git diff --check
git diff --exit-code HEAD -- crates/tiler-core/src/world_engine.rs
git diff -- crates
git status --short
```

- Final benchmark: exit 0, `1 passed; 0 failed`, 138.12 seconds test elapsed; 800 phase rows across 40 scenario/cells. Format and whitespace checks passed. The temporary test named in the command was removed after measurement, so that command no longer names a retained test.

### Counts, byte sizes, and total core time

- `W/O/Ws` means total windows / outputs / logical workspaces per output. Total logical workspace count is O multiplied by Ws. `Domains` is the actual retained inner-engine session count immediately before publication, not the logical ring product: empty trailing workspaces have no session. S3 removes the toggled floating workspace's session.
- Fit/float counts were constant across all 50 samples in each row. Fit means a nonfloating tiled row with frame/allocation and no frame hold; float includes rows whose workspace is floating even if the per-window floating flag is false. Every reported window was represented. All rows had zero held scopes and zero constraint-clamped windows. No clamp/write-failure feedback was injected.

| Case | W/O/Ws | Domains | Fit/float | Request/reply bytes | Handle p50/p95 us |
|---|---|---:|---|---|---|
| S0 | 10/1/2 | 1 | 10/0 | 4011/3595 | 424.946/478.187 |
| S1 | 10/1/2 | 1 | 10/0 | 4089/3652 | 313.730/361.936 |
| S2 | 10/1/2 | 1 | 10/0 | 4089/3652 | 311.283/371.042 |
| S3 | 10/1/2 | 0 | 0/10 | 4142/3319 | 299.832/355.262 |
| S0 | 10/2/2 | 2 | 10/0 | 4195/4206 | 313.549/349.836 |
| S1 | 10/2/2 | 2 | 10/0 | 4250/4316 | 329.111/388.831 |
| S2 | 10/2/2 | 2 | 10/0 | 4253/4318 | 325.839/381.729 |
| S3 | 10/2/2 | 1 | 5/5 | 4306/4161 | 329.206/373.124 |
| S0 | 10/4/2 | 4 | 10/0 | 4520/5422 | 316.014/384.529 |
| S1 | 10/4/2 | 4 | 10/0 | 4575/5642 | 418.147/557.729 |
| S2 | 10/4/2 | 4 | 10/0 | 4575/5642 | 419.782/537.492 |
| S3 | 10/4/2 | 3 | 7/3 | 4628/5561 | 424.911/495.005 |
| S0 | 50/1/2 | 1 | 50/0 | 15857/13487 | 9249.813/9609.346 |
| S1 | 50/1/2 | 1 | 50/0 | 16076/13542 | 2937.989/2982.124 |
| S2 | 50/1/2 | 1 | 50/0 | 16076/13542 | 2522.845/2973.127 |
| S3 | 50/1/2 | 0 | 0/50 | 16129/12041 | 2403.811/2625.742 |
| S0 | 50/2/2 | 2 | 50/0 | 16128/14090 | 4892.458/5374.127 |
| S1 | 50/2/2 | 2 | 50/0 | 16234/14200 | 1996.769/2223.420 |
| S2 | 50/2/2 | 2 | 50/0 | 16234/14200 | 2243.036/2282.270 |
| S3 | 50/2/2 | 1 | 25/25 | 16287/13429 | 2113.659/2236.009 |
| S0 | 50/4/2 | 4 | 50/0 | 16446/15576 | 2733.801/2968.446 |
| S1 | 50/4/2 | 4 | 50/0 | 16692/15796 | 2089.929/2170.932 |
| S2 | 50/4/2 | 4 | 50/0 | 16692/15796 | 2094.112/2165.658 |
| S3 | 50/4/2 | 3 | 37/13 | 16745/15361 | 2109.642/2162.905 |
| S0 | 200/1/2 | 1 | 200/0 | 60221/50561 | 138410.916/146071.248 |
| S1 | 200/1/2 | 1 | 200/0 | 61013/50616 | 27190.991/31469.007 |
| S2 | 200/1/2 | 1 | 200/0 | 61013/50616 | 30279.848/31535.361 |
| S3 | 200/1/2 | 0 | 0/200 | 61066/44741 | 28787.386/30713.287 |
| S0 | 200/2/2 | 2 | 200/0 | 60879/51340 | 67166.906/74458.677 |
| S1 | 200/2/2 | 2 | 200/0 | 61259/51450 | 18129.355/19343.520 |
| S2 | 200/2/2 | 2 | 200/0 | 61259/51450 | 17836.244/18671.261 |
| S3 | 200/2/2 | 1 | 100/100 | 61312/48479 | 17070.950/18995.944 |
| S0 | 200/4/2 | 4 | 200/0 | 61241/52714 | 34969.062/40580.728 |
| S1 | 200/4/2 | 4 | 200/0 | 61661/52934 | 12657.130/13011.265 |
| S2 | 200/4/2 | 4 | 200/0 | 61661/52934 | 11798.240/13187.541 |
| S3 | 200/4/2 | 3 | 150/50 | 61714/51433 | 11893.448/13788.725 |
| S0* | 50/2/20 | 38 | 50/0 | 16319/24361 | 11397.248/12174.774 |
| S1 | 50/2/20 | 38 | 50/0 | 16319/24361 | 10559.845/11589.596 |
| S2 | 50/2/20 | 38 | 50/0 | 16323/24363 | 10707.065/12183.460 |
| S3 | 50/2/20 | 37 | 43/7 | 16377/24127 | 10652.958/12272.516 |

### Core ingress phases (p50/p95 us)

| Case W/O/Ws | estimate | validate | digest | upper |
|---|---|---|---|---|
| S0 10/1/2 | 1.194/1.374 | 1.415/1.595 | 4.322/4.882 | 0.733/0.860 |
| S1 10/1/2 | 1.133/1.382 | 1.367/1.659 | 4.942/5.682 | 0.888/1.039 |
| S2 10/1/2 | 1.165/1.413 | 1.396/1.565 | 5.070/6.052 | 0.898/1.052 |
| S3 10/1/2 | 1.280/1.492 | 1.453/1.704 | 6.033/7.266 | 0.976/1.103 |
| S0 10/2/2 | 1.297/1.506 | 1.533/1.814 | 5.419/6.340 | 0.767/0.883 |
| S1 10/2/2 | 1.305/1.977 | 1.770/2.105 | 5.694/6.789 | 1.150/2.206 |
| S2 10/2/2 | 1.279/1.618 | 1.767/1.974 | 5.641/6.703 | 1.166/1.384 |
| S3 10/2/2 | 1.405/1.642 | 1.785/2.072 | 6.742/9.995 | 1.194/1.410 |
| S0 10/4/2 | 1.540/2.001 | 2.109/2.479 | 7.005/8.409 | 0.891/1.100 |
| S1 10/4/2 | 1.486/1.771 | 2.360/2.680 | 7.104/8.198 | 1.371/1.608 |
| S2 10/4/2 | 1.503/1.841 | 2.323/2.781 | 7.152/10.655 | 1.378/1.636 |
| S3 10/4/2 | 1.609/2.300 | 2.385/2.767 | 8.169/9.955 | 1.429/1.681 |
| S0 50/1/2 | 3.239/4.243 | 6.586/7.489 | 13.904/16.780 | 1.965/2.259 |
| S1 50/1/2 | 3.199/3.371 | 7.935/8.211 | 15.705/17.120 | 2.616/2.721 |
| S2 50/1/2 | 2.840/4.122 | 6.713/8.174 | 13.461/17.075 | 2.248/2.685 |
| S3 50/1/2 | 3.248/4.598 | 6.623/8.129 | 15.188/20.391 | 2.247/2.659 |
| S0 50/2/2 | 2.760/4.276 | 6.493/7.804 | 14.749/17.186 | 1.820/2.143 |
| S1 50/2/2 | 2.997/4.335 | 7.278/8.291 | 15.307/19.075 | 2.371/2.749 |
| S2 50/2/2 | 3.352/4.230 | 7.903/8.701 | 16.904/19.100 | 2.657/2.905 |
| S3 50/2/2 | 3.481/5.368 | 7.678/8.262 | 18.059/20.881 | 2.580/2.870 |
| S0 50/4/2 | 3.138/4.871 | 8.162/9.410 | 17.477/25.608 | 2.114/2.427 |
| S1 50/4/2 | 3.701/4.918 | 8.818/10.005 | 20.066/22.061 | 2.985/3.411 |
| S2 50/4/2 | 3.699/4.696 | 9.009/10.264 | 20.193/25.869 | 2.996/3.183 |
| S3 50/4/2 | 3.956/5.143 | 8.932/9.847 | 22.163/23.664 | 3.023/3.245 |
| S0 200/1/2 | 8.270/9.866 | 28.896/42.211 | 41.540/51.392 | 5.655/8.353 |
| S1 200/1/2 | 10.379/12.426 | 28.187/30.929 | 50.537/55.618 | 7.260/8.256 |
| S2 200/1/2 | 10.338/12.469 | 29.462/37.998 | 51.964/55.966 | 7.420/9.914 |
| S3 200/1/2 | 10.950/14.435 | 29.200/34.340 | 54.854/65.123 | 7.389/8.212 |
| S0 200/2/2 | 8.763/10.198 | 30.496/35.652 | 51.935/56.264 | 5.973/6.558 |
| S1 200/2/2 | 10.894/12.747 | 32.330/36.834 | 56.001/61.120 | 7.706/8.305 |
| S2 200/2/2 | 10.518/12.182 | 32.112/39.131 | 55.088/60.093 | 7.701/8.239 |
| S3 200/2/2 | 11.152/13.118 | 31.491/36.652 | 56.520/63.850 | 7.646/8.363 |
| S0 200/4/2 | 8.931/12.489 | 32.004/37.569 | 51.685/65.717 | 6.056/6.921 |
| S1 200/4/2 | 10.552/12.017 | 33.305/38.731 | 56.565/60.116 | 8.153/8.928 |
| S2 200/4/2 | 9.857/12.699 | 32.411/40.475 | 52.747/68.117 | 7.634/10.865 |
| S3 200/4/2 | 11.026/13.992 | 33.295/38.999 | 57.046/73.032 | 8.021/10.023 |
| S0* 50/2/20 | 4.771/6.306 | 8.484/9.364 | 19.408/21.895 | 25.445/30.566 |
| S1 50/2/20 | 4.695/6.371 | 7.672/8.680 | 18.182/22.047 | 22.815/28.892 |
| S2 50/2/20 | 4.593/6.218 | 8.003/9.110 | 18.186/22.586 | 23.219/27.276 |
| S3 50/2/20 | 5.238/7.306 | 8.092/9.654 | 21.065/27.044 | 24.154/28.412 |

### Staging, size gates, and replacement (p50/p95 us)

| Case W/O/Ws | clone | reset | run | reply_est | canonical | commit | cache |
|---|---|---|---|---|---|---|---|
| S0 10/1/2 | 0.796/0.980 | 0.631/0.710 | 402.044/451.068 | 1.718/2.188 | 8.342/9.771 | 0.876/1.151 | 2.335/3.531 |
| S1 10/1/2 | 18.802/23.330 | 0.000/0.000 | 262.842/300.206 | 1.583/2.026 | 8.736/11.059 | 6.299/10.248 | 6.153/7.443 |
| S2 10/1/2 | 19.039/27.193 | 0.000/0.000 | 259.095/306.452 | 1.646/2.147 | 8.888/10.507 | 6.562/8.504 | 6.149/8.399 |
| S3 10/1/2 | 19.396/22.888 | 0.000/0.000 | 249.711/292.880 | 1.301/1.564 | 5.129/5.953 | 6.507/8.602 | 6.437/7.296 |
| S0 10/2/2 | 0.736/0.839 | 0.611/0.721 | 287.693/316.307 | 1.855/2.114 | 8.921/10.310 | 0.849/0.964 | 2.758/4.450 |
| S1 10/2/2 | 22.255/28.975 | 0.000/0.000 | 270.137/320.647 | 1.823/2.171 | 9.726/10.844 | 7.491/9.146 | 7.400/8.508 |
| S2 10/2/2 | 22.021/28.222 | 0.000/0.000 | 266.415/314.339 | 1.795/2.173 | 9.578/11.843 | 7.489/9.987 | 7.421/10.299 |
| S3 10/2/2 | 22.288/27.511 | 0.000/0.000 | 269.930/305.179 | 1.708/2.011 | 7.945/9.140 | 7.600/9.129 | 7.728/9.178 |
| S0 10/4/2 | 0.782/0.985 | 0.600/0.715 | 283.783/333.864 | 2.140/2.973 | 10.297/26.059 | 0.899/1.035 | 3.749/4.370 |
| S1 10/4/2 | 26.832/33.659 | 0.000/0.000 | 341.680/476.196 | 2.157/3.423 | 11.183/15.317 | 9.254/13.319 | 10.147/12.590 |
| S2 10/4/2 | 27.205/33.789 | 0.000/0.000 | 340.567/449.192 | 2.178/2.540 | 11.152/13.683 | 9.391/12.168 | 10.025/12.228 |
| S3 10/4/2 | 26.936/35.215 | 0.000/0.000 | 347.472/410.619 | 2.116/2.911 | 10.377/13.584 | 9.310/12.506 | 10.149/12.111 |
| S0 50/1/2 | 2.123/2.958 | 0.962/1.475 | 9176.528/9521.117 | 6.633/7.888 | 29.282/35.031 | 1.907/2.977 | 6.794/7.479 |
| S1 50/1/2 | 78.822/85.136 | 0.000/0.000 | 2729.897/2776.160 | 5.339/6.851 | 32.493/34.452 | 27.624/33.152 | 27.140/28.896 |
| S2 50/1/2 | 67.465/81.534 | 0.000/0.000 | 2348.763/2770.588 | 4.814/6.442 | 27.738/36.174 | 24.002/36.726 | 23.226/28.154 |
| S3 50/1/2 | 66.681/98.100 | 0.000/0.000 | 2247.093/2428.131 | 3.218/5.102 | 12.561/19.032 | 24.892/36.337 | 23.079/29.072 |
| S0 50/2/2 | 1.356/2.487 | 0.819/1.185 | 4816.965/5293.321 | 5.461/7.518 | 27.118/32.699 | 1.558/2.410 | 6.184/7.264 |
| S1 50/2/2 | 71.036/83.105 | 0.000/0.000 | 1806.186/2016.395 | 5.063/6.775 | 30.091/42.680 | 25.537/40.796 | 24.365/27.371 |
| S2 50/2/2 | 79.209/89.891 | 0.000/0.000 | 2038.608/2060.151 | 5.526/6.967 | 33.676/36.427 | 27.411/30.432 | 27.520/29.355 |
| S3 50/2/2 | 74.134/81.600 | 0.000/0.000 | 1922.422/2036.206 | 4.632/6.148 | 24.607/35.310 | 26.635/30.633 | 26.928/28.630 |
| S0 50/4/2 | 1.160/3.102 | 0.870/1.544 | 2648.510/2859.105 | 6.319/8.377 | 33.909/40.431 | 1.501/3.547 | 7.964/11.366 |
| S1 50/4/2 | 85.763/93.003 | 0.000/0.000 | 1861.231/1915.115 | 6.310/8.309 | 37.903/42.066 | 29.847/37.894 | 30.617/34.545 |
| S2 50/4/2 | 86.293/101.560 | 0.000/0.000 | 1861.979/1929.393 | 6.255/8.839 | 37.964/42.617 | 29.665/38.550 | 30.546/34.824 |
| S3 50/4/2 | 86.230/91.638 | 0.000/0.000 | 1880.086/1911.001 | 5.885/7.682 | 33.306/38.610 | 29.821/36.727 | 31.056/32.475 |
| S0 200/1/2 | 3.676/4.533 | 1.971/3.044 | 138179.439/145858.903 | 17.840/23.451 | 93.017/101.477 | 3.257/3.886 | 18.726/22.710 |
| S1 200/1/2 | 270.543/337.070 | 0.000/0.000 | 26470.982/30692.211 | 17.526/19.679 | 103.300/120.701 | 139.280/166.495 | 84.413/95.041 |
| S2 200/1/2 | 268.145/294.827 | 0.000/0.000 | 29506.916/30802.866 | 17.724/19.939 | 107.783/115.003 | 133.705/168.966 | 86.617/96.016 |
| S3 200/1/2 | 268.339/297.234 | 0.000/0.000 | 28142.496/30032.768 | 11.690/14.798 | 41.266/50.068 | 129.763/160.362 | 85.945/96.675 |
| S0 200/2/2 | 3.454/4.131 | 1.875/2.908 | 66933.888/74185.077 | 18.655/21.561 | 99.781/112.550 | 3.163/4.341 | 19.617/22.247 |
| S1 200/2/2 | 277.004/328.609 | 0.000/0.000 | 17374.041/18572.185 | 18.174/25.406 | 111.912/124.075 | 129.477/163.550 | 88.521/96.282 |
| S2 200/2/2 | 266.040/307.344 | 0.000/0.000 | 17121.407/18018.760 | 17.826/22.418 | 109.650/116.893 | 129.207/161.658 | 86.927/102.739 |
| S3 200/2/2 | 268.307/306.083 | 0.000/0.000 | 16316.526/18254.550 | 14.896/17.707 | 75.568/86.597 | 126.840/172.525 | 88.251/107.755 |
| S0 200/4/2 | 3.365/4.646 | 1.653/2.953 | 34714.395/40299.003 | 18.717/24.341 | 104.141/129.908 | 3.200/4.802 | 20.062/26.496 |
| S1 200/4/2 | 273.963/318.273 | 0.000/0.000 | 11865.520/12231.683 | 18.597/25.388 | 115.363/134.903 | 129.778/176.305 | 91.402/106.200 |
| S2 200/4/2 | 260.032/322.399 | 0.000/0.000 | 11067.409/12434.868 | 17.233/21.195 | 107.467/124.218 | 114.826/166.650 | 83.289/114.816 |
| S3 200/4/2 | 283.529/377.368 | 0.000/0.000 | 11176.520/12958.213 | 16.758/20.449 | 98.316/113.923 | 128.261/169.513 | 88.746/109.472 |
| S0* 50/2/20 | 216.242/243.200 | 0.000/0.000 | 10853.666/11543.958 | 9.347/11.243 | 50.321/55.198 | 122.282/169.344 | 86.163/95.953 |
| S1 50/2/20 | 207.201/244.231 | 0.000/0.000 | 10004.658/11054.230 | 9.027/10.967 | 47.121/53.532 | 130.291/173.545 | 82.693/95.891 |
| S2 50/2/20 | 208.310/235.338 | 0.000/0.000 | 10188.164/11551.017 | 9.430/12.530 | 49.550/61.437 | 119.417/167.855 | 82.350/95.192 |
| S3 50/2/20 | 206.978/240.841 | 0.000/0.000 | 10159.209/11612.305 | 8.905/16.146 | 45.842/59.202 | 119.727/161.388 | 89.204/104.581 |

### Children inside run (p50/p95 us)

| Case W/O/Ws | stage_facts | settle | reconcile | command | publish |
|---|---|---|---|---|---|
| S0 10/1/2 | 7.166/8.362 | 11.972/15.546 | 307.041/347.380 | 0.000/0.000 | 43.338/52.060 |
| S1 10/1/2 | 4.747/6.676 | 10.694/12.693 | 163.267/191.175 | 0.000/0.000 | 41.877/48.941 |
| S2 10/1/2 | 4.884/5.810 | 10.940/13.269 | 159.966/190.165 | 0.000/0.000 | 41.550/52.148 |
| S3 10/1/2 | 4.820/5.461 | 10.985/12.669 | 162.094/190.219 | 5.656/7.190 | 17.201/20.059 |
| S0 10/2/2 | 7.064/8.310 | 14.046/15.950 | 193.369/216.626 | 0.000/0.000 | 41.190/46.978 |
| S1 10/2/2 | 5.199/6.173 | 13.363/15.966 | 159.014/196.021 | 0.000/0.000 | 42.590/50.604 |
| S2 10/2/2 | 5.219/6.486 | 13.114/16.924 | 157.067/188.459 | 0.000/0.000 | 42.108/53.948 |
| S3 10/2/2 | 5.234/6.180 | 13.403/17.709 | 158.769/181.290 | 5.903/6.996 | 32.202/38.812 |
| S0 10/4/2 | 7.689/8.929 | 16.573/20.124 | 175.259/207.445 | 0.000/0.000 | 46.087/53.850 |
| S1 10/4/2 | 5.525/6.504 | 16.707/19.216 | 210.810/252.075 | 0.000/0.000 | 48.854/62.052 |
| S2 10/4/2 | 5.651/6.514 | 16.815/19.953 | 210.075/245.246 | 0.000/0.000 | 49.242/58.053 |
| S3 10/4/2 | 5.489/6.548 | 16.736/20.525 | 210.006/246.379 | 5.217/7.416 | 43.757/50.907 |
| S0 50/1/2 | 32.112/46.906 | 56.126/62.064 | 8666.701/8989.790 | 0.000/0.000 | 279.947/293.742 |
| S1 50/1/2 | 27.221/30.555 | 57.990/63.184 | 2162.768/2197.628 | 0.000/0.000 | 277.040/286.374 |
| S2 50/1/2 | 23.265/27.933 | 50.751/63.441 | 1856.912/2192.004 | 0.000/0.000 | 233.908/281.812 |
| S3 50/1/2 | 22.735/28.709 | 49.116/69.561 | 1852.647/2035.882 | 21.877/31.281 | 63.914/95.321 |
| S0 50/2/2 | 27.361/30.713 | 53.168/63.811 | 4363.873/4798.197 | 0.000/0.000 | 239.603/281.876 |
| S1 50/2/2 | 23.603/26.743 | 54.987/61.615 | 1291.991/1426.023 | 0.000/0.000 | 239.338/277.061 |
| S2 50/2/2 | 26.738/27.511 | 62.000/65.357 | 1455.872/1474.137 | 0.000/0.000 | 271.423/281.647 |
| S3 50/2/2 | 26.086/31.452 | 59.593/69.086 | 1388.805/1466.006 | 20.706/23.817 | 172.513/182.838 |
| S0 50/4/2 | 31.303/36.508 | 64.785/80.175 | 2135.144/2309.914 | 0.000/0.000 | 258.744/285.409 |
| S1 50/4/2 | 27.209/30.166 | 69.439/75.478 | 1255.487/1281.847 | 0.000/0.000 | 268.749/278.303 |
| S2 50/4/2 | 27.191/29.634 | 69.238/75.736 | 1252.424/1300.369 | 0.000/0.000 | 268.690/282.798 |
| S3 50/4/2 | 27.180/30.184 | 69.426/74.449 | 1252.780/1274.207 | 21.097/23.724 | 225.382/230.745 |
| S0 200/1/2 | 106.786/120.548 | 269.231/293.205 | 136017.344/143899.221 | 0.000/0.000 | 1226.278/1309.048 |
| S1 200/1/2 | 109.182/119.338 | 276.675/325.535 | 24215.895/28190.345 | 0.000/0.000 | 1114.438/1261.151 |
| S2 200/1/2 | 109.458/116.948 | 290.840/304.017 | 27035.074/28307.875 | 0.000/0.000 | 1209.221/1274.903 |
| S3 200/1/2 | 107.759/115.256 | 284.523/309.195 | 26399.424/28198.803 | 88.306/99.948 | 271.415/294.097 |
| S0 200/2/2 | 113.823/120.580 | 289.487/317.826 | 65011.228/71805.805 | 0.000/0.000 | 1183.782/1309.835 |
| S1 200/2/2 | 109.584/128.999 | 302.663/326.913 | 14931.088/16145.012 | 0.000/0.000 | 1152.004/1237.067 |
| S2 200/2/2 | 105.420/114.914 | 294.116/312.960 | 14194.184/15129.393 | 0.000/0.000 | 1142.515/1198.532 |
| S3 200/2/2 | 107.437/125.149 | 292.791/323.277 | 14257.501/15854.712 | 75.912/84.866 | 692.876/765.143 |
| S0 200/4/2 | 108.872/124.979 | 298.014/337.754 | 32583.487/37845.166 | 0.000/0.000 | 1153.144/1307.001 |
| S1 200/4/2 | 106.949/120.072 | 314.172/333.884 | 9322.125/9591.993 | 0.000/0.000 | 1182.226/1250.573 |
| S2 200/4/2 | 104.415/124.221 | 310.727/355.147 | 8650.642/9791.565 | 0.000/0.000 | 1093.967/1352.669 |
| S3 200/4/2 | 102.340/118.731 | 300.252/350.313 | 8735.737/10161.194 | 68.785/85.772 | 929.264/1043.350 |
| S0* 50/2/20 | 30.423/33.955 | 115.266/129.778 | 9657.917/10311.225 | 0.000/0.000 | 606.628/660.543 |
| S1 50/2/20 | 27.857/32.920 | 105.377/120.752 | 8864.739/9848.722 | 0.000/0.000 | 560.338/655.180 |
| S2 50/2/20 | 29.083/34.824 | 108.466/122.447 | 8975.295/10241.007 | 0.000/0.000 | 576.574/671.463 |
| S3 50/2/20 | 28.896/33.798 | 108.534/125.814 | 9024.805/10234.792 | 42.690/50.003 | 549.924/629.290 |

### External protocol codec context (p50/p95 us)

| Case W/O/Ws | decode | reply_encode | protocol_total |
|---|---|---|---|
| S0 10/1/2 | 47.617/54.189 | 7.313/8.898 | 474.505/537.251 |
| S1 10/1/2 | 51.413/75.488 | 8.109/9.685 | 374.318/446.605 |
| S2 10/1/2 | 52.211/65.304 | 8.274/10.348 | 372.653/441.181 |
| S3 10/1/2 | 53.754/62.040 | 4.503/5.338 | 358.840/424.538 |
| S0 10/2/2 | 45.730/52.797 | 8.061/9.532 | 367.916/406.998 |
| S1 10/2/2 | 56.482/64.595 | 9.536/10.884 | 396.966/462.245 |
| S2 10/2/2 | 56.642/67.271 | 9.333/10.879 | 392.175/456.335 |
| S3 10/2/2 | 57.410/66.345 | 7.479/11.020 | 397.949/443.438 |
| S0 10/4/2 | 51.074/62.946 | 9.029/10.651 | 376.998/445.906 |
| S1 10/4/2 | 61.164/68.704 | 10.337/15.941 | 489.644/631.714 |
| S2 10/4/2 | 61.700/75.099 | 10.387/15.193 | 494.443/616.764 |
| S3 10/4/2 | 61.252/79.352 | 9.363/11.083 | 499.409/591.442 |
| S0 50/1/2 | 195.179/208.193 | 26.721/31.394 | 9477.992/9848.115 |
| S1 50/1/2 | 222.716/246.521 | 29.355/31.272 | 3193.247/3245.448 |
| S2 50/1/2 | 185.904/227.144 | 25.327/33.200 | 2735.383/3228.345 |
| S3 50/1/2 | 186.417/250.647 | 10.024/12.961 | 2621.747/2820.350 |
| S0 50/2/2 | 175.390/201.454 | 24.017/29.195 | 5101.532/5602.859 |
| S1 50/2/2 | 194.802/231.365 | 26.113/29.396 | 2223.504/2473.587 |
| S2 50/2/2 | 214.024/221.978 | 29.556/30.824 | 2485.431/2525.263 |
| S3 50/2/2 | 204.865/223.605 | 20.383/23.577 | 2342.951/2486.240 |
| S0 50/4/2 | 199.764/214.552 | 30.196/37.870 | 2968.060/3209.366 |
| S1 50/4/2 | 221.599/235.490 | 33.584/38.316 | 2344.149/2430.727 |
| S2 50/4/2 | 224.096/239.619 | 33.839/57.218 | 2353.073/2428.465 |
| S3 50/4/2 | 224.572/233.959 | 28.745/30.370 | 2363.387/2411.726 |
| S0 200/1/2 | 692.972/744.201 | 82.511/90.063 | 139199.215/146854.916 |
| S1 200/1/2 | 719.636/801.786 | 94.282/101.193 | 27932.637/32312.303 |
| S2 200/1/2 | 722.574/779.241 | 94.839/120.036 | 31145.656/32365.633 |
| S3 200/1/2 | 751.391/811.904 | 30.629/40.339 | 29565.471/31534.091 |
| S0 200/2/2 | 714.260/760.096 | 88.519/98.226 | 67989.059/75337.513 |
| S1 200/2/2 | 763.600/833.086 | 99.291/106.359 | 18993.811/20280.761 |
| S2 200/2/2 | 747.912/790.841 | 95.266/104.920 | 18679.242/19429.061 |
| S3 200/2/2 | 753.247/854.961 | 66.260/76.494 | 17974.954/19880.134 |
| S0 200/4/2 | 688.406/769.012 | 95.861/129.418 | 35702.396/41446.187 |
| S1 200/4/2 | 757.393/825.219 | 99.497/111.542 | 13529.958/13895.376 |
| S2 200/4/2 | 698.761/795.249 | 92.350/109.034 | 12571.312/13943.631 |
| S3 200/4/2 | 733.090/861.320 | 82.506/95.296 | 12651.537/14743.839 |
| S0* 50/2/20 | 235.476/253.126 | 43.138/49.368 | 11688.414/12472.270 |
| S1 50/2/20 | 216.342/245.603 | 41.494/56.816 | 10809.433/11867.694 |
| S2 50/2/20 | 226.366/252.137 | 41.658/50.933 | 10963.995/12484.011 |
| S3 50/2/20 | 232.416/260.619 | 37.780/44.665 | 10951.498/12564.981 |

### Observations, anomalies, and limits

- S1 `run` medians are 82-97% of the corresponding total medians. The initial reconciliation/sweep is the largest measured child: about 48-89% of total medians. These are ratios of separate medians, not per-sample attribution. At 200 windows, S1 clone medians are 270.543/277.004/273.963 us for 1/2/4 outputs; discarded canonical encoding is 103.300/111.912/115.363 us. These quantities do not include an adapter-side second clone or broker reparsing.
- First-report median at 200 windows decreases from 138410.916 us on one output to 34969.062 us on four outputs; S1 decreases from 27190.991 to 12657.130 us. At 50 windows S1 on four outputs (2089.929 us) is slightly slower than two outputs (1996.769 us). These runs do not establish an asymptotic complexity model or explain the historical pre-v3 64-domain results.
- Enlarged rings were created outside timing with `SelectTrailing` and workspace receipts, distributing windows among occupied workspaces; they were then reconverged. Two workspaces/output means four logical ring entries and two retained sessions; twenty/output means forty entries and 38 sessions. S1 total rises from 1996.769 to 10559.845 us (about 5.29x), reconciliation from 1291.991 to 8864.739 us (about 6.86x), clone from 71.036 to 207.201 us, and upper-bound work from 2.371 to 22.815 us. The extra domains and changed window distribution are confounded: this is not an isolated per-empty-domain cost measurement.
- S0* is explicitly `S0-first-after-ring-setup`, not a fresh engine's first-ever report. The public setup route required prior accepted reports to construct twenty workspaces/output; a literal enlarged-ring fresh-engine S0 was not constructed. Baseline S0 is literal first report in all nine main cells. No other requested main-matrix scenario was omitted.
- Cases ran in fixed blocks, without randomization or repeated independent complete matrices, scheduler isolation, or uninstrumented A/B timing. Variation such as S2 being faster than S1 at 50 windows/one output does not establish a beneficial external-change fast path. p95 from 50 samples has limited tail resolution. No live end-to-end latency, ordinary-sized-workarea capacity, native-owned-workspace behavior, rejection/duplicate cost, long-lived snapshot accumulation, or inner sweep subphase attribution is established.
- Earlier exploratory measurements were superseded, not merged into these tables. The initial harness did not prove convergence, rebuilt too many S2 facts, omitted command data from S3 wire JSON, and lacked full decoding/quantile coverage. Construction attempts also encountered invalid directional-focus wire encoding, a directional-focus no-op, out-of-range synthetic geometry, and an assertion that incorrectly treated floating-workspace rows as missing tiled geometry. The final run instead uses exact decoded report equality, a workspace-tiling command, valid output geometry, strict accepted-only outcomes, and reconvergence after enlarged-ring setup.
- observation (out of brief): logical workspace counts and actual retained session counts differ, and a workspace-tiling command changes the latter during the measured report. Both counts are recorded rather than using the ring product as the clone/session-domain count. No additional out-of-brief behavior claim was established.

### Temporary edits removed and final repository verification

- Modified then fully restored: `crates/tiler-core/src/world_engine.rs` (temporary probe struct/accessors/counters and clock spans, including actual-domain count). Final contents match HEAD.
- Created then deleted: `crates/tiler-protocol/tests/package_g_scale.rs` (ignored scale harness, report builder/echo preparation, workspace setup, timers, status assertions, and quantile output).
- These were the only temporary source edits/files. No Cargo manifest or dependency changes remain. Captured command output and a pre-append document comparison copy were scratch evidence outside the repository, not source/harness artifacts.
- After removal, `git diff -- crates` and `git diff --exit-code HEAD -- crates/tiler-core/src/world_engine.rs` produced no output; the harness was absent and probe-name search had no matches. The pre-existing document contents preceding this section were preserved. Final `git status --short` matches the recorded pre-run status; the review file remains untracked and contains this appended section:

```text
 M docs/backlog.md
 M docs/changes/windows-workspace-return-focus.md
 M docs/decisions.md
?? docs/changes/core-interface-review.md
```

## Evidence: package H (coordinate basis)

- D = direct source evidence; I = inference from cited evidence; U = unverified. Static inspection only; native behavior and visual equivalence were not exercised. These are evidence, not product decisions.

### H(1). Platform units, spaces, and conversion boundaries

- D: Windows verifies Per-Monitor-V2 awareness before geometry calls and refuses a wrong context because rectangles could be virtualized (`crates/tiler-windows/src/tiling_sys/monitors.rs:4-18`). Monitor full/work rectangles are physical pixels (`:52-77`), captured from `GetMonitorInfoW` (`crates/tiler-windows/src/ffi/enumeration.rs:67-83`) and copied directly into reports (`crates/tiler-windows/src/tiling_sys/world_capture.rs:1025-1033`).
- D: Windows window facts use DWM visible-frame bounds, not the outer `GetWindowRect` rectangle; both are read to measure insets (`crates/tiler-windows/src/tiling_sys/observe.rs:1291-1302,1332-1338`; `crates/tiler-windows/src/tiling_sys/world_capture.rs:959-979`). Native outer-track size hints are documented physical under PMv2 and normalized to visible-frame dimensions (`crates/tiler-windows/src/tiling_sys/minmax.rs:73-81,300-327`).
- D: Windows cursor capture/warp directly uses integer `GetCursorPos`/`SetCursorPos` coordinates (`crates/tiler-windows/src/ffi/input.rs:12-27`). I: Under the verified PMv2 basis, pointer and native move-frame geometry share the physical desktop space; the shared gesture contract uses integer cursor/frame facts (`crates/tiler-core/src/gesture.rs:126-134,177-189`). U: The complete Windows native gesture producer chain was not traced.
- D: Windows presentation border/underlay/preview bounds pass unchanged through the bridge (`crates/tiler-windows/src/tiling_sys/render_bridge.rs:151-192,216-250,271-295`); the overlay FFI uses outer x/y/w/h for destination and bitmap dimensions (`crates/tiler-windows/src/ffi/overlay.rs:273-338`). Pager content supplies source frames, not final carrier geometry (`crates/tiler-core/src/world_presentation.rs:520-555`).
- D: KDE reads output `geometry`, desktop `workspace.clientArea`, window `frameGeometry`, and `workspace.cursorPos`; each numeric rectangle component/cursor coordinate uses `Math.round`, then integer/bounds validation (`kwin/src/world-observe.ts:302-327,354-374,651-676,1477-1494`). Normalized hints use rounded KWin extents (`:483-514`); these paths do not multiply by output scale.
- D: Pinned KWin v6.7.4 exposes fractional frame/client rectangles and work areas (`src/window.h:128,143-158`; `src/scripting/workspace_wrapper.h:418`); output scale is separate from logical geometry (`src/core/output.cpp:466-480`, pin `8438567a741826da8b7536a8b10eb3af8fc8820d`, documented at `docs/research/kwin-rust-migration.md:5`). The adapter's explicit presentation contract calls the shared space KWin logical pixels (`kwin/native-effect/presentation_consumer.h:11-21`).
- D: KDE native gestures instead quantize `moveResizeGeometry` and `globalPointer` with `std::lround` (`kwin/native-effect/activewindowborder.cpp:178-222`); start/update/finish use these captures (`:997-1052`). Native gesture integers are validated and forwarded, not re-rounded by TypeScript (`kwin/src/world-gesture.ts:37-73,124-134,171-193`).
- D: KDE border inner/outer, underlay outer, and preview rectangles are consumed in logical scene units without a second DPI scale/outset; item anchoring only translates scene/local coordinates (`kwin/native-effect/presentation_consumer.h:11-21`; `kwin/native-effect/activewindowborder.cpp:591-601,1581-1597,1785-1822`). Pager ingress carries rounded live frame sources (`kwin/src/world-observe.ts:1643-1654`). U: A KDE native pager cell/layout consumer was not established.
- D: macOS topology documents global AppKit bottom-left points and CG/AX top-left points; conversion is `primary_height - (y + h)`, with x/w/h unchanged and no backing-scale multiplication (`crates/tiler-macos/src/topology.rs:13-22,146-192`). `NSScreen.frame`, `visibleFrame`, and `backingScaleFactor` are read separately (`crates/tiler-macos/src/ffi.rs:9520-9579`); full/work areas remain distinct in reports (`crates/tiler-macos/src/world_builder.rs:465-495`).
- D: macOS screen rectangles round each component to nearest whole point (`crates/tiler-macos/src/topology.rs:63-100`), but production AX window snapshots truncate each f64 component with `as i32` (`crates/tiler-macos/src/native_tile_backend.rs:538-552`). AX frame/minimum hints are in screen points; positive minimums ceil to whole units (`crates/tiler-macos/src/native_facts.rs:57-62,106-124`).
- D: macOS samples `CGEvent::location` (`crates/tiler-macos/src/synth.rs:458-479`); event-tap pointer coordinates round to integers (`crates/tiler-macos/src/ffi.rs:9167-9180`), and gesture ingress carries cursor/start facts (`crates/tiler-macos/src/drag_session.rs:739-763`). The report's ordinary cursor/cursor-hit fields are absent (`crates/tiler-macos/src/world_builder.rs:686-695`); the drag start frame truncates AX components (`crates/tiler-macos/src/drag_session.rs:188-199`).
- D: macOS border and drop preview flip published top-left core shapes back to AppKit bottom-left points, without recomputing an outset (`crates/tiler-macos/src/border_runtime.rs:622-659`; `crates/tiler-macos/src/drag_session.rs:786-814`). Pager miniatures aspect-fit source frames into a point-sized band (`crates/tiler-macos/src/pager.rs:119-165`). Production underlay is explicitly disabled pending a consuming loop (`crates/tiler-macos/src/active_border.rs:144-147`). U: Complete native pager backing conversion was not traced.

### H(2). What the contract declares

- D: Core `Rect` is four signed 32-bit integers, documented as an integer pixel rectangle with half-open extents (`crates/tiler-core/src/geometry.rs:18-26`); geometry projection calls gaps device units (`:7-11`). Neither declaration carries a unit tag or scale.
- D: `ScreenReport` declares coherent adapter space, separate full/work rectangles, and per-screen gap/outer-gap, but no unit enum, DPI, or scale (`crates/tiler-core/src/world_report.rs:664-681`). The capability block has workspace/birth booleans, not a coordinate/scale capability (`:1043-1056`); the settings catalog has plain numeric gap/border values (`crates/tiler-core/src/settings_catalog.rs:279-302`).
- D: Presentation ingress explicitly declares already host-scaled adapter units: physical pixels on Windows, KWin logical pixels on KDE (`crates/tiler-core/src/world_report.rs:612-617,633-651`). It does not name macOS points or carry per-output style/scale rows. Frame attempts separately carry optional before/after window DPI (`:455-465`), which is transition evidence, not output scale.
- I: The declared contract is adapter-space coherence, not a cross-platform physical-pixel or logical-pixel unit; the same untyped `i32` represents different platform units (`crates/tiler-core/src/world_report.rs:612-617,664-681`; `crates/tiler-macos/src/active_border.rs:116-130`).

### H(3). Pixel settings, constants, and scaling owners

- D: Border gap/width/extension are a single presentation tuple, not per-output fields (`crates/tiler-core/src/world_report.rs:633-651`). Windows scales configured logical values with rounded `logical * dpi / 96`, using foreground-window DPI (`crates/tiler-windows/src/active_border.rs:131-165`; `crates/tiler-windows/src/tiling_sys/world_capture.rs:545-558`). Core uses their literal summed outset (`crates/tiler-core/src/visual.rs:84-99`).
- D: KDE rounds/clamps configured style values to 0..1024 logical units and resolves extension before ingress (`kwin/src/world-observe.ts:537-550,1586-1603,1627-1629`); rendering does not DPI-scale them again (`kwin/native-effect/presentation_consumer.h:11-21`). macOS rounds configured width/gap to integer points and sets extension to rounded width (`crates/tiler-macos/src/active_border.rs:116-156`); separate backing-pixel helpers accept scale (`:396-443`), not core planning.
- D: Gaps are configurable per screen in the contract, but Windows repeats unchanged global `state.inner_gap/outer_gap` across physical-pixel outputs (`crates/tiler-windows/src/tiling_sys/world_capture.rs:1011-1033`). KDE re-reads global gap settings without an output argument (`kwin/src/world-settings-live.ts:284-310`; `kwin/src/world-observe.ts:1186-1214`). macOS repeats `input.gap/outer_gap` across screens (`crates/tiler-macos/src/world_builder.rs:485-515`), with native defaults 8/8 (`crates/tiler-macos/src/runtime_types.rs:44-46`; `crates/tiler-macos/src/world_runtime.rs:725-726`). I: These become physical units on Windows, logical units on KDE, points on macOS; core does not rescale them (`crates/tiler-core/src/geometry.rs:7-11`).
- D: Pager carrier/cell sizing is adapter-owned, not part of core pager content (`crates/tiler-core/src/world_presentation.rs:520-555`). Windows cells are 72 at 96 DPI, strip height 24..48, rounded at live taskbar DPI; tray gap 2 and borders 1 remain physical/unscaled (`crates/tiler-windows/src/pager.rs:66-104`). The 2026-10-16 16/8 logical-pixel decision concerns icon cap/minimum, not cells (`docs/decisions.md:1932-1938`; `crates/tiler-windows/src/pager_icons.rs:169-170`). macOS miniature layout uses points (`crates/tiler-macos/src/pager.rs:119-165`). U: KDE cell sizing was not established.
- D: Core group-drop edge depths are fixed 32/80, explicitly called physical pixels, and used directly against `Rect` coordinates; leaf drop zones use rounded thirds/normalized distances (`crates/tiler-core/src/cosmic_v1.rs:332-340,354-435,440-468`; `crates/tiler-core/src/session/ops/drag.rs:1125-1134`). There is no scale argument in this classification.
- D: Cosmic resize constants are pair minima 720x480, pointer-child minima 360x240, and keyboard steps 12,14,...,20, all documented physical pixels (`crates/tiler-core/src/cosmic_v1.rs:202-245`). Shared policy delegates directly (`crates/tiler-core/src/policy.rs:131-163`); resize compares these thresholds to actual geometry extents without DPI conversion (`crates/tiler-core/src/session/ops/resize.rs:1490-1515`). These are resize constraints, not a universal admission minimum for every tile.
- I: Fixed core drop/minimum/step constants operate in the current adapter basis despite their physical-pixel comments: physical on Windows, logical on KDE, points on macOS. No per-output scale exists at the cited call sites to preserve one physical or one logical interpretation across hosts (`crates/tiler-core/src/cosmic_v1.rs:384-400`; `crates/tiler-core/src/policy.rs:131-163`; `crates/tiler-core/src/world_report.rs:664-681`).
- D: Core gesture classification has no distance hysteresis: any integer position/size change is considered, with same-size movement classified as a move when cursor exists (`crates/tiler-core/src/gesture.rs:167-189`). U: Platform-native drag activation thresholds outside the inspected lifecycle paths were not established.

### H(4). Mixed-DPI spaces and visible sizing

- I: Windows uses one physical desktop-coordinate basis, but not a uniform logical-size metric across differently scaled outputs; KDE uses one logical scene basis; macOS uses one global point basis independent of Retina scale (`crates/tiler-windows/src/tiling_sys/monitors.rs:4-18,52-77`; `kwin/native-effect/presentation_consumer.h:11-21`; `crates/tiler-macos/src/topology.rs:13-22,167-192`). Uniform coordinate units do not imply equal real-world millimeters across different displays.
- D: The 2026-10-20 Windows decision preserves the intended physical allocation when DPI changes and permits one immediate rebase through fresh insets, retaining exact arrival/fences (`docs/decisions.md:1561-1571`). Implementation reads new frame/insets and reasserts the same visible target once (`crates/tiler-windows/src/tiling_sys/world_native_frame.rs:72-133`; `crates/tiler-windows/src/tiling/reconcile.rs:132-157`).
- D: The cross-DPI record distinguishes producer/handler evidence from physical evidence and leaves parts of the topology matrix open (`docs/changes/windows-dual-output-live.md:112-164,198-214,454-473`). Core separately treats changed known before/after DPI as invalidating placement evidence (`crates/tiler-core/src/world_frames.rs:189-195`).
- I: A fixed Windows tile gap stays the same physical pixel count and therefore occupies fewer logical units on a higher-scale output; border style instead follows foreground-window DPI. A configured tile gap and border gap need not share scaling semantics (`crates/tiler-windows/src/tiling_sys/world_capture.rs:545-558,1025-1033`). KDE logical gaps/borders and macOS point gaps/borders retain logical/point size as output scale changes, while backing-pixel coverage can differ (`kwin/native-effect/presentation_consumer.h:11-21`; `crates/tiler-macos/src/topology.rs:167-192`; `crates/tiler-macos/src/active_border.rs:116-156`).
- U: Cross-output visual equivalence, KWin backend device-pixel raster rounding, and production macOS border backing-scale selection were not independently verified. The cited paths establish adapter-coordinate semantics, not equal perceived/physical size.

### H(5). Rounding, exact arrival, and tolerance risks

- D: Core completion is exact desired/observed `Rect` equality; staged fact wins over immediate readback (`crates/tiler-core/src/world_frames.rs:3-8,127-139,170-181`). Windows native arrival helpers also require `live == target` (`crates/tiler-windows/src/tiling_sys/world_tick.rs:229-244`; `crates/tiler-windows/src/tiling_sys/world_native.rs:2545-2547`). No +/-1 arrival allowance appears in those paths.
- D: Core's separate size-hint clamp allowance is 2 device units per meaningful hinted axis, documented as slack for 1px rounding; it is not general frame-arrival equivalence (`crates/tiler-core/src/size_hints.rs:26-37,66-72,334-345`). macOS has a separate 1-point zoom/full-display heuristic, not frame-arrival slack (`crates/tiler-macos/src/border_runtime.rs:215-252`).
- I: KDE has inconsistent half-tie quantization between ordinary facts (`Math.round`) and native gesture facts (`std::lround`): -10.5 becomes -10 versus -11, respectively. Gesture integer validation preserves that discrepancy (`kwin/src/world-observe.ts:302-307`; `kwin/native-effect/activewindowborder.cpp:178-222`; `kwin/src/world-gesture.ts:37-73,124-134`). This can change an edge/start-pointer fact by one logical unit; no resulting live misclassification was reproduced.
- D: macOS topology rounds, while production AX snapshots/start frames truncate; partial AX completion readbacks also truncate (`crates/tiler-macos/src/topology.rs:80-100`; `crates/tiler-macos/src/native_tile_backend.rs:390-414,538-552`; `crates/tiler-macos/src/drag_session.rs:188-199`). Only partial completions supply immediate frame readback to core; successful setters still rely on staged facts (`crates/tiler-macos/src/world_runtime.rs:873-887`).
- D: macOS production logging calls readback "exact", but compares integer tuples after `as i32`, not original AX f64 values (`crates/tiler-macos/src/native_tile_backend.rs:1052-1078`). Exact f64 polling in `crates/tiler-macos/src/ax.rs:915-941` is explicitly fixture-only (`:945-948`), not evidence of production convergence semantics.
- I: Integer normalization can either hide a native fractional mismatch or map an intended integer allocation to a different reported integer, depending on sign/tie/native adjustment; exact core comparison cannot recover the lost fraction. Different rounding of origin/extent also does not prove globally seam-preserving topology. Sources establish risks, not a demonstrated never-converging frame (`kwin/src/world-observe.ts:302-327`; `crates/tiler-macos/src/topology.rs:80-100`; `crates/tiler-macos/src/native_tile_backend.rs:538-552`; `crates/tiler-core/src/world_frames.rs:170-181`).
- U: No reproducible fractional-geometry nonconvergence case, complete native setter quantization matrix, or universal one-pixel equivalence policy was established. Existing exact-arrival and hinted-clamp policies must not be conflated.

### H(6). Guide agreement, contradictions, and evidence limits

- D: The guide requires one coherent report space and adapter-owned report/actuation/render conversion (`docs/porting/world-contract-v3.md:558-564`); its ingress prose explicitly allows physical Windows versus logical KDE units (`:118-132`). The core's ingress comments repeat that platform divergence (`crates/tiler-core/src/world_report.rs:612-617`).
- I: Inspected screen/frame/presentation boundaries support that coherence at unit/origin level: Windows PMv2 physical coordinates plus visible/outer joins, KDE logical scene geometry, macOS point conversion plus AppKit output flips (H(1)). This is not exhaustive adapter compliance or native-runtime verification.
- I: Core constants labeled physical pixels do not match a platform-independent physical interpretation when shared geometry is KDE logical units/macOS points; their direct uses have no scale conversion (H(3)). The type/guide contract leaves this numeric-policy basis implicit rather than declaring a uniform unit.
- I: KDE's statement that capture uses TS half-ties toward positive infinity (`kwin/native-effect/presentation_consumer.h:11-16`) does not describe native gesture capture's `std::lround` path (`kwin/native-effect/activewindowborder.cpp:178-222`). macOS likewise has more than one fractional-to-integer rule (H(5)). These are source-level quantization divergences; no report was proven to mix physical and logical units.
- U: Complete native API unit guarantees, KDE pager layout, macOS pager backing conversion/production backing-scale selection, Windows full gesture capture, and live cross-scale convergence remain unverified. No size-hint client/frame correctness contradiction was established; the general guide wording alone does not prove one for macOS AX hints.

## Evidence: package I (identifier types)

- D = direct source evidence; I = inference from cited evidence; U = unverified. Static inspection only; risks below are not demonstrated native failures or settled product decisions.

### I(1). Workspace identity kinds, types, and crossings

- D: `OutputId`, `WorkspaceId`, `WindowId`, and `NodeId` are distinct public `String` tuple newtypes; leaves/groups share `NodeId` (`crates/tiler-core/src/directional.rs:76-91`). There is no logical/native workspace distinction in these wrappers: world workspace desire `id`, `native_binding`, view, and command targets use `String`/`Option<String>` (`crates/tiler-core/src/world_report.rs:324-362,857-881`).
- D: Logical window membership receipts are `WindowFact.observed_workspace: Option<String>`; native membership is separately named `native_workspace: Option<String>`. `ScreenView.view: String` is a logical receipt, empty meaning unknown (`crates/tiler-core/src/world_report.rs:683-732`). Native order/current/occupancy keys are `Vec<String>`, `Option<String>`, and `String`, scoped by `OutputId`; repeated native strings across outputs are permitted (`:734-779`).
- D: Membership attempts carry logical target and logical `(OutputId, String)` pre/post separately from native pre/post strings; view attempts likewise separate logical pre/post/baseline from native versions, all correlated by string operation ID (`crates/tiler-core/src/world_report.rs:468-554`). Native creation carries logical workspace/output/operation separately from native before/after order and optional created-native string (`:782-828`).
- D: Trailing/spare is an explicit `bool`, not an ID prefix or absence of binding; workspace `order: usize` is a zero-based ring position (`crates/tiler-core/src/world_report.rs:333-347`; `crates/tiler-core/src/world_publish.rs:1085-1115`). Numbered commands use `u8` positions, select `1..=9`, send `0..=9` with zero meaning trailing; positional removal plans use `Vec<usize>` (`crates/tiler-core/src/world_report.rs:877-918`; `crates/tiler-core/src/workspace.rs:7-32`).
- D: Workspace scope is another string: owning output, or literal `shared`; per-domain authority scope is formatted `output/workspace` (`crates/tiler-core/src/world_report.rs:107-114`; `crates/tiler-core/src/world_publish.rs:208-214`). Canonical JSON uses string `id/out/bind/scope/view/ws`, numeric `order`, and optional `trailing:true`; absent desire binding/view encode as empty strings (`crates/tiler-core/src/world_sizing.rs:1242-1296`). Native current/occupancy unknown instead use wire null (`crates/tiler-core/src/world_report.rs:734-740,760-778`).
- D: Logical IDs mint as `ws-<counter>`; new engine construction zeros the counter and empties maps, so this allocator is not restart-persistent (`crates/tiler-core/src/world_model.rs:818-846`; `crates/tiler-core/src/world_engine.rs:773-825`). Native-to-logical binding is explicitly `(output, native) -> logical`; host-owned receipts derive through it, not through adapter logical echoes (`crates/tiler-core/src/world_native.rs:59-76,85-128,146-168`).
- D: Windows uses captured logical view receipts with host-owned capability false and no native rows (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:348-376`). KDE observes `desktop.id` strings and, in host-owned mode, emits empty logical views plus native order/current (`kwin/src/world-observe.ts:734-748,1443-1474`). macOS emits native order/current from domain workspace keys, empty logical views, and host-owned true (`crates/tiler-macos/src/world_builder.rs:446-463,520-542,666-673`).
- D: macOS actually wraps native `space-<u64 SID>` in core `WorkspaceId` inside `NativeDomain`, then uses it in a `DomainKey` for native membership (`crates/tiler-macos/src/runtime_types.rs:313-319`; `crates/tiler-macos/src/topology.rs:504-508`; `crates/tiler-macos/src/world_builder.rs:548-560`). Current-scope filtering translates via `native_binding` to logical ID; its unmapped fallback puts the native text in the logical comparison slot, documented to match no rows (`crates/tiler-macos/src/world_dispatch.rs:1245-1269`).
- I: The last crossing relies on generated namespace disjointness, not a Rust type distinction. It is a concrete mixed-kind slot, not proof of incorrect routing. Inspected numbered paths use indices as positions, not stable workspace identities; macOS converts a bound SID to a fresh Desktop-N ordinal for native dispatch (`crates/tiler-macos/src/runtime_thread.rs:863-888`). U: No production index-as-ID misrouting was established.

### I(2). Window identity and reuse fences

- D: The world contract asks for a stable window lifetime but carries only `WindowId`, not PID/creation/window-generation facts (`crates/tiler-core/src/world_report.rs:683-720`). Core state is keyed by that ID; explicit closes remove membership, while incomplete inventories retain it (`crates/tiler-core/src/world_engine.rs:459-500,586-591`; `crates/tiler-core/src/world_members.rs:88-107`). I: Native lifetime discrimination is therefore an adapter obligation; equal strings alone do not let core discover a missed close/reuse.
- D: Windows mints per-run opaque `wN` tokens from `(HWND, process_creation)`, with explicit reissue for same-process reuse (`crates/tiler-windows/src/tiling/classification.rs:4-48`). Native `WindowKey` also has PID; HWND/PID/process-creation equality alone cannot detect same-process reuse, so a live lifetime tag must match (`crates/tiler-windows/src/native_guards.rs:17-68`). Admission drops stale state on tag mismatch and installs/checks a fresh tag before reissuing the token (`crates/tiler-windows/src/tiling_sys/assignments.rs:59-71,108-137`).
- D: KDE uses normalized live `internalId`, unwrapping only exact braced UUIDs; liveness compares normalized IDs, not unstable wrapper objects (`kwin/src/native-id.ts:1-7,42-86`; `kwin/src/world-native.ts:173-215`). Pinned KWin v6.8.1 mints `QUuid::createUuid()` in `Window` construction and exposes that UUID (`src/window.cpp:57-60`; `src/window.h:1116-1122`, pin `728a6561d13b82659696bcb0e109589c0a659397`). I: This is constructor-minted identity rather than raw XID/handle reuse; it is not proof of mathematical UUID noncollision.
- D: macOS uses `"<pid>_<wid>"`, parsed back into two `u32`s; `wid` is the CG window ID joined from AX via `_AXUIElementGetWindow` (`crates/tiler-macos/src/runtime_types.rs:926-939`; `crates/tiler-macos/src/ffi.rs:1834-1852`). App-generation/sequence gates reject stale observations and new-generation launch forgets old homing/correlation/pending closes (`crates/tiler-macos/src/runtime_thread.rs:1344-1373,1459-1538`).
- D: macOS native effects additionally revalidate process launch-date identity, CG ownership, membership, and topology, then freshly join AX by PID/WID (`crates/tiler-macos/src/native_tile_backend.rs:211-268,1537-1573,1636-1656`). I: These are process-lifetime and current-ownership fences, not a per-window generation in the core ID. A same-process recycled CGWindowID without an intervening observed close could alias the old string. U: Native CGWindowID reuse timing and an actual aliasing failure were not established.

### I(3). Output identity and retained ownership

- D: Core output identity is the `String` newtype above and string `out` on JSON (`crates/tiler-core/src/directional.rs:76-78`; `crates/tiler-core/src/world_sizing.rs:1271-1296`). Rings, views, native bindings, displaced origins, and disconnect-time facts retain output-string keys; homes carry output through `DomainKey` (`crates/tiler-core/src/world_engine.rs:532-576`; `crates/tiler-core/src/world_members.rs:20-35,45-65`). I: Return/retention continuity depends on the adapter reporting the same key, not on geometry or enumeration position.
- D: Windows reads `MONITORINFOEXW.szDevice` through `HMONITOR`; the handle itself is not the key, and an empty name falls back to `display-?` (`crates/tiler-windows/src/ffi/enumeration.rs:60-101`). Device strings map to retained `oN` wire IDs, never reordered/reused by topology sync (`crates/tiler-windows/src/tiling_sys/scope_hotplug.rs:13-39`). I: Stable device strings preserve identity despite reorder/geometry changes; fallback-name collisions or changed device naming are not repaired by a hardware UUID here.
- D: KDE uses the opaque `screen.name` directly as `out`, with geometry read separately (`kwin/src/world-observe.ts:749-769,1413-1429`). macOS uses `macos-display-<u32 CG display ID>`, documented session-stable, not display UUID (`crates/tiler-macos/src/topology.rs:498-502`). UUID is separately used for exact display-to-SLS homing; separate-Spaces mode does not use row positions (`:291-324`).
- U: Native output-name/device-name/CG display-ID continuity or reuse across unplug/replug, sleep/wake, resolution changes, reorder, and process/session restart was not independently established. Source mappings show conditional continuity, not a verified cross-event stability matrix or cross-restart output identity guarantee.

### I(4). Nodes, operations, and other exposed identifiers

- D: Leaf/group share string `NodeId`; leaf minting is `leaf-<window>` plus collision suffix, group minting `grp-<window>-r<base_revision>` plus suffix (`crates/tiler-core/src/directional.rs:84-87`; `crates/tiler-core/src/session/ops/lifecycle.rs:1240-1255`; `crates/tiler-core/src/session.rs:2932-2949`). Seed/capacity paths also generate `fit-gN`, `fit-lN`, and capacity group candidates (`crates/tiler-core/src/seed.rs:222-241,308-315`; `crates/tiler-core/src/session/capacity.rs:707`). I: These are tree-local derived identities, not a durable restart allocator; fresh engine construction supplies no restoration state (`crates/tiler-core/src/world_engine.rs:773-825`).
- D: Underlay exposes typed group/focused-leaf and member leaf IDs, serialized as JSON strings `group/leaf` alongside `baseRev` (`crates/tiler-core/src/world_presentation.rs:85-98`; `crates/tiler-core/src/world_sizing.rs:1530-1561`). KDE validates/stores these as opaque strings but strips them from the native presentation payload, retaining member window IDs (`kwin/src/world-report.ts:1128-1167`; `kwin/src/world-presentation.ts:171-179`). Windows includes group/leaf strings in underlay geometry diagnostics (`crates/tiler-windows/src/tiling_sys/group_underlay.rs:379-404`). U: No adapter parsing of node-ID contents or durable node-ID storage was established.
- D: Other minted wire correlation strings include command `op-<seq>`, commandless `op-born-<serial>`/`op-native-<serial>`, hotplug/view `op-hp-<revision>-<index>`/`op-dpv-<revision>-<index>`, gesture `gesture-<window>-<start_seq>`, and cursor-focus `ffc-<window>` (`crates/tiler-core/src/world_operations.rs:753-765`; `crates/tiler-core/src/world_born.rs:88-95`; `crates/tiler-core/src/world_native.rs:1188-1200`; `crates/tiler-core/src/world_model.rs:719-750`; `crates/tiler-core/src/world_gesture.rs:80-87`; `crates/tiler-core/src/world_cursor.rs:187-191`). The already-known explicit/automatic focus prefix finding is not re-established here.
- D: Born/native serials are generation-scoped and restart from zero in a new engine; adapter-incarnation rotation does not reset these serials (`crates/tiler-core/src/world_engine.rs:460-463,514-524,799-810`). Owner/generation/correlation have separate validated private-string types internally, but world owner/incarnation/generation/request fields are strings and canonical reply envelope fields remain JSON strings (`crates/tiler-core/src/ids.rs:1-16,52-59,80-87`; `crates/tiler-core/src/world_report.rs:1010-1014`; `crates/tiler-core/src/world_sizing.rs:1204-1217`). I: Repeated textual IDs across fresh generations are possible; envelope freshness is part of their interpretation, not encoded in every ID.

### I(5). ID text interpreted for meaning beyond focus prefixes

- D: Core clear continuation strips `op-`, parses the suffix as command sequence, and defaults to zero on failure before resuming the operation (`crates/tiler-core/src/world_clear.rs:547-551,579-596`). Thus operation identity text also carries recoverable control data, not only opaque correlation.
- D: Windows parses `oN` numeric suffixes to allocate the next output token (`crates/tiler-windows/src/tiling_sys/world_reads.rs:159-179`; `crates/tiler-windows/src/tiling_sys/scope_hotplug.rs:18-38`). KDE normalizes UUID shape/braces (`kwin/src/native-id.ts:42-86`) and extracts output from composite `output/workspace` scope text (`kwin/src/world-native.ts:555-584`).
- D: macOS splits window identity at `_`, filters retained state by `"<pid>_"` prefix, and strips/parses `space-` to nonzero `u64` native SID (`crates/tiler-macos/src/runtime_types.rs:926-939`; `crates/tiler-macos/src/world_runtime.rs:474-496`; `crates/tiler-macos/src/workspace_select.rs:677-685`; `crates/tiler-macos/src/runtime_thread.rs:863-874`). I: These adapters do not uniformly treat IDs as opaque; formats and the native/logical namespace distinction are operational dependencies.

### I(6). CLI/diagnostic ID spaces and evidence limits

- D: World direct workspace commands specify logical IDs, while numbered commands explicitly specify indices (`crates/tiler-core/src/world_report.rs:857-881,907-918`). Windows workspace CLI uses numeric `SendTarget::Numbered(request.index)` (`crates/tiler-windows/src/tiling_sys/cli_workspace.rs:547-552`); native capture instead accepts HWND and records HWND/PID/creation (`crates/tiler-windows/src/tiling_sys/commands.rs:705-710,720-723,746-756`). macOS diagnostic move/restore/switch commands accept raw `wid: u32`, `pid: u32`, `space: u64`, not core logical `ws-N` (`crates/tiler-macos/src/cli.rs:61-98`).
- D: Windows output log tokens fall back from wire ID to device string; workspace log tokens are logical IDs (`crates/tiler-windows/src/tiling_sys/world_reads.rs:148-156`). Admission trace deliberately joins `window: wN` with native `hwnd/pid/creation` (`crates/tiler-windows/src/tiling_sys/assignments.rs:138-151`). I: Diagnostic fields span distinct namespaces, and generic output-token labels can change namespace when unmapped; this is not evidence that core commands accept a raw native ID interchangeably.
- U: Exhaustive CLI/diagnostic acceptance, every ID-format consumer, native desktop/Space ID persistence/reuse, node-ID persistence in all external consumers, KWin support-floor identity guarantees, and live ID-reuse/hotplug/sleep scenarios remain unverified. No confirmed wrong-workspace actuation or stale-window actuation follows from this static package alone.
- D: Compatibility vocabulary also includes the fixed offline macOS output `macos-main` and native workspace `macos-space-1`, not production logical `ws-N` (`crates/tiler-macos/src/runtime_types.rs:23-35,126-138`). One positional identity substitute remains in topology homing: when separate-Spaces mode is unknown and exactly one display/one SLS row exist, it uses `rows[0]`; other ambiguous cases refuse (`crates/tiler-macos/src/topology.rs:345-378`). I: This is a guarded 1:1 fallback, not evidence of ordinal workspace IDs or wrong-display routing.
- D: Operation identities additionally suffix command sequence with `-swap-a`, `-swap-b`, and `-refill` for retained migration/view phases (`crates/tiler-core/src/world_commands.rs:483-512,1392`). The numeric adapter gesture session ID is distinct from the core-derived gesture operation string: `GestureIngress.id` and `GestureSettled.id` are `u64` (`crates/tiler-core/src/gesture.rs:110-134,143-146`; `crates/tiler-core/src/world_gesture.rs:80-87`).

## Evidence: package J (restart and crash recovery)

- D = direct source evidence; I = inference from cited evidence; U = unverified. Static inspection only; recorded decisions are distinguished from implementation, and candidate shared rules below are not settled judgements.

### J(1). Process topology and restart boundaries

- D: Windows constructs `WorldOwner` in the native tile loop; its sequencer owns `WorldEngine` in-process (`crates/tiler-windows/src/tiling_sys/run_loop.rs:38-44,157-159`; `crates/tiler-windows/src/tiling_sys/world_sequencer.rs:203-232`). A separate same-executable `watch-owner` process monitors the owner (`crates/tiler-windows/src/product_hide/sys/watcher.rs:8-46,178-205`). I: Ordinary tiler process death loses both core and adapter, not just one side.
- D: Explorer is not the core host: tray `TaskbarCreated` handling repairs the icon, and pager handling invalidates/re-resolves its taskbar scope (`crates/tiler-windows/src/ffi/tray.rs:541-543`; `crates/tiler-windows/src/ffi/pager.rs:1028-1033`; `crates/tiler-windows/src/tiling_sys/run_loop.rs:1159-1161,1251`). U: Actual Explorer-restart effects on all native state were not exercised.
- D: KDE separates the KWin script observer/actuator from the Rust D-Bus Planner/world broker and native KWin presentation effect (`kwin/src/entry.ts:5-15,27-32`; `crates/omnitiler/src/world_broker.rs:1-25`). Script evaluation remints its producer lifetime; discovery of a different producer constructs an entirely fresh broker/Engine even if the Rust process stayed alive (`crates/omnitiler/src/world_broker.rs:443-478,518-538`). I: Script reload, broker restart, KWin restart, and login restart are distinct events; script reload does not preserve the existing core session merely because the broker process survives.
- D: macOS engine thread owns both `WorldEngine` and adapter sequencer; native effects are supplied by runtime workers (`crates/tiler-macos/src/world_runtime.rs:77-85,125-158`; `crates/tiler-macos/src/runtime_backend.rs:270-349`). I: App relaunch replaces both sides; worker restart is not a separately restarted core. U: Host-session/native-Space restart continuity was not tested on any platform.

### J(2). Envelope identity, freshness, and change handling

- D: Shared core rejects a different generation before processing; same-incarnation lower request sequences reject, exact duplicate reports replay a cached reply, reused sequences with changed content reject, and older observations reject (`crates/tiler-core/src/world_engine.rs:1225-1233,1278-1325`). Owner is parsed separately; owner rebinding clears Engine sessions when the owner changes (`:1253-1261,1386-1395`).
- D: An accepted new adapter incarnation resets flights, ordering, desired caches, workspace mappings/history and presentation, while retaining underlying Engine sessions and lifetime birth/admission state. Reset is staged: rejection leaves the old incarnation intact (`crates/tiler-core/src/world_engine.rs:900-949,1271-1279,1383-1407`). I: This is not equivalent to constructing a new Engine or restoring an old layout.
- D: Windows supplies static `tiler-windows` as adapter identity and owner-process creation as core generation; sequencer bootstrap starts request/command counters at 1 (`crates/tiler-windows/src/tiling_sys/run_loop.rs:38-44,157-159`; `crates/tiler-windows/src/tiling_sys/world_sequencer.rs:209-232`). Explicit generation reset constructs a fresh Engine and clears ordering/flight state; delayed replies cannot perform that reset (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:41-66`).
- D: KDE's broker stamps core owner `omnitiler-broker`, its own minted incarnation/generation and monotonic sequences; core request identity is the producer's source correlation (`crates/omnitiler/src/world_broker.rs:197,377-389,443-460,891-919`). Generation mint is `w-<nanos>-<pid>-<counter>` (`crates/omnitiler/src/world_service.rs:86-100`). Same-producer rediscovery keeps state; different-producer discovery replaces Engine, queues/results, replay caches and presentation (`crates/omnitiler/src/world_broker.rs:443-478,528-538`).
- D: macOS uses fixed owner `macos-phase-1`, fixed generation `phase-1`, and process-ID incarnation `macos-<pid>`; sequencer mints `req-<sequence>` and rejects wrong generation/incarnation/request/observation/revision (`crates/tiler-macos/src/runtime_types.rs:32-35`; `crates/tiler-macos/src/world_runtime.rs:69-75,132-142`; `crates/tiler-macos/src/world_sequencer.rs:233-268,279-325`). Binding a generation clears counters, flight and accepted caches (`:353-367`). I: Identity minting is not uniform; macOS relies on its stated no-cross-process-report assumption rather than a fresh persisted generation.

### J(3). Hidden, restyled, and presentation state after failure

- D: Windows workspace suppression uses native hide, not an off-screen/minimize substitute in the inspected path. It commits the exact lifetime claim and original minimized/maximized show state before `SW_HIDE`/nonactivating hide (`crates/tiler-windows/src/product_hide/sys/mod.rs:1715-1726,1769-1793`). Reveal uses nonactivating show and preserves original minimization (`:1875-1903,1928-1960`).
- D: Product startup launches and verifies a ready crash watcher before the owner body can hide; normal teardown reveals claims and leaves the watcher alive if uncertainty remains (`crates/tiler-windows/src/lifecycle.rs:879-903,919-957`; `crates/tiler-windows/src/product_hide/sys/watcher.rs:24-95`). Watcher waits on the exact owner process, acquires the ledger lease, and invokes restoration; startup also attempts dead-residue reclamation (`crates/tiler-windows/src/product_hide/sys/watcher.rs:123-170,178-262`; `crates/tiler-windows/src/lifecycle.rs:418-440`).
- D: Windows recovery requires matching owner/caller identity and dead owner; uncertain window identity/readback retains residue and aborts cleanup, while successful recovery removes the ledger only after visibility and owned mouse-Snap restoration (`crates/tiler-windows/src/lifecycle.rs:1160-1190,1222-1252`). Existing ledger blocks a new guarded owner (`:808-811`). I: Hidden windows have a recovery mechanism, not unconditional crash safety: watcher failure, corrupt ledger, failed reveal or uncertainty can leave them hidden until recovery succeeds. U: Those failure combinations were not reproduced.
- D: Windows fullscreen separately stores style/maximize/frame preimages in window properties, then clears caption/thick-frame bits; explicit exit restores only the owned bits/frame, retaining metadata on failure with no automatic retry (`crates/tiler-windows/src/tiling_sys/fullscreen.rs:8-30,83-125,286-357`). The inspected crash restore handles ledger visibility/Snap, not fullscreen style restoration (`crates/tiler-windows/src/lifecycle.rs:1178-1252`). I: A crashed fullscreen toggle can leave chrome/frame altered, with a later explicit exit supported by surviving properties; classification markers alone grant no recovery authority (`docs/decisions.md:2852-2858`).
- D: KDE inactive domains come from native desktop membership; minimized/hidden are read separately and retained, not treated as closed (`kwin/src/world-observe.ts:885-904,1211-1258`; `kwin/src/world-close-tracker.ts:1-8,15-65`). I: This inspected workspace route does not need a Windows-like unhide ledger; it is not proof that every native visibility condition self-recovers.
- D: KDE automatic-float `keepAbove` ownership is an adapter-local set; explicit stop safely attempts release, but pre-existing above is left untracked (`kwin/src/world-native.ts:232-236,1095-1111,1149-1175,1307-1328`). The existing record reports pinned KWin unload deletes scripts without a JavaScript stop callback (`docs/changes/kde-automatic-float-stacking.md:61-67`). I: Surviving project-raised above becomes indistinguishable from pre-existing above after reload; this is a source-supported recovery gap, already parked rather than decided (`docs/decisions.md:2773-2779`).
- D: KDE sticky-on also stores pre-sticky floating/above/below only in local maps; sticky-off restores them only when those entries exist (`kwin/src/world-native.ts:227-230,765-840`). I: Native all-desktops/above may remain while pre-sticky origin/history is lost on script restart; membership persistence is not restoration of stacking history.
- D: Explicit KDE adapter stop clears presentation subscribers/waiters and callbacks; native `ClearPresentation` resets held paint (`kwin/src/world-entry.ts:4852-4929`; `kwin/native-effect/activewindowborder.cpp:79-83,1649-1656`). U: Abrupt script/broker death cleanup of the still-running KWin effect's held paint was not established; orderly stop is not crash evidence.
- D: Inspected macOS production frame backend writes AX geometry; minimized/HiddenApp facts are retained exclusions, not a product minimize-to-hide action (`crates/tiler-macos/src/runtime_backend.rs:270-349`; `crates/tiler-macos/src/runtime_core.rs:672-723`). Found `AXMinimized` mutation is explicitly owned-fixture control (`crates/tiler-macos/src/ax.rs:1140-1214`). U: No production crash reveal/frame-restore path was found in the traced paths; this does not establish crash-stranded hidden windows where product suppression was not implemented.
- D: macOS bounded watchdog calls `process::exit(1)` and explicitly leaves windows in place; it is not a restoring watcher (`crates/tiler-macos/src/tile.rs:2224-2259`; `crates/tiler-macos/src/watchdog.rs:42-69,84-89`). Border guard destroys its carrier on ordinary return; its comment relies on OS panel reaping for forced exit (`crates/tiler-macos/src/tile.rs:895-902,996-1001`). U: OS cleanup of all overlay carriers and native crash outcomes were not verified.

### J(4). Startup adoption and surviving storage

- D: Shared core bootstrap has no applicable desires until a full report; complete host-owned native lists take precedence over fallback singleton creation and build native-to-logical bindings in native order (`crates/tiler-core/src/world_engine.rs:864-873`; `crates/tiler-core/src/world_model.rs:382-479`). Clean recursive-fit adoption versus centre-split/minimum-infeasible fallback is shared Engine code, not three adapter algorithms (`crates/tiler-core/src/engine.rs:1676-1756`; `crates/tiler-core/src/seed.rs:91-111`).
- D: Windows report lane is tiler-owned (`host_owned_workspace_domains=false`, no native rows), whereas KDE/macOS publish native desktop/Space evidence (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:348-376`; `kwin/src/world-observe.ts:1443-1474`; `crates/tiler-macos/src/world_builder.rs:653-674`). I: Fresh Windows logical rings cannot recover their former partition merely by re-observing a host-owned workspace set; its hide/show ledger is not a saved logical layout.
- D: Windows admission hydrates intentional/sticky markers before tiling, gated by scope, PID and exact live lifetime; tile overrides hydrate classification without geometry/focus/topmost writes, including retained maximize/fullscreen handling (`crates/tiler-windows/src/tiling_sys/restart.rs:307-331,372-395,411-535`). Missing/corrupt markers supply no intent. These marker reads and retained-window admission are adapter logic, separate from shared fit/adoption.
- D: KDE startup reads float/tile membership into adapter-local sets; settled changes write complete membership snapshots (`kwin/src/world-entry.ts:1356-1424`). Rust runtime store namespaces by bus ID and KWin unique owner; missing reads empty, corrupt/unreadable/mismatched reads degrade empty; it stores no geometry/focus/stacking (`crates/omnitiler/src/float_intent_store.rs:4-34,222-268,384-410`). I: Broker/script restart within that namespace can recover settled membership, but KWin/new-login namespace replacement cannot adopt the old record.
- D: macOS reconstructs native order/current and window membership from supplied fresh native domains and process-local lifetime/minimized memory (`crates/tiler-macos/src/world_builder.rs:22-41,446-469,548-587,666-679`). U: No durable macOS intent-slot read was established; native observations alone cannot recover intentional floating that is not natively observable.

### J(5). Divergence relevant to a possible shared rule

- I: Shared core already supplies freshness gates and geometric/native adoption, but restart classification is reconstructed in Windows admission, KDE seed sets/store calls, and macOS process-local memory (J(2), J(4)). A core-owned opaque intent restore could unify classification/fallback policy without treating platform recovery claims as intent or moving native identity checks into core; this is an implementation candidate under the existing direction, not a new decision.
- I: Rebootstrap has materially different loss boundaries: direct accepted incarnation change retains Engine sessions, KDE producer replacement constructs a fresh Engine, and Windows/macOS process replacement destroys both core and adapter (J(1), J(2)). A contract-level restart matrix would need to distinguish these rather than promise that every adapter reconnect adopts retained sessions.
- I: Visibility recovery authority and intent persistence are different data. Windows has a write-ahead ledger plus independent watcher; KDE stacking preimages are local; macOS deadline exit deliberately leaves geometry. These are not interchangeable implementations of one proven crash-restore guarantee (J(3)); any shared rule still needs host-specific enforcement and evidence.

### J(6). Recorded targets, code gaps, and evidence limits

- D: D25 selects host-set retention, membership re-observation without moves, provable focus and layout re-adoption; D26 selects persisted settings/gaps plus fresh host set/membership/maximize observations, explicitly no saved-layout restore and no cross-login float identity. Both remain native-verification-only in the record (`docs/decisions.md:2797-2813`). I: Lost saved ratios/history are not by themselves a deviation from these targets; Windows tiler-owned partitions require separate evidence, not an assumed host-set guarantee (J(4)).
- D: Aligned startup adoption selects the recursive-cut/tolerance fit with sequential long-edge fallback and no topology search (`docs/decisions.md:3359-3364`); the shared code contains those fit/decline branches (J(4)). U: No cross-platform fresh-start/native restart acceptance was established by this inspection.
- D: Approved intent ownership makes the KDE runtime store a host slot for core's record, but current executable KDE hydration/persistence is still adapter-set membership; Windows hydration is also adapter admission (J(4); `docs/decisions.md:2824-2830`). I: Existing stores evidence a delivery gap to the newer core-owned-record direction, not completion of it; macOS durable-slot delivery remains unverified.
- D: Restart storage explicitly excludes pre-sticky stacking/history and hydration writes (`docs/decisions.md:2824-2841`); KDE stacking unload recovery is explicitly parked (`:2773-2779`). I: The lost ownership/preimage in J(3) is not repaired by preserving float membership; no settled resolution is asserted here.
- U: No live crash, forced-exit, Explorer/KWin restart, logout/login, watchdog-failure, store-corruption, or overlay-reaping journey was run. Host behavior after KWin crash/session reconstruction, exhaustive Windows topmost/overlay teardown, macOS durable intent recovery, and abrupt KDE paint cleanup remain unverified. No native crash-safety PASS follows from static source or earlier decision records.
- D: Envelope detail for J(2): Windows owner is also static `tiler-windows`; its sequencer constructs `req-<request_seq>` (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:91,355-364`). The reset method is implemented but marked dead-code/no-caller, not evidence of a separately restartable production core (`:620-653`).
- I: Shared core treats a different valid incarnation as a fresh namespace, not as a temporally newer authenticated lifetime; its changed-token test alone cannot distinguish a delayed old incarnation from a new one (`crates/tiler-core/src/world_engine.rs:1271-1290,1383-1395`). KDE's discovered-producer/generation binding supplies an additional boundary, whereas macOS explicitly assumes reports never cross processes (J(1), J(2)). U: No delayed-old-incarnation reset was demonstrated through a production transport.

## Evidence: package K (settings validation and rejection vocabulary)

- D = direct source evidence; I = inference from cited evidence; U = unverified. Static inspection only; the evidence below does not settle interface or policy judgements.

### K(1). Catalog coverage, validation, and platform differences

- D: The catalog has 23 fields; its validator rejects wrong types, non-finite/out-of-range numbers, invalid color grammar and unknown enum tokens rather than clamping (`crates/tiler-core/src/settings_catalog.rs:251-273,465-861,885-1009`). Windows catalog edits pass this core per-field validator, then independently validate a staged complete document; persistence uses the latter validator directly (`crates/tiler-windows/src/settings/windows_catalog.rs:454-484`; `crates/tiler-windows/src/settings/persistence.rs:12-16,71-80`). I: Catalog validation existing in core does not mean every adapter runtime calls it.
- D: `gaps.inner` and `gaps.outer`: catalog/Windows integer default 8, range 0..64; core also validates each report screen's gaps. KDE accepts integer numbers or trimmed integer strings, independently falling back to 8 for fractions/out-of-range/read failures; macOS uses fixed report defaults 8/8 (`crates/tiler-core/src/settings_catalog.rs:279-286,470-503`; `crates/tiler-core/src/world_report.rs:2105-2107`; `crates/tiler-windows/src/settings/validation.rs:23-28`; `kwin/src/domain-gap.ts:3-8,20-43,56-108`; `crates/tiler-macos/src/runtime_types.rs:43-46`; `crates/tiler-macos/src/world_builder.rs:412-419`).
- D: `workspace.default-tiled`: boolean true; `workspace.mode`: closed `per-output-local|global-unique|shared`, default per-output-local, startup-only. Windows strictly rejects unknown mode; KDE retains raw mode in its settings UI/startup drift state but observer substitutes per-output-local for an unknown token; KDE `defaultTiled` is schema-only and reads as `value !== false`. macOS hardcodes local mode/default-tiled true (`crates/tiler-core/src/settings_catalog.rs:505-537`; `crates/tiler-windows/src/settings/validation.rs:87-95`; `kwin/native-effect/kdesettings_adapter.cpp:85-94`; `kwin/src/world-observe.ts:688-698`; `kwin/src/world-settings-live.ts:385-402`; `crates/tiler-kde-settings-exporter/src/kde_adapter.rs:371-377`; `crates/tiler-macos/src/world_builder.rs:663,681-683`).
- D: `move.same-axis`: default `group-with-neighbor`, alternative `swap-with-neighbor`; Windows strictly validates, KDE defaults unknown/read failures, macOS operation mapping uses `SameAxisMove::default()` rather than a file setting (`crates/tiler-core/src/settings_catalog.rs:538-554`; `crates/tiler-windows/src/settings/validation.rs:29-33`; `kwin/src/world-settings-live.ts:26-38,312-321`; `crates/tiler-macos/src/world_command_map.rs:249-255`).
- D: `admission.born-maximized`, `admission.fixed-size`, `admission.anchor-fallback`, `admission.no-space`, `migration.refill`: closed enums, defaults respectively `tile-on-launch`, `both-axes-fixed`, `largest-splittable-window`, `float`, `last-remaining-workspace`. Windows rejects unknown tokens; KDE normalizes each field to its default; macOS report uses fixed/core defaults (`crates/tiler-core/src/settings_catalog.rs:555-639`; `crates/tiler-windows/src/settings/validation.rs:34-68`; `kwin/src/world-settings-live.ts:40-93,180-205,312-403`; `crates/tiler-macos/src/world_builder.rs:412-419,680-691`).
- D: `output.switch-target` default `cursor-output`; `output.fresh-placement` default `cursor-output-last-focused`; `output.focus-follows-cursor` and `output.cursor-warp` false. Windows accepts catalog values with strict token validation. KDE has no controls/keys and reports `focused-output`/`focused-output-last-focused`, false/false, unlike catalog defaults; macOS uses core targeting defaults and false/false (`crates/tiler-core/src/settings_catalog.rs:640-707`; `crates/tiler-windows/src/settings/validation.rs:69-86`; `crates/tiler-kde-settings-exporter/src/kde_adapter.rs:358-369`; `kwin/src/world-observe.ts:1669-1670`; `crates/tiler-macos/src/world_builder.rs:691-694`).
- D: `border.enabled`, `border.use-theme`: boolean true; `border.width`: finite 0..32/default 3; `border.gap`: 0..64/default 0; `border.radius`: 0..64/default 6; `border.color`: RGB/default `#2a82da`. Windows validates these bounds/color; KDE KConfig appearance defaults match and UI numeric bounds match, but there is no border-enabled key. macOS validates file/CLI width/gap/radius/color and shares defaults except host-derived radius: 10.5 older/unknown OS, 17.5 on 26+ at default width/gap (`crates/tiler-core/src/settings_catalog.rs:708-809`; `crates/tiler-windows/src/settings/validation.rs:103-117`; `kwin/native-effect/activeborderconfig.kcfg:8-27`; `kwin/native-effect/unifiedsettings.ui:228-284`; `crates/tiler-macos/src/dev_selection.rs:156-207,233-287`; `crates/tiler-macos/src/active_border.rs:22-82`).
- D: `underlay.enabled`: boolean true; `underlay.color`: ARGB/default `#662a82da`; `underlay.extension`: finite -1..32/default -1 (follow border). Windows validates; KDE stores color/extension with matching defaults/UI bounds but no enabled key; its observer resolves any negative extension to border width before conversion, and enables both overlays only when effect facts exist. macOS has no file fields for these catalog settings and reports underlay disabled (`crates/tiler-core/src/settings_catalog.rs:810-861`; `crates/tiler-windows/src/settings/validation.rs:128-137`; `kwin/native-effect/activeborderconfig.kcfg:33-40`; `kwin/native-effect/unifiedsettings.ui:338-354`; `kwin/src/world-observe.ts:1595-1598,1622-1629`; `crates/tiler-macos/src/dev_selection.rs:133-154`; `crates/tiler-macos/src/world_builder.rs:257-261`).
- D: Style conversion is not catalog validation: core accepts integer presentation width/gap/extension 0..1024, KDE rounds/clamps to that range, macOS rounds width/gap to integer points (pre-established). Radius/colors/theme remain native-renderer concerns rather than those three ingress numbers (`crates/tiler-core/src/world_report.rs:612-646,2511-2518`; `kwin/src/world-observe.ts:537-550,1586-1598`; `crates/tiler-macos/src/active_border.rs:70-79`). U: Raw invalid KDE KConfig number/color conversion is not fully established; XML defaults and UI bounds do not prove runtime rejection or repair (`kwin/native-effect/activewindowborder.cpp:232-245,484-505,1746-1752`).
- D: Adapter-only settings include KDE startup `shortcutProfile` (input profile) and `DragPreviewColor`; Windows takeover, Win+L opt-in, mouse-Snap prevention, pager and pager-highlight settings; macOS modifier/preset/excluded bundles, pager highlight, workspace indicator and drag preview. These control input/admission eligibility or native presentation outside the portable catalog (`crates/tiler-kde-settings-exporter/src/kde_adapter.rs:379-393`; `kwin/src/world-settings-live.ts:257-283,404-420`; `crates/tiler-windows/src/settings/windows_catalog.rs:79-194`; `crates/tiler-macos/src/dev_selection.rs:133-154,459-495`; `crates/tiler-macos/src/tile.rs:144-181`). U: Exhaustive inventory of all non-catalog CLI/integration options was not established.

### K(2). Storage, change delivery, and invalid settings

- D: Windows persists versioned/revisioned JSON in the per-user application settings directory, atomically replacing the file. Startup invalid document selects defaults/degraded status; owner polls metadata once per pump, validates changed documents, and retains last-good settings on invalid/deleted content without rewriting it. CLI overrides are reapplied; workspace mode stays restart-required (`crates/tiler-windows/src/native.rs:62-67`; `crates/tiler-windows/src/settings/persistence.rs:4-16,64-80,90-123,160-188`; `crates/tiler-windows/src/tiling_sys/commands.rs:96-114`; `crates/tiler-windows/src/tiling_sys/settings.rs:414-426,450-470,722-767`). Capture reads live settings and sequencer copies them into reports (`crates/tiler-windows/src/tiling_sys/world_capture.rs:1674-1729`; `crates/tiler-windows/src/tiling_sys/world_sequencer.rs:330-391`).
- D: KDE uses KConfig `kwinrc` groups `Script-omnitiler-kwin` and `Effect-omnitiler-active-border`. KCM saves owned fields and requests KWin reconfigure; `Options.configChanged` invalidates observation and checks startup-only drift; live script fields re-resolve per dispatch. Native effect reconfigure rereads style and republishes facts. Missing/invalid script values default per field, not as one document (`kwin/native-effect/unifiedsettings_module.cpp:1625-1650,1725-1777,1788-1820`; `kwin/src/world-entry.ts:4663-4680`; `kwin/src/world-settings-live.ts:95-104,254-403`; `kwin/native-effect/activewindowborder.cpp:484-505`). Report mode/targeting/settings and presentation are assembled by observer (`kwin/src/world-observe.ts:1586-1598,1657-1670`).
- D: macOS reads per-user Application Support JSON once at session startup, CLI-over-file-over-defaults; no plist setting delivery in this inspected path. Missing file defaults, but unreadable/UTF-8/JSON/validation errors refuse loading. Active-border config is startup-only; file has no portable gap/workspace/admission/output settings, so builder inserts defaults instead (`crates/tiler-macos/src/dev_selection.rs:126-154,233-239,396-449`; `crates/tiler-macos/src/main.rs:944-1008`; `crates/tiler-macos/src/world_builder.rs:412-419,653-705`).
- D: Settings participate in today's report digest (`crates/tiler-core/src/world_report.rs:1618,1936-1938`). Wire validation differs even within core-facing settings: predicate/refill/switch-target/fresh-placement and workspace mode reject unknown tokens, but missing/non-string/unknown anchor-fallback, no-space and born-maximized backfill defaults (`crates/tiler-protocol/src/world_protocol.rs:1752-1801`). I: A strict catalog editor and a tolerant wire decoder can accept different invalid-input journeys for the same setting.

### K(3). Rejection/refusal types and tokens

- D: `WorldOutcome::Rejected` and `CommandApplied::Rejected` carry `kind`/`message: &'static str`, not kind enums; rejected commands propagate into report-level rejection. `CommandResult` has DragPreview/Float/Migration/Directional/Maximize, no Rejected variant (`crates/tiler-core/src/world_engine.rs:357-370,1816-1817`; `crates/tiler-core/src/world_commands.rs:1445-1452`; `crates/tiler-core/src/world_report.rs:1367-1441`).
- D: Report gates use `generation-mismatch`, `oversized`, `unsupported-version`, `snapshot-invalid`, `owner-invalid`, `correlation-invalid`, `request-superseded`, `request-reused`, `observation-stale`, `reply-oversize`, `revision-exhausted` (`crates/tiler-core/src/world_engine.rs:61-85,1225-1360`). `WorldProtocolError` is also string-typed: `oversized`, `malformed`, `unknown-field`, `unknown-value`, `unsupported-version`, `snapshot-invalid` (`crates/tiler-protocol/src/world_protocol.rs:193-230`).
- D: World command kinds additionally include `direction-invalid`, `focus-unknown`, `migration-refused`, `selection-unknown`, `creation-exhausted`, `send-window-unknown`, `send-target-unknown`, `send-refused`, `scope-held`, `operation-refused`, `transfer-in-flight`, `view-output-unknown`, `view-target-unknown`, `tiling-target-unknown`; command paths can also emit `snapshot-invalid` (`crates/tiler-core/src/world_engine.rs:70-85`; `crates/tiler-core/src/world_commands.rs:568-753,867-1140`; `crates/tiler-core/src/world_operations.rs:165-299,1266-1278`; `crates/tiler-core/src/world_sends.rs:354,399,645`; `crates/tiler-core/src/world_tiling_views.rs:40-42,172-182`).
- D: Converters pass through `CoreReply::Rejected.kind`, `SnapshotInvalid.detail`, and typed `DivergenceKind.as_str()` without narrowing (`crates/tiler-core/src/boundary.rs:667-701`; `crates/tiler-core/src/world_commands.rs:1257-1269`; `crates/tiler-core/src/world_operations.rs:1266-1278,1714-1723`). Divergence tokens: `stale-revision`, `owner-mismatch`, `generation-mismatch`, `correlation-mismatch`, `capability-refused`, `partial-application`, `adapter-lost`, `postcondition-unverified`, `postcondition-mismatch`, `revision-exhausted` (`crates/tiler-core/src/contract.rs:246-277`).
- D: Inherited typed `RefusalKind` tokens: `duplicate-window`, `unknown-window`, `unknown-domain`, `focus-mismatch`, `not-tiled`, `planner-noop`, `planner-rejected`, `unsupported-capability`, `cross-domain-mismatch`, `malformed-input`, `malformed-topology`, `partial-observation`, `exception-behavior-unselected`, `unchanged`, `pair-below-minimum` (`crates/tiler-core/src/session.rs:190-230`).
- D: Additional Engine string literals/constants at converter inputs: `pending-exists`, `ambiguous-placement`, `direction-invalid`, `unknown-value`, `workspace-target-invalid`, `target-mismatch`, `absent-focus`, `overlay-present`, `domain-mismatch`; snapshot details `commit-rejected`, `seed-failed`, `move-op-invalid`, `missing-seed-order`, `toggle-orient-window-invalid`, `move-window-invalid`, `domain-invalid`, `focus-window-invalid`, `resize-window-invalid`, `pointer-resize-window-invalid`, `drag-drop-window-invalid`, `canonical-source-unavailable`, `canonical-target-unavailable`, `canonical-pair-identity-mismatch`, `canonical-pair-unusable`, `canonical-pair-domain-mismatch`, `canonical-pair-duplicate-state` (`crates/tiler-core/src/engine.rs:41-51,1098-1100,2084-2288,2938-3244,3270-3324,3714-3826,3990-4084,4315-4465,4520-5064,5298-5322`). U: Complete union through every `error.kind()` constructor and reachability of every inherited Engine token through current world commands are not established.
- D: `snapshot-invalid` fixed reason vocabulary, identity/action/gesture: `version-mismatch`, `identity-invalid`, `generation-invalid`, `section-over-cap`, `cursor-invalid`, `cursor-hit-without-cursor`, `cursor-hit-unknown`, `native-without-capability`, `action-output-invalid`, `action-outputs-invalid`, `duplicate-action-output`, `gesture-window-invalid`, `gesture-id-invalid`, `gesture-cursor-invalid`, `gesture-frame-invalid`, `command-pair-invalid`, `command-invalid` (`crates/tiler-core/src/world_report.rs:1985-2095`).
- D: Screen/window/view reasons: `screen-id-invalid`, `duplicate-screen`, `gap-invalid`, `screen-rect-invalid`, `screen-rect-partial`, `screen-complete-invalid`, `window-id-invalid`, `window-output-unknown`, `window-rect-invalid`, `duplicate-window`, `window-observed-invalid`, `window-native-invalid`, `closed-id-invalid`, `closed-overlaps-live`, `duplicate-closed`, `view-id-invalid`, `view-output-unknown`, `view-target-invalid`, `duplicate-view` (`crates/tiler-core/src/world_report.rs:2096-2202`).
- D: Native reasons: `native-invalid`, `native-output-unknown`, `duplicate-native`, `native-id-invalid`, `native-occupancy-incomplete`, `native-occupancy-invalid`, `duplicate-native-occupancy`, `native-over-cap` (`crates/tiler-core/src/world_report.rs:2204-2273`). Attempt reasons: `frame-window-invalid`, `frame-target-invalid`, `frame-readback-invalid`, `frame-prewrite-invalid`, `frame-operation-invalid`, `frame-pre-without-operation`, `duplicate-frame`, `member-window-invalid`, `member-target-invalid`, `member-homing-invalid`, `duplicate-member`, `view-operation-invalid`, `view-proof-invalid`, `duplicate-view-attempt`, `clear-window-invalid`, `duplicate-clear-attempt` (`crates/tiler-core/src/world_report.rs:2275-2410`).
- D: Creation reasons: `creation-target-invalid`, `creation-id-invalid`, `creation-order-over-cap`, `duplicate-creation-attempt`, `creation-triple-mismatch`, `creation-receipt-reused`, `creation-over-cap`; presentation reasons: `presentation-foreground-invalid`, `presentation-move-invalid`, `presentation-frame-invalid`, `presentation-style-invalid`, `presentation-order-over-cap`, `presentation-order-invalid`, `presentation-frames-over-cap` (`crates/tiler-core/src/world_report.rs:2413-2543`).
- D: Typed row/mechanical enums: `HoldReason` = `unreadable|incomplete-inventory|gesture-barrier|refused-target|ambiguous-creation|occupied-removal|constraint-clamp|minimized`, carried by `ScopeAuthority::Hold`; `AttemptOutcome` = `unattempted|possibly-executed|refused-proven|superseded-external|applied`; `FrameSetter` = `accepted|refused|threw` (`crates/tiler-core/src/world_report.rs:116-154,371-422`). These are not string-typed core reasons, although wire tokens are strings.
- D: `OperationOutcome` refusal variants are `ClearRefusedProven` (`refused-proven`), `ClearContinuationRefused` (`continuation-refused`), `FloatRefusedProven` (`float-refused-proven`); other lifecycle variants distinguish continuation, abandonment, supersession and proof. `OperationEvent.cause` is `Option<String>` (`crates/tiler-core/src/world_report.rs:1234-1284`).
- D: Typed `DirectionalOutcome` refusal tokens: `focus-refused-floating`, `move-refused-floating`, `focus-refused-workspace-floating`, `move-refused-workspace-floating`, `move-refused-fullscreen`, `focus-refused-maximize`, `focus-float-refused-stale`, `move-float-refused-stale`, `move-float-refused-invalid-geometry`, `move-float-refused-constraints`, `move-float-unknown-output`; retained/applied/need-clear outcomes are distinct. `DirectionalVerdict::Refused` and reply Directional results still carry optional String causes (`crates/tiler-core/src/world_directional.rs:22-89`; `crates/tiler-core/src/world_report.rs:1408-1430`).
- D: Constructed lifecycle/directional cause strings include `homing-loss`, `non-subject-focus`, `baseline-non-subject`, `refused-with-unmaximized`, `tiling-changed`, `unknown-output`, `invalid-geometry`, `constraints`, `invalid-direction`, `unknown-window`, `early-verdict`, `topology-unreadable`, `closed`, `external`, `no-target`, `focus-present`, `no-source`, `no-adjacent-output`; lifecycle also passes a rejected command kind through as cause (`crates/tiler-core/src/world_clear.rs:176-189,283-306,354-359,388-444,479-483,515-563,611-618`; `crates/tiler-core/src/world_settle.rs:1762-1885`; `crates/tiler-core/src/world_operations.rs:399-417,469-475,527-574,602-604,650-669,714-724`). I: These are not a closed cause vocabulary despite typed outer outcomes.
- D: Inspected core kinds above have no host name in their tokens; directional outcome docs explicitly describe Windows snap parity (`crates/tiler-core/src/world_directional.rs:1-12,22-24,42-89`). Windows additionally manufactures `WorldOutcome::Rejected { kind: "float-no-remap" }` after staging an otherwise accepted cross-output capacity-float result, preserving committed Engine state (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:868-889`). I: The apparent same rejection surface can therefore carry adapter policy, not only core validation.

### K(4). Adapter branching and consumption

- D: KDE semantically matches string kinds: take `generation-mismatch` drops dead-generation intents/rediscovers, `unknown-command` clears undecided correlation; capture `busy|unavailable` clears pending work and wakes follow-up, generation mismatch rediscovers; queue other rejection requeues the unfiled head for later events, not a timed retry loop; wait `unknown-adapter` holds for discovery/host deadline, other refusals use host deadline (`kwin/src/world-entry.ts:2849-2878,3866-3900,4082-4118,4386-4425`). These transport/broker kinds are not all core kinds in K(3).
- D: KDE rejection `kind/message`, row `hold/cause` parse as strings after shape checks, not semantic enums. Malformed reply handling is separate from kind-specific recovery (`kwin/src/world-report.ts:604-639,716-727,761-812,917-928`; `kwin/src/world-entry.ts:2833-2847,3902-3909`).
- D: Windows report rejection handling logs kind/message, drops the command, and immediately services observation-only catch-up without matching individual core kinds (`crates/tiler-windows/src/tiling_sys/world_tick.rs:2352-2386`). Directional handling branches on typed `DirectionalOutcome`, clears pending snap state/logs refusals; literal refusal strings emitted for missing target/rect are diagnostics, not reason-string comparisons (`crates/tiler-windows/src/tiling_sys/keyboard_directional.rs:326-447,470-579`).
- D: macOS core rejection clears flight and emits/counts `rejected-{kind}` generically. Its string equality `reason == "stale-observation"` is instead an adapter `ReplyVerdict::Refused` fence: retain an already-run select operation and attempt one permitted observation-only recovery, not replay the command (`crates/tiler-macos/src/world_dispatch.rs:487-498,616-658,742-783`). U: Exhaustive matching of all possible free-form causes throughout every adapter/UI path was not established.

### K(5). User-visible reporting

- D: Windows Settings Apply validation failure sets status, shows an error dialog and writes nothing; invalid loaded file disables Apply. Invalid live-file changes instead emit `settings-degraded` and preserve last-good settings (`crates/tiler-windows/src/settings_ui/draft.rs:511-523`; `crates/tiler-windows/src/settings_ui/mod.rs:697`; `crates/tiler-windows/src/settings_ui/tabs.rs:768`; `crates/tiler-windows/src/tiling_sys/settings.rs:748-764`). Refused-command path emits unconditional bounded JSON diagnostics, not a notification in that path (`crates/tiler-windows/src/tiling_sys/world_tick.rs:2359-2377`).
- D: KDE script invalid values normally default silently per field; KCM exposes saved/reconfigure-sent-unconfirmed/restart-required status, and script logs startup drift. Command refusals in inspected entry callbacks log kind/message and recovery action (`kwin/src/world-settings-live.ts:95-104,254-403`; `kwin/native-effect/unifiedsettings_module.cpp:1780-1830`; `kwin/src/world-entry.ts:2850,3895,4112,4417-4423,4669-4678`). U: Invalid raw effect-setting feedback beyond these paths is not established.
- D: macOS invalid settings write refused run outcome when requested and exit 2 with stderr; rejected SEND gets correlated stderr detail, while non-SEND refusals in the inspected dispatch path remain diagnostic-ring-only (`crates/tiler-macos/src/main.rs:23-27,954-1008`; `crates/tiler-macos/src/world_dispatch.rs:742-783`). I: Feedback diverges between a modal settings error, a status/default/log path, and startup refusal/diagnostic-only command handling. U: No exhaustive tray/notification absence claim or live user journey is established for any platform.

### K(6). Narrowest-scope evidence and limits

- D: Resilience says refuse/degrade the narrowest row/command/window/domain scope, never a whole report for one bad row (`docs/principles.md:165-180`). Core first-error validation rejects the report for one bad screen gap/rectangle, duplicate or invalid window, attempt, creation, or presentation row; `handle_report` returns `snapshot-invalid` before staging (`crates/tiler-core/src/world_report.rs:2096-2167,2275-2543`; `crates/tiler-core/src/world_engine.rs:1246-1251`). I: These are concrete broader-scope cases to assess against that rule, not a settled judgement about which envelope/identity failures legitimately require full refusal.
- D: One presentation style integer outside 0..1024 rejects the entire report; one unknown strict setting token rejects full wire decoding, while three tolerant settings default instead. KDE's pre-established rounding/clamping explicitly avoids style blocking every scope (`crates/tiler-core/src/world_report.rs:2511-2518`; `crates/tiler-protocol/src/world_protocol.rs:1752-1801`; `kwin/src/world-observe.ts:537-541`).
- D: One Windows invalid field rejects the whole settings document: startup uses defaults, live reload keeps the entire previous set; one macOS invalid present field/JSON error refuses startup settings loading. KDE script readers instead default individual fields (`crates/tiler-windows/src/settings/validation.rs:14-100`; `crates/tiler-windows/src/tiling_sys/commands.rs:96-114`; `crates/tiler-windows/src/tiling_sys/settings.rs:722-767`; `crates/tiler-macos/src/dev_selection.rs:396-449`; `kwin/src/world-settings-live.ts:254-403`). I: Whole-document handling and whole-world-report rejection are distinct blast radii requiring separate assessment.
- D: Logical command rejection returns a rejected report outcome, whereas typed scope holds/lifecycle refusals can coexist with an accepted reply; advisory bad size hints already sanitize to absent instead of rejecting the world (`crates/tiler-core/src/world_engine.rs:1816-1817`; `crates/tiler-core/src/world_report.rs:146-154,1274-1284,1312-1335,2144-2151`). I: The contract currently mixes narrow refusals with rollback of the accompanying observation; this is evidence relevant to the already selected command-result direction, not a new decision.
- U: No live invalid-setting, command-refusal or notification tests were run. Remaining gaps: exhaustive inherited Engine error-kind union/reachability, raw invalid KDE appearance conversion, every adapter-only CLI option, and every UI/cause-string consumer. Source evidence supports differences above, not cross-platform behavioral acceptance.

## Evidence: package L (window-state commands)

- D = direct source evidence; I = inference from cited evidence; U = unverified. Static inspection only; these observations do not settle interface or policy judgements.

### L(1). Commands and native bypasses

- D: The world has `ToggleMaximize { window }`; `Operation(CoreCommand)` has float/orientation toggles, but no fullscreen, maximize, minimize or sticky operation. There is no dedicated fullscreen/minimize/sticky world command (`crates/tiler-core/src/world_report.rs:867-1003`; `crates/tiler-core/src/boundary.rs:41-149`). Maximize returns an authorized native transition without topology mutation; fullscreen input means clear fullscreen and become maximized, otherwise flip observed maximize (`crates/tiler-core/src/world_commands.rs:853-920`; `crates/tiler-core/src/session.rs:997-1013`).
- D: Windows Win+M drains into adapter `dispatch_maximize_intent`, then `toggle_zoom_async`, not `ToggleMaximize`; native maximize uses `ShowWindowAsync`, restore uses window placement. Win+F11 and CLI fullscreen directly call owned enter/exit helpers (`crates/tiler-windows/src/tiling_sys/keyboard_tick.rs:89-93`; `crates/tiler-windows/src/tiling_sys/keyboard_overlays.rs:16-18,271-280,315-317,593-625`; `crates/tiler-windows/src/tiling_sys/probes.rs:165-227`; `crates/tiler-windows/src/tiling_sys/cli_workspace.rs:169-195,477-487`).
- D: Windows Win+Shift+G/CLI sticky selects adapter policy and product-marker writes, not a sticky world command. It is not wholly core-free: tiled/untiled conversion uses `Operation(ToggleFloat)` and cross-domain sticky-off can submit `Send` (`crates/tiler-windows/src/tiling_sys/dispatch.rs:788-817,848-852,909-1027,1041-1061,1159-1214`; `crates/tiler-windows/src/tiling_sys/float_toggle.rs:230-242,594-606`).
- D: KDE Meta+M/facade maximize submits `ToggleMaximize`; Meta+F11/facade fullscreen directly sets native fullscreen; Meta+Shift+G/facade sticky directly calls the native sticky setter. The latter two bypass core command authorization and report the resulting observation afterward (`kwin/src/world-shortcuts.ts:231-252`; `kwin/src/world-entry.ts:5034-5097`; `kwin/src/world-native.ts:792-800,888-925,950-1039`).
- D: macOS product tile hotkeys contain none of these four state actions; supported tile actions map through `Operation(CoreCommand)` (`crates/tiler-macos/src/native_input.rs:100-168`; `crates/tiler-macos/src/world_command_map.rs:229-294`; `crates/tiler-macos/src/runtime_thread.rs:1693-1709`). Direct AX minimize/fullscreen setters exist explicitly as owned fixture/helper controls, not shipped product state commands (`crates/tiler-macos/src/ax.rs:1211-1260,1411-1449`). I: No product minimize route was found on Windows/KDE either; Windows action/event enums omit it and KDE declares minimized read-only (`crates/tiler-windows/src/tiling/workspace.rs:78-99`; `crates/tiler-windows/src/snapkey/queue.rs:273-290`; `kwin/src/kwin-globals.d.ts:216-219`). U: This is not an exhaustive absence claim about every helper or external host action.

### L(2). Fact fields and provenance

- D: `WindowFact` has distinct `fullscreen`, `maximized`, `minimized`, `sticky` booleans; no provenance/unknown-state tag accompanies them. `rect=None` means unreadable, while minimized means retained homing with no managed geometry (`crates/tiler-core/src/world_report.rs:683-721`).
- D: Windows live minimize/maximize originate in `IsIconic`/`IsZoomed`; fullscreen is inferred from captionless style plus monitor-covering geometry, not a native fullscreen flag. Ordinary visible rows copy observed fullscreen/maximize and adapter sticky-table membership (`crates/tiler-windows/src/tiling_sys/observe.rs:1286-1321`; `crates/tiler-windows/src/tiling_sys/world_capture.rs:1159-1161,1261-1286`). Sticky is product property/table state, not native Windows all-workspaces truth (`crates/tiler-windows/src/product_hide/sys/mod.rs:414-461`).
- D: Windows minimized rows set fullscreen/maximized false and minimized true; unreadable rows also clear overlay flags. Retained/hidden rows can carry retained overlay flags or pre-hide maximize preimages, rather than fresh native truth (`crates/tiler-windows/src/tiling_sys/world_capture.rs:958-999,1223-1240,1367-1415,1481-1513`). I: The same boolean fields therefore mix observation, inference and retained evidence; they are not uniformly echoes of a command's requested target.
- D: KDE fullscreen/sticky directly project `fullScreen`/`onAllDesktops`; maximize is `maximizeMode != 0` (including partial maximize). Minimized is broadened to `minimized || !retained.eligible`, so hidden/ineligible retained members also report minimized; these are not merely the last setter request (`crates/omnitiler/src/kde_raw.rs:336-349,482-525`). Native safety reads also fold hidden into minimized and permit alias fallback (`kwin/src/world-native.ts:1074-1088,1431-1471`).
- D: macOS observes AX fullscreen/minimized, carries live fullscreen, and emits separate no-frame minimized rows; inspected live/minimized/unknown-home constructors never set maximized or sticky true (`crates/tiler-macos/src/native_facts.rs:442-475`; `crates/tiler-macos/src/runtime_core.rs:276-345`; `crates/tiler-macos/src/world_builder.rs:605-640`). Its separate presentation `foreground_maximized` is a work-area/full-display geometry inference with an engine-placement exemption, despite window facts remaining false (`crates/tiler-macos/src/world_builder.rs:212-251`).

### L(3). Geometry, membership and retained slots

- D: Core partitions explicit minimized facts away from ordinary Engine windows; Session removes a disappearing tiled leaf, collapses/reflows survivors, and saves its old tree/leaf as restore history. Ordinary restore uses the exact old tree if unchanged, else best-effort old-neighbor rejoin, else fresh admission; overlay/floating/sticky returns do not take that ordinary restore branch (`crates/tiler-core/src/world_observe.rs:344-369`; `crates/tiler-core/src/session/world.rs:1435-1465,1658-1741`). I: Minimize retains logical lifetime/history, not a live tile allocation; host UI owns the native minimize/restore action on the product routes found above.
- D: Existing tiled overlays can retain their reservation. Core publishes fullscreen allocation but no effective frame, with frame authority explicitly `Untouched`; its current tiled publisher does not apply the same fullscreen-only suppression to maximize (`crates/tiler-core/src/window_policy.rs:239-263`; `crates/tiler-core/src/world_publish.rs:623-648`). Windows fresh native geometry classification rejects fullscreen/maximize before writing (`crates/tiler-windows/src/tiling/classification.rs:188-200`; `crates/tiler-windows/src/tiling_sys/world_native_frame.rs:63-70`). I: A retained slot and an authorized native frame are separate, and current maximize frame fencing is not uniformly expressed by world publication.
- D: New fullscreen arrivals under fixed admission hold as slotless exceptions; first exit can reclassify/re-admit, unlike an existing retained tile (`crates/tiler-core/src/session/world.rs:1500-1551,1602-1625`). Birth maximize clearing is separately capability-gated, not implied by `ToggleMaximize` (`crates/tiler-core/src/world_report.rs:1051-1056`). U: Every born-maximized setting/platform combination and minimize-restore-into-fullscreen sequence was not traced end to end.
- D: Sticky is forced floating in KDE projection; observed tiled-to-floating conversion removes the old leaf rather than saving a minimized slot. Sticky homing is retained separately but excluded from occupancy reconciliation (`crates/omnitiler/src/kde_raw.rs:482-497`; `crates/tiler-core/src/session/world.rs:1472-1499`; `crates/tiler-core/src/world_members.rs:107-110`; `crates/tiler-core/src/world_model.rs:36-50`). KDE sticky-off restores prior floating intent; Windows sticky-off chooses prior-float preservation or unfloat/rehome (`kwin/src/world-native.ts:803-841`; `crates/tiler-windows/src/tiling_sys/dispatch.rs:1041-1119,1159-1214`). I: Sticky does not preserve a reserved tile; retiling is not minimized-neighborhood restoration. macOS has no reported sticky transition in these constructors.
- D: Windows fullscreen entry chooses a monitor FULL rect and owned exit restores saved frame or maximize state, then reconciliation can retile; KDE setters request host fullscreen/maximize without a tiler geometry calculation (`crates/tiler-windows/src/tiling_sys/fullscreen.rs:300-357,439-455`; `crates/tiler-windows/src/tiling_sys/keyboard_overlays.rs:615-624`; `kwin/src/world-native.ts:849-925,997-1038`). I: Fullscreen geometry is adapter-selected on Windows versus host-selected on KDE; after observed exit, core supplies ordinary tiled geometry where membership remains tiled. U: macOS zoom and separate-Space exit slot continuity were not established end to end.

### L(4). Fullscreen mechanisms and capability vocabulary

- D: Windows product fullscreen is tiler-owned borderless simulation using style/frame writes, and separately recognizes app-owned captionless fullscreen. KDE writes native KWin `fullScreen`; macOS reads native AX fullscreen, with owned helpers writing AX fullscreen and topology excluding fullscreen/system Spaces from ordinary domains (`crates/tiler-windows/src/tiling_sys/fullscreen.rs:300-357`; `crates/tiler-windows/src/tiling/float.rs:272-315`; `kwin/src/world-native.ts:901-924`; `crates/tiler-macos/src/ax.rs:1274-1408`; `crates/tiler-macos/src/world_builder.rs:446-449`). I: The inspected KDE/macOS paths use native fullscreen, not a tiler-simulated geometry mode; macOS topology accommodates a native fullscreen Space (`crates/tiler-core/src/world_report.rs:760-771`).
- D: The contract reports a fullscreen boolean and has host-workspace/equal-count/birth-clear opt-ins; it does not distinguish native fullscreen versus borderless simulation, ownership, separate-Space effects, or ability to exit (`crates/tiler-core/src/world_report.rs:705-721,1043-1059,1432-1441`). I: A portable exit cannot derive a safe mechanism solely from the existing fullscreen flag.

### L(5). Deferred neutral fullscreen-exit command and current exits

- D: The decision deferred a neutral fullscreen command for shortcuts such as Alt+Enter to this full interface review, rather than making presentation suspension supply command semantics. The backlog explicitly asks whether fullscreen/maximize/sticky toggles become neutral commands and says the ports do not pre-approve that change (`docs/decisions.md:195-207`; `docs/backlog.md:405-424`). I: The record gives a sequencing/interface-ownership deferral, not an accepted cross-platform exit design.
- D: KDE exits via the same direct F11/facade setter with `fullScreen=false`; additionally authorized `ToggleMaximize` clears fullscreen then requests maximize, fenced against source races (`kwin/src/world-shortcuts.ts:246-252`; `kwin/src/world-entry.ts:5077-5097`; `kwin/src/world-native.ts:979-1038`). Windows F11/CLI exits only project-owned fullscreen with valid restoration metadata; app-owned fullscreen refuses (`crates/tiler-windows/src/tiling/float.rs:272-315`; `crates/tiler-windows/src/tiling_sys/cli_workspace.rs:169-195`; `crates/tiler-windows/src/tiling_sys/keyboard_overlays.rs:593-625`).
- D: The separate Windows app-owned-fullscreen UX spike remains Future: keep refusal and investigate earlier style/frame capture to avoid guessing on exit (`docs/backlog.md:2016-2021`). macOS owned helpers accept false for AX fullscreen exit, but the inspected product action map exposes no fullscreen-exit command (`crates/tiler-macos/src/ax.rs:1411-1449`; `crates/tiler-macos/src/world_command_map.rs:229-294`). U: Application/host UI exit behavior and native success were not tested here.

### L(6). Candidate shared-rule evidence and limits

- I: Neutral-command review candidates are evidenced by Windows adapter maximize versus KDE core-authorized maximize, and both adapters' direct fullscreen/sticky writes, not a settled command design (L(1), L(5)). Fullscreen exit also needs the ownership/mechanism distinction absent from today's fact (L(4)); a boolean-only universal native setter would not reproduce Windows' current refusal contract.
- D: Shared sticky refusal vocabulary already prioritizes fullscreen then maximize, while KDE independently checks both before native sticky writes and Windows independently branches by overlay/workspace/float state (`crates/tiler-core/src/window_policy.rs:297-307`; `kwin/src/world-native.ts:750-763`; `crates/tiler-windows/src/tiling_sys/dispatch.rs:909-968`). I: Shared refusal/entry/exit policy is a concrete review candidate, distinct from host-specific mechanical setters and restoration evidence.
- I: Fact-vocabulary review candidates include KDE hidden-as-minimized, macOS always-false window maximize/sticky versus inferred presentation maximize, and Windows inferred/retained fullscreen/sticky evidence (L(2)). Existing tile retention, minimize restore history and sticky float conversion must not be conflated when comparing native state commands (L(3)).
- U: No live tests, state-command execution or behavioral acceptance were performed. Remaining gaps include exhaustive helper/action absence, all birth-setting combinations, macOS zoom/separate-Space slot continuity, minimize-restore-fullscreen combinations, and native refusal/partial-settlement journeys. These require evidence or discriminating reference rows before resolving behavior.

## Evidence: package M (diagnostics and logging)

- D = direct source evidence; I = inference from cited evidence; U = unverified. Static inspection only; these observations do not settle interface or policy judgements.

### M(1). Facilities, sinks and level controls

- D: Core declares no runtime dependencies; Windows, macOS and omnitiler manifests declare neither `log` nor `tracing`. Core exposes diagnostic outcomes as data rather than through those crates (`crates/tiler-core/Cargo.toml:1-11`; `crates/tiler-windows/Cargo.toml:7-15`; `crates/tiler-macos/Cargo.toml:21-42`; `crates/omnitiler/Cargo.toml:7-15`; `crates/tiler-core/src/world_engine.rs:357-369`). U: Exhaustive absence of every core print site or diagnostic recorder was not established; a core sink/default level is not evidenced.
- D: Windows tiling uses best-effort synchronous JSON-line file append with monotonic milliseconds, not a severity filter. Owner logs are named `run-{creation}.log`; `run --trace` defaults false. The helper itself has no size cap, throttle or severity argument (`crates/tiler-windows/src/tiling_sys/trace.rs:6-34`; `crates/tiler-windows/src/lifecycle.rs:18-24,47-62`). These source defaults also apply absent a release-specific override; compiled release behavior was not tested.
- D: macOS uses a custom structured async logger with a default stderr sink, plus direct stdout/stderr CLI output. `--log normal|verbose|trace` defaults `Normal`; verbose/trace add change-only heartbeats at about 10/2 seconds, not a standard info/debug filter (`crates/tiler-macos/src/runtime_log.rs:26-72`; `crates/tiler-macos/src/tile.rs:105-129,576-585,2631-2690`; `crates/tiler-macos/src/main.rs:23-27`).
- D: Rust KDE service summaries use unconditional `eprintln!`; the packaged planner unit routes stdout/stderr to the journal. Inspected summary call sites have no env/CLI level guard (`crates/omnitiler/src/planner_service.rs:467-470`; `packaging/systemd/omnitiler-planner.service:4-10`). U: No crate-wide runtime level control or external filtering guarantee was established.
- D: KWin script defaults to guarded `console.log`. Ordinary and installed builds define `__OMNITILER_TRACE__="0"`; `build:trace` defines `"1"`, enabling compile-gated probes rather than a runtime severity setting (`kwin/src/world-entry.ts:411-417,457-463`; `kwin/package.json:9-12`).
- D: Native effect/settings use Qt categories: active-border `qCInfo`, shortcut debug/info/warning/critical, and script-config/window-conflicts categories explicitly defaulting to `QtInfoMsg` (`kwin/native-effect/activewindowborder.cpp:43,1175-1184`; `kwin/native-effect/shortcutreconciler.cpp:3047,3086-3097`; `kwin/native-effect/unifiedsettings_module.cpp:30-31`). U: Effective Qt release thresholds, logging-rule environment configuration and host sink routing were not established; no separate native file or OS-log sink is evidenced here.

### M(2). User content and sanitization

- D: Principles prohibit secrets/user content and behavior-changing logging. Decisions require bounded correlated summaries, redact captions/content/secrets/raw environment/native IDs/native D-Bus payloads/unbounded pointer steps, and explicitly allow shortcut holder IDs/chords at info/warning while excluding friendly labels (`docs/principles.md:217-225`; `docs/decisions.md:4635-4648,4666-4685`). U: No additional general privacy rule was established in backlog or adapter-guide inspection.
- D: Windows `window-diag` source comments specify normal-level class/executable basename, no titles/full paths/SIDs/content, semantic dedupe and one global batch/second; these are documented properties, not verification of its serialized emitter (`crates/tiler-windows/src/tiling_sys/diag.rs:4-48`). U: The emitter, numeric batch cap, actual pseudonymization and class truncation were not located.
- D: Explicit Windows `inventory` outputs HWND/PID/executable basename/class/styles to stdout and never reads titles; explicit `children` additionally returns full executable path, SID and process-creation identity. These are inspection results, not evidence of ordinary background log content (`crates/tiler-windows/src/tiling_sys/commands.rs:779-821`; `crates/tiler-windows/src/tiling_sys/inspection.rs:72-77,94-117`).
- D: macOS normal event schema is sequence/time/PID/generation/reference/static kind/static outcome/count; arbitrary backend reasons map to fixed allowlisted or generic tokens. Input-refusal logs exclude keys/flags/bundles/content (`crates/tiler-macos/src/runtime_log.rs:26-39,74-148`; `crates/tiler-macos/src/native_input.rs:951-967`). Explicit `preflight` outputs frontmost PID/bundle; settings errors can print a full settings path (`crates/tiler-macos/src/main.rs:1100-1124,23-27`; `crates/tiler-macos/src/dev_selection.rs:425-447`).
- D: KWin operational `token()` allows lowercase letters/digits/hyphens up to 64 characters; Rust summary helpers bound correlation/counters and sanitize outcome/kind/message, not payload JSON. Script startup separately logs configured scope classes; compile-gated trace logs native window IDs and geometry (`kwin/src/world-entry.ts:968-974,872,503-516,603-634`; `crates/omnitiler/src/world_service.rs:570-596,679-758`).
- D: Native shortcut diagnostics accept component/action IDs and key images, truncate detail to 512 characters, and log conflict rows at info/warning in 400-character chunks; these are shortcut bindings, not a raw keystroke stream (`kwin/native-effect/shortcutreconciler.cpp:3066-3097,4354-4417`). U: No complete audit of window titles, app/process names, clipboard data or raw key data across all log/error paths was established; the cited helpers do not prove universal redaction.

### M(3). Buffers, inspection and crash records

- D: Windows pager trace retains up to 128 ordered native events between drains, counting overflow; fields are lifecycle tokens, fixed event/insert-after words, flags and geometry, not titles/handles/paths. `--trace`-gated pump drains write nonempty batches to the owner log; this is automatic, not a user-requested world-state dump (`crates/tiler-windows/src/ffi/pager_trace.rs:1-74,76-120`; `crates/tiler-windows/src/pager_sys.rs:412-447`).
- D: macOS retains 512 lifecycle entries and a default 256-entry async sink queue. Ring rotation and sink-copy drops are counted; entries contain metadata/static tokens, not full reports/replies/titles. Runtime callers can read `diagnostics()` and `log_snapshot()` in-process (`crates/tiler-macos/src/runtime_shared.rs:29-30,474-523`; `crates/tiler-macos/src/runtime_log.rs:159-188`; `crates/tiler-macos/src/runtime.rs:1646-1655`). U: A production CLI/IPC ring-export trigger was not established.
- D: KWin trace probe state caps pending windows, requested probes and source history at 16 each, with one-shot frame-change observation. It records window/source/time/requested-versus-observed geometry, not full contract history (`kwin/src/world-entry.ts:457-468,489-516,603-638`). Rust world request/reply transport caps are 4 MiB, not a retained log-ring capacity (`crates/omnitiler/src/world_service.rs:10-13`).
- D: Windows scoped panic hook appends sanitized source-basename/line/column/payload-type metadata and invokes the preserved hook; it does not dump world state or include panic-message text in its own record (`crates/tiler-windows/src/panic_diag.rs:2-11,49-68,90-96,125-139`). U: The preserved hook's output is not covered by that sanitization evidence; native/OS crash-dump behavior was not established.
- D: macOS optionally writes terminal `outcome.json` for a supplied run directory, including startup refusal reasons; watchdog dirty exit emits a fixed stderr line. Neither cited path exports the ring or a report/reply snapshot (`crates/tiler-macos/src/main.rs:954-965`; `crates/tiler-macos/src/tile.rs:632-674`; `crates/tiler-macos/src/watchdog.rs:84-89`). U: No general core/KDE state-dump command, crash recorder or dump-access boundary was established.

### M(4). Shared vocabulary and concrete comparisons

- D: Core has typed `WorldOutcome` and `HoldReason`; rejection kind/message are static strings, not a closed rejection enum. `validate_report_reason` returns fixed tokens such as `version-mismatch`/`section-over-cap` under `snapshot-invalid`; settings `Validity::Invalid` carries a static reason (`crates/tiler-core/src/world_engine.rs:357-369`; `crates/tiler-core/src/world_report.rs:116-143,1980-2009`; `crates/tiler-core/src/settings_catalog.rs:240-245,907-1008`). U: A common cross-platform log-event/severity schema implemented by every adapter was not established.
- D: Same event, core report rejected: Windows writes unconditional JSON `world-apply`, `delivery=rejected`, `reject_kind` and `reject_message`; macOS emits `core-reply` with kind only and counts `{op}-rejected-{kind}`; KDE logs `omnitiler:world:rejected kind=... msg=... recovery=on-next-event` after special transport/generation branches (`crates/tiler-windows/src/tiling_sys/world_tick.rs:2352-2377`; `crates/tiler-macos/src/world_dispatch.rs:742-779`; `kwin/src/world-entry.ts:3873-3899`). I: Shared core reason tokens survive, but event names, field sets and recovery narration differ.
- D: Same condition, invalid settings: Windows live reload keeps last-good values and logs changed `settings-degraded` status; macOS malformed/invalid startup settings return path/reason, exit 2 and optionally persist refusal; KDE readers catch/default individual values without a diagnostic in those functions (`crates/tiler-windows/src/tiling_sys/settings.rs:722-767`; `crates/tiler-macos/src/dev_selection.rs:425-447`; `crates/tiler-macos/src/main.rs:23-27,954-978`; `kwin/src/world-settings-live.ts:85-119`). I: These paths do not all render the shared `Validity` vocabulary verbatim.
- U: A complete adapter event-name inventory and same-event settle-timeout comparison were not established. Static evidence above establishes two comparisons, not equivalence of every refusal/recovery condition.

### M(5). Hot-path volume and formatting cost

- D: Rust `DescribeWorld` emits ingress and egress summaries on each successful evaluated call, with no local throttle; summary construction parses the request for each summary and parses the reply for egress, then formats bounded fields rather than whole JSON (`crates/omnitiler/src/planner_service.rs:434-470`; `crates/omnitiler/src/world_service.rs:603-604,679-758`). I: Bounded output does not mean constant parsing cost per capture.
- D: macOS production scoped observation emits `observation/applied` with member count before dispatch, independent of heartbeat verbosity. Every `emit` locks shared state, rotates/appends the ring and attempts nonblocking sink enqueue; default stderr can therefore receive per-accepted-observation traffic (`crates/tiler-macos/src/runtime_native.rs:463-499`; `crates/tiler-macos/src/runtime_shared.rs:474-523,544-561`; `crates/tiler-macos/src/runtime_log.rs:51-66`). Input refusals separately log first occurrence and every 50th/category (`crates/tiler-macos/src/native_input.rs:951-967`).
- D: Windows ordinary accepted native-apply diagnostic is trace-gated and emitted only when apply/hover/receipt evidence qualifies; rejection remains unconditional. Timing thresholds are 50 ms queue/owner work and 10 ms dispatch, with one-second summaries/repeat windows (`crates/tiler-windows/src/tiling_sys/world_tick.rs:1177-1247,2359-2377`; `crates/tiler-windows/src/input_health.rs:28-41`; `crates/tiler-windows/src/tiling_sys/owner_work.rs:39-67`). U: This does not prove every Windows normal log family is silent per event.
- D: KWin non-trace drop diagnostics stringify selected settlement/window/geometry/transfer fields, then at most two follow-up accepted captures; `drop-attempt` formats only the tracked window's nonempty attempt rows. These are structured report/reply subsets, not a whole report/reply dump (`kwin/src/world-entry.ts:2954-2967,3010-3018,3410-3420`). U: A numeric bound on every such serialized subset was not established.
- D: Native active-border visibility and presentation-transition info logs dedupe unchanged state, but `emitPresentationSetterDiag` logs every `SetPresentation` receipt with bounded scalar state (`kwin/native-effect/activewindowborder.cpp:1175-1184,1399-1467`). U: No blanket absence of per-frame info logging across the complete native effect, or whole-report `Debug`/JSON logging across all components, was established.

### M(6). User-visible surfaces and limits

- D: Windows Settings Apply validation writes status and shows an error dialog; live-file degradation instead follows the log path above. Run completion JSON includes status/log path; the inspected core refusal path only writes a diagnostic row (`crates/tiler-windows/src/settings_ui/draft.rs:511-523`; `crates/tiler-windows/src/tiling_sys/settings.rs:748-764`; `crates/tiler-windows/src/lifecycle.rs:958-978`; `crates/tiler-windows/src/tiling_sys/world_tick.rs:2359-2377`).
- D: KDE capture refusal path logs and schedules recovery; KCM separately exposes saved/reconfigure-sent-unconfirmed/restart-required text, not confirmation of application (`kwin/src/world-entry.ts:3895-3899`; `kwin/native-effect/unifiedsettings_module.cpp:1780-1837`). U: A core-refusal notification/tray surface was not established.
- D: macOS CLI returns stdout outcomes/stderr errors; rejected SEND additionally prints correlated detail. Despite the dispatch comment saying other refusals stay ring-only, `emit -> Shared::log -> LogSink` also forwards their generic `core-reply` event to the default stderr sink (`crates/tiler-macos/src/main.rs:23-27`; `crates/tiler-macos/src/world_dispatch.rs:742-779`; `crates/tiler-macos/src/runtime_shared.rs:474-523,552-561`; `crates/tiler-macos/src/runtime_log.rs:51-66`).
- U: No live tests or release execution were performed. Remaining gaps: exhaustive content/log-site audit, actual Windows `window-diag` emitter/bounds, full recorder/dump and trigger inventory, effective native Qt filtering/sinks, all tray/notification journeys, and performance measurements. No absence claim about unseen surfaces or OS-generated logs follows from this inspection.

## Evidence: package N (GNOME fit)

- D = direct source evidence; I = inference from cited evidence; U = unverified. Static inspection only; these observations do not settle interface or policy judgements.
- Clone HEADs inspected: `tilingshell` = `de30eb72d5436424929e78b0be612114ba073a68`; `PaperWM` = `8bf6dd264f60d6c0c402b63df7b424b888959a48`. Reference citations below name files at those revisions, read with `git show HEAD:<path>` or `git grep`.

### N(1). Lifecycle, lock and retained state

- D: Neither metadata file declares `session-modes`; Tiling Shell initializes settings/managers/signals on enable and destroys them on disable. PaperWM enables modules in order and disables them in reverse (`tilingshell:resources/metadata.json:1-25`; `tilingshell:src/extension.ts:115-181,765-806`; `PaperWM:metadata.json:1-9`; `PaperWM:extension.js:48-80`).
- D: PaperWM skips restoring overridden host settings when `Main.sessionMode.isLocked`; its backup of original boolean settings is written to GSettings and cleared after restoration (`PaperWM:patches.js:483-515,537-547`). U: This guard does not establish whether GNOME Shell invokes extension disable/enable on lock/unlock; the Shell extension manager was not inspected.
- D: Tiling Shell stores layout JSON and selected-layout arrays in Gio.Settings, rebuilding workspace/monitor selections from them; `originalSize` and `assignedTile` are fields attached to `Meta.Window` (`tilingshell:src/settings/settings.ts:81-83,116-117,541-582,694-712`; `tilingshell:src/utils/globalState.ts:200-239`; `tilingshell:src/components/tilingsystem/extendedWindow.ts:4-6`).
- D: PaperWM keeps a reusable in-process `SaveState` with spaces, connector-to-space mappings and viewport offsets across controlled disable/enable; workspace UUID/index/name settings separately use Gio.Settings (`PaperWM:tiling.js:98-105,207-238,3831-3903`; `PaperWM:workspace.js:9-78`).
- I: These are separate configuration and in-process retention mechanisms, not proof of full Shell-process restart recovery. Closest contract lifecycle fields are `owner`/`adapter_incarnation` and generation-reset rules (`docs/porting/world-contract-v3.md:353-369`). U: Full layout/window recovery after Shell restart, lock-time native visibility, and durable window-property persistence remain unverified.

### N(2). Native workspaces and per-monitor spaces

- D: Tiling Shell creates one tiling manager per monitor, with a layout map keyed by `Meta.Workspace`; selected layouts enumerate `get_n_workspaces()`/`get_workspace_by_index()` and follow workspace addition/removal/reorder (`tilingshell:src/extension.ts:193-200`; `tilingshell:src/components/tilingsystem/tilingManager.ts:112-134,237-290`; `tilingshell:src/utils/globalState.ts:115-195`).
- D: PaperWM's `Spaces` map is keyed by native workspace, enumerates existing workspaces and watches count/reorder changes. A Space index is its native workspace index; switching uses `workspace.activate()`/`activate_with_focus()`, while a separate monitor-to-Space map records visible assignments (`PaperWM:tiling.js:450-485,2170-2202,2424-2429`). I: The inspected per-monitor presentation is backed by native workspaces, not evidence of a second independently enumerated native workspace namespace.
- D: PaperWM forces `workspaces-only-on-primary=false`, backs up its original value, and refreshes monitor mappings when that setting changes (`PaperWM:patches.js:483-515,527-530`; `PaperWM:tiling.js:2225-2227`). U: The setting's host-side implementation and Tiling Shell's complete handling of it were not established.
- D: PaperWM's patched tracker adds workspaces until static counts cover monitors; dynamic mode reserves an extra workspace, preserves active/visible/minimum workspaces and removes other empty ones. Calls include `append_new_workspace()` and `remove_workspace()`; Tiling Shell also appends/reorders in an extension action (`PaperWM:patches.js:583-608,633-687`; `tilingshell:src/extension.ts:465-478`).
- I: Closest contract surfaces are `NativeObservation.native_order/current_native/occupancy`, per-output bindings, and correlated native creation desires/attempts, not the extensions' workspace indices themselves (`crates/tiler-core/src/world_report.rs:734-827`). U: Stable native workspace identifiers through reorder/recreation and exact per-output observations remain unverified.

### N(3). Window facts and coordinate evidence

- D: Tiling Shell indexes signal bookkeeping by `get_id()` and deletes it on `unmanaged`; PaperWM uses `get_stable_sequence()` for overview sorting. Both placement paths read `get_frame_rect()` (`tilingshell:src/components/raiseTogether/raiseTogetherManager.ts:54-63`; `PaperWM:overviewlayout.js:275`; `tilingshell:src/components/tilingsystem/tilingManager.ts:971-1002`; `PaperWM:tiling.js:593-623`). U: Identifier reuse/lifetime guarantees and restart identity were not established.
- D: Tiling Shell auto-tiling rejects non-normal, transient, attached-dialog, minimized and either-axis-maximized windows; another extension path checks fullscreen. PaperWM excludes transients/non-normal/all-workspace windows, observes fullscreen/minimize changes and distinguishes full maximize from horizontal flags (`tilingshell:src/components/tilingsystem/tilingManager.ts:1261-1274`; `tilingshell:src/extension.ts:507-523`; `PaperWM:tiling.js:3344-3374,3414-3429,3488-3527,3927-3942`).
- D: PaperWM's horizontal tiling maximize saves `unmaximizedRect` and changes frame geometry; Tiling Shell keeps its own `assignedTile` separately from native axis-maximize flags (`PaperWM:tiling.js:4794-4828`; `tilingshell:src/components/tilingsystem/extendedWindow.ts:4-6`; `tilingshell:src/components/tilingsystem/tilingManager.ts:1265-1276`). U: Native half-tile classification and exhaustive modal-state semantics are not established by those predicates.
- D: Frame coordinates feed move/resize directly. Tiling Shell separately obtains stage theme scale, falling back to monitor scale, and multiplies gap settings by scale; PaperWM applies theme scale to border geometry (`tilingshell:src/components/tilingsystem/tilingManager.ts:973-1002`; `tilingshell:src/utils/ui.ts:98-120`; `tilingshell:src/settings/settings.ts:509-538`; `PaperWM:tiling.js:2030-2033`). U: These excerpts do not establish logical versus physical frame units or fractional-scale rounding; Mutter coordinate internals were not inspected.
- I: Closest fact fields are `WindowFact.window/rect/fullscreen/maximized/minimized/sticky/floating` and size hints; screen/frame/style values share a coherent adapter coordinate space with conversion adapter-side (`crates/tiler-core/src/world_report.rs:610-617,664-720`). The citations identify observation inputs, not a validated GNOME projection.

### N(4). Native writes, hiding and asynchronous settlement

- D: Tiling Shell calls `move_to_monitor()`, `move_frame()` and `move_resize_frame()`; its animated path sets real-actor opacity to zero, animates a `Clutter.Clone`, restores opacity and destroys the clone (`tilingshell:src/components/tilingsystem/tilingManager.ts:993-1002`; `tilingshell:src/components/windowManager/tilingShellWindowManager.ts:112-174`).
- D: PaperWM's `animateWindow()` shows a clone and hides the real compositor actor; `showWindow()` reverses that. Scratch hiding instead calls `minimize()`, showing calls `unminimize()`/`make_above()`/`activate()`, and scratch entry calls `stick()` (`PaperWM:tiling.js:4765-4786`; `PaperWM:scratch.js:79-84,187-214`). Workspace switching separately calls `change_workspace()` (`PaperWM:tiling.js:2719-2729`). I: Actor visibility, native minimize and native membership changes are distinct mechanisms in these sources.
- D: Tiling Shell raises grouped tiled windows using `raise()` while blocking its own raised handler. PaperWM uses `Main.activateWindow()`, native activation and above-state writes; fullscreen entry saves/restores prior above state (`tilingshell:src/components/raiseTogether/raiseTogetherManager.ts:66-92`; `PaperWM:scratch.js:109-120,197-203`; `PaperWM:tiling.js:3488-3505`).
- D: PaperWM reads frames after resize, corrects position mismatches on `position-changed`, queues layout/size handling through `later_add(RESIZE)`, and has ten 100-ms target-height checks. Comments describe move-back/recursive-signal bugs, resize-from-size-signal trouble, and moving interrupting a Wayland resize; guards/deferred work accompany those comments (`PaperWM:tiling.js:601-623,816-825,923-968,1391-1408,3561-3578,3665-3674,3773-3774`).
- D: Tiling Shell waits for `first-frame` before newly created-window auto-tiling and rechecks minimized/maximize/transient/dialog state (`tilingshell:src/components/tilingsystem/tilingManager.ts:1280-1298`). I: Closest existing settlement surfaces are frame/member/view/clear attempt readbacks and fresh native visibility/frame/foreground proof, not callback completion alone (`docs/porting/world-contract-v3.md:438-482`). U: No native success, timing bound or equivalence between these callback patterns and contract settlement was tested.

### N(5). Input and actor-drawn decorations

- D: Tiling Shell uses `Main.wm.addKeybinding()` in `Shell.ActionMode.NORMAL` and removes bindings on teardown. PaperWM uses settings-backed bindings plus grabbed accelerators, action-id dispatch and explicit ungrab/removal (`tilingshell:src/keybindings.ts:100-118,364-385`; `PaperWM:keybindings.js:539-615`).
- D: Both connect grab begin/end signals; Tiling Shell can end/restart a grab with pointer/modifier data. It queries touch coordinates or `global.get_pointer()`; PaperWM queries pointer/monitor rectangles and implements pointer warping (`tilingshell:src/components/tilingsystem/resizeManager.ts:20-37`; `tilingshell:src/components/tilingsystem/tilingManager.ts:618-619,649-675`; `PaperWM:tiling.js:2218-2219`; `PaperWM:utils.js:219-288`).
- D: Tiling Shell's border extends `St.DrawingArea`. PaperWM builds Clutter clipping/space actors, St workspace indicators/labels, selection and border widgets, and minimap clones (`tilingshell:src/components/windowBorder/windowBorder.ts:22-34`; `PaperWM:tiling.js:277-340`; `PaperWM:minimap.js:37-56,153-154`). U: A native tabbed-window-group implementation was not established.
- I: Closest existing fields are normalized `WorldReport.gesture`, cursor/hit ingress, and `PresentationIngress` style/stack/frame data; the cited actor construction is rendering evidence only (`crates/tiler-core/src/world_report.rs:600-651,1163-1177`; `docs/porting/world-contract-v3.md:127-133`). U: Complete key conflicts, grab cancellation and pointer hit-test fidelity were not verified.

### N(6). Output identity and reconnect handling

- D: Tiling Shell builds managers from the monitor array and responds to `workareas-changed`: changed monitor count rebuilds managers/layout selections; unchanged count updates work areas by array index (`tilingshell:src/extension.ts:193-219`).
- D: PaperWM's DisplayConfig proxy requests monitor/logical-monitor data and declares `MonitorsChanged`; its upgrade consumes only the first monitor-spec string, `connector`, mapping through `get_monitor_for_connector()` to GNOME monitor indices (`PaperWM:utils.js:631-683`). U: Vendor/product/serial interpretation or consumption was not established; the four-string D-Bus signature alone does not label those fields.
- D: PaperWM listens to layout-manager `monitors-changed`, refreshes connectors, restores monitor-to-Space assignments by connector, and fills remaining outputs; `SaveState` records connector-to-space-index mappings (`PaperWM:tiling.js:2211-2213,2374-2418,3862-3869`). I: Closest contract field is stable `ScreenReport.output`, separate from FULL/work rectangles and completeness (`crates/tiler-core/src/world_report.rs:664-680`). U: Connector stability after renaming/replacement, duplicate-device identity and live reconnect outcomes remain unverified.

### N(7). Out-of-process routes and remaining limits

- D: Tiling Shell exports a session-bus `openLayoutEditor` method and unexports on disable; preferences call it with `Gio.DBus.session.call_sync()`. PaperWM uses a session-bus `Gio.DBusProxy` for Mutter DisplayConfig (`tilingshell:src/dbus.ts:1-29`; `tilingshell:src/prefs.ts:1239-1250`; `PaperWM:utils.js:631-683`).
- D: Tiling Shell uses `Gio.Subprocess` pipes for `dconf load/dump` and an asynchronous subprocess in its indicator menu. PaperWM has `GLib.spawn_async()` application/custom-command launch paths (`tilingshell:src/settings/settingsExport.ts:23-61`; `tilingshell:src/indicator/defaultMenu.ts:300-309`; `PaperWM:app.js:92-98,171-178`). I: These establish GJS-side process/bus usage, not an implemented OmniTiler Rust transport or its latency/reliability (`docs/porting/world-contract-v3.md:336-356`).
- U: Searches of both inspected revisions found no runtime socket or WebAssembly/Wasm implementation; this does not establish GJS capability or impossibility. No GNOME core adapter, Rust/GJS protocol prototype, lock/unlock execution, Shell restart, mixed-scale execution or live tests were evaluated. Mutter internals beyond the extension-visible APIs remain unverified.

## Evidence: package O (contract conformance testing)

- D = direct source evidence; I = inference from cited evidence; U = unverified. Static inspection only; these observations do not settle interface or policy judgements.

### O(1). Shared wire artifacts and their consumers

- D: `test-fixtures/world-wire/` contains 12 JSON files: one discovery, five reports and six replies. The regeneration example reads production-shaped requests and rewrites the six replies, including outer-gap (`crates/omnitiler/examples/world_wire_fixtures.rs:1-30,269-285`). These are producer-byte, parser/shape and behavior artifacts, not a standalone cross-adapter codec-roundtrip suite.

| Artifacts (all under `test-fixtures/world-wire/`) | Coverage and direct consumers |
|---|---|
| `discover.json`, `report-observe.json` | D: Production TS discovery/observation byte gates; observation includes native order, minimized lifetimes, raw hints, long IDs and extrema (`kwin/tests/world-bytes.test.ts:373-417`). Rust example reads observe to produce golden (`crates/omnitiler/examples/world_wire_fixtures.rs:227-228,257-267`). |
| `report-send.json` | D: Directional-send capture and separately queued direction-only command, checked byte-for-byte (`kwin/tests/world-bytes.test.ts:447-462`). |
| `report-kwin-size-hints.json`, `report-kwin-size-hints-select.json` | D: Qt zero/unbounded/partial-zero/fixed hint shapes and numbered-select follow-up bytes (`kwin/tests/world-bytes.test.ts:467-503`). |
| `report-outer-gap.json`, `reply-outer-gap.json` | D: Outer-gap report/reply regression reads both artifacts; example uses the report for broker reply generation (`kwin/tests/world-outer-gap-regression.test.ts:157-180`; `crates/omnitiler/examples/world_wire_fixtures.rs:269-285`). |
| `reply-golden.json` | D: Rust broker reply parsed/applied through production TS entry; mutations reject retired workspace field, extra top-level field and missing binding (`kwin/tests/world-bytes.test.ts:418-445`; `kwin/tests/world-wire.test.ts:624-646`). |
| `reply-group.json`, `reply-preview.json` | D: Group/drag-preview broker snapshots consumed by requester-result tests (`kwin/tests/world-command-result.test.ts:404,423`; generator `crates/omnitiler/examples/world_wire_fixtures.rs:287-345`). |
| `reply-float.json` | D: Retained float/frame-write-token scenario (`kwin/tests/world-apply-float.test.ts:19`; `kwin/tests/world-command-result.test.ts:586-604`; generator `crates/omnitiler/examples/world_wire_fixtures.rs:347-395`). |
| `reply-migrate.json` | D: Retained whole-workspace migration scenario (`kwin/tests/world-apply-migration.test.ts:22`; `kwin/tests/world-apply-aligned-migration.test.ts:20`; `kwin/tests/world-command-result.test.ts:517-536`; generator `crates/omnitiler/examples/world_wire_fixtures.rs:397-500`). |

- I: Direct readers/checkers are those seven KWin test files and the omnitiler regeneration example, not core/Windows/macOS tests: scoped `world-wire` searches in those three crates found no references. Other shared fixtures outside this directory are a different corpus. The example is a writer/shape gate, not a Rust test consuming all 12 artifacts (`crates/omnitiler/examples/world_wire_fixtures.rs:1-30,35-38,73-80`).
- D: Rejection coverage is generated by mutating valid replies, not separate rejection JSON files; queue/take also rejects an old-version reply (`kwin/tests/world-wire.test.ts:624-645,694-697`). Independent Rust codec tests compare protocol encoding with core canonical encoding (`crates/tiler-protocol/tests/world_codec.rs:1200-1217`).
- I: No common command/harness running this world-wire corpus against every adapter was found. No automated regeneration/freshness invocation outside the example was found in CI/scripts/recipes; its explicit command is documented in `crates/omnitiler/examples/world_wire_fixtures.rs:18-21`. This is an inventory finding, not a conformance judgement.

### O(2). Core boundary tests versus internal tests

- D: Core-only report/reply harnesses construct `WorldReport`, call `WorldEngine::handle_report`, and inspect `WorldReply`; they qualify as adapter-used engine-boundary tests without launching an adapter (`crates/tiler-core/tests/world_truthful_settlement.rs:10-13,52-128,138-150`).
- D: Static declaration counts are approximately 1,070 tests under core `src/` and 500 under `tests/`; 160 integration declarations are in 11 `world*.rs` files. Main report-driving files include `world_host_owned_stage1b.rs` (32), `world_native_retirement.rs` (29), `world_host_owned_gap_holds.rs` (23), `world_native_attempt_settlement.rs` (19), and `world_truthful_settlement.rs` (6). Counts are textual attributes, not executed cases; representative boundary and mixed-state assertions are at `crates/tiler-core/tests/world_truthful_settlement.rs:120-150` and `crates/tiler-core/tests/world_envelope_budget.rs:153-189`.
- D: The 160-test subset is not a verified pure-black-box count: envelope tests also inspect `state_revision()`/`retained_transfers()`. Internal report tests have full module access, and Session tests directly propose/commit focus rather than use the world boundary (`crates/tiler-core/tests/world_envelope_budget.rs:165-189`; `crates/tiler-core/src/world_report_tests.rs:1-7`; `crates/tiler-core/src/fullscreen_focus_cross_tests.rs:394-424`). U: An assertion-by-assertion black-box/internal split was not completed.

### O(3). Adapter projection, application and fake hosts

- D: Windows `world_diff_tests.rs` uses real owner tables, `capture_world`, `FreshHost`, and `FakeNative` (`crates/tiler-windows/src/tiling_sys/world_diff_tests.rs:1-9,134-192`). Capture assertions cover incomplete enumeration, recycled tokens, hidden/minimized/fullscreen facts (`:355-468`). `FakeNative` is in `world_diff.rs`, not a separate host module (`crates/tiler-windows/src/tiling_sys/world_diff.rs:598-705`); application harness calls `apply_world_reply` and returns raw member/view/frame attempts (`crates/tiler-windows/src/tiling_sys/world_diff_tests.rs:287-329`). Workspace creation/removal/binding writes are deliberately absent from this logical-workspace rig (`:7-9`).
- D: macOS inline builder tests assemble native contexts/observed windows and assert screen geometry, native order/current, completeness and no invented logical views (`crates/tiler-macos/src/world_builder.rs:871-903`); retained HiddenApp facts also pass real builder plus real engine (`:2009-2021,2081-2094`). Inline dispatch tests run real `consume_world_reply` and assert frame emission, held-reply no-write behavior and recovery (`crates/tiler-macos/src/world_dispatch.rs:2941-2947,3127-3203`).
- D: macOS has a scripted `FakeBackend: SelectionBackend` inside `runtime_thread.rs`, modeling native create/select/readback (`crates/tiler-macos/src/runtime_thread.rs:4641-4728`). The trailing-job test drives reply-authorized creation/select through that backend, stages native attempts through real jobs/report construction, then calls the engine for settlement (`:4771-4772,4794-4833,4839-4871,4882-4957`). Separate offline completion tests assert setter outcomes do not claim arrival (`crates/tiler-macos/src/world_runtime_tests.rs:1-4,389-408`). U: A universal macOS fake AX host covering every reply effect was not established.
- D: KDE's TS `makeHarness()` tests native-shaped screen/window/workspace capture, not preclassified `WindowFact` rows (`kwin/tests/world-entry.test.ts:370-395`). Rust raw-projection tests cover fixed classification, frame/client hint conversion and a separate `kde-raw-corpus.json` (`crates/omnitiler/src/kde_raw.rs:665-695,772-779`). Rust service tests construct capture envelopes and exercise service report/reply flow (`crates/omnitiler/tests/world_service_flow.rs:56-126,194-233`); native reply application is tested in the TS fixture consumers in O(1), not by Rust service tests writing KWin windows.
- D: Additional KWin scenarios use `world-wire-harness.ts`/`createMiniBroker` plus the Rust `raw_world_eval` bridge. This stamps transport while real Rust normalizes/executes reports; it explicitly excludes D-Bus/KWin and real broker auth/replay semantics (`kwin/tests/world-wire-harness.ts:1-15,54-57,97,125-130`; `kwin/tests/world-entry.test.ts:1619-1625,1650-1654`). I: This is dynamic TS-to-core scenario coverage, not a shared all-platform harness or proof of native compositor settlement.

### O(4). Cross-language drift protection

- D: TS contract unions/interfaces/parser and JSON encoder are hand-written and say they mirror Rust keys (`kwin/src/world-report.ts:1-15,24-55,604-648,1682-1684,1828-1853`). Build/test scripts typecheck/bundle TS rather than generate contract types (`kwin/package.json:7-12`).
- I: Searches of manifests/lockfile/scripts found no `ts-rs`, `schemars`, `typeshare`, `specta`, JSON-schema generation or Rust-to-TS codegen invocation. Observed protection is fixture bytes plus strict TS decode/shape rejection, Rust canonical-byte equality, and dynamic Rust bridge tests, not a generated common schema (`kwin/tests/world-bytes.test.ts:362-369,418-445`; `kwin/tests/world-wire.test.ts:624-646`; `crates/tiler-protocol/tests/world_codec.rs:1212-1217`; `kwin/tests/world-wire-harness.ts:54-57`). U: Exhaustive type/variant parity beyond exercised cases was not established.

### O(5). Reference-outcome executability

- I: No runner parsing `docs/spec/reference-outcomes.md`, complete row-ID/test index, or literal matrix-file reference in crates/KWin/scripts/recipes/CI was found. Some tests do cite individual rows: R-MAX-02 is an internal Session focus regression, not a markdown-driven scenario (`crates/tiler-core/src/fullscreen_focus_cross_tests.rs:394-424`). The matrix separately records offline core verification and exact native runtime TBD (`docs/spec/reference-outcomes.md:413-419`). U: Complete row coverage across core/offline adapters/live acceptance remains unverified; prose evidence is not executable acceptance.

### O(6). CI and local recipes

- D: Ubuntu CI runs KWin npm tests/typecheck (including compiled TS tests), Rust workspace tests/unsafe gate/fmt/Clippy/settings freshness, shell suites, and native effect/settings Nix checks (`.github/workflows/ci.yml:13-51,53-99`; `kwin/package.json:11`). License/source/bans and notices jobs are separate gates (`.github/workflows/ci.yml:101-131`).
- D: Windows CI runs fixture tests plus core/protocol/Windows/macOS crate tests and Clippy on `windows-latest`; macOS CI runs fixture plus core/protocol/macOS tests and Clippy on `macos-15` (`.github/workflows/ci.yml:133-174,176-210`). These adapters are not local-test-only. Building/testing the fixture crate is not invocation of a live desktop leg; neither job runs the live proof scripts in the inspected workflow.
- D: Root `justfile` dispatches OS imports (`justfile:15-22`). macOS `test` runs `cargo test --locked -p tiler-macos` via mise (`just/macos.just:51-52`); Windows has explicit offline check/mock versus live proof recipes (`just/windows.just:41-69,84-110`). Linux `check-portable` is a metadata/dependency/platform-leak check, not a suite recipe (`just/linux.just:1500-1510`). I: Whole-file recipe search found no generic Linux/Windows `test`; CI invokes the suites directly. Release workflow is source publication/OBS triggering, not another test pipeline (`.github/workflows/release.yml:1-18,56-68`).
- U: No suites, live tests or CI runs were executed for this package. Workflow declarations establish configured coverage, not current passing results; exact target-gated test availability, exhaustive fake-host effect coverage and a pure-black-box test count remain unverified.

## Evidence: package P (external control surfaces and IPC trust)

- D = direct source evidence; I = inference from cited evidence; U = unverified. Static inspection only; these observations do not settle interface or policy judgements.

### P(1). External entry-point inventory

- D: KDE Planner uses session D-Bus service `com.omnitiler.Planner`, object `/com/omnitiler/Planner`, interface `com.omnitiler.Planner1`; `WorldWake(gen, rev, kind)` is its outbound signal (`crates/omnitiler/src/planner_service.rs:3-9,48-57`). Below, UID means bus-derived caller UID equals service effective UID; KWin means the current `org.kde.KWin` unique owner (`crates/omnitiler/src/planner_service.rs:200-231,295-308`). Checks are application checks, not a claim about OS transport access.

| Platform / entry point | Capability and application check | Location |
|---|---|---|
| KDE Planner `DiscoverWorld` | D: Bootstrap/generation; UID + KWin. | `crates/omnitiler/src/planner_service.rs:313-359` |
| KDE Planner `DescribeWorld` | D: Capture/report into core, desired-state reply; UID + KWin. | `crates/omnitiler/src/planner_service.rs:374-432` |
| KDE Planner `QueueWorldCommand` | D: Queue a world command; UID, not exclusive KWin owner. | `crates/omnitiler/src/planner_service.rs:475-536` |
| KDE Planner `TakeCommandResult` | D: Read issuing command result; UID, sender-bound client namespace. | `crates/omnitiler/src/planner_service.rs:539-595` |
| KDE Planner `ReadWorldPresentation` | D: Read accepted snapshot/presentation; UID. | `crates/omnitiler/src/planner_service.rs:598-651` |
| KDE Planner `WaitWorldWake` | D: Park for work/lease; UID + KWin, generation/lifetime binding. | `crates/omnitiler/src/planner_service.rs:654-712` |
| KDE Planner `ReadFloatIntent` | D: Read persisted membership/tiling intent; UID + KWin. | `crates/omnitiler/src/planner_service.rs:715-813` |
| KDE Planner `WriteFloatIntent` | D: Persist intent; UID + KWin. | `crates/omnitiler/src/planner_service.rs:865-965` |
| KDE Tray `PublishSnapshot` | D: `com.omnitiler.Tray`, `/com/omnitiler/Tray`, `com.omnitiler.Tray1`; update tray status/scope, current KWin sender only. | `kwin/src/entry.ts:79-91`; `crates/omnitiler/src/tray_endpoint.rs:806-909` |
| KDE tray `org.kde.StatusNotifierItem` | D: Status/properties; activate/context/scroll handlers are no-ops, no caller gate in handlers. | `crates/omnitiler/src/tray.rs:917-1015` |
| KDE tray `com.canonical.dbusmenu` | D: `GetLayout`, `Event`, `AboutToShow`; Event launches settings, dispatches workspace toggle, or persists default; no caller gate in handlers. | `crates/omnitiler/src/tray.rs:1197-1255,550-620` |
| KDE effect `SetPresentation` | D: `com.omnitiler.ActiveBorder`, `/com/omnitiler/ActiveBorder`, `com.omnitiler.ActiveBorder1`; replace renderer paint, no sender check in handler. | `kwin/native-effect/activewindowborder.cpp:48-78,1358-1362` |
| KDE effect `ClearPresentation` | D: Clear renderer paint, no sender check in handler. | `kwin/native-effect/activewindowborder.cpp:79-84` |
| KDE effect `HidePresentation` | D: Hide renderer paint, no sender check in handler. | `kwin/native-effect/activewindowborder.cpp:85-90` |
| KDE effect `WaitPresentationFacts` | D: Read/hold style/chord/invalidation facts; KWin owner only. | `kwin/native-effect/activewindowborder.cpp:91-109`; `kwin/native-effect/presentationfacts.h:23-40` |
| KDE effect `WaitGesture` | D: `com.omnitiler.Gesture`, `/com/omnitiler/Gesture`, `com.omnitiler.Gesture1`; read/hold raw gesture, KWin owner only. | `kwin/native-effect/activewindowborder.cpp:1245`; `kwin/native-effect/gesturefacts.h:69-77`; `kwin/native-effect/gesturefacts.cpp:116-149` |
| KDE effect `LastGesture` | D: Immediate raw gesture snapshot, no owner gate in handler. | `kwin/native-effect/gesturefacts.cpp:189-194` |
| KDE effect `AckGesture` | D: Retire matching terminal gesture, return snapshot; KWin owner only. | `kwin/native-effect/gesturefacts.cpp:196-223` |
| Windows `workspace` / `workspace.request` | D: Select/send/stay/history/fullscreen/float/sticky; publisher checks medium caller + SID/session/exe + exact live owner; receiver compares owner fields in body. | `crates/tiler-windows/src/tiling_sys/commands.rs:118-229`; `crates/tiler-windows/src/tiling_sys/cli_workspace.rs:406-475` |
| Windows `resize` / `resize.request` | D: Live-subject keyboard resize; receiver checks owner creation/PID/exe/SID/session fields. | `crates/tiler-windows/src/tiling_sys/commands.rs:235-245`; `crates/tiler-windows/src/tiling_sys/cli_resize.rs:24-104` |
| Windows `directional` / `directional.request` | D: Live-subject move/focus; same receiver owner-field checks. | `crates/tiler-windows/src/tiling_sys/cli_directional.rs:26-106` |
| Windows `send-output` / `send-output.request` | D: Directional window send/follow or stay; same receiver owner-field checks. | `crates/tiler-windows/src/tiling_sys/cli_send_output.rs:27-107` |
| Windows `migrate-workspace` / `migrate-workspace.request` | D: Directional workspace migration; same receiver owner-field checks. | `crates/tiler-windows/src/tiling_sys/cli_migrate_workspace.rs:25-98` |
| Windows `stop` / `stop.request` | D: Graceful exit; publisher checks caller/owner identity and medium integrity; receiver matches trimmed owner-creation text, not writer credentials. | `crates/tiler-windows/src/lifecycle.rs:92-95,244-249,1275-1310` |
| Windows `emergency-stop` | D: Direct verified-owner termination, not file-message dispatch; same caller/owner checks. | `crates/tiler-windows/src/lifecycle.rs:1275-1291` |
| Windows `ready` | D: Ledger/liveness/readiness query; medium caller, SID/session/exe and exact live owner checks. | `crates/tiler-windows/src/lifecycle.rs:982-1025,1061-1081` |
| Windows `border-inspect` | D: Native border geometry/visibility query, medium caller + SID/session/exe + exact live owner checks. | `crates/tiler-windows/src/tiling_sys/inspection.rs:316-325,350-412` |
| Windows `underlay-inspect` | D: Native underlay geometry/visibility query, same carrier-inspection identity checks. | `crates/tiler-windows/src/tiling_sys/inspection.rs:327-337,350-412` |
| Windows `preview-inspect` | D: Native preview geometry/visibility query, same carrier-inspection identity checks. | `crates/tiler-windows/src/tiling_sys/inspection.rs:339-412` |
| Windows `watch-owner` | D: Bind PID/creation, publish readiness, await exit, restore exact owner residue; separate recovery path. | `crates/tiler-windows/src/product_hide/sys/watcher.rs:175-224` |
| Windows settings file / directory watch | D: Reload valid settings; no writer identity lookup in loader; invalid edits retain last-good values. | `crates/tiler-windows/src/settings/persistence.rs:31-62`; `crates/tiler-windows/src/tiling_sys/settings.rs:722-768`; `crates/tiler-windows/src/tiling_sys/run_loop.rs:697-744` |
| Windows tray `WM_APP + 100` | D: Icon-ID/mouse-word checks open menu, no sender check in handler; not a direct world-command message. | `crates/tiler-windows/src/tray.rs:53-54`; `crates/tiler-windows/src/ffi/tray.rs:575-584` |
| Windows registered `TaskbarCreated` | D: Tray/pager rediscovery and icon recovery; no sender check in handlers. | `crates/tiler-windows/src/ffi/tray.rs:541-553,638-641`; `crates/tiler-windows/src/ffi/pager.rs:802-805,1028-1037` |
| Windows pager `WM_LBUTTONDOWN` | D: Scalar coordinate hit-test, keep-latest workspace-selection sink; no sender check in handler. | `crates/tiler-windows/src/ffi/pager.rs:914-924`; `crates/tiler-windows/src/pager_sys.rs:489-499` |
| Windows settings `WM_COMMAND` | D: Scalar control ID/notification dispatch, including Apply/save; no sender check in handler; modal/suppression guards remain. | `crates/tiler-windows/src/ffi/settings_window.rs:158-161,198-230`; `crates/tiler-windows/src/settings_ui/mod.rs:355-357`; `crates/tiler-windows/src/settings_ui/draft.rs:556-579` |
| Windows first-run `WM_COMMAND` | D: Two accepted preset IDs choose outcome/destroy prompt; no sender check in handler. | `crates/tiler-windows/src/ffi/tray.rs:938-964` |
| KDE settings reconfiguration | D: Script `configChanged` reloads `readConfig`; native effect reconfigure rereads its config; these are host callbacks, not a script-exported command API. | `kwin/src/world-entry.ts:4666-4680`; `kwin/src/world-settings-live.ts:1-22`; `kwin/native-effect/activewindowborder.cpp:484-505` |
| macOS CLI probe/native actions | D: Per-invocation argv dispatch; owned AX actions require PID/nonce identity proof. No running-instance tiler transport is established by these paths. | `crates/tiler-macos/src/main.rs:13-29,114-148` |
| macOS settings file | D: Tile startup reads/validates settings using whole-file `fs::read`, without a writer check or explicit read cap in loader; live reload not established. | `crates/tiler-macos/src/main.rs:962-974`; `crates/tiler-macos/src/dev_selection.rs:425-448` |

### P(2). Authorization scope and fixed rejection

- D: Planner missing-sender/UID failures return fixed `error_body("unauthorized", "unauthorized")`, without parsing/echoing request correlation; intent admission returns `intent_rejection("-", "unauthorized")`. KWin-owner failures use separate fixed reasons (`crates/omnitiler/src/planner_service.rs:279-308,347-359,505-514,623-632`). This implements the recorded correlation omission, not a shared Windows/macOS transport response (`docs/decisions.md:4686-4689`).
- I: Windows CLI-side identity checks and receiver owner-binding checks are different evidence: request fields identify the intended owner, not the file writer. Direct file writes are not shown passing through `medium_caller`; no explicit filesystem ACL setup was established by scoped ACL/DACL API searches (`crates/tiler-windows/src/tiling_sys/commands.rs:133-181`; `crates/tiler-windows/src/tiling_sys/cli_workspace.rs:406-475`; `crates/tiler-windows/src/storage.rs:250-261`). U: Effective filesystem ACLs, bus policy, Windows message-delivery restrictions and OS credentials on uncited paths were not verified.

### P(3). Input, replay and admission bounds

- D: World request/reply caps are 4 MiB; protocol decode checks bytes before exact shape/types/vocabulary and core validation. Queue envelope keys are allowlisted and counters canonical; semantic commands decode into `WorldCommand` (`crates/tiler-core/src/world_report.rs:41-75`; `crates/tiler-protocol/src/world_protocol.rs:1392-1599`; `crates/omnitiler/src/world_broker.rs:1606-1661,1792-1810`).
- D: Queue generation must match; high-water/settlement keys bind authenticated sender + client + clientCmd, duplicates never enqueue. Bounds: 128 pending commands, 1,024 client namespaces, bounded 1,024-result retention; capture and intent use non-queuing single-flight admission, not a demonstrated time-based rate quota (`crates/omnitiler/src/world_broker.rs:200-206,1080-1135,1401-1452`; `crates/omnitiler/src/planner_service.rs:234-270,374-408`). Intent requests cap at 256 KiB (`crates/omnitiler/src/float_intent_store.rs:75`).
- D: Effect paint caps at 65,536 QString units, validates exact JSON keys/version and retains stale revision/coverage fences. Facts/gesture waits cap at 4,096 by QString size, tolerate malformed identity with a current snapshot, and supersede an existing waiter; gesture ack binds instance/serial (`kwin/native-effect/presentation_consumer.h:515-562`; `kwin/native-effect/presentationfacts.h:23-40`; `kwin/native-effect/gesturefacts.cpp:139-223`; `kwin/native-effect/gesturefacts.h:74-77`). The `MAX_REQUEST_BYTES` name does not itself make QString size a UTF-8 byte bound.
- D: Windows publishes one pending command file atomically without overwrite and consumes before dispatch; owner fields/correlation/version/action are checked, but receiver uses unbounded `read_to_string`. Workspace serde struct has no unknown-field rejection attribute. Settings instead enforce 65,536 bytes both before and during read (`crates/tiler-windows/src/tiling_sys/commands.rs:183-229`; `crates/tiler-windows/src/tiling_sys/cli_workspace.rs:406-475`; `crates/tiler-windows/src/tiling/workspace.rs:338-405`; `crates/tiler-windows/src/settings/schema.rs:19`; `crates/tiler-windows/src/settings/persistence.rs:31-62`). I: Consume-once is not a persistent client high-water fence against republishing a request.

### P(4). Read payloads and command capability

- D: `ReadWorldPresentation` carries cached snapshot/presentation/drop-preview geometry with window write tokens stripped, no effects/transfers/terminal results; presentation includes foreground/window IDs, border/underlay rectangles, workspace labels and pager window frames, not title/app-name fields in that encoder (`crates/omnitiler/src/world_broker.rs:1193-1220`; `crates/tiler-core/src/world_sizing.rs:1481-1638`). `LastGesture` exposes window, start rectangle/cursor, cursor, phase and instance/serial; facts expose chord/style/invalidation (`kwin/native-effect/gesturefacts.h:37-46`; `kwin/native-effect/activeborderlogic.h:44-79`). U: Exhaustive content audit of every result/snapshot subobject was not completed.
- D: Windows `ready` returns owner process identity and log path, not window titles; CLI also offers native inventory/overlay geometry inspections. macOS `cg-inspect` returns window IDs/PIDs/layer/onscreen/bounds, explicitly excluding titles, owner names and content; `owned-frame` returns owned AX/CG geometry/state (`crates/tiler-windows/src/lifecycle.rs:1061-1067`; `crates/tiler-windows/src/main.rs:34-36`; `crates/tiler-macos/src/main.rs:43-45,656-667`; `crates/tiler-macos/src/ffi.rs:9491-9517`). These native queries are not core world-state query transports.

### P(5). External vocabulary versus core variants

- D: Core has `FocusDirectional`, `MoveDirectional`, `SendDirectional`, numbered/relative selection/send, migration, maximize and `Operation(CoreCommand)`; float is not a top-level `WorldCommand` variant (`crates/tiler-core/src/world_report.rs:862-1003`). Windows help exposes adapter-specific verbs, while KDE JSON uses kebab-case `focus-directional`, `move-directional`, `send-directional`, `select-numbered`, etc.; float uses nested `operation` / `toggle-float` (`crates/tiler-windows/src/main.rs:29-33`; `crates/tiler-protocol/src/world_protocol.rs:1322-1327,1426-1586`).

| Action | Windows CLI | macOS CLI evidence | KDE `omnitiler` CLI / wire |
|---|---|---|---|
| Focus | `directional --focus --direction ...` | `owned-focus`, `ax-raise` (owned probes) | No action CLI; `focus-directional` JSON |
| Move | `directional --move --direction ...` | `ax-set-frame`, `spaces-move` (probe/native operations) | No action CLI; `move-directional` JSON |
| Send | `workspace --send/--stay`, `send-output --follow/--stay` | `spaces-move`, not an established running-tiler send CLI | No action CLI; `send-numbered` / `send-directional` JSON |
| Float toggle | `workspace --float` | No corresponding verb established | No action CLI; `operation` / `toggle-float`; script float action |
| Workspace select | `workspace --select/--relative/--previous` | `spaces-select`, `spaces-switch` (native selection probes) | No action CLI; `select-numbered` / relative/previous JSON |

- D: macOS parser supplies the cited probe/native verbs; KDE executable accepts only `planner-service` or `tray`, not those action verbs. KWin script submits focus/move/float and workspace actions to its world route (`crates/tiler-macos/src/cli.rs:1107-1219,1702-1705,2811-2845`; `crates/tiler-macos/src/main.rs:114-148`; `crates/omnitiler/src/main.rs:2-18`; `kwin/src/world-entry.ts:4975-5024,5102-5157`). I: Shared internal variants/wire spellings do not establish a shared cross-platform CLI vocabulary or equivalent probe semantics.

### P(6). Recorded intent and unverified coverage

- D: Recorded KDE intent trusts same-user processes, retains Planner's same-UID check and restricts tray snapshots to the current KWin owner; discovery/capture additionally bind KWin ownership (`docs/decisions.md:3931-3945`). Adapter guidance separates query/presentation/command authorization from exclusive report/actuator authority (`docs/porting/world-contract-v3.md:426-434`). Observability excludes user content/secrets and preserves fixed uncorrelated unauthorized replies (`docs/principles.md:217-224`; `docs/decisions.md:4637-4643,4686-4689`). U: A universal CLI vocabulary decision was not established in the inspected decisions/principles/guide.
- U: Scoped searches in Windows/macOS/protocol source did not establish production named-pipe, Unix/Mach socket, WM_COPYDATA, Apple-event or distributed-notification tiler-control endpoints; the macOS Unix-listener hit is a fixture client test (`crates/tiler-macos/src/fixture_post.rs:3832-3861`). This is not proof of absence. General time-based rate limits, exhaustive window-message reachability, macOS running-tiler control/settings reload, every native inspection CLI's data fields, and full transport-policy/ACL enforcement remain unverified. No live testing or binaries were used.

## Evidence: package Q (native workspace backing contract)

- D = direct source evidence; I = inference from cited evidence; U = unverified. Static inspection only; these observations do not settle interface or policy judgements.

### Q(1). Decision text and recorded reasons

| Item | Recorded status/date and decision | Recorded reason or race |
|---|---|---|
| Native-only receipts | D: "Tentative amendment, 2026-10-25": host-owned adapters supply native facts; core translates/binds and derives logical receipts; Windows keeps logical receipts (`docs/decisions.md:5820-5825`). | D: Bootstrap before logical ids exist without adapter-minted ids; unknown/conflicting evidence proves no arrival (`docs/porting/world-contract-v3.md:93-106`). |
| Hybrid creation | D: "Tentative hybrid creation correlation (option C), 2026-10-25; review after KDE and macOS changes land": causal created-id binding, otherwise sole-outstanding/singleton inference (`docs/decisions.md:5836-5848`). | D: Outside X can precede asynchronous requested T; uncorrelated X may bind and T later adopts independently. Several candidates/creations hold rather than blindly recreate; inference is not causal proof (`docs/decisions.md:5840-5848`). |
| Native preimages | D: "Tentative native attempt settlement (option 1), user-selected 2026-10-25; review after KDE and macOS land": capability-gated native before/after/baseline, translated only by core; Windows unchanged (`docs/decisions.md:5886-5896`). | D: Distinguish matching late tiler arrival from outside moves; user's switch back wins without reversing migration or reapplying source refill (`docs/decisions.md:5890-5896`). |
| Released backing | D: "Tentative released-source backing (option 1), user-selected 2026-10-25; comprehensive cross-platform review pending": release source binding, then fresh adoption, not dormant return binding or spare reuse (`docs/decisions.md:5897-5914`). | D: Positive arrival/close/newer off-output proof only; partial lists or pending creation defer cleanup beyond transfer retirement; refusal rollback keeps backing (`docs/decisions.md:5907-5914`). |
| Trailing spare/count | D: Adoption is "User-approved 2026-10-24; core implementation delivered, native acceptance TBD"; count clarification is "Tentative (later user review), 2026-10-24": four occupied native-bound rows plus ordinary trailing lifecycle, not four total (`docs/decisions.md:5796-5819`). | D: Preserve existing core behavior without a host-owned lifecycle exception; spare gains native backing only when used (`docs/decisions.md:5814-5819,280-284`). |
| Occupancy/retirement | D: "User 2026-10-29" selects managed KDE native retirement and neutral all-window occupancy; "User 2026-10-29, padding option 1" retains required bound padding; delivered offline, native/physical TBD (`docs/decisions.md:5862-5885`). | D: Protect unmanaged occupants, views/trailing/protected rows, readable coverage and minimum-two/parity; avoid deletion/replacement of old padding identity on staggered readback (`docs/decisions.md:5864-5885`). |

### Q(2). Core implementation and size

- D: Main modules (whole-file line counts, not feature-exclusive LOC): `world_native.rs` 1,823; `world_report.rs` 2,722; `world_settle.rs` 1,893; `world_model.rs` 1,479; `world_publish.rs` 1,271; `world_engine.rs` 1,929; pure lifecycle `workspace.rs` 280 (`crates/tiler-core/src/world_native.rs:1-1823`; `crates/tiler-core/src/world_report.rs:1-2722`; `crates/tiler-core/src/world_settle.rs:1-1893`; `crates/tiler-core/src/world_model.rs:1-1479`; `crates/tiler-core/src/world_publish.rs:1-1271`; `crates/tiler-core/src/world_engine.rs:1-1929`; `crates/tiler-core/src/workspace.rs:1-280`).
- D: Native-only path: `native_authoritative`, `effective_logical_receipt`, `native_echo_for` and `converge_native_pre`; unknown/unmapped/partial evidence retains home/view, never falls back to conflicting logical receipts. `WindowFact.native_workspace`, `NativeObservation.{output,native_order,current_native,occupancy}` and published `WorkspaceDesire.native_binding` carry the native/logical boundary (`crates/tiler-core/src/world_native.rs:85-169,212-278`; `crates/tiler-core/src/world_report.rs:695-779`; `crates/tiler-core/src/world_publish.rs:1085-1114`).
- D: Creation path: `logical_needs_backing`, `settle_native_creations`, `ensure_native_creations`; native order remains authoritative and outside additions adopt only after unresolved creation correlation permits it (`crates/tiler-core/src/world_native.rs:714-794,1163-1205,1247-1265`).
- D: Preimage path: `effective_member_post_on`, `effective_view_pre/post/baseline`, and `settle_rest` translate native attempts and retain held flights; non-null native attempt fields without capability refuse (`crates/tiler-core/src/world_settle.rs:75-156,632-820`; `crates/tiler-core/src/world_report.rs:2330-2338,2370-2387`).
- D: Release path: `release_migrated_source_backing` releases exact source pairs and calls `adopt_single_native`; `retry_pending_source_releases` survives parent settlement. Retirement path: `retirement_deletion_set`, `plan_workspaces`, `prune_vanished_output`, then ordinary trailing convergence (`crates/tiler-core/src/world_settle.rs:245-359`; `crates/tiler-core/src/world_native.rs:297-350,1209-1221`; `crates/tiler-core/src/world_publish.rs:1059-1115`; `crates/tiler-core/src/world_model.rs:22-30`).

### Q(3). States, holds and cross-report correlation

- D: Native membership/view evidence is known-mapped, unknown/unmapped, incomplete, or conflicting-order; `current_native: None` retains the logical view and never creates a workspace. Published holds include `IncompleteInventory` and `Unreadable`; complete disappearance removes a binding, never renames it (`crates/tiler-core/src/world_native.rs:98-169,297-317`; `crates/tiler-core/src/world_report.rs:743-779`; `crates/tiler-core/src/world_publish.rs:1091-1103`).
- D: `RetainedNativeCreation` stores `operation_id`, logical workspace, output, mint-time `order_before`, `minted_ms`, `pending` attempt and `ambiguous` latch. Desire/report fields are `WorldReply.native_creations` / `WorldReport.creation_attempts`; each attempt carries exact op/logical/output, setter, before/after orders, optional causal `created_native`, and `attempt_ms`. There is no separate attempt-id field here: op identity plus evidence/stamp supports replay (`crates/tiler-core/src/world_engine.rs:324-355`; `crates/tiler-core/src/world_report.rs:782-829`; `docs/porting/world-contract-v3.md:226-252`).
- D: Creation transitions include unbound-needed -> authorized -> pending readback -> bound; unchanged accepted/no-delta or absent correlated id stays pending. Changed replay, stale/future stamp, refusal/throw or collisions latch `AmbiguousCreation`; several candidates never authorize blind replacement. Correlated pending cannot fall back to an unrelated singleton (`crates/tiler-core/src/world_native.rs:757-794`; `crates/tiler-core/src/world_engine.rs:337-354`).
- D: `MemberAttempt` carries owning `operation_id`, window, target output/workspace, setter, logical pre/post, native pre/post, `attempt_ms`; `ViewAttempt` carries owning phase id, target, setter, logical pre/post/baseline and native pre/post/baseline, stamp. Host-owned ignores logical preimages; Windows ignores native ones. Settlement vocabulary is `Unattempted`, `PossiblyExecuted`, `RefusedProven`, `SupersededExternal`, `Applied`; unreadable preimages cannot prove refusal (`crates/tiler-core/src/world_report.rs:371-384,488-555`).
- D: Release transitions are retained source binding -> arrival-gated release, or deferred cleanup -> retry -> unbound native -> fresh source logical adoption; refusal rollback retains the original binding. `pending_source_releases` retains output/workspace pairs independently of parent retirement; listed non-current backing also adopts, without moving the retained view (`crates/tiler-core/src/world_settle.rs:245-359`; `crates/tiler-core/src/world_native.rs:1209-1221`; `docs/decisions.md:5907-5914`).
- D: Retirement transitions are mapped/retained -> globally eligible -> `WorkspaceDesire.existence=false` -> fresh adapter veto/removal -> complete absence readback -> binding prune/lifecycle. `NativeOccupancy.{native,count: Option<u32>}` uses unknown, not zero, for unreadable inventory; no retirement-specific operation/attempt id is introduced in these rows. Unknown/occupied/unmapped/protected/trailing/viewed, gesture barrier, floors and parity veto deletion; row/binding survive until absence proof (`crates/tiler-core/src/world_report.rs:327-347,734-779`; `crates/tiler-core/src/world_native.rs:297-350,354-462`; `crates/tiler-core/src/world_publish.rs:1059-1115`).

### Q(4). Adapter producers and consumers

| Adapter | Native evidence, attempts, creation and retirement |
|---|---|
| Windows | D: Logical receipts, no host-native mapping lane in inspected capture/sequencer; empty creation attempts and native member preimages `None`. Equal-count capture flag forwards to reports; inspected setup is false (`crates/tiler-windows/src/tiling_sys/world_capture.rs:563-586,1682`; `crates/tiler-windows/src/tiling_sys/world_sequencer.rs:339-401`; `crates/tiler-windows/src/tiling_sys/world_tick.rs:3229-3257,3481`). U: No native-workspace creation/retirement consumer was established in this pipeline. |
| macOS | D: Builder opts into host-owned, equal-count false, native order/current and explicit unknown occupancy; attempt arrays enter reports. Runtime stages native member/view/creation receipts. Select/send bind exact desires; creator uses fresh before/after and verified SID (`crates/tiler-macos/src/world_builder.rs:520-542,666-705`; `crates/tiler-macos/src/world_runtime.rs:178-221,224-314,327-343`; `crates/tiler-macos/src/runtime_thread.rs:842-860,921-939,1736-1787`; `crates/tiler-macos/src/workspace_select.rs:991-1006,1045-1072,1143,1847-1889`; `crates/tiler-macos/src/workspace_send_managed.rs:1596-1679`). U: No native-removal consumer established in inspected dispatch/runtime paths; unknown occupancy proves no emptiness. |
| KDE | D: `world-observe.ts` gates host-owned on readable desktop/current facts and emits order/current/all-window occupancy, unknown as null; `world-apply.ts` executes explicit desires, records causal creation receipts, native member/view preimages, and mechanically consumes deletion eligibility with fresh pin/view/occupant/minimum vetoes (`kwin/src/world-observe.ts:1323-1356,1465-1476`; `kwin/src/world-apply.ts:917-923,1443-1532,1548-1642,1729-1733`; `kwin/src/world-report.ts:175-184,213-258`). |

- D: Main adapter module sizes (whole-file lines): KDE observe/apply 1,673/2,429; macOS builder/dispatch 3,526/3,723; Windows capture/tick/sequencer 1,752/6,869/1,013 (`kwin/src/world-observe.ts:1-1673`; `kwin/src/world-apply.ts:1-2429`; `crates/tiler-macos/src/world_builder.rs:1-3526`; `crates/tiler-macos/src/world_dispatch.rs:1-3723`; `crates/tiler-windows/src/tiling_sys/world_capture.rs:1-1752`; `crates/tiler-windows/src/tiling_sys/world_tick.rs:1-6869`; `crates/tiler-windows/src/tiling_sys/world_sequencer.rs:1-1013`).

### Q(5). Trailing spare is not a numeric setting

- D: `plan_trailing` preserves a final empty/unoccupied spare, removes eligible empty non-tail rows and requests append when no spare exists or fewer than `MIN_WORKSPACES = 2` remain. `converge_logical_post` applies this lifecycle; `WorkspaceDesire.trailing` publishes the final spare designation (`crates/tiler-core/src/workspace.rs:3,20-65`; `crates/tiler-core/src/world_model.rs:22-30,91-113`; `crates/tiler-core/src/world_report.rs:327-347`; `crates/tiler-core/src/world_publish.rs:1080-1114`).
- I: "Trailing spare count" here describes total logical rows after adoption (four occupied bound plus ordinary spare), not a discovered numeric user setting. D: `equal_workspace_count` is a separate off-by-default capability for equal ring padding, not spare count; `host_owned_workspace_domains` selects native backing, not a lifecycle exception (`docs/decisions.md:5814-5825`; `docs/porting/world-contract-v3.md:107-114`; `crates/tiler-core/src/world_model.rs:196-279`).
- D: All three adapters use the common logical lifecycle; host-owned macOS/KDE leave an idle empty unviewed/untargeted spare unbound until needed, unlike eager native append. Windows uses the logical-only lane. KDE derives equal-count from observed per-output native list lengths; inspected Windows/macOS setups set it false (`crates/tiler-core/src/world_native.rs:85-95,714-754`; `crates/tiler-core/src/world_model.rs:22-30`; `kwin/src/world-observe.ts:1476`; `crates/tiler-macos/src/world_builder.rs:671-675`; `crates/tiler-windows/src/tiling_sys/world_capture.rs:1682`; `docs/spec/reference-outcomes.md:6415`).

### Q(6). Main core tests (static declarations, not executed)

| Mechanism | Main file and literal `#[test]` count |
|---|---|
| Native adoption/native-only receipts | D: `crates/tiler-core/tests/world_native_adoption.rs:198-1496`: 14; `crates/tiler-core/tests/world_host_owned_stage1b.rs:201-3169`: 32 (also creation). |
| Hybrid creation/replay/ambiguity | D: `crates/tiler-core/tests/world_host_owned_stage1b.rs:201-3169`: 32; ambiguous/no-blind-recreate discriminator at `crates/tiler-core/tests/world_host_owned_stage1b.rs:530-531`. |
| Native preimages and released source | D: `crates/tiler-core/tests/world_native_attempt_settlement.rs:384-2676`: 19; settlement at `:710-711`, all-closed release at `:2244-2245`, retry after creation resolves at `:2398-2399` in that file. |
| Partial/unreadable native evidence | D: `crates/tiler-core/tests/world_host_owned_gap_holds.rs:212-2198`: 23, including recovery at `:212-213` in that file. |
| Occupancy/retirement/readback | D: `crates/tiler-core/tests/world_native_retirement.rs:222-1686`: 29; unknown occupancy at `:374`, gesture barrier at `:559`, vanished desktop recycling at `:806` in that file. |
| Trailing designation | D: `crates/tiler-core/src/world_trailing_marker_tests.rs:157-334`: 6, including final empty spare in both streams at `:292` in that file. |

### Q(7). Recorded native evidence and remaining unknowns

- D: Adoption/count rows remain native TBD; backing/migration/released-source/redelivery rows explicitly record offline core/KDE coverage separately from native/reference TBD. Hybrid outside-first row records both core outcomes but KDE/macOS causal evidence remains TBD (`docs/spec/reference-outcomes.md:6425-6426,6431-6436,6438`).
- D: Trailing on-demand backing has recorded core/KDE desire-consumption regressions; exact native sequences and GlobalUnique creation remain TBD. Retirement records physical 2026-10-28 no-removal reports with unspecified sequence/configuration, then offline v3 coverage; exact native unmanaged-occupant/parity/readback outcomes remain TBD (`docs/spec/reference-outcomes.md:6406-6408,6415`).
- D: macOS native refusal/delayed arrival and deferred creation/readback timing remain TBD. A scoped signed-fixture hide/reveal observation records native current/AX inventory and restored identity/frame/membership/foreground, but is not creation/backing or full workspace acceptance; zero setter attempts and physical input remain TBD. Desire-backed numbered-send creation is recorded complete offline, live proof pending (`docs/spec/reference-outcomes.md:632-636,6439`; `docs/changes/macos-world-v2-port.md:924-926,1071-1079`).
- U: No tests or live legs were run. Current native acceptance, exact cross-platform correlation/readback timing, exhaustive adapter paths, macOS native retirement and a numeric spare-count setting were not established. Static test counts and recorded prior results do not establish current passing behavior.

## Evidence: package R (settling marker and border motion)

- Source inspection only; no tests or live legs run. KWin citations below use v6.6.3, the baseline named in `docs/research/d01-fullscreen-admission.md:112-115`; external source paths are labelled KWin.

### R(1). Pending-configure reproduction status

- Exact status: "A KDE/Wayland pending-configure risk remains source-derived, not a reproduced failure." (`docs/porting/world-contract-v3.md:578-579`). The discriminator still says "Native outcome and signal availability TBD" (`docs/spec/reference-outcomes.md:6430`). A reproduced pending-configure geometry misread was not found in the searched port notes, KDE change/archive records or configure-related test searches.
- Exact Windows qualifier: "The Windows log never showed a clamp Hold, so it does not establish that pending resize was misread as a clamp." (`docs/porting/world-contract-v3.md:582-583`). macOS notes likewise retain the conditional "only if a KDE/Wayland pending-configure failure is reproduced" (`docs/porting/macos-port-notes.md:143-147`).
- Distinct reproduced issue: "this reproduces the asynchronous false-refusal class already observed for fullscreen. No geometry-fighting defect is inferred." (`docs/changes/kde-live-acceptance.md:374-379`). Its archive excludes geometry policy and retains pending renewed live acceptance (`docs/changes/archive/kde-native-state-settlement.md:5-8,50-55`).
- Another archive describes asynchronous restore-configure timing at interactive cancellation from KWin source/upstream tests, not a reproduced core clamp misread (`docs/changes/archive/production-edge-drag-scripting-route.md:26-38`).

### R(2). KWin requested/current geometry and signals

- KWin documents `moveResizeGeometry()` as "last requested geometry" / "next geometry", versus current `frameGeometry()` (`KWin src/window.h:621-630`). The requested geometry is a plain C++ method there; the scripting limitation record explicitly reports no `Q_PROPERTY` or `Q_INVOKABLE` exposure (`docs/changes/archive/production-edge-drag-scripting-route.md:13-22`).
- Script-facing properties are writable `frameGeometry`, interactive `move` and `resize`; resize mode can make `frameGeometryChanged` stepwise or end-only (`KWin src/window.h:470-486`). Signals include `frameGeometryChanged`, `clientGeometryChanged`, `frameGeometryAboutToChange`, `moveResizedChanged` and interactive started/stepped/finished (`KWin src/window.h:1457-1468,1505-1509`).
- Effects expose `geometry` / `frameGeometry()`, move/resize start/step/finish and `windowFrameGeometryChanged`; step geometry can differ from current geometry in deferred mode (`KWin src/effect/effectwindow.h:45-49,400-405,683-714,752-757`).
- Internal Wayland state queues `m_configureEvents`, records `m_lastAcknowledgedConfigureSerial`, and consumes acknowledged events on commit (`KWin src/xdgshellwindow.cpp:114-168`). `maybeUpdateMoveResizeGeometry` avoids syncing committed geometry while the configure timer or position-changing unacknowledged events remain (`KWin src/xdgshellwindow.cpp:181-198`). Direct script/effect access to this pending queue/ack serial was not found in the inspected public surfaces above; interactive flags are not configure acknowledgements (inferred).

### R(3). Adapter-private representations and macOS AX gap

- Windows `WorldSequencer.pending_frames` stages latest-wins raw `FrameAttempt` rows for the next report; it is an attempt queue, not a native configure-completion signal (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:178-182,254-260`).
- KDE JS has `pendingUnfloat` and `pendingNative` (`wantMaximized`, `wantFullscreen`, scope), with `pollPendingNativeState` confirming once at the first fresh accepted boundary, not the issuing capture (`kwin/src/world-native.ts:84-94,260-283`). These are native maximize/fullscreen settlement, not a frame-configure queue. A private pending-configure/frame-settling representation in the searched KDE Rust adapter sources was not found.
- macOS `WorldSession.pending_frames` stages explicit float attempts; `pending_writes` correlates `(pid, cmd_id)` to window/frame/placement token, and `spent_frame_ops` prevents repeating explicit writes (`crates/tiler-macos/src/world_runtime.rs:100-110`). These represent write correlation/attempt staging, not an AX configure-ack marker (inferred).
- Existing macOS record explicitly says "AX writes, queued SLS membership and Mission Control transitions do not settle synchronously" and distinguishes successful dispatch from completion (`docs/changes/macos-world-v2-port.md:253`). `FrameChangeReport` retains preimage/attempted/applied/restore readback; restoration requires exact readback, not accepted OS status (`crates/tiler-macos/src/ax.rs:392-408,440-450`). A distinct native AX geometry-pending/committed marker was not found in the inspected adapter paths.

### R(4). Current core geometry settlement

- `settle_floats` correlates operation/window/target and retains `pending` attempt evidence; incomplete inventory, gesture barrier, minimized/unreadable facts hold. Success is exact `current == retained.target` without a fresh DPI flip (`crates/tiler-core/src/world_settle.rs:1756-1757,1770-1841`).
- Float refusal requires stable DPI, `Refused`, `pre != target`, `readback == pre` and `current == pre`; other fresh evidence latches `ambiguous` without re-actuation, while newer external geometry can abandon an unlatched target (`crates/tiler-core/src/world_settle.rs:1843-1889`). Pending/ambiguity state therefore exists, but these comparisons have no geometric tolerance.
- Ordinary tiled attempts run through `settle_frames` in `world_frames.rs`, not `world_settle.rs`: staged facts feed `consume_attempt`; `attempt_observed` prefers the staged frame over immediate readback (`crates/tiler-core/src/world_frames.rs:127-139,638-705`). Exact target equality clears memory; corroborated accepted mismatch clamps, changed mismatched readback re-arms, and unchanged exact desired/observed pairs can suppress (`crates/tiler-core/src/world_frames.rs:87-104,170-239`). A native configure-pending input is not present in those inspected comparison paths.

### R(5). Production borders during motion

- Windows replays absolute `last_reply().presentation.border.outer` when the foreground token matches and the reply border is visible; fresh eligibility and geometry vetoes still apply. This path does not derive border position from the newly read frame (`crates/tiler-windows/src/tiling_sys/focus.rs:272-316,345-358`). The carrier presents at the supplied outer rectangle (`crates/tiler-windows/src/active_border_sys/carrier.rs:173-197`); native anchor tracking was not found in this border route.
- KDE parents the border item to the tracked window item, but frame changes invalidate strict coverage; genuine mover motion hides border/underlay until fresh covering paint (`kwin/native-effect/activewindowborder.cpp:624-630,1121-1135`; `kwin/native-effect/presentation_stage5.cpp:97-111`). Visible paint replays accepted absolute inner/outer geometry through scene-to-item translation, not a relative descriptor (`kwin/native-effect/activewindowborder.cpp:1580-1599`).
- macOS production loads `last_presentation()` and calls `tick_reply`; that route uses absolute `border.outer`, only flipping coordinates, with native visibility/focus/Space/safety vetoes (`crates/tiler-macos/src/tile.rs:1927-1974`; `crates/tiler-macos/src/border_runtime.rs:513-523,562-586,607-659`). No motion-specific hide gate or native anchor tracking was found in that inspected reply route; absolute replay during otherwise eligible motion (inferred).

### R(6). macOS drag evidence status

- Exact port status: "Passive titlebar drag remains parked." Active Meta drag shares the offline Started/Updated/Released/Cancelled lifecycle and regressions (`docs/changes/macos-world-v2-port.md:331-346`). The later gate says "passive producer parked; no accepted native drag journey" and separately retains appearance acceptance pending (`docs/changes/macos-0.1-gate-proposal.md:115,119`).
- A macOS live drag-driven border-motion visual pass was not found in the searched macOS change/archive records; offline lifecycle coverage above is not a recorded native border-motion observation.

### R(7). Existing native event/paint routes, without a new polling loop

- KWin has `prePaintWindow` / `paintWindow` callbacks receiving `EffectWindow`, whose `frameGeometry()` exposes live geometry (`KWin src/effect/effect.h:637-649`; `KWin src/effect/effectwindow.h:400-405`). The current effect already parents the border to the window item and subscribes to `windowFrameGeometryChanged` (`kwin/native-effect/activewindowborder.cpp:624-630`); its visible mapping route is cited in R(5), not evidence of continuous motion display.
- Windows already installs move-size start/end and location-change hooks; `EVENT_OBJECT_LOCATIONCHANGE` wakes observation as `HookCause::Other` (`crates/tiler-windows/src/tiling_sys/run_loop.rs:397-407`; `crates/tiler-windows/src/tiling_sys/observe.rs:423-453`). This establishes an event route, not a native border anchor implementation.
- macOS subscribes to app `AXWindowMoved`/`AXWindowResized` and window `AXMoved`/`AXResized`; delivered notifications trigger immediate rereads, with a separate one-second silence backstop (`crates/tiler-macos/src/ffi.rs:931-955`; `crates/tiler-macos/src/native_tile_backend.rs:752-765`). This establishes a notification route, not visual tracking acceptance.

### R(8). Border reply shape

- `WorldReply.presentation: WorldPresentation` contains `border: BorderPresentation` (`crates/tiler-core/src/world_report.rs:1353-1358`; `crates/tiler-core/src/world_presentation.rs:111-127`). `BorderPresentation` carries `visible`, optional hide `reason`, `inner: Option<Rect>` and `outer: Option<Rect>`, documented as complete bounds in adapter units; relative/outset fields are not present in this Rust type (`crates/tiler-core/src/visual.rs:197-207`).

## Evidence: package S (KDE effect Rust viability)

- Evidence date: 2026-10-31. This section records sources and unknowns, not a selected design or recommendation. "Verified" means inspected source, history, or primary publication; a recorded historical test result is not a newly executed test. No build, runtime test, or live acceptance was performed for this package.
- Verified current scope: "Our own code is Rust wherever a host allows it" and "Weigh Rust's build, packaging and distribution cost against the complexity it removes from other languages" (`docs/principles.md:248-252`). Required C++ uses RAII and ASan/UBSan tests; the same record explicitly says sanitizer CI is not yet in place (`:259-265`).
- Verified source conventions: native filenames below expand to `kwin/native-effect/<filename>`; counts are physical `wc -l` lines, including comments/blanks. KWin citations use sibling-source pin `8438567a741826da8b7536a8b10eb3af8fc8820d` (v6.7.4), not clone HEAD. Historical `revision:path:line` citations refer to `git show` output.

### A. Native source/header inventory and Rust concept crosswalk

- Verified inventory: 35 `.cpp`/`.h` files, 31,042 lines: 23 non-test files (14,095 lines, including generated descriptors) and 12 test files (16,947 lines). Classification 1 = host ABI/integration (including D-Bus and settings UI); 2 = host-free state/parsing/computation. Qt value types do not by themselves make a state machine KWin-ABI-bound. Mixed files are not assigned speculative movable percentages. Include flags are direct includes only: K = KWin, Q = Qt, F = KDE Frameworks; `-` = none. Project-header transitive dependencies are stated separately.

| Verified file | Lines | Direct K/Q/F includes, with source | Role / classification and source |
| --- | ---: | --- | --- |
| `activeborderconfig_module.cpp` | 7 | -/-/F (`:3`, KPluginFactory) | 1: KCM plugin factory (`:5-7`). |
| `activeborderconfig_module.h` | 16 | -/-/-; includes unified KCM header (`:3`) | 1: thin KCM subclass (`:8-14`). |
| `scriptconfig_module.cpp` | 7 | -/-/F (`:3`, KPluginFactory) | 1: script KCM plugin factory (`:5-7`). |
| `scriptconfig_module.h` | 16 | -/-/-; includes unified KCM header (`:3`) | 1: thin KCM subclass (`:8-14`). |
| `activewindowborder.cpp` | 1,855 | K/Q/F (`:10-34`): KColorScheme/KSharedConfig; core/inputdevice, effecthandler, input/input_event/input_event_spy, options, scene, window/workspace; Qt D-Bus/image/timer/value types | 1: effect construction, native signals/input observation, EffectWindow/scene rendering and factory; local geometry/quantization helpers also occur (`:202-283,286-440`; paint mapping `:1580-1599`). |
| `activewindowborder.h` | 325 | K/Q/- (`:9-21`): effect/effectwindow, scene image/outlined-border items; Qt connection/containers/geometry | 1: `ActiveWindowBorderEffect : Effect`, paint hooks and native object state (`:30-32,176-325`). |
| `activeborderlogic.h` | 124 | -/Q/- (`:14-16`, QColor/QRectF/QString) | 2: facts JSON, stale/lease checks, color selection (`:44-121`). |
| `gesture_lifecycle.h` | 457 | -/Q/- (`:57-60`, Qt namespace/QPoint/QRect/QString) | 2: gesture identity, phases, start/update/finish/abort, terminal FIFO/ack and JSON snapshots (`:78-148,182-405`). |
| `gesturefacts.h` | 135 | -/Q/- (`:57-64`, QObject/QDBusContext/message/connection/timer/value types) | 1: gesture D-Bus endpoint and scriptable methods (`:69-71,109-112`), containing the host-free lifecycle store. |
| `gesturefacts.cpp` | 251 | -/Q/- (`:7-11`, D-Bus interface/error, JSON, UUID) | 1: bus ownership, facts publication/waiters and JSON transport (`:3-251`). |
| `presentation_consumer.h` | 843 | -/Q/- (`:105-112`, QByteArray, JSON, QRect/QString/List) | 2: payload parsing/validation, ordered paint state, coverage/freshness fences and invalidation (`:130-350,527-841`). |
| `presentation_router.h` | 260 | -/Q/- (`:25-27`, consumer header/QString) | 2: invalidation/generation/floor routing and paint-state coordination (`:32-260`). |
| `presentation_stage5.h` | 176 | -/Q/- (`:20-23`, project headers/QString) | Mixed: 2 host-view records, mover classifiers and refresh interfaces (`:35-98`); 1 injected facts-endpoint adapter (`:100-169`). |
| `presentation_stage5.cpp` | 301 | -/-/- directly (`:3`, project header) | Mixed: 2 counters/invalidation/refresh orchestration; 1 calls to injected facts publisher (`:15-35,97-111,120-300`), no direct KWin API. |
| `presentationfacts.h` | 88 | -/Q/- (`:5-10`, QObject/QDBusContext/connection/message/QString/QTimer) | 1: presentation D-Bus endpoint (`:32-34,59-60`). |
| `presentationfacts.cpp` | 196 | -/Q/- (`:7-11`, D-Bus interface/error, JSON, UUID) | 1: presentation facts transport and waiters (`:3-196`). |
| `generated/kde_settings_descriptors.h` | 147 | -/-/F (`:18-19`, KLocalizedString/KLazyLocalizedString) | 2: generated localized settings descriptor data (`:24-147`), not widget/host mutation. |
| `kdesettings_adapter.h` | 102 | -/Q/F (`:19-24`, KConfigGroup, QString/List, QVariantMap, generated descriptors) | 1: KConfig/widget settings adapter interface (`:26-102`). |
| `kdesettings_adapter.cpp` | 317 | -/Q/- (`:3-9`, checkbox/combo/spinbox/label/tab/widget) | 1: descriptor/config-to-widget value binding (`:1-317`). |
| `shortcutreconciler.h` | 1,191 | -/Q/- (`:3-11`, D-Bus data/messages, key sequence, containers/logging/variants) | Mixed: 2 reconciliation models and interfaces; 1 transport-shaped D-Bus/key data contracts (`:22-1191`); not a KWin subclass. |
| `shortcutreconciler.cpp` | 4,761 | -/Q/F (`:3-24`, KConfig/Group; D-Bus, filesystem, key sequence, regex, standard paths, XML) | Mixed: 2 catalog/key parsing, desired/current comparison and reconciliation; 1 native config, bus-owner/readback, persistence/journal I/O (e.g. `:889-1036,2583-2728,4194-4207,4448-4525`). |
| `unifiedsettings_module.h` | 263 | -/Q/F (`:3-10`, KCModule, QThread/DBus/value types) | 1: KCModule, UI and asynchronous worker declarations (`:27-263`). |
| `unifiedsettings_module.cpp` | 2,257 | -/Q/F (`:5-26`, KConfig/KSharedConfig/KCMultiDialog/i18n; widgets/dialogs/bus/event loop) | 1: settings UI, load/save/apply, shortcut dialogs and hot-apply (`:1-2257`). |

- Verified ancillary host inputs (not counted as C++ source/header): `unifiedsettings.ui`, plugin JSON metadata, KConfig `.kcfg/.kcfgc`, and CMake validation scripts. UI generation and both factory targets are wired at `CMakeLists.txt:56-58,97-153`; effect factory integration is at `:46-54,60-85`.

| Verified test source | Lines | Direct K/Q/F and include citation | Tested area / boundary |
| --- | ---: | --- | --- |
| `activeborderlogic_test.cpp` | 103 | -/Q/- (`:9`) | 2: pure facts/stale/lease/color helpers. |
| `presentationconsumer_test.cpp` | 895 | -/Q/- (`:14-15`) | 2: strict JSON shape, ordering/coverage, stale resurrection, publication/clear. |
| `gesturelifecycle_test.cpp` | 349 | -/Q/- (`:28`) | 2: gesture lifecycle/terminal retention/ack. |
| `gesturefacts_test.cpp` | 630 | -/Q/- (`:22-34`) | 1: private-bus gesture transport; exercises lifecycle through endpoint. |
| `presentationfacts_test.cpp` | 540 | -/Q/- (`:24-38`) | 1: private-bus publication/waiter transport. |
| `presentation_stage5_test.cpp` | 748 | -/Q/- (`:24-26`) | Mixed: production handlers/state plus private bus and stub host views. |
| `presentation_stage5_recovery_test.cpp` | 940 | -/Q/- (`:66-77`) | Mixed: recorded-fixture recovery/refresh through production handlers and private bus. |
| `activeborderconfig_module_test.cpp` | 799 | -/Q/F (`:6-22`) | 1: KCM config/widgets/D-Bus hot apply. |
| `activeborderconfig_shortcut_test.cpp` | 3,060 | -/Q/F (`:4-21`) | Mixed: KCM shortcut UI/reconciliation/recovery. |
| `kdesettings_adapter_test.cpp` | 623 | -/Q/F (`:11-27`) | 1: settings descriptor/config/widget adapter. |
| `scriptconfig_module_test.cpp` | 1,723 | -/Q/F (`:3-18`) | 1: script KCM settings/config/hot apply. |
| `shortcutreconciler_test.cpp` | 6,537 | -/Q/F (`:3-17`) | Mixed: reconciliation decisions and KConfig/D-Bus/filesystem seams. |

- Verified overlap is at the concept/type level, not proof of identical authority: native `GesturePhase`, snapshots and lifecycle (`gesture_lifecycle.h:78-148,182-405`) correspond to Rust `GestureKind`, `GesturePhase`, `GestureIngress`, `GestureSettled` (`crates/tiler-core/src/gesture.rs:24-149`) and their wire decoding (`crates/tiler-protocol/src/world_protocol.rs:1087-1114`). Core explicitly owns retained gesture session, pointer resolution and commit decisions; native identity/freshness/Esc observation remains adapter mechanical evidence (`gesture.rs:1-18`).
- Verified presentation overlap: native facts/consumer/router/stage5 retain JSON geometry, generations, revisions and invalidation; Rust owns `PresentationIngress`/facts (`crates/tiler-core/src/world_report.rs:633-655`), ingress decoding (`crates/tiler-protocol/src/world_protocol.rs:1140-1202`), border/underlay decisions and publication (`crates/tiler-core/src/world_presentation.rs:158-162,311-315,599-600,800-808`) and border geometry (`crates/tiler-core/src/visual.rs:197-207,264`). Unverified: semantic equivalence of these separate native consumer and core producer state machines; no matching full paint-consumer/router implementation was identified in those Rust crates.
- Verified earlier sharing: commit `c1ccbcfc` moved active-border/drag-preview/group-underlay policy into portable core while retaining native observation/rendering. Unverified: a complete symbol-by-symbol Rust equivalent of shortcut reconciliation or KDE widget/settings binding; none was established in this evidence.

### B. Recorded direction, removal and hurdles

- Verified exact earlier decision: "extend the out-of-process Planner first (route A)"; "measure interaction before any native replacement; effect-linked Rust (route B) only for optional acceleration" (`docs/decisions.md:4057-4068`, 2026-10-17). Reopening: "Prefer Rust over effect C++ wherever KWin allows" and "re-weigh effect-linked Rust (route B) beyond optional acceleration, setting the earlier build, packaging and distribution findings against the complexity now in effect C++" (`:4069-4073`). This was not a blanket record that Rust had never been allowed in the effect.
- Verified AR10 wording: "CMake invokes Cargo to build the workspace `tiler-kwin-effect-ffi` staticlib"; "Rust keeps group visibility and drag verdict policy; the POD-only C ABI and panic containment remain" (`docs/decisions.md:4117-4120`). Retirement: "the drag oracle moved into the effect's native gesture lifecycle and the unused Rust FFI crate was removed; the effect links no Rust today" (`:4121-4123`).
- Verified route comparison, historical research (explicitly inference, not measured): A has "Best process isolation; no additional KWin ABI coupling. Protocol rollout/versioning and recovery required"; B has "Direct C ABI calls synchronous; worker-only pure computation, owner-thread snapshot/apply" and "No crash isolation; exact KWin ABI rebuild and existing optional-effect packaging. Must retain effect-independent tiling/fallback" (`docs/research/kwin-rust-migration.md:60-65`). Research snapshot is `552a54ae`, not current inventory (`:3-6,28-34`).
- Verified edge-drag proposal: "Rust-first is achievable for policy and the engine, but a pure-Rust KWin effect is unproven"; "Estimate 60-110 C++ lines" for subclass/factory/moc, signals and POD projection; cxx-qt/qmetaobject-rs replacements were unproven (`docs/changes/rust-first-edge-drag-route.md:24-35`). The file now labels its DescribePlan transport discussion historical (`:3-5`).
- Verified historical transport uncertainty: "additional IPC adds an ordering race" and a synchronous effect-to-production-script bridge was unproven (`rust-first-edge-drag-route.md:51-56`). This is a recorded proposed-route hurdle, not a current world-route defect.

| Verified recorded hurdle | Evidence class / source |
| --- | --- |
| Qt/KWin factory/moc and native C++ ABI remain; pure-Rust replacement not demonstrated | Historical unverified feasibility claim/line estimate, not a failed Rust implementation (`rust-first-edge-drag-route.md:24-35`). |
| Synchronous FFI inherits callback latency; worker results need owner-context return; no native pointers on workers | Source-derived constraints/proposal, no performance measurement (`kwin-rust-migration.md:48-60,65`). |
| Linked Rust shares compositor process; catch_unwind cannot isolate abort, invalid pointers/UB, allocation failure or C++ crashes | Source-derived safety/failure distinction (`kwin-rust-migration.md:56,91`), not an observed Rust crash here. |
| Rust panics cannot escape the C/Qt boundary | Required boundary behavior (`rust-first-edge-drag-route.md:46-47`); AR10 reports all 11 callbacks caught panics (`docs/changes/archive/architecture-review-ar10-cargo-effect.md:22`). |
| KWin ABI rebuild and optional-effect packaging/fallback remain despite stable internal C ABI | Source-derived host/distribution constraints, not uniquely introduced by Rust (`kwin-rust-migration.md:49-50,65`; pinned host evidence in C). |
| Boundary code was spread across several effect Rust modules rather than one tiny dedicated FFI module | Recorded structural mismatch at research snapshot (`kwin-rust-migration.md:91`); later allowlist and module deletion are distinguished in C. |
| Cargo/CMake needs complete transitive rebuild dependencies and correct CTest Cargo/offline arguments | Observed review issues, recorded fixed, not outstanding build failures (`architecture-review-ar10-cargo-effect.md:23`). |
| Staticlib packaging needs locked vendored source and Cargo offline configuration | Implemented build requirement, with recorded successful Nix builds (`architecture-review-ar10-cargo-effect.md:15-17,25-27`). No new system dependency was reported (`:27`). |
| Static linking requires dependency notice coverage; GPLv2-only cannot combine with Apache-2.0-only | License/distribution constraints, not Rust compilation failures (`docs/research/licensing.md:366-369,476-479,511-527`; E below). |
| Worker/backend/unload timing, throughput and gaming cost | Explicitly unknown/unmeasured (`kwin-rust-migration.md:55,108-117`), not proof of a Rust regression. |

- Verified historical outcomes, as reports only: AR10 records 629 Rust tests including 36 effect tests, 27/27 CTest, host-builder/dogfood tests, and successful Nix builds against two KWin versions; it claims no native runtime acceptance (`architecture-review-ar10-cargo-effect.md:24-27`). No Rust compilation failure as the reason for removing the crate was found in the cited records.
- Verified commit `5a12e8aea47feb29b954dfcabb9a945cfaf27cca` message: "Build native effect Rust with Cargo and vendored Nix dependencies". Whole-commit diffstat: 20 files, +1,449/-1,178; crate-scoped: 4 files, +1,894. It converted pre-existing bare-rustc effect Rust, rather than introducing the first effect Rust (`architecture-review-ar10-cargo-effect.md:5,22`). Whole diff includes Cargo manifests/lock, crate, CMake/flake, host builder/tests, dogfood tests, C ABI consumer tests and records.
- Verified commit `21af12a2b2ca4700f77fb82d163cfdba0dc4d68f` message: "wip(kde): draft authoritative native gesture transport". Body: "UNACCEPTED: two retained JavaScript recovery regressions prove unbounded fast terminal-capture retries and a malformed-response recovery stall. One native lifecycle owns identity and causally fenced Esc cancellation; broker folding and exact settlement association replace the duplicate oracle route. Remove its unused Rust FFI, build wiring and test-only unsafe inventory. Stage 5 preview freshness remains a separate unresolved package; no live gesture acceptance is claimed." Whole diffstat: 97 files, +6,716/-4,965; crate-scoped: 3 files, -927. The reported regressions are JavaScript recovery/transport issues, not an attributed Rust build failure.
- Verified remaining crate history below is exhaustive for `git log --all -- .../tiler-kwin-effect-ffi`; stats are crate-scoped, so moves outside that path can appear as additions/deletions. Commit subjects are the messages for commits without a body. The two whole-commit statistics above must not be confused with these path-scoped totals.

| Verified commit | Message / subject | Crate-scoped diffstat |
| --- | --- | --- |
| `92010762` | wip(kde): draft Stage 5 read-only native renderer | 4 files, +8/-1,567 |
| `bfbe6022` | refactor(kde): remove the retired plan route and DescribePlan transport | 1 file, +4/-4 |
| `1fe27640` | refactor(windows): consolidate unsafe FFI behind safe boundary | 5 files, +10 |
| `63826598` | Apply MIT license with GPL native KWin effect exception | 1 file, +1 |
| `e89687ac` | Deliver KDE movement-only group underlay stages A/B | 2 files, +159/-15 |
| `c1ccbcfc` | Move active border, drag preview and group underlay policy into portable core | 3 files, +175/-3 |
| `6f25bb96` | Order group highlights by script correlation across workspaces | 1 file, +42/-23 |
| `aa94bd98` | Give the group underlay stream a per-instance generation and log effect-side group outcomes | 1 file, +9/-7 |
| `dd87158c` | Serialize drag oracle tests that share the last-verdict state | 1 file, +8 |
| `910af1cf` | Replace group outline with a sliding underlay and make underlay and drop preview colours configurable | 1 file, +77/-36 |
| `5fe0eb5e` | Apply resilience changes 3a, 3 and 4 and fail-closed sweep | 1 file, +104/-10 |
| `fcc3bb8a` | Keep dev tray running and recover tray and Planner failures | 1 file, +109/-8 |

- Verified `92010762` body calls frame-dirt/in-flight snapshots, cross-gesture preview publication and focus-clear ordering "causally unproven" and says helper/build gates did not establish native acceptance. It removed group-highlight/visual FFI; `21af12a2` removed the remaining oracle. `bfbe6022`'s body removes the unshipped plan/DescribePlan route while keeping the group-highlight formatter independent; it does not report a Rust linking failure. The serial-test commit `dd87158c` is recorded evidence of tests sharing the last-verdict state, not a compositor threading failure.

### C. Historical linkage/unsafe mechanics, host threading and bridge ecosystem

| Verified mechanism | Source / detail |
| --- | --- |
| Addition crate | `5a12e8ae:crates/tiler-kwin-effect-ffi/Cargo.toml:1-13`: edition 2024, staticlib, dependencies tiler-core/serde/serde_json; lib + drag_oracle/group_highlight modules in that tree. |
| Last crate state | `21af12a2^:crates/tiler-kwin-effect-ffi/Cargo.toml:1-14`: MIT, staticlib, empty dependencies, standalone unsafe_code deny. This differs from the addition/research snapshot. |
| Cargo invocation | `21af12a2^:kwin/native-effect/CMakeLists.txt:85-126`: release/locked Cargo package build; optional --offline from CARGO_NET_OFFLINE, optional source replacement from OMNITILER_VENDOR_DIR; explicit manifest/target-dir/output dependencies and custom target. |
| Link | Same historical CMake `:129-153`: effect depends on omnitiler-effect-ffi-rs and links libtiler_kwin_effect_ffi.a, Threads::Threads, CMAKE_DL_LIBS, KWin::kwin, KF6 Config/ColorScheme, Qt Core/Gui/DBus. Addition-revision Cargo/link glue is at `5a12e8ae:kwin/native-effect/CMakeLists.txt:55-118`. |
| C ABI layout | `21af12a2^:kwin/native-effect/drag_oracle_ffi.h:4-32`: int32 rectangle, byte flags/double press coordinates; sizeof 24 and offset 8/16 assertions match repr(C) Rust; record/borrowed-last/copy-last exports. No Qt/KWin object is an ABI argument. |
| Addition group ABI | `5a12e8ae:kwin/native-effect/group_highlight_ffi.h:9-75`: POD state/status/rect, bounded byte arrays/lengths, eight group exports: state_init, apply, clear, focus_matches, focus_eligible, is_visible, rect, status (all group_highlight_ prefixed). Together with the three oracle functions these are AR10's 11 exports. |
| Last three exports | `21af12a2^:crates/tiler-kwin-effect-ffi/src/drag_oracle.rs:178-264`: #[unsafe(no_mangle)] extern C drag_oracle_record, drag_oracle_last, drag_oracle_last_copy; each uses catch_unwind with zero/null fallback; record has nested fallback catch. |
| Raw memory operations | Same file `:192,213-215,244-246`: from_raw_parts on caller identity bytes, caller out-length write, copy_nonoverlapping into caller buffer. The borrowed-last API returns guard.as_ptr after the guard scope; copy-last copies while locked (`:211-217,241-247`). Caller pointer validity is not proved by null/size checks. Test-only raw-parts calls also occur at `:720,847,880`. |
| State synchronization | Same file `:205-264`: mutex-protected last JSON and explicit poisoned-storage reset/fail-closed result; serialization of tests sharing last state is independently visible in dd87158c. |
| Unsafe allowlist | `21af12a2^:crates/tiler-core/tests/unsafe_guardrail.rs:7-28,53-67` explicitly permits this crate's standalone deny and drag_oracle.rs allow root. Comments distinguish already-deleted group/visual modules. Earlier 1fe27640 included those roots too; final removal deletes the remaining crate/root and associated test fixtures. This was a live allowlist entry, not only a synthetic fixture. |
| Panic/allocator/visibility configuration | No explicit panic profile, global_allocator override, Rust linker export/version-script/visibility setting was found in the searched historical root Cargo/config, effect crate or CMake. Observed protection is catch_unwind plus no_mangle exports. Unverified: exact final symbol table/allocator implementation and unwind settings of a reproduced binary; no historical binary was rebuilt. |

- Verified pinned host loading: KWin `src/effect/effect.h:1091-1128` uses a C++ EffectPluginFactory subclass of KPluginFactory; `:1131,1144-1145,1161` supplies versioned IID, explicitly disclaims binary compatibility and emits Qt plugin metadata. `src/effect/effectloader.cpp:293-323` checks IID, loads Qt plugin instance and casts factory; `:363-388` makes the graphics context current and creates the effect.
- Verified explicit threading wording: "Loading the Effect has to be performed in the Compositor thread and thus blocks the Compositor" (KWin `src/effect/effectloader.h:121-129`); the comment separates background I/O and queues effects one per event cycle. KWin `src/main_wayland.cpp:152-158,661-663` constructs compositor/workspace/scene/plugins and enters a.exec(). Render-loop timer emits frameRequested (`src/core/renderloop.cpp:176-192`), connected by compositor (`src/compositor.cpp:928`); composite/render path is at `:652,725,798-809`; effect paint forwarding is at `src/effect/effect.cpp:376-383`.
- Verified scope limit: backend DRM/libinput/Wayland worker threads exist (`src/backends/drm/drm_commit_thread.cpp:33-34`, `src/backends/libinput/libinputbackend.cpp:21-25`, `src/backends/wayland/wayland_display.cpp:57` at the pin). The inspected load/paint call chains do not establish an every-backend/every-callback main-thread contract. Unverified: exhaustive callback affinity and worker/unload lifetime rules for a new Rust bridge.

| Verified ecosystem option, no selection | Primary source / observed version |
| --- | --- |
| Plain C ABI/staticlib | [Rust Reference linkage](https://doc.rust-lang.org/reference/linkage.html), [Rust Nomicon FFI](https://doc.rust-lang.org/nomicon/ffi.html): Rust staticlib is usable by a foreign linker, extern C/repr(C) describe the bridge. This is a compiler/language facility, not a separately versioned bridge crate. Historical OmniTiler implementation above demonstrates it. |
| CXX | [Project](https://cxx.rs/), [primary crate API](https://crates.io/api/v1/crates/cxx): latest/stable 1.0.205, published 2026-10-11; Rust/C++ bridge. No KWin factory replacement is established by that API. |
| CXX-Qt | [Project](https://kdab.github.io/cxx-qt/), [primary crate API](https://crates.io/api/v1/crates/cxx-qt): latest/stable 0.10.0, published 2026-08-24; Qt QObject interop. KWin effect-factory suitability remains unverified. |
| Corrosion | [Project](https://github.com/corrosion-rs/corrosion), [latest-release API](https://api.github.com/repos/corrosion-rs/corrosion/releases/latest): v0.6.1, published 2026-01-17; CMake integration for importing Rust executables/static/shared libraries. The older verified v0.6.0 release requires CMake 3.22; compatibility with this tree's declared 3.19 floor has not been tested. |

- Verified generic linker warning: the Rust Reference linkage section says staticlib includes upstream dependencies/public symbols and warns that separately linking multiple Rust staticlibs with a foreign linker is likely to conflict; it advises a single combined staticlib and limiting shared-library exports. This is link-time evidence. Unverified: collisions/interposition between independently loaded Rust-backed KWin plugin DSOs; that different dynamic-loading case was not demonstrated.
- Verified bounded negative search: git grep for Rust/Cargo/staticlib/cdylib terms in pinned KWin, excluding translations/JSON, found no Rust build integration. Unverified: whether a deployed KWin loads any other Rust plugin; absence from the pinned source does not establish ecosystem-wide absence.

### D. Current build graph, CI and Rust toolchain declarations

- Verified CMake: C++20 (`CMakeLists.txt:1-6`); both OMNITILER_BUILD_EFFECT and OMNITILER_BUILD_SETTINGS default ON (`:19-20`). ECM floor 6.24, Qt Core/DBus/Gui/Widgets, KF CoreAddons/Config/I18n/KCMUtils/WidgetsAddons; effect additionally discovers KWin >=6.6 and KF ColorScheme (`:22-43`). Production effect links only C++/Qt/KF/KWin targets, no Cargo/staticlib (`:60-85`). KCM factories share unifiedsettings/adapter/reconciler/config/UI sources and link Qt/KF without KWin (`:97-153`).
- Verified current API-version maintenance: KWin >=6.8 changes paintScreen return spelling via OMNITILER_KWIN_PAINTSCREEN_BOOL; source notes the effect API macro alone did not distinguish 6.7/6.8 (`CMakeLists.txt:86-94`). This is existing host C++ API cost, not evidence of a Rust-specific problem.
- Verified recipes: root just imports platform recipes; `just/linux.just:1488-1498` builds omnitiler, `:1501-1509` checks core portability, `:1513-1521` builds script, `:1524-1569` resolves exact host KWin provenance, invokes scripts/nix-host-kwin-build.sh build and stages effect plus two KCMs. Aggregate build depends on all three (`:1586`). `devenv.nix:4-6,8-34` enables Rust/JavaScript and declares development tools; native KWin dependency realization is handled by the host-matched/Nix build route, not an explicit effect target in devenv.nix.
- Verified Nix split: effect derivation has CMake/Ninja/pkg-config/ECM and KWin/Qt/KF inputs, no Cargo/Rust inputs (`flake.nix:88-131`); shipping effect-only, test variant settings ON. Settings derivation has Qt/KF inputs, effect OFF/settings ON, no KWin or Rust (`:179-214`). Install checks assert effect and KCM namespace paths and separation (`:143-174,223-240`).
- Verified CI: `.github/workflows/ci.yml:42-49` runs cargo test --workspace, explicit unsafe guardrail, fmt and strict Clippy. Native job invokes Nix native-effect-tests/native-settings-tests checks (`:97-99`); flake runs offscreen/isolated-config ctest --output-on-failure (`flake.nix:162-174,232-240`). No ASan/UBSan configuration was found in inspected native CMake/flake/CI; current principle independently records that gap (`docs/principles.md:259-265`).
- Verified declaration search: no rust-version in workspace Cargo manifests and no repository rust-toolchain/rust-toolchain.toml found. Root Cargo.toml only declares members/resolver and forbid unsafe (`:1-6`); mise.toml uses rolling stable for Windows/macOS, devenv enables Rust without a version. Nix dependency revisions, rather than an explicit project Rust channel/MSRV, are locked (flake.lock nixpkgs 54ba4bcec4043e72a4006d825e0d7aff5562008f; devenv.lock also pins its inputs). Unverified: exact Linux rustc selected from those closures and a declared/tested project MSRV; no toolchain realization/version acceptance was performed.

### E. Packaging, distribution and licensing

- Verified release-source contract: scripts/build-source-archive.sh:4-37,183-201,209-219 generates versioned source tarball plus checksum, prebuilt JS, cargo-vendored sources, revision identity and tracked notices; it validates locked/offline Cargo with source replacement. Consumers do not need npm/network during package build. Packaging is tentative/pre-release, not proof of published delivery (`packaging/arch/PKGBUILD:1-10,21-26`, `packaging/rpm/omnitiler.spec:1-12`).

| Verified route/files | Existing Rust build and effect split |
| --- | --- |
| Arch: packaging/arch/PKGBUILD; omnitiler.install, omnitiler-native-effect.install | makedepends includes rust (rustc + cargo), CMake, Qt/KF/KWin (`PKGBUILD:27-32`); "build --release --locked --offline -p omnitiler" with vendor source replacement (`:49-61`). One native build is split into core/script/two KCMs and effect-only companion (`:64-133`). Effect runtime KWin dependency deliberately unversioned (`:109-115`). AUR-only, not an OBS pacman repo; unpublished (`:1-3`; docs/installation.md:203-206). |
| Debian/Ubuntu: packaging/debian/control, rules, copyright and companion package metadata | "Core-only profile: DEB_BUILD_PROFILES=noeffect skips the effect build" (`rules:12-14`). Rust binary uses identical locked/offline vendor command (`:51-56`); independent settings-only and effect-only CMake builds (`:57-78`); exact compiled-against libkwin6 distro version is recorded in effect dependency (`:113-123`; control:41-50). Settings remain in core. |
| RPM: packaging/rpm/omnitiler.spec | BuildRequires cargo/rust, CMake/compiler/KWin/KF/Qt (`:22-55`), without a Rust version bound. Vendor/offline Rust command followed by full native build (`:139-165`); core includes settings, effect is a companion (`:74-85,95-121`). KWin runtime deliberately unversioned (`:99-109`). |
| OBS: packaging/obs/_service; release workflow | Service pins a versioned release archive/checksum; placeholder checksum fails closed. No provisioned OBS delivery or completed automated source upload: trigger_services only re-runs already-uploaded sources (`docs/installation.md:223-247`). This is an existing distribution blocker, not attributed to Rust-in-effect. |
| Nix: flake.nix, nixos-module.nix, home-manager-module.nix, nix/com.omnitiler.Planner.service | mkTray uses rustPlatform.buildRustPackage, Cargo.lock and package omnitiler (`flake.nix:291-301`); effect/settings are separate stdenv derivations without Rust (`:88-131,179-214`). NixOS installs script/effect/settings split (`nixos-module.nix:17-19`). |

- Verified recorded distro scope: openSUSE Tumbleweed first OBS target; Fedora 43/44 same spec with updates repository; Arch AUR only; Ubuntu 26.04/26.10 Debian recipe; no KDE neon provisioning (`docs/installation.md:191-221`). Recorded distro rustc/cargo versions (not MSRV claims) are Tumbleweed/Fedora 1.98.1, Arch 1.99, Ubuntu 26.04 1.93.1, Ubuntu 26.10 1.97.1 (`:191-218`). Actual recipes do not encode these as minimum Rust versions.
- Verified requirement comparison, not a cost verdict: full Arch/RPM/Debian source builds already require Rust/Cargo and vendored crates for omnitiler. Their recipes still pass OMNITILER_VENDOR_DIR to effect CMake (`PKGBUILD:54-56`, rpm spec:148-158, debian/rules:66-77), but current CMake contains no Cargo invocation/vendor handling. Standalone Nix effect builds currently have no Rust toolchain/vendor inputs; standalone settings builds currently need neither KWin nor Rust. KWin-matched rebuilds already apply to the C++ effect. Unverified: added build time, package size, new dependency closure/MSRV or exact packaging edits for any future effect staticlib; these depend on code/dependencies not selected here.
- Verified stale-record distinction: CMake introductory comment still mentions a Cargo staticlib (`CMakeLists.txt:8-18`), recipes/comments still mention two shipping Rust roots, and licensing research still inventories the removed effect crate (`docs/research/licensing.md:39-66`). Those are historical text, not proof current effect links Rust. The actual current target_link_libraries and current shipping notice root resolve that distinction (D and below).
- Verified source-license decision: project default/all Rust crates/shared FFI/config are MIT; GPL-2.0-or-later effect-source exception is limited to sources including KWin headers or reusing KWin code, with the 15 audited KWin-free logic/test sources MIT. Combined effect binary remains GPL-covered ([Licensing decision](../decisions/development.md#licensing); [completed bounded audit](archive/native-effect-license-audit.md)). The older four-file exception list in the dated 2026-10-11 licensing research is historical and superseded, not the current boundary.
- Verified compatibility wording: "MIT suits any GPL version; Apache-2.0 suits GPLv3 only, not GPLv2" (`docs/research/licensing.md:366-369`, primary [ASF compatibility](https://www.apache.org/licenses/GPL-compatibility.html), [GNU license list](https://www.gnu.org/licenses/license-list.html)). GPL-2.0-or-later allows a GPLv3 election; GPLv2-only does not. The historical dependency inventory offers MIT/Apache alternatives and no required copyleft (`:59-66`), but it is not a new audit of a future staticlib's dependencies.
- Verified notices mechanics: scripts/third-party-notices.sh:18-30,42-52,64-74,102-134 currently renders from crates/omnitiler/Cargo.toml and checks the normal-dependency closure of SHIPPING_ROOTS="omnitiler" for two Linux targets; about.toml:5-23 prefers harvested MIT text and excludes dev/build dependencies. Supplemental upstream COPYRIGHT/NOTICE files are appended (`script:137-193`). It does not currently enumerate an effect staticlib root. The historical effect was included before removal (21af12a2 changed script/about/template/notices); arbitrary new dependencies are not automatically covered by the current omnitiler-only root.
- Verified package notice delivery: effect and core carry generated THIRD-PARTY-NOTICES plus relevant first-party licenses (`PKGBUILD:93-97,126-133`; debian/rules:97-110; rpm spec:219-237; flake.nix:133-141,323-326). Vendored crate license files do not substitute for binary notice delivery (`docs/changes/archive/third-party-notices.md:49-74`). Distro transitive native-source/attribution closure remains a recorded follow-up (`docs/research/licensing.md:523-527`); historical 93-package/five-workspace-crate counts are not current metadata results.

### F. Native testing surface and migration evidence limit

- Verified line counts by primary area (each test file counted once): host-free facts/consumer/lifecycle = 3 files / 1,347 lines; D-Bus facts and Stage5 handler/recovery = 4 / 2,858; settings/KCM/shortcut reconciliation = 5 / 12,742; total 12 / 16,947. Individual files and host bindings are in A. These are source lines, not measured maintenance cost or independent test-case counts.
- Verified CTest definition count: 36 literal add_test definitions in CMakeLists.txt:167-572: seven effect logic/consumer/stage5/facts/gesture cases; four KCM config cases; seven KCM shortcut cases; four script config cases; one settings-descriptors case; one script discovery validation; ten reconciler cases; metadata/factory and unified-lifecycle validation. Conditions select effect/settings subsets. The flake's "35-test suite" comment is stale relative to these 36 definitions (`flake.nix:166`); no configured/executed test total was measured here.
- Verified host-free entry tests: native-effect-logic, native-effect-presentation-consumer, native-effect-gesture-lifecycle directly compile the corresponding C++ header logic and link Qt, not KWin (`CMakeLists.txt:160-184,280-294`). Stage5 suites additionally compile production handler/facts code and inject stub views/recorded fixtures; transport suites use private D-Bus sessions (`:186-278,296-310`). Settings/reconciler tests require Qt/KF widgets/config and isolated file/bus seams (`:312-539`). Metadata/discovery/lifecycle checks use CMake scripts rather than C++ test methods (`:493-500,552-581`).
- Verified execution wiring: Nix tests invoke ctest --output-on-failure with QT_QPA_PLATFORM=offscreen and isolated config; CI builds both native checks (D). Rust workspace tests/unsafe guardrail already have CI execution. No sanitizer test leg was found; the principle records it as not yet implemented.
- Unverified hypothetical changes: no migration patch exists, so changed Rust/C++ test counts, FFI parity coverage, sanitizer results and retained native integration coverage cannot be stated as observed outcomes. Current pure suites call C++/Qt interfaces; current D-Bus/KCM/factory tests also exercise host boundaries, not just reducer semantics. Historical AR10 separately demonstrates Rust policy/parser tests plus C++ ABI/consumer tests, but not acceptance of a new extraction (`architecture-review-ar10-cargo-effect.md:22-27`).

### G. Prior art and remaining unknowns

- Verified internal prior art: this project's own effect linked Rust before 21af12a2, with C ABI headers, staticlib Cargo/CMake/Nix glue and recorded offline tests (B/C). This does not establish a future pure-Rust factory or current runtime acceptance.
- Unverified external prior art: no independently verified KWin effect/plugin or KDE/Qt compositor plugin partly implemented in Rust, with primary source plus build configuration, was established in the GitHub/KDE/KWin Rust-plugin searches. Rust/Qt application demos and the existence of CXX-Qt are not classified as compositor-plugin evidence. No claim of ecosystem-wide absence follows.
- Unverified remaining items: deployed co-resident Rust plugins and dynamic symbol collisions; every-backend callback/teardown affinity; pure-Rust KWin factory feasibility; future bridge dependency/MSRV/allocator/export surface; measured build/package/runtime costs; current distro-native closure audits; new extraction's native and sanitizer acceptance. Existing source-derived ABI/panic/owner-context constraints and historical passing builds remain distinct from these unknowns.

### Backlog detail moved 2026-11-01: core per-report cost (original lines 531-537)

*Verbatim open-work detail retained from the 2026-11-01 backlog trim.*

- P1 | Core per-report cost | Steady-state `handle_report` re-runs the full
  reconciliation sweep on unchanged input and grows superlinearly with windows
  and occupied workspaces (200 windows/1 output about 27 ms; 50 windows with
  20 workspaces/output about 10.6 ms). Profile the sweep, remove the
  superlinear term, skip unchanged domains, then add a per-report cost
  regression gate ([F20](core-interface-review.md)). Cause unknown;
  needs a rebuilt scale harness and a sampling profiler.

## Backlog detail moved 2026-11-01

### Original backlog lines 95-102

````markdown
- P1 | Command outcome in the reply (v4) | Approved 2026-10-30
  ([decision](./decisions/contract.md#command-outcome-in-reply)): rejection becomes a
  reply field and core always incorporates the observation; delete the
  Windows (`world_tick.rs` catch-up), KDE (`world_broker.rs`
  `dispatch_plain_fallback`) and macOS rejection paths. Fixes the macOS bug
  where rejection drops taken attempts and skips catch-up
  ([F18](changes/core-interface-review.md)) at the macOS v4 cutover; not fixed
  separately before then.
````

### Original backlog lines 103-122

````markdown
- P1 | Handshake, settings and report split (v4) | Approved 2026-10-30
  ([decision](./decisions/contract.md#host-capabilities-and-settings)): version and
  capabilities once per connection (a change reconnects); revisioned settings
  sent at start and on change; the report keeps envelope, facts, command,
  attempt feedback and renderer input. Removes per-report settings digest and
  validation ([F1](changes/core-interface-review.md)). Same bump: one optional
  primary output id replaces per-screen `primary`
  ([decision](./decisions/contract.md#primary-output)); check whether the KWin script
  API exposes Plasma output priority. Same bump (F23): the handshake defines
  one adapter lifetime, core mints its generation and reports carry
  connection-scoped sequences, replacing per-adapter owner/generation/
  incarnation minting; each new lifetime starts fresh core state (current
  production behavior); remove core's retained-session incarnation path after
  confirming no production caller. Same bump (F24, Resilience): core
  validates each settings field (invalid keeps last-good or default, typed
  per-field diagnostic; adapters pass raw values, macOS no longer refuses to
  start, Windows no longer drops a whole document, KDE no longer defaults
  silently); closed enums for report, command and cause kinds (adapters stop
  matching strings); a bad row holds its scope as unreadable instead of
  rejecting the whole report.
````

### Original backlog lines 132-158

````markdown
- P2 | Adapter decisions behind the contract (R5) | Follow-through of the
  approved [Contract](./decisions/contract.md#state-centric-contract) entries, no behavior change:
  W `float-no-remap` veto becomes a declared capability (drops the second
  engine clone, F14); W float/sticky/orientation toggle refusals, M/K
  membership classification, M zoom-as-maximized inference, W border
  eligibility and W birth/restart adoption move into core; W/M direct engine
  queries become reply fields. Check whether the K re-actuation veto
  (`world-apply.ts:834-859`) reports as a safety refusal fact. Behavior change
  (user 2026-10-30): focused-fullscreen presentation suspension is
  unconditional; delete the KDE opt-in and the Windows `paused` lane, and
  have Windows report unmanaged fullscreen foregrounds
  ([decision](./decisions/world-rendering.md#kde-renderer-freshness)).
  Behavior change (user 2026-10-30): KDE automatic-focus guards
  (`world-apply.ts:2294-2377`) become core rules for every platform
  ([decision](./decisions/focus.md#automatic-focus-guards)); first check Windows
  maximize reporting and whether the KDE overlay guard holds explicit focus
  to floats. Behavior change (user 2026-10-30): automatic fixed-size float
  sends on every platform, `fixed_send_admission` removed
  ([decision](./decisions/workspaces.md#workspace-lifecycle-focus-and-history)); first check whether KDE/macOS move
  transient/modal dialogs with their parent natively. `maximized_focus_fence`
  removed with no behavior change: Windows reports maximize as a fact and the
  core G-06 fence replaces its adapter gate (`tiling_sys/world_sequencer.rs:86`).
  Audit (user 2026-10-31, G4): any other adapter-side decision outside the
  bounded host-synchronous list (mechanical safety vetoes, same-turn
  application of core decisions) gets its own dated entry and joins this item
  ([decision](./decisions/engine.md#core-owned-world-and-platform-boundary)).
  [Evidence](changes/core-interface-review.md) (F10, F12, F14, G4).
````

### Original backlog lines 159-186

````markdown
- P2 | Typed contract vocabulary (R6, v4) | No behavior change: typed
  `Direction` on all directional commands; drop the ignored `Focus.direction`
  payload; typed effect cause replaces `fx-`/`af-` id prefixes; promote live
  `Operation(CoreCommand)` uses (toggle-float, toggle-orientation, resize)
  to world commands and remove `Operation`; delete unused `SwitchView`
  (no production caller); rename `cloaked` vocabulary to `HostHidden`
  and `FrameSetter` to `SetterOutcome`; fix stale `ToggleMaximize`,
  `Operation` and `world_owner.rs` docs; state rules instead of "platform X
  parity" in core docs. [Evidence](changes/core-interface-review.md) (F2,
  F4, F11, F13). Behavior change (user 2026-10-30): macOS keyboard
  focus/move switch to `FocusDirectional`/`MoveDirectional`, gaining
  cross-output edge focus/move
  ([decision](./decisions/windows.md#windows-decision-set)). Behavior change
  (user 2026-10-30): keyboard sends of unhomed windows use the action-context
  chain; `HomeThenCachedLive` is removed
  ([decision](./decisions/contract.md#send-source-fallback)). Identifiers (F22, no
  behavior change): distinct `NativeWorkspaceId` type (macOS stops wrapping
  Space keys in `WorkspaceId` and drops the native-text logical fallback);
  typed command sequence instead of `op-` suffix parsing in `world_clear.rs`
  (parse failure defaults to 0 today); typed authority scope instead of KDE
  parsing `output/workspace` text; guide rule that window ids are never
  reused within an incarnation, after checking macOS CGWindowID reuse.
  Guide rule (F27): adapter-minted `NativeWorkspaceId` values are allowed and
  must stay stable across host reindexing within a lifetime. Attempts (F30):
  `MemberAttempt`/`ViewAttempt` carry one pre/post/baseline set typed as a
  logical-or-native workspace reference instead of parallel logical and
  native sets; check whether native retirement desires are capability-gated
  for adapters with no removal consumer (macOS).
````

### Original backlog lines 187-193

````markdown
- P2 | Coordinate quantization consistency (F21) | One fractional-to-integer
  rule per adapter: KDE native gesture capture (`std::lround`) matches
  ordinary facts (`Math.round`); macOS AX window and drag-start frames round
  like topology instead of truncating. Per-screen gap fields leave the report
  with the settings split. Exact arrival stays; check settle under KDE
  fractional scaling and fractional macOS AX frames before proposing any
  tolerance. [Evidence](changes/core-interface-review.md) (F21, package H).
````

### Original backlog lines 205-212

````markdown
- P2 | Output identity (R11, v4) | Behavior change (user 2026-10-31,
  [decision](./decisions/contract.md#output-identity-r11)): screen facts carry display
  manufacturer, model, serial and connector; core derives output identity
  and matches displaced-workspace return on it, connector deciding when the
  display identity is unreadable or shared. Before build: check KWin script
  exposure of the output `manufacturer`/`model`/`serialNumber` properties,
  Windows and macOS display-identity sources, and two identical monitors
  without serials. [Evidence](changes/core-interface-review.md) (F22).
````

### Original backlog lines 213-220

````markdown
- P2 | Crash-safe native writes (R12) | Behavior change (user 2026-10-31,
  [decision](./decisions/contract.md#native-writes-difficult-to-undo-r12)): record
  the original state in a host slot before any hard-to-undo native write;
  the next start restores leftovers. Windows: the existing watcher restores
  stripped fullscreen chrome as it restores hides. KDE: project-owned
  keep-above and sticky stacking preimages move into the session runtime
  file. Gates per-window suppression in the visibility-ready bump on each
  platform. [Evidence](changes/core-interface-review.md) (F23).
````

### Original backlog lines 221-232

````markdown
- P2 | Window-state commands (R14, v4) | Behavior change (user 2026-10-31,
  [decision](./decisions/contract.md#window-state-commands-r14)): fullscreen and sticky
  on/off/toggle world commands; Windows maximize via `ToggleMaximize`; core
  owns refusal (`window_policy.rs`) and sticky-off; delete KDE/Windows
  adapter-direct toggles and local re-checks (Windows sticky stops calling
  core float/send underneath); fullscreen fact gains ownership and
  exitability. macOS 0.1 (user 2026-10-31): maximize and fullscreen product
  actions through the commands (fullscreen Space handling with gate MX5);
  sticky declared unsupported until an all-Spaces route is proven (gate check
  32). Sticky-off keeps the existing shared KDE/Windows rule. Alt+Enter stays
  unbound (user 2026-10-31); `Meta+F11` / `Win+F11` drive the command.
  [Evidence](changes/core-interface-review.md) (F25).
````

### Original backlog lines 233-241

````markdown
- P2 | Scripting command surface (R18) | Public surface (user 2026-10-31,
  [decision](./decisions/contract.md#scripting-command-surface-r18)): experimental in 0.1 on
  KDE, Windows and macOS; verbs are the `WorldCommand` wire names and every
  command goes through core. Windows renames its CLI verbs (including the
  test-control workspace forms) and drops adapter-direct paths; macOS adds a
  same-user channel to the running tiler; KDE keeps `QueueWorldCommand`.
  User-facing docs list the verbs as experimental. Sequence after
  window-state commands (R14) so those verbs exist.
  [Evidence](changes/core-interface-review.md) (F29, package P).
````

### Original backlog lines 242-253

````markdown
- P2 | Contract diagnostics consistency (F26) | No product behavior change:
  core defines event names and fields for contract events (report rejected,
  command refused, row held, settings field invalid, settle timeout);
  adapters log core enums verbatim. Per-report/per-observation lines on KDE
  Rust and macOS move to verbose/trace per the Observability decision; KDE
  summaries come from decoded values, not a second JSON parse; one
  `normal`/`verbose`/`trace` level vocabulary on every adapter. Check where
  macOS stderr goes when bundled and whether the Windows run log is bounded.
  Privacy scope (user 2026-10-31, R15, [decision](./decisions/observability.md#cross-component-diagnostics)):
  app identity and process ids allowed at normal level; audit log sites so
  titles, content, full paths and native window handles/ids stay redacted.
  [Evidence](changes/core-interface-review.md) (F26, package M).
````

### Original backlog lines 254-260

````markdown
- P2 | Contract conformance gates (F28) | Testing only: CI regenerates
  `test-fixtures/world-wire/` with the omnitiler example and fails on a diff.
  At v4: TS contract types generated from Rust; the generator emits one
  sample per report/command/effect/outcome variant for the TS parser; core
  behavior tests sit at the `WorldEngine::handle_report` boundary and name
  the reference row id. [Evidence](changes/core-interface-review.md) (F28,
  package O).
````

### Original backlog lines 261-267

````markdown
- P3 | Control endpoint hardening (F29) | Same-user trust model unchanged:
  KDE effect `SetPresentation`/`ClearPresentation`/`HidePresentation` and
  `LastGesture` accept only the sender their sibling methods accept (confirm
  the real caller first); Windows command-file receivers cap bytes and reject
  unknown fields. The new macOS scripting channel (R18) checks same-user
  sender, caps bytes and parses strictly from the start.
  [Evidence](changes/core-interface-review.md) (F29, package P).
````

### Original backlog lines 268-276

````markdown
- P2 | Anchored border (R20, v4) | Behavior change (user 2026-10-31,
  [decision](./decisions/contract.md#anchored-border-r20)): the border reply becomes an
  anchored descriptor (subject lifetime, outsets/width/radius, eligibility,
  causal witness) applied to the live frame from existing native events;
  KDE effect stops motion hides, Windows and macOS border runtimes stop
  replaying absolute bounds; underlay stays absolute. The offline
  moving-border comparison becomes the regression test; physical check per
  platform, including a macOS drag.
  [Evidence](changes/core-interface-review.md) (F31, package R).
````

### Original backlog lines 277-298

````markdown
- P2 | Rust in the KDE native effect (re-evaluation) | Evidence first,
  proposal to the user (user 2026-10-31,
  [principle](principles.md#rust-and-memory-safety),
  [reopened route B](./decisions/engine.md#core-owned-world-and-platform-boundary)):
  classify the effect's ~13.9k source lines (plus ~16.9k test lines) into
  host-required shim versus logic (parsing, state machines, reconciliation,
  settings adaptation); propose a Rust boundary (prior shipped POD C ABI with
  panic containment, or alternatives) and its migration order; re-measure the
  earlier build, packaging and distribution costs (CMake-driven Cargo, Nix
  vendoring, offline vendored Ubuntu effect build, stock-distro Rust floors,
  third-party notices) against the C++ complexity removed. Pre- or post-0.1
  timing is the user's call on the evidence. Effect C++ facts measured
  2026-10-31; no Rust is linked into the effect since `21af12a2`.
  User 2026-10-31: run it as a "KDE effect Rust viability / tentative plan"
  review once the open review decisions are cleared: is it sensible, and
  which issues and hurdles arise in implementation, building, packaging and
  distribution. Review done 2026-10-31: viable; hurdles and candidate
  four-step plan in [F32 and R21](changes/core-interface-review.md). User
  2026-10-31 (option (a), [decision](./decisions/engine.md#core-owned-world-and-platform-boundary)):
  steps 0-2 before 0.1 (step 2 with the v4 anchored border), step 3
  (shortcut reconciler logic) after 0.1, or before it if rearchitecture and
  cleanup are finished before the first release.
````

### Original backlog lines 320-325

````markdown
- P3 | Settling marker closure (F31) | Docs only: the adapter guide's
  coordinates-and-safety rule allows proposing a neutral settling
  representation only with a reproduced failure and a host completion signal
  the adapter can read; KWin scripting/effects and macOS AX expose none today.
  Host discrepancies stay with "Shared: tolerate host-settled geometry".
  [Evidence](changes/core-interface-review.md) (F31, package R).
````
