# Windows port: decision plan

Status: research updated 2026-09-30. User-selected directions are in
`docs/decisions.md`; unproven APIs remain spikes. Builds on
[cross-platform feasibility](../cross-platform-support/feasibility.md) and the
[portable-policy audit](../cross-platform-core/extraction.md). `VISION.md`
requires tiling, groups, borders, shortcuts and workspaces, with no impact on
fullscreen gaming. Existing KWin decisions remain in `docs/decisions.md`.
This plan distinguishes approved goals from untested implementation choices.
The matching [macOS plan](../macos-port/plan.md) compares host constraints;
the [core extraction audit](../cross-platform-core/extraction.md) now includes
a three-host capability and sharing comparison.

**Answer [user decision 2026-09-30]:** Windows 11 x64, developed on the
physical Windows test system after KDE-first core extraction. Managed per-monitor
workspaces are required for feature completeness. Win+Arrow is opt-in but
must work for this user's feature-complete experience. Keep Win+L and a real
underlay as experiments, with explicit Win+L opt-in and an accepted group
outline fallback. Private virtual-desktop APIs remain out of the release path.

## Evidence key and baseline correction

The earlier feasibility study is a useful starting point; its Windows scope
needs a first-class workspace gate, and Win+L needs a separate policy choice.

- **V**: documented vendor API or checked license. **O**: upstream issue or
  implementation, not a Windows guarantee. **I**: proposal or unverified
  behavior. Source keys are listed below; checked 2026-09-29.
- [V: W5,W6; O: U1] The 2026-09-17 study correctly left cross-process
  `DWMWA_CLOAK` open and identified the public virtual-desktop limit.
  Managed workspaces still need shell and recovery proof.
- [I: `VISION.md:15`, `docs/decisions.md:689-727`] Its native-current-desktop
  scope is a good prototype but does not deliver per-output workspaces.
  KDE's Lock Session relocation does not by itself prove Win+L can be
  relocated; see the policy options below.
- [V: W21; user decision 2026-09-30] Windows 10 Home/Pro support ended
  2025-10-14. Windows 11 x64 is the selected target; the user may revisit it.

## Development and iteration

Day-one native setup, governance decisions and verification: [Windows development environment](../../windows-dev-environment.md).

Develop natively on the selected Windows 11 test system. It is also the historical KDE
multi-output host (DP-6 and HDMI-A-2); that historical assertion is unchanged,
and the current single Windows display is not proof of a present wiring change.
**Settled (user 2026-09-30):** this test system dual-boots
Windows 11 Pro x64 build 26200 and NixOS nixos-unstable. Develop on one display
here (accepted display baseline in the [runbook](../../windows-dev-environment.md));
Windows multi-monitor work moves to the user's other Win11 test system. A VM is optional,
not the safety net.

- [V: W33] Install the official `rustup` MSVC x64 toolchain, Visual Studio
  Build Tools with **Desktop development with C++** and the Windows SDK.
  Install `just` using its documented Windows distribution, with versions
  pinned alongside the future Windows build contract. The Linux toolchain
  remains managed by `devenv.nix`; no Windows build tools go into it.
- [I] Keep separate NixOS and Windows working copies and synchronize *source*
  through Git; do not share `target/`, live settings, ownership receipts or
  compiled effects across operating systems. Windows and KDE cannot be tested
  concurrently: switch sessions deliberately and re-establish each OS
  baseline. A separate Windows install/test system avoids rebooting but does not make
  Windows monitor IDs equal to KDE's DP-6 and HDMI-A-2. Verify the single Windows
  display identity on this test system; verify multi-monitor identities only on the other Win11 test system.
- [R: milestone 2] Windows recipes in `just/windows.just`, loaded by the root
  `justfile` (requires just >= 1.56; platform imports are conditional):
  `just dev` builds, refuses an existing owner, copies a fresh GUID payload
  under `target/windows-dev`, starts via Explorer's desktop broker, runs the
  native tile loop, streams the log, and verifies stop/restore/ledger on exit.
  `just dev trace` adds redacted tracing (`verbose` keeps default logging);
  `just stop` exits the owner then independently restores via the current
  payload pointer. `just lifecycle-smoke [trace]` is the non-tiling loop;
  `just tile` still requires explicit `--user-start`. The normal tile owner
  includes hooks, overlays and tray; the lifecycle smoke is non-tiling.
- [I] On the daily-use system, start with dedicated test apps where practical;
  first experiments must not hide real user windows. Keep a desktop/Start
  recovery shortcut outside the hotkey hook: `tiler-windows restore` reads
  the owner-tagged visibility ledger and reveals only verified project-hidden
  windows, even if the primary process has exited. `tiler-windows stop`
  requests graceful release; if unresponsive,
  an emergency stop targets only a verified dev-process identity, then runs
  the standalone restore path. Disable dev login startup before testing an
  intentionally crashing build. Prove recovery before allowing ordinary
  apps, workspace hiding or restart automation.
- [I: W7-W9] Native Windows handles DWM, Explorer, Snap, User Account
  Control (UAC), games and physical shortcut acceptance. Linux cross-compile
  (`cargo-xwin`) and Wine are optional smoke paths, not prerequisites.
  GitHub Windows runners gate build, Rust tests and release artifacts but
  cannot replace interactive desktop checks. **Settled (user 2026-09-30):**
  Sandbox is enabled, installed, rebooted, then closed after a failed
  preflight (user-dismissed WM_CLOSE; no processes remain). Phase 1-3 live
  proof is physical-desktop owned windows first; Win+L policy
  experiments are deferred to Phase 4 Sandbox guest-only.

## Process, authority and lifecycle

Run one Rust executable in each user's interactive session, not a Windows
service. Services run in noninteractive Session 0, where they cannot manage
the user's window handles (HWNDs) [V: W1,W2].

- [I: W2,W3; `docs/decisions.md:50-58`] The executable hosts event listeners,
  `tiler-core`, window control, recovery state, tray and an on-demand settings
  window. Stage 1 now puts both low-level hooks and synchronous classifiers on
  a dedicated input pump for latency (target C selected 2026-10-12, A then B);
  the owner retains Engine, actuation, rendering and recovery. See
  [threading contracts](owner-threading.md) and
  [offline delivery](../../changes/archive/windows-input-thread.md).
  KWin's planner/script/effect/tray responsibilities need no matching Windows
  processes or D-Bus/JSON hop. Use `tiler-protocol` for external IPC only.
- [V: W4] User Interface Privilege Isolation (UIPI) restricts lower-integrity
  processes from sending certain messages to higher-integrity processes.
  Integrity level (IL) is Windows' low/medium/high process trust level; an
  ordinary user app normally runs at medium IL. Test each window operation
  separately rather than treating UIPI as a blanket geometry ban.

| Concern | Proposed behavior and evidence |
| --- | --- |
| Login | [V/I: W10,W11,W19] Choose a per-user logon-triggered scheduled task without elevation; compare delayed `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, Startup folder and packaged MSIX startup. | Enable startup only by user choice. |
| Single instance / restart | Single instance *per session/user*, via named per-user mutex plus same-user ACL/lockfile; avoid global-name squatting. New CLI invocation talks to the running owner, or exits; never assumes an abandoned mutex restores windows [V: W10; I]. `RegisterApplicationRestart` does not guarantee unattended crash restart (runtime/consent conditions); compare scheduled task restart and a lightweight per-user launcher against the risk of restart loops. Always reconcile from current complete observations [V: W10; I]. |
| Lock, switch, RDP, sleep | `WTSRegisterSessionNotification`/`WM_WTSSESSION_CHANGE`, `WM_POWERBROADCAST`, display topology/DPI messages; stop writes/overlays during lock, UAC secure desktop and inactive/RDP session, invalidate monitor handles, replay no setter, enumerate fresh and rebind on unlock/resume [V: W10,W14; I]. Detect Explorer `TaskbarCreated`, re-add tray and reconcile work areas [V: W13]. |
| Elevation and IPC | Stay medium integrity. UIPI/foreground constraints mean elevated, system, UAC and protected windows are an explicitly unmanaged class until per-operation geometry/hook proof; never auto-elevate or use UIAccess as an assumed workaround [V: W4,W15; I]. Same-user trust follows `docs/decisions.md:118-125`; if a CLI/updater/settings sidecar is needed, use a session-local named pipe with user-scoped security descriptor, owner identity and request bounds/correlation, no open network port. Do not use `WM_COPYDATA` across IL as the primary transport [V: W4; I]. |

## Window control and eligibility

Start with ordinary same-integrity windows. Every move needs a fresh native
readback: an API call returning success is not proof that the app accepted
the requested rectangle.

- [V: W2,W16-W18] `EnumWindows` and out-of-context `SetWinEventHook` observe
  creation, foreground, location and move/size lifecycle. The hook thread
  needs a message loop. Shell hooks supplement but do not replace complete
  enumeration; UI Automation can supply role/bounds for difficult apps.
- [I: W2] Coalesce our own move events, enumerate afresh after missed events,
  Explorer restarts and resume, and keep UI Automation off the hot path.
- [V: W2,W16] `SetWindowPos` and `DeferWindowPos` batch window moves, but do
  not acknowledge client acceptance. Use `SWP_NOACTIVATE|SWP_NOZORDER` for
  geometry, then read back and reconcile app-enforced minimum/maximum sizes.
  `SetForegroundWindow` can refuse a focus request.
- [V: W16] Set Per Monitor v2 dots-per-inch (DPI) awareness in the manifest.
  `GetWindowRect` includes invisible resize borders and can be DPI-virtualized;
  DWM's `DWMWA_EXTENDED_FRAME_BOUNDS` gives visible bounds without DPI
  adjustment. Convert the desired visible rectangle to each app's outer
  window rectangle and remeasure after scale/output changes.
- [I: W4,W5,W16; `docs/decisions.md:551-583`] Classify owned dialogs,
  Universal Windows Platform (UWP) frames, cloaked, minimized, elevated and
  protected windows before tiling. Keep already-tiled fullscreen/maximized
  allocations, and do not classify an output-sized rectangle as fullscreen
  solely by its dimensions.

**Elevated windows [V: W4; I]:** an app launched with "Run as
administrator" runs at a higher integrity level than the ordinary tiler.
Examples include an administrator PowerShell/Terminal, Registry Editor,
Task Manager when elevated, installers and an administrator file manager.
The UAC confirmation screen itself is a separate secure desktop. If these
windows remain unmanaged, the user sees them float at their native position
without automatic tiling, group underlay or managed workspace hiding; other
normal windows continue tiling. A shortcut pressed while one is focused may
be observed, but its requested focus/geometry action may fail. Test each
operation rather than assuming UIPI blocks every call. User decision
2026-09-30: elevated apps stay unmanaged by default; a future user-chosen
option to run the tiler elevated may be considered.

## Shortcuts and host-setting conflicts

Many Win-key shortcuts can be overridden, but there are two distinct routes:
`RegisterHotKey` tries to register a chord, while `WH_KEYBOARD_LL` can suppress
the key event before it reaches ordinary consumers. `MOD_WIN` is documented as
reserved, so neither route guarantees a particular shell chord [V: W3].

- [V: W3; O: U11] AutoHotkey documents overriding native Win+E/Win+R.
  Bare `#key::` uses `RegisterHotKey` when possible; `#UseHook` or `$`
  explicitly selects its keyboard hook. Thus AutoHotkey bindings alone do
  **not** prove that `WH_KEYBOARD_LL` was used for Win+D, Win+Tab or Win+number.
- [V: W3; O: U11] A low-level hook runs on the installer's message-loop
  thread and may be silently removed after a timeout. AutoHotkey documents
  sending a harmless "menu mask key" before Win release to avoid opening
  Start. This is an implementation precedent, not a promise on our builds.
- [I: W3,U11] Offer configurable non-Win defaults first, with opt-in Win-key
  takeover where verified. Keep the hook fast and disabled in game mode.

Secure attention sequence (SAS) means Ctrl+Alt+Del's Winlogon-controlled
security path; an ordinary keyboard hook cannot replace it [V: W12].

| Chord | Documented / observed capability | Windows product position |
| --- | --- | --- |
| Win+Arrow | [V: W3; O: U3] OS Snap shortcut; FancyZones has an explicit override. | Opt-in after measuring Snap coexistence. |
| Win+Shift+Arrow | [V: W12; O: U3] Left/Right move between monitors; Up/Down stretch or restore. FancyZones deliberately leaves the monitor-move chords intact. | Probe hook takeover per direction; different default. |
| Win+D | [V: W12; O: U11] OS Show Desktop; AutoHotkey supports overriding built-in Win hotkeys generally. A verified hook-based `#d::` result is still missing. | Observed-overridable class; test actual method and shell suppression. |
| Win+Tab | [V: W12; O: U11] OS Task View; AutoHotkey's documented generic Win-key override applies, but no per-chord hook guarantee was found. | Same: probe `#Tab::` with and without forced hook. |
| Win+1..9 | [V: W12; O: U11] OS taskbar activation; generic override precedent, not a verified `#1::` hook result. | Same: probe each digit and taskbar result. |
| Win+L | [V: W12; O: U12] Lock path persists despite ordinary AutoHotkey interception. A per-user policy disables locking, but whether a hook can then claim Win+L is unproven. | Explicit opt-in option and isolation spike below; default preserves locking. |
| Win+Z, Win+G, Ctrl+Alt+Del | [V: W3,W12] Snap layouts, Game Bar, secure attention respectively. | Avoid by default; do not promise SAS takeover. |
| Other user chords | [V: W3] `RegisterHotKey` reports collisions rather than transferring ownership. | Configurable non-Win default, e.g. `Alt+H/J/K/L` after conflict testing. |

**Win+Arrow is a first-class acceptance gate [user decision 2026-09-30]:**
the defaults remain non-Win, but this user depends on a reliable opt-in
override. The early physical test system spike must prove key-down/up delivery,
suppression of Snap/Start side effects, reversal when disabled, operation
with focus on normal windows and no game interference. Do not call the
Windows experience feature-complete for them until the override passes a
real Win11 session check. FancyZones proves an override exists in at least
one product, not that our hook, every direction or every Windows build works
[O: U3].

**Win+L policy choice.** Windows maps "Remove Lock Computer" to
`HKCU\Software\Microsoft\Windows\CurrentVersion\Policies\System`, value
`DisableLockWorkstation=1` (DWORD). This is a *user-scoped* policy, not an
HKLM-wide administrator setting [V: W12]. Normal HKCU writes need no
elevation when the key's access control list allows `KEY_SET_VALUE`;
organizational policy can deny or overwrite the value [V: W30]. Windows
documents that enabling it prevents
workstation locking, including the Ctrl+Alt+Del Lock route [V: W12].

| Option | Consequence | Recommendation |
| --- | --- | --- |
| Keep lock policy unset; bind focus-right elsewhere | [V: W12] Win+L remains a reliable user lock action. | Default. |
| User explicitly opts into Win+L; set policy to 1 only after snapshot/readback | [V: W12; O: U12] Policy prevents locking; community reports say the Ctrl+Alt+Del Lock entry disappears and `LockWorkStation()` may also fail. Neither freeing Win+L nor moving lock to another chord is established. | Offer only if a safely isolated, reversible experiment proves hook delivery, replacement lock and restore. |

The policy documentation does not specify whether a registry write takes
effect without logoff or whether `LockWorkStation()` still works. Community
reports differ on refresh timing and report that the API can fail while the
policy is set [O: U12]. Test the policy, both locking paths and hook delivery
before/after writing it. Revert by removing only our owned value (or restoring
the exact prior value), read back, and re-test actual locking. If a policy
controller overwrites it or the old state cannot be proved, leave it alone
and show the conflict. This is closer to KDE's explicit Fix/Revert than a
silent shortcut reassignment, but carries a broader loss of lock access
[I: `docs/decisions.md:96-117,749-836`].

[I: W3,U11; `docs/decisions.md:96-117`] Test AutoHotkey-style menu masking,
key-down/up and Start behavior without logging raw keystrokes. Snap Assist
and layouts can compete with dragging as well as hotkeys; settings should
show current conflicts and explicit Fix/Revert, never change global Snap at
install/startup. Preserve a preimage for any setting we actually change;
uninstall restores only an unchanged project-owned override. Keep elevated
foreground windows unmanaged until hook, geometry and focus are tested
separately [I: W4].

## Dynamic workspace choice

Native Windows virtual desktops cannot supply independent workspaces on two
monitors through the documented API. A managed visibility layer is the
candidate, contingent on crash and task-switcher behavior.

| Approach | Capability / shell cost | Recommendation |
| --- | --- | --- |
| Native virtual desktops | Public `IVirtualDesktopManager` only Get ID, Is current, Move HWND to *known* desktop GUID; no create/list/select/remove, and native selection is global, not independently per monitor [V: W5; I]. Task View/taskbar and crash restoration stay Windows-owned [I]. | Interoperate with native desktop *current-visible* windows; no native-only parity claim. |
| `IVirtualDesktopManagerInternal`, VirtualDesktopAccessor, `winvd` | Undocumented COM interfaces/IIDs and method signatures have changed across builds; VirtualDesktopAccessor documents build-specific support, wrappers cannot make a private ABI stable [O: U6]. | Do not build release workspace authority on them. Could benchmark in throwaway spike only. |
| Project-managed, own logical `(monitor,workspace)` domains | Supports per-output-local/global-unique/shared, numbered trailing empty and background plans using existing core domains [O: `crates/tiler-core/src/session/world.rs:31-75`; I]. Visibility and focus/recovery become our responsibility; Alt+Tab/taskbar/Task View are not defined as a third-party workspace protocol [V: W6; I]. | Preferred target if a reversible visibility strategy passes gate; otherwise pause full workspace release rather than silently remove first-class behavior. |

**Visibility mechanics:** a hidden window is not a minimized window.

- [V: W6; O: U1,U2,U4] `ShowWindow(SW_HIDE/SW_SHOWNA)` is public, but may
  affect taskbar/Alt+Tab or be reversed by the app. Minimize changes the app's
  state; offscreen parking leaves a window mapped. komorebi uses private
  `IApplicationView::SetCloak` for foreign windows. The documented
  `DWMWA_CLOAK` setter has no documented cross-process authority; an external
  repro reports `E_ACCESSDENIED`.
- [I: W6] Prototype `ShowWindow` first, comparing other methods only to learn
  their shell behavior. A switch hides the owned inactive set, reveals the
  target without activation, observes focus and reconciles. Explicit quit,
  update and uninstall restore only project-hidden windows.
- [I] Before hiding, atomically record the owner, version and bounded window
  identities. On restart, verify process/session identity before revealing:
  an HWND can be reused. Keep a recovery command and visible failure status.
  No process can re-show windows after a crash until something relaunches it.
- [I] Gate adoption on two-monitor switching, taskbar/Alt+Tab/Task View,
  accessibility, fullscreen, pinned windows and forced-process-loss tests.

## Border, underlay and drop preview

Z-order insertion is a candidate for a real group underlay, not a proven
solution. Test a non-topmost layered window immediately **behind** the
group's lowest member and its behavior in Task View before selecting it.

| Surface | Option and limitation | Recommended order |
| --- | --- | --- |
| Active border | `DWMWA_BORDER_COLOR` Win11 build 22000+ is native but fixed by system frame geometry/thickness and foreign-HWND setter behavior must be probed; restore `DWMWA_COLOR_DEFAULT` on release [V: W6; I]. GlazeWM uses this attribute [O: U2]. | Spike first, adopt only if visibility/ownership and exact restore pass; configurable gap/width/radius needs own overlay [I]. |
| Configurable active outline / drag fill | [V: W6,W16; O: U1,U3] Own layered click-through `WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE` window; PowerToys uses per-monitor preview windows and komorebi uses separate border windows. | Place active outline above its target but below owned dialogs when possible; preview exists only during drag. Test focus/input and stacking. |
| Group **underlay** | [V: W2,W31; I] `SetWindowPos(underlay, lowest_member, ..., SWP_NOACTIVATE|SWP_NOMOVE|SWP_NOSIZE|SWP_NOOWNERZORDER)` inserts the underlay *after* (behind) that member in Z order. If group members occupy a contiguous non-topmost block, it is behind them and above lower non-group windows. | Leading candidate. Recheck membership and Z order on foreground/reorder events; measure lag, topmost and modal failure modes. |
| Per-window child / DirectComposition | Child of a foreign HWND, compositor injection or reparenting changes app ownership/input and is outside supported first path; DirectComposition can improve owned-layer rendering but adds complexity [V: W6; I]. | Do not use to bypass window-manager boundary. |

### Shell visibility and custom drawing

Tool-window styles should keep the underlay out of Alt+Tab and the taskbar.
No public API found guarantees that an overlay cannot appear in Task View or
in a *different window's thumbnail*. Test that in the actual Win11 shell.

| Route | Established behavior and limitation | Experiment |
| --- | --- | --- |
| Unowned top-level `WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE` | [V: W34] `TOOLWINDOW` excludes the taskbar and Alt+Tab; `NOACTIVATE` avoids focus and also omits a taskbar button by default. `WS_EX_APPWINDOW` would force one; do not set it. Task View and Win+Tab thumbnail behavior are undocumented for this combination. | Create a non-topmost, click-through layered underlay. Verify no separate taskbar, Alt+Tab or Task View entry, no target-window thumbnail contamination and no keyboard focus. |
| Overlay owned by our hidden host | [V: W34] A hidden owner is a documented alternative to suppress a taskbar button. Owned windows have owner-relative Z-order constraints [V: W2]. Ownership by a group member belonging to another process is not the same as our own hidden owner. | Compare with the unowned tool window for dialog stacking, Task View and desktop switching. Do not assume ownership improves underlay Z order. |
| DirectComposition / `Windows.UI.Composition` visuals | [V: W35] DirectComposition binds a visual tree only to an HWND owned by the caller; foreign HWND returns `DCOMPOSITION_ERROR_ACCESS_DENIED`. WinRT Composition examples create `DesktopWindowTarget` for the app's HWND. Better painting within our own surface, not permission to paint beneath foreign apps without a host window. | Animate multiple visuals within one owned window; compare latency and Task View behavior with a layered bitmap. |
| `DwmRegisterThumbnail` | [V: W36] Copies a top-level source window image into our top-level destination, or into the desktop window. The desktop exception avoids our own HWND but stays behind ordinary windows; it cannot place custom group color between arbitrary apps. | No underlay prototype; relevant only if a separate thumbnail preview is chosen later. |
| Magnification API | [V: W37] A magnifier control redraws sampled screen content inside a layered *host window*. It is for magnification, not injecting colored group geometry into DWM. | Reject for underlay; adds capture/composition and cannot remove the host HWND. |
| One monitor-sized transparent surface | [V: W34; I] One owned layered top-level HWND per monitor can draw active ring, filled group region and drop preview together. It reduces window count, not shell visibility or Z-order conflicts: one Z plane cannot simultaneously draw a ring above a group and an underlay beneath it. | Compare one surface with per-visual windows; test click-through, repaint cost, topmost dialogs, fullscreen, Task View and mixed-DPI monitors. Split underlay and ring into two planes if needed. |

[O: U1,U2,U3,W38] komorebi creates border HWNDs with `TOOLWINDOW`,
`NOACTIVATE` and initial `TOPMOST` styles; GlazeWM's Windows 11 focus border
uses `DWMWA_BORDER_COLOR` on the target app instead of a border HWND;
FancyZones creates unowned `WS_EX_TOOLWINDOW` zone windows per work area,
shows them only while needed with `SW_SHOWNA`, then hides/pools them. Those
choices explain their taskbar/Alt+Tab approach, **not** proven Task View
absence for a persistent underlay.

[I: W34-W38] Throwaway experiment: owned Notepad-like test windows, one
then two monitors. Compare unowned and own-host-owned tool overlays, one
full-monitor surface and per-visual HWNDs while opening/closing Task View
(Win+Tab), cycling Alt+Tab, switching native virtual desktops and launching
Explorer's Snap/Task View animations. Record separate overlay entries,
thumbnails, visual bleed/flash, focus, Z order, CPU and teardown. Test whether
`IVirtualDesktopManager::IsWindowOnCurrentVirtualDesktop` reports the overlay
as expected on each switch [V: W5]. No public Task View enter/leave callback
has been established, so do not build correctness on one; compare hiding on
observed focus/desktop change against leaving a passive overlay visible.
Repeat after Explorer restart and with a fullscreen game. If underlay
placement or Task View is unacceptable, use the user-approved outline.
Cross-process composition attachment, foreign-window reparenting, Explorer
injection and undocumented shell APIs are excluded [V: W35; I].

**Z-order failure cases:** placement is only as stable as the current stack.

- [V: W2,W31] Topmost windows stay above normal windows; owned popups must
  stay above their owner. `GetWindow(GW_HWNDNEXT)` can inspect what is below
  a group member, but Microsoft warns against an unbounded walk when HWNDs
  disappear mid-enumeration.
- [O: U1,U13] komorebi supports multiple border Z-order modes and repositions
  borders on events. A popup-overdraw report shows the cost of permanently
  topmost borders.
- [I] Bound and validate the walk, find the lowest eligible group member,
  insert the underlay beneath it and verify the resulting order. Recheck on
  foreground, reorder, show/hide and drag; skip redundant moves. Foreign
  windows can interleave members. Suppress the underlay if mixed topmost/
  owned members or occlusion prevent a correct view; never reorder apps to
  make the underlay visible. Place the active ring *above its target* and
  below its owned dialog when possible, rather than at the group's bottom.

**Game gate:** hide visuals and stop tiling writes while a game owns the
monitor. Measure residual work rather than assuming zero overhead.

- [V: W22] `SHQueryUserNotificationState` distinguishes exclusive Direct3D
  fullscreen from broader busy/presentation states, but it is a notification
  hint and sends no fullscreen-start event.
- [I: W3,W22] Cross-check foreground, monitor bounds and exclusions; test
  exclusive, borderless and fullscreen-optimized games plus Game Bar and
  anti-cheat. Measure input latency, frame time, CPU and geometry calls;
  disable hooks/overlays if their residual impact is measurable.

## UI, language and native feel

Users will notice a tiler that repaints unaffected apps, leaves a border
over dialogs or steals focus more than one that lacks animated moves. Favor
exact geometry and quiet, event-driven visuals.

- [V: W23,W24; I] `windows-rs` exposes Win32, DWM and Component Object Model
  (COM) APIs to Rust. Start with Rust for the tiler, Win32 tray and settings.
  WinUI 3 offers Fluent/Mica but adds Windows App SDK integration; WPF would
  require a C#/.NET settings process. Use either for a measured control or
  accessibility gap, not as a prerequisite for tiling.
- [V: W13,W25,W26; I] `winit` provides a window loop, not native controls;
  `egui` is non-native in appearance and needs Narrator/contrast checks.
  Slint has attribution/commercial licensing choices. Audit `tray-icon` for
  Explorer restart, menu keyboard use and notification icon version 4;
  `Shell_NotifyIcon` directly is another small Rust integration path.
- [V: W6,W13,W26; I] Use a keyboard-accessible notification-area menu and
  re-add its icon on Explorer's `TaskbarCreated`. Settings should follow
  Windows 11 spacing, system accent and optional Mica backdrop, with dark,
  light, contrast and reduced-motion support. Display confirmed state, not
  merely queued commands. Localize labels later; retain stable action IDs.

**Observed sources of jank:**

- [O: U14] komorebi users reported JetBrains IDEs flickering on *other*
  windows' events. Its `54c58be` fix compares both window and border
  rectangles before calling `SetWindowPos`. Skip no-op setters and measure
  repaint behavior across apps, not just the focused window.
- [O: U13,U14] A GlazeWM 3.10.1 report describes the same JetBrains symptom;
  no-op `SetWindowPos` is the reporter's hypothesis, not a proven GlazeWM
  cause. GlazeWM also merged a delayed border-color reapply for flicker.
  komorebi reported borders painted over Save dialogs and border lag with
  animation enabled. These are concrete focus/visual defects, not evidence
  that Windows itself animates every move.
- [O: U15] A komorebi user reported a Firefox title update pulling focus
  back to its monitor. A GlazeWM user reported workspace switches when an
  inactive Files window took focus after a download; their proposed fix was
  still an open PR. Do not switch workspace merely because a hidden app
  emits a foreground-like event. Check actual foreground and user intent.
- [I: W2,U13,U14] Test focus changes, window creation, resizing, native
  dialogs and optional animations for flicker, lag, redraw and unintended
  activation. `SWP_NOACTIVATE` prevents our overlay move from activating it;
  it cannot prevent another app or the shell from restacking windows.

**Windows move animations [V: W2,W6,W32]:** plain `SetWindowPos` specifies
geometry, not an animation curve. `AnimateWindow` is a separate API, while
Snap/minimize/maximize can have shell/DWM transitions. The per-HWND
`DWMWA_TRANSITIONS_FORCEDISABLED` attribute takes a BOOL: TRUE disables DWM
transitions and FALSE enables them. Documentation does not show that ordinary
`SetWindowPos` moves animate, that this attribute stops Snap, or that it
can safely be set on a foreign app's HWND. Do not blanket-disable it.
Compare video and readbacks on an owned window, then test foreign-HWND
permission and exact restoration before considering an opt-in override.
If the previous value cannot be read, do not change a foreign HWND [I].

## Packaging, signing, updates and observability

**Store is an option.** Pursue the obvious set for Windows users: a Microsoft
Store listing, a signed per-user installer on GitHub Releases and a `winget`
manifest for that installer. Store MSIX is preferable *if* desktop hooks,
login startup, workspace recovery and update/revert survive packaging proof.
Otherwise the Store can list the same signed MSI/EXE, with our updater rather
than Store-managed updates [I: W19,W20,W39-W42]. No second running owner.

| Path | Fit | Recommendation |
| --- | --- | --- |
| Store MSIX (preferred test) | [V: W19,W20,W39,W40,W46] Package a classic desktop app with `runFullTrust` (restricted capability requiring Store approval), `packagedClassicApp` and `mediumIL`, not a sandboxed UWP controller. User-controlled `windows.startupTask` supports login. Store signs/hosts MSIX at no cert charge and delivers updates. The package is per-user and has different storage/lifecycle behavior. | Prove window hooks, startup, registry/settings, restore on update/uninstall, capability approval, policy compliance and no side-by-side owner. Store updates only; do not run Velopack in this channel. |
| Store listing of MSI/EXE | [V: W39,W41] Store accepts a publisher-hosted, offline, silent, signed installer at an immutable versioned HTTPS URL. The installer **and all PE binaries** need a CA-trusted Authenticode signature; a self-signed certificate does not qualify. Store does not update existing installations. | Fallback Store route if MSIX proof fails; use the same signed installer and explicit updater as GitHub. |
| Signed direct installer + winget | [V: W20,W28; O: W42] Standard per-user WiX MSI or Inno EXE downloadable from GitHub; `winget` adds a command-line discovery/upgrade route. PowerToys documents Store, GitHub and `winget` together. | Ship the same publisher identity across channels; ensure install/update cannot create duplicate tiler owners. |
| Portable zip | [I] Manual launch with no guaranteed login/startup, update or uninstall revert. | Development/trial route only, with standalone restore/stop CLI. |

**Channel costs [V: W20,W39-W42]:** Store MSIX provides Microsoft signing,
hosting and updates without a signing-certificate purchase; Partner Center
developer registration is listed as free, but packaging and certification
still cost development time. Direct GitHub installers and Store-listed
MSI/EXE need a CA-trusted signer for the publisher path: Azure Artifact
Signing starts near US$9.99/month for eligible identities; an OV certificate
is roughly US$150-300/year plus hardware key storage. The Store-listed
MSI/EXE also needs publisher-hosted versioned binaries. `winget` adds a
manifest, not a second signing service, and uses the signed installer;
GitHub release hosting is separate from Store hosting. Actual eligibility
and ongoing delivery cost remain to be checked before publication [I].

**Store technical boundary:** a Win32 Store listing is possible, but
certification of this tiler's input and settings behavior remains unproven.

- [V: W3,W43] `WH_KEYBOARD_LL` is a user-session hook with callbacks on its
  own thread, not DLL injection into other apps. Microsoft documents hook
  delivery involving packaged Store apps, not certification of our override.
- [V: W46] A medium-integrity MSIX desktop app declares restricted
  `runFullTrust` and explains it for Store approval. The separate restricted
  `inputObservation`/`inputSuppression` capabilities refer to partner-only
  input APIs; they are not a substitute for a Win32 low-level hook.
- [V: W40] Packaged AppData and registry redirection depend on runtime
  behavior. A medium-integrity classic desktop process is distinct from an
  AppContainer process; packaging still needs a settings/recovery test.
- [V: W44] The effective Store policy is version 7.19 (since 2025-10-14).
  It requires supported methods and consent for changing Windows settings
  and forbids disabling platform safety features. Version 7.20 has been
  published but takes effect only on 2026-10-22.
- [I: W19,W39-W44] Prove packaged Win+Arrow hook behavior, user-enabled
  startup task and independent recovery. The Win+L lock-disabling policy
  might block Store acceptance even if the user opts in; do not promise it
  in the Store build. MSI/EXE login startup also requires separate consent.
- [O: W42,W45] PowerToys with FancyZones is in the Store and also offers
  GitHub and `winget`; FancyWM and an AutoHotkey v2 Store Edition have
  listings. This proves distribution precedents, not approval of our hook,
  Win+L policy or managed workspaces.

[V: W27] Velopack has a Rust client and HTTP update/packaging flow. Its
`vpk` CLI needs .NET SDK 8 on the Windows release runner, not the NixOS dev
shell. [I] Test update with a running owner, hidden-window restore, rollback
and signing order before choosing it for the installer channel. Do not run
two auto-updaters against one installation: Store MSIX uses Store updates,
Store-listed MSI/EXE and GitHub installers use the chosen app/installer update
path, and `winget upgrade` remains user-invoked. Scoop/Chocolatey can be
community-maintained alternatives [V: W28]. Keep Windows CI independent of
Nix/KDE delivery. Pin tools, record hashes and signing identity, and compare
unsigned payloads separately from timestamped signed packages [I].

[V: W20] Azure Artifact Signing (formerly Trusted Signing) costs about
US$9.99/month at the basic tier and limits individual eligibility by region.
Organization-validated certificates run roughly US$150-300/year plus hardware
key storage; extended validation costs more and no longer immediately clears
SmartScreen. Microsoft signs Store-submitted MSIX packages, but a direct
download of the same MSIX still needs its own trusted signature [V: W20].
[I] Check the owner's signing eligibility before purchasing; keep publisher
identity stable across EXE, installer and update metadata.

[I: `docs/principles.md:50-66`, W29] Write bounded structured `tracing`
summaries under `%LOCALAPPDATA%\omnitiler\logs`; enable redacted
event detail with `trace`. Correlate command, observation, setter, readback
and recovery; distinguish dispatch, acceptance, application and uncertainty.
Exclude titles, app content, raw HWND/process IDs, executable paths and
payloads from ordinary logs. Logging failure must not stop tiling. Event
Tracing for Windows (ETW) can later support performance diagnosis; Event Log
provider registration adds installation work. Use no code injection, kernel
driver, network listener, raw keystroke log or elevated helper by default;
scope any named pipe to the same user [V: W4].

## Upstream lessons and licensing

Upstream projects demonstrate practical techniques and failure modes, not
Windows guarantees. Their checked licenses constrain code reuse.

| Project | Evidence and transferable lesson | License / reuse |
| --- | --- | --- |
| komorebi (Rust) | [O: U1,U13,U14] `komorebi/src/window.rs` supports hide/minimize/private cloak and recovery; `komorebi/src/border_manager/mod.rs` defines top/non-topmost/bottom modes, while `border.rs` follows location changes. Its flicker and popup issues are instructive. | [V: U1,U6] Komorebi License 2.0.0 restricts derivative redistribution. Study only; independently licensed MIT `VirtualDesktopAccessor` code needs separate provenance. |
| GlazeWM (Rust) | `packages/wm-platform/src/native_window.rs` calls DWM border color; workspace/overlay methods expose Win11 1px limits and hide tradeoffs [U2]. | GPL-3.0 project: study, do not copy into a differently licensed binary without a deliberate license decision [U2]. |
| PowerToys FancyZones | WinEvent-backed per-monitor zone overlay and explicit Snap override; not automatic split-tree authority [U3]. | MIT, reuse possible with notice and independent fit assessment [U3]. |
| Whim | MIT C# plugin/workspace architecture; README explicitly avoids native desktops for per-monitor workspaces [U4]. | MIT, conceptual/data reuse with notice; not a Rust engine drop-in [U4]. |
| workspacer | MIT C# workspace/window filters and hide behavior [U5]. | MIT, reuse possible with notice; hide/reveal is not Windows virtual-desktop parity [U5]. |
| bug.n | AutoHotkey tiler/bar and shortcut patterns [U9]. | GPL-3.0, study only absent license change [U9]. |
| Seelen UI | Rust/Tauri custom desktop UI and workspace behavior; a larger replacement-shell direction than this tiler [U10]. | AGPL-3.0, study only absent license change [U10]. |

[I: W23,W25,U7] Check this repository's outbound license and the exact
third-party revision before reusing any code. `windows-rs`, `winit` and
`tray-icon` are candidates, not selected dependencies.

## Recommended architecture at a glance

One non-elevated Rust process owns desktop control. Native Windows APIs are
called in its adapter; no KWin processes or effect library are transplanted.

- [I] **Processes:** tiler, observer, actuator, visuals, tray and settings
  window share one user-session process. An installer/updater runs separately
  only while replacing binaries; an optional CLI uses a same-user named pipe.
- [I] **Crates:** retain `tiler-core` as layout authority. Use
  `tiler-protocol` only for external versioned messages. A proposed
  `tiler-windows` crate/binary owns Windows events, physical pixels, window
  visibility, input and shell lifecycle. Follow the
  [extraction audit](../cross-platform-core/extraction.md) for shared policy.
- [I] **Languages/dependencies:** Rust and `windows`/`windows-sys`, with
  `tracing`; evaluate `tray-icon` and Velopack behind their proof gates.
  C#/C++ is a contingency for a demonstrated settings/accessibility gap.
  Build Windows 11 x64 with the MSVC toolchain first.

## Phased roadmap (qualitative)

**KDE-first core extraction is Phase 0 [user decision 2026-09-30].** Windows
development starts only after its KWin fixtures and applicable user live
checks pass. Then use disposable owned apps on the physical Windows 11 test system;
make restore/stop work before hiding an ordinary desktop window.

| Phase | Goal / scope | Exit evidence | Throwaway spikes first | Effort / risk |
| --- | --- | --- | --- | --- |
| 0. KDE-first extraction | Move only proven portable policy; preserve current KWin behavior before Windows implementation. | KWin fixtures and user live checks for each extraction step, as specified in [extraction](../cross-platform-core/extraction.md); no Windows product build. | KWin pure-policy fixtures before edits; no Windows host mutation. | Medium / controlled KWin regression risk. |
| 1. Physical host + input/recovery | Install Windows toolchain, record single-display baseline (this test system), prove independent restore command and Win+Arrow. | Native build, dev restart/log loop and verified kill/restore on owned windows. **Win+Arrow opt-in works on the user's physical Win11 test system** with Snap/Start suppression and reversal; otherwise user-specific feature completeness is blocked. | `RegisterHotKey` vs `WH_KEYBOARD_LL` all four Win+Arrows, key-up masking, game disable; forced-crash restore on owned windows. Win+L policy is deferred to Phase 4 Sandbox guest-only. | Medium / highest input risk. |
| 2. Window geometry | Wire Windows observation/actuation to the extracted Engine for normal windows; retain evidence-based convergence. | Owned apps then representative desktop apps tile/read back with DPI, focus, sleep, display change and Explorer restart; no game writes. | WinEvent dropped-event, DWM invisible-frame/min-size and UWP/owned-dialog probes; hook timeout detection. | Large / high. |
| 3. Required workspaces + visuals | After visibility proof, implement per-monitor logical workspaces, borders, group visual and drop preview. | Two monitors with independent workspaces (other Win11 test system), trailing empty, taskbar/Alt+Tab/Task View accepted, crash recovery and gaming pass. True underlay **or approved outline fallback** works without shell pollution. | `ShowWindow` hide/reveal ledger and forced loss; underlay inserted behind group; owned/unowned tool HWND vs single custom-drawn monitor surface through Win+Tab, desktop switch, Explorer animation and fullscreen; private cloak only comparative. | Large / highest recovery and rendering risk. |
| 4. Settings + distribution | Live apply, consented conflict/revert, tray, Store proof, signed manual install and updates. | Store package feasibility or documented installer-listing fallback; `winget` manifest, signed GitHub release, clean install/update/uninstall and owner restoration; user accessibility journey. **Win+Arrow remains a feature-complete exit gate.** | Packaged full-trust hook/startup/install test; Store certification preflight; Win32/tray UI; MSIX vs signed MSI/EXE updater channel; signing identity and SmartScreen. | Medium-large / high. |

[I: W9] Test portable Rust behavior on Linux and Windows and headless
contracts on hosted CI. Run owner-verified interactive journeys on the user's
physical test system with project restore available, then physical/manual game, UAC,
fast-user-switch, sleep and remote-desktop checks. A VM is optional for
isolation, not assumed available. Hosted CI passing cannot prove hooks,
fullscreen or anti-cheat compatibility.

## Open user decisions

Windows 11 x64, managed-workspace release bar, shortcut defaults and
Win+Arrow gate, explicit Win+L opt-in, group-outline fallback, KDE-first
sequencing, physical test system development and unsurprising distribution are user
decisions of 2026-09-30. The technology behind them still needs proof.

| Status / decision | Options and consequences | Recommendation |
| --- | --- | --- |
| Decided: OS and workspaces | [User 2026-09-30] Win11 x64; managed per-monitor workspaces before Windows or macOS feature-complete. | Tiling-only previews can precede the workspace gate. |
| Decided: shortcuts | [User 2026-09-30] Non-Win defaults, proven opt-in Win+Arrow, explicit opt-in Win+L. Win+Arrow is required for this user's feature-complete experience. | Keep Win+L policy behavior as an unproven spike, not a promise. |
| Decided: group visual | [User 2026-09-30] Test custom drawing and shell pollution; outline fallback accepted if the underlay fails. | Choose a renderer only after Task View/Alt+Tab evidence. |
| Decided: development and distribution goal | [User 2026-09-30] Native physical Windows test system after KDE-first extraction; distribution should feel obvious. | Test Store MSIX alongside signed installer and winget, without promising MSIX certification. |
| Decided: elevated apps | [User 2026-09-30] Administrator apps stay unmanaged (floating) by default. | A future opt-in to run the tiler elevated may be considered; no UIAccess or elevated helper now. |
| Decided: workspace model | [User 2026-09-30] KDE-first extraction stops before the logical workspace model. | Refine the core workspace shape during Windows visibility spikes, then extract with matching KWin fixtures. |
| Open: Store implementation if both pass | [V: W39-W41; I] Store MSIX has Store signing/updates but differs in process/storage behavior; Store-listed MSI/EXE shares the manual installer and updater, but needs publisher signing and hosting. | Prefer MSIX only if desktop hooks, login, policy and recovery pass; otherwise list the signed installer. Resolve any user-visible updater/channel tradeoff with the user. |

The KDE-first logical workspace extraction choice is open in
[cross-platform extraction](../cross-platform-core/extraction.md); its shape
depends on Windows visibility evidence.

## Risks

The leading threats are unrecoverable hidden windows and any impact on games;
policy-based Win+L and overlay stacking add separate UX risks.

| Risk | Severity / trigger | Containment and proof |
| --- | --- | --- |
| Inactive windows stranded after crash | Critical; managed hide/reveal fails | Owner-tagged recovery ledger, startup restore before normal operation, forced-kill/uninstall smoke; if unsafe, no managed-workspace release [I]. |
| Game latency/anti-cheat or fullscreen overlay | Critical; hook or HWND overlap on game | Physically measured exclusive/borderless/anti-cheat matrix, immediate event-gated disable, no overlay/writes while game active [V: W3,W22; I]. |
| Private COM API churn | High; internal desktops/shell cloak changes | Avoid as required path; build matrix only for comparison [O: U1,U6]. |
| Lost shortcuts / Start/Snap interference | High; Win chords conflict or silent LL removal | Non-Win defaults; hook matrix and recovery; explicit Fix/Revert with settings preimage [V: W3; I]. |
| Win+Arrow override fails or hurts games | Critical for this user's feature-complete goal | Physical test system hook/Snap/Start/game matrix before claiming support; non-Win defaults and immediate revert remain usable for previews [O: U3; I]. |
| Lock action disabled by Win+L policy | Critical; no remaining keyboard/API lock route | Deferred to Phase 4 guest-only Sandbox experiment; prove alternate lock, refresh timing, policy owner and exact revert there [V: W12; O: U12; I]. |
| Underlay pollutes Task View or covers dialogs | High; shell includes overlay, topmost or non-contiguous members | Compare unowned/owned tool windows and custom-drawn per-monitor surface during Win+Tab and desktop switches; suppress failed visual and use approved outline fallback [V: W34,W35; O: U13; I]. |
| Daily test system recovery fails | Critical; hide/restart/shortcut loop strands desktop | Independent restore command and out-of-hook kill switch verified before real windows; disable startup for crash probes [I]. |
| Store package lacks control or certification | High; hook/startup/Win+L policy refused | MSIX full-trust/startup/certification experiment, same signed MSI/EXE Store-listing fallback, channel-specific updates [V: W39-W44; I]. |
| DPI/client refusal/UWP/mixed IL | High; geometry/readback mismatch | Per-monitor-v2, physical frame conversion, complete observation, explicit skip with reason, app matrix [V: W4,W16; I]. |
| Explorer, sleep, RDP, fast-user switching | High; stale handles/work area | WTS/power/display/taskbar signals, fresh enumeration and controlled rebind, interactive VM/physical journey [V: W10,W13; I]. |
| Signing/update trust and supply chain | Medium-high; broken updater or SmartScreen | Locked CI, signed package and metadata, staged update rollback/recovery, channel separation [V: W20,W27; I]. |

## Sources (W1-W32/U1-U15 checked 2026-09-29; W33-W46 checked 2026-09-30)

The source keys above distinguish documented API behavior from upstream
implementations and untested Windows outcomes.

- W1: [Microsoft, interactive services / Session 0](https://learn.microsoft.com/en-us/windows/win32/services/interactive-services). W2: [SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos), [DeferWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-deferwindowpos), [SetWinEventHook](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook).
- W3: [RegisterHotKey](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey), [LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc), [disabling game shortcut keys](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/disabling-shortcut-keys-in-games), [Windows Snap guide](https://support.microsoft.com/en-us/windows/experience/snap-your-windows). W4: [mandatory integrity control](https://learn.microsoft.com/en-us/windows/win32/secauthz/mandatory-integrity-control), [SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow), [UIAccess policy](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/security-policy-settings/user-account-control-only-elevate-uiaccess-applications-that-are-installed-in-secure-locations).
- W5: [IVirtualDesktopManager](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager). W6: [DWM window attributes](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute), [ShowWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow), [extended styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles), [DirectComposition](https://learn.microsoft.com/en-us/windows/win32/directcomp/directcomposition-portal).
- W7: [cargo-xwin](https://github.com/rust-cross/cargo-xwin). W8: [Wine hook implementation](https://github.com/wine-mirror/wine/blob/master/server/hook.c). W9: [GitHub hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), [Windows runner images](https://github.com/actions/runner-images), [interactive CI issue](https://github.com/actions/runner-images/issues/3180).
- W10: [WTSRegisterSessionNotification](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsregistersessionnotification), [WM_WTSSESSION_CHANGE](https://learn.microsoft.com/en-us/windows/win32/termserv/wm-wtssession-change), [power notifications](https://learn.microsoft.com/en-us/windows/win32/power/system-power-management-events), [RegisterApplicationRestart](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-registerapplicationrestart), [mutex objects](https://learn.microsoft.com/en-us/windows/win32/sync/mutex-objects), [Task Scheduler logon trigger](https://learn.microsoft.com/en-us/windows/win32/taskschd/logon-trigger-example--xml-). W11: [Run and RunOnce](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys). W12: [Winlogon](https://learn.microsoft.com/en-us/windows/win32/secauthn/winlogon), [DisableLockComputer policy](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-admx-ctrlaltdel#disablelockcomputer), [Keyboard Filter](https://learn.microsoft.com/en-us/windows/configuration/keyboard-filter/), [Windows keyboard shortcuts](https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec).
- W13: [Shell_NotifyIcon](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw), [TaskbarCreated](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar). W14: [WM_DPICHANGED](https://learn.microsoft.com/en-us/windows/win32/hidpi/wm-dpichanged). W15: [ChangeWindowMessageFilterEx](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-changewindowmessagefilterex). W16: [GetWindowRect](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowrect), [DwmGetWindowAttribute](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmgetwindowattribute), [Per Monitor v2](https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows), [SetProcessDpiAwarenessContext](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setprocessdpiawarenesscontext). W17: [RegisterShellHookWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registershellhookwindow). W18: [UI Automation overview](https://learn.microsoft.com/en-us/windows/win32/winauto/entry-uiauto-win32).
- W19: [MSIX packaging model](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/choose-packaging-model), [MSIX startup task](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.startuptask), [desktop StartupTask manifest](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-desktop-startuptask), [MSI per-user context](https://learn.microsoft.com/en-us/windows/win32/msi/msiinstallperuser). W20: [Windows signing options and prices](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options). W21: [Windows 10 Home/Pro lifecycle](https://learn.microsoft.com/en-us/lifecycle/products/windows-10-home-and-pro). W22: [SHQueryUserNotificationState](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shqueryusernotificationstate), [notification-state enumeration](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state).
- W23: [Microsoft windows-rs](https://github.com/microsoft/windows-rs), [Rust for Windows](https://learn.microsoft.com/en-us/windows/dev-environment/rust/rust-for-windows). W24: [WinUI 3](https://learn.microsoft.com/en-us/windows/apps/winui/winui3/), [deploy unpackaged Windows App SDK apps](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deploy-unpackaged-apps), [WPF](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/overview/). W25: [winit](https://github.com/rust-windowing/winit), [egui](https://github.com/emilk/egui), [Slint licenses](https://slint.dev/pricing). W26: [Mica Win32](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-mica-win32), [high contrast](https://learn.microsoft.com/en-us/windows/win32/winauto/high-contrast-parameter). W27: [Velopack Rust](https://docs.velopack.io/getting-started/rust). W28: [winget](https://learn.microsoft.com/en-us/windows/package-manager/winget/), [Scoop](https://github.com/ScoopInstaller/Scoop), [Chocolatey](https://docs.chocolatey.org/). W29: [ETW](https://learn.microsoft.com/en-us/windows/win32/etw/about-event-tracing).
- U1: [komorebi `LICENSE.md`](https://github.com/LGUG2Z/komorebi/blob/master/LICENSE.md), [window visibility implementation](https://github.com/LGUG2Z/komorebi/blob/master/komorebi/src/window.rs), [COM adapter](https://github.com/LGUG2Z/komorebi/tree/master/komorebi/src/com). U2: [GlazeWM license](https://github.com/glzr-io/GlazeWM/blob/main/LICENSE.md), [wm-platform](https://github.com/glzr-io/GlazeWM/tree/main/packages/wm-platform/src), [border limitation issue](https://github.com/glzr-io/glazewm/issues/334). U3: [PowerToys MIT license](https://github.com/microsoft/PowerToys/blob/main/LICENSE), [FancyZones design](https://github.com/microsoft/PowerToys/blob/86115a54/doc/devdocs/modules/fancyzones.md), [FancyZones shortcuts](https://learn.microsoft.com/en-us/windows/powertoys/fancyzones).
- U4: [Whim README + MIT license](https://github.com/dalyIsaac/Whim). U5: [workspacer MIT license + sources](https://github.com/workspacer/workspacer). U6: [VirtualDesktopAccessor MIT lineage / build support](https://github.com/Ciantic/VirtualDesktopAccessor), [winvd](https://crates.io/crates/winvd), [private API build-specific example](https://github.com/MScholtes/VirtualDesktop/blob/master/VirtualDesktop11-24H2.cs). U7: [tray-icon MIT/Apache license](https://github.com/tauri-apps/tray-icon). U8: [WiX](https://wixtoolset.org/), [Inno Setup](https://jrsoftware.org/isinfo.php). U9: [bug.n](https://github.com/fuhsjr00/bug.n). U10: [Seelen UI](https://github.com/eythaann/Seelen-UI).
- W30: [Registry key access control](https://learn.microsoft.com/en-us/windows/win32/sysinfo/registry-key-security-and-access-rights).
- W31: [GetWindow Z-order walk](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindow), [DeferWindowPos placement](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-deferwindowpos), [foreground and reorder events](https://learn.microsoft.com/en-us/windows/win32/winauto/event-constants).
- W32: [AnimateWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-animatewindow), [LockWorkStation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-lockworkstation).
- U11: AutoHotkey v2 [Windows hotkey override](https://www.autohotkey.com/docs/v2/misc/Override.htm), [forced hook](https://www.autohotkey.com/docs/v2/lib/_UseHook.htm), [ListHotkeys implementation mode](https://www.autohotkey.com/docs/v2/lib/ListHotkeys.htm), [menu mask key](https://www.autohotkey.com/docs/v2/lib/A_MenuMaskKey.htm).
- U12: [AutoHotkey Win+L and API reports](https://www.autohotkey.com/boards/viewtopic.php?t=69537); [community immediate-effect report](https://learn.microsoft.com/en-us/answers/questions/1306201). These are observations, not Windows API guarantees.
- U13: komorebi [border Z-order implementation](https://github.com/LGUG2Z/komorebi/blob/master/komorebi/src/border_manager/mod.rs), [border positioning](https://github.com/LGUG2Z/komorebi/blob/master/komorebi/src/border_manager/border.rs), [popup-overdraw issue #971](https://github.com/LGUG2Z/komorebi/issues/971), [border tracking issue #1607](https://github.com/LGUG2Z/komorebi/issues/1607).
- U14: komorebi [JetBrains flicker #781](https://github.com/LGUG2Z/komorebi/issues/781) and [no-op position fix](https://github.com/LGUG2Z/komorebi/commit/54c58be858ebe62acb329bd96ea2d60950dfb46f); GlazeWM [JetBrains flicker #1401](https://github.com/glzr-io/glazewm/issues/1401) and [border-flicker fix #752](https://github.com/glzr-io/glazewm/pull/752).
- U15: komorebi [Firefox focus theft #1235](https://github.com/LGUG2Z/komorebi/issues/1235); GlazeWM [focus-stealing request #793](https://github.com/glzr-io/glazewm/issues/793) and [unmerged Files workspace-switch PR #1160](https://github.com/glzr-io/glazewm/pull/1160).
- W33: [Microsoft Windows Rust setup (MSVC/rustup)](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup), [just Windows installation](https://github.com/casey/just#installation).
- W34: [Microsoft extended window styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles), [taskbar button ownership/styles](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar), [layered window painting and hit testing](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features).
- W35: [DirectComposition foreign-HWND denial](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice-createtargetforhwnd), [Windows.UI.Composition Win32 host](https://learn.microsoft.com/en-us/windows/uwp/composition/using-the-visual-layer-with-win32).
- W36: [DwmRegisterThumbnail destination ownership](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmregisterthumbnail).
- W37: [Magnification API host window](https://learn.microsoft.com/en-us/windows/win32/winauto/magapi/magapi-intro).
- W38: [FancyZones `WorkArea.cpp` toolwindow](https://github.com/microsoft/PowerToys/blob/main/src/modules/fancyzones/FancyZonesLib/WorkArea.cpp), [overlay rendering and hide](https://github.com/microsoft/PowerToys/blob/main/src/modules/fancyzones/FancyZonesLib/ZonesOverlay.cpp), [komorebi border HWND styles](https://github.com/LGUG2Z/komorebi/blob/master/komorebi/src/windows_api.rs), [GlazeWM native window](https://github.com/glzr-io/GlazeWM/blob/main/packages/wm-platform/src/native_window.rs).
- W39: [Microsoft Win32 Store paths and update ownership](https://learn.microsoft.com/en-us/windows/apps/distribute-through-store/how-to-distribute-your-win32-app-through-microsoft-store), [distribution comparison](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/choose-distribution-path).
- W40: [MSIX packagedClassicApp/mediumIL](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes), [desktop startup task](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-desktop-startuptask), [application manifest](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-f-application).
- W41: [Store MSI/EXE signing, silent install and immutable URL](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/app-package-requirements), [Store signing costs](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options).
- W42: [PowerToys Store, GitHub and winget installation](https://learn.microsoft.com/en-us/windows/powertoys/install).
- W43: [SetWindowsHookEx: Store-app hook delivery](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowshookexw), [low-level callback](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc).
- W44: [Store policy 7.19 (effective 2025-10-14)](https://learn.microsoft.com/en-us/windows/apps/publish/store-policy-archive/store-policy-7-19), [7.20 date and policy history](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies-change-history).
- W45: [PowerToys listing](https://apps.microsoft.com/detail/xp89dcgq3k6vld), [FancyWM listing](https://apps.microsoft.com/detail/9p1741lkhqs9), [AutoHotkey v2 Store Edition listing](https://apps.microsoft.com/detail/9plqfdg8hh9d).
- W46: [MSIX full-trust capability and Store approval](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations#which-kinds-of-apps-do-app-capabilities-apply-to), [restricted input API limits](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations#restricted-capability-list).
