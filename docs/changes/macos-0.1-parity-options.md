# macOS 0.1 parity options

**Status:** Decision-ready research. No decision is enacted, no native
acceptance is claimed, and no product changes were made. This note excludes
the MX6 inventory-gap question and the fullscreen fixture procedure (both
handled separately), all KDE/Windows work, and any relaxation of the
workspace, drag, fullscreen-carry, Settings or underlay gates
([macOS gate proposal](macos-0.1-gate-proposal.md),
[scope audit](macos-0.1-product-scope-audit.md),
[platform scope](../decisions/platforms.md)).

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

- **A — Lead recommendation (0.1).** Retain safe-hide and state the parity
  limit explicitly. Consequence: no rich multi-display UI, but no incorrect
  output attribution and no wrong-destination clicks.
- **B.** Status item scoped to the main display, or to a reliably resolved
  focused display, identified in the UI. Main is stable but hides other
  outputs; focus scope follows work but needs focus/Space identity and
  click-time proof. Safe-hide when scope is unknown.
- **C.** One status item with labeled groups per output (aggregate, not
  separate overlays). More coverage without multiple surfaces, but needs an
  authoritative output mapping and safe click routing.
- **D.** Separate pager per physical output: closest Windows parity, but needs
  stable physical identity, hotplug/scale/reanchor/suppression coverage and
  physical click/visibility proof.

No option waives MX6 safe-hide under ambiguity or the two-display operating
gates.

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
(`docs/decisions/reference-and-shortcuts.md:24-37`). The macOS counterparts
are unselected and there is no existing macOS gate leg
(`docs/changes/macos-0.1-product-scope-audit.md:41,50`). Plain Ctrl+B/T
conflict with common editing/shell use; a global collision-free claim is not
available. Owning records:
[shortcut decision](../decisions/reference-and-shortcuts.md),
[scope audit](macos-0.1-product-scope-audit.md).

- **A — Lead recommendation.** Configurable browser/terminal actions, unbound
  in both presets until checked; optional suggested Ctrl+Alt+B/T or user
  rebind. Browser goes through system association; terminal is an explicit
  configurable app with Terminal.app as a proposed seed and no silent
  fallback. Decide launch/focus versus guaranteed new window separately; do
  not promise the latter. Consequence: portable capability without an
  unproven default binding.
- **B.** Authentic-only Ctrl+Alt+B/T default, Compatible unbound. Familiar
  mnemonic, but Rectangle's alternate Ctrl+Alt+T collides and an older
  Ctrl+Alt+B seed may survive upgrades.
- **C.** Authentic-only Ctrl+Alt+Shift+B/T. More cumbersome, and Amethyst's
  T binding conflicts; Ctrl+Alt+Return for terminal is also Rectangle's
  alternate maximize, so it is not safer evidence.
- **D.** Omit portable actions explicitly for 0.1 and rely on external tools;
  no built-in convenience.

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

- **A.** Named windowed work-area fill overlay with explicit restore and
  retained membership, with no native Space or game fullscreen claim.
  Consequence: overlaps existing M4 unless a distinct outcome is chosen;
  needs ownership, protected preimage/geometry restoration and no unsafe
  writes.
- **B.** Project-owned AXFullScreen native enter/exit: the closest distinct
  user-facing fullscreen, but with a different Space lifecycle. Before
  advertising, prove eligibility, ownership, transition observation, exact
  restoration of authorized touched preimage/retained base, refusal of unknown
  app-owned exit, and recoverability.
- **C — Lead recommendation (0.1).** Explicitly defer only the project-owned
  actuator by an approved R14 scope amendment; keep composed maximize and
  native fullscreen observation, with under-fullscreen policies and carry
  gates unchanged. Consequence: no parity or pass claim; revisit only if a
  recoverable proven route exists.

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

- **A — Lead recommendation.** User-approved gate amendment: check 6 keeps
  maximize cycles and physical hold/release (one toggle, no stuck consumption
  or leakage) and explicitly excludes native sticky repetitions; check 32
  replaces the native adoption/persistence claim with a must-pass
  unsupported/no-claim check — publish unsupported, refuse sticky targets
  without mutation, and do not infer or persist sticky from partial or unknown
  visibility, including restart. Mark the original positive sticky journey
  explicitly outside macOS 0.1, never passed, with no M-to-K reclassification.
- **B.** Keep both original rows and block release until there is a supported
  route under the existing host posture, positive per-display
  all-managed-Space evidence, lifecycle/restart visibility, and input and
  safety proof.
- **C.** Hold release with the known unsupported limit and queue the decision:
  no changed gate and no acceptance. Differs from B by making no
  implementation commitment.

## 5. Five independent TD decisions

These are separate decisions. Existing wording is in
[decisions.md](../decisions.md) and
[backlog.md](../backlog.md).

### TD2 — KDE reconnect same-index units

- **Confirm.** Keep displaced same-index units separate and return their
  then-current membership; KDE RC remains required.
- **Retire.** Leave the merge policy undecided pending a discriminating
  result. This does not authorize merges or change current behavior.
- **Lead recommendation: confirm, with narrower wording.** Source-only
  comparator: historical COSMIC pins `cosmic-comp @
  3d55cba06c9cf6f27609cdefb520f7857dba20af src/shell/mod.rs:867-935,937-1029`
  and `src/shell/workspace.rs:589-645`, plus
  `docs/spec/reference-outcomes/multi-output.md:432-453,488-498`. The offline
  correction did not settle policy
  (`docs/changes/v4-phase2-kde-physical-reconnect.md:386-390`); KDE RC is still
  required. See [multi-output reference outcomes](../spec/reference-outcomes/multi-output.md)
  and [reconnect record](v4-phase2-kde-physical-reconnect.md).

### TD3 — M4 maximize sequencing

- **Confirm.** Preserve KDE ST/RC as a prerequisite for final slice-5 native
  acceptance. Implementation has landed and fixture apply/restore was
  observed; other macOS acceptance is still open.
- **Retire.** Cancel the sequencing dependency by explicit approval while
  keeping all macOS acceptance; this is not by itself a pass or unblock.
- **Lead recommendation: confirm the hold.** The distinct guarded two-launch
  controller and physical Ctrl+M/paint/replacement journeys remain open
  (`docs/changes/macos-v4-open-decisions-review.md:309,345`;
  `docs/backlog.md:43-45,138,145`). See
  [open decisions](macos-v4-open-decisions-review.md) and
  [backlog](../backlog.md).

### TD5 — MX8 distribution

- **Confirm.** Non-sandboxed Developer ID + notarized DMG via GitHub Releases
  and Homebrew Cask; no Store and no in-app updater.
- **Retire.** Drop the channel choice and stage a notarized DMG only, with an
  explicit Cask deferral. A Store alternative requires scope/security redesign
  and is not equivalent.
- **Lead recommendation: confirm.** No installed lifecycle or notarization
  gate pass follows from this choice
  (`docs/research/cross-platform-support/feasibility.md:149`;
  `docs/macos-dev-environment.md:159-170`;
  `docs/decisions/platforms.md:17-22`;
  `docs/changes/macos-0.1-gate-proposal.md:178`). See
  [feasibility](../research/cross-platform-support/feasibility.md),
  [dev environment](../macos-dev-environment.md),
  [platforms decision](../decisions/platforms.md),
  [gate proposal](macos-0.1-gate-proposal.md).

### TD6 — macOS 15 floor

- **Confirm.** R2/F4 dual-surface fix has landed; keep the macOS 15 floor as
  an unverified risk, with an optional floor run at RC.
- **Retire.** Condition floor sign-off on live macOS 15 evidence, adding a
  release dependency; no silent floor cut.
- **Lead recommendation: confirm.** The fix is verified offline; the macOS 15
  live Mission Control journey is not
  (`docs/changes/macos-v4-open-decisions-review.md:36,42,296,312`;
  `crates/tiler-macos/src/mc_probe.rs:92-160`;
  `docs/changes/macos-0.1-gate-proposal.md:209`). See
  [open decisions](macos-v4-open-decisions-review.md) and
  [gate proposal](macos-0.1-gate-proposal.md).

### TD7 — D4 store slot

- **Confirm.** Implemented private per-session
  `~/Library/Application Support/OmniTiler/intent-v1-<uid>-<asid>.json`
  (0700 dir, 0600 file, `O_NOFOLLOW`, atomic replace).
- **Retire.** Drop the fixed slot as contract and reopen design only for a
  material failure (migration/identity risk), maintaining safe storage
  meanwhile.
- **Lead recommendation: confirm.** Orderly-restart fixture evidence is
  bounded; it is not re-login/mismatched-ASID proof and not independent AX/CG
  journey proof (`crates/tiler-macos/src/intent_store.rs:1-28,449-528,800-823`;
  `docs/backlog.md:44`; `docs/decisions.md:69-73`). See
  [backlog](../backlog.md) and [decisions](../decisions.md).

## Next step

The user selects outcomes; owning records and the gate, plus minimal
discriminating rows if needed, are updated only after those selections. That
update is not done in this research note.
