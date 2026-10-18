# macOS full world-state replies

## Goal and scope

Replace the macOS runtime's per-domain reply concatenation and cursors with the
core's complete world reports/replies, world revisions, and input-watermark
fences. Keep the legacy core domain API compiling without changing its behavior.
Do not expand native Space capabilities or adopt additional admission policy.

Preserve AppKit/engine/AX ownership, per-app generations, latest-wins frame
slots, observed-focus authority, consume-first input, drag diagnostics, active
border, menu-bar indicator, and the existing select-only Space fallback.

## Approach and decisions

- Report observed Spaces using per-output-local mode, native topology and native
  domain keys, without invented logical creation or ordinary window transfers.
  Preserve the single-display select-only boundary and exactly the existing 18
  bindings; unsupported operations remain capability-refused.
- Use `WorldEngine::new`; its maximized-focus option is inert because macOS
  reports `maximized: false` rather than inferring maximization. Pass the existing
  fixed-size predicate through each report.
- Publish complete lifetime, completeness, minimized, hold, and per-property
  feedback facts. Hidden/unreadable scopes must not acquire write authority.
- Apply only current-generation, ready, newer-world-revision replies whose input
  watermark still matches. Spend one-shot effects before dispatch and preserve
  existing generation/context fences at native actuation.
- Share the input clock with submitters; invalidate before enqueueing native
  input, not merely when the engine dequeues it. Stale commands remain consumed
  at most once while observation-only catch-up repairs desired state.
- Carry the drag-preview insertion neighbor in the complete core reply rather
  than substituting the dragged window; preserve the existing overlay contract.

## Bounded work

1. Add and test the report builder and sequencing primitives without switching
   native actuation. Establish the report/reply boundary with offline fixtures.
2. Adopt world replies for reconciliation and commands, remove macOS domain-reply
   aggregation/cursors, and split runtime responsibilities where necessary.
3. Review the migration independently; resolve concrete findings and perform
   the native offline gates.
4. Retain this note until the pending scoped live procedure below passes;
   then record its raw evidence and archive the note.

## Acceptance and verification

- Offline coverage for bootstrap/order, generation/revision/watermark rejection,
  stale replies, explicit holds, per-property feedback, and Space refusals.
- Preserve all existing macOS tests; strict clippy and formatting pass.
- Build and verify the development bundle, then run the repository verification
  gate after the final relevant code change.
- Scoped live evidence: owned-window admission, directional focus/move, float,
  close reflow, select-only workspace switching, border, and indicator updates.
  Preserve raw protected-window before/after records rather than counts alone.
- No new unsafe code outside `ffi.rs`; baseline is 114 unsafe blocks, all there.
  Keep touched production modules near 1,500 lines and tests near 3,000 lines.

## Evidence and outcome

Initial inspection: core/KDE world-state integration and best-effort admission
policy contract are present. macOS has no world-state route yet. Existing Space
catalogue and tests establish 18 implemented select bindings; no new binding is
needed. No core contract blocker was identified.

The additive report/sequencer boundary is implemented, without changing native
actuation. Its focused verification passed: 747 macOS library tests and strict
macOS clippy. Report fixtures cover unreadable/minimized lifetimes, completeness,
feedback, and observed focus; sequencing fixtures cover bootstrap, generation,
revision, current-input fencing, and spend-before-dispatch effects. Adoption must
retain session-derived wire flags and use raw work-area bounds with the core's
single outer-gap inset; regression coverage must prevent mixed geometry paths.

The runtime now uses one world reply instead of domain-reply aggregation.
Independent review identified missing arrival-time invalidation, preview-neighbor
loss at the world boundary, and unnecessary ordinary-write feedback tracking.
Corrections add the shared clock, deterministic mid-flight stale-reply coverage,
the preview neighbor fact, operation-only write correlations, and honest select
feedback fixtures. macOS tests passed four consecutive parallel runs; core and
protocol suites also passed.

## Freshness defect resolved

Two earlier approaches failed the same invariant: dequeue-only invalidation
missed concurrent arrivals; arrival-clock stamping then allowed an immediate
catch-up to label retained old observations with a newer queued input's mark.
Passing eventual-convergence tests did not establish absence of stale setters.

The final implementation keeps submitted invalidation separate from proof of
incorporated observation state. `world_dispatch.rs` removes unconditional stale
catch-up. After reply acceptance, before spending effects or emitting setters,
`emission_coherent` revalidates the current watermark, topology revision, and
absence of queued observations/terminations that would incorporate new truth.
The queue classifier uses the same lifetime/sequence/context fences as dequeue.
Queued obsolete jobs do not block progress. An observation-only retry is bounded
to one attempt and is fully re-gated; it never replays a command. Arrival-time
invalidation remains before enqueue, including saturated-queue failures.

Deterministic `runtime_freshness` regressions forbid intermediate stale setters
with multiple queued observations, post-accept arrivals, stale queued drops,
mark-without-job recovery and saturation. Negative controls disabling the fence
failed the expected regressions. Command effects remain at-most-once.

Independent review also found and resolved the preview wire fixture mismatch,
an automatic admission-focus regression, unbound select identities, and paired
focus-test interference. Native focus remains explicit-command-only, correlated
to that command's exact effect identity; observation-only admission does not
steal focus. The new admission regression failed with that gate disabled.

## Shared-core scope

- Add `CommandResult::DragPreview.target`, populated from the insertion neighbor,
  encode it as additive `win`, update the conservative size estimate and pinned
  protocol fixture, and assert the identity. This preserves the macOS overlay's
  existing neighbor contract rather than substituting the dragged window.
- Add `OneShotEffect::explicit_focus` / `is_explicit_focus_for`, retaining exactly
  the existing effect IDs and core behavior. This lets the adapter correlate
  explicit focus without a prefix heuristic or dispatching admission focus.
- No legacy domain behavior or Space capability expansion. Changes were
  reconciled with the current shared admission fields before verification.

## Final offline evidence

- `mise exec -- just app 'OmniTiler Mac Dev'` and `mise exec -- just verify` pass;
  both recipes were checked to build/sign/verify only, without launching.
  Development signing verifies; pre-notarization `spctl` rejection is expected.
- macOS: 897 tests pass (755 library, 142 integration), including 38
  `runtime_phase1` and 7 `runtime_freshness` tests after the final test split.
  Four consecutive full runs passed before the mechanical splits; full and
  repeated focused suites passed afterwards.
- Core: 390 library tests plus all integration suites pass. Protocol: 155
  library tests plus all integration suites, including 23 world-codec tests,
  pass. Host-side Windows tests pass (488 library plus integrations); this is
  not native Windows verification.
- Strict clippy for macOS/core/protocol and `cargo fmt --all -- --check` pass.
  Final test organization also passed declared-toolchain clippy and formatting.
- Unsafe remains 114 blocks, all in unchanged `ffi.rs`; no unsafe added.
- `runtime.rs` is now 1,535 lines after natural lifecycle/shared-state splits
  into `runtime_apps.rs` (430) and `runtime_shared.rs` (320).
  `world_dispatch.rs` is 1,205 lines; `runtime_phase1` is 2,897 and
  `runtime_freshness` is 1,400, within the approximate size guidelines.
- KDE's TypeScript suite was not run because its dependencies are unavailable;
  static review found named-field parsing tolerant of the additive `win` key.
  Native Windows verification remains external. No dependencies were installed.

## Pending live verification

No live smoke was performed. Offline delivery does not establish desktop,
physical-input, border or indicator acceptance. Keep the backlog item pending
and this record active until the following procedure passes under
[the live-testing guide](../live-macos-testing.md).

1. Before each leg check the screen is unlocked and Secure Input is inactive;
   stop on either condition or uncertain identity. Protect the hosting terminal
   absolutely: no input, movement, resize, hiding, minimization, closure or Space
   transfer. Keep a finite `caffeinate -d` child bound to its own exact PID and
   reap it after the leg. Never use unscoped `just dev` or process-name kills.
2. Build/verify as above, then run `mise exec -- just preflight-app`; require
   `ax_trusted:true`. Capture source/artifact/signing identity. Use three
   disposable TextEdit windows on one existing Desktop, no ordinary documents.
   Scope only `com.apple.TextEdit`; establish exact baseline geometry, IDs,
   visibility, Space membership and focus, plus the exact-owner stop/restore
   routes required by the live guide.
3. Before tiling, retain the raw JSON from
   `mise exec -- just launch -- --timeout 60 -- cg-inspect` and
   `mise exec -- just launch -- --timeout 70 -- scope-inspect --scope com.apple.TextEdit`.
   Keep raw protected-window records, not merely unchanged counts.
4. Run `mise exec -- just tile --user-start --scope com.apple.TextEdit --input tap --preset authentic --seconds 120`.
   Border and workspace indicator are enabled by default. Confirm three-window
   admission with observed AX/CG frames matching the desired plan.
5. Physically focus the middle tile; press Ctrl+H then Ctrl+L, waiting for each
   observed focus change. Expect left neighbor then original window, unchanged
   frames. Press Ctrl+Shift+L then Ctrl+Shift+H, waiting for each observed swap
   and sibling reflow; focus remains on the moved window. Press Ctrl+G twice:
   float with sibling reflow, then re-admission. Record actual native readbacks
   and corresponding runtime results, not command return values alone.
6. With a second existing Desktop available, press Ctrl+2 then Ctrl+1 and verify
   arrival and return, the current indicator row and retained layouts. This must
   exercise runtime select bindings, not a standalone Space probe. No ordinary
   window transfer, Space creation or capability expansion is part of this leg.
7. Close one disposable window while TextEdit remains alive; verify close reflow
   and no ghost target. Focus each survivor: one active border follows the true
   focused frame, and indicator counts/current row update. Confirm border
   removal and `shutdown_clean` after the finite run.
8. Retain raw after-records using the same read-only commands; compare protected
   window identity/geometry/visibility/membership against the raw baseline and
   record any differences. Restore only the owned preimages and close only the
   disposable documents. Stop only owned processes by exact verified PID; leave
   no probes, taps, helpers, overlays or caffeinate children running.

Remaining limits: native gesture barriers retain the adapter's existing absent
gesture state; skipped-display parking has legacy fixture coverage but no new
world-route-specific native evidence. Neither is claimed as new capability.
