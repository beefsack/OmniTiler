# Windows taskbar workspace indicator: options

Initial research checked 2026-10-14; no production code or host settings changed.
Later product decisions and [live evidence](#live-spike-2026-10-14) supersede
the initial tray+popup/AppBar recommendations below.

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

### Prior art (observed docs/source; gaps noted, not universal claims)

| Project | Observed | Gaps / caveats |
| --- | --- | --- |
| TrafficMonitor ([TaskBarDlg][tm-basedlg], [Win11 dlg][tm-win11dlg], [main dialog][tm-maindlg]) | SetParent into Shell_TrayWnd/SecondaryTrayWnd on Win11; [Win10 path][tm-classic] reparents into ReBarWindow32 and shrinks MSTaskSwWClass. Start/TrayNotifyWnd geometry, 88px secondary-clock fallback, 22621 touch-height workaround. Main timer rechecks DPI/position, samples taskbar background, and reopens after secondary-count changes; TaskbarCreated closes/recreates child and re-adds tray icon | No dedicated child fullscreen hide verified; foreground fullscreen polling hides its floating main window, not proof for child/fallback/tooltips. Child relies on shell auto-hide; independent coverage unverified. Fallback requests topmost when taskbar foreground |
| ExplorerPatcher ([taskbar wiki][ep-wiki]) | Reimplements Win10 taskbar for 25921+/24H2 instead of relying on the old built-in implementation; separate build-specific DLLs include 25H2. Maintainer explicitly says updates can break it and crashes can occur | Tightly Explorer-coupled; replacement implementation, not a supported host API |
| Windhawk ([taskbar styler][wh-styler]) | XAML/resource hooking into Explorer; [issue #876][wh-876] reports mods stopped after Mar-2026 KB5079473+KB5081277 with reinstall fixing it (cause undiagnosed, report only); [discussion #2374][wh-2374] notes debug symbols needed to locate hooks | Hook mechanism / breakage cause not proven here |
| RoundedTB ([README][roundedtb]) | Geometry modification, not hosting; admits auto-hide flicker, dynamic size mismatch, and Win10 split mode only on primary taskbar | Illustrates layout fragility; no pager or game-safety contract |
| YASB / Zebar / komorebi-bar | Separate-window bars, not taskbar slots; see option 5 | Fullscreen behavior per-bar config; komorebi-bar none found |

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
  1. WM_GETICON (ICON_SMALL, then ICON_BIG) via SendMessageTimeout with
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
