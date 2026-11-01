# Windows Per-Window Float Parity

## Goal And Scope

- Parity item 5: Win+G matches KDE Meta+G. Reuse Engine intentional float,
  retained geometry and fresh-admission unfloat placement; no KDE changes.
- Floating leaves the tree and reflows siblings, retains exact toggle focus,
  requests keep-above, keeps the active border and suppresses group underlay.
- Directional focus/move/resize and workspace send refuse a focused float;
  workspace selection hides/reveals floats with their frames intact.
- Session-local float state resets on restart, matching KDE. Stop/crash leave
  geometry in place. Preserve born-fullscreen holds as a distinct adapter lane.
- Excludes workspace-wide floating, sticky float (item 6), settings UI and
  dependency installs. Win+Shift+G is not implemented.

## Units And Acceptance

1. Short read-only investigation of KDE/core/Windows gaps (complete).
2. Smallest adapter closure plus meaningful regression coverage and native gates.
3. Fresh independent public/live-behavior review; existing-harness-based helper
   and Notepad/Calculator/Paint proof, with exact cleanup and artifact binding.
4. Hosted CI; promote durable decisions and archive.

- Verify centered first float, sibling reflow, retained moved/resized frame,
  admission-axis unfloat, exact focus, repeat exclusion and overlay refusals.
- Verify border on floats, held underlay hidden, native pointer freedom,
  navigation/send exclusions, hidden float reveal, close, stop/crash/restart.
- Locked four-package build/test/strict all-target Clippy, rustfmt, whitespace,
  harness parse/mock and hosted Windows/Rust/KWin/shell gates.
- Live authority: current user grants desktop window control, hooks, overlays
  and exact project-owner crash probes; never close/kill/type into hosting
  Terminal. No registry/policy changes. End with zero project actors, overlays,
  ledger/requests or hidden residue; arranging 1, pen visualization 35.
- Synthetic evidence is machine proof; physical feel/input is user-owned.
  Unattainable rows remain explicitly unaccepted rather than blocking safe
  gated shipment.

## Investigation And Decisions

- KDE catalog: `kwin/src/plan-adapter-entry.ts` float Meta+G; float and keepAbove
  application in `plan-adapter.ts`; send observer explicitly excludes floating
  movers but retains floating survivors. Workspace occupancy includes floats.
- `tiler-core/src/session/ops/float.rs` and Engine ToggleFloat already own
  geometry retention, tree removal and fresh-admission axis. No core change
  expected. Windows lacks binding, intentional-float observation/actuation and
  related membership gates; existing floating rows currently mean born hold.
- KDE float state and keepAbove preimages are runtime-local; disable restores
  project keepAbove. Use official Win32 topmost as the Windows analogue,
  retaining a pre-existing topmost setting. Crash/restart details to verify.
- Known environment issue: cloaked Explorer ApplicationFrameWindow may block
  activation with zero project actors. Fixtures may prime an approved app;
  unavailable rows remain unaccepted. No production shell workaround.

## Evidence And Outcome

- Initial checkout verified clean main at 778e72e, equal origin/main. Adapter-
  only closure in `crates/tiler-windows/src/{snapkey,tiling,tiling_sys,
  workspace_owner}.rs`, with classifier, Engine placement/retention/topology
  and hidden-snapshot regressions. KDE and shared core are unchanged.
- Independent review found hidden born-row loss, premature Engine commit,
  ignored topmost effects, stale unfloat focus and runtime cleanup leaks.
  Corrected and independently checked. Original
  band preimages preserved before optional readback and across later attempts, and
  automatic band retries removed. Local Engine candidate commits only after
  target native effects verify; pre-existing topmost remains untouched.
- Native locked four-package build/test/strict all-target Clippy,
  rustfmt and whitespace pass. `scripts/windows-float.ps1` parse/mock passes;
  its status regression distinguishes partial/unexecuted rows from pass.
  Hosted CI status is recorded with the delivery commit below.
- Scoped completion ships the reviewed implementation with explicitly open
  live acceptance. Durable choices are in
  `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays`. No sticky/workspace-wide float delivered.

| Local receipt under `target/windows-float/` | Actual evidence |
| --- | --- |
| `20261003-075159-8020/float-report.json` | Honest `partial`, Win11 build 26200, medium/session1. Three helpers created; owner suspended at `fullscreen-foreground`; graceful stop preserves frames, restore and inter-stage cleanup succeed. Approved Notepad/Calculator/Paint read-only visible-frame snapshots. Prime extra identified before activation failure and closed by exact HWND; originals survive. No float action accepted. |
| `20261003-075159-8020/float-audit.json` | Actors absent, zero known-owner surfaces, clean ledger, arranging1, pen35,   original approved apps intact. Historical `terminal_alive` field checked the harness itself; corrected to captured WindowsTerminal PID/start identities before final shipment. The real Terminal independently audited. |

- Receipt binds baseline 778e72e plus production/test diff SHA256
  `8368771414B059716947203A9A8CE0BA72E0723950A0BB433CA0174300BA4DA7`, owner
  `250980BBBBAF40C0FDEFC81F724C7DAE3D9486CDE9728041FCA944DE73253D93`,
  helper `FA62FB887E7A307785CAD9E096712EB7DEDAA85C3CB23E759110D77E12034511`,
  harness `D13AA573C3FF35EF42D9BD745A624C63595E836C2BAE1EAF02679039B6A22BC5`.
  The later no-retry and Terminal-audit corrections have offline gates, not
  new live float evidence. Scoped identifiers stay local, not production logs.
- Read-only audit 2026-10-03 07:55:55 +10:00: zero project actors;
  ledger/stop/workspace requests absent; no owner/surfaces; arranging1, pen35;
  actual hosting WindowsTerminal PID18224/start identity survives. No hidden
  project residue reported; approved-app visibility confirmed in the live audit.

## Limits And Failed Approaches

- Cloaked Explorer ApplicationFrameWindow still holds foreground. E8/attach
  approved-app activation fails (`AttachThreadInput=false`); an Explorer-
  launched Notepad extra also never becomes foreground. No shell/product
  workaround or third activation approach. The pre-existing environment issue
  is not proof that every later shell effect is unrelated to the product.
- All float live rows remain unaccepted: centered placement/sibling reflow,
  exact focus/repeats, moved/resized retention/admission, topmost, border and
  held underlay, directional/send refusals and survivor preservation, hide/
  return/close, overlay refusal, stop while floated, hidden crash/watcher,
  restart and born hold. Ordinary-app float actions additionally require
  physical input because normal mode rejects synthetic chords and has no float
  CLI. No no-owner API hide cycle is counted as product workspace evidence.
- Earlier reports `073728`, `073806`, `073845` called partial journeys pass;
  superseded by the honest final report. Earlier admission/mark and AST helper
  dependency failures were fixture defects, not accepted product attempts.
  One leftover owned helper was closed by exact HWND/tag; final cleanup clean.
- Provisional, to discuss: crash can retain project-raised topmost; restart
  resets float state and takes current native band as-is, matching KDE's
  runtime-only preimage rather than adding persistence/recovery machinery.
- Physical Win+G/Start coexistence, display/feel, hung/refusing targets,
  same-process HWND reuse and other DPI/output arrangements remain user-owned.

## Completion And Handover

- Implementation and evidence delivered as `d8329e3`, pushed to main. Hosted
  [CI 37069767244](https://github.com/beefsack/OmniTiler/actions/runs/37069767244)
  passed Windows, Rust, KWin and shell jobs.
- Backlog proposal: item5 float shipped (Win+G), gated/independently reviewed;
  full live acceptance open under the Explorer activation blocker, crash band
  semantics provisional. Link this archive; next item6 belongs to another piece.
- The follow-up below supersedes the original next acceptance action.

## Foreground Recovery And Acceptance Follow-Up

- 2026-10-03, `3f70136`: cloak-aware foreground veto fixes the invisible-cover
  suspension defect without bypassing admission or shell safety. Native gates,
  independent review and hosted Windows/Rust/KWin/shell
  [CI 37072709516](https://github.com/beefsack/OmniTiler/actions/runs/37072709516)
  pass. Narrow WM_CLOSE failed; authorized exact-identity Explorer restarts
  cleared the foreground surface, taskbar returned and Terminal survived.
- New receipts under `target/windows-float/`; owner SHA256
  `E6CD9578B9AA49E7A7B5747764BE5B9303A0B2CF49194A593E7878F2F57646F4`, helper
  `ADAB72A5E9EAD69E1F8577E82FF334121B0161BCEB1CDF815E995D29FCC160FB`:

| Receipt | Observed evidence | Limit |
| --- | --- | --- |
| `20261003-084346-28728` | Approved-app activation succeeded, attach true; helper owner ready | Admission failed with cloaked helpers, eligible=[]; no float action |
| `20261003-084937-20684`, `20261003-085033-19256` | Approved-app read-only snapshots and cleanup | Originals unexpectedly maximized; no ordinary-app float proof |
| `ownerless-20261003-085453-15412` | Tagged helper cloak 0 before/idle, 2 after exact move with zero owner; style unchanged | Cloak mechanism unknown; not float behavior |
| `20261003-092035-18828` | Final shared gate records cloak 0->2->2, exact move/restore, explicit environment-precondition failure and exact probe close | WorkspaceFloat aborted before owner; no stage pass inferred |

- Shared disposable-helper preflight now runs before any owner/stage, requires
  readable cloak 0 before/after/restore, preserves exact identity/geometry guards,
  and reports blocked execution explicitly. Six executed mock cases plus live
  consumer verification pass after independent review. A cleanup diagnostic no
  longer catches its own maximized-original refusal as a failed native read.
- All live float behavior rows remain unaccepted/user-owned after bounded effort:
  placement/reflow, focus/repeats, moved/resized retention/admission, visuals/band,
  direction/send, workspace hide/return/close, overlay refusals, stop/crash/watcher/
  restart and born-hold isolation. Normal mode still rejects synthetic chords;
  ordinary-app Win+G remains physical dogfood. No product cloak bypass or new
  float CLI added. Physical feel/input and other output/DPI checks remain open.
- Exact next acceptance action: restore normal desktop state, verify a fresh
  ownerless helper move remains uncloaked, then rerun OwnedFloat/WorkspaceFloat
  and physically dogfood approved-app Win+G. Do not start item6 in this change.
- Audit 09:23:27 +10:00: no project actors/surfaces/ledger/requests, ready
  false, arranging 1, pen 35, taskbar visible, Terminal same creation. Follow-up
  record: [Windows foreground acceptance](windows-foreground-acceptance.md).
