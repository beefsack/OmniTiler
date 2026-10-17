# Windows fullscreen exit and physical acceptance

- Goal: ensure startup, new-window and managed fullscreen exits resume ordinary
  admission/tiling without writes while fullscreen, including borderless frames.
- Scope: Windows adapter regressions and minimal repair if required; record
  2026-10-12 physical single-output acceptance of directional navigation,
  workspace-retile maximize overlays, born-maximized settings and radius 6.
- Acceptance: cover startup-fullscreen, born-fullscreen, managed return, transient
  metadata veto and borderless exit; preserve D5 fixed-size and intentional-float
  decisions. No shared-core or KDE change is expected.
- Approach: inspect startup/admission, fullscreen observation and owned metadata
  retirement; exercise production routes through offline fixtures. Compare float
  versus retained-maximized focus priority using existing pinned reference
  evidence and already available reference sources only.
- Verification: native locked four-package build/test, all-workspace formatting,
  strict allowlisted Clippy and whitespace checks. Game, DPI, multi-output and
  native fixed-size legs remain pending.
- Bounded work: reference ranking tally; fullscreen lifecycle regression/repair;
  acceptance records and final native gates.
- Outcome: ordinary fullscreen exits already meet D5; corrected the directional
  post-clear fence to include fresh captionless flags and owned metadata,
  matching B9. Windows-only, no shared-core/protocol/KDE/dependency changes.
- Follow-up outcome (User 2026-10-12): automatically retire stale owned fullscreen
  metadata only on fresh proof of exit, retaining the veto on non-covering
  transition frames; retirement/retry/re-fullscreen regressions pass. Physical
  managed-slot and startup-owned exit acceptance is recorded separately from
  the later retirement change. Nonzero restored-chrome proof is delivered;
  originally captionless zero-bit metadata deliberately stays fail-closed
  without a restoration witness, per the user follow-up below.

## Lifecycle evidence

- Startup and born-after-start first exits: `admit_born_fullscreen` installs the
  slotless hold; `track_fullscreen_holds` releases it into current-hint admission;
  `consume_classified_first_exits` removes only committed marks. New startup/born
  fixtures drive production tracking/assembly/Engine commit. Native identity,
  tagging and app enumeration are not exercised by synthetic rows.
- Managed tiled return: retain identical slot before/during/after fullscreen;
  Engine still projects its allocation, while production `writable_subset`
  excludes the retained-only fullscreen token. Native fullscreen detection is
  modeled, not live-tested.
- Borderless: captionless monitor-containment predicate holds fullscreen;
  smaller borderless exit takes ordinary first-exit admission. Exclusive vs
  borderless game presentation is not established by geometry alone.
- Owned metadata: pre/post-clear veto is per invocation; owned exit restores
  only its style mask, verifies frame/style, reasserts prior maximize, then
  removes metadata. Eight scripted production-exit tests pin ordering, unrelated
  style preservation, failures and later explicit retry. B9 retry subsequently
  proves real fresh tiled admission, not just successful setter return.
- Existing fixed D5 controls remain green: current fixed hints automatically
  float, unknown/non-fixed tile, hidden first-exit classification and intentional
  float/sticky identity preserved. Floating workspaces defer ordinary tiling.

## Decisions and remaining evidence

- User 2026-10-12: leaving fullscreen, including borderless, uses ordinary
  admission/tiling, with no target writes while fullscreen; existing D5
  fixed-size/intentional-float exceptions apply.
- User decision 2026-10-12: automatically clear stale owned fullscreen metadata
  on fresh proven exit, including exit by other means; never use non-covering
  transition geometry alone as proof. Reconcile and directional/B9 clear routes
  now share the same metadata-only retirement sequence: originally cleared
  nonzero chrome bits restored, fullscreen false, exact member-tag/held-identity
  and preimage pin, then a second live proof before removal. Unreadable, raced,
  transitional and genuine re-fullscreen frames retain their veto.
- User follow-up 2026-10-12: originally captionless zero-bit preimages have no
  positive chrome restoration witness in the existing metadata and stay
  fail-closed without one. No timer or geometry-only expiry. Win+F11 owned
  toggle exit remains the release path; the user will watch for windows failing
  to tile during dogfood. This selects the current policy, not a remaining
  stronger-proof decision; native edge-case acceptance remains unestablished.
- Physical acceptance and reference tally are linked from the existing
  directional/retile/born-setting/radius archive records and updated Windows
  matrix/spec/backlog cells. Later automatic-retirement hunks have offline
  evidence only, distinct from the user-owned fullscreen-exit acceptance below.
- Verification corrections: managed-overlay fixture originally combined an
  eligible and retained row for one token; corrected to retained-only. Its
  earlier geometry-exclusion assertion then failed because Engine allocations
  include the overlay; production write filtering is the correct invariant and
  now passes with exact slot retention. No production workaround was added.
- Negative ordering probe: clearing owned metadata before restore readback
  caused 5 of 8 exit tests to fail; correct order restored and all pass.
  Directional owned-metadata wiring is inspected plus shared-policy tested,
  without a full scripted native directional dispatcher fixture.

## Final gates - 2026-10-12

- Rust 1.99.0, `x86_64-pc-windows-msvc`; locked build/test of `tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`: pass, 1475 tests,
  0 failed (Windows library 353, tiling integration 115; 14 added regressions).
- All-workspace format check, strict locked allowlisted all-targets Clippy
  with warnings denied and `git diff --check`: pass.
- Linux/CI and native app detection/settlement, exclusive games, fixed-size,
  DPI/multi-output remain pending; superseding follow-up gates are below.

## User-accepted physical fullscreen exit - 2026-10-12

- Scoped single-output Notepad/Paint, physical keyboard/mouse; base `e506350`
  plus directional/retile/B9 and fullscreen-exit regression changes before
  automatic stale-metadata retirement.
- Win+F11 owned fullscreen on tiled Notepad, then exit, returned it to its slot:
  PASS. Stop/relaunch with Notepad still fullscreen, then exit, tiled it: PASS
  (startup adoption). Born-after-start/app-native/borderless/fixed/game/DPI/
  multi-output and later automatic retirement are not established by this run.
- Each Win+F11 also raised the OS Xbox full screen experience prompt. Existing
  parked containment issue confirmed; user wants a fix eventually. No change
  to its parked gaming-coexistence scope.

## Automatic-retirement follow-up evidence - 2026-10-12

- Windows-only source changes: pure eligibility gate, checked live style read,
  shared double-proof retirement through existing native effects, automatic
  verified-member reconcile sweep and directional/B9 pre-clear integration.
  No shared core/protocol/KDE/dependency change.
- Twelve new regressions: restored stale marker permits B9 real fresh admission;
  transition/fullscreen/unreadable/identity/removal-failure controls; pure proof
  matrix; changed preimage/re-fullscreen/tag drift after initial proof; caller
  guards; genuine re-fullscreen after retirement; automatic sweep for normal
  and intentional-float members, with transition/guard/lifetime controls.
  Directional integration uses the same sequence; full native dispatcher timing
  is inspected, not established by the B9/sweep fixtures.
- Red/green: disabling removal made the stale-marker admission test fail with
  `fullscreen-raced`; restoring removal made it pass. Initial strict all-targets
  Clippy found a test-only `manual_contains`; mechanical correction passes.
- Final native gates: Rust 1.99.0 MSVC; locked four-package build/test PASS,
  1487 tests / 0 failed (Windows library 365, tiling integration 115);
  all-workspace fmt, strict locked allowlisted all-targets Clippy and
  `git diff --check` PASS. Linux/CI and retirement physical acceptance pending.
- Next: observe untiled-window cases during dogfood, using Win+F11 owned toggle
  exit as the captionless metadata release path; remaining app-native/born/
  borderless/fixed/game/DPI/multi-output checks and deeper reference tie-order
  research. No captionless automatic-retirement policy decision remains.

## Second agent-live sweep (2026-10-17)

- Paint's enabled native View-menu Full screen action was invoked through UI
  automation, then exited with Escape. No fullscreen shortcut was sent.
  Correlated observations held a born fullscreen overlay without target writes,
  suppressed visuals, then released the hold on exit. Paint's main-window style
  and tiled frame returned exactly; sibling frames were unchanged.
- This is an app-native modal-overlay leg: app action plus main-window changes
  and lifecycle sequence support attribution, but no direct opaque-token-to-
  native-surface mapping was retained. It does not prove that Paint's main
  window itself became fullscreen, nor borderless/exclusive-game behavior.
- Saved settings/SPI unchanged; independent restore/ledger cleanup passed and
  all proven owned disposable windows were closed. Other apps, games, automatic
  retirement edge cases and DPI/multi-output remain pending.
