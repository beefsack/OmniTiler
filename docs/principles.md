# Principles

Project principles, approved by the user.

- [VISION.md](../VISION.md) is the end goal the project strives for.
- [docs/principles.md](principles.md) contains high-level rules and reasoning - the what and why.
- [docs/spec/functional-spec.md](spec/functional-spec.md) is the exact behavior contract (requirement rows).
- [docs/decisions.md](decisions.md) records the how: selected behavior and implementation choices,
  including why each was chosen.
- Overlap is expected; state a thing once and link from elsewhere.

## Simplicity

- We are complexity averse. Always choose the simplest and least surprising
  solution that delivers the requirements within our constraints.
- This matters most in complex areas with uncontrolled elements and multiple
  failure modes, where added machinery multiplies the ways things can fail.

## Layering

- Push as much logic as is sensible down into the shared, system-agnostic
  core, so every platform reuses it.
- System-specific logic sits in a middle layer of platform code. The layer
  coupled directly to a host (scripts, effects, settings modules, OS hooks)
  stays as thin as reasonably possible.
- Pushing code down is ongoing work applied with judgement and within reason;
  it never overrides Simplicity or host-native behavior.

## Resilience

- Never give up permanently. The only acceptable terminal state is a hard
  failure that prevents all functionality, such as KWin missing or access to
  it denied. Every other failure is logged and followed by recovery; ephemeral
  issues must not disable tiling or any other functionality.
- We do not control the host. KWin, Wayland, and clients may report values
  that differ subtly from what was requested or expected. Tolerate these
  differences and converge gracefully, in ways that do not surprise the user.
- Windows change themselves. They resize, change modes, move, and close at any
  time, including mid-operation. Handle these as normal events, not failures.
- Fail closed only when recovery is impossible, or when continuing would cause
  harm such as system instability: refuse the narrowest scope
  (operation, window, or domain) while unaffected functionality and later
  recovery remain available.
- Otherwise degrade as narrowly as possible, log the cause, and keep
  functioning, reconciling differences as they are observed.

## Gaming Compatibility

- Gaming compatibility must be flawless. Games, including borderless and
  exclusive fullscreen titles, must run exactly as they would without the
  tiler: no unwanted tiling, resizing, focus changes, overlays or input
  interference.
- Aim for zero performance cost. Where a platform makes zero impossible, the
  residual cost must be demonstrated minimal and imperceivable ([VISION.md](../VISION.md)
  Gaming And Full-Screen Applications).
- When a behavior choice trades consistency or reference-WM parity against
  game safety, game safety wins.

## Unsafe Code

- Rust `unsafe` is avoided in our own code, in this order of preference:
  1. Use a popular, maintained, high-quality and battle-hardened dependency
     that contains the unsafe code and exposes safe Rust APIs. Unsafe code
     inside such a dependency is acceptable.
  2. Avoid unsafe code entirely.
  3. Otherwise, contain it in an absolutely minimal module that other Rust
     modules use through safe APIs.
- Unsafe code is forbidden by default across the project; each minimal
  module is an explicit, reviewed opt-in exception.

## Observability

- Observability is a core requirement across every component: normal logs must
  let us reconstruct what happened end to end, including why something failed
  and whether it recovered.
- Logs report outcomes honestly and never imply more certainty than is known.
- Observability must never change product behavior or expose secrets or user content.
