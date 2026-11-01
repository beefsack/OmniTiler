# macOS tier-2 plan revision

## Goal and scope

- Replace the superseded public-only default in the
  [macOS plan](../../research/macos-port/plan.md) with the user's 2026-10-03
  tier-2 direction: public plus private APIs, SIP enabled, no Dock injection.
- Research local reference implementations and map capability evidence to
  shared Rust policy and macOS adapter responsibilities.
- Documentation only; no live testing, installs, adapter implementation,
  backlog edits or new product decisions.

## Acceptance

- Exact source file/line evidence and checkout revisions for observation,
  geometry/latency, focus, workspaces, input, visuals, displays/DPI,
  fullscreen/Spaces, recovery, permissions, signing and version support.
- Verified source facts separated from proposals and runtime unknowns.
- Windows-shaped phases, risks and user-owned host/floor, Intel, signer and
  modifier choices with options and recommendations.
- ASCII, valid local links, whitespace checks, committed/pushed documentation
  and green hosted CI; archive this record on completion.

## Approach and units

1. One bounded research/edit unit: inspect the nine local macOS references,
   current supporting docs and adapter seams; revise the plan only.
2. Independent source review; correct authority/recovery overclaims and inspect
   the newer yabai non-SA Space/focus implementations.
3. Source spot-checks and evidence/link/ASCII/whitespace verification;
   publish, inspect hosted CI, then archive this record.

## Material decisions

- The tier-2 default is already user-approved in
  [decisions](../../decisions/platforms.md#platform-scope). No new durable choice is needed.
- Mechanism recommendations remain proposals until authorized Mac probes;
  absent source or runtime evidence remains explicitly unknown.

## Outcome and evidence

- Revised the plan with nine clean, SHA-pinned sibling checkouts, source
  file/line links and separate FACT/CLAIM/PROPOSAL/UNKNOWN labels.
- Accepted mechanisms: AX observers plus private window-ID join; AX
  size/position/size with enhanced-UI workaround; direct SLPS window focus;
  native-Space move branches and synthetic gesture focus; owned AppKit/SLS
  visuals; per-screen scale; durable preimages and independent restore proof.
- Independent review corrected the first draft's unsupported claims: parking
  was not user-selected, caller-owned order-out did not prove foreign-window
  hiding, in-memory preimages did not prove crash recovery, and yabai `sa.h`
  did not prove exclusive SA authority for every listed operation.
- Further local evidence changed the capability boundary: yabai's current
  non-SA Space moves and focus, plus `skip_window_focus_animation`, have
  upstream SIP-on claims and concrete fallback implementations. Hammerspoon
  creates/removes/selects Spaces via visibly animated Mission Control AX
  automation. Neither route is runtime-proven by this research.
- Lower-level-first probes remain proposals; parking is conditional fallback.
  No new durable product choice was promoted: the tier-2 default is already
  recorded, and host/floor, Intel, stable signer and modifier mapping stay open.
- Risky Space-move/focus, overlay ordering, AX geometry,
  parking, deployment/signing and core entry-point evidence spot-read. Reference trees
  were clean. Static gate: ASCII, 228 local link targets/heading anchors and
  214 source line ranges passed; `git diff --check` passed.
- No live tests, installs or product-code changes.
- Research commit `df018fb` passed all five hosted jobs: Rust, KWin, shell,
  Windows and macOS in
  [CI 37131056780](https://github.com/beefsack/OmniTiler/actions/runs/37131056780).
  The macOS job is toolchain smoke evidence, not native-adapter proof.
- Archived this record after source/static review and hosted CI acceptance.
  All three sequential units completed.

## Handover

- Main risks: private-symbol/version coupling, unproven foreign order-out,
  compat-ID mutation cleanup, gesture click/warp effects, stranded-window
  identity/recovery, TCC/signing survival and true-underlay stacking.
- Proposed backlog text: "P1 macOS port: tier-2 lower-level-first source plan
  revised; SIP enabled, no Dock injection. Nine local implementations mapped
  to Rust adapter seams. Next: Phase 0 host/floor, Intel, signer and Meta inputs,
  then Phase 1 lifecycle/recovery and hotkey capability probes."
- Next product action: collect the four Phase 0 user inputs; no Mac execution
  is authorized by this documentation delivery.
