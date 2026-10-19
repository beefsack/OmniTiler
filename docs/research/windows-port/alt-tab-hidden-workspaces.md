# Alt+Tab and taskbar presence for hidden managed workspaces

Research only. 0.1 presentation decided User 2026-10-17 (keep `SW_HIDE`); post-0.1 mechanism open. No product behavior or host settings changed.
The [active private-cloak spike](../../changes/windows-cloak-spike.md) adds
owned-helper-only tooling and exact user-run steps (2026-10-19); no product
behavior or 0.1 decision changes. Native listing, selection and crash recovery
results are pending. User intent for that bounded investigation is retained
taskbar/Alt+Tab entries with selection switching workspace and focusing, not
pulling the window; this is not a backend-adoption decision.
Checked 2026-10-04; extended 2026-10-17 with an owned-helper live
visibility baseline (no Alt+Tab/Task View visual observation) and a
per-option presentation comparison. User decision 2026-10-17: keep `SW_HIDE`
for 0.1; post-0.1 research investigates trivial configurability (e.g.
off-screen parking); switch-versus-pull remains open in post-0.1 research.
Documentation, pinned source,
upstream reports, live readbacks and inference are distinguished below.

Presentation selection for 0.1 is decided User 2026-10-17 (keep `SW_HIDE`).
The recommendation below is a recommendation only; post-0.1 mechanism and
switch-versus-pull activation policy are open research, not selected in this survey.

## Pins

Pins: ours local `0df451e5`; GlazeWM
`5709ad0a3c7c386bbc3e38166a865ffc12937515`; komorebi
`e0709f02bfae4e503bf4640f58ee75ecbbfdbb97`. Immutable pattern:
`https://github.com/<org>/<repo>/blob/<pin>/<path>#Lx-Ly`.

**Finding:** no supported public API was established for keeping arbitrary
`SW_HIDE`-hidden foreign windows in native Alt+Tab. Parking is a public-API
candidate that preserves visibility. Private ApplicationView cloak has upstream
evidence of cross-workspace Alt+Tab, but is outside the current API constraint.
The 2026-10-17 owned-helper baseline confirms the `SW_HIDE` visibility facts
below; shell listing itself remains documented-rule inference, not a current-PC
visual observation.

## Live visibility baseline 2026-10-17 (owned helper, not a shell listing proof)

Owned `tiler-test-window.exe` (`target/debug`), PID 28764, HWND 2687672,
driven by exact-HWND commands plus narrow P/Invoke `ShowWindowAsync` on that
HWND only. All visibility, geometry and focus writes in the final leg landed
on that owned HWND; no other window was hidden, moved, restyled or activated,
and no setting or registry key was written. The helper was driven by inline
shell commands (transcript only, no log file kept); the temp
`ws-probe-hide.ps1` script is the earlier Notepad-leg template, not the
executed helper transcript.

- Base: `visible=true`, rect 200,200-840,680, `DWMWA_CLOAKED=0`.
- `SW_HIDE` leg: `visible=false`, `DWMWA_CLOAKED=0` (plain hidden, not
  cloaked), geometry retained in the snapshot. Omission from Alt+Tab/taskbar
  follows from the Chen visible-window rule (inference); no Alt+Tab, Win+Tab
  or taskbar visual was observed.
- `SW_SHOWNA` restore: `visible=true`, identical geometry.
- Helper `minimize` leg: snapshot `visible=true`, iconic rect at -32000
  (standard iconic parking); foreground stayed on the other window
  (nonactivating). Shell listing is inferred retained (visible per Chen rule);
  not visually observed.
- Helper `restore` leg: `visible=true`, original geometry back.
- `close --tag`: helper process exited; verified absent afterward
  (transcript `Get-Process` readback only).

Earlier Notepad legs (same session, before the helper): self-spawned Notepad
processes were started, but spawned-PID window attribution failed (Win11
tabbed-Notepad singleton handoff), so no window handle was ever obtained and
no visibility write was ever posted to any Notepad window. Those instances
were closed by exact PID; the close readbacks exist in transcript only, with
no preserved log files. One further Notepad (PID 37804, HWND 2163304,
started inside the probe window, absent from the pre-probe sweep) was left
running at first write-up; read-only UIA then established a single
`Untitled. Unmodified` tab with zero document text, and it was closed by
normal exact-PID `CloseMainWindow` with verified process exit and HWND
invalidation (evidence files in approved temp, not the repo). No Notepad
residue remains; the controlling terminal was never touched.

Limitation: scripted probes establish visibility/cloak/geometry facts only.
Actual Alt+Tab, Win+Tab/Task View, taskbar-button, thumbnail and Peek visuals
still need user-observed native evidence on the user's desktop.

## Shell filtering background

- Classic Alt+Tab rule (Raymond Chen, with "implementation detail, can
  change at any time" warning): for each visible window, walk to the root
  owner, then down the visible last-active-popup chain; list only if the
  walk returns to it. `WS_EX_TOOLWINDOW` counts as invisible;
  `WS_EX_APPWINDOW` counts as ownerless:
  [Which windows appear in the Alt+Tab list?](https://devblogs.microsoft.com/oldnewthing/20071008-00/?p=24863).
  Direct claim: `SW_HIDE` (`IsWindowVisible` false) fails the first test.
- Cloaking (Chen 2020): the shell can cloak a window so Win32 visibility
  probes still read visible while the user sees nothing. Windows on a
  non-current virtual desktop are cloaked this way; enumerators must add an
  explicit `DwmGetWindowAttribute(DWMWA_CLOAKED)` check:
  [How can I detect that my window has been suppressed?](https://devblogs.microsoft.com/oldnewthing/20200302-00/?p=103507).
  Cloak keeps `IsWindowVisible` true; `DWMWA_CLOAKED` reasons
  (`APP`/`SHELL`/`INHERITED`) are documented
  ([DWMWINDOWATTRIBUTE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)).
  Cloak alone does not prove Alt+Tab exclusion: native off-desktop windows
  can be listed, and Glaze's upstream report below describes global Alt+Tab.
- Taskbar membership is controllable per-HWND via public
  `ITaskbarList::DeleteTab` / `AddTab`:
  [DeleteTab](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-itaskbarlist-deletetab).
  The documented purpose is taskbar membership, not a hidden-window Alt+Tab
  inclusion contract. Glaze reports suggest interaction with Alt+Tab listing.
- Foreground switches desktop (Chen 2017): "When a window becomes
  foreground, the system switches to the virtual desktop that the window
  belongs to"; the "do not switch" guidance constrains application requests,
  not the shell's own Alt+Tab/taskbar activation path:
  [current virtual desktop?](https://devblogs.microsoft.com/oldnewthing/20171002-00/?p=97116).
- Win11 Settings > System > Multitasking > Desktops exposes taskbar and
  Alt+Tab "show all open windows" choices: all desktops / only current desktop
  ([MS support](https://support.microsoft.com/en-us/windows/experience/configure-multiple-desktops-in-windows));
  these govern native desktop membership. They do not reveal our `SW_HIDE`
  windows or represent managed-workspace membership (architectural inference).

## Options at a glance

Ordinary eligible top-level windows are assumed; app-specific shell filtering
can still apply. Complexity includes activation and recovery, not just API calls.

| Option | Status | Alt+Tab / taskbar | Recovery and complexity |
| --- | --- | --- | --- |
| Keep `SW_HIDE` | Public | Omitted / omitted (visibility facts live-proven 2026-10-17; listing is rule inference) | Existing identity-safe ledger and watcher; no added work |
| `DWMWA_CLOAK` | Public; foreign writes reported denied | Neither shell result established for this path | Uncloak/preimage recovery needed; small setter, blocked applicability |
| Native virtual desktops | Public manager has limited surface | Both governed by native all/current settings | Task View remains usable after crash; large model/lifecycle change |
| Off-screen parking | Public geometry | Both expected to remain listed (inference) | Restore original geometry with identity fencing; medium complexity |
| Private ApplicationView cloak | Undocumented COM | Global Alt+Tab reported; taskbar retained unless explicitly removed | Uncloak + taskbar restoration; medium/high complexity and OS drift |

## Option 1: retain ShowWindow(SW_HIDE)

Official. `SW_HIDE`: "Hides the window and activates another window":
[ShowWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow).
Ours posts `ShowWindowAsync` with pumped `IsWindowVisible` readback
(`post_visibility`:
`crates/tiler-windows/src/product_hide/sys/mod.rs:678-700`). Shell effect: window
leaves Alt+Tab and the taskbar. Recovery is the strongest in-tree story
(identity-gated ledger, watcher readiness and timeouts;
`crates/tiler-windows/src/product_hide.rs:1-14,38-73`, verified current).
Public API only; zero added complexity. No shell selection of an omitted window
is available; workspace navigation remains the reveal path.

## Option 2: public DwmSetWindowAttribute(DWMWA_CLOAK)

Fully documented API value: "Cloaks the window such that it is not visible
to the user. The window is still composed by DWM":
[DWMWINDOWATTRIBUTE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute),
[DwmSetWindowAttribute](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmsetwindowattribute).
The docs state no foreign-HWND restriction. The restriction is an upstream
code-comment report, not a doc: komorebi notes `E_ACCESSDENIED` for foreign
HWNDs and uses private cloak instead:
[window.rs:279-283](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window.rs#L279-L283).
Treat this as an owner-app mechanism, not an established foreign-window solution.
Own-window success would not prove foreign-window applicability. Alt+Tab/taskbar
listing and activation for this public app-cloak path remain TBD. Recovery would
need verified uncloak and prior-state preservation, not `ShowWindow` alone.

## Option 3: public IVirtualDesktopManager (native desktops as workspaces)

Official but narrow: exactly `GetWindowDesktopId`,
`IsWindowOnCurrentVirtualDesktop`, `MoveWindowToDesktop` via
`CLSID_VirtualDesktopManager`:
[IVirtualDesktopManager](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager).
No create/enumerate/switch methods in this public interface. Its intended scope
is an app moving auxiliary windows to follow its main window,
"specifically not to let a program grab all of the user's windows and
scatter them across all their virtual desktops":
[Virtual desktops are an end-user feature](https://devblogs.microsoft.com/oldnewthing/20201123-00/?p=104476).
That is own-app design guidance, not an explicit access-check specification:
[MoveWindowToDesktop](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ivirtualdesktopmanager-movewindowtodesktop)
does not document a foreign-HWND rejection rule. Foreign-window success is not
established here; do not treat the public manager as a general WM API.
Native Alt+Tab/taskbar filtering, Settings toggles, and Task View then work
natively, with foreground activation switching desktops (Chen 2017 above).
Cost: workspace lifecycle ceded to Explorer; crash residue is windows on a
non-current native desktop (Task View recoverable). Stop-time restoration would
need original desktop IDs and handle deleted desktops. Native desktop topology
and user-driven changes complicate our per-output workspace model. Public APIs
do not supply the complete managed-workspace lifecycle.

## Option 4: off-screen parking (Glaze PlaceInCorner shape)

Official plain-`SetWindowPos` geometry, no hiding API. Glaze parks hidden
windows at a monitor corner with a 1px sliver:
[platform_sync.rs:351-385](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/commands/general/platform_sync.rs#L351-L385)
(opt-in on Windows via `hide_method: 'place_in_corner'`, macOS default).
By the Chen visible-window rule it stays listed in Alt+Tab and the taskbar
in principle (visible, uncloaked; inference, untested) - so it meets the
inclusion goal, at the price of not hiding. Foreign foreground is not
definitionally "pull": Glaze switches managed workspace on focus of a
hidden-workspace window (option 5 citation), so a parked-window foreground
can likewise switch. A future implementation must choose switch versus pull;
neither is forced by parking. A sliver stays clickable; fully off-screen parking
risks app/shell repositioning. Crash recovery needs a durable original-geometry
ledger, monitor/DPI changes, maximize/restore handling and parked-window detection.

## Option 5: private IApplicationView::SetCloak (Glaze/komorebi shape)

Private/undocumented `IApplicationView::set_cloak(1, 2/0)`, not public DWM cloak:
[Glaze native_window.rs:466-491](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/native_window.rs#L466-L491),
[komorebi COM](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/com/mod.rs#L67-L100).
Both default to cloak (prior comparison, section 1). Glaze
pairs cloak with `DeleteTab`/`AddTab` unless `show_all_in_taskbar: true`
([taskbar methods](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/native_window.rs#L511-L529),
[config gate](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/commands/general/platform_sync.rs#L316-L332));
komorebi has no `DeleteTab` pairing (targeted search at pin), so retaining
taskbar entries is expected. [PR #792](https://github.com/glzr-io/glazewm/pull/792)
reports taskbar retention, not Alt+Tab behavior.

Alt+Tab evidence: [Glaze discussion #830](https://github.com/glzr-io/glazewm/discussions/830)
initially reports current-workspace-only listing; its final reply reports
`show_all_in_taskbar: true` enables global Alt+Tab. Version/config are incompletely
specified and this is a user report, not a current-PC test or Microsoft contract.
komorebi's reconciliation code explicitly anticipates Alt+Tab to hidden windows;
that supports the intended integration, not proof every cloaked window is listed.
Inactive-workspace foreground reaction is switch in both references:
Glaze switches to the focused window's workspace when a hidden-workspace
window is force-shown:
[handle_window_focused.rs:71-82](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/events/handle_window_focused.rs#L71-L82).
komorebi reconciles on Show/Uncloak via `needs_reconciliation` /
`perform_reconciliation` ("When there was an `alt-tab` to a hidden window"):
[event path](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/process_event.rs#L483-L498),
[reconciliation](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/process_event.rs#L936-L1048).
These preserve workspace membership instead of pulling the selected window.
Recovery needs identity-safe uncloak and restoration of any removed taskbar tabs;
the current SW_HIDE watcher would need extension. COM interface/flag drift and
app compatibility are additional costs; adoption needs a separate API decision.

## KDE parity evidence

Task Switcher docs: Virtual Desktops filter offers Current / All other;
unchecked means unfiltered (the effective "all"):
[Task Switcher](https://docs.kde.org/stable_kf6/en/kwin/kcontrol/kwintabbox/index.html).
[TabBox accept](https://github.com/KDE/kwin/blob/0be32096d8af3fa47eecfa585a938f91c3a27f3a/src/tabbox/tabbox.cpp#L1039-L1046)
calls `Workspace::activateWindow`; its
[desktop policy](https://github.com/KDE/kwin/blob/0be32096d8af3fa47eecfa585a938f91c3a27f3a/src/activation.cpp#L294-L313)
switches to the target desktop, brings the window to the current desktop, or
does neither. [Default](https://github.com/KDE/kwin/blob/0be32096d8af3fa47eecfa585a938f91c3a27f3a/src/options.h#L833-L835)
is switch, not pull. This source pin is not a claim about the user's installed
version/settings. Task Manager desktop filtering is separate
([Plasma Tasks](https://userbase.kde.org/Plasma/Tasks)).

Our KDE adapter uses native desktop navigation
([workspace-native.ts](../../../kwin/src/workspace-native.ts), `writeCurrent`),
so native KWin filtering/activation policies apply. Windows managed workspaces
currently cannot offer that native switcher parity. `R-WS-07` in
[reference outcomes](../../spec/reference-outcomes.md) records the gap and the
unselected Windows switch-versus-pull behavior.

## Anti-cheat implications

All public options avoid adding private shell COM; none supplies an anti-cheat
compatibility guarantee. Parking changes game geometry; native desktop switches
and hiding/cloaking can affect presentation/focus. Private cloak adds unsupported
shell integration and version risk, not evidence of a ban or process injection.
No game/anti-cheat testing was performed. Existing fullscreen/protected-window
scope constraints remain relevant (prior comparison, section 7).

## Per-option presentation comparison (2026-10-17)

Pager presentation is already delivered and is not re-decided here. "Listed
(inference)" means the Chen visible-window rule predicts listing; no current-PC
shell visual was observed for any option.

| Dimension | SW_HIDE (current) | Minimization | Off-screen parking | Native virtual desktops | Private cloak |
| --- | --- | --- | --- | --- | --- |
| Alt+Tab | Omitted (rule inference + live `visible=false`) | Listed (inference; live `visible=true` iconic) | Listed (inference) | Listed per native all/current setting | Global listing reported upstream, not proven here |
| Win+Tab / Task View | Omitted (inference) | Listed (inference) | Listed (inference) | Native, per-desktop | Unknown |
| Taskbar buttons | Omitted (inference) | Retained (inference from standard minimized-button behavior; unobserved visual) | Retained (inference) | Governed by native setting | Retained unless `DeleteTab` paired (Glaze shape) |
| Thumbnails / Peek | None (no surface) | Standard minimized peek (inference; unobserved visual) | Live sliver composited; peek shows parked frame (inference; unobserved visual) | Native | DWM still composes (docs); shell peek TBD |
| Explicit selection | Nothing to select; pager is the reveal path | Select restores/focuses per shell; workspace-switch policy TBD | Foreground of parked window is observable; switch-versus-pull TBD | Foreground switches desktop (Chen 2017) | Switch-to-workspace in both references (Glaze/komorebi) |
| Urgency (D27 5a) | Workspace indicator is the planned surface; `FlashWindowEx` has no visible caption/button while hidden (mark/clear TBD) | `FlashWindowEx` flashes the retained taskbar button ([docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-flashwindowex)): "does not change the active state"; mark/clear TBD | Flash lands on a visible (sliver) window; may mislead (looks present); TBD | Native flashing + Task View badge; TBD visual | TBD; taskbar pairing decides the flash surface |
| Unsolicited activation (D27 5b) | Mechanism-independent: OS-permitted `Switch and focus` switches even over fullscreen/game; denied/default unchanged | Same as SW_HIDE | Same, plus foregrounding a parked window may precede workspace switch; TBD | Same, via native desktop switch | Same, via uncloak + switch (komorebi reconcile shape) |
| Crash / stop recovery | Strongest in-tree: identity-gated ledger plus same-executable watcher; the ledger records the restore obligation and `Uncertain` identities keep residue with no writes (`ProductTeardown::Uncertain`, `product_hide.rs:80-83`): a designed, readback-verified obligation, not a no-strand guarantee | Weaker: crash leaves windows minimized (visible in shell, recoverable by user) but tiler slot state (D23 `MinimizedSlot`) must distinguish hidden-workspace members from user-minimized ones; TBD | Weakest: crash leaves windows parked off-screen; needs durable geometry ledger + parked-window detection or windows strand out of view; must never strand invisible | Crash leaves windows on a non-current native desktop; Task View recoverable; needs desktop-ID ledger, deleted-desktop handling | Uncloak + `AddTab` restoration needed; private-COM drift risk (cf. Glaze `IApplicationViewCollection` crash report in prior survey); TBD |
| Multi-output | Output-agnostic (per-window ledger) | Output-agnostic; restore rectangle per output TBD | Needs per-monitor preimage + DPI/maximize handling | Native per-desktop; interacts with per-output Task View; user-driven desktop changes complicate managed model | Output-agnostic per-HWND |
| Accessibility | Hidden windows are expected to drop out of UIA enumeration (inference, untested); Narrator and exact UIA behavior unknown; taskbar `Win+T` path lost; pager is the keyboard alternative | Retained taskbar/keyboard path; exact screen-reader announcement TBD | Sliver remains hit-testable (a11y and click hazard); TBD | Native, best understood | TBD; cloak + `DeleteTab` removes both shell paths |
| Performance | Cheapest: not composed | Cheap: iconic, not composed | Still composed (1px sliver rendered; full off-screen still composed) | Native composition per desktop | Still composed by DWM (documented) |
| API support | Fully public (`ShowWindowAsync` + `IsWindowVisible` readback) | Fully public | Fully public (`SetWindowPos` geometry) | Public manager is lifecycle-incomplete (no create/enumerate/switch) | Undocumented COM; OS-version drift; outside current API constraint |
| Anti-cheat / AV | No injection surface; no ban evidence; no game testing | Same as SW_HIDE | Geometry mutation of games is itself a risk; no ban evidence; no game testing | Desktop switches under games untested; no ban evidence | No injection, but unsupported shell integration + version risk; no ban evidence; no game testing |

Gaming note (all options): no game or anti-cheat testing was performed for any
option. Public-API use is not a compatibility guarantee. See also the
[Gaming coexistence record](../changes/windows-gaming-coexistence.md).

## Reference-tier comparison

- Tier 1 (decide defaults/settings): COSMIC lists B and switches to WS1 with
  membership unchanged on selection (R-WS-07). Hyprland, i3 and sway have no
  Alt+Tab/listing counterpart in the profiled inventories, so they cast no
  vote on listing or switch-versus-pull.
- Tier 2 (tie-break only): awesome lists via its clients inventory and views
  B's tags with membership unchanged (switch, not pull). qtile/xmonad have no
  traced listing counterpart.
- Tier 3 (context): niri default-All lists B and switches with membership
  unchanged; PaperWM lists via native `live-alt-tab` only when the GNOME
  `current-workspace-only` setting is off; karousel is owner-specific (KWin
  TabBox owns listing, default switch, alternative pull).
- KDE parity evidence (actual, not inferred): Task Switcher Virtual Desktops
  filter and TabBox `activateWindow` with default switch-to-desktop, as cited
  above. Our KDE adapter rides native desktop navigation, so native policy
  applies automatically. No Windows option reproduces this without either
  shell listing (parking/minimize/native/cloak) or a pager substitute (today).
- D27 alignment: the user-selected default `Mark urgent only` plus planned
  workspace-urgent indicator (5a) pairs with omission (SW_HIDE) without
  contradiction; the approved OS-permitted `Switch and focus` over
  fullscreen/game (5b) constrains every inclusion option's activation path.
  Explicit user selection (Alt+Tab/taskbar/pager) is separate from unsolicited
  activation per the recorded decision.

## Recommendation (not a design)

Retain `SW_HIDE` and its omission for now (recommendation only). There is no simple
documented mechanism to reinsert hidden foreign windows into native
Alt+Tab: public cloak has an upstream-reported foreign-window access barrier;
the native-desktop model has public-API lifecycle gaps; private cloak requires
changing the API constraint. Minimization keeps shell presence but collides
with D23 (minimize releases allocation and stores a restore slot; a
hidden-workspace member is not a minimized tile), so it is recommended
against as the hiding backend; that recommendation is PARKED with the overall
presentation decision, not a final product selection. Parking is the only public inclusion candidate
and stays the first evaluation target: evaluate it on disposable windows first,
using KDE's default switch-to-workspace as the parity candidate. Select no mechanism or activation policy in this survey.

Decided for 0.1 User 2026-10-17 (keep `SW_HIDE`); post-0.1 research open for
mechanism plus switch-versus-pull on explicit shell selection. This survey
recommended retaining `SW_HIDE` before selection; the user has decided for 0.1.
User 2026-10-17 keeps `SW_HIDE` for 0.1; mechanism plus switch-versus-pull
moves to post-0.1 research (trivial-configurability investigation, e.g.
off-screen parking).

## Known uncertainties (explicit)

- Current-PC listing/selection under private cloak + taskbar retention, public
  app-cloak shell behavior, parking races, and foreign `MoveWindowToDesktop`
  enforcement are untested. Native all-desktops filtering is documented, but
  the settings journey was not exercised here. Actual Alt+Tab, Win+Tab,
  taskbar-button, thumbnail and Peek visuals are unobserved for every option;
  the 2026-10-17 baseline proves visibility/cloak/geometry facts only.
  Screen-reader and keyboard (`Win+T`) behavior per option is untested.
  These gaps do not block the recommendation to retain the existing mechanism.

## Sources (audit trail, minimal)

Official docs inline (`ShowWindow`, `DwmSetWindowAttribute`,
`DWMWINDOWATTRIBUTE`, `IVirtualDesktopManager`, `DeleteTab`, MS multitasking
page, KDE TabBox). Chen 2007 / 2020 cloak / 2017 foreground-switch / 2020
scope-limit posts. Pinned Glaze (`native_window.rs`, `platform_sync.rs`,
`handle_window_focused.rs`) and komorebi (`process_event.rs`, `window.rs`,
`core/mod.rs`, `com/mod.rs`). Upstream reports: Glaze #830, Glaze PR #792,
komorebi `E_ACCESSDENIED` comment. Ours: `product_hide.rs`, `workspace-native.ts`.
Related: [reference WM comparison](reference-wm-comparison.md), sections 1/5/7/12.
