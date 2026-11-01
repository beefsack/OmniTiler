# Activation requests

### Hidden-window activation default
- Decision: unsolicited activation request from existing window on hidden workspace defaults to `Mark urgent only`: mark urgent without switching workspace or changing focus where OmniTiler owns outcome (Windows). This reverses tentative `Switch and focus` default; Hyprland, i3, sway Tier 1 agree against COSMIC.
- Why: not recorded.
- Assumes / revisit if: no request-vs-selection classification choice is recorded; explicit user selection (Alt+Tab/taskbar) is separate. INS-07 no-steal applies to inactive-workspace tiling admission, not existing-window activation.
- Status: active, 2026-10-13
- Evidence: [approval](../research/release-0.1-triage.md#user-decisions-2026-10-12), [requirement](../spec/functional-spec.md#activation), [reference evidence](../spec/reference-outcomes/activation.md#r-act-01-hidden-window-sends-an-unsolicited-activation-request).

### Windows OS-permitted activation during fullscreen
- Decision: in `Switch and focus`, honor OS-permitted activation even while fullscreen window/game is foreground; no tiler gaming guard downgrades it to urgency-only. This supersedes earlier Windows guard-in-both-modes wording. `Mark urgent only` default and non-permitted requests unchanged; Windows only. KDE native exception/no-guard policy unchanged.
- Why: Windows itself permits activation; adding guard risks suppressing legitimate activation.
- Assumes / revisit if: consequence is workspace switch hides game's workspace, more than plain Windows; real-game check required and not established. Approved but not implemented; setting/request classifier not shipped. Setting's help must explain this behavior. Review KDE applicability when designed; no KDE setting decision.
- Status: active, 2026-10-16
- Evidence: [activation verification](../changes/archive/windows-d27-activation-verification.md), [R-ACT-01](../spec/reference-outcomes/activation.md#r-act-01-hidden-window-sends-an-unsolicited-activation-request).

### Urgency presentation and future setting
- Decision: hidden-window urgency marks workspace urgent in planned Windows indicator, cleared on visit; until then native Windows behavior only. Post-0.1 `Activation request` options: `Mark urgent only` default (tooltip Hyprland, i3, sway) and `Switch and focus` (tooltip COSMIC, xmonad). Use functional names/options; WM names only in tooltips. Retained in P2 settings follow-up. No new urgency UI is adopted here.
- Why: not recorded.
- Assumes / revisit if: setting implementation remains post-0.1; no existing shipped activation tooltip needs editing before implementation.
- Status: active, 2026-10-17
- Evidence: [activation requirement](../spec/functional-spec.md#activation), [reference evidence](../spec/reference-outcomes/activation.md#r-act-01-hidden-window-sends-an-unsolicited-activation-request).

### KDE native activation exception
- Decision: accept KWin native activation as KDE host exception. Valid-token / FromTool / allowed-X11 requests for hidden-desktop window switch desktop and focus natively; missing/stale-token requests are marked demand-attention by KWin. OmniTiler converges and never intercepts, reverts or suppresses KWin activation. KDE gaming guard is none; rely on KWin input-freshness gating (token serial/X11 user time). Requests during active play are expected mark-only; idle-game and pager/taskbar switches follow native behavior as without tiler.
- Why: alternatives are complex hacks, maintenance burden and add jank.
- Assumes / revisit if: KDE D27 is verification-only, no longer blocked; source trace complete but native outcomes TBD. Real-game must-pass covers active/idle/paused play; expectation is not evidence. Reopen only if real switch-away during active play observed.
- Status: active, 2026-10-13
- Evidence: [KWin D27 trace](../research/d27-kwin-activation-trace.md), [activation requirement](../spec/functional-spec.md#activation).
