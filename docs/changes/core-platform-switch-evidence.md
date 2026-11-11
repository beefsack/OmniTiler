# Core platform switches: evidence awaiting user decisions

- Status: evidence/options only, 2026-11-12; two separate decisions under
  [C6](core-v4-review.md#user-decisions-2026-11-12). No new behavior selected.
- Scope: source/history research at `4c419cee`; no implementation, contract or
  fixture changes. Native results below are historical reports, not new tests.
- Acceptance: trace both switches through callers, distinguish host facts from
  policy, check stable Tier 1 sources, record minimal discriminators and options.
  [Layering](../principles.md#layering) and [Simplicity](../principles.md#simplicity)
  apply. Native delivery and unresolved reference outcomes remain TBD.

## Current wiring (citations at the inspected revision)

| Item | Evidence |
| --- | --- |
| Definitions/defaults | `crates/tiler-core/src/world_engine.rs:1656-1680`: `WorldEngineConfig`, send=false/fence=true. `new` delegates to it; constructor sets both Engine values (`:1832-1847`), reset retains config (`:2038`). Engine fields/defaults: `engine.rs:104-111,459-460`; Session: `session.rs:781-797,957-958` (both false before world configuration). Core-relative paths below start at `crates/tiler-core/src/`. |
| Propagation | `engine.rs:669-692,904-908` setters update retained/new sessions; `session/world.rs:825-849` setters/getters; pair/split copy both (`session.rs:1356-1357,1585-1588`). |
| KDE / macOS / Windows | KDE `crates/omnitiler/src/world_broker.rs:439-441` and macOS `crates/tiler-macos/src/world_runtime.rs:230-240` use defaults: false/true. Windows explicitly uses true/false (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:80-88`, bootstrap/reset `:212,632`). This is the existing Windows world route, not evidence of a completed v4 port or native parity. |

## 1. `fixed_send_admission`

- **Difference:** with a slotless automatically fixed-size float F on tiled WS1,
  explicitly send F to tiled WS2 (or another output). False refuses automatic
  origin; true permits the existing state/size-preserving deferred transfer.
  Both retain sticky, explicit-tile-override and other safety refusals.
- **All gates:** `session/world.rs:681-737` checks retained exception/home/base
  first; flag arms are held born-fullscreen (`:710-714`, observed `fixed_auto`),
  ordinary float (`:719-725`) and float with exactly one overlay (`:729-735`,
  observed or retained automatic origin). True removes only automatic-origin
  exclusions, never `fixed_suppress`. Caller `session/ops/workspace.rs:144-155,919-947`
  routes exceptions and preserves flags/size without tree mutation; Engine's
  focus-sync fallback also checks eligibility (`engine.rs:3324-3339,3372-3377`).
  v4 workspace/output sends reach those paths (`world_sends.rs:529-536,671-697,788-795,911-937`).
  Either endpoint in workspace-floating mode bypasses the switch through
  membership-only transfer (`:584-634,829-877`); this is not a universal send ban.
- **Why / classification:** introduced by `b6539be1` (2026-10-16) for Windows
  ordinary fixed/Settings sends, deliberately preserving KDE/macOS behavior
  ([original record](windows-all-member-workspace-sends.md):5-12,25-39).
  Constructor profile accepted in `075f4c32`, implementing `0869f08c`
  ([record](archive/windows-full-world-state-replies.md):37-83). Recorded reason
  is product rollout/isolation, not host inability. No recorded host limitation
  justifies false on KDE/macOS. The later [send decision](../decisions/workspaces.md#workspace-sends-and-floating-state)
  (`:68-73`) selects automatic sends everywhere; `docs/porting/world-contract.md:72-75`
  agrees. Classification: historical platform preference, now stale policy residue.
  Windows same-output fixed-Settings carry was reported in the original record
  `:96-113`; it does not prove all hosts, cross-output, or transient-owner behavior.

## 2. `maximized_focus_fence`

- **Literal difference:** maximize tiled B, with a left sibling; directional
  focus left. Session true refuses `Unchanged`, false allows normal planning.
  At an edge with an occupied adjacent output, true also refuses crossing,
  false permits the cross plan subject to ordinary gates. Fullscreen exempt.
  These are Session discriminators, **not two different v4 native outcomes**.
- **All gates / reachability:** `session/ops/focus.rs:94-108` gates local tiled
  focus, `:490-501` cross-output focus from tiled or float origins. But v4
  `world_directional.rs:154-164` unconditionally refuses native maximized focus,
  and `world_operations.rs:535-547` returns before either Session path. Thus
  changing the constructor flag alone changes neither native v4 outcome.
  Composed maximize is separate: floats refuse (`world_operations.rs:561-572`),
  tiled subjects skip local siblings and may cross (`:620-703`), independent of
  the flag. This includes macOS, whose raw maximize is null and capability false
  (`crates/tiler-macos/src/runtime_core.rs:326-352`); null does not mean maximize
  policy is absent. `window_policy.rs:357-359` is an independent pure predicate,
  not this switch; its core calls at `:603,1175` are inside tests (`:491`).
- **Why / classification:** `1c26f2d9` (2026-10-10) implemented G-06's product
  focus fence, keeping Windows unchanged pending adoption, with KDE flags then
  adapter-local ([original record](archive/reference-comparison-implementation.md):42-45,78-79;
  `1c26f2d9:docs/decisions.md:1495-1511`). October 18 constructor rationale was
  preservation of adapter-owned gates, not an inability to focus other windows
  (constructor record `:54-79`). No host incapability is recorded. Classification:
  historical policy/adoption switch plus accidental stale/duplicate fencing.
  **November 1 explicitly rejected a universal no-op** and selected skip-local,
  cross-if-adjacent, else stay (`docs/decisions/window-state.md:29-35`,
  `docs/spec/functional-spec.md:154,325`). Native v4 contradicts that selection;
  composed tiled behavior already has its shape. C6's "both live" is true for
  Session reads, not for an observable native v4 flag difference; its untraced
  world short-circuit is now established. Neither forcing true nor merely
  deleting the flag repairs the native/composed discrepancy.

## Reference evidence and separate choices (no recommendation)

- [Minimal sequences and stable-tag citations](../spec/reference-outcomes.md#core-platform-switch-discriminators)
  extend R-WS-05/R-MAX-08/R-OUT-11 rather than substituting fullscreen for maximize.

| Tier 1 source | Fixed-float send to tiled target | Maximized focus: single output / adjacent output |
| --- | --- | --- |
| i3 4.25.1 | Allowed, float/size retained | Unsupported maximize fixture / unsupported |
| sway 1.12 | Allowed, float/size retained | Unsupported maximize fixture / unsupported |
| Hyprland v0.56.2 | Allowed, float/size retained, monitor-relative position | Stays at defaults / final target/state TBD |
| COSMIC epoch-1.10.0 | Allowed, fresh tiled admission rather than float retention | Stays for default-layout Left / final target/state TBD |

| Decision option | User consequences and deletions | Wire/fixtures and macOS adoption |
| --- | --- | --- |
| Send: shared true | KDE/macOS gain tiled-scope automatic-float sends; Windows retains them. Collapse three arms to `!fixed_suppress`; delete send field, accessors, propagation and profile override. No evidenced host prohibition; native carry/owner coupling still unproven. Matches the existing send selection, unlike shared false. | No new wire field; remove profile-specific expectations, cover shared send outcomes. macOS must deliver retained membership/placement without inventing intent or resize. |
| Send: shared false | KDE/macOS unchanged; Windows loses approved fixed-float/Settings sends at tiled boundaries. Delete same plumbing, retain automatic-origin refusal arms. No evidence any host requires this restriction; revises the existing send selection. | No wire addition; shared refusal coverage replaces enabled cases. macOS gains no send behavior. Floating-workspace bypass still requires explicit treatment if a universal refusal is intended. |
| Send: neutral capability | Candidate `independent_window_transfer`: host can reassign this member's workspace/output without resizing, changing float/overlay state or forcibly moving an owner group. Not an automatic-origin preference. Delete platform profile; gate shared send policy on a declared host fact. A per-window limitation cannot truthfully be encoded as a blanket platform false. | New typed capability (possibly per-window), codec/contract/fixture and producer work. Windows has bounded same-output positive evidence; KDE/macOS truth values, cross-output and owner-dependent exceptions are TBD, **not** false inferred from current defaults. |
| Focus: shared true or false literally | True retains universal native v4 refusal; false removes Session fences but world refusal survives. Delete focus config/storage/propagation either way; neither alone changes native v4 users. Removing the world fence as well under false enables ordinary sibling traversal, also contrary to the selected skip-local policy. No evidenced host prohibition for either action. | No wire addition; revise internal profile tests and add world-level native/composed, same/cross-output discriminators. macOS composed paths are unaffected unless deliberately unified. Both literal choices need the November 1 selection reconciled. |
| Focus: delete switch, implement already-selected shared policy | Replace terminal native fence and Session cross refusal with skip-local/cross-or-stay under existing gates, consistent with composed tiled route. KDE/Windows native-maximize origins gain adjacent-output escape; local focus still stays. Composed floats' refusal must be reconciled too, not silently retained as parity. Deletes focus profile/plumbing and duplicate policy rather than adding a host distinction. | No new wire field; shared fixtures must distinguish local stay, cross, destination overlay preservation, native/composed and float bases. macOS keeps null native maximize and uses existing composed ownership; native delivery remains unverified. |
| Focus: neutral capability | Candidate `overlay_preserving_focus_transfer`: host can focus an eligible destination while preserving source/destination maximize overlays. False means a demonstrated host restriction, never "prefer a fence". Delete profile; shared routing owns policy, capability narrows only impossible transfers. None of KDE/Windows/macOS has an evidenced false value; reports remain TBD until grounded. | Handshake/codec/contract/positive-negative fixture and producer work, and duplicate fences still need reconciliation. Existing `maximize` distinguishes native vs composed, not permission to leave an overlay (`connection.rs:91-124`). macOS must report its actual focus ability independently of native maximize support. |

- Wire evidence: neither switch is in closed handshake keys
  (`crates/tiler-protocol/src/connection_protocol.rs:211-267`); a capability adds
  coordinated schema and conformance work, unlike deletion. Existing fixture:
  `test-fixtures/world-wire/v4-active/messages/handshake-emulated.json:1`.
  Removing both switches also removes the otherwise empty `WorldEngineConfig`
  and its constructor/reset profile plumbing; no automatic-origin facts vanish.
- Verification: local pinned-source/history inspection; `git diff --check`.
  No suites or live tests. Unsupported references, native carry/focus and
  capability truth values stay TBD. Next: obtain the two separate user choices;
  reconcile the focus choice explicitly with November 1 before implementation.
