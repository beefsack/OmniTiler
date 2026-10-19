# Rust-First Edge-Drag Route

- Historical transport proposal: DescribePlan and its adapter were removed.
  Current production uses the world route; the old adapter-specific echo and
  transport discussion below is not a current implementation contract.

## Verified Facts

- KWin 6.7.4 snapshots `moveResizeGeometry()` before start emission
  (`src/window.cpp:1053-1062`). On cancel it synchronously restores that geometry
  before finish emission (`src/window.cpp:1069-1110`); `moveResize()` first sets
  the move-resize geometry (`src/window.cpp:3412-3420`).
- `EffectWindow` relays start and finish from `Window` (`src/effect/effectwindow.cpp:83-90`).
  An effect can retain the start rectangle and read `EffectWindow::window()->moveResizeGeometry()`
  at finish before `finishInteractiveMoveResize()` returns; both APIs are public
  (`src/effect/effectwindow.h:668`, `src/window.h:621-636`).
- Equal start/final rectangles mean cancellation or a committed net-zero drag. Both
  are the accepted no-share-change result; the API does not provide a literal cancel bit.
- The shipped 6.7.4 `libkwin.so.6.7.4` exports both methods (`nm -D`): their classes
  are `KWIN_EXPORT` (`src/window.h:114`, `src/effect/effectwindow.h:47`).

## Minimum C++ Surface

- Rust-first is achievable for policy and the engine, but a pure-Rust KWin effect is
  unproven. The factory macro emits a `Q_OBJECT`, plugin metadata, and versioned IID
  factory (`src/effect/effect.h:1091-1093`, `src/effect/effect.h:1131`,
  `src/effect/effect.h:1157-1180`). The in-tree effect uses that C++ macro and moc
  include (`kwin/native-effect/activewindowborder.cpp:109-113`).
- Estimate 60-110 C++ lines: an `Effect`/`QObject` subclass, factory/moc, signal
  connections, KWin-to-POD rectangle projection, and a no-throw C ABI callback.
  Rust owns oracle state and planning; no Qt or KWin type crosses the ABI.
- Qt method slots require a `QObject` receiver (`QtCore/qobject.h:224-228`); the
  effect itself already is one (`src/effect/effect.h:610-612`). `cxx-qt` and
  `qmetaobject-rs` are unproven to replace this KWin factory/moc boundary and do
  not remove the KWin C++ ABI.

## Performance And Reliability

- A default effect is active (`src/effect/effect.cpp:401-404`), so KWin calls
  `isActive()` and includes it in each paint pass (`src/effect/effecthandler.cpp:426-437`).
  The oracle must override `isActive()` to return false and implement no paint hooks.
- Then its idle cost is the per-frame `isActive()` virtual call plus persistent Qt
  connections; interactive emissions occur only on drag signals
  (`src/effect/effectwindow.cpp:83-91`). Therefore VISION's literal zero-performance
  impact is unachievable for any loaded effect; absolutely zero per-frame cost is unproven.
- Any Rust callback reached from KWin must catch panics before its `extern "C"`
  boundary. Unwinding into C++/Qt is forbidden.

## Recommended Design

- Transport: do not select a production transport yet. `unproven`: a synchronous
  production-script read of effect-owned data. `EffectWindow::setData()` stores data
  on the effect wrapper (`src/effect/effectwindow.cpp:396-408`), but this investigation
  found no source-evidenced scripting bridge. Direct POD FFI puts Rust in KWin but
  bypasses the preserved `DescribePlan` route; additional IPC adds an ordering race.
  The selected route is not implementation-ready until a synchronous bridge is proven.
- Echo fence: retain one exact, one-shot neighbour-write expectation keyed by the
  pointer command correlation and source. Before reconciliation, consume it only
  when same-scope fresh neighbour rectangles equal the planned rectangles; update
  `lastGood` and return. Any mismatch follows existing bounded reconciliation
  (`kwin/src/plan-adapter.ts:819-973`, `kwin/src/plan-adapter.ts:1227-1317`). This
  preserves allocation authority and rejects partial/constrained writes.

## Open Questions For The User

- Select or authorize investigation of a synchronous effect-to-script bridge. Without
  it, Option 1 cannot revive `5a760a09666c16ef36a70484446c4c6d96d9a1b6` safely.
- Approve the 60-110 line C++/moc shim and an in-KWin no-unwind Rust callback only
  if the resulting Rust-in-KWin-process boundary is acceptable.
