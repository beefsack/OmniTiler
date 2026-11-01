# Reference-WM outcome matrix

- Status: documentation delivered (2026-10-04); format discussion and new
  reference-WM testing remain user-owned.

## Goal and scope

- Start P1 Cross-platform functional specification with a compact, evidence-tagged
  outcome matrix for COSMIC, Hyprland, bspwm, i3, xmonad and ours (KDE/Windows).
- Carry forward recorded COSMIC user tests and decision-relevant ambiguities;
  use confident source evidence where cheap, otherwise explicit TBD cells.
- Documentation only. No live tests, implementation changes, backlog or
  principles edits. The format is provisional, to discuss.

## Acceptance and approach

- One Markdown matrix with stable IDs, precise start states, action sequences,
  observations, variants and per-WM configuration/version assumptions.
- Minimal non-overlapping rows; unresolved ambiguity creates a discriminating
  row for later user testing. Distinguish observed, sourced and intended behavior.
- Authors and independent validators validate the matrix; evidence reviewed
  and this record archived.
- ASCII, valid local links/anchors, whitespace checks and green
  hosted CI. Report row/evidence counts and five priority user tests.

## Material decisions

- Provisional, to discuss: one Markdown document with tables by area, stable IDs,
  six outcome columns and variant hooks. Compact citations distinguish dated
  user tests, pinned source and linked docs; unknown cells stay TBD. Recorded in
  [decisions](../../decisions/reference-and-shortcuts.md#reference-outcome-authority).
- Existing product choices are not reselected. Hooks index evidence for the
  future specification; foreign outcomes do not automatically become supported.

## Evidence and outcome

- Baseline: clean `main`, synchronized with `origin/main`.
- Delivered [matrix](../../spec/reference-outcomes.md): 27 scenarios in ten
  areas, including controlled workspace roundtrip, invalid-leaf fallback,
  floated roundtrip, clean/cascade/minimum-bound startup and retained-slot
  maximize. New fixtures are proposed inputs, not recovered historical tests.
- Areas: insertion 2, move 5, workspace 5, float/sticky 3, maximize/fullscreen 2,
  startup 3, close 2, groups 1, multi-output 2, mouse drag 2. Of 162 outcome
  cells, 5 contain user-test evidence, 19 source, 56 docs and 124 a TBD component
  (84 wholly TBD). Mixed cells count once in each applicable class, including
  linked corpus provenance; totals therefore overlap.
- Imported COSMIC user evidence uses exact original S1 transitions and
  2026-08-22 Tests B/C; authored corpus vectors retain docs provenance.
  Unknown tested versions/config are not retroactively pinned to local source.
- Independent validation rejected generic-dispatcher/enum evidence as proof of
  full outcomes, mismatched tested starts, invented ratio-memory claims and
  unselected labels for already-selected COSMIC behavior. Tightened starts,
  added source admission-axis evidence and the user's float/send sequence.
- Root AGENTS.md now requires an ambiguity to produce a minimal discriminating
  matrix row; the decisions entry is the backlog-adjacent discovery link.
- Verification: ASCII, local links/anchors, unique IDs, eleven-column row shape,
  evidence/variant keys and `git diff --check`. Hosted CI is checked against the
  pushed commit; the commit/run receipt reported with publication.
- No live testing or desktop mutation performed.

## Succession

- Priority user tests: R-WS-02 return anchor/axis; R-WS-04 invalid remembered
  leaf/history; R-WS-05 float/send roundtrip; R-START-03 minimum overflow policy;
  R-MAX-01 minimum-bound sibling stability and restore delay.
- Proposed backlog: "P1 | Cross-platform functional specification | Initial
  reference-WM outcome matrix delivered (27 scenarios); confirm provisional
  format, fill prioritized TBD outcomes, then specify explicitly selected
  user-configurable variants. Entry point: decisions, Cross-Platform Behavior."
- Exact next action: user confirms the format and records R-WS-02 outcomes with
  WM version/config, returned anchor, axis/order and focus. Advance
  the backlog using the proposed text.
