# KDE dual-output live testing and fixes

## Goal and acceptance

- Verify native two-output behavior with mixed scales and vertically offset
  outputs against the [coverage inventory](../research/two-output-test-environment.md#7-check-mapping-exact-triage-wording)
  and [multi-output backlog](../backlog.md#multi-output-setup).
- Record authoritative before/after state, distinguish passing subcases from
  incomplete gates, fix reproducible defects with regression coverage, and
  repeat affected live journeys.
- Preserve existing windows and restore settings, output configuration and
  development lifecycle state.

## Approach

- Establish owned native test clients, diagnostic capture and input delivery.
- Exercise static focus, transfer, workspace, geometry and overlay journeys.
- Exercise reversible topology changes and isolated process recovery.
- Prioritize defects by severity; retain unsupported product outcomes as TBD
  discriminating rows rather than inventing behavior.

## Must-pass checklist

Results below are from the first native pass. Partial coverage does not
complete the referenced inventory gate. Callback dispatch does not prove
physical shortcut delivery. Further live cases are blocked by the desktop
identity restoration failure described below.

| ID | Inventory | Journey | Outcome / evidence |
| --- | --- | --- | --- |
| T01 | D06, #46 | New native windows join the focused output; left/right focus crosses outputs without wrong-domain writes | PASS subcases: two newcomers on the first active output; local and cross-output focus both directions. BLOCKED remainder: newcomer on second active output and physical delivery |
| T02 | D13, #43, #46 | Directional move exhausts local structure then crosses; explicit output send-follow/stay, empty/occupied targets and floating boundaries | PASS subcases: local moves and explicit rightward sends to empty/occupied targets. BLOCKED remainder: exhausted crossing, leftward send, stay and floating boundaries |
| T03 | #14 | Per-output workspace switching/history, shared-scope refusal where required, preset-specific arrows and letter bindings | BLOCKED: not exercised before restoration failure |
| T04 | #15 | Relative workspace send follow/stay, ring ends, trailing-empty fill and sole-mover source emptying on both outputs | BLOCKED: not exercised before restoration failure |
| T05 | #42 | Whole-workspace migration preserves identity/order/tree/tiling state, occupied target and hidden workspace; overlay and float members | BLOCKED: not exercised; native per-output prerequisite still needs verification |
| T06 | #34, #44, N4 | Mixed-scale/origin geometry, gaps, border, underlay and preview; dynamic mode/scale/work-area changes and restoration | PASS subcase: destination tiles inside its offset work area with 8 logical-pixel gaps. BLOCKED remainder: overlays and dynamic changes |
| T07 | #1, #34 | Fullscreen/maximize on each output, carry/restore, reserved slot and no unsolicited fullscreen geometry/focus writes | BLOCKED: not exercised; real-game leg remains pending |
| T08 | #47 | Pointer drag across outputs to occupied/empty targets; preview/drop agreement, snap-back/cancel and source removal | BLOCKED: no verified pointer-input route; not exercised |
| T09 | #49 | Hidden/sticky domains quiet; immediate/delayed and rapid transfers reconcile without ping-pong or stale replay | BLOCKED: basic send convergence observed, specialized cases not exercised |
| T10 | #14, #51 | Disable/re-enable displacement/return and primary change; exact topology restoration; physical unplug remains a distinct leg | BLOCKED: no topology changes exercised |
| T11 | #14 | Output-local trailing empty workspace: fill, close/empty, switch, stable spare and independent other output | BLOCKED: startup cleanup observed, output-local anti-oscillation cases not exercised |
| T12 | #30, #50, N3 | Isolated Planner loss/recovery on two outputs; sleep/wake and native gaming remain separate legs | BLOCKED: graceful development teardown only; no loss/sleep/game evidence |
| T13 | #48 | KWin restart with surviving Planner and fresh-ID adoption | BLOCKED: restart not exercised |
| T14 | #2 | Wide source with two destination candidates | BLOCKED: requires a third output |
| T15 | #43 | Vertical output arrangement and work-area gap; up/down crossing and no-candidate refusal | BLOCKED: no vertical rearrangement exercised |

## Accepted live evidence

- Base revision: `e48d33a`; native clients were normal, resizable Wayland
  Konsole windows, verified by their platform selection and display socket.
  Window snapshots carried stable identities, output, focus and frame state.
- Two newcomers opened on the focused first output and took native focus.
  Second-output newcomer admission has not been tested.
- Left/right focus callbacks selected local peers and crossed the output
  boundary in both directions. Remembered target-leaf selection is implemented
  in `kwin/src/plan-adapter.ts:9675-9676`; these observations do not select a
  new nearest-window policy.
- Explicit output-right sends first placed one client alone on the destination,
  then placed the other beside it. Destination readback: two equal-share
  logical frames, with 8-pixel outer/inter-window gaps inside the
  panel-adjusted, vertically offset work area. Source snapshots showed both
  clients absent. Matching reconcile/send plans were applied.
- Seven local move callbacks produced applied plans and native layout changes;
  the fixture did not isolate an exhausted edge, so it does not prove D13/R4.
- Protected real windows survived every captured snapshot; no close or keyboard
  input was sent to them. Both owned clients were closed and their processes
  verified gone.

## Findings and parked restoration

- FAIL - live fixture preservation: controller startup retired a pre-existing
  backing desktop and appended a new spare. The desktop count remained four,
  but identity/order differed from the baseline after teardown. Two new
  native default-tiling groups were also persisted. They were left intact;
  no broad configuration rollback was attempted.
- Root cause: `kwin/src/workspace-native.ts:1869` adopts live shared desktops
  as managed; `ensureTrailingEmptyDesktop` at `:644-670` retires non-trailing
  invisible empties; `removeManagedEmptyShared` at `:3487-3529` performs native
  removal. The removal/create diagnostic pair was captured at startup.
  The fixture lacked per-desktop occupancy/visibility readback at that instant,
  so it cannot independently prove every removal guard.
- This mechanism is selected product behavior, not an established product
  defect: [workspace decisions](../decisions.md#workspaces)
  explicitly permit adopted managed empty retirement. Existing tests at
  `kwin/tests/workspace-native.test.ts:309-324,353-369` cover it. Changing
  cleanup to retain all pre-existing empties would change product intent.
- Corrected the stale R-WS-10 KDE source description that claimed the adapter
  only writes membership. No product behavior or new durable decision changed.
- Restoration options: accept the current desktop identities as a new baseline
  (recommended after reviewing contents), or restore the original configuration
  with KWin stopped at a session boundary. A live `createDesktop` call cannot
  recreate the original identity. Resolution is pending; do not treat the
  initial desktop baseline as restored.
- Requested/default versus effective workspace mode was not captured reliably;
  verify mode, native per-output capability, current view per output and all
  desktop memberships before another startup. Do not infer mode from output
  count or version.
- No product fix commit or tentative product decision from this partial pass.

## Verification and restoration

- Initial checkout clean; development controller/planner stopped.
- Initial output topology verified: two enabled outputs, mixed scale,
  vertically offset, distinct priorities.
- Development controller and Planner stopped through the owned teardown;
  packaged enablement restored. Original active real window refocused.
- Output mode/scale/position/priority/rotation unchanged and re-verified.
  Shortcut configuration byte-identical to baseline. Project enablement key
  restored; desktop identity/order and new tiling groups remain different.
- Native effect binary was not rebuilt or replaced; overlay rendering and
  loaded-binary freshness are not claimed by the geometry evidence.
- Diagnostic scripts unloaded; no owned test windows remain.
- Documentation diff whitespace check passed. No executable source changed;
  existing lifecycle regression coverage supports the policy explanation.

## Next action

- Resolve the desktop baseline restoration, then capture complete membership,
  visibility and effective mode before startup. Establish verified physical
  keyboard/pointer delivery, repeat T01 on the second output, and continue T02
  with an owned edge fixture before the workspace/topology checks.
