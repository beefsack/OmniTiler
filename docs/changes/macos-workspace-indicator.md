# macOS workspace indicator

## Goal and acceptance

Add a small, event-driven menu-bar indicator to the bounded macOS tiling preview.
Show native workspace numbers in native order, distinguish current and occupied
workspaces, and select existing workspaces through the existing guarded Mission
Control AX selector. Reuse retained off-current membership and layouts; do not
introduce a second workspace manager, idle polling, focus-taking UI, or overlays.

Acceptance: offline projection/config/lifecycle tests; live appearance/current,
Space-switch update, off-current occupancy, exact-hit-tested click selection and
teardown; verified restoration of the original one-Space desktop. macOS tests,
strict clippy, format, signed build/verify and hosted CI must pass. Physical
appearance, fullscreen/game behavior and unavailable capture remain explicit
acceptance limits rather than inferred results.

## Tentative approach

- One native status item, plain ordered workspace numbers by default; current
  highlighted and occupancy separately distinguished. All existing Spaces are
  shown, including native empty Spaces; no synthetic trailing workspace is added.
- Enable/configure the readout in preview settings. Offer current-only mode for
  space-constrained menu bars and optional compact representative application
  icons from retained membership without extra observation. Each number has its
  own direct native button; the all-workspace view is not hidden in a dropdown.
- Keep selection's existing single-display restriction. Multiple displays must
  refuse ambiguous selection rather than guess; rendering attribution must be
  explicit: zero or multiple displays hide the item. Numbers beyond 9 are visible
  but disabled because the current armed selector supports 1–9. Native fullscreen
  and games receive no extra window or input layer.
- Updates flow from existing observation/engine events to AppKit's main thread.
  Selection reuses fresh session/topology gates and verified-arrival history.

These choices are tentative pending user review. Alternatives: a separate status
item per workspace, occupied-only numbering like AeroSpace's ordered i3 style,
numeric SF Symbols instead of plain numbers, and an icon strip for only the
current workspace. Application icons must remain optional and compact.

## Bounded work and dependencies

1. Research AeroSpace, cloned bars, Windows/KDE semantics and macOS seams.
2. Implement projection, settings, native UI, guarded dispatch and offline tests.
3. Independently review native lifecycle/input and selection boundaries.
4. Run signed, bounded live proofs; restore and verify all owned effects.
5. Verify final gates/CI, record differences and physical appearance procedure.

## Research checkpoint

AeroSpace refreshes its tray model with layout events; its ordered i3 style
shows nonempty-or-visible workspaces and dims inactive entries. Its experimental
style menu is persisted in UserDefaults, not TOML. No SketchyBar clone was found.
Windows has per-output taskbar cells with miniature windows/icons and a
focused-output tray menu; KDE relies on Plasma's pager rather than a bespoke
strip. Both other adapters have fuller workspace lifecycles than macOS's
select-only preview. Existing objc2 AppKit bindings can host a status item without
a new crate. The exact occupancy and application-icon projection, unsafe baseline
and multi-display rendering rule were resolved by implementation inspection.

## Implementation and review checkpoint

The indicator uses retained engine-domain membership, including hidden domains,
with revision-fenced unknown fallback. Optional icons use a deterministic member
application (focused member preferred); missing artwork leaves the number intact.
Settings are `workspace_indicator.mode` (`all`/`current`/`off`) and
`workspace_indicator.icons` (false by default), with CLI overrides.

The initial dropdown-only rendering and mutable click-identity approach failed
review. Their correction renders native per-number buttons and carries immutable
SID, UI generation and topology revision through click admission and native drain.
Offline regressions cover pre-click rebuild and post-admission reorder refusals.
The independent global occupancy scanner was removed from production; a read-only
diagnostic scanner is explicitly not retained-state evidence. Live appearance,
selection and teardown still require a bounded fixture and independent review.

Current styling: bordered/bold current number; occupied bold at full alpha; empty
regular and dimmed; unknown has an explicit `?` suffix and tooltip.
Application icons are optional alongside, never instead of, numbers. These
appearance choices remain tentative pending physical review.

## Live checkpoint

The first signed live proof stopped at retained occupancy before rendering or
synthetic input: the scoped observer published zero admitted windows and one
classifier omission for the owned helper throughout the bounded wait. A separate
main-thread inspection classified an equivalently launched helper as tileable;
the exclusion mechanism is not yet established. No click, Space-switch update,
appearance or teardown acceptance is claimed.

Cleanup independently read back the original one-Space/current layout and removed
the temporary Space; helper, runtime and display-sleep companion stopped. The
protected-window snapshot comparison failed despite equal counts, so exact
protected-state restoration is not accepted until that discrepancy is explained.
Further live mutation is blocked on read-only diagnosis of the discrepancy and
the worker-path exclusion. Capture and physical appearance remain pending.

### Blocker diagnosis and continuation

The protected-window snapshots were already sorted, so ordering does not explain
the mismatch. The failed fixture compared the full records, including transient
`onscreen`, immediately after Mission Control cleanup, unlike the established
settled frame/membership gate. Unfortunately its failure JSON retained only counts,
not the before/after records. A transient visibility/readback difference and a
real protected-state change cannot be distinguished retrospectively. Do not claim
exact restoration or repeat the live proof on this evidence.

The classifier's omission reason was also absent from the retained logs. A
behavior-preserving observability addition now carries seven bounded exclusion
tokens from the scoped backend through the runtime; aggregate counts and snapshot
admission remain unchanged. Offline regressions cover unknown-subrole exclusion
and count consistency. No speculative classifier eligibility change was made.

Final local evidence: **842 tests** (708 library plus all integration suites),
strict all-target macOS clippy and workspace format pass. Signed app rebuild and
separate verification pass after the logging-only addition; final offline audit
found no additional serious issue. Main artifact SHA-256:
`ae9899dd34981789ae2fbc0aee01825f12f3b4f5073e2c1a64e6fe52d2521d9f`.
Unsafe blocks are 111 → 114 and unsafe impls
7 → 8, confined to `ffi.rs`; no new crate or shared-core change. No hosted CI or
publication claim is made for the uncommitted indicator.

Next: obtain read-only protected-state reverification and a decision on the
unrecoverable historical baseline gap. Preserve raw vectors in any revised
fixture and use the established bounded exact frame/membership settlement gate;
do not drop protected identities or loosen geometry tolerances. Only then name
the actual scoped exclusion with the new tokens, repair the proven cause, and
repeat the full indicator proof. Screenshot grant changes remain out of scope.

### User quick physical check (2026-10-17)

In an unscoped dogfood run on a single-Space desktop the indicator appeared and
appeared to work, showing a single `1`. No trailing empty workspace is shown,
as expected: trailing lifecycle is not implemented on macOS. Click-to-select,
Space-switch updates and teardown remain unaccepted.

## Explicit platform differences and remaining limits

- One native status item, not Windows' per-output external taskbar overlay or
  Plasma's native pager; no miniature window geometry, urgency or settings/tray
  controls. Numbers and optional representative app icons occupy menu-bar space.
- All existing native user Spaces, including empties, appear in native order;
  unlike KDE/Windows, no automatic minimum/trailing lifecycle or cleanup is armed.
  Retained managed-window occupancy is not a global application census; unknown
  membership is visibly `?`, not empty. Off-current domains remain retained.
- One display only; zero/multiple displays hide rather than choose menu-bar,
  focused or pointer display. KDE/Windows have per-output workspace presentation.
- Existing selectors 1–9 only; higher numbers are shown but disabled. Clicks use
  visible MC AX transitions, not the other adapters' host workspace switches.
  Sends, migration and trailing creation remain outside the select-only preview.
- Icons default off, one deterministic representative app per workspace rather
  than Windows' per-window miniature artwork. Missing artwork preserves numbers;
  refresh follows existing observations, not Windows' timed artwork refresh.
- Preview-lifetime native UI/settings, not Windows' persistent settings/hot-apply
  surface or Plasma configuration. `all`, `current`, `off` and icon CLI/file
  overrides are the current configuration surface.
- No new overlay or input hook; native menu-bar/fullscreen behavior, game safety,
  crowding, icon recognizability and physical focus/appearance remain unaccepted.

Both new source modules exceed the production soft size cap (the proof is roughly
3,000 lines); split by responsibility before treating the diagnostic as settled
infrastructure. This is a maintainability follow-up, not a reason to relax the
blocked live evidence gates.
