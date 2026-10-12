# Windows workspace tiling mode

## Final status

- Complete: original workspace-mode implementation `90ee5c2` plus R-MAX-03
  correction `ace352c`. Earlier incomplete/blocked sections below are historical,
  superseded by the accepted correction at the end of this record.
- Native locked gates, independent review, Notepad/Paint plan/write/matched
  readback and screenshot proof pass. Hosted CI for `ace352c` is green across
  Windows, Linux Rust, KWin, shell, native effect and macOS:
  <https://github.com/beefsack/OmniTiler/actions/runs/37196192226>.
- Decisions and R-MAX-03 promoted; record archived. Physical checks remain
  user-owned. Live cleanup independently verified.

## Goal and scope

- Match KDE's session-local per-workspace tiled/floating state, current-workspace
  tray checkbox, and persisted `defaultTiled` (initially true).
- Floating preserves native geometry and independent active border, suppresses
  tiling/group underlay/drop preview. Retile explicitly releases the shared
  Engine domain and freshly adopts observed geometry.
- New windows on floating workspaces remain native; cross-boundary sends change
  membership and reflow only the tiled side. Existing individual float/sticky
  and native maximize/fullscreen exceptions retain their semantics.
- No keyboard binding: KDE registers its tray action with an empty sequence.
- No backlog/principles edits, new toolkit/dependencies, registry/policy writes,
  persistent workspace overrides, or unrelated workspace-mode expansion.

## Acceptance and units

1. Implement portable workspace state, shared Engine release integration,
   native runtime/effect/send guards, tray truth, settings store/UI and bounded
   structured logs. Targeted behavior tests and four-package native gates.
2. Independent review of live/state boundaries; correct concrete findings and
   verify current gates; hosted CI green.
3. Scoped live Notepad/Calculator/Paint proof: tray toggle floating, native move,
   new-window admission, retile, sends across both boundaries, effects, default
   and settings UI. Capture screenshots and actual native effect readbacks.
4. Promote decisions, matrix evidence, archive this record and
   verify hosted CI plus final clean tree/live state.

## Approach and authorization

- Initial source `7f9b733`, clean main tracking origin/main. KDE reference:
  `tray-workspace-toggle.md`, `tray-toggle-simplicity.md`, decisions Tray,
  `kwin/src/plan-adapter-entry.ts` keyless registration, shared release-domain.
- Existing workspaces take saved default at owner startup; live default edits
  affect only newly created workspaces. Overrides reset on owner restart.
- User authorization permits scoped input, movement,
  hiding/restyling, overlays and disposable Notepad/Calculator/Paint actors.
  Never close/kill/type into hosting Terminal; prefer explicit scope filters.
- Before live work bind artifact/owner/actors to exact identities and capture
  baseline/settings preimage. Use existing proven stop/restore paths. No broad
  cleanup; close only newly opened disposable apps. End with no project actor,
  overlay, ledger or hidden test window; original settings-file state restored,
  normal taskbar, SPI arranging 1 and pen visualization 35.
- Physical input/feel and other output/DPI setups remain user-owned.

## Evidence and decisions

- Research confirms KDE floating/tiled live acceptance and no assigned shortcut.
  Existing shared Engine release is the required reuse boundary; Windows
  managed-workspace mapping owns platform session mode just as KWin does.
- Initial implementation passes native tests/fmt/clippy. Independent review
  found missing tiled-source boundary-send reflow, release losing individual
  floats, stale/duplicate pending releases, unbound tray scope, and directional
  focus divergence. Correct these before acceptance/live proof. A reported
  Settings stale overwrite is rejected: existing full-content Apply refusal
  already reloads without writing (`settings_ui.rs:517`).
- Corrected native locked build/test/clippy/fmt verified. Live scoped
  Notepad/Calculator/Paint evidence confirms floating leaves frames untouched,
  native moves persist, new windows remain native, retile freshly fits, border
  remains, settings default applies live only to future workspaces and startup.
- First live menu screenshot incorrectly read as Tiled checked; `shot-menu-boot-floating.png` inspected and that bug claim rejected: Floating
  is correctly checked. No product failed approach or correction follows it.
- Remaining send proof needs a minimal automation `workspace --send` command:
  normal owner intentionally rejects injected keyboard input. Reuse existing
  exact-owner request transport and native send dispatch with all fences.
- Cross-boundary sends and active gesture effects accepted below. Implementation
  `90ee5c2` pushed; hosted Windows/Linux Rust/KWin/shell/macOS CI green:
  <https://github.com/beefsack/OmniTiler/actions/runs/37181616531>.
- Final inspection reproduced first-seen maximized-window restore on a floating
  workspace. Mode-gating the admission clear fixes geometry, but first-seen
  maximized rows also need managed workspace membership so switching hides them.
  Two correction approaches have not completed the full invariant; blocked as
  detailed below. Preserve candidate diff; do not commit or archive as complete.

## Accepted verification and outcome

- Four Windows-built packages (`tiler-core`, `tiler-protocol`,
  `tiler-kwin-effect-ffi`, `tiler-windows`) pass locked build/test and strict
  all-target clippy; full rustfmt and diff checks pass after the final CLI
  change. Installed MSVC Cargo used; no dependencies installed. Shared core
  and KDE source remain unchanged; existing `ReleaseDomain` is reused.
- Independent findings corrected: one latest pending release per domain,
  successful-toggle stale release cancellation, exact-lifetime float carry
  across release, native tiled-source survivor reconcile, scoped tray request,
  and KDE-parity floating directional refusal. Existing stale Settings Apply
  refusal was retained. Actual Engine regressions exercise release/fresh float
  admission and native-boundary source removal/reflow; CLI tests cover exclusive
  select/send, malformed actions and exact transport fields.
- Live 2026-10-04, physical Windows 11 Pro build 26200, medium/session 1,
  DISPLAY1 2560x1440, DPI120. Explorer-broker owners scoped to Notepad/Paint
  and ApplicationFrameHost with Calculator child gate, caps <=600 seconds.
  Existing user app windows were controlled but not closed; only newly opened
  Notepad extras were closed. Hosting Terminal was excluded by test scope.
- Initial artifact SHA-256
  `19761840E8FA082D62B9CCAA91389D95D210DF8F510D706B8CA41D4F59D75689`:
  floating toggle preserved all eight frames; a native move survived ticks;
  new Notepad frame remained native; retile released nine-member domain then
  fresh fit with every applied native readback matched. Border remained visible.
  UI Apply saved Floating and owner adopted future-only; fresh startup with
  saved false left all frames untouched. Initial evidence:
  `target/windows-workspace-tiling/20261004-155929-4012/evidence.jsonl`.
- Final source `7f9b733` plus reviewed implementation/CLI diff, artifact
  `target/windows-workspace-tiling/20261004-163221-live/tiler-windows.exe`, SHA-256
  `D649DA769C5B2EC9010900896336BE4D20DA1BE6990287C27E39C1071B1E9A60`.
  Owners bound to creation `01dd53c1fc55a5db` and `01dd53c3d4918d67`.
- Final live boundary roundtrip: Paint sent tiled->floating (`act-111`), frame
  `(1680,694,2560,1319)` unchanged, six source survivors reflowed before hide
  with all six readbacks matched; exact mover foreground confirmed after follow.
  Select-source confirmed filled layout; floating->tiled (`act-157`) admitted
  Paint into a seven-member plan with seven matched writes, no duplicate/missing
  member. No floating-side plan/write. Source action-summary fields are absent
  for the native route; correlated `send-source` tick/readback logs are the
  source effect evidence.
- Actual native title-bar drag shows underlay and preview when tiled; preview
  inspect/log rectangle `[870,694,401,678]` agrees with screenshot. Floating
  drag leaves both absent with `group-underlay reason=floating`, zero shown
  events, and moved native frame persists without snapback.
- Tray default picks both ways save only the default and acknowledge live
  adoption; existing workspace stays tiled, subsequently allocated workspace
  starts floating. Settings shows saved Floating and Close preserves file bytes.
- Final menu/UI, floating Paint and tiled/floating drag screenshots inspected.
  Final ignored evidence: `target/windows-workspace-tiling/20261004-163221-live/`.
  Key screenshot SHA-256 values:
  - `shot-menu-ws1-stilltiled.png`:
    `0C02B7B3BD40B4EF66C22AC3F49739666CA5D0B1CDCE7F0B42268D3C407AB96A`.
  - `shot-menu-newws2.png`:
    `5925DA7ED02849809549B0045CF86A858E74EFCB0409E04668F4BF9ACA873FEA`.
  - `shot-settings-floating.png`:
    `9C2DA3BC8904643F8F1E005FB579C0F5EE5EF55805EB9FF027EFAD0FE9E75694`.
  - `shot-ws2-floating-member.png`:
    `FFA77B4815652776C47D6AB82F04612AAE7C28ABC7AFF929F32ADA8D981283EE`.
  - `shot-tiled-drag2-hold.png`:
    `7F3CB84D69165A817800B523DCD1AB63EDD4C9C5F7B7EE250B2AD5659FB708E5`.
- Fixture lessons: use exact overflow-icon UIA Invoke and visible menu-row
  coordinates; normal owner filters synthetic keyboard chords/modifier holds,
  so CLI send/native title-bar drag supply valid automated effects. Empty
  intermediate workspace cleanup shifts ordinal indices; re-resolve the live
  scope before the next probe. An invalid suppression probe on a newly tiled
  workspace was rejected and superseded by a correctly toggled floating drag.
  Owner duration expiry was recovered with restore before further testing.
- Final stop/restore, extra-window close and independent read-only recheck:
  no owner/helpers/UI/overlays or recovery ledger/request; original settings-file
  absence restored; numeric SPI GET success with arranging 1 and pen 35; normal
  taskbar and all baseline apps present/visible. Original hosting Terminal PID
  18224 creation `01dd512e9194d8b9` survives.
- Provisional decisions: existing JSON field location and minimal exact-owner
  send automation, recorded in `docs/decisions.md`. Runtime behavior follows KDE.

## Residual user-owned checks

- Physical tray/keyboard/mouse feel, rapid workspace/send/toggle use, per-window
  float/sticky/native maximize/fullscreen roundtrip feel, other DPI/outputs.
- Application minimums may still cause overlap/overflow on dense tiled layouts
  under the pre-existing minimum-size policy; workspace-mode toggles do not
  change that policy. No gaming/lock chord was injected in these proofs.

## Blocking admission follow-up (not accepted)

- Required invariant: first-seen maximized windows on a floating workspace
  preserve native geometry, participate in managed hide/reveal, and on first
  tiled admission restore once then receive a real fresh tile. Previously
  slotted native overlays must not re-clear across a mode flip.
- Approach 1: gate `clear_maximize_at_admission` by workspace mode. Reproduction
  on the committed artifact restored a maximized Notepad while default floating;
  the guard fixes that geometry mutation, but leaves first-seen maxima without
  workspace membership, so selecting away does not hide them. Rejected as
  incomplete. Local evidence: `target/windows-workspace-tiling/max-admission/`.
- Approach 2: add exact-lifetime `floating_max_hold`, hidden-frame seed, managed
  membership and release on fresh non-maximized observation. Live proof confirms
  untouched floating maximum and select-away/back hide/reveal, one admission
  clear on retile, and no re-clear after subsequent maximize/mode flips. However
  the claimed fresh tile rejected: after clear, the window stays at native
  restore frame `(40,40,1611,725)`, and the next normal observations still omit
  `w7` from every plan. The fixture incorrectly used `inventory` presence as
  managed/tiled proof. The temporary floating observation seeds an Engine
  exception which `assemble_domain_rows` continues to carry after the hold
  clears. No actual tiled write/readback for the restored window was proven.
- Latest candidate remains uncommitted in `tiling.rs`, `tiling_sys.rs` and
  `tests/tiling.rs`. Native locked build/test/clippy/fmt pass but do not establish
  this missing behavior. Candidate artifact SHA-256:
  `2810681BF0B953340DD8498238A9CCC83878EEFEDE400DF4AC78F8FB581EED8A`.
  Evidence: `target/windows-workspace-tiling/max-membership/evidence.jsonl`;
  owner log `run-01dd53ca153e8b1f.log`, clear at line 863, normal observation
  at 879 and five-window plan at 882 exclude `w7`.
- Candidate also carries native-send source summary/timings through the existing
  action log. Live `act-239` reports source applied 4, mismatched 0, readback_ok
  true and geometry 47ms; this narrowly corrects the earlier absent source
  summary. Floating mover frame remains unchanged. This part is accepted evidence
  but remains in the uncommitted candidate with the admission correction.
- No causal evidence-plumbing repair or product retry follows the second failed
  semantic approach. Stop threshold reached; no third correction attempted. Exact next action: distinguish the temporary
  max-hold Engine exception from intentional float state on release, then assert
  actual tiled plan/write/native readback after retile before accepting it.
- Final candidate run stopped/restored, exact extras closed, baseline Notepads
  visible, original settings absence restored, project actors/UI/overlays gone,
  SPI numeric arranging 1/pen 35, normal taskbar and original hosting Terminal
  alive. Earlier incorrect SPI action-name readings in this fixture are
  superseded by explicit numeric GET readbacks.
- Main implementation `90ee5c2` is pushed with green CI but the overall change
  is incomplete. Keep this record active; backlog must not mark it complete.

## Authorized third correction

- User authorizes one third semantic correction: retire only the temporary
  max-hold Engine exception; prove actual tiled plan, write and native readback,
  run gates, green CI and archive. If it fails, discard unaccepted
  source changes, record the known limitation here and in decisions
  and leave the tree clean. No further semantic iteration.
- KDE source establishes parity: `plan-adapter.ts:4905-4909` and `5213-5216`
  skip floating domains before admission clear; `5351-5418` restores unslotted
  maxima once and refetches native state for normal admission, while previously
  applied/slotted windows skip re-clear. Native user restore provides ordinary
  non-maximized observations. Only initial fullscreen has a synthetic floating
  hold (`2249-2297`); no maximize hold exists in KDE.
- Ordinary Windows integration choice: exclude only retired temporary max-hold
  state from Engine-exception feedback on fresh admission. Preserve exact
  lifetime/scope fences, intentional float/sticky/fullscreen state and previously
  slotted overlays. No new behavioral divergence or provisional choice needed.
- Third correction failed live; user-authorized fallback executed below.

## Third correction outcome and known limitation

- KDE behavior is established by source, not ambiguous: floating skips clear;
  first tiled admission of an unslotted maximum restores once and refetches for
  fresh tiling. A previously slotted native maximum skips admission clear and
  restores through ordinary native-state observation. No provisional divergence
  was selected.
- Third approach added exact-lifetime released-max tracking and excluded that
  token's stale Engine exception from observation/write eligibility, preserving
  intentional floats. An actual Engine regression passed flag adoption into a
  tiled plan; this did not prove the Windows owner supplied that observation.
- Candidate locked four-package build/test/clippy/fmt passed. Live final
  artifact `target/windows-workspace-tiling/third-correction/tiler-windows.exe`
  SHA-256 `8A274DC470F7BB9BAC76CA121688333CF3BAF92BE91B9761FB6385F5A64C2130`,
  source `90ee5c2` plus rejected candidate diff. Scoped Explorer-broker owner
  creation `01dd53cd55933c0b`, medium/session 1, Notepad only, DPI120 display.
- Live floating maximum and select-away/back geometry/visibility passed, but
  retile failed the required effect criterion: after one clear the frame
  remained `(120,120,1691,805)` through a 12-second settle. Token `w8` never
  appeared in a tiled plan, write or matched readback. Observation assignment
  seeded `member_rects` before the suppression gate, so its `known_slot` test
  prevented retiring the temporary Engine exception. No follow-up correction
  or further semantic iteration was attempted.
- Evidence retained locally under ignored
  `target/windows-workspace-tiling/third-correction/evidence.jsonl` and owner
  log `run-01dd53cd55933c0b.log`. The final checked-Tiling menu
  screenshot inspected; that check proves only mode, not tile admission. Screenshot
  `shot-menu-tiled.png` SHA-256:
  `2BF8F34CEA1DA1D978EE804D5423A9A9C1D5E34B5D95A28901AF36990634126A`.
- Per user fallback, all unaccepted changes in `tiler-windows` source/tests
  and `tiler-core/tests/session_observation_convergence.rs` were discarded to
  `90ee5c2`. This also discards the uncommitted source-summary/timing repair.
  Earlier candidate evidence remains historical only; no candidate source or
  regression is delivered. Committed native-send source tick/readback evidence
  remains valid even though its action summary has no source summary.
- Known limitation of retained implementation `90ee5c2`: first-seen maximized
  windows may be restored prematurely on a floating workspace because the
  admission clear precedes the mode gate. Full KDE parity for this case remains
  incomplete. The rejected hold/release implementations and their stale
  exceptions are not shipped. Record this as a defect in decisions and R-MAX-03;
  keep this one change record active, not archived as completed.
- Stop/restore succeeded, exact newly opened Notepad closed, baseline five
  Notepad HWNDs preserved, settings-file absence restored, project actors and
  recovery effects removed, numeric SPI GET arranging 1/pen 35, normal taskbar,
  original hosting   Terminal alive. Final independent recheck confirms owner/UI/ledger/pending/request absent, settings
  absent, both SPI GET calls successful with 1/35, and original Terminal PID
  18224 creation `01dd512e9194d8b9` unchanged.
- After discarding the candidates, the retained `90ee5c2` source passes locked
  four-package native build/test, strict all-target clippy, full rustfmt and diff
  checks. Only this record, decisions and the outcome matrix are committed in
  the fallback follow-up; no further source change or live attempt.
- Exact next action: none in this unit. Any further admission correction needs
  a separately authorized task; backlog should retain the known limitation.

## Bounded R-MAX-03 follow-up

- User authorizes fresh investigation, deterministic failing offline reproduction,
  then at most two semantic candidates with live Notepad/Paint plan/write/native
  readback and screenshots. Native gates, independent live-boundary review
  and green hosted CI are required. Discard unaccepted changes if
  both candidates fail; preserve findings and keep this record active.
- Candidate 1 follows KDE's separation of workspace membership from Engine
  admission: first-seen maxima join slotless without synthetic floating state;
  floating skips clear and slot seeding, first tiled admission clears once and
  refetches ordinary state. Prior exception-release approaches are not reused.
- Offline reproduction before production edits failed with `floating first-seen
  maximum must keep its native frame` (`cargo test --locked -p tiler-windows
  --test tiling r_max_03`). Candidate Windows source/test changes pass native
  tests, strict clippy and format checks. Live effect acceptance remains pending.

## R-MAX-03 accepted correction

- Candidate 1 accepted after independent review and inspection. Windows
  classified first-seen maxima as retained, outside normal membership admission;
  its clear ran before the workspace-mode gate. Temporary Engine floats in the
  rejected candidates conflated native maximum state with intentional float.
  The correction separates slotless membership from Engine admission: preserve
  floating maximum, seed no tile slot during floating release assembly, clear
  once on tiled admission, then refetch and tile normally. Exact-lifetime/scope
  fences remain; intentional float/sticky/fullscreen lanes retain their rules.
- The regression was strengthened after review to run actual Windows
  `assemble_domain_rows` on floating retained and tiled refetched observations,
  the real Engine and production writable selection. Removing the slot-seed
  guard makes it fail. Portable admission/Engine tests cover one-shot clear,
  slotted skip and intentional float. No shared core/KDE source changes.
- Locked build/test and strict all-target clippy for all
  four Windows-built packages, full rustfmt and diff checks independently run after the final
  source/test edit: all pass. Accepted commit `ace352c` pushed; hosted CI green
  across all six jobs (run linked in Final status).
- Live 2026-10-04, Windows 11 build 26200, medium/session 1, DISPLAY1
  2560x1440/work 2560x1380, DPI120. Artifact SHA-256
  `F29F89325468826E043841823BC797E798FBCB50FD4E3FC8F3AB8471C58821B5`,
  source `84c34b6` plus accepted Windows diff. Explorer-broker owners scoped
  Notepad/Paint; hosting Terminal excluded. Both new maximized actors preserve
  their floating frames with zero clear/write and hide/reveal maximized intact.
- Notepad owner `01dd53e92e1d1445`: token `w7`, exactly one clear, correlation
  `tick-26` has plan/native write/target-entry matched readback all
  `[8,8,1268,1364]`, agreeing with fresh DWM frame `(8,8,1276,1372)`.
  Previously slotted remax stays maximized through floating/tiled flips with no
  re-clear. Native restore returns to that tile; subsequent ticks 70/71 have
  plan and matched readback with zero writes because geometry already agrees.
  Accepted as a no-op restore, not a fresh-admission write claim.
- Paint owner `01dd53ec4fd098a8`: token `w5`, exactly one clear, `tick-18`
  plan/native write/matched target readback all `[8,8,1268,1364]`, fresh DWM
  agreement `(8,8,1276,1372)`, stable at tick 33. Both tiled-frame
  screenshots inspected. Accepted local evidence:
  `target/windows-workspace-tiling/rmax03-final/` (Notepad) and
  `target/windows-workspace-tiling/rmax03-paint-native/` (Paint).
- Screenshot SHA-256:
  - Notepad `shot-np-tiled-frame.png`:
    `850428CFFCE154FB57AAAF2DF80478B7FF16EB3C6866BD9DEBC33BCA9C0A0A10`.
  - Paint `shot-paint-tiled-frame.png`:
    `B9F2603C7915DC906EE800854EA4A0E6E492AC09A408C1DE5C9571ADE00ACA1A`.
- Fixture findings: post-toggle samples can already be tiled; compare DWM
  extended-frame coordinates, not invisible native borders. Anchor initial
  admission proof on the same token/tick/correlation for plan/write/matched
  readback. UIA icon/menu discovery failed pre-effect for Paint; its accepted
  proof opens the exact owner's normal tray menu via synthetic tray callback,
  then official `GetMenuBarInfo`/`GetMenuStringW`/`GetMenuItemRect` identify the
  enabled checkbox and exact click rectangle. No production fixture path added.
  Harness-only parsing/marshalling failures were repaired; one semantic candidate,
  zero failed product approaches in this follow-up. Earlier three rejected
  approaches remain historical evidence above.
- Every owner stopped/restored, all new extras closed by exact identity. Read-only
  recheck confirms baseline five Notepads/one Paint visible, settings
  original absence restored, no owner/helper/UI/overlay/ledger/pending/request,
  successful numeric SPI GET arranging 1/pen 35, normal taskbar, original hosting
  Terminal alive with unchanged creation.
- No new provisional product decision. Physical feel, rapid toggles and other
  DPI/output setups remain user-owned. Decisions/matrix updated and record
  archived after hosted CI green. Exact next action: none for this correction;
  backlog advancement and its archived-record link remain queued.

## User observation - 2026-10-12

- Scoped normal owner, single output, Notepad/Paint, physical keyboard/mouse;
  base `e506350` plus combined directional, retile-overlay and B9 Windows changes.
  Tray workspace tiling toggles do not restore the previously focused window.
  User notes this may be acceptable for keyboard accessibility of the tray;
  observation only, no behavior change requested. The current R-MAX-03 overlay
  retile/restore is physically accepted in
  [item 11](windows-maximized-floating-retile-overlay.md#user-accepted-physical-evidence---2026-10-12).
