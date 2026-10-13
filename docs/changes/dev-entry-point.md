# Cross-platform foreground development entry point

## Goal and acceptance

- User decision 2026-10-13: `just dev [verbose|trace]` is the foreground
  full-solution dogfooding entry point on each supported desktop. Logs stream
  to the terminal; Ctrl+C performs verified teardown.
- Root `justfile` selects platform imports using conditional attributes and
  requires just 1.56.0 or newer. Existing platform recipes stay accessible.
- Linux dispatches once from case-insensitive, colon-separated
  `XDG_CURRENT_DESKTOP`: KDE selects `dev-kde`, GNOME selects the explicit
  unsupported stub, other values fail with detected value and supported targets.
- Windows uses the existing normal user tiling launch, Explorer broker,
  readiness and provenance contracts; refuses an existing owner; follows the
  run log until interruption or owner exit; verifies stop, restore and clean
  ledger on exit. `trace` enables tracing; `verbose` retains default logging.
- The previous Windows non-tiling ten-minute loop becomes `lifecycle-smoke`.

## Approach and bounded work

1. Investigate Windows launch/stop contracts and harmless Ctrl+C mechanics.
2. Extract platform recipe files, add Linux dispatch and macOS stub.
3. Implement Windows foreground launch and teardown using existing helpers.
4. Update current command references and the durable entry-point decision.
5. Review lifecycle boundaries and run offline gates; retain this active record
   until Windows live acceptance and Linux KDE streaming/teardown checks pass.

Product code and KWin sources are outside scope. Existing multi-output work
is separate; shared edits are the decisions insertion and the multi-output
runbook's stop-command spelling.

## Material decisions

- Tentative, pending user review: platform files live under `just/`, and macOS
  has a small explicit unsupported `dev` stub.
- Windows cleanup belongs inside PowerShell `finally`, not subsequent just
  recipe lines. Do not register a PowerShell-scriptblock console cancel handler.
- Final cleanup reports use direct console output because Ctrl+C can suppress
  PowerShell pipeline output even after cleanup completes. The wrapper exits
  with the computed cleanup result; just's interrupted command-shell layer can
  still return 1, so the report and independent readbacks establish teardown.
- Tentative, pending user review: when KDE and GNOME both occur in the desktop
  list, KDE wins. Windows foreground payloads use unique child directories
  under `target/windows-dev`; a runtime pointer makes `just stop` recover the
  matching payload. Build-output copies are retained without automatic pruning.
- The unique executable path attributes a broker launch before readiness.
  Cleanup compares the full captured owner identity when available, refuses
  foreign ledger owners, and waits up to five seconds for publication when
  interrupted during startup. Unconfirmed exit retains the pointer and reports
  recovery rather than success.

## Evidence and remaining checks

- Offline gates (2026-10-13): just 1.58.0; root Windows recipe listing,
  root `--fmt --check --unstable`, direct parse/list/show for all three
  platform files, dev-mode/scoping dry-runs, four edited PowerShell scripts'
  parser checks, and `git diff --check` pass. Existing Linux recipe bodies
  match the original apart from the dev rename and new dispatcher; retained
  Windows recipe bodies match apart from the lifecycle action rename.
- Lifecycle review exposed unproven-startup cleanup, fail-open preflight,
  interrupt misreporting, creation-only matching and whole-log rereading.
  Unique payload attribution, full identity matching, fail-closed probes,
  incremental reads and bounded startup recovery address these findings.
- Failed approach: a scriptblock `ConsoleCancelEventHandler` ran without a
  runspace on the signal thread. Handler-free PowerShell `finally` succeeds.
  Real Ctrl+C bypasses `catch`; an incomplete body with no error record is
  classified as interruption, while ordinary errors remain failures.
- Final harmless OS Ctrl+C probe through actual just -> pwsh command shell ->
  pwsh file wrapper executed the real cleanup/reporting helpers with a dummy
  owner and mocked ledger: native cleanup work completed, dummy owner exited,
  ledger cleared, pointer removed, and verified interruption reported. Inner
  cleanup returned 0; just itself returned 1 from interruption. No success is
  inferred from the outer status.
- Mock checks cover normal exit, growing-log streaming, body/cleanup errors,
  missing logs, before-ready matching/no ledger, foreign/replaced identities,
  delayed publication, and canonical pointer containment. The native process
  creation time matches the .NET creation-time representation in a dummy probe.
- CI and Rust sources are unchanged by this work; no Rust gate is required.
  CI execution and Linux native execution remain pending.
- Current command references and proof source-hash lists use the root
  entry point. Historical evidence and archived records retain old command
  spellings. The multi-output runbook changes only its stop command spelling.
- Scoped Windows foreground acceptance now passes as detailed below. Pending:
  whole-desktop startup, tray Stop UI, visual/physical shortcut acceptance, and
  Linux KDE streaming and Ctrl+C teardown. This record stays active.

## Scoped Windows live outcome (2026-10-13)

- Normal tiling with disposable Notepad, Calculator and Paint windows preserved
  unrelated window geometry, visibility and styles. Three owned top-level
  windows were managed; hosted Calculator attribution used a fresh root and
  matching child identity. An unrelated childless host remained excluded.
- Ready identity, medium integrity, Explorer parent, full scoping arguments,
  trace mode and provenance/artifact hash binding passed. The live trace stream
  grew by 4,022 bytes during observation and contained native run-log events.
- A second unscoped `just dev` refused while the first owner was ready. Full
  owner identity, payload pointer and test-window geometry remained unchanged.
- Defect found and fixed: live Ctrl+C completed verified teardown but omitted
  its final terminal report. A harmless reproduction showed that `Write-Output`
  was suppressed by the stopping pipeline. Direct `[Console]::WriteLine` and
  exit with the computed result restore both success and failure reports.
  Offline normal, interruption and genuine cleanup-failure cases pass.
- Final live trace Ctrl+C targeted only the separate test console's process
  group. The terminal printed `dev interrupted: verified stop+restore, owner
  exited and ledger clean`; just returned 1 from interruption. Independent
  readbacks confirmed owner exit, no owner windows, clean ledger/requests and
  pointer, visible test windows with restored styles, and Mouse Snap restored
  to its captured preimage. Logs confirmed clean input unhook, tray teardown,
  overlay teardown and stopped owner lifecycle.
- `just dev verbose` contained no added trace flag. Idle logging remained
  bounded; exact-owner `stop` from a second process caused foreground streaming
  to end, drain teardown log events and print `dev success: owner exited and
  ledger clean`. Native stop and just both returned 0. This tests the owner-exit
  streaming path, not the tray Stop UI.
- Normal `tile` intentionally leaves geometry in place. Test windows remained
  visible with unchanged styles after teardown; original positions were not
  claimed restored. Disposable windows were closed by their verified handles.
- Early harness coordination/sender failures supplied no mid-run or Ctrl+C
  acceptance evidence; graceful recovery and readbacks completed. The final
  isolated sender/stream fixture passed with a harmless process before the
  successful live interruption check. Raw identity and window evidence stays
  outside repository records.
- Final state: no test owner or test applications, ledger/request/pointer
  residue absent, unrelated windows unchanged, Mouse Snap preimage restored.
- Post-fix PowerShell syntax, just format and whitespace gates pass. Product
  Rust and CI files are unchanged; no Rust gate is required for this fix.

## Acceptance journeys

### Windows

1. Follow the Windows live guide; use a normal, non-elevated terminal, ordinary
   disposable application windows and an accessible separate recovery terminal.
2. From the repository root run `mise exec -- just dev trace`. Confirm build,
   owner-ready, log and provenance paths, streamed logs, normal tiling, hooks,
   overlays and tray. The default run is whole-desktop tiling.
3. In the recovery terminal run `mise exec -- just dev`: it must refuse without
   stopping the first owner or changing its windows.
4. Press Ctrl+C once in the original terminal. Confirm the verified cleanup
   message, owner/tray/overlays gone, hooks released, windows visible/restored,
   and no ledger/request residue. A nonzero just interruption status alone is
   not failure when the verified cleanup result and readbacks pass.
5. Repeat with `mise exec -- just dev verbose`. Stop from the owner's tray:
   streaming must end on its own and report clean owner exit and ledger.
6. Repeat with `mise exec -- just dev trace --scope-exe notepad.exe`; confirm
   passthrough scoping, then Ctrl+C. If any cleanup is uncertain, use
   `mise exec -- just stop` from the recovery terminal and preserve the result.

### Linux KDE

1. Follow the KWin live guide in the devenv shell with just >= 1.56, from a
   known DOWN development session. Run `just dev trace` with the real KDE
   `XDG_CURRENT_DESKTOP`; confirm it prints selection of `dev-kde`.
2. Confirm the full build, tiling, native effects/shortcuts, tray, and labeled
   Planner/KWin/tray log streams. Press Ctrl+C; `just dev-status` must report
   DOWN with verified teardown and no owned tray remaining.
3. Repeat `just dev verbose` and teardown. Separately check GNOME/unknown
   dispatcher errors and case-insensitive colon-list selection in a harmless
   dispatcher fixture; do not mislabel a GNOME environment as KDE to launch it.
