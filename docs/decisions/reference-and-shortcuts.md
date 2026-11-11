# Reference and shortcuts

### Reference outcome authority
- Decision: Reference-WM outcome matrix records minimal action sequences and per-WM outcomes as evidence feeding functional spec; existing selections remain authoritative and outcomes become supported variants only when explicitly selected, with setting when applicable. Keep index plus per-area files, stable scenario IDs/precise fixtures/actions/observations; new scenarios use Given/When/Then with one Then per each of eight existing references, niri, PaperWM, karousel, paneru, Ours KDE and Ours Windows. Qualify unsupported fixtures/actions and owner journeys; unknowns TBD. Evidence expands corpus, never selected behavior or consensus denominator.
- Why: not recorded.
- Assumes / revisit if: wide existing tables unchanged until separate migration; columns/viewport notation model-qualified. New reference-WM findings use each WM's latest stable release tag, shipped defaults and named variants (currently COSMIC epoch-1.10.0, Hyprland v0.56.2, i3 4.25.1, sway 1.12). Existing development-commit matrix findings stay at their recorded pins; re-check and relabel only when a decision relies on them again. KDE's separate 6.6-floor/latest-stable policy is unchanged. PaperWM.spoon corroboration only.
- Status: active, 2026-10-06
- Evidence: [reference outcomes](../spec/reference-outcomes.md).

### Reference tiers and defaults
- Decision: Tier 1 COSMIC, Hyprland, i3, sway decide defaults/settings; Tier 2 awesome, qtile, xmonad support/tie-break only; Tier 3 contextual only. Default COSMIC unless all three other Tier 1 or two plus Tier 2 broadly agree against it. Setting only if alternative has one Tier 1 plus another Tier 1/2. Earlier decisions stand unless revisited; gaming safety and host/capability exceptions unchanged.
- Why: tiering reflects reach; COSMIC broad modern reach, xmonad less popular, bspwm declining/X11/older style.
- Assumes / revisit if: user explicitly revisits selected behavior.
- Status: active, 2026-10-12
- Evidence: [user-approved reference tiers](../changes/archive/release-0.1-triage-decisions.md).

### Functional specification and unresolved work
- Decision: 2026-10-11 triage D01/D03/D09/D10/D12/D13/D16/D22/D23/D25/D26/D27/D28 became user-decided 2026-10-12. Functional spec remains one file; requirements normative only when selected by recorded decision, otherwise OPEN/PROVISIONAL. Workflows transfer across Linux/Windows/macOS. Add shortest discriminating scenario for each uncovered ambiguity; read source when confident, else leave user's test TBD. Defer GNOME Shell/Mutter/wlroots source pinning and fixture wording (302 residual cells/92 rows; 40 current host-source candidates, 38 PaperWM/2 sway) until a specific decision needs them; not active P1. Defer live-test reference VMs until after 0.1 or tabbed-stack design.
- Why: not recorded.
- Assumes / revisit if: compact citations resolve to dated tests, pinned source line ranges or linked docs. Every new reference-WM finding carries its source version inline next to the finding (for example, `i3 4.25.1 src/config.c:229`); older/development evidence remains revision-bound and potentially outdated, with no version inferred from a later checkout. Re-check and relabel existing evidence only when a decision relies on it again. Matrix hook names are provisional, not product commitments. Remove backlog rows tracking only physical checks; keep pending status in change records; removal never implies acceptance. AGENTS.md remains operational guidance.
- Status: active, 2026-10-10
- Evidence: [residual ledger](../changes/archive/reference-source-fill-remaining.md).

### Windows launcher shortcuts
- Decision: Default-on Win+B opens the Windows default web browser by system association, never a named browser; Win+T opens Windows Terminal. Browser opens empty `https://` through shell `open`; missing `wt.exe` logs narrowly with no fallback. Use conflict model: Authentic takes over; Compatible disables.
- Why: Auto-tiler users rely on launch shortcuts.
- Assumes / revisit if: Notification-area/taskbar-cycling native holders are general knowledge, unverified in repository. KDE/macOS counterparts unselected; physical check pending.
- Status: active, 2026-10-14
- Evidence: [launcher delivery](../changes/archive/windows-launcher-shortcuts.md).

### Shared shortcut defaults and conflict model
- Decision: Meta/Win defaults across platforms: arrows navigate focus, Shift+arrows move windows, and every HJKL directional binding has its arrow alias. Grow uses Meta/Win+Alt+HJKL and arrows. Initial release supports standard US keyboards with hardcoded shifted aliases; layout detection, omission, opt-in configuration and migration are deferred. macOS Ctrl is configurable Meta by default. Per-binding conflicts offer keep/override, disable, rebind; Compatible avoids known OS conflicts, Authentic matches tilers and overrides; preset may appear first run. macOS Authentic takes safe/nonbreaking window-management conflicts and native settings with Revert.
- Why: not recorded.
- Assumes / revisit if: no broad shortcut deletion, unverified actions, startup mutation or unauthorized foreign-holder mutation.
- Status: active, 2026-10-15
- Evidence: [KDE conflict model](../changes/kde-shortcut-conflicts.md).

### Windows gaming shortcut containment
- Decision: Windows Authentic defaults; Compatible available. Owned-chord interception is independent of foreground/action eligibility; pressing explicitly bound command, including Win+G while gaming, is user action. Win+L stays opt-in/unproven. Warn on effective Win+G/F11 only when positive incomplete containment is proven (Game Bar package `Registered` or Xbox configuration `Enabled`), plus kept Win+L when runtime opt-in permits unreliable lock override. Unknown/absent/failed probes warn nowhere and show ordinary no-known-conflict text. Disable, rebind-away, Compatible or keyboard takeover-off clears warning; no warning proves containment for any other unproven chord.
- Why: not recorded.
- Assumes / revisit if: containment gap parked; mechanism/architecture or registry/policy workaround requires user decision. Probes cheap/cached/off input/tile hot paths; failure never disables tiling. Game safety and capture needs win; no manual acknowledgement or automatic resolution selected. Game Bar editor only adds shortcuts; PowerToys remapping did not suppress them in user tests.
- Status: active, 2026-10-17
- Evidence: [Xbox research](../research/windows-port/xbox-shortcut-conflicts.md), [coexistence](../changes/windows-gaming-coexistence.md).

### Windows gaming probe and resolution boundaries
- Decision: Game Bar requires a positive supported current-user Appx registration query, not capture switches; GamingHomeApp requires a positively present/configured per-user value from an undocumented registry source. Log source and categorical outcome. Every warning surface (tray, badge, detail/actions, binding-list/OS text, preview) follows the positive-only rule. Resolve only opens Gaming Fullscreen settings or Game Bar Settings/app settings and explains capture/widgets loss, managed app-data loss and Store reinstall; navigation is not remediation. No uninstall, registry or policy writes.
- Why: not recorded.
- Assumes / revisit if: containment gap and workaround mechanism remain parked for user decision.
- Status: active, 2026-10-17
- Evidence: [Xbox research](../research/windows-port/xbox-shortcut-conflicts.md), [coexistence](../changes/windows-gaming-coexistence.md).

### KDE shortcut staging and Force/Revert
- Decision: KDE stages Keep/Disable and Authentic/Compatible choices in one selection-scoped Apply/Force/Revert flow; ordinary Save isolated. Keep preserves current assignment including custom/empty; Authentic resets canonical catalog, staged until explicit Apply/confirmed Force and consumed only on success. Compatible resets to Keep then disables known/discovered collisions, no replacement or automatic foreign-default restore. Force lists all current conflicts and exact chord/key changes; order blockers then clearable sorted by component/action, status/dialog agree, no truncation. Project actions, Lock Session and authorized System Monitor Meta+Esc are exempt; actual relocation is explained. Force disabled if holder cannot be safely cleared.
- Why: not recorded.
- Assumes / revisit if: confirmed Force revalidates owner/images/full holder snapshot, then rereads each holder before write; stale/refusal has zero writes. Persist cleared IDs before mutation; interruption-safe union; Revert restores KDE defaults only for listed non-project IDs, keeps project IDs cleared, retains list on partial failure. Stateless Revert and broad deletion rejected; custom cleared bindings are lost by accepted full-key-set restore.
- Status: active, 2026-10-17
- Evidence: [KDE conflict model](../changes/kde-shortcut-conflicts.md), [Apply failure](../changes/archive/kde-shortcut-apply-failure.md).

### Shortcut mutation reliability and required chords
- Decision: Bind writes per explicit Apply/Force/Revert; serialized background worker reports working status, disables controls, epoch-binds reload staging. No per-call deadlines unless D-Bus hang observed. Remove legacy project identity handling; foreign holders ordinary. Required-chord overrides only through reversible flow, never broad deletion or startup mutation: focus/move arrows replace Quick Tile/Previous-Next Screen; grow arrows replace Switch Window; Meta+G Grid View; Meta+M Monocle; Meta+L relocates Lock to Meta+Esc, with System Monitor sole authorized target occupant.
- Why: not recorded.
- Assumes / revisit if: no other foreign occupier authorized, no per-component cleanup path. Lock exemption line omitted if actual keys equal post keys.
- Status: active, 2026-10-14
- Evidence: [Apply failure](../changes/archive/kde-shortcut-apply-failure.md).

### Windows key input and Snap
- Decision: Windows uses WH_KEYBOARD_LL with vkE8 menu-mask at Win key-up while Win held, consuming only approved catalog chords; RegisterHotKey rejected. Win+L explicit opt-in only and unproven. Take over native Snap by default with visible opt-out; prevent keyboard via hook and mouse via session-only SPI_SETWINARRANGING while tiled, visible Apply/Revert, exact preimage/readback/conditional restore on stop. Snap Layouts flyout/Assist coverage unproven.
- Why: not recorded.
- Assumes / revisit if: observed Windows builds require pvParam FALSE with uiParam TRUE; verify on host. Do not claim Win+L reliability.
- Status: active, 2026-10-01
- Evidence: [Windows gaming coexistence](../changes/windows-gaming-coexistence.md).

### Windows first-run, rebinding and Xbox alternatives
- Decision: On normal startup without settings, offer native Authentic (default) / Compatible choices explaining Win+G/F11 implications. Acquire owner lease before UI; publish settings by atomic create-if-absent so a file appearing during prompt is never replaced. Graceful stop cancels the pending prompt without publishing and uses ordinary teardown. Each binding supports Keep/Disable/Rebind, separate directional letter/arrow rows and exact modifier arms. Compatible disables known conflicts only, without claiming conflict-free behavior; both presets retain resize defaults because ownership is unverified. Xbox Compatible float/fullscreen use Ctrl+Win+G/F11, Authentic retains Win+G/F11; exact bound Ctrl toggle arms share consumed hold/release behavior. General alternate is authentic chord + Ctrl only if unused; Ctrl+Win+Left/Right/D are counterexamples. No fallback for missing `wt.exe`; browser uses system default with empty `https://` URL.
- Why: not recorded.
- Assumes / revisit if: initial rebind target was Win plus only action's existing Shift arm; Alt/Ctrl and unshifted Win+L refused; resize rows initially unavailable and further workspace mappings awaited runtime. Later resize behavior is captured separately. Unsupported workspace actions remain unbound; physical interception/rebind adoption user-owned. Resolve actions navigate only, never remediate. Initial Compatible G/F11 reset-and-disable-without-replacements direction is superseded by Xbox alternatives/positive-warning rule.
- Status: active, 2026-10-26
- Evidence: [first-run](../changes/archive/windows-first-run-stop.md), [Xbox conflicts](../research/windows-port/xbox-shortcut-conflicts.md).

### Windows keyboard resize bindings
- Decision: Windows resize uses dedicated Win+Alt grow and Win+Shift+Alt shrink intents, with H/J/K/L and arrow aliases, shared Engine pixel steps matching KDE, and per-binding Keep/Disable/Rebind controls with exact modifier arms. Both presets retain these defaults because OS ownership is unverified. Exact-owner test CLI `resize --direction DIR --mode MODE` uses production fences; it does not bypass the injected hook.
- Why: not recorded.
- Assumes / revisit if: physical interception and rebound adoption remain user-owned.
- Status: active, 2026-10-26
- Evidence: [keyboard resize](../changes/archive/windows-keyboard-resize.md).

### macOS preview presets and native shortcut ownership
- Decision: explicit Apply only is active (User 2026-11-10): no automatic startup Apply. Preview presets remain tentative: Authentic default consumes exact supported chords; Compatible leaves established unshifted Ctrl+arrow conflicts to macOS. Shift move/Alt resize have no established native holder; local catalogue mirrors supported focus/move/float/grow/shrink/orientation, unsupported workspace/digit/native-state actions unbound. No persistent symbolic-hotkey writes needed; stop releases ownership. Native shortcut Apply enables Desktop-N through session SkyLight API; Revert conditionally restores exact observed enabled/chord preimages, preserving later edits; private durable backup precedes mutation with OS-released lock. No plist writes. Remove the current Authentic startup Apply during the port, not this records change.
- Why: not recorded.
- Assumes / revisit if: remaining preset/ownership behavior is tentative. Native Apply/readback, fresh-process session-effective state and exact Revert were verified within one login session with unchanged preferences; logout/reboot persistence and native chord delivery remain unverified. Per-drain holder refusal and first-run setup cost require native evidence after slice 1 builds; the source test is unrun. This does not establish product workspace actions ready. Settings mutation requires owned preimages and Revert.
- Status: explicit Apply only active, confirmed 2026-11-10; remainder tentative, 2026-10-16
- Evidence: User 2026-11-10; source-read pre-port review: `crates/tiler-macos/src/tile.rs:2091-2127` (startup Apply contradicts `SESSION_PERSISTENCE_NOTE` at `desktop_shortcuts.rs:58-61`), `crates/tiler-macos/src/workspace_send_managed.rs:753-838` and `desktop_shortcuts.rs:438-458` (per-drain refusal), `workspace_send_managed_tests.rs:985-1034` (present, unrun); [review limits](../changes/v4-phase3-macos-port.md#pre-port-review-outcomes-2026-11-10), [macOS dogfood](../changes/macos-dogfood-preview.md), [managed workspaces](../changes/macos-managed-workspaces.md#native-settings-and-occlusion-diagnosis-checkpoint-2026-10-16).
