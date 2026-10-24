# Background Tiling

- Status: historical pre-v2 evidence. The `entry.ts`/`startPlanAdapterEntry`
  route below is removed; production uses the world route per the [core
  closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24)
  and [world adapter guide](../porting/world-adapter-guide.md). The live
  acceptance below remains the native residual; this is not the current contract.

## Delivered Boundary

- The production `kwin/src/entry.ts` route starts `startPlanAdapterEntry`, whose
  Plan adapter observes the active foreground domain and every other readable
  `(output, workspace)` domain. This includes a currently displayed desktop on
  an inactive output.
- Background work uses the existing Rust `admit`, `remove`, and `reconcile`
  lifecycle commands, owner/generation/correlation fences, and one shared Plan
  flight. It is blocked while workspace send is active; foreground work retains
  priority.
- KWin derives a deterministic domain-local structural anchor for a background
  snapshot, but background replies never request native focus or desktop
  switching. Geometry writes are scoped to the exact observed domain.
- A verified empty domain removes its final retained member only after the
  normal planned reply and exact post-observation. At that committed boundary,
  both the adapter baseline and Rust's empty retained session are retired.

## Fail-Closed Cases

- Unreadable, duplicate, or tainted observations are unknown, not empty.
  Exception-only floating, sticky, fullscreen, or maximized domains protect an
  existing baseline but do not cause a background lifecycle command.
- The retained lifecycle has one-remove acknowledgement/verification semantics.
  A fresh observation missing two or more retained members dispatches nothing
  and preserves the baseline rather than fabricating a multi-remove commit.
  Ordinary single-window hidden open, move, geometry reconciliation, and final
  close continue through the existing lifecycle.
- The original 16-domain and 64-window request bounds were retired by AR16;
  the codec request byte bound now applies. No recovery, replay, reseed,
  polling, or visibility/focus workaround was added here.

## Evidence And Live Gate

- Offline production-entry and Plan-adapter tests cover hidden startup, open,
  move, final removal, multi-output domains, exception and unreadable handling,
  send coordination, and unchanged native focus/desktop seams. Rust unit
  tests cover empty-session retirement and fail-closed multi-member collapse.
- No live KWin/Plasma action or rendered-state proof occurred. User-owned live
  acceptance remains for hidden startup/open/move, multi-output behavior, and
  unchanged native focus/desktop.

## Moved Evidence (from docs/decisions.md)

- Background tiling is implemented and statically verified; live acceptance
  remains pending.
