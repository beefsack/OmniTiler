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

Tentative (not yet reviewed): an existing KDE `global-unique` setting falls
back to `per-output-local` with a logged reason.

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

- Planned; no implementation. Whole-workspace migration (T05) on the
  disjoint model is paused because step 3 supersedes it.
