# D27 activation request - KWin native trace

- Goal: decide whether default Mark urgent only needs code or is verification-only, without overriding KWin (D01 ownership).
- Scope: trace KWin 6.7.4 (ref 8438567a) + 6.6 fallback for: Wayland xdg-activation valid/stale/absent token; Xwayland _NET_ACTIVE_WINDOW app-source; hidden-desktop handling (desktop switch?); FSP levels None/Low/Medium/High/Extreme incl. demandsAttention; fullscreen/game-focused guard; whether OmniTiler convergence follows native desktop switch as user switch.
- Non-goals: post-0.1 Activation request setting; explicit Alt+Tab/taskbar changes; product code before branch decision.
- Acceptance: per-request-class trace with file:line citations at pinned ref; branch decision recorded; if attention-only then verification path, else BLOCKED research note + backlog update, no product code.
- Approach: Worker 1 traces KWin source via GitHub at pinned ref (+6.6 diff if different); Worker 2 (after) checks OmniTiler convergence paths.
- Units: (1) KWin trace; (2) OmniTiler convergence check; (3) docs update + commit/push.
- Verification: source citations; ASCII/link checks for docs-only; full gates only if code written.
- Decisions: D27 option (a) Mark urgent only approved 2026-10-12; D01 native activation ownership stands.
- Evidence: KWin trace at 8438567a + v6.6.3; OmniTiler convergence check at 15aa5e3. Full record in docs/research/d27-kwin-activation-trace.md.
- Outcome: BLOCKED, no product code. Valid-token hidden-desktop requests switch + focus natively at default Low; urgency-only would override KWin (D01 conflict). Backlog D27 row marked BLOCKED with options and recommendation (accept native as host exception).
