# Offline review rectification

Correct the macOS test and dead-helper regressions, repair retained float placement across domain sweeps, and reconcile review findings and confirmed decisions. Native verification, the Windows port, and file splits are outside this change.

- Restore macOS report fixtures and canonicalize only the float-intent test root.
- Preserve held/floating-domain invariants during float release and cover both regressions. Determine whether native same-output crossings can reach the empty-workspace sweep.
- Retire uncalled startup holder and host-derived border-radius helpers (keep the shared radius 6 and the startup/no-mutation and radius regressions). Update TD1, TD4, shared first-float, and macOS indicator artwork records; scrub the eight review-identified records.

Acceptance: demonstrate the two core regressions on the old code; check formatting, core and protocol suites, macOS lib and integration suites, float-intent tests under the default temporary directory, and strict all-target clippy for the four affected packages.

KWin v6.7.4 supports changing desktops on the same output during a native
move: `Edge::handle` dispatches to `Edge::switchDesktop` while a non-resize
drag is active (`src/screenedge.cpp:368-372,511`), and KWin moves the
dragged window to that desktop (`src/workspace.cpp:1072-1074`). The KDE
adapter reports single-desktop membership as native workspace truth
(`kwin/src/world-observe.ts:1204-1208`). The empty-destination adoption rule
therefore needs to distinguish full domain keys, not just outputs.

Before the fix, the release regression found a layout incorrectly created
for the floating destination; the held-source regression found the
source's durable rectangle missing while its exception remained; and the
same-output regression found no placement at the destination.

The rehome retires both source records only once the fresh destination
admits the mover. Scope holds prevent inferring departures from missing
rows; they do not undo positive homing evidence for a live, unheld window
(`world_members.rs`, `reconcile_members`). Retiring only the proven
mover's exception and durable placement preserves the held source's
other windows and avoids depending on whether or when that scope sweeps.

A deferred variant was rejected in review: the destination could run
before a source that later held on inset exhaustion or reconciliation
failure, so staged readability could not prove the source had swept.

Offline verification: formatting passed. The locked suites passed
2,621 core tests, 394 protocol tests, 1,719 macOS library tests (3 ignored),
and 152 macOS integration tests, with no failures. The macOS integration
fixture needed a neutral `topmost` field in addition to the neutral
pointer fields in the library fixture. No native checks were run.
The locked `omnitiler` tests passed 19 gesture transport, 3 sequence
equivalence, 83 service flow, and 23 filtered float-intent store tests
with the default `TMPDIR`.
Strict all-target Clippy reported only nine pre-existing `dead_code`
diagnostics on this host, all in `crates/tiler-windows/src/log_sink.rs`
from `bf588317` and tracked in the Windows backlog.

The six-route sequence equivalence test covers both an explicit float
rectangle that changes the modeled native frame and a rectangle-free
toggle that preserves the visible frame. Both retain the comparison of
applied and omitted intermediate writes.
