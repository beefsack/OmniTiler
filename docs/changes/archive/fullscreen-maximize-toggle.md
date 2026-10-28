# Fullscreen maximize toggle

## Goal and scope

- User 2026-10-28 selects Hyprland R-MAX-11: explicit maximize toggle
  replaces fullscreen with maximized. Shared core policy and KDE mechanism
  only; Windows/macOS adapter integration remains separate.
- Tentative pending review: explicit target commands authorize the necessary
  native state writes under D5; retain tiled/floating base membership;
  both fullscreen+maximized flags become maximized, not fullscreen.
- Acceptance: retained tile slot and unchanged sibling allocations, second
  toggle restoring the base, floating base preserved, native setter ordering
  justified by KWin source, no unsolicited fullscreen writes.

## Approach and decisions

- Unlike float, maximize lacked a core command. Tentative integration choice
  pending user review: both ordinary and fullscreen Meta+M now submit
  `WorldCommand::ToggleMaximize`; Session decides the target and a command-only
  `CommandResult::Maximize` authorizes KDE's native setters. No lifecycle,
  topology or retained-operation changes. Production delta is a few hundred
  lines across the existing command/protocol and KDE routing layers.
- Additive wire command `{op:"toggle-maximize",win}`; result
  `{op:"maximize",win,maximized,clearFullscreen,wasMaximized}`. Source flags
  fence stale application. Review this shape with the coordinated protocol
  bump for unreadable-desktop workspace ownership and Windows staged switches;
  no protocol version bump here.
- Core requires focused known target, readable non-held scope and no pending
  same-window transfer. KDE uses the existing requester-only take correlation,
  fresh live identity/focus/source flags and apply-busy fences. Commandless and
  consumed commands cannot authorize new setters. Ordinary results call only
  `setMaximize(bool,bool)`; existing maximize signal/admission fences remain.
- KWin v6.7.4 `src/xdgshellwindow.cpp:1737-1880` and v6.8.1's corresponding
  setters synchronously change requested fullscreen before maximize checks
  resizability; `isResizable` reads requested fullscreen. `doSetFullScreen` /
  `doSetMaximized` stage configure flags, while applied script flags update on
  client acknowledgment. KDE therefore clears `fullScreen` then invokes
  `setMaximize` back-to-back, never gating the second setter on applied-full
  readback. Final readback distinguishes confirmed/unconfirmed; no retry or
  delay. Legacy native maximize fullscreen refusal remains intact.
- Required compatibility edits: exactly two compile-only diagnostic-label
  match arms in Windows `tiling_sys/world_tick.rs`, not runtime integration.
  macOS needs no exhaustive-match edit and its adapter remains unchanged.
- Tentative behavior details remain pending review, not parked implementation
  blockers: explicit-target D5 exception, retained tiled/floating base and
  both-flags -> maximized. Reference variant outcomes not source-established
  remain TBD. No automatic fullscreen write permission added.

## Verification and outcome

- Delivered offline; native/physical acceptance remains pending, user-owned.
  No live testing performed.
- Real Engine command regression covers tiled fullscreen/ordinary/both-flags
  decisions, exact retained subject/sibling allocations, floating fullscreen
  durable placement, command consumption, held/focus/transfer refusals and
  byte-exact command/result sizing. Replaces an observation-only probe.
- KDE entry fixtures use QML array-likes, real-property names, renewed wrappers,
  braced live ids and Qt size-hint sentinels. Seven new entry cases cover
  clear/maximize order, second restore, floating base, both flags, ordinary /
  commandless safety, stale focus/preimage, duplicate take and applied-state lag.
  Replies are fixture-authored; core policy is verified separately, not an
  end-to-end host proof. Ten new native cases and result-parser coverage also
  pass. Independent production diff review found no blocking correctness issue.
- Sequential gates: `cargo test -p tiler-core -p tiler-windows`: 2,194 passed;
  `cargo test -p tiler-protocol`: 64 passed, including wire decode, malformed
  target refusal, canonical result encoding and sizing coverage;
  `npm test --prefix kwin`: 812 passed; strict workspace clippy excluding
  macOS, KDE typecheck, Rust formatting, `just check-portable` and diff checks
  pass. Rust commands run through `devenv shell --impure --`.
- `cargo check -p tiler-macos` succeeds with 25 existing unused/dead-code
  warnings. Strict macOS clippy has the known 26 library / 16 library-test
  unused/dead-code/needless-return errors, no enum exhaustiveness error.
  The known nine Linux dock-swipe test failures were not rerun or changed.
- Follow-ups: native acceptance and Windows/macOS adoption of the shared
  maximize route are tracked in the backlog; no additional harness introduced.
