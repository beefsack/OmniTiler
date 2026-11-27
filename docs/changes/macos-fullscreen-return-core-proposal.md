# Proposal: return to a remembered native fullscreen member on macOS

**Status:** revised after the 2026-11-27 rejection, pending external re-review and host evidence. No wire, core, or return-route change is enabled by this proposal.

## Problem

Selecting a logical workspace should return to its remembered fullscreen member when that member still owns a separate native fullscreen Space. macOS exposes two distinct physical destinations: the user Desktop that anchors the logical workspace, and the disposable fullscreen Space that presently covers it. OmniTiler must switch to the latter for this explicit command, prove exact arrival, and leave focus, frame, and fullscreen state to the host. A fullscreen Space must not become a workspace, ring entry, or write-authorizing domain.

## Present representation and gap

- [`remembered_view_focus`](../../crates/tiler-core/src/world_tiling_views.rs) resolves focus from logical homing and last-active/layout history. `retain_view` can publish the exact lifetime as `FocusRequest.target`, with `cmd_seq` and `after_shown.op` identifying its parent select. [`host_owned_focus_bearing_fullscreen_focus_freezes_frameless_and_proves`](../../crates/tiler-core/src/world_selection_await_tests.rs) shows that this works for a fullscreen member reported on the **same native binding** as its logical home. It does not prove return to a separate unbound fullscreen Space.
- On macOS [`build_report`](../../crates/tiler-macos/src/world_builder.rs) emits a grounded fullscreen window with `fullscreen: true` and `NativeWorkspaceFact::Omitted` when it cannot home the window to a user Space. Physical membership is read by the adapter, not published as user membership. Core may retain the logical home; the exact `focus_request.target` is a useful selection subject, not a physical destination or an arrival receipt.
- [`AfterShown`](../../crates/tiler-core/src/world_report.rs) freezes logical view/member obligations. [`selection_native_proof` and `prove_selection_arrival`](../../crates/tiler-core/src/world_selection.rs) require a native current echo or explicit attempt that proves the logical target through its native binding, and native member proof against that binding. [`settle_views_pre`](../../crates/tiler-core/src/world_settle.rs) also settles against that logical/native binding. An exact post-read of the fullscreen SID therefore cannot prove this selection through today's v4 fields. Reporting the underlying Desktop SID as physically current or giving the fullscreen SID a second logical binding would change the meaning of those facts and could enable effects on the covered Desktop.

## Review and known limitation

The review rejected a defaulted capability and a singular top-level destination under the closed v4 contract, a singleton scene model that excludes Split View, and a separate settlement terminal. The replacement uses the existing per-output flight and proof engine, and distinguishes a detached surface from its logical home.

Selecting the current logical workspace while fullscreen still mints a host-owned View flight. The macOS owner mechanically refuses it before posting because the detached SID is not in the user Desktop order; it does not pull the user out of fullscreen on the current route. A core-side no-op belongs to this redesign. Any interim switch-away route must preserve that refusal.

## Proposed v4 delta

Amend the closed v4 schema in place. `HostCapabilities.detachedFullscreenView` is a **required boolean**, valid as `true` only for host-owned workspace domains. No omission, default or compatibility shape; update codecs and active fixtures together, preserving historical fixtures. KDE and Windows explicitly declare `false`, with no behavior change. See [wire conventions](../porting/world-contract.md#wire-conventions-and-bounds) and [contract authority](../decisions/contract.md#contract-authority-and-v4-migration).

| Contract location | Change |
|---|---|
| `NativeObservation` | Required nullable detached-current surface ref, separate from the user Desktop current and never part of `order` or occupancy. A positive detached-current fact leaves user Desktop current null. |
| `WindowFact` | Required nullable detached surface ref, valid with `fullscreen: true`. More than one live lifetime can share the same surface on the same output; membership proves neither a logical home nor a focused member. |
| View `TransferTarget` | Freeze a nullable typed native destination override on the existing per-output flight. Select only a detached surface whose exact remembered lifetime and logical home agree with that output and target workspace, and only for the explicit command. Shared fan-out can carry distinct overrides for each output; existing `RetainedTransfer` supersession and correlation apply. |
| `ViewAttempt.{pre,post,base}` | Use a closed native surface variant for user Desktop or detached fullscreen, with exact output and opaque native identity. The corresponding native arrival bracket can name the same surface. Neither fact rebinds a workspace. |

Use one settlement engine: `selection_native_proof`, `prove_selection_arrival` and `settle_views_pre` compare against each flight's frozen physical destination when overridden, otherwise continue to use the Desktop binding. Explicit return freezes the exact member independently of its physical surface; covered Desktop members need not become visible. Require a fresh physical current and matching correlated post with grounded member arrival; do not treat report silence as a move or success. Before a grant, missing, closed, ambiguous or changed membership leaves the ordinary Desktop path. After a grant, drift or unreadable proof never retargets or replays the operation.

## Core rules

- A redundant select while its detached surface is current is a no-op: do not replace the current surface with the covered Desktop or mint a switch or focus effect.
- [`adopt_idle_native_echoes`](../../crates/tiler-core/src/world_native.rs) may adopt a positively observed external switch onto a detached surface as the retained logical home's view only when that home is unambiguous. Keep the native Desktop current unknown rather than claiming it is visible. A Split View conflict between logical homes does not pick an arbitrary one.
- Advance [`logical_focus_cursor`](../../crates/tiler-core/src/world_close_focus.rs) to the fullscreen target from proven host foreground, without a focus setter. A surface with two members does not prove which member has focus.
- [`remembered_view_focus`](../../crates/tiler-core/src/world_tiling_views.rs) needs a fallback over grounded fullscreen members of the same logical home, including windows born fullscreen with no tile leaf and lifetimes no longer recorded in `last_active_window`. Reuse last-active/MRU precedence; break otherwise equal candidates deterministically without acquiring write authority.
- Never add a detached surface to a workspace ring, publish the covered Desktop as physically current, or grant a frame, show-state, focus, raise, or membership setter because of the override.

## Alternatives

- **Reuse `focus_request.target` with adapter-only switching:** adequate to find an exact candidate, but not to settle a separate native scene through the current proof contract. Returning to the Desktop would satisfy the existing binding but not the requested fullscreen return.
- **Report the user Desktop as current or map the fullscreen SID onto the logical home:** rejected; it invents physical currency, conflates transient and workspace identity, and risks granting writes to the covered Desktop.
- **Treat fullscreen as an ordinary user Space:** rejected; changes ordinal/history/creation semantics and gives the separate host scene a logical identity it does not own.
- **Only add a fullscreen SID to `after_shown`:** insufficient: a reply hint alone cannot prove operation-specific physical arrival or reconcile subsequent outside switches; default frozen members of the covered Desktop need not be visible.

## Review and tests

Test strict rejection of missing capability keys and invalid facts, capability-false round trips, competing members on one surface, per-output fan-out and supersession, redundant no-op, external adoption, and born-fullscreen focus fallback. Cover exact current/post/arrival agreement, stale or superseded proof, and no replay. Prove the settled logical view never implies the covered Desktop is current or authorizes a write.

## Host evidence required

Run signed SIP-on round trips `owned fullscreen → owned Desktop → same fullscreen` for the Dock gesture and pinned Mission Control AX separately. Record unchanged member identity, no fullscreen-window writes, the same detached SID while away and after return, other-display and pointer stability, and clean restoration. Capture the Space notification, exact SLS readback, and exact foreground observations on one monotonic clock; absence of a notification is not arrival proof. Split View membership and type are unverified: the signed fixture exposes individual fullscreen modes but no native Split View affordance.

**Probe status:** the first signed run observed the exact fixture window as the focused fullscreen AX window and as the sole member of a new SLS fullscreen Space. Neither route was measured: the pre-dispatch sentinel proof could not name the window via `AXWindows`, and the same proof refused explicit exit. Teardown tried ordinary restoration before the detached SID retired. A subsequent read-only inspection found the starting native currents and order restored and Mission Control closed. This proves a distinct detached SID on entry, not its lifetime while away, round-trip behavior, or notification ordering. Split View remains unprobed.

The user reported switching apps after a black teardown screen that showed only the Dock. The evidence-first archive recorded the seven visibility differences under the explicit user drift waiver; it did not restore or touch those windows. A final read-only recheck found exact starting topology, no owned fixture lifetime or Space, and no blocking drift before the receipt was moved. Closing the still-fullscreen fixture window is a suspected cause of the black screen, not a measured host mechanism.

The fixture requests exact windowed mode and waits for SLS to show the detached SID retired before closing the window. If retirement is not proved by the deadline, teardown requires a verified return to the saved starting current before allowing fixture shutdown. The previous signed attempt supplied only the body of the designated requirement to `--signer`; it stopped at signature bind after disposable-Space setup and never entered fullscreen. Pass the complete `designated => …` line from fixture signature verification, as one argument. The controller cleared that attempt's receipt without new residue; no owned processes remained.

The corrected signed SIP-on run reached fullscreen. Pinned Mission Control switched away: exact SLS Desktop current was sampled at 952,876 µs on the probe clock (dispatch started at 1,124 µs); neither a Space notification nor exact Desktop-sibling foreground was sampled. The fixture app's AX focus still named the fullscreen member, not the Desktop sibling, so the return source guard refused before dispatch. The bounded harness repair explicitly focuses only the owned, windowed Desktop sibling **after** measuring away, then re-proves the exact source before dispatching return. No product guard changed.

The signed retry completed both round trips with the same fixture lifetime and detached SID present while away and after both returns. All four legs held exact SLS arrival without bounce, preserved the other display's current, and issued no fullscreen member writes during switching. The only fullscreen member state requests were entry and teardown. The following are first **samples**, in µs from the retry's common monotonic clock. A dash means not sampled, not that the host never emitted a notification.

| Route and direction | Dispatch start | Space notification | Exact SLS current | Exact foreground |
|---|---:|---:|---:|---:|
| Pinned Mission Control → Desktop | 1,323 | — | 810,908 | — |
| Pinned Mission Control → fullscreen | 3,933,392 | 4,657,459 | 4,696,186 | 4,702,120 |
| Dock gesture → Desktop | 5,135,013 | 5,347,558 | 5,371,603 | 5,389,578 |
| Dock gesture → fullscreen | 6,342,059 | 6,650,051 | 6,688,100 | 6,639,194 |

The windowed-sibling setup fell between the away and return observations (Mission Control: 3,293,295–3,932,081 µs; Dock: 5,798,988–6,340,867 µs). The Dock return foreground sample preceded exact SLS current; it could not by itself prove physical arrival. This proves host switching on the tested layout, not unassisted Desktop focus for Mission Control or a general notification order.

These were the first live exercises of fullscreen-before-shutdown. Both runs retired the detached SID, restored the original currents and ordered topology, and left no owned fixture lifetime or Space and no pending receipt. The retry had only benign offscreen replacements in the controller comparison, with no new residue. Independent native rechecks confirmed restoration; every readable retry pointer sample matched the starting location. Exact Desktop currents and no remaining detached SID rule out a detached Space left current; pixels were not sampled, so visual absence of a black screen is unproven. Split View remains unprobed; the fixture has no native Split View affordance.

Independently, the current product owner refuses a detached current before dispatch; its Dock gesture source fence and pinned Mission Control fallback still require ordinary user Spaces. These product refusals do not establish what the host mechanisms can do.

The conservative contract still applies: keep temporary read gaps distinguishable from known absence, require bracketed physical readback around the exact foreground observation, and treat changed SID as supersession even for the same lifetime. The two round trips do not establish that the host never rekeys a fullscreen surface or guarantees event order.
