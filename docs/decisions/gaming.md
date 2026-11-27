# Gaming

### Anti-cheat mechanism limits
- Decision: interact with other processes only through host window-management mechanisms; never inject code or access process memory. Synthetic input is prohibited except for Dock Space gestures and title-bar drags for [macOS native Space switching or sends](macos.md#managed-native-spaces-input-and-transfers). Around fullscreen windows, the exception follows the shared [fullscreen-state rule](#fullscreen-state-based-protection) and [fullscreen workspace and layout actions](window-state.md#fullscreen-workspace-and-layout-actions), the same rules KDE follows.
- Why: anti-cheat false positives put user game accounts at risk.
- Assumes / revisit if: these mechanisms are those anti-cheat systems flag; revisit after anti-cheat research. Current code audit against this list remains outstanding.
- Status: active, 2026-11-27.
- Evidence: User 2026-11-27; [fullscreen-state rule](#fullscreen-state-based-protection), [fullscreen workspace and layout actions](window-state.md#fullscreen-workspace-and-layout-actions), [R-INS-06, Ours KDE](../spec/reference-outcomes/insertion.md#r-ins-06-explicit-selection-discriminator-fresh-fullscreen-leg), [Gaming Compatibility](../principles.md#gaming-compatibility), [interface review](../changes/core-interface-review.md).

### Fullscreen-state based protection
- Decision: Fullscreen windows (exclusive and borderless-windowed fullscreen, games included) are managed workspace members. The user can restore them, make them fullscreen again, and switch to and from them. Normal keys and actions apply; explicit workspace switches dispatch even with fullscreen on any output. Automatic actions and sends retain their safety fences. Without an explicit request, never tile or overlay a fullscreen window, intercept its input, or write its frame, show state, focus, stacking or native workspace membership. A windowed game is an ordinary window.
- Why: protect fullscreen state and avoid unasked interference without treating game identity itself as a safety state.
- Rejected: blanket treatment of every game as protected regardless of fullscreen state.
- Assumes / revisit if: host fullscreen facts distinguish fullscreen from windowed state; existing fullscreen safety gates remain authoritative.
- Status: active, 2026-11-01; normal actions and workspace dispatch clarified by User 2026-11-27.
- Evidence: [Gaming Compatibility](../principles.md#gaming-compatibility), [R-MAX-07](../spec/reference-outcomes/maximize-fullscreen.md#r-max-07-captionless-full-monitor-cover), [R-OUT-11](../spec/reference-outcomes/multi-output.md#r-out-11-directional-focus-between-fullscreen-or-maximized-outputs).

### Xbox-mode pause and release gate
- Decision: If a documented Microsoft Xbox-mode signal becomes available, pause automatic tiling and window effects while remembering windows/workspaces for restoration. Decision (User 2026-11-28): no blanket pause of explicit OmniTiler shortcuts; leave OS-owned shortcuts with the host. Do not use cloak/foreground heuristics; automatic pause remains unimplemented without signal. Alternate Game Bar access over fullscreen game is a later experiment after parity/correctness. Windows release gate after multi-output includes Win+G/F11 containment review, Xbox detection and M16 100ms pump-cost measurement; P0 label does not schedule it now.
- Why: keep ordinary commands available without taking host-owned shortcuts or imposing automatic writes.
- Assumes / revisit if: the fullscreen-state rule and its uncertainty fallback remain in force; alternate keyboard/mouse access remains later work.
- Status: active, 2026-10-10
- Evidence: [gaming coexistence](../changes/windows-gaming-coexistence.md).
