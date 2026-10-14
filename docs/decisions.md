# Current Decisions

Active, user-approved choices are recorded here. Explicitly flagged tentative
clauses are pending user review, not user-approved current rules.
Historical implementation detail is recoverable in Git history.

## Development Entry Point

- User decision 2026-10-13: `just dev [verbose|trace]` is the single
  foreground full-solution dogfooding entry point: tiling, hooks, effects or
  overlays, and tray, with terminal log streaming and verified Ctrl+C teardown.
- The root `justfile` selects only the current OS recipes using conditional
  imports and requires just 1.56.0 or newer. Linux selects the desktop from
  case-insensitive, colon-separated `XDG_CURRENT_DESKTOP`: KDE uses `dev-kde`,
  GNOME has an explicit unsupported stub, other desktops fail clearly.
- Windows uses normal `tile --user-start` through the existing Explorer
  broker, readiness and provenance path. `trace` adds tracing; `verbose`
  retains default logging. Additional arguments pass through to tiling.
  The prior non-tiling ten-minute Windows loop is `lifecycle-smoke`.
- Unsupported platforms fail explicitly rather than starting a partial loop.
  [Implementation and acceptance](changes/dev-entry-point.md).
- User 2026-10-14: Windows `just dev` user-accepted (including tray Stop and
  whole-desktop use); further issues become new backlog items.
- User 2026-10-14: agent-owned identity-tagged `tiler-test-window.exe` is the
  standard disposable helper for Windows agent live tests; each live test
  still needs a user desktop hand-over.
- User 2026-10-14 approved as delivered: `just/` platform-file layout, macOS
  `dev` stub, retained per-run Windows payload dirs with current pointer, and
  KDE-wins mixed desktop lists.

## Scope and Platform Goals

User decisions 2026-09-30. These select goals and sequencing, not untested
platform API behavior.

- Support Windows 11 x64 only initially; this can be revisited.
- Managed per-monitor workspaces are required before Windows or macOS is
  called feature-complete. Tiling-only development previews are allowed.
- Windows distribution should be the most obvious and unsurprising for users.
  Store availability alongside manual installation is research scope, not a
  selected package or update channel.
- Develop directly on native Windows 11, not a VM. Multi-monitor live work
  needs a multi-monitor Windows setup. Keep
  the NixOS/Linux flow intact.
- Sandbox closed 2026-09-30: Phase 1-3 live proof runs in a native
  Windows session with owned disposable windows first; Sandbox deferred to Phase 4.
  Route detail stays in [Windows plan](research/windows-port/plan.md).
- Elevated (administrator) windows stay unmanaged by default: they float at
  their native position while other windows keep tiling. A future opt-in
  where the user runs the tiler elevated may be considered; not a default now.
- macOS App Store, distribution, updates, and native-settings takeover choices
  remain open; the Phase 0 decisions below settle the initial architecture.
- macOS starts now (user 2026-10-14), superseding the 2026-10-10 sequencing
  after KDE 0.1 ships. K2/K3 core extraction and the shared restart store are now due for
  consumer-driven review, not automatic extraction. See the
  [kickoff evidence](changes/archive/macos-port-kickoff.md). Phase 0 carries
  capability-first spikes and one registration plus one interception hotkey
  path from the non-native research
  ([API layer/events](research/non-native-tiler-host-interaction.md#d1-api-layer-authority-events-and-reconcile),
  [hotkeys/consent](research/non-native-tiler-host-interaction.md#d2-hotkeys-consent-conflicts-secure-input-taps-focus),
  [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [evidence](changes/archive/non-native-tiler-host-interaction.md)).
- User direction 2026-10-03 (macOS approach): prefer the lower-level,
  lower-jank route first (yabai-style). An optional higher-level public-API
  route (AeroSpace-style) may be evaluated later. Default to tier 2, public
  plus private APIs with SIP left enabled; no Dock injection or reduced SIP.
  Re-evaluate deeper tiers only if tier 2 cannot solve a problem well.
- macOS Phase 0 decisions (2026-10-14):
  - **All Rust**, including UI. Dependency selection and any unsafe exception
    follow [Unsafe Code](principles.md#unsafe-code); implementation enforcement
    is recorded in the [Rust unsafe guardrail](#rust-unsafe-guardrail).
  - **Threading from day one:** main thread AppKit/UI only; dedicated input
    thread with minimal event-tap callbacks; one Accessibility thread per
    application; one engine thread owning core state; channels between them.
    A hung application's AX calls must not stall other apps. AX messaging
    timeouts and tap-disabled-by-timeout recovery are design inputs, not
    later optimizations.
  - **Native macOS Spaces first**, with yabai the primary reference and a
    strong general reference. An AeroSpace-style virtual/parking mode may
    later become a user setting alongside native Spaces; it is not initial
    implementation scope.
  - **Ctrl is default Meta**, and macOS Meta must be user-configurable.
    Retain the per-binding conflict list and Compatible/Authentic presets;
    Mission Control's Ctrl+arrow and Ctrl+digit bindings are expected conflicts.
  - **Apple Silicon only; macOS 15 minimum.** The floor uses the oldest
    version clearing approximately 90% in the usable current public sample:
    [Homebrew 30-day OS events](https://formulae.brew.sh/analytics/os-version/30d/),
    fetched 2026-10-14, period starting 2026-09-14: 12,461,979 events on
    macOS 15 or newer / 13,361,038 macOS events = **93.27%**; newer-only
    coverage is 71.72%. This is an opt-in developer/event-weighted proxy,
    not a measured share of all active Mac users (nor an arm64-only panel).
    [StatCounter](https://gs.statcounter.com/os-version-market-share/macos/desktop/worldwide)
    explicitly warns that modern releases are misreported as Catalina, so
    its September 2026 version shares cannot resolve this choice. No material
    API-risk evidence currently requires a newer floor: AX/Carbon/tap APIs
    predate it and yabai's private-Space workarounds cover 15. Private APIs
    still need per-release testing; a newer development OS does not validate
    the floor. Revisit the estimate with better population data.
  - **Persistent self-signed development identity initially**, with stable
    bundle ID/path/designated requirement. No paid Apple Developer membership
    initially; release signing/notarization remains a separate decision.
    The development recipe requires one exact-name code-signing identity and
    signs by its SHA-1, regardless of certificate trust; see the
    [setup procedure](macos-dev-environment.md#persistent-self-signed-development-identity).
  - **Owned probe helpers use simple child ownership:** retain the exact
    spawned Child, verify its PID, per-run pipe nonce, executable and fresh
    CG owner-PID/window association. No LaunchServices launch-date token or
    alternative process-start machinery. Separate commands additionally
    require a current nonce-title match; unreadable titles fail closed.
  - Live procedures are governed by the
    [macOS live-testing guide](live-macos-testing.md); capability and recovery
    evidence is tracked in [Phase 0](changes/macos-phase-0.md).
- User decisions 2026-10-10 (0.1 release scope and gate):
  - Release 0.1 is the existing MVP on KDE Plasma only. Windows continues
    as a development preview and ships in its own later release, requiring
    managed per-monitor workspaces (2026-09-30).
  - Must-pass gate: the VISION reliability list (sleep/wake, plugging/
    unplugging outputs, resolution and scaling changes, fullscreen
    applications, underlying configuration changes); gaming/fullscreen
    non-interference; core journeys (tiling, focus/move, workspaces,
    multi-output, border, shortcuts, settings, restart); missing, failed or
    removed effect leaves tiling working and KWin stable. Other live checks
    may ship as listed known issues provided none crashes or silently stops
    tiling. No separate dogfood period.
  - Classification of pending live checks into must-pass versus
    known-issue-allowed was a separate follow-up piece, approved 2026-10-12 below.
  - The original 60 OPEN requirements get a separate triage into 0.1-relevant
    (touches a must-pass core journey, or current behavior is undefined,
    refusal or surprising) versus post-0.1. Only the 0.1-relevant set is
    decided before 0.1 with the user.
  - Install paths: Nix flake/Home Manager, GitHub Release and OBS distro
    packages. KDE Store is excluded from 0.1, replacing the earlier KDE
    Store plus GitHub Release plan. External NixOS/Home Manager delivery
    validation stays P1 in the gate; mainstream packaging becomes P1.
  - OBS builds release tags only into one stable repository. Targets:
    openSUSE Tumbleweed, Fedora 43/44 with updates repos, and Arch via an AUR
    PKGBUILD (not an OBS pacman repo). Ubuntu 26.04/26.10 support was selected
    on 2026-10-11 under [KDE Support Floor](#kde-support-floor). Attempt KDE
    neon; return to the user for a decision if too challenging.
- Windows package/update channel (user 2026-10-10): deferred to Windows
  release planning, then decided from a short research refresh under the
  existing most-obvious-and-unsurprising criterion.
- User approval 2026-10-12: all 28 triage units and Proposal B / N1-N4
  classification are decided; triage review complete. Original 53 checks:
  39 must-pass / 12 known-issue-allowed / 2 Windows-release. K checks:
  5, 24, 26, 29, 35, 36, 37, 38, 40, 41, 52, 53; W checks: 3, 4; all others M.
  N1-N4 add four must-pass checks: total 57 = 43 / 12 / 2.
  [Approved inventory](research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2).
  D28 native fullscreen send including a real game is must-pass within check 1,
  counted once. Multi-output must-pass checks require a two-output setup
  (VM or hardware) and must pass before 0.1 ships. Known-issue-allowed checks
  may ship only with a listed issue and no crash or silent tiling stop;
  Windows-release checks remain outside KDE 0.1.
  Batch 1 and the thirteen units
  [approved 2026-10-12](research/release-0.1-triage.md#user-decisions-2026-10-12)
  are NORMATIVE;
  native evidence and implementation gaps remain separate.
- User direction 2026-10-13: promote only a user-operated two-output KDE
  environment for the 0.1 multi-output must-pass checks from the parked
  live-test reference-environments proposal to pre-0.1. General reference
  environments remain parked. [Route research](research/two-output-test-environment.md)
  recommended a full Plasma VM. User selection 2026-10-13: no VM; multi-output
  development and must-pass testing run natively on a user-owned PC with a
  physical second monitor, which also covers physical hotplug and sleep/wake.
  Check 2 still needs a third output. The approved must-pass gate is unchanged.

## Windows Owner Threading

- User selection 2026-10-12: target C (dedicated input and render pumps),
  delivering complete A first (stage 1), then B early (stage 2). Selected for
  latency only, with no Win+G/Win+F11 containment claim.
- Stage 1 implementation, 2026-10-12: both low-level hooks and synchronous
  consume/pass classifiers share one dedicated input pump. One owner retains
  observation, Engine, foreign/foreground actuation, rendering, services,
  Mouse Snap SPI and the existing recovery ledger/watcher.
- Owner-built immutable whole snapshots cross a bounded ordered handoff.
  Callbacks never wait for owner replies or locks; a publication/application
  atomic fence refuses fresh stale-policy consumption while retaining stored
  press verdicts, consumed pairs/repeats and Win-up mask obligations. One
  retained retry generation covers refused policy publication and reversion.
- Fixed safety slots preserve invalidation/clear obligations under saturation.
  Distinct-invalidation overflow conservatively clears all origins/armed
  windrag state, preserving swallowed Up/Cancel obligations; the owner forces
  a fresh whole snapshot behind that clear. Controls invalidate retained
  origins, and every successful installation reapplies the retained snapshot.
- Bounded tagged intent batches preserve classifier queue/terminal guarantees;
  monotonic atomic Esc sequences keep START/END ordering independent of owner
  drain timing. Logs stay on owner, with bounded input timing diagnostics and
  explicit limits on what they measure.
- Accepted keyboard/mouse intent batches signal one auto-reset owner event;
  the outer owner wait watches it alongside messages, retaining the periodic
  fallback. Diagnostics alone do not wake the owner. Focus-settle/proof waits
  remain message-only: pending intents do not trigger recursive dispatch or
  spin during foreground settlement. The event has shared owned-handle lifetime;
  creation failure refuses input-pump startup before hook installation.
- Creator-thread install/retry/unhook, readiness and bounded stop acknowledgement
  are part of stage 1. Missing/unclean stop retains a process-level pump
  reservation; no overlapping replacement pump or new recovery authority.
- Offline delivery and physical-check limits:
  [stage 1 record](changes/archive/windows-input-thread.md);
  [owner-wake correction](changes/archive/windows-input-owner-wake.md);
  [thread contracts and stage 2 design](research/windows-port/owner-threading.md).
- Stage 2 implementation, 2026-10-13: one dedicated renderer creates, pumps,
  paints, places and destroys border, group underlay and drop-preview carriers.
  Owner retains observation, Engine, foreign actuation, services and recovery;
  input classifiers remain on their dedicated pump. Existing owned proof paths
  keep same-thread carriers.
- Three bounded latest-wins frame slots carry desired geometry and lifetime
  facts; retained safety watermarks invalidate old Shows. Renderer rechecks
  fresh native identity/foreground/visibility/band/fullscreen facts and reports
  actual applied pixels/proof or unknown state, never desired-as-rendered.
  Versioned immutable overlay-ID snapshots exclude carriers from observation
  and WinEvents. Tagged identities prove window lifetime; untagged targets
  retain the existing process-fenced native race, reported as unproven.
- Renderer work and owner acknowledgement events have safe owned-handle
  lifetime on both threads. Acknowledgement-only owner turns service applied
  outcomes/pending finish without reposting frames; input events retain their
  immediate outer-wait wake. Logging stays on owner, with separate render queue
  age and present spans. Readiness requires hidden carrier creation; clean stop
  requires creator-side native destruction and successful thread exit. Missing
  completion prevents overlapping replacement, using existing process recovery.
- Preview finish is a bounded pending owner operation: creator-verified hide
  acknowledgement precedes fresh owner revalidation and actuation. User
  2026-10-14 approved as delivered: no hide proof within 500 ms refuses that
  finish without drop/snap-back writes; an overlapping pending finish refuses
  narrowly.
- Stage 2 native offline and scoped agent live evidence, with physical checks
  still pending: [render-thread record](changes/archive/windows-render-thread.md).
- Stage 3 single-output hardening, 2026-10-13: input/render idle waits are
  event/deadline-driven, with no fixed exchange quantum. Owner retains its
  existing 100 ms maintenance cadence. Input transfers callback work before
  blocking and after return; transient transport contention gets a bounded
  retry, while occupied outboxes resume on owner drain notification.
- Input clean stop requires successful hook release and successful finished
  thread join; repeated terminal waits return the cached result. Missing or
  unclean completion keeps the process reservation. Render stop signals its
  own work event. Native wait-return diagnostics stay bounded and trace-only,
  with no diagnostic-induced wake or new timer.
- Input/owner timing and routine render-frame diagnostics aggregate observed
  counts and maxima on existing owner turns, at most once per second per
  stream/carrier. First input/owner latency outliers log immediately; repeats
  are throttled independently of summary resets. Safety/proof/visibility
  transitions, decisions, window actuation and failures retain immediate
  evidence. Applied render samples distinguish measured acknowledgement
  timings from unmeasured same-thread fallback timings. See the
  [trace-volume record](changes/archive/windows-trace-volume.md).
- Combined offline and scoped automated desktop acceptance is recorded in
  [Stage 3 evidence](changes/archive/windows-owner-threading-acceptance.md). Physical
  visual/input checks, multi-output/mixed-DPI and gaming cost remain pending;
  a bounded trace-on idle CPU result is not gaming-cost acceptance.

- User direction 2026-10-13, post-0.1 KDE interaction performance/threading:
  Windows input/render threads drastically improved performance and UX. KDE
  shows no significant jank, so this is not a current priority. For KDE and
  later platforms, explore moving dynamic window resizing during drag and
  drag/move/resize interaction work into multithreaded Rust, with C++ and
  TypeScript as thin shims, relying on Rust thread safety. Research KWin
  effect/script threading constraints first; the result may change how
  OmniTiler interacts with KWin. Link with
  [Push code down](principles.md#layering) and Live sibling reflow while dragging
  in [backlog](backlog.md).

### Rust unsafe guardrail

- Policy and preference order live in [Unsafe Code](principles.md#unsafe-code)
  (2026-10-14), superseding the earlier ABI/FFI-only wording. Selected
  enforcement: workspace
  `[workspace.lints.rust] unsafe_code = "forbid"`; FFI crates override to
  `deny` and opt in with `#![allow(unsafe_code)]` only on the approved module;
  CI rejects any other `allow(unsafe_code)`. `windows`/`windows-sys` choice
  and verification that they keep Win32 calls `unsafe fn` are pending research
  ([backlog](backlog.md)).
- `tiler-core`, `tiler-protocol` and existing unsafe-free Windows modules
  already forbid unsafe. Linux planner (`crates/omnitiler`) delivered
  2026-10-13: tests use explicit trace values instead of process-env mutation;
  all planner roots forbid unsafe. The core/protocol test and example roots
  now also forbid unsafe; no gaps remain outside the two FFI crates.
  [Evidence](changes/archive/linux-planner-unsafe-guardrail.md).
  Windows must consolidate FFI into dedicated modules after its current
  release-gate work (timing tentative). KWin effect FFI remains a minimal
  boundary module.
- Stage 2 cross-thread state uses safe standard primitives and owned handles;
  no unsafe Send/Sync, mutable statics, UnsafeCell or cross-thread pointer
  dereferencing. Existing scattered Win32 FFI call sites are an implementation
  gap under the new rule. [Delivery evidence](changes/archive/windows-render-thread.md).

## Windows Multi-Output Foundation

- User 2026-10-14 approved as delivered: cross-output keyboard moves and
  native/Win-drag releases plan on a candidate Engine. Retained Engine state
  and workspace membership publish only after fresh topology, geometry
  readback and arrival verification; empty-destination drops also require
  source reflow verification. Refusal retains original logical membership.
  Native writes are not atomic: partial physical changes converge through
  subsequent observation, without replaying the refused transfer.
- User 2026-10-14 approved as delivered: a verified transfer stands if subsequent
  foreground actuation fails; report the focus failure without claiming
  successful follow-focus. No workspace switch or visibility hide accompanies
  these transfers.
- Existing shared selection and pointer-placement outcomes remain authoritative;
  domain-scoped hover state and occupied two-domain drag support add no planner
  wire fields. Physical multi-output acceptance remains pending in the
  [foundation record](changes/windows-multi-output-foundation.md).
- User 2026-10-14 approved as delivered: items 14/19 migration (nonactivating
  move-only fullscreen/float remaps; maximized/minimized keep restore rectangle
  and show state across outputs).

## Project Name

- User decision 2026-10-11: the project is **OmniTiler**, reflecting KDE,
  Windows and later macOS targets.
- Naming: display and upper CamelCase `OmniTiler`; lower CamelCase `omniTiler`;
  lowercase IDs, packages, binaries, filenames, snake identifiers, KWin/KPackage
  IDs, systemd units, Nix attributes/options and shortcut prefixes `omnitiler`;
  environment variables/macros `OMNITILER_`; reverse-DNS IDs `com.omnitiler.*`
  and object paths `/com/omnitiler/...`. Preserve functional suffixes. Generic
  internal crate names remain unchanged. No host-format exceptions are needed.
- GitHub repository: `github.com/beefsack/OmniTiler`. OBS/AUR package names:
  `omnitiler` and `omnitiler-native-effect`.
- User decision 2026-10-11: the registered domain `omnitiler.com` belongs to the
  project and backs its `com.omnitiler.*` reverse-DNS IDs. It is the future
  project home page, not yet live; package homepage fields retain the GitHub URL.
- Pre-release rename: no migrations or compatibility shims; old installed state
  stops being used.

## Licensing

- User decision 2026-10-11: project default is MIT, copyright Michael
  Alexander. All Rust crates, including the staticlib linked into the native
  effect, KWin TypeScript script, Settings KCMs, tray/planner, scripts,
  packaging and documentation are MIT.
- Exception: the native KWin effect implementation is GPL-2.0-or-later,
  matching KWin's effect API header grant. Exact sources under
  `kwin/native-effect`: `activewindowborder.cpp`, `activewindowborder.h`,
  `activeborderlogic.h`, `oraclepress.h`; its `metadata.json` declares the
  same license. Shared FFI/config inputs are MIT, not part of the exception.
- MIT source may link into a GPL-covered binary without changing its own
  source license. GPL effect source is chosen to make the boundary explicit
  and permit KWin code reuse; it is not a source-license requirement imposed
  by linking. Root LICENSE covers MIT sources; GPL sources carry SPDX
  license/copyright headers. Full texts are in `LICENSES/`.
- Metadata and first-party notice delivery are applied. Dependency audit and
  the CI gate are delivered ([evidence](changes/archive/licensing-audit-gate.md)).
- User decision 2026-10-13: keep the CI gate with `licenses`, `bans` and
  `sources`. Before 0.1, add tool-generated third-party notices, including
  license texts for MIT/Apache crates compiled into binaries, to GitHub Release
  binaries and Debian/RPM/Arch packages. This reverses the earlier tentative
  no-notice choice. Tool: `cargo-about` 0.9.0, already declared in `devenv.nix`.
- User decision 2026-10-13 (delivery): option B, one committed generated
  `THIRD-PARTY-NOTICES` union covering the shipping `omnitiler` executable and
  `tiler-kwin-effect-ffi` staticlib for supported Linux targets. Include full
  selected-license texts, copyrights and applicable upstream NOTICE content.
  Developers regenerate manually with `just third-party-notices`; CI fetches
  locked sources, regenerates into a temporary location and fails on differences,
  without updating or committing the tracked notice.
- Both distro packages carry the same union file: Debian under
  `/usr/share/doc/<pkg>/`, RPM via `%license`, Arch under
  `/usr/share/licenses/<pkg>/`; Rust-bearing Nix outputs use
  `$out/share/licenses/<pkg>/`. The release source archive carries the file at
  its root. No GitHub binary-archive producer is part of this change; its future
  notice delivery remains required before distributing those binaries.
- User decision 2026-10-13 (text encoding): generated files are exempt from
  non-ASCII restrictions. `THIRD-PARTY-NOTICES` preserves upstream bytes
  verbatim; authored files remain ASCII.

## Development Environment

- User decision 2026-10-03 (dev environments): single root `mise.toml` for
  Windows and macOS toolchains, run from the project root; OS-specific entries
  stay in that root file. devenv/Nix stays the source on Linux/NixOS, including
  system libraries. Minimal `AGENTS.md` changes needed to allow this are
  approved; avoid bloat.
- User decision 2026-10-10 (Windows shell and toolchain): install PS7 via
  `winget install --exact --id Microsoft.PowerShell --source winget`, not the
  Microsoft Store; use VS 2026 Build Tools or another 2026 edition with the Desktop C++
  workload, recommended x64/x86 MSVC tools and Windows SDK, via Visual
  Studio Installer (no confirmed 2026 winget ID). The VS 2026 route was
  validated by the native offline gates on 2026-10-10; live-testing and
  clean-runtime acceptance remain separate.
- Implemented dev route (2026-10-03): root `mise.toml` declares stable Rust
  via rustup with rustfmt/clippy and just/jq/gh/ripgrep on Windows/macOS;
  yq is Windows-only. Git, MSVC/SDK, Xcode/CLT and host shell bootstrap
  remain manual; installations remain user-owned. Rust resolves through
  process-local `RUSTUP_TOOLCHAIN`, without a persisted directory override,
  toolchain file, or `rust-toolchain.toml`. Linux Rust/CLIs follow the
  `devenv.yaml` nixpkgs pin. Operational commands live in
  [AGENTS.md](../AGENTS.md).
- User decision 2026-10-08 (dev tool versions): CLI selectors are `latest`,
  Rust is `stable`, and no `mise.lock` is committed. Pre-1.0 track latest
  stable Rust and fix breakage; revisit the upgrade process at 1.0.
- User decision 2026-10-08 (dev environment CI): verify mise installs
  and tool/host/component smoke checks on Windows and macOS 15 arm64, retaining
  Windows Cargo gates. No exact mise/Nix equality gate.
- Reference implementations (2026-10-03) remain inputs to the functional
  spec; upstream repositories and pinned revisions are listed in the
  [reference matrix](spec/reference-outcomes.md). cosmic-comp (n-ary splits)
  is the user's favourite tiling UX.
- User rule 2026-10-11: never commit local paths or information about local
  machines (paths, host names, hardware/resource details); describe test
  conditions generically. [Rule](../AGENTS.md#local-system-information).

## Settings, Tray and First-Run

- User-approved 2026-10-11 (KDE package ownership): distro core package
  `omnitiler` ships both KWin-independent Settings KCMs, providing
  tiling plus Settings/Apply/Revert without the optional effect.
  `omnitiler-native-effect` carries only the KWin-ABI-bound effect.
  Core means the distro package; no KDE code goes into `crates/tiler-core`.
  Nix exports independent `native-settings` and effect-only `native-effect`;
  the NixOS module installs both alongside the script. Removing the effect
  leaves Settings/Revert available; recovery after removing core requires
  reinstalling core. [Delivery](changes/archive/release-0.1-core-settings.md).
- User decision 2026-10-12: P2 `0.1 triage settings follow-up` contains
  approved post-0.1 D09, D10 and D27 settings, with names/options/WM tooltips
  under [Move commands](#move-layout-and-output-commands) and
  [Activation requests](#activation-requests).
  D03, D12, D13, D16, D22, D23, D25 and D28 have no setting.
- Shared:
  - Apply/Force/Revert is the only correction flow; ordinary Save never
    mutates shortcuts or host keys, and installation/startup never mutates
    global shortcuts.
  - Proof owners stay isolated from normal configuration.
- KDE: one unified Settings page (user 2026-09-28 option B) for border,
  shortcut overrides, gaps and `workspaceMode`; tray Settings, KWin Scripts
  Configure and Desktop Effects Configure open the same page. Saving changed
  gaps requests KWin reconfigure and the controller re-reads validated gaps;
  the request alone does not confirm application. `workspaceMode` is
  startup-only with a session-restart note. Removed
  `tilingAlgorithm`/`automaticSplitTarget`/`dropOutlinePreview` values are
  neither read nor rewritten. `shortcutProfile` stays hidden until distinct
  profiles exist; its saved value and startup read remain untouched.
  Launch blocker: every user-facing setting must apply live, except
  startup-only `workspaceMode`.
- KDE shortcut dialogs (user 2026-10-12): explicit Apply Shortcuts applies
  immediately; conflicts open one modal, scrollable preview with Force Apply
  and Cancel. Apply/Force/Revert applies immediately, with the result shown in a
  short dialog. User live review 2026-10-12: success text is "Shortcuts applied."
  for Apply/Force and "Shortcuts reverted to defaults." for Revert; result
  dialogs give no instructions about overall Settings Apply/OK. Errors show a
  single-line reason, capped at 240 characters with `...` for truncation;
  full diagnostics remain in the journal. Tentative, pending user review: no preliminary
  Apply confirmation and no second Force confirmation; the conflict dialog is
  the single Force confirmation. Revert retains its custom-binding-loss
  confirmation. [Flow](#shortcuts-conflicts-and-presets),
  [delivery](changes/archive/kde-shortcut-result-dialogs.md).
- KDE tray carrier: portable Rust StatusNotifierItem, KWin backend first;
  stays alive without a watcher, registers on live-confirmed watcher owner;
  outbound state-snapshot bridge, reconnecting, idempotent, no KWin executable
  allowlist; no shell, input, or general helper-to-KWin action route (narrow
  exceptions: action-name KGlobalAccel workspace-tiling toggle and
  saved-default/reconfigure writes). Snapshots require the sender's unique
  D-Bus name to equal the current `org.kde.KWin` owner. Ordering (user
  decision C4 option 2, 2026-09-27): reject and
  log same-generation lower-revision snapshots; equal-revision heartbeat may
  refresh; equal-revision different-content revokes trust. Status (user
  decision 2026-09-28, option 2, for now): fresh
  authenticated snapshot shows Active, missing/stale shows NeedsAttention.
  Host-conflict warning (`OverlayIconName=dialog-warning`, top menu row
  opening Settings) is separate from tiling status. Delivery (user decision
  2026-09-28, option 2): Home Manager
  systemd user unit (`Restart=on-failure`); no supervisor; second instance
  exits successfully on taken name. Diagnostics on stderr/journal with
  `route-diag component=tray-endpoint`.
- KDE host-setting conflicts (user 2026-09-29; Fix confirmed live 2026-09-29,
  Revert/tray-indicator acceptance pending): the unified page reads
  `kwinrc [Windows]` `ElectricBorderTiling`, `ElectricBorderMaximize` and
  `ElectricBorders` on open with short explanations. Boolean rows always
  visible: Fix writes `false` when on; Revert removes the local key when off
  so the KDE 6.7.5 default `true` takes effect. The `ElectricBorders` row
  appears only when nonzero; its sole Fix removes the local key so default
  `0` takes effect. No prior-value journal or ownership tracking. Explicit
  KConfig changes send KWin reconfigure and read back effective config; a
  failed write or send is shown/logged, and a queued send does not prove the
  running compositor applied the value. Startup and ordinary Save never change
  host keys. Host conflicts add a warning overlay to the tray icon and a top
  menu row opening Settings; left-click keeps opening the tray menu, snapshot
  loss keeps its separate NeedsAttention status; no notification or direct
  Settings-on-icon-click. For 0.1, document pressing Revert in Settings before
  removing in install/uninstall docs, package descriptions and a post-removal
  package note. Automatic revert remains parked research after 0.1
  ("uninstall leaves no changed KDE settings", user 2026-10-10).
- Windows (user 2026-10-08):
  - Validated version-1 JSON settings in
    `%LOCALAPPDATA%\omnitiler\settings.json`; normal owners read at
    startup and poll on the existing pump; explicit normal CLI switches stay
    authoritative per field; malformed live files keep last-good state.
  - Apply validates and atomically saves; Revert discards unsaved edits and
    reloads the saved file; Close never saves. Mouse prevention uses existing
    session-only preimage/readback/conditional restoration, no policy.
  - Plain official Win32 controls via `windows-sys`; `tiler-windows settings`
    plus tray; single UI instance per user/session; external stale edits
    trigger reload/refusal.
  - The normal running owner owns one official `Shell_NotifyIconW` icon; both
    clicks open its menu (conflict row, status, workspace tiling,
    new-workspace default, Settings, Stop). One stable icon GUID; TaskbarCreated
    revalidates or re-adds; proof owners create no tray or prompt. The
    taskbar workspace indicator (parity 10) and hidden-workspace Alt+Tab/taskbar
    semantics share one Windows workspace-presentation research piece
    after multi-output, before release, producing options and a recommendation
    for the user (2026-10-10). Existing
    [Alt+Tab research](research/windows-port/alt-tab-hidden-workspaces.md) and
    non-native lessons remain inputs
    ([workspaces/Alt+Tab](research/non-native-tiler-host-interaction.md#workspaces-monitors-overview-and-alttab-dim-4),
    [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
    [evidence](changes/archive/non-native-tiler-host-interaction.md)).
    Settings and the conflict row open the singleton UI; Stop uses ordinary
    teardown with graceful icon deletion and dead-owner cleanup through the
    existing recovery lease. Explorer may initially put the icon in overflow.
  - User 2026-10-14 approved as delivered: tray menu Stop uses a 50 ms
    menu-thread check and `EndMenu` (`f4b9f41`).
  - An amber warning overlay and conflict row identify enabled effective
    Win+G/F11 chords with known incomplete containment, plus kept Win+L when
    runtime opt-in allows its unreliable lock override. Disable, rebind-away,
    Compatible or keyboard takeover off clears the warning; absence of a
    warning never establishes containment for other unproven chords.
  - The own-executable Settings control window is unmanaged through existing
    dialog gates.
  - Status: synthetic native UI/Apply/Revert, geometry/border/SPI and cleanup
    proof passed; physical shortcut/Snap/Xbox and other DPI/output checks
    stay user-owned ([settings](changes/archive/windows-settings.md)).
  - Status: synthetic/native live proof passed for both presets, stale-choice
    refusal, menu buttons, warning changes, Stop, TaskbarCreated and crash
    recovery; real Explorer restart, physical input and other DPI/output
    arrangements remain user-owned
    ([tray/first-run](changes/archive/windows-tray-first-run.md)).

## Workspaces

- User decision 2026-10-12, D16 / REQ-WS-05 / R-WS-05, option (a): sending
  a floating window to another workspace keeps it floating on arrival and
  keeps its size; destination tiles are untouched. Applies to follow and stay,
  numbered and relative sends. No setting.
- D16 [tier rule](#reference-tiers) basis: Hyprland, i3 and sway (Tier 1)
  plus xmonad, qtile and awesome (Tier 2) retain float against COSMIC fresh
  re-tile. COSMIC's alternative has no second Tier 1/2 reference, so it does
  not qualify as a setting.
- User confirmation 2026-10-13, D16 arrival frame: same-output sends keep the
  exact frame, with no geometry write; this is no longer tentative.
- User decision 2026-10-13, D16 combo sends: explicit sends of
  floating+fullscreen and floating+maximized windows carry them exactly as-is,
  still floating and still fullscreen/maximized on arrival. Later leaving
  fullscreen/maximize returns to the floating frame. Sticky floats keep
  refusing. Shared-core/KDE delivery now uses the slotless deferred path.
- Ordinary and combo intentional-float sends are delivered offline
  ([record](changes/archive/d16-floating-transfer.md)). Native WS1 -> WS2 -> WS1
  roundtrip and destination overlay-exit acceptance remain pending. Shared
  delivery serves Windows; Windows adapter delivered offline 2026-10-13
  ([record](changes/archive/windows-d16-floating-transfer.md)); native checks
  remain in handoff item 22.
  Floating-workspace boundary sends remain a distinct approved journey.
  [Approval](research/release-0.1-triage.md#user-decisions-2026-10-12).
- R-CLOSE-03: closing the sole window retains the shown workspace empty, with no focused client (User 2026-10-10, 0.1 triage D08).
- R-WS-09: workspace return restores remembered focus (User 2026-10-10, 0.1 triage D17).
  Windows dogfood fix delivered offline 2026-10-11: explicit numbered/history/
  CLI departure records verified live foreground, including fullscreen members;
  return restores it without fullscreen geometry writes. Native game check
  pending ([record](changes/archive/windows-workspace-fullscreen-focus.md)).
- R-WS-02: returning B inserts after remembered target anchor A; no before/after setting (User 2026-10-10, 0.1 triage D18).
- Shared model:
  - Each managed workspace owns a session-local tiled/floating flag. Startup
    seeds all workspaces from the saved default (`true`); live default changes
    seed only subsequently created workspaces. Overrides reset on owner
    restart and never persist. COSMIC matches this for unpinned workspaces;
    it persists the tiling flag only for pinned workspaces, which we do not
    support (G-07, User 2026-10-10). Pinned/persistent workspaces are Future
    scope, grouped with the shared restart store. No keyboard binding for
    the default itself.
  - Floating preserves native frames, membership and hide/reveal while stopping
    domain tiling, directional navigation, group underlay and drop preview.
    The active border stays independent. Retile releases the Engine domain
    without writes, then fresh admission (clean/tolerance-valid recursive-cut
    fit, otherwise sequential long-edge seed, no centre inference).
    Intentional float/sticky and maximize/fullscreen exceptions keep their
    semantics; exact-lifetime float intent survives domain release. Mode flips
    cancel stale gesture effects.
  - Sends touching a floating workspace transfer project membership without a
    two-domain Engine plan; only the tiled side reconciles. Floating-side
    frames stay untouched. Sticky movers refuse; ordinary intentional-float
    sends and floating+fullscreen/maximized retention are delivered offline
    under D16; native boundary/combo acceptance remains pending.
  - `Meta/Win+1..9` select an existing workspace without creation; `0` reuses
    or creates the trailing empty. `Meta/Win+Shift+1..9` send the focused
    window; D16 ordinary/combo floating retention is delivered offline.
    Send follows by default (R-WS-01) with a bindable unbound
    send-and-stay counterpart. Relative sends use the scoped ring (item 2).
    Exact follow is one fresh mover-absent-from-source/present-in-target
    proof, switch-before-focus without waiting unrelated geometry; no
    retry/replay; setter returns and signal delivery alone are not proof.
    Stale, ambiguous, missing, no-op, wrong-target, owner, scope, and hook
    failures do not follow; unrelated async layout settling never gates
    confirmed follow.
    KWin geometry and membership are non-atomic and asynchronous. US shifted
    aliases (`Meta+!` through `Meta+)`, `Win` equivalents) register alongside
    digit sends, preserving foreign shortcut records, without establishing physical delivery.
- USER VISUAL/MANUAL acceptance (rapid multi-workspace move/follow use):
  accepted for repeated same-session use
  across many workspaces. The durable preference is graceful, unsurprising
  handling of confirmed partial successes and responsiveness during rapid
  use; it authorizes no ignored errors, retries, queue resets, or
  architecture/uncertain-recovery change.
- `workspaceMode` (`per-output-local`, `global-unique`, `shared`) runs on a
    session-local project-owned KWin backing-desktop mapping, never the native
    COSMIC workspace-set mapping. Adopted preexisting desktops are managed,
    never disposed on teardown. Every live local/global-unique output domain
    and the shared domain keeps at least two logical workspaces plus one
    structural trailing empty (literal native-order trailing empty retained;
    empty non-final managed desktops removable once invisible everywhere).
    Occupied (including floating, fullscreen, maximized), visible,
    flight-pinned, displaced, and unmapped desktops stay protected; sticky
    all-desktops windows do not occupy every backing desktop; unmapped
    desktops remain outside management. Disconnected output: displaced layout
    preserved in separate workspace(s), never merged into remaining layout.
    Active-focus survivor choice: disconnected-monitor active window shows its
    relocated workspace with focus retained; surviving-monitor active window
    preserves current view and focus. Deliberate COSMIC/6-reference deviation
    (G-02, User 2026-10-10): focus continuity keeps the window in use visible.
    On reconnection, displaced workspaces
    return automatically to their original monitor with then-current contents
    (never a saved snapshot, never individually pulled-back explicitly moved
    windows). Multiple-survivor destination: nearest surviving monitor from
    disconnect-time geometry, fallback current primary then output ordering;
    never post-disconnect frame geometry as proxy. Deliberate COSMIC
    first-remaining deviation (G-02, User 2026-10-10): spatial proximity,
    discriminating only with 3+ outputs. Reconnect focus: returning-
    workspace active window shows that workspace with focus retained;
    surviving-output actives preserve view/focus with no stealing. Reconnect
    selection never consults or restores prior-view history (R-WS-08 history
    still records observed hotplug changes per 1.5). Session-local scope, no
    restart-persistent mapping or return guarantee.
- KDE: tray writes only `defaultTiled` and requests KWin reconfigure; only a
  running KWin reread/snapshot confirms it. Per-workspace overrides are
  session-only and reset on script reload; shared mode toggles the backing
  workspace across outputs. KGlobalAccel workspace-tiling action is
  invoked by the tray without confirming application. No per-workspace history
  persists.
- Windows (user decisions 2026-10-02): one normal single per-user/session
  owner executable with standalone stop/restore. Managed
  workspaces use public `SW_HIDE` with nonactivating reveal and a simple
  same-executable watcher (the user is wary of complexity: no added
  machinery). Store KDE's `defaultTiled` equivalent as
  `core.workspace.default_tiled` in the existing version-1 Windows JSON
  settings; older settings backfill `true`. Tray picks update only that saved
  field through the existing store; settings polling confirms runtime
  adoption. `Win+1..9` select existing workspaces only;
  `Win+Shift+1..9` send the focused tiled window with follow after verified
  transfer. E8 prime plus `AttachThreadInput` plus exact foreground readback
  is the focus mechanism. Terminal is ordinary. CLI: `workspace --select N` /
  `workspace --send N`, plus opt-in `--scope-exe`/`--scope-host-child` proof
  fences. Recovery ledger v4 stores min/max show-state without geometry
  (readers accept v1-v3); the window-lifetime membership property is inert and
  distrusted. Exact-owner automation-only `workspace --send N` exists for
  boundary proof; dispatch acknowledgement is not effect proof.
  Status: native locked gates and scoped synthetic/native effect proof passed;
  physical feel and other output/DPI arrangements remain user-owned
  ([workspace tiling](changes/archive/windows-workspace-tiling.md)).
- Delivery coordination for items 1-5: see Move, Layout and Output Commands
  below.
- Windows multi-output is unparked (user 2026-10-10): after the single-output
  handoff queue 1-4, 6, 7, 9-11, 13, 15, 17, 18, 20, with item 18 before
  10/11, proceed to the multi-output foundation and dependent items 5, 12,
  14, 16, 19 ([handoff](backlog.md)).
- Item 1, R-WS-08 / R-WS-11:
    - Windows delivery 2026-10-10: local observed-history/ring, exact Ctrl/Tab
      action routing and settings/UI/presets delivered offline; native gates
      passed ([record](changes/archive/windows-workspace-history-ring.md)).
      Native CLI toggle/ring/>9 and Settings agent-observed 2026-10-11 with
      [item 2](changes/archive/windows-workspace-send-follow-stay.md);
      physical/preset OS journeys and non-local/multi-output remain pending.
    - 1.1: previous-view toggle is Meta+Ctrl+Tab on KDE / Win+Ctrl+Tab on
      Windows. Previous/next workspace uses Meta/Win+Ctrl+arrows and
      Meta/Win+Ctrl+H/J/K/L: left/up previous, right/down next (COSMIC
      parity, `cosmic-comp/data/keybindings.ron:49-56`). KDE stock KWin
      "Switch One Desktop Down/Up/to the Left/to the Right" holds the
      arrow forms (`~/.config/kglobalshortcutsrc` default column :95-98).
      Windows native desktop switching holds Win+Ctrl+Left/Right (general
      knowledge, unverified in repo). Authentic takes over and clears those
      holders; Compatible disables our conflicting arrow forms; letters
      remain.
    - 1.2: local and global-unique modes keep per-output previous history
      and relative rings; shared mode keeps one history/ring. Record every
      successful observed workspace change regardless of producer (our
      commands, native switches, verified send-follow). Same-workspace
      activation and merely focusing another output do not record. Previous
      is a two-view toggle, not MRU traversal.
    - 1.3: remember stable workspace IDs. A surviving workspace, even empty,
      remains valid. A removed/unassigned ID clears the previous entry;
      toggle is a no-op until the next recorded change. No ordinal
      reinterpretation or recreation. Keep this and 1.5 without a setting
      (G-04, User 2026-10-10): majority reference retention comes from global
      history models deliberately excluded by our per-output model in 1.2;
      COSMIC has no history verb. Removed workspaces are never recreated.
    - 1.4: the ring is every existing workspace in scoped order, including
      the trailing empty and ordinals beyond 9; first/last wrap. Selection
      itself creates nothing.
    - 1.5: reconnect selection never consults or restores history; its
      selection policy is unchanged. Hotplug-driven observed changes record
      like any other. A previous entry is valid only while that workspace
      remains in the recording output's scope; movement to another output
      (e.g. return on reconnect) clears it like removal, so toggle is a no-op
      until the next recorded change. A disconnected output's history
      is discarded with the output; output identity is session-local.
- Item 2, R-WS-01 / R-WS-14:
    - Windows delivery 2026-10-11: explicit numbered/relative follow/stay,
      unbound stay catalog/settings/UI and native gates complete; scoped
      ordinary-app CLI/MRU/null/floating/ring and Settings outcomes
      agent-observed. Physical containment and non-local/multi-output pending
      ([record](changes/archive/windows-workspace-send-follow-stay.md)).
    - 2.1: keep numbered follow chords Meta/Win+Shift+digits. Relative
      send-and-follow uses Meta/Win+Ctrl+Shift+arrows and +H/J/K/L.
      Numbered and relative send-and-stay are bindable, unbound by default.
      KDE stock KWin "Window One Desktop Down/Up/to the Left/to the Right"
      holds the arrow forms (`kglobalshortcutsrc:173-176`): Authentic clears
      them, Compatible disables our arrows; letters remain. Windows
      Win+Ctrl+Shift+arrows ownership is unknown.
    - 2.2: relative send uses the same scoped ring as 1.4 (ordinal step, not
      MRU, wrapping including the trailing empty). Sending into it fills it;
      normal lifecycle maintenance supplies the next empty. Resolve the target
      once before transfer. Follow/stay applies to absolute and relative
      sends. A sole-window send-next also fills the pre-transfer trailing
      empty. Deliberate COSMIC deviation (G-03, User 2026-10-10): COSMIC
      refuses, six references fill; predictable send is the reason.
    - KDE floating-boundary sends currently preserve source view; item 2
      routes explicit follow/stay through that path so the default follows
      (implementation gap, not a new decision).
- User decisions 2026-10-08 (workspace migration R-WS-12 D1-D9, outcomes
  decided):
  - D1 bindings: four directional active-workspace migration actions,
    bindable and UNBOUND by default, follow-only, as delivered (presets keep
    empty migration defaults; no new foreign-conflict claims).
  - D2 capability: local/global-unique only with strict
    `options.perOutputVirtualDesktops === true` as delivered. Shared
    mode, false or unreadable capability refuses with a reason before
    writes; never mutate native settings.
  - D3 targeting: active workspace only, using FULL-output-rect reciprocal
    edge-touch + positive-overlap candidates, no output wrap, follow only.
    Largest shared edge then left/top selection (User decision 2026-10-09).
    Unreadable topology refuses; no candidate is a no-op.
  - D4 destination: preserve backing ID, tree/order/shares, remembered
    focus and workspace tiling mode via core `relocate_domain` as
    delivered. Insert immediately after the target output's current
    workspace and show the moved workspace; the previous target
    workspace stays listed and hidden.
  - D5 source/empty (G-37, User 2026-10-10):
    `Source workspace after migration` selects `last-remaining-workspace`
    (default, label `Last remaining workspace`, tooltip COSMIC) or
    `most-recently-used-workspace`
    (label `Most recently used workspace`, tooltip bspwm, i3, awesome).
    KDE `migrationSourceRefill` with a KCM control; Windows
    `core.migration_source_refill`, additive settings schema version 1,
    missing defaults to `last-remaining-workspace`, no migration. Apply
    changes live to subsequent migrations. MRU uses per-output history
    from item 1.2, not a global-history model. Snapshot the previous stable
    ID before map mutation; it is eligible only if live and still assigned
    to the source output's remaining scoped ring, excluding the migrated ID.
    Surviving empties qualify; missing, removed, migrated or out-of-scope
    entries fall back to last remaining. Preserve a still-scoped live source
    current view in both modes. History invalidation in 1.3/1.5 still applies. Destination
    insertion in D4 is unchanged. Empty migration remains allowed and retains
    its backing ID; native lifecycle timing remains for user testing.
    Status: shared selector + KDE setting/KCM/live reread and MRU delivered
    offline ([record](changes/archive/reference-comparison-implementation.md));
    Windows schema-v1/UI/runtime and scoped MRU delivered offline 2026-10-14
    ([Windows record](changes/archive/windows-workspace-output-migration.md));
    native checks pending.
  - D6 focus: retain the moved active client only after all member
    arrivals and both view changes are verified, as delivered.
    Empty/sticky-active migration uses native output switching with
    no fabricated client activation; minimized clients are not
    unminimized or explicitly focused.
  - D7 floats/sticky: workspace-bound intentional/automatic floats are
    carried with class and origin preserved via native output remap,
    as delivered. Sticky all-desktops clients stay on the source and
    are not migrated members. (User decision 2026-10-09: sticky stays on
    the source output, sticky belongs to the output; COSMIC; floats carry.)
  - D8 overlays (CHANGED, decided):
    fullscreen plus maximized members are carried, matching
    Hyprland/sway/i3/niri/COSMIC and native KWin send-to-output. No
    refusal in moved members or affected views; only the native move,
    with no extra size/position/focus writes while fullscreen. An
    explicit user move is not unwanted interference (for example a
    game on the wrong output). Live check pending.
    Delivery 2026-10-09: shared core/KDE carry implemented offline
    ([record](changes/archive/migration-overlay-carry.md)). Windows native adapter
    delivered offline 2026-10-14 with real max/fullscreen observations,
    overlay-preserving remaps and fresh arrival/focus fences
    ([Windows record](changes/archive/windows-workspace-output-migration.md));
    physical carry/re-fit/focus remains pending.
  - D9 history/return: out-of-source-scope previous IDs clear,
    disconnected-output history is discarded, reconnect selection
    never consults or restores it. Remove only the explicitly moved
    ID from automatic hotplug-return associations. Migrated IDs lose
    auto return.
  - Status: core/Linux planner/KDE delivered offline
    ([record](changes/archive/kde-whole-workspace-output-migration.md),
    [spec](spec/functional-spec.md#workspaces) REQ-WS-12a..i); D3 largest-shared-edge
    then left/top selection (User decision 2026-10-09) delivered offline
    ([selection record](changes/archive/position-based-output-selection.md)).
    Windows local adapter, largest-edge selection and G-37 delivered offline
    2026-10-14 ([Windows record](changes/archive/windows-workspace-output-migration.md));
    locked native MSVC gates pass, 1,950 passed/zero failed/one ignored.
    Non-local modes remain item 12; native two-output acceptance remains pending.

## Move, Layout and Output Commands

- User 2026-10-14 approved as delivered: Windows movement and send commands
  repeat while their modifiers remain held, including Win+Shift+arrow and
  explicit output sends. Toggle commands retain one action per press in their
  topic sections.
- User decisions 2026-10-12 (0.1 triage): D09 / MOV-06 defaults to Geometric
  middle; post-0.1 `Nested move entry` has only Geometric middle (COSMIC) /
  Remembered child (i3, sway). D10 / MOV-07 defaults to Wrap; post-0.1
  `Orthogonal escape` has only Wrap (COSMIC) / Escape outward (i3, sway).
  Swap-target-leaf options are dropped (bspwm/awesome only).
- User decisions 2026-10-12: D12 / MOV-04 defaults to Extract to same axis,
  no setting (Retain nesting has Hyprland + bspwm only); D13 / OUT-02 defaults
  to Wrap locally, no setting (Cross output has bspwm/awesome only). These
  alternatives fail the [tier rule](#reference-tiers).
  [Triage selections](research/release-0.1-triage.md#user-decisions-2026-10-12).
- R-MOV-01: perpendicular flat-triple move restructures via COSMIC R1, changing root orientation to V[H[A,C],B] in the fixture rather than refusing (User 2026-10-10, 0.1 triage D11).
- R-RSZ-02/03: outward work-area-edge resize is a no-op; nested resize adjusts adjacent shares at the nearest matching-edge-axis split (User 2026-10-10, 0.1 triage D14).
- Delivery coordination (user 2026-10-07; [implementation order and Windows
  handoff](backlog.md)): the shared Rust core
  plus KDE adapter are implemented KDE-side; the separate Windows adapter
  is wired later.
  Correctness over non-breakage: Windows build/behavior breakage is
  acceptable provided the backlog lists the specific Windows changes needed.
  Compile-only Windows fixes preserving current Windows behavior are allowed;
  correctness-first adapter wiring remains in the backlog handoff.
- Item 1 (R-WS-08 / R-WS-11) and item 2 (R-WS-01 / R-WS-14): see Workspaces
  above. KDE floating-boundary sends currently preserve source view
  (`kwin/src/plan-adapter-entry.ts:4018-4022,4067-4085`); item 2 routes
  explicit follow/stay through that path so the default follows
  (implementation gap, not a new decision).
- Item 3, R-MOV-03 (user decision 2026-10-08; functional IDs delivered offline):
  - 3.1: one global setting, KDE `sameAxisMove`, Windows
    `core.same_axis_move`, with `group-with-neighbor` default (label
    `Group with neighbor`, tooltip names COSMIC) and `swap-with-neighbor`
    (label `Swap with neighbor`, tooltip names i3, sway).
    Windows adds the field within settings schema version 1; missing
    defaults to `group-with-neighbor`. Changes apply to subsequent moves
    without rebuilding existing trees. KDE gets a settings UI control.
    No migration.
  - 3.2: `swap-with-neighbor` replaces only R2c when the neighbor is an
    adjacent direct leaf sibling in the same group; shares travel with
    windows (existing swap semantics). Leaf/group neighbors keep current
    rules; add TBD discriminating rows before broadening.
  - Status: functional rename delivered offline in core/protocol/KDE ([record](changes/archive/admission-and-move-settings.md)); Windows schema-v1/settings UI/live move routing delivered 2026-10-11, base `fafcd31` + delivery commit ([record](changes/archive/windows-same-axis-move.md)). Native gates and agent-observed Apply/Revert/no-rebuild passed; physical directional journey pending.
- Item 4, R-LAY-01:
    - 4.1: Meta+O / Win+O (COSMIC parity). No stock KDE holder found in
      `kglobalshortcutsrc`; Windows Win+O is OS orientation lock
      (`crates/tiler-windows/src/settings.rs:983`). Authentic takes over;
      Compatible disables our Windows binding.
    - 4.2: toggle the immediate parent group, including root, preserving
      child order, shares and focus. Lone root leaf is a no-op. No saved
      orientation hint for future admissions; long-edge rule unchanged.
    - Status: shared core/protocol + KDE Meta+O/catalog/presets delivered
      offline ([record](changes/archive/parent-orientation-toggle.md));
      Windows Win+O/catalog/presets/input/owner routing delivered 2026-10-11,
      base `755aab8` + delivery commit ([record](changes/archive/windows-parent-orientation-toggle.md)).
      Native gates and agent-observed Settings/owner adoption passed; physical
      toggle and OS suppression remain user-owned pending.
    - Windows held-repeat choice (User 2026-10-14 approved as delivered): one toggle
      per discrete down; held repeats consumed without dispatch, release/repress
      toggles again. [Discriminator](spec/reference-outcomes.md#r-lay-01-windows-held-key-discriminator)
      leaves physical/KDE/unsupported reference outcomes TBD.
- User decision 2026-10-09 (R-LAY-04 workspace-local layout selection):
  PARK until a genuine second layout exists; revisit when tabbed stacks are
  designed after 0.1. Whether tabs count as L2 is a future decision.
  REQ-LAY-04 stays a parked requirement, not an implementation. Research
  completed ([research](research/workspace-local-layout-selection.md)).
- Item 5, R-MOV-08 / R-OUT-04:
    - 5.1: local restructure/swap/escape wins first; when none applies the
      window crosses. A sole root leaf also crosses with an adjacent output
      in all four directions, changing today's horizontal SingleRootLeaf
      no-cross too.
    - 5.2 (User decision 2026-10-09): keep reciprocal edge-touch +
      positive-overlap adjacency on FULL output rectangles, not work areas
      (panel gaps cannot block), in all four directions including horizontal.
      Unreadable topology refuses; no candidate is a no-op; no output
      wrapping. Window-based exhausted directional move (REQ-MOV-08/08b,
      R-MOV-11..13, REQ-OUT-01) and explicit output send (REQ-OUT-04,
      R-OUT-04/07) select the candidate whose shared edge contains the
      projection of the moving window centre; if none, the larger overlap
      with the window span along the edge; final left/top tie-break.
      Whole-workspace migration (REQ-WS-12, R-WS-12) selects the largest
      shared edge, then left/top. Deliberate COSMIC deviation: COSMIC
      origin-distance is no better; Hyprland, i3, sway, awesome and bspwm
      select by window position; no reference refuses.
      Technical convention: Left/Right project onto y, Up/Down onto x;
      shared edges are half-open [start,end), with exact half-pixel centres.
      Multiple centre-containing edges tie directly by left/top; span overlap
      is used only when no edge contains the centre. Left/top means ascending
      x, then y; equal positions use stable output identity. Reverse candidate
      multiplicity is valid; the resolved pair still touches reciprocally.
    - 5.3: explicit send-to-output has follow and stay forms. Follow binds
      Meta/Win+Ctrl+Alt+arrows and +H/J/K/L; stay is bindable, unbound.
      COSMIC's Super+Shift+Alt arm collides with our resize-shrink; niri's
      arm collides with 2.1. Meta+Ctrl+Alt arms are absent from
      `kglobalshortcutsrc`; Windows ownership is unknown.
    - 5.4: target the destination output's current workspace; ordinary
      workspace-send admission (remembered leaf, destination focus history,
      root fallback) and per-command follow/stay. Initially workspace-send
      tiled-subject eligibility, sticky excluded. Floating-workspace
      boundaries transfer membership only, reflowing only tiled sides.
      Ordinary float transfer remains a separate open item.
    - 5.5 (User decision 2026-10-14): exhausted directional R4 into an
      occupied target always wraps its whole tree in a new top-level D-axis
      split, with the mover nearest the source. Preserve nested same-axis
      groups: Left onto `H[A,B]` gives `H[H[A,B],W]`, Right gives
      `H[W,H[A,B]]`; Up onto `V[A,B]` gives `V[V[A,B],W]`, Down gives
      `V[W,V[A,B]]`. A single target leaf is wrapped identically; an empty
      target takes the mover alone. Never nest beside a remembered/last-focused
      leaf or append to the existing root. Tier 1 COSMIC observed
      [R4, S20 and S23](cosmic-move-conformance.md#the-rules) select this
      shape; i3 flat append was considered and rejected. Rationale: predictable
      placement independent of target focus history, preserving the target
      layout as one unchanged subtree. Shared core applies this to KDE and
      Windows. [Offline delivery and pending native proof](changes/kde-dual-output-live.md#whole-target-r4-wrap-decision-2026-10-14).
    - Status: shared core/KDE window-based selection delivered offline
      2026-10-09 ([selection record](changes/archive/position-based-output-selection.md));
      core/protocol four-direction moves, explicit output follow/stay
      and native catalog/presets delivered offline
      ([record](changes/archive/four-direction-output-transfer.md)). Windows
      explicit follow/stay wiring delivered offline `fda618f`, 2026-10-14
      ([send](changes/archive/windows-explicit-output-send.md),
      [selection](changes/archive/windows-position-based-output-selection.md));
      native gates 1,892 passed, zero failed, one ignored. User-owned two-output
      native journey pending. Full rectangles select adjacency only; placement
      retains per-desktop work areas. Largest-edge migration subsequently
      delivered offline with item 14
      ([record](changes/archive/windows-workspace-output-migration.md)).
    - Windows held output-send repeats (User 2026-10-14 approved as delivered):
      repeats dispatch while the original Win/Ctrl/Alt and Shift polarity
      remain live. Follow
      rejects Shift; explicitly rebound stay accepts Win+Ctrl+Alt or
      Win+Ctrl+Alt+Shift for now (exact Shift distinction only if a need
      appears) and uses its canonical directional letter hold slot.
      [Discriminator](spec/reference-outcomes.md#windows-output-send-held-key-discriminator).
## Fixed-Size Admission
- User decisions 2026-10-08 and 2026-10-10 (fixed-size admission R-SPC-04
  D1-D8; D1 per-axis zero and D5/D6/D7 delivered offline,
  native checks pending):
  - D1 predicate: both min/max vectors present, usable and nonnegative;
    retain full-zero and unbounded-sentinel guards, no inference from
    `resizeable`. The sentinel guard is a deliberate KDE host adaptation
    (G-01, User 2026-10-10): KWin's marker means "no limit"; references
    compare raw values, and ten references including COSMIC float raw-equal
    sentinels. This is not plain COSMIC parity. Setting values are
    functionally named `Width and height both fixed` (default, tooltip
    names COSMIC) and `Width or height fixed` (tooltip names Hyprland
    (Wayland) and sway). KDE `fixedSizePredicate`, Windows
    `core.fixed_size_predicate`: `both-axes-fixed` default or
    `either-axis-fixed`. Zero is unset per axis (G-05, User 2026-10-10):
    a fixed axis needs equal nonzero bounds. Equal partial-zero hints such
    as min=max=(640,0) are not fixed under `both-axes-fixed`, but are fixed
    under `either-axis-fixed` through the genuinely fixed nonzero axis.
    Deliberate COSMIC deviation: COSMIC raw equality floats partial-zero;
    five of eight non-scrolling references plus PaperWM/karousel tile;
    per-axis zero handling is consistent with the full-zero guard.
    Both predicates retain the other hint-validity guards. Changes affect
    subsequent admissions only, never existing classifications.
    Same-axis values:
    see item 3 under Move, Layout and Output Commands (canonical).
    Breaking configs is acceptable
    pre-release (dogfooding correctness priority), no migration.
    Status: D1 setting delivered offline ([record](changes/archive/admission-and-move-settings.md)); G-05 shared-core predicate/KDE delivered offline ([record](changes/archive/reference-comparison-implementation.md)); Windows schema/UI/live predicate and per-axis-zero transport delivered offline 2026-10-11 ([item 13](changes/archive/windows-fixed-size-admission.md), [item 17](changes/archive/windows-per-axis-zero-predicate.md)). Native hints/UI journey pending.
  - D2 hint changes: admission-only in both directions as delivered;
    keep reacting to windows resizing themselves to avoid
    gaps/overlaps. No new hint-signal work requested.
  - D3 user override: explicit tile/sticky-off-to-tile wins for the
    same live client (hide/show, cross-domain observation, workspace
    re-adoption); no setting. A new client classifies again; sticky
    origin/toggle rules stand.
  - D4 born-maximized: fixed plus born-maximized uses a floating base
    under native maximize as delivered, narrowing the Q3 intersection.
  - D5 born-fullscreen (CHANGED, delivered offline):
    born-fullscreen first exit classifies as newly admitted: fixed
    floats with no writes, otherwise tiles; previously fixed-floating
    restores float. No writes during fullscreen. Deliberate COSMIC
    deviation for game safety; pending live observation.
  - D6 workspace enable (CHANGED, delivered offline):
    enabling workspace tiling checks every window being tiled,
    including arrivals in a floating workspace: fixed stays/becomes
    untouched float, others tile. Explicit user tile overrides stay
    tiled; intentional/sticky floats keep existing rules. COSMIC
    deviation; pending live check.
  - D5/D6 status: shared core/KDE delivered offline ([record](changes/archive/fixed-size-exit-and-enable.md)); Windows current-hint first exit and every-candidate enable wiring delivered offline 2026-10-11 ([record](changes/archive/windows-fixed-size-admission.md)); native/game checks pending.
  - D7 startup: startup adoption classifies as delivered (foreground
    and hidden) PLUS fixed-window user tile overrides persist across
    owner restart in the same intentional-float store, with recompute
    fallback if unavailable.
  - D7 status: Linux planner/KDE delivered offline ([record](changes/archive/fixed-window-tile-override-restart.md)); Windows fixed-origin tile marker delivered offline 2026-10-11 ([record](changes/archive/windows-fixed-size-admission.md)). Only settled tile wins over known automatic fixed origin write `OmniTiler.TileOverride.v1`; generic non-fixed unfloats do not. Hydration covers visible and retained/maximized admitted lifetimes before automatic classification, without requiring hints or granting write/recovery authority.
  - Tentative Windows D3/D4/Q3 intersection, native verification pending (User
    2026-10-14: kept for now; Tier 1 research before a shared cross-platform
    decision): hydrated fixed-origin overrides keep the reserved tiled slot
    beneath maximize under both `core.born_maximized` values, including hint
    loss. Generic non-fixed maximize routing remains unchanged; the R-SPC-13
    native discriminator stays TBD.
  - D8 no-touch: automatic fixed floats are membership-only: no
    geometry, focus, stacking or keep-above writes. Manual floats
    retain keep-above because intentional floats stay above windowed
    games.
  - Status: shared core/Linux planner and KDE delivered offline
    ([record](changes/archive/fixed-size-admission.md)). Windows
    admission D1-D8/G-05 delivered offline 2026-10-11, prerequisite `a41fcb5`
    plus runtime delivery ([record](changes/archive/windows-fixed-size-admission.md));
    native checks of every user-selected choice remain pending.
## Restart Persistence

- User decision 2026-10-12, D25 / REQ-RST-01c / R-RST-01, option (a):
  orderly owner restart with apps kept alive retains the host-owned workspace
  set; re-observe membership from the host without moves; restore focus only
  where provable. Layout is rebuilt by re-adoption, with no saved-layout
  restore; intentional floats stay floating via the existing restart store.
  No setting. [Tier basis](#reference-tiers): i3 (Tier 1) plus xmonad (Tier 2)
  support membership/focus, set kept 5/8; COSMIC has no counterpart, no
  evidenced alternative. Verification-only; native outcomes remain pending.
- User decision 2026-10-12, D26 / REQ-RST-02 / R-RST-02, option (b):
  end-session restore reloads already-persisted OmniTiler gaps/settings and
  re-observes host workspace set/membership/maximize state; no invented layout
  restore. Listed 0.1 known limitation: intentional-float memory does not
  survive logout/login. Reopened apps have new identities; approved
  session-scoped REQ-RST-01d lifetime remains unchanged. Cross-login memory is
  revisited after 0.1 with the shared restart store. No code; verification-only.
- Native dogfood/live checks will assess D25/D26, per the user's 2026-10-12
  decision. Selection does not establish native acceptance.
  [Approvals](research/release-0.1-triage.md#user-decisions-2026-10-12),
  [gap assessment](backlog.md).
- User decisions 2026-10-08 (intentional-float restart R-RST-01 D1-D4,
  outcomes decided, delivered offline; native checks pending):
  - D1 storage: Rust planner-owned private runtime store under
    `$XDG_RUNTIME_DIR` with session-bus plus KWin-owner namespace as
    delivered (0700 dir, 0600 file, atomic replace, version and bounds
    checks, no symlink following; new KWin/login never adopts old
    namespace). Accepted architecture boundary change: relaxes the
    planner no-persistence boundary for this store only.
  - D2 geometry: membership-only restore, preserving the current live
    frame including native movement while the owner was stopped, as
    delivered. Store no geometry, focus, stacking or pre-sticky
    history; hydration writes none of those. Shared core restart store
    (float intent, tile overrides, possibly positions; platforms
    supply identity plus location) is reconsidered when macOS starts;
    deferred.
  - D3 settlement: persist explicit float/unfloat intent only after
    successful native application as delivered (confirmed
    adopted-sticky-off ordinary intent also persists). Clear on
    settled unfloat and verified close; prune only on a complete live
    inventory, never on scoped, hidden or minimized absence.
  - D4 availability: degraded store handling is diagnosed
    availability fallback as delivered: missing reads as empty;
    unreadable, corrupt or namespace-mismatched state emits a
    bounded degraded reason/count summary then proceeds empty; no
    hold or retry. Write failure keeps local intent and native state;
    the next settled membership update rewrites the full set.
  - Status: Linux planner/KDE delivered offline
    ([record](changes/archive/kde-intentional-float-restart.md)).
    Windows intentional/sticky identity delivered 2026-10-11 with native
    agent-owned double-restart/scope evidence ([record](changes/archive/windows-restart-intent.md));
    Windows fixed-origin tile overrides delivered offline with item 13
    ([record](changes/archive/windows-fixed-size-admission.md)). Physical/app checks remain pending.
- User decisions 2026-10-08 (workspace migration R-WS-12 D1-D9, outcomes
  decided): see Workspaces above; Windows local adapter and largest-edge
  selection delivered offline 2026-10-14
  ([record](changes/archive/windows-workspace-output-migration.md)).
- Windows restart intent persistence (user 2026-10-10, handoff item 8,
  option A): on-window `SetProp` markers with the distinctive project prefix,
  same pattern as `OmniTilerSticky`, for intentional-float and
  fixed-window tile-override intent. A marker restores classification only,
  never authorizes writes or recovery ownership; written after native
  success, removed on unfloat/sticky-off/re-float; unreadable means no
  intent, logged. No separate store file and no recovery-ledger change.
  REQ-RST-01c membership/set/focus is NORMATIVE D25 (User 2026-10-12),
  verification-only with native acceptance pending; post-restart un-stick
  remains TBD.
- Windows item 8 delivery 2026-10-11 ([record](changes/archive/windows-restart-intent.md)):
  intentional float and both sticky origins survive owner restart, verified
  offline and on owned native helpers through two graceful restarts and scope
  switches. Batched hydration precedes tiling; markers grant no write/recovery
  authority. Fixed tile-override marker is now wired by item 13/D7
  ([record](changes/archive/windows-fixed-size-admission.md)); only known
  automatic fixed provenance permits its success-only write.
  User 2026-10-14 approved as delivered: dotted/versioned `OmniTiler.*.v1`
  classification names replace the old sticky name (old markers not hydrated),
  and test-needed exact-owner `workspace --float` / `--sticky` controls reuse
  normal dispatch. Native `GetPropW` NULL cannot distinguish every unreadable
  read from absence; it means no intent, while invalid nonzero values diagnose.
  REQ-RST-01c is NORMATIVE D25 (User 2026-10-12); this delivery establishes
  float/sticky identity, with membership/set/focus native acceptance pending.
  Post-restart un-stick remains TBD.

## Reference Matrix and Spec Authority

- The 2026-10-11 tentative triage units D01, D03, D09,
  D10, D12, D13, D16, D22, D23, D25, D26, D27 and D28 are now user-decided 2026-10-12.
  [Triage authority](research/release-0.1-triage.md#user-decisions-2026-10-12).
- User direction 2026-10-03: the [reference-WM outcome matrix](spec/reference-outcomes.md)
  records minimal action sequences and per-WM outcomes as the evidence source of
  truth feeding the cross-platform functional specification. Existing selections
  remain authoritative; reference outcomes become supported variants only when
  explicitly selected, with user-settable configuration where applicable.
- User decision 2026-10-06 (matrix format): keep the matrix index at
  `docs/spec/reference-outcomes.md`, with one file per area under
  `docs/spec/reference-outcomes/`, stable scenario IDs and precise fixtures,
  actions and observations. Every new scenario uses Given/When/Then blocks
  with one Then bullet per profile: the eight existing references, niri,
  PaperWM, karousel, paneru, and separate Ours KDE and Ours Windows entries.
  Existing wide tables stay unchanged until a separate migration. Column and
  viewport notation is model-qualified; unsupported fixtures/actions and
  owner-specific journeys are explicitly qualified, applicable unknowns TBD.
  Reference baselines use pinned shipped defaults and named discriminating
  variants. PaperWM.spoon is corroboration only. Evidence expands the corpus,
  not the selected product behavior or consensus denominator.
- User decision 2026-10-07 (reference default rule), amended by the user's
  2026-10-12 [reference tiers](#reference-tiers) below. Earlier decisions stand
  unless revisited; gaming-safety and host/capability exceptions are unchanged.
- User decision 2026-10-07 (functional spec format): keep
  [the functional spec](spec/functional-spec.md) as a single file; revisit
  splitting if it grows much larger. Requirements are normative only where a
  recorded decision selects them; everything else stays OPEN or PROVISIONAL.
- Convention (documentation): compact cell citation keys resolve to dated
  user tests, pinned source file/line ranges or linked documentation;
  missing outcomes remain TBD, and tested versions are never inferred from
  later source checkouts. This convention selects no product behavior.
  Operational guidance lives in [AGENTS.md](../AGENTS.md).
- Matrix maintenance: reuse existing scenarios; for each uncovered ambiguity,
  add the shortest discriminating action sequence. Read source where confident;
  otherwise leave the outcome for the user's later test. Variant hook names are
  provisional indexing, not new product or settings commitments.
- User decision 2026-10-10 (matrix follow-ups): defer both GNOME Shell/Mutter/
  wlroots host-source pinning and fixture/scenario wording tightening (302
  residual cells across 92 rows). Revisit wording only when a specific open
  decision depends on such a row; neither follow-up is active P1 work.
  The former host-source estimate was 19 cells (17 PaperWM, 2 sway); completed
  extension tracing reclassified 21 additional PaperWM cells, giving 40
  current candidates (38 PaperWM, 2 sway), not guaranteed whole-cell closures.
  Completed source passes and remaining limits are retained in the
  [archived residual ledger](changes/archive/reference-source-fill-remaining.md).
- Workflows transfer across Linux, Windows and macOS (shortcuts and behavior
  consistency decided under Shortcuts above).
- Live-test reference environments are deferred until after 0.1 (user
  2026-10-10). Revisit when a specific decision needs live reference evidence
  or when tabbed stacks are designed; the
  [proposal](research/live-test-vms/proposal.md) stays parked research.

### Reference tiers

- User decision 2026-10-12:
  - Tier 1 (decide defaults and settings): COSMIC, Hyprland, i3, sway.
  - Tier 2 (supporting/tie-break, cannot carry a change alone): awesome,
    qtile, xmonad.
  - Tier 3 (context only): bspwm, niri, PaperWM, karousel, paneru,
    Windows/macOS tilers.
  - Default: COSMIC unless the other Tier 1 references (Hyprland, i3, sway)
    broadly agree against it (all three, or two plus Tier 2 support).
    Gaming-safety and host/capability exceptions are unchanged.
  - Setting: only when the alternative is held by at least one Tier 1 plus
    at least one more Tier 1 or Tier 2 reference.
  - Earlier decisions stand unless revisited.
- User's reasoning (2026-10-12):
  - Tiering reflects popularity/reach.
  - COSMIC ships in Pop!_OS with broad reach and modern design choices.
  - xmonad is less popular.
  - bspwm is declining, X11-only, and older style.

## Shortcuts, Conflicts and Presets

- Shared defaults (user 2026-10-01 option A): Meta/Win shortcuts are defaults
  across platforms, matching KDE: Meta/Win+Arrow navigates focus,
  Meta/Win+Shift+Arrow moves windows. USER rule 2026-09-28: every HJKL
  directional shortcut has an
  arrow-key alias. Grow uses `Meta/Win+Alt+H/J/K/L` plus arrows (the removed
  Custom Tile controller's legacy `insert-*` reservation on those arrow chords
  is retired; Plasma 6.7.5 defaults them to `kwin/Switch Window Left/Down/Up/
  Right`). The initial
  release supports standard US keyboards and preserves hardcoded shifted
  aliases; layout detection, omission, opt-in configuration and migration are
  deferred. macOS modifier mapping is decided when macOS starts.
- Launcher shortcuts (user 2026-10-11, Windows): default-on Win+B opens the
  user's Windows default web browser (system association, never a named
  browser) and Win+T opens Windows Terminal. Rationale: auto-tiler users
  rely on launch shortcuts. Both chords have native Windows holders
  (notification area / taskbar cycling; general knowledge, unverified in
  repo) and follow the conflict model
  (Authentic takes over, Compatible disables). KDE/macOS counterparts are
  not yet selected. Delivered offline 2026-10-12
  ([record](changes/archive/windows-launcher-shortcuts.md)); physical check
  pending. User 2026-10-14 approved as delivered: the browser opens an empty
  `https://` URL through the shell `open` verb, and a missing `wt.exe`
  logs narrowly with no fallback.
- Conflict model (user 2026-10-03, all platforms): a per-binding conflict
  list. Settings show each binding conflicting with an OS/desktop shortcut
  and let the user keep (override), disable or rebind it. Quick-set presets:
  Compatible (avoid conflicting OS bindings) and Authentic (stay consistent
  with tiling WMs such as COSMIC/Hyprland, override OS bindings). The preset
  choice may be offered on first run. Applies to KDE, Windows and macOS.
- Windows input and Snap (user 2026-10-01): `WH_KEYBOARD_LL` with `vkE8`
  menu-mask at Win key-up while Win held, consuming only approved catalog
  chords; `RegisterHotKey` rejected after all four chords returned 1409
  (owner unknown in observed runs). Win+L stays explicit opt-in only and unproven.
  Windows takes over native Snap shortcuts by default, with a visible setting
  to turn takeover off. While a workspace is tiled, prevent Snap through
  keyboard (selected LL hook) and mouse paths (session-only
  `SPI_SETWINARRANGING FALSE` while tiling is active, with visible
  Apply/Revert, exact preimage capture/readback and conditional restoration
  on stop; on observed Windows 11 builds, disabling via pvParam FALSE has
  required uiParam TRUE; verify on the host). Snap Layouts flyout/Snap Assist coverage remains to be proven.
- Windows Authentic/Compatible (user 2026-10-08): defaults to Authentic with
  Compatible available in Settings. Owned-chord interception is independent of
  foreground/action eligibility. Pressing an explicitly bound command
  (including Win+G while gaming) is an explicit user action for the tiler
  binding. Authentic Win+G/F11 ownership limitation stays disclosed; use
  Compatible, disable, or rebind for Game Bar access. The containment gap
  stays parked for later test and review; a mechanism/architecture change or
  registry/policy workaround needs a user decision. Windows 100ms pump gaming
  cost is unmeasured. Status in [gaming
  coexistence](changes/windows-gaming-coexistence.md).
- Windows first-run and rebinding (user 2026-10-08): normal startup with no
  settings file offers native Authentic (default) / Compatible choices,
  explaining Win+G/F11 implications. The owner lease precedes UI; atomic
  create-if-absent publication never replaces a file appearing during the
  prompt. User-approved 2026-10-11: graceful stop cancels the pending prompt
  without publishing settings and proceeds through ordinary teardown.
  Delivered offline with an exact owned, cancellable native dialog
  ([record](changes/archive/windows-first-run-stop.md)); physical check pending.
  Compatible resets to the default catalog then disables the
  OS-conflicting physical chords, including Win+G/F11, inventing no
  replacements. Per-binding Keep/Disable/Rebind with separate directional
  letter/arrow rows and actual rebound-chord conflicts. Initial rebind limit
  was Win plus the action's existing Shift arm only; Alt/Ctrl and unshifted
  Win+L targets refused; keyboard resize rows were initially unavailable; further
  workspace mappings await runtime implementation. Windows keyboard resize
  delivered 2026-10-11, base `f794cf9` + delivery commit: dedicated Win+Alt
  grow / Win+Shift+Alt shrink intents, HJKL and arrow aliases, shared Engine
  pixel steps matching KDE, and real Keep/Disable/Rebind controls with exact
  modifier arms ([record](changes/archive/windows-keyboard-resize.md)). Native
  gates, downstream CLI geometry and Settings Apply/Revert agent-observed;
  physical interception/rebound adoption user-owned. Tentative, pending user
  review: both presets keep resize defaults because Windows ownership is
  unverified; Compatible disables known conflicts only, with no conflict-free
  claim. Test-needed exact-owner `resize --direction DIR --mode MODE` control
  is also tentative; it uses production fences, not injected-hook bypass.
- KDE staged controls (user 2026-10-08): the unified Settings page stages
  Keep/Disable and Authentic/Compatible choices (see the shortcut catalog in
  `kwin/native-effect/shortcutreconciler.cpp`: `shortcutProjectCatalog` and
  `shortcutConflictTable`) over one selection-scoped Apply/Force/Revert flow;
  explicit Apply or confirmed and revalidated Force commits them.
  Ordinary settings Save stays isolated. Project Apply preserves the current
  assignment for Keep, including custom chords; Authentic explicitly resets to
  the canonical catalog, staged until explicit Apply/confirmed Force and consumed on
  success (failed/declined attempts retain intent). Delivered offline
  2026-10-09: Keep preserves custom, canonical, and empty assignments; conflicts
  on custom assignments show their actual chords without a silent reset.
  Compatible: reset to Keep, then disable compiled
  known conflicts and discovered foreign default/current-holder collisions; no
  replacement chords or automatic foreign-default restoration; use Revert
  Shortcuts separately after earlier Force clearing. Disabled focus-right
  leaves Lock Session and Meta+Esc untouched. Persistence: Disable clears only
  the project's KGlobalAccel assignment; native shortcut storage is
  authoritative, no parallel preset file; empty assignments cannot distinguish
  deliberate disabling from earlier unresolved registration; Keep of either
  stays empty unless Authentic was staged (M13 interpretation). Restart
  persistence is user-owned live acceptance. Deferred: first-run preset prompt
  and integrated rebind editor; KDE Shortcuts remains the custom-binding
  editor; no startup correction or re-registration. See [KDE conflict
  model](changes/kde-shortcut-conflicts.md) and [M13 delivery](changes/archive/kde-keep-preserving-shortcut-apply.md).
- Force/Revert contract (applying the user's 2026-09-26 Delivery 2 direction):
  Force may clear ANY holder of a project-required chord after listing and
  confirmation. User 2026-10-12: Apply Shortcuts acts directly when conflict-free;
  a zero-write holder refusal opens a modal conflict dialog, replacing inline
  refusal/Force controls. It lists all current conflicts with chords,
  component/action display names (IDs as fallback), found keys, exact
  required keys removed, and unrelated keys kept, including unknown and legacy
  project-owned IDs. Ordering belongs to the reconciler: default-only blockers
  first, then clearable holders, sorted by component/action (blocker chord as
  tie-breaker); status, refusal and dialog agree on the first conflict. No
  foreign-holder summary truncation. Project actions, Lock Session, and the
  authorized System Monitor `Meta+Esc` holder are exempt; the dialog describes
  the actual in-scope Lock relocation and authorized delivery displacement,
  or states that Lock is untouched. A holder claiming a chord with no
  required key in its active list is listed with its retained keys and blocks
  Force until unbound manually; Force Apply is disabled with the reason shown.
  Force Apply and Cancel are the dialog's actions. Cancel discards the preview
  without writes or another dialog. Tentative, pending user review: no stacked
  confirmations, so Apply has no preliminary prompt and this dialog is the
  sole Force confirmation. Apply/Force applies immediately, with the result
  shown in a short dialog (wording under [Settings](#settings-tray-and-first-run));
  operational/partial failures report a brief reason, without offering
  Force as recovery. Revert retains one custom-binding-loss confirmation and
  then shows a success/error result dialog, including empty-list success.
  Confirmed Force revalidates owner, live images, and the full holder snapshot
  before persist; stale confirmations fail closed with zero writes. After
  persist, each holder is re-read immediately before its foreign setter and
  aborts on drift with zero further writes; the persisted union is retained as
  an interruption-safe superset. Minimal durable cleared ID list at
  `~/.config/omnitiler/shortcut-clearedrc` is union-persisted BEFORE
  clearing and emptied only after successful Revert. Revert restores KDE
  defaults for every non-project ID in the cleared list; project-owned IDs
  stay cleared; absent/duplicate IDs fail closed retaining the list; an empty
  list is a no-op success. Journal files are ignored; no migration, Finish
  Apply, or Restore path remains. Force preview transient labels are never
  persisted. Stateless Revert was rejected: it would alter never-cleared
  actions, activate default-only holders, and miss custom holders. Revert
  replaces the full active key set from fresh current tuples (4-field
  actionId, no 2-field daemon assumption), so custom cleared bindings are
  lost, as the user accepted 2026-09-26. Partial Revert failure retains the
  list for a later resume. Shortcut operations emit bounded structured
  diagnostics (see Observability above); logging never affects behavior.
- Shortcut Apply reliability (approved by user 2026-10-14): bound
  writes per explicit Apply/Force/Revert operation rather than the Settings
  window lifetime. Additional serialization/throttling is unnecessary: writes
  are already serial and fast. Run shortcut discovery/mutation on the serialized
  background worker with working status, disabled shortcut controls and
  epoch-bound reload staging. In-flight work can delay Settings close if D-Bus
  hangs; do not add per-call deadlines now, revisit only if a hang is observed.
  Remove previous-identity handling for `kwin/plasma-auto-tiler-*`: ordinary
  foreign holders, no special label or project-owned Revert skip. This is
  pre-release development residue; no pre-release migrations, per Simplicity.
  Omit Lock Session's exempt line when actual keys equal post keys; show it only
  when Apply moves the binding, including preserved alternate keys and the
  approved System Monitor exception. [Delivery and
  live evidence](changes/archive/kde-shortcut-apply-failure.md).
- Required-chord corrections (user 2026-09-28; all through reversible
  Apply/Force/Revert, never
  relocated, no broad deletion): project focus/move arrows supersede KWin
  Quick Tile and Previous/Next Screen defaults; grow arrows supersede
  Switch Window defaults (clear, do not relocate); `Meta+G` supersedes Grid View and `Meta+M`
  supersedes Krohnkite Monocle (shadowed-delivery diagnostic until the user
  applies the override); focus-right `Meta+L` relocates `ksmserver` Lock
  Session to `Meta+Esc` (sole authorized target-occupant exception: System
  Monitor may hold `Meta+Esc`, never writable by the override). Approved
  2026-09-21, standing
  until revoked: no broad shortcut deletion, unverified actions,
  ownership/readback changes, or startup mutation. No other foreign occupier
  is authorized, and no per-component `cleanUp()` path exists.
- R-MOV-03 same-axis setting: see item 3 under Move, Layout and Output
  Commands (canonical). Functional naming per Functional naming below.
- Functional naming (user 2026-10-08): settings and their values use
  functional names; reference WMs appear only in tooltips. Exact IDs decided
  for `sameAxisMove`, `fixedSizePredicate`, and `migrationSourceRefill` above.
- Workspace tiling toggle default (user 2026-10-10): KDE `Meta+Y`, Windows
  `Win+Y`, matching COSMIC `ToggleTiling` on `Super+Y`
  (`cosmic-comp/data/keybindings.ron:85`, pinned `3d55cba0`). All other
  currently unbound actions stay unbound; COSMIC `SendToWorkspace` and
  `MigrateWorkspaceToOutput` also have no binding in that pinned file. KDE
  implementation uses the catalog and existing Authentic/Compatible conflict
  model. KDE delivered offline 2026-10-11: registration and native/KCM catalog
  use `Meta+Y`; no stock Plasma 6.7.5 holder found in static source/shortcut
  inventory ([record](changes/archive/kde-workspace-tiling-shortcut.md)).
  Physical delivery, existing-assignment/restart persistence and live presets
  remain user-owned. Windows delivered offline 2026-10-13
  ([record](changes/archive/windows-workspace-tiling-shortcut.md)): Win+Y
  routes to the existing tray workspace-mode toggle; no new focus/window
  policy. One toggle per discrete press; held repeats consumed without
  dispatch (User 2026-10-14 approved as delivered). The recorded Mixed Reality
  conflict follows Authentic takeover /
  Compatible disabled (87 catalog rows, 41 Compatible-disabled rows).
  Native/physical toggle, tray sync, Apply/Revert, presets, rebind/disable and
  takeover-off release remain user-owned.
## Gaming Safety

- Gaming compatibility must be flawless (see Principles). Provide alternate
  access to OS gaming surfaces displaced by authentic bindings (Windows Game
  Bar, displaced by Win+G), and avoid behavior that anti-cheat software could
  flag as a false positive.
- User decision 2026-10-03 (Windows "Xbox mode", the Xbox full screen
  experience): when detected, pause tiling, window effects (border/underlay)
  and shortcut handling, while remembering windows and workspaces so they
  are restored when Xbox mode ends. Detection uses a documented Microsoft
  signal only, no cloak/foreground heuristics; automatic pause stays
  unimplemented until such a signal exists.
- Alternate Game Bar access for keyboard and mouse players (a shortcut over
  a fullscreen game) is a later experiment, after parity and correctness
  work (user 2026-10-03).
- Shortcut containment (Authentic Win+G/F11) is decided under Shortcuts above;
  Windows 100ms pump gaming cost is unmeasured.
- Windows gaming coexistence is a Windows release gate after multi-output,
  before any Windows release (user 2026-10-10): Win+G/F11 containment review,
  Xbox mode detection and M16 100ms pump-cost measurement. Its P0 label does
  not schedule it now. Non-native research informs the measurement
  ([event/pump boundary](research/non-native-tiler-host-interaction.md#layer-layout-authority-events-and-reconciliation-dim-1),
  [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [evidence](changes/archive/non-native-tiler-host-interaction.md)).

## Placement, Minimums and Startup Adoption

- User decision 2026-10-12, D01 / INS-06 (a): native activation ownership.
  Admit C structurally into the layout behind B's retained maximize/fullscreen
  state and reserved slot. KWin decides activation/focus and stacking: honor
  host-approved activation, host denial and explicit user switching (e.g.
  Alt+Tab). Never write B's fullscreen geometry; OmniTiler never actuates focus
  while a fullscreen window is involved. The native activation ownership rule
  also applies to maximized B. KDE convergence's automatic focus fence is
  delivered offline, including a foreground re-read after geometry application;
  explicit focus shortcuts and send-follow/stay remain effective. CLOSE-05 (a)
  removal/refill/focus is verified offline; native cleanup remains live-pending
  ([record](changes/archive/kde-fullscreen-activation-and-send.md);
  [research](research/d01-fullscreen-admission.md)).
- User decision 2026-10-12, D03: INS-01b uses After focused; INS-04 (a)
  reapplies the policy per admission leg (legs 2-3 live-pending).
  No `New window position` setting: Before focused is held only by Tier 2
  xmonad/qtile and fails the [tier rule](#reference-tiers).
  [Triage selections](research/release-0.1-triage.md#user-decisions-2026-10-12).
- R-INS-03: first admission fills the work area as a single tile and natively activates the newcomer (User 2026-10-10, 0.1 triage D04).
- R-INS-05: admission with an ordinary float focused anchors at the nearest tiling neighbor (B, prior tiled focus in the fixture), using ordinary long-edge admission instead of root-wrap (User 2026-10-10, 0.1 triage D05).
  Verification-only with unchanged admission policy (2026-10-11):
  the existing shared Engine convergence route already retains tiled focus B
  while an ordinary float is focused. Keep the offline geometry/focus and KDE
  wire fixtures; native observation/activation acceptance remains user-owned.
  [Evidence](changes/archive/triage-batch1-kde-shared.md).
- R-OUT-05: opening with two occupied outputs routes to the focused output and focuses the newcomer, not by pointer (User 2026-10-10, 0.1 triage D06).
  User 2026-10-14 approves KDE adapter routing for 0.1. Move output selection
  into Rust core/Planner in post-0.1 Push code down, alongside born-maximized;
  the current Planner receives an already selected native domain. Native
  timing/safety checks remain adapter responsibilities. [Delivery](changes/kde-dual-output-live.md#focused-output-admission-correction-and-breadth-repeat-2026-10-14).
- R-CLOSE-02: a reopened client is fresh admission at current focus, never old-slot restoration (User 2026-10-10, 0.1 triage D07).
- Shared:
  - User statement 2026-10-03: default split placement is long-edge based: a
    tall target splits vertically (stacked) and a wide target horizontally
    (side by side), including windows arriving by workspace send.
  - Workspace send resolves the valid remembered destination leaf, then valid
    destination focus history, before the genuine no-focus root fallback.
    Minimum hints influence the projected target rectangle and final
    allocation; they do not search alternative axes or targets for
    feasibility. The shared Engine applies this on KDE and Windows. See
    [send-axis evidence](changes/archive/windows-send-split-axis.md).
  - User decision B6 (2026-10-05): writable admitted windows are placed at
    the proportional tile's origin with each native extent at least its
    declared minimum, rather than skipped while their tile space is reserved.
    Equality/readback/refusal use that effective target. Overconstrained
    leaves are flagged without skipping. Oversized windows may overlap
    siblings or extend beyond the work area when the sequential seed cannot
    fit; no alternative-axis search, floating fallback or global optimizer is
    selected. This is a deliberate deviation: 7/8 references ignore tiled
    minima (they can crop clients; our hosts and apps hold minimum sizes).
    Overlapping windows are an absolute last resort in an auto tiler.
  - User decision 2026-10-08 (startup adoption): fresh startup preserves
    clean/tolerance-valid recursive-cut adoption, and declines overlapping or
    minimum-infeasible fits to the existing deterministic sequential
    long-edge seed with no centre inference. No topology search or guaranteed
    balanced 2x2 is selected. Clean previously tiled 2x2/nested layouts retain
    their fit and identity order. This is not exact COSMIC parity.
- KDE: origin+minimum actuation delivered offline 2026-10-07 with
  effective-target equality and bounded host-shortfall acceptance; native
  R-MIN-01..03 journeys remain user-owned
  ([record](changes/archive/kde-minimum-origin-placement.md)).
- Windows:
  - Query fresh application-declared minimum track sizes and supply
    visible-frame physical-pixel hints to the shared minimum-aware projection
    (user 2026-10-02 option A). No learning; generic learned limits parked.
    Failed, timed-out or invalid native queries supply no hint. Keep existing
    refused-attempt suppression. Evidence:
    [placement correctness](changes/archive/windows-placement-correctness.md).
  - Retained overlay minimums (user 2026-10-08): tiled maximized/fullscreen
    members retain last-known declared minimum hints, bound to the member's
    lifetime token and canonical slot, until normal fresh queries resume.
    Floating/born-slotless, minimized and cloaked rows do not reuse hints. A
    successful asynchronous Win+M restore dispatch arms a two-second bounded
    completion wake on the existing 100ms pump; it reconciles once restore is
    observed. The wake is an observation window, not hint expiry or a latency
    guarantee; physical timing remains user-owned.

## Engine Architecture and Convergence

- Core extraction (user 2026-09-30): portable core completed through K1
  (visual policy in `tiler-core::visual`) under KDE first, with KWin fixtures
  and applicable live checks. K2 settings/action intent and K3 difference
  classification stay in the KWin script; revisit when macOS starts, using the
  candidates and boundary costs in the
  [post-Windows audit](research/cross-platform-core/post-windows-audit.md).
  Pending remainder stays in the backlog. Moving the workspace model and
  remaining portable policy to core is deferred until a non-KWin host needs
  it; the current KWin logical-workspace implementation is the "native
  workspaces" mode of that future choice.
- KDE current approach is retained after the non-native research (user
  2026-10-10): no new follow-up items; Rust-owned layout, KWin observation/
  actuation, host-parented visuals and explicit shortcut lifecycle remain.
  [Candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [cross-cutting findings](research/non-native-tiler-host-interaction.md#cross-cutting-findings)
  and [evidence](changes/archive/non-native-tiler-host-interaction.md) stay linked.
- Architecture direction (user-approved 2026-09-24 from the
  [architecture review](research/architecture-review/review.md)): a portable
  `tiler-core` Engine with world/domain state behind a `LayoutPolicy` seam;
  `tiler-protocol` as a thin codec; a Linux service crate; the KWin script as
  observer and actuator; the native effect as renderer. Host-synchronous paths
  may stay in the adapter where moving them would change latency or failure
  behavior. Keep OS/DE-agnostic logic and policy in or near the Rust core
  wherever possible; supply needed capabilities through the smallest reliable
  native integration; no capability, including input, is excluded merely for
  being native. Existing specific implementation choices stand until changed.
- Observed-membership convergence (user decision 2026-09-25; steps 1-2
  shipped offline, live acceptance pending; evidence in
  [observation-convergence](changes/archive/observation-convergence.md)):
  each complete per-domain observation controls portable membership and
  floating state before ordinary Engine operations. One Session convergence
  preserves surviving topology, removes absent members, normally admits
  newcomers and adopts floating transitions; the requested operation then runs
  at the converged revision. No sequence, world index, fingerprint extension,
  tombstone, retention marker, or wholesale reseed is added. Only the existing
  wire `floating` and advisory `fit_excluded` are in scope; fullscreen and
  maximized members keep their tiled allocations as native overlays. Step-2
  relocation: a unique same-workspace source relocates only when the new
  observation shares a retained tiled or floating-exception id; disjoint and
  ambiguous sources seed fresh. Step 3 retired the former pending-pair
  fresh-domain delay and send/R4 verification protocol.
- Robust difference reconciliation (user decision 2026-09-28; phase 1
  offline-verified): KWin compares complete foreground and hidden observations
  against applied evidence through one event-driven classifier; no idle
  polling. The script retains its overlay slots, sticky multi-home exception,
  initial-fullscreen hold, and unreadable-domain fences. Existing Rust
  `reconcile`/`update-gaps` handles changes; the interim three-strike
  acceptance remains (user decision A, 2026-09-26, interim): after three
  bounded reassertions, reconciliation accepts
  each exact client-held rectangle as per-window applied geometry without
  disabling the domain. Phase 1 was user-accepted; Phase 2 is parked and
  resumes only on a genuine Ghostty/output client-held limit (see backlog).
  Offline
  evidence in
  [robust-difference-reconciliation](changes/archive/robust-difference-reconciliation.md).
- KWin retains applied per-window geometry/domain/flag evidence for overlays,
  first-admission maximize, drag fallback, drift and reply checks only, never
  as membership authority. Changed gaps use one bounded fresh same-domain
  `update-gaps` retry only after an exact correlated gap mismatch.
  Relocated but previously unconverged sources still require exact membership
  and atomic rollback. A stale pre-write snapshot replans the same command
  once against a fresh complete observation (user decision G, 2026-09-26); a
  second staleness logs, drops and converges; never replay after any setter
  has run. The retained allocation owns topology over same-scope client drift.
- Initial maximize (AR9, shipped): the effect seeds each observed window from
  committed `window()->maximizeMode()`; native transitions then update it.
- Effect Rust build (AR10, shipped): CMake invokes Cargo to build the
  workspace `tiler-kwin-effect-ffi` staticlib, using serde for strict JSON
  parsing and `tiler-core` validation gates. Rust keeps group visibility and
  drag verdict policy; the POD-only C ABI and panic containment remain.
- Size caps (AR16, shipped offline): the 64-window and 16-domain count caps
  are retired. The codec rejects requests above 1 MiB; the KWin adapter
  mirrors this bound before dispatch. Separate reply, native/FFI, and field
  bounds remain.
- Threat model (AR13, shipped offline): processes of the same user are
  trusted. The tray runs single-instance by owning its D-Bus name with
  `DoNotQueue` and accepts snapshots only from the current `org.kde.KWin`
  owner. The Planner same-UID caller check remains. Live login and watcher
  acceptance remain pending.
- Testing investment: build test fixtures that are sensible and valuable for
  the change at hand; avoid extensive custom harnesses that constrain later
  development.
- Size hints (AR12), workspace send (step 3), unified settings (AR15) and tray
  workspace behavior are decided under Placement, Engine operations below,
  and Settings above respectively.
- Rust owns the durable portable model (logical tiling, split-tree grouping,
  navigation, movement policy, capability-gated plans, reconciliation);
  platform adapters retain observation, identity, permissions, actuation,
  event ordering, acknowledgement, recovery, effects, UI, and delivery
  authority. The core promises no uniform workspace, group, atomicity, or
  geometry semantics where public platform APIs cannot provide them;
  unsupported paths fail closed. KWin direct geometry remains sequential and
  non-atomic; the adapter minimizes visible intermediate frames and records
  applied-versus-acknowledged divergence without claiming atomicity. The
  current KWin adapter uses one session-D-Bus `DescribePlan` route to a
  pinned unique Planner owner, with same-UID caller checks; this selects no
  generic cross-platform IPC abstraction. A KWin fork or patch remains
  rejected. Retain JavaScript for discrete window add/remove management.
  Group behavior, inactive borders, Steam-specific handling, and complete
  keyboard-layout support remain deferred.

## Visuals: Border, Underlay and Grouping

- Shared: no visual may show while maximized or fullscreen. Neither the border
  nor the group visual may be visible then.

- KDE active border:
  - The active-window border is an MVP requirement. Use an experimental,
    disabled-by-default, OpenGL-only native C++ KWin effect
    for the active-window border. Colour, width, outline radius, and gap are
    configurable; `UseThemeColor` in `Effect-omnitiler-active-border`
    defaults true, migration-free: enabled retains theme highlight with
    configured fallback, disabled selects configured colour unconditionally.
    The native QWidget KCM controls it through the existing hot-apply/repaint
    path.
- Nix delivery and exact host KWin ABI/session discovery are required for
  runtime delivery; neither is an optional enhancement. Runtime acceptance
  remains unproven.
- Approved 2026-09-21: hide the active-window border for fullscreen or any
  native maximize axis, matching the adapter's nonzero maximize collapse. The
  public KWin maximize transition signals update the border before and after
  geometry changes. The user manually accepted active-border suppression on
  2026-09-21 and additionally requires the group visual to hide
  while maximized: neither visual may be visible.
- User decision 2026-09-29: hide the native active border when the active
  window is an applet popup (`EffectWindow::isAppletPopup()`), restoring it on
  ordinary focus. This is a first step; tighten other window-type exclusions
  only if live use shows a need. KRunner is out of scope for now.
- Effect observation seeds every window on load and addition from its committed
  native maximize mode; any maximize axis or fullscreen suppresses both
  visuals, and native transition signals remain authoritative (applying user-approved AR9: an unacknowledged Wayland maximize
  configure still renders normal, so the committed normal seed reflects that
  geometry; acknowledgement emits the observed maximize signal. Requested mode
  is never guessed; no polling, timers, or geometry heuristics).
- Approved 2026-09-21: the Slice 1 drag oracle is folded into the surviving
  `omnitiler-active-border` effect plugin (one exported effect hosting
  active border, group overlay, and drag oracle); no second
  `omnitiler-drag-oracle` effect, factory, metadata, or KCM entry remains.
- The outline never clips, reshapes, or changes window textures. Plasma 6.5+
  decoration-driven rounded corners remain the selected corner solution.
- The active border retains one effect-owned automatic-lifetime
  `KWin::OutlinedBorderItem`, without texture changes or clipping. User decision
  2026-09-28: replace the temporary Meta-held group outline with a filled
  underlay beneath its windows, extending beyond the border outer edge by a
  configurable size defaulting to the current border width. Colour (including
  alpha) is configurable; the original `#40808080` default is superseded below.
  The drop-target preview colour also becomes configurable with alpha, retaining
  its `#402a82da` default. All new keys live in the existing effect group and
  hot-apply through effect reconfigure; existing keys/defaults are unchanged.

- User decision 2026-10-13: group underlay default is candidate C `#662a82da`
  (40% accent blue, Qt `#AARRGGBB`: alpha 0x66, RGB #2a82da) on KDE and
  Windows, for now. KDE kcfg/effect/KCM default delivered offline 2026-10-13;
  explicit saved colors are preserved, with no migration (confirmed User
  2026-10-14). Windows render/
  settings defaults and CLI/proof mirrors delivered offline 2026-10-13;
  explicit saved/CLI colors preserved, schema v1, no migration
  ([delivery](changes/archive/windows-underlay-default.md)). Before 0.1, user-owned
  live-check of black, white and grey wallpapers at 100%/150% remains pending.
  [Options](changes/archive/kde-dogfood-underlay-options.md).

- User decision 2026-10-11: active border outline radius defaults to 6 on
  all platforms (looked right in Windows dogfood); revisit per platform if
  it does not suit a host. Other border defaults unchanged.
  - Windows offline-delivered: settings and CLI/proof defaults, missing-field
    handling and Apply/Revert coverage; radius 6 and live Settings Apply
    physically user-accepted 2026-10-12, scoped normal owner, single output,
    Notepad/Paint, base `e506350` plus combined directional/retile/B9 deliveries;
    Revert/game/DPI/multi-output pending
    ([record](changes/archive/windows-border-radius-default.md)).
  - KDE offline-delivered: kcfg default 6.0, explicit radii preserved, no
    migration; live appearance pending.
- Windows active border (selected 2026-10-02): default-on owned per-pixel-alpha layered,
  click-through, nonactivating tool-window surface. Do not mutate foreign
  window attributes. `DWMWA_BORDER_COLOR` controls colour only and cannot
  provide KDE's configurable thickness/gap; the visible-frame thickness
  attribute is Get-only. Use `tiler-core::visual` policy, KDE defaults
  (width 3.0, gap 0.0; radius default changed below) and target-DPI rounding. The border follows
  eligible active windows independently of tiling/floating membership, within
  the adapter's scope/identity fences, with maximize/fullscreen/minimize,
  hidden-workspace and targeted shell suppression.
- Place the surface immediately below the fresh target in the target's
  topmost/normal band, matching KWin's target-parented outline at Z=-1.
  Reconcile actual visibility, geometry and z-order even when cached drawing
  inputs are unchanged. DWM shadows can tint the composed ring; do not claim
  that DIB RGB equals final screen RGB. Owned-surface creation, alpha drawing,
   placement, hiding and teardown also carry the Windows group underlay below.
- Theme mapping (selected 2026-10-08, Windows settings slice): KDE uses the active Selection
  background from `KColorScheme` (desktop highlight). Windows uses the official
  [`DwmGetColorizationColor`](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmgetcolorizationcolor)
  system colourization/accent analogue, with the core availability/positive-
   alpha gate and configured fallback. The owned window flags a
   requery on `WM_DWMCOLORIZATIONCOLORCHANGED` or `WM_SETTINGCHANGE`; repaint
  only if the resolved colour changes. Default theme/accent on with configured
   `#2a82da` fallback, per the user decision 2026-10-08.
  `--active-border-theme` selects accent;
   `--no-active-border-theme` selects configured colour.
   `--no-active-border` disables the surface.
- Scoped evidence passed; physical display/input, other DPI/output
  arrangements and topmost/style variants remain bounded follow-up checks.
  Evidence and limitations:
   `changes/archive/windows-active-border.md`.
- Group underlay (user decision 2026-10-02, accepted for now):
  `changes/group-underlay-move-trigger.md` on Windows. Show for Win+Shift hold
  (both required, either order, extra modifiers allowed) OR a matching focused
  native title-bar interactive move. Win alone and resize alone do not trigger;
  the chord independently works during resize. End/cancel/removal clears the
  move arm; held chord is independent. The trigger is hard-coded with shared
  portable policy in `tiler-core::visual`; KDE adopted A/B offline on
  2026-10-05, superseding its Meta-held trigger. Win+drag movement itself belongs to parity item 7, which can
  supply the same move arm later.
- Use an enabled-by-default filled, premultiplied-alpha, layered click-through
  nonactivating tool window on the active-border carrier. KDE and Windows
  defaults are `#662a82da`; explicit saved colors remain intact. Extension -1
  follows border width; explicit extension 0..32
  logical pixels matches the KDE settings range. CLI mirrors only enable,
  colour and extension: `--no-group-underlay`, `--group-underlay-color #aarrggbb`,
  `--group-underlay-extension N`. No foreign-window attributes are changed.
- Query the focused tiled window's immediate-parent group through Engine
  `ActiveGroup` and the shared resolver/description, using the actual eligible
  managed foreground. Render the union of Engine-projected source rectangles,
  expanded by gap, border width and resolved extension, even while the live
  dragged frame moves. Hide unavailable/root-leaf/floating, minimised, maximised
  and fullscreen subjects. Existing pending/drag-residue gates fail closed;
  Win32 interactive movement pauses reconciliation without adding core residue.
  Dogfood correction delivered offline 2026-10-11: ActiveGroup uses the same
  minimum-hint projector as ordinary plans; Windows carries retained advisory
  hints only, never native/client rectangles. Existing hintless callers and
  wire schema are preserved. Native nested-layout check pending
  ([record](changes/archive/windows-group-underlay-geometry.md)).
- Anchor beneath the lowest visible, nonminimised/noncloaked managed member and
  the visible active-border surface, matching KDE underlay Z=-2 versus border
  Z=-1. Recheck actual surface geometry/visibility/stacking at refresh even when
  cached inputs match; repair above-anchor displacement. Sample chord levels
  at the existing 100ms pump and wake on transitions, without a new input hook.
  Move/resize classification samples bounded `WM_NCHITTEST` at START delivery;
  timeout, unknown/custom-client hit zones and keyboard-only moves do not
  trigger the move arm. The chord remains available.
- C is parked under the authorized simplicity checkpoint: a distinct unfocused
  dragged subject needs new Engine resolution and invalidation lifetime.
  Never substitute that subject into `focused_window`, which persists retained
  focus. Native activation from an unfocused title bar remains a physical check;
  a future movement producer should reassess the need for C.
- Scoped synthetic evidence passed; physical feel, sustained chord across a
  complete move, fullscreen/custom-frame/topmost cases and other output/DPI
  arrangements remain follow-up checks. Evidence:
  `changes/archive/windows-group-underlay.md`.
- KDE group underlay trigger (A/B delivered offline 2026-10-05): hard-coded
  Meta+Shift hold (both required, either order, extra modifiers allowed) OR a
  matching focused-window native interactive move; this supersedes Meta-only
  lifetime and the timed-flash fallback. Meta alone and resize alone do not
  trigger. Shared `tiler-core::visual` policy is reached through the effect
  FFI; no new script/Planner route or settings. The effect captures the exact
  move window at Started and rechecks live focus plus the Rust-accepted group
  subject on every refresh; title-bar and native modifier drags use the same
  move arm, and finish, cancel and exact-window removal clear it without
  clearing the chord. Existing geometry, anchor, focus/domain, root-leaf,
  floating, fullscreen/maximize and endpoint suppression remain. Passive `EffectsHandler::mouseChanged(...)`
  modifier state is unknown before its first signal; only the chord branch
  waits. No polling, timed flash or new input hook. Native timing/pixels and
  multi-output behavior remain user-owned acceptance:
  [A/B evidence and checks](changes/group-underlay-move-trigger.md).
- Group highlight delivery: the active `OutlinedBorderItem` is a negative-z
  child of its target; the filled underlay parents its `ImageItem` below the
  lowest painted group member via public stacking order and re-anchors on
  stacking/membership/visibility changes. Rust resolves the focused leaf's
  immediate parent group and projected members; the script forwards validated
  IDs with engine union bounds through one additional `SetGroupHighlight`
  payload field (no raw ID logging; 64 KiB bound). The approved writable
  bridge is the effect-owned session D-Bus endpoint
  `com.omnitiler.ActiveBorder` (`SetGroupHighlight`/`ClearGroupHighlight`,
  not an `/Effects` method; offline contract verified, live unverified).
  The child inherits its window's slide translation; where the extension
  overlaps a non-group window stacked below the anchor it may paint over that
  window's edge (accepted); no custom scene/rendering mechanism is selected.
  Endpoint registration failure retries on window-activation and reconfigure
  events with no timer or polling; partial rollbacks only what was acquired.
- Grouping here means nested split-tree structure and placement. Tabs,
  stacked/shared groups, and compositor group behavior are excluded.
- User decision 2026-10-07 (R-GRP-03): tabbed stacks leave deferral and become
  the first item after the 0.1 release; closing the active tab keeps the group
  and activates the next tab (COSMIC, Hyprland, i3, sway). Until then, stacks
  stay refused. Grouped/tabbed windows remain deferred pending Rust-engine
  tabs with own evidence when designed. No tab or stack carrier, controls,
  or bindings are selected. (User decision 2026-10-09: Custom Tile
  stability-proof precondition removed; first item after 0.1 unchanged.)

## Pointer, Drag and Drop

- R-MOU-01: plain click focuses at press; hover alone does not change focus (User 2026-10-10, 0.1 triage D15).
- R-FLT-14: host-native pointer move/resize keeps floats free-framed; project resize continues to refuse floats (User 2026-10-10, 0.1 triage D21).
- Shared:
  - User direction 2026-10-07 (R-DRAG-07): drag presentation follows the host
    platform's native behavior where one exists. KDE Meta+drag keeps KWin's
    pointer-following frame; Windows title-bar drag stays native. macOS
    follows the same rule when it starts.
  - User decision 2026-10-07 (R-DRAG-08): a Meta/Win client drag focuses the
    dragged window at press on both platforms. Windows delivered and
    agent-observed 2026-10-11 ([record](changes/archive/windows-drag-press-focus.md));
    drop-only activation is retired. KDE timing needs a live check.
  - User decision 2026-10-07 (R-DRAG-04): Esc keeps cancelling a drag on both
    platforms (host KWin/Windows move convention; i3 also cancels) despite
    7/8 references dropping at the pointer; a setting may be explored later.
  - Gap-drag (user 2026-10-10): no anchor at gap 0; gap-drag exists only when
    gaps are above 0, with edge-drag covering gap 0. P3, post-0.1.
  - Tiled move drops use the existing Rust core drop resolver through the
    synchronous `drag-drop` Plan route; cross-output moves join the
    destination output's tiling at the drop point (no snap-back, no
    source-scoped restore fight).
- Windows:
  - Item 7 (delivered; history in
    [mouse record](changes/archive/windows-mouse-drag.md)): native title-bar
    drag and the project-driven Win+left stationary producer are both accepted,
    each its own committed unit. The project-driven gesture keeps the real
    window at its source allocation while tracking the pointer; release places
    through the shared Engine. The failed native SC_MOVE/non-client initiation
    experiments select no product behavior; no native-loop injection or
    floating-window modifier-move implementation is selected.
  - Item 8 target preview: separate owned filled layered surface above
    windows, click-through/nonactivating and absent from taskbar/Alt+Tab; KDE
    blue `#2A82DA` at alpha64. Native title movement and project Win movement
    share Engine DragPreview/DragDrop with fresh complete size hints and exact
    carried 32px/80px group-edge prior. Source token/domain/revision is frozen
    at START; cancellation, invalidation, self/centre/outside refusal, Finish
    and teardown clear the surface. Physical resize/float exclusion journeys
    remain user checks; deterministic policy/frame gates cover their no-preview
    rule. Evidence: `changes/archive/windows-mouse-drag.md`.
  - An unfocused project-drag mover activates at press through existing
    identity-gated focus authority; a failed press clears its arm with no
    geometry/Engine plan. Drop retains fresh focus verification and refusal,
    skipping the setter when already foreground. Same-output synthetic
    no-move/Esc/drop agent-observed 2026-10-11, base `9d12c7f` plus delivery
    commit ([record](changes/archive/windows-drag-press-focus.md)); native gates
    passed. Physical input/resize feel remain user-owned. Distinct
    unfocused-subject underlay C remains parked; completed pointer-drag resize
    of an inactive tiled window keeps focus unchanged.
    Tentative, pending user review: reuse existing bounded focus settle
    (up to 500ms per attempted press, callbacks remain prompt; observed 6-10ms),
    rather than introduce separate press actuation policy.
- Drag-restore markers (user decision D, option 2, 2026-09-26): on topology-change signals, a validated complete
  desktop-ID or output-name list proves a marker's workspace/output removed if
  absent; settle `outcome=unavailable plan=none` and drop it. A failed or
  malformed list proves nothing. No timer, count cap, inferred departure or
  fabricated plan.
- Nested placement affordance: portable drag placement source-classifies
  COSMIC group edges (32px normally, 80px only for the exact prior portable
  `(group, edge)` hover), group interiors and window zones. Same-axis group
  edges use first/last insertion, perpendicular edges wrap the group, group
  interiors insert after the source predecessor. Window left/right create
  horizontal before/after placement, top/bottom vertical before/after. The
  middle-third center is a stack drop refused closed (stacks unselected).
  Split-tree structure only, never tabs, stacks, shared tiles or compositor
  groups.
- Mid-drag workspace change (selected 2026-09-29): a native same-output
  workspace send while Meta drag is held supersedes the Started workspace's
  drop target. Ignore that stale drop and let complete source and destination
  observations reflow/admit the mover normally, comparing the mover's fresh
  native workspace with Started (a legitimate pointer-based cross-domain drag
  remains available). The refused drop's restore reconcile binds to the
  freshly observed destination containing the mover; the source still reflows
  through send settlement. Confirmed live by the user (2026-09-29);
  see [mid-drag-destination-recovery](changes/archive/mid-drag-destination-recovery.md).
- Interactive resize uses drop intent (user 2026-09-24): fallback
  grabbed-edge classification from pointer and starting frame; a matching
  native press in the later verdict takes precedence for KWin-thirds
  classification. Retile from the oracle's final window edge on each grabbed
  side; corner drags use both axes, ignoring other edge changes from
  rounding, size increments, or a self-resizing client. A cancelled verdict, or a moved verdict
  with no usable grabbed edge, no movement on grabbed edges, or lost
  identity, routes no resize. On adapter or Planner rejection, converge to the
  retained layout with one bounded, drag-correlated reconcile, without retry
  or loop. Keyboard resize and other operations retain their focus rules. The modifier-resize press is passively observed
  without grabbing or consuming input; a usable press selects KWin 6.7.5
  thirds regardless of the 64px interior gate, otherwise the Started-pointer
  classifier applies with a bounded fallback log. Without a matching press, a
  start well inside follows KWin's exact thirds (including center); frame-edge
  starts keep the nearest-edge and corner-zone rule.
  Read the public effective modifier-resize binding; use a logged default only
  when unavailable. The shipped route has no reliable KWin grabbed-edge signal.
- Tiled move drops use the core drop resolver (window edge split, group edge
  first/last or wrap, group interior insert; center or unresolved snaps back
  through the existing restore marker) via the synchronous `drag-drop`
  Plan route with Finish pointer capture and single-flight dispatch. A move to
  another output joins that output's tiling at the drop point (no snap-back,
  no restore fight); destination means the output under the pointer at
  Finish; verify membership via existing observation, no new arrival timer or
  retry. Preview details under item 8 above, including the carried 80px sticky
  group-edge hover prior. Evidence:
  `changes/archive/cross-output-drag-preview.md`.
- Oracle verdicts route as ruled above; a floating move stays native-only; a
  cancelled or no-change verdict makes no plan. No stock-KWin parity or atomic
  native geometry-write claim is selected; trace-only measurement remains for
  drag diagnosis.
- Drag oracle (KDE): the shipped C++/moc effect shim with POD-only C ABI
  hosts the verdict; Rust owns verdict policy with panic containment. The
  effect passively observes the configured modifier-resize press via
  InputEventSpy and carries it atomically with the final-geometry verdict
  through a separate read-only session D-Bus endpoint that the script pulls
  after drag finish. AR8 closed 2026-09-24 with the shipped integration kept;
  no endpoint rewrite is selected.

## KDE Support Floor

- User decision 2026-10-11: KDE 0.1 supports KWin 6.6+, including tiling,
  Settings and the native effect, so stock Ubuntu 26.04 LTS is supported.
  Plasma 5, Debian 13 (KWin 6.3.6), and KWin 6.4/6.5 remain unsupported.
- KWin 6.6 has one current desktop on every output. Per-output workspace
  modes are unavailable: saved/requested per-output modes resolve to shared
  with an honest diagnostic; migration refuses as unsupported. KWin 6.7+
  retains its per-output behavior. Detect scripting capability at the narrow
  workspace seam; native builds require KWin >= 6.6 and ECM >= 6.24.
- The same Debian source recipe covers Ubuntu 26.04 release/updates and
  Ubuntu 26.10 (KWin 6.7.5, release scheduled 2026-10-15). Core owns both
  Settings KCMs and does not exactly pin KWin. The effect companion depends
  on the exact compiled-against `libkwin6` distro build, including revision,
  and is rebuilt per KWin package update. Remove the companion if a matching
  rebuild is unavailable when upgrading KWin; core tiling/Settings remain.
  RPM/Arch's existing loose KWin dependencies remain unchanged.
- User decision 2026-10-13, option (b): keep the exact effect pin. Solver
  acceptance shows both `apt upgrade` and `full-upgrade` hold KWin back until
  the rebuilt effect is installed. Queue a short installation-guide note and
  a line in the effect package description explaining that behavior.
  [Solver evidence](changes/archive/ubuntu-apt-solver-acceptance.md).
- Future upgrade smoothness research: whether an effect built against one
  KWin patch release loads on the next (IID/version binding), OBS rebuild
  latency and pin options. No pin relaxation selected.
- [Offline delivery and pending live checks](changes/archive/kwin-6.6-support.md),
  [workspace discriminator](spec/reference-outcomes.md#kwin-66-shared-workspace-fallback),
  [installation](installation.md).

## Delivery and Installation

- Unified Settings page: see Settings above. The script KPackage still needs
  the host-built native KCM companion for Configure; the effect need not be
  enabled. Existing live border updates remain live.
- Release/install paths and targets are selected under
  [Scope and Platform Goals](#scope-and-platform-goals).
- Split packages (user 2026-10-10, Settings ownership 2026-10-11): core
  (KWin script, planner, tray, both Settings KCMs) is independent of an optional
  ABI-bound native-effect package. KWin updates must remain possible without
  the companion; Debian's exact-pin policy is selected above. When the effect
  mismatches, fails or is absent/removed, tiling
  continues and KWin stays stable, while borders/drag preview degrade until
  rebuild. Missing, failed and removed effect cases are must-pass 0.1 checks.
- OBS attempt constraints ([research](research/distribution-package-feasibility/obs.md)):
  the source archive's pre-built JS bundle removes the distro Node build
  requirement. Ubuntu's ECM/API floors are resolved by the KWin 6.6 delivery
  above. KDE neon still needs OBS provisioning, which is unverified.
- Next, the user creates the OBS
  account/project, GitHub-to-OBS token and AUR account. Packaging prep (spec,
  PKGBUILD, OBS service files) may proceed offline before then.
- Offline packaging uses
  `omnitiler` for core and `omnitiler-native-effect` for the
  effect only, with both KCMs in core and recipes under `packaging/`. Release
  archives include the tagged-SHA JS bundle and vendored Rust crates; distro
  builds consume them without Node or network access. User units install
  inertly, with the tray explicitly enabled by the user. RPM/Arch native
  dependencies do not pin the KWin package version; Debian pins the exact
  build as selected above. KWin's versioned factory IID rejects
  different-version effects before instantiation. SONAME changes and
  same-version distro ABI patches still need transaction/rebuild validation.
  [Preparation and evidence](changes/archive/release-0.1-offline-packaging.md).
- Settings ownership is resolved under [Settings, Tray and First-Run](#settings-tray-and-first-run):
  removing only the companion preserves Settings/Configure/Revert in core.
  Ubuntu's core and effect packages build offline; neon provisioning remains
  unresolved.
- Offline preparation does not complete OBS delivery: the guarded post-release
  tag webhook exists, but `trigger_services` cannot update the stable
  package's pinned source URL/checksum. Keep `OBS_TOKEN` unset until that
  automated handoff is resolved. The source-service template currently needs
  a manual source commit after release; it is not an approved replacement
  for tag-only automated delivery. No accounts or release tags were created.
- User review 2026-10-13: OBS release handoff remains OPEN; no route selected.
  It may come in a later version and may not block 0.1, but neither is decided.
  Research candidates include SCM bridge/git-backed recipes plus exact checksum,
  scoped-token triggers of OBS-side work, and Route 3, a manual scripted
  `osc commit` per release. [Research](research/distribution-package-feasibility/obs-release-handoff.md).
- Publishing prerequisite: tool-based dependency/license audit and vendored
  attribution inventory are delivered; third-party notice artifacts remain
  required before 0.1. First-party MIT licensing and the
  GPL-2.0-or-later effect exception are applied under [Licensing](#licensing);
  package declarations and first-party notices match that boundary.
- Nix-first current-host delivery is selected. The repository flake statically
  exports `packages.default`, `packages.kwin-script`,
  `packages.native-effect`, and `packages.tray`, plus
  `lib.mkKwinScript`, `lib.mkNativeEffect`, and `lib.mkTray`. It also exports
  default NixOS and Home Manager modules. The convenience
  `packages.native-effect` is built from this flake's pinned nixpkgs input, so
  an external consumer that directly uses it must make this repository's
  `nixpkgs` input follow the host nixpkgs. The `lib.mkNativeEffect` factory
  instead uses the caller's `pkgs` and matching `kdePackages.kwin.dev`; the
  NixOS module calls that factory with its caller `pkgs`, making factory/module
  consumption host-pkgs safe. This project does not inspect or change the
  external consumer repository. The current build baseline remains `devenv.nix`.
- The native effect is not a portable prebuilt binary; every build targets the
  Nix-managed Plasma/KWin package set used for that build.
- The NixOS module owns the system KPackage/native-effect packages and writes
  only `[Plugins] omnitiler-kwinEnabled=true` in its immutable global
  KWin profile. It does not enable the native border or mutate shortcuts. Home
  Manager owns user-session delivery: the optional tray systemd user unit
  running the immutable `omnitiler tray` and the on-demand Planner
  D-Bus/systemd activation metadata. Neither writes user `kwinrc` authority.
- Flake source filesets are explicit for the KWin script, native effect/KCM,
  and tray package; build trees, generated artifacts, and unrelated repository
  files are excluded.
- Development iteration uses the packaged baseline plus a namespaced,
  reversible, user-local dogfood override as the smallest selected boundary.
  It must not coexist with a Nix-managed copy of the same KWin plugin IDs, must
  preserve exact normal-path restoration, and must not mutate system or
  unrelated state.
- Dev native delivery uses an explicit one-time `just dev-native-setup` (and
  matching `just dev-native-remove`) for this checkout's
  `target/kwin-native-effect-stage` via the project-owned
  `plasma-workspace/env` script, with robust quoting and exact-content
  ownership only; no `kwinrc` writes and no durable receipt state. The dogfood
  effect path and this dev path share the exact
  `plasma-workspace/env/60-omnitiler-native-effect.sh` path and cannot
  coexist there. `just dev`
  preflights both effects before startup, transiently loads only
  invocation-owned effects with KWin owner guards (preserving preloaded ones
  in reverse-order teardown), and promises no hot reload; unload verification
  never proves the library is unmapped.
- Native development setup recovers checkout relocation automatically: a
  native build cache recording a different source or build directory is
  recreated, while matching caches remain incremental. Setup may replace a
  byte-exact generated env export for `target/kwin-native-effect-stage` only
  when its previous stage no longer exists. Another existing stage,
  unfamiliar content, symlinks, and non-regular env files remain refused.
  Cache cleanup refuses unsafe source overlap and symlinked build directories.
  Session delivery still requires logout/login after setup or a native rebuild.
- Approved 2026-09-20, durable for native development: the current-system
  host KWin derivation is the native development authority.
  `scripts/nix-host-kwin-build.sh` resolves
  `/run/current-system/sw/bin/kwin_wayland` to its exact derivation and
  matching `dev` output, explicitly realizes that exact output, and builds
  inside `nix develop <host-drv>` with explicit `/nix/store` Cargo and rustc
  injected and `-DKWin_DIR=<resolved-dev>/lib/cmake/KWin`;
  legacy pinned CMake dirs never drive or leak. `just build-native-effect`
  and dogfood `effect-install` route through that builder with
  identity-keyed build dirs and fail closed on missing provenance. The former
  shared `e554fab72f81915600f3f449b786fd9af40439a5` dev pin is only
  portable/interim compatibility, no longer native authority.

## Linux Planner Activation

- Approved 2026-09-09: Linux/KWin delivery packages the existing
  `omnitiler planner-service` as one session D-Bus service named
  `com.omnitiler.Planner`. Its immutable package installs the exact
  D-Bus descriptor with `SystemdService=omnitiler-planner.service`;
  Home Manager places that package in the user D-Bus discovery path and owns
  the matching user `Type=dbus` unit with exact `BusName` and immutable
  `ExecStart=<store>/bin/omnitiler planner-service`. It has no shell,
  autostart target, durable PID/receipt state, or second Planner process mode.
- `programs.omnitiler.planner.enable` defaults true when this Home
  Manager module is imported. The service remains inert until D-Bus activation,
  so the default has no idle Planner process cost. Set it false to omit both
  the descriptor package and user unit.
- The unit uses `Restart=no`: Planner name loss terminates the old Planner
  session and any in-flight KWin transaction, and cannot form a systemd
  restart/rebind loop. A subsequent idle command may request a fresh D-Bus
  activation. User-manager session teardown stops the service; D-Bus
  connection/name loss also ends the Planner without durable recovery state.
- KWin restart with a surviving Planner (user decision 2026-09-28, option A):
  keep the fixed script generation `plan-1` and retained topology for domains
  sharing any observed window ID. KWin 6.7.5 constructs fresh per-window
  `internalId` UUIDs after restart; a complete nonempty reconcile with no
  shared IDs fresh-adopts that domain instead of rebuilding in arbitrary ID
  order. An empty observation retains normal retirement/revision semantics;
  a mismatched owner or generation still rejects rather than replacing the
  retained session. Other domains and the Planner binding remain intact.
- Selected 2026-09-16: on CONFIRMED Planner loss, establish one bounded fresh
  Planner session automatically. This on-demand activation is distinct from a
  systemd restart loop. It starts from current eligible windows only, with no
  durable layout snapshot or journal and no inference of the old session's
  internal history. The old in-flight transaction remains terminal with
  old-generation replies rejected; this does not replay interrupted commands,
  recover uncertain native mutation, apply stale old-session replies, or infer
  commit for an unresolved workspace send. Normal Plan transport pins a unique owner via
  strict `NameHasOwner` plus one bounded `StartServiceByName(..., 0)`
  accepting only `PrimaryOwner`/`AlreadyOwner` then `GetNameOwner`; ambiguous
  terminals may run one bounded identity probe and only absence or a changed
  owner recovers. A failed recovery stays bounded without loops.
  If the Planner survives sleep, retain its current in-memory layouts rather
  than rebuild them.
- Selected Rust KWin commands first resolve the Planner name. An absent name
  makes one bounded `StartServiceByName(..., 0)` request, accepts only
  `PrimaryOwner` or `AlreadyOwner`, then resolves and pins one unique owner
  before any planner method. Activation, identity, owner, stale, loss, and
  timeout failures refuse the selected Rust route with no Legacy fallback; a
  pending plan never rebinds after a restarted owner. Session activation
  improves immutable delivery but public KWin scripting still cannot attest the
  initially resolved same-UID Planner binary.
- Planner caller authorization is a single fail-closed session-bus check:
  `org.freedesktop.DBus.GetConnectionUnixUser` for the caller's unique name
  must equal the Planner process UID. A malformed name, failed lookup, unknown
  UID, or differing UID rejects. Process executable/PID/start-tick/boot,
  systemd MainPID/parentage, wrapper-pair, cgroup, KWin-owner, and pre/post
  revalidation do not participate in caller authorization.
- On authorization failure, `DescribePlan` returns the fixed bounded in-band
  JSON rejection `outcome:"rejected", kind:"unauthorized"` so KWin records
  `result=rejected`.
- `DescribePlan` is the sole shipped automatic-tiling route. Its approved lifecycle
  includes ordered N-ary `cosmic_v1` admission/removal, same-output send,
  restored backing-desktop/numbered workspace routes, bounded reconciliation,
  and the selected initial first-startup fitting direction below. General
  historical-layout reconstruction remains unselected; complete observed
  membership is now incrementally adopted into retained domains as described
  under Engine Architecture and Convergence. Before launch, this and every other
  user-facing setting must apply live; that is a mandatory launch blocker, and
  the gap-only reload does not satisfy it.
- Approved 2026-09-16: when the first startup domain has no usable retained
  session, Rust may use a versioned, best-effort near-layout fitting heuristic
  to minimize unnecessary initial window movement (simple, comprehensible,
  deterministic; Rust retains structural inference, KWin TS native
  observation). The fitted rule is decided under Placement above. The same
  attempt is selected for a post-CONFIRMED-loss fresh session only, from
  CURRENT eligible windows. Fresh-loss fitting may change grouping and does
  not reconstruct the old topology. It selects no park/unmanaged fallback or
  broader activation lifecycle.

## Live KWin/Plasma Boundary

- User 2026-10-14: desktop ID/order/count changes and default tiling groups
  produced by OmniTiler's normal lifecycle are observations, not restoration
  failures. [Test procedure](changes/kde-dual-output-live.md#approved-test-procedure-2026-10-14).
- Reversible, project-scoped live host tests may run under the reviewed
  repository protocol. They must be namespaced, fail closed, and provide exact
  restoration; if exact restoration cannot be verified, stop and leave the
  residue for user action.
- The user grants standing authorization, until revoked, to read and mutate the
  existing KWin session for project-scoped testing. This authorization does
  not broaden the live/manual boundaries below or select material product,
  security, or architecture decisions. Every action remains bounded to exact
  identified project resources with a recorded baseline and exact restoration;
  no broad cleanup, window closure, system path, dotfile, NixOS, Home Manager,
  sudo, session-boundary, irreversible, unrelated-host action, or preserved
  residue handling is authorized. Stop on ownership, parser, diagnostic,
  baseline, source, or restoration ambiguity. This covers project builds;
  native-effect staging/removal; the project's `plasma-workspace/env` script
  and same-name legacy migration; KWin `/Effects` load/unload and read-only
  queries; KWin script install, enable, disable, and reconfigure; bounded
  `/Scripting` load/unload; project tray-helper lifecycle, session-D-Bus
  operations, and journal/status reads; and disposable project-owned Custom
  Tile tests when exact restoration is verified.
- Physical or manual observations and every logout, login, or new-session
  boundary require user action. No `sudo`, system-path mutation,
  external-dotfiles mutation, unrelated host mutation, irreversible cleanup,
  or ambiguous-residue deletion is authorized.
- A session boundary is a required evidence boundary for claims
  about session-delivered packages.
- Deleting or restoring preserved candidates, containers, or host artifacts
  needs explicit user authorization plus exact path and identity or hash
  verification.
- The Custom Tile acceptance harness is retired/archived (User decision
  2026-10-09), not next work. It strictly diagnoses KWin and KGlobalAccel ownership and
  fails closed on stale state, collisions, drift, or provenance ambiguity; it
  performs no lifecycle or mutation. Its rollback and journal contract is for a
  later authorized run only; these guards still apply if materially reopened.
- The inert checkout carrier does not change the `authoritative_ready` verdict:
  current public KWin APIs provide no direct evaluated-memory source proof for
  the checkout controller, so `authoritative_ready` remains false. No
  preflight readiness phase authorizes a Custom Tile lifecycle, live journey,
  or user physical or manual action on its own; `journey_ready` and
  `authoritative_ready` remain false until the applicable acceptance gates are
  established; carrier setup is limited to its bounded operational binding.
- The user subsequently authorized reversible project-scoped KWin, session
  D-Bus, and Rust Planner testing for Rust-authority diagnosis. Each attempt
  must pin exact source, owner, baseline, bounded resources, and restoration;
  it must not close windows, touch unrelated state, or inspect or alter
  preserved advisory, shadow, or nested residue. Production KWin is never
  killed. A production script may change only through a reviewed lifecycle that
  proves exact source binding and exact restoration; otherwise the route stops
  before mutation. This authorization does not broaden access to non-project
  resources.
- The unidentified prior `omnitiler-advisory-*` runtime-directory
  residue is preserved untouched. Do not search for, enumerate, inspect,
  identify heuristically, modify, or delete it. No stale POC2/POC3 harness
  or checkpoint retry is authorized; recovery requires explicit user
  authorization and exact identity or hash verification. After the resource-order
  correction, the standing authorization above resumes only for fresh bounded
  attempts that stop before resource creation or prove exact restoration with
  no new ambiguity. Procedures, preflight, and residue handling detail live in
  [Live KWin/Plasma Testing](live-kwin-testing.md); grants and prohibitions
  above stay authoritative here.

## Observability

- Implementation and review must include the evidence needed to diagnose
  behavior and failures across every component.
- Project processes and Rust-path IPC must emit bounded, structured,
  correlated lifecycle and terminal logs for requests, significant decisions,
  failures, recovery, and terminal outcomes to their existing visible KWin
  console, stdout, or stderr/journal sinks. Future troubleshooting checks
  those logs first.
- Carry trace/correlation IDs across components and services; distinguish
  dispatch vs acceptance vs completion vs uncertainty.
- User 2026-10-14: output-send diagnostics carry the originating operation's
  route, including late replies after a newer opposite-kind flight starts.
  [Evidence](changes/kde-dual-output-live.md#product-defect-and-offline-fix).
- Normal operation shows bounded summaries; opt-in trace covers high-volume
  detail with no frame/poll noise.
- User 2026-10-14: Windows trace outlier thresholds are 50 ms
  queue/transport/work and 10 ms dispatch, otherwise one summary per second
  while active. Windows
  reconcile `tick` records now emit only a deduplicated skipped count
  (`eae2934`; [trace-volume record](changes/archive/windows-trace-volume.md)).
- Redact captions, application content, secrets, raw environment, native
  identifiers, raw native IDs, and raw native D-Bus payloads, plus unbounded
  pointer steps. Logging failures never block or change behavior.
- The shared `omnitiler:route-diag` schema identifies component,
  direction or stage, correlation, authority generation, revision, event/action,
  and outcome within the redaction rule above.
- `just dev verbose` keeps lifecycle summaries; `just dev trace` opt-in enables
  redacted high-volume per-window, hook, and bounded structural request/reply
  detail, never raw native D-Bus payloads.
- Shortcut operations emit bounded structured diagnostics on
  `omnitiler.shortcut` (operation, stage, outcome, shortcut component/action
  IDs, key images, cleared count/writes). User 2026-10-12: conflict records
  include every holder ID and chord at normal info/warning levels, with counts,
  row indices and bounded chunks for long rows, so journal records reconstruct
  the full list. Friendly labels and user content remain excluded; this narrow
  shortcut-identity exception does not permit native window IDs or raw payloads.
  Query with
  `journalctl --user --no-pager -g "omnitiler.shortcut op="`.
  Operational warnings and info are enabled by default with debug-only
  records; logging never affects behavior.
- The fixed unauthorized reply omits caller-supplied correlation, preserving
  the same-UID caller authorization boundary and each adapter's strict
  correlation fence; distinct unauthorized observability is intentionally
  not selected.

## Activation Requests

- User decision 2026-10-12, amended 2026-10-13, D27 / REQ-ACT-01 / R-ACT-01: an
  unsolicited activation request from an existing window on a hidden workspace
  defaults to `Mark urgent only`: mark urgent without switching workspace or
  changing focus on platforms where OmniTiler owns the outcome (Windows).
  This reverses the tentative `Switch and focus` default:
  Hyprland, i3 and sway (all Tier 1) agree against COSMIC under the
  [reference tier rule](#reference-tiers).
- Windows: while a fullscreen/game window is focused, requests are urgency-only
  in both modes; its guard decision is unchanged. Explicit user selection
  (Alt+Tab/taskbar) is separate from
  unsolicited activation. INS-07 no-steal covers tiling admission into
  inactive workspaces only; it is not an existing-window activation rule.
- Windows verification 2026-10-14 at `f4b9f41`: fresh background PowerShell
  requests against owned helpers were natively denied on current and hidden
  workspaces, with unchanged focus/visibility and no tiler workspace switch.
  Shell flash notifications were observed, including hidden targets, but visible
  urgency was not proven. A previously input-thread-attached caller could
  activate a hidden target and trigger foreground workspace convergence; this
  is not clean background/helper self-request or physical selection evidence.
  No dedicated product request/urgency observer, marker or classifier exists.
  Default/guard unchanged; native-permitted/helper-originated requests, visible
  marking/clearing, fullscreen/game and explicit selection remain TBD
  ([delivery and options](changes/archive/windows-d27-activation-verification.md),
  [earlier assessment](changes/archive/windows-item22-verification.md)).
- User 2026-10-14 (5a): hidden-window urgency marks the workspace urgent
  in the planned workspace indicator, cleared on visit; designed within Windows
  workspace presentation research; until then native Windows behavior only.
- Tentative, pending user review (2026-10-14): request-versus-selection handling
  remains unselected; no Windows native-switch exception or new UI is adopted.
- Post-0.1 setting `Activation request` has two options: `Mark urgent only`
  (default; tooltip Hyprland, i3, sway) and `Switch and focus` (tooltip
  COSMIC, xmonad). The alternative qualifies via COSMIC (Tier 1) plus xmonad
  (Tier 2). Functional names/options; WM names only in tooltips. Retained in
  P2 `0.1 triage settings follow-up` alongside D09/D10.
  Review this setting's KDE applicability when it is designed; no KDE setting
  applicability decision is made here.
- User decision 2026-10-13, option 1: accept KWin native activation as a KDE
  host exception. Valid-token / FromTool / allowed-X11 requests for a
  hidden-desktop window switch desktop and focus natively; missing/stale-token
  requests are marked demand-attention by KWin. OmniTiler converges and never
  intercepts, reverts or suppresses KWin activation. Alternatives are complex
  hacks, a maintenance burden and add jank. This aligns with
  [D01 native activation ownership](#placement-minimums-and-startup-adoption).
- KDE D27 is verification-only, no longer BLOCKED: native valid-token switch
  convergence and stale-token mark-only without interference. Source trace is
  complete; native outcomes remain TBD
  ([trace](research/d27-kwin-activation-trace.md)).
- User decision 2026-10-13, KDE gaming guard: no OmniTiler guard; rely on KWin
  input-freshness gating (token serial / X11 user time). Requests during active
  play are expected to be mark-only; idle-game and pager/taskbar switches are
  native behavior, exactly as without the tiler. Must-pass live check with a
  real game covers active play and idle/paused play. Reopen only if real
  switch-aways during active play are observed; expectation is not evidence.
- [Approval](research/release-0.1-triage.md#user-decisions-2026-10-12),
  [requirement](spec/functional-spec.md#activation),
  [reference evidence](spec/reference-outcomes/activation.md#r-act-01-hidden-window-sends-an-unsolicited-activation-request).

## Window State: Float, Sticky, Maximize, Fullscreen

- User decision 2026-10-12, D22 / REQ-FLT-03 / R-FLT-03, option (a): when
  a tile is floated out, surviving siblings keep their relative proportions
  (e.g. 50/25/25, float the middle 25 -> approximately 67/33). No setting.
  [Tier rule](#reference-tiers) basis: COSMIC (observed, not source-proven),
  i3 and sway (Tier 1) agree; no evidenced alternative. Hyprland and Tier 2
  remain untraced for this outcome. Thin basis disclosed: i3/sway are one
  family, with separate COSMIC observation.
  Verification-only: removal already retains survivor share ratios
  ([revision-bound assessment](backlog.md)); native reflow live check remains
  pending. [Approval](research/release-0.1-triage.md#user-decisions-2026-10-12).
- User decision 2026-10-12, D23 / REQ-MNZ-01/02/03 / R-MNZ-01..03,
  option (a): minimizing a tile releases its allocation; siblings reflow;
  focus falls to the MRU window; store its old slot. Restore returns to that
  stored slot; OmniTiler writes no focus, leaving any restore focus to the
  host. Minimizing a workspace's sole window retains the workspace with focus
  none and a stored restore slot. No setting.
- D23 [tier basis](#reference-tiers): COSMIC (Tier 1, source-proven) default;
  qtile/awesome (Tier 2) also vacate allocation, awesome restores old slot.
  Hyprland/i3/sway have no counterpart or are untraced (non-voting); no
  evidenced alternative to allocation release. Native Ours outcomes remain TBD.
- User decision 2026-10-13, D23 restore option (b): best-effort rejoin replaces
  exact-tree-equality validity. Return B beside a former neighbor, preserving
  side, group and index with a share comparable to its old one. Fresh ordinary
  admission only when no former neighbors remain, the domain is gone or B was
  sent elsewhere. Resizes, new windows, other-sibling closes and swaps do not
  invalidate by themselves. Stored slots remain session-only, not persisted.
- D23 rework delivered offline 2026-10-13: COSMIC `3d55cba0` matches the
  selected best-effort approach (`src/shell/layout/tiling/mod.rs:343-349,
  458-545,1329-1346`). It stores parent-or-sibling/orientation/index/sizes,
  rejoins a surviving anchor and otherwise admits fresh, without tree equality.
  Shared core preserves the old group/index when possible, otherwise regroups
  beside a surviving former neighbor on the stored side/axis. Stored share
  fractions drive insertion while current survivor ratios are retained.
  Minimize/reflow/MRU, no-focus-write restore and sole-null-focus remain intact.
  Requested intervening-change, restart and fallback regressions pass;
  native journeys remain pending ([evidence](changes/archive/d23-rework.md)).
- User decision 2026-10-13, D23 close cleanup option (b): clear a minimized
  window's stored slot when it closes. Add a small protocol distinction between
  close and minimize, folded into the D23 rework. Delivered offline: additive
  default-empty `closed_windows` distinguishes explicit close from omission;
  valid convergence forgets reported slots before session retirement. KDE
  retains undelivered close reports, including hidden-domain closes. Native
  close/minimize ordering remains pending ([evidence](changes/archive/d23-rework.md)).
- Windows adapter delivered offline 2026-10-13
  ([record](changes/archive/windows-d23-restore-close.md)): retained-allocation
  divergence replaced with release/reflow/rejoin, no restore focus write,
  sole-null-focus and verified closed-slot cleanup, including inactive-domain
  fanout without geometry/focus writes. Native journeys remain in item 22.
  Source-slot lifetime after send elsewhere is unchanged shared behavior, with
  a TBD discriminator; no adapter-specific exception or cleanup policy added.
  [Approval](research/release-0.1-triage.md#user-decisions-2026-10-12).
- User decision 2026-10-12, D28 / REQ-MAX-09 fullscreen leg / REQ-MAX-09b /
  R-MAX-09, option (b): KDE fullscreen carry ships in 0.1. An explicit send
  (follow/stay, numbered/relative) moves a fullscreen window to the target
  workspace and it remains fullscreen there, matching maximized carry G-D2
  and approved Windows carry (handoff item 9). The user rejected the recommended
  refusal hold: sending a fullscreen window, e.g. a game, is an explicit user
  action and should be honoured. The refusal is replaced offline for both
  retained tiles and never-tiled born-fullscreen members; it is no longer a
  listed 0.1 known issue ([record](changes/archive/kde-fullscreen-activation-and-send.md)).
  [Approval](research/release-0.1-triage.md#user-decisions-2026-10-12).
- User clarification 2026-10-12, D28 born-fullscreen option (a): explicit
  numbered/relative follow/stay sends also carry never-tiled born-fullscreen
  members. The window remains fullscreen and slotless on the destination;
  preserve the initial hold, with neither source nor target reflow and no
  source admission before transfer. On its first fullscreen exit, apply the
  existing D5 fresh-admission rule on the destination workspace. Preserve the
  fullscreen flag and geometry protections; focus writes are limited to the
  established explicit follow/stay semantics. Shared exception-member send/focus
  support is selected below. This clarification does not alter Windows delivery claims.
- Deferred-transfer extension selected 2026-10-12: add a shared slotless-member
  operation with window/source/target identities and no leaf; extend shared
  exception-member send/focus handling without mutating either tree. KDE applies
  native membership movement under existing follow/stay semantics and all
  existing fences. This is an additive extension within the existing
  architecture. Existing tiled contracts remain intact; Windows changes are
  limited to compilation support for the added variant.
- Delivered offline: shared `MoveDeferred` carries the window/source/target
  without a leaf, preserving the held exception and both trees. KDE accepts
  empty affected geometry for this operation, verifies the captured mover and
  native arrival, and performs no geometry setters for deferred sends. Shared
  follow focus retains a resolvable target tiled MRU (or null for an empty
  target) as bookkeeping; native follow focuses the moved window, while stay
  uses the source MRU. Initial hold/send/destination hold/first-exit coverage
  uses one real Planner instance; native/game acceptance remains pending
  ([record](changes/archive/kde-fullscreen-activation-and-send.md)).
- D28 [tier basis](#reference-tiers), original triage tally: i3/sway (Tier 1)
  plus qtile/awesome (Tier 2) carry; bspwm is context. COSMIC/Hyprland were
  untraced in that tally; no evidenced alternative, no setting. Detailed
  [R-MAX-09 source evidence](spec/reference-outcomes/maximize-fullscreen.md#r-max-09-send-a-maximizedfullscreen-window-to-another-workspace)
  is retained separately from the approval tally and native KDE acceptance.
- D28 gaming interpretation: honour explicit user commands; OmniTiler must
  not make unsolicited writes to fullscreen windows. Preserve the fullscreen
  flag and avoid tiling geometry writes to the fullscreen window beyond what
  the native workspace move requires. Native KDE fullscreen-send acceptance,
  including a real game, is a must-pass live check. If native observation
  during implementation shows KWin cannot carry without dropping or corrupting
  fullscreen, stop and return to the user. Implementation gaps are closed
  offline; [native verification](backlog.md) remains pending, outcome TBD.
- R-SPC-01: transient dialogs float rather than tile; Windows owned-dialog exclusion remains an intentional divergence, with parent-focus/modal legs TBD (User 2026-10-10, 0.1 triage D02).
- R-FLT-13: ordinary floats hide while their workspace is not shown and retain their frame across the switch (User 2026-10-10, 0.1 triage D19).
- R-FLT-12: focusing an overlapping float raises it; no project lower verb for now (User 2026-10-10, 0.1 triage D20).
- R-CLOSE-04: closing a focused ordinary float leaves tiles untouched and refocuses the MRU survivor (User 2026-10-10, 0.1 triage D24).
- Approved 2026-09-16: background tiling is supported at startup and on window
  open or move for non-visible workspaces, without switching visibility or
  stealing focus. Existing floating, sticky, fullscreen, maximize, and
  configured-gap rules remain authoritative, with Rust retaining structural
  ownership and the native adapter retaining observation and actuation.
- Pointer resize adjusts shared split boundaries or ratios and reflows
  neighbouring tiles.
- Intentional floating is a session-local per-window state carried by the Rust
  Session outside its tile tree. The internal `toggle-float` request floats an
  eligible active normal window and removes it from the tree; the Session
  selects the durable retained placement for a window that has floated before
  (the centered 60% work-area rectangle is only the first-time fallback), and
  unfloat is fresh planner admission, never prior-leaf restoration. Unfloat
  carries the window's live frame rect so a user moved/resized float is
  retained across float/unfloat/float. Fullscreen and maximized targets refuse
  with `float-refused-fullscreen` and `float-refused-maximize`. User decision
  2026-09-28, option A: unfloat uses exactly the new-window admission
  placement rule (focused leaf's projected rect, otherwise domain bounds),
  with no remembered-origin slot; COSMIC's `toggle_floating_window` maps
  through `tiling_layer.map(window, focus_stack)` as for a new window, and
  sway's `container_set_floating` likewise re-tiles by ordinary placement.
  The controller registers `Meta+G` without changing Grid View's record.
  KGlobalAccel permits
  both active records but dispatches the lower serial holder, so startup emits
  `shortcut-dispatch-shadowed` until the user applies the exact reversible KCM
  override. Explicit normal and sticky float toggles retain the exact toggled
  window: the adapter never actuates Rust survivor-focus bookkeeping and
  performs at most one synchronous native focus retention with stale/failure
  safety and no timer, retry, desktop switch, or later-focus fighting.
  `Meta+Shift+G` toggles sticky floating: tiled members float first,
  then set `Window.onAllDesktops=true` without changing current visibility or
  focus. KWin represents all-desktops as an empty membership list; sticky-off
  assigns the current desktop, restores prior floating placement there, or for
  prior tiled members fresh-admits there rather than restoring an old slot. The
  native write has one exact-reference `desktopsChanged` echo fence, with no
  retry, timeout, fallback, or polling. Approved 2026-09-21: an eligible normal
  window already native-sticky with an empty desktop list and no same-runtime
  origin is adopted as a sticky float with prior-float semantics only; sticky-off
  leaves it a normal float on the then-current desktop with preserved geometry
  and focus, and a later ordinary float toggle tiles it. Known tiled origins
  keep fresh admission and known float origins stay float.
  User decision 2026-09-25, option A: `Meta+G` on a sticky window uses the
  existing sticky-off path to clear all-desktops, restores the project's
  keep-above state, and returns to tiling regardless of whether the sticky
  window was previously tiled, an ordinary float, or adopted with unknown
  origin. `Meta+Shift+G` sticky-off still honors the previous floating origin
  as described above.
  Clarification applying user option A (2026-09-25): `Meta+G` tiles on
  the workspace where it is pressed, where native sticky-off leaves the
  window, even if its former tile belonged to another workspace. A subsequent
  `Meta+G` on a plain floating window must still tile it; moving or resizing
  a float alone never tiles it.
- User decision 2026-10-05: float-origin directional focus and move follow
  COSMIC. KDE and Windows deliver focus plus the first half-snap step offline;
  native actuation acceptance remains separate. Tile-origin navigation continues
  to skip floats. Windows delivery 2026-10-12 from base `e506350`:
  [record](changes/archive/windows-floating-directional-navigation.md).
- KDE float-origin focus searches only ordinary/sticky floats on the current
  output/workspace, by top-left position on the requested axis, ignoring the
  perpendicular coordinate. Up/Left include equal positions and select the
  first nearest tie; Down/Right require positive movement and select the last
  nearest tie. Sticky candidates precede ordinary floats, each in KWin native
  encounter order; equal-distance outcomes can differ from COSMIC Space order.
- On a local miss, reuse our existing edge behavior: no workspace cycling;
  Up/Down retain focus; Left/Right try reciprocal horizontal adjacent output
  remembered eligible tiled focus, otherwise retain. This deliberately differs
  from COSMIC's configured workspace-axis then output navigation. A float-only
  source can cross. Local tiles are never floating-search targets.
- Local selection and explicit native half-snaps stay host-synchronous in the
  KWin adapter. Rust keeps remembered cross-output focus authority through the
  minimal internal `focus.float_subject` flag, leafless `from_leaf: null`, and
  `focused-floating-window` precondition; tiled wire replies stay unchanged.
- Meta+Shift+arrow snaps an ordinary/sticky float to that work-area half,
  retaining floating/sticky membership and focus. Match COSMIC's integer
  `relative_geometry` formula: use the configured inner gap on outer edges and
  between halves, ignoring the separate outer gap. Every arrow requests its
  half again; quarter/maximize/outward workspace-output transfer transitions
  are deferred because they need per-window snap state and transfer integration.
  Fullscreen/maximized, interactive resize and incompatible declared-size
  constraints refuse geometry writes. One explicit write, no reassertion.
- Offline regression gates pass; physical acceptance remains user-owned.
  Evidence and next checks:
  [KDE floating navigation](changes/archive/kde-floating-directional-navigation.md).
  [Windows floating navigation](changes/archive/windows-floating-directional-navigation.md)
  passes native MSVC offline gates; Windows local misses retain until the
  existing cross-output actuation gap is delivered under handoff item 5.
- User 2026-10-14 approved (current observed-before-retained order kept):
  Windows float candidate encounter
  order is eligible observations followed by retained maximized rows, with
  sticky-first layering over the combined list. Equal-axis ties between the
  two sets can differ from host enumeration order. Visible sticky candidates
   participate across home workspaces on the same output; automatic fixed floats
   retain the selected D8 no-touch rule.
  Reference tally: 0 explicit observed-first outcomes, 2 applicable but
  incomparable live-list mechanisms (COSMIC/Hyprland), 7 unavailable profiles.
  Distance still wins across classes/layers; only equal-axis ties use this order.
  Deeper comparable tie-order research is closed with the P2 item; revisit only
  if dogfooding shows a wrong pick
  ([basis](changes/archive/windows-floating-directional-navigation.md#user-accepted-physical-evidence---2026-10-12)).

- Intentional normal and sticky floating request KWin's public `keepAbove=true`.
  KWin owns the normal keep-above/keep-below exclusive transition. The adapter
  records the prior pair and restores a project-cleared `keepBelow` (or prior
  `keepAbove=false`) before unfloat, fresh tiled admission, or controller
  disable; a pre-existing keep-above setting remains untouched. A missing,
  refused, or unverifiable native write fails the transition without a retry.
- Maximize (`Meta+M`) is workspace-local. Fullscreen (`Meta+F11`) is separate:
  the focused observed window toggles KWin's public `Window.fullScreen`
  property, while KWin keeps cover-and-restore ownership. The member retains
  its tree allocation, receives no geometry write while fullscreen, and
  restores that allocation on exit. User decision option 2 (2026-09-26): a
  window first observed by the KWin adapter as fullscreen instead stays out
  of the tree while initially fullscreen, so siblings occupy all tile space.
  Its first non-fullscreen observation receives normal fresh admission; any
  later fullscreen retains its tile as above. This includes startup and hidden
  domains. KWin carries the initial hold as a planner-only floating exception,
  never intentional native floating. Borderless
  output-sized windows are not inferred to be fullscreen.
- Maximize isolation mirrors fullscreen, authorized 2026-09-14. A nonzero KWin
  `maximizeMode` (1 vertical, 2 horizontal, 3 full) is collapsed to one adapter
  boolean; no horizontal or vertical maximize concept enters the Rust engine or
  its protocol, and no Rust protocol/session state is added for maximize. A
  maximized member keeps its observed identity, leaf/tree position, and share,
  receives no geometry write, and on unmaximize is restored to its exact
  retained allocation. Fullscreen takes precedence when a window is both
  fullscreen and maximized: fullscreen refusal tokens and the `skip-fullscreen`
  disposition win over maximize. User decision I (2026-09-26): a normal window
  without a connectable `maximizedChanged` remains tiled; log once and use
  fresh `maximizeMode` reads on later observations. An unseen native change
  remains unknown until a subsequent observation.
- `Meta+M` toggles KWin maximize through `Window.setMaximize(bool, bool)`, never
  `maximizeMode`. Fullscreen refuses first. Post-admission maximize retains its
  tile slot and skips geometry writes; unmaximize restores its retained
  allocation. The action has its own exact-reference `maximizedChanged` fence,
  so a deliberate post-admission maximize is never affected by admission-time
  clearing. Current read-only enumeration found `kwin/KrohnkiteMonocleLayout`
  on `Meta+M`; registration preserves that record and emits the shadowed-
  delivery diagnostic until the user applies the exact reversible KCM override.
- KDE toggle activation (user 2026-10-05, delivered offline): each explicit
  maximize/sticky activation makes at most one native toggle attempt; no
  persistent attempted-state map or automatic retry. Exact-reference echo
  fences, refusal gates and focus retention remain. Only the held
  born-fullscreen exit clear remains. Physical held-key/autorepeat
  delivery through KGlobalAccel remains user-owned acceptance. Evidence:
  [KDE follow-up](changes/archive/kde-post-windows-followups.md).
- Maximized directional operations (G-06, User 2026-10-10; REQ-MAX-08
  NORMATIVE, resolves review G-D1): both platforms follow COSMIC.
  Directional focus is fenced while the focused window is maximized (no-op);
  leave via unmaximize or Alt+Tab. Directional move of a maximized window
  unmaximizes it first, then moves. Existing maximized-subject pointer
  refusals remain; this decision changes directional commands only.
  KDE move makes one native clear attempt with the existing exact-reference
  echo fence. Only observed clear continues the ordinary move in that
  invocation (including eligible float half-snaps); there is no delayed move
  or automatic retry. Reobserve directional topology and validate the same
  reference, focus and source domain; failed/unconfirmed clear or a race
  logs a narrow refusal without a structural move. A later press can retry.
  Fullscreen precedence is unchanged. KDE's maximized flag remains local
  to preserve born-maximized admission; the adapter enforces its focus fence.
  The shared Engine focus fence is opt-in for direct flag observers, default
  off for Windows because its reconcile observation omits maximize; the native
  directional guard reads fresh retained flags, preserving admission semantics.
  Status: shared-core/KDE delivered offline ([record](changes/archive/reference-comparison-implementation.md)); Windows ordinary focus fence/clear-then-move/float-four-halves physically user-accepted 2026-10-12, scoped normal owner, single-output Notepad/Paint, physical keyboard/mouse, base `e506350` plus combined directional/retile/B9 deliveries ([record](changes/archive/windows-maximized-directional-navigation.md)); failure/race/preset/fixed/game/DPI/multi-output pending.
- Tentative 2026-10-12, pending user review: Windows pre-clear diagnostics
  prioritize `fullscreen-raced`, `domain-raced`, `focus-raced`, `suspended`,
  then `elevated-foreground`. Existing project fullscreen metadata also vetoes
  clear when a transitional frame no longer covers the output, preserving
  owned restore state. This is a narrow refusal, without delayed move/retry.
- User 2026-10-12: a window leaving fullscreen, including borderless windowed
  fullscreen, is admitted/tiled like a newly admitted window; no target writes
  while fullscreen. Startup-fullscreen and born-after-start first exits share
  D5 current-hint admission; fixed clients remain automatic floats, intentional
  floats/sticky preserve their selected identity, floating workspaces defer
  tiling. Previously tiled fullscreen members return to their retained slot.
  Windows lifecycle/owned-exit regression coverage is
  [recorded](changes/archive/windows-fullscreen-exit-acceptance.md).
- User decision 2026-10-12: automatically clear stale owned fullscreen
  metadata when fresh observation proves the window left fullscreen by other
  means, so explicit maximized move/unfloat is not stranded. Non-covering
  transition geometry alone is not exit proof; fullscreen/game safety wins.
  Windows retires only after the originally cleared nonzero chrome mask reads
  restored and fresh fullscreen reads false, with exact-lifetime/preimage
  revalidation and a second proof immediately before metadata removal.
  Reconcile and both maximize-clear routes share this metadata-only sequence.
  Fullscreen, transition, unreadable, changed-preimage and identity-race cases
  keep the veto; genuine re-fullscreen remains protected. No geometry write is
  added. Owned toggle exit still removes metadata after verified restore.
  User follow-up 2026-10-12: originally captionless zero-bit preimages stay
  fail-closed without a restoration witness; no automatic retirement inferred
  from time or non-coverage. Win+F11 owned toggle exit remains the release path.
  The user will watch for windows failing to tile during dogfood. This is the
  selected policy, not a remaining exit-proof decision.
  [Delivery and evidence](changes/archive/windows-fullscreen-exit-acceptance.md).
- User-accepted physical 2026-10-12: scoped single-output Notepad/Paint,
  base `e506350` plus directional/retile/B9 and fullscreen-exit regression
  changes before automatic-retirement delivery. Win+F11 owned fullscreen on
  tiled Notepad then exit returned it to its slot; stop/relaunch with Notepad
  still fullscreen then exit tiled it. Each Win+F11 also raised the known
  Xbox full screen experience prompt; containment remains parked under
  Windows gaming coexistence. This does not accept the later retirement hunks.
- Maximized workspace sends (G-D2, User 2026-10-10; REQ-MAX-09 maximize
  leg): sending a maximized window to another workspace keeps it maximized
  on arrival, cross-platform. Deliberate COSMIC/niri deviation (they
  unmaximize): a send relocates the whole window so its state travels,
  unlike an in-layout move; Hyprland/qtile/awesome agree. Windows fullscreen
  carry selected in Table A 2026-10-07 remains; KDE fullscreen carry is now
  selected by D28 (b) User 2026-10-12. Whole-workspace output migration D8
  remains its own carried-overlay rule.
  KDE allows flag-stable maximized tiled sends through native membership/
  output transfer, without unmaximize/remaximize or overlay geometry writes.
  Existing arrival/current-view/lifetime/follow/stay fences still apply.
  Status: KDE maximize carry delivered offline ([record](changes/archive/reference-comparison-implementation.md)); KDE native arrival preservation pending. Windows same-output numbered/relative follow/stay, source MRU and native target unmaximize agent-observed 2026-10-11 ([record](changes/archive/windows-workspace-send-follow-stay.md)); physical/cross-output pending. KDE fullscreen carry needs code (currently refuses fullscreen sends); D28's native journey including a real game is must-pass.
- Windows fullscreen workspace carry R-MAX-09 delivered 2026-10-11,
  base `6b76589` plus delivery commit ([record](changes/archive/windows-fullscreen-workspace-carry.md)).
  Same-output numbered follow/stay and relative follow, source reflow/MRU,
  hidden target and project exit to target allocation agent-observed; native
  gates passed. No restore before send or mover geometry writes.
  User 2026-10-14: explicit numbered/relative workspace FOLLOW sends and
  output FOLLOW sends focus the fullscreen mover after verified arrival,
  like other follow sends; STAY is unchanged. Delivered offline with
  mover-focus and no-mover-focus stay regressions; user physical recheck
  passed 2026-10-14.
  This supersedes the Table A item 9 fullscreen-follow focus suppression;
  explicit commands are honoured, with no unsolicited-event focus changes.
  Managed app-owned fullscreen with a retained tile uses the same stable-flag carry
  fences, while R-MAX-05 toggle refusal remains. Unreadable fullscreen flags
  defer before effects; slotless born-fullscreen stays ineligible.
  Test-needed `workspace --fullscreen` reuses exact-owner consume-once CLI
  transport and toggle authority (User 2026-10-14 approved as delivered).
  Physical shortcuts/native flag races/app-owned games/cross-output pending.
- KDE borderless-windowed fullscreen heuristic (born-fullscreen option 3,
  user 2026-10-10): stays conditional. The user watches for borderless games
  during dogfooding; revisit only if games arrive non-fullscreen. This is a
  watched risk, not a pending default selection; KDE remains flag-based.
  [research](changes/archive/born-fullscreen-admission.md)
- Windows (parity items 3-6):
  - Maximize (Win+M matches Meta+M): retained-tile overlay; siblings keep
    layout; never suspends the workspace. Directional focus is a no-op while
    the focused window is maximized; directional move unmaximizes first,
    then moves (shared rule above). Pointer operations still refuse a
    maximized subject; Win+Arrow stays focus, never Snap. Send/follow,
    select-away hiding and return preserve maximize. Born admission follows
    `core.born_maximized`; floating-to-tiled admission keeps maximize over a
    reserved slot independently of that setting (item 11 delivered offline
    2026-10-12). Retained slots exempt; fullscreen wins. Stop/crash preserve
    frames and maximize state; identity-safe recovery reveals hidden members.
    One native toggle attempt per discrete Win+M down (the discrete rule also
    fixes KDE's map-refusal case); held repeats consumed without dispatch
    (User 2026-10-14 approved as delivered).
    Status: scoped synthetic proof passed; native system-command and
    double-click paths proven in scoped runs; physical input and other output/DPI
    arrangements remain user-owned
    ([maximise](changes/archive/windows-maximise.md)).
    Status: G-06 focus fence/unmaximize-before-move delivered offline 2026-10-12
    ([record](changes/archive/windows-maximized-directional-navigation.md)); ordinary
    physical single-output acceptance 2026-10-12, variants/game/DPI pending;
    maximized-send preservation remains the existing Windows policy.
  - Fullscreen (Win+F11 matches Meta+F11): managed fullscreen retains
    membership/tree/shares, pauses geometry writes, restores the current
    Engine allocation on exit; precedes maximize; suppresses border and
    underlay. Unmanaged fullscreen foreground suspends the workspace.
    Directional movement, pointer operations and maximize refuse a fullscreen
    subject; Windows same-output single-window fullscreen workspace carry is
    selected with implementation pending (current code refuses: gap).
    First-seen fullscreen without a slot stays a slotless floating exception
    until first native exit. Project toggle uses official Win32 style/frame
    APIs with inert preimage properties; a new owner applies the first-seen
    hold before any later explicit exit. App-owned fullscreen without that
    preimage refuses (user 2026-10-07 R-MAX-05; a later spike explores
    retaining state early). No synthesized app F11, no guessed restore. One
    attempt per discrete down; held repeats consumed (User 2026-10-14 approved
    as delivered). Failed effects retain any
    usable preimage for a later explicit press. Captionless
    full-monitor windows classify as fullscreen on Windows for borderless-game
    compatibility while KDE stays flag-based (deliberate difference, user
    2026-10-07 R-MAX-07). Cloaked foreground reads exclude invisible covers
    from the veto; invalid/unreadable foreground facts fail closed. Never
    blanket-exclude ApplicationFrameWindow or Explorer from foreground safety
    on an unidentified shell surface. Status:
    scoped synthetic proof passed; shell-takeover purpose unidentified;
    physical input/display and other arrangements remain user-owned
    ([fullscreen](changes/archive/windows-fullscreen.md)).
  - Float (Win+G matches Meta+G): intentional floats leave the Engine tree
    and siblings reflow; first float uses the centered 60% fallback,
    later floats reuse retained geometry. Unfloat carries the live frame with
    ordinary admission placement, never a remembered slot. Directional
    focus searches ordinary/sticky floats, directional move requests stateless
    half-snaps (delivered offline 2026-10-12); tiled pointer operations refuse a
    floating subject; native
    moving and resizing remain free and never implicitly unfloat; send refuses
    a focused float while selection hide/reveal keeps geometry intact.
    Explicit ordinary maximized unfloat clears once, observes clear, then
    fresh-admits (B9 delivered offline 2026-10-12); unconfirmed clears retain
    float intent for a later press. Fullscreen, sticky maximize and
    normal-to-float maximize targets refuse. Topmost is the keep-above analogue
    with no keep-below; record the original band, verify effects, restore only
    a project-raised band, never clear a pre-existing band, and preserve band
    preimages even without later readback. Evaluate on a local Engine clone
    and commit after target effects verify; failed effects never strand
    membership. Born-fullscreen holds stay distinct from intentional float
    actuation. One attempt per discrete down (User 2026-10-14 approved as
    delivered). B9 selected behavior
    (unmaximize-then-fresh-admit) is delivered offline on KDE and Windows
    ([Windows record](changes/archive/windows-maximized-intentional-unfloat.md));
    native clear/frame/focus timing remains pending.
    Status: scoped evidence proves activation and cleanup only; float
    behavior, physical input/display and other arrangements remain user-owned
    ([float](changes/archive/windows-float.md)).
  - Sticky float (Win+Shift+G matches Meta+Shift+G): tiled-origin floats
    first then sets visibility; float-origin preserves its frame. Sticky
    floats stay visible across every managed workspace of their output. Same-
    runtime sticky-off (tiled origin fresh-admits, float origin stays float);
    Win+G on either sticky origin clears sticky and tiles. Markers are
    window-lifetime properties; stop/crash leaves them for next-owner
    normal-float adoption (KDE native-sticky adoption unchanged). Selected
    direction (user 2026-10-07 R-FLT-05, 2026-10-08 R-RST-01): intentional and
    sticky floats persist across owner restart; Windows still implements the
    old runtime-local reset pending its handoff. Status: scoped helper
    evidence passed; crash/watcher, physical input/display and other checks
    remain user-owned
    ([sticky](changes/archive/windows-sticky-float.md)).
  - Stop/crash: hidden windows are revealed without geometry recovery (the old
    tiling-only preview left geometry in place). Terminal is ordinary,
    matching KDE.
- H/V maximize is deliberately not modeled in the engine. Maximize is a
  recorded overlay state over the original layer, not a distinct topology or
  managed layer, so the engine carries no horizontal/vertical maximize concept.
- Q3 (user 2026-10-05, scope 2026-10-07): a first-seen (born) maximized
  window tiles with a reserved slot and keeps its maximize as an overlay; no
  launch unmaximize, on KDE and Windows. R-MAX-03 floating-to-tiled admission
  keeps maximize over a reserved slot with no one-shot restore. Native
  launch/restore and session-restore no-loop acceptance remain user-owned.
  Only a non-fixed held born-fullscreen exit clears once under the exact-ref
  hold. Fixed first exits float with no writes (D5 delivered offline).
  D6 workspace enable leaves fixed automatic clients floating, including
  maximized clients; explicit user tile overrides retain the Q3 slot route.
  - KDE: R-MAX-06 and R-MAX-03 delivered offline, the latter at `29c75fe` ([record](changes/archive/kde-maximized-floating-retile-overlay.md)). Windows R-MAX-03 ordinary retile/restore physically user-accepted 2026-10-12 under the G-06 conditions above ([record](changes/archive/windows-maximized-floating-retile-overlay.md)); hidden/session/fixed/game/DPI/multi-output pending. Tray previous-focus non-restoration is observation only, possibly suitable for keyboard accessibility; no behavior change requested.
    Tentative 2026-10-12, pending user review: separate lifetime-bound born and
    retile overlay latches keep born-setting adoption independent of retile.
  - CHANGED by user 2026-10-11 (R-MAX-06 born-maximized setting, option B,
    cross-platform): first-seen maximized admission becomes a setting.
    Default tiles: one launch unmaximize, then ordinary admission (tooltip
    names bspwm, i3, xmonad, sway, qtile). Alternative keeps the Q3
    reserved-slot overlay (tooltip names COSMIC, Hyprland). Functional
    names per Functional naming. IDs: `core.born_maximized`, default
    `tile-on-launch` ("Tile on launch"), alternative `keep-maximized-overlay`
    ("Keep maximized overlay"); reference WM names live in tooltips. Reason:
    a born-maximized launch (e.g. browser session restore) felt janky in
    Windows dogfood. Fullscreen (exclusive/borderless) stays untouched; game
    risk of a game launching in maximized-window mode is checked in the
    user's gaming test. R-MAX-03 floating-to-tiled overlay, D4 fixed
    born-maximized float and later user maximizes are unchanged.
    - Windows offline-delivered: both values, Settings Apply/Revert and
      lifetime-bound subsequent-admission adoption; both-values maximized Paint
      launch physically user-accepted 2026-10-12 under G-06's conditions;
      browser/session/fixed/game/DPI/multi-output acceptance pending
      ([record](changes/archive/windows-born-maximized-setting.md)).
      - User 2026-10-14: `CoreSettings::born_maximized_mode()` accessor accepted.
      R-MAX-03 parity delivered offline with item 11; fixed-client detection
      delivered offline with item 13.
      - KDE offline-delivered: setting + KCM + adapter subsequent-only
      adoption; R-MAX-03 independence preserved; live checks pending.
- User decision 2026-10-13: lift born-maximized policy into the shared core
  now, following [Layering](principles.md#layering) to avoid duplicate copies
  and drift. Rewire KDE with identical behavior and labels; `bornMaximized`
  is the accepted KDE key name. Windows handoff replaces its adapter copy
  with the shared policy after shared delivery.
  - Amended by user 2026-10-13, option C: core owns the two-value policy and
    default handling now; KDE keeps its existing TypeScript copy, pinned by
    one conformance case table consumed by Rust core and KWin tests. This
    supersedes "rewire KDE now": defer KDE rewiring to the post-0.1 Push code
    down pass. Its clear/keep decision is synchronous pre-dispatch and the
    planner protocol lacks that decision; runtime protocol changes and
    build-time generation were declined. KDE behavior, labels/tooltips,
    `bornMaximized` key and subsequent-admissions-only adoption stay unchanged.
    Windows replacement remains handoff item 23: Rust can call core directly,
    with the same table as acceptance and existing host gates preserved.
    [Core policy and conformance delivery](changes/archive/born-maximized-shared-policy.md).
    Windows handoff item 23 delivered offline 2026-10-13: core-backed
    settings/default/validation, admission predicates and radio read; shared
    fixture conformance and native MSVC gates pass. Behavior/schema and
    subsequent-admission semantics, including the existing tentative D7
    hydrated fixed-override maximize route, remain intact.
    [Windows delivery](changes/archive/windows-born-maximized-shared-policy.md).
- B9 (user decision 2026-10-08): an explicit unfloat of an intentionally
  floating window that is natively maximized unmaximizes then fresh-admits
  (COSMIC). No retained-maximize unfloat. The user's COSMIC R-FLT-06
  observation confirmation
  remains (not an implementation gate).
  - Status: shared-core regression/KDE delivered offline ([record](changes/archive/maximized-intentional-unfloat.md)); Windows item 15 ordinary scoped Notepad/Paint clear/frame/focus agent-observed 2026-10-12 ([record](changes/archive/windows-maximized-intentional-unfloat.md)); physical B9/fixed marker/restart/failure/game/DPI/multi-output and KDE native pending.
    Windows reuses D3/D7 fixed override settlement after observed clear and
    fresh admission; no marker/intent commit on an unconfirmed clear.
    Tentative 2026-10-12, pending user review: `float-maximize-clear` and
    `float-refused-maximize-clear` diagnostics carry the clear result/cause,
    matching directional clear diagnostics. Owned fullscreen metadata vetoes
    B9 both before and after clear, including non-covering transition frames.
  Analysis: [post-Windows audit](research/cross-platform-core/post-windows-audit.md),
  [cross-WM consensus](research/reference-wm-consensus.md).
- Observation-driven reconciliation, three-strike anti-fighting acceptance,
  send/R4 convergence and stale-snapshot replan rules: see Engine Architecture
  and Convergence above.
- Permissive admission, authorized 2026-09-14: an observed normal window's
  incoming frame rectangle never decides whether it may join a tiled domain.
  Admission assigns a new complete geometry for every member and may reflow
  existing members. In particular, an older out-of-work-area window must not
  prevent a later window from tiling or create a restart-persistent admission
  deadlock. User Resilience direction, 2026-09-29: valid observed frame rectangles
  outside the work area are host drift for retained commands, directional
  commands, and workspace-send target observations too; they must not reject
  the complete snapshot. Canonical projection and KWin's existing bounded
  per-window drift handling converge them. Malformed rectangles, domain
  homing, and reply geometry remain validated. Offline verified in
  [mid-drag-workspace-recovery](changes/archive/mid-drag-workspace-recovery.md);
  live acceptance pending.
- Workspace mapping, hotplug policy, numbered sends and follow mechanics:
  see Workspaces above.

## Engine Operations and Policy

- COSMIC-style tiling and directional movement are MVP. The directional path
  replaces the legacy path; there is no legacy fallback after a path is
  promoted.
- Durable direction, authorized 2026-09-07: Rust is the structural authority
  for portable deterministic topology, ordered N-ary groups and shares,
  logical workspaces and outputs, focus/navigation, movement, and
  reconciliation. Thin platform adapters own native observation, actuation,
  lifecycle, permissions, and effects. On KWin, direct geometry is the
  structural actuator; Custom Tiles are not a second topology authority.
- Production uses the single `DescribePlan` engine with direct geometry and no
  Custom Tile topology authority or Legacy fallback. Custom Tile production
  research is closed (User decision 2026-10-09;
  [archived feasibility](changes/archive/integrated-plasma-structural-feasibility.md));
  Custom Tiles are not adopted; reopen only on a material KWin change.
  Rust-engine tabbed stacks are selected first after 0.1 with detailed
  carrier/design still unselected (see Visuals for the grouping rule and
  tabs gate).
- Lifecycle foundation, authorized 2026-09-09: portable `cosmic_v1` lifecycle
  plans carry policy version 1. Step 3 replaced send/R4 pending acknowledgement
  with immediate planned-topology commit and complete observation convergence;
  other lifecycle operation contracts remain independent. Logical domains are
  keyed by the opaque `(output, workspace)` pair, so one logical output may
  retain independent workspace trees without inventing native workspace
  semantics. Source-evidenced `cosmic_v1` semantics govern focused-cell binary
  admission, physical-axis selection, equal new splits, proportional
  ordered-N-ary share adaptation, physical-pixel resize, and drag zones.
  The portable split-tree representation fails closed for COSMIC
  center stack drops because stacks are unselected and compositor-owned.
  Recursive collapse is retained. On a focused tiled removal, `cosmic_v1`
  removes the leaf from its source-domain MRU focus stack and selects that
  stack's remaining top; an unfocused removal preserves focus. Send placement
  with `direction=None` uses ordinary admission placement; the separate
  `follow` flag selects target focus (`follow=true`) or source fallback
  (`follow=false`). The portable tiled model clears focus when no source tiled stack entry
  remains. Floating and sticky exceptions and fullscreen/maximize tile overlays
   follow the current Window State rules above.
- Durable policy-mode direction, authorized 2026-09-09: selected policy modes
  target strong source-evidenced behavioral parity. `cosmic_v1` permits
  explicitly recorded functional alternatives and host/game-safety exceptions
  in addition to infeasible platform capabilities; a missing
  capability fails closed with narrow refusal, logging, and later recovery intact. Future Hyprland and other behavior belongs in a
  separate versioned policy mode sharing the portable engine, not in an
  unnamed generic fallback or a platform adapter.
- Future tiling profiles are required after MVP when adding a new tiling type,
  such as Hyprland. A selectable profile/type covers both its tiling
  behavior/algorithm and matching shortcuts; it is not an optional cosmetic
  shortcut preset. COSMIC remains current, while future behavior uses its own
  versioned policy mode sharing the portable engine. This selects no immediate
  Hyprland implementation, native compositor backend, exact parity/mode/
  version/config switching details, coupled-only versus independently
  overridable shortcut UI, or hot-switch semantics. Keyboard-layout
  localization remains separate and initial US-keyboard support is unchanged.
- COSMIC geometry parity, established 2026-09-13, closed 2026-09-16:
  Native KCM-owned `innerGap` and `outerGap` default to `(8, 8)` for the
  effective 8px work-area edge margin and sibling spacing; `outerGap` is not
  the raw COSMIC theme outer value. Retain shares without claiming exact
  COSMIC parity. Revisit only for a reproduced N-ary/deep-layout visual
  discrepancy or a required source-exact fixture that fails under shares. No
  pixel-authority migration, projector correction, numerical-policy change, or
  code/test work is selected.
- Workspace send and cross-output (R4) movement are portable lifecycle
  operations committing planned topology synchronously with complete
  observation convergence (no surviving pair, no ack/verify/pending); a send
  moves only the focused tiled window and recursively collapses its source
  tree, with validated last-active-leaf admission (`map_to_tree` root/output-
  geometry fallback; empty targets are a lone root). See
  Workspaces and Move above for bindings and targeting. Approved 2026-09-20, updated by USER step-3 decision 2026-09-25 (current, offline only, no live verification claimed): exhausted default-Vertical
  `Meta+Left`/`Meta+Right` R4 movement is the selected product behavior across
  a horizontally adjacent output into that output's currently selected logical
  workspace. Local R1/R2/R3 wins first. USER item 5 (2026-10-07) extends
  crossing to all four directions and sole root leaves, selecting reciprocal
  neighbors on full output rectangles; wrapping and workspace
  cycling remain excluded. Rust converges complete source plus target observations, then synchronously commits the planned R4 topology into the canonical per-domain sessions and returns both-domain geometry plus the native assignment; no pair survives the call. Occupied R4 uses whole-target D-axis wrapping nearest the source (Move item 5.5; explicit output send instead uses ordinary admission), retaining the existing owner, generation, revision, correlation,
  single-flight, visibility, exception, and
  fail-closed target fences, with no acknowledgement, verification, pending, status, cancel, or abandon. The active `DescribePlan` route delivers the
  corresponding exhausted horizontal focus transfer with no layout or
  membership writes. R4 uses KWin 6.7.5 public
  `workspace.sendClientToScreen(window, output)` and exact desktop assignment,
  with native write order output, then desktop membership, then source/target geometry (B6 effective origin+minimum targets for overconstrained members). KWin keeps a short flight-local source/target pin with separate unanswered-request and arrival deadlines, stale-reply discard before any setter, prompt follow once on fresh exact mover-on-target proof, forced complete both-domain reconcile even on equal applied evidence with unreadable quarantine, and no `blocksPlan`. Timeout, stale scope, wrong output,
  failed write, identity loss, or partial proof converge on the next complete observations without replay, phantom, or verified-success claim.
- User 2026-10-14 approves R4 own-mover observation tolerance only after native
  transfer setters: allow exactly the flight set minus its mover with retained
  native identity and in-scope output/single-desktop proof. Pre-transfer,
  domain and non-mover refusal guards remain; arrival/follow still need fresh
  exact target placement. Tier 1 COSMIC, Hyprland, i3 and sway all cross to the
  adjacent output with the mover followed/focused; this delivers that cleanly
  rather than through recovery. [Evidence](changes/kde-dual-output-live.md#rightward-r4-defect-correction-and-re-verification).
- The portable world Engine owns independent per-domain Sessions, outer gaps,
  seeding, and relocation behind
  typed events and replies. Do not merge per-domain revisions, fingerprints,
  divergence, or node identities into one permanent Session.
  Send/R4 assemble a transient canonical pair only for the synchronous planned-topology commit; no workspace/R4 pending pair state remains.
- A KWin fork or patch is rejected. The project must operate within existing
  KDE/Plasma/KWin. The Rust-engine/direct-geometry direction above is the
  selected replacement architecture; the bounded adapter remains active only
  until its individual replacement paths are promoted. Group trigger,
  highlight delivery and the tabs gate live under Visuals above (including
  user decision R/S, option 2, 2026-09-26 on endpoint retry).
