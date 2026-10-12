# Release 0.1 Triage Decisions And Reference Tiers

## Goal And Scope

- Record the user's 2026-10-12 reference tiers and D01, D03, D09, D10,
  D12, D13 and D27 selections in decisions, triage, spec, outcome cells and backlog.
- Preserve historical proposals and untested outcomes; update the Windows
  handoff and existing shortcut-dialog parity item.
- Documentation only; D01's KDE fullscreen focus fence remains code work,
  and D27 requires a native KWin trace before implementation assessment.

## Acceptance And Approach

- Promote nine selected requirement rows to NORMATIVE with decision links.
- Retain six pending units (D16, D22, D23, D25, D26, D28) plus Proposal B /
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

## Evidence And Outcome

- Initial 2026-10-12 record promoted eight rows (129 NORMATIVE / 9 PROVISIONAL /
  28 OPEN). D27 approval promotes REQ-ACT-01, yielding verified current totals
  of 130 NORMATIVE / 8 PROVISIONAL / 28 OPEN (166 rows).
- Six pending units (D16, D22, D23, D25, D26, D28) and Proposal B / N1-N4
  remain tentative. Historical options/tallies and unsupported reference
  outcomes are retained; D27 tier rescoring and user approval are recorded.
- Windows item 22 updated for approved defaults/settings and native foreground
  considerations; existing shortcut-dialog parity item updated to P2, with
  KDE user live review linked. No duplicate item added.
- D27 updated in decisions, triage, spec/counts/index, Ours target cells,
  backlog gap/settings and Windows item 22; no product code or live tests.
- Requirement counts, changed local link targets/anchors, ASCII additions,
  unchanged foreign R-ACT-01 outcomes and `git diff --check` passed. Native
  outcomes remain TBD; D01's fullscreen focus fence remains implementation work.
