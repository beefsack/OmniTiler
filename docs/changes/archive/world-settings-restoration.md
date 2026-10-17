# Full-state gap and settings restoration

## Goal and scope

Restore the existing KDE settings contract after the full-state migration,
including visible outer gaps and deliberate settings reloads. Keep the
stateful Engine, core-owned workspace policy and existing admission rules.
Windows/macOS adapter behavior and new settings are out of scope.

## Diagnosis and outcome

- The legacy planner decoder inset work-area bounds before Engine dispatch
  (`crates/tiler-protocol/src/planner_protocol.rs`). The world decoder correctly
  carried raw bounds and a separate outer gap, but world dispatch and publication
  used those raw bounds directly. Inner gaps worked; edge gaps disappeared.
- `world_engine.rs` now insets each domain once before Engine dispatch;
  `world_publish.rs` projects the same inset. Command, directional, send,
  migration and drag routes use the same convention on both domains.
- Existing sessions adopt changed bounds/gaps through `UpdateGaps`, not the
  domain-mismatch-rejecting reconcile path. Reload preserves retained topology;
  repeating a report does not inset again. Exhausted bounds produce complete,
  recoverable `constraint-clamp` holds rather than losing windows or topology.
- Implementation and regressions: `9fe0e79b`.

## Settings audit

| Setting/path | Full-state outcome |
| --- | --- |
| `innerGap`, `outerGap` | Restored live reads, edge insets and topology-preserving updates. |
| `sameAxisMove` | Replaced hardcoded behavior with the validated live selection on move commands. |
| Resize mode/repeat | Preserved requested mode; restored zero-based repeat progression and structural/focus reset, excluding geometry-only changes. |
| `bornMaximized` | Restored tile-on-launch versus keep-overlay admission. Clear happens at most once per native lifetime, only after a ready reply proves tiled authority. Floating, fixed and unknown/held scopes remain untouched. |
| Held born-fullscreen exit | Preserved the existing clear-on-maximized-exit lane under both born modes; consume the first exit so later manual maximizes are not births. Never write fullscreen state. |
| `migrationSourceRefill` | Added validated `refill` ingress; core selects last-remaining versus MRU and publishes a stable paired view target. Settings changes affect subsequent migrations, not committed state. |
| Migration refill application | Match the exact parent operation; wait for all member arrival proofs before source refill/follow, retaining ambiguous operations without blind replay. |
| `shortcutProfile` | Restored validated startup consumption and restart-required drift reporting; shared directional catalog and existing bindings remain unchanged. |
| `workspaceMode` | Restored startup-only adoption with restart-required drift reporting. |
| `defaultTiled` | Already carried as the core's new-workspace seed; retained workspace choices are not reset. |
| `fixedSizePredicate`, tile overrides | Already carried through observation and core admission; retained classifications are not rebuilt. Born admission honors the current fixed flags. |
| Border, group and drag-preview visuals | Not dropped Engine options: native effect configuration remains in `kwin/native-effect/activeborderconfig.kcfg` and `ActiveWindowBorderEffect::reconfigure`; core-derived geometry still feeds the renderer. |
| Enable lifecycle | No `enabled` script setting exists in `main.xml`; startup adapter enablement is not a missing settings field. |

The world request's required `refill` field changes its strict wire shape;
Planner and script must be rebuilt/restarted together. Shared fixtures,
validation, digest accounting and bounded-size accounting were updated together.
Explicit deferred exception admissions retain exact flag binding; only the
ordinary maximized-only tiled carrier receives the narrow admission allowance.
The existing maximized-unfloat refusal remains intact.

## Accepted offline evidence

- The new semantic regression failed against pre-fix production code:
  `[0,0,396,600]` instead of `[12,12,384,576]`; restored code passes.
- `kwin/tests/world-outer-gap-regression.test.ts` byte-gates a real producer
  report, with QML array-like collections, renewed wrappers, braced synthetic
  IDs and generic Qt size hints. Rust decodes that fixture through WorldEngine;
  the shared reply is applied through the TypeScript native host.
- On an 800x600 work area, inner 8 / outer 12 yields
  `[12,12,384,576]` and `[404,12,384,576]`: all four outer edges and the
  8-pixel inner gap are asserted independently of golden comparison.
- Core regressions cover actual gap value changes, repeated reports, zero gaps,
  commands after changes, rebind, float/overlay authority, inset exhaustion and
  recovery. Settings regressions cover both choices, lifetime/rebind safety,
  zero pre-ready native writes, fixed/floating scope safety and partial/delayed
  migration without premature view changes.
- Final integrated checks after updating the branch: `cargo test --workspace`
  passed 2,888 tests; KWin `npm test` passed 1,769 tests. KWin typecheck,
  `cargo fmt --all -- --check`, workspace all-target clippy with warnings denied,
  `just check-portable` and diff whitespace checks passed.
- Validation was offline. Native visual behavior and physical shortcut delivery
  remain user re-checks, not claimed live evidence.

## User re-check

1. Stop an existing development session with Ctrl+C or `just dev-off`, then run
   `just dev` (or detached `just dev-on`). `just reload` alone does not replace
   the script and is insufficient for this wire/settings update.
2. With two ordinary, nonmaximized tiled windows, set inner gap to 8 and outer
   gap to 12. Check 12 pixels against all work-area edges and 8 between tiles.
3. Change the gaps to inner 16 / outer 20 and apply. Check updated spacing
   without restart, then move/resize and switch away/back: gaps must persist
   without accumulating. Restore the preferred gap values.
4. For settings parity, check both same-axis move choices, repeated resize,
   and a newly launched maximized window under each born-maximized choice.
   With multiple outputs, check each migration source-refill choice. Workspace
   mode and shortcut-profile changes require a full development restart.
