# KDE aligned per-output workspaces

## Goal and boundaries

- KDE keeps one global desktop list (count, order, positions) with a
  per-output current desktop (`perOutputVirtualDesktops`). The current
  `per-output-local` mapping partitions native desktops into disjoint
  per-output sets, so project numbering disagrees with KDE Overview/Pager and
  a member hidden on another output's backing desktop is unreachable by
  keyboard navigation.
- Replace the disjoint mapping with aligned numbering: logical workspace N on
  every output is native desktop N. Core stays host-agnostic; KDE projects
  logical workspaces onto native desktops.
- Windows/macOS behavior is unchanged; their adoption is evaluated later.

## Dogfood finding (2026-10-18)

- Two outputs side by side, `per-output-local`, four native desktops: left
  ring `[1,4]`, right ring `[2,3]`. A right-output window on native 1 was
  outside the right ring and unreachable by Meta+1..9 or relative switching.
- Investigation: rings are intentional partitioning (shown currents claimed
  primary first, remaining natives to the primary, creation appends
  globally): `crates/tiler-core/src/world_publish.rs:1174-1256`,
  `crates/tiler-core/src/workspace_scope.rs:107-143`,
  `kwin/src/world-apply.ts:1026-1037`. Startup conversion examines only each
  output's current desktop, so a hidden member on a foreign-owned native is
  observed and tiled but excluded from its output's ring
  (`world_publish.rs:1330-1340,1392-1394,982-997`;
  `kwin/src/world-observe.ts:580-652`; `workspace_scope.rs:302-334`). The
  session log could not prove the exact history.
- KWin v6.7.5 (`ab7df7cc`): global desktop list and renumbering on removal
  (`src/virtualdesktops.cpp:448-601`); membership and output are separate
  (`src/window.cpp:219-236,702-750`); interactive drag and send-to-output
  replace source-current membership with destination-current
  (`src/window.cpp:1074-1099,3879-3922,4670-4699`); non-interactive output
  changes, including output-disable evacuation, do not adjust membership
  (`src/window.cpp:219-227,4672-4678`).

## User decisions 2026-10-18

1. Aligned model for KDE `per-output-local`: Meta+1..9 and previous/next
   address the full aligned ring on every output (an output whose own
   occupancy needs fewer cells still selects the higher, empty cells);
   selection still creates nothing.
2. Aligned ring length is an internal core option enabled by a platform,
   off by default (each output owns an independent workspace set). Not a user
   setting. KDE enables it.
3. Each output has its own trailing empty: the first empty cell after its
   last occupied cell. Meta+0 and trailing-target sends use it, not the last
   ring cell.
4. Remove `global-unique` on KDE; core keeps it for other platforms. KDE
   cannot present per-output or globally unique numbers because desktop
   positions and names are global. `shared` is unchanged.
5. Minimum is two workspaces in total, including the trailing empty (keeps
   KDE Overview available). Correct the "two plus trailing" wording in
   [decisions](../decisions.md#workspaces).
6. Holes: an empty cell stays while another output occupies that native.
   A native is removed when empty on every output, not a trailing empty, not
   shown, and not protected (flight-pinned, displaced, unmapped). KDE
   renumbers later desktops natively; no membership moves for compaction.
7. Whole-workspace migration in aligned mode places the moved workspace in
   the destination output's trailing empty; nothing is inserted and no other
   output renumbers. The logical workspace keeps tree, order/shares,
   remembered focus and tiling mode; only its native desktop changes.
   Supersedes D4 "insert after destination current" for aligned mode only.
   D1-D3 and D5-D9 stand.
8. Hotplug keeps the no-merge rule: record each window's origin logical
   workspace before native evacuation, place displaced workspaces in the
   survivor's trailing empty, return them on reconnect. Existing focus,
   then-current-content and no-individual-pullback rules stand. Implement
   with care for complexity and visible jank; stop and report for a pivot
   if it does not work well.
9. Startup adopts the native desktop/output grid without splitting a shared
   current or moving windows to establish ownership. Supersedes the
   2026-10-18 per-output-local startup conversion and one-ring-per-native
   rule. Exact creation attribution, friendly-name non-identity and
   incomplete-report protection remain.

Tentative (user review pending): an existing KDE `global-unique` setting falls
back to `per-output-local` with a logged reason.

- **Tentative (user review pending), 2026-10-19:** aligned migration applies D5
  to logical workspace identity: the moved source logical is excluded from the
  remaining source ring, including its newly empty replacement cell. Choose
  the previous surviving source cell for MRU, otherwise the last remaining
  cell; preserve an unrelated still-valid source current. Native IDs remain
  positional bindings, not proof that the moved logical still belongs there.
- Floating workspaces carry their workspace-bound contents and floating mode;
  absence of a retained tiled tree does not authorize a migration refusal.
  Sticky members stay on the source, as required by D7.

## Approach

1. Core: separate logical workspace identity from native binding; add the
   aligned-length option, per-output trailing marker, full-length rings and
   all-output removal eligibility. Existing paths assume the same native at
   migration destination (`crates/tiler-core/src/world_commands.rs:368-402`)
   and move native-ID lists for displacement
   (`crates/tiler-core/src/world_displacement.rs:105-118,190-293`); the KDE
   binding cache holds one output binding per native
   (`kwin/src/world-entry.ts:935-949`).
2. KDE: enable aligned mode in `per-output-local`, identity projection,
   startup adoption, `global-unique` removal, startup-conversion removal.
3. Whole-workspace migration into the destination trailing empty.
4. Hotplug: live probe of native disconnect event order first, then no-merge
   displacement/return; pivot checkpoint.
5. Live two-output acceptance: dogfood scenario, holes and removal, native
   drag and send-to-output, migration, hotplug where safe.
   - Migration regressions: real world_eval plus production apply, with fresh replies.
   - Cover replicated inventory, delayed/partial/unreadable arrival, follow/refill
     and member closure in both directions. Assert logical identity across native
     rebinding, trailing-empty landing, no premature settlement, replay or feedback loop.
6. Records: discriminator rows in
   [reference outcomes](../spec/reference-outcomes/workspaces.md) (hidden
   foreign startup member, unequal extent/padding, hole versus compaction,
   native output move, migration collision, hotplug collision/return,
   minimum/lifecycle) with unsupported outcomes TBD; mark superseded
   decisions; queue Windows/macOS adoption follow-ups.

## Current state

- Implemented 2026-10-19: default-off core option, enabled by KDE;
  per-output logical/native bindings, full aligned rings, per-output trailing,
  globally guarded retirement and minimum-two growth, startup grid adoption,
  and KDE `global-unique` removal/fallback. Shared mode and other platform
  defaults are unchanged.
- Whole-workspace migration rebinds the logical unit onto the destination
  trailing cell, carries tree/shares/focus/mode and workspace-bound exceptions,
  snapshots D5 refill before exact-cell history invalidation, and retains
  member/both-view proof across delayed and incomplete observations. Source
  and destination cells are flight-pinned; reservations prevent double booking.
- KDE observes native per-window output/desktop changes. Only the actuated
  window's synchronous safe homing echoes defer to one post-apply observation;
  closure, unsafe readback and other windows still invalidate immediately.
  Paired migration view switches share one fresh entry proof and independent
  exact-output readbacks, avoiding a source-refill stall on the target's signal.
- No deletion is safe in the shared core disjoint/startup helpers: the default
  independent-set model still calls them. KDE aligned mode bypasses them;
  legacy plan routes are unchanged and their larger removal remains separate.
- Disjoint-model T05 is superseded. This record remains active for parked
  hotplug and the remaining native journey evidence, not archived as complete.

## Accepted evidence (2026-10-19)

- Latest integrated gates: Rust workspace **3335 passed**, zero failures or
  ignored tests; KWin **1919/1919**, zero failures; typecheck/build, Rust fmt,
  strict all-target workspace Clippy, native settings **27/27** and native
  effect **34/34** pass.
- Core grid/migration regressions cover replicated inventory, padding,
  independent trailing, global hole/removal/minimum rules, startup adoption,
  exact creation binding, reservation conflicts, floating/minimized carry,
  retained topology under delayed/partial/unreadable arrival, close at either
  end, external supersession and logical-history invalidation.
- Real `world_eval` plus production entry/apply regressions use fresh replies:
  migration out/back, native membership echo, paired follow/refill, member
  closure in both directions, unreadable/stale rounds, empty migration and
  logical rebinding without replay/feedback loops. Native homing regressions
  cover output moves both ways, same-output desktop changes and teardown.
- Native two-output evidence: hidden member on the other output's native 1
  becomes reachable by numbered selection; unequal per-output trailing targets
  select independently; holes survive occupancy elsewhere; globally empty,
  unshown natives retire and the minimum of two holds. Differing pre-existing
  currents and memberships were adopted without startup conversion moves.
- Native migration round trip: **2/2** trailing landings, explicit core source
  refills, mover focus retained, and subsequent replies with no transfers.
  Complete correlated raw JSON plus native state establish settlement. The
  earlier one-view/refill stall reproduced offline before its correction.
- After correcting the missing per-window subscriptions, native KWin
  send-to-output **2/2** changed membership to destination-current and triggered
  automatic fresh observations, correct retiling and trailing lifecycle, with
  no project migration/send command. The earlier blind spot is not acceptance.
- Protected window frame/state/membership/focus, output configuration, original
  desktop count/names/views, shortcuts, effects and float-intent bytes were
  restored. Empty native IDs/default-tiling groups underwent lifecycle churn.
  The first run lacked a full pre-run configuration copy, so its configuration
  byte delta is not fully attributable; later runs retained full copies and
  only desktop-ID/default-tiling churn remained.

## Remaining native evidence

- Physical shortcuts, perceived latency and pointer drag are not established
  by callback invocation. Drag question: after a normal window is dragged
  across each output boundary, does membership follow destination-current and
  does the controller observe/reconcile it without an extra migration?
- An earlier OmniTiler directional-send trial changed membership without a
  proven output arrival and lacked structural transport capture. A subsequent
  [complete own-send trace](kde-dual-output-live.md#current-continuation-2026-10-19)
  proves output/membership arrival, follow and quiet settlement both ways on
  the aligned tree; the earlier anomaly did not recur. A separate discarded
  stay-focus result was found and corrected, then source-MRU focus passed both
  ways. The historical trial's precise cause remains unproven.
- Native shared-current startup, physical drag and hotplug remain unverified;
  shared-current startup is covered offline. A fresh automatic-fixed newcomer
  and a post-close empty scope produced held fixtures during investigation;
  their independent causes are not established by this change.

### Hotplug checkpoint (2026-10-19)

- Native disconnect probing stopped before disabling an output: Plasma surfaces
  on that output would be evacuated, and exact restoration of their placement
  was not established. Output configuration alone is insufficient restoration
  proof. The read-only inventory probe was unloaded and baseline verified.
- At pinned KWin `ab7df7cc`, output evacuation preserves desktop membership;
  per-window `outputChanged(oldOutput)` precedes workspace `screensChanged`
  (`src/workspace.cpp:2339-2346,1485-1490`; `src/window.cpp:219-227`). This is
  source evidence, not live event-order confirmation.
- Hotplug no-merge/return implementation and acceptance are parked rather than
  claiming the aligned path supports them. Question: can the disconnect probe
  run after affected Plasma surfaces are moved away, or should it use isolated
  virtual outputs? Origin retention must precede logical-cell pruning; the
  current aligned path does not preserve disconnected origins for return.

## Exact next action

- Resolve the hotplug probe placement question above, then implement retained
  origins/trailing displacement/return before claiming no-merge support.
  The own-send trace is complete; physical drag remains in the
  [dual-output continuation](kde-dual-output-live.md#current-continuation-2026-10-19).
