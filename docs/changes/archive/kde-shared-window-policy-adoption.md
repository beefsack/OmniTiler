# KDE shared window-policy adoption

## Scope and outcome (2026-10-29)

- Runtime adoption was already delivered by the KDE v2 port. This audit closes
  the classification/admission adoption item, not the whole port or its remaining
  policy deletions. No runtime, protocol, conformance-vector or behavior changes.
- The [approved boundary](../../decisions.md) keeps native acquisition and
  actuation local. As in the [Windows closure](windows-shared-window-policy-adoption.md),
  native B9 clear sequencing, sticky routing and focus fences are not missing
  classification extraction; unused boolean aliases do not require new callers.
- Actual runtime ingress: `kwin/src/world-observe.ts:977-998` captures raw facts;
  `crates/omnitiler/src/world_broker.rs:645-671` normalizes them before strict
  decode, with no projected-verdict fallback. Shared policy runs in Rust, not JS.
  Birth policy runs downstream in `WorldEngine`, not in a broker authorization lane.

## Public-predicate audit

Source abbreviations (line numbers describe the audited source):

- P: `crates/tiler-core/src/window_policy.rs`.
- K: `crates/omnitiler/src/kde_raw.rs`.
- B: `crates/tiler-core/src/world_born.rs`.

"Shared" includes transitive calls and engine execution on KDE facts. "Core"
means core-owned behavior without a call to that particular helper. "Native"
means retained acquisition/actuation, not shared classification ownership.
All 24 public predicates are listed; `EligibilityRefusal::as_str` is a diagnostic
formatter, not a predicate.

| Predicate / declaration | KDE runtime owner and evidence | Remaining KDE-local equivalent |
| --- | --- | --- |
| `safety_eligible` (P:63) | Shared transitively: K:437 -> P:225-235, including full eligibility. K:432-436 maps visible/minimized only. | No second safety classifier; unsupported Windows-style exclusions are not invented. |
| `admission_clear_eligible` (P:78) | No KDE call to this safety alias; birth authorization is core-owned (B:386-439). | None; adding an alias call would not move ownership. |
| `classify_geometry` (P:127) | No KDE call to the ordered refusal API. Shared retained eligibility is K:437,497; core frame authority is `world_publish.rs:614-649,733-750` under `crates/tiler-core/src`. | Live native write guards remain in `kwin/src/world-apply.ts:198-201,1090-1098`; not a second ordered classifier. |
| `classify_focus` (P:172) | No KDE call to this ordered API; effect authorization is core-owned, with native effect guards at `kwin/src/world-apply.ts:2275-2325`. | Those guards are not the overlay-independent safety classifier; fixed-origin admission veto is parked below. |
| `classify_membership` (P:194) | Shared transitively: K:437 -> P:225-232. | None. |
| `classify_retained_membership` (P:221) | Shared directly at K:437-441; hidden/minimized retained, transient floats, unmanaged explicitly omitted. | None. |
| `geometry_eligible` (P:245) | No KDE alias call; shared retained eligibility K:437,497 plus core frame authority as above. | `fitExcluded` at K:493 and `kwin/src/world-observe.ts:1123-1124` is a broader carrier, not this predicate. |
| `focus_actuation_eligible` (P:253) | No KDE alias call; shared retained eligibility and effect guards as above. | No independent full safety classifier. |
| `geometry_write_allowed` (P:262) | Core frame authority at `crates/tiler-core/src/world_publish.rs:619-649,733-750`; no helper call. | Overlay vetoes at `kwin/src/world-apply.ts:1090-1098` remain native setter fences. |
| `overlay_refusal` (P:270) | No KDE call to this diagnostic vocabulary. Overlay facts K:484-493 feed core authority. | Native sticky uses a combined overlay refusal, not this ordered token API (`kwin/src/world-native.ts:761-764`). |
| `float_toggle_refusal` (P:287) | No KDE call; explicit float intent routes to core even under fullscreen (`kwin/src/world-native.ts:648-664,742`). P:280-285 explicitly distinguishes this legacy native refusal from model-only toggles. | No matching refusal: forcing it into this route would change behavior. |
| `sticky_toggle_refusal` (P:300) | Native sticky write remains local; no equivalent core command (`kwin/src/world-entry.ts:4722`; port command-only outcome). | `kwin/src/world-native.ts:749-817`, especially :761-764, retains an overlay fence with combined `sticky-refused-overlay` vocabulary. |
| `orientation_toggle_refusal` (P:313) | No KDE local gate; `kwin/src/world-entry.ts:4713-4717` submits core `toggle-orientation`. | None. |
| `unfloat_maximize_clear_eligible` (P:329) | Native B9 clear/observe/continue sequencing, not a Rust predicate call. | Boolean-equivalent ordinary-float branch at `kwin/src/world-native.ts:684-717`; native setter and scope-bound settlement stay local. |
| `sticky_directional_refusal` (P:342) | No KDE call or drain-time sticky rejection; directionals route to core (port command-only outcome). | None; restoring a local rejection would change the selected command route. |
| `maximized_focus_fence` (P:357) | Core equivalent at `crates/tiler-core/src/session/ops/focus.rs:100-109,466-474`; enabled by default in `world_engine.rs:737-748`. | None in KDE; the inline condition is already in shared core. |
| `should_hold_born_fullscreen` (P:368) | Shared directly at B:348-352 with retained slot/lifetime context. | None. |
| `should_clear_maximize_at_admission` (P:382) | Unused default-mode wrapper; KDE engine uses the typed-mode entry below. | None. |
| `should_clear_maximize_at_admission_with_mode` (P:398) | Shared transitively: B:432 -> P:421-428. | None. |
| `should_clear_maximize_at_admission_with_born_mode` (P:413) | Shared directly at B:432-439; retained lifetime checks/latch surround it. | None. |
| `born_keep_latch` (P:437) | Shared directly at B:425-427. | None. |
| `should_admit_slotless_maximized` (P:448) | No KDE helper call; session/world lifecycle owns admission, e.g. `crates/tiler-core/src/session/world.rs:1576-1625`, with B:338-439 for births. Not a Windows restart adapter. | No KDE slot-admission copy. |
| `should_seed_member_slot` (P:462) | No KDE helper call or adapter-owned slot-seed path; same core session/world owner as above. | None. |
| `classify_born_fullscreen_exit` (P:479) | Shared directly at B:381-384; fixed/suppress and retained lifetime context remain core-owned. | None. |

## Remaining local policy and preserved exceptions

- Shared fixed-size classification is also called directly at K:466-474
  (`tiler_core::size_hints::is_fixed_size_with`, not a public predicate defined
  in P). Client hints classify; frame decoration is carried separately.
- **Known remaining duplicate, unchanged:** `kwin/src/world-observe.ts:23-51,1077-1081`
  still feeds the admission-focus veto and automatic keep-above consumers.
  Its migration is the separate [fixed-size consumer inventory and parked core
  shape](../kde-world-v2-port.md#fixed-size-admission-consumer-inventory-and-proposed-core-shape-2026-10-29).
  Closing this item does not claim that deletion or approve its proposed API.
- **Preserved native equivalents:** the B9 branch and sticky overlay fence in
  the table, plus last-moment setter safety checks, remain local actuation.
  JS cannot call a Rust predicate directly; moving those routes would require
  a separately scoped interface/behavior review, not a mechanical replacement.
- KDE dialog/transient/modal/known-owner maps to neutral transient, not Windows
  owned-dialog/no-activate exclusions (K:415-441). Hidden/minimized retains
  membership while projecting immediate ineligibility (K:494-497). Native
  candidate/lifecycle/readability checks remain acquisition. Richer control facts
  are not required merely to exercise every helper.
- The [adapter guide](../../porting/world-adapter-guide.md#classification-ingress-and-remote-transport-rough-edge)
  and [accepted birth cutover](../kde-world-v2-port.md#n6-core-intents-outcome-and-blocker-2026-10-26)
  describe these boundaries; the port's historical N5/N6 investigations are not
  evidence of a current missing shared call. Native toggles remain documented in
  its [command-only outcome](../kde-world-v2-port.md#stage-4-command-only-outcome-and-remaining-blockers-2026-10-26).

## Verification

- Source audit found no additional in-scope KDE classification/admission copy
  that can be replaced without changing behavior or core/protocol shape.
- Independent source review confirmed predicate coverage, ownership boundaries
  and record links; no findings.
- `devenv shell --impure -- npm test --prefix kwin`: passed, 838 tests,
  zero failed/skipped/cancelled.
- `devenv shell --impure -- npm run typecheck --prefix kwin`: passed.
- `git diff --check`: passed. Rust gates not run: no Rust changes.
- No live testing or new native acceptance claim; no new product decision.
