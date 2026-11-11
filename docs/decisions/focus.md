# Focus

### Automatic focus guards
- Decision: core withholds automatic focus if newcomer focus would activate over a different focused floating/sticky window; automatic focus never targets fullscreen, sticky, floating or excluded rows except admission focus for ordinary float not fullscreen/maximized/sticky; newcomer focus never goes behind fullscreen/maximized sibling in same domain. Explicit focus and send-follow are exempt. Withheld effect is spent, not retried.
- Positive close of the observed focused lifetime freezes the pre-close source-domain tiled MRU survivor, excluding the close cohort, before native fallback convergence. Ordinary float close uses the same tiled policy. Existing automatic-focus, visibility, lifetime, fullscreen/game and shown-domain fences remain. Target equality satisfies without a setter; a different eligible same-report fallback or positively known no-client permits one bounded correction. Unknown foreground forbids it. A later observed user focus change from the close-report baseline supersedes, except target equality satisfies first; a newer focus decision or a view change or target loss making the target ineligible also supersedes, while non-focus commands never cancel pending focus. No unrelated host-loss repair, alternate candidate, retry, native clear or view switch.
- Refocus is one global retained request for every cause (explicit-command, automatic-admission, focus-follows-cursor, positive-close), not a one-shot: stable id/target/issuing revision until satisfied, superseded, ineligible or refused. Same-report candidates resolve explicit-command, then automatic-admission, then positive-close, then focus-follows-cursor. Non-focus commands never cancel pending focus in any batch, including refused commands without a focus decision; only a newer focus decision of any cause, an observed user focus change, a view change or target loss making the target ineligible supersedes. This amends the earlier rule under which newer commands superseded. Each bound adapter spends the id before one attempt or safety no-call, even when issuing/intermediate publications are skipped or feedback is lost. Accepted-but-unchanged focus may retain an inert row, never another call. Each fresh qualifying hover entry after leaving the live target and returning mints a new id; unchanged reports or pointer movement over the already-pending target keep the id and never retry. Same-report attribution deliberately permits one close-gap click override and cannot see entirely between-capture A->B->A flips; no input hook or causal guard is added.
- Why: one cross-platform rule replaces KDE adapter guards; follows D01 and Gaming Compatibility.
- Rejected: KDE-only adapter guards; moving only D01 fence.
- Assumes / revisit if: hosts report maximize/fullscreen truthfully; Windows maximize reporting and Windows/macOS current behavior unchecked.
- Status: active, 2026-10-30; generalized to one retained request for every cause 2026-11-11 (approved, not yet implemented)
- Evidence: [review R5, F10](../changes/core-interface-review.md); [design](../changes/retained-focus-requests.md).

### Workspace active-window memory
- Decision: core remembers lifetime-bound floating, sticky and tiled identities in one workspace slot. Restore eligible remembered target first without float/tile preference, then existing tiled MRU/layout fallback. Accepted focus choices, including admission newcomers, record before native effects; exception observations use tiled-observation safeguards. Sticky focus records only in workspace selected/observed focused, not every view where visible.
- Positive-close selection freezes tiled MRU before ingesting host fallback; accepted logical selection never fabricates observed/native foreground. Workspace-return float/sticky memory does not expand close candidate eligibility.
- Why: not recorded.
- Assumes / revisit if: unattributed foreground changes may replace intended memory even after unfinished arrival; this residual remains with unknown-foreground policy. Windows dispatching sticky away from home requires fresh lifetime/visibility proof and controlled home/shown scopes on same output; native homing/foreground vetoes remain authoritative.
- Status: active, 2026-10-28
- Evidence: [outcome](../changes/archive/windows-floating-return-focus.md).

### Command-owned linked-focus recording
- Decision: reuse existing session-binding gates; if recording returns false, preserve accepted effect without retry or promised repair. Observation/departure ingestion stays unchanged.
- Why: not recorded.
- Assumes / revisit if: existing binding gates remain sufficient.
- Status: active, 2026-10-28
- Evidence: [record](../changes/windows-workspace-return-focus.md#command-owned-memory-outcome-2026-10-28).

### Cross-output directional focus
- Decision: crossing output boundary selects a destination linked member nearest the crossed edge, not destination MRU; shared-core policy on all adapters, no setting. Candidates include nested leaves touching the canonical projected entry edge; most recently focused wins. Without history, greatest overlap against source perpendicular span, then DFS child order. Never use observed frames. Fullscreen and maximized destination members are eligible and retain their overlay state; they rank by retained allocation. Eligibility/safety gates are unchanged. Destination internal tiled last-active then focus stack newest-first; only currently linked/homed candidates; stale closed/floated/moved entries miss. Admission/convergence seed history. Preserve workspace-return lifetime slot.
- Why: output crossing should reach an eligible overlay member without unmaximizing or exiting it.
- Rejected: excluding fullscreen/maximized destination members or changing their overlay state on focus.
- Assumes / revisit if: no-focus origin uses source output usable-area span; all four crossing directions retained, focused tiled crossing horizontal-only. Float-origin stays local; empty/ineligible destination no-op without clearing state. Internal float-crossing remembered-target proposal is not newly enabled.
- Status: active, 2026-11-01
- Evidence: [offline evidence](../changes/archive/cross-output-edge-focus.md), [discriminator](../spec/reference-outcomes.md#cross-output-focus-edge-discriminator), [R-OUT-11](../spec/reference-outcomes/multi-output.md#r-out-11-directional-focus-between-fullscreen-or-maximized-outputs) sway destination-fullscreen target; Ours KDE/Windows selected outcome, [REQ-FOC-07](../spec/functional-spec.md#focus) and [macOS port note](../porting/macos-port-notes.md#approved-core-changes-and-deferred-questions).
