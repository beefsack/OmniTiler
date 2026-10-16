# Windows Win-drag press focus

## Goal and scope

- Deliver handoff item 7 R-DRAG-08 after item 9: eligible Win+left mover
  activates at press through existing exact-lifetime focus authority.
- Preserve stationary real source/moving preview R-DRAG-07, native title-bar
  path, inactive resize focus, all arm/settle gates and unchanged Engine drop.
  No core, settings, chords or underlay-C implementation.

## Acceptance and units

- Bounded implementation/tests, independent live-contract review,
  bounded ordinary-app live verification, native gates, records/publication/CI.
- No-move press focuses with zero layout writes; Esc retains press focus and
  zero writes; failed press refuses arm/no plan; drop preserves verified-focus
  refusal and actual Engine transfer/preview contract without redundant setter.
- Physical Win chord effects/feel remain user-owned; deterministic tests and
  native exact-owner test-needed routes may verify owner effects separately.

## Authorized live scope

- Read live-windows-testing.md before live work. Exact owner plus newly opened
  Notepad/Calculator/Paint only; synthetic input/cursor permitted. Preserve
  every pre-existing HWND/terminal. No forced-owner/emergency/crash/helper-window
  probes, game/elevation/registry/policy work, or Win+G/Win+F11.
- Bind source/artifact SHA/owner/app/display baseline, prove independent
  graceful stop/restore, end owner stopped/hooks released/hidden apps revealed,
  settings restored/owned test apps closed with readbacks.

## Decisions and evidence

- Windows adapter only: press uses existing exact-lifetime focus authority
  after START capture; already-foreground skips prime/setter. A precheck avoids
  priming known suspend/elevation vetoes, existing late fence remains. Failed
  actuation may have attempted prime/setter but clears its arm with zero
  geometry/Engine plan. New press failure cleanup does not invoke unrelated
  preview's visible fallback; normal Cancel cleanup unchanged. Drop authority,
  native title-bar path, inactive resize and parked underlay C unchanged.
- Independent review accepted repairs; final native locked four-package build/
  test, fmt, strict all-target clippy and diff check passed 2026-10-11 using
  Rust1.99.0 MSVC, mise auto-install disabled. No shared source changes.
- Tentative, pending user review: reuse existing focus settle bound up to 500ms
  per press on immediate miss (hook callbacks stay prompt), rather than add a
  second actuation policy. Actual observed press duration 6-10ms, not a guarantee.
  Fresh press observation updates token caches but no Engine/frame effects.
- Test coverage: arm/refusal, queue/settle, scoped cleanup maps, same-output
  fences and real Engine preview/drop agreement. Positive press actuation,
  known-veto and already-foreground native paths need a live desktop. Scoped
  clear regression does not construct a visible native overlay; that case is
  source-reviewed, not proven by the map-level test.

## Live evidence and failures

- Initial launch exposed an existing first-run preset modal that blocks the
  owner pump and graceful stop. No gesture effects ran. Exact owner PID26088
  creation `01dd58da97c7c78b` was revalidated; exact owned dialog IDYES was
  posted, pending stop honored, independent restore passed, newly created
  settings restored to prior absence. Temporary Authentic choice was tentative,
  pending user review; no persistent user preset selected. No kill/crash probe.
  Isolate test settings before launch; modal stop remains a
  discovered product risk outside this piece.
- Prior fixture attempts (Paint path, Notepad PID reuse, helper ordering,
  origin-count and empty eligibility polls) failed before gestures. Two failed
  fixture approaches halted. Read-only diagnosis proved PowerShell unary-comma
  ArrayList double-wrapping discarded all poll HWNDs. One causal flatten repair
  restored the existing oracle; accepted three-Notepad scope avoids unproven
  Calculator Store-host fourth-origin suspicion. No product defect inferred.
  Original failures preserved under temp `windrag-rdrag08-7808/`.
- Accepted PRIMARY journeys ran twice on Windows11 Pro build26300, DISPLAY1
  primary full [0,0,2560,1440], work [0,0,2560,1380], DPI120. Medium session-1
  Explorer-parented exact owners PID22484 creation `01dd58de9eb7b603` and
  PID29268 creation `01dd58dec3bb4ba0`, `--scope-exe notepad.exe` only. Fixture
  exact three new Notepad HWNDs/identity/native mapping stabilized before any
  gesture; independent graceful stop/restore phase-zero passed each run.
- Base `9d12c7f` plus delivery production diff. Live artifact SHA-256
  `E711D3EF277381A2468DD71783346C9DFE77AA2B311806CBAEA55BB544222456`;
  source diff SHA-256
  `F40CF298BE764CF13F95A3F297B31BFAAF747A7D1CCAF719C5229D2685460390`.
  Subsequent scoped-cleanup fixture correction is test-only; production
  live evidence remains applicable. Final native gates passed after correction.
- Evidence: `evidence.jsonl` SHA-256
  `858921AFF252BA923A0EAD13F8A41DCD29A3A905A076DC13775645FEC8392828`;
  `harness.ps1` SHA-256
  `70ECC2528AF6AD962272EEEBCC8449A4F9B02F530AD424D4B570EC17F82AC0B9`.
  Scoped app/owner identities and machine responses in receipts; owner logs
  `session-1/run-01dd58de9eb7b603.log` and `run-01dd58dec3bb4ba0.log`.
- PRIMARY results: inactive B press binds foreground BEFORE motion/Up with all
  frames fixed; no-move -> `gesture-no-change`; Esc -> `gesture-cancelled-esc`
  with B retained foreground/frames fixed/no plan/preview hidden. Move/drop ->
  `drag-drop-applied`, mover and siblings frozen midhold, preview equals final
  mover allocation, B foreground. Actual press prime2/setter accepted at
  7/8/6ms and 10/8/6ms. Drop logs phase/drop focus-ok; sparse drop diagnostic
  has no setter boolean, so redundant-setter skip is source-verified, not a
  fabricated logged count. Synthetic Win async state plus admitted mouse route
  makes no physical chord/Start-mask containment claim.
- Whole runs FAILED despite fixture's erroneous `verdict:pass`: optional
  title-bar stage1 lacked ownerCopy; one causal fixture-field repair enabled
  stage2. Stage2 native drop applied/matched at tick29 but selected current
  slot, failing its topology-change oracle. No full-suite pass/changed topology
  claimed. Primary item7 journeys accepted separately; native title-bar path
  preservation source-reviewed and same-topology native apply observed. New
  topology-changing native title fixture and physical inactive resize remain
  user-owned; no further harness mechanism/probe tried.
- Cleanup receipts each run: exact graceful stop/independent restore, hidden
  owned apps revealed, three owned HWNDs closed, owner/hooks/overlays and
  ledger/requests cleared, inventory19 baseline preserved, terminal intact,
  settings absence and SPI arranging1/pen35 restored. No forced-owner probes.
- Pending physical input/feel/Start-mask, inactive resize focus/feel, actuation
  failure/race native journeys and cross-output. No shared-core change required.
- Pushed `9bb2b6f`. CI
  [38073331721](https://github.com/beefsack/OmniTiler/actions/runs/38073331721)
  completed success: windows, rust, kwin, shell, native and macos. Linux/KDE
  gates are hosted evidence; local native gates passed before publication.
- Exact next implementation action: none for scoped item7 delivery; pending
  user-owned/native edge checks and tentative review are listed above.

## Records reconciliation 2026-10-17

- Delivery already present at `5d953e98` (`9bb2b6f`, gate follow-up
  `f794cf96`); no implementation or regression changes needed. Source review
  confirms press/drop focus, refusal fences, cancel, native title-bar and
  inactive resize behavior preserved after the `tiling_sys` module split.
- Locked four-package build/test, fmt, strict all-target clippy and diff check
  passed: 2,448 passed, 0 failed, 1 ignored; doc-tests had no cases. No new live
  desktop run. The accepted 2026-10-11 primary journeys above remain applicable;
  the optional title-bar whole-run failure and pending checks remain unchanged.
- Corrected the stale backlog implementation-gap claim and linked the delivery
  in the handoff table and Windows spec/matrix cells. No new product decision.
