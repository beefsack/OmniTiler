# macOS Fullscreen Float Integration

- **Status:** Complete (2026-11-25), offline verification only.
- **Source:** [backlog](../../backlog.md) P1 "macOS fullscreen float integration"; [shared fullscreen float toggle](fullscreen-float-toggle.md) (macOS section); [window-state decision](../../decisions/window-state.md#floating-membership-and-placement).
- **Boundary:** No native, fixture, or AppKit verification.

## Goal

Deliver and verify the v4 macOS adapter's end-to-end fullscreen-float `ToggleFloat` path offline without a duplicate command route or adapter-side core policy:

- first toggle while the focused subject is already fullscreen floats the subject beneath the retained overlay, removes its tile allocation and reflows siblings, with zero subject frame writes, frame operations, focus writes or native state writes;
- a second toggle under the still-retained overlay re-tiles invisibly, still with zero subject writes;
- with fullscreen cleared and no command, the float base reveals its durable centered-60% placement and the tile base reveals its tiled allocation; the observed fullscreen frame is never adopted as a placement;
- the core placement token withheld during fullscreen is revealed by the adapter exactly once after the overlay clears, without adding a broad float-placement lane;
- fullscreen-subject writes stay suppressed by the existing `ScopeAuthority`/`frame_authority` emission rules, not a new special-case float route.

## Scope and non-goals

- Scope: audit the production semantic-action map, shared core reply shapes and macOS reply consumer; add the narrow adapter-owned deferred reveal for a float placement withheld during retained fullscreen; add focused macOS regressions through the genuine `WorldEngine` and the production `consume_world_reply`.
- Non-goals: shared core/session policy, protocol/wire behavior, Windows integration, KDE changes, native/fixture/AppKit proof, live testing, and the separately tracked ordinary first-float-in-place policy. Ordinary floats (not applied under fullscreen) stay write-free; no second command route and no adapter-side membership policy are added.

## Approach

- The production map is already complete: `InputSemanticAction::ToggleFloat` maps to `WorldCommand::Operation(CoreCommand::ToggleFloat { window: CurrentFocus, float_rect: None })` (`world_command_map.rs:387`), pinned by `world_runtime_tests::operation_commands_cover_every_supported_tiling_action`. The subject resolves core-side at execution; the adapter never substitutes an observed window.
- Shared core owns the policy. The first toggle floats the subject beneath the retained overlay (no tile allocation, durable centered-60% fallback minted, sibling reflow); the second fresh-admits it invisibly as a normal tile. During fullscreen the subject row is `Untouched`/`Untouched` with the core placement token still attached, while the sibling reflow row is `Controlled`.
- The existing macOS emission gate stages only `Controlled` rows with an effective frame in a current scope, and spends `frame_operation` only after that filter. That is why the fullscreen subject takes zero writes with no new special case; retained fullscreen truth additionally feeds D5 overlay accounting (`frames_overlay_skipped`).
- Because core keeps an ordinary float row `Untouched` (the same row shape while the overlay is retained and after it clears), the adapter records the core token and durable target for a floating subject under retained fullscreen in `WorldSession` — but only when the applied report carries the production `ToggleFloat` ticket command (`Operation(CoreCommand::ToggleFloat { window: CurrentFocus, .. })`) and the row's placement token belongs to that exact command (`row.command_seq == ticket command_seq` or `frame_operation == op-<command_seq>`). A floating lifetime that enters fullscreen on an observation-only reply is never mistaken for an under-overlay toggle, and an unrelated floating lifetime under fullscreen with an older outstanding token (e.g. on another fullscreen display) is never captured. Observation-only replies can reveal a recorded placement but never create one. It does not spend the token, stage a frame, or write native state while fullscreen remains.
- On a later applied reply after the overlay clears, the recorded placement settles exactly once through the existing frame/frame-op lane when the lifetime is still floating, in a current scope, still present in adapter retained truth (same generation/lifetime), with neither the row nor its scope held, and not vetoed by retained minimized/owner-hidden/sticky truth. It uses the exit row's same token and target when core still publishes them; otherwise the adapter owns the one-shot write of the remembered target. A matching observed frame, an unfloat/re-tile, a differing replacement token, a closed/lost lifetime, a row-level or scope-level hold, or a veto never writes a duplicate; vetoes and holds keep the placement pending until they clear. The target is always the core-published durable frame, never the fullscreen pose.
- The regressions build the command from the production map, feed reports to `WorldEngine::new_test_bound_with_initial_settings`, admit through `tiler_protocol::world_admission::handle_report_checked`, and consume the real replies through `world_dispatch::consume_world_reply` with a live `FrameSlot`. Writes are asserted from `diagnostics.frames_emitted`/`frames_overlay_skipped` deltas, `last_desired`, focus dispatch/queue, native-creation publication, the reply-derived emission-eligible row set and the session's pending-placement record.

## Units

1. Audit the map, core reply shapes, consumer emission gate and both exits.
2. Add the narrow `WorldSession` deferred-placement record and reveal in the world reply consumer.
3. Add focused macOS regressions for both toggles, both exits, the echo, vetoes/holds, close, ordinary floats (including observation-only fullscreen entry), the ticket discriminator and the authority gate.
4. Records, decision status/evidence, final scoped gates and staging.

## Accepted evidence

Offline gate at the final tree (2026-11-25):

- `cargo fmt --all -- --check` — clean.
- `cargo test --locked -p tiler-macos --lib` — **1,722 passed / 0 failed / 3 ignored** (baseline 1,716/0/3 plus six new regressions).
- `cargo test --locked -p tiler-macos --test integration` — **152 passed / 0 failed**.
- `cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-macos --all-targets -- -D warnings` — clean.

Focused regressions, all in `crates/tiler-macos/src/world_dispatch.rs` `mod tests`:

- `fullscreen_toggle_float_under_fullscreen_stages_no_subject_frames` — first toggle: subject floating with no allocation, `effective_frame` exactly the centered-60% fallback `(160,120,480,360)`, `Untouched` frame/membership authority, retained `op-2` token, empty state, no visibility desire; sibling reflows `Controlled` to `(0,0,800,600)`. The reply also carries a test-only unrelated floating fullscreen lifetime with an older `op-1` token: only the subject is recorded, the unrelated token stays unspent and schedules no reveal. The seed stages two frames; the toggle stages only the sibling, increments `frames_overlay_skipped` by two (subject plus unrelated fullscreen row), dispatches no focus or native creation, and records the pending `{op-2, fallback, generation}`. Second toggle: allocation returns as `(404,0,396,600)`, no effective frame, `Untouched`, no token, sibling shrinks; again only the sibling stages and the pending record clears on the re-tile.
- `fullscreen_toggle_float_exit_without_command_reveals_base_state` — float exit with native truth still at the fullscreen pose: durable fallback `(160,120,480,360)` with `Untouched` authority and retained token, never the fullscreen frame; the adapter spends `op-2`, stages exactly the sibling reflow plus one deferred subject write at the fallback, clears the pending record, and publishes the same fallback as plan truth. Tile exit after the double toggle takes the normal lane: both rows `Controlled` at `(0,0,396,600)`/`(404,0,396,600)`, both reveal writes staged, no fallback write and no pending record.
- `fullscreen_toggle_float_deferred_reveal_echo_veto_and_close` — the reveal writes once; a settled echo observed at the fallback stages no repeat subject write; retained roster minimized truth vetoes the reveal (record kept, token unspent) and clearing it releases exactly one write and spends the token; a test-only row-level `Hold(ConstraintClamp)` on the exit row likewise keeps the record pending and the next authoritative reply releases exactly one write; a lifetime pruned from adapter truth by explicit close clears the record with no invented write.
- `fullscreen_toggle_float_ordinary_first_float_stays_write_free` — an ordinary (not-fullscreen) first float arrives `Untouched` with the retained token; the adapter stages only the sibling, records no pending placement, and leaves the token unspent for the separately tracked ordinary float-in-place policy. The same lifetime then enters fullscreen on an observation-only reply (with a test-only stale token added to the row): no deferral is recorded and no placement write is invented, then the observation-only exit also stages only the sibling with the record still empty and the token unspent.
- `fullscreen_subject_emission_gate_requires_controlled_authority` — on the real applied first-toggle reply, the subject row carries geometry plus `op-2` but `Untouched` authority and stages nothing. A byte-identical test-only counterfactual with only `frame_authority` flipped to `Controlled` stages, proving both that the existing authority check is the discriminator and that the real fullscreen reply withholds the write without a special-case gate.
- `fullscreen_toggle_float_refused_after_accept_still_records_for_reveal` — the post-accept early-return legs, driven end to end through the real native dispatch route (production report builder over one readable live Space plus unit-test roster raw facts) and the production `CurrentFocus` ToggleFloat ticket. Leg A (`intent-write-held`): no durable store, so the applied reply's `intent_desired` row holds; the record-only reconciliation stages `{op-<seq>, durable target, generation}` from the real applied row before the refusal returns. Leg B (`stale-observation`): an isolated temporary durable store persists the intent row, then the post-accept hook invalidates the clock and the same record is staged before the stale refusal returns. In both legs the durable target is read from the real applied reply row (`effective_frame`, the settings-resolved centered float — `(165,125,470,350)` for this 800x600 fixture), never hand-picked. A later observation-only exit through the same production builder stages exactly the sibling reflow plus one deferred subject write (`frames_emitted` +2), clears the record, and spends the placement token; removing the four early-return record calls makes this test fail at the empty pending record.

## Material decisions

- The deferred reveal is adapter-owned state, not core policy: shared core keeps ordinary float rows `Untouched` and does not spend their token while fullscreen, so the adapter remembers the exact core-published durable target and settles it once after the overlay clears. No shared-core, wire or new command route changed.
- Ordinary floats remain write-free on macOS. The pending record is only created when the applied report carries the production `ToggleFloat` ticket command, the float row is under retained native fullscreen, and the row's placement token belongs to that exact command sequence; a row that is merely `Untouched` for another reason, an observation-only fullscreen entry, or an unrelated floating fullscreen lifetime with an older token never enters the lane. This keeps the approved macOS float-in-place decision (separate P1) untouched.
- Core token preference: when the exit row still carries the same token, its row target and token drive the one-shot write and settle core-side; when the token is gone, the adapter writes the remembered target once. A differing token supersedes and clears the record without a second write.
- A matching observation must not repeat a frame: the record is cleared without writing when adapter retained geometry already equals the target, and after the single reveal. A row-level hold (`ScopeAuthority::Hold(..)`) or a scope-level hold defers like a veto instead of writing through incomplete inventory; only an authoritative reply releases the write.
- D5 is unchanged: while fullscreen is retained the adapter records only, spends nothing and writes no frame/focus/native state; sibling reflow still stages. The fullscreen frame is never captured because the remembered target is the core fallback.
- The existing `ScopeAuthority`/`frame_authority` gate remains the reason fullscreen subjects are not written by the ordinary lane; the deferred lane is narrow, explicit-token-only, and does not generalize `Untouched` staging.

## Verification limits

- No native AppKit/AX run and no fixture or live leg: no real fullscreen transition, AX write trace or on-screen placement was observed.
- `FrameSlot` has no read API, so writes are proven by staging-count deltas plus the reply-derived emission-eligible row set; token spend is proven through the session's spend result and the pending-record state.
- The constraint-clamp `Hold`, stale-token observation, and unrelated-older-token checks still use test-only counterfactuals. As of 2026-11-26, `fullscreen_toggle_float_deferred_reveal_echo_veto_and_close` Leg B instead supplies a genuine minimized fact and checks the real core row: `floating:false`, `Hold(Minimized)`, no allocation, effective frame, placement token, or command sequence. Only the sibling stages during that veto. The next report restores the core's `Untouched` float and the same durable frame/token, and stages exactly one subject reveal. The focused macOS lib test passed.
- The post-accept early returns are covered end to end by `fullscreen_toggle_float_refused_after_accept_still_records_for_reveal` for both `intent-write-held` and `stale-observation`, plus the subsequent observation-only exit reveal. The regression drives the `WorldOutcome::Accepted` arm; the `WorldOutcome::CommandConsumed` early returns call the same record helper but are not separately driven end to end. The earlier synthetic-route attempt could not be used: the synthetic builder's empty raw facts classify the fixture windows as unknown holds, so core refuses the toggle as `partial-observation`; the regression instead drives a readable live Space with realistic roster raw facts. The stale leg depends on an isolated temporary durable intent store (test-only store parts) so the intent row persists before the emission gate refuses; the negative control is the same test with the four early-return record calls removed, which fails at the empty pending record. All of this remains offline evidence.
- Ordinary first-float-in-place remains unimplemented and separately tracked; this change only reveals a fullscreen-withheld durable placement after the overlay clears.

## Remaining native acceptance

- Real macOS fullscreen double-toggle with AX/frame-write tracing: zero subject geometry/focus/state writes while the overlay is retained, sibling reflow allowed, and exactly one deferred reveal at the durable placement after exit (no repeat on echo).
- Native verification of the minimized/hidden veto release and close/lifetime clearing on a live session.
- Windows adapter integration remains separately pending; ordinary macOS first-float-in-place remains the separate P1 item.

### Approved signed-fixture procedure (2026-11-26)

Start with the signed fixture on a controller-owned ordinary Space and
prove an independent exit and return route before changing fullscreen
state. Use the fixture window only, with borderless native fullscreen
and no exclusive mode. Continue only with fresh `AXFullScreen` and
native Space readback for the same window lifetime. Count subject
frame, focus and state writes while fullscreen and read sibling reflow
independently. On exit, require one reveal at the durable frame and no
repeat on its echo. Stop on lost identity, permission, topology or
exit route; restore only owned state. The approval is for this
fixture journey, not MX6 fullscreen/game suppression.

The fixture socket supports `borderless_fullscreen` and `windowed`.
Those are requests, not native exit or return readbacks. The legacy
`owned-fullscreen` command uses a different helper's spawn record and
cannot authenticate the signed fixture. Its held-child exit route also
does not apply to the LaunchServices fixture. Do not count either
legacy helper behavior or a successful socket reply as the independent
exit proof required by this procedure.
