# macOS v4 adoption sweep

Offline audit of macOS output-local rosters, single report ingress, unified retained focus, retained settlements and retained drag preview against the [world contract](../../porting/world-contract.md) and [focus decisions](../../decisions/focus.md). No operation or switch actuator or protocol change was added.

The connection capability and ingress were already present. For focus, the
core validates retained automatic requests against fullscreen and sibling
ownership before publication. The duplicate adapter retained-roster veto and
its test-only copy were removed, with the native target's live safety checks
unchanged. The new consumer test reproduced a zero-attempt veto before the
fix.

| Item | Offline evidence |
| --- | --- |
| `sharedNativeRoster:false` — already done | `world_connection.rs:49–54,165–182` sends the typed production handshake (`hostOwned:true`, per-output, no shared roster). `empty_acquisition_accepts_on_core_defaults` exercises that connection; the active `handshake-macos.json` agrees on the roster tuple, and `manifest_connection_fixtures_round_trip_or_reject` passed (92 active connection fixtures). |
| Single report ingress and flag removal — already done | `world_dispatch.rs:389–391` calls `handle_report_checked` (`world_admission.rs:168–178` bridges to the one prepared transition). `typed_ingress_refuses_invalid_and_over_cap_typed_reports` and `typed_ingress_classifies_duplicate_reused_and_superseded` exercise the route. `world_builder.rs:216` forwards `foreground_fullscreen` as a fact, not a capability or opt-in, covered by `presentation_ingress_focused_fullscreen_never_pauses_without_game_signal`; core's `focused_fullscreen_presentation_tests` passed (10). |
| Unified retained focus — partly done; duplicate veto removed | `world_dispatch.rs:3927,4472` consumes the latest request and leaves automatic policy to core. `world_close_focus.rs:784–919` owns eligibility; `fullscreen_sibling_vetoes_pending_request_without_alternate` and `implicit_acts_on_restored_sticky_foreground` passed. The live scoped target fence (`native_tile_backend.rs:524–539`) remains. `retained_focus_does_not_reapply_automatic_fullscreen_policy`, `focus_envelope_accepts_every_cause_latest_only`, `close_latest_only_skips_issuing_and_dispatches_once`, and `selection_focus_latest_only_releases_once_through_real_owner` cover retained application. `close_focus_wire` passed (13), including rejection of the retired `effects` key. |
| Retained settlements — partly done; typed operations and gesture recovery added | `world_runtime.rs:104` merges typed operation `(window, kind)` and switch `output` projections; absent owners retire silently. `world_dispatch.rs:1986,2036,2076` keeps operation/switch outcomes diagnostic-only and publishes gesture terminals only from applied replies. `drag_session.rs:1331` matches the pending drop's exact gesture id and a newer publication. `retained_operation_settlements_distinguish_phases_and_prune_departed_owner`, `switch_settlement_survives_a_skipped_reply_via_the_retained_projection`, and `gesture_release_settles_from_later_accepted_report_without_gesture_ticket` exercise this path, including inert redelivery. |
| Retained drag preview and `result` removal — partly done; preview identity fixed | `world_runtime.rs:1623` and the runtime, drop and fixture paths use the identity-gated retained preview projection; `gesture_start_binds_then_update_previews_core_slot` rejects a replacement id. `WorldReplyV4.result` was already gone; non-preview results, when read, come from the issuing dispositions (`macos_first_float_lands_at_current_tile_frame`, `world_dispatch.rs:15469`). Shared omitted-ingress retention and stale-replacement regressions passed (2); strict retained operation/switch/preview and retired `effects` decoder checks passed (3). |

The existing corrected-switch test now supplies the complete projection,
including the unchanged output, rather than an artificial partial update.

The focused macOS run passed 24 tests. The full macOS library run passed
1,725 tests (three ignored), and the integration run passed 152 tests.
Formatting and strict all-targets Clippy passed for the verification gate.

Native checks are unrun. The diagnostic Meta+left carrier still hides between
samples (`drag_session.rs:450`); the [macOS drag decision](../../decisions/macos.md#drag-and-drop)
keeps that producer diagnostic-only. Painting commandless retained replies in
the diagnostic carrier versus waiting for the passive producer remains a
product decision. The five backlog lines carry the remaining work.
