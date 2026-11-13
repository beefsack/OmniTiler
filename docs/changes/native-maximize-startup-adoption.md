# Native maximize startup adoption

- Status: user decision 2026-11-13; implemented and verified offline. Native
  acceptance remains open.
- Goal: preserve native maximize for windows already present when a fresh core
  starts, including tiler-restart survivors, while retaining tile-on-launch for
  windows first appearing later.
- Scope: core born admission and shared offline regressions; native maximize
  routes use existing overlay/reserved-slot semantics. No adapter policy,
  settings/schema change, fullscreen change or composed-maximize persistence.
- Cause: `world_run_v4.rs` sweeps initial inventory into slots, then
  `world_born.rs` mints a one-shot clear for first-seen maximized tiled windows
  under the `TileOnLaunch` default. A fresh generation has no earlier membership
  or born latches. This confirms the source path, not the setter correlation in
  the earlier native trace.

## Approach and acceptance

- Startup means the first accepted usable complete inventory after initial
  settings and screen readiness. Earlier incomplete/unready captures do not
  close this boundary. Retain startup presence for fact and held identities,
  even when admission is delayed; prune on lifetime end using existing rules.
- Use existing born keep-overlay lifetime ownership; do not infer maximized
  facts for held rows. Fullscreen birth/exit precedence stays unchanged.
- Preserve existing imported float/sticky intent and fixed-size admission
  precedence: these remain slotless floats beneath native maximize; explicit
  tile intent retains ordinary admission/slot semantics. No intent override.
- A re-handshake resets the world model and adopts a new startup inventory;
  transport recovery without that reset retains known-window latches. Unknown
  later windows still use the configured born policy.
- Regressions: startup survivor/no clear/reserved slot; later born clear once;
  both keep-overlay cases; user restore to reserved slot; multiple survivors
  mixed with ordinary tiled survivors without restore-time sibling reflow;
  incomplete bootstrap, held startup, reconnect, float/fixed-size and fullscreen.
- Work units: source investigation; core implementation and regression evidence;
  durable decision/spec/contract records; full offline verification and independent
  complete-diff review.
- Verification: format, build and strict clippy for core/protocol/service;
  permitted full Rust suite excluding `workspace_unsafe_posture`; KWin typecheck
  and tests. Historical fixture bytes remain immutable; affected active fixtures
  must come from genuine recorders. Native KDE/Windows behavior remains unverified
  until separate fixture-scoped live acceptance.

## Offline outcome (2026-11-13)

- `world_born.rs` seeds the existing `born_keep_overlay` lifetime set from
  startup fact/held identities before per-window decisions. No adapter logic,
  new state owner, native-fact inference or wire/settings change is needed.
  The fullscreen-exit lane still precedes that exemption.
- Before the production fix, the permitted full Rust command failed with nine
  new startup regressions failing in core (2,532 passed before the runner
  stopped). Afterward, 3,378 pass, zero fail/ignore, with only the unsafe-posture
  test filtered. Eleven new core regressions and one service journey distinguish
  startup from later launch; older clear-settlement tests now begin with an empty
  complete startup inventory. They retain the clear-once/async/refusal oracles.
- Final sequential gates pass: `cargo fmt --all -- --check`;
  `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`;
  `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`;
  `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`;
  in `kwin/`, `npm run typecheck` and `npm test` (1,297 passed, zero failed).
- Both stage5 proof binaries remain enabled and pass. Byte/hash and file-set
  checks against the baseline found no changes or extra files in historical
  `world-wire/v4` (547), `stage5` (32), `stage5-v4` (52), `kde-v4` (20), or active
  `world-wire/v4-active` (167), `stage5-v4-active` (93), `kde-v4-active` (20).
  No recorder refresh was necessary.
- Platform boundary: KDE and Windows native routes receive the shared-core
  startup exemption; post-startup born policy is unchanged. macOS declares
  `maximize:false`, cannot report born maximize, and still drops its composed
  overlay on restart. No platform adapter source changed. No live run was made;
  the prior KDE trace still lacks direct correlation to the actual setter.
- Complete-diff source review found no blocking defects or weakened clear-lane
  oracles. Broad startup seeding is intentional: it records presence, not native
  maximize, and uses existing lifetime pruning. Imported float/sticky precedence
  is source-checked; new startup tests exercise floating/fixed/sticky facts rather
  than an explicit opaque-intent import journey, which remains a coverage limit.
- Native follow-up: join exact fixture lifetimes, maximize multiple ordinary
  tiles, retain native flags/storage and full traces before stopping only the
  tiler; restart, prove readiness, rejoin the same lifetimes and verify native
  maximize/slots with no clear. User unmaximize must reveal reserved slots
  without sibling reflow. Contrast a later maximized launch and both settings;
  keep fullscreen and imported float/fixed-size cases separate.
