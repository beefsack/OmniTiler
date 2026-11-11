# Principles

Project principles, approved by the user.

Project records (user-approved 2026-10-30; existing records are migrating to
this model):

- [VISION.md](../VISION.md): why the project exists and its end goals.
- [docs/principles.md](principles.md): a few durable trade-off rules. Each
  has Intent, Rule (1-4 bullets), Yields to (the principles that win on
  conflict, or `absolute`) and an optional Example, in about 3-8 lines;
  [Priorities](#priorities) is the general conflict order. A principle states
  the trade-off; mechanisms it implies go in the contract document, spec or
  decisions, linked.
- [Functional spec](spec/functional-spec.md) and
  [reference outcomes](spec/reference-outcomes.md): all user-observable
  behavior, as rows.
- [World contract](porting/world-contract.md): the normative interface target
  (ownership, messages, apply and ordering rules, capabilities, version,
  conformance fixtures); [world-contract-v3.md](porting/world-contract-v3.md)
  is authoritative for current code until the coordinated v4 cutover.
- [docs/decisions.md](decisions.md): the current choice per topic between
  viable alternatives with lasting cost (product, architecture, contract),
  with why, alternatives rejected, key assumption and when to revisit.
  Replaced text lives in Git.
- `docs/changes/`: designs, evidence, measurements and status; deleted when
  done.
  [docs/backlog.md](backlog.md): pending work only, one line per item.
- Deletion: delete a document once nothing current needs it, after moving anything worth keeping into decisions, the spec, the contract document or code comments. Git is the only archive. Where a current record cites a deleted document as evidence, it keeps a one-line summary of the decisive fact and a GitHub permalink pinned to the last commit containing the file (https://github.com/beefsack/OmniTiler/blob/<sha>/<path>); purely historical links are dropped.
- Code comments: implementation choices and their reasons at point of use.
- Placement: observable behavior goes in the spec; a lasting choice between
  alternatives in decisions; the core/platform interface in the contract
  document; anything derivable from code or tests in a code comment or change
  record. State a thing once and link from elsewhere.
- Principles, spec behavior, contract shape and decisions change only with
  user approval. Before 0.1, a decision whose revisit condition is met is
  reopened by default.

## General

### Priorities

- Intent: settle conflicts between goals the same way every time, toward an
  authentic i3/sway/Hyprland/COSMIC tiler on hosts that are not their home
  systems.
- Rule: when goals conflict, prefer in this order, weighing the size of each
  cost (a judgement guide, not a strict rule):
  1. Authenticity to the Tier 1 input tilers, consistency between platforms,
     reliability. Where Tier 1 tilers disagree, the
     [reference tiers](decisions.md#reference-tiers) select the reference.
  2. Jank-free behavior, performance, gaming polish.
  3. Configurability, visual UX.
- Yields to: the `absolute` parts of
  [Gaming Compatibility](#gaming-compatibility) (anti-cheat safety,
  performance).
- Example: simplify a feature like the pager before accepting a significant
  performance cost, but do not drop it for a negligible gain.

### Gaming Compatibility

- Intent: playing with the tiler running feels the same as without it, and
  the tiler never puts the user's game accounts at risk.
- Rule:
  - Key on window state, not on "is this a game". A window in exclusive or
    borderless fullscreen keeps that state and is never tiled, resized,
    refocused, overlaid or moved unasked. A windowed game is an ordinary window.
  - When fullscreen detection is uncertain, prefer leaving the window alone:
    the user can still act on it, while wrongly tiling a fullscreen game
    breaks it.
  - Explicit user actions apply to fullscreen windows as to any window.
  - Anti-cheat safety: nothing the tiler does to another process may look
    like cheating to anti-cheat systems
     ([mechanism limits](decisions/gaming.md#anti-cheat-mechanism-limits)).
  - No noticeable performance impact while a game runs, demonstrated by
    measurement ([VISION.md](../VISION.md) Gaming And Full-Screen Applications).
- Yields to: explicit user actions, for placement, focus and fullscreen
  state. Anti-cheat safety and performance are `absolute`.

### Simplicity

- Intent: fewer moving parts mean fewer failure modes, most of all in complex
  areas with uncontrolled host elements, where added machinery multiplies the
  ways things can fail.
- Rule:
  - Choose the simplest, least surprising solution that delivers the
    requirements within our constraints.
  - Prefer converging to desired state over tracking individual operations:
    where re-applying is safe, re-apply rather than correlate each attempt.
  - Add no timeout, expiry, retry cap or other limit unless it solves a
    specific, identified problem or provides tangible value; state that
    reason where the limit is defined.
  - When a design replaces another, remove the replaced mechanism in the same
    change or as a tracked backlog item with an owner; never leave it running
    underneath as a layer, shim or fallback. Before 0.1, keep no
    compatibility paths.
  - One mechanism per job: store each fact once and give each kind of work
    one path. Extend the existing mechanism before adding a parallel one.
- Yields to: the requirements themselves. Never simplify away authenticity,
  consistency, reliability or the `absolute` parts of
  [Gaming Compatibility](#gaming-compatibility).

### Focus and Return on Investment

- Intent: effort goes where it most advances the goals that serve
  [VISION.md](../VISION.md) (current goals: top of
  [docs/backlog.md](backlog.md)).
- Rule:
  - Prioritise by return toward the current goals: the most goal-advancing or
    risk-reducing progress per unit of effort comes first.
  - Tooling, test harnesses and evidence collection are means, not ends. Size
    them to the decision they inform. When effort outgrows the value, stop
    and choose a simpler route, or drop the work, or accept weaker evidence
    and record it as weaker.
- Yields to: the `absolute` parts of
  [Gaming Compatibility](#gaming-compatibility); never accept weaker evidence
  for anti-cheat safety or performance.

## Product

### Consistency

- Intent: the same action gives the same result on every target platform and
  matches the Tier 1 input tilers; this is a main reason the project exists.
- Rule:
  - Share defaults and semantics wherever possible. A behavior difference
    that is a product choice is a user setting with one shared default, never
    a per-platform behavior.
  - Unavoidable host differences are documented, naming the host limitation
    behind each. Parity is proven by shared conformance fixtures
    ([Testing](#testing)).
- Yields to: unavoidable host limitations (documented as above) and the
  `absolute` parts of [Gaming Compatibility](#gaming-compatibility).

## Architecture

### Layering

- Intent: one model and one decision maker, so policy is written once and
  every platform behaves the same.
- Rule:
  - Core owns the model (screens, workspaces, windows, focus, cursor), the
    policy for every action and the resulting desired world state, including
    presentation geometry.
  - Platforms capture input, promptly report host facts and outside changes,
    converge the host to the latest desired state and draw what core
    describes. They never decide policy.
  - Core strives to stay platform agnostic: host differences reach core as
    platform-neutral capabilities, never as platform checks or platform-specific
    state.
  - Core is independent of the wire format: it exchanges typed values, and
    encoding, field names and byte limits belong to the transport.
  - A platform that applies only the latest publication, skipping any earlier
    ones, is fully consistent with core. Core carries every pending one-time
    effect (such as a focus request) in each publication until it settles, so
    nothing depends on seeing an intermediate publication.
  - Platforms apply every published decision the host allows. A decision a
    platform cannot or must not apply is a declared platform-neutral capability
    or a contract-listed exception, never a silent local policy; the next report
    shows the real host state and core continues from it.
  - Push shared logic down into core or a shared library; keep host-coupled
    code (scripts, effects, settings modules, OS hooks) thin.
  - A platform acts without a core round trip only where the round trip would cause visible jank, a performance cost or an unsafe effect. Such an exception is limited to mechanical safety vetoes or applying a core decision in the same turn, never policy, and each is listed in the contract document.
- Yields to: Simplicity and host-native behavior when pushing code down; the
  `absolute` parts of [Gaming Compatibility](#gaming-compatibility).
- Contract rules that follow: [decisions: Contract](decisions.md#contract).

### Ordering and Races

- Intent: behavior is correct under any event order the host can produce,
  without guessing from timing.
- Rule:
  - Never assume an ordering the host does not guarantee; in particular,
    never assume an event, reply or observation reflects state after a
    request we sent.
  - User intent is resolved against core-retained state at execution, not
    against values sampled earlier, and nothing re-decides it afterwards.
  - Prove freshness and identity causally, never from timing, geometry
    similarity or age. Mint correlation identities only for effects that are
    unsafe to re-apply, such as focus grants and one-time writes.
  - Never add a delay or settle wait for unknown host timing: act on the
    host event or readback that proves the needed state, or document the gap
    and degrade truthfully.
- Yields to: a genuine deadline or recovery bound
  ([Efficiency](#efficiency)).

### Resilience

- Intent: in a host we do not control, the tiler keeps working and recovers
  by itself.
- Rule:
  - Never give up permanently. The only terminal state is a hard failure that
    prevents all functionality (for example, the host's window-management
    interface is missing or access is denied). Every other failure is logged
    and followed by recovery.
  - Host values that differ from what was requested, and windows that change
    themselves at any time (including mid-operation), are normal events:
    tolerate them and converge without surprising the user.
  - Refuse or degrade the narrowest scope (row, command, window, domain),
    never a whole report for one bad row, while unaffected functionality and
    later recovery stay available.
  - When a scope cannot continue, prefer failing open: release it to the host so the user keeps a usable desktop (hidden windows shown, held input and shortcuts released, recoverable native changes undone). Fail closed, leaving windows, input or the desktop held, only where letting go would cause harm, break other system functions or leave a worse state, or the platform requires it; decide case by case and state the reason where it is implemented. Refusing one uncertain action is not failing closed.
- Yields to: the `absolute` parts of
  [Gaming Compatibility](#gaming-compatibility).

### Efficiency

- Intent: low latency and no pointless work, especially while games run.
- Rule:
  - Prefer events over polling: react to host notifications, wakes and
    signals, so work happens only when something changed. Use a timer only
    for a genuine deadline or where the host offers no event, and state that
    reason where the timer is defined.
  - Core cost per report stays within a stated latency budget, guarded by a
    regression check; input that has not changed costs little.
  - User input is never delayed for batching: a command reaches core as soon
    as it is captured. Debounce only bursts of host events.
- Yields to: authenticity, consistency and reliability
  ([Priorities](#priorities)), except the performance guarantee in
  [Gaming Compatibility](#gaming-compatibility), which is `absolute`.

### Host Integration

- Intent: the best UX the host allows, without breaking when an
  undocumented route changes.
- Rule:
  - Where an undocumented host route is much faster or gives clearly better
    UX than the documented one, prefer it.
  - Keep a documented route as the automatic fallback when the undocumented
    route is unavailable; where no documented route exists, degrade
    truthfully.
  - Make the route user-configurable where its breakage risk is real, and
    state that risk where the route is defined.
- Yields to: [Simplicity](#simplicity) when the gain is small; the
  `absolute` parts of [Gaming Compatibility](#gaming-compatibility).

## Engineering

### Observability

- Intent: normal logs alone let us reconstruct what happened end to end,
  including why something failed and whether it recovered.
- Rule:
  - Every component logs its decisions and outcomes honestly, never implying
    more certainty than is known.
  - Logging never changes product behavior and never exposes secrets or user
    content.
- Yields to: the performance guarantee in
  [Gaming Compatibility](#gaming-compatibility), which is `absolute`.

### Testing

- Intent: tests catch the failures that happen on real hosts, not on
  idealized ones.
- Rule:
  - Test data looks like real data. Fixtures at host, process and wire
    boundaries are shaped exactly as real producers send them (sentinel
    values, native container types, identifier formats, objects that
    disappear on removal), preferably captured from real runs.
  - Every adapter passes the same shared contract conformance fixtures.
  - Platform code cites the host ordering guarantees it relies on, and tests
    cover the interleavings that matter, including those only some hosts
    produce.
- Yields to: [Focus and Return on Investment](#focus-and-return-on-investment),
  for how far to take each test.

### Rust and Memory Safety

- Intent: memory safety, maintainability, consistency and reuse in our own
  code, by writing as much of it as possible in safe Rust.
- Rule:
  - Our own code is Rust wherever a host allows it, including host-coupled
    code outside core. Other languages (C++, TypeScript) only for the thin
    shim a host interface requires, such as the KWin effect, settings module
    and script entry points. Weigh Rust's build, packaging and distribution
    cost against the complexity it removes from other languages.
  - Avoid Rust `unsafe`, in this order of preference:
    1. A popular, maintained, battle-hardened dependency that contains the
       unsafe code behind safe APIs.
    2. A design that needs no unsafe code.
    3. An absolutely minimal module that other modules use through safe
       APIs, as an explicit, reviewed exception.
  - Required C++ uses RAII (Qt parent ownership where Qt requires it), never
    owning raw pointers, and its tests run under AddressSanitizer and
    UndefinedBehaviorSanitizer in CI.
- Yields to: nothing beyond Rust option 3 and the host-required shims.
  Enforced by the workspace `unsafe_code = "forbid"` lint and an allowlist
  test ([guardrail](decisions.md#rust-unsafe-guardrail)); C++ sanitizer CI is
  not yet in place (backlog).

### File Size

- Intent: each source file can be read and reviewed in one pass.
- Rule:
  - Soft cap: about 1,500 lines per production source file and about 3,000
    lines per test file.
  - The cap is a trigger to split by responsibility, not a hard limit;
    exceed it only when splitting would make the code harder to follow.
  - Not enforced by tooling; current overruns are tracked in the backlog
    ("Module size refactoring").
- Yields to: [Focus and Return on Investment](#focus-and-return-on-investment),
  for when a split pays for itself.
