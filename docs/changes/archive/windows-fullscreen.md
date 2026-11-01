# Windows Fullscreen Parity

## Outcome And Scope

- Windows parity item 4 implementation shipped with scoped acceptance,
  2026-10-03. Full live acceptance remains unaccepted where listed below.
  The gated, independently reviewed product has no known blocking defect;
  incomplete desktop proof is explicitly retained rather than counted as pass.
- KDE's current catalog/wrappers are the reference: Win+F11, retained slots,
  stable siblings, focus eligibility, movement/pointer/maximize/send refusals,
  workspace visibility and first-seen fullscreen hold. KDE is unchanged.
- Scope excludes parity item 5, settings UI, exclusive-mode probing, dependency
  installs and registry/policy writes. Detection remains captionless physical
  monitor coverage, without application-content inspection.
- Native implementation: `crates/tiler-windows/src/{snapkey,tiling,tiling_sys,
  workspace_owner}.rs`, focused `tests/{snapkey,tiling}.rs`, shortcut catalog
  row and `scripts/windows-fullscreen.ps1`. Durable choices promoted to
  `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays`.

## Material Choices And Review

- Provisional, to discuss: official Win32 style/frame calls implement project
  borderless fullscreen because Windows has no generic fullscreen setter.
  Inert window-lifetime scalar properties capture only cleared frame bits and
  prior maximize state. Owned metadata wins over geometry on exit; unrelated
  app style changes survive. Stop/crash leave frames and metadata in place.
  App-owned fullscreen without a preimage refuses; never send app F11 blindly.
- Provisional, to discuss: one attempt per discrete Win+F11 down, consuming
  held repeats without dispatch. No automatic retry/attempted-state map.
- Managed fullscreen remains observable and focusable, never writable for
  geometry; unmanaged fullscreen still suspends. Born-held members are
  slotless floating Engine exceptions but occupy hideable workspaces until
  exit or close. Restart follows the same first-seen hold rule.
- Previous review corrections preserved fresh pre-effect identity/scope fences,
  overlay-bypass safety gates and preimages after partial style effects. A new
  independent review found no blocking product defect or maximize/workspace
  regression. Low residual: `member_rects` also seeds born-held hidden snapshots;
  that seed is not an Engine tile slot and is removed on release/close.
- Harness review corrected overlapping owners, shortcut-only workspace mode,
  send-success crash setup, select-current-workspace hiding, DPI-mixed rect
  comparisons, normal-close-as-born labeling and style-only app entry proof.
  Activation now makes one bound attempt with diagnostics, without machine-
  specific waits or retry loops. Report status is `partial` if any required
  observation is unavailable/unaccepted/skipped; CLI marks precede dispatch.

## Native And Live Evidence

- Reran locked build/test/strict all-target Clippy for `tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`: pass. Rustfmt,
  whitespace, fullscreen parse/mock and shortcuts mock: pass. Hosted Windows,
  Rust, KWin and shell checks are required on the pushed commit; their status
  is tracked with the commit's CI checks.
- Physical host: Windows 11 build 26200, medium integrity, one 2560x1440
  monitor, DPI 120, work area 0,0,2560,1380. Synthetic results are not physical
  input/display acceptance. Local receipts are under `target/windows-fullscreen/`.
- Production/shortcut working diff stayed byte-identical to the inherited
  `fbefea1` baseline throughout live work: SHA256
  `66C283BFCCA35B38C2CBE736D2B6350ABFE0A6865F136B6DE6FF172899B0FD5B`.
  Owner artifact SHA256
  `FC96AC747141ABD2E6B8C7D88018035477BF1EC5FB39568F299C1E603C08517B`;
  helper `EBCA63109CC5B1785363CF84562E166E65EAED374832624367A06E9772F4B993`.
  Each report pins its harness hash and operations.

| Receipt directory | Accepted observations | Limitation |
| --- | --- | --- |
| `20261003-054249-30968`, `20261003-054522-18352` | Unmanaged suspension/resume; project exact cover/style/preimage; immediate foreground held; stable siblings; both visuals hidden | Later refocus refused; full OwnedFs matrix did not finish |
| `20261003-060236-11048` | Born held once and slotless; workspace hide/return with intact frame; app-owned toggle refusal | Explorer foreground prevents post-restore resume |
| `20261003-060412-15916` | Notepad/Calculator/Paint normal tiling baseline and readback | Fullscreen app entry/exit journeys unavailable |
| `20261003-062006-33476` | Born native release and convergence; project enter/exit restores exact retained allocation | Normal close convergence timed out; log shows fullscreen-foreground suspension after exit |
| `20261003-062657-13604` | Independent born-fullscreen close, token removal/release once, no refire, two siblings converge, graceful cleanup | All executed stage oracles pass |
| `20261003-062944-24600` | Graceful stop/restore preserves project cover, valid preimage and sibling frames; zero overlays | Crash park unavailable; historical top-level `pass` is only partial evidence, fixed in final harness |

## Original Foreground Diagnosis And Unaccepted Checks

- Direct native reads identify the persistent foreground as an Explorer-owned
  `ApplicationFrameWindow`, with no child windows/process, no owner/parent,
  topmost/no-activate style and DWM cloak 2. It is not an identified hosted UWP
  app. Direct reads find it even when `EnumWindows` omits it. The precise shell
  surface/purpose remains unknown; Explorer was not terminated.
- Independent control reproduced helper activation refusal with zero project
  processes while ordinary Notepad held foreground. That establishes an
  environment activation problem, not causality for every later takeover.
  Entry's immediate foreground readback passes; seconds-later shell takeover
  remains unexplained. Official NOACTIVATE/NOZORDER flags and that immediate
  readback do not prove the product cannot trigger a later shell response.
- Earlier approaches were E8/attach activation, then raise-first activation;
  neither resolved the shell foreground. A 30-second release wait also failed
  and was removed. Independent stages obtained more evidence without a product
  workaround. Recovery attempt `20261003-062718-36268` timed out before born
  hold under the same suspension; attributed park in the next run was refused.
- Unaccepted/user-owned: sustained foreground retention and full owned
  repeat/focus-out-in/movement/maximize/send/held-underlay matrix; captionless
  pointer refusal (no native title-bar fixture); retained-fullscreen workspace
  journeys; hidden crash/watcher reveal/restart-owned exit; approved-app actual
  fullscreen cover/exit allocation; physical input/display/feel and other
  DPI/output arrangements. App exit harness explicitly lacks a plan-token
  allocation oracle and does not infer it from style/non-cover geometry.
- Every live run ended through exact stop/restore and helper cleanup: no
  project actors, overlays, ledger or hidden residue; arranging 1, pen 35.
  Hosting Terminal survived.
- Final read-only audit at 2026-10-03 06:36:58 +10:00 independently
  verifies zero project processes/native surfaces, no ledger/stop/workspace
  request, ready false, arranging 1, pen 35 and Windows Terminal present.
- The follow-up below supersedes the original next acceptance action.

## Foreground Recovery And Acceptance Follow-Up

- 2026-10-03, `3f70136`: fresh DWM cloak facts now prevent invisible covering
  foregrounds from suspending tiling; invalid/unreadable reads remain blocked,
  real unmanaged fullscreen still suspends. Independently reviewed; native
  four-package gates and hosted Windows/Rust/KWin/shell
  [CI 37072709516](https://github.com/beefsack/OmniTiler/actions/runs/37072709516)
  pass. No blanket ApplicationFrameWindow/Explorer exception.
- WM_CLOSE and graceful termination did not clear the unidentified shell frame.
  Two authorized exact-identity Explorer restarts removed it and restored the
  taskbar; the surface recurred between restarts. Approved-app activation later
  succeeded. Its precise purpose and foreground refusal mechanism remain unknown.
- New local receipts under `target/windows-fullscreen/`, owner SHA256
  `E6CD9578B9AA49E7A7B5747764BE5B9303A0B2CF49194A593E7878F2F57646F4`, helper
  `ADAB72A5E9EAD69E1F8577E82FF334121B0161BCEB1CDF815E995D29FCC160FB`:

| Receipt | Additional observations | Still unaccepted |
| --- | --- | --- |
| `20261003-083115-23852`, `20261003-083403-20852` | Unmanaged suspend/resume; project cover/preimage/stable siblings/exact exit; one toggle for held F11 repeats | Sustained focus and complete refusal/held-underlay matrix; second exit gained WS_MAXIMIZE and failed exact-style oracle |
| `20261003-083711-24948` | Born hold, hide/return and external release once | Retained-fullscreen workspace chain; convergence saw an unexpectedly maximized helper |
| `20261003-083839-8284`, `20261003-083947-36816` | Recovery readiness and approved-app census only | Crash/watcher/restart; approved-app cover/exit; helpers/apps became cloaked |
| `20261003-092018-35968` | Final shared preflight reports explicit environment failure before any recovery owner | Recovery stage never dispatched; no feature pass inferred |

- Independent ownerless probes reproduce helper cloak 0->2 on exact native move,
  with unchanged style and exact geometry; restoring geometry leaves cloak 2.
  Both fixtures now check a disposable helper before any owner/stage, distinguish
  unreadable cloak, and fail with structured environment-precondition facts.
  Shared gate regressions and native runtime probes pass after independent review.
- Ownerless cloak reproduction does not explain the shell's purpose, every
  takeover, or unexpected maximize bits. Those attribution gaps remain open;
  no production workaround or softened geometry/style oracle was introduced.
- User-owned after bounded effort: full focus/refusal/held-underlay matrix,
  retained-fullscreen workspace journeys, hidden crash/watcher/restart-owned
  exit, approved-app actual cover/exit allocation, captionless pointer refusal,
  physical input/display/feel and other output/DPI arrangements. Full acceptance
  remains open. First restore a normal desktop session and verify the ownerless
  move gate passes, then rerun OwnedFs/WorkspaceFs/RecoveryFs/NormalSmoke.
- Audit at 09:23:27 +10:00: zero actors/native project surfaces, ready false,
  ledger/stop/workspace requests absent, arranging 1, pen 35, taskbar visible,
  hosting Terminal same creation. Follow-up record:
  [Windows foreground acceptance](windows-foreground-acceptance.md).
