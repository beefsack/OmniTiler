# Retained cursor warp request

- Status: deferred to the Windows v4 port (User 2026-11-15); Q1 and Q2 stay open until then. Draft design only. No acknowledged-events lane or timer/expiry.
- Preserved product scope: `cursorWarp` defaults `false`, i3/sway preset enables cross-output warp, Hyprland preset leaves warp off pending per-verb evidence ([spec](../spec/functional-spec.md) lines 410-416, [outcomes](../spec/reference-outcomes.md#cursor-first-output-targeting-discriminators)).

## Today (source at `b2e55839`)

- Paths: `C/` = `crates/tiler-core/src/`, `P/` = `crates/tiler-protocol/src/`, `K/` = `kwin/src/`, `M/` = `crates/tiler-macos/src/`, `W/` = `crates/tiler-windows/src/`.
- Core computes `cursor_warp: Option<(i32, i32)>` (`C/world_report.rs:2662`) in `warp_for_publish` (`C/world_cursor.rs:256-329`), called from `publish_v4` (`C/world_publish_v4.rs:147,216`).
- Emits only when `cursor_warp_setting()` is true (`C/world_engine.rs:2249-2256`), `inventory_complete` is true, `!cursor_game_paused`, `!gesture_barrier && gesture.is_none()`, `observation.cursor` maps to a `cursor_output`, and `effects.iter().rfind(EffectKind::Focus)` finds a target in `windows` on a different output with an unheld destination scope and `effective_frame` center clamped to destination `work_area` within `+-GEOMETRY_BOUND`.
- Source mismatch in v4: `C/world_publish_v4.rs:102-123` strips `EffectKind::Focus` from wire `effects`, while `:147` reads `&effects` and `C/world_cursor.rs:264-266` assumes the point rides alongside its wire focus effect. Also `effects.rfind` ignores `drain_focus_candidates` cause precedence (`C/world_close_focus.rs:587-660`) and misses `PositiveClose` (held in `pending_focus`, not `effects`).
- Skipped/coalesced vs exact duplicate: skipping the issuing reply loses the warp because `effects` are per-run and later observation-only replies with no new focus candidate emit `null` (commandless reports can still originate hover/admission/close); exact cached duplicate input returns the cached reply (`C/world_admit_v4.rs:348-358`), repeating the bare point.
- Capability and platforms: `HostCapabilities.warp` is fixed per connection; `warp:false` rejects non-null `warpCursor` (`P/world_protocol.rs:6446-6448`, `warp-capability-false`); `cursorWarp:true` with `warp:false` retains the raw setting and emits `unsupported-capability` (`docs/porting/world-contract.md:198,219-220,264-268,283,932`, `C/world_engine.rs:2249-2256`). KDE declares `warp:false` (`K/world-connection.ts:130`), parses bounded `[x,y]` (`K/world-report.ts:1953-1968`), and logs ignored (`K/world-entry.ts:5767-5769`). macOS declares `warp:false` (`M/world_connection.rs:61`) and refuses non-null (`M/world_dispatch.rs:109,4564-4576`). Windows legacy consumer (`W/tiling_sys/world_diff.rs:4411-4498`) needs wire `effects.rfind(Focus)`, `is_foreground`, `shown`, cursor-equality skip, and `last_warp_obs` dedup; not ported to v4.

## Proposed retained form

- Replace bare `warpCursor: [x, y] | null` with one latest nullable slot per `(adapter, gen)` carrying `WarpRequest`: `{id, target, rev, out, ws, point}` (`id: Id`, `target: WindowId`, `rev: Counter`, `out: OutputId`, `ws: WorkspaceId`, `point: Point`; plus frozen `pointerSeq` baseline if Q2A is chosen). Freeze all tuple values at mint; the same `id` never retargets or recenters on relayout.
- Mint from the `drain_focus_candidates` winner (not `effects.rfind`), capturing its opaque `id` and immutable issuing `rev` before `pending_focus` satisfaction removes `pending_focus`; this fixes the stale `effects` producer coupling to match existing one-focus-winner policy. Keep `warp:false` rejection unchanged.
- Correlation and live readiness: adapter readiness checks **live** native `foreground == warp.target` and live shown `(warp.out, warp.ws)` immediately before the setter (report `focused_global == target` alone does not prove foreground stayed unchanged). While an exact matching `focusRequest` (`focusRequest.id == warp.id`) awaits arrival, adapter waits; once `focusRequest` satisfies and clears (`focusRequest: null`), correlation survives without requiring non-null `focusRequest`.
- Exact-once adapter-local accounting: spend `(adapter, gen, id)` before at most one `move_cursor` attempt (or safety/already-at-point no-call; no retry and no guarantee of native warp success). No core acknowledgment or removal on adapter spend: the slot remains inert until semantic retirement. A latest current-bound `null` or different-id request cancels unstarted old work; a later accepted publication with the same `id` retains pending work; stale or old-generation replies and callbacks are ignored and cannot authorize or alter current work. Retain bounded accounting (latest spent/retired id/rev). Preserve accounting across same-gen reconnect; lost accounting requires a fresh handshake (`gen` reset), clearing both core's slot and adapter accounting.
- Retain/clear/replace: retain across skipped/coalesced, commandless, duplicate replies, and `pending_focus` satisfaction (`focused_global == target`, per latest-only C3 delivery). Clear on window/output departure, exclusion, homing change away from `(out, ws)`, view drift making frozen `(out, ws)` no longer current/eligible (not expected selection arrival to that scope), output/topology/work-area changes making the frozen `point` invalid for its destination output or work area (never silently recenter the same `id`), scope hold/unreadable (distinct from adapter no-call), game-pause, gesture-barrier, `cursorWarp` disabled live, `pending_focus` retiring via failure/refusal/baseline-override, or later observed foreground overriding the correlated decision even after `pending_focus` is gone (waiting for `target` while matching `pending_focus` is pending is not an override; requires no historical focus log). Supersession by later focus or pointer motion follows Q1-Q2.

## Effects and adoption boundary

- In-place pre-release v4 amendment (`docs/decisions/contract.md#contract-authority-and-v4-migration`), no version bump or dual shapes: replace `warp_for_publish(&effects, ...)` and stale `effects` comments with `pending_warp` and strict `WarpRequest` codec (`warp:false` still rejects non-null); update KDE parser/ignore log (`warp:false`, no actuator) and macOS typed/strict reply (`warp:false`); adopt in Windows v4 port replacing `effects.rfind(Focus)` + `last_warp_obs`.
- Replace directly, do not layer. Preserve frozen `world-wire/v4`, `stage5`, `stage5-v4`, and `kde-v4`; refresh active producer fixtures at implementation.

## Tier 1 source evidence (in-process; no retained-transport replay analogue inferred)

| Reference | Trigger and config gates | Miss / failure behavior |
| --- | --- | --- |
| `cosmic-comp` `epoch-1.11.0` | `src/shell/focus/mod.rs:439-480`, `src/input/actions.rs:582`: `cursor_follows_focus` + `should_update_cursor` moves pointer to target center on focus update | Skips if `focused_geometry` is `None` |
| `Hyprland` `v0.57.2` | `src/config/shared/actions/ConfigActions.cpp:62-125` (focus/monitor verbs call `warpCursor` directly), `:1077-1087` (workspace warp only if `WARP > 0` via workspace/monitor inheritance, `force` on `WARP == 2`); `src/desktop/view/window/Window.cpp:1020-1031` | `src/pointer/PointerController.cpp:16-30`: master `cursor:no_warps` skips unless `force`, updating monitor focus only |
| `i3` `4.25.1` | `src/x.c:1549-1556,1368-1387`: `x_set_warp_to` stages `warp_to` when `mouse_warping != none`; `x_push_changes` queries pointer/outputs and calls `xcb_warp_pointer` if cross-output | Clears `warp_to = NULL` unconditionally after query failure or same-output check |
| `sway` `1.12` | `sway/input/seat.c:1526-1547`, `sway/input/cursor.c:1166-1201`: `seat_consider_warp_to_focus` gated by `mouse_warping`, output box (`WARP_OUTPUT`), and container box (unless `force`) | Returns without warp if `WARP_NO`, `!focus`, or point already inside output/container |

## Open questions (user choice required)

Existing `warp:false` capability behavior is already normative and not reopened.

### Q1: What supersedes a pending warp on later focus or non-focus activity?

- Options: (A) Any newer focus decision of any cause (including same-output/non-warping focus) clears/replaces `pending_warp`; non-focus commands never clear it. (B) Only a newer cross-output warp replaces it; same-output focus leaves old warp pending.
- Consequences: A and B differ before C's native arrival (logical decision supersession vs delayed observed supersession): under B, W1 remains projected while C's focus is pending and can still warp if B is/returns foreground before C arrives; once C is observed foreground, observed-override retirement clears W1 under either option. A matches `focusRequest` supersession (`docs/decisions/focus.md:6`). **Rec: A**.
- Tier 1 evidence: Shared Tier 1 table above (in-process focus-coupled triggers bind warp to the current focus step; no skipped-transport replay analogue inferred, no native-tested replay behavior claimed).

| Variant | Minimal action sequence | Candidate outcomes |
| --- | --- | --- |
| Warp focus supersession | Cursor on L, focus A on L; focus B on R (warp W1 minted), skip issuing reply; incorporate a focus decision for C on L, before C is observed foreground; consume latest reply | A: `warpCursor` is `null`. B: W1 still projected (zero setter unless B foreground and all gates pass); once C is observed foreground, override retirement clears W1 under either option |

### Q2: Should user pointer motion cancel a pending warp (and incur a new causal witness)?

- Current gap: `W/ffi/llhook.rs:252-257` notes raw `SetCursorPos` warps can surface as non-injected `WM_MOUSEMOVE`, so `MOUSE_MOVE_SEQ` does NOT distinguish user vs own-warp motion. Coordinate-only comparison (`report.cursor != baseline`) is rejected under causal freshness (`docs/principles.md#ordering-and-races`), not offered as a viable choice.
- Options:
  - A (user motion cancels, new contract/adapter cost): Add a cumulative generation-bound pointer-activity witness in reports (input fact, not delivery ack) excluding own-warp injections via a later-proven native origin route (`warp:false` hosts report absent/unavailable; unproven hosts stay truthfully unsupported). Freeze baseline in `WarpRequest`; revalidate the witness live immediately before the setter and refuse any newer witnessed motion. This catches observed post-capture input, not an atomic read-and-warp guarantee; native adoption must document host ordering and any remaining check-to-call race; lost origin/accounting forbids replay.
  - B (user motion does not cancel): No new pointer-activity witness; still-current warp stays until newer focus, departure, or safety fence, accepting delayed pointer yank.
- Consequences: A cancels on causally witnessed user motion without timing, at the cost of a new `warp:true` host origin witness; B avoids hook/origin cost but allows delayed yank after user pointer movement. **Rec: A** (native origin gap explicit).
- Tier 1 evidence: `i3` (`4.25.1:src/x.c:1379-1382`) masks `ROOT_EVENT_MASK` around `xcb_warp_pointer` to suppress own-warp `MotionNotify`; this does not establish a Windows provenance route or delayed cancellation rule, and no skipped-transport analogue is inferred.

| Variant | Minimal action sequence | Candidate outcomes |
| --- | --- | --- |
| Warp pointer-motion retirement | Cursor `(100,100)` on L, focus A on L; focus B on R (warp W1 to `(900,300)`), skip issuing reply; report B focused with pointer unmoved; user moves pointer `(100,100)->(200,200)->(100,100)` on L; consume latest reply | A: pointer-activity witness retires W1 (`warpCursor:null`). B: W1 retained; warps once when B foreground and other gates pass |

## Test and native-check plan (offline/future)

- Core/protocol: winner precedence (`PositiveClose`/`ExplicitCommand` over `Hover`), skipped-reply retention across `pending_focus` satisfaction (`focused_global == B`, `focusRequest:null`, W1 retained and warps once), later observed foreground override clearing `pending_warp`, close/id-reuse/output departure, `cursorWarp` setting disable, Q1/Q2 retirement, `warp:false` non-null rejection, and exact/upper-bound codec sizing.
- Adapter/harness: exact-once `(adapter, gen, id)` spending, different vs unchanged `id` (same `id` retains pending work and never recenters on relayout; current-bound `null`/different `id` cancels; old-gen ignored), live pre-setter foreground and post-capture motion mismatch -> no setter + raw report drives core retirement, inert redelivery, same-gen reconnect vs fresh-handshake reset, and KDE `warp:false` parse/ignore-log coverage.
- Refresh active producer fixtures at implementation; keep frozen `world-wire/v4`, `stage5`, `stage5-v4`, and `kde-v4` untouched.
- Native checks (future, fixture-only per [KDE guide](../live-kwin-testing.md) and [Windows guide](../live-windows-testing.md)): KDE `warp:false` negative check (no actuator); supported-port positive check later with authorization (skipped-reply delayed warp, own-warp vs user-motion origin proof, no-double-warp).
