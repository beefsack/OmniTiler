# KDE port notes (world contract v2)

How the KDE adapter moves to the strict v2 contract in
[world-adapter-guide](world-adapter-guide.md). Status below is
implemented vs approved-not-implemented vs open as of `01a73798`.

KWin source is read from the local sibling clone at the documented
`v6.7.4` pin; port work must not web-fetch KWin sources. Mechanical
API checks use `git show <pin>:<path>` and `git grep <pattern>
<pin>` inside that clone.

## Recommended topology

- The external Rust service (`crates/omnitiler/src/`, planner) extends
  the existing Planner and owns exactly one `WorldEngine`. The plain
  KWin JS observes, captures and actuates through the async
  session-bus `callDBus`; the native C++ effect
  (`kwin/native-effect/`) is a pure renderer of the serialized core
  presentation. It retains native anchoring plus immediate hide-only
  vetoes.
- A future pager widget/plasmoid consumes cached presentation plus
  notifications with its own client ids; it never invents authority.
- The service side is one logical adapter/broker, not transparent
  multi-client RPC: sole report identity and sequencer, one complete
  stream, one request/observation namespace and one monotonic clock.
  Fold other clients' cursor, gesture and presentation facts into the
  coherent report, preserving invalidation, lifetime and one gesture
  identity. Independent counters collide; an incarnation reset clears
  state.
- Core command dedup is one high-watermark; retained operations use
  `op-<command_seq>`. The broker maps its client id to globally
  ordered commands. Native work is actuator only: route transient
  terminal results to the requester (issuing-reply-only results; a
  cache is insufficient) and never replay one-shot effects to
  subscribers.
- Separate presentation/query/command authorization from the exclusive
  report/actuator authority. A reconnect gets the current accepted
  presentation.
- Current planner endpoint (`crates/omnitiler/src/planner_service.rs`,
  `crates/omnitiler/src/world_service.rs`): only the KWin unique owner
  plus bootstrap/full-report routes. `world_service` still carries
  stale `aligned_per_output_workspaces` and v1 discovery; the port
  removes them and moves JS and Rust together to the v2 wire, with no
  shims and no compatibility route.

## Current KDE inventory (port or delete)

JS adapter (`kwin/src/`):

- `world-seq.ts` still pins `WORLD_CONTRACT_VERSION = 1`: must move to
  2 together with the Rust service. `world-report.ts` still carries v1
  shapes (topology/bindings/domains, `out`/`ws` command fields):
  replace with the v2 report/command vocabulary (screens, facts,
  closes, views, `Select*` with `output: None` for core-resolved
  targeting, direction-only commands, `SendSource` intents).
- `world-observe.ts`: keep the capture skeleton (frame reads,
  inventory, closes, completeness) and re-target it to v2 facts,
  observed logical receipts and presentation ingress. Geometry stays
  in the coherent KWin adapter space (KWin logical pixels).
- `world-apply.ts`, `world-native.ts`, `world-float-nav.ts`: port to
  v2 desires and operation tokens (`frame_operation`,
  `clear_operation`, `op-<cmdSeq>` transfers). One-shot effects are
  spent before dispatch, never replayed.
- `world-entry.ts` (sequencer, invalidation, dispatch, busy fences):
  collapse into the single broker behind the one stream; delete the
  legacy plan route remnants and the v1 `DiscoverWorld`/`DescribeWorld`
  assumptions in step with `world_service.rs`.
- `world-drag.ts`, `world-command-context.ts`, `world-state.ts`,
  `world-shortcuts.ts`, `world-settings-live.ts`: delete adapter-side
  policy copies (admission, gesture resolution, refill, tiling toggle
  decisions). Shortcuts submit planner-resolved world commands only.
- `workspace-native.ts`, `born-maximized.ts`,
  `fixed-size-predicate.ts`, `migration-source-refill.ts`,
  `scope-class.ts`, `same-axis-move.ts`: delete TypeScript policy
  copies; these are core-owned (`workspace_scope.rs`,
  `window_policy.rs`, `output_selection.rs`, `admission.rs`).
  `tray-publisher.ts` keeps native placement/anchoring/styling and
  click-to-command; pager content comes from core.
- `world-born-maximized.ts`, `provenance-entry.ts`, `native-id.ts`,
  `native-alias.ts`: keep only native identity/binding/recovery claims;
  logical homing moves to core receipts.

Native effect (`kwin/native-effect/`, `activewindowborder.*`,
`activeborderlogic.*`, `group_highlight_ffi.h`, `drag_oracle_ffi.h`,
`visual_policy_ffi.h`): delete the current geometry/policy derivation
and consume core bounds from the reply presentation instead. Native
styling (colors, anchoring, artwork) stays native. Pager/plasmoid
work is future and separate.

## Host styling on KDE

- Two independent host styling colors, never ring-coupled (see the
  [generic rule](world-adapter-guide.md#8-border-underlay-drop-preview-and-pager)):
  the active-window ring keeps its existing border color/theme tokens
  (default: live system accent), and the pager focused-window
  highlight gets independent highlight color/theme tokens (default:
  live system accent), each explicitly overridable without touching
  the other. The focused miniature paints a full-strength accent
  border over a dimmer accent fill of the same color; unfocused
  miniatures are unchanged. The pager active workspace cell keeps the
  system accent with no theme switch.

Tests: `kwin/tests/world-*.test.ts` and the `test-fixtures/world-wire/`
v1 vectors (`report-observe.json` is `v:1` with topology/bindings/
domains) must move to the v2 wire or be deleted; do not keep v1
fixtures passing through shims.

## KWin API facts the port relies on

- `workspace.cursorPos` (declared in `kwin/src/kwin-globals.d.ts`,
  backed by `src/scripting/workspace_wrapper.h` at the pin) is the
  cursor sample, and the script side can also call the script-visible
  `workspace.windowAt(pos, count)` hit test
  (`src/scripting/workspace_wrapper.h:483`,
  `src/scripting/workspace_wrapper.cpp:369` at the pin) without effect
  involvement or polling. The local `.d.ts` declares only `cursorPos`,
  so it is incomplete on this point: never treat the declaration file
  as the API ceiling. Neither the JS nor the effect side has a warp
  API: cursor-warp is unsupported on KDE. Never claim an applied
  `cursor_warp`.
- Frame reads use `frameGeometry`; client sizes use `clientGeometry`
  (see `world-observe.ts` `readRect`/`readClientArea`). Writes go
  through the guarded `frameGeometry` setter (`world-native.ts`).
- Desktop select/transfer uses the declared wrappers
  (`currentDesktopForScreen`, `setCurrentDesktopForScreen`,
  `sendClientToScreen`, `createDesktop`/`removeDesktop`,
  `currentDesktopChanged`, `desktopsChanged`) through the
  `workspace-native.ts` per-output capability seam. KWin 6.7+ only
  APIs stay behind that seam, never called directly.
- Geometry is one coherent KWin-logical space. Document and convert
  fractional KWin rounding (`Math.round`/`Math.trunc` in
  `world-observe.ts`, `world-drag.ts`) and keep it inside the adapter.
  Style fields ride the same coherent adapter units as fact frames
  (core presentation math is unit-agnostic); see the corrected
  `PresentationIngress` docs in
  `crates/tiler-core/src/world_report.rs`.
- KWin size hints are client sizes: project decorated frames with
  `clientGeometry`, never with Windows invisible-border compensation.
- Fresh native readbacks (visibility, frame, foreground joined to the
  managed lifetime) prove arrival; setter returns and signal delivery
  alone do not. Async configure is a source-derived KDE/Wayland risk:
  first reproduce it natively and check whether KWin exposes pending/completion
  evidence before proposing a neutral settling representation. No new core
  signal is selected (see deferred work in
  [decisions](../decisions.md#port-deferred-investigations-2026-10-24)).

## Workspace modes on KDE (user-approved 2026-10-24)

- Shared (`shared`) is linked views with the user label "Workspaces
  span displays": all displays show and switch the same workspace.
- `per-output-local` and `global-unique` are separate views with the
  labels "Displays have separate workspaces"; COSMIC calls this
  `WorkspaceMode::Global` vs `OutputBound`.
- `equal_workspace_count` is a host limitation ("equal workspace
  count"), never called shared: same lengths but independent current
  views. On KDE the native global list means `equal_workspace_count`
  is true; expect empty padded workspaces (for example output 2 with
  ws4 occupied while 1-3 stay empty). Do not overspecify lifecycle
  beyond the existing rules.
- KDE default stays `per-output-local`. Both separate-view modes are
  fully supported only with the KWin per-output desktop setting
  enabled; warn when either per-monitor scope runs with that setting
  disabled, through actionable settings if feasible. `shared` needs no
  warning. The warning is required port work; direct settings action is
  conditional on feasibility.
- These workspace decisions are user-approved in
  [decisions](../decisions.md#workspace-vocabulary-2026-10-24) and
  supersede the stale tentative 2026-10-19 global-unique fallback. Core
  linked-vs-separate modes are valid on every platform; no new setting
  is added.

## Approved core changes this port must deliver

- Host-owned native adoption (user-approved 2026-10-24, not yet
  implemented in core): a fresh core generation adopts native
  desktops/Spaces in native order for each display with native window
  membership preserved and no workspace moves at launch; tiling still
  follows the existing startup arrangement inference
  (size/position). Subsequent outside moves are adopted as new
  membership; an outside-created native desktop becomes a workspace.
  The KDE port delivers and accepts against native facts; macOS
  reuses the same core change. Current `native[]` rows that do not
  bootstrap rings or publish `native_binding` are the gap to
  implement, not an uncertain product choice.
- Four occupied native desktops on a fresh core mean four logical
  workspaces with the same members and no workspace-move
  discriminator; the native result stays TBD until KDE-tested.

## Build, check and gate commands

- Linux shell: `devenv shell` (system deps come from `devenv.nix`
  only). KWin script lifecycle and fixture-first rules:
  [live-kwin-testing](../live-kwin-testing.md).
- Planner/service: `cargo build --locked -p omnitiler`,
  `cargo test --locked -p omnitiler`, strict clippy and `cargo fmt
  --check` through the devenv shell.
- KWin script, from `kwin/`: `npm run typecheck` for the static
  declaration/bundle check, `npm run build` for the script bundle,
  and `npm test` for the full `kwin/tests/world-*.test.ts` suite
  (bundles, then runs the compiled `dist/tests` cases). Targeted v2
  legs include `kwin/tests/world-wire.test.ts`,
  `kwin/tests/world-apply-*.test.ts`,
  `kwin/tests/world-observe-caps.test.ts` and
  `kwin/tests/world-entry*.test.ts`.
- `just dev-on [--scope-class NAME]` / `just dev-off`
  (`just/linux.just`) stage and remove the dev-mode script; they are
  live session commands under the live-testing guide, not offline
  checks.
- Core-only gates the port must keep green (no native host needed):
  `cargo run --locked -p tiler-protocol --example world_acceptance --
  test-fixtures/world-actions.json` from the devenv shell, plus locked
  core/protocol builds, tests and strict all-target clippy.

## Material rough edges (no behavior change selected)

- The v2 world wire carries projected `WindowFact`s, not raw `SafetyFacts` or
  rich native classification/rule facts. Define the JS-to-Rust broker capture
  envelope so the Rust owner calls the existing shared policy entry points;
  never replace missing transport with TypeScript policy copies. A shared
  classification-ingress redesign is not implemented here.

- `kwin/src/world-seq.ts` still pins contract version 1 while core
  and protocol are strict v2; `kwin/src/world-report.ts` command and
  reply shapes predate v2 (`out`/`ws` fields, topology/bindings/
  domains, missing `SendSource`, direction-only and operation-token
  vocabulary). Port JS and Rust together; no shims.
- `test-fixtures/world-wire/*` and several `kwin/tests/world-*`
  vectors still speak v1 (topology/bindings/domains, native-derived
  mapping). They cannot gate v2 acceptance without a rewrite.
- Fractional KWin rounding and the style-unit conversion path have no
  documented port proof yet; keep both inside the adapter and record
  the evidence with the port.
- No JS/effect warp API exists; cursor-warp stays unsupported on KDE
  even when the core setting is on.
- The local `kwin-globals.d.ts` omits the pinned `windowAt` hit test;
  the port extends those declarations instead of assuming no hit API.
- The current effect derives geometry and presentation eligibility instead of
  consuming core bounds; delete that derivation, retaining native host styling.
