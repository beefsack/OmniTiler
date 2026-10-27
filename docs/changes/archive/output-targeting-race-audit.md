# Output-targeting race audit (2026-10-28)

- Outcome: option A implemented offline. Current production callers already
  satisfy it; delivery adds race regressions and closes the stale audit without
  changing production behavior. Independent review found no blocking issues.

## Goal and scope

- Implement the approved 2026-10-26 option A: implicit selection/send/move
  commands defer output resolution to core execution; genuine explicit output
  choices remain explicit. Prove unchanged per-platform behavior offline.
- Scope: KDE, Windows and macOS command producers, shared-core race regressions
  and adapter contract pins. No live acceptance claim or new command semantics.
- Non-goals: explicit-send current-workspace sampling, float-toggle geometry,
  macOS local-only directional policy and unrelated port work.

## Approach and acceptance

- Current-source audit finds implicit selectors already pass no output; sends
  use implicit source intent; directional commands already defer resolution.
  The N6 audit table predates these routes and needs a dated correction.
- Tentative technical choice: retain these production routes without churn;
  close the approved audit through execution-interleaving regression coverage
  and per-platform producer pins, rather than invent a redundant API change.
- Add regression tests for cursor/focus changing after intent construction,
  queued implicit commands using execution-time retained state, and explicit
  outputs remaining honored. Demonstrate discriminating failure by temporary
  target-preselection mutation, then restore the production route.
- Audit platform fallbacks without standardizing them. In particular, retain
  send-home precedence and existing action-versus-cached-live fallbacks.
- Independent review and full relevant KDE-package offline gates precede landing.

## Bounded units

1. Read-only cross-platform audit: complete; no required production edits found.
2. Core race regressions and verification of existing platform producer pins:
   complete.
3. Independent review, full offline gates and documentation reconciliation:
   complete.

## Parked boundary

- Windows sticky-rehome uses explicit `Send` with a sampled current workspace.
  Keep the existing explicit-current semantics in this slice. Recommendation:
  evaluate a separately approved execution-time current-view intent, not an
  optional-output change to the explicit `Send` contract.
- macOS local Move/Focus remain local-only; changing that is visible behavior,
  not output-preselection cleanup. No production macOS change is proposed.

## Evidence

- Two regressions in `world_logical_contract_tests.rs` construct intents before
  changed observations: queued numbered/previous/relative selections resolve
  cursor targeting and retained history sequentially; focused-mode selection
  follows changed focus while explicit output survives cursor/focus disagreement.
- Focused evidence: all 42 logical-contract tests pass. Intentional
  first-output pinning makes both new tests fail their output assertions; the
  restored resolver passes. This is mutation discrimination, not a claim that
  unchanged production was initially broken. An earlier redundant send fixture
  rejected absent focus and was removed; existing send coverage is retained.
- Existing `implicit_send_source_freezes_pre_observation_home` covers retained
  send-home precedence, same-report rehoming refusal and distinct fallback
  routes. Core freezes the retained home at accepted-report execution before
  staging, not at adapter intent construction; this contract is unchanged.

## Per-platform audit

| Platform | Current implicit route | Unchanged boundary |
| --- | --- | --- |
| KDE | `world-shortcuts.ts` and `world-entry.ts` pass `null` to numbered/relative/previous/trailing selection; numbered/relative sends use `home-then-cached-live`; movement/output send/migration are direction-only | Existing producer/wire and directional-intent tests pin these routes; no production edits |
| Windows | `workspace_navigation.rs`, `cli_workspace.rs`, `history.rs` pass `None`; keyboard sends use `HomeThenCachedLive`, CLI sends `HomeThenAction`; directional routes defer targets | CLI/action fallback can consult pointer for an unhomed mover; keyboard cached-live fallback cannot. Existing cursor-targeting tests and native-source inspection cover their respective scopes; no production edits |
| macOS | `runtime_thread.rs` passes `None` for ordinary taps and uses `send_command_implicit`; `world_command_map.rs` passes through genuine explicit indicator output | Existing mapping tests distinguish `None`/`Some` and implicit sends. Indicator focused-home/current-domain fallback and local-only Move/Focus remain unchanged; no production edits |

- The N6 audit table's Windows-navigation/macOS preselection and macOS explicit
  production-send descriptions are superseded by these current-source findings.
- No visible behavior difference is introduced on any platform: only tests and
  records change. This establishes source-level parity, not new physical/native
  acceptance. Windows-native-only paths cannot execute on Linux; macOS native
  behavior is likewise not physically tested here.

## Final offline gates

- Through the declared development environment: locked all-target build, tests
  and strict all-target clippy (`-- -D warnings`) for `tiler-core`,
  `tiler-protocol`, `tiler-windows` and `omnitiler` pass. Tests: 2,540 passed,
  zero failures; logical-contract suite 42/42.
- `cargo fmt --all -- --check` and `git diff --check` pass. Exact
  `world_acceptance` over `test-fixtures/world-actions.json` passes 6/6.
- KWin `typecheck`, `build` and full `test` pass: 765/765, zero failures.
- macOS locked all-target check and `--lib world` tests pass: 86/86. Existing
  non-native dead-code warnings remain; no macOS strict-clippy or full native
  library acceptance claim.
- Independent source/diff review confirms unchanged production, no residual
  mutation, no missed implicit producer preselection and adequate execution-time
  cursor/focus, retained-history and explicit-output coverage. `SelectTrailing`
  shares the reviewed resolver but is not a new variant-specific regression.
- Exact next action for this audit: none. Explicit-current sends and native
  platform acceptance remain separately scoped as above.
