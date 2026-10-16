# Windows restart intent

- Goal: handoff item 8, R-RST-01 / R-FLT-05. Intentional floats and sticky floats from both origins survive owner restart.
- Scope: Windows adapter on-window markers, classification hydration, exact-lifetime fences, regression coverage and agent-owned restart journeys. No store file, recovery-ledger changes or layout restoration.
- Acceptance: settled explicit intent persists; fresh admitted lifetimes hydrate before tiling; sticky remains sticky; corrupt/unreadable markers diagnose and mean no intent; markers grant no write/recovery authority. Same-owner origin behavior remains; post-restart un-stick TBD and REQ-RST-01c unresolved.
- Approach: namespaced SetProp markers, following existing sticky mechanism. Reserve fixed tile-override marker for item 13/D7. Tentative, pending user review: stronger project namespace for all classification markers, without accepting old ambiguous names.
- Units: Windows implementation/regressions; independent authorization/lifetime review; native gates and scoped live restart verification; documentation and publication.
- Verification: native build/test/fmt/clippy gates for core/protocol/FFI/Windows; diff check; exact-owner CLI journeys with owned disposable windows and independent stop/restore proof; CI after push.
- Live authority: Windows 11 physical host, ordinary integrity, only session-created Notepad/Calculator/Paint or owned helper windows; owner hooks/overlay and synthetic input authorized. No pre-existing window mutation, Win+G/Win+F11, games, elevated or protected resources. End owner stopped, hooks released, hidden windows revealed, owned settings restored and test apps closed.
- Status: item 8 delivered 2026-10-11, base `e48aed1` plus delivery commit. Windows adapter only; no shared-core, KDE, store-file or recovery-ledger changes. Item 13/D7 remains unwired.

## Accepted implementation and review

- `OmniTiler.Sticky.v1`, `OmniTiler.FloatIntent.v1` and reserved
  `OmniTiler.TileOverride.v1` are distinct window-lifetime properties.
  Tentative, pending user review: rename sticky too and ignore the old
  `OmniTilerSticky`; pre-release upgrades do not hydrate that old name.
- Successful explicit native float/sticky application writes and verifies
  markers through existing held-process/member-tag gates. Settled unfloat and
  sticky-off clear the corresponding markers; sticky-off-to-float repairs
  ordinary intent. Adoption preserves live frames and markers, changes only
  classification, and grants no native-effect or independent-recovery authority.
- `adopt_restart_markers` stages all valid admitted markers, then reconciles
  once per domain before first tiling. A mixed four-window real-`TileLoop`
  regression covers both observation orders; both sticky origins also survive
  two fresh Engine owners. Corrupt values, absent markers, lifetime refusal,
  actual property install/remove and retained Engine membership are covered.
- Independent review accepted exact-owner controls, admission/lifetime fences
  and the narrow owned-band hide exception. Native-success/readback failure
  preserves local/native truth and logs a degraded outcome.
- `GetPropW` provides NULL for absence without a reliable read-error distinction.
  NULL is therefore silent no-intent; present invalid values diagnose corrupt
  no-intent. The classifier supports diagnosed unreadable inputs, but the native
  reader cannot claim it distinguishes every unreadable NULL from absence.
- Tentative, pending user review: `workspace --float` / `--sticky` are test-needed
  consume-once exact-owner controls, reusing the workspace request transport.
  They resolve current foreground and call production intent dispatch with all
  fences; proof owners refuse, no HWND is transported. Outer logs say `cli`;
  the inner intent uses the existing down-edge vocabulary, not physical evidence.
- The live scope journey exposed an existing contradiction: explicit floats
  raise keep-above, but hide admission refused all topmost windows. Only a fresh
  exact-key runtime record proving a project-raised band (`Some(false)`) can
  bypass that one refusal. Pre-existing topmost and marker-only windows still
  refuse; identity, lifetime, held process, journal and restore gates remain.
  Per-window `workspace-hide` diagnostics are bounded to 64 per selection.

## Native gates and live evidence

- Rust 1.99.0 MSVC, mise auto-install disabled. Locked build/test for core,
  protocol, KWin-effect FFI and Windows passed after the final code change;
  Windows lib 243, all executed suites green. Whole-workspace fmt check,
  four-package all-target clippy `-D warnings` and `git diff --check` passed.
- Agent-observed, Windows 11 Pro 26300, medium integrity 8192/session 1;
  DISPLAY1 full `[0,0,2560,1440]`, work `[0,0,2560,1380]`, DPI120.
  Four exact-owned Explorer-brokered helper HWNDs: 5047720/PID3668,
  5047756/PID22016, 13305580/PID27304, 11732628/PID22948.
- Artifact `target/debug/tiler-windows.exe` and launched copy SHA-256
  `9B5928E1EBC528B5B726909B2C8367BA8C99602CFAC43803D64689AB16463E17`.
  Source base `e48aed1` + eight-file code/test diff SHA-256
  `FB50517DBA00427B8B76A43DC5A909C6C2C6EE911E2BE47530E59DE33CD71C55`,
  bound to the delivery commit; no later code changes.
- Evidence: `final-batched.json`,
  SHA-256 `94E9FB50821DF6E0B2C3F6BDE45B2F835F49C8D85B92B33A260D50DFEBC6FBDA`.
  Owner generations PID20752/creation `01dd591c97827f7a`,
  PID32696/`01dd591ca1a5353a`, PID15432/`01dd591ca57c34e1`;
  launch `tile --user-start --seconds 300 --trace --scope-exe tiler-test-window.exe`.
- Independent graceful stop/restore proof preceded the accepted journey.
  Two graceful restarts kept both sticky origins and ordinary intentional float,
  with raw markers unchanged before stop, after restore and after new-owner
  readiness; float frames unchanged and no hydration writes. Before restart and
  after each restart, selecting WS2 kept both stickies visible at the same frame
  while ordinary float/tile hid; returning WS1 revealed them. No candidate
  rejection/unfloated diagnostics in the accepted journey.
- Post-restart un-stick observed: prior-tiled marker cleared and tiled; prior-float
  sticky marker cleared, float marker remained, live float frame kept. These are
  observations only: R-FLT-05 un-stick remains TBD, REQ-RST-01c remains unresolved
  OPEN for this delivery. Physical shortcuts/app dogfood remain user-owned.
- End: owner/processes stopped, hooks released through exit, zero overlays,
  independent restore clean, all four helpers closed, original settings absence
  restored, arranging1/pen35 restored, inventory19->19, unrelated windows intact.

## Failures and reusable discoveries

- One semantic restart failure: per-window hydration seeded a later marked
  window as retained tiled focus, then its float flip rejected `focus-mismatch`.
  The marker was intact. Four-window reproduction failed before the batched
  repair and passes after; the accepted live journey verifies the same fix.
- Temp-harness faults are not product evidence: AST function imports omitted
  topmost constants/transitive dependencies; dot-sourcing `windows-dev.ps1`
  reset `$TileArgs` to empty. Literal native masks and uniquely named launch
  arguments set after sourcing fixed them. Prior keep-above/OS-broker and
  marker-loss explanations were withdrawn. No OS security changes were needed.
- An early forced-loss preflight ran beyond this unit's graceful-only
  brief. It reported exact-owned cleanup; those probes are not accepted feature
  evidence. Subsequent units explicitly excluded forced loss.
- Floated foreground can produce existing reconcile focus-mismatch diagnostics;
  shared core was left untouched. This is separate from restart hydration.
- Item 13+17 next: wire bounded max-track observation, schema-v1 predicate/UI,
  automatic origin and D5/D6 no-touch admission, then D7 using the reserved
  tile marker/native ops. Preserve batched hydration and exact-lifetime fencing;
  enable Engine fixed-size admission only after full Windows observation/origin
  wiring. Reuse exact-owner CLI and corrected harness dependencies above.

## Publication and hosted fixture repair

- Pushed `a3b6cdaf9ad4f2a5742fced2aa12b8455277f1ce`
  (`Preserve Windows float and sticky intent across restart`). Rebase preserved
  concurrent packaging/licensing `0f5e75e`/`8e04a80` without conflict; no Windows
  or shared Rust production changes entered from that rebase.
- CI [38100860417](https://github.com/beefsack/OmniTiler/actions/runs/38100860417):
  rust, kwin, shell, native and macos passed; four Windows native hide-classifier
  fixtures failed at the earlier integrity gate. Hosted Windows runs elevated;
  owned invisible windows inherit that integrity. The product correctly refused
  them before topmost/visibility checks, unlike the medium-integrity dev host.
- Fixture-only correction asserts integrity refusal on non-medium runners and
  retains actual topmost/visibility assertions on medium runners. No tests
  ignored, no elevation or product-gate changes. Native four-package tests,
  fmt and strict clippy passed again; production artifact/live evidence above
  remains current. Follow-up CI outcome is reported after publication.

## Agent-live follow-up (2026-10-17)

- Single-output scoped ordinary-app sweep at `dffdf871`: two fresh centered
  restart journeys preserved both sticky origins, ordinary float intent,
  markers and exact frames at pre-stop, owner-dead, post-independent-restore,
  first new-owner observation and settlement. Workspace switches verified the
  sticky visible set and ordinary float/tile membership; return focus matched
  the observed member. New-owner tick 1 adopted all three with zero writes.
  Evidence: `sendunit-j2r/20261017-071531-23728` and
  `sendunit-j2r/20261017-072700-13132`.
- FAIL, parked: an earlier centered-start restart at the same revision moved
  all three floats into tile-shaped frames. Run `01dd5da7a422aac2`, tick 1:
  two `sticky-adopt` and one `float-adopt` returned
  `candidate-rejected duplicate-window`, followed by four tiled writes. Tick 2
  adopted intent without repairing the frames. A quadrant-start repeat also
  rejected adoption initially but masked the geometry change. Passing fresh
  runs do not erase these failure receipts; restart acceptance remains partial.
- Rollback in `crates/tiler-windows/src/tiling_sys/restart.rs` and fresh/retained
  Engine admission are implicated, not conclusively rooted. Static branch
  isolation was inconclusive; a temporary reconstructed-input Engine probe
  reportedly adopted cleanly, but no retained probe artifact independently
  confirms that result. No fix or
  shared-core/KDE behavior change was made. Options: capture the rejected
  candidate's full bounded facts, then add a failing regression; consider
  Windows-only hydration hardening only with that regression. Root trigger and
  rejection handling remain parked; no new layout-restoration policy selected.
- Post-restart un-stick observations remain just observations: tiled-origin
  became tiled, float-origin retained ordinary float intent. The existing TBD
  policy is unchanged. The [minimal discriminator](../../spec/reference-outcomes.md#windows-restart-hydration-discriminator)
  separates clean hydration from rejected first-observation adoption.
- Final cleanup stopped all owners, removed the recovery ledger, restored saved
  settings bytes and SPI/Snap values, and closed identified disposable fixtures.
  The protected terminal's apparent frame discrepancy was mixed-DPI readback,
  not a move; one consistent convention confirmed its identity/frame unchanged.
  Calculator's float marker was cleared to the captured zero baseline, but its
  original geometry was not recoverable and was left unchanged at cleanup.
  One Notepad window absent from the fixture roster was closed in a known test
  process without proving that window's origin; another unclaimed Notepad was
  left untouched. Full initial-desktop restoration is therefore not established.
- Locked build/test for core/protocol/FFI/Windows passed: 2,458 passed, 0 failed,
  1 ignored. Workspace fmt, strict all-target clippy and diff check passed.
  No production or regression-test changes accompanied this evidence update.

### Rejection containment follow-up (2026-10-17)

- The [hydration repair record](../windows-restart-hydration-rejection.md)
  captures a live fixed-size first-tiled anchor triggering convergence followed
  by duplicate admission. A mixed baseline again displaced three marked floats.
  Windows-only observation holds now prevent those fallback writes; six mixed
  protected restarts preserved starting frames and adopted on the next observation.
- The shared-core rejection is unchanged pending a separate KDE-compatible
  reply/projection decision. Historical missing-candidate and cleanup limitations
  above, full D25 acceptance, and post-restart un-stick TBDs are not erased.
