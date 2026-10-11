# Ubuntu native effect research

## Goal and scope

Find a viable Ubuntu 26.04 shipping route for the 0.1 KDE native effect:
active border, group underlay and drop preview. Research only; product build
floors, CMake and packaging remain unchanged pending a user decision.

## Acceptance

- Assess the actual ECM module requirements and source-archive bundling.
- Check stock KWin development headers, source compatibility and runtime ABI
  constraints against the current effect target.
- Establish whether prebuilt archive JavaScript removes the Node build-floor
  obstacle.
- Assess Kubuntu backports, KDE neon provisioning and OBS Ubuntu targets.
- Prefer a stock Ubuntu container build experiment; distinguish verified
  results from unresolved runtime and service claims.
- Publish a sourced recommendation and options in
  [the research report](../../research/distribution-package-feasibility/ubuntu-native-effect.md),
  then link its pending decision from the backlog.

## Approach

1. Inspect existing evidence and effect/archive sources; test ECM and KWin
   compatibility in an isolated Ubuntu container.
2. Check current distribution repositories and OBS provisioning constraints.
3. Review evidence, record options and consequences, and retain all material
   product choices for user decision.

## Outcome

- Research complete 2026-10-11; recommendation and product choices remain
  pending user decision in the linked report. Build floors and recipes were
  not changed.
- Stock Ubuntu updates now provide KWin 6.6.6 (base remains 6.6.4), ECM 6.24,
  Qt 6.10.2 and KF6 6.24. A throwaway ECM floor change to 6.24 produced the
  full native tree and correct install paths without C++ changes.
- Effect-only build with locked vendored Rust and network disabled passed
  configure/build/install and six offscreen/non-compositor checks. The effect
  embeds the matching KWin 6.6.6 IID. Original-floor configure fails on ECM;
  missing container DRM/pkg-config/certificate inputs were corrected before
  successful verification.
- No used ECM module needs 6.26. Bundling is license-feasible but untested;
  prefer a complete pinned upstream source over an unproven selective
  module/template closure if that fallback is selected.
- The supplied KWin reference resolves to 6.7.4, not the 6.7.5 distro target.
  Stock internal headers suffice; version guards are unnecessary for the
  proven source build and cannot remove same-version ABI risk.
- Prebuilt archive JavaScript removes Node from distro builds. A separate
  Node 22 bundle probe passed with an engine warning; no supported-floor
  change is inferred.
- Kubuntu backports offers no newer KWin/ECM than stock updates. Neon User
  publishes KWin 6.7.5 and ECM 6.30 on noble, but no public OBS target was
  verified. OBS Ubuntu:26.04 exists with universe/update pockets; actual
  backend builds, source handoff, runtime/solver validation remain pending.
- Next action: user selects the route and base/updates coverage. Recommended
  follow-up is ECM 6.24 plus an effect-only Debian companion for stock
  updated Ubuntu, then exact-archive/OBS and runtime validation.
