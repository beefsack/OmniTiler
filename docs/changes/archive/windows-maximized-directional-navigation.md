# Windows maximized directional navigation

## Goal and acceptance

- Handoff item 18, G-06 / R-MAX-08 / REQ-MAX-08: directional focus from a
  maximized non-fullscreen origin is a no-op. Directional move clears maximize
  once, then moves only after observed clear in the same invocation.
- Validate the exact lifetime, focused origin, home/displayed domain and
  suspension/elevation fences before clear; reobserve and validate after clear.
  Failed/unconfirmed clear has no structural move, delayed intent or retry.
- Cleared tiled subjects use the existing Engine move; cleared intentional or
  sticky floats use item 10's half-snap. Focus TO a maximum remains valid.
  Fullscreen/game safety, pointer refusals, admission and workspace sends retain
  their existing policies.

## Approach and bounded work

1. Guard directional focus using fresh retained overlay flags. Windows reconcile
   events omit maximize, so enabling the shared opt-in fence alone cannot enforce
   this policy; keep the native guard, without changing shared observation semantics.
2. Reuse retained-aware exact-subject construction and native revalidation,
   hold process identity across one nonactivating `restore_zoom_placement`, then
   route on a fresh observation only after all post-clear checks pass.
3. Cover focus/fullscreen precedence, pre-clear authorization and post-clear
   refusal policy offline; coordinate retained float targets with item 10.
4. Update linked records; verify actual focus/state/geometry timing natively.

## Decisions

- [G-06 selected policy](../../decisions.md#window-state-float-sticky-maximize-fullscreen)
  requires one attempt and observed-clear continuation, not API acceptance.
- Tentative 2026-10-12, pending user review: pre-clear refusal diagnostics use
  `fullscreen-raced`, `domain-raced`, `focus-raced`, `suspended`, then
  `elevated-foreground`, consistent with fullscreen precedence and existing
  toggle-gate vocabulary. Existing owned fullscreen metadata vetoes clear even
  during a non-covering transitional frame, preserving restore authority.

## Accepted evidence and outcome

- Delivered offline 2026-10-12 from base `e506350`, Windows adapter/tests only;
  no shared core/protocol or KDE code changes. Shared tiled move policy reused.
- Initial review found missing immediate lifetime/suspension fences before
  clear; corrected with held revalidation, focus/domain checks and a write-time
  veto. Follow-up inspection found the sticky displayed-view check missing
  after clear and a fresh fullscreen subject permitted by the focus-relaxed
  validator; corrected with displayed-view and explicit fresh-fullscreen/owned
  metadata fences before final gates. Failed authorizations do not clear.
- Native MSVC offline gates: rustc 1.99.0, locked four-package build/test,
  all-workspace formatting, strict four-package all-targets clippy and
  `git diff --check` pass. Combined items 10/18 add 19 offline regressions;
  pre/post-clear policy tests are not native routing/timing evidence.
- Native pending: all-direction maximum focus fence, isolated tiled move,
  maximized float half-snap, asynchronous/unconfirmed clear, focus/flag races,
  physical held repeats and preset suppression. Linux/CI gates pending
  publication. No native timing/game acceptance claimed.

## Next action

- Run remaining unconfirmed-clear/race, held-repeat and preset checks; ordinary
  maximized directional physical acceptance is recorded below.

## Native-session result - 2026-10-12

- Maximum focus fence and unmaximize-before-move NOT-RUN: no normal-owner
  directional CLI and injected keys are excluded by the product filter. Native
  R-MAX-08 remains TBD; the distinct B9 native-clear success does not establish
  directional routing, focus fence or half-snap behavior.
- Combined scoped session stopped and restored cleanly; owner absent, saved
  settings byte-identical, all test windows closed, non-test inventory unchanged
   and Terminal visible/unminimized. No physical input/game/DPI acceptance claimed.

## User-accepted physical evidence - 2026-10-12

- Scoped normal owner, single output, Notepad/Paint, physical keyboard/mouse;
  base `e506350` plus combined directional, retile-overlay and B9 Windows changes.
- Maximized tiled window keeps focus on Win+arrow. Maximized float with
  Win+Shift+arrow unmaximizes then snaps to left/top/right/bottom halves.
  Maximized tiled window with Win+Shift+arrow unmaximizes then moves;
  user accepts this behavior as sensible.
- Unconfirmed clear/flag races, exact timing, held repeats/preset variants,
  fixed-size, game/DPI/multi-output remain pending. Fullscreen transition
  post-clear veto is tightened by the later
  [fullscreen-exit regression record](windows-fullscreen-exit-acceptance.md);
  this later safety hunk has offline evidence only.
