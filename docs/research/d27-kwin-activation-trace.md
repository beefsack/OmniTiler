# D27: KWin native activation trace (verification-only on KDE)

- Status: trace complete 2026-10-12 at KWin ref `8438567a` (= 6.7.4); v6.6.3 compared. User 2026-10-13 selected option 1, native KWin host exception; no longer BLOCKED, KDE verification-only. No product code written. Native acceptance pending.
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
- Gaming guard: absent in code; User 2026-10-13 selects no OmniTiler guard on KDE.

## Historical blocker and options (resolved 2026-10-13)

- Historical blocker: implementing default Mark urgent only for the valid-token class would intercept/revert native KWin desktop switch + focus, contradicting D01 native ownership. Resolved by accepting the host exception below.
- Option 1 (recommended): accept KWin native behavior on KDE as a host exception - valid-token requests switch + focus natively and OmniTiler converges (already does); urgency-only applies only where KWin itself marks (missing/stale token) plus any explicit fullscreen/game guard implemented narrowly without touching desktop switching. Consequence: REQ-ACT-01 default differs from Hyprland/i3/sway reference on KDE for the valid-token class; gaming guard needs a separate decision on whether to suppress native switches while fullscreen is focused (also an override).
- Option 2: intercept valid-token activations and mark urgent (suppress the switch). Consequence: overrides KWin, conflicts with D01, needs to fight the compositor (revert desktop + focus), gaming-safety and complexity risk; not recommended.
- Option 3: document user-side KWin config (raise FSP to Extreme, or set activation desktop policy to DoNothing). Consequence: host-wide behavior change outside OmniTiler, affects all apps; weakest product story.
- Recommendation was option 1. User selected it 2026-10-13, with no OmniTiler gaming guard; the historical guard-scope question is resolved.

## User decisions 2026-10-13

- Accept native KDE host exception: valid-token / FromTool / allowed-X11
  hidden-desktop requests switch desktop and focus natively; missing/stale-token
  requests are marked demand-attention by KWin. OmniTiler converges, never
  intercepts, reverts or suppresses activation. Alternatives are complex hacks,
  a maintenance burden and add jank.
- No OmniTiler gaming guard on KDE; rely on KWin token-serial / X11 user-time
  input-freshness gating. Active-play requests are expected mark-only, not
  proven; idle-game and pager/taskbar switches are exactly native without-tiler
  behavior. Source has no fullscreen-specific guard and can permit authorized
  switches; do not claim freshness guarantees all request classes.
- Must-pass live real-game check covers active play and idle/paused game.
  Reopen only if real active-play switch-aways occur. Default Mark urgent only
  and Windows gaming guard remain unchanged where OmniTiler owns the outcome.
- KDE D27 is verification-only: valid-token native switch convergence and
  stale-token mark-only without interference. Post-0.1 Activation request
  setting's KDE applicability is reviewed at design, not selected here.
  [Current decision](../decisions.md#activation-requests).

## Pending live checks (user-owned, not run)

- Tiler-off/on comparison at default Low: valid-token activation from hidden desktop (switch + focus expected both ways); stale/missing-token request (mark-only both ways); Xwayland app-source vs pager-source; FSP Extreme spot check; fullscreen-focused request outcome; attention-clear on focus.
- Must-pass real-game active-input and idle/paused legs: compare native requests
  and pager/taskbar switching with tiler off/on; observe actual switch-away,
  marker, desktop, focus and game state. Reopen only on actual active-play
  switch-away. No native/game outcome recorded yet.

## Sources

- https://raw.githubusercontent.com/KDE/kwin/8438567a/src/activation.cpp
- https://raw.githubusercontent.com/KDE/kwin/8438567a/src/xdgactivationv1.cpp
- https://raw.githubusercontent.com/KDE/kwin/8438567a/src/netinfo.cpp
- https://raw.githubusercontent.com/KDE/kwin/8438567a/src/kwin.kcfg
- https://raw.githubusercontent.com/KDE/kwin/v6.6.3/src/xdgactivationv1.cpp
- https://raw.githubusercontent.com/KDE/kwin/v6.6.3/src/activation.cpp
