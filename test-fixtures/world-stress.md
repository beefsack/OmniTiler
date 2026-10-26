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
189/155/157); reordered strictly exceeds batched, so a degenerate reorder
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

The Windows connector is mechanically parked pending the
[hidden-frame oracle decision](../docs/changes/platform-neutral-stress-acceptance.md#blocking-decision-and-next-acceptance-step).
Delay-zero native testing repaired one adapter bug but has no complete pass;
nonzero delays and trio-churn are unrun. The following describes the connector
mapping, not native acceptance.

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
