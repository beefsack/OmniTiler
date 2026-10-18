# Windows Maximise Parity

## Goal And Scope

- Deliver Windows parity item 3 against the current KDE implementation and
  shortcut catalog. Win maps to Meta; Win+Arrow remains directional focus.
- Retain maximised members' tile slots and sibling layout, skip overlay geometry
  writes, restore retained allocation, and preserve workspace and recovery rules.
- Match KDE's actual navigation/movement and initial-admission behavior. Do not
  introduce a new suppression policy or change KDE behavior. Item 4 is outside scope.

## Units And Approach

1. Investigate KDE sources/catalog/archive and current Windows routes; identify
   the smallest gaps and a concrete acceptance matrix.
2. Implement native adapter gaps using official Win32 APIs, with focused
   regression coverage and reusable existing proof-harness patterns.
3. Independently review public/live behavior; verify native gates and scoped
   helper plus Notepad/Calculator/Paint journeys, then check CI.
4. Promote durable decisions and archive this record at completion.

## Acceptance And Verification

- KDE default maximise binding, native maximise/restore, retained slots and
  sibling layout, both-visual suppression, navigation/movement, workspace
  send/hide/reveal, initial admission and stop/crash frame preservation.
- Locked four-package build/test, strict all-target Clippy, rustfmt, whitespace
  checks, relevant harness checks and hosted CI green.
- Synthetic machine evidence is distinct from user-owned physical acceptance.
  Live protocol and current session authorization govern every live run.
- Clean live end state: no project actors/overlays/ledger/hidden-window residue;
  SPI arranging 1 and pen visualization 35.

## Outcome And Decisions

- KDE source: `plan-adapter-entry.ts` catalog uses Meta+M; `plan-adapter.ts`
  retains overlay allocations, allows focus and refuses maximised move/pointer
  operations; `workspace-send-adapter.ts` skips overlay geometry on send.
  `maximize-isolation.md` and `shortcut-sticky-maximize.md` supply the archived
  context; current decisions supersede their historical hard-signal requirement.
- Windows now routes Win+M and focus through verified retained members, carries
  canonical tile rectangles and restores once at first eligible admission. The
  existing eligible-only write subset and fresh native classifier already
  exclude maximised geometry writes; no new core overlay state is introduced.
- Focus revalidation allows maximised members without relaxing geometry writes.
  Workspace sends and remembered focus include them, with fresh pre-dispatch
  overlay checks and existing pre-effect identity/state fences.
- Native maximize uses ShowWindowAsync; restore uses nonactivating asynchronous
  WINDOWPLACEMENT. Dispatched requests are not logged as observed completion.
- Provisional, to discuss: one native attempt per discrete Win+M down, held
  repeats consumed without dispatch, no persistent attempted-state map. A stale
  map reproduced KDE's identical-toggle refusal after native restore; removing
  it is the smallest reversible correction. KDE behavior/code is unchanged.
- Durable behavior and the provisional choice promoted to `docs/decisions.md`.

## Accepted Evidence

| Evidence | Scope |
|---|---|
| `target/windows-maximise/20261003-032549-25592/maximise-report.json` | Current-artifact All journey on Win11 build26200, medium integrity, session1: one-shot admission with unchanged foreground; Win+M max/restore and repeat exclusion; native SC_MAXIMIZE/RESTORE and title-bar double-click; siblings stable and exact restore slot; exact directional focus OUT/INTO max; move refusal; genuinely held Win+Shift underlay show/suppress/reshow; populated ws2 and trailing sends with exact follow foreground and focus-ok, target sibling allocation, hide/return preserving max, restore target slot; graceful frame preservation, hidden-max crash watcher reveal; scoped Notepad/Calculator/Paint native max/hide/reveal/restore. |
| Final native gates and harness mock | Locked four-package stable build/test, strict all-target Clippy, rustfmt and whitespace pass. Binding/modifier/origin/mask/repeat, overlay allocation/admission/write exclusion   and focus eligibility regressions pass. Native gates repeated after latest code. |
| Independent review and correction follow-up | Admission gates, retained origins, async nonactivating restore, focus-only eligibility and geometry isolation checked. No remaining actionable serious finding. |
| Final read-only audit, 2026-10-03 03:32:51 +10:00 | Zero project actors and global border/underlay HWNDs; ledger/stop/workspace request absent; arranging1, pen35. Harness cleanup verifies helpers gone and approved apps visible. |

- Accepted run pins baseline e07b84c plus recorded working diff, owner SHA256
  `A9EA04456B6859DB293ED73B52230FE61A35D23855693A1C20912A0A8AFA503C`,
  helper `4A55D535BB1EA7A785797CF8ED1D3BF381E8CF144D7EE280106B24C2B15F4F1C`,
  harness `5C8D69A978A3B5EE1C793C3D84C01555F4F3340433C3BEBC0734B7F30FA3E94D`.
  Evidence stays local under target; production logs contain opaque tokens only.

## Failed Approaches And Limits

- Initial implementation passed tests but review exposed missing retained
  shortcut origins, overbroad admission-clear candidates and activating restore.
  Corrected before accepted live proof. Initial pointer-path investigation was
  wrong: Windows already has pointer resize; the corrected route preserves its
  overlay refusal. Dead helper-only tests were removed or wired to production.
- Initial toggle attempted-state fence refused own-max/native-restore/Win+M;
  replaced by discrete attempts as above. `20261003-025034-34636` is partial
  evidence only: sends were focus-unverified, the supposed populated target was
  empty and underlay suppression lacked an active hold. All are superseded by
  the accepted run's exact focus, seeded target and held visual gates.
- Two blind maximise-button mouse fixtures hit Close and destroyed only owned
  helpers. GetTitleBarInfo fixture queries failed with error87; no third mouse
  approach was attempted. Button-click acceptance remains physical/user-owned,
  with native system-command and double-click convergence proven separately.
- Fixture corrections: async-dispatch waiter, overlay visibility field, slot
  rather than native-max neighbour math, DWM rather than virtualized outer rect,
  watcher ledger-settle wait, marked SendKey overload and workspace-action focus
  field. `031952` and `032104` failed those last two fixture contracts; the final
  `032549` run passes. No production exception masks a fixture failure.
- Physical input/Start coexistence, button click and feel, hung targets,
  same-process HWND-reuse stress and multi-output/mixed-DPI remain follow-ups.
  Fullscreen toggle belongs to item4 and was not implemented or live-tested.
- Post-plan send uncertainty uses existing observation convergence; no new
  rollback/pending framework is selected. No Linux/KDE live test is claimed.

## Completion

- Delivered as `1be97a1`, pushed to main. Hosted
  [CI 37041636260](https://github.com/beefsack/OmniTiler/actions/runs/37041636260)
  passed Windows, Rust, KWin and shell jobs. This record is archived at scoped
  completion; physical follow-ups and the provisional choice remain explicit.
- Backlog proposal: mark item3 maximise delivered with this archived evidence;
  retain physical checks and provisional discrete-toggle choice; next item4
  belongs to a fresh piece. No further implementation for this change.

## Second agent-live sweep (2026-10-17)

- Win+M keyboard acceptance was not attempted: injected keys are rejected by
  the product hook and would reach shell minimize-all. The safety condition
  for protecting unrelated windows was therefore not met. Existing accepted
  maximize evidence stands; this sweep adds no physical-toggle acceptance.
- Scoped probe teardown left settings/SPI unchanged, ledger clean and no tiler
  residue. No input-filter bypass or shortcut policy change was introduced.

## Scoped follow-up review (2026-10-18)

- Win+M input leg not run: the normal owner rejects injected input, leaving the
  shell minimize-all chord capable of disturbing unrelated windows. No injected
  Win+M, input-filter bypass or duplicate native-maximize journey was attempted.
- Existing native maximize/send/restore evidence is retained. Physical Win+M
  interception, pairing and ordinary-app dogfood acceptance remain user-owned;
  helper or system-command results cannot establish those input assertions.
