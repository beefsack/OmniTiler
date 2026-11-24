# V4 phase 3: macOS port

## Purpose, status and gates

- Entry-point plan for porting `tiler-macos` to the approved v4 boundary, in
  usable increments modelled on KDE's [dogfoodable slices](archive/v4-phase2-kde-port.md#later-dogfoodable-slices).
  The linked contracts, decisions and native records own the detail; this plan
  neither replaces them nor reopens approved behavior.
- KDE gate evidence from the upstream acceptance record: under the [migration decision](../decisions/contract.md#contract-authority-and-v4-migration),
  KDE phase 2 is complete offline, including remaining v3 removal, and the
  [user-run physical ST acceptance passed 2026-11-10](v4-phase2-kde-state-targets.md#2026-11-10-physical-state-target-acceptance)
  apart from desktop/stacking and compositor-boundary (KWin restart) recovery.
  The user deferred those sub-legs until after macOS start; they remain open KDE
  acceptance, not start gates. ST is recorded in the
  [single acceptance checklist](kde-v4-acceptance-checklist.md#additional-kde-acceptance-and-diagnostic-legs).
  The KDE reconnect (RC) leg-2 fix and merge decision
  must land before this port reaches its reconnect/rehome slice and before 0.1. Other KDE acceptance remains separate, not implicitly
  cleared by opening this port.
- **Started under the amended gate (2026-11-10); slice 1 implementation in progress.**
  Under the [migration decision](../decisions/contract.md#contract-authority-and-v4-migration),
  KDE phase 2 offline completion including v3 removal is done. macOS may start
  while KDE ST/RC work continues. Neither slice 5 (state targets) nor slice 7
  (physical reconnect/rehome) acceptance is claimed until both user-run KDE
  ST/RC checks pass and resulting core changes are absorbed; see the
  [single acceptance checklist](kde-v4-acceptance-checklist.md#additional-kde-acceptance-and-diagnostic-legs).
  The RC leg-2 fix and merge decision remain pending before acceptance and 0.1.
  Other KDE acceptance is separate, not implicitly cleared by opening this port.
- The [pre-port review outcomes](#pre-port-review-outcomes-2026-11-10) are decided;
  their implementation/native proof remains below. Shared
  [host-settled geometry design](../decisions/layout-and-admission.md#host-settled-geometry-design-timing)
  is deferred until after slice 1 and brought for user approval before slice 3;
  approval remains required before any core change.
- The [2026-11-24 open-decision review](macos-v4-open-decisions-review.md) is
  user-approved: D0–D6 are decided, R1–R5 rectifications are tracked there, and
  the bounded D5 fixture diagnostic ran: close identity matched exactly, but
  one window hide emitted a false destroyed event for a still-alive window.
  The second bounded diagnostic (2026-11-24) captured event-time AX/CG state:
  that hide delivery was AX-absent while CG-present/onscreen, identical to the
  close delivery. No safe close-only predicate exists; the CG-alive oracle
  stays authoritative and transport remains unimplemented.
  Decisions are canonical in [macOS v4 open decisions](../decisions/macos.md#macos-v4-open-decisions-2026-11-24).
  Follow-up code review findings (**F1–F7**) and tentative decisions (**TD1–TD7**,
  pending later user confirmation) are recorded in
  [Part 5 of the review](macos-v4-open-decisions-review.md#part-5-follow-up-code-review-and-tentative-decisions-2026-11-24).
- Port/build/live work runs on a macOS host through the [development environment](../macos-dev-environment.md)
  and [live-testing guide](../live-macos-testing.md). Use the signed
  [fixture bundle and exact scope](../live-macos-testing.md#fixture-first-live-tests),
  not direct executable launch. Physical/input/visual acceptance is not inferred
  from dispatch receipts or offline gates.
- KDE and macOS use report scale 1. The shared per-output resize-minimum,
  keyboard-step and group-edge scaling prerequisite gates Windows, not macOS;
  [port ordering and any later overlap](../decisions/contract.md#contract-authority-and-v4-migration)
  remain unchanged. Native/release scope stays in the [macOS 0.1 gate](../decisions/macos.md#macos-0-1-scope-and-gate)
  and [remaining port acceptance](macos-world-v2-port.md#original-backlog-lines-1767-1959).

## Current state and rebase onto the current core (2026-11-23)

This section is the starting point; it supersedes the status lines above and
the slice-1 evidence below for planning.

- **Stale evidence.** The [slice 1 evidence](#slice-1-implementation-and-evidence)
  and the [suppressed admission focus blocker](#blocker-suppressed-admission-focus-changes-the-next-implicit-subject)
  predate the core changes in the table below. Treat both as unverified. Core
  has since moved to one retained focus settlement owner and removed reply
  `effects`, so the blocker may be resolved or changed; re-run its minimal
  case against the restored build before acting on it.
- **Build restored (2026-11-23).** The port below removes the drift: `tiler-macos`
  builds with `cargo check`, builds `--all-targets`, formats, passes strict
  clippy and the library tests (1601/0/3). The mock `tests/integration` target
  now reports 149 passed / 1 failed, one documented core-policy stop; see
  [integration triage](#integration-triage-2026-11-23). No core or protocol
  file changed.
- **Historical build state.** At `dfb70175`, a Linux `cargo check --offline --locked -p tiler-macos`
  failed with 77 errors, all core/protocol API drift; `--all-targets` added 7
  from the `tiler-windows` dev-dependency (`HostCaps`/`FieldDef.capabilities`,
  `59a4e77b`). Code under `cfg(target_os = "macos")` added an unknown number on
  a macOS host. The pre-port review's 80-error macOS build predates these changes.
- **No broker.** `tiler-macos` depends on `tiler-core` and `tiler-protocol`
  only, so broker-side protocol work (PW-BR-*), the KDE renderer read
  (PW-OUT-04) and the shared-roster slot route (PW-ROSTER-01; macOS is to
  declare `sharedNativeRoster: false`) need no macOS change.
- **KDE reference.** Most changes below landed with their KDE producer and
  consumer in the same commit; `git show <commit> -- kwin/` is the worked
  example. `877409e8` and `01eba77d` are core/protocol-only (KDE talks to the
  broker over the wire); use the Rust call sites in that commit instead.

| Change | Commit | Compile symptom in `tiler-macos` | Kind | Backlog item |
| --- | --- | --- | --- | --- |
| Retained drag preview split from command outcomes | `9a4c2125` | `WorldReplyV4.command_result`, `CommandResult::DragPreview` (`drag_fixture.rs`, `world_runtime.rs`) | Mechanical: read `reply.drag_preview` | macOS: adopt retained `dragPreview` and command `result` removal |
| Typed command IDs and subjects | `877409e8` | `WorldCommand::Implicit`, `WindowSubject`/`WorkspaceId`/`FocusDirectionalSubject` mismatches, `.0` on `WindowSubject` (`world_command_map.rs`, `workspace_send.rs`) | Mechanical | C5 typed boundary adoption on macOS and Windows |
| One validated prepared transition per report | `dbdb2dd6` | `.0` on `TransitionResult` (`world_dispatch.rs`, `workspace_select.rs`, `drag_fixture.rs`) | Mechanical | macOS: verify single report ingress and flag removal |
| Unified v4 property attempts | `cb6a31e9` | `WindowFact.last_write`, `ReportWireOptionals.*_attempts_present`, `DecodedReportV4.property_attempt_order` (`world_builder.rs`, `runtime_core.rs`, `world_dispatch.rs`) | Producer rewrite: emit one `propertyAttempts` section | C5 typed boundary adoption on macOS and Windows |
| Held presentation evidence at wire ingress | `01eba77d` | `DecodedReportV4.residual` | Mechanical | none |
| Wire cleanup (unreachable decode, writer checks) | `bdefeef6` | `ReplyWireOptionals`, `DecodedReplyV4.wire` (`world_dispatch.rs`) | Mechanical: derive from the typed reply | none |
| Reply `effects` removed (PW-OUT-03) | `fa60a5f3` | `WorldReplyV4.effects` (`world_sequencer.rs`) | Mechanical | macOS: re-verify unified retained focus after core reconciliation |
| Settings catalog host exposure flags removed | `59a4e77b` | `HostCaps`, `FieldDef.capabilities` (`tiler-windows` dev-dependency) | Mechanical | none |
| Automatic maximize clear via `state.maximized` (PW-EV-02) | `d253fe52` | `ClearAttempt`, `WorldReportV4.clear_attempts`, `WorldEngine::new_test_bound_with_capabilities` (`world_builder.rs`, `world_dispatch.rs`, `drag_fixture.rs`) | Delete the clear lane; macOS reports native `maximized: null` (M4), so no state-lane attempt replaces it | macOS maximize (M4) |
| Tagged attempt dispositions and frame purposes (PW-EV-04) | `92505653` | `VisibilitySetter`, `FocusFeedback.setter`/`.reason`, `FrameAttempt.setter`/`.prewrite`/`.operation_id`, `MemberAttempt`/`ViewAttempt.setter` (`runtime_native.rs`, `runtime_thread.rs`, `runtime_worker.rs`, `world_dispatch.rs`, `world_runtime.rs`, `diag_cause.rs`) | Needs native mapping: each AX/Space call result to the tagged disposition, each frame write to its purpose, per the [contract](../porting/world-contract.md#raw-attempts-and-settlement); record the mapping | macOS: adopt retained settlement outcomes |
| Explicit frame attempt identity, nullable `attemptMs` (PW-EV-06) | `fc6e3935` | `Option<u64>` mismatch (`world_runtime.rs`) | Mechanical, but never send a zero or invented clock | none |

Changes that compile but still need macOS behaviour, each with its own
backlog item: declare `sharedNativeRoster`; unified retained focus; pending-focus
command subjects; retained settlement outcomes; retained `intentDesired`
(needs a host storage decision, M3); switch and fallback removals; no-space
removal; perpendicular focus recency; batch 3 admission; best-effort
admission; settings-key parity and the 16-output `actionOutputs` cap
(PW-IN-03, PW-IN-06; `fa60a5f3`).

### Starter brief

1. **Restore the build.** Mechanical edits only, following the table and the
   KDE side of each commit. Record a table of error group, change, commit and
   files touched. Do not add aliases, shims or compatibility conversions, and
   do not weaken or delete assertions to make tests compile; port the test to
   the current shape or list it as blocked. The macOS test and clippy gates
   build the `tiler-windows` dev-dependency, so repair its 7 catalog errors
   the same mechanical way (`git show 59a4e77b -- crates/tiler-kde-settings-exporter`).
2. **Offline gates.** Run the [allowlisted macOS gates](../macos-dev-environment.md#4-prove-the-portable-offline-cargo-baseline-allowlist-only)
   and report pass/fail counts per suite. Classify each failure as stale
   expectation, adoption gap (name the backlog item) or defect.
3. **Native smoke.** Under the [live guide](../live-macos-testing.md), the
   signed fixture bundle with exact scope on ordinary admission/reflow only
   (slice 1's native floor). Then re-run the suppressed admission focus
   minimal case.

Stop and write up instead of implementing when a change needs a product or
contract decision, any `tiler-core` or `tiler-protocol` change, the shared
host-settled geometry design, or M3 storage. Record findings in this section
and update the matching backlog items.

### Rebase implementation: build restore (2026-11-23)

Mechanical-only port of `tiler-macos` (and the `tiler-windows` settings-catalog
compile unit in its dev-dependency path) to the current core/protocol shapes.
No core, protocol, contract or product-semantics change; no aliases, shims or
compatibility conversions. Every assertion was ported to the current shape
except where the old mechanism was deleted by core (listed below).

| Error group | Core change (commit) | macOS port | Files |
| --- | --- | --- | --- |
| Typed command IDs and subjects | `877409e8` | Direct typed `FocusDirectional`/`MoveDirectional` commands with `WindowSubject::CurrentFocus`/`FocusDirectionalSubject::CurrentFocus` replacing the `Implicit` wrapper; explicit subjects wrapped in `WindowSubject::Window`; logical ids wrapped in `WorkspaceId`; subject-aware log rendering (no `.0`, no `Display`) | `world_command_map.rs`, `workspace_send.rs` |
| Retained drag preview split | `9a4c2125` | Read `reply.drag_preview` (`DragPreviewV4`); preview outcomes no longer come from `CommandResult` | `world_runtime.rs`, `drag_fixture.rs` |
| One validated prepared transition | `dbdb2dd6` | `TransitionResult.outcome` instead of tuple `.0` | `workspace_select.rs`, `drag_fixture.rs`, `world_dispatch.rs` |
| Unified v4 property attempts | `cb6a31e9` | `WindowFact.last_write` removed (macOS never produced one); report gains `visibility_attempts: Vec::new()` because macOS owns no visibility write lane, so no truthful row exists | `world_builder.rs`, `runtime_core.rs` |
| Held presentation evidence at ingress | `01eba77d` | `DecodedReportV4` carries `residual` (canonical `None` for adapter-built reports) and `property_attempt_order` | `world_dispatch.rs` |
| Wire cleanup | `bdefeef6` | `ReplyWireOptionals`/`DecodedReplyV4.wire` removed; `ReportWireOptionals` keeps only cursor-hit, monotonic-clock, creation-receipt and gesture bits (attempt presence derives from the typed rows) | `world_dispatch.rs` |
| Reply `effects` removed (PW-OUT-03) | `fa60a5f3` | Deleted the vestigial one-shot-effect lane (`spent_effects`, `spend_effect`, `take_unspent_effects`, `focus_effect`); the adapter already ignored the spent list. Focus rides `reply.focus_request` into the retained focus queue. The single test of the deleted mechanism is removed with that reason; the retained-request identity it indirectly covered has its own sequencer tests | `world_sequencer.rs`, `world_dispatch.rs` |
| Settings catalog host flags | `59a4e77b` | `HostCaps` import, `capabilities` initializers, the local caps constant, its re-export and the drift assertion removed (same treatment as the KDE exporter) | `tiler-windows/settings/windows_catalog.rs`, `tiler-windows/settings/mod.rs` |
| Automatic maximize clear (PW-EV-02) | `d253fe52` | Clear lane deleted; macOS reports native `maximized: null` (M4) and what remains of the clear path now rides `state.maximized` core-side, so the adapter produces no clear attempt | `world_builder.rs`, `world_runtime.rs` |
| Tagged dispositions and frame purposes (PW-EV-04) | `92505653` | `FocusFeedback.disposition`, `ViewAttempt.disposition`, `MemberAttempt.disposition`, `FrameAttempt.disposition`/`purpose`; mapping below | `runtime_native.rs`, `runtime_thread.rs`, `runtime_worker.rs`, `diag_cause.rs`, `world_dispatch.rs`, `world_runtime.rs` |
| Explicit frame attempt identity (PW-EV-06) | `fc6e3935` | Producer-generation frame attempt ids allocated in the sequencer and retained on resend; `attemptMs` is the measured completion stamp or `None`, never an invented zero | `world_runtime.rs`, `world_sequencer.rs` |
| Fixture engine binding | `d253fe52` fixture constructor removal | `drag_fixture` binds through the genuine production route (`WorldEngine::new` + `world_connection::connect`, macOS capabilities handshake and revision-1 settings) instead of the test-only constructor | `drag_fixture.rs` |

Attempt and frame mapping applied (contract
[raw attempts](../porting/world-contract.md#raw-attempts-and-settlement)):

| macOS native outcome | v4 evidence |
| --- | --- |
| AX completion `Applied` | `called { setter: accepted }`; focus/visibility row reason `""` |
| AX completion `Failed` | `called { setter: refused }`; reason `native-refused` |
| AX completion `PartialApplied`/`Timeout` | `called { setter: threw }`; reason `native-refused` |
| Gate/fence refused before any AX call | focus `not-called { focus-* }` / visibility `not-called { reason }`; old empty reason → `focus-no-reason`; old `focus-unproven` → `focus-foreground-unproven` |
| Membership consumed with readback already at target | `already-at-target` (membership only) |
| Ordinary layout frame write | `purpose: layout` |
| Explicit float-placement frame write | `purpose: float-placement { op, pre }`, `pre` only when a read exists |
| Frame stamp unavailable / not a native call | `attemptMs: null` for frames; a view/frame/state write with no native call stages no row |

Superseded behavior note: the integration case
`admission_never_steals_focus_but_explicit_focus_dispatches` asserted the old
admission-suppression policy. The approved
[retained-focus resolution](#blocker-suppressed-admission-focus-changes-the-next-implicit-subject)
replaces it, so that case is ported to the current requirement (one retained
focus request per cause, admission included) rather than kept as a suppression
assertion.

### Failure classification after the build restore (2026-11-23)

All 16 failures from the first green build are now classified. No core or
protocol file was edited.

| Case | Class | Evidence and resolution |
| --- | --- | --- |
| `world_builder::tests::direct_target_focus_at_hidden_minimized_refuses_without_effect`, `world_builder::tests::first_transition_same_report_focus_never_restores_hidden` | Port defect (test input) | `WorldCommand::Focus { direction: Some(..) }` is refused by `valid_command_v4`; direct-target focus is directionless in v4. Direction dropped. |
| `world_runtime_tests::staged_native_attempts_report_absence_not_translated_logical` | Port defect (test input) | `WorldCommand::SwitchView` is not a v4 command; the case is `SelectWorkspace`. |
| `runtime_thread::trailing_join_tests::selection_focus_null_and_newer_epoch_cancel_without_setter` | Stale premise | Approved same-report precedence publishes the away selection intent's explicit request (`focus-2`, `ExplicitCommand`, `cmd_seq Some(1)`, target `4243_9`, op-1 `after_shown`) instead of cancelling the deferred intent for the admission one. The deferred intent stays bound (`selection_focus_pending` op-1/focus-2), the drop counter is unchanged, close epoch is `focus-2`, zero dispatches and zero queue rows. A later publication that reports the old target focused but carries no proven foreground leg (`focus_evidence`/`sys_focus`/`global_focus` unset) keeps the retained request; observed equality alone no longer retires the lane. Ported to those outcomes, no product change. |
| `runtime_thread::drag_gesture_handoff_tests` ×3 | Two adapter defects, one stale premise | (1) `world_sequencer::accept_reply` refused a correlated reply whose terminal changes no state (cancel, orphan cancel, late drain) because `state_revision` did not advance, losing the requester's terminal; the monotonic guard now exempts replies that carry a gesture terminal. (2) A repeated stable gesture id reached core a second time and reported `committed: true` again; `handle_drag_gesture` now claims each terminal id through `Sequencer::claim_gesture_terminal` (bounded memory, cleared per generation) so a replay never dispatches, matching the documented at-most-once-by-stable-id rule. (3) The late old press case is fenced pre-dispatch by press-revision drift (same rule as every stale press), so it mints no terminal at all; ported to that outcome. Terminals are additionally filtered to the gesture id carried by the report, so a late drain of an older id never surfaces on a newer ingress. |
| `dock_swipe_select_tests` ×9 | Host-dependent test premise (not a mapping defect) | The `Refused`/`Threw` mapping is correct; the cases never reached posting because `OwnerSelectBackend::select` read the running host major (`crate::ffi::macos_major()`), and newer host majors refuse the pinned yabai payload by design (`space_switch_edges.rs`, own test). The backend now carries the payload major and offline tests pin the newest supported major (26) through `test_force_os_major`, so the scripted seams exercise the payload itself. `Refused` and `Threw` keep their distinct meanings in every assertion. |

### Native leg attempts (2026-11-23)

Fixture bundle: `just fixture-app <development identity>` → `fixture-verify`
OK; bundle `io.github.omnitiler.live-fixture`, minos 15.0, dev-signed
(`spctl` rejects before notarization, expected).

`just preflight-app` (LaunchServices, bundle-attributed): `ax_trusted:true`,
`post_event_access:true`, `session.lock:Unlocked`, `screen_lock:Absent`,
`secure_input:Inactive`, `sls_symbols:true`.

Spaces inventory: one display, Spaces `[1,7]`, current `1`, so n = 7 and no
Space > n exists; the guarded controller must create one.

Proven on the native desktop:

- Signed fixture launched through LaunchServices on the current Space
  (inspection only, no tiling): two windows spawned over the socket,
  `scope-inspect --scope io.github.omnitiler.live-fixture` under the probe
  bundle (`ax_trusted:true`) observed both as `AXWindow`/`AXStandardWindow`
  with AX frames, CG bounds/layer/onscreen and Space membership; clean
  `shutdown` (process, socket and run directory gone).
- The guarded `fixture-leg` controller originally refused before any Space
  creation, product start, tap or window effect at the CG-title join. The
  harness now proves the same identity through AX: the announced fixture
  PID's own `AXWindows` element -> AX title parsed for caller+run tag -> AX
  window id, corroborated by a fresh title-free CG `(pid, wid)` entry. Never
  a title or PID guess alone; the AX application element scopes the read to
  the fixture process. `fixture_post` full-read and commit authentication use
  `cg_window_list_basic()` for the `(pid, wid)` association and take the
  window's title from the fixture's own AX element (`ax_title_for_wid`), so
  the stale-target and incarnation checks keep working title-free. The
  `fixture-leg` now focuses each owned window through the fixture's own
  socket before posting its chord; the post path refuses a target that is
  not the app's selected window, and the leg previously posted to whichever
  window happened to be focused.
- After the AX change the signature stage passes (the `--signer` value must
  be the full `designated => …` tail), the AX join passes and the AX title
  authentication passes.
- The leg then refused at `focus-agreement`:
  `focus disagreement: focus agreement timeout (5s): want pid N wid X, have
  slps N workspace N AX (N,Some(Y)) cg_owned=true` — the fixture's own
  socket focus request did not make the target the AX focused window while
  the scoped product was running. Fixed in `a6599b89`: the leg focuses each
  owned window through the fixture's own focus action and requires the
  post path's selected-window proof before posting, so agreement is proven
  per window instead of assuming whichever window happened to be focused.
- Chord arming: under `--preset compatible` with the Ctrl role, bare
  Ctrl+arrows are deliberately left to macOS, so `focus-right` is not armed
  and the tap sees the deliveries with `matched:0`. `--preset authentic`
  arms the chord. Recorded so a future leg does not read that as a delivery
  defect.
- Teardown: the host's `CursorUIViewService` (pid 52832, 64x64 offscreen
  cursor window) replaces its own window mid-leg, which used to fail the
  protected-window roster comparison. `fc1d455d` classifies exactly that
  case with the existing `cg_diff::classify_benign_churn` predicate (same
  pid, both offscreen, identical layer and bounds); the raw before/after
  rows are retained with `benign:true`, and every other category still
  blocks the leg.

Teardown receipts per attempt: `residue:[]`, `owned_reaped:[]`,
`product_reaped:true`, `current_restored:true`; Spaces stayed `[1,7]`
current `1`. One receipt reported `native-topology: separate-Spaces mode
changed during the leg`; the baseline said `separate_spaces:false` while the
finish read disagreed, with the Space inventory itself stable before and
after, so this reads as a baseline-capture disagreement rather than a desktop
change (no owned residue was reported).

A stale 2026-11-12 receipt whose only mismatch was an appeared protected
window was inspected and cleared after confirming that window absent and no
owned residue; copies were kept outside the repository.

Proven on the native desktop in one leg (`f4bf760e`, `--preset authentic`,
`--chord focus-right`, operator settle windows via
`OMNITILER_FIXTURE_LEG_SETTLE_MS`):

- **Admission and reflow through the genuine route.** With one fixture
  window observed (A, full tile `8,38 2224×1123`) a second owned window was
  admitted (B) and the scoped product retiled both to halves (A
  `8,38 1108×1123`, B `1124,38 1108×1123`). AX frames and CG
  bounds/layer/onscreen agreed (`Tile`, Space 1) at every phase, and the
  fixture's own CG oracle agreed (A `2216×2182` px, B `2216×2182 @2248,76`).
  Closing B reflowed A back to the full tile.
- **Retained admission focus per cause.** `cause=automatic-admission`,
  `target=91375_1912`, `observed=91375_1912`, `outcome=already-satisfied`:
  the published request named B and the observation agreed. This is the
  minimal case the
  [suppressed admission focus blocker](#blocker-suppressed-admission-focus-changes-the-next-implicit-subject)
  demanded, so that blocker is closed against the current core.
- **Explicit focus-right.** After A was focused, the chord dispatched with
  `cause=explicit-command` to B, the setter was accepted
  (`pre=91375_1908 post=91375_1912 proven=1`, `attempt_ms=9`), the fixture
  oracle reported `b focused=true` with unchanged frames (focus causes no
  reflow), and both owned windows posted the chord from the leg itself.
- **Clean teardown**: `product_reaped:true`, `current_restored:true`,
  `residue:[]`, `owned_reaped:[]`, `caffeinate_reaped:true`; Spaces `[1,7]`
  current `1` before and after.

Space inventory: one display with Spaces `[1,7]` and `space_count 2`, so a
leg that needs a Space > n must create one — proven below. The controller's
Space creation is **not** an unsupported stub: the AX Mission Control
`mc.spaces.add` press is implemented on macOS (`mc_lifecycle.rs`
`mc_add_space`/`mc_add_space_pinned`), the controller plans owned
destinations for space-mutating chords (`select-N`/`send-N`,
`plan_owned_destinations`), and the product's own creation leg is wired
through `NativeSelection::create`. The single-shot `spaces-create` CLI keeps
its default refusal as policy and offers the same press only through an
explicit opt-in. The leg's exit-time product JSON (preset, `active_chords`,
tap counters) is preserved by the harness from `f4bf760e` onward.

**Space > n leg attempt and the Tahoe Mission Control surface.** The first
`select-3` leg refused at `prove_return` → `probe-create`:
`add-Space press refused (mission-control probe error: Mission Control presence did not become true within 2000ms)`.
Nothing mutated — no SID appended, no current change, no product launch, no
owned residue — but the failed open stranded Mission Control on screen, which
the harness's own public route recovered (`open -b com.apple.exposelauncher`
plus `mc-inspect`/`cg-inspect` verification; reproduced twice).

Root cause, observed rather than inferred: on a newer macOS release family the
Dock exposes **no `mc` AX child at all** (checked at 0.58 s … 13.9 s while
Mission Control was displayed and the WindowManager overlay windows were on
screen). The
Mission Control tree has moved to **`com.apple.WindowManager`**: the
application element's direct child `AXGroup` `mc.display` is the presence
marker, `mc.display` → `mc.spaces` → `mc.spaces.list` holds the "Desktop N"
buttons, and the create control is the sibling `AXButton mc.spaces.add`
("Add Desktop", `AXPress`). With Mission Control closed the WindowManager
element has zero children, the Dock tree has no identifiers, and
`com.apple.exposelauncher` is never a running application.

Proven with one authorized create and remove on this host: press
`mc.spaces.add` → `spaces-diagnose` `[1,7,36]` current `1` (exactly one
appended SID), then `AXRemoveDesktop` → `[1,7]` current `1`, with a
`cg-inspect` comparison of 40 same / 0 changed / 0 gone / 0 extra windows.
Public AX only; no private SLS symbol and no synthetic input. The adapter now
points presence and `spaces_group` at that surface, keeping the 2 s poll,
300 ms stabilize, `AXDisplayID` join, single press and every validator
unchanged. `mc-tree [--labels]` is the read-only dump used for the map.

The single-shot `spaces-create` CLI now has an off-by-default
`--route mission-control-ax --allow-untested` path (`a1aedb47`) that reuses
the same press and reports an attempt plus the verified append; it keeps
`causal_identity` `unproven (M5)` and never claims `created`.

**Space > n leg, proven (`9e4d0303`).** One `select-3` leg completed with
`--preset authentic` on the corrected surface:

- `prove_return` passed (temp create, exact append, empty, goto away, goto
  back, empty, remove) with no `probe-mc` shortfall.
- One owned destination was created: `spaces-diagnose` `[1,7,43]` with
  currents unchanged.
- The fixture window on the owned Space was admitted and tiled: wid 2091,
  AX `8,38 2224×1123`, CG identical, layer 0, onscreen, `disposition:"Tile"`,
  Space `[1]`.
- The chord dispatched: `select-3`, key_code 20, `outcome:"posted"`; the
  product's exit JSON reports `preset:"authentic"`, `active_chords:63`,
  tap `deliveries:2 matched:2 suppressed:2`, `shutdown_clean:true`.
- Teardown: `residue:[]`, `owned_reaped:[]`, `product_reaped:true`,
  `current_restored:true`; Spaces `[1,7]` current `1`; Mission Control
  verified closed (no WindowManager window at layer >= 0).
- The CLI route was exercised once on the real desktop:
  `spaces-create --route mission-control-ax --allow-untested` →
  `appended_sid:44`, `effect_verified:true`, `currents_unchanged:true`,
  `mc_closed:true`; then `mc-remove-empty --space 44` →
  `remove_dispatched:true`, `protected_restored:true`, back to `[1,7]`
  current `1`.

Two lifecycle defects surfaced and were fixed rather than gated around: the
create path discarded the press outcome's Mission Control state (a single
blind toggle could leave Mission Control displayed, and its presentation
window then blocked the emptiness gate, correctly), and a goto press
auto-dismisses Mission Control so an unconditional close could re-toggle it
open. `9e4d0303` makes the close decision explicit (`decide_close`) and
verifies closure (AX group absent and no WindowManager window at layer >= 0,
PID-reuse fenced) before any leg continues; the emptiness predicate,
single-press rule, poll shape, display join and trust/ownership gates are
unchanged, and the async append is now awaited instead of read once.

### Behaviour rows after the port (2026-11-24)

Every rebase row that "compiles but still needs macOS behaviour" was checked
against the adapter source. Status is code-and-test evidence, not a claim of
native acceptance.

| Row | Status |
| --- | --- |
| `sharedNativeRoster` declared | Implemented (the key is required by the codec); the value itself is not asserted by an adapter test. |
| Unified retained focus | Implemented and natively proven for automatic admission, explicit command, close reflow and the cursor-hit (focus-follows-cursor) cause. |
| Pending-focus command subjects | Fixed (`a6451e41`): the four window-shape commands send core's current-focus subject, the observed-foreground substitution and its pre-dispatch refusals are gone, and the homing/domain gates run at apply time on the window core resolved. |
| Retained settlement outcomes | Switch half consumed (`d67b1733`): the retained projection delivers the verdict once per settlement identity, survives a skipped reply, is inert on replay and is authoritative over an earlier reply. The operation half still has no consumer because macOS has no clear lane. |
| Retained `intentDesired` | Adapter-owned atomic host store (`intent_store.rs`) and desired-vs-durable reconciler landed offline (permanent session latch removed); follow-up review findings **F1/F2/F5** and native restart/failure proof remain ([review](macos-v4-open-decisions-review.md#part-5-follow-up-code-review-and-tentative-decisions-2026-11-24)). |
| Switch and fallback removals | No adapter-local policy remains. The no-neighbour directional stay is natively proven on a controller-owned Space 2026-11-24: one tiled fixture window stayed across the work area, `move-left` produced `planner-noop` + `move-accepted` with no frame/focus setter or retry, native AX/CG frame and owned-SID membership were stable pre/post, and teardown was clean. The earlier on-current-Space observation (925/925 identical samples) remains behaviour evidence only. Fixed-float send remains unproven: AppKit exposes no AX size-bound attributes on fixture windows (measured), the shared hints-only predicate requires all four bounds, and a post-admission `set` is repair rather than admission; a product/core admission decision is required ([detail](#fixed-size-admission-exposure-2026-11-24)). [Evidence](#optional-live-legs-2026-11-24). |
| No-space removal | Inherited from core; the adapter writes core rects and produces no no-space rows. Native proof remains blocked: no fixture-scoped way to constrain `NSScreen.visibleFrame`, and 32 default-hint windows cannot geometrically exhaust a 2240×1123 work area. Oversized minimum hints are declared-minimum infeasibility, not the no-space discriminator. [Evidence](#optional-live-legs-2026-11-24). |
| Perpendicular focus recency | Both arms natively proven: explicit-command history and accepted-hover recency (hover leg). |
| Batch 3 admission | Implemented for the measured `AXFloatingWindow`: it maps to `utility` and floats as a palette. `AXSystemFloatingWindow` remains withheld pending a truthful sample ([evidence](macos-batch3-utility-mapping.md)). |
| Best-effort admission | Inherited; live proof blocked by the same missing constrained-work-area route (no-space admission and repair). |
| Settings-key parity | Implemented by construction (raw pass-through, no adapter caps). |
| 16-output `actionOutputs` cap | Inert: the field is always empty because no action context is produced. |

Follow-up findings **F1–F5**, R3 typed ingress, M4 offline implementation, and
the D4 native restart/store proof are landed. The second D5 diagnostic resolved
to fallback Option A (no safe close-only predicate; CG-alive guard retained).
Remaining native-Space, physical, workspace, renderer, lifecycle and release
acceptance stays gated on KDE ST/RC where slice 5 or 7 is involved.
Batch-3 `AXSystemFloatingWindow` is withheld pending a truthful sample (see
[the measurement gap](macos-batch3-utility-mapping.md#withheld-axsystemfloatingwindow)).
Post-only owned-Space placement is now live-proven, so the optional live legs
are blocked only on fixture capability: fixed-float admission and work-area
controls, not adapter policy. The prior receipt is archived; no canonical
receipt remains.

### Cursor/hover producer: implemented and natively proven (2026-11-24)

Before this change the adapter never emitted the cursor pair: `world_builder.rs`
hardcoded `cursor: None, cursor_hit: None`, `ReportInput` had no cursor field,
and `world_runtime.rs` supplied none, so the `focus-follows-cursor` cause and
the accepted-hover arm of perpendicular recency were unreachable.

Facts that constrain the implementation:

- Core's only pointer window is the staged report's `cursor_hit`, and it must
  be a live, non-minimized, non-fullscreen/sticky/host-hidden, homed,
  current-view tiled member (`world_cursor.rs`).
- The protocol requires `cursorHit` to be paired with a non-null `cursor`
  and an id from that same report's `windows` rows (`world_protocol.rs`
  around :4244-4262), so the pair is emitted together or not at all.
- KDE emits the hit only when the raw hit resolves to the same report's
  final emitted windows and omits chrome, foreign and unreadable hits.
- Both hit-test legs already exist unused: `ffi.rs` `ax_element_at_position`
  (public `AXUIElementCopyElementAtPosition`, bounded, `noValue` = honest
  miss) and `ax_hit_test_owner` (bounded parent walk to an exact
  `(pid, wid)`); the cursor point comes from `synth::cursor_position()`.
- The main thread must do no AX on a cadence and the engine thread does none
  today, and reports are only built on an observation/command/terminate, so a
  pointer move alone currently produces no report.

Implemented shape (`8b44a2fa`): `cursor_observer.rs` runs the bounded observer
thread (250 ms tick, 1 s hit refresh, one pending sample retried until the
engine acks its sequence) and posts `EngineJob::CursorSample` on the data lane;
the engine acks `cursor_ack_seq`, attributes the raw hit once at sample time
(`attribute_cursor_hit`), stores it on `WorldSession` and dispatches one
report. `ReportInput` pairs `cursor`/`cursor_hit` only when the stored hit
still names a window row emitted in that same report, so chrome, foreign,
unreadable and stale hits omit the pair; `focused_global`, `foreground_known`
and `focus_changes` never derive from it. The read-only `cursor-probe` CLI and
the CLI/fixture-leg `--focus-follows-cursor` pass-through are also in.

Harness: the opt-in `fixture-leg --hover-probe` flag (adapter-only, requires
at least three `--chord` windows before any effect) runs one bounded sequence
on the same owned fixtures after the normal chords. It launches the product
with `TILER_MACOS_STAGE_TIMING=1` through `open --env` so the cause tokens ride
that launch only, parks the pointer for an honest miss, settles two identical
AX frame reads, and classifies one tall anchor beside a stacked pair. It then
seeds accepted history with explicit chords, warps onto one member while the
other is focused, requires the native focus transition plus a
`focus-follows-cursor` cause token, departs and waits out the resting refresh,
natively refocuses the anchor, and posts the perpendicular chord, requiring the
hovered member to win. Frames, focus transitions, token counts/stamps and
cursor restore are recorded; a layout, focus, or token mismatch refuses instead
of passing. Five unit tests cover the classifier and token accounting.

Offline evidence at `8b44a2fa` plus the harness commit:

- `cargo fmt --all -- --check`, `cargo build --locked -p tiler-macos`, and
  `cargo clippy --locked -p tiler-macos --all-targets -- -D warnings` pass
  (the committed `8b44a2fa` tree needed one rustfmt line in `cursor_probe.rs`,
  folded into the harness commit).
- `cargo test --locked -p tiler-macos --lib`: 1642 passed, 0 failed,
  3 ignored.
- `cargo test --locked -p tiler-macos --test integration`: 149 passed, 1 failed
  — only the documented `runtime_phase1::focus_saturation_drops_counted_and_survives`
  core-policy stop, no new failure.
- Focused tests cover observer cadence/retry, builder pairing and omission,
  sample-time attribution and persistence, the cursor reply token path, and
  the hover classifier/token accounting.

Native receipt (fixture scope `io.github.omnitiler.live-fixture`, signed
ad-hoc `OmniTiler Mac Dev` bundles, preflight `ax_trusted:true`):

- `cursor-probe`: `ax_trusted:true`, element role `AXTextArea`, subrole null,
  AX hit owner present, no error.
- Leg `completed`: three chord receipts posted from three distinct owned
  windows; tap deliveries 12/matched 12/suppressed 12; product scoped
  shutdown clean, joined 4, workers 1/0 failed, observation failures 0.
- Layout: tall anchor 1108×1123 on the left; stacked pair 1108×557 above and
  1108×558 below on the right; off point on the menu bar with an AX honest
  miss (`no-window`, not fixture/owned).
- Native focus transitions: anchor; `focus-l` 123 ms to the above member;
  `focus-j` 96 ms to the below member; hovering the above member produced
  focus-follows-cursor in 224 ms (below→above) with
  `cause=focus-follows-cursor outcome=dispatched-native` at 4.139620 s;
  departure kept focus; anchor refocus; perpendicular `focus-l` 93 ms to the
  hovered above member (expected), with
  `cause=explicit-command outcome=dispatched-native` at 6.498077 s.
- Recency: after the alternate was seeded, it was the newest accepted column
  member and both leaves had accepted history, so the first-child fallback
  could not apply; the hovered member's accepted focus (native feedback
  `setter=accepted proven=1` at 4.195 s) is the only accepted column focus
  between that seed and the perpendicular, so only hover-updated history
  explains selecting it.
- Teardown: `product_reaped:true`, `owned_reaped:[]`, `residue:[]`,
  `current_restored:true`, `caffeinate_reaped:true`; two `benign:true`
  protected-window host churn mismatches (same-pid offscreen 64×64
  replacement, identical layer/bounds), not owned residue. Spaces unchanged
  before and after (current 1, spaces `[1,7]`, count 2); no leftover
  fixture/probe processes or run directory; the terminal was never targeted.

Residual risks: the observer dispatches one report per movement and one per
second while resting even when the core setting is off; a sample landing
mid-flight advances the observation clock and can make that reply refuse stale
(converges through the existing successor path); attribution trusts retained
app/homing rows, so PID reuse after a silent exit is bounded by the prune paths
rather than an independent liveness read. The hover probe proves only the first
three joined windows, so extra owned windows are outside the proof. Its cause
recorder requires `outcome=dispatched-native`, so spent-envelope, duplicate and
accepted-feedback lines cannot satisfy a wait.
The initial admission FFC and the post-observation
pointer-restore FFC fall outside the recorded snapshot (honest miss / cleanup),
and `--preset authentic` still logs transient frame-set retry failures that
converge to exact readbacks.

### Batch-3 utility mapping: `AXFloatingWindow` floats (2026-11-24)

The opt-in transient utility panel was measured natively on the signed probe
bundle (`ax_trusted:true`). The helper's `NSPanel`
(`Titled|Closable|UtilityWindow`, `setFloatingPanel(true)`) reports
`AXWindow`/`AXFloatingWindow` at CG layer 3, onscreen, with matching AX/CG
frame, in two independent runs; the main window reports `AXStandardWindow` at
layer 0. `AXFloatingWindow` therefore maps to `WindowType::Utility`, which core
treats as a float kind; the previous `None` held it. `AXSystemFloatingWindow`
stays withheld: no owned fixture emits it and no truthful local sample exists.
The old fixture comment predicting a dialog subrole was stale and is corrected.
Detail and evidence: [batch-3 utility mapping](macos-batch3-utility-mapping.md).

### Optional live legs (2026-11-24)

Artifacts: the signed probe and fixture bundles were rebuilt under the
persistent dev identity; `preflight-app` reported `ax_trusted:true`, unlocked,
secure input inactive. Inventory: one display, Spaces `[1,7]`, current 1.

- **No-neighbour stay: natively proven on a controller-owned Space.** The
  guarded `fixture-leg` controller now runs an all-post-only leg on one owned
  Space: it executes `prove_return`, snapshots the display row, reads a fresh
  occupancy inventory (fixture-scope windows excluded; unreadable membership
  degrades to the explicit row upper bound), appends exactly one owned Space,
  switches to it, verifies current/ordinal/one-append, spawns and joins the
  fixture window, proves its membership on the owned SID, launches the gated
  product with `--fixture-owned-space`/`--fixture-starting-space`, and
  re-checks current plus membership immediately before posting; any drift
  refuses before a post. A `move-left --preset authentic --seconds 90` run
  with the operator settle (15 s holds) on 2026-11-24 produced:
  - Envelope `outcome:"completed"`; `owned_space` `n:2`, `n_kind:"occupied"`,
    `n_unreadable_windows:0` from the fresh row `[1,7]`, owned SID 57 at
    ordinal 3, `spaces_before:[1,7]` → `spaces_after:[1,7,57]` (exactly one
    append), `starting_sid:1`.
  - Native: one fixture window (wid 2570, `AXStandardWindow`, AX and CG frame
    `(8,38,2224,1123)`, layer 0, onscreen, classified `Tile`, memberships
    `[57]`); five pre-chord and three post-chord samples identical, current
    Space the owned SID throughout.
  - Chord: `kind=input outcome=move count=0`, a `command-refused` with
    `internal=planner-noop`, and a `core-reply outcome=move-accepted`; no tile
    frame-set, focus setter or retry; tap report deliveries 2 / matched 2 /
    suppressed 2 with zero consumed refusals.
  - Teardown: `residue: []`, `current_restored:true`, product stopped
    (`shutdown_clean:true`), owned Space removed, run directory absent, no
    fixture window or leg process left, canonical receipts removed; the only
    mismatches were four benign same-pid offscreen host-churn window-id
    replacements.
  Two harness defects surfaced and were fixed in the controller: the
  separate-Spaces baseline read ran before AppKit's shared application existed
  (`screensHaveSeparateSpaces` answers a stale `false` first, so an unchanged
  setting compared unequal at teardown), and a post issued immediately after
  focus was consumed-refused-basic until the product's input-eligibility
  attestation caught the fresh focus (the runner now keeps one bounded 500 ms
  pacing window). The earlier on-current-Space observation (925/925 sampled
  readings) remains behaviour evidence only. The receipts from the two
  diagnostic attempts (signer-prefix refusal, then the pre-fix mode-read
  mismatch) were inspected and archived as baseline drift with both receipts;
  no canonical receipt remains.
- **Fixed-float send: not run, AX exposure gap.** The controller spawns
  caller windows by id only (`fixture_leg.rs:1563`,
  `fixture_post.rs:181-183`), so it cannot request fixed hints at admission;
  the fixture protocol itself already supports creation-time
  `min_size_px`/`max_size_px`. A bounded read-only exposure check on
  2026-11-24 (below) found that AppKit advertises no size-bound attributes on
  fixture windows at all, so the shared hints-only fixed predicate cannot
  fire and a max-hint reader alone would not help. A `set` after admission is
  repair, not admission, and the controller has no chord route to an
  operator-spawned window. The probe helper's transient panel is a different
  bundle and scope.

- **Constrained work area: not run, harness gap.** The only work-area source
  is `NSScreen.visibleFrame()` with no fixture or CLI override; changing it
  requires forbidden system settings. The 32-chord cap of default-hint
  windows cannot geometrically exhaust a 2240×1123 work area, and oversized
  minimum hints are declared-minimum infeasibility, explicitly not the
  no-space discriminator.

#### Constrained-work-area route audit (2026-11-24)

Route audit only: no fixture or product was launched, no chord was posted, and
no Space, display, or window was written. No capacity float, eviction, or
`float_origin`/`noSpace` row was produced. The native no-space proof remains
**not proven**.

- **Route A (existing seam): none.** Work area is host-global
  (`ffi.rs:9908-9950` reads `NSScreen.visibleFrame()`), flows through the
  topology snapshot to `NativeDomain.visible`
  (`runtime_types.rs:283,440-443`) and `ScreenReport.work_area`
  (`world_builder.rs:457-465`). The product hardcodes
  `TopologyMode::Native` (`tile.rs:2186-2200`); the synthetic default is a
  test-only constructor seam (`runtime_types.rs:1311`). The `tile` CLI has no
  topology or work-area flag (`cli.rs:2690-2990`), `DevSettings` accepts no
  layout/gap section (`dev_selection.rs:137-154`), and raw core settings
  produce only the border keys plus `focusFollowsCursor`
  (`main.rs:1026-1035`, `dev_selection.rs:388-430`). The fixture gate
  (`fixture_gate.rs`) has no geometry seam, and controller spawns carry
  caller ids only (`fixture_post.rs:181-183`, `fixture_leg.rs:1563-1566`).
- **Route B (many-window exhaustion): rejected at ~20k windows.** An exact
  integer model of `geometry::layout_into` plus the serial admission split
  (equal `[1,1]` shares on the plain long edge; focused leaf first, then the
  largest-splittable fallback) was validated against two core fixtures:
  10x10/gap 8 exhausts after four tiles and floats the last two
  (`capacity_admission_tests/engine_admission.rs:80-120`), and 22x9/gap 10
  tiles two then floats `win-c` (`world_connection_tests.rs:552-617`).
  Exhaustion counts: bounds 2224x1123/gap 8 **19,688**; 2240x1123 **21,400**;
  2224x1107 **16,744**; the count is identical when every leg splits the
  largest leaf, so admission order is not the lever. Even the catalog maximum
  gap 64 (`settings_catalog.rs:258,418-440`) needs **512** windows (gap 32:
  2,048; gap 16: 8,192). The controller's 32-chord bound (`fixture_leg.rs:77`,
  one caller per chord `:1214-1217`) is short by 16x-600x, and spawning ~20k
  owned AppKit windows (each needing a native join, membership proof, and
  product observation) has no bounded safe estimate. Rejected.
- **Route C (virtual display): unverified; not attempted.** The
  `CGVirtualDisplay`/`Descriptor`/`Mode`/`Settings` ObjC classes are private
  but exported by CoreGraphics (SDK `CoreGraphics.tbd`; no public headers).
  DeskPad and Chromium's display tests use them from ordinary, non-sandboxed
  processes with no documented entitlement, but that is not verified for the
  signed fixture identity. Chromium's test util documents flaky first-display
  removal and adds a temporary second display to work around it, and display
  add/remove reconfigures displays/Spaces. Exact ownership, product Space
  homing on a virtual output, and a bounded restore path are therefore
  unproven; no display was created.
- **Route D (product/core work-area override): decision only, not
  implemented.** The minimal hook is a fixture-gated work-area rect carried
  in `TileRequest`/`RuntimeConfig` and substituted for the hosted display's
  `visible` in `NativeContext::derive` (`runtime_types.rs:355-445`). It
  cannot be per-Space: one `ScreenReport` per output carries the work area
  (`world_builder.rs:457-465`), and `NativeDomain.visible` is display-wide.
  Impact: the adapter would place real fixture windows inside a sub-rectangle
  while `NSScreen.visibleFrame()` stays unchanged, changing work-area-based
  maximize/zoom inference and border/pager geometry inputs for the whole
  display; it needs a product decision entry, a CLI/gate surface, and tests.
  A modified work area is classed as faked capacity here, so this is recorded
  as an option, not a route to implement.
- **Provenance observability is also missing.** The adapter emits only
  `toggle-float-accepted|consumed` reply tokens (`world_dispatch.rs:63-64`);
  it decodes row `float_origin` (`world_protocol.rs:4589-4591`) but does not
  emit it, and has no `noSpace` token. Core carries `FloatOrigin::Capacity`
  on rows (`world_publish.rs:1049-1053`) and `noSpace` only on an explicit
  ToggleFloat result (`world_command_result.rs:38`). A native no-space proof
  would need a fixture-only provenance/observability addition even after a
  capacity route existed.
- **Decision options.** (1) Defer the native leg; keep the core geometric
  capacity coverage and adapter mapping tests (recommended; no product
  change). (2) Product decision for the fixture-gated work-area override
  above, accepting that the OS work area is not smaller. (3) A separate
  fixture helper that creates a scoped virtual display, only after a
  read-only proof and an exact bounded restore (adds a display and Spaces;
  Chromium's flaky-removal workaround applies). (4) A fixture-only adapter
  diagnostic for `float_origin`/`noSpace` provenance, required by any native
  proof above.
- **Not proven.** The no-space and best-effort admission native acceptance
  remain unproven: no product launch, no chord, no capacity float, no
  eviction/repair, no `float_origin`/`noSpace` row, and no existing
  allocation or Space/display residue was touched or created.

#### Fixed-size admission exposure (2026-11-24)

- **Code path.** The portable predicate needs all four bounds: any missing
  bound returns `not-fixed-missing` before the equality test
  (`size_hints.rs:254,261-263`), and both axes need nonzero equal pins
  (`:286-290`). macOS builds hints from minimums only: `size_hints()` maps
  `min_size` to `min_w`/`min_h` and hard-sets `max_w`/`max_h: None`
  (`native_facts.rs:115-128`); `min_size` comes from `ax_min_size`
  (`native_facts.rs:536`), which probes only
  `AXMinSize`/`AXMinimumSize` (`ffi.rs:9654-9666`), and `max_size` is
  documented and constructed as always `None`
  (`native_facts.rs:62-63,552`). Admission is predicate-only
  (`window_classify.rs:488-496`); `size_settable` is carried, never an
  admission input (`classifier.rs:98-100`). The spawn wire shape is
  caller-id-only (`fixture_post.rs:181-183`), while the fixture protocol
  accepts creation-time `min_size_px`/`max_size_px` (`win.rs:303-314`,
  `protocol.rs:107-109,289-298`).
- **Live evidence.** Signed fixture and probe bundles were rebuilt under the
  persistent dev identity; `preflight-app` reported `ax_trusted:true`,
  unlocked, secure input inactive. The fixture was launched through
  LaunchServices in a private `0700` run dir and bound by its announced
  pid/socket/nonce (the socket inode was owned by that pid). Four owned
  windows were spawned without a Space switch and inspected with the
  read-only `scope-inspect --scope io.github.omnitiler.live-fixture`: fixed
  (640x480, `min_size_px=max_size_px=[640,480]`, `resizable:false`), plain
  (900x520, no hints), bounds (700x500, `min_size_px=[560,420]`,
  `max_size_px=[820,600]`, `resizable:true`), and fixres (640x480,
  `min_size_px=max_size_px=[640,480]`, `resizable:true`). Every size,
  min/max and resizable field was accepted; AX frames matched the requested
  sizes; the fixed window reported `size_settable:false` and the others
  `true`.
- **Result.** All four windows returned `attr_names_status:"ok"` with the
  same 28-name inventory. The only size-related names were `AXFrame`,
  `AXGrowArea`, `AXSize`, and `AXZoomButton`; there was no `AXMinSize`,
  `AXMinimumSize` or `AXMaxSize` on any window, including the resizable
  windows with explicit bounds. The only Min/Max matches anywhere in the
  lists were `AXMinimized` and `AXMinimizeButton`. Every window also reported
  `min_size:null` and `hints_min_w/h:null`, so the value read through
  `ax_min_size` failed too, not just the names inventory. The fixed window
  was an `AXStandardWindow` classified `Tile`, not a dialog/utility float.
- **Why it cannot fire.** The fixed window's facts carve no minimum either,
  so the predicate returns `not-fixed-missing` (the absent max is not the
  only blocker). Because the minimum is absent too, adding an AX max-hint
  reader alone would still return `not-fixed-missing`; adding both readers
  would find no advertised attribute on these windows. Passing
  `min_size_px`/`max_size_px` through the controller's spawn payload is also
  insufficient: the fixture accepted both hints at creation and AppKit still
  exposed neither. The gap is therefore AX exposure on this host, not just
  the controller's id-only payload.
- **Minimal options.** (1) Defer the native send leg and keep the offline
  predicate coverage; native fixed-float send stays unproven with no product
  change. (2) A product/core decision to admit fixed from a different native
  fact (for example `AXSize` not settable, or min equal to the current size):
  this contradicts the recorded admission-only predicate and the explicit
  "resizable is never an admission input" rule, would auto-float
  non-resizable/dialog-like windows, and needs a contract/decision entry plus
  core policy and adapter plumbing. (3) A fixture-only synthetic fact
  injection to exercise classification end to end: explicitly not truthful
  admission, so it cannot count as a native send proof. A max-hint reader
  plus controller spawn payload is not an option by itself; the evidence
  shows the minimum is absent too.
- **Not proven.** The fixed-float send leg remains unproven: no product was
  launched, no chord was posted, no window was sent or moved. Teardown was
  clean (shutdown reply, server exited, socket and owned windows gone, no
  fixture or caffeinate process, private run dir removed, Spaces
  unchanged).

### Integration triage (2026-11-23)

Corrected classification of the red `tests/integration` target (the earlier
"35 all `failed-snapshot`" reading was wrong: `failed-snapshot` is the fake
backend's expected legacy-snapshot refusal, not a cause). Counts:
118 passed / 32 failed at triage start; **148 passed / 2 failed** at triage,
and **149 passed / 1 failed** now. The single remaining case is a documented
core-policy stop, not a stale mock.

| Group | Root cause | Resolution |
| --- | --- | --- |
| `macos_classification` ×19 | The fixture ran `RuntimeConfig::default()` (synthetic topology) and submitted legacy `AxObservation`s; a v4 report built without a native context carries no screen rows, so no row gained allocation or frame. | Test-only: fixture starts in `TopologyMode::Native`, publishes the suite's 1920x1080 single-display snapshot, submit helpers build `ScopedObservation` rows, nonzero-layer geometry asserts the gap-inset native domain, the partial-zero predicate travels as a core setting, and the minimize/restore case drives the production `poll_scoped` + `ExcludedOmission` route. All green. |
| `runtime_phase1` ×6 | Two causes: (1) admission focus is published per cause while the close-focus lane re-reads the selected oracle, whose fake default was `None`, so requests ended `unproven-read`; (2) v4 seeds the focused leaf last, so a naive step direction is an edge (`internal=unchanged`) rather than a defect. | Each fake attests the focus it actually establishes; steps use the direction the seeded layout makes resolvable; asynchronous focus rows settle on the `focus-result` terminal before draining. Five green; `focus_saturation_drops_counted_and_survives` is a documented stop: one session cannot mint a second distinct focus publication while the first is unsettled (a repeat dedups: `world_sequencer.rs` duplicate arm), so the capacity fence is unreachable without a distinct cause this module does not drive. |
| `scoped_gates` ×3 | Stale expectations plus one fixture gap. | Legs expect `["size","position"]` with a redundant final-size skip; the fix-up refusal arms the readback call so the final size leg is still attempted; own-bundle unscoped admission now compares own-process refusal plus a scoped listed-probe refusal. Green. |
| `scoped_tile` ×3, `focus_gaps_trace` ×1 | Owner-slot dispatch retires the outbox row at its terminal completion; stale-select admission has no busy gate; arrival needs its own attested selected oracle. | All three ported with those source contracts cited. `deleted_space_retires_domain_and_reseeds` is green: the fake host now runs a genuine create leg (tree's read-only `RuntimeHandle::native_creation_requests`, mint-baseline `order_before`, a real append, an `Accepted` uncorrelated attempt with `created_native: None`, then the appended readback), so the core binds the trailing spare by its unique-delta rule before the deletion. No assertion was weakened and no core file changed. Two limits recorded: creating *after* the delete cannot bind (the launch spare's mint baseline no longer matches the live order and a second pending row per output latches `AmbiguousCreation`), and the stale-mint-baseline conflict noted in `docs/porting/world-contract.md` remains an open core question rather than something this change resolves. |
| `runtime_freshness` ×2 | Both were settle-order races in the tests, not product flakes: the retained admission focus is controller-visible only until its terminal result. | Tests wait for the `focus-result`/`not-called` terminal before draining and assert one armed dispatch with an empty outbox; `input_eos_is_observed_not_spun` settles on the input owner's join edge (the EOS counter bumps from inside the exiting thread, before the join edge). Green (8/8 and 5/5 repeated runs). |

**Resolved 2026-11-24.** The stop above was a test-shape limit, not a core
allowance. The test now drives a distinct `PositiveClose` cause from a second
live app and reaches the same depth-1 outbox fence (`focus_dispatched=1`,
`focus_dropped_full=1`); the integration target is 150 passed / 0 failed. See
the [focus-saturation resolution](archive/macos-focus-saturation-close-cause.md).

### Offline gates (2026-11-23)

Exact commands from the [macOS environment gate](../macos-dev-environment.md#4-prove-the-portable-offline-cargo-baseline-allowlist-only),
run after the port:

| Gate | First run | After fixes | Notes |
| --- | --- | --- | --- |
| `cargo build --locked -p tiler-core -p tiler-protocol -p tiler-macos` | exit 0 | — | |
| `cargo test --locked -p tiler-core -p tiler-protocol -p tiler-macos` | exit 101: 4373 passed, 16 failed | now exit 101: core 2468 passed / 0 failed, every protocol suite ok, `tiler-macos` lib 1617 passed / 0 failed / 3 ignored; `tests/integration` **149 passed / 1 failed** | The single failure is the core-policy stop `focus_saturation_drops_counted_and_survives`, classified above; no core or protocol failure, no stale mock. |
| `cargo fmt --all -- --check` | exit 1, two ported test helpers unformatted (`drag_drop.rs`, `native_lifetime_tests.rs`) | exit 0 after `cargo fmt --all` | formatting only, no semantic change |
| `cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-macos --all-targets -- -D warnings` | exit 101, 53 `-D warnings` lints (18 lib, 35 test-target) | exit 0 | mechanical lint fixes only: `too_many_arguments` allows per repo convention, private tuple type aliases, `repeat_n`, `clone_on_copy`, needless borrows, `let_and_return`, `collapsible_if`, `field_reassign_with_default`. Two lib lints (`border_runtime.rs` needless return, `native_tile_backend.rs` argument count) were latent in untouched files: the crate did not compile before the port, so the gate had never run. |

## Pre-port review outcomes (2026-11-10)

- Records-only outcome: the approved choices below amend existing policy;
  this review changes no code or v4 contract and claims no native acceptance.
- Records verification: `git diff --check` passed; changed decision links and
  heading anchors were checked. No build, mocked suite or native leg was rerun.
- Evidence boundary from the read-only pre-port review (reported execution,
  not rerun for this records change):
  `mise exec -- cargo build --locked --offline -p tiler-core -p tiler-protocol -p tiler-macos`
  failed with 80 errors from retired v3 APIs. `scripts/macos-dev.test.sh`
  reported `PASS=104 FAIL=0`, mocked only. Native evidence was unrun because
  the current `tiler-macos` HEAD did not build. Source-read facts below are not
  native proof; the historical signed-fixture result does not establish v4 acceptance.

| Approved topic | Pending port implementation / native evidence |
| --- | --- |
| [Fullscreen games](../decisions/macos.md#fullscreen-games) | Slice 5: remove blanket pre-core fullscreen-send policy (`runtime_thread.rs:2242-2267`); core decides membership/policy and adapter reports truthful native refusal. Keep known-exitability-only explicit exit and no autonomous exit. Exact switch/send mechanics remain TBD in the [discriminator](../spec/reference-outcomes.md#v4-macos-platform-fit-discriminators). |
| [Fixture/scoped input](../decisions/macos.md#fixture-and-scoped-input-defaults) | Slice 1: JSON/CLI raw acquisition feeds core revisioned validation/defaulting/per-field diagnostics. Remove missing-bundle-to-own (`native_facts.rs:301-321`) and bundle-wide product self-exclusion, including unscoped probe/helper exclusions (`dev_selection.rs:75-100`); retain scoped test-safety fences. Keep event-first AX recovery plus provisional tunable deduplicated 1 s reread. Native chord/conflict, minimize/restore notification coverage, stale/unwatched suppression and M2 continuity remain evidence, not M7 paint proof. |
| [Development preview](../decisions/development.md#tentative-macos-development-preview) | Preserve signed LaunchServices foreground dev, ordinary apps on each display's current user Space, bounded exact-run teardown with clean owner proof, and scoped finite runs. After slice 1 builds, prove v4 native rebuild/teardown/readiness, permission survival across rebuild and GUI-session recovery. M6 still requires core `exist:false` for native-Space removal. Signed fixture tiling passed 2026-10-20 (`crates/tiler-live-fixture/README.md:228-267`); a chord/workspace harness with explicit fixture ownership remains missing. Retire legacy helper exceptions only after replacement proof. |
| [Explicit Apply only](../decisions/reference-and-shortcuts.md#macos-preview-presets-and-native-shortcut-ownership) | Remove Authentic startup Apply (`tile.rs:2091-2127`) so `SESSION_PERSISTENCE_NOTE` (`desktop_shortcuts.rs:58-61`) is true. After slice 1 builds, verify native per-drain refusal (`workspace_send_managed.rs:753-838`, `desktop_shortcuts.rs:438-458`; test `workspace_send_managed_tests.rs:985-1034` present, unrun) and first-run setup cost. Other preset/ownership choices remain tentative. |

## Contract map

- Authority: [v4 world contract](../porting/world-contract.md), reconciled by the
  [approved M1-M9 resolutions](v4-migration.md#m1-m9-technical-resolution-pending-0c-approval)
  and later amendments. The [native experiments](macos-v4-platform-fit.md#outcome-and-limits)
  are bounded evidence, not implementation or native acceptance. The
  [port notes](../porting/macos-port-notes.md) retain historical v2 inventory;
  their v4 normalization target and native-mechanics inventory are useful, not
  permission to retain retired shapes. Older [macOS decisions](../decisions/macos.md)
  must be read under the newer v4 authority, especially own-process exclusion,
  creation, removal and maximize.

| V4 boundary | macOS obligation and linked constraint |
| --- | --- |
| [Ownership](../porting/world-contract.md#ownership-and-g4-exceptions), [wire/conformance](../porting/world-contract.md#conformance) | Core owns policy, classification and bindings; native AX/CG/Space mechanics stay local. One exact v4 route, not a legacy converter. |
| [Handshake/lifetime](../porting/world-contract.md#handshake-and-lifetime), [settings](../porting/world-contract.md#revisioned-settings) | Adapter lifetime, core-minted generation, readiness and replay fences; revisioned raw settings with per-field diagnostics. M1: only known own-process identity excludes internal UI, never same bundle/executable in another process. |
| [Screens](../porting/world-contract.md#screenreport), [scale](../porting/world-contract.md#scale-and-rounding) | FULL/work rectangles and truthful completeness; CG/AX point-space scale 1, not backing scale. [Native normalization](../porting/macos-port-notes.md#v4-native-coordinate-normalization) remains a port target. M9: null unknown metadata/connector; no display-ID/UUID/ordinal substitute; ambiguous reconnect holds. |
| [Window facts](../porting/world-contract.md#windowfact), [classification](../porting/world-contract.md#neutral-classification-vocabulary-and-applicability), [native observations](../porting/world-contract.md#native-observation) | Neutral facts, fixed unsupported-fact declarations and explicit unknowns, not adapter membership/policy echoes. M2: retained AX identity plus process fences; positive close retires/remints, omission/off-Space absence does not. |
| [Commands/results](../porting/world-contract.md#commands-and-results), [desired world](../porting/world-contract.md#desired-world-and-intent) | Port existing native-Space [selection/send mechanisms](../decisions/macos.md#managed-native-spaces-input-and-transfers) under core authorization; no adapter-local replan or policy fallback. Preserve FIFO input and latest-only native publication. |
| [Attempts/settlement](../porting/world-contract.md#raw-attempts-and-settlement), [visibility/focus](../porting/world-contract.md#visibility-and-focus-ordering) | Native before/after evidence and original spent stamps; setter success is not arrival. M5: without causal creation proof keep `created:null`, hold and never retry. M6/G2: only core `exist:false` authorizes removal, subject to fresh occupancy/pin/minimum/completeness vetoes. |
| [State policy](../porting/world-contract.md#window-state-policy), [stacking](../porting/world-contract.md#persistent-stacking) | M4: `maximize=false`, native `maximized=null`, core-composed work-area overlay; Zoom is ordinary drift, not product maximize. Follow the [composed amendment](v4-migration.md#core-composed-maximize-amendment). Fullscreen exitability and provisional SIP-on sticky/above limits remain truthful; [source findings](macos-v4-platform-fit.md#m-numbered-findings) do not prove universal impossibility. |
| [Intent storage](../porting/world-contract.md#opaque-intent-storage), [recovery](../porting/world-contract.md#freshness-and-recovery) | M3: atomic opaque storage before dependent writes, exact-lifetime verification and intentional-float retention across orderly within-session restart. No title/ID guesses; storage survival remains open. Composed maximize restarts to base without an M3 extension. |
| [Cross-output capability](../porting/world-contract.md#handshake-and-lifetime) | M8: `tiledFloatTransfer=false` provisionally means float-no-remap, not a general move ban. Prove target-display frame plus native Space membership with W2 arrival; until proved, fail with native observation, no guessed remap. |
| [Presentation/renderer](../porting/world-contract.md#presentation-and-renderer-input) | K5/M7: prove local capture/event/renderer causal joins and frame-event re-resolution; suppress unwatched/stale eligibility. Existing unsigned diagnostics are not signed-route acceptance; no new FrameWitness field is evidenced. |

- Carry the [eight OPEN scenario gates](v4-migration.md#phase-0c-approval-outcome-2026-11-02)
  and [per-platform port gates](../backlog.md#open-work), including K5/M7,
  storage survival and exact-lifetime recovery; approval is not closure.
- Track the existing P1 M3, M4, M8 and post-v4 session-lifecycle work, plus the
  **P2**, not P1, [responsiveness comparison](macos-v4-platform-fit.md#responsiveness-evidence-floor-and-design-overhead).
  Lifecycle checks cover lock/unlock, sleep/wake and logout/login with normal
  relaunch; the [observed development-service context loss](macos-v4-platform-fit.md#gui-session-lifecycle-risk-and-post-port-follow-up)
  is not a demonstrated tiler defect.

## First-slice source baseline

- The original plan's source-only baseline is now supplemented by the reported
  failed build above. No native execution or passing v4 build is established;
  historical v2 build results do not establish current v4 buildability.
- Production [world_builder.rs](../../crates/tiler-macos/src/world_builder.rs)
  still imports/constructs the retired `WorldReport` in `build_report`, carrying
  `owner`, `views`, host capabilities and policy/settings fields. Current
  [WorldReportV4/ScreenReport](../../crates/tiler-core/src/world_report.rs)
  instead separate handshake/settings and use v4 facts, attempts and presentation.
  No v4 handshake/settings/report producer was found in the macOS source.
- Production [runtime_core.rs](../../crates/tiler-macos/src/runtime_core.rs)
  `collect_window_facts`/`minimized_fact` still construct removed
  `floating/fit_excluded/fixed_auto/fixed_suppress/observed_workspace` fields,
  bool state facts and old native-membership shapes instead of current nullable
  neutral `WindowFact` fields. These are source-confirmed API mismatches, not
  a complete per-error compiler inventory; the reported build independently fails.
- Test-only legacy constructors also remain in
  [pager.rs](../../crates/tiler-macos/src/pager.rs),
  [world_dispatch.rs](../../crates/tiler-macos/src/world_dispatch.rs) and
  [world_command_map.rs](../../crates/tiler-macos/src/world_command_map.rs),
  including old screen and presentation shapes. First restore a genuine v4
  production loop and its necessary tests; do not mask breakage with aliases or
  a compatibility report. Retained shared enum/codec arms alone prove no macOS
  v4 producer support.

## Proposed dogfoodable slices

- Each row retains the prior report/reply foundation and leaves a build usable
  at its explicitly proven level. Connection alone is preparatory; slice 1 ends
  at ordinary fixture tiling. Later functionality is not claimed early.

| Order | Increment and acceptance evidence |
| --- | --- |
| 1. Handshake and report/reply | Replace the baseline above with genuine handshake, revisioned settings, v4 capture/decode/apply and necessary attempt/storage plumbing. Prove real producer bytes through real core/protocol, readiness, narrow holds, generation/restart fences and ordinary signed-fixture admission/reflow; no repeated handshake on a bound wait. |
| 2. Selection | Port native selector and view-attempt correlation; prove focusless terminal selection and continued ordinary tiling. Explicitly bound/document populated-selection limitations; no fabricated focus acknowledgement. |
| 3. Arrival | Port W2 native before/after membership and settlement. Prove fixture transfer to the intended native home, refusal/unknown holds and arrival before follow/departure; setter acceptance alone never passes. |
| 4. Focus feedback | Port causal retained-request focus evidence and anchored-descriptor consumption under the approved unified design. Prove authorized focus/refusal without replay, exact lifetime joins and K5/M7 native event/render evidence, including stale/unwatched suppression. |
| 5. State targets | Port fullscreen/sticky/stacking and composed-maximize frame application, attempts and recovery. Prove base restore, external-clear/fullscreen replacement, send/work-area changes and restart-to-base; retain durable intent/preimages where required. M4 implementation dependencies belong in shared core, not adapter-owned policy. Acceptance waits for both KDE ST/RC passes and absorption of resulting core changes. |
| 6. Scaled metrics at scale 1 | Apply logical metrics once, separate native fractional-coordinate normalization from scaling, and recheck tiling/rendering on backing-scale changes while report scale stays 1. Do not absorb the Windows non-unit-scale prerequisite. |
| 7. Physical reconnect/rehome | Supply M9 physical identity facts and prove user-run disconnect/return, ambiguous identity holds, FULL/work geometry and rehome behavior without regressing prior slices. Rehomed members are not new admissions. M8 crossing and two-display acceptance remain explicit gates. Acceptance waits for both KDE ST/RC passes and absorption of resulting core changes, including the RC fix/merge decision. |
| 8. Scenario runner | Add a fixture-first ordered native runner with exact owned-state cleanup, plus offline genuine-producer journeys using real native shapes. Cover earlier slices and FIFO bursts; transport/readiness receipts never substitute for native outcomes. Physical keys, paint, hotplug and crash recovery remain separately named evidence. |
| 9. Remaining v2/v3 removal | Map each supported legacy assertion to genuine v4 coverage; port uncovered behavior first. Cull obsolete implementations separately with no-live-path evidence and review. Retain immutable recorded fixtures, active migration records and both shared stage5 proofs. Cull proved-obsolete touched code earlier; no dedicated broad sweep. |

- For each slice, run the relevant [macOS allowlisted build/test/fmt/clippy gates](../macos-dev-environment.md#4-prove-the-portable-offline-cargo-baseline-allowlist-only)
  and affected producer regressions. Native legs require fresh identity joins,
  independent AX/CG/Space readback and exact owned-resource cleanup under the
  [live guide](../live-macos-testing.md). Record code presence, offline execution,
  native readback, physical/visual proof and unrun conditions separately.
- State the actual usable surface after each native increment. Development
  dogfoodability is not conformance across all adapters or release acceptance;
  unsupported behavioral outcomes stay TBD in the existing
  [minimal discriminators](../spec/reference-outcomes.md#v4-macos-platform-fit-discriminators).

## Slice 1 implementation and evidence

- Scope: replace the retired production boundary with real v4 handshake,
  revisioned raw settings, neutral reports, decoded replies and fenced apply.
  Ordinary signed-fixture admission/reflow is the native acceptance floor;
  later slice behavior and M3 restart survival are not inferred from it.
- Separate signed-fixture recovery leg: retain identified fixture windows on
  an off-current or unreadable native Space through the held interval, with no
  inferred close or writes while held. Switch to that owned Space, obtain a
  fresh complete capture, and prove those same lifetimes are picked up and
  tiled normally using independent AX/CG frame readback. Report this outcome
  separately from first admission and close/reflow; callback or switch receipts
  alone do not pass it.
- Implementation units: restore the macOS-owned production boundary and its
  affected tests; remove approved self-exclusion policy; separately remove
  automatic startup shortcut Apply; exercise genuine producer/core/protocol
  journeys; then run allowlisted gates and signed-fixture native acceptance.
- Storage ordering: incorporate opaque intent writes before dependent native
  writes, returning only exact-live-lifetime records. A nondurable or failed
  store cannot authorize dependent writes. The shared contract already supplies
  these lanes; ordinary observation-only admission/reflow emits no intent save
  (`world_publish.rs::drain_intent_saves`). M3 storage survival and re-identification
  remain separate acceptance work.
- The macOS-owned boundary now contains handshake/settings and report/reply
  codec dispatch; this is code presence, not native acceptance.
- Current offline follow-up (2026-11-11; test execution reported, source and
  retained logs reviewed): the three superseded lifecycle expectations are
  repaired without production changes. Full macOS library: raw exit 0,
  **1519 passed / 0 failed**. Scoped Native integration fakes now provide
  explicit neutral facts through the real macOS capability handshake, not
  all-supported capabilities or production raw-fact shortcuts. Original
  behavior assertions remain: `tile_entry` 8/0 and `runtime_freshness` 9/0
  (both raw exit 0); `runtime_phase1` 34/5 (raw exit 101). A termination-focus
  case also varied between earlier runs; stability is not established.
  The last full integration baseline was 89/61 (raw exit 101), before these
  repairs; no combined post-repair result is claimed. Classification (18
  baseline failures), own-bundle gate (1), and scoped topology/selection (2)
  remain untriaged. Fresh core library and combined strict clippy remain unrun.
- Protocol follow-up: the first full run passed 228 tests before a launch
  stall at `target_lanes`; a second passed 109 before `intent_wire`. Neither
  produced a final raw exit code (tool timeouts, not suite passes).
  `target_lanes --list`, each of its two tests individually, and both together
  subsequently passed with raw exit 0; both bodies took 0.00 seconds.
  `intent_wire --list` reproduced the stall after a 0.15-second completed
  build, before harness output. An active-process sample stayed entirely in
  `_dyld_start`: this is a pre-main launch/runtime stall, not an identified
  hanging test body or slow compilation. Why dyld stalls remains unknown.
  Full protocol verification remains incomplete.
- Offline checkpoint (2026-11-10): allowlisted locked/offline build and macOS
  all-target test compilation pass. A fresh full macOS library run passed all
  1474 tests. After absorbing the upstream sticky-placement fix, core library
  passed 2085 tests and the build, compilation and focused producer/restart
  regressions passed again. Modified Rust sources pass edition-2024 formatting.
  Integration fixtures remain under repair: 65 passed / 85 failed initially;
  explicit neutral facts in scoped captures improve this to 83 passed / 67
  failed. Missing synthetic raw facts and input codec routing remain to audit.
  The mocked development shell suite passes 104/0. No native acceptance yet.
- Directional routing correction: production focus/move mapping now emits v4
  implicit directional commands instead of codec-invalid legacy operations.
  Only the command-correlated focus effect dispatches; no admission focus is
  authorized by this change. Producer-map byte round trips pass for both
  commands. Real-engine focus controls, exhausted-edge typed refusal and
  consumed-sequence replay pass. The integration focus walk retains the edge
  and recovers with unchanged geometry/workspace desires. Its Accepted envelope
  counter does not imply command success; the command refusal is separately
  pinned. Identical desired frames may be rewritten on reconciliation, so this
  proves no frame change, not zero native setter calls. All four focus/gap
  integrations and the two affected scoped focus/move tests pass. The library
  passed 1475 tests before adding the final edge proof. After absorbing the
  dynamic-workspace lifecycle change, the fresh full library result is 1473
  passed / 3 failed; those three tests assert superseded spare-backing/native
  minimum expectations. Core library passes 2100 tests, allowlisted build and
  macOS all-target compilation pass. Fresh integration result: 86 passed / 64
  failed. Most failures still use rawless synthetic fixtures; three genuine
  close/focus-loss failures are isolated below.
- Raw settings checkpoint: focused acquisition/connection tests pass for CLI
  precedence, explicit null versus omission, invalid-field diagnostics with
  valid siblings retained, and revision/generation refusals. Core-effective
  values supply the existing presentation carrier; no hot reload is claimed.
- Explicit Apply: the automatic Authentic startup Desktop-N holder mutation
  was removed. The focused startup no-mutation regression passed; explicit
  Apply CLI and per-drain refusal preservation also passed focused checks.
  Native first-run cost and holder refusal readback remain unrun.
- Native readback, physical/visual proof and signed-fixture admission/reflow
  remain unrun. The earlier close/focus-loss blocker is superseded by the
  linked implementation below; the admission/explicit-focus mismatch remains
  open. Source checks are not native acceptance. Slice 1 is
  incomplete; no usable native surface or conformance acceptance is claimed.
- Neutral-fact follow-up: hidden-app and minimized rows now preserve available
  raw facts separately (`minimized` versus `host_hidden`), including process
  identity. Focused regressions passed during implementation. Test captures
  now supply explicit ordinary native facts and production macOS capabilities;
  this restored genuine admission and hide/restore geometry assertions instead
  of accidentally exercising all-null classification holds.
- Unpersisted-save safety: a session-lifetime latch now prevents a later reply
  without writes from bypassing an earlier unpersisted intent write. Two focused
  tests passed during implementation. This is fail-closed plumbing, not a
  durable store or M3 restart acceptance. A production-dispatch regression now
  proves the initial save refusal and a later observation-only reply remain
  no-write, while a save-free control applies.
- **Shared focus correction implemented:** direct focus now refuses the staged
  minimized/host-hidden target before retained membership can authorize it;
  directional focus also refuses a hidden source. Existing refusal vocabulary,
  observations, held layout and separate minimized restoration are preserved.
  The genuine macOS producer regression passes both complete and incomplete
  inventory contrasts:
  `mise exec -- cargo test --locked --offline -p tiler-macos --lib world_builder::tests::first_transition_same_report_focus_never_restores_hidden`
  Independent source review accepted the narrow guard; focused core tests pass.
   Core/protocol build and behavioral targets pass. The workspace unsafe
   allowlist test fails on three unchanged fixture attributes; protocol strict
    clippy fails on two unchanged single-element loops. After upstream changes,
    core strict clippy also reports eight `unnecessary_get_then_check` findings
    in frame-policy tests. macOS strict clippy is blocked by those dependencies.
    Attribution to the base is source comparison only. Full gate
   closure remains pending. See the
  [original blocker](macos-core-trailing-pager.md#core-blocker-same-report-minimized-target-during-an-inventory-gap).
- M5: an observed appended native Space is no longer claimed as causal creation
  identity. The staged attempt preserves native orders and setter evidence with
  `created:null`; its focused stage/codec regression and the repaired native
  attempt lifetime suite pass. No causal native creation proof is claimed.
- Genuine producer journey: native-shaped captures admit 1, then 2, then 3
  ordinary windows through production dispatch/codecs/core and a mocked frame
  sink. An incomplete inventory retains all lifetimes without native writes;
  complete recovery and positive-absence close/reflow pass. A rebuilt-report
  codec check is supplemental, not an internal transaction transcript.
- Restart fences: sessions obtain a fresh OS-random nonce for incarnation and
  generation seed; entropy failure remains unbound. Focused tests pass old
  traffic rejection, reused-PID-shaped isolation and entropy failure. These
  fences do not establish M3 durable intent recovery.
- Reply decoding uses actual macOS capabilities before sequencing/apply.
  Dead pre-v4 report/session policy plumbing is removed; normalization with
  remaining test readers is test-only. No compatibility report is retained.
- Latest verification limits: the integration target is 149 passed / 1
  documented core-policy stop; every other offline gate is green. The native
  admission/reflow floor and the admission-focus minimal case are proven (see
  [native leg attempts](#native-leg-attempts-2026-11-23)); later slice behavior
  is not inferred from them.

### Blocker: suppressed admission focus changes the next implicit subject

- Historical minimal offline case: observe A focused; admit ordinary B to its right
  while observing A still focused; suppress automatic native admission focus;
  issue explicit focus-right. `admission_never_steals_focus_but_explicit_focus_dispatches`
  reproduces no queued focus request instead of one request for B.
- Source/log agreement: `world_publish.rs::assemble_publication_rows` calls
  `world_sends.rs::record_published_focus`, which records admission effects in
  the canonical implicit-focus slot. macOS
  `world_dispatch.rs::dispatch_focus_effects` deliberately spends but does not
  actuate admission effects. The next command log names B (`1001_12`) as subject
  while observed focus remains A (`1001_11`), with `internal=unchanged` and
  `wire=operation-refused`. `focus-accepted` only counts the accepted envelope;
  it does not prove a successful command or a ticket/effect correlation bug.
- Resolution approved 2026-11-11: macOS shall actuate published admission focus
  through [one retained focus request for every cause](retained-focus-requests.md),
  replacing admission suppression without changing candidate selection. Core's
  execution-time logical cursor is separate from observed foreground; adapters
  need only the latest publication, with no current-command ticket requirement.
  Q1-Q3 fix any-batch non-focus preservation, same-report cause precedence and
  fresh-hover identity. Implementation and per-cause skipped-publication proofs
  remain pending; the red reproduction above is not a new result for that target.
  Preserve FIFO implicit subjects, selection dependencies and native safety.
  No assertion weakening is authorized. The other four phase-1 focus failures
  are not yet causally attributed to this case; native acceptance remains open.
- Closed natively 2026-11-23: the retained-focus implementation publishes one
  request per cause and the adapter actuates it. In one live signed-fixture leg
  (`f4bf760e`) admitting B published
  `cause=automatic-admission target=91375_1912 observed=91375_1912 outcome=already-satisfied`
  (the request named B and the observation agreed), and the following explicit
  focus-right dispatched to B and applied (`pre=A post=B proven=1`). See
  [native leg attempts](#native-leg-attempts-2026-11-23).
- Minimal discriminator and unsupported outcomes are retained in the
  [platform-fit table](../spec/reference-outcomes.md#v4-macos-platform-fit-discriminators).

### Superseded blocker: positive close with no observed focus

- Historical minimal case: admit A/B in separate app lifetimes, focus A, positively close
  A while its app remains alive, and report complete AX/CG absence plus B's
  ordinary facts with no new observed focus. Three scoped close/reflow tests
  failed: survivor desires had no frames and the occupied scope held Unreadable.
  The covering close journey was a distinct passing case.
- Independent offline review instrumented an isolated source copy and reported
  the exact complete close report and shared sweep refusal: `focus-mismatch`,
  followed by `Hold(Unreadable)`, zero frames. No repository source was changed
  for that probe. Source inspection at that checkpoint confirmed the focusless
  Reconcile escape in `tiler-core/src/engine.rs` excluded reports with staged
  closes; `world_publish.rs::classify_sweep_hold` mapped the refusal to Unreadable.
- Resolution: the [approved core-owned positive-close implementation](core-owned-close-refocus.md)
  now reflows readable survivors without fabricated observed focus and retains
  eligible close-refocus requests in the latest publication. All four scoped
  close regressions pass in that record. The reproduction above is historical,
  not a current unresolved design choice. Native close/reflow and held-Space
  recovery acceptance remain unestablished; use the linked user-run procedure
  for close/refocus, not the superseded automated preparation.
- Other reviewed boundaries, source-only: ambiguous creation retains
  `created:null`; native retirement is guarded by core `exist:false` and fresh
  occupancy proof (currently unknown occupancy prevents removal); unsupported
  capabilities and unpersisted intent writes remain fail-closed. These checks
  do not close native M3/M5/M6 or FIFO/render acceptance.

### Draft user-run off-current Space recovery check

- Assessment only; unrun and not full held/unreadable acceptance. Resume after
  the open offline focus blocker and verification gates are resolved. Use the
  [signed-bundle preflight, bounded launch and stop route](core-owned-close-refocus.md#user-run-macos-native-acceptance)
  with a fresh log directory, replacing TextEdit scope with
  `io.github.omnitiler.live-fixture`. Do not overlap product instances.
1. Follow the [fixture bundle setup and ownership joins](../../crates/tiler-live-fixture/README.md#macos-bundle-launch-and-scope);
   create two disposable ordinary fixture windows on an existing user Space.
   Do not create/remove Spaces or move unrelated windows for this check.
2. Start the fixture-only bounded product run. After admission/reflow settles,
   retain fixture `get`/`list`, run ownership, and signed-route
   `scope-inspect --scope io.github.omnitiler.live-fixture` output: native window
   ids, Space membership, AX frames and CG bounds. Keep the same fixture process
   and both windows alive throughout.
3. Physically switch to another existing user Space, leave the fixture windows
   untouched, and switch back before the bounded run expires. If the fixture
   remains visible on the other Space, the off-current leg was not established.
4. Obtain fresh complete scoped readback. Expect the same owned native windows
   to remain and tile normally; compare independent AX/CG frames and report
   disappearance, held/non-recovery state, jumps or jank rather than retrying.
5. Let the bounded product run stop; require `shutdown_clean:true` and released
   interception/border. Shut down only the verified fixture instance and verify
   process/window/socket absence using its documented cleanup route.
- Evidence limit: native ids/frames plus visual recovery do not prove retained
  core lifetime identity or zero setters during the away interval. This does
  not induce arbitrary AX unreadability. Full acceptance still needs joined
  core-lifetime/publication and no-write evidence; do not infer it from physical
  switching, callbacks or `scope-inspect`'s launch-date presence bit.

## Slice 5 M4 composed-maximize offline implementation (2026-11-24)

- Scope: close the macOS adapter gap for the approved composed-maximize route
  (`maximize:false`, native `maximized:null`, core-owned work-area overlay).
  This is offline implementation plus regression evidence, not slice-5 native
  acceptance; final acceptance remains gated on KDE ST/RC and core absorption
  under tentative TD3.
- Input: the scoped-tile catalogue adds the Ctrl-role `M` chord mapped to
  `InputSemanticAction::ToggleMaximize`, dispatched as
  `WorldCommand::ToggleMaximize { window: CurrentFocus }`. The catalogue grows
  34 to 35 chords (Compatible 30 to 31; product 63 to 64). Like orientation,
  it is edge-triggered: a held repeat consumes without dispatch until the
  physical `M` key-up, so a broken chord cannot wedge the next press. No
  adapter-owned maximize policy or native state write is introduced.
- Frame application: the existing typed reply/desire lane already applies
  core-authorized frames through the native backend; no adapter maximize path
  was added. The offline integration test records frames the fake backend
  actually applied, proving the reply reached the effect lane rather than only
  the desired-geometry cache.
- Offline evidence:
  - [`tile_entry.rs`](../../crates/tiler-macos/tests/integration/tile_entry.rs)
    drives Ctrl+M through the real runtime/fakes: the overlay applies as the
    1440x900 work area, a held repeat is ignored, the second discrete press
    applies the retained base, and a top-anchored 1280x800 work-area change
    re-derives and applies the new overlay before clear publishes the
    recomputed base.
  - [`world_builder.rs`](../../crates/tiler-macos/src/world_builder.rs) proves
    the producer carries no compose bit: a fresh engine restarts to the base
    slot; a fullscreen subject is refused with no work-area write; a
    cross-output send preserves the bit and recomputes the destination work
    area, clearing to the destination base; a committed gesture move clears
    the bit before reflow and the next quiet report publishes the base.
  - Core per-case coverage remains in
    [`world_composed_maximize_tests.rs`](../../crates/tiler-core/src/world_composed_maximize_tests.rs).

### M4 native apply/restore proof (2026-11-24)

- One bounded signed-fixture leg with the M4 diagnostic harness used two
  fixture windows on an owned post-only Space and the real product tap. The
  leg-only `maximize-toggle` diagnostic focused the first window, posted
  `Ctrl+M` twice through the real tap, and read native AX frame plus CG
  onscreen state before, between and after the presses.
- Readback: the base right-half frame was `1108x1123` at `(1124,38)`; the
  first press applied `2240x1138` at `(0,30)`; the second press restored the
  exact base. `AXZoomed` was absent (`null`) at all three reads, while CG
  reported onscreen `true` throughout. The product tap forwarded all three
  chord actions (5 matches and 5 suppressions), and the product stopped
  cleanly with `shutdown_clean:true`; the leg retained no receipt. Teardown
  mismatches were benign same-PID offscreen host churn only. A prior attempt
  refused teardown on unrelated protected-window host churn; the clean rerun
  is the recorded evidence.
- This proves native frame application and exact restore through the real
  product path on the signed fixture. It does **not** prove physical Ctrl+M
  delivery, paint, native Zoom/drift interaction, external move/resize clear,
  fullscreen replacement, send/output/work-area changes, restart-to-base, or
  slice-5 acceptance. Exact work-area geometry remains the offline core claim.
- Harness boundary: `maximize-m` is a fixture-route-only chord and
  `maximize-toggle` is a leg-only diagnostic alias; neither adds product
  policy, a native state lane, or an M3 dependency.

- Remaining native legs: physical Ctrl+M delivery, paint, native Zoom/drift
  interaction, external move/resize clear, fullscreen replacement,
  send/output/work-area changes and restart-to-base. Do not infer those from
  the offline fakes or from this apply/restore proof.

## D4 native intent-store restart proof (2026-11-24)

- Scope: one bounded signed-fixture leg at `1c90b677`, using an isolated
  intent store under the leg's owned run directory. The real per-user store
  fingerprint was unchanged before/after and no `intent-v1*` row appeared
  there. The probe/harness changes were diagnostic-only and were not ported.
- Sequence: launch fixture A (two windows) → first product → float A and
  confirm the durable row before B exists → launch fixture B (two windows)
  → second product ingests the staggered report → orderly stop → third
  product over the same store → focus, add a B window, toggle another A
  window → stop and exact teardown.
- Evidence: A's float row was durable before B's PID existed; B's row was
  hydrated on its later report. A and B float frames were identical before
  and after restart. Store rows were byte-identical after the first stop,
  after the second stop and after restart. The post-restart toggle wrote and
  read back a new row. Both products reported `shutdown_clean:true`; teardown
  left no fixture/probe processes, run directories or receipts.
- Assertion limit: the probe's `frames_equal_across_restart` was hardcoded and
  its tile predicate ignored tile position. The observed App A tile/float
  overlap is the documented float-outside-tree model plus slot re-adoption,
  not late hydration: two tiles split the work area while floats keep native
  frames. A future rerun should compare the tile-rect multiset, keep float
  size+position exact, and record expected overlap rather than failing it.
- Unproved here: store failure/recovery, re-login namespace rejection,
  numeric launch-date stability beyond the observed in-session rows, and
  native visual z-order after restart.

## Transferable KDE lessons

- Fail closed: unknown never matches; setter acceptance is not native settlement ([runner oracles](v4-phase2-kde-scenario-runner.md#selected-design)).
- Fixture-first runners own setup and exact owned-state cleanup, not the whole desktop ([runner lifecycle](v4-phase2-kde-scenario-runner.md#outcome-and-accepted-evidence), [fixture rules](../live-kwin-testing.md#fixture-first-live-tests)).
- Check native shortcut holders and reverse key collisions before injected chords ([selection evidence](v4-phase2-kde-scenario-runner.md#selection-scenario)).
- Batch FIFO commands after one capture and apply only the latest publication; resolve implicit focus at execution ([rapid-input decision](../decisions/commands.md#kde-rapid-input)).
- Rehomes are not admissions and must not spend admission focus effects ([reconnect correction](v4-phase2-kde-physical-reconnect.md#disconnect-rehomed-members-are-not-admissions)).
- Offline journeys must use the genuine producer/core route and fakes seeded from real shapes ([producer journeys](v4-phase2-kde-scenario-runner.md#offline-genuine-producer-journeys), [captured-shape evidence](v4-phase2-kde-scenario-runner.md#outcome-and-accepted-evidence)).
- Green offline suites do not close native acceptance ([evidence separation](kde-v4-acceptance-checklist.md#scope-counts-and-result-rules)).
- Never use pointer-click activation as a runner shortcut ([selection runner gate](v4-phase2-kde-scenario-runner.md#selection-scenario)).
- Never repeat a possibly executed mutating transfer without fresh unambiguous proof ([settlement rule](../decisions/workspaces.md#transfer-and-view-settlement)).

## Open decisions and unknowns for the macOS port

- The pre-port choices above are approved; implementation/evidence remains.
  Shared host-settled geometry design still needs user approval after slice 1
  and before slice 3, and before any core change.
- M3: D4 (2026-11-24) selects an adapter-owned atomic store with live-lifetime
  verification, pre-admission import, desired-versus-durable reconciliation and
  recoverable hold; numeric launch-date stability, store adoption and re-login
  namespace rejection still need native proof. No store adoption is established
  by finite CG ID churn experiments.
- M4's product route is decided, not reopened here; shared implementation and
  native restore/clear/send/restart outcomes remain unproved. M5 causal creation,
  M6 retirement consumer and M2 lifetime continuity still need native proof.
- M8 target-display/Space arrival, M9 connector normalization and identical-monitor
  reconnect/re-login stability remain unknown; multi-display integration stays
  [parked](../decisions/macos.md#drag-and-drop) unless its scope is revisited.
- K5/M7 signed-route witness joins, event coverage/watch-cap behavior and
  frame-event re-resolution remain open; do not infer paint proof from callbacks.
- SIP-on foreign sticky/above mechanisms, Settings own/foreign-process distinction,
  FULL-bounds two-display validation, SkyLight group-underlay evaluation and
  physical input/fullscreen-game behavior retain their existing
  [platform-fit limits](macos-v4-platform-fit.md#m-numbered-findings),
  [0.1 scope](../decisions/macos.md#macos-0-1-scope-and-gate) and backlog gates.
- Establish the bounded within-macOS responsiveness baseline and post-port
  session-lifecycle/recovery evidence before extending claims across platforms.
- All nine lessons above have linked point-of-use records; no new generic
  lessons file or uncited lesson is proposed.
