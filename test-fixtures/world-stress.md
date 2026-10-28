# world-stress fixtures: platform-neutral high-volume acceptance

Two single-output, tiled-only, zero-gap, per-output-local stress cases with
independently authored final oracles:

- `world-stress-home-away.json`: two borderless windows, 102 operations,
  16 hide/reveal checkpoints.
- `world-stress-trio-churn.json`: bootstrap pair plus a third-window
  admission, 102 operations, 19 checkpoints covering close/re-admit churn.

Run the development-only core runner (one mode, three schedules):

- Windows/macOS: `mise exec -- cargo run --locked -p tiler-protocol --example world_stress_acceptance -- test-fixtures/world-stress-home-away.json --schedule all`
- Linux: `devenv shell --impure -- cargo run --locked -p tiler-protocol --example world_stress_acceptance -- test-fixtures/world-stress-home-away.json --schedule all`

Focused gates: `mise exec -- cargo test --locked -p tiler-protocol world_stress`
plus `mise exec -- cargo clippy --locked -p tiler-protocol --all-targets -- -D warnings`
and `mise exec -- cargo fmt --all -- --check`.

## What the driver proves

Shared strict schema (`crates/tiler-protocol/src/world_stress.rs`, unknown
fields refuse). Each case pins layout, membership/homing, retained focus,
expected real foreground, shown workspace and pager
workspace/current/occupied expectations at explicit checkpoints and at the
final oracle. Layout tiles are hand-authored from the positional split rule
on the work area, never copied from engine output; coverage is re-proven
from actual reply allocations per homed workspace, so any layout change
fails loudly. Report counts are locked per schedule (duo 186/149/151, trio
190/156/158); reordered strictly exceeds batched, so a degenerate reorder
fails.

## Op reference

Command ops carry `seq` (strictly increasing, at least 100 per case).
Observation ops (`admit`, `close`, `focus-window`, `checkpoint`) carry none.

- `select-workspace` (explicit `output`, `workspace` alias),
  `select-numbered` (`index` 1-9, `output` null means core-resolved),
  `select-relative` (`delta` -1/+1, `output` null means core-resolved),
  `select-previous` (`output` null means core-resolved).
- `focus-directional` (`window`, `direction`, `expect_effect` id,
  `expect_target`). `move-directional` (plus `same_axis`, same effect
  fields; local moves are one-shot and focus the mover). Native legs cover
  the `group-with-neighbor` mode only (the product default); the core
  driver accepts any named mode.
- `send` (`window`, explicit `output`, workspace alias, `follow`).
- `admit` (`window`, arbitrary arrival `rect`). `close` (`window`,
  explicit close of a live window).
- `focus-window` (`window`): explicit foreground acquisition. The named
  window must be live and homed in the currently shown view; a hidden
  window refuses. Carries the focused observation, never a command, and
  never changes retained core focus. The native leg performs the fixture
  focus with readback and aborts when acquisition fails.
- `checkpoint` (`name`, `view`, `members`, `homing`, optional `tiles`,
  `focus`, `foreground`, `pager_current`, `pager_occupied`).

## Foreground discipline

Command reports carry the predicted real foreground, never the command
subject. A subject-carrying command (`send`, `focus-directional`,
`move-directional`) is natively honorable only when its subject already
holds that foreground; anything else refuses at load time as
`does not hold the predicted foreground`. Cases acquire the foreground
only through explicit `focus-window` observations, placed exactly where
genuinely needed (first touch, after a view switch reveals the subject,
after a stay-send hides the mover, after an admission whose native focus
is host-determined). There is no silent refocus before commands and no
fabricated observed foreground.

`focus` is the retained core focus (last one-shot effect target, kept
while its workspace hides). `foreground` is the expected real foreground:
a live window id, or null exactly when no managed fixture window can hold
it (a hidden focus never counts as actual foreground). Stay sends clear
the predicted foreground (the mover hides); follow sends move it to the
mover; selects keep it until the receipt hides its window. Admission
arrives with no foreground.

## Layout rules (read from the core sources, pinned by these cases)

- Bootstrap admits newest-first; selects never reorder.
- Send-home and admits split the anchor leaf (the retained-focus
  resident's leaf when it homes the target, else the first-listed
  resident): the anchor keeps the first half along the long axis, the
  newcomer takes the second.
- Close/send-away removes the leaf; siblings expand along the freed axis,
  preserving survivor order. Inverse move pairs net to zero.
- Lone members take the full area; local moves focus the mover.
- Follow names the mover; stay names the source-MRU survivor (nothing when
  the source empties, keeping retained focus); empty views name nothing;
  view returns follow the remembered/MRU/layout-first candidate order.

Tile literals apply these rules by hand to each checkpoint. A different
observed order fails loudly instead of being copied in.

## Native connector notes (Windows leg)

The Windows connector emits
[exact pager/world proof binding](../docs/changes/archive/platform-neutral-stress-acceptance.md#proof-binding-repair-and-final-acceptance).
Hidden geometry follows the approved retained-frame oracle: exact identity,
hidden visibility and raw retained frame proof, then exact logical tile geometry
on reveal. Both cases pass at 0/10/50ms after the directional CLI rebuilds
origins from a fresh observation before its early foreground lookup. This
adds no settlement waits and preserves downstream lifetime/scope/focus gates.

Before case actions, the connector verifies authored `initial_facts` against
settled world proof and fresh identity-bound native frames. If the two-window
bootstrap is exactly pairwise-swapped, one existing exact-owner directional
move establishes the authored layout and a strict setup checkpoint verifies
it. Exact initial layout needs no setup command; other shapes refuse. Setup
is recorded separately from the 102 commands and their publication timings;
expected tiles are never copied from the observed layout.

Each action maps 1:1 to the existing exact-owner CLI controls:
`select-workspace`/`select-numbered` to `workspace --select INDEX`
(home 1, away 2), `select-relative` to `workspace --relative
previous|next`, `select-previous` to `workspace --previous`, `send` with
follow/stay to `workspace --send/--stay INDEX`, `focus-directional` to
`directional --focus`, `move-directional` to `directional --move`;
`admit`/`close` map to fixture spawn/close, `focus-window` to fixture
focus with readback. The subject of every subject-carrying control is the
live foreground; the expected subject is an oracle, not permission to repair
foreground or wait for convergence before the next burst command.
Checkpoint ops are the mandatory native hide/reveal evidence
points: join final native frames/visibility/foreground to fixture caller
identities (ignoring IME auxiliaries) and compare membership/view plus the
actual pager renderer payload separately. Normalize tile literals to the
discovered single-output work area with `normalize_tiles_to_work`
(zero-gap halves must scale integrally). No new product IPC; the
core-only paint lists stay empty here by design. Native waits only on
transport (single-pending request-file availability) between burst
commands; per-command dispatch/production outcomes are joined from the
owner log, and full native convergence is awaited only at defined
checkpoints. Delays are configurable including zero; zero means no added
pacing.

Shared APIs for the native runner: `load_stress_case`, `run_stress`,
`StressSchedule::{Ordered,Batched,Reordered}`, `StressCase::name`,
`normalize_tiles_to_work`.

## Keyboard coverage (Windows, offline only)

```powershell
pwsh -NoProfile -File scripts/windows-world-stress.ps1 -KeyboardCoverage
# Optional: -Case test-fixtures/world-stress-home-away.json
pwsh -NoProfile -File scripts/windows-world-stress-check.ps1
pwsh -NoProfile -File scripts/windows-world-stress.mock.ps1
```

The coverage report reads the same cases and validates every action. It starts
no owner or fixture, changes no settings and sends no input. JSON contains one
row per action, catalog binding ids, default chord candidates and per-case
command/unbound counts. `status: pass` means coverage analysis succeeded, not
that keyboard execution or a final-world oracle passed. Home=1/away=2 is an
authored coverage assumption, not live ring resolution or active settings.
Actual aliases must follow the observed ring; shortcuts are configurable.

| Action | Default candidate / route | Commands per case |
| --- | --- | --- |
| `focus-directional` | Win+H/J/K/L for left/down/up/right | 48 |
| `move-directional` | Win+Shift+H/J/K/L | 2 |
| `select-workspace` | Win+N after dynamic alias resolution | 14 |
| `select-numbered` | Win+N | 12 |
| `select-relative` | Win+Ctrl+H previous, Win+Ctrl+J next | 12 |
| `select-previous` | Win+Ctrl+Tab | 10 |
| `send`, follow | Win+Shift+N after dynamic alias resolution | 2 |
| `send`, stay | `stay-workspace-N`, configurable but unbound by default | 2 |
| `admit` / `close` / `focus-window` | Fixture spawn / close / verified focus, not shortcuts | Not commands |
| `checkpoint` | Existing shared oracle observation, not a shortcut | Not commands |

Both cases have 102 commands, 100 with default bindings and two unbound stay
sends. Home-away has 148 total actions; trio-churn has 155 including churn.
Bindings come from `crates/tiler-windows/src/settings/catalog.rs` (focus/move,
history, stay rows and `workspace_row`); arrow alternatives exist but the report
chooses one letter candidate. Relative selection is not relative send.

### Injected-input blocker and baseline contract

The normal product hook intentionally passes injected keys through without
dispatching shortcuts (`snapkey/sys/mod.rs`, `snapkey/classifier.rs` under
`crates/tiler-windows/src/`); the separate shortcut-proof marker is not product
acceptance. `tests/shortcut_proof.rs::product_never_accepts_injected` locks this
boundary. The [live guide](../docs/live-windows-testing.md#agent-live-test-practice)
forbids weakening the filter. OS-owned chords can still execute outside the
tiler: do not test the refusal with injected Win chords, never send Win+D and
never use a proof owner as a substitute stress target.

Thus `injection_accepted: false` and `blocked_reason:
normal-product-injected-filter` are source findings, not live probe results.
No keyboard mode is executable yet. The remaining choice is a user-driven
physical shortcut baseline or deferral until a separately reviewed route
preserves the input boundary; a full run also needs explicit stay bindings.
No batch-file/stdin product CLI or coalescing extension is introduced.

Once unblocked, retain the same final-world oracle and fixture churn, record
the source commit containing the staged switch, and measure publication bursts
above the CLI baseline (about 3.4 actions/s overall, 101 ms contiguous gap).
Use the accepted [practical BEFORE phase method](../docs/changes/windows-workspace-return-focus.md#practical-phase-measurement-decision-2026-10-28):
command receipt, all shows issued, arrivals visible, foreground confirmed,
departures hidden, and E2E to the last applicable phase. Report missing proof as
unavailable and empty-arrival phases as N/A, not successes. Record observed
intermediate worlds/superseded renders and queue depth only where traced; do
not infer visible jank from queue acceptance. Live execution still requires
all preflight, fixture-only scope, independent hide/restore proof, process
provenance, foreground guards for every input batch, and recovery in the live
guide. A pre-existing user owner blocks the live leg; do not stop it.
