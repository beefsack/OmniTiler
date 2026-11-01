# Settings, tray and first-run

### Shared settings catalog and platform ownership
- Decision: Pre-0.1 route D uses dependency-free Rust core catalog and native Windows tabbed Settings renderer. Catalog owns stable tab/section/field IDs, types, tokens, defaults, bounds, functional labels/help, validity and timing; widgets, storage, load policy and operations stay platform-owned. Toolkit types/serialized JSON stay out of core. Windows v1 nested snake_case retains strict refusal/last-good; KDE keys/tolerant decoding and saved/requested/confirmed state remain. Startup-only/next-admission notes describe actual adoption, not Save confirmation. Windows tabs: Tiling & Workspaces (gaps, workspace defaults/scope, move, admission/refill); Appearance (border/underlay); Shortcuts (takeover, Win+L, presets, bindings, Xbox/Game Bar); Desktop Integration (mouse Snap/taskbar pager). Preserve persistent status/conflict feedback and scrolling long notes; Windows-only fields/native panels stay adapter-owned.
- Why: not recorded.
- Assumes / revisit if: KDE catalog migration uses generated descriptors and drift checking; macOS participation stays consumer-driven. Existing platform storage/adoption contracts remain authoritative.
- Status: active, 2026-10-16
- Evidence: [Windows catalog](../changes/shared-settings-catalog-windows-tabs.md), [KDE catalog](../changes/archive/kde-settings-core-catalog.md).

### Settings correction and proof-owner boundary
- Decision: Apply/Force/Revert is the only correction flow. Ordinary Save never mutates shortcuts/host keys; install/startup never mutates global shortcuts. A generic field setter never invokes takeover/remediation, presets, conflict operations or KDE Fix/Force/Revert. Proof owners stay isolated from normal configuration.
- Why: not recorded.
- Assumes / revisit if: KDE behavior unchanged by Windows catalog migration; macOS settings participation is consumer-driven, not probe-flag migration. Catalog-driven live apply is launch blocker except startup-only KDE `workspaceMode`.
- Status: active, 2026-10-16
- Evidence: [shared-settings boundary and compatibility](../research/shared-settings-schema.md#recommended-boundary-and-compatibility), [Windows catalog implementation](../changes/shared-settings-catalog-windows-tabs.md).

### KDE settings package ownership
- Decision: Distro `omnitiler` core ships both KWin-independent Settings KCMs, providing tiling and Settings/Apply/Revert without optional effect; `omnitiler-native-effect` carries only KWin-ABI-bound effect. Core means distro package; no KDE code in `crates/tiler-core`. Nix exposes independent native-settings and effect-only native-effect; NixOS module installs both with script. Removing effect leaves Settings/Revert; after removing core, recovery requires reinstalling core.
- Why: not recorded.
- Assumes / revisit if: package ownership boundary unchanged.
- Status: active, 2026-10-11
- Evidence: [release packaging](../changes/archive/release-0.1-core-settings.md).

### P2 settings follow-up scope
- Decision: Post-0.1 P2 settings include approved D09, D10 and D27, with their labels/options/tooltips in Move commands and Activation requests. D03, D12, D13, D16, D22, D23, D25 and D28 have no setting.
- Why: not recorded.
- Assumes / revisit if: names/options remain functional names; reference WMs only in tooltips.
- Status: active, 2026-10-12
- Evidence: [approved P2 settings scope](../changes/archive/release-0.1-triage-decisions.md).

### Windows Settings surface and persistence
- Decision: Windows schema-v1 JSON is validated at `%LOCALAPPDATA%` by the app; normal owners load at startup and poll on the existing pump, explicit CLI switches win per field, and malformed live files retain last-good state. Apply validates and atomically saves; Revert discards the draft and reloads saved state; Close never saves. Save does not claim runtime delivery; presets stage changes and Revert never undoes an earlier Apply or restores shortcut defaults.
- Why: not recorded.
- Assumes / revisit if: external stale edits continue to trigger reload/refusal; mouse prevention uses session-only preimage/readback/conditional restoration, without policy writes.
- Status: active, 2026-10-08
- Evidence: [Settings evidence](../changes/archive/windows-settings.md), [dialog evidence](../changes/archive/windows-shortcut-settings-dialogs.md).

### KDE unified settings and host conflicts
- Decision: One KDE Settings page serves tray, KWin Scripts Configure and Desktop Effects Configure. Saving changed gaps requests reconfigure and rereads validated values; request alone is not confirmation. `workspaceMode` startup-only with restart note. Removed `tilingAlgorithm`/`automaticSplitTarget`/`dropOutlinePreview` neither read nor rewritten; hidden `shortcutProfile` saved value/startup read untouched until distinct profiles exist. On open, read `kwinrc [Windows]` ElectricBorderTiling, ElectricBorderMaximize, ElectricBorders with short explanations. Two Boolean rows always visible: Fix writes false when on; Revert removes local key when off so KDE 6.7.5 default true applies. ElectricBorders row only if nonzero; sole Fix removes local key so default 0 applies. No prior-value journal/ownership tracking.
- Why: not recorded.
- Assumes / revisit if: explicit KConfig changes send reconfigure/read back effective config; failed write/send shown/logged; queued send is not proof compositor applied. Startup/Save never mutate host keys. Host warning badge + top menu row opens Settings; left-click still menu, snapshot loss separately NeedsAttention; no notification/direct Settings-on-icon-click. Before uninstall, docs/package descriptions/post-removal note tell user press Revert; automatic revert parked after 0.1.
- Status: active, 2026-10-10
- Evidence: [KDE host conflict record](../changes/archive/kde-shortcut-result-dialogs.md).

### Settings result dialogs
- Decision: KDE explicit Apply Shortcuts acts immediately when conflict-free; conflict opens one scrollable modal preview with Force Apply/Cancel. Apply/Force/Revert applies immediately and shows result. Success text is `Shortcuts applied.` for Apply/Force and `Shortcuts reverted to defaults.` for Revert; no overall Settings Apply/OK instructions. Errors are single-line reason, max 240 chars with `...` truncation; full diagnostics remain journaled. No preliminary Apply or second Force confirmation; conflict dialog sole Force confirmation, accepted-for-now; Revert keeps custom-binding-loss confirmation. Windows owned modal results say `Settings saved.` / `Reverted to saved settings.`; single-line errors preserve full diagnostics; save isn't runtime delivery. Advisory Apply/Cancel preview, no extra confirmation; whole-draft discard.
- Why: not recorded.
- Assumes / revisit if: shipped UX remains accepted-for-now; later UX review is nonblocking. Errors stay single-line, capped at 240 characters for KDE, with full diagnostics in journal.
- Status: active, 2026-10-17
- Evidence: [KDE dialogs](../changes/archive/kde-shortcut-result-dialogs.md), [Windows dialogs](../changes/archive/windows-shortcut-settings-dialogs.md).

### Tray ownership and status
- Decision: KDE uses a portable Rust StatusNotifierItem, alive without watcher and registering to the live-confirmed watcher owner. It accepts outbound state snapshots only from current KWin's unique D-Bus owner; reject/log lower same-generation revisions, permit equal-revision heartbeat refresh, revoke trust on equal-revision content conflict. Fresh authenticated snapshot means Active; missing/stale means NeedsAttention. Host-conflict badge is distinct. Use Home Manager systemd user unit with Restart=on-failure; no supervisor; second instance exits successfully.
- Why: not recorded.
- Assumes / revisit if: no shell/input/general helper-to-KWin route; narrow action-name toggle and saved-default/reconfigure writes remain. Diagnostics go to stderr/journal.
- Status: active, 2026-09-28
- Evidence: [tray implementation](../changes/archive/kde-settings-core-catalog.md).

### Windows tray, pager and first run
- Decision: One normal owner owns one official Shell_NotifyIcon icon with stable GUID; both clicks open menu. TaskbarCreated revalidates/re-adds it; proof owners never create tray/prompt. Menu has conflict row, status, workspace tiling, new-workspace default, Settings and Stop; Settings/conflict open singleton UI, Stop tears down and deletes icon with recovery-lease dead-owner cleanup. Explorer may initially place icon in overflow. Stop menu handler uses 50ms menu-thread check and EndMenu.
- Why: not recorded.
- Assumes / revisit if: tray pager remains single-output stopgap; embedded taskbar pager is separate. Tray number/menu stays available when taskbar pager suppressed.
- Status: active, 2026-10-17
- Evidence: [tray](../changes/archive/windows-tray-first-run.md).

### Windows tray workspace pager
- Decision: Tray icon overlays focused-output active workspace number on existing glyph, preserving warning badge. Menu adds plain `Workspace N` row for each focused-output workspace in order before Settings; active row radio-checked. Both left/right click retain existing menu; no popup, app icons or urgency. Existing tray remains for settings/toggles.
- Why: not recorded.
- Assumes / revisit if: this is single-output native accepted stopgap, distinct from visual taskbar pager.
- Status: active, 2026-10-15
- Evidence: [tray evidence](../changes/archive/windows-tray-first-run.md).

### Windows taskbar pager placement and geometry
- Decision: Default-on `core.taskbar_pager` opt-out places a numbered Plasma-style pager inside each output taskbar showing only its workspaces, miniature window rectangles/icons, click-to-switch. Host it as nonactivating topmost external tool-window overlay anchored left of tray, not Explorer child. Always anchor left of tray; may overlap app icons when crowded but never tray, never shrink/hide for app crowding. If fixed-width cells exceed entire tray-left span, hide pager, log reason, retain tray number/menu. Never guess secondary-anchor or non-bottom-taskbar geometry.
- Why: not recorded.
- Assumes / revisit if: AppBar reservation and Explorer/XAML injection/hooks rejected (maximized-window interactions, shell/update stability, AV/anti-cheat/security). Tray placeholders rejected (position not guaranteed/icons hideable). Native rendering, click, refresh and managed-fullscreen hide/return passed one output; child-parenting failure unexplained but not blocking. Multi-output, games, real Explorer restart, DPI/auto-hide and over-wide strips unverified.
- Status: active, 2026-10-16
- Evidence: [product overlay](../research/windows-port/taskbar-workspace-indicator.md#product-overlay-2026-10-16).

### Windows taskbar pager artwork and typography
- Decision: Obtain window/class/executable artwork asynchronously; omit unavailable or too-small icons without changing geometry. Cap icons at 16 logical px, minimum 8 logical px, DPI-scaled, with one physical border and one padding pixel clear per side. Miniature icons render at 50% opacity (128/255 artwork weight over existing background, preserving source alpha); workspace numbers are 10pt Segoe UI Bold. Use centered black halo: eight symmetric text copies at one DPI-scaled logical pixel radius (1 physical px at 96 DPI, 2 at 144/192 DPI), then centered white glyph. Keep label centering, cells, strip geometry and tray gap unchanged.
- Why: not recorded.
- Assumes / revisit if: exact packaged artwork deferred; plain gray miniature initially retained, then focused-window styling superseded gray only for focused miniature. Wider visual variants remain pending.
- Status: active, 2026-10-19 to 2026-10-24
- Evidence: [pager dogfood](../changes/windows-taskbar-pager-icons.md), [single-output acceptance](../changes/windows-single-output-dogfood.md).

### Windows taskbar pager focus color and focus source
- Decision: Focused miniature uses independently resolved full highlight border and dimmer fill; opaque channels scale by 0x66/255; unfocused miniatures retain gray border/fill. Focus comes from accepted reply foreground token, not another focus-policy owner. Active-cell fill always uses available Windows accent regardless border-theme switch, fallback `#2a82da`; use documented WinRT source and broadcast-driven refresh, no pager theme setting.
- Why: not recorded.
- Assumes / revisit if: system accent physically accepted after 10/10 starts; broader broadcast/fault paths remain unverified. Correct native stacking accepted; physical acceptance does not cover wider variants.
- Status: active, 2026-10-19 to 2026-10-24
- Evidence: [pager dogfood](../changes/windows-single-output-dogfood.md).

### Windows taskbar pager suppression and safety
- Decision: Hide pager throughout taskbar auto-hide, even while taskbar temporarily appears; retain tray number/menu; when disabled restore from fresh geometry. Suppression also drops queued clicks, including managed fullscreen. Elevated-foreground gate suppresses only successfully read integrity >= high (`0x3000`), not low/AppContainer/medium; process-open/integrity-read failure fail closed. Keep no-foreground/own-process skips and fullscreen/game suppression. This is integrity classification, not Explorer/Start/window-class exemption; tiling admission/dispatch/fullscreen manageability retain separate medium-only policy. Transition-only native readbacks after actual shows plus existing topmost recovery report visibility, checked topmost, and bounded above/below/unknown order relative to the primary taskbar; no timer/polling is added.
- Why: not recorded.
- Assumes / revisit if: normal transition-only readbacks after actual shows, plus existing topmost recovery, report visibility, checked topmost and bounded above/below/unknown order to primary taskbar; no timer/polling. A policy gate is not proof of presentation.
- Status: active, 2026-10-17 to 2026-10-19
- Evidence: [pager evidence](../changes/windows-taskbar-pager-icons.md).

### Windows taskbar pager bounded work
- Decision: Fixed five-second overlay and icon-worker spawn retries; identity-deduplicated icon queues without arbitrary entry caps; retain only latest pending click. Positive artwork refresh every ten seconds because Windows has no icon-change event; negative retry every five seconds because apps often set icons after creation.
- Why: not recorded.
- Assumes / revisit if: no added polling timer beyond specified positive icon refresh; pending click is latest only.
- Status: active, 2026-10-16
- Evidence: [pager implementation](../changes/windows-taskbar-pager-icons.md).

### macOS workspace indicator
- Decision: Bounded select-only preview defaults to one native menu-bar item listing existing Space numbers in native order. `all` is default; `current` and `off` are alternatives; optional icons default false. Current highlight is independent of occupancy. Selection uses guarded AX route and refuses stale identities; ambiguous multi-display scope hides the item; numbers beyond armed 1-9 remain visible but disabled.
- Why: not recorded.
- Assumes / revisit if: membership means retained managed-window membership, not all desktop apps; no trailing Space/lifecycle policy or polling loop. Physical/game/fullscreen acceptance remains TBD.
- Status: active, 2026-10-17
- Evidence: [indicator record](../changes/macos-workspace-indicator.md).
