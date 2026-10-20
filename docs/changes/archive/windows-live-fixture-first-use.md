# Windows live fixture first use

## Goal and scope

Prove the live fixture's Windows transport and native window controls on both
outputs, then use fixture-only scope for an integrated tiler smoke. Fix bounded
fixture defects and document trustworthy readbacks and host limitations.
Non-goals: physical shortcut acceptance, synthetic drag, foreign application
acceptance, exclusive display-mode changes, and core architecture changes.

## Acceptance

- Multiple caller-identified windows spawn, query, change without recreation,
  and close on both outputs. Cover decorations, resizability, sizes and size
  limits, windowed/maximized/minimized and borderless fullscreen modes.
- Native identity, geometry, styles and visibility independently corroborate
  fixture observations; unsupported controls are reported honestly.
- A fixture-only tiler run admits and tiles fixture windows without changing
  protected windows. Exact-owner controls exercise directional focus/move,
  cross-output sends and workspace switching on both outputs.
- Owned windows/processes stop, settings and protected geometry remain at
  baseline, and recovery/control residue is clear.
- Touched-crate tests, strict clippy and formatting pass; published CI status
  distinguishes fixture changes from unrelated failures.

## Approach and bounded units

1. Inspect entry points and existing helpers; build and run standalone native
   fixture coverage with independent observations and bounded cleanup.
2. Resolve reproduced fixture defects with regression tests, then repeat the
   affected live checks before accepting the fixture contract.
3. Run fixture-only integrated tiling checks using control-plane requests.
4. Independently review live-behavior changes, run native gates, and summarize
   accepted evidence in point-of-use documentation.

## Verification and outcome

- Tested source `da24770b` on native Windows 11 with two differently scaled
  outputs. Raw process/artifact identities, topology, scripts, replies and
  native readbacks are retained outside the repository.
- Standalone: multi-window operations, ordered batches, same-connection
  subscriptions, runtime decorations/resizability/size/min/max and native
  modes passed on both outputs without HWND recreation. Native track-limit
  readback corroborated size hints; programmatic resize bypasses those hints.
  Visibility/focus matched native observations. Compact resize waits matched,
  and an intentional mismatch exited nonzero at its deadline.
- Integrated: four tiled fixture windows, directional focus/move on both
  outputs, cross-output send/follow/back and workspace send/select roundtrips
  passed. Scope arguments, the managed-token/frame/HWND join and protected
  geometry comparisons corroborated fixture-only admission.
- Graceful shutdown and exact-process forced termination passed. Scoped owner
  stop/restore released hooks; fixture/owner processes, ledger and pending
  control requests were absent afterward. Protected geometry and settings
  remained unchanged.
- A suspected fullscreen scaling anomaly was an observation error: unaware
  metrics were compared with physical frames. Independent PMv2 monitor data
  and display-mode readback resolved it; no fixture or host defect remained.
- Fixture tests: 119 passed; strict all-target clippy and workspace fmt passed.
  No fixture or tiler code change was needed.
- Baseline CI [39419800653](https://github.com/beefsack/OmniTiler/actions/runs/39419800653)
  passed native fixture gates but remained red in Linux Rust and Windows
  portable builds: `tiler-macos` references `AX_ATTRIBUTE_UNSUPPORTED` and
  `take_trailing_proof` unavailable on non-macOS. Native macOS CI passed.
  These unrelated portability defects remain separate work.

## Material decisions and remaining limits

- Keep existing hint/readback semantics. Document native behavior instead of
  adding resize clamps or claiming fullscreen mode from configured intent.
- Stage exact physical sizes after cross-DPI placement. Use independent PMv2
  HWND observations for client/outer/DWM comparisons and managed-roster joins.
- The fixture already supplies multiple windows, caller IDs, batches and
  observed waits needed for layout staging. Native frame joins still require
  an external observer; no structured outer-frame field or layout-history
  protocol was added. Size constraints cannot currently be cleared through
  the protocol. Exclusive display modes and physical shortcut/drag acceptance
  remain outside this evidence.
- Documentation now carries accepted Windows coverage and point-of-use
  limits. Independent review corroborated the native evidence and cleanup;
  no behavioral defects or policy changes were needed. The Windows portion
  of fixture first-use acceptance is complete.
