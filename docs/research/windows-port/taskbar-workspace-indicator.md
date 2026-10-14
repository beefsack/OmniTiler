# Windows taskbar workspace indicator: options

Research only. No product selection, no code or host settings changed.
Checked 2026-10-14. No live desktop probe performed.

## Local baseline

- Tray policy/glyph/menu: [crates/tiler-windows/src/tray.rs](../../../crates/tiler-windows/src/tray.rs)
  (portable, unsafe-forbidden; 32x32 BGRA glyph; scoped toggle requests).
- Native tray effects: [crates/tiler-windows/src/tray_sys.rs](../../../crates/tiler-windows/src/tray_sys.rs)
  (Shell_NotifyIconW, stable GUID, hidden callback HWND, TaskbarCreated
  recovery, change-gated sync).
- Workspace model: [crates/tiler-windows/src/workspace.rs](../../../crates/tiler-windows/src/workspace.rs)
  (outputs/membership/active snapshots; no urgency field).
- Dependency surface: [crates/tiler-windows/Cargo.toml](../../../crates/tiler-windows/Cargo.toml)
  (windows-sys 0.61 incl. Shell/Gdi/HiDpi).
- Urgency decisions: [decisions.md#activation-requests](../../decisions.md#activation-requests)
  (D27 mark-urgent-only default, fullscreen/game urgency-only guard, 5a
  workspace-urgent-cleared-on-visit; no observer/classifier exists).
- Related independent decision: [alt-tab-hidden-workspaces.md](./alt-tab-hidden-workspaces.md).
- Apply [principles](../../principles.md): active/occupied/urgent state and
  clear-on-visit policy belong in the shared core; the Windows layer owns
  identity-verified event ingestion, rendering and clicks. Use new modules,
  respecting the ~1,500-line production-file soft cap, not tiling_sys.rs.
- Resilience: icon failure must not block tiling; retain the desired snapshot
  and retry/re-add after shell recovery. Release replaced HICON handles.

## Shared legend (same for every option; never color-only)

- Active: workspace number plus outline/check plus "(active)" text.
- Occupied: dot marker plus window count text; empty shows number only.
- Urgent: badge (triangle/exclamation) plus "urgent" text; cleared on visit.
- Indicator UI is separate from the urgency-acquisition gap (no observer
  exists); HSHELL_FLASH is a candidate only and RegisterShellHookWindow is
  [documented "not intended for general use"](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registershellhookwindow); WinEvent flash delivery is
  not inferred. No automatic reveal/switch/balloon on urgency.

## Official shell facts used

- Tray: Shell_NotifyIconW NIM_ADD/MODIFY/DELETE/SETVERSION
  ([Shell_NotifyIconW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw));
  identity via hWnd+uID or guidItem, NIF_GUID Win7+
  ([NOTIFYICONDATA](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw)).
- Explorer restart: shell broadcasts registered TaskbarCreated; apps re-add
  ([The Taskbar](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar)).
- Visibility: legacy docs say icons hidden by default with user-only control
  ([Taskbar Extensions](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar-extensions));
  not verified as universal Win10/11 behavior. Supported claim only: an icon
  may remain in overflow; no supported force-pin API.
- DPI/theme: LoadIconMetric loads resource icons at a system metric
  ([LoadIconMetric](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nf-commctrl-loadiconmetric?redirectedfrom=MSDN));
  it is not a dynamic renderer. Plan: render each digit HICON at the actual
  small-icon/DPI metric, invalidate on theme change, draw a contrasting
  backplate so one asset works in light/dark/high-contrast.
- Overlay: one per-group 16x16-at-96dpi overlay, last-received wins, no queue;
  reapply after TaskbarButtonCreated; small-icon mode ignores it; supplied
  graphics must be self-contrasted with pszDescription alt text
  ([SetOverlayIcon](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-itaskbarlist3-setoverlayicon)).
- Thumbnail toolbar: max 7 buttons, right-to-left truncation; TaskbarButtonCreated
  required before any ITaskbarList3 call
  ([Taskbar Extensions](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar-extensions)).
- AppBar: SHAppBarMessage ABM_NEW/QUERYPOS/SETPOS/REMOVE reserves work area
  ([AppBars](https://learn.microsoft.com/en-us/windows/win32/shell/application-desktop-toolbars));
  ABN_FULLSCREENAPP requires drop-to-bottom on fullscreen open (necessary, not
  sufficient proof of game-safety for borderless games).
- Deskband rejected: IDeskBand says "use thumbnail toolbars in new development
  in place of desk bands, which are not supported as of Windows 7"
  ([IDeskBand](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ideskband)).
- Widgets rejected: Widgets Board is a Win11 flyout only; providers must be
  packaged Win32/PWA
  ([Widgets design](https://learn.microsoft.com/en-us/windows/apps/design/widgets/),
  [Widget providers](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-providers)).

## Options

| # | Option | Verdict | Active / occupied / urgent encoding |
| --- | --- | --- | --- |
| A | Single dynamic tray icon + popup grid/list | Recommended first | Icon: focused-output active number; urgent badge on icon if ANY output urgent. Popup/tooltip: per-output rows with full legend; exact urgent output/workspace named |
| B | Multiple numbered tray icons | Feasible but poor | Per icon: active gets frame/check in glyph, occupied gets dot, urgent gets badge; tooltip names workspace + state text |
| C | Ordinary taskbar button + overlay / thumbnail toolbar | Feasible, narrow | Overlay carries one state only (recommend urgent badge). Toolbar: up to 7 numbered icon buttons with active frame, occupied dot and urgent badge where legible; tooltips carry full state text |
| D | Standalone AppBar / docked strip | Feasible, heaviest | Full cells with legend on every cell; desktop-only visibility, never over fullscreen |
| E | External bar via custom OmniTiler IPC (Zebar evaluated) | Out of product scope unless user opts in | Urgency needs OmniTiler state publisher + custom widget work; neither Zebar's glazewm provider nor stock widgets accept OmniTiler without that |

## Prior art (verified 2026-10-14)

- komorebi sibling HEAD e0709f02: `komorebi-bar/src/widgets/komorebi.rs`
  render path (`render_workspaces`, L193-222: per-monitor strip, selected
  framing, `should_show`/hide-empty) and click path (L215-219 click sends
  `FocusMonitorWorkspaceNumber`; L353-368 mouse-follow-off batching).
  No fullscreen auto-hide and no urgency state found in inspected source.
- GlazeWM sibling HEAD 5709ad0a:
  `packages/wm-platform/src/platform_impl/windows/native_window.rs` L466-529
  (cloak/ITaskbarList control hidden-window taskbar presence) is not a
  workspace indicator. GlazeWM README documents Zebar as its status bar
  ("Integration with Zebar as a status bar").
- Zebar evaluated (web, no local source): glazewm provider exposes
  `currentWorkspaces` (per-monitor via `currentMonitor`), `allWorkspaces`,
  `displayedWorkspace`, `focusedWorkspace`, plus `runCommand`
  (e.g. `focus --workspace 1`) for click-to-focus
   ([zebar README](https://github.com/glzr-io/zebar#glazewm),
   [`packages/client-api/src/providers/glazewm/glazewm-provider-types.ts`](https://github.com/glzr-io/zebar/blob/main/packages/client-api/src/providers/glazewm/glazewm-provider-types.ts)).
  Active/occupied pattern corroborated by YASB GlazeWM widget docs
  (active/focused/populated/empty button classes,
  [widget doc](https://docs.yasb.dev/latest/widgets/glazewm-workspaces)).
   No dedicated urgency field in the inspected GlazeWmOutput interface;
   urgent presentation and game/fullscreen hiding remain unverified.
  Zebar/YASB do not accept OmniTiler state without a custom IPC/provider.

## Per-option notes

| Dim | A: single tray+popup | B: N tray icons | C: taskbar button | D: AppBar strip | E: external |
| --- | --- | --- | --- | --- | --- |
| Mechanism | Shell_NotifyIconW NIM_MODIFY + native popup menu/window | Shell_NotifyIconW per stable workspace GUID | Visible owner HWND + ITaskbarList3 SetOverlayIcon/ThumbBarAddButtons | SHAppBarMessage registered window; query/set position and reserve work area | Socket/IPC snapshot + custom widget, separate bar window |
| Per-output | Popup groups by output; icon reflects focused output (user chooses; default: focused) | Icons global and system-ordered; per-output grouping unverified | Button is per-window, not per-output; grouping via AppUserModelID unverified | One strip per monitor possible; "always visible" means desktop only, not fullscreen/auto-hide | Same as D via external config; bar is a separate top-level window, not a native taskbar insertion |
| Click switch | Explicit click opens popup; row pick requests switch under stale-scope fence | Click icon requests its workspace; left/right mapping undecided | Thumbnail click routes WM_COMMAND to owner; activation side effects need validation; ordinary taskbar-button click activates the owner window | Direct cell click; hit-test owned | Provider runCommand-style IPC focus call |
| Fullscreen/game | Design intent: no unsolicited window/overlay/balloon/animation; games keep ordinary taskbar visibility; game acceptance unmeasured | Same intent; N icons multiply overflow noise | Thumbnail UI needs user hover; no auto-popup by intent | ABN_FULLSCREENAPP lowering alone is insufficient; requirement is hide/yield safely, unproven | Depends on external bar; unowned |
| Unsafe/deps | Extends windows-sys Shell/Gdi/HiDpi path; minimum reviewed unsafe containment | Same APIs, N-fold handles + ordering code | Adds Shell COM ITaskbarList3 + visible HWND; prefer maintained safe wrappers; windows/windows-sys expected | New visible window + SHAppBarMessage + per-monitor DPI/reserve; largest | IPC can stay safe Rust; external runtime (Zebar uses native webviews/WebView2) |
| Theme/DPI | Digit HICON at actual metric + backplate + DPI/theme-change invalidation; multi-digit numbers/badges need legibility checks | Same times N; several markers compete at 16px | Overlay graphics self-contrasted and DPI-scaled; tooltips/alt text for accessibility | Full render control; per-monitor DPI + light/dark/high-contrast required | External renderer/theme; validate DPI and contrast |
| Win10 vs 11 | Tray on both; may remain in overflow; user controls pinning | Same, worse (N icons to pin) | Both; grouping/visual differences unverified | Both; Win11 bottom-only taskbar affects edge choice | Depends on bar matrix |
| New handwritten LOC (estimate; excl. tests and urgency acquisition) | ~200-400 (digit renderer + popup + sync; plus game/theme/DPI hardening) | ~300-500 (ledger + ordering + sync) | ~400-700 (visible window + COM + toolbar/overlay) | ~1200-2500 (window + reserve + per-monitor + input + DPI/theme) | ~200-500 IPC publisher; widget/provider extra |
| Risks | Glyph size; popup discoverability; overflow | Overflow xN; unstable ordering; N pins | 7-button cap; one overlay state; visible-window taskbar rules interact | Permanent reservation; z-order/focus bugs; most code | Version drift; install/support burden |

Shared plumbing separate estimate: active/occupied/urgent snapshot model in
tiler-core ~200-400 LOC (estimate, excl. tests and urgency
acquisition/classification, which is the existing gap, not costed here).

## Recommendation and open question

- Recommended first: A (single dynamic tray icon + popup grid/list).
  Smallest unsafe delta, reuses TaskbarCreated/change-gated sync, no screen
  reservation, no unsolicited UI by design intent. Update only on state/DPI/
  theme changes, without a new animation or polling timer. Zero perceivable
  game cost is an acceptance target, not established by this research.
- If the user insists all cells always visible: A cannot do it; needs D
  (owned strip cost) or E (dependency + custom IPC/provider cost). C is not
  a substitute (one overlay state, max 7 buttons).
- Question for user: is tray-with-popup acceptable, or is an always-visible
  strip required? Also choose: which output the icon reflects, and click
  behavior (popup vs direct cycle). Defaults: focused output; click opens popup.

## Later user-validation needs

- Confirm icon stays out of overflow only via user pinning (no force-pin API).
- Confirm popup open/focus behavior beside fullscreen games on each OS build.
- Confirm C click activation side effects and D fullscreen yield before use.
- Confirm E bar choice and who owns its install/config.
