# Panel And Workspace-Overview Helper

Status: research complete on 2026-09-17. Decision support only. It selects no
workspace authority, tiling control, settings owner, toolkit, package format,
adapter, IPC topology, or implementation work. No live Plasma, DBus, panel,
configuration, compositor, or manual test occurred.

This record answers the request for a tiling-WM-style panel, chiefly a workspace
overview. It follows the current [workspace decision](../decisions.md#workspaces),
[tray boundary](../decisions.md#settings-tray-and-first-run), [cross-platform feasibility](cross-platform-support/feasibility.md),
and [profile support research](reference-wm-profile-support.md). Panels and
workspaces remain first-class product requirements under `VISION.md`; a panel
must not become a second workspace or layout authority.

## Recommendation

**Proposal, not selected:** interpret "workspace overview" first as a compact,
persistent, clickable workspace strip, not a thumbnail renderer or a full-screen
shell replacement. On Plasma, start by configuring one explicitly selected stock
Pager on a user-selected panel as a compact numbered strip. The Plasma 6.7.4
default panel already includes Pager, so silently adding a second unchanged Pager
would not be useful. When the selected panel has no Pager, add and configure one;
never create duplicates silently. This preserves the task manager, clock, tray,
and all unrelated applets.

The stock Pager is a **native-desktop pager**, not a faithful view of this
project's logical workspaces. It is useful as an opt-in Plasma convenience, but
must be named and previewed that way. A logical workspace strip is a separate
future scope because the current canonical mapping is session-local inside the
KWin workspace adapter and has no widget-facing state or command contract.

| Meaning | Plasma surface | Assessment |
| --- | --- | --- |
| Compact workspace strip | `org.kde.plasma.pager` | Recommended smallest helper scope. Displays desktop cells, window rectangles/icons, and supports native desktop selection and drag/drop. It is schematic, not live thumbnails. |
| Miniature previews | Pager window geometry; task-manager hover thumbnails are a separate compositor feature | Do not require capture or thumbnail rendering for numeric/text workspace state. No first-party live-preview pager widget was verified for Wayland. |
| Full-screen overview | KWin Overview / Desktop Grid | Keep native. Overview is a modal search/window view, not a panel configuration. Do not replace or rebind it. |

## Plasma Facts And Mapping Boundary

The Plasma 6.7.4 default panel ships a Pager between the launcher and icon task
manager. Pager's plugin ID is `org.kde.plasma.pager`; it has a related activity
mode, `org.kde.plasma.activitypager`. Its model enumerates KWin virtual desktops,
tracks the current desktop, and scales task geometry into each cell. Clicks
change the selected native desktop; dragging can send a window to a native
desktop. `showOnlyCurrentScreen` filters displayed windows by screen geometry;
it does not create a per-output desktop namespace.

### Verified Pager Presentation Schema

Pager has one `General` configuration group in Plasma Desktop `v6.7.4`. The
following keys are source-verified from its `main.xml` and configuration QML:

| Key | Values and default | Compact-strip use |
| --- | --- | --- |
| `displayedText` | `0=Number`, `1=Name`, `2=None` (default) | Set `0` for the requested numbered workspace strip. `1` is an explicit named-text alternative. |
| `showWindowOutlines` | Boolean, default `true` | Set `false` to remove miniature window rectangles. |
| `showWindowIcons` | Boolean, default `false` | Set `false`; icons are meaningful only when outlines are enabled. |
| `showOnlyCurrentScreen` | Boolean, default `false` | Do not change for the first compact strip. It filters native windows and Pager screen geometry, not the global desktop list. |
| `wrapPage` | Boolean, default `false` | Leave unchanged; it controls wheel navigation only. |
| `currentDesktopSelected` | `0=DoNothing` (default), `1=ShowDesktop` | Leave unchanged; it affects a click on the current native desktop. |
| `pagerLayout` | `0=Default`, `1=Horizontal`, `2=Vertical` | Do not use for virtual desktops: the v6.7.4 UI and layout code apply it only to the activity pager. |

`displayedText=0`, `showWindowOutlines=false`, and
`showWindowIcons=false` is the narrow source-backed configuration for a compact
numbered native-desktop strip. It does not rename desktops, capture thumbnails,
or solve the logical-workspace mapping limitation below.

KWin's public scripting surface exposes one global desktop pool through
`workspace.desktops`, with per-screen current-desktop getters/setters. The
project instead keeps a session-local mapping from logical `(output, workspace)`
domains to backing KWin desktop IDs in `WorkspaceNativeAdapter`
([`kwin/src/workspace-native.ts`](../../kwin/src/workspace-native.ts)). Its
`per-output-local` and `global-unique` modes assign distinct backing desktops
per output; `shared` deliberately maps the same backing desktop to all outputs.
The adapter also owns trailing empty desktop lifecycle and disconnected-output
displacement/return.

Consequences:

| Project mode | What stock Pager shows | Fit for a logical workspace strip |
| --- | --- | --- |
| `per-output-local` | The whole global backing-desktop pool on each panel, including desktops assigned to other outputs and trailing empties. | Incorrect. Logical position 1 on one output is not Pager cell 1 globally. |
| `global-unique` | The whole global pool, while the adapter may exchange visible backing desktops between outputs. | Incorrect as a stable per-output logical index. |
| `shared` | The same global pool selected on every output, including lifecycle extras. | Closest, but still not guaranteed to be the logical list. |

Pager clicks are compositor-native desktop changes, not calls to
`selectLogical()` or the planned workspace-send route. They must not be
presented as the project's numbered workspace navigation. Renaming KWin desktops
to imitate logical labels would couple a user-visible Plasma setting to the
project's session lifecycle and is not proposed.

Therefore a future exact logical widget needs a separately approved, bounded
contract that provides a canonical snapshot and selection command for the
current output. The current code exposes neither. The contract must define
output identity, logical order, selected state, availability, and hotplug
behavior without making the widget authoritative. It must not be inferred from
Pager cells, the tray snapshot, or private adapter fields.

## Proposed Plasma Helper UX

**Proposal, not selected. First host target:** Plasma Desktop and KWin 6.7.4,
the repository's source-pinned baseline. Pin the exact `plasma-desktop` build
and Pager configuration schema again before implementation; this research does
not claim compatibility with other Plasma releases.

1. `Preview` enumerates panels and Pager applets, then requires an explicit
   target panel and, if necessary, one Pager applet. It shows location/screen,
   current values, and the three proposed values above. If that panel has no
   Pager, it says one compact Pager will be added. It makes no change.
2. `Apply` configures the selected existing Pager when present. When absent, it
   adds one fresh `org.kde.plasma.pager` at an explicit index, then applies the
   same three keys. It does not move, configure, or remove the task manager,
   clock, system tray, launcher, any unselected Pager, or another applet.
3. `Revert` restores only key/value preimages recorded for a configured existing
   Pager, and only when their current values are still the values set by Apply.
   It removes an added Pager only when its exact owned ID/type still matches. A
   new dedicated panel remains opt-in, is wholly helper-owned, and Revert removes
   only that exact panel.

Plasma's supported scripting model supplies panel enumeration/creation,
`Containment.addWidget`, widget lookup/removal/order, and applet configuration
access through the Plasma shell. KWin script code cannot directly configure the
Plasma shell; a helper would need its own bounded Plasma scripting invocation.
The source-verified sequence is `currentConfigGroup = ["General"]`,
`readConfig`, `writeConfig`, and `reloadConfig` on the selected Pager. Detection
uses the panel's widgets filtered by `org.kde.plasma.pager`; no broad appletsrc
rewrite is needed.

### Ownership, Idempotence, And User Edits

The safe default is mutable, explicit user action rather than declaratively
merging arbitrary `plasma-org.kde.plasma.desktop-appletsrc` state through Nix or
Home Manager. Current Nix/Home Manager delivery deliberately does not own user
`kwinrc` authority, and no supported general-purpose merge/reconciliation model
for a user's customized Plasma panels is established here.

The future helper needs a minimal ownership record containing only its schema
version, target panel/applet IDs, created applet index when applicable, and the
three changed Pager keys with their preimages and applied values. This is
restoration metadata, not telemetry or a general desktop state ledger. Without
it, Revert cannot distinguish a helper-created Pager from a pre-existing user
Pager or safely restore only keys it still owns. It must never restore a whole
appletsrc backup: that would overwrite legitimate user edits made after Apply.

| Case | Required behavior |
| --- | --- |
| Re-run after successful Apply | Locate the exact owned resource and report it as already applied; create nothing else. |
| Existing Pager or customized panel | Configure only the explicitly selected Pager and only the three documented keys. Do not add a duplicate. |
| No Pager on selected panel | Add one compact Pager after preview; own and remove only that applet. |
| Multiple panels/outputs | Require an explicit target panel. Do not infer a primary panel or replicate across outputs. |
| User moves/configures an added Pager | Preserve the edit. Revert removes only the owned applet, not surrounding order or configuration. |
| User changes a key applied to an existing Pager | Preserve the edit. Revert skips that key because its value no longer matches the helper-owned applied value. |
| Resource missing or ID/type mismatch | Fail closed and offer no broad cleanup. |
| Upgrade | Read the minimal record, migrate only its own schema, and preserve current user-owned panel state. |
| Uninstall | Offer Revert first. If unavailable or ambiguous, leave the resource and report its exact identity. |

The stock Pager needs no new project runtime dependency beyond the selected
Plasma Desktop installation. A helper package, if later selected, would need a
version-pinned Plasma scripting route and a delivery decision. A custom logical
plasmoid would additionally need a Plasma 6 applet KPackage and an approved
canonical workspace contract. Neither package channel is selected here.

## Host Options

The panel is presentation plus optional native command dispatch. It never owns
workspace membership, layout topology, focus history, window movement, or
thumbnail capture. A click can issue one host-native workspace selection only
after that host's command ownership and Revert behavior are selected.

| Host baseline | Practical native surface | Workspace-state fit | Helper recommendation and limit |
| --- | --- | --- | --- |
| Plasma 6.7.4 | Existing panel plus `org.kde.plasma.pager`; KWin Overview/Desktop Grid | Pager is global native-desktop state, not the project's multi-output logical model. | Proposed first helper: configure an explicit existing Pager as numbered/no-outlines, or add one only when absent, with a native-mapping warning. Defer exact logical widget and custom overview. |
| GNOME 49-51 | Native Activities/Overview and the official Workspace Indicator extension | Native workspaces are global. | Prefer Activities/Overview. Workspace Indicator provides an official compact name/menu or embedded-preview top-bar surface, but it is Shell-major-specific and must not replace task switching. |
| PaperWM v50.0.1 | Its workspace name, top bar, and position bar | PaperWM owns GNOME workspace policy and tiling presentation. | Do not co-enable another layout authority. Workspace Indicator is not listed as incompatible, but its top-bar workspace surface overlaps PaperWM's default menu and needs a selected Shell-version coexistence check. |
| Hyprland v0.56.2 with a pinned Waybar | Waybar `hyprland/workspaces` via compositor IPC | Native workspace state, including active, visible, urgent, empty, persistent, special, and per-output presentation. | Ship a separate Waybar include and CSS fragment only after pinning Waybar and IPC compatibility. Do not merge arbitrary JSONC/CSS or treat the bar as layout authority. |
| bspwm 0.9.12 with a pinned Polybar | Polybar `internal/bspwm` or upstream lemonbar report consumer | bspwm desktop/monitor report exposes focused, occupied, urgent, and empty state. | X11-only. Click/scroll invokes `bspc desktop -f`; it is a command writer, never a competing tree authority. Keyboard bindings remain external `sxhkd`; pointer bindings are bspwm settings. |
| Windows | Notification-area companion; retain Task View/taskbar/Alt-Tab | Public virtual desktop lifecycle/switch API is insufficient for a portable logical strip. | A tray companion can show selected native or later managed state, not modify the taskbar or replace Task View. Managed logical workspaces need their own recovery/shell prototype. |
| macOS | `NSStatusItem`; retain Dock, Command-Tab, Mission Control, and Spaces | Public APIs do not select/create Spaces or move arbitrary windows across them. | A menu-bar companion is feasible; it cannot be a Spaces controller. Offscreen-managed logical workspaces remain a separate prototype with Dock/task-switcher/recovery costs. |

For Hyprland, the v0.56.2 first-run configuration is editable Lua example
configuration, not a universal keymap. Waybar's current workspace module reads
IPC and dispatches native workspace commands on click/scroll; `all-outputs`,
`move-to-monitor`, persistent workspaces, and special-workspace display change
the apparent scope. Pin the Waybar release and its IPC behavior before any
claim. For bspwm, the upstream `sxhkdrc` is only an example; bspwm itself has no
keyboard catalog. These profile facts support an algorithm-plus-shortcut profile,
not a panel-selected profile or backend.

### GNOME Workspace Indicator And PaperWM

GNOME's maintained `gnome-shell-extensions` repository includes Workspace
Indicator (`workspace-indicator@gnome-shell-extensions.gcampax.github.com`). The
current `51.0` source snapshot builds for one Shell major; the Extensions website
listed active uploads for Shell 49, 50, and 51 at the access date. It is a thin
`PanelMenu.Button` added through `Main.panel.addToStatusArea`, not an overview or
tiler replacement. Its `embed-previews` default shows embedded workspace
previews; disabling it shows an active-workspace-name button and menu. Both
modes can activate native workspaces; its menu can rename them and opens its
preferences. GNOME's native Activities Overview remains the full-screen window
and workspace view.

PaperWM `v50.0.1` hides the Activities status item by default and inserts its
own workspace menu, focus button, open-position button, and optional position
bar. It restores the GNOME settings and known conflicting keybindings it changes
on disable. Its compatibility guidance specifically calls out desktop-icon,
window-shape, Space-Bar-style workspace-name, Dash-to-Panel-style panel, and
some tiling/gesture extensions; it describes broader workspace modifiers as
potentially only partially working. It does not name Workspace Indicator as a
hard conflict. The accurate proposal is therefore not a blanket prohibition:
use either top-bar workspace affordance by default, and evaluate the explicit
dual-indicator overlap on the selected PaperWM/Shell version before claiming
coexistence.

## Proposed Helper Delivery

**Proposal, not selected:** deliver the Plasma convenience as a separate,
opt-in companion command with an exportable Plasma shell script, rather than a
KCM button, tray action, or copy-paste snippet. Its narrow interface is
`preview`, `apply --panel <id>` (and an explicit Pager choice if a panel has
multiple), and `revert`. `Preview` is read-only; the mutating forms use only the
documented Plasma shell scripting surface and the ownership rules above.

This is the smallest grounded user entry point because it keeps the effect-scoped
KCM as owner of tiler settings and preserves the tray's selected status-plus-
Settings-only boundary. It also exposes the selected panel and exact change in a
user-visible command before mutation, without requiring a new persistent tray
or KCM flow. A bare snippet lacks ownership/revert behavior. A KCM action would
make the Desktop Effects settings entry own Plasma containment changes; a tray
action would expand an explicitly no-action helper route. Neither is proposed.

If later packaged, this is a separate optional helper artifact pinned to the
Plasma 6.7.4 baseline, not a new core dependency or an addition to existing tray
autostart. Nix/Home Manager may distribute that artifact but should not merge
arbitrary user panel configuration declaratively. This is a delivery proposal,
not a package-format, UI-toolkit, or implementation selection.

## Integrated Roadmap

This distinguishes technical prerequisites from a proposed priority order. It does
not change active decisions or make a stock Plasma Pager contingent on ports,
profiles, or a custom overview.

| Slice | Actual prerequisite | Not a prerequisite |
| --- | --- | --- |
| Plasma native Pager convenience | Target-tag Pager schema, an explicit panel/Pager selection, supported Plasma scripting, and narrow ownership/Revert behavior. | A new host adapter, a new WM profile, a logical-workspace contract, full packaging work, or the all-settings-live launch blocker. |
| Exact Plasma logical workspace strip | An approved canonical workspace snapshot/selection contract and click ownership for the current logical `(output, workspace)` model. | Thumbnail capture, a custom full-screen overview, Windows/macOS/GNOME ports, or a new tiling profile. |
| New host using current COSMIC behavior | A selected host/version plus its adapter capability contract and bounded native proof. | Implementing Hyprland, bspwm, or PaperWM behavior first. The existing `cosmic_v1` core can remain the selected policy. |
| New WM behavior profile | A selected profile/version and its algorithm plus matching shortcut catalog, then vectors and host capability requirements. | A native panel or host backend. |
| Managed logical workspaces off KWin | An approved user journey that needs portable numbered send/per-output mapping, then a host recovery/shell prototype. | A panel indicator. The indicator consumes the selected model; it does not choose it. |
| Native overlays/input and package fronts | A selected host authority path, permissions/conflicts, and host-specific evidence. | The Plasma stock Pager route. |

**Priority proposal, not a gate:** finish the current all-settings-live launch
blocker and pending KWin acceptance before broadening user-facing implementation
work. The gap-only reload is static-complete but partial; `workspaceMode` and
`shortcutProfile` are startup-consumed, while `tilingAlgorithm`,
`automaticSplitTarget`, and `dropOutlinePreview` remain unconsumed. Startup
near-strip fitting, fresh-session recovery after confirmed Planner loss, and
disconnect/reconnect lifecycle are offline-complete with live acceptance
pending. This priority does not technically block a separately approved,
standalone native Pager convenience helper.

After the current-host priority, the practical target order is: define portable
capability/settings semantics before adding host settings; select native versus
managed workspace semantics for each new host; use `cosmic_v1` unless a different
profile is selected; prove one normal-window adapter; then add overlays/input and
host-native packaging. The KWin group-outline diagnostic is delivered; visual
no-outline diagnosis remains parked for a user new session/status capture.
Meta-held is selected; no automatic one-second highlight is selected.

The existing tray remains status and Settings only. It is not the panel helper,
does not issue workspace commands, and has separate watcher, packaging, login,
and update/rollback live gates. A future panel click route is a proposal, not an
expansion of that tray boundary.

## Decisions For The User

1. Is the first desired result the proposed native-desktop Pager convenience, or
   is a faithful per-output logical workspace strip required from the start?
2. May a future panel issue native workspace-selection commands, or is it
   display-only plus Settings? If commands are allowed, what exact Apply/Revert
   ownership is acceptable?
3. Is an explicitly selected existing Plasma panel acceptable as the default
   target, with an opt-in dedicated helper-owned panel only when requested?
4. Should panel implementation wait for the all-settings-live launch blocker,
   while its contract may be specified in parallel, or be prioritized directly
   after that blocker?
5. For later ports, are reduced native workspace semantics acceptable, or does a
   user journey require managed logical workspaces despite recovery and native
   shell tradeoffs?
6. Which single profile/host/version should follow Plasma: a portable policy
   profile, native integration, or one only after a bounded capability prototype?

## Sources And Unknowns

Pager and GNOME source snapshots below were accessed on 2026-09-16; the remaining
web sources were accessed on 2026-09-17. Plasma claims are pinned to Plasma
Desktop/KWin `v6.7.4` where source is cited; the package version actually
installed by a future helper must still be checked.

- KDE, [Plasma scripting](https://develop.kde.org/docs/plasma/scripting/),
  [scripting API](https://develop.kde.org/docs/plasma/scripting/api/), and
  [scripting examples](https://develop.kde.org/docs/plasma/scripting/examples/)
  - panel, containment, widget, configuration, and shell-script surfaces.
- KDE, [KWin scripting API](https://develop.kde.org/docs/plasma/kwin/api/) -
  global desktop pool and per-screen current-desktop surface; documentation is
  generated for KWin 6.0, so target-tag source remains the compatibility check.
- KDE GitHub mirrors, [Pager schema](https://raw.githubusercontent.com/KDE/plasma-desktop/v6.7.4/applets/pager/main.xml),
  [Pager configuration](https://raw.githubusercontent.com/KDE/plasma-desktop/v6.7.4/applets/pager/qml/configGeneral.qml),
  [Pager rendering](https://raw.githubusercontent.com/KDE/plasma-desktop/v6.7.4/applets/pager/qml/main.qml),
  [Pager model](https://raw.githubusercontent.com/KDE/plasma-desktop/v6.7.4/applets/pager/pagermodel.cpp),
  [default panel layout](https://raw.githubusercontent.com/KDE/plasma-desktop/v6.7.4/layout-templates/org.kde.plasma.desktop.defaultPanel/contents/layout.js),
  and [KWin Overview](https://github.com/KDE/kwin/tree/v6.7.4/src/plugins/overview).
- Project static source, [`workspace-native.ts`](../../kwin/src/workspace-native.ts)
  and [`plan-adapter-entry.ts` @ ae30b867](https://github.com/beefsack/OmniTiler/blob/ae30b867/kwin/src/plan-adapter-entry.ts) - selected
  workspace mapping, numbered routes, and absence of a widget-facing contract.
- [Cross-platform feasibility](cross-platform-support/feasibility.md) - public
  workspace, settings, packaging, and managed-workspace limits for Windows,
  macOS, and GNOME.
- [Profile support](reference-wm-profile-support.md) - Hyprland `v0.56.2`,
  bspwm `0.9.12`, PaperWM `v50.0.1`, and their panel/authority boundaries.
- GNOME, [Workspace Indicator source](https://github.com/GNOME/gnome-shell-extensions/tree/main/extensions/workspace-indicator),
  [version metadata](https://raw.githubusercontent.com/GNOME/gnome-shell-extensions/main/meson.build),
  [Extensions listing](https://extensions.gnome.org/extension/21/workspace-indicator/),
  [Activities introduction](https://help.gnome.org/gnome-help/shell-introduction.html),
  and [workspace overview](https://help.gnome.org/gnome-help/shell-workspaces.html).
- PaperWM `v50.0.1`, [release metadata](https://github.com/paperwm/PaperWM/blob/v50.0.1/metadata.json),
  [top bar](https://github.com/paperwm/PaperWM/blob/v50.0.1/topbar.js), and
  [compatibility guidance](https://github.com/paperwm/PaperWM/blob/v50.0.1/README.md).
- Waybar, [Hyprland workspaces module](https://github.com/Alexays/Waybar/blob/master/man/waybar-hyprland-workspaces.5.scd),
  and Polybar, [bspwm module](https://github.com/polybar/polybar/wiki/Module:-bspwm)
  - ecosystem documentation only; exact versions are unknown until selected.

Open implementation facts: Pager multi-output runtime behavior, Plasma shell
locking and panel-ID behavior, Wayland thumbnail availability, Waybar and Polybar
release pins, GNOME Shell/Mutter major support, Windows virtual-desktop and MSIX
probes, and macOS Space, Dock, fullscreen, and Accessibility behavior. None is
resolved by this record.
