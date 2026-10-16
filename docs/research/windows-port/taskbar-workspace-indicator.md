# Windows taskbar workspace indicator: options

Initial research checked 2026-10-14; no production code or host settings changed.
Later product decisions and [live evidence](#live-spike-2026-10-14) supersede
the initial tray+popup/AppBar recommendations below. The
[product overlay](#product-overlay-2026-10-16) now supersedes the child-host
implementation blocker; child-parenting failures themselves remain unexplained.

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
  (D27 mark-urgent-only default; user-approved 2026-10-16 Switch and focus
  honors OS-permitted activation over fullscreen/games, implementation pending; 5a
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

## Pager-style taskbar follow-up

User target: on EVERY taskbar, monitor-only numbered cells; empty =
darker rectangle; windows = lighter rectangles at scaled positions with
light 1px border and centered icon; active workspace = active color.
Source research checked 2026-10-14; bounded live findings follow below.

### Definitive answer

On Win11 there is no supported API for arbitrary third-party pager UI
hosted inside the taskbar. Microsoft's [Windows 11 specifications][win11-specs]
explicitly say "Apps can no longer customize areas of the Taskbar."
Taskbar Extensions covers jump lists, max-7
thumbnail toolbar buttons in the hover flyout, overlay/progress and tray
([Taskbar Extensions][taskbar-ext]); no pager slot exists. On Win10,
legacy DeskBands technically host custom taskbar UI, but IDeskBand/
IDeskBand2 directs new development to thumbnail toolbars instead, "not
supported as of Windows 7" ([IDeskBand2][ideskband2]); a Win10-only
deskband is therefore technically possible but unsuitable for a supported
Win10/11 implementation. Widgets providers deliver Adaptive Cards to
the Widgets Board flyout only ([Widget providers][widget-prov]);
rotating announcements and notification badges on the Widgets button are shell-controlled
([Widgets help][widgets-help]), not a reusable per-output host.
Search and original Copilot-style shell buttons are first-party features,
not reusable third-party extension points; a pinned Copilot app has an ordinary app button.
TaskbarManager pin-to-taskbar (updated July 2026) is a user-approved launch pin only;
the KB5074105 LAF relaxation adds no UI hosting
([Pin to taskbar][pin-taskbar]). Secondary-monitor note: 24H2/25H2
KB5065789 adds Notification Center clock/calendar click on the secondary
monitor, NOT application icon hosting ([KB5065789][kb5065789]); the
notification-icon API (hWnd/uID or guidItem) has no monitor-routing
field. Default Win11 secondary taskbars lack the application notification
area: [TrafficMonitor's secondary-taskbar branch][tm-win11dlg] explicitly
handles its absence, using a clock-width fallback. This is source-based
corroboration, not a live observation; Notification Center is a different surface.
Multiple tray icons cannot supply one monitor-scoped pager on every taskbar.
The recommendation is tray+popup first, native
own adjacent per-monitor AppBar as aspiration. "Adjacent" means a
separate reserved strip next to the taskbar, never literally embedded.

### Options

| # | Option | Target fidelity | Support / per-monitor | Game behavior | Cost (rough LOC, excl. tests + shared snapshot/urgency) | Key risk |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Tray icon + popup pager | Low at rest (digits), full grid on click | Supported both OS; tray is global, NOT per-monitor routed; N icons possible but overflow/order user-controlled | Closed popup shows no window; hide any open popup on a fullscreened monitor, never auto-open or steal focus | Full pager popup w/ icon rendering ~600-1000 (vs ~200-400 prior digit-only glyph + menu) | Discoverability; pinning is user-only, no force-pin |
| 2 | Own adjacent per-monitor AppBar pager (aspiration) | Full visuals, adjacent rather than inside | SHAppBarMessage ABM_NEW/QUERYPOS/SETPOS/REMOVE ([AppBars][appbars]); one HWND per monitor, rc seeded from its bounds ([SETAUTOHIDEBAREX][autohidebarex] shows monitor-from-rc); re-query on ABN_POSCHANGED | Hide on exclusive/borderless fullscreen there; no game resize or work-area flapping on hide/show; coexistence validation TBD | Full pager window + icon pipeline ~1500-3000 (vs ~1200-2500 prior strip) | Reservation fights; z-order/focus bugs; largest code |
| 3 | Separate overlay near/over taskbar (no reserve) | Full pixels, zero layout rights | Own HWND via public windowing APIs is supported; what is UNSUPPORTED is any inside-taskbar positioning contract (no QUERYPOS/SETPOS); taskbar may cover it ([YASB #301][yasb-301]) | Own fullscreen hide required; always_on_top alone interferes | ~800-1500 + same icon pipeline | Covered-by-taskbar; topmost over games if hide fails |
| 4 | SetParent/embed/inject into taskbar windows | Inside, if space can be found | Unsupported version-coupled path; primary and secondary HWNDs can be targeted | Child may follow shell hiding, but independent game safety is unproven | Initial prototype not meaningful; maintenance unbounded | Breaks on servicing updates |
| 5 | External bar via OmniTiler IPC (Zebar/YASB style) | Full, owned by other renderer | Separate top-level window, not a taskbar slot; YASB flags windows_app_bar / always_on_top / hide_on_fullscreen ([YASB config][yasb-config]), hide gated on always_on_top ([bar.py][yasb-barpy]); komorebi-bar inspected source: no fullscreen auto-hide | Depends on external bar config; unowned | ~200-500 IPC publisher + widget/provider extra | Install/support burden; version drift |

### Product constraints (user-approved 2026-10-15)

- Explorer injection/hooking (Windhawk-style, TaskbarWidgets-style XAML
  hooks) is rejected, reconfirmed 2026-10-15 for shell stability, Windows
  update breakage, AV/anti-cheat flags and security posture.
- Win11 offers no supported reservation among taskbar app icons;
  tray-icon placeholders are rejected (position not guaranteed, icons
  hideable).
- The pager is always anchored left of the system tray: it may overlap app
  icons when full but never the tray, and never shrinks or hides due to
  crowding. It is enabled by default with an opt-out setting; users avoiding
  overlap use the tray number plus menu pager.

### Prior art (observed docs/source; gaps noted, not universal claims)

| Project | Observed | Gaps / caveats |
| --- | --- | --- |
| TrafficMonitor ([TaskBarDlg][tm-basedlg], [Win11 dlg][tm-win11dlg], [main dialog][tm-maindlg]) | SetParent into Shell_TrayWnd/SecondaryTrayWnd on Win11; [Win10 path][tm-classic] reparents into ReBarWindow32 and shrinks MSTaskSwWClass. Start/TrayNotifyWnd geometry, 88px secondary-clock fallback, 22621 touch-height workaround. Main timer rechecks DPI/position, samples taskbar background, and reopens after secondary-count changes; TaskbarCreated closes/recreates child and re-adds tray icon | No dedicated child fullscreen hide verified; foreground fullscreen polling hides its floating main window, not proof for child/fallback/tooltips. Child relies on shell auto-hide; independent coverage unverified. Fallback requests topmost when taskbar foreground |
| ExplorerPatcher ([taskbar wiki][ep-wiki]) | Reimplements Win10 taskbar for 25921+/24H2 instead of relying on the old built-in implementation; separate build-specific DLLs include 25H2. Maintainer explicitly says updates can break it and crashes can occur | Tightly Explorer-coupled; replacement implementation, not a supported host API |
| Windhawk ([taskbar styler][wh-styler]) | XAML/resource hooking into Explorer; [issue #876][wh-876] reports mods stopped after Mar-2026 KB5079473+KB5081277 with reinstall fixing it (cause undiagnosed, report only); [discussion #2374][wh-2374] notes debug symbols needed to locate hooks | Hook mechanism / breakage cause not proven here |
| RoundedTB ([README][roundedtb]) | Geometry modification, not hosting; admits auto-hide flicker, dynamic size mismatch, and Win10 split mode only on primary taskbar | Illustrates layout fragility; no pager or game-safety contract |
| YASB / Zebar / komorebi-bar | Separate-window bars, not taskbar slots; see option 5 | Fullscreen behavior per-bar config; komorebi-bar none found |

### Reference implementations (read-only study, user records 2026-10-15)

Next step is read-only study of the implementations below to answer the
open hosting questions; this is study only, and the product path stays an
explicit own implementation with no GPL or Anti-996 code copied. Open
questions: taskbar-child versus topmost-overlay hosting and why spike 3
failed (native `SetParent` error 87, direct child creation error 5);
z-order above the app list; tray left-edge discovery on primary and
secondary taskbars; Explorer restart, auto-hide, DPI and fullscreen behavior.

| Upstream repo | Commit | License | Study note |
| --- | --- | --- | --- |
| [zhongyang219/TrafficMonitor](https://github.com/zhongyang219/TrafficMonitor) | 930f175 | Anti-996 License 1.0 | study only, no code reuse |
| [mgvs/winstats](https://github.com/mgvs/winstats) | 9ba25e8 | MIT | Rust, Shell_TrayWnd child, per-pixel alpha |
| [Radeyun/taskbar-player](https://github.com/Radeyun/taskbar-player) | 41d564a | MIT | UpdateLayeredWindow child + topmost fallback, SetWinEventHook |
| [hypnguyen1209/taskbar-monitor](https://github.com/hypnguyen1209/taskbar-monitor) | 821d751 | GPL | study only; tray-edge discovery |
| [mechanicwb2-hub/now-playing-taskbar-widget](https://github.com/mechanicwb2-hub/now-playing-taskbar-widget) | a60265e | MIT | external topmost overlay, multi-monitor/auto-hide/DPI |

Owned UI still needs lifecycle handling: an overlay has to rediscover taskbar
rectangles/visibility and avoid overlapping shell controls, with no reserved slot.
An AppBar has the public positioning contract: taskbar stays outermost on a shared
edge, ABN_POSCHANGED triggers query/set, ABN_STATECHANGE reports auto-hide changes;
only one auto-hide AppBar per monitor edge can register, potentially conflicting
with the taskbar ([AppBars][appbars], [SETAUTOHIDEBAREX][autohidebarex]).
Both need [WM_DPICHANGED][dpi-change], theme/high-contrast invalidation and display
topology recovery. After Explorer restart, rediscover overlay anchors or
re-register/re-query the AppBar on [TaskbarCreated][taskbar-doc]; stale HWNDs must
not be reused. These are implementation obligations, not validated recovery claims.

### Full-pager rendering (options 1 popup / 2 AppBar cells)

- Geometry mapping: scale each output's virtual workspace work area into
  its numbered cell; draw window rects clipped to the cell; 1px light
  border + centered icon per rect; active workspace gets the active
  color plus the shared legend (number + outline/check + text, never
  color-only). Unresolved: tiny-icon omission vs collision when many
  small windows share a cell. Hidden workspaces use stored rects, not
  live minimized/offscreen rects.
- Candidate icon chain per window HWND (research-only; fallback is caller-owned):
   1. WM_GETICON (ICON_SMALL2, ICON_SMALL, then ICON_BIG) via SendMessageTimeout with
     SMTO_ABORTIFHUNG and short timeout; plain SendMessage can block on
     hung windows ([WM_GETICON][wm-geticon],
     [SendMessageTimeout][sendtimeout]).
  2. GetClassLongPtr GCLP_HICON / GCLP_HICONSM. Message-returned and
     class handles are BORROWED: never DestroyIcon them; destroy only
     owned handles (CreateIconIndirect / CopyIcon /
     CreateIconFromResourceEx without LR_SHARED)
     ([DestroyIcon][destroyicon], [About Icons][about-icons]).
  3. SHGetFileInfo SHGFI_ICON|SHGFI_SMALLICON on the owning exe path
     ([SHGetFileInfo][shgetfileinfo]); returned HICON is owned and must
     be DestroyIcon-ed; call from a background thread with COM
     initialized, it can block UI.
  4. ApplicationFrameHost caveat: the top-level HWND may belong to the
     host, so exe lookup can yield the host. Window AUMID via
     SHGetPropertyStoreForWindow + PKEY_AppUserModel_ID is a CANDIDATE
     identity (property may be absent; frame-vs-child ambiguity)
     ([SHGetPropertyStoreForWindow][propstore],
     [AppUserModelIDs][appids]); prefer resolving the icon through
     IShellItemImageFactory on the AppsFolder item
     ([GetImage][getimage], [FOLDERID_AppsFolder][knownfolder]) before
     any manifest-logo scraping heuristic.
- Paint choice:

| Path | Cost / machinery | Fit for pager cells |
| --- | --- | --- |
| GDI double-buffer (compat DC + DIB, one blit on WM_PAINT) | Smallest; per-monitor DPI + theme invalidation still required | Sufficient for flat rects/icons/borders; recommended start |
| Direct2D | Device/context lifetime + device-lost handling | No established need at this fidelity; adds machinery |
| Composition (DirectComposition/DWM) | Swap-chain/visual lifetime handling | No established need; live thumbnails explicitly out of scope |

- Refresh costs (no game-zero-cost guarantee): event/diff-gated repaint
  on snapshot, urgency, DPI/theme and icon arrival; coalesce bursts;
  cache icons/bitmaps keyed by HWND identity with lifetime
  invalidation; release replaced handles; no per-frame timers, no
  animation while hidden. Prefer the existing fullscreen observer; any
  additional detection/refresh cost must be measured, not presumed free.
- Rust API per [principles](../../principles.md) Unsafe Code: evaluate
  maintained safe UI wrappers first (winit/egui optional if a window is
  needed); `windows` is the expected projection where native calls are
  needed, and many of its calls are themselves unsafe, so keep them in a
  minimal reviewed module. Do not migrate the existing windows-sys tray
  path gratuitously; extend it.
- Layering ([principles](../../principles.md)): layout, state and
  clear-on-visit in shared core; Windows layer owns HWND/icon
  ingestion, geometry, paint.

### Gaming constraints (research-only, unvalidated)

Per [principles](../../principles.md) Gaming Compatibility: hide OWN UI
on an output with exclusive/borderless fullscreen on THAT output only;
never topmost over the game; no focus/input touches; stop painting,
animation and icon lookup while hidden; resume on close. ABN_FULLSCREENAPP
lowering ([AppBars][appbars]) is one signal, not borderless proof.
Hide-vs-reserve tradeoff is an open research constraint, not finalized
behavior: hiding must not resize the game or flap the work area, and
whether to ABM_REMOVE vs hide-keep-reservation is validation TBD.

### Initial recommendation (historical; superseded by live spike)

- Stage 1: tray icon + popup pager (option 1). Smallest unsafe delta,
  reuses TaskbarCreated/change-gated sync, no reservation, no
  unsolicited UI by design intent; per Simplicity principle.
- Stage 2: if always-visible cells are required, pilot ONE owned
  adjacent AppBar pager (option 2) on the primary output; bottom edge
  matches the requested target, other edges stay possible; generalize
  per-monitor only after per-output game-hide validation.
- Stage 3: external-bar IPC (option 5) only if the user opts into a
  third-party bar dependency, not the default.
- Options 3 and 4 are recommended against for product use on the above
  evidence; final selection stays with the user.

### Initial questions (historical; answered by later user decisions)

- Accept an adjacent strip instead of literal in-taskbar embedding?
  Recommended: yes; it preserves pager visuals through supported APIs.
- Accept tray+popup for initial dogfooding? Recommended: yes.
- If AppBar pilot: bottom edge adjacent to taskbar, primary output
  first? Recommended: yes, matches the requested target.

### Later validation needs

- Confirm icon stays out of overflow only via user pinning (no force-pin API).
- Confirm popup open/focus behavior beside fullscreen games on each OS build.
- Confirm C click activation side effects and D fullscreen yield before use.
- Confirm E bar choice and who owns its install/config.
- Fullscreen hide proof per output on Win10 + Win11 builds (exclusive
  AND borderless); popup never auto-opens over games; no work-area flap.
- Per-monitor AppBar reserve/z-order beside taskbar auto-hide,
  secondary taskbars, DPI change, Explorer restart.
- Icon chain timing/failure rates (hung windows, UWP AUMID path) with
  cache-hit logging; no icon failure may block tiling.
- LOC figures are rough engineering estimates, no precision promised.

## Live spike 2026-10-14

### Product decisions

- Pager inside every output's taskbar, showing only that output's workspaces:
  numbered cells, darker empty cells, lighter miniature window rectangles with
  light 1px borders and centered app icons, active workspace in an active color.
- Visualization plus click-to-switch; the existing tray icon stays for
  settings/toggles (user 2026-10-14).
- AppBar rejected: work-area reservation interacts with maximized windows.
  Injection rejected. An owned `SetParent` child was the feasibility candidate.

### Conditions and evidence

- Physical Windows 11 build 26300.9457, one scaled-DPI primary output, ordinary
  integrity; no secondary taskbar present. No taskbar settings changed.
- Throwaway Cargo/GDI prototype, baseline revision `977e039`; native PMv2
  rectangles, taskbar UIA inspection, taskbar-filtered WinEvents, bounded
  disposable Notepad/Paint/Calculator/helper churn, and foreground borderless
  fullscreen helper. Desktop pixel samples were clipped to the pager rectangle.
- Evidence: baseline/final native+UIA snapshots, 30-second event/poll record,
  child placement/hit-test/paint record, and corrected fullscreen record with
  embedded/floating pixel samples. App lifecycle summaries were reconstructed
  from recorded command outputs; temporary raw records are outside the repo.

| Question | Observed result / limit |
| --- | --- |
| Structure | `Shell_TrayWnd` hosts a full-width `Windows.UI.Composition.DesktopWindowContentBridge`, `TrayNotifyWnd`, and legacy `ReBarWindow32`/`MSTaskSwWClass`/`MSTaskListWClass`. UIA exposes `TaskbarFrame`, six `NotifyItemIcon` elements and four `SystemTrayIcon` elements, including Clock. Clock bounds are available through that UIA leaf, not a separate clock HWND here. Legacy `Start` is invisible; current Start/Widgets/app-button UIA bounds were not isolated. An InputSite provider rectangle disagreed with the physical root rectangle. No secondary evidence. |
| Movement / events | Native tray left edge: 0px movement; legacy app-list right edge: 55px range during app churn. Tray-adjacent moved less for these measured bounds; actual XAML app-icon content movement remains unverified. Six `EVENT_OBJECT_LOCATIONCHANGE` callbacks, all at one originating tick, came from ReBar/MSTaskSw/MSTaskList HWNDs; delivery delay rounded to 0ms. 578 polls at roughly 50ms cadence observed anchor geometry, costing about 70us average / 296us maximum per poll. This establishes a working bounded signal, not reliable coverage or end-to-end latency for every layout change. UIA property events and notification-icon churn were not established. |
| Embedded child | A 216px-wide child immediately left of `TrayNotifyWnd` had verified parent and physical rectangle, matching parent/child DPI, and 3/3 `WindowFromPoint` hits. Posted messages changed local demo selection from cell 0 to 1; no physical click or product workspace switch was tested. GDI paint ran, but the composed crop showed taskbar background without the expected cells/borders/text. Visible hosting failed; app-icon rendering was not proven. Native sibling order does not explain the compositor result; XAML coverage is only a candidate explanation. |
| Fullscreen | The helper became foreground and covered its output. Taskbar and child retained `WS_VISIBLE` and their rectangles, but the helper won all three hit tests and the crop showed only helper pixels. Thus they were occluded, not API-hidden. The same-sized floating topmost comparator visibly painted over fullscreen. Because the child was already invisible at rest, this does not prove that a visibly working child would follow taskbar hiding. |
| Stability / cleanup | No Explorer identity change or taskbar geometry/style drift observed. Prototype windows were destroyed, disposable apps/helpers exited, and the final nine-child taskbar set and UIA bounds matched baseline, apart from clock text. The app-list edge returned to baseline. Calculator needed exact-owned-process forced cleanup; its preexisting frame host was preserved. No post-cleanup pixel-equivalence sample or physical flicker/input acceptance was obtained. |

### Recommendation and residual risks

- **Do not adopt the tested embedded-child approach yet.** Tray-adjacent is
  the better measured anchor, but successful parenting, painting and hit-testing
  did not produce a visible pager. This is the feasibility blocker; no product
  hosting mechanism was selected. AppBar and injection remain rejected.
- A floating topmost fallback fails the fullscreen requirement as tested.
  No automatic fallback over fullscreen/game windows is justified.
- Remaining gaps: unsupported shell/build coupling; XAML composition and
  invisible hit-test targets; actual app-content bounds and crowded-taskbar
  collision handling; secondary taskbars; DPI changes/mixed DPI; Explorer/display
  lifecycle; event coverage; left/center alignment; auto-hide; exclusive and
  borderless games. Physical taskbar appearance/input remains user verification.
- Harness corrections preceded accepted results: parent-return/coordinate
  handling was fixed, and an unpainted comparator capture was superseded by a
  message-pumped capture. Separate-process helper recovery restored a bounded
  nudge while the main controller was alive; post-controller-exit recovery was
  not proven. These are bounded observations, not production recovery acceptance.

### Live spike 2

**Visible layered-child rendering was demonstrated; repeatability is now
blocked.** Follow-up conditions: physical Windows 11 build 26300.9711,
one scaled-DPI primary output, ordinary integrity, no secondary taskbar.

#### Working technique

- `Shell_TrayWnd` had extended style `0x88`, without
  `WS_EX_NOREDIRECTIONBITMAP`. Its full-width
  `Windows.UI.Composition.DesktopWindowContentBridge` had `0x200000`, with
  that flag. Explicit native `HWND_TOP` ordering alone did not expose plain
  GDI cells, even with the child ahead of the bridge in native sibling order.
- Create a captionless owned window with `WS_EX_LAYERED | WS_EX_TOOLWINDOW |
  WS_EX_NOACTIVATE`; `SetParent` into the taskbar; replace `WS_POPUP` with
  `WS_CHILD | WS_VISIBLE`. Map the tray-adjacent screen origin to parent client
  coordinates; use `SetWindowPos(... HWND_TOP ...)` with
  `SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED`. Paint only on changes
  through ordinary GDI `WM_PAINT`/`BeginPaint`/`EndPaint`.
- Both `SetLayeredWindowAttributes(..., 255, LWA_ALPHA)` and magenta
  `LWA_COLORKEY` produced visible cells. The opaque-alpha path is the simplest
  candidate for fully painted pager cells. The latter mirrors TrafficMonitor's
  `TaskBarDlg.cpp` `ApplyWindowTransparentColor` GDI path; its DirectComposition
  branch is separate. TrafficMonitor sources were inspected without pinning
  an upstream revision; this is not a TrafficMonitor runtime compatibility test.
- Layered children require the Windows 8+ compatibility manifest. The prototype
  embedded `app.manifest` through MSVC `/MANIFEST:EMBED` + `/MANIFESTINPUT`,
  declaring Win8/8.1/Win10 `supportedOS`, `asInvoker`, and PMv2 `dpiAwareness`.
  The working probe's embedded `RT_MANIFEST` was extracted and its launch
  succeeded. The repository currently has no custom `.manifest`/`build.rs`
  or manifest-embedding dependency in `tiler-windows`; product adoption needs
  an explicit compatibility-manifest build change.
- Evidence capture must include layered windows: `SRCCOPY | CAPTUREBLT`.
  Corrected captures showed 4,060 red, 4,060 green and 4,060 blue pixels plus
  780 border pixels in the 216x60 diagnostic child, matching the floating
  positive control. Initial layered negatives omitted this capture flag;
  mixed layered APIs and prematurely released swap-chain resources also
  invalidated exploratory negatives. No actual DirectComposition target was
  tested, and none was needed for the successful layered-GDI run.

#### Behavior and cost evidence

| Check | Result / limit |
| --- | --- |
| App icons / visuals | One real Character Map icon obtained through bounded `WM_GETICON` (big-icon fallback), borrowed without destruction, and drawn centered with `DrawIconEx`; composed pixels changed. A second distinct icon and icons inside miniature window rectangles remain unproven. Packaged Notepad/Paint launch delegation complicated ownership; Calculator/ApplicationFrameHost icon resolution remains a separate gap, not solved by taking the host's icon. |
| Cell mouse input / DPI | Three absolute virtual-desktop `SendInput` clicks delivered nine injected events and three `WM_LBUTTONDOWN` messages; local selection changed and its tint composed. Not a product workspace-switch test. Parent/child DPI matched and `AreDpiAwarenessContextsEqual(PMv2)` passed at the existing scale. Fixed-size diagnostic geometry does not prove dynamic or mixed-DPI correctness. |
| Fullscreen | With the working visible child retained, a foreground borderless owned helper occluded all pager pixels and won all three hit tests. The child reappeared after helper restoration without a floating fallback or explicit pager hide. `WS_VISIBLE` stayed true, so that flag is not a fullscreen safety signal. Exclusive fullscreen, games and auto-hide remain untested. |
| Shell interaction | A Start-area mouse click was injected, but the open check matched a generic visible CoreWindow rather than an identity-bound Start host. Reliable Start-click acceptance and an owned app's taskbar-button click remain open; the app-button test was deferred when its bounds were not resolved. |
| Sustained redraw / cost | A 32-second Character Map churn run reported no disappearance in 33 sparse samples, with eight paints. Prototype CPU: 15ms over 10s idle (about 0.15% of one core), 94ms over 32.8s active (about 0.29%). Average paint 1.55ms, maximum 2.80ms. Paint invalidation was change-driven, but the harness woke its message pump every 10ms. Idle paint count was not retained; these are rough prototype costs, not zero-idle or continuous no-flicker proof. GPU/DWM costs were not measured. |

#### Repeatability blocker, cleanup and recommendation

- Later validation stopped at `SetParent` with projection HRESULT
  `0x80070057` (`E_INVALIDARG`). Native last-error reset/readback was missing,
  so this does not establish an Explorer-side error or its cause. A rerun of
  the hash-identical working probe also failed its child factory/verification;
  even its floating positive-control capture lacked the expected colors.
  Successful rendering remains valid for the earlier run, not repeatably proven.
- Native child enumeration also returned only three descendants despite all
  nine baseline handles being individually alive with unchanged parents,
  classes, rectangles and Explorer identity. Its cause is unknown; a factory
  that relies solely on that enumeration can falsely reject parenting.
- Final read-only checks found no prototype/app residue; baseline taskbar
  handles and Explorer/Terminal process identities were preserved. No shell
  repair was performed. Full visual/input restoration remains a physical check.
- Earlier separate-controller nudge/recover restored the helper geometry but
  checked only class/window existence. Strong identity-bound recovery and
  post-controller-exit acceptance were not established; the strengthened final
  code never reached those phases. Earlier behavior records and later failed
  binaries must not be treated as one verified artifact.
- **Do not begin product implementation until parenting and the pixel positive
  control are repeatable.** Once re-proven, prefer the manifested opaque layered
  GDI child at the tray-adjacent anchor, with no topmost floating fallback.
  Close two-icon, shell-click and exact-recovery gaps before broader use.
- Rough implementation estimate: **800-1,400 handwritten Windows-specific LOC**
  in a new native pager module, not `tiling_sys.rs`: host/paint/DPI, icon
  fetch/cache, input-to-existing-workspace requests, fullscreen guard and
  identity/lifecycle handling. Tests and any shared-core additions are separate.
  Keep unsafe API effects contained; no DirectComposition dependency is justified
  by the successful run. Secondary taskbars, crowded layouts, display/Explorer
  lifecycle, alignment/auto-hide, changing DPI and real-game safety remain open.

### Live spike 3

Historical child-host blocker; the external product overlay below does not
depend on resolving it.

**No-go for product implementation: 0/10 fresh-process taskbar-host passes;
the native failure is attributed, but its root cause remains unknown.** Checked
2026-10-15 on physical Windows 11, ordinary medium integrity, at the existing
DPI. Only the primary taskbar was tested; no shell settings changed.

#### Native diagnosis and repeatability

- A new raw `windows-sys` prototype embedded Win8/8.1/Win10 `supportedOS`,
  `asInvoker` and PerMonitorV2 through `/MANIFEST:EMBED` + `/MANIFESTINPUT`.
  It used the opaque layered-GDI technique described in spike 2; no injection
  or shell modification was used.
- Ten consecutive fresh processes each cleared last-error immediately before
  raw `SetParent` and read it immediately afterward. All returned NULL with
  **87 (`ERROR_INVALID_PARAMETER`)**, and `GetParent` did not equal the taskbar.
  This is not the successful NULL-previous-parent case, which requires zero
  last-error. The earlier projected `E_INVALIDARG` is consistent with this
  native error; the projection alone did not cause these ten failures.
- Direct `CreateWindowExW(WS_CHILD, hWndParent = Shell_TrayWnd)` failed with
  **5 (`ERROR_ACCESS_DENIED`)** in every process. Taskbar HWND, owning process
  lifetime, rectangle, styles and DPI remained stable. No stale taskbar handle
  was observed.
- The initial floating control, positioned over the taskbar, also failed all
  ten pixel checks and hit-tested to the taskbar. It was not layered, so those
  runs are **ten raw-parenting negatives, not ten render-qualified cycles**.
  Later placement/pixel negatives after failed parenting were discarded: the
  harness had incorrectly applied client coordinates to an unparented window.
- One corrected fresh-process diagnostic used a layered, alpha-255 control
  within the desktop work area. `SRCCOPY | CAPTUREBLT` found **4,060 red,
  4,060 green and 4,060 blue pixels**, plus 780 border pixels, and the control
  won its hit test. Layered painting and capture therefore work independently
  of taskbar hosting. This does not establish why the over-taskbar control failed.
- That corrected diagnostic compared the actual taskbar and owned-window DPI
  contexts, not just the caller's thread: both were PMv2. Explicitly setting
  the thread to the taskbar's context and restoring it afterward succeeded.
  Setting `WS_CHILD` **before** raw `SetParent` still returned 87; the earlier
  **after** order had already failed ten times. Direct child creation under
  the matched context still returned 5. These tested style orders and a DPI
  context mismatch do not explain the failure.
- Token readbacks showed medium integrity for both processes and equal session
  ids. The caller's station/desktop was the interactive default. These checks
  do not establish the access-control mechanism behind error 5 or rule out all
  UIPI/desktop restrictions. No privilege escalation or shell repair was attempted.
- Child enumeration remained inconsistent: recursive enumeration returned three
  descendants while a direct sibling walk returned none. Independent read-only
  checks verified all nine previously recorded descendant handles individually,
  with their original classes, parents, owner and geometry. Enumeration counts
  were not accepted as ownership or cleanup proof.

**Result:** 0/10 initial taskbar-host passes; one additional corrected,
render-qualified diagnostic also failed parenting. No working host existed on
which to run ten successful create/render/verify/destroy cycles. Error 87/5 is
now native evidence; **the underlying cause remains unknown**. No shell-hardening
or servicing-change cause is established.

#### Content, interaction, robustness and cost

| Check | Spike 3 result / limit |
| --- | --- |
| Three cells / real window rectangles | Diagnostic color cells rendered in the work-area control. Real-window-scaled miniature rectangles were not tested because taskbar hosting failed. |
| Two distinct icons / Calculator | Not tested. Two-icon rendering and Calculator CoreWindow/AUMID/package-icon resolution remain open; no generic glyph was demonstrated as a Calculator fallback. Spike 2's single Character Map icon is earlier evidence only. |
| Cell clicks | Not run on the failed host. Spike 2's injected selection result is not a current artifact result or a product workspace-switch proof. |
| Start and owned app button | Not run; identity-bound Start open/close and app-button activation remain unverified. |
| 60-second churn / 100ms sampling / shell redraw | Not run; failed parenting prevents a meaningful child visibility/position or redraw-survival test. Ten short negative factory runs are not sustained robustness evidence. |
| Fullscreen occlusion and return | Not rerun. Earlier borderless-helper evidence remains valid only for spike 2's working artifact; games/exclusive fullscreen remain open. |
| Theme | Existing settings preserved; theme contrast and theme-change rendering were not tested. |
| 30-second idle CPU / per-paint timing | Not measured for a working pager. Earlier prototype costs are not spike 3 cost evidence. |

#### Cleanup, manifest and implementation recommendation

- All diagnostic processes exited. The corrected control and style-order window
  each returned successful `DestroyWindow`, immediately followed by false
  `IsWindow`; direct child creation produced no HWND. Final independent checks
  found no prototype top-level windows or processes, and the nine known taskbar
  descendants remained unchanged. Taskbar root identity/geometry/styles/DPI
  matched baseline. Process exit and per-handle checks, rather than the
  undercounting enumeration, support cleanup. No physical appearance/input
  acceptance is inferred.
- `crates/tiler-windows` still has no application `.manifest`, `build.rs` or
  manifest-embedding dependency. Its `ensure_pm_v2` in `tiling_sys.rs` sets or
  verifies runtime PMv2; that does **not** declare Windows 8+ `supportedOS`.
  Product adoption needs an embedded compatibility manifest with PMv2, verified
  in the built executable, in addition to a proven host factory.
- **Do not begin product hosting implementation.** The exact previously working
  candidate remains: manifested `WS_EX_LAYERED | WS_EX_TOOLWINDOW |
  WS_EX_NOACTIVATE`, `WS_CHILD | WS_VISIBLE` under the taskbar, tray-left anchor
  mapped into parent client coordinates, `HWND_TOP` without a floating fallback,
  GDI change-driven paint and `SetLayeredWindowAttributes(LWA_ALPHA, 255)`.
  It is a historical working technique, not a repeatable spike 3 result.
- If hosting is re-established, propose new `pager.rs` and `pager_sys.rs` modules
  in `crates/tiler-windows`: portable snapshot/layout/hit-testing and contained
  native hosting/painting/icon ownership respectively. Keep settings in the
  existing tray path; do not add pager effects to `tiling_sys.rs`. Cache icons
  with bounded asynchronous lookup and identity invalidation; release owned
  copies, never borrowed `WM_GETICON`/class handles. Coalesce snapshot changes,
  stop work while hidden and revalidate/recreate hosts after shell/topology changes.
- Required owner data: **per-output ordered workspace list and stable tokens,
  active workspace id, output/work-area rectangle, saved window rectangles and
  HWND + process-lifetime identities per workspace, and workspace/window urgency**.
  Include snapshot generation for stale click rejection; route clicks through
  existing workspace requests rather than making the UI an authority. Hidden
  workspaces need stored geometry. Urgency acquisition remains a separate gap.
- Retain the provisional **800-1,400 Windows-specific handwritten LOC** estimate
  for a proven host/render/icon/input/lifecycle path, excluding tests, shared-core
  snapshot additions and urgency acquisition. Unknown hosting diagnosis and
  unsupported shell compatibility have no bounded estimate. Remaining risks:
  crowded-taskbar collisions, actual app-content bounds, secondary taskbars,
  changing/mixed DPI, alignment/auto-hide, shell/display lifecycle, accessibility,
  physical clicks and real-game safety.
- Evidence baseline `b912400`; initial ten-cycle executable SHA-256
  `b22a5ab2e69de58a32fc4e6ebc219e06aaa2151c3cb2d1724f4ab1b3afd96f12`;
  corrected diagnostic SHA-256
  `4b62e63178f552f5b2add68ed6151bbb5c3b27915921d7e6e1edc93b12f058d7`.
  Raw source, per-cycle JSON, composed captures and independent cleanup checks
  remain in temporary evidence outside the repository. No product code changed.

## Product overlay 2026-10-16

User-approved 2026-10-16 hosting: external, process-owned
`WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE` popup, positioned over
the taskbar, anchored left of the tray, with GDI painting. No Explorer
`Shell_TrayWnd` child parenting, injection, AppBar,
placeholder icons or shell mutation. Unlike the child-layered candidate, this
route needs no Windows-8 child-window compatibility manifest; runtime PMv2 is
verified before geometry and window creation.

- A bounded overlay-only probe passed native active/empty/border pixel samples,
  hit testing, unchanged foreground, tray exclusion and exact destruction.
  An initial capture failed because of virtualized coordinates; repeating in
  physical PMv2 coordinates passed. `SRCCOPY | CAPTUREBLT` remains required.
- Production modules are `pager.rs` (portable layout/snapshot/gates),
  `pager_sys.rs` (native window/GDI lifecycle) and `tiling_sys/pager_host.rs`
  (owner integration). Workspace selection reuses the existing resolver.
  All workspace cells and managed window miniatures render without a window
  count cap; actual observed frames take precedence over stored hidden frames.
- The primary `TrayNotifyWnd` left edge, taskbar visibility/geometry and DPI
  are read every pump, not cached for seconds. Cells end two physical pixels
  left of the tray; app-button crowding never changes the layout. Invalid,
  off-screen or physically insufficient strip geometry refuses with a log.
  User-approved 2026-10-16: if fixed-width cells exceed the entire tray-left
  span, do not draw the pager; log the reason and retain the tray number/menu.
  This is not hiding due to app-icon crowding; no alternate treatment is open.
- Settings exposes default-on `core.taskbar_pager`, hot-applied. Cell geometry
  and fonts scale with DPI; display/settings/DPI broadcasts immediately hide
  stale pixels before fresh layout. Fullscreen, managed fullscreen, elevated
  foreground and unreadable safety facts hide the pager and drop queued clicks.
- Shell discovery/retry is ongoing, with `TaskbarCreated` handling and periodic
  safe topmost reassertion. Missing Explorer does not permanently disable it.
  Secondary bars are discovered but refuse `secondary-tray-unproven`: no
  clock/tray boundary is guessed. Non-bottom taskbars also refuse.

### Product live evidence

Single-output Windows 11, ordinary medium integrity and fractional scaling;
owned Notepad/Paint windows only, keyboard takeover and Mouse Snap prevention
disabled. Executable/hash/process identities and scoped baseline/cleanup were
bound in temporary evidence; no unrelated windows or saved settings changed.

- Captures proved numbered active/empty cells, lighter miniatures and light
  one-pixel borders, exact tray-left gap and nonactivating tool-window styles.
- Messages to the owned pager switched both ways through `edge:pager`; native
  status, hide/reveal and active-colour changes agreed. This proves the window
  message/dispatch path, not physical mouse delivery.
- Creating, sending and closing owned windows changed miniature geometry and
  workspace count live; hidden-workspace cells retained miniatures. Interior
  pixel samples agreed with independently read real window rectangles.
- A sustained check across the topmost-reassert interval retained identical
  pixels/geometry. An owned-window `TaskbarCreated` message retained the pager;
  Explorer itself was not restarted.
- Managed fullscreen hid the pager; leaving restored it. Graceful stop and
  exact-owner emergency stop left no pager, and independent restoration and
  owned-app cleanup left a clean ledger; saved settings remained unchanged.
- Locked native gates: 2,134 passed, zero failed, one pre-existing ignored;
  build, fmt, strict scoped all-target clippy and diff check passed.

Still pending: actual Explorer restart; physical mouse/game/anti-cheat
acceptance; DPI/auto-hide setting changes; secondary taskbar anchor discovery
and physical multi-output checks; non-bottom taskbars; very large workspace
counts exceeding the entire available strip width; accessibility/theme/cost
measurement. Live opt-out was not attempted against shared saved settings;
schema, hot-apply and queued-click refusal have offline coverage. The MVP
omitted icons rather than using generic substitutes. See the
[implementation record](../../changes/archive/windows-taskbar-pager.md).

### Miniature icon implementation 2026-10-16

- The product painter is opaque GDI, not the earlier layered child prototype.
  A dedicated worker draws copied/owned HICONs onto a gray-backed top-down
  32-bit DIB with DrawIconEx, flattens alpha to opaque BGRA, and releases native
  handles. WM_PAINT only StretchDIBits-copies shared bytes. Premultiplied alpha
  for UpdateLayeredWindow is not needed by this unchanged hosting path.
- Classic chain: WM_GETICON SMALL2/SMALL/BIG through 50 ms
  SendMessageTimeout/SMTO_ABORTIFHUNG calls, class small/big icons, then
  ExtractIconExW on the verified owning executable. Borrowed handles are copied,
  never destroyed directly. No shell extension/COM image lookup was added.
- A process-held HWND/pid/creation check surrounds lookup. Cache entries prune
  with membership, hits refresh after 10 seconds (no Windows icon-change event),
  misses after 5 seconds (apps often set icons after window creation). Only
  sufficiently large served miniatures enqueue work; queues deduplicate
  in-flight requests without arbitrary entry caps, idle workers sleep, and
  suppression rejects old results. User-approved 2026-10-16 cleanup also uses
  fixed five-second overlay/worker-spawn retries to recover without busy-looping,
  and retains only the latest pending pager click rather than replaying old clicks.
- User-approved 2026-10-16 icon sizing caps at 16 logical px and omits below 8 logical px, preserving a
  1px border plus 1px padding per side. Unavailable icons leave the rectangle
  unchanged. DPI-aware placement stretches a cached 32px raster; higher scales
  can upscale. Existing workspace labels remain painted last.
- Packaged apps can work through obtainable window/class icons. Exact
  ApplicationFrameHost child/AUMID/AppsFolder artwork is deferred by the user
  2026-10-16 until Calculator has been seen physically;
  host exe fallback is omitted rather than mislabelled as the real app icon.
- ExtractIconExW is not timeout-bounded and can stall the single artwork worker;
  it cannot block tiling/painting or shutdown. Same-process HWND reuse remains
  limited by the existing process-lifetime key. Visual alpha edges, resource
  stability, hung-client and icon-change behavior still require native evidence.
- Locked native gates passed: 2,158 tests passed, zero failed, one ignored,
  including 13 new icon tests; build/fmt/strict clippy/diff check passed. This is
  offline evidence only. See the active
  [icon record and acceptance plan](../../changes/windows-taskbar-pager-icons.md).

[taskbar-ext]: https://learn.microsoft.com/en-us/windows/win32/shell/taskbar-extensions
[win11-specs]: https://www.microsoft.com/en-us/windows/windows-11-specifications#table4
[ideskband2]: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl/nn-shobjidl-ideskband2
[widget-prov]: https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-providers
[widgets-help]: https://support.microsoft.com/en-us/windows/stay-up-to-date-with-widgets-in-windows-7ba79aaa-dac6-4687-b460-ad16a06be6e4
[pin-taskbar]: https://learn.microsoft.com/en-us/windows/apps/develop/windows-integration/pin-to-taskbar
[kb5065789]: https://support.microsoft.com/en-us/servicing/os/windows-11/2025/09/september-29-2025-kb5065789-os-builds-26200-6725-and-26100-6725-preview
[appbars]: https://learn.microsoft.com/en-us/windows/win32/shell/application-desktop-toolbars
[autohidebarex]: https://learn.microsoft.com/en-us/windows/win32/shell/abm-setautohidebarex
[wm-geticon]: https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-geticon
[sendtimeout]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendmessagetimeoutw
[destroyicon]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-destroyicon
[about-icons]: https://learn.microsoft.com/en-us/windows/win32/menurc/about-icons
[shgetfileinfo]: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shgetfileinfoa
[propstore]: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shgetpropertystoreforwindow
[appids]: https://learn.microsoft.com/en-us/windows/win32/shell/appids
[getimage]: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishellitemimagefactory-getimage
[knownfolder]: https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid
[yasb-config]: https://docs.yasb.dev/preview/configuration
[yasb-barpy]: https://github.com/amnweb/yasb/blob/28e4ba5b/src/core/bar.py
[yasb-301]: https://github.com/amnweb/yasb/discussions/301
[tm-basedlg]: https://raw.githubusercontent.com/zhongyang219/TrafficMonitor/master/TrafficMonitor/TaskBarDlg.cpp
[tm-win11dlg]: https://github.com/zhongyang219/TrafficMonitor/blob/master/TrafficMonitor/Win11TaskbarDlg.cpp
[tm-maindlg]: https://github.com/zhongyang219/TrafficMonitor/blob/master/TrafficMonitor/TrafficMonitorDlg.cpp
[tm-classic]: https://github.com/zhongyang219/TrafficMonitor/blob/master/TrafficMonitor/ClassicalTaskbarDlg.cpp
[dpi-change]: https://learn.microsoft.com/en-us/windows/win32/hidpi/wm-dpichanged
[taskbar-doc]: https://learn.microsoft.com/en-us/windows/win32/shell/taskbar
[ep-wiki]: https://github.com/valinet/ExplorerPatcher/wiki/ExplorerPatcher%27s-taskbar-implementation
[wh-styler]: https://windhawk.net/mods/windows-11-taskbar-styler
[wh-876]: https://github.com/ramensoftware/windhawk/issues/876
[wh-2374]: https://github.com/ramensoftware/windhawk-mods/discussions/2374
[roundedtb]: https://github.com/RoundedTB/RoundedTB
