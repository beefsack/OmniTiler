# macOS 0.1 parity options

**Status:** Decisions confirmed 2026-11-26/27 and recorded in the owning
[decisions](../decisions.md). No new native acceptance is claimed. MX6 and
the fullscreen fixture procedure are tracked separately. The existing
[native gate](macos-0.1-gate-proposal.md) stays in place, including workspace,
drag, fullscreen-carry, Settings, and underlay.

The [native Spaces prior art](macos-0.1-native-spaces-prior-art.md) informs
implementation and the next sticky discriminator. Keep native Spaces and
native macOS functions with SIP on and the cross-platform consistency rule.
An impossible feature is a Spaces-mode limitation; a separate virtualised
mode remains a later user decision.

**Evidence key:** Tagged external citations are source inspection only, not
native execution or accepted evidence. AeroSpace v0.21.3-Beta is the latest
available tagged pre-release; there is no stable AeroSpace tag. The TD2
COSMIC comparator uses historical pins, not current upstream code.

## 1. Indicator

The current status item is single-display and hides when multi-display scope is
ambiguous (`docs/decisions/settings.md:109-114`;
`docs/changes/macos-0.1-gate-proposal.md:176`;
`docs/changes/macos-0.1-product-scope-audit.md:34,49`). The Windows baseline is
a per-output pager (`docs/decisions/settings.md:74-79`). Owning records:
[settings decision](../decisions/settings.md),
[gate proposal](macos-0.1-gate-proposal.md),
[indicator record](macos-workspace-indicator.md),
[scope audit](macos-0.1-product-scope-audit.md).

**Selected:** Per-output rendering plus proven SIP-on selection on the
intended display is a 0.1 consistency blocker. Keep safe-hide under
ambiguity until scope is proven; a single main-display or aggregate view
does not replace per-output rendering. Physical click, visibility,
hotplug/scale/reanchor, and suppression remain unverified.

Source inspection: AeroSpace v0.21.3-Beta
`Sources/AppBundle/ui/TrayMenuModel.swift:29-36,55-63` (one aggregated bar
content with per-monitor entries); FlashSpace v4.18.79
`FlashSpace/Features/MenuBar/MenuBarTitle.swift:16-34` and
`FlashSpace/Features/Workspaces/WorkspaceManager.swift:29-32` (active workspace
only despite a per-display model); Amethyst v0.24.3
`Amethyst/AppDelegate.swift:82-91` and
`Amethyst/Managers/ScreenManager.swift:382-390,406-408` (static menu plus
transient layout HUD, not a workspace pager); glazewm v3.10.1 `README.md:35,239,243-254`
(per-output workspace data exposed to external bars such as Zebar; separate
bar rendering was not verified).

## 2. Launchers

Windows Authentic Win+B uses the system browser association and Win+T opens
Windows Terminal; Compatible disables both
([Windows launcher shortcuts](../decisions/reference-and-shortcuts.md#windows-launcher-shortcuts)).
Plain Ctrl+B/T conflict with common editing/shell use; a global
collision-free claim is not available.

**Selected:** Configurable browser and terminal actions, unbound in both
presets. Suggest Ctrl+Alt+B/T. Use the system browser association and seed
the configurable terminal app with Terminal.app, without silent fallback.
No guarantee of a new window. No Fn/Globe tiler modifier in 0.1. See
[launchers and modifier scope](../decisions/reference-and-shortcuts.md#macos-launchers-and-modifier-scope).

Source inspection: AeroSpace v0.21.3-Beta `build-release.sh:71-72`,
`docs/config-examples/default-config.toml:106-110` (commented Alt+Return
Terminal), `docs/config-examples/i3-like-config-example.toml:18-21` (example
active), `docs/aerospace-exec-and-forget.adoc:19-34` (shell runner),
`docs/goodies.adoc:169-204` (named app activate vs new window); Amethyst
v0.24.3 `Amethyst/Preferences/UserConfiguration.swift:106-142,343,388` and
`Amethyst/default.amethyst:8-16,185-192` (no launch action; Ctrl+Alt+Shift+T
toggles tiling); yabai v7.1.25 `README.md:31,40`,
`examples/skhdrc:79-145` (external, comments); skhd v0.3.9
`src/skhd.c:489-491`, `examples/skhdrc:81,109-115` (user config; sample
Cmd+Return kitty, commented named Terminal); i3 4.25.1 `etc/config:57,63` and
`i3-sensible-terminal:13-21` (default Mod1+Return resolves terminal via
`$TERMINAL` then candidates); glazewm v3.10.1
`resources/assets/sample-config.yaml:259-263` (sample Alt+Return launches
named cmd or wt); Rectangle v2.0.3 `Rectangle/WindowAction.swift:708,719`
(alternate), Rectangle v1.100
`Rectangle/PrefsWindow/SettingsViewController.swift:1111` and Rectangle
v2.0.3 `Rectangle/TodoMode/TodoManager.swift:22-31,75-80` (old Ctrl+Alt+B
seed may survive upgrades; not the current default).

## 3. Fullscreen

Windows has project-owned enter/exit with exact preimage restoration; native
app-owned exit is separate
(`docs/changes/windows-fullscreen-exit-baseline.md:3-15,27-39`). On macOS,
composed Ctrl+M is maximize, the fixture AX setter is not a product route, and
the reported fullscreen capability is false; R14 nevertheless keeps fullscreen
in scope (`docs/decisions/contract.md:177-187`,
`docs/decisions/macos.md:110-115`,
`crates/tiler-macos/src/world_connection.rs:49-62`,
`docs/changes/macos-0.1-product-scope-audit.md:22-24`). Owning records:
[contract](../decisions/contract.md),
[macOS decisions](../decisions/macos.md),
[baseline](windows-fullscreen-exit-baseline.md),
[gate proposal](macos-0.1-gate-proposal.md),
[open decisions](macos-v4-open-decisions-review.md).

**Selected:** Pursue project-owned native `AXFullScreen`. Leave capability
unadvertised until owned exact restore, settled Space arrival, and recovery
are proven. Unknown app-owned exit remains refused. Composed maximize and
native fullscreen observation are separate and do not satisfy this gate.

Source inspection: AeroSpace v0.21.3-Beta
`Sources/AppBundle/command/impl/FullscreenCommand.swift:8-33`,
`Sources/AppBundle/layout/layoutRecursive.swift:99-103`,
`Sources/AppBundle/model/Monitor.swift:84-85` (synthetic visible work area),
`Sources/AppBundle/command/impl/MacosNativeFullscreenCommand.swift:4-8,18-37`
(separate AX route, flicker caution); Amethyst v0.24.3
`Amethyst/Layout/Layouts/FullscreenLayout.swift:11-30`,
`Amethyst/Model/Screen.swift:77-78`, `Amethyst/default.amethyst:33-36`
(adjusted-frame layout, default Opt+Shift+D, not AX); Silica v0.3.7
`Silica/NSScreen+Silica.m:26-31` (visibleFrame); yabai v7.1.25
`src/window_manager.c:2296-2323,2355-2398` and `src/display.c:123-170` (AX
native, BSP zoom, windowed saves/restores frame only, not a durable Windows
preimage); Rectangle v2.0.3
`Rectangle/WindowCalculation/MaximizeCalculation.swift:7-15`,
`Rectangle/WindowManager.swift:73-93,119-135`,
`Rectangle/RepeatedMaximizeRestore.swift:8-58` (visibleFrame maximize and
guarded frame restore, no managed-window native fullscreen); Loop 1.4.2
`Loop/Window Management/Window/Window.swift:214-227`,
`Loop/Window Management/Window Manipulation/WindowEngine.swift:87-89,113-121`,
`Loop/Extensions/NSScreen+Extensions.swift:38-59`,
`Loop/Window Management/Window Action/WindowAction+Defaults.swift:14-41` (AX
fullscreen separate from safe-frame maximize; fullscreen not default bound;
no claim of exact native preimage).

## 4. Sticky float

Check 6 (M/A/O) reads: "Repeat maximize/sticky native-change cycles and
physical hold/release; one toggle per intended activation, no stuck
consumption or OS leakage." Check 32 (M/H/U) reads: "Sticky only with positive
all-Spaces proof; restart preserves proven sticky identity and visibility."
Both are at `docs/changes/macos-0.1-gate-proposal.md:100,126`. The current
limit is macOS unsupported (`docs/decisions/window-state.md:55-60`,
`docs/decisions/contract.md:187`); both native sticky and suppression are
absent (`crates/tiler-macos/src/world_connection.rs:49-62`,
`docs/changes/v4-migration.md:966-972,1094`). A refusal cannot pass the
original checks, and positive proof cannot be inferred from subsets
(`docs/changes/macos-0.1-product-scope-audit.md:25,50`). Owning records:
[gate proposal](macos-0.1-gate-proposal.md),
[window-state decision](../decisions/window-state.md),
[contract](../decisions/contract.md),
[v4 migration](v4-migration.md).

yabai v7.1.25 `doc/yabai.asciidoc:666-668` and `src/window_manager.c:2215-2230`
use a scripting addition / partial SIP; this is not evidence for the selected
SIP-on host or an acceptable security change.

**Pending:** macOS currently does not advertise sticky. Run the bounded
SIP-on live probe before deciding its release disposition; checks 6 and 32
remain unchanged. Neither no-op nor refusal passes the current positive
journeys.

## 5. Confirmed TD decisions

### TD2 — KDE reconnect same-index units

Keep displaced same-index units separate and return their then-current
membership. Physical KDE RC proof is still required. See the
[owning decision](../decisions/workspaces.md#kde-common-model-output-reconnection),
[historical COSMIC comparison](../spec/reference-outcomes/multi-output.md),
and [reconnect record](v4-phase2-kde-physical-reconnect.md).

### TD3 — M4 maximize sequencing

Hold final slice-5 native acceptance until KDE ST/RC clears. No other
cross-platform sequencing dependency remains. Implementation has landed
and fixture apply/restore was observed; the signed two-launch restart
proof and physical Ctrl+M/paint/replacement journeys remain open. See the
[acceptance decision](../decisions/contract.md#contract-authority-and-v4-migration).

### TD5 — MX8 distribution

Developer ID signing and Apple notarization; GitHub Releases DMG and
Homebrew Cask, no Mac App Store and no in-app updater in 0.1. Installed
lifecycle and release-artifact acceptance remain open. See the
[distribution decision](../decisions/platforms.md#macos-01-distribution).

### TD6 — macOS 15 floor

Keep macOS 15 as the support floor. R2/F4 was verified offline; live macOS 15
Mission Control is still unverified and is an explicit risk. See the
[platform decision](../decisions/platforms.md#macos-phase-0-and-host-foundations)
and [gate proposal](macos-0.1-gate-proposal.md).

### TD7 — D4 store slot

Use the private per-session
`~/Library/Application Support/OmniTiler/intent-v1-<uid>-<asid>.json`
(`0700` dir, `0600` file, `O_NOFOLLOW`, atomic replace). Orderly-restart
fixture evidence is not re-login/mismatched-ASID or independent AX/CG
journey proof. See the [persistence decision](../decisions/persistence.md#macos-restart-intent-store).
