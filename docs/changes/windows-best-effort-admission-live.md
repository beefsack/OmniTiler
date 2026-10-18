# Windows best-effort admission native follow-up

- Status: partial native verification; remaining discriminators blocked below.
- Prior evidence: [offline delivery and partial native checks](archive/windows-best-effort-admission.md#scoped-native-follow-up-2026-10-18).

## Goal and scope

- Complete the remaining runnable owned-helper native admission checks: fresh
  exhaustion/largest-splittable fallback, ordered shrink, retry, hidden repair,
  restart, same-output sends, overlap, and changed-policy Settings persistence.
- Preserve the accepted cross-output refusal policy. Check that route only if
  a second output is available. Physical shortcuts, games and drag acceptance
  remain outside these agent-driven journeys.
- Fix only reproduced, clear Windows adapter defects with regression coverage;
  leave product ambiguities unresolved rather than inventing behavior.

## Acceptance and approach

1. Follow the live Windows recovery/preflight contract, retaining exact identities,
   settings backup, desktop preimages and per-journey native evidence externally.
2. Establish foreground-qualified helper operations and owned Settings modal/save
   completion before dependent journeys. Bound each journey below helper expiry.
3. Distinguish requested operations from observed membership, markers and geometry;
   verify the previously exploratory one-pixel unfloat discrepancy if reproducible.
4. Restore settings byte-exactly, stop and independently restore the owner, verify
   taskbar/SPI state and no owned helper or recovery residue.
5. Run locked four-package build/test/strict all-target clippy, formatting and
   whitespace gates after any fix; independently review live-affecting changes.

## Work units and verification

- Foreground and Settings save/reopen proof, with cleanup.
- Separate exhaustion, admission-order/shrink/retry, hidden repair/restart,
  same-output sends and overlap journeys, with fresh helper lifetimes.
- Diagnose reproduced failures before selecting any Windows-only correction.
- Review accepted evidence, update the prior outcome table and backlog, and
  archive this note once the runnable scope is resolved.

## Decisions and evidence

- Foreground/Settings journey `be-fgset-20261018` at `4f5e7640`: verified six
  helper foreground transitions; changed both catalog policies through native
  controls, completed both owned Apply modals, confirmed saved keys/revision and
  reopened selections. Three helper frames/classifications were unchanged and
  helper focus returned after closing Settings. Helpers remained alive throughout.
- Native source `4f5e7640`; SHA-256 owner
  `3508F59DD61ECA1EE19180AE6F17711D38AE699E210175E62AE30862F44E97C7`,
  helper `43A7867EA7E03BA9B5240476AB8BD69BFFC282B512C18625B1D2CA4C0C4A7625`.
  Full identities, provenance, timestamps and pre/post observations remain in
  external groups `be-fgset-20261018`, `be-cap-20261018`, `be-capHR-20261018`,
  `be-rem-20261018` and `be-ovl-20261018`. Normal owner scope and disabled Snap
  takeover/prevention match the prior bounded native procedure.

| Check | Outcome | Accepted evidence / remaining discriminator |
| --- | --- | --- |
| Fresh exhaustion / largest fallback | Not-run to acceptance | Thirty fresh helpers at 64/64 still tiled. Reflow geometry was observed, but ordinary-anchor identity and a fallback wrap were not established; no fresh capacity float was observed. |
| Ordered shrink | Partial | Forty serially observed tiled admissions; 8/8 -> 32/32 floated exactly the newest 28 identities, then 64/64 the next two. All 30 markers read `ok`, victim frames were retained and survivors reflowed without a focus change. This proves newest-suffix victim sets, not intra-batch eviction order, shortest-prefix minimality or protected-member behavior. |
| Failed unfloat / successful retry | Not-run to acceptance | Clean foreground-qualified unfloat found a feasible tile, not no-space. Float-to-tile settlement reported `unfloat-mismatch` with a one-pixel planned/readback delta. Earlier center-titlebar clicks hit minimize/maximize buttons and contaminate those attempts; they are excluded. |
| Hidden repair | Partial, unresolved discrepancy | Forty helpers hidden at 8/8 took zero hidden geometry writes after 64/64 adoption; every hidden frame stayed unchanged. Reveal projected all 40 with zero capacity victims, unlike visible shrink's 30 victims. Actual select hints were 40/40 `[118,30]`, with no timeout or budget omission. No causal defect line is isolated. |
| Restart persistence | Pass, bounded | Thirty capacity floats retained frames after smaller gaps without automatic tiling; all 40 helpers remained alive across exact owner stop/restore/relaunch. New-owner observations cover all 40, with 30 `float-adopted` ordinary floats, clean marker health and preserved float frames. Tiled survivors changed slots; D5 does not promise tiled-history persistence. |
| Same-output capacity sends | Not-run to acceptance | Verified-foreground follow and stay requests still projected tiles; no `MoveToFloat` or capacity arrival proof. Default follow and no-search stay do not establish an exhausted destination. |
| Overlap | Partial | Fresh 40-tile 8/8 roster: policy-only adoption caused zero writes/reflow. Valid 64/64 adoption kept all 40 positive targets with zero capacity floats, unlike the default-policy shrink. Actual overlap/overflow, fresh no-space admission and navigation remain unproven. |
| Changed-policy Settings Apply/save/reopen | Pass, bounded | Both native policy selections persisted through two owned Apply modals and reopen; helper frames, classification and post-close focus unchanged. Full physical traversal/layout acceptance is separate. |
| Cross-output refusal | Not-run | One output only; accepted T2 refusal unchanged. |
| Cleanup | Pass | Exact settings-byte restoration, taskbar/SPI equality, no remaining owned processes or ledger, and final `mise exec -- just stop` confirmed for each completed journey. |

- Evidence corrections: adoption-log iteration is not eviction order; the ordered
  result above compares victim sets against all observed admission identities.
  Restart mapping must consume every entry in each coalesced visibility batch,
  not only its first entry. Settings/readback success alone is not no-space proof.
- An invalid 96/96 overlap-shrink fixture was rejected during review (maximum
  supported gaps are 64). Its result is excluded. One causal fixture correction
  used fresh helpers at 8/8 and a newly confirmed valid 64/64 adoption.
- No production change or new product decision was made. Independent review did
  not establish a clear Windows-only defect. A feasible-tile unfloat settlement
  reported `unfloat-mismatch`; the one-pixel planned/readback diagnostic in
  `be-rem-20261018` (`window-diag`, tick 275) still needs causal isolation.

## Blockers and exact next action

- Fresh, retry and send rosters did not produce the required no-space state;
  shrink victims are not proof that fresh/transfer admission cannot split.
  Do not repeat those same fixtures or expand window counts without a grounded
  exhaustion design. Protected-member and full overlap geometry legs remain open.
- Hidden select and visible shrink have identical loose-gap allocations and
  fresh hints, but this does not prove identical retained tree/share state.
  Both call the same Engine gap-update path, which can repair capacity.
  No missing-hint hypothesis or topology-preserving-helper comment explains the
  discrepancy. Do not change semantics without isolating the retained input.
- Next action: capture a retained-tree/session discriminator for visible and
  hidden gap adoption, with matched admission/topology preimages; compare the
  UpdateGaps outcomes before choosing any fix. Separately design a genuinely
  exhausted fresh/send/retry fixture and isolate the clean one-pixel readback.

## Final verification

- After upstream integration through `a914195a`, locked four-package build/test
  and strict all-target clippy passed: **2,852 passed, 0 failed, 1 ignored across
  71 suites**. Workspace formatting and whitespace checks passed. Rust/cargo
  1.99.0, native MSVC target. These are offline gates, not a re-run of the
  source-bound native observations above. No implementation files changed.
- Final independent `mise exec -- just stop` reported `stopped clean`.
  Readback confirmed no owner/helper processes, `ready=false`, no ledger,
  settings byte identity with the backup, and taskbar/SPI equality to preimages.
