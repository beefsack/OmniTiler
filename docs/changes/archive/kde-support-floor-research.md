# KDE support floor research

## Goal and scope

Extend the Ubuntu native-effect research with release-pocket Ubuntu 26.04,
Debian 13 and older KDE stack compatibility. Research and isolated experiments
only; recommend a supported floor without changing product/build requirements.

## Acceptance

- Confirm current/previous Ubuntu LTS and Debian stable/LTS KDE baselines.
- Attempt an effect build against Ubuntu 26.04 release-pocket KWin 6.6.4.
- Attempt core, Settings and effect builds on Debian 13 stock dependencies;
  identify ECM, Qt/KF6, Rust/locked-dependency and CMake blockers precisely.
- Audit consumed KWin scripting APIs against older Plasma 6 source tags;
  distinguish required API gaps from guarded optional behavior.
- Record a distro compatibility table and recommended support floor, with
  source/build evidence and unverified runtime limitations.
- Add brief neon package-reuse context and update the pending-decision backlog.

## Approach

1. Verify release-pocket Ubuntu and Debian source builds in disposable builders.
2. Compare the script's consumed APIs and older distribution baselines.
3. Review evidence and update
   [the research report](../../research/distribution-package-feasibility/ubuntu-native-effect.md#support-floor-and-older-distributions).

## Outcome

- Complete 2026-10-11; support scope remains pending user decision.
- Release-only Ubuntu 26.04 effect build against matching KWin 6.6.4 headers
  and library passed offline build/install plus 6/6 checks at ECM 6.24.
  Image-baked updates initially prevented release-only dependency resolution;
  a pinned downgrade in the builder resolved preparation.
- Debian 13: Settings build/install passed at ECM 6.13 / Qt 6.8.2 / KF6 6.13.
  Stock Rust 1.85.1 failed locked core dependencies and newer source features.
  A private ECM 6.24 bundle cleared the effect's macro configure failure;
  newer-Rust diagnostic built core/staticlib but the effect C++ failed on
  missing `scene/outlinedborderitem.h`. No source compatibility patch applied.
- Observed Rust lower bound is 1.88; minimum compiler versions were not
  exhaustively tested. Current manifests do not declare an MSRV.
- Required per-output desktop script APIs arrived in KWin 6.7. Observation
  gates require them regardless of workspace mode; shared-mode navigation's
  fallback does not restore tiling snapshots. This supersedes treating the
  earlier stock-Ubuntu native build recommendation as full-product support.
- Native border-header introduction is 6.5; older Leap/Fedora base sources
  lack it. Qt6/KF6 and renamed script interfaces exclude stock Plasma 5
  without a separate port. Debian 12 is now LTS; Ubuntu 26.04 is current LTS.
- Recommendation: KWin 6.7 for unchanged 0.1 scope. Stock Ubuntu support
  requires a separate script/workspace compatibility decision before native
  companion delivery. Neon base migration does not remove exact-KWin IID
  matching or Qt/KF6 dependency-validation requirements.
- Next action: user chooses the support floor versus older-KWin compatibility
  work. Runtime/compositor, package-solver and actual OBS acceptance remain
  distinct pending gates.
