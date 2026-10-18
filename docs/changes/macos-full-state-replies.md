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
4. Retain this note until the remaining scoped live procedure below passes;
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
- Live close testing additionally exposed a shared convergence defect in
  `session/world.rs`: write-once focus-stack seeding omitted later admissions.
  Closing the first-admitted focused window could exhaust fallback history,
  produce `FocusMismatch`, and hold the surviving scope unreadable indefinitely.
  Append missing admitted/restored leaves without duplicates or reordering
  retained history. This minimal repair preserves existing MRU fallback intent,
  observed-focus authority and strict focus binding; it does not broaden
  admission policy. Two core and four macOS regressions cover the exact failure,
  all close positions, the two-app control and named hold diagnostics. Negative
  controls reproduce the live first-close failure. Independent review accepted
  the correction; the sequencing/emission fence is unchanged.

## Final offline evidence

- `mise exec -- just app 'OmniTiler Mac Dev'` and `mise exec -- just verify` pass;
  both recipes were checked to build/sign/verify only, without launching.
  Development signing verifies; pre-notarization `spctl` rejection is expected.
- macOS: 897 tests pass (755 library, 142 integration), including 38
  `runtime_phase1` and 7 `runtime_freshness` tests after the final test split.
  Four consecutive full runs passed before the mechanical splits; full and
  repeated focused suites passed afterwards.
- After the close-reflow correction: 901 macOS tests pass, along with complete
  core, protocol and host-side Windows suites, strict clippy and formatting.
  The signed bundle build/verify and focused three-app/two-app live close tests
  were repeated against that correction.
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

## Live evidence and remaining verification

Scoped owned-helper smoke ran through the signed LaunchServices route with
`ax_trusted:true`, lock/Secure Input gates and exact-child cleanup. Source was
`182ba9a6`; close re-verification additionally included the core MRU repair,
held-scope diagnostics and their two regression files. Final tested executable
SHA-256 values:

- Probe: `be5b5ab61921cf7b18f0bf0fdf7754761b8aae33c2e8b2f580936779def36525`.
- Helper: `9f03a5192513f3d12c4e4b2fee228318d745cf1383ff0e5aa28da900979a1fee`.
- Bundle IDs: `io.github.omnitiler.macos.probe` and
  `io.github.omnitiler.macos.probe.helper`; signing identity `OmniTiler Mac Dev`,
  designated-requirement leaf `040f96377aaf110a77905d453bb76681e7133319`.

| Leg | Native evidence / outcome |
| --- | --- |
| Admission | Three owned apps: AX frames exactly matched CG and desired tiled frames; three frame writes, clean stop. |
| Directional focus | Rightward focus arrived at the intended owned window by SLPS/per-app AX/CG oracle; border followed. Leftward edge input correctly did nothing. One post's key-up oracle refused after focus had already changed; no complete physical key-pair acceptance is claimed. |
| Move | Right/left moves replanned sibling frames and retained true focus; both key edges posted for the tested move pairs. |
| Float | Float preserved that window's frame while siblings filled; unfloat restored admission geometry. |
| Close reflow | Initially failed when the focused first admission closed with its app alive. After the MRU repair, the same three-app shape emitted two frames, the split-column survivor expanded to a full column, and AX equaled CG. Two-app control expanded its survivor to the full work area. No held-scope lines; both stopped cleanly. |
| Active border | Native owned overlay presence/frame/bleed followed observed focus, with honest no-focus hiding and teardown. Pixel/color appearance not observed. |
| Indicator | Rows/current/count state advanced from unknown to occupied; status readback and runtime counters agreed. Visual strip legibility not observed. |
| Workspace switch | Runtime tap-to-engine select remains unverified: `owned-post` has no select chord family. A separate native Mission Control lifecycle created, selected, returned and removed a temporary Space; it is not runtime-hotkey acceptance. |

All owned helper/probe/tap/overlay children were stopped and absence checked;
each test-owned bounded caffeinate child was killed/reaped by exact PID.
Pre-existing processes were left alone. No Ghostty keys, mouse events or window
writes were issued. During the separate Mission Control leg, its final guard
observed external Ghostty frame drift and stopped; no further live work followed.
The temporary Space was removed, original current Space restored, Mission
Control absent, and an independent final record showed the original terminal
frame again. Do not treat this interrupted leg as a protected-window non-change
pass or as runtime select acceptance.

### Raw protected-window records

Title-free protected-terminal records below are the actual before/final arrays
for both close-fix legs (KF3 three-app and KC2 two-app). Full CG/AX snapshots were
retained separately; comparisons here cover these protected records, not ambient
offscreen menu-extra churn. Resource IDs are scoped evidence, not ownership
authority for future tests.

```json
{"before":[
 {"pid":14379,"wid":273,"layer":0,"onscreen":false,"bounds":{"x":0.0,"y":760.0,"w":64.0,"h":64.0}},
 {"pid":14379,"wid":396,"layer":0,"onscreen":true,"bounds":{"x":0.0,"y":30.0,"w":1150.0,"h":1139.0}},
 {"pid":14379,"wid":530,"layer":0,"onscreen":false,"bounds":{"x":8.0,"y":34.0,"w":2220.0,"h":1134.0}},
 {"pid":14379,"wid":1621,"layer":0,"onscreen":false,"bounds":{"x":1123.0,"y":603.0,"w":1108.0,"h":558.0}},
 {"pid":14379,"wid":1627,"layer":103,"onscreen":false,"bounds":{"x":320.0,"y":106.0,"w":254.0,"h":19.0}},
 {"pid":14379,"wid":3425,"layer":0,"onscreen":false,"bounds":{"x":0.0,"y":0.0,"w":2240.0,"h":30.0}},
 {"pid":14379,"wid":3428,"layer":0,"onscreen":false,"bounds":{"x":0.0,"y":0.0,"w":2240.0,"h":30.0}},
 {"pid":14379,"wid":3431,"layer":0,"onscreen":false,"bounds":{"x":0.0,"y":0.0,"w":2240.0,"h":30.0}},
 {"pid":14379,"wid":3434,"layer":0,"onscreen":false,"bounds":{"x":0.0,"y":0.0,"w":2240.0,"h":30.0}}
],"after":[
 {"pid":14379,"wid":273,"layer":0,"onscreen":false,"bounds":{"x":0.0,"y":760.0,"w":64.0,"h":64.0}},
 {"pid":14379,"wid":396,"layer":0,"onscreen":true,"bounds":{"x":0.0,"y":30.0,"w":1150.0,"h":1139.0}},
 {"pid":14379,"wid":530,"layer":0,"onscreen":false,"bounds":{"x":8.0,"y":34.0,"w":2220.0,"h":1134.0}},
 {"pid":14379,"wid":1621,"layer":0,"onscreen":false,"bounds":{"x":1123.0,"y":603.0,"w":1108.0,"h":558.0}},
 {"pid":14379,"wid":1627,"layer":103,"onscreen":false,"bounds":{"x":320.0,"y":106.0,"w":254.0,"h":19.0}},
 {"pid":14379,"wid":3425,"layer":0,"onscreen":false,"bounds":{"x":0.0,"y":0.0,"w":2240.0,"h":30.0}},
 {"pid":14379,"wid":3428,"layer":0,"onscreen":false,"bounds":{"x":0.0,"y":0.0,"w":2240.0,"h":30.0}},
 {"pid":14379,"wid":3431,"layer":0,"onscreen":false,"bounds":{"x":0.0,"y":0.0,"w":2240.0,"h":30.0}},
 {"pid":14379,"wid":3434,"layer":0,"onscreen":false,"bounds":{"x":0.0,"y":0.0,"w":2240.0,"h":30.0}}
]}
```

The interrupted separate Space leg recorded the following changed main surface
(the other eight protected records stayed equal). Its Space topology returned
from temporary Space 272 to the original `[1]`, current 1:

```json
{"wid":396,"pid":14379,"onscreen":true,"spaces":[1],
 "before":{"x":0.0,"y":30.0,"w":1150.0,"h":1139.0},
 "guard_after":{"x":56.0,"y":76.0,"w":1085.0,"h":1075.0},
 "independent_final":{"x":0.0,"y":30.0,"w":1150.0,"h":1139.0}}
```

### Exact remaining procedure

Keep this note active and the backlog item pending until runtime select and
visual checks pass under [the live-testing guide](../live-macos-testing.md).
No broader focus-None policy is selected; its discriminator remains TBD.

1. Establish a stable desktop, unlocked session and inactive Secure Input before
   each leg. Protect Ghostty absolutely, use an exact-PID bounded caffeinate
   child, and prove stop/restoration for the scoped resources. Stop on drift;
   never use unscoped `just dev` or process-name kills.
2. Build/verify and run `mise exec -- just preflight-app`, requiring
   `ax_trusted:true`. Use three disposable TextEdit documents only, with a second
   Desktop available. Record raw before CG/AX/membership using
   `mise exec -- just launch -- --timeout 60 -- cg-inspect` and
   `mise exec -- just launch -- --timeout 70 -- scope-inspect --scope com.apple.TextEdit`.
3. Run `mise exec -- just tile --user-start --scope com.apple.TextEdit --input tap --preset authentic --seconds 120`.
   Physically press Ctrl+2 and Ctrl+1, waiting for observed arrival and return.
   Verify current indicator row, retained layouts and runtime select results,
   not merely standalone native selection. No window transfers are included.
4. Focus each owned survivor and visually check exactly one active ring follows
   the true focused frame and the indicator is legible/current. Confirm border
   removal and `shutdown_clean` at the finite end.
5. Retain raw after records with the same commands; compare every protected
   record, restore only owned preimages and close only disposable documents.
   Leave no test-owned processes, taps, helpers, overlays or awake children.

Remaining limits: native gesture barriers retain the adapter's existing absent
gesture state; skipped-display parking has legacy fixture coverage but no new
world-route-specific native evidence. KDE TypeScript and native Windows gates
remain external. None is claimed as a new capability.
