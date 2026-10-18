# KDE rapid input and geometry settlement

## Goal and scope

Resolve production world-adapter directional input loss and geometry stalls,
including repeated same-output moves and occupied-target R4 output crossings.
The legacy plan adapters and unrelated desktop/workspace policy are out of scope.

## Acceptance

- Producer-shaped regressions fail before each correction and pass afterward.
- Rapid opposite directional inputs survive debounce and in-flight requests in
  their selected order; exhausted historical geometry targets do not block new
  successful layout transitions.
- Unequal-height occupied-target R4 crossings remain correctly projected in both
  directions after repeated transitions.
- Full KWin tests, typecheck/build and relevant Rust tests pass.
- Repeat the two-output symptoms live where a restorable baseline is available;
  distinguish authoritative frame/focus evidence from physical-input evidence.

## Approach and bounded units

1. Verify clamp-budget lifetime and add an alternating-target regression/fix.
2. Audit pinned Tier 1 input handling, select or park replay semantics, then
   exercise production-entry debounce and in-flight input paths.
3. Replay unequal-height R4 crossings, investigate remaining height/overlap
   failures, and verify the integrated production route.
4. Independently review behavior-sensitive changes and run a bounded live leg.

## Outcome (2026-10-19)

- Production tiled directional focus/move now retain unsent presses in FIFO
  order. Enqueue does not invalidate native observations; each dispatch binds
  the subject to current native focus. Idle commands skip observation debounce;
  accepted tails drain next tick unless native invalidation requires debounce.
  Sent heads never replay. Failure catchup is bounded; generation changes drop
  old queued contexts. Non-directional and native float actions retain their
  existing fail-closed behavior.
- Tiled clamp budgets now reset only on exact authoritative frame convergence,
  including delayed convergence observed before a different target. Historical
  successful A/B targets no longer exhaust; genuinely clamped B still spends
  three attempts even when interleaved with successful A.
- Fixed an additional R4 return refusal: a vacated focusless retained domain
  can report the global active lifetime homed on another domain without becoming
  held. Unknown-focus mismatch remains refused. R4 whole-target policy is unchanged.
- No legacy adapter changes or dependency changes.

## Decisions

- **Tentative (user review pending), 2026-10-19:** preserve each tiled directional
  press in order and resolve focus/move against settled native focus at dispatch,
  including mixed focus-then-move. This follows per-press sequential Tier 1 paths:
  i3 `903bcd51` `src/key_press.c:24-38`; sway `1652c54b`
  `sway/input/keyboard.c:480-530`; Hyprland `19fb395d`
  `src/keybinds/Manager.cpp:190-280`; COSMIC `3d55cba0`
  `src/input/mod.rs` `filter_keyboard_input` / `handle_action`.
  The asynchronous queue is KDE-specific, not a foreign queue implementation.
- Native float/sticky actions, missing/unsafe subjects, transport uncertainty and
  generation loss do not gain blind replay. No fixed press-count drop threshold.

## Accepted evidence

- RED/GREEN: both sync/delayed alternating-target regressions stall on the old
  clamp code; corrected code passes alongside continuous/interleaved clamp bounds.
  Planner-backed rapid focus tests fail before drain-time rebinding, then prove
  neighbor-then-return and mixed focus/move native effects. The core R4 producer
  refuses the return before the cross-domain focus correction, then passes.
- Latest integrated gates: KWin **1903/1903**, zero skipped; Rust workspace
  **3289 passed**, zero failures after integration; KWin typecheck/build, strict all-target workspace
  Clippy and Rust formatting pass. Independent review accepted safety gates;
  misleading flat-R4 fixture comments were removed and exact half-width/full-height
  assertions added and verified.
- Offline production entry with real WorldEngine: eight occupied-target R4
  crossings over unequal-height/offset outputs, four visits per target; exact
  full-height near-side half-width arrivals, no overlap, stable transfer identities,
  no duplicate relocation. No shared R4 policy change.
- Native callback-driven two-output checks: **8/8** same-output moves, **16/16**
  occupied-target R4 crossings and one rapid opposite move pair accepted with
  matching frames. A separate corrected non-edge rapid focus pair accepted both
  replies, rebound its tail to the first arrival, and returned to original focus.
  Complete serial-correlated request/reply capture and native state support these
  outcomes; invocation alone is not evidence.
- The first native focus trial began at an exhausted edge and returned `unchanged`;
  it is excluded from neighbor-then-return acceptance. Later move/R4 checks have
  independent accepted replies and native frames. The historical short-height
  incident itself had no frame capture, so its exact original cause is unproven;
  full-height arrival passes current offline/native repeats.
- Protected window frame/state/membership and original view/focus restored;
  owned clients removed, controller unloaded and Planner stopped. Shortcuts,
  float-intent bytes and loaded effects match. Configuration settings match;
  native empty-desktop/default-tiling churn remains the documented exception,
  not a byte-identical configuration claim.

## Remaining limits

- Physical key delivery and perceived latency are not established by D-Bus
  shortcut invocation. Debounce ordering is proven offline; the native focus
  pair specifically proves in-flight queuing, not a separately forced debounce.
- No unresolved implementation blocker or product-policy question beyond review
  of the explicitly tentative input choice. Exact next action: none.
