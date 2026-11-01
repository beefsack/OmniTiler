# Windows Sticky Float Parity

## Goal And Scope

- Parity item 6: Win+Shift+G matches KDE Meta+Shift+G; sticky floats remain
  visible across all managed workspaces of their output. KDE is unchanged.
- Preserve KDE origin-sensitive sticky-off and Win+G sticky-to-tiled behavior,
  focus, geometry, keep-above, directional/send exclusions and visual policy.
- Use the smallest adapter closure supported by existing Engine/workspace
  policy. Restart adoption follows KDE where feasible; unresolved Windows
  analogues are recorded as provisional, to discuss.
- Excludes parity item 7, settings UI and dependency installs.

## Units And Acceptance

1. Short read-only investigation: KDE catalog/code, core/workspace support and
   Windows gaps; identify exact semantics and minimal closure.
2. Adapter implementation and meaningful regression tests; locked four-package
   build/test/strict all-target Clippy, rustfmt and whitespace gates.
3. Fresh independent public/live-behavior review, existing float-harness-based
   offline/live proof and clean recovery audit. Fix concrete findings.
4. Green hosted CI, durable decisions and archive.

- Verify tiled/float sticky-on, origin-sensitive sticky-off on current workspace,
  Win+G clearing sticky and tiling, workspace selection/trailing-empty occupancy,
  navigation/send exclusions, visuals, refusal, close and stop/crash/restart.
- Live authority: current session permits desktop window control, hooks,
  overlays and exact-owner crash probes; preserve hosting Terminal process tree.
  No registry/policy writes. End with no project actors, overlays, ledger,
  requests or hidden residue; arranging 1, pen visualization 35.
- Ownerless move/cloak precondition gates live proofs. Quickly check Windows
  virtual-desktop assignment; if still blocked, ship gated offline evidence and
  explicitly transfer unexecuted rows to the user. Physical acceptance is
  user-owned; no synthetic result is represented as physical proof.

## Evidence And Decisions

- Initial checkout: clean main 13f2b69, equal to origin/main.
- KDE decisions distinguish known tiled origins (fresh tile on sticky-off),
  known float origins (normal float), and adopted sticky origins (normal float
  on current workspace, next Win+G tiles). Win+G on any sticky origin tiles on
  current workspace. Verified in `kwin/src/plan-adapter.ts` requestSticky,
  requestFloat and sticky echo handling; catalog is in plan-adapter-entry.ts.
- Core already models floating/sticky exception facts and owns retained float
  geometry/fresh admission. Sticky is host-side in KDE; no core change expected.
- Windows gaps: shifted-G routing, output-scoped sticky membership/visibility,
  occupancy exclusion and runtime origin tracking. Reuse existing float native
  verification, topmost preimages, exact identity and Engine clone/commit.
- Restart analogue: provisional, to discuss. A window-lifetime sticky marker
  survives stop/crash; the next owner consumes it as a normal float on the
  current managed workspace, preserving the live frame. No prior tiled origin
  survives; the next Win+G tiles. Use no file persistence or recovery-ledger
  expansion; retain the existing float crash-topmost policy.

## Accepted Implementation And Verification

- Adapter-only closure in `crates/tiler-windows/src/{snapkey,model,product_hide,
  tiling,tiling_sys,workspace,workspace_owner}.rs`; tests in workspace/owner and
  snapkey/tiling suites. Shared core and KDE behavior are unchanged.
- Independent review corrected startup adoption, cross-domain band authorization,
  stale runtime membership, an overly broad float-frame hint and missing focus
  retention. Consolidated float/sticky actuation and cross-domain rehome paths;
  all marker writes now retain fresh full-identity/lifetime/scope/proof guards.
  Final independent authorization/fixture review accepts the corrected slice.
- Native locked four-package build/test/strict all-target Clippy, rustfmt and
  whitespace gates pass after the final product changes. Existing float harness
  extended with `-Stage StickyFloat`; parse/mock passes, including executed
  membership-oracle, receipt completeness, classifier and ownerless-gate cases.
- Live exposed a helper ownership defect: `query_owned` rejected topmost after
  project keep-above, breaking later proof identity/focus/cleanup. Removed that
  mutable-state ownership gate; exact exe/class/process/SID/session/integrity/
  tag/parent/owner fences remain. Ordinary production admission is unchanged.
- Fixture failures were corrected against source/live facts: centered geometry
  uses Engine bounds inset by the 8px outer gap; stateless `inspect eligible`
  is not Engine membership; plan/readback domain marks live in tick summaries;
  a single floated member legitimately leaves an empty tile plan. Failed runs
  `111139`..`113807`, `115120`, `115418` are not acceptance evidence. Early VDM
  IID errors were read-only fixture defects, not host API unavailability.

| Local receipt under `target/windows-float/` | Accepted machine evidence |
| --- | --- |
| `20261003-115814-25112/float-report.json` | Honest partial; all nine implemented sticky legs execute. Win+Shift+G tiled-on retains focus, marker1, centered frame `517,281,2043,1099`, sibling reflow, topmost and border. Selecting workspace2 hides sibling while sticky stays visible/frame unchanged; sticky-off tiles there. Float-origin marker2 preserves frame; next Win+G tiles. Win+G clears sticky; directional focus refuses. Graceful stop/restart consumes marker into normal float with frame preserved, next Win+G tiles. |
| `20261003-115814-25112/float-audit.json` | Actors absent, overlays0, clean ledger, arranging1, pen35, originals intact, real Terminal creation preserved. |

- Receipt binds baseline13f2b69 plus tracked diff SHA256
  `C92C0A95501083222D98EB8FC994C20338B7DECC97E9BA9BC845CF7EBED8ACBD`, owner
  `CC63B12FFB939336B4F37B8BEAC5ED932D5E1ADD3A813BA3E931652F34630EC0`, helper
  `0705C9F73126D5BC669A73C3F7291379FEF41F400AF3B7EF824BC2A03DA88AC2`, harness
  `59FC05C6892E6D1B5BB7942632AAFA5C3589F1334C90316485577B4BC504D23F`.
- Ownerless gate cloak0 before/move/restore. Public IVirtualDesktopManager says
  helper is on-current, same desktop ID as foreground. This neither enumerates
  all Windows virtual desktops nor explains the earlier cloak2 environment.

## Remaining Acceptance And Handover

- User-owned: sticky crash/watcher and native restart variants, held-underlay,
  directional move/resize and send refusals, overlay/born-fullscreen refusal,
  close/refusing-window/lifetime races, ordinary-app physical Win+Shift+G/Win+G,
  Start/Snap coexistence, appearance/feel and other output/DPI arrangements.
- Prior OwnedFloat/WorkspaceFloat fixtures still contain stateless eligibility
  membership assertions; the new StickyFloat oracle uses owner plans instead.
  Their older full live acceptance remains open; do not infer it from this run.
- Provisional decisions: Windows restart consumes sticky into normal float;
  existing crash-retained topmost choice carries forward. Durable rules are in
  `docs/../../decisions/window-state.md#fullscreen-and-maximize-overlays`.
- Proposed backlog: item6 sticky float shipped (Win+Shift+G), native gates/CI and
  scoped helper origins/select/current-off/Win+G/restart journey verified;
  broader crash/refusal/visual/physical checks user-owned, restart analogue
  provisional. Link this archive; next queue item7 belongs to another change.

## Completion

- Implementation/evidence delivered as `292d8c1`, pushed to main. Hosted
  [CI 37088627986](https://github.com/beefsack/OmniTiler/actions/runs/37088627986)
  passes Windows, Rust, KWin and shell jobs. Completion archive is a separate
  documentation-only delivery; its CI receipt reported after publication.
- Read-only audit 2026-10-03 12:06:08 +10:00: zero project actors,
  ready false, ledger/stop/workspace requests absent, arranging1, pen35;
  real Terminal PID18224 creation `01dd512e9194d8b9` intact. Final live receipt
  additionally verifies zero native owner surfaces and no hidden helper residue.
  No parity item7 work performed.
- Exact next action: apply the proposed item6 backlog text;
  user physically dogfoods Win+Shift+G/Win+G and the listed remaining rows.
