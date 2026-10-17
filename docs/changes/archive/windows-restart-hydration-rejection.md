# Windows restart hydration rejection

## Goal

Preserve intentional ordinary and sticky floats from the first observation after restart, including transient observation anomalies. Isolate the `duplicate-window` hydration rejection and prevent its harmful tiled-write fallback without changing valid KDE behavior.

## Scope and non-goals

- Capture the failing candidate facts without user content and isolate the rejection path.
- Fix the responsible layer and cover the failure with regressions.
- Keep valid-input KDE behavior unchanged and existing KDE regression tests passing.
- Do not select new restart layout restoration or post-restart un-stick behavior.

## Acceptance

- Explain the rejection using concrete candidate facts and the responsible code path.
- Repeated live restart trials preserve marked ordinary and sticky floats on the first observation without tiled writes.
- Offline regressions cover the identified mechanism and any narrowly degraded anomaly handling.
- Required locked build/test, formatting, strict all-target clippy, diff checks, and relevant CI streams pass; unrelated known CI failures are reported separately.
- Update the reference discriminator and relevant verification matrix with supported evidence only.

## Approach and bounded units

1. Investigate the recorded failure and current admission path.
2. Add bounded normal-level diagnostics and an offline reproduction before changing behavior.
3. Reproduce live with scoped, owned fixtures and restore changed state.
4. Apply the smallest supported fix, review Windows safety and KDE valid-input compatibility, then verify offline and live.
5. Record the supported outcome and archive this note after acceptance.

## Root cause and implementation

- A fixed-size first-tiled anchor is converted to an automatic-float exception during fresh floating-aware convergence. The retained session can have no tiled focus despite a tiled remainder. `fresh_admit_shared`'s fixed-anchor branch requires a tiled focus, falls through when it is absent, and legacy seeding re-admits the already-present anchor as `duplicate-window`.
- Windows discarded the candidate and rolled back staged float lanes, exposing valid marked floats to the same observation's ordinary tiled planning. Later adoption restored intent, not displaced frames.
- The fix is Windows-only: `restart_reject_hold` records the exact validated staged lifetime keys on failed hydration. Row assembly carries these as floats and the writable gate excludes them. The hold is cleared and rebuilt on every observation, pruned on membership/lifetime loss, and absent after successful hydration or invalid markers. Rejected candidates never commit; durable markers and recovery authority are unchanged.
- Normal-level diagnostics retain at most eight anonymous candidate facts, exact total/omitted counts, focus, hints, geometry, pre-session revision, and admission reports. Eight covers the recorded small candidate with headroom while bounding normal-log expansion. No titles, paths, process identities, or user content are logged.

## Accepted evidence (2026-10-17)

- Seven new regressions: two shared-core mechanism/control characterizations, two bounded-diagnostic tests, and three Windows real-marker/real-Engine cases for repeated rejection, later successful adoption, invalid markers, and replaced lifetimes. Production core is unchanged.
- Captured mixed candidate: five windows, three hintless floats, first-tiled anchor `w25` with equal minimum/maximum `[1157,789]`, tiled remainder `w26` with minimum `[401,246]`, no pre-existing session. Reports show `fit_excluded`, fixed admission `fixed-equal`, convergence admitting one tiled window and four exceptions, then `duplicate-window`. All five candidate facts were retained, with no omissions.
- Eighteen semantic restart trials: six ordinary diagnostic-only passes; four fixed-anchor/all-float-remainder baseline rejections; two mixed baseline rejections; six protected mixed rejections. All six protected trials preserved their starting frames with zero tiled writes and adopted intent on the next observation. Four protected trials began with centered floats.
- Causal mixed A/B: the first baseline restart tiled all three intentional floats on tick 1, then adopted on tick 2 without frame repair. The second baseline trial began already displaced and made no further float writes. Both protected A/B trials preserved those starting frames with zero writes. The protection does not repair prior displacement; centered-frame protection is separately covered by the four earlier protected trials.
- Artifact SHA256: diagnostic-only baseline `B38D3A5F17BEB820B2DAD452ADAF3DFA8F2AB84DC8262AD2DE216737513191DF`; protected `E3FD1AF475F514F90EE66E31B0F76209D789B87BDF29931D2B700BDDEFEC46C8`. Both were built from `7511ae4c` with the documented diagnostic/test and protection deltas respectively. Logs and complete candidate facts were retained outside the repository.
- Locked four-package build/test passed before publication and again after rebasing: 2,465 passed, zero failed, one pre-existing ignored benchmark. Workspace formatting, strict four-package all-target clippy, and diff checks passed.
- Delivery commit `e6e2ba86`: [CI 38975584726](https://github.com/beefsack/OmniTiler/actions/runs/38975584726) passed Windows (3,170 tests, one ignored; includes the additional macOS package), Rust workspace (2,780 tests), KWin (1,703 tests and typecheck), macOS, native effect/settings (33/26 tests), licenses, and notices. Only the known unrelated shell `kwin-66-api-guard` failed (`PASS=6 FAIL=2`, clean-tree/clean-marker for unaudited `fullscreen`, `maximized`, `sticky` symbols). No other stream failed; that guard is not changed here.
- Independent review accepted the lifetime fencing, observation ordering, tiled-write coverage, rollback, diagnostic bounds, and KDE compatibility. An explicit unfloat during a rejection hold can delay its native reflow until the next observation; marker clearing still succeeds and recovery is automatic.
- All positively-owned live fixtures, including previous residue, were closed after identity checks. Settings and changed state were restored; the controlling terminal and non-owned windows were unchanged. Earlier historical cleanup limitations are not retroactively resolved.

## Remaining boundaries

- The exact historical candidate was not retained, so equivalence to that original run remains qualified despite reproduction of the same rejection, matching control minimums, and harmful write sequence.
- The shared-core rejection remains characterized, not repaired. Completing the no-focus fixed-anchor branch would change valid KDE input behavior and requires a separate decision and KDE regression coverage. Recommended follow-up: repair that branch only after selecting its accepted no-focus reply/projection contract.
- Full D25 membership/set/focus acceptance, post-restart un-stick, saved-layout restoration, and unsupported reference outcomes remain outside this repair.

## Outcome

- Harmful first-observation tiled writes to validated restart floats are prevented at the Windows observation/write boundary, with repeated live verification and offline regressions.
- The underlying shared-core no-focus rejection remains a separate reply/projection choice; completing that guard is recommended only after selecting its valid-input KDE contract.

## Decisions and boundaries

- Diagnostics must exclude titles and other user content, explain any bounds, and preserve behavior.
- A fix that changes KDE behavior for valid input requires a product decision before implementation.
- Unsupported reference outcomes remain TBD.
- Do not repair shared fresh admission here: changing its reply for this valid fixed-size input would also change KDE behavior. Preserve its characterization and report that separate repair boundary.
- Apply Windows-only, observation-scoped write protection for the marker-validated candidates whose hydration fails. Preserve their pre-observation frames without committing a rejected Engine candidate. Revalidate normally on the next observation; do not introduce durable float state or new un-stick semantics. This implements narrow degradation for already-established intent, not a new restart-layout policy.

## Second agent-live regression (2026-10-17)

- An ordinary float and a sticky float retained identical frames and process
  lifetimes across orderly restart; sticky visibility across workspace switches
  was unchanged. Without a fixed anchor, first observation adopted both without
  hydration rejection.
- A separate run used a newly owned tiler Settings window with positively
  observed equal minimum/maximum tracks. First observation reproduced the
  four-candidate `duplicate-window` rejection, including `fixed-equal` anchor
  admission. Protected ordinary/sticky/anchor frames remained exact; both
  marked floats adopted on the next observation. The fixed anchor also retained
  its frame through workspace hide/reveal. No shared-core repair was made.
- Settings/SPI were restored, ledger/request/overlays absent and all proven
  owned disposable windows closed. The shared no-focus reply/projection
  decision, full restart matrix and logout/session-expiry remain outside scope.
