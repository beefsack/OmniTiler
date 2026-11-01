# Q2: fixed-size float admission (R-SPC-04)

- Follow-up 2026-10-08: D1 predicate setting delivered offline in shared core
  and KDE; [D1 delivery](admission-and-move-settings.md). User decisions in
  [decisions](../../decisions/layout-and-admission.md#fixed-size-admission-predicate) supersede the provisional
  clauses below; D5/D6/D7 changes remain pending.

## Goal and scope

- Deliver the accepted 2026-10-07 fixed-size admission exception in shared
  core, existing Linux planner and KDE. Delivered offline 2026-10-08 under
  tentative PROVISIONAL selections D1-D8, pending user review (not user decisions).
- Windows changes may only repair compilation while preserving behavior.
  Real Windows wiring belongs in numbered backlog handoff item 13.
- No live testing, dependency installs, commits, pulls or adapter extraction.
  Root guidance and the live KWin testing guide were read; initial tree clean.

## Acceptance and approach

- Classify newly admitted eligible fixed-size clients without a tiled slot;
  honor selected hint normalization, lifecycle and overlay precedence.
- Preserve existing transient/dialog eligibility, sticky commands, workspace
  membership and user override rules. Oversized resizable minima still use B6.
- Gaming Compatibility is binding: automatic floating must not resize, move,
  refocus or otherwise interfere with games. Do not reuse intentional-float
  actuation blindly: it writes geometry and keep-above and retains focus.
- Log the admission decision with bounded structured reason/lifecycle context
  and existing correlation tokens, without native IDs or application content.
- Units: research, core/planner implementation and
  transactional repair, KDE lifetime integration and failure/entry fixtures,
  independent review, offline delivery gates, evidence review, records and integration.
- Required delivery gates: npm tests/typecheck, offline workspace Rust
  tests/clippy, fmt, portable check, affected offline shell tests after build,
  native Nix checks if native code changes, staged diff/ASCII checks.

## Reviewed reference and pre-delivery source evidence (2026-10-08)

- Accepted fixture: min=max 640x480 floats in all eight original references
  (`docs/research/reference-wm-consensus.md:161`); baseline REQ-SPC-04 was a
  gap (`docs/spec/functional-spec.md:319`). This does not establish other hints.
- COSMIC @3d55cba0: `src/shell/layout/mod.rs:17-55` treats parents/types/modal
  and equal whole min/max sizes as dialogs. `src/shell/element/surface.rs:565-595`
  drops only Wayland (0,0), not each zero axis. Equal (640,0) and (0,480)
  therefore float; unequal vectors with one fixed axis tile. X11 delegates
  Option hints without this Wayland zero filter; absent hints do not match.
- COSMIC admission: `src/shell/mod.rs:2957-3026` classifies once, excludes
  dialogs from stack joins, branches fullscreen first, then float/tile,
  sticky, maximize, minimize. Its float admission may place/focus the client
  (`:2998-2999,3028-3041`); this is not evidence for our game-safe no-write rule.
- Other predicates: bspwm @e11eff4 `src/rule.c:290-293`, i3 @903bcd51
  `src/manage.c:461-474`, xmonad @284dd52c `src/XMonad/Operations.hs:93-119`,
  qtile @83c697a5 `libqtile/backend/x11/window.py:553-564`, awesome @0a5e50cf
  `lib/awful/client.lua:895-906` use both axes. Hyprland @19fb395d
  `src/desktop/view/window/Window.cpp:1026-1038` uses either axis for Wayland
  with both minima >1, but `X11Backend.cpp:83-95` uses both positive axes for
  X11. Sway @1652c54b `sway/desktop/xdg_shell.c:229-235` uses either axis
  with both minima nonzero. Niri @ed22699d `src/window/mod.rs:377-396` uses
  fixed positive height only. Off-fixture predicates are not unanimous.
- Hint changes: COSMIC's fixed predicate is in initial `map_window`; no
  dynamic reclassification was traced. Awesome
  `lib/awful/client.lua:988-1024` DOES recompute implicit floating on
  `property::size_hints` in both directions unless explicitly overridden.
  Exact resulting frames remain TBD. R-SPC-05's minimum-raise leg is not
  evidence for become-fixed or cease-fixed transitions.
- User toggle: COSMIC `src/shell/workspace.rs:1491-1519` maps fixed floats
  directly into tiling without checking hints; fullscreen focused toggle
  refuses. Existing project unfloat is fresh admission
  (`docs/decisions.md:1373-1417`, core `session/ops/float.rs:18-84`). A new
  generic hint check in every admission would accidentally undo this override.
- Maximize: COSMIC `src/shell/mod.rs:2998-3022,4470-4498` admits fixed clients
  floating before applying maximize, recording Floating as original layer.
  Q3 selects born-maximized reserved tiled slots in
  `docs/decisions.md:563-573`; intersection with fixed-size is not specified.
- Fullscreen: COSMIC `src/shell/mod.rs:2960-2968` admits born fullscreen with
  no restore state; `:2754-2774` exits to tiling on a tiled workspace without
  rechecking hints. A previously floating client restores its floating layer
  (`:2775-2807`). These are different lifecycles, not one float outcome.
- Sticky: COSMIC `src/shell/mod.rs:3016-3018,4769-4868` applies sticky after
  fixed-float classification and restores previous layer on sticky-off.
  Our recorded native-sticky adoption and Meta+G/Meta+Shift+G origin rules
  (`docs/decisions.md:1395-1417`) remain authoritative.
- Workspace enable: COSMIC `src/shell/workspace.rs:1440-1454` retiles EVERY
  ordinary floater, fixed clients included, without rechecking hints. Our
  intentional-float exceptions survive workspace toggles (R-FLT-04 Ours
  cells); deciding whether automatic fixed floats share that identity matters.
- Native workspace/rule assignment: COSMIC resolves activation workspace
  before classification (`src/shell/mod.rs:2907-2945`) and float-only app
  exceptions are ORed with dialogs (`:2957-2998`). No project tile/float rule
  engine was found; native KWin rule interactions remain TBD. Do not invent
  rule actions or broaden existing dialog gates for Q2.
- Startup: KDE observes existing foreground/hidden clients and resyncs
  (`kwin/src/plan-adapter-entry.ts:4010-4053`). COSMIC fresh map is not proof
  of a KWin script-owner stop/start adoption journey. Float-identity restart
  persistence is separate accepted R-RST-01 work, not a Q2 implementation.

## Tentative provisional selections (2026-10-08), pending user review

These are tentative selections for implementation, not user
decisions. Each is recorded literally "Provisional, to discuss" in
[decisions](../../decisions/layout-and-admission.md#fixed-size-admission-predicate), with review in backlog.

| ID / discriminator | Selected provisional clause / review alternative |
|---|---|
| D1 / R-SPC-06,07 | Present usable nonnegative equal whole vectors, not full-zero/sentinel; partial-zero counts. No resizeable inference. Either-axis Hyprland-Wayland/sway alternative reviewable; no setting now. |
| D2 / R-SPC-08 | Admission-only in both directions; hint projection/clamp continues. Dynamic awesome-style identity is an alternative, not a consensus. |
| D3 / R-SPC-09 | User tile/sticky-off wins for same live client through hide/show, cross-domain observation and workspace re-adoption; new client classifies. Existing sticky origin rules unchanged. |
| D4 / R-SPC-10 | Fixed-floating base under native maximize. Q3 and R-SPC-04 both follow COSMIC: only fixed-size birth touches the Q3 boundary. Non-fixed Q3 and R-MAX-03 stay unchanged; later fixed/maximized retile reserves the Q3 slot. |
| D5 / R-SPC-11 | Born-fullscreen fixed exits tiled on tiled workspace; prior fixed-floating restores floating; no automatic writes while fullscreen. |
| D6 / R-SPC-12 | Workspace enable retiles automatic fixed floats; intentional/sticky floats keep existing rules. Minimal automatic-vs-explicit distinction. |
| D7 / R-SPC-13 | Foreground/hidden startup adoption classifies; restart recomputes absent authoritative override. No Q2 tile-override persistence; R-RST-01 float identity work separate. |
| D8 / R-SPC-04 qualifier | Membership-only automatic classification: no admitted-client geometry, focus, stacking or keep-above writes. Explicit commands keep existing behavior. |

## Delivered implementation and Windows boundary

- Shared `crates/tiler-core/src/size_hints.rs` adds `is_fixed_size` over raw
  bounds; clamp's positive-only normalization stays separate. Session
  observation/admission create slotless floating exceptions with automatic
  origin and retained tile overrides, committed transactionally through
  `PendingDesired`; paired send/split carries and partitions those marks.
- Engine policy is opt-in OFF by default. Existing Linux planner actually
  resides in `crates/tiler-protocol/src/planner_protocol.rs`; `Planner::new`
  enables it, optional `fullscreen`, `sticky`, `fixed_auto`, `fixed_suppress`
  observations carry native overlay/origin facts. No new extraction/refactor.
- KDE `kwin/src/plan-adapter.ts` keeps one exact-reference record per live
  client, never evicting on scoped/minimized absence. Raw observation plus
  automatic identity drives explicit float/sticky commands. Metadata for user
  operations stages in the flight and commits only on `planned-applied`,
  surviving the existing one-replan path without leaking on failures.
- `kwin/src/plan-adapter-entry.ts` wires verified native removal and
  confirmed-release workspace enable. Foreground/hidden startup uses the
  existing observation route. Native sticky adoption becomes intentional;
  failed sticky writes retain automatic identity.
- Diagnostics: KDE `plan:fixed-size-classification` uses `phase=classify` /
  `classified` before dispatch; Planner `fixed-size-admission` reports
  committed membership. Both use correlation/bounded counts/reason only;
  native application/failure is a separate existing terminal, no per-frame log.
- Windows compile-only edits initialize the four new EngineWindow fields
  false in `tiling.rs`, `workspace_owner.rs` and two `tiling_sys.rs` test
  literals. Engine remains OFF; native behavior unchanged. Backlog item 13
  identifies min-only `query_outer_min_track` / `min_hint_for` / hidden row
  assembly, max-side DPI/inset/budget/lifetime work and no-touch wiring before
  any Windows opt-in. Linux gates do not establish native Windows acceptance.

## Review, outcome and verification (2026-10-08, offline)

- Sequential research pins were verified read-only. Actual
  COSMIC predicate/getters/admission/toggle/workspace/maximize/fullscreen source,
  Hyprland backend predicates, sway xdg predicate, awesome dynamic updater,
  core admission/convergence/float logic, KDE hint/actuation/startup paths and
  Windows min-only query/row assembly reviewed.
- Initial research claims about partial-zero hints, Hyprland
  X11 support, dynamic awesome hints, workspace re-enable and fullscreen birth corrected.
- The initial eager core markers/global binding relaxation and
  KDE per-domain eviction/hint-change classification rejected. Core metadata now stages
  atomically; opt-out retains original binding/refusal behavior and tests.
  KDE uses exact live-reference identity plus explicit wire provenance.
- Fabricated Planner replies rejected as no-touch evidence. Real Planner
  subprocess fixtures and production-entry release/startup fixtures now assert
  planned/applied terminals, sibling writes, target no-write accounting and
  actual desired slots. Independent review found no proven blocker; follow-up
  pair-transfer, maximized-retile and entry fixtures close material gaps.
- Failed rollback-on-command-failure approach was replaced with success-staged
  metadata because timeout teardown bypassed the rollback path. Regression
  rows cover timeout/late reply, native sticky failure, single/double drift,
  retry and stage survival through replan. No unresolved failed approach.
- R-SPC-06..13 record the shortest discriminating sequences. Untraced source
  and native legs stay TBD; no UT evidence is invented. KDE offline outcomes
  link actual fixtures, and D1-D8 remain PROVISIONAL pending user review.
- Documentation review: special-windows has 13 scenarios / 182 Then bullets;
  Q2 adds 8 / 112. Coverage is 147 scenarios; requirement totals are 74
  NORMATIVE / 61 OPEN / 17 PROVISIONAL, with eight explicit Q2 clauses.
- `npm --prefix kwin test`: 1067 passed, 150 suites, zero failed/skipped.
  Production ES2017 bundle and real offline Planner fixtures pass.
  `npm --prefix kwin run typecheck`: production and test projects pass.
- `cargo test --workspace --offline`: 1201 passed, 46 successful suite
  results, zero failed. Q2-specific coverage: 23 shared Session/Engine rows,
  7 Planner rows, predicate unit coverage, 21 KDE adapter rows (including
  predicate) and 2 production-entry rows. Windows portable gates stay green.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`,
  `cargo fmt --all -- --check`, `just check-portable`: pass; no warnings,
  formatting errors, normal core dependencies or platform leaks.
- `cargo build -p omnitiler --offline`, then all nine verified offline
  `scripts/*.test.sh`: pass. Counted assertions 1713 (tray 29 fixture + 16
  self-test, dev-loop 380, dogfood 572, native-dev 163, host-build 93,
  live-harness 237, Custom Tile 131, floor-ratio 92); build-kpackage contracts
  also pass without a numeric count. No live host commands were run.
- Native Nix checks conditional not applicable: no native effect/KCM edits.
  `git diff --cached --check` passes; staged added-line ASCII scan is empty.
- Decisions, matrix/spec indexes, diagnostics, Q2 status, Windows handoff item
  13 and user-review/native-check queues updated. No dependency install,
  commit, push or live acceptance; implementation has no blocking question.

## Pending user review and live checks

- Review every D1-D8 tentative choice, especially equal partial-zero hints,
  the deferred either-axis alternative and fixed-size/Q3 maximize boundary.
- Native fixed/game admission must leave frame, focus and stacking alone;
  gain/loss hints, tile/sticky overrides, hide/show/domain adoption/new refs,
  owner restart, foreground/hidden startup and correlated terminal evidence.
- Fixed birth under maximize stays floating; later floating-workspace retile
  while maximized reserves the Q3 slot; native unmaximize lands in it.
  Born-fullscreen exits tiled, prior fixed-floating restores floating.
- Workspace enable retiles automatic only; intentional/sticky floats preserve
  existing rules. Native/Windows checks remain user-owned in backlog item 13.
- Next action: review the staged delivery;
  user reviews provisional choices and performs live checks.
