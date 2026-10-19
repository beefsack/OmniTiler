# Windows single-output dogfood corrections

## Goal and scope

- Restore the actually focused managed maximized/fullscreen member when returning
  to its workspace, without weakening identity or foreground safety fences.
- Draw pager miniatures back-to-front in native stacking order; hidden workspaces
  retain their last observed relative order.
- Keep non-activating geometry reconciliation for other verified managed windows
  under an unmanageable fullscreen foreground. Elevated games remain excluded
  and untouched; automatic focus and overlays remain blocked.
- Soften pager icons and improve workspace-number contrast without moving labels
  or changing pager geometry.
- Replace the accepted offset number shadow with a centered black halo; use the
  documented Windows Personalization accent for border theme mode and pager fill.
- Keep the taskbar pager visible on desktop-background clicks and while Start is
  open, without weakening fullscreen/game or unreadable-foreground safety.
- No shared-core changes, game admission, game geometry/style writes, or changes
  to the workspace SW_HIDE mechanism.
- Retire positively verified app-hidden members through the existing close path;
  reshow admits fresh. Unknown reads and tiler-owned hides continue to hold.

## Acceptance and approach

1. Record departure focus from the verified live foreground member, including
   retained maximized/fullscreen rows, independently of stale/missing snap origin.
   Reject nonmembers, wrong-workspace members and unverified identities. Preserve
   return-time fullscreen/elevation fences and existing one-shot activation.
2. Reuse the existing EnumWindows observation order, paint topmost last, retain
   hidden workspace order, and prune departed identities. No new native queries
   in pager painting or icon lookup.
3. Under unmanageable fullscreen, reconcile geometry only for existing verified
   members: no admission, native unmaximize, marker/visibility writes, automatic
   focus or overlay refresh. Per-write fresh foreground and identity checks remain.
   Preserve the existing managed/holdable fullscreen behavior. Invalid/unreadable
   foreground facts remain fully blocked.
4. Blend icons at 128/255 opacity over their existing background after source-alpha
   composition. Numbers use 10pt Segoe UI Bold (previously 9pt regular), white over
   a centered black halo of one DPI-scaled logical pixel radius (1px at 96 DPI,
   2px at 144/192 DPI). Eight symmetric surrounding black text copies precede
   the unchanged centered white glyph; cells and tray gap stay unchanged.
5. Add offline regressions, run native locked package gates, workspace format and
   diff checks. Halo appearance and accent parity are physically accepted below;
   broader notification, DPI and output variants remain separate.
6. Use Windows-target-only `windows` with minimal WinRT features for
   `UISettings.GetColorValue(UIColorType::Accent)`. Border theme-off keeps its
   configured color; pager always follows the available accent independently,
   with existing blue fallback. Shared availability retains the positive-alpha
   gate. Refresh on existing broadcasts, not polling; do not alter saved settings.
7. Diagnose the desktop/Start disappearance before changing suppression gates.
   Distinguish fullscreen covering, integrity/read failures and native burial;
   any exception must identify the specific surface and remain pager-local unless
   a broader behavior change is separately decided.
8. Normal-level pager diagnostics record an initial baseline and changes to gate
   or safe foreground facts, never identical repeated snapshots. Separate
   process-open, integrity-read and integrity-category outcomes; unknown/skipped
   reads must not imply elevation. No behavior change or application-content logs.
9. Resolve accent once per observed broadcast version on the owner thread, keeping
   its WinRT apartment alive for that owner scope. Both surfaces consume the same
   plain snapshot; no renderer COM objects or process-static activation factory.
   Preserve fallbacks, idle notification delivery and synchronous trace stages.
10. App-hide close requires live invisible uncloaked lifetime-fenced membership,
    no tiler hide claim or ledger entry, and no native minimize. Reflow survivors
    and apply existing close/MRU rules; forget the slot before fresh reshow.
    Normal-level domain-deferral diagnostics report actual reasons only on
    transitions, with recovery separately observable. No shared-core change.

## Diagnosis and decisions

- Fullscreen: the incident log records repeated fullscreen-foreground suspension,
  including an approximately 90-second interval without reconciliation. The user
  confirmed the game ran elevated while the tiler ran at medium integrity.
  `holdable_key` rejects inaccessible/non-medium processes; `suspend_read` cannot
  exempt that foreground, and the run loop returns before normal reconciliation.
  The log records no individual failed admission gate; elevation attribution is
  user-confirmed, not independently instrumented evidence.
- Existing first-seen fullscreen holds already provide workspace membership for
  eligible windows. Extending that mechanism to elevated games is not requested.
- User-approved 2026-10-19: allow only non-activating geometry reconciliation of
  other verified managed windows while keeping the unmanageable foreground
  excluded and all automatic focus/overlays suppressed. This narrows the previous
  global suspension without admission or writes to the game. Full ordinary
  behavior could disturb the game through activation or overlays; performance and
  game-safety acceptance remain separate, with no imperceptibility claim.
- Managed/holdable fullscreen keeps its existing managed-overlay behavior; only
  blocking unmanageable fullscreen takes the limited lane. Existing members can
  reflow after a close; fresh admission, hidden-domain close drains, stale-marker
  retirement and capacity-float adoption defer until ordinary reconciliation
  resumes. No new timer, retry, dependency or shared-core change.
- Focus: before correction, departure recording depended on a matching snap
  origin; native focus or stale origin could leave workspace focus memory stale.
  Existing return targeting
  already accepts retained maximized/fullscreen members. Incident logs include
  both successful returns and protected focus refusals; no unique failing
  maximized-window switch can be identified from the available log.
- Pager: member iteration is HWND-key order, and the live rectangle map discards
  the existing top-to-bottom enumeration order. Use that observation order rather
  than a new native z-order query.
- Accent diagnosis, 2026-10-19: the pager active fill was hardcoded `#2a82da`;
  unlike the border it had no accent resolver. The border defaults to theme-on,
  but persisted theme-off explicitly selects its configured color. Its previous
  documented source, DWM colorization, describes glass composition, not necessarily
  the Personalization accent. The reported gray alone does not identify a query
  failure or establish the OS-returned value; no native accent probe was performed.
  User-selected replacement source, 2026-10-19:
  [UISettings.GetColorValue](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uisettings.getcolorvalue)
  with [UIColorType.Accent](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uicolortype).
  The Windows-target-only `windows` dependency replaces the DWM analogue; no
  undocumented registry reads, new setting or saved-settings mutation. The pager
  uses accent independently of border theme on/off, falling back to `#2a82da`;
  border theme mode retains its configured-color fallback. Zero-alpha results
  are unavailable under the existing shared positive-alpha guard.
- Pager disappearance follow-up, 2026-10-19: the supplied incident log contains
  one `pager-suppress` hide, `elevated-foreground`, followed about 1.7 seconds
  later by `show`; it contains no fullscreen-suspension event. Suppression checks
  fullscreen before integrity, so this recorded hide did not take the fullscreen
  branch. `foreground_elevated` also returns true on process-open or integrity-read
  failure, not just non-medium integrity. The log identifies neither the
  foreground surface nor which integrity branch fired; it cannot attribute this
  interval to desktop or Start, or rule out separate native z-order burial.
- A proposed class/host shell exception was rejected and removed: it cleared the
  global fullscreen veto rather than the recorded independent integrity gate,
  resumed more than pager rendering, and lacked production-wiring regression
  evidence. Existing suppression policy remains unchanged. The next discriminator
  needs foreground-surface identity, bounded integrity/read outcome and all pager
  gate facts associated with the desktop/Start journeys. Do not blanket-exclude
  Explorer or ApplicationFrameWindow, or relax game/unreadable-fact suppression.

## Verification and remaining acceptance

- User physical acceptance 2026-10-19 PASSED: maximized windows regain focus on
  workspace return, and pager miniature occlusion respects native z-order.
- Further user physical acceptance 2026-10-19 PASSED: the unmanageable-fullscreen
  tiling correction is effective and the softened icons/bold-number styling is a
  substantial visual improvement. This accepts those fixes, not every game,
  selection route, DPI or multi-output variant, nor measured zero performance
  cost.
- Further user physical acceptance 2026-10-19 PASSED: centered black number halo
  and shared system accent for border theme mode and active pager highlight;
  the user confirmed the correct accent color. This does not establish every DPI
  variant, idle broadcast route or fallback/fault path.
- Accent reliability reopened 2026-10-19: the user subsequently reports black
  instead of the selected accent on roughly half of starts, independently for
  border and pager. Earlier correct-color acceptance is a single observed result,
  not reliable startup acceptance; halo acceptance is unaffected.
- Corrected-build physical acceptance 2026-10-19 PASSED: 10/10 user-run starts
  succeeded with the correct accent on both active border and pager. This closes
  the reported startup-reliability acceptance gap, not idle broadcast, fallback,
  hidden/reshown, theme-off or wider output/DPI variants.
- Focus implemented in the Windows departure helper and its four selection
  callers. It no longer requires snap-origin authority; fresh identity/token
  or retained max/fullscreen identity/token still must independently verify.
  Return activation, eligibility and geometry paths are unchanged.
- Pager implemented with one retained top-to-bottom identity vector sourced
  from the existing observation. Fresh verified candidates are included before
  assignment so first admission paints correctly without a second observation;
  frame building still filters actual membership. Current visible order wins,
  absent members retain their relative order, departed identities prune and
  never-observed members use deterministic key order. Painting reverses it.
- Earlier offline regressions cover native-focused maximized/fullscreen departure,
  stale/missing origin, identity/token refusal and geometry exemption; pager
  native versus HWND order, mixed overlay order, hidden retention, close/reuse
  and same-turn first admission. Windows library: 1,202 passed, zero failed,
  one pre-existing ignored; targeted pager: 62 passed. Full Windows-package
  tests (including integration/doc tests): 1,615 passed, zero failed, one ignored.
- Locked five-package build and strict all-target clippy passed, as did workspace
  format, unsafe guardrail (32 tests), and diff checks. Separate locked tests of
  core/protocol/KWin-FFI/Windows passed: 2,852 tests, zero failed, one ignored.
  The combined five-package test command failed in eight unchanged macOS tests
  which spawn Unix `sleep`/`true` helpers unavailable on Windows; the macOS
  library otherwise passed 884 tests. No dependency or macOS test change was
  made. Cross-platform CI remains pending.
- Independent source review accepted identity/focus fences, retained overlay
  order, native painter sequence and absence of extra window queries. The
  first-admission correction was additionally inspected and passed the complete
  Windows tests after its final change. No live tests were run.
- First-admission ordering initially filtered fresh nonmembers before assignment;
  an offline observe/assign/paint discriminator reproduced it. Retaining fresh
  verified candidates alongside members fixes that gap without extra queries.
  An initial hidden-miniature fixture lacked admission snapshots; staging the
  normal token/slot snapshots restored that fixture without production changes.
- Limited lane implemented in the adapter using shared domain geometry planning,
  with foreground-excluding writables and unchanged `SWP_NOACTIVATE|SWP_NOZORDER`.
  Each suspend read uses one captured foreground HWND for native classification,
  managed verification and exclusion; each write still takes a fresh read.
  `suspend`/`suspend-mode`/`resume` identify `limited` versus `full`; normal geometry
  diagnostics retain actual plan/write/readback outcomes, not implied success.
- An initial full-reconcile implementation was rejected in source review because
  admission could natively unmaximize a window. The limited lane now skips that
  preamble/postamble, and the corrected source review accepted the effect boundary.
  Five targeted tests cover refusal policy, foreground exclusion, actual Engine
  planning and scoped writables, unchanged membership/admission attempts, and
  blocked focus/native transfers through the run-loop lane. Native setters remain
  stubbed in these tests; no game-safety or native-actuation proof is inferred.
- Pager styling composes source alpha through existing GDI rendering, then fades
  the opaque pixels toward the same miniature gray. Transparent regions stay
  unchanged and partial-alpha edges retain their gradient. Text shadow adds no
  GDI resource; font ownership/cleanup remains with the pager window. Two new
  offline tests cover blending and font/shadow sizes.
- Final offline gates after both changes: locked five-package build and strict
  all-target clippy passed; locked core/protocol/KWin-FFI/Windows tests passed
  2,859/0/1 (Windows 1,622/0/1, library 1,209/0/1). Targeted fullscreen 5/0,
  pager 64/0; workspace format, unsafe guardrail 32/0 and diff checks passed.
  The known eight unchanged macOS Unix-helper failures were not rerun; no macOS
  source changed. Cross-platform CI remains pending. No live tests were run here.
- Centered-halo follow-up delivered offline: replaces the single offset shadow
  with eight symmetric black text copies at the same DPI-scaled radius, then the
  original white centered label. No additional GDI resources, font-size, icon,
  geometry, cell or tray-gap changes. Regression covers symmetric nonzero offsets,
  radius scaling and unchanged font constants.
- Post-halo gates: locked five-package build/strict all-target clippy and locked
  four-package tests passed, 2,859/0/1 (Windows 1,622/0/1); targeted painter 6/0,
  pager 64/0; workspace format and diff checks passed. Known macOS Unix-helper
  failures were not rerun; no live tests were run for this follow-up.
- Accent delivered offline: `windows` 0.61.3, default features disabled, only
  `UI_ViewManagement` and `Win32_System_WinRT` requested. Safe UISettings calls
  query on the consuming thread. The minimal FFI guard pairs successful
  `RoInitialize` calls (including already-initialized success) with exactly one
  same-thread `RoUninitialize`; failed/different-apartment initialization queries
  nothing, leaves existing apartments alone and returns unavailable.
- Border and pager each cache the shared resolver's result against a broadcast
  version. `WM_SETTINGCHANGE` and `WM_DWMCOLORIZATIONCOLORCHANGED` advance it;
  consumers observe, never consume it. A change during a query remains pending,
  hidden surfaces refresh on reshow, and failures use fallback until the next
  notification. The renderer signals the existing owner wake for accent-only
  events; the owner promotes that wake before its ack-only skip and reobserves
  through ordinary safety fences. No fake ack, stale-frame replay, added timer
  or native steady-state query. Pager fill/source changes survive creation backoff.
- Normal border/pager logs identify accent versus fallback, including same-RGB
  source changes. Source review caught and corrected initialization balancing,
  backoff repaint loss and idle notification delivery before acceptance. A
  duplicate global-version test was removed to prevent cross-test notification
  races; independent source verification accepted the final boundaries.
- Post-accent gates: locked five-package build/strict all-target clippy, locked
  four-package tests 2,872/0/1 (Windows 1,635/0/1, library 1,222/0/1), targeted
  accent 16/0 and pager 69/0, unsafe guardrail 32/0, workspace format and diff
  checks passed. New dependency declarations offer MIT/Apache-2.0 terms within
  policy; license-tool and cross-platform CI checks remain pending. The current
  notices scope excludes Windows-only roots, so no notice regeneration applies.
  No native WinRT query/live test was run; known macOS Unix-helper failures were
  not rerun. No shared-core or development-environment dependency changes.
- Remaining user-run checks, following the live Windows protocol: separately
  approved DPI scales must preserve white glyph centers, cell/strip geometry and
   tray gap. Corrected startup accent reliability passed 10/10 user-run starts;
   change the accent while windows are
  otherwise idle. Both surfaces must refresh without geometry/focus
  changes, including pager-disabled border operation and hidden/reshown carriers.
  With border theme off, border stays configured while pager still follows accent.
  Fallback/init-error behavior has offline coverage, not physical fault evidence.
  Wider fullscreen/game, route and DPI/output variants remain unproven beyond
  the accepted journeys; no quantified performance claim is made.
- Desktop/Start correction BLOCKED on discriminating evidence and a safe,
  pager-local remedy. The initial investigation retained no runtime change;
  diagnostic-only follow-up is delivered below. For a diagnostic capture,
  click the desktop background, open/dismiss
  Start, and return to an ordinary app; correlate each transition with pager
  gates and foreground identity/read results. Pager must stay visible for the
  shell journeys, while genuine fullscreen/game and unreadable-fact suppression
  remain intact. Physical outcomes beyond the reported disappearance are TBD.
  Final post-removal workspace format and diff checks passed; candidate test
  passes are not evidence for a delivered fix. The active record remains open.
- Pager diagnostics delivered offline: normal `pager-gate` records the first
  baseline and full fact transitions, including allowed-to-allowed class changes,
  sub-outcome changes under the same suppression reason and recovery. Identical
  repeats stay silent. Existing `pager-suppress` reason/show transitions and
  scheduler state stay separate and unchanged; `allowed` means gate pass, not
  proof that native rendering succeeded.
- Each gate event includes enabled/fullscreen/elevation/unknown booleans,
  covering reason and desktop/visible/readable facts, class names and bounded
  surface labels for the covering and integrity captures. Generic XAML/task-view
  classes are explicitly heuristic, not verified Start identity or an exemption.
  `fg_handles_match` compares handle values from sequential captures, not atomic
  snapshot or lifetime proof. No titles, executable names/paths, process IDs,
  raw handles, error payloads or application content are logged.
- `elev_open`, `elev_integrity_read`, `elev_integrity` and `elev_detail` distinguish
  failed open, failed read, successful level category and gate verdict. Skipped
  or failed reads retain unknown category. Categories distinguish medium-plus
  from high without altering the existing medium-range predicate; fullscreen,
  admission, focus, clicks and fail-closed rules are unchanged. Existing native
  foreground/process reads are reused; at most two diagnostic class reads per
  pager evaluation are added, with no new timer or polling loop.
- Diagnostic verification: 20-row original-bool/category table; real log-file
  tests cover baseline, repeat silence, allowed surface change, same-reason
  failure-detail change and recovery. Scheduler-state and privacy/shape checks
  pass. Independent source review accepted the behavior/privacy boundary. A
  parallel log-fixture collision was repaired with a per-process atomic suffix;
  three subsequent parallel pager-host runs passed 21/21 each.
- Final diagnostic gates: locked five-package build, full locked Windows-package
  tests (library 1,233 passed, zero failed, one pre-existing ignored; all
  integration/doc tests passed), strict all-target Windows clippy, workspace
  format and diff checks passed. A separate final pager-host run passed 21/21.
  Known eight unchanged macOS Unix-helper failures were not rerun. No live
  diagnostic capture or corrected-visibility acceptance is claimed.
- User-run discriminator, following the live Windows guide: with pager enabled
  and an ordinary scoped app focused, note the initial `pager-gate`; click the
  desktop background, return to the app, open/dismiss Start, then focus a second
  ordinary app as control. Hold each state briefly and repeat once. Compare
  `pager-gate` facts with `pager-suppress` hide/show and any `pager-refused` or
  `pager-taskbar` lines. A disappearance with gates still allowed is not evidence
  of a fullscreen/integrity veto. Stop/restore through the existing dev route;
  games, protected/elevated windows and secure-desktop probes are not needed.

## Intermittent black accent investigation

- Targeted incident evidence: one startup `pager-accent` reports black with
  `outcome:accent`, while 25 border events report the expected nonblack color
  with `source:accent`, beginning about 283 ms after startup. No pager fallback
  or later changed pager-accent event appears. These are reported resolver/
  presentation outcomes, not independent measurements of the underlying API.
- With the current source, that pager outcome requires a present, positive-alpha
  result. API/initialization failures and zero alpha instead take the existing
  blue fallback. Channel conversion passes named ARGB bytes directly; an empty
  cache does not supply black. The two surfaces query separately on their own
  threads and cache their results until a notification-version change, so a bad
  successful read can persist independently on either surface.
- The cause of the successful black result remains unproven. Supplied logs omit
  individual query attempts, raw alpha, initialization/API outcomes and query
  versions; unchanged re-queries are silent. They cannot distinguish a transient
  startup value from persistent thread-specific behavior or later replacement.
  No native query was run during investigation; no runtime fix is claimed.
- Additional incident evidence contains both inverse cases: correct border with
  black pager, then black border with correct pager. Both label black as accent,
  not fallback. Two startup-exit logs end at an allowed pager gate, before first
  pager accent/window output, without panic, callback-return or run-end records.
  This narrows the last observed phase but does not prove a native fault, panic,
  external termination or an ordinary unlogged exit. The dev route's clean-ledger
  message does not establish a successful owner exit.
- All four follow-up runs lack the newly delivered query/stage fields; artifact
  provenance and successful-query paths identify pre-diagnostic payloads. They
  cannot establish which WinRT stage or apartment outcome produced the bad color
  or termination. No COM mode/lifetime/factory defect is established by source
  inspection; startup readiness and thread-specific behavior remain hypotheses.
- The apparent periodic output is pre-existing trace summaries at about two
  seconds, riding the pager's documented two-second topmost recovery maintenance
  (complete native event coverage is not proven). Trace summaries do not arm a
  timer themselves. The supplied runs contain one pager-accent transition each,
  not a repeated accent or pager-gate diagnostic loop. No timer policy changed.

### Diagnostic delivery and discriminator

- Normal `accent-query` now emits once per actual startup or notification-version
  query, even if RGB/source is unchanged. Cache hits emit nothing. Fields include
  `surface`, closed querying `role`, `trigger`, `version`, exact raw `init_hr` and
  init category (`ok`, `already`, `changed-mode`, `failed`), activation/query result
  codes, raw `#aarrggbb`, availability and resolved outcome/color/fallback reason.
  Skipped stages and absent colors are null, never fabricated zero results.
  Activation/query success codes are the safe projection's success-to-zero value;
  errors retain their numeric HRESULT. Raw init preserves already-initialized
  success, which the previous safe projection collapsed into ordinary success.
- Outcome `invalid` means returned zero alpha under the unchanged availability
  rule, with the actual fallback RGB recorded. Positive-alpha black remains
  `accent`; theme-off reports configured fallback separately from raw validity.
  There is no black blacklist, retry, new query, polling, changed init mode,
  apartment lifetime, cache policy or fallback behavior.
- Renderer completions use a separate owner-drained FIFO rather than latest-wins
  acknowledgements, preserving multiple completed query reports before one poll.
  The owner drains it on existing polls and after normal renderer stop; the
  same-thread fallback takes pending metadata before destruction. There is no
  cap/drop policy or backpressure. Hard process loss can still lose renderer
  metadata before the owner writes it; no renderer file logging was added.
- Trace-only owner/pager `accent-query-stage` marks `start`, then entry into
  `init`, `activate`, `query`, `release`, and `done` after teardown. Init failure
  skips stages not reached. The release marker precedes UISettings release and
  apartment cleanup in their existing order. Actual pager creation attempts have
  trace-only `pager-create` start/done markers. These never run on cache hits or
  fabricate an attempt when no window is being created. Renderer pre-call stage
  markers are not provided; only its completed query record crosses the FIFO.
- A scoped normal panic hook writes only role, source basename/line/column and
  payload type, then delegates to the previous hook. It restores on ordinary
  scope exit, never calls hook replacement while unwinding, and logs no panic
  payload, path, native identity or application content. `owner-exit` records
  callback-returned `ok`/`err` before the existing unchanged cleanup/result path.
  Native faults, aborts, external kills and failed log writes can bypass these
  records; absence alone does not determine the termination mechanism.
- Independent source verification accepted query cadence/value preservation,
  privacy, FIFO overwrite protection, teardown drain and panic-hook delegation.
  Tests cover unchanged/repeated cache hits, same-RGB notification queries,
  init/error/skipped/zero-alpha/black outcomes, multiple reports before one poll,
  normal-stop drain, hook restoration and unwinding without double panic.
  Final offline gates: locked five-package build/strict all-target clippy;
  locked four-package tests 2,902 passed, zero failed, one ignored (Windows
  1,665/0/1, library 1,252/0/1); accent 25/0, panic 6/0, render-frame 16/0,
  pager 81/0; unsafe guardrail 32/0, workspace format and diff checks passed.
  Windows-only stage helper cfg boundaries were inspected; cross-platform CI
  remains pending. Known macOS Unix-helper failures were not rerun. No live
  WinRT query, forced termination or settings mutation was performed here.
- User-run steps under the live Windows guide:
  1. Keep the existing ordinary-app scope, border theme enabled and pager enabled.
     Run `mise exec -- just dev trace` with that same scope. Confirm the new run
     contains `accent-query` with init/result/ARGB fields, not only old accent
     source lines; retain that run's provenance. Observe both colors or an exit.
  2. Stop using `mise exec -- just stop` from the existing out-of-hook route and
     repeat until six starts are captured, keeping the sixth running for step 3.
     Retain each log. Do not force a crash or broaden
     the tested windows. A startup with no pager must retain its final stage,
     panic and owner-exit records, if any.
  3. On the final running owner, manually choose another accent in Windows
     Settings, observe the result, then restore the original accent before stop.
     Compare each surface's startup record and subsequent `trigger:broadcast`
     records by role/version, init category/codes, raw alpha/RGB and resolution.
     Stable idle cache hits must add no `accent-query` lines.
  4. For an exit, compare the final owner/pager stage with `pager-create`, `panic`,
     `owner-exit` and `run-end`. A last `release` without `done` narrows cleanup;
     a `pager-create:start` without its completion narrows creation. Neither
      missing completion nor a clean ledger alone proves the native cause.

### Native activation fault and lifetime correction

- New diagnostic evidence supersedes the earlier pre-diagnostic limitation:
  repeated startup runs end synchronously at owner/pager `activate`, before
  `query`, pager creation or normal completion. Control flow establishes usable
  initialization, but the raw init HRESULT is unavailable because the completion
  record was never written. Existing Windows Application Error/WER records
  matched to the executable/start provenance establish access violations
  (`c0000005`), not clean exits or Rust panics. The matching executable's MSVC
  disassembly localizes the fault to the activation-factory vtable method-slot
  read in `IGenericFactory::ActivateInstance<UISettings>`, before the native
  activation method can run. No crash was induced for this investigation.
- Exact dependency source: `windows` 0.61.3 implements `UISettings::new` through
  a process-static `FactoryCache`; `windows-core` 0.61.2 caches agile factories
  indefinitely by retaining their raw pointer. Its agility check permits
  cross-apartment calls; it does not establish safety after apartment teardown.
  The previous resolver initialized/uninitialized an STA around each query on
  both owner and renderer. Startup posts the border before pager intake, so
  renderer-first activation and teardown are possible; the user observed a
  correct border before these exits. A stale cached factory is strongly
  consistent with the invalid-vtable dispatch, but the faulting pointer's
  cached-versus-fresh origin was not captured. Do not claim a proven dependency
  defect or that changing apartment mode alone would cure it.
- Black-color evidence remains narrower: older runs reported positive-alpha
  black as accent on either surface independently, not fallback. Separate
  successful queries and caches allowed disagreement to persist. Those runs
  lack raw query diagnostics, so they do not establish why the API result was
  black or prove that it shares the activation fault's mechanism. No black
  blacklist, arbitrary delay or confirmation retry is introduced.
- Overlap incident: only the initial ordinary reconciliation completed. A
  first-seen maximized member was admitted slotless and its native restore was
  dispatched; it remained retained for that observation, with normal geometry
  convergence intended on the next tick. The other member received one tiling
  write. Readback already placed both in the same slot although the plan assigned
  opposite slots. The process then suffered the same activation fault before a
  second reconciliation. This identifies an interrupted startup/admission
  transition, not the limited unmanageable-fullscreen lane: pager gates were
  allowed, suspension was false, and no limited-lane event occurred. Whether the
  next healthy tick corrects this exact native restore journey remains user-run
  acceptance; no separate geometry fix is inferred from the crash log.
- Delivered correction: one lazy, thread-confined owner source holds its STA
  apartment for the owner scope. Each actual versioned resolve uses documented
  `RoActivateInstance` and an interface cast, bypassing the process-static
  `UISettings::new` factory cache. All local COM references drop before the
  scope's same-thread cleanup. Both `S_OK` and `S_FALSE` own exactly one cleanup;
  failed initialization owns none and is cached without retry until shutdown.
  RAII covers errors/unwinding; explicit shutdown is idempotent and clears init
  state before any later scope. No dependency, shared-core, timer, polling or
  saved-settings change.
- One owner `AccentCache` records the pre-query version. Border frames carry
  only the plain accent snapshot; same-thread border and pager use that same
  cache. Existing broadcasts and renderer-to-owner wake/ack-only promotion stay
  intact. Mid-query notifications remain pending, hidden surfaces refresh on
  reshow, and pager creation backoff cannot consume a needed repaint. Unavailable
  or zero-alpha accent keeps configured border color and existing pager blue;
  border theme-off still leaves pager accent enabled. Opaque black remains valid.
- Diagnostics now emit one `accent-query` with `surface:shared`, `role:owner`
  per actual source resolve, never per consumer/cache hit. `init_hr` is the
  cached scope-init result; activation HRESULT includes activation/interface-cast
  failures. `resolved` uses pager policy, not a custom border fallback or proof
  of presentation. Appearance/ack logs retain actual consumer color/source.
  Trace stages write synchronously before calls: `init` only on actual init,
  no per-query apartment-release marker, `done` after local references release.
  Panic, owner-exit, pager-create and pager-gate diagnostics remain intact.
- Offline regressions cover real source-lifecycle policy through the version
  cache with injected native steps, success/already-initialized balance, failed
  init caching, unwind/drop, shutdown/reuse, one read for both consumption orders,
  an in-query version bump, synchronous stage delivery, missing/zero-alpha/black
  snapshots, theme-off fallback and exact posted renderer snapshot semantics.
  Review corrected initialization balancing, buffered-stage loss, a fake-renderer
  fallback bypass and shutdown/reuse before final verification.
- Final gates: locked five-package build and strict all-target clippy passed;
  locked core/protocol/KWin-FFI/Windows tests passed 2,924/0/1 (Windows full
  1,687/0/1; library 1,274/0/1). Accent filter passed 49 tests across targets,
  render-frame 16/0, unsafe guardrail 32/0; workspace format and diff checks
  passed. Known eight unchanged macOS Unix-helper failures were not rerun.
  Cross-platform CI remained pending; subsequent user-run startup reliability
  passed 10/10 as recorded above. No native WinRT
  query, window-control test or settings change was performed here.
- Remaining user-run discriminator under the live Windows guide: retain the
  existing ordinary-app scope and out-of-hook stop/restore route. Startup accent
  reliability is accepted above; the initially maximized-member journey still
  needs next-tick separation verification. Change the accent once while otherwise idle, observe
  both surfaces, then restore the original accent before stop. Compare shared
  startup/broadcast records and actual appearance logs; steady cache hits must
  stay silent. Pager-disabled, hidden/reshown and theme-off variants remain
  separately bounded checks. Desktop/Start visibility is still unresolved; this
  active record remains open rather than treating the crash fix as its remedy.

## Close-to-hide allocation investigation

- Reported journey: three managed windows in `H[W1 V[W2 W3]]`; closing the
  top-right application leaves the bottom-right survivor in its old slot rather
  than filling the right column. Ordinary removal/refill remains the CLOSE oracle.
- Targeted incident evidence: ticks 1-7 plan three members and all readbacks
  match. At tick 8 the top-right member becomes absent/not-observed, with a
  successful lifetime-fenced native probe reporting invisible, uncloaked and no
  workspace-hide claim. Cleanup already ran before this probe, so its member and
  raw-inventory presence survived. This is a live hidden window, not a verified
  destroyed-window close. Native iconic state and the application's tray policy
  are not established: the visibility gate precedes the iconic query.
- Ticks 8-12 observe only two eligible windows but produce no plan, minimum-hint
  summary, tick result or readback. Pager gates remain allowed and unsuspended;
  the owner later exits normally. There is no failed survivor geometry attempt
  to attribute to minimums, readback fighting or the limited-fullscreen lane.
- Mechanism: `observe.rs` returns Unknown before identity resolution for invisible
  windows; `assignments.rs::workspace_close_cleanup` preserves enumerated members;
  `rows.rs` requires non-minimize-omitted members to have a view;
  `workspace_owner/rows.rs::domain_rows` returns None for the missing view;
  `reconcile.rs` then silently defers that domain before Engine dispatch. These
  retention/assembly rules predate the recent dogfood changes and remain unchanged.
- Offline confirmation: the existing
  `transient_and_unreadable_evidence_never_closes_and_defers` test passes and pins
  inventory retention, no close obligation and deferred assembly. Three adjacent
  minimize/hidden-close tests also pass. This validates the safety mechanism, not
  a native hide-on-close fix or the application's exact close implementation.
- BLOCKED on a product distinction: verified application-hidden windows are not
  defined as destroyed closes or native minimizes. Options are retaining the
  current hold/gap, releasing allocation with D23-style best-effort rejoin on
  reshow, or retiring layout state and fresh-admitting on reshow. Recommend a
  narrowly verified external-hide release with D23-style rejoin; unknown reads
  and tiler-owned hide claims must still hold. This is a proposal, not a decision;
  no grace timer, broad omission rule, runtime change or shared-core change was
  made. Exact return/focus policy and reference outcomes remain TBD in the
  discriminating reference row.
- Next decision: select external-hide allocation/return semantics before an
  adapter implementation. Any subsequent user-owned check should compare the
  same nested layout under actual destroy, native minimize, external hide and
  reshow; only the existing ordinary-app scope is needed. No live test was run.
  Final offline checks: four targeted existing D23 adapter tests, workspace
  format and diff checks passed. No source changed; broad build/clippy/package
  gates were not rerun for this documentation-only investigation.
  Known eight unchanged macOS Unix-helper failures remain unrelated and were not
   rerun. The desktop/Start visibility item also remains open.

## External-hide close implementation and safety blocker

- User-selected 2026-10-19: verified external hide retires allocation like close,
  not D23 best-effort rejoin. Reshow is fresh admission without a remembered slot.
  The earlier proposal above is superseded by this decision; unknown facts and
  tiler hide claims or ledger entries still hold. Decision and Tier 1 rationale
  are recorded in `decisions.md`; requirement is `REQ-CLOSE-06`.
- Adapter candidate uses `pending_closed`/`closed_windows`, dropping native member
  state only after alive/invisible/uncloaked/tag/PID checks. Domain assembly now
  emits normal transition-only `reconcile-deferred` reasons and
  `reconcile-recovered`. Candidate regression geometry proves W3 fills the right
  column, close forgets its slot, and existing focus is preserved.
- BLOCKED on a durable-claim invariant. A single-window hide can commit its
  ledger entry and then fail; if the rescue ledger read also fails, no in-memory
  claim is registered. Initial candidate wrongly classified this as external.
  The attempted repair adds an uncertainty hold but clears it on a visible
  observation while the durable entry remains. A later app hide then wrongly
  closes that member again. The repair also gates its marker on the uncertain
  error prefix, missing other post-commit failures with unreadable rescue state.
  Passing tests currently encode the premature clear, so do not establish safety.
- Next implementation must distinguish unknown rescue reads from proven absence,
  retain uncertain ownership across visibility, and reconcile it only against
  authoritative durable-claim evidence. Extend the regression through visible
  recovery followed by another hide with the ledger still present, and cover a
  post-commit non-uncertain error plus unreadable rescue. Preserve destroyed-close
  cleanup and clear proven-absent uncertainty without a new timer or retry loop.
- Current evidence: targeted D23 adapter 34/34; Windows library 1,281 passed,
  zero failed, one pre-existing ignored; strict Windows all-target clippy,
  workspace format and diff checks passed. Independent review reproduced the
  remaining invariant violation by source trace despite all seven external-hide
  tests passing. No shared-core change or live test. Full offline acceptance is
  not claimed. The same-token pre-drain fixture tests shared stale-close handling,
  not the production fresh-token reshow route; that coverage remains incomplete.

## Desktop/Start persistent disappearance follow-up

- The supplied diagnostic run identifies one suppression at about 2.4 seconds:
  foreground class `Windows.UI.Core.CoreWindow`, low integrity, successful process
  open and integrity read, `non-medium`, no fullscreen suspension and no unknown
  facts. Its shell-surface label is heuristic, not proof of Start identity.
  The branch is the existing integrity gate, not a failed process/read or a
  fullscreen veto. No exception was introduced.
- At about 4.4 seconds the gate emits `show`/`allowed` on an ordinary medium
  foreground. Later foreground transitions remain allowed until clean stop at
  about 11 seconds. This proves policy recovery, not actual native visibility:
  the user's reported persistent disappearance is not contradicted by `show`.
  There is no second suppression, `pager-refused`, taskbar-loss/change event, or
  positively identified desktop-background foreground in this run.
- BLOCKED: current events cannot distinguish native burial/visibility loss from
  other presentation failures after the gate recovered. The existing native
  show/topmost recovery path can be reached with allowed gates, but these logs
  do not prove its physical result. No blanket Explorer/CoreWindow/host exemption,
  game-safety relaxation, or runtime remedy is supported by this evidence.
- Targeted pager-host tests passed 21/21; no source change for this investigation.
  Next discriminator is a user-correlated invisible interval (desktop, ordinary
  app, Start open/dismiss, ordinary app control), recording policy and actual
  presentation separately. If gates remain allowed, bounded native visibility/
  topmost-loss diagnostics need a separate decision before implementation.
- User-owned acceptance after both corrections: compare actual destroy, native
  minimize, app hide and reshow in the same nested layout; verify refill, MRU and
  fresh reshow. For pager visibility, hold each desktop/Start/control state and
  record whether it is actually visible through focus/mouse recovery. Follow the
  live Windows protocol; no new live testing is claimed here.

## Completion and remaining acceptance

- External-hide safety blocker above is resolved. Visibility never clears an
  uncertain tiler-hide hold. A readable durable ledger resolves ownership;
  unreadable rescue evidence retains a hold for every single-hide error branch,
  not just `uncertain:` errors. Exact durable identity matches restore the
  in-memory claim. Proven absence clears only the uncertainty, not membership;
  destroyed-window cleanup still retires normally. No shared-core change.
- Eight external-hide regressions cover close/refill/MRU, unknown facts,
  transition-only defer/recover logs, visible recovery followed by another hide
  with a kept ledger, tiler workspace hide/reveal, non-uncertain post failure,
  authoritative absence and fresh-token reshow without an old slot. Independent
  safety review accepted the corrected production lifecycle; D23 adapter tests
  pass 35/35. Fresh-token reshow fixtures supply post-admission snapshots, not
  native token stamping or a live application hide.
- Pager classification defect is corrected: successful low/AppContainer reads
  no longer imply elevation. Only integrity at/above high suppresses, alongside
  unchanged process-open/read failures. Fullscreen/game, unknown-fact and click
  suppression stay intact. No shell exception. Shared tiling admission/dispatch
  and fullscreen holdability are intentional medium-only manageability fences,
  not the pager overlay's elevated classification, and remain unchanged.
- `pager-native` logs normal-level baselines and changed readbacks after actual
  create/show effects and the existing topmost recovery. Counts report visible,
  checked topmost/unknown, above/below/unknown relative to a freshly found primary
  taskbar, taskbar visibility and checked topmost. The relative walk is bounded
  to 64 steps per direction; absence/failure/bound exhaustion stays unknown.
  Identical readbacks stay silent across stages; suppression resets the baseline
  for the next show. No added timer, polling or application-content logging.
  Independent source review accepted policy, native readback and privacy;
  production logger tests prove baseline/change/repeat silence/recovery offline.
- Final offline gates: locked five-package build and strict all-target clippy
  passed; locked core/protocol/KWin-FFI/Windows tests passed 2,939/0/1 (Windows
  1,702/0/1, library 1,289/0/1). Pager filter 90/0; unsafe guardrail 32/0;
  workspace format and diff checks passed. Known eight unchanged macOS tests
  requiring Unix `sleep`/`true` were not rerun on Windows. Cross-platform CI
  remains pending. No live Windows test, native accent query, settings change or
  fault injection was performed for these corrections.
- User-run acceptance, under the live Windows guide and existing scoped ordinary
  apps with the established out-of-hook stop/restore route:
  1. Compare actual destroy, native minimize, app hide and reshow in
     `H[W1 V[W2 W3]]`. App hide must refill W3, preserve existing close/MRU
     behavior, and reshow must fresh-admit without the old slot. Ordinary workspace
     hide/reveal must retain layout, never close members. Correlate any hold with
     `reconcile-deferred` and subsequent `reconcile-recovered`.
  2. With pager enabled: ordinary app -> desktop background -> ordinary app ->
     Start open/dismiss -> second ordinary app; hold each state, repeat once.
     Record actual visibility alongside `pager-gate`, `pager-suppress` and
     `pager-native` shown/reasserted readbacks. Allowed policy alone is not native
     visibility proof. Persistent disappearance with visible/above readbacks
     remains unexplained and needs that evidence, not a shell exemption.
  3. Retain the already-passed 10/10 startup accent result. Outstanding separate
     checks: idle accent change/restoration, pager-disabled/hidden/theme-off
     variants, and healthy next-tick separation after maximized startup admission.
     Wider game/DPI/output/fault variants remain unproven, not blockers for the
     bounded offline corrections.
- Delivery pending upstream integration: the remote main branch advanced seven
  commits after this tree's base, including shared-core workspace changes,
  removal of the legacy protocol route, macOS work and overlapping specification
  records. The passing gates above precede that integration, not evidence for a
  combined tree. No upstream work has been overwritten. Integrate both sets of
  changes without dropping either set of records, rerun applicable offline gates,
  then check CI before treating delivery as complete.
- Open product decisions: none for these corrections. Exact next action: finish
  upstream integration and combined-tree verification, then run corrected-build
  user acceptance above and correlate remaining presentation failures. Keep this
  record active until those physical outcomes are known.
