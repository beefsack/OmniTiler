# Windows keyboard resize (handoff item 6)

## Goal and acceptance

- Deliver R-RSZ-01 / REQ-RSZ-01b using the retained Engine pixel resize command,
  dedicated Alt-capable input intent, and real Settings Keep/Disable/Rebind rows.
- Match KDE: Win+Alt+H/J/K/L and arrows grow; Win+Alt+Shift variants shrink.
  Steps are 12/14/16/18/20px, keyed by focus/direction/mode; preserve adjacent
  shares, minima clamps, origin/lifetime/suspension and overlay refusal fences.
- Preserve existing input pairing, exact modifiers, remapping and E8 masking.
  No shared-core changes unless demonstrably necessary; shared core is delivered KDE-side.
- Native build/test/fmt/clippy gates, independent review, bounded owned-window
  native checks and clean stop/restore; physical input acceptance is user-owned.

## Approach and bounded units

1. Windows classifier/settings/UI plus portable input tests.
2. Windows retained Engine/runtime resize dispatch plus semantic fixtures and
   narrowly needed exact-owner test CLI plumbing.
3. Independent review, native gates and scoped live evidence.
4. Matrix/spec/decision delivery pointers, concise backlog status, archive
   and verify CI.

## Material decisions

- Tentative, pending user review: resize defaults remain enabled under Authentic
  and Compatible because no Windows owner/conflict is recorded. Compatible
  disables only known conflicts, not unknown ownership. Both presets and actual
  rebound chords must explicitly disclose unknown ownership and unproven live
  containment. This does not claim conflict-free status or foreign takeover.
- Synthetic SendInput cannot prove physical shortcuts because the product
  injected-input fence rejects them. Exact-owner test-needed commands may prove
  downstream intent/geometry only; native physical interception stays user-owned.
- Tentative, pending user review: `resize --direction DIR --mode MODE` is a
  test-needed exact-owner, consume-once normal-owner control. It captures live
  foreground through the existing origin map and dispatches through the real
  keyboard drain with production fences. No carried HWND or injected-input
  bypass; proof owners refuse. Literal arguments are strictly validated.
- Repeat tracking mirrors KDE focus/direction/mode state, advancing only after
  pre-dispatch fences and before Engine dispatch. Key-up does not reset it;
  repeated discrete presses on the same subject/direction/mode also accelerate.

## Authority and live bounds

- User authorizes implementation, native gates, commits/pushes, input hooks,
  movement/hiding/restyling/overlays for identified disposable Notepad,
  Calculator and Paint windows, and synthetic input on this Windows desktop.
- Never target pre-existing windows or the opencode terminal. No Win+G/F11,
  registry/policy changes, installs or forced-loss probes in this change.
- Before live runs record artifact/owner/display/settings baseline, prove
  independent stop/restore, create settings before startup to avoid the modal,
  bound each run and end stopped with hooks released, windows revealed,
  owned settings restored and disposable apps closed.

## Accepted implementation and review

- Delivered 2026-10-11, base `f794cf9` plus delivery commit. Windows adapter
  only; no shared-core/protocol/KDE edits. Eight logical resize rows expose
  sixteen default physical chords. Dedicated mode-carrying classifier, queue,
  E8-mask and origin intents never become Move commands.
- Exact modifier/rebind/disable matching, held-pair suppression, queue saturation
  and aliases verified in `tests/snapkey.rs`; catalog/conflict/preset/modifier
  validation and all-eight canonical letter slots in `tests/settings.rs`.
- `dispatch_resize_intent` checks takeover, suspension/elevation, fresh origin,
  lifetime/proof/scope, subject float/sticky, workspace-floating, overlay and
  pending/drag gates; retained `CoreCommand::Resize` replies go through existing
  planned-writes/native apply/readback machinery.
- Retained Engine fixtures in `tests/tiling.rs` prove first/repeat steps, mode
  reversal, edge/cross-axis no mutation, horizontal/vertical adjacency, far
  sibling share preservation, shrink-side minimum clamp/exhaustion, focus/order
  retention. Windows-specific dispatch fences are source-reviewed, not implied
  by portable Engine tests.
- Independent review accepted routing, masks, fences, Engine integration and
  exact-owner transport. Three low findings resolved: inward-down canonical
  letter first (regression assertion added), subject-before-workspace refusal
  priority, and honest actual/unknown CLI log labels instead of invented values.

## Native gates

- Rust 1.99.0 `x86_64-pc-windows-msvc`, mise auto-install disabled, 2026-10-11.
- Locked build and test for tiler-core, tiler-protocol, tiler-kwin-effect-ffi,
  tiler-windows passed. Windows lib 230, tiling 91, snapkey 76, settings 30;
  every executed suite passed with zero failures.
- `cargo fmt --all -- --check`, four-package all-target clippy `-D warnings`
  and `git diff --check` passed after review corrections. No installs.
- Hosted CI/publication evidence recorded below after push; Linux/KDE gates
  are hosted checks, not native Windows execution.

## Second agent-live sweep (2026-10-17)

- Physical resize acceptance remains blocked for scripted input: the product
  intentionally rejects injected keys. An active scoped hook and owned
  foreground were established; harmless unassigned input produced no resize,
  move or focus dispatch and left settled frames unchanged. This is fence
  evidence, not physical shortcut acceptance or a reason to bypass the fence.
- Snapkey, settings, tiling and Windows library supplements passed. No Win+M
  or resize chord was sent through the shell; physical suppression/hold/rebind
  and remaining native geometry legs stay open. Settings/SPI restored, ledger
  clean and owned disposable windows subsequently closed.

## Agent-observed native evidence

- Windows 11 Pro build 26300, primary DISPLAY1 full [0,0,2560,1440], work
  [0,0,2560,1380], DPI120. Medium integrity 8192, session1. Live artifact
  `target/debug/tiler-windows.exe` SHA-256
  `F5BADB46FD316418008B5D81E95540619ABE91B390F2FEEBD46A1CE5EDCA3681`;
  Explorer-launched copy identical. Source base `f794cf9` + nine-file feature
  diff, bound to the delivery commit; no code changed after gates/live checks.
- Successful resize run: `evidence.json`,
  SHA-256 `26753C20FC595C302467FBE9413911803F12FCB8CD8C23324D267CB42A3F30FD`.
  Phase-zero owner PID20256 creation `01dd58e7cd94ca95` independent stop/restore
  passed before effects. Exact scoped owner PID24768 creation
  `01dd58e7cdef8aae`; scope only notepad.exe/mspaint.exe, baseline had neither.
  Owned Notepad HWND20710590 PID26796 creation `01dd58e7cfbaee0e`, Paint
  HWND2360828 PID27208 creation `01dd58e7cfbc7fe1`; later Paint HWND7995644
  PID18556 creation `01dd58e7de222d8f` supplies the vertical neighbor.
- Real keyboard drain via exact-owner CLI: Notepad right/outwards width
  1282->1294 (+12), neighbor -12. Subsequent indices 1..5 add 14/16/18/20/20
  (total +100 including press0). Mode-change inward press0 subtracts12.
  Left outer edge returns Engine `unchanged`, rect bit-identical. Vertical
  down grow adds12 height; top-edge up returns unchanged. Native maximized
  and fullscreen requests refuse with matching resize outcomes and no resize
  writes. Project fullscreen exit restores the slot.
- Eight additional inward presses verified the shrinking schedule, but did
  not reach the live minimum floor. Floor exhaustion is offline evidence only.
- Successful Settings run: `evidence.json`,
  SHA-256 `98EEBF20B398045649B7D3FC1E76C66A286AA4C86C01EE92DC53AC91441C1827`.
  Settings PID32456 creation `01dd58e8b092ecdb`, same artifact, exact-owned
  controls. All 8 resize rows available with defaults/unknown-owner notes.
  Wrong-arm rebind refuses; Disable Apply revision1, Keep Apply revision2,
  Win+Alt+O Rebind Apply revision3 verified saved JSON. Staged Disable then
  Revert reloads Keep without a file write. Preset disclosure read back.
- Teardown verified: owner stopped/process gone, hooks released through exit,
  independent restore clean, no owned hidden windows remain, settings original
  absence restored, SPI arranging1/pen35 restored, all owned apps and Settings
  closed. Inventory returned to 19 with identical executable groups; terminal
  and pre-existing windows preserved. No registry/policy or forced-loss probes.

## Failures, discoveries and remaining acceptance

- No product semantic failed approach. Failed fixture/harness runs are not
  accepted feature evidence: Notepad stub PID/HWND oracle (inventory discovery
  repaired it); log scan skipped production line in the CLI poll batch (scan
  from pre-request mark repaired it); Settings UIA custom controls exposed as
  Pane, not List/Text (hybrid int-only native messages + Pane readbacks repaired
  it). Each failed run cleaned up its exact-owned actors and settings and left
  no owned residue; owner runs used stop/restore.
- Three Paint minima (864px each) overconstrain 2560px, so an initial fixture
  correctly returned unchanged. Successful horizontal fixture uses two apps;
  the later vertical split adds a third without the same-axis overconstraint.
- Physical Win+Alt/Shift+Alt suppression, alias delivery/hold feel, takeover-off
  release and rebound owner adoption remain user-owned (synthetic injection
  is rejected). Native workspace-floating/busy/race refusals, exact nested
  scope, larger app-declared minimums and multi-output/game/elevation journeys
  remain pending; bounded intentional-float/sticky and portable-floor evidence
  is added below. Source review and portable policy are not native proof of the
  remaining legs.
- Tentative preset and test-needed CLI choices above require user review.
  Implementation complete; next acceptance action is the physical item6
  journey in backlog, not another injected-input experiment.

## Publication

- Pushed `241cf7b52dacd697528087344060d6db67659107` (`Add Windows keyboard
  resize`). Rebase preserved concurrent `9fdfc29`/`37af573` packaging work
  without conflict; it did not change Windows/core Rust or the live artifact.
- CI [38077899121](https://github.com/beefsack/OmniTiler/actions/runs/38077899121)
  completed success: windows, rust, kwin, shell, native and macos. No failed
  jobs or repairs. Final evidence-record publication is documentation only.

## Scoped native refusal follow-up (2026-10-18)

- Normal owner scoped only to identity-tagged disposable helpers; source
  `e8b8eb34` plus documentation-only working changes. Owner/helper SHA-256 are
  the same `366a3619...` / `3825ce9a...` artifacts fully recorded in the
  [scoped idle follow-up](windows-owner-event-idle.md#scoped-native-follow-up-2026-10-18).
  No keyboard injection or input-filter change.
- **Pass, intentional-float refusal:** establish exact helper foreground,
  toggle float and observe production `float-applied`; capture both frames
  after that geometry change. Exact-owner `resize --direction right --mode
  outwards` reaches production `resize-refused-floating` at tick 169, with both
  native frame readbacks identical to that post-toggle baseline. Toggle-back
  reports `unfloat-applied` and restores the raised-band preimage.
- **Pass, sticky refusal:** the equivalent post-sticky baseline and resize
  request reach `resize-refused-sticky` at tick 274; both frame readbacks stay
  identical. Toggle-back reports `unfloat-applied` with topmost restoration.
- These are downstream native refusal checks, not physical chord/modifier,
  hold/release, Esc or shortcut-suppression acceptance. Helpers were PMv2, but
  the separate helper inspection process was DPI-unaware: frame comparisons
  use the same virtualized convention, not a claim of physical-pixel geometry.
- External evidence `wu-resize-20261018-165251-30608`: `report-phase3.json`,
  `report-phase4.json`, initial `report.json`, and owner log
  `run-01dd5ec4eb1ea493.log`. Foreground acquisition failures were parked without
  injecting input; only the observed refusal legs above are accepted.
- Helpers were visible before exact tagged close. Final `just stop` completed;
  owner readiness was false and ledger/stop/workspace/resize requests absent.
  Successful SPI reads and actual taskbar `ABM_GETSTATE` matched their preimages.
  Settings comparison in this refusal harness was decoded-text equality, not
  byte-array equality; it does not establish byte-exact restoration.

### Physical-coordinate portable-floor supplement

- Fresh trace run at `3f2583eb`, same Windows artifacts, scoped to two passive
  PMv2 helpers. Native outer/DWM visible bounds were read in a verified PMv2
  thread context, not through the DPI-unaware helper inspection process.
- Observed 47 applied inward requests: 12/14/16/18/20 px schedule, then capped
  steps, ending with an 8 px transfer to a 360 px visible width. Request 48
  returned production `unchanged` with the same physical pair frames and
  foreground. No mismatch was reported. Only one post-floor unchanged request
  was observed, not two or physical held-key acceptance.
- Diagnosis: the harness incorrectly expected the helper's declared 150 px
  width minimum to be the resize floor. Existing `cosmic_v1.rs` enforces a
  portable 360 px horizontal / 240 px vertical keyboard child floor;
  `session/ops/resize.rs` returns `Unchanged` when that clamp cannot transfer
  more. The observed 360 px stop matches that contract; no Windows defect or
  policy change is selected. Raw harness `fail` remains intact as an oracle
  error, not a product-failure claim. Larger app-hint clamping (including the
  distinct projected `resize-noop` path), vertical/nested minima and the full
  physical matrix remain pending.
- Final `mise exec -- just stop` completed. Raw settings bytes were captured
  with `WriteAllBytes(ReadAllBytes(...))` before launch and matched afterward
  by byte equality, SHA-256 and length; no settings-edit leg ran. Successful
  SPI reads and actual taskbar state matched their preimages. Helpers were
  visible before exact close, exited, owner readiness was false and all four
  ledger/request files were absent. This fresh run's stronger restoration
  proof does not retroactively repair earlier text-only comparisons.
- External evidence `wu-minfloor90-20261018-170547-30608`: `report.json`,
  `harness-used.ps1`, retained pre-stop `owner-run.log.copy`, completed
  `owner-run-stopped.log`, raw settings preimage, `verification-used.ps1` and
  `verify-post-stop.json`. An earlier 40-request sample stopped above the
  portable floor and establishes no clamp result; its evidence is not used as
  floor acceptance.
- Independent source/evidence review accepted these bounded findings. Final
  locked four-package build/test, all-package formatting, strict all-target
  clippy and whitespace gates passed at `3f2583eb`: 2,852 passed, 0 failed,
  1 ignored. External `live-sweep-gates-20261018` retains `final-gates.json` and
  `*-final.log`; no production or shared-policy changes.
- Post-integration gates at `871d2bbc` also passed: locked four-package
  build/test, formatting, strict all-target clippy and whitespace; 2,868 passed,
  0 failed, 1 ignored. An initial test invocation exceeded the 120 s tool bound
  without a suite result; the longer-bounded retry completed without source
  changes. The interrupted run's stall cause is unproved, not classified as a
  product defect or a passing run. Evidence remains separately retained in
  `live-sweep-gates-20261018/post-rebase` and `post-rebase-long`. Live observations
  remain pinned to their earlier recorded artifacts, not a world-route cutover.
