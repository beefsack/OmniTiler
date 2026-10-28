# Cross-output edge focus

## Goal and scope

- Directional output crossing selects destination tiled leaves touching the
  entry edge in canonical projected allocation, including nested leaves.
- Rank eligible edge candidates by destination workspace focus history; absent
  history, use largest perpendicular overlap, then DFS layout order.
- Keep local navigation, workspace-return memory, output adjacency/reciprocity,
  capability, observation, projection and source-maximize fences unchanged.
- No adapter implementation changes, float-origin crossing or macOS hotkey
  integration. Physical acceptance remains pending.

## Decisions

- D1/D2 selected 2026-10-28: edge candidates, history first, overlap and layout
  order fallback; use canonical allocations, not observed frames.
- Tentative D3: no-focus origins use the same selector with source usable-area
  span, retaining four-direction crossing and no local recovery.
- Tentative D4: fullscreen/maximized members rank by retained tiled allocation;
  existing eligibility fences remain; floats stay local-only and empty or
  ineligible destinations retain state.
- Tentative D5: macOS hotkeys with crossing disabled remain out of scope.

## Units and acceptance

1. Add failing-first core regressions, then implement the shared selector and
   justify its focus-history source.
2. Reconcile policy, reference outcomes and adapter documentation.
3. Independent diff review and resolve concrete findings.
4. Run core/protocol/Windows tests and clippy, workspace fmt and diff checks;
   archive this note after accepted evidence. Optional live fixture evidence
   must follow the Windows live protocol; it is not physical acceptance.

Required regressions: horizontal edge discriminator, stacked history winner,
no-history overlap/order, nesting, both horizontal and vertical crossing,
no-focus origins, fullscreen/maximized candidates, empty destination no-op and
unchanged float-origin behavior.

## Evidence and outcome

- Shared-core selector implemented for tiled and no-focus origins. History
  uses the domain's last-active tiled leaf, then its MRU stack newest-first;
  only linked/homed edge candidates qualify. Stale last-active leaves cannot
  win. Existing admission/convergence seeding remains history, so true
  no-history overlap/DFS behavior is covered at the pure selector boundary.
- Failing-first Session suite: seven failures and four passes before the
  selector change. Failures picked the non-edge remembered leaf or refused
  converged target tiles without a remembered leaf.
- Current coverage: 24 edge tests plus three added no-focus tests, including
  left/right, stacked history, nested layout, overlap/DFS, all four entry
  edges, global offsets, stale history, fullscreen/maximized retained links,
  empty/ineligible no-op and unchanged float-cross refusal precedence.
  No-focus regressions include vertical crossing; focused vertical crossing
  remains unsupported. Existing no-focus journey now expects the actual
  horizontal fixture's edge tile, not its non-edge remembered tile.
- Independent review found stale integration expectations, a test-only import,
  formatting and inaccurate history comments; corrected. The combined-fence
  float refusal order is restored and regression-tested. Tiled capability
  validation remains before geometry-dependent ranking; every safety fence
  remains intact.
- Required tests for core/protocol/Windows, all-target strict clippy, workspace
  fmt check and diff whitespace check pass after corrections and pass again
  in final independent review. No blocking review findings remain.
- No live leg performed; native/physical acceptance remains pending. No
  adapter implementation changed. Shared adapter guide and macOS port notes
  identify the policy change and disabled macOS hotkey crossing.
- D3-D5 remain tentative pending user review. No further product question
  blocks this bounded core change.
