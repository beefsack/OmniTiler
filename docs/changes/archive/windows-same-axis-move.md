# Windows live same-axis move setting

## Goal, scope and acceptance

- Deliver Windows handoff item 3, REQ-MOV-03 / R-MOV-03/09/10: additive
  schema-v1 `core.same_axis_move`, missing defaults `group-with-neighbor`,
  exact alternative `swap-with-neighbor`, strict invalid-file/last-good handling.
- Functional Settings labels with COSMIC / i3, sway tooltips; Apply/Revert.
  Presets preserve this setting. Subsequent moves read the live value without
  rebuilding trees; retained Engine owns movement semantics and shares/focus.
- Windows adapter only; shared core/KDE and other handoff items out of scope.
- Acceptance: schema/live and real Engine regressions, native build/test/fmt/
  strict clippy, independent review, scoped native Settings/movement evidence,
  matrix/spec/decision pointers, publication and CI.

## Approach and bounded units

- Implementation, tooltip correction and independent contract review accepted.
  Review required horizontal left/right unequal-share, nested-edge parity and
  a direct production `apply_live_settings` no-rebuild regression; all repaired
  and independently re-reviewed with no remaining blocking finding.
- User standing authority covers owner, Notepad/Calculator/Paint, synthetic
  input/cursor. Never touch pre-existing windows/terminal; no Win+G/Win+F11,
  registry/policy changes, games, elevation or forced crash probes.
- Read live-windows-testing.md; record source/artifact, owner PID/start/path/
  session/integrity, display rectangles/DPI and owned app identities. Prove
  independent stop/restore before live effects. End stopped, hooks released,
  hidden apps revealed, settings restored and disposable apps closed.
- Physical input/OS containment remains user-owned; unsupported outcomes TBD.

## Material decisions and outcome

- No shared source change required. No new product decision or tentative choice.
- Native Win32 tooltips name COSMIC / i3, sway; visible text is functional only.
  Required correction also separated workspace/same-axis radio groups:
  otherwise selecting one setting could clear the other setting's radio.
- Ran native allowlisted build/test/fmt/strict clippy and diff check on
  2026-10-11, base `fafcd31` plus accepted diff: passed. Rust 1.99.0 MSVC,
  `MISE_AUTO_INSTALL=0`. Linux/KWin/macOS gates go to publication CI.
- Portable real Engine regressions cover H4 left/right traveling unequal
  shares/focus, default wrap, binary/group-neighbor/nested escape/edge parity.
  Native production adoption test proves false wake result and unchanged
  retained snapshot; schema tests cover missing/default/exact tokens, invalid
  value/type, atomic save, live last-good and presets preserving the field.

## Agent-observed native evidence

- 2026-10-11 physical Windows 11 Pro build 26300, DISPLAY1 primary full
  `[0,0,2560,1440]`, work `[0,0,2560,1380]`, DPI 120, medium owner 8192,
  session 1, Explorer-parented; newly created Notepad HWNDs only.
- Evidence: `evidence.json`.
  Source base `fafcd31` plus delivery diff SHA-256
  `F2C546E0189C33B62F75B92E895704BD11FBB75306C309CCE0868729F2BA4DC9`;
  artifact copied to that run's `bin/tiler-windows.exe`, SHA-256
  `735CEE53B4D031FC86EAAAF8A1273238B802E467473E1F9CE12ACE00070CAE32`.
- Proven independent graceful stop/restore first on one owned Notepad, then
  Settings Apply both exact tokens, independent radio groups, saved-file Revert
  (byte-identical file), functional labels and owned tooltip popup existence.
  Tooltip content is code/unit verified, not a physical hover observation.
- Live tiled owner PID 19492 creation `01dd58c545af4582`, exact copied executable,
  manages four new Notepad HWNDs. Both same-axis-only Apply changes leave all
  rectangles/visibility unchanged; `settings-applied` reports `lanes:[]`.
  Exact owner/app identities, readbacks and settings preimage are in evidence.
- Cleanup passed: owner/watcher/settings exited, hooks released, independent
  restore and ledger clean, test apps closed, settings absent preimage restored,
  SPI baseline restored, pre-existing terminal untouched.
- Two failed harness runs (automatic `$PID` parameter collision and wrong
  single-process Notepad close oracle) retained under `ws-verify13/20261011-013502-22524`
  and `20261011-013617-19556`; exact-owned residue recovered. No product semantic
  failed approach inferred. Successful run used repaired identity/close checks.
- Live directional moves remain user-owned: product hooks reject injected
  input; CLI has no move verb. No proof-gate bypass or synthetic physical
  acceptance claim. R-MOV-03/09/10 move effects verified offline only;
  physical right/left/unequal-width/group-neighbor journey remains pending.
- Delivery complete for implementation/offline and scoped native settings
  verification. Publication/CI result follows in the delivery handover.
- Pushed `755aab8`; publication rebase preserved concurrent `e6f396d` tentative
  triage work (one mechanical adjacent-row spec conflict, no source conflict).
  CI [38060844283](https://github.com/beefsack/OmniTiler/actions/runs/38060844283)
  completed success: windows, rust, kwin, shell, native and macos. No CI repair.

## Scoped follow-up (2026-10-18)

- Native move-topology leg not run: the normal owner ignores injected keys and
  its exact-owner CLI has no move verb. No input-filter bypass or duplicate
  Settings-only journey was used as a substitute for actual movement.
- Both-policy native movement, traveling shares/focus and physical triggers
  remain pending. External evidence `wu-sweep-20261018-164250-30608`,
  `report.json` (`check16-same-axis`); retained offline evidence is unchanged.
