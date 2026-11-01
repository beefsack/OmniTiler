# Gaming

### Anti-cheat mechanism limits
- Decision: interact with other processes only through host window-management mechanisms; never inject code, access process memory or send synthetic input.
- Why: anti-cheat false positives put user game accounts at risk.
- Assumes / revisit if: these mechanisms are those anti-cheat systems flag; revisit after anti-cheat research. Current code audit against this list remains outstanding.
- Status: active, 2026-10-30
- Evidence: [Gaming Compatibility](../principles.md#gaming-compatibility), [interface review](../changes/core-interface-review.md).

### Fullscreen-state based protection
- Decision: Fullscreen windows (exclusive and borderless-windowed fullscreen, games included) remain workspace members; explicit user workspace actions apply. The tiler never tiles, resizes, focuses, overlays or intercepts input for a fullscreen window unasked. A windowed game is an ordinary window.
- Why: protect fullscreen state and avoid unasked interference without treating game identity itself as a safety state.
- Rejected: blanket treatment of every game as protected regardless of fullscreen state.
- Assumes / revisit if: host fullscreen facts distinguish fullscreen from windowed state; existing fullscreen safety gates remain authoritative.
- Status: active, 2026-11-01
- Evidence: [Gaming Compatibility](../principles.md#gaming-compatibility), [R-MAX-07](../spec/reference-outcomes/maximize-fullscreen.md#r-max-07-captionless-full-monitor-cover), [R-OUT-11](../spec/reference-outcomes/multi-output.md#r-out-11-directional-focus-between-fullscreen-or-maximized-outputs).

### Xbox-mode pause and release gate
- Decision: If a documented Microsoft Xbox-mode signal becomes available, pause tiling, window effects and shortcut handling while remembering windows/workspaces for restoration. Do not use cloak/foreground heuristics; automatic pause remains unimplemented without signal. Alternate Game Bar access over fullscreen game is a later experiment after parity/correctness. Windows release gate after multi-output includes Win+G/F11 containment review, Xbox detection and M16 100ms pump-cost measurement; P0 label does not schedule it now.
- Why: not recorded.
- Assumes / revisit if: the fullscreen-state rule and its uncertainty fallback remain in force; alternate keyboard/mouse access remains later work.
- Status: active, 2026-10-10
- Evidence: [gaming coexistence](../changes/windows-gaming-coexistence.md).
