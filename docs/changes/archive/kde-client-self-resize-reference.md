# KDE tiled client self-resize reference audit

## Goal and scope

- Complete the existing client-driven geometry discriminator with Tier 1
  source evidence at the matrix pins; explain current KDE observation and
  reconciliation. No product behavior change.
- Separate client size changes from hint changes and interactive drags.
  Preserve unsupported native outcomes as TBD; do not amend decisions.

## Acceptance and approach

- Check existing rows and Fixed-Size Admission D2 before recommending behavior.
- Trace X11 request rejection separately from Wayland commit/ack handling in
  i3, sway, Hyprland and COSMIC; cite pinned source paths and line ranges.
- Trace KDE subscriptions through current world-v2 frame publication/apply;
  record the Windows event-path contrast only where source-supported.
- Complete the existing matrix discriminator, then park the correction contract
  in the existing backlog item with a recommendation and one alternative.
- Verify documentation-only scope, ASCII additions and `git diff --check`.

## Findings and decision boundary

- D2 already requires reacting to self-resize to avoid gaps/overlaps; it does
  not specify event timing or retries. R-SPC-08 covers hint-driven admission,
  not the missing native resize discriminator.
- X11 Tier 1 paths reject tiled size requests immediately; Wayland commit
  paths do not establish a common immediate corrective-configure policy.
- KDE observes no ordinary frame signal. A later report retains allocation
  authority and uses existing frame suppression, not unconditional retries.
- Desired correction timing and persistent-refusal behavior remain parked;
  evidence is recorded in the matrix, not a new normative decision.

## Verification

- Reviewed the completed protocol-specific matrix rows and pinned handler
  evidence; unsupported rendering, buffer/ack and native outcomes stay TBD.
- Current KDE world-v2 publication/apply was checked separately from the
  historical DescribePlan reconciliation record. Windows location-change
  wake support is source evidence only, not native acceptance.
- Only the matrix, existing backlog item and this record changed; no product
  source or decisions changed. Documentation additions are ASCII.
- `git diff --check`: passed. No new runtime results are claimed.

## Outcome

- [Matrix evidence and parked alternatives](../../spec/reference-outcomes.md#client-driven-tiled-geometry)
  replace the previously unsupported F1 reference cells. The backlog now
  points to that evidence rather than requesting another reference audit.
- Next: choose the correction timing/contract before repair; D2's existing
  anti-gap direction remains unchanged.
