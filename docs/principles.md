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
- Add no timeout, expiry, retry cap or other limit unless it solves a
  specific, identified problem or provides tangible value; state that reason
  where the limit is defined. Otherwise it is just complexity.

## Efficiency

- Prefer events over polling: react to host notifications, wakes and
  signals instead of periodically checking for change, so work happens
  only when something changed.
- This serves low latency (react immediately, not at the next tick),
  efficiency, and game safety (no pointless execution while idle).
- Use a timer only for a genuine deadline or where the host offers no
  event; state that reason where the timer is defined.

## Host Integration

- Good UX is critical. Where an undocumented host route is much faster or
  gives clearly better UX than the documented one, prefer the undocumented
  route.
- Undocumented routes carry a higher breakage risk, so always keep a
  documented route as the fallback, used automatically when the undocumented
  route is unavailable.
- Make the route user-configurable, so users can work around breakage
  themselves.

## Consistency

- Consistent behavior across target platforms and with the Tier 1 input
  tilers is a primary product goal and a main reason the project exists.
- Share defaults and semantics wherever possible, and explicitly document
  unavoidable host differences, subject to game safety
  ([Gaming Compatibility](#gaming-compatibility)).

## Priorities

- The project exists to give i3/sway/Hyprland/COSMIC users an authentic
  tiler on hosts that are not their home systems.
- When goals conflict, prefer in this order (a judgement guide, not a strict
  rule; weigh the size of each cost):
  1. Authenticity to the Tier 1 input tilers, consistency between platforms,
     reliability. Full authenticity on each platform yields consistency.
  2. Jank-free behavior, performance, gaming polish.
  3. Configurability, visual UX.
- Game safety (Gaming Compatibility) stays absolute above this ordering.
- Example: simplify a feature like the pager before accepting a significant
  performance cost, but do not drop it for a negligible gain.

## Focus and Return on Investment

- Work advances explicit goals that serve [VISION.md](../VISION.md). Current
  goals are listed at the top of [docs/backlog.md](backlog.md) and change as
  milestones land.
- Prioritise the backlog by return toward the current goals: the most
  goal-advancing or risk-reducing progress per unit of effort comes first.
- Tooling, test harnesses and evidence collection are means, not ends. Size
  them to the decision they inform. When effort outgrows the value, stop and
  choose a simpler route, accept weaker evidence, or drop the work.

## Layering

- The core is the single model and decision maker: it owns screens, workspaces and their per-screen mapping, windows, focus and cursor position; the policy for every action; and the resulting desired world state, including the shapes and bounds of borders, underlays and drop overlays.
- Platform code only captures user input, reports host facts and changes made
  outside the core (displays, settings, windows changed by apps, users or the OS,
  facts the core cannot know such as "cannot be controlled"), converges the host
  to the latest desired state with host mechanisms as quickly as it can,
  skipping superseded intermediate states, and draws what the core describes.
  Platform code never decides policy.
- Intentional changes made by users or apps are reported to the core promptly,
  including while the desired state is being applied. Transient side effects
  of applying the desired state are not world changes. Platforms avoid those
  side effects where possible and distinguish them by causal evidence, never
  timing. Actual host state and uncertainty are reported separately from policy
  changes; safety checks always use the live host state.
- Policy lives in the core. Platforms classify the facts only they can know
  (for example, whether a foreground change came from the user, an app, a core
  request, or an unknown cause) and report them, with uncertainty, as facts;
  the core decides what they mean for placement, membership, workspaces and
  focus. A thin platform carries no policy; it does not pass raw state for the
  core to guess from.
- Host limitations reach the core as platform-neutral options (for example, every screen always has the same workspace count), never as platform checks.
- Logic several platforms share that is not part of the core's role goes into a separate shared library, decided case by case.
- Push as much logic as is sensible down into the shared, system-agnostic
  core within its role above, so every platform reuses it.
- System-specific mechanisms sit in a middle layer of platform code. The layer
  coupled directly to a host (scripts, effects, settings modules, OS hooks)
  stays as thin as reasonably possible.
- Pushing code down is ongoing work applied with judgement and within reason;
  it never overrides Simplicity, Gaming Compatibility or host-native
  behavior, and never transfers core policy to platforms.
- The core is platform-agnostic and true to the Tier 1 input tilers. When a
  platform needs special core behavior to work around a host limitation,
  first check whether host mechanisms alone can honor existing core policy;
  otherwise represent the limitation through platform-neutral core options,
  aligned as closely as possible with these principles and the Tier 1 input
  tilers. Platform workarounds must not become policy owners.
- Where core support is still needed, it is as small and simple as possible,
  optional with opt-in from the platform, and built generically: named for
  the capability or limitation, never the platform, with a comment explaining
  which host limitation it serves (for example, an output-shared workspace
  option for KDE virtual desktops spanning all outputs, not a "kde" option).
- Core changes requested for a specific platform are discussed with and
  approved by the user before implementation.

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

## Ordering and Races

- Hosts deliver events, replies and the results of our own requests
  asynchronously, and each host orders them differently. Never assume an
  ordering the host does not guarantee; in particular, never assume an event,
  reply or observation reflects state after a request we sent.
- Core commands carry user intent and are resolved against core-retained state
  at execution, not against values the platform sampled earlier. Platform code
  sends only what the user actually chose or what the input itself carries
  (stable identities, live input coordinates).
- Freshness and identity are proven causally (generations, tokens, identities
  minted at the source), never inferred from timing, geometry similarity,
  window identity or age.
- Platform code cites the host ordering guarantees it relies on, and tests
  cover the interleavings that matter, including those only some hosts
  produce.
- Never add a delay, sleep or settle wait to work around unknown host timing
  or ordering. Find the host hook, event or readback that proves the needed
  state (for example, an event showing a window can now take focus) and act
  on it. If the host offers none, document the gap where it is used and
  degrade truthfully; a timer remains only a genuine deadline or recovery
  bound ([Efficiency](#efficiency)).

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
- Games remain workspace members. Explicit user workspace actions (send,
  switch away, return) apply to them; the tiler itself never tiles, resizes,
  focuses, overlays or intercepts input for them unasked.

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

## File Size

- Keep source files small enough to read and review in one pass. Soft cap:
  about 1,500 lines per production source file and about 3,000 lines per
  test file.
- The cap is a trigger to split by responsibility, not a hard limit; exceed
  it only when splitting would make the code harder to follow.

## Observability

- Observability is a core requirement across every component: normal logs must
  let us reconstruct what happened end to end, including why something failed
  and whether it recovered.
- Logs report outcomes honestly and never imply more certainty than is known.
- Observability must never change product behavior or expose secrets or user content.

## Testing

- Test data should look like real data. Fixtures at host, process and wire
  boundaries use values shaped exactly as real producers send them
  (sentinel values, native container types, identifier formats, objects
  that disappear on removal), preferably captured from real runs.
  Idealized mocks hide the failures that matter most.
