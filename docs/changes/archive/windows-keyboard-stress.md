# Windows keyboard stress prerequisites

## Goal and scope

Prepare keyboard dispatch stress using the existing world-stress definitions
and checkpoint/final-world oracles, without changing product input acceptance,
coalescing, command contracts or the public CLI. The requested native baseline
must measure a revision containing the staged switch, above the existing CLI
publication rate, with final-world verdicts, end-to-end/per-phase latency and
render/queue observations using the existing BEFORE trace method.

## Finding and bounded delivery

- The normal hook rejects injected input before shortcut classification
  (`crates/tiler-windows/src/snapkey/sys/mod.rs`); the proof-marker exception is
  limited to the shortcut-proof owner. The live guide explicitly forbids
  weakening the filter. A synthetic product burst cannot be measured safely
  by switching to the proof owner or falling back to OS-owned chords.
- Stay sends are bindable but unbound by default. All remaining command
  families have configurable default shortcuts; fixture spawn/close/focus and
  checkpoint observations are not keyboard commands.
- Deliver an offline shortcut-coverage entry point in the existing runner,
  sharing its case validation and fixtures, plus fault-injection tests and
  point-of-use documentation. No injection implementation or live effects.

## Acceptance and units

1. Inspect source input gates and map every action in both shared cases,
   distinguishing default bindings, unbound actions and fixture observations.
2. Add pure per-action coverage output, dynamic alias handling and explicit
   injected-input blocking. Offline output must never launch an owner, fixture,
   build, change settings or inject input; existing CLI behavior stays intact.
3. Run runner parser/seam and mock checks, and `git diff --check`. Obtain
   independent input-safety/scope review before committing.

## Parked question

Choose a user-driven physical shortcut baseline, or defer keyboard-burst data
until a separately reviewed input route can preserve the injected-input safety
boundary. Stay-send bindings also need an explicit test configuration before a
full shortcut run. No batch-file/stdin product CLI or filter exception is chosen.

## Evidence and outcome (2026-10-28)

- Investigation found 102 commands per case, including two unbound stay sends
  per case; source inspection establishes the injection block without a live
  probe. The existing offline mock passed before changes.
- Keyboard baseline, per-phase latency and jank data remain unmeasured, not
  passing or failing. Existing CLI measurements are not keyboard evidence.
- Source inspected at `4dc89fb3` on main, which contains staged-switch prototype
  `50f910b4`. This identifies the inspection, not a measured keyboard build.
- Delivered `windows-world-stress.ps1 -KeyboardCoverage`, offline JSON output
  over either selected case or both shared cases. It labels authored alias
  assumptions, catalog defaults versus active settings, unbound stay sends,
  fixture/observation rows, and the product injected-input block. Existing CLI
  publication, oracles, product hook and coalescing are unchanged.

| Case | Actions | Commands with default bindings | Unbound stay sends | Keyboard final-world / latency / jank |
| --- | --- | --- | --- | --- |
| home-away | 148 | 100 | 2 | Not run: intentional injected-input filter |
| trio-churn | 155 | 100 | 2 | Not run: intentional injected-input filter |

- Independent review found no blocking input-safety or scope issue. Its two
  low-severity findings were resolved: coverage mapping assertion counts now
  reflect the tests, and direct coverage mapping rejects non-integer indices,
  deltas and alias ordinals instead of rounding them. Unknown aliases also
  refuse even if a caller supplies a matching dictionary entry. Existing CLI
  validation remains unchanged.
- Offline parser/seam check passes with the new pre-effect conflict gate and
  existing forbidden-input scan. The mock passes default mappings, both shared
  cases, malformed-input refusals, effect-shadowed coverage, subprocess entry
  and six conflicting-flag refusals. Direct coverage output confirms 102
  commands and two unbound sends per case. `git diff --check` passes.
- No Rust changes, new dependencies, native launch, injected input, settings
  mutation or coalescing changes. No new product decision is made. The bounded
  prerequisite delivery is complete; keyboard baseline work remains blocked on
  the parked question above.

## Backlog detail moved 2026-11-01

### Original backlog lines 452-480

````markdown
- P1 | Keyboard stress runner | Offline `-KeyboardCoverage` map delivered
  (`6c850ae0`, [outcome](changes/archive/windows-keyboard-stress.md)): 100 of
  102 commands per case are default-bound; the two stay-sends are unbound by
  default. The product hook rejects injected shortcuts. User 2026-10-29:
  (a) run a user-driven physical shortcut burst baseline after the staged
  switch lands, with trace and a short run sheet; and (b) add an opt-in startup
  flag so the normal owner accepts synthetic shortcuts for testing. Default
  behavior unchanged; flag only with fixture scope, logged in trace. Key safety
  requirement: no injected key may reach a non-fixture window, especially an
  agent terminal (a stray Esc can cancel agent sessions). The driver injects
  only bound chords the owner swallows, verifies a fixture foreground and an
  active accepting hook immediately before each injection, never injects Esc or
  unbound keys, and stops on any foreground change. Independent input-safety
  review before live use.
  Raised from P2 2026-10-28: first deliver the
  current-order keyboard-burst baseline, which supplies the data for wider
  coalescing. User 2026-10-27: share definitions and oracles
  with the existing deterministic-correctness CLI mode; use injected real
  shortcuts through the hook path for real-input bursts and the test window tool
  for churn. First check injected-shortcut acceptance and shortcut coverage for
  every stress action. CLI rate accepted for now: 3.4 actions/s overall, 101ms
  between contiguous commands; about 100ms/action already exceeds typical human
  input speed. No batch file/stdin CLI planned; possible follow-up only if
  keyboard mode cannot cover a case, requiring user approval as a new public CLI
  feature ([decision](./decisions/world-rendering.md#stress-acceptance)).
  User 2026-10-28: after the staged switch, measure real keyboard-burst jank
  before extending render-latest-only coalescing beyond selections. Goal:
  coalesce fully and rapidly to any latest world state; command-contract changes
  per family follow the data ([decision](./decisions/windows.md#windows-decision-set)).
````
