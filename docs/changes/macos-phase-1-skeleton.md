# macOS Phase 1 threading/lifecycle skeleton

Delivered offline: one shared-core engine owner, dedicated exact-chord input
boundary, per-app AX observer/actuation owners, typed bounded handoffs,
generation/sequence fences, bounded stop/reap and structured logs.

Evidence: 135 unit, 9 bounded and 39 runtime integration tests pass; macOS
build, all-target strict clippy and formatting pass. Linux/Windows Rust gates
and dependency license/notice gates also pass in CI. Live capability acceptance
continues separately in [Phase 0](macos-phase-0.md).

The completed goal, approach, review evidence and limits are in the
[archived change record](archive/macos-phase-1-skeleton.md). Current port
contract: [macOS port notes](../porting/macos-port-notes.md).
