# Multi-output sole-close reference evidence

## Goal and scope

- Extend R-CLOSE-03 with a multi-output sole-close discriminator for the
  Tier 1 input tilers: COSMIC, Hyprland, i3 and sway.
- Record shown-workspace retention, replacement focus and current output
  when the other output is occupied; include the both-empty variant only
  where source evidence discriminates it cheaply.
- Documentation and pinned-source research only. No OmniTiler outcome is
  selected; unsupported reference outcomes remain TBD for later testing.

## Acceptance and approach

- Use locally cloned sources at the matrix's existing pins, with exact
  source citations and explicit cursor/focus-follows-mouse conditions.
- Add Given/When/Observe/Then evidence to the close matrix and reconcile
  its index and consensus summaries without changing unrelated outcomes.
- Verify the source paths, evidence chains, documentation diff and row
  accounting; no source changes or live testing.

## Outcome and verification

- Added a 14-profile multi-output leg within R-CLOSE-03; no new scenario
  ID or scenario-count change. Updated the index and consensus summary.
- COSMIC, Hyprland, i3 and sway all retain the shown empty workspace on
  L and do not select a window on R or another workspace on L. COSMIC
  clears keyboard focus (keyboard-focused output None, cursor output L);
  Hyprland clears window focus and retains monitor L; i3/sway focus the
  empty workspace node on L. Shipped pointer-focus defaults are recorded.
- bspwm also retains the shown empty desktop and monitor L while
  clearing X input to pointer-root. Other profiles and Ours remain TBD;
  no OmniTiler behavior selected. The optional both-empty variant was
  omitted; the occupied-R leg establishes the cross-output discriminator.
- Pinned sources and focus/output chains independently checked for all
  Tier 1 profiles, including candidate-search scope, empty-workspace
  retention and cursor-output fallback. Source ranges resolve through
  the matrix's evidence keys. Fourteen multi-output Then bullets retained;
  documentation diff checked with `git diff --check`.
- No source/test changes or live tests.
