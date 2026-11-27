# Scope and platform goals

### Platform scope
- Decision: initial Windows target is Windows 11 x64; Windows/macOS require managed per-monitor workspaces for feature completeness, though tiling-only previews are allowed. macOS work started 2026-10-14; KDE 0.1 does not block it. macOS uses public and private window-management APIs with SIP enabled; no reduced-SIP or Dock-injection tier.
- Why: not recorded.
- Assumes / revisit if: Windows distribution channel remains selected by most-obvious/unsurprising criterion after research; macOS follows the [0.1 distribution decision](#macos-01-distribution). Develop on native Windows, not VM; retain NixOS/Linux flow. Multi-monitor live work requires multiple displays.
- Status: active, 2026-09-30; deeper SIP-off tier permanently rejected 2026-11-27.
- Evidence: [Windows plan](../research/windows-port/plan.md), [macOS kickoff](../changes/archive/macos-port-kickoff.md).

### Windows safety and sandbox
- Decision: elevated/admin windows stay unmanaged by default and float in native position while others tile; elevated tiler opt-in may be considered, not default. Phase 1-3 proof runs in native Windows session with owned disposable windows first; Sandbox deferred to Phase 4.
- Why: not recorded.
- Assumes / revisit if: elevated-tile mode remains future opt-in question.
- Status: active, 2026-09-30
- Evidence: [Windows plan](../research/windows-port/plan.md).

### macOS Phase 0 and host foundations
- Decision: all Rust including UI; unsafe exception policy per [Rust and Memory Safety](../principles.md#rust-and-memory-safety). Main thread AppKit/UI only; dedicated input thread with minimal tap callbacks, per-app AX thread, engine thread owning core, channels between; hung AX app cannot stall others. Ctrl default/configurable Meta; preserve per-binding conflicts and Compatible/Authentic presets; Mission Control Ctrl+arrow/digits expected conflicts. Apple Silicon only, macOS 15 minimum (TD6); the [2026-10-14 Homebrew 30-day OS events](https://formulae.brew.sh/analytics/os-version/30d/) sample (period starting 2026-09-14) had 12,461,979 macOS 15+ events / 13,361,038 macOS events (93.27%); newer-only coverage was 71.72%. This opt-in event-weighted sample is not all active Mac users or an arm64-only panel. [StatCounter](https://gs.statcounter.com/os-version-market-share/macos/desktop/worldwide) warns modern releases are misreported as Catalina; AX/Carbon/tap APIs predate 15 and yabai private-Space workarounds cover 15, but private APIs need per-release tests and a newer development OS does not validate the floor. Revisit with better population data. Persistent self-signed development identity initially, stable bundle ID/path/designated requirement, no paid membership initially; use the separate [release distribution decision](#macos-01-distribution). Owned probes retain exact Child, verify PID, per-run pipe nonce, executable and fresh CG owner-PID/window association; separate commands also require a current nonce-title match and unreadable titles fail closed. No LaunchServices launch-date token or alternative process-start machinery. Phase 0 uses capability-first spikes, one registration and one interception hotkey path from the non-native host-interaction research. Live work follows macOS guide.
- Why: not recorded.
- Assumes / revisit if: macOS 15 native behavior is unverified until the [required pre-release VM check](macos.md#macos-0-1-scope-and-gate); multi-display on 15 is not proven by that check. Native/private APIs and OS floor remain subject to per-release validation; Homebrew data limitation remains.
- Status: active, 2026-10-03; floor reaffirmed and release check required 2026-11-27.
- Evidence: [macOS dev setup](../macos-dev-environment.md#persistent-self-signed-development-identity), [live guide](../live-macos-testing.md), [Phase 0](../changes/macos-phase-0.md).

### macOS 0.1 release scope
- Decision: 0.1 includes KDE Plasma, Windows and macOS; goal feature parity with KDE as sensible per host. Common 0.1 gate categories are VISION reliability, gaming/fullscreen noninterference, core journeys, failure/missing companion leaves tiling working, settings/restart. Each non-KDE platform proposes applicable checks, host exceptions and extras for user approval; parity is subject to approval.
- macOS 0.1 ships only native Spaces with SIP enabled, in [full multi-display scope](macos.md#macos-0-1-scope-and-gate). If a feature cannot conform in native Spaces, document a Spaces-mode limitation. Exactly two workspace modes are approved: SIP-on native Spaces and virtualised workspaces. After 0.1, virtualised becomes the fresh-install default only once its safety gates pass; native remains selectable and becomes opt-in. This does not select a migration policy for existing installations.
- Before flipping the default, require an out-of-process watchdog with a durable pre-park frame journal that restores after `SIGKILL`; overlap-minimising edge/corner parking with a global outermost-corner fallback; “Displays have separate Spaces” kept on; a focus fence for empty workspaces; and read-only native Space and fullscreen exclusion.
- Why: preserve native compatibility without making its limitations a reason to weaken system security. Gate the virtualised default on recovery and noninterference, not just switch speed.
- Rejected: permanent SIP-off/Dock scripting-addition tier. Dock code injection violates the [anti-cheat rule](gaming.md#anti-cheat-mechanism-limits) and conflicts with [notarized Developer ID distribution](#macos-01-distribution).
- Assumes / revisit if: exact host parity is not presumed. Virtualised safety gates and native release probes are requirements, not observed passes.
- Status: active, 2026-11-27; supersedes the earlier undecided virtualised-mode wording.
- Evidence: User 2026-11-27; [mode review and gated default](../changes/macos-virtualised-workspaces-mode.md); [approved common gates](../changes/windows-0.1-gate-proposal.md#user-decisions).

### macOS 0.1 distribution
- Decision: use Developer ID signing and Apple notarization; distribute a DMG on GitHub Releases and via Homebrew Cask. No Mac App Store and no in-app updater in 0.1.
- Status: active (TD5), 2026-11-26. Installed lifecycle and release-artifact acceptance remain separate.

### KDE 0.1 release gate and delivery
- Decision: KDE 0.1 gate includes sleep/wake, output plug/unplug, resolution/scaling, fullscreen apps and underlying configuration changes; gaming/fullscreen noninterference; tiling, focus/move, workspaces, multi-output, border, shortcuts, settings/restart; missing/failed/removed effect leaves tiling working and KWin stable. Other checks may ship only as listed known issues with no crash/silent tiling stop; no separate dogfood period. Triage only 0.1-relevant requirements before release. 2026-10-12 approved 57 checks: 43 must-pass, 12 known-issue-allowed, 2 Windows-release; D28 native fullscreen send including real game counted once; multi-output must-pass needs two outputs; known issue rules above. On 2026-10-18 this gate template became common cross-platform.
- Why: not recorded.
- Assumes / revisit if: Windows 0.1 gate accepted separately: base 45 M/7 K/5 excluded, WX1-WX8 must-pass, only Authentic Win+G/Win+F11 exact-chord containment listed limitation; native/physical acceptance and delivery pending. This is not release acceptance.
- Status: active, 2026-10-10
- Evidence: [triage](../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2), [Windows gate](../changes/windows-0.1-gate-proposal.md#user-decisions).

- Decision: KDE install paths: Nix flake/Home Manager, GitHub Release, OBS distro packages; KDE Store excluded from 0.1. OBS release tags to one stable repo; openSUSE Tumbleweed, Fedora 43/44 updates, Arch via AUR PKGBUILD (not OBS pacman); Ubuntu 26.04/26.10 selected; KDE neon attempt ended in 2026-10-18 delivery decision. External NixOS/Home Manager validation and mainstream packaging P1. User-operated physical second-monitor PC, no VM, for multi-output development/must-pass testing; also hotplug/sleep-wake; check 2 still needs third output; general reference environments parked.
- Why: not recorded.
- Assumes / revisit if: Windows package/update channel deferred to Windows release planning and short research refresh.
- Status: active, 2026-10-10
- Evidence: [two-output route research](../research/two-output-test-environment.md), [KDE support floor](kde.md#kde-support-floor).
