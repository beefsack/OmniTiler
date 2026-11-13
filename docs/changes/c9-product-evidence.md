# C9 product evidence

- Status: fixed-largest and fixed-float removals implemented and independently reviewed offline. Shared Rust, exporter and KDE TypeScript gates pass; the native recovery fixture correction below closes the isolated native gate (37/37). Requirement batch 1 is decided User 2026-11-13, implemented and independently reviewed with all shared offline gates passing below. The scoped KDE batch-1 checks below establish both focus histories, explicit-command recency, closed-candidate fallthrough, swap and adjacent-output entry; hover, history-free fallback and literal single-output coverage remain open. Batch 2 below records latest-release evidence, two unresolved implementation boundaries and one normative urgency deferral, with no runtime changes. Other native adoption and 23 requirement choices remain open.
- Goal: remove both capacity settings and their storage/UI/wire propagation, preserving ordinary-anchor-first then largest-splittable fallback and fixed float on exhaustion. Keep the remaining product evidence for subsequent decisions.
- Scope: shared policy, settings surfaces, protocol witnesses and offline regressions. Preserve ordinary anchoring, B6 minimum handling, next-decision timing and `noSpace` result provenance. Retire selectable capacity overflow and proven-unreachable accepted-overlap machinery. Missing decision "Why" entries are not backfilled.
- Acceptance/evidence: ordinary anchor wins when splittable; otherwise largest area descending, child-order DFS then lifetime ID. Removed input reports UnknownField without losing valid siblings; removed stored keys are not adopted. Retired alternate policies and controls disappear. Historical fixture families remain unchanged; active witnesses come from their real producers. Add before/after regression evidence, run shared Rust and KDE checks, and review the complete settings/policy diff. Native outcomes not established by source remain TBD.
- Approach: bounded shared-core removal and regression; platform storage/UI removal; genuine active-witness regeneration; owning decision/contract updates; integrated checks and review.
- Reading key: `C/` = `crates/tiler-core/src/`; `W/` = `crates/tiler-windows/src/`; `M/` = `crates/tiler-macos/src/`; `R/` = `docs/spec/reference-outcomes/`. Line citations describe the inspected revision `7a6d6e12`, not future locations.
- Reference releases: i3 `4.25.1`, sway `1.12`, Hyprland `v0.57.2`, COSMIC `epoch-1.11.0`. Release/tag metadata checked 2026-11-12: i3/COSMIC remote tags, sway/Hyprland latest stable release metadata ([sway](https://github.com/swaywm/sway/releases/tag/1.12), [Hyprland](https://github.com/hyprwm/Hyprland/releases/tag/v0.57.2)). All four source clones were available; source read at tags, not working-tree versions. [Reference authority](../decisions/reference-and-shortcuts.md#reference-outcome-authority) requires latest stable, but its example Hyprland/COSMIC versions are stale; those examples are not a fixed-pin exemption.

## Fixed-largest implementation

- Removed `AnchorFallback`, closest-relative/no-search branches, Engine/Session
  policy storage and propagation, catalog/options and Settings key ordering.
  `session/capacity.rs` retains ordinary-first strict+hinted admission followed
  by area/DFS/lifetime ranking. No-space, overlap retention, minimum handling,
  named-send destination scope and exact-target R4/drag rules are unchanged.
- Removed KDE XML/descriptors/control/read/write/normalization and Windows
  schema/validation/catalog/control/logging plumbing. KDE never reads or
  rewrites the old stored key; Windows serde ignores it and drops it when
  saving. Existing removed-key handling is used, without aliases or migration.
  KDE artifacts were regenerated with `cargo run -p tiler-kde-settings-exporter`
  and its `--check` verified them. Native Qt/C++, Windows and macOS builds and
  native consumption are not established by the offline checks.
- Two added regressions failed before removal: SettingsState treated removed
  `anchorFallback` as known, and the world admission test obeyed its no-search
  value. After removal, valid siblings survive with unknown-field diagnostics
  and the world uses fixed largest fallback. The normal full Rust suite passed
  3377 tests, zero failures/ignored, one filtered. Existing largest/tie and
  no-space/overlap coverage remains; Windows saved-key and KDE unread/unwritten
  key regressions were added (native Windows/Qt tests unrun).
- Frozen `world-wire/v4`, `stage5`, and existing `stage5-v4` remain unchanged.
  Active world-wire Settings previously inherited five frozen messages; its
  manifest now points to five new active messages. The explicit typed
  [Settings recorder](../../crates/tiler-protocol/tests/connection_settings_v4_record.rs)
  constructs intended requests and uses `encode_settings`, including deliberate
  invalid/unknown-field cases. Removed-key behavior is separately covered by
  core unit/world tests. Normal mode verifies fresh bytes; explicit recording
  uses `UPDATE_SETTINGS_V4_FIXTURES=1` with the full Rust suite.
- An initial read/delete-key/re-encode writer and whole-document reserialization
  were rejected and replaced by that typed recorder. The ten scenario diffs now
  remove only 21 Settings lines; three eval JSONL lines change only Settings
  payloads, with surrounding formatting/key order restored. Manifest differences
  are five active pointers plus retired schema/coverage metadata; README records
  provenance and invocation. The new messages use compact canonical bytes.
- Existing genuine KDE byte/migration and stage5 recorder routes refreshed the
  active families. KDE changes only remove the setting; stage5 also carries
  genuine volatile generation/source identities, with replay reading the recorded
  manifest generation. No unrelated key-order churn remains.
- Final offline gates passed: `cargo fmt --all -- --check`;
  `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`;
  `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`;
  `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`
  (3377 passed, zero failed/ignored, one filtered); in `kwin/`, `npm run typecheck`
  and `npm test` (1302 passed, zero failed/skipped, 191 suites). Independent
  complete-diff review found no functional defects; its informational fallback
  terminology comment was clarified. Native build/runtime gaps remain as above.
- [D2 and D10](../decisions/layout-and-admission.md#best-effort-admission-fallback)
  explicitly supersede their old alternatives/exposure. Native adoption still
  needs fixed-fallback exhaustion, removed controls/keys and valid no-space
  behavior checked on each platform; reference/native outcomes below stay TBD.

## Native build repair and KDE follow-up (2026-11-13)

- The removed control left 15 exposed KDE fields, but the generator, generated
  header, native descriptor test and static KCM test still asserted 16. Corrected
  those assertions, regenerated through the exporter and passed its `--check`
  and 14 unit tests. Shifted the remaining warning row to close the UI grid gap.
  Independent review found no blocking count, index or layout issue.
- An isolated host-matched native build succeeds, including both settings
  modules. Native tests report 36 passed and one pre-existing failure, not a
  passing full native gate. The recovery test consumes unchanged `stage5-v2`
  fixtures: `347af034` advanced covering revisions from 5/8 to 6/9 without
  updating its three hardcoded fence assertions. It does not consume the newly
  recorded `stage5-v4-active` family; that unrelated test was left unchanged.
- The preloaded border branch of `just dev` skips the native build, leaving the
  existing staged settings modules stale. This isolated build did not replace
  them or reload the mapped border library. A fresh out-of-process KCM can use
  the rebuilt config plugins through an isolated plugin path; ordinary staged
  use requires an explicit native build and a fresh settings process. Refreshing
  the mapped border library still requires a new desktop session.
- Live largest-fallback admission passes with fixture-only windows: successive
  nested admissions exhaust the focused anchor's planned tile to one pixel on
  its split axis. The next admission splits the largest remaining tile while
  the exhausted anchor stays unchanged. Native PID/title/UUID/frame joins,
  production requested-frame traces and before/after presentations agree.
  Client minimum clamping is observed separately; minimum hints alone were not
  treated as proof that an anchor cannot split.
- Rebuilt settings-page load/absent-control inspection and no-space Apply are
  NOT RUN. Native offscreen KCM tests verify control absence and retained
  no-space save behavior in private test configuration, not live page adoption.
  Manual acceptance remains: inspect the rebuilt page, confirm the anchor
  control is absent, then exercise no-space Apply and next-admission timing.
- Follow-up format, all-target build and strict clippy pass for core, protocol,
  CLI and exporter. The permitted Rust suite reports 3377 passed, zero failed
  or ignored, one filtered; KDE typecheck and 1302 tests across 191 suites pass.
  The [shared live follow-up](core-platform-switch-evidence.md#kde-follow-up-2026-11-13)
  records readiness, helper-evidence qualifications and remaining native gaps.

## Fixed-float implementation

- User 2026-11-13 fixes geometric exhaustion to float and removes
  `admission.no-space` / `noSpaceResult`, not the diagnostic `noSpace` result.
  Shared enum/catalog/typed state, Engine/Session propagation and selection
  branches are removed. Admission preserves opening frame; D7 repair floats
  the shortest newest-first eligible victim prefix, preserving last live frame,
  protected members and native focus. Ordinary anchoring, fixed-largest
  fallback, minimum settlement and next-decision-only timing remain.
- KDE XML/descriptors are generated through the exporter, with all exposed-field
  assertions reduced from 15 to 14 and the remaining UI rows closed up. KCM and
  live acquisition neither read nor write the retired stored key. Windows
  schema/validation/control/acquisition edits are mechanical and uncompiled:
  unknown stored fields are ignored and omitted on save, removed controls no
  longer offer overlap, and removed adoption logging no longer claims pickup.
  macOS has no setting surface to remove; shared-core admission and repair now
  target fixed float there too. No native runtime acceptance is claimed.
- Against saved pre-removal production, the identical final three regressions
  fail: removed-key unknown-field/valid siblings, exhausted admission with
  retired `allow-overlap`, and newest-victim repair with retired `allow-overlap`.
  The admission `float` iteration already passes that baseline. After removal
  all three pass in the permitted full shared suite. The typed Settings recorder
  and genuine KDE/stage5 producers refreshed only active fixture families;
  immutable `world-wire/v4`, `stage5` and `stage5-v4` remain unchanged.
- Removed selection-only tests cover enum/default/setter choices, overlap
  admission/retention, send/focus/resize overlap and the R4 overlap arm; removed
  `d9_overlap_golden.rs` tested only the retired projector. KDE reader/live-timing
  overlap-alphabet tests and Windows no-space choice tests are removed rather
  than skipped. The matrix retains float discriminators and explicitly retires
  the overlap alternative; unsupported reference/native outcomes remain TBD.
- Active fixture scope: five compact world-wire Settings messages, ten scenario
  Settings payloads and three eval Settings lines lose only `noSpaceResult`;
  manifest drops its enum coverage. KDE byte Settings and migration Settings
  lose only the key. Stage5 full/s2 discovery Settings lose the key; genuine
  source/request/clock/attempt nonces refresh, and top-level generations are
  normalized as its README specifies. Dispatch/projection bytes change only
  generation identity, not layout or behavior; no key-order churn.

### Capacity-overlap route audit

- Accepted-overlap state was generation-local `Session`/pending state, initialized
  empty and never serialized or supplied by IntentStore, seed/report or Settings.
  Only the removed selection produced a new mark; moves/pair sessions copied
  existing marks. Native overlapping frames and B6 minimum overconstraint do not
  produce this authority. Consequently no retained accepted allocation remains
  reachable in this implementation; no further product choice is required.
- Locations below identify the inspected pre-removal sources, not current lines.

| Route | Pre-removal source | Reachability after selection removal / action |
| --- | --- | --- |
| Admission / explicit tile | `session/ops/lifecycle.rs:746,836,1140` | Only selected overlap produced marks; delete branch/mark and use fixed float |
| Convergence / startup seed | `session/world.rs:1907,1934,2016` | Only selected overlap produced marks; delete branch, retain strict seed and D7 |
| Send / deferred send | `session/ops/workspace.rs:323,379,489,624,670,773,1013` | Selection or copied mark only; delete overlap fallback/propagation, retain transfer fences |
| Move / R4 | `session/ops/move.rs:302,330,345,347,482` | Selected overlap or already accepted mark only; delete, retain exact wrap and float fallback |
| Focus | `session/ops/focus.rs:144,541`; `session/world.rs:1116` | Prior mark only; strict projection remains |
| Orientation | `session/ops/toggle_orientation.rs:155` | Prior mark only; strict projection remains |
| Drag | `session/ops/drag.rs:153,327,517,810,958,1053` | Prior mark only; strict preview/drop/snap-back gates remain |
| Resize | `session/ops/resize.rs:154,215,557,618,941,1004` | Prior mark only; strict geometry and minimum handling remain |
| Repair | `session/capacity.rs:411,414-436,508` | Selection-only producer/shelter removed; newest-first float repair and safety holds remain |
| Boundary projection | `boundary.rs:826-843` | Prior mark only; strict projector remains |
| Pair/pending state | `session.rs:1344,1357-1358,1614-1620` | Internal copies, no external ingress; remove marks and policy state |

- With no producer or accepted-state ingress, positive-overflow helpers in
  `geometry.rs` and `size_hints.rs` have no production callers and are deleted.
  Invalid bounds/topology, unrepresentable native targets and protected impossible
  remainders still hold rather than inventing rectangles.
- Final integrated evidence: exporter generation/check and 14 tests pass (14
  exposed fields); formatting, all-target build and strict clippy pass for core,
  protocol, CLI and exporter. Exact full shared suite passes 3366 tests with
  zero failures/ignored and one filtered unsafe-posture check; compliant typed
  recording and normal verification both pass. KDE typecheck and 1297 tests in
  189 suites pass. The count reductions remove retired alternatives, not skips.
- Isolated host-matched native build succeeds including both settings modules;
  all 37 native tests run, 36 pass and only the known pre-existing
  `native-effect-stage5-recovery` failure remains (the unchanged assertions
  described above). This is not a passing full native gate. No native runtime
  acceptance or Windows/macOS build is claimed.
- Independent complete-delta review found no blocking functional/safety issues;
  strict projection/B6, invalid/unrepresentable/protected holds, removed-key
  handling, every overlap-state ingress and fixture semantic stability were
  checked. Native platform consumption remains unverified.

## Native recovery correction and scoped follow-up (2026-11-13)

- `347af034` made the stage5 recorder fulfill eager spare creation through its
  live desktop inventory and `desktopsChanged`. The extra settled genesis capture
  advances subsequent full-chain revisions by one (`internalReads` 12 to 13):
  `stage5-v2/full/dispatch-cover.json` records 6 and `dispatch-cover2.json` records
  9. The native recovery test still expected 5/8. Corrected only its three exact
  fence assertions and explanatory comments; fixtures, stale-paint rejection,
  foreign-instance rejection and malformed-input display-hide/fence retention
  remain unchanged. This is a stale oracle, not evidence of a recovery defect.
- Fresh isolated host-matched build via `scripts/nix-host-kwin-build.sh resolve`
  then `build --source kwin/native-effect --build-dir <fresh-build>
  --expected-identity <resolved-identity>` passes. In the resolved derivation's
  development shell, configure with its `KWin_DIR` and `BUILD_TESTING=ON`, build,
  then run `QT_QPA_PLATFORM=offscreen ctest --test-dir <fresh-test-build>
  --output-on-failure`: 37/37 pass, versus 36/37 before the assertion correction.
  No staged or mapped native library was replaced. No Rust or TypeScript suite
  was required or rerun for this native-test-only correction.

| Scoped Wayland check | Observed evidence and limit |
| --- | --- |
| Controller evaluation | Both initial start and restart have current-owner ready/discovered plus accepted dispatch/applied pairs. Clean helper-lifecycle acceptance is incomplete as described below; readiness alone does not establish it. |
| Complete no-space admission | NOT RUN. Exhausting every splittable leaf with ordinary work-area/gap inputs was not practical as a lightweight direct leg. No `noSpace`, opening-frame preservation or exhaustion-focus result is claimed. |
| Shrink repair | NOT RUN. No fixture strut/layer-shell or established dev-only work-area/gap input was available without changing display configuration or stored settings. |
| Minimum-only sanity | Observed two declared minima whose combined heights exceed their column, yet both remain tiled with proportional, disjoint native allocations matching requested frames. This is bounded minimum-versus-capacity sanity, not proof of native clamp settlement or the complete B6/M18 journey. Presentation captures were empty and helper evidence is qualified. |
| Rebuilt settings page | NOT RUN / inconclusive. An out-of-process offscreen attempt retained private HOME/XDG/KDEHOME, private runtime and D-Bus isolation plus the isolated build's plugin path before launch. Module discovery/page-load and absent controls were not established. No Apply was performed. Passing native KCM tests are not live page adoption. |

- Evidence filenames: `leg1-readiness.txt`, `leg6-readiness.txt`,
  `leg4-spawn.jsonl`, `leg4-native-3win.json`, `just-dev-stdout.log`,
  `leg5-isolation-receipt.txt` and `leg5-kcmshell*.log` in the externally retained
  scoped follow-up bundle. The [restart finding](retained-intent-desire.md#scoped-native-maximize-restart-reproduction-2026-11-13)
  is separate from capacity admission.
- Lifecycle evidence is not compliant: the first helper's post-unload named
  state parsed empty rather than false, but testing continued instead of stopping
  on the accounting mismatch. Later receipts record false and all five record
  returned ID 2, not distinct increasing IDs. Numeric-path pre-load collision
  checks and successful raw load/introspection replies were not retained; generic
  introspection errors could be mistaken for object absence. Later cleanup does
  not repair that historical gap. Do not treat these native observations as an
  unqualified acceptance pass.
- Trace copies exist for both incarnations before teardown, with saved restart
  and final `dev-off` output. Subsequent direct read-only checks establish dev
  DOWN, all five named helpers unloaded, their recorded script object absent,
  and no fixture, planner, tray, log-follower or input-helper processes. No
  settings, physical-input or pending-target acceptance follows from that cleanup.

## Part 1: admission settings (historical two-item decision evidence)

- Selection: User 2026-11-12 removed anchor-fallback and fixed largest; User 2026-11-13 removed no-space and fixed float. The latter supersedes D3's overlap alternative, D7's selectable overlap retention, D9's capacity-overflow projection and D10's no-space exposure, preserving float placement/focus/provenance, victim ordering/protection, strict safety holds, B6 minima and next-decision timing. The tables below describe the inspected pre-removal revision unless stated otherwise.

### Meaning, defaults, authority and exposure

| Setting | Values in plain language | Definition/default | Minimal action sequence |
| --- | --- | --- | --- |
| `admission.anchor-fallback` | `largest-splittable-window`: try the biggest usable tile first; `closest-splittable-relative`: try nearby branches of the tree first (not screen distance or MRU); `no-search`: do not try another tile, use the no-space result immediately | `C/admission.rs:22-62,211-277`; catalog `C/settings_catalog.rs:371-387,589-605`. Default largest; equal areas use child-order DFS then lifetime ID. Ordinary anchor always tried first | [Existing discriminator](../spec/reference-outcomes.md#best-effort-capacity-discriminators-ours-to-be-open), lines 761-762: B cannot split, sibling C can, outside A is larger; open N with B focused. Largest splits A, relative splits C, no-search floats N under the no-space default even though room exists elsewhere |
| `admission.no-space` | `float`: keep the newcomer outside the tile tree at its opening frame; `allow-overlap`: keep tiled membership and positive rectangles, permitting overflow/covering other tiles | `C/admission.rs:65-101`; catalog `C/settings_catalog.rs:389-400,606-622`. Default float. This is geometric exhaustion, not declared-minimum infeasibility | Existing lines 763/770: in a positive 1x1 usable domain with gap 8, admit zero-hint N beside A: float N versus positive overflowing tiles. Separately shrink an existing domain: float newest eligible victims versus retain overlapping tiles |

- Support **exists**: [D2](../decisions/layout-and-admission.md#best-effort-admission-fallback), `layout-and-admission.md:157-162`, explicitly selects largest; [D3/D4](../decisions/layout-and-admission.md#best-effort-d3-d4-no-space-result-and-focus), `:164-169`, selects float and preserves overlap; D5-D10 `:171-211` specify persistence, route coverage, repair, startup, overlap geometry and next-decision timing. [Original record](archive/best-effort-admission.md):6-7,80-109 records the user request, float preference and all selections. These are not settings invented without authority.
- No Tier 1 attribution for these alternatives/defaults was found in those decisions, their original record or the option tips (`C/settings_catalog.rs:371-400`). The original source audit explicitly distinguishes reference anchor splitting from alternate-target search/capacity floating (`archive/best-effort-admission.md:42-48`). Existing selections remain authoritative even if new evidence fails the normal new-setting support threshold ([tiers](../decisions/reference-and-shortcuts.md#reference-tiers-and-defaults)).
- Timing: Apply alone never retries floats, rebuilds trees or evicts tiles; next admission or actual changed-bounds/gaps repair reads the setting (`C/session/capacity.rs:128-156,414-464`; matrix line 774). A retained overlap allocation is sheltered under unchanged geometry even after selecting float. Minimum-only overconstraint remains B6 under both values.

| Platform | User exposure and current limitation |
| --- | --- |
| KDE | Tiling & Workspaces / Admission, two KCM combos: `kwin/native-effect/unifiedsettings.ui:75-98`; generated descriptors `kwin/native-effect/generated/kde_settings_descriptors.h:109-110`; module save/load `kwin/native-effect/unifiedsettings_module.cpp:1582-1583,1637-1640`. Native keys `anchorFallback` / `noSpaceResult`, defaults/choices `kwin/contents/config/main.xml:56-72`; live acquisition/normalization `kwin/src/world-settings-live.ts:180-205,355-392` |
| Windows | Settings selectors and saved `core.anchor_fallback` / `core.no_space_result`: `W/settings/schema.rs:228-303`, `W/settings_ui/choice.rs:120-176`, `W/settings_ui/tabs.rs:426-433`; invalid saved tokens refuse the file (`W/settings/validation.rs:48-58`). Exposure is not runtime acceptance: settings logs say adopted (`W/tiling_sys/settings.rs:593-628`), but the current report builder supplies defaults (`W/tiling_sys/world_sequencer.rs:379-380`); native pickup and the pending v4 port are not proved here |
| macOS | Neither exposed by the portable catalog (`C/settings_catalog.rs:183-187,1175-1189`); adapter sends raw acquisition, omitted values use core defaults (`M/world_connection.rs:108-128,239-251`). No native settings pickup demonstrated |

### Every production policy gate (not merely the UI)

- Ingestion/propagation: `C/settings_state.rs:18-19,53-54,80-81,234-239`; accepted settings into Engine `C/world_engine.rs:2069-2103`; Engine/session defaults and propagation `C/engine.rs:785-815,909-910`, `C/session.rs:1360,1611-1612`. Wire keys are `anchorFallback` and `noSpaceResult` (`crates/tiler-protocol/src/connection_protocol.rs:515-524`). Core validates per field, retaining last-good on invalid input (`C/settings_state.rs:128-172,270-291`); KDE acquisition normalizes invalid tokens to defaults; Windows saved-file validation is stricter. Do not equate these input journeys.
- **Anchor setting:** ordinary-first selector `C/session/capacity.rs:53-126`, admission wrapper `:665-732`; all three policy-input sites are `C/session/ops/lifecycle.rs:525-538` (fresh/rejoin/explicit-tile admission), `C/session/world.rs:1893-1906` (observation/startup seeding), `C/session/ops/workspace.rs:352-364` (named-send destination only). Candidate must pass whole-tree strict and hinted projection; minimum flags alone do not disqualify (`C/admission.rs:104-150`). R4/drag do not search for a different promised target.
- **No-space admission/transfer:** `C/session/ops/lifecycle.rs:563-574,732-747` (float versus overlap admission), `C/session/world.rs:1916-1981` (convergence seed), `C/session/ops/workspace.rs:371-380` (destination exhaustion), `C/session/ops/move.rs:450-484` (R4 float fallback only after exact-wrap projection fails; overlap was already tried by its projection gate, not an unconditional refusal).
- **No-space repair:** `C/session/capacity.rs:442-527`: overlap projection or shortest newest-successful-admission-first eligible victim prefix; protect unsafe/reserved/fixed-override members. Invalid bounds or unrepresentable overlap still hold, never fabricate native rectangles.
- **No-space retained projection:** strict first, then overlap when selected **or previously accepted for that domain**, via `C/session/capacity.rs:735-793`. Call sites: `C/boundary.rs:826-831`; `C/session/world.rs:1162-1171`; `C/session/ops/lifecycle.rs:1141-1151`; `workspace.rs:322-325,475-478,625-626,671-672,767-775,1007-1015`; `move.rs:301-304`; `focus.rs:161-163,565-567`; `toggle_orientation.rs:155-157`; `drag.rs:152-155,327,517,810,958,1053-1054`; `resize.rs:154-155,215-216,557-558,618-619,941-942,1004-1005`. These cover removal, send/deferred send, local/cross-output focus/move, orientation, drag preview/drop/snap-back, keyboard/pointer/corner resize. Removing the setting does not automatically justify deleting retained-overlap handling.

### Tier 1 comparison for the same exhaustion sequences

- Fixtures/actions already discriminate the values (matrix lines 761/763); their exact reference/native outcomes remain TBD. Latest-tag path evidence below, also recorded [at the matrix](../spec/reference-outcomes.md#capacity-source-recheck-2026-11-12), is not a claim that native hosts can realize a 1x1 work area or that all degenerate client sizes settle.

| Reference/tag | Exhausted-anchor / no-room evidence | Limit of conclusion |
| --- | --- | --- |
| COSMIC `epoch-1.11.0` | `src/shell/layout/tiling/mod.rs:407-435`: map to tree then update positions; `:2998-3012,3052-3087`: subtract outer/inner gaps during layout | No alternate-anchor capacity search/float-result gate on this inspected map/layout path. Exact tiny geometry/client settlement TBD; not evidence for a scrolling solution |
| Hyprland `v0.57.2`, Dwindle | `src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:83-116,138-150`: choose opening node, max-size-hint float diversion, wrap it; `:28-62`: recursive splits with nonnegative-size clamp | The max-size diversion is not zero-hint geometric exhaustion. No D2/D3 counterpart on this path; exact post-gap sizes/native settlement TBD |
| i3 `4.25.1` | `src/render.c:224-250,402-435`: divide parent extent by normalized child percentages and assign split rectangles | Arithmetic allocation, not a search for another splittable leaf. Does not prove final client extent or absence of later sanitization; native result TBD |
| sway `1.12` | `sway/tree/arrange.c:54-95,139-180`: reduce gap budget before fraction-based allocation; dimensions below 10 zero both | Different compensation: squeeze gaps then permit zero allocations, not the same positive-overflow or float choice. Exact client result TBD |

- These inspected paths do **not** establish a Tier 1 vote for largest/relative search or float versus OmniTiler's positive-overflow projector. Nor does absence on those paths prove no comparable feature anywhere. The named host boundary is OmniTiler's requirement to publish positive, representable rectangles through another WM/OS, unlike a compositor owning its own allocations (`archive/best-effort-admission.md:121,128-138`); this is not a claim that every host rejects the same coordinates.

### Options and consequences (no verdict)

| Item | Keep as setting, with cited default | Remove setting and fix behavior | Other evidence-led option |
| --- | --- | --- | --- |
| Anchor fallback | Keep all three values, cite D2's user-selected largest default, explicitly label host compensation rather than a Tier 1 variant. No code/wire deletion | Fix **largest**: remove relative/no-search branches, catalog options, storage/UI and setting propagation; preserve D2 default. Fix **no-search**: additionally remove fallback ranking/ring search, but float/overlap occurs even with room elsewhere; closer to inspected ordinary-anchor-only paths, not proved native parity. Fix **relative**: removes ranking branch but changes current default and placement locality | Keep a narrower expert control rather than normal UI: same behavior/wire complexity remains and hidden/manual input becomes another exposure decision |
| No-space | Keep float/overlap, cite D3's user-selected float default; preserve next-decision-only timing and diagnostic capacity origin. No deletion | Fix **float**: remove overlap choice and selection plumbing; positive-overflow admission/repair/projection and retained marks become deletion candidates only after auditing all routes above. Fix **allow-overlap**: delete float-on-capacity branches/newest-first eviction specific to D3/D7; existing ordinary/fixed floats remain. Changes usability, geometry/navigation and restart outcomes | Investigate **gap squeezing first**, prompted by sway: changes supported gaps and fit semantics, requires a new discriminator/geometry fixtures before selection. It does not itself solve zero-extent exhaustion; final behavior still needed |

- Per-platform deletion scope for either setting: shared enum/catalog/typed settings/Engine-session setters; KDE XML, generated descriptors, KCM controls/save/load/live readers; Windows schema/validation, controls and acquisition. macOS has no control to remove, but changing the fixed behavior changes its shared-core target too. Defaults-only removal has less observable impact on macOS/current Windows report builder than on working KDE controls; no platform-specific replacement default is implied.
- Wire/fixture impact: removal deletes the relevant settings-request keys/order and settings validation journeys, not a new wire feature. Update bootstrap/settings vectors (`crates/tiler-protocol/tests/world_codec.rs:241-242`; `tests/data/world_eval_v4.jsonl:3`), catalog/export goldens and choice/default tests. Fixed-default-only can retain behavior fixtures but must remove alternate-setting journeys. No-space removal may simplify capacity-float/overlap fixtures depending on the fixed choice; `noSpace` result provenance (`C/world_report.rs:2412-2419`) is not the setting key and survives if capacity floats survive. No protocol/fixture edits are made here.
- Existing D2/D3/D7/D9/D10 decisions, catalog help and contract settings/timing text would need an explicit superseding decision if removed or fixed differently. No pre-0.1 compatibility migration is assumed.

## Part 2: exact OPEN inventory

- **19 requirement rows** have Status `OPEN` in `docs/spec/functional-spec.md` after batch 1's four selections, batch 2's urgency deferral and batch 3's four selections; the original inventory below had 28. Count is not every occurrence of OPEN (many normative rows have an open platform cell). No additional OPEN requirement IDs were found in the reference subfiles. The spec introduction distinguishes selected R-MIN-04/R-INS-09 capacity/burst behavior from their still-unproved reference/native outcomes, and counts all 184 requirements (165 NORMATIVE including qualified REQ-OUT-05, 19 OPEN). Batch 2 does not decide the two focus rows; batch 3 leaves the feasible-minimum conflict OPEN.
- Table: spec citations use `F:` = `docs/spec/functional-spec.md` line. Reference citations use `R/` defined above and are existing revision-bound evidence, **not newly rechecked release evidence**, except the release-qualified batch 1 and 2 entries below. `Choice` means user product selection; `source` means further targeted latest-source work; `live` means user/native testing. "Absent" is bounded to inspected command/model surfaces, not a universal assertion about every adapter.
- Code key **V**: complete command surfaces `C/world_report.rs:2009-2171` + `C/boundary.rs:41-149` contain no preselect, cycle, container-focus, equalize, rotate/mirror, master, layout-select or column/viewport verbs. **D**: `C/directional.rs:1179-1246` implements leaf directional focus, not viewport/container focus. **T**: absent column model/verbs on V/D; existing per-window sends `C/world_report.rs:2013-2018,2048-2066` do not implement whole-column semantics. These are code absence checks, not acceptance of the feature gap.

| Batch | ID / F line | Plain open question | Existing evidence | Implemented behavior / remaining need |
| --- | --- | --- | --- | --- |
| 1 DECIDED | REQ-FOC-01 / 148 | User 2026-11-13: recent focus | `R/focus.md:17-79`: COSMIC geometric/no history, other Tier 1 history | Focus-only core recency replaces first-child when history exists; exact latest COSMIC/native fixture TBD |
| 1 DECIDED | REQ-FOC-02 / 149 | User 2026-11-13: stay, no wrap/setting | Historical tally is not a revalidated default vote; latest Hyprland explicit zero-gap variant wraps (below) | Existing edge retain confirmed; adjacent-output crossing unchanged; shipped Hyprland geometry/native result TBD |
| 1 DECIDED | REQ-MOV-02 / 167 | User 2026-11-13: swap, retain mover focus | `R/move.md:47-81`: all Tier 1 swap | Existing R2a `SwapNeighbor` confirmed; latest Hyprland exact geometry TBD |
| 1 DECIDED | REQ-MOV-05 / 170 | User 2026-11-13: single-output edge no-op | `R/move.md:168-211`: all Tier 1 visually unchanged | Existing no-adjacent-output no-op confirmed; latest Hyprland exact rebuilt geometry TBD |
| 2 STOP | REQ-FOC-03 / 151 | Need next/previous cycle; which order, wrap, floats? | [Latest release recheck](#requirement-batch-2-focus-and-attention-2026-11-13): COSMIC none, Hyprland creation-order incl floats, i3/sway parent-orientation tiled walk | Substantial synchronized command/shortcut surface; no implementation or selection |
| 2 STOP | REQ-FOC-04 / 152 | Need parent/child container focus? | [Latest release recheck](#requirement-batch-2-focus-and-attention-2026-11-13): COSMIC/i3/sway group then remembered child; Hyprland none | Leaf-only model would change; no implementation or selection |
| 2 DEFERRED | REQ-ACT-02 / 461 | Urgency clears on focus or stays? | [Latest release recheck](#requirement-batch-2-focus-and-attention-2026-11-13): COSMIC workspace marker retained, Hyprland/i3/sway per-window clear | NORMATIVE scope deferral beyond 0.1; native behavior unchanged, future product timing unselected |
| 3 DECIDED | REQ-INS-07b / 138 | Target-local remembered tiled anchor; inactive admission never focuses/switches/replays | [Batch 3 latest release recheck](#requirement-batch-3-admission-and-fullscreen-2026-11-13): COSMIC/i3/sway target focus; Hyprland cursor-closest variant | Observed destination supported, no app-rule API added; inactive automatic-focus guard and shared regressions; native rule pickup TBD |
| 3 DECIDED | REQ-SPC-02 / 497 | Splash and utility float under existing safety/ordinary-focus policy | [Batch 3 latest release recheck](#requirement-batch-3-admission-and-fullscreen-2026-11-13): splash 4/4 float, utility 3/4 float | Core type hold removed, KDE candidates widened; ASSUMPTION no type-specific focus exception; native focus/switcher TBD |
| 3 DECIDED | REQ-SPC-03 / 498 | Ordinary reported facts, not PiP title/app inference | [Batch 3 latest release recheck](#requirement-batch-3-admission-and-fullscreen-2026-11-13): ordinary predicates/rules, exact apps TBD | ASSUMPTION no shipped app exception/new subsystem; metadata/fact discriminator; actual app/native TBD |
| 3 STOP | REQ-SPC-05b / 509 | Retain prior feasible-minimum reallocation or supersede it to match Tier 1 no-reflow? | [Batch 3 latest release recheck](#requirement-batch-3-admission-and-fullscreen-2026-11-13): tiled allocations retained in four inspected paths | Earlier Placement and minimums decision honors feasible minima; no new behavior selected or implemented; native journeys TBD |
| 4 | REQ-INS-08 / 138 | Need preselected split, one-shot or persistent? | `R/insertion.md:309-341`: Hyprland one-shot, i3/sway persistent | Absent (V); Choice + source |
| 4 | REQ-RSZ-04 / 185 | Need equalize/balance action? | `R/resize.md:192-290`: several lower-tier alternatives, Tier 1 no counterpart recorded | Absent (V), existing pixel resize distinct; Choice + source |
| 4 | REQ-LAY-02 / 193 | Need rotate/mirror; which scope? | `R/layout-commands.md:78-135`: bspwm full legs, Hyprland narrower | Absent (V); Choice + source |
| 4 | REQ-LAY-03 / 194 | Need promote-to-master/root? | `R/layout-commands.md:137-182`: xmonad/awesome master, Hyprland root | Absent (V); Choice + source |
| 3 DECIDED (from 5) | REQ-MAX-12 / 331 | Preserve fullscreen across workspace return/tiling toggle; parent orientation no-op | [Batch 3 latest release recheck](#requirement-batch-3-admission-and-fullscreen-2026-11-13): state retained; parent mutation splits 2/2 | Existing commands confirmed by shared regressions; ASSUMPTION orientation no-op, native-free fullscreen suppression veto unchanged; native outcome TBD |
| 5 | REQ-WS-07 / 214 | Shell listing and activation: switch workspace or pull window? | `R/workspaces.md:377-431`: native filtering; SW_HIDE accepted for 0.1 | Native-shell behavior, no listing/activation verb (V); keep-SW_HIDE already selected F:214; remaining Choice + live, post-0.1 |
| 5 | REQ-LAY-04 / 195 | Which workspace-local alternative layout? | `R/layout-commands.md:184-227`; expressly parked until a genuine second layout | Layout-select absent (V); already parked by User 2026-10-09, not a fresh 0.1 choice |
| 6 | REQ-COL-01 / 517 | Need admission into a scrollable three-column strip? | `R/column-mechanics.md:22-60`: non-voting scrolling lineage | Absent (T); Choice whether in scope before source work |
| 6 | REQ-COL-02 / 518 | Need column-width presets? | `R/column-mechanics.md:64-104`: scroller preset forms | Absent (T); Choice |
| 6 | REQ-COL-03 / 519 | Need consume/expel between columns? | `R/column-mechanics.md:109-156`: scroller variants | Absent (T); Choice |
| 6 | REQ-COL-04 / 520 | Focus off-screen window by identity? | `R/column-mechanics.md:160-200`: scroller scroll-to | Leaf focus exists (D), no viewport (T); Choice |
| 7 | REQ-COL-05 / 521 | Need explicit column/window centering? | `R/column-mechanics.md:204-243`: one-shot scroller centering | Absent (T); Choice |
| 7 | REQ-COL-06 / 522 | Need tabbed/stacked column display? | `R/column-mechanics.md:247-290`: different scroller displays | Absent (T); Choice, coordinate already-selected post-0.1 tabbed-stack design |
| 7 | REQ-COL-07 / 523 | Send whole column to workspace/output? | `R/column-mechanics.md:294-340`: whole-column versus window/group | Per-window send only (T); Choice |
| 8 | REQ-COL-08 / 524 | Scroll viewport without moving focus? | `R/column-mechanics.md:344-387`: gesture/scroll variants | Absent (T); Choice |
| 8 | REQ-COL-09 / 525 | Need virtual rows and cross-row sends? | `R/column-mechanics.md:391-428`: index-based workspaces versus row model | Workspace IDs/index sends exist `C/world_report.rs:2023-2066`, no row verb (T); Choice |
| 8 | REQ-COL-10 / 526 | Need app-native tabs nested in stacks? | `R/column-mechanics.md:432-468`: per-window gates, unresolved native tabs | No native-tab command (V); Choice + app/live if selected |

- Batch order: 1 settles already-running daily navigation/moves; 2 handles related focus/attention completeness; 3 admission/classification and observed-host correctness (plus MAX-12 brought forward from 5); 4 optional layout verbs; 5 remaining explicitly deferred items; 6-8 non-voting column mechanics. The original inventory used at most four rows per batch; batch 3 covers five. Latest-source checks now cover batches 1-3 and MAX-12, not remaining batches 4-8.

## Requirement batch 1: historical evidence and options

- All four choices below were resolved User 2026-11-13; the tables retain the
  pre-selection implementation/options and release-qualified evidence, not open decisions.

- Existing shortest discriminators cover all four original questions. Added minimal R-FOC-01 unequal-share and R-FOC-02 fallback-variant rows where rechecking uncovered a finer ambiguity: geometric-nearest is not first-child, and a partial-width fallback miss is not a full-width stay. Evidence is about the exact fixtures, not a universal tree policy. Source mechanics below were checked at release tags; unchanged historical cells retain original versions.

| Row / minimal sequence | COSMIC `epoch-1.11.0` | Hyprland `v0.57.2` | i3 `4.25.1` | sway `1.12` |
| --- | --- | --- | --- | --- |
| FOC-01: `H[V[A,B],C*]`; focus A,B,C, left; fresh run B,A,C, left | Geometric descent, no history: `src/shell/layout/tiling/mod.rs:2022-2061`; center includes -1 offsets, so do not claim universal exact-distance ties. Matrix A/A; exact release fixture TBD | History method 0 default (`src/config/values/ConfigValues.cpp:634`); eligible directional candidates selected by history `src/desktop/state/WindowQuery.cpp:146-195`: B/A for the qualifying equal-half fixture | Perpendicular descent uses focus list: `src/con.c:1737-1797`, hence B/A | Sibling target + inactive focused view: `sway/commands/focus.c:193-200`, hence B/A |
| FOC-02: single output `H[A*,B]`, focus left | Exhaustion in `next_focus`, `src/shell/layout/tiling/mod.rs:1835-2086`; output fallback `src/input/actions.rs:746-795`; no next output (`src/shell/mod.rs:2389-2418`); matrix stays A | `src/config/shared/actions/ConfigActions.cpp:488-533`: default `general:no_focus_fallback=false` (`src/config/values/ConfigValues.cpp:181`) permits opposite-edge candidate search. Zero-gap/no-reserved equal halves imply **B**, not matrix's A (`src/desktop/state/WindowQuery.cpp:71-93,146-195`); true variant stays A. Shipped-gap/reserved exact geometry/native result TBD | Wrapping on (`src/config.c:229`), directional wrapping `src/tree.c:503-577`: B | `sway/config.c:274` WRAP_YES; `sway/commands/focus.c:177-220`: B |
| MOV-02: `H[A,V[C,B*]]`, move B up | Two-child swap `src/shell/layout/tiling/mod.rs:1781-1790`: B above C | Ordinary move removes/reinserts at focal point, with direct-partner side override: `src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:558-608,162-180`; not the separate swap verb. Historical matrix B above C; exact release geometry TBD | In-parent leaf swap `src/move.c:310-339`: B above C | Same-parent leaf `list_swap`, `sway/commands/move.c:112-124`: B above C |
| MOV-05: single output `H[A*,B]`, move left | Default Vertical workspace layout (cosmic-comp's `cosmic-comp-config/src/workspace.rs:41-44`), exhausted left becomes output move (`src/input/actions.rs:813-850`); no other output -> no move (`:614-628`, `src/shell/mod.rs:2389-2418`) | Monitor point query chooses closest monitor even outside its box (`src/state/MonitorQueryCore.cpp:64-69,99-131`); same-monitor remove/reinsert (`src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:569-602`). Historical fixture visually unchanged, but not a structural no-op; exact rebuilt release geometry TBD | No sibling -> output attempt (`src/move.c:342-346`), no output returns (`:206-213`): no-op | No next output -> false (`sway/commands/move.c:278-297`): no-op |

| Row | Existing implementation | Options and consequences, no verdict |
| --- | --- | --- |
| FOC-01 | Stable first-child, not COSMIC's general geometric-nearest algorithm (`C/directional.rs:1179-1211`, explicit adaptation comment). Matrix `R/focus.md:73-77` records A/A | Select **history**: change core perpendicular descent/history inputs plus fixtures, no new wire verb required; preserves B/A across run histories. Select **current stable order**: no code change, but record deviation from three other Tier 1 defaults and distinction from COSMIC geometry. Select **geometric**: needs retained geometry and tie rule; the added unequal-share discriminator implies B under stated rectangles while current first-child selects A, exact native result TBD. A setting needs a specifically supported alternative, not equating stable-first with COSMIC geometry; Hyprland method 1 is overlap length, not necessarily OmniTiler stable-first. Tier rule favors history if the 3-versus-1 source split is accepted; not an automatic product selection |
| FOC-02 | Edge return, no local focus write (`C/directional.rs:1213-1246`; historical `R/focus.md:118-121`) | Fix **stay**: no code/wire deletion, cite COSMIC and Hyprland's named no-fallback variant, not a proved Hyprland shipped-default vote. Fix **wrap**: add core wrap descent with shared fixtures; no new command shape. **Stay/wrap setting**: wrap has i3/sway support; stay has COSMIC plus named Hyprland variant (or historical Tier 2 stay evidence), with default/variant support checked separately. New catalog/wire/UI fields and default decision required. Do not rely on the old 2-versus-2 tally. Scope this single-output row separately from multi-output crossing; do not silently wrap before a valid output transfer |
| MOV-02 | R2a swap (`C/directional.rs:874-886`; `R/move.md:79-80`) | **Select existing swap**: record authority, no code/settings/wire deletion; focus remains mover, geometry ratios still need exact fixture evidence. **Leave OPEN for exact latest Hyprland fixture/user test**: no behavior change. A non-swap alternative has no support in this fixture's recorded Tier 1 evidence; would require a new justification, not an inferred setting |
| MOV-05 | NoAdjacentOutput no-op (`C/directional.rs:1075-1088`; `R/move.md:202-210`) | **Select visually unchanged at a single-output edge**: no code/wire/settings change; separate structural no-op from Hyprland rebuild. **Leave OPEN for exact release/native outcome**: no code change. Cross-workspace behavior is a different fixture/configuration (COSMIC horizontal workspace layout), not evidence to silently change this default-Vertical left-edge leg |

- Shared-core changes for either focus choice would affect all platforms; KDE/Windows/macOS native delivery remains independently unproved here. Spec platform cells are historical and cannot establish current macOS absence during its active port. New setting choices would require native controls/acquisition on each supported surface and shared conformance journeys; fixed choices need only behavioral fixtures, not new wire fields. MOV selections alone do not require new fixtures or protocol changes; existing journeys remain useful evidence, not retroactive live acceptance.

## Requirement batch 1: decisions and implementation (2026-11-13)

- Goal/acceptance: implement history-sensitive perpendicular focus and ratify
  the three unchanged edge/swap behaviors, without changing eligibility,
  move/send/admission anchors, cross-output entry ranking or persistence.
  `H[V[A,B],C*]`: A,B,C then left selects B; fresh B,A,C selects A.
  [Focus decisions](../decisions/focus.md#perpendicular-directional-focus) and
  [move decisions](../decisions/layout-and-admission.md#in-group-move-and-single-output-edge)
  own the four dated selections and reasons. All four spec rows are NORMATIVE.
- Approach: use existing focus memory if adequate; otherwise minimal focus-only
  history, then shared scenario regressions, integrated offline checks and
  complete-diff review. Existing tiled `focus_stack` is inadequate: accepted
  hover advances the logical cursor before that stack changes on native arrival.
  Advancing the old stack earlier would also change unrelated anchor/close and
  cross-output consumers. `Session::focus_history` therefore stores only tiled
  leaf recency per workspace/domain, oldest first, with no wire or adapter state.
- Recording: gated `record_canonical` records accepted explicit, hover,
  admission, close and published focus choices before native effects. Accepted
  changed foreground observations seed history through that same gated path;
  unchanged lagging reports and rejected observations do not refresh it.
  Direct Session `remember_focus` choices and verified focus commits also record;
  `sync_focus_from_window` only synchronizes the execution subject and never
  refreshes this history.
  Topology commits prune; domain relocation and pair/unpair remap identities.
  Closed, floated or moved-away entries cannot win selection. No persistence is
  added: restart starts with no history, then accepted focus seeds it anew.
- Selection: `plan_focus_with_history` chooses the child containing the newest
  eligible descendant at every perpendicular level, falling back to stable first
  child without history. Parallel descent keeps the bordering edge, even when
  a different leaf in that child supplied recency. Only local directional focus
  consumes this new history; pure callers without history keep stable order.
  Cross-output focus continues its independent projected-edge candidate policy
  and existing MRU/overlap/DFS ranking; workspace-return lifetime memory and
  move/send/admission anchoring are unchanged. Hidden/held/float and overlay
  eligibility rules are unchanged; no contract tie-rule text needed updating.
- Regressions: the identical added tests fail when perpendicular selection is
  temporarily returned to first-child (five failures, including both-history,
  nested and closed-MRU coverage; the new history API is retained for compilation).
  A further discriminator fails with legacy `focus_stack` selection: accepted
  hover before native confirmation selects stale B instead of A. Final tests
  cover both histories, no-history fallback, nesting, explicit command focus,
  accepted hover and lagging foreground, closed/floated/moved MRU candidates,
  and unchanged move/send geometry. Initial test syntax/setup failures are not
  acceptance evidence; final integrated results follow below.
- Fixtures: immutable `world-wire/v4`, `stage5` and existing `stage5-v4` remain
  unchanged and pass the shared suite. No mutable fixture or recorder refresh
  was needed. One existing scenario expectation changes: recovered foreground
  in `H[B,V[C,A]]` enters remembered A rather than old first-child C.
- Platform target: shared core changes KDE, macOS and Windows focus descent;
  no adapter edits. Native actuation/adoption remains unverified. Follow-up KDE
  fixture-only testing should establish both histories in fresh `H[V[A,B],C]`
  fixtures, verify left chooses B/A, exercise explicit/hover recency, close MRU,
  focus edge stay, in-group swap with mover focus and move-edge no-op. Check an
  adjacent-output entry separately against the unchanged edge-ranking decision.
- Final sequential verification: `cargo fmt --all -- --check`;
  `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`;
  `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`;
  `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`;
  then in `kwin/`, `npm run typecheck` and `npm test`: all pass. Rust reports
  3388 passed, zero failed/ignored, one filtered; KDE reports 1297 passed in
  189 suites, zero failed/skipped. Complete-diff review and correction recheck
  leave no blocking findings; whitespace review is clean. Native runtime and
  Windows/macOS builds were not exercised for this change.
- Review correction: execution-subject synchronization no longer writes the new
  recency, and split sessions immediately prune stale source/target entries.
  A direct Session discriminator fails with the sync writer restored and passes
  without it. The proposed end-to-end lagging-foreground reproduction was not
  established: report convergence resets the Session subject first, and refused
  requests discard their cloned mutations. This correction separates accepted
  focus authority; it is not proof of that proposed runtime failure.

## KDE batch-1 native check (2026-11-13)

- Goal: native adoption evidence for `a82747f7`, checked from `7a743b0d`;
  no implementation changes. Wayland fixtures only, with exact class filtering,
  on a workspace appended beyond the pre-start roster. Two outputs were present;
  no display changes were made. Shortcut invocation bypassed physical input, so
  these results do not establish physical shortcut delivery.
- Evidence bundle: externally retained KDE batch-1 native evidence. Each step's
  `*-commands.jsonl` preserves calls, raw replies and exit status;
  `*-native.json`, `*-fixture.jsonl` and `*-presentation.json` preserve independent
  native PID/title/UUID/frame joins, fixture observations and core allocations.
  Journal sources below are in `final-journal.jsonl`; corresponding accepted
  outcomes are in `saved-planner.log`. Presentation revisions are checkpoints,
  not pending-operation or settlement proof by themselves.
- Controller readiness: current-owner `ready` and `discovered`, accepted
  `src-1-1ikd84j` and matching `observe-applied` revision 1 appear after the
  pre-start cursor (`readiness-journal.jsonl`, planner log). A separately retained
  current-owner `QT_CATEGORY=kwin_scripting` check is empty. Every tested fixture
  joined one-to-one by sentinel title and fixture PID to its native UUID;
  normal/resizable and test-workspace membership were read back before use.
  Native custom-tile association was false, consistent with direct geometry;
  tiled membership claims below come from core controlled allocations.

| Leg | Verdict | Concrete observed evidence and qualification |
| --- | --- | --- |
| Fresh `H[V[A,B],C]`, A,B,C then left | PASS | `h1-a/b/c` readbacks establish the sequence and unchanged geometry; `h1-result` revision 14 has native active B, fixture B focused and core foreground B. Command source `src-d-1rp51zj` applies one focus effect at revision 13. |
| Fresh `H[V[A,B],C]`, B,A,C then left | PASS | All three native UUIDs differ from the first run. `h2-b/a/c` establish B (admission focus), A, C; `h2-result` revision 28 has native/fixture/core focus A. Source `src-r-1khyznd` applies one focus effect at revision 27. The two-window staging order needed a verified swap before B admission. |
| Explicit-command recency | PASS | From A, directional down reaches B (`explicit-b`, revision 36), right reaches C (`explicit-c`, 38), left returns to B (`explicit-result`, 40), with native active and fixture/core focus agreeing at every checkpoint. Sources `src-z-h7i8u1`, `src-11-1ikpvnx`, `src-13-1r3s5jt` each apply a focus effect. |
| Accepted-hover recency | NOT RUN | The KDE capture explicitly supplies `focusFollowsCursor: false` (`kwin/src/world-observe.ts`); the script cursor is read-only. No accepted hover was generated or inferred from ordinary focus. Shared offline hover evidence above is not native acceptance. |
| Close newest candidate, choose next-most-recent | PASS | `close-four` has `H[V[H[B,D],A],C]`; `close-a/d/c` establish A,D,C. After closing D, `close-pruned` revision 49 retains C focused and `H[V[B,A],C]`. Left selects lower A, not stable-first upper B (`close-result`, revision 51; source `src-1e-1xwfii6`, focus effect at 50). D is absent from native and core rosters. |
| History-free stable-first fallback | NOT RUN | No live history-free candidate subtree was established; admission and observed focus already seed history. The close leg discriminates next-most-recent from stable-first, not the no-history fallback. |
| REQ-FOC-02 edge stay | PARTIAL | Root `H[A,C]` on the outer-left output: `root-edge-before` revision 53 and `root-focus-edge-result` 55 retain native/fixture/core focus A and identical allocations. Planner source `src-1i-16qd5uk` reports `internal=unchanged`. This proves no-adjacent-output stay with two outputs present, not literal single-output topology. |
| REQ-MOV-02 in-group swap, mover focus | PASS | `explicit-result` has B below A; move-up source `src-15-swe59r` applies two geometry changes at revision 41. `swap-result` revision 42 swaps their exact prior frames/allocations, leaves C unchanged and retains native/fixture/core focus B. |
| REQ-MOV-05 move edge no-op | PARTIAL | Correct root `H[A,C]` fixture: `root-focus-edge-result` 55 and `root-move-edge-result` 57 have identical native frames, core allocations/membership and focus A. Source `src-1k-kvqexl` reports `internal=planner-noop`; applied revision 56 has zero geometry/member/focus effects. Literal single-output topology was not exercised. |
| Adjacent-output entry | PASS | Destination `H[A,E]`: E was most recently focused (`adjacent-e`, 64); C on the source output is then focused (`adjacent-source`, 65). Right enters the destination's left-edge A, not newer non-edge E (`adjacent-result`, 67; source `src-1u-1gc7c0f`, focus effect at 66). Native/fixture/core focus agree and allocations are unchanged. This is one ordinary entry discriminator, not nested-edge ties or overlay coverage. |
| Incompatible minimum-size observation | PARTIAL | `minimum-hints` accepts both hints; `minimum-observed` independently reads both native minima wider than their respective allocations, yet both remain core-tiled with disjoint matching native frames. Hint-only mutation leaves the presentation at revision 67; a subsequent focus command reaches accepted revision 68 and `minimum-result` 69 retains the same allocations. Saved core logs contain accepted outcomes but no `overconstrained`/`noSpace` diagnostic. This is an observation, not native minimum enforcement, immediate hint pickup or complete B6 acceptance. |
| Optional hover guards | NOT RUN | No accepted-hover input route was established; no guard acceptance is inferred. |

- Staging/collection deviations: the first read-only helper evaluated but the
  scratch collector tried to JSON-parse its void `run` reply; its strict cleanup
  still proved named/object absence before continuing. An activation's journal
  line arrived after its immediate read: the saved later journal and next native
  readback established it without repeating activation. Collection was changed
  to a bounded journal-result wait. A later cumulative journal read exceeded the
  collector buffer after native/fixture/presentation capture; the buffer was
  enlarged and later complete capture retained. These are collection failures,
  not passing product attempts or controller-readiness failures.
- A first move-left used `H[V[A,B],C]`, not the promised root-edge fixture:
  `move-edge-result` revision 32 shows the left subgroup changing to horizontal.
  It is not counted as REQ-MOV-05 evidence. Orientation was restored and the
  later root-edge check used the independently verified two-leaf fixture above.
  No product fix was made. The original native desktop was retired by ordinary
  controller lifecycle churn; cleanup selected a surviving pre-existing desktop,
  not an invented restoration. The appended test desktop then retired normally.
- Lifecycle: all 58 nonce-named helpers retain source, plugin, signed load ID,
  pre-load child roster, exact-object introspection and raw run/unload replies.
  Each was unloaded before another helper or controller teardown, with strictly
  parsed plugin false and exact child absence. IDs 1 before controller startup
  and 2 during its lifetime were reused only after verified absence. Final
  read-only checks of all 58 names return false; only the untouched pre-existing
  script object remains (`final-accounting-commands.jsonl`,
  `final-accounting.json`). No helper survived controller unload.
- Teardown: fixture shutdown returned successfully, its server/wrapper exited
  and native windows/socket/private directory were absent. Combined tmpfs dev,
  planner and tray logs were copied before teardown. `dev-off` unloaded the
  controller and stopped the planner; the foreground dev wrapper still owned
  its tray/followers, so its exact verified shell was terminated through its
  cleanup trap. Final exact-PID checks show all recorded fixture, planner, tray,
  wrapper and eight follower processes absent. `final-dev-status.txt` is DOWN;
  `final-ydotoold.txt` is empty. Preloaded native effects were preserved. A final
  empty-directory removal found the fixture had already removed its directory;
  no resource remains unresolved. No stored tiler setting was changed.
- Verdict audit: independent review of saved raw calls, native/fixture/core
  checkpoints, correlated journals and cleanup receipts confirms all 12 scoped
  verdicts above and all 58 helper lifecycles, with no unresolved accounting
  mismatch. No PASS was widened to cover hover, history-free fallback, literal
  single-output topology or physical input; batch-1 acceptance remains partial.

## Requirement batch 2: focus and attention (2026-11-13)

- Goal: resolve REQ-FOC-03/04 and REQ-ACT-02 against latest stable Tier 1
  sources without inventing native results or reopening earlier behavior.
  Acceptance separates a product scope decision from implementation and native
  proof; requirements that need a new focus model or substantial command surface
  remain OPEN with evidence-led alternatives.
- Approach/outcome: targeted source rechecks, command/model/shortcut inventory,
  scope decisions and independent diff review. This batch makes no runtime,
  protocol, world-contract, fixture or adapter changes. No active-family refresh
  is needed; all immutable fixtures are untouched. No live test is claimed.
- Releases: COSMIC `epoch-1.11.0`, Hyprland `v0.57.2`, i3 `4.25.1`, sway
  `1.12`; local tags and read-only origin tag listings rechecked 2026-11-13.
  All inspected source versions were available locally; no fetch was needed.
  Matching matrix cells carry their own version labels and native/TBD limits.

| Row / shortest sequence | COSMIC `epoch-1.11.0` | Hyprland `v0.57.2` | i3 `4.25.1` | sway `1.12` | Outcome / assumption |
| --- | --- | --- | --- | --- | --- |
| FOC-03: create A,B,C in order, `H[A,B*,C]`; next, previous; fresh C-next and A-previous; fresh ordinary F | No compositor cycle on inspected surface; external switcher. `data/keybindings.ron:9-18,109-112` | C/B, wrap A/C, F included, creation order. `src/config/shared/actions/ConfigActions.cpp:1759-1804`, `src/desktop/state/WindowQuery.cpp:278-291,310-314` | C/B, wrap A/C, F excluded from tiled walk; parent-relative directional semantics, not global DFS. `src/commands.c:1290-1357`, `src/tree.c:503-577,593-629` | C/B, wrap A/C, F excluded from tiled walk; parent-relative direction and inactive-view descent. `sway/commands/focus.c:17-59,138-224,423-474` | STOP: substantial command/shortcut surface; no selection or assumption |
| FOC-04: `H[V[A,B*],C]`; parent, child | V then B, group child stack. `src/shell/layout/tiling/mod.rs:1856-1936` | No counterpart on inspected focus/group-tab inventory. `src/config/shared/actions/ConfigActions.hpp:50-85` | V then B, level up/down. `src/commands.c:1401-1425`, `src/tree.c:386-430` | V then B, parent/active child. `sway/commands/focus.c:339-362,416-421` | STOP: focus-model change; no selection or assumption |
| ACT-02: `H[A*,B]`; mark B, user focuses B on same workspace | Serial-less sandbox route marks workspace; same-workspace focus retains, workspace switch clears. `src/wayland/handlers/xdg_activation.rs:72-78,124-130`, `src/shell/focus/mod.rs:199-225,285-335`, `src/shell/mod.rs:557-562` | Marks unfocused window, focus clears. `src/desktop/view/window/Window.cpp:815-834`, `src/desktop/state/FocusState.cpp:213-214` | Demand-attention sets leaf; focus clears leaf and recomputes aggregates. `src/handlers.c:695-704`, `src/con.c:250-274,2424-2460` | Marks, same-workspace focus clears immediately; workspace-switch timer is separate. `sway/tree/view.c:509-513,1205-1229`, `sway/input/seat.c:1223-1241` | DEFERRED beyond 0.1: normative scope only, no future timing selected; no assumption |

### Focus options and implementation boundaries

- REQ-FOC-03 remains OPEN. Options: (a) preserve no cycle verb in 0.1,
  consistent with existing post-0.1 triage and COSMIC's compositor inventory;
  (b) add tiled parent-orientation next/previous with wrapping as i3/sway;
  (c) add creation-order cycle including floats as Hyprland. The flat fixture
  cannot select nested behavior, so the matrix adds a nested discriminator with
  exact endpoints left TBD. Production completion is not just a planner helper:
  `WorldCommand`, implicit-subject resolution, sizing, v4 codecs and shared tests;
  KDE `world-report.ts`, `world-entry.ts`, `world-shortcuts.ts`;
  native-effect `shortcutreconciler.cpp`/`.h`, catalog/preset/conflict tests;
  and mechanical exhaustive-match adoption in Windows `world_tick.rs`.
  Default chords and nested/cross-output scope need selection before that surface
  is introduced. No command, wire variant, setting or shortcut is added here.
- REQ-FOC-04 remains OPEN. Options: (a) preserve leaf-only focus in 0.1;
  (b) design virtual group selection, remembered child and subsequent command
  scope while native focus remains on a member. A separate auxiliary group
  cursor is another design option, not selected or added. Current
  `Session::focused_leaf` (`C/session.rs:763-764`),
  `FocusPlan::Focused { leaf, route }` (`C/directional.rs:575-578`) and
  `FocusRequest::target: WindowId` (`C/world_report.rs:420-437`) cannot represent
  an active hostless group merely by adding parent/child aliases. Tier 1's
  three group-capable WMs supply product evidence, not a host mapping or scope
  decision for move/resize/close. No architecture/principle is changed here.

### Urgency scope decision

- REQ-ACT-02 is NORMATIVE only as a deferral: product-owned urgency mark/clear
  is deferred beyond 0.1, decided under the standing Tier 1 alignment rule
  (User 2026-11-13). [The topic decision](../decisions/activation-requests.md#urgency-markclear-scope)
  preserves the prior Windows native-only-until-indicator and indicator
  cleared-on-visit choices, plus KDE native activation ownership. No urgency
  protocol/state/UI or native clear override is added; future product timing
  remains unselected. This is not a claim that Tier 1 votes for deferral:
  Hyprland/i3/sway support later clear-on-focus, while the earlier product scope
  supplies the reason not to add that subsystem in 0.1.
- Prior `P` triage means post-0.1, not an explicit prerequisite-gated park;
  these three rows are distinct from LAY-04's expressly parked second-layout
  decision, which is unchanged. Deferral resolves scope, not the native mark/
  clear acceptance gap or REQ-ACT-01 implementation. No new behavior assumption,
  timer, urgency setting or platform exception is selected.
- Native/source limits: KWin's already-cited `8438567a src/activation.cpp:250-253`
  clears attention on activation, but no live journey was run. Existing adapters
  do not expose product urgency state; historical Windows flash notifications
  are not visible marker or clear proof. Native outcomes remain TBD. Because
  this is a scope-only decision with no executable change, no new behavioral
  regression or recorder update applies; the unchanged shared suites are the
  offline safety check, not evidence of urgency parity.
- Counts: 184 total = 161 NORMATIVE (including qualified REQ-OUT-05),
  23 OPEN, 0 PROVISIONAL. Batch 2 resolves one scope row and leaves two OPEN;
  batches 3-8 retain 21 OPEN rows. No macOS/Windows behavior changes or adoption
  items are introduced by this documentation-only batch.
- Final sequential offline gates, with complete output retained externally:
  `cargo fmt --all -- --check`;
  `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`;
  `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`;
  `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`;
  then in `kwin/`, `npm run typecheck` and `npm test`: all exit 0. Rust:
  3388 passed, zero failed/ignored, one filtered. KDE: 1297 passed in
  189 suites, zero failed/skipped. Exporter is untouched; native builds and
  runtime remain untested. Requirement enumeration independently confirms
  184/161/23/0; whitespace review is clean.
- Independent complete-diff review found no blocking defects. Its two
  informational findings were corrected: continuation indentation now matches
  the surrounding matrix, and this final gate/review record replaces the
  pending placeholder. Implementation remains stopped for the two focus rows,
  regardless of offline gate results; batch 2 is not fully resolved.

## Requirement batch 3: admission and fullscreen (2026-11-13)

- Goal/acceptance: resolve inactive-destination anchoring, splash/utility and
  app-specific PiP admission, hint-change reaction and fullscreen workspace/
  layout interactions against latest stable Tier 1 sources. Preserve existing
  architecture, gaming principle, minimum policy and fixed-size decisions.
  Product selection is separate from actual app/native evidence. Four rows
  become NORMATIVE; SPC-05b stops at an earlier-decision conflict, not a lack of
  implementation effort.
- Approach: release-qualified local source inspection, minimal discriminators,
  shared/KDE regression implementation, sequential integrated offline gates and
  complete-diff review. COSMIC `epoch-1.11.0`, Hyprland `v0.57.2`, i3 `4.25.1`
  and sway `1.12` were checked against read-only origin tag listings on
  2026-11-13; no newer stable tag was found. Matrix cells carry inline versions
  and exact source paths; app/native outcomes not proved by source stay TBD.

| Row / discriminator | COSMIC `epoch-1.11.0` | Hyprland `v0.57.2` | i3 `4.25.1` | sway `1.12` | Decision / assumption |
| --- | --- | --- | --- | --- | --- |
| INS-07b: W1 A focused, W2 H[B,C] remembers C; D routed to W2 | Workspace token, target last-active tiled node; no inactive focus. `src/shell/mod.rs:3044-3086,3153-3184`, `src/shell/layout/tiling/mod.rs:417-419,2826-2845` | Silent workspace rule, inactive cursor-closest anchor, no switch/focus. `src/desktop/view/window/Window.cpp:1370-1415,1519,1565-1571`, `src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:95-106,650-666` | Assignment, target focused tiled descendant, invisible workspace suppresses focus. `src/manage.c:289-316,601-613` | Assignment, target inactive tiled focus, differing workspace suppresses focus. `sway/tree/view.c:632-715,850-872,898-902,945-957` | Target-local remembered/fallback; no focus/switch/replay for inactive births. No assumption; existing placement settings stand |
| SPC-02: fresh typed splash; utility/toolbar variant | Splash floats, utility normally tiles. `src/shell/layout/mod.rs:20-58` | Both float, initial focus suppressed. `src/desktop/view/window/X11Backend.cpp:59-80` | Both float, visible-workspace focus. `src/manage.c:462-474,601-613` | Both float; focus/switcher TBD. `sway/desktop/xwayland.c:310-329` | Float both, retain safety gates. ASSUMPTION no new type-focus exception; host switcher unchanged |
| SPC-03: PiP label only versus positive ordinary float fact | Parent/type/fixed predicates and configured app/title exceptions. `src/shell/layout/mod.rs:20-109` | Backend traits and floating rule. `src/desktop/view/window/WaylandBackend.cpp:31-38`, `X11Backend.cpp:59-95`, `Window.cpp:1038-1046,1232-1233,1301` | Ordinary type/modal/fixed gate. `src/manage.c:462-474` | Ordinary xdg/Xwayland float predicates. `sway/desktop/xdg_shell.c:228-235`, `sway/desktop/xwayland.c:310-342` | ASSUMPTION ordinary facts, no app/title heuristic/new rule subsystem/topmost inference. Actual app outcomes TBD for all four |
| SPC-05b: runtime non-fixed minimum raise, feasible 600/600 -> min B700 versus infeasible | Tiled configure retained, no inspected tiled-minimum path. `src/xwayland.rs:918-974` | Tiled configure resend, floating-only client hint clamp. `src/desktop/view/window/Window.cpp:974-980,1791-1795` | Resize/hint update path floating-only. `src/handlers.c:345-376,1006-1019` | Tiled configure retained; hint listener urgency-only. `sway/desktop/xwayland.c:589-605,756-774` | STOP, OPEN: earlier feasible-minimum policy reallocates 500/700. No new decision/assumption/behavior change |
| MAX-12: fullscreen move, workspace switch/return, orientation; extra workspace-tiling off/on | Switch retains fullscreen layer; orientation no-op; tiling toggle leaves fullscreen layer. `src/shell/mod.rs:541-584,1778-1813`, `src/shell/layout/tiling/mod.rs:2089-2098`, `src/shell/workspace.rs:1430-1489` | Switch retains fullscreen, return cover focus; orientation guard. `src/output/Monitor.cpp:1330-1428`, `src/desktop/state/FocusState.cpp:80-85`, `src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.cpp:774-785` | Switch retains fullscreen; parent layout may mutate. `src/workspace.c:438-522`, `src/commands.c:1174-1202,1597-1648` | Switch retains fullscreen; parent may mutate. `sway/tree/workspace.c:731-743`, `sway/commands/split.c:53-84`, `sway/commands/layout.c:117-175` | Retain fullscreen, existing move fence. ASSUMPTION orientation no-op in even split; existing workspace tiling toggle preserves fullscreen |

- Authority: the four selections were decided under the standing Tier 1
  alignment rule (User 2026-11-13). The topic decisions own reasons,
  assumptions and revisit conditions: [inactive destination](../decisions/layout-and-admission.md#inactive-destination-admission),
  [special admission](../decisions/window-state.md#splash-utility-and-picture-in-picture-admission)
  and [fullscreen actions](../decisions/window-state.md#fullscreen-workspace-and-layout-actions).
  No new command, shortcut, setting, native-kind arm, wire field or adapter rule
  subsystem is introduced. PiP is a fact-based policy decision, not proof of
  any unspecified media application's flags or a Tier 1 vote for one app rule.
- Implementation: `take_admission_focus` spends each converged birth on an
  inactive workspace before active-newcomer selection; remembered target-local
  anchoring is unchanged. This prevents an inactive birth masking an eligible
  active one in the same report and prevents replay after workspace return.
  Utility/splash now share the existing dialog/sheet float-origin classification,
  including independent nullable transient/modal/layer/hint inputs; safety gates
  remain prior. KDE captures non-normal utility/toolbar/splash through raw facts
  rather than dropping them. Ordinary floats carry no automatic frame or native
  membership-write authority. No new focus/stacking exception is added.
- Shared regressions: two report-level inactive-destination tests prove remembered
  C anchoring, no automatic focus/view transfer, per-birth spending, mixed active/
  inactive births and no replay. Four classification journeys prove utility/
  splash float and null independence, safety precedence, metadata-only PiP tile
  versus ordinary float facts, and existing infeasible-minimum/drift retention.
  Four fullscreen command journeys prove host-owned return focus/arrival fencing,
  refusal of orientation and directional move, tiling off/on without fullscreen
  subject frame/state/automatic focus, and unchanged native-free suppression veto.
  KDE capture regression covers all three type flags. Existing feasible-minimum
  regression proves the STOP discriminator; it is not changed.
- SPC-05b options: (a) explicitly retain the recorded host adaptation, feasible
  minima redistribute slack while infeasible minima remain proportional, then
  test native convergence; (b) explicitly supersede that decision for runtime
  tiled hints to match the inspected Tier 1 no-reflow paths. The earlier
  [Placement and minimums](../decisions/layout-and-admission.md#placement-and-minimums)
  choice and delayed host-settled-geometry design timing are untouched. Added
  infeasible-hint/drift regression describes existing behavior only; native
  reaction remains TBD and this row stays OPEN pending that product decision.
- Fullscreen limits: parent orientation and workspace tiling off/on are distinct
  existing commands, not the parked alternate-layout feature. Native-free
  selection still withholds controlled suppression for fullscreen; explicit
  return uses existing direct-focus fallback rather than host-owned awaitShown.
  No safety gate is relaxed to claim parity. Native visibility, stacking and
  fullscreen retention must be checked on the actual host.
- Fixtures/contract: no immutable or active family changed; no recorder refresh
  was needed. The world contract clarifies the new float origins under the
  existing type vocabulary, not a protocol/version change. `tiler-protocol` and
  the settings exporter are untouched. No live desktop test was run.
- Integration corrections: a new remembered-leaf assertion needed a retained
  snapshot binding to compile. Initial classification oracles incorrectly
  expected ordinary floats to have native membership-write authority and a
  feasible fixed-minimum update to retain exact proportional geometry; they now
  assert existing ordinary-float authority and retained tiled membership. The
  infeasible-minimum exact-allocation assertion is unchanged. KDE's test surface
  type needed the three native flags added before typecheck could compile.
- Sequential integrated gates: `cargo fmt --all -- --check`;
  `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`;
  `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`;
  `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`;
  then in `kwin/`, `npm run typecheck` and `npm test`: corrected integrated
  results exit 0, Rust 3398 passed, zero failed/ignored, one filtered; KDE
  1298 passed in 189 suites, zero failed/skipped. Earlier integrated test and
  typecheck failures are not acceptance evidence. Complete output retained
  externally. Independent complete-diff review found no defects; source
  predicate spot-checks, the new untracked test, all production/test/doc hunks
  and the 184/165/19/0 enumeration were reviewed. Whitespace review is clean.
- Counts: 184 total = 165 NORMATIVE (including qualified REQ-OUT-05), 19 OPEN,
  0 PROVISIONAL. Batch 3 resolves four rows (including MAX-12 from batch 5),
  stops one. Batch 2 retains two OPEN focus rows; other batches retain 16 OPEN.
- Platform adoption: shared inactive-birth focus suppression and utility/splash
  float-origin admission change the Windows/macOS core target too, with no
  adapter edits/builds here. Windows tool-window/owned gates remain; AX floating
  windows normalized to utility now get a positive float origin rather than a
  hold. PiP metadata behavior and fullscreen command behavior are confirmed,
  not newly altered. Native pickup, focus/switcher, actual PiP apps and fullscreen
  return/tiling toggles remain unverified on all hosts.

## Verification, limits and next decision

- Historical research verification: source reads, exact OPEN-status enumeration, tag-qualified local source inspection and docs diff/whitespace review. Inventory IDs exactly match all 28 OPEN rows; batch sizes are 4/3/4/4/3/4/3/3. New-note relative file links resolve, added matrix target exists, note is ASCII. A broader link check found pre-existing missing `docs/changes/retained-focus-requests.md` targets in the matrix (also present at the inspected base); those unrelated links are unchanged. Research alone claimed no live outcomes or test-suite results; implementation evidence is separate above. Investigation reports were checked against key source spans; over-broad "Tier 1 always tiles" and "COSMIC equals stable-first" claims were rejected.
- C9 corrections: `core-v4-review.md:267-268` is accurate about absent reference citations, but not an absence of user authority; `:275` correctly counts 28 but incorrectly describes all as already implemented. Absent cycle/preselect/layout/column verbs and the explicitly parked LAY-04 contradict that characterization. Its 183-row total (`:265`) and the spec's draft total are also stale: current enumeration is 184 rows (155 literal NORMATIVE plus qualified-NORMATIVE REQ-OUT-05, 28 OPEN). This is not evidence that the remaining OPEN rows should automatically become normative. Separately, the source recheck contradicts R-FOC-02's historical Hyprland-retain claim in the explicit zero-gap variant; opposite-edge fallback is not a stay rule for a half-width A.
- Unverified/TBD: exact tiny rectangles/client settlement on every WM; exact latest-tag COSMIC FOC-01 fixture, Hyprland shipped-gap FOC-02 and move reconstruction; native settings pickup/parity, especially Windows report defaults; remaining batches 4-8 latest-source checks except MAX-12. Research alone selects nothing; batch selections and assumptions are recorded separately above.
- Next action: remaining native adoption of fixed-largest/fixed-float and the
  qualified batch-1 gaps above.
  Batch 2 retains two OPEN focus decisions; SPC-05b retains its earlier-decision
  conflict; other remaining batches retain 16 OPEN rows.
