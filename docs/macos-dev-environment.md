# Native macOS development environment

- **P:** use a physical Apple Silicon Mac and native zsh. A second display
  is needed for multi-display acceptance; a Mac-hosted VM can supplement
  disposable permission/installer tests, not physical input/game evidence.
- Evidence checked 2026-10-03: **V** = fetched primary doc/source below;
  **R** = repository inspection; **O** = upstream report; **U** = unverified on a Mac;
  **P** = recommendation, not an adopted decision. Source keys at the end.
- Read-only host baseline 2026-10-14 (generic; no machine details recorded):
  Apple Silicon host newer than the provisional macOS 15 floor; CLT selected
  with the SDK path resolving, mise tools installed, SIP enabled, and the
  portable-crate build boundary (section 4) passing. Full Xcode absent, so
  `xcodebuild` is unavailable; plain `codesign` is present without full Xcode.
- Related research: [prior art](research/macos-port/prior-art.md) and the
  [tier-2 plan](research/macos-port/plan.md). Selected default (user
  2026-10-03, [decisions](decisions.md#scope-and-platform-goals)): tier 2,
  public plus private APIs with SIP left enabled; no Dock injection, no
  reduced SIP. Surveyed managers use different routes.
  This runbook grants no live-testing authority and makes no install.
- Adopted 2026-10-14: **macOS 15 minimum, Apple Silicon only**. Current
  Homebrew Mac-only OS events give 15+ approximately 93.27% coverage; this
  developer/event-weighted sample is a proxy, not population proof.
  [Decision, sources, and API-risk rationale](decisions.md#scope-and-platform-goals).
  A newer development OS does not validate the floor; Intel is out of scope.

## Adopted and remaining setup decisions

| Decision | Options and consequences | Recommendation (P) |
| --- | --- | --- |
| Install governance | This doc as the macOS dependency list (mirrors the Windows rule in root `AGENTS.md`) vs another declaration file. New macOS tools added here first with user approval; the user performs installs. | **Adopted:** root `AGENTS.md` names this doc as the macOS dependency list with the same user-owns-installs rule as Windows. |
| OS floor and Intel | **Adopted 2026-10-14:** macOS 15+, Apple Silicon only. | Separate floor-runtime validation remains required. |
| Dev signer and release membership | **Adopted 2026-10-14:** persistent self-signed development identity; no paid membership initially. | Release Developer ID/notarization remains open; see section 5. |
| mise as install route | Tool-agnostic installs now vs adopting mise later as the cross-platform route. | **Adopted:** root `mise.toml` manages Rust (via rustup), just, gh, rg, jq with OS-filtered entries (Linux ignores it); `yq` stays Windows-only and is not in the macOS inventory. Manual prerequisites stay manual: Xcode/CLT, host shell bootstrap, Git route per user choice. User runs `mise trust` / `mise install` from the repo root; gates use `mise exec -- cargo ...`. |
| UI language | **Adopted 2026-10-14:** all Rust, including UI. | [Unsafe Code](principles.md#unsafe-code); [enforcement](decisions.md#rust-unsafe-guardrail). |

## Day-one setup, in order

Commands are for the **user on the Mac**. Agents ask before any install.
Use an ordinary admin-capable account; restart the shell after PATH
changes. Native zsh throughout; no bash-isms required.

### 1. Confirm host, chip, and displays

```zsh
sw_vers
uname -m
system_profiler SPDisplaysDataType
```

- Require `arm64` (`uname -m` prints `arm64`). Check the adopted floor locally;
  do not record the development host's OS version in repository files.
- Second display is optional. Accept single-display first; two-display
  proof needs the physical second screen, not a virtual one.

### 2. Install CLT or full Xcode, then verify the toolchain

**V: S1:** the Command Line Tools package carries the same macOS SDK,
man pages, and toolchain binaries as Xcode and installs at
`/Library/Developer/CommandLineTools`. CLT is sufficient as the Rust
linker/SDK. **V: S1:** `xcodebuild` (and `xctrace`) ship with Xcode only,
so the `xcodebuild`-based archive/export workflow needs full Xcode; plain
`codesign` invocation does not itself require it (see host baseline above).

```zsh
xcode-select --install
xcode-select -p
xcrun --show-sdk-path
xcrun --show-sdk-version
clang --version
pkgutil --pkg-info=com.apple.pkg.CLTools_Executables
```

- If Xcode is installed instead, select it and check both tools:
  `sudo xcode-select -s /Applications/Xcode.app`, then
  `xcodebuild -version` and the `xcrun` SDK path above.
- **V: S1:** after a macOS upgrade, re-check with Software Update or
  `softwareupdate -l`; an old CLT package may be incompatible. The current
  generic host baseline is recorded above.

### 3. Install mise, then Rust and CLIs (root `mise.toml`)

**V: S11/S12:** rustup remains the Rust installer; mise drives it (installs
rustup if absent, sets [process-local `RUSTUP_TOOLCHAIN`](https://mise.jdx.dev/lang/rust.html),
no persisted directory override). Pre-1.0 policy tracks latest stable
(consistent with Windows); rustfmt plus clippy; no toolchain file.
Follow the [official mise installation route](https://mise.jdx.dev/installing-mise.html),
then run the declared tools from the repo root:

```zsh
curl -fsSL https://mise.run | sh
# or optionally: brew install mise
```

For the curl route, add mise to this shell's PATH (and the user's zsh
startup configuration for future shells):

```zsh
export PATH="$HOME/.local/bin:$PATH"
```

From the repo root:

```zsh
mise trust
mise install
mise exec -- rustup show
mise exec -- rustc -vV
mise exec -- cargo -vV
mise exec -- rustup which rustc
mise exec -- rustup which cargo
```

- Git comes from CLT or the route the user chooses (system Git prompt or
  a manager). just, gh, rg and jq are mise-managed (root `mise.toml`,
  [OS-filtered entries](https://mise.jdx.dev/dev-tools/)).
  **V: S16:** just offers Homebrew, Cargo and prebuilt routes;
  verify `mise exec -- just --version`. The root `justfile` is the cross-OS
  entry point (per-OS imports; only the current OS recipes load); macOS
  recipes live in `just/macos.just`: `preflight`, `build`, `test`, `clippy`,
  `fmt`, `app <identity>`, and `verify`. `dev` builds the signed development
  bundle and launches the existing preview; `dev-stop` stops its exact owner.
  These recipes do not establish v2 compatibility: the macOS consumer still
  needs its [world-contract port](porting/macos-port-notes.md).
- **V: S14:** Homebrew is optional, never forced. Default prefix on
  Apple Silicon is `/opt/homebrew`; follow its homepage post-install
  shellenv directions for zsh.
- Run gates with `mise exec -- cargo ...` so they consume the
  mise-selected toolchain; no toolchain file.

### 4. Prove the portable offline Cargo baseline (allowlist only)

**R:** `Cargo.toml` workspace members are `tiler-core`,
`tiler-protocol`, `omnitiler`, `tiler-windows`,
`tiler-macos`, `tiler-kde-settings-exporter` and `tiler-live-fixture`.
The portable crates plus macOS probe
below are the Mac baseline. `omnitiler` is the Linux/KDE service (rustix plus
zbus/D-Bus integration), not a supported Mac target; `tiler-windows`
is the Windows adapter. This is a supported-gate boundary, not a claim
that every excluded crate necessarily fails to compile on macOS.

The commands below define the intended macOS gate after its v2 port, not a
current passing result. Earlier native evidence predates the core boundary
replacement; retain it as history and rerun affected acceptance after porting.

```zsh
mise exec -- cargo build --locked -p tiler-core -p tiler-protocol -p tiler-macos
mise exec -- cargo test --locked -p tiler-core -p tiler-protocol -p tiler-macos
mise exec -- cargo fmt --all -- --check
mise exec -- cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-macos --all-targets -- -D warnings
```

- **P:** explicit `-p` flags are the boundary; do not change
  `default-members` or gate dependencies to fake a workspace build.
- Linux gates (workspace tests/clippy, Nix, KWin) stay on Linux.

### 5. Signing: dev versus release

- Local dev builds need no notarization; arm64 Mach-O executables normally
  receive at least an ad-hoc signature. This is not a stable TCC identity.
  Downloaded/quarantined builds may also hit Gatekeeper (see below).
- **V: S4/S6:** release distribution outside the Mac App Store needs a
  Developer ID certificate (Account Holder generates it) plus
  notarization; ad-hoc, Apple Developer, local development, and Mac
  Distribution certificates do not qualify.
- **V: S7:** paid Apple Developer Program membership ($99/year page)
  backs Developer ID distribution and notarization services.
- **V: S4/S5:** notarization prerequisites are hardened runtime enabled,
  valid Developer ID signature, secure timestamp, macOS 10.9+ SDK link,
  and no `com.apple.security.get-task-allow=true`.
- **V: S3:** `xcrun notarytool submit ... --wait` uploads (keychain
  profile preferred over cleartext password), `xcrun notarytool log`
  always gets checked, and `xcrun stapler staple` attaches the ticket.
- For TCC dev, either an Apple Development identity or one persistent
  local self-signed cert can stabilize the identity; local signing does not
  establish release-distribution trust. **O: S17; V: S18:** the local route uses
  Keychain Access > Certificate Assistant > Create a Certificate, with
  type Code Signing; retain that certificate/private key across builds.
  Inspect available identities with `security find-identity -p codesigning`.
  Sign nested executables first and the bundle last. The adopted probe bundle
  ID is `io.github.omnitiler.macos.probe`; the following procedure retains one
  self-signed identity across rebuilds.

#### Persistent self-signed development identity

Use the built-in **Keychain Access** application (no full Xcode or paid
account required):

1. Select the **login** keychain, then **Keychain Access > Certificate
   Assistant > Create a Certificate**.
2. Name it **OmniTiler Mac Dev**, select **Self Signed Root** as Identity
   Type and **Code Signing** as Certificate Type. Create it in the login
   keychain, retaining its private key. Do not create a fresh identity per build.
3. Trust changes, including **Always Trust**, are **not required**. An identity
   marked `CSSMERR_TP_NOT_TRUSTED` can sign and produce a stable designated
   requirement; leave trust settings at their defaults.
4. Check `security find-identity -p codesigning`; its **Matching identities**
   section must contain exactly one full-name match for the intended persistent
   identity. The later **Valid identities only** section is a repeated trusted
   subset, not an additional match. Never put the private key/password in
   source, scripts, logs, or repository files.
5. From the repository root:

   ```zsh
   mise exec -- just app 'OmniTiler Mac Dev'
   mise exec -- just verify
   ```

The recipe uses the fixed build-tree path `target/macos/OmniTilerProbe.app`,
fixed ID, nested-first signing, hardened runtime with no timestamp (local
development only), and no ad-hoc fallback. It requires only existing CLT and
macOS tools: `codesign`, `security`, `otool`, `spctl`, and `shasum` plus the
host shell utilities. It builds with `MACOSX_DEPLOYMENT_TARGET=15.0` and checks
both Mach-O minimum-version load commands. Plain Cargo builds do not enforce
that bundle deployment target. `spctl` rejection of an unnotarized local
self-signed build is diagnostic, not a permission grant or reason to disable
Gatekeeper. The recipe accepts trusted and untrusted code-signing identities,
requires one exact-name match, and signs by its resolved SHA-1. If codesign
requests private-key access during the build, handle that specific key's
authorization locally in the system dialog; certificate trust is separate
from private-key access. Compare the bundle ID and designated requirement
across rebuilds, then verify Accessibility grant survival separately.
[Current evidence](changes/macos-phase-0.md).

### 6. Permissions, TCC resets, and stable identity

- Purpose: Accessibility for AX control; Input Monitoring (`ListenEvent`)
  may be needed for keyboard observation depending on tap mode/OS;
  Screen Recording (`ScreenCapture`) only if
  a future feature captures pixels. Request only what is needed; default
  is no Screen Recording capture. Do not claim AX consent alone grants
  every input mode; event-tap shapes, Secure Input blackout, and
  timeout-disable vary by route (see prior-art survey).
- The commands below are permission **resets**, not grants. After a
  reset the user relaunches the app and grants again in System Settings.
  Service support varies by host OS: check `man tccutil` on the Mac.
  The following forms target only the development probe's adopted bundle ID.
  Run a reset only when diagnosing that app's stale permission state, after
  stopping it; it is not a normal build step.

```zsh
man tccutil
tccutil reset Accessibility 'io.github.omnitiler.macos.probe'
tccutil reset ListenEvent 'io.github.omnitiler.macos.probe'
tccutil reset ScreenCapture 'io.github.omnitiler.macos.probe'
```

- **V: S9:** omitting the bundle ID resets that service for every app in
  the current account; `All` affects all services. Use the per-app forms.
- Stable-identity rule: keep the same bundle ID, install path, and
  signing identity across runs. **O: S17:** changed ad-hoc binaries can
  acquire a different cdhash and lose TCC grants. Standard mitigations: one
  persistent dev identity, fixed ID and path, separate dev vs release
  IDs, and stop/recover the app before rebuilding. Verify grant survival
  across real rebuilds; a stable path alone is insufficient. **V: S18:**
  designated requirements, rather than a filename, define signed identity.
- Inspect what TCC will see before blaming the OS:
  `codesign --display --verbose=2 <path-to-app>` and
  `codesign -d -r - <path-to-app>` show the authority and designated
  requirement. Self-signed local trust does not transfer to other
  machines; release uses Developer ID (see above). Never edit the TCC
  database directly.
- Once the signed bundle exists, add **OmniTilerProbe.app** with the **+**
  button under **System Settings > Privacy & Security > Accessibility**, and
  enable it. Use the file picker's **Go to Folder** to reach the fixed bundle
  under the checkout's `target/macos/` directory. Stop/relaunch the probe and
  run `mise exec -- just preflight-app` through LaunchServices; require
  `ax_trusted:true` before AX writes. Add the same signed bundle under
  **Input Monitoring** only if
  the bounded tap leg reports that permission is required; stop/relaunch
  after a change. Do not grant the terminal as a workaround. No Screen
  Recording grant is needed for these metadata-only probes. Direct shell
  execution is attributed to the terminal and does not test the bundle's grant;
  see the [launch protocol](live-macos-testing.md#required-preflight).

### 7. Unsigned runs, quarantine, and Gatekeeper

- **V: S10:** an unnotarized/unknown-developer app shows a warning; the
  documented user path is System Settings > Privacy and Security > Open
  Anyway (about an hour after the blocked launch), not a bypass.
- Do not recommend disabling Gatekeeper or SIP. Keep both on; treat an
  unsigned dev build as dev-only friction, not a shipping state.
- **V: S18:** `spctl -a -vvv -t exec <path-to-app>` reports the Gatekeeper
  assessment for a built artifact; it is a diagnostic, not a grant.

### 8. CI (cheap macOS smoke job)

- **V: S13 (fetched inventory):** macOS arm64 labels are `macos-14`,
  `macos-15`, `macos-26`, `macos-latest`; Intel labels are
  `macos-15-intel`, `macos-26-intel` (plus an `xcode-27` preview label).
  `macos-latest` currently resolves to the macOS 26 arm64 image.
- Implemented: one arm64 job pinned at `macos-15` using
  `jdx/mise-action@v4` (runs `mise install`), refreshing stable via
  `mise exec -- rustup update stable --no-self-update`, with tool version smoke
  checks (`mise exec -- <tool> --version` for rustc, cargo, just, gh, rg,
  jq), a `rustc -vV` host check (`aarch64-apple-darwin`), rustfmt/clippy
  presence checks (`cargo fmt/clippy --version`), and `git diff --check`.
  The job also builds/tests/formats/strict-clippies the portable allowlist
  plus `tiler-macos`; Windows checks its non-macOS stub and Linux workspace
  gates include it. No live TCC/AX claims from CI;
  headless success does not establish TCC, AX, visuals or gaming.

### 9. Future native iteration loop

- `tiler-macos` contains explicit bounded probes, an owned AppKit helper, and
  authenticated `probe-stop`; it is not a tiling adapter or full dev loop.
  `just app`/`just verify` build and inspect the fixed signed bundle without
  launching hooks or windows. Read the [probe CLI](../crates/tiler-macos/README.md)
  and [live protocol](live-macos-testing.md) before experiments. Owned
  native-Space move/restore commands are direct-SLS-only and untested;
  switch/create/destroy report unsupported source-derived routes. The current
  helper uses its exact spawned Child and a per-run pipe nonce, not a
  LaunchServices launch-date token. `spaces-session` proves owned CG identity,
  Space membership and graceful/kill cleanup while holding that Child; it
  dispatches no Space writes. Cross-command nonce-title authentication is
  blocked when CG titles are withheld; a bounded AX-title fallback is
  implemented but its positive leg awaits Accessibility trust. Independent
  Space restoration and all move/switch effects remain unproven. The AX
  partial-write restoration fix has fault-injection coverage, not live
  acceptance. [Current evidence](changes/macos-phase-0.md).

## Verification checklist

- [ ] Supported OS/architecture and display topology verified locally with
  `sw_vers`, `uname -m`; omit development-host version and hardware details
  from repository evidence.
- [ ] CLT or Xcode path verified: `xcode-select -p`, SDK path, and
  (Xcode only) `xcodebuild -version`.
- [ ] `mise exec -- rustc -vV` shows `aarch64-apple-darwin`;
  mise-selected stable toolchain, no persisted directory override or toolchain file.
- [ ] Section 4 allowlist build/test/fmt/strict clippy pass via
  `mise exec`; root Linux crate and `tiler-windows` correctly excluded.
- [ ] Dev signer vs release membership decided; no cert secrets stored.
- [ ] Only needed TCC services requested; resets understood as resets;
  `man tccutil` checked on the host; stable ID/path/identity kept.
- [ ] Gatekeeper/SIP left on; quarantine path exercised via Open Anyway
  at most; `spctl` assessment recorded where relevant.
- [ ] CI smoke job (`macos-15`, mise install + version/host/component
  checks) green; labels re-verified at authoring time (fetched labels
  drift).

## Sources (only URLs fetched in this unit, 2026-10-03)

- S1: https://developer.apple.com/documentation/xcode/installing-the-command-line-tools.md
- S3: https://developer.apple.com/documentation/security/customizing-the-notarization-workflow.md
- S4: https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution.md
- S5: https://developer.apple.com/documentation/security/hardened-runtime.md
- S6: https://developer.apple.com/developer-id/
- S7: https://developer.apple.com/programs/
- S8: https://developer.apple.com/support/xcode/
- S9: https://developer.apple.com/documentation/xcode/resetting-access-to-protected-resources-in-macos.md
- S10: https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac
- S11: https://www.rust-lang.org/tools/install
- S12: https://rust-lang.github.io/rustup/installation/other.html
- S13: https://docs.github.com/en/actions/reference/runners/github-hosted-runners
- S14: https://docs.brew.sh/Installation
- S15: https://mise.jdx.dev/getting-started.html
- S16: https://just.systems/man/en/packages.html
- S17 (upstream development report, not an Apple guarantee): https://github.com/jackielii/skhd.zig/blob/main/docs/CODE_SIGNING.md
- S18: https://developer.apple.com/library/archive/technotes/tn2206/_index.html
- R: `Cargo.toml`, `crates/tiler-core/Cargo.toml`,
  `crates/tiler-protocol/Cargo.toml`,
  `crates/omnitiler/Cargo.toml`,
  `docs/research/macos-port/prior-art.md`, root `AGENTS.md`.
