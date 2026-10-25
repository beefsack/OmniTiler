# world-actions.json: core-only narrow acceptance case

Single-output, tiled-only, zero-gap, per-output-local case
(`tiled-home-away-return`): one output, two known windows, one ordered
admission, select-away, select-previous return. Run with the
development-only example (one mode only, one invocation):

- Windows/macOS: `mise exec -- cargo run --locked -p tiler-protocol --example world_acceptance -- test-fixtures/world-actions.json`
- Linux: `devenv shell --impure -- cargo run --locked -p tiler-protocol --example world_acceptance -- test-fixtures/world-actions.json`

There is no `verify` mode: this runner executes the actual `WorldEngine`
on authored host inputs and checks the narrow accepted result: desired
roster/homing/authorities, actual allocations with work-area coverage,
oracle tiles, effective frames, output views, workspace ring/orders/scopes
(including output affinity and null host-owned bindings), retained view
flights, one-shot effects (id/kind/target plus issued revision),
settlement status, and report/reply freshness and pairing. Presentation
shapes are explicitly out of scope and never read.

## What the checker proves

Six ordered steps, each exactly once, with request/observation/revision
advancing by one (no missing intermediate observations). Symbolic
`home`/`away` aliases bind ONCE from the bootstrap reply ring (home is the
initial view, away is the other ring member) and the ring set plus orders
stay frozen; raw `ws-<n>` ids never appear in this file.

Per step, the executed report binds exactly to its step (request seq,
observation seq, request id, case-built command) and the reply pairs the
report (request id, observation seq, adapter, generation, contract
version; `ready`, no consumed command):

- exact desired roster, every member homed home with `Controlled` frame
  and membership authority, no float/operation tokens or other classes;
- coverage over the ACTUAL reply allocations (positive area, containment
  in the work area, no positive overlap, area sum equals the work area);
- authored tile literals checked separately against actual allocations,
  effective frames checked separately (equal to allocations here);
- outputs show the expected logical view; workspaces/scopes match the
  bound ring with null host-owned bindings; effects match exactly with
  issued revision equal to the reply revision; no command result,
  gesture, settled operations, or cursor warp. The `select-previous`
  return (command seq 12) expects exactly one focus effect (`fx-12`
  targeting `win-a`): returning to `home` re-emits the remembered leaf
  focus per the already-landed approved R-WS-09 rule (with the authored
  empty observed focus, the retained bootstrap focus survives the
  `admit-c` admission). This restates landed core behavior and makes no
  host or native actuation claim.

`settlement: command-pending` steps pin exactly one retained unheld view
flight (`op-<cmdSeq>`, linked to the command); `settlement: settled`
steps must carry zero transfers, so a pending flight can never pass
settled. View transfers settle only through explicit `views` echo
receipts (`away-settled`, `home-settled`): dropping a receipt leaves the
flight outstanding and fails, as does any missing, truncated, or
out-of-order step.

## What is NOT proven (explicit gaps)

- Host visibility is not observed by the core: the away interval only
  proves the LOGICAL view receipt and stable homing/allocations while the
  away view is selected. Nothing here claims native hide/reveal.
- No native verifier exists yet. A future Windows leg needs, at minimum:
  fresh caller/lifetime/output/operation/provenance bindings per run and
  mandatory independent native hide/reveal checkpoints; controlled-scope
  and effect fields must follow the actual trace vocabulary of the
  producer, and no new diag should be added solely to mirror a retired
  draft. No live claim is made until such a leg is proven separately.
- Out of scope (fail as not covered): float/sticky/fullscreen/maximize/
  minimize/size-hint constraints, gaps, host-owned workspaces, gestures,
  cursor/focus-follows/warp, multi-output, migration/sends, presentation.

## Authoring rules (anti-circularity)

- Host fact rects are authored inputs, never previous reply frames. The
  admission newcomer arrives at an arbitrary rect; settled reports carry
  converged tiles. The checker rebuilds the expected command per step and
  requires the executed report's command to equal it.
- Tile literals come from this deterministic split rule on the work area,
  never from engine output: two windows split into equal halves
  (second-listed window takes the left half); the third window splits the
  right half horizontally (top keeps the incumbent, bottom takes the
  newcomer). Coverage is re-proven from actual reply allocations, so
  layout changes fail.
- Unknown logical ids bind via `home`/`away` only; never rebind per step.
