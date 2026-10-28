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
