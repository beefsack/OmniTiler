# AR6: logical workspace model in core - integration decision pending

- Historical proposal: the ownership inventory below describes the retired KDE
  domain route, not current production. World adoption superseded that route;
  `workspace-native.ts` now retains only shared free functions. See
  [world architecture](../decisions.md#engine-architecture-and-convergence) and
  [removal evidence](kde-legacy-route-removal.md).

## Goal and scope

Move the portable logical-workspace policy into dependency-free `tiler-core` while preserving the workspace modes, synchronous native behavior and Planner-loss behavior in `docs/decisions.md`. KWin retains `outputTuple`, observation, signal wiring, desktop/window references and all native reads/writes. Windows/macOS `Managed` capability implementations are out of scope. AR4/AR11 and the architecture review are untouched.

## Current ownership and behavior

- `kwin/src/workspace-native.ts` owns desktop ordering (`orderedDesktopEntries`, native-number ordering only when all numbers are available), output-key mapping, mode-specific local/global/shared workspace maps, session-local owned versus managed sets, the displaced-origin association and last-visible/last-known memberships. Shared selects the same backing desktop on all outputs; local and global-unique keep distinct assignments.
- The same module resolves existing 1-based logical selection and move targets without creating them; `0` reuses the current trailing empty or synchronously appends one. Global-unique selection may swap a desktop visible elsewhere. `ensureTrailingEmptyDesktop` currently invokes remove/create callbacks, and `enforce*Trailing` plus `removeManagedEmpty*` enforce global and per-domain minimum two, literal trailing retention, and protection for occupied, visible, transaction-retained, displaced, unmanaged/unmapped and non-final native desktops. Occupancy includes floats, fullscreen and maximized windows, but not sticky all-desktops windows. Native observation failure defers cleanup.
- `handleTopologySignal` synchronously reconciles disconnect/reconnect before cleanup. It displaces whole backing workspaces to a surviving output and returns them to the same output key with current contents; focus/view changes only if the active window belongs to the displaced/returning workspace. `chooseDisplacedDestination` supports nearest if removed-output geometry is supplied, but this adapter passes `null`, selecting KWin's `activeScreen` key if it survives, then native output order. `lastVisibleByKey` and `lastKnownByKey` preserve disconnected-output memberships without historical geometry.
- `kwin/src/plan-adapter-entry.ts` shortcut callbacks call `selectLogical`/`selectTrailingOrCreate` directly; topology/window signals call `handleTopologySignal` directly. Move callbacks synchronously resolve or append the target, then hand it to the asynchronous `WorkspaceSendAdapter`. The pending workspace provider protects a send's source/target after the plan is bound, through ack/verify; it returns no retention during pre-plan dispatch. These native selection and topology paths do not consult Planner, so desktop selection and hotplug still work when Planner is absent. Tiled workspace sends depend on the Planner; loss terminates their old flight and confirmed loss starts bounded recovery from current observations, without replay.

## Candidate core contract (design only)

Keep a typed `WorkspaceMap` alongside `Engine`, holding `WorkspaceMode`, ordered per-output/shared backing IDs, managed versus created ownership, displacement associations and last-visible/last-known IDs. Feed it complete adapter-normalized facts: live ordered `(id, optional native number)` entries, stable output keys and current desktop per output, occupancy (excluding sticky), visible IDs, transaction-pinned IDs, active-window membership, and optional handling-time removed-output geometry. Unknown/incomplete inputs must defer rather than remove or rehome.

Typed requests: observe/reconcile topology, select existing ordinal, select/reuse/append trailing, resolve existing move ordinal, resolve/reuse/append move target. Typed results: resolved ID or refusal plus ordered intents to create, remove or show a desktop and optionally retain exact active focus. Core would decide eligibility, mode mapping, destination and minimum counts; the adapter would translate opaque IDs to live native references, perform setters synchronously where required, report actual create/remove outcomes and re-observe before subsequent dependent intents. A create cannot invent its native ID; a failed native action cannot advance core mapping. `ensureTrailingEmptyDesktop` must return create/remove intents, never call native callbacks. The contract must preserve literal native order, at most one append per domain per maintenance pass, never remove unmapped desktops, and distinguish adopted-managed from lifetime-owned desktops on disable. These are proposed shapes, not a selected wire schema or implementation.

## Integration route and blocker

There is no in-process Rust execution path in the KWin script. The only existing core route is asynchronous Planner D-Bus: a new typed request would need owner/activation lookup and a Planner method response before native selection or cleanup. `Meta+1..9` currently performs zero D-Bus round trips and synchronously calls KWin's current-desktop setter; hotplug, trailing cleanup and `Meta+0` are also synchronous. Adding a round trip makes shortcuts/hotplug dependent on Planner availability, response latency and timeout, and would break the existing no-Planner desktop-switching behavior unless a second TS policy authority remained. The existing asynchronous send route does not remove that blocker for synchronous target resolution or native `0` creation.

**Stop before integration as requested.** Recommendation for the owner's decision: put the pure portable map in core with shared byte-exact golden test vectors and retain a temporary synchronous TS mirror for KWin, comparing both policies offline; this preserves current latency and Planner-loss operation while establishing a reusable core model. This alternative is not yet adopted. A Planner-only route would require an explicit product decision to accept altered latency/failure and a specified Planner-loss fallback; do not silently select it. With no removed-output geometry, destination uses KWin's `activeScreen` key as the surviving primary proxy, then existing output ordering, not the active window's output.

## Conditional migration slices and acceptance

If the mirror route is selected: (1) lock existing TS behavior with shared vectors for all three modes, ordering and move resolution; (2) port trailing create/remove decision and all protection/minimum rules to typed core results, with adapter native success/failure replay in vectors; (3) port disconnect/reconnect, survivor choice and focus/view preservation using current-membership facts; (4) compare Rust/TS on vectors and retain native synchronous shortcuts/signals and Planner-loss fallback; (5) only remove mirrored TS policy after a viable in-process or equivalent synchronous core route is available and reviewed. No wire changes are implied by the pure-core slice. If a D-Bus integration is later selected, design typed event/action wire and update protocol goldens before actuation.

Acceptance for an approved integration: regressions for each mode, desktop ordering, trailing create/remove, occupied/visible/pinned/displaced/unmapped protections and minimum two, displacement/return (including survivor selection and no focus theft), both move-target variants, and Planner-unavailable selection. Verify full Rust fmt/check/clippy/test, KWin typecheck/tests/build, `just check-portable`, changed wire goldens if any, approximate TS lines removed, and independent implementation review. Update `docs/decisions.md` only for a selected or shipped mechanism. Independent design review checked the current paths and confirmed the latency/failure blocker; three wording corrections about per-domain append, post-plan retention and `activeScreen` selection were incorporated. No implementation or offline test claims yet.

## Moved Evidence (from docs/decisions.md)

- All initial hotplug product choices are resolved; offline coverage exists for
  the stated displacement/return, atomicity, and preservation pieces and live
  disconnect/reconnect verification remains pending with no live result
  claimed.
