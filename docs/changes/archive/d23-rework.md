# D23 best-effort minimize restore and close cleanup

## Goal and acceptance

- Deliver the selected session-only restore: rejoin beside a surviving former
  neighbor, preserving side/group/index where possible and comparable old share.
- Resize, admission, other-sibling close, swap, other minimize/restore and
  gaps/work-area changes must not alone invalidate restore context.
- Fresh admission only when former neighbors are absent, the domain is gone,
  or the window was sent elsewhere. Restart discards restore context.
- Preserve minimize release/reflow/MRU, sole-minimize focus none and restore
  without a focus write. Clear stored slots when minimized windows close.
- Shared core first, then Linux planner/protocol and KDE adapter. Windows
  adapter delivery remains handoff item 22. Native acceptance remains pending.

## Pinned reference finding

COSMIC `3d55cba0` matches the selected best-effort approach; no material conflict:

| Concern | Pinned source | Finding |
| --- | --- | --- |
| Restore context | `src/shell/layout/tiling/mod.rs:343-349,1329-1346` | Stores parent or sibling anchor, orientation, index and sizes; two-child removal uses sibling because its group flattens. |
| Rejoin | `src/shell/layout/tiling/mod.rs:458-502,505-541` | Inserts into surviving parent at old index or recreates group beside surviving sibling. No whole-tree equality check. |
| Share and fallback | `src/shell/layout/tiling/mod.rs:467-487,544-545` | Stored sizes rescale in the matching-shape case; only anchor miss uses fresh admission. |
| Lifetime and focus | `src/shell/workspace.rs:109,1199-1252`; `src/shell/mod.rs:1511-1524` | Restore context is memory-only; tiled restore does not write the focus stack. |

Minor robustness difference: COSMIC stores one parent-or-sibling anchor. The
selected behavior permits recovery through surviving former neighbors when
that anchor disappears. Stored sizes are hints, not a requirement to replace
the current sibling vector after unrelated membership changes.

## Approach and bounded units

1. Shared restore context/rejoin and close cleanup, with portable regressions.
2. Small close-versus-minimize wire distinction, Linux planner and KDE adapter
   wiring, with real planner/entry regression coverage.
3. Review the cross-layer contract and run workspace tests, clippy with warnings
   denied, formatting, KWin tests, typecheck and bundle. Native CTest applies
   only if native code changes.
4. Update spec/evidence and handoff records; archive this note on completion.

Behavioral ambiguity receives a minimal discriminator in the reference matrix;
unsupported reference and native outcomes stay TBD. No settings or saved-slot
persistence are added.

- User-approved 2026-10-16 D23 item 6 process: source-slot lifetime after
  membership rehoming while minimized is NOT decided. Research Tier 1
  references first, then decide for consistency; earlier option B preference
  is context only. This delivery selected no source cleanup policy.

## Delivered outcome (2026-10-13)

- Shared core: exact restoration remains a fast path. Otherwise a surviving
  former group takes insertion at the stored index; missing/changed group
  regroups beside a live former neighbor on the stored side/axis. Current
  survivor proportions remain intact. Exact rational scaling preserves old
  share fractions even against small normalized current weights.
- Core/protocol: additive default-empty `closed_windows` reports verified
  closes. Successful convergence forgets reported slots before retirement,
  including slots minted by an ordinary close in that observation. Invalid
  reports do not mutate retained state.
- KDE: exact-ref close uses live-time homing, including hidden members;
  undelivered reports persist, delivered reports clear on matching planned
  replies, and successful work drains multiple domains through the existing
  refresh chain. Unreadable observations remain unknown; null sole-close
  cleanup requires prior applied-empty evidence.
- Linux planner uses the shared protocol/Engine route without planner-local
  policy. Windows changes initialize the additive field empty only;
  subsequent Windows adapter release/rejoin/close wiring delivered offline
  2026-10-13 ([record](windows-d23-restore-close.md)); native journeys remain
  [handoff item 22](../../backlog.md).

## Accepted evidence

| Gate | Result |
| --- | --- |
| `cargo test --workspace` | 57 suites, 1631 passed, 0 failed, 0 ignored |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| KWin `npm test` | 1425 tests, 210 suites, 0 failed |
| KWin `npm run typecheck` | Passed |
| KWin `npm run build` | Passed |
| Native CTest | Not applicable; native code untouched |

- `crates/tiler-core/tests/minimize_restore.rs`: 25 passing tests covering
  minimize/reflow/MRU/sole-none, no-focus restore, focus change, resize,
  newcomer, sibling close, swap, other minimize/restore, gaps/resolution,
  restart, all three fallback cases, close cleanup and divergence fencing.
- Core share fixtures cover skewed stored ratios, root regroup, surviving
  group/index, normalized-current-weight rational insertion and rejection of
  share vectors whose individual entries fit but whose group total overflows.
- `crates/tiler-protocol/tests/planner_close_cleanup.rs`: 7 passing tests for
  absent-field compatibility, id validation, sole retirement and rejected
  report retention followed by delivery.
- `kwin/tests/minimize-close-cleanup-entry.test.ts`: 3 production-entry/real
  planner fixtures for minimized close, sole close and newcomer-then-restore.
- `kwin/tests/plan-close-cleanup.test.ts`: 9 passing adapter fixtures for
  delivery, rejection retention, stale id reuse, applied-empty cleanup,
  unreadable evidence, hidden closes/homing and multiple-domain draining.
- Independent cross-layer review accepted after correcting hidden homing,
  close-drain and empty-evidence handling. Initial average-share and
  unit-rounded insertion were rejected during verification; stored rational
  shares now drive rejoin. Red/green fixtures discriminate those approaches.

## Remaining acceptance and limits

Native journeys remain user-owned and pending: minimize/reflow/MRU,
restore position/share/no-focus-write, sole minimized workspace retention,
intervening mutations, restart and minimized-window close. Exact native
outcomes and unsupported reference variants stay TBD in the
[matrix](../../spec/reference-outcomes/minimize.md#d23-restore-and-close-cleanup-discriminators-2026-10-13).
Transport failures retain close reports for later ordinary recovery/signals;
no poll or persistent restore store is added. Unrepresentable share products
use the existing fresh-admission fallback rather than corrupting tree shares.
