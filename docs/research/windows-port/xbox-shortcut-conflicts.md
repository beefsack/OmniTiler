# Windows Xbox shortcut conflicts: research and proposal

- Research date: 2026-10-16. Status: research snapshot; the implementation
  choices below are superseded by the approved decisions immediately below.
- User decision 2026-10-17 (supersedes the 2026-10-16 conservative-warning
  choices in this snapshot for BOTH Game Bar and GamingHomeApp): warnings only
  on provably present conflict. Game Bar positive installed query only;
  failure/unknown same as absence, no notes/conflict rows/badges.
  GamingHomeApp positively present/configured only; unknown/absent/unreadable
  no amber/tray/Settings warning. Honest normal-level probe-outcome logs
  retained. Code implementation pending Part B.
- Approved 2026-10-16: Compatible uses Ctrl+Win+G (float) and Ctrl+Win+F11
  (fullscreen); Authentic keeps Win+G/F11. Read-only Xbox detection uses the
  undocumented per-user `GamingHomeApp` mapping: exact Xbox AUMID on, present
  empty REG_SZ off; absent/unreadable/unrecognised means unknown with warning
  and Settings navigation. Game Bar uses current-user package registration.
  Resolve actions open Settings pages only; no uninstall or registry/policy
  writes. The conservative Xbox unknown fallback is confirmed, not acknowledgement.
  User-approved 2026-10-16 Game Bar correction (delivered offline): successful
  absence clears all Win+G conflict warnings/notes/rows; detected installation
  warns. "Just in case" checks would logically require warning for every chord.
  Query failure remains distinct from absence; preserving the current explicit
  unknown/query-failed warning is reported for review, not newly approved.
  [Decisions](../../decisions.md#shortcuts-conflicts-and-presets) and
  [implementation/physical acceptance](../../changes/windows-gaming-coexistence.md)
  are current; descriptions of disabled Compatible chords and open choices
  below document the pre-implementation research, not current product policy.
- Approved direction: Compatible rebinds the two tiler actions; Authentic keeps
  the authentic chords and recommends disabling Xbox mode and removing Game Bar.
  Resolve actions must be safe and reversible. Try authentic plus Ctrl first,
  except where Windows already uses it; otherwise choose another sensible unused
  chord. This rule is general; this investigation changes no other binding.
- Recommendation: propose `Ctrl+Win+G` for float and `Ctrl+Win+F11` for fullscreen;
  offer official Settings links, not registry writes or automatic removal.
  Exact alternate adoption and detection/resolve UX below remain for review.
- Evidence boundaries: source/catalog review and web documentation only. The
  [incident record](../../changes/windows-gaming-coexistence.md) establishes
  consumed Win+F11 leaking into Xbox mode, not a reliable suppression mechanism.
  User tests establish Ctrl+Win+G does not open Game Bar, Game Bar's shortcut
  editor only adds shortcuts, and PowerToys remapping did not suppress them.
  No package presence, configuration state or physical acceptance is inferred
  from a development system.

## 1. Current bindings and proposed Compatible replacements

| Action / catalog ID | Authentic today | Compatible today | Proposed Compatible | Evidence / remaining check |
| --- | --- | --- | --- | --- |
| Toggle floating window / `toggle-float` | `Win+G` | Disabled; no replacement | `Ctrl+Win+G` | User verified no Game Bar opening; actual tiler routing/holds still unverified |
| Toggle fullscreen window / `toggle-fullscreen` | `Win+F11` | Disabled; no replacement | `Ctrl+Win+F11` | No matching shortcut in Microsoft's Windows 11 list or Xbox-mode guide; physical Xbox-mode nonactivation TBD |
| Toggle sticky floating window / `toggle-sticky` | `Win+Shift+G` | Kept | Unchanged | Distinct chord, not the float replacement |

- Source: [`catalog.rs`](../../../crates/tiler-windows/src/settings/catalog.rs),
  `binding_catalog`, `FOCUS_MOVE_ROWS`, `RESIZE_ROWS`, `HISTORY_ROWS`,
  `RELATIVE_SEND_ROWS`, `OUTPUT_SEND_ROWS`, `MIGRATION_ROWS`, `workspace_row`;
  [`presets.rs`](../../../crates/tiler-windows/src/settings/presets.rs),
  `apply_preset` and `compatible_disabled_ids` (41 disabled IDs). Authentic
  resets overrides; Compatible currently resets then disables known conflicts.
- Checked both candidates against the entire default catalog and the subset
  retained by Compatible: neither is occupied. Existing Ctrl defaults use
  Tab/H/J/K/L/arrows for history, Ctrl+Shift for relative sends and Ctrl+Alt for
  output sends. Numbered stay, relative stay, output stay and migration defaults
  are unbound. Resize uses Alt, not Ctrl+G/F11. Custom user bindings still need
  the existing duplicate validation when a proposed preset/rebind is applied.
- Microsoft's [Windows shortcuts][shortcuts] includes Ctrl+Win+Left/Right/D/F4
  for virtual desktops and other Ctrl+Win commands, but neither candidate;
  [KB5070297][xbox-mode] documents Win+F11, not Ctrl+Win+F11. Absence from these
  lists is not a reservation guarantee or proof against OEM/third-party owners.
  [RegisterHotKey][register-hotkey] says Win combinations are reserved for the
  OS. Keep the existing mechanism; do not claim these become RegisterHotKey-safe.
- These are not settings-only changes: `binding_modifiers_ok` currently refuses
  Ctrl for these toggles; `snapkey/classifier.rs::direct_action` and
  `snapkey/push_command.rs::{push_fullscreen,push_g_arm}` also reject fresh Ctrl
  toggle arms. Changing only preset strings/validation would leave routing
  incorrect. Support the two rebound arms without claiming fresh unbound Ctrl
  combinations or widening unrelated actions. Preserve consumed-hold/release,
  repeat and modifier-change invariants.

### Other current Authentic collisions: list only

| Current default chord(s) | Tiler use | Documented Windows use / qualification |
| --- | --- | --- |
| `Win+H/J/K/L` | Directional focus | Voice dictation / Recall on supported devices / Cast / lock; Win+L requires existing explicit opt-in |
| `Win+Left/Right/Up/Down` | Directional focus | Snap / maximize / minimize |
| `Win+Shift+Left/Right/Up/Down` | Directional move | Move between monitors / vertical stretch / restore |
| `Win+0..9`, `Win+Shift+0..9` | Workspace select / send-follow | Taskbar launch/switch / new instance |
| `Win+M/O/Y` | Maximize / orientation / workspace tiling | Minimize all / orientation lock / Mixed Reality input switch (feature-dependent) |
| `Win+B/T` | Browser / terminal launch | Notification-area focus / taskbar cycle |
| `Ctrl+Win+Left/Right` | Previous/next workspace | Native virtual-desktop switch |
| `Win+Alt+Up/Down` | Grow up/down | Snap to top/bottom half |
| `Win+Alt+H/K` | Grow left/up | Voice-typing keyboard focus when voice typing is open / microphone mute in supported apps |

- Source comparison: catalog above versus [Windows 11 shortcut tables][shortcuts].
  The last two rows are also kept in Compatible today; source labels resize
  ownership unverified. Flagged only, no preset/catalog expansion selected.
  No additional current default Game Bar capture chord match was established.

## 2. Read-only configuration detection

### Xbox mode: availability, enabled configuration and active mode differ

- **Documented:** [KB5070297][xbox-mode] requires Windows 11 24H2+ and a
  supported market. Other PCs use Settings > Gaming > Xbox mode > Enable Xbox
  mode. Handhelds use Choose home app; None turns it off. Startup and Task View
  visibility controls are not disable controls. Win+F11 enters/exits when enabled.
- **Documented navigation, not detection:** [Settings URI reference][settings]
  lists `ms-settings:gaming-fullscreen` for Xbox Mode. Page availability varies
  by version/SKU. A version check gives only possible applicability; URI launch
  success proves neither availability of that specific page nor enabled state.
  Do not launch Settings as a background probe, scrape its UI, or enable a
  feature flag to discover state.
- **No supported cheap enabled/available query established:** reviewed Microsoft
  Support/Learn material provides no Xbox-mode configuration getter, registry
  value contract, feature-availability API or active-mode event. Report
  `unknown / check Settings`, not `enabled`, when that is all the evidence says.
  This is a bounded research result, not a claim that Microsoft has no such API.
- **Undocumented/unstable candidate, not recommended for the initial detector:**
  [ElevenForum][xbox-registry] maps
  `HKCU\Software\Microsoft\Windows\CurrentVersion\GamingConfiguration`
  string `GamingHomeApp` to the Xbox app AUMID for on and empty for off.
  A single read would be cheap and unelevated under ordinary ACLs, but absent
  values, alternate home apps, form factors and future Windows versions have no
  established contract. Do not infer off from missing/empty or authorize writes
  from this recipe. Microsoft's [same-named unattend setting][unattend] is an
  OEM provisioning interface, not documentation of this HKCU mapping.
- **Not substitutes:** Game Bar visibility/input-redirection, notification
  suitability, fullscreen geometry, process names, DWM cloak and Game Mode/power
  signals do not establish Xbox mode. Automatic pause/recovery remains the
  separate documented-signal blocker in the incident record.

### Game Bar: package registration is detectable; keyboard-enabled state is not

- **Documented API:** [PackageManager.FindPackagesForUser][find-packages] accepts
  an empty SID for the current user and a family filter. Another user's SID
  requires admin; the WinRT reference also lists `packageQuery` capability.
  For this native desktop app, prefer [GetPackagesByPackageFamily][get-packages]
  if querying a known family: current-user scope, Kernel32, sized-buffer pattern,
  no PowerShell process or all-user enumeration needed.
- **Identity caveat:** `Microsoft.XboxGamingOverlay` / family
  `Microsoft.XboxGamingOverlay_8wekyb3d8bbwe` is the observed/community package
  identity, not a stable product-identity contract in those API references.
  [Microsoft-hosted Q&A][gamebar-community] contains the name/removal recipe;
  its answer is explicitly AI-generated, not authoritative API documentation.
  Validate identity/publisher from supported package metadata before any future
  mutation, never by translated display name or a broad `*Xbox*` match.
- A successful matching query supports `registered for this user`; a successful
  empty query supports `no matching family registered for this user`, not
  system-wide absence, a disabled keyboard dispatcher or Xbox mode being off.
  Query failure/unsupported identity becomes `unknown`, not absent. Package
  presence does not establish that Win+G will fire or that capture is enabled.
- [Windows.Gaming.UI.GameBar][gamebar-api] describes overlay visibility and
  input redirection, not Installed/Enabled configuration. No documented Win11
  Win+G-enabled getter or reliable all-off toggle was established.
- **Do not substitute recording values:**
  `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR\AppCaptureEnabled`
  and `HKCU\System\GameConfigStore\GameDVR_Enabled` recipes are undocumented
  for this purpose. Capture/controller-open switches do not prove Win+G disabled.
  The official [AllowGameDVR policy][gamedvr] maps to
  `HKLM\Software\Policies\Microsoft\Windows\GameDVR\AllowGameDVR`, but governs
  recording/broadcasting, is device scope, and explicitly says enforcement is
  Windows 10 desktop only. It is neither an Xbox-mode detector nor a Windows 11
  shortcut-dispatch contract.

### Recommended cheap probe lifecycle (proposal)

- Query the one current-user package family asynchronously at startup, on
  conflict-page open and explicit Recheck; cache the categorical result. No
  polling every owner pump, process/window enumeration, package launching or
  installed-app inventory logging. Keep query work out of the LL callback.
- Keep separate facts: effective enabled chord/takeover, package registration,
  Xbox configuration `unknown`, and any explicit user-confirmed setting. An
  observation error preserves a potential conflict; it never clears amber as
  if absence were proved. Do not persist a package list or deployment paths.
- An honest first version cannot automatically confirm Xbox mode is off.
  Choices: retain `check required` until a documented getter exists, or let
  the user explicitly acknowledge the manual check with a clearly unverified
  state. Recommend retaining `check required`; acknowledgment semantics need
  review, not an inferred green state. Enabled-state automation remains open.

## 3. Resolve actions and safety

- Product direction is to recommend Xbox-mode disable/Game Bar removal for
  Authentic. This does not authorize silent capture loss. Users who need those
  features must retain the choice of Compatible, per-binding rebind/disable or
  takeover off. Game safety wins over achieving authentic shortcut ownership.
- Official APIs do not guarantee that uninstall suppresses Win+G prompts or
  that turning Xbox mode off removes all F11 handling. Physical checks remain
  necessary. Reinstall is not an exact settings/data rollback: Windows removes
  the package's managed [app data][app-data] on uninstall. This is separate
  from user media in libraries; do not claim existing capture files are deleted
  or preserved without checking where that app stores them.

| Conflict / candidate action | Official status | Reversal / undo | Elevation / UAC | Risk, failure and recovery | Button recommendation (proposal) |
| --- | --- | --- | --- | --- | --- |
| Xbox mode: open `ms-settings:gaming-fullscreen`; user turns Enable Xbox mode off, or Choose home app = None on handheld | Exact URI [documented][settings]; controls [documented][xbox-mode] | Re-enable or reselect the previous home app in Settings; user owns the change | Launch needs no elevation; managed controls may be unavailable | Removes console-style experience/entry points; do outside a game. Missing/redirected page: show manual path, leave check unresolved. Launch is not successful remediation | **Offer Open Xbox mode settings**, with instructions; do not label click itself Resolved |
| Xbox mode: write HKCU `GamingHomeApp` empty | Registry API is supported; value semantics only [third-party][xbox-registry], unstable | Exact prior existence/type/data, including alternate home app, would be required; do not restore a hardcoded Xbox AUMID | Ordinary HKCU write normally needs no UAC; custom ACL/policy can deny | Can change gaming home/startup behavior; missing key is ambiguous. Prompt-suppression values merely skip consent, not disable mode | **Do not offer direct write** without a documented contract and separate review |
| Game Bar: open `ms-settings:gaming-gamebar` | Exact URI [documented][settings]; current global Win+G-off behavior not established | User restores changed UI options | Launch needs no elevation | Controller/capture controls may not disable Win+G; never treat recording off as resolved. Missing page: manual path and unresolved status | **Offer Open Game Bar settings** as inspection, not guaranteed fix |
| Game Bar: open `ms-settings:appsfeatures-app?<PFN>` using validated family, fallback `ms-settings:appsfeatures`; user selects Uninstall if Windows offers it | App page + family syntax [documented][settings]; Uninstall availability for Game Bar varies/not established here | Reinstall Xbox Game Bar through Microsoft Store; managed app data is removed, preferences must be recreated | Page launch unelevated; any uninstall consent/elevation remains with Windows | May stop captures/widgets/party or controller integrations; Uninstall may be absent/blocked or app listed under system components. Do not force a bypass. Store access/region/policy may prevent reinstall | **Preferred removal route: Open Game Bar app settings**, with explicit feature-loss and reinstall explanation |
| Game Bar: `PackageManager.RemovePackageAsync(packageFullName)` or `Remove-AppxPackage -Package <exact full name>` for current user only | [API][remove-api] / [cmdlet][remove-command] documented; Game Bar identity/removability and shortcut result not guaranteed | Reinstall from Store; not exact undo. API/cmdlet PreserveApplicationData is development-only, not an ordinary Store-package backup | Medium-IL desktop API caller meets documented requirement; no routine UAC for own-user scope. AllUsers/other-user removal needs admin: excluded | API request cannot be canceled after start and shuts down associated apps automatically; unused dependencies may also be removed. Capture interruption/data loss possible. Report error/policy/nonremovable failure, requery; never retry elevated, remove provisioning or force-close games | **Defer direct Uninstall button** until explicit opt-in UX, package identity, reinstall and capture-safety validation; no Resolve all removal |
| Game Bar: set HKCU `AppCaptureEnabled` / `GameDVR_Enabled` = 0 | Undocumented as Win11 hotkey controls | Would require exact per-value preimages and conditional restore | Normally no UAC; permissions may deny | Silently breaks recording without proven Win+G fix; updates/policy may overwrite | **Reject as shortcut remedy** |
| Game Bar: set device `AllowGameDVR` policy = 0 | Policy [documented][gamedvr], not a supported Win11 dispatch solution | Restore prior configured/unconfigured policy through its managing authority | Device/admin/management authority required | Broad capture/broadcast loss, policy ownership conflict, Win11 enforcement caveat | **Reject**; no policy/service/security bypass |
| Either: rebind/disable the tiler chord or choose Compatible / takeover off | Existing product controls | Keep/Authentic or restore previous tiler settings | None | Preserves Windows gaming features; original physical Win+G/F11 still belongs to Windows | **Retain visible non-destructive alternatives** |

### Registry and uninstall security posture

- Per-user is narrower than machine policy, not automatically safe or reversible.
  No HKLM/policy writes, service stops, ownership/ACL changes, feature-flag tools,
  unsigned scripts, driver workarounds or game injection are proposed.
- If a future documented per-user setting merits a button: fixed allowlisted
  key/value/type, minimum rights, explicit preimage including absence, durable
  ownership/readback, restore only if value still equals our write, and visible
  partial-failure recovery. Do not overwrite a later user/Windows change. This
  is a prerequisite, not approval to implement registry writes.
- A future direct removal action must resolve the exact current-user package
  again before consent, show impact/undo before starting, and honor
  policy/nonremovable refusal without escalation. No wildcards/all-user or
  provisioned-package removal; no deleting WindowsApps. Do not auto-reinstall
  on tiler exit. Store launch only navigates; it must not silently install.
- [RemovalOptions.PreserveApplicationData][removal-options] applies only to
  DevelopmentMode-deployed packages; incompatible Store packages return
  `E_INVALIDARG`. Do not promise this flag as rollback for retail Game Bar.
- Supported recovery navigation: `ms-windows-store://search/?query=Xbox%20Game%20Bar`
  or `ms-windows-store://mylibrary`, per [Store URI docs][store]. User verifies
  Microsoft publisher and reinstalls. Offline/blocked Store means no immediate
  undo; this is why uninstall is not equivalent to a reversible setting toggle.

## 4. Existing warning UX and proposed extension

- Existing implementation:
  [`tray.rs`](../../../crates/tiler-windows/src/tray.rs)::`unresolved_conflicts`
  evaluates active effective chords and takeover, warns on G/F11 incomplete
  containment (plus opted-in Win+L), and supplies the amber icon overlay and
  conflict menu row. The row opens the singleton Settings window.
  `tray_sys.rs` projects the icon/menu/tooltip; `settings_ui.rs` exposes the
  binding conflicts. It currently does not query Xbox configuration or offer
  a dedicated resolve flow. Reuse these surfaces, not a new desktop overlay.
- Proposed Settings detail rows: action + effective chord + distinct conflict
  + evidence state (`Game Bar registered`, `Xbox mode: check required`, or
  `query failed`). Xbox row recommends disable through Settings. Game Bar row
  recommends removal for Authentic while explaining capture/widgets loss and
  the Compatible/rebind alternative. Keep inspection distinct from remediation.
- Recommend per-conflict Open settings / Open app settings and Recheck buttons.
  No automatic Resolve all mutation. If an all-conflicts button is desired,
  call it Review conflicts and open this list; do not silently open several
  external pages or uninstall packages in a batch.
- Rebinding away, disabling or takeover off still removes the relevant active
  chord warning. Clearing an OS conflict must depend on its own evidence, not
  successful navigation. User-approved 2026-10-16 supersedes the proposed
  residual shortcut-check reason: successful package absence clears the whole
  Win+G conflict, with no check note or warning row. Correction B is delivered
  offline, including static binding-list and selected OS text.
  Query errors must not be converted to absence. Xbox enabled-state
  unknown must not masquerade as confirmed enabled or disabled. Exact
  acknowledgment is not selected; query-failure warning handling remains for review.

## 5. Recommended implementation slices and estimate

| Slice | Bounded change / acceptance | Estimate |
| --- | --- | --- |
| Compatible two-chord avoidance | Targeted Ctrl toggle routing/validation, preset overrides, first-run and rebind copy; duplicate/default audit tests; release-order/repeat/modifier/takeover-off tests, no fresh unbound chord capture | Small-medium, 1-2 engineering days |
| Authentic configuration guidance | Current-user async package fact + unknown Xbox state, existing amber/conflict-row integration, exact allowlisted Settings/Store navigation, per-row explanation/Recheck, errors/absent pages covered offline | Medium, 2-3 engineering days |
| Native/user acceptance | Physically check the chords and relevant Settings pages; test missing/blocked package/page cases and confirm ordinary game/capture behavior remains unchanged | About 1 testing day; acceptance pending |
| Optional direct uninstall / reliable Xbox enabled detector | Separate security/recovery validation and supported-contract work | Excluded; 1-2 extra days for uninstall UX/testing if approved, Xbox getter unestimable until a contract is found |

- Total recommended initial slice: **4-6 engineering days**, roughly 8-12
  source/test files plus records; estimate, not a commitment. No hook architecture
  redesign, runtime Xbox recovery, general preset overhaul or OS policy change.
- Verify offline unit/regression tests, locked Windows native tests/build,
  strict clippy/rustfmt, then CI. Navigation/package facts need native acceptance;
  offline tests cannot establish Windows physical dispatch.
- Required physical checks: Ctrl+Win+F11 with Xbox mode enabled (does not enter
  it), then both proposed chords with actual tiler float/fullscreen routing,
  holds/repeats/release ordering and plain Win; Authentic after user-owned
  Xbox disable and optional Game Bar removal (including activation prompts and
  reinstall). Removal/reinstall tests must be separately opted into, outside
  games/capture. See the minimal [discriminators](../../spec/reference-outcomes.md#windows-xbox-shortcut-configuration-discriminators).
- Review choices: approve exact two alternates; choose conservative unknown
  warning versus explicit manual acknowledgment for Xbox configuration; approve
  Settings-only resolve navigation initially versus separately validated direct
  uninstall. Recommendation: candidates above, conservative unknown, Settings
  navigation only. Approved preset direction is not approval of these details.

## Sources

- Microsoft references accessed 2026-10-16; absence claims are bounded to the
  material reviewed. Third-party/community entries are explicitly non-contractual.
- [Windows keyboard shortcuts][shortcuts]: Windows 11 Windows-key, taskbar,
  desktop and virtual-desktop tables.
- [KB5070297][xbox-mode]: Xbox-mode setup, supported versions/markets and Win+F11.
- [Settings URI reference][settings]: Gaming and Apps tables; page-availability
  caveat. Reviewed page source revision `b49af5514951dc5e0f332ee988e361a92e0469e2`.
- [Package query][find-packages], [native package query][get-packages],
  [GameBar API][gamebar-api], [OEM GamingHomeApp][unattend].
- [RemovePackageAsync][remove-api], [Remove-AppxPackage][remove-command],
  [RemovalOptions][removal-options], [app-data lifetime][app-data],
  [AllowGameDVR][gamedvr], [Store URIs][store], [Win-key reservation][register-hotkey].
- [ElevenForum Xbox registry recipe][xbox-registry] and
  [Microsoft-hosted Q&A removal recipe][gamebar-community]: undocumented/unstable,
  not Microsoft product contracts (the latter answer is AI-generated).
- Related prior art: [reference-WM hook comparison](reference-wm-comparison.md#4-input-win-key-game-bar--xbox-parked),
  [AutoHotkey lessons](autohotkey-learnings.md),
  [Windows port shortcut research](plan.md). None establishes G/F11 containment.

[shortcuts]: https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
[xbox-mode]: https://support.microsoft.com/en-us/topic/windows-gaming-full-screen-experience-67fb8d12-5467-4a95-8adf-0a10789576ab
[settings]: https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings
[find-packages]: https://learn.microsoft.com/en-us/uwp/api/windows.management.deployment.packagemanager.findpackagesforuser
[get-packages]: https://learn.microsoft.com/en-us/windows/win32/api/appmodel/nf-appmodel-getpackagesbypackagefamily
[gamebar-api]: https://learn.microsoft.com/en-us/uwp/api/windows.gaming.ui.gamebar
[unattend]: https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-gaming-configuration-gaminghomeapp
[remove-api]: https://learn.microsoft.com/en-us/uwp/api/windows.management.deployment.packagemanager.removepackageasync
[remove-command]: https://learn.microsoft.com/en-us/powershell/module/appx/remove-appxpackage
[removal-options]: https://learn.microsoft.com/en-us/uwp/api/windows.management.deployment.removaloptions
[app-data]: https://learn.microsoft.com/en-us/windows/apps/develop/data/store-and-retrieve-app-data
[gamedvr]: https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-applicationmanagement#allowgamedvr
[store]: https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-store-app
[register-hotkey]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey
[xbox-registry]: https://www.elevenforum.com/t/enable-or-disable-xbox-mode-in-windows-11.42303/
[gamebar-community]: https://learn.microsoft.com/en-us/answers/questions/5672600/how-uninstall-windows-game-bar
