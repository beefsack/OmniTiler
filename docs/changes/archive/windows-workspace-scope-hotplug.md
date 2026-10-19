# Windows workspace scope hotplug

## Goal and acceptance

- Complete Windows non-local workspace mode hotplug integration using the shared
  scope policy and Windows owner introduced in the preceding stages.
- Synchronize mappings, history and shared order on disconnect/reconnect while
  preserving sibling Engine domains, displaced members and in-flight state.
  Keep per-output-local behavior unchanged.
- Follow tentative KDE parity: shared mode does not displace projects; disconnect
  discards output-scoped history, retaining the single shared history; reconnect
  does not use history to select a workspace.
  Unsupported R-WS-17 outcomes remain TBD for physical testing.
- Keep orchestration in bounded scope modules and runtime call sites thin.

## Approach and verification

1. Implement scoped topology integration and simulated topology regressions.
2. Review topology/state lifetime invariants independently and resolve findings.
3. Run locked native build/test/strict clippy for core, protocol, KWin effect FFI
   and Windows, workspace format and diff checks.
4. Observe single-output select/previous/relative/send regression in each mode,
   verify graceful stop and exact saved-settings restoration.
5. Update parity/specification evidence and pending physical multi-output checks.

## Boundaries and decisions

- Reuse the existing tentative workspace decisions and KDE hotplug behavior.
- Physical two-output hotplug, mixed-DPI continuity and keyboard acceptance
  require separate user testing; nearest-survivor discrimination needs three
  outputs.

## Outcome and evidence

- Delivered 2026-10-15 on the stage B `e0cb522` baseline, following stage A
  `ecd183e` and owner integration `bba6317`.
- `sync_monitor_outputs` dispatches by adopted startup mode. Local retains its
  previous body unchanged. Global-unique retains displacement/return mechanics,
  discards removed-output scoped history, observes the survivor after selection,
  re-primes returning/new outputs without history-driven selection, and converges
  trailing cleanup with displaced/in-flight/unmapped-state protection.
- Shared disconnect converges missing logical ids before re-homing members onto
  the survivor, without a displaced association, project return mapping or Engine
  relocation. Reconnect uses the live shared order and survivor view. Both sibling
  output-bound layouts and actual pending/drag state survive; removed-output
  idle-empty Engine domains prune. Shared history remains scope-wide and records
  only observed view changes. Unsupported R-WS-17 outcomes remain TBD.
- Seventeen new simulated-topology tests cover each mode, removed versus surviving
  active output, sends/new members while absent, then-current displaced return,
  history discard/re-prime and conditional R-WS-17 second-toggle invalidation,
  shared no displacement and stable order, sibling layout retention, true pending
  proposal/drag retention versus idle pruning, divergent shared ids, and fresh
  output priming. Global foreground-removal tests inject an identified location
  into the production branch; native foreground reads/lifetime gates on physical
  removal remain unverified.
- Review found and corrected a cold shared-start regression, membership stranding
  on divergent ids and idle orphan-domain retention. The startup regression now
  drives the exact stage B sync-then-prime oracle from empty state, with stable-key
  and primary ordering deliberately different. Follow-up review accepted the
  corrections; no unresolved implementation finding remains.
- Physical line counts (including blanks): `tiling_sys_scope_hotplug.rs` 511,
  `tiling_sys_scope_hotplug_tests.rs` 948; both under `crates/tiler-windows/src/`.
  `workspace.rs` adds 137 lines (2,918 total) beside existing model mutation APIs;
  `tiling_sys.rs` adds 15 net lines (61,917 total), consisting of dispatcher,
  module declarations and startup documentation. Existing trailing helpers gain
  sibling-module visibility; `workspace_scope_owner.rs` changes documentation only.
- Locked native MSVC gates passed for `tiler-core`, `tiler-protocol`,
  `tiler-kwin-effect-ffi`, `tiler-windows`: build, tests, strict all-target clippy
  (`-D warnings`), workspace format and `git diff --check`, all through the
  declared tool route. Rust/cargo 1.99.0. Total: 2,095 passed / 0 failed / 1 ignored
  across 55 result suites, including doc tests; Windows library: 808 passed /
  1 ignored. This adds exactly 17 tests to the integrated stage B baseline.

## Single-output native evidence (2026-10-15)

- Fresh built artifacts were bound to the baseline plus this integration diff
  with SHA-256 and exact process/creation/session identities. Medium-integrity
  disposable helpers and an identified single-output baseline were used; only
  helpers entered management. Full provenance remains outside the repository.
- Independent recovery preflight observed visible -> hidden; verified owner exit
  alone left the helper hidden; independent restore revealed that same helper at
  its original rectangle, followed by exact-identity closure and clean ledger.
- Each adopted mode passed numbered selection, Previous twice, relative wrapping,
  numbered follow/stay sends and trailing selection. Follow exposed/focused the
  mover on its destination; stay retained the source view and hid the mover.
  Focus-only observations created no workspace event. Saved-mode drift reported
  restart-required while the adopted mode remained fixed.
- The reused shared relative/trailing-send fixture initially refused
  `view-changed`/`unmanaged` after activating a hidden helper on an emptied source.
  Those attempts are excluded from send evidence. One causal fixture repair
  established visible mover/current view/foreground before each request; the
  focused repeat passed relative follow, relative stay and trailing follow with
  native visibility, membership and foreground checks.
- Final `mise exec -- just stop` reported `stopped clean`; owner exited, zero helper
  actors, no live hooks/overlays, clean ledger/request state and byte-identical
  saved settings. Physical keyboard and multi-output outcomes are not established
  by these single-output CLI observations.

## Remaining physical journeys

- User-owned checks are listed in [multi-output pending checks](../../backlog.md#multi-output-setup):
  both-output isolated history/rings; global visible-elsewhere swap/back with
  distinct layouts, floats/overlays and focus; shared selection and follow/stay
  from each output with one history/trailing; visible/occupied cleanup; shared
  stable-key startup anchor; disconnect/reconnect with foreground on removed and
  survivor, sends/new contents while absent, history invalidation/re-prime,
  local/global return and shared no displacement, mixed-DPI geometry/focus and
  clean stop/restore. Nearest-survivor discrimination requires three outputs.
- User 2026-10-15 approved mode schema/adoption, global swap/addressing, shared
  no displacement and shared startup anchor in
  [Workspaces](../../decisions.md#workspaces). No additional product choice was
  introduced. Physical journeys and unsupported reference outcomes stay parked
  for that testing; offline item 12 implementation is complete.
