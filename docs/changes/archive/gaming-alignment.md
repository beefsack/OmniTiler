# Fullscreen gaming alignment

## Goal and acceptance

Apply the parent orientation toggle to focused fullscreen members (User 2026-11-27), without native writes to the fullscreen member. Reflow siblings normally, retain the new tile allocation, and apply it when fullscreen ends. Record that fullscreen windows are managed and ordinary explicit actions apply, without weakening no-write, anti-cheat, automatic-action or send fences.

## Plan

1. Trace orientation dispatch and replace the no-op with the ordinary parent mutation; prove the sibling reflow, no fullscreen writes, and correct exit layout in offline regressions.
2. Align vision, principles, decisions and spec; audit adapter and platform exceptions, retaining conservative safety assumptions and separating larger implementation gaps.
3. Verify the affected crate tests and Clippy. Native outcomes remain unverified.

## Audit outcomes

- Explicit workspace switch dispatch is separate from native actuation. Keep exact arrival, unmanageable/unknown foreground, automatic-action, native sends and protected-window write fences. The macOS detached fullscreen return still needs product proof; Windows selection ingress still applies fullscreen suspension before dispatch.
- ASSUMPTION: presentation suspension and any future documented Xbox-mode pause do not blanket-disable explicit shortcuts. Host-owned chords and safety/permission boundaries still win.
- ASSUMPTION: directional focus still skips occluded local siblings; the horizontal-only fullscreen crossing in core leaves a vertical parity gap. Keep fullscreen sticky refusal pending a design that cannot write or reveal the fullscreen member. Keep macOS fullscreen-exit restoration and ownership proof; do not infer exitability.
- The shared fullscreen-float target already has a Windows keyboard command route and macOS adapter integration; the older adapter-absence wording overstated the gap. Native acceptance is still required.

## Outcome and evidence

- The ordinary split flip now accepts a retained fullscreen tile. Core retains its new allocation but publishes no native subject write; its sibling receives the new controlled frame. The fullscreen member receives the flipped allocation's frame after exit. Maximize-only refusal, slotless eligibility and directional move are unchanged.
- Offline: the core library passed 2,517 tests, including the split and shared policy regressions. `cargo test --locked -p tiler-core --test world_fullscreen_req_max_12` passed all four tests, and `cargo test --locked -p tiler-windows --test tiling` passed all 105 tests. The broader serial gate was stopped after the core library, cursor targeting and output topology targets passed because fresh executables repeatedly stalled before the harness started.
- Strict scoped Clippy stopped on the nine existing Windows `log_sink.rs` dead-code diagnostics. Independent review found no blocking issue. Native adapter actuation was not tested; Windows-only code was not compiled by the portable gate.
