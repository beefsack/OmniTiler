# Windows D23 minimize restore and close cleanup

## Goal and acceptance

- Replace retained minimized allocation with release/reflow/MRU and session-only
  best-effort neighbor rejoin; preserve sole-minimize workspace with focus none.
- Rejoin keeps former side/group/index and comparable old share where possible.
  Intervening resize/admission/sibling close/swap/gaps do not invalidate alone;
  only no former neighbors, missing domain or send elsewhere admit fresh.
- Restore writes no focus; native nonactivating minimized reveal stays unchanged.
- Verified closed lifetime ids report through `closed_windows` on valid domain
  observations, including hidden workspaces and multiple-domain close bursts.
  Rejected/unreadable observations retain reports; minimize/transient absence
  never counts as close. Cross-output pair observations retain empty reports.

## Approach and verification

- Omit known minimized members from carried rows while retaining adapter identity;
  shared convergence releases/stores slots and performs best-effort restoration.
- Queue verified closes before dropping member state; stamp domain-local reports
  and remove them only after planner acceptance.
- Verify observation completeness, native lifetime classification, hidden-domain
  delivery, slot-aware retirement and absence of restore focus writes at actual
  production seams; run the native offline allowlisted gates.

## Delivered outcome and offline evidence (2026-10-13)

- Windows-only adapter delivery complete. Known minimized rows omit; shared
  convergence releases/reflows and stores the slot. Hidden iconic state is
  refreshed read-only behind exact-lifetime ownership, preserving the reveal
  preimage. Native minimized reveal is unchanged; reconcile writes no focus.
- Verified lifetime closes queue with domain homing before membership removal.
  Valid observations drain reports only after acceptance; inactive-domain fanout
  uses complete carried membership without geometry/focus writes. Gap drift
  adopts through the existing UpdateGaps route. Confirmed domain release drops
  reports whose slots were destroyed. Cross-output pair report sets stay empty.
- Twenty-seven adapter tests cover real observation/cleanup/assembly/Engine
  seams, MRU/null/observed-restored focus, actual resize and swap, newcomer,
  sibling close, interleaved minimize, bounds/gaps, restart/fallback, rejection,
  unreadable/lifetime evidence, real hidden claims and automatic close fanout.
  Injected observations bypass native effects; these are not native journeys.
- Review corrected hidden snapshot omission, stale hide-time iconic state and
  inactive-domain close delivery. A hidden gap-drift regression failed with the
  adoption branch disabled and passed with UpdateGaps; no unresolved offline
  acceptance blocker remains.
- Rust 1.99.0, `x86_64-pc-windows-msvc`: locked build/test for `tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows` passed: 1,845 passed,
  0 failed, 1 ignored. All-workspace fmt, allowlisted all-target strict clippy
  and `git diff --check` passed. Shared core/protocol unchanged.
- Source stored-slot lifetime after sending elsewhere remains the existing
  shared/KDE behavior, with destination fresh admission. No new source cleanup
  policy was inferred; the [minimal discriminator](../../spec/reference-outcomes.md#d23-best-effort-restore-and-close-discriminators)
  leaves source keep/forget intent and native outcomes TBD.
- User-approved 2026-10-16 D23 item 6 follow-up: research Tier 1 reference-WM
  behavior first, then decide consistently with it. The earlier lean toward
  forgetting the source slot is context, not approval. No new research or
  source-slot policy change is delivered here.

## Pending physical acceptance

- Tile unequal-share A/B/C, minimize B, inspect sibling reflow/MRU, restore B;
  inspect former neighborhood, comparable share and native focus behavior.
- Repeat fresh with focus change, resize, newcomer D, sibling close, swap,
  another minimize/restore, gaps/work-area change; repeat with no former neighbor,
  removed domain and send elsewhere to verify fresh-admission fallback.
- Minimize sole A, leave/return to the workspace, verify retention/null focus,
  restore; restart while minimized and verify session-only slot expiry.
- Close minimized B, sole minimized A, and hidden minimized windows on multiple
  domains; verify slot cleanup, identity reuse and eventual domain cleanup.
- Exact native frames/focus/ordering remain TBD; no live testing performed.

## Verification follow-up (2026-10-14)

- Initially minimize/restore rejoin and close-slot cleanup were not exercised.
  The scoped lifecycle pass verified owner exit, settings restoration and a clean ledger,
  but did not newly demonstrate post-exit restoration of hidden ordinary-window
  claims. Subsequent helper recovery preflight passed as described in the
  [D16 follow-up](windows-d16-floating-transfer.md#verification-follow-up-2026-10-14).
- Agent-observed basic unequal-share minimize/restore passed with scoped
  helpers: native iconic state removed the allocation and reflowed a neighbor;
  restore recovered the former neighborhood and exact prior frame/share.
  Correlated restore trace contained geometry writes without product focus
  writes; activation caused by native `SW_RESTORE` is distinct from a tiler
  focus write. A basic sole-window minimize/restore retained its frame.
- Closing a minimized tagged helper was followed by process exit, fewer managed
  windows and eventual survivor reflow. This is partial cleanup evidence, not
  proof that internal stored slots were removed. There is no per-close trace
  event; absence of a `closed_windows` log line is not a failure oracle.
- A later fresh-admission probe captured overlapping frames during admission
  and an incompletely attributed managed count. It does not establish settled
  slot cleanup or identity-reuse acceptance. Those checks and the full changed-
  neighborhood, hidden-domain and restart matrix remain pending.
- Final owners/fixtures were absent, ledger/request/pointer clean, settings
  byte-identical and Mouse Snap restored. No new D23 product defect or physical
  acceptance is claimed.

## Agent-live follow-up (2026-10-17)

- Single-output scoped ordinary-app run at `dffdf871`: minimizing B reflowed
  siblings; after an intervening focus change, restoring B recovered all four
  measured pre-minimize frames exactly. Closing minimized Paint exited its
  exact process and survivors reflowed. Evidence:
  `sendunit-j4/20261017-065934-24144`.
- A minimized member stayed iconic across orderly stop/restore/relaunch;
  restoring afterward produced a different, fresh-admission-shaped slot.
  This is one observation, not complete session-expiry acceptance.
- The attempted sole-window leg used an unavailable numbered target and
  received `unknown-target`; it did not isolate the member and is rejected as
  sole/null-focus evidence. Internal stored-slot cleanup, identity reuse,
  hidden-domain bursts and the broader changed-neighborhood matrix remain open.
  Process exit and absent close log lines do not prove internal slot retirement.
- Locked four-package gates passed: 2,458 passed, 0 failed, 1 ignored; build,
  fmt, strict all-target clippy and diff check passed. No source change.

## Second agent-live sweep (2026-10-17)

- Middle minimize released allocation and reflowed siblings; restore recovered
  exact pre-minimize frames, including a resize-skewed unequal-neighborhood
  variant. Sole minimize retained iconic state across workspace leave/return
  and restored its frame. Native foreground on the iconic member does not
  establish engine-internal null focus; that claim remains open.
- A minimized newcomer was closed, survivors recovered their skewed frames,
  and a fresh newcomer admitted beside a different anchor occupied a different
  region rather than the closed member's slot. Combined with the passing
  `minimized_close_reports_and_forgets_slot_until_acceptance` regression, this
  supports stored-slot retirement; native geometry alone is not an internal
  state oracle.
- No new D23 defect. Settings/SPI restored, ledger clean, no tiler residue.
  Hidden-domain bursts, session expiry and remaining changed-neighborhood
  variants remain pending. Source-slot policy after send is unchanged.
- Evidence retention limitation: the unequal-share native report was later
  overwritten. Contemporaneous independent inspection recorded the differing
  admission regions; the surviving equal-share native run alone is not a
  non-reuse discriminator. The targeted slot-absence regression was rerun and
  passed. Do not infer full internal native-state visibility from geometry.
