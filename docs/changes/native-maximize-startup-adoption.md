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

## KDE native check (2026-11-13)

- Outcome: partial acceptance, not a full pass. Both startup survivors retained
  native maximize and the ordinary tiled sibling retained its frame. No-clear
  and later-born clear-once remain unverified because the retained captures omit
  the clear-operation lane. Windows native acceptance remains separate.
- Staging: three normal resizable Wayland fixture windows A/B/C on one appended
  workspace and one output; A and B were both natively maximized (mode 3), with
  C still tiled. Exact sentinel/PID/native-UUID joins were retained externally.
  Both controller incarnations used
  `just dev trace --scope-class OmniTilerTestWindow`; readiness was evaluated
  independently for each. No stored setting was changed.
- Evidence names below identify retained external artifacts, not repository
  files. The complete helper receipt inventory is in `audit-report.md:24-40`.

| Leg | Verdict and observed result | Evidence |
| --- | --- | --- |
| Clean startup | PASS: ready/discovered and accepted dispatch/applied pairs | `leg1-readiness-gate.txt:1-6`; `just-dev-1.stdout.log:73-74` |
| Tile three; maximize two | PASS for native state and member/allocation view: A/B maximized together, C unchanged; allocation snapshots are presentation-only | `leg2-pre-max-presentation.json:190-274`; `leg2-max-ab.json:1-6`; `helpers/h5-read-max-receipt.json:1-117` |
| Tiler-only restart | PASS for orderly dev teardown and fresh evaluated readiness; both incarnations' logs retained, helpers absent before teardown | `leg3-pre-teardown-introspect.json:1`; `just-dev-1.stdout.log:395-433`; `leg3-mid-dev-status.txt:1-8`; `leg4-readiness-gate.txt:1-6` |
| Startup adoption | PARTIAL: same A/B native lifetimes, mode and frames retained; C frame unchanged. No-clear is NOT VERIFIED | `leg4-wininfo-a.json:1`; `leg4-wininfo-b.json:1`; `leg4-wininfo-c.json:1`; `leg4-presentation.json:190-274` |
| Command-route restore | PARTIAL: KGlobalAccel maximize command cleared A into its post-restart reserved slot; B stayed maximized and C did not reflow. A/B reservations exchanged across restart, so original per-window slot preservation is not demonstrated | `leg5b-kwin-journal.log:31-33,59`; `leg5b-wininfo-a.json:1`; `leg5b-presentation.json:1-43`; `leg4-presentation.json:220-274` |
| Later born-maximized contrast | PARTIAL: fixture spawned D maximized after startup, then native readback showed it unmaximized and tiled under the default; exactly one clear is NOT VERIFIED | `leg6-spawn-d.json:1`; `leg6-kwin-journal.log:4,71-72`; `leg6-wininfo-d.json:1`; `leg6-kwinrc-omnitiler.txt` |

- Observability gap: all snapshots used `ReadWorldPresentation`, not full
  correlated `Describe`. The born clear path calls `clearMaximized` and records
  `recordClearAttempt` (`kwin/src/world-apply.ts:1422-1443`), but trace attempt
  emission reads only frame/state rows (`kwin/src/world-entry.ts:863-880`).
  Absence of `state-attempt` lines therefore cannot establish absence of a born
  clear or count clears. Likely owner: KDE adapter diagnostics/evidence capture.
- Slot finding: the two maximized survivors exchanged reserved allocations at
  startup; restoring A exposed its new reservation, not its original tile.
  Core staging sorts identities (`world_observe.rs:636-637`), and admission ties
  on identical maximized rectangles (`seed.rs:430-441`). Likely owner: core
  startup seeding. The current restart discriminator does not specify ordering
  between multiple maximized survivors; exact original-slot preservation is
  unresolved, not classified here as an established policy violation.
- Fixture observation discrepancy: after the successful command clear, KWin
  reported maximize mode 0 while the fixture still reported maximized and its
  windowed predicate timed out (`leg5b-wait-a-windowed.json:1-4`). The fixture
  reads `window.is_maximized()` (`crates/tiler-live-fixture/src/win.rs:1122-1123`);
  likely investigation owner is fixture/winit/KWin configure readback, not a
  proven core restore failure.
- Artifact audit verified all eleven helpers' raw load replies, nonce-bound
  source receipts and named/object absence after unloading. Each returned ID 2,
  reused only after absence; the malformed desktop-selection helper failed and
  was accounted for before correction. An initial wrong shortcut component was
  a no-op; the successful route was `/component/kwin`. Physical input and the
  alternative stored setting were not tested in this check.
- Dev teardown output and final status/process listings were retained; direct
  final verification found dev down with no fixture, dev or ydotoold processes.
  No product code changed. Native acceptance stays open for correlated
  no-clear/clear-once evidence and the multi-survivor slot-order question.
