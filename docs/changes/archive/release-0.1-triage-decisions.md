# Release 0.1 Triage Decisions And Reference Tiers

## Goal And Scope

- Record the user's 2026-10-12 reference tiers and D01, D03, D09, D10,
  D12, D13, D16, D22, D23 and D27 selections in decisions, triage, spec, outcome cells and backlog.
- Preserve historical proposals and untested outcomes; update the Windows
  handoff and existing shortcut-dialog parity item.
- Documentation only; D01's KDE fullscreen focus fence remains code work,
  D16 needs shared-core/KDE float transfer after native roundtrip verification,
  D23 needs shared-core/KDE minimize storage/restore and null-focus handling,
  and D27 requires a native KWin trace before implementation assessment.

## Acceptance And Approach

- Promote fourteen selected requirement rows to NORMATIVE with decision links.
- Retain three pending units (D25, D26, D28) plus Proposal B /
  N1-N4; limit the P2 settings list to approved D09, D10 and D27.
- Reconcile current summaries and indexes, verify changed local links and
  anchors, and run `git diff --check`.

## Material Decisions

- [Reference tiers](../../decisions.md#reference-tiers) replace the earlier default
  rule; prior decisions stand unless revisited.
- [Admission](../../decisions.md#placement-minimums-and-startup-adoption) preserves
  native activation ownership, including explicit switching and host denial.
- [Move defaults](../../decisions.md#move-layout-and-output-commands) retain only
  the two-option D09/D10 post-0.1 settings.
- [D27 activation](../../decisions.md#activation-requests), approved option (a):
  Mark urgent only by default, no workspace/focus change; Hyprland/i3/sway
  Tier-1 agreement reverses tentative Switch and focus. Gaming guard is
  urgency-only in both modes; explicit Alt+Tab/taskbar selection is separate,
  and INS-07 no-steal remains scoped to inactive-workspace tiling admission.
- Post-0.1 `Activation request` setting: `Mark urgent only` (default; tooltip
  Hyprland, i3, sway) / `Switch and focus` (tooltip COSMIC, xmonad). COSMIC
  Tier 1 plus xmonad Tier 2 qualifies the alternative; functional names/options,
  WM names tooltips only. P2 retains D09/D10/D27.
- D27 KDE trace-first condition: native Wayland xdg-activation and Xwayland
  under default focus-stealing prevention. Attention-only is verification-only;
  native workspace/focus switching makes an urgency-only override conflict
  with D01, requiring a return to the user before code. Otherwise request/urgency
  handling needs code per the gap row; native outcomes stay TBD.
- [D16 floating transfer](../../decisions.md#workspaces), approved option (a):
  retain float and size on arrival, destination tiles untouched, for follow/stay
  and numbered/relative sends. Hyprland/i3/sway Tier 1 plus xmonad/qtile/awesome
  Tier 2 retain float against COSMIC fresh re-tile. No setting: the alternative
  has no second Tier 1/2 reference.
- D16 needs code: shared core `NotTiled` and KDE non-tiled send refusals remain
  gaps. First verify untested native WS1 -> WS2 -> WS1 roundtrip, then shared
  core first (serves Windows), then KDE adapter; Windows adapter work goes to
  item 22. Reconcile earlier intentional-float ineligibility as a D16 gap,
  retaining the separate floating-workspace boundary journey and sticky refusal.
- [D22 float-out reflow](../../decisions.md#window-state-float-sticky-maximize-fullscreen),
  approved option (a): surviving siblings keep relative proportions (50/25/25,
  float middle 25 -> approximately 67/33). No setting. COSMIC (observed, not
  source-proven), i3 and sway (Tier 1) agree; no evidenced alternative;
  Hyprland/Tier 2 untraced. Thin basis disclosed: i3/sway one family plus
  separate COSMIC observation.
- D22 is verification-only: shared removal retains survivor share ratios per
  the revision-bound gap assessment. Native R-FLT-03 reflow live check remains
  pending; selection establishes no native evidence.
- [D23 minimize/restore](../../decisions.md#window-state-float-sticky-maximize-fullscreen),
  approved option (a): minimize releases allocation, siblings reflow, MRU focus,
  stored old slot; restore to stored slot with no OmniTiler focus write (host
  handles restore focus); sole minimize retains workspace, focus none, stored
  slot. No setting. COSMIC Tier 1 source-proven default; qtile/awesome Tier 2
  vacate, awesome restores old slot; Hyprland/i3/sway no counterpart or
  untraced non-voting; no evidenced alternative to allocation release.
- D23 needs shared-core-first storage/restore and sole-minimize null-focus
  handling, then KDE wiring. Windows follows later via item 22: retained
  allocation is divergence to change, not an approved exception. Native
  outcomes remain TBD; earlier restore "no focus steal" wording is superseded
  by no OmniTiler focus write with host focus left native.

## Evidence And Outcome

- Initial 2026-10-12 record promoted eight rows (129 NORMATIVE / 9 PROVISIONAL /
  28 OPEN). D27 approval promoted REQ-ACT-01 (130 NORMATIVE / 8 PROVISIONAL /
  28 OPEN). D16 approval promoted REQ-WS-05 (131 NORMATIVE / 7 PROVISIONAL /
  28 OPEN). D22 approval promoted REQ-FLT-03 (132 NORMATIVE / 6 PROVISIONAL /
  28 OPEN). D23 approval promotes REQ-MNZ-01/02/03, yielding verified current
  totals of 135 NORMATIVE / 3 PROVISIONAL / 28 OPEN (166 rows).
- Three pending units (D25, D26, D28) and Proposal B / N1-N4
  remain tentative. Historical options/tallies and unsupported reference
  outcomes are retained; D16/D22/D23/D27 tier rationale and user approval are recorded.
- Windows item 22 updated for approved defaults/settings and native foreground
  considerations; existing shortcut-dialog parity item updated to P2, with
  KDE user live review linked. No duplicate item added.
- D27 updated in decisions, triage, spec/counts/index, Ours target cells,
  backlog gap/settings and Windows item 22; no product code or live tests.
- D16 updated in decisions, triage, spec/counts/index, R-WS-05 Ours targets,
  backlog approved gap/implementation queue and Windows item 22. Existing
  R-WS-05 supplies the roundtrip discriminator; no product code or live tests.
- D22 updated in decisions, triage, spec/counts/index, R-FLT-03 Ours targets,
  backlog approved verification row/queue and Windows item 22. Existing
  R-FLT-03 supplies the ratio-vs-equalize discriminator; no product code or live tests.
- D23 updated in decisions, triage, spec/counts/index, R-MNZ-01..03 Ours
  targets, backlog approved gap/implementation queue and Windows item 22.
  Existing minimize/restore/sole-window scenarios discriminate the selected
  behavior; no product code or live tests.
- Requirement counts, changed local link targets/anchors, ASCII additions,
  unchanged foreign activation/workspace/floating/minimize outcomes and
  `git diff --check` passed. Native outcomes remain TBD; D01's fullscreen focus
  fence remains implementation work.
