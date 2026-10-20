# Controllable live-test window helper

- Status: delivered; KDE Wayland/XWayland isolation validated with two small
  reporting fixes. X11 exclusive fullscreen and visual rendering remain
  unverified; controller integration remains deferred.
- Subsequent [API expansion](live-fixture-api-expansion.md) adds level,
  focus/attention, useful hints and compact/event-driven observation waits;
  it supersedes the settlement-ergonomics gap below, not the remaining
  exclusive-mode or controller-integration gaps.

## Goal and scope

- Development-only winit helper for multiple owned native windows, controlled by
  caller IDs over versioned local JSON lines; existing Windows/macOS helpers stay.
- Common geometry, hints, state, runtime decorations and description; render
  requested and observed state without pretending host acceptance.
- One small isolated KDE/X11 extension demonstrates the platform action pattern.
- No GTK/Qt, production tiler integration, arbitrary foreign-window targeting or
  game-renderer exclusive-presentation claim.

## Acceptance and approach

- Non-colliding workspace binary; safe Rust by default; event-driven main-thread
  window ownership, software text rendering and license-clean bundled font.
- Spawn-many, ID-list get/set/close, list, ordered non-atomic batch and event
  subscriptions; private local endpoint and per-run nonce.
- Explicit unsupported/unknown values and requested-versus-observed snapshots.
- Point-of-use usage and exact manual smoke commands; durable decisions promoted.
- Offline build, protocol/dispatch/state tests, clippy, formatting and relevant
  repository dependency/license gates. Native desktop behavior remains unverified.

## Bounded units

1. Audit dependencies, available APIs, naming and verification entry points.
2. Implement helper, targeted tests and point-of-use usage.
3. Independent contract/security review, integrate corrections and verify.

## Current evidence

- Design input: `docs/research/live-test-window.md`; project principles and live
  KWin guide read. Existing sentinel: `OmniTilerTestWindow`.
- Existing Windows binary is `tiler-test-window`; new binary must not collide.
- Selected name: `tiler-live-fixture`, avoiding the existing Windows binary.
- Use dynamic-loading winit/softbuffer features for compilation without native
  development headers; declare required runtime libraries in `devenv.nix` before
  native use. A refreshed development environment is required for those additions.
- Bundled MIT-compatible `font8x8` bitmap glyphs avoid an outline-font stack.
- KDE/X11 example: creation-time normal/dialog/utility window type through safe
  winit extension APIs. It exercises admission/classification without external
  scripting or arbitrary-window access; other backends report it unsupported.

## Outcome and evidence

- Delivered `crates/tiler-live-fixture`: `serve`/`send`, versioned JSON lines,
  caller IDs, spawn-many, ID-list get/set/close, ordered non-atomic batch,
  connection-scoped subscriptions and explicit shutdown. Main thread owns both
  state and native windows; IO only queues whole requests. All Rust stays safe.
- Full requested/configured/observed snapshots and independent description render
  inside each window. Geometry/hints use physical pixels; global positioning,
  visibility, restore and fullscreen limitations are explicit per backend.
  X11 fullscreen getter is a setter cache, so it is not called an observation.
  `settled` means geometry feedback, not mode confirmation or host acceptance.
- Contract/security review found and resolved a request/event-loop lock cycle,
  ineffective unsubscribe, preexisting-directory chmod, and state-reporting
  defects. Targeted regressions cover those paths; follow-up review found no
  remaining blockers. No review findings are deferred into live testing.
- `cargo build --locked -p tiler-live-fixture`: passed.
- `cargo test --locked -p tiler-live-fixture`: 77 passed.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked -p tiler-live-fixture --all-targets -- -D warnings`:
  passed. Workspace tests and all-target clippy pass with `--exclude tiler-macos`.
- Full-workspace test/clippy commands fail in unchanged `tiler-macos` sources:
  `textedit_post.rs` references a macOS-only `AX_ATTRIBUTE_UNSUPPORTED` on Linux;
  `select_owner.rs` calls missing `take_trailing_proof`. No unrelated fix included.
- Initial license rejection was resolved by the 2026-10-20 global BSD-2-Clause,
  BSD-3-Clause and ISC approval in both `deny.toml` and `about.toml`; Wayland
  decorations and dynamic loading remain enabled. License/bans/sources checks
  pass (duplicate-version warnings in bans are non-fatal).
- Notice generation initially swept development-only members into its documented
  shipping-root scope and failed harvested-text checks. Rooting cargo-about at
  `omnitiler` preserves the current two-root union (effect-FFI dependencies are
  a subset); the existing union completeness guard still fails on missing crates.
  `just third-party-notices` and `just check-third-party-notices` pass: 70 covered
  third-party versions, all harvested, two supplemental upstream notice files.
- Durable scope/default/license decisions are in `docs/decisions.md`; usage,
  exact two-terminal smoke commands and host limits are in the crate README.
- `devenv.nix` declares runtime Wayland/xkbcommon/X11 libraries and loader path.
  Environment refresh is required before native use; offline dynamic-loading
  compilation does not establish runtime availability.
- At offline delivery, native desktop behavior and Windows/macOS compilation
  remained unverified. KDE isolation results below supersede the native gap;
  Windows/macOS compilation is still unverified.

## Native validation preflight (2026-10-20)

- KDE Wayland preflight: `just dev-status` reported Planner unowned and controller
  unloaded (`DOWN`); no fixture server or windows were started at that stage.
- The KDE development controller has no fixture-only admission scope:
  `kwin/src/world-observe.ts` includes every readable normal window, and
  `just dev` offers no class/app-id filter. Starting it cannot preserve an
  unrelated normal window from tiling; integration testing needs a separate
  isolation decision before startup. No controller was loaded.
- Inventory-route investigation invoked KWin's no-argument `queryWindowInfo`
  D-Bus method and received a timeout. This is not a proven passive inventory
  route: an outstanding interactive selection could not be excluded. Validation
  stopped without fixture mutation. The picker was subsequently cleared and the
  baseline confirmed before fixture testing resumed. The live guide now forbids
  that call as an inventory probe.

## KDE isolation results (2026-10-20)

- Conditions: KDE Wayland session with fractional scaling; native Wayland and
  forced X11/XWayland fixture backends, no Planner/controller. Raw correlated
  JSON replies were retained outside the repository. Fixture `get` supplied
  native client readbacks; X11 `xprop` queried only owned window handles.
- README smoke plus extended feature runs completed on both backends. Immediate
  replies often preceded native feedback; later bounded readbacks established
  geometry/state outcomes. Cache-only acceptance is not native confirmation.

| Feature | Wayland | X11/XWayland |
| --- | --- | --- |
| Physical size, multiple windows, runtime programmatic resize | Observed requested sizes | Observed requested sizes |
| Min/max hints | Accepted; below-min and above-max programmatic sizes were not clamped | Observed min/max clamping; contradictory hints rejected |
| Fixed/resizable | Accepted; fixed window still accepted programmatic resize | Accepted; fixed-size native hints verified |
| Global move | Unsupported | Moved; observed position included a decoration offset |
| Maximize/windowed | Work-area expansion and prior-size restore observed; normal mode can remain unknown | Mode and geometry readbacks confirmed both |
| Minimize/restore | Minimize accepted, state unknown; restore explicitly unsupported | Minimized/windowed readbacks confirmed both |
| Borderless fullscreen | Fullscreen configure and output-size geometry observed | Output-size geometry observed; mode readback deliberately unknown |
| Exclusive fullscreen | Explicitly unsupported | Not exercised: automatic video-mode selection risks output changes |
| Runtime decorations | Off/on accepted; no trustworthy readback | Off/on verified by native frame extents |
| Descriptions, including long/wrapped text | Accepted; reporting bug fixed | Accepted; reporting bug fixed |
| ID lists, partial errors, ordered non-atomic batch | Passed | Passed |
| Subscriptions, replacement, unsubscribe | Created/closed/state events; post-unsubscribe mutation silence verified | Same, including post-unsubscribe mutation silence |
| Creation-only normal/dialog/utility | Non-normal types explicitly unsupported | Native type/class properties verified; runtime type change refused |
| Visibility | Explicit requests unsupported; spurious reporting bug fixed | Hide/show readbacks confirmed |
| Close-all, ID reuse, shutdown | Empty-list/server-alive and process/socket cleanup confirmed | Same; shutdown also tested with an owned window still open |

### Small fixes and verification

- Spawn no longer caches local-only description in `configured`; subsequent
  description changes cannot leave a stale spawn value there.
- Explicit visibility intent is tracked internally so default Wayland
  `visible:true` does not create an unsupported tag after an unrelated update.
  Explicit unsupported requests still persist; requested wire defaults are unchanged.
- Both defects reproduced before correction with failing targeted tests; both
  fixes were rechecked live on Wayland and X11. Four regression tests added.
- `cargo test --locked -p tiler-live-fixture`: 81 passed.
- `cargo clippy --locked -p tiler-live-fixture --all-targets -- -D warnings` and
  fixture formatting checks passed; independent code/evidence review passed.
- Original runs retained before/after controller status, desktop and config
  evidence; some follow-up cleanup checks were observed but not file-retained.
  The final Wayland constraint probe retained status/config/desktop checks and
  server exit status. No controller/helpers were loaded, no foreign windows were
  targeted, and owned windows, server processes and sockets were cleaned up.
  Foreign-window geometry and complete shortcut/effect restoration were not
  independently established; no visual rendering observation is claimed.

### Practical verdict and remaining work

- Useful immediately for repeatable admission/state scenarios: caller IDs,
  multi-window batches, explicit unsupported fields and real readbacks worked.
  Full snapshots are verbose; settlement needs bounded external orchestration,
  and asynchronous events can duplicate unchanged snapshots. No tiler-log
  correlation was evaluated because integration was not started.
- A small bounded per-window change history would help explain concrete
  request/readback gaps: X11 hint clamping, decoration-offset moves, delayed
  show/maximize/fullscreen feedback and repeated state events. Request correlation
  is useful; exclusive causal attribution is not reliable. An unmatched change
  can only be external/unknown, never reliably user versus WM. Defer a ledger
  implementation until repeated live journeys justify it; avoid durable logging
  or a comprehensive attribution system.
- Priorities: (1) bounded settlement/readback ergonomics, (2) safe current-mode
  selection or explicit mode choice before X11 exclusive testing, (3) only if
  still needed, bounded timestamped change history with request correlation.
  README now explains immediate replies, hint/move limits and exclusive-mode risk.
- No tiler defect was established. A separate desktop or output within the same
  KWin is not isolation: `world-observe.ts` includes hidden desktop members.
- Recommended integration direction: private nested KWin with fixture-only
  clients. Read-only checks found KWin and `dbus-run-session` available; KWin is
  host-provided, not declared in devenv. Bus calls and Planner store namespace
  follow the private bus/runtime, but normal `just dev` is not a verified nested
  route: native-effect delivery and host-journal readiness assumptions need a
  bounded launcher/capture adaptation. No nested compositor was launched.
