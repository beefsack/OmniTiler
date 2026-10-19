# KDE legacy route removal

## Goal and scope

- Remove the unshipped KDE plan/workspace-send route and its exclusive tests,
  fixtures, planner transport and obsolete acceptance tooling before 0.1.
- Preserve production world-route behavior, native rendering/settings, float
  persistence, and the per-domain core APIs still used by Windows/macOS.
- Preserve unrelated in-progress work. No platform adoption or parked product
  decisions are part of this change.

## Removal evidence and approach

- `kwin/src/entry.ts` starts only `world-entry` and `tray-publisher`.
  The four plan/workspace-send adapter modules have no production callers.
- `world-entry.ts` uses only endpoint constants and payload formatting from
  `active-group-highlight.ts`; retain that boundary, not its DescribePlan client.
- Native effect group highlighting consumes SetGroupHighlight payloads, not
  DescribePlan. Preserve its validation/rendering and world active-group ops.
- Review the complete exclusive-dependency list before deletion. Retain shared
  helpers and mixed tests; trim legacy-only assertions instead of dropping
  coverage of surviving interfaces.
- Remove DescribePlan-specific KDE service/protocol plumbing only after tracing
  shared float-intent and native-platform callers. Keep all still-used core APIs.
- Retarget current API guards and CI fixtures to the world route. Remove the
  obsolete custom-tile acceptance harness and its exclusive tests.

## Acceptance and verification

- Independent pre-removal inventory review and final-diff review.
- Full KWin suite, typecheck/build; Rust workspace tests/check, format and strict
  all-target Clippy; exporter freshness, native effect/settings tests and shell
  CI-equivalent gates. Inspect native-only Windows/macOS consumers explicitly.
- Baseline: KWin 1962 passed, 0 failed, 305 suites; Linux workspace check passed.
- Live production smoke: two disposable tiled windows, border/group rendering,
  cross-output send and Settings; complete baseline capture and exact restore.
- Update current architecture/spec/decision text and backlog, retain historical
  evidence where it still explains surviving contracts. Report removal delta.

## Bounded units

1. Inventory and independently review exclusive reachability and consumers.
2. Remove legacy TypeScript route, preserve shared production boundaries and
   migrate mixed test/tooling contracts.
3. Remove unused KDE DescribePlan transport without changing portable core APIs.
4. Integrate documentation, run final gates/review, and smoke-test production.

## Outcome

- Removal implemented and independently reviewed before deletion and after
  integration. No still-used code removal or portable API regression found.
- Deleted 73 tracked files, totaling 110,703 lines against the prior revision:
  eight TypeScript source files, 55 TypeScript test files, seven Rust
  protocol/example/test files, two obsolete shell scripts and one fixture.
  Additional exclusive branches were trimmed from mixed modules/tests/docs.
- The group formatter keeps its existing effect schema and 64 KiB cap without
  importing the retired plan module. Native KCM coverage now compares actual
  world registrations with the native catalog, explicitly pinning the existing
  four extra arrow rows and four output-follow chord differences. No shortcut
  behavior was changed. Installed-build source provenance now comes from the
  world ready log rather than the deleted helper.
- World placement diagnostics moved into world_protocol. DiscoverWorld,
  DescribeWorld and Read/WriteFloatIntent retain their authentication, separate
  admission and persistence behavior; DescribePlan and its retained backend are
  gone. No current KDE setting was dead, so none was removed here.

## Accepted offline evidence

- KWin: 1962 -> 649 passed, 305 -> 113 suites; zero failures.
- Rust workspace: 3353 -> 3149 passed; zero failures/ignored. Delta is exactly
  204: service library -11, protocol library -152, removed integration tests -41.
- Final workspace tests/check (all targets), formatting, strict all-target
  Clippy, KWin typecheck/build, exporter freshness and all 14 shell suites pass.
  Shell tray tests used the explicitly built binary, matching CI's route.
- Native effect 34/34 and native settings 27/27 pass.
- Windows/macOS have no callers of the removed protocol/service symbols.
  All Linux-buildable workspace targets compile; Windows-native and macOS-native
  cfg branches were inspected but cannot be compiled on the Linux host. Shared
  core engine/session/active-group APIs and native FFI behavior remain intact.

## Native smoke and restoration

- Production evaluated-readiness and complete correlated world transport proved.
  Two disposable normal clients tiled; Settings opened and closed cleanly.
- Actual output-send shortcut submitted send-directional with a command sequence;
  the accepted reply carried transfer and focus intents, and native readback
  confirmed cross-output arrival plus follow focus. A separate direct host move
  proved only observation convergence, not command delivery. Neither sample
  proves physical keyboard delivery.
- Border eligibility/visibility status was observed. Pixels/paint invocation
  were not verified; group rendering was not demonstrated.
- Protected windows, focus, frames, memberships, outputs/views, pointer, exact
  shortcut inventory, effects and intent-store bytes restored. Controller,
  diagnostic helpers, effect and planner were verified removed. Original desktop
  count/names/order restored and checked twice after teardown. Recreated desktop
  IDs necessarily changed; native persistence added their default tiling groups.
  No other configuration differences were found.
- A second-leg summary initially misread its captured desktop baseline. Raw
  before/after inventories showed retirement during the controller run; missing
  original desktops were recreated after teardown and verified stable. The
  earlier claim that KWin retired them without a controller is withdrawn.

## Remaining acceptance and parked question

- The world route has a pre-existing missing active-group query producer:
  world-entry forwards command results, but neither normal observations nor
  existing commands produce that result. The removed DescribePlan client was
  called only by the unshipped plan entry, never by world production. This is
  not a deletion regression. Native group display is additionally chord/move
  gated; an eligible border status is not rendering proof.
- Should production submit read-only active-group operations on the former
  focus/domain/tree lifecycle triggers, or introduce a native visibility-driven
  trigger? This changes shipped behavior and remains unselected. The minimal
  [discriminator](../spec/reference-outcomes.md#kde-world-route-group-highlight-discriminator)
  retains unsupported rendering outcomes as TBD.
- Removal and offline gates are delivered; full requested visual/group live
  acceptance remains incomplete. Keep this note active until that gap is
  resolved or explicitly separated from removal acceptance.
