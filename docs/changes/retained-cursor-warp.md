# Retained cursor warp request

- Status: stage 1 landed in core and protocol (2026-11-25): one retained,
  nullable `warpRequest` per reply with resolved Q1/Q2 and report
  `pointerActivity`/`warpSpent` observations. The point-only `warpCursor` field
  is gone. No acknowledged-events lane or timer/expiry. Stage 2 (2026-11-25)
  landed the Windows exact-once adapter and raw-input witness; its clean-HEAD
  live receipt at `fdead7d45e` flipped Windows to `warp:true` in the next
  commit. Windows Raw Input cannot distinguish physical from injected input;
  the adapter's own `SetCursorPos` was proven not to advance the witness, and
  injected motion is conservatively treated as user activity.
- Decisions: Q1 = any newer focus decision targeting another lifetime cancels,
  a newer decision for the same target keeps; Q2 = newer or unknown report
  `pointerActivity` cancels against the frozen `pointerBaseline`, and an own
  adapter warp must not advance the hardware-origin generation.
- Preserved product scope: `cursorWarp` defaults `false`, i3/sway preset enables cross-output warp, Hyprland preset leaves warp off pending per-verb evidence ([spec](../spec/functional-spec.md) lines 410-416, [outcomes](../spec/reference-outcomes.md#cursor-first-output-targeting-discriminators)).

## Current stage-1 contract

- Paths: `C/` = `crates/tiler-core/src/`, `P/` = `crates/tiler-protocol/src/`, `K/` = `kwin/src/`, `M/` = `crates/tiler-macos/src/`, `W/` = `crates/tiler-windows/src/`.
- Wire: the reply carries one required nullable `warpRequest: null | WarpRequest`, replacing the point-only `warpCursor` field. The closed object has ordered keys `{id,target,rev,out,ws,point,pointerBaseline}`: `id` a bounded opaque nonempty Id, `target` a WindowId, `rev` the quoted decimal u64 focus-decision revision that minted it, `out` an OutputId, `ws` a WorkspaceId, `point` the carried `[x,y]`, and `pointerBaseline` a quoted decimal u64 or null when unknown. Reports add `pointerActivity` (quoted decimal u64 cumulative hardware-origin generation; null unknown) and `warpSpent` (bounded opaque spent id, accepted or no-call; null none). Both are observations, never core commands.
- Mint and freeze: one request max per reply, minted from the winning cross-output focus decision at its issuing revision. Every tuple is frozen: the same `id` never retargets or recenters on relayout and an unchanged request is identical bytes. Never same-output, never a fabricated point.
- Retention: core keeps the request across observation-only replies, unresolved focus, applied-but-unverified focus feedback and delayed foreground verification. Exact `warp_spent`/`warpSpent` id match retires it. Target departure/recycle, unreadable frame, incomplete inventory, setting off, capability off, gesture and game pause also clear it.
- Capabilities: `HostCapabilities.warp` is fixed per connection; `warp:false` rejects any non-null `warpRequest` with `warp-capability-false`, and `cursorWarp:true` with `warp:false` keeps the raw setting while emitting `unsupported-capability`. KDE and macOS declare `warp:false` and emit/accept only `null`; Windows declares `warp:true` and implements the raw-input witness, `SetCursorPos`, and spend-before-move (see the live receipt at `fdead7d45e`). Normative tables: [world contract](../porting/world-contract.md#one-retained-cursor-warp-request).

## Retained-form decisions

- In-place pre-release v4 amendment, no version bump or dual shapes: strict closed codec with exact/upper-bound sizing and unchanged `warp:false` rejection. Frozen `world-wire/v4`, `stage5`, `stage5-v4` and `kde-v4` stay frozen; active producer fixtures follow the new lanes.
- Adapter exact-once accounting `(adapter, gen, id)` before at most one move attempt (or a safety/already-at-point no-call), with no retry or guarantee of native success, is implemented for Windows (`W/tiling_sys/world_diff.rs`), while KDE and macOS remain `warp:false`. `warpSpent` reports the spend truthfully; it never removes a core request by itself. An own adapter warp must not advance the hardware-origin pointer-activity generation.
- Focus correlation uses exact opaque `(id,target,rev)` equality; readiness checks live foreground and shown `(out,ws)` immediately before any setter. Windows implements these checks around its exact-once warp lane.

## Tier 1 source evidence (in-process; no retained-transport replay analogue inferred)

| Reference | Trigger and config gates | Miss / failure behavior |
| --- | --- | --- |
| `cosmic-comp` `epoch-1.11.0` | `src/shell/focus/mod.rs:439-480`, `src/input/actions.rs:582`: `cursor_follows_focus` + `should_update_cursor` moves pointer to target center on focus update | Skips if `focused_geometry` is `None` |
| `Hyprland` `v0.57.2` | `src/config/shared/actions/ConfigActions.cpp:62-125` (focus/monitor verbs call `warpCursor` directly), `:1077-1087` (workspace warp only if `WARP > 0` via workspace/monitor inheritance, `force` on `WARP == 2`); `src/desktop/view/window/Window.cpp:1020-1031` | `src/pointer/PointerController.cpp:16-30`: master `cursor:no_warps` skips unless `force`, updating monitor focus only |
| `i3` `4.25.1` | `src/x.c:1549-1556,1368-1387`: `x_set_warp_to` stages `warp_to` when `mouse_warping != none`; `x_push_changes` queries pointer/outputs and calls `xcb_warp_pointer` if cross-output | Clears `warp_to = NULL` unconditionally after query failure or same-output check |
| `sway` `1.12` | `sway/input/seat.c:1526-1547`, `sway/input/cursor.c:1166-1201`: `seat_consider_warp_to_focus` gated by `mouse_warping`, output box (`WARP_OUTPUT`), and container box (unless `force`) | Returns without warp if `WARP_NO`, `!focus`, or point already inside output/container |

## Decisions (Q1 and Q2 resolved)

Existing `warp:false` capability behavior is already normative and not reopened.

### Q1 (resolved): supersession on later focus activity

- Decision: any newer focus decision targeting another lifetime cancels the
  retained request; a newer decision for the same target keeps it. Observed
  foreground override and the safety gates still clear it. This matches
  `focusRequest` supersession (`docs/decisions/focus.md:6`) and applies to
  same-output/non-warping focus decisions as well.
- Tier 1 evidence: shared table above; in-process focus-coupled triggers bind
  warp to the current focus step, with no skipped-transport replay analogue
  inferred and no native-tested replay behavior claimed.

| Variant | Minimal action sequence | Candidate outcome |
| --- | --- | --- |
| Warp focus supersession | Cursor on L, focus A on L; focus B on R (warp W1 minted), skip issuing reply; incorporate a focus decision for C on L, before C is observed foreground; consume latest reply | C targets another lifetime: W1 cancels (`warpRequest:null`). A newer decision for B's lifetime keeps W1; once C is observed foreground, override retirement clears W1 |

### Q2 (resolved): pointer-motion cancellation

- Decision: add the hardware-origin `pointerActivity` generation to reports and
  freeze `pointerBaseline` in `WarpRequest`. A newer or unknown report
  generation cancels the request; own adapter warps must not advance the
  generation. No coordinate-only comparison is used; a `warp:true` host must
  prove native origin before relying on the witness, since raw `SetCursorPos`
  can surface as non-injected motion. `warp:false` hosts report no witness.
  Windows implementation: Raw Input is the witness, but Windows exposes no
  physical-versus-injected origin bit. The adapter's own `SetCursorPos` was
  proven not to advance it; injected input may, so it is conservatively
  treated as user motion. This is the recorded Windows deviation from the
  shared "hardware-origin" label.
- Evidence: `i3` (`4.25.1:src/x.c:1379-1382`) masks `ROOT_EVENT_MASK` around
  `xcb_warp_pointer` to suppress own-warp `MotionNotify`; this does not establish
  a Windows provenance route or delayed cancellation rule, and no
  skipped-transport analogue is inferred.

| Variant | Minimal action sequence | Candidate outcome |
| --- | --- | --- |
| Warp pointer-motion retirement | Cursor `(100,100)` on L, focus A on L; focus B on R (warp W1 to `(900,300)`), skip issuing reply; report B focused with pointer unmoved; user moves pointer `(100,100)->(200,200)->(100,100)` on L; consume latest reply | Newer/unknown `pointerActivity` retires W1 (`warpRequest:null`); a baseline-equal generation keeps it inert |

## Verification boundary

- Stage 1 covers core mint/retention, the strict `warpRequest` codec, and the
  `pointerActivity`/`warpSpent` report observations.
- Stage 2 covers the Windows exact-once adapter: spend-before-move by request
  id, already-there spend without a call, fail-closed gates (capability,
  exact foreground, fresh lifetime/scope/view, bounded point, pre-setter
  pointer generation), no retry on refusal/throw, and the ignored live probe
  at `fdead7d45e` proving `SetCursorPos` non-increment plus witness liveness.
- Still open: the full fixture-scoped live acceptance matrix (including a live
  positive warp through the real engine), same-generation reconnect accounting,
  and physical-versus-injected provenance (not exposed by Windows Raw Input).
