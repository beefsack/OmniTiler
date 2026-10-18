# Windows best-effort admission adoption

- Status: delivered offline; partial scoped native evidence added 2026-10-18.
  Full native acceptance remains pending below.

## Goal and scope

- Adopt the selected [D1-D10 capacity behavior](best-effort-admission.md) in
  the Windows legacy-domain runtime, matching KDE policy with native safety.
- Dependency audit: the existing Engine path supports partial seed rosters,
  capacity exceptions, newest-first repair, overlap and `MoveToFloat` replies.
  Session diagnostics expose committed capacity identities. The parked
  [world-state migration](../windows-full-world-state-replies.md) is not required.
- Windows-only adapter changes; no shared semantics, KDE/macOS changes,
  dependencies, new polling or recovery ownership. Preserve games, overlays,
  lifetime fencing, D16/D23 and the recovery ledger.

## Acceptance and approach

1. Persist committed capacity floats through ordinary float markers and retained
   live frames, including fresh/seed admission and shrink victims. Never act on
   rejected, staged-only or native-unproven transfers.
2. Consume same-output workspace capacity-float results through existing
   transfer/arrival-proof lanes; refuse cross-output capacity-float transfers
   without an established safe frame remap. Failed tile retry stays floating.
3. Expose both global catalog settings in the existing Windows tabs/storage;
   defaults remain largest-splittable/float, Apply affects later decisions only.
4. Add producer-shaped offline regressions, independent safety/ordering review,
   locked four-package build/test/strict all-target clippy, formatting and
   whitespace gates. Verify hosted CI after publication.
5. Record precise pending native checks below. Native execution is not part of
   the delivered offline evidence; physical-input and multi-output acceptance
   remain distinct.

## Evidence and status

- Source audit accepted: `tiling_sys/admission.rs` dispatches the legacy Engine;
  core reconcile/gap repair commits capacity victims but Projection omits their
  identities. `SendWorkspacePlan` accepts `MoveToFloat`. Neither the parked
  world migration nor a shared API/semantic change is needed.
- KDE consumes complete world rows and settles ordinary float intent for user
  and capacity origins alike. Windows consumes committed Session facts and
  reuses its ordinary-float marker rather than adopting world replies.
- Delivered: ordinary float lanes/frame retention and marker persistence for
  fresh/seed capacity admissions, newest-first shrink victims and hidden/select
  convergence; event-driven missing-marker repair; same-output follow/stay
  `MoveToFloat` handling; explicit tile retries retain float intent on no-space.
- Both shared catalog fields now map to additive schema-v1 native keys
  `core.anchor_fallback` and `core.no_space_result`. Native tabs derive options,
  labels, help and grouping from the catalog. Startup/live adoption preserves
  defaults and strict invalid/last-good handling; policy-only Apply does not
  reflow or retry settled windows. Overlap plans retain positive geometry.
- Independent safety/ordering review accepted after correcting selection and
  hidden-domain registration plus event-driven marker recovery. Offline
  fixtures cover ordinary-marker restart re-adoption (capacity reason may be
  lost, floating/frame/survivor geometry remain), staged/rejected/foreign
  membership, stale tags/PIDs, protected fullscreen, newest-first repair,
  unfloat no-space, transfer refusal, overlap and settings timing.
- Locked native four-package build/test and strict all-target clippy pass after
  integrating the upstream cross-domain admission-age correction:
  **2,750 passed, 0 failed, 1 ignored across 68 suites** (Windows library:
  **1,167 passed, 1 ignored**). Workspace formatting, whitespace and changed-code
  unsafe checks pass; no new unsafe code, dependencies or other-host edits.
- At offline delivery, no live tiler run, test-app launch, injected input,
  taskbar change or overlay was performed. Cargo fixtures are offline evidence,
  not desktop acceptance; the later partial native evidence is separate below.
- Initial hosted CI at `18b69b61` passed seven jobs; Windows reported three
  failures in the new scripted marker fixtures. Their fake held gate consumed
  the test runner's real integrity instead of the scripted target's fact.
  The narrow fixture correction scripts the native RID and retains the same
  medium-integrity predicate, with an explicit high-integrity no-write negative.
  Production gates and all original assertions are unchanged. The four-package
  gates above pass again after this correction. Implementation `18b69b61` and
  fixture correction `6c4bcefd` are published; [CI 39083833668](https://github.com/beefsack/OmniTiler/actions/runs/39083833668)
  passes all eight jobs, including native Windows/macOS, Rust/KWin, shell,
  native effect/settings, licenses and notices.

## Host differences and bounded limitations

- Shared policy/defaults match KDE. Windows retains its native game, overlay,
  lifetime, recovery and D16/D23 fences rather than importing KDE actuation.
- Both explicit output sends and R4 capacity-float results refuse
  `float-no-remap`, preserving source Engine/membership. Windows has no
  established move-only float frame remap/arrival proof for this route.
  User accepted 2026-10-18 (T2): retain this refusal until a remap rule is
  separately approved. This is the accepted operation boundary, not a silent
  success or rejection of existing whole-workspace migration remaps.
  [Decision](../../decisions.md#best-effort-admission).
- Drag capacity floating remains an honest refusal/snap-back until preview can
  represent that fallback. Overlap preview/drop parity still needs native proof.
- Pure route gap synchronization remains Engine-only. Any committed D7 victims
  are excluded from tiled writes immediately; float lanes/markers settle on the
  next ordinary reconcile/select. A targeted regression pins this backstop.
  Marker installation remains identity-gated, best-effort like ordinary float
  intent; missing values repair on later events without polling or a ledger.

## Scoped native follow-up, 2026-10-18

- Native desktop, disposable identity-tagged helpers only; normal owners used
  `tile --user-start --scope-exe tiler-test-window.exe`, with keyboard Snap
  takeover and mouse Snap prevention disabled for these journeys. Separate
  lifecycle/proof runs are not used as admission-policy acceptance.
- Source `76a34040`, clean tree. SHA-256: owner
  `C09D5917E7023EE2EBF294B37B193A877B6B7ECEB8F5D52DFE14D1A5ABB27054`;
  helper `42D0440C3F28B0A725FF34B0206F954F9A0DCC5DCAFDC6263012B12AE8B89887`.
  Exact process/start/session/integrity identities, display/window preimages,
  logs and rectangle snapshots remain in external scoped evidence, journey
  groups `be-admit-20261018` and `be-admit2-20261018`.
- Repeated locked four-package build/test and strict all-target clippy passed:
  **2,750 passed, 0 failed, 1 ignored** at the live source. After integrating
  upstream through `8b05a888`, all gates passed again: **2,753 passed, 0 failed,
  1 ignored**. Workspace formatting and whitespace checks passed. Rust/cargo
  1.99.0, native MSVC target. The later gates are offline-only; native observations
  remain bound to `76a34040` and do not close the unverified native legs.

| Check | Outcome | Accepted observation or remaining gap |
| --- | --- | --- |
| Exhaust a domain by gap growth; default capacity float | Pass, bounded | `d-gapshrink`: 220 admitted at gaps 8/8, then file-route 64/64 produced 210 capacity floats and 10 reflowed survivors. `f-shrink120`: 120 admitted, then the same change produced 110 capacity floats and 10 reflowed survivors. All 320 float-marker readbacks were `ok`; every victim retained its pre-repair frame. |
| Fresh exhaustion; largest-splittable fallback | Not-run to acceptance | Fresh batches did not reach geometric exhaustion. Minimum-size hints are not the positive-projection admission threshold; those batches do not prove fallback or fresh capacity floating. |
| Newest-first shortest eviction prefix; protected members | Partial | Capacity repair, victim-frame preservation and survivor reflow passed. Successful admission order was not captured; marker-log order is unordered membership iteration, not eviction order. Newest-first/minimal-prefix and protected-member claims remain offline-only. |
| Failed unfloat retention; later successful retry; no auto-tile | Not-run to acceptance | Foreground-qualified retry was not established under exhaustion. An exploratory unfloat reported a one-pixel native readback mismatch; neither that result nor unmanaged/no-op CLI replies proves this check. |
| Hidden-domain repair | Partial | A hidden/select roundtrip had no hidden rectangle changes. It did not cause capacity repair while hidden, so selection-time victim registration remains unverified. |
| Restart capacity floats as ordinary floats | Not-run to acceptance | Surviving helper frames matched, but the new owner's observed roster was empty after helper expiry/identity rejection. Frame equality alone is not re-adoption proof. |
| Same-output capacity sends, follow and stay | Not-run to acceptance | No accepted capacity-float send/arrival proof was captured; requests without managed foreground do not count. |
| Allow-overlap, fresh admission and real shrink | Not-run | No accepted live overlap-policy journey; no preview/drop or navigation claim. |
| Catalog controls, choices and help | Partial | Native accessibility enumeration exposed both admission groups, largest/closest/no-search and float/allow-overlap choices, four tabs and no-reflow help. Full traversal/layout and save/reopen were not established. |
| Changed policy Apply, save/reopen and no reflow | Not-run to acceptance | Apply invocation did not produce a verified save. Unchanged frames after a zero-edit invocation do not prove changed-policy Apply. File-route gap adoption did work; it is not UI persistence evidence. |
| Cross-output capacity-float refusal (T2) | Not-run | Only one output was available; accepted refusal policy is unchanged. |
| Cleanup and exact restoration | Pass | Verified owner stop plus independent restore, final `mise exec -- just stop`, no remaining owner/helper processes or recovery requests/ledger. Original settings bytes matched the backup exactly. Valid taskbar and Mouse Snap/SPI pre/post readbacks matched. |

- Review found no established production defect. Apparent unexplained workspace
  selections were attributable to test-script CLI calls. A foreground change
  without a recorded target identity is excluded from focus evidence. Helpers
  are not permanently non-activating: no-activate show/move flags preserve focus
  for those calls only. Settings Apply can present owned conflict/result modals;
  invocation without confirmed modal completion/save is not acceptance.
- Procedure limits: the helper lifetime is bounded to 600 seconds; expiry
  invalidated the restart leg. Stop repeated focus/Apply attempts rather than
  infer a product defect or weaken native identity/input guards. No production
  fix, fixture change, dependency change or new product decision was made.
- Physical shortcuts, games, drag preview/drop, broader application behavior
  and multi-output acceptance are not implied. Remaining checks stay open.

## Additional scoped native evidence, 2026-10-18

- [Follow-up matrix and unresolved discriminators](../windows-best-effort-admission-live.md):
  native changed-policy Apply/save/reopen and capacity-float restart persistence
  passed with live roster proof and exact cleanup. Ordered shrink matched newest
  victim suffixes in two steps; minimal-prefix and protected-member acceptance
  remain open. Valid overlap-policy shrink preserved a positive 40-tile roster
  without capacity floats; geometric overflow and fresh exhaustion remain open.
- Hidden-gap adoption preserved all hidden frames, but reveal produced no victims
  where visible shrink did; retained-tree/session discrimination is still needed.
  Fresh admission, failed unfloat and same-output sends did not reach no-space.
  A feasible-tile unfloat reported `unfloat-mismatch` with a one-pixel readback
  diagnostic; no clear production defect is established. Cross-output refusal
  remains not-run.
- This supplements, rather than rewrites, the earlier journey outcomes above.

## Remaining live checks

- Before execution, follow [Windows preflight/recovery](../../live-windows-testing.md):
  exact source/artifact/owner identities, owned-helper-only scope, baseline
  settings/desktop/window preimages, proven stop/restore paths. Do not scope in
  terminals or unrelated apps. Verify foreground identity before every injected
  key; cancel menus by owned-window actions, never global Esc.
- Settings: save/reopen both catalog controls; verify defaults, choices, help,
  layout/accessibility, and no frame/focus/classification change on policy-only
  Apply. Restore exact original settings bytes afterward.
- Fresh exhaustion/seed: create enough owned helpers in one test domain to
  exhaust positive geometric capacity. Verify largest-splittable fallback,
  default capacity float at its observed frame, ordinary float navigation and
  decoration, unchanged birth activation, no extra workspace or repeated writes.
- Recovery/retry: close tiles or grow available space; floats must remain floats.
  Explicit tile must retry once, stay float/no-space while exhausted, and clear
  ordinary float intent only after a successful tile.
- Shrink/gap growth: record successful admission order and last live frames;
  shrink usable space until eviction is required. Verify newest-first shortest
  victim prefix, unchanged victim frames/focus, surviving topology contraction,
  and protected overlay/fixed-override members. Restore bounds/gaps; no auto-tile.
- Hidden domain: shrink or increase gaps while hidden, reveal/select it, and
  verify committed victims acquire ordinary float lanes/markers immediately at
  selection without hidden geometry writes or unrelated focus changes.
- Restart: restart only the verified test owner while owned windows remain;
  verify every capacity/evicted float re-adopts as an ordinary float with the
  same frame and tiled survivors unchanged, even with capacity now available.
- Transfers: capacity-float a tiled mover on an exhausted same-output named
  workspace for follow and stay. Verify source reflow, membership/arrival proof,
  exact mover frame, marker persistence, and truthful refusal without proof.
- Overlap: select allow-overlap, trigger fresh admission and real shrink repair;
  verify positive overflowing targets, no capacity float/reflow on Apply alone,
  tree-relative navigation and preview/drop agreement for drag. Two-output
  follow-up: R4/output capacity-float refusal preserves source and membership.
- Cleanup: restore original policy/gaps/workspace state, close only verified
  owned helpers, stop/restore the exact test owner, and verify no owner/hooks/
  overlays, restored Mouse Snap/session settings, revealed windows and clean
  recovery ledger. Physical keyboard/game/multi-output acceptance is not implied.

- Exact next action: follow the [current discriminators](../windows-best-effort-admission-live.md#blockers-and-exact-next-action).
  Settings save/reopen and live-roster restart no longer need the earlier method
  repair. Retain cross-output refusal; verify it only with a second output.
