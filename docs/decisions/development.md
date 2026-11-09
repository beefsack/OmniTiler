# Development, identity and licensing

### Development entry point
- Decision: `just dev [verbose|trace]` is foreground full-solution dogfooding entry: tiling, hooks, effects/overlays and tray, streamed terminal logs and verified Ctrl+C teardown. Root justfile conditionally imports OS recipes, requires just >=1.56. Linux case-insensitive colon-separated `XDG_CURRENT_DESKTOP`: KDE `dev-kde`, GNOME explicit unsupported stub, other desktops fail clearly. Windows normal `tile --user-start` via Explorer broker/readiness/provenance; trace adds tracing, verbose retains default logging; args pass through. Former Windows non-tiling 10-minute loop is lifecycle-smoke. Unsupported platforms fail explicitly.
- Why: not recorded.
- Assumes / revisit if: Windows `just dev` accepted 2026-10-14 including tray Stop and whole-desktop use; further issues are new backlog items. The macOS preview entry is active below; v4 native acceptance and other effects remain pending.
- Status: active, 2026-10-13
- Evidence: [dev entry implementation](../changes/dev-entry-point.md).

<a id="tentative-macos-development-preview"></a>
### macOS development preview
- Decision: `just dev [verbose|trace]` builds signed dev bundle and launches via LaunchServices, streams foreground logs with paths at start/end; bare dev runs on ordinary apps on each display's current user Space. Ctrl+C or `just dev-stop` requests bounded teardown through exact-run control dir; clean owner result, not process exit/stderr silence, proves stop. Scoped finite runs remain; other effects are later work.
- Why: retain a usable signed development route with observable readiness and bounded exact-run stop.
- Assumes / revisit if: v4 native rebuild, teardown, permission survival across rebuild, GUI-session recovery and readiness acceptance remain pending, as do visual and fixture Space/fullscreen/input acceptance. Clean teardown never authorizes native-Space cleanup: only core `exist:false` does (M6). Legacy helper exceptions retire only after fixture-harness replacement proof, not merely fixture tiling success.
- Status: active, confirmed 2026-11-10
- Evidence: User 2026-11-10; source-read pre-port review: `just/macos.just:81-104` (signed build/launch and stop), `crates/tiler-macos/src/input_session_physical.rs:65` (foreground logs), `crates/tiler-macos/src/dev_selection.rs:11-20` (current user Space/scoping); [preview guide](../changes/macos-dogfood-preview.md), [review limits](../changes/v4-phase3-macos-port.md#pre-port-review-outcomes-2026-11-10).

### Project name
- Decision: project is OmniTiler for KDE, Windows and later macOS. Display/upper CamelCase `OmniTiler`; lower CamelCase `omniTiler`; lowercase IDs/packages/binaries/files/snake identifiers/KWin and KPackage IDs/systemd/Nix/shortcut prefixes `omnitiler`; env/macros `OMNITILER_`; reverse DNS `com.omnitiler.*`, object paths `/com/omnitiler/...`. Preserve functional suffixes; internal generic crate names unchanged; no host-format exceptions. GitHub repo `beefsack/OmniTiler`; OBS/AUR package names `omnitiler`, `omnitiler-native-effect`. `omnitiler.com` belongs to project and backs reverse DNS, future homepage not live; package homepage remains GitHub. Pre-release rename has no migrations/shims; old installed state ceases use.
- Why: reflect supported platforms.
- Assumes / revisit if: domain homepage not yet live.
- Status: active, 2026-10-11.
- Evidence: [repository naming and registered-domain record](../changes/archive/repository-record-hygiene.md).

### Licensing
- Decision: project default MIT, copyright Michael Alexander; Rust crates, KWin TypeScript, KCMs, tray/planner, scripts, packaging/docs MIT. The 15 audited KWin-free native effect logic/test sources are MIT. `activewindowborder.cpp/.h`, which include KWin headers, remain GPL-2.0-or-later; `metadata.json` and the combined effect binary remain GPL-2.0-or-later. Shared FFI/config inputs remain MIT. MIT source linked into a GPL binary retains its source license. Root LICENSE and copyright apply to MIT sources; MIT source files follow the project convention of no per-file license or copyright header. GPL source retains explicit SPDX headers; texts in LICENSES.
- Why: keep the source boundary at direct KWin header use or actual KWin code reuse; independently matching observed behavior is not code reuse. Linking does not require changing MIT sources.
- Rejected: GPL every file/test built into binary; decide boundary after Rust viability.
- Assumes / revisit if: independently matching observed KWin behavior is not code reuse. Revisit an affected source if it later reuses KWin implementation code or includes KWin headers. Allow MIT, Apache-2.0, Unicode-3.0, BSD-2-Clause, BSD-3-Clause, ISC; permissive notice obligations, App Store concern applies to copyleft.
- Status: active, 2026-11-01
- Evidence: [licensing research](../research/licensing.md#41-kwin-gpl-compatibility-required-headers-gpl-20-or-later), [native-effect license audit](../changes/archive/native-effect-license-audit.md), [audit gate](../changes/archive/licensing-audit-gate.md).

### Third-party notices
- Decision: CI license gate uses `licenses`, `bans`, `sources`. Before 0.1 provide generated third-party notices, including MIT/Apache texts for binary dependencies. Use `cargo-about` 0.9.0; one committed generated union for shipping `omnitiler` and supported Linux targets, full selected texts/copyright/NOTICE. The native effect has no Rust staticlib linkage. Manual `just third-party-notices`; CI regenerates to temporary location and fails differences without updating tracked file. Debian `/usr/share/doc/<pkg>/`, RPM `%license`, Arch `/usr/share/licenses/<pkg>/`, Rust Nix outputs `$out/share/licenses/<pkg>/`, source archive root. No GitHub binary archive producer yet; notice remains required before such distribution. Generated files may contain non-ASCII; upstream bytes verbatim, authored files ASCII.
- Why: reverses earlier tentative no-notice choice.
- Assumes / revisit if: GitHub binary delivery remains required before binaries distributed.
- Status: active, 2026-10-13
- Evidence: [licensing audit](../changes/archive/licensing-audit-gate.md).

### Development environment tools
- Decision: Windows/macOS toolchains in one root `mise.toml`, invoked from project root; Linux/NixOS uses devenv/Nix including system libraries. OS-specific entries stay root; minimal AGENTS change. CLI selectors `latest`, Rust `stable`, no committed `mise.lock`; pre-1.0 track latest stable and fix breakage, revisit upgrade process at 1.0. CI verifies mise installs and tool/host/component smoke checks on Windows and macOS 15 arm64 with Windows Cargo gates; no exact mise/Nix equality. Windows PS7 via winget exact Microsoft.PowerShell (not Store); VS 2026 Build Tools or another 2026 edition, Desktop C++ workload, recommended x64/x86 MSVC and Windows SDK via Visual Studio Installer. Git/MSVC+SDK/Xcode+CLT/shell bootstrap remain manual/user-installed. Stable Rust via rustup, rustfmt/clippy, just/jq/gh/rg on Windows/macOS, yq Windows-only; process-local `RUSTUP_TOOLCHAIN`, no persisted override/toolchain files; Linux tools from devenv nixpkgs pin.
- Why: not recorded.
- Assumes / revisit if: Windows native offline gates validated chosen VS route, separate from live/clean-runtime acceptance; installations remain user-owned.
- Status: active, 2026-10-03
- Evidence: [AGENTS operational commands](../../AGENTS.md), [Windows environment](../windows-dev-environment.md), [macOS environment](../macos-dev-environment.md).

### Reference sources and design preference
- Decision: reference implementations remain inputs to functional spec; upstreams/revisions in reference matrix. cosmic-comp n-ary splits is user's favorite tiling UX.
- Why: not recorded.
- Assumes / revisit if: sources are inputs, not automatically adopted behavior.
- Status: active, 2026-10-03
- Evidence: [reference matrix](../spec/reference-outcomes.md).

### KDE development scope and live fixture
- Decision: development `--scope-class NAME` limits to exact case-sensitive KWin `resourceClass`/Wayland app_id; empty scope is whole-world; admission/focus/command fences/launcher remain KDE layer. Recommend `OmniTilerTestWindow`; allowlist only development bundles; installed builds unscoped. Malformed scope refuses, live scope change needs fresh controller. New live fixture uses pragmatic winit helper, native decorations, no GTK/Qt; common geometry/state plus small KDE/X11 type sample; isolate extensions. Use shared caller IDs/ordered batches, many windows/process, fullscreen display-mode/state not game-renderer parity, private local JSON-lines transport/subscriptions. Unix sockets Linux/macOS, owner-restricted named pipe Windows. Fixture expansion includes low-complexity level/focus/attention/bounded observation, preserve identity, honest unsupported/readback, no unsafe; history/elevated Windows/exclusive mode deferred.
- Why: not recorded.
- Assumes / revisit if: platform-specific fixture limits remain; signed-fixture macOS tiling passed 2026-10-20 (`crates/tiler-live-fixture/README.md:228-267`). The remaining gap is a chord/workspace harness on fixture windows with an explicit fixture ownership contract, not bundle scoping. New agent live legs scoped fixture-only first; foreign apps only for app-specific investigation. Legacy helper exceptions only for explicitly bound proofs (`run --hide`, bound macOS probe-target) until fixture-harness replacement is proved.
- Status: active, 2026-10-20
- Evidence: [dev usage](../dev-loop.md#linux-kde), [fixture README](../../crates/tiler-live-fixture/README.md#platform-status), [fixture expansion](../changes/archive/live-fixture-api-expansion.md).

macOS product and port choices live in [macOS decisions](macos.md).
