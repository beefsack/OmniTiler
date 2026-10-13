# D27 activation request - KWin native trace

- Goal: decide whether default Mark urgent only needs code or is verification-only, without overriding KWin (D01 ownership).
- Scope: trace KWin 6.7.4 (ref 8438567a) + 6.6 fallback for: Wayland xdg-activation valid/stale/absent token; Xwayland _NET_ACTIVE_WINDOW app-source; hidden-desktop handling (desktop switch?); FSP levels None/Low/Medium/High/Extreme incl. demandsAttention; fullscreen/game-focused guard; whether OmniTiler convergence follows native desktop switch as user switch.
- Non-goals: post-0.1 Activation request setting; explicit Alt+Tab/taskbar changes; product code before branch decision.
- Acceptance: per-request-class trace with pinned file:line citations; native
  ownership decision recorded; verification path separated from implementation.
- Approach: trace pinned KWin source plus 6.6 comparison; check OmniTiler
  convergence; record the resolved decision and pending native checks.
- Verification: source citations; ASCII/link checks for docs-only; full gates only if code written.
- Decisions: User 2026-10-13 selected option 1, native KWin KDE host exception,
  no tiler guard. Valid-token/FromTool/allowed-X11 switches + focuses;
  missing/stale-token demand-attention. Never intercept/revert/suppress.
  Alternatives are complex hacks, maintenance burden and jank. Windows default
  Mark urgent only and guard unchanged; setting KDE applicability at design.
- Evidence: KWin trace at 8438567a + v6.6.3; OmniTiler convergence check at
  15aa5e3. [Full trace](../../research/d27-kwin-activation-trace.md),
  [decision](../../decisions.md#activation-requests).
- Outcome: decision resolved 2026-10-13, no product code. KDE D27 is
  verification-only, no longer BLOCKED. Trace work complete, archived; native
  valid-token convergence/stale-token non-interference and must-pass real-game
  active/idle checks remain in [backlog](../../backlog.md). Reopen only if real
  active-play switch-aways occur.
