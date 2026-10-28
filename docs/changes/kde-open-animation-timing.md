# KDE open-animation timing probe

## Goal and scope

- Diagnose the initial-placement/open-effect/tile jump on the production
  `DescribeWorld` route. No jump fix, admission/planning change, core/protocol
  change, new setting or native clock surface.
- Trace-only events: added, actual admission debounce start/fire, world
  send/reply, requested frame and first signal-observed frame with exact match.
  Correlate by opaque window id and sent source snapshot, not live pending ids.
- Keep this record active until the single-window video/journal capture is
  reviewed. No live timing evidence has been collected.

## Material choices and implementation

- Reuse `KWIN_TRACE_ENABLED` / `__OMNITILER_TRACE__="1"` via `build:trace`.
  Inline gates in `kwin/src/world-entry.ts` allow ordinary/installed bundles
  to strip the probe with an explicit off define and `--minify-syntax`.
  A trace-only weak map keys each probe by its adapter state, preserving
  independent lifetimes while stripping even unused local probe bindings.
  No extra observes, native writes or timers. Only trace builds connect a
  read/log-only `frameGeometryChanged` one-shot before each traced setter;
  listeners detach on first fire, replacement, eviction, refusal/throw or stop.
- Per-adapter pending ids, source snapshots and requested-frame listeners are
  bounded to 16 entries. Windows joining an armed debounce do not fabricate a
  new start; arrivals after a send do not inherit that flight's reply.
- Tentative trace-only clock choice: each line carries `Date.now()` wall-clock
  milliseconds. Journal `short-monotonic` receipt timestamps cross-check one
  capture; rerun negative/implausible intervals after a clock step. Neither is
  compositor presentation proof. No native monotonic-now API is exposed by
  KWin scripting; `console.timeEnd` consumes labels rather than returning a
  reusable numeric timestamp. The prior clock investigation is not repeated.
- Format, limitations and exact user-run capture steps live at
  [Development Loop diagnostics](../dev-loop.md#kde-open-animation-timing-probe-trace-only).

## Acceptance and verification

- Regressions cover ordered producer-shaped admission, match/mismatch,
  synchronous signal, armed-debounce arrival, late in-flight arrival,
  disconnect/stop, missing signal, refused/thrown setters and logging failure.
  Real ordinary/trace bundles run the same admission harness and compare
  production requests, writes, timers, subscriptions and non-probe logs.
- Ordinary/installed bundle checks require no probe markers, probe state/work
  or `frameGeometryChanged` subscription; trace bundle retains the probe.
- One late-arrival fixture incorrectly expected writes from a superseded
  reply; corrected to require the existing stale/no-setter behavior. A new
  refusal fixture retained an old accessor setter; a read-only data property
  restores the intended false-return check without an exact-optional TypeScript
  descriptor error. No production behavior workaround.
- Final sequential gates (all via `devenv shell --impure --`):

  | Command | Result |
  | --- | --- |
  | `npm test --prefix kwin` | 855 passed, zero failed |
  | `npm run typecheck --prefix kwin` | Passed, source and test projects |
  | `npm run build:trace --prefix kwin` | Passed; emitted probe present |
  | `npm run build --prefix kwin` | Passed; no probe markers, state or frame subscription |
  | `git diff --check` | Passed |

- Earlier typecheck fixture failures were repaired separately before this
  probe; no Rust files are changed. No live testing was performed.

## Next

- Run the documented single-window recording and journal capture. Use the
  measured intervals and video to locate the jump;
  changing admission or animation behavior remains separate work.
