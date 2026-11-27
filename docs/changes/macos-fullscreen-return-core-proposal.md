# Proposal: return to a remembered native fullscreen member on macOS

**Status:** for independent review. No wire, core, or return-route change is enabled by this proposal.

## Problem

Selecting a logical workspace should return to its remembered fullscreen member when that member still owns a separate native fullscreen Space. macOS exposes two distinct physical destinations: the user Desktop that anchors the logical workspace, and the disposable fullscreen Space that presently covers it. OmniTiler must switch to the latter for this explicit command, prove exact arrival, and leave focus, frame, and fullscreen state to the host. A fullscreen Space must not become a workspace, ring entry, or write-authorizing domain.

## Present representation and gap

- [`remembered_view_focus`](../../crates/tiler-core/src/world_tiling_views.rs) resolves focus from logical homing and last-active/layout history. `retain_view` can publish the exact lifetime as `FocusRequest.target`, with `cmd_seq` and `after_shown.op` identifying its parent select. [`host_owned_focus_bearing_fullscreen_focus_freezes_frameless_and_proves`](../../crates/tiler-core/src/world_selection_await_tests.rs) shows that this works for a fullscreen member reported on the **same native binding** as its logical home. It does not prove return to a separate unbound fullscreen Space.
- On macOS [`build_report`](../../crates/tiler-macos/src/world_builder.rs) emits a grounded fullscreen window with `fullscreen: true` and `NativeWorkspaceFact::Omitted` when it cannot home the window to a user Space. Physical membership is read by the adapter, not published as user membership. Core may retain the logical home; the exact `focus_request.target` is a useful selection subject, not a physical destination or an arrival receipt.
- [`AfterShown`](../../crates/tiler-core/src/world_report.rs) freezes logical view/member obligations. [`selection_native_proof` and `prove_selection_arrival`](../../crates/tiler-core/src/world_selection.rs) require a native current echo or explicit attempt that proves the logical target through its native binding, and native member proof against that binding. [`settle_views_pre`](../../crates/tiler-core/src/world_settle.rs) also settles against that logical/native binding. An exact post-read of the fullscreen SID therefore cannot prove this selection through today's v4 fields. Reporting the underlying Desktop SID as physically current or giving the fullscreen SID a second logical binding would change the meaning of those facts and could enable effects on the covered Desktop.

## Proposed v4 delta

Keep the existing `FocusRequest`, `AfterShown`, `WorkspaceRef`, and user-Desktop bindings. Negotiate one optional `hostFullscreenView` capability (default false; valid only with host-owned workspace domains). The additions below are capability-gated, read-only host facts and a core-published, one-operation destination; omitted without the capability. Wire names are proposed:

| Wire field | Typed value and meaning |
|---|---|
| Report `nativeFullscreenScenes[]` | `NativeFullscreenScene { out: OutputId, window: WindowId, native_scene: String, current: bool }`. Exact live lifetime, singleton native fullscreen membership, unique output join, and whether the physical scene is current; use the platform's normal opaque native identity encoding. A failed read omits the row; omission is not a close. The window's fullscreen fact must agree. This is neither a `WorkspaceRef` nor a logical-home assertion. |
| Reply `fullscreenViewTarget` (optional) | `FullscreenViewTarget { op: String, window: WindowId, out: OutputId, ws: String, native_scene: String }`. Core freezes it only for the same explicit select and its resolved remembered member, with retained logical home matching the selected `(out, ws)` and an exact scene fact on that output. This selects the scene and authorizes **no** focus, frame, show-state, raise, or membership call for that lifetime. Do not issue a separate focus activation for this target. |
| `ViewAttempt.fullscreenScene` (optional) | `FullscreenSceneAttempt { target: String, pre: Option<String>, post: Option<String> }` on the existing op/output/workspace-correlated attempt. `target` echoes the frozen scene; `pre`/`post` are exact physical current scene observations if fullscreen, otherwise null; ordinary Desktop pre/post still use `WorkspaceRef`. No adapter verdict or inferred logical arrival. |

The host-owned fullscreen selection terminal is separate from ordinary `after_shown` member activation: require both the correlated post and a fresh current scene with the same window/output/scene. The host foreground may provide affirmative focus proof without a setter. Do not require covered Desktop members to become visible, consume any unrelated activation, add the scene to a ring, or mark the underlying user Desktop current. Preserve logical view/history on proven settlement; a later known physical departure wins. An unreadable or drifted proof must not reinterpret the observed scene, retarget the frozen attempt, or replay. Missing, closed, ambiguous, off-output, or no-longer-fullscreen member **before** grant uses the ordinary Desktop route. After grant, changed proof is a mechanical refusal; never issue a second switch for that operation. KDE and Windows do not negotiate the capability: no change in their demand, proof, wire output, or default behavior.

## Alternatives

- **Reuse `focus_request.target` with adapter-only switching:** adequate to find an exact candidate, but not to settle a separate native scene through the current proof contract. Returning to the Desktop would satisfy the existing binding but not the requested fullscreen return.
- **Report the user Desktop as current or map the fullscreen SID onto the logical home:** rejected; it invents physical currency, conflates transient and workspace identity, and risks granting writes to the covered Desktop.
- **Treat fullscreen as an ordinary user Space:** rejected; changes ordinal/history/creation semantics and gives the separate host scene a logical identity it does not own.
- **Only add a fullscreen SID to `after_shown`:** insufficient: a reply hint alone cannot prove operation-specific physical arrival or reconcile subsequent outside switches; default frozen members of the covered Desktop need not be visible.

## Review and tests

Test report/reply/feedback round-trip and absent-capability behavior, validation of duplicate/unknown/off-output scene facts, same-op focused fullscreen target versus ordinary/unknown/closed/member-moved cases, post/readback agreement, stale or superseded proof, and no replay; prove the settled logical view does not imply a current user desktop or authorize any writes. Keep native-free selection regressions for KDE and Windows unchanged. After review and implementation, run signed SIP-on fixture `WS1-fullscreen → WS2 → WS1`, requiring the exact fullscreen SID and unchanged fullscreen member, zero unasked writes, unchanged other display and pointer, and clean restore.

Open questions: Should fullscreen scene observations be complete per output or explicitly tri-state to distinguish temporary read gaps from positive absence? What bounded observation sequence ties physical readback and optional foreground proof tightly enough to avoid terminal success after an outside switch? Does native fullscreen transition ever allocate another scene ID for the same lifetime while offscreen, requiring distinct supersession handling? These questions affect settlement and should be reviewed before any contract change.
