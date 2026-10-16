# macOS consume-first input parity

## Goal and scope

Match the current Windows rule: an armed owned chord is swallowed before
foreground/action eligibility is evaluated. Fullscreen and game foregrounds do
not cause native pass-through; unsafe actions are refused after consumption.
This is the current rule, not a final cross-platform commitment. Lock and Secure
Input release interception. Unarmed chords retain native delivery.

Preserve exact chord ownership, select-only workspace bindings, tap health-check
and re-enable recovery, and synthetic workspace-chord provenance bypass. Do not
redesign shared core or add managed fullscreen behavior.

## Acceptance

- Consumption does not depend on AX foreground/scope/fullscreen eligibility;
  action execution still checks current eligibility and refuses without effects.
- Fullscreen/game handling follows Windows consume-then-refuse semantics.
- Every remaining host difference is explicit and justified.
- Regression coverage proves consume/refuse separation and preserved release,
  exact ownership, recovery and provenance boundaries.
- macOS tests, strict clippy, fmt, signed app build/verification and hosted CI
  pass after the final relevant change.
- Bounded live evidence is collected where practical; genuinely physical
  acceptance remains an explicit dogfood procedure rather than a synthetic claim.
- Unsafe code stays confined to `ffi.rs`; before/after counts are reported.

## Approach and bounded units

1. Compare Windows hook/dispatch and macOS callback/action routing. Complete:
   Windows interception is foreground-independent; macOS category eligibility
   currently prevents suppression. Windows fullscreen fencing happens afterwards.
2. Move the macOS eligibility boundary after suppression and test the newly
   consumed refusal paths, including select-only workspace routing.
3. Collect bounded live evidence and add practical physical acceptance steps.
4. Independently review safety, input ownership and refusal coverage; resolve
   findings, rerun gates, and verify hosted CI.

## Decisions and evidence

The existing user decision in `decisions.md` (2026-10-16) controls this change.
No new core semantics or fullscreen ownership model is selected.

Investigation baseline: `ffi.rs` has 165 lines matching `unsafe` (including
comments); it is the sole opted-in unsafe module. A stable code-only metric will
also be recorded during implementation.

## Open acceptance

Physical application delivery and fullscreen/game shell behavior cannot be
established by synthetic events or CI. Record only directly observed results.

## Implementation and accepted evidence — 2026-10-17

The callback no longer reads basic/select foreground eligibility; it consumes
exact armed chords while the global trust/privacy gate permits interception.
The input owner triages each consumed down against the appropriate fresh cache
read. Refused hits never leave the production drain as engine input. Ups still
release holds. Category-ineligible states no longer park a disabled tap; existing
verified re-enable/fresh-guard recovery remains category-independent.

Diagnostics are bounded owner-side `consumed-refused-basic/select` summaries
(first/every 50th per category), not callback logging or raw keys. Existing
terminal tap JSON remains unchanged and has no refusal totals. Windows uses
per-intent consumed disposition/outcome/origin records; macOS exposes suppression
counts plus owner refusal summaries, not that Windows-specific record schema.

### Remaining host boundaries

- macOS CGEventTap callbacks cannot perform AX eligibility work: owner-published
  facts and bounded cache freshness replace Windows' native foreground checks.
  Native effect fences still recheck scope, current Space and show state.
- macOS Secure Input, lock and unknown-session trust states release interception;
  these native privacy/security gates are not foreground action eligibility.
- macOS has no managed-fullscreen overlay ownership to exempt from suspension;
  native fullscreen remains untouched. Adding that capability is outside this
  parity change, not a reason to pass an owned chord through.
- Ctrl is macOS' configured default Meta and Compatible leaves known native
  arrow conflicts unarmed. Existing host-native presets/catalogues are unchanged:
  34 Authentic basic arms, 30 Compatible, 18 select-only workspace arms.

There is no remaining intentional foreground-based pass-through difference for
an armed chord. Diagnostic schema/thread placement differ, not consumption policy.

### Gates and review

- 540 macOS library tests and all integration suites pass; strict all-targets
  clippy and workspace fmt check pass.
- Signed `just app 'OmniTiler Mac Dev'` and `just verify` pass.
- Independent safety review found no blockers. Follow-ups removed an obsolete
  category-specific disabled-tap park and added real-channel production-drain
  regression coverage; all gates were rerun afterwards.
- Unsafe-containing lines: **165 → 165**; code-only unsafe-containing lines
  after stripping line comments: **149 → 149**. Unsafe remains confined to
  `ffi.rs`.
- Hosted CI: pending publication.

### Live evidence

Signed LaunchServices bundle `io.github.omnitiler.macos.probe`, identity
`OmniTiler Mac Dev`, certificate leaf
`040f96377aaf110a77905d453bb76681e7133319`. Main executable SHA-256:
`d339e39823cb03c25f3546422f1c1fc55624c61bebe96b5576c6744da39fab1d`;
source was `76c4185f` plus this consume-first change.

Preflight reported AX trust, unlocked session, inactive Secure Input and event
posting access. Two bounded owned-helper synthetic legs completed: exact
suppression 6/6, extra-modifier/plain pairs delivered, and formerly armed pair
delivered after release. Explicit disable produced a real UserInput notification,
verified same-tap re-enable and subsequent suppression in 53 ms. Timeout
notification was not claimed. Lock checks preceded each leg; bounded display
awake children were reaped, with no remaining probe/helper/awake child.
Helper cleanup/CG absence, focus restoration and unchanged Space topology were
read back; settings status had no outstanding preimage. No settings were changed.

The fixture does not exercise product scoped refusal or synthetic provenance
bypass. Physical marker non-deletion, native fullscreen refusal, empty-Space
basic/select distinction and real game behavior remain unobserved, not inferred
from synthetic suppression. The existing dogfood guide now has a
[bounded physical procedure](archive/macos-dogfood-hardening.md#5-consume-first-parity-2026-10-17).

### Decisions and parked questions

The owner-side boundary is recorded as **Tentative (pending user review)** in
`decisions.md`. The final cross-platform input rule remains open by user direction.
No new core contract, managed fullscreen behavior, transfer route or game
detection heuristic is selected. Physical acceptance remains in the macOS backlog.
