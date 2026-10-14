# Windows Whole-Workspace Output Migration And Source Refill

## Goal And Scope

- Deliver Windows handoff items 14/19 offline: four unbound follow-only
  active-workspace migration actions, largest-shared-FULL-edge selection,
  retained Engine domain rekey, source refill and D8 fullscreen/maximized carry.
- Add schema-v1 `core.migration_source_refill`, missing=shared default, exact
  token validation, settings UI Apply/Revert and live use on subsequent migrations.
- Local project-owned per-output workspaces only; item 12 non-local runtime
  modes remain a dependency. Physical two-output journeys remain user-owned.
- Reuse item 5 full-modifier routing and commit-last transfer fences. No
  unrelated admission, game/preimage, core/KDE or native-policy changes.

## Acceptance And Approach

1. Add config/UI/catalog routes and stable-ID workspace reassignment with
   pre-mutation per-output history snapshots, scoped invalidation and removal
   of only the moved hotplug-return association.
2. Plan on a candidate Engine using `MigrateWorkspace`, preserving tree,
   shares, remembered focus, tiling mode and member classes/origins. Insert
   after target current, show migrated and select the configured source refill.
3. Fence pinned topology, independent current views, member identities,
   lifetimes and live overlay state at every native boundary. Verify all
   arrivals and both views before publication/follow; partial writes converge
   through observation without replay or guessed rollback.
4. Carry workspace floats, exclude source sticky clients, preserve minimized
   state; native overlay transfer suppresses extra overlay geometry and
   fullscreen focus writes. Replace maximized placeholders with observations.
5. Cover real-Engine synthetic multi-output journeys, stale/partial failures,
   selection ties, MRU eligibility, empty migration, unbound rebinding and
   settings default/invalid/live behavior. Independently review boundaries.
6. Run native locked four-package build/test, format, strict all-target clippy
   and whitespace gates; update dated Windows spec/matrix/backlog evidence
   and archive the completed record.

## Decisions And Dependencies

- Existing user decisions D1-D9 and G-37 apply unchanged; no new product
  decision is required for native maximized observation or default bindings.
- Item 12 remains required for non-local runtime modes. Windows capability
  comes from the existing separable per-output project workspace owner, not
  a fabricated KWin capability flag or registry setting.
- Fullscreen/game migration is an explicit user action under D8; native
  two-output overlay settlement remains unverified until user testing.

## Evidence

- 2026-10-14 scope audit: Windows migration command/config routes are absent;
  shared migration/selection/refill contracts and item 5 foundation exist.
  Baseline delivery recorded 1,892 passed, zero failed, one ignored.

## Outcome And Accepted Evidence

- Delivered offline 2026-10-14, base `807ed97`: all four unbound follow actions,
  modifier-aware queue routing, shared largest-edge selection, stable-ID project
  reassignment and retained tiled Engine `MigrateWorkspace` rekey. Source refill
  uses the current validated setting and pre-mutation per-output history;
  migrated previous IDs clear and only the moved hotplug-return ID is removed.
- Local independent per-output views supply capability. Untiled workspaces
  retain mode and use move-only remaps; their Engine session is normally absent
  after release, and a stale unreleased session refuses before transfer.
  Item 12 remains the non-local-mode dependency.
- Tiled plain members use planned geometry; floats/fullscreen use nonactivating
  move-only remaps. Maximized and concealed members use placement remaps,
  preserving normal restore size and show state. Sticky members stay source.
  Live overlay reads exclude overlay geometry and fullscreen focus setters.
  Ledger-backed target-prior hide and source-refill reveal precede publication.
- Arrival uses fresh visible frames or normal placements, not pre-move caches.
  Placement conversion preserves virtual-desktop origins and accounts for
  work-minus-full taskbar offsets; tool-window placements remain screen-based.
  Verified membership/views publish project and Engine candidates together;
  uncertain effects reconcile observations without transfer replay or rollback.
- Boundary review exposed missing overlay/concealed movement and view effects,
  stale arrivals, outer-versus-visible comparisons and restore-position errors.
  Corrections add real native-transfer seams, fresh arrivals, per-effect fences,
  preserved normal geometry and fresh fullscreen follow guards. Coordinate
  regressions cover unequal taskbar insets, positive/negative and stacked outputs.
- Synthetic real-Engine coverage includes nested unequal shares/remembered
  focus, floats, sticky exclusion, empty migration, default/MRU eligibility,
  scoped history/return siblings, topology/identity drift and failed arrivals.
  Settings tests cover additive schema-v1 defaults, invalid tokens, UI draft
  Apply/Revert and unbound catalog/rebinding in both presets.
- Final native offline gates: rustc 1.99.0, `x86_64-pc-windows-msvc`; locked
  four-package build/test, workspace format check, strict all-target clippy and
  whitespace check pass. **1,950 passed, zero failed, one ignored** (+58 Windows
  tests): core 615, protocol 190, KWin FFI 40, Windows 1,105 (707 library,
  398 integration; one library benchmark ignored). Formatting-only correction
  followed the semantic gates and the format/whitespace checks were repeated.
- Shared core/protocol/KDE source is unchanged. The upstream core/protocol
  comparison contained no newer commits at verification.

## Pending Physical Journeys

- Two-output user testing: occupied prior target and source refill visibility,
  all directions/rebinding, empty/sticky-active and minimized/hidden members,
  automatic/intentional floats, retained tree/shares/focus and reconnect siblings.
- Fullscreen/maximized carry: actual target monitor, native re-fit/focus timing,
  restored normal rectangle and reserved slot; mixed DPI/taskbar insets, related
  transients and scoped recovery after partial writes. Offline seams establish
  adapter behavior, not native settlement or physical acceptance.
