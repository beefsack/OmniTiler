# Admission and space-exhaustion reference research

## Goal and scope

Establish source-evidenced Tier 1 admission anchors, geometric exhaustion,
rebalancing, burst ordering, tiling enable and startup/restart behavior.
Record minimal discriminators in the functional outcome matrix and link
requirements without selecting new product behavior. Tier 2 evidence is
supporting only; unsupported outcomes remain TBD. No runtime or live testing.

## Acceptance

- Reuse R-INS-01/04 and R-START-01..03 rather than duplicate their fixtures.
- Distinguish client minimum infeasibility from exhausted tile geometry.
- Cite commit-pinned source keys for COSMIC, Hyprland/Dwindle, i3 and sway.
- Record current OmniTiler behavior from source, separately from selected intent.
- Explain the capacity-blocker alternatives and conflicts with existing choices.
- Preserve clean-layout startup adoption and all existing product decisions.

## Approach and bounded units

1. Trace COSMIC and Hyprland admission, allocation, burst and enable paths.
2. Trace i3 and sway equivalents, including pixel floors and startup adoption.
3. Verify current OmniTiler paths; integrate minimal matrix/REQ additions.
4. Check citations, scenario coverage, counts and documentation diff.

## Evidence and outcome

- Added R-MIN-04 (exhausted focused anchor with room elsewhere) and R-INS-09
  (serial maps versus whole-roster delivery), with linked OPEN requirements.
  Reused ordinary-anchor and startup fixtures; distinguished per-window
  unfloat, algorithm switches, cold start, saved restart and config reload.
- Tier 1 sources admit without capacity-based alternate-target search.
  COSMIC/Dwindle wrap the anchor; i3/sway renormalize its existing parent,
  not ancestors. i3 floors client/frame dimensions to one pixel; sway
  reduces gap budgets, then zeroes sub-10px tiles. COSMIC/Dwindle may form
  degenerate post-gap targets; visible client settlement remains TBD.
- Ordinary maps are serial. COSMIC enable remaps front-to-back with frozen
  focus history, unlike ordinary newcomer-focus progression. The other
  Tier 1 profiles have no workspace-wide tiling toggle. Hyprland algorithm
  switches refeed stored tiled-target order; floating members stay floating.
- Verified current core fit-first adoption, spatial seed order with focus
  last, and projection refusal when gaps plus positive child pixels cannot
  fit. The motivating 13/14 report is not a universal numeric cap.
- Existing clean-layout preservation is intentional and not COSMIC parity.
  Changing only an exhausted fallback need not remove preservation, but a
  new seed topology or alternate-target policy requires revisiting the
  selected sequential seed/no-search choices. B6 minimum actuation alone
  does not address pure geometric exhaustion. No choice was changed here.

## Verification

- Independent source review accepted the new discriminators and current-core
  trace; citation ranges tightened following review.
- Consistency checks passed: 158 scenarios; 168 unique requirement rows
  (138 NORMATIVE, 30 OPEN); all 14 profiles in each new scenario; new
  evidence keys and scenario links resolve. No new duplicate source keys.
- Scoped `git diff --check` passed. Source-only evidence; no runtime or
  native test claims. Existing product choices remain unchanged.
