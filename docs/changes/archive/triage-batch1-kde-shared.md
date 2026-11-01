# 0.1 triage batch 1: KDE/shared offline verification

## Goal and scope

- Prove approved D05 / REQ-INS-05: admission while an ordinary float is
  focused uses prior tiled focus B as the ordinary long-edge anchor.
- Prove D07 / REQ-CLOSE-02: close then reopen receives fresh admission,
  without introducing restore policy.
- Add discriminating shared fixtures and KDE adapter coverage; update reference
  outcomes, durable decisions and the requested backlog status.
- Native live acceptance is pending and user-owned. No live testing, unrelated
  triage implementation, or Windows-specific behavior changes.

## Acceptance and approach

- Verify R-INS-05 on the existing shared Engine convergence route and the KDE
  PlanAdapter wire boundary. Admission policy remains unchanged.
- R-INS-05 must distinguish insertion beside B from root wrapping; R-CLOSE-02
  must distinguish fresh admission from restoring the closed slot.
- Preserve portable Windows compatibility and existing admission behavior.
- Run workspace tests, strict clippy/fmt, portable checks and Windows portable
  allowlist, KWin JS tests/typecheck/bundle, and native CTest if native code changes.

## Work units

1. Investigate and verify shared/KDE behavior, fixtures and spec
   reference cells; report exact commands, counts and material discoveries.
2. Inspect evidence and diff, update project records and archive this note.

## Coordination and decisions

- Started from clean `fa6983a`; initial `git pull --rebase` was up to date.
- User approved D05 and D07 on 2026-10-10; product decisions require a stop and report.
- 2026-10-11: D05 verification-only for simplicity: the
  approved behavior already holds via the Engine path. Revert the candidate
  remembered-tile fallback and its dependent seed/admission fixtures.

## Evidence and outcome

- Source tracing and the pre-fix probe show retained float-focus admission
  already anchors at B: Engine
  forwards native F to convergence (`engine.rs:695-714`), but Session admission
  starts with retained tiled focus B (`session/world.rs:1058-1080`). Float
  focus does not clear that tiled focus. The backlog's production-gap premise
  is not reproduced.
- The discarded repair changed direct Session propose/seed admission with
  off-domain/unset focus and valid tiled history, rather than the requested
  production float-focus journey. All production source edits and the two
  dependent fixtures were reverted after that decision.
- Independent review rejected the original off-domain-only fixture as proof
  of float-focus behavior and caught an incorrect Horizontal axis assertion.
  Corrected fixtures use B's tall 60x80 target (Vertical) and add actual native
  F convergence characterization. Review also identified an unsupported
  survivor-ratio claim; that leg is now TBD. Unneeded convergence fallback
  changes were removed rather than broadening close/open semantics.
- Three shared fixtures now prove retained float-focus long-edge geometry
  (`H[A,V[B,C]]` in a 120x80 domain, exact A/B/C frames), genuine multi-leaf
  no-anchor root-wrap, and D07 close/refocus/fresh-reopen without old-slot
  restoration. The mocked KDE PlanAdapter fixture proves F/C wire routing.
  Native observation, activation and ratio acceptance remain user-owned live
  checks; no policy or adapter production changes are shipped.
- Final gates: Rust workspace 1327 passed; explicit CI Windows
  portable package allowlist on Linux 1175 passed; KWin 1308 passed across 188
  suites. Workspace/allowlist strict clippy, fmt, portable dependency/leak
  check, allowlist build, KWin typecheck/bundle and diff whitespace pass.
  Logs: `d05-cargo-test.log`,
  `d05r-win-test.log`, `d05-kwin-test.log`.
- No native source changed; no live testing. Existing stashes remain untouched.
  Offline scope is complete; pending native acceptance stays in the backlog.

## Backlog detail moved 2026-11-01

### Original backlog lines 1312-1344

````markdown
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
  - 2026-10-18 single-output attempt: blocked before any D-unit journey; no
    acceptance advanced. Owned-desktop selection refused, later selection
    summaries lacked canonical wire evidence, and an empty backing desktop
    retired. Original desktop identity/name was not restored; runtime float-intent
    preimage was missed by the helper. Preserve both pending disposition, repair
    capture, then discriminate fixture mapping from adoption/view application
    before any product fix. Options and producer-shaped outline:
    [native triage record](changes/kde-triage-live.md#blockers-and-next-discriminator).
  - Windows: D05 shares the proven Engine route; D14's keyboard trigger gap is already
    handoff item 6 (include R-RSZ-02/03 acceptance there). D02's owned-dialog
    exclusion is the approved divergence; D17 remembered focus and D19 hide/
    reveal are already wired. No new Windows-specific handoff item warranted.
````
