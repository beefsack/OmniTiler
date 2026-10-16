# Windows D27 activation-request verification

- User-approved 2026-10-16 amendment supersedes the historical guard policy:
  Windows `Switch and focus` honors OS-permitted activation even over a
  fullscreen/game foreground. Default `Mark urgent only` and non-permitted
  requests are unchanged. Implementation remains pending; evidence below is
  historical, not proof of the new policy. KDE behavior is unchanged.
## Goal and acceptance

- Establish current and hidden-workspace activation-request behavior against
  Windows' approved `Mark urgent only` default and fullscreen/game guard.
- Distinguish native foreground denial/flash from a real foreground change and
  OmniTiler's workspace convergence. Verify whether hidden urgency is visible.
- Update D27/item 22, decision delivery pointers and Windows reference/spec
  cells with dated evidence; preserve unsupported outcomes as TBD.
- No new urgency presentation without a product decision. No KDE behavior change.

## Approach and bounded work

1. Source investigation: foreground observer, hide/reveal, focus fences, helper
   capabilities and relevant reference cells.
2. Scoped owned-helper native probe: current/hidden requests, flash notification,
   selected workspace/focus readbacks and exact graceful stop/restoration.
3. Assess conformance gaps, record presentation options if needed, verify and
   archive the outcome.

## Accepted source evidence (2026-10-14)

- At `f4b9f41`, Windows has no `FlashWindowEx`, shell-hook activation/flash
  observer or urgency marker in product source.
- `poll_foreground_workspace` in `tiling_sys.rs` converges to an exact verified
  hidden member that actually becomes foreground; a denied request alone does
  not select its workspace. It does not classify request versus user selection.
- `product_hide.rs` uses `SW_HIDE`; reveal is nonactivating. Existing research
  documents removal from native taskbar/Alt+Tab while hidden.
- Existing fullscreen/elevation fences gate OmniTiler's focus/geometry writes;
  they do not implement dedicated D27 request classification or marking.

## Native evidence (2026-10-14)

- Conditions: Windows 11 physical desktop, ordinary medium integrity, scoped
  disposable helpers, normal owner restricted to `tiler-test-window.exe`,
  keyboard/mouse takeover disabled. Source `f4b9f41`, no product source changes.
- Owner artifact SHA-256:
  `67471947DEBA851C87B2307C727163995FF290F331AE474F50CEC6F83BBDF83B`.
  Helper artifact SHA-256:
  `4C70383C5B9E6BF8D25E30CE9C83401EA0F2BB63B71A34CAA099F16C6910F0C7`.
  Process creation/executable/tag identities and window baselines were checked
  in the disposable probe evidence; local resource identifiers are omitted here.
- The request caller was PowerShell targeting an exactly verified helper, not
  the helper's own thread. Setup activation and request provenance are distinct.

| Probe | Request provenance | Observed outcome |
| --- | --- | --- |
| F1 / F3, tiler off | Fresh background process, never attached to an input thread; current B / native-hidden C, A foreground | Plain `SetForegroundWindow` returned false; A remained foreground immediately, at 500 ms and 2 s; target visibility unchanged |
| F5 / F6, tiler on | Same fresh-caller route; current B / product-hidden B on workspace 2, A on workspace 1 | Returned false; A remained foreground through 2 s; hidden B stayed hidden; owner log contained no `edge=foreground` select |
| F2 / F4 / F7 | Fresh background `FlashWindowEx(FLASHW_TRAY, count=3)` on current B / native-hidden C / product-hidden B | Foreground and visibility unchanged through 1 s; return false describes previous inactive caption, not failure |
| Shell observer | Separate message-pumped `RegisterShellHookWindow` observer; exact owned targets only | Registration succeeded; activation (4) positive controls heard; `HSHELL_FLASH` (0x8006) heard for B/C; no `HSHELL_RUDEAPPACTIVATED` (0x8004) heard. Events lacked timestamps, so individual flashes are not attributed to individual request/flash legs |
| T1, earlier controller | Request thread had previously used `AttachThreadInput` for setup and then detached | Plain setter returned true; hidden B became foreground and visible, A became hidden; log recorded workspace 1 -> 2 `edge=foreground`, `outcome=ok`, `focus=focus-ok` |

### Evidence correction and limits

- Independent review rejected the initial inference that T1 established a
  clean background request route. The controller's prior attachment confounded
  foreground rights. Fresh requester processes corrected that fixture and
  established the denied class above. T1 still demonstrates convergence after
  permitted hidden-target activation; it is neither helper self-request proof
  nor an explicit Alt+Tab/taskbar selection result.
- Initial taskbar UI Automation matching failed its visible-helper positive
  controls. It supplies no presence/absence or visible-flash evidence. Native
  shell flash traffic is not proof that the user sees urgency for a `SW_HIDE`
  window. Existing [hidden-workspace research](../../research/windows-port/alt-tab-hidden-workspaces.md)
  documents the taskbar/Alt+Tab omission risk; presentation remains unresolved.
- Native denial plus no tiler switch is verified for this ordinary fresh-caller
  route only. No dedicated request/urgency observer, marker or clear-on-focus
  state exists in product source. Full conformance is not established.
- Helper-originated and native-permitted clean background activation, visual
  urgency/clear-on-focus, physical explicit selection, fullscreen and real-game
  active/idle outcomes remain TBD. Ordinary focus fences do not prove D27's guard.

## Gap assessment and options for review

- At this verification's 2026-10-14 baseline, the Windows default and
  guard-in-both-modes policy were unchanged. The 2026-10-16 amendment above
  now selects OS-permitted Switch and focus over fullscreen/games.
- Removing `poll_foreground_workspace` switching indiscriminately would also
  break explicit external selection. Existing foreground/shell activation
  signals identify the activated HWND, not a reliable request-versus-selection
  provenance. No obvious conformance patch preserves both outcomes today.
- Presentation options for the existing Windows workspace presentation research:
  1. Native taskbar flash only: smallest UI footprint, but hidden `SW_HIDE`
     windows have no established visible indicator. Not acceptable as verified
     conformance without a passing hidden-window visual check.
  2. Workspace-level urgency in the planned workspace presentation, fed by
     identity-verified shell flash notifications: keeps hidden windows hidden;
     needs selected appearance, lifetime and clear-on-focus semantics.
  3. Tray-only urgency badge/menu entry: uses existing UI, but less directly
     identifies the affected workspace and still needs mark/clear semantics.
- Recommendation: evaluate option 2 with
  [Windows workspace presentation research](../../backlog.md), alongside the
  [existing options survey](../../research/windows-port/alt-tab-hidden-workspaces.md).
  Shell-hook availability is an API maintenance tradeoff, not adopted architecture.
  Permitted-request classification/interception needs a separate technical
  assessment before promising urgency-only for that class.
- Tentative, pending user review: carry hidden urgency presentation into the
  existing workspace presentation research; leave UI and request-classification
  design unselected. This does not relax D27 or mark it complete.

## Verification and outcome

- Native offline MSVC gates: Rust 1.99.0 (`b940084d7`), locked four-package
  build/tests passed (1,852 passed, 0 failed, 1 ignored); all-workspace fmt and
  allowlisted all-target strict clippy and `git diff --check` passed.
  Documentation-only delivery.
- Both probes finished with `mise exec -- just stop`: graceful owner exit,
  independent restore and clean ledger. Product-hidden helpers returned visible;
  native-hidden helper remained at its baseline until exact-tag close. All owned
  helpers, requester and observer processes exited; foreground returned to its
  pre-probe value. No remaining probe windows/hooks/overlays.
- Updated decision delivery pointer, item 22/D27 and Windows spec/reference
  cells. Verification delivery complete; product acceptance remains open for
  the explicitly listed discriminators and presentation/classification choices.
- Next action: select urgency presentation within Windows workspace presentation
  research, then verify helper-originated/permitted requests and physical
  selection/game contrasts before implementing request handling.

## API interpretation sources (checked 2026-10-14)

- [SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow):
  native eligibility varies with input/foreground rights; denied activation can
  flash the taskbar button. An observed denial is not a universal deny policy.
- [FlashWindowEx](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-flashwindowex):
  no activation change; return describes the previous caption state.
- [RegisterShellHookWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registershellhookwindow):
  flash/activation notifications identify a window; API is not intended for
  general use and may change. Probe success does not select it for production.
