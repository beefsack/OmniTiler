# Activation (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. No existing activation rows to backfill (0 existing rows, 0 cells). R-WS-07 is user-selected shell activation, not an application's unsolicited request, and is reused by citation only.

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the
index. Each scenario below has exactly one Then bullet per profile (14).
`S()` tags attach only to the established sub-leg; anything else on that
line stays TBD. Column Given bullets are separate fixtures, never H/V
ancestry claims. Ours cells cite current actual Engine + adapter source
at `29bc4d9` via the new `S(S-ours-act)` model key; selected intent and
doc assertions are never evidence. Generic focus actuators (directional
focus ops, shell-selected activation) are never request evidence; only
the native activation/urgency handler paths below count.

Request/urgency handler inventory (one path per profile; each When below
uses the profile's native request route, no forced kill, no shell
switcher): COSMIC xdg-activation token/context branches (ordinary
unsandboxed clients get Workspace tokens; sandboxed serial-less tokens
are urgency-only and stale-serial tokens are denied at creation);
Hyprland `CWindow::activate` reached both by the X11
`onActivationRequest` path and the Wayland xdg-activation dispatch;
bspwm `_NET_ACTIVE_WINDOW` client message plus hint-urgency flags; i3
`_NET_ACTIVE_WINDOW`/configure handlers plus urgency client messages;
xmonad EwmhDesktops `_NET_ACTIVE_WINDOW` via the default `doFocus`
activate hook (core reads `WMHints` for input-focus only with no
urgency handling; `doAskUrgent` marking is opt-in via
`setEwmhActivateHook` plus `UrgencyHook` wiring, absent from this
profile); sway xdg-activation plus `focus_on_window_activation`
policy (tokens from a focus-less client mark urgent); qtile X11
`_NET_ACTIVE_WINDOW` dispatch via `activate_by_config` (shipped
`smart` marks off-screen requesters urgent) plus hint urgency and
focus-time clear; awesome `request::activate`
filter (no shipped `ewmh` filter; hidden-tag requests mark urgent
without switching) plus `request::urgent` handler and focus-time
clear; niri `request_activation`
plus `on-xdg-activate` rules (serial-less tokens mark only;
invalid-serial tokens are denied unless the debug flag is set);
PaperWM/paneru have no traced activation/urgency handler at
the pins (host journeys only); karousel pinned code only observes
host `windowActivated`, so its legs resolve via host KWin
`S(S-kwin-act)` where the host mechanism is deterministic, else
stay TBD with the host fork named. Ordinary unsandboxed COSMIC clients
take the privileged branch to Workspace tokens; the declared
sandboxed route exposes the workspace-level marker only.

### R-ACT-01: hidden window sends an unsolicited activation request

- Given (tree profiles): hidden `WS1=H[B]`, shown `WS2=H[A*]`; no
  recent user action in B, so B holds no qualifying user serial.
  Ordinary windows, no rules, scale 1.
- Given (column profiles): hidden `WS1=COL[C1[B]]`, shown
  `WS2=COL[C1[A*]]`; same serial condition; shipped defaults apply.
- Given (COSMIC sandboxed variant): same fixture with B sandboxed
  under a non-panel security context, so the privileged token
  branch does not apply; a stale-serial token attempt is the second
  leg of this variant.
- When: B sends its native activation request. Primary variant: the
  ordinary unsandboxed request (X11 `_NET_ACTIVE_WINDOW` on X11
  profiles, serial-less token request where xdg-activation owns
  the route, owned Wayland dispatch where traced). COSMIC
  sandboxed variant: serial-less token request plus stale-serial
  token attempt as its second leg.
- Observe: switch-to-B vs pull-B vs urgency-only marker vs denial;
  request token/user-timestamp qualification.
- Then COSMIC: switches to WS1 and focuses B. Ordinary unsandboxed
  B takes the privileged branch to a Workspace token, and shipped
  `Focus` drives `activate_surface` across workspaces. Sandboxed
  variant: serial-less stays `UrgentOnly` (marker only, no switch);
  stale-serial tokens are denied at creation. `S(S-cos-act)`.
- Then Hyprland/Dwindle: urgency mark only; stays on WS2 with A
  focused at shipped default. Both the X11 request path and the
  Wayland xdg-activation dispatch funnel into `activate()`, which
  always sets the urgent hint but focuses only under
  `misc:focus_on_activate` (default false).
  `S(S-hyp-act)`.
- Then bspwm: switches to WS1 and focuses B. The
  `_NET_ACTIVE_WINDOW` message (honored at shipped
  `ignore_ewmh_focus=false`) calls `focus_node` on the hidden
  desktop, which shows that desktop and focuses the node.
  `S(S-bsp-act)`.
- Then i3: urgency mark only; stays on WS2 with A focused at shipped
  default. Default `smart` marks hidden-workspace requesters urgent
  instead of showing their workspace.
  `S(S-i3-act)`.
- Then xmonad/Tall+Navigation2D: switches to WS1 and focuses B.
  The profiled `ewmh` composite handles `_NET_ACTIVE_WINDOW`
  through the default `doFocus` activate hook, which focuses
  immediately, switching workspace if necessary.
  `S(S-xmo-act)`.
- Then sway: urgency only; stays on WS2 with A focused at shipped
  default. A token from a client holding no focus reaches neither
  the internal-seat nor the focused-surface branch, so it falls to
  `view_request_urgent`; the shipped `focus_on_window_activation=urgent`
  marks without focusing.
  `S(S-sway-act)`.
- Then qtile/Columns: urgency mark only; stays on the shown group
  with A focused at shipped default. An app-source
  `_NET_ACTIVE_WINDOW` message reaches `activate_by_config`, and the
  shipped `focus_on_window_activation="smart"` marks requesters
  whose group screen is not the current screen urgent instead of
  switching to their group (pager-source requests bypass to
  `activate` and are not this leg).
  `S(S-qti-act)`.
- Then awesome/tile: urgency mark only; stays on the shown tag with
  A focused at shipped default. `_NET_ACTIVE_WINDOW` emits
  `request::activate` context `ewmh` with `raise=true`; no shipped
  `ewmh`/generic filter claims the request (the sole shipped filter
  is `mouse_enter`-scoped), the hidden-tag B fails `isvisible` so
  focus is skipped, and the raise branch sets `c.urgent` without
  switching tags.
  `S(S-awe-act)`.
- Then niri: urgency-only marker; no switch or focus at shipped
  default. With no `on-xdg-activate` rule in the shipped config, a
  serial-less token takes the `UrgentOnlyMarker` branch and only
  sets urgent; invalid-serial tokens are denied instead (separate
  no-op leg unless the debug flag is set).
  `S(S-nir-act)`.
- Then PaperWM: TBD (no activation-request route exists in the pinned
  extension inventory; hidden-space request routing plus workspace-switch
  and focus ride host GNOME Shell/Mutter). `S(S-pap-acts)`; queued (host).
- Then karousel/Lazy: TBD (host KWin `RootInfo::changeActiveWindow`
  forks on the request timestamp at shipped defaults: stale/zero
  timestamps fail `allowWindowActivation` to `demandAttention`
  (marker only); an unknown timestamp activates under shipped
  `SwitchToOtherDesktop`. The row fixes no message timestamp, and
  karousel itself contributes no request route beyond observing
  host `windowActivated`). `S(S-kwin-act)`; queued.
- Then paneru: TBD (no activation/urgency request path in the pinned
  source: the `Operation` inventory carries no request/mark verb and
  cross-strip listing/activation is macOS-owned, so whether hidden B's
  request switches to WS1 with B focused vs marks vs denies rides
  opaque host WindowServer/AppKit policy). `S(S-pan-cmds)` +
  `S(S-pan-switcher)`; queued (host).
- Then Ours KDE: native journey TBD; source trace at KWin `8438567a` and
  v6.6.3 shows valid-token/FromTool/allowed-X11 requests switch + focus,
  missing/stale-token requests demand-attention. Selected User 2026-10-13:
  accept native host exception, converge without intercept/revert/suppress;
  verification-only, no longer BLOCKED. No OmniTiler gaming guard, rely on
  KWin input freshness; real-game active/idle check must pass, reopen only on
  actual active-play switch-away. Post-0.1 setting KDE applicability reviewed
  at design ([decision](../../decisions.md#activation-requests),
  [trace](../../research/d27-kwin-activation-trace.md)). `S(S-ours-act)`
  establishes observation only, not native acceptance.
- Then Ours Windows: partial native evidence 2026-10-14 at `f4b9f41`:
  fresh background PowerShell `SetForegroundWindow` against current and
  product-hidden owned helpers was denied, focus/visibility unchanged and no
  foreground-driven workspace switch. Shell `HSHELL_FLASH` observed for probe
  targets including hidden helpers, not proof of visible urgency. Previously
  input-thread-attached caller could activate hidden B and trigger workspace
  convergence; not helper self-request or physical selection evidence.
  No dedicated product request/urgency observer, marker or classifier.
  Helper-originated/native-permitted clean requests, visual mark/clear,
  fullscreen/game and explicit selection remain TBD
  ([record/options](../../changes/archive/windows-d27-activation-verification.md)).
  `S(S-ours-act)`; default Mark urgent only unchanged. User-approved 2026-10-16:
  Windows Switch and focus honors OS-permitted activation over fullscreen/games,
  with no urgency-only downgrade; non-permitted requests unchanged. Switching
  hides the game's workspace, an accepted consequence awaiting real-game evidence.
  This amendment and the post-0.1 two-option setting are not yet implemented.
  KDE exception is unchanged and is not Windows evidence.
- Variant hook: provisional/TBD (activation-routing hook, to discuss).

### R-ACT-02: urgency marker set, then user focuses the window

- Given (tree profiles): `H[A*,B]`, B not urgent. Ordinary windows,
  no rules, scale 1.
- Given (column profiles): `COL[C1[A*],C2[B]]`; same legs; shipped
  defaults apply.
- Given (COSMIC): B runs sandboxed with a non-panel security context, so the
  marker leg uses the serial-less workspace-marker route; an ordinary
  request would steal focus and cannot substitute for it.
- When: B sets urgency via its native mark path, then the user
  focuses B. Native mark paths: urgency hint (bspwm/qtile/awesome),
  demand-attention client message (i3), serial-less activation request
  (sandboxed COSMIC/niri), native activation request through `activate`
  (Hyprland), focus-less-client xdg request under policy `urgent`
  (sway). Mark and clear are separate sub-legs; a sourced mark never
  implies a sourced clear.
- Observe: focus stolen vs attention marker only/ignored while
  unfocused; marker cleared on focus vs retained.
- Then COSMIC: WS1 marked without stealing focus; focusing B on
  the same workspace retains the workspace-level `Urgent` marker
  (no focus-time clear). The serial-less route adds
  `WState::Urgent`; `set_focus`/`update_active` write focus state
  only, and the sole `Urgent` removal runs on workspace switch
  (`self.active != idx`).
  `S(S-cos-act)` + `S(S-cos-actclear)`.
- Then Hyprland/Dwindle: B marked urgent without stealing focus;
  focusing B clears the hint. `activate()` marks unconditionally
  while focus stays gated, and taking focus strips the urgent bit.
  `S(S-hyp-act)`.
- Then bspwm: B flagged urgent without stealing focus; focusing B
  clears the flag. Hint urgency sets the client flag, and the focus
  path clears it.
  `S(S-bsp-act)`.
- Then i3: B marked urgent without stealing focus; focusing B
  clears the leaf, parents, and workspace flags. Demand-attention
  messages and hidden-requester policy set urgency; the focus path
  resets leaf urgency.
  `S(S-i3-act)`.
- Then xmonad/Tall+Navigation2D: no native urgency marker is set;
  user focus is ordinary focus with nothing to clear. Core reads
  `WMHints` for input-focus only with no urgency handling anywhere
  in `src/XMonad`; the profiled `ewmh` composite maps
  `_NET_ACTIVE_WINDOW` to the default `doFocus` focus hook
  (activation focus, not a marker substitute), and `doAskUrgent`
  marking is opt-in via `setEwmhActivateHook` plus `UrgencyHook`
  wiring absent from this profile, so the native mark leg has no
  counterpart here.
  `S(S-xmo-act)`.
- Then sway: B marked urgent without stealing focus; focusing B
  clears it (immediately, or via the `urgent_timeout` timer when
  the focus arrives with a workspace switch). Shipped
  `focus_on_window_activation=urgent` marks; the seat focus path
  unsets or arms the timer.
  `S(S-sway-act)`.
- Then qtile/Columns: B marked urgent without stealing focus;
  focusing B clears the hint and strips `_NET_WM_STATE_DEMANDS_ATTENTION`.
  Hint updates set the flag off-focus; the focus path resets it.
  `S(S-qti-act)`.
- Then awesome/tile: B marked urgent without stealing focus;
  focusing B clears it. The `request::urgent` handler sets
  `c.urgent` off-focus; taking focus runs `client_focus_update`,
  which clears the urgent flag (EWMH), and the `focus` signal drops
  B from the urgent stack.
  `S(S-awe-act)`.
- Then niri: B marked urgent without stealing focus; focusing B
  clears it. `set_urgent` refuses while focused, and taking focus
  resets the flag.
  `S(S-nir-act)`.
- Then PaperWM: TBD (no urgency mark/clear path exists in the pinned
  extension source: keyword sweep finds no urgency handling across
  `tiling.js`/`keybindings.js`/`navigator.js`/`liveAltTab.js`/`scratch.js`/
  `settings.js`/`workspace.js`/`extension.js`/`prefs.js`, and the closed
  registered action inventory lists no mark/clear verb; host Shell/Mutter
  mark/clear behavior stays untraced). `S(S-pap-acts)`; queued (host).
- Then karousel/Lazy: B marked urgent without stealing focus;
  focusing B clears it, via host KWin. A hint-urgency property
  notify runs `updateUrgency` into `demandAttention`, which only
  sets the flag (refused while active, never focuses); taking focus
  runs host `setActiveWindow`, which calls
  `demandAttention(false)`. Karousel itself contributes no
  mark/clear path and only observes host `windowActivated`.
  `S(S-kwin-act)`.
- Then paneru: TBD (no urgency mark/clear path in the pinned source:
  no mark/clear verb in the `Operation` inventory and no
  activation-request/urgency-mark event in the `Event` window
  inventory; user focus follows only via the guarded host
  `WindowFocused` path with frontmost/app-reported guards, but whether
  a marker exists to clear rides opaque host policy). `S(S-pan-cmds)` +
  `S(S-pan-focusobs)`; queued (host).
- Then Ours KDE: TBD (the Engine resyncs focus on ordinary
  activation of a known window; the KDE observer exposes no
  attention/urgency signal and the adapter actuates `setActive`
  only, so both the native mark and the focus-time clear are
  untraced). `S(S-ours-act)`; queued.
- Then Ours Windows: partial evidence 2026-10-14 at `f4b9f41`:
  background `FlashWindowEx(FLASHW_TRAY)` on current/native-hidden/product-hidden
  helpers left focus/visibility unchanged; shell flash notifications heard.
  Return value is previous caption state, not mark success. Visible marker and
  focus-time clear remain TBD; no product observer/marker path
  ([record](../../changes/archive/windows-d27-activation-verification.md)).
  `S(S-ours-act)`; queued.
- Variant hook: provisional/TBD (urgency hook, to discuss).
