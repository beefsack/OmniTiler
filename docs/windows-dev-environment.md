# Native Windows development environment

- Use Windows 11 x64, Windows Terminal, PowerShell 7 and Rust MSVC natively.
  Keep a separate Windows clone; synchronize source through Git, never Linux
  `target/`, settings or compiled effects. No WSL or development VM.
- Evidence checked 2026-09-30: **V** = verified primary documentation/source;
  **R** = verified repository inspection; **O** = upstream issue report;
  **U** = unverified on the host; **P** = recommendation, not an adopted decision.
  Source keys are at the end. Documented commands still need host verification.
- Record the Windows 11 x64 host's OS build and actual
  display topology (identities, modes, scale, work areas) in each live test
  record; observed values are test evidence, never runbook prerequisites.
  Do not change host boot or partition arrangement as part of setup.
- **Accepted baseline (user 2026-09-30):** initial owned-window proof may be
  bounded to one display. Multi-monitor acceptance needs a multi-monitor
  Windows setup (native Windows, not a VM).

- **R:** KDE-first extraction ended at K1. Start Windows Phase 1 with native
  build/tests, the recorded host display topology, independent stop/restore
  on owned test windows (initial proof may be bounded to one display),
  then physical Win+Arrow proof. Native dev/stop, exact-owner emergency
  exit and independent owned-window restore must be proven on the host before
  broader use (see the [historical record](changes/archive/windows-phase1-implementation.md));
  input proof remains pending.
  This runbook grants no live-testing authority.

## Decisions needed from the user

### Before the first Windows session

| Decision | Options and consequences | Recommendation (P) |
| --- | --- | --- |
| Root AGENTS.md dependency rule | **Decided (user 2026-09-30):** root `AGENTS.md` keeps `devenv.nix` for Linux; on Windows this document is the dependency list. New Windows tools are added here first with user approval; the user performs installs. | Done. |
| Dependency declaration file | **Decided (user 2026-09-30, extended with mise):** root `mise.toml` declares the Windows/macOS CLI and Rust tooling (OS-filtered entries; user runs installs). This runbook remains the dependency list and manual-prerequisite record. A WinGet Configuration (`.config/configuration.winget`) may follow once the setup is proven on the host. | mise route active; WinGet config on revisit. |
| `.gitattributes` | **Decided (user 2026-09-30): repository-wide LF policy**, the contract below, added before the Windows clone. Renormalization changed no tracked file. | Done. |
| Rust upgrades | **Settled (user 2026-09-30, option A):** pre-1.0 track latest stable Rust and fix breakage. Windows uses rustup stable default with rustfmt/clippy, no directory override and no toolchain file. Linux and existing Linux CI use the regularly bumped nixpkgs pin in `devenv.yaml`. | Revisit the upgrade process at 1.0. |
| Windows live-testing governance | **Decided (user 2026-09-30, option A):** write [live Windows testing](live-windows-testing.md) before the first Windows session. Each experiment class still needs user authorization. | Done; follow the protocol before live work. |

### During the first Windows session

| Decision | Options and consequences | Recommendation (P) |
| --- | --- | --- |
| Storage and long paths | **Settled (user 2026-09-30):** use a short checkout path; Git longpaths enabled per clone. The host long-path setting is optional: probe it read-only, review errors, and never write registry automatically. Keep paths short. | Done. |
| Credentials | **Settled (user 2026-09-30):** HTTPS + GCM. | Done; do not copy credentials into the runbook. |
| CLI scope | **Settled (user 2026-09-30; shell/toolchain updated 2026-10-10):** Git, rg, PS7, rustup, VS 2026 toolchain + SDK, just, jq, yq, gh. No Coreutils; qsv skipped. Root `mise.toml` manages Rust (via rustup), rg, just, jq, yq, gh; Git and the shell/toolchain prerequisites stay manual (see §§1, 4 below for routes and validation). | Done. |
| opencode configuration | **Settled (user 2026-09-30):** opencode via winget; transfer the known-working config without an explicit shell path. Smoke-test the transferred configuration before work. | Follow the §6 smoke test. Stop/report incompatibility rather than silently changing configuration. |
| Isolation | **Settled (user 2026-09-30):** Sandbox for Phase 4 clean runtime plus Win+L guest-only policy; Phase 1-3 live proof is physical-desktop owned windows first. Historical 2026-09-30 preflight state is in the [Phase 1 note](changes/archive/windows-phase1-implementation.md). Never host policy writes. | Physical first under the live protocol. Sandbox only for Phase 4 clean runtime and guest-only Win+L; if unavailable, defer those; no policy experiments on the daily-use system. |

### Line-ending contract

Adopted in root `.gitattributes` (user 2026-09-30). **V: G1:** covers Rust, TS, shell, Nix, CMake, docs, JSON/TOML and future
PowerShell files without an extension-by-extension list. PS7 accepts LF; use
UTF-8 without BOM for project text. Future CMD/batch files check out as CRLF.

```gitattributes
* text=auto eol=lf
*.cmd text eol=crlf
*.bat text eol=crlf
*.kwinscript binary
```

- **R:** at adoption the index held 445 LF text files and one binary
  `.kwinscript` ZIP, no CRLF text; a renormalization dry run changed no
  tracked file.
- **P:** the fresh-clone command below also stores LF settings before
  checkout (belt and braces). For an existing clone, inspect first; stop on
  conversion churn and ask the user, rather than resetting/re-checking out.

### Dependency declaration and Rust parity

- **V: W1:** `winget configure` is a supported Microsoft provisioning feature
  since WinGet 1.6.2631, not a universal package lockfile or Nix substitute.
  Microsoft's convention is `.config/configuration.winget`. Package resources
  can declare versions; referenced DSC modules also need reviewed versions.
  Resources execute code and can request admin/UAC (`securityContext:
  elevated`); start unelevated so user-scoped installs keep the user's identity.
- **V: W1:** `Microsoft.WinGet.DSC/WinGetPackage` installs packages;
  `Microsoft.VisualStudio.DSC/VSComponents` can apply workloads/components
  from `.vsconfig` (Microsoft's example permits prerelease resources).
  A package declaration for Build Tools alone does not prove C++/SDK installed.
  `winget configure show -f FILE`, `validate -f FILE`, `test -f FILE`, then
  `winget configure -f FILE` are the review/validation/application sequence.
  These files are proposals and are not present in this repository.
- **P:** WinGet installs rustup, not the chosen rustup toolchain or project
  override. `coreutils-manager disable link` is also an explicit post-install
  action. Generic DSC Script resources could do both, but add executable
  automation without a demonstrated need. WinGet import/export alone cannot
  represent these post-install states or a VS workload contract.
- **R; V: R1:** `devenv.nix` selects default nixpkgs Rust from pinned
  `devenv.yaml`; verify the selected version with `rustc -vV` instead of assuming one. No rustup or
  toolchain file is used. Direct Nix binaries ignore `rust-toolchain.toml`;
  devenv can explicitly use `languages.rust.toolchainFile` with rust-overlay.
  Existing Ubuntu CI invokes Cargo inside devenv, so a TOML file alone does
  not align CI. Rustup proxies elsewhere would honor it and might download
  a toolchain. Verify actual command paths before claiming cross-OS parity.

### Nixpkgs Rust pin bump procedure (Linux pin bump; Windows refresh below)

- **P: scope:** the `devenv.yaml` nixpkgs pin drives Linux Rust/KWin/shell
  (`devenv.nix` selects default nixpkgs Rust; Ubuntu CI invokes Cargo
  inside devenv). The `flake.nix` pin is separately reviewed scope: it
  supports native CI (`nix build .#checks.x86_64-linux.native-effect-tests`)
  and standalone delivery; the KWin CI job runs script tests in devenv,
  not a native effect
  compile. `scripts/nix-host-kwin-build.sh` always resolves the exact
  installed host KWin derivation, so a devenv bump never changes host
  KWin build authority. Never edit a flake input without regenerating
  its lock.
- **P: procedure (on Linux):**
  1. Compare the candidate `nixos-unstable` revision first: its `1_xx.nix`
     `rustcVersion` must equal latest stable (confirm the default `rust =`
     selection in `pkgs/top-level/all-packages.nix`), plus the KWin/Qt/KF
     versions (`pkgs/kde/generated/sources/{plasma,frameworks}.json`,
     `pkgs/development/libraries/qt-6/srcs.nix`).
     If latest stable is not yet on `nixos-unstable`, defer and retry a
     later revision. Overlay or `staging` routes need a separate decision.
  2. Edit the `devenv.yaml` nixpkgs `url` rev, then run
     `devenv update nixpkgs` (pinned CI CLI `github:cachix/devenv/v2.4.0`).
     Inspect the rev/hash/`devenv.lock` diff; never hand-edit hashes.
  3. Restart the Linux devenv session, then verify the selected version with
     `devenv shell --impure -- rustc -vV` before running gates.
  4. Run the Linux gates per `.github/workflows/ci.yml` (kwin, rust,
     shell, native jobs) plus the local Windows gates via the approved
     tool (`mise exec -- cargo ...`).
  5. Only when validating/activating a native build: host user runs
     `scripts/nix-host-kwin-build.sh resolve`, rebuilds the native
     effect host-matched, and activates in a fresh Plasma session.
- **P: Windows:** unchanged. `mise.toml` tracks rolling stable Rust; the
  user owns updates (`mise exec -- rustup update stable --no-self-update`).
  `mise install` can reuse an already-installed stable without refreshing it.

## Day-one setup, in order

Commands are for the **user on the Windows host**. This list is the Windows dependency
contract (root `AGENTS.md`); agents ask before any install. Use an ordinary
account/terminal; allow installer UAC only where required. Reopen Terminal
after installers change PATH; restart opencode after configuration changes.

### mise tool route (root `mise.toml`)

**Settled:** root `mise.toml` declares the Windows/macOS CLI and Rust
tooling with [OS-filtered entries](https://mise.jdx.dev/dev-tools/);
Linux ignores it (devenv/Nix route). The user owns installs; agents ask
before anything. After the clone (§2) and the manual prerequisites
(§§1, 4), install mise itself through its
[official winget route](https://mise.jdx.dev/installing-mise.html):

```powershell
winget install --exact --id jdx.mise --source winget
```

Reopen the terminal so PATH picks up mise, then from the repo root:

```powershell
mise --version
mise trust
mise install
mise exec -- rustc -vV
mise exec -- cargo -vV
mise exec -- rg --version
```

- Managed by mise: stable Rust via rustup (mise installs rustup if absent;
  [process-local `RUSTUP_TOOLCHAIN`](https://mise.jdx.dev/lang/rust.html),
  no persisted directory override), just, jq, yq, gh, rg. Manual
  prerequisites stay manual: Git, the VS 2026 toolchain + SDK (Visual Studio
  Installer route), winget PS7 bootstrap (PowerShell stays a manual host shell; it is not mise-managed).
- Run gates with `mise exec -- cargo ...` so they consume the
  mise-selected toolchain. No `rust-toolchain.toml`. Require
  `host: x86_64-pc-windows-msvc`.

### 1. Establish PowerShell 7 and a short checkout location

**User decision 2026-10-10:** install PS7 through winget;
no explicit shell path in opencode. Use a Windows Terminal PS7 profile.
Check the shell generically (edition Core, current stable); no version is pinned.

```powershell
winget --version
winget install --exact --id Microsoft.PowerShell --source winget
```

Open a fresh Terminal PS7 profile:

```powershell
$PSVersionTable
$PSHOME
where.exe pwsh
winver
```

- **P:** require edition `Core`, version >= 7.4 if Coreutils is installed;
  use current stable PS7 (Coreutils recommends >= 7.6 for `~` support).
- **V: W3:** optional Dev Drive: Windows 11 build >= 22621.2338, 8 GB RAM
  minimum/16 GB recommended, >= 50 GB free, admin setup. ReFS Dev Drive is
  Microsoft-recommended for repos/package caches, not VS/SDK/tool installs.
  Use Settings to create it only with user approval; do not repartition any host
  as part of setup. Defender performance mode requires a
  trusted drive, Defender primary AV and real-time protection on (platform
  >= 4.18.2303.8, intelligence >= 1.385.1455.0). It scans asynchronously;
  it is not an exclusion. Verify `fsutil devdrv query D:` and Windows Security
  > Dev Drive protection. Keep default Cargo cache paths initially; a later
  `CARGO_HOME` relocation also requires updating the rustup shim PATH.

### 2. Install native Git and rg, then clone safely

**V: G2:** Git installer defaults: PATH option `Cmd` (Git from command line
and third-party tools), `core.autocrlf=true`, Credential Manager enabled.
Choose `Cmd`, not `CmdTools` (adds `mingw64\bin` and `usr\bin` GNU/MSYS tools).
For interactive installation choose "Checkout as-is, commit as-is"; WinGet
defaults can instead be overridden for this clone as shown. Git Bash remains
an explicit escape hatch; never the default tool shell or native build path.

```powershell
winget install --exact --id Git.Git --source winget
# rg is mise-managed (see the mise route above), not a winget install.
```

The following uses `C:\src\pat` as a fresh-clone example; use a short host path. Verify the selected parent exists before cloning:

```powershell
git clone --config core.autocrlf=false --config core.eol=lf --config core.longpaths=true https://github.com/beefsack/OmniTiler.git C:\src\pat
Set-Location C:\src\pat
git branch --show-current
git config --show-origin --get-regexp '^core\.(autocrlf|eol|longpaths)$'
git config --show-origin --get-all credential.helper
git check-attr text eol -- Cargo.toml kwin/src/entry.ts scripts/nix-host-kwin-build.sh devenv.nix kwin/native-effect/CMakeLists.txt docs/windows-dev-environment.md example.ps1 example.cmd
git ls-files --eol
git status --short
git diff --check
where.exe git
$env:PATH -split ';'
```

For an existing clone, set the same contract locally instead of re-cloning:

```powershell
git config --local core.autocrlf false
git config --local core.eol lf
git config --local core.longpaths true
git config --show-origin --get-regexp '^core\.(autocrlf|eol|longpaths)$'
```

On a fresh host, set the commit identity yourself before the first commit
(agents never change Git config):

```powershell
git config --global user.name "<name>"
git config --global user.email "<email>"
```

- **V: G3:** Git `core.longpaths` covers built-in Git operations, not every
  shell/tool. **V: W4:** Windows' separate `LongPathsEnabled` opt-in also
  needs app `longPathAware` support (or supported extended-path APIs); it
  does not make every Rust build script/SDK/linker/Explorer path safe.
  **U:** deep Cargo target paths need host verification. Prefer short
  checkout/cache/output roots and verify the native build on the host;
  these steps make no registry write.
  Read-only check:

```powershell
Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' -Name LongPathsEnabled
```

- **P:** `example.ps1`/`example.cmd` above are deliberate attribute-pattern
  probes, not files to create. After adopting the proposal they should report
  `text: set`, with `eol: lf`/`crlf`; otherwise `unspecified` is expected.

### 3. Optional named CLI tools and Coreutils

**V: W5/W6:** only tools named by the global instructions/project are included.
just, jq, yq and gh are mise-managed (see the mise route above).
Verify with `mise exec -- <tool> --version`. qsv: choose the upstream
Windows x64 MSVC release ZIP and put `qsv.exe` in the user's approved
tools directory on PATH; no verified WinGet ID is assumed. No Node or
additional package manager is needed for the official opencode CLI
executable or the portable Rust gates.

**V: W7:** Microsoft Coreutils is a separate preview Windows package built
on uutils (first release `v2026.5.29`), including a hard-link utility named
`link`. If selected, install and disable it **before building**:

```powershell
winget install --exact --id Microsoft.Coreutils --source winget
coreutils-manager disable link
where.exe link
Get-Command ls, ls.exe, cat, cat.exe, rm, rm.exe
```

- **V: W7:** PowerShell aliases shadow utilities; use `.exe` suffixes for
  native binaries. No sed/awk; use dedicated read/search/edit tools, PS7 or jq/yq/qsv.
  The PSReadLine input wrapper is interactive, not a tool-shell guarantee.
  Coreutils does not make POSIX scripts native PowerShell scripts.

### 4. Install the VS toolchain and an explicitly selected Rust MSVC toolchain

**V: R2/W8; user decision 2026-10-10:** VS 2026 Build Tools or another 2026
edition with the Desktop development with C++ workload, recommended x64/x86
MSVC tools and Windows SDK; full Visual Studio IDE is unnecessary. Install
through Visual Studio Installer and confirm these components there; no 2026
winget package ID is confirmed, so no winget install command is given here.
**Validated 2026-10-10:** the native offline gates in §5 passed with the
VS 2026 MSVC linker. Live-testing and clean-runtime acceptance remain separate.

Reopen PS7 at the checkout and follow the mise tool route above before
the Rust checks below. Pre-1.0 policy is latest stable (user
2026-09-30, option A; revisit at 1.0). Record observed rustc/cargo versions
with each gate run; observed numbers are test evidence, never pins. The
Linux toolchain follows the `devenv.yaml` nixpkgs pin.

```powershell
mise exec -- rustup show
mise exec -- rustc -vV
mise exec -- cargo -vV
mise exec -- where.exe rustc
mise exec -- where.exe cargo
```

- No persisted directory override and no `rust-toolchain.toml`. Require
  `host: x86_64-pc-windows-msvc`; never GNU/MSYS for product builds.

### 5. Prove linker discovery and run native offline-development gates

**R:** source/dependency inspection, not an MSVC compilation result:

| Crate / gate | Native Windows recommendation |
| --- | --- |
| `tiler-core`, `tiler-protocol` | Build/test: pure Rust policy; zero normal core dependencies, protocol serde/serde_json. |
| `tiler-kwin-effect-ffi` | Include Rust tests/staticlib build: only portable Rust/core/serde/POD exports. Its Qt/KWin C++ consumer is Linux-only. |
| `tiler-windows` | Include build/tests/clippy; native APIs are Windows-gated, ledger tests portable. Build also produces the owned test-window binary. |
| `omnitiler` | Exclude: unguarded `rustix::process::geteuid` plus D-Bus/KDE service/tray integration. It cannot compile as-is on MSVC; Windows needs its own adapter. |
| fmt / strict clippy | fmt all packages without compilation; clippy only the allowlist. |
| KWin npm | `npm ci` and `typecheck` can use native Node >= 24. Current build/test scripts use POSIX `rm -rf`/`VAR=value`; tests also invoke `npx` with `execFileSync` (Windows `.cmd` issue). Keep build/tests on Linux unchanged. |
| Linux gates | Workspace Rust tests/clippy, `just check-portable` (bash/jq/grep), shell suites, Nix checks, native KWin build/CTest and user live tests stay on Linux. |

- **R; P:** verify Windows-target std availability in the Linux devenv
  sysroot before claiming offline `cargo check --target
  x86_64-pc-windows-msvc` results; do not install toolchains ad hoc to work
  around a missing target.
  **P:** explicit package flags are the smallest reliable boundary; changing
  `default-members` changes Linux defaults, while target-gating dependencies
  alone cannot repair unguarded Linux source.

From ordinary PS7:

```powershell
where.exe link
mise exec -- cargo build --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows
mise exec -- cargo test --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows
mise exec -- cargo fmt --all -- --check
mise exec -- cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows --all-targets -- -D warnings
```

- **V: R3:** rustc's MSVC discovery normally finds an absolute VS linker and
  injects SDK/LIB/PATH even outside a Developer shell. No `where.exe link`
  result in ordinary PS can coexist with a successful Rust build. In a
  Developer environment (`VCINSTALLDIR`), discovery trusts the first matching
  PATH tool: a foreign `link.exe` there can break a build despite installed VS.
  Failed discovery falls back to PATH; explicit linker overrides can bypass
  the discovered executable. `cc` build scripts discover `cl`/`lib` too;
  scripts directly invoking `link.exe` still depend on their own PATH.
- **V: W9; P:** diagnostic: open Start's **x64 Native Tools Command Prompt
  for VS**, then launch PS7 from that CMD window to inherit
  its environment (no hardcoded install path):

```cmd
pwsh.exe -NoProfile
```

```powershell
where.exe link
where.exe cl
where.exe lib
link.exe /?
$env:VCINSTALLDIR
$env:LIB
$env:INCLUDE
```

- **P:** first linker must be Microsoft's MSVC `Hostx64\x64\link.exe`.
  Do not permanently add SDK/MSVC or Git `usr\bin` to PATH; do not set an
  arbitrary Cargo linker override to hide incomplete installation.
- **R:** Windows CI job: `windows-latest`, `shell: pwsh`, checkout,
  `jdx/mise-action@v4` (runs `mise install` and exports the mise
  environment), the four commands above via `mise exec -- cargo ...`,
  a mise tool version/host smoke check (rustc host
  must be `x86_64-pc-windows-msvc`; just/jq/gh/rg/yq versions), then
  `git diff --check`, with the four explicit packages above.
  No Nix/KWin/live tests in this job. Hosted runner preinstalled tools are
  **not** clean-runtime evidence [W10].
- **P:** Windows sessions touching shared code must pass these native gates
  plus all applicable Linux gates via CI or Linux. The Windows host has no
  Linux/macOS build targets, so non-Windows compile errors surface first in
  CI; after shared-core commits from other streams, check Windows CI too
  (example regression repaired at `c75fe675`).
  Report Linux gates pending until green; physical KDE/Windows acceptance is
  user-owned. Do not claim deferred gates passed. `just dev` dispatches per OS
  from the root justfile (requires just >= 1.56; conditional imports of
  `just/linux.just`, `just/windows.just`, `just/macos.just`). Native Windows
  loop: `just dev`, `just dev trace`, `just stop`; bounded owned-window
  journey: `just proof`. `just lifecycle-smoke [trace]` covers the previous
  non-tiling loop; `just tile` still requires explicit `--user-start`.
  `just dev [trace|verbose] [tile args]` forwards extra tile arguments (for
  example `--scope-exe`, `--group-underlay-color`). After Ctrl+C, `just`
  reports the recipe failed with exit code 1 even when the printed cleanup
  report says the stop, restore and ledger check succeeded; trust the report.
- The dev loop copies binaries before launch so builds do not overwrite running
  images. Explorer's desktop `Document.Application` broker keeps dev/test actors
  outside the protected Terminal tree; plain Shell.Application is caller-context.
  CLI `stop`/`emergency-stop` exit the verified owner only; call `restore` next
  (the Just `stop` recipe does both). Recovery state is per-user LocalAppData/session.

### 6. Configure opencode and smoke-test the actual tool environment

- **Settled (user 2026-09-30):** install opencode via winget and transfer the
  known-working configuration. Keep PS7 shell resolution without an explicit
  opencode shell path.
- **V: O2:** global paths are `~/.config/opencode/opencode.json` and
  `~/.config/opencode/AGENTS.md` (normally `$HOME\.config\opencode\...` on
  Windows). Transfer the user's existing global rules,
  adapt Linux paths; preserve provider settings.
- **O; U: O3:** reports cover Store pwsh -> 5.1 fallback, ripgrep extraction,
  Windows plugin cache paths containing illegal `:`, and Git/PATH casing.
  Reports are verified, their applicability/fix status on the chosen binary
  is not fully established. Verify the transferred configuration on the
  host before work; a passing smoke test does not verify every setting. Installing
  rg on PATH does not prove opencode's internal bootstrap will use it.

```powershell
opencode --version
opencode debug config
```

- **P:** inspect effective config locally (it can contain secrets); restart
  opencode after changes. Run a shell tool call executing:

```powershell
$PSVersionTable
$PSHOME
Get-Location
Get-Command git.exe, rg.exe, rustc.exe, cargo.exe
rustc -vV
where.exe link
git status --short
```

- **P:** require `PSEdition = Core` from that **tool call**, not just Terminal;
  verify paths with spaces, dedicated glob/read/grep on `Cargo.toml`, then one
  fresh read-only smoke-test unit. Never probe
  an actual shutdown, policy write or destructive delete. Report failure.

### 7. Separate clean runtime from risky experiments

- **V: R4:** MSVC is genuinely native but defaults to dynamic C runtime.
  For the future Windows release binary, recommend target-scoped
  `-C target-feature=+crt-static`, with compatible native dependencies, to meet
  "no extra runtime installation". This is a build recommendation, not a
  Cargo change. In the VS environment inspect the actual executable:
  `dumpbin.exe /DEPENDENTS C:\path\to\actual-built.exe`. Only intended
  Windows 11 OS DLLs should be required, no VC redistributable/dev/MSYS DLLs.
  A staticlib `.lib` is not runnable runtime proof.
- **V: W11:** optional Windows Sandbox requires a supported edition
  (Pro/Enterprise/Education, not Home), BIOS virtualization, compatible CPU
  virtualization/SLAT, >= 4 GB RAM (8 recommended), two cores, >= 1 GB disk.
  Enable Windows Sandbox in Windows Features as a user/admin action; restart
  if prompted. It is a disposable test guest on the Windows host, not development
  in a VM or an assumption that another host can provide one.
- **P:** map only the exact shipped-payload directory read-only, disable
  networking/clipboard in `.wsb`, copy payload into guest-local storage and
  run without installing Rust, Git, PS7 or build tools. Repeat on a clean
  machine if Sandbox is unavailable. Phase 1-3 proves startup/stop/restore on
  owned windows on the physical desktop first; Phase 4 repeats clean
  runtime there. **U:** actual runtime dependency closure is not yet proven.
- **V: W11:** closing Sandbox discards guest state; guest restarts retain it
  on Win11 22H2+, but writable mapped folders persist host changes. Avoid
  writable mappings. Win11 24H2 does not guarantee inbox Notepad/Terminal;
  bring the owned test harness. Verify process integrity: Sandbox's default
  user is an administrator, not proof of ordinary medium-integrity behavior.

| Phase 1 experiment | Environment and evidence limits |
| --- | --- |
| Clean binary startup; owned-window hide/reveal and forced-process-loss recovery | Physical desktop with owned disposable windows first (Phase 1-3). Phase 4 repeats clean-install/runtime in Sandbox when available. Discarding a guest is containment, not proof the independent restore path works. |
| Win+L policy | Deferred to Phase 4 Sandbox guest-only snapshot/write/readback/locking-API/restore experiment. **U:** redirected Win+L may reach the host; no primary guarantee of guest chord delivery or reliable guest unlock. Never modify host policy. |
| Win+Arrow, Snap/Start suppression, down/up and disable reversal | User's physical desktop; guest hooks see redirected input, not equivalent shell behavior. All four directions, ordinary integrity and game-disable proof. |
| Two monitors, mixed DPI, games/anti-cheat, UAC/secure desktop | A multi-monitor Windows setup (native Windows, not a VM). Sandbox's one guest display is not the host's topology; vGPU/RDP is not physical game compatibility. |

### Verification checklist

- [ ] Windows live-test boundary decided; installed tools match this list and the latest-stable Rust policy.
- [ ] Tool call: Core PS7, correct `$PSHOME`, native executable paths.
- [ ] `rustc -vV`: x86_64-pc-windows-msvc; stable default active in checkout, no override.
- [ ] Native build/test/fmt/strict clippy pass; required Linux gates identified.
- [ ] `where.exe link` checked in ordinary and x64 VS environments; no foreign
  linker in the VS environment; Coreutils link disabled if installed.
- [ ] Git settings/origins, LF checkout/attributes, `core.longpaths`, short
  paths, clean status and `git diff --check`; no mass conversion.
- [ ] Host OS/boot arrangement recorded; do not change the host boot arrangement.
- [ ] Sandbox availability confirmed before Phase 4 + Win+L guest-only work; the historical 2026-09-30 preflight state is in the [Phase 1 note](changes/archive/windows-phase1-implementation.md), not current proof.
- [ ] Host display topology recorded (see top of this doc); initial owned-window proof may be bounded to one display; multi-monitor acceptance needs a multi-monitor Windows setup. Physical input/display/game acceptance remains pending.
- [ ] Clean-runtime and independent recovery evidence obtained when a runnable
  Windows spike exists; static CRT/OS imports are supporting, not proof.
  Sandbox use stays deferred to Phase 4 clean-runtime and guest-only work.
  Host input/game gates remain pending until user-tested.
- [ ] scripts/windows-hide-proof.ps1 source-hash manifest coverage
  reviewed/updated after module splits/moves (especially moves outside the
  recursive roots); per-file Get-FileHash rows are UPPERCASE and the
  combined Sha256-String digest lowercase. Preserve exact casing in
  reproduction/comparison: row text casing affects the combined digest.
- [ ] Records and backlog can lag code; verify delivery in code before
  implementing an item (example: press-focus R-DRAG-08 was already
  delivered).

## Proposed Windows global AGENTS.md section

**P:** append to the user's existing global rules, not replace them. Root
dependency/live-testing amendments remain separate user decisions.

```markdown
## Native Windows

- Use native PowerShell 7 Core for shell calls; verify $PSVersionTable in a tool call.
- No WSL, MSYS/Git Bash default shell or GNU Rust target for native builds.
- Use dedicated read/search/edit tools; otherwise PowerShell and named native tools.
  Call .exe explicitly when PowerShell aliases would change semantics. No assumed sed/awk.
- Dependencies follow the repo's Windows list; ask before installs, git-config,
  PATH, profile, registry/policy, security-setting or external-file changes.
- Build/test portable packages explicitly; root justfile/KWin/Nix gates run on Linux.
  Shared changes need applicable Linux evidence before acceptance.
- Do not execute input hooks, window movement/hiding or policy experiments without
  the approved Windows live-testing protocol and explicit scoped user authorization.
  Inspect build scripts/test targets before running; builds can execute code too.
- Prove independent stop/restore and out-of-hook recovery on owned test windows first.
  Never pause a hooking process in a debugger; no policy writes outside Sandbox.
- No broad deletes, security exclusions or changes to boot/session setup.
  Use the exact approved resource identity; ask on ownership/restoration ambiguity.
```

## Windows live testing

Before live Windows work, read and follow [live Windows testing](live-windows-testing.md); it does not grant mutation authorization.

## Risk table

| Risk | Trigger | Containment | Verification |
| --- | --- | --- | --- |
| CRLF/normalization churn | Git installer defaults or editor conversion | Proposed attributes, per-clone LF before checkout | Config origins, attributes, ls-files --eol, diff check |
| Wrong linker/native tool | Coreutils/Git usr-bin ahead in dev PATH; explicit override | Disable Coreutils link; Git Cmd PATH; VS discovery | Ordinary build plus first MSVC linker in x64 VS shell |
| Dependency/pin drift | Nix-only rule on Windows; rustup stable | Windows list in this doc, latest stable pre-1.0 (revisit at 1.0) | Versions/paths, workload/SDK inventory, both OS gates |
| Deep target path failure | Rust dependencies/build script uses legacy path API | Short repo/cache paths; Git longpaths is partial | Actual native build, OS policy readback |
| Desktop/input stranded | Hook hang, hiding, forced crash | Owned windows, independent restore/kill proven first | User live protocol and crash-recovery readbacks |
| Lock loss / invalid guest evidence | Win+L policy or redirected keyboard | Guest-only policy; no host writes; defer if unavailable | Guest exact restore; physical chord remains unproven |
| AV block / SmartScreen reputation | Low-prevalence unsigned builds; detection | Keep Defender/SmartScreen on; inspect own hash/detection, submit false positive | Windows Security history and Microsoft submission [W13]; **U** hook-specific keylogger heuristic, not guaranteed |
| Slow build | Scan-heavy repo/cache | Optional trusted Dev Drive performance mode, no broad exclusions | Drive query, Defender UI, measured build [W3] |
| Extra runtime dependency | Dynamic CRT/dev DLLs | Future static CRT and artifact-only clean guest/machine | dumpbin imports plus clean launch; hosted CI insufficient |
| Tool environment mismatch | shell resolution, rg bootstrap, git cache/path issue | PS7 with no explicit opencode shell path, official CLI, fresh smoke test | Tool-call Core/version/path, dedicated search |

## Sources

Unless noted otherwise, sources were accessed **2026-09-30**. The
`Microsoft.PowerShell` package ID was confirmed **2026-10-10** with read-only
`winget show --exact --id Microsoft.PowerShell --source winget`; no VS 2026
winget ID was confirmed. Primary docs/source verify mechanisms; issue
reports do not prove current-host behavior. Local inspection is **R**, not a
Windows execution result.

- G1: [Git attributes](https://git-scm.com/docs/gitattributes).
- G2: [Git installer options/defaults](https://gitforwindows.org/silent-or-unattended-installation.html), [installer source](https://github.com/git-for-windows/build-extra/blob/master/installer/install.iss).
- G3: [Git for Windows core.longpaths source docs](https://github.com/git-for-windows/git/blob/master/Documentation/config/core.adoc).
- R1: [devenv Rust](https://devenv.sh/languages/rust/), [module implementation](https://github.com/cachix/devenv/blob/main/src/modules/languages/rust.nix), [rustup overrides](https://rust-lang.github.io/rustup/overrides.html).
- R2: [rustup MSVC setup](https://rust-lang.github.io/rustup/installation/windows-msvc.html), [rustup installer arguments](https://rust-lang.github.io/rustup/installation/other.html).
- R3: [rustc linker discovery](https://github.com/rust-lang/rust/blob/master/compiler/rustc_codegen_ssa/src/back/linker.rs), [find-msvc-tools 0.1.14](https://docs.rs/find-msvc-tools/0.1.14/src/find_msvc_tools/find_tools.rs.html) (especially lines 517-553), [cc](https://docs.rs/cc/latest/cc/).
- R4: [Rust CRT linkage](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes), [dumpbin /DEPENDENTS](https://learn.microsoft.com/en-us/cpp/build/reference/dependents?view=msvc-170).
- W1: [WinGet configure](https://learn.microsoft.com/en-us/windows/package-manager/winget/configure), [authoring/VS resources/UAC](https://learn.microsoft.com/en-us/windows/package-manager/configuration/create).
- W2: [PowerShell Windows installation](https://learn.microsoft.com/en-us/powershell/scripting/install/installing-powershell-on-windows).
- W3: [Dev Drive](https://learn.microsoft.com/en-us/windows/dev-drive/), [Defender performance mode](https://learn.microsoft.com/en-us/defender-endpoint/microsoft-defender-endpoint-antivirus-performance-mode).
- W4: [Windows long paths](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation).
- W5: [WinGet manifests](https://github.com/microsoft/winget-pkgs): [rg MSVC](https://github.com/microsoft/winget-pkgs/tree/master/manifests/b/BurntSushi/ripgrep/MSVC), [jq](https://github.com/microsoft/winget-pkgs/tree/master/manifests/j/jqlang/jq), [yq](https://github.com/microsoft/winget-pkgs/tree/master/manifests/m/MikeFarah/yq), [gh](https://github.com/microsoft/winget-pkgs/tree/master/manifests/g/GitHub/cli).
- W6: [just Windows installation](https://just.systems/man/en/packages.html), [qsv Windows release assets](https://github.com/dathere/qsv/releases).
- W7: [Microsoft Coreutils overview](https://learn.microsoft.com/en-us/windows/core-utils/overview), [preview/manager/shell conflicts](https://github.com/microsoft/coreutils), [first release](https://github.com/microsoft/coreutils/releases/tag/v2026.5.29).
- W8: [Previously used VS 2022 route's workloads/components](https://learn.microsoft.com/en-us/visualstudio/install/workload-component-id-vs-build-tools?view=vs-2022); see §4 for the selected VS 2026 route and its native offline validation.
- W9: [VS native command-line environments](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170).
- W10: [GitHub hosted runner images/software](https://github.com/actions/runner-images).
- W11: [Sandbox editions/requirements](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/), [installation](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-install), [configuration/isolation/persistence](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file).
- W12: [LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc), [Windows shortcut/security path context](research/windows-port/plan.md#shortcuts-and-host-setting-conflicts).
- W13: [Defender false positives](https://learn.microsoft.com/en-us/defender-endpoint/defender-endpoint-false-positives-negatives), [submission](https://www.microsoft.com/wdsi/filesubmission), [SmartScreen reputation](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).
- O1: [Native install docs](https://opencode.ai/docs/), [official CLI releases](https://github.com/anomalyco/opencode/releases), [Windows guidance](https://opencode.ai/docs/windows-wsl).
- O2: [opencode config/shell](https://opencode.ai/docs/config/), [global rules](https://opencode.ai/docs/rules/), [published schema](https://opencode.ai/config.json).
- O3 (**O**): [Store shell resolution #41426](https://github.com/anomalyco/opencode/issues/41426), [rg extraction #24489](https://github.com/anomalyco/opencode/issues/24489), [git plugin cache path #22280](https://github.com/anomalyco/opencode/issues/22280), [Git/PATH #21826](https://github.com/anomalyco/opencode/issues/21826).
- O4: [Windows child-process .cmd limitation](https://nodejs.org/api/child_process.html#spawning-bat-and-cmd-files-on-windows).
- **R:** `Cargo.toml`, `crates/*/Cargo.toml`, FFI Rust sources,
  `crates/omnitiler/src/planner_service.rs`, `devenv.nix`, `devenv.yaml`,
  `.github/workflows/ci.yml`, `justfile`, `kwin/package.json`,
  `kwin/tests/{trace,source-rev,artifact-smoke,refresh-quiet-trace}.test.ts`;
  `git ls-files --eol`; verify target/sysroot inventory on the host.
- **U remaining:** actual tool/plugin/permission behavior on the chosen opencode version,
  exact Sandbox display/input behavior, security detection, deep target paths,
  clean runtime closure and all physical shortcut/display/game acceptance.
