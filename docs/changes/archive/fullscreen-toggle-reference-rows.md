# Fullscreen float and maximize toggle reference rows

## Goal and scope

- Add minimal independent float/maximize-on-fullscreen discriminators, with
  pinned COSMIC, Hyprland, i3 and sway evidence and separate current Ours cells.
- Documentation/source research only; no product changes or live tests.
- Leave behavior selection open; preserve recorded refusals and D5 game safety.

## Acceptance and approach

1. Trace each native project verb at the existing matrix pins, including
   fullscreen cleanup, restored membership/slot and focus; mark unknowns TBD.
2. Add GWT rows, index/discriminator links and functional-spec open-question
   cross-references without creating normative requirements.
3. Check every Tier 1 citation, row/profile accounting, local links and
   `git diff --check`; archive this record with accepted evidence.

## Evidence and decisions

- Initial repository state clean. Sequences start independently from ordinary
  tiled A/B on one output; no app-owned fullscreen, games or intervening input.
- Cleanup must distinguish monitor fullscreen from work-area maximize: exit
  fullscreen only if the first toggle leaves monitor fullscreen active. Never
  blindly toggle again and accidentally enter fullscreen or clear maximize.
- Reference behavior and current refusal are evidence, not a new selection.
- Added [R-MAX-10/11](../../spec/reference-outcomes/maximize-fullscreen.md#r-max-10-float-toggle-on-fullscreen)
  with 14 profiles each and index/functional-spec OPEN cross-references.
- Pinned-source review: COSMIC `3d55cba0` rejects both toggles while the
  Fullscreen focus target is active, then restores B's saved tiled position.
  Hyprland `19fb395d` applies float with fullscreen clear/reapply, exiting to
  float; maximize instead replaces fullscreen with work-area MAXIMIZED.
  i3 `903bcd51` and sway `1652c54b` apply floating immediately beneath
  retained fullscreen and leave B floating on exit; neither has a maximize
  verb, so neither votes for refusal. Full pins and source ranges are in
  the matrix legend; each cited Tier 1 range was inspected directly.
- Monitor-fullscreen-only cleanup preserves Hyprland's resulting maximize.
  Dwindle inherits the base fullscreen handler even though Lua defaults to
  layout-aware requests; this does not select a different fullscreen model.
- Tier-rule implication, not a selection: float has all three other Tier 1
  references against COSMIC, suggesting accepted float intent with fullscreen
  retained until explicit exit; maximize has no consensus and leaves COSMIC
  as the tier default. Transient native state/configure changes are not
  proven game-safe; D5 and current Ours decisions remain unchanged.
- KDE's 2026-10-28 physical report is partial evidence only: chords appear
  dropped and fullscreen exit tiles; exact slot/share/focus unmeasured.
  Windows exact sequences, settled reference frames/timing, and all Tier 2/3
  combined sequences remain TBD. No runtime proof inferred from source.

## Verification and outcome

- Reviewed the documentation diff and all Tier 1 citation chains against the
  existing pins. Both added GWT rows have exactly 14 separate Then profiles;
  scenario accounting advances 158 -> 160, Then bullets 2212 -> 2240.
- New local link anchors and citation-key resolution checked; added spec
  lines are ASCII; `git diff --check` passed. No spec-specific executable
  documentation/link checker is prescribed by the justfiles or matrix
  verification records; their local-link/profile/citation checks were used.
- No requirement rows/statuses, product code or live behavior changed.
  Next: user decision on the two OPEN behavior questions and D5 tradeoff.
