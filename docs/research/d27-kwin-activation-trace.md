# D27: KWin native activation trace (BLOCKED, no code)

- Status: trace complete 2026-10-12 at KWin ref `8438567a` (= 6.7.4); v6.6.3 compared. Branch (B): KWin natively switches desktop + focuses for a request class at default FSP, so default urgency-only would override KWin and conflict with D01 native activation ownership. No product code written. Decision required before any implementation.
- Lead verification: fetched `src/activation.cpp` at the pinned ref (activateWindow desktop-switch block, mayActivate token gate, setActiveWindow attention clear corroborated); local `rg "demandAttention|demandsAttention|urgent|urgency" kwin/src` returns no files, confirming no OmniTiler urgency handling.

## KWin findings (all lines at 8438567a unless noted)

- Default FSP is Low (1): `src/kwin.kcfg:112-116`. Default desktop policy on activation is SwitchToOtherDesktop: `src/kwin.kcfg:138-145`.
- Wayland xdg-activation, valid token: `XdgActivationV1Integration::activateSurface` (`src/xdgactivationv1.cpp:104-125`) calls `mayActivate` (`src/activation.cpp:578-637`); token gate `token == m_activationToken && lastInteractionSerial <= tokenSerial` (`src/activation.cpp:602`). On true, `Workspace::activateWindow` (`src/activation.cpp:294-357`) raises, SWITCHES desktop when `!isOnCurrentDesktop` (309-322), unminimizes/unhides, then `requestFocus` (340-342). Hidden-desktop window: switches + focuses.
- Wayland, missing/stale token: no denial reply; mark-only via `window->demandAttention()` (`src/xdgactivationv1.cpp:121`). Denied token issuance returns `not-granted-666` without storing (`src/xdgactivationv1.cpp:62-70,97`). Exceptions returning true without exact token: no active window (580-582), FSP None (585-587), active-transient (589-591), ancestor serial (597-600), appId match below Extreme (610-614), fullscreen layer-shell overlay (622-634). Extreme requires exact token (604-607). New-window mapping has an extra FSP<=Low bypass (`src/workspace.cpp:933`) that does NOT apply to activateSurface on existing windows.
- Xwayland _NET_ACTIVE_WINDOW: `RootInfo::changeActiveWindow` (`src/netinfo.cpp:149-177`). FromTool (pager, data 2, legacy 0 normalized at 156-158) forces `activateWindow(c, true)` (159-160), ignoring FSP. FromApplication (data 1) checks `X11Window::allowWindowActivation` (165, defined `src/x11window.cpp:4257-4331`) then non-force `activateWindow` (166), else requestor-window check (168-171), else `demandAttention` (173). Both activate paths switch desktops via the same block above; force only changes the focus tail (340, 385-387).
- `requestFocus` alone never switches (`src/activation.cpp:366-402`); rejects hidden windows (389-392). Desktop switching lives only in `activateWindow`.
- demandsAttention: set in `Window::demandAttention(true)` (`src/window.cpp:679-691`, no-op if active); cleared on activation in `Workspace::setActiveWindow` (`src/activation.cpp:250-253`). X11 urgency re-raises on set-active (`src/x11window.cpp:4214-4221`).
- Gaming guard: NO fullscreen-focused special casing in the activation decision path. With a valid token, a FromTool request, or an allowed X11 timestamp, KWin still switches away from a focused fullscreen window. Only adjacent code: multi-output ActiveLayer repair (255-262) and the layer-shell overlay exception (616-634).
- Call chains: Wayland `activateRequested` (`src/wayland/xdgactivation_v1.cpp:113-116`) -> `activateSurface` -> `mayActivate` -> `activateWindow(force=false)` -> `requestFocus` -> `takeFocus` + `setActiveWindow`. X11 `changeActiveWindow` -> `allowWindowActivation` -> `activateWindow`. No `forceActivateWindow` symbol at this ref.
- v6.6.3: same behavior for D27 questions. xdgactivation logic identical (minus windowAdded clearFeedback wiring); mayActivate uses transientFor parent recursion instead of the 6.7 ancestor-serial loop; activateWindow switch same shape (6.6.3 setCurrent lacks per-output args). No behavioral change.

## Default-FSP answer

- Valid token + hidden desktop at Low: SWITCHES desktop and FOCUSES. Not mark-only.
- Missing/stale token on existing hidden window at Low: mark-only, no switch.
- Therefore attention-only is native ONLY for the missing/stale-token class.

## OmniTiler current behavior (main HEAD 15aa5e3)

- Activation-request handler: ABSENT. No demandAttention/urgency/xdg-activation subscription in `kwin/src` (verified by grep). `windowActivated` used only for highlight refresh (`kwin/src/plan-adapter-entry.ts:8802-8809`) and resync trigger (5130-5136).
- Urgency signal handling: ABSENT in both adapters and shared core (protocol has no demandAttention/urgency input; mismatch refuses with FocusMismatch).
- Convergence FOLLOWS native switches: `currentDesktopChanged`/`desktopsChanged`/`screensChanged`/`windowAdded/Removed` all route to read-only `handleTopologySignal` (`kwin/src/plan-adapter-entry.ts:7363-7397`, `kwin/src/workspace-native.ts:1110-1136`) plus history record; no write-back. Shared `sync_focus_from_window` (`crates/tiler-core/src/session/world.rs:407-450`) adopts host focus only, fail-closed otherwise.
- No OmniTiler path turns requests into switches: all `workspace.activeWindow=` sites (plan-adapter-entry.ts:5041-5043, 6203-6212, 6553-6561, 7211-7219; workspace-send-adapter-entry.ts:693-695) are explicit user follow paths with arrival proof. Desktop writes (`workspace-native.ts:554,3251,3267`) are explicit shortcut/send routes only.
- Gaming guard: absent in code; exists only as D27 selection text.

## Blocker and options

- BLOCKED: implementing default Mark urgent only for the valid-token class means intercepting or reverting a native KWin desktop switch + focus, contradicting D01 native activation ownership. Per the approved condition, stop before code and return to the user.
- Option 1 (recommended): accept KWin native behavior on KDE as a host exception - valid-token requests switch + focus natively and OmniTiler converges (already does); urgency-only applies only where KWin itself marks (missing/stale token) plus any explicit fullscreen/game guard implemented narrowly without touching desktop switching. Consequence: REQ-ACT-01 default differs from Hyprland/i3/sway reference on KDE for the valid-token class; gaming guard needs a separate decision on whether to suppress native switches while fullscreen is focused (also an override).
- Option 2: intercept valid-token activations and mark urgent (suppress the switch). Consequence: overrides KWin, conflicts with D01, needs to fight the compositor (revert desktop + focus), gaming-safety and complexity risk; not recommended.
- Option 3: document user-side KWin config (raise FSP to Extreme, or set activation desktop policy to DoNothing). Consequence: host-wide behavior change outside OmniTiler, affects all apps; weakest product story.
- Recommendation: option 1. Then D27 becomes verification-only on KDE (prove convergence follows native switch; prove stale-token mark-only is observed without interference) plus a pending decision on the fullscreen/game-focused guard scope.

## Pending live checks (user-owned, not run)

- Tiler-off/on comparison at default Low: valid-token activation from hidden desktop (switch + focus expected both ways); stale/missing-token request (mark-only both ways); Xwayland app-source vs pager-source; FSP Extreme spot check; fullscreen-focused request outcome; attention-clear on focus.

## Sources

- https://raw.githubusercontent.com/KDE/kwin/8438567a/src/activation.cpp
- https://raw.githubusercontent.com/KDE/kwin/8438567a/src/xdgactivationv1.cpp
- https://raw.githubusercontent.com/KDE/kwin/8438567a/src/netinfo.cpp
- https://raw.githubusercontent.com/KDE/kwin/8438567a/src/kwin.kcfg
- https://raw.githubusercontent.com/KDE/kwin/v6.6.3/src/xdgactivationv1.cpp
- https://raw.githubusercontent.com/KDE/kwin/v6.6.3/src/activation.cpp
