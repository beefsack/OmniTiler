# Backlog

Only meaningful pending or active work is listed.

Live runs are user-owned; agents do not run live cases
([testing](live-kwin-testing.md)). Architecture review claims must be verified
before acting ([review](research/architecture-review/review.md)); user
decisions of 2026-09-24 are under
[Architecture Direction](decisions.md#engine-architecture-and-convergence).

## Open work

- Next implementation order (user review 2026-10-13): D23 best-effort restore
  rework plus close cleanup delivered offline ([evidence](changes/archive/d23-rework.md));
  D16 ordinary/combo floating sends delivered offline ([evidence](changes/archive/d16-floating-transfer.md));
  Born-maximized core policy and KDE conformance delivered offline
  ([evidence](changes/archive/born-maximized-shared-policy.md));
  KDE underlay default delivered offline ([evidence](changes/archive/kde-dogfood-underlay-options.md#8-kde-default-delivery-2026-10-13));
  next code: unsafe planner explicit-config
  rework plus crate-wide forbid, third-party notices, then KWin-to-Rust
  migration research (delivered below). Native verification remains queued.
- P2 | KWin-to-Rust migration research | [Options and recommendation](research/kwin-rust-migration.md)
  delivered 2026-10-13; recommendation pending user review. Informs post-0.1
  Push code down and KDE interaction performance/threading; no implementation
  or 0.1 schedule change.

- P0/P1 | 0.1 release (KDE Plasma) | Existing MVP, KDE only (user 2026-10-10;
  [scope](decisions.md#scope-and-platform-goals)). Must-pass gate: VISION
  reliability (sleep/wake, output plug/unplug, resolution/scaling changes,
  fullscreen applications, underlying configuration changes), gaming/fullscreen
  non-interference, core journeys (tiling, focus/move, workspaces, multi-output,
  border, shortcuts, settings, restart), and missing/failed/removed effect
  leaves tiling working and KWin stable. Other live checks may ship as listed
  known issues provided none crashes or silently stops tiling; no separate
  dogfood period. Includes these pending pieces:
  - [OPEN-requirement triage and live-check gate](research/release-0.1-triage.md):
    User 2026-10-10 approved recommendation (a) for 15 units: D02, D04, D05,
    D06, D07, D08, D11, D14, D15, D17, D18 (no setting), D19, D20 (no lower
    verb for now), D21, D24; 16 rows approved. User 2026-10-12 also approved
    D01, D03, D09, D10, D12, D13, D16, D22, D23, D25, D26, D27, D28 (seventeen rows).
    All 28 units and Proposal B / N1-N4 classification approved; triage review complete.
    Spec now 138 NORMATIVE / 0 PROVISIONAL / 28 OPEN; verification and
    implementation gaps are below.
  - Proposal B classification approved exactly User 2026-10-12: original 53
    checks = 39 must-pass / 12 known-issue-allowed / 2 Windows-release; N1-N4
    add four must-pass checks, total 57 = 43 / 12 / 2. K checks: 5, 24, 26,
    29, 35, 36, 37, 38, 40, 41, 52, 53; W checks: 3, 4; all others M.
    D28 KDE fullscreen-send acceptance including a real game is explicitly
    must-pass (User 2026-10-12), a sub-leg of the reference-comparison check;
    counted once within original check 1. Multi-output must-pass checks require
    a two-output setup (VM or hardware) and must pass before 0.1 can ship.
    Implementation and native/live acceptance remain pending.
  - Original 60 OPEN rows triaged: 32 relevant / 28 post-0.1. P2 settings now
    contain approved D09/D10/D27; D03/D12/D13/D16/D22/D23/D25/D28
    have no setting. [User decisions](research/release-0.1-triage.md#user-decisions-2026-10-12).
  - P1 0.1 packaging: offline recipes/archive/tag workflows and
    Revert-before-removal guidance prepared 2026-10-11; distro offline
    builds passed. Core Settings ownership approved and delivered 2026-10-11
    ([evidence](changes/archive/release-0.1-core-settings.md)); effect companion
    is effect-only. Remaining: OBS handoff decision (0.1 blocker status OPEN),
    accounts/provisioning, third-party notice delivery and runtime validation.
    License audit/CI gate and Ubuntu apt solver acceptance are complete. OBS/AUR names are
    `omnitiler` and `omnitiler-native-effect` (user 2026-10-11). KWin 6.6 support
    delivered offline: both Debian packages built from one archive on Ubuntu
    26.04 release/updates and 26.10; effect exactly pins its KWin distro build.
    Ubuntu live acceptance remains; neon provisioning remains blocked.
    [Compatibility delivery](changes/archive/kwin-6.6-support.md).
    [Ubuntu research](research/distribution-package-feasibility/ubuntu-native-effect.md).
    [Evidence](changes/archive/release-0.1-offline-packaging.md).
  - External NixOS/Home Manager validation (P1 gate).
  - Workspace tiling toggle Meta+Y (KDE) delivered offline 2026-10-11;
    physical/restart/preset acceptance in Pending live checks. Win+Y remains
    Windows handoff item 21 ([record](changes/archive/kde-workspace-tiling-shortcut.md)).
  Windows continues as a development preview with its own later release;
  macOS starts after KDE 0.1 ships. KDE current approach retained; no new
  non-native-research items. Lessons persist linked
  ([candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [cross-cutting findings](research/non-native-tiler-host-interaction.md#cross-cutting-findings),
  [evidence](changes/archive/non-native-tiler-host-interaction.md)).
- P1 | Ubuntu 26.04/26.10 acceptance | KWin 6.6 floor selected by user
  2026-10-11 and delivered offline; release 6.6.4, updates 6.6.6 and 26.10
  6.7.5 two-package builds and full apt install/upgrade/removal solver acceptance
  passed ([solver evidence](changes/archive/ubuntu-apt-solver-acceptance.md)).
  User 2026-10-13 selected (b): exact effect pin retained; installation-guide
  note and Debian effect-package description explain the rebuild wait
  ([delivery](changes/archive/ubuntu-kwin-upgrade-note.md)). Remaining:
  user-owned shared-workspace/Settings/native-visual journeys on 6.6
  and per-output regression on 6.7+, OBS target provisioning/source handoff.
  Plasma 5 and KWin <= 6.5 remain unsupported.
  [Decision](decisions.md#kde-support-floor),
  [evidence and live checks](changes/archive/kwin-6.6-support.md).
- P1 | Licensing | Decision and declarations applied 2026-10-11: MIT default,
  GPL-2.0-or-later native effect, MIT Rust staticlib and Settings KCMs.
  [Research and dependency inventory](research/licensing.md). Third-party
  notice implementation delivered 2026-10-13: committed generated union,
  manual `just` regeneration, CI freshness lint, source archive and
  Debian/RPM/Arch/Nix installation. Pending: actual distro package
  builds/extraction; hosted CI execution;
  notice delivery in future GitHub binary archives (producer outside this
  change). [Evidence](changes/archive/third-party-notices.md). Audit gate delivered
  2026-10-12: keep `licenses`, `bans`, `sources`; per-distro closure and vendored
  inventory recorded; [evidence](changes/archive/licensing-audit-gate.md).
- OPEN | OBS release handoff redesign | User 2026-10-13 left OPEN; possibly
  later than 0.1 and possibly not a blocker, neither decided. No route
  selected. Prior recommendation remains research only. Candidate routes:
  SCM bridge (git recipes plus exact checksum, no OBS password in CI), scoped
  token triggering OBS-side asset fetch (including a version-free latest asset
  without pinned checksum), or Route 3 manual scripted `osc commit` per
  release. Candidate feasibility remains unresearched; narrow tokens cannot
  upload sources/checksums. [Research](research/distribution-package-feasibility/obs-release-handoff.md).
- P2 | [Push code down](principles.md#layering) | Recurring post-0.1 passes: move logic from host-coupled
  `kwin/src` TypeScript and `kwin/native-effect` C++ (including settings and
  shortcut reconciliation) into system-specific Rust, then into the agnostic
  shared core where appropriate. First pass after 0.1, aligned with or ahead
  of deferred K2/K3. Born-maximized core policy is delivered offline;
  user 2026-10-13 option C defers KDE rewiring to this pass. KDE retains its
  conformance-pinned TypeScript copy because clear/keep runs synchronously
  before dispatch and the planner protocol lacks the decision. Runtime
  protocol changes and build-time generation were declined
  ([record](changes/archive/born-maximized-shared-policy.md)).
  Link with KDE interaction performance/threading and
  [Live sibling reflow while dragging](#open-work).
- P2 | KDE interaction performance and threading | New post-0.1 research,
  not a current priority (user 2026-10-13; no significant KDE jank). Windows
  input/render threading drastically improved UX. Explore dynamic resizing
  during drag and drag/move/resize work in multithreaded Rust, C++/TypeScript
  thin shims, relying on Rust thread safety; include later platforms. First
  research KWin effect/script threading constraints; interaction with KWin may
  change. Linked with Push code down and Live sibling reflow while dragging.
- P2 | Smooth KWin upgrades | Future research (user 2026-10-13): patch-release
  effect loading compatibility (IID/version binding), OBS rebuild latency and
  pin options. Aim for the smoothest possible user upgrades; exact pin remains
  selected. [Decision](decisions.md#kde-support-floor).
- P1 | 0.1 triage decisions - batch 1 verification (KDE/shared) | User
  2026-10-10 approved the 15 units above, exactly recommendation (a) in
  [triage](research/release-0.1-triage.md#user-decisions-2026-10-10).
  - D05 / REQ-INS-05: verification-only (2026-10-11). The existing
    shared Engine convergence route retains prior tiled focus B while an
    ordinary float is focused; newcomer admission splits B's long edge rather
    than root-wrapping. Shared geometry/focus and KDE PlanAdapter wire fixtures
    prove this offline under unchanged admission policy; native observation/
    activation acceptance remains user-owned in Pending live checks.
    [Evidence](changes/archive/triage-batch1-kde-shared.md).
  - Observe-first, contingent implementation: D06 / REQ-OUT-05 currently
    admits on the newcomer's observed native output, with no focused-output
    override; verify focused output + newcomer activation. D17 / REQ-WS-09
    retains shared `last_active`, but KDE return focus is shell-driven; verify
    remembered B on return. Implement only if these native outcomes mismatch.
  - Verification-only: D02, D04, D07, D08, D11, D14 (both rows), D15, D18,
    D19, D20, D21, D24. Existing mechanisms/defaults match; exact native
    journeys remain pending in Pending live checks. D07's shared close/refocus/
    fresh-reopen fixture now proves fresh admission offline with no old-slot
    restoration; exact native ratio/order/focus evidence remains pending.
    D18 has no setting; D20 needs raise-order proof, not a lower action.
  - Windows: D05 shares the proven Engine route; D14's keyboard trigger gap is already
    handoff item 6 (include R-RSZ-02/03 acceptance there). D02's owned-dialog
    exclusion is the approved divergence; D17 remembered focus and D19 hide/
    reveal are already wired. No new Windows-specific handoff item warranted.
- P1 | 0.1 triage decisions - implementation and verification (KDE/shared) |
  D01, D03, D09, D10, D12, D13, D16, D22, D23, D25, D26, D27, D28 approved by
  User 2026-10-12. Historical gap assessment at `fafcd31`, D01 refined by
  [research](research/d01-fullscreen-admission.md) at `01be284`. D01 and D28
  are delivered offline ([record](changes/archive/kde-fullscreen-activation-and-send.md));
  native acceptance remains pending. [Selections and caveats](research/release-0.1-triage.md#user-decisions-2026-10-12).
  Verification-only means a matching mechanism exists, not native acceptance.
  Approved units include contingent fixes if live outcomes differ, subject to
  their recorded return-to-user conditions.
  Paths below are repository-relative; line locators are revision-bound.

  | Unit | Gap assessment | One-line reason / source |
  | --- | --- | --- |
  | D01 | delivered offline; native verification pending | KDE automatic convergence re-observes before focus and fences overlay/native-focus races; admission retains slots/state and explicit switching/shortcuts remain effective. Shared fixtures verify host-approved/denied admission and exact CLOSE-05 removal/refill/focus. Joint native admission/overlay-close cleanup pending ([record](changes/archive/kde-fullscreen-activation-and-send.md)). |
  | D03 | verification-only | After-focused `[old,new]` and per-admission focus anchor exist (`crates/tiler-core/src/session.rs:2218-2231`, `crates/tiler-core/src/session/ops/lifecycle.rs:374-384`); chained legs 2-3 need live pointer/focus proof. |
  | D09 | verification-only | R-MOV-06 two-child neighbor uses middle index `len/2` (`crates/tiler-core/src/directional.rs:907-918`, `crates/tiler-core/src/session/ops/move.rs:897-905`); exact native fixture pending. |
  | D10 | verification-only | Orthogonal escape continues through R1 wrap (`crates/tiler-core/src/directional.rs:1003-1007`, `crates/tiler-core/src/session/ops/move.rs:1089-1140`). |
  | D12 | verification-only | Same-axis escape inserts into the ancestor axis (`crates/tiler-core/src/directional.rs:993-1002`, `crates/tiler-core/src/session/ops/move.rs:1080-1087`). |
  | D13 | verification-only | Local move wins before output crossing (`crates/tiler-core/src/directional.rs:1046-1088`); OUT-02 local-wrap multi-output fixture remains live-pending. |
  | D16 | delivered offline; native verification pending | Shared-core/KDE ordinary and floating+fullscreen/maximized sends use slotless MoveDeferred; both flags retained, no geometry/overlay writes or source/destination tile reflow. Real numbered/relative follow/stay and destination exit regressions pass; exit remains floating at the remembered frame. Sticky/automatic-fixed refusals unchanged; no setting. Native roundtrip/combo exit pending; Windows adapter item 22 ([record](changes/archive/d16-floating-transfer.md); [decision](decisions.md#workspaces)). |
  | D22 | approved; verification-only | Float-out preserves survivor proportions (50/25/25, float middle 25 -> approximately 67/33); no setting. Removal retains survivor share ratios (`crates/tiler-core/src/cosmic_v1.rs:175-198`, `crates/tiler-core/src/session.rs:2398-2399` at `fafcd31`, revision-bound). COSMIC observed, not source-proven, plus i3/sway Tier 1; thin i3/sway one-family basis disclosed; no evidenced alternative, Hyprland/Tier 2 untraced. Native reflow live check pending. [Decision](decisions.md#window-state-float-sticky-maximize-fullscreen). |
  | D23 | delivered offline; native verification pending | COSMIC `3d55cba0` source gate passed. Shared-core best-effort neighbor rejoin uses stored group/index/side/share hints; additive `closed_windows` clears closed slots through KDE, including hidden domains. Minimize/reflow/MRU, no-focus-write restore and sole null focus preserved; session-only. Required regressions and full offline gates pass; Windows adapter remains item 22. [Evidence](changes/archive/d23-rework.md), [decision](decisions.md#window-state-float-sticky-maximize-fullscreen). |
  | D25 | approved (a); verification-only | Keep host-owned set; re-observe host membership, no moves; focus only where provable; rebuild layout by re-adoption, no saved-layout restore; intentional floats stay floating via existing restart store; no setting. Generic fresh snapshot/rebuild/converge supports membership/provable focus (`kwin/src/plan-adapter-entry.ts:2186-2300`, `crates/tiler-core/src/session/world.rs:746-799` at `fafcd31`, revision-bound). Native dogfood/live checks will assess orderly restart; outcomes pending. [Decision](decisions.md#restart-persistence). |
  | D26 | approved (b); verification-only, no code | Restore already-persisted owned gaps/settings; re-observe host set/membership/maximize, no invented layout restore. Gaps/settings reload exists (`kwin/src/domain-gap.ts:90` at `fafcd31`); intent cannot cross login/bus/KWin namespace (`crates/omnitiler/src/float_intent_store.rs:11,30-34`). Listed 0.1 known limitation: intentional-float memory lost on logout/login, reopened apps new identities; unchanged REQ-RST-01d lifetime. Revisit after 0.1 with shared restart store. Native dogfood/live checks will assess; outcomes pending. [Decision](decisions.md#restart-persistence). |
  | D27 | KDE verification-only; no longer BLOCKED | User 2026-10-13 accepts native KWin host exception: valid-token/FromTool/allowed-X11 switches + focuses, missing/stale-token demand-attention; converge without intercept/revert/suppress. No KDE gaming guard; rely on native input freshness. Real-game active and idle/paused check must pass; reopen only on actual active-play switch-away. Windows Mark urgent only/guard unchanged; setting KDE applicability deferred to design ([research](research/d27-kwin-activation-trace.md); [decision](decisions.md#activation-requests)). |
  | D28 | delivered offline, including born-fullscreen option (a); native/game verification pending | Explicit numbered/relative follow/stay carries retained fullscreen tiles and never-tiled slotless members. Shared MoveDeferred rehomes the initial hold without tree mutation, source admission or geometry setters; D5 first exit admits on the destination. Fullscreen flag and existing send fences retained. Native KDE journey including a real game is must-pass; stop if KWin drops/corrupts fullscreen. No setting; native outcome TBD ([record](changes/archive/kde-fullscreen-activation-and-send.md); [decision](decisions.md#window-state-float-sticky-maximize-fullscreen)). |

  - D16 verification queue: ordinary and approved floating+fullscreen/maximized
    sends delivered offline in shared core/KDE ([record](changes/archive/d16-floating-transfer.md)).
    Native R-WS-05 roundtrip (WS1 -> WS2 -> WS1), numbered/relative follow/stay,
    sole-window source/empty target, retained float/overlay flags and exact
    same-output frame remain pending. Verify zero geometry/overlay writes,
    unchanged source/destination tiles, restart float memory and later overlay
    exit returning to floating frame. Sticky/automatic-fixed refusals unchanged;
    no setting. Windows adapter/native acceptance follows in item 22.
  - D22 verification queue: native R-FLT-03 float-out reflow live check remains
    pending; surviving siblings must preserve their relative shares. Shared
    removal already matches; verification-only, no setting.
  - D23 verification queue (user 2026-10-13): best-effort rejoin and explicit
    close-slot cleanup delivered offline after matching COSMIC `3d55cba0`
    source verification ([evidence](changes/archive/d23-rework.md)). Native
    journeys remain pending: focus change, resize, new window, sibling close,
    swap, other minimize/restore, gaps/resolution, restart and close cleanup;
    verify same side/group/index where possible, comparable old share,
    session-only lifetime and no restore focus write. Minimize/reflow/MRU and
    sole-none are preserved. Windows adapter follows in item 22.
  - D27 KDE verification queue: native valid-token switch convergence,
    missing/stale-token mark-only without interference; FromTool/allowed-X11
    native behavior. Must-pass real-game active play and idle/paused check,
    tiler-off/on comparison; rely on KWin freshness, no tiler guard. Reopen
    only on actual active-play switch-away. Included in reference-comparison
    check 1, counted once; native evidence TBD.
  - D25/D26 verification queue: native dogfood/live checks will assess orderly
    owner restart with apps alive and end-session restore. D25 keeps host set,
    observes membership without moves, restores only provable focus and
    intentional floats via existing store; re-adoption rebuilds layout. D26
    reloads persisted gaps/settings and observes host set/membership/maximize;
    cross-login intentional-float loss is the approved 0.1 limitation.
    Verification-only; no saved-layout restore or D26 code work.
  - D01/D28 verification queue: compare tiler-off/on native admission over
    maximized/fullscreen B, host-approved/denied activation and explicit
    switching; verify overlay-close cleanup. D28 must-pass native acceptance
    covers numbered/relative follow/stay and a real game, both retained and
    never-tiled born-fullscreen. The latter stays slotless with neither source
    nor target reflow until D5 exit admits on the destination; include sole
    held source and empty target. Record native flag/membership/view/focus,
    geometry and visible/input state; stop and return to the user if KWin
    drops/corrupts fullscreen.
  - Windows impact: shared core fixes serve Windows; Linux work owns KDE/core,
    Windows adapter/settings/native work goes to handoff item 22 below.
    D23 Windows retained allocation is a divergence to change to the approved
    release/reflow target, not an approved exception. D28 KDE carry aligns with
    approved Windows carry (item 9). No Windows code changes here.
- P2 | 0.1 triage settings follow-up | User 2026-10-12 approved post-0.1
  D09/D10 with two options each; functional names/options/WM tooltips are in
  [Move decisions](decisions.md#move-layout-and-output-commands).
  Approved D27 `Activation request`: `Mark urgent only` (default; tooltip
  Hyprland, i3, sway) / `Switch and focus` (tooltip COSMIC, xmonad), Windows
  gaming guard in both modes. KDE applicability is reviewed when the setting
  is designed; no decision now. Functional names/options; WM names only in tooltips.
  Qualifies via COSMIC Tier 1 plus xmonad Tier 2 under the
  [tier rule](decisions.md#reference-tiers); [activation decision](decisions.md#activation-requests).
  Shared core policy settings serve Windows; Windows schema/UI/live wiring
  belongs to handoff item 22, coordinated after shared delivery.
- P0 | Windows dogfood 2026-10-11 | User live run `tile --user-start --trace`
  at `7f407ca` (log `%LOCALAPPDATA%\omnitiler\session-1\run-01dd5920bbba8256.log`).
  Passed: TOEM 2 unimpeded across exclusive/borderless/maximized/windowed
  modes and hide/reveal across a workspace switch. Windows order (user):
  (a) stop blocked by an open first-run preset prompt: stop must cancel the
  pending prompt with no preset written; (b) DEFECT: returning to WS1 did not
  refocus the previously focused fullscreen game (Steam took focus; REQ-WS-09);
  (c) DEFECT: group underlay too short in `V[W1 H[W2 W3]]` with W2 focused
  (bottom of screen to ~60% of W2); (d) born-maximized setting, default tile
  ([decision](decisions.md#window-state-float-sticky-maximize-fullscreen)
  R-MAX-06 CHANGED 2026-10-11); (e) border radius default 6
  ([decision](decisions.md#visuals-border-underlay-and-grouping)); then
  handoff items 10+18, 11+15 after offline-delivered 13+17. Later: (f) Win+B
  default browser and
  Win+T Windows Terminal launchers ([decision](decisions.md#shortcuts-conflicts-and-presets));
  (g) underlay visibility: user used `#50909090`, needs a brighter default and
  research into blend options visible on black, white and grey. Win+G/F11
  leakage observed again (known, parked under gaming coexistence).
  - (f) Offline-delivered 2026-10-12 (`b8edd14`, `8e56546`, CI green):
    Win+B opens the system-default browser via shell `open` of bare
    `https://`, Win+T launches `wt.exe` (missing Terminal logs and
    continues); both tentative, pending user review. Physical check pending:
    Authentic launches, Compatible inert, rebind/disable
    ([record](changes/archive/windows-launcher-shortcuts.md)).
  - (a) Offline-delivered: pending first-run prompt cancels on graceful stop,
    with no settings publication; native dismissal/teardown live check pending
    ([record](changes/archive/windows-first-run-stop.md)).
  - (b) Offline-delivered: verified departure focus now feeds remembered return
    focus, including fullscreen members; game focus/display-mode live check
    pending ([record](changes/archive/windows-workspace-fullscreen-focus.md)).
  - (c) Offline-delivered: group projection now honors the same minimum hints
    as tile placement; exact trace replay and nested-layout regression pass.
    Underlay appearance live check pending
    ([record](changes/archive/windows-group-underlay-geometry.md)).
  - (d) Offline-delivered: `core.born_maximized`, default `tile-on-launch`
    and working `keep-maximized-overlay`; live changes affect subsequent
    admissions only. Both-values maximized Paint launch user-accepted physically
    2026-10-12, scoped normal owner, single output, Notepad/Paint, base `e506350`
    plus combined directional/retile/B9 changes. Browser restore, detailed
    Settings, fixed-size, game/DPI/multi-output checks pending
    ([record](changes/archive/windows-born-maximized-setting.md)); R-MAX-03
    overlay parity is delivered offline with item 11; fixed admission is
    delivered with 13+17 below.
  - (e) Offline-delivered: active border radius defaults to 6 in Windows
    settings and CLI/proof owners; explicit radii preserved. Radius 6 appearance
    and live Settings Apply user-accepted physically 2026-10-12 under the same
    conditions as (d); Revert/game/DPI/multi-output pending
    ([record](changes/archive/windows-border-radius-default.md)).
  - Items 13+17 offline-delivered: bounded min/max observations, predicate
    schema/UI, current-hint fullscreen exit/enable classification and fixed-origin
    restart markers. Native hints/overlay/game no-touch acceptance pending;
    hydrated fixed-override maximize routing is tentative
    ([item 13](changes/archive/windows-fixed-size-admission.md),
    [item 17](changes/archive/windows-per-axis-zero-predicate.md)).
  - Items 10+18 offline-delivered 2026-10-12: ordinary/sticky float directional
    focus and stateless half-snaps; maximized focus fence and one observed-clear
    unmaximize-before-move. User-accepted physically 2026-10-12 under (d)'s
    conditions: float focus/half-snaps, maximized tiled focus fence, maximized
    float clear then all four halves, maximized tiled clear then move (sensible
    to user). Tie/sticky/clamp/preset variants, failure/race, fixed-size,
    game/DPI/multi-output pending
    ([item 10](changes/archive/windows-floating-directional-navigation.md),
    [item 18](changes/archive/windows-maximized-directional-navigation.md)).
  - Items 11+15 offline-delivered 2026-10-12: R-MAX-03 workspace retile retains
    maximize over a reserved slot under both born settings; ordinary maximized
    unfloat clears/observes before fresh admission with fixed D3/D7 overrides.
    Item 11 retile-preserves-maximize/restore-to-slot user-accepted physically
    2026-10-12 under (d)'s conditions; tray toggle does not restore previous
    window focus (observation only; may suit tray keyboard accessibility).
    Item 15 ordinary clear/admission/focus already has scoped native evidence;
    physical B9, hidden retile, session restore, fixed marker/restart,
    failure/race, game/DPI/multi-output pending
    ([item 11](changes/archive/windows-maximized-floating-retile-overlay.md),
    [item 15](changes/archive/windows-maximized-intentional-unfloat.md)).
- P0 | Windows owner threading | User direction 2026-10-12: rendering moves
  off the input hook thread so LL mouse/keyboard hook delivery never waits on
  owner work, done now while the owner is still small. Evidence: edge
  drag-resize feels ~30-60 Hz with the owner running versus smooth at the
  display refresh without it, after the border-raster fix; LL hooks share one
  pump with observation, Engine, actuation, overlay rendering and trace I/O.
  Recommended target C, staged: (1) dedicated input thread for LL hooks plus
  the synchronous consume/pass classifier over published snapshots; (2)
  dedicated render thread owning border/underlay/drop-preview windows,
  latest-wins desired state with hide/cancel epochs; (3) combined
  stress/recovery, then multi-output/DPI and gaming-cost checks. One
  observation/Engine/recovery owner remains. User selected target C
  2026-10-12: stage 1 (complete A) first, then stage 2 (B) early. Selected for
  latency only; parked Win+G/F11 containment is not
  claimed. Stage 1 offline-delivered 2026-10-12 (`ba2eb66`, CI green): both
  LL hooks and classifiers on a dedicated input pump. User physical
  2026-10-12 (dev, trace, Notepad scope): edge drag-resize much faster, still
  short of tiler-off; graceful stop clean. Win+arrow felt slightly slower; cause
  was a missing owner wake (up to 100 ms poll), fixed in `21a9e58` and
  user-confirmed physically
  ([record](changes/archive/windows-input-thread.md),
  [wake fix](changes/archive/windows-input-owner-wake.md)). Release and
  trace-off comparison, Win+drag/Esc and preheld-modifier checks pending.
  Stage 2 offline-delivered 2026-10-12 (`8fe0224`, CI green): border,
  underlay and drop preview on a dedicated render thread with latest-wins
  generations, safety epochs and acknowledged hide before drop actuation;
  agent live evidence on scoped Notepad/Calculator/Paint (carrier placement,
  preview hide-before-drop, clean stop). Tentative: 500 ms hide-proof timeout
  refuses that drop without writes. User physical check pending (pixels,
  follow, z-order, cross-slot drop, Esc, stale-show, edge-resize, Win+arrow)
  ([record](changes/archive/windows-render-thread.md)). Stage 3
  single-output hardening delivered 2026-10-12 (`baee279`, `2fce63f`, CI
  green): fixed repeated-stop panic, false-clean stop after thread panic,
  10 ms input/render idle polling, stranded final callback/retained input and
  renderer stop wake; agent live bursts and five clean start/stop runs; idle
  trace-on dev ~1% of one core with owner maintenance still periodic
  ([record](changes/archive/windows-owner-threading-acceptance.md)).
  Remaining: user physical single-output check, multi-output/mixed-DPI
  (after multi-output foundation) and gaming-cost legs. [Design](research/windows-port/owner-threading.md),
  [evidence](changes/archive/windows-resize-hook-latency.md).
- P1 | Rust `unsafe` policy | User rule 2026-10-13: only tiny ABI/FFI
  boundaries in one dedicated FFI module per crate; every other module and
  crate uses `#![forbid(unsafe_code)]`. Audit at `2af85a7`:
  `tiler-core` and `tiler-protocol` have none; stage 1 thread handoff
  (`input_handoff.rs`) has none and uses no `unsafe impl Send/Sync`,
  `static mut`, `UnsafeCell` or cross-thread raw pointers; remaining
  `tiler-windows` and `tiler-kwin-effect-ffi` uses are Win32/C FFI calls and
  hook/window procedures, which Rust cannot express without `unsafe`.
  Guardrail applied 2026-10-12: `#![forbid(unsafe_code)]` on `tiler-core`, `tiler-protocol`
  and unsafe-free Windows modules (`25de29a`; workspace build/test, clippy
  `-D warnings` and fmt pass). Linux planner delivered 2026-10-13:
  five unsafe env set/remove call sites replaced by explicit trace inputs,
  preserving `OMNITILER_TRACE` semantics; all three planner target roots now
  forbid unsafe. Audit also closed 29 unsafe-free core/protocol test/example
  root gaps. Workspace tests: 1,644 passed, zero failed/ignored; all-target
  clippy `-D warnings` and fmt pass
  ([record](changes/archive/linux-planner-unsafe-guardrail.md)).
  Windows FFI consolidation into dedicated modules is required after
  its current release-gate work (timing tentative), in the Windows queue.
  KWin effect FFI stays a minimal boundary module.
- P1 | KDE follow-ups from 2026-10-11 Windows dogfood | Delivered
  offline: R-MAX-06 born-maximized setting (default tile), active border
  radius 6. Born-maximized shared-core policy delivered offline with one
  Rust/KWin conformance table ([record](changes/archive/born-maximized-shared-policy.md)).
  User 2026-10-13 option C amends immediate KDE rewiring: keep its pinned copy
  and defer rewiring to post-0.1 Push code down; behavior, labels and accepted
  `bornMaximized` key unchanged. Windows adapter-copy replacement remains
  handoff item 23; native Windows compilation of the lift remains pending.
  Underlay candidate C `#662a82da` selected for now on both platforms; KDE
  kcfg/KCM default delivered offline 2026-10-13 (native build, CTest 32/32,
  KWin static-contract tests 15/15). Windows default remains item 24.
  Settings UI, both-values
  launch, radius appearance and black/white/grey underlay live checks pending;
  underlay visual check before 0.1 ([options](changes/archive/kde-dogfood-underlay-options.md)).
- P1 | Windows unsafe FFI consolidation | Required under user rule 2026-10-13:
  one tiny dedicated FFI module per crate, every other module/crate forbids
  unsafe. Queue after current Windows release-gate work; timing tentative.
  Preserve safe input/render handoffs. [Rule](decisions.md#rust-unsafe-guardrail).
- P2 | Windows handoff: shortcut settings dialogs | Match KDE's modal
  conflict preview and Apply/Revert success/error dialogs in
  `crates/tiler-windows`; use the short result wording in
  [Settings decisions](decisions.md#settings-tray-and-first-run). Shortcuts
  apply immediately; avoid stacked confirmations (tentative, user review pending) and
  retain required data-loss safety. KDE user-tested live 2026-10-12: dialogs
  worked; result text shortened in `b718987` (short-text recheck pending).
  [flow and evidence](changes/archive/kde-shortcut-result-dialogs.md).
- P2 | Windows float/maximized focus tie research | Tentative 2026-10-12,
  pending user review: observed-before-retained encounter order only affects
  equal-axis ties, not distance ranking. Existing-source tally: 0 explicit
  observed-first, 2 applicable but incomparable live-list mechanisms, 7
  unavailable profiles. Keep current order; deeper comparable reference
  tie-order research pending. [Discriminator](spec/reference-outcomes.md#windows-floating-candidate-discriminator-2026-10-12),
  [tally](changes/archive/windows-floating-directional-navigation.md#user-accepted-physical-evidence---2026-10-12).
- P1 | Windows fullscreen-exit acceptance | Startup/born exits, retained managed
  slot return, borderless exit and owned-exit metadata retirement pinned offline
  2026-10-12; directional post-clear owned-metadata veto corrected. Owned
  Notepad managed-slot return and fullscreen stop/relaunch/exit tiling
  physically user-accepted 2026-10-12, scoped single-output Notepad/Paint,
  base `e506350` plus pre-retirement Windows changes. User-selected automatic
  stale retirement delivered offline for positively restored nonzero chrome
  masks, with double live proof/lifetime/preimage fences and transition/
  re-fullscreen regressions. User follow-up 2026-10-12: originally captionless
  zero-bit metadata stays fail-closed without a restoration witness; Win+F11
  owned toggle exit is the release path. User watches for untiled windows
  during dogfood; no remaining policy decision for this case. App-native/born-after-start/
  borderless, retirement physical, fixed/game/DPI/multi-output pending.
  [Record](changes/archive/windows-fullscreen-exit-acceptance.md).
- P0 | Windows port | KDE-first core extraction finished at K1 (user
  2026-09-30; K2/K3 revisit when macOS starts, see
  [extraction](research/cross-platform-core/extraction.md),
  [K2 audit](changes/archive/portable-core-k2-settings-actions.md)).
  User decisions (2026-09-30): Windows 11 x64 only; managed per-monitor
  workspaces required for feature-complete; Meta/Win default shortcuts matching
  KDE (2026-10-01), Windows Snap takeover with a visible off setting, and Win+L
  explicit opt-in; custom-drawing
  underlay experiment with outline fallback; most obvious distribution
  (Store plus signed installer plus winget under evaluation); develop
  natively on Windows 11; multi-output live work needs a multi-output Windows setup;
  elevated (administrator) windows unmanaged by default, a user-chosen
  run-elevated option possible later; logical workspace model refined in the
  spikes; shared settings/action intent (deferred K2) and difference
  classification (deferred K3) revisited when macOS starts. Open: final
  package/update channel. Status 2026-10-02: Phase 1 lifecycle/recovery plus
  WH_KEYBOARD_LL input done and accepted on a single-display setup; Phase 2
  automatic Engine-driven tiling-only dogfood preview verified (no shortcuts,
  input hooks or hiding; stop/crash leaves windows in place). Shortcut slice
  completed with retained-Engine focus/move, default-on keyboard takeover and
  session-only mouse prevention plus explicit CLI off switches. Exact native
  foreground and geometry proof passed the full owned-helper journey
  `20261002-040025-21256`; graceful/crash setting restore and ordinary-app
  default-on/both-off smoke passed. Foreground lock is handled by E8 prime plus
  temporary foreground-thread attachment; the synthetic Shift misrouting was
  a proof-arrow extended-key encoding defect, corrected without changing
  product filtering. Native gates and independent mutation review pass.
  Initial SPI target mix-up was recovered; final arranging is 1 and pen is 35.
  Windows settings UI Apply/Revert delivered; physical shortcuts and Snap
  Layouts/bar/Assist/shake coverage remain pending. Managed workspaces now pass
  current-artifact helper and scoped Notepad/Calculator/Paint journeys: existing
  ordinals, send/follow, trailing empty, independent layouts, identity-safe
  hiding and graceful/crash recovery. Terminal is an ordinary managed app by
  default. The physical send trace exposed a new-target outer-gap mismatch;
  fixed with both-domain regression coverage. Fresh independent review and
  native gates pass. Workspace latency fix 06f9d8d passes scoped helper and
  ordinary-app recovery/timing: 72-124 ms ordinary-app selects, 171-241 ms
  physical select/send trace; user accepts select/send/follow and drag-resize
  usability. [Latency evidence](changes/archive/windows-workspace-action-latency.md).
  Follow-up 2026-10-12 edge-resize lag: pixel/checksum-identical border raster
  optimization passes native offline gates; user dev/trace-on reports much
  better but still laggy. LL hooks share the rendering/owner thread; unchanged
  hide traces suppressed and owner busy-span diagnostics delivered offline.
  Trace-off and release smoothness/release-settle comparison pending
  ([raster record](changes/archive/windows-edge-resize-latency.md),
  [input-thread evidence and commands](changes/archive/windows-resize-hook-latency.md)).
  Owner threading assessment 2026-10-12: separate rendering early; user selected
  target C (isolated input then render pumps, A first), with one observation/Engine/recovery owner; latency selection
  is separate from parked Win+G/F11 containment
  ([design](research/windows-port/owner-threading.md)).
  Minimum-size hints shipped in 49d4191 and were physically accepted, including
  send into a populated workspace (2026-10-02). Late directional dogfood moves
  match KDE/core R2c/R3; minimum-pinned geometry explains zero visible change.
  [Minimum-size evidence](changes/archive/windows-minimum-size-hints.md).
  [Directional diagnosis](changes/archive/windows-directional-dogfood-diagnosis.md).
  Send split-axis defect fixed in `c4a837b` (shared core): stale destination
  focus after send-away/return now falls back to valid destination focus
  history; minimum sizes did not force the axis. One user return split
  stacked as expected, but a later run (2026-10-03) returned Notepad side
  by side at the right edge, then into the middle; see dogfood defects.
  [Evidence](changes/archive/windows-send-split-axis.md).
  Next: Windows feature parity queue below.
  [Managed workspaces](changes/archive/windows-managed-workspaces.md).
  [Shortcut slice and handover](changes/archive/windows-shortcut-slice.md).
  Full owned-helper proof passed `20261001-222611-6068`: 8/8 gaps at 125%,
  minimize/restore, close reflow, graceful/emergency stop preserving frames,
  11 owned writes and zero foreign writes; clean recovery. Normal-mode smoke
  `20261001-223101-30052` passed Notepad/Calculator/Paint/Terminal admission,
  true new-window open/close, Calculator minimize/restore and unchanged stop
  frames. Paint held 617px against a 543px plan, causing 66px overlap; minimum-
  size neighbour replanning was an initial preview limitation, resolved by the
  accepted minimum-size hints. Standard native gates
  passed; multi-monitor/mixed-DPI, gestures and games remain unaccepted.
  See [Phase 2 record](changes/archive/windows-phase2-tiling.md).
  [plan](research/windows-port/plan.md)
  [decision](decisions.md#scope-and-platform-goals)
  Day-one setup and pending governance: [Windows development environment](windows-dev-environment.md).
- P0 | Windows feature parity queue | User order (2026-10-02), each matching
  KDE behavior and bindings: (1) active window border done (`b5374ce`, native
  gates/CI and scoped composed-pixel/live routes pass;
  [evidence](changes/archive/windows-active-border.md); temporary development
  default `#ffff00` with theme off, `3fcc953`); (2) group underlay stages A/B
  done (`cb790d9`; [evidence](changes/archive/windows-group-underlay.md)), C
  (unfocused dragged subject) parked for reassessment with item 7;
  (3) maximise done (`1be97a1`, Win+M; [evidence](changes/archive/windows-maximise.md);
  physical button/input/feel checks remain); (4) fullscreen shipped
  (`e84b1a7`, Win+F11; [evidence](changes/archive/windows-fullscreen.md));
  (5) float shipped (`d8329e3`, Win+G; [evidence](changes/archive/windows-float.md));
  (6) sticky float shipped (`292d8c1`, Win+Shift+G;
  [evidence](changes/archive/windows-sticky-float.md)). Live acceptance of
  4-6 is user-owned: agent runs were disturbed by Xbox mode (cloaked
  `ApplicationFrameWindow` foreground, DWM cloak 0->2;
  [record](changes/archive/windows-foreground-acceptance.md)).
  (7) mouse drag move and (8) drop preview delivered same-output
  (`1b9bf7b` title-bar drag, `7dc6aa3` Win+left drag, `b68d490` preview; CI
  green; [evidence](changes/archive/windows-mouse-drag.md)); selected:
  title-bar stays native, Win+drag keeps the source frame and moves only the
  preview. Move-drag press-focus is selected; current drop activation is an
  implementation gap (handoff item 7); inactive resize keeps focus. Physical feel checks
  user-owned; underlay C stays parked (needs extra machinery).
  (9) multi-output support UNPARKED (user 2026-10-10): after single-output
  handoff items 1-4, 6, 7, 9-11, 13, 15, 17, 18, 20, with item 18 before
  10/11, then the multi-output foundation and dependent items 5, 12, 14, 16,
  19. Live work needs a multi-output Windows setup;
  (10) taskbar workspace indicator is combined with hidden-workspace Alt+Tab/
  taskbar semantics in Windows workspace-presentation research after
  multi-output, as a Windows release gate
  ([note](research/windows-port/alt-tab-hidden-workspaces.md));
  (11) settings core slice delivered (`df4edc5`, `1c97c52`, `999f2f3`,
  `81986ab`; CI green; `just --justfile windows.justfile settings`;
  [evidence](changes/archive/windows-settings.md)): JSON store under
  LocalAppData, plain Win32 Apply/Revert UI, live gaps/visuals/Snap
  takeover, conflict list and presets (compatible disables 35 conflicting
  chords, authentic stays the default), system accent border default with
  KDE `#2a82da` fallback (yellow removed). Ratified 2026-10-08. Tray
  (Settings, Stop, shortcut-conflict warning, TaskbarCreated re-add) and
  first-run Authentic/Compatible prompt delivered (`60fd7bb`, `533f3c9`;
  [evidence](changes/archive/windows-tray-first-run.md)); tray workspace
  mode toggle omitted at first, then delivered with per-workspace
  tiling/floating modes and `core.workspace.default_tiled` (`90ee5c2`,
  `0fa3d29`, `ace352c`, `618683b`;
  [record](changes/archive/windows-workspace-tiling.md)); maximized windows
  on floating workspaces now match KDE (R-MAX-03 resolved).
  Remaining: broader rebinding, physical checks incl. real Explorer
  restart.
  Original item 11 scope: settings
  (UI with Apply/Revert parity, including Snap takeover off; decide the final
  accent/configured border default and remove the temporary yellow default;
  per-binding OS-conflict list with compatible/authentic quick-set presets,
  first-run prompt ratified 2026-10-08; [decision](decisions.md#settings-tray-and-first-run)).
  Provisional choices 2-6 accepted by the user (2026-10-03).
- P0 | Windows gaming coexistence | Windows release gate after multi-output,
  before any Windows release (user 2026-10-10); not now despite P0.
  User (2026-10-03): (a) authentic mode
  until settings exist: our bindings must not leak to OS shortcuts (Win+G
  opened Xbox Game Bar when the foreground was unmanaged, and Xbox mode was
  entered during agent tests; user dogfood 2026-10-03: Win+F11 entered Xbox
  mode, so Win+F11 collides with the OS full screen experience shortcut);
  (b) detect Xbox mode (full screen
  experience), pause tiling, effects and shortcuts, remember windows and
  workspaces, restore on exit; (c) research alternate Game Bar access and
  anti-cheat false-positive risk (LL keyboard hook, overlays, window
  moves). Status 2026-10-03: (a) authentic containment offline-verified,
  CI green (`962b0f3`, `5746fca`; the Win+F11 leak was our deliberate
  pass-through). Physical test after those commits FAILED: Win+F11 still
  opens the Xbox mode prompt and Win+G still opens Game Bar (traces
  `run-01dd53116aa74f37.log`, `run-01dd5311a7282421.log`). PARKED
  2026-10-03 (`2c9c11c`, `bbe5b0f`, `6c4ffad`): a provably current,
  consumed Win+F11 tap still produced the Xbox prompt, also after
  reinstalling our hook last; no official-API fix found; prior art has
  none. User 2026-10-08 keeps Authentic ownership with the limitation
  disclosed; Compatible/disable/rebind provide Game Bar access. Containment
  remains parked for later test and user review, grouped with this work;
  host-setting, registry/policy or dedicated-hook-thread alternatives remain
  unselected and require separate authorization. (b) user chose
  documented-signal-only detection (2026-10-03); blocked until one exists.
  (c) researched; alternate Game Bar access deferred (see Future).
  - User physical re-check 2026-10-12, scoped single-output Notepad/Paint,
    base `e506350` plus pre-retirement Windows changes: every Win+F11 owned
    fullscreen/exit press also raised the Xbox full screen experience prompt.
    Known parked containment issue confirmed; user wants a fix eventually.
    Remains parked in gaming coexistence, with no new containment work.
  - M16 gaming-cost measurement: compare the existing 100 ms pump's
    incremental CPU, frame-time and input cost against a without-tiler game
    baseline, including suspended work/effects. Cost remains unmeasured;
    suspension and nominal tick frequency are not acceptance evidence.
    Apply the M04 bar: aim for zero; demonstrate any unavoidable residual
    minimal and imperceivable. Physical game measurements are user-owned.
    [principle](principles.md#gaming-compatibility)
    Non-native measurement lessons:
    [event/pump boundary](research/non-native-tiler-host-interaction.md#layer-layout-authority-events-and-reconciliation-dim-1),
    [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
    [evidence](changes/archive/non-native-tiler-host-interaction.md).
  [record](changes/windows-gaming-coexistence.md)
  [decision](decisions.md#gaming-safety)
- P0 | Windows release gate | Own later release (user 2026-10-10): managed
  per-monitor workspaces (multi-output), gaming coexistence (Win+G/F11
  containment review, Xbox mode detection, M16 100ms pump measurement),
  combined workspace-presentation research (hidden-workspace Alt+Tab/taskbar
  semantics plus taskbar indicator parity 10), restart persistence (item 8,
  on-window markers selected 2026-10-10) and package/update channel decision (short research refresh
  during Windows release planning; most obvious and unsurprising for users).
  Gaming and Windows presentation research run after multi-output,
  before release; presentation produces options plus a recommendation for
  the user. Non-native lessons and evidence stay linked:
  [event/pump boundary](research/non-native-tiler-host-interaction.md#layer-layout-authority-events-and-reconciliation-dim-1),
  [workspaces/Alt+Tab](research/non-native-tiler-host-interaction.md#workspaces-monitors-overview-and-alttab-dim-4),
  [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [evidence](changes/archive/non-native-tiler-host-interaction.md).
- P0 | Windows workspace presentation research | After multi-output, before
  Windows release (user 2026-10-10). Windows work combines hidden-workspace
  Alt+Tab/taskbar semantics and taskbar workspace indicator (parity 10),
  producing options plus a recommendation for the user. Existing Alt+Tab
  research (`580c766`, `8129b37`;
  [note](research/windows-port/alt-tab-hidden-workspaces.md)):
  keep SW_HIDE (no supported reinsertion); public off-screen parking is the
  only official candidate, with visibility/recovery costs; native desktops
  and private cloaking need separate API decisions. Activation behaviour
  is matrix row R-WS-07. No presentation design selected. Non-native inputs:
  [workspaces/Alt+Tab](research/non-native-tiler-host-interaction.md#workspaces-monitors-overview-and-alttab-dim-4),
  [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [evidence](changes/archive/non-native-tiler-host-interaction.md).
- P1 | KDE follow-ups from the Windows port | Audit 2026-10-05
  ([note](research/cross-platform-core/post-windows-audit.md)); user order
  B1/B2, B3-B5, B7, B8. B1/B2 stale maximize/sticky attempted-state
  refusals repaired offline (one attempt per activation; 843 KWin tests
  pass); B3-B5 shared-core KDE fixtures added, no defect found
  ([record](changes/archive/kde-post-windows-followups.md)). User-owned live check
  (2026-10-05): Meta+M, then Meta+Shift+G refused while maximized, then
  Meta+M restored, as decided. DEFECT: Meta+Shift+G on a tiled window
  floats it sticky but focus moves to another window (Meta+G keeps focus);
  contradicted the exact-toggle focus retention decision. Supplied trace plus
  real Engine replay identified the follow-up foreground flag reconcile
  actuating survivor focus (pre-existing source defect, not B2). Repaired
  offline by skipping that activation over a focused float/sticky subject;
  faithful regression red/green, 846 KWin tests and all gates pass. User
  live re-check (2026-10-05): normal and sticky float focus correct
  ([evidence](changes/archive/kde-post-windows-followups.md#test-system-re-check-user-owned)).
  Q8 offline repeat evidence delivered: second same-reference maximize/sticky
  native-change cycles, duplicate sticky notification convergence and one native
  attempt per delivered maximize callback; existing maximize/fullscreen/geometry
  repeat fixtures reused ([record](changes/archive/kde-repeat-offline-fixtures.md)).
  Still pending, user-owned live: native-change repeats and held-key autorepeat;
  physical delivery/suppression remains
  [TBD](spec/reference-outcomes.md#r-max-04--r-flt-02-kde-held-key-discriminator).
  User decisions 2026-10-05: B6 infeasible minimums use origin+minimum on
  both platforms (Windows already does it); KDE delivered offline
  `cf6ab31` (920 KWin / 1107 Rust tests, CI green; client shortfalls get
  three reassertions then quiet acceptance; live steps in
  [record](changes/archive/kde-minimum-origin-placement.md)). Q3 born-maximized admission follows COSMIC:
  tile with a reserved slot and keep the maximize as an overlay, no launch
  unmaximize (KDE and Windows); KDE delivered offline `9b612be` (901 KWin
  tests, CI green; live steps in
  [record](changes/archive/kde-born-maximized-overlay.md): born-maximized
  launch beside a sibling, native restore into the slot, session-restore
  no loop). User Q3 scope decision 2026-10-07 also covers R-MAX-03:
  KDE floating-to-tiled reserved-slot overlay delivered offline (921 KWin /
  1107 Rust tests); no admission clear, later unmaximize lands in the slot,
  synchronous/repeated maximize signals settle. Native check pending in
  [record](changes/archive/kde-maximized-floating-retile-overlay.md).
  B9 overlaid intentional unfloat: user 2026-10-08 selected unmaximize then
  fresh admission. Shared-core regression/KDE delivered offline
  ([record](changes/archive/maximized-intentional-unfloat.md)); Windows refusal
  remains a gap owned by handoff item 15; COSMIC confirmation and KDE native check pending.
  Pinned 11-WM source comparison added. B7
  movement-only underlay A/B delivered offline (Meta+Shift chord or focused
  native user move; host-matched native build and all gates pass; paired
  FFI signature change; C parked); user tested A/B on a single-output setup
  (2026-10-05): all good; multi-output remap checks later. Next: B8 after user acceptance of the existing KDE
  shortcut controls. Core extraction: no new move until macOS starts.
  Shared restart store in core (float intent, tile overrides, possibly positions); platforms supply window identity and storage location (user 2026-10-08).
- P1 | Windows parity with the 2026-10-05 KDE session | Next Windows session:
  (a) float-origin directional focus and Meta+Shift+arrow half-snaps
  (R-FLT-07..11; Windows delivered offline 2026-10-12,
  [record](changes/archive/windows-floating-directional-navigation.md), ordinary
  physical single-output user acceptance 2026-10-12; variants/game/DPI pending);
  (b) Q3 born-maximized
  reserved slot plus maximize overlay, replacing the one-shot admission
  clear on Windows (and KDE, same decision); user 2026-10-07: Q3 also
  covers the R-MAX-03 floating-to-tiled case (stays maximized over a
  reserved slot). KDE R-MAX-03 and R-MAX-06 delivered offline, native checks
  pending ([scope record](changes/archive/kde-maximized-floating-retile-overlay.md));
   Windows parity (b) delivered offline 2026-10-12
   ([record](changes/archive/windows-maximized-floating-retile-overlay.md));
   born R-MAX-06 uses the delivered 2026-10-11 setting, R-MAX-03 independent;
    ordinary single-output retile/restore and both-values born Paint launch
    user-accepted physically 2026-10-12; hidden/session/fixed/game/DPI pending.
  (c) B9 overlaid unfloat: unmaximize then fresh-admit, selected 2026-10-08;
  remove the Windows refusal, with the user's COSMIC check as confirmation
   only (handoff item 15; KDE and Windows delivered offline,
   [Windows record](changes/archive/windows-maximized-intentional-unfloat.md),
    ordinary native clear/frame/focus agent-observed 2026-10-12; physical B9,
    marker/restart/failure/game/DPI pending); (d) keyboard resize delivered
  2026-10-11, base `f794cf9` + delivery commit (handoff item 6,
  [record](changes/archive/windows-keyboard-resize.md)); physical interception
  remains user-owned. Non-local workspace modes remain unimplemented despite
  catalog/settings text (`workspace.rs:95-108`).
  [decisions](decisions.md#window-state-float-sticky-maximize-fullscreen)
  [audit](research/cross-platform-core/post-windows-audit.md)
- P1 | Adopt reference-consensus additions | User 2026-10-07 accepted the
  recommendations from the consensus Table A; items 1-5 shared
  core/KDE delivered (item 1 single-output user-confirmed, items 2-5 offline),
  each through the shared Engine where possible, KDE and Windows unless
  noted ([consensus](research/reference-wm-consensus.md)):
  R-MOV-08 allow vertical move onto another output once local movement is
  exhausted; R-LAY-01 parent split-axis toggle; R-WS-08 previous-workspace
  toggle; R-WS-11 wrapping
  next/previous workspace switch; R-WS-12 move whole workspace to another
  output; R-WS-14 send window to next/previous workspace; R-OUT-04 send
  window to output; R-MAX-09 carry fullscreen across workspace send on
  Windows (observe KDE first); R-SPC-04 float fixed-size windows on
  admission; R-RST-01 keep intentional floats floating across owner
  restart, including Windows sticky floats staying sticky (R-FLT-05, user
  2026-10-07); R-RSZ-01 Windows keyboard resize (overlaps Windows parity (d));
  R-WS-01 separate send-and-stay command beside the existing follow send
  (user 2026-10-07; COSMIC Send/Move pair; stay unbound by default);
  R-MOV-03 setting: COSMIC wrap default, flat sibling
  swap alternative (user 2026-10-07); R-DRAG-08 Windows Win+drag activates
  the mover at press instead of on drop (user 2026-10-07).
  R-LAY-04: PARKED / OPEN by User decision 2026-10-09 until a genuine second
  layout exists; revisit when tabbed stacks are designed after 0.1. Whether tabs
  count as L2 remains a future decision; no active implementation.
  [Findings](research/workspace-local-layout-selection.md).
  Detailed selections 2026-10-07 (items 1-5 and D1 in
  [decisions](decisions.md#move-layout-and-output-commands)); KDE items 1-5 delivered
  offline, remaining implementation pending:
  Q2 R-SPC-04 shared core/Linux planner + KDE delivered offline under user
  decisions 2026-10-08 (normative as delivered); D1 predicate setting and
  functional naming [delivered offline](changes/archive/admission-and-move-settings.md);
  D5 first-fullscreen-exit and D6 tiling-enable check
  [delivered offline](changes/archive/fixed-size-exit-and-enable.md);
  D7 tile-override store [delivered offline](changes/archive/fixed-window-tile-override-restart.md), native checks pending;
  [record](changes/archive/fixed-size-admission.md). Windows Q2
  behavior remains unchanged; handoff item 13 supplies its exact wiring.
  Q3 R-RST-01 Linux planner/KDE intentional-float restart delivered offline
  under user decisions 2026-10-08 (normative as delivered): private
  session-scoped membership store, current-frame adoption, native-success
  settlement and diagnosed-empty availability fallback. Native acceptance
  remains pending; [record](changes/archive/kde-intentional-float-restart.md).
  Windows behavior remains unchanged; handoff item 8 carries its design seams.
  Q4 R-WS-12 core/Linux planner + KDE whole-active-workspace output migration
  delivered offline under user decisions 2026-10-08 (normative as delivered):
  four unbound follow actions, strict native per-output capability, retained
  domain rekey, source refill, floats/sticky split and fullscreen+maximized
  carry (D8 delivered offline 2026-10-09). Native acceptance pending;
  [record](changes/archive/kde-whole-workspace-output-migration.md),
  [D8](changes/archive/migration-overlay-carry.md).
  Windows changes are compile-only `maximized: false`; item 14 wires behavior.
  Shared Rust core + KDE adapter are delivered KDE-side; separate Windows
  adapter wiring follows. Correctness over non-breakage: Windows build/behavior
  may break provided the handoff below lists the specific changes needed.
  Implementation order:
  1. R-WS-08 + R-WS-11 (KDE delivered; single-output native journey
     confirmed by user 2026-10-07, multi-output/presets pending;
     [record](changes/archive/kde-workspace-history-ring.md)):
     Meta/Win+Ctrl+Tab previous-view two-state toggle;
     Meta/Win+Ctrl+arrows and +H/J/K/L relative switch (left/up previous,
     right/down next). Local/global-unique per-output history/ring, shared
     one history/ring; every successful observed workspace change records,
     including native/send-follow/hotplug, not same-workspace activation or
     output focus alone. Stable IDs, surviving empties valid; removed,
     unassigned or out-of-recording-output-scope IDs clear; no-op until next
     recorded change, no recreation/ordinal reinterpretation. Disconnected
     output history discarded; reconnect selection never consults/restores
     history (1.5). Ring wraps all existing scoped workspaces, trailing empty
     and ordinals beyond 9 included; selection creates nothing. Authentic
     clears conflicting holders; Compatible disables conflicting arrows,
     letters remain (KDE KWin desktop-switch arrows; Windows native Left/Right
     ownership general knowledge, unverified in repo).
  2. R-WS-01 + R-WS-14 (shared core + KDE delivered offline; native journey
     pending; [record](changes/archive/kde-workspace-send-follow-stay.md)):
     keep numbered follow Meta/Win+Shift+digits; relative
     follow Meta/Win+Ctrl+Shift+arrows and +H/J/K/L. Numbered/relative stay
     bindable, unbound. Relative targets ordinal step in item 1 ring, not
     MRU, resolved once before transfer; fills trailing empty, normal
     lifecycle supplies next spare. Follow/stay both absolute/relative.
     Authentic clears KDE KWin window-desktop arrow holders; Compatible
     disables our arrows, letters remain; Windows ownership unknown.
     Explicit follow/stay now also reaches KDE's membership-only
     floating-boundary path, repairing its existing-default gap: default
     follows; stay preserves the source view/native boundary focus.
  3. R-MOV-03 (shared core + KDE delivered offline; native journey pending;
     [record](changes/archive/same-axis-move-setting.md)):
     global KDE `sameAxisMove` / Windows `core.same_axis_move`,
     `group-with-neighbor` default or `swap-with-neighbor`; Windows additive version-1 field,
     missing defaults to group. Apply to subsequent moves without tree
     rebuilding; KDE settings UI control. Swap-with-neighbor replaces R2c only for
     adjacent direct leaf siblings; shares travel with windows. Leaf/group
     rules unchanged; discriminating TBD rows before broadening. User
     2026-10-08: setting and both values take functional names with WM
     tooltips (functional value IDs); breaking pre-release configs accepted,
     no migration; functional rename [delivered offline](changes/archive/admission-and-move-settings.md), Windows settings wiring pending.
  4. R-LAY-01 (shared core + KDE delivered offline; native journey pending;
     [record](changes/archive/parent-orientation-toggle.md)):
     Meta+O / Win+O immediate-parent axis toggle including root,
     preserving order/shares/focus; sole root leaf no-op, no saved admission
     hint, long-edge rule unchanged. No KDE stock holder found; Windows
     orientation lock conflict: Authentic takes over, Compatible disables.
     Pending live check for user: Meta+O toggles root and nested immediate
     parent, twice restores order/shares/focus, lone window is a no-op.
  5. R-MOV-08 + R-OUT-04 (shared core/protocol + KDE delivered offline;
     two-output native journey pending;
     [record](changes/archive/four-direction-output-transfer.md)):
     local restructure/swap/escape first, then cross
     all four directions, including sole root leaf (horizontal too).
     Reciprocal edge-touch + positive-overlap adjacency on FULL
     output rectangles, horizontal too; no candidate no-op, unreadable
     refuses, no wrap. User decision 2026-10-09: select by window centre
     projection, then window-span overlap, then left/top; selection delivered
     offline ([record](changes/archive/position-based-output-selection.md)). Explicit output-follow Meta/Win+Ctrl+Alt+
     arrows and +H/J/K/L; stay bindable unbound. KDE arms absent from
     `kglobalshortcutsrc`, Windows ownership unknown; COSMIC's arm collides
     with resize-shrink, niri's with item 2. Target destination current
     workspace with ordinary send admission (remembered leaf, focus history,
     root) and command follow/stay. Initially tiled-subject eligibility,
     sticky excluded; floating boundaries membership-only, tiled sides
     reflow. Ordinary float transfer stays open.
     Full rectangles select adjacency only; work-area placement and R4 edge
     landing nearest the source remain. Explicit send reuses item-2 follow/
     source-MRU stay, with pinned source/target current-view arrival fences.
  Bindings for other accepted additions remain to be chosen.
- P1 | Windows handoff: reference-consensus additions | D1, user 2026-10-07:
  Windows adapter pieces are wired after their KDE-side delivery.
  ### How to use this handoff

  - Historical source checks 2026-10-08 at
    `9bbc83b6bf37295cb2883d9d31880599e7bdf088` for items 1-5 and shared
    plumbing.
  - Historical revision check 2026-10-08 at
    `db31234f450e11af78406518150438c623ea91e4` for entries 6-12 and the
    matrix/spec corrections below; original revision provenance above is
    preserved. Deliveries through `1c26f2d` (2026-10-10) supersede this
    baseline, including output selection `a8e32ce` and items 17-20. Line
    references below are historical locators; re-find named symbols after pulling.
  - KDE/shared deliveries: item 1 `7f1a9ee`, item 2 `8d476ee`, item 3
    `0dc7518`, item 4 `fa15add`, item 5 `9bbc83b`. Item 1 has user-confirmed
    SINGLE-output live acceptance (2026-10-07, unspecified edge cases/presets).
    Items 2-5 are offline-delivered; multi-output/hotplug/preset and native
    journeys remain pending. None of this proves Windows native behavior.
  - D1: shared core plus KDE are delivered KDE-side; Windows adapter is
    delivered Windows-side. Correctness wins over Windows non-breakage, with
    exact repairs recorded here. Decision 2.3 permits behavior-preserving
    compile-only Windows fixes for items 2-5; those fixes are not completed
    Windows implementations.
  - Progress: scope, acceptance, exact pending work and evidence per
    implementation piece; archive on completion. Update linked
    Ours Windows matrix cells and spec Win cells with dated, revision-bound
    offline evidence; native outcomes stay TBD until user-tested. Preserve
    reference outcomes and product decisions; add decision delivery pointers.

  ### Execution order and shared Windows plumbing

  | Order | Deliverable | Dependency / boundary |
  | --- | --- | --- |
  | 1 | Item 1 local history/ring plus exact Ctrl/Tab input/settings support | Shared input foundation for items 2 and 5. Global-unique/shared runtime and multi-output remain separately pending. |
  | 2 | Item 2 explicit follow/stay, then relative sends | Reuse item 1 ring/action routing, not MRU target selection. |
  | 3 | Item 3 live same-axis setting | Independent; replace compile constant without rebuilding trees. |
  | 4 | Item 4 orientation action | Reuse input/catalog/live settings infrastructure. |
  | 5 | Unparked parity-queue multi-output foundation after single-output items 1-4, 6, 7, 9-11, 13, 15, 17, 18, 20 (18 before 10/11), then handoff items 5, 12, 14, 16, 19 | Per-monitor current-view observation, membership/geometry/visibility/recovery fences first; user live checks need a multi-output Windows setup. Offline topology/Engine tests can precede that. User physical 2026-10-12 on a two-output host (primary left, secondary right): repeated Win+Shift+Right stopped at the primary's right edge and a mouse drag would not drop on the secondary; consistent with the absent foundation, trace logs held outside the repo. |
  | 6 | Keyboard resize R-RSZ-01 (parity d) | Independent; needs fresh Alt-capable trigger plus dedicated resize intent. No dependency on items 1-5 except shared modifier routing. |
  | 7 | Press-focus R-DRAG-08 | Independent of items 1-5; touches Win-drag arm only. Keep R-DRAG-07 stationary-source/moving-preview split intact. |
  | 8 | Restart R-RST-01/R-FLT-05 sticky/intentional persistence | Independent of items 1-5; KDE Q3 plus D7 tile-override store delivered offline. Ordinary/sticky intent must remain distinct from automatic fixed origin and recovery authority; Windows mechanism selected 2026-10-10 (prefixed on-window markers, item 8 below). REQ-RST-01c is NORMATIVE D25 (User 2026-10-12), membership/set/focus native acceptance pending. Coordinate D7 membership with item 13. |
  | 9 | Fullscreen send R-MAX-09 (Windows carry; NOT the parity-queue multi-output foundation) | Depends on handoff item 2 follow/stay wiring only; same-output workspace carry, no cross-output claim. |
  | 10 | Float/half-snap parity (a) R-FLT-07..11 | Windows delivered offline; ordinary focus/half-snaps physically user-accepted 2026-10-12, scoped single-output normal owner Notepad/Paint. Tie/sticky/clamp/preset/fixed/game/DPI pending; cross-output item 5. [Record](changes/archive/windows-floating-directional-navigation.md). |
  | 11 | Born-max/floating-retile overlay parity (b) incl R-MAX-03 | Windows delivered offline; ordinary retile/restore and both-values born Paint launch physically user-accepted 2026-10-12, scoped single output. Tray focus observation only; hidden/session/fixed/game/DPI pending. [Record](changes/archive/windows-maximized-floating-retile-overlay.md). |
  | 12 | Non-local workspace modes parity (d) | Depends on handoff items 1/2 (ring + follow/stay) and the parity-queue multi-output foundation; last. |
  | 13 | Q2 fixed-size float admission R-SPC-04 | Windows delivered offline 2026-10-11, prerequisite a41fcb5 plus runtime delivery: max-track observation, predicate/schema/UI, D1-D8 lifetime/origin and fixed-origin D7 markers. Native/game acceptance pending; fixed-override maximize route tentative. [Record](changes/archive/windows-fixed-size-admission.md). |
  | 14 | Q4 whole-workspace migration R-WS-12 | Core/KDE offline delivery including D8 overlay carry under user decisions 2026-10-08 (normative as delivered); depends on item 5's multi-output foundation, item 1 history invalidation and item 12 for non-local runtime modes. Windows adapter including D8 pending; no Windows behavior delivered. |
  | 15 | B9 / M09 maximized intentional unfloat R-FLT-06 | Windows delivered offline; ordinary scoped native clear/observe/fresh admission and focus agent-observed 2026-10-12. Physical B9/fixed marker/restart/failure/game/DPI pending; independent of Q3 retile. [Record](changes/archive/windows-maximized-intentional-unfloat.md). |
  | 16 | Position-based output selection | Shared core/KDE delivered offline 2026-10-09; Windows handoff only. Wire with item 5's multi-output foundation and item 14 migration, preserving arrival/current-view/lifetime fences. |
  | 17 | G-05 per-axis-zero predicate | Windows delivered offline 2026-10-11 with item 13, shared predicate reused; converted partial-zero orientations/default/either and subsequent-admission controls pass. Native hint representation pending. [Record](changes/archive/windows-per-axis-zero-predicate.md). |
  | 18 | G-06 maximized directional focus/move | Windows delivered offline; tiled focus fence and clear-before-tiled-move/float-four-halves physically user-accepted 2026-10-12, scoped single output. Failure/race/preset/fixed/game/DPI pending. [Record](changes/archive/windows-maximized-directional-navigation.md). |
  | 19 | G-37 migration source-refill setting | Shared core/KDE delivered offline 2026-10-10; Windows config/UI/runtime pending with items 1/14 and the multi-output foundation. |
  | 20 | G-D2 maximized send carry | Windows same-output numbered/relative follow/stay verified agent-observed 2026-10-11 with item 2; KDE offline. Physical/cross-output pending; item 5 foundation required. |
  | 22 | D16 ordinary/combo sends and D23 best-effort restore/close cleanup | Shared D16 ordinary/combo MoveDeferred and D23 rejoin/closed_windows contracts delivered offline; Windows adapter pending. D16: open only intentional-float send gate, propagate floating mover overlay flags, membership-only deferred actuation, retain float frame/intent across destination exit. Windows D27 urgent-only/guard unchanged. [D16 evidence](changes/archive/d16-floating-transfer.md), [D23 evidence](changes/archive/d23-rework.md). |
  | 23 | Born-maximized shared policy | After core lift, replace Windows adapter copy; preserve behavior, labels, schema and subsequent-admission semantics. |
  | 24 | Underlay default `#662a82da` | Pending: change Windows render/settings defaults and CLI/proof mirrors to candidate C, 40% accent blue, for now; preserve saved colors; black/white/grey visual check at 100%/150%. KDE default delivered offline. |
  | Later | Unsafe FFI consolidation | Required after current Windows release-gate work (timing tentative); one dedicated minimal FFI module per crate, forbid elsewhere. |

  Q2 R-SPC-04 occupies handoff item 13 below; Q3 R-RST-01 KDE delivery is
  complete offline and updates existing item 8. Q4 R-WS-12 fills item 14.
  Q5 R-LAY-04 is PARKED / OPEN (User decision 2026-10-09) until a genuine
  second layout exists; no KDE piece or Windows handoff implementation is active.

  - Source notation: `src/...` / `tests/...` below are under
    `crates/tiler-windows/`; `core/...` is `crates/tiler-core/src/`;
    `protocol/...` is `crates/tiler-protocol/src/`. KDE paths start `kwin/`.
    Proposed Windows IDs below are new implementation names mirroring KDE
    suffixes, not existing rows; existing Windows digit IDs are retained.
  - Verified input limits: `src/snapkey.rs:1012` `SnapClassify::push` rejects
    fresh Ctrl/Alt chords at :1101; `push_owned` :1146/:1250 also rejects them.
    Consumed holds retain their paired verdict when modifiers change.
    `ChordRemap` :130 and `ChordDisable` :144 carry only VK+Shift.
    `src/settings.rs:389` `parse_chord` ALREADY parses Ctrl/Alt tokens, but
    `vk_for_key_name` :337 supports letters/digits/F1-F24/arrows, not Tab or
    Escape. Add Tab for item 1; Escape currently only passes through for drag
    cancellation (`src/snapkey.rs:54-56`), not a selected binding addition.
    `validate_bindings` (`src/settings.rs:1317/:1363`) and
    `apply_rebind_text` (`src/settings_ui.rs:403/:425`) reject Ctrl/Alt rebinds.
  - Items 1/2/5 share changes to `BindingFamily` (`src/settings.rs:475`),
    `binding_wants_shift` :1032, `binding_canonical_vk` :1044,
    `effective_bindings` :1071, `build_remap` :1154, `build_disabled` :1206,
    `validate_bindings` :1317. Carry exact Shift/Ctrl/Alt plus explicit ACTION
    through routing, duplicate detection, canonical hold slots and release
    pins. VK alone cannot identify focus/relative select/relative follow/output
    follow; an unbound stay row has no canonical default VK but must rebind.
  - Extend `src/snapkey.rs:427` `WorkspaceOp`, `WorkspaceIntent` :445,
    `Classified` :503, `MaskTrigger` :545, `SnapClassify` :580,
    `push` :1012/`push_owned` :1146, `QueuedWorkspaceIntent` :2055,
    `QueuedSnapEvent` :2116 and `classify_and_queue` :2217. Preserve origin
    binding (`SnapOrigin` :1976, `resolve_origin` :2003), down/repeat/up pairing,
    held Disable/rebind behavior, saturation verdict and E8 mask reservation
    (:2303). Do not just remove Ctrl/Alt guards and reuse Shift-derived actions.
    Authentic owns chords independently of foreground eligibility; native
    actions still require fresh origin, lifetime, suspension and scope guards.
  - Schema stays version 1 (`src/settings.rs:31`): binding overrides remain
    `bindings[id] = {state, chord?}` (:280-299), absent means Keep, missing
    `chord` defaults None. New stay rows have no default chords; Keep means
    unbound, not Disabled/effective interception. Repair `effective_bindings`,
    canonical-target validation and `src/settings_ui.rs:175/:231`
    `row_text`/`refresh_info`; do not parse empty sequence as a key.
  - Dynamic Settings rows use `binding_catalog` (:757), `refresh_list`
    (`src/settings_ui.rs:207`), `stage_state` :459 and `collect_draft` :345.
    Support Keep/Disable/Rebind, full-modifier duplicates and actual rebound
    conflict text. Preserve atomic Apply (`do_apply` :495) and saved-file
    Revert (`do_revert` :580). Live routing adoption uses
    `src/tiling_sys.rs:13432` `apply_live_settings` / :13547 `poll_live_settings`.
    Update hard-coded UI text in `src/settings_ui.rs:1088` preset explanation,
    :1093 "Shortcuts (48 rows)" and :1100 rebind/modifier note as rows land;
    unknown OS ownership must not be labeled conflict-free.
  - Presets use `src/settings.rs:1490` `apply_preset` (reset overrides, no
    replacement chords) and `compatible_disabled_ids` :1528 (currently 35).
    Update fixed counts/assertions as rows land. Compatible treatment is
    specified per item below, not copied from KDE foreign-holder tables.
    Windows uses owned-chord interception, not KGlobalAccel holder clearing.
  - Tray: `src/tray.rs:120` `unresolved_conflicts` warns only known-incomplete
    containment (G/F11, opt-in physical L); unknown containment adds no warning.
    `menu_items` :235 and `settings_for_choice` :345 already expose Settings,
    presets and workspace mode. Propagate catalog/presets/status tests; these
    five pieces do not select additional menu commands.

  ### Common definition of done and live-check protocol

  - [ ] Each item meets its behavior, adapter, settings/input and tests below;
    replace compile placeholders where required. Portable tests use real
    retained Engine replies, exact-lifetime and hidden-row policy, not just
    callback/catalog presence. Pure workspace/owner/tiling/settings/classifier/
    tray tests run on Linux; cfg(windows) owner/hook/UI wiring needs Windows.
  - [ ] Run native offline gates from ordinary PS7 with root mise-selected
    MSVC (`rustc -vV` host must be `x86_64-pc-windows-msvc`), per the
    [Windows runbook](windows-dev-environment.md#5-prove-linker-discovery-and-run-native-offline-development-gates):

    ```powershell
    mise exec -- rustc -vV
    mise exec -- cargo build --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows
    mise exec -- cargo test --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows
    mise exec -- cargo fmt --all -- --check
    mise exec -- cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows --all-targets -- -D warnings
    git diff --check
    ```

  - [ ] CI `.github/workflows/ci.yml`: `windows` :95-128, `rust` :32-47 and
    applicable `kwin` :13-30, `shell` :49-80, `native` :82-93 green after
    publication; `macos` :130-148 remains green.
    Linux handback: `devenv shell --impure -- cargo test --workspace`,
    `devenv shell --impure -- cargo clippy --workspace --all-targets -- -D warnings`,
    `devenv shell --impure -- cargo fmt --all -- --check`; shared changes also
    need applicable KWin/mock-shell/native checks and `just check-portable`
    through Linux/CI. Record unrun gates as pending.
  - [ ] Update the linked Ours Windows matrix cells, spec `REQ-*` Win/status/
    shortcut rows, decision delivery pointers and one archived change record
    per piece. Keep native pending separate from offline completion. Preserve
    existing TBD order/focus/lifecycle outcomes. New ambiguities need shortest
    discriminating matrix rows with unsupported outcomes TBD.
  - Live steps below are a USER queue, not mutation authorization. Read
    [live Windows testing](live-windows-testing.md) before planning them. User
    owns physical input and ordinary-app dogfood; agent native experiments need
    separate classes/resources/chords/duration/end condition and identified
    disposable owned windows. Follow the approved dependency list/mise route;
    installs are user-owned. This docs handoff ran no live tests.
  - EACH live journey: record OS build, revision/diff, artifact path/SHA-256,
    exact owner PID/start/session/integrity, display full/work rectangles/DPI
    and permitted windows; prove independent stop/restore first. User starts
    the authorized tiling owner
    (`mise exec -- just --justfile windows.justfile tile --user-start --trace`;
    `dev` runs only the lifecycle owner),
    applies the specified preset/settings, performs the steps, then runs
    `mise exec -- just --justfile windows.justfile stop`. Verify hook release,
    hidden-window reveal and owned session-setting restoration. Forced loss
    needs separate authority and the exact-owner recovery ladder. Callback/API
    success alone does not prove physical OS suppression.

  ### Item implementation checklists
  - Item 1: Windows local history/ring and exact Ctrl/Tab settings/input/UI
    delivered offline 2026-10-10 (base `91db9d2` + delivery commit,
    [record](changes/archive/windows-workspace-history-ring.md)); native gates
    passed; native CLI toggle/ring/>9 and UI journeys also agent-observed
    2026-10-11 with [item 2](changes/archive/windows-workspace-send-follow-stay.md).
    Physical chords/preset OS effects pending; non-local/multi-output parked.
    KDE adapter delivered offline, no shared Rust core/API changes. Historical checklist:

    #### Item 1 behavior and reference seams

    - Normative: decisions 1.1-1.5; [spec](spec/functional-spec.md#workspaces)
      REQ-WS-08/11; [matrix](spec/reference-outcomes/workspaces.md)
      R-WS-08/11/15/16/17. Two-view toggle, not MRU traversal. Record successful
      observed numbered/relative/toggle/native/foreground/send-follow/hotplug
      view changes; never attempted setters, same-workspace activation or
      output focus alone. Local/global-unique history/rings are per-output;
      shared has one history/ring. Stable previous ID survives emptiness but
      clears on removal/unassignment/leaving recording-output scope. No-op
      until next recorded change; never recreate/reinterpret by ordinal.
    - Ring includes ALL existing scoped order, trailing empty and >9; wrap
      both ends, select creates nothing. Disconnect drops previous AND observed
      baseline. Reconnect selection never consults/restores history; prime a
      fresh baseline and record subsequent observed displacement/return changes.
      Observe before/after hotplug actuation so A->B->D records B, not A.
    - Core currently supplies only existing workspace lifecycle primitives:
      `core/workspace.rs:7` `select_existing`, :16 `resolve_send_target`, :21
      `WorkspaceFacts`, :29 `TrailingPlan`, :35 `plan_trailing`, :88
      `choose_displaced_destination`. Numbered resolvers stop at 9; do NOT
      implement the ring through those resolvers. No history/ring CoreCommand,
      protocol command or reply was added by `7f1a9ee`; this is adapter state.
    - KDE reference: `kwin/src/workspace-native.ts:990` `selectPrevious`,
      :1051 `selectRelative`, :2141 `recordHistoryObservations`, :2185
      `observeSharedHistory`, :2203 `scopedRingIds`, :2224
      `validatePreviousEntries`; `handleTopologySignal` :921 observes hotplug.
      Catalog :152-222; native compiled holder identities at
      `kwin/native-effect/shortcutreconciler.h:55-62`.
      [Offline fixtures](../kwin/tests/workspace-previous-relative.test.ts),
      [delivery/live status](changes/archive/kde-workspace-history-ring.md).

    #### Item 1 catalog, conflicts and acceptance

    | Proposed Windows id | Default chord | Intent | Compatible |
    | --- | --- | --- | --- |
    | `workspace-previous` | Win+Ctrl+Tab | Two-view toggle | Keep |
    | `workspace-prev-h`, `workspace-prev-k` | Win+Ctrl+H, Win+Ctrl+K respectively | Previous ordinal | Keep |
    | `workspace-prev-left-arrow` | Win+Ctrl+Left | Previous ordinal | Disable |
    | `workspace-prev-up-arrow` | Win+Ctrl+Up | Previous ordinal | Keep |
    | `workspace-next-j`, `workspace-next-l` | Win+Ctrl+J, Win+Ctrl+L respectively | Next ordinal | Keep |
    | `workspace-next-down-arrow` | Win+Ctrl+Down | Next ordinal | Keep |
    | `workspace-next-right-arrow` | Win+Ctrl+Right | Next ordinal | Disable |

    - Authentic keeps all nine rows. Win+Ctrl+Left/Right native virtual-desktop
      ownership is UNVERIFIED in repository (decision 1.1), not measured here.
      Extend `src/settings.rs:954` `chord_conflict` past its Ctrl/Alt early
      return: identify those chords with text such as "Windows virtual desktop
      switch (ownership unverified in repository); override needs takeover,
      containment unproven live". Tab/letters/Up/Down get an honest ownership-
      unknown note, not a stock-holder or conflict-free claim. Compatible
      disables these two new rows for that recorded conflict; KDE disables
      all four arrows for different KWin holders. Test presets independently.
    - Exact native integration: `src/tiling_sys.rs:9819` `workspace_do_select`
      activates only after verified hide/reveal (:10061), then obtains fresh
      target focus/geometry. Add observation recording at that completed view
      transition, including when later focus/geometry fails; view evidence,
      not the final action success label, controls history. Dispatch through
      `workspace_tick` :11395 and its selection arm :11530; also cover
      `poll_workspace_cli_request` :11130, `poll_foreground_workspace` :11912
      and `sync_monitor_outputs` :12050. `chord_output` :8508 /
      `src/workspace_owner.rs:25` `output_context` resolve command scope.
    - State/resolvers/invalidation are the `ManagedWorkspaces` changes below;
      input/catalog/UI use the shared plumbing checklist above. No new
      persisted history/settings field or standalone history UI control;
      expose the nine action rows through the existing binding editor.
      Exact state sites: `src/workspace.rs:95` `ManagedWorkspaces`, :244
      `activate`, :255 `select`, :262 `select_trailing`, :297 `resolve_send`,
      :303 `resolve_send_trailing`, :508 `apply_cleanup`, :561
      `displace_output_to`, :603 `reconnect_output`, :673 `workspace_ids`.
      Add pure stable-ID toggle/ring resolvers without activating/appending;
      observe actual completed transitions separately from these setters.
    - [ ] Portable: add observed-change vs failed-attempt tests, repeated
      toggle WS1->WS2->WS3->WS2->WS3, same-view/output-focus isolation,
      surviving empty vs deleted/moved ID, disconnect/reconnect baseline,
      local/global-unique/shared scope fixtures, >9/trailing wrap and unchanged
      inventory. Extend `tests/snapkey.rs` / `tests/settings.rs` for exact
      Ctrl/Tab routing, extra modifiers, held live rebind/disable, queue/mask,
      duplicate modifiers and Compatible vs Authentic rows.
    - [ ] Windows-only: owner selection/foreground/CLI producers, native
      hook release/suppression and settings Keep/Disable/Rebind Apply/Revert.
      Local-only delivery must explicitly keep non-local/multi-output pending.
    - User journey: occupy WS1/2/3; select 1->2->3; Win+Ctrl+Tab twice must
      show 2 then 3. Select first, previous must reach last existing trailing
      empty; next wraps to first without creation. Populate through ordinal
      10 using normal trailing sends, then verify relative selection >9.
      Re-select same view, toggle unchanged; after verified send-follow toggle
      returns to source. Repeat Left/Right with Compatible (project disabled,
      record native desktop effect) and Authentic (project view switch, record
      whether native desktop ALSO switches), plus disable/takeover-off release.
      Multi-output user: R-WS-15 isolation and R-WS-17 disconnect/toggle/reconnect
      sequence; moved previous D clears on L if no intervening change.

    - `workspace.rs` / `ManagedWorkspaces`: stable-ID previous plus observed
      current baseline per output for local/global-unique, one shared scope
      when non-local modes land. Resolve previous and ordinal relative targets
      without activating, appending or invoking trailing creation; use every
      existing scoped ID, trailing empty and >9 positions, wrap both ends.
    - `tiling_sys.rs` / `workspace_do_select`: dispatch toggle/relative targets
      through existing verified hide/reveal/focus selection. Record actual
      successful view changes from numbered/relative/toggle, native/foreground
      activation, verified send-follow and hotplug, not attempted setters,
      same-view activation or output focus. `ManagedWorkspaces::activate`,
      cleanup, displacement and return must converge on that observation rule.
    - Invalidate removed/unassigned/out-of-scope previous IDs (including global
      swap and returning workspace); never reinterpret an ordinal or recreate.
      Discard BOTH previous and observed baseline on disconnected outputs;
      keep displacement associations separate. Prime a reconnect baseline,
      record any subsequently observed return change, never consult history
      for reconnect selection. Observe before/after hotplug actuation so two
      changes in one handler leave the actual immediately preceding view.
    - `settings.rs`, `snapkey.rs` and settings UI: add one Win+Ctrl+Tab toggle
      and eight separate Win+Ctrl+H/K/Left/Up previous, J/L/Down/Right next
      rows. Add Tab VK recognition and exact Ctrl modifier routing without
      conflating focus/move/digit arms; extend intents, queues, hold/repeat,
      suppression, remap and duplicate validation to include Ctrl (current
      rebind/suppression keys carry VK+Shift only; Ctrl arms are rejected).
      Authentic owns the new chords; Compatible disables Left/Right for the
      native-desktop conflict, keeps letters/Tab/Up/Down enabled with no
      invented holder claims. Physical native suppression needs Windows evidence.
    - Port KDE offline acceptance cases (toggle, producer/scope isolation,
      empty survival/removal, relocation/disconnect/reconnect, >9/trailing wrap,
      no creation, exact modifiers and presets); non-local modes/multi-output
      runtime remain the separately parked Windows work.
  - Item 2: Windows explicit numbered/relative follow/stay, bindable unbound
    stay rows and exact input/settings/UI delivered 2026-10-11 (base `553c65b`
    + delivery commit, [record](changes/archive/windows-workspace-send-follow-stay.md)).
    Native gates passed; scoped CLI/MRU/null/floating/>9 and UI outcomes
    agent-observed. Physical chords/presets and non-local/multi-output pending.
    Shared core/KDE offline delivery retained. Historical handoff:

    #### Item 2 behavior and shared contract

    - Normative: decisions 2.1/2.2/2.3 and R-WS-01 follow/stay selection;
      [spec](spec/functional-spec.md#workspaces) REQ-WS-01/01b/06/14;
      [matrix](spec/reference-outcomes/workspaces.md) R-WS-01/14/18/19/20.
      Numbered follow stays default; separate numbered/relative stay unbound.
      Relative target is item 1 scoped ordinal step, never previous-history,
      resolved ONCE before transfer, wrapping through trailing empty and >9.
      Filling trailing empty invokes ordinary lifecycle for the next spare.
    - Tiled follow focuses mover at target after verified transfer. Tiled stay
      retains source selection/visibility and source focused-removal MRU;
      null focus issues no setter, native removal focus stands. Admission,
      source collapse and destination geometry are identical for both intents:
      valid remembered destination leaf, then valid destination focus history,
      then genuine root fallback, long-edge split. Floating boundaries remain
      membership-only, floating frames untouched, tiled sides reflow; stay
      retains existing native boundary focus, no invented float MRU tracking.
      Sticky/intentional-float eligibility remains as shipped.
    - Exact core: `core/boundary.rs:93-100`
      `CoreCommand::SendToWorkspace {window,target_output,target_workspace,follow}`;
      :170-185 `CoreEvent` carries source `domain/domain_key/windows/outer_gap`
      and `target_domain/target_windows`. :328-337 `SendWorkspacePlan` fields
      `base_revision, policy_version, geometry, focus_domain, focus_leaf,
      follow, operation, preconditions`; :666 `CoreReply::SendWorkspace`.
      `core/session.rs:125` `SessionCommand::MoveToWorkspace` and
      `core/contract.rs:394` `LifecycleIntent::MoveToWorkspace` carry explicit
      follow. `core/session/ops/workspace.rs:103` `propose_send_impl` owns
      admission (:198) and source-MRU/null stay (:269).
    - `core/engine.rs:1865` `transfer_request` converges the complete canonical
      pair and immediately commits planned topology (:2140), not native
      success. Windows calls Engine directly; no second Session transaction or
      JSON roundtrip is needed. `protocol/planner_protocol.rs:3740-3750`
      `SyncCommand::SendToWorkspace` wire op `send-to-workspace` defaults
      omitted `follow` to true; native Windows must pass the actual intent.
    - KDE reference: `kwin/src/workspace-native.ts:1162`
      `resolveRelativeMoveTarget`; `kwin/src/plan-adapter-entry.ts:4964`
      `requestWorkspaceMove`, :5104 `requestWorkspaceRelativeMove`, :5156
      `transferResolvedWorkspace` (floating boundary); `kwin/src/workspace-send-adapter.ts:1309`
      `requestSend`, :2635 `followOnce`, :2915 `confirmStayOnce` (fresh arrival,
      pinned source visibility and exact MRU/null focus, no desktop switch).
      [Fixtures](../kwin/tests/workspace-send-follow-stay.test.ts),
      [record](changes/archive/kde-workspace-send-follow-stay.md).

    #### Item 2 binding and acceptance checklist

    | Windows id | Default chord | Intent |
    | --- | --- | --- |
    | Existing `workspace-send-1`..`workspace-send-9`, `workspace-send-0` | Win+Shift+1..9/0 | Numbered follow; 0 reuse/append trailing |
    | Proposed `send-prev-h/k/left-arrow/up-arrow` (four ids) | Win+Ctrl+Shift+H/K/Left/Up respectively | Relative previous follow |
    | Proposed `send-next-j/l/down-arrow/right-arrow` (four ids) | Win+Ctrl+Shift+J/L/Down/Right respectively | Relative next follow |
    | Proposed `stay-workspace-1`..`stay-workspace-9`, `stay-workspace-0` | Unbound (ten rows) | Numbered stay; 0 reuse/append trailing |
    | Proposed `send-stay-prev-h/k/left-arrow/up-arrow`, `send-stay-next-j/l/down-arrow/right-arrow` | Unbound (eight separate rows) | Relative stay |

    - KDE catalog `kwin/src/workspace-native.ts:225-392` includes shifted-symbol
      aliases for numbered stay; Windows digits share the same VK with symbols
      (`src/settings.rs:830-834`), so no duplicate physical symbol rows.
      Use explicit follow/stay ACTION in shared input/remap/queue work, not
      focus inference or Shift alone. Existing CLI send remains follow unless
      its transport is explicitly extended for needed tests.
    - Authentic keeps follow rows and unbound stay. Windows Ctrl+Shift arrow
      ownership is UNKNOWN (2.1); no new Windows Compatible disables are
      selected from KDE's four Window One Desktop holders. Keep new defaults
      pending evidenced conflicts, display "Windows shortcut ownership unknown;
      containment unproven live" by extending `chord_conflict` :954 past its
      Ctrl/Alt early return (:956-957), including effective rebound chords.
      KDE Compatible disables four arrows; do not claim those Windows holders.
    - Replace `follow:true` in builder/test below with real command wiring.
      No new global follow setting/schema field; per-binding action encodes
      intent. Keep schema-v1 missing binding overrides at Keep/unbound as above.
    - [ ] Portable `src/workspace_owner.rs` / `tests/tiling.rs`: explicit true/
      false events, stamped targets preserve intent, actual Engine admission
      byte/topology equality, stay MRU vs null, hidden target no-write, relative
      wrap/trailing/>9/frozen target. Port real core/KDE fixture semantics.
      `tests/snapkey.rs` / `tests/settings.rs`: all bound and unbound actions,
      modifier arms, rebind-away defaults, full-key duplicates, live holds.
    - [ ] Windows-only: Engine/native-boundary tails, source-selected fences
      before writes AND focus, target hidden/reveal policy, recycled lifetime,
      changed view/mode/gaps, partial transfer reconciliation without replay.
      Run common CI/doc gates; item 1 history records follow but not stay.
    - User: WS1 A,B (focus A then B), WS2 C; Win+Shift+2 follows B, source
      collapses to A. Fresh fixture, bind a stay row through Settings to a
      validated unused chord (example Win+Ctrl+Shift+F6, after checking it is
      unused in the effective catalog); send B, WS1 remains shown/focused A,
      WS2 stays hidden. Select WS2 to inspect unchanged admission. Repeat relative next
      from WS10 into trailing E and previous from WS1 to pre-transfer last E;
      E fills and one next spare appears. Sole-B stay keeps source selected,
      no explicit focus setter; record native focus/lifecycle as TBD evidence.
      Tray-toggle WS2 floating, repeat both intents both directions: floating
      frames stable and only tiled sides reflow. Apply/Revert/unbind restores
      defaults; physical Ctrl+Shift arrows must not leak under Authentic.

    - Compile fix applied: `src/workspace_owner.rs:127-132` constructor sets
      `follow: true` at :131; the exhaustive command match in
      `send_event_binds_target_and_focused_mover` at :1286-1290 requires
      `follow: true` at :1290. Both preserve always-follow Windows behavior.
      Real wiring remains: `build_send_event` :70 must accept explicit
      `follow: bool` instead of the constant. `stamp_send_target` :138/:139
      already uses `..`; preserve intent while replacing target IDs.
    - Thread that parameter through every `build_send_event` caller:
      `src/tiling_sys.rs:10848` (`workspace_do_send`) and test calls in
      `src/workspace_owner.rs:1270,1726,1915,2070,2282,2588` and
      `tests/tiling.rs:2217`. Existing follow tests pass true; add false cases.
    - `src/tiling_sys.rs` / `workspace_do_send` :10552, reply match :10876
      and verified transfer/tail :10945 onward: retain the `SendWorkspace`
      payload, branch on `plan.follow`. Follow selects/reveals target and
      focuses mover; stay hides the transferred mover without selecting or
      revealing target, keeps source selected/visible, applies source-bound
      `plan.focus_domain/focus_leaf` through exact-lifetime leaf/window
      resolution (no setter for null), and reflows only writable source rows.
      Destination admission/structural geometry remain identical; target
      writes obey existing hidden-row fences. Do not infer intent from focus.
    - `src/tiling_sys.rs` / `workspace_do_send_native` :10314 and call :10692:
      thread the same explicit intent through floating boundaries. Membership
      transfer remains native-only, tiled sides reconcile, floating frames
      stay untouched; follow uses verified target selection/mover focus;
      stay keeps source view and existing native boundary focus, no target
      selection. Sticky/intentional-float eligibility remains unchanged.
    - Changed core types searched across the entire Windows crate:
      `SessionCommand::MoveToWorkspace` and `LifecycleIntent::MoveToWorkspace`
      have no Windows sites; no direct `SendWorkspacePlan` constructors or
      destructures. Its added `follow` field reaches `CoreReply::SendWorkspace`.
      Besides production :10876 above, existing payload reads in
      `src/workspace_owner.rs:1210,1513,1602,1946,2099,2163,2215,2311,3016,3225`
      only consume geometry/operation and need no shape repair; extend relevant
      send tests to assert intent/focus. Wildcard reply arms
      :1093,1167,1188,2003,2611,2993,3135,3206,3288 likewise need no shape repair.
    - `src/workspace.rs` / `ManagedWorkspaces`: reuse item 1's scoped existing
      ordinal ring for relative targets, resolve once before mutation, wrap
      both ends including trailing empty and >9; ordinary lifecycle supplies
      next spare. Keep source-empty selection distinct from target admission.
    - `src/snapkey.rs` / `WorkspaceOp` :427, classifier/dispatch/queues and
      `src/settings.rs` catalog/rebind/presets: split explicit follow/stay,
      add Ctrl+Shift H/K/Left/Up previous and J/L/Down/Right next follow;
      numbered 1..9/0 and relative stay register unbound. Carry exact modifiers
      through suppression/repeat/remap (item 1 Ctrl work reused). Windows
      arrow ownership is unknown: do not invent a holder claim.
    - Despite cfg(windows) native actuation, portable `workspace_owner.rs`
      participates in Linux workspace builds. The compile-only fix restores
      full-workspace `cargo test`/`clippy` gates; stay/relative adapter wiring
      remains pending. Port follow/stay MRU/null, admission equality, visibility
      fences, relative wrap/spare and floating-boundary regression coverage.
  - Item 3: Windows schema-v1/live setting/UI/move routing delivered 2026-10-11
      (`755aab8`, [record](changes/archive/windows-same-axis-move.md));
      native gates/CI and agent-observed Apply/Revert/no-rebuild passed. Physical
      directional journey pending; checklist below retained as acceptance detail.
      Sites below under `crates/tiler-windows/`:

     #### Item 3 behavior, references and acceptance

     - Normative: decisions 3.1/3.2; [spec](spec/functional-spec.md#move)
       REQ-MOV-03; [matrix](spec/reference-outcomes/move.md) R-MOV-03/09/10.
        Global `core.same_axis_move`, `group-with-neighbor` default, `swap-with-neighbor`
       alternative. Only R2c adjacent direct LEAF siblings in the same N-ary
       group change; unequal shares travel with window identities. Binary R2a,
       group-neighbor wraps/insertion, escape and output boundaries keep their
       rules. Default `H[A,B*,C,D]` right -> `H[A,H[B,C],D]`; flat ->
       `H[A,C,B*,D]`. No rebuilding, retroactive flattening or new swap verb.
      - Core: `core/directional.rs` `SameAxisMove::{GroupWithNeighbor,SwapWithNeighbor}`,
       :229 `as_wire_str`, :239 `parse_wire`; :202 `MoveIntent.same_axis_move`.
       `core/boundary.rs:48-57` `CoreCommand::Move {window,direction,
       cross_output_transfer,same_axis_move}`; :372 `MovePlanReply`, :670
       `CoreReply::MoveDirectional` unchanged. `core/engine.rs:2531`
       `directional_move_request` threads the typed mode. Windows should use
       these semantics, not reimplement swapping in native geometry.
       `protocol/planner_protocol.rs:3686-3697` `SyncCommand::Move` decodes
       `same_axis_move` string (missing wrap), :3800-3812 maps the validated
       token. Unknown/mistyped values refuse; core has no serde dependency.
     - KDE: `kwin/src/same-axis-move.ts:15/:33` normalize/read;
       `kwin/src/plan-adapter-entry.ts:7414-7427` reloads subsequent-move value;
       `kwin/src/plan-adapter.ts:3100` `requestMove` sends explicit flat token
       at :3178 (default omitted). KCM `kwin/native-effect/unifiedsettings_module.cpp:57`
       `readSameAxisMove`, :115-120 combo; Save/reconfigure live pickup detailed
       in [record](changes/archive/same-axis-move-setting.md).
       [KDE fixtures](../kwin/tests/same-axis-move.test.ts) and
       [core fixtures](../crates/tiler-core/tests/session_movement.rs).
     - Windows schema/control: add `CoreSettings.same_axis_move` at
       `src/settings.rs:99` with serde missing-default and Default :124;
       validate exact tokens in `validate_settings` :1268 (map via shared
       `parse_wire`, reject unknown, no silent invalid->default). Keep v=1,
       startup invalid-file and live last-good behavior. UI adds a global
       "Same-axis move" two-choice control in `src/settings_ui.rs:939`
       `cmd_settings` / control
       construction :1054, `refresh_all` :292 and `collect_draft` :345; Apply
       writes tokens and Revert reloads saved selection. Read last-good per
       move in `keyboard_tick` (`src/tiling_sys.rs:5645`, command :6102-6110),
       with wrap fallback for proof owners lacking `settings_live` (:1210).
       Same-axis-only settings adoption must not trigger tree rebuild/resync.
     - Bindings/presets/conflicts/tray: no new action/chord or conflict entry;
       existing Win+Shift+H/J/K/L/arrows use the setting. Authentic/Compatible
       change bindings only (`src/settings.rs:1490` `apply_preset`), not this core setting.
       Existing tray Settings opens its control, no menu addition.
     - [ ] Portable: schema-v1 old file missing field -> wrap; both token
       roundtrips, invalid value/type refused, atomic save and live invalid file
       keeps last-good. Retained Engine tests for right AND left unequal-share
       leaf swaps, wrap default, group neighbor R-MOV-10 unchanged, binary/
       nested-boundary parity, focus retention, mode change no topology mutation
       until next move. Replace test constant only where exercising settings;
       retain explicit default-wrap fixtures as appropriate.
     - [ ] Windows-only: Settings Apply/Revert and startup/proof fallback;
       verify `keyboard_tick` uses adopted setting, not hard-coded mode; common
       gates/docs update only R-MOV-03/09/10, not item-5 rows.
     - User: prepare R-MOV-03 four-leaf row; default right wraps B/C. Fresh
        row, Settings Swap with neighbor + Apply, right yields A,C,B,D focused B. Repeat
       left and unequal widths, verify widths travel. Change setting on an
       existing nested tree: Apply alone changes no layout; next move uses
       new mode. R-MOV-10 group-neighbor result stays wrap, not whole-group
       swap. Stage another choice then Revert, verify saved choice returns.

     - Applied compile fixes: `src/tiling_sys.rs:6102`, `SnapOp::Move` arm
       constructing `CoreCommand::Move`, sets
        `same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor`;
       `tests/snapkey.rs:915-919`, `engine_focus_moves_through_nested_topology`,
       uses the same constant (production field :6109, test field :919).
       No Windows behavior change.
     - `src/settings.rs:99` / `CoreSettings`, `Default` :124 and
       `validate_settings` :1268: add `core.same_axis_move` within schema
        version 1, serde missing default `group-with-neighbor`, exact values
        `group-with-neighbor`/`swap-with-neighbor`; reject unknown/old values through
        existing invalid-file/last-good behavior, retain atomic saves. Labels
        `Group with neighbor` / `Swap with neighbor`; tooltips COSMIC / i3, sway.
        No aliases or migration for retired `cosmic-wrap` / `flat-swap` IDs.
     - `src/settings_ui.rs` / `cmd_settings` :939, controls :1054,
       `refresh_all` :292 and `collect_draft` :345: add the global
       setting control, display/persist the selected token, validate Apply,
       preserve existing Revert behavior.
     - `src/tiling_sys.rs` / `TileLoop.settings_live` :1210,
       `apply_live_settings` :13432, `poll_live_settings` :13547 and
       `src/settings.rs` / `LiveSettings` :1689, `poll_for_change` :1722:
       adopt valid changes for subsequent moves only, without tree rebuild
       or reconciliation. In `SnapOp::Move` :6102 replace the constant with
       the last-good setting mapped to shared `SameAxisMove`. Other
       input/focus/resize/float move paths retain existing rules.
     - Changed shared constructors searched: the above production and test
       `CoreCommand::Move` sites are the only Windows sites; no Windows
       `MoveIntent`/`SyncCommand::Move` constructors or exhaustive move-field
       destructures require repair. The reply shape is unchanged: existing
       `CoreReply::MoveDirectional` branch at `src/tiling_sys.rs:6173` continues
       applying returned geometry for R2c as well as R2a.
     - Extend portable settings decode/default/validation/round-trip and
       live last-good tests; native settings Apply/Revert tests; move tests
        for N-ary swap-with-neighbor both directions with traveling unequal shares,
       default wrap, group-neighbor/boundary parity and live changes without
       tree rebuild. Linux workspace tests/clippy compile portable Windows
       modules; native Windows UI/owner checks remain Windows-owned.
  - Item 4: Windows Win+O/catalog/presets/input/owner routing delivered 2026-10-11
      (`f5801d8`, test-only CI repair `83085a3`, [record](changes/archive/windows-parent-orientation-toggle.md));
      native gates/CI and agent-observed Settings/owner adoption passed. Physical
      toggle/OS suppression pending; held-repeat choice tentative pending user
      review. Checklist below retained as acceptance detail (Windows paths):

     #### Item 4 behavior, references and acceptance

     - Normative: decisions 4.1/4.2; [spec](spec/functional-spec.md#layout)
       REQ-LAY-01; [matrix](spec/reference-outcomes/layout-commands.md)
       R-LAY-01/05/06. Win+O toggles focused tiled leaf's immediate parent,
       root included, retaining order/shares/focus. Twice returns exact tree.
       Lone root, float/no tiled focus and floating workspace are no-write
       outcomes; focused maximized/fullscreen refuses. Sibling overlays remain
       reserved slots, reproject but skip native geometry/state writes. No
       future-admission orientation hint; long-edge and collapse unchanged.
     - Core: `core/boundary.rs:90-92`
       `CoreCommand::ToggleOrientation {window}`; :200
       `TiledKind::ToggleOrientation`; :250-259 `TiledPlan` carries revision,
       policy version, kind, geometry, focus domain/leaf and optional float
       fields (empty for orientation). :665 `CoreReply::Tiled`.
       `core/session.rs:161` `SessionCommand::ToggleOrientation`,
       `core/contract.rs:418/:460` lifecycle intent/operation;
       `core/session/ops/toggle_orientation.rs:39` `propose_toggle_orientation`
       and :212 `flip_group_axis` preserve direct parent's children/shares;
       :102-121 focused overlay refusal / lone `unchanged`.
       `core/engine.rs:2464` `toggle_orientation_request` ordinary retained
       single-domain handling. `protocol/planner_protocol.rs:3738-3739` wire
       `{"op":"toggle-orientation","window":"<id>"}`, :3857 typed mapping.
     - KDE: `kwin/src/plan-adapter.ts:3269` `requestToggleOrientation` gates
       subject/workspace, :3314 exact command; :1655
       `isToggleOrientationDetail`, :7014 orientation reply validation.
       `kwin/src/plan-adapter-entry.ts:449-456` catalog; native key constant
       `kwin/native-effect/shortcutreconciler.h:126`.
       [Fixtures](../kwin/tests/plan-toggle-orientation.test.ts),
       [record](changes/archive/parent-orientation-toggle.md).
     - Binding id `toggle-orientation`, default Win+O, unshifted. Add
       `ToggleKind::Orientation` at `src/settings.rs:523` with token/Shift
       methods (:530-548), catalog :757 and live effective/remap handling.
       Authentic keeps; Compatible adds this one disabled id (:1528) for
       orientation lock. Existing `chord_conflict` :983 text is
       "orientation lock owns Win+O; override needs takeover". The catalog
       claim is repository-VERIFIED with Microsoft-list attribution (:943-952);
       OS ownership was not independently rechecked by this handoff, and
       physical suppression is UNVERIFIED. No current Windows Win+O action.
     - No persisted core field; schema-v1 absent binding override means Keep.
       Dynamic settings editor exposes Keep/Disable/Rebind; add O (0x4F) to
       classifier inventory, exact Win+O action/queue/mask handling below.
       Default Shift/Ctrl/Alt variants pass through; preserve key pairing and
       existing activation fences. Resolve any unselected autorepeat outcome
       with a discriminating matrix row/TBD, not a new policy in this handoff.
       Dedicated `src/tiling_sys.rs:5645` `keyboard_tick` arm, not a SnapOp movement request;
       assembly uses `src/tiling.rs:1226` `build_reconcile_event_for_floating`,
       replace command, accept only matching Tiled kind, apply geometry with
       fresh fences. Current helpers do not magically dispatch the new action.
     - [ ] Portable Engine/owner: root both axes, nested parent-only, unequal
       shares double roundtrip, focus/minimum allocation, lone no-op then wide
       long-edge admission, float/no-focus/floating-workspace/overlay fixtures;
       focused overlay refusal and sibling write-skip must be distinguished.
       `tests/settings.rs` / `tests/snapkey.rs`: O catalog/preset/conflict,
       rebind/disable, exact modifiers, repeat/release, mask and saturation.
     - [ ] Windows-only owner/input/UI: stale origin/lifetime or suspension
       causes zero writes; real matching Tiled reply actuates without view
       switch, sibling overlays untouched. Common gates/docs: R-LAY-01/05/06.
     - User: ordinary root pair A,B (B focused), Win+O -> V then -> H with
       identities/shares/focus retained. R-LAY-05 nested H[A,V[B*,C]] toggles
       only V. Sole A on wide area: no effect; opening B still uses long edge
       (record native newcomer order/focus, currently TBD). Float/workspace-
       floating/focused overlay: no effect; sibling overlay native state stays.
       Compatible passes Win+O to OS, Authentic toggles only project layout;
       record OS orientation-lock effect/suppression with unchanged displays,
       and test Disable/rebind/Apply/Revert/takeover-off release.

     - `src/settings.rs:523` / `ToggleKind` and token/Shift methods :530-548:
       add orientation; `binding_catalog` :757-827 adds `toggle-orientation`
       default Win+O. Reuse `chord_conflict` :983 orientation-lock text;
       `compatible_disabled_ids` :1528 adds the new id/count, Authentic
       `apply_preset` :1490 keeps it. Dynamic settings UI consumes the catalog;
       test Keep/Disable/rebind and effective conflict text, without claiming
       physical OS suppression before Windows evidence.
     - `src/snapkey.rs:43-58` key inventory: add O (0x4F); `Classified` :503,
       `MaskTrigger` :545, `SnapClassify::push` :1012 / `push_owned` :1146,
       `QueuedSnapEvent` :2116 and `classify_and_queue` :2217: extend the
       toggle family with exact unshifted Win+O, paired downs/repeats/ups,
       origin identity, live suppression/remap, saturation and E8 mask.
       Shift/Ctrl/Alt variants pass through; Compatible disables our binding,
       Authentic takes over the orientation-lock chord. Extend portable
       `tests/snapkey.rs` and `tests/settings.rs` coverage.
     - `src/tiling_sys.rs:6095-6205` directional retained Engine/apply route
       and :6224 toggle queue arm are integration examples, not new SnapOp
       movement semantics: add a dedicated queued orientation arm with fresh
       origin/suspension/lifetime/tiled-workspace/focused-overlay guards.
       Build an ordinary single-domain event with
       `CoreCommand::ToggleOrientation { window }`; accept
       `CoreReply::Tiled` only with `TiledKind::ToggleOrientation`, then use
       `apply_geometry` :4866 with existing writable/hidden/overlay/minimum
       fences and readback. Focus stays on the same window; no view switch.
       `unchanged` / `not-tiled` / no tiled focus are no-write outcomes;
       sibling overlays keep their native state and skip native writes.
     - Port root H/V, nested parent-only, double-toggle unequal-share/order/
       focus, minimum allocation, lone-leaf no-op plus long-edge admission,
       floating-workspace/focus and overlay fixtures. Linux workspace tests
       compile/test portable Windows modules; Windows native
       owner/input/UI and physical Win+O takeover remain Windows-owned.
  - Item 5: shared Rust core/protocol + KDE delivered offline; zero Windows
      compile fixes required, current Windows behavior preserved. Depends on
      the unparked Windows multi-output foundation, after the single-output
      queue above. Exact sites under `crates/tiler-windows/`:

     #### Item 5 behavior and shared contract

     - Normative: decisions 5.1-5.4; [spec](spec/functional-spec.md#move)
       REQ-MOV-08/08b and [output](spec/functional-spec.md#output)
       REQ-OUT-01/04; [move matrix](spec/reference-outcomes/move.md)
       R-MOV-08/11/12/13 and [output matrix](spec/reference-outcomes/multi-output.md)
       R-OUT-01/04/07. Local restructure/swap/escape wins first; exhausted
       movement crosses all four ways, sole root leaf included. Explicit send
       crosses before exhaustion. No wrapping outputs or cycling workspaces.
     - User decision 2026-10-09: resolve candidates on FULL rectangles with
       exact touching edges and positive perpendicular overlap. Select by
       shared edge containing the moving window centre projection; if none,
       larger overlap with its span along that edge; final left/top tie-break.
       Panels cannot break adjacency; work areas still govern placement.
       No candidate no-op; unreadable refuses; no wrap. Shared core/KDE selection
       delivered offline; item 16 supplies the shared selector and fence details.
       Directional R4 landing is target edge nearest source; explicit send uses
       ordinary remembered-leaf/destination-MRU/root long-edge admission into
       destination CURRENT workspace, not R4 edge insertion. Directional focus
       keeps existing horizontal policy; vertical focus is not selected here.
     - Follow/stay as item 2: follow focuses mover on target output (target
       workspace already shown, no needless desktop switch); stay retains
       source current view/MRU or null no-setter focus. Tiled subject initially,
       sticky/intentional floats excluded; floating-workspace boundaries are
       membership-only with only tiled-side reflow. Ordinary float transfer
       remains OPEN, not implicitly implemented by output send.
     - Core types: `core/boundary.rs:48-57` `CoreCommand::Move` capability flag
       and typed same-axis mode; :170-185 `CoreEvent.directional` source-first
       pair plus `directional_target_outer_gap`, both-domain `windows`.
       `core/directional.rs:147-152` `Output.adjacent` accepts Direction four ways;
       :334 `MoveOperation::CrossOutput`. Actual Engine reply is
       `CoreReply::MoveDirectional(MovePlanReply)` (`core/boundary.rs:670`), where
       `MovePlanReply.cross: Option<MoveCrossView>` (:372-400) carries source/
       target output/workspace, mover window/leaf/direction, target occupancy,
       source root-child index and preconditions. Windows handles `plan.cross`,
       not a nonexistent `MoveOperation` field in the reply.
     - Explicit send `core/boundary.rs:105-112`
       `CoreCommand::SendToOutput {window,target_output,target_workspace,follow}`
       uses `target_domain/target_windows`, returns :669
       `CoreReply::SendOutput(SendWorkspacePlan)` with item-2 fields.
       `core/session.rs:140` `SessionCommand::MoveToOutput`,
       `core/contract.rs:411` `LifecycleIntent::MoveToOutput`;
       `core/engine.rs:1865` `transfer_request` canonical ordinary send and
       :2531 `directional_move_request` R4 planned commit. Geometry/native
       arrival remain Windows adapter work. Same workspace ID on different
       outputs is valid; workspace send stays same-output distinct-workspace.
     - Protocol `protocol/planner_protocol.rs:3758-3766` `SyncCommand::SendToOutput`
       wire `send-to-output` (missing follow true), :1948
       `serialize_send_output_reply` emits distinct kind, move-tiled operation
       + follow and desired focus/geometry. Directional adjacency parsing
       :930-1049 and fingerprint :73-166 include up/down; adapter owns geometric
       uniqueness. Windows direct Engine path needs complete validated pair
       construction, not an invocation of private codec types.
     - KDE mirrors: `kwin/src/plan-adapter-entry.ts:562` `readFullOutputRect`,
       :601 `readOutputTopology`, :677 `selectAdjacentOutput`, :2557
       `observeOutputSendTarget`, :5304 `requestOutputSend` (floating boundary),
       :5571 `focusOutputMover`; `kwin/src/workspace-send-adapter.ts:1434`
       `requestSendToOutput`, :2820 `followOutputOnce`, :2915 `confirmStayOnce`.
       Retain frozen source identity when active mover changes outputs; actual
       source visibility is a separate fence. Both views/modes/gaps/window
       sets and exact lifetime must still match before EACH membership,
       geometry and focus write; partial arrival cannot masquerade as success.
       [Engine fixtures](../kwin/tests/output-send-engine-fixture.test.ts),
       [adapter fixtures](../kwin/tests/output-send.test.ts),
       [record](changes/archive/four-direction-output-transfer.md).

     #### Item 5 bindings, exact native additions and acceptance

     | Proposed Windows id | Default chord | Intent / presets |
     | --- | --- | --- |
     | `send-output-left`, `send-output-down`, `send-output-up`, `send-output-right` | Win+Ctrl+Alt+H/J/K/L respectively | Follow, both presets keep |
     | `send-output-left-arrow`, `send-output-down-arrow`, `send-output-up-arrow`, `send-output-right-arrow` | Win+Ctrl+Alt+Left/Down/Up/Right respectively | Follow, both presets keep |
     | `send-output-left-stay`, `send-output-down-stay`, `send-output-up-stay`, `send-output-right-stay` | Unbound (four canonical rows) | Stay, both presets keep unbound |

     - KDE catalog `kwin/src/plan-adapter-entry.ts:169-201` and native constants
       `kwin/native-effect/shortcutreconciler.h:174-181` establish 8 follow + 4
       stay rows. Ownership UNKNOWN on Windows (5.3), use truthful unknown/
       containment-unproven `chord_conflict` text, extending the Ctrl/Alt early
       return in `src/settings.rs:956-957`. No new Compatible disables;
       do not borrow resize-shrink (COSMIC) or item-2 arm (niri).
       No new global field; schema-v1 binding defaults and full Ctrl+Alt action
       rebinding/queue/suppression reuse items 1/2. Native OS suppression pending.
     - Exact additions: topology from `src/tiling_sys.rs:264` `all_monitors`
       / :282 `monitor_fulls` preserving device/full/work association;
       domain bounds through `workspace_domain_for` :8540. `keyboard_tick`
       :5645 constructs directional event at :6082-6110, handle `plan.cross`
       at :6173 before geometry (:4866 `apply_geometry`), verify membership
       and native target-output arrival before one `actuate_focus` (:5402).
       Explicit output builder beside `src/workspace_owner.rs:70`
       `build_send_event` must construct DISTINCT `SendToOutput`, not merely
       retarget workspace send. Extend `planned_writes` :396 / native
       `desired_entries` (`src/tiling_sys.rs:4211`) for `SendOutput`; add output
       reply/actuation route beside `workspace_do_send` (:10552), not its same-output tail.
     - Output target is already visible: stay must not hide the arriving mover
       simply because item-2 workspace stay hides an INACTIVE destination.
       Preserve source visibility, keep target current view, reflow writable
       visible tiled rows on both outputs; only focus distinguishes follow.
       Membership-only floating boundaries use the same distinction. Native
       placement/transfer capability is part of parked item 9; do not enable
       `cross_output_transfer` before it exists. Failure converges both domains
       without setter replay, guessed rollback or success claims.
     - [ ] Portable: full-rect compass tests, no candidate, forward/reverse
       candidate selection by centre/overlap/left-top, panel gap both axes,
       local-first under both item-3 modes,
       sole-root empty/occupied targets, directional nearest-source landing
       vs explicit remembered/MRU/root admission, same workspace on different
       outputs, follow/stay/null, floating-boundary only-tiled reflow and
       excluded floats/sticky. Use real Engine reply geometry, `plan.cross`
       and `SendOutput` extraction; catalog/modifier/preset/rebind tests.
     - [ ] Windows-only: physical full/work area + mixed-DPI placement, fresh
       output/current-view/mode/gap/lifetime fences during writes, half-applied
       transfer, delayed arrival, no repeated follow; independent owned-window
       stop/restore across outputs. Common gates/docs R-MOV-08/11..13 and
       R-OUT-01/04/07; item 9 and physical acceptance remain explicit blockers.
     - User on a two-output setup: stack outputs, lower V[A*,B], upper X; exhausted
       Win+Shift+Up moves A nearest source below X, source B; fresh sole A also
       crosses. Mirror Left/Right/Down, empty and occupied destinations. With
       local move available it wins; explicit Win+Ctrl+Alt+direction instead
       transfers before exhaustion to CURRENT target workspace. Prepare
       R-OUT-07 remembered Y: admission splits Y long edge; follow focuses A,
       bind `send-output-right-stay` to an unused validated Win+Ctrl+Alt+F6,
       then stay leaves source B focused with target A visible. Panel gap
       R-MOV-13 still crosses, placement respects work area. User-readable
       multi-candidate topology selects by centre/overlap/left-top once
       implemented; unreadable/no-candidate must refuse/no-op with no writes.
       Floating-boundary frames stable/tiled-side reflow; source/target drift
       and lifetime failure cases belong in fixtures, not uncontrolled dogfood.

     - `src/tiling_sys.rs:6082-6110` retained directional event construction:
       supply complete source plus adjacent output's current workspace in
       `CoreEvent.directional`, including target outer gap, and opt into
       `cross_output_transfer` only with native transfer/fence support.
       Derive reciprocal edge-touch + positive-overlap candidates in
       all four directions from FULL monitor rectangles, not work areas;
       select by moving window centre projection, else larger window-span
       overlap, then left/top (User decision 2026-10-09); no candidate no-op,
       unreadable refuses, no wrap. Keep domain
       bounds as work areas. Local move rules and sole-leaf eligibility are
       now core-owned. Focus remains the existing horizontal policy.
     - `src/tiling_sys.rs:6173-6205` currently applies only geometry for
       `CoreReply::MoveDirectional`: handle `plan.cross` (`MoveCrossView`,
       constructed from `MoveOperation::CrossOutput` by the core) and
       native output/membership transfer before both-domain geometry, with
       exact lifetime, frozen domain/gap/mode/current-view fences, fresh
       arrival and one follow. Preserve nearest-source directional landing;
       failed/partial transfer converges without replay or success claims.
     - `src/workspace_owner.rs:70-147` / `build_send_event`,
       `stamp_send_target`: add a DISTINCT output-send builder producing
       `CoreCommand::SendToOutput { window, target_output, target_workspace,
       follow }`; resolve destination CURRENT workspace once. Workspace-send
       remains same-output. Reuse ordinary admission/canonical pair machinery;
       same desktop ID on different outputs is valid (no last-desktop gate).
     - `src/workspace_owner.rs:396-417` / `planned_writes` currently falls
       through for new `CoreReply::SendOutput`; extract its geometry.
       `src/tiling_sys.rs:10848-10884` send builder/reply dispatch and
       `workspace_do_send` are same-output patterns, not output-send wiring:
       add the new reply route and apply command follow/stay (reuse item 2),
       source MRU/null focus, visibility and live-arrival fences.
     - `src/tiling_sys.rs:10314` / `workspace_do_send_native` and
       `src/workspace_owner.rs:162-171` / `SendRoute`: extend output sends
       through floating-workspace boundaries with membership-only transfer
       and tiled-side reflow; sticky/intentional float movers stay excluded.
       Ordinary float output transfer remains OPEN.
     - `src/snapkey.rs:427` / `WorkspaceOp`, classifier/queues/suppression,
       `src/settings.rs:757-827` / `binding_catalog` and preset/rebind validation:
       distinct output-follow Win+Ctrl+Alt+arrows/HJKL and four unbound
       directional stay rows; exact Ctrl+Alt modifier routing, hold/repeat/
       release and origin lifetime. Reuse item-1/2 modifier work; Windows
       shortcut ownership is unknown, do not invent stock-holder claims.
     - Portable Linux workspace tests/clippy pass with no Windows edits.
       Port core/KDE compass, local-first, sole-leaf, full-rect panel-gap,
       multi-candidate selection, ordinary remembered/MRU/root admission, shared
       desktop, follow/stay, floating-boundary and stale/lifetime fixtures;
       native Windows acceptance remains Windows-owned.

  - Item 6: keyboard resize R-RSZ-01 (parity d) delivered 2026-10-11, base
    `f794cf9` + delivery commit ([record](changes/archive/windows-keyboard-resize.md)).
    Native gates, retained-Engine geometry, agent-observed exact-owner CLI
    grow/repeat/reversal/edge/overlay refusals and Settings Apply/Revert passed.
    Physical suppression/rebound adoption and remaining native refusal/minimum
    journeys user-owned. Both presets keep unknown-owner resize defaults
    tentatively, pending user review. Historical checklist: Table A accepted
    2026-10-07 (Windows keyboard trigger via shared Engine pixel path; P1
    adoption entry in this file, [consensus](research/reference-wm-consensus.md);
    KDE match stays B). Catalog text already documents the rows; the trigger
    is the gap.

    #### Item 6 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#resize) REQ-RSZ-01 pixel
      `cosmic_v1` path plus REQ-RSZ-01b Windows trigger; [matrix](spec/reference-outcomes/resize.md)
      R-RSZ-01 (Ours KDE: 12px press 0, then 14/16/18/20 on repeat, adjacent
      shares only, inwards reverses, minima clamp; Ours Windows:
      no-counterpart). Four outward and four inward rows, each with letter
      and arrow defaults (16 physical chords): Win+Alt grows outwards,
      Win+Alt+Shift shrinks inwards.
      Step schedule `(10 + 2 + 2 * press_index).min(20)` (`core/cosmic_v1.rs:228-240`;
      first press 12px). Dedicated resize intent; never `SnapOp::Move`.
    - Core/engine/protocol (verified 2026-10-08 at `db31234`):
      `core/boundary.rs:69-74` `CoreCommand::Resize {window, direction, mode,
      press_index}`; `core/engine.rs:892` dispatch plus :3044-3108
      `resize_request` (retained converge, `ResizeCapabilities
      {keyboard_resize: true, pointer_resize: false}`, `ResizePlanReply::
      from_keyboard`); `core/contract.rs:830-883` capability gates;
      `core/session/ops/resize.rs:49` `propose_resize`, gate :134, keyboard
      derivation :1383-1504; `protocol/planner_protocol.rs:208-211`
      `parse_mode`, :2683 `SyncCommand::Resize` parse, :2734 core mapping,
      :3712-3719 wire shape, :3824-3834 typed mapping, :1797-1858 reply
      serializer, :2112 reply dispatch, :2314 retained eval.
    - KDE reference: `kwin/src/plan-adapter.ts:3197` `requestResize` (fences:
      disabled, invalid direction/mode, busy, observe, activeExcluded,
      workspace-floating, fullscreen, maximized; repeat state
      `repeatFocused/repeatDirection/repeatMode/repeatNext` :2110-2113 and
      :3241-3257; dispatch body `{op: "resize", window, direction, mode,
      press_index}` :3253-3257); `kwin/src/plan-adapter-entry.ts:4102`
      catalog entry, delegates :7536/:7844; `kwin/tests/plan-adapter.test.ts:417`
      resize-param test plus :898 busy-refused.
    - Current Windows gap: `src/settings.rs:693` `RESIZE_ROWS` (8 ids, Alt
      outwards defaults, Alt+Shift inwards defaults), `binding_catalog`
      :769-777 marks them `implemented:false` ("not intercepted on Windows:
      Alt chords pass through untracked"). `src/snapkey.rs:104` `SnapOp`
      has only Focus/Move; `snap_repeat_live` :301 pins Win-only repeats;
      `key_op` maps :870/:1176/:1208/:1254. `src/tiling_sys.rs:5645`
      `keyboard_tick` builds Focus (:6096-6100) and Move (:6102-6110) only.

    #### Item 6 adapter checklist

    - `src/snapkey.rs`: add a Resize op beside `SnapOp` :104 (`as_str`
      :109-114); extend `snap_repeat_live` :301 with an Alt-pinned arm
      (Win+Alt held, Shift selecting inwards vs outwards per
      `binding_wants_shift` `src/settings.rs:1038`); extend `key_op` maps
      :870/:1176/:1208/:1254 and `push` :1012 / `push_owned` :1146
      Ctrl/Alt acceptance scoped to these documented resize chords only;
      carry exact Alt (+Shift) plus ACTION through intents, queues
      (`QueuedSnapEvent` :2116, `classify_and_queue` :2217), hold/repeat/
      release pins and E8 mask (:2303). Unrelated Ctrl/Alt-rebind refusal
      (`validate_bindings` :1317, `apply_rebind_text`
      `src/settings_ui.rs:403/:425`) stays.
    - `src/settings.rs`: flip each `RESIZE_ROWS` row to `implemented:true`
      as it lands; extend `chord_conflict` :954 past the Alt early return
      with truthful ownership text (Windows Alt-chord ownership is
      unrecorded: "ownership unverified in repository; containment unproven
      live", never conflict-free). Presets: `compatible_disabled_ids`
      :1528 lists implemented conflicting rows only; NO Compatible-disable
      or Authentic-takeover is recorded for Alt resize chords, so preset
      treatment is TBD, not a silent keep. KDE clears stock kwin Switch
      Window arrows through KCM Apply/Force/Revert ([shortcuts decision](decisions.md#shortcuts-conflicts-and-presets)
      2026-09-28); the Windows analogue is unselected.
    - `src/tiling_sys.rs` `keyboard_tick` :5645: add a dedicated Resize arm
      (not a `SnapOp::Move` request) constructing an ordinary single-domain
      event with `CoreCommand::Resize {window: focused mover, direction,
      mode, press_index}`; `press_index` from per-focus/direction/mode
      repeat tracking mirroring KDE :3241-3257; pre-dispatch fences mirror
      KDE :3197-3239 (suspension/lifetime, floating subject, floating
      workspace, fullscreen/maximized overlay). On `CoreReply::Resize`,
      extract via existing `planned_writes` (`src/workspace_owner.rs:412`)
      and apply through `apply_geometry` :4866 with the Move-arm fences
      (:6172-6205); pointer-resize precedent :13040-13060.
    - [ ] Portable: `tests/snapkey.rs` exact Alt/Shift routing, extra
      modifiers, held live rebind/disable, queue/mask, duplicate modifiers;
      `tests/settings.rs` catalog flags, conflict text, preset TBD behavior;
      `tests/tiling.rs` retained-Engine resize geometry: 12px first press,
      14/16/18/20 repeats, adjacent-shares-only, inward reversal, one-sided
      minima clamp, no Move semantic. Port `crates/tiler-core/tests/session_resize.rs`
      semantics (`horizontal_both_directions` :358, `vertical_both_directions`
      :389, `edge_refusals_are_unchanged` :516, `normalization_and_clamp_to_exhaustion`
      :565, `focus_retained_and_plan_carries_semantics` :648).
    - [ ] Windows-only: owner hook release/suppression plus settings
      Keep/Disable/Rebind Apply/Revert for the 8 rows; native chord
      suppression needs Windows evidence.
    - User journey: use horizontal and vertical tiled pairs, focusing the
      member with a shared boundary in each requested direction. Win+Alt+
      H/L/J/K plus arrows grow outward once each (12px before clamps);
      outer edges without a resize boundary remain no-ops. Win+Alt+Shift variants
      shrink inward (reversal); repeat-hold one direction to the 20px cap;
      drive one pair into its minimum (clamp one-sided, no axis search);
      Apply/Revert plus takeover-off release; unrelated Ctrl/Alt chords
      still pass through.
    - DoD: all 8 rows resize through the shared pixel path with
      press_index steps; no Move semantic; catalog/conflict/preset-tests
      updated with preset TBD recorded; matrix `R-RSZ-01` Ours Windows cell
      updated with dated offline evidence, native pending.

  - Item 7: press-focus R-DRAG-08 delivered 2026-10-11, base `9d12c7f`
    plus delivery commit ([record](changes/archive/windows-drag-press-focus.md));
    native gates passed; synthetic no-move press, Esc and stationary-preview
    drop agent-observed. Physical/resize/refusal-race checks user-owned;
    native title-bar path intact (same-topology apply observed). Historical
    checklist: User decision 2026-10-07 (R-DRAG-08;
    [decisions](decisions.md#pointer-drag-and-drop)): a Meta/Win client move
    drag focuses the dragged window at press on both platforms. Windows
    changes from activate-on-drop; KDE timing needs a live check ([pending
    live checks](#pending-live-checks)).

    #### Item 7 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#drag) REQ-DRAG-08 plus
      REQ-DRAG-03 producer note; [matrix](spec/reference-outcomes/mouse.md)
      R-DRAG-08 (Ours Windows today: foreground retained during hold, B
      activated on valid drop; Ours KDE: exact focus timing TBD).
      R-DRAG-07 stationary-source/moving-preview split and parked underlay
      C stay unchanged.
    - Current Windows exact (verified 2026-10-08 at `db31234`): Win+Left
      arm `windrag_down` (`src/tiling_sys.rs:12427`) validates tiled-only
      published origins, live identity/tag/token (`no-token`,
      `stale-snapshot`), captures gesture start (`capture_gesture_start`
      :12342), sets `gesture_producer` "windrag", `windrag_start_cursor`,
      `windrag_bound`, `move_kind` Move and `active`. Focus is bound later
      at drop-settle in `gesture_tick` :12900-12935 via `actuate_focus`
      (:5402, E8-prime plus attach plus one setter with exact readback),
      logged `windrag-focus`; failed actuation refuses with snap-back, no
      plan. Model: `src/win_mouse.rs:38` `WinDragSnapshot`, :51
      `WinDragEdge`, :71 `validate_windrag_down`, :90 `WinDragKind`, :104
      `WinDragQueue`, :329 `settle_pointer_journey`. Title-bar drag runs
      the native modal loop (OS focuses at press; no project code); Esc
      cancel is `clear_windrag_gesture` :12531 (no plan, no geometry change).
    - KDE: Ours KDE cell timing is TBD by live check, not by adapter work;
      oracle grab/drag route diagnostics live at
      `kwin/src/plan-adapter-entry.ts:5888-6073` (`grabSource`) and
      :6864-6894. Port evidence: `kwin/tests/drag-press-evidence.test.ts`.
    - Core/drop seam: `core/boundary.rs:113-125` `DragDrop` carries the
      mover and hover prior, not native press-focus authority. Windows
      `src/tiling_sys.rs:13018-13028` constructs that command only at drop;
      :13051-13074 applies the retained reply. Press activation adds no
      Engine mutation or layout write to that unchanged drop contract.

    #### Item 7 adapter checklist

    - `src/tiling_sys.rs`: bind mover focus in `windrag_down` :12427 right
      after the `active` insert (all START fences already passed:
      unmanaged, unknown-subject, busy, identity-changed, stale-snapshot,
      no-token, no-pre), using the same `actuate_focus` :5402 authority as
      the drop path. On press-actuation failure, refuse the arm with
      snap-back and no plan (mirror :12912-12928). At drop-settle
      :12900-12935, avoid redundant activation only after fresh foreground
      readback; preserve settle-time refusal if the required focus cannot
      be verified. Keep every other gate (START identity/lifetime, member match,
      sticky/float refusal, domain routing, same-output fence) and the
      Engine plan path unchanged. `windrag_up` :12508 and
      `clear_windrag_gesture` :12531 unchanged; title-bar path untouched.
      No new chord; no settings/catalog/preset row is touched.
    - [ ] Portable: `src/win_mouse.rs` model tests (`validate_windrag_down`
      edges, queue cap/discipline, `settle_pointer_journey` ordering) plus
      new press-ordering cases: unfocused-B press binds focus before any
      move; no-move press yields focus with zero layout writes; Esc cancel
      keeps focus with zero writes; drop at edge transfers with the mover
      focused; failed press actuation snaps back with no plan. Port
      `kwin/tests/drag-press-evidence.test.ts` ordering assertions.
    - [ ] Windows-only: owner-side arm/drop/cancel with exact foreground
      readback; native foreground evidence on press.
    - User journey: unfocused-B Win+Left press with no move (B focused,
      layout unchanged); Esc cancel (B stays focused, no write); press,
      move, drop at A's edge (transfer, B focused); underlay C remains
      parked.
    - DoD: press-focus on Win client drag with no drop-path regression;
      R-DRAG-07 split preserved; matrix `R-DRAG-08` Ours Windows cell
      updated with dated offline evidence, KDE timing still live-TBD.

  - Item 8: R-RST-01/R-FLT-05 delivered 2026-10-11, base `e48aed1` plus
    delivery commit ([record](changes/archive/windows-restart-intent.md));
    native gates and owned-helper double-restart/sticky-scope journeys passed.
    Physical/app dogfood pending; post-restart un-stick TBD;
    REQ-RST-01c NORMATIVE D25, membership/set/focus native acceptance pending;
    D7 fixed-origin marker now delivered offline with item 13
    ([record](changes/archive/windows-fixed-size-admission.md)). Stronger names and exact-owner float/sticky
    CLI are tentative, pending user review. Historical checklist follows.
    Restart R-RST-01 plus R-FLT-05 sticky persistence. User
    decisions 2026-10-07 (R-FLT-05, R-RST-01 float identity;
    [decisions](decisions.md#restart-persistence)): sticky floats stay
    sticky across owner restart including Windows, delivered with the
    R-RST-01 work. KDE Q3 delivered offline 2026-10-08 under user decisions
    2026-10-08 (normative as delivered);
    [record](changes/archive/kde-intentional-float-restart.md).
    Windows code/behavior unchanged in that delivery. KDE D7 tile-override
    restart store [delivered offline](changes/archive/fixed-window-tile-override-restart.md);
    Windows still lacks float persistence and tile overrides remain a gap.
    User decision 2026-10-10 (mechanism, option A): on-window `SetProp`
    markers, same pattern as `OmniTilerSticky`; intentional-float and
    fixed-window tile-override markers added. Names carry the distinctive
    project prefix to minimise collision with other processes. A marker
    restores classification only, never authorizes writes or recovery
    ownership; written after native success, removed on unfloat/sticky-off/
    re-float; unreadable means no intent, logged. No separate store file and
    no recovery-ledger change. Post-restart un-stick remains TBD;
    REQ-RST-01c is NORMATIVE D25 with native acceptance pending.

    #### Item 8 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#startup) REQ-RST-01/01b plus
      REQ-FLT-05; [matrix](spec/reference-outcomes/restart-persistence.md)
      R-RST-01 (Ours KDE: healthy runtime store hydrates intentional F before
      first planning, degraded store diagnoses then adopts empty; Ours Windows:
      runtime store resets while settings persist without layout restore) and
      [matrix](spec/reference-outcomes/floating.md) R-FLT-05. Sticky stays
      sticky and intentional floats survive owner restart. REQ-RST-01c
      membership/set/focus is NORMATIVE D25, native acceptance pending.
      KDE's selected private runtime store
      is not a selected Windows storage scheme.
    - Core seams (verified 2026-10-08 at `db31234`): `core/session.rs:546`
      `exceptions` map, `core/session/world.rs:433` `is_exception` (Engine
      float/settle reads at `core/engine.rs:835/:1467`), `core/session.rs:148`
      intentional-float transition. Exceptions ride the session, never the
      settings file.
    - KDE Q3 runtime (2026-10-08 offline delivery): `kwin/src/plan-adapter.ts`
      `startIntentBootstrap` hydrates before planning; the existing Q2
      successful-application point calls `updateSettledIntentFromFlight`.
      `crates/omnitiler/src/float_intent_store.rs` owns the private
      runtime membership file; existing Planner1 gains `ReadFloatIntent` /
      `WriteFloatIntent`, with same-UID + current-KWin-owner authorization.
      Bus ID + KWin unique owner fence the namespace. Membership only; no
      geometry/focus/stacking/pre-sticky history or automatic fixed origin.
      No effect dependency or new shared adapter extraction. These are KDE
      implementation seams, not instructions to port D-Bus/file storage to Win.
    - KDE design handoff (user decisions 2026-10-08, Q3 plus D7 delivered offline): recovered
      ordinary floats retain the live frame with no hydration writes. Only
      settled explicit intent is durable; scoped/hidden/minimized absence
      does not clear it. Missing state is empty; corrupt/unreadable/mismatched
      reads diagnose then proceed empty. Write failure preserves native/local
      truth and the next settled membership update rewrites the full set.
      Storage ACK is distinct from native application; no atomicity claim.
      Ordinary intent and automatic fixed origin remain separate (R-RST-04,
      Windows item 13). D7 adds disjoint fixed-window tile membership to the
      same store, no positions: persist after successful application, clear
      on re-float/close, prune only complete inventory. Matched overrides
      survive hint/predicate changes; missing/degraded/unmatched records
      recompute (fixed -> untouched float). Windows must deliver this with
      item 13 after selecting its item-8 persistence mechanism; no KDE D-Bus
      storage requirement for Windows. Recovery ownership must never derive from marker alone.
    - Windows exact (verified 2026-10-08 at `db31234`): runtime intent
      `state.floated: BTreeSet<WindowKey>` (`src/tiling_sys.rs:1452`,
      init empty :13813); float toggle inserts :7361/:8103, unfloat
      removes :7668, close cleanup :4150; domain-release carry via
      `float_carry_tokens` (`src/workspace_owner.rs:207`, used :4286;
      fixtures :1352 `float_carry_survives_a_fresh_engine_session`);
      Engine read `engine_is_float` :3963. Sticky lane: `state.sticky` map
      plus on-window SetProp markers `src/tiling.rs:450`
      `sticky_marker_value` (1 = pre-sticky tiled, 2 = pre-sticky float)
      and :457 `parse_sticky_marker`; `src/product_hide.rs:487-547`
      read/install/remove; adoption fixtures
      `src/workspace_owner.rs:2894` roundtrip and :3245
      `sticky_adopt_consumes_both_markers_as_normal_float_then_tiles`.
      Runtime maps (`member_tokens`, `hidden_claims`, `member_identity`,
      `member_tags` :1280-1303) never serialize. These existing maps and
      recovery records are not a selected durable float-intent mechanism.

    #### Item 8 adapter checklist

    - `src/tiling_sys.rs` + `src/workspace_owner.rs`: on fresh observation
      after owner restart, restore the accepted intentional-float/sticky
      classification before first tiling. Current marker-adoption fixtures
      consume both origins as ordinary float (:3245); that is the gap,
      not the intended restart outcome. Preserve exact-lifetime admission
      fences (`member_tokens`/`member_identity`/`member_tags`,
      `visible_lifetime_ok`, `reused_hwnd_stale`) and independent recovery.
      Do not authorize writes from a marker or guessed HWND alone.
      Mechanism: prefixed on-window markers (user decision 2026-10-10 above).
      REQ-RST-01c membership/set/focus is NORMATIVE D25, native acceptance pending. Existing
      live sticky toggles retain the two pre-sticky origins; the exact
      post-restart un-stick result is not selected by R-FLT-05 and stays TBD.
      No new shortcut/catalog row or preset is selected.
    - [ ] Portable: `float_carry_survives_a_fresh_engine_session` :1352,
      marker roundtrip :2894, sticky-adopt :3245, plus new offline-restart
      cases: surviving sticky marker re-adopts as sticky (both origins),
      intentional ordinary float survives, marker mismatch fails closed,
      recycled HWND refuses. Keep same-owner un-stick origin tests; leave
      post-restart un-stick assertions TBD under R-FLT-05/R-RST-01. Core:
      `crates/tiler-core/tests/restart_disjoint_adoption.rs` and
      `crates/tiler-core/tests/session_lifecycle.rs` exception
      fixtures. Windows: `tests/restore.rs` (reveal/skip/missing/ambiguous),
      `tests/storage.rs` roundtrip/refusal, `tests/lifecycle.rs`.
    - [ ] Windows-only: owner restart with live windows across all three
      lanes; ledger/journal crash recovery unchanged.
    - User journey: two sticky floats (one made sticky from tiled, one
      from intentional float), one ordinary intentional float and one tile;
      restart owner; both sticky windows remain sticky and the ordinary
      float remains floating. Observe and record un-stick results for both
      origins without selecting a post-restart outcome; membership/focus
      native acceptance stays TBD under NORMATIVE REQ-RST-01c (D25).
      No saved-layout restoration.
    - DoD: sticky/intentional floats survive restart without identity
      guessing; post-restart un-stick remains TBD; REQ-RST-01c native acceptance pending;
      matrix `R-RST-01` and `R-FLT-05` Ours Windows cells updated with
      dated offline evidence, native journey TBD.

  - Item 9: fullscreen send R-MAX-09 delivered same-output 2026-10-11,
    base `6b76589` plus delivery commit ([record](changes/archive/windows-fullscreen-workspace-carry.md));
    native gates passed; CLI numbered follow/stay, relative follow and exit
    to target allocation agent-observed. Physical/flag-race/app-owned/game/
    cross-output pending; fullscreen follow focus suppression and exact-owner
    fullscreen CLI are tentative, pending user review. Historical checklist:
    Table A accepted 2026-10-07 carries state without restoring first. Depends on
    handoff item 2 follow/stay wiring only; same-output workspace carry,
    never the parity-queue multi-output foundation.

    #### Item 9 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#maximize) REQ-MAX-09;
      [matrix](spec/reference-outcomes/maximize-fullscreen.md) R-MAX-09
      (Ours Windows today: tiled maximized B sends through the retained
      Engine route with follow, fullscreen B refuses with no writes;
      Ours KDE: same-output desktop-send outcome TBD, R4 cross-output
      Engine path cited). KDE observe-first/maximize leg stays OPEN; only
      workspace carry is selected, never output-fullscreen carry.
    - R-MAX-05 boundary (not a send outcome): the project-toggle refusal on
      app-owned fullscreen without a preimage is a separate invariant
      (user decision 2026-10-07 R-MAX-05; [matrix](spec/reference-outcomes/maximize-fullscreen.md)
      R-MAX-05 Ours Windows; `src/tiling_sys.rs:6808`
      `fullscreen-refused-app-owned`). This item preserves that toggle path
      untouched; it selects fullscreen workspace-send carry only.
    - Current Windows gates (verified 2026-10-08 at `db31234`):
      `workspace_do_send` (`src/tiling_sys.rs:10552`) pre-dispatch overlay
      gate :10713-10737 (retained mover: fullscreen refuses
      `send-refused-fullscreen` :10728-10730, maximized proceeds through
      `send_flags_stable` plus `is_zoomed_now` rechecks); native boundary
      `workspace_do_send_native` :10314 with its own gate :10374; retained
      tail :10900-10945 (same fullscreen/maximized branches, post-tag
      lifetime recheck, `assign`, verified transfer before follow);
      builder/reply :10848-10884. `src/workspace_owner.rs:70`
      `build_send_event`, :138 `stamp_send_target`, :170-180 `SendRoute`,
      :396 `planned_writes`. Core: `core/engine.rs:1858-1865`
      `transfer_request`, `core/boundary.rs:93-112`
      `SendToWorkspace`/`SendToOutput`.
    - KDE seams, not carry acceptance: numbered workspace send enters
      `kwin/src/plan-adapter-entry.ts:4964` `requestWorkspaceMove`; tiled
      routing uses `kwin/src/workspace-send-adapter.ts:1309` `requestSend`
      and :2635 `followOnce`. Its :1363-1365 `non-tiled-focus` gate and
      native send behavior are context for the observe-first KDE R-MAX-09
      row, not authority to select a KDE fullscreen outcome.

    #### Item 9 adapter checklist

    - `src/tiling_sys.rs`: at the :10728-10730 and :10914 fullscreen
      branches, let a project-fullscreened mover proceed exactly like the
      maximized leg beside it: live flag-stability recheck, ordinary
      `transfer_request` admission (remembered leaf, destination focus
      history, root fallback), target allocation kept, overlay geometry
      never written, command follow/stay per handoff item 2 (follow
      selects/reveals target and focuses mover after verified transfer;
      stay hides the mover without selecting target and applies source
      MRU/null focus), verified transfer before follow. Retain every
      exclusion (unmanaged, suspended, identity/lifetime) and setter
      isolation; R-MAX-05 toggle path untouched. Any genuinely unresolved
      mover subcase gets a TBD governing matrix row, never an invented
      refusal.
    - [ ] Portable: `src/workspace_owner.rs` send fixtures
      (`send_event_binds_target_and_focused_mover` :1251,
      `send_plan_scopes_source_reflow_through_writable_subset` :2229) plus
      new fullscreen-carry cases: true-Engine source/target allocation
      equality with the maximized leg, overlay geometry skip (zero native
      writes for the mover), follow vs stay focus/visibility, maximized vs
      fullscreen distinction; `tests/tiling.rs` retained-Engine send
      coverage with lifetime/arrival/recovery.
    - [ ] Windows-only: Engine/native-boundary tails, source-selected
      fences before writes and focus, hidden target reveal policy, recycled
      lifetime, partial-transfer reconciliation without replay.
    - Settings/input/catalog/presets: reuse handoff item 2's follow/stay
      actions and exact-modifier routing. Current numbered send rows are
      `src/settings.rs:823-826` / `workspace_row` :830; this carry change
      selects no additional binding, conflict, preset or UI control.
      `apply_preset` :1490 / `compatible_disabled_ids` :1528 stay under
      item 2's recorded policy, not a new fullscreen-specific policy.
    - User journey: project-fullscreened B on WS1, WS2 occupied; follow
      send (B arrives still fullscreen, source collapses to A, B focused);
      fresh fixture, stay send (WS1 stays shown on A, B fullscreen on
      hidden WS2, select WS2 to verify); maximized leg repeats unchanged;
      native restore/exit of arrived B behaves as before.
    - DoD: workspace fullscreen carry without prior restore; exclusions and
      R-MAX-05 toggle refusal intact; matrix `R-MAX-09` Ours Windows cell
      updated with dated offline evidence, KDE leg still observe-first,
      native TBD.

  - Item 10: float/half-snap parity (a), R-FLT-07..11. User decision
    2026-10-05 float-nav ([decisions](decisions.md#window-state-float-sticky-maximize-fullscreen));
    KDE delivered offline ([record](changes/archive/kde-floating-directional-navigation.md)).
    Windows delivered offline 2026-10-12 from base `e506350`
    ([record](changes/archive/windows-floating-directional-navigation.md)); native
    MSVC gates pass. Focus candidate/reconcile fixtures include retained maximum
    and visible non-home sticky targets; automatic fixed floats retain D8 no-touch.
    Ordinary focus/half-snaps user-accepted physically 2026-10-12, scoped normal
    owner, single output, Notepad/Paint, base `e506350` plus combined deliveries.
    Tie/sticky/clamp/preset/fixed/game/DPI/multi-output pending. Historical seams below.

    #### Item 10 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#floating) REQ-FLT-07/08/09/
      09b/10/11; [matrix](spec/reference-outcomes/floating.md) R-FLT-07..11.
      Tile-origin focus lands on tiles, floats never targeted; float-origin
      focus selects same-domain floats by top-left axis distance, considering
      sticky entries before ordinary entries in encounter order; nearer
      ordinary entries can win. Up/Left ties keep first, Down/Right last. Miss
      retains focus and reuses the recorded horizontal output-edge behavior
      without COSMIC workspace cycling; float-origin Meta/Win+Shift+arrow
      snaps to the stateless work-area half, stays floating and focused.
      Quarters, maximize and repeated-outward transfer states deferred.
    - KDE exact (verified 2026-10-08 at `db31234`):
      `kwin/src/plan-adapter.ts:944` `floatSubjectEntry` (focused
      same-domain float/sticky gate), :974 `selectFloatFocusTarget`
      (sticky entries first, then ordinary in one axis-distance race;
      Up/Left admit non-positive deltas and keep first tie, Down/Right
      strictly positive and keep last), :1036 `floatHalfSnapRect`
      (inner-gap formula over work-area bounds, trunc halves), :2460
      `requestFloatFocus` via dispatch :2433, :2533 `requestFloatMove` via
      :3129 (exactly one `setGeometry` plus focus retention; no Engine
      admission, no dispatch, no overlay clear, repeats request the same
      half again). Fixtures: `kwin/tests/plan-float-focus.test.ts`,
      `kwin/tests/plan-float-move.test.ts:144-282` (four-direction
      arithmetic, trunc, sticky parity, stateless repeats, stale refusal).
    - Core: `core/boundary.rs:58-67` Focus op (`float_subject` flag),
      `core/engine.rs:2840/:3022` focus proposal,
      `core/session/ops/focus.rs:26` `propose_focus`; cross-output float
      focus leg covered in
      `crates/tiler-core/tests/session_float_focus_cross.rs`.
      Half-snaps are adapter-native geometry (no Engine plan) on KDE;
      Windows must do the same, never an Engine admission.
    - Current Windows exact (verified 2026-10-08 at `db31234`):
      `src/tiling_sys.rs:5911-5935` sticky refusal (`focus-refused-sticky` /
      `move-refused-sticky`) plus float refusal (`focus-refused-floating` /
      `move-refused-floating`); `CoreCommand::Focus` :6096 with
      `float_subject:false` :6100; focus actuation `actuate_focus` :5402;
      float read `engine_is_float` :3963.

    #### Item 10 adapter checklist

    - `src/tiling_sys.rs` `keyboard_tick` :5645: before the Engine
      Focus/Move build, branch on float subject (`engine_is_float` :3963
      or `state.sticky` member, mirroring the :5911-5935 gates): Focus
      runs the ported `selectFloatFocusTarget` over same-domain observed
      floats (one distance race, sticky encounter-order first, same tie rules) via
      `actuate_focus` :5402; miss retains focus and reuses the existing
      horizontal output-edge path, never workspace cycling and never a new
      crossing. Move runs the ported `floatHalfSnapRect` over the work-area
      bounds plus inner gap, performs exactly one geometry write with the
      existing writable/hidden/minimum fences, and retains focus on the
      same subject; no Engine call. Keep the refusal vocabulary for
      ineligible subjects, invalid geometry, stale subjects and the
      deferred quarter/max/outward states.
    - [x] Offline: `tests/tiling.rs`, cfg(windows) retained Engine fixtures and
      existing `tests/snapkey.rs` catalog/pairing coverage (existing
      focus/move rows, no new chords): nearer ordinary vs farther sticky,
      first/last tie selection, miss
      retention, repeated half-snaps stateless with exact allocation,
      tile-origin skip, deferred states still refused. Port
      `kwin/tests/plan-float-focus.test.ts` axis/tie cases and
      `kwin/tests/plan-float-move.test.ts:144-282` arithmetic/repeat/stale cases.
    - Settings/input/catalog/presets: existing `FOCUS_MOVE_ROWS`
      (`src/settings.rs:575`) / `binding_catalog` :757-768 are implemented;
      retain their Keep/Disable/Rebind and `compatible_disabled_ids` :1528
      conflict policy. Half-snaps use current inner-gap settings; no new
      shortcut, preset rule or persistent snap-state setting is selected.
    - [ ] Windows-only: single-write readback, minimum-hint clamp,
      mixed-DPI work-area placement; native suppression evidence.
    - User journey: float-to-float Win+arrows (ordinary pair, then sticky
      pair: verify encounter-order tie rules); Win+Shift+arrows all four halves
      twice (same half again); miss holds focus; tile-origin Win+arrows
      ignore floats.
    - DoD: REQ-FLT-07..11 behave with deferred states still deferred;
      matrix `R-FLT-07..11` Ours Windows cells updated with dated offline
      evidence, native TBD.

  - Item 11: born-max/floating-retile overlay parity (b), Q3 including
    R-MAX-03. User decisions Q3 plus Q3 scope 2026-10-07
    ([decisions](decisions.md#window-state-float-sticky-maximize-fullscreen)): born-maximized
    tiles with a reserved slot and keeps maximize as an overlay, no launch
    unmaximize, on KDE and Windows; Q3 also covers R-MAX-03
    floating-to-tiled admission, replacing the one-shot restore on both.
    KDE R-MAX-06 delivered offline, R-MAX-03 delivered offline in
    `29c75fe` ([record](changes/archive/kde-maximized-floating-retile-overlay.md),
    born-max [record](changes/archive/kde-born-maximized-overlay.md)).
    R-MAX-06 now has the 2026-10-11 setting: Windows both values delivered
    offline ([record](changes/archive/windows-born-maximized-setting.md));
    R-MAX-03 delivered offline 2026-10-12, independent of that setting
    ([Windows record](changes/archive/windows-maximized-floating-retile-overlay.md)).
    Native MSVC offline gates pass; ordinary retile/restore and both-values born
    Paint launch user-accepted physically 2026-10-12 under item 10's conditions.
    Tray toggle previous-focus non-restoration is observation only; no change
    requested. Hidden/session/fixed/game/DPI/multi-output remain pending.

    #### Item 11 behavior and reference seams (historical handoff)

    - Current Windows seams: `retile_keep_overlay`, `admit_slotless_maximized`,
      `admission_clear_planned`, `clear_maximize_at_admission` and assembly
      slot seeding reuse born-overlay machinery. R-MAX-03 no longer clears;
      D5 first fullscreen exit is delivered offline with item 13. Historical
      line numbers and pending implementation descriptions below are superseded
      by the Windows record; native checklist remains open.

    - Normative: [spec](spec/functional-spec.md#maximize) REQ-MAX-03/03b/06;
      [matrix](spec/reference-outcomes/maximize-fullscreen.md) R-MAX-03
      (Ours KDE: floating skips slot seeding, retile reserves the slot,
      native unmaximize lands in it; Ours Windows: one-shot retile restore
      still in code) and R-MAX-06 (Ours KDE: reserved-slot overlay, no
      launch clear; Ours Windows: one admission-time clear attempt,
      retained slots/fullscreen/floating domains exempt). No launch
      unmaximize, no one-shot retile restore. First fullscreen exit is separate
      (D5 core/KDE delivered offline, Windows pending); B9 unmaximize/fresh-admit is item 15;
      R-MAX-08 focus fence/unmaximize-before-move is selected 2026-10-10,
      implemented separately under handoff item 18.
    - KDE exact (verified 2026-10-08 at `db31234`):
      `kwin/src/plan-adapter.ts:5933` `clearMaximizeAtAdmission` (skips
      fullscreen, skips attempted ids via `maximizeAdmissionAttempts`,
      clears only the non-fixed exact-ref-held born-fullscreen exit
      `heldInitialFullscreen`, echo arm/clear, observed refetch); call
      sites :5476/:5791; env `clearMaximize` :626; overlay = retained
      tile slot plus `fit_excluded` / `skip-maximized` isolation with
      quiet repeated/synchronous signals (matrix R-MAX-03/06 KDE cells).
      Fixtures: `kwin/tests/plan-adapter.test.ts`.
      Carried projection is `kwin/src/plan-adapter.ts:2721-2740`, payload
      `fit_excluded` :6071 and native `skip-maximized` :8338-8339. Shared
      `core/engine.rs:478-512` converges the carried set; core projection
      `core/boundary.rs:802-807` omits excluded frames from observed-clamp
      assessment while retaining planned geometry. No new Engine behavior
      is selected by this parity piece.
      The decisions register records KDE R-MAX-03 delivered offline at
      `29c75fe`; Windows parity and native acceptance remain pending.
    - Windows exact (verified 2026-10-08 at `db31234`):
      `src/tiling.rs:550` `should_clear_maximize_at_admission`,
      :567 `should_admit_slotless_maximized`; callers
      `admission_clear_eligible` (`src/tiling_sys.rs:3785`),
      `clear_maximize_at_admission` :3810 (attempt-key
      mark-before-native-call, `restore_zoom_placement`, assembly
      :3802-3898), `admit_slotless_maximized` :8726 (is_member,
      `member_rects`, holdable-key, lifetime gates),
      `admit_born_fullscreen` :8622. First-exit lane kept:
      `FullscreenMeta` read/write :3400-3496, `enter_fullscreen` :3520,
      `exit_fullscreen_owned` :3574, `track_fullscreen_holds` :3713.
      Current overlay refusal lane: `float_toggle_refusal` (`src/tiling.rs:402`)
      and `sticky_toggle_refusal` (:418); B9 intentional-maximized-unfloat
      refusal is a separate implementation gap. Fixture:
      `floating_retained_max_stays_slotless_then_tiled_seeds_and_plans`
      (`src/tiling_sys.rs:17164`).
      Row assembly is `src/tiling_sys.rs:4243`, retained maxima :4423-4468;
      slot seeding calls `src/tiling.rs:581` `should_seed_member_slot` at
      :4432, with canonical retained rectangles from `src/tiling.rs:480`.

    #### Item 11 adapter checklist

    - `src/tiling.rs` + `src/tiling_sys.rs`: replace the one-shot clear on
      both admission legs (slotless maximize :8726 and tiled clear :3810;
      :8622 is the separate born-fullscreen hold) with the reserved-slot
      overlay: retain membership, seed the slot (`member_rects`), skip native
      clear and geometry writes while overlaid, land native unmaximize in
      the slot, settle repeated maximize/geometry signals quiet with no
      unsolicited toggles or fighting. Retire
      `should_clear_maximize_at_admission` one-shot use; keep the held
      born-fullscreen single exit unchanged within this unit (D5 is separate).
      B9 unmaximize/fresh-admit is separately owned by item 15; preserve
      all unrelated overlay refusals. No new chord, setting or R-MAX-08 change.
    - Settings/input/catalog/presets: reuse current native/project maximize
      and workspace tiled/floating controls. Catalog toggles remain in
      `src/settings.rs:799-818`, workspace default :250-263, Settings UI
      `src/settings_ui.rs:314/:394` and tray `src/tray.rs:179-275`; no new catalog row or
      `compatible_disabled_ids` :1528 policy is selected by Q3.
    - [x] Offline Engine/adapter: both born-setting values, reserved slot and
      sibling share, restore projection, quiet repeat and reused lifetime.
    - [ ] Remaining native/application scenarios: born-max beside a sibling
      (sibling keeps its allocated share), hidden-workspace retile,
      floating-toggle retile, native restore to the exact retained slot,
      quiet repeated signals, reused IDs, session-boundary restored max
      with no loop.
    - [ ] Windows-only: native maximize/unmaximize roundtrip into the
      slot; graceful/crash recovery of overlaid members.
    - User journey: born-maximized launch beside a sibling (slot reserved,
      overlay quiet); native unmaximize lands in the slot; repeat
      maximize/unmaximize settles quiet; floating workspace with maximized
      A beside ordinary B, toggle tiled (A stays maximized over its slot,
      B takes its share), native unmaximize lands in the slot; repeat
      with a session-restored maximized A at a user-owned session
      boundary, no loop.
    - DoD: one-shot clear replaced by reserved-slot overlay on both legs;
      first-fullscreen-exit separate (D5 Windows delivered offline) from B9; D6 fixed
      automatic workspace-enable exceptions stay slotless per item 13;
      matrix `R-MAX-03` /
      `R-MAX-06` Ours Windows cells updated with dated offline evidence,
      native TBD.

  - Item 12: non-local workspace modes parity (d). Decisions 1.2/1.5
    ([decisions](decisions.md#workspaces)): local and
    global-unique keep per-output history/rings, shared keeps one
    history/ring. KDE mappings delivered offline; Windows local-only
    today. Depends on handoff items 1/2 (history/ring + follow/stay) and
    the parity-queue multi-output foundation; last.

    #### Item 12 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#workspaces) REQ-WS-08/11
      (Win columns: history/toggle and relative-switch implementation
      pending, non-local modes pending);
      [matrix](spec/reference-outcomes/workspaces.md) R-WS-08/11 plus
      R-WS-15 (per-output isolation vs shared), R-WS-16 (removed-ID
      invalidation), R-WS-17 (hotplug history/scope). KDE mappings are
      selected and the spec carries scoped history/rings; any exact
      Windows outcome not defined is a TBD governing row, never invention.
    - KDE exact (verified 2026-10-08 at `db31234`):
      `kwin/src/workspace-native.ts:990` `selectPrevious` (shared branch
      :995 `selectPreviousShared` :2250; per-output `previousByOutput`,
      `previousInScope`, `swapGlobalIfVisibleElsewhere` for global-unique),
      :1051 `selectRelative` (shared branch :1059 `selectRelativeShared`
      :2281; `rebuildGlobalMapping` vs `rebuildLocalMapping`,
      `scopedRingIds` :2203), :2141 `recordHistoryObservations`, :2185
      `observeSharedHistory`. Fixtures:
      `kwin/tests/workspace-previous-relative.test.ts`.
      Current shared Rust policy `core/workspace.rs:7-17` resolves local
      ordinals, and :35 `plan_trailing` handles local cleanup. It does not
      supply the KDE global/shared mapping machinery; preserve D1 ownership
      if this adapter parity needs a new shared-core contract.
    - Windows exact (verified 2026-10-08 at `db31234`):
      `src/workspace.rs:95-109` `ManagedWorkspaces` fields are
      outputs/displaced/membership/next_ws/next_output/default_tiled/tiled:
      no previous-history or relative-ring resolver, no mode-scope field.
      Workspace order exists; mode provision is
      per-workspace tiled/floating ONLY: `is_tiled` :150, `set_tiled`
      :166, `default_tiled` :133/:139, settings
      `core.workspace.default_tiled` (`src/settings.rs:250-263`), settings
      UI :314/:394, tray per-workspace toggle (`src/tray.rs:179-275`). No
      global/local/shared scope setting exists anywhere in settings, UI
      or tray; the tray workspace-tiling action is a per-workspace
      tiled/floating toggle, never a global mode selector. Routing:
      `src/tiling_sys.rs:9819` `workspace_do_select`, verified
      hide/reveal :10061, `workspace_tick` :11395, selection arm :11530,
      `poll_foreground_workspace` :11912, `sync_monitor_outputs` :12050,
      `chord_output` :8508, `src/workspace_owner.rs:25`
      `output_context`. Cleanup `plan_cleanup`/`apply_cleanup`
      (`src/workspace.rs:453-508`) invoked :9228-9234 and :10074-10080.

    #### Item 12 adapter checklist

    - `src/workspace.rs` `ManagedWorkspaces`: add mode scope
      (local/global-unique/shared) plus per-output previous/observed
      history and scoped ring resolvers from handoff item 1 (pure
      stable-ID resolve, no activation/append/trailing creation);
      global-unique swaps the visible-elsewhere entry instead of
      duplicating selection (KDE `swapGlobalIfVisibleElsewhere` parity),
      shared keeps one history/ring. Record actual completed transitions
      at :10061 (including failed focus/geometry), `workspace_tick`
      :11530, foreground :11912 and monitor sync :12050; invalidate
      removed/unassigned/out-of-scope ids through the existing cleanup;
      discard disconnected-output history, never consult history for
      reconnect selection. Membership/visibility/recovery routing follows
      the transfer. No new shortcut, no new preset, no tray command.
    - [ ] Portable: local vs global-unique vs shared isolation (R-WS-15),
      surviving-empty vs removed/moved id (R-WS-16), disconnect/displacement/
      reconnect/return scope with conditional second toggle (R-WS-17),
      output-focus-alone non-recording, cross-output previous and relative
      selection, >9/trailing wrap per scope. Port
      `kwin/tests/workspace-previous-relative.test.ts` case for case.
    - Settings/input/catalog/presets: existing workspace shortcut rows
      (`src/settings.rs:819-826`) and handoff items 1/2 supply the selected
      chords; `compatible_disabled_ids` :1528 remains their policy. Scope
      configuration exposure/schema and live mode-change transition behavior
      are not selected by the provision at `decisions.md:169`; resolve those
      before adding controls. Existing `WorkspaceSettings` :250-263, UI
      `src/settings_ui.rs:314/:394` and `src/tray.rs:179-275` are
      tiled/floating controls, not scope.
    - [ ] Windows-only: owner selection/foreground/CLI producers, hotplug
      journeys, multi-output native evidence on a multi-output Windows setup.
    - User journey: two-output L/R with WS1/WS2 on L; L WS1->WS2, change
      R, focus L, previous twice (local: WS1 then WS2; R unchanged);
      shared mode WS1->WS2->WS3 then previous twice (WS2 then WS3);
      disconnect/reconnect return-scope sequence per R-WS-17.
    - DoD: non-local modes behave per scoped history/ring with undefined
      outcomes as TBD rows; matrix `R-WS-15..17` Ours Windows cells updated
      with dated offline evidence, native TBD.

  - Item 13: Q2 fixed-size float admission R-SPC-04. User accepted the
    ordinary fixed-size admission addition 2026-10-07; exact D1-D8 edges
    are user decisions 2026-10-08 (normative as delivered).
    KDE/shared delivered offline; Windows delivered offline 2026-10-11,
    prerequisite `a41fcb5` plus runtime delivery
    ([Windows record](changes/archive/windows-fixed-size-admission.md)).
    D1 predicate setting and functional naming delivered offline
    ([record](changes/archive/admission-and-move-settings.md)); D5/D6 delivered
    offline ([record](changes/archive/fixed-size-exit-and-enable.md)); D7
    delivered offline ([record](changes/archive/fixed-window-tile-override-restart.md)).
    Windows predicate, D5/D6 and fixed-origin D7 marker wiring are complete
    offline; native/game acceptance pending. Hydrated fixed-override maximize
    routing under both born-maximized values is tentative; R-SPC-13 native TBD.
    [Record](changes/archive/fixed-size-admission.md),
    [decisions](decisions.md#fixed-size-admission),
    [matrix](spec/reference-outcomes/special-windows.md#q2-fixed-size-admission-discriminators-2026-10-08).

    #### Item 13 behavior and reference seams (historical handoff)

    - Normative addition: REQ-SPC-04 floats ordinary fixed-size clients.
      Decided edges (user 2026-10-08/10) REQ-SPC-04a..h: equal usable bounds;
      zero is unset per axis (G-05, item 17), partial-zero tiles under
      both-axes-fixed and floats under either-axis-fixed via its nonzero axis;
      missing/full-zero/sentinel excluded; admission-only hint
      status; same-live-client user tile overrides; fixed-floating base under
      native maximize; born-fullscreen first exit classifies as newly admitted
      (fixed floats with no writes, otherwise tiles; prior fixed floats
      restore); workspace enable checks every window being tiled (fixed stays
      untouched float, others tile; user tile overrides stay tiled);
      startup/restart adoption plus tile-override persistence in the same
      intentional-float store with recompute fallback;
      membership-only automatic admission with zero native target writes.
      Either-axis alternative is now `either-axis-fixed` (`Width or height fixed`),
      default `both-axes-fixed` (`Width and height both fixed`); both use the
      existing hint-validity guards. Breaking pre-release configs accepted, no
      migration. D6 fixed/maximized workspace enable keeps the floating base
      without a slot; explicit user tile overrides still use R-MAX-03's
      reserved slot. Windows borderless-game
      inference stays selected; B9 unmaximize/fresh-admit is separately
      delivered offline on KDE, Windows handoff item 15 pending.
    - References: COSMIC @3d55cba0 `src/shell/layout/mod.rs:46-52`,
      `src/shell/element/surface.rs:565-595`, `src/shell/mod.rs:2957-3022`,
      `src/shell/workspace.rs:1440-1454,1491-1519`. Fullscreen birth/restore
      are distinct (`src/shell/mod.rs:2754-2807,2960-2968`). COSMIC admission
      placement/focus is not permission to touch games; our D8 is no-touch.
    - Shared exact sites in this staged Q2 delivery: `core/size_hints.rs`
      `FixedSizePredicate`, `is_fixed_size_with` / `fixed_size_reason_with`; `core/session/world.rs`
      `converge_observation`, automatic/override sets; transactional
      `PendingDesired` in `core/session.rs` and `ops/lifecycle.rs` /
      `ops/float.rs`; `core/engine.rs` `set_fixed_size_admission` default OFF;
      `core/seed.rs` `EngineWindow` origin/overlay fields. Existing Linux
      Planner in `protocol/planner_protocol.rs` `Planner::new` enables it;
      Windows uses `Engine::new` directly and stays OFF. This is no new
      adapter-logic extraction. Fixtures: `crates/tiler-core/tests/fixed_size_admission.rs`,
      `crates/tiler-protocol/tests/planner_fixed_size.rs`.
    - KDE exact sites: `kwin/src/plan-adapter.ts` `withFixedSizeFloats`,
      `fixedClients` exact-reference records, `commitFixedStage` after applied
      command success, `retileAutomaticFixed`; optional `fixed_auto` and
      `fixed_suppress` wire assertions carry origin/override through domain
      release/adoption. `kwin/src/plan-adapter-entry.ts` `markTiledAndResync`
      resets sighted automatic and ordinary-admission pins only after confirmed release
      so enable checks current hints/predicate; explicit user tile overrides
      survive; current domain sightings protect omitted/moved identities. Native removal
      evicts identity, scoped/minimized absence does not. Real Planner and
      production-entry fixtures: `kwin/tests/fixed-size-admission.test.ts`,
      `kwin/tests/fixed-size-workspace-entry.test.ts`.

    #### Item 13 Windows adapter checklist (historical seams)

    - Max-side data is missing today: `src/tiling_sys.rs:733-817`
      `seed_minmaxinfo`, `query_outer_min_track`, `min_hint_for` extract only
      minimum track; maximum track is seeded at :743-747 but never extracted.
      Extend the same one bounded WM_GETMINMAXINFO query, preserving timeout,
      aggregate budget and failure-as-unknown rules. Normalize both sides
      with the same fresh DPI/frame-inset conversion at
      `src/tiling.rs:99-154` `normalize_min_track` / `min_hints_from_outer`;
      no geometry/style/resizeable inference of fixedness.
    - Wire visible/verified-hidden/retained rows through
      `src/tiling_sys.rs:804` `min_hint_for`, :827 `hidden_hint_for`, :4243
      `assemble_domain_rows`; unknown max hints never classify. Keep overlay
      hint retention lifetime-bound; do not reuse an automatic float's stale
      minimum as evidence for fresh fixed-size admission.
    - Select classification before native slot/restore/write side effects:
      `src/tiling_sys.rs:3656-3668` eligibility, :3785
      `admission_clear_eligible`, :3810 `clear_maximize_at_admission`, :8726
      `admit_slotless_maximized`, :8622 `admit_born_fullscreen`, and
      `src/product_hide.rs:825` `admit_managed_claim`. Fixed/maximized birth
      floats without clear/slot. On the FIRST born-fullscreen exit for the
      same lifetime, query current usable hints and apply the configured
      predicate before maximize-clear/restore/slot side effects: fixed becomes
      an untouched automatic float, non-fixed tiles; prior fixed floats
      restore floating. Later fullscreen toggles retain ordinary admission
      identity. No writes during fullscreen or to automatic fixed floats.
      On workspace enable, check EVERY tile candidate, including arrivals
      while floating and formerly ordinary tiles whose hints changed; reset
      ordinary admission/automatic marks but preserve explicit user tile
      overrides and intentional/sticky rules. Fixed stays/becomes untouched
      float, non-fixed tiles, including under maximize; only explicit tile
      overrides use item 11's reserved-overlay route. Never change the captionless full-monitor
      inference or app-owned fullscreen/preimage rules as part of Q2.
    - Carry live lifetime-bound origin and override through
      `src/tiling.rs:1226` `build_reconcile_event_for_floating`, :1281
      `build_reconcile_event`, `src/workspace_owner.rs:70` `build_send_event`
      and domain row assembly. This delivery's only Windows edits initialize
      `fullscreen`, `sticky`, `fixed_auto`, `fixed_suppress` to false in
      those constructors and two native preview-test literals. Enable
      `Engine::set_fixed_size_admission(true)` only after complete observation,
      origin and no-touch wiring; never infer automatic origin from an
      intentional floating flag or fixed hints alone.
    - Same live client keeps explicit tile/sticky intent through hide/show,
      scope changes and domain release; native lifetime replacement clears
      it. Workspace enable checks every window being tiled (fixed stays
      untouched float, others tile); intentional/sticky stay under existing
      rules, user tile overrides stay tiled. Restart recomputes absent
      authoritative identity with fallback. D7 is delivered on KDE only:
      successfully applied fixed-window tile overrides persist membership in
      the same intentional-float store, no positions. Clear on re-float,
      verified close and complete-inventory prune; retain through hint/predicate
      changes. Missing/degraded/unmatched records recompute fixed to untouched
      float, with bounded outcome logs and no hold/retry. Windows lacks float
      persistence: coordinate with item 8's mechanism before implementing D7;
      this handoff delivers no Windows runtime behavior.
    - Add `CoreSettings.fixed_size_predicate` / `core.fixed_size_predicate`
      within schema version 1: missing defaults to `both-axes-fixed`, alternative
      `either-axis-fixed`; validate using shared `FixedSizePredicate::parse_wire`.
      Unknown values use existing invalid-file/last-good handling. UI labels
      `Width and height both fixed` / `Width or height fixed`; tooltips COSMIC /
      Hyprland (Wayland), sway. Adopt last-good changes with
      `Engine::set_fixed_size_predicate` for subsequent admissions only, without
      resync/reclassification; live user tile overrides still win. Log effective
      values with other settings. KDE `fixedSizePredicate` and protocol
      `fixed_size_predicate` are delivered offline; Windows remains handoff-only.
      No new shortcut, preset or hook; no migration. Automatic membership must
      emit bounded correlated decision evidence and cause no game geometry,
      focus, stacking/keep-above or effects/input interference. Do not call
      the explicit-float placement/focus path for automatic admission.
    - [x] Portable/offline: full hint table, opt-out unchanged behavior, fresh
      foreground/hidden startup, both hint transitions, explicit unfloat /
      sticky overrides through hide/domain adoption, ID reuse, fixed/max
      birth and enable/native restore, born/prior-float fullscreen, first-exit
      hint/predicate changes and repeated toggles, fixed/non-fixed arrivals
      while floating, changed hints/predicate at enable, explicit tile
      overrides vs ordinary admission pins, intentional/sticky controls,
      failure paths and all setter no-write accounting; D7 success-only persist,
      restart tile hydration, re-float/close/complete-prune, hint-loss/enable,
      degraded/legacy/mismatched fallback and non-fixed controls. Both new classification
      points must emit bounded correlated decision diagnostics.
    - [ ] Windows-only: fresh max-track/DPI/inset observations, timeout/hung
      clients, native overlay/restore and fixed borderless/exclusive games;
      user owns live Windows checks. Linux tests prove none of these.
    - [x] Windows offline: predicate default/variants/invalid and subsequent-admission
      tests, setting/schema/UI/live wiring; same-axis uses item 3's renamed
      values, with old IDs rejected and no aliases. User owns live checks.
    - DoD: decided clauses implemented with current Windows
      rules preserved; dated Ours Windows matrix/spec evidence, no native
      claim without user testing; user decisions 2026-10-08 approved, changed
      D1/D5/D6/D7 Windows wiring delivered offline; native checks above pending.
      Current evidence: MSVC locked four-package build/test, format, strict
      clippy and diff checks pass; 325 Windows library tests. Owned message-only
      marker readback/hydration covers retained-maximized hint loss, both born
      modes and identity failures; this is not an ordinary-app/game journey.

  - Item 14: Q4 whole-active-workspace output migration R-WS-12. The addition
    was accepted 2026-10-07; D1-D9 detail is user decisions 2026-10-08
    (normative as delivered). Core/Linux planner/KDE delivered offline;
    Windows behavior remains unchanged. D8 carry delivered offline in core/KDE
    2026-10-09; Windows wiring pending. Depends on the multi-output
    foundation (item 5), scoped history (item 1), and item 12 for non-local
    modes. [Record](changes/archive/kde-whole-workspace-output-migration.md),
    [spec](spec/functional-spec.md#workspaces) REQ-WS-12/12a..i,
    [matrix](spec/reference-outcomes/workspaces.md#r-ws-21-migrate-mode-capability).

    #### Item 14 behavior and shared contract

    - Four directional active-only, follow-only actions, all UNBOUND by default.
      Resolve FULL-output-rectangle reciprocal adjacency, no wrap; choose
      largest shared edge, then left/top (User decision 2026-10-09; selection
      delivered offline in core/KDE, Windows item 16 pending the item 5 foundation).
      Hidden R-WS-12 baseline has no counterpart; explicitly select its active
      leg. KDE local/global-unique requires strict native per-output desktops;
      shared/false/unreadable refuses. Windows has no KWin option: establish
      separable per-monitor views through its existing project workspace owner,
      not a guessed registry policy or an emulated native capability flag.
    - Preserve backing ID, tree/order/shares/remembered focus/tiling mode;
      insert after target current and show migrated, prior target stays hidden.
      Source selects last remaining scoped entry, existing minimum-two/trailing
      spare converges; empty allowed. Carry workspace floats with class/origin,
      sticky stays source. Carry fullscreen plus maximized members (changed D8,
      user 2026-10-08, core/KDE delivered offline): no refusal in moved members or
      affected views; native move only with no extra fullscreen
      size/position/focus writes; an explicit user move is not unwanted
      interference. Preserve maximized overlay/reserved slot, no unmaximize;
      native re-fit owns overlay geometry on the target.
    - Shared `core/boundary.rs` `CoreCommand::MigrateWorkspace { direction }`
      and `CoreReply::MigrateWorkspace(MigrateWorkspacePlan)`; event target has
      the SAME workspace ID on a DIFFERENT output and empty `target_windows`.
      Destination's DIFFERENT current workspace remains adapter-owned, including
      its currency fences (overlays are not a refusal). `core/engine.rs` `migrate_workspace_request` rekeys via
      `core/session/world.rs:251` `relocate_domain`, not per-window re-admission.
      Reply geometry/focus/active_window/member counts and postconditions mean
      retained `planned`, never native completion. Protocol `migrate-workspace`
      is codec-only; Windows calls the public Engine types directly.
    - Follow moved active client only after every member/output/membership and
      both views are verified; empty/sticky-active uses native output switching
      without invented focus, minimized clients stay minimized. Fullscreen
      follow is native-only, with no focus setter or unverified focus claim.
      Verify overlay arrival by identity/membership/output, not its old frame;
      re-read live overlay flags before geometry/focus writes and suppress them
      while overlaid/fullscreen respectively. Keep lifetime/currency fences and
      reconcile mode changes; do not replay or skip a carried member.
      History 1.3/1.5
      invalidates out-of-scope previous IDs; remove ONLY moved ID from hotplug
      return associations. Partial/uncertain native writes reconcile observed
      domains without replay, guessed rollback or false completion.

    #### Item 14 exact Windows adapter sites

    - `src/workspace.rs:95` `ManagedWorkspaces` currently stores output order,
      active index, membership, displaced origins and tiled state. Add one
      whole-workspace reassignment primitive, preserving those stable IDs and
      state; use `workspace_members` :659 / `workspace_ids` :673 for exact
      membership/order, and `active_id` :236 / `activate` :244 for source/target
      view transitions. `displace_output_to` :561 / `reconnect_output` :603
      must forget only the explicitly moved ID. Do not simulate this by N sends
      or move the destination workspace back to source.
    - Add a migrate event builder beside `src/workspace_owner.rs:70`
      `build_send_event`; permit empty active client, carry every workspace
      member's overlay/sticky/class/origin flags and target-scope currency.
      `stamp_send_target` :148 is SendToWorkspace-specific; do not reuse its
      command. Extend `planned_writes` :406 and native
      `src/tiling_sys.rs:4211` `desired_entries` for the new typed reply.
      `verify_membership_transfer` :164 proves ONE mover only: migration must
      prove the complete set plus retained mapping and both current views.
    - Add a distinct route beside `src/tiling_sys.rs:10552` `workspace_do_send`
      (event construction :10848, Engine handle :10865, membership proof :10952)
      and :10314 `workspace_do_send_native`; their eligibility and :11017
      `workspace_send_follow` are window-send/same-output tails, not a whole
      migration implementation. Resolve full rectangles via `all_monitors`
      :264 / `monitor_fulls` :282, bounds via `workspace_domain_for` :8540.
      Fence all windows/related natives, source/target current views, mode,
      lifetime and overlay state before the first and every later setter;
      overlay state suppresses geometry/fullscreen focus, never a carry refusal. Verify
      arrivals before geometry/view/focus completion. Protect hidden/minimized
      clients and existing product-hide/recovery authority through partial writes.
    - Compile-only edits in this delivery: `src/tiling.rs:1263/:1315`,
      `src/workspace_owner.rs:98/:115`, and native preview-test constructors
      `src/tiling_sys.rs:16719/:16927` initialize `EngineWindow.maximized` to
      false. These are NOT fresh overlay observations. Replace placeholders
      only when the Windows adapter supplies actual overlay observations;
      retain existing fullscreen inference and game/preimage policies.
    - Proposed Windows IDs `migrate-workspace-left/right/up/down`, defaults
      empty, follow-only, no new holder claim. Add catalog family/routing in
      `src/settings.rs:475` `BindingFamily` / :757 `binding_catalog`;
      `src/snapkey.rs:427` `WorkspaceOp` / :445 `WorkspaceIntent` currently
      encode digit select/send, not migration. Reuse items 1/2's explicit ACTION
      rebinding and full-modifier queue/hold pairing, never a canonical empty VK.
      Both presets retain unbound defaults; dynamic settings rows remain
      rebindable with no new tray command. Actual rebound chord conflict policy
      follows existing validation, not a fabricated default conflict.
    - KDE mirrors: `kwin/src/plan-adapter-entry.ts` `requestWorkspaceMigrate` /
      `observeMigrateWorkspace`, `kwin/src/workspace-native.ts`
      `commitWorkspaceMigration`, `kwin/src/workspace-send-adapter.ts`
      `requestMigrateWorkspace` / `progressMigrate` and per-setter fences.
      Port `kwin/tests/workspace-migrate.test.ts` and
      `workspace-migrate-engine-fixture.test.ts` invariants, not duplicated
      adapter architecture; no core extraction is selected.
    - [ ] Portable: retained unequal-share/nested tree, floats/origins, sticky
      active and minimized/null focus, empty lifecycle, insert-after-current,
      scoped modes/history/return siblings, all-direction adjacency,
      fullscreen+maximized carry with no extra overlay writes, stale
      revision/ref replacement/removal/partial-arrival
      recovery; actual Engine reply extraction and unbound rebind/preset tests.
    - [ ] Windows-only: whole-member physical output transfer, mixed DPI/work
      areas, native arrival/view/focus timing, hidden/minimized/transient lifetime
      and carried-overlay native-move-only writes, owned-window recovery after partial
      writes. User owns live journeys on a two-output setup.
    - User journey/DoD: select WS2 `H[A,V[B*,C]]` on L, R shows occupied WS3;
      bind migration right, verify same WS2/tree/shares, WS3 hidden, L last
      remaining, B focus only after arrival. Repeat empty, intentional/automatic
      float plus sticky, fullscreen+maximized carry (Windows D8 pending) and
      displaced-origin reconnect.
      Update Ours Windows/spec with dated offline evidence; native outcomes TBD
      until user-tested. User decisions 2026-10-08 approved; D8 carry
      core/KDE delivered offline, Windows adapter pending.

  - Item 15: B9 / M09=A maximized intentional unfloat R-FLT-06 (User
    2026-10-08). Shared-core/KDE delivered offline; Windows delivered offline
    2026-10-12 ([Windows record](changes/archive/windows-maximized-intentional-unfloat.md)).
    Native MSVC gates pass; scripted native effects plus real Engine cover
    clear-before-admission, restored frame, narrow refusal/later-press recovery,
    post-clear lifetime/focus/domain/fullscreen races and fixed override. Native
    ordinary clear/frame/focus agent-observed 2026-10-12 in scoped Notepad/Paint;
    physical B9, fixed marker/restart, failure/race/game/DPI remain pending. Checklist below
    records the delivered contract and remaining native acceptance.
    [Record](changes/archive/maximized-intentional-unfloat.md),
    [spec](spec/functional-spec.md#7-floating-r-flt),
    [row](spec/reference-outcomes/floating.md#r-flt-06-float-toggle-over-a-maximized-window).
    - Replace `src/tiling.rs` `float_toggle_refusal` maximize refusal ONLY
      for an ordinary currently floating target being explicitly tiled.
      Preserve normal-to-float overlay refusals, fullscreen handling,
      sticky gates, native identity and verified effect/settlement fences.
    - One native unmaximize attempt per explicit activation; observe the
      same live client unmaximized before Engine fresh admission or geometry
      writes. Use the restored frame and ordinary new-window rule, never an
      old slot. Reuse existing native observation/settlement patterns, no timers.
    - Refused/unobserved/raced clear: log cause, retain float membership and
      usable preimages, end only that operation, keep other functionality and
      later explicit presses available. No automatic clear retry or fighting.
    - Fixed automatic or intentional floats use the existing D3/D7 explicit
      user tile override on successful admission (items 8/13); no automatic
      re-float. Q3 born-max/workspace-retile overlay retention stays item 11.
    - Acceptance: clear-before-geometry real Engine/native-setter regression,
      ordinary nonmax unfloat unchanged, refusal/unobserved clear with no
      stuck state and later-press recovery, exact-ref/focus/fullscreen races,
      fixed override after subsequent observation/restart. Update Windows
      REQ-FLT-06/06b and Ours cell with offline evidence; native journey TBD.

  - Item 16: position-based output selection (User 2026-10-09). Shared core/KDE
    delivered offline; Windows runtime unchanged, multi-output unparked and
    scheduled after the single-output handoff queue (user 2026-10-10).
    Implement with items 5/14 when their multi-output foundation is resumed.
    [Record](changes/archive/position-based-output-selection.md),
    [spec](spec/functional-spec.md#move) REQ-MOV-08/08b,
    [output](spec/functional-spec.md#output) REQ-OUT-01/04,
    [workspaces](spec/functional-spec.md#workspaces) REQ-WS-12/12c.
    - Reuse `tiler_core::output_selection` (`OutputEntry`, `Candidate`,
      `candidates_in_direction`, `select_for_window`, `select_for_migration`).
      Supply complete, readable FULL monitor topology with unique stable IDs
      and a matching source entry. `Err(UnreadableTopology)` refuses;
      `Ok(None)` is a no-op. No candidate skipping, output wrap or origin-distance
      fallback. Existing item-5 monitor/event sites remain the wiring seams.
    - Exhausted moves and explicit output sends use the moving window's native
      frame in the same coordinates as FULL monitor rectangles. Left/Right
      project y, Up/Down x; half-open shared intervals [start,end) and doubled
      centres preserve odd-extent halves. A containing edge wins; multiple
      containing edges tie directly by left/top. Only if none contains the
      centre, maximize window-span overlap; final x, then y, then stable ID.
      Migration uses largest shared-edge length, then the same tie-break,
      never the focused window. Deliberate COSMIC origin-distance deviation.
    - Keep full rectangles for selection only; domain bounds/placement stay
      work-area based. Present the selected reciprocal pair to the Engine;
      reverse multiplicity must not refuse. Local move rules still win,
      R4 lands nearest the source, send uses ordinary admission and follow/stay.
      Migration carries workspace floats, keeps sticky windows on source.
    - Pin both dispatch source and chosen target through every native transfer,
      membership, geometry, arrival and follow/stay check. Never re-rank after
      relocating the mover; it now has a target frame. Re-read topology and
      verify the pinned candidate still touches, retaining current-view,
      domain/gap/mode/member/lifetime fences and partial-transfer reconciliation.
      KDE mirrors: `observeDirectionalDomain` and `r4RawFresh` carry
      `pinnedSource.targetOutput`; `requestOutputSend` ranks by frame;
      `observeMigrateWorkspace` ranks by shared-edge length. Focus navigation
      retains its existing policy.
    - [ ] Portable: all four directions/two candidates, centre precedence,
      seam/odd extent, overlap fallback, left/top/identity ties, multiple
      containing edges, reverse multiplicity, unreadable other output/source
      mismatch, no candidate, migration largest-edge/equal-edge and no window
      dependency. Core tests: `position_based_output_selection.rs`.
    - [ ] Adapter/real Engine: non-left/top move and send, non-top largest-edge
      migration, FULL-vs-work-area placement, exact native arrival/follow/stay,
      delayed arrival and chosen-target removal/drift mid-flight. Port the
      `output-send-engine-fixture.test.ts`, `workspace-migrate-engine-fixture.test.ts`
      and `plan-directional.test.ts` discriminators to Windows's existing harness.
      Linux portable tests prove no Windows native behavior. User owns live checks
      on a multi-output Windows setup; update Windows matrix/spec only with actual evidence.

  - Item 17: G-05 per-axis-zero fixed-size predicate (User 2026-10-10,
    REQ-SPC-04a). Coordinate with item 13: zero means unset on each axis;
    equal partial-zero hints (640,0) and (0,480) are not fixed under
    `both-axes-fixed`, but their genuinely fixed nonzero axis makes them fixed
    under `either-axis-fixed`. Keep full-zero and unbounded-sentinel guards.
    Shared-core predicate + KDE delivered offline 2026-10-10; Windows delivered
    offline 2026-10-11 with item 13's max-track/schema/UI/Engine opt-in
    ([record](changes/archive/windows-per-axis-zero-predicate.md)).
    Exact seams: `core/size_hints.rs` `fixed_size_reason_with` /
    `is_fixed_size_with` now require equal nonzero bounds per axis;
    `kwin/src/plan-adapter.ts` `isFixedSize` mirrors it. No signature/wire
    change or Windows predicate copy. Reuse item 13's bounded hint observation
    and `Engine::set_fixed_size_admission` opt-in plus
    `Engine::set_fixed_size_predicate`; do not rebuild retained classifications.
    Port `fixed_size_admission.rs` partial-zero/admissions-only cases and
    `kwin/tests/fixed-size-admission.test.ts` real-Planner discriminators.
    - [x] Offline acceptance: positive fixed, both partial-zero orientations, full-zero,
      missing, sentinel and subsequent-admission/no-reclassification controls.
    - [ ] Native hint representation and game no-touch checks remain pending.

  - Item 18: G-06 maximized directional focus/move (User 2026-10-10,
    REQ-MAX-08 NORMATIVE; resolves G-D1). Both platforms follow COSMIC:
    directional focus no-ops while the focused window is maximized (leave via
    unmaximize or Alt+Tab); directional move unmaximizes first, then moves.
    Replace Windows focus-permit/movement-refusal, retaining pointer refusals
    and fullscreen policy. Shared opt-in focus fence + KDE delivered offline
    2026-10-10; Windows delivered offline 2026-10-12 from base `e506350`
    ([record](changes/archive/windows-maximized-directional-navigation.md)). Native
    MSVC gates pass; pre/post-clear authorization, fullscreen precedence and
    coordinated float targets covered offline. User-accepted physically
    2026-10-12 under item 10's conditions: maximized tiled focus fence and
    clear then tiled move/float all four halves. Failure/race/held-repeat/preset,
    fixed-size, game/DPI/multi-output pending.
    Windows keeps the local fresh-flag guard because reconcile omits maximize;
    shared Engine still owns ordinary tiled move. Historical seams:
    - `core/engine.rs` `set_maximized_focus_fence` propagates to retained/fresh
      sessions; default OFF preserves Windows. `core/session/ops/focus.rs`
      fences local/cross-output focus for a maximized non-fullscreen origin.
      Enable only with fresh observed flags; KDE wire still omits maximize
      for born-maximized admission and uses its local fence instead.
    - `src/tiling_sys.rs` directional origin overlay gate near `SnapOp::Move`
      / `overlay_refusal_for` (currently :5980-6000) must fence focus from
      the maximized origin and replace move-refusal with one nonactivating
      `restore_zoom_placement` attempt (:3303). Dispatch only after observed
      clear and fresh same-lifetime/focus/domain checks; async `dispatched`
      is not observed completion. Keep fullscreen/gaming/suspension fences.
      `build_reconcile_event_for_floating` / command assignment (:6082-6111)
      currently omit overlay flags; opt-in alone does not replace this guard.
    - Keep focus TO a maximized target valid from an ordinary origin; do not
      blanket-reject `revalidate`'s `allow_maximized` focus path (:3249).
      Coordinate ordinary float half-snaps with item 10's pending routing.
    - KDE reference: `PlanAdapter.requestFocus`, `requestFloatFocus`,
      `requestMove` and `dispatchMove`; one clear, exact-reference echo,
      fresh directional reread, no delayed intent/retry. Port
      `maximized_focus_send.rs`, `plan-maximized-focus-move.test.ts` and
      `g06-g37-entry-discrimination.test.ts` (real retained Engine).
    Acceptance: focused-max focus in all
    directions; fresh isolated maximized mover clears before ordinary move;
    ordinary navigation and pointer/fullscreen controls unchanged. Native
    focus/state/geometry timing checks pending on both platforms.

  - Item 19: G-37 Source workspace after migration (User 2026-10-10,
    REQ-WS-12d/e). Coordinate with items 1/14: Windows
    `core.migration_source_refill`, KDE `migrationSourceRefill` + KCM;
    `last-remaining-workspace` default (`Last remaining workspace`, tooltip
    COSMIC) / `most-recently-used-workspace` (`Most recently used workspace`,
    tooltip bspwm, i3, awesome). Additive schema-v1 field, missing=default,
    no migration; live changes affect subsequent migrations. MRU uses item
    1.2 per-output previous stable ID snapshotted before map mutation; it is
    eligible only while live in the remaining source-output scoped ring,
    excluding the migrated ID. Surviving empties qualify; missing, removed,
    migrated or out-of-scope entries fall back to last remaining. Preserve
    a still-scoped live source current view in both modes. Keep scoped
    invalidation, destination insertion and empty migration unchanged.
    Shared-core/KDE setting and MRU delivered offline 2026-10-10; Windows config,
    settings UI and runtime wiring pending with the multi-output
    foundation. Exact seams:
    - `core/output_selection.rs` `MigrationSourceRefill::{LastRemaining,
      MostRecentlyUsed}`, `parse_wire` / `as_wire_str` and pure
      `select_migration_source_refill(mode, previous, remaining_scope)`;
      caller owns live scope/history invalidation, selector creates nothing.
    - KDE `kwin/src/migration-source-refill.ts` mirrors selection;
      `WorkspaceNativeAdapter.commitWorkspaceMigration` snapshots history
      before rekey and chooses source refill after scoped invalidation;
      entry `configChanged` calls `setMigrationSourceRefill` for live reread.
      KCM `unifiedsettings_module.cpp` / `.ui` owns the functional labels,
      WM tooltips, defaults and typed reconfigure, application unconfirmed.
    - Windows `src/settings.rs` `CoreSettings` / `Default` /
      `validate_settings`: ADD the version-1 string field and exact-token
      validation (not delivered here); `src/settings_ui.rs` needs control
      and Apply/Revert. Adopt through `apply_live_settings` into item 14's
      future migration route using item 1's per-output observed history.
      No Windows source/schema changes in this delivery.
    - Port core selector cases, `migration-source-refill.test.ts` and
      production-entry real-Engine `g06-g37-entry-discrimination.test.ts`.
    Acceptance: MRU differs from last-remaining, no eligible
    entry fallback, migrated/removed entry exclusion, output-local history,
    default/invalid/live settings, retained moved workspace state; native
    journeys pending on both platforms.

  - Item 20: G-D2 maximized workspace-send carry (User 2026-10-10,
    REQ-MAX-09 maximize leg).
    Windows same-output numbered/relative follow/stay and native unmaximize
    agent-observed 2026-10-11 at base `553c65b` + item-2 delivery commit
    ([record](changes/archive/windows-workspace-send-follow-stay.md)); native
    gates passed. Physical/flag-race and cross-output legs remain pending.
    Keep maximize on arrival: Windows existing
    policy, cross-platform target; KDE carry delivered offline 2026-10-10.
    Deliberate COSMIC/niri
    deviation: a send relocates the whole window/state, unlike in-layout move.
    Verify Windows retained-overlay send/follow and send-and-stay through
    ordinary send integration; do not replace it with item 18's unmax-move.
    Exact seams: `src/tiling_sys.rs` `workspace_do_send` retains the
    pre-dispatch `is_zoomed_now` check and post-plan `send_flags_stable`
    overlay check before `workspaces.assign` (:10718-10737/:10885-10945).
    Keep source/target allocations and geometry overlay skip; item 2 adds
    follow/stay without restoring the mover. Native-boundary sends use
    `workspace_do_send_native`. KDE reference:
    `observeSendTarget` / `observeOutputSendTarget` and same-output
    `workspace-send-adapter-entry.ts` observer admit maximize/fullscreen while
    refusing float/sticky/independent fit exclusions;
    `WorkspaceSendAdapter.crossOutputMoverLive` / `apply` require flag-stable
    arrival and skip overlay geometry. Port
    `workspace-send-maximized-carry.test.ts` follow/stay and `output-send.test.ts`
    cross-output carry/flag races. Actual KDE native maximize carry is pending
    observation; no restore/remaximize workaround was inferred offline.
    Fullscreen stays item 9/Table A 2026-10-07; KDE D28 carry is delivered
    offline and whole-workspace D8 carry remains distinct. Acceptance: maximize on
    arrival, source reflow/target slot and follow/stay, native unmaximize on
    target; live checks pending, fullscreen controls unchanged.

  - Item 21: workspace tiling toggle default Win+Y (user 2026-10-10), matching
    COSMIC `ToggleTiling` Super+Y (`cosmic-comp/data/keybindings.ron:85`,
    pinned `3d55cba0`). Wire the Windows catalog and existing Authentic/
    Compatible conflict model; Win+Y is recorded as a Mixed Reality conflict.
    Other currently unbound actions stay unbound. Acceptance: shortcut and
    tray checkbox toggle the same per-workspace state; Apply/Revert, presets
    and takeover-off release work; update shortcut records with dated offline
    evidence, native checks pending. KDE Meta+Y delivered offline 2026-10-11
    ([record](changes/archive/kde-workspace-tiling-shortcut.md)); user-owned
    physical/restart/preset checks remain below.

  - Item 22: 0.1 triage Windows follow-ups (updated User 2026-10-13).
    D01, D03, D09, D10, D12, D13, D16, D22, D23, D25, D26, D27, D28 are approved;
    no triage units remain tentative.
    Shared core delivery serves Windows; Windows adapter/native work follows
    the shared contract. D01's fullscreen focus fence is delivered in KDE
    with shared verification; Windows has its own foreground rules. Consider native
    approval/denial and explicit switching in the Windows handoff, without
    importing KWin stacking or asserting a Windows fence gap. D03/D09/D10/
    D12/D13 use existing shared defaults, with native verification pending.
    D22 survivor-share preservation is verification-only under shared removal,
    no setting; Windows native float-out reflow remains pending.
    Approved D16 ordinary/combo shared delivery is complete: intentional floats
    transfer through `MoveDeferred`, retaining float class/size without either
    tree changing; live fullscreen/maximize travels natively. KDE is delivered
    offline; Windows adapter/native acceptance remains
    ([record](changes/archive/d16-floating-transfer.md)). No setting. Same-output
    exact frame/no geometry write confirmed User 2026-10-13. Windows adapter steps:
    (1) In `src/tiling_sys.rs` `workspace_do_send`, narrow the current
    `send-refused-floating` gate to admit ordinary intentional floats and
    floating+fullscreen/maximized combos on tiled-to-tiled sends. Keep
    `send-refused-sticky` and automatic-fixed refusal using existing
    `engine_is_automatic_fixed` / fixed-origin state. The floating-source
    `workspace_do_send_native` boundary route already carries native float frames;
    it does not deliver the refused tiled-to-tiled deferred shapes.
    (2) Preserve floating state from `assemble_domain_rows` / `domain_rows` /
    `build_send_event`; bind the focused float mover's exact live overlay flags
    from `overlay_now` through `stamp_engine_event`. Existing `build_send_event`
    defaults `maximized` false and `stamp_fixed_admission_flags` only stamps
    fullscreen/fixed flags: floating+maximized needs a send-specific flag path,
    without changing tiled/survivor/admission encoding or protocol semantics.
    (3) Explicitly handle `SendWorkspace(plan)` with `plan.operation` equal to
    `MoveDeferred`: no fabricated leaf, no mover geometry, no source/target
    geometry application (including `workspace_send_stay`'s source-only tail),
    and no maximize restore/fullscreen toggles. Keep `workspaces.assign`,
    `verify_membership_transfer`, `send_fences_hold` / `send_overlay_gate`,
    identity/scope/lifetime/hide/reveal checks and existing follow/stay routing.
    (4) Retain core float geometry, adapter `float_rects` / `floated` intent and
    native overlay restoration state across membership movement. Destination
    exit returns to the floating frame, never tiled admission; restart float
    memory follows the window identity. Keep destination tiles and slotless
    source untouched.
    Acceptance: both combos x follow/stay x numbered/relative, ordinary/combo
    WS1 -> WS2 -> WS1 roundtrip, exact arrival flags/frame, zero geometry/overlay
    writes, destination exit still floating, sticky/automatic-fixed refusals,
    sole-window source and empty target. Extend `workspace_owner.rs` send-event,
    retained-overlay, writable-subset and sole-stay seams; native acceptance
    remains pending. No Windows source behavior was changed by shared/KDE delivery.
    Approved D23 shared rework delivered offline 2026-10-13
    ([evidence](changes/archive/d23-rework.md)); Windows adapter pending: replace retained
    Windows minimize allocation (divergence, not an approved exception) with
    release/reflow/MRU and session-only best-effort stored-slot rejoin. COSMIC
    `3d55cba0` source gate passed. Current retained-allocation sites:
    `crates/tiler-windows/src/tiling_sys.rs` `observe_window` (iconic returns
    `ObserveFailure::Known(.., SkipReason::Minimized)` with no frame),
    `TileLoop::observe` (that arm pushes a frameless `RetainedRow { rect: None,
    .. }`), `assemble_domain_rows` (the retained-occupancy lane rides the
    `member_rects` last-known rect as a tiled `floating: false` `MemberView`),
    `crates/tiler-windows/src/workspace_owner.rs` `domain_rows` (keeps the
    observation complete so Engine membership survives), and
    `workspace_close_cleanup` (retained members stay enumerated; only absent
    HWNDs drop). Adapter steps: (1) drop the minimized window from the carried
    Engine observation on minimize so `Session::converge_observation` releases
    its leaf via `remove_leaf_from_tree`, reflows survivors, resolves focus via
    retained/MRU-or-none, and stores the `MinimizedSlot` (homing domain + leaf
    id + restore context); (2) on native restore, re-carry it purely tiled for
    best-effort rejoin beside a former neighbor, same side/group/index and
    comparable old share. Shared best-effort rejoin is delivered; exact equality
    is only a fast path, not a validity prerequisite. Resize/new-window/
    other-sibling-close/swap do not invalidate alone. Fresh admission only if
    no former neighbors remain, domain gone or sent elsewhere; (3) never write focus on
    restore - the core keeps pre-restore MRU focus (the converge focus arm skips
    `observed_focus` when it names a just-restored window), leaving host restore
    focus native; (4) sole-minimize keeps the workspace via
    `Engine::committed_session_is_empty` honoring `has_live_minimized_slot`,
    with focus none; (5) native reveal stays in
    `crates/tiler-windows/src/model.rs` the `show.minimized` lane
    (nonactivating restore), unchanged; (6) report minimized-window close via
    additive `CoreEvent.closed_windows` on the valid domain observation, using
    verified closed HWND lifetime ids (never minimize or transient absence).
    Engine forgets slots after successful convergence and before retirement;
    absent/empty ids preserve existing behavior. Retain undelivered reports
    until planner acceptance; include hidden workspaces and multiple-domain
    close bursts, and do not fabricate empty membership on unreadable native
    evidence. Native cleanup routes include `workspace_close_cleanup`; keep
    close-vs-minimize classification at observation/lifetime boundaries.
    Shared contract: `MinimizedSlot` docs
    plus `minimized_slot`/`minimized_count`/`has_live_minimized_slot` accessors
    (`crates/tiler-core/src/session/world.rs`),
    `Session::forget_minimized_slot` / `Engine::forget_minimized_slot`, slot-aware
    `committed_session_is_empty` and the disjoint-reseed guard treating a stored
    slot as known (`crates/tiler-core/src/engine.rs`). Test hooks: portable
    Engine minimize/restore convergence tests run on Linux
    (`crates/tiler-core/tests/minimize_restore.rs`); `cfg(windows)` owner wiring
    (`observe`/`assemble_domain_rows`/`domain_rows` carry) needs Windows CI plus
    user-owned native minimize/restore journeys (tile, minimize, sibling reflow,
    best-effort restore, sole-minimize empty workspace, close-slot cleanup).
    Portable regressions pass for focus change, resize, new window, sibling
    close, swap, other minimize/restore, gaps/resolution, restart, closed-slot
    cleanup and fallback; reproduce those at the Windows adapter boundary.
    No setting. Approved D27
    needs supported activation-request/urgency observation, default Mark urgent
    only (no workspace/focus change) and a fullscreen/game-focused urgency-only
    guard in both modes; explicit Alt+Tab/taskbar selection is separate.
    KDE's native-activation exception and no-guard decision are host-specific,
    not Windows evidence; Windows default/guard unchanged.
    Approved D25 (a) keeps host workspace set, re-observes membership without
    moves, restores focus only where provable, rebuilds layout via re-adoption
    and keeps intentional floats via existing restart store; no setting.
    Approved D26 (b) restores persisted owned gaps/settings and re-observes host
    set/membership/maximize. Intentional-float memory is lost on logout/login
    (reopened apps new identities), a listed 0.1 known limitation; revisit
    cross-login memory after 0.1 with shared restart store. Native dogfood/live
    checks will assess both with item 8; membership/set/focus acceptance remains
    pending, no saved-layout restore or new cross-login identity recovery.
    D28 (b) selects KDE fullscreen carry for 0.1, matching item 9 Windows
    carry; preserve existing Windows delivery evidence and pending native/game
    legs. No new Windows carry behavior is selected by this KDE decision.
    Post-0.1 schema/UI/live wiring follows P2 `0.1 triage settings follow-up`:
    approved D09/D10/D27 two-option settings, including D27's default Mark urgent
    only / Switch and focus. See [current settings scope](decisions.md#settings-tray-and-first-run).
    No Windows behavior is claimed inspected or delivered by this docs change.

  - Item 23: born-maximized shared policy (User 2026-10-13, amended option C).
    Core policy is available in `tiler_core::born_maximized::BornMaximized`.
    Replace Windows `settings.rs`'s default/token validation and
    `CoreSettings::born_maximized_keep_overlay`, plus
    `tiling.rs`'s `born_maximized_keep_overlay` token comparison, with core
    parsing/default handling and `should_clear_at_admission`'s clear/keep
    choice. Preserve strict invalid-file refusal/last-good adoption: use
    `parse_wire` for validation, not fallback normalization to accept invalid
    saved values. Keep schema-v1 `core.born_maximized`, labels/tooltips,
    lifetime-bound subsequent-admission adoption, native identity/eligibility
    fences, fullscreen/fixed-size gates, one-shot attempts and independent
    born/retile latches unchanged. Acceptance: consume the same
    `test-fixtures/born-maximized.json` table in Windows adapter tests, then
    pass existing setting/latch/clear/retile regressions and native Windows
    compile/tests. No Windows rewire is included in the core delivery. KDE
    keeps its conformance-pinned copy until post-0.1 Push code down
    ([record](changes/archive/born-maximized-shared-policy.md)).
  - Item 24: underlay default (User 2026-10-13), pending Windows implementation.
    Match the delivered KDE kcfg default `#662a82da` (ARGB: alpha 0x66 = 40%,
    RGB #2a82da), for now. Set `DEFAULT_COLOR_ARGB` in
    `crates/tiler-windows/src/group_underlay.rs` to `(0x66, 0x2a, 0x82, 0xda)`
    and `DEFAULT_UNDERLAY_COLOR_HEX` in `crates/tiler-windows/src/settings.rs`
    to `#662a82da`. Align CLI help in `crates/tiler-windows/src/main.rs`,
    default/parity and raster assertions in the underlay/settings modules and
    `crates/tiler-windows/tests/tiling.rs`, and the default-color trace gate in
    `scripts/windows-group-underlay.ps1`. Preserve explicit saved/CLI colors;
    no migration or alpha/blend changes. Acceptance: native Windows build/tests,
    missing-value/default and explicit-value round trips; user-owned live visual
    check on black, white and grey wallpapers at 100%/150% before 0.1.

  ### Source discrepancies to preserve and report

  | Existing assertion | Current source / implementation gap | Handoff treatment |
  | --- | --- | --- |
  | [R-OUT-01](spec/reference-outcomes/multi-output.md#r-out-01-scrolling-assessment-move-left-onto-an-occupied-output) Ours Windows claimed exhausted-horizontal R4; [R-MOV-05](spec/reference-outcomes/move.md#r-mov-05-backfill-edge-move-with-no-left-neighbor-scrolling) claimed horizontal crossing with Up/Down excluded. | `src/tiling_sys.rs:6099/6105` sets `cross_output_transfer:false` in a single-domain event; no Windows cross actuation. Shared core supports four-direction R4. | Corrected both cells 2026-10-08 at `db31234` to local-only current-code facts. Handoff item 5 is sequenced after the single-output queue (18 before 10/11); no native acceptance claimed. |
  | [R-WS-10](spec/reference-outcomes/workspaces.md#r-ws-10-send-b-away-empty-middle-retained-vs-removed) and [R-WS-16](spec/reference-outcomes/workspaces.md#r-ws-16-previous-after-the-visited-empty-workspace-is-removed) Ours Windows said no removal path; spec REQ-WS-10 repeated it. | `src/workspace.rs:508` removes stable IDs; `src/tiling_sys.rs:9228-9234` retirement cleanup and :10074-10080 selection invoke it. `core/workspace.rs:35` plans eligible invisible-empty removal, protecting the trailing spare and minimum count. | Corrected both cells and REQ-WS-10 2026-10-08 at `db31234`. Exact native scenarios remain TBD; handoff item 1 invalidates actually removed IDs and tests surviving empties separately. |
  | Existing higher-level item-2 summary says Compatible disables arrows, while decision 2.1 identifies stock holders only on KDE and says Windows ownership unknown. | `src/settings.rs:954-957` currently returns None for all Ctrl/Alt chords; no Windows holder evidence for new send/output arms. Verified 2026-10-08 at `db31234`; status outstanding, no new policy selected. | KDE arrows are disabled by KDE Compatible. Windows unknown new arms keep defaults with unknown-ownership text until evidenced policy is recorded; do not manufacture Windows holder claims. |
  | Current settings/rebind helpers assume every implemented action has a canonical default chord. | `src/settings.rs:1044` canonical helper returns None for empty defaults; `build_remap` :1182 skips that action; `validate_bindings` :1405 rejects it. Keep-empty is currently active/effective (:1082); UI `refresh_info` (`src/settings_ui.rs:248`) labels it disabled. Verified 2026-10-08 at `db31234`; status outstanding, plus `settings_ui.rs:286` resize note. | Items 2/5 must wire unbound ACTION targets explicitly, make Keep-empty non-intercepting and display unbound distinctly from Disabled. These are adapter limitations, not a decision reversal. |

- P1 | Shortcut conflict model on KDE and macOS | Per-binding conflict list
  plus compatible/authentic presets (user 2026-10-03); KDE builds on its
  existing shortcut override Apply/Force/Revert; macOS when it starts.
  KDE catalog/presets and Keep-preserving Apply delivered offline (M13,
  2026-10-09); live acceptance is in Pending live checks below.
  Integrated rebind and KDE first-run prompt deferred. KDE live
  acceptance pending: [checks](live-shortcut-override-verification.md),
  [record](changes/kde-shortcut-conflicts.md).
- P1 | Cross-platform functional specification | After the Windows tiling
  dogfood slice, define window/workspace behavior and keyboard shortcuts as
  the selected behavior contract for Linux, Windows and macOS, except recorded
  exceptions. COSMIC is the reference-default yardstick; macOS modifier
  mapping is decided when macOS starts.
  [Reference outcomes](spec/reference-outcomes.md) use per-area Given/When/Then
  scenarios, twelve pinned reference profiles and separate Ours KDE/Windows
  outcomes. Source evidence is not a product decision; AGENTS.md requires a
  discriminating row per ambiguity. All twelve remaining source-fill passes
  completed (`f04511c`..`bfa486b`): zero not-attempted/unsupported-pending
  candidates remain; residual host, fixture and live limits are in the
  [archived ledger](changes/archive/reference-source-fill-remaining.md).
  [Functional spec](spec/functional-spec.md) remains single-file with a
  [grouped open-decisions index](spec/functional-spec.md#open-index).
  Current review 2026-10-12: all nine prior provisional requirements ratified;
  B9 no-refusal sub-leg selected; R-LAY-04 parked OPEN; G-06 makes
  REQ-MAX-08 NORMATIVE. Batch 1 approved 16 more rows; thirteen units / seventeen rows
  approved 2026-10-12, all 28 triage units decided. Totals: 138 NORMATIVE,
  28 OPEN, 0 PROVISIONAL requirement rows; 156 scenarios including R-WS-27 with TBD
  reference outcomes. Platform implementation/native gaps stay
  distinct from selected intent; Windows control cells now cite Windows evidence.
  [Cross-WM consensus analysis](research/reference-wm-consensus.md) Table A
  now lists 24 Ours-vs-strong-consensus differences with recommendations
  (selected decisions in [decisions](decisions.md)), plus strong-consensus predicates where Ours is
  TBD pending native observation. Splits decided 2026-10-07:
  R-DRAG-07 host-native presentation (KDE frame follows pointer, Windows
  Win-drag stationary preview) and R-DRAG-08 focus at press on both
  platforms (Windows implementation pending, KDE timing needs a live check).
  Historical live-test queue snapshot: 508 cells grouped by environment
  (2026-10-07, not the current TBD total); candidate per-WM environments
  are proposed in the
  [live-test environment proposal](research/live-test-vms/proposal.md).
  Table A selections and 2026-10-10 comparison decisions are recorded in
  [decisions](decisions.md#reference-matrix-and-spec-authority);
  priority native TBD rows R-WS-02, R-WS-04, R-WS-05, R-START-03, R-MAX-01 remain.
- P3 | Prior-art catalogue upkeep | Catalogue delivered 2026-10-03; maintain
  [the index](research/prior-art.md) as projects are studied. Deferred general
  source analysis: FancyWM core submodules and niri/sway/river/awesome/dwm
  (distinct from the completed bounded matrix source-fill passes).
  [Evidence](changes/archive/prior-art-catalogue.md).
- P1 | macOS port | Starts after KDE 0.1 ships (user 2026-10-10). Readiness research
  done (`f095042`): setup/TCC/signing runbook, sourced prior-art survey and
  tentative plan, which recommended public AX/AppKit. User direction
  (2026-10-03) supersedes that default: lower-level, lower-jank yabai-style
  route first, optional higher-level AeroSpace-style alternative later;
  default tier 2 (public plus private APIs, SIP enabled, no Dock injection).
  Plan revised to tier 2 from nine pinned local sources (`df018fb`,
  `05d1d1f`; [record](changes/archive/macos-tier2-plan.md)). Phase 0 inputs
  are decided when macOS starts: host model/macOS floor (Apple Silicon,
  provisional 15+), Intel (arm64 first), stable signer (Developer ID if
  available), Meta mapping (Cmd). Core extraction (K2/K3) and shared restart
  store remain tied to that start. Carry the non-native capability-first
  spikes and one registration plus one interception hotkey path into Phase 0,
  then Phase 1 lifecycle/recovery and hotkey probes on a Mac.
  [API layer/events](research/non-native-tiler-host-interaction.md#d1-api-layer-authority-events-and-reconcile),
  [hotkeys/consent](research/non-native-tiler-host-interaction.md#d2-hotkeys-consent-conflicts-secure-input-taps-focus),
  [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [evidence](changes/archive/non-native-tiler-host-interaction.md).
  [setup](macos-dev-environment.md)
  [survey](research/macos-port/prior-art.md)
  [plan](research/macos-port/plan.md)
- P1 | Rust toolchain tracking (recurring) | User (2026-09-30): track latest
  stable Rust pre-1.0 and fix breakage. Windows uses rustup `stable`; Linux
  and CI get Rust from the nixpkgs pins in `devenv.yaml` and `flake.lock`, which must be bumped
  regularly to stay close to stable. Revisit the upgrade process at 1.0.
  Checked 2026-10-04 (`4212456`, `929adf5`, `c486c59`; CI green): hosted
  Windows/macOS CI now refreshes stable (1.99.0); Linux devenv stays at
  1.98.1 and native CI flake at 1.97.1 until 1.99.0 reaches nixos-unstable
  (read-only recheck 2026-10-09: devenv `e554fab7` 1.98.1; native flake
  `54ba4bce` 1.97.1, lagging devenv; latest stable 1.99.0 dated 2026-10-01;
   unstable HEAD `e7439b6b` dated 2026-10-08 still 1.98.1, no `1_99.nix`;
2945:   read-only recheck 2026-10-12: latest stable still 1.99.0 dated 2026-10-01;
2946:   unstable HEAD `6510408d` dated 2026-10-10 still defaults to 1.98.1, no
2947:   `1_99.nix`, KWin/Qt/KF unchanged at 6.7.5 / 6.11.2 / 6.30.0; 1.99.0 has
2948:   reached `staging-next` but per procedure that route needs a separate decision).
  No bump. Next: recheck, then bump devenv and native flake
  pins; user re-enters devenv after the bump.
  [record](changes/archive/rust-toolchain-tracking-2026-10.md)
- PARKED | Group underlay stage C (unfocused dragged window) | A/B delivered
  and user-confirmed 2026-10-05; multi-output remap checks remain under K1 visual
  policy below. C is parked
  for reassessment with Windows parity item 7; stop/report if it grows complex.
  [change](changes/group-underlay-move-trigger.md)
- P1 | Ghostty/local native alignment | The ~56 px shortfall is unexplained
  (source-only baseline fix is in; needs a fresh `just dev trace` local-move
  plus follow-on command trace), and one requested `2032x1092` became
  `1920x1036` while another primary Ghostty accepted full size (per-window
  native cap or stale output-derived cap unproven). Hypothesis
  (2026-09-28): 1920x1036 equals another output's work area, so the
  window may be constrained to the other output. Next: `just dev trace`
  with one tall tiled window on the source output; check output, bounds and constraints.
  Gates reconciliation phase 2.
  [bounds fix](changes/archive/multi-output-domain-bounds.md)
  [phase 2 parked](changes/learned-size-limits.md)
  [drag investigation](changes/window-alignment-drag-investigation.md)
- P1 | External NixOS/Home Manager delivery validation | Clean external
  install, update, rollback, and host-matching KWin ABI still unproven
  on an independent host. Remains P1 in the 0.1 gate (user 2026-10-10).
  [change](changes/archive/nix-current-host-delivery.md)
- P1 | 0.1 packaging: mainstream install path | Nix flake/Home Manager,
  GitHub Release and OBS distro packages (user 2026-10-10; KDE Store excluded).
  OBS builds release tags only into one stable repository: openSUSE Tumbleweed,
  Fedora 43/44 with updates repos; Arch via AUR PKGBUILD, not OBS pacman.
  Ubuntu 26.04/26.10 support delivered offline under the KWin 6.6 floor;
  [evidence](changes/archive/kwin-6.6-support.md). KDE neon OBS provisioning
  remains unverified. User creates OBS
  account/project, GitHub-to-OBS token and AUR account next; spec, PKGBUILD
  and OBS service prep may proceed offline.
  Acceptance: core script/planner/tray independent of optional ABI-bound effect;
  KWin updates remain possible without the companion (Debian exactly pins the
  compiled-against KWin build); mismatched/absent effect degrades borders/drag preview
  until rebuild. Must-pass missing/failed/removed effect check keeps tiling working
  and KWin stable. Install/uninstall docs, package descriptions and a post-removal
  package note say "press Revert in Settings before removing".
  [research](research/distribution-package-feasibility/feasibility.md)
  [OBS](research/distribution-package-feasibility/obs.md)
  Offline prep delivered 2026-10-11: tagged-SHA prebuilt JS + vendored Rust
  source archive, checksums, release/credential-gated trigger workflows,
  RPM and AUR split recipes, Ubuntu core Debian recipe, user activation units
  and Revert guidance. Exact scratch-tag archive built without networking in
  Tumbleweed/Fedora 44 RPM, Arch split and Ubuntu 26.04 Debian containers;
  Fedora 43 passed an earlier source-contract probe. External Nix validation
  remains separate. Settings ownership is delivered: both KCMs remain in core
  when the effect is absent. Packaging item remains P1: resolve
  OBS stable-source/checksum decision (0.1 blocker status OPEN) before enabling token,
  deliver third-party notices, provision accounts/targets
  and prove remaining solver/live-session behavior (Ubuntu apt solver passed). Neon target
  provisioning remains open.
  [Preparation](changes/archive/release-0.1-offline-packaging.md)
- P2 | Simplify drag preview + cross-output drop code | Post-live cleanup only:
  cut advisory `hover_prior` validation, lag-only fences, verbose comments;
  net deletion, no behavior change.
  [change](changes/archive/cross-output-drag-preview.md)
- P2 | Live sibling reflow while dragging | Deferred, not light (est. several
  hundred to ~1,000 lines, write-fighting risk); research done.
  [research](research/drag-and-drop-reorganisation.md). Link with post-0.1
  KDE interaction performance/threading and Push code down: research KWin
  effect/script threading constraints before considering multithreaded Rust
  interaction work with thin host shims (User 2026-10-13).
- P2 | Robust difference reconciliation | Scope decided (user, 2026-09-28):
  (1) event/next-command detection only, no polling (revisit after testing);
  (2) learn size limits only from settled repeatable evidence; (3) replan
  neighbours around learned limits only, native maximum behavior unchanged;
  (4) KWin script and Rust tiling only, native effect unchanged. Added
  complexity must deliver more value than it costs. Phase 1 (shared
  foreground/hidden classifier, refresh classification logs) passed the
  user's live test at `062d707` (2026-09-28, "felt good and minimally
  janky"; trace `omnitiler-dev.uE1S5n.log`). Quiet
  refresh outcomes were ~60% of refresh log lines. Phase 2 (learned limits)
  parked by the user (2026-09-28) after a candidate exceeded the complexity
  rule; preserved on branch `wip/learned-size-limits`; resumes only if the
  P1 Ghostty/output check leaves a genuine client-held limit.
  [record](changes/archive/robust-difference-reconciliation.md)
  [parked](changes/learned-size-limits.md)
- P2 | PID 3568836 SIGABRT (`QKeySequence` D-Bus abort) | Formally open;
  needs sender, method, and fault-stack evidence before attributing it to
  the effect or script.
  [record](changes/archive/kwin-qkeysequence-dbus-abort.md)
- PARKED | JavaScript workload evidence | User decision 2026-10-09;
  relevant only if a native replacement of KWin-script window add/remove
  handling is proposed, requiring sustained-workload evidence then.
  [change](changes/js-workload.md)
- DEFERRED 2026-10-10 | Reference matrix follow-ups | Both host-source pinning
  (GNOME Shell/Mutter/wlroots; former estimate 19 cells, now 40 host-source
  candidates after reclassification: 38 PaperWM, 2 sway) and fixture/scenario
  wording tightening (302 cells across 92 rows) are deferred. Revisit wording
  only when a specific open decision depends on a row; neither is P1 work.
  [Residual ledger](changes/archive/reference-source-fill-remaining.md),
  [decision](decisions.md#reference-matrix-and-spec-authority).
- P2 | Keyboard-layout support | After initial release; US keyboards only
  for MVP.
  [change](changes/shortcuts.md)
- P3 | AR6/AR7 workspace model + portable policy in core | Deferred until a
  non-KWin host needs it; current KWin is the "native workspaces" mode.
  Portable layout/domain relocation is shared; remaining host mapping is
  not proven extracted. Revisit when macOS starts; candidates and gates are in the
  cross-platform audit.
  [design](changes/architecture-review-ar6-workspaces.md)
  [audit](research/cross-platform-core/extraction.md)
- PARKED | Live-test reference environments | Deferred until after 0.1;
  revisit when a specific decision needs live reference evidence or tabbed
  stacks are designed (user 2026-10-10). Keep the proposal as research.
  [proposal](research/live-test-vms/proposal.md)
- P3 | Gap-drag anchor (post-0.1) | No anchor at gap 0; gap-drag exists only
  with gaps above 0, edge-drag covers gap 0 (user 2026-10-10). Research complete
  (layer-shell gap surfaces first, KWin input filter fallback; needs a Rust
  split-boundary request).
  [native boundary](decisions.md#engine-architecture-and-convergence)
- P3 | Stale branches | Twelve stale branches need explicit user
  authorization before deletion.
  [branches](https://github.com/beefsack/OmniTiler/branches)
- P3 | Other compositor validation | bspwm, Hyprland, COSMIC runtime
  validation only; pinned-semantics research is not a substitute.
  [comparison](reference-wm-comparison.md)
  [profile research](research/reference-wm-profile-support.md)
- P3 | Artifact publication | GitHub Release plus OBS distro packages once
  0.1 delivery prerequisites are complete, for the 0.1 release; KDE Store
  excluded (user 2026-10-10).
  [foundations](changes/archive/delivered-foundations.md)
- P3 | Live workspace-mode switch (post-MVP) | Quiesce, rebuild mapping,
  fresh-adopt without native moves; needs a safe Planner generation
  transition.
  [research](research/live-settings-after-ar15.md)
- P3 | Post-MVP tiling profiles | Re-expose `shortcutProfile` with distinct
  catalogs and live switching when adding new tiling types.
  [change](changes/shortcuts.md)
- P3 | Tabbed stacks (next after the 0.1 release) | User 2026-10-07: tabs are
  a feature of the most important reference tilers (COSMIC, Hyprland, i3,
  sway), so V-GROUP-STACK leaves deferral and is scheduled as the first
  item after 0.1. User decision 2026-10-09: built in the Rust engine, with
  their own evidence when designed; no Custom Tile stability-proof precondition.
  User sketch: size stacked windows slightly shorter and
  draw the tab strip with machinery like the group underlay. Matrix
  inputs: R-INS-02, R-GRP-01..03 (closing the active tab keeps the group
  and activates the next tab, 4/4 references), R-DRAG-01, R-COL-06. Risk:
  mouse interaction with tabs across Linux, Windows and macOS may be
  complex; scope keyboard-first if so.

## Future

Unprioritised ideas; not scheduled.

<a id="pinned-persistent-workspaces"></a>
- Pinned/persistent workspaces (COSMIC pin, Hyprland `persistent` rule, niri
  named workspaces): an empty-surviving workspace with monitor affinity;
  pinned workspaces keep their tiling override across restart; group with
  the shared restart store (G-07, User 2026-10-10). Ordinary unpinned overrides
  still reset; feature design/implementation remains Future scope.

- Windows app-owned fullscreen UX spike: options for exiting/toggling
  fullscreen an application entered itself (user 2026-10-03, later).
  User 2026-10-07 (R-MAX-05, references 8/8 exit without refusal): keep
  the refusal until the spike; explore capturing/retaining window style
  and frame state early (at admission or before fullscreen) so an exit
  can restore without guessing at fullscreen time.
- Windows alternate Game Bar shortcut experiment: keyboard and mouse players
  need a shortcut that opens Game Bar over a fullscreen game (for example
  Xbox voice chat); Start menu and controller access are not enough (user
  2026-10-03, after parity and correctness work).

- Automatic uninstall revert of host settings: parked research after 0.1
  (user 2026-10-10), aiming for "uninstall leaves no changed KDE settings".
  Possibly via package manager hooks; no verified per-user hook exists for
  Nix/Home Manager, KDE Store or distro packages. For 0.1, document pressing
  Revert in Settings before removal in install/uninstall docs, package
  descriptions and a post-removal package note (P1 packaging acceptance).
  [research](changes/archive/host-settings-conflicts.md)

## Pending live checks

Items below retain their stated pending scope; dated user confirmations are
recorded separately from unexercised legs. Reference-WM checks test other compositors.

- Proposal B classification approved exactly User 2026-10-12: original 53
  checks = 39 must-pass / 12 known-issue-allowed / 2 Windows-release. K checks:
  5, 24, 26, 29, 35, 36, 37, 38, 40, 41, 52, 53; W checks: 3, 4; all others M.
  Per-check classifications are in
  [Proposal B](research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2).
  Known-issue-allowed still requires a listed issue and no crash/silent tiling
  stop. Four approved new must-pass checks below give 57 total = 43/12/2;
  D28's native fullscreen-send/game journey stays a must-pass sub-leg of check 1,
  not a separately counted check. D27's real-game active/idle activation check
  is also a must-pass sub-leg of reference-comparison check 1 (User 2026-10-13),
  counted once. Multi-output must-pass checks require a
  two-output setup (VM or hardware) and must pass before 0.1 can ship.
  Batch-1 extras remain outside this inventory; native/live evidence is pending.

  | Check | Approved classification | Action | Expected result |
  | --- | --- | --- | --- |
  | N1 effect-absent | must-pass | Start with effect missing/disabled; disable mid-session; remove optional effect package; exercise failed endpoint with existing resilience check 28. | Tiling keeps working throughout; KWin stable. |
  | N2 external config | must-pass | Change host configuration outside Settings, then reconfigure. | Validated reconcile without unintended writes; Fix/Revert ownership respected; no crash or silent stop. |
  | N3 sleep + gaming | must-pass | Sleep/wake with fullscreen game and tiled session. | Game unimpeded; tiling recovers; no stuck holds. |
  | N4 scaling + overlays | must-pass | Change scale with maximized/fullscreen/fixed-float windows. | Overlays retained; no fullscreen geometry writes; quiet settlement. |

- D27 KDE real-game activation (must-pass, check 1 sub-leg): compare tiler-off/on
  unsolicited hidden-desktop requests during active play and idle/paused play.
  Native input-freshness gating is expected to mark only during active play;
  idle/pager/taskbar behavior matches without-tiler native behavior. No
  OmniTiler guard/interception/reversion/suppression. Verify valid-token switch
  convergence and missing/stale-token attention without interference. Reopen
  only on real active-play switch-away; native result TBD.
- Underlay candidate C visual check before 0.1: after default implementation,
  check `#662a82da` over black, white and grey wallpapers on KDE; Windows
  matching visual check is queued with handoff item 24. No visual evidence yet.

- 0.1 triage batch 1 (User 2026-10-10; native KDE checks outside the original
  Proposal B inventory). These are
  verification-only, with D06/D17
  observe-first; record actual outcomes in existing
  [reference scenarios](spec/reference-outcomes.md), leaving unsupported legs TBD.
  - D02 / R-SPC-01: open transient/modeless and modal dialog legs; dialog stays
    floating, tiles untouched. Record native type mapping and parent-focus/
    modal fence results separately; those request outcomes remain unselected.
  - D04 / R-INS-03: open the first ordinary window on an empty shown workspace;
    full work-area tile and real native newcomer focus, not just desired focus.
  - D05 / R-INS-05 (offline-proven, native live pending): focus an ordinary float
    after B was the tiled focus; open a newcomer. Anchor at tiling neighbor B
    using its long edge, not root-wrap; verify real native newcomer activation.
  - D06 / R-OUT-05 (two outputs, observe-first): occupy both, focus one with
    pointer on the other; open a newcomer. Focused-output arrival and newcomer
    activation required; record native mapping before designing any reroute.
  - D07 / R-CLOSE-02: close, refocus a different tile, fresh-reopen; new admission
    at current focus, no old-slot restoration. Exact ratio/order/focus evidence TBD.
  - D08 / R-CLOSE-03: close the sole shown-workspace window; same workspace stays
    shown empty, with no focused client.
  - D11 / R-MOV-01: flat-triple perpendicular move produces V[H[A,C],B], changing
    root orientation rather than refusing; shared R1 is already proven offline.
  - D14 / R-RSZ-02/03: outward edge resize no-ops; nested resize changes adjacent
    shares at nearest matching-edge-axis split, not ancestor shares. KDE pixel
    path exists; Windows keyboard legs await existing handoff item 6.
  - D15 / R-MOU-01: hover between clients retains focus; plain click focuses at
    press. Host-native default path; no focus-policy mutation selected.
  - D17 / R-WS-09 (observe-first): focus B, select WS2 then WS1 with no sends;
    remembered B must return. KDE shell-driven outcome/viewport remains unverified.
  - D18 / R-WS-02: use existing send-back/return journey; verify B inserts after
    remembered A in tall and wide legs. Anchor synthetic proof exists; exact
    after-order/feel pending, no setting.
  - D19 / R-FLT-13: switch away from ordinary float F then return; F hides while
    away, frame retained, remembered focus restored under D17. Desktop membership
    already scopes KDE floats; Windows hide/reveal is already wired.
  - D20 / R-FLT-12: focus each overlapping float in turn; native stacking raises
    the focused float. Existing activation is sourced; relative order TBD.
    No lower-verb check or implementation is required for now.
  - D21 / R-FLT-14: native pointer move/resize preserves free float geometry;
    project resize still refuses. Native journey is verification-only.
  - D24 / R-CLOSE-04: close focused ordinary float; tiles keep their geometry
    and MRU survivor B receives focus. Shared exception-drop already sourced.

- Reference comparison decisions 2026-10-10 (shared/core KDE delivered offline,
  Windows G-05 and G-06 delivered offline; G-37 wiring pending; user-owned checks):
  [Delivery](changes/archive/reference-comparison-implementation.md).
  - G-05: freshly admit (640,0) and (0,480) equal partial-zero hints under
    both predicates: tile for both-axes, untouched automatic float for
    either-axis. Full-zero/unbounded sentinel still tile; fully positive
    fixed still floats; changing settings never reclassifies existing clients.
  - G-06: maximize focused B among tiles, press each directional focus chord:
    no-op until unmaximize/Alt+Tab. In fresh maximized-B legs, directional
    move clears maximize first and then performs the ordinary move; record
    native clear/move timing for horizontal/vertical/partial maximize, synchronous
    and delayed native notifications. Unconfirmed clear must log refusal,
    without a delayed structural move; a later press retries. Repeat with an
    eligible maximized float (clear then half-snap). Existing pointer/fullscreen
    controls unchanged. Windows offline delivery:
    [item 18](changes/archive/windows-maximized-directional-navigation.md);
    Windows ordinary single-output tiled focus fence/clear-then-move and float
    four-half-snaps user-accepted physically 2026-10-12, scoped normal owner,
    Notepad/Paint, base `e506350` plus combined directional/retile/B9 deliveries;
    partial-maximize/exact timing/failure/race/preset/fixed/game/DPI/multi-output
    and KDE native checks remain pending.
  - G-37 (multi-output): visit surviving WS1 then active WS2 on L, with a
    distinct trailing E; migrate WS2 to R. Default shows last remaining E;
    MRU shows eligible WS1. Clear/invalidate history and repeat: MRU falls
    back to last remaining. Check each output's independent history, live
    settings pickup (KCM labels/tooltips/Save/Defaults/Revert), empty migrations
    and unchanged destination insertion. Surviving empty previous qualifies;
    removed/migrated/out-of-scope previous does not, with no recreation.
  - G-D2: send maximized B to another occupied workspace with follow and
    stay; it arrives maximized, unmaximizing there restores its target slot.
    Test numbered/relative and same-/cross-output paths. Windows existing
    maximize policy is verification; KDE native membership/output setters
    leave maximize untouched offline, real compositor carry remains unobserved.
    Fullscreen Windows carry remains item 9/Table A. D28 (b), User 2026-10-12:
    KDE explicit fullscreen send (follow/stay, numbered/relative), including
    a real game, is must-pass for 0.1. Arrival must retain fullscreen; no tiling
    geometry writes to the fullscreen mover beyond native workspace movement,
    no unsolicited fullscreen writes. Record flag, source/target membership,
    view/focus, visible/input state and native geometry/write behavior. If
    KWin cannot carry without dropping/corrupting fullscreen, stop and return
    to the user. Implementation is delivered offline; native/game acceptance
    remains pending. Repeat with a never-tiled born-fullscreen B: no source
    admission or source/target reflow, slotless target hold retained, first
    exit admits at destination. Include a sole held source and empty target.

- KDE two-candidate output move/send/migration (offline delivered 2026-10-09):
  use one wide source and two outputs touching the requested edge. Exhaust local
  moves, then cross with Meta+Shift+arrow; explicit Meta+Ctrl+Alt+arrow sends
  before exhaustion. Place the mover so its centre projects onto each candidate
  in fresh legs, then in a gap to check overlap and left/top ties; rebind stay
  and verify source view/focus. With unequal shared edges, bind whole-workspace
  migration and verify the largest edge wins even when the focused window
  projects onto the smaller candidate. Check work-area placement, chosen-target
  native arrival and no replay, float carry/sticky stay on migration. User-owned;
  native evidence pending; Windows cross-output legs await the item 5 foundation.
  [Record](changes/archive/position-based-output-selection.md),
  [R-MOV-12](spec/reference-outcomes/move.md#r-mov-12-exhausted-up-move-with-two-candidate-outputs-above),
  [R-WS-27](spec/reference-outcomes/workspaces.md#r-ws-27-two-candidate-migration-selection).

### Windows session

- User physical acceptance 2026-10-12: radius 6/live Apply, ordinary float
  focus/half-snaps, G-06 maximum focus fence/clear then move, R-MAX-03
  retile/restore and both born-setting Paint launches accepted under the
  [dogfood conditions](#open-work). Remaining exact fixtures, fixed-size,
  game/DPI/multi-output legs stay pending; no physical B9 claim added.
- Fullscreen exit: owned Notepad managed-slot return and stop/relaunch with
  fullscreen then exit tiling physically user-accepted 2026-10-12, scoped
  single-output Notepad/Paint, base `e506350` plus pre-retirement changes.
  Born-after-start/app-native/borderless, automatic-retirement/explicit-clear
  recovery and fixed/game/DPI/multi-output remain pending. Each Win+F11 also
  raised the known parked Xbox prompt; eventual fix stays in gaming coexistence.
  [Lifecycle record](changes/archive/windows-fullscreen-exit-acceptance.md).
- Placement physical acceptance (fixes live-API verified, ratified 2026-10-08):
  dogfood startup adoption on Windows, Win+M maximize/restore and send returns.
  Clean/tolerance-valid layouts keep their fit; overlap/minimum-infeasible fits
  use sequential long-edge seeds, origin+minimum can overlap/exceed the work area.
  Verify retained-overlay minima and bounded restore timing. KDE startup/B6/Q3
  legs are listed below; the original Windows dogfood anchor remains unknown.
  [Record](changes/archive/windows-placement-correctness.md).
- Delivered Windows parity/settings/tray physical acceptance: maximize,
  fullscreen, float/sticky, title-bar/Win-drag feel, physical shortcuts and
  Snap Layouts/bar/Assist/shake; real Explorer restart and other DPI/output
  arrangements; Steam/Firefox/UWP admission observations from the completed
  [reference-WM comparison](research/windows-port/reference-wm-comparison.md).
  Settings Apply/Revert and tray proof are delivered, not open
  implementation; physical containment limitations remain disclosed/parked.
  [Settings](changes/archive/windows-settings.md),
  [tray/first-run](changes/archive/windows-tray-first-run.md),
  [foreground acceptance](changes/archive/windows-foreground-acceptance.md).

### Reference WMs (user)

- B9 / R-FLT-06 on COSMIC: tiled workspace, intentionally float B, natively
  maximize it, then toggle float off once. Confirm the sourced
  unmaximize-then-fresh-admit behavior selected 2026-10-08. Record the
  actual result; this is confirmation, not a product or implementation gate.
  [row](spec/reference-outcomes/floating.md)

### Single-output setup

- B1/B2 native-change repeats and held-key autorepeat (Q8 offline evidence
  delivered): repeat Meta+M/native-restore/repress and the inverse ordering;
  repeat sticky-on/native-unstick/repress and sticky-off/native-restick/repress,
  both tiled and floating origins. Hold/release Meta+M and Meta+Shift+G, record
  KGlobalAccel callback count/visible cycling and release/repress behavior.
  Physical repeat behavior remains TBD; these live checks stay user-owned.
  [Original steps](changes/archive/kde-post-windows-followups.md#user-owned-test-system-checks),
  [offline record](changes/archive/kde-repeat-offline-fixtures.md),
  [discriminator](spec/reference-outcomes.md#r-max-04--r-flt-02-kde-held-key-discriminator).
- B9 unfloat while maximized on KDE (offline delivered): intentionally float
  B, natively maximize it, then toggle float off once. Verify maximize clears
  before fresh tiled placement, focus/settlement logs, ordinary nonmax unfloat,
  fixed-client explicit tile override, refused/raced clear and later-press
  recovery. Fullscreen/sticky gates remain unchanged. User-owned, native TBD.
  [Record](changes/archive/maximized-intentional-unfloat.md).
- D7 fixed-window tile override across script reload/planner restart (KDE
  offline delivered): successfully tile a fixed client, reload the script and
  restart the planner with the same live client; it remains tiled. Check hint
  loss/predicate changes then workspace enable, re-float/close/complete-inventory
  prune, hidden clients, degraded/old-format/unmatched fallback to untouched
  fixed float, and separate hydrate/persist/clear ACK counts. Ordinary non-fixed
  commands gain no tile persistence; float controls receive zero extra writes.
  [Record](changes/archive/fixed-window-tile-override-restart.md). User-owned, native TBD.
- D5 born-fullscreen first exit incl. games (KDE offline delivered): both
  predicates, current hints/predicate after fullscreen churn, non-fixed exit
  tiles, prior fixed float restores, later toggles retain identity. Verify no
  fullscreen writes and no geometry/focus/stacking/keep-above/maximize-clear
  writes to fixed first exits; correlated classification logs.
  [Record](changes/archive/fixed-size-exit-and-enable.md). User-owned, native TBD.
- D6 tiling enable with fixed windows (KDE offline delivered): fixed/non-fixed
  arrivals while floating, formerly ordinary tiles gaining fixed hints,
  predicate changes, explicit user tile overrides, intentional/sticky floats
  and fixed/maximized controls. Fixed automatic floats stay untouched; others
  tile, overrides retain Q3 slots. Verify classification logs and no writes.
  [Record](changes/archive/fixed-size-exit-and-enable.md). User-owned, native TBD.
- KDE Q3 R-RST-01/03/04 (offline delivered, user decisions 2026-10-08): intentional
  ordinary float plus automatic fixed float and a tile; script stop/start
  preserves ordinary intent/current frame, automatic fixed identity recomputes.
  Move/resize the intentional float while stopped; restart must not write its
  geometry, focus or stacking. Check hidden/minimized and excluded live clients,
  native hidden-to-hidden relocation, verified close during startup and settled
  unfloat. Restart the planner alone; current float observation still recovers
  without rehydration. Corrupt the private store only in a user-owned isolated
  journey: normal degraded reason/count logs, empty adoption, then next settled
  full write replaces state. Store write failure keeps local/native state.
  User logout/login/new KWin must not adopt older intent; no settings/layout
  persistence claim. Physical startup-time sends and separate storage ACK
  tokens also need verification. [Record](changes/archive/kde-intentional-float-restart.md),
  [live guide](live-kwin-testing.md). No Q3 live result claimed.
- KDE D1 settings UI (offline delivered): verify predicate labels/tooltips,
  Save/Defaults/Revert and live pickup; one-axis-fixed new clients tile under
  `both-axes-fixed`, float without writes under `either-axis-fixed`. Switch
  values with existing tiles/floats: no reclassification, live tile overrides
  survive; subsequent admissions use the new value. Verify renamed Same-axis
  move labels/tooltips and effective reload logs.
  [Record](changes/archive/admission-and-move-settings.md). User-owned, native TBD.
- KDE Q2 R-SPC-04/06..13 (offline delivered, user decisions 2026-10-08): open a
  fixed-size ordinary client beside a tile; record unchanged incoming frame,
  focus and stacking, including a fixed borderless game. Check single-axis
  clients tile by default and float under `either-axis-fixed`; equal partial-zero
  clients tile by default and float via their nonzero axis under `either-axis-fixed`
  after G-05 delivery (offline complete; exact partial-zero legs are above);
  unset/full-zero/sentinel
  hints do not auto-float. Gain/lose fixed hints after admission without
  changing float identity. Meta+G tiles an automatic float and stays tiled
  through minimize/restore, workspace/output observation and domain re-adoption;
  new client/ref recomputes; owner restart restores matched fixed-window tile
  overrides with recompute fallback (D7 delivered offline). Sticky origin commands retain
  their prior semantics. Fixed born-maximized stays floating beneath native
  maximize; workspace enable keeps fixed automatic floats slotless while
  maximized, native unmaximize stays floating (explicit tile overrides use Q3).
  Born-fullscreen first exit classifies
  as newly admitted (fixed floats with no writes,
  otherwise tiles; prior fixed-floating fullscreen restores floating; no
  writes while fullscreen). Workspace enable checks every window being tiled
  (fixed stays/becomes untouched float including arrivals
  while floating, others tile; user tile overrides stay tiled;
  intentional/sticky keep existing rules).
  Foreground/hidden startup and correlated classification/admission/terminal
  logs need native verification. [Record](changes/archive/fixed-size-admission.md),
  [live guide](live-kwin-testing.md). No Q2 live result claimed.
- KDE R-WS-08/11 item 1: user reported "worked perfectly" on a SINGLE output
  (2026-10-07); single-output native journey confirmed. The report did not
  specify individual edge/>9 cases or which presets were exercised.
  Remaining: multi-output per-output/shared scope, hotplug/return (R-WS-15..17),
  and preset-specific checks. Verify Authentic's confirmed Apply/Force clears stock KWin "Switch One
  Desktop to the Left", "Switch One Desktop to the Right", "Switch One
  Desktop Up", and "Switch One Desktop Down" holders; Compatible disables our
  four arrows while letters/Tab work, stock holders remain unchanged on a
  fresh baseline, and Revert restores defaults after earlier clearing.
  [Offline record](changes/archive/kde-workspace-history-ring.md),
  [live guide](live-kwin-testing.md).
- KDE R-WS-01/14 item 2 (offline delivered): physical relative send-follow
  Meta+Ctrl+Shift+H/K/Left/Up previous and J/L/Down/Right next. Check both-end
  wrap, >9 and trailing-empty fill/new spare, including a sole mover emptying
  the source. Rebind numbered/append and relative send-and-stay from their
  empty defaults: source remains selected, source tiled MRU gets focus,
  destination not selected; check empty-source native focus separately.
  Floating-boundary default send now follows; explicit stay keeps source,
  floating frames unchanged and only tiled sides reflow. Verify Authentic
  Apply/Force clears stock "Window One Desktop to the Left/to the Right/Up/Down"
  holders; Compatible disables our four arrows, keeps letters/stay rebindings;
  Revert restores cleared defaults. Multi-output relative-ring scope remains
  user-owned. [Record](changes/archive/kde-workspace-send-follow-stay.md),
  [live guide](live-kwin-testing.md). No item-2 live result claimed.
- KDE R-MOV-03 item 3 (offline delivered): toggle Same-axis move in KCM
  between Group with neighbor and Swap with neighbor. In an N-ary group, move an interior
  leaf toward a direct leaf sibling: grouping creates the nested pair, swapping
  exchanges windows with unequal shares traveling and focus on the mover.
  Verify Save live reread without controller/session restart, including
  switching back on an existing tree; no tree rebuild on setting change.
  Check a group neighbor and group-end move retain existing rules.
  [Record](changes/archive/same-axis-move-setting.md),
  [live guide](live-kwin-testing.md). User-owned; no item-3 live result claimed.
- R-DRAG-08 on KDE: with A focused, Meta+left press on unfocused tiled B,
  move, release at A's edge; record whether B is focused at press, during
  the hold, or only after drop (decision: focus at press).
- KDE Q3 born-maximized overlay (`9b612be`) and B6 origin+minimum
  (`cf6ab31`): steps in
  [Q3 record](changes/archive/kde-born-maximized-overlay.md) and
  [B6 record](changes/archive/kde-minimum-origin-placement.md).
- KDE R-MAX-03 Q3 scope: on a floating workspace, first observe maximized A
  without a tile slot beside ordinary B. Toggle the workspace tiled: A stays
  maximized over a reserved slot, B takes its allocated share. Natively
  unmaximize A: it lands in that slot; repeat maximize/unmaximize and confirm
  quiet settlement with no unsolicited toggles or geometry fighting. Repeat
  with a session-restored maximized A at a user-owned session boundary; no loop.
  [Scope record](changes/archive/kde-maximized-floating-retile-overlay.md),
  [live guide](live-kwin-testing.md).

- Tray workspace toggle: floating/tiled toggling confirmed live by the user
  (2026-09-29). Remaining: default change logs `stage=persist
  ... outcome=written`, `plan:config-reloaded stage=default-tiled`, applies
  to new workspaces and to all workspaces after a session restart;
  cross-boundary send is a native move with tiled-side reflow. Red flags:
  geometry changes while floating, menu check disagreeing with KWin, re-tile
  without confirmed release or `native-failed` sends. The same action now
  defaults to Meta+Y (user 2026-10-10); physical delivery is pending below.
  [change](changes/archive/tray-workspace-toggle.md)
- KDE workspace tiling Meta+Y (offline delivered 2026-10-11): physical chord
  and tray checkbox toggle the same current-workspace state, with matching
  snapshot/menu checks and no effect on another workspace or saved default.
  Verify the native/KCM row, Keep/custom/empty preservation, Authentic reset,
  Compatible holder discovery, Apply/Force/Revert isolation and restart
  persistence, including upgrading an existing empty assignment. No automatic
  correction is expected: explicitly stage Authentic and confirm Apply/Force
  to assign the canonical chord to an existing empty/custom row. Other 36
  unbound rows remain unbound. No stock Plasma 6.7.5 holder found offline;
  distro/user assignments can differ. User-owned, no live result claimed.
  [Record](changes/archive/kde-workspace-tiling-shortcut.md),
  [steps](live-shortcut-override-verification.md#pending-workspace-tiling-default-metay).
- Active border skips Plasma applet popups (user option A, 2026-09-29;
  KRunner out of scope for now): with an ordinary bordered window active,
  opening the Application Launcher or a tray popup hides the border and logs
  `active-border:visible vis=0 reason=applet-popup appletPopup=1`; closing
  it and refocusing restores `vis=1 reason=eligible appletPopup=0`; ordinary
  dialogs and tool windows keep the border. Red flags: border on the
  launcher, `appletPopup=0` while the launcher is active. If more shell
  surfaces still get the border, tighten further.
  [decision](decisions.md#visuals-border-underlay-and-grouping)
- Host settings Revert and tray conflict indicator: Fix confirmed live by the
  user (2026-09-29). Remaining: Revert removes the local key and native edge
  previews return (one `op=revert setting=<key> outcome=ok reason=ok` line
  per click). Tray (fresh session): with a conflict, a `dialog-warning`
  overlay on the icon and a top "Conflicting KDE settings..." menu row that
  opens Settings; Fix clears both without a tray restart and logs one
  `outcome=conflict-updated conflict=false`; Revert brings them back with one
  `conflict=true`. Red flags: stale overlay/row, repeated conflict lines on
  heartbeats, left-click no longer opening the menu, snapshot-loss
  `NeedsAttention` changed.
  [change](changes/archive/host-settings-conflicts.md)

- Confirmed live by the user (2026-09-29): tiling gap Save re-spaces tiles
  without restart and the unified settings page shows everything (closes the
  P0 settings live-application launch blocker); group underlay survives
  workspace switches and `just dev` restarts, its settings apply live and it
  slides with workspace transitions; a plain drag released off-screen
  retiles correctly with no `snapshot-invalid` (trace `H28tD1`); mid-drag
  workspace send now tiles the mover on its destination (after `8be1a32`).
  Maximise and fullscreen hide the underlay; tray Settings and Desktop
  Effects Configure open the unified page. KWin Scripts Configure is not
  listed under `just dev` (script loaded from the worktree, not installed);
  verify with an installed package when available.
  [settings](changes/archive/unified-settings-page.md)
  [underlay](changes/archive/group-underlay-and-preview-colors.md)
  [mid-drag](changes/archive/mid-drag-destination-recovery.md)

- Quiet refresh logs: compare `stage=refresh terminal=quiet` under normal
  and trace logging; non-quiet terminals stay visible in both. (Unfloat
  placement and the duplicate-implementation unification were confirmed live
  by the user on 2026-09-28.)
  [change](changes/archive/unfloat-admission-axis.md)
- Same-output drag drops + preview overlay: user confirmed live at
  `826b233` (2026-09-28) that a paused drag stays under the pointer, the
  preview renders well and drops place correctly. Remaining: overlay
  independent of the Meta outline, Esc/cancel/no-Finished clearing.
  [change](changes/archive/cross-output-drag-preview.md)
  [policy](changes/archive/drag-drop-reorganisation.md)
- Drop-intent edge drag leftovers: tiled Meta+left snap-back, Esc/zero-move,
  size-increment client, and the `p13` 36 px bottom-edge shortfall.
  [follow-up](changes/archive/passive-press-move-snapback.md)
- Convergence step-1 leftovers: close/reopen focus, transient unreadable
  frames, minimize/fullscreen/maximize restoration, rapid commands (sticky /
  Meta+G cross-workspace already confirmed live).
  [change](changes/archive/observation-convergence.md)
- Resilience batches: empty-login open, resize completion, Meta+M; group
  bridge demarshalling, modifier/render delivery and performance, including
  outline after config change and close/reopen; endpoint
  `stage=failed`/`available=1` transitions; tray status current.
  [change 3](changes/archive/resilience-change-3.md)
  [change 4](changes/archive/resilience-change-4.md)
  [audit](changes/archive/resilience-audit.md)
  [sweep](changes/archive/fail-closed-sweep.md)
  [bridge](changes/archive/active-group-highlight-design.md)
- Tray Active status under `just dev` (worktree tray): healthy shows Active,
  snapshot loss shows NeedsAttention; confirm panel presence and lifecycle
  (no tray launcher existed in the earlier report). Systemd checks need a packaged
  Home Manager install: login starts one tray; killing it restarts it; a
  second invocation exits cleanly; logout stops it without a loop; `just dev`
  preserves the packaged owner; tray diagnostics appear once each in
  `journalctl --user` (native journald submission removed).
  [change](changes/archive/tray-and-restart-followups.md)
  [audit](changes/archive/resilience-audit.md)
- Process-loss and sleep recovery cases on a single-output setup.
  [live plan](changes/archive/recovery-process-sleep-audit.md#live-test-plan)
- Native border delivery and suppression: fresh-session plugin discovery,
  border rendering, and oracle endpoint; fullscreen/any-maximise suppresses
  both outlines; active-border disappearance gets a fresh combined trace if
  it recurs.
  [diagnostics](changes/archive/active-border-visibility-diagnostics.md)
  [decision](decisions.md#visuals-border-underlay-and-grouping)
- Sticky adoption restart/re-enable: intentional sticky identity stays sticky;
  unknown-origin native sticky adoption retains the selected sticky-off rules.
  Verify separately from Windows' pending restart-preservation implementation.
  [decision](decisions.md#window-state-float-sticky-maximize-fullscreen)
- Non-visible workspace tiling: hidden startup/open/move with unchanged
  native focus/desktop and no rendered-state claim.
  [record](changes/background-tiling.md)
- Reliability gates: fullscreen residual-cost gate plus gaming cost baseline,
  work-area projection gate (resolution/scaling/
  work-area change incl. fullscreen isolation and restoration).
  Maximize-admission/session-restore checks are covered by Q3/R-MAX-03 above.
  [investigation](changes/reliability-condition-investigation.md)
- Custom Tile manual runtime check: separately authorized run with exact
  restoration.
  [change](changes/custom-tile-runtime.md)
- Nested placement visual smoke: occupied-leaf whole-rectangle preview and
  nested split placement.
  [change](changes/archive/nested-placement-affordance.md)
- Tray live/release acceptance: KWin-owner snapshots, watcher ordering,
  update/rollback; login/autostart uses the packaged systemd checks above.
  [change](changes/archive/architecture-review-ar13-tray.md)
  [carrier](changes/archive/tray-carrier.md)
- Drag-oracle post-fix proof: rebuild/new session with committed resizes,
  8 px gaps, one planned-applied result, no immediate reconcile.
  [decision](decisions.md#pointer-drag-and-drop)
- Startup adoption: current hybrid is clean/tolerance-valid recursive-cut fit,
  otherwise sequential long-edge seeding (`2c918d3`, 2026-10-03), with no
  centre-split fallback. Restart over previously tiled windows was confirmed
  live 2026-09-29. Remaining: a fresh session after confirmed Planner loss;
  check clean tiles preserve their fit and overlapping windows seed a
  long-edge chain (KDE fixtures added in B5).
  [record](changes/placement-aware-startup-adoption.md)
- KWin controller silent unload (diagnostic only): attribute only with
  before/after `isScriptLoaded`, exact `Script<ID>`, and KWin PID/start
  evidence if it recurs.
  [protocol](live-kwin-testing.md)
- Correlated observability live capture: whole-system lifecycle coverage
  and live capture unproven; per-route offline diagnostics shipped.
  [coverage](changes/archive/observability-coverage-assessment.md)

### Multi-output setup

- KDE Q4 R-WS-12/R-WS-21..26 (offline delivered, user decisions 2026-10-08): rebind
  the four empty-default migration actions; local/global-unique with native
  `perOutputVirtualDesktops` on migrates, off/unreadable/shared refuses before
  writes with exact reason. Select active WS2 with nested unequal-share tree,
  occupied target WS3 and hidden WS4; verify stable ID/order/tiling mode,
  insertion after WS3, WS3 hidden, source last remaining and trailing lifecycle.
  Observe delayed member arrival and both views before focus; empty uses native
  output switch without client focus. Check intentional/automatic float remap
  across work areas/scales, sticky source, minimized/transient clients, member
  removal and partial-write reconciliation. D8 migration with fullscreen/maximized
  members (delivered offline): native move only with no extra
  fullscreen size/position/focus writes; an explicit user move is not unwanted
  interference. Check native fullscreen focus retention (no tiler focus write),
  native re-fit across work areas/scales, preserved maximize slot after unmaximize
  on the target, affected-view overlays and mid-flight mode changes. Previous
  invalidation and reconnect forget only moved ID.
  [Record](changes/archive/kde-whole-workspace-output-migration.md),
  [live guide](live-kwin-testing.md). User-owned; native outcomes remain TBD.
- KDE item 5 R-MOV-08/R-OUT-04 (offline delivered, needs two outputs): stack
  outputs vertically; Meta+Shift+Up/Down crosses after local swap/restructure/
  escape is exhausted, including a sole window. Add a panel work-area gap:
  full rectangles still select the neighbor, tiles stay inside work areas.
  Horizontal Meta+Shift+Left/Right still works, including sole windows;
  no candidate stays put, unreadable topology refuses. Multi-candidate
  position-based selection delivered offline in `a8e32ce`; its dedicated
  two-candidate live check is above. Meta+Ctrl+Alt+arrows/
  HJKL sends before local exhaustion to the target's CURRENT workspace and
  follows the mover; rebind directional output-stay from empty defaults and
  verify source selection/MRU focus and ordinary remembered-leaf admission.
  Check membership-only floating boundaries with only tiled-side reflow.
  [Record](changes/archive/four-direction-output-transfer.md),
  [live guide](live-kwin-testing.md). User-owned; no item-5 live result claimed.
- Core extraction K1 visual policy: confirmed by the user
  (2026-09-30: active border, fullscreen/maximise suppression, group
  underlay, drag preview). Remaining: border, underlay and preview remap
  correctly across outputs with differing scales/origins.
  [change](changes/archive/portable-core-k1-visual-policy.md)
- Born-fullscreen dogfood: game fullscreen beside tiles (no gap,
  `initial-fullscreen-held`), exit (fresh tile, `initial-fullscreen-released`),
  re-enter/exit (slot retained); optionally close before first exit.
  [change](changes/archive/born-fullscreen-admission.md)
- Directional + shortcut + convergence re-test: shortcut Apply/Force/Revert per
  [shortcut plan](live-shortcut-override-verification.md) (Lock Session
  relocation, Revert, Phase-2 resize chords insofar as not covered there),
  then the observation-convergence steps 2-3 list (startup fit; open/close
  foreground and hidden; float/sticky/maximize/fullscreen restore; gap
  reload; displaced return; hidden admission; sends incl. rapid;
  refusal/close; R4 occupied/empty/failed; follow-once; phantom revisit;
  correlated logs).
  [contract](changes/shortcut-override.md)
  [failures](changes/archive/multi-output-failures.md)
  [snapshot](changes/archive/multi-output-directional-snapshot.md)
  [convergence](changes/archive/observation-convergence.md)
- Cross-output drops at the pointer: occupied and empty destinations (incl.
  native-output lag), source membership removal, preview/drop agreement at
  group edges, center snap-back with hidden preview, wrong-output and refusal
  recovery.
  [change](changes/archive/cross-output-drag-preview.md)
- KWin restart with surviving Planner: all-new window IDs fresh-adopt
  spatially per output (no ID-order rebuild), tray status recovers; script
  reload with shared IDs keeps groups/splits.
  [change](changes/archive/tray-and-restart-followups.md)
- Reconciliation phase 1 (multi-output setup): hidden and sticky domains stay quiet without
  ping-pong, unreadable outputs imply no departure, stale replies ignored,
  immediate/delayed send/R4 follow and forced refresh.
  [record](changes/archive/robust-difference-reconciliation.md)
- Process-loss and sleep recovery cases on a multi-output setup.
  [live plan](changes/archive/recovery-process-sleep-audit.md#live-test-plan)
- Output hotplug displacement/return: disconnect preserves layouts per
  surviving monitor; reconnect returns workspaces with current contents;
  explicit moves stay at their destination.
  [investigation](changes/reliability-condition-investigation.md)
- Multi-output workspace anti-oscillation: trailing-empty behavior on a
  multi-output setup.
  [runbook](live-oscillation-verification.md)
- Native dev lifecycle removal/dogfood coexistence: removal and
  dogfood-coexistence refusal unverified (startup/discovery already
  accepted).
  [record](changes/archive/native-dev-setup-lifecycle.md)
  [host builder](changes/archive/host-matched-native-development-builds.md)

## Open user decisions

- Windows Win+F11/Win+G containment (release gate after multi-output):
  Authentic ownership and disclosed limitation are selected 2026-10-08;
  Compatible/disable/rebind provide Game Bar access. Later test and user review
  owns containment and any host-setting/thread/registry/policy alternative;
  none of those mechanisms is selected or authorized by the ratification.
  [record](changes/windows-gaming-coexistence.md)
- macOS Phase 0: after KDE 0.1 ships, decide host/floor, Intel support, stable
  signer and Meta mapping when macOS starts. [plan](research/macos-port/plan.md)
- Windows package/update channel: deferred to Windows release planning;
  short research refresh then user decision, most obvious and unsurprising
  for users. Blocks release packaging, not parity. [plan](research/windows-port/plan.md)
- Windows workspace presentation: after multi-output, Windows options
  and recommendation for hidden-workspace Alt+Tab/taskbar semantics plus
  taskbar workspace indicator (parity 10), as a Windows release gate.
  [note](research/windows-port/alt-tab-hidden-workspaces.md)
- Other unselected behavior: the functional spec's
  [open-decisions index](spec/functional-spec.md#open-index) lists 28 OPEN
  requirement rows; no PROVISIONAL requirement rows remain. KDE fullscreen
  carry is delivered offline under NORMATIVE REQ-MAX-09b (D28 (b), with
  born-fullscreen option (a)); native acceptance remains pending.
  Design questions remain distinct from delivered implementation/live gaps.

## Known issues and risks

- Approved 0.1 known limitation D26(b) (User 2026-10-12): intentional-float
  memory does not survive logout/login under unchanged session-scoped
  REQ-RST-01d lifetime; reopened apps have new identities. Restore persisted
  gaps/settings and re-observe host set/membership/maximize; no invented layout
  restore. Verification-only; native dogfood/live checks will assess. Cross-login
  memory is revisited after 0.1 with the shared restart store.
- Borderless-windowed fullscreen heuristic (born-fullscreen option 3):
  watched, conditional (user 2026-10-10). The user watches for borderless
  games during dogfooding; pursue only if games arrive non-fullscreen.
  [change](changes/archive/born-fullscreen-admission.md)
- PID 3568836 SIGABRT (`QKeySequence` D-Bus abort) unattributed; needs
  sender, method, and fault-stack evidence.
  [record](changes/archive/kwin-qkeysequence-dbus-abort.md)
- Whole-snapshot `window-out-of-bounds` rejection removed offline
  (2026-09-29); observed out-of-work-area frames are now tolerated drift.
  Watch for any regression where a genuinely foreign window is tiled.
  [change](changes/archive/mid-drag-workspace-recovery.md)
- Ghostty-class shortfalls (~56 px) have no proven native cause; interim
  acceptance can leave a visible gap or reassert the accepted rect when a
  neighbour drifts.
  [bounds fix](changes/archive/multi-output-domain-bounds.md)
  [drag investigation](changes/window-alignment-drag-investigation.md)
  [record](changes/archive/robust-difference-reconciliation.md)
- Sticky pager appearance and Ghostty fullscreen-to-maximise on workspace
  return remain unconfirmed; no native cause or workaround established.
  [behavior](decisions.md#window-state-float-sticky-maximize-fullscreen)
