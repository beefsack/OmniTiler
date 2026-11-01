# Cursor-first output-targeting evidence

## Goal and scope

- Capture pinned Tier 1 evidence for the 2026-10-20 output-targeting direction:
  COSMIC, Hyprland, i3 and sway.
- Separate pointer motion, active output, keyboard focus, launch context and map
  admission. Record workspace selection, focused-window sends, empty-desktop
  clicks, destination insertion, and optional focus/warp settings and defaults.
- Non-goals: change R-OUT-05 / REQ-OUT-05, select an override, implement behavior,
  or run live reference tests.

## Acceptance

- Minimal discriminating action sequences have per-profile outcomes and pinned
  citations; unproven outcomes remain TBD for later user testing.
- Matrix index, multi-output cells, functional-spec evidence pointers and
  consensus research agree without changing normative status.
- Recommendations distinguish source evidence from tentative product choices
  and identify clauses requiring user resolution.
- Documentation links, source citations, ASCII and whitespace are checked.

## Approach and bounded units

1. Trace COSMIC and Hyprland input, command, admission and setting paths at the
   existing reference pins, including pointer-only output changes.
2. Trace i3 and sway at the existing pins with the same discriminators.
3. Integrate evidence into minimal matrix scenarios and a clause-level research
   summary; retain historical consensus tallies as historical where necessary.
4. Review source-to-cell claims and verify the resulting documentation.

## Decisions and outcome

- The approved focused-output requirement remains in force. Research alone does
  not authorize an override. Unsupported native/launcher outcomes stay TBD.
- Evidence captured at COSMIC `3d55cba0`, Hyprland `19fb395d`, i3 `903bcd51`
  and sway `1652c54b`: pointer-only motion with hover focus disabled routes
  workspace selection and ordinary admission to R in COSMIC/Hyprland, but
  leaves them on focused L in i3/sway. Built-in shortcut launch contexts
  capture the same R/R/L/L destinations.
- COSMIC separates pointer-output switches from focused-output sends.
  All four send focused A, but Hyprland relative destinations resolve from
  the focus monitor (R), unlike focused-output-relative COSMIC/i3/sway.
- Other-output insertion is COSMIC target-MRU versus Hyprland cursor-closest;
  same-output anchoring agrees on focused A, not identical split topology.
- Focus-following defaults are COSMIC off versus all three others on. The
  tier-rule exception therefore opposes the requested default-off; cursor
  warping has distinct per-profile trigger scopes. These choices need review.
- Durable evidence: [R-OUT-05a/08/09/10](../../spec/reference-outcomes/multi-output.md#r-out-05a-motion-then-ordinary-open),
  [pinned source keys](../../spec/reference-outcomes.md#evidence-tags), and the
  [clause-level recommendations](../../research/reference-wm-consensus.md#cursor-first-output-targeting-supplement-2026-10-20).
- Verification: independent source-to-cell review accepted; 14 profile cells
  in each new row; original REQ-OUT-05 requirement text identical and status
  still NORMATIVE; relative Markdown links/new anchors, ASCII additions and
  `git diff --check` pass. No implementation or live-reference evidence added.
- Remaining unknowns are explicit in the matrix: settled focus/pixels,
  COSMIC direction/layout variants, external/reused launch contexts,
  non-Tier-1 and project cursor-first native outcomes.
- Outcome: research complete; user review must select any normative override,
  default deviations and the definition of another output's standard anchor.

## Backlog detail moved 2026-11-01

### Original backlog lines 378-398

````markdown
- P0 | Cross-platform cursor-first output targeting (User 2026-10-20) |
  Decided on [Tier 1 evidence](changes/archive/cursor-first-output-evidence.md):
  all-platform switch target cursor (default, COSMIC) or focused output (i3/sway);
  three-way new-window placement COSMIC (default: cursor output/last-focused
  window), Hyprland (cursor output/cursor-closest), i3/sway (focused
  output/last-focused). Sends use the focused window's output; relative
  next/previous targets count there too, no setting (Decision 1c). Focus-follows-
  cursor on/off and cursor-warp are settings, both default off (Decision 1d);
  i3/sway preset enables cross-output warp. Replaces R-OUT-05's old default.
  Shared-core placement where approved; platforms supply cursor/settings. Core
  switch/placement/send targeting and Windows delivered 2026-10-22
  (fixture-verified both outputs; rearchitecture closed 2026-10-24); active
  focus-follows-cursor and warp are stage 4 (default-off core/Windows delivered;
  native hover-follow unverified). KDE/macOS by their ports
  ([guide](porting/world-adapter-guide.md), [KDE](porting/kde-port-notes.md),
  [macOS](porting/macos-port-notes.md)).
  User-approved 2026-10-26: three-way placement subsumes launch targeting.
  Output-targeting race option A audited/implemented offline 2026-10-28:
  implicit callers already defer to core; execution-time cursor/focus/history
  regressions added, all-platform production unchanged. Explicit-send current
  sampling remains separate ([evidence](changes/archive/output-targeting-race-audit.md)).
````
